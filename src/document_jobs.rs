//! One active document worker plus one replaceable pending immutable request.
pub use crate::pandoc::ProcessingError;
use crate::{
    illustration::{self, AssetFile, IllustrationMetadata, PreparationInput, PreparedFigure},
    notes::{NoteId, Revision},
    pandoc::{self, PandocConfig},
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};

#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    pub note_id: NoteId,
    pub revision: Revision,
    pub source: String,
    /// Exact safe relative Markdown filename, including its logical directory.
    pub logical_path: String,
    /// Explicitly selected, owned files keyed relative to the snapshot root.
    pub resources: Vec<AssetFile>,
}
#[derive(Clone, Debug)]
pub enum ProcessingConfig {
    NativeIllustrations,
    Pandoc(PandocConfig),
}
#[derive(Debug)]
pub struct ProcessedDocument {
    pub source: String,
    pub logical_path: String,
    pub resources: Vec<AssetFile>,
    pub semantic: Option<serde_json::Value>,
    pub figures: Vec<PreparedFigure>,
    pub metadata: IllustrationMetadata,
}
impl ProcessedDocument {
    /// Figure references are source-relative; resource keys are snapshot-root-relative.
    pub fn figure_resource(&self, figure: &PreparedFigure) -> Option<&AssetFile> {
        let image = self.metadata.illustrations.get(&figure.id)?;
        if image.image != figure.asset_path
            || image.media_type != figure.kind.media_type()
            || !pandoc::safe_path(&image.image)
        {
            return None;
        }
        let parent = std::path::Path::new(&self.logical_path).parent()?;
        let key = parent.join(&image.image).to_str()?.replace('\\', "/");
        self.resources
            .iter()
            .find(|asset| asset.relative_path == key)
    }
}
#[derive(Debug)]
pub struct ProcessingResult {
    pub generation: u64,
    pub note_id: NoteId,
    pub revision: Revision,
    pub outcome: Result<ProcessedDocument, ProcessingError>,
}
struct Request {
    generation: u64,
    snapshot: DocumentSnapshot,
    config: ProcessingConfig,
}
struct Active {
    generation: u64,
    note_id: NoteId,
    revision: Revision,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<ProcessingResult>,
    thread: JoinHandle<()>,
}
#[derive(Default)]
pub struct DocumentJobs {
    generation: u64,
    active: Option<Active>,
    pending: Option<Request>,
}
impl DocumentJobs {
    /// Call only for actual source/resource/configuration requests, never each frame.
    /// Every submission invalidates older outcomes, including same-revision resource changes.
    pub fn submit(
        &mut self,
        snapshot: DocumentSnapshot,
        config: ProcessingConfig,
    ) -> Result<u64, ProcessingError> {
        // Invalidate old work even when the replacement snapshot is invalid.
        self.cancel();
        validate(&snapshot)?;
        let generation = self.generation;
        self.pending = Some(Request {
            generation,
            snapshot,
            config,
        });
        self.start_pending();
        Ok(generation)
    }
    pub fn cancel(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("document generation exhausted");
        self.pending = None;
        if let Some(active) = &self.active {
            active.cancel.store(true, Ordering::Release);
        }
    }
    pub fn is_busy(&self) -> bool {
        self.active.is_some() || self.pending.is_some()
    }
    /// Nonblocking. Caller must additionally match current note and revision before installing.
    pub fn poll(&mut self) -> Option<ProcessingResult> {
        let ready = self
            .active
            .as_ref()
            .is_some_and(|active| active.thread.is_finished());
        if !ready {
            return None;
        }
        let active = self.active.take().expect("checked active worker");
        let result = active
            .receiver
            .try_recv()
            .unwrap_or_else(|_| ProcessingResult {
                generation: active.generation,
                note_id: active.note_id,
                revision: active.revision,
                outcome: Err(ProcessingError::Command(
                    "document worker stopped without a result".into(),
                )),
            });
        let _ = active.thread.join();
        self.start_pending();
        (result.generation == self.generation).then_some(result)
    }
    fn start_pending(&mut self) {
        if self.active.is_some() {
            return;
        }
        let Some(request) = self.pending.take() else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let generation = request.generation;
        let note_id = request.snapshot.note_id;
        let revision = request.snapshot.revision;
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let Request {
                generation,
                snapshot,
                config,
            } = request;
            let note_id = snapshot.note_id;
            let revision = snapshot.revision;
            let outcome = process(snapshot, config, worker_cancel);
            let _ = sender.send(ProcessingResult {
                generation,
                note_id,
                revision,
                outcome,
            });
        });
        self.active = Some(Active {
            generation,
            note_id,
            revision,
            cancel,
            receiver,
            thread,
        });
    }
}
impl Drop for DocumentJobs {
    fn drop(&mut self) {
        // Never stall the UI on drop. The bounded worker owns its snapshot and reaps children.
        self.cancel();
    }
}
fn validate(snapshot: &DocumentSnapshot) -> Result<(), ProcessingError> {
    if !pandoc::safe_path(&snapshot.logical_path)
        || snapshot.source.len() > 1024 * 1024
        || snapshot.resources.len() > 128
    {
        return Err(ProcessingError::Snapshot(
            "unsafe logical path or source/file-count quota".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    for resource in &snapshot.resources {
        total = total.saturating_add(resource.bytes.len());
        if !pandoc::safe_path(&resource.relative_path)
            || resource.relative_path == snapshot.logical_path
            || !seen.insert(&resource.relative_path)
        {
            return Err(ProcessingError::Snapshot(
                "unsafe, duplicate or source-conflicting resource key".into(),
            ));
        }
        if resource.bytes.len() > 64 * 1024 * 1024 || total > 80 * 1024 * 1024 {
            return Err(ProcessingError::Snapshot("resource byte quota".into()));
        }
    }
    Ok(())
}
fn process(
    snapshot: DocumentSnapshot,
    config: ProcessingConfig,
    cancel: Arc<AtomicBool>,
) -> Result<ProcessedDocument, ProcessingError> {
    if cancel.load(Ordering::Acquire) {
        return Err(ProcessingError::Cancelled);
    }
    let mut semantic = match config {
        ProcessingConfig::NativeIllustrations => None,
        ProcessingConfig::Pandoc(config) => Some(pandoc::run(
            &snapshot.source,
            &snapshot.logical_path,
            &snapshot.resources,
            &config,
            cancel.clone(),
        )?),
    };
    // The named artwork subset must not become a general Markdown/YAML gate.
    let references = semantic
        .as_ref()
        .map(IllustrationMetadata::from_pandoc)
        .transpose()?
        .flatten();
    let prepared = if has_artwork_fence(&snapshot.source) {
        illustration::prepare_with_metadata(
            PreparationInput {
                source: snapshot.source.clone(),
                logical_path: snapshot.logical_path.clone(),
                assets: snapshot.resources.clone(),
            },
            references.as_ref(),
        )?
    } else {
        if references
            .as_ref()
            .is_some_and(|meta| !meta.illustrations.is_empty())
        {
            return Err(illustration::PreparationError::Metadata(
                "image references have no authored illustration fences".into(),
            )
            .into());
        }
        illustration::PreparedIllustrations {
            figures: Vec::new(),
            metadata: IllustrationMetadata::default(),
        }
    };
    if let Some(semantic) = &mut semantic {
        prepared.metadata.write_pandoc(semantic);
    }
    if cancel.load(Ordering::Acquire) {
        return Err(ProcessingError::Cancelled);
    }
    Ok(ProcessedDocument {
        source: snapshot.source,
        logical_path: snapshot.logical_path,
        resources: snapshot.resources,
        semantic,
        figures: prepared.figures,
        metadata: prepared.metadata,
    })
}

pub(crate) fn has_artwork_fence(source: &str) -> bool {
    use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
    Parser::new_ext(source, Options::all())
        .into_offset_iter()
        .any(|(event, _)| {
            let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event else {
                return false;
            };
            if let Some(attributes) = crate::markdown::fence_attributes(&info) {
                attributes
                    .classes
                    .iter()
                    .any(|class| matches!(class.as_str(), "illustration" | "comic"))
            } else {
                // Keep malformed intended artwork fences on the strict validation path.
                info.split_whitespace().any(|token| {
                    matches!(token.trim_matches(['{', '}']), ".illustration" | ".comic")
                })
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(source: &str) -> DocumentSnapshot {
        DocumentSnapshot {
            note_id: NoteId(uuid::Uuid::new_v4()),
            revision: Revision(uuid::Uuid::new_v4()),
            source: source.into(),
            logical_path: "chapter/source.md".into(),
            resources: Vec::new(),
        }
    }
    #[test]
    fn processed_metadata_is_the_renderer_resource_authority() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/fixtures/mvp-2");
        let original = std::fs::read_to_string(root.join("with-image-references.md")).unwrap();
        let mut request = snapshot(&original);
        request.logical_path = "chapter/source.md".into();
        for path in [
            "assets/fig-add-inputs/figure.svg",
            "assets/fig-choosing-a-result/strip.png",
        ] {
            request.resources.push(AssetFile {
                relative_path: format!("chapter/{path}"),
                bytes: std::fs::read(root.join(path)).unwrap(),
            });
        }
        let mut result = process(
            request,
            ProcessingConfig::NativeIllustrations,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let expected: IllustrationMetadata = serde_json::from_slice(
            &std::fs::read(root.join("expected-derived-metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(result.metadata, expected);
        assert_eq!(result.source, original);
        for figure in &result.figures {
            let asset = result.figure_resource(figure).unwrap();
            assert_eq!(
                asset.relative_path,
                format!(
                    "chapter/{}",
                    result.metadata.illustrations[&figure.id].image
                )
            );
        }
        let figure = result.figures[0].clone();
        result.metadata.illustrations.remove(&figure.id);
        assert!(result.figure_resource(&figure).is_none());
        // A conventional file in the snapshot does not authorize a missing or
        // inconsistent metadata entry, even when the figure's old path exists.
        result.metadata = expected.clone();
        result
            .metadata
            .illustrations
            .get_mut(&figure.id)
            .unwrap()
            .media_type = "image/png".into();
        assert!(result.figure_resource(&figure).is_none());
        result.metadata = expected;
        result
            .metadata
            .illustrations
            .get_mut(&figure.id)
            .unwrap()
            .image = "missing.svg".into();
        assert!(result.figure_resource(&figure).is_none());
    }

    #[test]
    fn latest_request_and_resource_validation() {
        let mut jobs = DocumentJobs::default();
        jobs.submit(snapshot("old"), ProcessingConfig::NativeIllustrations)
            .unwrap();
        let latest = jobs
            .submit(snapshot("latest"), ProcessingConfig::NativeIllustrations)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if let Some(result) = jobs.poll() {
                assert_eq!(result.generation, latest);
                assert_eq!(result.outcome.unwrap().source, "latest");
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(std::time::Duration::from_millis(2));
        }
        let ordinary = "---\ntitle: Hello!\n---\n\nA plain note.\n";
        let processed = process(
            snapshot(ordinary),
            ProcessingConfig::NativeIllustrations,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(processed.source, ordinary);
        assert!(processed.figures.is_empty());
        let mut bad = snapshot("unchanged");
        bad.resources.push(AssetFile {
            relative_path: "../escape".into(),
            bytes: vec![],
        });
        assert!(
            jobs.submit(bad, ProcessingConfig::NativeIllustrations)
                .is_err()
        );
    }
}
