use crate::files::{
    ExportBaseline, ExportJob, FileCompletion, FileError, FileJob, FileWorker, Reconcile, reconcile,
};
use crate::store::{
    BatchSubmitError, MirrorConflict, SaveAck, Store, StoreError, StoreEvent, StoreRequest,
};
use crate::{
    commands::{Caller, Command, CommandError, CommandResult, ImportDestination},
    notes::{
        CodexProjectBinding, NoteId, NoteRecord, NoteView, Revision, SaveState, TextEdit,
        edited_text, valid_path,
    },
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
    Research(Uuid),
    ResearchAsset {
        workspace: Uuid,
        expected: Revision,
        title: String,
        thread: Option<crate::research::ThreadRef>,
        round_id: Option<Uuid>,
        source_id: Option<Uuid>,
        ocr: bool,
        role: crate::research::ArtifactRole,
        source_ids: Vec<Uuid>,
    },
    CodexProject,
    AutomationPermission,
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
#[derive(Clone)]
struct PaperReceipt {
    ocr: bool,
    source_id: Uuid,
    pages: Vec<crate::research::SourcePage>,
    extraction_error: Option<String>,
}
impl PaperReceipt {
    fn attach(
        &self,
        workspace: &mut crate::research::ResearchWorkspace,
        artifact: &crate::research::ResearchArtifact,
    ) -> Result<(), CommandError> {
        let source = workspace
            .sources
            .iter_mut()
            .find(|s| s.id == self.source_id)
            .ok_or(CommandError::NotFound)?;
        if source.paper.is_some() && !self.ocr {
            return Err(CommandError::InvalidResearch(
                "source already has a retained paper".into(),
            ));
        }
        // An unsuccessful OCR attempt must never replace already-readable evidence.
        if self.ocr && self.extraction_error.is_some() && crate::research::source_has_text(source) {
            return Ok(());
        }
        source.paper = Some(crate::research::PaperContent {
            artifact_id: artifact.id,
            extraction_error: self.extraction_error.clone(),
            ocr: self.ocr,
        });
        source.kind = if artifact.kind == crate::research::ArtifactKind::Pdf {
            crate::research::SourceKind::Pdf
        } else {
            crate::research::SourceKind::Text
        };
        source.pages = self.pages.clone();
        Ok(())
    }
}
/// The application thread exclusively owns the ECS World.
#[derive(Default)]
pub struct App {
    automation_write_preference: Option<bool>,
    automation_permission_dirty: bool,
    automation_permission_error: Option<String>,
    research: HashMap<Uuid, crate::research::ResearchWorkspace>,
    research_error: Option<String>,
    research_requests: HashMap<Uuid, Uuid>,
    research_assets: Option<crate::research_assets::ResearchAssetWorker>,
    research_receipts: HashMap<Uuid, (Uuid, crate::research::ResearchArtifact)>,
    research_paper_receipts: HashMap<Uuid, PaperReceipt>,
    acquisition_armed: std::collections::HashSet<Uuid>,
    acquisition_cancelled: std::collections::HashSet<Uuid>,
    acquisition_requests: HashMap<Uuid, (Uuid, Uuid)>,
    codex_project: Option<CodexProjectBinding>,
    codex_project_persisted: Option<CodexProjectBinding>,
    codex_project_error: Option<String>,
    #[cfg(feature = "lan")]
    lan_state: crate::lan_owner::LanState,
    #[cfg(feature = "lan")]
    lan_join: Option<JoinCommit>,
    #[cfg(feature = "lan")]
    lan_prepare_pending: bool,
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
            automation_write_preference: state.automation_write_enabled,
            research_assets: Some(
                crate::research_assets::ResearchAssetWorker::open(directory)
                    .map_err(StoreError::Storage)?,
            ),
            research: state
                .research
                .into_iter()
                .map(|workspace| (workspace.id, workspace))
                .collect(),
            codex_project: state.codex_project.clone(),
            codex_project_persisted: state.codex_project,
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
            StoreEvent::AutomationWriteSaved {
                request_id,
                enabled,
                result,
            } => {
                self.io_pending.remove(&request_id);
                match result {
                    Ok(()) => {
                        self.automation_write_preference = Some(enabled);
                        self.automation_permission_dirty = false;
                        self.automation_permission_error = None;
                        self.io_status.insert(request_id, IoStatus::Completed);
                    }
                    Err(error) => {
                        self.automation_write_preference = Some(false);
                        self.automation_permission_error = Some(error.clone());
                        self.io_status.insert(request_id, IoStatus::Failed(error));
                    }
                }
            }
            StoreEvent::ResearchSaved {
                request_id,
                workspace,
                result,
            } => {
                self.io_pending.remove(&request_id);
                self.research_requests.insert(request_id, workspace.id);
                match result {
                    Ok(()) => {
                        self.research_receipts.retain(|_, (id, artifact)| {
                            *id != workspace.id
                                || !workspace.artifacts.iter().any(|a| a.id == artifact.id)
                        });
                        self.research_paper_receipts
                            .retain(|request, _| self.research_receipts.contains_key(request));
                        self.acquisition_requests.retain(|_, (id, job)| {
                            *id != workspace.id
                                || !workspace.acquisitions.iter().any(|j| {
                                    j.id == *job
                                        && matches!(
                                            j.status,
                                            crate::research::AcquisitionStatus::Ready
                                                | crate::research::AcquisitionStatus::Failed
                                                | crate::research::AcquisitionStatus::Cancelled
                                        )
                                })
                        });
                        self.research.insert(workspace.id, workspace);
                        self.io_status.insert(request_id, IoStatus::Completed);
                    }
                    Err(error) => {
                        self.research_error = Some(error.clone());
                        self.io_status.insert(request_id, IoStatus::Failed(error));
                    }
                }
            }
            StoreEvent::CodexProjectSaved {
                request_id,
                project,
                result,
            } => {
                self.io_pending.remove(&request_id);
                match result {
                    Ok(()) => {
                        self.codex_project_persisted = Some(project);
                        self.codex_project_error = None;
                        self.io_status.insert(request_id, IoStatus::Completed);
                    }
                    Err(error) => {
                        self.codex_project_error = Some(error.clone());
                        self.io_status.insert(request_id, IoStatus::Failed(error));
                    }
                }
            }
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
                    if let StoreRequest::SetAutomationWriteEnabled { request_id, .. } = request {
                        let error = "persistence worker disconnected".to_owned();
                        self.io_pending.remove(&request_id);
                        self.automation_permission_error = Some(error.clone());
                        self.io_status.insert(request_id, IoStatus::Failed(error));
                    } else {
                        self.storage_jobs.push_front(request);
                    }
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
    /// One shared, rebuildable parsed sub-world per accepted note revision.
    pub fn parsed_document(
        &mut self,
        id: NoteId,
    ) -> Result<std::sync::Arc<crate::document_world::DocumentWorld>, CommandError> {
        use crate::document_world::{DocumentWorld, ParsedDocument};
        let entity = *self.ids.get(&id).ok_or(CommandError::NotFound)?;
        let record = self
            .world
            .get::<NoteRecord>(entity)
            .ok_or(CommandError::NotFound)?;
        let previous = self
            .world
            .get::<ParsedDocument>(entity)
            .map(|document| document.0.clone());
        if let Some(document) = previous
            .as_ref()
            .filter(|document| document.revision() == record.revision)
        {
            return Ok(document.clone());
        }
        let document = std::sync::Arc::new(DocumentWorld::parse(
            id,
            record.revision,
            &record.text,
            previous.as_deref(),
        ));
        self.world
            .entity_mut(entity)
            .insert(ParsedDocument(document.clone()));
        Ok(document)
    }
    /// Effective permission; absence preserves the startup compatibility default.
    /// Grants become visible only after durable acknowledgement.
    pub fn automation_write_preference(&self) -> Option<bool> {
        self.automation_write_preference
    }
    pub fn automation_permission_pending(&self) -> bool {
        self.automation_permission_dirty
    }
    pub fn automation_permission_error(&self) -> Option<&str> {
        self.automation_permission_error.as_deref()
    }
    pub fn codex_project(&self) -> Option<&CodexProjectBinding> {
        self.codex_project.as_ref()
    }
    pub fn codex_project_pending(&self) -> bool {
        self.codex_project != self.codex_project_persisted
            || self
                .io_pending
                .values()
                .any(|job| matches!(job, PendingIo::CodexProject))
    }
    pub fn codex_project_error(&self) -> Option<&str> {
        self.codex_project_error.as_deref()
    }
    pub fn vault_directory(&self) -> Option<&Path> {
        self.vault_directory.as_deref()
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
        self.tick_research_assets();
        self.tick_research_acquisitions();
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
            if let Some(error) = &self.automation_permission_error {
                return Err(StoreError::Storage(error.clone()));
            }
            if let Some(error) = &self.codex_project_error {
                return Err(StoreError::Storage(error.clone()));
            }
            if let Some(error) = &self.research_error {
                return Err(StoreError::Storage(error.clone()));
            }
            let source_idle = !self.io_pending.values().any(|pending| {
                matches!(
                    pending,
                    PendingIo::Research(_) | PendingIo::ResearchAsset { .. }
                )
            }) && self.pending.is_empty()
                && self.inflight.is_empty()
                && !self.codex_project_pending()
                && !self.automation_permission_pending();
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
        self.research_assets.take();
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
                self.research_requests.remove(&id);
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
        if caller != Caller::Local {
            if let Command::Research(research) = &command {
                let target = match research {
                    crate::research::ResearchCommand::RetryAcquisition { id, job_id, .. } => {
                        Some((*id, vec![*job_id]))
                    }
                    crate::research::ResearchCommand::RetryAcquisitions { id, job_ids, .. } => {
                        Some((*id, job_ids.clone()))
                    }
                    _ => None,
                };
                if let Some((id, jobs)) = target {
                    if self.research.get(&id).is_some_and(|w| {
                        w.acquisitions.iter().any(|j| {
                            jobs.contains(&j.id)
                                && matches!(
                                    j.input,
                                    crate::research::ArtifactInput::LocalFile { .. }
                                )
                        })
                    }) {
                        return Err(CommandError::Unauthorized);
                    }
                }
            }
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
            Command::SetAutomationWriteEnabled { enabled } => {
                if self
                    .io_pending
                    .values()
                    .any(|job| matches!(job, PendingIo::AutomationPermission))
                {
                    return Err(CommandError::Busy);
                }
                let request_id = if self.store.is_some() {
                    self.accept_io()?
                } else {
                    Uuid::new_v4()
                };
                self.automation_permission_error = None;
                if self.store.is_some() {
                    // Disable legacy startup grants as soon as any preference is requested.
                    self.automation_write_preference = Some(false);
                    self.automation_permission_dirty = true;
                    self.io_pending
                        .insert(request_id, PendingIo::AutomationPermission);
                    self.storage_jobs
                        .push_back(StoreRequest::SetAutomationWriteEnabled {
                            request_id,
                            enabled,
                        });
                } else {
                    self.automation_write_preference = Some(enabled);
                    self.io_status.insert(request_id, IoStatus::Completed);
                }
                Ok(CommandResult::Accepted { request_id })
            }
            Command::Research(command) => self.execute_research(command),
            Command::GetCodexProject => Ok(CommandResult::CodexProject(self.codex_project.clone())),
            Command::SetCodexProject { expected, project } => {
                if self
                    .io_pending
                    .values()
                    .any(|job| matches!(job, PendingIo::CodexProject))
                {
                    return Err(CommandError::Busy);
                }
                if self.codex_project != expected {
                    return Err(CommandError::CodexProjectChanged);
                }
                if !project.valid() {
                    return Err(CommandError::InvalidCodexProject(
                        "name, id and creation key must be nonempty bounded text".into(),
                    ));
                }
                if let Some(current) = &self.codex_project {
                    if current.creation_key != project.creation_key
                        || (current.project_id.is_some()
                            && current.project_id != project.project_id)
                    {
                        return Err(CommandError::InvalidCodexProject(
                            "project identity cannot change".into(),
                        ));
                    }
                }
                let request_id = self.accept_io()?;
                self.codex_project = Some(project.clone());
                self.codex_project_error = None;
                self.io_pending.insert(request_id, PendingIo::CodexProject);
                self.storage_jobs.push_back(StoreRequest::SetCodexProject {
                    request_id,
                    project,
                });
                Ok(CommandResult::Accepted { request_id })
            }
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

impl App {
    pub fn research_workspaces(&self) -> Vec<crate::research::ResearchWorkspace> {
        let mut workspaces: Vec<_> = self.research.values().cloned().collect();
        workspaces.sort_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));
        workspaces
    }
    pub fn research_workspace(&self, id: Uuid) -> Option<&crate::research::ResearchWorkspace> {
        self.research.get(&id)
    }
    pub fn research_pending(&self, id: Uuid) -> bool {
        self.io_pending.values().any(|pending| match pending {
            PendingIo::Research(workspace) | PendingIo::ResearchAsset { workspace, .. } => {
                *workspace == id
            }
            _ => false,
        })
    }
    /// The exact workspace acknowledged for this request; inspect io_status for success.
    pub fn research_created_workspace(&self, request_id: Uuid) -> Option<Uuid> {
        self.research_requests.get(&request_id).copied()
    }
    /// A retained result remains available if metadata registration failed.
    pub fn research_artifact_receipt(
        &self,
        request_id: Uuid,
    ) -> Option<&crate::research::ResearchArtifact> {
        self.research_receipts
            .get(&request_id)
            .map(|(_, artifact)| artifact)
    }
    pub fn research_artifact_receipts(
        &self,
    ) -> Vec<(Uuid, Uuid, crate::research::ResearchArtifact)> {
        self.research_receipts
            .iter()
            .map(|(request, (workspace, artifact))| (*request, *workspace, artifact.clone()))
            .collect()
    }
    pub fn cancel_research_artifact(&self, request_id: Uuid) {
        if matches!(
            self.io_pending.get(&request_id),
            Some(PendingIo::ResearchAsset { .. })
        ) {
            if let Some(worker) = &self.research_assets {
                worker.cancel(request_id);
            }
        }
    }
    fn append_imports(
        workspace: &mut crate::research::ResearchWorkspace,
        paths: Vec<String>,
    ) -> Result<(), CommandError> {
        use crate::research::{ResearchSource, SourceKind, SourceMetadata, SourceOrigin};
        if paths.len() > 64 {
            return Err(CommandError::InvalidResearch(
                "choose at most 64 files".into(),
            ));
        }
        for path in paths {
            if path.is_empty() || path.len() > 16384 {
                return Err(CommandError::InvalidPath);
            }
            let source_id = Uuid::new_v4();
            let title = std::path::Path::new(&path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if workspace
                .sources
                .iter()
                .any(|s| s.metadata.origin_label.as_deref() == Some(&path))
            {
                continue;
            }
            workspace.sources.push(ResearchSource {
                id: source_id,
                title,
                kind: SourceKind::Text,
                origin: SourceOrigin::Imported,
                metadata: SourceMetadata {
                    origin_label: Some(path.clone()),
                    ..Default::default()
                },
                pages: vec![],
                selected: true,
                human_assessment: None,
                agent_assessment: None,
                paper: None,
            });
            workspace.acquisitions.push(crate::research::Acquisition {
                id: Uuid::new_v4(),
                source_id,
                input: crate::research::ArtifactInput::LocalFile { path },
                ocr: false,
                status: crate::research::AcquisitionStatus::Queued,
                error: None,
            });
        }
        Ok(())
    }
    /// Validated managed originals only; no arbitrary user-supplied viewer path.
    pub fn research_asset_path(&self, relative: &str) -> Result<std::path::PathBuf, CommandError> {
        let vault = self
            .vault_directory
            .as_ref()
            .ok_or(CommandError::NotFound)?;
        if !relative.starts_with("research-assets/")
            || std::path::Path::new(relative).components().count() != 2
            || !std::path::Path::new(relative)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err(CommandError::InvalidPath);
        }
        let path = vault
            .join(relative)
            .canonicalize()
            .map_err(|_| CommandError::NotFound)?;
        let directory = vault
            .join("research-assets")
            .canonicalize()
            .map_err(|_| CommandError::NotFound)?;
        if path.parent() != Some(directory.as_path()) || !path.is_file() {
            return Err(CommandError::InvalidPath);
        }
        Ok(path)
    }
    fn stage_research(
        &mut self,
        mut workspace: crate::research::ResearchWorkspace,
    ) -> Result<Uuid, CommandError> {
        crate::research::validate(&workspace)?;
        let expected = workspace.revision;
        workspace.revision = Revision::fresh();
        self.research_error = None;
        let request_id = self.accept_io()?;
        self.io_pending
            .insert(request_id, PendingIo::Research(workspace.id));
        self.storage_jobs.push_back(StoreRequest::SaveResearch {
            request_id,
            expected: Some(expected),
            workspace,
        });
        Ok(request_id)
    }
    pub fn research_jobs_active(&self) -> bool {
        self.research_error.is_none()
            && (self.research.values().any(|w| {
                w.acquisitions.iter().any(|j| {
                    matches!(
                        j.status,
                        crate::research::AcquisitionStatus::Queued
                            | crate::research::AcquisitionStatus::Running
                    )
                })
            }) || self
                .io_pending
                .values()
                .any(|p| matches!(p, PendingIo::Research(_) | PendingIo::ResearchAsset { .. })))
    }
    pub fn research_error(&self) -> Option<&str> {
        self.research_error.as_deref()
    }
    fn tick_research_acquisitions(&mut self) {
        if self.research_error.is_some() {
            return;
        }
        use crate::research::{AcquisitionStatus as S, ResearchCommand as R};
        let workspaces = self.research_workspaces();
        for mut workspace in workspaces {
            if self.research_pending(workspace.id) {
                continue;
            }
            // Finish failed transient requests durably before retrying another job.
            let failed = self
                .acquisition_requests
                .iter()
                .find_map(|(request, (id, job))| {
                    if *id == workspace.id {
                        match self.io_status.get(request) {
                            Some(IoStatus::Failed(error)) => Some((*request, *job, error.clone())),
                            _ => None,
                        }
                    } else {
                        None
                    }
                });
            if let Some((request, job_id, error)) = failed {
                if let Some(job) = workspace.acquisitions.iter_mut().find(|j| j.id == job_id) {
                    job.status = if self.acquisition_cancelled.remove(&job_id) {
                        S::Cancelled
                    } else {
                        S::Failed
                    };
                    job.error = Some(error.chars().take(4000).collect());
                }
                if self.stage_research(workspace).is_ok() {
                    self.acquisition_requests.remove(&request);
                }
                continue;
            }
            let Some(job) = workspace
                .acquisitions
                .iter()
                .find(|j| matches!(j.status, S::Running | S::Queued))
                .cloned()
            else {
                continue;
            };
            if job.status == S::Queued {
                workspace
                    .acquisitions
                    .iter_mut()
                    .find(|j| j.id == job.id)
                    .unwrap()
                    .status = S::Running;
                if self.stage_research(workspace).is_ok() {
                    self.acquisition_armed.insert(job.id);
                }
            } else if self.acquisition_armed.remove(&job.id) {
                let result = self.execute_research(R::SavePaper {
                    id: workspace.id,
                    expected: workspace.revision,
                    source_id: job.source_id,
                    input: job.input,
                    ocr: job.ocr,
                });
                match result {
                    Ok(CommandResult::Accepted { request_id }) => {
                        self.acquisition_requests
                            .insert(request_id, (workspace.id, job.id));
                    }
                    Err(error) => {
                        workspace
                            .acquisitions
                            .iter_mut()
                            .find(|j| j.id == job.id)
                            .unwrap()
                            .status = S::Failed;
                        workspace
                            .acquisitions
                            .iter_mut()
                            .find(|j| j.id == job.id)
                            .unwrap()
                            .error = Some(error.to_string());
                        let _ = self.stage_research(workspace);
                    }
                    _ => {}
                }
            } else {
                // A previous process stopped after arming a worker; do not silently rerun network I/O.
                workspace
                    .acquisitions
                    .iter_mut()
                    .find(|j| j.id == job.id)
                    .unwrap()
                    .status = S::Failed;
                workspace
                    .acquisitions
                    .iter_mut()
                    .find(|j| j.id == job.id)
                    .unwrap()
                    .error = Some("Interrupted before completion; retry this source".into());
                let _ = self.stage_research(workspace);
            }
        }
    }
    fn tick_research_assets(&mut self) {
        for _ in 0..16 {
            let Some(completion) = self
                .research_assets
                .as_ref()
                .and_then(crate::research_assets::ResearchAssetWorker::try_recv)
            else {
                break;
            };
            let request_id = completion.request_id;
            let Some(PendingIo::ResearchAsset {
                workspace: id,
                expected,
                title,
                thread,
                round_id,
                source_id,
                ocr,
                role,
                source_ids,
            }) = self.io_pending.remove(&request_id)
            else {
                continue;
            };
            let result = completion
                .result
                .map_err(CommandError::Unavailable)
                .and_then(|asset| {
                    let artifact = crate::research::ResearchArtifact {
                        id: Uuid::new_v4(),
                        title,
                        kind: asset.kind,
                        url: asset.url,
                        final_url: asset.final_url,
                        managed_path: asset.managed_path,
                        markdown: asset.markdown,
                        captured_at: asset.captured_at,
                        thread,
                        round_id,
                        role,
                        source_ids,
                    };
                    self.research_receipts
                        .insert(request_id, (id, artifact.clone()));
                    if let Some(source_id) = source_id {
                        self.research_paper_receipts.insert(
                            request_id,
                            PaperReceipt {
                                ocr,
                                source_id,
                                pages: asset.pages.unwrap_or_default(),
                                extraction_error: asset.extraction_error,
                            },
                        );
                    }
                    let mut workspace = self
                        .research
                        .get(&id)
                        .ok_or(CommandError::NotFound)?
                        .clone();
                    if workspace.revision != expected {
                        return Err(CommandError::StaleRevision {
                            current: workspace.revision,
                        });
                    }
                    crate::research::validate_provenance(
                        &workspace,
                        artifact.thread.as_ref(),
                        artifact.round_id,
                    )?;
                    if let Some(paper) = self.research_paper_receipts.get(&request_id) {
                        paper.attach(&mut workspace, &artifact)?;
                    }
                    if let Some((_, job_id)) = self.acquisition_requests.get(&request_id) {
                        if let Some(job) =
                            workspace.acquisitions.iter_mut().find(|j| j.id == *job_id)
                        {
                            job.status = if ocr
                                && self
                                    .research_paper_receipts
                                    .get(&request_id)
                                    .is_some_and(|p| p.extraction_error.is_some())
                            {
                                crate::research::AcquisitionStatus::Failed
                            } else {
                                crate::research::AcquisitionStatus::Ready
                            };
                            job.error = self
                                .research_paper_receipts
                                .get(&request_id)
                                .and_then(|paper| paper.extraction_error.clone());
                        }
                    }
                    workspace.artifacts.push(artifact);
                    crate::research::validate(&workspace)?;
                    workspace.revision = Revision::fresh();
                    self.io_pending.insert(request_id, PendingIo::Research(id));
                    self.storage_jobs.push_back(StoreRequest::SaveResearch {
                        request_id,
                        expected: Some(expected),
                        workspace,
                    });
                    Ok(())
                });
            if let Err(error) = result {
                let error = error.to_string();
                if !self.acquisition_requests.contains_key(&request_id) {
                    self.research_error = Some(error.clone());
                }
                self.io_status.insert(request_id, IoStatus::Failed(error));
            }
        }
    }
    fn execute_research(
        &mut self,
        command: crate::research::ResearchCommand,
    ) -> Result<CommandResult, CommandError> {
        use crate::research::{
            ResearchCommand as R, ResearchResult, ResearchSource, ResearchWorkspace, SourceKind,
            SourceMetadata, SourceOrigin, SourcePage,
        };
        if let R::CancelAcquisition {
            id,
            expected,
            job_id,
        } = &command
        {
            let current = self.research.get(id).ok_or(CommandError::NotFound)?;
            if current.revision != *expected {
                return Err(CommandError::StaleRevision {
                    current: current.revision,
                });
            }
            if let Some(request) =
                self.acquisition_requests
                    .iter()
                    .find_map(|(request, (workspace, job))| {
                        (*workspace == *id && *job == *job_id).then_some(*request)
                    })
            {
                if matches!(
                    self.io_pending.get(&request),
                    Some(PendingIo::ResearchAsset { .. })
                ) {
                    self.acquisition_cancelled.insert(*job_id);
                    self.cancel_research_artifact(request);
                    return Ok(CommandResult::Accepted {
                        request_id: request,
                    });
                }
            }
        }
        let target = command.target();
        let mut workspace = match &command {
            R::List => {
                return Ok(CommandResult::Research(ResearchResult::Workspaces(
                    self.research_workspaces(),
                )));
            }
            R::FindPassages { id, query, limit } => {
                return Ok(CommandResult::Research(ResearchResult::Passages(
                    crate::research_desktop::retrieve(
                        self.research.get(id).ok_or(CommandError::NotFound)?,
                        query,
                        *limit,
                    ),
                )));
            }
            R::EvidenceGaps { id } => {
                return Ok(CommandResult::Research(ResearchResult::Suggestions(
                    crate::research_desktop::evidence_gaps(
                        self.research.get(id).ok_or(CommandError::NotFound)?,
                    ),
                )));
            }
            R::Read { id } => {
                return self
                    .research
                    .get(id)
                    .cloned()
                    .map(|workspace| CommandResult::Research(ResearchResult::Workspace(workspace)))
                    .ok_or(CommandError::NotFound);
            }
            R::ReadSource {
                id,
                source_id,
                page,
                offset,
                limit,
            } => {
                let source = self
                    .research
                    .get(id)
                    .ok_or(CommandError::NotFound)?
                    .sources
                    .iter()
                    .find(|s| s.id == *source_id)
                    .ok_or(CommandError::NotFound)?;
                return crate::research::read_source(source, *page, *offset, *limit)
                    .map(|excerpt| CommandResult::Research(ResearchResult::Source(excerpt)));
            }
            R::ReadIndex { id } => {
                let workspace = self.research.get(id).ok_or(CommandError::NotFound)?;
                return Ok(CommandResult::Research(ResearchResult::Index(
                    crate::research::research_index(workspace),
                )));
            }
            R::ReadRound { id, round_id } => {
                return self
                    .research
                    .get(id)
                    .ok_or(CommandError::NotFound)?
                    .rounds
                    .iter()
                    .find(|round| round.id == *round_id)
                    .cloned()
                    .map(|round| CommandResult::Research(ResearchResult::Round(round)))
                    .ok_or(CommandError::NotFound);
            }
            R::ReadArtifact {
                id,
                artifact_id,
                offset,
                limit,
            } => {
                let artifact = self
                    .research
                    .get(id)
                    .ok_or(CommandError::NotFound)?
                    .artifacts
                    .iter()
                    .find(|artifact| artifact.id == *artifact_id)
                    .ok_or(CommandError::NotFound)?;
                return crate::research::read_artifact(artifact, *offset, *limit)
                    .map(|excerpt| CommandResult::Research(ResearchResult::Artifact(excerpt)));
            }
            R::BuildContext { id, mode, scope } => {
                return crate::research::build_context(
                    self.research.get(id).ok_or(CommandError::NotFound)?,
                    *mode,
                    *scope,
                )
                .map(|prompt| CommandResult::Research(ResearchResult::Prompt(prompt)));
            }
            R::BuildReport { id } => {
                return crate::research::report_markdown(
                    self.research.get(id).ok_or(CommandError::NotFound)?,
                )
                .map(|report| CommandResult::Research(ResearchResult::Report(report)));
            }
            R::BuildPrompt { id } => {
                return crate::research::build_prompt(
                    self.research.get(id).ok_or(CommandError::NotFound)?,
                )
                .map(|prompt| CommandResult::Research(ResearchResult::Prompt(prompt)));
            }
            R::Discover { .. } => {
                return Err(CommandError::Unavailable(
                    "research discovery provider is not configured".into(),
                ));
            }
            R::Create { title, question } => ResearchWorkspace {
                id: Uuid::new_v4(),
                revision: Revision::fresh(),
                title: title.clone(),
                question: question.clone(),
                sources: Vec::new(),
                research_notes: Vec::new(),
                brief: Default::default(),
                threads: Vec::new(),
                rounds: Vec::new(),
                artifacts: Vec::new(),
                acquisitions: Vec::new(),
                assessment_history: Vec::new(),
            },
            _ => {
                let (id, expected) =
                    target.ok_or(CommandError::InvalidResearch("missing workspace".into()))?;
                if self.research_pending(id) {
                    return Err(CommandError::Busy);
                }
                let workspace = self.research.get(&id).ok_or(CommandError::NotFound)?;
                if workspace.revision != expected {
                    return Err(CommandError::StaleRevision {
                        current: workspace.revision,
                    });
                }
                workspace.clone()
            }
        };
        match command {
            R::CollectReferences {
                candidates,
                thread,
                turn_id,
                ..
            } => {
                if thread
                    .as_ref()
                    .is_some_and(|thread| !crate::research::valid_thread(thread))
                    || turn_id.as_ref().is_some_and(|id| id.len() > 1024)
                {
                    return Err(CommandError::InvalidResearch(
                        "invalid reference provenance".into(),
                    ));
                }
                if candidates.len() > 64 {
                    return Err(CommandError::InvalidResearch(
                        "collect at most 64 candidates".into(),
                    ));
                }
                for candidate in candidates {
                    let url = crate::research_desktop::normalize_url(&candidate.url).ok_or(
                        CommandError::InvalidResearch("reference requires an HTTP(S) URL".into()),
                    )?;
                    if workspace.sources.iter().any(|source| {
                        source
                            .metadata
                            .url
                            .as_ref()
                            .and_then(|u| crate::research_desktop::normalize_url(u))
                            .as_ref()
                            == Some(&url)
                    }) {
                        continue;
                    }
                    workspace.sources.push(ResearchSource {
                        id: Uuid::new_v4(),
                        title: candidate.title,
                        kind: SourceKind::Text,
                        origin: SourceOrigin::Imported,
                        metadata: SourceMetadata {
                            author: candidate.author,
                            date: candidate.date,
                            url: Some(url.clone()),
                            origin_label: Some(format!(
                                "Reference only{}{}",
                                thread
                                    .as_ref()
                                    .map(|t| format!("; conversation {}", t.thread_id))
                                    .unwrap_or_default(),
                                turn_id
                                    .as_ref()
                                    .map(|t| format!("; turn {t}"))
                                    .unwrap_or_default()
                            )),
                        },
                        pages: vec![],
                        selected: true,
                        human_assessment: None,
                        agent_assessment: None,
                        paper: None,
                    });
                }
            }
            R::ImportSource { path, .. } => Self::append_imports(&mut workspace, vec![path])?,
            R::ImportSources { paths, .. } => Self::append_imports(&mut workspace, paths)?,
            R::AcquireSources { source_ids, .. } => {
                if source_ids.len() > 128 {
                    return Err(CommandError::InvalidResearch(
                        "choose at most 128 sources".into(),
                    ));
                }
                for source_id in source_ids {
                    let source = workspace
                        .sources
                        .iter()
                        .find(|s| s.id == source_id)
                        .ok_or(CommandError::NotFound)?;
                    if source.paper.is_some()
                        || workspace.acquisitions.iter().any(|j| {
                            j.source_id == source_id
                                && matches!(
                                    j.status,
                                    crate::research::AcquisitionStatus::Queued
                                        | crate::research::AcquisitionStatus::Running
                                )
                        })
                    {
                        continue;
                    }
                    let Some(url) = source.metadata.url.clone() else {
                        continue;
                    };
                    workspace.acquisitions.push(crate::research::Acquisition {
                        id: Uuid::new_v4(),
                        source_id,
                        input: crate::research::ArtifactInput::Url { url, archive: true },
                        ocr: false,
                        status: crate::research::AcquisitionStatus::Queued,
                        error: None,
                    });
                }
            }
            R::RetryAcquisitions { job_ids, .. } => {
                if job_ids.len() > 256 {
                    return Err(CommandError::InvalidResearch("too many jobs".into()));
                }
                self.acquisition_requests
                    .retain(|_, (_, job)| !job_ids.contains(job));
                let mut retry_sources: Vec<_> = workspace
                    .acquisitions
                    .iter()
                    .filter(|j| {
                        !job_ids.contains(&j.id)
                            && matches!(
                                j.status,
                                crate::research::AcquisitionStatus::Queued
                                    | crate::research::AcquisitionStatus::Running
                            )
                    })
                    .map(|j| j.source_id)
                    .collect();
                for job in workspace
                    .acquisitions
                    .iter_mut()
                    .filter(|j| job_ids.contains(&j.id))
                {
                    if job.status == crate::research::AcquisitionStatus::Ready
                        || retry_sources.contains(&job.source_id)
                    {
                        continue;
                    }
                    retry_sources.push(job.source_id);
                    self.acquisition_armed.remove(&job.id);
                    job.status = crate::research::AcquisitionStatus::Queued;
                    job.error = None;
                }
            }
            R::AcquireSource {
                source_id,
                input,
                ocr,
                ..
            } => {
                let source = workspace
                    .sources
                    .iter()
                    .find(|s| s.id == source_id)
                    .ok_or(CommandError::NotFound)?;
                if source.paper.is_some() && !ocr {
                    return Err(CommandError::InvalidResearch(
                        "source already retained".into(),
                    ));
                }
                if !matches!(
                    &input,
                    crate::research::ArtifactInput::Url { archive: true, .. }
                        | crate::research::ArtifactInput::LocalFile { .. }
                ) {
                    return Err(CommandError::InvalidResearch(
                        "choose a URL or file to acquire".into(),
                    ));
                }
                if workspace.acquisitions.iter().any(|j| {
                    j.source_id == source_id
                        && matches!(
                            j.status,
                            crate::research::AcquisitionStatus::Queued
                                | crate::research::AcquisitionStatus::Running
                        )
                }) {
                    return Err(CommandError::Busy);
                }
                workspace.acquisitions.push(crate::research::Acquisition {
                    id: Uuid::new_v4(),
                    source_id,
                    input,
                    ocr,
                    status: crate::research::AcquisitionStatus::Queued,
                    error: None,
                });
            }
            R::RetryAcquisition { job_id, .. } => {
                let source_id = workspace
                    .acquisitions
                    .iter()
                    .find(|j| j.id == job_id)
                    .ok_or(CommandError::NotFound)?
                    .source_id;
                if workspace.acquisitions.iter().any(|j| {
                    j.id != job_id
                        && j.source_id == source_id
                        && matches!(
                            j.status,
                            crate::research::AcquisitionStatus::Queued
                                | crate::research::AcquisitionStatus::Running
                        )
                }) {
                    return Err(CommandError::Busy);
                }
                let job = workspace
                    .acquisitions
                    .iter_mut()
                    .find(|j| j.id == job_id)
                    .ok_or(CommandError::NotFound)?;
                if !matches!(
                    job.status,
                    crate::research::AcquisitionStatus::Failed
                        | crate::research::AcquisitionStatus::Cancelled
                ) {
                    return Err(CommandError::InvalidResearch(
                        "only failed or cancelled acquisitions can be retried".into(),
                    ));
                }
                job.status = crate::research::AcquisitionStatus::Queued;
                job.error = None;
            }
            R::CancelAcquisition { job_id, .. } => {
                let job = workspace
                    .acquisitions
                    .iter_mut()
                    .find(|j| j.id == job_id)
                    .ok_or(CommandError::NotFound)?;
                job.status = crate::research::AcquisitionStatus::Cancelled;
                job.error = None;
            }
            R::SetArtifactRole {
                artifact_id, role, ..
            } => {
                workspace
                    .artifacts
                    .iter_mut()
                    .find(|a| a.id == artifact_id)
                    .ok_or(CommandError::NotFound)?
                    .role = role;
            }
            R::CreateOutput {
                id,
                expected,
                mut spec,
            } => {
                spec.source_urls.clear();
                crate::research_desktop::render_output(&spec)
                    .map_err(CommandError::InvalidResearch)?;
                let title = spec.title.clone();
                let mut source_ids = Vec::new();
                for marker in spec.cards.iter().flat_map(|card| &card.citations) {
                    let (source_id, page) =
                        crate::research::citation_locator(marker).ok_or_else(|| {
                            CommandError::InvalidResearch(
                                "output citation must name a retained source/page".into(),
                            )
                        })?;
                    let source = workspace
                        .sources
                        .iter()
                        .find(|s| s.id == source_id)
                        .ok_or(CommandError::NotFound)?;
                    if !crate::research::source_has_text(source)
                        || page.is_some_and(|page| {
                            !source.pages.iter().any(|p| p.number == Some(page))
                        })
                    {
                        return Err(CommandError::InvalidResearch(
                            "output citation has no captured evidence at this location".into(),
                        ));
                    }
                    if !source_ids.contains(&source_id) {
                        source_ids.push(source_id);
                        let original = source
                            .paper
                            .as_ref()
                            .and_then(|paper| {
                                workspace
                                    .artifacts
                                    .iter()
                                    .find(|a| a.id == paper.artifact_id)
                            })
                            .and_then(|a| a.managed_path.as_ref())
                            .and_then(|p| std::path::Path::new(p).file_name())
                            .map(|p| p.to_string_lossy().into_owned());
                        if let Some(url) = original.or_else(|| source.metadata.url.clone()) {
                            spec.source_urls.insert(source_id, url);
                        }
                    }
                }
                crate::research_desktop::render_output(&spec)
                    .map_err(CommandError::InvalidResearch)?;
                let request_id = self.accept_io()?;
                let Some(worker) = self.research_assets.as_ref() else {
                    self.io_status.remove(&request_id);
                    return Err(CommandError::Unavailable("open a vault first".into()));
                };
                if let Err(error) = worker.try_submit(crate::research_assets::AssetJob {
                    request_id,
                    input: crate::research::ArtifactInput::Generated { spec },
                    extract_text: false,
                    ocr: false,
                }) {
                    self.io_status.remove(&request_id);
                    return Err(CommandError::Unavailable(error));
                }
                self.io_pending.insert(
                    request_id,
                    PendingIo::ResearchAsset {
                        workspace: id,
                        expected,
                        title,
                        thread: None,
                        round_id: None,
                        source_id: None,
                        ocr: false,
                        role: crate::research::ArtifactRole::Output,
                        source_ids,
                    },
                );
                return Ok(CommandResult::Accepted { request_id });
            }
            R::SetBrief { brief, .. } => workspace.brief = brief,
            R::AssociateThread { thread, .. } => {
                if let Some(existing) = workspace
                    .threads
                    .iter_mut()
                    .find(|t| t.same_identity(&thread))
                {
                    *existing = thread;
                } else {
                    workspace.threads.push(thread);
                }
            }
            R::RemoveThread { thread, .. } => {
                let index = workspace
                    .threads
                    .iter()
                    .position(|t| t.same_identity(&thread))
                    .ok_or(CommandError::NotFound)?;
                // Historical checkpoint provenance remains immutable after unlinking.
                workspace.threads.remove(index);
            }
            R::AddRound { round, .. } => {
                crate::research::validate_provenance(&workspace, round.thread.as_ref(), None)?;
                workspace.rounds.push(round);
            }
            R::CollectArtifact {
                id,
                expected,
                title,
                input,
                thread,
                round_id,
            } => {
                crate::research::validate_provenance(&workspace, thread.as_ref(), round_id)?;
                if title.trim().is_empty() || title.len() > 1024 || workspace.artifacts.len() >= 512
                {
                    return Err(CommandError::InvalidResearch(
                        "invalid artifact title or artifact limit reached".into(),
                    ));
                }
                if self.research_receipts.len() >= 512 {
                    return Err(CommandError::Busy);
                }
                if self.research_assets.is_none() {
                    return Err(CommandError::Unavailable("open a vault first".into()));
                }
                let request_id = self.accept_io()?;
                let result = self
                    .research_assets
                    .as_ref()
                    .ok_or_else(|| CommandError::Unavailable("open a vault first".into()))?
                    .try_submit(crate::research_assets::AssetJob {
                        request_id,
                        input,
                        extract_text: false,
                        ocr: false,
                    });
                if let Err(error) = result {
                    self.io_status.remove(&request_id);
                    return Err(CommandError::Unavailable(error));
                }
                self.research_error = None;
                self.io_pending.insert(
                    request_id,
                    PendingIo::ResearchAsset {
                        workspace: id,
                        expected,
                        title,
                        thread,
                        round_id,
                        source_id: None,
                        ocr: false,
                        role: crate::research::ArtifactRole::Input,
                        source_ids: vec![],
                    },
                );
                return Ok(CommandResult::Accepted { request_id });
            }
            R::SavePaper {
                id,
                expected,
                source_id,
                input,
                ocr,
            } => {
                if !matches!(
                    &input,
                    crate::research::ArtifactInput::Url { archive: true, .. }
                        | crate::research::ArtifactInput::LocalFile { .. }
                ) {
                    return Err(CommandError::InvalidResearch(
                        "SavePaper requires an archived direct PDF URL or locally authorized file"
                            .into(),
                    ));
                }
                let source = workspace
                    .sources
                    .iter()
                    .find(|s| s.id == source_id)
                    .ok_or(CommandError::NotFound)?;
                if source.paper.is_some() && !ocr {
                    return Err(CommandError::InvalidResearch(
                        "source already has a retained paper; read it with ReadSource".into(),
                    ));
                }
                if workspace.artifacts.len() >= 512 || self.research_receipts.len() >= 512 {
                    return Err(CommandError::Busy);
                }
                let title = source.title.clone();
                let worker = self
                    .research_assets
                    .as_ref()
                    .ok_or_else(|| CommandError::Unavailable("open a vault first".into()))?;
                // Reserve only after confirming a worker exists, undo if submission fails.
                let _ = worker;
                let request_id = self.accept_io()?;
                if let Err(error) = self.research_assets.as_ref().unwrap().try_submit(
                    crate::research_assets::AssetJob {
                        request_id,
                        input,
                        extract_text: true,
                        ocr,
                    },
                ) {
                    self.io_status.remove(&request_id);
                    return Err(CommandError::Unavailable(error));
                }
                self.research_error = None;
                self.io_pending.insert(
                    request_id,
                    PendingIo::ResearchAsset {
                        workspace: id,
                        expected,
                        title,
                        thread: None,
                        round_id: None,
                        source_id: Some(source_id),
                        ocr,
                        role: crate::research::ArtifactRole::Input,
                        source_ids: vec![source_id],
                    },
                );
                return Ok(CommandResult::Accepted { request_id });
            }
            R::RetryArtifact { id, receipt_id, .. } => {
                let (target, artifact) = self
                    .research_receipts
                    .get(&receipt_id)
                    .ok_or(CommandError::NotFound)?;
                if *target != id {
                    return Err(CommandError::InvalidResearch(
                        "receipt belongs to another workspace".into(),
                    ));
                }
                crate::research::validate_provenance(
                    &workspace,
                    artifact.thread.as_ref(),
                    artifact.round_id,
                )?;
                if workspace.artifacts.iter().any(|a| a.id == artifact.id) {
                    return Err(CommandError::InvalidResearch(
                        "artifact already registered".into(),
                    ));
                }
                if let Some(paper) = self.research_paper_receipts.get(&receipt_id) {
                    paper.attach(&mut workspace, &artifact)?;
                }
                workspace.artifacts.push(artifact.clone());
            }
            R::Update {
                title, question, ..
            } => {
                workspace.title = title;
                workspace.question = question;
            }
            R::AddNote { note_id, .. } => {
                let note = self.note(note_id)?.note;
                if note.trashed {
                    return Err(CommandError::Trashed);
                }
                workspace.sources.push(ResearchSource {
                    id: Uuid::new_v4(),
                    title: note.path.clone(),
                    kind: SourceKind::Note,
                    origin: SourceOrigin::NoteSnapshot {
                        id: note.id,
                        revision: note.revision,
                    },
                    metadata: SourceMetadata {
                        origin_label: Some(note.path),
                        ..Default::default()
                    },
                    pages: vec![SourcePage {
                        number: None,
                        text: note.text,
                    }],
                    selected: true,
                    human_assessment: None,
                    agent_assessment: None,
                    paper: None,
                });
            }
            R::AddImported { source, .. } => {
                if !matches!(source.kind, SourceKind::Text | SourceKind::Pdf) {
                    return Err(CommandError::InvalidResearch(
                        "only local text and PDF imports are supported".into(),
                    ));
                }
                workspace.sources.push(ResearchSource {
                    id: Uuid::new_v4(),
                    title: source.title,
                    kind: source.kind,
                    origin: SourceOrigin::Imported,
                    metadata: source.metadata,
                    pages: source.pages,
                    selected: true,
                    human_assessment: None,
                    agent_assessment: None,
                    paper: None,
                });
            }
            R::RemoveSource { source_id, .. } => {
                workspace
                    .acquisitions
                    .retain(|job| job.source_id != source_id);
                let index = workspace
                    .sources
                    .iter()
                    .position(|source| source.id == source_id)
                    .ok_or(CommandError::NotFound)?;
                workspace.sources.remove(index);
            }
            R::SelectSource {
                source_id,
                selected,
                ..
            } => {
                workspace
                    .sources
                    .iter_mut()
                    .find(|source| source.id == source_id)
                    .ok_or(CommandError::NotFound)?
                    .selected = selected
            }
            R::AssessSource {
                source_id,
                human,
                assessment,
                ..
            } => {
                workspace
                    .assessment_history
                    .push(crate::research::AssessmentEvent {
                        source_id,
                        human,
                        assessment: assessment.clone(),
                    });
                let source = workspace
                    .sources
                    .iter_mut()
                    .find(|source| source.id == source_id)
                    .ok_or(CommandError::NotFound)?;
                if human {
                    source.human_assessment = Some(assessment);
                } else {
                    source.agent_assessment = Some(assessment);
                }
            }
            R::LinkNote { note_id, .. } => {
                let note = self.note(note_id)?;
                if note.note.trashed {
                    return Err(CommandError::Trashed);
                }
                if note.save.persisted != Some(note.note.revision) {
                    return Err(CommandError::InvalidResearch(
                        "save the note before linking it to research".into(),
                    ));
                }
                if !workspace.research_notes.contains(&note_id) {
                    workspace.research_notes.push(note_id);
                }
            }
            R::Create { .. } => {}
            _ => {
                return Err(CommandError::InvalidResearch(
                    "invalid workspace mutation".into(),
                ));
            }
        }
        crate::research::validate(&workspace)?;
        workspace.revision = Revision::fresh();
        let request_id = self.accept_io()?;
        self.research_error = None;
        self.io_pending
            .insert(request_id, PendingIo::Research(workspace.id));
        self.storage_jobs.push_back(StoreRequest::SaveResearch {
            request_id,
            expected: target.map(|(_, revision)| revision),
            workspace,
        });
        Ok(CommandResult::Accepted { request_id })
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
    fn desktop_batch_intents_are_durable_and_invalid_jobs_never_persist() {
        use crate::research::{AcquisitionStatus as S, ResearchCommand as R};
        let directory = std::env::temp_dir().join(format!("tanglewood-batch-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let create = request(
            &mut app,
            Command::Research(R::Create {
                title: "Batch".into(),
                question: String::new(),
            }),
        );
        assert_eq!(wait_io(&mut app, create), IoStatus::Completed);
        let id = app.research_created_workspace(create).unwrap();
        let expected = app.research_workspace(id).unwrap().revision;
        let refs = crate::research_desktop::references(
            "[A](https://example.org/a) [B](https://example.org/b)",
        );
        let collect = request(
            &mut app,
            Command::Research(R::CollectReferences {
                id,
                expected,
                candidates: refs.clone(),
                thread: None,
                turn_id: Some("actual-answer".into()),
            }),
        );
        assert_eq!(wait_io(&mut app, collect), IoStatus::Completed);
        let w = app.research_workspace(id).unwrap().clone();
        assert_eq!(w.sources.len(), 2);
        assert!(
            w.sources
                .iter()
                .all(|s| !crate::research::source_has_text(s))
        );
        assert!(
            app.execute(
                Command::Research(R::AcquireSource {
                    id,
                    expected: w.revision,
                    source_id: w.sources[0].id,
                    input: crate::research::ArtifactInput::Url {
                        url: "https://user:secret@example.org/x".into(),
                        archive: true
                    },
                    ocr: false
                }),
                Caller::AutomationReadWrite
            )
            .is_err()
        );
        let queue = request(
            &mut app,
            Command::Research(R::AcquireSources {
                id,
                expected: w.revision,
                source_ids: w.sources.iter().map(|s| s.id).collect(),
            }),
        );
        assert_eq!(wait_io(&mut app, queue), IoStatus::Completed);
        // Queue acknowledgement precedes starting any network worker. Dropping here simulates restart at that boundary.
        assert_eq!(app.research_workspace(id).unwrap().acquisitions.len(), 2);
        drop(app);
        let reopened = App::open(&directory).unwrap();
        let loaded = reopened.research_workspace(id).unwrap();
        assert_eq!(loaded.acquisitions.len(), 2);
        assert!(
            loaded
                .acquisitions
                .iter()
                .all(|j| matches!(j.status, S::Queued | S::Running))
        );
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn desktop_research_queue_outputs_and_legacy_roles_survive_restart() {
        use crate::research::{AcquisitionStatus as S, ArtifactRole, ResearchCommand as R};
        use crate::research_desktop::{OutputCard, OutputKind, OutputSpec};
        let directory = std::env::temp_dir().join(format!("tanglewood-desktop-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let create = request(
            &mut app,
            Command::Research(R::Create {
                title: "Research".into(),
                question: "Which boats?".into(),
            }),
        );
        assert_eq!(wait_io(&mut app, create), IoStatus::Completed);
        let id = app.research_created_workspace(create).unwrap();
        let file = directory.join("evidence.md");
        std::fs::write(&file, "# Boats\nSails use wind. Engine boats use fuel.\n").unwrap();
        let expected = app.research_workspace(id).unwrap().revision;
        let importing = request(
            &mut app,
            Command::Research(R::ImportSource {
                id,
                expected,
                path: file.to_string_lossy().into(),
            }),
        );
        assert_eq!(wait_io(&mut app, importing), IoStatus::Completed);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            app.tick();
            let w = app.research_workspace(id).unwrap();
            if w.acquisitions.iter().all(|job| job.status == S::Ready) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "queue failed: {:?}",
                w.acquisitions
            );
            std::thread::sleep(Duration::from_millis(3));
        }
        let w = app.research_workspace(id).unwrap().clone();
        assert_eq!(w.sources.len(), 1);
        assert!(crate::research::source_has_text(&w.sources[0]));
        assert_eq!(
            std::fs::read_to_string(
                app.research_asset_path(w.artifacts[0].managed_path.as_deref().unwrap())
                    .unwrap()
            )
            .unwrap(),
            "# Boats\nSails use wind. Engine boats use fuel.\n"
        );
        let source_id = w.sources[0].id;
        let mut failed_ocr = w.clone();
        PaperReceipt {
            ocr: true,
            source_id,
            pages: vec![],
            extraction_error: Some("OCR unavailable".into()),
        }
        .attach(&mut failed_ocr, &w.artifacts[0])
        .unwrap();
        assert_eq!(failed_ocr.sources[0], w.sources[0]);
        for kind in [
            OutputKind::Slides,
            OutputKind::Infographic,
            OutputKind::MindMap,
        ] {
            let expected = app.research_workspace(id).unwrap().revision;
            let output = request(
                &mut app,
                Command::Research(R::CreateOutput {
                    id,
                    expected,
                    spec: OutputSpec {
                        source_urls: Default::default(),
                        title: "Boat evidence".into(),
                        kind,
                        cards: vec![OutputCard {
                            heading: "Wind".into(),
                            body: "Sails use wind.".into(),
                            citations: vec![format!("[source:{source_id}]")],
                        }],
                    },
                }),
            );
            assert_eq!(wait_io(&mut app, output), IoStatus::Completed);
        }
        let w = app.research_workspace(id).unwrap().clone();
        assert_eq!(
            w.artifacts
                .iter()
                .filter(|a| a.role == ArtifactRole::Output)
                .count(),
            3
        );
        for artifact in w
            .artifacts
            .iter()
            .filter(|a| a.role == ArtifactRole::Output)
        {
            assert_eq!(artifact.source_ids, vec![source_id]);
            assert!(
                std::fs::read_to_string(
                    app.research_asset_path(artifact.managed_path.as_deref().unwrap())
                        .unwrap()
                )
                .unwrap()
                .contains("Boat evidence")
            );
        }
        if let Some(output) = std::env::var_os("TANGLEWOOD_DESKTOP_OUTPUT_EVIDENCE") {
            let output = std::path::PathBuf::from(output);
            std::fs::create_dir_all(&output).unwrap();
            for artifact in &w.artifacts {
                let path = app
                    .research_asset_path(artifact.managed_path.as_deref().unwrap())
                    .unwrap();
                std::fs::copy(&path, output.join(path.file_name().unwrap())).unwrap();
            }
            std::fs::write(
                output.join("workspace.json"),
                serde_json::to_vec_pretty(&w).unwrap(),
            )
            .unwrap();
        }
        let mut legacy = serde_json::to_value(&w).unwrap();
        legacy["artifacts"][0]
            .as_object_mut()
            .unwrap()
            .remove("role");
        let old: crate::research::ResearchWorkspace = serde_json::from_value(legacy).unwrap();
        assert_eq!(old.artifacts[0].role, ArtifactRole::Unclassified);
        app.shutdown().unwrap();
        drop(app);
        let reopened = App::open(&directory).unwrap();
        assert_eq!(reopened.research_workspace(id).unwrap(), &w);
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn paper_save_keeps_reference_identity_and_failed_extraction_after_restart() {
        use crate::research::{
            ArtifactInput, ImportedSource, ResearchCommand as R, SourceKind, SourceMetadata,
            SourcePage,
        };
        let directory =
            std::env::temp_dir().join(format!("tanglewood-paper-owner-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let created = request(
            &mut app,
            Command::Research(R::Create {
                title: "Papers".into(),
                question: String::new(),
            }),
        );
        assert_eq!(wait_io(&mut app, created), IoStatus::Completed);
        let id = app.research_created_workspace(created).unwrap();
        let expected = app.research_workspace(id).unwrap().revision;
        let add = request(
            &mut app,
            Command::Research(R::AddImported {
                id,
                expected,
                source: ImportedSource {
                    title: "Reference".into(),
                    kind: SourceKind::Text,
                    metadata: SourceMetadata {
                        url: Some("https://example.org/paper".into()),
                        ..Default::default()
                    },
                    pages: vec![SourcePage {
                        number: None,
                        text: "Reference only".into(),
                    }],
                },
            }),
        );
        assert_eq!(wait_io(&mut app, add), IoStatus::Completed);
        let source_id = app.research_workspace(id).unwrap().sources[0].id;
        let expected = app.research_workspace(id).unwrap().revision;
        let file = directory.join("malformed.pdf");
        std::fs::write(&file, b"%PDF-1.7\nbad PDF").unwrap();
        let db = rusqlite::Connection::open(directory.join("notes.sqlite3")).unwrap();
        db.execute_batch("CREATE TRIGGER reject_paper BEFORE UPDATE ON research_workspaces BEGIN SELECT RAISE(FAIL,'paper disk failure'); END;").unwrap();
        let saving = request(
            &mut app,
            Command::Research(R::SavePaper {
                ocr: false,
                id,
                expected,
                source_id,
                input: ArtifactInput::LocalFile {
                    path: file.to_string_lossy().into(),
                },
            }),
        );
        assert!(app.research_pending(id));
        assert!(matches!(wait_io(&mut app, saving), IoStatus::Failed(_)));
        assert!(
            app.research_workspace(id).unwrap().sources[0]
                .paper
                .is_none()
        );
        assert!(app.research_artifact_receipt(saving).is_some());
        db.execute_batch("DROP TRIGGER reject_paper").unwrap();
        drop(db);
        let expected = app.research_workspace(id).unwrap().revision;
        let retry = request(
            &mut app,
            Command::Research(R::RetryArtifact {
                id,
                expected,
                receipt_id: saving,
            }),
        );
        assert_eq!(wait_io(&mut app, retry), IoStatus::Completed);
        let workspace = app.research_workspace(id).unwrap().clone();
        assert_eq!(workspace.sources.len(), 1);
        assert_eq!(workspace.sources[0].id, source_id);
        assert_eq!(
            workspace.sources[0].metadata.url.as_deref(),
            Some("https://example.org/paper")
        );
        assert!(workspace.sources[0].pages.is_empty());
        let paper = workspace.sources[0].paper.as_ref().unwrap();
        assert!(paper.extraction_error.is_some());
        assert_eq!(workspace.artifacts.len(), 1);
        assert_eq!(workspace.artifacts[0].id, paper.artifact_id);
        let retained = directory.join(workspace.artifacts[0].managed_path.as_ref().unwrap());
        assert_eq!(std::fs::read(retained).unwrap(), b"%PDF-1.7\nbad PDF");
        assert!(
            app.execute(
                Command::Research(R::ReadSource {
                    id,
                    source_id,
                    page: 0,
                    offset: 0,
                    limit: 32768
                }),
                Caller::AutomationReadOnly
            )
            .is_err()
        );
        assert!(
            app.execute(
                Command::Research(R::SavePaper {
                    ocr: false,
                    id,
                    expected: workspace.revision,
                    source_id,
                    input: ArtifactInput::LocalFile {
                        path: file.to_string_lossy().into()
                    }
                }),
                Caller::Local
            )
            .is_err()
        );
        app.shutdown().unwrap();
        drop(app);
        let reopened = App::open(&directory).unwrap();
        assert_eq!(reopened.research_workspace(id).unwrap(), &workspace);
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn research_flow_records_restart_context_and_retained_receipt() {
        use crate::research::{
            ArtifactInput, ContextMode, ResearchBrief, ResearchCommand as R, ResearchRound,
            SourceScope, ThreadRef,
        };
        let directory = std::env::temp_dir().join(format!("tanglewood-flow-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let create = request(
            &mut app,
            Command::Research(R::Create {
                title: "Investigation".into(),
                question: "Which design?".into(),
            }),
        );
        assert_eq!(wait_io(&mut app, create), IoStatus::Completed);
        let id = app.research_created_workspace(create).unwrap();
        let brief = ResearchBrief {
            purpose: "Compare boats".into(),
            scope: "Coastal".into(),
            assumptions: "Small crew".into(),
            open_questions: "Hull?".into(),
        };
        let mutate = |app: &mut App, command| {
            let request = request(app, Command::Research(command));
            assert_eq!(wait_io(app, request), IoStatus::Completed);
        };
        let revision = app.research_workspace(id).unwrap().revision;
        mutate(
            &mut app,
            R::SetBrief {
                id,
                expected: revision,
                brief: brief.clone(),
            },
        );
        let thread = ThreadRef {
            provider: "codex".into(),
            project_id: Some("project".into()),
            thread_id: "thread-a".into(),
            title: "Hull design".into(),
        };
        for name in ["thread-a", "thread-b"] {
            let revision = app.research_workspace(id).unwrap().revision;
            mutate(
                &mut app,
                R::AssociateThread {
                    id,
                    expected: revision,
                    thread: ThreadRef {
                        thread_id: name.into(),
                        ..thread.clone()
                    },
                },
            );
        }
        let round = ResearchRound {
            id: Uuid::new_v4(),
            captured_at: 1,
            question: "Compare hulls".into(),
            summary: "Need further evidence".into(),
            open_questions: "Weather limits".into(),
            discussion: "User prefers a small crew".into(),
            thread: Some(thread.clone()),
            turn_id: Some("turn-1".into()),
            artifact_ids: vec![],
        };
        let revision = app.research_workspace(id).unwrap().revision;
        mutate(
            &mut app,
            R::AddRound {
                id,
                expected: revision,
                round: round.clone(),
            },
        );
        assert!(matches!(
            app.execute(
                Command::Research(R::SetBrief {
                    id,
                    expected: revision,
                    brief: brief.clone()
                }),
                Caller::Local
            ),
            Err(CommandError::StaleRevision { .. })
        ));
        let revision = app.research_workspace(id).unwrap().revision;
        let local = R::CollectArtifact {
            id,
            expected: revision,
            title: "Private file".into(),
            input: ArtifactInput::LocalFile {
                path: "/tmp/private".into(),
            },
            thread: None,
            round_id: None,
        };
        assert_eq!(
            app.execute(Command::Research(local), Caller::AutomationReadWrite)
                .unwrap_err(),
            CommandError::Unauthorized
        );
        let capture = R::CollectArtifact {
            id,
            expected: revision,
            title: "Notes".into(),
            input: ArtifactInput::Markdown {
                markdown: "# Comparison".into(),
                url: Some("https://example.com/evidence".into()),
            },
            thread: Some(thread.clone()),
            round_id: Some(round.id),
        };
        let request_id = request(&mut app, Command::Research(capture));
        assert!(app.research_pending(id));
        assert_eq!(wait_io(&mut app, request_id), IoStatus::Completed);
        // A retained original survives failed SQLite registration and can be retried.
        let connection = rusqlite::Connection::open(directory.join("notes.sqlite3")).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_capture BEFORE UPDATE ON research_workspaces BEGIN SELECT RAISE(FAIL,'capture registration failure'); END;").unwrap();
        let source_file = directory.join("original.png");
        std::fs::write(&source_file, b"retained original bytes").unwrap();
        let revision = app.research_workspace(id).unwrap().revision;
        let failed = request(
            &mut app,
            Command::Research(R::CollectArtifact {
                id,
                expected: revision,
                title: "Original image".into(),
                input: ArtifactInput::LocalFile {
                    path: source_file.to_string_lossy().into_owned(),
                },
                thread: None,
                round_id: None,
            }),
        );
        assert!(matches!(wait_io(&mut app, failed), IoStatus::Failed(_)));
        let receipt = app.research_artifact_receipt(failed).unwrap();
        assert_eq!(
            std::fs::read(directory.join(receipt.managed_path.as_ref().unwrap())).unwrap(),
            b"retained original bytes"
        );
        assert_eq!(app.research_workspace(id).unwrap().artifacts.len(), 1);
        connection
            .execute_batch("DROP TRIGGER fail_capture")
            .unwrap();
        drop(connection);
        mutate(
            &mut app,
            R::RetryArtifact {
                id,
                expected: revision,
                receipt_id: failed,
            },
        );
        assert!(app.research_artifact_receipt(failed).is_none());
        let workspace = app.research_workspace(id).unwrap().clone();
        assert_eq!(workspace.rounds, vec![round]);
        assert!(workspace.artifacts[0].managed_path.is_some());
        assert!(crate::research::build_prompt(&workspace).is_err());
        let context = crate::research::build_context(
            &workspace,
            ContextMode::Report,
            SourceScope::ExistingOnly,
        )
        .unwrap();
        assert!(
            context.contains("thread-a")
                && context.contains("thread-b")
                && context.contains("Need further evidence")
                && context.contains("NEW ordinary Markdown note")
        );
        let report = crate::research::report_markdown(&workspace).unwrap();
        assert!(report.contains("Need further evidence") && report.contains("retained local copy"));
        app.shutdown().unwrap();
        drop(app);
        let mut reopened = App::open(&directory).unwrap();
        assert_eq!(reopened.research_workspace(id).unwrap(), &workspace);
        reopened.shutdown().unwrap();
        drop(reopened);
        // Older JSON rows default the new records without a schema migration.
        let connection = rusqlite::Connection::open(directory.join("notes.sqlite3")).unwrap();
        let mut old = serde_json::to_value(&workspace).unwrap();
        for field in ["brief", "threads", "rounds", "artifacts"] {
            old.as_object_mut().unwrap().remove(field);
        }
        connection
            .execute(
                "UPDATE research_workspaces SET payload=?1 WHERE id=?2",
                rusqlite::params![old.to_string(), id.to_string()],
            )
            .unwrap();
        drop(connection);
        let mut reopened = App::open(&directory).unwrap();
        let loaded = reopened.research_workspace(id).unwrap();
        assert_eq!(loaded.brief, ResearchBrief::default());
        assert!(
            loaded.threads.is_empty() && loaded.rounds.is_empty() && loaded.artifacts.is_empty()
        );
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn research_snapshot_restart_cas_policy_and_storage_failure() {
        use crate::research::{ResearchCommand as R, SourceOrigin};
        let directory =
            std::env::temp_dir().join(format!("tanglewood-research-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let source = note(
            app.execute(
                Command::CreateNote {
                    path: "evidence.md".into(),
                    text: "original evidence".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        )
        .note;
        let create = R::Create {
            title: "Research".into(),
            question: "What is supported?".into(),
        };
        assert_eq!(
            app.execute(
                Command::Research(create.clone()),
                Caller::AutomationReadOnly
            )
            .unwrap_err(),
            CommandError::Unauthorized
        );
        let id = request(&mut app, Command::Research(create));
        assert!(app.research_workspaces().is_empty());
        assert_eq!(wait_io(&mut app, id), IoStatus::Completed);
        let workspace = app.research_workspaces().remove(0);
        let linked = note(
            app.execute(
                Command::CreateNote {
                    path: "research-note.md".into(),
                    text: "Research observations".into(),
                },
                Caller::Local,
            )
            .unwrap(),
        )
        .note;
        let link = R::LinkNote {
            id: workspace.id,
            expected: workspace.revision,
            note_id: linked.id,
        };
        assert!(matches!(
            app.execute(Command::Research(link.clone()), Caller::Local),
            Err(CommandError::InvalidResearch(_))
        ));
        assert!(
            app.research_workspace(workspace.id)
                .unwrap()
                .research_notes
                .is_empty()
        );
        app.flush().unwrap();
        let link_request = request(&mut app, Command::Research(link));
        assert_eq!(wait_io(&mut app, link_request), IoStatus::Completed);
        let workspace = app.research_workspace(workspace.id).unwrap().clone();
        assert_eq!(workspace.research_notes, vec![linked.id]);
        let capture = request(
            &mut app,
            Command::Research(R::AddNote {
                id: workspace.id,
                expected: workspace.revision,
                note_id: source.id,
            }),
        );
        assert!(app.research_pending(workspace.id));
        assert_eq!(wait_io(&mut app, capture), IoStatus::Completed);
        let captured = app.research_workspace(workspace.id).unwrap().clone();
        assert!(
            matches!(captured.sources[0].origin,SourceOrigin::NoteSnapshot{id,revision} if id==source.id && revision==source.revision)
        );
        assert!(matches!(
            app.execute(
                Command::Research(R::Update {
                    id: workspace.id,
                    expected: workspace.revision,
                    title: "stale".into(),
                    question: String::new()
                }),
                Caller::Local
            ),
            Err(CommandError::StaleRevision { .. })
        ));
        app.execute(
            Command::EditNote {
                id: source.id,
                expected: source.revision,
                edits: vec![TextEdit {
                    start: 0,
                    end: source.text.len(),
                    replacement: "changed".into(),
                }],
            },
            Caller::Local,
        )
        .unwrap();
        app.flush().unwrap();
        drop(app);
        let mut app = App::open(&directory).unwrap();
        assert_eq!(app.research_workspace(workspace.id).unwrap(), &captured);
        assert_eq!(captured.sources[0].pages[0].text, "original evidence");
        let prompt = crate::research::build_prompt(&captured).unwrap();
        assert!(prompt.contains(&format!("[source:{}]", captured.sources[0].id)));
        assert!(prompt.contains("original evidence"));
        let db = rusqlite::Connection::open(directory.join("notes.sqlite3")).unwrap();
        db.execute_batch("CREATE TRIGGER reject_research BEFORE UPDATE ON research_workspaces BEGIN SELECT RAISE(FAIL,'test full disk'); END;").unwrap();
        let failed = request(
            &mut app,
            Command::Research(R::Update {
                id: captured.id,
                expected: captured.revision,
                title: "not durable".into(),
                question: String::new(),
            }),
        );
        assert!(matches!(wait_io(&mut app, failed), IoStatus::Failed(_)));
        assert_eq!(app.research_workspace(captured.id).unwrap(), &captured);
        assert!(app.flush().is_err());
        drop(db);
        drop(app);
        let app = App::open(&directory).unwrap();
        assert_eq!(app.research_workspace(captured.id).unwrap(), &captured);
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn automation_permission_requires_local_durable_grant_and_revokes_immediately() {
        let directory = std::env::temp_dir().join(format!(
            "tanglewood-automation-permission-{}",
            Uuid::new_v4()
        ));
        let mut app = App::open(&directory).unwrap();
        assert_eq!(app.automation_write_preference(), None);
        for caller in [Caller::AutomationReadOnly, Caller::AutomationReadWrite] {
            assert!(matches!(
                app.execute(Command::SetAutomationWriteEnabled { enabled: true }, caller),
                Err(CommandError::Unauthorized)
            ));
        }
        let grant = request(
            &mut app,
            Command::SetAutomationWriteEnabled { enabled: true },
        );
        assert_eq!(app.automation_write_preference(), Some(false));
        assert!(app.automation_permission_pending());
        assert!(matches!(
            app.execute(
                Command::SetAutomationWriteEnabled { enabled: false },
                Caller::Local
            ),
            Err(CommandError::Busy)
        ));
        assert_eq!(wait_io(&mut app, grant), IoStatus::Completed);
        assert_eq!(app.automation_write_preference(), Some(true));
        app.shutdown().unwrap();
        drop(app);
        let mut app = App::open(&directory).unwrap();
        assert_eq!(app.automation_write_preference(), Some(true));
        // A real SQLite failure must never restore runtime write access.
        let db = rusqlite::Connection::open(directory.join("notes.sqlite3")).unwrap();
        db.execute_batch("CREATE TRIGGER reject_automation BEFORE UPDATE ON settings WHEN OLD.key='automation_write_enabled' BEGIN SELECT RAISE(ABORT, 'test disk failure'); END;").unwrap();
        let revoke = request(
            &mut app,
            Command::SetAutomationWriteEnabled { enabled: false },
        );
        assert_eq!(app.automation_write_preference(), Some(false));
        assert!(matches!(wait_io(&mut app, revoke), IoStatus::Failed(_)));
        assert_eq!(app.automation_write_preference(), Some(false));
        assert!(app.automation_permission_pending());
        assert!(app.automation_permission_error().is_some());
        assert!(app.shutdown().is_err());
        let grant = request(
            &mut app,
            Command::SetAutomationWriteEnabled { enabled: true },
        );
        assert!(matches!(wait_io(&mut app, grant), IoStatus::Failed(_)));
        assert_eq!(app.automation_write_preference(), Some(false));
        db.execute_batch("DROP TRIGGER reject_automation").unwrap();
        let retry = request(
            &mut app,
            Command::SetAutomationWriteEnabled { enabled: false },
        );
        assert_eq!(wait_io(&mut app, retry), IoStatus::Completed);
        assert!(!app.automation_permission_pending());
        assert!(app.automation_permission_error().is_none());
        app.shutdown().unwrap();
        drop(app);
        drop(db);
        let mut app = App::open(&directory).unwrap();
        assert_eq!(app.automation_write_preference(), Some(false));
        app.shutdown().unwrap();
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
        let mut memory = App::new();
        request(
            &mut memory,
            Command::SetAutomationWriteEnabled { enabled: true },
        );
        assert_eq!(memory.automation_write_preference(), Some(true));
        assert!(!memory.automation_permission_pending());
    }
    #[test]
    fn codex_project_cas_survives_restart_and_rejects_identity_changes() {
        let directory =
            std::env::temp_dir().join(format!("tanglewood-codex-project-{}", Uuid::new_v4()));
        let mut app = App::open(&directory).unwrap();
        let pending = CodexProjectBinding {
            project_id: None,
            name: "My vault".into(),
            creation_key: Uuid::new_v4().to_string(),
        };
        assert!(matches!(
            app.execute(
                Command::SetCodexProject {
                    expected: None,
                    project: pending.clone()
                },
                Caller::AutomationReadWrite
            ),
            Err(CommandError::Unauthorized)
        ));
        let id = request(
            &mut app,
            Command::SetCodexProject {
                expected: None,
                project: pending.clone(),
            },
        );
        assert!(app.codex_project_pending());
        assert_eq!(wait_io(&mut app, id), IoStatus::Completed);
        assert!(!app.codex_project_pending());
        assert!(matches!(
            app.execute(
                Command::SetCodexProject {
                    expected: None,
                    project: pending.clone()
                },
                Caller::Local
            ),
            Err(CommandError::CodexProjectChanged)
        ));
        let associated = CodexProjectBinding {
            project_id: Some("project-123".into()),
            ..pending.clone()
        };
        let id = request(
            &mut app,
            Command::SetCodexProject {
                expected: Some(pending),
                project: associated.clone(),
            },
        );
        assert_eq!(wait_io(&mut app, id), IoStatus::Completed);
        app.shutdown().unwrap();
        drop(app);
        let mut app = App::open(&directory).unwrap();
        assert_eq!(app.codex_project(), Some(&associated));
        let renamed = CodexProjectBinding {
            name: "Renamed vault".into(),
            ..associated.clone()
        };
        let id = request(
            &mut app,
            Command::SetCodexProject {
                expected: Some(associated.clone()),
                project: renamed.clone(),
            },
        );
        assert_eq!(wait_io(&mut app, id), IoStatus::Completed);
        let changed = CodexProjectBinding {
            project_id: Some("another-project".into()),
            ..renamed.clone()
        };
        assert!(matches!(
            app.execute(
                Command::SetCodexProject {
                    expected: Some(renamed.clone()),
                    project: changed
                },
                Caller::Local
            ),
            Err(CommandError::InvalidCodexProject(_))
        ));
        app.shutdown().unwrap();
        drop(app);
        let mut app = App::open(&directory).unwrap();
        assert_eq!(app.codex_project(), Some(&renamed));
        app.shutdown().unwrap();
        drop(app);
        // Inject a real SQLite failure while no application owns the vault.
        let db = rusqlite::Connection::open(directory.join("notes.sqlite3")).unwrap();
        db.execute_batch("CREATE TRIGGER reject_project BEFORE UPDATE ON codex_project BEGIN SELECT RAISE(ABORT, 'test disk failure'); END;").unwrap();
        drop(db);
        let mut app = App::open(&directory).unwrap();
        let unsaved = CodexProjectBinding {
            name: "Unsaved rename".into(),
            ..renamed.clone()
        };
        let id = request(
            &mut app,
            Command::SetCodexProject {
                expected: Some(renamed.clone()),
                project: unsaved.clone(),
            },
        );
        assert!(matches!(wait_io(&mut app, id), IoStatus::Failed(_)));
        assert_eq!(app.codex_project(), Some(&unsaved));
        assert!(app.codex_project_pending());
        assert!(app.codex_project_error().is_some());
        assert!(app.shutdown().is_err());
        drop(app);
        let mut app = App::open(&directory).unwrap();
        assert_eq!(app.codex_project(), Some(&renamed));
        assert!(!app.codex_project_pending());
        app.shutdown().unwrap();
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
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
