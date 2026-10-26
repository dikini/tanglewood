//! Bounded persistence worker. Only this worker owns the SQLite connection.
use crate::notes::{NoteId, NoteRecord, Revision};
use fs2::FileExt;
use rusqlite::{Connection, params};
use std::{
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
#[derive(Debug)]
pub struct SaveAck {
    pub id: NoteId,
    pub revision: Revision,
    pub result: Result<(), String>,
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
    Flush(mpsc::Sender<()>),
    Shutdown,
}

pub struct Store {
    jobs: SyncSender<Job>,
    acks: Receiver<SaveAck>,
    worker: Option<JoinHandle<()>>,
}
impl Store {
    /// Startup may block; execute commands only after this handshake completes.
    pub fn open(directory: &Path) -> Result<(Self, Vec<NoteRecord>), StoreError> {
        std::fs::create_dir_all(directory).map_err(storage)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("vault.lock"))
            .map_err(storage)?;
        lock.try_lock_exclusive().map_err(|_| StoreError::Locked)?;
        let database = directory.join("notes.sqlite3");
        let (jobs, work) = mpsc::sync_channel(CAPACITY);
        let (results, acks) = mpsc::sync_channel(CAPACITY);
        let (ready, startup) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("tanglewood-store".into())
            .spawn(move || {
                // Keep the lock alive until the connection and worker are gone.
                let _lock: File = lock;
                let initialized = initialize(&database);
                let (mut connection, notes) = match initialized {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                if ready.send(Ok(notes)).is_err() {
                    return;
                }
                while let Ok(job) = work.recv() {
                    match job {
                        Job::Save(notes) => {
                            let result =
                                save(&mut connection, &notes).map_err(|error| error.to_string());
                            for note in notes {
                                if results
                                    .send(SaveAck {
                                        id: note.id,
                                        revision: note.revision,
                                        result: result.clone(),
                                    })
                                    .is_err()
                                {
                                    return;
                                }
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
            Ok(notes) => Ok((
                Self {
                    jobs,
                    acks,
                    worker: Some(worker),
                },
                notes,
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
    pub fn try_recv(&self) -> Option<SaveAck> {
        self.acks.try_recv().ok()
    }
    /// Wait for submitted jobs while pumping bounded completions. Owner must apply
    /// every returned ack and retain/retry failures. Does not submit App dirty work.
    pub fn flush(&self) -> Result<Vec<SaveAck>, StoreError> {
        let (done, barrier) = mpsc::channel();
        let mut acks = Vec::new();
        self.send_pumping(Job::Flush(done), &mut acks)?;
        loop {
            while let Ok(ack) = self.acks.try_recv() {
                acks.push(ack);
            }
            match barrier.try_recv() {
                Ok(()) => {
                    while let Ok(ack) = self.acks.try_recv() {
                        acks.push(ack);
                    }
                    return Ok(acks);
                }
                Err(mpsc::TryRecvError::Disconnected) => return Err(StoreError::Disconnected),
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if let Ok(ack) = self.acks.recv_timeout(Duration::from_millis(1)) {
                acks.push(ack);
            }
        }
    }
    fn send_pumping(&self, mut job: Job, acks: &mut Vec<SaveAck>) -> Result<(), StoreError> {
        loop {
            match self.jobs.try_send(job) {
                Ok(()) => return Ok(()),
                Err(TrySendError::Disconnected(_)) => return Err(StoreError::Disconnected),
                Err(TrySendError::Full(returned)) => job = returned,
            }
            if let Ok(ack) = self.acks.recv_timeout(Duration::from_millis(1)) {
                acks.push(ack);
            }
        }
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let mut completed = Vec::new();
        let _ = self.send_pumping(Job::Shutdown, &mut completed);
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
fn initialize(path: &Path) -> Result<(Connection, Vec<NoteRecord>), StoreError> {
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
    if version > 1 {
        return Err(StoreError::Storage(format!(
            "unsupported schema version {version}"
        )));
    }
    connection.execute_batch("BEGIN; CREATE TABLE IF NOT EXISTS notes (id TEXT PRIMARY KEY NOT NULL, path TEXT UNIQUE NOT NULL, text TEXT NOT NULL, revision TEXT NOT NULL, trashed INTEGER NOT NULL CHECK(trashed IN (0,1))); PRAGMA user_version=1; COMMIT;").map_err(storage)?;
    let notes = {
        let mut statement = connection
            .prepare("SELECT id,path,text,revision,trashed FROM notes ORDER BY path")
            .map_err(storage)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, bool>(4)?,
                ))
            })
            .map_err(storage)?;
        let mut notes = Vec::new();
        for row in rows {
            let (id, path, text, revision, trashed) = row.map_err(storage)?;
            notes.push(NoteRecord {
                id: NoteId(Uuid::parse_str(&id).map_err(storage)?),
                path,
                text,
                revision: Revision(Uuid::parse_str(&revision).map_err(storage)?),
                trashed,
            });
        }
        notes
    };
    Ok((connection, notes))
}
fn save(connection: &mut Connection, notes: &[NoteRecord]) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    // Absolute staging paths cannot collide with a valid vault-relative note.
    // These paths exist only inside this transaction and rollback on any failure.
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
        transaction.execute("INSERT INTO notes(id,path,text,revision,trashed) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET path=excluded.path,text=excluded.text,revision=excluded.revision,trashed=excluded.trashed", params![note.id.0.to_string(),note.path,note.text,note.revision.0.to_string(),note.trashed])?;
    }
    transaction.commit()
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
        assert!(completed.iter().all(|ack| ack.result.is_ok()));
        assert_eq!(completed.last().unwrap().revision, note.revision);
        drop(store);
        let (store, loaded) = Store::open(&directory).unwrap();
        assert_eq!(loaded, vec![note.clone()]);
        let mut conflicting = note.clone();
        conflicting.id = NoteId(Uuid::new_v4());
        store.try_save(conflicting).unwrap();
        let failed = store.flush().unwrap();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].result.is_err());
        drop(store);
        let (store, loaded) = Store::open(&directory).unwrap();
        assert_eq!(loaded, vec![note]);
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
