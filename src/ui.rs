//! Direct-style editor. Drafts and widget undo identity stay outside the ECS.
use std::collections::HashMap;

use eframe::egui;

mod code;
#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
mod codex_context;
#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
mod codex_markdown;
#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
mod conversation_view;
#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
mod model_picker;
mod document;
mod editor;
#[cfg(all(target_os = "linux", feature = "kimi"))]
pub(crate) mod kimi_panel;
mod layout;
mod navigation;
mod processing;
mod research;
mod theme;
use editor::Draft;
#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
use crate::conversation_runtime::ConversationMessage;
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
    conversation: Option<conversation_view::ConversationView>,
    #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
    conversation_waker_installed: bool,
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
            #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
            conversation_waker_installed: false,
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
        self.app.register_conversation_provider(crate::model_catalog::ProviderConfig::Codex(config.clone()));
        if self.conversation.is_none() {
            let record=crate::conversation_runtime::ConversationRecord::new_codex(config);
            match self.app.insert_conversation(record) {
                Ok(id)=>self.conversation=Some(conversation_view::ConversationView::new(id)),
                Err(error)=>self.error=Some(error),
            }
        }
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
                        panel.sidebar(ui, &self.app);
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
                        SidebarView::Codex => "Conversations",
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
                    panel.set_context_open(true);
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
                        context_closed = !panel.context_panel(ui, ctx, &self.app);
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
    #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
    fn apply_conversation_actions(&mut self) {
        use conversation_view::ViewAction;
        use model_picker::ConnectionAction;
        let actions=self.conversation.as_mut().map(|view|view.drain_actions()).unwrap_or_default();
        for action in actions {
            let result:Result<(),String>=match action {
                ViewAction::Record(action)=>self.app.execute_conversation(action),
                ViewAction::SelectModel(source,key)=>self.app.choose_conversation_model(source,key).map(|id| {if let Some(view)=&mut self.conversation {view.selected=id;}}),
                ViewAction::NewChat(source)=>self.app.conversation(source).map(|record|record.provider.clone()).ok_or_else(||"Conversation no longer exists.".into()).and_then(|provider|self.app.create_conversation(&provider)).map(|id|{if let Some(view)=&mut self.conversation {view.selected=id;}}),
                ViewAction::OpenRemote(source,remote)=>self.app.open_remote_conversation(source,&remote).map(|id|{if let Some(view)=&mut self.conversation {view.selected=id;}}),
                ViewAction::Connect(id)=>self.app.connect_conversation(id),
                ViewAction::Retry(id)=>self.app.retry_conversation_model(id),
                ViewAction::StartResearch=>{self.research.start_from_conversation();self.sidebar_view=SidebarView::Research;self.show_notes=true;Ok(())},
                ViewAction::References(text,turn)=>{let origin=self.conversation.as_ref().map(|view|view.research_capture(&self.app).current).flatten();self.research.open_references_with_origin(&text,origin,turn);self.sidebar_view=SidebarView::Research;Ok(())},
                ViewAction::VaultPermission(enabled)=>self.app.execute(Command::SetAutomationWriteEnabled {enabled},Caller::Local).map(|_|()).map_err(|error|error.to_string()),
                ViewAction::Connections(ConnectionAction::RefreshAll)=>{self.app.catalog_mut().refresh_all();Ok(())},
                ViewAction::Connections(ConnectionAction::RetryUnavailable)=>{self.app.catalog_mut().retry_unavailable();Ok(())},
                ViewAction::Connections(ConnectionAction::Stop)=>{self.app.catalog_mut().stop();Ok(())},
                ViewAction::Connections(ConnectionAction::SetAutomatic(enabled))=>{let catalog=self.app.catalog_mut();catalog.automatic=enabled;if enabled {catalog.refresh_all();}else{catalog.stop();}Ok(())},
                ViewAction::Connections(ConnectionAction::DisconnectAll)=>{if let Some(view)=&mut self.conversation {view.confirm_disconnect_all=true;}Ok(())},
                ViewAction::ConfirmDisconnectAll=>{self.app.disconnect_all_conversations();Ok(())},
            };
            if let Err(error)=result {self.error=Some(error);}
        }
    }
    fn show_frame(&mut self, ctx: &egui::Context) {
        if !self.theme_installed {
            theme::install(ctx);
            self.theme_installed = true;
        }
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        if !self.conversation_waker_installed {
            let context=ctx.clone(); self.app.set_conversation_waker(move || context.request_repaint());
            self.conversation_waker_installed=true;
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
        if let Some(view) = &mut self.conversation {
            view.poll(ctx,&self.app);
            let allowed=self.mcp.as_ref().map(|socket|socket.policy(&self.app)==crate::mcp::AccessPolicy::ReadWrite);
            self.app.set_conversation_vault_permission(view.selected,allowed);
            let workspace=if self.sidebar_view==SidebarView::Research {self.research.selected_workspace_id()} else {None};
            self.app.set_conversation_research_workspace(view.selected,workspace);
            self.research.set_conversation(view.research_capture(&self.app));
            if matches!(self.sidebar_view,SidebarView::Codex|SidebarView::Research) {self.app.catalog_mut().start_automatic();}
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
        let context_before = self.conversation.as_ref().is_some_and(|p| p.context_open());
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
                        if theme::selected_button(ui, theme::Icon::Codex, "Conversations", true, codex_mode)
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
                                SidebarView::Codex => "Conversations",
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
                                        "Conversations",
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
                                if self.app.conversation(panel.selected).is_some_and(|record|record.thread.is_none() && !record.history_loading) {
                                    ui.add_space(24.0);
                                    ui.heading("What would you like to explore?");
                                    if ui.button("Choose a conversation…").clicked() {
                                        self.research_chats_open = true;
                                    }
                                    ui.add_space(16.0);
                                }
                                panel.show(ui, ctx, &self.app, None, true);
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
                        panel.show(ui, ctx, &self.app, self.selected, false);
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
        if self.conversation.as_ref().is_some_and(|p| p.context_open())
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
                                |ui| panel.sidebar(ui, &self.app),
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
                                panel.set_context_open(true);
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
                        panel.set_context_open(pane == layout::Pane::Context);
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
                            .research_capture(&self.app)
                            .threads
                            .iter()
                            .any(|candidate| candidate.same_identity(&thread))
                        {
                            match self.app.open_remote_conversation(panel.selected,&thread.thread_id) {
                                Ok(id)=>panel.selected=id,
                                Err(error)=>self.research.error=Some(error),
                            }
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
                        match self.app.prepare_conversation_research(panel.selected,prompt,evidence,external) {
                            Ok(()) => {
                                self.research.conversation_prepared();
                                if self.sidebar_view != SidebarView::Research {
                                    self.sidebar_view = SidebarView::Codex;
                                    self.show_notes = true;
                                }
                            }
                            Err(error) => self.research.error = Some(error),
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
        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
        {
            if let Some(view)=&mut self.conversation {view.overlays(ctx,&self.app);}
            self.apply_conversation_actions();
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
                        #[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
                        if let Some(view)=&mut self.conversation {
                            if ui.button("Manage agent connections…").clicked() {view.open_connections();}
                            ui.add_space(10.0);
                        }
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
            conversation_pending = !self.app.begin_conversation_shutdown();
            if conversation_pending {ctx.request_repaint_after(std::time::Duration::from_millis(20));}
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
                    status.push_str(" Waiting for agent connections and model discovery to close.");
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
    fn test_codex_record() -> crate::conversation_runtime::ConversationRecord {
        crate::conversation_runtime::ConversationRecord::new_codex(crate::codex::CodexConfig {
            executable:"codex".into(),cwd:"/tmp".into(),mcp_bridge:"/tmp/bridge".into(),socket:"/tmp/socket".into(),resume_thread:None,
            project:Some(crate::notes::CodexProjectBinding {project_id:Some("project".into()),name:"Project".into(),creation_key:"key".into()}),
        })
    }
    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn research_capture_requires_listed_project_and_matching_thread_origin() {
        let mut record=test_codex_record();record.project_ready=true;record.binding_durable=true;record.ready=true;record.thread=Some("current".into());
        record.threads.push(crate::conversation::ThreadSummary {id:"current".into(),title:"Investigation".into(),updated_at:0,state:crate::conversation::ThreadWorkState::Idle});
        for (thread,text) in [("current","current finding"),("old","stale finding")] {record.messages.push(crate::conversation_runtime::ConversationMessage {role:"Codex",key:Some((thread.into(),"turn".into(),"item".into())),text:text.into(),command:None,activity:None,uncertain:false});}
        record.messages.push(crate::conversation_runtime::ConversationMessage {role:"Codex",key:None,text:"unconfirmed cached observation".into(),command:None,activity:None,uncertain:true});
        let mut app=App::new();let id=app.insert_conversation(record).unwrap();let view=super::conversation_view::ConversationView::new(id);
        let capture=view.research_capture(&app);
        assert_eq!(capture.answer,Some(("current finding".into(),Some("turn".into()))));
        assert!(!capture.discussion.contains("stale finding"));
        assert!(!capture.discussion.contains("unconfirmed cached observation"));
        assert_eq!(capture.current.unwrap().project_id.as_deref(),Some("project"));
        app.prepare_conversation_research(id,"gather".into(),"exact evidence".into(),true).unwrap();
        assert!(app.conversation(id).unwrap().controls.network_access);
        assert_eq!(app.conversation(id).unwrap().context_data.draft.attachments[0].text,"exact evidence");
        assert!(app.prepare_conversation_research(id,"replace".into(),"other".into(),false).is_err());
        assert_eq!(app.conversation(id).unwrap().prompt,"gather");
    }
    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn research_preparation_preserves_existing_drafts_and_access() {
        let mut occupied=test_codex_record();occupied.ready=true;occupied.thread=Some("draft".into());occupied.prompt="keep draft".into();
        let mut unrestricted=test_codex_record();unrestricted.ready=true;unrestricted.thread=Some("access".into());unrestricted.controls.approval=crate::codex::ApprovalMode::FullAccess;
        let mut app=App::new();let occupied=app.insert_conversation(occupied).unwrap();let unrestricted=app.insert_conversation(unrestricted).unwrap();
        assert!(app.prepare_conversation_research(occupied,"new".into(),"evidence".into(),false).is_err());
        assert_eq!(app.conversation(occupied).unwrap().prompt,"keep draft");
        assert!(app.prepare_conversation_research(unrestricted,"new".into(),"evidence".into(),false).is_err());
        assert_eq!(app.conversation(unrestricted).unwrap().controls.approval,crate::codex::ApprovalMode::FullAccess);
    }
    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn conversation_reducer_keeps_goal_and_command_output_scoped() {
        let mut record=test_codex_record();record.thread=Some("A".into());record.ready=true;
        assert!(!record.mode_consistent());record.accept_goal(None);assert!(record.mode_consistent());
        record.accept_goal(Some(crate::codex::GoalInfo {objective:"Research".into(),status:"active".into(),tokens_used:0,time_used_seconds:42,token_budget:None}));
        assert_eq!(record.controls.mode,crate::codex::WorkMode::Goal);
        let command=crate::codex::CommandExecution {item:"item".into(),command:"echo test".into(),cwd:"/tmp".into(),status:"inProgress".into(),exit_code:None,output:"test".into(),truncated:false,output_complete:true};
        record.command_card("B".into(),"turn".into(),command.clone());assert!(record.messages.is_empty());
        record.command_card("A".into(),"turn".into(),command.clone());assert_eq!(record.messages[0].text,"test");
        let mut completed=command;completed.status="completed".into();completed.exit_code=Some(0);completed.output="test done".into();
        record.command_card("A".into(),"turn".into(),completed);assert_eq!(record.messages[0].text,"test done");
        assert_eq!(super::chat_status("completed",Some(101)),("Failed",true));
    }
    #[cfg(all(target_os = "linux", feature = "codex"))]
    #[test]
    fn destructive_thread_action_preserves_other_chat_drafts() {
        use crate::conversation_runtime::{ConversationAction,ConversationActionKind};
        let mut source=test_codex_record();source.project_ready=true;source.binding_durable=true;source.client_attached=true;source.threads.push(crate::conversation::ThreadSummary {id:"remote".into(),title:"Remote".into(),updated_at:0,state:crate::conversation::ThreadWorkState::Idle});
        let mut other=test_codex_record();other.prompt="retained other draft".into();
        let mut app=App::new();let source=app.insert_conversation(source).unwrap();let other=app.insert_conversation(other).unwrap();
        let action=ConversationAction {chat_id:source,expected_generation:Some(0),kind:ConversationActionKind::ThreadAction {thread:"remote".into(),action:crate::codex::ThreadAction::Remove}};
        assert!(app.execute_conversation(action).is_err());assert_eq!(app.conversation(other).unwrap().prompt,"retained other draft");
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
        assert!(!ui.app.conversation(ui.conversation.as_ref().unwrap().selected).unwrap().has_adapter());
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
use crate::codex::CodexConfig;

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

/// Developer-only native preview with deterministic content and no agent runtime.

#[cfg(all(target_os = "linux", feature = "codex", debug_assertions))]
pub fn run_transcript_preview() -> eframe::Result {
    struct Preview { app: App, view: conversation_view::ConversationView }
    impl eframe::App for Preview {
        fn update(&mut self,ctx:&egui::Context,_frame:&mut eframe::Frame) {
            self.app.tick(); self.view.poll(ctx,&self.app);
            if self.view.context_open() {egui::SidePanel::right("preview-context").default_width(300.0).show(ctx,|ui|{self.view.context_panel(ui,ctx,&self.app);});}
            egui::CentralPanel::default().frame(egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(32,20))).show(ctx,|ui|self.view.show(ui,ctx,&self.app,None,false));
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
            let mut record = crate::conversation_runtime::ConversationRecord::new_codex(CodexConfig {
                executable: "codex".into(),
                cwd: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                mcp_bridge: "unused".into(),
                socket: "unused".into(),
                resume_thread: None,
                project: None,
            });
            record.thread = Some("local-preview".into());
            record.status = "Local presentation fixture · no agent connected".into();
            record.messages.push(ConversationMessage {
                role: "You",
                key: None,
                text: "Show a readable research report with code, sources and a checklist.".into(),
                command: None,
                activity: None, uncertain: false,
            });
            record.activity_card(
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
            record.command_card(
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
            record.activity_card(
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
            record.activity_card("local-preview".into(), "fixture".into(), crate::conversation::Activity {
                item: "tool-unknown-outcome".into(), kind: "mcpToolCall".into(),
                title: "accepted but storage failed: downloaded content is not a PDF; inspect state before retry".into(),
                status: "unknownOutcome".into(),
                details: "accepted but storage failed: downloaded content is not a PDF; inspect state before retry".into(),
                truncated: false,
            });
            record.context_data.files = vec![
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
            record.messages.push(ConversationMessage {
                role: "Codex",
                key: Some(("local-preview".into(), "fixture".into(), "reply".into())),
                text: include_str!("../docs/fixtures/mvp-2/codex-response.md").into(),
                command: None,
                activity: None, uncertain: false,
            });
            record.context_data.plan = vec![
                crate::codex::PlanStep {
                    text: "Compare sources".into(),
                    status: "completed".into(),
                },
                crate::codex::PlanStep {
                    text: "Review the report".into(),
                    status: "pending".into(),
                },
            ];
            let mut app=App::new();
            let id=app.insert_conversation(record).expect("insert preview record");
            Ok(Box::new(Preview {app,view:conversation_view::ConversationView::new(id)}))
        }),
    )

}

/// Developer-only research layout fixture using a disposable vault and no agent runtime.
#[cfg(all(target_os = "linux", feature = "codex", debug_assertions))]
pub fn run_research_preview(vault: std::path::PathBuf) -> eframe::Result {
    let mut ui = TanglewoodUi::new(App::open(&vault).expect("open disposable preview vault"));
    ui.sidebar_view = SidebarView::Research;
    let mut record = crate::conversation_runtime::ConversationRecord::new_codex(CodexConfig {
        executable: "codex".into(),
        cwd: vault,
        mcp_bridge: "unused".into(),
        socket: "unused".into(),
        resume_thread: None,
        project: None,
    });
    record.thread = Some("local-research-preview".into());
    record.messages.push(ConversationMessage {
        role: "You",
        key: None,
        text: "How should we compare the collected material before writing a report?".into(),
        command: None,
        activity: None, uncertain: false,
    });
    record.messages.push(ConversationMessage {
        role: "Codex", key: None,
        text: "## Start with the question\n\nCompare what each source contributes to the investigation, then separate documented observations from proposed ideas.\n\n- Identify the claims supported by the collected material.\n- Keep conflicting evidence and limitations visible.\n- Note the questions we still need to investigate.\n\nWhen you are ready, we can turn the discussion and selected sources into a report.\n\n*Local layout fixture; no research or agent call was performed.*".into(),
        command: None, activity: None, uncertain: false,
    });
    if let Some(workspace) = ui.app.research_workspaces().first() {
        if let Some(source) = workspace.sources.first() {
            record.messages.push(ConversationMessage {role:"Codex",key:None,text:format!("### Retained evidence\n\nOpen the retained source [source:{} page:1] to inspect the original page.\n\nReference collection fixture: [First reference](https://example.org/research/one), [Second reference](https://example.org/research/two). These are interface test references, not research findings.",source.id),command:None,activity:None,uncertain:false});
        }
    }
    let id=ui.app.insert_conversation(record).expect("insert preview conversation");
    ui.conversation = Some(conversation_view::ConversationView::new(id));
    eframe::run_native(
        "Tanglewood · research preview",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 920.0]),
            ..Default::default()
        },
        Box::new(move |_| Ok(Box::new(ui))),
    )
}
