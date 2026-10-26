//! Direct-style editor. Drafts and widget undo identity stay outside the ECS.
use std::collections::HashMap;

use eframe::egui;

use crate::{
    App,
    commands::{Caller, Command, CommandResult},
    markdown::{self, BlockKind, Document, Inline, RenderBlock},
    notes::{NoteId, NoteView, Revision, TextEdit},
};

struct Draft {
    text: String,
    base_text: String,
    revision: Revision,
    widget_generation: u64,
    conflict: bool,
    composing: bool,
}

pub struct TanglewoodUi {
    pub app: App,
    selected: Option<NoteId>,
    drafts: HashMap<NoteId, Draft>,
    parsed: HashMap<NoteId, (Revision, Document)>,
    query: String,
    new_path: String,
    rename_path: String,
    show_trash: bool,
    show_preview: bool,
    error: Option<String>,
    show_notes: bool,
    closing: bool,
    #[cfg(all(target_os = "linux", feature = "mcp"))]
    mcp: Option<crate::mcp::OwnerSocket>,
}

impl TanglewoodUi {
    pub fn new(app: App) -> Self {
        Self {
            app,
            selected: None,
            drafts: HashMap::new(),
            parsed: HashMap::new(),
            query: String::new(),
            new_path: "Untitled.md".into(),
            rename_path: String::new(),
            show_trash: false,
            show_preview: false,
            error: None,
            show_notes: true,
            closing: false,
            #[cfg(all(target_os = "linux", feature = "mcp"))]
            mcp: None,
        }
    }

    #[cfg(all(target_os = "linux", feature = "mcp"))]
    pub fn with_mcp(mut self, socket: crate::mcp::OwnerSocket) -> Self {
        self.mcp = Some(socket);
        self
    }

    fn select(&mut self, id: NoteId) {
        if self.selected != Some(id) {
            if let Some(previous) = self.selected.and_then(|old| self.drafts.get_mut(&old)) {
                if previous.composing {
                    // Focus changed mid-composition: retain the text for explicit recovery.
                    previous.composing = false;
                    previous.conflict = true;
                }
            }
        }
        self.selected = Some(id);
        if let Ok(view) = self.app.note(id) {
            self.rename_path = view.note.path;
        }
    }

    fn adopt(&mut self, view: &NoteView) {
        let generation = self
            .drafts
            .get(&view.note.id)
            .map_or(0, |draft| draft.widget_generation + 1);
        self.drafts.insert(
            view.note.id,
            Draft {
                text: view.note.text.clone(),
                base_text: view.note.text.clone(),
                revision: view.note.revision,
                widget_generation: generation,
                conflict: false,
                composing: false,
            },
        );
    }

    fn dispatch(&mut self, command: Command) {
        match self.app.execute(command, Caller::Local) {
            Ok(CommandResult::Note(view)) => {
                self.error = None;
                if let Some(draft) = self.drafts.get_mut(&view.note.id) {
                    // Accepted text is now the draft base; persistence remains independently visible.
                    if draft.text == view.note.text {
                        draft.base_text = view.note.text.clone();
                        draft.revision = view.note.revision;
                        draft.conflict = false;
                    }
                }
                self.select(view.note.id);
            }
            Ok(CommandResult::Notes(_)) => {}
            Ok(CommandResult::Accepted { .. }) => {
                self.error = Some("Operation queued; waiting for completion.".into());
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn refresh(&mut self, notes: &[NoteView]) {
        for view in notes {
            if self
                .parsed
                .get(&view.note.id)
                .is_none_or(|(revision, _)| *revision != view.note.revision)
            {
                self.parsed.insert(
                    view.note.id,
                    (
                        view.note.revision,
                        markdown::read(view.note.id, view.note.revision, &view.note.text),
                    ),
                );
            }
            match self.drafts.get_mut(&view.note.id) {
                Some(draft) if draft.revision != view.note.revision => {
                    if draft.text == draft.base_text {
                        self.adopt(view);
                    } else {
                        draft.conflict = true;
                    }
                }
                None => self.adopt(view),
                _ => {}
            }
        }
    }
}

impl eframe::App for TanglewoodUi {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.tick();
        #[cfg(all(target_os = "linux", feature = "mcp"))]
        if let Some(socket) = &mut self.mcp {
            let context = ctx.clone();
            socket.set_waker(move || context.request_repaint());
            socket.poll(&mut self.app);
            if socket.has_pending() {
                ctx.request_repaint_after(std::time::Duration::from_millis(20));
            }
        }
        let notes = self.app.notes();
        self.refresh(&notes);
        if self.selected.is_none() {
            if let Some(view) = notes.iter().find(|view| !view.note.trashed) {
                self.select(view.note.id);
            }
        }
        let mut command = None;
        let mut select = None;
        let mut adopt = None;
        let mut retry = false;
        let mut navigate = None;
        if ctx.input(|input| input.viewport().close_requested()) {
            self.closing = true;
        }
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::P)) {
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("quick-switch")));
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Tanglewood");
                ui.toggle_value(&mut self.show_notes, "Notes");
                ui.checkbox(&mut self.show_preview, "Preview");
                if ui.button("Save now").clicked() {
                    retry = true;
                }
                if ui.button("Retry saves").clicked() {
                    retry = true;
                }
            });
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
        });
        if self.show_notes {
            egui::SidePanel::left("notes")
                .resizable(true)
                .default_width(230.0)
                .show(ctx, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id(egui::Id::new("quick-switch"))
                            .hint_text("Search / quick switch (Ctrl+P)"),
                    );
                    ui.checkbox(&mut self.show_trash, "Show trash");
                    let needle = self.query.to_lowercase();
                    egui::ScrollArea::vertical()
                        .max_height(ui.available_height() - 90.0)
                        .show(ui, |ui| {
                            for view in &notes {
                                if (!self.show_trash && view.note.trashed)
                                    || (!view.note.path.to_lowercase().contains(&needle)
                                        && !view.note.text.to_lowercase().contains(&needle))
                                {
                                    continue;
                                }
                                let label = format!(
                                    "{}{}",
                                    if view.note.trashed { "[Trash] " } else { "" },
                                    view.note.path
                                );
                                if ui
                                    .selectable_label(self.selected == Some(view.note.id), label)
                                    .clicked()
                                {
                                    select = Some(view.note.id);
                                }
                            }
                        });
                    ui.separator();
                    ui.text_edit_singleline(&mut self.new_path);
                    if ui.button("Create note").clicked() {
                        command = Some(Command::CreateNote {
                            path: self.new_path.clone(),
                            text: String::new(),
                        });
                    }
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
          egui::ScrollArea::vertical().id_salt("editor-scroll").show(ui, |ui| {
            let Some(view) = notes.iter().find(|view| Some(view.note.id) == self.selected) else {
                ui.label("Create or select a Markdown note."); return;
            };
            ui.horizontal_wrapped(|ui| {
                ui.text_edit_singleline(&mut self.rename_path);
                if ui.button("Rename").clicked() {
                    command = Some(Command::RenameNote { id: view.note.id, expected: view.note.revision, path: self.rename_path.clone() });
                }
                if view.note.trashed {
                    if ui.button("Restore").clicked() { command = Some(Command::RestoreNote { id: view.note.id, expected: view.note.revision }); }
                } else if ui.button("Move to trash").clicked() {
                    command = Some(Command::TrashNote { id: view.note.id, expected: view.note.revision });
                }
            });
            if let Some(error) = &view.save.error { ui.colored_label(egui::Color32::LIGHT_RED, format!("Save failed: {error}")); }
            else if view.save.persisted == Some(view.save.accepted) { ui.label("Saved"); }
            else { ui.label("Unsaved — waiting for storage"); }
            if self.app.mirror_enabled() {
                let export = self.app.export_status(view.note.id);
                if let Some(error) = export.error { ui.colored_label(egui::Color32::LIGHT_RED, format!("Markdown export failed: {error}")); }
                else if export.pending { ui.label("Markdown export pending"); }
                else { ui.label("Markdown mirror up to date"); }
            }
            for conflict in self.app.mirror_conflicts().iter().filter(|conflict| conflict.note.id == view.note.id) {
                ui.colored_label(egui::Color32::YELLOW, format!("Retained mirror conflict: {} ({})", conflict.path, conflict.kind));
            }
            let draft = self.drafts.get_mut(&view.note.id).expect("refresh creates draft");
            if draft.conflict {
                ui.colored_label(egui::Color32::YELLOW, "Note changed elsewhere. Your draft is retained; copy it before adopting the stored version.");
            }
            if draft.conflict || draft.text != draft.base_text {
                if ui.button("Discard local draft and adopt current note").clicked() { adopt = Some(view.clone()); }
            }
            if draft.text != draft.base_text { ui.label("Local draft has changes not accepted by the application."); }
            let response = ui.add_enabled(!view.note.trashed,
                egui::TextEdit::multiline(&mut draft.text)
                    .id(egui::Id::new(("source", view.note.id.0, draft.widget_generation)))
                    .desired_width(f32::INFINITY).desired_rows(16));
            // Composition stays in the widget draft until the IME commits.
            let mut ime_finished = false;
            ctx.input(|input| {
                for event in &input.events {
                    match event {
                        egui::Event::Ime(egui::ImeEvent::Preedit(text)) => draft.composing = !text.is_empty(),
                        egui::Event::Ime(egui::ImeEvent::Commit(_)) | egui::Event::Ime(egui::ImeEvent::Disabled) => {
                            draft.composing = false; ime_finished = true;
                        }
                        _ => {}
                    }
                }
            });
            if (response.changed() || ime_finished) && draft.text != draft.base_text && !draft.conflict && !draft.composing && command.is_none() {
                command = Some(Command::EditNote { id: view.note.id, expected: draft.revision,
                    edits: vec![contiguous_edit(&draft.base_text, &draft.text)] });
            }
            ui.separator();
            ui.label("Backlinks");
            for other in &notes {
                if other.note.trashed || other.note.id == view.note.id { continue; }
                if self.parsed.get(&other.note.id).is_some_and(|(_, document)| document.links.iter().any(|target|
                    resolve_note_target(&notes, &other.note.path, target) == Some(view.note.id)))
                    && ui.link(&other.note.path).clicked() { select = Some(other.note.id); }
            }
            if self.show_preview {
                ui.separator(); ui.heading("Preview");
                if let Some((_, document)) = self.parsed.get(&view.note.id) {
                    egui::ScrollArea::vertical().id_salt("preview").show(ui, |ui| {
                        for block in &document.blocks { render_block(ui, block, &mut navigate); }
                    });
                }
            }
          });
        });
        let navigation_source = notes
            .iter()
            .find(|view| Some(view.note.id) == self.selected)
            .map(|view| view.note.path.clone());
        if let Some(view) = adopt {
            self.adopt(&view);
        }
        if let Some(command) = command {
            self.dispatch(command);
        }
        if let Some(id) = select {
            self.select(id);
        }
        if retry {
            self.app.retry_saves();
        }
        if let Some(target) = navigate {
            if let Some(id) = navigation_source
                .as_deref()
                .and_then(|source| resolve_note_target(&notes, source, &target))
            {
                self.select(id);
            } else {
                self.error = Some(format!("Note link not found: {target}"));
            }
        }
        let current = self.app.notes();
        let pending = current
            .iter()
            .any(|view| view.save.persisted != Some(view.save.accepted));
        let local_drafts = self
            .drafts
            .values()
            .any(|draft| draft.text != draft.base_text || draft.composing);
        if self.closing {
            if pending || local_drafts {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                let failures: Vec<_> = current
                    .iter()
                    .filter_map(|view| {
                        view.save
                            .error
                            .as_ref()
                            .map(|error| format!("{}: {error}", view.note.path))
                    })
                    .collect();
                let mut status = if failures.is_empty() {
                    "Close paused: waiting for pending saves.".to_owned()
                } else {
                    format!(
                        "Close paused: save failed for {}. Use Retry saves.",
                        failures.join("; ")
                    )
                };
                if local_drafts {
                    status.push_str(" A local draft is not accepted; preserve it or adopt the current note first.");
                }
                self.error = Some(status);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if pending
            || current
                .iter()
                .any(|view| self.app.export_status(view.note.id).pending)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn render_block(ui: &mut egui::Ui, block: &RenderBlock, navigate: &mut Option<String>) {
    match &block.kind {
        BlockKind::Fence(fence) => {
            ui.monospace(&fence.body);
        }
        BlockKind::Source(source) => {
            ui.monospace(source);
        }
        BlockKind::Paragraph | BlockKind::Heading(_) => {
            ui.horizontal_wrapped(|ui| {
                let mut emphasis = false;
                let mut strong = matches!(block.kind, BlockKind::Heading(_));
                for inline in &block.inlines {
                    match inline {
                        Inline::Text(text) => {
                            let mut text = egui::RichText::new(text);
                            if emphasis {
                                text = text.italics();
                            }
                            if strong {
                                text = text.strong();
                            }
                            ui.label(text);
                        }
                        Inline::EmphasisStart => emphasis = true,
                        Inline::EmphasisEnd => emphasis = false,
                        Inline::StrongStart => strong = true,
                        Inline::StrongEnd => strong = matches!(block.kind, BlockKind::Heading(_)),
                        Inline::Code(text) => {
                            ui.monospace(text);
                        }
                        Inline::Link {
                            target,
                            label,
                            wiki,
                        } => {
                            if *wiki {
                                if ui.link(label).clicked() {
                                    *navigate = Some(target.clone());
                                }
                            } else {
                                if target.contains("://") || target.starts_with("mailto:") {
                                    ui.hyperlink_to(label, target);
                                } else if ui.link(label).clicked() {
                                    *navigate = Some(target.clone());
                                }
                            }
                        }
                        Inline::Break => {
                            ui.end_row();
                        }
                    }
                }
            });
        }
        BlockKind::Quote => {
            ui.group(|ui| {
                for child in &block.children {
                    render_block(ui, child, navigate);
                }
            });
        }
        BlockKind::Div => {
            for child in &block.children {
                render_block(ui, child, navigate);
            }
        }
    }
}

/// Both wiki and Markdown note targets resolve relative to the referring note.
/// A leading slash means vault root; anchors select the note but do not scroll yet.
fn linked_path(source: &str, target: &str) -> Option<String> {
    if target.contains(':') {
        return None;
    }
    let target = target.split('#').next()?;
    if target.is_empty() {
        return Some(source.to_owned());
    }
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        source
            .rsplit_once('/')
            .map_or(Vec::new(), |(parent, _)| parent.split('/').collect())
    };
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

fn resolve_note_target(notes: &[NoteView], source: &str, target: &str) -> Option<NoteId> {
    let path = linked_path(source, target)?;
    notes
        .iter()
        .find(|view| {
            !view.note.trashed
                && (view.note.path == path
                    || view.note.path.strip_suffix(".md") == Some(path.as_str()))
        })
        .map(|view| view.note.id)
}

/// Small contiguous edit with byte offsets that remain UTF-8 boundaries.
fn contiguous_edit(before: &str, after: &str) -> TextEdit {
    let prefix = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let suffix = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    TextEdit {
        start: prefix,
        end: before.len() - suffix,
        replacement: after[prefix..after.len() - suffix].to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        App, Caller, Command, CommandResult, contiguous_edit, linked_path, resolve_note_target,
    };

    #[test]
    fn contiguous_edit_preserves_unicode_boundaries_and_unchanged_suffix() {
        for (before, after) in [
            ("aé👩z", "aê👩z"),
            ("α尾", "α🙂尾"),
            ("α🙂尾", "α尾"),
            ("same", "same"),
        ] {
            let edit = contiguous_edit(before, after);
            assert!(before.is_char_boundary(edit.start) && before.is_char_boundary(edit.end));
            let mut text = before.to_owned();
            text.replace_range(edit.start..edit.end, &edit.replacement);
            assert_eq!(text, after);
        }
        assert_eq!(linked_path("A.md", "B#part").as_deref(), Some("B"));
        assert_eq!(
            linked_path("folder/A.md", "B.md#part").as_deref(),
            Some("folder/B.md")
        );
        assert_eq!(linked_path("folder/A.md", "/B#part").as_deref(), Some("B"));
        assert_eq!(
            linked_path("folder/A.md", "../B#part").as_deref(),
            Some("B")
        );
        assert_eq!(linked_path("A.md", "../escape"), None);
        let mut app = App::new();
        let CommandResult::Note(target) = app
            .execute(
                Command::CreateNote {
                    path: "B.md".into(),
                    text: String::new(),
                },
                Caller::Local,
            )
            .unwrap()
        else {
            panic!("expected note");
        };
        assert_eq!(
            resolve_note_target(&app.notes(), "A.md", "B#part"),
            Some(target.note.id)
        );
        let edit = contiguous_edit("aé👩z", "aê👩z");
        assert_eq!(
            (edit.start, edit.end, edit.replacement.as_str()),
            (1, 3, "ê")
        );
    }
}
