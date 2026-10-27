use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct NoteId(pub Uuid);

/// Local vault association with a Codex project; never synchronized as note content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct CodexProjectBinding {
    pub project_id: Option<String>,
    pub name: String,
    pub creation_key: String,
}
impl CodexProjectBinding {
    pub(crate) fn valid(&self) -> bool {
        fn text(value: &str, maximum: usize) -> bool {
            !value.trim().is_empty()
                && value.len() <= maximum
                && !value.chars().any(char::is_control)
        }
        text(&self.name, 256)
            && text(&self.creation_key, 256)
            && self.project_id.as_deref().is_none_or(|id| text(id, 256))
    }
}

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
