//! Plain client-side conversation observations and drafts; no widget state or ECS ownership.
use crate::conversation::{Adapter, Event, Request};
use crate::conversation::{
    ConnectionInfo, ContextFile, GoalInfo, PlanStep, PromptAttachment, SkillInfo,
};
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct DraftContext {
    pub attachments: Vec<PromptAttachment>,
    pub skills: Vec<SkillInfo>,
}
impl DraftContext {
    pub fn bytes(&self) -> usize {
        self.attachments
            .iter()
            .map(|a| a.name.len() + a.text.len())
            .sum::<usize>()
            + self
                .skills
                .iter()
                .map(|s| s.name.len() + s.path.len() + s.description.len())
                .sum::<usize>()
    }
    pub fn is_empty(&self) -> bool {
        self.attachments.is_empty() && self.skills.is_empty()
    }
}

#[derive(Clone, Default)]
pub(crate) struct ContextData {
    pub goal: Option<GoalInfo>,
    pub goal_loaded: bool,
    pub plan: Vec<PlanStep>,
    pub plan_text: Option<String>,
    pub files: Vec<ContextFile>,
    pub skills: Vec<SkillInfo>,
    pub connections: Vec<ConnectionInfo>,
    pub capabilities_loaded: bool,
    pub draft: DraftContext,
    pub(crate) cached: HashMap<String, DraftContext>,
    pub(crate) generation: u64,
    pub(crate) attachment_pending: bool,
    pub(crate) file_job_pending: bool,
    pub(crate) attachment_job: Option<uuid::Uuid>,
}
impl ContextData {
    pub fn has_draft(&self) -> bool {
        !self.draft.is_empty() || self.attachment_pending
    }
    pub fn cached_draft(&self, id: &str) -> bool {
        self.cached.get(id).is_some_and(|d| !d.is_empty())
    }
    pub fn any_drafts(&self) -> bool {
        self.has_draft() || self.cached.values().any(|d| !d.is_empty())
    }
    pub fn bytes(&self) -> usize {
        self.draft.bytes() + self.cached.values().map(DraftContext::bytes).sum::<usize>()
    }
    pub fn total_input_bytes(&self) -> usize {
        self.draft
            .attachments
            .iter()
            .map(|a| a.name.len() + a.text.len() + 64)
            .sum::<usize>()
            + self
                .draft
                .skills
                .iter()
                .map(|s| s.name.len() + s.path.len() + s.description.len() + 64)
                .sum::<usize>()
    }
    pub fn skills_valid(&self) -> bool {
        self.draft.skills.iter().all(|selected| {
            self.skills
                .iter()
                .any(|s| s.enabled && s.path == selected.path && s.name == selected.name)
        })
    }
    pub fn switch(&mut self, old: &str, new: &str, carry: bool) {
        self.generation = self.generation.wrapping_add(1);
        if !carry {
            let draft = std::mem::take(&mut self.draft);
            if !draft.is_empty() {
                self.cached.insert(old.into(), draft);
            } else {
                self.cached.remove(old);
            }
            self.draft = self.cached.remove(new).unwrap_or_default();
        }
        self.goal = None;
        self.goal_loaded = false;
        self.plan.clear();
        self.plan_text = None;
        self.files.clear();
    }
    pub fn retire(&mut self, id: &str, current: bool, cancel_preview: bool) {
        if !self.cached_draft(id) {
            self.cached.remove(id);
        }
        if current {
            if cancel_preview {
                self.generation = self.generation.wrapping_add(1);
            }
            self.goal = None;
            self.goal_loaded = false;
            self.plan.clear();
            self.plan_text = None;
            self.files.clear();
        }
    }
    pub fn merge_files(&mut self, files: Vec<ContextFile>) {
        for file in files {
            if let Some(old) = self.files.iter_mut().find(|f| f.path == file.path) {
                *old = file;
            } else if self.files.len() < 128 {
                self.files.push(file);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelKey {
    pub provider: crate::model_catalog::ProviderKey,
    pub model: String,
    /// Kimi's actual config option ID; Codex uses no option ID.
    pub option_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelPurpose {
    Fresh,
    InPlace,
    NewWithTransfer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelPhase {
    Waiting,
    Setting,
    Failed,
}
#[derive(Clone, Debug)]
pub(crate) struct ModelIntent {
    pub id: uuid::Uuid,
    pub chat_id: uuid::Uuid,
    pub key: ModelKey,
    pub generation: u64,
    pub purpose: ModelPurpose,
    pub phase: ModelPhase,
    pub baseline_modes: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Freshness {
    NeverObserved,
    Attached,
    /// A transport exists, but it has not confirmed current remote activity.
    Unconfirmed,
    Disconnected,
}

/// Provider client and locally observed conversation state. Presentation data
/// and widget resources remain in the UI view.
#[derive(bevy_ecs::component::Component)]
pub(crate) struct ConversationRecord {
    pub chat_id: uuid::Uuid,
    pub provider: crate::model_catalog::ProviderKey,
    pub draft_revision: u64,
    pub model_intent: Option<ModelIntent>,
    pub confirmed_model: Option<ModelKey>,
    pub permission_review_required: bool,
    pub thread_model_seen: bool,
    pub client_attached: bool,
    pub client_generation: u64,
    pub effects: VecDeque<Request>,
    pub observation: Freshness,
    pub observed_activity: crate::conversation::ThreadWorkState,
    #[cfg(feature = "kimi")]
    pub kimi: Option<KimiState>,
    #[cfg(feature = "kimi")]
    pub active_kimi: bool,
    pub vault_permission_requested: Option<bool>,
    pub vault_write_enabled: Option<bool>,
    pub research_requested: bool,
    pub collect_references: Option<(String, Option<String>)>,
    pub relevant_sources: bool,
    pub research_workspace: Option<uuid::Uuid>,
    #[cfg(feature = "codex")]
    pub config: crate::codex::CodexConfig,
    pub project_ready: bool,
    pub fresh_thread: bool,
    pub resume_requested: bool,
    pub pending_thread: Option<Option<String>>,
    #[cfg(feature = "codex")]
    pub pending_thread_action: Option<String>,
    pub binding_durable: bool,
    pub persist_project: bool,
    #[cfg(feature = "codex")]
    pub project_expected: Option<crate::notes::CodexProjectBinding>,
    pub metadata_error: Option<String>,
    pub threads: Vec<crate::conversation::ThreadSummary>,
    /// IDs actually listed by the current connection, excluding retained cache rows.
    pub listed_threads: std::collections::HashSet<String>,
    pub threads_cursor: Option<String>,
    pub threads_loading: bool,
    pub threads_append: bool,
    pub prompt_drafts: HashMap<String, String>,
    pub unavailable_threads: std::collections::HashSet<String>,
    pub history_cursor: Option<String>,
    pub history_loading: bool,
    pub history_reloading: bool,
    pub history_incomplete: bool,
    pub project_name: String,
    #[cfg(feature = "codex")]
    pub models: Vec<crate::codex::ModelChoice>,
    #[cfg(feature = "codex")]
    pub models_cursor: Option<String>,
    #[cfg(feature = "codex")]
    pub models_loading: bool,
    #[cfg(feature = "codex")]
    pub selected_model: Option<String>,
    #[cfg(feature = "codex")]
    pub selected_effort: Option<String>,
    #[cfg(feature = "codex")]
    pub model_override: bool,
    #[cfg(feature = "codex")]
    pub controls: crate::codex::TurnControls,
    pub context_data: ContextData,
    pub context_goal_pending: Option<String>,
    pub context_submit_pending: bool,
    #[cfg(feature = "codex")]
    pub codex_submission: Option<CodexSubmission>,
    pub ready: bool,
    pub starting: bool,
    pub active_turn: Option<String>,
    pub thread: Option<String>,
    pub disconnecting: bool,
    pub prompt: String,
    pub messages: Vec<ConversationMessage>,
    #[cfg(feature = "codex")]
    pub approvals: Vec<ConversationApproval>,
    pub status: String,
    pub error: Option<String>,
}

#[cfg(feature = "codex")]
use crate::codex::{ApprovalDecision, CodexEvent, CodexRequest, ThreadAction, WorkMode};
use crate::conversation::ThreadWorkState;
#[cfg(feature = "kimi")]
use crate::kimi::{ConfigOption, KimiConfig, KimiEvent, KimiRequest, PermissionOption};

#[derive(Clone, Copy)]
enum HistoryMerge {
    Replace,
    Reconcile,
    Prepend,
}

#[derive(Clone)]
pub(crate) struct ConversationMessage {
    /// A cached local observation not confirmed by the current replay.
    pub(crate) uncertain: bool,
    pub(crate) role: &'static str,
    pub(crate) key: Option<(String, String, String)>,
    pub(crate) text: String,
    pub(crate) command: Option<crate::conversation::CommandExecution>,
    pub(crate) activity: Option<crate::conversation::Activity>,
}
#[cfg(feature = "codex")]
pub(crate) struct ConversationApproval {
    pub(crate) answering: bool,
    pub(crate) id: serde_json::Value,
    pub(crate) thread: String,
    pub(crate) turn: String,
    pub(crate) method: String,
    pub(crate) details: String,
    pub(crate) decisions: Vec<ApprovalDecision>,
}

#[cfg(feature = "codex")]
pub(crate) struct CodexSubmission {
    pub id: uuid::Uuid,
    pub thread: String,
    pub text: String,
    pub attachments: Vec<PromptAttachment>,
    pub skills: Vec<SkillInfo>,
    pub draft_revision: u64,
}

#[cfg(feature = "kimi")]
pub(crate) struct Permission {
    pub(crate) id: serde_json::Value,
    pub(crate) details: String,
    pub(crate) options: Vec<PermissionOption>,
    pub(crate) answering: bool,
}
#[cfg(feature = "kimi")]
pub(crate) struct Submission {
    pub(crate) session: String,
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) attachments: Vec<crate::conversation::PromptAttachment>,
}

#[cfg(feature = "kimi")]
pub(crate) struct KimiState {
    pub(crate) config: KimiConfig,
    pub(crate) option_pending: bool,
    pub(crate) permissions: Vec<Permission>,
    pub(crate) options: Vec<ConfigOption>,
    pub(crate) known: std::collections::HashSet<String>,
    pub(crate) submission: Option<Submission>,
    pub(crate) confirmed_session: Option<String>,
    pub(crate) cancelling: bool,
    pub(crate) generation: u64,
    pub(crate) resume: Option<String>,
}
#[cfg(feature = "kimi")]
impl KimiState {
    pub(crate) fn new(config: KimiConfig, _codex_available: bool) -> Self {
        let resume = config.resume_session.clone();
        Self {
            config,
            option_pending: false,
            permissions: Vec::new(),
            options: Vec::new(),
            known: Default::default(),
            submission: None,
            confirmed_session: None,
            cancelling: false,
            generation: 0,
            resume,
        }
    }
    pub(crate) fn permission_bytes(&self) -> usize {
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

impl ConversationRecord {
    const BYTE_LIMIT: usize = 2 * 1024 * 1024;
    #[cfg(feature = "codex")]
    pub(crate) fn accept_turn(&mut self, thread: &str) {
        if self.thread.as_deref() == Some(thread) && self.context_submit_pending {
            if let Some(submission) = self.codex_submission.take() {
                if submission.thread == thread
                    && submission.draft_revision == self.draft_revision
                    && self.prompt == submission.text
                    && self.context_data.draft.attachments == submission.attachments
                    && self.context_data.draft.skills == submission.skills
                {
                    self.prompt.clear();
                    self.context_data.draft = Default::default();
                    self.draft_revision = self.draft_revision.saturating_add(1);
                }
            }
            self.context_submit_pending = false;
        }
    }
    #[cfg(feature = "codex")]
    pub(crate) fn start_turn(&mut self, thread: String, turn: String) {
        // A goal can start a turn without accepting our queued message/context.
        if self.thread.as_ref() != Some(&thread) {
            return;
        }
        self.fresh_thread = false;
        self.starting = false;
        self.active_turn = Some(turn);
        self.status = "Streaming…".into();
    }
    #[cfg(feature = "codex")]
    pub(crate) fn send_request(&mut self, request: CodexRequest) -> bool {
        if !self.client_attached {
            self.error = Some("Connect to Codex first".into());
            return false;
        }
        if self.effects.len() >= 16 {
            self.error = Some("Conversation request queue is full; try again.".into());
            return false;
        }
        self.effects.push_back(Request::Codex(request));
        true
    }
    #[cfg(feature = "codex")]
    pub(crate) fn refresh_threads(&mut self, cursor: Option<String>) {
        if self.threads_loading {
            return;
        }
        self.threads_append = cursor.is_some();
        self.threads_loading = self.send_request(CodexRequest::ListThreads { cursor });
    }
    #[cfg(feature = "codex")]
    pub(crate) fn begin_thread_open(&mut self, id: Option<String>) {
        // The old conversation owns its draft until the server confirms a new one.
        self.pending_thread = Some(id);
        self.resume_requested = false;
        self.ready = false;
        self.starting = true;
        self.error = None;
        self.status = "Opening conversation…".into();
    }
    #[cfg(feature = "codex")]
    pub(crate) fn commit_thread_open(&mut self, thread: String) {
        if let Some(target) = self.pending_thread.take() {
            let same = target.as_ref() == Some(&thread) && self.thread.as_ref() == Some(&thread);
            self.history_reloading = same;
            if same {
                self.fresh_thread = false;
            } else {
                let carry_prompt = (self.fresh_thread || self.thread.is_none()) && target.is_none();
                self.context_data.switch(
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
                self.history_incomplete = false;
                self.history_cursor = None;
            }
        }
        self.thread = Some(thread.clone());
        self.config.resume_thread = Some(thread);
        self.ready = true;
        self.starting = false;
        self.status = if self.history_reloading && self.observation != Freshness::Attached {
            "Connected · activity unknown".into()
        } else {
            "Ready".into()
        };
    }
    #[cfg(feature = "codex")]
    pub(crate) fn accept_goal(&mut self, goal: Option<crate::codex::GoalInfo>) {
        if goal.as_ref().is_some_and(|g| g.status == "active") {
            self.controls.mode = WorkMode::Goal;
        } else if self.controls.mode == WorkMode::Goal {
            self.controls.mode = WorkMode::Work;
        }
        self.context_data.goal = goal;
        self.context_data.goal_loaded = true;
    }
    #[cfg(feature = "codex")]
    pub(crate) fn mode_consistent(&self) -> bool {
        let active = self
            .context_data
            .goal
            .as_ref()
            .is_some_and(|g| g.status == "active");
        self.context_data.goal_loaded && (active == (self.controls.mode == WorkMode::Goal))
    }
    #[cfg(feature = "codex")]
    pub(crate) fn thread_state(
        &self,
        thread: &crate::conversation::ThreadSummary,
    ) -> ThreadWorkState {
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
    pub(crate) fn busy(&self) -> bool {
        self.starting
            || self.active_turn.is_some()
            || self.has_pending_provider_operation()
            || self.context_data.file_job_pending
            || self.context_goal_pending.is_some()
            || self.context_submit_pending
    }
    fn message_bytes(message: &ConversationMessage) -> usize {
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
    }
    pub(crate) fn bytes(&self) -> usize {
        self.messages.iter().map(Self::message_bytes).sum::<usize>()
            + self.prompt_drafts.values().map(String::len).sum::<usize>()
            + self.context_data.bytes()
            + self.approval_bytes()
    }
    /// Stage replay before replacing displayed observations. Matched occurrences
    /// are consumed once and in order; retained unmatched observations remain
    /// explicitly unconfirmed rather than being silently dropped.
    fn install_history(
        &mut self,
        thread: &str,
        provider: &'static str,
        history: Vec<crate::conversation::HistoryMessage>,
        mode: HistoryMerge,
    ) -> bool {
        if matches!(mode, HistoryMerge::Reconcile) && history.is_empty() {
            for message in &mut self.messages {
                message.uncertain = true;
            }
            self.history_incomplete = true;
            return true;
        }
        let incoming_bytes: usize = history
            .iter()
            .map(|m| {
                m.text.len()
                    + m.message_id.as_ref().map_or(0, String::len)
                    + m.turn_id.as_ref().map_or(0, String::len)
            })
            .sum();
        if history.len() > 256 || incoming_bytes > Self::BYTE_LIMIT {
            for message in &mut self.messages {
                message.uncertain = true;
            }
            self.history_incomplete = true;
            self.error = Some("History replay exceeds the display limit. Cached observations and draft are retained.".into());
            return false;
        }
        let replay: Vec<_> = history
            .into_iter()
            .map(|message| ConversationMessage {
                role: if matches!(message.role.as_str(), "user" | "You") {
                    "You"
                } else {
                    provider
                },
                key: message
                    .message_id
                    .map(|id| (thread.to_owned(), message.turn_id.unwrap_or_default(), id)),
                text: message.text,
                command: None,
                activity: None,
                uncertain: false,
            })
            .collect();
        let staged = match mode {
            HistoryMerge::Replace => replay,
            HistoryMerge::Prepend => {
                let mut staged = replay;
                staged.extend(self.messages.iter().cloned());
                staged
            }
            HistoryMerge::Reconcile => {
                fn stable_id(message: &ConversationMessage) -> Option<(&str, &str, &str)> {
                    let (thread, turn, item) = message.key.as_ref()?;
                    if matches!(item.as_str(), "user" | "pending-user")
                        || item.starts_with("chunk:")
                    {
                        return None;
                    }
                    Some((thread, turn, item))
                }
                fn score(
                    thread: &str,
                    old: &ConversationMessage,
                    new: &ConversationMessage,
                ) -> usize {
                    if old
                        .key
                        .as_ref()
                        .is_some_and(|(old_thread, _, _)| old_thread != thread)
                        || old.role != new.role
                        || old.command.is_some()
                        || old.activity.is_some()
                    {
                        return 0;
                    }
                    let same_or_more = old.text == new.text
                        || (old.role != "You"
                            && !old.text.is_empty()
                            && new.text.starts_with(&old.text));
                    if !same_or_more {
                        return 0;
                    }
                    match (stable_id(old), stable_id(new)) {
                        (
                            Some((old_thread, old_turn, old_id)),
                            Some((new_thread, new_turn, new_id)),
                        ) => {
                            if old_thread == new_thread
                                && old_id == new_id
                                && (old_turn.is_empty()
                                    || new_turn.is_empty()
                                    || old_turn == new_turn)
                            {
                                4
                            } else {
                                0
                            }
                        }
                        _ if old.text == new.text => 2,
                        _ => 1,
                    }
                }
                let old = &self.messages;
                let columns = replay.len() + 1;
                let mut best = vec![0usize; (old.len() + 1) * columns];
                for i in (0..old.len()).rev() {
                    for j in (0..replay.len()).rev() {
                        let pair = score(thread, &old[i], &replay[j]);
                        best[i * columns + j] = best[(i + 1) * columns + j]
                            .max(best[i * columns + j + 1])
                            .max(if pair > 0 {
                                pair + best[(i + 1) * columns + j + 1]
                            } else {
                                0
                            });
                    }
                }
                let mut staged = Vec::with_capacity(old.len().max(replay.len()));
                let (mut i, mut j) = (0, 0);
                while i < old.len() && j < replay.len() {
                    let pair = score(thread, &old[i], &replay[j]);
                    if pair > 0 && best[i * columns + j] == pair + best[(i + 1) * columns + j + 1] {
                        staged.push(replay[j].clone());
                        i += 1;
                        j += 1;
                    } else if best[(i + 1) * columns + j] >= best[i * columns + j + 1] {
                        let mut retained = old[i].clone();
                        retained.uncertain = true;
                        staged.push(retained);
                        i += 1;
                    } else {
                        staged.push(replay[j].clone());
                        j += 1;
                    }
                }
                while i < old.len() {
                    let mut retained = old[i].clone();
                    retained.uncertain = true;
                    staged.push(retained);
                    i += 1;
                }
                staged.extend(replay[j..].iter().cloned());
                staged
            }
        };
        let current_bytes: usize = self.messages.iter().map(Self::message_bytes).sum();
        let final_bytes: usize = staged.iter().map(Self::message_bytes).sum();
        if staged.len() > 256
            || self
                .bytes()
                .saturating_sub(current_bytes)
                .saturating_add(final_bytes)
                > Self::BYTE_LIMIT
        {
            for message in &mut self.messages {
                message.uncertain = true;
            }
            self.history_incomplete = true;
            self.error = Some("History replay exceeds the display limit. Cached observations and draft are retained.".into());
            return false;
        }
        if !matches!(mode, HistoryMerge::Prepend) {
            self.history_incomplete = staged.iter().any(|message| message.uncertain);
        }
        self.messages = staged;
        true
    }
    /// Retire requests and tokens tied to the old transport without changing
    /// the remote identity, draft, cached messages or last observed activity.
    pub(crate) fn prepare_connection(&mut self) {
        if self.client_generation > 0 && self.thread.is_some() {
            for message in &mut self.messages {
                message.uncertain = true;
            }
            self.history_incomplete = true;
        }
        self.effects.clear();
        self.starting = false;
        self.active_turn = None;
        self.pending_thread = None;
        self.context_goal_pending = None;
        self.context_submit_pending = false;
        self.threads_loading = false;
        self.history_loading = false;
        self.history_reloading = false;
        self.listed_threads.clear();
        self.ready = false;
        self.project_ready = false;
        self.confirmed_model = None;
        self.thread_model_seen = false;
        #[cfg(feature = "codex")]
        if !self.is_kimi() {
            self.approvals.clear();
            self.codex_submission = None;
            self.pending_thread_action = None;
            self.models_loading = false;
            self.project_expected = self.config.project.clone();
            if let Some(thread) = self.thread.clone() {
                self.config.resume_thread = Some(thread);
            }
            self.resume_requested = self.config.resume_thread.is_some();
        }
        #[cfg(feature = "kimi")]
        if let Some(state) = &mut self.kimi {
            state.permissions.clear();
            state.submission = None;
            state.option_pending = false;
            state.cancelling = false;
            state.confirmed_session = None;
            state.options.clear();
            state.known.clear();
            if let Some(thread) = self.thread.clone() {
                state.config.resume_session = Some(thread);
            }
            state.resume = state.config.resume_session.clone();
        }
    }
    pub(crate) fn disconnect(&mut self) {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            self.disconnect_kimi();
            return;
        }
        #[cfg(feature = "codex")]
        if !self.disconnecting && self.send_request(CodexRequest::Disconnect) {
            self.disconnecting = true;
            self.status = "Disconnecting…".into();
        }
    }
    pub(crate) fn limit_exceeded(&mut self) {
        self.error=Some("Conversation display limit reached; additional output is not displayed. The connection remains active.".into());
    }
    pub(crate) fn message(&mut self, key: (String, String, String), text: String, replace: bool) {
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
            self.messages[index].uncertain = false;
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
                uncertain: false,
            });
        }
    }
    pub(crate) fn command_card(
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
    pub(crate) fn activity_card(
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
    pub(crate) fn retire_activity(
        &mut self,
        thread: Option<&str>,
        turn: Option<&str>,
        status: &str,
    ) {
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
    pub(crate) fn is_kimi(&self) -> bool {
        #[cfg(feature = "kimi")]
        {
            return self.active_kimi;
        }
        #[cfg(not(feature = "kimi"))]
        false
    }
    pub(crate) fn provider_name(&self) -> &'static str {
        #[cfg(feature = "kimi")]
        if self.active_kimi {
            return "Kimi";
        }
        "Codex"
    }
    pub(crate) fn provider_cwd(&self) -> std::path::PathBuf {
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
    pub(crate) fn has_adapter(&self) -> bool {
        self.client_attached
    }
    pub(crate) fn has_pending_provider_operation(&self) -> bool {
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
    pub(crate) fn approval_count(&self) -> usize {
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
    pub(crate) fn approval_bytes(&self) -> usize {
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
    #[cfg(feature = "codex")]
    pub(crate) fn reduce_codex(&mut self, event: CodexEvent) {
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
                    if let Some(submission) = self.codex_submission.take() {
                        self.messages.retain(|m| {
                            m.key.as_ref().is_none_or(|(_, turn, item)| {
                                turn != &submission.id.to_string() || item != "pending-user"
                            })
                        });
                    }
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
                        self.context_data.goal = goal;
                        self.context_data.goal_loaded = true;
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
                        self.context_data.plan = steps;
                    }
                    if text.is_some() {
                        self.context_data.plan_text = text;
                    }
                }
            }
            CodexEvent::Files { thread, files } => {
                if self.thread.as_ref() == Some(&thread) {
                    self.context_data.merge_files(files);
                }
            }
            CodexEvent::Skills(skills) => {
                self.context_data.skills = skills;
                self.context_data.capabilities_loaded = true;
            }
            CodexEvent::Connections(connections) => {
                self.context_data.connections = connections;
                self.context_data.capabilities_loaded = true;
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
                    self.thread_model_seen = true;
                    if self.model_intent.is_none() {
                        self.confirmed_model = Some(ModelKey {
                            provider: self.provider.clone(),
                            model: model.clone(),
                            option_id: None,
                        });
                    }
                    self.selected_model = Some(model);
                    self.selected_effort = effort;
                    self.model_override = false;
                }
            }
            CodexEvent::ProjectReady { project_id, name } => {
                if self.project_expected.is_none() {
                    self.project_expected = self.config.project.clone();
                }
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
                    self.listed_threads.clear();
                    self.threads.retain(|t| {
                        (self.fresh_thread && Some(&t.id) == self.thread.as_ref())
                            || (self.unavailable_threads.contains(&t.id)
                                && ((Some(&t.id) == self.thread.as_ref()
                                    && (!self.prompt.is_empty() || self.context_data.has_draft()))
                                    || self.context_data.cached_draft(&t.id)
                                    || self
                                        .prompt_drafts
                                        .get(&t.id)
                                        .is_some_and(|p| !p.is_empty())))
                    });
                }
                self.unavailable_threads
                    .retain(|id| self.threads.iter().any(|t| &t.id == id));
                for thread in threads {
                    self.listed_threads.insert(thread.id.clone());
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
                    let reloading = self.history_reloading;
                    let empty = messages.is_empty();
                    let mode = if reloading {
                        HistoryMerge::Reconcile
                    } else {
                        HistoryMerge::Prepend
                    };
                    if self.install_history(&thread, "Codex", messages, mode) {
                        self.history_cursor = next_cursor;
                        self.history_incomplete = (reloading && empty)
                            || self.history_cursor.is_some()
                            || self.messages.iter().any(|message| message.uncertain);
                        if reloading && empty {
                            self.status = "History replay unavailable · cached observations may be incomplete".into();
                        }
                    }
                    self.history_reloading = false;
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
                self.context_data.goal_loaded = false;
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
            } => {
                if self.thread.as_ref() == Some(&thread) && self.active_turn.as_ref() == Some(&turn)
                {
                    self.message((thread, turn, item), text, false);
                }
            }
            CodexEvent::MessageComplete {
                thread,
                turn,
                item,
                text,
            } => {
                if self.thread.as_ref() == Some(&thread) && self.active_turn.as_ref() == Some(&turn)
                {
                    self.message((thread, turn, item), text, true);
                }
            }
            CodexEvent::TurnCompleted {
                thread,
                turn,
                status,
            } => {
                self.retire_activity(Some(&thread), Some(&turn), "ended");
                if self.thread.as_ref() == Some(&thread) && self.active_turn.as_ref() == Some(&turn)
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
                if self.thread.as_ref() != Some(&thread) || self.active_turn.as_ref() != Some(&turn)
                {
                    return;
                }
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
                        answering: false,
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
                if self.history_loading {
                    for message in &mut self.messages {
                        message.uncertain = true;
                    }
                    self.history_incomplete = true;
                }
                self.models_loading = false;
                self.error = Some(error);
                self.threads_loading = false;
                self.history_loading = false;
                if self.active_turn.is_none() && self.pending_thread.is_none() {
                    self.starting = false;
                }
            }
            CodexEvent::Disconnected(error) => {
                self.context_data.goal_loaded = false;
                // The worker reports this only after its child has been killed/reaped.
                self.client_attached = false;
                self.context_submit_pending = false;
                self.pending_thread = None;
                self.pending_thread_action = None;
                self.ready = false;
                self.project_ready = false;
                self.models_loading = false;
                self.models.clear();
                self.models_cursor = None;
                self.context_data.skills.clear();
                self.context_data.connections.clear();
                self.context_data.capabilities_loaded = false;
                self.context_goal_pending = None;
                self.threads_loading = false;
                self.history_loading = false;
                self.starting = false;
                self.disconnecting = false;
                self.observation = crate::conversation_runtime::Freshness::Disconnected;
                self.status = "Disconnected · last remote state unknown".into();
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

    #[cfg(feature = "codex")]
    pub(crate) fn finish_thread_action(&mut self, id: &str, action: &ThreadAction) {
        if self.pending_thread_action.as_deref() == Some(id) {
            self.pending_thread_action = None;
        }
        match action {
            ThreadAction::Rename(name) => {
                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == id) {
                    thread.title = name.clone();
                }
            }
            ThreadAction::Archive | ThreadAction::Remove => {
                let current = self.thread.as_deref() == Some(id);
                let has_draft = (current
                    && (!self.prompt.is_empty() || self.context_data.has_draft()))
                    || self.context_data.cached_draft(id)
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
                self.context_data.retire(id, current, false);
                if current {
                    if !has_draft {
                        self.thread = None;
                        self.messages.clear();
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

    #[cfg(feature = "codex")]
    pub(crate) fn state_label(state: ThreadWorkState) -> &'static str {
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
}

#[cfg(feature = "kimi")]
impl ConversationRecord {
    pub(crate) fn send_kimi(&mut self, request: KimiRequest) -> bool {
        if !self.client_attached {
            self.error = Some("Connect to Kimi first".into());
            return false;
        }
        if self.effects.len() >= 16 {
            self.error = Some("Conversation request queue is full; try again.".into());
            return false;
        }
        self.effects.push_back(Request::Kimi(request));
        true
    }
    pub(crate) fn disconnect_kimi(&mut self) {
        if self.disconnecting {
            return;
        }
        if self.send_kimi(KimiRequest::Disconnect) {
            self.disconnecting = true;
            self.status = "Disconnecting…".into();
        }
    }
    pub(crate) fn select_kimi(&mut self, id: Option<String>) {
        let resuming_same = id.is_some() && id == self.thread && !self.ready;
        if (self.busy() && !resuming_same)
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
            && (!self.prompt.trim().is_empty() || self.context_data.has_draft())
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
    pub(crate) fn apply_kimi_open(
        &mut self,
        session: String,
        options: Vec<ConfigOption>,
        history: Vec<crate::conversation::HistoryMessage>,
    ) {
        let same = self.thread.as_ref() == Some(&session);
        if let Some(target) = self.pending_thread.take() {
            if same {
                self.fresh_thread = false;
            } else {
                let carry = (self.fresh_thread || self.thread.is_none()) && target.is_none();
                self.context_data.switch(
                    self.thread.as_deref().unwrap_or_default(),
                    &session,
                    carry,
                );
                if !carry {
                    self.prompt_drafts.insert(
                        self.thread.clone().unwrap_or_default(),
                        std::mem::take(&mut self.prompt),
                    );
                    self.prompt = self.prompt_drafts.remove(&session).unwrap_or_default();
                }
                self.fresh_thread = target.is_none();
            }
        }
        let empty = history.is_empty();
        let mode = if same {
            HistoryMerge::Reconcile
        } else {
            HistoryMerge::Replace
        };
        let installed = self.install_history(&session, "Kimi", history, mode);
        self.history_cursor = None;
        self.thread = Some(session.clone());
        self.ready = true;
        self.starting = false;
        self.status = if !installed {
            "Connected · history replay exceeds display limit".into()
        } else if same && empty {
            "History replay unavailable · cached observations may be incomplete".into()
        } else if same && self.observation != Freshness::Attached {
            "Connected · activity unknown".into()
        } else {
            "Ready".into()
        };
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
    pub(crate) fn kimi_open_failed(&mut self, error: String) {
        for message in &mut self.messages {
            message.uncertain = true;
        }
        self.history_incomplete = true;
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
    pub(crate) fn kimi_submission_matches(&self, session: &str, id: &str) -> bool {
        self.thread.as_deref() == Some(session)
            && self
                .kimi
                .as_ref()
                .and_then(|k| k.submission.as_ref())
                .is_some_and(|s| s.session == session && s.id == id)
    }
    pub(crate) fn remove_kimi_submission_row(&mut self, session: &str, id: &str) {
        self.messages.retain(|m| {
            m.key.as_ref().is_none_or(|(thread, turn, item)| {
                !(thread == session && turn == id && item == "user")
            })
        });
    }
    pub(crate) fn kimi_turn_finished(
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
            "Cancelled · draft retained".into()
        } else if status == "end_turn" {
            "Completed".into()
        } else {
            format!("Turn {}", status.replace('_', " "))
        };
        if let Some(k) = &mut self.kimi {
            k.permissions.clear();
            k.cancelling = false;
            if let Some(shadow) = k.submission.take() {
                if !cancelled
                    && self.prompt == shadow.text
                    && self
                        .context_data
                        .draft
                        .attachments
                        .iter()
                        .map(|a| (&a.name, &a.text))
                        .eq(shadow.attachments.iter().map(|a| (&a.name, &a.text)))
                {
                    self.prompt.clear();
                    self.context_data.draft.attachments.clear();
                }
            }
        }
        if cancelled {
            self.remove_kimi_submission_row(&session, &submission);
        }
    }
    pub(crate) fn kimi_turn_failed(
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
    pub(crate) fn cancel_kimi(&mut self) {
        if self.kimi.as_ref().is_some_and(|k| k.cancelling) {
            return;
        }
        if self.send_kimi(KimiRequest::Cancel) {
            if let Some(k) = &mut self.kimi {
                k.cancelling = true;
            }
            self.status = "Stopping…".into();
        }
    }
    pub(crate) fn reduce_kimi(&mut self, event: KimiEvent) {
        match event {
            KimiEvent::Connecting => self.status = "Connecting…".into(),
            KimiEvent::Connected { .. } => {
                self.project_ready = true;
                self.binding_durable = true;
                self.status = "Connected".into();
                self.threads_loading = self.send_kimi(KimiRequest::ListSessions { cursor: None });
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
                    if self.kimi.as_ref().is_some_and(|k| k.known.contains(&id)) {
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
                        self.error =
                            Some("Requested Kimi session is not listed in this workspace.".into());
                    }
                }
            }
            KimiEvent::Opened {
                session,
                options,
                history,
            } => {
                if self.model_intent.is_none() {
                    self.confirmed_model = options
                        .iter()
                        .find(|option| option.category.as_deref() == Some("model"))
                        .map(|option| ModelKey {
                            provider: self.provider.clone(),
                            model: option.current.clone(),
                            option_id: Some(option.id.clone()),
                        });
                }
                self.apply_kimi_open(session, options, history)
            }
            KimiEvent::OpenFailed { error, .. } => self.kimi_open_failed(error),
            KimiEvent::Options {
                session,
                options,
                settled,
            } => {
                if self.thread.as_ref() == Some(&session) {
                    if let Some(k) = &mut self.kimi {
                        if settled && self.model_intent.is_none() {
                            self.confirmed_model = options
                                .iter()
                                .find(|option| option.category.as_deref() == Some("model"))
                                .map(|option| ModelKey {
                                    provider: self.provider.clone(),
                                    model: option.current.clone(),
                                    option_id: Some(option.id.clone()),
                                });
                        }
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
                    self.context_data.plan = steps;
                    self.context_data.plan_text = None;
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
                self.pending_thread = None;
                self.starting = false;
                self.ready = false;
                self.project_ready = false;
                let intentional = self.disconnecting;
                self.disconnecting = false;
                self.threads_loading = false;
                if let Some(k) = &mut self.kimi {
                    self.client_attached = false;
                    k.option_pending = false;
                    k.confirmed_session = None;
                    k.cancelling = false;
                }
                if !intentional {
                    self.error = Some(format!(
                        "{error}\nAny unconfirmed message and context remain in the composer."
                    ));
                }
                self.observation = crate::conversation_runtime::Freshness::Disconnected;
                self.status = "Disconnected · last remote state unknown".into();
                return;
            }
        }
    }
}

impl ConversationRecord {
    pub(crate) fn reduce_event(&mut self, event: Event) {
        // Activity is only updated by correlated provider observations. Loss of a
        // transport changes freshness, never fabricates a terminal work state.
        let mut next_activity = None;
        match &event {
            #[cfg(feature = "codex")]
            Event::Codex(event) if !self.is_kimi() => match event {
                CodexEvent::ThreadStatus { thread, state }
                    if self.thread.as_ref() == Some(thread) && self.pending_thread.is_none() =>
                {
                    next_activity = Some(*state)
                }
                CodexEvent::TurnStarted { thread, .. } if self.thread.as_ref() == Some(thread) => {
                    next_activity = Some(ThreadWorkState::Working)
                }
                CodexEvent::Approval { thread, turn, .. }
                    if self.thread.as_ref() == Some(thread)
                        && self.active_turn.as_ref() == Some(turn) =>
                {
                    next_activity = Some(ThreadWorkState::WaitingOnApproval)
                }
                CodexEvent::ApprovalResolved { id, thread }
                    if self.thread.as_ref() == Some(thread)
                        && self
                            .approvals
                            .iter()
                            .any(|a| &a.id == id && &a.thread == thread)
                        && self.approvals.len() == 1 =>
                {
                    next_activity = Some(ThreadWorkState::Working)
                }
                CodexEvent::TurnCompleted {
                    thread,
                    turn,
                    status,
                } if self.thread.as_ref() == Some(thread)
                    && self.active_turn.as_ref() == Some(turn) =>
                {
                    next_activity = Some(if status == "failed" {
                        ThreadWorkState::Error
                    } else {
                        ThreadWorkState::Idle
                    })
                }
                _ => {}
            },
            #[cfg(feature = "kimi")]
            Event::Kimi(event) if self.is_kimi() => match event {
                KimiEvent::TurnStarted {
                    session,
                    submission,
                    ..
                } if self.kimi_submission_matches(session, submission) => {
                    next_activity = Some(ThreadWorkState::Working)
                }
                KimiEvent::Permission { session, turn, .. }
                    if self.thread.as_ref() == Some(session)
                        && self.active_turn.as_ref() == Some(turn) =>
                {
                    next_activity = Some(ThreadWorkState::WaitingOnApproval)
                }
                KimiEvent::PermissionResolved { session, id }
                    if self.thread.as_ref() == Some(session)
                        && self.kimi.as_ref().is_some_and(|k| {
                            k.permissions.len() == 1 && k.permissions.iter().any(|p| &p.id == id)
                        }) =>
                {
                    next_activity = Some(ThreadWorkState::Working)
                }
                KimiEvent::TurnFinished {
                    session,
                    turn,
                    submission,
                    ..
                } if self.active_turn.as_ref() == Some(turn)
                    && self.kimi_submission_matches(session, submission) =>
                {
                    next_activity = Some(ThreadWorkState::Idle)
                }
                KimiEvent::TurnFailed {
                    session,
                    submission,
                    ..
                } if self.kimi_submission_matches(session, submission) => {
                    next_activity = Some(ThreadWorkState::Error)
                }
                _ => {}
            },
            _ => {}
        }
        if let Some(activity) = next_activity {
            self.observation = Freshness::Attached;
            self.observed_activity = activity;
        }
        match event {
            #[cfg(feature = "codex")]
            Event::Codex(event) if !self.is_kimi() => self.reduce_codex(event),
            #[cfg(feature = "kimi")]
            Event::Kimi(event) if self.is_kimi() => self.reduce_kimi(event),
            _ => self.error = Some("Event belongs to a different conversation provider.".into()),
        }
    }
    pub(crate) fn dispatch_succeeded(&mut self, request: &Request) {
        let _ = request;
    }
    pub(crate) fn dispatch_failed(&mut self, request: Request, error: String) {
        let preparation_request = match &request {
            #[cfg(feature = "codex")]
            Request::Codex(
                CodexRequest::CreateThread
                | CodexRequest::ResumeThread(_)
                | CodexRequest::ListModels { .. },
            ) => true,
            #[cfg(feature = "kimi")]
            Request::Kimi(
                KimiRequest::NewSession
                | KimiRequest::LoadSession(_)
                | KimiRequest::SetOption { .. },
            ) => true,
            _ => false,
        };
        if preparation_request && self.model_intent.is_some() {
            if let Some(intent) = &mut self.model_intent {
                intent.phase = ModelPhase::Failed;
            }
            self.pending_thread = None;
            self.starting = false;
        }
        match request {
            #[cfg(feature = "kimi")]
            Request::Kimi(KimiRequest::Prompt {
                session,
                submission,
                ..
            }) => self.kimi_turn_failed(session, String::new(), submission, error),
            #[cfg(feature = "codex")]
            Request::Codex(CodexRequest::AnswerApproval { id, .. }) => {
                if let Some(approval) = self.approvals.iter_mut().find(|a| a.id == id) {
                    approval.answering = false;
                }
                self.error = Some(error);
            }
            #[cfg(feature = "codex")]
            Request::Codex(CodexRequest::SendPromptWithControls { .. }) => {
                self.context_submit_pending = false;
                if let Some(submission) = self.codex_submission.take() {
                    self.messages.retain(|m| {
                        m.key.as_ref().is_none_or(|(_, turn, item)| {
                            turn != &submission.id.to_string() || item != "pending-user"
                        })
                    });
                }
                self.starting = false;
                self.error = Some(format!("{error}\nThe draft remains available."));
            }
            #[cfg(feature = "kimi")]
            Request::Kimi(KimiRequest::SetOption { .. }) => {
                if let Some(k) = &mut self.kimi {
                    k.option_pending = false;
                }
                self.error = Some(error);
            }
            _ => {
                self.error = Some(error);
                self.threads_loading = false;
                self.history_loading = false;
                if self.active_turn.is_none() {
                    self.starting = false;
                }
            }
        }
    }
}

pub(crate) struct ClientConnection {
    pub generation: u64,
    pub adapter: Adapter,
}

#[cfg(feature = "codex")]
impl ConversationRecord {
    pub(crate) fn new_codex(config: crate::codex::CodexConfig) -> Self {
        let project_name = config
            .project
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let resume_requested = config.resume_thread.is_some();
        Self {
            chat_id: uuid::Uuid::new_v4(),
            provider: crate::model_catalog::ProviderKey {
                backend: crate::conversation::Backend::Codex,
                workspace: config.cwd.clone(),
            },
            draft_revision: 0,
            model_intent: None,
            confirmed_model: None,
            permission_review_required: false,
            thread_model_seen: false,
            client_attached: false,
            client_generation: 0,
            effects: Default::default(),
            observation: crate::conversation_runtime::Freshness::NeverObserved,
            observed_activity: ThreadWorkState::Unknown,
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
            config,
            project_name,
            project_ready: false,
            fresh_thread: false,
            resume_requested,
            pending_thread: None,
            pending_thread_action: None,
            binding_durable: false,
            persist_project: false,
            project_expected: None,
            metadata_error: None,
            threads: Vec::new(),
            listed_threads: Default::default(),
            threads_cursor: None,
            threads_loading: false,
            threads_append: false,
            prompt_drafts: HashMap::new(),
            unavailable_threads: Default::default(),
            history_cursor: None,
            history_loading: false,
            history_reloading: false,
            history_incomplete: false,
            models: Vec::new(),
            models_cursor: None,
            models_loading: false,
            selected_model: None,
            selected_effort: None,
            model_override: false,
            controls: Default::default(),
            context_data: Default::default(),
            context_goal_pending: None,
            context_submit_pending: false,
            codex_submission: None,
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
}

#[cfg(feature = "kimi")]
impl ConversationRecord {
    pub(crate) fn new_kimi(config: KimiConfig) -> Self {
        #[cfg(feature = "codex")]
        {
            let mut record = Self::new_codex(crate::codex::CodexConfig {
                executable: "codex".into(),
                cwd: config.cwd.clone(),
                mcp_bridge: config.mcp_bridge.clone(),
                socket: config.socket.clone(),
                resume_thread: None,
                project: None,
            });
            record.provider = crate::model_catalog::ProviderKey {
                backend: crate::conversation::Backend::Kimi,
                workspace: config.cwd.clone(),
            };
            record.kimi = Some(KimiState::new(config, false));
            record.active_kimi = true;
            record.binding_durable = true;
            record
        }
        #[cfg(not(feature = "codex"))]
        {
            Self {
                chat_id: uuid::Uuid::new_v4(),
                provider: crate::model_catalog::ProviderKey {
                    backend: crate::conversation::Backend::Kimi,
                    workspace: config.cwd.clone(),
                },
                draft_revision: 0,
                model_intent: None,
                confirmed_model: None,
                permission_review_required: false,
                thread_model_seen: false,
                client_attached: false,
                client_generation: 0,
                effects: Default::default(),
                observation: crate::conversation_runtime::Freshness::NeverObserved,
                observed_activity: ThreadWorkState::Unknown,
                kimi: Some(KimiState::new(config, false)),
                active_kimi: true,
                vault_permission_requested: None,
                vault_write_enabled: None,
                research_requested: false,
                collect_references: None,
                relevant_sources: false,
                research_workspace: None,
                project_ready: false,
                fresh_thread: false,
                resume_requested: false,
                pending_thread: None,
                binding_durable: true,
                persist_project: false,
                metadata_error: None,
                threads: Vec::new(),
                listed_threads: Default::default(),
                threads_cursor: None,
                threads_loading: false,
                threads_append: false,
                prompt_drafts: HashMap::new(),
                unavailable_threads: Default::default(),
                history_cursor: None,
                history_loading: false,
                history_reloading: false,
                history_incomplete: false,
                project_name: String::new(),
                context_data: Default::default(),
                context_goal_pending: None,
                context_submit_pending: false,
                ready: false,
                starting: false,
                active_turn: None,
                thread: None,
                disconnecting: false,
                prompt: String::new(),
                messages: Vec::new(),
                status: "Disconnected".into(),
                error: None,
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct DerivedEvidence {
    pub workspace: uuid::Uuid,
    pub revision: crate::notes::Revision,
    pub prompt: String,
    pub text: String,
}

/// A UI or other client names the record and optionally its connection epoch.
pub(crate) struct ConversationAction {
    pub chat_id: uuid::Uuid,
    pub expected_generation: Option<u64>,
    pub kind: ConversationActionKind,
}
pub(crate) enum ConversationActionKind {
    SetDraft {
        expected_revision: u64,
        text: String,
    },
    EditDraft {
        expected_revision: u64,
        text: String,
        context: DraftContext,
    },
    SetContextDraft {
        expected_revision: u64,
        draft: DraftContext,
    },
    Submit {
        evidence: Option<DerivedEvidence>,
    },
    Stop,
    ClearDisplay,
    AcknowledgePermissionReview,
    SetRelevantSources(bool),
    RefreshThreads {
        cursor: Option<String>,
    },
    LoadHistory,
    #[cfg(feature = "codex")]
    ThreadAction {
        thread: String,
        action: crate::codex::ThreadAction,
    },
    #[cfg(feature = "codex")]
    RenameProject(String),
    SetEffort(String),
    #[cfg(feature = "codex")]
    Goal(crate::codex::CodexRequest),
    #[cfg(feature = "codex")]
    SetCodexControls(crate::codex::TurnControls),
    #[cfg(feature = "codex")]
    AnswerCodex {
        id: serde_json::Value,
        decision: crate::codex::ApprovalDecision,
    },
    #[cfg(feature = "kimi")]
    AnswerKimi {
        id: serde_json::Value,
        option: Option<String>,
    },
    #[cfg(feature = "kimi")]
    SetKimiOption {
        id: String,
        value: String,
    },
    Request(Request),
    AttachFile {
        path: std::path::PathBuf,
    },
    Disconnect,
}

#[cfg(test)]
mod replay_regressions {
    use super::*;

    fn row(role: &'static str, text: impl Into<String>) -> ConversationMessage {
        ConversationMessage {
            role,
            key: None,
            text: text.into(),
            command: None,
            activity: None,
            uncertain: false,
        }
    }
    fn history(role: &str, text: impl Into<String>) -> crate::conversation::HistoryMessage {
        crate::conversation::HistoryMessage {
            role: role.into(),
            text: text.into(),
            message_id: None,
            turn_id: None,
        }
    }
    fn texts(record: &ConversationRecord) -> Vec<(&str, &str)> {
        record
            .messages
            .iter()
            .map(|m| (m.role, m.text.as_str()))
            .collect()
    }
    #[cfg(feature = "kimi")]
    fn kimi_record() -> ConversationRecord {
        let cwd = std::env::temp_dir();
        let config = KimiConfig {
            executable: cwd.join("unused"),
            cwd: cwd.clone(),
            mcp_bridge: cwd.join("bridge"),
            socket: cwd.join("socket"),
            resume_session: Some("session".into()),
        };
        let mut record = ConversationRecord::new_kimi(config);
        record.thread = Some("session".into());
        record.pending_thread = Some(Some("session".into()));
        record.observation = Freshness::Unconfirmed;
        record
    }
    #[cfg(feature = "kimi")]
    #[test]
    fn kimi_replay_preserves_order_repetitions_and_replaces_partial_observations() {
        let mut record = kimi_record();
        let mut partial = row("Kimi", "partial");
        partial.key = Some(("session".into(), "old-turn".into(), "message:reply".into()));
        let mut activity = row("Kimi", "local activity details");
        activity.activity = Some(crate::conversation::Activity {
            item: "tool".into(),
            kind: "mcpToolCall".into(),
            title: "Local tool".into(),
            status: "completed".into(),
            details: String::new(),
            truncated: false,
        });
        let mut shadow = row("You", "pending local draft");
        shadow.key = Some(("session".into(), "local".into(), "user".into()));
        record.messages = vec![
            row("You", "older cached"),
            row("You", "Continue"),
            activity,
            partial,
            row("You", "Continue"),
            shadow,
        ];
        let mut full = history("assistant", "partial completed");
        full.message_id = Some("message:reply".into());
        record.apply_kimi_open(
            "session".into(),
            vec![],
            vec![
                history("user", "Continue"),
                full,
                history("user", "Continue"),
                history("assistant", "new response"),
            ],
        );
        assert_eq!(
            texts(&record),
            vec![
                ("You", "older cached"),
                ("You", "Continue"),
                ("Kimi", "local activity details"),
                ("Kimi", "partial completed"),
                ("You", "Continue"),
                ("You", "pending local draft"),
                ("Kimi", "new response")
            ]
        );
        assert!(
            record.messages[0].uncertain
                && record.messages[2].uncertain
                && record.messages[5].uncertain
        );
        assert!(record.messages[2].activity.is_some());
        assert!(
            !record.messages[1].uncertain
                && !record.messages[3].uncertain
                && !record.messages[4].uncertain
        );
        assert!(record.history_incomplete);
        assert!(record.error.is_none());

        // A record can already carry its remote ID before its first load.
        let mut first_open = kimi_record();
        first_open.apply_kimi_open(
            "session".into(),
            vec![],
            vec![
                history("user", "Continue"),
                history("assistant", "Done"),
                history("user", "Continue"),
            ],
        );
        assert_eq!(
            texts(&first_open),
            vec![("You", "Continue"), ("Kimi", "Done"), ("You", "Continue")]
        );
        assert!(!first_open.history_incomplete);
    }
    #[cfg(feature = "kimi")]
    #[test]
    fn empty_or_oversize_kimi_replay_retains_cache_and_reports_the_gap() {
        let mut record = kimi_record();
        record.messages = vec![row("Kimi", "cached observation")];
        record.apply_kimi_open("session".into(), vec![], vec![]);
        assert_eq!(texts(&record), vec![("Kimi", "cached observation")]);
        assert!(record.status.to_lowercase().contains("cached"));
        assert!(record.history_incomplete && record.messages[0].uncertain);
        record.kimi_open_failed("replay failed".into());
        assert!(record.history_incomplete && record.messages[0].uncertain);
        record.pending_thread = Some(Some("session".into()));
        record.apply_kimi_open(
            "session".into(),
            vec![],
            (0..257)
                .map(|i| history("user", format!("message-{i}")))
                .collect(),
        );
        assert_eq!(texts(&record), vec![("Kimi", "cached observation")]);
        assert!(
            record
                .error
                .as_deref()
                .is_some_and(|error| error.contains("limit"))
        );
    }
    #[cfg(feature = "kimi")]
    #[test]
    fn different_provider_ids_are_not_collapsed_even_when_text_matches() {
        let mut record = kimi_record();
        let mut prior = row("Kimi", "identical");
        prior.key = Some(("session".into(), "old-turn".into(), "message:old".into()));
        record.messages.push(prior);
        let mut replay = history("assistant", "identical");
        replay.message_id = Some("message:new".into());
        record.apply_kimi_open("session".into(), vec![], vec![replay]);
        assert_eq!(record.messages.len(), 2);
        assert!(record.messages[0].uncertain);
        assert!(!record.messages[1].uncertain);
    }
    #[cfg(feature = "codex")]
    #[test]
    fn codex_replay_checks_final_display_bounds_without_double_counting() {
        let cwd = std::env::temp_dir();
        let config = crate::codex::CodexConfig {
            executable: cwd.join("unused"),
            cwd: cwd.clone(),
            mcp_bridge: cwd.join("bridge"),
            socket: cwd.join("socket"),
            resume_thread: None,
            project: None,
        };
        let mut record = ConversationRecord::new_codex(config);
        record.thread = Some("thread".into());
        record.history_reloading = true;
        record.messages = (0..200)
            .map(|i| row("You", format!("message-{i}")))
            .collect();
        record.reduce_event(Event::Codex(CodexEvent::History {
            thread: "thread".into(),
            messages: (0..200)
                .map(|i| history("You", format!("message-{i}")))
                .collect(),
            next_cursor: None,
        }));
        assert_eq!(record.messages.len(), 200);
        assert!(record.error.is_none());
        record.history_reloading = true;
        record.reduce_event(Event::Codex(CodexEvent::History {
            thread: "thread".into(),
            // Each side fits on its own; the staged combined display does not.
            messages: (0..57)
                .map(|i| history("You", format!("new-{i}")))
                .collect(),
            next_cursor: None,
        }));
        assert_eq!(record.messages.len(), 200);
        assert!(record.history_incomplete && record.messages.iter().all(|m| m.uncertain));
        assert!(
            record
                .error
                .as_deref()
                .is_some_and(|error| error.contains("limit"))
        );
    }
}
