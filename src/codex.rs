//! Small pinned Codex app-server boundary. No ECS or vault ownership crosses here.
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::process::CommandExt},
    path::PathBuf,
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

const LINE_LIMIT: usize = 1024 * 1024;
const PROMPT_LIMIT: usize = 256 * 1024;
const EVENT_BUDGET: usize = 4 * 1024 * 1024;
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
const TURN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Debug)]
pub struct CodexConfig {
    pub executable: PathBuf,
    /// Separate working directory; note mutations go through the owner MCP socket.
    pub cwd: PathBuf,
    pub mcp_bridge: PathBuf,
    pub socket: PathBuf,
    pub resume_thread: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    Accept,
    Decline,
    Cancel,
}
impl ApprovalDecision {
    fn wire(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
        }
    }
}
#[derive(Clone, Debug)]
pub enum CodexRequest {
    SendPrompt(String),
    Interrupt,
    AnswerApproval {
        id: Value,
        decision: ApprovalDecision,
    },
    Disconnect,
}
#[derive(Clone, Debug)]
pub enum CodexEvent {
    Connecting,
    Ready {
        thread: String,
    },
    TurnStarted {
        thread: String,
        turn: String,
    },
    Delta {
        thread: String,
        turn: String,
        item: String,
        text: String,
    },
    MessageComplete {
        thread: String,
        turn: String,
        item: String,
        text: String,
    },
    TurnCompleted {
        thread: String,
        turn: String,
        status: String,
    },
    Approval {
        id: Value,
        thread: String,
        turn: String,
        method: String,
        details: Value,
        decisions: Vec<ApprovalDecision>,
    },
    ApprovalResolved {
        id: Value,
        thread: String,
    },
    Error(String),
    Disconnected(String),
}
struct QueuedEvent {
    event: CodexEvent,
    bytes: usize,
}

pub struct CodexAdapter {
    requests: SyncSender<CodexRequest>,
    events: Receiver<QueuedEvent>,
    bytes: Arc<AtomicUsize>,
    fault: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl CodexAdapter {
    /// Starts only initialization and thread creation/resumption; never starts a paid turn.
    pub fn spawn(config: CodexConfig, wake: impl Fn() + Send + Sync + 'static) -> io::Result<Self> {
        if !config.cwd.is_absolute()
            || !config.mcp_bridge.is_absolute()
            || !config.socket.is_absolute()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Codex cwd, bridge and socket must be absolute",
            ));
        }
        let (requests, receive) = mpsc::sync_channel(16);
        let (send, events) = mpsc::sync_channel(128);
        let bytes = Arc::new(AtomicUsize::new(0));
        let fault = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let sink = EventSink {
            send,
            bytes: bytes.clone(),
            fault: fault.clone(),
            wake: Arc::new(wake),
        };
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("tanglewood-codex".into())
            .spawn(move || {
                if let Err(error) = run_worker(config, receive, &sink, &worker_stop) {
                    sink.fail(format!("Codex disconnected: {error}"));
                }
            })?;
        Ok(Self {
            requests,
            events,
            bytes,
            fault,
            stop,
            worker: Some(worker),
        })
    }
    pub fn try_send(&self, request: CodexRequest) -> Result<(), String> {
        if matches!(&request, CodexRequest::SendPrompt(text) if text.len() > PROMPT_LIMIT) {
            return Err("prompt exceeds 256 KiB".into());
        }
        self.requests
            .try_send(request)
            .map_err(|error| format!("Codex request not queued: {error}"))
    }
    pub fn try_recv(&self) -> Option<CodexEvent> {
        if let Ok(queued) = self.events.try_recv() {
            self.bytes.fetch_sub(queued.bytes, Ordering::AcqRel);
            return Some(queued.event);
        }
        self.fault.lock().ok()?.take().map(CodexEvent::Disconnected)
    }
}
impl Drop for CodexAdapter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct EventSink {
    send: SyncSender<QueuedEvent>,
    bytes: Arc<AtomicUsize>,
    fault: Arc<Mutex<Option<String>>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
impl EventSink {
    fn emit(&self, event: CodexEvent) -> Result<(), String> {
        let bytes = match &event {
            CodexEvent::Connecting => 64,
            CodexEvent::Ready { thread } => thread.len() + 64,
            CodexEvent::TurnStarted { thread, turn } => thread.len() + turn.len() + 64,
            CodexEvent::Delta {
                thread,
                turn,
                item,
                text,
            }
            | CodexEvent::MessageComplete {
                thread,
                turn,
                item,
                text,
            } => thread.len() + turn.len() + item.len() + text.len() + 64,
            CodexEvent::TurnCompleted {
                thread,
                turn,
                status,
            } => thread.len() + turn.len() + status.len() + 64,
            CodexEvent::Approval {
                id,
                thread,
                turn,
                method,
                details,
                ..
            } => {
                id.to_string().len()
                    + thread.len()
                    + turn.len()
                    + method.len()
                    + details.to_string().len()
                    + 128
            }
            CodexEvent::ApprovalResolved { id, thread } => id.to_string().len() + thread.len() + 64,
            CodexEvent::Error(message) | CodexEvent::Disconnected(message) => message.len() + 64,
        };
        self.bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |total| {
                total
                    .checked_add(bytes)
                    .filter(|total| *total <= EVENT_BUDGET)
            })
            .map_err(|_| "conversation event byte budget exceeded".to_owned())?;
        if self.send.try_send(QueuedEvent { event, bytes }).is_err() {
            self.bytes.fetch_sub(bytes, Ordering::AcqRel);
            return Err("conversation event queue full or disconnected".into());
        }
        (self.wake)();
        Ok(())
    }
    fn fail(&self, message: String) {
        if let Ok(mut fault) = self.fault.lock() {
            *fault = Some(message);
        }
        (self.wake)();
    }
}

#[derive(Clone, Copy, Debug)]
enum RequestKind {
    Initialize,
    Thread,
    Turn,
    Interrupt,
}
struct Pending {
    kind: RequestKind,
    deadline: Instant,
}
#[derive(Clone, Copy)]
enum ApprovalKind {
    Process,
    TanglewoodTool,
}
struct Approval {
    kind: ApprovalKind,
    id: Value,
    thread: String,
    turn: String,
    decisions: Vec<ApprovalDecision>,
}
struct Protocol {
    next_id: u64,
    pending: HashMap<u64, Pending>,
    approvals: HashMap<String, Approval>,
    thread: Option<String>,
    turn: Option<String>,
    turn_starting: bool,
    turn_deadline: Option<Instant>,
    cancel_pending: bool,
    resume: Option<String>,
    cwd: String,
}
impl Protocol {
    fn new(config: &CodexConfig) -> Self {
        Self {
            next_id: 1,
            pending: HashMap::new(),
            approvals: HashMap::new(),
            thread: None,
            turn: None,
            turn_starting: false,
            turn_deadline: None,
            cancel_pending: false,
            resume: config.resume_thread.clone(),
            cwd: config.cwd.to_string_lossy().into_owned(),
        }
    }
    fn request(&mut self, kind: RequestKind, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(
            id,
            Pending {
                kind,
                deadline: Instant::now() + RPC_TIMEOUT,
            },
        );
        json!({"id":id,"method":method,"params":params})
    }
    fn initialize(&mut self) -> Value {
        self.request(RequestKind::Initialize, "initialize", json!({"clientInfo":{"name":"tanglewood","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":false}}))
    }
    fn command(&mut self, request: CodexRequest) -> Result<Vec<Value>, String> {
        if self.pending.len() >= 16
            && matches!(
                &request,
                CodexRequest::SendPrompt(_) | CodexRequest::Interrupt
            )
        {
            return Err("Codex has 16 pending RPC requests".into());
        }
        match request {
            CodexRequest::SendPrompt(text) => {
                if text.len() > PROMPT_LIMIT {
                    return Err("prompt exceeds 256 KiB".into());
                }
                let thread = self.thread.clone().ok_or("Codex is not ready")?;
                if self.turn.is_some() || self.turn_starting {
                    return Err("a Codex turn is already running".into());
                }
                self.turn_starting = true;
                self.turn_deadline = Some(Instant::now() + TURN_TIMEOUT);
                Ok(vec![self.request(RequestKind::Turn, "turn/start", json!({"threadId":thread,"input":[{"type":"text","text":text}],"approvalPolicy":"on-request","approvalsReviewer":"user","sandboxPolicy":{"type":"readOnly","networkAccess":false}}))])
            }
            CodexRequest::Interrupt => {
                if let (Some(thread), Some(turn)) = (self.thread.clone(), self.turn.clone()) {
                    Ok(vec![self.request(
                        RequestKind::Interrupt,
                        "turn/interrupt",
                        json!({"threadId":thread,"turnId":turn}),
                    )])
                } else if self.turn_starting {
                    self.cancel_pending = true;
                    Ok(Vec::new())
                } else {
                    Err("no active turn to interrupt".into())
                }
            }
            CodexRequest::AnswerApproval { id, decision } => {
                let key = id.to_string();
                let approval = self
                    .approvals
                    .get(&key)
                    .ok_or("approval is no longer pending")?;
                if Some(&approval.thread) != self.thread.as_ref()
                    || Some(&approval.turn) != self.turn.as_ref()
                {
                    return Err("approval belongs to an inactive turn".into());
                }
                if !approval.decisions.contains(&decision) {
                    return Err("decision not offered by this approval".into());
                }
                let approval = self.approvals.remove(&key).unwrap();
                let result = match approval.kind {
                    ApprovalKind::Process => json!({"decision":decision.wire()}),
                    ApprovalKind::TanglewoodTool if decision == ApprovalDecision::Accept => {
                        json!({"action":"accept","content":{}})
                    }
                    ApprovalKind::TanglewoodTool => json!({"action":decision.wire()}),
                };
                Ok(vec![json!({"id":approval.id,"result":result})])
            }
            CodexRequest::Disconnect => Err("disconnect requested".into()),
        }
    }
    fn frame(&mut self, frame: Value) -> Result<(Vec<Value>, Vec<CodexEvent>), String> {
        let mut writes = Vec::new();
        let mut events = Vec::new();
        if let Some(method) = frame.get("method").and_then(Value::as_str) {
            let params = frame.get("params").cloned().unwrap_or(Value::Null);
            if let Some(id) = frame.get("id") {
                let thread = params
                    .get("threadId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let turn = params
                    .get("turnId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let kind = if matches!(
                    method,
                    "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
                ) {
                    Some(ApprovalKind::Process)
                } else if method == "mcpServer/elicitation/request"
                    && tanglewood_tool_approval(&params)
                {
                    Some(ApprovalKind::TanglewoodTool)
                } else {
                    None
                };
                if kind.is_some()
                    && self.thread.as_deref() == Some(thread)
                    && self.turn.as_deref() == Some(turn)
                    && self.approvals.len() < 16
                {
                    let mut decisions = vec![
                        ApprovalDecision::Accept,
                        ApprovalDecision::Decline,
                        ApprovalDecision::Cancel,
                    ];
                    if let Some(available) =
                        params.get("availableDecisions").and_then(Value::as_array)
                    {
                        decisions.retain(|decision| {
                            available
                                .iter()
                                .any(|value| value.as_str() == Some(decision.wire()))
                        });
                    }
                    let key = id.to_string();
                    if self.approvals.contains_key(&key) {
                        return Err("duplicate server approval id".into());
                    }
                    self.approvals.insert(
                        key,
                        Approval {
                            kind: kind.expect("checked approval kind"),
                            id: id.clone(),
                            thread: thread.into(),
                            turn: turn.into(),
                            decisions: decisions.clone(),
                        },
                    );
                    events.push(CodexEvent::Approval {
                        id: id.clone(),
                        thread: thread.into(),
                        turn: turn.into(),
                        method: method.into(),
                        details: params,
                        decisions,
                    });
                } else {
                    writes.push(json!({"id":id,"error":{"code":-32601,"message":"unsupported or inactive server request; no approval granted"}}));
                }
                return Ok((writes, events));
            }
            let thread = params
                .get("threadId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if self.thread.as_deref() != Some(thread) {
                return Ok((writes, events));
            }
            if method == "serverRequest/resolved" {
                let id = params
                    .get("requestId")
                    .ok_or("resolved request missing requestId")?;
                if self.approvals.remove(&id.to_string()).is_some() {
                    events.push(CodexEvent::ApprovalResolved {
                        id: id.clone(),
                        thread: thread.into(),
                    });
                }
                return Ok((writes, events));
            }
            let turn = params
                .get("turnId")
                .and_then(Value::as_str)
                .or_else(|| params.pointer("/turn/id").and_then(Value::as_str))
                .unwrap_or_default();
            if method == "turn/started"
                && self.turn_starting
                && self.turn.is_none()
                && !turn.is_empty()
            {
                self.turn = Some(turn.into());
                events.push(CodexEvent::TurnStarted {
                    thread: thread.into(),
                    turn: turn.into(),
                });
            }
            if self.turn.as_deref() != Some(turn) {
                return Ok((writes, events));
            }
            match method {
                "item/agentMessage/delta" => events.push(CodexEvent::Delta {
                    thread: thread.into(),
                    turn: turn.into(),
                    item: field(&params, "itemId")?,
                    text: field(&params, "delta")?,
                }),
                "item/completed"
                    if params.pointer("/item/type").and_then(Value::as_str)
                        == Some("agentMessage") =>
                {
                    events.push(CodexEvent::MessageComplete {
                        thread: thread.into(),
                        turn: turn.into(),
                        item: field(&params["item"], "id")?,
                        text: field(&params["item"], "text")?,
                    });
                }
                "turn/completed" => {
                    events.push(CodexEvent::TurnCompleted {
                        thread: thread.into(),
                        turn: turn.into(),
                        status: field(&params["turn"], "status")?,
                    });
                    self.turn = None;
                    self.turn_starting = false;
                    self.turn_deadline = None;
                    self.cancel_pending = false;
                    self.approvals.clear();
                }
                "error" => events.push(CodexEvent::Error(
                    params
                        .get("error")
                        .map(Value::to_string)
                        .unwrap_or_else(|| "Codex turn error".into()),
                )),
                _ => {}
            }
        } else if let Some(id) = frame.get("id").and_then(Value::as_u64) {
            let Some(pending) = self.pending.remove(&id) else {
                return Ok((writes, events));
            };
            if let Some(error) = frame.get("error") {
                if matches!(pending.kind, RequestKind::Initialize | RequestKind::Thread) {
                    return Err(format!("Codex setup rejected: {error}"));
                }
                if matches!(pending.kind, RequestKind::Turn) {
                    self.turn_starting = false;
                    self.turn = None;
                    self.turn_deadline = None;
                    self.cancel_pending = false;
                    self.approvals.clear();
                }
                events.push(CodexEvent::Error(format!(
                    "Codex request rejected: {error}"
                )));
                return Ok((writes, events));
            }
            let result = frame.get("result").ok_or("Codex response missing result")?;
            match pending.kind {
                RequestKind::Initialize => {
                    writes.push(json!({"method":"initialized","params":{}}));
                    let mut params = json!({"cwd":self.cwd,"approvalPolicy":"on-request","approvalsReviewer":"user","sandbox":"read-only","developerInstructions":"Use the tanglewood MCP server for all note reads and mutations. Never open note vault files directly. MCP permissions are independent from process permissions."});
                    let method = if let Some(thread) = &self.resume {
                        params["threadId"] = json!(thread);
                        params["excludeTurns"] = json!(true);
                        "thread/resume"
                    } else {
                        "thread/start"
                    };
                    writes.push(self.request(RequestKind::Thread, method, params));
                }
                RequestKind::Thread => {
                    if result.get("approvalPolicy").and_then(Value::as_str) != Some("on-request")
                        || result.get("approvalsReviewer").and_then(Value::as_str) != Some("user")
                        || result.pointer("/sandbox/type").and_then(Value::as_str)
                            != Some("readOnly")
                        || result
                            .pointer("/sandbox/networkAccess")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                        || result.get("cwd").and_then(Value::as_str) != Some(self.cwd.as_str())
                    {
                        return Err("Codex did not confirm requested read-only sandbox, approval policy, reviewer and working directory".into());
                    }
                    let thread = field(&result["thread"], "id")?;
                    if self
                        .resume
                        .as_ref()
                        .is_some_and(|expected| expected != &thread)
                    {
                        return Err("Codex resumed an unexpected thread".into());
                    }
                    self.thread = Some(thread.clone());
                    events.push(CodexEvent::Ready { thread });
                }
                RequestKind::Turn if self.turn_starting => {
                    let turn = field(&result["turn"], "id")?;
                    if self.turn.as_ref().is_some_and(|active| active != &turn) {
                        return Err("Codex turn response disagrees with active turn".into());
                    }
                    if self.turn.is_none() {
                        self.turn = Some(turn.clone());
                        events.push(CodexEvent::TurnStarted {
                            thread: self.thread.clone().unwrap(),
                            turn,
                        });
                    }
                }
                _ => {}
            }
        }
        if self.cancel_pending && self.turn.is_some() {
            self.cancel_pending = false;
            writes.extend(self.command(CodexRequest::Interrupt)?);
        }
        Ok((writes, events))
    }
    fn check_timeouts(&self, now: Instant) -> Result<(), String> {
        if self.pending.values().any(|pending| now >= pending.deadline) {
            return Err("Codex RPC timed out; outcome may be unknown".into());
        }
        if self.turn_deadline.is_some_and(|deadline| now >= deadline) {
            return Err("Codex turn exceeded 10 minute limit".into());
        }
        Ok(())
    }
}
/// Only the captured Codex empty-form confirmation for our own MCP tools.
/// Arbitrary forms/URLs and other MCP servers are not implicit permissions.
fn tanglewood_tool_approval(params: &Value) -> bool {
    if params.get("mode").and_then(Value::as_str) != Some("form")
        || params.get("serverName").and_then(Value::as_str) != Some("tanglewood")
        || params
            .pointer("/_meta/codex_approval_kind")
            .and_then(Value::as_str)
            != Some("mcp_tool_call")
        || params.get("requestedSchema") != Some(&json!({"type":"object","properties":{}}))
    {
        return false;
    }
    params
        .pointer("/_meta/tool_params")
        .is_some_and(Value::is_object)
}

fn field(value: &Value, name: &str) -> Result<String, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("Codex protocol missing {name}"))
}

struct Process {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
    errors: ChildStderr,
    read: Vec<u8>,
    writes: std::collections::VecDeque<Vec<u8>>,
    written: usize,
    queued_bytes: usize,
}
impl Process {
    fn spawn(config: &CodexConfig) -> Result<Self, String> {
        let command = config
            .mcp_bridge
            .to_str()
            .ok_or("bridge path must be UTF-8")?;
        let socket = config.socket.to_str().ok_or("socket path must be UTF-8")?;
        // JSON string quoting is compatible with these TOML basic string/array values.
        let mcp = format!(
            "mcp_servers={{tanglewood={{command={},args={}}}}}",
            serde_json::to_string(command).map_err(|e| e.to_string())?,
            serde_json::to_string(&["--socket", socket]).map_err(|e| e.to_string())?
        );
        let mut child = Command::new(&config.executable)
            .args([
                "-c",
                &mcp,
                "-c",
                "sandbox_mode=\"read-only\"",
                "-c",
                "approval_policy=\"on-request\"",
                "-c",
                "approvals_reviewer=\"user\"",
                "app-server",
            ])
            .current_dir(&config.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|error| format!("cannot launch Codex: {error}"))?;
        let input = child.stdin.take().ok_or("missing child stdin")?;
        let output = child.stdout.take().ok_or("missing child stdout")?;
        let errors = child.stderr.take().ok_or("missing child stderr")?;
        let process = Self {
            child,
            input,
            output,
            errors,
            read: Vec::new(),
            writes: std::collections::VecDeque::new(),
            written: 0,
            queued_bytes: 0,
        };
        for fd in [
            process.input.as_raw_fd(),
            process.output.as_raw_fd(),
            process.errors.as_raw_fd(),
        ] {
            // SAFETY: these are live owned pipe descriptors; F_GETFL/F_SETFL use scalar flags.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(io::Error::last_os_error().to_string());
            }
        }
        Ok(process)
    }
    fn enqueue(&mut self, frame: Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(&frame).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        if bytes.len() > LINE_LIMIT
            || self.writes.len() >= 32
            || self.queued_bytes + bytes.len() > 2 * LINE_LIMIT
        {
            return Err("Codex outgoing queue limit exceeded".into());
        }
        self.queued_bytes += bytes.len();
        self.writes.push_back(bytes);
        Ok(())
    }
    fn pump(&mut self) -> Result<Vec<Value>, String> {
        while let Some(bytes) = self.writes.front() {
            match self.input.write(&bytes[self.written..]) {
                Ok(0) => return Err("Codex stdin closed".into()),
                Ok(count) => {
                    self.written += count;
                    if self.written == bytes.len() {
                        self.queued_bytes -= bytes.len();
                        self.writes.pop_front();
                        self.written = 0;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(format!("Codex write failed: {error}")),
            }
        }
        let mut frames = Vec::new();
        let mut chunk = [0; 16384];
        // Bound work per tick, even if the child floods its pipes.
        for _ in 0..16 {
            match self.output.read(&mut chunk) {
                Ok(0) => return Err("Codex stdout disconnected".into()),
                Ok(count) => self.read.extend_from_slice(&chunk[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(format!("Codex read failed: {error}")),
            }
            while let Some(end) = self.read.iter().position(|byte| *byte == b'\n') {
                if end > LINE_LIMIT {
                    return Err("Codex line exceeded 1 MiB".into());
                }
                let line: Vec<_> = self.read.drain(..=end).collect();
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                frames.push(
                    serde_json::from_slice(&line)
                        .map_err(|error| format!("invalid Codex JSONL: {error}"))?,
                );
                if frames.len() > 128 {
                    return Err("Codex notification burst exceeded limit".into());
                }
            }
            if self.read.len() > LINE_LIMIT {
                return Err("Codex line exceeded 1 MiB".into());
            }
        }
        // Drain diagnostics to keep the child running; do not copy authentication diagnostics
        // or arbitrary child stderr into logs/transcripts.
        for _ in 0..4 {
            match self.errors.read(&mut chunk) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(format!("Codex stderr failed: {error}")),
            }
        }
        if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("Codex child exited: {status}"));
        }
        Ok(frames)
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: this child was launched in its own process group; terminate its
        // inherited bridge processes as well, never a shared application process.
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.child.wait();
    }
}
struct VersionProbe {
    child: Child,
    output: ChildStdout,
}
impl Drop for VersionProbe {
    fn drop(&mut self) {
        // SAFETY: the probe was launched in a dedicated process group.
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.child.wait();
    }
}
fn read_version_probe(
    mut child: Child,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<(), String> {
    let output = child.stdout.take().ok_or("version probe missing stdout")?;
    let mut probe = VersionProbe { child, output };
    // SAFETY: output is a live pipe descriptor; fcntl uses scalar flags.
    let flags = unsafe { libc::fcntl(probe.output.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe {
            libc::fcntl(
                probe.output.as_raw_fd(),
                libc::F_SETFL,
                flags | libc::O_NONBLOCK,
            )
        } < 0
    {
        return Err(io::Error::last_os_error().to_string());
    }
    let mut bytes = Vec::new();
    let mut ended = false;
    loop {
        if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err("Codex version check canceled or timed out".into());
        }
        let mut chunk = [0; 256];
        match probe.output.read(&mut chunk) {
            Ok(0) => ended = true,
            Ok(count) => {
                bytes.extend_from_slice(&chunk[..count]);
                if bytes.len() > 1024 {
                    return Err("Codex version output exceeded limit".into());
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.to_string()),
        }
        if let Some(status) = probe.child.try_wait().map_err(|error| error.to_string())? {
            // Exit is insufficient: an inherited stdout may still be open in a descendant.
            if ended {
                return if status.success()
                    && std::str::from_utf8(&bytes)
                        .is_ok_and(|version| version.trim() == "codex-cli 0.163.0")
                {
                    Ok(())
                } else {
                    Err("Codex adapter requires codex-cli 0.163.0".into())
                };
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn verify_version(config: &CodexConfig, stop: &AtomicBool) -> Result<(), String> {
    let child = Command::new(&config.executable)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|error| error.to_string())?;
    read_version_probe(child, stop, Instant::now() + Duration::from_secs(5))
}

fn run_worker(
    mut config: CodexConfig,
    requests: Receiver<CodexRequest>,
    sink: &EventSink,
    stop: &AtomicBool,
) -> Result<(), String> {
    sink.emit(CodexEvent::Connecting)?;
    config.cwd = std::fs::canonicalize(&config.cwd)
        .map_err(|error| format!("Codex working directory unavailable: {error}"))?;
    if !config.cwd.is_dir() {
        return Err("Codex working directory must be a directory".into());
    }
    verify_version(&config, stop)?;
    let mut process = Process::spawn(&config)?;
    let mut protocol = Protocol::new(&config);
    process.enqueue(protocol.initialize())?;
    while !stop.load(Ordering::Acquire) {
        for _ in 0..16 {
            match requests.try_recv() {
                Ok(CodexRequest::Disconnect) => return Err("disconnected by user".into()),
                Ok(request) => match protocol.command(request) {
                    Ok(frames) => {
                        for frame in frames {
                            process.enqueue(frame)?;
                        }
                    }
                    Err(error) => sink.emit(CodexEvent::Error(error))?,
                },
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("conversation owner disconnected".into());
                }
            }
        }
        for frame in process.pump()? {
            let (writes, events) = protocol.frame(frame)?;
            for write in writes {
                process.enqueue(write)?;
            }
            for event in events {
                sink.emit(event)?;
            }
        }
        protocol.check_timeouts(Instant::now())?;
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_orders_initialization_correlates_turns_and_requires_explicit_approval() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/app.sock".into(),
            resume_thread: None,
        };
        let mut state = Protocol::new(&config);
        assert_eq!(state.initialize()["method"], "initialize");
        assert!(
            state
                .command(CodexRequest::SendPrompt("early".into()))
                .is_err()
        );
        let (writes, _) = state.frame(json!({"id":1,"result":{}})).unwrap();
        assert_eq!(writes[0]["method"], "initialized");
        assert_eq!(writes[1]["method"], "thread/start");
        let (_, events) = state
            .frame(json!({"id":2,"result":{"thread":{"id":"thread-a"},"approvalPolicy":"on-request","approvalsReviewer":"user","sandbox":{"type":"readOnly"},"cwd":"/tmp"}}))
            .unwrap();
        assert!(matches!(&events[0],CodexEvent::Ready{thread} if thread=="thread-a"));
        let turn = state
            .command(CodexRequest::SendPrompt("hello".into()))
            .unwrap();
        assert_eq!(turn[0]["params"]["sandboxPolicy"]["type"], "readOnly");
        state
            .frame(json!({"id":3,"result":{"turn":{"id":"turn-a"}}}))
            .unwrap();
        let(_,events)=state.frame(json!({"method":"item/agentMessage/delta","params":{"threadId":"thread-a","turnId":"stale","itemId":"i","delta":"ignored"}})).unwrap();
        assert!(events.is_empty());
        let(_,events)=state.frame(json!({"method":"item/agentMessage/delta","params":{"threadId":"thread-a","turnId":"turn-a","itemId":"i","delta":"hello"}})).unwrap();
        assert!(matches!(&events[0],CodexEvent::Delta{text,..} if text=="hello"));
        let(writes,events)=state.frame(json!({"id":"approval-1","method":"item/commandExecution/requestApproval","params":{"threadId":"thread-a","turnId":"turn-a","availableDecisions":["decline"],"command":"example"}})).unwrap();
        assert!(writes.is_empty());
        assert!(matches!(&events[0], CodexEvent::Approval { .. }));
        assert!(
            state
                .command(CodexRequest::AnswerApproval {
                    id: json!("approval-1"),
                    decision: ApprovalDecision::Accept
                })
                .is_err()
        );
        let answer = state
            .command(CodexRequest::AnswerApproval {
                id: json!("approval-1"),
                decision: ApprovalDecision::Decline,
            })
            .unwrap();
        assert_eq!(answer[0]["result"]["decision"], "decline");
        state.frame(json!({"id":"approval-2","method":"item/fileChange/requestApproval","params":{"threadId":"thread-a","turnId":"turn-a"}})).unwrap();
        let (_, events) = state.frame(json!({"method":"serverRequest/resolved","params":{"threadId":"thread-a","requestId":"approval-2"}})).unwrap();
        assert!(
            matches!(&events[0], CodexEvent::ApprovalResolved { id, .. } if id == "approval-2")
        );
        assert!(
            state
                .command(CodexRequest::AnswerApproval {
                    id: json!("approval-2"),
                    decision: ApprovalDecision::Accept
                })
                .is_err()
        );
        let (writes, _) = state
            .frame(json!({"id":9,"method":"unsupported/request","params":{}}))
            .unwrap();
        assert_eq!(writes[0]["error"]["code"], -32601);
        let interrupt = state.command(CodexRequest::Interrupt).unwrap();
        assert_eq!(interrupt[0]["params"]["turnId"], "turn-a");
        state.frame(json!({"id":4,"result":{}})).unwrap();
        let(_,events)=state.frame(json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-a","status":"interrupted"}}})).unwrap();
        assert!(matches!(&events[0],CodexEvent::TurnCompleted{status,..} if status=="interrupted"));
        assert!(
            state
                .command(CodexRequest::AnswerApproval {
                    id: json!("approval-1"),
                    decision: ApprovalDecision::Decline
                })
                .is_err()
        );
        state
            .command(CodexRequest::SendPrompt("timeout".into()))
            .unwrap();
        assert!(
            state
                .check_timeouts(Instant::now() + RPC_TIMEOUT + Duration::from_secs(1))
                .is_err()
        );
        // Captured live empty form: a tool call requires an explicit one-action answer.
        state
            .frame(json!({"id":5,"result":{"turn":{"id":"turn-b"}}}))
            .unwrap();
        let captured = json!({"id":0,"method":"mcpServer/elicitation/request","params":{
            "threadId":"thread-a","turnId":"turn-b","serverName":"tanglewood","mode":"form",
            "_meta":{"codex_approval_kind":"mcp_tool_call","persist":["session","always"],
                "tool_params":{"command":{"ReadNote":{"id":"00000000-0000-0000-0000-000000000000"}}}},
            "message":"Allow the tanglewood MCP server to run tool execute?",
            "requestedSchema":{"type":"object","properties":{}}}});
        let (writes, events) = state.frame(captured.clone()).unwrap();
        assert!(writes.is_empty());
        assert!(matches!(&events[0], CodexEvent::Approval { .. }));
        let answer = state
            .command(CodexRequest::AnswerApproval {
                id: json!(0),
                decision: ApprovalDecision::Accept,
            })
            .unwrap();
        assert_eq!(
            answer[0],
            json!({"id":0,"result":{"action":"accept","content":{}}})
        );
        for (id, decision) in [
            (20, ApprovalDecision::Decline),
            (21, ApprovalDecision::Cancel),
        ] {
            let mut request = captured.clone();
            request["id"] = json!(id);
            state.frame(request).unwrap();
            let answer = state
                .command(CodexRequest::AnswerApproval {
                    id: json!(id),
                    decision,
                })
                .unwrap();
            assert_eq!(
                answer[0],
                json!({"id":id,"result":{"action":decision.wire()}})
            );
        }
        let mut resolved = captured.clone();
        resolved["id"] = json!(22);
        state.frame(resolved).unwrap();
        let (_,events)=state.frame(json!({"method":"serverRequest/resolved","params":{"threadId":"thread-a","requestId":22}})).unwrap();
        assert!(matches!(&events[0], CodexEvent::ApprovalResolved { .. }));
        assert!(
            state
                .command(CodexRequest::AnswerApproval {
                    id: json!(22),
                    decision: ApprovalDecision::Accept
                })
                .is_err()
        );
        let mut nonempty = captured.clone();
        nonempty["id"] = json!(23);
        nonempty["params"]["requestedSchema"]["properties"] = json!({"confirm":{"type":"boolean"}});
        let (writes, events) = state.frame(nonempty).unwrap();
        assert!(events.is_empty());
        assert_eq!(writes[0]["error"]["code"], -32601);
        let mut capabilities = captured.clone();
        capabilities["id"] = json!(25);
        capabilities["params"]["_meta"]["tool_params"] = json!({});
        capabilities["params"]["message"] =
            json!("Allow the tanglewood MCP server to run tool capabilities?");
        let (writes, events) = state.frame(capabilities).unwrap();
        assert!(writes.is_empty());
        assert!(matches!(&events[0], CodexEvent::Approval { .. }));
        let answer = state
            .command(CodexRequest::AnswerApproval {
                id: json!(25),
                decision: ApprovalDecision::Accept,
            })
            .unwrap();
        assert_eq!(
            answer[0],
            json!({"id":25,"result":{"action":"accept","content":{}}})
        );
        let mut url = captured;
        url["id"] = json!(24);
        url["params"]["mode"] = json!("url");
        let (writes, events) = state.frame(url).unwrap();
        assert!(events.is_empty());
        assert_eq!(writes[0]["error"]["code"], -32601);
        // Connect returns without touching the filesystem; unavailable cwd fails asynchronously.
        let mut unavailable = config.clone();
        unavailable.cwd =
            std::env::temp_dir().join(format!("tanglewood-codex-absent-{}", uuid::Uuid::new_v4()));
        let adapter = CodexAdapter::spawn(unavailable, || {}).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(CodexEvent::Disconnected(error)) = adapter.try_recv() {
                assert!(error.contains("working directory unavailable"));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "asynchronous cwd failure missing"
            );
            thread::sleep(Duration::from_millis(2));
        }
        drop(adapter);
        // Parent exits with valid version text, but its descendant holds stdout open.
        let mut probe = Command::new("/bin/sh")
            .args(["-c", "sleep 30 & printf 'codex-cli 0.163.0\n'; exit 0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap();
        probe.wait().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        let canceler = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            cancel.store(true, Ordering::Release);
        });
        let started = Instant::now();
        assert!(
            read_version_probe(probe, &stop, started + Duration::from_secs(3))
                .unwrap_err()
                .contains("canceled")
        );
        canceler.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        // A real short-lived child checks EOF handling without credentials or a model turn.
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        let mut process = Process {
            input: child.stdin.take().unwrap(),
            output: child.stdout.take().unwrap(),
            errors: child.stderr.take().unwrap(),
            child,
            read: Vec::new(),
            writes: std::collections::VecDeque::new(),
            written: 0,
            queued_bytes: 0,
        };
        assert!(process.pump().unwrap_err().contains("disconnected"));
    }
}
