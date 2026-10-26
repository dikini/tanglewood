use crate::files::{
    ExportBaseline, ExportJob, FileCompletion, FileError, FileJob, FileWorker, Reconcile, reconcile,
};
use crate::store::{
    BatchSubmitError, MirrorConflict, SaveAck, Store, StoreError, StoreEvent, StoreRequest,
};
use crate::{
    commands::{Caller, Command, CommandError, CommandResult, ImportDestination},
    notes::{NoteId, NoteRecord, NoteView, Revision, SaveState, TextEdit, edited_text, valid_path},
};
use bevy_ecs::prelude::{Entity, World};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::TrySendError;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IoStatus {
    Pending,
    Completed,
    Failed(String),
    Conflict(i64),
}
#[derive(Clone, Debug)]
pub struct ExportStatus {
    pub pending: bool,
    pub error: Option<String>,
}
enum PendingIo {
    Configure,
    Import(ImportDestination),
    Export,
    Rescan {
        notes: Vec<NoteView>,
        baselines: HashMap<NoteId, ExportBaseline>,
    },
}

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
    // Join file work before the following store releases the vault lock.
    files: Option<FileWorker>,
    store: Option<Store>,
    pending: HashMap<NoteId, NoteRecord>,
    inflight: HashMap<NoteId, Revision>,
    mirror_enabled: bool,
    baselines: HashMap<NoteId, ExportBaseline>,
    exports: HashMap<NoteId, NoteRecord>,
    export_inflight: HashMap<NoteId, NoteRecord>,
    export_errors: HashMap<NoteId, String>,
    control_inflight: usize,
    conflicts: Vec<MirrorConflict>,
    storage_jobs: VecDeque<StoreRequest>,
    file_jobs: VecDeque<FileJob>,
    io_pending: HashMap<Uuid, PendingIo>,
    io_status: HashMap<Uuid, IoStatus>,
}
impl App {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn open(directory: &Path) -> Result<Self, StoreError> {
        let (store, state) = Store::open_state(directory)?;
        let mut app = Self {
            store: Some(store),
            files: Some(
                FileWorker::open(&directory.join("markdown"))
                    .map_err(|error| StoreError::Storage(error.to_string()))?,
            ),
            mirror_enabled: state.mirror_enabled,
            baselines: state.baselines,
            exports: state
                .pending
                .into_iter()
                .map(|note| (note.id, note))
                .collect(),
            conflicts: state.conflicts,
            ..Self::new()
        };
        for record in state.notes {
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
    pub fn io_status(&self, request_id: Uuid) -> Option<&IoStatus> {
        self.io_status.get(&request_id)
    }
    pub fn mirror_enabled(&self) -> bool {
        self.mirror_enabled
    }
    pub fn mirror_conflicts(&self) -> &[MirrorConflict] {
        &self.conflicts
    }
    pub fn export_status(&self, id: NoteId) -> ExportStatus {
        ExportStatus {
            pending: self.exports.contains_key(&id) || self.export_inflight.contains_key(&id),
            error: self.export_errors.get(&id).cloned(),
        }
    }
    fn apply_store_event(&mut self, event: StoreEvent) {
        if !matches!(event, StoreEvent::Saved(_)) {
            self.control_inflight = self.control_inflight.saturating_sub(1);
        }
        match event {
            StoreEvent::Saved(ack) => self.apply_ack(ack),
            StoreEvent::Configured {
                request_id,
                enabled,
                result,
            } => {
                self.io_pending.remove(&request_id);
                match result {
                    Ok(pending) => {
                        self.mirror_enabled = enabled;
                        for note in pending {
                            self.exports.insert(note.id, note);
                        }
                        self.io_status.insert(request_id, IoStatus::Completed);
                    }
                    Err(error) => {
                        self.io_status.insert(request_id, IoStatus::Failed(error));
                    }
                }
            }
            StoreEvent::ExportRecorded {
                note,
                baseline,
                result,
            } => {
                self.export_inflight.remove(&note.id);
                match result {
                    Ok(()) => {
                        match baseline {
                            Some(base) => {
                                self.baselines.insert(note.id, base);
                            }
                            None => {
                                self.baselines.remove(&note.id);
                            }
                        }
                        if self
                            .exports
                            .get(&note.id)
                            .is_some_and(|current| current.revision == note.revision)
                        {
                            self.exports.remove(&note.id);
                        }
                        self.export_errors.remove(&note.id);
                    }
                    Err(error) => {
                        self.export_errors.insert(note.id, error);
                    }
                }
            }
            StoreEvent::ConflictRecorded {
                request_id,
                mut conflict,
                result,
            } => {
                self.export_inflight.remove(&conflict.note.id);
                match result {
                    Ok(id) => {
                        conflict.conflict_id = id;
                        self.export_errors.insert(
                            conflict.note.id,
                            format!("external conflict retained as {id}"),
                        );
                        if !self.conflicts.iter().any(|old| old.conflict_id == id) {
                            self.conflicts.push(conflict);
                        }
                        if let Some(request) = request_id {
                            self.io_status.insert(request, IoStatus::Conflict(id));
                        }
                    }
                    Err(error) => {
                        self.export_errors.insert(conflict.note.id, error.clone());
                        if let Some(request) = request_id {
                            self.io_status.insert(request, IoStatus::Failed(error));
                        }
                    }
                }
            }
        }
    }
    fn retain_conflict(
        &mut self,
        note: NoteRecord,
        path: String,
        external: Option<String>,
        kind: &str,
        request_id: Option<Uuid>,
    ) {
        self.storage_jobs.push_back(StoreRequest::Conflict {
            request_id,
            conflict: MirrorConflict {
                conflict_id: 0,
                note,
                path,
                external,
                kind: kind.into(),
            },
        });
    }
    fn apply_file_event(&mut self, event: FileCompletion) {
        match event {
            FileCompletion::Export {
                id,
                revision,
                result,
            } => {
                let Some(note) = self
                    .export_inflight
                    .get(&id)
                    .filter(|note| note.revision == revision)
                    .cloned()
                else {
                    return;
                };
                match result {
                    Ok(baseline) => self
                        .storage_jobs
                        .push_back(StoreRequest::Exported { note, baseline }),
                    Err(FileError::Conflict { path, external }) => {
                        self.retain_conflict(note, path, external, "export", None)
                    }
                    Err(error) => {
                        self.export_inflight.remove(&id);
                        self.export_errors.insert(id, error.to_string());
                    }
                }
            }
            FileCompletion::ExplicitExport { request_id, result } => {
                self.io_pending.remove(&request_id);
                self.io_status.insert(
                    request_id,
                    match result {
                        Ok(()) => IoStatus::Completed,
                        Err(error) => IoStatus::Failed(error.to_string()),
                    },
                );
            }
            FileCompletion::Import { request_id, result } => {
                let Some(PendingIo::Import(destination)) = self.io_pending.remove(&request_id)
                else {
                    return;
                };
                let outcome = result.map_err(|error| error.to_string()).and_then(|text| {
                    let command = match destination {
                        ImportDestination::Create { path } => Command::CreateNote { path, text },
                        ImportDestination::Replace { id, expected } => {
                            let view = self.note(id).map_err(|error| error.to_string())?;
                            Command::EditNote {
                                id,
                                expected,
                                edits: vec![TextEdit {
                                    start: 0,
                                    end: view.note.text.len(),
                                    replacement: text,
                                }],
                            }
                        }
                    };
                    self.execute(command, Caller::Local)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                });
                self.io_status.insert(
                    request_id,
                    match outcome {
                        Ok(()) => IoStatus::Completed,
                        Err(error) => IoStatus::Failed(error),
                    },
                );
            }
            FileCompletion::Rescan { request_id, result } => {
                let Some(PendingIo::Rescan { notes, baselines }) =
                    self.io_pending.remove(&request_id)
                else {
                    return;
                };
                match result {
                    Err(error) => {
                        self.io_status
                            .insert(request_id, IoStatus::Failed(error.to_string()));
                    }
                    Ok(files) => {
                        let mut observed: HashMap<_, _> = files
                            .into_iter()
                            .map(|file| (file.path, file.text))
                            .collect();
                        let mut conflicts = false;
                        for view in notes {
                            let note = view.note;
                            let base = baselines.get(&note.id);
                            // Associate old mirror path during a pending app rename too.
                            let physical = base
                                .map(|base| base.path.clone())
                                .unwrap_or_else(|| note.path.clone());
                            let external = observed.remove(&physical);
                            if physical != note.path {
                                observed.remove(&note.path);
                            }
                            if note.trashed {
                                continue;
                            }
                            match reconcile(&note, base, external.as_deref()) {
                                Reconcile::Unchanged => {}
                                Reconcile::Import(text) => {
                                    let command = Command::EditNote {
                                        id: note.id,
                                        expected: note.revision,
                                        edits: vec![TextEdit {
                                            start: 0,
                                            end: note.text.len(),
                                            replacement: text.clone(),
                                        }],
                                    };
                                    if self.execute(command, Caller::Local).is_err() {
                                        conflicts = true;
                                        self.retain_conflict(
                                            note,
                                            physical,
                                            Some(text),
                                            "rescan-stale",
                                            Some(request_id),
                                        );
                                    }
                                }
                                Reconcile::Conflict(external) => {
                                    conflicts = true;
                                    self.retain_conflict(
                                        note,
                                        physical,
                                        external,
                                        "rescan",
                                        Some(request_id),
                                    );
                                }
                                Reconcile::Missing => {
                                    // Missing exports retain both note and baseline; never trash source.
                                    if base.is_some() {
                                        conflicts = true;
                                        self.retain_conflict(
                                            note,
                                            physical,
                                            None,
                                            "missing",
                                            Some(request_id),
                                        );
                                    }
                                }
                            }
                        }
                        for (path, text) in observed {
                            if let Err(error) = self.execute(
                                Command::CreateNote {
                                    path: path.clone(),
                                    text: text.clone(),
                                },
                                Caller::Local,
                            ) {
                                if let Some(note) = self
                                    .notes()
                                    .into_iter()
                                    .find(|view| view.note.path == path)
                                    .map(|view| view.note)
                                {
                                    conflicts = true;
                                    self.retain_conflict(
                                        note,
                                        path,
                                        Some(text),
                                        "rescan-create",
                                        Some(request_id),
                                    );
                                } else {
                                    self.io_status
                                        .insert(request_id, IoStatus::Failed(error.to_string()));
                                    return;
                                }
                            }
                        }
                        if !conflicts {
                            self.io_status.insert(request_id, IoStatus::Completed);
                        }
                    }
                }
            }
        }
    }
    fn tick_files(&mut self) {
        for _ in 0..32 {
            let event = self.files.as_ref().and_then(FileWorker::try_recv);
            let Some(event) = event else {
                break;
            };
            self.apply_file_event(event);
        }
        for _ in 0..32 {
            let Some(request) = self.storage_jobs.pop_front() else {
                break;
            };
            let Some(store) = &self.store else {
                self.storage_jobs.push_front(request);
                break;
            };
            match store.try_request(request) {
                Ok(()) => {
                    self.control_inflight += 1;
                }
                Err(TrySendError::Full(request)) => {
                    self.storage_jobs.push_front(request);
                    break;
                }
                Err(TrySendError::Disconnected(request)) => {
                    self.storage_jobs.push_front(request);
                    break;
                }
            }
        }
        for _ in 0..16 {
            let Some(job) = self.file_jobs.pop_front() else {
                break;
            };
            let Some(files) = &self.files else {
                self.file_jobs.push_front(job);
                break;
            };
            match files.try_submit(job) {
                Ok(()) => {}
                Err(TrySendError::Full(job)) => {
                    self.file_jobs.push_front(job);
                    break;
                }
                Err(TrySendError::Disconnected(job)) => {
                    self.file_jobs.push_front(job);
                    break;
                }
            }
        }
        if !self.mirror_enabled
            || self
                .io_pending
                .values()
                .any(|pending| matches!(pending, PendingIo::Rescan { .. }))
        {
            return;
        }
        let ready: Vec<_> = self
            .exports
            .values()
            .filter(|note| {
                !self.export_inflight.contains_key(&note.id)
                    && !self.export_errors.contains_key(&note.id)
                    && !self.pending.contains_key(&note.id)
                    && !self.inflight.contains_key(&note.id)
                    && self
                        .note(note.id)
                        .is_ok_and(|current| current.note.revision == note.revision)
            })
            .take(16)
            .cloned()
            .collect();
        for note in ready {
            let Some(files) = &self.files else {
                break;
            };
            // Equal text does not establish file ownership. A target owned by
            // another baseline, or cleanup path claimed by another accepted
            // note, must never be replaced/removed as an idempotent retry.
            let conflict_path = self
                .baselines
                .iter()
                .find(|(id, base)| **id != note.id && base.path == note.path)
                .map(|(_, base)| base.path.clone())
                .or_else(|| {
                    self.notes()
                        .into_iter()
                        .find(|view| view.note.id != note.id && view.note.path == note.path)
                        .map(|view| view.note.path)
                })
                .or_else(|| {
                    self.baselines
                        .get(&note.id)
                        .filter(|base| base.path != note.path)
                        .and_then(|base| {
                            self.notes()
                                .into_iter()
                                .find(|view| view.note.id != note.id && view.note.path == base.path)
                                .map(|_| base.path.clone())
                        })
                });
            let job = match conflict_path {
                Some(path) => FileJob::RetainConflict {
                    note: note.clone(),
                    path,
                },
                None => FileJob::Export(ExportJob {
                    note: note.clone(),
                    baseline: self.baselines.get(&note.id).cloned(),
                }),
            };
            match files.try_submit(job) {
                Ok(()) => {
                    self.export_inflight.insert(note.id, note);
                }
                Err(_) => break,
            }
        }
    }
    /// Wait for mirror/export jobs independently from database-save status.
    pub fn flush_files(&mut self) -> Result<(), StoreError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.tick();
            if let Some(error) = self.export_errors.values().next() {
                return Err(StoreError::Storage(error.clone()));
            }
            if self.control_inflight == 0
                && self.storage_jobs.is_empty()
                && self.file_jobs.is_empty()
                && self.io_pending.is_empty()
                && self.export_inflight.is_empty()
                && (!self.mirror_enabled || self.exports.is_empty())
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(StoreError::Storage(
                    "file work timed out; pending exports retained".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
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
            self.apply_store_event(ack);
        }
        self.tick_files();
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
                if ack.export_pending {
                    self.exports.insert(ack.id, ack.note);
                }
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
        self.export_errors.clear();
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
        self.files.take();
        self.store.take();
        Ok(())
    }
    fn accept_io(&mut self) -> Result<Uuid, CommandError> {
        if self.store.is_none() || self.files.is_none() {
            return Err(CommandError::Unavailable(
                "filesystem worker requires an open vault".into(),
            ));
        }
        if self.io_pending.len() >= 64 {
            return Err(CommandError::Busy);
        }
        if self.io_status.len() >= 256 {
            if let Some(id) = self
                .io_status
                .iter()
                .find(|(_, status)| !matches!(status, IoStatus::Pending))
                .map(|(id, _)| *id)
            {
                self.io_status.remove(&id);
            }
        }
        let request_id = Uuid::new_v4();
        self.io_status.insert(request_id, IoStatus::Pending);
        Ok(request_id)
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
            Command::SetMirrorEnabled { enabled } => {
                let request_id = self.accept_io()?;
                #[cfg(not(target_os = "linux"))]
                if enabled {
                    return Err(CommandError::Unavailable(
                        "directory mirror requires Linux".into(),
                    ));
                }
                self.io_pending.insert(request_id, PendingIo::Configure);
                self.storage_jobs.push_back(StoreRequest::Configure {
                    request_id,
                    enabled,
                });
                Ok(CommandResult::Accepted { request_id })
            }
            Command::ImportMarkdown {
                source_path,
                destination,
            } => {
                match &destination {
                    ImportDestination::Create { path } => self.check_path(path, None)?,
                    ImportDestination::Replace { id, expected } => {
                        let note = self.note(*id)?;
                        if note.note.revision != *expected {
                            return Err(CommandError::StaleRevision {
                                current: note.note.revision,
                            });
                        }
                    }
                }
                let request_id = self.accept_io()?;
                self.io_pending
                    .insert(request_id, PendingIo::Import(destination));
                self.file_jobs.push_back(FileJob::Import {
                    request_id,
                    path: source_path.into(),
                });
                Ok(CommandResult::Accepted { request_id })
            }
            Command::ExportMarkdown { id, path } => {
                let note = self.note(id)?.note;
                let request_id = self.accept_io()?;
                self.io_pending.insert(request_id, PendingIo::Export);
                self.file_jobs.push_back(FileJob::ExplicitExport {
                    request_id,
                    note,
                    path: path.into(),
                });
                Ok(CommandResult::Accepted { request_id })
            }
            Command::RescanMirror => {
                if !self.mirror_enabled {
                    return Err(CommandError::Unavailable(
                        "directory mirror is disabled".into(),
                    ));
                }
                let request_id = self.accept_io()?;
                self.io_pending.insert(
                    request_id,
                    PendingIo::Rescan {
                        notes: self.notes(),
                        baselines: self.baselines.clone(),
                    },
                );
                self.file_jobs.push_back(FileJob::Rescan { request_id });
                Ok(CommandResult::Accepted { request_id })
            }
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
    fn request(app: &mut App, command: Command) -> Uuid {
        match app.execute(command, Caller::Local).unwrap() {
            CommandResult::Accepted { request_id } => request_id,
            _ => panic!("expected I/O request"),
        }
    }
    fn wait_io(app: &mut App, id: Uuid) -> IoStatus {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            app.tick();
            let status = app.io_status(id).unwrap().clone();
            if status != IoStatus::Pending {
                return status;
            }
            assert!(Instant::now() < deadline, "I/O request stuck");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn identical_text_mirror_swap_keeps_both_files_and_durable_conflicts() {
        let directory =
            std::env::temp_dir().join(format!("tanglewood-mirror-swap-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let mut created = Vec::new();
        for path in ["a.md", "b.md"] {
            created.push(note(
                app.execute(
                    Command::CreateNote {
                        path: path.into(),
                        text: "identical".into(),
                    },
                    Caller::Local,
                )
                .unwrap(),
            ));
        }
        app.flush().unwrap();
        let request = request(&mut app, Command::SetMirrorEnabled { enabled: true });
        assert_eq!(wait_io(&mut app, request), IoStatus::Completed);
        app.flush_files().unwrap();
        let a = created[0].note.id;
        let b = created[1].note.id;
        let temporary = note(
            app.execute(
                Command::RenameNote {
                    id: a,
                    expected: created[0].note.revision,
                    path: "temporary.md".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        );
        app.execute(
            Command::RenameNote {
                id: b,
                expected: created[1].note.revision,
                path: "a.md".into(),
            },
            Caller::Local,
        )
        .unwrap();
        app.execute(
            Command::RenameNote {
                id: a,
                expected: temporary.note.revision,
                path: "b.md".into(),
            },
            Caller::Local,
        )
        .unwrap();
        app.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.export_status(a).error.is_none() || app.export_status(b).error.is_none() {
            app.tick();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.export_status(a).pending && app.export_status(b).pending);
        assert!(app.flush_files().is_err());
        for path in ["a.md", "b.md"] {
            assert_eq!(
                std::fs::read_to_string(directory.join("markdown").join(path)).unwrap(),
                "identical"
            );
        }
        assert!(
            app.mirror_conflicts()
                .iter()
                .any(|conflict| conflict.note.id == a)
        );
        assert!(
            app.mirror_conflicts()
                .iter()
                .any(|conflict| conflict.note.id == b)
        );
        app.shutdown().unwrap();
        drop(app);
        let mut app = App::open(&directory).unwrap();
        assert!(app.export_status(a).pending && app.export_status(b).pending);
        assert_eq!(app.mirror_conflicts().len(), 2);
        for path in ["a.md", "b.md"] {
            assert!(directory.join("markdown").join(path).exists());
        }
        // Backend recovery uses ordinary revision-checked commands: move both
        // notes to fresh paths, retry, then move to the desired swapped paths.
        for (id, path) in [(a, "recovery-a.md"), (b, "recovery-b.md")] {
            let current = app.note(id).unwrap();
            app.execute(
                Command::RenameNote {
                    id,
                    expected: current.note.revision,
                    path: path.into(),
                },
                Caller::Local,
            )
            .unwrap();
        }
        app.retry_saves();
        app.flush().unwrap();
        app.flush_files().unwrap();
        for (id, path) in [(a, "b.md"), (b, "a.md")] {
            let current = app.note(id).unwrap();
            app.execute(
                Command::RenameNote {
                    id,
                    expected: current.note.revision,
                    path: path.into(),
                },
                Caller::Local,
            )
            .unwrap();
        }
        app.flush().unwrap();
        app.flush_files().unwrap();
        assert!(!app.export_status(a).pending && !app.export_status(b).pending);
        for path in ["a.md", "b.md"] {
            assert_eq!(
                std::fs::read_to_string(directory.join("markdown").join(path)).unwrap(),
                "identical"
            );
        }
        app.shutdown().unwrap();
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn mirror_import_conflict_rename_and_pending_restart_scenario() {
        let directory = std::env::temp_dir().join(format!("tanglewood-mirror-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        assert!(!app.mirror_enabled());
        let source = directory.join("source.md");
        std::fs::write(&source, "initial α\r\n").unwrap();
        let import = Command::ImportMarkdown {
            source_path: source.to_string_lossy().into(),
            destination: ImportDestination::Create {
                path: "a.md".into(),
            },
        };
        assert_eq!(
            app.execute(import.clone(), Caller::AutomationReadWrite)
                .unwrap_err(),
            CommandError::Unauthorized
        );
        let id = request(&mut app, import);
        assert_eq!(wait_io(&mut app, id), IoStatus::Completed);
        app.flush().unwrap();
        let original = app.notes()[0].clone();
        let id = original.note.id;
        let replacing = request(
            &mut app,
            Command::ImportMarkdown {
                source_path: source.to_string_lossy().into(),
                destination: ImportDestination::Replace {
                    id,
                    expected: original.note.revision,
                },
            },
        );
        app.execute(
            Command::EditNote {
                id,
                expected: original.note.revision,
                edits: vec![TextEdit {
                    start: 0,
                    end: 0,
                    replacement: "kept ".into(),
                }],
            },
            Caller::Local,
        )
        .unwrap();
        assert!(matches!(wait_io(&mut app, replacing), IoStatus::Failed(_)));
        let original = app.note(id).unwrap();
        assert!(original.note.text.starts_with("kept "));
        let explicit = directory.join("selected-export.md");
        let exporting = request(
            &mut app,
            Command::ExportMarkdown {
                id,
                path: explicit.to_string_lossy().into(),
            },
        );
        assert_eq!(wait_io(&mut app, exporting), IoStatus::Completed);
        assert_eq!(
            std::fs::read_to_string(explicit).unwrap(),
            original.note.text
        );
        app.flush().unwrap();
        let enabled = request(&mut app, Command::SetMirrorEnabled { enabled: true });
        assert_eq!(wait_io(&mut app, enabled), IoStatus::Completed);
        app.flush_files().unwrap();
        let file = directory.join("markdown/a.md");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original.note.text);
        std::fs::write(&file, "external 🦀").unwrap();
        let scan = request(&mut app, Command::RescanMirror);
        assert_eq!(wait_io(&mut app, scan), IoStatus::Completed);
        app.flush().unwrap();
        app.flush_files().unwrap();
        let imported = app.note(id).unwrap();
        assert_eq!(imported.note.text, "external 🦀");
        app.execute(
            Command::EditNote {
                id,
                expected: imported.note.revision,
                edits: vec![TextEdit {
                    start: 0,
                    end: imported.note.text.len(),
                    replacement: "internal".into(),
                }],
            },
            Caller::Local,
        )
        .unwrap();
        std::fs::write(&file, "external conflict").unwrap();
        let scan = request(&mut app, Command::RescanMirror);
        assert!(matches!(wait_io(&mut app, scan), IoStatus::Conflict(_)));
        app.flush().unwrap();
        assert_eq!(
            app.note(id).unwrap().save.persisted,
            Some(app.note(id).unwrap().note.revision)
        );
        assert!(app.export_status(id).error.is_some());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "external conflict");
        app.shutdown().unwrap();
        drop(app);
        let mut app = App::open(&directory).unwrap();
        assert!(
            app.mirror_conflicts()
                .iter()
                .any(|conflict| conflict.note.text == "internal"
                    && conflict.external.as_deref() == Some("external conflict"))
        );
        assert_eq!(app.note(id).unwrap().note.text, "internal");
        // Explicit recovery keeps the durable conflict history and retries export.
        std::fs::write(&file, "internal").unwrap();
        app.retry_saves();
        app.flush_files().unwrap();
        let current = app.note(id).unwrap();
        app.execute(
            Command::RenameNote {
                id,
                expected: current.note.revision,
                path: "folder/b.md".into(),
            },
            Caller::Local,
        )
        .unwrap();
        app.flush().unwrap();
        app.flush_files().unwrap();
        assert!(!file.exists());
        let renamed = directory.join("markdown/folder/b.md");
        assert_eq!(std::fs::read_to_string(&renamed).unwrap(), "internal");
        let current = app.note(id).unwrap();
        app.execute(
            Command::TrashNote {
                id,
                expected: current.note.revision,
            },
            Caller::Local,
        )
        .unwrap();
        app.flush().unwrap();
        app.flush_files().unwrap();
        assert!(!renamed.exists());
        let current = app.note(id).unwrap();
        app.execute(
            Command::RestoreNote {
                id,
                expected: current.note.revision,
            },
            Caller::Local,
        )
        .unwrap();
        app.flush().unwrap();
        app.flush_files().unwrap();
        assert!(renamed.exists());
        std::fs::remove_file(&renamed).unwrap();
        let scan = request(&mut app, Command::RescanMirror);
        assert!(matches!(wait_io(&mut app, scan), IoStatus::Conflict(_)));
        assert!(!app.note(id).unwrap().note.trashed);
        app.shutdown().unwrap();
        drop(app);
        std::fs::remove_dir_all(&directory).unwrap();
        // Simulate interruption after SQLite commit and before any filesystem job.
        let (store, _) = Store::open(&directory).unwrap();
        store
            .try_request(StoreRequest::Configure {
                request_id: Uuid::new_v4(),
                enabled: true,
            })
            .unwrap();
        store.flush().unwrap();
        let pending = NoteRecord {
            id: NoteId(Uuid::new_v4()),
            path: "pending.md".into(),
            text: "durable pending".into(),
            revision: Revision::fresh(),
            trashed: false,
        };
        store.try_save(pending.clone()).unwrap();
        store.flush().unwrap();
        drop(store);
        let mut app = App::open(&directory).unwrap();
        app.flush_files().unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.join("markdown/pending.md")).unwrap(),
            pending.text
        );
        app.shutdown().unwrap();
        drop(app);
        let (store, state) = Store::open_state(&directory).unwrap();
        assert!(state.pending.is_empty());
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
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
        app.apply_store_event(ack);
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
