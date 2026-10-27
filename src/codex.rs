//! Small pinned Codex app-server boundary. No ECS or vault ownership crosses here.
pub use crate::conversation::{
    Activity, CommandExecution, ConnectionInfo, ContextFile, GoalInfo, HistoryMessage, PlanStep,
    PromptAttachment, SkillInfo, ThreadSummary, ThreadWorkState,
};
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
const KNOWN_THREAD_LIMIT: usize = 512;
const KNOWN_THREAD_BYTES: usize = 256 * 1024;
const THREAD_ID_LIMIT: usize = 1024;

const THREAD_DEVELOPER_GUIDANCE: &str = "Use the tanglewood MCP server for vault notes and research workspaces and sources: use read for queries, capabilities for current authorization, and execute for authorized note and research mutations. Inspect Research List and Research ReadIndex for the current workspace and its source IDs; sources are workspace records, not a specially named note. Do not infer the current workspace or source selection from chat history. Use Research CollectReferences to collect candidate links explicitly; use AcquireSource for durable queued acquisition, FindPassages for selected local evidence, EvidenceGaps for grounded coverage limitations, and CreateOutput with cited structured cards for saved slide HTML/infographic SVG/mind-map SVG. These create local files through the owner, never direct shell vault writes or publication. Do not fabricate source citations or confuse lexical retrieval with verification. For papers, create or find the source reference, then use Research SavePaper with its source_id, current expected workspace revision, and input Url {url: direct PDF URL, archive:true}; URLs and local provenance do not require a DOI. This saves the original PDF and extracts text through the owner. Use Research ReadSource with page:0, offset:0, limit:32768; follow next_offset then next_page. Check source.paper.extraction_error and artifact managed_path before claiming full-text success. Downloading PDFs through SavePaper is allowed with owner write permission; local-file inputs still require local authorization. Never open or write vault files directly. The execute schema stays stable across permission changes and does not grant writes; the vault owner checks its current local policy on every call. MCP permissions and the vault owner's validation are independent from process permissions. In Work/default collaboration mode, use the built-in update_plan tool for multi-step work and keep progress in its structured checklist rather than repeatedly posting the full numbered list as assistant messages. In Plan collaboration mode, produce the proposed plan using that mode's normal proposal workflow; update_plan is a separate progress checklist and must not be called in Plan mode.";
fn child_server_args(mcp: &str) -> Vec<String> {
    [
        "-c",
        mcp,
        "-c",
        "sandbox_mode=\"read-only\"",
        "-c",
        "approval_policy=\"on-request\"",
        "-c",
        "approvals_reviewer=\"user\"",
        "-c",
        "tools.update_plan.enabled=true",
        "app-server",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
#[derive(Clone, Debug)]
pub struct CodexConfig {
    pub executable: PathBuf,
    /// Separate working directory; note mutations go through the owner MCP socket.
    pub cwd: PathBuf,
    pub mcp_bridge: PathBuf,
    pub socket: PathBuf,
    pub resume_thread: Option<String>,
    pub project: Option<crate::notes::CodexProjectBinding>,
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkMode {
    #[default]
    Work,
    Plan,
    Research,
    Goal,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ApprovalMode {
    #[default]
    Ask,
    AutoReview,
    NoPrompts,
    FullAccess,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct TurnControls {
    pub mode: WorkMode,
    pub approval: ApprovalMode,
    pub network_access: bool,
}
#[derive(Clone, Debug)]
pub enum CodexRequest {
    SendPromptWithControls {
        text: String,
        model: Option<String>,
        effort: Option<String>,
        attachments: Vec<PromptAttachment>,
        skills: Vec<SkillInfo>,
        controls: TurnControls,
    },
    RefreshContext,
    SetGoal {
        objective: String,
    },
    PauseGoal,
    ClearGoal,
    SendPromptWithContext {
        text: String,
        model: Option<String>,
        effort: Option<String>,
        attachments: Vec<PromptAttachment>,
        skills: Vec<SkillInfo>,
    },
    ListModels {
        cursor: Option<String>,
    },
    SendPromptWithOptions {
        text: String,
        model: Option<String>,
        effort: Option<String>,
    },
    ListThreads {
        cursor: Option<String>,
    },
    CreateThread,
    ResumeThread(String),
    RenameProject(String),
    ThreadAction {
        thread: String,
        action: ThreadAction,
    },
    LoadHistory {
        cursor: Option<String>,
    },
    SendPrompt(String),
    Interrupt,
    AnswerApproval {
        id: Value,
        decision: ApprovalDecision,
    },
    Disconnect,
}
#[derive(Clone, Debug)]
pub struct ModelChoice {
    pub model: String,
    pub name: String,
    pub description: String,
    pub default_effort: String,
    pub efforts: Vec<(String, String)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadAction {
    Rename(String),
    Archive,
    Remove,
}
impl ThreadWorkState {
    fn from_status(status: &Value) -> Self {
        match status["type"].as_str() {
            Some("notLoaded") => Self::NotLoaded,
            Some("idle") => Self::Idle,
            Some("systemError") => Self::Error,
            Some("active") => {
                let Some(flags) = status["activeFlags"].as_array() else {
                    return Self::Unknown;
                };
                if flags.iter().any(|flag| {
                    !matches!(
                        flag.as_str(),
                        Some("waitingOnApproval" | "waitingOnUserInput")
                    )
                }) {
                    return Self::Unknown;
                }
                if flags.iter().any(|flag| flag == "waitingOnApproval") {
                    Self::WaitingOnApproval
                } else if flags.iter().any(|flag| flag == "waitingOnUserInput") {
                    Self::WaitingOnInput
                } else {
                    Self::Working
                }
            }
            _ => Self::Unknown,
        }
    }
}
const COMMAND_OUTPUT_LIMIT: usize = 64 * 1024;
#[derive(Clone, Debug)]
pub enum CodexEvent {
    Activity {
        thread: String,
        turn: String,
        activity: Activity,
    },
    TurnAccepted {
        thread: String,
        turn: String,
    },
    GoalMutationFinished {
        thread: String,
        goal: Option<GoalInfo>,
    },
    Command {
        thread: String,
        turn: String,
        command: CommandExecution,
    },
    CommandOutput {
        thread: String,
        turn: String,
        item: String,
        text: String,
        truncated: bool,
    },
    TurnRejected {
        thread: String,
        error: String,
    },
    GoalFailed {
        thread: String,
        error: String,
    },
    Goal {
        thread: String,
        goal: Option<GoalInfo>,
    },
    Plan {
        thread: String,
        steps: Vec<PlanStep>,
        text: Option<String>,
    },
    Files {
        thread: String,
        files: Vec<ContextFile>,
    },
    Skills(Vec<SkillInfo>),
    Connections(Vec<ConnectionInfo>),
    Models {
        models: Vec<ModelChoice>,
        next_cursor: Option<String>,
    },
    ThreadModel {
        thread: String,
        model: String,
        effort: Option<String>,
    },
    ThreadActionFinished {
        thread: String,
        action: ThreadAction,
    },
    ThreadActionFailed {
        thread: String,
        error: String,
    },
    ThreadStatus {
        thread: String,
        state: ThreadWorkState,
    },
    ProjectReady {
        project_id: String,
        name: String,
    },
    Threads {
        threads: Vec<ThreadSummary>,
        next_cursor: Option<String>,
    },
    History {
        thread: String,
        messages: Vec<HistoryMessage>,
        next_cursor: Option<String>,
    },
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
    /// Initializes the connection/project; legacy callers also create or resume a thread.
    /// Never starts a model turn.
    pub fn spawn(config: CodexConfig, wake: impl Fn() + Send + Sync + 'static) -> io::Result<Self> {
        if config
            .project
            .as_ref()
            .is_some_and(|project| !project.valid())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid Codex project binding",
            ));
        }
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
        validate_context_request(&request)?;
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
            CodexEvent::Activity {
                thread,
                turn,
                activity,
            } => {
                thread.len()
                    + turn.len()
                    + activity.item.len()
                    + activity.kind.len()
                    + activity.title.len()
                    + activity.status.len()
                    + activity.details.len()
                    + 128
            }
            CodexEvent::Command {
                thread,
                turn,
                command,
            } => {
                thread.len()
                    + turn.len()
                    + command.item.len()
                    + command.command.len()
                    + command.cwd.len()
                    + command.status.len()
                    + command.output.len()
                    + 128
            }
            CodexEvent::CommandOutput {
                thread,
                turn,
                item,
                text,
                ..
            } => thread.len() + turn.len() + item.len() + text.len() + 96,
            CodexEvent::GoalFailed { thread, error }
            | CodexEvent::TurnRejected { thread, error } => thread.len() + error.len() + 64,
            CodexEvent::Goal { thread, goal }
            | CodexEvent::GoalMutationFinished { thread, goal } => {
                thread.len()
                    + goal
                        .as_ref()
                        .map_or(0, |g| g.objective.len() + g.status.len() + 64)
                    + 64
            }
            CodexEvent::Plan {
                thread,
                steps,
                text,
            } => {
                thread.len()
                    + steps
                        .iter()
                        .map(|s| s.text.len() + s.status.len() + 32)
                        .sum::<usize>()
                    + text.as_ref().map_or(0, String::len)
                    + 64
            }
            CodexEvent::Files { thread, files } => {
                thread.len()
                    + files
                        .iter()
                        .map(|f| f.path.len() + f.kind.len() + 32)
                        .sum::<usize>()
                    + 64
            }
            CodexEvent::Skills(skills) => {
                skills
                    .iter()
                    .map(|s| s.name.len() + s.path.len() + s.description.len() + 64)
                    .sum::<usize>()
                    + 64
            }
            CodexEvent::Connections(items) => {
                items
                    .iter()
                    .map(|s| s.name.len() + s.auth_status.len() + 64)
                    .sum::<usize>()
                    + 64
            }
            CodexEvent::Models {
                models,
                next_cursor,
            } => {
                models
                    .iter()
                    .map(|m| {
                        m.model.len()
                            + m.name.len()
                            + m.description.len()
                            + m.default_effort.len()
                            + m.efforts
                                .iter()
                                .map(|(id, description)| id.len() + description.len() + 32)
                                .sum::<usize>()
                            + 128
                    })
                    .sum::<usize>()
                    + next_cursor.as_ref().map_or(0, String::len)
            }
            CodexEvent::ThreadModel {
                thread,
                model,
                effort,
            } => thread.len() + model.len() + effort.as_ref().map_or(0, String::len) + 64,
            CodexEvent::ThreadActionFinished { thread, action } => {
                thread.len()
                    + match action {
                        ThreadAction::Rename(name) => name.len(),
                        _ => 0,
                    }
                    + 64
            }
            CodexEvent::ThreadActionFailed { thread, error } => thread.len() + error.len() + 64,
            CodexEvent::ThreadStatus { thread, .. } => thread.len() + 64,
            CodexEvent::ProjectReady { project_id, name } => project_id.len() + name.len() + 64,
            CodexEvent::Threads {
                threads,
                next_cursor,
            } => {
                threads
                    .iter()
                    .map(|t| t.id.len() + t.title.len() + 64)
                    .sum::<usize>()
                    + next_cursor.as_ref().map_or(0, String::len)
                    + 64
            }
            CodexEvent::History {
                thread,
                messages,
                next_cursor,
            } => {
                thread.len()
                    + messages
                        .iter()
                        .map(|m| m.role.len() + m.text.len() + 64)
                        .sum::<usize>()
                    + next_cursor.as_ref().map_or(0, String::len)
                    + 64
            }
            CodexEvent::Connecting => 64,
            CodexEvent::Ready { thread } => thread.len() + 64,
            CodexEvent::TurnStarted { thread, turn }
            | CodexEvent::TurnAccepted { thread, turn } => thread.len() + turn.len() + 64,
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

#[derive(Clone, Debug)]
enum RequestKind {
    Goal {
        thread: String,
        clear: bool,
        mutation: bool,
    },
    Skills,
    Connections {
        page: usize,
        cursor: Option<String>,
    },
    Models {
        reset: bool,
    },
    Initialize,
    Project,
    RenameProject,
    ThreadAction {
        thread: String,
        action: ThreadAction,
    },
    Threads,
    History {
        thread: String,
    },
    Thread,
    Turn {
        thread: String,
    },
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
    project: Option<crate::notes::CodexProjectBinding>,
    project_ready: bool,
    known_threads: std::collections::HashSet<String>,
    models: Vec<ModelChoice>,
    skills: Vec<SkillInfo>,
    connections: Vec<ConnectionInfo>,
    active_goals: std::collections::HashSet<String>,
    command_output: HashMap<String, usize>,
    activity_items: std::collections::HashSet<String>,
}
impl Protocol {
    fn new(config: &CodexConfig) -> Self {
        Self {
            project: config.project.clone(),
            project_ready: false,
            known_threads: Default::default(),
            models: Vec::new(),
            skills: Vec::new(),
            connections: Vec::new(),
            active_goals: Default::default(),
            command_output: HashMap::new(),
            activity_items: Default::default(),
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
    fn remember_thread(&mut self, id: &str) -> Result<(), String> {
        if id.is_empty() || id.len() > THREAD_ID_LIMIT || id.chars().any(char::is_control) {
            return Err("Codex returned an invalid thread identity".into());
        }
        if self.known_threads.contains(id) {
            return Ok(());
        }
        let bytes: usize = self.known_threads.iter().map(String::len).sum();
        if self.known_threads.len() >= KNOWN_THREAD_LIMIT || bytes + id.len() > KNOWN_THREAD_BYTES {
            return Err(
                "Codex project thread identity limit reached; reconnect to refresh the list".into(),
            );
        }
        self.known_threads.insert(id.to_owned());
        Ok(())
    }
    fn project_id(&self) -> Result<&str, String> {
        if !self.project_ready {
            return Err("Codex project is not ready".into());
        }
        self.project
            .as_ref()
            .and_then(|p| p.project_id.as_deref())
            .ok_or_else(|| "Codex project is not configured".into())
    }
    fn idle(&self) -> Result<(), String> {
        if self.turn.is_some()
            || self.turn_starting
            || !self.approvals.is_empty()
            || self.pending.values().any(|p| {
                matches!(
                    p.kind,
                    RequestKind::Thread
                        | RequestKind::History { .. }
                        | RequestKind::ThreadAction { .. }
                )
            })
        {
            return Err("finish the active Codex operation before switching threads".into());
        }
        Ok(())
    }
    fn thread_request(&mut self) -> Value {
        let mut params = json!({"cwd":self.cwd,"approvalPolicy":"on-request","approvalsReviewer":"user","sandbox":"read-only","developerInstructions":THREAD_DEVELOPER_GUIDANCE});
        let method = if let Some(thread) = &self.resume {
            params["threadId"] = json!(thread);
            params["excludeTurns"] = json!(true);
            "thread/resume"
        } else {
            if let Some(project) = &self.project {
                params["projectId"] = json!(project.project_id);
            }
            "thread/start"
        };
        self.request(RequestKind::Thread, method, params)
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
        self.request(RequestKind::Initialize, "initialize", json!({"clientInfo":{"name":"tanglewood","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}))
    }
    fn command(&mut self, request: CodexRequest) -> Result<Vec<Value>, String> {
        validate_context_request(&request)?;
        if self.pending.len() >= 14 {
            return Err("Codex has too many pending RPC requests".into());
        }
        match request {
            CodexRequest::RefreshContext => {
                if self.pending.values().any(|p| {
                    matches!(
                        p.kind,
                        RequestKind::Skills | RequestKind::Connections { .. }
                    )
                }) {
                    return Err("Context refresh already pending.".into());
                }
                self.skills.clear();
                self.connections.clear();
                let mut requests = vec![
                    self.request(
                        RequestKind::Skills,
                        "skills/list",
                        json!({"cwds":[self.cwd],"forceReload":true}),
                    ),
                    self.request(
                        RequestKind::Connections {
                            page: 0,
                            cursor: None,
                        },
                        "mcpServerStatus/list",
                        json!({"limit":1,"detail":"toolsAndAuthOnly"}),
                    ),
                ];
                if let Some(thread) = self.thread.clone().filter(|thread| !self.pending.values().any(|p| matches!(&p.kind, RequestKind::Goal {thread: pending, ..} if pending == thread))) {
                    requests.push(self.request(
                        RequestKind::Goal {
                            thread: thread.clone(),
                            clear: false,
                            mutation:false,
                        },
                        "thread/goal/get",
                        json!({"threadId":thread}),
                    ));
                }
                Ok(requests)
            }
            CodexRequest::SetGoal { objective } => {
                self.goal_mutation_ready()?;
                if objective.trim().is_empty() || objective.len() > 16384 {
                    return Err("Goal must contain 1–16384 bytes.".into());
                }
                let thread = self.thread.clone().ok_or("no selected thread")?;
                self.supersede_goal_requests(&thread);
                Ok(vec![self.request(RequestKind::Goal {thread:thread.clone(),clear:false,mutation:true},"thread/goal/set",json!({"threadId":thread,"objective":objective,"status":"active","origin":"user"}))])
            }
            CodexRequest::PauseGoal | CodexRequest::ClearGoal => {
                self.goal_mutation_ready()?;
                let thread = self.thread.clone().ok_or("no selected thread")?;
                self.supersede_goal_requests(&thread);
                let clear = matches!(request, CodexRequest::ClearGoal);
                let params = if clear {
                    json!({"threadId":thread,"origin":"user"})
                } else {
                    json!({"threadId":thread,"status":"paused","origin":"user"})
                };
                Ok(vec![self.request(
                    RequestKind::Goal {
                        thread,
                        clear,
                        mutation: true,
                    },
                    if clear {
                        "thread/goal/clear"
                    } else {
                        "thread/goal/set"
                    },
                    params,
                )])
            }
            CodexRequest::ListModels { cursor } => Ok(vec![self.request(
                RequestKind::Models {
                    reset: cursor.is_none(),
                },
                "model/list",
                json!({"cursor":cursor,"limit":50,"includeHidden":false}),
            )]),
            CodexRequest::ListThreads { cursor } => {
                let id = self.project_id()?.to_owned();
                Ok(vec![self.request(RequestKind::Threads,"thread/list",json!({"projectId":id,"cursor":cursor,"limit":50,"sortKey":"updated_at","sortDirection":"desc","sourceKinds":["cli","vscode","exec","appServer","unknown"]}))])
            }
            CodexRequest::RenameProject(name) => {
                let id = self.project_id()?.to_owned();
                if name.trim().is_empty() || name.len() > 256 || name.chars().any(char::is_control)
                {
                    return Err("project name must contain 1–256 bytes".into());
                }
                Ok(vec![self.request(
                    RequestKind::RenameProject,
                    "project/update",
                    json!({"projectId":id,"name":name.trim()}),
                )])
            }
            CodexRequest::ThreadAction { thread, action } => {
                self.project_id()?;
                self.idle()?;
                if self
                    .pending
                    .values()
                    .any(|p| matches!(p.kind, RequestKind::Threads))
                {
                    return Err("wait for the thread list before changing a thread".into());
                }
                if !self.known_threads.contains(&thread) {
                    return Err("thread is not listed in this vault project".into());
                }
                let (method, params) = match &action {
                    ThreadAction::Rename(name) => {
                        if name.trim().is_empty()
                            || name.len() > 256
                            || name.chars().any(char::is_control)
                        {
                            return Err(
                                "thread name must contain 1–256 bytes without control characters"
                                    .into(),
                            );
                        }
                        (
                            "thread/name/set",
                            json!({"threadId":thread,"name":name.trim()}),
                        )
                    }
                    ThreadAction::Archive => ("thread/archive", json!({"threadId":thread})),
                    ThreadAction::Remove => ("thread/delete", json!({"threadId":thread})),
                };
                let action = match action {
                    ThreadAction::Rename(name) => ThreadAction::Rename(name.trim().to_owned()),
                    other => other,
                };
                Ok(vec![self.request(
                    RequestKind::ThreadAction { thread, action },
                    method,
                    params,
                )])
            }
            CodexRequest::CreateThread => {
                self.project_id()?;
                self.idle()?;
                self.resume = None;
                Ok(vec![self.thread_request()])
            }
            CodexRequest::ResumeThread(id) => {
                self.project_id()?;
                self.idle()?;
                if !self.known_threads.contains(&id) {
                    return Err("thread is not listed in this vault project".into());
                }
                self.resume = Some(id);
                Ok(vec![self.thread_request()])
            }
            CodexRequest::LoadHistory { cursor } => {
                self.idle()?;
                let thread = self.thread.clone().ok_or("no selected thread")?;
                Ok(vec![self.request(RequestKind::History { thread: thread.clone() },"thread/turns/list",json!({"threadId":thread,"cursor":cursor,"limit":20,"sortDirection":"desc","itemsView":"full"}))])
            }
            CodexRequest::SendPrompt(text) => self.command(CodexRequest::SendPromptWithOptions {
                text,
                model: None,
                effort: None,
            }),
            CodexRequest::SendPromptWithOptions {
                text,
                model,
                effort,
            } => self.command(CodexRequest::SendPromptWithContext {
                text,
                model,
                effort,
                attachments: Vec::new(),
                skills: Vec::new(),
            }),
            CodexRequest::SendPromptWithControls {
                text,
                model,
                effort,
                attachments,
                skills,
                controls,
            } => {
                let selected = model
                    .as_ref()
                    .and_then(|id| self.models.iter().find(|m| &m.model == id))
                    .ok_or("Choose an available model before using composer modes.")?;
                let effective_effort = effort
                    .clone()
                    .unwrap_or_else(|| selected.default_effort.clone());
                if !selected
                    .efforts
                    .iter()
                    .any(|(id, _)| id == &effective_effort)
                {
                    return Err("Reasoning effort is not supported by this model.".into());
                }
                if controls.mode == WorkMode::Goal
                    && !self
                        .thread
                        .as_ref()
                        .is_some_and(|thread| self.active_goals.contains(thread))
                {
                    return Err(
                        "Set or resume the selected thread's goal before sending in Goal mode."
                            .into(),
                    );
                }
                let instructions = if controls.mode == WorkMode::Research {
                    Some(
                        "Research the user's topic: gather, compare, and cite sources; use attached source snapshots as context. Inspect Research List and ReadIndex for current workspace/source records rather than guessing from chat history or treating sources as a specially named note. Use authorized Research commands for workspace/source updates. Create research notes through the tanglewood MCP tools when useful, subject to their permissions and approvals. Use a topic subfolder under Research/<topic>/ by default, or the destination requested by the user; create Summary.md and Sources.md when useful through the normal CreateNote path command, without direct filesystem access. Preserve existing authored source unless the user requests changes. Do not implement software unless requested. Never write vault files directly or bypass the tanglewood MCP command boundary.",
                    )
                } else {
                    None
                };
                let collaboration = json!({"mode":if controls.mode == WorkMode::Plan {"plan"}else{"default"},"settings":{"model":selected.model,"reasoning_effort":effective_effort,"developer_instructions":instructions}});
                let mut writes = self.command(CodexRequest::SendPromptWithContext {
                    text,
                    model,
                    effort: Some(effective_effort),
                    attachments,
                    skills,
                })?;
                for write in &mut writes {
                    write["params"]["collaborationMode"] = collaboration.clone();
                    write["params"]["approvalPolicy"] = json!(if matches!(
                        controls.approval,
                        ApprovalMode::NoPrompts | ApprovalMode::FullAccess
                    ) {
                        "never"
                    } else {
                        "on-request"
                    });
                    write["params"]["sandboxPolicy"] =
                        if controls.approval == ApprovalMode::FullAccess {
                            json!({"type":"dangerFullAccess"})
                        } else {
                            json!({"type":"readOnly","networkAccess":controls.network_access})
                        };
                    write["params"]["approvalsReviewer"] =
                        json!(if controls.approval == ApprovalMode::AutoReview {
                            "auto_review"
                        } else {
                            "user"
                        });
                }
                Ok(writes)
            }
            CodexRequest::SendPromptWithContext {
                text,
                model,
                effort,
                attachments,
                skills,
            } => {
                if attachments.len() > 8 || skills.len() > 8 {
                    return Err("At most eight attachments and skills are allowed.".into());
                }
                let bytes = attachments
                    .iter()
                    .try_fold(text.len(), |n, a| {
                        n.checked_add(a.name.len() + a.text.len() + 64)
                    })
                    .ok_or("context payload too large")?;
                if bytes > PROMPT_LIMIT {
                    return Err("prompt and attachments exceed 256 KiB".into());
                }
                for skill in &skills {
                    if !self
                        .skills
                        .iter()
                        .any(|s| s.enabled && s.name == skill.name && s.path == skill.path)
                    {
                        return Err(
                            "Skill is not enabled in the current catalog; refresh context.".into(),
                        );
                    }
                }
                let mut input = vec![json!({"type":"text","text":text})];
                for a in attachments {
                    input.push(json!({"type":"text","text":format!("Attached source snapshot: {}\n{}",a.name,a.text)}));
                }
                for s in skills {
                    input.push(json!({"type":"skill","name":s.name,"path":s.path}));
                }

                if let Some(id) = &model {
                    let choice = self
                        .models
                        .iter()
                        .find(|m| &m.model == id)
                        .ok_or("Model is not in the current Codex catalog; refresh models.")?;
                    if effort
                        .as_ref()
                        .is_some_and(|effort| !choice.efforts.iter().any(|(id, _)| id == effort))
                    {
                        return Err("Reasoning effort is not supported by this model.".into());
                    }
                } else if effort.is_some() {
                    return Err("Select a model before choosing reasoning effort.".into());
                }

                if text.len() > PROMPT_LIMIT {
                    return Err("prompt exceeds 256 KiB".into());
                }
                let thread = self.thread.clone().ok_or("Codex is not ready")?;
                if self.turn.is_some()
                    || self.turn_starting
                    || self.pending.values().any(|p| {
                        matches!(
                            p.kind,
                            RequestKind::Thread
                                | RequestKind::History { .. }
                                | RequestKind::ThreadAction { .. }
                        )
                    })
                {
                    return Err("a Codex turn is already running".into());
                }
                self.command_output.clear();
                self.activity_items.clear();
                self.turn_starting = true;
                self.turn_deadline = Some(Instant::now() + TURN_TIMEOUT);
                let mut params = json!({"threadId":thread,"input":input,"approvalPolicy":"on-request","approvalsReviewer":"user","sandboxPolicy":{"type":"readOnly","networkAccess":false}});
                if let Some(model) = model {
                    params["model"] = json!(model);
                }
                if let Some(effort) = effort {
                    params["effort"] = json!(effort);
                }
                Ok(vec![self.request(
                    RequestKind::Turn { thread },
                    "turn/start",
                    params,
                )])
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
            if matches!(
                method,
                "thread/archived" | "thread/deleted" | "thread/name/updated"
            ) {
                if self.pending.values().any(|p|matches!(&p.kind,RequestKind::ThreadAction {thread:target,..} if target==thread)) {
                    // Notifications may precede RPC acknowledgement.
                    return Ok((writes,events));
                }
                if self.known_threads.contains(thread) {
                    let action = match method {
                        "thread/archived" => ThreadAction::Archive,
                        "thread/deleted" => ThreadAction::Remove,
                        _ => ThreadAction::Rename(
                            params["threadName"]
                                .as_str()
                                .filter(|n| !n.is_empty())
                                .unwrap_or("New thread")
                                .chars()
                                .take(256)
                                .collect(),
                        ),
                    };
                    if !matches!(action, ThreadAction::Rename(_)) {
                        self.known_threads.remove(thread);
                        self.active_goals.remove(thread);
                        if self.thread.as_deref() == Some(thread) {
                            self.thread = None;
                            self.resume = None;
                            self.turn = None;
                            self.turn_starting = false;
                            self.turn_deadline = None;
                            self.cancel_pending = false;
                            self.approvals.clear();
                        }
                    }
                    events.push(CodexEvent::ThreadActionFinished {
                        thread: thread.into(),
                        action,
                    });
                }
                return Ok((writes, events));
            }
            if method == "thread/status/changed" {
                if self.known_threads.contains(thread) || self.thread.as_deref() == Some(thread) {
                    events.push(CodexEvent::ThreadStatus {
                        thread: thread.into(),
                        state: ThreadWorkState::from_status(&params["status"]),
                    });
                }
                return Ok((writes, events));
            }
            if self.known_threads.contains(thread) || self.thread.as_deref() == Some(thread) {
                match method {
                    "thread/goal/updated" | "thread/goal/cleared" => {
                        self.pending.retain(|_,p|!matches!(&p.kind,RequestKind::Goal {thread:pending,mutation:false,..} if pending==thread));
                        let goal = if method == "thread/goal/cleared" {
                            None
                        } else {
                            parse_goal(&params["goal"])
                        };
                        self.record_goal(thread, goal.as_ref());
                        events.push(CodexEvent::Goal {
                            thread: thread.into(),
                            goal: if method == "thread/goal/cleared" {
                                None
                            } else {
                                parse_goal(&params["goal"])
                            },
                        });
                    }
                    "turn/plan/updated" => events.push(CodexEvent::Plan {
                        thread: thread.into(),
                        steps: params["plan"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .take(64)
                            .map(|s| PlanStep {
                                text: bounded(&s["step"], 4096),
                                status: bounded(&s["status"], 64),
                            })
                            .collect(),
                        text: params["explanation"]
                            .as_str()
                            .map(|s| s.chars().take(16384).collect()),
                    }),
                    "item/completed" => context_item(thread, &params["item"], &mut events),
                    _ => {}
                }
            }
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
                && (self.turn_starting || self.active_goals.contains(thread))
                && self.turn.is_none()
                && !turn.is_empty()
                && turn.len() <= THREAD_ID_LIMIT
            {
                self.command_output.clear();
                self.activity_items.clear();
                self.turn_starting = true;
                self.turn_deadline = Some(Instant::now() + TURN_TIMEOUT);
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
                "item/started" | "item/completed"
                    if matches!(
                        params["item"]["type"].as_str(),
                        Some("mcpToolCall" | "webSearch" | "fileChange")
                    ) =>
                {
                    if let Some(activity) =
                        parse_activity(&params["item"], method == "item/completed")
                    {
                        if self.activity_items.contains(&activity.item)
                            || self.activity_items.len() < 32
                        {
                            self.activity_items.insert(activity.item.clone());
                            events.push(CodexEvent::Activity {
                                thread: thread.into(),
                                turn: turn.into(),
                                activity,
                            });
                        }
                    }
                }
                "item/started" | "item/completed"
                    if params["item"]["type"] == "commandExecution" =>
                {
                    if let Some(command) = parse_command(&params["item"]) {
                        if self.command_output.contains_key(&command.item)
                            || self.command_output.len() < 32
                        {
                            self.command_output
                                .insert(command.item.clone(), command.output.len());
                            events.push(CodexEvent::Command {
                                thread: thread.into(),
                                turn: turn.into(),
                                command,
                            });
                        }
                    }
                }
                "item/commandExecution/outputDelta" => {
                    if let Some(item) = params["itemId"].as_str() {
                        if let Some(used) = self.command_output.get_mut(item) {
                            let delta = params["delta"].as_str().unwrap_or_default();
                            let was_truncated = *used > COMMAND_OUTPUT_LIMIT;
                            let remaining = COMMAND_OUTPUT_LIMIT.saturating_sub(*used);
                            let text = byte_prefix(delta, remaining).to_owned();
                            let truncated = text.len() < delta.len();
                            *used = if truncated {
                                COMMAND_OUTPUT_LIMIT + 1
                            } else {
                                *used + text.len()
                            };
                            if !text.is_empty() || (truncated && !was_truncated) {
                                events.push(CodexEvent::CommandOutput {
                                    thread: thread.into(),
                                    turn: turn.into(),
                                    item: item.into(),
                                    text,
                                    truncated,
                                });
                            }
                        }
                    }
                }
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
                    self.command_output.clear();
                    self.activity_items.clear();
                    if let Some(error) = params["turn"].get("error").filter(|v| !v.is_null()) {
                        events.push(CodexEvent::Error(error_text(error)));
                    }
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
                        .filter(|value| !value.is_null())
                        .map(error_text)
                        .unwrap_or_else(|| "Codex turn error".into()),
                )),
                _ => {}
            }
        } else if let Some(id) = frame.get("id").and_then(Value::as_u64) {
            let Some(pending) = self.pending.remove(&id) else {
                return Ok((writes, events));
            };
            if let Some(error) = frame.get("error") {
                let error = error_text(error);
                if matches!(
                    pending.kind,
                    RequestKind::Initialize | RequestKind::Thread | RequestKind::Project
                ) {
                    return Err(format!("Codex setup rejected: {error}"));
                }
                if matches!(pending.kind, RequestKind::Turn { .. }) {
                    self.turn_starting = false;
                    if self.turn.is_none() {
                        self.turn_deadline = None;
                        self.cancel_pending = false;
                        self.approvals.clear();
                    }
                }
                if let RequestKind::Turn { thread } = pending.kind {
                    events.push(CodexEvent::TurnRejected {
                        thread,
                        error: format!("Codex turn rejected: {error}"),
                    });
                } else if let RequestKind::Goal { thread, .. } = pending.kind {
                    events.push(CodexEvent::GoalFailed {
                        thread,
                        error: format!("Codex goal request rejected: {error}"),
                    });
                } else if let RequestKind::ThreadAction { thread, .. } = pending.kind {
                    events.push(CodexEvent::ThreadActionFailed {
                        thread,
                        error: format!("Codex request rejected: {error}"),
                    });
                } else {
                    events.push(CodexEvent::Error(format!(
                        "Codex request rejected: {error}"
                    )));
                }
                return Ok((writes, events));
            }
            let result = frame.get("result").ok_or("Codex response missing result")?;
            match pending.kind {
                RequestKind::Goal {
                    thread,
                    clear,
                    mutation,
                } => {
                    let goal = if clear {
                        None
                    } else {
                        parse_goal(&result["goal"])
                    };
                    self.record_goal(&thread, goal.as_ref());
                    if mutation {
                        writes.push(self.request(
                            RequestKind::Goal {
                                thread: thread.clone(),
                                clear: false,
                                mutation: false,
                            },
                            "thread/goal/get",
                            json!({"threadId":thread}),
                        ));
                        events.push(CodexEvent::GoalMutationFinished { thread, goal });
                    } else {
                        events.push(CodexEvent::Goal { thread, goal });
                    }
                }
                RequestKind::Skills => {
                    self.skills = result["data"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|e| e["cwd"].as_str() == Some(self.cwd.as_str()))
                        .flat_map(|e| e["skills"].as_array().into_iter().flatten())
                        .filter(|s| {
                            ["name", "path"].iter().all(|key| {
                                s[*key].as_str().is_some_and(|v| {
                                    !v.is_empty()
                                        && v.len() <= if *key == "name" { 256 } else { 4096 }
                                        && !v.chars().any(char::is_control)
                                })
                            })
                        })
                        .take(128)
                        .map(|s| SkillInfo {
                            name: bounded(&s["name"], 256),
                            path: bounded(&s["path"], 4096),
                            description: bounded(&s["description"], 4096),
                            enabled: s["enabled"].as_bool().unwrap_or(false),
                        })
                        .collect();
                    events.push(CodexEvent::Skills(self.skills.clone()));
                }
                RequestKind::Connections { page, cursor } => {
                    let data = result["data"].as_array();
                    if data.is_none_or(|items| items.len() > 1) {
                        events.push(CodexEvent::Error(
                            "Codex connection inventory exceeded its one-server page limit.".into(),
                        ));
                    } else {
                        for server in data.into_iter().flatten() {
                            if self.connections.len() < 128 {
                                self.connections.push(ConnectionInfo {
                                    name: bounded(&server["name"], 256),
                                    auth_status: bounded(&server["authStatus"], 128),
                                    tools: server["tools"]
                                        .as_object()
                                        .map_or(0, |tools| tools.len()),
                                });
                            }
                        }
                        events.push(CodexEvent::Connections(self.connections.clone()));
                        if let Some(next) = result["nextCursor"].as_str() {
                            if next.len() > 4096 || cursor.as_deref() == Some(next) {
                                events.push(CodexEvent::Error(
                                    "Codex connection inventory returned an invalid cursor.".into(),
                                ));
                            } else if page < 127 && self.connections.len() < 128 {
                                writes.push(self.request(
                                    RequestKind::Connections {
                                        page: page + 1,
                                        cursor: Some(next.into()),
                                    },
                                    "mcpServerStatus/list",
                                    json!({"cursor":next,"limit":1,"detail":"toolsAndAuthOnly"}),
                                ));
                            }
                        }
                    }
                }
                RequestKind::Models { reset } => {
                    let data = result["data"].as_array().ok_or("missing model catalog")?;
                    if data.len() > 50 {
                        return Err("model page exceeded limit".into());
                    }
                    let mut models = Vec::new();
                    for model in data {
                        if model["hidden"].as_bool() == Some(true) {
                            continue;
                        }
                        let id = field(model, "model")?;
                        if id.len() > 256 || id.chars().any(char::is_control) {
                            return Err("invalid model identity".into());
                        }
                        let options = model["supportedReasoningEfforts"]
                            .as_array()
                            .ok_or("missing model reasoning choices")?;
                        if options.len() > 16 {
                            return Err("too many reasoning effort choices".into());
                        }
                        let mut efforts = Vec::new();
                        for option in options {
                            let effort = field(option, "reasoningEffort")?;
                            if effort.len() > 64 || effort.chars().any(char::is_control) {
                                return Err("invalid reasoning effort".into());
                            }
                            efforts.push((
                                effort,
                                option["description"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .chars()
                                    .take(1024)
                                    .collect(),
                            ));
                        }
                        let default_effort = field(model, "defaultReasoningEffort")?;
                        if default_effort.len() > 64 {
                            return Err("invalid default reasoning effort".into());
                        }
                        models.push(ModelChoice {
                            model: id,
                            name: model["displayName"]
                                .as_str()
                                .unwrap_or("Model")
                                .chars()
                                .take(256)
                                .collect(),
                            description: model["description"]
                                .as_str()
                                .unwrap_or_default()
                                .chars()
                                .take(2048)
                                .collect(),
                            default_effort,
                            efforts,
                        });
                    }
                    if !reset && self.models.len() + models.len() > 128 {
                        return Err("model catalog limit reached".into());
                    }
                    if reset {
                        self.models.clear();
                    }
                    for model in &models {
                        if let Some(old) = self.models.iter_mut().find(|m| m.model == model.model) {
                            *old = model.clone();
                        } else {
                            self.models.push(model.clone());
                        }
                    }
                    events.push(CodexEvent::Models {
                        models,
                        next_cursor: result["nextCursor"].as_str().map(str::to_owned),
                    });
                }
                RequestKind::Initialize => {
                    writes.push(json!({"method":"initialized","params":{}}));
                    if let Some(project) = self.project.clone() {
                        let (method, params) = if let Some(id) = project.project_id {
                            ("project/read", json!({"projectId":id}))
                        } else {
                            (
                                "project/create",
                                json!({"idempotencyKey":project.creation_key,"name":project.name,"roots":[{"path":self.cwd}]}),
                            )
                        };
                        writes.push(self.request(RequestKind::Project, method, params));
                    } else {
                        writes.push(self.thread_request());
                    }
                }
                RequestKind::Project | RequestKind::RenameProject => {
                    let project = &result["project"];
                    let id = field(project, "id")?;
                    if self
                        .project
                        .as_ref()
                        .and_then(|p| p.project_id.as_ref())
                        .is_some_and(|expected| expected != &id)
                    {
                        return Err("Codex returned an unexpected project".into());
                    }
                    let name = field(project, "name")?;
                    let binding = self.project.as_mut().ok_or("unexpected project response")?;
                    binding.project_id = Some(id.clone());
                    binding.name = name.clone();
                    self.project_ready = true;
                    events.push(CodexEvent::ProjectReady {
                        project_id: id,
                        name,
                    });
                }
                RequestKind::ThreadAction { thread, action } => {
                    if !matches!(action, ThreadAction::Rename(_)) {
                        self.known_threads.remove(&thread);
                        self.active_goals.remove(&thread);
                        if self.thread.as_deref() == Some(&thread) {
                            self.thread = None;
                            self.resume = None;
                            self.turn = None;
                            self.turn_starting = false;
                            self.turn_deadline = None;
                            self.cancel_pending = false;
                            self.approvals.clear();
                        }
                    }
                    events.push(CodexEvent::ThreadActionFinished { thread, action });
                }
                RequestKind::Threads => {
                    let id = self.project_id()?.to_owned();
                    let data = result["data"].as_array().ok_or("missing thread list")?;
                    if data.len() > 50 {
                        return Err("thread page exceeded limit".into());
                    }
                    let mut threads = Vec::new();
                    for t in data {
                        if t["projectId"].as_str() != Some(&id) {
                            return Err("thread list crossed project boundary".into());
                        }
                        let tid = field(t, "id")?;
                        self.remember_thread(&tid)?;
                        threads.push(ThreadSummary {
                            state: ThreadWorkState::from_status(&t["status"]),
                            id: tid,
                            title: t["name"]
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .or_else(|| t["preview"].as_str())
                                .unwrap_or("New thread")
                                .chars()
                                .take(256)
                                .collect(),
                            updated_at: t["updatedAt"].as_i64().unwrap_or(0),
                        });
                    }
                    events.push(CodexEvent::Threads {
                        threads,
                        next_cursor: result["nextCursor"].as_str().map(str::to_owned),
                    });
                }
                RequestKind::History { thread } => {
                    let data = result["data"].as_array().ok_or("missing history page")?;
                    if data.len() > 20 {
                        return Err("history page exceeded limit".into());
                    }
                    let mut messages = Vec::new();
                    for turn in data.iter().rev() {
                        for item in turn["items"].as_array().into_iter().flatten() {
                            if events.len() < 128 {
                                context_item(&thread, item, &mut events);
                            }
                            match item["type"].as_str() {
                                Some("agentMessage") => messages.push(HistoryMessage {
                                    role: "Codex".into(),
                                    text: field(item, "text")?,
                                }),
                                Some("userMessage") => {
                                    let text = item["content"]
                                        .as_array()
                                        .into_iter()
                                        .flatten()
                                        .filter(|c| c["type"] == "text")
                                        .filter_map(|c| c["text"].as_str())
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    messages.push(HistoryMessage {
                                        role: "You".into(),
                                        text,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    events.push(CodexEvent::History {
                        thread,
                        messages,
                        next_cursor: result["nextCursor"].as_str().map(str::to_owned),
                    });
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
                    if let Some(project) = &self.project {
                        if result["thread"]["projectId"].as_str() != project.project_id.as_deref() {
                            return Err("Codex resumed a thread outside this vault project".into());
                        }
                    }
                    self.remember_thread(&thread)?;
                    self.thread = Some(thread.clone());
                    events.push(CodexEvent::Ready {
                        thread: thread.clone(),
                    });
                    if let Some(model) = result["model"].as_str() {
                        if model.len() > 256 {
                            return Err("thread model identity exceeded limit".into());
                        }
                        events.push(CodexEvent::ThreadModel {
                            thread,
                            model: model.into(),
                            effort: result["reasoningEffort"]
                                .as_str()
                                .map(|s| s.chars().take(64).collect()),
                        });
                    }
                }
                RequestKind::Turn { thread } => {
                    let turn = field(&result["turn"], "id")?;
                    if turn.is_empty()
                        || turn.len() > THREAD_ID_LIMIT
                        || self.turn.as_ref().is_some_and(|active| active != &turn)
                    {
                        self.turn_starting = false;
                        events.push(CodexEvent::TurnRejected {thread,error:"Codex acknowledged a different turn while another turn is active; submission outcome is uncertain and draft context was retained. Refresh or reconnect before retrying.".into()});
                    } else {
                        if self.turn.is_none() && self.turn_starting {
                            self.turn = Some(turn.clone());
                            events.push(CodexEvent::TurnStarted {
                                thread: thread.clone(),
                                turn: turn.clone(),
                            });
                        }
                        self.turn_starting = false;
                        events.push(CodexEvent::TurnAccepted { thread, turn });
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
    fn record_goal(&mut self, thread: &str, goal: Option<&GoalInfo>) {
        if goal.is_some_and(|g| g.status == "active") {
            if self.active_goals.len() < KNOWN_THREAD_LIMIT {
                self.active_goals.insert(thread.to_owned());
            }
        } else {
            self.active_goals.remove(thread);
        }
    }
    // Retiring IDs makes late replies unobservable without retaining a per-thread
    // generation map. The already bounded pending map is the correlation owner.
    fn supersede_goal_requests(&mut self, thread: &str) {
        self.pending.retain(
            |_, p| !matches!(&p.kind, RequestKind::Goal {thread: pending, ..} if pending == thread),
        );
    }
    fn goal_mutation_ready(&self) -> Result<(), String> {
        if self.pending.values().any(|p| {
            matches!(
                p.kind,
                RequestKind::Thread
                    | RequestKind::History { .. }
                    | RequestKind::ThreadAction { .. }
            )
        }) {
            return Err("Wait for the selected thread operation before changing its goal.".into());
        }
        Ok(())
    }
    fn check_timeouts(&mut self, now: Instant) -> Result<Vec<CodexEvent>, String> {
        let mut events = Vec::new();
        self.pending.retain(|_, p| {
            let expired = now >= p.deadline
                && matches!(
                    p.kind,
                    RequestKind::Goal { .. }
                        | RequestKind::Skills
                        | RequestKind::Connections { .. }
                );
            if expired {
                events.push(match &p.kind {
                    RequestKind::Goal {thread,..} => CodexEvent::GoalFailed {thread:thread.clone(),error:"Codex goal request timed out; the change may have an unknown outcome. Refresh context before retrying.".into()},
                    _ => CodexEvent::Error("Codex context request timed out. Refresh context to retry.".into()),
                });
            }
            !expired
        });
        if self.pending.values().any(|pending| now >= pending.deadline) {
            return Err("Codex RPC timed out; outcome may be unknown".into());
        }
        if self.turn_deadline.is_some_and(|deadline| now >= deadline) {
            return Err("Codex turn exceeded 10 minute limit".into());
        }
        Ok(events)
    }
}
fn byte_prefix(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn error_text(error: &Value) -> String {
    let mut text = error
        .as_str()
        .or_else(|| error.get("message").and_then(Value::as_str))
        .filter(|text| !text.is_empty())
        .map_or_else(|| error.to_string(), str::to_owned);
    if let Some(code) = error.get("code").and_then(Value::as_i64) {
        text.push_str(&format!(" (code {code})"));
    }
    for key in ["additionalDetails", "data", "codexErrorInfo"] {
        if let Some(details) = error.get(key).filter(|value| !value.is_null()) {
            let details = details
                .as_str()
                .map_or_else(|| details.to_string(), str::to_owned);
            if !details.is_empty() && details != text {
                text.push('\n');
                text.push_str(&details);
            }
        }
    }
    byte_prefix(&text, 4096).into()
}

/// Only decode our versioned domain envelope after an explicit tool failure.
fn tool_error_text(server: &str, text: &str) -> (String, bool) {
    if server == "tanglewood" && text.len() <= 8192 {
        if let Ok(response) = serde_json::from_str::<crate::mcp::ResponseEnvelope>(text) {
            if response.version == crate::mcp::PROTOCOL_VERSION {
                match response.outcome {
                    crate::mcp::Outcome::Error(error) => return (error.to_string(), false),
                    crate::mcp::Outcome::Rejected(reason) => {
                        return (format!("Request rejected: {reason}"), false);
                    }
                    crate::mcp::Outcome::UnknownOutcome(reason) => {
                        return (format!("Outcome unknown: {reason}"), true);
                    }
                    _ => {}
                }
            }
        }
    }
    (byte_prefix(text, 4096).into(), false)
}

fn parse_activity(item: &Value, completed: bool) -> Option<Activity> {
    let kind = item["type"].as_str()?;
    let id = item["id"].as_str()?;
    if id.is_empty() || id.len() > 1024 {
        return None;
    }
    let (mut title, details) = match kind {
        "mcpToolCall" => (
            format!(
                "{}.{}",
                bounded(&item["server"], 256),
                bounded(&item["tool"], 256)
            ),
            json!({"arguments":item["arguments"],"result":item["result"],"error":item["error"]})
                .to_string(),
        ),
        "webSearch" => (
            format!("Search: {}", bounded(&item["query"], 512)),
            json!({"query":item["query"],"action":item["action"],"results":item["results"]})
                .to_string(),
        ),
        "fileChange" => (
            "File changes".into(),
            json!({"changes":item["changes"]}).to_string(),
        ),
        _ => return None,
    };
    let mut status =
        item["status"]
            .as_str()
            .unwrap_or(if completed { "completed" } else { "inProgress" });
    let error = item.get("error").filter(|value| !value.is_null());
    if error.is_some()
        || item.pointer("/result/isError").and_then(Value::as_bool) == Some(true)
        || status == "failed"
    {
        status = "failed";
        let message = error.map(error_text).or_else(|| {
            item.pointer("/result/content")
                .and_then(Value::as_array)?
                .iter()
                .filter(|content| content["type"] == "text")
                .filter_map(|content| content["text"].as_str())
                .find(|text| !text.trim().is_empty())
                .map(str::to_owned)
        });
        if let Some(message) = message {
            let (message, uncertain) =
                tool_error_text(item["server"].as_str().unwrap_or_default(), &message);
            if uncertain {
                status = "unknownOutcome";
            }
            title.push_str(" — ");
            title.push_str(&message);
        }
    }
    Some(Activity {
        item: id.into(),
        kind: kind.into(),
        title: byte_prefix(&title, 512).into(),
        status: byte_prefix(status, 64).into(),
        details: byte_prefix(&details, 8192).into(),
        truncated: details.len() > 8192,
    })
}
fn parse_command(item: &Value) -> Option<CommandExecution> {
    if item["type"] != "commandExecution" {
        return None;
    }
    let id = item["id"].as_str()?;
    if id.is_empty() || id.len() > 1024 {
        return None;
    }
    let output = item["aggregatedOutput"].as_str().unwrap_or_default();
    Some(CommandExecution {
        item: id.into(),
        command: bounded(&item["command"], 16384),
        cwd: bounded(&item["cwd"], 4096),
        status: bounded(&item["status"], 64),
        exit_code: item["exitCode"].as_i64(),
        output: byte_prefix(output, COMMAND_OUTPUT_LIMIT).into(),
        truncated: output.len() > COMMAND_OUTPUT_LIMIT,
        output_complete: item["aggregatedOutput"].is_string(),
    })
}
fn validate_context_request(request: &CodexRequest) -> Result<(), String> {
    match request {
        CodexRequest::SendPromptWithContext {
            text,
            attachments,
            skills,
            ..
        }
        | CodexRequest::SendPromptWithControls {
            text,
            attachments,
            skills,
            ..
        } => {
            if attachments.len() > 8 || skills.len() > 8 {
                return Err("At most eight attachments and skills are allowed.".into());
            }
            let total = text
                .len()
                .saturating_add(
                    attachments
                        .iter()
                        .map(|a| a.name.len().saturating_add(a.text.len()).saturating_add(64))
                        .sum::<usize>(),
                )
                .saturating_add(
                    skills
                        .iter()
                        .map(|s| s.name.len() + s.path.len() + s.description.len() + 64)
                        .sum::<usize>(),
                );
            if total > PROMPT_LIMIT {
                return Err("prompt and context exceed 256 KiB".into());
            }
        }
        CodexRequest::SendPromptWithOptions { text, .. } if text.len() > PROMPT_LIMIT => {
            return Err("prompt exceeds 256 KiB".into());
        }
        _ => {}
    }
    Ok(())
}
fn bounded(value: &Value, limit: usize) -> String {
    value
        .as_str()
        .unwrap_or_default()
        .chars()
        .take(limit)
        .collect()
}
fn parse_goal(value: &Value) -> Option<GoalInfo> {
    if !value.is_object() {
        return None;
    }
    Some(GoalInfo {
        objective: bounded(&value["objective"], 16384),
        status: bounded(&value["status"], 64),
        tokens_used: value["tokensUsed"].as_u64().unwrap_or(0),
        time_used_seconds: value["timeUsedSeconds"].as_u64().unwrap_or(0),
        token_budget: value["tokenBudget"].as_u64(),
    })
}
fn context_item(thread: &str, item: &Value, events: &mut Vec<CodexEvent>) {
    match item["type"].as_str() {
        Some("plan") => events.push(CodexEvent::Plan {
            thread: thread.into(),
            steps: Vec::new(),
            text: Some(bounded(&item["text"], 32768)),
        }),
        Some("fileChange") => events.push(CodexEvent::Files {
            thread: thread.into(),
            files: item["changes"]
                .as_array()
                .into_iter()
                .flatten()
                .take(128)
                .map(|f| ContextFile {
                    path: bounded(&f["path"], 4096),
                    kind: "file".into(),
                })
                .collect(),
        }),
        Some("imageGeneration") if item["savedPath"].is_string() => {
            events.push(CodexEvent::Files {
                thread: thread.into(),
                files: vec![ContextFile {
                    path: bounded(&item["savedPath"], 4096),
                    kind: "image".into(),
                }],
            })
        }
        _ => {}
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
            .args(child_server_args(&mcp))
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
                Ok(request) => {
                    let goal_thread = matches!(
                        &request,
                        CodexRequest::SetGoal { .. }
                            | CodexRequest::PauseGoal
                            | CodexRequest::ClearGoal
                    )
                    .then(|| protocol.thread.clone().unwrap_or_default());
                    let turn_thread = matches!(
                        &request,
                        CodexRequest::SendPrompt(_)
                            | CodexRequest::SendPromptWithOptions { .. }
                            | CodexRequest::SendPromptWithContext { .. }
                            | CodexRequest::SendPromptWithControls { .. }
                    )
                    .then(|| protocol.thread.clone().unwrap_or_default());
                    match protocol.command(request) {
                        Ok(frames) => {
                            for frame in frames {
                                process.enqueue(frame)?;
                            }
                        }
                        Err(error) => sink.emit(match goal_thread {
                            Some(thread) => CodexEvent::GoalFailed { thread, error },
                            None => match turn_thread {
                                Some(thread) => CodexEvent::TurnRejected { thread, error },
                                None => CodexEvent::Error(error),
                            },
                        })?,
                    }
                }
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
        for event in protocol.check_timeouts(Instant::now())? {
            sink.emit(event)?;
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_plan_tool_and_explicit_approval_modes_have_real_fields() {
        let args = child_server_args("mcp_servers={}");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-c", "tools.update_plan.enabled=true"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-c", "sandbox_mode=\"read-only\""])
        );
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        assert_eq!(
            p.initialize()["params"]["capabilities"]["experimentalApi"],
            true
        );
        p.pending.clear();
        let start = p.thread_request();
        let guidance = start["params"]["developerInstructions"].as_str().unwrap();
        assert!(
            guidance.contains("built-in update_plan")
                && guidance.contains("authorized note and research mutations")
                && guidance.contains("read for queries")
                && guidance.contains("Research ReadIndex")
                && guidance.contains("Do not infer the current workspace")
        );
        assert_eq!(start["params"]["sandbox"], "read-only");
        p.pending.clear();
        p.thread = Some("a".into());
        p.models.push(ModelChoice {
            model: "model-a".into(),
            name: "A".into(),
            description: String::new(),
            default_effort: "medium".into(),
            efforts: vec![("medium".into(), String::new())],
        });
        for (approval, policy, reviewer, sandbox) in [
            (ApprovalMode::Ask, "on-request", "user", "readOnly"),
            (
                ApprovalMode::AutoReview,
                "on-request",
                "auto_review",
                "readOnly",
            ),
            (ApprovalMode::NoPrompts, "never", "user", "readOnly"),
            (
                ApprovalMode::FullAccess,
                "never",
                "user",
                "dangerFullAccess",
            ),
        ] {
            let writes = p
                .command(CodexRequest::SendPromptWithControls {
                    text: "test".into(),
                    model: Some("model-a".into()),
                    effort: None,
                    attachments: vec![],
                    skills: vec![],
                    controls: TurnControls {
                        mode: WorkMode::Work,
                        approval,
                        network_access: false,
                    },
                })
                .unwrap();
            assert_eq!(writes[0]["params"]["approvalPolicy"], policy);
            assert_eq!(writes[0]["params"]["approvalsReviewer"], reviewer);
            assert_eq!(writes[0]["params"]["sandboxPolicy"]["type"], sandbox);
            if sandbox == "readOnly" {
                assert_eq!(writes[0]["params"]["sandboxPolicy"]["networkAccess"], false);
            }
            p.pending.clear();
            p.turn_starting = false;
        }
    }
    #[test]
    fn activities_are_scoped_bounded_and_keep_lifecycle() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("a".into());
        p.turn = Some("turn".into());
        let item = json!({"type":"mcpToolCall","id":"tool","server":"tanglewood","tool":"create_note","status":"inProgress","arguments":{"path":"Research/Topic/Summary.md"}});
        let (_,events)=p.frame(json!({"method":"item/started","params":{"threadId":"a","turnId":"other","item":item}})).unwrap();
        assert!(events.is_empty());
        let (_,events)=p.frame(json!({"method":"item/started","params":{"threadId":"a","turnId":"turn","item":item}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::Activity {activity,..} if activity.kind=="mcpToolCall" && activity.status=="inProgress" && activity.details.contains("Summary.md"))
        );
        let (_,events)=p.frame(json!({"method":"item/completed","params":{"threadId":"a","turnId":"turn","item":{"type":"mcpToolCall","id":"tool","server":"tanglewood","tool":"create_note","status":"completed","result":{"text":"é".repeat(16384)}}}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::Activity {activity,..} if activity.status=="completed" && activity.truncated && activity.details.len()<=8192)
        );
        for (kind, id) in [("webSearch", "search"), ("fileChange", "files")] {
            let (_,events)=p.frame(json!({"method":"item/started","params":{"threadId":"a","turnId":"turn","item":{"type":kind,"id":id,"query":"topic","changes":[]}}})).unwrap();
            assert!(
                matches!(&events[0],CodexEvent::Activity {activity,..} if activity.kind==kind && activity.status=="inProgress")
            );
        }
    }
    #[test]
    fn structured_errors_keep_their_meaning() {
        // Shapes captured from the morning Tanglewood session; identities removed.
        let message = "invalid command arguments: missing field `include_trash`";
        let uncertain = "accepted but storage failed: downloaded content is not a PDF; inspect state before retry";
        for (result, expected, status) in [
            (
                json!({"isError":true,"content":[{"type":"text","text":message}]}),
                message.to_owned(),
                "failed",
            ),
            (
                json!({"isError":true,"content":[{"type":"text","text":json!({"version":1,"request_id":"00000000-0000-0000-0000-000000000001","outcome":{"Error":{"InvalidResearch":"only local text and PDF imports are supported"}}}).to_string()}]}),
                "invalid research request: only local text and PDF imports are supported".into(),
                "failed",
            ),
            (
                json!({"isError":true,"content":[{"type":"text","text":json!({"version":1,"request_id":"00000000-0000-0000-0000-000000000001","outcome":{"UnknownOutcome":uncertain}}).to_string()}]}),
                format!("Outcome unknown: {uncertain}"),
                "unknownOutcome",
            ),
        ] {
            let item = json!({"type":"mcpToolCall","id":"tool","server":"tanglewood","tool":"execute","status":"failed","result":result});
            let activity = parse_activity(&item, true).unwrap();
            assert_eq!(activity.status, status);
            assert!(activity.title.contains(&expected), "{}", activity.title);
            let details: Value = serde_json::from_str(&activity.details).unwrap();
            assert_eq!(details["result"], item["result"]);
        }
        let item = json!({"type":"mcpToolCall","id":"tool","server":"other","tool":"read","status":"completed","error":{"message":"Permission denied"}});
        let activity = parse_activity(&item, true).unwrap();
        assert_eq!(activity.status, "failed");
        assert!(activity.title.contains("Permission denied"));
        let item = json!({"type":"mcpToolCall","id":"tool","server":"tanglewood","tool":"read","status":"completed","result":{"isError":false,"content":[{"type":"text","text":"An example containing the word error"}]}});
        let activity = parse_activity(&item, true).unwrap();
        assert_eq!(activity.status, "completed");
        assert_eq!(activity.title, "tanglewood.read");
        let item = json!({"type":"mcpToolCall","id":"tool","server":"other","tool":"read","status":"failed","error":{"message":"é".repeat(10000)}});
        let activity = parse_activity(&item, true).unwrap();
        assert!(activity.title.len() <= 512 && activity.details.len() <= 8192);
        assert!(activity.truncated);

        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut protocol = Protocol::new(&config);
        protocol.thread = Some("a".into());
        protocol.turn = Some("turn".into());
        for error in [
            json!("Connection interrupted"),
            json!({"message":"Connection interrupted","additionalDetails":"Reconnect when ready."}),
        ] {
            let (_, events) = protocol.frame(json!({"method":"error","params":{"threadId":"a","turnId":"turn","willRetry":false,"error":error}})).unwrap();
            let expected = if error.is_string() {
                "Connection interrupted"
            } else {
                "Connection interrupted\nReconnect when ready."
            };
            assert!(matches!(&events[..], [CodexEvent::Error(text)] if text == expected));
        }
        let (_, events) = protocol.frame(json!({"method":"error","params":{"threadId":"other","turnId":"turn","willRetry":false,"error":{"message":"Unrelated"}}})).unwrap();
        assert!(events.is_empty());
        let (_, events) = protocol.frame(json!({"method":"turn/completed","params":{"threadId":"a","turn":{"id":"turn","status":"failed","error":{"message":"Download failed","additionalDetails":"Inspect state before retrying."}}}})).unwrap();
        assert!(
            matches!(&events[..], [CodexEvent::Error(text), CodexEvent::TurnCompleted {status,..}] if text == "Download failed\nInspect state before retrying." && status == "failed")
        );
        assert!(protocol.turn.is_none());
    }
    #[test]
    fn client_rejection_does_not_cancel_independent_goal_turn() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("a".into());
        p.active_goals.insert("a".into());
        let send = p
            .command(CodexRequest::SendPrompt("context".into()))
            .unwrap()[0]
            .clone();
        let (_,events)=p.frame(json!({"method":"turn/started","params":{"threadId":"a","turn":{"id":"automatic"}}})).unwrap();
        assert!(matches!(&events[0], CodexEvent::TurnStarted { .. }));
        let deadline = p.turn_deadline;
        let (_, events) = p
            .frame(json!({"id":send["id"],"error":{"message":"busy"}}))
            .unwrap();
        assert!(matches!(&events[0], CodexEvent::TurnRejected { .. }));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, CodexEvent::TurnAccepted { .. }))
        );
        assert_eq!(p.turn.as_deref(), Some("automatic"));
        assert_eq!(p.turn_deadline, deadline);
        let conflicting = p.request(
            RequestKind::Turn { thread: "a".into() },
            "turn/start",
            json!({}),
        );
        let (_, events) = p
            .frame(json!({"id":conflicting["id"],"result":{"turn":{"id":"different"}}}))
            .unwrap();
        assert!(matches!(&events[0], CodexEvent::TurnRejected { .. }));
        assert_eq!(p.turn.as_deref(), Some("automatic"));
        p.turn = None;
        p.pending.clear();
        let send = p
            .command(CodexRequest::SendPrompt("context".into()))
            .unwrap()[0]
            .clone();
        let (_, events) = p
            .frame(json!({"id":send["id"],"result":{"turn":{"id":"own"}}}))
            .unwrap();
        assert!(events.iter().any(
            |e| matches!(e,CodexEvent::TurnAccepted {thread,turn} if thread=="a" && turn=="own")
        ));
    }
    #[test]
    fn confirmed_goal_turns_can_start_without_client_turn_request() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("a".into());
        let start = |thread| json!({"method":"turn/started","params":{"threadId":thread,"turn":{"id":"goal-turn"}}});
        assert!(p.frame(start("a")).unwrap().1.is_empty());
        p.frame(json!({"method":"thread/goal/updated","params":{"threadId":"a","goal":{"objective":"Goal","status":"active"}}})).unwrap();
        assert!(p.frame(start("foreign")).unwrap().1.is_empty());
        let (_, events) = p.frame(start("a")).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::TurnStarted {thread,turn} if thread=="a" && turn=="goal-turn")
        );
        assert!(p.turn_deadline.is_some());
        let (_,events)=p.frame(json!({"method":"item/agentMessage/delta","params":{"threadId":"a","turnId":"goal-turn","itemId":"message","delta":"Working"}})).unwrap();
        assert!(matches!(&events[0],CodexEvent::Delta {text,..} if text=="Working"));
        let (_,events)=p.frame(json!({"id":"approval","method":"item/commandExecution/requestApproval","params":{"threadId":"a","turnId":"goal-turn","availableDecisions":["decline"],"command":"example"}})).unwrap();
        assert!(matches!(&events[0],CodexEvent::Approval {thread,..} if thread=="a"));
    }
    #[test]
    fn pause_ack_wins_earlier_usage_notification_then_refreshes() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("a".into());
        let pause = p.command(CodexRequest::PauseGoal).unwrap()[0].clone();
        p.frame(json!({"method":"thread/goal/updated","params":{"threadId":"a","goal":{"objective":"Goal","status":"active","tokensUsed":5}}})).unwrap();
        assert!(p.pending.contains_key(&pause["id"].as_u64().unwrap()));
        let (writes,events)=p.frame(json!({"id":pause["id"],"result":{"goal":{"objective":"Goal","status":"paused","tokensUsed":5}}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::GoalMutationFinished {goal:Some(goal),..} if goal.status=="paused")
        );
        assert_eq!(writes[0]["method"], "thread/goal/get");
        assert_eq!(writes[0]["params"]["threadId"], "a");
        assert!(!p.active_goals.contains("a"));
    }
    #[test]
    fn command_events_follow_active_turn_and_bound_output() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("a".into());
        p.turn = Some("turn".into());
        let item = json!({"type":"commandExecution","id":"cmd","command":"echo test","cwd":"/tmp","status":"inProgress"});
        let (_,events)=p.frame(json!({"method":"item/started","params":{"threadId":"a","turnId":"turn","item":item}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::Command {command,..} if command.command=="echo test")
        );
        let delta = "é".repeat(COMMAND_OUTPUT_LIMIT);
        let (_,events)=p.frame(json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"a","turnId":"other","itemId":"cmd","delta":delta}})).unwrap();
        assert!(events.is_empty());
        let (_,events)=p.frame(json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"a","turnId":"turn","itemId":"cmd","delta":delta}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::CommandOutput {text,truncated:true,..} if text.len()==COMMAND_OUTPUT_LIMIT)
        );
        let (_,events)=p.frame(json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"a","turnId":"turn","itemId":"cmd","delta":"more"}})).unwrap();
        assert!(events.is_empty());
        let (_,events)=p.frame(json!({"method":"item/completed","params":{"threadId":"a","turnId":"turn","item":{"type":"commandExecution","id":"cmd","command":"echo test","cwd":"/tmp","status":"completed","exitCode":0,"aggregatedOutput":"test"}}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::Command {command,..} if command.exit_code==Some(0) && command.output=="test" && command.status=="completed")
        );
        let parsed=parse_command(&json!({"type":"commandExecution","id":"cmd","aggregatedOutput":null,"status":"completed","exitCode":0})).unwrap();
        assert!(!parsed.output_complete);
        let parsed=parse_command(&json!({"type":"commandExecution","id":"cmd","aggregatedOutput":"","status":"completed"})).unwrap();
        assert!(parsed.output_complete);
    }
    #[test]
    fn composer_controls_use_real_modes_without_weakening_sandbox() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("a".into());
        p.models.push(ModelChoice {
            model: "model-a".into(),
            name: "A".into(),
            description: String::new(),
            default_effort: "medium".into(),
            efforts: vec![("medium".into(), String::new())],
        });
        let request = |mode, approval| CodexRequest::SendPromptWithControls {
            text: "test".into(),
            model: Some("model-a".into()),
            effort: None,
            attachments: vec![],
            skills: vec![],
            controls: TurnControls {
                mode,
                approval,
                network_access: false,
            },
        };
        assert!(
            p.command(request(WorkMode::Goal, ApprovalMode::Ask))
                .is_err()
        );
        let plan = p
            .command(request(WorkMode::Plan, ApprovalMode::AutoReview))
            .unwrap();
        assert_eq!(plan[0]["params"]["collaborationMode"]["mode"], "plan");
        assert_eq!(
            plan[0]["params"]["collaborationMode"]["settings"]["reasoning_effort"],
            "medium"
        );
        assert_eq!(plan[0]["params"]["approvalsReviewer"], "auto_review");
        assert_eq!(plan[0]["params"]["approvalPolicy"], "on-request");
        assert_eq!(
            plan[0]["params"]["sandboxPolicy"],
            json!({"type":"readOnly","networkAccess":false})
        );
        p.pending.clear();
        p.turn_starting = false;
        let work = p
            .command(request(WorkMode::Work, ApprovalMode::Ask))
            .unwrap();
        assert_eq!(work[0]["params"]["collaborationMode"]["mode"], "default");
        assert_eq!(work[0]["params"]["approvalsReviewer"], "user");
        assert_eq!(work[0]["params"]["sandboxPolicy"]["networkAccess"], false);
        p.pending.clear();
        p.turn_starting = false;
        let mut research = request(WorkMode::Research, ApprovalMode::Ask);
        if let CodexRequest::SendPromptWithControls { controls, .. } = &mut research {
            controls.network_access = true;
        }
        let research = p.command(research).unwrap();
        assert_eq!(
            research[0]["params"]["collaborationMode"]["mode"],
            "default"
        );
        let instructions =
            research[0]["params"]["collaborationMode"]["settings"]["developer_instructions"]
                .as_str()
                .unwrap();
        assert!(instructions.contains("Create research notes through the tanglewood MCP"));
        assert!(instructions.contains("Research/<topic>/"));
        assert!(
            work[0]["params"]["collaborationMode"]["settings"]["developer_instructions"].is_null()
        );
        assert!(
            plan[0]["params"]["collaborationMode"]["settings"]["developer_instructions"].is_null()
        );
        assert_eq!(
            research[0]["params"]["sandboxPolicy"],
            json!({"type":"readOnly","networkAccess":true})
        );
        assert_eq!(research[0]["params"]["approvalPolicy"], "on-request");
        assert_eq!(research[0]["params"]["approvalsReviewer"], "user");
        p.pending.clear();
        p.turn_starting = false;
        let work = p
            .command(request(WorkMode::Work, ApprovalMode::Ask))
            .unwrap();
        assert_eq!(work[0]["params"]["sandboxPolicy"]["networkAccess"], false);

        p.pending.clear();
        p.turn_starting = false;
        let legacy = p
            .command(CodexRequest::SendPrompt("legacy".into()))
            .unwrap();
        assert!(legacy[0]["params"].get("collaborationMode").is_none());
        assert_eq!(legacy[0]["params"]["approvalsReviewer"], "user");
        let goal = parse_goal(&json!({"objective":"test","status":"active","timeUsedSeconds":42}))
            .unwrap();
        assert_eq!(goal.time_used_seconds, 42);
        p.pending.clear();
        p.turn_starting = false;
        p.record_goal("a", Some(&goal));
        assert!(
            p.command(request(WorkMode::Goal, ApprovalMode::Ask))
                .is_ok()
        );
    }
    #[test]
    fn turn_rpc_rejection_preserves_thread_correlation() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("a".into());
        let send = p.command(CodexRequest::SendPrompt("test".into())).unwrap();
        let (_, events) = p
            .frame(json!({"id":send[0]["id"],"error":{"message":"rejected"}}))
            .unwrap();
        assert!(matches!(&events[0],CodexEvent::TurnRejected {thread,..} if thread=="a"));
        assert!(!p.turn_starting);
    }
    #[test]
    fn connection_inventory_pages_one_server_and_bounds_cursors() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        let requests = p.command(CodexRequest::RefreshContext).unwrap();
        assert_eq!(requests[1]["params"]["limit"], 1);
        assert_eq!(requests[1]["params"]["detail"], "toolsAndAuthOnly");
        let (writes,events)=p.frame(json!({"id":requests[1]["id"],"result":{"data":[{"name":"first","authStatus":"oAuth","tools":{"one":{}}}],"nextCursor":"next"}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::Connections(items) if items.len()==1 && items[0].tools==1)
        );
        assert_eq!(writes[0]["params"]["limit"], 1);
        let (writes,events)=p.frame(json!({"id":writes[0]["id"],"result":{"data":[{"name":"second","authStatus":"unsupported","tools":{}}],"nextCursor":"next"}})).unwrap();
        assert!(writes.is_empty());
        assert!(matches!(&events[0],CodexEvent::Connections(items) if items.len()==2));
        assert!(matches!(&events[1], CodexEvent::Error(_)));
    }
    #[test]
    fn newer_goal_actions_and_notifications_supersede_old_reads() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("a".into());
        let read = p.command(CodexRequest::RefreshContext).unwrap()[2].clone();
        let mutation = p
            .command(CodexRequest::SetGoal {
                objective: "New goal".into(),
            })
            .unwrap()[0]
            .clone();
        let (_, events) = p
            .frame(json!({"id":read["id"],"result":{"goal":null}}))
            .unwrap();
        assert!(events.is_empty());
        let (_,events)=p.frame(json!({"id":mutation["id"],"result":{"goal":{"objective":"New goal","status":"active"}}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::GoalMutationFinished {goal:Some(goal),..} if goal.objective=="New goal")
        );
        p.pending.clear();
        let read = p.command(CodexRequest::RefreshContext).unwrap()[2].clone();
        let (_,events)=p.frame(json!({"method":"thread/goal/updated","params":{"threadId":"a","goal":{"objective":"Live goal","status":"active"}}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::Goal {goal:Some(goal),..} if goal.objective=="Live goal")
        );
        let (_, events) = p
            .frame(json!({"id":read["id"],"result":{"goal":null}}))
            .unwrap();
        assert!(events.is_empty());
        p.pending.clear();
        let mutation = p.command(CodexRequest::PauseGoal).unwrap()[0].clone();
        let (_,events)=p.frame(json!({"method":"thread/goal/updated","params":{"threadId":"a","goal":{"objective":"Live goal","status":"active"}}})).unwrap();
        assert!(matches!(&events[0], CodexEvent::Goal { .. }));
        assert!(p.pending.contains_key(&mutation["id"].as_u64().unwrap()));
        let (_, events) = p
            .frame(json!({"id":mutation["id"],"error":{"message":"unsupported"}}))
            .unwrap();
        assert!(matches!(&events[0],CodexEvent::GoalFailed {thread,..} if thread=="a"));
        p.command(CodexRequest::PauseGoal).unwrap();
        let events = p
            .check_timeouts(Instant::now() + RPC_TIMEOUT + Duration::from_secs(1))
            .unwrap();
        assert!(matches!(&events[0],CodexEvent::GoalFailed {thread,..} if thread=="a"));
    }
    #[test]
    fn context_is_correlated_bounded_and_skill_allowlisted() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("a".into());
        let req = p.command(CodexRequest::RefreshContext).unwrap();
        assert!(req.iter().all(|r| r["method"] != "turn/start"));
        let (_,events)=p.frame(json!({"id":req[0]["id"],"result":{"data":[{"cwd":"/tmp","skills":[{"name":"test","path":"/tmp/SKILL.md","description":"test","enabled":true}]}]}})).unwrap();
        let CodexEvent::Skills(skills) = &events[0] else {
            panic!("missing catalog")
        };
        let skills = skills.clone();
        p.thread = Some("b".into());
        let (_,events)=p.frame(json!({"id":req[2]["id"],"result":{"goal":{"objective":"old goal","status":"paused"}}})).unwrap();
        assert!(matches!(&events[0],CodexEvent::Goal {thread,..} if thread=="a"));
        let goal = p
            .command(CodexRequest::SetGoal {
                objective: "explicit".into(),
            })
            .unwrap();
        assert_eq!(goal[0]["params"]["origin"], "user");
        let send = p
            .command(CodexRequest::SendPromptWithContext {
                text: "do it".into(),
                model: None,
                effort: None,
                attachments: vec![PromptAttachment {
                    name: "note.md".into(),
                    text: "snapshot".into(),
                }],
                skills: skills.clone(),
            })
            .unwrap();
        assert_eq!(send[0]["params"]["input"][2]["type"], "skill");
        assert!(
            send[0]["params"]["input"][1]["text"]
                .as_str()
                .unwrap()
                .contains("snapshot")
        );
        p.turn_starting = false;
        p.pending.clear();
        let mut forged = skills;
        forged[0].path = "/other".into();
        assert!(
            p.command(CodexRequest::SendPromptWithContext {
                text: "x".into(),
                model: None,
                effort: None,
                attachments: vec![],
                skills: forged
            })
            .is_err()
        );
        let (_,events)=p.frame(json!({"method":"thread/goal/updated","params":{"threadId":"foreign","goal":{"objective":"no"}}})).unwrap();
        assert!(events.is_empty());
        let optional = p.command(CodexRequest::RefreshContext).unwrap();
        assert!(
            p.frame(json!({"id":optional[0]["id"],"error":{"message":"unsupported"}}))
                .is_ok()
        );
        assert!(
            p.check_timeouts(Instant::now() + RPC_TIMEOUT + Duration::from_secs(1))
                .is_ok()
        );
        let history = p
            .command(CodexRequest::LoadHistory { cursor: None })
            .unwrap();
        assert!(p.command(CodexRequest::PauseGoal).is_err());
        p.thread = Some("c".into());
        let (_,events)=p.frame(json!({"id":history[0]["id"],"result":{"data":[{"items":[{"type":"fileChange","changes":[{"path":"/tmp/result.svg"}]},{"type":"plan","text":"A plan"}]}]}})).unwrap();
        assert!(events.iter().all(|event|matches!(event,CodexEvent::Files {thread,..}|CodexEvent::Plan {thread,..}|CodexEvent::History {thread,..} if thread=="b")));
        assert!(
            validate_context_request(&CodexRequest::SendPromptWithContext {
                text: String::new(),
                model: None,
                effort: None,
                attachments: vec![PromptAttachment {
                    name: "large".into(),
                    text: "x".repeat(PROMPT_LIMIT)
                }],
                skills: vec![]
            })
            .is_err()
        );
    }
    #[test]
    fn model_catalog_limits_effort_choices_and_turn_overrides() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        };
        let mut p = Protocol::new(&config);
        p.thread = Some("thread-a".into());
        let list = p
            .command(CodexRequest::ListModels { cursor: None })
            .unwrap();
        assert_eq!(list[0]["method"], "model/list");
        let (_,events)=p.frame(json!({"id":list[0]["id"],"result":{"data":[{"id":"menu-id","model":"model-a","displayName":"Model A","description":"Test model","defaultReasoningEffort":"medium","supportedReasoningEfforts":[{"reasoningEffort":"low","description":"Quick"},{"reasoningEffort":"medium","description":"Balanced"}]}],"nextCursor":null}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::Models {models,..} if models[0].model=="model-a" && models[0].efforts.len()==2)
        );
        assert!(
            p.command(CodexRequest::SendPromptWithOptions {
                text: "x".into(),
                model: Some("model-a".into()),
                effort: Some("ultra".into())
            })
            .is_err()
        );
        assert!(
            p.command(CodexRequest::SendPromptWithOptions {
                text: "x".into(),
                model: Some("foreign".into()),
                effort: None
            })
            .is_err()
        );
        let send = p
            .command(CodexRequest::SendPromptWithOptions {
                text: "hello".into(),
                model: Some("model-a".into()),
                effort: Some("low".into()),
            })
            .unwrap();
        assert_eq!(send[0]["params"]["model"], "model-a");
        assert_eq!(send[0]["params"]["effort"], "low");
        assert_eq!(send[0]["params"]["sandboxPolicy"]["type"], "readOnly");
        let mut legacy = Protocol::new(&config);
        legacy.thread = Some("thread-a".into());
        let send = legacy
            .command(CodexRequest::SendPrompt("default".into()))
            .unwrap();
        assert!(send[0]["params"].get("model").is_none());
        assert!(send[0]["params"].get("effort").is_none());
    }
    #[test]
    fn thread_actions_are_project_scoped_and_acknowledged_before_context_changes() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/app.sock".into(),
            resume_thread: None,
            project: Some(crate::notes::CodexProjectBinding {
                project_id: Some("project-a".into()),
                name: "Vault".into(),
                creation_key: "key".into(),
            }),
        };
        let mut p = Protocol::new(&config);
        p.project_ready = true;
        p.remember_thread("thread-a").unwrap();
        p.thread = Some("thread-a".into());
        assert!(
            p.command(CodexRequest::ThreadAction {
                thread: "foreign".into(),
                action: ThreadAction::Remove
            })
            .is_err()
        );
        p.turn = Some("running".into());
        assert!(
            p.command(CodexRequest::ThreadAction {
                thread: "thread-a".into(),
                action: ThreadAction::Archive
            })
            .is_err()
        );
        p.turn = None;
        let rename = p
            .command(CodexRequest::ThreadAction {
                thread: "thread-a".into(),
                action: ThreadAction::Rename("Renamed".into()),
            })
            .unwrap();
        assert_eq!(rename[0]["method"], "thread/name/set");
        assert_eq!(
            rename[0]["params"],
            json!({"threadId":"thread-a","name":"Renamed"})
        );
        assert!(
            p.command(CodexRequest::SendPrompt("must wait".into()))
                .is_err()
        );
        let (_, early)=p.frame(json!({"method":"thread/name/updated","params":{"threadId":"thread-a","threadName":"Renamed"}})).unwrap();
        assert!(early.is_empty());
        let (_, events) = p.frame(json!({"id":rename[0]["id"],"result":{}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::ThreadActionFinished {thread,action:ThreadAction::Rename(name)} if thread=="thread-a" && name=="Renamed")
        );
        let archive = p
            .command(CodexRequest::ThreadAction {
                thread: "thread-a".into(),
                action: ThreadAction::Archive,
            })
            .unwrap();
        assert_eq!(archive[0]["method"], "thread/archive");
        assert_eq!(p.thread.as_deref(), Some("thread-a"));
        let (_, events) = p
            .frame(json!({"id":archive[0]["id"],"error":{"message":"save failed"}}))
            .unwrap();
        assert!(
            matches!(&events[0],CodexEvent::ThreadActionFailed {thread,..} if thread=="thread-a")
        );
        assert_eq!(p.thread.as_deref(), Some("thread-a"));
        let remove = p
            .command(CodexRequest::ThreadAction {
                thread: "thread-a".into(),
                action: ThreadAction::Remove,
            })
            .unwrap();
        assert_eq!(remove[0]["method"], "thread/delete");
        let (_, events) = p.frame(json!({"id":remove[0]["id"],"result":{}})).unwrap();
        assert!(matches!(
            &events[0],
            CodexEvent::ThreadActionFinished {
                action: ThreadAction::Remove,
                ..
            }
        ));
        assert!(p.thread.is_none());
        assert!(!p.known_threads.contains("thread-a"));
        assert!(
            p.command(CodexRequest::SendPrompt("no context".into()))
                .is_err()
        );
        p.remember_thread("external").unwrap();
        p.thread = Some("external".into());
        p.turn = Some("active".into());
        p.turn_starting = true;
        p.turn_deadline = Some(Instant::now());
        let (_, events) = p
            .frame(json!({"method":"thread/deleted","params":{"threadId":"external"}}))
            .unwrap();
        assert!(matches!(
            &events[0],
            CodexEvent::ThreadActionFinished {
                action: ThreadAction::Remove,
                ..
            }
        ));
        assert!(p.thread.is_none());
        assert!(p.turn.is_none());
        assert!(!p.turn_starting);
        assert!(p.turn_deadline.is_none());
    }
    #[test]
    fn project_identity_listing_and_history_do_not_start_model_turns() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/app.sock".into(),
            resume_thread: None,
            project: Some(crate::notes::CodexProjectBinding {
                project_id: None,
                name: "Vault".into(),
                creation_key: "stable-key".into(),
            }),
        };
        let mut p = Protocol::new(&config);
        assert_eq!(
            p.initialize()["params"]["capabilities"]["experimentalApi"],
            true
        );
        let (writes, _) = p.frame(json!({"id":1,"result":{}})).unwrap();
        assert_eq!(writes[1]["method"], "project/create");
        assert_eq!(writes[1]["params"]["idempotencyKey"], "stable-key");
        let (writes, events) = p
            .frame(json!({"id":2,"result":{"project":{"id":"project-a","name":"Vault"}}}))
            .unwrap();
        assert!(writes.is_empty());
        assert!(
            matches!(&events[0],CodexEvent::ProjectReady {project_id,..} if project_id=="project-a")
        );
        let list = p
            .command(CodexRequest::ListThreads { cursor: None })
            .unwrap();
        assert_eq!(list[0]["params"]["projectId"], "project-a");
        assert!(
            list[0]["params"]["sourceKinds"]
                .as_array()
                .unwrap()
                .contains(&json!("appServer"))
        );
        p.frame(json!({"id":3,"result":{"data":[{"id":"thread-a","projectId":"project-a","preview":"First","updatedAt":1}],"nextCursor":"next"}})).unwrap();
        assert!(
            p.command(CodexRequest::ResumeThread("foreign".into()))
                .is_err()
        );
        let resume = p
            .command(CodexRequest::ResumeThread("thread-a".into()))
            .unwrap();
        assert_eq!(resume[0]["method"], "thread/resume");
        assert!(p.command(CodexRequest::CreateThread).is_err());
        p.frame(json!({"id":4,"result":{"thread":{"id":"thread-a","projectId":"project-a"},"approvalPolicy":"on-request","approvalsReviewer":"user","sandbox":{"type":"readOnly"},"cwd":"/tmp"}})).unwrap();
        p.remember_thread("thread-other").unwrap();
        let (_, events) = p.frame(json!({"method":"thread/status/changed","params":{"threadId":"thread-other","status":{"type":"active","activeFlags":["waitingOnApproval"]}}})).unwrap();
        assert!(
            matches!(&events[0], CodexEvent::ThreadStatus {thread, state:ThreadWorkState::WaitingOnApproval} if thread=="thread-other")
        );
        let (_, events) = p.frame(json!({"method":"thread/status/changed","params":{"threadId":"foreign","status":{"type":"idle"}}})).unwrap();
        assert!(events.is_empty());
        assert_eq!(
            ThreadWorkState::from_status(&json!({"type":"notLoaded"})),
            ThreadWorkState::NotLoaded
        );
        assert_eq!(
            ThreadWorkState::from_status(&json!({"type":"active","activeFlags":[]})),
            ThreadWorkState::Working
        );
        assert_eq!(
            ThreadWorkState::from_status(&json!({"type":"active","activeFlags":["future"]})),
            ThreadWorkState::Unknown
        );
        assert_eq!(
            ThreadWorkState::from_status(&Value::Null),
            ThreadWorkState::Unknown
        );
        let history = p
            .command(CodexRequest::LoadHistory { cursor: None })
            .unwrap();
        assert_eq!(history[0]["method"], "thread/turns/list");
        let (_,events)=p.frame(json!({"id":5,"result":{"data":[{"items":[{"type":"userMessage","content":[{"type":"text","text":"hello"}]},{"type":"agentMessage","text":"reply"}]}],"nextCursor":null}})).unwrap();
        assert!(
            matches!(&events[0],CodexEvent::History {messages,..} if messages.len()==2 && messages[0].text=="hello")
        );
        let rename = p
            .command(CodexRequest::RenameProject("Renamed".into()))
            .unwrap();
        assert_eq!(rename[0]["params"]["projectId"], "project-a");
        p.frame(json!({"id":6,"result":{"project":{"id":"project-a","name":"Renamed"}}}))
            .unwrap();
        let new = p.command(CodexRequest::CreateThread).unwrap();
        assert_eq!(new[0]["params"]["projectId"], "project-a");
        // Reconnection reads the persisted identity, never creates a replacement.
        let mut reconnect = config;
        reconnect.project.as_mut().unwrap().project_id = Some("project-a".into());
        let mut p = Protocol::new(&reconnect);
        p.initialize();
        let (writes, _) = p.frame(json!({"id":1,"result":{}})).unwrap();
        assert_eq!(writes[1]["method"], "project/read");
        assert!(
            p.frame(json!({"id":2,"result":{"project":{"id":"wrong","name":"Other"}}}))
                .is_err()
        );
        // Retain known identities while rejecting count, byte and individual-ID overflow.
        for index in 0..KNOWN_THREAD_LIMIT {
            p.remember_thread(&format!("thread-{index}")).unwrap();
        }
        assert!(p.remember_thread("one-too-many").is_err());
        assert_eq!(p.known_threads.len(), KNOWN_THREAD_LIMIT);
        assert!(p.known_threads.contains("thread-0"));
        p.known_threads.clear();
        for index in 0..256 {
            p.remember_thread(&format!("{index:04}{}", "x".repeat(1020)))
                .unwrap();
        }
        assert!(p.remember_thread("another").is_err());
        assert_eq!(p.known_threads.len(), 256);
        assert!(p.remember_thread(&"x".repeat(THREAD_ID_LIMIT + 1)).is_err());
    }
    #[test]
    fn protocol_orders_initialization_correlates_turns_and_requires_explicit_approval() {
        let config = CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/app.sock".into(),
            resume_thread: None,
            project: None,
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
