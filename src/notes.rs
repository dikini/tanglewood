use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct NoteId(pub Uuid);

/// Opaque equality token. Revisions have no ordering semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct Revision(pub Uuid);
impl Revision {
    pub(crate) fn fresh() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Owned storage/transport record; ECS entity handles never escape the App.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Component)]
pub struct NoteRecord {
    pub id: NoteId,
    pub path: String,
    pub text: String,
    pub revision: Revision,
    pub trashed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Component)]
pub struct SaveState {
    pub accepted: Revision,
    pub persisted: Option<Revision>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteView {
    pub note: NoteRecord,
    pub save: SaveState,
}

/// Ranges are UTF-8 byte offsets into the original source, before any edit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

pub(crate) fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0', ':'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && path.ends_with(".md")
}

pub(crate) fn edited_text(source: &str, edits: &mut [TextEdit]) -> Option<String> {
    edits.sort_by_key(|edit| (edit.start, edit.end));
    let mut previous: Option<&TextEdit> = None;
    for edit in edits.iter() {
        if edit.start > edit.end
            || edit.end > source.len()
            || !source.is_char_boundary(edit.start)
            || !source.is_char_boundary(edit.end)
        {
            return None;
        }
        if let Some(prior) = previous {
            // Multiple insertions at one offset are ambiguous; reject them too.
            if prior.end > edit.start || prior.start == edit.start {
                return None;
            }
        }
        previous = Some(edit);
    }
    let mut result = source.to_owned();
    for edit in edits.iter().rev() {
        result.replace_range(edit.start..edit.end, &edit.replacement);
    }
    Some(result)
}
