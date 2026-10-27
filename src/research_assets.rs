//! Bounded, cancellable local artifact retention. This worker never writes workspace records.
use crate::research::{ArtifactInput, ArtifactKind};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const MAX_ASSET_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_MARKDOWN_BYTES: usize = 8 * 1024 * 1024;
const CAPACITY: usize = 8;
type Notifier = Arc<dyn Fn() + Send + Sync>;
type Pending = Arc<Mutex<HashMap<Uuid, Arc<AtomicBool>>>>;

pub struct AssetJob {
    pub request_id: Uuid,
    pub input: ArtifactInput,
    pub extract_text: bool,
    pub ocr: bool,
    /// Trusted owner context; never supplied by a guest.
    #[cfg(feature = "wasm-extensions")]
    pub guard: Option<AssetGuard>,
}

/// Every complete guard must independently authorize each source and redirect.
/// `revoked` is owner-controlled and independent of the guest lifetime.
#[cfg(feature = "wasm-extensions")]
#[derive(Clone, Debug)]
pub struct AssetGuard {
    pub guards: Vec<crate::plugin_permissions::Guard>,
    pub protected_registry: Arc<File>,
    pub revoked: Arc<AtomicBool>,
}

#[cfg(feature = "wasm-extensions")]
type GuardRef<'a> = Option<&'a AssetGuard>;
#[cfg(not(feature = "wasm-extensions"))]
type GuardRef<'a> = Option<&'a ()>;

#[cfg(feature = "wasm-extensions")]
impl AssetGuard {
    fn check(&self, input: &ArtifactInput) -> Result<(), String> {
        if self.revoked.load(Ordering::Acquire) || self.guards.is_empty() {
            return Err("artifact authority revoked or missing".into());
        }
        for guard in &self.guards {
            crate::plugin_permissions::authorize_saved_input(guard, input)
                .map_err(|_| "artifact source denied by authority".to_owned())?;
        }
        Ok(())
    }

    fn open_local(&self, path: &Path) -> Result<File, String> {
        self.check(&ArtifactInput::LocalFile {
            path: path.to_string_lossy().into_owned(),
        })?;
        let mut pinned: Option<File> = None;
        for guard in &self.guards {
            let file = crate::plugin_io::open_read(path, guard, &self.protected_registry)
                .map_err(|error| format!("artifact source denied: {error}"))?;
            if let Some(first) = &pinned {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    let old = first.metadata().map_err(|e| e.to_string())?;
                    let new = file.metadata().map_err(|e| e.to_string())?;
                    if (old.dev(), old.ino()) != (new.dev(), new.ino()) {
                        return Err("artifact source changed during authorization".into());
                    }
                }
                #[cfg(not(unix))]
                return Err("guarded artifact reads unavailable on this platform".into());
            } else {
                pinned = Some(file);
            }
        }
        self.check(&ArtifactInput::LocalFile {
            path: path.to_string_lossy().into_owned(),
        })?;
        pinned.ok_or_else(|| "artifact authority missing".into())
    }
}

fn guard_active(guard: GuardRef<'_>) -> Result<(), String> {
    #[cfg(feature = "wasm-extensions")]
    if guard.is_some_and(|guard| guard.revoked.load(Ordering::Acquire) || guard.guards.is_empty()) {
        return Err("artifact authority revoked or missing".into());
    }
    #[cfg(not(feature = "wasm-extensions"))]
    let _ = guard;
    Ok(())
}

fn guard_input(guard: GuardRef<'_>, input: &ArtifactInput) -> Result<(), String> {
    #[cfg(feature = "wasm-extensions")]
    if let Some(guard) = guard {
        guard.check(input)?;
    }
    #[cfg(not(feature = "wasm-extensions"))]
    let _ = (guard, input);
    Ok(())
}

#[derive(Debug)]
pub struct RetainedAsset {
    pub kind: ArtifactKind,
    pub url: Option<String>,
    pub final_url: Option<String>,
    pub managed_path: Option<String>,
    pub markdown: Option<String>,
    pub captured_at: u64,
    pub pages: Option<Vec<crate::research::SourcePage>>,
    pub extraction_error: Option<String>,
}
pub struct AssetCompletion {
    pub request_id: Uuid,
    pub result: Result<RetainedAsset, String>,
}
pub struct ResearchAssetWorker {
    jobs: SyncSender<(AssetJob, Arc<AtomicBool>)>,
    completed: Receiver<AssetCompletion>,
    pending: Pending,
    stopped: Arc<AtomicBool>,
}
impl ResearchAssetWorker {
    /// Starts the worker without accessing the filesystem on the caller's thread.
    pub fn open(vault: &Path) -> Result<Self, String> {
        Self::open_with_notifier(vault, None)
    }
    pub fn open_with_notifier(vault: &Path, notifier: Option<Notifier>) -> Result<Self, String> {
        let vault = vault.to_owned();
        let (jobs, receive) = mpsc::sync_channel::<(AssetJob, Arc<AtomicBool>)>(CAPACITY);
        let (send, completed) = mpsc::sync_channel(CAPACITY);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        thread::Builder::new()
            .name("research-assets".into())
            .spawn(move || {
                while let Ok((job, cancel)) = receive.recv() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    #[cfg(feature = "wasm-extensions")]
                    let guard = job.guard.as_ref();
                    #[cfg(not(feature = "wasm-extensions"))]
                    let guard: GuardRef<'_> = None;
                    #[cfg(feature = "wasm-extensions")]
                    let monitor = guard.map(|guard| {
                        let revoked = guard.revoked.clone();
                        let cancel = cancel.clone();
                        let (stop, receive) = mpsc::sync_channel::<()>(1);
                        let thread = thread::Builder::new()
                            .name("research-authority".into())
                            .spawn(move || {
                                loop {
                                    if revoked.load(Ordering::Acquire) {
                                        cancel.store(true, Ordering::Release);
                                        break;
                                    }
                                    if !matches!(
                                        receive.recv_timeout(Duration::from_millis(10)),
                                        Err(mpsc::RecvTimeoutError::Timeout)
                                    ) {
                                        break;
                                    }
                                }
                            });
                        (stop, thread)
                    });
                    #[cfg(feature = "wasm-extensions")]
                    let monitor_failed =
                        monitor.as_ref().is_some_and(|(_, thread)| thread.is_err());
                    #[cfg(not(feature = "wasm-extensions"))]
                    let monitor_failed = false;
                    let result = if monitor_failed {
                        Err("could not monitor artifact authority".into())
                    } else {
                        retain_guarded(&vault, job.input, job.extract_text, job.ocr, &cancel, guard)
                    };
                    #[cfg(feature = "wasm-extensions")]
                    if let Some((stop, Ok(thread))) = monitor {
                        let _ = stop.send(());
                        let _ = thread.join();
                    }
                    if send
                        .try_send(AssetCompletion {
                            request_id: job.request_id,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    if let Some(notify) = &notifier {
                        notify();
                    }
                }
            })
            .map_err(|e| format!("start research asset worker: {e}"))?;
        Ok(Self {
            jobs,
            completed,
            pending,
            stopped,
        })
    }
    pub fn try_submit(&self, job: AssetJob) -> Result<(), String> {
        validate_input(&job.input)?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "asset worker unavailable")?;
        if pending.len() >= CAPACITY {
            return Err("asset worker busy; collect completed requests first".into());
        }
        if pending.contains_key(&job.request_id) {
            return Err("duplicate asset request ID".into());
        }
        let request_id = job.request_id;
        let cancel = Arc::new(AtomicBool::new(false));
        pending.insert(request_id, cancel.clone());
        if let Err(error) = self.jobs.try_send((job, cancel)) {
            pending.remove(&request_id);
            return Err(format!("asset worker unavailable or busy: {error}"));
        }
        Ok(())
    }
    pub fn try_recv(&self) -> Option<AssetCompletion> {
        let completion = self.completed.try_recv().ok()?;
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&completion.request_id);
        }
        Some(completion)
    }
    pub fn cancel(&self, request_id: Uuid) {
        if let Ok(pending) = self.pending.lock()
            && let Some(cancel) = pending.get(&request_id)
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Drop for ResearchAssetWorker {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Ok(pending) = self.pending.lock() {
            for cancel in pending.values() {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        // Deliberately no join: subprocess cancellation and cleanup happen off the UI thread.
    }
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("artifact capture cancelled".into())
    } else {
        Ok(())
    }
}
fn validate_url(url: &str) -> Result<(), String> {
    if crate::research::valid_artifact_url(url) {
        Ok(())
    } else {
        Err("artifact URL must be an absolute HTTP(S) URL without credentials".into())
    }
}
fn validate_input(input: &ArtifactInput) -> Result<(), String> {
    match input {
        ArtifactInput::Generated { .. } => Ok(()),
        ArtifactInput::Url { url, .. } => validate_url(url),
        ArtifactInput::Markdown { markdown, url } => {
            if markdown.len() > MAX_MARKDOWN_BYTES || markdown.trim().is_empty() {
                return Err("Markdown must be nonempty and at most 8 MiB".into());
            }
            if let Some(url) = url {
                validate_url(url)?;
            }
            Ok(())
        }
        ArtifactInput::LocalFile { path } => {
            if path.is_empty() || path.len() > 16384 {
                Err("invalid local artifact path".into())
            } else {
                Ok(())
            }
        }
    }
}

// Linux and Android directory descriptors anchor every asset path, including across directory
// renames. O_NOFOLLOW rejects a substituted research-assets symlink. No caller path is appended.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn directory(path: &Path) -> Result<File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(0x10000 | 0x20000)
        .open(path)
        .map_err(|e| format!("open asset directory: {e}"))
}
#[cfg(any(target_os = "linux", target_os = "android"))]
fn anchored(file: &File) -> PathBuf {
    use std::os::fd::AsRawFd;
    PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
}
struct AssetDirectory {
    _vault: File,
    assets: File,
}
impl AssetDirectory {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn open(vault: &Path) -> Result<Self, String> {
        let vault = directory(vault)?;
        let path = anchored(&vault).join("research-assets");
        match fs::create_dir(&path) {
            Ok(()) => vault
                .sync_all()
                .map_err(|e| format!("sync vault asset entry: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
        let assets = directory(&path)?;
        Ok(Self {
            _vault: vault,
            assets,
        })
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    fn open(_vault: &Path) -> Result<Self, String> {
        Err("artifact retention unavailable on this platform".into())
    }
    fn path(&self) -> PathBuf {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            anchored(&self.assets)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            PathBuf::new()
        }
    }
}
struct Scratch(PathBuf);
impl Scratch {
    fn new(assets: &AssetDirectory) -> Result<Self, String> {
        let path = assets.path().join(format!(".capture-{}", Uuid::new_v4()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .map_err(|e| e.to_string())?;
        }
        #[cfg(not(unix))]
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn create(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.write(true).read(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|e| format!("create capture file: {e}"))
}
fn publish(
    assets: &AssetDirectory,
    input: &Path,
    extension: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    check_cancel(cancel)?;
    let name = format!("{}.{}", Uuid::new_v4(), extension);
    // A hard link publishes complete bytes atomically and never replaces an existing artifact.
    fs::hard_link(input, assets.path().join(&name))
        .map_err(|e| format!("publish artifact: {e}"))?;
    assets
        .assets
        .sync_all()
        .map_err(|e| format!("sync artifact directory: {e}"))?;
    Ok(format!("research-assets/{name}"))
}
#[cfg(test)]
fn retain(
    vault: &Path,
    input: ArtifactInput,
    extract_text: bool,
    ocr: bool,
    cancel: &AtomicBool,
) -> Result<RetainedAsset, String> {
    retain_guarded(vault, input, extract_text, ocr, cancel, None)
}

fn retain_guarded(
    vault: &Path,
    input: ArtifactInput,
    extract_text: bool,
    ocr: bool,
    cancel: &AtomicBool,
    guard: GuardRef<'_>,
) -> Result<RetainedAsset, String> {
    check_cancel(cancel)?;
    validate_input(&input)?;
    guard_input(guard, &input)?;
    if ocr && !extract_text {
        return Err("OCR requires source text extraction".into());
    }
    let captured_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let mut result = RetainedAsset {
        kind: ArtifactKind::File,
        url: None,
        final_url: None,
        managed_path: None,
        markdown: None,
        captured_at,
        pages: None,
        extraction_error: None,
    };
    if extract_text
        && matches!(
            &input,
            ArtifactInput::Generated { .. }
                | ArtifactInput::Markdown { .. }
                | ArtifactInput::Url { archive: false, .. }
        )
    {
        return Err("paper save requires a PDF file or archived PDF URL".into());
    }
    if let ArtifactInput::Url {
        url,
        archive: false,
    } = input
    {
        result.kind = ArtifactKind::Web;
        result.url = Some(url);
        return Ok(result);
    }
    let assets = AssetDirectory::open(vault)?;
    let scratch = Scratch::new(&assets)?;
    let output = scratch.0.join("asset");
    let extension;
    match input {
        ArtifactInput::Generated { spec } => {
            let rendered = crate::research_desktop::render_output(&spec)?;
            if rendered.content.len() > MAX_MARKDOWN_BYTES {
                return Err("generated output exceeds 8 MiB".into());
            }
            let mut file = create(&output)?;
            file.write_all(rendered.content.as_bytes())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            result.kind = if rendered.extension == "svg" {
                ArtifactKind::Image
            } else {
                ArtifactKind::File
            };
            extension = rendered.extension;
        }
        ArtifactInput::LocalFile { path } => {
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(any(target_os = "linux", target_os = "android"))]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(0x800);
            }
            #[cfg(feature = "wasm-extensions")]
            let mut source = if let Some(guard) = guard {
                guard.open_local(Path::new(&path))?
            } else {
                options
                    .open(&path)
                    .map_err(|e| format!("read original artifact: {e}"))?
            };
            #[cfg(not(feature = "wasm-extensions"))]
            let mut source = options
                .open(&path)
                .map_err(|e| format!("read original artifact: {e}"))?;
            let metadata = source.metadata().map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() > MAX_ASSET_BYTES {
                return Err("original must be a regular file of at most 32 MiB".into());
            }
            let mut target = create(&output)?;
            let mut count = 0u64;
            let mut buffer = [0u8; 65536];
            loop {
                check_cancel(cancel)?;
                let size = source.read(&mut buffer).map_err(|e| e.to_string())?;
                if size == 0 {
                    break;
                }
                count += size as u64;
                if count > MAX_ASSET_BYTES {
                    return Err("original exceeds 32 MiB".into());
                }
                target
                    .write_all(&buffer[..size])
                    .map_err(|e| e.to_string())?;
            }
            target.sync_all().map_err(|e| e.to_string())?;
            let original_ext = Path::new(&path)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            extension = if !original_ext.is_empty()
                && original_ext.len() <= 16
                && original_ext.bytes().all(|c| c.is_ascii_alphanumeric())
            {
                original_ext
            } else {
                "bin".into()
            };
            result.kind = match extension.as_str() {
                "pdf" => ArtifactKind::Pdf,
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" => {
                    ArtifactKind::Image
                }
                _ => ArtifactKind::File,
            };
        }
        ArtifactInput::Markdown { markdown, url } => {
            let mut target = create(&output)?;
            target
                .write_all(markdown.as_bytes())
                .map_err(|e| e.to_string())?;
            target.sync_all().map_err(|e| e.to_string())?;
            extension = "md".into();
            result.kind = ArtifactKind::Markdown;
            result.url = url;
            result.markdown = Some(markdown);
        }
        ArtifactInput::Url { url, archive: true } => {
            let html = scratch.0.join("html");
            let provenance = scratch.0.join("provenance");
            let fetch_url = if extract_text {
                resolve_pdf_url(&url, "").unwrap_or_else(|| url.clone())
            } else {
                url.clone()
            };
            let mut final_url = download_url(&fetch_url, &html, &provenance, cancel, guard)?;
            if extract_text {
                if require_pdf(&html).is_err() {
                    let landing =
                        fs::read_to_string(&html).map_err(|_| "landing page is not UTF-8 HTML")?;
                    let pdf_url = resolve_pdf_url(&final_url, &landing).ok_or("No accessible PDF link found in this landing page; provide a direct PDF URL")?;
                    final_url = download_url(&pdf_url, &html, &provenance, cancel, guard)?;
                }
                require_pdf(&html)?;
                fs::rename(&html, &output).map_err(|e| e.to_string())?;
                File::open(&output)
                    .and_then(|file| file.sync_all())
                    .map_err(|e| e.to_string())?;
                result.kind = ArtifactKind::Pdf;
                result.url = Some(url);
                result.final_url = Some(final_url);
                extension = "pdf".into();
            } else {
                let input_file = File::open(&html).map_err(|e| e.to_string())?;
                convert_html(input_file, &output, &scratch.0, cancel)?;
                let markdown = fs::read_to_string(&output)
                    .map_err(|e| format!("web copy is not UTF-8: {e}"))?;
                if markdown.trim().is_empty() {
                    return Err("web conversion produced empty Markdown".into());
                }
                result.kind = ArtifactKind::Web;
                result.url = Some(url);
                result.final_url = Some(final_url);
                result.markdown = Some(markdown);
                extension = "md".into();
            }
        }
        ArtifactInput::Url { archive: false, .. } => unreachable!(),
    }
    if extract_text && !ocr && matches!(extension.as_str(), "txt" | "md" | "markdown" | "text") {
        let bytes = fs::read(&output).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_MARKDOWN_BYTES {
            return Err("source text exceeds 8 MiB".into());
        }
        let text = String::from_utf8(bytes).map_err(|_| "Source is not valid UTF-8 text")?;
        if text.trim().is_empty() {
            return Err("Source contains no usable text".into());
        }
        result.kind = if matches!(extension.as_str(), "md" | "markdown") {
            ArtifactKind::Markdown
        } else {
            ArtifactKind::File
        };
        result.pages = Some(vec![crate::research::SourcePage { number: None, text }]);
    } else if extract_text {
        require_pdf(&output)?;
        result.kind = ArtifactKind::Pdf;
    }
    let extension = if extract_text && result.kind == ArtifactKind::Pdf {
        "pdf"
    } else {
        &extension
    };
    if extract_text && result.kind == ArtifactKind::Pdf {
        match if ocr {
            crate::research_import::ocr_retained_pdf(&output, cancel)
        } else {
            crate::research_import::extract_retained_pdf(&output, cancel)
        } {
            Ok(pages) => result.pages = Some(pages),
            Err(error) => result.extraction_error = Some(error.to_string()),
        }
        check_cancel(cancel)?;
    }
    // Keep bytes in scratch until extraction and cancellation checks finish. An ordinary
    // extraction error still publishes the original; a cancelled job publishes nothing.
    guard_active(guard)?;
    result.managed_path = Some(publish(&assets, &output, extension, cancel)?);
    Ok(result)
}
fn require_pdf(path: &Path) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut signature = [0; 5];
    file.read_exact(&mut signature)
        .map_err(|_| "downloaded content is not a PDF".to_owned())?;
    if &signature != b"%PDF-" {
        return Err("downloaded content is not a PDF; use a direct full-text PDF URL".into());
    }
    Ok(())
}
fn convert_html(
    input: File,
    output: &Path,
    scratch: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let diagnostics = scratch.join("pandoc-errors");
    let mut pandoc = Command::new("/usr/bin/pandoc");
    pandoc
        .args(["--from=html", "--to=gfm", "--wrap=none", "--sandbox"])
        .stdin(input)
        .stdout(create(output)?)
        .stderr(create(&diagnostics)?);
    run(
        &mut pandoc,
        &[(output, MAX_MARKDOWN_BYTES as u64), (&diagnostics, 65536)],
        cancel,
    )?;
    File::open(output)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}
fn run(command: &mut Command, outputs: &[(&Path, u64)], cancel: &AtomicBool) -> Result<(), String> {
    let mut child = command
        .spawn()
        .map_err(|e| format!("optional archive tool unavailable: {e}"))?;
    let start = Instant::now();
    loop {
        let error = if cancel.load(Ordering::Relaxed) {
            Some("artifact capture cancelled".to_owned())
        } else if start.elapsed() > Duration::from_secs(20) {
            Some("archive tool timed out".to_owned())
        } else if outputs
            .iter()
            .any(|(path, limit)| fs::metadata(path).is_ok_and(|m| m.len() > *limit))
        {
            Some("archive tool exceeded output limit".to_owned())
        } else {
            None
        };
        if let Some(error) = error {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return if outputs
                    .iter()
                    .any(|(path, limit)| fs::metadata(path).is_ok_and(|m| m.len() > *limit))
                {
                    Err("archive tool exceeded output limit".into())
                } else if status.success() {
                    Ok(())
                } else {
                    Err(format!("archive tool failed ({status})"))
                };
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("wait for archive tool: {error}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Vault(PathBuf);
    impl Vault {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("tanglewood-assets-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Vault {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn paper_fixture() -> String {
        let stream = "BT /F1 12 Tf 72 720 Td (Paper evidence) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        ];
        let mut pdf = "%PDF-1.4\n".to_owned();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let xref = pdf.len();
        pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        ));
        pdf
    }
    #[test]
    fn retained_text_preserves_original_and_exposes_source_page() {
        let vault = Vault::new();
        let path = vault.0.join("notes.md");
        let text = "# Evidence\r\nαβγ\n";
        fs::write(&path, text).unwrap();
        let retained = retain(
            &vault.0,
            ArtifactInput::LocalFile {
                path: path.to_string_lossy().into(),
            },
            true,
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(retained.kind, ArtifactKind::Markdown);
        assert_eq!(retained.pages.unwrap()[0].text, text);
        assert_eq!(
            fs::read(vault.0.join(retained.managed_path.unwrap())).unwrap(),
            text.as_bytes()
        );
    }
    #[test]
    fn resolves_landing_metadata_and_arxiv_without_guessing_links() {
        assert_eq!(
            resolve_pdf_url(
                "https://example.org/papers/one",
                "<META content='../files/a.pdf?x=1&amp;y=2' NAME='citation_pdf_url'>"
            )
            .as_deref(),
            Some("https://example.org/files/a.pdf?x=1&y=2")
        );
        assert_eq!(
            resolve_pdf_url("https://arxiv.org/abs/2401.12345v2", "").as_deref(),
            Some("https://arxiv.org/pdf/2401.12345v2")
        );
        assert!(
            resolve_pdf_url(
                "https://example.org",
                "<meta name='citation_pdf_url' content='file:///tmp/a.pdf'>"
            )
            .is_none()
        );
        assert!(
            resolve_pdf_url("https://example.org", "<a href='unrelated.pdf'>PDF</a>").is_none()
        );
    }
    #[test]
    fn landing_download_retains_pdf_and_final_provenance() {
        use std::net::TcpListener;
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        server.set_nonblocking(true).unwrap();
        let base = format!("http://{}", server.local_addr().unwrap());
        let pdf = paper_fixture();
        let expected = pdf.clone();
        let server_thread = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut handled = 0;
            while handled < 2 && Instant::now() < deadline {
                if let Ok((mut stream, _)) = server.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut buffer = [0; 4096];
                    let size = stream.read(&mut buffer).unwrap();
                    let request = String::from_utf8_lossy(&buffer[..size]);
                    let body = if request.starts_with("GET /paper.pdf ") {
                        pdf.as_str()
                    } else {
                        "<meta name=\"citation_pdf_url\" content=\"/paper.pdf\">"
                    };
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                    handled += 1;
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });
        let vault = Vault::new();
        let retained = retain(
            &vault.0,
            ArtifactInput::Url {
                url: format!("{base}/landing"),
                archive: true,
            },
            true,
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        server_thread.join().unwrap();
        assert_eq!(
            retained.final_url.as_deref(),
            Some(format!("{base}/paper.pdf").as_str())
        );
        assert_eq!(
            fs::read(vault.0.join(retained.managed_path.unwrap())).unwrap(),
            expected.as_bytes()
        );
    }
    #[test]
    fn explicit_ocr_rasterizes_original_and_retains_page_identity() {
        if !Path::new("/usr/bin/tesseract").is_file() || !Path::new("/usr/bin/pdftoppm").is_file() {
            return;
        }
        let vault = Vault::new();
        let path = vault.0.join("ocr.pdf");
        let pdf = paper_fixture();
        fs::write(&path, &pdf).unwrap();
        let pages =
            crate::research_import::ocr_retained_pdf(&path, &AtomicBool::new(false)).unwrap();
        assert_eq!(pages[0].number, Some(1));
        assert!(pages[0].text.contains("Paper evidence"));
        assert_eq!(fs::read(path).unwrap(), pdf.as_bytes());
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn retained_paper_extracts_numbered_text_without_changing_original() {
        if !Path::new("/usr/bin/pdftotext").is_file() {
            return;
        }
        let pdf = paper_fixture();
        let vault = Vault::new();
        // Content recognition, rather than a supplied extension, determines PDF storage.
        let original = vault.0.join("download.bin");
        fs::write(&original, &pdf).unwrap();
        let retained = retain(
            &vault.0,
            ArtifactInput::LocalFile {
                path: original.to_string_lossy().into_owned(),
            },
            true,
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(retained.extraction_error.is_none());
        let pages = retained.pages.unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].number, Some(1));
        assert!(pages[0].text.contains("Paper evidence"));
        let path = retained.managed_path.unwrap();
        assert!(path.ends_with(".pdf"));
        assert_eq!(fs::read(vault.0.join(path)).unwrap(), pdf.as_bytes());
        assert_eq!(fs::read(original).unwrap(), pdf.as_bytes());
    }
    #[test]
    fn paper_retains_failed_pdf_but_rejects_html_disguised_as_pdf() {
        let vault = Vault::new();
        let original = vault.0.join("paper.pdf");
        let bytes = b"%PDF-1.7\ninvalid PDF body";
        fs::write(&original, bytes).unwrap();
        let retained = retain(
            &vault.0,
            ArtifactInput::LocalFile {
                path: original.to_string_lossy().into_owned(),
            },
            true,
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(retained.kind, ArtifactKind::Pdf);
        assert!(retained.pages.is_none());
        assert!(retained.extraction_error.is_some());
        assert_eq!(
            fs::read(vault.0.join(retained.managed_path.unwrap())).unwrap(),
            bytes
        );
        fs::write(&original, b"<html>not a PDF</html>").unwrap();
        assert!(
            retain(
                &vault.0,
                ArtifactInput::LocalFile {
                    path: original.to_string_lossy().into_owned(),
                },
                true,
                false,
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
    #[test]
    fn retains_original_bytes_and_supplied_markdown() {
        let vault = Vault::new();
        let original = vault.0.join("original.pdf");
        let bytes = b"%PDF-1.7\0\xffunchanged";
        fs::write(&original, bytes).unwrap();
        let cancel = AtomicBool::new(false);
        let file = retain(
            &vault.0,
            ArtifactInput::LocalFile {
                path: original.to_string_lossy().into_owned(),
            },
            false,
            false,
            &cancel,
        )
        .unwrap();
        assert_eq!(file.kind, ArtifactKind::Pdf);
        assert_eq!(
            fs::read(vault.0.join(file.managed_path.unwrap())).unwrap(),
            bytes
        );
        assert_eq!(fs::read(original).unwrap(), bytes);
        let text = "# Evidence\r\n\r\nOriginal **bytes**.\n";
        let markdown = retain(
            &vault.0,
            ArtifactInput::Markdown {
                markdown: text.into(),
                url: Some("https://example.org/evidence".into()),
            },
            false,
            false,
            &cancel,
        )
        .unwrap();
        assert_eq!(markdown.markdown.as_deref(), Some(text));
        assert_eq!(
            fs::read(vault.0.join(markdown.managed_path.unwrap())).unwrap(),
            text.as_bytes()
        );
        let reference = retain(
            &vault.0,
            ArtifactInput::Url {
                url: "https://example.org".into(),
                archive: false,
            },
            false,
            false,
            &cancel,
        )
        .unwrap();
        assert!(reference.managed_path.is_none());
        assert!(reference.markdown.is_none());
    }
    #[test]
    fn rejects_asset_directory_symlink_and_cancelled_capture() {
        let vault = Vault::new();
        let outside = Vault::new();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside.0, vault.0.join("research-assets")).unwrap();
            assert!(
                retain(
                    &vault.0,
                    ArtifactInput::Markdown {
                        markdown: "evidence".into(),
                        url: None
                    },
                    false,
                    false,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
        }
        assert!(
            retain(
                &vault.0,
                ArtifactInput::Markdown {
                    markdown: "evidence".into(),
                    url: None
                },
                false,
                false,
                &AtomicBool::new(true)
            )
            .is_err()
        );
    }
    #[test]
    fn installed_pandoc_converts_local_html_without_network() {
        if !Path::new("/usr/bin/pandoc").is_file() {
            return;
        }
        let vault = Vault::new();
        let html = vault.0.join("fixture.html");
        fs::write(
            &html,
            "<h1>Local evidence</h1><p>Retained <strong>result</strong>.</p>",
        )
        .unwrap();
        let output = vault.0.join("result.md");
        convert_html(
            File::open(html).unwrap(),
            &output,
            &vault.0,
            &AtomicBool::new(false),
        )
        .unwrap();
        let text = fs::read_to_string(output).unwrap();
        assert!(text.contains("# Local evidence"));
        assert!(text.contains("**result**"));
    }
    #[test]
    fn worker_queue_and_completion_budget_are_bounded() {
        let vault = Vault::new();
        let worker = ResearchAssetWorker::open(&vault.0).unwrap();
        for _ in 0..CAPACITY {
            worker
                .try_submit(AssetJob {
                    request_id: Uuid::new_v4(),
                    extract_text: false,
                    ocr: false,
                    #[cfg(feature = "wasm-extensions")]
                    guard: None,
                    input: ArtifactInput::Url {
                        url: "https://example.org".into(),
                        archive: false,
                    },
                })
                .unwrap();
        }
        assert!(
            worker
                .try_submit(AssetJob {
                    request_id: Uuid::new_v4(),
                    extract_text: false,
                    ocr: false,
                    #[cfg(feature = "wasm-extensions")]
                    guard: None,
                    input: ArtifactInput::Url {
                        url: "https://example.org".into(),
                        archive: false
                    }
                })
                .is_err()
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut count = 0;
        while count < CAPACITY && Instant::now() < deadline {
            if worker.try_recv().is_some() {
                count += 1;
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
        assert_eq!(count, CAPACITY);
    }
}

fn resolve_pdf_url(base: &str, html: &str) -> Option<String> {
    let base = url::Url::parse(base).ok()?;
    if matches!(base.host_str(), Some("arxiv.org" | "www.arxiv.org"))
        && base.path().starts_with("/abs/")
    {
        return Some(format!("https://arxiv.org/pdf/{}", &base.path()[5..]));
    }
    for tag in html.split('<').take(100_000) {
        let tag = tag.split('>').next().unwrap_or_default();
        if !tag.to_ascii_lowercase().starts_with("meta ") {
            continue;
        }
        if html_attribute(tag, "name").is_some_and(|v| v.eq_ignore_ascii_case("citation_pdf_url")) {
            let value = html_attribute(tag, "content")?.replace("&amp;", "&");
            let resolved = base.join(&value).ok()?.to_string();
            return crate::research::valid_artifact_url(&resolved).then_some(resolved);
        }
    }
    None
}
fn html_attribute(tag: &str, key: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let name = &tag[start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i == bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let quote = bytes[i];
        let quoted = quote == b'\'' || quote == b'"';
        if quoted {
            i += 1;
        }
        let start = i;
        while i < bytes.len()
            && if quoted {
                bytes[i] != quote
            } else {
                !bytes[i].is_ascii_whitespace()
            }
        {
            i += 1;
        }
        let value = &tag[start..i];
        if quoted && i < bytes.len() {
            i += 1;
        }
        if name.eq_ignore_ascii_case(key) {
            return Some(value.into());
        }
    }
    None
}
/// Follow redirects ourselves so every destination is authorized before curl
/// sends a request. The same check also covers discovered PDF URLs.
fn download_url(
    start_url: &str,
    output: &Path,
    provenance: &Path,
    cancel: &AtomicBool,
    guard: GuardRef<'_>,
) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut current = start_url.to_owned();
    for redirects in 0..=5 {
        check_cancel(cancel)?;
        validate_url(&current)?;
        guard_input(
            guard,
            &ArtifactInput::Url {
                url: current.clone(),
                archive: true,
            },
        )?;
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("archive tool timed out")?;
        let timeout = format!("{:.3}", remaining.as_secs_f64().max(0.001));
        let mut command = Command::new("/usr/bin/curl");
        command
            .args([
                "--disable",
                "--silent",
                "--show-error",
                "--fail",
                "--max-time",
                &timeout,
                "--connect-timeout",
                "10",
                "--proto",
                "=http,https",
                "--max-filesize",
                "33554432",
                "--write-out",
                "%{stderr}%{http_code}\\n%{redirect_url}",
                "--url",
                &current,
            ])
            .stdin(Stdio::null())
            .stdout(File::create(output).map_err(|e| e.to_string())?)
            .stderr(File::create(provenance).map_err(|e| e.to_string())?);
        run(
            &mut command,
            &[(output, MAX_ASSET_BYTES), (provenance, 65536)],
            cancel,
        )?;
        let status_and_redirect = fs::read_to_string(provenance).map_err(|e| e.to_string())?;
        let (status, location) = status_and_redirect
            .split_once('\n')
            .ok_or("invalid HTTP status")?;
        let status: u16 = status.parse().map_err(|_| "invalid HTTP status")?;
        if (200..300).contains(&status) {
            return Ok(current);
        }
        if !matches!(status, 301 | 302 | 303 | 307 | 308) {
            return Err(format!("unexpected HTTP status {status}"));
        }
        if redirects == 5 {
            return Err("too many HTTP redirects".into());
        }
        if location.is_empty() || location.chars().any(char::is_control) {
            return Err("invalid redirect location".into());
        }
        current = url::Url::parse(&current)
            .map_err(|_| "invalid redirect base")?
            .join(location)
            .map_err(|_| "invalid redirect target")?
            .to_string();
    }
    Err("too many HTTP redirects".into())
}

#[cfg(all(test, feature = "wasm-extensions", target_os = "linux"))]
mod guarded_tests {
    use super::*;
    use crate::plugin_permissions::{Action, Guard, Origin, Rule, Scope};
    use std::net::TcpListener;

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("tanglewood-guarded-assets-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn accept_bounded(listener: &TcpListener) -> Option<std::net::TcpStream> {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match listener.accept() {
                Ok((stream, _)) => return Some(stream),
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(5))
                }
                _ => return None,
            }
        }
    }
    fn rule(action: Action) -> Rule {
        Rule {
            action,
            notes: None,
            workspaces: Some(Scope::All),
            note_paths: None,
            filesystem_roots: None,
            url_origins: None,
            administrative_targets: None,
            input_kinds: None,
            human: None,
        }
    }
    fn context(root: &Temp, rule: Rule) -> AssetGuard {
        let registry = root.0.join("registry");
        fs::create_dir_all(&registry).unwrap();
        AssetGuard {
            guards: vec![Guard {
                rule,
                origin: Origin::Local,
            }],
            protected_registry: Arc::new(File::open(registry).unwrap()),
            revoked: Arc::new(AtomicBool::new(false)),
        }
    }
    #[test]
    fn guarded_file_descriptor_survives_path_replacement() {
        let root = Temp::new();
        let allowed = root.0.join("allowed");
        fs::create_dir(&allowed).unwrap();
        let path = allowed.join("paper.txt");
        fs::write(&path, "authorized").unwrap();
        let mut rule = rule(Action::ResearchSavePaper);
        rule.filesystem_roots = Some(Scope::Only(vec![allowed]));
        let guard = context(&root, rule);
        let mut file = guard.open_local(&path).unwrap();
        fs::rename(&path, path.with_extension("old")).unwrap();
        fs::write(&path, "replacement").unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        assert_eq!(text, "authorized");
    }
    #[test]
    fn all_file_guards_must_allow_the_same_source() {
        let root = Temp::new();
        let allowed = root.0.join("allowed");
        let other = root.0.join("other");
        fs::create_dir(&allowed).unwrap();
        fs::create_dir(&other).unwrap();
        let path = allowed.join("paper.txt");
        fs::write(&path, "source").unwrap();
        let mut first = rule(Action::ResearchSavePaper);
        first.filesystem_roots = Some(Scope::Only(vec![allowed]));
        let mut second = rule(Action::ResearchRetryAcquisition);
        second.filesystem_roots = Some(Scope::Only(vec![other]));
        let mut guard = context(&root, first);
        guard.guards.push(Guard {
            rule: second,
            origin: Origin::Local,
        });
        assert!(guard.open_local(&path).is_err());
    }
    #[test]
    fn revocation_cancels_an_active_download() {
        let root = Temp::new();
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.local_addr().unwrap());
        let (started, receiving) = mpsc::sync_channel(1);
        let (finish, stopped) = mpsc::sync_channel::<()>(1);
        let responder = thread::spawn(move || {
            let Some(mut stream) = accept_bounded(&server) else {
                return;
            };
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 1024\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            started.send(()).unwrap();
            let _ = stopped.recv_timeout(Duration::from_secs(5));
        });
        let mut rule = rule(Action::ResearchSavePaper);
        rule.url_origins = Some(Scope::Only(vec![base.clone()]));
        let guard = context(&root, rule);
        let revoked = guard.revoked.clone();
        let worker = ResearchAssetWorker::open(&root.0).unwrap();
        worker
            .try_submit(AssetJob {
                request_id: Uuid::new_v4(),
                input: ArtifactInput::Url {
                    url: format!("{base}/paper.pdf"),
                    archive: true,
                },
                extract_text: true,
                ocr: false,
                guard: Some(guard),
            })
            .unwrap();
        receiving
            .recv_timeout(Duration::from_secs(5))
            .expect("download started");
        revoked.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(3);
        let completion = loop {
            if let Some(completion) = worker.try_recv() {
                break completion;
            }
            assert!(
                Instant::now() < deadline,
                "revoked download did not stop promptly"
            );
            thread::sleep(Duration::from_millis(10));
        };
        finish.send(()).unwrap();
        responder.join().unwrap();
        assert!(completion.result.is_err());
    }
    #[test]
    fn same_origin_redirect_is_followed_and_retained() {
        let root = Temp::new();
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.local_addr().unwrap());
        let responder = thread::spawn(move || {
            for index in 0..2 {
                let mut stream = accept_bounded(&server).expect("authorized request");
                let mut request = [0; 2048];
                let size = stream.read(&mut request).unwrap();
                if index == 0 {
                    write!(stream, "HTTP/1.1 302 Found\r\nLocation: /paper.pdf\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    assert!(
                        String::from_utf8_lossy(&request[..size]).starts_with("GET /paper.pdf ")
                    );
                    let body = "%PDF-1.4\n";
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                }
            }
        });
        let mut rule = rule(Action::ResearchSavePaper);
        rule.url_origins = Some(Scope::Only(vec![base.clone()]));
        let guard = context(&root, rule);
        let retained = retain_guarded(
            &root.0,
            ArtifactInput::Url {
                url: format!("{base}/start"),
                archive: true,
            },
            true,
            false,
            &AtomicBool::new(false),
            Some(&guard),
        )
        .expect("redirect remains within grant");
        responder.join().unwrap();
        assert_eq!(
            retained.final_url.as_deref(),
            Some(format!("{base}/paper.pdf").as_str())
        );
        assert!(retained.managed_path.is_some());
    }
    #[test]
    fn landing_pdf_origin_is_checked_before_download() {
        let root = Temp::new();
        let allowed = TcpListener::bind("127.0.0.1:0").unwrap();
        let denied = TcpListener::bind("127.0.0.1:0").unwrap();
        denied.set_nonblocking(true).unwrap();
        let allowed_url = format!("http://{}", allowed.local_addr().unwrap());
        let denied_url = format!("http://{}/paper.pdf", denied.local_addr().unwrap());
        let responder = thread::spawn(move || {
            let Some(mut stream) = accept_bounded(&allowed) else {
                return false;
            };
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            let body = format!("<meta name='citation_pdf_url' content='{denied_url}'>");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            true
        });
        let mut rule = rule(Action::ResearchSavePaper);
        rule.url_origins = Some(Scope::Only(vec![allowed_url.clone()]));
        let guard = context(&root, rule);
        let result = retain_guarded(
            &root.0,
            ArtifactInput::Url {
                url: format!("{allowed_url}/landing"),
                archive: true,
            },
            true,
            false,
            &AtomicBool::new(false),
            Some(&guard),
        );
        assert!(
            responder.join().unwrap(),
            "allowed origin was not contacted: {result:?}"
        );
        assert!(matches!(result, Err(message) if message.contains("denied by authority")));
        assert!(
            matches!(denied.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
    #[test]
    fn redirect_to_disallowed_origin_is_rejected_before_contact() {
        let root = Temp::new();
        let allowed = TcpListener::bind("127.0.0.1:0").unwrap();
        let denied = TcpListener::bind("127.0.0.1:0").unwrap();
        denied.set_nonblocking(true).unwrap();
        let allowed_url = format!("http://{}", allowed.local_addr().unwrap());
        let denied_url = format!("http://{}/stolen", denied.local_addr().unwrap());
        let responder = thread::spawn(move || {
            let Some(mut stream) = accept_bounded(&allowed) else {
                return false;
            };
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            write!(stream, "HTTP/1.1 302 Found\r\nLocation: {denied_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            true
        });
        let mut rule = rule(Action::ResearchSavePaper);
        rule.url_origins = Some(Scope::Only(vec![allowed_url.clone()]));
        let guard = context(&root, rule);
        let result = retain_guarded(
            &root.0,
            ArtifactInput::Url {
                url: format!("{allowed_url}/paper"),
                archive: true,
            },
            true,
            false,
            &AtomicBool::new(false),
            Some(&guard),
        );
        assert!(
            responder.join().unwrap(),
            "allowed origin was not contacted: {result:?}"
        );
        assert!(matches!(result, Err(message) if message.contains("denied by authority")));
        assert!(
            matches!(denied.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}
