//! Bounded metadata-only discovery for enabled agent configurations.
//! Helper teardown is off the application thread; no prompt or history is sent.
use crate::conversation::Backend;
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderKey {
    pub backend: Backend,
    pub workspace: PathBuf,
}

#[derive(Clone)]
pub enum ProviderConfig {
    #[cfg(feature = "codex")]
    Codex(crate::codex::CodexConfig),
    #[cfg(feature = "kimi")]
    Kimi(crate::kimi::KimiConfig),
}
impl ProviderConfig {
    pub fn key(&self) -> ProviderKey {
        match self {
            #[cfg(feature = "codex")]
            Self::Codex(c) => ProviderKey {
                backend: Backend::Codex,
                workspace: c.cwd.clone(),
            },
            #[cfg(feature = "kimi")]
            Self::Kimi(c) => ProviderKey {
                backend: Backend::Kimi,
                workspace: c.cwd.clone(),
            },
        }
    }
}
#[derive(Clone, Debug)]
pub enum Snapshot {
    #[cfg(feature = "codex")]
    Codex(Vec<crate::codex::ModelChoice>),
    #[cfg(feature = "kimi")]
    Kimi(Vec<crate::kimi::ConfigOption>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Queued,
    Loading,
    CleaningUp,
    Available,
    Failed,
    Paused,
}

pub struct Entry {
    pub key: ProviderKey,
    pub generation: u64,
    pub phase: Phase,
    pub snapshot: Option<Snapshot>,
    pub error: Option<String>,
    pub cleanup_issue: Option<String>,
    config: ProviderConfig,
}
struct Running {
    index: usize,
    generation: u64,
    cancel: Arc<AtomicBool>,
}
enum Update {
    Cleaning {
        index: usize,
        generation: u64,
    },
    Finished {
        index: usize,
        generation: u64,
        result: Outcome,
    },
}
struct Outcome {
    snapshot: Option<Snapshot>,
    error: Option<String>,
    cleanup_issue: Option<String>,
    cancelled: bool,
}
impl Outcome {
    fn failure(error: impl Into<String>, cancelled: bool) -> Self {
        Self {
            snapshot: None,
            error: Some(error.into()),
            cleanup_issue: None,
            cancelled,
        }
    }
}

pub struct ModelCatalog {
    pub entries: Vec<Entry>,
    pub automatic: bool,
    queued_once: bool,
    queue: VecDeque<usize>,
    running: Option<Running>,
    tx: SyncSender<Update>,
    rx: Receiver<Update>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
impl Default for ModelCatalog {
    fn default() -> Self {
        let (tx, rx) = mpsc::sync_channel(8);
        Self {
            entries: Vec::new(),
            automatic: true,
            queued_once: false,
            queue: VecDeque::new(),
            running: None,
            tx,
            rx,
            wake: Arc::new(|| {}),
        }
    }
}
impl ModelCatalog {
    pub fn set_waker(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.wake = Arc::new(wake);
    }
    pub fn enable(&mut self, config: ProviderConfig) {
        let key = config.key();
        if self.entries.iter().any(|e| e.key == key) {
            return;
        }
        self.entries.push(Entry {
            key,
            generation: 0,
            phase: Phase::Idle,
            snapshot: None,
            error: None,
            cleanup_issue: None,
            config,
        });
    }
    /// First conversation/picker opening starts all enabled metadata jobs once.
    pub fn start_automatic(&mut self) {
        if self.queued_once || !self.automatic {
            return;
        }
        self.queued_once = true;
        self.refresh_all();
    }
    pub fn refresh_all(&mut self) {
        if let Some(running) = &self.running {
            running.cancel.store(true, Ordering::Release);
        }
        self.queue.clear();
        for (index, entry) in self.entries.iter_mut().enumerate() {
            entry.generation = entry.generation.saturating_add(1);
            entry.phase = Phase::Queued;
            entry.error = None;
            self.queue.push_back(index);
        }
        self.dispatch();
    }
    pub fn retry_unavailable(&mut self) {
        for (index, entry) in self.entries.iter_mut().enumerate() {
            if matches!(entry.phase, Phase::Failed | Phase::Paused) && !self.queue.contains(&index)
            {
                entry.generation = entry.generation.saturating_add(1);
                entry.phase = Phase::Queued;
                entry.error = None;
                self.queue.push_back(index);
            }
        }
        self.dispatch();
    }
    pub fn stop(&mut self) {
        self.queued_once = true;
        for index in self.queue.drain(..) {
            self.entries[index].phase = Phase::Paused;
        }
        if let Some(running) = &self.running {
            running.cancel.store(true, Ordering::Release);
        }
    }
    pub fn is_stopped(&self) -> bool {
        self.running.is_none() && self.queue.is_empty()
    }
    /// Drains bounded worker updates and schedules the next helper, never joining a child.
    pub fn tick(&mut self) {
        for _ in 0..8 {
            let Ok(update) = self.rx.try_recv() else {
                break;
            };
            match update {
                Update::Cleaning { index, generation } => {
                    if let Some(entry) = self.entries.get_mut(index) {
                        if entry.generation == generation {
                            entry.phase = Phase::CleaningUp;
                        }
                    }
                }
                Update::Finished {
                    index,
                    generation,
                    result,
                } => {
                    if self
                        .running
                        .as_ref()
                        .is_some_and(|r| r.index == index && r.generation == generation)
                    {
                        self.running = None;
                    }
                    if let Some(entry) = self.entries.get_mut(index) {
                        // A cleanup issue belongs to the helper that created the session even after refresh.
                        if result.cleanup_issue.is_some() {
                            entry.cleanup_issue = result.cleanup_issue;
                        }
                        if entry.generation == generation {
                            if result.snapshot.is_some() {
                                entry.snapshot = result.snapshot;
                            }
                            entry.error = result.error;
                            entry.phase = if result.cancelled {
                                Phase::Paused
                            } else if entry.snapshot.is_some() && entry.error.is_none() {
                                Phase::Available
                            } else {
                                Phase::Failed
                            };
                        }
                    }
                }
            }
        }
        self.dispatch();
    }
    fn dispatch(&mut self) {
        if self.running.is_some() {
            return;
        }
        let Some(index) = self.queue.pop_front() else {
            return;
        };
        let entry = &mut self.entries[index];
        entry.phase = Phase::Loading;
        let generation = entry.generation;
        let config = entry.config.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let tx = self.tx.clone();
        let wake = self.wake.clone();
        self.running = Some(Running {
            index,
            generation,
            cancel,
        });
        let spawn = std::thread::Builder::new()
            .name("tanglewood-model-catalog".into())
            .spawn(move || {
                let report_cleaning = || {
                    let _ = tx.try_send(Update::Cleaning { index, generation });
                    wake();
                };
                let result = run_helper(config, &worker_cancel, &report_cleaning);
                let _ = tx.send(Update::Finished {
                    index,
                    generation,
                    result,
                });
                wake();
            });
        if let Err(error) = spawn {
            self.running = None;
            entry.phase = Phase::Failed;
            entry.error = Some(format!("Could not start model discovery: {error}"));
            self.dispatch();
        }
    }
}
impl Drop for ModelCatalog {
    fn drop(&mut self) {
        self.stop();
        // On normal UI shutdown the owner has already waited asynchronously. This
        // fallback keeps a helper alive long enough to finish its bounded cleanup.
        let deadline = Instant::now() + Duration::from_secs(72);
        while !self.is_stopped() && Instant::now() < deadline {
            self.tick();
            if !self.is_stopped() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn run_helper(config: ProviderConfig, cancel: &AtomicBool, cleaning: &impl Fn()) -> Outcome {
    match config {
        #[cfg(feature = "codex")]
        ProviderConfig::Codex(config) => codex_helper(config, cancel),
        #[cfg(feature = "kimi")]
        ProviderConfig::Kimi(config) => kimi_helper(config, cancel, cleaning),
    }
}
#[cfg(feature = "codex")]
fn codex_helper(config: crate::codex::CodexConfig, cancel: &AtomicBool) -> Outcome {
    use crate::codex::{CodexAdapter, CodexEvent, CodexRequest};
    let adapter = match CodexAdapter::spawn_catalog(config, || {}) {
        Ok(adapter) => adapter,
        Err(error) => return Outcome::failure(error.to_string(), false),
    };
    // One absolute deadline plus page/no-progress caps prevent a provider from
    // starving later helpers with an endless succession of fresh cursors.
    let deadline = Instant::now() + Duration::from_secs(35);
    let mut models: Vec<crate::codex::ModelChoice> = Vec::new();
    let mut cursors = HashSet::new();
    let mut pages = 0usize;
    let mut no_progress = 0usize;
    let result = loop {
        if cancel.load(Ordering::Acquire) {
            break Outcome {
                snapshot: None,
                error: None,
                cleanup_issue: None,
                cancelled: true,
            };
        }
        if Instant::now() >= deadline {
            break Outcome::failure("Codex model discovery timed out.", false);
        }
        match adapter.try_recv() {
            Some(CodexEvent::Models {
                models: page,
                next_cursor,
            }) => {
                pages += 1;
                let before = models.len();
                for model in page {
                    if let Some(old) = models.iter_mut().find(|m| m.model == model.model) {
                        *old = model;
                    } else {
                        models.push(model);
                    }
                }
                if models.len() > 128 {
                    break Outcome::failure("Codex returned more than 128 models.", false);
                }
                no_progress = if models.len() == before {
                    no_progress + 1
                } else {
                    0
                };
                if next_cursor.is_some() && (pages >= 32 || no_progress >= 4) {
                    break Outcome::failure(
                        "Codex model pagination did not complete within its limit.",
                        false,
                    );
                }
                match next_cursor {
                    Some(cursor)
                        if cursor.is_empty()
                            || cursor.len() > 4096
                            || !cursors.insert(cursor.clone())
                            || models.len() >= 128 =>
                    {
                        break Outcome::failure(
                            "Codex returned an invalid or excessive model cursor.",
                            false,
                        );
                    }
                    Some(cursor) => {
                        if let Err(error) = adapter.try_send(CodexRequest::ListModels {
                            cursor: Some(cursor),
                        }) {
                            break Outcome::failure(error, false);
                        }
                    }
                    None => {
                        break Outcome {
                            snapshot: Some(Snapshot::Codex(models)),
                            error: None,
                            cleanup_issue: None,
                            cancelled: false,
                        };
                    }
                }
            }
            Some(CodexEvent::Error(error) | CodexEvent::Disconnected(error)) => {
                break Outcome::failure(error, false);
            }
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    drop(adapter);
    result
}
#[cfg(feature = "kimi")]
fn kimi_helper(
    config: crate::kimi::KimiConfig,
    cancel: &AtomicBool,
    cleaning: &impl Fn(),
) -> Outcome {
    kimi_helper_with_timeout(config, cancel, cleaning, Duration::from_secs(35))
}
#[cfg(feature = "kimi")]
fn kimi_helper_with_timeout(
    mut config: crate::kimi::KimiConfig,
    cancel: &AtomicBool,
    cleaning: &impl Fn(),
    timeout: Duration,
) -> Outcome {
    use crate::kimi::{KimiAdapter, KimiEvent, KimiRequest};
    const UNCERTAIN: &str = "A temporary Kimi session may have been created, but its identity is unknown. Inspect Kimi's session list before retrying; verify ownership before any manual cleanup.";
    config.resume_session = None;
    let adapter = match KimiAdapter::spawn(config, || {}) {
        Ok(adapter) => adapter,
        Err(error) => return Outcome::failure(error.to_string(), false),
    };
    let mut deadline = Instant::now() + timeout;
    let mut requested = false;
    let mut owned: Option<String> = None;
    let mut snapshot = None;
    let mut failure = None;
    let mut cleanup = false;
    let result = loop {
        let cancelled = cancel.load(Ordering::Acquire);
        if cancelled && !requested {
            break Outcome {
                snapshot,
                error: None,
                cleanup_issue: None,
                cancelled: true,
            };
        }
        if Instant::now() >= deadline {
            let detail = match &owned {
                Some(id) => Some(format!(
                    "Temporary Kimi session {id} was created by discovery; cleanup was not acknowledged. Verify that identity in Kimi before manual cleanup."
                )),
                None if requested => Some(UNCERTAIN.into()),
                None => None,
            };
            break Outcome {
                snapshot,
                error: Some("Kimi model discovery timed out.".into()),
                cleanup_issue: detail,
                cancelled,
            };
        }
        match adapter.try_recv() {
            Some(KimiEvent::Connected { .. }) if !requested => {
                if cancelled {
                    break Outcome {
                        snapshot,
                        error: None,
                        cleanup_issue: None,
                        cancelled: true,
                    };
                }
                match adapter.try_send(KimiRequest::NewSession) {
                    Ok(()) => {
                        requested = true;
                        deadline = Instant::now() + timeout;
                    }
                    Err(error) => break Outcome::failure(error, false),
                }
            }
            Some(KimiEvent::Opened {
                session, options, ..
            }) => {
                owned = Some(session.clone());
                snapshot = Some(Snapshot::Kimi(options));
                cleanup = true;
                cleaning();
                if let Err(error) =
                    adapter.try_send(KimiRequest::DeleteCreatedSession(session.clone()))
                {
                    break Outcome {
                        snapshot,
                        error: None,
                        cleanup_issue: Some(format!(
                            "Temporary Kimi session {session} cleanup failed: {error}. Verify this helper-created identity before manual cleanup."
                        )),
                        cancelled,
                    };
                }
                deadline = Instant::now() + timeout;
            }
            Some(KimiEvent::OpenFailed {
                session: Some(session),
                error,
            }) => {
                // The adapter queues close/delete itself when initial config parsing fails.
                owned = Some(session);
                failure = Some(error);
                cleanup = true;
                cleaning();
                deadline = Instant::now() + timeout;
            }
            Some(KimiEvent::OpenFailed {
                session: None,
                error,
            }) => {
                break Outcome {
                    snapshot,
                    error: Some(error),
                    cleanup_issue: requested.then(|| UNCERTAIN.into()),
                    cancelled,
                };
            }
            Some(KimiEvent::Deleted { session }) if owned.as_ref() == Some(&session) => {
                break Outcome {
                    snapshot,
                    error: failure,
                    cleanup_issue: None,
                    cancelled,
                };
            }
            Some(KimiEvent::Error(error) | KimiEvent::Disconnected(error)) => {
                let issue = if cleanup {
                    owned.as_ref().map(|id|format!("Temporary Kimi session {id} cleanup failed: {error}. Verify this helper-created identity before manual cleanup."))
                } else if requested {
                    Some(UNCERTAIN.into())
                } else {
                    None
                };
                break Outcome {
                    snapshot,
                    error: if cleanup { failure } else { Some(error) },
                    cleanup_issue: issue,
                    cancelled,
                };
            }
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    drop(adapter);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, sync::atomic::AtomicBool};
    struct Fixture {
        root: PathBuf,
        executable: PathBuf,
        audit: PathBuf,
    }
    impl Fixture {
        fn new(behavior: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("tanglewood-catalog-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            let executable = root.join("agent");
            let audit = root.join("audit.jsonl");
            let state = root.join("state.json");
            let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("docs/fixtures/model-picker/agent.py");
            let wrapper = format!(
                "#!/bin/sh\nTW_FAKE_AUDIT='{}' TW_FAKE_STATE='{}' TW_FAKE_BEHAVIOR='{}' exec python3 '{}' \"$@\"\n",
                audit.display(),
                state.display(),
                behavior,
                script.display()
            );
            fs::write(&executable, wrapper).unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                root,
                executable,
                audit,
            }
        }
        fn methods(&self) -> Vec<String> {
            fs::read_to_string(&self.audit)
                .unwrap_or_default()
                .lines()
                .map(|line| {
                    serde_json::from_str::<serde_json::Value>(line).unwrap()["method"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
                .collect()
        }
        #[cfg(feature = "kimi")]
        fn kimi(&self) -> crate::kimi::KimiConfig {
            crate::kimi::KimiConfig {
                executable: self.executable.clone(),
                cwd: self.root.clone(),
                mcp_bridge: self.root.join("bridge"),
                socket: self.root.join("socket"),
                resume_session: Some("must-not-load".into()),
            }
        }
        #[cfg(feature = "codex")]
        fn codex(&self) -> crate::codex::CodexConfig {
            crate::codex::CodexConfig {
                executable: self.executable.clone(),
                cwd: self.root.clone(),
                mcp_bridge: self.root.join("bridge"),
                socket: self.root.join("socket"),
                resume_thread: Some("must-not-resume".into()),
                project: None,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[cfg(feature = "codex")]
    #[test]
    fn codex_helper_lists_without_project_thread_or_prompt() {
        let fixture = Fixture::new("normal");
        let result = codex_helper(fixture.codex(), &AtomicBool::new(false));
        assert!(matches!(result.snapshot,Some(Snapshot::Codex(ref models)) if models.len()==1));
        assert!(result.error.is_none());
        let methods = fixture.methods();
        assert!(methods.iter().any(|m| m == "model/list"));
        assert!(!methods.iter().any(|m| m.starts_with("project/")
            || m.starts_with("thread/")
            || m.starts_with("turn/")));
    }
    #[cfg(feature = "kimi")]
    #[test]
    fn kimi_helper_cleans_only_its_session_and_keeps_failures_separate() {
        let fixture = Fixture::new("normal");
        let result = kimi_helper(fixture.kimi(), &AtomicBool::new(false), &|| {});
        assert!(
            matches!(result.snapshot,Some(Snapshot::Kimi(ref options)) if options.iter().any(|o|o.category.as_deref()==Some("model")))
        );
        assert!(result.error.is_none() && result.cleanup_issue.is_none());
        let methods = fixture.methods();
        assert_eq!(methods.iter().filter(|m| *m == "session/new").count(), 1);
        assert_eq!(methods.iter().filter(|m| *m == "session/close").count(), 1);
        assert_eq!(methods.iter().filter(|m| *m == "session/delete").count(), 1);
        assert!(
            !methods
                .iter()
                .any(|m| m == "session/load" || m == "session/prompt")
        );
        let failed = Fixture::new("cleanup_failure");
        let result = kimi_helper(failed.kimi(), &AtomicBool::new(false), &|| {});
        assert!(result.snapshot.is_some());
        assert!(
            result
                .cleanup_issue
                .as_deref()
                .is_some_and(|s| s.contains("fixture-"))
        );
        let malformed = Fixture::new("malformed_config");
        let result = kimi_helper(malformed.kimi(), &AtomicBool::new(false), &|| {});
        assert!(
            result.snapshot.is_none() && result.error.is_some() && result.cleanup_issue.is_none()
        );
        assert!(malformed.methods().iter().any(|m| m == "session/delete"));
    }
    #[cfg(feature = "kimi")]
    #[test]
    fn cancellation_waits_for_owned_session_cleanup() {
        let fixture = Fixture::new("slow_new");
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let config = fixture.kimi();
        let worker = std::thread::spawn(move || kimi_helper(config, &worker_cancel, &|| {}));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !fixture.methods().iter().any(|m| m == "session/new") {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        cancel.store(true, Ordering::Release);
        let result = worker.join().unwrap();
        assert!(result.cancelled && result.cleanup_issue.is_none());
        assert!(fixture.methods().iter().any(|m| m == "session/delete"));
        assert!(
            !fixture
                .methods()
                .iter()
                .any(|m| m == "session/prompt" || m == "session/load")
        );
    }
    #[cfg(feature = "codex")]
    #[test]
    fn codex_fresh_empty_cursors_are_bounded() {
        let fixture = Fixture::new("fresh_empty_pages");
        let result = codex_helper(fixture.codex(), &AtomicBool::new(false));
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|e| e.contains("pagination"))
        );
        let calls = fixture
            .methods()
            .into_iter()
            .filter(|m| m == "model/list")
            .count();
        assert!((1..=4).contains(&calls), "model pages: {calls}");
    }
    #[cfg(feature = "kimi")]
    #[test]
    fn uncertain_creation_survives_missing_id_disconnect_and_timeout() {
        for behavior in [
            "missing_new_id",
            "disconnect_new",
            "open_failed_new",
            "no_new_reply",
        ] {
            let fixture = Fixture::new(behavior);
            let result = kimi_helper_with_timeout(
                fixture.kimi(),
                &AtomicBool::new(false),
                &|| {},
                Duration::from_millis(700),
            );
            assert!(result.error.is_some(), "{behavior}");
            assert!(
                result
                    .cleanup_issue
                    .as_deref()
                    .is_some_and(|s| s.contains("identity is unknown") && s.contains("Inspect")),
                "{behavior}"
            );
            let methods = fixture.methods();
            assert!(methods.iter().any(|m| m == "session/new"), "{behavior}");
            assert!(
                !methods.iter().any(|m| m == "session/list"
                    || m == "session/load"
                    || m == "session/delete"
                    || m == "session/prompt"),
                "{behavior}"
            );
        }
        // Retrying does not erase an unresolved ownership warning.
        let fixture = Fixture::new("missing_new_id");
        let mut catalog = ModelCatalog::default();
        catalog.enable(ProviderConfig::Kimi(fixture.kimi()));
        catalog.start_automatic();
        let end = Instant::now() + Duration::from_secs(5);
        while !matches!(catalog.entries[0].phase, Phase::Failed) {
            catalog.tick();
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(10));
        }
        let issue = catalog.entries[0].cleanup_issue.clone();
        catalog.refresh_all();
        assert_eq!(catalog.entries[0].cleanup_issue, issue);
        catalog.stop();
        let end = Instant::now() + Duration::from_secs(5);
        while catalog.running.is_some() {
            catalog.tick();
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(catalog.entries[0].cleanup_issue.is_some());
    }
}
