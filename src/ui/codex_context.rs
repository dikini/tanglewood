//! View-local Codex context. Source snapshots are explicit inputs, never vault paths.
use super::theme;
use crate::conversation::{
    ConnectionInfo, ContextFile, GoalInfo, PlanStep, PromptAttachment, SkillInfo,
};
use crate::{app::App, notes::NoteId};
use eframe::egui;
use std::{collections::HashMap, io::Read, path::PathBuf, sync::mpsc};

const INPUT_LIMIT: usize = 256 * 1024;
const PREVIEW_LIMIT: usize = 8 * 1024 * 1024;
#[derive(Clone, Default)]
pub(super) struct DraftContext {
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
pub(super) enum Action {
    Refresh,
    SetGoal(String),
    PauseGoal,
    ClearGoal,
    DraftPlan,
    DraftSketch,
}
enum FilePayload {
    Text(String),
    Image(egui::ColorImage),
}
enum PreviewPayload {
    Text(String),
    Image(egui::TextureHandle),
}
struct LoadedFile {
    generation: u64,
    path: String,
    attach: bool,
    result: Result<FilePayload, String>,
}
#[derive(Default)]
pub(super) struct ContextUi {
    pub open: bool,
    pub goal: Option<GoalInfo>,
    pub goal_loaded: bool,
    pub plan: Vec<PlanStep>,
    pub plan_text: Option<String>,
    pub files: Vec<ContextFile>,
    pub skills: Vec<SkillInfo>,
    pub connections: Vec<ConnectionInfo>,
    pub capabilities_loaded: bool,
    pub draft: DraftContext,
    cached: HashMap<String, DraftContext>,
    tab: usize,
    goal_dialog: Option<String>,
    file_dialog: Option<String>,
    job: Option<mpsc::Receiver<LoadedFile>>,
    preview: Option<(String, PreviewPayload)>,
    generation: u64,
    job_attach: bool,
    preview_generation: u64,
    error: Option<String>,
}
/// Reported metadata only: opening a card is the sole trigger for reading a file.
fn file_card(ui: &mut egui::Ui, file: &ContextFile, enabled: bool) -> bool {
    let path = std::path::Path::new(&file.path);
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&file.path);
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let kind = if extension.is_empty() {
        file.kind.clone()
    } else {
        extension.to_ascii_uppercase()
    };
    let mut preview = false;
    egui::Frame::new()
        .fill(theme::colors(ui.visuals().dark_mode).raised)
        .stroke(egui::Stroke::new(
            1.0,
            theme::colors(ui.visuals().dark_mode).border,
        ))
        .corner_radius(8)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let (_, icon_rect) = ui.allocate_space(egui::vec2(
                    14.0,
                    ui.text_style_height(&egui::TextStyle::Body),
                ));
                theme::image(ui, theme::Icon::Notes)
                    .fit_to_exact_size(egui::Vec2::splat(14.0))
                    .tint(theme::muted(ui))
                    .paint_at(
                        ui,
                        egui::Rect::from_center_size(icon_rect.center(), egui::Vec2::splat(14.0)),
                    );
                let name = ui
                    .add(egui::Label::new(egui::RichText::new(name).strong()).wrap())
                    .on_hover_text(&file.path);
                name.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &file.path)
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.weak(kind);
                let button = ui.add_enabled(enabled, egui::Button::new("Preview"));
                button.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        enabled,
                        format!("Preview {}", file.path),
                    )
                });
                if button.has_focus() && !button.hovered() {
                    egui::Tooltip::for_widget(&button).show(|ui| {
                        ui.label(&file.path);
                    });
                }
                preview = button.on_hover_text(&file.path).clicked();
            });
        });
    preview
}

impl ContextUi {
    fn file_cards(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        cwd: &std::path::Path,
        limit: usize,
    ) {
        let mut open = None;
        for file in self.files.iter().take(limit) {
            ui.push_id(&file.path, |ui| {
                if file_card(ui, file, !self.loading()) {
                    open = Some(file.path.clone());
                }
            });
        }
        if let Some(path) = open {
            let path = PathBuf::from(path);
            self.open = true;
            self.tab = 2;
            self.load(
                if path.is_absolute() {
                    path
                } else {
                    cwd.join(path)
                },
                false,
                ctx,
            );
        }
    }
    pub fn transcript_files(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        cwd: &std::path::Path,
    ) {
        if self.files.is_empty() {
            return;
        }
        ui.add_space(8.0);
        ui.strong("Files and artifacts");
        ui.weak("Reported in this conversation");
        self.file_cards(ui, ctx, cwd, 3);
        if self.files.len() > 3
            && ui
                .button(format!("View all {} files", self.files.len()))
                .clicked()
        {
            self.open = true;
            self.tab = 2;
        }
    }
    pub fn open_goal_dialog(&mut self, prefill: String) {
        self.goal_dialog = Some(prefill.chars().take(4000).collect());
    }
    pub fn goal_banner(&mut self, ui: &mut egui::Ui, enabled: bool) -> Option<Action> {
        let goal = self.goal.as_ref()?;
        let mut action = None;
        let mut expand = false;
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(12)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("◎")
                        .on_hover_text(format!("Goal · {}", goal.status));
                    let width = (ui.available_width() - 150.0).max(40.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 24.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(egui::Label::new(&goal.objective).truncate())
                                .on_hover_text(&goal.objective);
                        },
                    );
                    let seconds = goal.time_used_seconds;
                    let elapsed = if seconds >= 86400 {
                        format!("{}d {}h", seconds / 86400, seconds / 3600 % 24)
                    } else if seconds >= 3600 {
                        format!("{}h {}m", seconds / 3600, seconds / 60 % 60)
                    } else {
                        format!("{}m {}s", seconds / 60, seconds % 60)
                    };
                    ui.weak(elapsed)
                        .on_hover_text("Time reported by Codex for this goal");
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(if goal.status == "active" {
                                "Ⅱ"
                            } else {
                                "▷"
                            })
                            .frame(false),
                        )
                        .on_hover_text(if goal.status == "active" {
                            "Pause goal"
                        } else {
                            "Resume goal"
                        })
                        .clicked()
                    {
                        action = Some(if goal.status == "active" {
                            Action::PauseGoal
                        } else {
                            Action::SetGoal(goal.objective.clone())
                        });
                    }
                    if theme::button(ui, theme::Icon::Cancel, "Clear goal", enabled).clicked() {
                        action = Some(Action::ClearGoal);
                    }
                    if ui.button("↗").on_hover_text("Show goal details").clicked() {
                        expand = true;
                    }
                });
            });
        if expand {
            self.open = true;
            self.tab = 0;
        }
        ui.add_space(4.0);
        action
    }
    pub fn has_draft(&self) -> bool {
        !self.draft.is_empty() || (self.loading() && self.job_attach)
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
    pub fn loading(&self) -> bool {
        self.job.is_some()
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
        self.preview = None;
    }
    pub fn retire(&mut self, id: &str, current: bool) {
        if !self.cached_draft(id) {
            self.cached.remove(id);
        }
        if current {
            if self.loading() && !self.job_attach {
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
    fn attach(&mut self, attachment: PromptAttachment) {
        if attachment.name.len() > 256 || attachment.name.chars().any(char::is_control) {
            self.error = Some(
                "Attachment name exceeds the supported length or contains control characters."
                    .into(),
            );
            return;
        }
        if self.draft.attachments.len() >= 8
            || self
                .draft
                .attachments
                .iter()
                .map(|a| a.text.len() + a.name.len())
                .sum::<usize>()
                + attachment.text.len()
                + attachment.name.len()
                > INPUT_LIMIT
        {
            self.error=Some("Attachment limit: eight text snapshots, 256 KiB total. Existing attachments retained.".into());
            return;
        }
        self.draft.attachments.push(attachment);
    }
    fn load(&mut self, path: PathBuf, attach: bool, ctx: &egui::Context) {
        if self.loading() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.job = Some(rx);
        self.job_attach = attach;
        let generation = self.generation;
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                use std::os::unix::fs::OpenOptionsExt;
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                    return Err("Select a regular file.".into());
                }
                let raster = matches!(
                    path.extension()
                        .and_then(|s| s.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("png")
                );
                let limit = if !attach && raster {
                    PREVIEW_LIMIT
                } else {
                    INPUT_LIMIT
                };
                let mut bytes = Vec::new();
                file.take(limit as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > limit {
                    return Err(format!("File exceeds the {} KiB limit.", limit / 1024));
                }
                if !attach && raster {
                    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
                        .with_guessed_format()
                        .map_err(|e| e.to_string())?;
                    let format = reader.format().ok_or("Unrecognized image format")?;
                    let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
                    if u64::from(width) * u64::from(height) > 4_000_000 {
                        return Err("Image exceeds the four-million-pixel preview limit.".into());
                    }
                    let image = image::load_from_memory_with_format(&bytes, format)
                        .map_err(|e| e.to_string())?
                        .into_rgba8();
                    Ok(FilePayload::Image(
                        egui::ColorImage::from_rgba_unmultiplied(
                            [width as usize, height as usize],
                            image.as_raw(),
                        ),
                    ))
                } else {
                    String::from_utf8(bytes).map(FilePayload::Text).map_err(|_|"Attach UTF-8 text files in this pass; PNG results can be previewed; SVG is shown as source.".into())
                }
            })();
            let _ = tx.send(LoadedFile {
                generation,
                path: path.to_string_lossy().into_owned(),
                attach,
                result,
            });
            wake.request_repaint();
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else {
            return;
        };
        match job.try_recv() {
            Ok(file) => {
                self.job = None;
                self.job_attach = false;
                if file.generation != self.generation {
                    self.error=Some("File operation canceled because the conversation changed. Select the file again to attach or preview it.".into());
                    return;
                }
                match file.result {
                    Ok(FilePayload::Text(text)) if file.attach => {
                        let name = PathBuf::from(file.path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .chars()
                            .take(256)
                            .collect();
                        self.attach(PromptAttachment { name, text });
                    }
                    Ok(payload) => {
                        self.preview_generation = self.preview_generation.wrapping_add(1);
                        let payload = match payload {
                            FilePayload::Text(text) => PreviewPayload::Text(text),
                            FilePayload::Image(pixels) => PreviewPayload::Image(ctx.load_texture(
                                format!("codex-context-preview-{}", self.preview_generation),
                                pixels,
                                egui::TextureOptions::LINEAR,
                            )),
                        };
                        self.preview = Some((file.path, payload));
                        self.open = true;
                        self.tab = 2;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.job = None;
                self.error = Some("File reader stopped; existing inputs retained.".into());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
    pub fn composer(
        &mut self,
        ui: &mut egui::Ui,
        app: &App,
        selected: Option<NoteId>,
        enabled: bool,
        kimi: bool,
    ) -> Option<Action> {
        let mut action = None;
        let button = theme::button(
            ui,
            theme::Icon::New,
            "Add context or choose a capability",
            enabled && !self.loading(),
        );
        egui::Popup::menu(&button).show(|ui| {
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Attach current note"))
                .clicked()
            {
                if let Some(note) = selected.and_then(|id| app.note(id).ok()) {
                    self.attach(PromptAttachment {
                        name: note.note.path.clone(),
                        text: format!(
                            "Note source snapshot; revision {}\n\n{}",
                            note.note.revision.0, note.note.text
                        ),
                    });
                }
                ui.close();
            }
            ui.menu_button("Attach a note…", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for note in app
                            .notes()
                            .into_iter()
                            .filter(|n| !n.note.trashed)
                            .take(128)
                        {
                            if ui.button(&note.note.path).clicked() {
                                self.attach(PromptAttachment {
                                    name: note.note.path,
                                    text: format!(
                                        "Note source snapshot; revision {}\n\n{}",
                                        note.note.revision.0, note.note.text
                                    ),
                                });
                                ui.close();
                            }
                        }
                    });
            });
            if ui.button("Attach text file…").clicked() {
                self.file_dialog = Some(String::new());
                ui.close();
            }
            if !kimi {
            ui.separator();
            ui.menu_button("Skills / plugins", |ui| {
                if self.skills.is_empty() {
                    ui.weak("No available skills reported.");
                }
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for skill in &self.skills {
                            let selected = self.draft.skills.iter().any(|s| s.path == skill.path);
                            if ui
                                .add_enabled(
                                    skill.enabled,
                                    egui::Button::new(&skill.name).selected(selected),
                                )
                                .on_hover_text(&skill.description)
                                .clicked()
                            {
                                if selected {
                                    self.draft.skills.retain(|s| s.path != skill.path);
                                } else if self.draft.skills.len() < 8 {
                                    self.draft.skills.push(skill.clone());
                                }
                                ui.close();
                            }
                        }
                    });
            });
            if ui.button("Set goal…").clicked() {
                self.goal_dialog = Some(
                    self.goal
                        .as_ref()
                        .map(|g| g.objective.clone())
                        .unwrap_or_default(),
                );
                ui.close();
            }
            if ui.button("Draft a planning request").clicked() {
                action = Some(Action::DraftPlan);
                ui.close();
            }
            if ui.button("Draw a sketch…").on_hover_text("Draft a visual request; review it before sending. Requires a capable model/tool.").clicked() {action=Some(Action::DraftSketch);ui.close();}
            }
            if ui.button("Show context").clicked() {
                self.open = true;
                ui.close();
            }
        });
        action
    }
    pub fn chips(&mut self, ui: &mut egui::Ui) {
        if self.draft.is_empty() {
            return;
        }
        let mut remove_attachment = None;
        let mut remove_skill = None;
        ui.horizontal_wrapped(|ui| {
            for (index, attachment) in self.draft.attachments.iter().enumerate() {
                let remove = ui
                    .add(
                        egui::Button::new(format!("{}  ×", attachment.name))
                            .corner_radius(6)
                            .min_size(egui::vec2(0.0, 28.0)),
                    )
                    .on_hover_text(format!(
                        "Remove {} source snapshot from the next message",
                        attachment.name
                    ));
                remove.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        remove.enabled(),
                        format!(
                            "Remove {} source snapshot from next message",
                            attachment.name
                        ),
                    )
                });
                if remove.clicked() {
                    remove_attachment = Some(index);
                }
            }
            for (index, skill) in self.draft.skills.iter().enumerate() {
                let remove = ui
                    .add(
                        egui::Button::new(format!("${}  ×", skill.name))
                            .corner_radius(6)
                            .min_size(egui::vec2(0.0, 28.0)),
                    )
                    .on_hover_text(format!("Remove {} skill from the next message", skill.name));
                remove.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        remove.enabled(),
                        format!("Remove {} skill from next message", skill.name),
                    )
                });
                if remove.clicked() {
                    remove_skill = Some(index);
                }
            }
        });
        if let Some(index) = remove_attachment {
            self.draft.attachments.remove(index);
        }
        if let Some(index) = remove_skill {
            self.draft.skills.remove(index);
        }
    }
    pub fn panel(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        cwd: &std::path::Path,
        ready: bool,
        kimi: bool,
    ) -> Option<Action> {
        if kimi {
            ui.horizontal(|ui| {
                ui.strong("Context");
                if theme::button(ui, theme::Icon::Cancel, "Close context", true).clicked() {
                    self.open = false;
                }
            });
            ui.add_space(12.0);
            ui.strong("Plan");
            if self.plan.is_empty() {
                ui.weak("No plan reported yet.");
            }
            for step in &self.plan {
                ui.horizontal_wrapped(|ui| {
                    ui.label(match step.status.as_str() {
                        "completed" => "✓",
                        "in_progress" => "●",
                        _ => "○",
                    });
                    ui.label(&step.text);
                });
            }
            return None;
        }
        let mut action = None;
        ui.horizontal(|ui| {
            ui.strong("Context");
            if theme::button(ui, theme::Icon::Connections, "Refresh context", ready).clicked() {
                action = Some(Action::Refresh);
            }
            if theme::button(ui, theme::Icon::Cancel, "Close context", true).clicked() {
                self.open = false;
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (index, label) in ["Goal", "Plan", "Files", "Plugins"].into_iter().enumerate() {
                if ui.selectable_label(self.tab == index, label).clicked() {
                    self.tab = index;
                }
            }
        });
        ui.add_space(12.0);
        egui::ScrollArea::vertical().show(ui,|ui| {match self.tab {
            0=>{
                if let Some(goal)=&self.goal {ui.label(&goal.objective);ui.weak(&goal.status);ui.weak(format!("{} tokens used{}",goal.tokens_used,goal.token_budget.map(|b|format!(" / {b}")).unwrap_or_default()));}
                else {ui.weak(if self.goal_loaded {"No goal set."} else {"Goal not loaded."});}
                if ui.add_enabled(ready,egui::Button::new("Set goal…")).clicked() {self.goal_dialog=Some(self.goal.as_ref().map(|g|g.objective.clone()).unwrap_or_default());}
                if self.goal.is_some() {ui.horizontal(|ui| {if ui.add_enabled(ready,egui::Button::new("Pause")).clicked() {action=Some(Action::PauseGoal);}if ui.add_enabled(ready,egui::Button::new("Clear goal")).clicked() {action=Some(Action::ClearGoal);}});}
            }
            1=>{
                if let Some(text)=&self.plan_text {ui.add(egui::Label::new(text).selectable(true).wrap());}
                if self.plan.is_empty() && self.plan_text.is_none() {ui.weak("No plan reported by Codex yet.");}
                for step in &self.plan {ui.horizontal_wrapped(|ui| {ui.label(match step.status.as_str() {"completed"=>"✓","inProgress"=>"●",_=>"○"});ui.label(&step.text);});}
            }
            2=>{
                if self.files.is_empty() {ui.weak("No files or images reported in loaded history.");}
                self.file_cards(ui, ctx, cwd, usize::MAX);
                if let Some((name,payload))=&mut self.preview {ui.separator();ui.weak(name.as_str());match payload {PreviewPayload::Text(text)=>{ui.add(egui::TextEdit::multiline(text).interactive(false).desired_width(f32::INFINITY));},PreviewPayload::Image(texture)=>{ui.add(egui::Image::new((texture.id(),texture.size_vec2())).max_width(ui.available_width()));}}}
            }
            _=>{
                ui.weak("Available capabilities from this Codex connection");
                if !self.capabilities_loaded {ui.weak("Capabilities not loaded.");}
                for connection in &self.connections {ui.label(&connection.name);ui.weak(format!("{} · {} tools",match connection.auth_status.as_str() {"unsupported"=>"No OAuth","oAuth"=>"OAuth","bearerToken"=>"Token auth","notLoggedIn"=>"Sign-in needed",_=>"Auth unknown"},connection.tools));}
                ui.separator();ui.strong("Skills");for skill in &self.skills {ui.label(&skill.name).on_hover_text(&skill.description);}
                ui.add_space(8.0);ui.weak("Plugin installation and account connections are managed in Codex. This panel reports available skills and MCP connections.");
            }
        }});
        action
    }
    pub fn dialogs(&mut self, ctx: &egui::Context, enabled: bool) -> Option<Action> {
        let mut action = None;
        if let Some(mut text) = self.goal_dialog.take() {
            let mut close = false;
            let modal=egui::Modal::new(egui::Id::new("codex-context-goal")).show(ctx,|ui| {
                ui.set_width(420.0);ui.heading("Set thread goal");ui.label("Set the objective for this thread. An active goal may let Codex continue work until paused or cleared.");
                let field = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(5).desired_width(f32::INFINITY).char_limit(4000));
                super::name_field(field, &text, "Thread goal");
                ui.horizontal(|ui| {if ui.button("Cancel").clicked() {close=true;}if ui.add_enabled(enabled && !text.trim().is_empty(),egui::Button::new("Set goal")).clicked() {action=Some(Action::SetGoal(text.trim().into()));close=true;}});
            });
            if !close && !modal.should_close() {
                self.goal_dialog = Some(text);
            }
        }
        if let Some(mut path) = self.file_dialog.take() {
            let mut close = false;
            let modal = egui::Modal::new(egui::Id::new("codex-attach-file")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.heading("Attach text file");
                let label = ui.label("File path (UTF-8 text, up to 256 KiB)");
                ui.text_edit_singleline(&mut path).labelled_by(label.id);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                    if ui
                        .add_enabled(
                            enabled && !self.loading() && !path.trim().is_empty(),
                            egui::Button::new("Attach snapshot"),
                        )
                        .clicked()
                    {
                        self.load(PathBuf::from(path.trim()), true, ctx);
                        close = true;
                    }
                });
            });
            if !close && !modal.should_close() {
                self.file_dialog = Some(path);
            }
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("Context error")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(error);
                    if ui.button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_and_skills_keep_thread_ownership_without_mutating_source() {
        let mut ui = ContextUi::default();
        let text = "---\ntitle: Original\n---\n\nα `code`\n";
        ui.attach(PromptAttachment {
            name: "Lesson.md".into(),
            text: text.into(),
        });
        ui.switch("A", "B", false);
        assert!(!ui.has_draft());
        assert!(ui.cached_draft("A"));
        ui.attach(PromptAttachment {
            name: "B.md".into(),
            text: "Other".into(),
        });
        ui.switch("B", "A", false);
        assert_eq!(ui.draft.attachments[0].text, text);
        ui.retire("B", false);
        assert!(ui.cached_draft("B"));
        let before = ui.draft.attachments.len();
        ui.attach(PromptAttachment {
            name: "Large.md".into(),
            text: "x".repeat(INPUT_LIMIT),
        });
        assert_eq!(ui.draft.attachments.len(), before);
        assert!(ui.error.is_some());
        let skill = SkillInfo {
            name: "test".into(),
            path: "/trusted/SKILL.md".into(),
            description: "Test".into(),
            enabled: true,
        };
        ui.draft.skills.push(skill.clone());
        assert!(!ui.skills_valid());
        ui.skills.push(skill);
        assert!(ui.skills_valid());
    }
    #[test]
    fn background_file_reader_loads_exact_text_and_rejects_oversize() {
        let root =
            std::env::temp_dir().join(format!("tanglewood-context-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("source.md");
        std::fs::write(&path, "# Exact\n\n尾\n").unwrap();
        let mut ui = ContextUi::default();
        let ctx = egui::Context::default();
        ui.load(path.clone(), true, &ctx);
        assert!(ui.has_draft());
        ui.retire("A", true); // External removal keeps the in-flight snapshot owned/recoverable.
        let wait = |ui: &mut ContextUi| {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while ui.loading() && std::time::Instant::now() < end {
                ui.poll(&ctx);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(!ui.loading());
        };
        wait(&mut ui);
        assert_eq!(ui.draft.attachments[0].text, "# Exact\n\n尾\n");
        std::fs::write(&path, vec![b'x'; INPUT_LIMIT + 1]).unwrap();
        ui.load(path, true, &ctx);
        wait(&mut ui);
        assert_eq!(ui.draft.attachments.len(), 1);
        assert!(ui.error.as_deref().unwrap().contains("exceeds"));
        let png = root.join("result.png");
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255u8; 16]).unwrap();
        }
        std::fs::write(&png, bytes).unwrap();
        ui.load(png, false, &ctx);
        wait(&mut ui);
        assert!(matches!(ui.preview, Some((_, PreviewPayload::Image(_)))));
        let svg = root.join("external.svg");
        std::fs::write(&svg, "<svg><image href='file:///never-read'/></svg>").unwrap();
        ui.load(svg, false, &ctx);
        wait(&mut ui);
        assert!(
            matches!(&ui.preview,Some((_,PreviewPayload::Text(text))) if text.contains("never-read"))
        );
        ui.load(root.join("source.md"), true, &ctx);
        ui.switch("A", "B", false);
        wait(&mut ui);
        assert!(!ui.has_draft());
        assert!(ui.error.as_deref().unwrap().contains("canceled"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
