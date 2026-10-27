//! Shared provider identity and presentation values; protocols remain provider-specific.
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Backend {
    #[cfg(feature = "codex")]
    Codex,
    #[cfg(feature = "kimi")]
    Kimi,
}

/// A session is unique within a provider and canonical workspace.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConversationKey {
    pub backend: Backend,
    pub workspace: PathBuf,
    pub session: String,
}

/// Changes whenever a child connection is replaced, including reconnects.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConnectionIdentity {
    pub backend: Backend,
    pub workspace: PathBuf,
    pub generation: u64,
}

impl ConnectionIdentity {
    pub fn owns(&self, key: &ConversationKey) -> bool {
        self.backend == key.backend && self.workspace == key.workspace
    }
}

#[derive(Clone, Debug)]
pub struct ContextFile {
    pub path: String,
    pub kind: String,
}
#[derive(Clone, Debug)]
pub struct PlanStep {
    pub text: String,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct PromptAttachment {
    pub name: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadWorkState {
    NotLoaded,
    Idle,
    Working,
    WaitingOnApproval,
    WaitingOnInput,
    Error,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct ThreadSummary {
    pub state: ThreadWorkState,
    pub id: String,
    pub title: String,
    pub updated_at: i64,
}
#[derive(Clone, Debug)]
pub struct HistoryMessage {
    pub role: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct CommandExecution {
    pub item: String,
    pub command: String,
    pub cwd: String,
    pub status: String,
    pub exit_code: Option<i64>,
    pub output: String,
    pub truncated: bool,
    pub output_complete: bool,
}
#[derive(Clone, Debug)]
pub struct Activity {
    pub item: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub details: String,
    pub truncated: bool,
}

#[cfg(all(test, feature = "codex", feature = "kimi"))]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn same_session_id_and_stale_connection_remain_distinct() {
        let workspace = PathBuf::from("/tmp/workspace");
        let codex = ConversationKey {
            backend: Backend::Codex,
            workspace: workspace.clone(),
            session: "same".into(),
        };
        let kimi = ConversationKey {
            backend: Backend::Kimi,
            workspace: workspace.clone(),
            session: "same".into(),
        };
        let mut drafts = HashMap::new();
        drafts.insert(codex.clone(), "codex draft");
        drafts.insert(kimi.clone(), "kimi draft");
        assert_eq!(drafts.len(), 2);
        let current = ConnectionIdentity {
            backend: Backend::Kimi,
            workspace,
            generation: 2,
        };
        let old = ConnectionIdentity {
            generation: 1,
            ..current.clone()
        };
        assert!(current.owns(&kimi));
        assert!(!current.owns(&codex));
        assert_ne!(current, old);
    }
}

#[derive(Clone, Debug)]
pub struct GoalInfo {
    pub objective: String,
    pub status: String,
    pub tokens_used: u64,
    pub time_used_seconds: u64,
    pub token_budget: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct SkillInfo {
    pub name: String,
    pub path: String,
    pub description: String,
    pub enabled: bool,
}
#[derive(Clone, Debug)]
pub struct ConnectionInfo {
    pub name: String,
    pub auth_status: String,
    pub tools: usize,
}

/// Provider-specific commands retain their native semantics.
#[cfg(any(feature = "codex", feature = "kimi"))]
pub enum Request {
    #[cfg(feature = "codex")]
    Codex(crate::codex::CodexRequest),
    #[cfg(feature = "kimi")]
    Kimi(crate::kimi::KimiRequest),
}
#[cfg(any(feature = "codex", feature = "kimi"))]
pub enum Event {
    #[cfg(feature = "codex")]
    Codex(crate::codex::CodexEvent),
    #[cfg(feature = "kimi")]
    Kimi(crate::kimi::KimiEvent),
}
#[cfg(any(feature = "codex", feature = "kimi"))]
pub enum Adapter {
    #[cfg(feature = "codex")]
    Codex(crate::codex::CodexAdapter),
    #[cfg(feature = "kimi")]
    Kimi(crate::kimi::KimiAdapter),
}
#[cfg(any(feature = "codex", feature = "kimi"))]
impl Adapter {
    pub fn try_send(&self, request: Request) -> Result<(), String> {
        match (self, request) {
            #[cfg(feature = "codex")]
            (Self::Codex(adapter), Request::Codex(request)) => adapter.try_send(request),
            #[cfg(feature = "kimi")]
            (Self::Kimi(adapter), Request::Kimi(request)) => adapter.try_send(request),
            #[cfg(all(feature = "codex", feature = "kimi"))]
            _ => Err("request belongs to a different conversation backend".into()),
        }
    }
    pub fn try_recv(&self) -> Option<Event> {
        match self {
            #[cfg(feature = "codex")]
            Self::Codex(adapter) => adapter.try_recv().map(Event::Codex),
            #[cfg(feature = "kimi")]
            Self::Kimi(adapter) => adapter.try_recv().map(Event::Kimi),
        }
    }
}
