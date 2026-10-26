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
pub enum Command {
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
    pub available: bool,
    pub mutation: bool,
}
impl CommandDescriptor {
    pub fn allowed(self, caller: Caller) -> bool {
        self.available && !(self.mutation && caller == Caller::AutomationReadOnly)
    }
}
impl Command {
    pub fn descriptor(&self) -> CommandDescriptor {
        let name = match self {
            Self::CreateNote { .. } => "note.create",
            Self::ReadNote { .. } => "note.read",
            Self::EditNote { .. } => "note.edit",
            Self::RenameNote { .. } => "note.rename",
            Self::TrashNote { .. } => "note.trash",
            Self::RestoreNote { .. } => "note.restore",
            Self::SearchNotes { .. } => "note.search",
        };
        CommandDescriptor {
            name,
            available: true,
            mutation: self.is_mutation(),
        }
    }
    pub fn is_mutation(&self) -> bool {
        !matches!(self, Self::ReadNote { .. } | Self::SearchNotes { .. })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CommandResult {
    Note(NoteView),
    Notes(Vec<NoteView>),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum CommandError {
    #[error("caller is not authorized for this command")]
    Unauthorized,
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
