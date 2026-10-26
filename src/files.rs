//! Concrete filesystem jobs; observations return to App commands, never mutate source.
use crate::notes::{NoteId, NoteRecord, Revision, valid_path};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread,
};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExportBaseline {
    pub path: String,
    pub text: String,
}
#[derive(Clone, Debug)]
pub struct ExportJob {
    pub note: NoteRecord,
    pub baseline: Option<ExportBaseline>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileError {
    InvalidPath,
    Symlink(String),
    Io(String),
    Conflict {
        path: String,
        external: Option<String>,
    },
}
impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FileError {}
fn io(error: impl std::fmt::Display) -> FileError {
    FileError::Io(error.to_string())
}

#[derive(Clone, Debug)]
pub enum FileJob {
    Export(ExportJob),
    /// Ownership collision: capture external bytes for durable retention without writing.
    RetainConflict {
        note: NoteRecord,
        path: String,
    },
    Import {
        request_id: Uuid,
        path: PathBuf,
    },
    ExplicitExport {
        request_id: Uuid,
        note: NoteRecord,
        path: PathBuf,
    },
    Rescan {
        request_id: Uuid,
    },
}
#[derive(Debug)]
pub enum FileCompletion {
    Export {
        id: NoteId,
        revision: Revision,
        result: Result<Option<ExportBaseline>, FileError>,
    },
    Import {
        request_id: Uuid,
        result: Result<String, FileError>,
    },
    ExplicitExport {
        request_id: Uuid,
        result: Result<(), FileError>,
    },
    Rescan {
        request_id: Uuid,
        result: Result<Vec<ExternalFile>, FileError>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalFile {
    pub path: String,
    pub text: String,
}

/// Bounded, nonblocking owner-facing I/O boundary. Dropping closes submission;
/// worker exits when completions disconnect. App retains unsent jobs on Full.
pub struct FileWorker {
    jobs: Option<SyncSender<FileJob>>,
    worker: Option<thread::JoinHandle<()>>,
    completions: Receiver<FileCompletion>,
}
impl FileWorker {
    pub fn open(root: &Path) -> Result<Self, FileError> {
        if fs::symlink_metadata(root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(FileError::Symlink(root.to_string_lossy().into()));
        }
        fs::create_dir_all(root).map_err(io)?;
        let root = fs::canonicalize(root).map_err(io)?;
        let (jobs, receiver) = mpsc::sync_channel(16);
        let (results, completions) = mpsc::sync_channel(16);
        let worker = thread::Builder::new()
            .name("tanglewood-files".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    let result = match job {
                        FileJob::RetainConflict { note, path } => {
                            let result = read(&root, &path)
                                .and_then(|external| Err(conflict(&path, external)));
                            FileCompletion::Export {
                                id: note.id,
                                revision: note.revision,
                                result,
                            }
                        }
                        FileJob::Export(job) => FileCompletion::Export {
                            id: job.note.id,
                            revision: job.note.revision,
                            result: export(&root, &job),
                        },
                        FileJob::Import { request_id, path } => FileCompletion::Import {
                            request_id,
                            result: fs::read_to_string(path).map_err(io),
                        },
                        FileJob::ExplicitExport {
                            request_id,
                            note,
                            path,
                        } => {
                            let result = (|| {
                                let parent = path
                                    .parent()
                                    .filter(|path| !path.as_os_str().is_empty())
                                    .unwrap_or_else(|| Path::new("."));
                                let root = fs::canonicalize(parent).map_err(io)?;
                                let relative = path
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .ok_or(FileError::InvalidPath)?;
                                let mut note = note;
                                note.path = relative.to_owned();
                                note.trashed = false;
                                export(
                                    &root,
                                    &ExportJob {
                                        note,
                                        baseline: None,
                                    },
                                )
                                .map(|_| ())
                            })();
                            FileCompletion::ExplicitExport { request_id, result }
                        }
                        FileJob::Rescan { request_id } => FileCompletion::Rescan {
                            request_id,
                            result: rescan(&root),
                        },
                    };
                    if results.send(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(io)?;
        Ok(Self {
            jobs: Some(jobs),
            completions,
            worker: Some(worker),
        })
    }
    pub fn try_submit(&self, job: FileJob) -> Result<(), TrySendError<FileJob>> {
        match &self.jobs {
            Some(jobs) => jobs.try_send(job),
            None => Err(TrySendError::Disconnected(job)),
        }
    }
    pub fn try_recv(&self) -> Option<FileCompletion> {
        self.completions.try_recv().ok()
    }
}

impl Drop for FileWorker {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(worker) = self.worker.take() {
            while !worker.is_finished() {
                let _ = self
                    .completions
                    .recv_timeout(std::time::Duration::from_millis(1));
            }
            let _ = worker.join();
        }
    }
}

/// Pure rescan decision; caller converts Import into revision-checked commands.
#[derive(Debug, PartialEq, Eq)]
pub enum Reconcile {
    Unchanged,
    Import(String),
    Conflict(Option<String>),
    Missing,
}
pub fn reconcile(
    note: &NoteRecord,
    baseline: Option<&ExportBaseline>,
    external: Option<&str>,
) -> Reconcile {
    let Some(external) = external else {
        return Reconcile::Missing;
    };
    if external == note.text {
        return Reconcile::Unchanged;
    }
    match baseline {
        Some(base) if external == base.text => Reconcile::Unchanged,
        Some(base) if note.text == base.text => Reconcile::Import(external.to_owned()),
        _ => Reconcile::Conflict(Some(external.to_owned())),
    }
}
fn checked(root: &Path, relative: &str) -> Result<PathBuf, FileError> {
    if !valid_path(relative) {
        return Err(FileError::InvalidPath);
    }
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(FileError::Symlink(relative.into()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
    }
    Ok(path)
}
fn read(root: &Path, path: &str) -> Result<Option<String>, FileError> {
    match fs::read_to_string(checked(root, path)?) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(error)),
    }
}
fn conflict(path: &str, external: Option<String>) -> FileError {
    FileError::Conflict {
        path: path.into(),
        external,
    }
}
fn export(root: &Path, job: &ExportJob) -> Result<Option<ExportBaseline>, FileError> {
    let note = &job.note;
    let target = read(root, &note.path)?;
    let same_path = job.baseline.as_ref().filter(|base| base.path == note.path);
    // A crash may have already replaced target before DB export acknowledgement.
    let desired = if note.trashed {
        None
    } else {
        Some(note.text.as_str())
    };
    if target.as_deref() != desired && target.as_deref() != same_path.map(|base| base.text.as_str())
    {
        return Err(conflict(&note.path, target));
    }
    if let Some(old) = &job.baseline {
        if old.path != note.path {
            let external = read(root, &old.path)?;
            if external.as_deref().is_some_and(|text| text != old.text) {
                return Err(conflict(&old.path, external));
            }
        }
    }
    if note.trashed {
        if target.is_some() {
            fs::remove_file(checked(root, &note.path)?).map_err(io)?;
            sync_parent(root, &note.path)?;
        }
    } else if target.as_deref() != desired {
        atomic_write(root, &note.path, &note.text)?;
    }
    if let Some(old) = &job.baseline {
        if old.path != note.path {
            let external = read(root, &old.path)?;
            if external.as_deref().is_some_and(|text| text != old.text) {
                return Err(conflict(&old.path, external));
            }
            if external.is_some() {
                fs::remove_file(checked(root, &old.path)?).map_err(io)?;
                sync_parent(root, &old.path)?;
            }
        }
    }
    if !note.trashed {
        fs::File::open(checked(root, &note.path)?)
            .and_then(|file| file.sync_all())
            .map_err(io)?;
        sync_parent(root, &note.path)?;
    }
    Ok((!note.trashed).then(|| ExportBaseline {
        path: note.path.clone(),
        text: note.text.clone(),
    }))
}
fn sync_parent(root: &Path, relative: &str) -> Result<(), FileError> {
    let path = checked(root, relative)?;
    let parent = path.parent().ok_or(FileError::InvalidPath)?;
    for directory in parent.ancestors() {
        fs::File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(io)?;
        if directory == root {
            break;
        }
    }
    if let Some(parent) = root.parent() {
        fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(io)?;
    }
    Ok(())
}
fn atomic_write(root: &Path, relative: &str, text: &str) -> Result<(), FileError> {
    let path = checked(root, relative)?;
    let parent = path.parent().ok_or(FileError::InvalidPath)?;
    fs::create_dir_all(parent).map_err(io)?;
    // Recheck ancestors after creating directories.
    let path = checked(root, relative)?;
    let temporary = parent.join(format!(".tanglewood-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(io)?;
        file.write_all(text.as_bytes()).map_err(io)?;
        file.sync_all().map_err(io)?;
        fs::rename(&temporary, path).map_err(io)?;
        sync_parent(root, relative)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn rescan(root: &Path) -> Result<Vec<ExternalFile>, FileError> {
    fn walk(root: &Path, directory: &Path, files: &mut Vec<ExternalFile>) -> Result<(), FileError> {
        for entry in fs::read_dir(directory).map_err(io)? {
            let entry = entry.map_err(io)?;
            let kind = entry.file_type().map_err(io)?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                walk(root, &entry.path(), files)?;
            } else if kind.is_file() && entry.path().extension().is_some_and(|ext| ext == "md") {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .map_err(io)?
                    .to_str()
                    .ok_or(FileError::InvalidPath)?
                    .to_owned();
                if let Some(text) = read(root, &relative)? {
                    files.push(ExternalFile {
                        path: relative,
                        text,
                    });
                }
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, root, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mirror_retries_preserve_external_conflicts_and_missing_notes() {
        let root = std::env::temp_dir().join(format!("tanglewood-files-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let mut note = NoteRecord {
            id: NoteId(Uuid::new_v4()),
            path: "a.md".into(),
            text: "original α\r\n".into(),
            revision: Revision::fresh(),
            trashed: false,
        };
        let job = ExportJob {
            note: note.clone(),
            baseline: None,
        };
        let baseline = export(&root, &job).unwrap().unwrap();
        assert_eq!(export(&root, &job).unwrap(), Some(baseline.clone()));
        fs::write(root.join("a.md"), "external 🦀").unwrap();
        assert_eq!(
            reconcile(&note, Some(&baseline), Some("external 🦀")),
            Reconcile::Import("external 🦀".into())
        );
        note.text = "internal".into();
        let changed = ExportJob {
            note: note.clone(),
            baseline: Some(baseline.clone()),
        };
        assert!(matches!(
            export(&root, &changed),
            Err(FileError::Conflict { .. })
        ));
        assert_eq!(
            fs::read_to_string(root.join("a.md")).unwrap(),
            "external 🦀"
        );
        assert!(matches!(
            reconcile(&note, Some(&baseline), Some("external 🦀")),
            Reconcile::Conflict(_)
        ));
        fs::remove_file(root.join("a.md")).unwrap();
        assert_eq!(reconcile(&note, Some(&baseline), None), Reconcile::Missing);
        assert!(matches!(
            export(&root, &changed),
            Err(FileError::Conflict { .. })
        ));
        fs::write(root.join("a.md"), &baseline.text).unwrap();
        note.path = "folder/b.md".into();
        let renamed = export(
            &root,
            &ExportJob {
                note: note.clone(),
                baseline: Some(baseline),
            },
        )
        .unwrap()
        .unwrap();
        assert!(!root.join("a.md").exists());
        assert_eq!(
            fs::read_to_string(root.join("folder/b.md")).unwrap(),
            "internal"
        );
        note.trashed = true;
        export(
            &root,
            &ExportJob {
                note,
                baseline: Some(renamed),
            },
        )
        .unwrap();
        assert!(!root.join("folder/b.md").exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(std::env::temp_dir(), root.join("escape")).unwrap();
            assert!(matches!(
                read(&root, "escape/x.md"),
                Err(FileError::Symlink(_))
            ));
            assert!(matches!(
                FileWorker::open(&root.join("escape")),
                Err(FileError::Symlink(_))
            ));
        }
        fs::remove_dir_all(root).unwrap();
    }
}
