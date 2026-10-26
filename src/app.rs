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
#[derive(Clone, Debug)]
pub struct SyncStatus {
    pub enabled: bool,
    pub migrating: bool,
    pub accepted_generation: u64,
    pub persisted_generation: u64,
    pub error: Option<String>,
}
enum PendingIo {
    #[cfg(feature = "lan")]
    Lan,
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

#[cfg(feature = "sync")]
struct Migration {
    request_id: Uuid,
    candidate: Option<crate::sync::Replica>,
    remaining: VecDeque<NoteRecord>,
    committing: bool,
    history: Option<crate::sync::HistoryBundle>,
}
#[cfg(feature = "lan")]
struct JoinCommit {
    request_id: Uuid,
    peer: crate::lan::Fingerprint,
    session: usize,
    transfer: Uuid,
    bundle: crate::sync::HistoryBundle,
    replica: crate::sync::Replica,
    notes: Vec<NoteRecord>,
}
/// The application thread exclusively owns the ECS World.
#[derive(Default)]
pub struct App {
    #[cfg(feature = "lan")]
    lan_state: crate::lan_owner::LanState,
    #[cfg(feature = "lan")]
    lan_join: Option<JoinCommit>,
    #[cfg(feature = "lan")]
    lan_prepare_pending: bool,
    #[cfg(feature = "lan")]
    vault_directory: Option<std::path::PathBuf>,
    #[cfg(feature = "sync")]
    replica: Option<crate::sync::Replica>,
    #[cfg(feature = "sync")]
    migration: Option<Migration>,
    #[cfg(feature = "sync")]
    generation: u64,
    #[cfg(feature = "sync")]
    persisted_generation: u64,
    #[cfg(feature = "sync")]
    history_inflight: Option<(u64, crate::sync::HistoryBundle)>,
    #[cfg(feature = "sync")]
    durable_history: Option<crate::sync::HistoryBundle>,
    #[cfg(feature = "sync")]
    history_error: Option<String>,
    #[cfg(feature = "sync")]
    sync_conflicts: HashMap<NoteId, crate::sync::MergeConflicts>,
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
        #[cfg(feature = "sync")]
        let history = state.history.clone();
        let mut app = Self {
            store: Some(store),
            #[cfg(feature = "lan")]
            vault_directory: Some(directory.to_path_buf()),
            #[cfg(feature = "lan")]
            lan_state: crate::lan_owner::LanState::with_preparation(state.preparation),
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
        #[cfg(feature = "sync")]
        if let Some(history) = history {
            let replica = crate::sync::Replica::load(&history)
                .map_err(|error| StoreError::Storage(error.to_string()))?;
            let projected = replica
                .project()
                .map_err(|error| StoreError::Storage(error.to_string()))?;
            if projected.len() != app.ids.len()
                || projected.iter().any(|note| {
                    !app.note(note.id).is_ok_and(|view| {
                        view.note.text == note.text
                            && view.note.path == note.path
                            && view.note.trashed == note.trashed
                    })
                })
            {
                return Err(StoreError::Storage(
                    "CRDT projections disagree with stored records".into(),
                ));
            }
            app.sync_conflicts = projected
                .into_iter()
                .map(|note| (note.id, note.conflicts))
                .collect();
            app.replica = Some(replica);
            app.durable_history = Some(history);
        }
        Ok(app)
    }
    #[cfg(feature = "lan")]
    pub fn set_lan_runtime(&mut self, handle: tokio::runtime::Handle) {
        self.lan_state.runtime = Some(handle);
    }
    #[cfg(feature = "lan")]
    fn lan_context(&self) -> crate::lan_owner::Context<'_> {
        crate::lan_owner::Context {
            history: self.durable_history.as_ref(),
            accepted: self.generation,
            persisted: self.persisted_generation,
        }
    }
    #[cfg(feature = "lan")]
    fn execute_lan(&mut self, command: Command) -> Result<CommandResult, CommandError> {
        use crate::lan::{Control, Fingerprint, Ticket};
        if matches!(command, Command::GetLanTicket) {
            return self
                .lan_state
                .ticket()
                .map(|ticket| CommandResult::LanTicket { ticket })
                .map_err(CommandError::Unavailable);
        }
        if matches!(command, Command::GetLanStatus) {
            return Ok(CommandResult::LanStatus(
                self.lan_state.status(&self.lan_context()),
            ));
        }
        if self.lan_join.is_some()
            && matches!(command, Command::PrepareJoin { .. } | Command::CancelJoin)
        {
            return Err(CommandError::Busy);
        }
        let directory = self
            .vault_directory
            .clone()
            .ok_or_else(|| CommandError::Unavailable("open a vault first".into()))?;
        let request_id = self.accept_io()?;
        self.io_pending.insert(request_id, PendingIo::Lan);
        let outcome = (|| -> Result<(), String> {
            match command {
                Command::PrepareJoin { ticket } => {
                    if self.migration.is_some() || self.lan_prepare_pending {
                        return Err("join preparation is busy".into());
                    }
                    if !self.ids.is_empty() || self.replica.is_some() {
                        return Err("join requires an empty unmigrated vault".into());
                    }
                    let parsed = Ticket::parse(&ticket).map_err(|e| e.to_string())?;
                    if self
                        .lan_state
                        .preparation
                        .as_ref()
                        .is_some_and(|p| p.vault_id != parsed.vault_id)
                    {
                        return Err("cancel the existing join preparation first".into());
                    }
                    let device_id = self
                        .lan_state
                        .preparation
                        .as_ref()
                        .map(|p| p.device_id)
                        .unwrap_or_else(Uuid::new_v4);
                    self.storage_jobs.push_back(StoreRequest::PrepareJoin {
                        request_id,
                        preparation: Some(crate::lan_owner::Preparation {
                            vault_id: parsed.vault_id,
                            device_id,
                            ticket,
                        }),
                    });
                    self.lan_prepare_pending = true;
                }
                Command::CancelJoin => {
                    if self.lan_prepare_pending {
                        return Err("join preparation is busy".into());
                    }
                    if let Some(cancelled) = self.lan_state.pause() {
                        self.io_pending.remove(&cancelled);
                        self.io_status.insert(
                            cancelled,
                            IoStatus::Failed("LAN startup cancelled by CancelJoin".into()),
                        );
                    }
                    self.storage_jobs.push_back(StoreRequest::PrepareJoin {
                        request_id,
                        preparation: None,
                    });
                    self.lan_prepare_pending = true;
                }
                Command::ListenLan {
                    bind,
                    advertised_ip,
                } => {
                    let vault = self
                        .durable_history
                        .as_ref()
                        .map(|h| h.vault_id)
                        .or_else(|| self.lan_state.preparation.as_ref().map(|p| p.vault_id))
                        .ok_or("enable sync or prepare joining first")?;
                    self.lan_state.start(
                        request_id,
                        directory,
                        vault,
                        bind.parse().map_err(|_| "invalid numeric bind address")?,
                        advertised_ip
                            .map(|ip| ip.parse().map_err(|_| "invalid advertised IP"))
                            .transpose()?,
                    )?;
                }
                Command::ConfirmPeer { ticket } => {
                    let ticket = Ticket::parse(&ticket).map_err(|e| e.to_string())?;
                    self.lan_state
                        .control(request_id, Control::Confirm(ticket))?;
                }
                Command::UnpairPeer { fingerprint } => {
                    let peer = Fingerprint(fingerprint);
                    self.lan_state.deny(&peer);
                    self.lan_state.control(request_id, Control::Unpair(peer))?;
                }
                Command::ConnectPeer { fingerprint } => {
                    let ticket = self.lan_state.known(&fingerprint)?;
                    self.lan_state
                        .control(request_id, Control::Connect(ticket))?;
                }
                Command::PauseLan => {
                    if let Some(cancelled) = self.lan_state.pause() {
                        self.io_pending.remove(&cancelled);
                        self.io_status.insert(
                            cancelled,
                            IoStatus::Failed("LAN startup cancelled by pause".into()),
                        );
                    }
                    self.io_status.insert(request_id, IoStatus::Completed);
                    self.io_pending.remove(&request_id);
                }
                Command::ResumeLan => {
                    let vault = self
                        .durable_history
                        .as_ref()
                        .map(|h| h.vault_id)
                        .or_else(|| self.lan_state.preparation.as_ref().map(|p| p.vault_id))
                        .ok_or("no selected vault")?;
                    self.lan_state.resume(request_id, directory, vault)?;
                }
                _ => unreachable!(),
            }
            Ok(())
        })();
        if let Err(error) = outcome {
            self.io_pending.remove(&request_id);
            self.io_status
                .insert(request_id, IoStatus::Failed(error.clone()));
            return Err(CommandError::Unavailable(error));
        }
        Ok(CommandResult::Accepted { request_id })
    }
    #[cfg(feature = "lan")]
    fn tick_lan(&mut self) {
        use crate::lan_owner::Action;
        let mut state = std::mem::take(&mut self.lan_state);
        let actions = state.tick(&self.lan_context());
        for action in actions {
            match action {
                Action::Status(id, result) => {
                    self.io_pending.remove(&id);
                    self.io_status.insert(
                        id,
                        match result {
                            Ok(()) => IoStatus::Completed,
                            Err(error) => IoStatus::Failed(error),
                        },
                    );
                }
                Action::Receive {
                    peer,
                    bundle,
                    receipts,
                } => match self.merge_history(&bundle) {
                    Ok(generation) => state.received(&peer, generation, receipts),
                    Err(error) => {
                        state.fail(&peer, error.to_string());
                    }
                },
                Action::Join {
                    peer,
                    session,
                    transfer,
                    bundle,
                } => {
                    if !self.ids.is_empty()
                        || self.replica.is_some()
                        || self.migration.is_some()
                        || self.lan_join.is_some()
                    {
                        state.committing = false;
                        state.error = Some("join refused: local vault is no longer empty".into());
                        continue;
                    }
                    match crate::sync::Replica::load(&bundle).and_then(|replica| {
                        replica.project().map(|projection| (replica, projection))
                    }) {
                        Ok((replica, projection)) => {
                            let notes = projection
                                .into_iter()
                                .map(|note| NoteRecord {
                                    id: note.id,
                                    path: note.path,
                                    text: note.text,
                                    trashed: note.trashed,
                                    revision: Revision::fresh(),
                                })
                                .collect::<Vec<_>>();
                            let request_id = Uuid::new_v4();
                            self.storage_jobs.push_back(StoreRequest::JoinVault {
                                request_id,
                                history: bundle.clone(),
                                notes: notes.clone(),
                            });
                            self.lan_join = Some(JoinCommit {
                                request_id,
                                peer,
                                session,
                                transfer,
                                bundle,
                                replica,
                                notes,
                            });
                        }
                        Err(error) => {
                            state.committing = false;
                            state.error = Some(error.to_string());
                        }
                    }
                }
            }
        }
        self.lan_state = state;
    }
    pub fn sync_enabled(&self) -> bool {
        #[cfg(feature = "sync")]
        {
            self.replica.is_some()
        }
        #[cfg(not(feature = "sync"))]
        {
            false
        }
    }
    pub fn sync_status(&self) -> SyncStatus {
        #[cfg(feature = "sync")]
        {
            SyncStatus {
                enabled: self.replica.is_some(),
                migrating: self.migration.is_some(),
                accepted_generation: self.generation,
                persisted_generation: self.persisted_generation,
                error: self.history_error.clone(),
            }
        }
        #[cfg(not(feature = "sync"))]
        {
            SyncStatus {
                enabled: false,
                migrating: false,
                accepted_generation: 0,
                persisted_generation: 0,
                error: None,
            }
        }
    }
    #[cfg(feature = "sync")]
    pub(crate) fn durable_history(&self) -> Option<&crate::sync::HistoryBundle> {
        self.durable_history.as_ref()
    }
    #[cfg(feature = "sync")]
    fn tick_migration(&mut self) {
        let Some(mut migration) = self.migration.take() else {
            return;
        };
        if migration.committing {
            self.migration = Some(migration);
            return;
        }
        if migration.candidate.is_none() {
            if let Some(error) = self.notes().into_iter().find_map(|view| view.save.error) {
                self.io_status
                    .insert(migration.request_id, IoStatus::Failed(error));
                return;
            }
            if !self.pending.is_empty() || !self.inflight.is_empty() {
                self.migration = Some(migration);
                return;
            }
            match crate::sync::Replica::bootstrap(&[]) {
                Ok(candidate) => migration.candidate = Some(candidate),
                Err(error) => {
                    self.io_status
                        .insert(migration.request_id, IoStatus::Failed(error.to_string()));
                    return;
                }
            }
            migration.remaining = self.notes().into_iter().map(|view| view.note).collect();
        }
        for _ in 0..8 {
            let Some(note) = migration.remaining.pop_front() else {
                break;
            };
            if let Err(error) = migration.candidate.as_mut().unwrap().insert(&note) {
                self.io_status
                    .insert(migration.request_id, IoStatus::Failed(error.to_string()));
                return;
            }
        }
        if migration.remaining.is_empty() {
            let history = migration.candidate.as_mut().unwrap().save();
            self.storage_jobs.push_back(StoreRequest::Migrate {
                request_id: migration.request_id,
                history: history.clone(),
                notes: self.notes().into_iter().map(|view| view.note).collect(),
            });
            migration.history = Some(history);
            migration.committing = true;
        }
        self.migration = Some(migration);
    }
    #[cfg(feature = "sync")]
    fn tick_history(&mut self) {
        if self.history_inflight.is_some()
            || self.generation == self.persisted_generation
            || self.history_error.is_some()
        {
            return;
        }
        let history = self.replica.as_mut().unwrap().save();
        let notes: Vec<_> = self.pending.values().cloned().collect();
        for note in self.pending.values() {
            self.inflight.insert(note.id, note.revision);
        }
        self.pending.clear();
        self.storage_jobs.push_back(StoreRequest::SaveHistory {
            generation: self.generation,
            history: history.clone(),
            notes,
            conflicts: self
                .sync_conflicts
                .iter()
                .map(|(id, conflict)| (*id, conflict.clone()))
                .collect(),
        });
        self.history_inflight = Some((self.generation, history));
    }
    #[cfg(feature = "sync")]
    fn refresh_projection(&mut self) -> Result<(), CommandError> {
        let Some(replica) = &self.replica else {
            return Ok(());
        };
        let notes = replica
            .project()
            .map_err(|error| CommandError::Unavailable(error.to_string()))?;
        for note in notes {
            let metadata_changed = self.sync_conflicts.get(&note.id) != Some(&note.conflicts);
            self.sync_conflicts.insert(note.id, note.conflicts);
            if let Ok(mut view) = self.note(note.id) {
                if metadata_changed
                    || view.note.text != note.text
                    || view.note.path != note.path
                    || view.note.trashed != note.trashed
                {
                    view.note.text = note.text;
                    view.note.path = note.path;
                    view.note.trashed = note.trashed;
                    view.note.revision = Revision::fresh();
                    view.save.accepted = view.note.revision;
                    view.save.error = None;
                    self.world
                        .entity_mut(self.ids[&note.id])
                        .insert((view.note.clone(), view.save));
                    self.pending.insert(note.id, view.note);
                }
            } else {
                let revision = Revision::fresh();
                let record = NoteRecord {
                    id: note.id,
                    path: note.path,
                    text: note.text,
                    trashed: note.trashed,
                    revision,
                };
                let entity = self
                    .world
                    .spawn((
                        record.clone(),
                        SaveState {
                            accepted: revision,
                            persisted: None,
                            error: None,
                        },
                    ))
                    .id();
                self.ids.insert(note.id, entity);
                self.pending.insert(note.id, record);
            }
        }
        Ok(())
    }
    /// Private owner input for a future authenticated adapter. The returned
    /// generation is accepted only; durable receipt waits for persistence.
    #[cfg(feature = "sync")]
    pub(crate) fn merge_history(
        &mut self,
        history: &crate::sync::HistoryBundle,
    ) -> Result<u64, CommandError> {
        if self.migration.is_some() {
            return Err(CommandError::Busy);
        }
        let mut candidate = self
            .replica
            .as_ref()
            .ok_or_else(|| CommandError::Unavailable("vault not migrated".into()))?
            .clone();
        candidate
            .merge(history)
            .map_err(|error| CommandError::Unavailable(error.to_string()))?;
        self.replica = Some(candidate);
        self.generation += 1;
        self.refresh_projection()?;
        Ok(self.generation)
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
            #[cfg(feature = "lan")]
            StoreEvent::Prepared {
                request_id,
                preparation,
                result,
            } => {
                self.lan_prepare_pending = false;
                self.io_pending.remove(&request_id);
                match result {
                    Ok(()) => {
                        self.lan_state.preparation = preparation;
                        self.io_status.insert(request_id, IoStatus::Completed);
                    }
                    Err(error) => {
                        self.io_status.insert(request_id, IoStatus::Failed(error));
                    }
                }
            }
            #[cfg(feature = "lan")]
            StoreEvent::Joined { request_id, result } => {
                if let Some(join) = self.lan_join.take() {
                    if join.request_id == request_id {
                        match result {
                            Ok(()) => {
                                self.sync_conflicts = join
                                    .replica
                                    .project()
                                    .unwrap()
                                    .into_iter()
                                    .map(|note| (note.id, note.conflicts))
                                    .collect();
                                for note in join.notes {
                                    let id = note.id;
                                    let revision = note.revision;
                                    if self.mirror_enabled {
                                        self.exports.insert(id, note.clone());
                                    }
                                    let entity = self
                                        .world
                                        .spawn((
                                            note,
                                            SaveState {
                                                accepted: revision,
                                                persisted: Some(revision),
                                                error: None,
                                            },
                                        ))
                                        .id();
                                    self.ids.insert(id, entity);
                                }
                                self.replica = Some(join.replica);
                                self.durable_history = Some(join.bundle.clone());
                                if let Err(error) = self.lan_state.joined(
                                    &join.peer,
                                    join.session,
                                    join.transfer,
                                    &join.bundle,
                                ) {
                                    self.lan_state.error = Some(error);
                                }
                            }
                            Err(error) => {
                                self.lan_state.committing = false;
                                self.lan_state.error = Some(error);
                            }
                        }
                    }
                }
            }
            #[cfg(feature = "sync")]
            StoreEvent::Migrated { request_id, result } => {
                if let Some(mut migration) = self.migration.take() {
                    if migration.request_id == request_id {
                        match result {
                            Ok(_) => {
                                self.replica = migration.candidate.take();
                                self.sync_conflicts = self
                                    .replica
                                    .as_ref()
                                    .unwrap()
                                    .project()
                                    .unwrap()
                                    .into_iter()
                                    .map(|note| (note.id, note.conflicts))
                                    .collect();
                                self.durable_history = migration.history.take();
                                if self.mirror_enabled {
                                    for view in self.notes() {
                                        self.exports.insert(view.note.id, view.note);
                                    }
                                }
                                self.io_status.insert(request_id, IoStatus::Completed);
                            }
                            Err(error) => {
                                self.io_status.insert(request_id, IoStatus::Failed(error));
                            }
                        }
                    }
                }
            }
            #[cfg(feature = "sync")]
            StoreEvent::HistorySaved {
                generation,
                notes,
                result,
            } => {
                if self
                    .history_inflight
                    .as_ref()
                    .is_some_and(|(submitted, _)| *submitted == generation)
                {
                    let (_, history) = self.history_inflight.take().unwrap();
                    match &result {
                        Ok(_) => {
                            self.persisted_generation = generation;
                            self.durable_history = Some(history);
                            self.history_error = None;
                        }
                        Err(error) => self.history_error = Some(error.clone()),
                    }
                    for note in notes {
                        self.apply_ack(SaveAck {
                            id: note.id,
                            revision: note.revision,
                            note,
                            result: result.as_ref().map(|_| ()).map_err(Clone::clone),
                            export_pending: result.as_ref().copied().unwrap_or(false),
                        });
                    }
                }
            }
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
        #[cfg(feature = "lan")]
        self.tick_lan();
        self.tick_files();
        #[cfg(feature = "sync")]
        {
            self.tick_migration();
            if self.replica.is_some() {
                self.tick_history();
                return;
            }
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
        #[cfg(feature = "sync")]
        {
            self.history_error = None;
        }
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
            #[cfg(feature = "sync")]
            if let Some(error) = &self.history_error {
                return Err(StoreError::Storage(error.clone()));
            }
            let source_idle = self.pending.is_empty() && self.inflight.is_empty();
            #[cfg(feature = "sync")]
            let source_idle = source_idle
                && self.migration.is_none()
                && self.history_inflight.is_none()
                && self.generation == self.persisted_generation;
            #[cfg(feature = "lan")]
            let source_idle = source_idle && self.lan_join.is_none();
            if source_idle {
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
        #[cfg(feature = "lan")]
        self.lan_state.pause();
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
        if !command.descriptor().available {
            return Err(CommandError::Unavailable(command.descriptor().name.into()));
        }
        if !command.descriptor().allowed(caller) {
            return Err(CommandError::Unauthorized);
        }
        #[cfg(feature = "lan")]
        {
            if matches!(
                command,
                Command::ListenLan { .. }
                    | Command::GetLanTicket
                    | Command::PrepareJoin { .. }
                    | Command::CancelJoin
                    | Command::ConfirmPeer { .. }
                    | Command::UnpairPeer { .. }
                    | Command::ConnectPeer { .. }
                    | Command::GetLanStatus
                    | Command::PauseLan
                    | Command::ResumeLan
            ) {
                return self.execute_lan(command);
            }
            if self.lan_join.is_some() && command.is_mutation() {
                return Err(CommandError::Busy);
            }
            if matches!(command, Command::EnableSync) && self.lan_state.preparation.is_some() {
                return Err(CommandError::Unavailable(
                    "cancel join preparation before creating another vault".into(),
                ));
            }
        }
        #[cfg(feature = "sync")]
        if self.migration.is_some()
            && command.is_mutation()
            && !matches!(
                command,
                Command::SetMirrorEnabled { .. } | Command::ExportMarkdown { .. }
            )
        {
            return Err(CommandError::Busy);
        }
        match command {
            Command::EnableSync => {
                #[cfg(feature = "sync")]
                {
                    if self.io_pending.values().any(|pending| {
                        matches!(pending, PendingIo::Import(_) | PendingIo::Rescan { .. })
                    }) {
                        return Err(CommandError::Busy);
                    }
                    let request_id = self.accept_io()?;
                    if self.replica.is_some() {
                        self.io_status.insert(request_id, IoStatus::Completed);
                    } else {
                        self.migration = Some(Migration {
                            request_id,
                            candidate: None,
                            remaining: VecDeque::new(),
                            committing: false,
                            history: None,
                        });
                    }
                    Ok(CommandResult::Accepted { request_id })
                }
                #[cfg(not(feature = "sync"))]
                Err(CommandError::Unavailable("sync feature is disabled".into()))
            }
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
                #[cfg(feature = "sync")]
                if let Some(replica) = &self.replica {
                    let mut candidate = replica.clone();
                    candidate
                        .insert(&record)
                        .map_err(|error| CommandError::Unavailable(error.to_string()))?;
                    self.replica = Some(candidate);
                    self.generation += 1;
                }
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
                #[cfg(feature = "sync")]
                self.refresh_projection()?;
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
                #[cfg(feature = "sync")]
                let engine_command = mutation.clone();
                #[cfg(feature = "sync")]
                let original_len = view.note.text.len();
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
                #[cfg(feature = "sync")]
                if let Some(replica) = &self.replica {
                    let mut candidate = replica.clone();
                    let result = match engine_command {
                        Command::EditNote { edits, .. }
                            if edits.len() == 1
                                && edits[0].start == 0
                                && edits[0].end == original_len =>
                        {
                            candidate.replace_text(id, &edits[0].replacement)
                        }
                        Command::EditNote { edits, .. } => candidate.edit(id, &edits),
                        Command::RenameNote { path, .. } => candidate.rename(id, &path),
                        Command::TrashNote { .. } => candidate.trash(id, true),
                        Command::RestoreNote { .. } => candidate.trash(id, false),
                        _ => unreachable!(),
                    };
                    result.map_err(|error| CommandError::Unavailable(error.to_string()))?;
                    candidate
                        .project()
                        .map_err(|error| CommandError::Unavailable(error.to_string()))?;
                    self.replica = Some(candidate);
                    self.generation += 1;
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
                #[cfg(feature = "sync")]
                self.refresh_projection()?;
                Ok(CommandResult::Note(self.view(id)?))
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
    #[cfg(feature = "lan")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lan_two_apps_join_and_live_unicode_history() {
        async fn pump(a: &mut App, b: &mut App, condition: impl Fn(&App, &App) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                a.tick();
                b.tick();
                if condition(a, b) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "LAN timeout: {:?} / {:?}",
                    a.lan_state.error,
                    b.lan_state.error
                );
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
        async fn command(a: &mut App, b: &mut App, c: Command) {
            let id = request(a, c);
            pump(a, b, |a, _| a.io_status(id) != Some(&IoStatus::Pending)).await;
            assert_eq!(a.io_status(id), Some(&IoStatus::Completed));
        }
        fn ticket(a: &mut App) -> String {
            match a.execute(Command::GetLanTicket, Caller::Local).unwrap() {
                CommandResult::LanTicket { ticket } => ticket,
                _ => panic!("ticket"),
            }
        }
        let root = std::env::temp_dir().join(format!("tanglewood-owner-lan-{}", Uuid::new_v4()));
        let mut a = App::open(&root.join("a")).unwrap();
        let mut b = App::open(&root.join("b")).unwrap();
        a.set_lan_runtime(tokio::runtime::Handle::current());
        b.set_lan_runtime(tokio::runtime::Handle::current());
        let original = note(
            a.execute(
                Command::CreateNote {
                    path: "shared.md".into(),
                    text: "α🦀".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        );
        command(&mut a, &mut b, Command::EnableSync).await;
        command(
            &mut a,
            &mut b,
            Command::ListenLan {
                bind: "127.0.0.1:0".into(),
                advertised_ip: None,
            },
        )
        .await;
        let at = ticket(&mut a);
        command(&mut b, &mut a, Command::PrepareJoin { ticket: at.clone() }).await;
        assert!(b.notes().is_empty());
        command(
            &mut b,
            &mut a,
            Command::ListenLan {
                bind: "127.0.0.1:0".into(),
                advertised_ip: None,
            },
        )
        .await;
        let bt = ticket(&mut b);
        command(&mut a, &mut b, Command::ConfirmPeer { ticket: bt }).await;
        command(&mut b, &mut a, Command::ConfirmPeer { ticket: at.clone() }).await;
        let fingerprint = crate::lan::Ticket::parse(&at).unwrap().fingerprint.0;
        command(&mut b, &mut a, Command::ConnectPeer { fingerprint }).await;
        pump(&mut a, &mut b, |_, b| {
            b.sync_enabled() && b.notes().len() == 1
        })
        .await;
        assert_ne!(
            a.durable_history().unwrap().device_id,
            b.durable_history().unwrap().device_id
        );
        assert_eq!(b.note(original.note.id).unwrap().note.text, "α🦀");
        let current = a.note(original.note.id).unwrap();
        a.execute(
            Command::EditNote {
                id: original.note.id,
                expected: current.note.revision,
                edits: vec![TextEdit {
                    start: 2,
                    end: 6,
                    replacement: "🌳β".into(),
                }],
            },
            Caller::Local,
        )
        .unwrap();
        pump(&mut a, &mut b, |a, b| {
            a.note(original.note.id).unwrap().note.text
                == b.note(original.note.id).unwrap().note.text
        })
        .await;
        a.flush().unwrap();
        b.flush().unwrap();
        assert_eq!(b.note(original.note.id).unwrap().note.text, "α🌳β");
        command(&mut a, &mut b, Command::PauseLan).await;
        assert!(a.lan_state.controller.is_none());
        a.tick();
        assert!(a.lan_state.controller.is_none());
        command(&mut b, &mut a, Command::PauseLan).await;
        let id = original.note.id;
        let av = a.note(id).unwrap();
        a.execute(
            Command::RenameNote {
                id,
                expected: av.note.revision,
                path: "offline.md".into(),
            },
            Caller::Local,
        )
        .unwrap();
        let bv = b.note(id).unwrap();
        b.execute(
            Command::TrashNote {
                id,
                expected: bv.note.revision,
            },
            Caller::Local,
        )
        .unwrap();
        a.flush().unwrap();
        b.flush().unwrap();
        // A cancelled startup must never install its late controller on tick.
        let pending = request(&mut a, Command::ResumeLan);
        request(&mut a, Command::PauseLan);
        assert!(matches!(a.io_status(pending), Some(IoStatus::Failed(_))));
        tokio::time::sleep(Duration::from_millis(20)).await;
        a.tick();
        assert!(a.lan_state.controller.is_none());
        let cancelled_by_join = request(&mut a, Command::ResumeLan);
        command(&mut a, &mut b, Command::CancelJoin).await;
        assert!(matches!(
            a.io_status(cancelled_by_join),
            Some(IoStatus::Failed(_))
        ));
        tokio::time::sleep(Duration::from_millis(20)).await;
        a.tick();
        assert!(a.lan_state.controller.is_none());
        a.shutdown().unwrap();
        b.shutdown().unwrap();
        drop(a);
        drop(b);
        let mut a = App::open(&root.join("a")).unwrap();
        let mut b = App::open(&root.join("b")).unwrap();
        a.set_lan_runtime(tokio::runtime::Handle::current());
        b.set_lan_runtime(tokio::runtime::Handle::current());
        for first in [true, false] {
            if first {
                command(
                    &mut a,
                    &mut b,
                    Command::ListenLan {
                        bind: "127.0.0.1:0".into(),
                        advertised_ip: None,
                    },
                )
                .await;
            } else {
                command(
                    &mut b,
                    &mut a,
                    Command::ListenLan {
                        bind: "127.0.0.1:0".into(),
                        advertised_ip: None,
                    },
                )
                .await;
            }
        }
        let at = ticket(&mut a);
        let bt = ticket(&mut b);
        command(&mut a, &mut b, Command::ConfirmPeer { ticket: bt }).await;
        command(&mut b, &mut a, Command::ConfirmPeer { ticket: at.clone() }).await;
        command(
            &mut b,
            &mut a,
            Command::ConnectPeer {
                fingerprint: crate::lan::Ticket::parse(&at).unwrap().fingerprint.0,
            },
        )
        .await;
        pump(&mut a, &mut b, |a, b| {
            let a = a.note(id).unwrap().note;
            let b = b.note(id).unwrap().note;
            a.path == "offline.md" && b.path == a.path && a.trashed && b.trashed
        })
        .await;
        a.flush().unwrap();
        b.flush().unwrap();
        a.shutdown().unwrap();
        b.shutdown().unwrap();
        drop(a);
        drop(b);
        let mut reopened = App::open(&root.join("b")).unwrap();
        assert_eq!(reopened.note(original.note.id).unwrap().note.text, "α🌳β");
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(feature = "sync")]
    #[test]
    fn explicit_migration_commands_remote_ack_and_restart_preserve_authority() {
        let directory = std::env::temp_dir().join(format!("tanglewood-migrate-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let initial = note(
            app.execute(
                Command::CreateNote {
                    path: "a.md".into(),
                    text: "α 🦀\r\n".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        );
        assert!(!app.sync_enabled());
        assert_eq!(
            app.execute(Command::EnableSync, Caller::AutomationReadWrite)
                .unwrap_err(),
            CommandError::Unauthorized
        );
        let migration = request(&mut app, Command::EnableSync);
        assert_eq!(
            app.execute(
                Command::RenameNote {
                    id: initial.note.id,
                    expected: initial.note.revision,
                    path: "busy.md".into()
                },
                Caller::Local
            )
            .unwrap_err(),
            CommandError::Busy
        );
        assert_eq!(wait_io(&mut app, migration), IoStatus::Completed);
        assert_eq!(app.note(initial.note.id).unwrap().note, initial.note);
        let mirror = request(&mut app, Command::SetMirrorEnabled { enabled: true });
        assert_eq!(wait_io(&mut app, mirror), IoStatus::Completed);
        app.flush_files().unwrap();
        let durable = app.durable_history().unwrap().clone();
        let connection = rusqlite::Connection::open_with_flags(
            directory.join("notes.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let backup: String = connection
            .query_row("SELECT backup_path FROM sync_meta", [], |row| row.get(0))
            .unwrap();
        let backup = rusqlite::Connection::open_with_flags(
            backup,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let (text, id, revision): (String, String, String) = backup
            .query_row("SELECT text,id,revision FROM notes", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        assert_eq!(
            (text, id, revision),
            (
                initial.note.text.clone(),
                initial.note.id.0.to_string(),
                initial.note.revision.0.to_string()
            )
        );
        drop(backup);
        drop(connection);
        app.shutdown().unwrap();
        drop(app);
        let mut app = App::open(&directory).unwrap();
        assert_eq!(app.durable_history().unwrap().device_id, durable.device_id);
        assert_eq!(app.durable_history().unwrap().vault_id, durable.vault_id);
        let id = initial.note.id;
        let edited = note(
            app.execute(
                Command::EditNote {
                    id,
                    expected: initial.note.revision,
                    edits: vec![TextEdit {
                        start: 3,
                        end: 7,
                        replacement: "L🦀".into(),
                    }],
                },
                Caller::Local,
            )
            .unwrap(),
        );
        assert!(matches!(
            app.execute(
                Command::EditNote {
                    id,
                    expected: initial.note.revision,
                    edits: vec![]
                },
                Caller::Local
            ),
            Err(CommandError::StaleRevision { .. })
        ));
        app.flush().unwrap();
        let old_durable = app.durable_history().unwrap().clone();
        let mut peer = crate::sync::Replica::join(&old_durable).unwrap();
        peer.edit(
            id,
            &[TextEdit {
                start: 0,
                end: 2,
                replacement: "β".into(),
            }],
        )
        .unwrap();
        peer.rename(id, "remote.md").unwrap();
        peer.trash(id, true).unwrap();
        let local = note(
            app.execute(
                Command::EditNote {
                    id,
                    expected: edited.note.revision,
                    edits: vec![TextEdit {
                        start: 0,
                        end: 0,
                        replacement: "local ".into(),
                    }],
                },
                Caller::Local,
            )
            .unwrap(),
        );
        let trashed_local = note(
            app.execute(
                Command::TrashNote {
                    id,
                    expected: local.note.revision,
                },
                Caller::Local,
            )
            .unwrap(),
        );
        let restored_local = note(
            app.execute(
                Command::RestoreNote {
                    id,
                    expected: trashed_local.note.revision,
                },
                Caller::Local,
            )
            .unwrap(),
        );
        app.execute(
            Command::RenameNote {
                id,
                expected: restored_local.note.revision,
                path: "local.md".into(),
            },
            Caller::Local,
        )
        .unwrap();
        let local = app.note(id).unwrap();
        let accepted_generation = app.merge_history(&peer.save()).unwrap();
        let merged = app.note(id).unwrap();
        assert_ne!(merged.note.revision, local.note.revision);
        assert!(merged.note.trashed);
        assert!(merged.note.text.contains("local "));
        assert!(merged.note.text.contains("β"));
        assert_eq!(app.durable_history().unwrap().catalog, old_durable.catalog);
        assert!(app.persisted_generation < accepted_generation);
        app.flush().unwrap();
        assert!(app.persisted_generation >= accepted_generation);
        let connection = rusqlite::Connection::open_with_flags(
            directory.join("notes.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let (revision, text, trashed): (String, String, bool) = connection
            .query_row(
                "SELECT revision,text,trashed FROM pending_exports WHERE id=?1",
                [id.0.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (revision, text, trashed),
            (
                merged.note.revision.0.to_string(),
                merged.note.text.clone(),
                true
            )
        );
        let bytes: Vec<u8> = connection
            .query_row(
                "SELECT metadata FROM sync_conflicts WHERE note_id=?1",
                [id.0.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        let metadata: crate::sync::MergeConflicts = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(metadata.paths, vec!["local.md", "remote.md"]);
        assert_eq!(metadata.tombstones, vec![false, true]);
        drop(connection);

        let restored = note(
            app.execute(
                Command::RestoreNote {
                    id,
                    expected: merged.note.revision,
                },
                Caller::Local,
            )
            .unwrap(),
        );
        let source = directory.join("import.md");
        std::fs::write(&source, "imported β 🦀\r\n").unwrap();
        let importing = request(
            &mut app,
            Command::ImportMarkdown {
                source_path: source.to_string_lossy().into(),
                destination: ImportDestination::Replace {
                    id,
                    expected: restored.note.revision,
                },
            },
        );
        assert_eq!(wait_io(&mut app, importing), IoStatus::Completed);
        let current = app.note(id).unwrap();
        let renamed = note(
            app.execute(
                Command::RenameNote {
                    id,
                    expected: current.note.revision,
                    path: "final.md".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        );
        let created = note(
            app.execute(
                Command::CreateNote {
                    path: "created.md".into(),
                    text: "new note".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        );
        let trashed = note(
            app.execute(
                Command::TrashNote {
                    id: created.note.id,
                    expected: created.note.revision,
                },
                Caller::Local,
            )
            .unwrap(),
        );
        app.execute(
            Command::RestoreNote {
                id: created.note.id,
                expected: trashed.note.revision,
            },
            Caller::Local,
        )
        .unwrap();
        app.flush().unwrap();
        let expected = app.notes();
        let history = app.durable_history().unwrap().clone();
        app.shutdown().unwrap();
        drop(app);
        let mut app = App::open(&directory).unwrap();
        assert_eq!(app.notes(), expected);
        assert_eq!(app.note(id).unwrap().note, renamed.note);
        assert_eq!(
            crate::sync::Replica::load(app.durable_history().unwrap())
                .unwrap()
                .project()
                .unwrap(),
            crate::sync::Replica::load(&history)
                .unwrap()
                .project()
                .unwrap()
        );
        // Invalid persistence identity cannot acknowledge or lose accepted data.
        app.replica = Some(crate::sync::Replica::join(&history).unwrap());
        let current = app.note(id).unwrap();
        let failed = note(
            app.execute(
                Command::EditNote {
                    id,
                    expected: current.note.revision,
                    edits: vec![TextEdit {
                        start: 0,
                        end: 0,
                        replacement: "unsaved ".into(),
                    }],
                },
                Caller::Local,
            )
            .unwrap(),
        );
        assert!(app.flush().is_err());
        assert_eq!(app.note(id).unwrap().note, failed.note);
        assert_ne!(
            app.note(id).unwrap().save.persisted,
            Some(failed.note.revision)
        );
        assert_eq!(app.durable_history().unwrap().device_id, history.device_id);
        drop(app);
        let (store, _) = Store::open(&directory).unwrap();
        let record = renamed.note;
        store.try_save(record).unwrap();
        assert!(
            store
                .flush()
                .unwrap()
                .iter()
                .any(|event| matches!(event,StoreEvent::Saved(ack) if ack.result.is_err()))
        );
        drop(store);
        if let Some(destination) = std::env::var_os("TANGLEWOOD_MIGRATION_FIXTURE") {
            std::fs::create_dir_all(&destination).unwrap();
            std::fs::copy(
                directory.join("notes.sqlite3"),
                Path::new(&destination).join("notes.sqlite3"),
            )
            .unwrap();
        }
        std::fs::remove_dir_all(directory).unwrap();
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
