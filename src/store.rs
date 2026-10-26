//! SQLite connection and durable mirror bookkeeping owned by one worker.
use crate::{
    files::ExportBaseline,
    notes::{NoteId, NoteRecord, Revision},
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
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(storage)?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(storage)?;
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(storage)?;
    if version > 2 {
        return Err(StoreError::Storage(format!(
            "unsupported schema version {version}"
        )));
    }
    connection.execute_batch("BEGIN;
CREATE TABLE IF NOT EXISTS notes(id TEXT PRIMARY KEY NOT NULL,path TEXT UNIQUE NOT NULL,text TEXT NOT NULL,revision TEXT NOT NULL,trashed INTEGER NOT NULL CHECK(trashed IN(0,1)));
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value INTEGER NOT NULL);
INSERT OR IGNORE INTO settings VALUES('mirror_enabled',0);
CREATE TABLE IF NOT EXISTS export_baselines(note_id TEXT PRIMARY KEY,path TEXT NOT NULL,text TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS pending_exports(id TEXT PRIMARY KEY,path TEXT NOT NULL,text TEXT NOT NULL,revision TEXT NOT NULL,trashed INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS mirror_conflicts(conflict_id INTEGER PRIMARY KEY,note_id TEXT NOT NULL,path TEXT NOT NULL,revision TEXT NOT NULL,database_text TEXT NOT NULL,note_path TEXT NOT NULL,trashed INTEGER NOT NULL,external_text TEXT,kind TEXT NOT NULL);
PRAGMA user_version=2;COMMIT;").map_err(storage)?;
    let mut state = StoreStartup {
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
    let transaction = connection.transaction()?;
    let mirror = enabled(&transaction)?;
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
    transaction.commit()?;
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
