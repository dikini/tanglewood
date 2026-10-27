//! Kimi-specific state and controls for the shared conversation surface.
use super::*;
use crate::kimi::{
    ConfigOption, KimiAdapter, KimiConfig, KimiEvent, KimiRequest, PermissionOption,
};

#[derive(Default)]
struct Parked {
    project_ready: bool,
    binding_durable: bool,
    fresh_thread: bool,
    thread: Option<String>,
    threads: Vec<crate::conversation::ThreadSummary>,
    threads_cursor: Option<String>,
    prompt_drafts: HashMap<String, String>,
    unavailable_threads: std::collections::HashSet<String>,
    prompt: String,
    context: codex_context::ContextUi,
    messages: Vec<ConversationMessage>,
    markdown: codex_markdown::Cache,
    history_cursor: Option<String>,
    status: String,
    error: Option<String>,
}
impl Parked {
    fn swap(&mut self, panel: &mut ConversationPanel) {
        std::mem::swap(&mut self.project_ready, &mut panel.project_ready);
        std::mem::swap(&mut self.binding_durable, &mut panel.binding_durable);
        std::mem::swap(&mut self.fresh_thread, &mut panel.fresh_thread);
        std::mem::swap(&mut self.thread, &mut panel.thread);
        std::mem::swap(&mut self.threads, &mut panel.threads);
        std::mem::swap(&mut self.threads_cursor, &mut panel.threads_cursor);
        std::mem::swap(&mut self.prompt_drafts, &mut panel.prompt_drafts);
        std::mem::swap(
            &mut self.unavailable_threads,
            &mut panel.unavailable_threads,
        );
        std::mem::swap(&mut self.prompt, &mut panel.prompt);
        std::mem::swap(&mut self.context, &mut panel.context);
        std::mem::swap(&mut self.messages, &mut panel.messages);
        std::mem::swap(&mut self.markdown, &mut panel.markdown);
        std::mem::swap(&mut self.history_cursor, &mut panel.history_cursor);
        std::mem::swap(&mut self.status, &mut panel.status);
        std::mem::swap(&mut self.error, &mut panel.error);
    }
}
pub(super) struct Permission {
    id: serde_json::Value,
    details: String,
    options: Vec<PermissionOption>,
    answering: bool,
}
struct Submission {
    session: String,
    id: String,
    text: String,
    attachments: Vec<crate::conversation::PromptAttachment>,
}
pub(super) struct KimiState {
    pub(super) config: KimiConfig,
    pub(super) adapter: Option<KimiAdapter>,
    pub(super) option_pending: bool,
    pub(super) permissions: Vec<Permission>,
    options: Vec<ConfigOption>,
    known: std::collections::HashSet<String>,
    submission: Option<Submission>,
    confirmed_session: Option<String>,
    cancelling: bool,
    generation: u64,
    resume: Option<String>,
    switch_to: Option<bool>,
    codex_available: bool,
    parked_codex: Parked,
    parked_kimi: Parked,
}
impl KimiState {
    fn new(config: KimiConfig, codex_available: bool) -> Self {
        let resume = config.resume_session.clone();
        Self {
            config,
            adapter: None,
            option_pending: false,
            permissions: Vec::new(),
            options: Vec::new(),
            known: Default::default(),
            submission: None,
            confirmed_session: None,
            cancelling: false,
            generation: 0,
            resume,
            switch_to: None,
            codex_available,
            parked_codex: Parked::default(),
            parked_kimi: Parked::default(),
        }
    }
    pub(super) fn permission_bytes(&self) -> usize {
        self.permissions
            .iter()
            .map(|p| {
                p.id.to_string().len()
                    + p.details.len()
                    + p.options
                        .iter()
                        .map(|o| o.id.len() + o.name.len() + o.kind.len())
                        .sum::<usize>()
            })
            .sum()
    }
}
impl TanglewoodUi {
    pub fn with_kimi(mut self, config: KimiConfig) -> Self {
        if let Some(panel) = &mut self.conversation {
            panel.kimi = Some(KimiState::new(config, true));
        } else {
            self.conversation = Some(ConversationPanel::new_kimi(config));
        }
        self
    }
}
impl ConversationPanel {
    fn new_kimi(config: KimiConfig) -> Self {
        #[cfg(feature = "codex")]
        {
            let mut panel = Self::new(crate::codex::CodexConfig {
                executable: "codex".into(),
                cwd: config.cwd.clone(),
                mcp_bridge: config.mcp_bridge.clone(),
                socket: config.socket.clone(),
                resume_thread: None,
                project: None,
            });
            panel.kimi = Some(KimiState::new(config, false));
            panel.active_kimi = true;
            panel.binding_durable = true;
            panel
        }
        #[cfg(not(feature = "codex"))]
        {
            Self {
                kimi: Some(KimiState::new(config, false)),
                active_kimi: true,
                vault_permission_requested: None,
                vault_write_enabled: None,
                research_requested: false,
                collect_references: None,
                relevant_sources: false,
                research_workspace: None,
                relevant_cache: None,
                project_ready: false,
                fresh_thread: false,
                resume_requested: false,
                pending_thread: None,
                binding_durable: true,
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
                project_name: String::new(),
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
                status: "Disconnected".into(),
                error: None,
            }
        }
    }
    pub(super) fn backend_picker(&mut self, ui: &mut egui::Ui) {
        if !self.kimi.as_ref().is_some_and(|k| k.codex_available) {
            return;
        }
        let enabled = !self.busy()
            && !self.history_loading
            && self.pending_thread.is_none()
            && self.approval_count() == 0
            && !self.disconnecting
            && self.kimi.as_ref().is_none_or(|k| k.switch_to.is_none());
        let current = self.provider_name();
        let button = ui
            .add_enabled(
                enabled,
                egui::Button::new(format!("{current} ⌄")).frame(false),
            )
            .on_hover_text("Choose conversation backend");
        button.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                enabled,
                "Choose conversation backend",
            )
        });
        egui::Popup::menu(&button).show(|ui| {
            for (kimi, label) in [(false, "Codex"), (true, "Kimi")] {
                if ui
                    .selectable_label(self.active_kimi == kimi, label)
                    .clicked()
                    && self.active_kimi != kimi
                {
                    if let Some(state) = &mut self.kimi {
                        state.switch_to = Some(kimi);
                    }
                    if self.has_adapter() {
                        self.disconnect();
                    }
                    self.advance_backend_switch();
                    ui.close();
                }
            }
        });
    }
    pub(super) fn advance_backend_switch(&mut self) {
        let Some(target) = self.kimi.as_ref().and_then(|k| k.switch_to) else {
            return;
        };
        if self.has_adapter() {
            return;
        }
        let Some(mut state) = self.kimi.take() else {
            return;
        };
        if self.active_kimi {
            state.parked_kimi.swap(self);
            state.parked_codex.swap(self);
        } else {
            state.parked_codex.swap(self);
            state.parked_kimi.swap(self);
        }
        self.active_kimi = target;
        self.ready = false;
        self.starting = false;
        self.disconnecting = false;
        self.pending_thread = None;
        self.history_loading = false;
        self.threads_loading = false;
        self.active_turn = None;
        if self.status.is_empty() {
            self.status = "Disconnected".into();
        }
        state.switch_to = None;
        self.kimi = Some(state);
    }
    fn send_kimi(&mut self, request: KimiRequest) -> bool {
        let result = self
            .kimi
            .as_ref()
            .and_then(|k| k.adapter.as_ref())
            .ok_or_else(|| "Connect to Kimi first".to_owned())
            .and_then(|a| a.try_send(request));
        match result {
            Ok(()) => true,
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }
    fn connect_kimi(&mut self, ctx: &egui::Context) {
        if self.bytes() >= Self::BYTE_LIMIT {
            self.error = Some("Clear the conversation before connecting.".into());
            return;
        }
        let Some(state) = &mut self.kimi else {
            return;
        };
        state.generation = state.generation.saturating_add(1);
        state.confirmed_session = None;
        state.cancelling = false;
        state.resume = state.config.resume_session.clone();
        let context = ctx.clone();
        match KimiAdapter::spawn(state.config.clone(), move || context.request_repaint()) {
            Ok(adapter) => {
                state.adapter = Some(adapter);
                self.status = "Connecting…".into();
                self.ready = false;
                self.disconnecting = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    pub(super) fn disconnect_kimi(&mut self) {
        if self.disconnecting {
            return;
        }
        if self.send_kimi(KimiRequest::Disconnect) {
            self.disconnecting = true;
            self.status = "Disconnecting…".into();
        }
    }
    pub(super) fn select_kimi(&mut self, id: Option<String>) {
        if self.busy()
            || self.threads_loading
            || self.pending_thread.is_some()
            || self.history_loading
            || !self.project_ready
            || self.approval_count() != 0
        {
            return;
        }
        if id.is_some() && id == self.thread && self.ready {
            return;
        }
        if id
            .as_ref()
            .is_some_and(|id| !self.kimi.as_ref().is_some_and(|k| k.known.contains(id)))
        {
            self.error = Some("This Kimi conversation is no longer listed; any existing draft remains in the current view.".into());
            return;
        }
        if self.fresh_thread
            && id.is_some()
            && id != self.thread
            && (!self.prompt.trim().is_empty() || self.context.has_draft())
        {
            self.error =
                Some("Send or clear this new conversation's draft before opening another.".into());
            return;
        }
        if self.prompt_drafts.len() >= 256 || self.bytes() + self.prompt.len() > Self::BYTE_LIMIT {
            self.error =
                Some("Conversation drafts limit reached; clear a draft before switching.".into());
            return;
        }
        let request = id
            .clone()
            .map_or(KimiRequest::NewSession, KimiRequest::LoadSession);
        if self.send_kimi(request) {
            self.pending_thread = Some(id);
            self.ready = false;
            self.starting = true;
            self.error = None;
            self.status = "Opening conversation…".into();
        }
    }
    fn apply_kimi_open(
        &mut self,
        session: String,
        options: Vec<ConfigOption>,
        history: Vec<crate::conversation::HistoryMessage>,
    ) {
        if let Some(target) = self.pending_thread.take() {
            let carry = self.fresh_thread && target.is_none();
            self.context
                .switch(self.thread.as_deref().unwrap_or_default(), &session, carry);
            if !carry {
                self.prompt_drafts.insert(
                    self.thread.clone().unwrap_or_default(),
                    std::mem::take(&mut self.prompt),
                );
                self.prompt = self.prompt_drafts.remove(&session).unwrap_or_default();
            }
            self.fresh_thread = target.is_none();
        }
        self.messages = history
            .into_iter()
            .map(|m| ConversationMessage {
                role: if m.role == "user" { "You" } else { "Kimi" },
                key: None,
                text: m.text,
                command: None,
                activity: None,
            })
            .collect();
        self.markdown = Default::default();
        self.history_cursor = None;
        self.thread = Some(session.clone());
        self.ready = true;
        self.starting = false;
        self.status = "Ready".into();
        if let Some(state) = &mut self.kimi {
            state.known.insert(session.clone());
            state.confirmed_session = Some(session.clone());
            state.config.resume_session = Some(session.clone());
            state.options = options;
            state.option_pending = false;
        }
        if !self.threads.iter().any(|t| t.id == session) {
            self.threads.insert(
                0,
                crate::conversation::ThreadSummary {
                    id: session,
                    title: "New conversation".into(),
                    updated_at: 0,
                    state: ThreadWorkState::Idle,
                },
            );
        }
    }
    fn kimi_open_failed(&mut self, error: String) {
        self.pending_thread = None;
        self.starting = false;
        self.ready = self.thread.as_ref().is_some_and(|thread| {
            self.kimi
                .as_ref()
                .and_then(|k| k.confirmed_session.as_ref())
                == Some(thread)
        });
        self.status = if self.ready { "Ready" } else { "Connected" }.into();
        self.error = Some(error);
    }
    fn kimi_submission_matches(&self, session: &str, id: &str) -> bool {
        self.thread.as_deref() == Some(session)
            && self
                .kimi
                .as_ref()
                .and_then(|k| k.submission.as_ref())
                .is_some_and(|s| s.session == session && s.id == id)
    }
    fn remove_kimi_submission_row(&mut self, session: &str, id: &str) {
        self.messages.retain(|m| {
            m.key.as_ref().is_none_or(|(thread, turn, item)| {
                !(thread == session && turn == id && item == "user")
            })
        });
    }
    fn kimi_turn_finished(
        &mut self,
        session: String,
        turn: String,
        submission: String,
        status: String,
    ) {
        if self.active_turn.as_ref() != Some(&turn)
            || !self.kimi_submission_matches(&session, &submission)
        {
            return;
        }
        self.retire_activity(Some(&session), Some(&turn), "ended");
        self.active_turn = None;
        self.starting = false;
        // A terminal response confirms the first prompt reached this session,
        // including cancellation; its retained draft can now be parked normally.
        self.fresh_thread = false;
        let cancelled = status == "cancelled" || status == "canceled";
        self.status = if cancelled {
            "Turn cancelled · draft retained".into()
        } else {
            format!("Turn {status}")
        };
        if let Some(k) = &mut self.kimi {
            k.permissions.clear();
            k.cancelling = false;
            if let Some(shadow) = k.submission.take() {
                if !cancelled
                    && self.prompt == shadow.text
                    && self
                        .context
                        .draft
                        .attachments
                        .iter()
                        .map(|a| (&a.name, &a.text))
                        .eq(shadow.attachments.iter().map(|a| (&a.name, &a.text)))
                {
                    self.prompt.clear();
                    self.context.draft.attachments.clear();
                }
            }
        }
        if cancelled {
            self.remove_kimi_submission_row(&session, &submission);
        }
    }
    fn kimi_turn_failed(
        &mut self,
        session: String,
        turn: String,
        submission: String,
        error: String,
    ) {
        if !self.kimi_submission_matches(&session, &submission) {
            return;
        }
        self.retire_activity(Some(&session), Some(&turn), "stopped");
        self.active_turn = None;
        self.starting = false;
        self.remove_kimi_submission_row(&session, &submission);
        if let Some(k) = &mut self.kimi {
            k.permissions.clear();
            k.cancelling = false;
            k.submission = None;
        }
        self.error = Some(format!(
            "{error}\nThe message and attached context remain in the composer; inspect the session before retrying."
        ));
    }
    fn cancel_kimi(&mut self) {
        if self.kimi.as_ref().is_some_and(|k| k.cancelling) {
            return;
        }
        if self.send_kimi(KimiRequest::Cancel) {
            if let Some(k) = &mut self.kimi {
                k.cancelling = true;
                k.permissions.clear();
            }
            self.status = "Stopping…".into();
        }
    }
    pub(super) fn poll_kimi(&mut self, ctx: &egui::Context) {
        self.context.poll(ctx);
        for _ in 0..64 {
            let Some(event) = self
                .kimi
                .as_ref()
                .and_then(|k| k.adapter.as_ref())
                .and_then(KimiAdapter::try_recv)
            else {
                return;
            };
            match event {
                KimiEvent::Connecting => self.status = "Connecting…".into(),
                KimiEvent::Connected { .. } => {
                    self.project_ready = true;
                    self.binding_durable = true;
                    self.status = "Connected".into();
                    self.threads_loading =
                        self.send_kimi(KimiRequest::ListSessions { cursor: None });
                }
                KimiEvent::Sessions {
                    sessions,
                    next_cursor,
                } => {
                    self.threads_loading = false;
                    if !self.threads_append {
                        if let Some(k) = &mut self.kimi {
                            k.known.clear();
                        }
                        self.threads.retain(|t| {
                            Some(&t.id) == self.thread.as_ref()
                                || self.prompt_drafts.contains_key(&t.id)
                        });
                    }
                    for session in sessions {
                        if let Some(k) = &mut self.kimi {
                            k.known.insert(session.id.clone());
                        }
                        if let Some(old) = self.threads.iter_mut().find(|t| t.id == session.id) {
                            *old = session;
                        } else if self.threads.len() < 256 {
                            self.threads.push(session);
                        }
                    }
                    self.threads_cursor = next_cursor;
                    let resume = self.kimi.as_ref().and_then(|k| k.resume.clone());
                    if let Some(id) = resume {
                        if self.threads.iter().any(|t| t.id == id) {
                            if let Some(k) = &mut self.kimi {
                                k.resume = None;
                            }
                            self.select_kimi(Some(id));
                        } else if let Some(cursor) = self.threads_cursor.clone() {
                            self.threads_append = true;
                            self.threads_loading = self.send_kimi(KimiRequest::ListSessions {
                                cursor: Some(cursor),
                            });
                        } else {
                            if let Some(k) = &mut self.kimi {
                                k.resume = None;
                            }
                            self.error = Some(
                                "Requested Kimi session is not listed in this workspace.".into(),
                            );
                        }
                    }
                }
                KimiEvent::Opened {
                    session,
                    options,
                    history,
                } => self.apply_kimi_open(session, options, history),
                KimiEvent::OpenFailed { error, .. } => self.kimi_open_failed(error),
                KimiEvent::Options {
                    session,
                    options,
                    settled,
                } => {
                    if self.thread.as_ref() == Some(&session) {
                        if let Some(k) = &mut self.kimi {
                            k.options = options;
                            if settled {
                                k.option_pending = false;
                            }
                        }
                    }
                }
                KimiEvent::OptionFailed { session, error } => {
                    if self.thread.as_ref() == Some(&session) {
                        if let Some(k) = &mut self.kimi {
                            k.option_pending = false;
                        }
                        self.error = Some(error);
                    }
                }
                KimiEvent::TurnStarted {
                    session,
                    turn,
                    submission,
                } => {
                    if self.kimi_submission_matches(&session, &submission) {
                        self.starting = false;
                        self.active_turn = Some(turn);
                        self.status = "Streaming…".into();
                    }
                }
                KimiEvent::Delta {
                    session,
                    turn,
                    item,
                    text,
                } => {
                    if self.thread.as_ref() == Some(&session)
                        && self.active_turn.as_ref() == Some(&turn)
                    {
                        self.message((session, turn, item), text, false);
                    }
                }
                KimiEvent::Activity {
                    session,
                    turn,
                    activity,
                } => {
                    if self.thread.as_ref() == Some(&session)
                        && self.active_turn.as_ref() == Some(&turn)
                    {
                        self.activity_card(session, turn, activity);
                    }
                }
                KimiEvent::Plan {
                    session,
                    turn,
                    steps,
                } => {
                    if self.thread.as_ref() == Some(&session)
                        && self.active_turn.as_ref() == Some(&turn)
                    {
                        self.context.plan = steps;
                        self.context.plan_text = None;
                    }
                }
                KimiEvent::Permission {
                    session,
                    turn,
                    id,
                    details,
                    options,
                } => {
                    if self.thread.as_ref() == Some(&session)
                        && self.active_turn.as_ref() == Some(&turn)
                    {
                        if let Some(k) = &mut self.kimi {
                            if !k.cancelling && k.permissions.len() < 16 {
                                k.permissions.push(Permission {
                                    id,
                                    details,
                                    options,
                                    answering: false,
                                });
                            }
                        }
                    }
                }
                KimiEvent::PermissionResolved { session, id } => {
                    if self.thread.as_ref() == Some(&session) {
                        if let Some(k) = &mut self.kimi {
                            k.permissions.retain(|p| p.id != id);
                        }
                    }
                }
                KimiEvent::TurnFinished {
                    session,
                    turn,
                    submission,
                    status,
                } => self.kimi_turn_finished(session, turn, submission, status),
                KimiEvent::TurnFailed {
                    session,
                    turn,
                    submission,
                    error,
                } => self.kimi_turn_failed(session, turn, submission, error),
                KimiEvent::Deleted { .. } => {}
                KimiEvent::Error(error) => {
                    self.threads_loading = false;
                    self.error = Some(error);
                }
                KimiEvent::Disconnected(error) => {
                    self.retire_activity(None, None, "stopped");
                    self.pending_thread = None;
                    self.starting = false;
                    self.active_turn = None;
                    self.ready = false;
                    self.project_ready = false;
                    let intentional = self.disconnecting;
                    self.disconnecting = false;
                    self.threads_loading = false;
                    let unconfirmed = self
                        .kimi
                        .as_ref()
                        .and_then(|k| k.submission.as_ref())
                        .map(|s| (s.session.clone(), s.id.clone()));
                    if let Some((session, id)) = unconfirmed {
                        self.remove_kimi_submission_row(&session, &id);
                    }
                    let switching = self.kimi.as_ref().is_some_and(|k| k.switch_to.is_some());
                    if let Some(k) = &mut self.kimi {
                        k.adapter = None;
                        k.option_pending = false;
                        k.permissions.clear();
                        k.submission = None;
                        k.confirmed_session = None;
                        k.cancelling = false;
                    }
                    if !switching && !intentional {
                        self.error = Some(format!(
                            "{error}\nAny unconfirmed message and context remain in the composer."
                        ));
                    }
                    self.status = "Disconnected".into();
                    self.advance_backend_switch();
                    ctx.request_repaint();
                    return;
                }
            }
        }
        ctx.request_repaint();
    }
    pub(super) fn kimi_sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.strong("Kimi");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let connected = self.has_adapter();
                let button = theme::button(
                    ui,
                    theme::Icon::Connections,
                    if connected {
                        "Disconnect Kimi"
                    } else {
                        "Connect Kimi"
                    },
                    !self.disconnecting,
                ).on_hover_text("Kimi uses your ordinary process privileges; its mode controls tool approvals. Vault updates use the separate Tanglewood permission.");
                if button.clicked() {
                    if connected {
                        self.disconnect();
                    } else {
                        self.connect_kimi(ctx);
                    }
                }
            });
        });
        ui.add_space(12.0);
        if ui
            .add_enabled(
                self.project_ready && !self.busy() && !self.disconnecting,
                egui::Button::new("New conversation"),
            )
            .clicked()
        {
            self.select_kimi(None);
        }
        ui.horizontal(|ui| {
            ui.weak("Conversations");
            if ui
                .add_enabled(
                    self.project_ready && !self.threads_loading,
                    egui::Button::new("Refresh").frame(false),
                )
                .clicked()
            {
                self.threads_append = false;
                self.threads_loading = self.send_kimi(KimiRequest::ListSessions { cursor: None });
            }
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut select = None;
            for thread in &self.threads {
                let label =
                    ui.selectable_label(self.thread.as_ref() == Some(&thread.id), &thread.title);
                if label.on_hover_text(&thread.title).clicked() {
                    select = Some(thread.id.clone());
                }
            }
            if let Some(id) = select {
                self.select_kimi(Some(id));
            }
        });
        if let Some(cursor) = self.threads_cursor.clone() {
            if ui
                .add_enabled(
                    !self.threads_loading,
                    egui::Button::new("More conversations"),
                )
                .clicked()
            {
                self.threads_append = true;
                self.threads_loading = self.send_kimi(KimiRequest::ListSessions {
                    cursor: Some(cursor),
                });
            }
        }
    }
    fn kimi_options(&mut self, ui: &mut egui::Ui) {
        let options = self
            .kimi
            .as_ref()
            .map(|k| k.options.clone())
            .unwrap_or_default();
        let enabled = self.ready && !self.busy() && !self.disconnecting;
        for option in options {
            let current = option
                .values()
                .find(|v| v.value == option.current)
                .map(|v| v.name.as_str())
                .unwrap_or(&option.current);
            let response = ui
                .add_enabled(
                    enabled,
                    egui::Button::new(format!("{}: {} ⌄", option.name, current)).frame(false),
                )
                .on_hover_text(&option.description);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    enabled,
                    format!("Choose {}", option.name),
                )
            });
            egui::Popup::menu(&response).show(|ui| {
                for group in &option.groups {
                    if let Some(name) = &group.name {
                        ui.weak(name);
                    }
                    for choice in &group.values {
                        if ui
                            .selectable_label(option.current == choice.value, &choice.name)
                            .on_hover_text(&choice.description)
                            .clicked()
                        {
                            if self.send_kimi(KimiRequest::SetOption {
                                id: option.id.clone(),
                                value: choice.value.clone(),
                            }) {
                                if let Some(k) = &mut self.kimi {
                                    k.option_pending = true;
                                }
                            }
                            ui.close();
                        }
                    }
                }
            });
        }
    }
    pub(super) fn show_kimi(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &App,
        selected: Option<NoteId>,
        research: bool,
    ) {
        let generation = self.kimi.as_ref().map_or(0, |k| k.generation);
        let cwd = self.provider_cwd();
        ui.push_id(("kimi-conversation",cwd,generation),|ui| {
            if !research {
                self.backend_picker(ui);
                ui.horizontal(|ui|{
                    let title=self.thread.as_ref().and_then(|id|self.threads.iter().find(|t|&t.id==id)).map(|t|t.title.as_str()).unwrap_or("Kimi");
                    ui.add(egui::Label::new(egui::RichText::new(title).size(18.0)).truncate()).on_hover_text(title);
                    if ui.button("Context").on_hover_text("Show the reported plan").clicked(){
                        if ctx.data(|d|d.get_temp::<bool>(egui::Id::new("shell-context-undocked"))).unwrap_or(false){ctx.data_mut(|d|d.insert_temp(egui::Id::new("shell-pane-request"),Some(layout::Pane::Context)));}else{self.context.open=!self.context.open;}
                    }
                });
            }
            if !research || self.busy(){ui.weak(&self.status);}
            if self.has_adapter() || self.vault_write_enabled.is_some(){
                let more=theme::button(ui,theme::Icon::More,"Conversation actions",true);
                egui::Popup::menu(&more).show(|ui|{
                    if let Some(allowed)=self.vault_write_enabled {ui.menu_button(if allowed{"Vault updates · Allowed"}else{"Vault updates · Read only"},|ui|{if let Some(enabled)=agent_permission_controls(ui,app,allowed){self.vault_permission_requested=Some(enabled);}});}
                    if !research && ui.add_enabled(self.kimi_research_capture().current.is_some()&&!self.busy(),egui::Button::new("Start research from discussion…")).clicked(){self.research_requested=true;ui.close();}
                    if ui.add_enabled(!self.busy()&&self.approval_count()==0,egui::Button::new("Clear displayed messages")).clicked(){self.messages.clear();self.markdown=Default::default();ui.close();}
                });
            }
            if let Some(error)=&self.error {ui.colored_label(ui.visuals().error_fg_color,error);}
            self.transcript(ui,ctx);
            let mut answer=None;
            if let Some(k)=&self.kimi {for approval in &k.permissions {
                ui.group(|ui|{
                    ui.strong("Permission required");
                    egui::ScrollArea::vertical().id_salt(("kimi-permission",approval.id.to_string())).max_height(150.0).show(ui,|ui|{ui.add(egui::Label::new(egui::RichText::new(&approval.details).monospace()).selectable(true).wrap());});
                    ui.horizontal_wrapped(|ui|{for option in &approval.options {if ui.add_enabled(!approval.answering&&!self.disconnecting,egui::Button::new(&option.name)).on_hover_text(&option.kind).clicked(){answer=Some((approval.id.clone(),Some(option.id.clone())));}}if ui.add_enabled(!approval.answering,egui::Button::new("Cancel request")).clicked(){answer=Some((approval.id.clone(),None));}});
                });
            }}
            if let Some((id,option))=answer {if self.send_kimi(KimiRequest::AnswerPermission{id:id.clone(),option}){if let Some(k)=&mut self.kimi{if let Some(p)=k.permissions.iter_mut().find(|p|p.id==id){p.answering=true;}}}}
            let composer=egui::Frame::new().fill(ui.visuals().widgets.inactive.bg_fill).corner_radius(20).inner_margin(egui::Margin::symmetric(14,12)).show(ui,|ui|{
                egui::ScrollArea::vertical().id_salt("composer-body").max_height((ctx.content_rect().height()*0.35).clamp(48.0,320.0)).auto_shrink([false,true]).show(ui,|ui|{
                    ui.add_enabled_ui(!self.busy(),|ui|self.context.chips(ui));
                    let editable=self.ready&&!self.busy()&&!self.disconnecting;
                    let edit=ui.add(egui::TextEdit::multiline(&mut self.prompt).id(egui::Id::new(("kimi-composer",self.thread.as_deref(),generation))).frame(false).interactive(editable).hint_text(if self.thread.is_some(){"Message Kimi"}else{"Connect, then choose or create a conversation"}).desired_rows(3).desired_width(f32::INFINITY).char_limit(256*1024));
                    name_field(edit,&self.prompt,"Message Kimi");
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui|{
                    let can_context=self.ready&&!self.busy()&&!self.disconnecting;
                    let _=self.context.composer(ui,app,selected,can_context,true);
                    self.kimi_options(ui);
                    if self.starting && self.pending_thread.is_none() || self.active_turn.is_some() {
                        let can_stop=!self.disconnecting && self.kimi.as_ref().is_none_or(|k| !k.cancelling);
                        let stop=ui.add_enabled(can_stop,egui::Button::new("■").corner_radius(16).min_size(egui::vec2(32.0,32.0))).on_hover_text("Stop response");
                        stop.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,stop.enabled(),"Stop response"));
                        if stop.clicked(){self.cancel_kimi();}
                    }else{
                        let enabled=self.ready&&!self.busy()&&!self.disconnecting&&!self.prompt.trim().is_empty()&&self.prompt.len()+self.context.total_input_bytes()<=256*1024&&self.messages.len()<256;
                        let send=ui.add_enabled(enabled,egui::Button::new("↑").corner_radius(16).min_size(egui::vec2(32.0,32.0))).on_hover_text("Send message");
                        send.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,send.enabled(),"Send message"));
                        if send.clicked(){let text=self.prompt.clone();let attachments=self.context.draft.attachments.clone();let session=self.thread.clone().unwrap_or_default();let submission=uuid::Uuid::new_v4().to_string();
                            if self.send_kimi(KimiRequest::Prompt{session:session.clone(),submission:submission.clone(),text:text.clone(),attachments:attachments.clone()}){
                                if let Some(k)=&mut self.kimi{k.submission=Some(Submission{session:session.clone(),id:submission.clone(),text:text.clone(),attachments});}
                                self.messages.push(ConversationMessage{role:"You",key:Some((session,submission,"user".into())),text,command:None,activity:None});self.starting=true;self.status="Starting turn…".into();self.error=None;
                            }
                        }
                    }
                });
            });
            self.composer_height=composer.response.rect.height()+ui.spacing().item_spacing.y;
            if let Some(_action)=self.context.dialogs(ctx,self.ready&&!self.busy()) { /* Kimi does not offer Codex goal actions. */ }
        });
    }
    pub(super) fn kimi_research_capture(&self) -> research::ConversationCapture {
        if !self.project_ready {
            return Default::default();
        }
        let threads: Vec<_> = self
            .threads
            .iter()
            .filter(|t| self.kimi.as_ref().is_some_and(|k| k.known.contains(&t.id)))
            .map(|t| crate::research::ThreadRef {
                provider: "kimi".into(),
                project_id: None,
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
        if capture.current.is_none() || self.pending_thread.is_some() || !self.ready {
            return capture;
        }
        let messages: Vec<_> = self
            .messages
            .iter()
            .filter(|m| {
                m.command.is_none()
                    && m.activity.is_none()
                    && matches!(m.role, "You" | "Kimi")
                    && m.key
                        .as_ref()
                        .is_none_or(|(id, _, _)| self.thread.as_ref() == Some(id))
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
                .find(|m| m.role == "Kimi" && !m.text.trim().is_empty())
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
    pub(super) fn prepare_kimi_research(
        &mut self,
        prompt: String,
        evidence: String,
        external: bool,
    ) -> Result<(), &'static str> {
        if !self.prompt.is_empty()
            || self.context.has_draft()
            || self.pending_thread.is_some()
            || self.busy()
        {
            return Err(
                "Conversation has a draft, context, or active work. Copy the prepared prompt or finish that work first.",
            );
        }
        if !self.ready || self.thread.is_none() {
            return Err(
                "Connect and open or create a conversation first. The prepared question and sources remain in Research.",
            );
        }
        self.prompt = prompt;
        self.context
            .draft
            .attachments
            .push(crate::conversation::PromptAttachment {
                name: if external {
                    "Research context · external gathering"
                } else {
                    "Research sources"
                }
                .into(),
                text: evidence,
            });
        self.status="Research draft prepared · Kimi controls tool and network access · review before sending".into();
        Ok(())
    }
}

#[cfg(not(feature = "codex"))]
impl ConversationPanel {
    pub(super) fn research_capture(&self) -> research::ConversationCapture {
        self.kimi_research_capture()
    }
    pub(super) fn prepare_research_context(
        &mut self,
        prompt: String,
        evidence: String,
        external: bool,
    ) -> Result<(), &'static str> {
        self.prepare_kimi_research(prompt, evidence, external)
    }
    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        self.poll_kimi(ctx);
    }
    pub(super) fn select_thread(&mut self, id: Option<String>) {
        self.select_kimi(id);
    }
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.kimi_sidebar(ui, ctx);
    }
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &App,
        selected: Option<NoteId>,
    ) {
        self.show_kimi(ui, ctx, app, selected, false);
    }
    pub(super) fn show_surface(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &App,
        selected: Option<NoteId>,
        research: bool,
    ) {
        self.show_kimi(ui, ctx, app, selected, research);
    }
    pub(super) fn context_action(&mut self, _action: codex_context::Action) {}
}

#[cfg(all(test, feature = "codex"))]
mod tests {
    use super::*;
    #[test]
    fn provider_switch_keeps_same_id_drafts_and_kimi_research_identity_separate() {
        let config = crate::codex::CodexConfig {
            executable: "codex".into(),
            cwd: "/tmp/codex".into(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/socket".into(),
            resume_thread: None,
            project: None,
        };
        let kimi = KimiConfig {
            executable: "kimi".into(),
            cwd: "/tmp/kimi".into(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/socket".into(),
            resume_session: None,
        };
        let mut panel = ConversationPanel::new(config);
        panel.kimi = Some(KimiState::new(kimi, true));
        panel.thread = Some("same".into());
        panel.prompt = "Codex draft".into();
        panel
            .context
            .draft
            .attachments
            .push(crate::conversation::PromptAttachment {
                name: "C".into(),
                text: "Codex source".into(),
            });
        panel.kimi.as_mut().unwrap().switch_to = Some(true);
        panel.advance_backend_switch();
        assert!(panel.active_kimi);
        assert!(panel.prompt.is_empty());
        panel.thread = Some("same".into());
        panel.prompt = "Kimi draft".into();
        panel
            .context
            .draft
            .attachments
            .push(crate::conversation::PromptAttachment {
                name: "K".into(),
                text: "Kimi source".into(),
            });
        panel.project_ready = true;
        panel.ready = true;
        panel.threads.push(crate::conversation::ThreadSummary {
            id: "same".into(),
            title: "Kimi conversation".into(),
            updated_at: 0,
            state: ThreadWorkState::Idle,
        });
        panel.kimi.as_mut().unwrap().known.insert("same".into());
        let capture = panel.kimi_research_capture();
        assert_eq!(capture.current.as_ref().unwrap().provider, "kimi");
        assert_eq!(capture.current.as_ref().unwrap().project_id, None);
        panel.kimi.as_mut().unwrap().switch_to = Some(false);
        panel.advance_backend_switch();
        assert!(!panel.active_kimi);
        assert_eq!(panel.prompt, "Codex draft");
        assert_eq!(panel.context.draft.attachments[0].text, "Codex source");
        panel.kimi.as_mut().unwrap().switch_to = Some(true);
        panel.advance_backend_switch();
        assert_eq!(panel.prompt, "Kimi draft");
        assert_eq!(panel.context.draft.attachments[0].text, "Kimi source");
    }
    #[test]
    fn submissions_and_open_failures_keep_only_the_current_draft() {
        let config = KimiConfig {
            executable: "kimi".into(),
            cwd: "/tmp/kimi".into(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/socket".into(),
            resume_session: None,
        };
        let mut panel = ConversationPanel::new_kimi(config);
        panel.thread = Some("session".into());
        panel.ready = true;
        panel.kimi.as_mut().unwrap().confirmed_session = Some("session".into());
        let prepare = |panel: &mut ConversationPanel, id: &str, text: &str| {
            panel.prompt = text.into();
            panel.context.draft.attachments = vec![crate::conversation::PromptAttachment {
                name: "source".into(),
                text: "snapshot β".into(),
            }];
            panel.kimi.as_mut().unwrap().submission = Some(Submission {
                session: "session".into(),
                id: id.into(),
                text: text.into(),
                attachments: panel.context.draft.attachments.clone(),
            });
            panel.messages.push(ConversationMessage {
                role: "You",
                key: Some(("session".into(), id.into(), "user".into())),
                text: text.into(),
                command: None,
                activity: None,
            });
            panel.starting = true;
        };
        for (id, text) in [("one", "first"), ("two", "second")] {
            prepare(&mut panel, id, text);
            panel.active_turn = Some(id.into());
            panel.kimi_turn_finished("session".into(), id.into(), id.into(), "end_turn".into());
            assert!(panel.prompt.is_empty());
        }
        prepare(&mut panel, "three", "unconfirmed");
        panel.kimi_turn_failed(
            "session".into(),
            "".into(),
            "stale".into(),
            "ignored".into(),
        );
        assert!(panel.starting);
        panel.kimi_turn_failed(
            "session".into(),
            "".into(),
            "three".into(),
            "rejected before turn".into(),
        );
        assert!(!panel.starting);
        assert_eq!(panel.prompt, "unconfirmed");
        assert_eq!(panel.context.draft.attachments[0].text, "snapshot β");
        assert_eq!(
            panel
                .messages
                .iter()
                .map(|m| m.text.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        prepare(&mut panel, "four", "cancelled draft");
        panel.fresh_thread = true;
        panel.active_turn = Some("turn-four".into());
        panel.kimi_turn_finished(
            "session".into(),
            "turn-four".into(),
            "four".into(),
            "cancelled".into(),
        );
        assert_eq!(panel.prompt, "cancelled draft");
        assert!(!panel.fresh_thread);
        assert_eq!(panel.context.draft.attachments[0].text, "snapshot β");
        assert_eq!(
            panel
                .messages
                .iter()
                .map(|m| m.text.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        panel.kimi.as_mut().unwrap().confirmed_session = None;
        panel.pending_thread = Some(Some("session".into()));
        panel.kimi_open_failed("reload failed".into());
        assert!(!panel.ready);
        assert_eq!(panel.prompt, "cancelled draft");
        panel.kimi.as_mut().unwrap().confirmed_session = Some("session".into());
        panel.kimi_open_failed("other session failed".into());
        assert!(panel.ready);
    }
    #[test]
    fn reconnect_reopens_fresh_session_and_preserves_its_draft() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};
        let directory =
            std::env::temp_dir().join(format!("tw-kimi-reopen-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let script = directory.join("fake-kimi");
        std::fs::write(&script, r##"#!/usr/bin/env python3
import json, os, sys
for line in sys.stdin:
    frame = json.loads(line)
    method = frame.get('method')
    if method == 'initialize':
        result = {'protocolVersion':1,'agentInfo':{'name':'Kimi Code CLI','version':'2.2.0'},'agentCapabilities':{'loadSession':True,'sessionCapabilities':{'list':{}}}}
    elif method == 'session/list':
        result = {'sessions':[{'sessionId':'same','cwd':os.getcwd(),'title':'Existing'}]}
    elif method == 'session/load':
        open('loaded', 'w').close()
        notification = {'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'same','update':{'sessionUpdate':'user_message_chunk','content':{'type':'text','text':'earlier'}}}}
        print(json.dumps(notification), flush=True)
        result = {'configOptions':[]}
    else:
        result = {}
    if 'id' in frame:
        print(json.dumps({'jsonrpc':'2.0','id':frame['id'],'result':result}), flush=True)
"##).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = KimiConfig {
            executable: script,
            cwd: directory.clone(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/socket".into(),
            resume_session: Some("same".into()),
        };
        let mut panel = ConversationPanel::new_kimi(config);
        panel.thread = Some("same".into());
        panel.fresh_thread = true;
        panel.prompt = "retained prompt".into();
        panel
            .context
            .draft
            .attachments
            .push(crate::conversation::PromptAttachment {
                name: "source".into(),
                text: "retained snapshot β".into(),
            });
        let ctx = egui::Context::default();
        panel.connect_kimi(&ctx);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !panel.ready {
            panel.poll_kimi(&ctx);
            if let Some(error) = &panel.error {
                panic!("reopen failed: {error}");
            }
            assert!(
                Instant::now() < deadline,
                "current session was not reopened"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(directory.join("loaded").is_file());
        assert_eq!(panel.thread.as_deref(), Some("same"));
        assert_eq!(panel.prompt, "retained prompt");
        assert_eq!(
            panel.context.draft.attachments[0].text,
            "retained snapshot β"
        );
        assert_eq!(panel.messages[0].text, "earlier");
        assert!(!panel.fresh_thread);
        panel.kimi.as_mut().unwrap().adapter.take();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
