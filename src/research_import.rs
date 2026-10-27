//! Bounded local extraction; OCR is an explicit, optional asset-worker operation.

use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::Duration,
};
use uuid::Uuid;

const MAX_INPUT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_PAGES: usize = 500;
const QUEUE_SIZE: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportedKind {
    Pdf,
    Text,
}

#[derive(Debug)]
pub(crate) struct ImportedPage {
    pub number: Option<u32>,
    pub text: String,
}

#[derive(Debug)]
pub(crate) struct ImportedSource {
    pub title: String,
    pub origin: String,
    pub kind: ImportedKind,
    pub pages: Vec<ImportedPage>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ImportError {
    #[error("research import is busy; wait for pending imports")]
    Busy,
    #[error("research import worker is unavailable")]
    Unavailable,
    #[error("research import was cancelled")]
    Cancelled,
    #[error("select a regular PDF, UTF-8 text, or Markdown file")]
    InvalidPath,
    #[error("source exceeds the 32 MiB input limit")]
    InputTooLarge,
    #[error("extracted source exceeds the 8 MiB text limit")]
    TextTooLarge,
    #[error("PDF exceeds the 500 page limit")]
    TooManyPages,
    #[error("source is not valid UTF-8 text")]
    InvalidText,
    #[error("source contains no usable text; scanned PDFs need the explicit OCR action")]
    NoText,
    #[error(
        "PDF extraction requires Poppler's pdftotext on Linux; install Poppler or import a text export"
    )]
    PdfUnavailable,
    #[error("PDF extraction or OCR reached its processing time limit")]
    TimedOut,
    #[error("PDF extraction failed; check that the PDF is readable and not password protected")]
    PdfFailed,
    #[error("could not read research source: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug)]
pub(crate) struct ImportCompletion {
    pub id: Uuid,
    pub result: Result<ImportedSource, ImportError>,
}

struct ImportJob {
    id: Uuid,
    path: PathBuf,
    generation: u64,
}

struct Cancellation {
    stopped: AtomicBool,
    generation: AtomicU64,
}
impl Cancellation {
    fn check(&self, generation: u64) -> Result<(), ImportError> {
        if self.stopped.load(Ordering::Relaxed)
            || self.generation.load(Ordering::Relaxed) != generation
        {
            Err(ImportError::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub(crate) struct ImportWorker {
    jobs: SyncSender<ImportJob>,
    completions: Receiver<ImportCompletion>,
    cancellation: Arc<Cancellation>,
}

impl ImportWorker {
    pub fn new(repaint: impl Fn() + Send + 'static) -> io::Result<Self> {
        let (jobs, incoming) = mpsc::sync_channel::<ImportJob>(QUEUE_SIZE);
        let (finished, completions) = mpsc::sync_channel(QUEUE_SIZE);
        let cancellation = Arc::new(Cancellation {
            stopped: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        });
        let worker_cancel = Arc::clone(&cancellation);
        thread::Builder::new()
            .name("research-import".into())
            .spawn(move || {
                while !worker_cancel.stopped.load(Ordering::Relaxed) {
                    let job = match incoming.recv_timeout(Duration::from_millis(25)) {
                        Ok(job) => job,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let result = import_path(&job.path, &worker_cancel, job.generation);
                    let mut completion = ImportCompletion { id: job.id, result };
                    loop {
                        if worker_cancel.stopped.load(Ordering::Relaxed) {
                            return;
                        }
                        match finished.try_send(completion) {
                            Ok(()) => {
                                repaint();
                                break;
                            }
                            Err(TrySendError::Full(value)) => {
                                completion = value;
                                thread::sleep(Duration::from_millis(25));
                            }
                            Err(TrySendError::Disconnected(_)) => return,
                        }
                    }
                }
            })?;
        Ok(Self {
            jobs,
            completions,
            cancellation,
        })
    }

    /// Submission never reads the filesystem or blocks the UI thread.
    pub fn try_import(&self, path: PathBuf) -> Result<Uuid, ImportError> {
        let id = Uuid::new_v4();
        let generation = self.cancellation.generation.load(Ordering::Relaxed);
        self.jobs
            .try_send(ImportJob {
                id,
                path,
                generation,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => ImportError::Busy,
                TrySendError::Disconnected(_) => ImportError::Unavailable,
            })?;
        Ok(id)
    }

    pub fn poll(&self) -> Option<ImportCompletion> {
        self.completions.try_recv().ok()
    }

    /// Cancels active and queued jobs; already delivered completions retain their IDs.
    pub fn cancel_all(&self) {
        self.cancellation.generation.fetch_add(1, Ordering::Relaxed);
    }
}
impl Drop for ImportWorker {
    fn drop(&mut self) {
        // Do not join filesystem/process work on the UI thread. The worker observes
        // shutdown, kills/reaps its child, and drops all owned resources itself.
        self.cancellation.stopped.store(true, Ordering::Relaxed);
    }
}

fn import_path(
    path: &Path,
    cancellation: &Cancellation,
    generation: u64,
) -> Result<ImportedSource, ImportError> {
    cancellation.check(generation)?;
    let kind = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pdf") => ImportedKind::Pdf,
        Some("txt" | "md" | "markdown" | "text") => ImportedKind::Text,
        _ => return Err(ImportError::InvalidPath),
    };
    // Reject non-regular paths before open (in particular named pipes).
    if !std::fs::metadata(path)?.is_file() {
        return Err(ImportError::InvalidPath);
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(ImportError::InvalidPath);
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(ImportError::InputTooLarge);
    }
    let origin = path.canonicalize()?.to_string_lossy().into_owned();
    let pages = match kind {
        ImportedKind::Text => {
            let bytes = read_bounded(file, MAX_TEXT_BYTES)?;
            cancellation.check(generation)?;
            let text = String::from_utf8(bytes).map_err(|_| ImportError::InvalidText)?;
            if text.trim().is_empty() {
                return Err(ImportError::NoText);
            }
            vec![ImportedPage { number: None, text }]
        }
        ImportedKind::Pdf => extract_pdf(file, &|| cancellation.check(generation))?,
    };
    cancellation.check(generation)?;
    let title = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok(ImportedSource {
        title,
        origin,
        kind,
        pages,
    })
}

fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, ImportError> {
    let mut bytes = Vec::new();
    reader.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(ImportError::TextTooLarge);
    }
    Ok(bytes)
}

fn pdf_pages(text: String) -> Result<Vec<ImportedPage>, ImportError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(ImportError::TextTooLarge);
    }
    // Poppler terminates each page with a form feed. Drop only its final empty
    // sentinel; preserve blank physical pages so later citations keep their number.
    let text = text.strip_suffix('\u{c}').unwrap_or(&text);
    let mut pages = Vec::new();
    for (index, page) in text.split('\u{c}').enumerate() {
        if index >= MAX_PAGES {
            return Err(ImportError::TooManyPages);
        }
        pages.push(ImportedPage {
            number: Some(index as u32 + 1),
            text: page.to_owned(),
        });
    }
    if pages.iter().all(|page| page.text.trim().is_empty()) {
        return Err(ImportError::NoText);
    }
    Ok(pages)
}

/// Reuses the bounded PDF extraction pipeline for an already retained original.
/// Called only by the asset worker, with its cancellation token.
pub(crate) fn extract_retained_pdf(
    path: &Path,
    cancel: &AtomicBool,
) -> Result<Vec<crate::research::SourcePage>, ImportError> {
    let check = || {
        if cancel.load(Ordering::Relaxed) {
            Err(ImportError::Cancelled)
        } else {
            Ok(())
        }
    };
    check()?;
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(ImportError::InvalidPath);
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(ImportError::InputTooLarge);
    }
    let pages = extract_pdf(file, &check)?;
    check()?;
    Ok(pages
        .into_iter()
        .map(|page| crate::research::SourcePage {
            number: page.number,
            text: page.text,
        })
        .collect())
}

/// Explicit opt-in OCR. Keeps the original untouched and preserves physical page numbers.
/// English is the installed baseline; runtime errors never masquerade as extracted evidence.
pub(crate) fn ocr_retained_pdf(
    path: &Path,
    cancel: &AtomicBool,
) -> Result<Vec<crate::research::SourcePage>, ImportError> {
    use crate::research_reader::{Scratch, pdf_page_count, process, render_page, snapshot_pdf};
    use std::process::Command;
    use std::time::Instant;
    let error = |message: String| ImportError::Io(io::Error::other(message));
    let scratch = Scratch::new().map_err(error)?;
    let snapshot = snapshot_pdf(path, &scratch.0, cancel).map_err(error)?;
    let count = pdf_page_count(&snapshot, cancel).map_err(error)?;
    let started = Instant::now();
    let mut pages = Vec::new();
    let mut total = 0;
    for page in 1..=count {
        if cancel.load(Ordering::Relaxed) {
            return Err(ImportError::Cancelled);
        }
        if started.elapsed() > Duration::from_secs(180) {
            return Err(ImportError::TimedOut);
        }
        render_page(&snapshot, page, &scratch.0, 2200, cancel).map_err(error)?;
        let image = scratch.0.join(format!("page-{page}.png"));
        let mut command = Command::new("tesseract");
        command.arg(&image).args(["stdout", "-l", "eng"]);
        let (success, bytes) = process(
            &mut command,
            cancel,
            Duration::from_secs(30),
            MAX_TEXT_BYTES as u64,
            &[],
        )
        .map_err(error)?;
        if !success {
            return Err(error(
                "OCR failed; install Tesseract and English (eng) language data, and check the PDF"
                    .into(),
            ));
        }
        total += bytes.len();
        if total > MAX_TEXT_BYTES {
            return Err(ImportError::TextTooLarge);
        }
        let text = String::from_utf8(bytes).map_err(|_| ImportError::InvalidText)?;
        pages.push(crate::research::SourcePage {
            number: Some(page),
            text,
        });
        let _ = std::fs::remove_file(image);
    }
    if pages.iter().all(|page| page.text.trim().is_empty()) {
        return Err(ImportError::NoText);
    }
    Ok(pages)
}

#[cfg(target_os = "linux")]
fn extract_pdf(
    mut file: File,
    check_cancel: &dyn Fn() -> Result<(), ImportError>,
) -> Result<Vec<ImportedPage>, ImportError> {
    use std::{
        fs::OpenOptions,
        process::{Command, Stdio},
        time::Instant,
    };
    // Snapshot only the explicitly selected, bounded input. pdftotext never sees
    // a path that could be replaced by a FIFO or grow indefinitely after validation.
    struct Snapshot(PathBuf);
    impl Drop for Snapshot {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
    let snapshot_path =
        std::env::temp_dir().join(format!("tanglewood-research-{}.pdf", Uuid::new_v4()));
    let mut output = options.open(&snapshot_path)?;
    let snapshot = Snapshot(snapshot_path);
    let copied = io::copy(&mut (&mut file).take(MAX_INPUT_BYTES + 1), &mut output)?;
    if copied > MAX_INPUT_BYTES {
        return Err(ImportError::InputTooLarge);
    }
    drop(output);
    check_cancel()?;
    let mut child = Command::new("pdftotext")
        .args(["-enc", "UTF-8", "-eol", "unix"])
        .arg(&snapshot.0)
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ImportError::PdfUnavailable
            } else {
                ImportError::Io(error)
            }
        })?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(ImportError::PdfFailed);
    };
    // Drain stdout concurrently so a full pipe cannot deadlock the process.
    // The reader owns at most MAX_TEXT_BYTES + 1 bytes and closes on overflow.
    let reader = match thread::Builder::new()
        .name("research-pdf-output".into())
        .spawn(move || read_bounded(stdout, MAX_TEXT_BYTES))
    {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ImportError::Io(error));
        }
    };
    let started = Instant::now();
    let status = loop {
        let failure = check_cancel().err().or_else(|| {
            (started.elapsed() >= Duration::from_secs(20)).then_some(ImportError::TimedOut)
        });
        if let Some(error) = failure {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(ImportError::Io(error));
            }
        }
    };
    let bytes = reader.join().map_err(|_| ImportError::PdfFailed)??;
    if !status.success() {
        return Err(ImportError::PdfFailed);
    }
    pdf_pages(String::from_utf8(bytes).map_err(|_| ImportError::InvalidText)?)
}

#[cfg(not(target_os = "linux"))]
fn extract_pdf(
    _: File,
    _: &dyn Fn() -> Result<(), ImportError>,
) -> Result<Vec<ImportedPage>, ImportError> {
    Err(ImportError::PdfUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TempFile(PathBuf);
    impl TempFile {
        fn text(bytes: &[u8]) -> Self {
            let path =
                std::env::temp_dir().join(format!("tanglewood-import-test-{}.md", Uuid::new_v4()));
            std::fs::write(&path, bytes).unwrap();
            Self(path)
        }
    }
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    fn cancellation() -> Cancellation {
        Cancellation {
            stopped: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        }
    }

    #[test]
    fn pdf_keeps_blank_page_numbers_and_drops_only_final_sentinel() {
        let pages = pdf_pages("first\u{c}\u{c}third\u{c}".into()).unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[1].number, Some(2));
        assert_eq!(pages[1].text, "");
        assert_eq!(pages[2].number, Some(3));
        assert_eq!(pages[2].text, "third");
        assert!(matches!(
            pdf_pages("\u{c}\u{c}".into()),
            Err(ImportError::NoText)
        ));
    }

    #[test]
    fn rejects_output_and_page_overflow_without_truncation() {
        assert!(matches!(
            read_bounded(&b"12345"[..], 4),
            Err(ImportError::TextTooLarge)
        ));
        assert!(matches!(
            pdf_pages("page\u{c}".repeat(MAX_PAGES + 1)),
            Err(ImportError::TooManyPages)
        ));
    }

    #[test]
    fn imports_explicit_utf8_markdown_without_rewriting_it() {
        let file = TempFile::text("# Title\n\nαβγ\r\n".as_bytes());
        let imported = import_path(&file.0, &cancellation(), 0).unwrap();
        assert_eq!(imported.kind, ImportedKind::Text);
        assert_eq!(imported.pages[0].number, None);
        assert_eq!(imported.pages[0].text, "# Title\n\nαβγ\r\n");
        assert_eq!(
            imported.origin,
            file.0.canonicalize().unwrap().to_string_lossy()
        );
        let invalid = TempFile::text(&[0xff]);
        assert!(matches!(
            import_path(&invalid.0, &cancellation(), 0),
            Err(ImportError::InvalidText)
        ));
    }

    #[test]
    fn rejects_oversized_input_and_cancelled_job() {
        let file = TempFile::text(b"text");
        File::options()
            .write(true)
            .open(&file.0)
            .unwrap()
            .set_len(MAX_INPUT_BYTES + 1)
            .unwrap();
        assert!(matches!(
            import_path(&file.0, &cancellation(), 0),
            Err(ImportError::InputTooLarge)
        ));
        let cancelled = cancellation();
        cancelled.generation.store(1, Ordering::Relaxed);
        assert!(matches!(
            import_path(&file.0, &cancelled, 0),
            Err(ImportError::Cancelled)
        ));
    }
}
