//! Direct-style editor. Drafts and widget undo identity stay outside the ECS.
use std::collections::HashMap;

use eframe::egui;

mod document;
mod editor;
mod navigation;
mod processing;
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

fn field_single(ui: &mut egui::Ui, text: &mut String, name: &str) -> egui::Response {
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
        response
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = name;
        ui.text_edit_singleline(text)
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarView {
    Notes,
    Search,
    Backlinks,
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
    #[cfg(feature = "lan")]
    lan_panel: LanPanel,
    #[cfg(target_os = "android")]
    picker: Option<crate::android_picker::AndroidPicker>,
    #[cfg(target_os = "android")]
    picker_status: String,
    #[cfg(all(target_os = "linux", feature = "mcp"))]
    mcp: Option<crate::mcp::OwnerSocket>,
    #[cfg(all(target_os = "linux", feature = "codex"))]
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
            #[cfg(feature = "lan")]
            lan_panel: LanPanel::default(),
            #[cfg(target_os = "android")]
            picker: None,
            #[cfg(target_os = "android")]
            picker_status: String::new(),
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

    fn sidebar(&mut self, view: SidebarView, toggle: bool) {
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
            || conversation_pending
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
            Ok(CommandResult::Notes(_)) => {}
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
        if !self.theme_installed {
            theme::install(ctx);
            self.theme_installed = true;
        }
        self.app.tick();
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
        #[cfg(all(target_os = "linux", feature = "codex"))]
        if let Some(panel) = &mut self.conversation {
            panel.poll(ctx);
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
        let mobile = cfg!(target_os = "android") || ctx.content_rect().width() < 600.0;
        let dark = ctx.style().visuals.dark_mode;
        let main_fill = egui::Color32::from_gray(if dark { 25 } else { 250 });
        let side_fill = egui::Color32::from_gray(if dark { 22 } else { 242 });
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
        if navigation::enter_reader(ctx, self.show_preview && !self.show_settings
            && !self.show_status && self.note_dialog.is_none()) {
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
        egui::SidePanel::left("activity-rail")
            .resizable(false)
            .exact_width(48.0)
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
                #[cfg(all(target_os = "linux", feature = "codex"))]
                if let Some(panel) = &mut self.conversation {
                    if theme::selected_button(
                        ui,
                        theme::Icon::Codex,
                        "Codex conversation",
                        true,
                        panel.visible,
                    )
                    .clicked()
                    {
                        panel.visible = !panel.visible;
                    }
                }
                let bottom_controls_height = 2.0 * if mobile { 44.0 } else { 34.0 }
                    + ui.spacing().item_spacing.y;
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
        if self.show_notes {
            egui::SidePanel::left("notes")
                .resizable(!mobile)
                .default_width(if mobile {
                    ctx.available_rect().width()
                } else {
                    228.0
                })
                .width_range(if mobile {
                    180.0..=ctx.available_rect().width().max(180.0)
                } else {
                    180.0..=360.0
                })
                .frame(
                    egui::Frame::new()
                        .fill(side_fill)
                        .inner_margin(egui::Margin::symmetric(12, 17)),
                )
                .show(ctx, |ui| {
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
                        ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .id(egui::Id::new("quick-switch"))
                                .hint_text("Search notes…")
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(8.0);
                    }
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(match self.sidebar_view {
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
                            for other in &notes {
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
                                        [ui.available_width(), if mobile { 44.0 } else { 34.0 }],
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
                                    select = Some(other.note.id);
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
                });
            if !mobile { navigation::panel_focus(ctx, egui::Id::new("notes"), false, "Resize notes pane"); }
        }
        #[cfg(all(target_os = "linux", feature = "codex"))]
        if let Some(panel) = &mut self.conversation {
            if panel.visible {
                egui::SidePanel::right("codex-conversation")
                    .resizable(true)
                    .default_width(300.0)
                    .frame(egui::Frame::new().fill(main_fill).inner_margin(18))
                    .show(ctx, |ui| panel.show(ui, ctx));
            }
        }
        if !mobile || !self.show_notes {
            egui::TopBottomPanel::top("document-header")
                .exact_height(66.0)
                .frame(
                    egui::Frame::new()
                        .fill(main_fill)
                        .inner_margin(egui::Margin::symmetric(
                            if mobile { 12 } else { 24 },
                            if mobile { 11 } else { 16 },
                        )),
                )
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        if theme::button(ui, theme::Icon::Sidebar, "Toggle sidebar", true).clicked()
                        {
                            self.sidebar(SidebarView::Notes, true);
                        }
                        let actions_width = if mobile { 96.0 } else { 76.0 };
                        let title_width = (ui.available_width() - actions_width - 8.0).max(0.0);
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
                    let width=ui.available_width().min(730.0);
                    let margin=((ui.available_width()-width)*0.5).max(0.0);
                    ui.horizontal_top(|ui| {
                        ui.add_space(margin);
                        ui.allocate_ui_with_layout(egui::vec2(width,0.0),egui::Layout::top_down(egui::Align::Min),|ui| {
                            egui::Frame::new().inner_margin(egui::Margin{left:if mobile {30} else {52},right:if mobile {30} else {52},top:if mobile {22} else {32},bottom:46}).show(ui,|ui| {
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
        if !self.show_preview {
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
        if let Some(action) = editor_action {
            if let Some(view) = view {
                let draft = self.drafts.get(&view.note.id).expect("selected note draft");
                return_to_block = draft.range.as_ref().map(|range| (view.note.id, range.start));
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
                    }
                    SettingsPage::Connections => {
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
                                            egui::Button::new("Export accepted source"),
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
                                    "Exports accepted source only; drafts are excluded.",
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
            if mobile && self.selected == Some(id) {
                self.show_notes = false;
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
            visible: false,
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
