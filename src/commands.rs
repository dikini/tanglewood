use crate::notes::{CodexProjectBinding, NoteId, NoteView, Revision, TextEdit};
use serde::{Deserialize, Serialize};

/// Constructed by a trusted adapter, never from a request's claimed identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    Local,
    AutomationReadOnly,
    AutomationReadWrite,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LanPeerStatus {
    pub fingerprint: String,
    pub connected: bool,
    pub session: Option<u64>,
    pub last_receipt: Option<uuid::Uuid>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LanStatus {
    pub listening: bool,
    pub paused: bool,
    pub prepared: bool,
    pub joining: bool,
    pub vault_id: Option<uuid::Uuid>,
    pub ticket: Option<String>,
    pub peers: Vec<LanPeerStatus>,
    pub pending_messages: usize,
    pub error: Option<String>,
    pub accepted_generation: u64,
    pub persisted_generation: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub enum ImportDestination {
    Create { path: String },
    Replace { id: NoteId, expected: Revision },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub enum Command {
    #[cfg(feature = "wasm-extensions")]
    Plugin(crate::plugin_jobs::PluginCommand),
    Research(crate::research::ResearchCommand),
    GetCodexProject,
    SetAutomationWriteEnabled {
        enabled: bool,
    },
    SetCodexProject {
        expected: Option<CodexProjectBinding>,
        project: CodexProjectBinding,
    },
    ListenLan {
        bind: String,
        advertised_ip: Option<String>,
    },
    GetLanTicket,
    PrepareJoin {
        ticket: String,
    },
    CancelJoin,
    ConfirmPeer {
        ticket: String,
    },
    UnpairPeer {
        fingerprint: String,
    },
    ConnectPeer {
        fingerprint: String,
    },
    GetLanStatus,
    PauseLan,
    ResumeLan,
    EnableSync,
    SetMirrorEnabled {
        enabled: bool,
    },
    ImportMarkdown {
        source_path: String,
        destination: ImportDestination,
    },
    ExportMarkdown {
        id: NoteId,
        path: String,
    },
    RescanMirror,
    CreateNote {
        path: String,
        text: String,
    },
    ReadNote {
        id: NoteId,
    },
    EditNote {
        id: NoteId,
        expected: Revision,
        edits: Vec<TextEdit>,
    },
    RenameNote {
        id: NoteId,
        expected: Revision,
        path: String,
    },
    TrashNote {
        id: NoteId,
        expected: Revision,
    },
    RestoreNote {
        id: NoteId,
        expected: Revision,
    },
    SearchNotes {
        query: String,
        include_trash: bool,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct CommandDescriptor {
    pub name: &'static str,
    pub wire_variant: &'static str,
    pub available: bool,
    pub mutation: bool,
    pub local_only: bool,
}
const COMMANDS: [CommandDescriptor; 26] = [
    CommandDescriptor {
        name: "lan.listen",
        wire_variant: "ListenLan",
        available: cfg!(feature = "lan"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "lan.ticket",
        wire_variant: "GetLanTicket",
        available: cfg!(feature = "lan"),
        mutation: false,
        local_only: true,
    },
    CommandDescriptor {
        name: "lan.prepare_join",
        wire_variant: "PrepareJoin",
        available: cfg!(feature = "lan"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "lan.cancel_join",
        wire_variant: "CancelJoin",
        available: cfg!(feature = "lan"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "lan.confirm",
        wire_variant: "ConfirmPeer",
        available: cfg!(feature = "lan"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "lan.unpair",
        wire_variant: "UnpairPeer",
        available: cfg!(feature = "lan"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "lan.connect",
        wire_variant: "ConnectPeer",
        available: cfg!(feature = "lan"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "lan.status",
        wire_variant: "GetLanStatus",
        available: cfg!(feature = "lan"),
        mutation: false,
        local_only: true,
    },
    CommandDescriptor {
        name: "lan.pause",
        wire_variant: "PauseLan",
        available: cfg!(feature = "lan"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "lan.resume",
        wire_variant: "ResumeLan",
        available: cfg!(feature = "lan"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "sync.enable",
        wire_variant: "EnableSync",
        available: cfg!(feature = "sync"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "note.create",
        wire_variant: "CreateNote",
        available: true,
        mutation: true,
        local_only: false,
    },
    CommandDescriptor {
        name: "note.read",
        wire_variant: "ReadNote",
        available: true,
        mutation: false,
        local_only: false,
    },
    CommandDescriptor {
        name: "note.edit",
        wire_variant: "EditNote",
        available: true,
        mutation: true,
        local_only: false,
    },
    CommandDescriptor {
        name: "note.rename",
        wire_variant: "RenameNote",
        available: true,
        mutation: true,
        local_only: false,
    },
    CommandDescriptor {
        name: "note.trash",
        wire_variant: "TrashNote",
        available: true,
        mutation: true,
        local_only: false,
    },
    CommandDescriptor {
        name: "note.restore",
        wire_variant: "RestoreNote",
        available: true,
        mutation: true,
        local_only: false,
    },
    CommandDescriptor {
        name: "note.search",
        wire_variant: "SearchNotes",
        available: true,
        mutation: false,
        local_only: false,
    },
    CommandDescriptor {
        name: "mirror.configure",
        wire_variant: "SetMirrorEnabled",
        available: cfg!(target_os = "linux"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "markdown.import",
        wire_variant: "ImportMarkdown",
        available: true,
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "markdown.export",
        wire_variant: "ExportMarkdown",
        available: true,
        mutation: false,
        local_only: true,
    },
    CommandDescriptor {
        name: "mirror.rescan",
        wire_variant: "RescanMirror",
        available: cfg!(target_os = "linux"),
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "codex.project.read",
        wire_variant: "GetCodexProject",
        available: true,
        mutation: false,
        local_only: false,
    },
    CommandDescriptor {
        name: "codex.project.set",
        wire_variant: "SetCodexProject",
        available: true,
        mutation: true,
        local_only: true,
    },
    CommandDescriptor {
        name: "research.execute",
        wire_variant: "Research",
        available: true,
        mutation: false,
        local_only: false,
    },
    CommandDescriptor {
        name: "automation.set_write_enabled",
        wire_variant: "SetAutomationWriteEnabled",
        available: true,
        mutation: true,
        local_only: true,
    },
];
impl CommandDescriptor {
    pub fn allowed(self, caller: Caller) -> bool {
        self.available
            && (!self.local_only || caller == Caller::Local)
            && !(self.mutation && caller == Caller::AutomationReadOnly)
    }
}
/// Dispatch effect and completion are separate from caller authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandEffect {
    Query,
    Action,
    Mutation,
    Program,
    Control,
}
/// A completed domain I/O receipt is not always a durable storage receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCompletion {
    Immediate,
    Io,
    Persistence,
}
impl CommandEffect {
    pub fn records_effect(self) -> bool {
        matches!(self, Self::Action | Self::Mutation)
    }
}
impl Command {
    pub fn descriptor(&self) -> CommandDescriptor {
        #[cfg(feature = "wasm-extensions")]
        if let Self::Plugin(command) = self {
            use crate::plugin_jobs::PluginCommand as P;
            return CommandDescriptor {
                name: match command {
                    P::List => "plugin.list",
                    P::Describe { .. } => "plugin.describe",
                    P::Run { .. } => "plugin.run",
                    P::Evaluate { .. } => "plugin.evaluate",
                    P::Status { .. } => "plugin.status",
                    P::Cancel { .. } => "plugin.cancel",
                },
                wire_variant: "Plugin",
                available: true,
                mutation: false,
                local_only: false,
            };
        }
        if let Self::Research(command) = self {
            return CommandDescriptor {
                mutation: command.is_mutation(),
                local_only: matches!(
                    command,
                    crate::research::ResearchCommand::ImportSources { .. }
                        | crate::research::ResearchCommand::ImportSource { .. }
                        | crate::research::ResearchCommand::AcquireSource {
                            input: crate::research::ArtifactInput::LocalFile { .. },
                            ..
                        }
                        | crate::research::ResearchCommand::SavePaper {
                            input: crate::research::ArtifactInput::LocalFile { .. },
                            ..
                        }
                        | crate::research::ResearchCommand::AssessSource { human: true, .. }
                        | crate::research::ResearchCommand::RetryArtifact { .. }
                        | crate::research::ResearchCommand::CollectArtifact {
                            input: crate::research::ArtifactInput::LocalFile { .. }
                                | crate::research::ArtifactInput::Url { archive: true, .. },
                            ..
                        }
                ),
                ..COMMANDS[24]
            };
        }
        let index = match self {
            #[cfg(feature = "wasm-extensions")]
            Self::Plugin(_) => unreachable!("plugin descriptors returned above"),
            Self::Research(_) => 24,
            Self::SetAutomationWriteEnabled { .. } => 25,
            Self::ListenLan { .. } => 0,
            Self::GetLanTicket => 1,
            Self::PrepareJoin { .. } => 2,
            Self::CancelJoin => 3,
            Self::ConfirmPeer { .. } => 4,
            Self::UnpairPeer { .. } => 5,
            Self::ConnectPeer { .. } => 6,
            Self::GetLanStatus => 7,
            Self::PauseLan => 8,
            Self::ResumeLan => 9,

            Self::EnableSync => 10,
            Self::CreateNote { .. } => 11,
            Self::ReadNote { .. } => 12,
            Self::EditNote { .. } => 13,
            Self::RenameNote { .. } => 14,
            Self::TrashNote { .. } => 15,
            Self::RestoreNote { .. } => 16,
            Self::SearchNotes { .. } => 17,
            Self::SetMirrorEnabled { .. } => 18,
            Self::ImportMarkdown { .. } => 19,
            Self::ExportMarkdown { .. } => 20,
            Self::RescanMirror => 21,
            Self::GetCodexProject => 22,
            Self::SetCodexProject { .. } => 23,
        };
        COMMANDS[index]
    }
    pub fn automation_variants(caller: Caller) -> Vec<(&'static str, &'static str)> {
        #[allow(unused_mut)]
        let mut commands: Vec<_> = COMMANDS
            .iter()
            .filter(|descriptor| descriptor.allowed(caller) && !descriptor.local_only)
            .map(|descriptor| (descriptor.wire_variant, descriptor.name))
            .collect();
        #[cfg(feature = "wasm-extensions")]
        commands.extend([
            ("Plugin", "plugin.list"),
            ("Plugin", "plugin.describe"),
            ("Plugin", "plugin.run"),
            ("Plugin", "plugin.evaluate"),
            ("Plugin", "plugin.status"),
            ("Plugin", "plugin.cancel"),
        ]);
        commands
    }
    /// Program execution and run control never enter the non-mutating read tool.
    pub fn is_query(&self) -> bool {
        #[cfg(feature = "wasm-extensions")]
        if let Self::Plugin(command) = self {
            return command.is_query();
        }
        self.descriptor().allowed(Caller::AutomationReadOnly)
    }
    pub fn effect(&self) -> CommandEffect {
        use CommandEffect as E;
        match self {
            #[cfg(feature = "wasm-extensions")]
            Self::Plugin(command) => match command {
                crate::plugin_jobs::PluginCommand::Run { .. }
                | crate::plugin_jobs::PluginCommand::Evaluate { .. } => E::Program,
                crate::plugin_jobs::PluginCommand::Cancel { .. } => E::Control,
                _ => E::Query,
            },
            Self::ReadNote { .. }
            | Self::SearchNotes { .. }
            | Self::GetCodexProject
            | Self::GetLanTicket
            | Self::GetLanStatus => E::Query,
            Self::ExportMarkdown { .. }
            | Self::ListenLan { .. }
            | Self::ConnectPeer { .. }
            | Self::PauseLan
            | Self::ResumeLan => E::Action,
            Self::Research(command) if !command.is_mutation() => E::Query,
            _ => E::Mutation,
        }
    }
    pub fn completion(&self) -> CommandCompletion {
        use CommandCompletion as C;
        match self {
            Self::ListenLan { .. }
            | Self::ConnectPeer { .. }
            | Self::PauseLan
            | Self::ResumeLan => C::Immediate,
            Self::ImportMarkdown { .. } | Self::RescanMirror => C::Io,
            _ if self.effect().records_effect() => C::Persistence,
            _ => C::Immediate,
        }
    }
    /// Outer run admission and volatile controls reply immediately.
    pub fn waits_for_completion(&self) -> bool {
        self.completion() != CommandCompletion::Immediate
    }
    pub fn waits_for_persistence(&self) -> bool {
        self.completion() == CommandCompletion::Persistence
    }
    pub fn is_mutation(&self) -> bool {
        #[cfg(feature = "wasm-extensions")]
        if matches!(self, Self::Plugin(_)) {
            return false;
        }
        if let Self::Research(command) = self {
            return command.is_mutation();
        }
        !matches!(
            self,
            Self::ReadNote { .. } | Self::SearchNotes { .. } | Self::GetCodexProject
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CommandResult {
    #[cfg(feature = "wasm-extensions")]
    Plugin(crate::plugin_jobs::PluginResult),
    Research(crate::research::ResearchResult),
    CodexProject(Option<CodexProjectBinding>),
    LanTicket {
        ticket: String,
    },
    LanStatus(LanStatus),
    Accepted {
        request_id: uuid::Uuid,
    },
    Note(NoteView),
    Notes(Vec<NoteView>),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum CommandError {
    #[error("invalid research request: {0}")]
    InvalidResearch(String),
    #[error("Codex project association changed")]
    CodexProjectChanged,
    #[error("invalid Codex project: {0}")]
    InvalidCodexProject(String),
    #[error("caller is not authorized for this command")]
    Unauthorized,
    #[error("I/O queue is full")]
    Busy,
    #[error("note does not exist")]
    NotFound,
    #[error("note revision changed")]
    StaleRevision { current: Revision },
    #[error("invalid vault-relative Markdown path")]
    InvalidPath,
    #[error("path already belongs to another note")]
    PathCollision,
    #[error("invalid or overlapping UTF-8 byte ranges")]
    InvalidEdits,
    #[error("note is trashed")]
    Trashed,
    #[error("capability unavailable: {0}")]
    Unavailable(String),
}

#[cfg(test)]
mod paper_policy_tests {
    use super::*;
    #[test]
    fn paper_download_is_authorized_but_local_paths_stay_local() {
        let base = serde_json::json!({"Research":{"SavePaper":{
            "id":uuid::Uuid::new_v4(),"expected":uuid::Uuid::new_v4(),
            "source_id":uuid::Uuid::new_v4(),
            "input":{"Url":{"url":"https://example.org/paper.pdf","archive":true}}
        }}});
        let remote: Command = serde_json::from_value(base.clone()).expect("paper save command");
        assert!(remote.descriptor().allowed(Caller::AutomationReadWrite));
        assert!(!remote.descriptor().allowed(Caller::AutomationReadOnly));
        let mut local = base;
        local["Research"]["SavePaper"]["input"] =
            serde_json::json!({"LocalFile":{"path":"/tmp/paper.pdf"}});
        let local: Command = serde_json::from_value(local).unwrap();
        assert!(local.descriptor().allowed(Caller::Local));
        assert!(!local.descriptor().allowed(Caller::AutomationReadWrite));
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;
    #[test]
    fn receipt_completion_distinguishes_volatile_io_and_durable_effects() {
        for command in [
            Command::PauseLan,
            Command::ResumeLan,
            Command::ConnectPeer {
                fingerprint: "peer".into(),
            },
            Command::ListenLan {
                bind: "127.0.0.1:0".into(),
                advertised_ip: None,
            },
        ] {
            assert!(command.effect().records_effect());
            assert!(!command.waits_for_completion());
            assert!(!command.waits_for_persistence());
        }
        for command in [
            Command::RescanMirror,
            Command::ImportMarkdown {
                source_path: "/tmp/input.md".into(),
                destination: ImportDestination::Create {
                    path: "note.md".into(),
                },
            },
        ] {
            assert!(command.waits_for_completion());
            assert!(!command.waits_for_persistence());
        }
        for command in [
            Command::CreateNote {
                path: "note.md".into(),
                text: String::new(),
            },
            Command::SetMirrorEnabled { enabled: true },
            Command::Research(crate::research::ResearchCommand::Create {
                title: "research".into(),
                question: String::new(),
            }),
        ] {
            assert!(command.waits_for_completion());
            assert!(command.waits_for_persistence());
        }
    }
}
