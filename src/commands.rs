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
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub enum ImportDestination {
    Create { path: String },
    Replace { id: NoteId, expected: Revision },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub enum Command {
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
const COMMANDS: [CommandDescriptor; 12] = [
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
            Self::EnableSync => 0,
            Self::CreateNote { .. } => 1,
            Self::ReadNote { .. } => 2,
            Self::EditNote { .. } => 3,
            Self::RenameNote { .. } => 4,
            Self::TrashNote { .. } => 5,
            Self::RestoreNote { .. } => 6,
            Self::SearchNotes { .. } => 7,
            Self::SetMirrorEnabled { .. } => 8,
            Self::ImportMarkdown { .. } => 9,
            Self::ExportMarkdown { .. } => 10,
            Self::RescanMirror => 11,
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
