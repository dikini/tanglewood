//! Offline Automerge engine. App remains the only command validation boundary.
use crate::notes::{NoteId, NoteRecord, TextEdit, edited_text, valid_path};
use automerge::{
    ActorId, AutoCommit, LoadOptions, ObjId, ObjType, ROOT, ReadDoc, ScalarValue, TextEncoding,
    Value, transaction::Transactable,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    #[error("CRDT error: {0}")]
    Crdt(#[from] automerge::AutomergeError),
    #[error("invalid CRDT history: {0}")]
    Invalid(String),
    #[error("history belongs to a different vault")]
    DifferentVault,
    #[error("note not found")]
    NotFound,
    #[error("invalid UTF-8 edit ranges")]
    InvalidEdits,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentHistory {
    pub id: NoteId,
    pub bytes: Vec<u8>,
}
/// Persist the entire bundle atomically with its SQL/ECS projections. Never send
/// an accepted-but-unpersisted bundle or acknowledge remote durability early.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryBundle {
    pub version: u32,
    pub vault_id: Uuid,
    pub device_id: Uuid,
    pub catalog: Vec<u8>,
    pub documents: Vec<DocumentHistory>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeConflicts {
    pub paths: Vec<String>,
    pub tombstones: Vec<bool>,
    pub resolved_path: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedNote {
    pub id: NoteId,
    pub text: String,
    pub path: String,
    pub trashed: bool,
    pub conflicts: MergeConflicts,
}
#[derive(Clone)]
pub struct Replica {
    vault_id: Uuid,
    device_id: Uuid,
    catalog: AutoCommit,
    documents: HashMap<NoteId, AutoCommit>,
}
impl Replica {
    /// Bootstrap once during migration. A peer must use join, not independently
    /// bootstrap the same Markdown/UUID records into unrelated histories.
    pub fn bootstrap(notes: &[NoteRecord]) -> Result<Self, MergeError> {
        let vault_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let mut catalog = new_document(device_id);
        catalog.put(ROOT, "vault_id", vault_id.to_string())?;
        catalog.put_object(ROOT, "notes", ObjType::Map)?;
        let mut replica = Self {
            vault_id,
            device_id,
            catalog,
            documents: HashMap::new(),
        };
        for note in notes {
            replica.insert(note)?;
        }
        Ok(replica)
    }
    /// A restart resumes the persisted device actor, rather than inventing one.
    pub fn load(bundle: &HistoryBundle) -> Result<Self, MergeError> {
        if bundle.version != 1 {
            return Err(MergeError::Invalid("unsupported history version".into()));
        }
        let catalog = load_document(&bundle.catalog, bundle.device_id)?;
        if scalar_string(&catalog, ROOT, "vault_id")? != bundle.vault_id.to_string() {
            return Err(MergeError::DifferentVault);
        }
        let mut documents = HashMap::new();
        for history in &bundle.documents {
            let document = load_document(&history.bytes, bundle.device_id)?;
            if scalar_string(&document, ROOT, "note_id")? != history.id.0.to_string() {
                return Err(MergeError::Invalid("note identity mismatch".into()));
            }
            if documents.insert(history.id, document).is_some() {
                return Err(MergeError::Invalid("duplicate note document".into()));
            }
        }
        let replica = Self {
            vault_id: bundle.vault_id,
            device_id: bundle.device_id,
            catalog,
            documents,
        };
        replica.project()?;
        Ok(replica)
    }
    /// Pairing joins the original shared histories and installs a fresh local
    /// device actor. Persist this returned identity before making local changes.
    pub fn join(bundle: &HistoryBundle) -> Result<Self, MergeError> {
        let mut replica = Self::load(bundle)?;
        replica.device_id = Uuid::new_v4();
        let actor = replica.actor();
        replica.catalog.set_actor(actor.clone());
        for document in replica.documents.values_mut() {
            document.set_actor(actor.clone());
        }
        Ok(replica)
    }
    pub fn vault_id(&self) -> Uuid {
        self.vault_id
    }
    pub fn device_id(&self) -> Uuid {
        self.device_id
    }
    fn actor(&self) -> ActorId {
        ActorId::from(self.device_id.as_bytes().to_vec())
    }
    pub fn save(&mut self) -> HistoryBundle {
        let mut documents: Vec<_> = self
            .documents
            .iter_mut()
            .map(|(id, document)| DocumentHistory {
                id: *id,
                bytes: document.save(),
            })
            .collect();
        documents.sort_by_key(|history| history.id.0);
        HistoryBundle {
            version: 1,
            vault_id: self.vault_id,
            device_id: self.device_id,
            catalog: self.catalog.save(),
            documents,
        }
    }
    pub(crate) fn insert(&mut self, note: &NoteRecord) -> Result<(), MergeError> {
        if self.documents.contains_key(&note.id) || !valid_path(&note.path) {
            return Err(MergeError::Invalid(
                "duplicate identity or invalid path".into(),
            ));
        }
        let mut document = new_document(self.device_id);
        document.put(ROOT, "note_id", note.id.0.to_string())?;
        document.put(ROOT, "path", note.path.clone())?;
        document.put(ROOT, "trashed", note.trashed)?;
        let text = document.put_object(ROOT, "text", ObjType::Text)?;
        document.splice_text(&text, 0, 0, &note.text)?;
        let notes = object(&self.catalog, ROOT, "notes", ObjType::Map)?;
        self.catalog.put(&notes, note.id.0.to_string(), true)?;
        self.documents.insert(note.id, document);
        Ok(())
    }
    /// Called only after App revision/policy validation. All byte offsets refer
    /// to the same original source; map them to Unicode scalar counts then apply
    /// from the end. Combining marks and astral characters count separately.
    pub(crate) fn edit(&mut self, id: NoteId, edits: &[TextEdit]) -> Result<(), MergeError> {
        let document = self.documents.get_mut(&id).ok_or(MergeError::NotFound)?;
        let text = object(document, ROOT, "text", ObjType::Text)?;
        let source = document.text(&text)?;
        let mut edits = edits.to_vec();
        edited_text(&source, &mut edits).ok_or(MergeError::InvalidEdits)?;
        for edit in edits.iter().rev() {
            let position = source[..edit.start].chars().count();
            let deleted = source[edit.start..edit.end].chars().count();
            let deleted = isize::try_from(deleted).map_err(|_| MergeError::InvalidEdits)?;
            document.splice_text(&text, position, deleted, &edit.replacement)?;
        }
        Ok(())
    }
    pub(crate) fn replace_text(&mut self, id: NoteId, replacement: &str) -> Result<(), MergeError> {
        let document = self.documents.get(&id).ok_or(MergeError::NotFound)?;
        let text = object(document, ROOT, "text", ObjType::Text)?;
        let source = document.text(text)?;
        let prefix = source
            .chars()
            .zip(replacement.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        let suffix = source[prefix..]
            .chars()
            .rev()
            .zip(replacement[prefix..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        if source != replacement {
            self.edit(
                id,
                &[TextEdit {
                    start: prefix,
                    end: source.len() - suffix,
                    replacement: replacement[prefix..replacement.len() - suffix].into(),
                }],
            )?;
        }
        Ok(())
    }
    pub(crate) fn rename(&mut self, id: NoteId, path: &str) -> Result<(), MergeError> {
        if !valid_path(path) {
            return Err(MergeError::Invalid("invalid path".into()));
        }
        self.documents
            .get_mut(&id)
            .ok_or(MergeError::NotFound)?
            .put(ROOT, "path", path)?;
        Ok(())
    }
    pub(crate) fn trash(&mut self, id: NoteId, trashed: bool) -> Result<(), MergeError> {
        self.documents
            .get_mut(&id)
            .ok_or(MergeError::NotFound)?
            .put(ROOT, "trashed", trashed)?;
        Ok(())
    }
    pub fn merge(&mut self, incoming: &HistoryBundle) -> Result<(), MergeError> {
        if incoming.vault_id != self.vault_id {
            return Err(MergeError::DifferentVault);
        }
        let mut remote = Self::load(incoming)?;
        if object(&self.catalog, ROOT, "notes", ObjType::Map)?
            != object(&remote.catalog, ROOT, "notes", ObjType::Map)?
        {
            return Err(MergeError::Invalid(
                "catalog was independently bootstrapped".into(),
            ));
        }
        let mut candidate = self.clone();
        candidate.catalog.merge(&mut remote.catalog)?;
        for (id, mut document) in remote.documents {
            if let Some(local) = candidate.documents.get_mut(&id) {
                if object(local, ROOT, "text", ObjType::Text)?
                    != object(&document, ROOT, "text", ObjType::Text)?
                {
                    return Err(MergeError::Invalid(
                        "note was independently bootstrapped".into(),
                    ));
                }
                local.merge(&mut document)?;
            } else {
                document.set_actor(candidate.actor());
                candidate.documents.insert(id, document);
            }
        }
        candidate.project()?;
        *self = candidate;
        Ok(())
    }
    pub fn project(&self) -> Result<Vec<ProjectedNote>, MergeError> {
        let membership = object(&self.catalog, ROOT, "notes", ObjType::Map)?;
        let mut ids = HashSet::new();
        let mut notes = Vec::new();
        for key in self.catalog.keys(&membership) {
            let id = NoteId(
                Uuid::parse_str(&key)
                    .map_err(|_| MergeError::Invalid("invalid catalog identity".into()))?,
            );
            if key != id.0.to_string() || !ids.insert(id) {
                return Err(MergeError::Invalid(
                    "noncanonical or duplicate catalog identity".into(),
                ));
            }
            let document = self
                .documents
                .get(&id)
                .ok_or_else(|| MergeError::Invalid("catalog document missing".into()))?;
            let mut paths: Vec<_> = document
                .get_all(ROOT, "path")?
                .into_iter()
                .map(|(value, _)| {
                    value
                        .to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| MergeError::Invalid("path is not text".into()))
                })
                .collect::<Result<_, _>>()?;
            paths.sort();
            paths.dedup();
            if paths.is_empty() || paths.iter().any(|path| !valid_path(path)) {
                return Err(MergeError::Invalid("invalid path history".into()));
            }
            let mut tombstones: Vec<_> = document
                .get_all(ROOT, "trashed")?
                .into_iter()
                .map(|(value, _)| match value {
                    Value::Scalar(value) => match value.as_ref() {
                        ScalarValue::Boolean(value) => Ok(*value),
                        _ => Err(MergeError::Invalid("invalid tombstone".into())),
                    },
                    _ => Err(MergeError::Invalid("invalid tombstone".into())),
                })
                .collect::<Result<_, _>>()?;
            tombstones.sort();
            tombstones.dedup();
            if tombstones.is_empty() {
                return Err(MergeError::Invalid("missing tombstone".into()));
            }
            let text = document.text(object(document, ROOT, "text", ObjType::Text)?)?;
            notes.push(ProjectedNote {
                id,
                text,
                path: paths[0].clone(),
                trashed: tombstones.contains(&true),
                conflicts: MergeConflicts {
                    paths,
                    tombstones,
                    resolved_path: false,
                },
            });
        }
        if ids.len() != self.documents.len() {
            return Err(MergeError::Invalid(
                "document outside shared catalog".into(),
            ));
        }
        notes.sort_by_key(|note| note.id.0);
        let desired: Vec<_> = notes.iter().map(|note| note.path.clone()).collect();
        let mut assigned: Vec<String> = Vec::new();
        for note in &mut notes {
            if assigned.iter().any(|path| paths_collide(path, &note.path)) {
                let mut attempt = 0;
                loop {
                    let path = format!("recovered-{}-{attempt}.md", note.id.0);
                    if !desired
                        .iter()
                        .chain(assigned.iter())
                        .any(|other| paths_collide(other, &path))
                    {
                        note.path = path;
                        note.conflicts.resolved_path = true;
                        break;
                    }
                    attempt += 1;
                }
            }
            assigned.push(note.path.clone());
        }
        Ok(notes)
    }
}
fn paths_collide(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}
fn new_document(device_id: Uuid) -> AutoCommit {
    AutoCommit::new_with_encoding(TextEncoding::UnicodeCodePoint)
        .with_actor(ActorId::from(device_id.as_bytes().to_vec()))
}
fn load_document(bytes: &[u8], device_id: Uuid) -> Result<AutoCommit, MergeError> {
    let mut document = AutoCommit::load_with_options(
        bytes,
        LoadOptions::new().text_encoding(TextEncoding::UnicodeCodePoint),
    )?;
    document.set_actor(ActorId::from(device_id.as_bytes().to_vec()));
    Ok(document)
}
fn scalar_string(
    document: &AutoCommit,
    object: impl AsRef<ObjId>,
    key: &str,
) -> Result<String, MergeError> {
    document
        .get(object, key)?
        .and_then(|(value, _)| value.to_str().map(str::to_owned))
        .ok_or_else(|| MergeError::Invalid(format!("missing string {key}")))
}
fn object(
    document: &AutoCommit,
    parent: impl AsRef<ObjId>,
    key: &str,
    kind: ObjType,
) -> Result<ObjId, MergeError> {
    match document.get(parent, key)? {
        Some((Value::Object(actual), id)) if actual == kind => Ok(id),
        _ => Err(MergeError::Invalid(format!("missing object {key}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::Revision;
    #[test]
    fn offline_unicode_rename_delete_conflicts_converge_after_history_restart() {
        let id = NoteId(Uuid::new_v4());
        let second = NoteId(Uuid::new_v4());
        let make = |id, path: &str, text: &str| NoteRecord {
            id,
            path: path.into(),
            text: text.into(),
            revision: Revision::fresh(),
            trashed: false,
        };
        let mut left = Replica::bootstrap(&[
            make(id, "a.md", "α 🦀 é\r\n"),
            make(second, "b.md", "other"),
        ])
        .unwrap();
        let seed = left.save();
        let mut right = Replica::join(&seed).unwrap();
        assert_ne!(left.device_id(), right.device_id());
        left.edit(
            id,
            &[TextEdit {
                start: 3,
                end: 7,
                replacement: "L🦀".into(),
            }],
        )
        .unwrap();
        right
            .edit(
                id,
                &[TextEdit {
                    start: 0,
                    end: 2,
                    replacement: "β".into(),
                }],
            )
            .unwrap();
        left.rename(id, "left.md").unwrap();
        right.rename(id, "right.md").unwrap();
        left.trash(id, true).unwrap();
        right.trash(id, true).unwrap();
        right.trash(id, false).unwrap();
        right.rename(second, "left.md").unwrap();
        let left_saved = left.save();
        let right_saved = right.save();
        let mut left = Replica::load(&left_saved).unwrap();
        let mut right = Replica::load(&right_saved).unwrap();
        assert_eq!(left.device_id(), left_saved.device_id);
        assert_eq!(
            left.documents[&id].text_encoding(),
            TextEncoding::UnicodeCodePoint
        );
        left.merge(&right_saved).unwrap();
        right.merge(&left_saved).unwrap();
        assert_eq!(left.project().unwrap(), right.project().unwrap());
        let projected = left.project().unwrap();
        let merged = projected.iter().find(|note| note.id == id).unwrap();
        assert_eq!(merged.text, "β L🦀 é\r\n");
        assert!(merged.trashed);
        assert_eq!(merged.conflicts.paths, vec!["left.md", "right.md"]);
        assert_eq!(merged.conflicts.tombstones, vec![false, true]);
        assert_ne!(projected[0].path, projected[1].path);
        assert!(projected.iter().any(|note| note.conflicts.resolved_path));
        let snapshot = left.save();
        let mut restarted = Replica::load(&snapshot).unwrap();
        assert_eq!(restarted.project().unwrap(), projected);
        // An alias of an existing UUID must not project a second note/entity.
        let mut malformed = Replica::load(&snapshot).unwrap();
        let membership = object(&malformed.catalog, ROOT, "notes", ObjType::Map).unwrap();
        malformed
            .catalog
            .put(membership, id.0.simple().to_string(), true)
            .unwrap();
        let malformed = malformed.save();
        let before = restarted.save();
        assert!(matches!(
            restarted.merge(&malformed),
            Err(MergeError::Invalid(_))
        ));
        assert_eq!(restarted.project().unwrap(), projected);
        let after = restarted.save();
        assert_eq!(after.catalog, before.catalog);
        assert_eq!(
            after
                .documents
                .iter()
                .map(|document| (&document.id, &document.bytes))
                .collect::<Vec<_>>(),
            before
                .documents
                .iter()
                .map(|document| (&document.id, &document.bytes))
                .collect::<Vec<_>>()
        );
        // Sequential restore resolves observed tombstones, preserving merged text.
        restarted.trash(id, false).unwrap();
        restarted.replace_text(id, "restored 🦀").unwrap();
        let restored = restarted
            .project()
            .unwrap()
            .into_iter()
            .find(|note| note.id == id)
            .unwrap();
        assert!(!restored.trashed);
        assert_eq!(restored.text, "restored 🦀");
        let mut unrelated = Replica::bootstrap(&[]).unwrap();
        assert!(matches!(
            unrelated.merge(&snapshot),
            Err(MergeError::DifferentVault)
        ));
    }
}
