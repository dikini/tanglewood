//! Locally controlled plugin bundles. A run binds to loaded bytes and one
//! registry generation; reload or revoke invalidates the old binding.
use crate::plugin_manifest::{
    Kind, MAX_COMPONENT_BYTES, MAX_MANIFEST_BYTES, Manifest, ManifestError, valid_id,
};
use crate::plugin_permissions::Origin;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("plugin registry I/O: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("invalid plugin ID")]
    InvalidId,
    #[error("bundle manifest ID does not match its registry ID")]
    IdMismatch,
    #[error("component digest does not match manifest")]
    DigestMismatch,
    #[error("plugin is not registered")]
    NotRegistered,
    #[error("invocation is not authorized")]
    Denied,
    #[error("registry generation exhausted")]
    GenerationExhausted,
}

#[derive(Clone, Debug)]
pub struct PluginBinding {
    pub id: String,
    pub generation: u64,
    pub vault_id: Uuid,
    pub vault_generation: u64,
    pub origin: Origin,
    pub manifest: Arc<Manifest>,
    pub component: Arc<[u8]>,
}
struct Entry {
    generation: u64,
    manifest: Arc<Manifest>,
    component: Arc<[u8]>,
}

pub struct PluginRegistry {
    root: File,
    entries: HashMap<String, Entry>,
    next_generation: u64,
}
impl PluginRegistry {
    /// The application chooses this trusted local directory; guests/agents may
    /// only select an installed ID. The caller must keep it outside every app
    /// command's writable locations, including symlink/hardlink aliases.
    pub fn open(root: &Path) -> Result<Self, RegistryError> {
        if !root.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "registry root must be absolute",
            )
            .into());
        }
        let root = open_root(root)?;
        Ok(Self {
            root,
            entries: HashMap::new(),
            next_generation: 0,
        })
    }
    /// A pinned directory handle for excluding the authority store from file
    /// operations. The handle remains anchored even if its pathname changes.
    pub fn root_descriptor(&self) -> io::Result<File> {
        self.root.try_clone()
    }
    /// Failure revokes any previous entry as well: an invalid replacement can
    /// never leave a stale permission silently active.
    pub fn reload(&mut self, id: &str) -> Result<u64, RegistryError> {
        if !valid_id(id) {
            return Err(RegistryError::InvalidId);
        }
        self.entries.remove(id);
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(RegistryError::GenerationExhausted)?;
        let generation = self.next_generation;
        let bundle = open_child(&self.root, Path::new(id), true)?;
        let manifest_file = open_child(&bundle, Path::new("manifest.json"), false)?;
        let manifest_bytes = read_bounded(manifest_file, MAX_MANIFEST_BYTES)?;
        let manifest = Manifest::parse(&manifest_bytes)?;
        if manifest.id != id {
            return Err(RegistryError::IdMismatch);
        }
        let component_file = open_child(&bundle, Path::new(&manifest.component), false)?;
        let component = read_bounded(component_file, MAX_COMPONENT_BYTES)?;
        let digest = component_digest(&component);
        if digest != manifest.component_sha256 {
            return Err(RegistryError::DigestMismatch);
        }
        self.entries.insert(
            id.into(),
            Entry {
                generation,
                manifest: Arc::new(manifest),
                component: Arc::from(component),
            },
        );
        Ok(generation)
    }
    pub fn revoke(&mut self, id: &str) {
        self.entries.remove(id);
    }
    /// Installed metadata, sorted by stable registry ID. Component bytes are not exposed by discovery.
    pub fn list(&self) -> Vec<(u64, Arc<Manifest>)> {
        let mut entries: Vec<_> = self
            .entries
            .values()
            .map(|entry| (entry.generation, entry.manifest.clone()))
            .collect();
        entries.sort_by(|a, b| a.1.id.cmp(&b.1.id));
        entries
    }
    pub fn bind(
        &self,
        id: &str,
        origin: Origin,
        source: bool,
        vault_id: Uuid,
        vault_generation: u64,
    ) -> Result<PluginBinding, RegistryError> {
        let entry = self.entries.get(id).ok_or(RegistryError::NotRegistered)?;
        if !entry.manifest.invocation.allows(origin, source)
            || (source && entry.manifest.kind != Kind::Interpreter)
            || (!source && entry.manifest.kind != Kind::Compiled)
        {
            return Err(RegistryError::Denied);
        }
        Ok(PluginBinding {
            id: id.into(),
            generation: entry.generation,
            vault_id,
            vault_generation,
            origin,
            manifest: entry.manifest.clone(),
            component: entry.component.clone(),
        })
    }
    /// Called at every owner dispatch, not just when the run is started.
    pub fn is_current(
        &self,
        binding: &PluginBinding,
        vault_id: Uuid,
        vault_generation: u64,
    ) -> bool {
        binding.vault_id == vault_id
            && binding.vault_generation == vault_generation
            && self
                .entries
                .get(&binding.id)
                .is_some_and(|e| e.generation == binding.generation)
    }
}
fn component_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        // Writing hexadecimal into a String is infallible.
        let _ = write!(digest, "{byte:02x}");
    }
    digest
}
fn read_bounded(file: File, limit: u64) -> io::Result<Vec<u8>> {
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bundle member is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bundle member too large",
        ));
    }
    Ok(bytes)
}

// Each path component is opened relative to an already-open directory with
// NOFOLLOW. The verified bytes are read from the same descriptor and retained.
#[cfg(target_os = "linux")]
fn open_root(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}
#[cfg(not(target_os = "linux"))]
fn open_root(_: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "secure plugin registry unavailable on this platform",
    ))
}

#[cfg(target_os = "linux")]
fn open_child(base: &File, path: &Path, directory: bool) -> io::Result<File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;
    let parts: Vec<_> = path.components().collect();
    if parts.is_empty() || parts.iter().any(|p| !matches!(p, Component::Normal(_))) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid bundle path",
        ));
    }
    let mut current = base.try_clone()?;
    for (index, part) in parts.iter().enumerate() {
        let Component::Normal(part) = part else {
            unreachable!()
        };
        let name = CString::new(part.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid bundle name"))?;
        let is_dir = directory || index + 1 < parts.len();
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if is_dir { libc::O_DIRECTORY } else { 0 };
        // SAFETY: name is NUL-terminated, the directory descriptor is live,
        // and the returned descriptor is owned exactly once by File.
        let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        current = unsafe { File::from_raw_fd(fd) };
    }
    Ok(current)
}
#[cfg(not(target_os = "linux"))]
fn open_child(_: &File, _: &Path, _: bool) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "secure plugin registry unavailable on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("tanglewood-plugin-registry-{}", Uuid::new_v4()));
            fs::create_dir_all(path.join("example")).unwrap();
            Self(path)
        }
        fn install(&self, component: &[u8], manifest: serde_json::Value) {
            fs::write(self.0.join("example/component.wasm"), component).unwrap();
            fs::write(
                self.0.join("example/manifest.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn manifest(component: &[u8]) -> serde_json::Value {
        serde_json::json!({
            "manifest_version": 1, "id": "example", "version": "1", "kind": "compiled",
            "wit_version": "0.2.0", "component": "component.wasm",
            "component_sha256": component_digest(component),
            "invocation": {"local": true, "automation": true},
            "permissions": {"rules": [{"action": "note.read", "notes": {"mode": "all"}}]}
        })
    }
    #[test]
    fn reload_and_revoke_invalidate_bound_runs_even_on_bad_replacement() {
        let dir = TestRoot::new();
        let bytes = b"component one";
        dir.install(bytes, manifest(bytes));
        let mut registry = PluginRegistry::open(&dir.0).unwrap();
        registry.reload("example").unwrap();
        let vault = Uuid::new_v4();
        let first = registry
            .bind("example", Origin::Local, false, vault, 7)
            .unwrap();
        assert!(registry.is_current(&first, vault, 7));
        assert!(!registry.is_current(&first, vault, 8));
        registry.reload("example").unwrap();
        assert!(!registry.is_current(&first, vault, 7));
        let second = registry
            .bind("example", Origin::AutomationReadOnly, false, vault, 7)
            .unwrap();
        fs::write(dir.0.join("example/component.wasm"), b"substituted").unwrap();
        assert!(matches!(
            registry.reload("example"),
            Err(RegistryError::DigestMismatch)
        ));
        assert!(!registry.is_current(&second, vault, 7));
        dir.install(bytes, manifest(bytes));
        registry.reload("example").unwrap();
        let third = registry
            .bind("example", Origin::Local, false, vault, 7)
            .unwrap();
        registry.revoke("example");
        assert!(!registry.is_current(&third, vault, 7));
    }
    #[test]
    fn manifests_reject_unknown_grants_and_bundle_escapes() {
        let dir = TestRoot::new();
        let bytes = b"component";
        let mut bad = manifest(bytes);
        bad["permissions"]["rules"][0]["action"] = "note.future".into();
        dir.install(bytes, bad);
        let mut registry = PluginRegistry::open(&dir.0).unwrap();
        assert!(matches!(
            registry.reload("example"),
            Err(RegistryError::Manifest(_))
        ));
        let mut bad = manifest(bytes);
        bad["delegation"] = serde_json::json!({});
        dir.install(bytes, bad);
        assert!(matches!(
            registry.reload("example"),
            Err(RegistryError::Manifest(_))
        ));
        let mut bad = manifest(bytes);
        bad["component"] = "../outside.wasm".into();
        dir.install(bytes, bad);
        assert!(matches!(
            registry.reload("example"),
            Err(RegistryError::Manifest(_))
        ));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn symlink_component_is_not_followed() {
        use std::os::unix::fs::symlink;
        let dir = TestRoot::new();
        let bytes = b"component";
        dir.install(bytes, manifest(bytes));
        let path = dir.0.join("example/component.wasm");
        fs::remove_file(&path).unwrap();
        fs::write(dir.0.join("outside.wasm"), bytes).unwrap();
        symlink("../outside.wasm", path).unwrap();
        let mut registry = PluginRegistry::open(&dir.0).unwrap();
        assert!(matches!(
            registry.reload("example"),
            Err(RegistryError::Io(_))
        ));
    }
}
