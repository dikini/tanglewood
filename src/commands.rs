use crate::notes::{NoteId, NoteView, Revision, TextEdit};
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
const COMMANDS: [CommandDescriptor; 22] = [
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
];
impl CommandDescriptor {
    pub fn allowed(self, caller: Caller) -> bool {
        self.available
            && (!self.local_only || caller == Caller::Local)
            && !(self.mutation && caller == Caller::AutomationReadOnly)
    }
}
impl Command {
    pub fn descriptor(&self) -> CommandDescriptor {
        let index = match self {
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
        };
        COMMANDS[index]
    }
    pub fn automation_variants(caller: Caller) -> Vec<(&'static str, &'static str)> {
        COMMANDS
            .iter()
            .filter(|descriptor| descriptor.allowed(caller) && !descriptor.local_only)
            .map(|descriptor| (descriptor.wire_variant, descriptor.name))
            .collect()
    }
    pub fn is_mutation(&self) -> bool {
        !matches!(self, Self::ReadNote { .. } | Self::SearchNotes { .. })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CommandResult {
    LanTicket { ticket: String },
    LanStatus(LanStatus),
    Accepted { request_id: uuid::Uuid },
    Note(NoteView),
    Notes(Vec<NoteView>),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum CommandError {
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
