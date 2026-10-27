//! Direct-style editor. Drafts and widget undo identity stay outside the ECS.
use std::collections::HashMap;

use eframe::egui;

mod code;
#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
mod codex_context;
#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
mod codex_markdown;
mod document;
mod editor;
#[cfg(all(target_os = "linux", feature = "kimi"))]
mod kimi_panel;
mod layout;
mod navigation;
mod processing;
mod research;
mod theme;
use editor::Draft;
#[cfg(test)]
use editor::contiguous_edit;

use crate::{
    App,
    commands::{Caller, Command, CommandResult},
    document_world::DocumentWorld,
    notes::{NoteId, NoteView, Revision},
};

fn name_field(response: egui::Response, _text: &str, label: &str) -> egui::Response {
    #[cfg(not(target_os = "android"))]
    response
        .ctx
        .accesskit_node_builder(response.id, |node| node.set_label(label));
    #[cfg(target_os = "android")]
    let _ = label;
    response
}

fn field_single(ui: &mut egui::Ui, text: &mut String, name: &str) -> egui::Response {
    let label = match name {
        "quick-switch" => "Search notes",
        "document-import-path" | "markdown-import-path" => "Destination Markdown path",
        "markdown-file-path" => "Markdown file path",
        "research-title" | "research-new-title" => "Workspace title",
        "research-note-path" | "research-report-path" => "Research note path",
        "research-import-path" => "Research import file path",
        "lan-bind" => "LAN listen address",
        "lan-advertised" => "LAN advertised address",
        "lan-fingerprint" => "Peer fingerprint",
        _ => "Markdown path",
    };
    #[cfg(target_os = "android")]
    {
        let id = egui::Id::new(("android-field", name));
        let change = crate::android_input::before(ui.ctx(), id, text);
        let output = egui::TextEdit::singleline(text).id(id).show(ui);
        crate::android_input::after(ui.ctx(), &output, text);
        let mut response = output.response;
        if change.changed {
            response.mark_changed();
        }
        name_field(response, text, label)
    }
    #[cfg(not(target_os = "android"))]
    {
        let response = ui.text_edit_singleline(text);
        name_field(response, text, label)
    }
}
#[cfg(feature = "lan")]
fn field_multi(ui: &mut egui::Ui, text: &mut String, name: &str) -> egui::Response {
    #[cfg(target_os = "android")]
    {
        let id = egui::Id::new(("android-field", name));
        let change = crate::android_input::before(ui.ctx(), id, text);
        let output = egui::TextEdit::multiline(text).id(id).show(ui);
        crate::android_input::after(ui.ctx(), &output, text);
        let mut response = output.response;
        if change.changed {
            response.mark_changed();
        }
        response
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = name;
        ui.text_edit_multiline(text)
    }
}

/// The local user owns this permission; agent execution access is independent.
fn agent_permission_controls(ui: &mut egui::Ui, app: &App, enabled: bool) -> Option<bool> {
    let mut requested = None;
    let mut value = enabled;
    let error = app.automation_permission_error();
    ui.strong("Vault permission");
    if ui.add_enabled(
        !app.automation_permission_pending() || error.is_some(),
        egui::Checkbox::new(&mut value, "Allow agents to update notes and research"),
    ).on_hover_text("Applies to all connected tools for this vault. Process and network access are separate.").changed() {
        requested = Some(value);
    }
    if let Some(error) = error {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Updates are disabled. Permission could not be saved: {error}"),
        );
        if ui
            .button("Keep updates disabled and retry saving")
            .clicked()
        {
            requested = Some(false);
        }
    } else if app.automation_permission_pending() {
        ui.weak("Saving permission… Updates remain disabled until saved.");
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    requested
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarView {
    Notes,
    Search,
    Backlinks,
    Research,
    Codex,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    Processing,
    Connections,
    Files,
}
#[derive(Clone, Copy)]
enum NoteDialog {
    New,
    Rename { id: NoteId, expected: Revision },
}
fn note_title(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name)
}

pub struct TanglewoodUi {
    pub app: App,
    selected: Option<NoteId>,
    drafts: HashMap<NoteId, Draft>,
    parsed: HashMap<NoteId, std::sync::Arc<DocumentWorld>>,
    query: String,
    new_path: String,
    rename_path: String,
    show_trash: bool,
    show_preview: bool,
    error: Option<String>,
    show_notes: bool,
    closing: bool,
    theme_installed: bool,
    show_settings: bool,
    settings_page: SettingsPage,
    sidebar_view: SidebarView,
    note_dialog: Option<NoteDialog>,
    dialog_generation: u64,
    show_status: bool,
    markdown_path: String,
    io_request: Option<uuid::Uuid>,
    io_message: String,
    io_error: Option<String>,
    processing: processing::Processing,
    research: research::ResearchUi,
    research_chats_open: bool,
    research_outcomes_open: bool,
    layout: layout::Responsive,
    layout_plan: Option<layout::Plan>,
    touch_input: bool,
    right_pane: layout::Pane,
    #[cfg(feature = "lan")]
    lan_panel: LanPanel,
    #[cfg(target_os = "android")]
    picker: Option<crate::android_picker::AndroidPicker>,
    #[cfg(target_os = "android")]
    picker_status: String,
    #[cfg(all(target_os = "linux", feature = "mcp"))]
    mcp: Option<crate::mcp::OwnerSocket>,
    #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
    conversation: Option<ConversationPanel>,
}

impl TanglewoodUi {
    pub fn new(app: App) -> Self {
        let processing =
            processing::Processing::for_vault(app.vault_directory().map(|path| path.to_path_buf()));
        Self {
            app,
            selected: None,
            drafts: HashMap::new(),
            parsed: HashMap::new(),
            query: String::new(),
            new_path: "Untitled.md".into(),
            rename_path: String::new(),
            show_trash: false,
            show_preview: true,
            error: None,
            show_notes: !cfg!(target_os = "android"),
            closing: false,
            theme_installed: false,
            show_settings: false,
            settings_page: SettingsPage::Processing,
            sidebar_view: SidebarView::Notes,
            note_dialog: None,
            dialog_generation: 0,
            show_status: false,
            markdown_path: String::new(),
            io_request: None,
            io_message: String::new(),
            io_error: None,
            processing,
            research: research::ResearchUi::default(),
            research_chats_open: false,
            research_outcomes_open: true,
            layout: layout::Responsive::default(),
            layout_plan: None,
            touch_input: cfg!(target_os = "android"),
            right_pane: layout::Pane::Outcomes,
            #[cfg(feature = "lan")]
            lan_panel: LanPanel::default(),
            #[cfg(target_os = "android")]
            picker: None,
            #[cfg(target_os = "android")]
            picker_status: String::new(),
            #[cfg(all(target_os = "linux", feature = "mcp"))]
            mcp: None,
            #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
            conversation: None,
        }
    }

    #[cfg(all(target_os = "linux", feature = "mcp"))]
    pub fn with_mcp(mut self, socket: crate::mcp::OwnerSocket) -> Self {
        self.mcp = Some(socket);
        self
    }

    #[cfg(all(target_os = "linux", feature = "codex"))]
    pub fn with_codex(mut self, mut config: crate::codex::CodexConfig) -> Self {
        let binding = self.app.codex_project().cloned().unwrap_or_else(|| {
            crate::notes::CodexProjectBinding {
                project_id: None,
                name: self
                    .app
                    .vault_directory()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Notes".into()),
                creation_key: uuid::Uuid::new_v4().to_string(),
            }
        });
        if self.app.codex_project().is_none() {
            if let Err(error) = self.app.execute(
                Command::SetCodexProject {
                    expected: None,
                    project: binding.clone(),
                },
                Caller::Local,
            ) {
                self.error = Some(error.to_string());
            }
        }
        config.project = Some(binding);
        self.conversation = Some(ConversationPanel::new(config));
        self
    }

    #[cfg(target_os = "android")]
    pub fn with_android_picker(mut self, picker: crate::android_picker::AndroidPicker) -> Self {
        self.picker = Some(picker);
        self
    }

    #[cfg(target_os = "android")]
    fn poll_picker(&mut self, ctx: &egui::Context) {
        use crate::android_picker::PickerCompletion;
        let completion = self.picker.as_mut().and_then(|picker| picker.poll());
        if let Some(completion) = completion {
            self.picker_status = match completion {
                PickerCompletion::ImportReady {
                    request_id,
                    command,
                } => {
                    // Preserve selection and all drafts. refresh() will flag a changed
                    // canonical revision as a conflict when a local draft is dirty.
                    match self.app.execute(command, Caller::Local) {
                        Ok(CommandResult::Note(view)) => format!(
                            "Import {request_id} accepted: {} (revision {:?}); storage pending",
                            view.note.path, view.note.revision
                        ),
                        Ok(_) => format!("Import {request_id}: unexpected response"),
                        Err(error) => {
                            format!("Import {request_id} rejected; draft retained: {error}")
                        }
                    }
                }
                PickerCompletion::Exported {
                    request_id,
                    note,
                    revision,
                } => format!(
                    "Document export {request_id} complete: {note:?}, canonical revision {revision:?}. SQLite save status is separate."
                ),
                PickerCompletion::Cancelled { request_id } => {
                    format!("Document request {request_id} cancelled; source unchanged")
                }
                PickerCompletion::Failed { request_id, error } => {
                    format!("Document request {request_id} failed: {error}")
                }
            };
        }
        if self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.is_pending())
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    #[cfg(target_os = "android")]
    fn picker_controls(&mut self, ui: &mut egui::Ui) {
        let selected = self.selected.and_then(|id| self.app.note(id).ok());
        let Some(picker) = &mut self.picker else {
            return;
        };
        let pending = picker.is_pending();
        let mut result = None;
        ui.horizontal_wrapped(|ui| {
            ui.label("Import into new vault path:");
            field_single(ui, &mut self.new_path, "document-import-path");
            if ui
                .add_enabled(!pending, egui::Button::new("Choose document to import"))
                .clicked()
            {
                result = Some(picker.import_create(self.new_path.clone()));
            }
            if let Some(view) = selected.as_ref().filter(|view| !view.note.trashed) {
                if ui
                    .add_enabled(
                        !pending,
                        egui::Button::new("Replace selected from document"),
                    )
                    .clicked()
                {
                    result = Some(picker.import_replace(view));
                }
                if ui
                    .add_enabled(
                        !pending,
                        egui::Button::new("Export canonical selected revision"),
                    )
                    .clicked()
                {
                    result = Some(picker.export(view));
                }
                ui.label(format!(
                    "Canonical revision {:?}; local drafts are excluded from export",
                    view.note.revision
                ));
            }
            if pending && ui.button("Cancel document request").clicked() {
                self.picker_status = match picker.cancel() {
                    Ok(()) => "Cancellation requested".into(),
                    Err(error) => error,
                };
            }
        });
        if let Some(result) = result {
            self.picker_status = match result {
                Ok(id) => format!("Document request {id} pending"),
                Err(error) => format!("Cannot open document picker: {error}"),
            };
        }
        ui.label(&self.picker_status);
    }

    fn dialog_field_name(&self, dialog: NoteDialog) -> String {
        match dialog {
            NoteDialog::New => format!("note-dialog-{}-new", self.dialog_generation),
            NoteDialog::Rename { id, .. } => {
                format!("note-dialog-{}-rename-{}", self.dialog_generation, id.0)
            }
        }
    }
    fn close_note_dialog(&mut self) {
        if let Some(dialog) = self.note_dialog.take() {
            #[cfg(target_os = "android")]
            crate::android_input::retire_widget(
                egui::Id::new(("android-field", self.dialog_field_name(dialog))),
                true,
            );
            #[cfg(not(target_os = "android"))]
            let _ = dialog;
        }
    }
    fn open_note_dialog(&mut self, dialog: NoteDialog) {
        self.close_note_dialog();
        let Some(generation) = self.dialog_generation.checked_add(1) else {
            self.error = Some(
                "Dialog input generation exhausted; restart before opening another dialog.".into(),
            );
            return;
        };
        self.dialog_generation = generation;
        self.note_dialog = Some(dialog);
    }

    fn navigation_contents(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        notes: &[NoteView],
        overlay: bool,
        select: &mut Option<NoteId>,
    ) {
        let view = notes
            .iter()
            .find(|view| Some(view.note.id) == self.selected);
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        let codex_mode = self.sidebar_view == SidebarView::Codex && self.conversation.is_some();
        #[cfg(not(all(target_os = "linux", any(feature = "codex", feature = "kimi"))))]
        let codex_mode = false;
        ui.scope_builder(
            egui::UiBuilder::new().id("shell-navigation-content"),
            |ui| {
                #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
                if codex_mode {
                    if let Some(panel) = &mut self.conversation {
                        panel.sidebar(ui, ctx);
                    }
                    return;
                }
                if self.sidebar_view == SidebarView::Research {
                    if self.research.sidebar(ui, &mut self.app) && overlay {
                        self.layout.close();
                    }
                    return;
                }
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.strong("Tanglewood");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::button(ui, theme::Icon::New, "New note", true).clicked() {
                            self.open_note_dialog(NoteDialog::New);
                        }
                    });
                });
                ui.add_space(18.0);
                if self.sidebar_view == SidebarView::Search {
                    #[cfg(target_os = "android")]
                    field_single(ui, &mut self.query, "quick-switch");
                    #[cfg(not(target_os = "android"))]
                    let search = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id(egui::Id::new("quick-switch"))
                            .hint_text("Search notes…")
                            .desired_width(f32::INFINITY),
                    );
                    #[cfg(not(target_os = "android"))]
                    name_field(search, &self.query, "Search notes");
                    ui.add_space(8.0);
                }
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(match self.sidebar_view {
                        SidebarView::Codex => "Codex",
                        SidebarView::Research => "Research",
                        SidebarView::Backlinks => "Backlinks",
                        SidebarView::Search => "Search",
                        SidebarView::Notes => {
                            if self.show_trash {
                                "Trash"
                            } else {
                                "Notes"
                            }
                        }
                    })
                    .size(12.0)
                    .color(theme::muted(ui)),
                );
                let needle = if self.sidebar_view == SidebarView::Search {
                    self.query.to_lowercase()
                } else {
                    String::new()
                };
                let note_scroll = egui::ScrollArea::vertical()
                    .id_salt("note-list")
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - 32.0).max(1.0))
                    .show(ui, |ui| {
                        let mut count = 0;
                        for other in notes {
                            if (self.show_trash && !other.note.trashed)
                                || (!self.show_trash && other.note.trashed)
                            {
                                continue;
                            }
                            if !needle.is_empty()
                                && !other.note.path.to_lowercase().contains(&needle)
                                && !other.note.text.to_lowercase().contains(&needle)
                            {
                                continue;
                            }
                            if self.sidebar_view == SidebarView::Backlinks
                                && !view.is_some_and(|selected| {
                                    other.note.id != selected.note.id
                                        && self.parsed.get(&other.note.id).is_some_and(|doc| {
                                            doc.links().iter().any(|target| {
                                                resolve_note_target(
                                                    &notes,
                                                    &other.note.path,
                                                    target,
                                                ) == Some(selected.note.id)
                                            })
                                        })
                                })
                            {
                                continue;
                            }
                            count += 1;
                            let selected = self.selected == Some(other.note.id);
                            let row = ui
                                .add_sized(
                                    [ui.available_width(), theme::control_size(ui).max(36.0)],
                                    egui::Button::new("")
                                        .selected(selected)
                                        .frame(true)
                                        .frame_when_inactive(selected)
                                        .corner_radius(6),
                                )
                                .on_hover_text(&other.note.path);
                            row.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    &other.note.path,
                                )
                            });
                            let mut job = egui::text::LayoutJob::simple(
                                note_title(&other.note.path).to_owned(),
                                egui::FontId::proportional(14.0),
                                ui.visuals().text_color(),
                                (row.rect.width() - 22.0).max(1.0),
                            );
                            job.wrap.max_rows = 1;
                            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                            let pos = egui::pos2(
                                row.rect.left() + 11.0,
                                row.rect.center().y - galley.size().y * 0.5,
                            );
                            ui.painter().galley(pos, galley, ui.visuals().text_color());
                            if row.clicked() {
                                *select = Some(other.note.id);
                            }
                        }
                        if count == 0 {
                            ui.label(
                                egui::RichText::new(
                                    if self.sidebar_view == SidebarView::Backlinks {
                                        "No backlinks"
                                    } else {
                                        "No notes"
                                    },
                                )
                                .small()
                                .color(theme::muted(ui)),
                            );
                        }
                    });
                navigation::scroll_focus(ctx, &note_scroll);
                ui.label(
                    egui::RichText::new("Personal vault")
                        .small()
                        .color(theme::muted(ui)),
                );
            },
        );
    }

    fn pane_transition_allowed(&mut self) -> bool {
        if self
            .selected
            .and_then(|id| self.drafts.get(&id))
            .is_some_and(|d| d.composing)
        {
            return false;
        }
        self.selected.is_none_or(|id| self.prepare_transition(id))
    }

    fn toggle_navigation(&mut self) {
        self.sidebar(self.sidebar_view, true);
    }

    fn request_pane(&mut self, pane: layout::Pane) {
        if !self.pane_transition_allowed() {
            return;
        }
        if pane == layout::Pane::Navigation {
            self.toggle_navigation();
            return;
        }
        let docked = self.layout_plan.is_some_and(|plan| plan.right);
        if docked && pane.right() {
            if self.sidebar_view == SidebarView::Research {
                self.research_outcomes_open =
                    !self.research_outcomes_open || self.right_pane != pane;
                self.right_pane = pane;
            }
            #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
            if pane == layout::Pane::Context {
                if let Some(panel) = &mut self.conversation {
                    panel.context.open = true;
                }
            }
        } else {
            self.layout.toggle(pane);
        }
    }

    fn right_contents(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pane: layout::Pane,
        notes: &[NoteView],
    ) -> Option<research::Action> {
        let mut action = None;
        #[allow(unused_mut)]
        let mut context_closed = false;
        ui.scope_builder(
            egui::UiBuilder::new().id(if pane == layout::Pane::Outcomes {
                "shell-outcomes-content"
            } else {
                "shell-context-content"
            }),
            |ui| {
                if pane == layout::Pane::Outcomes {
                    action = self.research.outcomes(ui, ctx, &mut self.app, notes);
                }
                #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
                if pane == layout::Pane::Context {
                    if let Some(panel) = &mut self.conversation {
                        let ready = panel.ready
                            && panel.pending_thread.is_none()
                            && !panel.disconnecting
                            && panel.context_goal_pending.is_none();
                        let cwd = panel.provider_cwd();
                        panel.context.open = true;
                        let kimi = panel.is_kimi();
                        if let Some(action) = panel.context.panel(ui, ctx, &cwd, ready, kimi) {
                            panel.context_action(action);
                        }
                        context_closed = !panel.context.open;
                    }
                }
            },
        );
        if context_closed {
            if self.layout.overlay == Some(layout::Pane::Context) {
                self.layout.close();
            } else if self.sidebar_view == SidebarView::Research {
                self.research_outcomes_open = false;
            }
        }
        action
    }

    fn sidebar(&mut self, view: SidebarView, toggle: bool) {
        if self.layout_plan.is_some_and(|plan| !plan.left) {
            if !self.pane_transition_allowed() {
                return;
            }
            let same = self.sidebar_view == view;
            self.sidebar_view = view;
            if toggle && same {
                self.layout.toggle(layout::Pane::Navigation);
            } else {
                self.layout.open(layout::Pane::Navigation);
            }
            return;
        }
        if toggle && self.show_notes && self.sidebar_view == view {
            self.show_notes = false;
            return;
        }
        if !self.show_notes && self.selected.is_some_and(|id| !self.prepare_transition(id)) {
            return;
        }
        self.sidebar_view = view;
        self.show_notes = true;
    }

    fn select(&mut self, id: NoteId) {
        #[cfg(target_os = "android")]
        {
            self.show_notes = false;
        }
        if self.selected != Some(id) {
            if self
                .selected
                .is_some_and(|old| !self.prepare_transition(old))
            {
                return;
            }
            if let Some(previous) = self.selected.and_then(|old| self.drafts.get_mut(&old)) {
                if previous.composing {
                    // Focus changed mid-composition: retain the text for explicit recovery.
                    previous.composing = false;
                    previous.conflict = true;
                }
            }
        }
        if self.selected != Some(id) {
            self.show_preview = self
                .drafts
                .get(&id)
                .is_none_or(|draft| draft.can_change_mode());
        }
        self.selected = Some(id);
        if let Ok(view) = self.app.note(id) {
            if !matches!(self.note_dialog, Some(NoteDialog::Rename { .. })) {
                self.rename_path = view.note.path;
            }
        }
    }

    fn prepare_transition(&mut self, id: NoteId) -> bool {
        #[cfg(target_os = "android")]
        if let Some(draft) = self.drafts.get_mut(&id) {
            let input = crate::android_input::prepare_transition(
                egui::Id::new(("source", id.0, draft.widget_generation)),
                &mut draft.text,
            );
            if input.conflict || draft.composing {
                draft.conflict = true;
                draft.composing = input.composing;
                if self.selected == Some(id) {
                    self.show_preview = false;
                }
                self.error=Some("Pending input retained in the original draft. Copy/review it before Cancel; transition paused.".into());
                return false;
            }
        }
        #[cfg(not(target_os = "android"))]
        let _ = id;
        true
    }
    fn adopt(&mut self, view: &NoteView) {
        if !self.prepare_transition(view.note.id) {
            return;
        }
        let generation = self
            .drafts
            .get(&view.note.id)
            .map_or(0, |draft| draft.widget_generation + 1);
        let context = self
            .app
            .parsed_document(view.note.id)
            .ok()
            .filter(|document| {
                document.revision() == view.note.revision && document.source() == view.note.text
            })
            .unwrap_or_else(|| {
                std::sync::Arc::new(DocumentWorld::parse(
                    view.note.id,
                    view.note.revision,
                    &view.note.text,
                    None,
                ))
            });
        self.drafts
            .insert(view.note.id, Draft::with_context(view, generation, context));
    }

    fn file_actions_available(&self) -> bool {
        self.io_request.is_none() && self.io_error.is_none()
    }
    fn close_blocked(&self, pending_saves: bool, conversation_pending: bool) -> bool {
        pending_saves
            || self.research.busy()
            || self.research.unsaved(&self.app)
            || conversation_pending
            || self.app.codex_project_pending()
            || self.app.automation_permission_pending()
            || self.io_request.is_some()
            || self.io_error.is_some()
            || self.drafts.values().any(Draft::unsubmitted)
    }
    fn dispatch(&mut self, command: Command) {
        let file_action = matches!(
            &command,
            Command::ImportMarkdown { .. }
                | Command::ExportMarkdown { .. }
                | Command::RescanMirror
                | Command::SetMirrorEnabled { .. }
        );
        if (file_action && !self.file_actions_available())
            || (self.io_request.is_some()
                && !matches!(
                    &command,
                    Command::EditNote { .. }
                        | Command::TrashNote { .. }
                        | Command::RestoreNote { .. }
                ))
        {
            // Retain the first correlation/error until it finishes and is acknowledged.
            return;
        }
        match self.app.execute(command, Caller::Local) {
            Ok(CommandResult::Note(view)) => {
                self.error = None;
                if self
                    .drafts
                    .get(&view.note.id)
                    .is_some_and(|draft| draft.proposed_text() == view.note.text)
                {
                    self.adopt(&view);
                }
                self.select(view.note.id);
            }
            Ok(
                CommandResult::Notes(_)
                | CommandResult::CodexProject(_)
                | CommandResult::Research(_),
            ) => {}
            Ok(CommandResult::LanTicket { .. } | CommandResult::LanStatus(_)) => {}
            Ok(CommandResult::Accepted { request_id }) => {
                self.io_request = Some(request_id);
                self.io_message = "File operation pending".into();
            }
            Err(error) => {
                if file_action {
                    self.io_error = Some(error.to_string());
                } else {
                    self.error = Some(error.to_string());
                }
            }
        }
    }

    fn refresh(&mut self, notes: &[NoteView]) {
        for view in notes {
            if self
                .parsed
                .get(&view.note.id)
                .is_none_or(|document| document.revision() != view.note.revision)
            {
                match self.app.parsed_document(view.note.id) {
                    Ok(document) => {
                        self.parsed.insert(view.note.id, document);
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            match self.drafts.get_mut(&view.note.id) {
                Some(draft) if draft.revision != view.note.revision => {
                    if draft.can_change_mode() {
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

#[cfg(feature = "lan")]
struct LanPanel {
    bind: String,
    advertised: String,
    source_ticket: String,
    peer_ticket: String,
    fingerprint: String,
    ticket: String,
    status: Option<crate::commands::LanStatus>,
    message: String,
    request: Option<uuid::Uuid>,
    last_poll: Option<std::time::Instant>,
}
#[cfg(feature = "lan")]
impl Default for LanPanel {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:0".into(),
            advertised: String::new(),
            source_ticket: String::new(),
            peer_ticket: String::new(),
            fingerprint: String::new(),
            ticket: String::new(),
            status: None,
            message: String::new(),
            request: None,
            last_poll: None,
        }
    }
}
#[cfg(feature = "lan")]
impl LanPanel {
    fn execute(&mut self, app: &mut App, command: Command) {
        match app.execute(command, Caller::Local) {
            Ok(CommandResult::LanTicket { ticket }) => {
                self.ticket = ticket;
                self.message =
                    "Ticket ready; verify fingerprints with the other device before confirming."
                        .into();
            }
            Ok(CommandResult::LanStatus(status)) => {
                if let Some(ticket) = &status.ticket {
                    self.ticket = ticket.clone();
                }
                self.status = Some(status);
            }
            Ok(CommandResult::Accepted { request_id }) => {
                self.request = Some(request_id);
                self.message =
                    format!("Sync operation {request_id} pending; note save status is separate");
            }
            Ok(_) => self.message = "Sync command accepted; inspect status below".into(),
            Err(error) => self.message = format!("Sync command failed: {error}"),
        }
        self.last_poll = None;
    }
    fn ticket_identity(ui: &mut egui::Ui, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        match crate::lan::Ticket::parse(text.trim()) {
            Ok(ticket) => {
                ui.label(format!(
                    "Fingerprint: {}\nAddress: {}\nVault: {}",
                    ticket.fingerprint.0, ticket.address, ticket.vault_id
                ));
            }
            Err(error) => {
                ui.colored_label(egui::Color32::YELLOW, format!("Invalid ticket: {error}"));
            }
        }
    }
    fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, app: &mut App) {
        let mut expanded = false;
        ui.collapsing("Sync", |ui| {
            expanded = true;
            egui::ScrollArea::vertical()
                .id_salt("lan-controls")
                .max_height((ctx.content_rect().height() * 0.45).max(1.0))
                .show(ui, |ui| {
            ui.label("Foreground LAN only. Pairing and vault migration require explicit actions.");
            let mut command = None;
            ui.horizontal_wrapped(|ui| {
                if ui.button("Enable sync for this vault").clicked() {
                    command = Some(Command::EnableSync);
                }
                if ui.button("Status").clicked() {
                    command = Some(Command::GetLanStatus);
                }
                if ui.button("Pause").clicked() {
                    command = Some(Command::PauseLan);
                }
                if ui.button("Resume").clicked() {
                    command = Some(Command::ResumeLan);
                }
            });
            ui.label("Listen bind address (IP:port)");
            field_single(ui, &mut self.bind, "lan-bind");
            ui.label("Advertised IP (required for wildcard bind)");
            field_single(ui, &mut self.advertised, "lan-advertised");
            if ui.button("Listen on this address").clicked() {
                command = Some(Command::ListenLan {
                    bind: self.bind.clone(),
                    advertised_ip: (!self.advertised.trim().is_empty())
                        .then(|| self.advertised.trim().to_owned()),
                });
            }
            if ui.button("Read local ticket").clicked() {
                command = Some(Command::GetLanTicket);
            }
            if !self.ticket.is_empty() {
                ui.add(
                    egui::TextEdit::multiline(&mut self.ticket)
                        .desired_rows(2)
                        .interactive(false),
                );
                Self::ticket_identity(ui, &self.ticket);
                if ui.button("Copy local ticket").clicked() {
                    ctx.copy_text(self.ticket.clone());
                    #[cfg(target_os="android")]
                    crate::android_input::copy_text(&self.ticket);
                }
            }
            ui.label("Source ticket for an empty joining vault");
            field_multi(ui, &mut self.source_ticket, "lan-source_ticket");
            Self::ticket_identity(ui, &self.source_ticket);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Prepare empty join").clicked() {
                    command = Some(Command::PrepareJoin {
                        ticket: self.source_ticket.trim().into(),
                    });
                }
                if ui.button("Cancel prepared join").clicked() {
                    command = Some(Command::CancelJoin);
                }
            });
            ui.label("Peer ticket: compare fingerprint out of band before confirming trust");
            field_multi(ui, &mut self.peer_ticket, "lan-peer_ticket");
            Self::ticket_identity(ui, &self.peer_ticket);
            if ui.button("Confirm this peer ticket").clicked() {
                command = Some(Command::ConfirmPeer {
                    ticket: self.peer_ticket.trim().into(),
                });
            }
            ui.label("Peer fingerprint");
            field_single(ui, &mut self.fingerprint, "lan-fingerprint");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Connect fingerprint").clicked() {
                    command = Some(Command::ConnectPeer {
                        fingerprint: self.fingerprint.trim().into(),
                    });
                }
                if ui.button("Unpair fingerprint").clicked() {
                    command = Some(Command::UnpairPeer {
                        fingerprint: self.fingerprint.trim().into(),
                    });
                }
            });
            if let Some(command) = command {
                self.execute(app, command);
            }
            if let Some(status) = &self.status {
                ui.label(format!(
                    "Listening: {} · paused: {} · prepared: {} · joining: {}",
                    status.listening, status.paused, status.prepared, status.joining
                ));
                ui.label(format!(
                    "Pairing state storage: {}/{} generations; queued messages: {}",
                    status.persisted_generation,
                    status.accepted_generation,
                    status.pending_messages
                ));
                for peer in &status.peers {
                    ui.label(format!(
                        "{} · connected: {} · receipt: {:?}",
                        peer.fingerprint, peer.connected, peer.last_receipt
                    ));
                }
                if let Some(error) = &status.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            }
            ui.label(&self.message);
                });
        });
        self.poll(ctx, app, expanded);
    }
    /// Foreground owner polling must continue when its settings view is closed.
    fn poll(&mut self, ctx: &egui::Context, app: &mut App, visible: bool) {
        if let Some(request) = self.request {
            if let Some(status) = app.io_status(request) {
                if !matches!(status, crate::app::IoStatus::Pending) {
                    self.message = format!(
                        "Sync operation {request}: {status:?}; note save status is separate"
                    );
                    self.request = None;
                }
            }
        }
        let active = self.request.is_some()
            || self.status.as_ref().is_some_and(|status| {
                status.listening || status.joining || status.pending_messages > 0
            });
        if self.last_poll.is_none() || visible || active {
            if self
                .last_poll
                .is_none_or(|last| last.elapsed() >= std::time::Duration::from_millis(500))
            {
                self.execute(app, Command::GetLanStatus);
                self.last_poll = Some(std::time::Instant::now());
            }
        }
        let active = self.request.is_some()
            || self.status.as_ref().is_some_and(|status| {
                status.listening || status.joining || status.pending_messages > 0
            });
        if visible || active {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }
}

impl eframe::App for TanglewoodUi {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show_frame(ctx);
    }
}
impl TanglewoodUi {
    fn show_frame(&mut self, ctx: &egui::Context) {
        if !self.theme_installed {
            theme::install(ctx);
            self.theme_installed = true;
        }
        self.app.tick();
        if self.app.research_jobs_active() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(target) =
            ctx.data_mut(|data| data.remove_temp::<String>(egui::Id::new("research-open-citation")))
        {
            let locator = target
                .strip_prefix("source://")
                .map(|value| {
                    let (id, page) = value
                        .split_once("?page=")
                        .map(|(id, page)| (id, Some(page)))
                        .unwrap_or((value, None));
                    page.map(|p| format!("[source:{id} page:{p}]"))
                        .unwrap_or_else(|| format!("[source:{id}]"))
                })
                .unwrap_or(target);
            self.research.open_citation(&self.app, &locator);
            self.sidebar_view = SidebarView::Research;
        }
        self.research.poll(ctx, &mut self.app);
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        if let Some(panel) = &mut self.conversation {
            if let Some(enabled) = panel.vault_permission_requested.take() {
                if let Err(error) = self.app.execute(
                    Command::SetAutomationWriteEnabled { enabled },
                    Caller::Local,
                ) {
                    panel.error = Some(error.to_string());
                }
            }
            panel.vault_write_enabled = self
                .mcp
                .as_ref()
                .map(|socket| socket.policy(&self.app) == crate::mcp::AccessPolicy::ReadWrite);
            self.research.set_conversation(panel.research_capture());
            let workspace = if self.sidebar_view == SidebarView::Research {
                self.research.selected_workspace_id()
            } else {
                None
            };
            if panel.research_workspace != workspace {
                panel.relevant_sources = false;
                panel.relevant_cache = None;
                panel.research_workspace = workspace;
            }
            if let Some((text, turn)) = panel.collect_references.take() {
                self.research.open_references_with_origin(
                    &text,
                    panel.research_capture().current,
                    turn,
                );
                self.sidebar_view = SidebarView::Research;
            }
            if std::mem::take(&mut panel.research_requested) {
                self.research.start_from_conversation();
                self.sidebar_view = SidebarView::Research;
                self.show_notes = true;
            }
        }
        #[cfg(feature = "lan")]
        self.lan_panel.poll(ctx, &mut self.app, false);
        #[cfg(target_os = "android")]
        self.poll_picker(ctx);
        #[cfg(all(target_os = "linux", feature = "mcp"))]
        if let Some(socket) = &mut self.mcp {
            let context = ctx.clone();
            socket.set_waker(move || context.request_repaint());
            socket.poll(&mut self.app);
            if socket.has_pending() {
                ctx.request_repaint_after(std::time::Duration::from_millis(20));
            }
        }
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        if let Some(panel) = &mut self.conversation {
            panel.poll(ctx);
            #[cfg(feature = "codex")]
            if !panel.is_kimi() {
                if panel.persist_project {
                    if let Some(project) = panel.config.project.clone() {
                        let expected = self.app.codex_project().cloned();
                        match self.app.execute(
                            Command::SetCodexProject { expected, project },
                            Caller::Local,
                        ) {
                            Ok(_) => panel.persist_project = false,
                            Err(error) => panel.error = Some(error.to_string()),
                        }
                    }
                }
                panel.binding_durable = !self.app.codex_project_pending()
                    && self.app.codex_project() == panel.config.project.as_ref();
                panel.metadata_error = self.app.codex_project_error().map(str::to_owned);
                if panel.resume_requested
                    && panel.project_ready
                    && panel.binding_durable
                    && !panel.threads_loading
                    && !panel.busy()
                    && !panel.history_loading
                {
                    if let Some(id) = panel
                        .config
                        .resume_thread
                        .clone()
                        .filter(|id| panel.threads.iter().any(|t| &t.id == id))
                    {
                        panel.select_thread(Some(id));
                        panel.resume_requested = false;
                    }
                }
            }
        }
        if let Some(request) = self.io_request {
            if let Some(status) = self.app.io_status(request) {
                if !matches!(status, crate::app::IoStatus::Pending) {
                    self.io_message = format!("File operation: {status:?}");
                    match status {
                        crate::app::IoStatus::Failed(error) => self.io_error = Some(error.clone()),
                        crate::app::IoStatus::Conflict(id) => {
                            self.io_error = Some(format!(
                                "File operation retained conflict {id}; inspect Status details."
                            ))
                        }
                        _ => {}
                    }
                    self.io_request = None;
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
        let notes = self.app.notes();
        self.refresh(&notes);
        if self.selected.is_none() {
            if let Some(view) = notes.iter().find(|view| !view.note.trashed) {
                self.select(view.note.id);
            }
        }
        let mut research_action = None;
        let mut command = None;
        let mut select = None;
        let mut adopt = None;
        let mut retry = false;
        let mut navigate = None;
        let mut edit_range = None;
        let mut read_requested = false;
        let mut editor_action = None;
        let mut return_to_block = None;
        let files_available = self.file_actions_available();
        if ctx.input(|input| input.viewport().close_requested()) {
            self.closing = true;
        }
        self.layout.focus_before_frame = ctx.memory(|m| m.focused());
        if ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Touch { .. }))
        }) {
            self.touch_input = true;
        }
        theme::set_touch_density(ctx, self.touch_input);
        let width = ctx.content_rect().width();
        let left_width = egui::containers::panel::PanelState::load(ctx, egui::Id::new("notes"))
            .map_or(theme::SIDEBAR_WIDTH, |s| s.rect.width())
            .clamp(200.0, 360.0);
        let right_width =
            egui::containers::panel::PanelState::load(ctx, egui::Id::new("shell-right"))
                .map_or(theme::CONTEXT_WIDTH, |s| s.rect.width())
                .clamp(240.0, 360.0);
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        let context_before = self.conversation.as_ref().is_some_and(|p| p.context.open);
        #[cfg(not(all(target_os = "linux", any(feature = "codex", feature = "kimi"))))]
        let context_before = false;
        // Test docking capacity even when a pane is closed, so its toggle can reopen it.
        let plan = self
            .layout
            .plan(width, self.touch_input, Some(left_width), Some(right_width));
        if let Some(previous) = self.layout.previous {
            let focused = self
                .layout
                .focus_before_frame
                .and_then(|id| ctx.read_response(id))
                .map(|r| r.rect.center());
            if previous.left
                && !plan.left
                && focused.is_some_and(|p| self.layout.left_rect.is_some_and(|r| r.contains(p)))
            {
                self.layout.open(layout::Pane::Navigation);
            }
            if previous.right
                && !plan.right
                && focused.is_some_and(|p| self.layout.right_rect.is_some_and(|r| r.contains(p)))
            {
                self.layout
                    .open(if self.sidebar_view == SidebarView::Research {
                        self.right_pane
                    } else {
                        layout::Pane::Context
                    });
            }
        }
        // A pane has exactly one host per frame; reuse its stable child IDs.
        if self.layout.overlay.is_some_and(|pane| match pane {
            layout::Pane::Navigation => plan.left && self.show_notes,
            layout::Pane::Outcomes => {
                plan.right
                    && self.sidebar_view == SidebarView::Research
                    && self.research_outcomes_open
                    && self.right_pane == pane
            }
            layout::Pane::Context => {
                plan.right
                    && ((self.sidebar_view == SidebarView::Codex && context_before)
                        || (self.sidebar_view == SidebarView::Research
                            && self.research_outcomes_open
                            && self.right_pane == pane))
            }
            layout::Pane::Conversations => false,
        }) {
            self.layout.close();
        }
        self.layout_plan = Some(plan);
        self.layout.previous = Some(plan);
        let mobile = plan.compact();
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("shell-context-undocked"), !plan.right);
            d.insert_temp(egui::Id::new("shell-active-overlay"), self.layout.overlay);
        });
        let dark = ctx.style().visuals.dark_mode;
        let colors = theme::colors(dark);
        let main_fill = colors.content;
        let side_fill = colors.navigation;
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::P)) {
            self.sidebar(SidebarView::Search, false);
            #[cfg(target_os = "android")]
            let id = egui::Id::new(("android-field", "quick-switch"));
            #[cfg(not(target_os = "android"))]
            let id = egui::Id::new("quick-switch");
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
        let view = notes
            .iter()
            .find(|view| Some(view.note.id) == self.selected);
        if navigation::enter_reader(
            ctx,
            self.layout.overlay.is_none()
                && self.sidebar_view != SidebarView::Codex
                && self.sidebar_view != SidebarView::Research
                && self.show_preview
                && !self.show_settings
                && !self.show_status
                && self.note_dialog.is_none(),
        ) {
            if let Some(view) = view.filter(|view| !view.note.trashed) {
                self.processing.focus_block(view.note.id, 0);
                ctx.request_repaint();
            }
        }
        if let Some(view) = view {
            if let Some(document) = self.parsed.get(&view.note.id) {
                self.processing.update(ctx, view, document.clone());
            }
        }
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        let codex_mode = self.sidebar_view == SidebarView::Codex && self.conversation.is_some();
        #[cfg(not(all(target_os = "linux", any(feature = "codex", feature = "kimi"))))]
        let codex_mode = false;
        if !mobile {
            egui::SidePanel::left("activity-rail")
                .resizable(false)
                .exact_width(plan.rail)
                .frame(
                    egui::Frame::new()
                        .fill(side_fill)
                        .inner_margin(egui::Margin::symmetric(if mobile { 2 } else { 7 }, 14)),
                )
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.y = 14.0;
                    if theme::selected_button(
                        ui,
                        theme::Icon::Notes,
                        "Notes",
                        true,
                        self.show_notes && self.sidebar_view == SidebarView::Notes,
                    )
                    .clicked()
                    {
                        self.sidebar(SidebarView::Notes, true);
                    }
                    if theme::selected_button(
                        ui,
                        theme::Icon::Search,
                        "Search notes · Ctrl P",
                        true,
                        self.show_notes && self.sidebar_view == SidebarView::Search,
                    )
                    .clicked()
                    {
                        self.sidebar(SidebarView::Search, false);
                    }
                    if theme::selected_button(
                        ui,
                        theme::Icon::Links,
                        "Backlinks",
                        true,
                        self.show_notes && self.sidebar_view == SidebarView::Backlinks,
                    )
                    .clicked()
                    {
                        self.sidebar(SidebarView::Backlinks, false);
                    }
                    if theme::selected_button(
                        ui,
                        theme::Icon::Folder,
                        "Research",
                        true,
                        self.sidebar_view == SidebarView::Research,
                    )
                    .clicked()
                    {
                        if self.selected.is_none_or(|id| self.prepare_transition(id)) {
                            self.sidebar(SidebarView::Research, false);
                        }
                    }
                    #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
                    if self.conversation.is_some() {
                        if theme::selected_button(ui, theme::Icon::Codex, "Codex", true, codex_mode)
                            .clicked()
                        {
                            self.sidebar(SidebarView::Codex, false);
                        }
                    }
                    let bottom_controls_height =
                        2.0 * theme::control_size(ui) + ui.spacing().item_spacing.y;
                    ui.add_space((ui.available_height() - bottom_controls_height).max(0.0));
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        if theme::selected_button(
                            ui,
                            theme::Icon::Connections,
                            "Sync and connections",
                            true,
                            self.show_settings && self.settings_page == SettingsPage::Connections,
                        )
                        .clicked()
                        {
                            self.settings_page = SettingsPage::Connections;
                            self.show_settings = true;
                        }
                        if theme::selected_button(
                            ui,
                            theme::Icon::Settings,
                            "Settings",
                            true,
                            self.show_settings && self.settings_page == SettingsPage::Processing,
                        )
                        .clicked()
                        {
                            self.settings_page = SettingsPage::Processing;
                            self.show_settings = true;
                        }
                    });
                });
        } else {
            egui::TopBottomPanel::top("compact-navigation")
                .exact_height(theme::HEADER_HEIGHT)
                .frame(
                    egui::Frame::new()
                        .fill(side_fill)
                        .inner_margin(egui::Margin::symmetric(8, 4)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if theme::button(ui, theme::Icon::Sidebar, "Toggle navigation pane", true)
                            .clicked()
                        {
                            self.toggle_navigation();
                        }
                        let mut destination = self.sidebar_view;
                        egui::ComboBox::from_id_salt("compact-destination")
                            .selected_text(match destination {
                                SidebarView::Notes => "Notes",
                                SidebarView::Search => "Search",
                                SidebarView::Backlinks => "Backlinks",
                                SidebarView::Research => "Research",
                                SidebarView::Codex => "Codex",
                            })
                            .width((ui.available_width() - 110.0).clamp(70.0, 180.0))
                            .show_ui(ui, |ui| {
                                for (value, label) in [
                                    (SidebarView::Notes, "Notes"),
                                    (SidebarView::Search, "Search"),
                                    (SidebarView::Backlinks, "Backlinks"),
                                    (SidebarView::Research, "Research"),
                                ] {
                                    ui.selectable_value(&mut destination, value, label);
                                }
                                #[cfg(all(
                                    target_os = "linux",
                                    any(feature = "codex", feature = "kimi")
                                ))]
                                if self.conversation.is_some() {
                                    ui.selectable_value(
                                        &mut destination,
                                        SidebarView::Codex,
                                        "Codex",
                                    );
                                }
                            });
                        if destination != self.sidebar_view {
                            self.sidebar(destination, false);
                        }
                        if self.sidebar_view == SidebarView::Codex
                            && theme::button(ui, theme::Icon::Source, "Conversation context", true)
                                .clicked()
                        {
                            self.request_pane(layout::Pane::Context);
                        }
                        let more =
                            theme::button(ui, theme::Icon::More, "Application actions", true);
                        egui::Popup::menu(&more).show(|ui| {
                            if ui.button("Settings").clicked() {
                                self.show_settings = true;
                                ui.close();
                            }
                            if ui.button("Connections").clicked() {
                                self.settings_page = SettingsPage::Connections;
                                self.show_settings = true;
                                ui.close();
                            }
                        });
                    });
                });
        }
        if self.show_notes && plan.left {
            let pane = egui::SidePanel::left("notes")
                .resizable(true)
                .default_width(theme::SIDEBAR_WIDTH)
                .width_range(200.0..=360.0)
                .frame(
                    egui::Frame::new()
                        .fill(side_fill)
                        .inner_margin(egui::Margin::symmetric(12, 17)),
                )
                .show(ctx, |ui| {
                    self.navigation_contents(ui, ctx, &notes, false, &mut select);
                });
            self.layout.left_rect = Some(pane.response.rect);
            navigation::panel_focus(ctx, egui::Id::new("notes"), false, "Resize notes pane");
        }
        if self.sidebar_view == SidebarView::Research {
            let wide_research = plan.right;
            #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
            let conversation_available = self.conversation.is_some();
            #[cfg(not(all(target_os = "linux", any(feature = "codex", feature = "kimi"))))]
            let conversation_available = false;
            egui::TopBottomPanel::top("research-header")
                .exact_height(theme::HEADER_HEIGHT)
                .frame(
                    egui::Frame::new()
                        .fill(main_fill)
                        .inner_margin(egui::Margin::symmetric(12, 4)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if !mobile
                            && theme::button(ui, theme::Icon::Sidebar, "Toggle sources", true)
                                .clicked()
                        {
                            self.toggle_navigation();
                        }
                        let title = self.research.selected_title(&self.app);
                        let action_count = if conversation_available { 3.0 } else { 1.0 };
                        let width = (ui.available_width()
                            - action_count
                                * (theme::control_size(ui) + ui.spacing().item_spacing.x))
                            .max(1.0);
                        ui.add_sized([width, 34.0], egui::Label::new(title).truncate());
                        if conversation_available
                            && theme::button(ui, theme::Icon::Codex, "Conversations", true)
                                .clicked()
                        {
                            self.request_pane(layout::Pane::Conversations);
                        }
                        if conversation_available
                            && theme::button(ui, theme::Icon::Source, "Conversation context", true)
                                .clicked()
                        {
                            self.request_pane(layout::Pane::Context);
                        }
                        if theme::button(ui, theme::Icon::Notes, "Toggle outcomes", true).clicked()
                        {
                            self.request_pane(layout::Pane::Outcomes);
                        }
                    });
                });
            if wide_research && self.research_outcomes_open {
                let pane = egui::SidePanel::right("shell-right")
                    .default_width(theme::CONTEXT_WIDTH)
                    .width_range(240.0..=360.0)
                    .frame(egui::Frame::new().fill(side_fill).inner_margin(16))
                    .show(ctx, |ui| {
                        research_action = self.right_contents(ui, ctx, self.right_pane, &notes);
                    });
                self.layout.right_rect = Some(pane.response.rect);
                navigation::panel_focus(
                    ctx,
                    egui::Id::new("shell-right"),
                    true,
                    "Resize context pane",
                );
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(main_fill).inner_margin(20))
                .show(ctx, |ui| {
                    let width = ui.available_width().min(780.0);
                    let mut rect = ui.available_rect_before_wrap();
                    rect.min.x += (rect.width() - width) * 0.5;
                    rect.max.x = rect.min.x + width;
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                        |ui| {
                            if self.research.reader_visible() {
                                self.research.source_reader(ui, ctx, &mut self.app);
                                return;
                            }
                            #[cfg(all(
                                target_os = "linux",
                                any(feature = "codex", feature = "kimi")
                            ))]
                            if let Some(panel) = &mut self.conversation {
                                if panel.thread.is_none() && !panel.history_loading {
                                    ui.add_space(24.0);
                                    ui.heading("What would you like to explore?");
                                    if ui.button("Choose a conversation…").clicked() {
                                        self.research_chats_open = true;
                                    }
                                    ui.add_space(16.0);
                                }
                                panel.show_surface(ui, ctx, &self.app, None, true);
                                return;
                            }
                            ui.add_space(32.0);
                            ui.heading("Research");
                            ui.weak("Sources and outcomes stay together here.");
                            ui.add_space(12.0);
                            ui.label("Open this vault with Codex enabled to use the conversation.");
                        },
                    );
                });
        } else if codex_mode {
            if plan.right && context_before {
                let pane = egui::SidePanel::right("shell-right")
                    .default_width(theme::CONTEXT_WIDTH)
                    .width_range(240.0..=360.0)
                    .frame(egui::Frame::new().fill(side_fill).inner_margin(16))
                    .show(ctx, |ui| {
                        let _ = self.right_contents(ui, ctx, layout::Pane::Context, &notes);
                    });
                self.layout.right_rect = Some(pane.response.rect);
                navigation::panel_focus(
                    ctx,
                    egui::Id::new("shell-right"),
                    true,
                    "Resize context pane",
                );
            }
            #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(main_fill).inner_margin(24))
                .show(ctx, |ui| {
                    if let Some(panel) = &mut self.conversation {
                        panel.show(ui, ctx, &self.app, self.selected);
                    }
                });
        } else if self.sidebar_view != SidebarView::Research {
            egui::TopBottomPanel::top("document-header")
                .exact_height(theme::HEADER_HEIGHT)
                .frame(
                    egui::Frame::new()
                        .fill(main_fill)
                        .inner_margin(egui::Margin::symmetric(
                            if mobile { 12 } else { 24 },
                            if self.touch_input { 4 } else { 11 },
                        )),
                )
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        if !mobile
                            && theme::button(ui, theme::Icon::Sidebar, "Toggle sidebar", true)
                                .clicked()
                        {
                            self.toggle_navigation();
                        }
                        let actions_width =
                            2.0 * (theme::control_size(ui) + ui.spacing().item_spacing.x);
                        let title_width = (ui.available_width() - actions_width).max(0.0);
                        ui.add_sized(
                            [title_width, 34.0],
                            egui::Label::new(
                                egui::RichText::new(
                                    view.map_or("Tanglewood", |view| note_title(&view.note.path)),
                                )
                                .size(14.0),
                            )
                            .truncate(),
                        );
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let can_mode = view.is_some_and(|view| {
                                !view.note.trashed
                                    && self
                                        .drafts
                                        .get(&view.note.id)
                                        .is_some_and(Draft::can_change_mode)
                            });
                            if theme::button(
                                ui,
                                theme::Icon::Source,
                                "Edit document source",
                                can_mode,
                            )
                            .clicked()
                            {
                                edit_range = Some(None);
                            }
                            let more = theme::button(ui, theme::Icon::More, "Note actions", true);
                            egui::Popup::menu(&more).show(|ui| {
                                if let Some(view) = view {
                                    if ui.button("Rename…").clicked() {
                                        self.rename_path = view.note.path.clone();
                                        self.open_note_dialog(NoteDialog::Rename {
                                            id: view.note.id,
                                            expected: view.note.revision,
                                        });
                                        ui.close();
                                    }
                                    if view.note.trashed {
                                        if ui.button("Restore note").clicked() {
                                            command = Some(Command::RestoreNote {
                                                id: view.note.id,
                                                expected: view.note.revision,
                                            });
                                            ui.close();
                                        }
                                    } else if ui.button("Move to trash").clicked() {
                                        command = Some(Command::TrashNote {
                                            id: view.note.id,
                                            expected: view.note.revision,
                                        });
                                        ui.close();
                                    }
                                    ui.separator();
                                    let can_mode = self
                                        .drafts
                                        .get(&view.note.id)
                                        .is_some_and(Draft::can_change_mode)
                                        && !view.note.trashed;
                                    if ui
                                        .add_enabled(
                                            can_mode,
                                            egui::Button::new("Edit document source"),
                                        )
                                        .clicked()
                                    {
                                        edit_range = Some(None);
                                        ui.close();
                                    }
                                    ui.menu_button("Edit original block…", |ui| {
                                        if let Some(doc) = self.parsed.get(&view.note.id) {
                                            for entity in doc.roots() {
                                                let Some(block) = doc.node(*entity) else {
                                                    continue;
                                                };
                                                if let Some(origin) =
                                                    block.origin.as_ref().filter(|origin| {
                                                        origin.note == view.note.id
                                                            && origin.revision == view.note.revision
                                                    })
                                                {
                                                    let label = view
                                                        .note
                                                        .text
                                                        .get(origin.bytes.clone())
                                                        .unwrap_or("")
                                                        .lines()
                                                        .find(|line| !line.trim().is_empty())
                                                        .unwrap_or("Source block")
                                                        .chars()
                                                        .take(48)
                                                        .collect::<String>();
                                                    if ui
                                                        .add_enabled(
                                                            can_mode,
                                                            egui::Button::new(label),
                                                        )
                                                        .clicked()
                                                    {
                                                        edit_range =
                                                            Some(Some(origin.bytes.clone()));
                                                        ui.close();
                                                    }
                                                }
                                            }
                                        }
                                    });
                                    if ui.button("Refresh preview").clicked() {
                                        self.processing.request_refresh();
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui.button("Import / export Markdown…").clicked() {
                                    self.settings_page = SettingsPage::Files;
                                    self.show_settings = true;
                                    ui.close();
                                }
                                if ui.button("Directory mirror…").clicked() {
                                    self.settings_page = SettingsPage::Files;
                                    self.show_settings = true;
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button("Retry saving").clicked() {
                                    retry = true;
                                    ui.close();
                                }
                                if ui
                                    .button(if self.show_trash {
                                        "Show notes"
                                    } else {
                                        "Show trash / restore…"
                                    })
                                    .clicked()
                                {
                                    self.show_trash = !self.show_trash;
                                    self.sidebar(SidebarView::Notes, false);
                                    ui.close();
                                }
                                if ui.button("Status details…").clicked() {
                                    self.show_status = true;
                                    ui.close();
                                }
                            });
                        });
                    });
                });
            egui::TopBottomPanel::bottom("document-status")
                .exact_height(32.0)
                .frame(
                    egui::Frame::new()
                        .fill(main_fill)
                        .inner_margin(egui::Margin::symmetric(24, 4)),
                )
                .show(ctx, |ui| {
                    ui.spacing_mut().interact_size.y = 20.0;
                    ui.spacing_mut().button_padding = egui::Vec2::ZERO;
                    ui.horizontal(|ui| {
                        let (processing_status, processing_error) = self.processing.status();
                        let draft = view.and_then(|view| self.drafts.get(&view.note.id));
                        let export = view.map(|view| self.app.export_status(view.note.id));
                        let mirror_conflict = view.is_some_and(|view| {
                            self.app
                                .mirror_conflicts()
                                .iter()
                                .any(|conflict| conflict.note.id == view.note.id)
                        });
                        let (label, error) = if self.io_error.is_some() {
                            ("File operation failed", true)
                        } else if self.io_request.is_some() {
                            ("File operation pending", false)
                        } else if self.error.is_some() {
                            ("Action failed", true)
                        } else if view.is_some_and(|view| view.save.error.is_some()) {
                            ("Save failed", true)
                        } else if export.as_ref().is_some_and(|export| export.error.is_some()) {
                            ("Mirror failed", true)
                        } else if mirror_conflict {
                            ("Mirror conflict", true)
                        } else if draft.is_some_and(|draft| draft.conflict) {
                            ("Draft conflict", true)
                        } else if draft.is_some_and(|draft| draft.composing) {
                            ("Composing…", false)
                        } else if draft.is_some_and(Draft::dirty) {
                            ("Draft", false)
                        } else if processing_error {
                            (processing_status, true)
                        } else if view
                            .is_some_and(|view| view.save.persisted != Some(view.save.accepted))
                        {
                            ("Saving…", false)
                        } else if export.as_ref().is_some_and(|export| export.pending) {
                            ("Mirror pending", false)
                        } else if processing_status == "Processing…" {
                            (processing_status, false)
                        } else {
                            ("Saved", false)
                        };
                        let color = if error {
                            ui.visuals().error_fg_color
                        } else {
                            theme::muted(ui)
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(label).size(11.0).color(color),
                                )
                                .frame(true)
                                .frame_when_inactive(false)
                                .min_size(egui::vec2(0.0, 20.0)),
                            )
                            .on_hover_text("Open status details")
                            .clicked()
                        {
                            self.show_status = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new("Markdown")
                                    .small()
                                    .color(theme::muted(ui)),
                            )
                            .on_hover_text("Artwork stays local; text sync is separate");
                        });
                    });
                });
            egui::CentralPanel::default().frame(egui::Frame::new().fill(main_fill)).show(ctx,|ui| {
                let document_scroll = egui::ScrollArea::vertical().id_salt("editor-scroll").auto_shrink([false,false]).show(ui,|ui| {
                    // The contextual edit action reserves another 32 points beside the text.
                    let width=ui.available_width().min(theme::READING_WIDTH + 32.0 + 2.0 * f32::from(plan.gutter()));
                    let margin=((ui.available_width()-width)*0.5).max(0.0);
                    ui.horizontal_top(|ui| {
                        ui.add_space(margin);
                        ui.allocate_ui_with_layout(egui::vec2(width,0.0),egui::Layout::top_down(egui::Align::Min),|ui| {
                            egui::Frame::new().inner_margin(egui::Margin{left:plan.gutter(),right:plan.gutter(),top:if mobile {22} else {32},bottom:46}).show(ui,|ui| {
                                let Some(view)=view else {return;};
                                #[cfg(target_os="android")]
                                crate::android_input::notice(ui);
                                let draft=self.drafts.get_mut(&view.note.id).expect("refresh creates draft");
                                if draft.conflict {ui.colored_label(ui.visuals().warn_fg_color,"Draft retained after a conflicting change. Copy it before Cancel adopts accepted source.");}
                                if !self.show_preview && draft.range.is_none() {
                                    let height=(ctx.content_rect().height()-240.0).clamp(80.0,480.0);
                                    editor_action=editor::source_editor(ui,draft,view.note.id,view.note.trashed,false,height);
                                } else if !self.show_preview && draft.inline_block().is_some() {
                                    let context=draft.context.clone();
                                    let source=context.source();
                                    let range=draft.range.clone().expect("inline block has a range");
                                    self.processing.show_inline(ui,view,&context,source,&range,&mut |ui| {
                                        let height=(ctx.content_rect().height()*0.65).max(80.0);
                                        editor_action=editor::source_editor(ui,draft,view.note.id,view.note.trashed,true,height);
                                    });
                                } else if let Some(range)=self.processing.show(ui,view,&mut navigate) {
                                    if self.show_preview && draft.can_change_mode() && !view.note.trashed {edit_range=Some(Some(range));}
                                }
                            });
                        });
                    });
                });
                navigation::scroll_focus(ctx, &document_scroll);
            });
        } else {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(side_fill))
                .show(ctx, |_| {});
        }
        if !codex_mode && self.sidebar_view != SidebarView::Research && !self.show_preview {
            if let Some(view) = view {
                if let Some(draft) = self.drafts.get_mut(&view.note.id) {
                    if draft.range.is_some() && draft.inline_block().is_none() {
                        // The reading surface remains laid out at its existing scroll
                        // position. Only this bounded native overlay captures input.
                        egui::Modal::new(egui::Id::new((
                            "block-source-modal",
                            view.note.id.0,
                            draft.widget_generation,
                        )))
                        .backdrop_color(egui::Color32::from_black_alpha(45))
                        .show(ctx, |ui| {
                            ui.set_width((ctx.content_rect().width() - 64.0).clamp(180.0, 680.0));
                            ui.label(
                                egui::RichText::new("Edit block source")
                                    .size(13.0)
                                    .color(theme::muted(ui)),
                            );
                            ui.add_space(8.0);
                            if draft.conflict {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    "Source changed. Draft retained; copy it before cancelling.",
                                );
                            }
                            let height =
                                (ctx.content_rect().height() * 0.6 - 80.0).clamp(80.0, 480.0);
                            editor_action = editor::source_editor(
                                ui,
                                draft,
                                view.note.id,
                                view.note.trashed,
                                false,
                                height,
                            );
                        });
                    }
                }
            }
        }
        if self.research_chats_open {
            self.layout.open(layout::Pane::Conversations);
            self.research_chats_open = false;
        }
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        if self.conversation.as_ref().is_some_and(|p| p.context.open)
            && !context_before
            && !plan.right
        {
            self.layout.open(layout::Pane::Context);
        }
        if let Some(request) = ctx.data_mut(|d| {
            d.remove_temp::<Option<layout::Pane>>(egui::Id::new("shell-pane-request"))
                .flatten()
        }) {
            self.request_pane(request);
        }
        let mut panes = vec![layout::Pane::Navigation];
        if self.sidebar_view == SidebarView::Research {
            panes.push(layout::Pane::Outcomes);
        }
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        if self.conversation.is_some()
            && matches!(
                self.sidebar_view,
                SidebarView::Research | SidebarView::Codex
            )
        {
            panes.extend([layout::Pane::Context, layout::Pane::Conversations]);
        }
        if self
            .layout
            .overlay
            .is_some_and(|pane| !panes.contains(&pane))
        {
            self.layout.close();
        }
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("shell-context-undocked"), !plan.right);
            d.insert_temp(egui::Id::new("shell-active-overlay"), self.layout.overlay);
        });
        if let Some(pane) = self.layout.overlay {
            let newly_opened = std::mem::take(&mut self.layout.newly_opened);
            let can_dock = if pane == layout::Pane::Navigation {
                plan.left
            } else {
                pane.right() && plan.right
            };
            let result = layout::pullout(
                ctx,
                pane,
                if pane == layout::Pane::Navigation {
                    left_width.max(300.0)
                } else {
                    right_width.max(340.0)
                },
                &panes,
                can_dock,
                |ui| {
                    if pane == layout::Pane::Navigation {
                        self.navigation_contents(ui, ctx, &notes, true, &mut select);
                    } else if pane.right() {
                        if let Some(action) = self.right_contents(ui, ctx, pane, &notes) {
                            research_action = Some(action);
                        }
                    }
                    #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
                    if pane == layout::Pane::Conversations {
                        if let Some(panel) = &mut self.conversation {
                            ui.scope_builder(
                                egui::UiBuilder::new().id("research-conversations"),
                                |ui| panel.sidebar(ui, ctx),
                            );
                        }
                    }
                },
            );
            if newly_opened {
                ctx.memory_mut(|m| m.request_focus(result.focus));
                ctx.request_repaint();
            }
            if result.close && self.pane_transition_allowed() {
                self.layout.close();
            }
            if let Some(next) = result.switch {
                if self.pane_transition_allowed() {
                    if next == layout::Pane::Navigation && plan.left {
                        self.show_notes = true;
                        self.layout.close();
                    } else if next.right() && plan.right {
                        self.right_pane = next;
                        self.research_outcomes_open = true;
                        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
                        if let Some(panel) = &mut self.conversation {
                            if next == layout::Pane::Context {
                                panel.context.open = true;
                            }
                        }
                        self.layout.close();
                    } else {
                        self.layout.open(next);
                    }
                }
            }
            if result.dock {
                if pane == layout::Pane::Navigation {
                    self.show_notes = true;
                } else {
                    self.right_pane = pane;
                    self.research_outcomes_open = true;
                    #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
                    if let Some(panel) = &mut self.conversation {
                        panel.context.open = pane == layout::Pane::Context;
                    }
                }
                self.layout.close();
            }
        }
        if let Some(id) = self.layout.restore_focus.take() {
            ctx.memory_mut(|m| m.request_focus(id));
            ctx.request_repaint();
        }
        if let Some(action) = editor_action {
            if let Some(view) = view {
                let draft = self.drafts.get(&view.note.id).expect("selected note draft");
                return_to_block = draft
                    .range
                    .as_ref()
                    .map(|range| (view.note.id, range.start));
                match action {
                    editor::EditorAction::Ready => {
                        if let Some(edit) = draft.edit() {
                            command = Some(Command::EditNote {
                                id: view.note.id,
                                expected: draft.revision,
                                edits: vec![edit],
                            });
                        } else {
                            read_requested = true;
                        }
                    }
                    editor::EditorAction::Cancel => {
                        adopt = Some(view.clone());
                        self.show_preview = true;
                    }
                }
            }
        }
        if let Some(action) = self.research.overlays(ctx, &mut self.app, &notes) {
            research_action = Some(action);
        }
        if let Some(action) = research_action {
            match action {
                research::Action::OpenNote(id) => {
                    self.select(id);
                    self.sidebar_view = SidebarView::Notes;
                }
                research::Action::OpenThread(thread) => {
                    #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
                    if let Some(panel) = &mut self.conversation {
                        if panel
                            .research_capture()
                            .threads
                            .iter()
                            .any(|candidate| candidate.same_identity(&thread))
                        {
                            panel.select_thread(Some(thread.thread_id));
                            if self.sidebar_view != SidebarView::Research {
                                self.sidebar_view = SidebarView::Codex;
                                self.show_notes = true;
                            }
                            self.research_chats_open = false;
                        } else {
                            self.research.error = Some("Conversation is not listed in the current Codex project. Refresh that project's conversations first.".into());
                        }
                    }
                    #[cfg(not(all(
                        target_os = "linux",
                        any(feature = "codex", feature = "kimi")
                    )))]
                    {
                        let _ = thread;
                        self.research.error =
                            Some("Codex conversations are unavailable in this build.".into());
                    }
                }
                research::Action::Prepare {
                    prompt,
                    evidence,
                    external,
                } => {
                    #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
                    if let Some(panel) = &mut self.conversation {
                        match panel.prepare_research_context(prompt, evidence, external) {
                            Ok(()) => {
                                self.research.conversation_prepared();
                                if self.sidebar_view != SidebarView::Research {
                                    self.sidebar_view = SidebarView::Codex;
                                    self.show_notes = true;
                                }
                            }
                            Err(error) => self.research.error = Some(error.into()),
                        }
                    } else {
                        self.research.error = Some(
                            "Conversation is unavailable in this session. Use Copy prompt.".into(),
                        );
                    }
                    #[cfg(not(all(
                        target_os = "linux",
                        any(feature = "codex", feature = "kimi")
                    )))]
                    {
                        let _ = (prompt, evidence, external);
                        self.research.error = Some(
                            "Conversation is unavailable in this build. Use Copy prompt.".into(),
                        );
                    }
                }
            }
        }
        if let Some(dialog) = self.note_dialog {
            let field_name = self.dialog_field_name(dialog);
            let mut open = true;
            let mut accepted = false;
            egui::Window::new(match dialog {
                NoteDialog::New => "New note",
                NoteDialog::Rename { .. } => "Rename note",
            })
            .collapsible(false)
            .resizable(false)
            .default_width(330.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Markdown path");
                let path = match dialog {
                    NoteDialog::New => &mut self.new_path,
                    NoteDialog::Rename { .. } => &mut self.rename_path,
                };
                field_single(ui, path, &field_name);
                #[cfg(target_os = "android")]
                crate::android_input::menu(ui);
                if ui
                    .add_enabled(
                        self.io_request.is_none(),
                        egui::Button::new(match dialog {
                            NoteDialog::New => "Create",
                            NoteDialog::Rename { .. } => "Rename",
                        }),
                    )
                    .clicked()
                {
                    command = match dialog {
                        NoteDialog::New => Some(Command::CreateNote {
                            path: self.new_path.clone(),
                            text: String::new(),
                        }),
                        NoteDialog::Rename { id, expected } => Some(Command::RenameNote {
                            id,
                            expected,
                            path: self.rename_path.clone(),
                        }),
                    };
                    accepted = true;
                }
            });
            if !open || accepted {
                self.close_note_dialog();
            }
        }
        if self.show_settings {
            let mut open = self.show_settings;
            egui::Window::new(match self.settings_page {
                SettingsPage::Processing => "Document processing",
                SettingsPage::Connections => "Sync and connections",
                SettingsPage::Files => "Markdown files",
            })
            .open(&mut open)
            .default_width(400.0)
            .max_width((ctx.content_rect().width() - 32.0).max(180.0))
            .max_height((ctx.content_rect().height() - 80.0).max(120.0))
            .vscroll(true)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(
                        &mut self.settings_page,
                        SettingsPage::Processing,
                        "Processing",
                    );
                    ui.selectable_value(
                        &mut self.settings_page,
                        SettingsPage::Connections,
                        "Connections",
                    );
                    ui.selectable_value(&mut self.settings_page, SettingsPage::Files, "Files");
                });
                ui.add_space(12.0);
                match self.settings_page {
                    SettingsPage::Processing => {
                        self.processing.controls(ui);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Appearance");
                            if ui.selectable_label(dark, "Dark").clicked() {
                                ctx.set_visuals(egui::Visuals::dark());
                                theme::palette(ctx);
                            }
                            if ui.selectable_label(!dark, "Light").clicked() {
                                ctx.set_visuals(egui::Visuals::light());
                                theme::palette(ctx);
                            }
                        });
                        ui.checkbox(&mut self.touch_input,"Touch-friendly controls").on_hover_text("Larger targets on any platform; does not change text size or pane layout.");
                    }
                    SettingsPage::Connections => {
                        #[cfg(all(target_os = "linux", feature = "mcp"))]
                        let allowed = self.mcp.as_ref().map_or_else(
                            || self.app.automation_write_preference().unwrap_or(false),
                            |socket| {
                                socket.policy(&self.app) == crate::mcp::AccessPolicy::ReadWrite
                            },
                        );
                        #[cfg(not(all(target_os = "linux", feature = "mcp")))]
                        let allowed = self.app.automation_write_preference().unwrap_or(false);
                        if let Some(enabled) = agent_permission_controls(ui, &self.app, allowed) {
                            if let Err(error) = self.app.execute(
                                Command::SetAutomationWriteEnabled { enabled },
                                Caller::Local,
                            ) {
                                self.error = Some(error.to_string());
                            }
                        }
                        ui.add_space(14.0);
                        #[cfg(feature = "lan")]
                        self.lan_panel.show(ui, ctx, &mut self.app);
                        #[cfg(not(feature = "lan"))]
                        ui.label("Foreground LAN is not enabled in this build.");
                    }
                    SettingsPage::Files => {
                        #[cfg(target_os = "android")]
                        self.picker_controls(ui);
                        #[cfg(not(target_os = "android"))]
                        {
                            ui.label("Local Markdown file path");
                            field_single(ui, &mut self.markdown_path, "markdown-file-path");
                            ui.label("New vault note path for import");
                            field_single(ui, &mut self.new_path, "markdown-import-path");
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled(
                                        files_available,
                                        egui::Button::new("Import as new note"),
                                    )
                                    .clicked()
                                {
                                    command = Some(Command::ImportMarkdown {
                                        source_path: self.markdown_path.clone(),
                                        destination: crate::commands::ImportDestination::Create {
                                            path: self.new_path.clone(),
                                        },
                                    });
                                }
                                if let Some(view) = view {
                                    if ui
                                        .add_enabled(
                                            files_available,
                                            egui::Button::new("Replace selected from file"),
                                        )
                                        .clicked()
                                    {
                                        command = Some(Command::ImportMarkdown {
                                            source_path: self.markdown_path.clone(),
                                            destination:
                                                crate::commands::ImportDestination::Replace {
                                                    id: view.note.id,
                                                    expected: view.note.revision,
                                                },
                                        });
                                    }
                                    if ui
                                        .add_enabled(
                                            files_available,
                                            egui::Button::new("Export current Markdown…"),
                                        )
                                        .clicked()
                                    {
                                        command = Some(Command::ExportMarkdown {
                                            id: view.note.id,
                                            path: self.markdown_path.clone(),
                                        });
                                    }
                                }
                            });
                            ui.label(
                                egui::RichText::new(
                                    "Unapplied edits aren't included.",
                                )
                                .small()
                                .color(theme::muted(ui)),
                            );
                        }
                        ui.separator();
                        let mut mirror = self.app.mirror_enabled();
                        if ui
                            .add_enabled(
                                files_available,
                                egui::Checkbox::new(&mut mirror, "Markdown directory mirror"),
                            )
                            .changed()
                        {
                            command = Some(Command::SetMirrorEnabled { enabled: mirror });
                        }
                        if ui
                            .add_enabled(files_available, egui::Button::new("Rescan mirror"))
                            .clicked()
                        {
                            command = Some(Command::RescanMirror);
                        }
                        if !self.io_message.is_empty() {
                            ui.label(&self.io_message);
                        }
                        if let Some(error) = &self.io_error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                            if ui.button("Review file operation status").clicked() {
                                self.show_status = true;
                            }
                        }
                    }
                }
                #[cfg(target_os = "android")]
                crate::android_input::menu(ui);
            });
            self.show_settings = open;
        }
        if self.show_status {
            let mut open = self.show_status;
            egui::Window::new("Status")
                .open(&mut open)
                .default_width(420.0)
                .vscroll(true)
                .show(ctx, |ui| {
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if let Some(error) = &self.io_error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if let Some(error) = self.processing.error_details() {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if let Some(view) = view {
                        if let Some(error) = &view.save.error {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                format!("Save failed: {error}"),
                            );
                        }
                        let export = self.app.export_status(view.note.id);
                        if let Some(error) = export.error {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                format!("Markdown mirror: {error}"),
                            );
                        } else if export.pending {
                            ui.label("Markdown mirror pending");
                        }
                        for conflict in self
                            .app
                            .mirror_conflicts()
                            .iter()
                            .filter(|conflict| conflict.note.id == view.note.id)
                        {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                format!(
                                    "Retained mirror conflict: {} ({})",
                                    conflict.path, conflict.kind
                                ),
                            );
                        }
                    }
                    if !self.io_message.is_empty() {
                        ui.label(&self.io_message);
                    }
                    if ui.button("Retry saving").clicked() {
                        retry = true;
                    }
                    if ui.button("Refresh preview").clicked() {
                        self.processing.request_refresh();
                    }
                    if ui.button("Dismiss action message").clicked() {
                        self.error = None;
                        self.io_error = None;
                    }
                    #[cfg(target_os = "android")]
                    crate::android_input::notice(ui);
                });
            self.show_status = open;
        }
        let navigation_source = notes
            .iter()
            .find(|view| Some(view.note.id) == self.selected)
            .map(|view| view.note.path.clone());
        if read_requested
            && self
                .selected
                .and_then(|id| self.drafts.get(&id))
                .is_none_or(Draft::can_change_mode)
        {
            let retired = self.selected.is_none_or(|id| self.prepare_transition(id));
            if retired {
                self.show_preview = true;
            }
        }
        if let Some(range) = edit_range {
            if let Some(id) = self.selected {
                let retired = self.prepare_transition(id);
                if let Some(draft) = self.drafts.get_mut(&id) {
                    if retired && draft.choose_range(range) {
                        self.show_preview = false;
                    }
                }
            }
        }
        if let Some(view) = adopt {
            #[cfg(target_os = "android")]
            if let Some(draft) = self.drafts.get(&view.note.id) {
                crate::android_input::retire_widget(
                    egui::Id::new(("source", view.note.id.0, draft.widget_generation)),
                    true,
                );
            }
            // Cancel is an explicit discard, including interrupted composition.
            if let Some(draft) = self.drafts.get_mut(&view.note.id) {
                draft.composing = false;
                draft.conflict = false;
            }
            self.adopt(&view);
        }
        if let Some(command) = command {
            let ready = matches!(command, Command::EditNote { .. });
            let transition_ok =
                !ready || self.selected.is_none_or(|id| self.prepare_transition(id));
            if transition_ok {
                self.dispatch(command);
            }
            if ready && transition_ok && self.error.is_none() {
                self.show_preview = true;
            }
        }
        if self.show_preview {
            if let Some((note, offset)) = return_to_block {
                self.processing.focus_block(note, offset);
                ctx.request_repaint();
            }
        }
        if let Some(id) = select {
            self.select(id);
            if self.selected == Some(id) && self.layout.overlay == Some(layout::Pane::Navigation) {
                self.layout.close();
            }
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
        if self.closing {
            if let Some(id) = self.selected {
                // A callback may have arrived after TextEdit's before() this frame.
                // Drain it before deciding whether any local work blocks close.
                self.prepare_transition(id);
            }
        }
        let current = self.app.notes();
        let pending = current
            .iter()
            .any(|view| view.save.persisted != Some(view.save.accepted));
        let local_drafts = self.drafts.values().any(|draft| draft.unsubmitted());
        #[allow(unused_mut)]
        let mut conversation_pending = false;
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        if self.closing {
            if let Some(panel) = &mut self.conversation {
                panel.disconnect();
                conversation_pending = panel.has_adapter();
                if conversation_pending {
                    ctx.request_repaint_after(std::time::Duration::from_millis(20));
                }
            }
        }
        if self.closing {
            if self.close_blocked(pending, conversation_pending) {
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
                if self.io_request.is_some() {
                    status.push_str(" Waiting for the pending file operation to finish.");
                }
                if self.io_error.is_some() {
                    status.push_str(
                        " Review and acknowledge the file error in Status details before closing.",
                    );
                }
                if self.app.automation_permission_pending() {
                    status.push_str(" Vault permission is not saved; open Settings → Connections to review or retry.");
                }
                if self.app.codex_project_pending() {
                    status
                        .push_str(" Project metadata is not saved; open Codex to review or retry.");
                }
                if self.research.busy() || self.research.unsaved(&self.app) {
                    self.research.error=Some("Close paused: save or discard research edits and finish or discard any retained import first.".into());
                    status.push_str(" Research changes need attention in Research.");
                }
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
            || self.app.codex_project_pending()
            || (self.app.automation_permission_pending()
                && self.app.automation_permission_error().is_none())
            || self.io_request.is_some()
            || current
                .iter()
                .any(|view| self.app.export_status(view.note.id).pending)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
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

#[cfg(test)]
mod tests {
    use super::{
        App, Caller, Command, CommandResult, contiguous_edit, linked_path, resolve_note_target,
    };

    #[cfg(not(target_os = "android"))]
    #[test]
    fn accesskit_click_opens_exact_whole_document_source() {
        use eframe::egui::{self, accesskit};
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut shell = super::TanglewoodUi::new(App::new());
        let source = "# Exact source\n\nUnicode α 🌲 and **markup**.\n";
        shell.dispatch(Command::CreateNote {
            path: "Native-action.md".into(),
            text: source.into(),
        });
        let note = shell.selected.unwrap();
        shell.refresh(&shell.app.notes());
        shell.show_preview = true;
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 850.0),
            )),
            ..Default::default()
        };
        let output = ctx.run(input(), |ctx| shell.show_frame(ctx));
        let update = output
            .platform_output
            .accesskit_update
            .expect("accessibility tree");
        let (target, node) = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Edit document source"))
            .expect("fresh source action node");
        assert!(node.supports_action(accesskit::Action::Click));
        let mut action_input = input();
        action_input
            .events
            .push(egui::Event::AccessKitActionRequest(
                accesskit::ActionRequest {
                    action: accesskit::Action::Click,
                    target: *target,
                    data: None,
                },
            ));
        let _ = ctx.run(action_input, |ctx| shell.show_frame(ctx));
        assert!(
            !shell.show_preview,
            "AccessKit Click must enter source mode"
        );
        let draft = &shell.drafts[&note];
        assert_eq!(draft.range, None);
        assert_eq!(draft.text, source);
        assert_eq!(shell.app.note(note).unwrap().note.text, source);
    }

    #[test]
    fn responsive_frames_preserve_source_draft_and_use_one_pane_host() {
        let ctx = eframe::egui::Context::default();
        let mut shell = super::TanglewoodUi::new(App::new());
        shell.dispatch(Command::CreateNote {
            path: "Draft.md".into(),
            text: "Original source".into(),
        });
        let note = shell.selected.unwrap();
        shell.refresh(&shell.app.notes());
        shell.show_preview = false;
        shell.drafts.get_mut(&note).unwrap().text = "Unsubmitted α draft".into();
        let generation = shell.drafts[&note].widget_generation;
        let frame = |shell: &mut super::TanglewoodUi, width: f32| {
            let _ = ctx.run(
                eframe::egui::RawInput {
                    screen_rect: Some(eframe::egui::Rect::from_min_size(
                        eframe::egui::Pos2::ZERO,
                        eframe::egui::vec2(width, 844.0),
                    )),
                    ..Default::default()
                },
                |ctx| shell.show_frame(ctx),
            );
        };
        frame(&mut shell, 1440.0);
        frame(&mut shell, 390.0);
        shell.toggle_navigation();
        assert_eq!(shell.layout.overlay, Some(super::layout::Pane::Navigation));
        frame(&mut shell, 390.0);
        let overlay_focus = ctx.memory(|m| m.focused());
        let _ = ctx.run(
            eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(390.0, 844.0),
                )),
                events: vec![
                    eframe::egui::Event::Text("Must not enter covered draft".into()),
                    eframe::egui::Event::Key {
                        key: eframe::egui::Key::F6,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: eframe::egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| shell.show_frame(ctx),
        );
        assert_eq!(shell.drafts[&note].text, "Unsubmitted α draft");
        assert_eq!(ctx.memory(|m| m.focused()), overlay_focus);
        frame(&mut shell, 1440.0);
        assert_eq!(shell.layout.overlay, None); // preferred dock restored, no duplicate global IDs
        assert_eq!(shell.drafts[&note].text, "Unsubmitted α draft");
        assert_eq!(shell.drafts[&note].widget_generation, generation);
        assert_eq!(shell.app.note(note).unwrap().note.text, "Original source");
        assert!(!shell.show_preview);
        shell.touch_input = true;
        frame(&mut shell, 390.0);
        assert_eq!(ctx.style().spacing.interact_size.y, 48.0);
    }

    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn research_capture_requires_listed_project_and_matching_thread_origin() {
        use crate::codex::{CodexConfig, ThreadSummary, ThreadWorkState};
        let mut panel = super::ConversationPanel::new(CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: Some(crate::notes::CodexProjectBinding {
                project_id: Some("project".into()),
                name: "Project".into(),
                creation_key: "key".into(),
            }),
        });
        panel.project_ready = true;
        panel.binding_durable = true;
        panel.ready = true;
        panel.thread = Some("current".into());
        panel.threads.push(ThreadSummary {
            id: "current".into(),
            title: "Investigation".into(),
            updated_at: 0,
            state: ThreadWorkState::Idle,
        });
        for (thread, text) in [("current", "current finding"), ("old", "stale finding")] {
            panel.messages.push(super::ConversationMessage {
                role: "Codex",
                key: Some((thread.into(), "turn".into(), "item".into())),
                text: text.into(),
                command: None,
                activity: None,
            });
        }
        let capture = panel.research_capture();
        assert_eq!(
            capture.answer,
            Some(("current finding".into(), Some("turn".into())))
        );
        assert!(!capture.discussion.contains("stale finding"));
        panel.pending_thread = Some(Some("other".into()));
        assert!(panel.research_capture().answer.is_none());
        assert!(panel.research_capture().discussion.is_empty());
        panel.pending_thread = None;
        panel.threads.clear();
        assert!(panel.research_capture().current.is_none());
        panel
            .prepare_research_context("gather".into(), "context".into(), true)
            .unwrap();
        assert!(panel.controls.network_access);
        assert_eq!(
            panel.context.draft.attachments[0].name,
            "Research context · external gathering"
        );
        assert!(
            panel
                .prepare_research_context("replace".into(), "replace".into(), false)
                .is_err()
        );
        assert_eq!(panel.prompt, "gather");
        assert!(panel.controls.network_access);
    }

    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn research_preparation_preserves_drafts_context_and_access() {
        use crate::codex::{ApprovalMode, CodexConfig, PromptAttachment};
        let mut panel = super::ConversationPanel::new(CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        });
        let rejected = panel
            .prepare_research_prompt("research".into(), "exact source evidence".into())
            .unwrap_err();
        assert!(rejected.contains("open or create a conversation"));
        assert!(panel.prompt.is_empty());
        assert!(panel.context.draft.attachments.is_empty());
        assert!(panel.prompt_drafts.is_empty());
        panel.thread = Some("research-thread".into());
        panel.prompt = " ".into();
        assert!(
            panel
                .prepare_research_prompt("research".into(), "exact source evidence".into())
                .is_err()
        );
        assert_eq!(panel.prompt, " ");
        assert!(panel.context.draft.attachments.is_empty());
        panel.prompt.clear();
        panel.context.draft.attachments.push(PromptAttachment {
            name: "evidence".into(),
            text: "retain".into(),
        });
        assert!(
            panel
                .prepare_research_prompt("research".into(), "exact source evidence".into())
                .is_err()
        );
        assert_eq!(panel.context.draft.attachments[0].text, "retain");
        assert_eq!(panel.context.draft.attachments.len(), 1);
        assert!(panel.prompt.is_empty());
        panel.context.draft.attachments.clear();
        panel.controls.approval = ApprovalMode::FullAccess;
        assert!(
            panel
                .prepare_research_prompt("research".into(), "exact source evidence".into())
                .is_err()
        );
        assert_eq!(panel.controls.approval, ApprovalMode::FullAccess);
        assert!(panel.prompt.is_empty());
        assert!(panel.context.draft.attachments.is_empty());
        panel.controls.approval = ApprovalMode::Ask;
        panel.controls.network_access = true;
        assert!(
            panel
                .prepare_research_prompt("research".into(), "exact source evidence".into())
                .is_ok()
        );
        assert_eq!(panel.prompt, "research");
        assert!(!panel.controls.network_access);
        assert_eq!(panel.context.draft.attachments.len(), 1);
        assert_eq!(panel.context.draft.attachments[0].name, "Research sources");
        assert_eq!(
            panel.context.draft.attachments[0].text,
            "exact source evidence"
        );
        // Confirmed switches cache both pieces under the selected conversation.
        panel
            .prompt_drafts
            .insert("other-thread".into(), "other draft".into());
        panel.begin_thread_open(Some("other-thread".into()));
        panel.commit_thread_open("other-thread".into());
        assert_eq!(panel.prompt, "other draft");
        assert!(panel.context.draft.attachments.is_empty());
        panel.begin_thread_open(Some("research-thread".into()));
        panel.commit_thread_open("research-thread".into());
        assert_eq!(panel.prompt, "research");
        assert_eq!(panel.context.draft.attachments.len(), 1);
        assert_eq!(panel.context.draft.attachments[0].name, "Research sources");
        assert_eq!(
            panel.context.draft.attachments[0].text,
            "exact source evidence"
        );
        assert!(panel.adapter.is_none());
        assert!(panel.active_turn.is_none());
    }

    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn composer_profiles_keep_goals_and_command_output_scoped() {
        use crate::codex::{CodexConfig, CommandExecution, GoalInfo, WorkMode};
        let mut panel = super::ConversationPanel::new(CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        });
        panel.thread = Some("A".into());
        panel.ready = true;
        assert!(!panel.mode_consistent());
        panel.accept_goal(None);
        assert!(panel.mode_consistent());
        panel.select_mode(WorkMode::Research);
        assert_eq!(panel.controls.mode, WorkMode::Research);
        assert!(panel.controls.network_access);
        panel.prompt = "keep draft".into();
        panel.select_mode(WorkMode::Goal); // Opens setup; no goal or turn is invented.
        assert_eq!(panel.controls.mode, WorkMode::Research);
        assert_eq!(panel.prompt, "keep draft");
        assert!(panel.context.goal.is_none());
        panel.accept_goal(Some(GoalInfo {
            objective: "Research".into(),
            status: "active".into(),
            tokens_used: 0,
            time_used_seconds: 42,
            token_budget: None,
        }));
        assert_eq!(panel.controls.mode, WorkMode::Goal);
        assert!(panel.mode_consistent());
        panel.select_mode(WorkMode::Goal);
        assert!(!panel.controls.network_access);
        panel.controls.network_access = true; // Later explicit Access choice survives routine updates.
        panel.accept_goal(panel.context.goal.clone());
        assert!(panel.controls.network_access);
        panel.select_mode(WorkMode::Plan); // No adapter: pause fails, so Send must stay blocked.
        assert_eq!(panel.controls.mode, WorkMode::Plan);
        assert!(!panel.mode_consistent());
        assert!(panel.context_goal_pending.is_none());
        assert!(!panel.controls.network_access);
        panel.accept_goal(None);
        assert!(panel.mode_consistent());
        let command = CommandExecution {
            item: "item".into(),
            command: "echo test".into(),
            cwd: "/tmp".into(),
            status: "inProgress".into(),
            exit_code: None,
            output: "test".into(),
            truncated: false,
            output_complete: true,
        };
        panel.command_card("B".into(), "turn".into(), command.clone());
        assert!(panel.messages.is_empty());
        panel.command_card("A".into(), "turn".into(), command.clone());
        assert_eq!(panel.messages.len(), 1);
        assert_eq!(panel.messages[0].text, "test");
        let mut completed = command;
        completed.status = "completed".into();
        completed.exit_code = Some(0);
        completed.output = "test done".into();
        panel.command_card("A".into(), "turn".into(), completed);
        assert_eq!(panel.messages.len(), 1);
        assert_eq!(panel.messages[0].text, "test done");
        assert_eq!(
            panel.messages[0].command.as_ref().unwrap().exit_code,
            Some(0)
        );
        let mut no_aggregate = panel.messages[0].command.clone().unwrap();
        no_aggregate.output_complete = false;
        no_aggregate.truncated = true;
        panel.command_card("A".into(), "turn".into(), no_aggregate);
        assert_eq!(panel.messages[0].text, "test done");
        assert!(panel.messages[0].command.as_ref().unwrap().truncated);
        // Completion is a lifecycle signal, not a successful exit status.
        assert_eq!(super::chat_status("completed", Some(101)), ("Failed", true));
        assert_eq!(super::chat_status("failed", None), ("Failed", true));
        assert_eq!(
            super::chat_status("unknownOutcome", None),
            ("Outcome unknown", true)
        );
        assert_eq!(
            super::chat_status("awaitingInput", None),
            ("awaitingInput", false)
        );
        panel.context.goal_loaded = false;
        assert!(!panel.mode_consistent());
        panel
            .context
            .draft
            .attachments
            .push(crate::codex::PromptAttachment {
                name: "source".into(),
                text: "exact snapshot".into(),
            });
        panel.context_submit_pending = true;
        panel.start_turn("A".into(), "automatic-goal-turn".into());
        assert!(panel.context_submit_pending);
        assert_eq!(panel.context.draft.attachments[0].text, "exact snapshot");
        panel.accept_turn("B");
        assert!(panel.context_submit_pending);
        panel.accept_turn("A");
        assert!(!panel.context_submit_pending);
        assert!(panel.context.draft.is_empty());
        panel.prompt = "keep prompt".into();
        let activity = crate::codex::Activity {
            item: "tool".into(),
            kind: "mcpToolCall".into(),
            title: "Read a note".into(),
            status: "inProgress".into(),
            details: "args".into(),
            truncated: false,
        };
        let count = panel.messages.len();
        panel.activity_card("B".into(), "turn".into(), activity.clone());
        assert_eq!(panel.messages.len(), count);
        panel.activity_card("A".into(), "turn".into(), activity.clone());
        panel.retire_activity(Some("B"), Some("turn"), "ended");
        assert_eq!(
            panel
                .messages
                .last()
                .unwrap()
                .activity
                .as_ref()
                .unwrap()
                .status,
            "inProgress"
        );
        panel.retire_activity(Some("A"), Some("different"), "ended");
        assert_eq!(
            panel
                .messages
                .last()
                .unwrap()
                .activity
                .as_ref()
                .unwrap()
                .status,
            "inProgress"
        );
        panel.retire_activity(Some("A"), Some("turn"), "ended");
        assert_eq!(
            panel
                .messages
                .last()
                .unwrap()
                .activity
                .as_ref()
                .unwrap()
                .status,
            "ended"
        );
        assert_eq!(panel.messages.last().unwrap().text, "args");
        let mut complete = activity;
        complete.status = "completed".into();
        complete.details = "result".into();
        panel.activity_card("A".into(), "turn".into(), complete);
        assert_eq!(panel.messages.len(), count + 1);
        assert_eq!(panel.messages.last().unwrap().text, "result");
        assert_eq!(
            panel
                .messages
                .last()
                .unwrap()
                .activity
                .as_ref()
                .unwrap()
                .status,
            "completed"
        );
        assert_eq!(panel.prompt, "keep prompt");
    }

    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn thread_actions_preserve_drafts_and_only_clear_acknowledged_target() {
        use crate::codex::{ThreadAction, ThreadSummary, ThreadWorkState};
        let mut panel = super::ConversationPanel::new(crate::codex::CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        });
        panel.project_ready = true;
        panel.binding_durable = true;
        panel.threads = vec![
            ThreadSummary {
                id: "A".into(),
                title: "First".into(),
                state: ThreadWorkState::Idle,
                updated_at: 0,
            },
            ThreadSummary {
                id: "B".into(),
                title: "Second".into(),
                state: ThreadWorkState::Idle,
                updated_at: 0,
            },
        ];
        panel.thread = Some("A".into());
        panel.ready = true;
        panel.prompt = "unsent".into();
        assert!(panel.can_thread_action("A", &ThreadAction::Remove).is_err());
        assert!(
            panel
                .can_thread_action("A", &ThreadAction::Rename("Title".into()))
                .is_ok()
        );
        panel.prompt.clear();
        panel
            .prompt_drafts
            .insert("B".into(), "cached draft".into());
        assert!(
            panel
                .can_thread_action("A", &ThreadAction::Archive)
                .is_err()
        );
        panel.prompt_drafts.clear();
        assert!(
            panel
                .can_thread_action("foreign", &ThreadAction::Remove)
                .is_err()
        );
        panel.threads[1].state = ThreadWorkState::Working;
        assert!(panel.can_thread_action("A", &ThreadAction::Remove).is_err());
        panel.threads[1].state = ThreadWorkState::Idle;
        assert!(panel.can_thread_action("A", &ThreadAction::Remove).is_ok());
        panel.finish_thread_action("B", &ThreadAction::Rename("New title".into()));
        assert_eq!(panel.threads[1].title, "New title");
        panel.finish_thread_action("B", &ThreadAction::Archive);
        assert_eq!(panel.thread.as_deref(), Some("A"));
        assert!(panel.ready);
        panel.finish_thread_action("A", &ThreadAction::Remove);
        assert!(panel.thread.is_none());
        assert!(panel.config.resume_thread.is_none());
        assert!(!panel.ready);
        assert!(panel.threads.is_empty());
        panel.threads = vec![ThreadSummary {
            id: "A".into(),
            title: "First".into(),
            state: ThreadWorkState::Idle,
            updated_at: 0,
        }];
        panel.thread = Some("A".into());
        panel.prompt = "recover me".into();
        panel.active_turn = Some("work".into());
        panel.finish_thread_action("A", &ThreadAction::Remove);
        assert_eq!(panel.thread.as_deref(), Some("A"));
        assert_eq!(panel.prompt, "recover me");
        assert!(!panel.busy());
        assert!(!panel.ready);
        assert!(panel.unavailable_threads.contains("A"));
        panel.select_thread(None);
        assert!(panel.error.as_deref().unwrap().contains("Copy or clear"));
        panel.prompt.clear();
        panel.thread = None;
        panel
            .prompt_drafts
            .insert("A".into(), "cached recovery".into());
        panel.select_thread(Some("A".into()));
        assert_eq!(panel.prompt, "cached recovery");
        assert!(!panel.ready);
        panel
            .prompt_drafts
            .insert("not-on-page".into(), "off-page recovery".into());
        panel.finish_thread_action("not-on-page", &ThreadAction::Archive);
        assert!(panel.threads.iter().any(|t| t.id == "not-on-page"));
        assert_eq!(panel.prompt_drafts["not-on-page"], "off-page recovery");

        panel.models = vec![crate::codex::ModelChoice {
            model: "small".into(),
            name: "Small".into(),
            description: String::new(),
            default_effort: "low".into(),
            efforts: vec![("low".into(), "Fast".into())],
        }];
        panel
            .context
            .draft
            .attachments
            .push(crate::codex::PromptAttachment {
                name: "note.md".into(),
                text: "source snapshot".into(),
            });
        panel.threads = vec![ThreadSummary {
            id: "C".into(),
            title: "Context".into(),
            state: ThreadWorkState::Idle,
            updated_at: 0,
        }];
        panel.unavailable_threads.clear();
        panel.thread = Some("C".into());
        panel.prompt.clear();
        assert!(panel.can_thread_action("C", &ThreadAction::Remove).is_err());
        panel.finish_thread_action("C", &ThreadAction::Remove);
        assert!(panel.unavailable_threads.contains("C"));
        assert!(panel.context.has_draft());
        panel.context_submit_pending = true;
        panel.starting = false;
        assert!(panel.busy()); // Optional inventory errors cannot unlock in-flight input.
        assert_eq!(panel.context.draft.attachments[0].text, "source snapshot");
        panel.context_submit_pending = false;
        panel.selected_effort = Some("ultra".into());
        panel.choose_model("small");
        assert_eq!(panel.selected_effort.as_deref(), Some("low"));
        assert!(panel.model_override);
    }

    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn failed_thread_switch_keeps_prompt_ownership_and_provisional_drafts() {
        let mut panel = super::ConversationPanel::new(crate::codex::CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp".into(),
            mcp_bridge: "bridge".into(),
            socket: "owner".into(),
            resume_thread: None,
            project: None,
        });
        panel.thread = Some("A".into());
        panel.ready = true;
        panel.prompt = "Draft A".into();
        panel.prompt_drafts.insert("B".into(), "Draft B".into());
        panel.begin_thread_open(Some("B".into()));
        assert_eq!(panel.prompt, "Draft A");
        assert_eq!(panel.thread.as_deref(), Some("A"));
        // Failed/open-disconnected: no prompt was moved or cache entry overwritten.
        panel.pending_thread = None;
        panel.starting = false;
        panel.begin_thread_open(Some("A".into()));
        panel.commit_thread_open("A".into());
        assert_eq!(panel.prompt, "Draft A");
        assert_eq!(panel.prompt_drafts["B"], "Draft B");
        panel.begin_thread_open(Some("B".into()));
        panel.commit_thread_open("B".into());
        assert_eq!(panel.prompt, "Draft B");
        assert_eq!(panel.prompt_drafts["A"], "Draft A");
        panel.project_ready = true;
        panel.binding_durable = true;
        panel.fresh_thread = true;
        panel.select_thread(Some("A".into()));
        assert_eq!(panel.prompt, "Draft B");
        assert!(panel.error.as_deref().unwrap().contains("Send or clear"));
    }

    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn codex_sidebar_preserves_note_drafts_and_waits_for_project_metadata() {
        let vault =
            std::env::temp_dir().join(format!("tanglewood-codex-sidebar-{}", uuid::Uuid::new_v4()));
        let app = App::open(&vault).unwrap();
        let mut ui = super::TanglewoodUi::new(app).with_codex(crate::codex::CodexConfig {
            executable: "codex".into(),
            cwd: vault.join("context"),
            mcp_bridge: "tanglewood-mcp".into(),
            socket: vault.join("owner.sock"),
            resume_thread: None,
            project: None,
        });
        assert!(ui.close_blocked(false, false));
        assert!(ui.conversation.as_ref().unwrap().adapter.is_none());
        ui.dispatch(Command::CreateNote {
            path: "Draft.md".into(),
            text: "Original".into(),
        });
        let id = ui.selected.unwrap();
        ui.refresh(&ui.app.notes());
        ui.show_preview = false;
        ui.drafts.get_mut(&id).unwrap().text = "Unsubmitted text".into();
        ui.sidebar(super::SidebarView::Codex, false);
        assert_eq!(ui.selected, Some(id));
        ui.sidebar(super::SidebarView::Notes, false);
        assert_eq!(ui.drafts[&id].text, "Unsubmitted text");
        assert!(!ui.show_preview);
        ui.app.flush().unwrap();
        let binding = ui.app.codex_project().cloned().unwrap();
        assert!(binding.project_id.is_none());
        assert_eq!(ui.app.note(id).unwrap().note.text, "Original");
        ui.app.shutdown().unwrap();
        drop(ui);
        std::fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn file_request_keeps_correlation_and_blocks_close_until_acknowledged() {
        let mut ui = super::TanglewoodUi::new(App::new());
        let request = uuid::Uuid::new_v4();
        ui.io_request = Some(request);
        ui.io_message = "original request".into();
        assert!(ui.close_blocked(false, false));
        ui.dispatch(Command::ImportMarkdown {
            source_path: "not-started.md".into(),
            destination: crate::commands::ImportDestination::Create {
                path: "import.md".into(),
            },
        });
        ui.dispatch(Command::CreateNote {
            path: "blocked.md".into(),
            text: String::new(),
        });
        assert_eq!(ui.io_request, Some(request));
        assert_eq!(ui.io_message, "original request");
        assert!(ui.app.notes().is_empty());
        // A terminal file error is sticky across later successful note actions.
        ui.io_request = None;
        ui.io_error = Some("retained import failure".into());
        ui.dispatch(Command::CreateNote {
            path: "other.md".into(),
            text: String::new(),
        });
        assert_eq!(ui.io_error.as_deref(), Some("retained import failure"));
        assert!(!ui.file_actions_available());
        assert!(ui.close_blocked(false, false));
        ui.io_error = None; // Explicit acknowledgement in the Status window.
        assert!(ui.file_actions_available());
        assert!(!ui.close_blocked(false, false));
        ui.open_note_dialog(super::NoteDialog::New);
        let first = ui.dialog_field_name(ui.note_dialog.unwrap());
        ui.close_note_dialog();
        ui.open_note_dialog(super::NoteDialog::New);
        let reopened = ui.dialog_field_name(ui.note_dialog.unwrap());
        assert_ne!(first, reopened);
        let note = ui.app.notes().remove(0).note;
        ui.open_note_dialog(super::NoteDialog::Rename {
            id: note.id,
            expected: note.revision,
        });
        let renamed = ui.dialog_field_name(ui.note_dialog.unwrap());
        assert_ne!(renamed, reopened);
        assert!(
            matches!(ui.note_dialog,Some(super::NoteDialog::Rename{id,expected}) if id==note.id && expected==note.revision)
        );
        ui.close_note_dialog();
        assert!(ui.note_dialog.is_none());
    }

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
use crate::codex::{
    ApprovalDecision, ApprovalMode, CodexAdapter, CodexConfig, CodexEvent, CodexRequest,
    ThreadAction, TurnControls, WorkMode,
};

#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
use crate::conversation::ThreadWorkState;

#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
fn chat_status(status: &str, exit_code: Option<i64>) -> (&str, bool) {
    if exit_code.is_some_and(|code| code != 0) {
        return ("Failed", true);
    }
    match status {
        "inProgress" | "in_progress" | "started" | "running" => ("Running", false),
        "completed" | "done" => ("Done", false),
        "failed" | "error" => ("Failed", true),
        "unknownOutcome" => ("Outcome unknown", true),
        "stopped" | "interrupted" | "cancelled" | "canceled" => ("Stopped", false),
        "" => ("Unknown", false),
        other => (other, false),
    }
}

#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
fn chat_event_icon(
    ui: &mut egui::Ui,
    openness: f32,
    response: &egui::Response,
    icon: theme::Icon,
    running: bool,
    color: egui::Color32,
) {
    egui::collapsing_header::paint_default_icon(ui, openness, response);
    let center = response.rect.center() + egui::vec2(ui.spacing().indent / 2.0 + 7.0, 0.0);
    theme::image(ui, icon)
        .fit_to_exact_size(egui::Vec2::splat(14.0))
        .tint(color)
        .paint_at(
            ui,
            egui::Rect::from_center_size(center, egui::Vec2::splat(14.0)),
        );
    if running {
        egui::Spinner::new().size(12.0).color(color).paint_at(
            ui,
            egui::Rect::from_center_size(center + egui::vec2(20.0, 0.0), egui::Vec2::splat(12.0)),
        );
    }
}

#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
struct ConversationMessage {
    role: &'static str,
    key: Option<(String, String, String)>,
    text: String,
    command: Option<crate::conversation::CommandExecution>,
    activity: Option<crate::conversation::Activity>,
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
struct ThreadActionDialog {
    thread: String,
    title: String,
    action: ThreadAction,
    focus: bool,
}
#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
struct ConversationPanel {
    #[cfg(feature = "kimi")]
    kimi: Option<kimi_panel::KimiState>,
    #[cfg(feature = "kimi")]
    active_kimi: bool,
    vault_permission_requested: Option<bool>,
    vault_write_enabled: Option<bool>,
    research_requested: bool,
    collect_references: Option<(String, Option<String>)>,
    relevant_sources: bool,
    research_workspace: Option<uuid::Uuid>,
    relevant_cache: Option<(
        uuid::Uuid,
        Revision,
        String,
        Vec<crate::research_desktop::Passage>,
    )>,
    #[cfg(feature = "codex")]
    config: CodexConfig,
    #[cfg(feature = "codex")]
    adapter: Option<CodexAdapter>,
    project_ready: bool,
    fresh_thread: bool,
    resume_requested: bool,
    pending_thread: Option<Option<String>>,
    #[cfg(feature = "codex")]
    pending_thread_action: Option<String>,
    #[cfg(feature = "codex")]
    thread_action_dialog: Option<ThreadActionDialog>,
    binding_durable: bool,
    persist_project: bool,
    metadata_error: Option<String>,
    threads: Vec<crate::conversation::ThreadSummary>,
    threads_cursor: Option<String>,
    threads_loading: bool,
    threads_append: bool,
    prompt_drafts: HashMap<String, String>,
    unavailable_threads: std::collections::HashSet<String>,
    history_cursor: Option<String>,
    history_loading: bool,
    renaming: bool,
    project_name: String,
    #[cfg(feature = "codex")]
    models: Vec<crate::codex::ModelChoice>,
    #[cfg(feature = "codex")]
    models_cursor: Option<String>,
    #[cfg(feature = "codex")]
    models_loading: bool,
    #[cfg(feature = "codex")]
    selected_model: Option<String>,
    #[cfg(feature = "codex")]
    selected_effort: Option<String>,
    #[cfg(feature = "codex")]
    model_override: bool,
    #[cfg(feature = "codex")]
    controls: TurnControls,
    composer_height: f32,
    context: codex_context::ContextUi,
    context_goal_pending: Option<String>,
    context_submit_pending: bool,
    ready: bool,
    starting: bool,
    active_turn: Option<String>,
    thread: Option<String>,
    disconnecting: bool,
    prompt: String,
    messages: Vec<ConversationMessage>,
    markdown: codex_markdown::Cache,
    #[cfg(feature = "codex")]
    approvals: Vec<ConversationApproval>,
    status: String,
    error: Option<String>,
}
#[cfg(all(target_os = "linux", feature = "codex"))]
impl ConversationPanel {
    #[cfg(test)]
    fn prepare_research_prompt(
        &mut self,
        prompt: String,
        evidence: String,
    ) -> Result<(), &'static str> {
        self.prepare_research_context(prompt, evidence, false)
    }
    fn research_capture(&self) -> research::ConversationCapture {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            return self.kimi_research_capture();
        }

        let Some(project_id) = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.project_id.as_ref())
        else {
            return Default::default();
        };
        if !self.project_ready || !self.binding_durable {
            return Default::default();
        }
        let threads: Vec<_> = self
            .threads
            .iter()
            .filter(|t| !self.unavailable_threads.contains(&t.id))
            .map(|t| crate::research::ThreadRef {
                provider: "codex".into(),
                project_id: Some(project_id.clone()),
                thread_id: t.id.clone(),
                title: t.title.clone(),
            })
            .collect();
        let current = self
            .thread
            .as_ref()
            .and_then(|id| threads.iter().find(|t| &t.thread_id == id))
            .cloned();
        let mut capture = research::ConversationCapture {
            threads,
            current,
            ..Default::default()
        };
        if capture.current.is_none()
            || self.pending_thread.is_some()
            || self.history_loading
            || !self.ready
        {
            return capture;
        }
        // History belongs to the currently confirmed thread; keyed live messages must match too.
        let messages: Vec<_> = self
            .messages
            .iter()
            .filter(|m| {
                m.command.is_none()
                    && m.activity.is_none()
                    && matches!(m.role, "You" | "Codex")
                    && m.key
                        .as_ref()
                        .is_none_or(|(thread, _, _)| Some(thread) == self.thread.as_ref())
            })
            .collect();
        let mut pieces = Vec::new();
        let mut bytes = 0;
        for message in messages.iter().rev() {
            let piece = format!("{}: {}", message.role, message.text);
            if bytes + piece.len() > 24_000 {
                break;
            }
            bytes += piece.len();
            pieces.push(piece);
        }
        pieces.reverse();
        capture.discussion = pieces.join("\n\n");
        if !self.busy() {
            capture.answer = messages
                .iter()
                .rev()
                .find(|m| m.role == "Codex" && !m.text.trim().is_empty())
                .filter(|m| m.text.len() <= 24_000)
                .map(|m| {
                    (
                        m.text.clone(),
                        m.key.as_ref().map(|(_, turn, _)| turn.clone()),
                    )
                });
        }
        capture
    }
    fn prepare_research_context(
        &mut self,
        prompt: String,
        evidence: String,
        external: bool,
    ) -> Result<(), &'static str> {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            return self.prepare_kimi_research(prompt, evidence, external);
        }

        if !self.prompt.is_empty()
            || !self.context.draft.is_empty()
            || self.pending_thread.is_some()
            || self.history_loading
            || self.busy()
            || self.controls.mode == WorkMode::Goal
        {
            return Err(
                "Conversation has a draft, context, or active work. Copy the prepared prompt or finish that work first.",
            );
        }
        if self.controls.approval == crate::codex::ApprovalMode::FullAccess {
            return Err(
                "Choose a sandboxed Access profile in the conversation before preparing grounded research. Full access permits network use.",
            );
        }
        if self.thread.is_none() {
            return Err(
                "Connect and open or create a conversation first. The prepared question and sources remain in Research.",
            );
        }
        self.prompt = prompt;
        self.context
            .draft
            .attachments
            .push(crate::codex::PromptAttachment {
                name: if external {
                    "Research context · external gathering"
                } else {
                    "Research sources"
                }
                .into(),
                text: evidence,
            });
        self.controls.network_access = external;
        self.status = if external {
            "Research draft prepared · external gathering requested · review before sending"
        } else {
            "Research draft prepared · network access disabled · review before sending"
        }
        .into();
        Ok(())
    }
    fn new(config: CodexConfig) -> Self {
        let project_name = config
            .project
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let resume_requested = config.resume_thread.is_some();
        Self {
            #[cfg(feature = "kimi")]
            kimi: None,
            #[cfg(feature = "kimi")]
            active_kimi: false,
            vault_permission_requested: None,
            vault_write_enabled: None,
            research_requested: false,
            collect_references: None,
            relevant_sources: false,
            research_workspace: None,
            relevant_cache: None,
            config,
            adapter: None,
            project_name,
            project_ready: false,
            fresh_thread: false,
            resume_requested,
            pending_thread: None,
            pending_thread_action: None,
            thread_action_dialog: None,
            binding_durable: false,
            persist_project: false,
            metadata_error: None,
            threads: Vec::new(),
            threads_cursor: None,
            threads_loading: false,
            threads_append: false,
            prompt_drafts: HashMap::new(),
            unavailable_threads: Default::default(),
            history_cursor: None,
            history_loading: false,
            renaming: false,
            models: Vec::new(),
            models_cursor: None,
            models_loading: false,
            selected_model: None,
            selected_effort: None,
            model_override: false,
            controls: Default::default(),
            composer_height: 160.0,
            context: Default::default(),
            context_goal_pending: None,
            context_submit_pending: false,
            ready: false,
            starting: false,
            active_turn: None,
            thread: None,
            disconnecting: false,
            prompt: String::new(),
            messages: Vec::new(),
            markdown: Default::default(),
            approvals: Vec::new(),
            status: "Disconnected".into(),
            error: None,
        }
    }
    fn accept_turn(&mut self, thread: &str) {
        if self.thread.as_deref() == Some(thread) && self.context_submit_pending {
            self.context.draft = Default::default();
            self.context_submit_pending = false;
        }
    }
    fn start_turn(&mut self, thread: String, turn: String) {
        // A goal can start a turn without accepting our queued message/context.
        if self.thread.as_ref() != Some(&thread) {
            return;
        }
        self.fresh_thread = false;
        self.starting = false;
        self.active_turn = Some(turn);
        self.status = "Streaming…".into();
    }
    fn poll(&mut self, ctx: &egui::Context) {
        #[cfg(feature = "kimi")]
        self.advance_backend_switch();
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            self.poll_kimi(ctx);
            return;
        }

        self.context.poll(ctx);
        for _ in 0..64 {
            let Some(event) = self.adapter.as_ref().and_then(CodexAdapter::try_recv) else {
                return;
            };
            match event {
                CodexEvent::Activity {
                    thread,
                    turn,
                    activity,
                } => self.activity_card(thread, turn, activity),
                CodexEvent::Command {
                    thread,
                    turn,
                    command,
                } => self.command_card(thread, turn, command),
                CodexEvent::CommandOutput {
                    thread,
                    turn,
                    item,
                    text,
                    truncated,
                } => {
                    if self.thread.as_ref() == Some(&thread) {
                        let key = (thread, turn, item);
                        // A delta only belongs to a command that has already started.
                        if self
                            .messages
                            .iter()
                            .any(|m| m.key.as_ref() == Some(&key) && m.command.is_some())
                        {
                            self.message(key.clone(), text, false);
                            if truncated {
                                if let Some(c) = self
                                    .messages
                                    .iter_mut()
                                    .find(|m| m.key.as_ref() == Some(&key))
                                    .and_then(|m| m.command.as_mut())
                                {
                                    c.truncated = true;
                                }
                            }
                        }
                    }
                }
                CodexEvent::TurnRejected { thread, error } => {
                    if self.thread.as_ref() == Some(&thread) || thread.is_empty() {
                        self.context_submit_pending = false;
                        self.starting = false;
                    }
                    self.error = Some(error);
                }
                CodexEvent::GoalFailed { thread, error } => {
                    if self.context_goal_pending.as_ref() == Some(&thread) || thread.is_empty() {
                        self.context_goal_pending = None;
                    }
                    self.error = Some(error);
                }
                CodexEvent::GoalMutationFinished { thread, goal } => {
                    if self.context_goal_pending.as_ref() == Some(&thread) {
                        self.context_goal_pending = None;
                        if self.thread.as_ref() == Some(&thread) {
                            self.accept_goal(goal);
                        }
                    }
                }
                CodexEvent::Goal { thread, goal } => {
                    if self.thread.as_ref() == Some(&thread) {
                        if self.context_goal_pending.is_none() {
                            self.accept_goal(goal);
                        } else {
                            self.context.goal = goal;
                            self.context.goal_loaded = true;
                        }
                    }
                }
                CodexEvent::Plan {
                    thread,
                    steps,
                    text,
                } => {
                    if self.thread.as_ref() == Some(&thread) {
                        if !steps.is_empty() {
                            self.context.plan = steps;
                        }
                        if text.is_some() {
                            self.context.plan_text = text;
                        }
                    }
                }
                CodexEvent::Files { thread, files } => {
                    if self.thread.as_ref() == Some(&thread) {
                        self.context.merge_files(files);
                    }
                }
                CodexEvent::Skills(skills) => {
                    self.context.skills = skills;
                    self.context.capabilities_loaded = true;
                }
                CodexEvent::Connections(connections) => {
                    self.context.connections = connections;
                    self.context.capabilities_loaded = true;
                }
                CodexEvent::Models {
                    models,
                    next_cursor,
                } => {
                    self.models_loading = false;
                    for model in models {
                        if let Some(old) = self.models.iter_mut().find(|m| m.model == model.model) {
                            *old = model;
                        } else if self.models.len() < 128 {
                            self.models.push(model);
                        }
                    }
                    self.models_cursor = next_cursor;
                }
                CodexEvent::ThreadModel {
                    thread,
                    model,
                    effort,
                } => {
                    if self.thread.as_ref() == Some(&thread) {
                        self.selected_model = Some(model);
                        self.selected_effort = effort;
                        self.model_override = false;
                    }
                }
                CodexEvent::ProjectReady { project_id, name } => {
                    if let Some(binding) = &mut self.config.project {
                        binding.project_id = Some(project_id);
                        binding.name = name.clone();
                    }
                    self.project_name = name;
                    self.project_ready = true;
                    if self.models.is_empty() && !self.models_loading {
                        self.models_loading =
                            self.send_request(CodexRequest::ListModels { cursor: None });
                    }
                    self.persist_project = true;
                    self.status = "Connected".into();
                    self.refresh_threads(None);
                }
                CodexEvent::Threads {
                    threads,
                    next_cursor,
                } => {
                    self.threads_loading = false;
                    if !self.threads_append {
                        self.threads.retain(|t| {
                            (self.fresh_thread && Some(&t.id) == self.thread.as_ref())
                                || (self.unavailable_threads.contains(&t.id)
                                    && ((Some(&t.id) == self.thread.as_ref()
                                        && (!self.prompt.is_empty() || self.context.has_draft()))
                                        || self.context.cached_draft(&t.id)
                                        || self
                                            .prompt_drafts
                                            .get(&t.id)
                                            .is_some_and(|p| !p.is_empty())))
                        });
                    }
                    self.unavailable_threads
                        .retain(|id| self.threads.iter().any(|t| &t.id == id));
                    for thread in threads {
                        self.unavailable_threads.remove(&thread.id);
                        if let Some(old) = self.threads.iter_mut().find(|old| old.id == thread.id) {
                            *old = thread;
                        } else if self.threads.len() < 256 {
                            self.threads.push(thread);
                        }
                    }
                    self.threads_cursor = next_cursor;
                }
                CodexEvent::History {
                    thread,
                    messages,
                    next_cursor,
                } => {
                    if self.thread.as_ref() == Some(&thread) {
                        self.history_loading = false;
                        let extra: usize = messages.iter().map(|m| m.text.len()).sum();
                        if self.messages.len() + messages.len() > 256
                            || self.bytes() + extra > Self::BYTE_LIMIT
                        {
                            self.limit_exceeded();
                        } else {
                            let mut older: Vec<_> = messages
                                .into_iter()
                                .map(|m| ConversationMessage {
                                    role: if matches!(m.role.as_str(), "user" | "You") {
                                        "You"
                                    } else {
                                        "Codex"
                                    },
                                    key: None,
                                    text: m.text,
                                    command: None,
                                    activity: None,
                                })
                                .collect();
                            older.append(&mut self.messages);
                            self.messages = older;
                            self.history_cursor = next_cursor;
                        }
                    }
                }
                CodexEvent::ThreadActionFinished { thread, action } => {
                    self.finish_thread_action(&thread, &action);
                }
                CodexEvent::ThreadActionFailed { thread, error } => {
                    if self.pending_thread_action.as_ref() == Some(&thread) {
                        self.pending_thread_action = None;
                    }
                    self.error = Some(error);
                }
                CodexEvent::ThreadStatus { thread, state } => {
                    if let Some(summary) = self.threads.iter_mut().find(|t| t.id == thread) {
                        summary.state = state;
                    }
                    if self.thread.as_ref() == Some(&thread) && self.pending_thread.is_none() {
                        self.status = Self::state_label(state).into();
                    }
                }
                CodexEvent::Connecting => self.status = "Connecting…".into(),
                CodexEvent::Ready { thread } => {
                    self.selected_model = None;
                    self.selected_effort = None;
                    self.model_override = false;
                    self.controls = Default::default();
                    self.context.goal_loaded = false;
                    self.commit_thread_open(thread);
                    self.send_request(CodexRequest::RefreshContext);
                    if self.config.project.is_some() {
                        if self.fresh_thread {
                            if !self
                                .threads
                                .iter()
                                .any(|t| Some(&t.id) == self.thread.as_ref())
                            {
                                self.threads.insert(
                                    0,
                                    crate::conversation::ThreadSummary {
                                        id: self.thread.clone().unwrap(),
                                        title: "New thread".into(),
                                        updated_at: 0,
                                        state: ThreadWorkState::Idle,
                                    },
                                );
                            }
                        } else {
                            self.history_loading =
                                self.send_request(CodexRequest::LoadHistory { cursor: None });
                        }
                        self.refresh_threads(None);
                    }
                }
                CodexEvent::TurnAccepted { thread, .. } => self.accept_turn(&thread),
                CodexEvent::TurnStarted { thread, turn } => self.start_turn(thread, turn),
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
                    self.retire_activity(Some(&thread), Some(&turn), "ended");
                    if self.thread.as_ref() == Some(&thread)
                        && self.active_turn.as_ref() == Some(&turn)
                    {
                        self.starting = false;
                        self.active_turn = None;
                        self.approvals.clear();
                        self.status = format!("Turn {status}");
                        if let Some(summary) = self.threads.iter_mut().find(|t| t.id == thread) {
                            summary.state = if status == "failed" {
                                ThreadWorkState::Error
                            } else {
                                ThreadWorkState::Idle
                            };
                        }
                        if self.project_ready {
                            self.refresh_threads(None);
                        }
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
                    self.models_loading = false;
                    self.error = Some(error);
                    self.threads_loading = false;
                    self.history_loading = false;
                    if self.active_turn.is_none() && self.pending_thread.is_none() {
                        self.starting = false;
                    }
                }
                CodexEvent::Disconnected(error) => {
                    self.retire_activity(None, None, "stopped");
                    self.context.goal_loaded = false;
                    // The worker reports this only after its child has been killed/reaped.
                    self.adapter = None;
                    self.context_submit_pending = false;
                    self.pending_thread = None;
                    self.pending_thread_action = None;
                    self.ready = false;
                    self.project_ready = false;
                    self.models_loading = false;
                    self.models.clear();
                    self.models_cursor = None;
                    self.context.skills.clear();
                    self.context.connections.clear();
                    self.context.capabilities_loaded = false;
                    self.context_goal_pending = None;
                    self.threads_loading = false;
                    self.history_loading = false;
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
                    #[cfg(feature = "kimi")]
                    self.advance_backend_switch();
                    ctx.request_repaint();
                    return;
                }
            }
        }
        ctx.request_repaint();
    }
    fn send_request(&mut self, request: CodexRequest) -> bool {
        match self
            .adapter
            .as_ref()
            .ok_or_else(|| "Connect to Codex first".to_owned())
            .and_then(|a| a.try_send(request))
        {
            Ok(()) => true,
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }
    fn refresh_threads(&mut self, cursor: Option<String>) {
        if self.threads_loading {
            return;
        }
        self.threads_append = cursor.is_some();
        self.threads_loading = self.send_request(CodexRequest::ListThreads { cursor });
    }
    fn select_thread(&mut self, id: Option<String>) {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            self.select_kimi(id);
            return;
        }

        if self.busy() || self.history_loading || !self.binding_durable || !self.project_ready {
            return;
        }
        if id.is_some() && self.thread == id && self.ready {
            return;
        }
        if id.is_none() && self.fresh_thread && self.ready {
            return;
        }
        if self
            .thread
            .as_ref()
            .is_some_and(|id| self.unavailable_threads.contains(id))
            && (!self.prompt.is_empty() || self.context.has_draft())
        {
            self.error = Some(
                "Copy or clear this recovered draft before leaving the removed conversation."
                    .into(),
            );
            return;
        }
        if self.fresh_thread
            && id.is_some()
            && (!self.prompt.trim().is_empty() || self.context.has_draft())
        {
            self.error = Some(
                "Send or clear this new thread's draft before opening another conversation.".into(),
            );
            return;
        }
        let request = id.as_ref().map_or(CodexRequest::CreateThread, |id| {
            CodexRequest::ResumeThread(id.clone())
        });
        if self.bytes() + self.prompt.len() > Self::BYTE_LIMIT || self.prompt_drafts.len() >= 256 {
            self.error = Some(
                "Conversation drafts limit reached; clear a draft before switching threads.".into(),
            );
            return;
        }
        if let Some(target) = id
            .as_ref()
            .filter(|id| self.unavailable_threads.contains(*id))
        {
            let target = target.clone();
            self.begin_thread_open(Some(target.clone()));
            self.commit_thread_open(target);
            self.ready = false;
            self.config.resume_thread = None;
            self.status = "Recovered draft — copy or clear before leaving".into();
            return;
        }
        if self.send_request(request) {
            self.begin_thread_open(id);
        }
    }
    fn begin_thread_open(&mut self, id: Option<String>) {
        // The old conversation owns its draft until the server confirms a new one.
        self.pending_thread = Some(id);
        self.resume_requested = false;
        self.ready = false;
        self.starting = true;
        self.error = None;
        self.status = "Opening conversation…".into();
    }
    fn commit_thread_open(&mut self, thread: String) {
        if let Some(target) = self.pending_thread.take() {
            let carry_prompt = self.fresh_thread && target.is_none();
            self.context.switch(
                self.thread.as_deref().unwrap_or_default(),
                &thread,
                carry_prompt,
            );
            if !carry_prompt {
                self.prompt_drafts.insert(
                    self.thread.clone().unwrap_or_default(),
                    std::mem::take(&mut self.prompt),
                );
                self.prompt = self.prompt_drafts.remove(&thread).unwrap_or_default();
            }
            self.fresh_thread = target.is_none();
            self.messages.clear();
            self.markdown = Default::default();
            self.history_cursor = None;
        }
        self.thread = Some(thread.clone());
        self.config.resume_thread = Some(thread);
        self.ready = true;
        self.starting = false;
        self.status = "Ready".into();
    }
    fn can_thread_action(&self, id: &str, action: &ThreadAction) -> Result<(), &'static str> {
        if !self.project_ready
            || !self.binding_durable
            || self.disconnecting
            || self.busy()
            || self.history_loading
            || self.threads_loading
        {
            return Err("Wait for the Codex connection and pending operation.");
        }
        if self.unavailable_threads.contains(id) {
            return Err("This thread is unavailable; copy or clear its recovered draft.");
        }
        if !self.threads.iter().any(|t| t.id == id) {
            return Err("Thread is no longer listed in this vault project.");
        }
        if self.fresh_thread && self.thread.as_deref() == Some(id) {
            return Err("Send the first message before managing this new thread.");
        }
        if !matches!(action, ThreadAction::Rename(_)) {
            // These service operations also affect descendants. Preserve all local drafts.
            if !self.prompt.is_empty()
                || self.context.any_drafts()
                || self.prompt_drafts.values().any(|draft| !draft.is_empty())
            {
                return Err(
                    "Send or clear conversation drafts before archiving or removing a thread.",
                );
            }
            if self.threads.iter().any(|t| {
                matches!(
                    self.thread_state(t),
                    ThreadWorkState::Working
                        | ThreadWorkState::WaitingOnApproval
                        | ThreadWorkState::WaitingOnInput
                )
            }) {
                return Err("Finish active conversations before archiving or removing a thread.");
            }
        }
        Ok(())
    }
    fn finish_thread_action(&mut self, id: &str, action: &ThreadAction) {
        if self.pending_thread_action.as_deref() == Some(id) {
            self.pending_thread_action = None;
        }
        if self
            .thread_action_dialog
            .as_ref()
            .is_some_and(|d| d.thread == id)
        {
            self.thread_action_dialog = None;
        }
        match action {
            ThreadAction::Rename(name) => {
                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == id) {
                    thread.title = name.clone();
                }
            }
            ThreadAction::Archive | ThreadAction::Remove => {
                let current = self.thread.as_deref() == Some(id);
                let has_draft = (current && (!self.prompt.is_empty() || self.context.has_draft()))
                    || self.context.cached_draft(id)
                    || self.prompt_drafts.get(id).is_some_and(|p| !p.is_empty());
                if has_draft {
                    self.unavailable_threads.insert(id.into());
                    if let Some(row) = self.threads.iter_mut().find(|t| t.id == id) {
                        if !row.title.starts_with("Draft — ") {
                            row.title = format!("Draft — {}", row.title);
                        }
                        row.state = ThreadWorkState::Error;
                    } else {
                        // At most 256 ordinary rows plus one row per bounded cached draft.
                        self.threads.push(crate::conversation::ThreadSummary {
                            id: id.into(),
                            title: "Draft — unavailable conversation".into(),
                            state: ThreadWorkState::Error,
                            updated_at: 0,
                        });
                    }
                } else {
                    self.threads.retain(|t| t.id != id);
                    self.prompt_drafts.remove(id);
                    self.unavailable_threads.remove(id);
                }
                self.context.retire(id, current);
                if current {
                    if !has_draft {
                        self.thread = None;
                        self.messages.clear();
                        self.markdown = Default::default();
                    }
                    self.config.resume_thread = None;
                    self.resume_requested = false;
                    self.ready = false;
                    self.fresh_thread = false;
                    self.active_turn = None;
                    if self.pending_thread.is_none() {
                        self.starting = false;
                    }
                    self.approvals.clear();
                    self.history_loading = false;
                    self.history_cursor = None;
                    self.status = if has_draft {
                        "Recovered draft — copy or clear before leaving"
                    } else {
                        "Connected"
                    }
                    .into();
                }
            }
        }
    }
    fn thread_menu(
        ui: &mut egui::Ui,
        enabled: bool,
        destructive: bool,
        reason: Option<&str>,
    ) -> Option<ThreadAction> {
        let mut action = None;
        for (label, candidate) in [
            ("Rename…", ThreadAction::Rename(String::new())),
            ("Archive…", ThreadAction::Archive),
            ("Remove…", ThreadAction::Remove),
        ] {
            let allowed = enabled && (matches!(candidate, ThreadAction::Rename(_)) || destructive);
            let response = ui.add_enabled(allowed, egui::Button::new(label));
            let response = if !allowed {
                response
                    .on_disabled_hover_text(reason.unwrap_or("Finish the pending operation first."))
            } else {
                response
            };
            if response.clicked() {
                action = Some(candidate);
                ui.close();
            }
        }
        action
    }
    fn thread_action_modal(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.thread_action_dialog.take() else {
            return;
        };
        let mut close = false;
        let mut submit = false;
        let response=egui::Modal::new(egui::Id::new("codex-thread-action")).show(ctx,|ui| {
            ui.set_width(360.0);
            let (heading,button)=match dialog.action {ThreadAction::Rename(_)=>("Rename thread","Rename"),ThreadAction::Archive=>("Archive thread?","Archive"),ThreadAction::Remove=>("Remove thread permanently?","Remove")};
            ui.heading(heading);
            ui.add_space(8.0);
            if let ThreadAction::Rename(name)=&mut dialog.action {
                let field=ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY).char_limit(256));
                if dialog.focus {field.request_focus();dialog.focus=false;}
            } else {
                ui.label(&dialog.title);
                ui.add_space(8.0);
                ui.label(if matches!(dialog.action,ThreadAction::Remove) {"This permanently deletes this conversation and its spawned child threads from Codex. It cannot be undone."} else {"This archives the conversation and attempts to archive its spawned child threads. You can restore archived conversations through Codex."});
            }
            let available=self.can_thread_action(&dialog.thread,&dialog.action);
            if let Err(reason)=available {ui.weak(reason);}
            let valid_name=match &dialog.action {ThreadAction::Rename(name)=>!name.trim().is_empty() && name.len()<=256 && !name.chars().any(char::is_control),_=>true};
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {close=true;}
                if ui.add_enabled(available.is_ok() && valid_name,egui::Button::new(button)).clicked() {submit=true;}
            });
        });
        if response.should_close() {
            close = true;
        }
        if submit
            && self.send_request(CodexRequest::ThreadAction {
                thread: dialog.thread.clone(),
                action: dialog.action.clone(),
            })
        {
            self.pending_thread_action = Some(dialog.thread.clone());
            self.error = None;
        }
        if !close {
            self.thread_action_dialog = Some(dialog);
        }
    }
    fn state_label(state: ThreadWorkState) -> &'static str {
        match state {
            ThreadWorkState::Working => "Working",
            ThreadWorkState::WaitingOnApproval => "Waiting for approval",
            ThreadWorkState::WaitingOnInput => "Waiting for input",
            ThreadWorkState::Idle => "Idle",
            ThreadWorkState::Error => "Error",
            ThreadWorkState::NotLoaded => "Not loaded",
            ThreadWorkState::Unknown => "Status unknown",
        }
    }
    fn thread_state(&self, thread: &crate::conversation::ThreadSummary) -> ThreadWorkState {
        if !self.project_ready || self.disconnecting {
            return ThreadWorkState::Unknown;
        }
        if self.thread.as_deref() == Some(&thread.id) {
            if !self.approvals.is_empty() {
                return ThreadWorkState::WaitingOnApproval;
            }
            if (self.active_turn.is_some() || (self.starting && self.pending_thread.is_none()))
                && matches!(
                    thread.state,
                    ThreadWorkState::Idle | ThreadWorkState::NotLoaded | ThreadWorkState::Unknown
                )
            {
                return ThreadWorkState::Working;
            }
        }
        thread.state
    }
    fn state_indicator(ui: &egui::Ui, center: egui::Pos2, state: ThreadWorkState) {
        let muted = theme::muted(ui);
        match state {
            ThreadWorkState::Working => {
                let colour = egui::Color32::from_rgb(100, 170, 230);
                ui.painter().circle_filled(center, 3.0, colour);
                ui.painter()
                    .circle_stroke(center, 5.0, egui::Stroke::new(1.0, colour));
            }
            ThreadWorkState::WaitingOnApproval | ThreadWorkState::WaitingOnInput => {
                let points = vec![
                    center + egui::vec2(0.0, -5.0),
                    center + egui::vec2(5.0, 0.0),
                    center + egui::vec2(0.0, 5.0),
                    center + egui::vec2(-5.0, 0.0),
                ];
                let colour = egui::Color32::from_rgb(214, 168, 79);
                let fill = if state == ThreadWorkState::WaitingOnApproval {
                    colour
                } else {
                    egui::Color32::TRANSPARENT
                };
                ui.painter().add(egui::Shape::convex_polygon(
                    points,
                    fill,
                    egui::Stroke::new(1.0, colour),
                ));
            }
            ThreadWorkState::Idle => {
                ui.painter()
                    .circle_stroke(center, 3.5, egui::Stroke::new(1.0, muted));
            }
            ThreadWorkState::Error => {
                let stroke = egui::Stroke::new(1.5, ui.visuals().error_fg_color);
                for direction in [-1.0, 1.0] {
                    ui.painter().line_segment(
                        [
                            center + egui::vec2(-3.5, -3.5 * direction),
                            center + egui::vec2(3.5, 3.5 * direction),
                        ],
                        stroke,
                    );
                }
            }
            ThreadWorkState::NotLoaded => {
                ui.painter().line_segment(
                    [center - egui::vec2(3.5, 0.0), center + egui::vec2(3.5, 0.0)],
                    egui::Stroke::new(1.0, muted),
                );
            }
            ThreadWorkState::Unknown => {
                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "?",
                    egui::FontId::proportional(12.0),
                    muted,
                );
            }
        }
    }
    fn sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            self.kimi_sidebar(ui, ctx);
            return;
        }

        ui.horizontal(|ui| {
            ui.strong("Codex");
            ui.add_space(
                (ui.available_width() - 68.0 - 3.0 * ui.spacing().item_spacing.x).max(0.0),
            );
            let connected = self.adapter.is_some();
            let label = if self.disconnecting {
                "Disconnecting…"
            } else if connected {
                "Disconnect Codex"
            } else {
                "Connect Codex"
            };
            if theme::selected_button(
                ui,
                theme::Icon::Links,
                label,
                !self.disconnecting && (connected || self.binding_durable),
                connected,
            )
            .clicked()
            {
                if connected {
                    self.disconnect();
                } else {
                    self.connect(ctx);
                }
            }
            if theme::button(
                ui,
                theme::Icon::New,
                "New thread",
                self.project_ready && self.binding_durable && !self.busy() && !self.history_loading,
            )
            .clicked()
            {
                self.select_thread(None);
            }
        });
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.add(theme::image(ui, theme::Icon::Folder));
            let width = (ui.available_width() - 42.0).max(1.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
            let mut job = egui::text::LayoutJob::simple(
                self.project_name.clone(),
                egui::FontId::proportional(14.0),
                ui.visuals().text_color(),
                width,
            );
            job.wrap.max_rows = 1;
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let pos = rect.left_center() - egui::vec2(0.0, galley.size().y * 0.5);
            ui.painter().galley(pos, galley, ui.visuals().text_color());
            let more = theme::button(ui, theme::Icon::More, "Project actions", true);
            egui::Popup::menu(&more).show(|ui| {
                if ui
                    .add_enabled(
                        self.project_ready && self.binding_durable && !self.busy(),
                        egui::Button::new("Rename project…"),
                    )
                    .clicked()
                {
                    self.renaming = true;
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.project_ready && !self.threads_loading,
                        egui::Button::new("Refresh threads"),
                    )
                    .clicked()
                {
                    self.refresh_threads(None);
                    ui.close();
                }
            });
        });
        if self.renaming {
            ui.add(
                egui::TextEdit::singleline(&mut self.project_name)
                    .hint_text("Project name")
                    .char_limit(256),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.project_name.trim().is_empty(),
                        egui::Button::new("Rename"),
                    )
                    .clicked()
                    && self.send_request(CodexRequest::RenameProject(
                        self.project_name.trim().to_owned(),
                    ))
                {
                    self.renaming = false;
                }
                if ui.button("Cancel").clicked() {
                    self.renaming = false;
                    self.project_name = self
                        .config
                        .project
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                }
            });
        }
        let mut selected = None;
        let mut menu_action = None;
        let scroll = egui::ScrollArea::vertical()
            .id_salt("codex-threads")
            .show(ui, |ui| {
                for thread in &self.threads {
                    let current = self.thread.as_deref() == Some(&thread.id);
                    let state = self.thread_state(thread);
                    let label = if self.unavailable_threads.contains(&thread.id) {
                        "Recovered draft: conversation archived or removed"
                    } else if !self.project_ready {
                        "Disconnected"
                    } else {
                        Self::state_label(state)
                    };
                    let enabled = self
                        .can_thread_action(&thread.id, &ThreadAction::Rename(thread.title.clone()));
                    let destructive = self.can_thread_action(&thread.id, &ThreadAction::Archive);
                    ui.push_id(&thread.id, |ui| {
                        ui.horizontal(|ui| {
                            let row = ui
                                .add_enabled(
                                    !self.busy() && !self.history_loading && self.binding_durable,
                                    egui::Button::new("")
                                        .selected(current)
                                        .frame(true)
                                        .frame_when_inactive(current)
                                        .corner_radius(6)
                                        .min_size(egui::vec2(
                                            (ui.available_width() - 42.0).max(1.0),
                                            34.0,
                                        )),
                                )
                                .on_hover_text(label);
                            row.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    format!("{} — {}", thread.title, label),
                                )
                            });
                            let mut job = egui::text::LayoutJob::simple(
                                thread.title.clone(),
                                egui::FontId::proportional(14.0),
                                ui.visuals().text_color(),
                                (row.rect.width() - 40.0).max(1.0),
                            );
                            job.wrap.max_rows = 1;
                            let galley = ui.fonts_mut(|f| f.layout_job(job));
                            let pos =
                                row.rect.left_center() + egui::vec2(10.0, -galley.size().y * 0.5);
                            ui.painter().galley(pos, galley, ui.visuals().text_color());
                            Self::state_indicator(
                                ui,
                                row.rect.right_center() - egui::vec2(13.0, 0.0),
                                state,
                            );
                            row.context_menu(|ui| {
                                if let Some(action) = Self::thread_menu(
                                    ui,
                                    enabled.is_ok(),
                                    destructive.is_ok(),
                                    enabled.err().or(destructive.err()),
                                ) {
                                    menu_action =
                                        Some((thread.id.clone(), thread.title.clone(), action));
                                }
                            });
                            if row.clicked() {
                                selected = Some(thread.id.clone());
                            }
                            let more = theme::button(ui, theme::Icon::More, "Thread actions", true);
                            egui::Popup::menu(&more).show(|ui| {
                                if let Some(action) = Self::thread_menu(
                                    ui,
                                    enabled.is_ok(),
                                    destructive.is_ok(),
                                    enabled.err().or(destructive.err()),
                                ) {
                                    menu_action =
                                        Some((thread.id.clone(), thread.title.clone(), action));
                                }
                            });
                        });
                    });
                }
                if self.threads_loading {
                    ui.weak("Loading…");
                }
                if self.project_ready && !self.threads_loading && self.threads.is_empty() {
                    ui.weak("No threads yet");
                }
            });
        navigation::scroll_focus(ui.ctx(), &scroll);
        if let Some((thread, title, mut action)) = menu_action {
            if let ThreadAction::Rename(name) = &mut action {
                *name = title.clone();
            }
            self.thread_action_dialog = Some(ThreadActionDialog {
                thread,
                title,
                action,
                focus: true,
            });
        }
        if let Some(id) = selected {
            self.select_thread(Some(id));
        }
        if self.threads_cursor.is_some()
            && ui
                .add_enabled(
                    !self.threads_loading && self.threads.len() < 256,
                    egui::Button::new("More threads"),
                )
                .clicked()
        {
            self.refresh_threads(self.threads_cursor.clone());
        }
    }
    fn connect(&mut self, ctx: &egui::Context) {
        if !self.binding_durable {
            return;
        }
        self.resume_requested = self.config.resume_thread.is_some() && !self.fresh_thread;
        if self.bytes() >= Self::BYTE_LIMIT {
            self.error = Some("Clear the conversation before connecting.".into());
        } else {
            let context = ctx.clone();
            match CodexAdapter::spawn(self.config.clone(), move || context.request_repaint()) {
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
    fn effort_label(effort: &str) -> &str {
        match effort {
            "none" => "None",
            "minimal" => "Minimal",
            "low" => "Low",
            "medium" => "Medium",
            "high" => "High",
            "xhigh" => "Extra high",
            "max" => "Max",
            "ultra" => "Ultra",
            other => other,
        }
    }
    fn choose_model(&mut self, model: &str) {
        if let Some(choice) = self.models.iter().find(|m| m.model == model) {
            self.selected_model = Some(choice.model.clone());
            self.selected_effort = choice
                .efforts
                .iter()
                .any(|(id, _)| id == &choice.default_effort)
                .then(|| choice.default_effort.clone());
            self.model_override = true;
        }
    }
    fn model_picker(&mut self, ui: &mut egui::Ui) {
        let choice = self
            .models
            .iter()
            .find(|m| Some(&m.model) == self.selected_model.as_ref());
        let title = choice
            .map(|m| m.name.as_str())
            .or(self.selected_model.as_deref())
            .unwrap_or("Model");
        let effort = self
            .selected_effort
            .as_deref()
            .map(Self::effort_label)
            .unwrap_or("Default");
        let label = format!("{title}  {effort}  ⌄");
        let picker = ui
            .add_enabled(
                self.project_ready && self.ready && !self.busy() && !self.disconnecting,
                egui::Button::new(label).frame(false).corner_radius(8),
            )
            .on_hover_text("Choose model and reasoning effort for the next message");
        let mut chosen = None;
        let mut effort_choice = None;
        let mut list_models = false;
        egui::Popup::menu(&picker)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(270.0);
                ui.set_max_width(300.0);
                ui.spacing_mut().slider_width = 240.0;
                ui.menu_button(title, |ui| {
                    ui.set_min_width(240.0);
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for model in &self.models {
                                if ui
                                    .selectable_label(
                                        self.selected_model.as_ref() == Some(&model.model),
                                        &model.name,
                                    )
                                    .on_hover_text(&model.description)
                                    .clicked()
                                {
                                    chosen = Some(model.model.clone());
                                    ui.close();
                                }
                            }
                        });
                    if self.models_loading {
                        ui.spinner();
                    }
                    if (self.models_cursor.is_some() || self.models.is_empty())
                        && ui
                            .add_enabled(
                                !self.models_loading && self.models.len() < 128,
                                egui::Button::new(if self.models_cursor.is_some() {
                                    "More models…"
                                } else {
                                    "Load models"
                                }),
                            )
                            .clicked()
                    {
                        list_models = true;
                    }
                });
                if let Some(model) = choice {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Reasoning effort");
                        if ui
                            .small_button("↺")
                            .on_hover_text("Reset to this model's default effort")
                            .clicked()
                        {
                            effort_choice = model
                                .efforts
                                .iter()
                                .any(|(id, _)| id == &model.default_effort)
                                .then(|| model.default_effort.clone());
                        }
                    });
                    if !model.efforts.is_empty() {
                        let mut index = model
                            .efforts
                            .iter()
                            .position(|(id, _)| Some(id) == self.selected_effort.as_ref())
                            .or_else(|| {
                                model
                                    .efforts
                                    .iter()
                                    .position(|(id, _)| id == &model.default_effort)
                            })
                            .unwrap_or(0);
                        ui.label(Self::effort_label(&model.efforts[index].0));
                        if ui
                            .add(
                                egui::Slider::new(&mut index, 0..=model.efforts.len() - 1)
                                    .show_value(false)
                                    .handle_shape(egui::style::HandleShape::Circle),
                            )
                            .on_hover_text(&model.efforts[index].1)
                            .changed()
                        {
                            effort_choice = Some(model.efforts[index].0.clone());
                        }
                        ui.add(
                            egui::Label::new(egui::RichText::new(&model.efforts[index].1).weak())
                                .wrap(),
                        );
                    } else {
                        ui.weak("This model does not offer reasoning choices.");
                    }
                } else {
                    ui.weak(if self.models_loading {
                        "Loading available models…"
                    } else {
                        "Choose a model to see supported reasoning efforts."
                    });
                }
            });
        if let Some(model) = chosen {
            self.choose_model(&model);
        }
        if let Some(effort) = effort_choice {
            self.selected_effort = Some(effort);
            self.model_override = true;
        }
        if list_models {
            self.models_loading = self.send_request(CodexRequest::ListModels {
                cursor: self.models_cursor.clone(),
            });
        }
    }
    fn context_action(&mut self, action: codex_context::Action) {
        let request = match action {
            codex_context::Action::Refresh => CodexRequest::RefreshContext,
            codex_context::Action::SetGoal(objective) => {
                if !self.ready
                    || self.busy()
                    || self.history_loading
                    || self.context_goal_pending.is_some()
                {
                    return;
                }
                self.context_goal_pending = self.thread.clone();
                CodexRequest::SetGoal { objective }
            }
            codex_context::Action::PauseGoal => {
                if !self.ready
                    || self.history_loading
                    || self.pending_thread.is_some()
                    || self.pending_thread_action.is_some()
                    || self.context_goal_pending.is_some()
                {
                    return;
                }
                self.context_goal_pending = self.thread.clone();
                CodexRequest::PauseGoal
            }
            codex_context::Action::ClearGoal => {
                if !self.ready
                    || self.history_loading
                    || self.pending_thread.is_some()
                    || self.pending_thread_action.is_some()
                    || self.context_goal_pending.is_some()
                {
                    return;
                }
                self.context_goal_pending = self.thread.clone();
                CodexRequest::ClearGoal
            }
            codex_context::Action::DraftSketch => {
                let request = "Create a visual sketch/mockup of the idea below. Use available image/visualization capabilities, and include the result or its saved file path.\n\n";
                self.prompt.insert_str(0, request);
                return;
            }
            codex_context::Action::DraftPlan => {
                if self.prompt.is_empty() {
                    self.prompt =
                        "Please propose a plan for this task before implementing it.\n\n".into();
                } else {
                    self.prompt
                        .push_str("\n\nPlease propose a plan before implementing this task.");
                }
                return;
            }
        };
        let setting_goal = matches!(&request, CodexRequest::SetGoal { .. });
        let leaving_goal = matches!(&request, CodexRequest::PauseGoal | CodexRequest::ClearGoal);
        if self.send_request(request) {
            if setting_goal {
                self.controls.mode = WorkMode::Goal;
                self.controls.network_access = false;
            } else if leaving_goal && self.controls.mode == WorkMode::Goal {
                self.controls.mode = WorkMode::Work;
            }
        } else {
            self.context_goal_pending = None;
        }
    }
    fn accept_goal(&mut self, goal: Option<crate::codex::GoalInfo>) {
        if goal.as_ref().is_some_and(|g| g.status == "active") {
            self.controls.mode = WorkMode::Goal;
        } else if self.controls.mode == WorkMode::Goal {
            self.controls.mode = WorkMode::Work;
        }
        self.context.goal = goal;
        self.context.goal_loaded = true;
    }
    fn mode_consistent(&self) -> bool {
        let active = self
            .context
            .goal
            .as_ref()
            .is_some_and(|g| g.status == "active");
        self.context.goal_loaded && (active == (self.controls.mode == WorkMode::Goal))
    }
    fn select_mode(&mut self, mode: WorkMode) {
        if !self.ready || self.busy() || self.history_loading || !self.context.goal_loaded {
            return;
        }
        if mode == WorkMode::Goal {
            if let Some(goal) = &self.context.goal {
                if goal.status == "active" {
                    self.controls.mode = mode;
                    self.controls.network_access = false;
                } else if goal.status != "complete" {
                    self.context_action(codex_context::Action::SetGoal(goal.objective.clone()));
                } else {
                    self.context.open_goal_dialog(self.prompt.clone());
                }
            } else {
                self.context.open_goal_dialog(self.prompt.clone());
            }
        } else {
            self.controls.mode = mode;
            self.controls.network_access = mode == WorkMode::Research;
            if self
                .context
                .goal
                .as_ref()
                .is_some_and(|g| g.status == "active")
            {
                self.context_action(codex_context::Action::PauseGoal);
            }
        }
    }
    fn turn_pickers(&mut self, ui: &mut egui::Ui) {
        let enabled = self.ready && !self.busy() && !self.history_loading && !self.disconnecting;
        let approval = ui
            .add_enabled(
                enabled,
                egui::Button::new(match self.controls.approval {
                    ApprovalMode::Ask => "Ask approval ⌄",
                    ApprovalMode::AutoReview => "Approve for me ⌄",
                    ApprovalMode::NoPrompts => "No prompts ⌄",
                    ApprovalMode::FullAccess => "Full access ⌄",
                })
                .frame(false)
                .corner_radius(8),
            )
            .on_hover_text(
                "Command approval mode for the next message; MCP tool approvals are separate",
            );
        egui::Popup::menu(&approval).show(|ui| {
            ui.set_max_width(320.0);
            for (mode, label, description) in [
                (ApprovalMode::Ask, "Ask for approval", "Send command and tool approval requests to you."),
                (ApprovalMode::AutoReview, "Approve for me", "Route eligible approvals to Codex automatic review; retain the sandbox."),
                (ApprovalMode::NoPrompts, "No prompts", "Never ask for command approval. Keep the sandbox/network limits; blocked actions can fail. MCP tools retain their separate approval policy."),
                (ApprovalMode::FullAccess, "Full access", "Remove the command sandbox and command approval prompts. File/network access is unrestricted. MCP tools may still request approval under their separate policy."),
            ] {
                if ui.selectable_label(self.controls.approval == mode, label).on_hover_text(description).clicked() {
                    self.controls.approval = mode; ui.close();
                }
            }
        });
        let access = ui.add_enabled(
            enabled,
            egui::Button::new(if self.controls.approval == ApprovalMode::FullAccess {
                "Access · unrestricted ⌄"
            } else if self.controls.network_access {
                "Access · online ⌄"
            } else {
                "Access ⌄"
            })
            .frame(false)
            .corner_radius(8),
        );
        egui::Popup::menu(&access).show(|ui| {
            ui.set_max_width(320.0);
            ui.strong("Execution access");
            if self.controls.approval == ApprovalMode::FullAccess {
                ui.label("Full access: commands can read/write files and use the network without sandbox restrictions.");
                let mut enabled=true;
                ui.add_enabled(false,egui::Checkbox::new(&mut enabled,"Network included in Full access"));
            } else {
                ui.label("Commands can run inside the read-only sandbox.");
                ui.weak(if self.controls.approval==ApprovalMode::NoPrompts {"Requests needing broader access are rejected instead of prompting."} else {"Commands that need broader access can request approval. Allow once grants the requested command."});
                ui.checkbox(&mut self.controls.network_access,"Allow network access").on_hover_text("Applies to commands in subsequent messages in this conversation. Managed restrictions may still apply.");
            }
            ui.weak("Vault tools use the owner's independent MCP permissions. Attached files remain explicit snapshots.");
        });
        // Keep profile and model together when the context panel narrows the composer.
        let remaining = ui.available_rect_before_wrap().width();
        if remaining < 350.0 {
            ui.allocate_space(egui::vec2(remaining.max(0.0), 0.0));
        }
        let mode = ui
            .add_enabled(
                enabled && self.context.goal_loaded,
                egui::Button::new(match self.controls.mode {
                    WorkMode::Work => "Work ⌄",
                    WorkMode::Plan => "Plan ⌄",
                    WorkMode::Research => "Research ⌄",
                    WorkMode::Goal => "Goal ⌄",
                })
                .frame(false)
                .corner_radius(8),
            )
            .on_hover_text("Work, plan, research, or work toward a saved goal");
        let mut selected = None;
        egui::Popup::menu(&mode).show(|ui| {
            for (mode, label) in [
                (WorkMode::Work, "Work"),
                (WorkMode::Plan, "Plan"),
                (WorkMode::Research, "Research"),
                (WorkMode::Goal, "Goal"),
            ] {
                if ui
                    .selectable_label(self.controls.mode == mode, label)
                    .clicked()
                {
                    selected = Some(mode);
                    ui.close();
                }
            }
        });
        if let Some(mode) = selected {
            self.select_mode(mode);
        }
    }
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &App,
        selected: Option<NoteId>,
    ) {
        self.show_surface(ui, ctx, app, selected, false);
    }
    fn show_surface(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &App,
        selected: Option<NoteId>,
        research: bool,
    ) {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            self.show_kimi(ui, ctx, app, selected, research);
            return;
        }

        let title = self
            .thread
            .as_ref()
            .and_then(|id| self.threads.iter().find(|t| &t.id == id))
            .map(|t| t.title.as_str())
            .unwrap_or("Codex")
            .to_owned();
        if !research {
            #[cfg(feature = "kimi")]
            self.backend_picker(ui);
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 92.0).max(40.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(width, 28.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(&title).size(18.0)).truncate())
                            .on_hover_text(&title);
                    },
                );
                if ui
                    .add(
                        egui::Button::new("Context")
                            .selected(
                                if ctx
                                    .data(|d| {
                                        d.get_temp::<bool>(egui::Id::new("shell-context-undocked"))
                                    })
                                    .unwrap_or(false)
                                {
                                    ctx.data(|d| {
                                        d.get_temp::<Option<layout::Pane>>(egui::Id::new(
                                            "shell-active-overlay",
                                        ))
                                    })
                                    .flatten()
                                        == Some(layout::Pane::Context)
                                } else {
                                    self.context.open
                                },
                            )
                            .corner_radius(6),
                    )
                    .on_hover_text("Show goal, plan, files and plugins")
                    .clicked()
                {
                    if ctx
                        .data(|d| d.get_temp::<bool>(egui::Id::new("shell-context-undocked")))
                        .unwrap_or(false)
                    {
                        ctx.data_mut(|d| {
                            d.insert_temp(
                                egui::Id::new("shell-pane-request"),
                                Some(layout::Pane::Context),
                            )
                        });
                    } else {
                        self.context.open = !self.context.open;
                    }
                }
            });
        }
        if !research || self.busy() || self.history_loading {
            ui.weak(&self.status);
        }
        if let Some(error) = &self.metadata_error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Project metadata could not be saved: {error}"),
            );
            if ui.button("Retry saving project association").clicked() {
                self.persist_project = true;
            }
        }
        if self.adapter.is_some() || research || self.vault_write_enabled.is_some() {
            ui.horizontal(|ui| {
                if research {
                    ui.label(egui::RichText::new(&title).small().color(theme::muted(ui)));
                }
                let more = theme::button(ui, theme::Icon::More, "Conversation actions", true);
                egui::Popup::menu(&more).show(|ui| {
                    ui.set_max_width(340.0);
                    if let Some(allowed) = self.vault_write_enabled {
                        ui.menu_button(
                            if allowed {
                                "Vault updates · Allowed"
                            } else {
                                "Vault updates · Read only"
                            },
                            |ui| {
                                ui.set_max_width(320.0);
                                if let Some(enabled) = agent_permission_controls(ui, app, allowed) {
                                    self.vault_permission_requested = Some(enabled);
                                }
                            },
                        );
                        ui.separator();
                    }
                    if research {
                        ui.weak(&title);
                        if ui.button("Conversation context…").clicked() {
                            self.context.open = !self.context.open;
                            ui.close();
                        }
                    } else if ui
                        .add_enabled(
                            self.research_capture().current.is_some()
                                && !self.busy()
                                && !self.history_loading,
                            egui::Button::new("Start research from discussion…"),
                        )
                        .clicked()
                    {
                        self.research_requested = true;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            !self.busy() && !self.history_loading && self.approvals.is_empty(),
                            egui::Button::new("Clear displayed messages"),
                        )
                        .clicked()
                    {
                        self.messages.clear();
                        self.markdown = Default::default();
                        self.error = None;
                        ui.close();
                    }
                });
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if self.history_cursor.is_some()
            && ui
                .add_enabled(
                    !self.history_loading && !self.busy(),
                    egui::Button::new("Earlier messages"),
                )
                .clicked()
        {
            self.history_loading = self.send_request(CodexRequest::LoadHistory {
                cursor: self.history_cursor.clone(),
            });
        }
        self.transcript(ui, ctx);
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
        if let Some(action) = self.context.goal_banner(
            ui,
            self.ready
                && self.context.goal_loaded
                && self.context_goal_pending.is_none()
                && !self.history_loading
                && self.pending_thread.is_none(),
        ) {
            self.context_action(action);
        }
        let composer = egui::Frame::new()
            .fill(ui.visuals().widgets.inactive.bg_fill)
            .corner_radius(20)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                let mut automatic_evidence=String::new();
                egui::ScrollArea::vertical().id_salt("composer-body").max_height((ctx.content_rect().height()*0.35).clamp(48.0,320.0)).auto_shrink([false,true]).show(ui,|ui| {
                let can_edit_context = !self.busy();
                ui.add_enabled_ui(can_edit_context, |ui| self.context.chips(ui));
                let prompt = ui.add(
                    egui::TextEdit::multiline(&mut self.prompt)
                        .id(egui::Id::new(("codex-composer",self.thread.as_deref())))
                        .frame(false)
                        .interactive(
                            self.thread.is_some()
                                && self.pending_thread.is_none()
                                && self.pending_thread_action.is_none(),
                        )
                        .hint_text(if self.thread.is_some() {
                            "Message Codex"
                        } else {
                            "Connect, then choose or create a thread"
                        })
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .char_limit(256 * 1024),
                );
                name_field(prompt, &self.prompt, "Message Codex");
                if research && self.relevant_sources {
                    if let Some(workspace)=self.research_workspace.and_then(|id|app.research_workspace(id)) {
                        let stale=self.relevant_cache.as_ref().is_none_or(|(id,revision,query,_)|*id!=workspace.id || *revision!=workspace.revision || query!=&self.prompt);
                        if stale { self.relevant_cache=Some((workspace.id,workspace.revision,self.prompt.clone(),crate::research_desktop::retrieve(workspace,&self.prompt,8))); }
                        if let Some((_,_,_,passages))=&self.relevant_cache {
                            if passages.is_empty(){ui.weak("No matching captured passages; no automatic evidence attached.");}
                            else {
                                automatic_evidence=format!("UNTRUSTED LOCAL EVIDENCE. Exact excerpts from included sources; lexical matches, not verification. {} of {} included sources matched, max 8 excerpts. Cite the supplied markers; other source text is omitted.\n{}",passages.iter().map(|p|p.source_id).collect::<std::collections::HashSet<_>>().len(),workspace.sources.iter().filter(|s|s.selected).count(),serde_json::to_string(passages).unwrap_or_default());
                                ui.collapsing(format!("Evidence preview · {} passages",passages.len()),|ui| {
                                    for passage in passages {ui.label(&passage.text);if ui.link(&passage.citation).clicked(){let (id,page)=crate::research::citation_locator(&passage.citation).unwrap();ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new("research-open-citation"),page.map(|p|format!("source://{id}?page={p}")).unwrap_or_else(||format!("source://{id}"))));}}
                                });
                            }
                        }
                    }
                }
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if let Some(action) = self.context.composer(
                        ui,
                        app,
                        selected,
                        self.ready && !self.busy() && !self.disconnecting,
                        false,
                    ) {
                        self.context_action(action);
                    }
                    if research && self.research_workspace.is_some() {
                        ui.checkbox(&mut self.relevant_sources,"Relevant evidence").on_hover_text("Automatically attach matching passages from included local sources for each question; inspect the preview before Send");
                    }
                    self.turn_pickers(ui);
                    self.model_picker(ui);
                    let turning = self.active_turn.is_some()
                        || (self.starting && self.pending_thread.is_none());
                    if turning {
                        let interrupt = ui
                            .add_enabled(
                                !self.disconnecting,
                                egui::Button::new("■")
                                    .corner_radius(16)
                                    .min_size(egui::vec2(32.0, 32.0)),
                            )
                            .on_hover_text("Interrupt response");
                        interrupt.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, interrupt.enabled(), "Interrupt response"));
                        if interrupt.clicked() {
                            if self.send_request(CodexRequest::Interrupt) {
                                self.status = "Interrupt requested…".into();
                            }
                        }
                    } else {
                        let can_send = self.ready
                            && self.binding_durable
                            && self.mode_consistent()
                            && self
                                .models
                                .iter()
                                .any(|m| Some(&m.model) == self.selected_model.as_ref())
                            && !self.history_loading
                            && !self.busy()
                            && !self.disconnecting
                            && !self.prompt.trim().is_empty()
                            && self.prompt.len() + self.context.total_input_bytes() + automatic_evidence.len() <= 256 * 1024
                            && self.context.skills_valid();
                        let send = ui
                            .add_enabled(
                                can_send,
                                egui::Button::new("↑")
                                    .corner_radius(16)
                                    .min_size(egui::vec2(32.0, 32.0)),
                            )
                            .on_hover_text("Send message");
                        send.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, send.enabled(), "Send message"));
                        if send.clicked() {
                            if self.bytes() + self.prompt.len() > Self::BYTE_LIMIT
                                || self.messages.len() >= 256
                            {
                                self.limit_exceeded();
                            } else {
                                let request = CodexRequest::SendPromptWithControls {
                                    text: self.prompt.clone(),
                                    model: self.selected_model.clone(),
                                    effort: self.selected_effort.clone(),
                                    attachments: {
                                        let mut attachments=self.context.draft.attachments.clone();
                                        if !automatic_evidence.is_empty(){attachments.push(crate::codex::PromptAttachment {name:"Relevant local evidence".into(),text:automatic_evidence.clone()});}
                                        attachments
                                    },
                                    skills: self.context.draft.skills.clone(),
                                    controls: self.controls,
                                };
                                if self.send_request(request) {
                                    self.context_submit_pending = true;
                                    self.messages.push(ConversationMessage {
                                        role: "You",
                                        key: None,
                                        text: std::mem::take(&mut self.prompt),
                                        command: None,
                                        activity: None,
                                    });
                                    self.starting = true;
                                    self.status = "Starting turn…".into();
                                    self.error = None;
                                }
                            }
                        }
                    }
                });
            });
        self.composer_height = composer.response.rect.height() + ui.spacing().item_spacing.y;
        if self.prompt.len() + self.context.total_input_bytes() > 256 * 1024 {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Message and attached context exceed 256 KiB; shorten or detach before sending.",
            );
        }
        if let Some(action) = self.context.dialogs(
            ctx,
            self.ready && !self.busy() && self.context_goal_pending.is_none(),
        ) {
            self.context_action(action);
        }
        self.thread_action_modal(ctx);
    }
}

#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
impl ConversationPanel {
    const BYTE_LIMIT: usize = 2 * 1024 * 1024;
    fn busy(&self) -> bool {
        self.starting
            || self.active_turn.is_some()
            || self.has_pending_provider_operation()
            || self.context.loading()
            || self.context_goal_pending.is_some()
            || self.context_submit_pending
    }
    fn bytes(&self) -> usize {
        self.messages
            .iter()
            .map(|message| {
                message.text.len()
                    + message.activity.as_ref().map_or(0, |a| {
                        a.item.len() + a.kind.len() + a.title.len() + a.status.len()
                    })
                    + message.command.as_ref().map_or(0, |c| {
                        c.command.len() + c.cwd.len() + c.status.len() + c.item.len()
                    })
                    + message.key.as_ref().map_or(0, |(thread, turn, item)| {
                        thread.len() + turn.len() + item.len()
                    })
            })
            .sum::<usize>()
            + self.prompt_drafts.values().map(String::len).sum::<usize>()
            + self.context.bytes()
            + self.approval_bytes()
    }
    fn disconnect(&mut self) {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            self.disconnect_kimi();
            return;
        }
        #[cfg(feature = "codex")]
        if !self.disconnecting {
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
                role: self.provider_name(),
                key: Some(key),
                text,
                command: None,
                activity: None,
            });
        }
    }
    fn command_card(
        &mut self,
        thread: String,
        turn: String,
        mut command: crate::conversation::CommandExecution,
    ) {
        if self.thread.as_ref() != Some(&thread) {
            return;
        }
        let key = (thread, turn, command.item.clone());
        if !command.output_complete {
            if let Some(previous) = self.messages.iter().find(|m| m.key.as_ref() == Some(&key)) {
                command.output = previous.text.clone();
                command.truncated |= previous.command.as_ref().is_some_and(|c| c.truncated);
            }
        }
        let output = std::mem::take(&mut command.output);
        let metadata_bytes =
            command.command.len() + command.cwd.len() + command.status.len() + command.item.len();
        if self.bytes() + output.len() + metadata_bytes > Self::BYTE_LIMIT {
            self.limit_exceeded();
            return;
        }
        self.message(key.clone(), output, true);
        if let Some(message) = self
            .messages
            .iter_mut()
            .find(|m| m.key.as_ref() == Some(&key))
        {
            message.command = Some(command);
        }
    }
    fn activity_card(
        &mut self,
        thread: String,
        turn: String,
        mut activity: crate::conversation::Activity,
    ) {
        if self.thread.as_ref() != Some(&thread) {
            return;
        }
        let key = (thread, turn, activity.item.clone());
        let details = std::mem::take(&mut activity.details);
        let metadata = activity.item.len()
            + activity.kind.len()
            + activity.title.len()
            + activity.status.len();
        if self.bytes() + details.len() + metadata > Self::BYTE_LIMIT {
            self.limit_exceeded();
            return;
        }
        self.message(key.clone(), details, true);
        if let Some(message) = self
            .messages
            .iter_mut()
            .find(|m| m.key.as_ref() == Some(&key))
        {
            message.activity = Some(activity);
        }
    }
    fn retire_activity(&mut self, thread: Option<&str>, turn: Option<&str>, status: &str) {
        for message in &mut self.messages {
            let Some((owner, work, _)) = &message.key else {
                continue;
            };
            if thread.is_some_and(|t| t != owner) || turn.is_some_and(|t| t != work) {
                continue;
            }
            if let Some(activity) = &mut message.activity {
                if matches!(
                    activity.status.as_str(),
                    "inProgress" | "in_progress" | "started" | "running"
                ) {
                    activity.status = status.into();
                }
            }
            if let Some(command) = &mut message.command {
                if matches!(
                    command.status.as_str(),
                    "inProgress" | "in_progress" | "started" | "running"
                ) {
                    command.status = status.into();
                }
            }
        }
    }
    fn plan_rows(&mut self, ui: &mut egui::Ui) {
        if self.context.plan.is_empty() && self.context.plan_text.is_none() {
            return;
        }
        egui::CollapsingHeader::new("Plan")
            .id_salt(("conversation-plan", &self.thread))
            .default_open(self.context.plan.iter().any(|s| s.status == "inProgress"))
            .show(ui, |ui| {
                if let Some(text) = &self.context.plan_text {
                    self.markdown.show(ui, ("plan-text", &self.thread), text);
                }
                for step in &self.context.plan {
                    ui.horizontal_top(|ui| {
                        match step.status.as_str() {
                            "inProgress" | "in_progress" if self.active_turn.is_some() => {
                                ui.add(egui::Spinner::new().size(12.0));
                            }
                            "inProgress" | "in_progress" => {
                                ui.weak("◉");
                            }
                            "completed" => {
                                ui.weak("✓");
                            }
                            _ => {
                                ui.weak("○");
                            }
                        }
                        ui.add(egui::Label::new(&step.text).wrap())
                            .on_hover_text(format!("Plan step · {}", step.status));
                    });
                }
            });
        ui.add_space(12.0);
    }
    fn transcript(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::ScrollArea::vertical()
            .id_salt("codex-transcript")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .max_height(
                (ui.available_height()
                    - self.composer_height
                    - if self.context.goal.is_some() {
                        44.0
                    } else {
                        0.0
                    }
                    - self.approval_count() as f32 * 160.0)
                    .max(100.0),
            )
            .show(ui, |ui| {
                for (index, message) in self.messages.iter().enumerate() {
                    if let Some(activity) = &message.activity {
                        let (status, failed) = chat_status(&activity.status, None);
                        let running = status == "Running"
                            && message.key.as_ref().is_some_and(|(thread, turn, _)| {
                                self.thread.as_ref() == Some(thread)
                                    && self.active_turn.as_ref() == Some(turn)
                            });
                        let kind = match activity.kind.as_str() {
                            "webSearch" => "Search",
                            "mcpToolCall" => "Tool",
                            "fileChange" => "Files",
                            other => other,
                        };
                        let icon = match activity.kind.as_str() {
                            "webSearch" => theme::Icon::Search,
                            "mcpToolCall" => theme::Icon::Tool,
                            _ => theme::Icon::Notes,
                        };
                        let color = if failed {
                            theme::colors(ui.visuals().dark_mode).error
                        } else {
                            theme::muted(ui)
                        };
                        let width = (ui.available_width()
                            - ui.spacing().indent
                            - 2.0 * ui.spacing().button_padding.x)
                            .max(1.0);
                        let mut title = egui::text::LayoutJob::simple(
                            format!("{kind} · {status} — {}", activity.title),
                            egui::FontId::proportional(13.0),
                            color,
                            width,
                        );
                        // Reserve actual layout space for the topical icon and optional spinner.
                        title.sections[0].leading_space = if running { 40.0 } else { 20.0 };
                        title.wrap = egui::text::TextWrapping::truncate_at_width(width);
                        let title = ui.fonts_mut(|fonts| fonts.layout_job(title));
                        let header = egui::CollapsingHeader::new(title)
                            .id_salt(("activity", &message.key))
                            .icon(move |ui, openness, response| {
                                chat_event_icon(ui, openness, response, icon, running, color)
                            });
                        header.show(ui, |ui| {
                            ui.weak(format!("{kind} · {status}"));
                            egui::ScrollArea::both()
                                .id_salt(("activity-details", &message.key))
                                .max_height(200.0)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(&activity.title).selectable(true).wrap(),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&message.text)
                                                .monospace()
                                                .size(12.0),
                                        )
                                        .selectable(true),
                                    );
                                });
                            if activity.truncated {
                                ui.weak("Details truncated at 8 KiB.");
                            }
                        });
                    } else if let Some(command) = &message.command {
                        let (status, failed) = chat_status(&command.status, command.exit_code);
                        let running = status == "Running"
                            && message.key.as_ref().is_some_and(|(thread, turn, _)| {
                                self.thread.as_ref() == Some(thread)
                                    && self.active_turn.as_ref() == Some(turn)
                            });
                        let color = if failed {
                            theme::colors(ui.visuals().dark_mode).error
                        } else {
                            theme::muted(ui)
                        };
                        let width = (ui.available_width()
                            - ui.spacing().indent
                            - 2.0 * ui.spacing().button_padding.x)
                            .max(1.0);
                        let mut title = egui::text::LayoutJob::simple(
                            format!("Command · {status} — {}", command.command),
                            egui::FontId::proportional(13.0),
                            color,
                            width,
                        );
                        title.sections[0].leading_space = if running { 40.0 } else { 20.0 };
                        title.wrap = egui::text::TextWrapping::truncate_at_width(width);
                        let title = ui.fonts_mut(|fonts| fonts.layout_job(title));
                        let header = egui::CollapsingHeader::new(title)
                            .id_salt(("command", &message.key))
                            .icon(move |ui, openness, response| {
                                chat_event_icon(
                                    ui,
                                    openness,
                                    response,
                                    theme::Icon::Terminal,
                                    running,
                                    color,
                                )
                            });
                        header.show(ui, |ui| {
                            ui.weak(format!(
                                "{} · {}{}",
                                command.cwd,
                                status,
                                command
                                    .exit_code
                                    .map(|c| format!(" · exit {c}"))
                                    .unwrap_or_default()
                            ));
                            egui::ScrollArea::both()
                                .id_salt(("command-output", &message.key))
                                .max_height(240.0)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(&command.command).selectable(true).wrap(),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&message.text).monospace(),
                                        )
                                        .selectable(true),
                                    );
                                });
                            if command.truncated {
                                ui.weak("Output truncated at 64 KiB.");
                            }
                        });
                    } else if message.role == "You" {
                        egui::Frame::new()
                            .fill(theme::colors(ui.visuals().dark_mode).selected)
                            .corner_radius(14)
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.strong("You");
                                ui.add(egui::Label::new(&message.text).selectable(true).wrap());
                            });
                    } else {
                        ui.strong(self.provider_name());
                        self.markdown
                            .show(ui, (&self.thread, &message.key, index), &message.text);
                        let copy =
                            theme::button(ui, theme::Icon::Copy, "Copy response Markdown", true);
                        copy.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Copy response Markdown",
                            )
                        });
                        if copy.on_hover_text("Copy response Markdown").clicked() {
                            ui.ctx().copy_text(message.text.clone());
                        }
                        let candidates = crate::research_desktop::references(&message.text);
                        if !candidates.is_empty()
                            && ui
                                .add(
                                    egui::Button::new(format!("Sources · {}", candidates.len()))
                                        .frame(false)
                                        .small(),
                                )
                                .on_hover_text("Review references from this answer")
                                .clicked()
                        {
                            self.collect_references = Some((
                                message.text.clone(),
                                message.key.as_ref().map(|(_, turn, _)| turn.clone()),
                            ));
                        }
                    }
                    ui.add_space(if message.command.is_some() || message.activity.is_some() {
                        6.0
                    } else {
                        20.0
                    });
                }
                self.context.transcript_files(ui, ctx, &self.provider_cwd());
                self.plan_rows(ui);
                if self.starting || self.active_turn.is_some() {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(12.0));
                        ui.weak(if self.starting {
                            "Starting…"
                        } else {
                            "Working…"
                        });
                    });
                }
            });
    }

    fn is_kimi(&self) -> bool {
        #[cfg(feature = "kimi")]
        {
            return self.active_kimi;
        }
        #[cfg(not(feature = "kimi"))]
        false
    }
    fn provider_name(&self) -> &'static str {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            return "Kimi";
        }
        "Codex"
    }
    fn provider_cwd(&self) -> std::path::PathBuf {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            return self
                .kimi
                .as_ref()
                .map(|k| k.config.cwd.clone())
                .unwrap_or_default();
        }
        #[cfg(feature = "codex")]
        {
            return self.config.cwd.clone();
        }
        #[cfg(not(feature = "codex"))]
        std::path::PathBuf::new()
    }
    fn has_adapter(&self) -> bool {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            return self.kimi.as_ref().is_some_and(|k| k.adapter.is_some());
        }
        #[cfg(feature = "codex")]
        {
            return self.adapter.is_some();
        }
        #[cfg(not(feature = "codex"))]
        false
    }
    fn has_pending_provider_operation(&self) -> bool {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            return self.kimi.as_ref().is_some_and(|k| k.option_pending);
        }
        #[cfg(feature = "codex")]
        {
            return self.pending_thread_action.is_some();
        }
        #[cfg(not(feature = "codex"))]
        false
    }
    fn approval_count(&self) -> usize {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            return self.kimi.as_ref().map_or(0, |k| k.permissions.len());
        }
        #[cfg(feature = "codex")]
        {
            return self.approvals.len();
        }
        #[cfg(not(feature = "codex"))]
        0
    }
    fn approval_bytes(&self) -> usize {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            return self.kimi.as_ref().map_or(0, |k| k.permission_bytes());
        }
        #[cfg(feature = "codex")]
        {
            return self
                .approvals
                .iter()
                .map(|a| {
                    a.details.len()
                        + a.thread.len()
                        + a.turn.len()
                        + a.method.len()
                        + a.id.to_string().len()
                })
                .sum();
        }
        #[cfg(not(feature = "codex"))]
        0
    }
}

/// Developer-only native preview with deterministic content and no agent runtime.
#[cfg(all(target_os = "linux", feature = "codex", debug_assertions))]
pub fn run_transcript_preview() -> eframe::Result {
    struct Preview {
        app: App,
        panel: ConversationPanel,
    }
    impl eframe::App for Preview {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.panel.context.poll(ctx);
            if self.panel.context.open {
                egui::SidePanel::right("preview-context")
                    .default_width(300.0)
                    .show(ctx, |ui| {
                        let _ =
                            self.panel
                                .context
                                .panel(ui, ctx, &self.panel.config.cwd, false, false);
                    });
            }
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::central_panel(&ctx.style())
                        .inner_margin(egui::Margin::symmetric(32, 20)),
                )
                .show(ctx, |ui| {
                    self.panel.show(ui, ctx, &self.app, None);
                });
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 850.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Tanglewood · conversation preview",
        options,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            theme::palette(&cc.egui_ctx);
            let mut panel = ConversationPanel::new(CodexConfig {
                executable: "codex".into(),
                cwd: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                mcp_bridge: "unused".into(),
                socket: "unused".into(),
                resume_thread: None,
                project: None,
            });
            panel.thread = Some("local-preview".into());
            panel.status = "Local presentation fixture · no agent connected".into();
            panel.messages.push(ConversationMessage {
                role: "You",
                key: None,
                text: "Show a readable research report with code, sources and a checklist.".into(),
                command: None,
                activity: None,
            });
            panel.activity_card(
                "local-preview".into(),
                "fixture".into(),
                crate::conversation::Activity {
                    item: "sources".into(),
                    kind: "webSearch".into(),
                    title: "Compared the reference sources".into(),
                    status: "completed".into(),
                    details: "Local example activity; no search was performed.".into(),
                    truncated: false,
                },
            );
            panel.command_card(
                "local-preview".into(),
                "fixture".into(),
                crate::conversation::CommandExecution {
                    item: "command-failed".into(),
                    command: "cargo test --offline".into(),
                    cwd: env!("CARGO_MANIFEST_DIR").into(),
                    status: "completed".into(),
                    exit_code: Some(101),
                    output:
                        "error: local fixture test failed\nNo command was executed in this preview."
                            .into(),
                    truncated: false,
                    output_complete: true,
                },
            );
            panel.activity_card(
                "local-preview".into(),
                "fixture".into(),
                crate::conversation::Activity {
                    item: "tool-failed".into(),
                    kind: "mcpToolCall".into(),
                    title: "invalid command arguments: missing field `include_trash`".into(),
                    status: "failed".into(),
                    details: "invalid command arguments: missing field `include_trash`".into(),
                    truncated: false,
                },
            );
            panel.activity_card("local-preview".into(), "fixture".into(), crate::conversation::Activity {
                item: "tool-unknown-outcome".into(), kind: "mcpToolCall".into(),
                title: "accepted but storage failed: downloaded content is not a PDF; inspect state before retry".into(),
                status: "unknownOutcome".into(),
                details: "accepted but storage failed: downloaded content is not a PDF; inspect state before retry".into(),
                truncated: false,
            });
            panel.context.files = vec![
                crate::codex::ContextFile {
                    path: "docs/fixtures/mvp-2/codex-response.md".into(),
                    kind: "file".into(),
                },
                crate::codex::ContextFile {
                    path: "docs/design-system/evidence/expanded-dark.png".into(),
                    kind: "image".into(),
                },
                crate::codex::ContextFile {
                    path: "docs/plans/2026-10-27-chat-hierarchy.md".into(),
                    kind: "file".into(),
                },
                crate::codex::ContextFile {
                    path: "Cargo.toml".into(),
                    kind: "file".into(),
                },
            ];
            panel.messages.push(ConversationMessage {
                role: "Codex",
                key: Some(("local-preview".into(), "fixture".into(), "reply".into())),
                text: include_str!("../docs/fixtures/mvp-2/codex-response.md").into(),
                command: None,
                activity: None,
            });
            panel.context.plan = vec![
                crate::codex::PlanStep {
                    text: "Compare sources".into(),
                    status: "completed".into(),
                },
                crate::codex::PlanStep {
                    text: "Review the report".into(),
                    status: "pending".into(),
                },
            ];
            Ok(Box::new(Preview {
                app: App::new(),
                panel,
            }))
        }),
    )
}

/// Developer-only research layout fixture using a disposable vault and no agent runtime.
#[cfg(all(target_os = "linux", feature = "codex", debug_assertions))]
pub fn run_research_preview(vault: std::path::PathBuf) -> eframe::Result {
    let mut ui = TanglewoodUi::new(App::open(&vault).expect("open disposable preview vault"));
    ui.sidebar_view = SidebarView::Research;
    let mut panel = ConversationPanel::new(CodexConfig {
        executable: "codex".into(),
        cwd: vault,
        mcp_bridge: "unused".into(),
        socket: "unused".into(),
        resume_thread: None,
        project: None,
    });
    panel.thread = Some("local-research-preview".into());
    panel.messages.push(ConversationMessage {
        role: "You",
        key: None,
        text: "How should we compare the collected material before writing a report?".into(),
        command: None,
        activity: None,
    });
    panel.messages.push(ConversationMessage {
        role: "Codex", key: None,
        text: "## Start with the question\n\nCompare what each source contributes to the investigation, then separate documented observations from proposed ideas.\n\n- Identify the claims supported by the collected material.\n- Keep conflicting evidence and limitations visible.\n- Note the questions we still need to investigate.\n\nWhen you are ready, we can turn the discussion and selected sources into a report.\n\n*Local layout fixture; no research or agent call was performed.*".into(),
        command: None, activity: None,
    });
    if let Some(workspace) = ui.app.research_workspaces().first() {
        if let Some(source) = workspace.sources.first() {
            panel.messages.push(ConversationMessage {role:"Codex",key:None,text:format!("### Retained evidence\n\nOpen the retained source [source:{} page:1] to inspect the original page.\n\nReference collection fixture: [First reference](https://example.org/research/one), [Second reference](https://example.org/research/two). These are interface test references, not research findings.",source.id),command:None,activity:None});
        }
    }
    ui.conversation = Some(panel);
    eframe::run_native(
        "Tanglewood · research preview",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 920.0]),
            ..Default::default()
        },
        Box::new(move |_| Ok(Box::new(ui))),
    )
}
