use crate::store::{BatchSubmitError, SaveAck, Store, StoreError};
use crate::{
    commands::{Caller, Command, CommandError, CommandResult},
    notes::{NoteId, NoteRecord, NoteView, Revision, SaveState, edited_text, valid_path},
};
use bevy_ecs::prelude::{Entity, World};
use std::collections::HashMap;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// The application thread exclusively owns the ECS World.
#[derive(Default)]
pub struct App {
    world: World,
    ids: HashMap<NoteId, Entity>,
    store: Option<Store>,
    pending: HashMap<NoteId, NoteRecord>,
    inflight: HashMap<NoteId, Revision>,
}
impl App {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn open(directory: &Path) -> Result<Self, StoreError> {
        let (store, records) = Store::open(directory)?;
        let mut app = Self {
            store: Some(store),
            ..Self::new()
        };
        for record in records {
            app.check_path(&record.path, None)
                .map_err(|error| StoreError::Storage(error.to_string()))?;
            if app.ids.contains_key(&record.id) {
                return Err(StoreError::Storage("duplicate note identity".into()));
            }
            let id = record.id;
            let revision = record.revision;
            let entity = app
                .world
                .spawn((
                    record,
                    SaveState {
                        accepted: revision,
                        persisted: Some(revision),
                        error: None,
                    },
                ))
                .id();
            app.ids.insert(id, entity);
        }
        Ok(app)
    }
    pub fn note(&self, id: NoteId) -> Result<NoteView, CommandError> {
        self.view(id)
    }
    pub fn notes(&self) -> Vec<NoteView> {
        let mut notes: Vec<_> = self
            .ids
            .keys()
            .filter_map(|id| self.view(*id).ok())
            .collect();
        notes.sort_by(|a, b| a.note.path.cmp(&b.note.path));
        notes
    }
    /// Drain and submit a bounded amount of work without blocking the owner.
    pub fn tick(&mut self) {
        for _ in 0..32 {
            let ack = self.store.as_ref().and_then(Store::try_recv);
            let Some(ack) = ack else {
                break;
            };
            self.apply_ack(ack);
        }
        // A rename cycle must never be split across save transactions, and a
        // later batch must not overtake the paths owned by an earlier batch.
        if !self.inflight.is_empty() || self.pending.is_empty() {
            return;
        }
        if self
            .pending
            .keys()
            .any(|id| self.view(*id).is_ok_and(|v| v.save.error.is_some()))
        {
            return;
        }
        let Some(store) = &self.store else {
            return;
        };
        let records: Vec<_> = self.pending.values().cloned().collect();
        match store.try_save_batch(records) {
            Ok(()) => {
                for (id, record) in self.pending.drain() {
                    self.inflight.insert(id, record.revision);
                }
            }
            Err(BatchSubmitError::Full(_)) => {}
            Err(BatchSubmitError::Disconnected(_)) => {
                for id in self.pending.keys() {
                    if let Some(mut save) = self.world.get_mut::<SaveState>(self.ids[id]) {
                        save.error = Some("persistence worker disconnected".into());
                    }
                }
            }
        }
    }

    fn apply_ack(&mut self, ack: SaveAck) {
        if self.inflight.get(&ack.id) != Some(&ack.revision) {
            return;
        }
        self.inflight.remove(&ack.id);
        let Some(&entity) = self.ids.get(&ack.id) else {
            return;
        };
        match ack.result {
            Ok(()) => {
                if let Some(mut save) = self.world.get_mut::<SaveState>(entity) {
                    save.persisted = Some(ack.revision);
                    save.error = None;
                }
            }
            Err(error) => {
                if let Some(mut save) = self.world.get_mut::<SaveState>(entity) {
                    save.error = Some(error);
                }
                if let Some(record) = self.world.get::<NoteRecord>(entity) {
                    self.pending.insert(ack.id, record.clone());
                }
            }
        }
    }
    pub fn retry_saves(&mut self) {
        for entity in self.ids.values() {
            if let Some(mut save) = self.world.get_mut::<SaveState>(*entity) {
                save.error = None;
            }
        }
        self.tick();
    }
    /// Explicit save/shutdown boundary. Failure preserves accepted dirty data.
    pub fn flush(&mut self) -> Result<(), StoreError> {
        if self.store.is_none() {
            return Err(StoreError::Storage(
                "memory-only app has no persistence".into(),
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.tick();
            if let Some(error) = self.notes().into_iter().find_map(|view| view.save.error) {
                return Err(StoreError::Storage(error));
            }
            if self.pending.is_empty() && self.inflight.is_empty() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(StoreError::Storage(
                    "save timed out; accepted changes remain dirty".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    /// Close storage only after all accepted changes are acknowledged. On error
    /// keep the application and dirty records alive for recovery or retry.
    pub fn shutdown(&mut self) -> Result<(), StoreError> {
        self.flush()?;
        self.store.take();
        Ok(())
    }
    pub fn execute(
        &mut self,
        command: Command,
        caller: Caller,
    ) -> Result<CommandResult, CommandError> {
        if !command.descriptor().allowed(caller) {
            return Err(CommandError::Unauthorized);
        }
        match command {
            Command::CreateNote { path, text } => {
                self.check_path(&path, None)?;
                let id = NoteId(Uuid::new_v4());
                let revision = Revision::fresh();
                let record = NoteRecord {
                    id,
                    path,
                    text,
                    revision,
                    trashed: false,
                };
                let entity = self
                    .world
                    .spawn((
                        record,
                        SaveState {
                            accepted: revision,
                            persisted: None,
                            error: None,
                        },
                    ))
                    .id();
                self.ids.insert(id, entity);
                self.pending.insert(id, self.view(id)?.note);
                Ok(CommandResult::Note(self.view(id)?))
            }
            Command::ReadNote { id } => Ok(CommandResult::Note(self.view(id)?)),
            Command::SearchNotes {
                query,
                include_trash,
            } => {
                let query = query.to_lowercase();
                let mut notes: Vec<_> = self
                    .ids
                    .keys()
                    .filter_map(|id| self.view(*id).ok())
                    .filter(|v| {
                        (include_trash || !v.note.trashed)
                            && (v.note.path.to_lowercase().contains(&query)
                                || v.note.text.to_lowercase().contains(&query))
                    })
                    .collect();
                notes.sort_by(|a, b| a.note.path.cmp(&b.note.path));
                Ok(CommandResult::Notes(notes))
            }
            mutation => {
                let (id, expected) = match &mutation {
                    Command::EditNote { id, expected, .. }
                    | Command::RenameNote { id, expected, .. }
                    | Command::TrashNote { id, expected }
                    | Command::RestoreNote { id, expected } => (*id, *expected),
                    _ => unreachable!(),
                };
                let mut view = self.view(id)?;
                if view.note.revision != expected {
                    return Err(CommandError::StaleRevision {
                        current: view.note.revision,
                    });
                }
                match mutation {
                    Command::EditNote { mut edits, .. } => {
                        if view.note.trashed {
                            return Err(CommandError::Trashed);
                        }
                        view.note.text = edited_text(&view.note.text, &mut edits)
                            .ok_or(CommandError::InvalidEdits)?;
                    }
                    Command::RenameNote { path, .. } => {
                        self.check_path(&path, Some(id))?;
                        view.note.path = path;
                    }
                    Command::TrashNote { .. } => view.note.trashed = true,
                    Command::RestoreNote { .. } => view.note.trashed = false,
                    _ => unreachable!(),
                }
                let revision = Revision::fresh();
                view.note.revision = revision;
                view.save.accepted = revision;
                view.save.error = None;
                let entity = self.ids[&id];
                self.world
                    .entity_mut(entity)
                    .insert((view.note.clone(), view.save.clone()));
                self.pending.insert(id, view.note.clone());
                Ok(CommandResult::Note(view))
            }
        }
    }
    fn view(&self, id: NoteId) -> Result<NoteView, CommandError> {
        let entity = *self.ids.get(&id).ok_or(CommandError::NotFound)?;
        Ok(NoteView {
            note: self
                .world
                .get::<NoteRecord>(entity)
                .ok_or(CommandError::NotFound)?
                .clone(),
            save: self
                .world
                .get::<SaveState>(entity)
                .ok_or(CommandError::NotFound)?
                .clone(),
        })
    }
    fn check_path(&self, path: &str, except: Option<NoteId>) -> Result<(), CommandError> {
        if !valid_path(path) {
            return Err(CommandError::InvalidPath);
        }
        // Reserve paths even in trash: restoring must never overwrite another note.
        if self.ids.keys().any(|id| {
            Some(*id) != except
                && self.view(*id).is_ok_and(|v| {
                    v.note.path == path
                        || v.note.path.starts_with(&format!("{path}/"))
                        || path.starts_with(&format!("{}/", v.note.path))
                })
        }) {
            return Err(CommandError::PathCollision);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::TextEdit;
    fn note(result: CommandResult) -> NoteView {
        match result {
            CommandResult::Note(v) => v,
            _ => panic!("expected note"),
        }
    }
    #[test]
    fn coalesced_rename_cycles_over_queue_capacity_persist_atomically() {
        let directory = std::env::temp_dir().join(format!("tanglewood-swap-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let mut notes = Vec::new();
        for n in 0..40 {
            notes.push(note(
                app.execute(
                    Command::CreateNote {
                        path: format!("{n}.md"),
                        text: format!("note {n}"),
                    },
                    Caller::Local,
                )
                .unwrap(),
            ));
        }
        app.flush().unwrap();
        // Open the first path, rotate >32 participants, close the cycle before tick.
        let moved = note(
            app.execute(
                Command::RenameNote {
                    id: notes[0].note.id,
                    expected: notes[0].note.revision,
                    path: "temporary.md".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        );
        for n in 1..40 {
            app.execute(
                Command::RenameNote {
                    id: notes[n].note.id,
                    expected: notes[n].note.revision,
                    path: format!("{}.md", n - 1),
                },
                Caller::Local,
            )
            .unwrap();
        }
        app.execute(
            Command::RenameNote {
                id: moved.note.id,
                expected: moved.note.revision,
                path: "39.md".into(),
            },
            Caller::Local,
        )
        .unwrap();
        app.flush().unwrap();
        let expected = app.notes();
        app.shutdown().unwrap();
        drop(app);
        let app = App::open(&directory).unwrap();
        assert_eq!(app.notes(), expected);
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn acknowledged_restart_and_stale_ack_keep_newer_edits_dirty() {
        let directory = std::env::temp_dir().join(format!("tanglewood-app-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let initial = note(
            app.execute(
                Command::CreateNote {
                    path: "a.md".into(),
                    text: "old".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        );
        app.tick();
        let newer = note(
            app.execute(
                Command::EditNote {
                    id: initial.note.id,
                    expected: initial.note.revision,
                    edits: vec![TextEdit {
                        start: 0,
                        end: 3,
                        replacement: "new 🦀".into(),
                    }],
                },
                Caller::Local,
            )
            .unwrap(),
        );
        // Wait for the real older commit, then inspect before submitting newer work.
        let ack = app.store.as_ref().unwrap().flush().unwrap().pop().unwrap();
        app.apply_ack(ack);
        let dirty = app.note(initial.note.id).unwrap();
        assert_eq!(dirty.save.persisted, Some(initial.note.revision));
        assert_ne!(dirty.save.persisted, Some(dirty.save.accepted));
        app.flush().unwrap();
        let saved = app.note(initial.note.id).unwrap();
        assert_eq!(saved.save.persisted, Some(newer.note.revision));
        // No unfinished changes remain at orderly shutdown.
        app.shutdown().unwrap();
        drop(app);
        let app = App::open(&directory).unwrap();
        let reloaded = app.note(initial.note.id).unwrap();
        assert_eq!(reloaded.note, newer.note);
        assert_eq!(reloaded.save.persisted, Some(newer.note.revision));
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn command_scenario_preserves_source_and_rejects_invalid_writes() {
        let mut app = App::new();
        let first = note(
            app.execute(
                Command::CreateNote {
                    path: "a.md".into(),
                    text: "α beta 🦀\r\n".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        );
        assert_eq!(first.save.persisted, None);
        let id = first.note.id;
        let edit = |expected, start, end| Command::EditNote {
            id,
            expected,
            edits: vec![TextEdit {
                start,
                end,
                replacement: "B".into(),
            }],
        };
        assert_eq!(
            app.execute(edit(first.note.revision, 1, 2), Caller::Local)
                .unwrap_err(),
            CommandError::InvalidEdits
        );
        assert_eq!(
            app.execute(edit(first.note.revision, 3, 7), Caller::AutomationReadOnly)
                .unwrap_err(),
            CommandError::Unauthorized
        );
        assert_eq!(
            app.execute(
                Command::EditNote {
                    id,
                    expected: first.note.revision,
                    edits: vec![
                        TextEdit {
                            start: 3,
                            end: 7,
                            replacement: "x".into()
                        },
                        TextEdit {
                            start: 4,
                            end: 5,
                            replacement: "y".into()
                        }
                    ]
                },
                Caller::Local
            )
            .unwrap_err(),
            CommandError::InvalidEdits
        );
        let edited = note(
            app.execute(edit(first.note.revision, 3, 7), Caller::Local)
                .unwrap(),
        );
        assert_eq!(edited.note.text, "α B 🦀\r\n");
        assert!(matches!(
            app.execute(edit(first.note.revision, 3, 4), Caller::Local),
            Err(CommandError::StaleRevision { .. })
        ));
        assert_eq!(
            app.execute(
                Command::RenameNote {
                    id,
                    expected: edited.note.revision,
                    path: "../bad.md".into()
                },
                Caller::Local
            )
            .unwrap_err(),
            CommandError::InvalidPath
        );
        let renamed = note(
            app.execute(
                Command::RenameNote {
                    id,
                    expected: edited.note.revision,
                    path: "folder/b.md".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        );
        assert_eq!(
            app.execute(
                Command::CreateNote {
                    path: "folder/b.md".into(),
                    text: "".into()
                },
                Caller::Local
            )
            .unwrap_err(),
            CommandError::PathCollision
        );
        assert_eq!(
            app.execute(
                Command::CreateNote {
                    path: "folder/b.md/nested.md".into(),
                    text: String::new()
                },
                Caller::Local
            )
            .unwrap_err(),
            CommandError::PathCollision
        );
        let trashed = note(
            app.execute(
                Command::TrashNote {
                    id,
                    expected: renamed.note.revision,
                },
                Caller::Local,
            )
            .unwrap(),
        );
        assert!(
            matches!(app.execute(Command::SearchNotes { query: "B".into(), include_trash: false }, Caller::Local).unwrap(), CommandResult::Notes(v) if v.is_empty())
        );
        let restored = note(
            app.execute(
                Command::RestoreNote {
                    id,
                    expected: trashed.note.revision,
                },
                Caller::Local,
            )
            .unwrap(),
        );
        assert_eq!(restored.note.id, id);
        assert_eq!(restored.note.text, edited.note.text);
        assert_eq!(restored.save.persisted, None);
        assert!(
            matches!(app.execute(Command::SearchNotes { query: "🦀".into(), include_trash: false }, Caller::Local).unwrap(), CommandResult::Notes(v) if v.len() == 1)
        );
    }
}
