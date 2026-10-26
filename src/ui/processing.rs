//! Small source-to-preview coordinator. Local artwork is explicitly selected, not synced.
use super::document::{ArtworkCache, render_block, render_pandoc};
use crate::{
    document_jobs::{DocumentJobs, DocumentSnapshot, ProcessedDocument, ProcessingConfig},
    illustration::AssetFile,
    notes::{NoteId, NoteView, Revision},
    pandoc::{FilterStage, PandocConfig},
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};

#[derive(Clone, Default, PartialEq, Eq)]
struct Settings {
    root: String,
    pandoc: bool,
    stages: String,
}
type Loaded = Result<(DocumentSnapshot, ProcessingConfig), String>;

#[derive(Default)]
pub(super) struct Processing {
    pub(super) resource_root: String,
    pandoc: bool,
    stages: String,
    applied: Settings,
    jobs: DocumentJobs,
    loading: Option<mpsc::Receiver<Loaded>>,
    requested: Option<(NoteId, Revision)>,
    result: Option<(NoteId, Revision, ProcessedDocument)>,
    artwork: ArtworkCache,
    native: Option<crate::markdown::Document>,
    error: Option<String>,
    refresh: bool,
}
impl Processing {
    pub(super) fn controls(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Document processing", |ui| {
            ui.label("Artwork directory (local only; not transferred by sync)");
            super::field_single(ui, &mut self.resource_root, "resource-root");
            #[cfg(target_os = "linux")]
            {
                ui.checkbox(&mut self.pandoc, "Use installed Pandoc");
                if self.pandoc {
                    ui.label("Ordered filters: lua:/path, json:/path, or prepare:[\"program\",\"argument\"] (one per line)");
                    ui.add(egui::TextEdit::multiline(&mut self.stages).desired_rows(3));
                }
            }
            #[cfg(target_os = "android")]
            ui.label("Local illustration/comic preparation subset; external Pandoc unavailable.");
            if self.applied != self.pending_settings() { ui.label("Settings changed — Process applies them together."); }
            if ui.button("Process / refresh artwork").clicked() {
                self.applied = self.pending_settings();
                self.refresh = true;
            }
        });
    }

    pub(super) fn update(&mut self, ctx: &egui::Context, view: &NoteView) {
        let key = (view.note.id, view.note.revision);
        if self.refresh && self.requested.is_some() {
            self.jobs.cancel();
            self.requested = None;
        }
        if self.loading.is_none() && (self.requested != Some(key) || self.refresh) {
            self.refresh = false;
            self.requested = Some(key);
            self.jobs.cancel();
            let source = view.note.text.clone();
            let logical_path = view.note.path.clone();
            let root = self.applied.root.clone();
            let config = self.config();
            let (sender, receiver) = mpsc::sync_channel(1);
            self.loading = Some(receiver);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let snapshot = config.and_then(|config| {
                    let resources = if root.trim().is_empty() {
                        Ok(Vec::new())
                    } else {
                        collect_resources(Path::new(root.trim()))
                    };
                    resources.map(|resources| {
                        (
                            DocumentSnapshot {
                                note_id: key.0,
                                revision: key.1,
                                source,
                                logical_path,
                                resources,
                            },
                            config,
                        )
                    })
                });
                let _ = sender.send(snapshot);
                ctx.request_repaint();
            });
        }
        if let Some(receiver) = &self.loading {
            match receiver.try_recv() {
                Ok(snapshot) => {
                    self.loading = None;
                    match snapshot {
                        Ok((snapshot, config))
                            if (snapshot.note_id, snapshot.revision) == key && !self.refresh =>
                        {
                            match self
                                .jobs
                                .submit(snapshot, config)
                                .map_err(|e| e.to_string())
                            {
                                Ok(_) => self.error = None,
                                Err(error) => self.error = Some(error),
                            }
                        }
                        Ok(_) => {
                            self.requested = None;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.loading = None;
                    self.error = Some("Resource reader stopped.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(result) = self.jobs.poll() {
            if (result.note_id, result.revision) == key {
                match result.outcome {
                    Ok(document) => {
                        let assets = document
                            .figures
                            .iter()
                            .filter_map(|figure| {
                                document.figure_resource(figure).map(|file| {
                                    (
                                        figure.id.clone(),
                                        file.bytes.clone(),
                                        matches!(
                                            figure.kind,
                                            crate::illustration::ArtworkKind::Illustration
                                        ),
                                    )
                                })
                            })
                            .collect();
                        self.artwork.prepare(ctx, result.generation, assets);
                        self.native = Some(crate::markdown::read(key.0, key.1, &document.source));
                        self.result = Some((key.0, key.1, document));
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        }
        self.artwork.poll(ctx);
        if self.loading.is_some() || self.jobs.is_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn pending_settings(&self) -> Settings {
        Settings {
            root: self.resource_root.clone(),
            pandoc: self.pandoc,
            stages: self.stages.clone(),
        }
    }

    fn config(&self) -> Result<ProcessingConfig, String> {
        if !self.applied.pandoc {
            return Ok(ProcessingConfig::NativeIllustrations);
        }
        let mut config = PandocConfig::default();
        for line in self
            .applied
            .stages
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let (kind, value) = line.split_once(':').ok_or("Filter needs a kind and ':'")?;
            config.stages.push(match kind {
                "lua" => FilterStage::Lua(PathBuf::from(value)),
                "json" => FilterStage::Json(PathBuf::from(value)),
                "prepare" => {
                    let mut args: Vec<String> =
                        serde_json::from_str(value).map_err(|e| e.to_string())?;
                    if args.is_empty() {
                        return Err("Preparation command needs a program.".into());
                    }
                    FilterStage::Preparation {
                        program: PathBuf::from(args.remove(0)),
                        args,
                    }
                }
                _ => return Err(format!("Unknown filter kind: {kind}")),
            });
        }
        Ok(ProcessingConfig::Pandoc(config))
    }

    pub(super) fn show(&self, ui: &mut egui::Ui, view: &NoteView, navigate: &mut Option<String>) {
        if self.loading.is_some() || self.jobs.is_busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Processing…");
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        let Some((id, revision, result)) = self
            .result
            .as_ref()
            .filter(|(id, _, _)| *id == view.note.id)
        else {
            ui.label("No processed preview yet. Source remains editable.");
            return;
        };
        let _ = id;
        if *revision != view.note.revision || self.error.is_some() {
            ui.label("Previous rendering — refresh pending or failed.");
        }
        if let Some(blocks) = result
            .semantic
            .as_ref()
            .and_then(|value| value["blocks"].as_array())
        {
            render_pandoc(ui, blocks, &self.artwork, &result.figures);
        } else {
            let Some(native) = &self.native else { return };
            let frontmatter_end = frontmatter_end(&result.source);
            for block in &native.blocks {
                if block
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin.bytes.end <= frontmatter_end)
                {
                    continue;
                }
                if let Some(figure) = block.origin.as_ref().and_then(|origin| {
                    result
                        .figures
                        .iter()
                        .find(|figure| figure.source_bytes == origin.bytes)
                }) {
                    self.artwork.figure(ui, figure);
                } else {
                    render_block(ui, block, navigate);
                }
            }
        }
    }
}

/// Snapshot regular files only. Do not follow symlinks or silently traverse a huge tree.
fn collect_resources(root: &Path) -> Result<Vec<AssetFile>, String> {
    fn visit(
        root: &Path,
        directory: &Path,
        files: &mut Vec<AssetFile>,
        total: &mut usize,
        depth: usize,
        visited: &mut usize,
    ) -> Result<(), String> {
        if depth > 12 {
            return Err("Resource directory nesting limit exceeded.".into());
        }
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            *visited += 1;
            if *visited > 2048 {
                return Err("Resource directory entry limit exceeded.".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err("Resource snapshots do not follow symlinks.".into());
            }
            if metadata.is_dir() {
                visit(root, &path, files, total, depth + 1, visited)?;
            } else if metadata.is_file() {
                // Only saved artwork and its sidecars are part of this slice.
                if !path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|ext| matches!(ext, "json" | "svg" | "png"))
                {
                    continue;
                }
                if files.len() >= 128 || metadata.len() > 64 * 1024 * 1024 {
                    return Err("Resource file quota exceeded.".into());
                }
                use std::io::Read;
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take(64 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                *total += bytes.len();
                if bytes.len() > 64 * 1024 * 1024 || *total > 80 * 1024 * 1024 {
                    return Err("Resource byte quota exceeded.".into());
                }
                let relative_path = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("Resource path is not UTF-8")?
                    .replace('\\', "/");
                files.push(AssetFile {
                    relative_path,
                    bytes,
                });
            }
        }
        Ok(())
    }
    if std::fs::symlink_metadata(root)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Select a regular resource directory.".into());
    }
    let mut files = Vec::new();
    let mut total = 0;
    visit(root, root, &mut files, &mut total, 0, &mut 0)?;
    Ok(files)
}

// Presentation only: original metadata remains in source mode and its exact source outline.
fn frontmatter_end(source: &str) -> usize {
    if !source.starts_with("---\n") {
        return 0;
    }
    source[4..].find("\n---\n").map_or(0, |end| end + 9)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_are_latched_and_superseded_capture_is_rejected() {
        let mut app = crate::App::new();
        let crate::commands::CommandResult::Note(view) = app
            .execute(
                crate::commands::Command::CreateNote {
                    path: "note.md".into(),
                    text: "Ordinary note".into(),
                },
                crate::commands::Caller::Local,
            )
            .unwrap()
        else {
            panic!("note")
        };
        let key = (view.note.id, view.note.revision);
        let capture = || {
            let (tx, rx) = mpsc::sync_channel(1);
            tx.send(Ok((
                DocumentSnapshot {
                    note_id: key.0,
                    revision: key.1,
                    source: view.note.text.clone(),
                    logical_path: view.note.path.clone(),
                    resources: Vec::new(),
                },
                ProcessingConfig::NativeIllustrations,
            )))
            .unwrap();
            rx
        };
        let ctx = egui::Context::default();
        let mut processing = Processing {
            requested: Some(key),
            loading: Some(capture()),
            pandoc: true,
            stages: "invalid pending settings".into(),
            ..Default::default()
        };
        // Visible edits have not been applied: the captured native config must win.
        processing.update(&ctx, &view);
        assert!(processing.error.is_none());
        assert!(processing.jobs.is_busy() || processing.result.is_some());
        assert!(matches!(
            processing.config(),
            Ok(ProcessingConfig::NativeIllustrations)
        ));
        // Applying the changed configuration invalidates an older capture, even at the same revision.
        processing.result = None;
        processing.applied = processing.pending_settings();
        processing.refresh = true;
        processing.loading = Some(capture());
        processing.update(&ctx, &view);
        assert!(processing.refresh);
        assert!(processing.requested.is_none());
        assert!(processing.result.is_none());
        assert!(processing.config().is_err());
    }
}
