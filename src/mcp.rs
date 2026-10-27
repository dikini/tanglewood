//! Linux MCP adapter: stdio is rmcp; Unix sockets only forward shared commands.
use crate::{
    App,
    commands::{Caller, Command, CommandError, CommandResult},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

type Wake = Arc<std::sync::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;

pub const PROTOCOL_VERSION: u16 = 1;
const REQUEST_LIMIT: usize = 1024 * 1024;
const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;
const CAPACITY: usize = 16;
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessPolicy {
    #[default]
    ReadOnly,
    ReadWrite,
}
impl AccessPolicy {
    fn caller(self) -> Caller {
        match self {
            Self::ReadOnly => Caller::AutomationReadOnly,
            Self::ReadWrite => Caller::AutomationReadWrite,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    pub version: u16,
    pub request_id: Uuid,
    /// None asks for capabilities; caller identity is never a payload field.
    pub command: Option<Command>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    pub version: u16,
    pub request_id: Uuid,
    pub outcome: Outcome,
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Outcome {
    Capabilities {
        commands: Vec<String>,
        policy: AccessPolicy,
    },
    Result(CommandResult),
    Error(CommandError),
    Rejected(String),
    /// A mutation may have been accepted. Inspect state; never blindly retry.
    UnknownOutcome(String),
}

struct Pending {
    request: RequestEnvelope,
    deadline: Instant,
    reply: SyncSender<ResponseEnvelope>,
}
struct Waiting {
    pending: Pending,
    result: CommandResult,
}

/// Owned by the same thread as App. Socket workers hold only data and channels.
pub struct OwnerSocket {
    path: PathBuf,
    identity: (u64, u64),
    requests: Receiver<Pending>,
    waiting: Vec<Waiting>,
    stop: Arc<AtomicBool>,
    listener: Option<thread::JoinHandle<()>>,
    policy: AccessPolicy,
    wake: Wake,
}

impl OwnerSocket {
    /// Parent must already be a real, current-user-only directory (mode0700).
    /// Existing socket paths are refused, including stale paths.
    pub fn bind(path: &Path, policy: AccessPolicy) -> io::Result<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| invalid("socket needs a private parent directory"))?;
        let metadata = fs::symlink_metadata(parent)?;
        // SAFETY: geteuid has no pointer arguments or preconditions.
        let uid = unsafe { libc::geteuid() };
        if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "socket parent must be owned by current user with mode0700",
            ));
        }
        if fs::symlink_metadata(path).is_ok() {
            return Err(invalid(
                "socket path already exists; refusing to replace it",
            ));
        }
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        let metadata = fs::symlink_metadata(path)?;
        let identity = (metadata.dev(), metadata.ino());
        listener.set_nonblocking(true)?;
        let (send, requests) = mpsc::sync_channel(CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let active = Arc::new(AtomicUsize::new(0));
        let wake: Wake = Arc::new(std::sync::Mutex::new(None));
        let listener_wake = wake.clone();
        let handle = thread::spawn(move || {
            while !stop_thread.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if !same_uid(&stream, uid) || active.load(Ordering::Acquire) >= CAPACITY {
                            continue;
                        }
                        active.fetch_add(1, Ordering::AcqRel);
                        let active = active.clone();
                        let send = send.clone();
                        let wake = listener_wake.clone();
                        thread::spawn(move || {
                            serve_connection(stream, send, wake);
                            active.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            path: path.to_owned(),
            identity,
            requests,
            waiting: Vec::new(),
            stop,
            listener: Some(handle),
            policy,
            wake,
        })
    }

    /// The saved vault preference overrides the startup policy immediately.
    /// A missing preference preserves the explicit `--mcp-write` fallback.
    pub fn policy(&self, app: &App) -> AccessPolicy {
        match app.automation_write_preference() {
            Some(true) => AccessPolicy::ReadWrite,
            Some(false) => AccessPolicy::ReadOnly,
            None => self.policy,
        }
    }

    /// Bounded owner-thread dispatch and acknowledgement polling; never waits for I/O.
    pub fn poll(&mut self, app: &mut App) {
        app.tick();
        let mut retained = Vec::new();
        for waiting in self.waiting.drain(..) {
            let outcome = match &waiting.result {
                CommandResult::Note(accepted) => match app.note(accepted.note.id) {
                    Ok(current) if current.save.persisted == Some(accepted.note.revision) => {
                        // Confirm the requested snapshot, even if a newer edit is now dirty.
                        let mut confirmed = accepted.clone();
                        confirmed.save.persisted = Some(accepted.note.revision);
                        confirmed.save.error = None;
                        Some(Outcome::Result(CommandResult::Note(confirmed)))
                    }
                    Ok(current) if current.save.error.is_some() => {
                        Some(Outcome::UnknownOutcome(format!(
                            "accepted but storage failed: {}",
                            current.save.error.unwrap_or_default()
                        )))
                    }
                    Ok(current) if current.note.revision != accepted.note.revision => {
                        Some(Outcome::UnknownOutcome(
                            "accepted revision was superseded before confirmation; re-read state"
                                .into(),
                        ))
                    }
                    Err(_) => Some(Outcome::UnknownOutcome(
                        "accepted note no longer available".into(),
                    )),
                    _ => None,
                },
                CommandResult::Accepted { request_id } => match app.io_status(*request_id) {
                    Some(crate::app::IoStatus::Completed) => {
                        Some(Outcome::Result(waiting.result.clone()))
                    }
                    Some(crate::app::IoStatus::Failed(error)) => Some(Outcome::UnknownOutcome(
                        format!("accepted but storage failed: {error}; inspect state before retry"),
                    )),
                    Some(crate::app::IoStatus::Conflict(id)) => Some(Outcome::UnknownOutcome(
                        format!("accepted request has conflict {id}; inspect state before retry"),
                    )),
                    Some(crate::app::IoStatus::Pending) | None => None,
                },
                _ => Some(Outcome::UnknownOutcome(
                    "unsupported mutation acknowledgement".into(),
                )),
            };
            if let Some(outcome) = outcome {
                reply(&waiting.pending, outcome);
            } else if Instant::now() >= waiting.pending.deadline {
                reply(
                    &waiting.pending,
                    Outcome::UnknownOutcome(
                        "persistence confirmation timed out; inspect state before retry".into(),
                    ),
                );
            } else {
                retained.push(waiting);
            }
        }
        self.waiting = retained;
        for _ in 0..CAPACITY {
            if self.waiting.len() >= CAPACITY {
                break;
            }
            let Ok(pending) = self.requests.try_recv() else {
                break;
            };
            if Instant::now() >= pending.deadline {
                reply(
                    &pending,
                    Outcome::Rejected("request expired before dispatch".into()),
                );
                continue;
            }
            if pending.request.version != PROTOCOL_VERSION {
                reply(
                    &pending,
                    Outcome::Rejected("unsupported protocol version".into()),
                );
                continue;
            }
            let policy = self.policy(app);
            let Some(command) = pending.request.command.clone() else {
                reply(
                    &pending,
                    Outcome::Capabilities {
                        commands: allowed_variants(policy)
                            .iter()
                            .map(|(_, name)| (*name).to_owned())
                            .collect(),
                        policy,
                    },
                );
                continue;
            };
            if !allowed_variants(policy)
                .iter()
                .any(|(_, name)| *name == command.descriptor().name)
            {
                reply(&pending, Outcome::Error(CommandError::Unauthorized));
                continue;
            }
            let mutation = command.is_mutation();
            match app.execute(command, policy.caller()) {
                Ok(result) if mutation => self.waiting.push(Waiting { pending, result }),
                Ok(result) => reply(&pending, Outcome::Result(result)),
                Err(error) => reply(&pending, Outcome::Error(error)),
            }
        }
        app.tick();
    }

    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(Arc::new(wake));
        }
    }

    pub fn has_pending(&self) -> bool {
        !self.waiting.is_empty()
    }
}

impl Drop for OwnerSocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.listener.take() {
            let _ = handle.join();
        }
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| (metadata.dev(), metadata.ino()) == self.identity)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn allowed_variants(policy: AccessPolicy) -> Vec<(&'static str, &'static str)> {
    Command::automation_variants(policy.caller())
}
fn reply(pending: &Pending, outcome: Outcome) {
    let _ = pending.reply.try_send(ResponseEnvelope {
        version: PROTOCOL_VERSION,
        request_id: pending.request.request_id,
        outcome,
    });
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn same_uid(stream: &UnixStream, uid: u32) -> bool {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: correctly sized writable ucred and socklen_t; stream fd is live.
    let status = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut credentials as *mut _ as *mut libc::c_void,
            &mut length,
        )
    };
    status == 0 && length as usize == std::mem::size_of::<libc::ucred>() && credentials.uid == uid
}
fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "transport deadline expired; mutation outcome may be unknown",
            )
        })
}
fn read_exact_deadline(
    stream: &mut UnixStream,
    mut bytes: &mut [u8],
    deadline: Instant,
) -> io::Result<()> {
    while !bytes.is_empty() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        let count = stream.read(bytes)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "application disconnected",
            ));
        }
        bytes = &mut bytes[count..];
    }
    Ok(())
}
fn read_frame(stream: &mut UnixStream, limit: usize, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut header = [0; 4];
    read_exact_deadline(stream, &mut header, deadline)?;
    let length = u32::from_be_bytes(header) as usize;
    if length > limit {
        return Err(invalid("message exceeds configured byte limit"));
    }
    let mut bytes = vec![0; length];
    read_exact_deadline(stream, &mut bytes, deadline)?;
    Ok(bytes)
}
fn write_frame(
    stream: &mut UnixStream,
    bytes: &[u8],
    limit: usize,
    deadline: Instant,
) -> io::Result<()> {
    if bytes.len() > limit {
        return Err(invalid("message exceeds configured byte limit"));
    }
    let mut framed = (bytes.len() as u32).to_be_bytes().to_vec();
    framed.extend_from_slice(bytes);
    let mut bytes = framed.as_slice();
    while !bytes.is_empty() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        let count = stream.write(bytes)?;
        if count == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "socket closed"));
        }
        bytes = &bytes[count..];
    }
    Ok(())
}
fn serve_connection(mut stream: UnixStream, queue: SyncSender<Pending>, wake: Wake) {
    let deadline = Instant::now() + TIMEOUT;
    let Ok(bytes) = read_frame(&mut stream, REQUEST_LIMIT, deadline) else {
        return;
    };
    let Ok(request) = serde_json::from_slice::<RequestEnvelope>(&bytes) else {
        return;
    };
    let (send, receive) = mpsc::sync_channel(1);
    let id = request.request_id;
    let pending = Pending {
        request,
        deadline,
        reply: send,
    };
    let outcome = match queue.try_send(pending) {
        Ok(()) => {
            if let Ok(slot) = wake.lock() {
                if let Some(wake) = slot.as_ref() {
                    wake();
                }
            }
            match remaining(deadline)
                .and_then(|timeout| receive.recv_timeout(timeout).map_err(io::Error::other))
            {
                Ok(response) => response,
                Err(_) => ResponseEnvelope {
                    version: PROTOCOL_VERSION,
                    request_id: id,
                    outcome: Outcome::UnknownOutcome(
                        "application did not confirm outcome before deadline; do not blindly retry"
                            .into(),
                    ),
                },
            }
        }
        Err(_) => ResponseEnvelope {
            version: PROTOCOL_VERSION,
            request_id: id,
            outcome: Outcome::Rejected("application request queue is full or unavailable".into()),
        },
    };
    let bytes = encode_limited(&outcome, RESPONSE_LIMIT).or_else(|_| {
        encode_limited(
            &ResponseEnvelope {
                version: PROTOCOL_VERSION,
                request_id: id,
                outcome: Outcome::UnknownOutcome(
                    "result exceeded response byte limit; mutation may already be persisted".into(),
                ),
            },
            RESPONSE_LIMIT,
        )
    });
    if let Ok(bytes) = bytes {
        let _ = write_frame(
            &mut stream,
            &bytes,
            RESPONSE_LIMIT,
            Instant::now() + Duration::from_secs(1),
        );
    }
}

fn encode_limited(value: &impl Serialize, limit: usize) -> io::Result<Vec<u8>> {
    struct Buffer {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(invalid("message exceeds byte limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut buffer, value).map_err(io::Error::other)?;
    Ok(buffer.bytes)
}

fn connect_deadline(path: &Path) -> io::Result<UnixStream> {
    use std::os::{fd::FromRawFd, unix::ffi::OsStrExt};
    let path = path.as_os_str().as_bytes();
    // SAFETY: zero is a valid starting representation for sockaddr_un.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if path.len() >= address.sun_path.len() || path.contains(&0) {
        return Err(invalid("invalid Unix socket path"));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (destination, byte) in address.sun_path.iter_mut().zip(path) {
        *destination = *byte as libc::c_char;
    }
    // SAFETY: socket takes scalar constants; a successful fd is owned below.
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is newly created, unique and valid.
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    // SAFETY: address points to a fully initialized sockaddr_un with valid length.
    let status = unsafe {
        libc::connect(
            fd,
            &address as *const _ as *const libc::sockaddr,
            (std::mem::size_of::<libc::sa_family_t>() + path.len() + 1) as libc::socklen_t,
        )
    };
    if status != 0 {
        let error = io::Error::last_os_error();
        if !matches!(
            error.raw_os_error(),
            Some(libc::EINPROGRESS) | Some(libc::EAGAIN)
        ) {
            return Err(error);
        }
        let mut descriptor = libc::pollfd {
            fd,
            events: libc::POLLOUT,
            revents: 0,
        };
        // SAFETY: descriptor is one valid writable pollfd.
        let ready = unsafe { libc::poll(&mut descriptor, 1, TIMEOUT.as_millis() as libc::c_int) };
        if ready < 0 {
            return Err(io::Error::last_os_error());
        }
        if ready == 0 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "socket connect timed out",
            ));
        }
        if let Some(error) = stream.take_error()? {
            return Err(error);
        }
    }
    stream.set_nonblocking(false)?;
    Ok(stream)
}

/// No vault opening or retry: a missing owner is an explicit transport error.
pub fn forward(path: &Path, request: &RequestEnvelope) -> io::Result<ResponseEnvelope> {
    let bytes = encode_limited(request, REQUEST_LIMIT)?;
    if bytes.len() > REQUEST_LIMIT {
        return Err(invalid("request exceeds 1MiB"));
    }
    let mut stream = connect_deadline(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("running Tanglewood application unavailable: {error}"),
        )
    })?;
    // Authenticate the owner as our own UID before sending note contents.
    let uid = unsafe { libc::geteuid() };
    if !same_uid(&stream, uid) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "socket owner UID mismatch",
        ));
    }
    let deadline = Instant::now() + TIMEOUT;
    write_frame(&mut stream, &bytes, REQUEST_LIMIT, deadline)?;
    let bytes = read_frame(&mut stream, RESPONSE_LIMIT, deadline)?;
    let response: ResponseEnvelope = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if response.version != PROTOCOL_VERSION || response.request_id != request.request_id {
        return Err(invalid("socket response protocol/id mismatch"));
    }
    Ok(response)
}

use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    service::RequestContext,
};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExecuteRequest {
    command: Command,
}

/// MCP service contains a socket path only; it can never open a vault.
#[derive(Clone)]
pub struct McpBridge {
    socket: PathBuf,
    active: Arc<AtomicUsize>,
}
struct BridgePermit(Arc<AtomicUsize>);
impl Drop for BridgePermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
impl McpBridge {
    pub fn new(socket: PathBuf) -> Self {
        Self {
            socket,
            active: Arc::new(AtomicUsize::new(0)),
        }
    }
    async fn request(&self, command: Option<Command>) -> io::Result<ResponseEnvelope> {
        self.active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < CAPACITY).then_some(count + 1)
            })
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "MCP bridge has 16 active requests",
                )
            })?;
        let permit = BridgePermit(self.active.clone());
        let path = self.socket.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            forward(
                &path,
                &RequestEnvelope {
                    version: PROTOCOL_VERSION,
                    request_id: Uuid::new_v4(),
                    command,
                },
            )
        })
        .await
        .map_err(io::Error::other)?
    }
    async fn capabilities(&self) -> io::Result<ResponseEnvelope> {
        self.request(None).await
    }
    async fn read(&self, command: Command) -> io::Result<ResponseEnvelope> {
        if !command.descriptor().allowed(Caller::AutomationReadOnly) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read only accepts authorized non-mutating commands",
            ));
        }
        self.request(Some(command)).await
    }
    async fn execute(&self, command: Command) -> io::Result<ResponseEnvelope> {
        self.request(Some(command)).await
    }
}

fn execute_schema(policy: AccessPolicy) -> serde_json::Map<String, serde_json::Value> {
    let mut schema = serde_json::to_value(schemars::schema_for!(ExecuteRequest))
        .expect("generated schema serializes");
    let allowed = allowed_variants(policy);
    // Preserve the schema emitted from shared types; remove unauthorized enum alternatives.
    if let Some(variants) = schema
        .pointer_mut("/$defs/Command/oneOf")
        .and_then(serde_json::Value::as_array_mut)
    {
        variants.retain_mut(|variant| {
            // Schemars groups unit variants into a string enum, while variants
            // carrying fields use object properties. Filter both representations.
            if let Some(units) = variant
                .get_mut("enum")
                .and_then(serde_json::Value::as_array_mut)
            {
                units.retain(|unit| {
                    unit.as_str()
                        .is_some_and(|wire| allowed.iter().any(|(name, _)| *name == wire))
                });
                return !units.is_empty();
            }
            variant
                .get("properties")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|properties| {
                    properties
                        .keys()
                        .any(|wire| allowed.iter().any(|(name, _)| name == wire))
                })
        });
    } else {
        // Fail closed if the generator changes its enum layout.
        return serde_json::json!({"type":"object","not":{}})
            .as_object()
            .unwrap()
            .clone();
    }
    schema.as_object().expect("schema root is object").clone()
}

fn tool_annotations(read_only: bool) -> ToolAnnotations {
    ToolAnnotations::from_raw(
        None,
        Some(read_only),
        Some(!read_only),
        Some(read_only),
        Some(false),
    )
}

// Codex can cache tools/list for the lifetime of a thread. Keep execute's shape
// and mutation annotations stable; discovery does not grant authority. The owner
// evaluates its current local policy when each command is actually dispatched.
fn bridge_tools(_policy: AccessPolicy) -> Vec<Tool> {
    vec![
        Tool::new("read", "Query vault notes and research workspaces, sources, indexes, and archived artifacts without modifying them. Use Research List and ReadIndex to discover current research state, and ReadSource for bounded page-numbered paper text.", execute_schema(AccessPolicy::ReadOnly))
            .with_annotations(tool_annotations(true)),
        Tool::new("execute", "Dispatch shared revision-checked note and research commands. This stable schema includes mutations, but grants no write permission: the owner checks its current local policy on every call. Use capabilities for current authorization and read for queries. For an existing source use Research SavePaper with a direct PDF URL to retain original bytes and extract text; then read via ReadSource. DOI is optional. Inspect paper.extraction_error before claiming full-text success. Local-file inputs require local authorization. Mutation success requires durable confirmation.", execute_schema(AccessPolicy::ReadWrite))
            .with_annotations(tool_annotations(false)),
        Tool::new("capabilities", "List commands allowed by the running app's local MCP policy.", serde_json::json!({"type":"object","properties":{},"additionalProperties":false}).as_object().unwrap().clone())
            .with_annotations(tool_annotations(true)),
    ]
}

impl ServerHandler for McpBridge {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("Read vault notes and research workspaces and dispatch authorized shared Tanglewood commands in the existing running app. Read capabilities for current authorization, then use read for queries and execute for authorized mutations. The execute schema is stable across permission changes; exposed commands do not grant writes. The owner enforces current local permission on every call. A timeout or UnknownOutcome may follow a committed mutation; re-read state and never blindly retry.".to_owned())
    }
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let response = self
            .capabilities()
            .await
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        let Outcome::Capabilities { policy, .. } = response.outcome else {
            return Err(ErrorData::internal_error(
                "invalid capability response",
                None,
            ));
        };
        Ok(ListToolsResult {
            tools: bridge_tools(policy),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let response = match request.name.as_ref() {
            "execute" | "read" => {
                let parsed = serde_json::from_value::<ExecuteRequest>(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ));
                match parsed {
                    Ok(parsed) => {
                        if request.name.as_ref() == "read" {
                            self.read(parsed.command).await
                        } else {
                            self.execute(parsed.command).await
                        }
                    }
                    Err(error) => {
                        return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                            "invalid command arguments: {error}"
                        ))])
                        .into());
                    }
                }
            }
            "capabilities" => self.capabilities().await,
            _ => return Err(ErrorData::invalid_params("unknown tool", None)),
        };
        let result = match response {
            Ok(response) => {
                let failed = !matches!(
                    response.outcome,
                    Outcome::Result(_) | Outcome::Capabilities { .. }
                );
                let content = vec![ContentBlock::text(
                    serde_json::to_string(&response)
                        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?,
                )];
                if failed {
                    CallToolResult::error(content)
                } else {
                    CallToolResult::success(content)
                }
            }
            Err(error) => {
                let message = if request.name.as_ref() == "read"
                    || request.name.as_ref() == "capabilities"
                {
                    format!("read request failed: {error}")
                } else {
                    format!(
                        "application transport failed; mutation outcome may be unknown, do not blindly retry: {error}"
                    )
                };
                CallToolResult::error(vec![ContentBlock::text(message)])
            }
        };
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn round_trip(
        owner: &mut OwnerSocket,
        app: &mut App,
        path: &Path,
        command: Option<Command>,
    ) -> Outcome {
        let path = path.to_owned();
        let (send, receive) = mpsc::channel();
        thread::spawn(move || {
            send.send(forward(
                &path,
                &RequestEnvelope {
                    version: PROTOCOL_VERSION,
                    request_id: Uuid::new_v4(),
                    command,
                },
            ))
            .unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            owner.poll(app);
            if let Ok(response) = receive.try_recv() {
                return response.unwrap().outcome;
            }
            assert!(Instant::now() < deadline, "socket request did not complete");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn read_tool_is_annotated_and_rejects_forged_mutations_before_forwarding() {
        for policy in [AccessPolicy::ReadOnly, AccessPolicy::ReadWrite] {
            let tools = bridge_tools(policy);
            for tool in &tools {
                let metadata = serde_json::to_value(tool).unwrap();
                let read_only = tool.name != "execute";
                assert_eq!(metadata["annotations"]["readOnlyHint"], read_only);
                assert_eq!(metadata["annotations"]["destructiveHint"], !read_only);
                assert_eq!(metadata["annotations"]["idempotentHint"], read_only);
                assert_eq!(metadata["annotations"]["openWorldHint"], false);
            }
            let read = &tools[0];
            let schema_value = serde_json::Value::Object((*read.input_schema).clone());
            let schema = schema_value
                .pointer("/$defs/Command/oneOf")
                .unwrap()
                .to_string();
            assert!(schema.contains("ReadNote"));
            assert!(schema.contains("SearchNotes"));
            assert!(!schema.contains("CreateNote"));
            assert!(!schema.contains("ExportMarkdown"));
        }
        // No socket exists: PermissionDenied proves rejection happened before transport.
        let bridge = McpBridge::new(PathBuf::from("/nonexistent/tanglewood-read-test.sock"));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for command in [
            Command::CreateNote {
                path: "forged.md".into(),
                text: "no".into(),
            },
            Command::ExportMarkdown {
                id: crate::notes::NoteId(Uuid::new_v4()),
                path: "/tmp/forged-export".into(),
            },
        ] {
            let error = runtime.block_on(bridge.read(command)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        }
    }

    #[test]
    fn cached_execute_schema_stays_usable_across_policy_changes() {
        let cached = bridge_tools(AccessPolicy::ReadOnly).remove(1);
        let granted = bridge_tools(AccessPolicy::ReadWrite).remove(1);
        assert_eq!(cached.input_schema, granted.input_schema);
        let schema = serde_json::Value::Object((*cached.input_schema).clone()).to_string();
        assert!(schema.contains("CreateNote"));
        assert!(schema.contains("Research"));
        assert!(!schema.contains("SetAutomationWriteEnabled"));
        assert!(!schema.contains("SetCodexProject"));
        assert_eq!(
            serde_json::to_value(cached).unwrap()["annotations"]["readOnlyHint"],
            false
        );
    }

    #[test]
    fn same_socket_grants_and_revokes_before_queued_dispatch() {
        let directory = std::env::temp_dir().join(format!("tw-mcp-live-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join("app.sock");
        let mut app = App::open(&directory.join("vault")).unwrap();
        let mut owner = OwnerSocket::bind(&path, AccessPolicy::ReadOnly).unwrap();
        let create = Command::CreateNote {
            path: "live.md".into(),
            text: "accepted".into(),
        };
        assert!(matches!(
            round_trip(&mut owner, &mut app, &path, Some(create.clone())),
            Outcome::Error(CommandError::Unauthorized)
        ));
        app.execute(
            Command::SetAutomationWriteEnabled { enabled: true },
            Caller::Local,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.automation_write_preference() != Some(true) {
            app.tick();
            assert!(Instant::now() < deadline, "grant was not persisted");
            thread::sleep(Duration::from_millis(2));
        }
        assert!(matches!(
            round_trip(&mut owner, &mut app, &path, None),
            Outcome::Capabilities {
                policy: AccessPolicy::ReadWrite,
                ..
            }
        ));
        assert!(matches!(
            round_trip(
                &mut owner,
                &mut app,
                &path,
                Some(Command::SetAutomationWriteEnabled { enabled: true })
            ),
            Outcome::Error(CommandError::Unauthorized)
        ));
        // Dispatch once so accepted durable work is awaiting acknowledgement.
        let (accepted_send, requests) = mpsc::sync_channel(1);
        owner.requests = requests;
        let (reply, accepted_response) = mpsc::sync_channel(1);
        accepted_send
            .send(Pending {
                request: RequestEnvelope {
                    version: PROTOCOL_VERSION,
                    request_id: Uuid::new_v4(),
                    command: Some(create),
                },
                deadline: Instant::now() + TIMEOUT,
                reply,
            })
            .unwrap();
        owner.poll(&mut app);
        assert_eq!(owner.waiting.len(), 1);
        // Queue a mutation without dispatching, then revoke locally.
        let (send, requests) = mpsc::sync_channel(1);
        owner.requests = requests;
        let (reply, response) = mpsc::sync_channel(1);
        send.send(Pending {
            request: RequestEnvelope {
                version: PROTOCOL_VERSION,
                request_id: Uuid::new_v4(),
                command: Some(Command::CreateNote {
                    path: "denied.md".into(),
                    text: "no".into(),
                }),
            },
            deadline: Instant::now() + TIMEOUT,
            reply,
        })
        .unwrap();
        app.execute(
            Command::SetAutomationWriteEnabled { enabled: false },
            Caller::Local,
        )
        .unwrap();
        owner.poll(&mut app);
        assert!(matches!(
            response.try_recv().unwrap().outcome,
            Outcome::Error(CommandError::Unauthorized)
        ));
        assert_eq!(app.notes().len(), 1);
        assert_eq!(owner.policy(&app), AccessPolicy::ReadOnly);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            owner.poll(&mut app);
            if let Ok(response) = accepted_response.try_recv() {
                assert!(matches!(
                    response.outcome,
                    Outcome::Result(CommandResult::Note(_))
                ));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "accepted mutation did not complete after revoke"
            );
            thread::sleep(Duration::from_millis(2));
        }
        drop(owner);
        app.shutdown().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn simultaneous_clients_preserve_revisions_and_survive_other_disconnects() {
        let directory = std::env::temp_dir().join(format!("tw-mcp-clients-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join("app.sock");
        let mut app = App::open(&directory.join("vault")).unwrap();
        let mut owner = OwnerSocket::bind(&path, AccessPolicy::ReadWrite).unwrap();
        let create = |path: &str, text: &str| Command::CreateNote {
            path: path.into(),
            text: text.into(),
        };
        let Outcome::Result(CommandResult::Note(first)) =
            round_trip(&mut owner, &mut app, &path, Some(create("a.md", "one")))
        else {
            panic!("first create failed")
        };
        let Outcome::Result(CommandResult::Note(second)) =
            round_trip(&mut owner, &mut app, &path, Some(create("b.md", "two")))
        else {
            panic!("second create failed")
        };
        let (send, receive) = mpsc::channel();
        let mut clients = Vec::new();
        let mut joins = Vec::new();
        for client in 0..2 {
            let socket = path.clone();
            let send = send.clone();
            let (commands, incoming) = mpsc::channel::<Command>();
            clients.push(commands);
            joins.push(thread::spawn(move || {
                let bridge = McpBridge::new(socket);
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                while let Ok(command) = incoming.recv() {
                    let result = runtime.block_on(async {
                        if command.is_mutation() {
                            bridge.execute(command).await
                        } else {
                            bridge.read(command).await
                        }
                    });
                    send.send((client, result.unwrap())).unwrap();
                }
            }));
        }
        let collect = |owner: &mut OwnerSocket, app: &mut App, count| {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut results = Vec::new();
            while results.len() < count {
                owner.poll(app);
                while let Ok(response) = receive.try_recv() {
                    results.push(response);
                }
                assert!(
                    Instant::now() < deadline,
                    "client requests did not complete"
                );
                thread::sleep(Duration::from_millis(2));
            }
            results.sort_by_key(|(client, _)| *client);
            results
        };
        clients[0]
            .send(Command::ReadNote { id: first.note.id })
            .unwrap();
        clients[1]
            .send(Command::ReadNote { id: second.note.id })
            .unwrap();
        let reads = collect(&mut owner, &mut app, 2);
        assert!(
            matches!(&reads[0].1.outcome, Outcome::Result(CommandResult::Note(note)) if note.note.text == "one")
        );
        assert!(
            matches!(&reads[1].1.outcome, Outcome::Result(CommandResult::Note(note)) if note.note.text == "two")
        );
        // Two independent transports may reuse the same request ID; the owner must
        // return each response over its own reply channel, never by global ID.
        let shared_id = Uuid::new_v4();
        let (collision_send, collision_receive) = mpsc::channel();
        for (client, id) in [first.note.id, second.note.id].into_iter().enumerate() {
            let socket = path.clone();
            let send = collision_send.clone();
            thread::spawn(move || {
                let response = forward(
                    &socket,
                    &RequestEnvelope {
                        version: PROTOCOL_VERSION,
                        request_id: shared_id,
                        command: Some(Command::ReadNote { id }),
                    },
                )
                .unwrap();
                send.send((client, response)).unwrap();
            });
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut collisions = Vec::new();
        while collisions.len() < 2 {
            owner.poll(&mut app);
            while let Ok(value) = collision_receive.try_recv() {
                collisions.push(value);
            }
            assert!(
                Instant::now() < deadline,
                "overlapping request IDs did not complete"
            );
            thread::sleep(Duration::from_millis(2));
        }
        collisions.sort_by_key(|(client, _)| *client);
        assert!(collisions.iter().all(|(_, r)| r.request_id == shared_id));
        assert!(
            matches!(&collisions[0].1.outcome, Outcome::Result(CommandResult::Note(note)) if note.note.text == "one")
        );
        assert!(
            matches!(&collisions[1].1.outcome, Outcome::Result(CommandResult::Note(note)) if note.note.text == "two")
        );
        let edit = |replacement: &str| Command::EditNote {
            id: first.note.id,
            expected: first.note.revision,
            edits: vec![crate::notes::TextEdit {
                start: 0,
                end: 3,
                replacement: replacement.into(),
            }],
        };
        clients[0].send(edit("alpha")).unwrap();
        clients[1].send(edit("beta")).unwrap();
        let writes = collect(&mut owner, &mut app, 2);
        assert_eq!(
            writes
                .iter()
                .filter(|(_, r)| matches!(r.outcome, Outcome::Result(CommandResult::Note(_))))
                .count(),
            1
        );
        assert_eq!(
            writes
                .iter()
                .filter(|(_, r)| matches!(
                    r.outcome,
                    Outcome::Error(CommandError::StaleRevision { .. })
                ))
                .count(),
            1
        );
        let confirmed = writes
            .iter()
            .find_map(|(_, r)| {
                if let Outcome::Result(CommandResult::Note(note)) = &r.outcome {
                    Some(note)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(confirmed.save.persisted, Some(confirmed.note.revision));
        app.execute(
            Command::SetAutomationWriteEnabled { enabled: false },
            Caller::Local,
        )
        .unwrap();
        clients[0].send(create("blocked-a.md", "a")).unwrap();
        clients[1].send(create("blocked-b.md", "b")).unwrap();
        assert!(
            collect(&mut owner, &mut app, 2)
                .iter()
                .all(|(_, r)| matches!(r.outcome, Outcome::Error(CommandError::Unauthorized)))
        );
        drop(clients.remove(0));
        joins.remove(0).join().unwrap();
        clients[0]
            .send(Command::ReadNote { id: second.note.id })
            .unwrap();
        let read = collect(&mut owner, &mut app, 1);
        assert!(
            matches!(&read[0].1.outcome, Outcome::Result(CommandResult::Note(note)) if note.note.text == "two")
        );
        drop(clients);
        joins.remove(0).join().unwrap();
        drop(owner);
        app.shutdown().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn owner_socket_routes_policy_revision_and_durable_mutations() {
        let directory = std::env::temp_dir().join(format!("tw-mcp-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join("app.sock");
        let mut app = App::open(&directory.join("vault")).unwrap();
        let mut owner = OwnerSocket::bind(&path, AccessPolicy::ReadOnly).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        let create = Command::CreateNote {
            path: "a.md".into(),
            text: "original".into(),
        };
        assert!(matches!(
            round_trip(&mut owner, &mut app, &path, Some(create.clone())),
            Outcome::Error(CommandError::Unauthorized)
        ));
        assert!(app.notes().is_empty());
        let Outcome::Capabilities { commands, .. } = round_trip(&mut owner, &mut app, &path, None)
        else {
            panic!("capabilities missing")
        };
        assert_eq!(
            commands.len(),
            allowed_variants(AccessPolicy::ReadOnly).len()
        );
        assert!(commands.iter().any(|name| name == "research.execute"));
        let schema = serde_json::Value::Object(execute_schema(AccessPolicy::ReadOnly));
        let variants = schema
            .pointer("/$defs/Command/oneOf")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            variants.len(),
            allowed_variants(AccessPolicy::ReadOnly).len()
        );
        assert!(
            !variants
                .iter()
                .any(|variant| variant.to_string().contains("CreateNote"))
        );
        assert!(
            !variants
                .iter()
                .any(|variant| variant.to_string().contains("SetCodexProject"))
        );
        drop(owner);
        assert!(!path.exists());
        let mut owner = OwnerSocket::bind(&path, AccessPolicy::ReadWrite).unwrap();
        let Outcome::Result(CommandResult::Note(created)) =
            round_trip(&mut owner, &mut app, &path, Some(create))
        else {
            panic!("create failed")
        };
        assert_eq!(created.save.persisted, Some(created.note.revision));
        // Research mutations use asynchronous request acknowledgements, not Note results.
        let Outcome::Result(CommandResult::Accepted { request_id }) = round_trip(
            &mut owner,
            &mut app,
            &path,
            Some(Command::Research(
                crate::research::ResearchCommand::Create {
                    title: "Workshop research".into(),
                    question: "Which evidence applies?".into(),
                },
            )),
        ) else {
            panic!("durable research acknowledgement missing")
        };
        assert_eq!(
            app.io_status(request_id),
            Some(&crate::app::IoStatus::Completed)
        );
        assert_eq!(app.research_workspaces().len(), 1);
        // Another owner-side edit may arrive while a bridge waits for an older ack.
        app.execute(
            Command::EditNote {
                id: created.note.id,
                expected: created.note.revision,
                edits: vec![crate::notes::TextEdit {
                    start: 0,
                    end: 8,
                    replacement: "newer".into(),
                }],
            },
            Caller::Local,
        )
        .unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        owner.waiting.push(Waiting {
            pending: Pending {
                request: RequestEnvelope {
                    version: PROTOCOL_VERSION,
                    request_id: Uuid::new_v4(),
                    command: None,
                },
                deadline: Instant::now() + TIMEOUT,
                reply: send,
            },
            result: CommandResult::Note(created.clone()),
        });
        owner.poll(&mut app);
        let Outcome::Result(CommandResult::Note(confirmed)) = receive.try_recv().unwrap().outcome
        else {
            panic!("old ack confirmation missing")
        };
        assert_eq!(confirmed.note.text, "original");
        assert_eq!(confirmed.note.revision, created.note.revision);
        assert_eq!(confirmed.save.persisted, Some(created.note.revision));

        assert!(matches!(
            round_trip(
                &mut owner,
                &mut app,
                &path,
                Some(Command::EditNote {
                    id: created.note.id,
                    expected: crate::notes::Revision(Uuid::new_v4()),
                    edits: Vec::new(),
                })
            ),
            Outcome::Error(CommandError::StaleRevision { .. })
        ));
        drop(owner);
        app.shutdown().unwrap();
        let mut reopened = App::open(&directory.join("vault")).unwrap();
        assert_eq!(reopened.note(created.note.id).unwrap().note.text, "newer");
        reopened.shutdown().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }
}
