//! Optional desktop I/O: no workspace writes or UI ownership.
use crate::research_desktop::ReferenceCandidate;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;
const CAPACITY: usize = 4;
pub enum ReaderRequest {
    PickFiles,
    RenderPdf { path: PathBuf, page: u32 },
    Discover { query: String },
}
#[derive(Debug)]
pub enum ReaderResult {
    Files(Vec<PathBuf>),
    PdfPage { page: u32, pages: u32, png: Vec<u8> },
    Discovery(Vec<ReferenceCandidate>),
}
pub struct ReaderCompletion {
    pub request_id: Uuid,
    pub result: Result<ReaderResult, String>,
}
pub struct ResearchReaderWorker {
    jobs: SyncSender<(Uuid, ReaderRequest, Arc<AtomicBool>)>,
    completed: Receiver<ReaderCompletion>,
    pending: std::sync::Mutex<Vec<(Uuid, Arc<AtomicBool>)>>,
}
impl ResearchReaderWorker {
    pub fn new(wake: impl Fn() + Send + 'static) -> Result<Self, String> {
        let (jobs, incoming) =
            mpsc::sync_channel::<(Uuid, ReaderRequest, Arc<AtomicBool>)>(CAPACITY);
        let (send, completed) = mpsc::sync_channel(CAPACITY);
        thread::Builder::new()
            .name("research-reader".into())
            .spawn(move || {
                while let Ok((request_id, request, cancel)) = incoming.recv() {
                    let result = perform(request, &cancel);
                    if send
                        .try_send(ReaderCompletion { request_id, result })
                        .is_err()
                    {
                        break;
                    }
                    wake();
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            jobs,
            completed,
            pending: std::sync::Mutex::new(Vec::new()),
        })
    }
    pub fn try_submit(&self, request: ReaderRequest) -> Result<Uuid, String> {
        if let ReaderRequest::Discover { query } = &request {
            if query.trim().is_empty() || query.len() > 2000 {
                return Err("Enter a research query of at most 2000 bytes".into());
            }
        }
        let mut pending = self.pending.lock().map_err(|_| "reader unavailable")?;
        if pending.len() >= CAPACITY {
            return Err("Reader is busy; wait for current requests".into());
        }
        let id = Uuid::new_v4();
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs
            .try_send((id, request, cancel.clone()))
            .map_err(|_| "Reader unavailable or busy")?;
        pending.push((id, cancel));
        Ok(id)
    }
    pub fn try_recv(&self) -> Option<ReaderCompletion> {
        let value = self.completed.try_recv().ok()?;
        if let Ok(mut pending) = self.pending.lock() {
            pending.retain(|(id, _)| *id != value.request_id);
        }
        Some(value)
    }
    pub fn cancel_all(&self) {
        if let Ok(pending) = self.pending.lock() {
            for (_, cancel) in pending.iter() {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    }
}
impl Drop for ResearchReaderWorker {
    fn drop(&mut self) {
        self.cancel_all();
    }
}
#[derive(Clone, Debug)]
pub struct RuntimeReadiness {
    pub picker: bool,
    pub pdf: bool,
    pub ocr: bool,
    pub discovery: bool,
    pub guidance: String,
}
pub fn runtime_readiness() -> RuntimeReadiness {
    let picker = tool("zenity").is_some() || tool("kdialog").is_some();
    let pdf = tool("pdftoppm").is_some() && tool("pdfinfo").is_some();
    let ocr = pdf && tool("tesseract").is_some();
    let discovery = tool("curl").is_some();
    RuntimeReadiness{picker,pdf,ocr,discovery,guidance:"Optional desktop tools: zenity or kdialog (file picker), Poppler (PDF pages/text), Tesseract with eng language data (OCR), curl and network access (Crossref discovery).".into()}
}
fn tool(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|p| p.join(name))
        .find(|p| p.is_file())
}
fn perform(request: ReaderRequest, cancel: &AtomicBool) -> Result<ReaderResult, String> {
    match request {
        ReaderRequest::PickFiles => pick_files(cancel).map(ReaderResult::Files),
        ReaderRequest::RenderPdf { path, page } => {
            let scratch = Scratch::new()?;
            let snapshot = snapshot_pdf(&path, &scratch.0, cancel)?;
            let count = pdf_page_count(&snapshot, cancel)?;
            if page == 0 || page > count {
                return Err(format!("PDF page must be between 1 and {count}"));
            }
            let png = render_page(&snapshot, page, &scratch.0, 1600, cancel)?;
            Ok(ReaderResult::PdfPage {
                page,
                pages: count,
                png,
            })
        }
        ReaderRequest::Discover { query } => discover(&query, cancel).map(ReaderResult::Discovery),
    }
}
fn pick_files(cancel: &AtomicBool) -> Result<Vec<PathBuf>, String> {
    let mut cmd = if let Some(path) = tool("zenity") {
        let mut c = Command::new(path);
        c.args([
            "--file-selection",
            "--multiple",
            "--separator=\n",
            "--title=Add research sources",
            "--file-filter=Research sources | *.pdf *.md *.txt *.markdown",
        ]);
        c
    } else if let Some(path) = tool("kdialog") {
        let mut c = Command::new(path);
        c.args([
            "--getopenfilename",
            ".",
            "*.pdf *.md *.txt *.markdown",
            "--multiple",
            "--separate-output",
            "--title",
            "Add research sources",
        ]);
        c
    } else {
        return Err(
            "Install zenity or kdialog to select files, or drag files into Tanglewood".into(),
        );
    };
    let (status, bytes) = process_status(&mut cmd, cancel, Duration::from_secs(300), 65536, &[])?;
    if status.code() == Some(1) {
        return Ok(Vec::new());
    }
    if !status.success() {
        return Err("Native file picker failed; try dragging files into Tanglewood".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "File picker returned invalid text")?;
    let files = text
        .lines()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if files.len() > 32 {
        return Err("Select at most 32 files at a time".into());
    }
    Ok(files)
}
fn discover(query: &str, cancel: &AtomicBool) -> Result<Vec<ReferenceCandidate>, String> {
    if query.trim().is_empty() || query.len() > 2000 {
        return Err("Invalid discovery query".into());
    }
    let mut url = url::Url::parse("https://api.crossref.org/works").unwrap();
    url.query_pairs_mut()
        .append_pair("query.bibliographic", query)
        .append_pair("rows", "12");
    let mut cmd = Command::new("curl");
    cmd.args([
        "--disable",
        "--silent",
        "--show-error",
        "--fail",
        "--max-time",
        "20",
        "--connect-timeout",
        "10",
        "--proto",
        "=https",
        "--user-agent",
        "Tanglewood/0.1 research discovery",
        "--url",
        url.as_str(),
    ]);
    let (success, bytes) = process(
        &mut cmd,
        cancel,
        Duration::from_secs(25),
        2 * 1024 * 1024,
        &[],
    )?;
    if !success {
        return Err("Crossref search failed; check network access and retry".into());
    }
    parse_crossref(&bytes)
}
fn parse_crossref(bytes: &[u8]) -> Result<Vec<ReferenceCandidate>, String> {
    let data: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid Crossref response: {e}"))?;
    let items = data
        .pointer("/message/items")
        .and_then(|v| v.as_array())
        .ok_or("Crossref response has no results")?;
    Ok(items
        .iter()
        .take(12)
        .filter_map(|item| {
            let title = item.get("title")?.as_array()?.first()?.as_str()?.to_owned();
            if title.trim().is_empty() || title.len() > 1024 {
                return None;
            }
            let url = item
                .get("URL")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .or_else(|| {
                    item.get("DOI")
                        .and_then(|v| v.as_str())
                        .map(|s| format!("https://doi.org/{s}"))
                })?;
            if url.len() > 4096 || !crate::research::valid_artifact_url(&url) {
                return None;
            }
            let authors = item
                .get("author")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .take(20)
                        .filter_map(|v| v.get("family").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|s| !s.is_empty() && s.len() <= 4096);
            let date = item
                .pointer("/issued/date-parts/0/0")
                .and_then(|v| v.as_u64())
                .map(|v| v.to_string());
            Some(ReferenceCandidate {
                title,
                url,
                author: authors,
                date,
            })
        })
        .collect())
}
pub(crate) struct Scratch(pub PathBuf);
impl Scratch {
    pub(crate) fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("tanglewood-reader-{}", Uuid::new_v4()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(crate) fn snapshot_pdf(
    path: &Path,
    scratch: &Path,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x800);
    }
    let source = options
        .open(path)
        .map_err(|e| format!("Open original PDF: {e}"))?;
    let metadata = source.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err("Select a regular PDF of at most 32 MiB".into());
    }
    let mut bytes = Vec::new();
    source
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if !bytes.starts_with(b"%PDF-") || bytes.len() > 32 * 1024 * 1024 {
        return Err("Original is not a supported PDF".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    let output = scratch.join("original.pdf");
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(output)
}
pub(crate) fn pdf_page_count(path: &Path, cancel: &AtomicBool) -> Result<u32, String> {
    let mut cmd = Command::new("pdfinfo");
    cmd.arg(path);
    let (ok, bytes) = process(&mut cmd, cancel, Duration::from_secs(20), 65536, &[])?;
    if !ok {
        return Err("Cannot read PDF; check encryption or file integrity".into());
    }
    let text = String::from_utf8_lossy(&bytes);
    let pages = text
        .lines()
        .find_map(|l| {
            l.strip_prefix("Pages:")
                .and_then(|s| s.trim().parse::<u32>().ok())
        })
        .ok_or("PDF page count unavailable")?;
    if pages == 0 || pages > 500 {
        return Err("PDF must have between 1 and 500 pages".into());
    }
    Ok(pages)
}
pub(crate) fn render_page(
    path: &Path,
    page: u32,
    scratch: &Path,
    size: u32,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let prefix = scratch.join(format!("page-{page}"));
    let png = prefix.with_extension("png");
    let mut cmd = Command::new("pdftoppm");
    cmd.args([
        "-f",
        &page.to_string(),
        "-l",
        &page.to_string(),
        "-singlefile",
        "-scale-to",
        &size.to_string(),
        "-png",
    ])
    .arg(path)
    .arg(&prefix);
    let (ok, _) = process(
        &mut cmd,
        cancel,
        Duration::from_secs(30),
        65536,
        &[(&png, 16 * 1024 * 1024)],
    )?;
    if !ok {
        return Err("PDF page rendering failed".into());
    }
    let bytes = fs::read(&png).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Invalid or oversized PDF page image".into());
    }
    Ok(bytes)
}
/// Capture bounded stdout without pipe deadlocks; kill/reap on cancellation or timeout.
pub(crate) fn process(
    cmd: &mut Command,
    cancel: &AtomicBool,
    timeout: Duration,
    limit: u64,
    outputs: &[(&Path, u64)],
) -> Result<(bool, Vec<u8>), String> {
    process_status(cmd, cancel, timeout, limit, outputs)
        .map(|(status, bytes)| (status.success(), bytes))
}
fn process_status(
    cmd: &mut Command,
    cancel: &AtomicBool,
    timeout: Duration,
    limit: u64,
    outputs: &[(&Path, u64)],
) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    let scratch = Scratch::new()?;
    let path = scratch.0.join("stdout");
    let out = File::options()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Optional research tool unavailable: {e}"))?;
    let start = Instant::now();
    loop {
        let overflow = || {
            fs::metadata(&path).is_ok_and(|m| m.len() > limit)
                || outputs
                    .iter()
                    .any(|(p, n)| fs::metadata(p).is_ok_and(|m| m.len() > *n))
        };
        let fail = if cancel.load(Ordering::Relaxed) {
            Some("Cancelled")
        } else if start.elapsed() > timeout {
            Some("Research tool timed out")
        } else if overflow() {
            Some("Research tool exceeded output limit")
        } else {
            None
        };
        if let Some(error) = fail {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if overflow() {
                    return Err("Research tool exceeded output limit".into());
                }
                return Ok((status, fs::read(&path).map_err(|e| e.to_string())?));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "explicit live Crossref network check"]
    fn live_crossref_discovery_returns_candidates() {
        let result = discover(
            "session types programming languages",
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(!result.is_empty());
        assert!(result.len() <= 12);
        assert!(
            result
                .iter()
                .all(|c| !c.title.is_empty() && crate::research::valid_artifact_url(&c.url))
        );
        println!("Crossref returned {} uncollected candidates", result.len());
    }
    #[test]
    fn provider_metadata_is_uncollected_and_bounded() {
        let candidates=parse_crossref(br#"{"message":{"items":[{"title":["Paper"],"URL":"https://example.org/paper","author":[{"family":"Smith"}],"issued":{"date-parts":[[2025]]}},{"title":["Unsafe"],"URL":"file:///tmp/paper"}]}}"#).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].author.as_deref(), Some("Smith"));
        assert_eq!(candidates[0].date.as_deref(), Some("2025"));
    }
    #[test]
    fn renderer_rejects_non_pdf_and_cancellation() {
        let scratch = Scratch::new().unwrap();
        let path = scratch.0.join("not.pdf");
        fs::write(&path, b"<html>oops").unwrap();
        assert!(snapshot_pdf(&path, &scratch.0, &AtomicBool::new(false)).is_err());
        let mut cmd = Command::new("true");
        assert!(
            process(
                &mut cmd,
                &AtomicBool::new(true),
                Duration::from_secs(1),
                100,
                &[]
            )
            .is_err()
        );
    }
}
