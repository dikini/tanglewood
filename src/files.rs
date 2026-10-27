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
    #[cfg(feature = "wasm-extensions")]
    GuardedImport {
        request_id: Uuid,
        path: PathBuf,
        guard: crate::plugin_permissions::Guard,
        protected_registry: std::sync::Arc<fs::File>,
    },
    #[cfg(feature = "wasm-extensions")]
    GuardedExplicitExport {
        request_id: Uuid,
        note: NoteRecord,
        path: PathBuf,
        guard: crate::plugin_permissions::Guard,
        protected_registry: std::sync::Arc<fs::File>,
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
    #[cfg(feature = "wasm-extensions")]
    protected_registry: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<fs::File>>>>,
    #[cfg(feature = "wasm-extensions")]
    vault_root: PathBuf,
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
        #[cfg(feature = "wasm-extensions")]
        let vault_root = root.clone();
        #[cfg(feature = "wasm-extensions")]
        let protected_registry =
            std::sync::Arc::new(std::sync::Mutex::new(None::<std::sync::Arc<fs::File>>));
        #[cfg(feature = "wasm-extensions")]
        let worker_protection = protected_registry.clone();
        let worker = thread::Builder::new()
            .name("tanglewood-files".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    #[cfg(feature = "wasm-extensions")]
                    let protection = worker_protection.lock();
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
                        FileJob::Export(job) => {
                            #[cfg(feature = "wasm-extensions")]
                            let result = match &protection {
                                Ok(protection) => match protection.as_ref() {
                                    Some(registry) => {
                                        crate::plugin_io::mirror_export(&root, &job, registry)
                                    }
                                    None => export(&root, &job),
                                },
                                Err(_) => Err(io("plugin registry protection lock poisoned")),
                            };
                            #[cfg(not(feature = "wasm-extensions"))]
                            let result = export(&root, &job);
                            FileCompletion::Export {
                                id: job.note.id,
                                revision: job.note.revision,
                                result,
                            }
                        }
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
                                #[cfg(feature = "wasm-extensions")]
                                {
                                    match &protection {
                                        Ok(protection) => {
                                            if let Some(registry) = protection.as_ref() {
                                                if !valid_path(relative) {
                                                    return Err(FileError::InvalidPath);
                                                }
                                                return crate::plugin_io::export(
                                                    &root.join(relative),
                                                    &note.text,
                                                    &crate::plugin_permissions::Scope::All,
                                                    registry,
                                                );
                                            }
                                        }
                                        Err(_) => {
                                            return Err(io(
                                                "plugin registry protection lock poisoned",
                                            ));
                                        }
                                    }
                                }
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
                        #[cfg(feature = "wasm-extensions")]
                        FileJob::GuardedImport {
                            request_id,
                            path,
                            guard,
                            protected_registry,
                        } => {
                            let result = (|| {
                                if guard.rule.action
                                    != crate::plugin_permissions::Action::MarkdownImport
                                {
                                    return Err(FileError::InvalidPath);
                                }
                                let configured = protection
                                    .as_ref()
                                    .map_err(|_| io("plugin registry protection lock poisoned"))?
                                    .as_ref()
                                    .ok_or_else(|| {
                                        io("plugin registry protection is not configured")
                                    })?;
                                if !crate::plugin_io::same_directory(
                                    configured,
                                    &protected_registry,
                                )? {
                                    return Err(io("plugin registry protection changed"));
                                }
                                crate::plugin_io::read(&path, &guard, configured)
                            })();
                            FileCompletion::Import { request_id, result }
                        }
                        #[cfg(feature = "wasm-extensions")]
                        FileJob::GuardedExplicitExport {
                            request_id,
                            note,
                            path,
                            guard,
                            protected_registry,
                        } => {
                            let result = (|| {
                                if guard.rule.action
                                    != crate::plugin_permissions::Action::MarkdownExport
                                    || !guard.rule.notes.as_ref().is_some_and(|scope| match scope {
                                        crate::plugin_permissions::Scope::All => true,
                                        crate::plugin_permissions::Scope::Only(ids) => {
                                            ids.contains(&note.id.0)
                                        }
                                    })
                                {
                                    return Err(FileError::InvalidPath);
                                }
                                let configured = protection
                                    .as_ref()
                                    .map_err(|_| io("plugin registry protection lock poisoned"))?
                                    .as_ref()
                                    .ok_or_else(|| {
                                        io("plugin registry protection is not configured")
                                    })?;
                                if !crate::plugin_io::same_directory(
                                    configured,
                                    &protected_registry,
                                )? {
                                    return Err(io("plugin registry protection changed"));
                                }
                                if path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .is_none_or(|n| !valid_path(n))
                                {
                                    return Err(FileError::InvalidPath);
                                }
                                crate::plugin_io::guarded_export(
                                    &path, &note.text, &guard, configured,
                                )
                            })();
                            FileCompletion::ExplicitExport { request_id, result }
                        }
                    };
                    #[cfg(feature = "wasm-extensions")]
                    drop(protection);
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
            #[cfg(feature = "wasm-extensions")]
            protected_registry,
            #[cfg(feature = "wasm-extensions")]
            vault_root,
        })
    }
    /// Install protection before enabling the registry or guest file commands.
    #[cfg(feature = "wasm-extensions")]
    pub fn protect_plugin_registry(
        &self,
        registry: std::sync::Arc<fs::File>,
    ) -> Result<(), FileError> {
        let vault = fs::File::open(&self.vault_root).map_err(io)?;
        if crate::plugin_io::overlaps(&vault, &registry)? {
            return Err(io("plugin registry and vault directories overlap"));
        }
        let mut protection = self
            .protected_registry
            .try_lock()
            .map_err(|_| io("file worker is busy or registry protection is unavailable"))?;
        *protection = Some(registry);
        Ok(())
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

#[cfg(all(test, feature = "wasm-extensions", target_os = "linux"))]
mod plugin_tests {
    use super::*;
    use crate::plugin_permissions::{Action, Guard, Origin, Rule, Scope};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("tanglewood-protected-worker-{}", Uuid::new_v4()));
            for child in ["vault", "registry/plugin", "allowed"] {
                fs::create_dir_all(root.join(child)).unwrap();
            }
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn completion(worker: &FileWorker) -> FileCompletion {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(result) = worker.try_recv() {
                return result;
            }
            assert!(Instant::now() < deadline, "file worker timed out");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn worker_enforces_guard_and_blocks_local_registry_export() {
        let root = Fixture::new();
        let protected = Arc::new(fs::File::open(root.0.join("registry")).unwrap());
        let worker = FileWorker::open(&root.0.join("vault")).unwrap();
        worker.protect_plugin_registry(protected.clone()).unwrap();
        let allowed = root.0.join("allowed");
        fs::write(allowed.join("input.md"), "input").unwrap();
        let guard = Guard {
            origin: Origin::Local,
            rule: Rule {
                action: Action::MarkdownImport,
                notes: Some(Scope::All),
                workspaces: None,
                note_paths: None,
                filesystem_roots: Some(Scope::Only(vec![allowed.clone()])),
                url_origins: None,
                administrative_targets: None,
                input_kinds: None,
                human: None,
            },
        };
        worker
            .try_submit(FileJob::GuardedImport {
                request_id: Uuid::new_v4(),
                path: allowed.join("input.md"),
                guard,
                protected_registry: protected.clone(),
            })
            .unwrap();
        assert!(
            matches!(completion(&worker), FileCompletion::Import { result: Ok(text), .. } if text == "input")
        );
        let granted_id = NoteId(Uuid::new_v4());
        let another = NoteRecord {
            id: NoteId(Uuid::new_v4()),
            path: "other.md".into(),
            text: "other note".into(),
            revision: Revision(Uuid::new_v4()),
            trashed: false,
        };
        let export_guard = Guard {
            origin: Origin::Local,
            rule: Rule {
                action: Action::MarkdownExport,
                notes: Some(Scope::Only(vec![granted_id.0])),
                workspaces: None,
                note_paths: None,
                filesystem_roots: Some(Scope::Only(vec![allowed.clone()])),
                url_origins: None,
                administrative_targets: None,
                input_kinds: None,
                human: None,
            },
        };
        worker
            .try_submit(FileJob::GuardedExplicitExport {
                request_id: Uuid::new_v4(),
                note: another,
                path: allowed.join("other.md"),
                guard: export_guard,
                protected_registry: protected.clone(),
            })
            .unwrap();
        assert!(matches!(
            completion(&worker),
            FileCompletion::ExplicitExport { result: Err(_), .. }
        ));
        assert!(!allowed.join("other.md").exists());
        let manifest = root.0.join("registry/plugin/policy.md");
        fs::write(&manifest, "trusted policy").unwrap();
        let note = NoteRecord {
            id: NoteId(Uuid::new_v4()),
            path: "policy.md".into(),
            text: "attack".into(),
            revision: Revision(Uuid::new_v4()),
            trashed: false,
        };
        worker
            .try_submit(FileJob::ExplicitExport {
                request_id: Uuid::new_v4(),
                note,
                path: manifest.clone(),
            })
            .unwrap();
        assert!(matches!(
            completion(&worker),
            FileCompletion::ExplicitExport { result: Err(_), .. }
        ));
        assert_eq!(fs::read_to_string(manifest).unwrap(), "trusted policy");
        let overlap = FileWorker::open(&root.0.join("registry/plugin")).unwrap();
        assert!(overlap.protect_plugin_registry(protected).is_err());
    }
}
