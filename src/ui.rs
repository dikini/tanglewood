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
    #[cfg(all(target_os = "linux", feature = "codex"))]
    conversation: Option<ConversationPanel>,
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
            #[cfg(all(target_os = "linux", feature = "codex"))]
            conversation: None,
        }
    }

    #[cfg(all(target_os = "linux", feature = "mcp"))]
    pub fn with_mcp(mut self, socket: crate::mcp::OwnerSocket) -> Self {
        self.mcp = Some(socket);
        self
    }

    #[cfg(all(target_os = "linux", feature = "codex"))]
    pub fn with_codex(mut self, config: crate::codex::CodexConfig) -> Self {
        self.conversation = Some(ConversationPanel::new(config));
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
        #[cfg(all(target_os = "linux", feature = "codex"))]
        if let Some(panel) = &mut self.conversation {
            panel.poll(ctx);
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
                #[cfg(all(target_os = "linux", feature = "codex"))]
                if let Some(panel) = &mut self.conversation {
                    ui.toggle_value(&mut panel.visible, "Codex");
                }
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
        #[cfg(all(target_os = "linux", feature = "codex"))]
        if let Some(panel) = &mut self.conversation {
            if panel.visible {
                egui::SidePanel::right("codex-conversation")
                    .resizable(true)
                    .default_width(360.0)
                    .show(ctx, |ui| panel.show(ui, ctx));
            }
        }
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
        #[allow(unused_mut)]
        let mut conversation_pending = false;
        #[cfg(all(target_os = "linux", feature = "codex"))]
        if self.closing {
            if let Some(panel) = &mut self.conversation {
                panel.disconnect();
                conversation_pending = panel.adapter.is_some();
                if conversation_pending {
                    ctx.request_repaint_after(std::time::Duration::from_millis(20));
                }
            }
        }
        if self.closing {
            if pending || local_drafts || conversation_pending {
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
                if conversation_pending {
                    status.push_str(" Waiting for Codex to disconnect.");
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

#[cfg(all(target_os = "linux", feature = "codex"))]
use crate::codex::{ApprovalDecision, CodexAdapter, CodexConfig, CodexEvent, CodexRequest};

#[cfg(all(target_os = "linux", feature = "codex"))]
struct ConversationMessage {
    role: &'static str,
    key: Option<(String, String, String)>,
    text: String,
}
#[cfg(all(target_os = "linux", feature = "codex"))]
struct ConversationApproval {
    id: serde_json::Value,
    thread: String,
    turn: String,
    method: String,
    details: String,
    decisions: Vec<ApprovalDecision>,
}
#[cfg(all(target_os = "linux", feature = "codex"))]
struct ConversationPanel {
    config: CodexConfig,
    adapter: Option<CodexAdapter>,
    visible: bool,
    ready: bool,
    starting: bool,
    active_turn: Option<String>,
    thread: Option<String>,
    disconnecting: bool,
    prompt: String,
    messages: Vec<ConversationMessage>,
    approvals: Vec<ConversationApproval>,
    status: String,
    error: Option<String>,
}
#[cfg(all(target_os = "linux", feature = "codex"))]
impl ConversationPanel {
    const BYTE_LIMIT: usize = 2 * 1024 * 1024;
    fn new(config: CodexConfig) -> Self {
        Self {
            config,
            adapter: None,
            visible: true,
            ready: false,
            starting: false,
            active_turn: None,
            thread: None,
            disconnecting: false,
            prompt: String::new(),
            messages: Vec::new(),
            approvals: Vec::new(),
            status: "Disconnected".into(),
            error: None,
        }
    }
    fn busy(&self) -> bool {
        self.starting || self.active_turn.is_some()
    }
    fn bytes(&self) -> usize {
        self.messages
            .iter()
            .map(|message| {
                message.text.len()
                    + message.key.as_ref().map_or(0, |(thread, turn, item)| {
                        thread.len() + turn.len() + item.len()
                    })
            })
            .sum::<usize>()
            + self
                .approvals
                .iter()
                .map(|approval| {
                    approval.details.len()
                        + approval.thread.len()
                        + approval.turn.len()
                        + approval.method.len()
                        + approval.id.to_string().len()
                })
                .sum::<usize>()
    }
    fn disconnect(&mut self) {
        if self.disconnecting {
            return;
        }
        if let Some(adapter) = &self.adapter {
            match adapter.try_send(CodexRequest::Disconnect) {
                Ok(()) => {
                    self.disconnecting = true;
                    self.status = "Disconnecting…".into();
                }
                Err(error) => self.error = Some(error),
            }
        }
    }
    fn limit_exceeded(&mut self) {
        self.error=Some("Conversation display limit reached. Disconnecting; additional output is not displayed. Clear the conversation before reconnecting.".into());
        self.disconnect();
    }
    fn message(&mut self, key: (String, String, String), text: String, replace: bool) {
        let existing = self
            .messages
            .iter()
            .position(|message| message.key.as_ref() == Some(&key));
        let previous = existing.map_or(0, |index| {
            if replace {
                self.messages[index].text.len()
            } else {
                0
            }
        });
        if self.bytes().saturating_sub(previous)
            + text.len()
            + key.0.len()
            + key.1.len()
            + key.2.len()
            > Self::BYTE_LIMIT
            || (existing.is_none() && self.messages.len() >= 256)
        {
            self.limit_exceeded();
            return;
        }
        if let Some(index) = existing {
            if replace {
                self.messages[index].text = text;
            } else {
                self.messages[index].text.push_str(&text);
            }
        } else {
            self.messages.push(ConversationMessage {
                role: "Codex",
                key: Some(key),
                text,
            });
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        for _ in 0..64 {
            let Some(event) = self.adapter.as_ref().and_then(CodexAdapter::try_recv) else {
                return;
            };
            match event {
                CodexEvent::Connecting => self.status = "Connecting…".into(),
                CodexEvent::Ready { thread } => {
                    self.thread = Some(thread.clone());
                    self.config.resume_thread = Some(thread);
                    self.ready = true;
                    self.status = "Ready".into();
                }
                CodexEvent::TurnStarted { thread, turn } => {
                    self.thread = Some(thread);
                    self.starting = false;
                    self.active_turn = Some(turn);
                    self.status = "Streaming…".into();
                }
                CodexEvent::Delta {
                    thread,
                    turn,
                    item,
                    text,
                } => self.message((thread, turn, item), text, false),
                CodexEvent::MessageComplete {
                    thread,
                    turn,
                    item,
                    text,
                } => self.message((thread, turn, item), text, true),
                CodexEvent::TurnCompleted {
                    thread,
                    turn,
                    status,
                } => {
                    if self.thread.as_ref() == Some(&thread)
                        && self.active_turn.as_ref() == Some(&turn)
                    {
                        self.starting = false;
                        self.active_turn = None;
                        self.approvals.clear();
                        self.status = format!("Turn {status}");
                    }
                }
                CodexEvent::Approval {
                    id,
                    thread,
                    turn,
                    method,
                    details,
                    decisions,
                } => {
                    let details = details.to_string();
                    if self.bytes()
                        + details.len()
                        + thread.len()
                        + turn.len()
                        + method.len()
                        + id.to_string().len()
                        > Self::BYTE_LIMIT
                        || self.approvals.len() >= 16
                    {
                        self.limit_exceeded();
                    } else {
                        self.approvals.push(ConversationApproval {
                            id,
                            thread,
                            turn,
                            method,
                            details,
                            decisions,
                        });
                    }
                }
                CodexEvent::ApprovalResolved { id, thread } => self
                    .approvals
                    .retain(|approval| approval.id != id || approval.thread != thread),
                CodexEvent::Error(error) => {
                    self.error = Some(error);
                    if self.active_turn.is_none() {
                        self.starting = false;
                    }
                }
                CodexEvent::Disconnected(error) => {
                    // The worker reports this only after its child has been killed/reaped.
                    self.adapter = None;
                    self.ready = false;
                    self.starting = false;
                    self.active_turn = None;
                    self.approvals.clear();
                    self.disconnecting = false;
                    self.status = "Disconnected".into();
                    self.error = Some(match self.error.take() {
                        Some(previous) if previous.starts_with("Conversation display limit") => {
                            format!("{previous}\n{error}")
                        }
                        _ => error,
                    });
                    return;
                }
            }
        }
        ctx.request_repaint();
    }
    fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Codex");
        ui.label(&self.status);
        if let Some(thread) = &self.thread {
            ui.label("Thread (resume ID)");
            ui.add(egui::Label::new(thread).selectable(true).wrap());
        }
        ui.horizontal_wrapped(|ui| {
            if self.adapter.is_none()
                && ui
                    .button(if self.config.resume_thread.is_some() {
                        "Reconnect / resume"
                    } else {
                        "Connect"
                    })
                    .clicked()
            {
                if self.bytes() >= Self::BYTE_LIMIT {
                    self.error = Some("Clear the conversation before connecting.".into());
                } else {
                    let context = ctx.clone();
                    match CodexAdapter::spawn(self.config.clone(), move || {
                        context.request_repaint()
                    }) {
                        Ok(adapter) => {
                            self.adapter = Some(adapter);
                            self.status = "Connecting…".into();
                            self.ready = false;
                            self.disconnecting = false;
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            }
            if self.adapter.is_some()
                && ui
                    .add_enabled(!self.disconnecting, egui::Button::new("Disconnect"))
                    .clicked()
            {
                self.disconnect();
            }
            if !self.busy()
                && self.approvals.is_empty()
                && ui.button("Clear displayed messages").clicked()
            {
                self.messages.clear();
                self.error = None;
            }
            if self.adapter.is_none() && ui.button("New thread").clicked() {
                self.config.resume_thread = None;
                self.thread = None;
                self.messages.clear();
                self.error = None;
                self.status = "Disconnected — new thread on Connect".into();
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        egui::ScrollArea::vertical()
            .id_salt("codex-transcript")
            .stick_to_bottom(true)
            .max_height((ui.available_height() - 230.0).max(100.0))
            .show(ui, |ui| {
                for message in &self.messages {
                    ui.strong(message.role);
                    ui.add(egui::Label::new(&message.text).selectable(true).wrap());
                    ui.separator();
                }
            });
        let mut answer = None;
        for approval in &self.approvals {
            ui.group(|ui| {
                ui.strong("Approval required");
                ui.label(&approval.method);
                egui::ScrollArea::vertical()
                    .id_salt(("approval", approval.id.to_string()))
                    .max_height(150.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&approval.details).monospace())
                                .selectable(true)
                                .wrap(),
                        );
                    });
                ui.horizontal_wrapped(|ui| {
                    for decision in &approval.decisions {
                        let label = match decision {
                            ApprovalDecision::Accept => "Allow once",
                            ApprovalDecision::Decline => "Decline",
                            ApprovalDecision::Cancel
                                if approval.method == "mcpServer/elicitation/request" =>
                            {
                                "Cancel request"
                            }
                            ApprovalDecision::Cancel => "Cancel turn",
                        };
                        if ui
                            .add_enabled(!self.disconnecting, egui::Button::new(label))
                            .clicked()
                        {
                            answer = Some((approval.id.clone(), *decision));
                        }
                    }
                });
            });
        }
        if let Some((id, decision)) = answer {
            if let Some(adapter) = &self.adapter {
                match adapter.try_send(CodexRequest::AnswerApproval {
                    id: id.clone(),
                    decision,
                }) {
                    Ok(()) => self.approvals.retain(|approval| approval.id != id),
                    Err(error) => self.error = Some(error),
                }
            }
        }
        ui.separator();
        ui.add(
            egui::TextEdit::multiline(&mut self.prompt)
                .hint_text("Message Codex")
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .char_limit(256 * 1024),
        );
        if self.prompt.len() > 256 * 1024 {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Prompt exceeds 256 KiB; shorten it before sending.",
            );
        }
        ui.horizontal_wrapped(|ui| {
            let can_send = self.ready
                && !self.busy()
                && !self.disconnecting
                && !self.prompt.trim().is_empty()
                && self.prompt.len() <= 256 * 1024;
            if ui
                .add_enabled(can_send, egui::Button::new("Send"))
                .clicked()
            {
                if self.bytes() + self.prompt.len() > Self::BYTE_LIMIT || self.messages.len() >= 256
                {
                    self.limit_exceeded();
                } else if let Some(adapter) = &self.adapter {
                    match adapter.try_send(CodexRequest::SendPrompt(self.prompt.clone())) {
                        Ok(()) => {
                            self.messages.push(ConversationMessage {
                                role: "You",
                                key: None,
                                text: std::mem::take(&mut self.prompt),
                            });
                            self.starting = true;
                            self.status = "Starting turn…".into();
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
            }
            if ui
                .add_enabled(
                    self.busy() && !self.disconnecting,
                    egui::Button::new("Interrupt"),
                )
                .clicked()
            {
                if let Some(adapter) = &self.adapter {
                    match adapter.try_send(CodexRequest::Interrupt) {
                        Ok(()) => self.status = "Interrupt requested…".into(),
                        Err(error) => self.error = Some(error),
                    }
                }
            }
        });
    }
}
