//! View-local research buffers. Every durable change goes through shared commands.
use super::{field_single, theme};
use crate::research_desktop::{OutputCard, OutputKind, OutputSpec, ReferenceCandidate};
use crate::{
    App,
    commands::{Caller, Command, CommandResult},
    notes::{NoteId, NoteView, Revision},
    research::{
        ArtifactInput, ContextMode, ResearchBrief, ResearchCommand as Cmd, ResearchResult,
        ResearchRound, ResearchWorkspace, SourceAssessment, SourceKind, SourceMetadata, SourcePage,
        SourceScope, ThreadRef,
    },
};
use eframe::egui;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use uuid::Uuid;

struct WorkspaceDraft {
    expected: Revision,
    title: String,
    question: String,
}
struct AssessmentDraft {
    expected: Revision,
    value: SourceAssessment,
}
#[derive(Clone, Default)]
pub(super) struct ConversationCapture {
    pub threads: Vec<ThreadRef>,
    pub current: Option<ThreadRef>,
    pub discussion: String,
    pub answer: Option<(String, Option<String>)>,
}
#[derive(Default)]
struct FlowDraft {
    expected: Option<Revision>,
    brief: ResearchBrief,
    question: String,
    summary: String,
    open_questions: String,
    discussion: String,
    thread: Option<ThreadRef>,
    turn_id: Option<String>,
    artifact_ids: Vec<Uuid>,
    artifact_title: String,
    artifact_value: String,
    artifact_url: String,
    artifact_mode: usize,
    archive: bool,
    external: bool,
    followup: String,
    output: usize,
}
impl FlowDraft {
    fn checkpoint_dirty(&self) -> bool {
        !self.question.is_empty()
            || !self.summary.is_empty()
            || !self.open_questions.is_empty()
            || !self.discussion.is_empty()
    }
    fn capture_answer(&mut self, capture: &ConversationCapture) -> bool {
        if self.checkpoint_dirty() || capture.current.is_none() {
            return false;
        }
        let Some((answer, turn)) = &capture.answer else {
            return false;
        };
        self.summary = answer.clone();
        self.thread = capture.current.clone();
        self.turn_id = turn.clone();
        self.discussion = capture.discussion.clone();
        true
    }
    fn dirty(&self, w: &ResearchWorkspace) -> bool {
        self.brief != w.brief
            || self.checkpoint_dirty()
            || !self.artifact_value.is_empty()
            || !self.artifact_title.is_empty()
            || !self.artifact_url.is_empty()
            || !self.followup.is_empty()
    }
}
struct StartCapture {
    thread: Option<ThreadRef>,
    discussion: String,
    workspace: Option<Uuid>,
    step: usize,
}
#[derive(Default)]
pub(super) struct ResearchUi {
    selected: Option<Uuid>,
    details_open: bool,
    preview_open: bool,
    report_open: bool,
    inputs_open: bool,
    artifact_detail: Option<Uuid>,
    finding: Option<(Uuid, Uuid)>,
    conversation: ConversationCapture,
    flows: HashMap<Uuid, FlowDraft>,
    start: Option<StartCapture>,
    pending_round: Option<(Uuid, Uuid)>,
    pending_artifact: Option<Uuid>,
    new_discussion: String,
    new_thread: Option<ThreadRef>,
    prepared_external: bool,
    source: Option<Uuid>,
    citation: String,
    jump: Option<(Uuid, Option<u32>)>,
    prepared_for: Option<(Uuid, Revision)>,
    built_input: Option<(String, usize)>,
    drafts: HashMap<Uuid, WorkspaceDraft>,
    assessments: HashMap<(Uuid, Uuid, bool), AssessmentDraft>,
    agent_assessment: bool,
    pending_import: bool,
    pending_link: bool,
    link_waiting: bool,
    link: Option<(Uuid, Revision, NoteId)>,
    new_open: bool,
    new_title: String,
    new_question: String,
    add_open: bool,
    import_path: String,
    note_path: String,
    pending: Option<Uuid>,
    pending_create: bool,
    worker: Option<crate::research_import::ImportWorker>,
    import: Option<(Uuid, Uuid, Revision)>,
    retained_import: Option<(Uuid, Revision, crate::research::ImportedSource)>,
    pub error: Option<String>,
    status: String,
    prompt: String,
    collect_open: bool,
    candidates: Vec<(bool, ReferenceCandidate)>,
    collect_markdown: String,
    discovery_query: String,
    reader_worker: Option<crate::research_reader::ResearchReaderWorker>,
    picker_job: Option<(Uuid, Uuid)>,
    discovery_job: Option<Uuid>,
    discovery_context: Option<(Uuid, String)>,
    pdf_job: Option<(Uuid, Uuid, u32)>,
    pdf_texture: Option<(Uuid, u32, egui::TextureHandle)>,
    reader_page: usize,
    reader_search: String,
    reader_hit: Option<(Uuid, usize, usize)>,
    reader_scroll_hit: bool,
    reader_pdf: bool,
    reader_center: bool,
    file_queue: VecDeque<(Uuid, PathBuf)>,
    acquire_queue: VecDeque<(Uuid, Uuid, ArtifactInput, bool)>,
    retry_queue: VecDeque<(Uuid, Uuid)>,
    evidence_open: bool,
    evidence_query: String,
    passages: Vec<crate::research_desktop::Passage>,
    passage_context: Option<(Uuid, Revision, String, Vec<Uuid>)>,
    output_open: bool,
    output_title: String,
    output_kind: usize,
    output_workspace: Option<Uuid>,
    collect_workspace: Option<Uuid>,
    collect_thread: Option<ThreadRef>,
    collect_turn: Option<String>,
    requested_pdf_page: Option<(Uuid, u32)>,
    output_cards: Vec<OutputCard>,
    desktop_dirty: bool,
    pending_output: bool,
    failed_file: Option<(Uuid, Vec<PathBuf>)>,
    pdf_failed: Option<(Uuid, u32)>,
    pdf_pages: Option<(Uuid, u32)>,
    drop_target: Option<egui::Rect>,
}
pub(super) enum Action {
    OpenNote(NoteId),
    Prepare {
        prompt: String,
        evidence: String,
        external: bool,
    },
    OpenThread(ThreadRef),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum FlowView {
    Details,
    Inputs,
    Report,
}

impl ResearchUi {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
            || self.import.is_some()
            || self.retained_import.is_some()
            || self.link.is_some()
            || self.start.is_some()
    }
    pub fn unsaved(&self, app: &App) -> bool {
        self.desktop_dirty
            || !self.file_queue.is_empty()
            || !self.acquire_queue.is_empty()
            || !self.retry_queue.is_empty()
            || self.failed_file.is_some()
            || self.drafts.iter().any(|(id, d)| {
                app.research_workspace(*id)
                    .is_some_and(|w| d.title != w.title || d.question != w.question)
            })
            || self.assessments.iter().any(|((id, source, human), draft)| {
                app.research_workspace(*id)
                    .and_then(|w| w.sources.iter().find(|s| s.id == *source))
                    .is_some_and(|s| {
                        let canonical = if *human {
                            s.human_assessment.as_ref()
                        } else {
                            s.agent_assessment.as_ref()
                        };
                        canonical.cloned().unwrap_or_default() != draft.value
                    })
            })
            || self
                .flows
                .iter()
                .any(|(id, d)| app.research_workspace(*id).is_some_and(|w| d.dirty(w)))
            || (!self.new_title.is_empty()
                || !self.new_question.is_empty()
                || !self.new_discussion.is_empty())
    }
    fn execute(&mut self, app: &mut App, command: Cmd) {
        match app.execute(Command::Research(command), Caller::Local) {
            Ok(CommandResult::Accepted { request_id }) => {
                self.pending = Some(request_id);
                self.status = "Saving workspace…".into();
                self.error = None;
            }
            Ok(CommandResult::Research(ResearchResult::Prompt(prompt))) => {
                self.prompt = prompt;
                self.error = None;
            }
            Ok(_) => self.error = Some("Unexpected research response".into()),
            Err(error) => self.error = Some(format!("{error}. Your input is retained.")),
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context, app: &mut App) {
        self.poll_desktop(ctx, app);
        if let Some(request) = self.pending {
            match app.io_status(request) {
                Some(crate::app::IoStatus::Pending) | None => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Some(crate::app::IoStatus::Completed) => {
                    self.pending = None;
                    self.status = "Workspace saved locally".into();
                    if self.pending_output {
                        self.pending_output = false;
                        self.desktop_dirty = false;
                        self.output_open = false;
                        self.output_cards.clear();
                        self.output_title.clear();
                    }
                    if let Some((id, round)) = self.pending_round.take() {
                        if app
                            .research_workspace(id)
                            .is_some_and(|w| w.rounds.iter().any(|r| r.id == round))
                        {
                            if let Some(d) = self.flows.get_mut(&id) {
                                d.question.clear();
                                d.summary.clear();
                                d.open_questions.clear();
                                d.discussion.clear();
                                d.thread = None;
                                d.turn_id = None;
                                d.artifact_ids.clear();
                            }
                        }
                    }
                    if let Some(id) = self.pending_artifact.take() {
                        if let Some(d) = self.flows.get_mut(&id) {
                            d.artifact_title.clear();
                            d.artifact_value.clear();
                            d.artifact_url.clear();
                        }
                    }
                    if self.pending_create {
                        if let Some(start) = &mut self.start {
                            start.workspace = app.research_created_workspace(request);
                        }
                        if let Some(id) = self.start.as_ref().and_then(|start| start.workspace) {
                            self.selected = Some(id);
                            self.source = None;
                        }
                        self.status =
                            "Workspace saved locally. Select it in the Research sidebar.".into();
                        self.pending_create = false;
                        self.new_open = false;
                    }
                    if self.pending_link {
                        self.link = None;
                        self.pending_link = false;
                    }
                    if self.pending_import {
                        self.retained_import = None;
                        self.pending_import = false;
                    }
                }
                Some(status) => {
                    self.error = Some(format!(
                        "Workspace save failed: {status:?}. Input retained."
                    ));
                    self.pending_output = false;
                    self.pending_artifact = None;
                    self.pending_round = None;
                    if let Some(start) = &mut self.start {
                        start.step = start.step.saturating_sub(1);
                    }
                    self.pending = None;
                    self.status = "Workspace save failed; input retained".into();
                    self.pending_import = false;
                    self.pending_link = false;
                    if self.pending_create {
                        self.start = None;
                    }
                    self.pending_create = false;
                }
            }
        }
        if self.pending.is_none() && self.error.is_none() {
            self.advance_start(app);
            if self.start.is_some() && self.pending.is_none() && self.error.is_none() {
                ctx.request_repaint();
            }
        }
        if let Some(completion) = self.worker.as_ref().and_then(|w| w.poll()) {
            self.apply_import_completion(completion, app);
        }
        if self.link_waiting && self.pending.is_none() {
            if let Some((id, expected, note_id)) = self.link {
                match app.note(note_id) {
                    Ok(note) if note.save.error.is_some() => {
                        self.link_waiting = false;
                        self.error = Some(format!(
                            "Research note save failed: {}. Note retained; retry saves and then retry linking.",
                            note.save.error.unwrap_or_default()
                        ));
                        self.status = "Note not yet saved; link retained".into();
                    }
                    Ok(note)
                        if note.save.persisted == Some(note.save.accepted)
                            && !app.research_pending(id) =>
                    {
                        self.link_waiting = false;
                        self.execute(
                            app,
                            Cmd::LinkNote {
                                id,
                                expected,
                                note_id,
                            },
                        );
                        self.pending_link = self.pending.is_some();
                    }
                    Ok(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                    Err(error) => {
                        self.link_waiting = false;
                        self.error = Some(format!("Cannot link retained note: {error}"));
                    }
                }
            }
        }
    }
    fn apply_import_completion(
        &mut self,
        completion: crate::research_import::ImportCompletion,
        app: &mut App,
    ) {
        if self
            .import
            .as_ref()
            .is_some_and(|(job, _, _)| completion.id == *job)
        {
            if let Some((_job, id, expected)) = self.import.take() {
                match completion.result {
                    Ok(source) => {
                        use crate::research_import::ImportedKind;
                        self.retained_import = Some((
                            id,
                            expected,
                            crate::research::ImportedSource {
                                title: source.title,
                                kind: match source.kind {
                                    ImportedKind::Pdf => SourceKind::Pdf,
                                    ImportedKind::Text => SourceKind::Text,
                                },
                                metadata: SourceMetadata {
                                    origin_label: Some(source.origin),
                                    ..Default::default()
                                },
                                pages: source
                                    .pages
                                    .into_iter()
                                    .map(|p| SourcePage {
                                        number: p.number,
                                        text: p.text,
                                    })
                                    .collect(),
                            },
                        ));
                        self.save_import(app, false);
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        self.status = "Import failed".into();
                    }
                }
            }
        }
    }
    fn save_import(&mut self, app: &mut App, latest: bool) {
        if let Some((id, expected, source)) = &self.retained_import {
            let expected = if latest {
                app.research_workspace(*id)
                    .map_or(*expected, |w| w.revision)
            } else {
                *expected
            };
            self.execute(
                app,
                Cmd::AddImported {
                    id: *id,
                    expected,
                    source: source.clone(),
                },
            );
            self.pending_import = self.pending.is_some();
        }
    }
    pub fn sidebar(&mut self, ui: &mut egui::Ui, app: &mut App) -> bool {
        self.drop_target = Some(ui.max_rect());
        if self.selected.is_none() {
            self.selected = app.research_workspaces().first().map(|w| w.id);
        }
        let mut navigated = false;
        ui.horizontal(|ui| {
            let action_width = theme::control_size(ui);
            let picker_width =
                (ui.available_width() - action_width - ui.spacing().item_spacing.x).max(60.0);
            ui.allocate_ui_with_layout(
                egui::vec2(picker_width, action_width),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    egui::ComboBox::from_id_salt("research-workspace")
                        .selected_text(self.selected_title(app))
                        .width(picker_width)
                        .truncate()
                        .show_ui(ui, |ui| {
                            for workspace in app.research_workspaces() {
                                if ui
                                    .selectable_label(
                                        self.selected == Some(workspace.id),
                                        &workspace.title,
                                    )
                                    .clicked()
                                {
                                    self.selected = Some(workspace.id);
                                    self.source = None;
                                    self.preview_open = false;
                                    navigated = true;
                                }
                            }
                        });
                },
            );
            if theme::button(ui, theme::Icon::New, "New research workspace", !self.busy()).clicked()
            {
                self.new_open = true;
                navigated = true;
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("research-sources-sidebar")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(22.0);
                if let Some(workspace) = self
                    .selected
                    .and_then(|id| app.research_workspace(id))
                    .cloned()
                {
                    ui.horizontal(|ui| {
                        ui.strong("Sources");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let more = theme::button(
                                ui,
                                theme::Icon::More,
                                "Source actions, research details and citations",
                                true,
                            );
                            egui::Popup::menu(&more).show(|ui| {
                                if ui.button("Research details…").clicked() {
                                    self.source = None;
                                    self.preview_open = false;
                                    self.details_open = true;
                                    ui.close();
                                }
                                if ui.add_enabled(!self.busy() && !app.research_pending(workspace.id), egui::Button::new("Get full text for linked papers")).clicked() { self.queue_full_text(app, &workspace); ui.close(); }
                                if ui.button("Find evidence…").clicked() { self.evidence_open = true; ui.close(); }
                                ui.separator();
                                let label = ui.label("Open a source citation");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.citation)
                                        .hint_text("[source:… page:2]"),
                                ).labelled_by(label.id);
                                if ui.button("Open citation").clicked() {
                                    match parse_citation(&self.citation) {
                                        Some((source_id, page)) => {
                                            let target =
                                                app.research_workspaces().into_iter().find(|w| {
                                                    w.sources.iter().any(|s| {
                                                        s.id == source_id
                                                            && s.pages.iter().any(|p| {
                                                                page.is_none() || p.number == page
                                                            })
                                                    })
                                                });
                                            if let Some(w) = target {
                                                self.selected = Some(w.id);
                                                self.source = Some(source_id);
                                                self.jump = Some((source_id, page));
                                                self.error = None;
                                                self.preview_open = true;
                                            } else {
                                                self.error = Some(
                            "Citation source or page is not available in local workspaces.".into(),
                        );
                                            }
                                        }
                                        None => {
                                            self.error = Some(
                                                "Paste a citation such as [source:UUID page:2]."
                                                    .into(),
                                            )
                                        }
                                    }

                                    ui.close();
                                }
                            });
                        });
                    });
                    ui.scope(|ui| {
                        ui.visuals_mut().button_frame = false;
                    ui.menu_button("+  Add source", |ui| {
                        if ui.button("Choose files…").clicked() {
                            self.pick_files(ui.ctx(), workspace.id);
                            ui.close();
                        }
                        if ui.button("From answer or links…").clicked() {
                            let answer = self
                                .conversation
                                .answer
                                .as_ref()
                                .map(|(text, _)| text.clone())
                                .unwrap_or_default();
                            self.open_references(&answer);
                            ui.close();
                        }
                        if ui.button("Find papers…").clicked() {
                            self.collect_workspace = Some(workspace.id);
                            self.collect_thread = None;
                            self.collect_turn = None;
                            self.collect_open = true;
                            ui.close();
                        }
                        if ui.button("Existing note…").clicked() {
                            self.add_open = true;
                            ui.close();
                        }
                        if ui.button("Link or retained material…").clicked() {
                            self.inputs_open = true;
                            self.artifact_detail = None;
                            ui.close();
                        }
                    });
                    });
                    self.acquisition_controls(ui, app, &workspace);
                    if let Some(error) = app.research_error().map(str::to_owned) {
                        ui.colored_label(ui.visuals().error_fg_color, "Research queue paused").on_hover_text(&error);
                        ui.collapsing("Details", |ui| { ui.small(&error); });
                        if ui.add_enabled(!self.busy() && !app.research_pending(workspace.id), egui::Button::new("Retry queue")).on_hover_text("Resume this workspace's unfinished acquisitions after resolving the storage error").clicked() {
                            let job_ids = workspace.acquisitions.iter().filter(|job| job.status != crate::research::AcquisitionStatus::Ready).map(|job| job.id).collect();
                            self.execute(app, Cmd::RetryAcquisitions { id: workspace.id, expected: workspace.revision, job_ids });
                        }
                    }
                    ui.add_space(10.0);
                    for source in &workspace.sources {
                        ui.horizontal(|ui| {
                            let mut selected = source.selected;
                            let include = ui.add_enabled(
                                !self.busy() && !app.research_pending(workspace.id),
                                egui::Checkbox::without_text(&mut selected),
                            ).on_hover_text("Include this source in research context");
                            include.widget_info(|| egui::WidgetInfo::selected(
                                egui::WidgetType::Checkbox, include.enabled(), selected,
                                format!("Include {} in research context", source.title),
                            ));
                            if include.changed()
                            {
                                self.execute(
                                    app,
                                    Cmd::SelectSource {
                                        id: workspace.id,
                                        expected: workspace.revision,
                                        source_id: source.id,
                                        selected,
                                    },
                                );
                            }
                        let state = workspace
                            .acquisitions
                            .iter()
                            .rev()
                            .find(|job| job.source_id == source.id);
                        let label = if let Some(job) = state {
                            match job.status {
                                crate::research::AcquisitionStatus::Queued => "◌ Queued",
                                crate::research::AcquisitionStatus::Running => {
                                    "◌ Getting full text"
                                }
                                crate::research::AcquisitionStatus::Failed => {
                                    "! Acquisition failed"
                                }
                                crate::research::AcquisitionStatus::Cancelled => "Cancelled",
                                crate::research::AcquisitionStatus::Ready => {
                                    if crate::research::source_has_text(source) {
                                        "Text available"
                                    } else {
                                        "Original saved · no text"
                                    }
                                }
                            }
                        } else if crate::research::source_has_text(source) {
                            "Text available"
                        } else {
                            "Reference only"
                        };
                        let icon = match state.map(|job| &job.status) {
                            Some(crate::research::AcquisitionStatus::Queued | crate::research::AcquisitionStatus::Running) => "◌",
                            Some(crate::research::AcquisitionStatus::Failed) => "!",
                            Some(crate::research::AcquisitionStatus::Cancelled) => "○",
                            _ if crate::research::source_has_text(source) => "✓",
                            _ => "○",
                        };
                        let availability = ui.label(egui::RichText::new(icon).small().color(theme::muted(ui))).on_hover_text(label);
                        availability.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
                            let active = self.preview_open && self.source == Some(source.id);
                            let row = ui.add(
                                    egui::Button::selectable(
                                        active,
                                        &source.title,
                                    )
                                    .truncate(),
                                ).on_hover_text(&source.title);
                            row.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, row.enabled(), active, &source.title));
                            if active {
                                ui.painter().line_segment(
                                    [row.rect.left_top(), row.rect.left_bottom()],
                                    egui::Stroke::new(2.0, theme::colors(ui.visuals().dark_mode).accent),
                                );
                            }
                            if row.has_focus() && !row.hovered() {
                                egui::Tooltip::for_widget(&row).show(|ui| { ui.label(&source.title); });
                            }
                            if row.has_focus() {
                                ui.painter().rect_stroke(row.rect, 3.0,
                                    egui::Stroke::new(2.0, theme::colors(ui.visuals().dark_mode).accent),
                                    egui::StrokeKind::Inside);
                            }
                            if row.clicked() {
                                self.source = Some(source.id);
                                self.reader_page = 0;
                                self.reader_pdf = false;
                                self.reader_search.clear();
                                self.preview_open = true;
                            }
                        });

                    }
                    if workspace.sources.is_empty() && workspace.artifacts.is_empty() {
                        ui.weak("Add files, PDFs or links.");
                    }
                    let collected: Vec<_> = workspace.artifacts.iter().filter(|a| a.role != crate::research::ArtifactRole::Output && !workspace.sources.iter().any(|source| source.paper.as_ref().is_some_and(|paper| paper.artifact_id == a.id))).collect();
                    if !collected.is_empty() {
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("Collected material")
                                .small()
                                .color(theme::muted(ui)),
                        );
                    }
                    for artifact in collected
                    {
                        if ui
                            .add(
                                egui::Button::new(format!("↗  {}", artifact.title))
                                    .frame(false)
                                    .truncate(),
                            )
                            .on_hover_text("View collected input details")
                            .clicked()
                        {
                            self.artifact_detail = Some(artifact.id);
                            self.inputs_open = true;
                        }
                    }
                    if app
                        .research_artifact_receipts()
                        .iter()
                        .any(|(_, id, _)| *id == workspace.id)
                        && ui.button("Retry retained input…").clicked()
                    {
                        self.inputs_open = true;
                    }
                } else {
                    ui.weak("Create a workspace to collect sources.");
                }
                if self.retained_import.is_some() && self.pending.is_none() {
                    if ui.button("Retry retained import").clicked() {
                        self.save_import(app, true);
                    }
                    if ui.button("Discard retained import").clicked() {
                        self.retained_import = None;
                    }
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                if self.start.is_some() && self.pending.is_none() && self.error.is_some() {
                    if ui.button("Retry initial research capture").clicked() {
                        self.error = None;
                    }
                    if ui
                        .button("Keep workspace without remaining initial capture")
                        .clicked()
                    {
                        self.start = None;
                    }
                }
                if !self.new_open
                    && (!self.new_title.is_empty()
                        || !self.new_question.is_empty()
                        || !self.new_discussion.is_empty())
                    && self.start.is_none()
                {
                    if ui.button("Resume new workspace draft").clicked() {
                        self.new_open = true;
                    }
                    if ui.button("Discard new workspace draft").clicked() {
                        self.new_title.clear();
                        self.new_question.clear();
                        self.new_discussion.clear();
                        self.new_thread = None;
                    }
                }
            });
        navigated
    }
    pub(super) fn conversation_prepared(&mut self) {
        if let Some(id) = self.selected {
            if self
                .prepared_for
                .is_some_and(|(prepared, _)| prepared == id)
            {
                if let (Some(draft), Some((transferred, _))) =
                    (self.flows.get_mut(&id), self.built_input.as_ref())
                {
                    if draft.followup == *transferred {
                        draft.followup.clear();
                    }
                }
            }
        }
        self.report_open = false;
        self.details_open = false;
    }

    pub(super) fn selected_workspace_id(&self) -> Option<Uuid> {
        self.selected
    }

    pub(super) fn selected_title(&self, app: &App) -> String {
        self.selected
            .and_then(|id| app.research_workspace(id))
            .map_or_else(|| "Research".into(), |w| w.title.clone())
    }

    pub(super) fn outcomes(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        app: &mut App,
        notes: &[NoteView],
    ) -> Option<Action> {
        let mut action = None;
        ui.strong("Outcomes");
        ui.add_space(20.0);
        egui::ScrollArea::vertical()
            .id_salt("research-outcomes")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(w) = self
                    .selected
                    .and_then(|id| app.research_workspace(id))
                    .cloned()
                else {
                    ui.weak("Reports and research notes appear here.");
                    return None;
                };
                if ui
                    .add(egui::Button::new("+  Create report…").frame(false))
                    .clicked()
                {
                    let draft = self.flows.entry(w.id).or_insert_with(|| FlowDraft {
                        expected: Some(w.revision),
                        brief: w.brief.clone(),
                        ..Default::default()
                    });
                    draft.output = 1;
                    self.report_open = true;
                }
                if ui
                    .add(egui::Button::new("+  Create output…").frame(false))
                    .clicked()
                {
                    if self.output_cards.is_empty() {
                        self.output_workspace = Some(w.id);
                        self.output_title = w.title.clone();
                        self.output_cards = w
                            .rounds
                            .iter()
                            .take(8)
                            .map(|round| OutputCard {
                                heading: round.question.clone(),
                                body: round.summary.clone(),
                                citations: source_citations(&round.summary),
                            })
                            .collect();
                        if self.output_cards.is_empty() {
                            self.output_cards.push(OutputCard {
                                heading: String::new(),
                                body: String::new(),
                                citations: Vec::new(),
                            });
                        }
                    }
                    self.output_open = true;
                }
                for artifact in w
                    .artifacts
                    .iter()
                    .filter(|a| a.role == crate::research::ArtifactRole::Output)
                {
                    if ui
                        .add(egui::Button::new(&artifact.title).frame(false))
                        .on_hover_text("Open generated local output")
                        .clicked()
                    {
                        if let Some(path) = &artifact.managed_path {
                            match app.research_asset_path(path) {
                                Ok(path) => {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(
                                        url::Url::from_file_path(&path)
                                            .map(|url| url.to_string())
                                            .unwrap_or_default(),
                                    ));
                                }
                                Err(error) => self.error = Some(error.to_string()),
                            }
                        }
                    }
                    if let Some(path) = &artifact.managed_path {
                        ui.label(
                            egui::RichText::new(format!("{:?} · saved locally", artifact.kind))
                                .small()
                                .color(theme::muted(ui)),
                        )
                        .on_hover_text(path);
                    }
                }
                ui.add_space(18.0);
                ui.label(egui::RichText::new("Notes").small().color(theme::muted(ui)));
                let mut count = 0;
                for id in &w.research_notes {
                    if let Some(note) = notes.iter().find(|n| n.note.id == *id && !n.note.trashed) {
                        count += 1;
                        if ui
                            .add(
                                egui::Button::new(super::note_title(&note.note.path))
                                    .frame(false)
                                    .truncate(),
                            )
                            .clicked()
                        {
                            action = Some(Action::OpenNote(*id));
                        }
                    }
                }
                if count == 0 {
                    ui.weak("No linked notes yet.");
                }
                if !w.rounds.is_empty() {
                    ui.add_space(20.0);
                    ui.label(
                        egui::RichText::new("Findings")
                            .small()
                            .color(theme::muted(ui)),
                    );
                    for round in &w.rounds {
                        if ui
                            .add(
                                egui::Button::new(if !round.question.trim().is_empty() {
                                    round.question.as_str()
                                } else {
                                    round
                                        .summary
                                        .lines()
                                        .find(|line| !line.trim().is_empty())
                                        .unwrap_or("Saved finding")
                                })
                                .frame(false)
                                .truncate(),
                            )
                            .on_hover_text(&round.summary)
                            .clicked()
                        {
                            self.finding = Some((w.id, round.id));
                        }
                    }
                }

                action
            })
            .inner
    }

    pub(super) fn overlays(
        &mut self,
        ctx: &egui::Context,
        app: &mut App,
        notes: &[NoteView],
    ) -> Option<Action> {
        let mut action = self.desktop_dialogs(ctx, app);
        if (self.preview_open && !self.reader_center) || self.details_open {
            let preview = self.preview_open;
            let mut open = true;
            egui::Window::new(if preview {
                "Source preview"
            } else {
                "Research details"
            })
            .id(egui::Id::new("research-inspector"))
            .open(&mut open)
            .default_width(650.0)
            .max_width((ctx.content_rect().width() - 32.0).max(160.0))
            .default_height(600.0)
            .collapsible(false)
            .show(ctx, |ui| {
                action = self.show(ui, ctx, app, notes);
            });
            if !open {
                self.preview_open = false;
                self.details_open = false;
            }
        }
        let workspace = self
            .selected
            .and_then(|id| app.research_workspace(id))
            .cloned();
        if let Some(w) = &workspace {
            if self.inputs_open {
                let mut open = true;
                egui::Window::new("Collected material")
                    .open(&mut open)
                    .default_width(520.0)
                    .max_width((ctx.content_rect().width() - 32.0).max(160.0))
                    .vscroll(true)
                    .show(ctx, |ui| {
                        if let Some(error) = &self.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        self.flow(ui, app, w, &mut action, FlowView::Inputs);
                    });
                self.inputs_open = open;
            }
            if self.report_open {
                let mut open = true;
                let title = if self.flows.get(&w.id).is_some_and(|d| d.output == 0) {
                    "Continue research"
                } else {
                    "Create report"
                };
                egui::Window::new(title)
                    .id(egui::Id::new("research-context-dialog"))
                    .open(&mut open)
                    .default_width(540.0)
                    .max_width((ctx.content_rect().width() - 32.0).max(160.0))
                    .vscroll(true)
                    .show(ctx, |ui| {
                        if let Some(error) = &self.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        self.flow(ui, app, w, &mut action, FlowView::Report);
                    });
                self.report_open = open;
            }
        }
        if let Some((workspace_id, round_id)) = self.finding {
            let finding = app.research_workspace(workspace_id).and_then(|w| {
                w.rounds
                    .iter()
                    .find(|round| round.id == round_id)
                    .map(|round| (w.title.clone(), round.clone()))
            });
            if let Some((workspace_title, round)) = finding {
                let mut open = true;
                egui::Window::new("Finding")
                    .id(egui::Id::new("research-finding"))
                    .open(&mut open)
                    .default_width(600.0)
                    .max_width((ctx.content_rect().width() - 32.0).max(160.0))
                    .vscroll(true)
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.weak(workspace_title);
                        if !round.question.trim().is_empty() {
                            ui.heading(&round.question);
                            ui.add_space(12.0);
                        }
                        ui.add(egui::Label::new(&round.summary).wrap().selectable(true));
                        if !round.open_questions.trim().is_empty() {
                            ui.add_space(18.0);
                            ui.strong("Open questions");
                            ui.add(
                                egui::Label::new(&round.open_questions)
                                    .wrap()
                                    .selectable(true),
                            );
                        }
                        ui.add_space(18.0);
                        ui.weak(crate::research::capture_time_label(round.captured_at));
                        if ui.button("Research details…").clicked() {
                            self.selected = Some(workspace_id);
                            self.source = None;
                            self.preview_open = false;
                            self.details_open = true;
                            self.finding = None;
                        }
                    });
                if !open {
                    self.finding = None;
                }
            } else {
                self.finding = None;
            }
        }
        if let Some((id, _, note_id)) = self.link {
            egui::Window::new("Research note link")
                .default_width(480.0)
                .max_width((ctx.content_rect().width() - 32.0).max(160.0))
                .vscroll(true)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Note {note_id:?} is retained. {}",
                        if self.link_waiting {
                            "Waiting for the note to save before linking."
                        } else {
                            "The workspace link is not yet saved."
                        }
                    ));
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if self.pending.is_none() && !self.link_waiting {
                        if ui
                            .button("Retry saves and link using current workspace revision")
                            .clicked()
                        {
                            if let Some(w) = app.research_workspace(id) {
                                self.link = Some((id, w.revision, note_id));
                                self.link_waiting = true;
                                app.retry_saves();
                                self.error = None;
                            }
                        }
                        if ui.button("Keep note without linking").clicked() {
                            self.link = None;
                            self.status = "Note retained without a research link".into();
                        }
                    }
                });
        }
        self.dialogs(ctx, app, workspace.as_ref(), notes);
        action
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &mut App,
        notes: &[NoteView],
    ) -> Option<Action> {
        let mut action = None;
        let workspace = self
            .selected
            .and_then(|id| app.research_workspace(id))
            .cloned();
        egui::ScrollArea::vertical()
            .id_salt("research-main")
            .show(ui, |ui| {
                let width = ui.available_width().min(730.0);
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                    ui.vertical(|ui| {
                        ui.set_max_width(width);
                        ui.add_space(20.0);
                        if let Some(workspace) = &workspace {
                            ui.heading(&workspace.title);
                            ui.add_space(14.0);
                            if let Some(source) = self
                                .source
                                .and_then(|id| workspace.sources.iter().find(|s| s.id == id))
                            {
                                ui.heading(&source.title);
                                ui.collapsing("Source details and assessments", |ui| {
                                    ui.label(format!("{:?} · {:?}", source.kind, source.origin));
                                    if let Some(origin) = &source.metadata.origin_label {
                                        ui.label(origin);
                                    }
                                    ui.label(format!(
                                        "Author: {} · Date: {}",
                                        source.metadata.author.as_deref().unwrap_or("Unknown"),
                                        source.metadata.date.as_deref().unwrap_or("Unknown")
                                    ));
                                    ui.horizontal(|ui| {
                                        ui.selectable_value(
                                            &mut self.agent_assessment,
                                            false,
                                            "Human assessment",
                                        );
                                        ui.selectable_value(
                                            &mut self.agent_assessment,
                                            true,
                                            "Agent assessment",
                                        );
                                    });
                                    let human = !self.agent_assessment;
                                    let canonical = if human {
                                        source.human_assessment.as_ref()
                                    } else {
                                        source.agent_assessment.as_ref()
                                    };
                                    let busy = self.busy();
                                    let draft = self
                                        .assessments
                                        .entry((workspace.id, source.id, human))
                                        .or_insert_with(|| AssessmentDraft {
                                            expected: workspace.revision,
                                            value: canonical.cloned().unwrap_or_default(),
                                        });
                                    if canonical == Some(&draft.value) {
                                        draft.expected = workspace.revision;
                                    }
                                    let label = ui.label("Relevance");
                                    ui.text_edit_multiline(&mut draft.value.relevance).labelled_by(label.id);
                                    let label = ui.label("Quality");
                                    ui.text_edit_multiline(&mut draft.value.quality).labelled_by(label.id);
                                    let label = ui.label("Limitations");
                                    ui.text_edit_multiline(&mut draft.value.limitations).labelled_by(label.id);
                                    let command = ui
                                        .add_enabled(!busy, egui::Button::new("Save assessment"))
                                        .clicked()
                                        .then(|| Cmd::AssessSource {
                                            id: workspace.id,
                                            expected: draft.expected,
                                            source_id: source.id,
                                            human,
                                            assessment: draft.value.clone(),
                                        });
                                    if let Some(command) = command {
                                        self.execute(app, command);
                                    }
                                    if ui.button("Discard my assessment edits").clicked() {
                                        self.assessments.remove(&(workspace.id, source.id, human));
                                    }

                                    if self.assessments.get(&(workspace.id, source.id, human))
                                        .is_some_and(|d| d.expected != workspace.revision) {
                                        ui.weak("This research workspace changed while you were editing. Review the latest version before applying your assessment. Your edits are kept; review does not save them.");
                                        ui.collapsing("Latest saved assessment", |ui| {
                                            if let Some(latest) = canonical {
                                                ui.label(format!("Relevance: {}", latest.relevance));
                                                ui.label(format!("Quality: {}", latest.quality));
                                                ui.label(format!("Limitations: {}", latest.limitations));
                                            } else { ui.weak("No saved assessment."); }
                                        });
                                    }
                                    if self
                                        .assessments
                                        .get(&(workspace.id, source.id, human))
                                        .is_some_and(|d| d.expected != workspace.revision)
                                        && ui
                                            .button("Review latest version; keep my assessment")
                                            .clicked()
                                    {
                                        if let Some(draft) = self.assessments.get_mut(&(
                                            workspace.id,
                                            source.id,
                                            human,
                                        )) {
                                            draft.expected = workspace.revision;
                                        }
                                    }
                                    ui.collapsing("Assessment history", |ui| {
                                        let history: Vec<_> = workspace.assessment_history.iter().filter(|event| event.source_id == source.id).collect();
                                        if history.is_empty() { ui.weak("No saved assessments yet."); }
                                        for (index, event) in history.iter().enumerate().rev() {
                                            ui.strong(format!("{} · {}", index + 1, if event.human { "Human" } else { "Agent" }));
                                            ui.label(format!("Relevance: {}", event.assessment.relevance));
                                            ui.label(format!("Quality: {}", event.assessment.quality));
                                            ui.label(format!("Limitations: {}", event.assessment.limitations));
                                        }
                                    });
                                    if ui.button("Prepare source assessment with agent…").clicked() {
                                        action = Some(Action::Prepare { prompt: format!("Assess source {} for the current research question. Read its retained text, distinguish evidence from inference, state relevance, quality, limitations and conflicting sources. Save it as an agent assessment, never as a human assessment.", source.id), evidence: String::new(), external: false });
                                    }
                                    if ui
                                        .add_enabled(
                                            !self.busy(),
                                            egui::Button::new("Remove source"),
                                        )
                                        .clicked()
                                    {
                                        self.execute(
                                            app,
                                            Cmd::RemoveSource {
                                                id: workspace.id,
                                                expected: workspace.revision,
                                                source_id: source.id,
                                            },
                                        );
                                        self.source = None;
                                    }
                                });
                                self.reader_body(ui, ctx, app, workspace, source);
                            } else {
                                ui.add(egui::Label::new(&workspace.question).wrap());
                                self.overview(ui, app, workspace, notes, &mut action);
                            }
                        } else {
                            ui.heading("Research");
                            ui.label("Bring together local sources and a question.");
                            if ui.button("Create workspace").clicked() {
                                self.new_open = true;
                            }
                        }
                        if !self.status.is_empty() {
                            ui.add_space(16.0);
                            ui.label(
                                egui::RichText::new(&self.status)
                                    .small()
                                    .color(theme::muted(ui)),
                            );
                        }
                        if let Some(error) = &self.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        if self.retained_import.is_some() && self.pending.is_none() {
                            if ui
                                .button("Retry retained import against current workspace")
                                .clicked()
                            {
                                self.save_import(app, true);
                            }
                            if ui.button("Discard retained import").clicked() {
                                self.retained_import = None;
                                self.status = "Import discarded".into();
                            }
                        }
                    });
                });
            });
        action
    }
    fn overview(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut App,
        w: &ResearchWorkspace,
        notes: &[NoteView],
        action: &mut Option<Action>,
    ) {
        let busy = self.busy() || app.research_pending(w.id);
        ui.collapsing("Edit workspace", |ui| {
            let draft = self.drafts.entry(w.id).or_insert_with(|| WorkspaceDraft {
                expected: w.revision,
                title: w.title.clone(),
                question: w.question.clone(),
            });
            if draft.title == w.title && draft.question == w.question {
                draft.expected = w.revision;
            }
            ui.label("Title");
            field_single(ui, &mut draft.title, "research-title");
            let label = ui.label("Research question");
            ui.text_edit_multiline(&mut draft.question).labelled_by(label.id);
            let command = ui
                .add_enabled(!busy, egui::Button::new("Save workspace"))
                .clicked()
                .then(|| Cmd::Update {
                    id: w.id,
                    expected: draft.expected,
                    title: draft.title.clone(),
                    question: draft.question.clone(),
                });
            if let Some(command) = command {
                self.execute(app, command);
            }
            if ui.button("Discard workspace edits").clicked() {
                self.drafts.remove(&w.id);
            }

            if self.drafts.get(&w.id).is_some_and(|d| d.expected != w.revision) {
                ui.weak("This workspace changed while you were editing. Review the latest title and question before saving. Your edits are kept.");
                ui.collapsing("Latest saved workspace", |ui| {
                    ui.label(&w.title);
                    ui.label(&w.question);
                });
            }
            if self
                .drafts
                .get(&w.id)
                .is_some_and(|draft| draft.expected != w.revision)
                && ui
                    .button("Review latest version; keep my workspace edits")
                    .clicked()
            {
                if let Some(draft) = self.drafts.get_mut(&w.id) {
                    draft.expected = w.revision;
                }
            }
        });
        ui.add_space(20.0);
        ui.strong("Research notes");
        for id in &w.research_notes {
            if let Some(note) = notes.iter().find(|n| n.note.id == *id && !n.note.trashed) {
                if ui.link(super::note_title(&note.note.path)).clicked() {
                    *action = Some(Action::OpenNote(*id));
                }
            }
        }
        ui.collapsing("Link or create a note", |ui| {
            for note in notes
                .iter()
                .filter(|n| !n.note.trashed && !w.research_notes.contains(&n.note.id))
            {
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(format!("Link {}", super::note_title(&note.note.path))),
                    )
                    .clicked()
                {
                    self.link = Some((w.id, w.revision, note.note.id));
                    self.link_waiting = true;
                    self.status = "Waiting for note persistence before linking…".into();
                }
            }
            ui.label("New note path");
            field_single(ui, &mut self.note_path, "research-note-path");
            if ui
                .add_enabled(
                    !busy && !self.note_path.trim().is_empty(),
                    egui::Button::new("Create and link note"),
                )
                .clicked()
            {
                match app.execute(
                    Command::CreateNote {
                        path: self.note_path.clone(),
                        text: format!("# {}\n\n{}\n", w.title, w.question),
                    },
                    Caller::Local,
                ) {
                    Ok(CommandResult::Note(note)) => {
                        self.link = Some((w.id, w.revision, note.note.id));
                        self.link_waiting = true;
                        self.status = "Saving new note before linking…".into();
                    }
                    Ok(_) => self.error = Some("Unexpected create note response".into()),
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        });
        ui.add_space(16.0);
        if ui.button("Continue with research context…").clicked() {
            let draft = self.flows.entry(w.id).or_insert_with(|| FlowDraft {
                expected: Some(w.revision),
                brief: w.brief.clone(),
                ..Default::default()
            });
            draft.output = 0;
            self.report_open = true;
        }
        self.flow(ui, app, w, action, FlowView::Details);
    }
    pub fn set_conversation(&mut self, capture: ConversationCapture) {
        self.conversation = capture;
    }
    pub fn start_from_conversation(&mut self) {
        if self.new_open
            || !self.new_title.is_empty()
            || !self.new_question.is_empty()
            || !self.new_discussion.is_empty()
            || self.start.is_some()
        {
            self.error = Some("An initial research draft already exists. Resume or discard it before capturing another discussion.".into());
            return;
        }
        if let Some(thread) = &self.conversation.current {
            self.new_title = thread.title.clone();
            self.new_question.clear();
            self.new_discussion = self.conversation.discussion.clone();
            self.new_thread = Some(thread.clone());
            self.new_open = true;
        }
    }
    fn advance_start(&mut self, app: &mut App) {
        let Some(mut start) = self.start.take() else {
            return;
        };
        let Some(w) = start
            .workspace
            .and_then(|id| app.research_workspace(id))
            .cloned()
        else {
            self.start = Some(start);
            return;
        };
        let command = match start.step {
            0 => start.thread.clone().map(|thread| Cmd::AssociateThread {
                id: w.id,
                expected: w.revision,
                thread,
            }),
            1 if !start.discussion.trim().is_empty() => Some(Cmd::AddRound {
                id: w.id,
                expected: w.revision,
                round: ResearchRound {
                    id: Uuid::new_v4(),
                    captured_at: now(),
                    question: w.question.clone(),
                    summary: "Initial discussion context".into(),
                    open_questions: String::new(),
                    discussion: start.discussion.clone(),
                    thread: start.thread.clone(),
                    turn_id: None,
                    artifact_ids: Vec::new(),
                },
            }),
            _ => None,
        };
        if let Some(command) = command {
            self.execute(app, command);
            if self.pending.is_some() {
                start.step += 1;
            }
        } else {
            start.step += 1;
        }
        if start.step <= 2 {
            self.start = Some(start);
        } else {
            self.new_title.clear();
            self.new_question.clear();
            self.new_discussion.clear();
            self.new_thread = None;
        }
    }
    fn flow(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut App,
        w: &ResearchWorkspace,
        action: &mut Option<Action>,
        view: FlowView,
    ) {
        let busy = self.busy() || app.research_pending(w.id);
        if busy {
            ui.disable();
        }
        let mut d = self.flows.remove(&w.id).unwrap_or_else(|| FlowDraft {
            expected: Some(w.revision),
            brief: w.brief.clone(),
            ..Default::default()
        });
        if d.brief == w.brief && !d.checkpoint_dirty() && d.artifact_value.is_empty() {
            d.expected = Some(w.revision);
        }
        let expected = d.expected.unwrap_or(w.revision);
        if view == FlowView::Details {
            ui.add_space(24.0);
            ui.collapsing("Investigation brief", |ui| {
                for (label, value) in [
                    ("Purpose", &mut d.brief.purpose),
                    ("Scope", &mut d.brief.scope),
                    ("Assumptions", &mut d.brief.assumptions),
                    ("Open questions", &mut d.brief.open_questions),
                ] {
                    let label = ui.label(label);
                    ui.add(
                        egui::TextEdit::multiline(value)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(label.id);
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("Save brief"))
                    .clicked()
                {
                    self.execute(
                        app,
                        Cmd::SetBrief {
                            id: w.id,
                            expected,
                            brief: d.brief.clone(),
                        },
                    );
                }
                if ui.button("Discard brief edits").clicked() {
                    d.brief = w.brief.clone();
                }
            });
            ui.collapsing("Conversations", |ui| {
                for thread in &w.threads {
                    let verified = self
                        .conversation
                        .threads
                        .iter()
                        .any(|t| t.same_identity(thread));
                    if ui
                        .add_enabled(verified, egui::Button::new(&thread.title))
                        .on_disabled_hover_text(
                            "Open this conversation's project and refresh its thread list first.",
                        )
                        .clicked()
                    {
                        *action = Some(Action::OpenThread(thread.clone()));
                    }
                }
                for thread in self
                    .conversation
                    .threads
                    .clone()
                    .into_iter()
                    .filter(|t| !w.threads.iter().any(|old| old.same_identity(t)))
                {
                    if ui
                        .add_enabled(
                            !busy,
                            egui::Button::new(format!("Associate {}", thread.title)),
                        )
                        .clicked()
                    {
                        self.execute(
                            app,
                            Cmd::AssociateThread {
                                id: w.id,
                                expected: w.revision,
                                thread,
                            },
                        );
                    }
                }
                if self.conversation.threads.is_empty() {
                    ui.weak("Connect a Codex project to associate its listed conversations.");
                }
            });
            ui.collapsing("Research checkpoints", |ui| {
            for (index, round) in w.rounds.iter().enumerate() {
                ui.push_id(round.id, |ui| {
                    ui.collapsing(format!("{} · {}", index + 1, round.question), |ui| {
                        ui.label(&round.summary);
                        if !round.open_questions.is_empty() {
                            ui.label(format!("Open questions: {}", round.open_questions));
                        }
                        ui.weak(format!("Captured {}", crate::research::capture_time_label(round.captured_at)));
                        if let Some(thread) = &round.thread {
                            ui.weak(format!("{} · {}", thread.provider, thread.title));
                        }
                        if let Some(turn) = &round.turn_id {
                            ui.collapsing("Conversation reference", |ui| { ui.weak(format!("Turn {turn}")); });
                        }
                        if !round.discussion.is_empty() {
                            ui.collapsing("Discussion context · not evidence", |ui| {
                                ui.label(&round.discussion);
                            });
                        }
                        for id in &round.artifact_ids {
                            if let Some(a) = w.artifacts.iter().find(|a| a.id == *id) {
                                ui.label(format!("Artifact: {}", a.title));
                            }
                        }
                    });
                });
            }
            ui.collapsing("New checkpoint", |ui| {
                if ui
                    .add_enabled(
                        !busy
                            && !d.checkpoint_dirty()
                            && self.conversation.current.is_some()
                            && self.conversation.answer.is_some(),
                        egui::Button::new("Capture current assistant answer"),
                    )
                    .on_disabled_hover_text("Save or discard the current checkpoint draft before capturing another answer.")
                    .clicked()
                {
                    d.capture_answer(&self.conversation);
                }
                let label = ui.label("Question / intent");
                ui.text_edit_multiline(&mut d.question).labelled_by(label.id);
                let label = ui.label("Findings / summary");
                ui.text_edit_multiline(&mut d.summary).labelled_by(label.id);
                let label = ui.label("Open questions");
                ui.text_edit_multiline(&mut d.open_questions).labelled_by(label.id);
                if !d.discussion.is_empty() {
                    ui.collapsing("Editable discussion context · not evidence", |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut d.discussion)
                                .desired_rows(5)
                                .char_limit(24_000),
                        );
                    });
                }
                for artifact in &w.artifacts {
                    let mut selected = d.artifact_ids.contains(&artifact.id);
                    if ui.checkbox(&mut selected, &artifact.title).changed() {
                        if selected {
                            d.artifact_ids.push(artifact.id);
                        } else {
                            d.artifact_ids.retain(|id| *id != artifact.id);
                        }
                    }
                }
                if ui
                    .add_enabled(
                        !busy && !d.summary.trim().is_empty(),
                        egui::Button::new("Save immutable checkpoint"),
                    )
                    .clicked()
                {
                    let round = ResearchRound {
                        id: Uuid::new_v4(),
                        captured_at: now(),
                        question: d.question.clone(),
                        summary: d.summary.clone(),
                        open_questions: d.open_questions.clone(),
                        discussion: d.discussion.clone(),
                        thread: d.thread.clone(),
                        turn_id: d.turn_id.clone(),
                        artifact_ids: d.artifact_ids.clone(),
                    };
                    let round_id = round.id;
                    self.execute(
                        app,
                        Cmd::AddRound {
                            id: w.id,
                            expected,
                            round,
                        },
                    );
                    if self.pending.is_some() {
                        self.pending_round = Some((w.id, round_id));
                    }
                }
                if ui.button("Discard checkpoint draft").clicked() {
                    d.question.clear();
                    d.summary.clear();
                    d.open_questions.clear();
                    d.discussion.clear();
                    d.thread = None;
                    d.turn_id = None;
                    d.artifact_ids.clear();
                }
            });
        });
        }
        if view == FlowView::Inputs {
            ui.scope(|ui| {
                for artifact in &w.artifacts {
                    ui.push_id(artifact.id, |ui| {
                        egui::CollapsingHeader::new(&artifact.title)
                            .id_salt(artifact.id)
                            .open((self.artifact_detail == Some(artifact.id)).then_some(true))
                            .show(ui, |ui| {
                                ui.weak(format!(
                                    "{:?} · {} · captured {}",
                                    artifact.kind,
                                    if artifact.managed_path.is_some()
                                        || artifact.markdown.is_some()
                                    {
                                        "retained locally"
                                    } else {
                                        "reference only"
                                    },
                                    crate::research::capture_time_label(artifact.captured_at)
                                ));
                                ui.horizontal(|ui| {
                                    ui.label("Role");
                                    for (role, label) in [
                                        (
                                            crate::research::ArtifactRole::Unclassified,
                                            "Unclassified",
                                        ),
                                        (crate::research::ArtifactRole::Input, "Input"),
                                        (crate::research::ArtifactRole::Output, "Output"),
                                    ] {
                                        if ui
                                            .selectable_label(artifact.role == role, label)
                                            .clicked()
                                        {
                                            self.execute(
                                                app,
                                                Cmd::SetArtifactRole {
                                                    id: w.id,
                                                    expected: w.revision,
                                                    artifact_id: artifact.id,
                                                    role,
                                                },
                                            );
                                        }
                                    }
                                });
                                if let Some(url) = &artifact.url {
                                    ui.hyperlink(url);
                                }
                                if let Some(path) = &artifact.managed_path {
                                    if let Some(vault) = app.vault_directory() {
                                        let full_path = vault.join(path);
                                        let raster = full_path
                                            .extension()
                                            .and_then(|s| s.to_str())
                                            .is_some_and(|ext| {
                                                matches!(
                                                    ext,
                                                    "png"
                                                        | "jpg"
                                                        | "jpeg"
                                                        | "gif"
                                                        | "webp"
                                                        | "bmp"
                                                        | "tif"
                                                        | "tiff"
                                                )
                                            });
                                        if raster
                                            && let Ok(url) = url::Url::from_file_path(&full_path)
                                        {
                                            ui.hyperlink_to("Open retained image", url.as_str());
                                        }
                                        if ui.small_button("Copy retained file path").clicked() {
                                            ui.ctx().copy_text(
                                                full_path.to_string_lossy().into_owned(),
                                            );
                                        }
                                    }
                                }
                                if let Some(thread) = &artifact.thread {
                                    ui.weak(format!("From {}", thread.title));
                                }
                                if let Some(markdown) = &artifact.markdown {
                                    ui.collapsing("Retained Markdown", |ui| {
                                        ui.label(markdown);
                                    });
                                }
                            });
                    });
                }
                ui.add_space(16.0);
                ui.strong("Add retained material");
                ui.scope(|ui| {
                    egui::ComboBox::from_id_salt("artifact-mode")
                        .selected_text(
                            ["Original local file", "Web URL", "Supplied Markdown"]
                                [d.artifact_mode],
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut d.artifact_mode,
                                0,
                                "Original local file (PDF, image or other)",
                            );
                            ui.selectable_value(&mut d.artifact_mode, 1, "Web URL");
                            ui.selectable_value(&mut d.artifact_mode, 2, "Supplied Markdown");
                        });
                    let label = ui.label("Title");
                    ui.text_edit_singleline(&mut d.artifact_title)
                        .labelled_by(label.id);
                    let input_label =
                        ui.label(["Local file path", "HTTP(S) URL", "Markdown"][d.artifact_mode]);
                    if d.artifact_mode == 2 {
                        ui.text_edit_multiline(&mut d.artifact_value)
                            .labelled_by(input_label.id);
                        let label = ui.label("Origin URL (optional)");
                        ui.text_edit_singleline(&mut d.artifact_url)
                            .labelled_by(label.id);
                    } else {
                        ui.text_edit_singleline(&mut d.artifact_value)
                            .labelled_by(input_label.id);
                    }
                    if d.artifact_mode == 1 {
                        ui.checkbox(&mut d.archive, "Retain a Markdown web copy (uses network)");
                    }
                    if ui
                        .add_enabled(
                            !busy
                                && !d.artifact_value.trim().is_empty()
                                && !d.artifact_title.trim().is_empty(),
                            egui::Button::new("Add material"),
                        )
                        .clicked()
                    {
                        let input = match d.artifact_mode {
                            1 => ArtifactInput::Url {
                                url: d.artifact_value.clone(),
                                archive: d.archive,
                            },
                            2 => ArtifactInput::Markdown {
                                markdown: d.artifact_value.clone(),
                                url: (!d.artifact_url.trim().is_empty())
                                    .then(|| d.artifact_url.clone()),
                            },
                            _ => ArtifactInput::LocalFile {
                                path: d.artifact_value.clone(),
                            },
                        };
                        self.execute(
                            app,
                            Cmd::CollectArtifact {
                                id: w.id,
                                expected,
                                title: d.artifact_title.clone(),
                                input,
                                thread: self
                                    .conversation
                                    .current
                                    .clone()
                                    .filter(|t| w.threads.iter().any(|old| old.same_identity(t))),
                                round_id: None,
                            },
                        );
                        if self.pending.is_some() {
                            self.pending_artifact = Some(w.id);
                        }
                    }
                    if ui.button("Discard artifact inputs").clicked() {
                        d.artifact_title.clear();
                        d.artifact_value.clear();
                        d.artifact_url.clear();
                    }
                });
                for (receipt, workspace, artifact) in app.research_artifact_receipts() {
                    if workspace == w.id {
                        ui.label(format!(
                            "Retained locally; registration needs retry: {}",
                            artifact.title
                        ));
                        if ui
                            .add_enabled(
                                !busy,
                                egui::Button::new("Retry registration using current revision"),
                            )
                            .clicked()
                        {
                            self.execute(
                                app,
                                Cmd::RetryArtifact {
                                    id: w.id,
                                    expected: w.revision,
                                    receipt_id: receipt,
                                },
                            );
                        }
                    }
                }
            });
        }
        if expected != w.revision && d.dirty(w) {
            ui.weak("This workspace changed. Review the current workspace above before saving. Review keeps your drafts and does not submit them.");
        }
        if expected != w.revision
            && d.dirty(w)
            && ui
                .button("Review latest version; keep research drafts")
                .clicked()
        {
            d.expected = Some(w.revision);
        }
        if view == FlowView::Report {
            ui.add_space(20.0);
            ui.strong(if d.output == 0 {
                "Continue in conversation"
            } else {
                "Prepare a report in conversation"
            });
            ui.add(
                egui::TextEdit::multiline(&mut d.followup)
                    .hint_text(if d.output == 0 {
                        "Next question…"
                    } else {
                        "What should the report cover?"
                    })
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.checkbox(&mut d.external, "Gather external research (allow network)");
            egui::ComboBox::from_id_salt("research-output")
                .selected_text(if d.output == 0 {
                    "Continue investigation"
                } else {
                    "Ask agent for report synthesis"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.output, 0, "Continue investigation");
                    ui.selectable_value(&mut d.output, 1, "Ask agent for report synthesis");
                });
            let dirty = d.brief != w.brief
                || self
                    .drafts
                    .get(&w.id)
                    .is_some_and(|old| old.title != w.title || old.question != w.question);
            if dirty {
                ui.weak("Save the brief and workspace edits before preparing context.");
            }
            if ui
                .add_enabled(
                    !busy && !dirty,
                    egui::Button::new("Prepare in conversation"),
                )
                .clicked()
            {
                self.execute(
                    app,
                    Cmd::BuildContext {
                        id: w.id,
                        mode: if d.output == 0 {
                            ContextMode::Continue
                        } else {
                            ContextMode::Report
                        },
                        scope: if d.external {
                            SourceScope::ExternalGathering
                        } else {
                            SourceScope::ExistingOnly
                        },
                    },
                );
                if self.error.is_none() {
                    self.prepared_for = Some((w.id, w.revision));
                    self.built_input = Some((d.followup.clone(), d.output));
                    self.prepared_external = d.external;
                    let request = if !d.followup.trim().is_empty() {
                        d.followup.trim()
                    } else if !w.question.trim().is_empty() {
                        w.question.trim()
                    } else {
                        w.title.trim()
                    };
                    let mut prompt = if d.output == 0 {
                        request.to_owned()
                    } else {
                        format!("Write a report on {request}")
                    };
                    prompt.push_str(if d.external {
                        "\n\nUse the attached research context and include external research."
                    } else {
                        "\n\nUse only the attached research context."
                    });
                    *action = Some(Action::Prepare {
                        prompt,
                        evidence: self.prompt.clone(),
                        external: d.external,
                    });
                }
            }
            if ui
                .add_enabled(
                    !busy && !d.followup.is_empty(),
                    egui::Button::new("Discard request draft"),
                )
                .clicked()
            {
                d.followup.clear();
            }
            ui.weak("Preparation preserves existing drafts and does not send. Review the context and use Send in the conversation.");
            if !self.prompt.is_empty() {
                let current = self.prepared_for == Some((w.id, w.revision))
                    && self
                        .built_input
                        .as_ref()
                        .is_some_and(|(q, output)| q == &d.followup && *output == d.output)
                    && d.external == self.prepared_external;
                ui.collapsing("Inspect research context", |ui| {
                    ui.label(&self.prompt);
                });
                if !current {
                    ui.weak("Earlier context snapshot. Prepare again to use current research.");
                }
                if ui.button("Copy prepared context").clicked() {
                    ui.ctx().copy_text(self.prompt.clone());
                }
            }
            ui.collapsing("Save report from saved checkpoints", |ui| {
                ui.weak(
                    "Creates a Markdown report from the workspace’s saved research checkpoints.",
                );
                ui.label("New note path");
                field_single(ui, &mut self.note_path, "research-report-path");
                if ui
                    .add_enabled(
                        !busy && !dirty && !self.note_path.trim().is_empty(),
                        egui::Button::new("Create and link checkpoint report"),
                    )
                    .clicked()
                {
                    match crate::research::report_markdown(w).and_then(|text| {
                        app.execute(
                            Command::CreateNote {
                                path: self.note_path.clone(),
                                text,
                            },
                            Caller::Local,
                        )
                    }) {
                        Ok(CommandResult::Note(note)) => {
                            self.link = Some((w.id, w.revision, note.note.id));
                            self.link_waiting = true;
                            self.status = "Saving report before linking…".into();
                        }
                        Ok(_) => self.error = Some("Unexpected report note response".into()),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            });
        }
        self.flows.insert(w.id, d);
    }

    fn dialogs(
        &mut self,
        ctx: &egui::Context,
        app: &mut App,
        w: Option<&ResearchWorkspace>,
        notes: &[NoteView],
    ) {
        if self.new_open {
            let mut open = true;
            egui::Window::new("New research workspace")
                .default_width(480.0)
                .max_width((ctx.content_rect().width() - 32.0).max(160.0))
                .vscroll(true)
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    if self.busy() {
                        ui.disable();
                    }
                    ui.label("Title");
                    field_single(ui, &mut self.new_title, "research-new-title");
                    let label = ui.label("Question");
                    ui.text_edit_multiline(&mut self.new_question)
                        .labelled_by(label.id);
                    if self.new_thread.is_some() {
                        ui.label("Discussion snapshot · context, not evidence");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.new_discussion)
                                .desired_rows(8)
                                .char_limit(24_000),
                        );
                    }
                    if ui
                        .add_enabled(
                            !self.busy() && !self.new_title.trim().is_empty(),
                            egui::Button::new("Create"),
                        )
                        .clicked()
                    {
                        self.start = Some(StartCapture {
                            thread: self.new_thread.clone(),
                            discussion: self.new_discussion.clone(),
                            workspace: None,
                            step: 0,
                        });
                        self.execute(
                            app,
                            Cmd::Create {
                                title: self.new_title.clone(),
                                question: self.new_question.clone(),
                            },
                        );
                        self.pending_create = self.pending.is_some();
                        if !self.pending_create {
                            self.start = None;
                        }
                    }
                    if ui
                        .add_enabled(
                            !self.busy(),
                            egui::Button::new("Discard new workspace draft"),
                        )
                        .clicked()
                    {
                        self.new_title.clear();
                        self.new_question.clear();
                        self.new_discussion.clear();
                        self.new_thread = None;
                        self.new_open = false;
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                });
            if !open {
                // Keep buffers and close guard until explicitly discarded or saved.
                self.new_open = false;
            }
        }
        if self.add_open {
            let mut open = true;
            egui::Window::new("Add research sources")
                .default_width(480.0)
                .max_width((ctx.content_rect().width() - 32.0).max(160.0))
                .open(&mut open)
                .collapsible(false)
                .vscroll(true)
                .show(ctx, |ui| {
                    if let Some(w) = w {
                        let busy = self.busy() || app.research_pending(w.id);
                        if ui
                            .add_enabled(!busy, egui::Button::new("Choose PDF or text files…"))
                            .clicked()
                        {
                            self.pick_files(ctx, w.id);
                        }
                        ui.collapsing("Enter a local file path", |ui| {
                            field_single(ui, &mut self.import_path, "research-import-path");
                            if ui
                                .add_enabled(
                                    !busy && !self.import_path.trim().is_empty(),
                                    egui::Button::new("Retain and import"),
                                )
                                .clicked()
                            {
                                self.file_queue
                                    .push_back((w.id, PathBuf::from(self.import_path.trim())));
                                self.import_path.clear();
                            }
                        });
                        ui.add_space(12.0);
                        ui.label("Add a snapshot of an existing note");
                        for note in notes.iter().filter(|n| !n.note.trashed) {
                            if ui
                                .add_enabled(
                                    !busy,
                                    egui::Button::new(super::note_title(&note.note.path)),
                                )
                                .clicked()
                            {
                                self.execute(
                                    app,
                                    Cmd::AddNote {
                                        id: w.id,
                                        expected: w.revision,
                                        note_id: note.note.id,
                                    },
                                );
                            }
                        }
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                });
            if !open {
                self.add_open = false;
            }
        }
    }
}

impl ResearchUi {
    pub(super) fn open_references(&mut self, markdown: &str) {
        self.open_references_with_origin(
            markdown,
            self.conversation.current.clone(),
            self.conversation
                .answer
                .as_ref()
                .and_then(|(_, turn)| turn.clone()),
        );
    }
    pub(super) fn open_references_with_origin(
        &mut self,
        markdown: &str,
        thread: Option<ThreadRef>,
        turn_id: Option<String>,
    ) {
        self.discovery_job = None;
        self.discovery_context = None;
        self.collect_workspace = self.selected;
        self.collect_thread = thread;
        self.collect_turn = turn_id;
        self.collect_markdown = markdown.to_owned();
        self.candidates = crate::research_desktop::references(markdown)
            .into_iter()
            .map(|candidate| (true, candidate))
            .collect();
        self.collect_open = true;
    }
    pub(super) fn open_citation(&mut self, app: &App, marker: &str) -> bool {
        let Some((source_id, page)) = parse_citation(marker) else {
            return false;
        };
        for workspace in app.research_workspaces() {
            if let Some(source) = workspace
                .sources
                .iter()
                .find(|source| source.id == source_id)
            {
                let index = page.map_or(Some(0), |number| {
                    source.pages.iter().position(|p| p.number == Some(number))
                });
                let pdf_only = index.is_none()
                    && source.paper.as_ref().is_some_and(|paper| {
                        workspace.artifacts.iter().any(|artifact| {
                            artifact.id == paper.artifact_id
                                && artifact.kind == crate::research::ArtifactKind::Pdf
                        })
                    })
                    && page.is_some();
                let index = if pdf_only {
                    page.unwrap_or(1).saturating_sub(1) as usize
                } else if let Some(index) = index {
                    index
                } else {
                    self.error = Some(
                        "This citation's page is not available in the retained source.".into(),
                    );
                    return false;
                };
                self.requested_pdf_page = if pdf_only {
                    page.map(|number| (source.id, number))
                } else {
                    None
                };
                self.selected = Some(workspace.id);
                self.source = Some(source_id);
                self.reader_page = index;
                self.reader_pdf = pdf_only;
                self.reader_search.clear();
                self.preview_open = true;
                return true;
            }
        }
        self.error = Some("The cited source is not available in this vault.".into());
        false
    }
    pub(super) fn reader_visible(&self) -> bool {
        self.preview_open && self.source.is_some()
    }
    pub(super) fn source_reader(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, app: &mut App) {
        self.reader_center = true;
        let workspace = self
            .selected
            .and_then(|id| app.research_workspace(id))
            .cloned();
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("‹  Back to conversation").frame(false))
                .clicked()
            {
                self.preview_open = false;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::button(
                    ui,
                    theme::Icon::More,
                    "Source details and assessments",
                    true,
                )
                .clicked()
                {
                    self.details_open = true;
                }
            });
        });
        ui.separator();
        if let Some(w) = workspace {
            if let Some(source) = self
                .source
                .and_then(|id| w.sources.iter().find(|s| s.id == id))
            {
                ui.add(egui::Label::new(egui::RichText::new(&source.title).heading()).wrap());
                ui.weak(format!(
                    "{} · {}",
                    if source.selected {
                        "Included in context"
                    } else {
                        "Excluded from context"
                    },
                    if crate::research::source_has_text(source) {
                        "Text available"
                    } else if source.paper.as_ref().is_some_and(|paper| w
                        .artifacts
                        .iter()
                        .any(|a| a.id == paper.artifact_id && a.managed_path.is_some()))
                    {
                        "Original retained · no readable text"
                    } else {
                        "Reference only"
                    }
                ));
                self.reader_body(ui, ctx, app, &w, source);
            } else {
                ui.weak("This source was removed. Return to the conversation.");
            }
        }
    }
    fn ensure_reader(&mut self, ctx: &egui::Context) -> bool {
        if self.reader_worker.is_none() {
            let repaint = ctx.clone();
            match crate::research_reader::ResearchReaderWorker::new(move || {
                repaint.request_repaint()
            }) {
                Ok(worker) => self.reader_worker = Some(worker),
                Err(error) => {
                    self.error = Some(error);
                    return false;
                }
            }
        }
        true
    }
    fn pick_files(&mut self, ctx: &egui::Context, workspace: Uuid) {
        if self.picker_job.is_some() || !self.ensure_reader(ctx) {
            return;
        }
        match self
            .reader_worker
            .as_ref()
            .unwrap()
            .try_submit(crate::research_reader::ReaderRequest::PickFiles)
        {
            Ok(job) => self.picker_job = Some((job, workspace)),
            Err(error) => self.error = Some(error),
        }
    }
    fn poll_desktop(&mut self, ctx: &egui::Context, app: &mut App) {
        // Drops only enqueue local imports; they never send messages or replace the composer.
        let files = ctx.input(|input| input.raw.dropped_files.clone());
        let target = self.drop_target.take();
        if let Some(workspace) = self.selected.filter(|_| {
            target
                .zip(ctx.pointer_hover_pos())
                .is_some_and(|(rect, position)| rect.contains(position))
        }) {
            for file in files {
                if let Some(path) = file.path {
                    self.file_queue.push_back((workspace, path));
                }
            }
        }
        let mut completions = Vec::new();
        if let Some(worker) = &self.reader_worker {
            while let Some(completion) = worker.try_recv() {
                completions.push(completion);
            }
        }
        for completion in completions {
            let picker = self
                .picker_job
                .filter(|(id, _)| *id == completion.request_id);
            let discovery = self.discovery_job == Some(completion.request_id);
            if discovery
                && self.discovery_context.as_ref()
                    != self
                        .collect_workspace
                        .or(self.selected)
                        .map(|id| (id, self.discovery_query.clone()))
                        .as_ref()
            {
                self.discovery_job = None;
                self.discovery_context = None;
                continue;
            }
            let pdf = self
                .pdf_job
                .filter(|(id, _, _)| *id == completion.request_id);
            if picker.is_none() && !discovery && pdf.is_none() {
                continue;
            }
            if picker.is_some() {
                self.picker_job = None;
            }
            if discovery {
                self.discovery_job = None;
                self.discovery_context = None;
            }
            if pdf.is_some() {
                self.pdf_job = None;
            }
            match completion.result {
                Ok(crate::research_reader::ReaderResult::Files(paths)) => {
                    if let Some((_, workspace)) = picker {
                        for path in paths {
                            self.file_queue.push_back((workspace, path));
                        }
                    }
                }
                Ok(crate::research_reader::ReaderResult::Discovery(candidates)) => {
                    if discovery {
                        self.candidates = candidates
                            .into_iter()
                            .map(|candidate| (true, candidate))
                            .collect();
                        self.status = "Search complete · choose references to collect".into();
                    }
                }
                Ok(crate::research_reader::ReaderResult::PdfPage { page, pages, png }) => {
                    if let Some((_, source, requested)) = pdf {
                        if self.source == Some(source) && page == requested {
                            self.pdf_pages = Some((source, pages));
                            self.requested_pdf_page = None;
                            match egui_extras::image::load_image_bytes(&png) {
                                Ok(image) => {
                                    self.pdf_texture = Some((
                                        source,
                                        page,
                                        ctx.load_texture(
                                            format!("research-pdf-{source}-{page}"),
                                            image,
                                            egui::TextureOptions::LINEAR,
                                        ),
                                    ));
                                }
                                Err(error) => {
                                    self.error = Some(format!("Cannot decode PDF page: {error}"))
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    if let Some((_, source, page)) = pdf {
                        self.pdf_failed = Some((source, page));
                    }
                    self.error = Some(error);
                }
            }
        }
        if self.pending.is_some() {
            return;
        }
        if let Some((id, _)) = self.file_queue.front().cloned() {
            if app.research_pending(id) {
                return;
            }
            if let Some(workspace) = app.research_workspace(id) {
                let expected = workspace.revision;
                let paths: Vec<_> = self
                    .file_queue
                    .iter()
                    .filter(|(workspace, _)| *workspace == id)
                    .map(|(_, path)| path.clone())
                    .collect();
                self.file_queue.retain(|(workspace, _)| *workspace != id);
                self.execute(
                    app,
                    Cmd::ImportSources {
                        id,
                        expected,
                        paths: paths
                            .iter()
                            .map(|path| path.to_string_lossy().into_owned())
                            .collect(),
                    },
                );
                if self.pending.is_none() {
                    self.failed_file = Some((id, paths));
                }
            } else {
                self.file_queue.retain(|(workspace, _)| *workspace != id);
                self.error = Some("The import's research workspace no longer exists.".into());
            }
            return;
        }
        if let Some((id, _)) = self.retry_queue.front().copied() {
            if app.research_pending(id) {
                return;
            }
            if let Some(workspace) = app.research_workspace(id) {
                let expected = workspace.revision;
                let job_ids = self
                    .retry_queue
                    .iter()
                    .filter(|(workspace, _)| *workspace == id)
                    .map(|(_, job)| *job)
                    .collect();
                self.retry_queue.retain(|(workspace, _)| *workspace != id);
                self.execute(
                    app,
                    Cmd::RetryAcquisitions {
                        id,
                        expected,
                        job_ids,
                    },
                );
            } else {
                self.retry_queue.retain(|(workspace, _)| *workspace != id);
            }
            return;
        }
        if let Some((id, source_id, input, ocr)) = self.acquire_queue.front().cloned() {
            if app.research_pending(id) {
                return;
            }
            if let Some(workspace) = app.research_workspace(id) {
                let expected = workspace.revision;
                self.acquire_queue.pop_front();
                self.execute(
                    app,
                    Cmd::AcquireSource {
                        id,
                        expected,
                        source_id,
                        input,
                        ocr,
                    },
                );
            } else {
                self.acquire_queue.pop_front();
            }
        }
    }
    fn queue_full_text(&mut self, app: &mut App, w: &ResearchWorkspace) {
        let source_ids: Vec<_> = w
            .sources
            .iter()
            .filter(|source| source.paper.is_none() && source.metadata.url.is_some())
            .map(|source| source.id)
            .collect();
        if source_ids.is_empty() {
            self.status = "No additional linked papers to acquire".into();
            return;
        }
        self.execute(
            app,
            Cmd::AcquireSources {
                id: w.id,
                expected: w.revision,
                source_ids,
            },
        );
    }
    fn acquisition_controls(&mut self, ui: &mut egui::Ui, app: &mut App, w: &ResearchWorkspace) {
        let active = w
            .acquisitions
            .iter()
            .filter(|job| {
                matches!(
                    job.status,
                    crate::research::AcquisitionStatus::Queued
                        | crate::research::AcquisitionStatus::Running
                )
            })
            .count();
        let failed: Vec<_> = w
            .acquisitions
            .iter()
            .filter(|job| matches!(job.status, crate::research::AcquisitionStatus::Failed))
            .collect();
        if active > 0 {
            ui.weak(format!("{active} acquiring full text"));
        }
        if let Some((id, paths)) = self.failed_file.clone() {
            ui.weak(format!("{} files need attention", paths.len()));
            ui.horizontal(|ui| {
                if ui.button("Retry import").clicked() {
                    self.failed_file = None;
                    for path in paths {
                        self.file_queue.push_back((id, path));
                    }
                }
                if ui.button("Discard import").clicked() {
                    self.failed_file = None;
                }
            });
        }

        if !failed.is_empty() {
            if ui
                .add_enabled(!self.busy(), egui::Button::new("Retry failed downloads"))
                .clicked()
            {
                for job in &failed {
                    if !self.retry_queue.iter().any(|(_, id)| *id == job.id) {
                        self.retry_queue.push_back((w.id, job.id));
                    }
                }
            }
            ui.collapsing(format!("{} need attention", failed.len()), |ui| {
                for job in failed {
                    ui.label(
                        w.sources
                            .iter()
                            .find(|s| s.id == job.source_id)
                            .map_or("Removed source", |s| s.title.as_str()),
                    );
                    if let Some(error) = &job.error {
                        ui.small(error);
                    }
                    if ui
                        .add_enabled(
                            !self.busy() && !app.research_pending(w.id),
                            egui::Button::new("Retry"),
                        )
                        .clicked()
                    {
                        self.execute(
                            app,
                            Cmd::RetryAcquisition {
                                id: w.id,
                                expected: w.revision,
                                job_id: job.id,
                            },
                        );
                    }
                }
            });
        }
        if active > 0 {
            ui.collapsing("Downloads", |ui| {
                for job in &w.acquisitions {
                    if matches!(
                        job.status,
                        crate::research::AcquisitionStatus::Queued
                            | crate::research::AcquisitionStatus::Running
                    ) {
                        ui.horizontal(|ui| {
                            ui.label(
                                w.sources
                                    .iter()
                                    .find(|s| s.id == job.source_id)
                                    .map_or("Removed source", |s| s.title.as_str()),
                            );
                            if ui
                                .add_enabled(!self.busy(), egui::Button::new("Cancel"))
                                .clicked()
                            {
                                self.execute(
                                    app,
                                    Cmd::CancelAcquisition {
                                        id: w.id,
                                        expected: w.revision,
                                        job_id: job.id,
                                    },
                                );
                            }
                        });
                    }
                }
            });
        }
    }
    fn reader_body(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &mut App,
        w: &ResearchWorkspace,
        source: &crate::research::ResearchSource,
    ) {
        let count = if self.reader_pdf {
            self.pdf_pages.filter(|(id, _)| *id == source.id).map_or(
                source.pages.len().max(
                    self.requested_pdf_page
                        .filter(|(id, _)| *id == source.id)
                        .map_or(1, |(_, page)| page as usize),
                ),
                |(_, pages)| pages as usize,
            )
        } else if crate::research::source_has_text(source) {
            source.pages.len()
        } else {
            0
        };
        if let Some((id, page)) = self.jump.take() {
            if id == source.id {
                self.reader_page = page
                    .and_then(|number| source.pages.iter().position(|p| p.number == Some(number)))
                    .unwrap_or(0);
            }
        }
        self.reader_page = self.reader_page.min(count.saturating_sub(1));
        let original = source.paper.as_ref().and_then(|paper| {
            w.artifacts
                .iter()
                .find(|artifact| artifact.id == paper.artifact_id)
        });
        let original_path = original.and_then(|artifact| artifact.managed_path.as_ref());
        let pdf_path = original
            .filter(|artifact| artifact.kind == crate::research::ArtifactKind::Pdf)
            .and_then(|artifact| artifact.managed_path.as_ref());
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.reader_pdf, false, "Text");
            if pdf_path.is_some() {
                ui.selectable_value(&mut self.reader_pdf, true, "PDF");
            }
            let previous = ui
                .add_enabled(self.reader_page > 0, egui::Button::new("‹").frame(false))
                .on_hover_text("Previous page");
            previous.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    previous.enabled(),
                    "Previous page",
                )
            });
            if previous.clicked() {
                self.reader_page -= 1;
            }
            let mut page = self.reader_page + 1;
            if ui
                .add(
                    egui::DragValue::new(&mut page)
                        .range(1..=count.max(1))
                        .prefix("Page "),
                )
                .changed()
            {
                self.reader_page = page.saturating_sub(1);
            }
            ui.weak(format!("of {}", count.max(1)));
            let next = ui
                .add_enabled(
                    self.reader_page + 1 < count,
                    egui::Button::new("›").frame(false),
                )
                .on_hover_text("Next page");
            next.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, next.enabled(), "Next page")
            });
            if next.clicked() {
                self.reader_page += 1;
            }
            if let Some(current) = source.pages.get(self.reader_page) {
                let marker = current.number.map_or_else(
                    || format!("[source:{}]", source.id),
                    |n| format!("[source:{} page:{n}]", source.id),
                );
                if theme::button(ui, theme::Icon::Copy, "Copy citation to this page", true)
                    .clicked()
                {
                    ctx.copy_text(marker);
                }
            }
        });
        if let Some(paper) = &source.paper {
            if let Some(error) = &paper.extraction_error {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Original retained · {error}"),
                );
            }
        }
        ui.horizontal(|ui| {
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.reader_search).hint_text("Find in source…"),
            );
            super::name_field(search.clone(), &self.reader_search, "Find in source");
            if search.changed() {
                self.reader_hit = None;
                self.reader_scroll_hit = false;
            }
            if ui
                .add(egui::Button::new("Next match").frame(false))
                .clicked()
                && !self.reader_search.trim().is_empty()
                && !source.pages.is_empty()
            {
                let count = source.pages.len();
                let current = self.reader_page.min(count - 1);
                let previous = self
                    .reader_hit
                    .filter(|(id, page, _)| *id == source.id && *page == current)
                    .map(|(_, _, start)| start);
                let hit = (0..=count).find_map(|offset| {
                    let page = (current + offset) % count;
                    let matches =
                        reader_matches(&source.pages[page].text, self.reader_search.trim());
                    matches
                        .into_iter()
                        .find(|range| offset != 0 || previous.is_none_or(|last| range.start > last))
                        .map(|range| (page, range.start))
                });
                if let Some((page, start)) = hit {
                    self.reader_page = page;
                    self.reader_pdf = false;
                    self.reader_hit = Some((source.id, page, start));
                    self.reader_scroll_hit = true;
                } else {
                    self.status = "No matching text in this source".into();
                }
            }
            ui.menu_button("…", |ui| {
                if let Some(path) = original_path {
                    if ui.button("Open retained original").clicked() {
                        match app.research_asset_path(path) {
                            Ok(path) => ctx.open_url(egui::OpenUrl::new_tab(
                                url::Url::from_file_path(&path)
                                    .map(|url| url.to_string())
                                    .unwrap_or_default(),
                            )),
                            Err(error) => self.error = Some(error.to_string()),
                        }
                        ui.close();
                    }
                }
                if let Some(path) = pdf_path {
                    if ui
                        .add_enabled(
                            !self.busy(),
                            egui::Button::new("Recognise scanned text (OCR)"),
                        )
                        .clicked()
                    {
                        match app.research_asset_path(path) {
                            Ok(path) => self.acquire_queue.push_back((
                                w.id,
                                source.id,
                                ArtifactInput::LocalFile {
                                    path: path.to_string_lossy().into_owned(),
                                },
                                true,
                            )),
                            Err(error) => self.error = Some(error.to_string()),
                        }
                        ui.close();
                    }
                } else if let Some(url) = &source.metadata.url {
                    if ui.button("Get full text").clicked() {
                        self.acquire_queue.push_back((
                            w.id,
                            source.id,
                            ArtifactInput::Url {
                                url: url.clone(),
                                archive: true,
                            },
                            false,
                        ));
                        ui.close();
                    }
                }
            });
        });
        if self.reader_pdf {
            if let Some(path) = pdf_path {
                let physical = source
                    .pages
                    .get(self.reader_page)
                    .and_then(|p| p.number)
                    .unwrap_or((self.reader_page + 1) as u32);
                let cached = self
                    .pdf_texture
                    .as_ref()
                    .is_some_and(|(id, page, _)| *id == source.id && *page == physical);
                if cached {
                    if let Some((_, _, texture)) = &self.pdf_texture {
                        egui::ScrollArea::both()
                            .id_salt(("pdf-reader", source.id))
                            .show(ui, |ui| {
                                ui.add(egui::Image::new(texture).fit_to_exact_size(
                                    texture.size_vec2()
                                        * (ui.available_width() / texture.size_vec2().x).min(1.0),
                                ));
                            });
                    }
                } else if self.pdf_job.is_none()
                    && self.pdf_failed != Some((source.id, physical))
                    && self.ensure_reader(ctx)
                {
                    match app.research_asset_path(path) {
                        Ok(path) => match self.reader_worker.as_ref().unwrap().try_submit(
                            crate::research_reader::ReaderRequest::RenderPdf {
                                path,
                                page: physical,
                            },
                        ) {
                            Ok(job) => self.pdf_job = Some((job, source.id, physical)),
                            Err(error) => self.error = Some(error),
                        },
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                if !cached {
                    if self.pdf_failed == Some((source.id, physical)) {
                        if ui.button("Retry page rendering").clicked() {
                            self.pdf_failed = None;
                        }
                    } else {
                        ui.spinner();
                        ui.weak("Rendering page…");
                    }
                }
            }
        } else if let Some(page) = source
            .pages
            .get(self.reader_page)
            .filter(|_| crate::research::source_has_text(source))
        {
            let query = self.reader_search.trim().to_lowercase();
            if !query.is_empty() {
                ui.weak(format!(
                    "{} matches on this page",
                    reader_matches(&page.text, &query).len()
                ));
            }
            egui::ScrollArea::vertical()
                .id_salt(("source-page", source.id, self.reader_page))
                .show(ui, |ui| {
                    let hit = self
                        .reader_hit
                        .filter(|(id, page, _)| *id == source.id && *page == self.reader_page)
                        .map(|(_, _, start)| start);
                    let job = reader_layout_with_matches(
                        &page.text,
                        ui.available_width(),
                        ui.visuals().text_color(),
                        self.reader_search.trim(),
                        hit,
                    );
                    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                    let response = ui.add(egui::Label::new(galley.clone()).selectable(true));
                    if self.reader_scroll_hit {
                        if let Some(start) = hit {
                            let cursor =
                                egui::text::CCursor::new(page.text[..start].chars().count());
                            let target = galley
                                .pos_from_cursor(cursor)
                                .translate(response.rect.min.to_vec2());
                            ui.scroll_to_rect(target, Some(egui::Align::Center));
                        }
                        self.reader_scroll_hit = false;
                    }
                });
        } else {
            ui.weak("Reference only. Get full text to read this paper here.");
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }
    fn passage_binding(
        &self,
        workspace: &ResearchWorkspace,
    ) -> (Uuid, Revision, String, Vec<Uuid>) {
        (
            workspace.id,
            workspace.revision,
            self.evidence_query.clone(),
            workspace
                .sources
                .iter()
                .filter(|source| source.selected)
                .map(|source| source.id)
                .collect(),
        )
    }
    fn invalidate_passages(&mut self, workspace: &ResearchWorkspace) {
        if self.passage_context.as_ref() != Some(&self.passage_binding(workspace)) {
            self.passages.clear();
            self.passage_context = None;
        }
    }
    fn desktop_dialogs(&mut self, ctx: &egui::Context, app: &mut App) -> Option<Action> {
        let mut action = None;
        let workspace = self
            .selected
            .and_then(|id| app.research_workspace(id))
            .cloned();
        if self.collect_open {
            let workspace = self
                .collect_workspace
                .or(self.selected)
                .and_then(|id| app.research_workspace(id))
                .cloned();
            let mut open = true;
            egui::Window::new("Collect sources").id(egui::Id::new("collect-references")).open(&mut open).default_width(620.0).max_width((ctx.content_rect().width() - 32.0).max(180.0)).vscroll(true).show(ctx, |ui| {
                let Some(w) = &workspace else { ui.weak("Create a research workspace before collecting sources."); return; };
                ui.strong(format!("Into {}", w.title));
                ui.weak("Choose references first. Collection does not send a message or download files.");
                ui.collapsing("Paste links or Markdown", |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.collect_markdown).hint_text("Paper links, a bibliography, or an assistant answer…").desired_rows(4));
                    if ui.button("Find references").clicked() { self.discovery_job = None; self.discovery_context = None; self.candidates = crate::research_desktop::references(&self.collect_markdown).into_iter().map(|candidate| (true, candidate)).collect(); }
                });
                ui.collapsing("Search public papers", |ui| {
                    ui.weak("Search Crossref. Results remain candidates until you add them.");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.discovery_query).hint_text("Research question or title"));
                        if ui.add_enabled(self.discovery_job.is_none() && !self.discovery_query.trim().is_empty(), egui::Button::new("Search")).clicked() && self.ensure_reader(ctx) {
                            match self.reader_worker.as_ref().unwrap().try_submit(crate::research_reader::ReaderRequest::Discover { query: self.discovery_query.clone() }) { Ok(job) => { self.discovery_job = Some(job); self.discovery_context = Some((w.id, self.discovery_query.clone())); }, Err(error) => self.error = Some(error) }
                        }
                    });
                    if self.discovery_job.is_some() { ui.horizontal(|ui| { ui.spinner(); ui.weak("Searching…"); }); }
                });
                ui.collapsing("Desktop tools", |ui| {
                    let ready = crate::research_reader::runtime_readiness();
                    ui.label(format!("File picker: {} · PDF: {} · OCR: {} · Discovery: {}", if ready.picker { "ready" } else { "unavailable" }, if ready.pdf { "ready" } else { "unavailable" }, if ready.ocr { "ready" } else { "unavailable" }, if ready.discovery { "ready" } else { "unavailable" }));
                    ui.weak(ready.guidance);
                });
                ui.separator();
                let existing: std::collections::HashSet<_> = w.sources.iter().filter_map(|source| source.metadata.url.as_deref().and_then(crate::research_desktop::normalize_url)).collect();
                let mut available = 0;
                let mut duplicates = 0;
                for (selected, candidate) in &mut self.candidates {
                    let duplicate = crate::research_desktop::normalize_url(&candidate.url).is_some_and(|url| existing.contains(&url));
                    if duplicate { duplicates += 1; *selected = false; } else { available += 1; }
                    ui.push_id(&candidate.url, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_enabled(!duplicate, egui::Checkbox::without_text(selected));
                            ui.vertical(|ui| {
                                ui.label(&candidate.title);
                                ui.add(egui::Label::new(egui::RichText::new(&candidate.url).small().color(theme::muted(ui))).wrap());
                                if duplicate { ui.weak("Already collected"); }
                            });
                        });
                    });
                }
                if self.candidates.is_empty() { ui.weak("No references found yet. Paste links or search public papers."); }
                if duplicates > 0 { ui.weak(format!("{duplicates} existing references skipped")); }
                let selected: Vec<_> = self.candidates.iter().filter(|(selected, _)| *selected).map(|(_, candidate)| candidate.clone()).collect();
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.busy() && !app.research_pending(w.id) && !selected.is_empty(), egui::Button::new(format!("Add selected ({})", selected.len()))).clicked() {
                        self.execute(app, Cmd::CollectReferences { id: w.id, expected: w.revision, candidates: selected, thread: self.collect_thread.clone(), turn_id: self.collect_turn.clone() });
                    }
                    if ui.add_enabled(!self.busy() && !app.research_pending(w.id) && available > 0, egui::Button::new(format!("Add all ({available})"))).clicked() {
                        let candidates = self.candidates.iter().filter(|(_, candidate)| !crate::research_desktop::normalize_url(&candidate.url).is_some_and(|url| existing.contains(&url))).map(|(_, candidate)| candidate.clone()).collect();
                        self.execute(app, Cmd::CollectReferences { id: w.id, expected: w.revision, candidates, thread: self.collect_thread.clone(), turn_id: self.collect_turn.clone() });
                    }
                    if self.pending.is_some() { ui.spinner(); }
                });
                if let Some(error) = &self.error { ui.colored_label(ui.visuals().error_fg_color, error); }
            });
            self.collect_open = open;
            if !open {
                self.discovery_job = None;
                self.discovery_context = None;
            }
        }
        if self.evidence_open {
            let mut open = true;
            egui::Window::new("Find evidence").open(&mut open).default_width(620.0).max_width((ctx.content_rect().width() - 32.0).max(180.0)).vscroll(true).show(ctx, |ui| {
                let Some(w) = &workspace else { return; };
                ui.weak("Search captured text in included sources. Word matches are not a trust score.");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.evidence_query).hint_text("Question or keywords"));
                    if ui.button("Find passages").clicked() { self.passages = crate::research_desktop::retrieve(w, &self.evidence_query, 8); self.passage_context = Some(self.passage_binding(w)); }
                });
                self.invalidate_passages(w);
                let mut open_marker = None;
                let mut evidence = String::new();
                for passage in &self.passages {
                    ui.separator();
                    let title = w.sources.iter().find(|source| source.id == passage.source_id).map_or("Source", |s| s.title.as_str());
                    if ui.button(format!("{} · {}", title, passage.page.map_or_else(|| "Text".into(), |page| format!("Page {page}")))).clicked() { open_marker = Some(passage.citation.clone()); }
                    ui.add(egui::Label::new(&passage.text).wrap().selectable(true));
                    ui.weak(&passage.citation);
                    evidence.push_str(&format!("{}\n{}\n\n", passage.citation, passage.text));
                }
                if let Some(marker) = open_marker { self.open_citation(app, &marker); }
                if !self.passages.is_empty() {
                    ui.weak(format!("{} passages · {} characters of evidence", self.passages.len(), evidence.chars().count()));
                    if ui.button("Prepare question with these passages").clicked() {
                        // Recheck current owner state, then retrieve afresh: displayed snippets never
                        // become authority after a workspace, inclusion or revision change.
                        if let Some(current) = app.research_workspace(w.id).filter(|current| self.selected == Some(current.id) && self.passage_context.as_ref() == Some(&self.passage_binding(current))) {
                            let current_passages = crate::research_desktop::retrieve(current, &self.evidence_query, 8);
                            let evidence = current_passages.iter().map(|passage| format!("{}\n{}\n\n", passage.citation, passage.text)).collect::<String>();
                            action = Some(Action::Prepare { prompt: self.evidence_query.clone(), evidence, external: false });
                        } else { self.passages.clear(); self.passage_context = None; self.error = Some("Research context changed. Find passages again before preparing evidence.".into()); }
                    }
                }
                ui.collapsing("Evidence gaps and next steps", |ui| {
                    for gap in crate::research_desktop::evidence_gaps(w) { ui.label(gap); }
                    if ui.button("Prepare follow-up research").clicked() {
                        action = Some(Action::Prepare { prompt: format!("Investigate the following evidence gaps. Distinguish source claims from inference; corroborate disagreements and cite retained sources.\n{}", crate::research_desktop::evidence_gaps(w).join("\n")), evidence: String::new(), external: true });
                    }
                });
                ui.weak("Prepare fills the composer for review; it does not send.");
            });
            self.evidence_open = open;
        }
        if self.output_open {
            let workspace = self
                .output_workspace
                .and_then(|id| app.research_workspace(id))
                .cloned();
            let mut open = true;
            egui::Window::new("Create research output").open(&mut open).default_width(600.0).max_width((ctx.content_rect().width() - 32.0).max(180.0)).vscroll(true).show(ctx, |ui| {
                let Some(w) = &workspace else { return; };
                ui.strong(format!("In {}", w.title));
                ui.label("Title");
                if ui.text_edit_singleline(&mut self.output_title).changed() { self.desktop_dirty = true; }
                ui.horizontal_wrapped(|ui| { ui.selectable_value(&mut self.output_kind, 0, "Slides"); ui.selectable_value(&mut self.output_kind, 1, "Infographic"); ui.selectable_value(&mut self.output_kind, 2, "Mind map"); });
                ui.weak("Local HTML / SVG export is ready. Review the content and citations before saving.");
                let mut remove = None;
                for (index, card) in self.output_cards.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.separator();
                        ui.horizontal(|ui| { if ui.add(egui::TextEdit::singleline(&mut card.heading).hint_text("Heading")).changed() { self.desktop_dirty = true; } if ui.small_button("×").on_hover_text("Remove card").clicked() { remove = Some(index); self.desktop_dirty = true; } });
                        if ui.add(egui::TextEdit::multiline(&mut card.body).desired_rows(4).hint_text("Finding or explanation")).changed() { self.desktop_dirty = true; }
                        let mut citations = card.citations.join("\n");
                        if ui.add(egui::TextEdit::multiline(&mut citations).desired_rows(2).hint_text("Source citations, one per line")).changed() { self.desktop_dirty = true; card.citations = citations.lines().filter(|line| !line.trim().is_empty()).map(str::to_owned).collect(); }
                    });
                }
                if let Some(index) = remove { self.output_cards.remove(index); }
                if ui.button("+ Card").clicked() { self.desktop_dirty = true; self.output_cards.push(OutputCard { heading: String::new(), body: String::new(), citations: Vec::new() }); }
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!self.busy() && !app.research_pending(w.id) && !self.output_title.trim().is_empty() && output_ready(self.output_kind, &self.output_cards), egui::Button::new("Save local output")).clicked() {
                        self.execute(app, Cmd::CreateOutput { id: w.id, expected: w.revision, spec: OutputSpec { title: self.output_title.clone(), kind: match self.output_kind { 1 => OutputKind::Infographic, 2 => OutputKind::MindMap, _ => OutputKind::Slides }, cards: self.output_cards.clone(), source_urls: Default::default() } });
                        self.pending_output = self.pending.is_some();
                    }
                    if ui.button("Prepare content with agent…").clicked() {
                        action = Some(Action::Prepare { prompt: format!("Prepare evidence-based content for a {} titled {}. Give concise cards with headings, body text and exact retained-source citations. Do not publish. Use Research CreateOutput only after content is ready.", match self.output_kind { 1 => "infographic", 2 => "mind map", _ => "slide deck" }, self.output_title), evidence: String::new(), external: false });
                    }
                });
                if ui.add_enabled(!self.busy(), egui::Button::new("Discard output draft")).clicked() { self.desktop_dirty = false; self.output_title.clear(); self.output_cards.clear(); self.output_open = false; }
                if let Some(error) = &self.error { ui.colored_label(ui.visuals().error_fg_color, error); }
            });
            if !self.output_title.is_empty() {
                self.output_open = open;
            }
        }
        action
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn output_ready(kind: usize, cards: &[OutputCard]) -> bool {
    !cards.is_empty()
        && cards.iter().all(|card| !card.heading.trim().is_empty())
        && (cards
            .iter()
            .any(|card| !card.body.trim().is_empty() || !card.citations.is_empty())
            || (kind == 2 && cards.len() >= 2))
}

fn reader_matches(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut folded = String::new();
    let mut original = Vec::new();
    for (start, character) in text.char_indices() {
        let lower: String = character.to_lowercase().collect();
        original.extend(std::iter::repeat_n(
            (start, start + character.len_utf8()),
            lower.len(),
        ));
        folded.push_str(&lower);
    }
    let query = query.to_lowercase();
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for (start, matched) in folded.match_indices(&query) {
        let range = original[start].0..original[start + matched.len() - 1].1;
        if ranges.last().is_none_or(|last| last.end <= range.start) {
            ranges.push(range);
        }
    }
    ranges
}
#[cfg(test)]
fn reader_layout(text: &str, width: f32, color: egui::Color32) -> egui::text::LayoutJob {
    reader_layout_with_matches(text, width, color, "", None)
}
fn reader_layout_with_matches(
    text: &str,
    width: f32,
    color: egui::Color32,
    query: &str,
    selected: Option<usize>,
) -> egui::text::LayoutJob {
    let format = egui::TextFormat {
        font_id: egui::FontId::proportional(theme::READING_FONT_SIZE),
        color,
        line_height: Some(theme::READING_LINE_HEIGHT),
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width.min(theme::READING_WIDTH).max(1.0);
    let mut end = 0;
    for range in reader_matches(text, query) {
        job.append(&text[end..range.start], 0.0, format.clone());
        let mut highlight = format.clone();
        highlight.background = if selected == Some(range.start) {
            egui::Color32::from_rgba_unmultiplied(160, 120, 25, 150)
        } else {
            egui::Color32::from_rgba_unmultiplied(160, 120, 25, 65)
        };
        job.append(&text[range.clone()], 0.0, highlight);
        end = range.end;
    }
    job.append(&text[end..], 0.0, format);
    job
}

fn source_citations(text: &str) -> Vec<String> {
    let mut citations = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("[source:") {
        remaining = &remaining[start..];
        let Some(end) = remaining.find(']') else {
            break;
        };
        let marker = &remaining[..=end];
        if parse_citation(marker).is_some() && !citations.iter().any(|existing| existing == marker)
        {
            citations.push(marker.to_owned());
        }
        remaining = &remaining[end + 1..];
        if citations.len() == 32 {
            break;
        }
    }
    citations
}

fn parse_citation(text: &str) -> Option<(Uuid, Option<u32>)> {
    let inner = text.trim().strip_prefix("[source:")?.strip_suffix(']')?;
    let mut parts = inner.split_whitespace();
    let id = Uuid::parse_str(parts.next()?).ok()?;
    let page = match parts.next() {
        Some(p) => Some(p.strip_prefix("page:")?.parse::<u32>().ok()?),
        None => None,
    };
    if page == Some(0) || parts.next().is_some() {
        return None;
    }
    Some((id, page))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_request_transfer_retains_other_research_drafts() {
        let selected = Uuid::new_v4();
        let other = Uuid::new_v4();
        let mut state = ResearchUi {
            selected: Some(selected),
            prepared_for: Some((selected, Revision::fresh())),
            built_input: Some(("Transferred request".into(), 1)),
            report_open: true,
            ..Default::default()
        };
        state.flows.insert(
            selected,
            FlowDraft {
                followup: "Transferred request".into(),
                summary: "Unsaved finding".into(),
                artifact_title: "Retained material draft".into(),
                brief: ResearchBrief {
                    purpose: "Unsaved purpose".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        state.flows.insert(
            other,
            FlowDraft {
                followup: "Other request".into(),
                ..Default::default()
            },
        );
        state.conversation_prepared();
        let selected_draft = &state.flows[&selected];
        assert!(selected_draft.followup.is_empty());
        assert_eq!(selected_draft.summary, "Unsaved finding");
        assert_eq!(selected_draft.artifact_title, "Retained material draft");
        assert_eq!(selected_draft.brief.purpose, "Unsaved purpose");
        assert_eq!(state.flows[&other].followup, "Other request");
        assert!(!state.report_open);

        // A newer local draft was not part of the accepted transfer.
        state.flows.get_mut(&selected).unwrap().followup = "New request".into();
        state.conversation_prepared();
        assert_eq!(state.flows[&selected].followup, "New request");
    }

    #[test]
    fn checkpoint_capture_retains_authored_draft() {
        let capture = ConversationCapture {
            current: Some(ThreadRef {
                provider: "codex".into(),
                project_id: Some("project".into()),
                thread_id: "thread".into(),
                title: "Discussion".into(),
            }),
            discussion: "captured discussion".into(),
            answer: Some(("new answer".into(), Some("turn".into()))),
            ..Default::default()
        };
        let mut draft = FlowDraft {
            summary: "my edited findings".into(),
            discussion: "my retained context".into(),
            ..Default::default()
        };
        assert!(!draft.capture_answer(&capture));
        assert_eq!(draft.summary, "my edited findings");
        assert_eq!(draft.discussion, "my retained context");
        let mut empty = FlowDraft::default();
        assert!(empty.capture_answer(&capture));
        assert_eq!(empty.summary, "new answer");
        assert_eq!(empty.turn_id.as_deref(), Some("turn"));
        assert!(!empty.capture_answer(&capture));
    }
    #[test]
    fn late_cancelled_import_does_not_consume_new_job() {
        let mut state = ResearchUi::default();
        let mut app = App::new();
        let new_job = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        let revision = Revision::fresh();
        state.import = Some((new_job, workspace, revision));
        state.apply_import_completion(
            crate::research_import::ImportCompletion {
                id: Uuid::new_v4(),
                result: Err(crate::research_import::ImportError::Cancelled),
            },
            &mut app,
        );
        assert_eq!(state.import, Some((new_job, workspace, revision)));
        assert!(state.error.is_none());
        state.apply_import_completion(
            crate::research_import::ImportCompletion {
                id: new_job,
                result: Err(crate::research_import::ImportError::InvalidText),
            },
            &mut app,
        );
        assert!(state.import.is_none());
        assert!(state.error.is_some());
    }
    #[test]
    fn discussion_capture_preserves_existing_initial_draft_and_close_guard() {
        let mut ui = ResearchUi::default();
        let first = ThreadRef {
            provider: "codex".into(),
            project_id: Some("project".into()),
            thread_id: "first".into(),
            title: "First investigation".into(),
        };
        ui.conversation.current = Some(first.clone());
        ui.conversation.discussion = "You: investigate this".into();
        ui.start_from_conversation();
        ui.new_open = false;
        assert!(ui.unsaved(&App::new()));
        ui.conversation.current = Some(ThreadRef {
            thread_id: "second".into(),
            title: "Second".into(),
            ..first.clone()
        });
        ui.conversation.discussion = "replacement".into();
        ui.start_from_conversation();
        assert_eq!(ui.new_thread, Some(first));
        assert_eq!(ui.new_discussion, "You: investigate this");
        assert!(ui.error.is_some());
    }
    #[test]
    fn reader_search_highlights_unicode_without_rewriting_evidence_and_output_requires_content() {
        let text = "İSTANBUL café CAFÉ\r\nλ";
        let ranges = reader_matches(text, "CAFÉ");
        assert_eq!(
            ranges
                .iter()
                .map(|range| &text[range.clone()])
                .collect::<Vec<_>>(),
            vec!["café", "CAFÉ"]
        );
        assert_eq!(&text[reader_matches(text, "i")[0].clone()], "İ");
        let job = reader_layout_with_matches(
            text,
            680.0,
            egui::Color32::WHITE,
            "CAFÉ",
            Some(ranges[0].start),
        );
        assert_eq!(job.text.as_bytes(), text.as_bytes());
        assert_eq!(
            job.sections
                .iter()
                .filter(|section| section.format.background != egui::Color32::TRANSPARENT)
                .count(),
            2
        );
        let mut cards = vec![OutputCard {
            heading: "Finding".into(),
            body: String::new(),
            citations: Vec::new(),
        }];
        assert!(!output_ready(0, &cards));
        cards[0].body = "Evidence".into();
        assert!(output_ready(0, &cards));
        cards[0].heading.clear();
        assert!(!output_ready(0, &cards));
    }
    #[test]
    fn reader_layout_preserves_exact_captured_text_and_reading_rhythm() {
        let text = "Exact  spacing\r\n\tUnicode: é → λ\n";
        let job = reader_layout(text, 680.0, egui::Color32::WHITE);
        assert_eq!(job.text.as_bytes(), text.as_bytes());
        assert!(job.sections.iter().all(|section| section.format.line_height
            == Some(theme::READING_LINE_HEIGHT)
            && section.format.font_id.size == theme::READING_FONT_SIZE));
    }
    #[test]
    fn evidence_binding_rejects_workspace_revision_query_and_inclusion_changes() {
        let source_id = Uuid::new_v4();
        let workspace = ResearchWorkspace {
            id: Uuid::new_v4(),
            revision: Revision::fresh(),
            title: "Research".into(),
            question: String::new(),
            sources: vec![crate::research::ResearchSource {
                id: source_id,
                title: "Paper".into(),
                kind: SourceKind::Text,
                origin: crate::research::SourceOrigin::Imported,
                metadata: SourceMetadata::default(),
                pages: vec![SourcePage {
                    number: Some(1),
                    text: "Exact retained evidence".into(),
                }],
                paper: None,
                selected: true,
                human_assessment: None,
                agent_assessment: None,
            }],
            research_notes: vec![],
            brief: ResearchBrief::default(),
            threads: vec![],
            rounds: vec![],
            artifacts: vec![],
            acquisitions: vec![],
            assessment_history: vec![],
        };
        let mut state = ResearchUi {
            selected: Some(workspace.id),
            evidence_query: "evidence".into(),
            ..Default::default()
        };
        for change in 0..4 {
            state.passage_context = Some(state.passage_binding(&workspace));
            state.passages =
                crate::research_desktop::retrieve(&workspace, &state.evidence_query, 8);
            assert!(!state.passages.is_empty());
            let mut changed = workspace.clone();
            match change {
                0 => changed.id = Uuid::new_v4(),
                1 => changed.revision = Revision::fresh(),
                2 => state.evidence_query = "another query".into(),
                _ => changed.sources[0].selected = false,
            }
            state.invalidate_passages(&changed);
            assert!(state.passages.is_empty());
            assert!(state.passage_context.is_none());
            state.evidence_query = "evidence".into();
        }
        let origin = ThreadRef {
            provider: "codex".into(),
            project_id: None,
            thread_id: "old-thread".into(),
            title: "Old answer".into(),
        };
        state.open_references_with_origin(
            "[Paper](https://example.org/paper)",
            Some(origin.clone()),
            Some("old-turn".into()),
        );
        state.conversation.current = None;
        assert_eq!(state.collect_thread, Some(origin));
        assert_eq!(state.collect_turn.as_deref(), Some("old-turn"));
    }
    #[test]
    fn output_seed_keeps_exact_unique_citations_and_transfer_keeps_output_draft() {
        let id = Uuid::new_v4();
        let marker = format!("[source:{id} page:12]");
        assert_eq!(
            source_citations(&format!(
                "Claim {marker}. Repeated {marker}. [source:invalid]"
            )),
            vec![marker]
        );
        let mut state = ResearchUi {
            output_title: "My output".into(),
            desktop_dirty: true,
            ..Default::default()
        };
        state.output_cards.push(OutputCard {
            heading: "Claim".into(),
            body: "Authored content".into(),
            citations: Vec::new(),
        });
        state.conversation_prepared();
        assert!(state.unsaved(&App::new()));
        assert_eq!(state.output_cards[0].body, "Authored content");
        assert_eq!(state.output_title, "My output");
    }
    #[test]
    fn citation_locator_preserves_exact_source_and_page() {
        let id = Uuid::new_v4();
        assert_eq!(
            parse_citation(&format!("[source:{id} page:12]")),
            Some((id, Some(12)))
        );
        assert_eq!(parse_citation(&format!("[source:{id}]")), Some((id, None)));
        for value in [
            format!("[source:{id} page:0]"),
            format!("[source:{id} page:no]"),
            format!("[source:{id} page:1 extra]"),
            "[source:bad]".into(),
        ] {
            assert!(parse_citation(&value).is_none());
        }
    }
}
