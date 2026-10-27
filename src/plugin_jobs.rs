//! Application-owned plugin runs and bounded requests back to the owner.
use crate::{
    commands::{Caller, Command, CommandError, CommandResult},
    plugin_manifest::{Kind, Manifest},
    plugin_permissions::{Origin, Rule},
    plugin_registry::{PluginBinding, PluginRegistry},
    wasm_commands::{self, host},
    wasm_extensions::{BridgeReceipt, CallContext, CommandBridge, Invocation, Job, Runtime},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

pub(crate) const MAX_ACTIVE_RUNS: usize = 4;
const MAX_RUN_RECORDS: usize = 32;
const MAX_REQUESTS: usize = 16;
pub(crate) const MAX_RETAINED_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub enum PluginCommand {
    List,
    Describe {
        id: String,
    },
    Run {
        id: String,
        input: String,
    },
    Evaluate {
        id: String,
        source: String,
        input: String,
    },
    Status {
        run_id: Uuid,
    },
    Cancel {
        run_id: Uuid,
    },
}
impl PluginCommand {
    pub fn is_query(&self) -> bool {
        matches!(
            self,
            Self::List | Self::Describe { .. } | Self::Status { .. }
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub version: String,
    pub kind: Kind,
    pub languages: Vec<String>,
    pub generation: u64,
    pub effective_rules: Vec<Rule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RunPhase {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunStatus {
    pub run_id: Uuid,
    pub plugin_id: String,
    pub script_sha256: Option<String>,
    pub phase: RunPhase,
    pub output: Option<String>,
    pub error: Option<String>,
    pub diagnostics: Vec<String>,
    /// Accepted effects survive guest failure; inspect these before retrying.
    pub receipts: Vec<EffectReceipt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectReceipt {
    pub command: String,
    pub note_id: Option<Uuid>,
    pub revision: Option<Uuid>,
    pub request_id: Option<Uuid>,
    pub completion: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PluginResult {
    Plugins(Vec<PluginInfo>),
    Info(PluginInfo),
    Started { run_id: Uuid },
    Status(RunStatus),
    CancelRequested { run_id: Uuid },
}

pub(crate) enum Operation {
    Execute(Command),
    ReadResult {
        token: String,
        path: Vec<host::PathElement>,
        offset: u64,
        limit: u32,
    },
    Capabilities,
}
pub(crate) enum Reply {
    Receipt(Result<BridgeReceipt, host::CommandError>),
    Node(Result<host::ResultNode, host::CommandError>),
    Capabilities(Result<Vec<host::Capability>, host::CommandError>),
}
pub(crate) struct Request {
    pub run_id: Uuid,
    pub context: CallContext,
    pub operation: Operation,
    pub reply: SyncSender<Reply>,
}

struct Bridge {
    run_id: Uuid,
    send: SyncSender<Request>,
}
impl Bridge {
    fn request(
        &self,
        operation: Operation,
        context: &CallContext,
    ) -> Result<Reply, host::CommandError> {
        if context.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(host::CommandError::Cancelled);
        }
        let (reply, receive) = mpsc::sync_channel(1);
        self.send
            .try_send(Request {
                run_id: self.run_id,
                context: context.clone(),
                operation,
                reply,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => host::CommandError::Busy,
                TrySendError::Disconnected(_) => host::CommandError::Expired,
            })?;
        loop {
            if context.cancelled.load(std::sync::atomic::Ordering::Acquire) {
                return Err(host::CommandError::Cancelled);
            }
            let remaining = context
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(host::CommandError::Timeout)?;
            match receive.recv_timeout(remaining.min(Duration::from_millis(5))) {
                Ok(reply) => return Ok(reply),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(host::CommandError::Expired);
                }
            }
        }
    }
}
impl CommandBridge for Bridge {
    fn execute(
        &self,
        command: Command,
        context: &CallContext,
    ) -> Result<BridgeReceipt, host::CommandError> {
        match self.request(Operation::Execute(command), context)? {
            Reply::Receipt(result) => result,
            _ => Err(host::CommandError::Expired),
        }
    }
    fn read_result(
        &self,
        token: String,
        path: Vec<host::PathElement>,
        offset: u64,
        limit: u32,
        context: &CallContext,
    ) -> Result<host::ResultNode, host::CommandError> {
        match self.request(
            Operation::ReadResult {
                token,
                path,
                offset,
                limit,
            },
            context,
        )? {
            Reply::Node(result) => result,
            _ => Err(host::CommandError::Expired),
        }
    }
    fn capabilities(
        &self,
        context: &CallContext,
    ) -> Result<Vec<host::Capability>, host::CommandError> {
        match self.request(Operation::Capabilities, context)? {
            Reply::Capabilities(result) => result,
            _ => Err(host::CommandError::Expired),
        }
    }
}

pub(crate) struct PendingReceipt {
    pub request: Request,
    pub command: Command,
    pub result: CommandResult,
    pub effect_index: Option<usize>,
}
pub(crate) struct SavedResult {
    pub command: Command,
    pub cursor: Option<wasm_commands::QueryCursor>,
    pub retained: Option<wasm_commands::RetainedResult>,
}
pub(crate) struct Run {
    pub binding: PluginBinding,
    pub job: Option<Job>,
    pub status: RunStatus,
    pub results: HashMap<String, SavedResult>,
    pub downstream_jobs: u32,
    pub permission_epoch: u64,
}

pub(crate) struct PluginJobs {
    pub registry: Option<PluginRegistry>,
    pub registry_root: Option<PathBuf>,
    pub registry_guard: Option<Arc<std::fs::File>>,
    pub runs: HashMap<Uuid, Run>,
    pub order: VecDeque<Uuid>,
    pub pending: Vec<PendingReceipt>,
    pub vault_id: Uuid,
    pub vault_generation: u64,
    pub permission_epoch: u64,
    pub asset_guards: Vec<(
        String,
        Origin,
        std::sync::Weak<std::sync::atomic::AtomicBool>,
    )>,
    runtimes: HashMap<String, Runtime>,
    send: SyncSender<Request>,
    pub receive: Receiver<Request>,
}
impl Default for PluginJobs {
    fn default() -> Self {
        let (send, receive) = mpsc::sync_channel(MAX_REQUESTS);
        Self {
            registry: None,
            registry_root: None,
            registry_guard: None,
            runs: HashMap::new(),
            order: VecDeque::new(),
            pending: Vec::new(),
            vault_id: Uuid::new_v4(),
            vault_generation: 0,
            permission_epoch: 0,
            asset_guards: Vec::new(),
            runtimes: HashMap::new(),
            send,
            receive,
        }
    }
}

pub(crate) fn origin(caller: Caller) -> Origin {
    match caller {
        Caller::Local => Origin::Local,
        Caller::AutomationReadOnly => Origin::AutomationReadOnly,
        Caller::AutomationReadWrite => Origin::AutomationReadWrite,
    }
}

impl PluginJobs {
    pub fn revoke_guards(&mut self, plugin: Option<&str>, automation_only: bool) {
        self.asset_guards.retain(|(id, origin, token)| {
            let Some(token) = token.upgrade() else {
                return false;
            };
            if plugin.is_none_or(|plugin| plugin == id)
                && (!automation_only || *origin != Origin::Local)
            {
                token.store(true, std::sync::atomic::Ordering::Release);
            }
            true
        });
    }
    pub fn info(
        &self,
        manifest: &Manifest,
        generation: u64,
        origin: Origin,
        writes: bool,
    ) -> PluginInfo {
        PluginInfo {
            id: manifest.id.clone(),
            version: manifest.version.clone(),
            kind: manifest.kind,
            languages: manifest.languages.clone(),
            generation,
            effective_rules: crate::plugin_permissions::effective_rules(
                &manifest.permissions.rules,
                origin,
                writes,
            ),
        }
    }
    pub fn current(&self, run: &Run) -> bool {
        self.registry.as_ref().is_some_and(|registry| {
            registry.is_current(&run.binding, self.vault_id, self.vault_generation)
        })
    }
    pub fn authorized_run(&self, id: Uuid, caller: Caller) -> Result<&Run, CommandError> {
        let run = self.runs.get(&id).ok_or(CommandError::NotFound)?;
        // MCP currently authenticates the same-user automation class, not individual agents.
        let access = origin(caller);
        let same_class = access == Origin::Local
            || (access != Origin::Local && run.binding.origin != Origin::Local);
        if !same_class {
            return Err(CommandError::Unauthorized);
        }
        Ok(run)
    }
    pub fn start(
        &mut self,
        id: &str,
        invocation: Invocation,
        caller: Caller,
    ) -> Result<Uuid, CommandError> {
        if self.runs.values().filter(|run| run.job.is_some()).count() >= MAX_ACTIVE_RUNS {
            return Err(CommandError::Busy);
        }
        while self.runs.len() >= MAX_RUN_RECORDS {
            let removable = self
                .order
                .iter()
                .position(|id| self.runs.get(id).is_some_and(|run| run.job.is_none()));
            let Some(position) = removable else {
                return Err(CommandError::Busy);
            };
            if let Some(id) = self.order.remove(position) {
                self.runs.remove(&id);
            }
        }
        let binding = self
            .registry
            .as_ref()
            .ok_or_else(|| CommandError::Unavailable("plugin registry is not configured".into()))?
            .bind(
                id,
                origin(caller),
                matches!(invocation, Invocation::Interpreter { .. }),
                self.vault_id,
                self.vault_generation,
            )
            .map_err(|error| CommandError::Unavailable(error.to_string()))?;
        let script_sha256 = match &invocation {
            Invocation::Interpreter { source, .. } => {
                use sha2::Digest;
                Some(
                    sha2::Sha256::digest(source.as_bytes())
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                )
            }
            _ => None,
        };
        let run_id = Uuid::new_v4();
        let bridge: Arc<dyn CommandBridge> = Arc::new(Bridge {
            run_id,
            send: self.send.clone(),
        });
        if !self.runtimes.contains_key(id) && self.runtimes.len() >= 16 {
            let idle = self
                .runtimes
                .keys()
                .find(|candidate| {
                    !self
                        .runs
                        .values()
                        .any(|run| run.job.is_some() && &run.binding.id == *candidate)
                })
                .cloned();
            if let Some(idle) = idle {
                self.runtimes.remove(&idle);
            } else {
                return Err(CommandError::Busy);
            }
        }
        let runtime = self.runtimes.entry(id.to_owned()).or_default();
        let job = runtime
            .start_component(
                binding.component.clone(),
                invocation,
                bridge,
                binding.manifest.limits,
            )
            .map_err(|error| CommandError::Unavailable(error.to_string()))?;
        self.order.push_back(run_id);
        self.runs.insert(
            run_id,
            Run {
                binding,
                job: Some(job),
                status: RunStatus {
                    run_id,
                    plugin_id: id.into(),
                    script_sha256,
                    phase: RunPhase::Running,
                    output: None,
                    error: None,
                    diagnostics: Vec::new(),
                    receipts: Vec::new(),
                },
                results: HashMap::new(),
                downstream_jobs: 0,
                permission_epoch: self.permission_epoch,
            },
        );
        Ok(run_id)
    }
    pub fn poll_jobs(&mut self) {
        for run in self.runs.values_mut() {
            let Some(job) = &run.job else {
                continue;
            };
            let current = self.registry.as_ref().is_some_and(|registry| {
                registry.is_current(&run.binding, self.vault_id, self.vault_generation)
            });
            if !current
                || (run.binding.origin != Origin::Local
                    && run.permission_epoch != self.permission_epoch)
            {
                job.cancel();
            }
            let Some(result) = job.try_result() else {
                continue;
            };
            run.status.diagnostics = job
                .diagnostics()
                .into_iter()
                .map(|diagnostic| format!("{:?}: {}", diagnostic.level, diagnostic.message))
                .collect();
            match result {
                Ok(result) => {
                    run.status.phase = RunPhase::Completed;
                    run.status.output = Some(result.output);
                }
                Err(error) => {
                    run.status.phase =
                        if matches!(error, crate::wasm_extensions::ExtensionError::Cancelled) {
                            RunPhase::Cancelled
                        } else {
                            RunPhase::Failed
                        };
                    run.status.error = Some(error.to_string());
                }
            }
            run.results.clear();
            run.job = None;
        }
    }
}

impl Drop for PluginJobs {
    fn drop(&mut self) {
        self.revoke_guards(None, false);
        for run in self.runs.values() {
            if let Some(job) = &run.job {
                job.cancel();
            }
        }
    }
}
