//! View-local projection of application-owned conversations. No adapter or draft lives here.
use super::{
    chat_event_icon, chat_status, codex_context, codex_markdown, model_picker, name_field,
    research, theme,
};
use crate::{
    App,
    conversation::{ConversationKey, ThreadWorkState},
    conversation_runtime::{
        ContextData, ConversationAction, ConversationActionKind, ConversationRecord, ModelKey,
    },
    notes::NoteId,
};
use eframe::egui;
use std::collections::HashMap;
use uuid::Uuid;

pub(super) enum ViewAction {
    Record(ConversationAction),
    SelectModel(Uuid, ModelKey),
    NewChat(Uuid),
    OpenRemote(Uuid, String),
    Connect(Uuid),
    Retry(Uuid),
    StartResearch,
    References(String, Option<String>),
    VaultPermission(bool),
    Connections(model_picker::ConnectionAction),
    ConfirmDisconnectAll,
}
#[derive(Default)]
struct ChatView {
    context: codex_context::ContextUi,
    markdown: codex_markdown::Cache,
    composer_height: f32,
    relevant_cache: Option<(
        Uuid,
        crate::notes::Revision,
        String,
        Vec<crate::research_desktop::Passage>,
    )>,
}
#[cfg(feature = "codex")]
struct ThreadDialog {
    source: Uuid,
    thread: String,
    title: String,
    action: crate::codex::ThreadAction,
}

#[derive(Default)]
pub(super) struct ConversationView {
    pub selected: Uuid,
    views: HashMap<Uuid, ChatView>,
    picker: model_picker::ModelPickerUi,
    actions: Vec<ViewAction>,
    pub confirm_disconnect_all: bool,
    pending_cross_model: Option<(Uuid, ModelKey)>,
    pending_disconnect: Option<(Uuid, u64)>,
    review_permission: Option<Uuid>,
    #[cfg(feature = "codex")]
    thread_dialog: Option<ThreadDialog>,
    #[cfg(feature = "codex")]
    project_dialog: Option<(Uuid, String)>,
}
impl ConversationView {
    pub fn new(selected: Uuid) -> Self {
        Self {
            selected,
            ..Default::default()
        }
    }
    pub fn context_open(&self) -> bool {
        self.views
            .get(&self.selected)
            .is_some_and(|v| v.context.open)
    }
    pub fn set_context_open(&mut self, open: bool) {
        self.views.entry(self.selected).or_default().context.open = open;
    }
    pub fn open_connections(&mut self) {
        self.picker.open_connections();
    }
    pub fn drain_actions(&mut self) -> Vec<ViewAction> {
        std::mem::take(&mut self.actions)
    }
    fn action(id: Uuid, generation: u64, kind: ConversationActionKind) -> ViewAction {
        ViewAction::Record(ConversationAction {
            chat_id: id,
            expected_generation: Some(generation),
            kind,
        })
    }
    pub fn poll(&mut self, ctx: &egui::Context, app: &App) {
        for (id, view) in &mut self.views {
            if let Some(record) = app.conversation(*id) {
                let mut data = record.context_data.clone();
                view.context.poll(&mut data, ctx);
            }
        }
    }
    pub fn sidebar(&mut self, ui: &mut egui::Ui, app: &App) {
        let selected = self.selected;
        let Some(current) = app.conversation(selected) else {
            return;
        };
        ui.horizontal(|ui| {
            let controls_width = 2.0 * theme::control_size(ui) + 2.0 * ui.spacing().item_spacing.x;
            let title_width = (ui.available_width() - controls_width).max(0.0);
            ui.add_sized(
                [title_width, theme::control_size(ui)],
                egui::Label::new(egui::RichText::new("Conversations").strong()).truncate(),
            );
            if theme::button(ui, theme::Icon::New, "New conversation", true).clicked() {
                self.actions.push(ViewAction::NewChat(selected));
            }
            let more = theme::button(ui, theme::Icon::More, "Conversation list actions", true);
            egui::Popup::menu(&more).show(|ui| {
                if ui
                    .add_enabled(
                        current.project_ready && !current.threads_loading,
                        egui::Button::new("Refresh conversations"),
                    )
                    .clicked()
                {
                    self.actions.push(Self::action(
                        selected,
                        current.client_generation,
                        ConversationActionKind::RefreshThreads { cursor: None },
                    ));
                    ui.close();
                }
                if ui.button("Manage connections…").clicked() {
                    self.picker.open_connections();
                    ui.close();
                }
            });
        });
        ui.add_space(10.0);
        let mut ids: Vec<_> = app.conversation_ids().collect();
        ids.sort();
        egui::ScrollArea::vertical()
            .id_salt("conversation-list")
            .show(ui, |ui| {
                for id in ids {
                    let Some(record) = app.conversation(id) else {
                        continue;
                    };
                    let title = record
                        .thread
                        .as_ref()
                        .and_then(|id| record.threads.iter().find(|t| &t.id == id))
                        .map(|t| t.title.as_str())
                        .unwrap_or("New conversation");
                    let status = match record.observation {
                        crate::conversation_runtime::Freshness::Disconnected => {
                            "Disconnected · activity unknown".to_owned()
                        }
                        crate::conversation_runtime::Freshness::NeverObserved
                        | crate::conversation_runtime::Freshness::Unconfirmed => {
                            format!(
                                "Activity unknown · last observed: {}",
                                state_label(record.observed_activity)
                            )
                        }
                        crate::conversation_runtime::Freshness::Attached => {
                            state_label(record.observed_activity).to_owned()
                        }
                    };
                    let attention = record.error.is_some()
                        || record.observed_activity == ThreadWorkState::Error;
                    let label = format!(
                        "{}{} · {}",
                        if attention { "⚠ " } else { "" },
                        record.provider_name(),
                        title
                    );
                    let tooltip = record.error.as_ref().map_or_else(
                        || status.clone(),
                        |error| format!("{error}\nOpen this conversation to review."),
                    );
                    let response = if record.client_attached {
                        ui.horizontal(|ui| {
                            let width = (ui.available_width()
                                - theme::control_size(ui)
                                - ui.spacing().item_spacing.x)
                                .max(0.0);
                            let response = ui
                                .add_sized(
                                    [width, theme::control_size(ui)],
                                    egui::Button::selectable(id == selected, &label).truncate(),
                                )
                                .on_hover_text(&tooltip);
                            let menu =
                                theme::button(ui, theme::Icon::More, "Conversation actions", true);
                            egui::Popup::menu(&menu)
                                .show(|ui| self.local_conversation_menu(ui, id, record, title));
                            response
                        })
                        .inner
                    } else {
                        ui.selectable_label(id == selected, label)
                            .on_hover_text(&tooltip)
                    };
                    if response.clicked() {
                        self.selected = id;
                    }
                    if record.client_attached {
                        response
                            .context_menu(|ui| self.local_conversation_menu(ui, id, record, title));
                    }
                    if record.approval_count() > 0 {
                        ui.weak("Approval needed");
                    } else if record.client_attached
                        && (record.observation
                            == crate::conversation_runtime::Freshness::Unconfirmed
                            || matches!(
                                record.observed_activity,
                                ThreadWorkState::Working
                                    | ThreadWorkState::WaitingOnApproval
                                    | ThreadWorkState::WaitingOnInput
                            ))
                    {
                        ui.weak(&status);
                    }
                }
                if current.project_ready && !current.threads.is_empty() {
                    ui.separator();
                    ui.weak(format!("{} conversations", current.provider_name()));
                    for remote in &current.threads {
                        let key = ConversationKey {
                            backend: current.provider.backend,
                            workspace: current.provider.workspace.clone(),
                            session: remote.id.clone(),
                        };
                        if app.conversation_for_remote(&key).is_some() {
                            continue;
                        }
                        #[cfg(feature = "codex")]
                        let has_menu = !current.is_kimi() && current.client_attached;
                        #[cfg(not(feature = "codex"))]
                        let has_menu = false;
                        let response = if has_menu {
                            ui.horizontal(|ui| {
                                let width = (ui.available_width()
                                    - theme::control_size(ui)
                                    - ui.spacing().item_spacing.x)
                                    .max(0.0);
                                let response = ui
                                    .add_sized(
                                        [width, theme::control_size(ui)],
                                        egui::Button::new(&remote.title).truncate(),
                                    )
                                    .on_hover_text(state_label(remote.state));
                                let menu = theme::button(
                                    ui,
                                    theme::Icon::More,
                                    "Conversation actions",
                                    true,
                                );
                                #[cfg(feature = "codex")]
                                egui::Popup::menu(&menu).show(|ui| {
                                    self.thread_actions_menu(
                                        ui,
                                        selected,
                                        &remote.id,
                                        &remote.title,
                                    )
                                });
                                #[cfg(not(feature = "codex"))]
                                let _ = menu;
                                response
                            })
                            .inner
                        } else {
                            ui.button(&remote.title)
                                .on_hover_text(state_label(remote.state))
                        };
                        if response.clicked() {
                            self.actions
                                .push(ViewAction::OpenRemote(selected, remote.id.clone()));
                        }
                        #[cfg(feature = "codex")]
                        if has_menu {
                            response.context_menu(|ui| {
                                self.thread_actions_menu(ui, selected, &remote.id, &remote.title)
                            });
                        }
                    }
                }
                if let Some(cursor) = current.threads_cursor.clone() {
                    if ui
                        .add_enabled(
                            !current.threads_loading,
                            egui::Button::new("More conversations…"),
                        )
                        .clicked()
                    {
                        self.actions.push(Self::action(
                            selected,
                            current.client_generation,
                            ConversationActionKind::RefreshThreads {
                                cursor: Some(cursor),
                            },
                        ));
                    }
                }
                #[cfg(feature = "codex")]
                if !current.is_kimi()
                    && current.project_ready
                    && ui.button("Rename project…").clicked()
                {
                    self.project_dialog = Some((selected, current.project_name.clone()));
                }
            });
    }
    fn local_conversation_menu(
        &mut self,
        ui: &mut egui::Ui,
        id: Uuid,
        record: &ConversationRecord,
        title: &str,
    ) {
        #[cfg(feature = "codex")]
        if !record.is_kimi() {
            if let Some(thread) = record
                .thread
                .as_ref()
                .filter(|thread| record.threads.iter().any(|t| &t.id == *thread))
            {
                self.thread_actions_menu(ui, id, thread, title);
            }
        }
        #[cfg(not(feature = "codex"))]
        let _ = title;
        if record.client_attached && ui.button("Disconnect this conversation").clicked() {
            self.pending_disconnect = Some((id, record.client_generation));
            ui.close();
        }
    }
    #[cfg(feature = "codex")]
    fn thread_actions_menu(&mut self, ui: &mut egui::Ui, source: Uuid, thread: &str, title: &str) {
        for (label, action) in [
            ("Rename…", crate::codex::ThreadAction::Rename(title.into())),
            ("Archive…", crate::codex::ThreadAction::Archive),
            ("Remove…", crate::codex::ThreadAction::Remove),
        ] {
            if ui.button(label).clicked() {
                self.thread_dialog = Some(ThreadDialog {
                    source,
                    thread: thread.into(),
                    title: title.into(),
                    action,
                });
                ui.close();
            }
        }
    }
    pub fn context_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, app: &App) -> bool {
        let id = self.selected;
        let Some(record) = app.conversation(id) else {
            return false;
        };
        let view = self.views.entry(id).or_default();
        view.context.open = true;
        let mut data = record.context_data.clone();
        let ready = record.ready
            && !record.busy()
            && !record.disconnecting
            && record.context_goal_pending.is_none();
        let action = view.context.panel(
            &mut data,
            ui,
            ctx,
            &record.provider_cwd(),
            ready,
            record.is_kimi(),
        );
        let open = view.context.open;
        if data.draft != record.context_data.draft {
            self.actions.push(Self::action(
                id,
                record.client_generation,
                ConversationActionKind::SetContextDraft {
                    expected_revision: record.draft_revision,
                    draft: data.draft,
                },
            ));
        }
        if let Some(path) = view.context.take_attachment_request() {
            self.actions.push(Self::action(
                id,
                record.client_generation,
                ConversationActionKind::AttachFile { path },
            ));
        }
        if let Some(action) = action {
            Self::context_action(&mut self.actions, record, action, None);
        }
        open
    }
    fn context_action(
        actions: &mut Vec<ViewAction>,
        record: &ConversationRecord,
        action: codex_context::Action,
        prompt: Option<&mut String>,
    ) {
        #[cfg(feature = "codex")]
        if !record.is_kimi() {
            use crate::codex::CodexRequest;
            let request = match action {
                codex_context::Action::Refresh => Some(CodexRequest::RefreshContext),
                codex_context::Action::SetGoal(objective) => {
                    Some(CodexRequest::SetGoal { objective })
                }
                codex_context::Action::PauseGoal => Some(CodexRequest::PauseGoal),
                codex_context::Action::ClearGoal => Some(CodexRequest::ClearGoal),
                codex_context::Action::DraftSketch => {
                    if let Some(prompt) = prompt {
                        prompt.insert_str(0,"Create a visual sketch/mockup of the idea below. Use available image/visualization capabilities, and include the result or its saved file path.\n\n");
                    }
                    None
                }
                codex_context::Action::DraftPlan => {
                    if let Some(prompt) = prompt {
                        if prompt.is_empty() {
                            prompt.push_str(
                                "Please propose a plan for this task before implementing it.\n\n",
                            );
                        } else {
                            prompt.push_str(
                                "\n\nPlease propose a plan before implementing this task.",
                            );
                        }
                    }
                    None
                }
            };
            if let Some(request) = request {
                actions.push(Self::action(
                    record.chat_id,
                    record.client_generation,
                    ConversationActionKind::Goal(request),
                ));
            }
            return;
        }
        let _ = (actions, record, action, prompt);
    }
    pub fn research_capture(&self, app: &App) -> research::ConversationCapture {
        let Some(record) = app.conversation(self.selected) else {
            return Default::default();
        };
        if !record.project_ready {
            return Default::default();
        }
        #[cfg(feature = "codex")]
        if !record.is_kimi()
            && (!record.binding_durable
                || record
                    .config
                    .project
                    .as_ref()
                    .and_then(|p| p.project_id.as_ref())
                    .is_none())
        {
            return Default::default();
        }
        #[cfg(feature = "codex")]
        let project_id = if !record.is_kimi() {
            record
                .config
                .project
                .as_ref()
                .and_then(|p| p.project_id.clone())
        } else {
            None
        };
        #[cfg(not(feature = "codex"))]
        let project_id = None;
        let provider = record.provider_name().to_lowercase();
        let threads: Vec<_> = record
            .threads
            .iter()
            .filter(|t| !record.unavailable_threads.contains(&t.id))
            .filter(|t| {
                #[cfg(feature = "kimi")]
                if record.is_kimi() {
                    return record
                        .kimi
                        .as_ref()
                        .is_some_and(|state| state.known.contains(&t.id));
                }
                true
            })
            .map(|t| crate::research::ThreadRef {
                provider: provider.clone(),
                project_id: project_id.clone(),
                thread_id: t.id.clone(),
                title: t.title.clone(),
            })
            .collect();
        let current = record
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
            || record.pending_thread.is_some()
            || record.history_loading
            || !record.ready
        {
            return capture;
        }
        let messages: Vec<_> = record
            .messages
            .iter()
            .filter(|m| {
                m.command.is_none()
                    && m.activity.is_none()
                    && !m.uncertain
                    && matches!(m.role, "You" | "Codex" | "Kimi")
                    && m.key
                        .as_ref()
                        .is_none_or(|(thread, _, _)| record.thread.as_ref() == Some(thread))
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
        if !record.busy() {
            capture.answer = messages
                .iter()
                .rev()
                .find(|m| m.role == record.provider_name() && !m.text.trim().is_empty())
                .filter(|m| m.text.len() <= 24_000)
                .map(|m| {
                    (
                        m.text.clone(),
                        m.key
                            .as_ref()
                            .and_then(|(_, turn, _)| (!turn.is_empty()).then(|| turn.clone())),
                    )
                });
        }
        capture
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &App,
        selected_note: Option<NoteId>,
        research_mode: bool,
    ) {
        let id = self.selected;
        let Some(record) = app.conversation(id) else {
            return;
        };
        let view = self.views.entry(id).or_default();
        let mut data = record.context_data.clone();
        let mut prompt = record.prompt.clone();
        let mut deferred = Vec::new();
        let title = record
            .thread
            .as_ref()
            .and_then(|id| record.threads.iter().find(|t| &t.id == id))
            .map(|t| t.title.as_str())
            .unwrap_or(record.provider_name());
        if !research_mode {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(title).size(18.0)).truncate());
                if ui
                    .button("Context")
                    .on_hover_text("Show goal, plan, files and capabilities")
                    .clicked()
                {
                    view.context.open = !view.context.open;
                }
            });
        }
        if !record.status.is_empty() && (!research_mode || record.busy()) {
            ui.weak(&record.status);
        }
        if record.history_incomplete {
            ui.weak("History may be incomplete. Earlier observations are marked when the replay could not confirm them.");
        }
        if let Some(error) = &record.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if record
            .model_intent
            .as_ref()
            .is_some_and(|i| i.phase == crate::conversation_runtime::ModelPhase::Failed)
            && ui.button("Retry preparation").clicked()
        {
            deferred.push(ViewAction::Retry(id));
        }
        if record.permission_review_required && ui.button("Review Kimi access mode…").clicked() {
            self.review_permission = Some(id);
        }
        let more = theme::button(ui, theme::Icon::More, "Conversation actions", true);
        egui::Popup::menu(&more).show(|ui| {
            if record.client_attached && ui.button("Disconnect this conversation").clicked() {
                self.pending_disconnect = Some((id, record.client_generation));
                ui.close();
            }
            if !record.client_attached && ui.button("Connect conversation").clicked() {
                deferred.push(ViewAction::Connect(id));
                ui.close();
            }
            if !research_mode
                && ui
                    .add_enabled(
                        !record.busy(),
                        egui::Button::new("Start research from discussion…"),
                    )
                    .clicked()
            {
                deferred.push(ViewAction::StartResearch);
                ui.close();
            }
            if ui
                .add_enabled(
                    !record.busy() && record.approval_count() == 0,
                    egui::Button::new("Clear displayed messages"),
                )
                .clicked()
            {
                deferred.push(Self::action(
                    id,
                    record.client_generation,
                    ConversationActionKind::ClearDisplay,
                ));
                view.markdown = Default::default();
                ui.close();
            }
            if let Some(allowed) = record.vault_write_enabled {
                ui.menu_button(
                    if allowed {
                        "Vault updates · Allowed"
                    } else {
                        "Vault updates · Read only"
                    },
                    |ui| {
                        if let Some(enabled) = super::agent_permission_controls(ui, app, allowed) {
                            deferred.push(ViewAction::VaultPermission(enabled));
                        }
                    },
                );
            }
        });
        if record.history_cursor.is_some()
            && ui
                .add_enabled(
                    !record.history_loading && !record.busy(),
                    egui::Button::new("Earlier messages"),
                )
                .clicked()
        {
            deferred.push(Self::action(
                id,
                record.client_generation,
                ConversationActionKind::LoadHistory,
            ));
        }
        if let Some(refs) = view.transcript(record, &mut data, ui, ctx) {
            deferred.push(ViewAction::References(refs.0, refs.1));
        }
        #[cfg(feature = "codex")]
        if !record.is_kimi() {
            for approval in &record.approvals {
                ui.group(|ui| {
                    ui.strong("Approval required");
                    ui.label(&approval.method);
                    ui.add(egui::Label::new(&approval.details).selectable(true).wrap());
                    ui.horizontal_wrapped(|ui| {
                        for decision in &approval.decisions {
                            let label = match decision {
                                crate::codex::ApprovalDecision::Accept => "Allow once",
                                crate::codex::ApprovalDecision::Decline => "Decline",
                                crate::codex::ApprovalDecision::Cancel => "Cancel",
                            };
                            if ui
                                .add_enabled(
                                    !approval.answering && !record.disconnecting,
                                    egui::Button::new(label),
                                )
                                .clicked()
                            {
                                deferred.push(Self::action(
                                    id,
                                    record.client_generation,
                                    ConversationActionKind::AnswerCodex {
                                        id: approval.id.clone(),
                                        decision: *decision,
                                    },
                                ));
                            }
                        }
                    });
                });
            }
        }
        #[cfg(feature = "kimi")]
        if record.is_kimi() {
            if let Some(state) = &record.kimi {
                for permission in &state.permissions {
                    ui.group(|ui| {
                        ui.strong("Permission required");
                        ui.add(
                            egui::Label::new(&permission.details)
                                .selectable(true)
                                .wrap(),
                        );
                        ui.horizontal_wrapped(|ui| {
                            for option in &permission.options {
                                if ui
                                    .add_enabled(
                                        !permission.answering && !record.disconnecting,
                                        egui::Button::new(&option.name),
                                    )
                                    .on_hover_text(&option.kind)
                                    .clicked()
                                {
                                    deferred.push(Self::action(
                                        id,
                                        record.client_generation,
                                        ConversationActionKind::AnswerKimi {
                                            id: permission.id.clone(),
                                            option: Some(option.id.clone()),
                                        },
                                    ));
                                }
                            }
                            if ui
                                .add_enabled(
                                    !permission.answering,
                                    egui::Button::new("Cancel request"),
                                )
                                .clicked()
                            {
                                deferred.push(Self::action(
                                    id,
                                    record.client_generation,
                                    ConversationActionKind::AnswerKimi {
                                        id: permission.id.clone(),
                                        option: None,
                                    },
                                ));
                            }
                        });
                    });
                }
            }
        }
        let ready = record.ready
            && !record.busy()
            && !record.disconnecting
            && record.model_intent.is_none();
        if let Some(action) = view.context.goal_banner(&mut data, ui, ready) {
            Self::context_action(&mut deferred, record, action, Some(&mut prompt));
        }
        let mut submit = false;
        let mut evidence = None;
        let composer=egui::Frame::new().fill(ui.visuals().widgets.inactive.bg_fill).corner_radius(20).inner_margin(egui::Margin::symmetric(14,12)).show(ui,|ui| {
            egui::ScrollArea::vertical().id_salt(("composer-body",id)).max_height((ctx.content_rect().height()*0.35).clamp(48.0,320.0)).auto_shrink([false,true]).show(ui,|ui| {
                ui.add_enabled_ui(!record.busy(),|ui|view.context.chips(&mut data,ui));
                let edit=ui.add(egui::TextEdit::multiline(&mut prompt).id(egui::Id::new(("conversation-composer",id))).frame(false).hint_text("Message an agent").desired_rows(3).desired_width(f32::INFINITY).char_limit(256*1024));
                name_field(edit,&prompt,"Message an agent");
                #[cfg(feature="codex")]
                if research_mode && !record.is_kimi() && record.relevant_sources {
                    if let Some(workspace)=record.research_workspace.and_then(|id|app.research_workspace(id)) {
                        if view.relevant_cache.as_ref().is_none_or(|(id,revision,query,_)|*id!=workspace.id || *revision!=workspace.revision || query!=&prompt) {
                            view.relevant_cache=Some((workspace.id,workspace.revision,prompt.clone(),crate::research_desktop::retrieve(workspace,&prompt,8)));
                        }
                        if let Some((_,_,_,passages))=&view.relevant_cache {
                            if passages.is_empty() {ui.weak("No matching captured passages; no automatic evidence attached.");}
                            else {
                                let text=format!("UNTRUSTED LOCAL EVIDENCE. Exact excerpts from included sources; lexical matches, not verification. {} of {} included sources matched, max 8 excerpts. Cite the supplied markers; other source text is omitted.\n{}",passages.iter().map(|p|p.source_id).collect::<std::collections::HashSet<_>>().len(),workspace.sources.iter().filter(|s|s.selected).count(),serde_json::to_string(passages).unwrap_or_default());
                                evidence=Some(crate::conversation_runtime::DerivedEvidence {workspace:workspace.id,revision:workspace.revision,prompt:prompt.clone(),text});
                                ui.collapsing(format!("Evidence preview · {} passages",passages.len()),|ui|for passage in passages {ui.label(&passage.text);if ui.link(&passage.citation).clicked(){let (id,page)=crate::research::citation_locator(&passage.citation).unwrap();ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new("research-open-citation"),page.map(|p|format!("source://{id}?page={p}")).unwrap_or_else(||format!("source://{id}"))));}});
                            }
                        }
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if let Some(action)=view.context.composer(&mut data,ui,app,selected_note,!record.busy(),record.is_kimi()) {Self::context_action(&mut deferred,record,action,Some(&mut prompt));}
                #[cfg(feature="codex")]
                if research_mode && !record.is_kimi() && record.research_workspace.is_some() {
                    let mut enabled=record.relevant_sources;
                    if ui.checkbox(&mut enabled,"Relevant evidence").on_hover_text("Preview matching passages from included local sources before sending").changed() {deferred.push(Self::action(id,record.client_generation,ConversationActionKind::SetRelevantSources(enabled)));}
                }
                let pending=record.model_intent.as_ref().filter(|intent|intent.phase!=crate::conversation_runtime::ModelPhase::Failed).map(|intent|&intent.key);
                if let Some(key)=self.picker.model(ui,&app.catalog().entries,record.confirmed_model.as_ref(),pending,!record.disconnecting && pending.is_none()) {
                    if key.provider!=record.provider && (record.thread.is_some() || !record.messages.is_empty()) {self.pending_cross_model=Some((id,key));}
                    else {deferred.push(ViewAction::SelectModel(id,key));}
                }
                let (current,efforts)=efforts(record);
                if let Some(value)=model_picker::effort(ui,current.as_deref(),&efforts,ready && !record.permission_review_required) {deferred.push(Self::action(id,record.client_generation,ConversationActionKind::SetEffort(value)));}
                native_controls(ui,record,view,&mut deferred);
                if record.starting || record.active_turn.is_some() {
                    let stop=ui.add_enabled(!record.disconnecting,egui::Button::new("■").corner_radius(16).min_size(egui::vec2(32.0,32.0))).on_hover_text("Stop response");
                    stop.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,stop.enabled(),"Stop response"));
                    if stop.clicked() {deferred.push(Self::action(id,record.client_generation,ConversationActionKind::Stop));}
                } else {
                    let mut can_send=ready && record.confirmed_model.is_some() && !record.permission_review_required && !prompt.trim().is_empty() && prompt.len()+data.total_input_bytes()<=256*1024;
                    #[cfg(feature="codex")]
                    if !record.is_kimi() {can_send &= record.binding_durable && record.mode_consistent() && data.skills_valid();}
                    let send=ui.add_enabled(can_send,egui::Button::new("↑").corner_radius(16).min_size(egui::vec2(32.0,32.0))).on_hover_text("Send message");
                    send.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,send.enabled(),"Send message"));if send.clicked(){submit=true;}
                }
            });
        });
        view.composer_height = composer.response.rect.height() + ui.spacing().item_spacing.y;
        if let Some(action) = view.context.dialogs(&mut data, ctx, ready) {
            Self::context_action(&mut deferred, record, action, Some(&mut prompt));
        }
        if prompt != record.prompt || data.draft != record.context_data.draft {
            self.actions.push(Self::action(
                id,
                record.client_generation,
                ConversationActionKind::EditDraft {
                    expected_revision: record.draft_revision,
                    text: prompt,
                    context: data.draft,
                },
            ));
        }
        if let Some(path) = view.context.take_attachment_request() {
            self.actions.push(Self::action(
                id,
                record.client_generation,
                ConversationActionKind::AttachFile { path },
            ));
        }
        self.actions.extend(deferred);
        if submit {
            self.actions.push(Self::action(
                id,
                record.client_generation,
                ConversationActionKind::Submit { evidence },
            ));
        }
        if self.picker.take_connections_requested() {
            self.picker.open_connections();
        }
    }
    pub fn overlays(&mut self, ctx: &egui::Context, app: &App) {
        if let Some(action) = self.picker.connections(
            ctx,
            &app.catalog().entries,
            &app.active_conversation_counts(),
            app.catalog().automatic,
        ) {
            self.actions.push(ViewAction::Connections(action));
        }
        #[cfg(feature = "codex")]
        if let Some(mut dialog) = self.thread_dialog.take() {
            let mut decision = None;
            let modal=egui::Modal::new(egui::Id::new("manage-codex-thread")).show(ctx,|ui| {
                let heading=match dialog.action {crate::codex::ThreadAction::Rename(_)=>"Rename conversation",crate::codex::ThreadAction::Archive=>"Archive conversation?",crate::codex::ThreadAction::Remove=>"Remove conversation permanently?"};
                ui.heading(heading);
                if let crate::codex::ThreadAction::Rename(name)=&mut dialog.action {ui.text_edit_singleline(name);}else {ui.label(&dialog.title);ui.label(if matches!(dialog.action,crate::codex::ThreadAction::Remove) {"This permanently removes this conversation and its spawned child conversations from Codex."}else{"This archives the conversation and attempts to archive its spawned children."});}
                ui.horizontal(|ui| {if ui.button("Cancel").clicked(){decision=Some(false);}if ui.button("Continue").clicked(){decision=Some(true);}});
            });
            if decision == Some(true) {
                if let Some(record) = app.conversation(dialog.source) {
                    self.actions.push(Self::action(
                        dialog.source,
                        record.client_generation,
                        ConversationActionKind::ThreadAction {
                            thread: dialog.thread,
                            action: dialog.action,
                        },
                    ));
                }
            } else if decision.is_none() && !modal.should_close() {
                self.thread_dialog = Some(dialog);
            }
        }
        #[cfg(feature = "codex")]
        if let Some((id, mut name)) = self.project_dialog.take() {
            let mut decision = None;
            let modal = egui::Modal::new(egui::Id::new("rename-codex-project")).show(ctx, |ui| {
                ui.heading("Rename project");
                ui.text_edit_singleline(&mut name);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                    if ui.button("Rename").clicked() {
                        decision = Some(true);
                    }
                });
            });
            if decision == Some(true) {
                if let Some(record) = app.conversation(id) {
                    self.actions.push(Self::action(
                        id,
                        record.client_generation,
                        ConversationActionKind::RenameProject(name),
                    ));
                }
            } else if decision.is_none() && !modal.should_close() {
                self.project_dialog = Some((id, name));
            }
        }
        if let Some(id) = self.review_permission {
            let mut close = false;
            let modal=egui::Modal::new(egui::Id::new("review-kimi-mode")).show(ctx,|ui| {
                ui.heading("Review Kimi access mode");
                if let Some(record)=app.conversation(id) {
                    #[cfg(feature="kimi")]
                    if let Some(mode)=record.kimi.as_ref().and_then(|state|state.options.iter().find(|o|o.category.as_deref()==Some("mode"))) {ui.label(format!("{}: {}",mode.name,mode.current));}
                    ui.label("Kimi uses ordinary process privileges. Review the current mode before sending.");
                    ui.horizontal(|ui| {if ui.button("Cancel").clicked(){close=true;} if ui.button("I reviewed this mode").clicked(){self.actions.push(Self::action(id,record.client_generation,ConversationActionKind::AcknowledgePermissionReview));close=true;}});
                } else {close=true;}
            });
            if close || modal.should_close() {
                self.review_permission = None;
            }
        }
        if let Some((source, key)) = self.pending_cross_model.clone() {
            let mut decision = None;
            let modal=egui::Modal::new(egui::Id::new("new-provider-conversation")).show(ctx,|ui| {
                ui.heading("Start a new conversation?");
                ui.label("Your current conversation and its work stay in place. The new conversation receives the current draft and explicit attachments.");
                ui.horizontal(|ui| {if ui.button("Cancel").clicked() {decision=Some(false);}if ui.button("Start new conversation").clicked() {decision=Some(true);}});
            });
            if decision == Some(true) {
                self.actions.push(ViewAction::SelectModel(source, key));
                self.pending_cross_model = None;
            } else if decision == Some(false) || modal.should_close() {
                self.pending_cross_model = None;
            }
        }
        if let Some((id, generation)) = self.pending_disconnect {
            let mut decision = None;
            let modal = egui::Modal::new(egui::Id::new("disconnect-one-conversation")).show(ctx, |ui| {
                ui.heading("Disconnect this conversation?");
                if let Some(record) = app.conversation(id) {
                    let title = record.thread.as_ref().and_then(|thread| record.threads.iter().find(|t| &t.id == thread)).map(|t| t.title.as_str()).unwrap_or("New conversation");
                    ui.label(format!("{} · {}", record.provider_name(), title));
                    if record.approval_count() > 0 || record.busy() || matches!(record.observed_activity, ThreadWorkState::Working | ThreadWorkState::WaitingOnApproval | ThreadWorkState::WaitingOnInput) {
                        ui.label("This conversation may have work or an approval in progress.");
                    }
                    ui.label("Closing this connection stops live updates and may interrupt the local agent process. Remote work may continue or stop depending on the provider.");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() { decision = Some(false); }
                        if ui.add_enabled(record.client_attached && record.client_generation == generation, egui::Button::new("Disconnect")).clicked() { decision = Some(true); }
                    });
                } else { decision = Some(false); }
            });
            if decision == Some(true) {
                self.actions.push(Self::action(
                    id,
                    generation,
                    ConversationActionKind::Disconnect,
                ));
                self.pending_disconnect = None;
            } else if decision == Some(false) || modal.should_close() {
                self.pending_disconnect = None;
            }
        }
        if self.confirm_disconnect_all {
            let count: usize = app
                .active_conversation_counts()
                .iter()
                .map(|(_, count)| *count)
                .sum();
            let modal=egui::Modal::new(egui::Id::new("disconnect-all-conversations")).show(ctx,|ui| {
                ui.heading("Disconnect all conversations?");
                ui.label(format!("Close {count} client connection{}?", if count == 1 { "" } else { "s" }));
                ui.label("Closing connections stops live updates and may interrupt the local agent processes. Remote work may continue or stop depending on the provider.");
                ui.horizontal(|ui| {if ui.button("Cancel").clicked() {self.confirm_disconnect_all=false;} if ui.add_enabled(count > 0, egui::Button::new("Disconnect all")).clicked() {self.confirm_disconnect_all=false;self.actions.push(ViewAction::ConfirmDisconnectAll);}});
            });
            if modal.should_close() {
                self.confirm_disconnect_all = false;
            }
        }
    }
}

impl ChatView {
    fn plan_rows(&mut self, record: &ConversationRecord, ui: &mut egui::Ui) {
        if record.context_data.plan.is_empty() && record.context_data.plan_text.is_none() {
            return;
        }
        egui::CollapsingHeader::new("Plan")
            .id_salt(("conversation-plan", &record.thread))
            .default_open(
                record
                    .context_data
                    .plan
                    .iter()
                    .any(|s| s.status == "inProgress"),
            )
            .show(ui, |ui| {
                if let Some(text) = &record.context_data.plan_text {
                    self.markdown.show(ui, ("plan-text", &record.thread), text);
                }
                for step in &record.context_data.plan {
                    ui.horizontal_top(|ui| {
                        match step.status.as_str() {
                            "inProgress" | "in_progress" if record.active_turn.is_some() => {
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
    fn transcript(
        &mut self,
        record: &ConversationRecord,
        data: &mut ContextData,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) -> Option<(String, Option<String>)> {
        let mut references = None;
        egui::ScrollArea::vertical()
            .id_salt(("conversation-transcript", record.chat_id))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .max_height(
                (ui.available_height()
                    - self.composer_height
                    - if record.context_data.goal.is_some() {
                        44.0
                    } else {
                        0.0
                    }
                    - record.approval_count() as f32 * 160.0)
                    .max(100.0),
            )
            .show(ui, |ui| {
                for (index, message) in record.messages.iter().enumerate() {
                    if message.uncertain {
                        ui.weak("Previous observation · not confirmed in the replay");
                    }
                    if let Some(activity) = &message.activity {
                        let (status, failed) = chat_status(&activity.status, None);
                        let running = status == "Running"
                            && message.key.as_ref().is_some_and(|(thread, turn, _)| {
                                record.thread.as_ref() == Some(thread)
                                    && record.active_turn.as_ref() == Some(turn)
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
                                record.thread.as_ref() == Some(thread)
                                    && record.active_turn.as_ref() == Some(turn)
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
                        ui.strong(record.provider_name());
                        self.markdown.show(
                            ui,
                            (&record.thread, &message.key, index),
                            &message.text,
                        );
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
                            references = Some((
                                message.text.clone(),
                                message.key.as_ref().and_then(|(_, turn, _)| {
                                    (!turn.is_empty()).then(|| turn.clone())
                                }),
                            ));
                        }
                    }
                    ui.add_space(if message.command.is_some() || message.activity.is_some() {
                        6.0
                    } else {
                        20.0
                    });
                }
                let cwd = record.provider_cwd();
                self.context.transcript_files(data, ui, ctx, &cwd);
                self.plan_rows(record, ui);
                if record.starting || record.active_turn.is_some() {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(12.0));
                        ui.weak(if record.starting {
                            "Starting…"
                        } else {
                            "Working…"
                        });
                    });
                }
            });
        references
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
#[cfg(feature = "codex")]
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
fn efforts(record: &ConversationRecord) -> (Option<String>, Vec<model_picker::EffortOption>) {
    #[cfg(feature = "kimi")]
    if record.is_kimi() {
        if let Some(option) = record.kimi.as_ref().and_then(|k| {
            k.options
                .iter()
                .find(|o| o.category.as_deref() == Some("thought_level"))
        }) {
            return (
                Some(option.current.clone()),
                option
                    .values()
                    .map(|v| model_picker::EffortOption {
                        id: v.value.clone(),
                        name: v.name.clone(),
                        description: v.description.clone(),
                    })
                    .collect(),
            );
        }
        return (None, Vec::new());
    }
    #[cfg(feature = "codex")]
    {
        let options = record
            .models
            .iter()
            .find(|m| Some(&m.model) == record.selected_model.as_ref())
            .map(|m| {
                m.efforts
                    .iter()
                    .map(|(id, description)| model_picker::EffortOption {
                        id: id.clone(),
                        name: effort_label(id).into(),
                        description: description.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        return (record.selected_effort.clone(), options);
    }
    #[cfg(not(feature = "codex"))]
    (None, Vec::new())
}
fn native_controls(
    ui: &mut egui::Ui,
    record: &ConversationRecord,
    view: &mut ChatView,
    actions: &mut Vec<ViewAction>,
) {
    let id = record.chat_id;
    let generation = record.client_generation;
    let enabled =
        record.ready && !record.busy() && record.model_intent.is_none() && !record.disconnecting;
    #[cfg(feature = "kimi")]
    if record.is_kimi() {
        if let Some(state) = &record.kimi {
            for option in state
                .options
                .iter()
                .filter(|o| !matches!(o.category.as_deref(), Some("model" | "thought_level")))
            {
                let button = ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(format!("{}: {} ⌄", option.name, option.current))
                            .frame(false),
                    )
                    .on_hover_text(&option.description);
                egui::Popup::menu(&button).show(|ui| {
                    for group in &option.groups {
                        if let Some(name) = &group.name {
                            ui.weak(name);
                        }
                        for value in &group.values {
                            if ui
                                .selectable_label(option.current == value.value, &value.name)
                                .on_hover_text(&value.description)
                                .clicked()
                            {
                                actions.push(ConversationView::action(
                                    id,
                                    generation,
                                    ConversationActionKind::SetKimiOption {
                                        id: option.id.clone(),
                                        value: value.value.clone(),
                                    },
                                ));
                                ui.close();
                            }
                        }
                    }
                });
            }
        }
        return;
    }
    #[cfg(feature = "codex")]
    {
        use crate::codex::{ApprovalMode, WorkMode};
        let mut controls = record.controls;
        let mut changed = false;
        let approval = ui
            .add_enabled(
                enabled,
                egui::Button::new(format!("{:?} ⌄", controls.approval)).frame(false),
            )
            .on_hover_text("Command approval mode; vault access is separate");
        egui::Popup::menu(&approval).show(|ui| {
            for (mode, label) in [
                (ApprovalMode::Ask, "Ask for approval"),
                (ApprovalMode::AutoReview, "Approve for me"),
                (ApprovalMode::NoPrompts, "No prompts"),
                (ApprovalMode::FullAccess, "Full access"),
            ] {
                if ui
                    .selectable_label(controls.approval == mode, label)
                    .clicked()
                {
                    controls.approval = mode;
                    changed = true;
                    ui.close();
                }
            }
        });
        let access = ui.add_enabled(enabled, egui::Button::new("Access ⌄").frame(false));
        egui::Popup::menu(&access).show(|ui| {
            ui.label("Vault tools use the independent Tanglewood permission.");
            if controls.approval == ApprovalMode::FullAccess {
                ui.label("Full access permits unrestricted file and network access.");
            } else if ui
                .checkbox(&mut controls.network_access, "Allow network access")
                .changed()
            {
                changed = true;
            }
        });
        let mode = ui.add_enabled(
            enabled && record.context_data.goal_loaded,
            egui::Button::new(format!("{:?} ⌄", controls.mode)).frame(false),
        );
        egui::Popup::menu(&mode).show(|ui| {
            for (choice, label) in [
                (WorkMode::Work, "Work"),
                (WorkMode::Plan, "Plan"),
                (WorkMode::Research, "Research"),
                (WorkMode::Goal, "Goal"),
            ] {
                if ui
                    .selectable_label(controls.mode == choice, label)
                    .clicked()
                {
                    if choice == WorkMode::Goal {
                        view.context.open_goal_dialog(
                            record
                                .context_data
                                .goal
                                .as_ref()
                                .map(|g| g.objective.clone())
                                .unwrap_or_else(|| record.prompt.clone()),
                        );
                    } else {
                        controls.mode = choice;
                        controls.network_access = choice == WorkMode::Research;
                        changed = true;
                    }
                    ui.close();
                }
            }
        });
        if changed {
            actions.push(ConversationView::action(
                id,
                generation,
                ConversationActionKind::SetCodexControls(controls),
            ));
        }
    }
}

#[cfg(all(test, feature = "kimi"))]
mod tests {
    use super::*;
    use crate::{
        conversation::Request,
        kimi::{KimiConfig, KimiRequest},
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    #[test]
    fn hidden_error_is_visible_and_selects_its_conversation() {
        use eframe::egui::accesskit;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let cwd = std::env::temp_dir();
        let config = KimiConfig {
            executable: cwd.join("unused"),
            cwd: cwd.clone(),
            mcp_bridge: cwd.join("bridge"),
            socket: cwd.join("socket"),
            resume_session: None,
        };
        let mut app = App::new();
        let selected = app
            .insert_conversation(ConversationRecord::new_kimi(config.clone()))
            .unwrap();
        let mut hidden = ConversationRecord::new_kimi(config);
        hidden.error = Some("Fixture connection failed".into());
        hidden.observed_activity = ThreadWorkState::Error;
        let hidden = app.insert_conversation(hidden).unwrap();
        let mut view = ConversationView::new(selected);
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500.0, 600.0),
            )),
            ..Default::default()
        };
        let output = ctx.run(input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| view.sidebar(ui, &app));
        });
        let update = output
            .platform_output
            .accesskit_update
            .expect("accessibility tree");
        let (target, node) = update
            .nodes
            .iter()
            .find(|(_, node)| {
                node.label()
                    .is_some_and(|label| label.contains("⚠") && label.contains("Kimi"))
            })
            .expect("hidden error indicator");
        assert!(node.supports_action(accesskit::Action::Click));
        let mut clicked = input();
        clicked.events.push(egui::Event::AccessKitActionRequest(
            accesskit::ActionRequest {
                action: accesskit::Action::Click,
                target: *target,
                data: None,
            },
        ));
        let _ = ctx.run(clicked, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| view.sidebar(ui, &app));
        });
        assert_eq!(view.selected, hidden);
        assert_eq!(
            app.conversation(hidden).unwrap().error.as_deref(),
            Some("Fixture connection failed")
        );
    }

    #[test]
    fn destroying_view_does_not_stop_stream_or_lose_draft() {
        let root = std::env::temp_dir().join(format!("tw-view-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/fixtures/model-picker/agent.py");
        let audit = root.join("audit");
        let state = root.join("state");
        let wrapper = root.join("kimi");
        fs::write(&wrapper,format!("#!/bin/sh\nTW_FAKE_AUDIT='{}' TW_FAKE_STATE='{}' TW_FAKE_BEHAVIOR=long_stream exec python3 '{}' \"$@\"\n",audit.display(),state.display(),fixture.display())).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let config = KimiConfig {
            executable: wrapper,
            cwd: root.clone(),
            mcp_bridge: root.join("bridge"),
            socket: root.join("socket"),
            resume_session: None,
        };
        let mut shell = super::super::TanglewoodUi::new(App::new()).with_kimi(config);
        let id = shell.conversation.as_ref().unwrap().selected;
        shell.app.connect_conversation(id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        while !shell
            .app
            .conversation(id)
            .is_some_and(|r| r.project_ready && !r.threads_loading)
        {
            shell.app.tick();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(8));
        }
        let generation = shell.app.conversation(id).unwrap().client_generation;
        shell
            .app
            .execute_conversation(ConversationAction {
                chat_id: id,
                expected_generation: Some(generation),
                kind: ConversationActionKind::Request(Request::Kimi(KimiRequest::NewSession)),
            })
            .unwrap();
        while !shell
            .app
            .conversation(id)
            .is_some_and(|r| r.ready && r.confirmed_model.is_some())
        {
            shell.app.tick();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(8));
        }
        shell
            .app
            .execute_conversation(ConversationAction {
                chat_id: id,
                expected_generation: Some(generation),
                kind: ConversationActionKind::SetDraft {
                    expected_revision: 0,
                    text: "retained draft".into(),
                },
            })
            .unwrap();
        shell
            .app
            .execute_conversation(ConversationAction {
                chat_id: id,
                expected_generation: Some(generation),
                kind: ConversationActionKind::Submit { evidence: None },
            })
            .unwrap();
        while !shell
            .app
            .conversation(id)
            .is_some_and(|r| r.messages.iter().any(|m| m.role == "Kimi"))
        {
            shell.app.tick();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(8));
        }
        let before = shell
            .app
            .conversation(id)
            .unwrap()
            .messages
            .iter()
            .filter(|m| m.role == "Kimi")
            .map(|m| m.text.len())
            .sum::<usize>();
        shell.conversation = None;
        shell.sidebar_view = super::super::SidebarView::Notes;
        let context = egui::Context::default();
        while shell
            .app
            .conversation(id)
            .unwrap()
            .messages
            .iter()
            .filter(|m| m.role == "Kimi")
            .map(|m| m.text.len())
            .sum::<usize>()
            <= before
        {
            let _ = context.run(egui::RawInput::default(), |ctx| shell.show_frame(ctx));
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(shell.app.conversation(id).unwrap().prompt, "retained draft");
        shell.conversation = Some(ConversationView::new(id));
        assert!(shell.app.conversation(id).unwrap().client_attached);
        let before_disconnect = fs::read_to_string(&audit).unwrap();
        assert_eq!(
            before_disconnect
                .lines()
                .filter(|line| line.contains("session/prompt"))
                .count(),
            1
        );
        assert!(
            !before_disconnect
                .lines()
                .any(|line| line.contains("session/cancel"))
        );
        shell
            .app
            .execute_conversation(ConversationAction {
                chat_id: id,
                expected_generation: Some(generation),
                kind: ConversationActionKind::Disconnect,
            })
            .unwrap();
        while !shell.app.begin_conversation_shutdown() {
            shell.app.tick();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(8));
        }
        drop(shell);
        let _ = fs::remove_dir_all(root);
    }
}
