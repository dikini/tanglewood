//! Race-resistant filesystem operations for application-mediated plugin jobs.
//! All traversal is relative to already-open descriptors; symlinks are refused.
use crate::files::{ExportBaseline, ExportJob, FileError};
use crate::plugin_permissions::{Guard, Scope};
use std::fs::File;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::collections::HashSet;
    use std::ffi::{CString, OsStr};
    use std::io::{self, Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::Component;
    use uuid::Uuid;

    type Identity = (u64, u64);
    const MAX_REGISTRY_ENTRIES: usize = 8192;
    const MAX_DIRECTORY_DEPTH: usize = 128;
    const MAX_GUARDED_TEXT_BYTES: u64 = 8 * 1024 * 1024;

    fn bounded_text(file: File) -> Result<String, FileError> {
        if file.metadata().map_err(err)?.len() > MAX_GUARDED_TEXT_BYTES {
            return Err(err("guarded text operation exceeds 8 MiB"));
        }
        let mut text = String::new();
        file.take(MAX_GUARDED_TEXT_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(err)?;
        if text.len() as u64 > MAX_GUARDED_TEXT_BYTES {
            return Err(err("guarded text operation exceeds 8 MiB"));
        }
        Ok(text)
    }

    fn err(error: impl std::fmt::Display) -> FileError {
        FileError::Io(error.to_string())
    }
    fn identity(file: &File) -> io::Result<Identity> {
        let metadata = file.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    fn open_at(parent: &File, name: &OsStr, flags: i32, mode: u32) -> io::Result<File> {
        let name = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid path component"))?;
        // SAFETY: the parent descriptor and NUL-terminated name remain live;
        // successful descriptors are transferred exactly once to File.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_CLOEXEC,
                mode,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn directory(parent: &File, name: &OsStr) -> io::Result<File> {
        open_at(
            parent,
            name,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            0,
        )
    }
    fn file_read(parent: &File, name: &OsStr) -> io::Result<File> {
        let file = open_at(
            parent,
            name,
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            0,
        )?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a regular file",
            ));
        }
        Ok(file)
    }
    fn root() -> io::Result<File> {
        File::open("/")
    }
    fn safe_absolute(path: &Path) -> bool {
        path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
    }
    fn safe_relative(path: &Path) -> bool {
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
    }
    fn scoped(path: &Path, scope: &Scope<PathBuf>) -> bool {
        safe_absolute(path)
            && match scope {
                Scope::All => true,
                Scope::Only(roots) => roots.iter().any(|root| path.starts_with(root)),
            }
    }
    fn permitted_scope(guard: &Guard) -> Result<&Scope<PathBuf>, FileError> {
        guard.rule.validate().map_err(err)?;
        guard
            .rule
            .filesystem_roots
            .as_ref()
            .ok_or(FileError::InvalidPath)
    }
    fn no_registry() -> FileError {
        FileError::Io("plugin authority registry is protected".into())
    }

    /// Capture every directory and regular-file identity in the protected tree.
    /// Matching directories catches bind-mounted aliases; matching files catches
    /// hard links. Limits make a changed or unexpectedly large registry fail closed.
    fn protected_identities(protected: &File) -> io::Result<HashSet<Identity>> {
        let mut identities = HashSet::new();
        let mut entry_count = 0usize;
        let mut directories = vec![(protected.try_clone()?, 0usize)];
        while let Some((dir, depth)) = directories.pop() {
            if depth > MAX_DIRECTORY_DEPTH {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "registry directory depth exceeded",
                ));
            }
            let id = identity(&dir)?;
            if !identities.insert(id) {
                continue;
            }
            let path = PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()));
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                entry_count += 1;
                if entry_count > MAX_REGISTRY_ENTRIES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "registry entry limit exceeded",
                    ));
                }
                let entry = open_at(&dir, &entry.file_name(), libc::O_PATH | libc::O_NOFOLLOW, 0)?;
                let metadata = entry.metadata()?;
                if metadata.file_type().is_symlink() {
                    continue;
                }
                let id = (metadata.dev(), metadata.ino());
                if metadata.is_dir() {
                    // Reopen the pinned entry via procfs. It is the same inode,
                    // even if a directory entry is renamed while scanning.
                    let path = PathBuf::from(format!("/proc/self/fd/{}", entry.as_raw_fd()));
                    let child = File::open(path)?;
                    if identity(&child)? != id {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "registry changed during scan",
                        ));
                    }
                    directories.push((child, depth + 1));
                } else if metadata.is_file() {
                    identities.insert(id);
                }
            }
        }
        Ok(identities)
    }
    fn check_dir(dir: &File, ids: &HashSet<Identity>) -> Result<(), FileError> {
        if ids.contains(&identity(dir).map_err(err)?) {
            Err(no_registry())
        } else {
            Ok(())
        }
    }
    fn check_file(file: &File, ids: &HashSet<Identity>) -> Result<(), FileError> {
        if ids.contains(&identity(file).map_err(err)?) {
            Err(no_registry())
        } else {
            Ok(())
        }
    }
    fn mkdir_at(dir: &File, name: &OsStr) -> io::Result<()> {
        let name = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid directory"))?;
        // SAFETY: the descriptor and NUL-terminated component are valid.
        let result = unsafe { libc::mkdirat(dir.as_raw_fd(), name.as_ptr(), 0o700) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        dir.sync_all()
    }
    enum ParentError {
        Missing,
        Other(FileError),
    }
    impl ParentError {
        fn into_file(self) -> FileError {
            match self {
                Self::Missing => FileError::Io("directory does not exist".into()),
                Self::Other(error) => error,
            }
        }
    }
    fn parent_at(
        base: &File,
        path: &Path,
        ids: &HashSet<Identity>,
        create: bool,
    ) -> Result<(File, std::ffi::OsString), ParentError> {
        if !safe_relative(path) {
            return Err(ParentError::Other(FileError::InvalidPath));
        }
        let name = path
            .file_name()
            .ok_or(ParentError::Other(FileError::InvalidPath))?
            .to_os_string();
        let mut current = base.try_clone().map_err(|e| ParentError::Other(err(e)))?;
        check_dir(&current, ids).map_err(ParentError::Other)?;
        if let Some(parent) = path.parent() {
            for part in parent.components() {
                let Component::Normal(name) = part else {
                    return Err(ParentError::Other(FileError::InvalidPath));
                };
                let next = match directory(&current, name) {
                    Ok(next) => next,
                    Err(error) if error.kind() == io::ErrorKind::NotFound && create => {
                        match mkdir_at(&current, name) {
                            Ok(()) => {}
                            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                            Err(error) => return Err(ParentError::Other(err(error))),
                        }
                        directory(&current, name).map_err(|e| ParentError::Other(err(e)))?
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        return Err(ParentError::Missing);
                    }
                    Err(error) => return Err(ParentError::Other(err(error))),
                };
                check_dir(&next, ids).map_err(ParentError::Other)?;
                current = next;
            }
        }
        Ok((current, name))
    }
    fn absolute_parent(
        path: &Path,
        ids: &HashSet<Identity>,
    ) -> Result<(File, std::ffi::OsString), FileError> {
        if !safe_absolute(path) {
            return Err(FileError::InvalidPath);
        }
        let relative = path.strip_prefix("/").map_err(err)?;
        parent_at(&root().map_err(err)?, relative, ids, false).map_err(ParentError::into_file)
    }
    fn existing(
        parent: &File,
        name: &OsStr,
        ids: &HashSet<Identity>,
    ) -> Result<Option<String>, FileError> {
        let file = match file_read(parent, name) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(err(error)),
        };
        check_file(&file, ids)?;
        bounded_text(file).map(Some)
    }
    fn rename_at(parent: &File, old: &OsStr, new: &OsStr, no_replace: bool) -> io::Result<()> {
        let old = CString::new(old.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid old path"))?;
        let new = CString::new(new.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid new path"))?;
        // SAFETY: descriptors and both names are valid for the duration of the syscall.
        let rc = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                parent.as_raw_fd(),
                old.as_ptr(),
                parent.as_raw_fd(),
                new.as_ptr(),
                if no_replace {
                    libc::RENAME_NOREPLACE
                } else {
                    0
                },
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        parent.sync_all()
    }
    fn unlink_at(parent: &File, name: &OsStr) -> io::Result<()> {
        let name = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid path"))?;
        // SAFETY: descriptor and NUL-terminated name are valid.
        let rc = unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        parent.sync_all()
    }
    fn write_at(
        parent: &File,
        name: &OsStr,
        text: &str,
        no_replace: bool,
    ) -> Result<(), FileError> {
        let temporary = std::ffi::OsString::from(format!(".tanglewood-{}.tmp", Uuid::new_v4()));
        let mut file = open_at(
            parent,
            &temporary,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
            0o600,
        )
        .map_err(err)?;
        let result = (|| {
            file.write_all(text.as_bytes()).map_err(err)?;
            file.sync_all().map_err(err)?;
            rename_at(parent, &temporary, name, no_replace).map_err(err)
        })();
        if result.is_err() {
            let _ = unlink_at(parent, &temporary);
        }
        result
    }

    /// Return the authorized open file, so downstream binary/PDF workers can
    /// stream this descriptor instead of reopening the user-supplied path.
    pub fn open_read(path: &Path, guard: &Guard, protected: &File) -> Result<File, FileError> {
        if guard.origin != crate::plugin_permissions::Origin::Local {
            return Err(FileError::InvalidPath);
        }
        if !matches!(
            guard.rule.action,
            crate::plugin_permissions::Action::MarkdownImport
                | crate::plugin_permissions::Action::ResearchImportSource
                | crate::plugin_permissions::Action::ResearchImportSources
                | crate::plugin_permissions::Action::ResearchAcquireSource
                | crate::plugin_permissions::Action::ResearchSavePaper
                | crate::plugin_permissions::Action::ResearchCollectArtifact
                | crate::plugin_permissions::Action::ResearchRetryAcquisition
                | crate::plugin_permissions::Action::ResearchRetryAcquisitions
                | crate::plugin_permissions::Action::ResearchRetryArtifact
        ) {
            return Err(FileError::InvalidPath);
        }
        if guard
            .rule
            .input_kinds
            .as_ref()
            .is_some_and(|kinds| !kinds.contains(&crate::plugin_permissions::InputKind::LocalFile))
        {
            return Err(FileError::InvalidPath);
        }
        if !scoped(path, permitted_scope(guard)?) {
            return Err(FileError::InvalidPath);
        }
        let ids = protected_identities(protected).map_err(err)?;
        let (parent, name) = absolute_parent(path, &ids)?;
        let file = file_read(&parent, &name).map_err(err)?;
        check_file(&file, &ids)?;
        Ok(file)
    }
    pub fn read(path: &Path, guard: &Guard, protected: &File) -> Result<String, FileError> {
        bounded_text(open_read(path, guard, protected)?)
    }
    pub fn export(
        path: &Path,
        text: &str,
        scope: &Scope<PathBuf>,
        protected: &File,
    ) -> Result<(), FileError> {
        if !scoped(path, scope) {
            return Err(FileError::InvalidPath);
        }
        let ids = protected_identities(protected).map_err(err)?;
        let (parent, name) = absolute_parent(path, &ids)?;
        let current = existing(&parent, &name, &ids)?;
        if current.as_deref() == Some(text) {
            let file = file_read(&parent, &name).map_err(err)?;
            check_file(&file, &ids)?;
            file.sync_all().map_err(err)?;
            parent.sync_all().map_err(err)?;
            return Ok(());
        }
        if current.is_some() {
            return Err(FileError::Conflict {
                path: path.to_string_lossy().into(),
                external: current,
            });
        }
        write_at(&parent, &name, text, true)
    }
    pub fn guarded_export(
        path: &Path,
        text: &str,
        guard: &Guard,
        protected: &File,
    ) -> Result<(), FileError> {
        if guard.rule.action != crate::plugin_permissions::Action::MarkdownExport {
            return Err(FileError::InvalidPath);
        }
        export(path, text, permitted_scope(guard)?, protected)
    }

    /// Keep the existing mirror reconciliation semantics while pinning both
    /// destination directories before reading, replacing, renaming or removing.
    pub fn mirror_export(
        root: &Path,
        job: &ExportJob,
        protected: &File,
    ) -> Result<Option<ExportBaseline>, FileError> {
        let note = &job.note;
        if !crate::notes::valid_path(&note.path) {
            return Err(FileError::InvalidPath);
        }
        let ids = protected_identities(protected).map_err(err)?;
        let base = File::open(root).map_err(err)?;
        check_dir(&base, &ids)?;
        let destination = match parent_at(&base, Path::new(&note.path), &ids, !note.trashed) {
            Ok(pair) => Some(pair),
            Err(ParentError::Missing) if note.trashed => None,
            Err(error) => return Err(error.into_file()),
        };
        let target = match &destination {
            Some((parent, name)) => existing(parent, name, &ids)?,
            None => None,
        };
        let same_path = job.baseline.as_ref().filter(|v| v.path == note.path);
        let desired = if note.trashed {
            None
        } else {
            Some(note.text.as_str())
        };
        if target.as_deref() != desired && target.as_deref() != same_path.map(|v| v.text.as_str()) {
            return Err(FileError::Conflict {
                path: note.path.clone(),
                external: target,
            });
        }
        let mut previous = None;
        if let Some(old) = &job.baseline {
            if old.path != note.path {
                if !crate::notes::valid_path(&old.path) {
                    return Err(FileError::InvalidPath);
                }
                match parent_at(&base, Path::new(&old.path), &ids, false) {
                    Ok((old_parent, old_name)) => {
                        let external = existing(&old_parent, &old_name, &ids)?;
                        if external.as_deref().is_some_and(|text| text != old.text) {
                            return Err(FileError::Conflict {
                                path: old.path.clone(),
                                external,
                            });
                        }
                        previous = Some((old_parent, old_name, external.is_some()));
                    }
                    Err(ParentError::Missing) => {}
                    Err(error) => return Err(error.into_file()),
                }
            }
        }
        if let Some((parent, name)) = destination {
            if note.trashed {
                if target.is_some() {
                    unlink_at(&parent, &name).map_err(err)?;
                }
            } else if target.as_deref() != desired {
                write_at(&parent, &name, &note.text, target.is_none())?;
            }
            if !note.trashed {
                let file = file_read(&parent, &name).map_err(err)?;
                check_file(&file, &ids)?;
                file.sync_all().map_err(err)?;
                parent.sync_all().map_err(err)?;
            }
        }
        if let Some((old_parent, old_name, true)) = previous {
            unlink_at(&old_parent, &old_name).map_err(err)?;
        }
        Ok((!note.trashed).then(|| ExportBaseline {
            path: note.path.clone(),
            text: note.text.clone(),
        }))
    }
    fn ancestor(ancestor: &File, descendant: &File) -> io::Result<bool> {
        let wanted = identity(ancestor)?;
        let mut current = descendant.try_clone()?;
        for _ in 0..MAX_DIRECTORY_DEPTH {
            let current_id = identity(&current)?;
            if current_id == wanted {
                return Ok(true);
            }
            let parent = directory(&current, OsStr::new(".."))?;
            if identity(&parent)? == current_id {
                return Ok(false);
            }
            current = parent;
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "directory depth exceeded",
        ))
    }
    pub fn overlaps(a: &File, b: &File) -> Result<bool, FileError> {
        Ok(ancestor(a, b).map_err(err)? || ancestor(b, a).map_err(err)?)
    }
    pub fn same_directory(a: &File, b: &File) -> Result<bool, FileError> {
        Ok(a.metadata().map_err(err)?.is_dir()
            && b.metadata().map_err(err)?.is_dir()
            && identity(a).map_err(err)? == identity(b).map_err(err)?)
    }
}

#[cfg(target_os = "linux")]
pub use linux::{export, guarded_export, mirror_export, open_read, overlaps, read, same_directory};

#[cfg(not(target_os = "linux"))]
fn unsupported() -> FileError {
    FileError::Io("secure plugin file operations are unavailable on this platform".into())
}
#[cfg(not(target_os = "linux"))]
pub fn read(_: &Path, _: &Guard, _: &File) -> Result<String, FileError> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub fn open_read(_: &Path, _: &Guard, _: &File) -> Result<File, FileError> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub fn export(_: &Path, _: &str, _: &Scope<PathBuf>, _: &File) -> Result<(), FileError> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub fn guarded_export(_: &Path, _: &str, _: &Guard, _: &File) -> Result<(), FileError> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub fn mirror_export(
    _: &Path,
    _: &ExportJob,
    _: &File,
) -> Result<Option<ExportBaseline>, FileError> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub fn overlaps(_: &File, _: &File) -> Result<bool, FileError> {
    Err(unsupported())
}

#[cfg(not(target_os = "linux"))]
pub fn same_directory(_: &File, _: &File) -> Result<bool, FileError> {
    Err(unsupported())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::notes::{NoteId, NoteRecord, Revision};
    use crate::plugin_permissions::{Action, Origin, Rule};
    use std::fs;
    use std::os::unix::fs::symlink;
    use uuid::Uuid;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("tanglewood-plugin-io-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("registry/plugin")).unwrap();
            fs::create_dir_all(root.join("allowed")).unwrap();
            fs::create_dir_all(root.join("vault")).unwrap();
            Self(root)
        }
        fn protected(&self) -> File {
            File::open(self.0.join("registry")).unwrap()
        }
        fn guard(&self, action: Action) -> Guard {
            Guard {
                origin: Origin::Local,
                rule: Rule {
                    action,
                    notes: Some(Scope::All),
                    workspaces: None,
                    note_paths: if action == Action::MarkdownImport {
                        Some(Scope::All)
                    } else {
                        None
                    },
                    filesystem_roots: Some(Scope::Only(vec![self.0.join("allowed")])),
                    url_origins: None,
                    administrative_targets: None,
                    input_kinds: None,
                    human: None,
                },
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn guarded_io_confines_roots_rejects_links_and_preserves_conflicts() {
        let fixture = Fixture::new();
        let protected = fixture.protected();
        let allowed = fixture.0.join("allowed");
        fs::write(allowed.join("input.md"), "input").unwrap();
        let import = fixture.guard(Action::MarkdownImport);
        assert_eq!(
            read(&allowed.join("input.md"), &import, &protected).unwrap(),
            "input"
        );
        let mut automated = import.clone();
        automated.origin = Origin::AutomationReadWrite;
        assert!(read(&allowed.join("input.md"), &automated, &protected).is_err());
        fs::write(fixture.0.join("outside.md"), "outside").unwrap();
        assert!(read(&fixture.0.join("outside.md"), &import, &protected).is_err());
        symlink(fixture.0.join("outside.md"), allowed.join("link.md")).unwrap();
        assert!(read(&allowed.join("link.md"), &import, &protected).is_err());
        let export_guard = fixture.guard(Action::MarkdownExport);
        assert!(read(&allowed.join("input.md"), &export_guard, &protected).is_err());
        assert!(guarded_export(&allowed.join("wrong.md"), "wrong", &import, &protected).is_err());
        guarded_export(
            &allowed.join("new.md"),
            "authored",
            &export_guard,
            &protected,
        )
        .unwrap();
        assert!(matches!(
            guarded_export(
                &allowed.join("new.md"),
                "different",
                &export_guard,
                &protected
            ),
            Err(FileError::Conflict { .. })
        ));
        assert_eq!(
            fs::read_to_string(allowed.join("new.md")).unwrap(),
            "authored"
        );
    }
    #[test]
    fn registry_and_its_symlink_and_hardlink_aliases_are_protected() {
        let fixture = Fixture::new();
        let protected = fixture.protected();
        let manifest = fixture.0.join("registry/plugin/manifest.md");
        fs::write(&manifest, "trusted policy").unwrap();
        let allowed = fixture.0.join("allowed");
        symlink(
            fixture.0.join("registry/plugin"),
            allowed.join("plugin-link"),
        )
        .unwrap();
        fs::hard_link(&manifest, allowed.join("hardlink.md")).unwrap();
        let all = Scope::All;
        assert!(export(&manifest, "attack", &all, &protected).is_err());
        assert!(
            export(
                &allowed.join("plugin-link/manifest.md"),
                "attack",
                &all,
                &protected
            )
            .is_err()
        );
        assert!(export(&allowed.join("hardlink.md"), "attack", &all, &protected).is_err());
        assert_eq!(fs::read_to_string(manifest).unwrap(), "trusted policy");
    }
    #[test]
    fn protected_mirror_preserves_edit_rename_and_trash_semantics() {
        let fixture = Fixture::new();
        let protected = fixture.protected();
        let vault = fixture.0.join("vault");
        let mut note = NoteRecord {
            id: NoteId(Uuid::new_v4()),
            path: "one.md".into(),
            text: "first".into(),
            revision: Revision(Uuid::new_v4()),
            trashed: false,
        };
        let baseline = mirror_export(
            &vault,
            &ExportJob {
                note: note.clone(),
                baseline: None,
            },
            &protected,
        )
        .unwrap();
        note.text = "second".into();
        let baseline = mirror_export(
            &vault,
            &ExportJob {
                note: note.clone(),
                baseline,
            },
            &protected,
        )
        .unwrap();
        note.path = "nested/two.md".into();
        let baseline = mirror_export(
            &vault,
            &ExportJob {
                note: note.clone(),
                baseline,
            },
            &protected,
        )
        .unwrap();
        assert!(!vault.join("one.md").exists());
        assert_eq!(
            fs::read_to_string(vault.join("nested/two.md")).unwrap(),
            "second"
        );
        note.trashed = true;
        assert!(
            mirror_export(&vault, &ExportJob { note, baseline }, &protected)
                .unwrap()
                .is_none()
        );
        assert!(!vault.join("nested/two.md").exists());
    }
    #[test]
    fn guarded_sparse_inputs_and_existing_targets_have_bounded_reads() {
        let fixture = Fixture::new();
        let protected = fixture.protected();
        let path = fixture.0.join("allowed/large.md");
        File::create(&path)
            .unwrap()
            .set_len(2 * 1024 * 1024 * 1024)
            .unwrap();
        assert!(read(&path, &fixture.guard(Action::MarkdownImport), &protected).is_err());
        assert!(
            guarded_export(
                &path,
                "replacement",
                &fixture.guard(Action::MarkdownExport),
                &protected
            )
            .is_err()
        );
        assert_eq!(fs::metadata(path).unwrap().len(), 2 * 1024 * 1024 * 1024);
    }
}
