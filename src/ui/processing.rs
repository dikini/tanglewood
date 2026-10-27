//! Small source-to-preview coordinator. Saved local artwork loads automatically, not via sync.
use super::document::{ArtworkCache, editable_block, render_node, render_pandoc};
use crate::document_world::DocumentWorld;
use crate::{
    document_jobs::{DocumentJobs, DocumentSnapshot, ProcessedDocument, ProcessingConfig},
    illustration::AssetFile,
    notes::{NoteId, NoteView, Revision},
    pandoc::{FilterStage, PandocConfig},
};
use eframe::egui;
use std::sync::Arc;
use std::{
    ops::Range,
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
    vault_root: Option<PathBuf>,
    pub(super) resource_root: String,
    pandoc: bool,
    stages: String,
    applied: Settings,
    jobs: DocumentJobs,
    loading: Option<mpsc::Receiver<Loaded>>,
    requested: Option<(NoteId, Revision)>,
    result: Option<(NoteId, Revision, ProcessedDocument)>,
    artwork: ArtworkCache,
    staging_artwork: ArtworkCache,
    staged: Option<(NoteId, Revision, ProcessedDocument)>,
    native: Option<Arc<DocumentWorld>>,
    accepted_native: Option<Arc<DocumentWorld>>,
    error: Option<String>,
    presentation_error: Option<String>,
    refresh: bool,
    focus_after_edit: std::cell::Cell<Option<(NoteId, usize)>>,
}
impl Processing {
    pub(super) fn focus_block(&mut self, note: NoteId, offset: usize) {
        self.focus_after_edit.set(Some((note, offset)));
    }

    pub(super) fn for_vault(vault_root: Option<PathBuf>) -> Self {
        Self {
            vault_root,
            ..Default::default()
        }
    }
    pub(super) fn controls(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.label("Artwork directory override (blank uses vault-relative saved images)");
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
                self.request_refresh();
            }
        });
    }

    /// Retry the applied configuration; pending Settings edits need explicit Apply.
    pub(super) fn request_refresh(&mut self) {
        self.refresh = true;
        self.discard_staged();
    }
    fn discard_staged(&mut self) {
        self.staged = None;
        self.staging_artwork.invalidate();
    }

    fn bindings_current(&self, view: &NoteView) -> bool {
        let key = (view.note.id, view.note.revision);
        self.requested == Some(key)
            && !self.refresh
            && self.loading.is_none()
            && !self.jobs.is_busy()
            && self.staged.is_none()
            && self.error.is_none()
            && !view.note.trashed
            && self.result.as_ref().is_some_and(|(id, revision, result)| {
                (*id, *revision) == key && result.source == view.note.text
            })
    }

    pub(super) fn status(&self) -> (&str, bool) {
        if self.error_details().is_some() {
            ("Preview needs attention", true)
        } else if self.refresh
            || self.loading.is_some()
            || self.jobs.is_busy()
            || self.artwork.is_busy()
            || self.staged.is_some()
            || self.staging_artwork.is_busy()
        {
            ("Processing…", false)
        } else {
            ("", false)
        }
    }
    pub(super) fn error_details(&self) -> Option<&str> {
        self.error
            .as_deref()
            .or_else(|| self.artwork.error())
            .or(self.presentation_error.as_deref())
    }

    pub(super) fn update(
        &mut self,
        ctx: &egui::Context,
        view: &NoteView,
        document: Arc<DocumentWorld>,
    ) {
        let key = (view.note.id, view.note.revision);
        if (document.note(), document.revision()) != key || document.source() != view.note.text {
            self.error = Some("Source context does not match accepted note revision.".into());
            return;
        }
        self.accepted_native = Some(document);
        if self.requested != Some(key) && self.staged.is_some() {
            self.discard_staged();
        }
        if self.refresh && self.requested.is_some() {
            self.jobs.cancel();
            self.requested = None;
        }
        if self.loading.is_none() && (self.requested != Some(key) || self.refresh) {
            self.refresh = false;
            self.discard_staged();
            // Keep the last-good document and its matching textures under their old
            // identity while processing. bindings_current disables every old edit action.
            self.requested = Some(key);
            self.jobs.cancel();
            let source = view.note.text.clone();
            let logical_path = view.note.path.clone();
            let root = self.applied.root.clone();
            let vault_root = self.vault_root.clone();
            let config = self.config();
            let (sender, receiver) = mpsc::sync_channel(1);
            self.loading = Some(receiver);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let snapshot = config.and_then(|config| {
                    let resources = if root.trim().is_empty() {
                        vault_root.as_deref().map_or_else(
                            || Ok(Vec::new()),
                            |root| collect_saved_resources(root, &logical_path, &source),
                        )
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
                    Ok(document) => self.stage_document(ctx, key, result.generation, document),
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        }
        self.finish_staged(ctx, view);
        if self.loading.is_some()
            || self.jobs.is_busy()
            || self.staging_artwork.is_busy()
            || self.staged.is_some()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn stage_document(
        &mut self,
        ctx: &egui::Context,
        key: (NoteId, Revision),
        generation: u64,
        document: ProcessedDocument,
    ) {
        let assets: Vec<_> = document
            .figures
            .iter()
            .filter_map(|figure| {
                document.figure_resource(figure).map(|file| {
                    (
                        figure.id.clone(),
                        file.bytes.clone(),
                        document.metadata.illustrations[&figure.id].media_type == "image/svg+xml",
                    )
                })
            })
            .collect();
        if assets.len() != document.figures.len() {
            self.error = Some("Artwork file missing from processed snapshot.".into());
            self.discard_staged();
            return;
        }
        self.staging_artwork.prepare(ctx, generation, assets);
        self.staged = Some((key.0, key.1, document));
    }

    fn finish_staged(&mut self, ctx: &egui::Context, view: &NoteView) {
        self.staging_artwork.poll(ctx);
        if self.staging_artwork.is_busy() {
            return;
        }
        let Some((id, revision, document)) = self.staged.take() else {
            return;
        };
        let key = (view.note.id, view.note.revision);
        if (id, revision) != key
            || self.requested != Some(key)
            || self.refresh
            || self.loading.is_some()
            || self.jobs.is_busy()
            || document.source != view.note.text
        {
            self.staging_artwork.invalidate();
            return;
        }
        if let Some(error) = self.staging_artwork.error() {
            self.error = Some(error.to_owned());
            self.staging_artwork.invalidate();
            return;
        }
        self.presentation_error = document
            .semantic
            .as_ref()
            .and_then(|value| value["blocks"].as_array())
            .and_then(|blocks| {
                super::document::pandoc_diagnostic(blocks, &document.figures, &document.source)
            });
        let Some(native) = self.accepted_native.as_ref().filter(|native| {
            (native.note(), native.revision()) == key && native.source() == document.source
        }) else {
            self.error = Some("Source context unavailable for processed revision.".into());
            self.discard_staged();
            return;
        };
        self.native = Some(native.clone());
        std::mem::swap(&mut self.artwork, &mut self.staging_artwork);
        self.staging_artwork.invalidate();
        self.result = Some((id, revision, document));
        self.error = None;
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

    pub(super) fn show(
        &self,
        ui: &mut egui::Ui,
        view: &NoteView,
        navigate: &mut Option<String>,
    ) -> Option<Range<usize>> {
        if let Some((_, _, result)) = self
            .result
            .as_ref()
            .filter(|(id, _, _)| *id == view.note.id)
        {
            let current = self.bindings_current(view);
            if let Some(blocks) = result
                .semantic
                .as_ref()
                .and_then(|value| value["blocks"].as_array())
            {
                render_pandoc(
                    ui,
                    blocks,
                    &self.artwork,
                    &result.figures,
                    &result.source,
                    current,
                    navigate,
                )
            } else {
                self.show_native(
                    ui,
                    view,
                    self.native.as_ref()?,
                    &result.source,
                    &result.figures,
                    current,
                    navigate,
                    None,
                )
            }
        } else {
            // This is the accepted original source, never guessed filter output.
            // Missing artwork must not erase the note's independently readable prose.
            let (source, native) = self.fallback(view)?;
            self.show_native(
                ui,
                view,
                native,
                source,
                &[],
                !view.note.trashed,
                navigate,
                None,
            )
        }
    }

    /// An original-source context snapshot keeps the active block and its neighbours
    /// stable during a draft, even if the accepted revision changes underneath it.
    pub(super) fn show_inline(
        &self,
        ui: &mut egui::Ui,
        view: &NoteView,
        native: &DocumentWorld,
        source: &str,
        range: &Range<usize>,
        editor: &mut dyn FnMut(&mut egui::Ui),
    ) {
        let figures = self
            .result
            .as_ref()
            .filter(|(id, revision, result)| {
                *id == native.note() && *revision == native.revision() && result.source == source
            })
            .map_or(&[][..], |(_, _, result)| result.figures.as_slice());
        self.show_native(
            ui,
            view,
            native,
            source,
            figures,
            false,
            &mut None,
            Some((range, editor)),
        );
    }

    fn fallback(&self, view: &NoteView) -> Option<(&str, &DocumentWorld)> {
        self.accepted_native
            .as_ref()
            .filter(|document| {
                document.note() == view.note.id
                    && document.revision() == view.note.revision
                    && document.source() == view.note.text
            })
            .map(|document| (document.source(), document.as_ref()))
    }

    fn show_native(
        &self,
        ui: &mut egui::Ui,
        view: &NoteView,
        native: &DocumentWorld,
        source: &str,
        figures: &[crate::illustration::PreparedFigure],
        current: bool,
        navigate: &mut Option<String>,
        mut editing: Option<(&Range<usize>, &mut dyn FnMut(&mut egui::Ui))>,
    ) -> Option<Range<usize>> {
        let frontmatter_end = frontmatter_end(source);
        let mut selected = None;
        if current && editing.is_none() {
            if let Some((note, offset)) = self.focus_after_edit.take() {
                if note == view.note.id {
                    if let Some(block) =
                        native
                            .roots()
                            .iter()
                            .filter_map(|e| native.node(*e))
                            .find(|b| {
                                b.origin.as_ref().is_some_and(|o| {
                                    o.bytes.end > frontmatter_end && o.bytes.end > offset
                                })
                            })
                    {
                        ui.memory_mut(|m| m.request_focus(super::navigation::block_id(block.id)));
                    }
                }
            }
            let blocks: Vec<_> = native
                .roots()
                .iter()
                .filter_map(|entity| {
                    let block = native.node(*entity)?;
                    let origin = block.origin.as_ref()?;
                    (origin.bytes.end > frontmatter_end
                        && origin.note == view.note.id
                        && origin.revision == view.note.revision
                        && valid_range(source, &origin.bytes))
                    .then(|| super::navigation::block_id(block.id))
                })
                .collect();
            super::navigation::navigate(ui.ctx(), &blocks);
        }
        for entity in native.roots() {
            let Some(block) = native.node(*entity) else {
                continue;
            };
            if let Some((range, editor)) = editing.as_mut() {
                if block
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin.bytes == **range)
                {
                    editable_block(ui, block.id, None, |ui| editor(ui));
                    continue;
                }
            }
            if block
                .origin
                .as_ref()
                .is_some_and(|origin| origin.bytes.end <= frontmatter_end)
            {
                continue;
            }
            let range = current
                .then(|| block.origin.as_ref())
                .flatten()
                .filter(|origin| {
                    origin.note == view.note.id
                        && origin.revision == view.note.revision
                        && valid_range(source, &origin.bytes)
                })
                .map(|origin| origin.bytes.clone());
            if let Some(range) = editable_block(ui, block.id, range, |ui| {
                if let Some(figure) = block.origin.as_ref().and_then(|origin| {
                    figures
                        .iter()
                        .find(|figure| figure.source_bytes == origin.bytes)
                }) {
                    self.artwork.figure(ui, figure);
                } else {
                    render_node(ui, native, *entity, navigate);
                }
            }) {
                selected = Some(range);
            }
        }
        selected
    }
}

/// Default path: no directory discovery and no filesystem work on the UI thread.
fn collect_saved_resources(
    root: &Path,
    logical_path: &str,
    source: &str,
) -> Result<Vec<AssetFile>, String> {
    use std::{collections::BTreeSet, io::Read};
    if !crate::document_jobs::has_artwork_fence(source) {
        return Ok(Vec::new());
    }
    if !crate::pandoc::safe_path(logical_path) {
        return Err("Invalid note resource path.".into());
    }
    let references =
        crate::illustration::saved_image_references(source).map_err(|error| error.to_string())?;
    let parent = Path::new(logical_path)
        .parent()
        .ok_or("Invalid note resource parent.")?;
    let mut resources = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    for image in references.illustrations.values() {
        let relative = parent.join(&image.image);
        if !seen.insert(relative.clone()) {
            continue;
        }
        // Reject symlinks at every component, not only the final image file.
        let mut path = root.to_path_buf();
        for (index, component) in relative.components().enumerate() {
            let std::path::Component::Normal(component) = component else {
                return Err("Invalid saved image path.".into());
            };
            if index >= 12 {
                return Err("Resource directory nesting limit exceeded.".into());
            }
            path.push(component);
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("{}: {error}", relative.display()))?;
            if metadata.file_type().is_symlink() {
                return Err("Saved images cannot follow symlinks.".into());
            }
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("Saved image is not a regular file.".into());
        }
        if metadata.len() > 64 * 1024 * 1024 {
            return Err("Resource file quota exceeded.".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .map_err(|error| error.to_string())?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        total += bytes.len();
        if bytes.len() > 64 * 1024 * 1024 || total > 80 * 1024 * 1024 {
            return Err("Resource byte quota exceeded.".into());
        }
        resources.push(AssetFile {
            relative_path: relative
                .to_str()
                .ok_or("Resource path is not UTF-8")?
                .replace('\\', "/"),
            bytes,
        });
    }
    Ok(resources)
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

fn valid_range(source: &str, range: &Range<usize>) -> bool {
    range.start < range.end && source.get(range.clone()).is_some()
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
    fn saved_images_render_on_startup_without_settings_or_directory_discovery() {
        let root =
            std::env::temp_dir().join(format!("tanglewood-auto-artwork-{}", uuid::Uuid::new_v4()));
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/fixtures/mvp-2");
        for path in [
            "assets/fig-add-inputs/figure.svg",
            "assets/fig-choosing-a-result/strip.png",
        ] {
            let target = root.join("chapter").join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(fixture.join(path), target).unwrap();
        }
        // An unrelated symlink would reject a recursive directory snapshot;
        // automatic loading must not visit it at all.
        #[cfg(unix)]
        std::os::unix::fs::symlink(std::env::temp_dir(), root.join("unrelated")).unwrap();
        let mut app = crate::App::open(&root).unwrap();
        let crate::commands::CommandResult::Note(view) = app
            .execute(
                crate::commands::Command::CreateNote {
                    path: "chapter/source.md".into(),
                    text: include_str!("../../docs/fixtures/mvp-2/with-image-references.md").into(),
                },
                crate::commands::Caller::Local,
            )
            .unwrap()
        else {
            panic!("note")
        };
        let mut shell = crate::ui::TanglewoodUi::new(app);
        assert!(shell.processing.resource_root.is_empty());
        assert!(!shell.processing.applied.pandoc);
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !shell.processing.bindings_current(&view) {
            shell.processing.update(
                &ctx,
                &view,
                shell.app.parsed_document(view.note.id).unwrap(),
            );
            assert!(
                shell.processing.error_details().is_none(),
                "{:?}",
                shell.processing.error_details()
            );
            assert!(
                std::time::Instant::now() < deadline,
                "automatic processing timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let document = &shell.processing.result.as_ref().unwrap().2;
        assert_eq!(document.resources.len(), 2);
        assert_eq!(document.figures.len(), 2);
        assert_eq!(document.source, view.note.text);
        assert!(
            document
                .figures
                .iter()
                .all(|figure| document.figure_resource(figure).is_some())
        );
        #[cfg(unix)]
        {
            let assets = root.join("chapter/assets");
            std::fs::rename(&assets, root.join("held-assets")).unwrap();
            std::os::unix::fs::symlink(root.join("held-assets"), &assets).unwrap();
            assert!(
                collect_saved_resources(&root, &view.note.path, &view.note.text)
                    .unwrap_err()
                    .contains("symlink")
            );
            std::fs::remove_file(&assets).unwrap();
            std::fs::rename(root.join("held-assets"), &assets).unwrap();
        }
        std::fs::remove_file(root.join("chapter/assets/fig-add-inputs/figure.svg")).unwrap();
        assert!(collect_saved_resources(&root, &view.note.path, &view.note.text).is_err());
        assert!(
            collect_saved_resources(
                &root.join("absent"),
                "plain.md",
                "---\ntitle: Hello!\n---\nOrdinary note"
            )
            .unwrap()
            .is_empty()
        );
        shell.app.shutdown().unwrap();
        drop(shell);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inline_editor_keeps_neighbours_and_large_source_field_has_real_height() {
        use crate::ui::editor::{Draft, source_editor};
        let mut app = crate::App::new();
        let crate::commands::CommandResult::Note(view) = app.execute(
            crate::commands::Command::CreateNote {
                path: "context.md".into(),
                text: "# Before\n\nOriginal paragraph.\nSecond line.\nThird line.\nFourth line.\nFifth line.\n\n# After\n\n```{#art .illustration}\nA detailed illustration brief.\n```\n".into(),
            }, crate::commands::Caller::Local).unwrap() else { panic!("note") };
        let mut draft = Draft::new(&view, 0);
        let range = draft
            .context
            .node(draft.context.roots()[1])
            .unwrap()
            .origin
            .unwrap()
            .bytes
            .clone();
        assert!(draft.choose_range(Some(range.clone())));
        assert!(draft.inline_block().is_some());
        let context = draft.context.clone();
        let source = draft.base_text.clone();
        let ctx = egui::Context::default();
        let processing = Processing::default();
        let mut editor_rect = egui::Rect::NOTHING;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700.0, 900.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    processing.show_inline(ui, &view, &context, &source, &range, &mut |ui| {
                        let start = ui.cursor().min;
                        source_editor(ui, &mut draft, view.note.id, false, true, 400.0);
                        editor_rect = egui::Rect::from_min_max(start, ui.min_rect().max);
                    });
                });
            },
        );
        fn texts(shapes: &[egui::epaint::ClippedShape], found: &mut Vec<(String, f32)>) {
            fn visit(shape: &egui::Shape, found: &mut Vec<(String, f32)>) {
                match shape {
                    egui::Shape::Text(text) => {
                        found.push((text.galley.job.text.clone(), text.pos.y))
                    }
                    egui::Shape::Vec(shapes) => {
                        for shape in shapes {
                            visit(shape, found);
                        }
                    }
                    _ => {}
                }
            }
            for shape in shapes {
                visit(&shape.shape, found);
            }
        }
        let mut found = Vec::new();
        texts(&output.shapes, &mut found);
        let before = found.iter().find(|(text, _)| text == "Before").unwrap().1;
        let after = found.iter().find(|(text, _)| text == "After").unwrap().1;
        assert!(before < editor_rect.top() && editor_rect.bottom() <= after);
        assert!(editor_rect.height() >= 160.0 && editor_rect.height() < 240.0);
        assert_eq!(app.note(view.note.id).unwrap().note.text, view.note.text);
        // Short modal drafts fit their content with adjacent actions; long
        // drafts retain a substantial bounded viewport instead of collapsing.
        let range = context
            .node(*context.roots().last().unwrap())
            .unwrap()
            .origin
            .unwrap()
            .bytes
            .clone();
        assert!(draft.choose_range(Some(range)));
        assert!(draft.inline_block().is_none());
        for long in [false, true] {
            if long {
                draft.text = format!(
                    "```custom\n{}\n```",
                    "A detailed illustration brief.\n".repeat(80)
                );
            }
            let mut height = 0.0;
            let mut field = egui::Rect::NOTHING;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.allocate_ui(egui::vec2(650.0, 0.0), |ui| {
                        let start = ui.cursor().min.y;
                        source_editor(ui, &mut draft, view.note.id, false, false, 360.0);
                        height = ui.min_rect().bottom() - start;
                        field = ctx
                            .read_response(egui::Id::new((
                                "source",
                                view.note.id.0,
                                draft.widget_generation,
                            )))
                            .unwrap()
                            .interact_rect;
                        assert!(
                            ui.min_rect().bottom() < ctx.content_rect().bottom(),
                            "action footer outside viewport"
                        );
                        assert!(
                            field.bottom() + 34.0 <= ui.min_rect().bottom(),
                            "missing space for action footer"
                        );
                    });
                });
            });
            if long {
                assert!(
                    field.height() >= 350.0,
                    "long source field collapsed: {field:?}"
                );
                assert!(
                    height <= 430.0,
                    "long source editor exceeded bound: {height}"
                );
            } else {
                assert!(
                    (60.0..180.0).contains(&height),
                    "short source editor failed content fit: {height}"
                );
            }
        }
    }

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
        let document_context = app.parsed_document(view.note.id).unwrap();
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
        processing.update(&ctx, &view, document_context.clone());
        assert!(processing.error.is_none());
        assert!(
            processing.jobs.is_busy() || processing.staged.is_some() || processing.result.is_some()
        );
        assert!(matches!(
            processing.config(),
            Ok(ProcessingConfig::NativeIllustrations)
        ));
        // Applying the changed configuration invalidates an older capture, even at the same revision.
        processing.result = None;
        processing.applied = processing.pending_settings();
        processing.refresh = true;
        processing.loading = Some(capture());
        processing.update(&ctx, &view, document_context.clone());
        assert!(processing.refresh);
        assert!(processing.requested.is_none());
        assert!(processing.result.is_none());
        assert!(processing.config().is_err());
        let (fallback_source, fallback) = processing
            .fallback(&view)
            .expect("accepted original source remains readable before a successful process");
        assert_eq!(fallback_source, view.note.text);
        assert!(
            fallback
                .roots()
                .iter()
                .filter_map(|entity| fallback.node(*entity))
                .any(|block| matches!(block.kind, crate::markdown::BlockKind::Paragraph))
        );
        assert!(
            fallback
                .roots()
                .iter()
                .filter_map(|entity| fallback.node(*entity))
                .all(|block| block
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin.note == key.0
                        && origin.revision == key.1
                        && valid_range(fallback_source, &origin.bytes)))
        );
        assert!(processing.result.is_none());

        let document = ProcessedDocument {
            source: view.note.text.clone(),
            logical_path: view.note.path.clone(),
            resources: Vec::new(),
            semantic: None,
            figures: Vec::new(),
            metadata: Default::default(),
        };
        let mut retained = Processing {
            requested: Some(key),
            result: Some((key.0, key.1, document)),
            native: Some(document_context.clone()),
            ..Default::default()
        };
        assert!(retained.bindings_current(&view));
        retained.request_refresh();
        assert!(retained.result.is_some() && retained.native.is_some());
        assert!(!retained.bindings_current(&view));
        retained.update(&ctx, &view, document_context.clone());
        assert!(retained.result.is_some() && retained.native.is_some());
        assert!(!retained.bindings_current(&view));
        retained.error = Some("replacement processing failed".into());
        assert!(retained.result.is_some());
        assert!(!retained.bindings_current(&view));

        // This is valid saved PNG artwork inside the core's 32M-pixel envelope,
        // but beyond the deliberately smaller 4M preview decoding quota.
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 2100, 2000);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&vec![255; 2100 * 2000 * 3])
                .unwrap();
            writer.finish().unwrap();
        }
        let mut decoding = Processing {
            requested: Some(key),
            result: Some((
                key.0,
                key.1,
                ProcessedDocument {
                    source: view.note.text.clone(),
                    logical_path: "last-good.md".into(),
                    resources: Vec::new(),
                    semantic: None,
                    figures: Vec::new(),
                    metadata: Default::default(),
                },
            )),
            native: Some(document_context.clone()),
            ..Default::default()
        };
        let replacement = ProcessedDocument {
            source: view.note.text.clone(),
            logical_path: "new.md".into(),
            resources: vec![AssetFile {
                relative_path: "image.png".into(),
                bytes: png_bytes,
            }],
            semantic: None,
            metadata: serde_json::from_value(serde_json::json!({"illustrations":{"fig-new":{
                "image":"image.png","media-type":"image/png"
            }}}))
            .unwrap(),
            figures: vec![crate::illustration::PreparedFigure {
                id: "fig-new".into(),
                kind: crate::illustration::ArtworkKind::Comic,
                caption: "New caption".into(),
                description: "New artwork".into(),
                width_mm: None,
                source_bytes: 0..view.note.text.len(),
                asset_path: "image.png".into(),
                asset_sha256: String::new(),
                dimensions: [2100.0, 2000.0],
            }],
        };
        decoding.stage_document(&ctx, key, 1, replacement);
        assert!(!decoding.bindings_current(&view));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while decoding.staged.is_some() {
            decoding.finish_staged(&ctx, &view);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            decoding
                .error_details()
                .unwrap()
                .contains("preview pixel limit")
        );
        assert_eq!(
            decoding.result.as_ref().unwrap().2.logical_path,
            "last-good.md"
        );
        assert!(decoding.native.is_some());
        assert!(decoding.artwork.error().is_none()); // Failed staging never replaced the live cache.
        assert!(!decoding.bindings_current(&view));
    }
}
