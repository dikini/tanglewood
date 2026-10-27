//! SQLite connection and durable mirror bookkeeping owned by one worker.
use crate::{
    files::ExportBaseline,
    notes::{CodexProjectBinding, NoteId, NoteRecord, Revision},
};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    path::Path,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::Duration,
};
use uuid::Uuid;
const CAPACITY: usize = 32;
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("vault already has a writer")]
    Locked,
    #[error("persistence worker disconnected")]
    Disconnected,
    #[error("capability unavailable: {0}")]
    Unavailable(String),
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MirrorConflict {
    pub conflict_id: i64,
    pub note: NoteRecord,
    pub path: String,
    pub external: Option<String>,
    pub kind: String,
}
#[derive(Default)]
pub struct StoreStartup {
    pub automation_write_enabled: Option<bool>,
    pub research: Vec<crate::research::ResearchWorkspace>,
    pub codex_project: Option<CodexProjectBinding>,
    #[cfg(feature = "lan")]
    pub preparation: Option<crate::lan_owner::Preparation>,
    #[cfg(feature = "sync")]
    pub history: Option<crate::sync::HistoryBundle>,
    pub notes: Vec<NoteRecord>,
    pub mirror_enabled: bool,
    pub baselines: HashMap<NoteId, ExportBaseline>,
    pub pending: Vec<NoteRecord>,
    pub conflicts: Vec<MirrorConflict>,
}
#[derive(Debug)]
pub struct SaveAck {
    pub note: NoteRecord,
    pub id: NoteId,
    pub revision: Revision,
    pub result: Result<(), String>,
    pub export_pending: bool,
}
#[derive(Debug)]
pub enum StoreEvent {
    AutomationWriteSaved {
        request_id: Uuid,
        enabled: bool,
        result: Result<(), String>,
    },
    ResearchSaved {
        request_id: Uuid,
        workspace: crate::research::ResearchWorkspace,
        result: Result<(), String>,
    },
    CodexProjectSaved {
        request_id: Uuid,
        project: CodexProjectBinding,
        result: Result<(), String>,
    },
    #[cfg(feature = "lan")]
    Prepared {
        request_id: Uuid,
        preparation: Option<crate::lan_owner::Preparation>,
        result: Result<(), String>,
    },
    #[cfg(feature = "lan")]
    Joined {
        request_id: Uuid,
        result: Result<(), String>,
    },
    #[cfg(feature = "sync")]
    Migrated {
        request_id: Uuid,
        result: Result<String, String>,
    },
    #[cfg(feature = "sync")]
    HistorySaved {
        generation: u64,
        notes: Vec<NoteRecord>,
        result: Result<bool, String>,
    },
    Saved(SaveAck),
    Configured {
        request_id: Uuid,
        enabled: bool,
        result: Result<Vec<NoteRecord>, String>,
    },
    ExportRecorded {
        note: NoteRecord,
        baseline: Option<ExportBaseline>,
        result: Result<(), String>,
    },
    ConflictRecorded {
        request_id: Option<Uuid>,
        conflict: MirrorConflict,
        result: Result<i64, String>,
    },
}
#[derive(Debug)]
pub enum StoreRequest {
    SetAutomationWriteEnabled {
        request_id: Uuid,
        enabled: bool,
    },
    SaveResearch {
        request_id: Uuid,
        expected: Option<Revision>,
        workspace: crate::research::ResearchWorkspace,
    },
    SetCodexProject {
        request_id: Uuid,
        project: CodexProjectBinding,
    },
    #[cfg(feature = "lan")]
    PrepareJoin {
        request_id: Uuid,
        preparation: Option<crate::lan_owner::Preparation>,
    },
    #[cfg(feature = "lan")]
    JoinVault {
        request_id: Uuid,
        history: crate::sync::HistoryBundle,
        notes: Vec<NoteRecord>,
    },
    #[cfg(feature = "sync")]
    Migrate {
        request_id: Uuid,
        history: crate::sync::HistoryBundle,
        notes: Vec<NoteRecord>,
    },
    #[cfg(feature = "sync")]
    SaveHistory {
        generation: u64,
        history: crate::sync::HistoryBundle,
        notes: Vec<NoteRecord>,
        conflicts: Vec<(NoteId, crate::sync::MergeConflicts)>,
    },
    Configure {
        request_id: Uuid,
        enabled: bool,
    },
    Exported {
        note: NoteRecord,
        baseline: Option<ExportBaseline>,
    },
    Conflict {
        request_id: Option<Uuid>,
        conflict: MirrorConflict,
    },
}
#[derive(Debug)]
pub enum SubmitError {
    Full(NoteRecord),
    Disconnected(NoteRecord),
}
#[derive(Debug)]
pub enum BatchSubmitError {
    Full(Vec<NoteRecord>),
    Disconnected(Vec<NoteRecord>),
}
enum Job {
    Save(Vec<NoteRecord>),
    Control(StoreRequest),
    Flush(mpsc::Sender<()>),
    Shutdown,
}
pub struct Store {
    jobs: SyncSender<Job>,
    acks: Receiver<StoreEvent>,
    worker: Option<JoinHandle<()>>,
}
impl Store {
    pub fn open(directory: &Path) -> Result<(Self, Vec<NoteRecord>), StoreError> {
        let (store, start) = Self::open_state(directory)?;
        Ok((store, start.notes))
    }
    pub fn open_state(directory: &Path) -> Result<(Self, StoreStartup), StoreError> {
        std::fs::create_dir_all(directory).map_err(storage)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("vault.lock"))
            .map_err(storage)?;
        lock.try_lock_exclusive().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                StoreError::Locked
            } else {
                storage(error)
            }
        })?;
        let database = directory.join("notes.sqlite3");
        let (jobs, work) = mpsc::sync_channel(CAPACITY);
        let (results, acks) = mpsc::sync_channel(CAPACITY);
        let (ready, startup) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("tanglewood-store".into())
            .spawn(move || {
                let _lock: File = lock;
                let (mut connection, state) = match initialize(&database) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                if ready.send(Ok(state)).is_err() {
                    return;
                }
                while let Ok(job) = work.recv() {
                    match job {
                        Job::Save(notes) => {
                            let outcome =
                                save(&mut connection, &notes).map_err(|error| error.to_string());
                            for note in notes {
                                let ack = SaveAck {
                                    id: note.id,
                                    revision: note.revision,
                                    note: note.clone(),
                                    result: outcome.as_ref().map(|_| ()).map_err(Clone::clone),
                                    export_pending: outcome.as_ref().copied().unwrap_or(false),
                                };
                                if results.send(StoreEvent::Saved(ack)).is_err() {
                                    return;
                                }
                            }
                        }
                        Job::Control(request) => {
                            let event = match request {
                                StoreRequest::SetAutomationWriteEnabled { request_id, enabled } => {
                                    let result = connection.execute(
                                        "INSERT INTO settings(key,value) VALUES('automation_write_enabled',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                                        params![enabled],
                                    ).map(|_| ()).map_err(|error| error.to_string());
                                    StoreEvent::AutomationWriteSaved { request_id, enabled, result }
                                },
                                StoreRequest::SaveResearch { request_id, expected, workspace } => {
                                    let result = save_research(&connection, expected, &workspace).map_err(|e|e.to_string());
                                    StoreEvent::ResearchSaved { request_id, workspace, result }
                                },
                                StoreRequest::SetCodexProject { request_id, project } => {
                                    let result = connection.execute("INSERT INTO codex_project(singleton,project_id,name,creation_key) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET project_id=excluded.project_id,name=excluded.name,creation_key=excluded.creation_key", params![project.project_id, project.name, project.creation_key]).map(|_| ()).map_err(|e| e.to_string());
                                    StoreEvent::CodexProjectSaved { request_id, project, result }
                                },
                                #[cfg(feature = "lan")]
                                StoreRequest::PrepareJoin {
                                    request_id,
                                    preparation,
                                } => {
                                    let result =
                                        prepare_join(&mut connection, preparation.as_ref())
                                            .map_err(|e| e.to_string());
                                    StoreEvent::Prepared {
                                        request_id,
                                        preparation,
                                        result,
                                    }
                                }
                                #[cfg(feature = "lan")]
                                StoreRequest::JoinVault {
                                    request_id,
                                    history,
                                    notes,
                                } => {
                                    let result =
                                        migrate(&mut connection, &database, &history, &notes, true)
                                            .map(|_| ())
                                            .map_err(|e| e.to_string());
                                    StoreEvent::Joined { request_id, result }
                                }
                                #[cfg(feature = "sync")]
                                StoreRequest::Migrate {
                                    request_id,
                                    history,
                                    notes,
                                } => StoreEvent::Migrated {
                                    request_id,
                                    result: migrate(
                                        &mut connection,
                                        &database,
                                        &history,
                                        &notes,
                                        false,
                                    )
                                    .map_err(|error| error.to_string()),
                                },
                                #[cfg(feature = "sync")]
                                StoreRequest::SaveHistory {
                                    generation,
                                    history,
                                    notes,
                                    conflicts,
                                } => {
                                    let result =
                                        save_history(&mut connection, &history, &notes, &conflicts)
                                            .map_err(|error| error.to_string());
                                    StoreEvent::HistorySaved {
                                        generation,
                                        notes,
                                        result,
                                    }
                                }
                                StoreRequest::Configure {
                                    request_id,
                                    enabled,
                                } => StoreEvent::Configured {
                                    request_id,
                                    enabled,
                                    result: configure(&mut connection, enabled)
                                        .map_err(|error| error.to_string()),
                                },
                                StoreRequest::Exported { note, baseline } => {
                                    let result =
                                        record_export(&mut connection, &note, baseline.as_ref())
                                            .map_err(|error| error.to_string());
                                    StoreEvent::ExportRecorded {
                                        note,
                                        baseline,
                                        result,
                                    }
                                }
                                StoreRequest::Conflict {
                                    request_id,
                                    conflict,
                                } => {
                                    let result = record_conflict(&mut connection, &conflict)
                                        .map_err(|error| error.to_string());
                                    StoreEvent::ConflictRecorded {
                                        request_id,
                                        conflict,
                                        result,
                                    }
                                }
                            };
                            if results.send(event).is_err() {
                                return;
                            }
                        }
                        Job::Flush(done) => {
                            let _ = done.send(());
                        }
                        Job::Shutdown => break,
                    }
                }
            })
            .map_err(storage)?;
        match startup.recv().map_err(|_| StoreError::Disconnected)? {
            Ok(state) => Ok((
                Self {
                    jobs,
                    acks,
                    worker: Some(worker),
                },
                state,
            )),
            Err(error) => {
                let _ = worker.join();
                Err(error)
            }
        }
    }
    pub fn try_save(&self, note: NoteRecord) -> Result<(), SubmitError> {
        self.try_save_batch(vec![note])
            .map_err(|error| match error {
                BatchSubmitError::Full(mut notes) => SubmitError::Full(notes.remove(0)),
                BatchSubmitError::Disconnected(mut notes) => {
                    SubmitError::Disconnected(notes.remove(0))
                }
            })
    }
    pub fn try_save_batch(&self, notes: Vec<NoteRecord>) -> Result<(), BatchSubmitError> {
        match self.jobs.try_send(Job::Save(notes)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(Job::Save(notes))) => Err(BatchSubmitError::Full(notes)),
            Err(TrySendError::Disconnected(Job::Save(notes))) => {
                Err(BatchSubmitError::Disconnected(notes))
            }
            _ => unreachable!(),
        }
    }
    pub fn try_request(&self, request: StoreRequest) -> Result<(), TrySendError<StoreRequest>> {
        match self.jobs.try_send(Job::Control(request)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(Job::Control(request))) => Err(TrySendError::Full(request)),
            Err(TrySendError::Disconnected(Job::Control(request))) => {
                Err(TrySendError::Disconnected(request))
            }
            _ => unreachable!(),
        }
    }
    pub fn try_recv(&self) -> Option<StoreEvent> {
        self.acks.try_recv().ok()
    }
    /// Blocking explicit boundary; pump every completion to avoid bounded-queue deadlocks.
    pub fn flush(&self) -> Result<Vec<StoreEvent>, StoreError> {
        let (done, barrier) = mpsc::channel();
        let mut events = Vec::new();
        self.send_pumping(Job::Flush(done), &mut events)?;
        loop {
            while let Ok(event) = self.acks.try_recv() {
                events.push(event);
            }
            match barrier.try_recv() {
                Ok(()) => {
                    while let Ok(event) = self.acks.try_recv() {
                        events.push(event);
                    }
                    return Ok(events);
                }
                Err(mpsc::TryRecvError::Disconnected) => return Err(StoreError::Disconnected),
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if let Ok(event) = self.acks.recv_timeout(Duration::from_millis(1)) {
                events.push(event);
            }
        }
    }
    fn send_pumping(&self, mut job: Job, events: &mut Vec<StoreEvent>) -> Result<(), StoreError> {
        loop {
            match self.jobs.try_send(job) {
                Ok(()) => return Ok(()),
                Err(TrySendError::Disconnected(_)) => return Err(StoreError::Disconnected),
                Err(TrySendError::Full(returned)) => job = returned,
            }
            if let Ok(event) = self.acks.recv_timeout(Duration::from_millis(1)) {
                events.push(event);
            }
        }
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let mut events = Vec::new();
        let _ = self.send_pumping(Job::Shutdown, &mut events);
        if let Some(worker) = self.worker.take() {
            while !worker.is_finished() {
                let _ = self.acks.recv_timeout(Duration::from_millis(1));
            }
            let _ = worker.join();
        }
    }
}
fn storage(error: impl std::fmt::Display) -> StoreError {
    StoreError::Storage(error.to_string())
}
fn initialize(path: &Path) -> Result<(Connection, StoreStartup), StoreError> {
    let connection = Connection::open(path).map_err(storage)?;
    require_sync_support(&connection, cfg!(feature = "sync"))?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(storage)?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(storage)?;
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(storage)?;
    if version > 3 {
        return Err(StoreError::Storage(format!(
            "unsupported schema version {version}"
        )));
    }
    connection.execute_batch("BEGIN;
CREATE TABLE IF NOT EXISTS notes(id TEXT PRIMARY KEY NOT NULL,path TEXT UNIQUE NOT NULL,text TEXT NOT NULL,revision TEXT NOT NULL,trashed INTEGER NOT NULL CHECK(trashed IN(0,1)));
CREATE TABLE IF NOT EXISTS research_workspaces(id TEXT PRIMARY KEY NOT NULL,revision TEXT NOT NULL,payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS codex_project(singleton INTEGER PRIMARY KEY CHECK(singleton=1),project_id TEXT,name TEXT NOT NULL,creation_key TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value INTEGER NOT NULL);
INSERT OR IGNORE INTO settings VALUES('mirror_enabled',0);
CREATE TABLE IF NOT EXISTS export_baselines(note_id TEXT PRIMARY KEY,path TEXT NOT NULL,text TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS pending_exports(id TEXT PRIMARY KEY,path TEXT NOT NULL,text TEXT NOT NULL,revision TEXT NOT NULL,trashed INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS mirror_conflicts(conflict_id INTEGER PRIMARY KEY,note_id TEXT NOT NULL,path TEXT NOT NULL,revision TEXT NOT NULL,database_text TEXT NOT NULL,note_path TEXT NOT NULL,trashed INTEGER NOT NULL,external_text TEXT,kind TEXT NOT NULL);
COMMIT;").map_err(storage)?;
    if version < 2 {
        connection
            .pragma_update(None, "user_version", 2)
            .map_err(storage)?;
    }
    #[cfg(feature="lan")]
    connection.execute_batch("CREATE TABLE IF NOT EXISTS lan_preparation(singleton INTEGER PRIMARY KEY CHECK(singleton=1),payload BLOB NOT NULL)").map_err(storage)?;
    let mut state = StoreStartup {
        automation_write_enabled: {
            let value: Option<i64> = connection
                .query_row(
                    "SELECT value FROM settings WHERE key='automation_write_enabled'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(storage)?;
            match value {
                None => None,
                Some(0) => Some(false),
                Some(1) => Some(true),
                Some(_) => return Err(storage("invalid persisted automation write permission")),
            }
        },
        research: load_research(&connection)?,
        codex_project: {
            let project: Option<CodexProjectBinding> = connection
                .query_row(
                    "SELECT project_id,name,creation_key FROM codex_project WHERE singleton=1",
                    [],
                    |row| {
                        Ok(CodexProjectBinding {
                            project_id: row.get(0)?,
                            name: row.get(1)?,
                            creation_key: row.get(2)?,
                        })
                    },
                )
                .optional()
                .map_err(storage)?;
            if project.as_ref().is_some_and(|project| !project.valid()) {
                return Err(storage("invalid persisted Codex project"));
            }
            project
        },
        #[cfg(feature = "lan")]
        preparation: {
            let bytes: Option<Vec<u8>> = connection
                .query_row(
                    "SELECT payload FROM lan_preparation WHERE singleton=1",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(storage)?;
            bytes
                .map(|bytes| serde_json::from_slice(&bytes).map_err(storage))
                .transpose()?
        },
        #[cfg(feature = "sync")]
        history: load_history(&connection).map_err(storage)?,
        notes: load_notes(&connection, "notes").map_err(storage)?,
        pending: load_notes(&connection, "pending_exports").map_err(storage)?,
        mirror_enabled: enabled(&connection).map_err(storage)?,
        ..Default::default()
    };
    {
        let mut statement = connection
            .prepare("SELECT note_id,path,text FROM export_baselines")
            .map_err(storage)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(storage)?;
        for row in rows {
            let (id, path, text) = row.map_err(storage)?;
            state.baselines.insert(
                NoteId(Uuid::parse_str(&id).map_err(storage)?),
                ExportBaseline { path, text },
            );
        }
    }
    {
        let mut statement=connection.prepare("SELECT conflict_id,note_id,path,revision,database_text,trashed,external_text,kind,note_path FROM mirror_conflicts ORDER BY conflict_id").map_err(storage)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                ))
            })
            .map_err(storage)?;
        for row in rows {
            let (conflict_id, id, path, revision, text, trashed, external, kind, note_path) =
                row.map_err(storage)?;
            state.conflicts.push(MirrorConflict {
                conflict_id,
                note: NoteRecord {
                    id: NoteId(Uuid::parse_str(&id).map_err(storage)?),
                    path: note_path,
                    text,
                    revision: Revision(Uuid::parse_str(&revision).map_err(storage)?),
                    trashed,
                },
                path,
                external,
                kind,
            });
        }
    }
    Ok((connection, state))
}
fn load_notes(connection: &Connection, table: &str) -> rusqlite::Result<Vec<NoteRecord>> {
    let mut statement = connection.prepare(&format!(
        "SELECT id,path,text,revision,trashed FROM {table} ORDER BY path"
    ))?;
    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let revision: String = row.get(3)?;
        let parse = |text: &str, index| {
            Uuid::parse_str(text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        };
        Ok(NoteRecord {
            id: NoteId(parse(&id, 0)?),
            path: row.get(1)?,
            text: row.get(2)?,
            revision: Revision(parse(&revision, 3)?),
            trashed: row.get(4)?,
        })
    })?;
    rows.collect()
}
fn enabled(connection: &Connection) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT value FROM settings WHERE key='mirror_enabled'",
        [],
        |row| row.get(0),
    )
}
fn save(connection: &mut Connection, notes: &[NoteRecord]) -> rusqlite::Result<bool> {
    if has_history(connection)? {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let transaction = connection.transaction()?;
    let mirror = write_notes(&transaction, notes)?;
    transaction.commit()?;
    Ok(mirror)
}
fn write_notes(
    transaction: &rusqlite::Transaction<'_>,
    notes: &[NoteRecord],
) -> rusqlite::Result<bool> {
    let mirror = enabled(transaction)?;
    for note in notes {
        transaction.execute(
            "UPDATE notes SET path=?1 WHERE id=?2",
            params![
                format!("/tanglewood-stage-{}", Uuid::new_v4()),
                note.id.0.to_string()
            ],
        )?;
    }
    for note in notes {
        transaction.execute("INSERT INTO notes(id,path,text,revision,trashed) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET path=excluded.path,text=excluded.text,revision=excluded.revision,trashed=excluded.trashed",params![note.id.0.to_string(),note.path,note.text,note.revision.0.to_string(),note.trashed])?;
        if mirror {
            transaction.execute("INSERT INTO pending_exports(id,path,text,revision,trashed) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET path=excluded.path,text=excluded.text,revision=excluded.revision,trashed=excluded.trashed",params![note.id.0.to_string(),note.path,note.text,note.revision.0.to_string(),note.trashed])?;
        }
    }
    Ok(mirror)
}
fn configure(connection: &mut Connection, on: bool) -> rusqlite::Result<Vec<NoteRecord>> {
    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE settings SET value=?1 WHERE key='mirror_enabled'",
        [on],
    )?;
    if on {
        transaction.execute("INSERT INTO pending_exports SELECT id,path,text,revision,trashed FROM notes WHERE true ON CONFLICT(id) DO UPDATE SET path=excluded.path,text=excluded.text,revision=excluded.revision,trashed=excluded.trashed",[])?;
    }
    transaction.commit()?;
    load_notes(connection, "pending_exports")
}
fn record_export(
    connection: &mut Connection,
    note: &NoteRecord,
    baseline: Option<&ExportBaseline>,
) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    match baseline {
        Some(base) => {
            transaction.execute("INSERT INTO export_baselines(note_id,path,text) VALUES(?1,?2,?3) ON CONFLICT(note_id) DO UPDATE SET path=excluded.path,text=excluded.text",params![note.id.0.to_string(),base.path,base.text])?;
        }
        None => {
            transaction.execute(
                "DELETE FROM export_baselines WHERE note_id=?1",
                [note.id.0.to_string()],
            )?;
        }
    }
    transaction.execute(
        "DELETE FROM pending_exports WHERE id=?1 AND revision=?2",
        params![note.id.0.to_string(), note.revision.0.to_string()],
    )?;
    transaction.commit()
}
fn record_conflict(
    connection: &mut Connection,
    conflict: &MirrorConflict,
) -> rusqlite::Result<i64> {
    let existing=connection.query_row("SELECT conflict_id FROM mirror_conflicts WHERE note_id=?1 AND revision=?2 AND path=?3 AND external_text IS ?4 AND kind=?5",params![conflict.note.id.0.to_string(),conflict.note.revision.0.to_string(),conflict.path,conflict.external,conflict.kind],|row|row.get(0)).optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    connection.execute("INSERT INTO mirror_conflicts(note_id,path,revision,database_text,trashed,external_text,kind,note_path) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![conflict.note.id.0.to_string(),conflict.path,conflict.note.revision.0.to_string(),conflict.note.text,conflict.note.trashed,conflict.external,conflict.kind,conflict.note.path])?;
    Ok(connection.last_insert_rowid())
}
#[cfg(feature = "lan")]
fn prepare_join(
    connection: &mut Connection,
    preparation: Option<&crate::lan_owner::Preparation>,
) -> Result<(), StoreError> {
    let transaction = connection.transaction().map_err(storage)?;
    if preparation.is_some()
        && (has_history(&transaction).map_err(storage)?
            || !load_notes(&transaction, "notes")
                .map_err(storage)?
                .is_empty())
    {
        return Err(StoreError::Storage(
            "join requires an empty unmigrated vault".into(),
        ));
    }
    match preparation {
        Some(preparation) => {
            let bytes = serde_json::to_vec(preparation).map_err(storage)?;
            transaction.execute("INSERT INTO lan_preparation VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET payload=excluded.payload",[bytes]).map_err(storage)?;
        }
        None => {
            transaction
                .execute("DELETE FROM lan_preparation", [])
                .map_err(storage)?;
        }
    }
    transaction.commit().map_err(storage)
}
fn has_history(connection: &Connection) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='sync_meta')",
        [],
        |row| row.get(0),
    )
}
fn require_sync_support(connection: &Connection, supported: bool) -> Result<(), StoreError> {
    if !supported && has_history(connection).map_err(storage)? {
        return Err(StoreError::Unavailable(
            "vault requires CRDT support".into(),
        ));
    }
    Ok(())
}
#[cfg(feature = "sync")]
fn load_history(connection: &Connection) -> rusqlite::Result<Option<crate::sync::HistoryBundle>> {
    if !has_history(connection)? {
        return Ok(None);
    }
    let (version, vault, device, catalog): (u32, String, String, Vec<u8>) = connection.query_row(
        "SELECT history_version,vault_id,device_id,catalog FROM sync_meta WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let parse = |value: &str| {
        Uuid::parse_str(value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    };
    let mut statement =
        connection.prepare("SELECT note_id,history FROM sync_documents ORDER BY note_id")?;
    let documents = statement
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(crate::sync::DocumentHistory {
                id: NoteId(parse(&id)?),
                bytes: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(crate::sync::HistoryBundle {
        version,
        vault_id: parse(&vault)?,
        device_id: parse(&device)?,
        catalog,
        documents,
    }))
}
#[cfg(feature = "sync")]
fn write_history(
    transaction: &rusqlite::Transaction<'_>,
    history: &crate::sync::HistoryBundle,
    conflicts: &[(NoteId, crate::sync::MergeConflicts)],
) -> Result<(), StoreError> {
    transaction.execute("UPDATE sync_meta SET history_version=?1,vault_id=?2,device_id=?3,catalog=?4 WHERE singleton=1",params![history.version,history.vault_id.to_string(),history.device_id.to_string(),history.catalog]).map_err(storage)?;
    for document in &history.documents {
        transaction.execute("INSERT INTO sync_documents(note_id,history) VALUES(?1,?2) ON CONFLICT(note_id) DO UPDATE SET history=excluded.history",params![document.id.0.to_string(),document.bytes]).map_err(storage)?;
    }
    transaction
        .execute("DELETE FROM sync_conflicts", [])
        .map_err(storage)?;
    for (id, metadata) in conflicts {
        let bytes = serde_json::to_vec(metadata).map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO sync_conflicts(note_id,metadata) VALUES(?1,?2)",
                params![id.0.to_string(), bytes],
            )
            .map_err(storage)?;
    }
    Ok(())
}
#[cfg(feature = "sync")]
fn migrate(
    connection: &mut Connection,
    database: &Path,
    history: &crate::sync::HistoryBundle,
    notes: &[NoteRecord],
    joining: bool,
) -> Result<String, StoreError> {
    if has_history(connection).map_err(storage)? {
        return Err(StoreError::Storage("vault already migrated".into()));
    }
    let mut current = load_notes(connection, "notes").map_err(storage)?;
    let mut supplied = notes.to_vec();
    current.sort_by_key(|note| note.id.0);
    supplied.sort_by_key(|note| note.id.0);
    if (joining && !current.is_empty()) || (!joining && current != supplied) {
        return Err(StoreError::Storage("migration source changed".into()));
    }
    let backup = database
        .parent()
        .ok_or_else(|| StoreError::Storage("vault directory missing".into()))?
        .join(format!("pre-crdt-{}.sqlite3", Uuid::new_v4()));
    // Reserve a fresh path without ever replacing an existing recovery copy.
    // VACUUM INTO accepts this empty file, which was created exclusively here.
    drop(
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&backup)
            .map_err(storage)?,
    );
    connection
        .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
        .map_err(storage)?;
    File::open(&backup)
        .and_then(|file| file.sync_all())
        .map_err(storage)?;
    File::open(backup.parent().unwrap())
        .and_then(|file| file.sync_all())
        .map_err(storage)?;
    let transaction = connection.transaction().map_err(storage)?;
    transaction.execute_batch("CREATE TABLE sync_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1),history_version INTEGER NOT NULL,vault_id TEXT NOT NULL,device_id TEXT NOT NULL,catalog BLOB NOT NULL,backup_path TEXT NOT NULL);CREATE TABLE sync_documents(note_id TEXT PRIMARY KEY NOT NULL,history BLOB NOT NULL);CREATE TABLE sync_conflicts(note_id TEXT PRIMARY KEY NOT NULL,metadata BLOB NOT NULL);").map_err(storage)?;
    transaction
        .execute(
            "INSERT INTO sync_meta VALUES(1,?1,?2,?3,?4,?5)",
            params![
                history.version,
                history.vault_id.to_string(),
                history.device_id.to_string(),
                history.catalog,
                backup.to_string_lossy()
            ],
        )
        .map_err(storage)?;
    write_history(&transaction, history, &[])?;
    write_notes(&transaction, notes).map_err(storage)?;
    transaction
        .pragma_update(None, "user_version", 3)
        .map_err(storage)?;
    #[cfg(feature = "lan")]
    transaction
        .execute("DELETE FROM lan_preparation", [])
        .map_err(storage)?;
    transaction.commit().map_err(storage)?;
    Ok(backup.to_string_lossy().into())
}
#[cfg(feature = "sync")]
fn save_history(
    connection: &mut Connection,
    history: &crate::sync::HistoryBundle,
    notes: &[NoteRecord],
    conflicts: &[(NoteId, crate::sync::MergeConflicts)],
) -> Result<bool, StoreError> {
    let durable = load_history(connection)
        .map_err(storage)?
        .ok_or_else(|| StoreError::Storage("vault not migrated".into()))?;
    if durable.vault_id != history.vault_id || durable.device_id != history.device_id {
        return Err(StoreError::Storage("history identity changed".into()));
    }
    let transaction = connection.transaction().map_err(storage)?;
    let mirror = write_notes(&transaction, notes).map_err(storage)?;
    write_history(&transaction, history, conflicts)?;
    transaction.commit().map_err(storage)?;
    Ok(mirror)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn committed_records_survive_restart_and_lock_prevents_second_writer() {
        let directory = std::env::temp_dir().join(format!("tanglewood-store-{}", Uuid::new_v4()));
        let (store, notes) = Store::open(&directory).unwrap();
        assert!(notes.is_empty());
        assert!(matches!(Store::open(&directory), Err(StoreError::Locked)));
        let mut note = NoteRecord {
            id: NoteId(Uuid::new_v4()),
            path: "a.md".into(),
            text: "α\r\n".into(),
            revision: Revision::fresh(),
            trashed: false,
        };
        // Exceed both queue capacities, pumping acknowledgements as the owner does.
        let mut completed = Vec::new();
        for n in 0..100 {
            note.text = format!("α {n}\r\n");
            note.revision = Revision::fresh();
            let mut pending = note.clone();
            loop {
                match store.try_save(pending) {
                    Ok(()) => break,
                    Err(SubmitError::Full(returned)) => pending = returned,
                    Err(error) => panic!("{error:?}"),
                }
                if let Some(ack) = store.try_recv() {
                    completed.push(ack);
                }
                thread::yield_now();
            }
        }
        completed.extend(store.flush().unwrap());
        assert_eq!(completed.len(), 100);
        assert!(
            completed
                .iter()
                .all(|event| matches!(event,StoreEvent::Saved(ack) if ack.result.is_ok()))
        );
        assert_eq!(
            match completed.last().unwrap() {
                StoreEvent::Saved(ack) => ack.revision,
                _ => panic!("save ack"),
            },
            note.revision
        );
        drop(store);
        let (store, loaded) = Store::open(&directory).unwrap();
        assert_eq!(loaded, vec![note.clone()]);
        let mut conflicting = note.clone();
        conflicting.id = NoteId(Uuid::new_v4());
        store.try_save(conflicting).unwrap();
        let failed = store.flush().unwrap();
        assert_eq!(failed.len(), 1);
        assert!(matches!(&failed[0],StoreEvent::Saved(ack) if ack.result.is_err()));
        drop(store);
        let (store, loaded) = Store::open(&directory).unwrap();
        assert_eq!(loaded, vec![note]);
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

fn save_research(
    connection: &Connection,
    expected: Option<Revision>,
    workspace: &crate::research::ResearchWorkspace,
) -> Result<(), StoreError> {
    let payload = serde_json::to_string(workspace).map_err(storage)?;
    let changed = if let Some(expected) = expected {
        connection
            .execute(
                "UPDATE research_workspaces SET revision=?1,payload=?2 WHERE id=?3 AND revision=?4",
                params![
                    workspace.revision.0.to_string(),
                    payload,
                    workspace.id.to_string(),
                    expected.0.to_string()
                ],
            )
            .map_err(storage)?
    } else {
        connection
            .execute(
                "INSERT INTO research_workspaces(id,revision,payload) VALUES(?1,?2,?3)",
                params![
                    workspace.id.to_string(),
                    workspace.revision.0.to_string(),
                    payload
                ],
            )
            .map_err(storage)?
    };
    if changed != 1 {
        return Err(storage("research workspace revision changed"));
    }
    Ok(())
}
fn load_research(
    connection: &Connection,
) -> Result<Vec<crate::research::ResearchWorkspace>, StoreError> {
    let mut statement = connection
        .prepare("SELECT id,revision,payload FROM research_workspaces ORDER BY id")
        .map_err(storage)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(storage)?;
    let mut workspaces = Vec::new();
    for row in rows {
        let (id, revision, payload) = row.map_err(storage)?;
        let workspace: crate::research::ResearchWorkspace =
            serde_json::from_str(&payload).map_err(storage)?;
        if workspace.id.to_string() != id || workspace.revision.0.to_string() != revision {
            return Err(storage(
                "research workspace identity disagrees with storage",
            ));
        }
        crate::research::validate(&workspace).map_err(storage)?;
        workspaces.push(workspace);
    }
    Ok(workspaces)
}
