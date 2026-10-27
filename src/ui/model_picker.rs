//! Temporary model-selection and connection controls over application-owned data.
use eframe::egui::{self, AtomExt};

use crate::{
    conversation::Backend,
    conversation_runtime::ModelKey,
    model_catalog::{Entry, Phase, ProviderKey, Snapshot},
};

use super::theme;

/// View-local state only; catalog and conversation state stay in the application.
#[derive(Default)]
pub(super) struct ModelPickerUi {
    search: String,
    connections_requested: bool,
    connections_open: bool,
    connections_invoker: Option<egui::Id>,
}

/// One reasoning value advertised by the selected model/session.
pub(super) struct EffortOption {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// The application handles these intents, including disconnect confirmation.
pub(super) enum ConnectionAction {
    RefreshAll,
    RetryUnavailable,
    Stop,
    SetAutomatic(bool),
    DisconnectAll,
}

struct ModelRow<'a> {
    key: ModelKey,
    name: &'a str,
    description: &'a str,
}

fn provider_name(key: &ProviderKey) -> &'static str {
    match key.backend {
        #[cfg(feature = "codex")]
        Backend::Codex => "Codex",
        #[cfg(feature = "kimi")]
        Backend::Kimi => "Kimi",
    }
}

fn model_rows(entry: &Entry) -> Vec<ModelRow<'_>> {
    let mut rows = Vec::new();
    match &entry.snapshot {
        #[cfg(feature = "codex")]
        Some(Snapshot::Codex(models)) => {
            for model in models {
                rows.push(ModelRow {
                    key: ModelKey {
                        provider: entry.key.clone(),
                        model: model.model.clone(),
                        option_id: None,
                    },
                    name: &model.name,
                    description: &model.description,
                });
            }
        }
        #[cfg(feature = "kimi")]
        Some(Snapshot::Kimi(options)) => {
            for option in options
                .iter()
                .filter(|option| option.category.as_deref() == Some("model"))
            {
                for value in option.values() {
                    rows.push(ModelRow {
                        key: ModelKey {
                            provider: entry.key.clone(),
                            model: value.value.clone(),
                            option_id: Some(option.id.clone()),
                        },
                        name: &value.name,
                        description: &value.description,
                    });
                }
            }
        }
        None => {}
    }
    rows
}

fn display_name<'a>(entries: &'a [Entry], key: &ModelKey) -> Option<&'a str> {
    entries
        .iter()
        .find(|entry| entry.key == key.provider)
        .and_then(|entry| {
            model_rows(entry)
                .into_iter()
                .find(|row| row.key == *key)
                .map(|row| row.name)
        })
}

fn catalog_status(phase: Phase, has_snapshot: bool) -> &'static str {
    match (phase, has_snapshot) {
        (Phase::Idle, _) => "Not loaded · Open Connections to refresh",
        (Phase::Queued, true) => "Queued · Showing previous models",
        (Phase::Queued, false) => "Queued for discovery…",
        (Phase::Loading, true) => "Refreshing · Showing previous models",
        (Phase::Loading, false) => "Loading models…",
        (Phase::CleaningUp, true) => "Finishing discovery · Showing previous models",
        (Phase::CleaningUp, false) => "Finishing discovery…",
        (Phase::Available, _) => "Available",
        (Phase::Failed, true) => "Unavailable · Showing previous models · Retry in Connections",
        (Phase::Failed, false) => "Unavailable · Retry in Connections",
        (Phase::Paused, true) => "Paused · Showing previous models · Retry in Connections",
        (Phase::Paused, false) => "Paused · Retry in Connections",
    }
}

fn connection_catalog_status(phase: Phase) -> &'static str {
    match phase {
        Phase::Idle => "Not loaded",
        Phase::Queued => "Queued",
        Phase::Loading => "Loading",
        Phase::CleaningUp => "Cleaning up",
        Phase::Available => "Available",
        Phase::Failed => "Unavailable",
        Phase::Paused => "Paused",
    }
}

fn tooltip(row: &ModelRow<'_>) -> String {
    if row.description.trim().is_empty() {
        format!("{}\n{}", row.name, row.key.model)
    } else {
        format!("{}\n{}\n{}", row.name, row.description, row.key.model)
    }
}

fn selectable_row(
    ui: &mut egui::Ui,
    marker: &'static str,
    text: String,
    selected: bool,
) -> egui::Response {
    let mark = marker.atom_size(egui::vec2(18.0, 18.0));
    let label = text.atom_shrink(true);
    ui.add_sized(
        [ui.available_width(), theme::control_size(ui)],
        egui::Button::selectable(selected, (mark, label, egui::Atom::grow())).truncate(),
    )
}

impl ModelPickerUi {
    /// Draw the first-level searchable model list and return a selection intent.
    pub(super) fn model(
        &mut self,
        ui: &mut egui::Ui,
        entries: &[Entry],
        confirmed: Option<&ModelKey>,
        pending: Option<&ModelKey>,
        enabled: bool,
    ) -> Option<ModelKey> {
        let text = if let Some(pending) = pending {
            format!("Preparing {}…", provider_name(&pending.provider))
        } else {
            confirmed
                .and_then(|key| display_name(entries, key))
                .unwrap_or("Model")
                .to_owned()
        };
        let picker = ui
            .add_enabled(
                enabled,
                egui::Button::new(format!("{text} ⌄"))
                    .frame(false)
                    .truncate(),
            )
            .on_hover_text("Choose model for the next message");
        picker.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, format!("Model: {text}"))
        });
        let popup_id = egui::Popup::default_response_id(&picker);
        let search_id = popup_id.with("model-search");
        let was_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        let opening = enabled && picker.clicked() && !was_open;
        let escape = was_open && ui.input(|input| input.key_pressed(egui::Key::Escape));
        if !enabled && was_open {
            egui::Popup::close_id(ui.ctx(), popup_id);
        }
        let available = ui.ctx().content_rect();
        let popup_width = (available.width() - 40.0).clamp(32.0, 332.0);
        let list_height = (available.height() - 160.0).clamp(72.0, 340.0);
        let mut chosen = None;

        egui::Popup::menu(&picker)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(popup_width)
            .show(|ui| {
                ui.set_width(popup_width);
                ui.set_max_width(popup_width);
                ui.label(egui::RichText::new("Choose a model").strong());

                // Consume vertical navigation before TextEdit can use it for the caret.
                let search_focused = ui.memory(|memory| memory.has_focus(search_id));
                let search_direction = if search_focused {
                    ui.input_mut(|input| {
                        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                            Some(1)
                        } else if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                            Some(-1)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .id(search_id)
                        .hint_text("Search models")
                        .desired_width(popup_width),
                );
                #[cfg(not(target_os = "android"))]
                ui.ctx()
                    .accesskit_node_builder(search.id, |node| node.set_label("Search models"));
                if opening {
                    search.request_focus();
                }
                let query = self.search.trim().to_lowercase();
                let mut rendered_rows: Vec<(egui::Response, ModelKey)> = Vec::new();
                egui::ScrollArea::vertical()
                    .id_salt("model-list")
                    .max_height(list_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for entry in entries {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(provider_name(&entry.key)).strong())
                                .on_hover_text(format!(
                                    "Workspace: {}",
                                    entry.key.workspace.display()
                                ));
                            let rows = model_rows(entry);
                            let provider_matches =
                                provider_name(&entry.key).to_lowercase().contains(&query);
                            let mut visible = 0usize;
                            if entry.phase != Phase::Available {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(catalog_status(
                                            entry.phase,
                                            entry.snapshot.is_some(),
                                        ))
                                        .small()
                                        .color(theme::muted(ui)),
                                    )
                                    .wrap(),
                                );
                            }
                            for row in &rows {
                                if !query.is_empty()
                                    && !provider_matches
                                    && !row.name.to_lowercase().contains(&query)
                                    && !row.key.model.to_lowercase().contains(&query)
                                {
                                    continue;
                                }
                                visible += 1;
                                let preparing = pending == Some(&row.key);
                                let selected = !preparing && confirmed == Some(&row.key);
                                let marker = if preparing {
                                    "…"
                                } else if selected {
                                    "✓"
                                } else {
                                    ""
                                };
                                let label = if preparing {
                                    format!("{} · Preparing", row.name)
                                } else {
                                    row.name.to_owned()
                                };
                                let response = selectable_row(ui, marker, label, selected)
                                    .on_hover_text(tooltip(row));
                                if response.clicked() {
                                    chosen = Some(row.key.clone());
                                    ui.close();
                                }
                                rendered_rows.push((response, row.key.clone()));
                            }
                            if entry.phase == Phase::Available && visible == 0 {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(
                                            if query.is_empty() || rows.is_empty() {
                                                "No models offered"
                                            } else {
                                                "No matching models"
                                            },
                                        )
                                        .small()
                                        .color(theme::muted(ui)),
                                    )
                                    .wrap(),
                                );
                            }
                        }
                        if entries.is_empty() {
                            ui.weak("No providers enabled.");
                        }
                        let focused_row = rendered_rows
                            .iter()
                            .position(|(response, _)| response.has_focus());
                        let row_direction = if focused_row.is_some() {
                            ui.input_mut(|input| {
                                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                                    Some(1)
                                } else if input
                                    .consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                                {
                                    Some(-1)
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        };
                        if let Some(direction) = search_direction.or(row_direction) {
                            let len = rendered_rows.len();
                            if len > 0 {
                                let target = match (focused_row, direction) {
                                    (Some(index), 1) => (index + 1) % len,
                                    (Some(index), _) => (index + len - 1) % len,
                                    (None, 1) => 0,
                                    (None, _) => len - 1,
                                };
                                rendered_rows[target].0.request_focus();
                                rendered_rows[target]
                                    .0
                                    .scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });
                ui.separator();
                if ui
                    .add_sized(
                        [ui.available_width(), theme::control_size(ui)],
                        egui::Button::new("Connections…").frame(false),
                    )
                    .clicked()
                {
                    self.connections_requested = true;
                    self.connections_invoker = Some(picker.id);
                    ui.close();
                }
            });
        if escape {
            picker.request_focus();
        }
        chosen
    }

    /// Open the shared Connections surface, including from another shell affordance.
    pub(super) fn open_connections(&mut self) {
        self.connections_open = true;
    }

    /// Consume the request made by the model-list footer.
    pub(super) fn take_connections_requested(&mut self) -> bool {
        std::mem::take(&mut self.connections_requested)
    }

    /// Draw Connections and return at most one explicit application intent.
    pub(super) fn connections(
        &mut self,
        ctx: &egui::Context,
        entries: &[Entry],
        active_counts: &[(ProviderKey, usize)],
        automatic: bool,
    ) -> Option<ConnectionAction> {
        if !self.connections_open {
            return None;
        }
        if self.connections_invoker.is_none() {
            self.connections_invoker = ctx.memory(|memory| memory.focused());
        }
        let viewport = ctx.content_rect();
        let width = (viewport.width() - 48.0).clamp(32.0, 560.0);
        let height = (viewport.height() - 100.0).clamp(72.0, 620.0);
        let position = viewport.center() - egui::vec2(width, height) * 0.5;
        let total: usize = active_counts.iter().map(|(_, count)| *count).sum();
        let mut action = None;
        let mut auto = automatic;
        egui::Window::new("Connections")
            .open(&mut self.connections_open)
            .fixed_pos(position)
            .fixed_size(egui::vec2(width, height))
            .collapsible(false)
            .constrain_to(viewport.shrink(8.0))
            .show(ctx, |ui| {
                ui.set_min_width(0.0);
                ui.set_max_width((width - 24.0).max(16.0));
                egui::ScrollArea::vertical()
                    .id_salt("connections-content")
                    .max_height((height - 48.0).max(24.0))
                    .show(ui, |ui| {
                if ui
                    .checkbox(&mut auto, "Automatically discover model lists")
                    .on_hover_text("Discover models without opening a conversation or sending a message.")
                    .changed()
                {
                    action = Some(ConnectionAction::SetAutomatic(auto));
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Refresh model lists").clicked() {
                        action = Some(ConnectionAction::RefreshAll);
                    }
                    if ui.button("Retry unavailable").clicked() {
                        action = Some(ConnectionAction::RetryUnavailable);
                    }
                    if ui.button("Stop discovery").clicked() {
                        action = Some(ConnectionAction::Stop);
                    }
                });
                ui.separator();
                if entries.is_empty() {
                            ui.weak("No providers enabled.");
                        }
                        for entry in entries {
                            ui.add_space(4.0);
                            ui.strong(provider_name(&entry.key));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(entry.key.workspace.display().to_string())
                                        .small()
                                        .color(theme::muted(ui)),
                                )
                                .wrap(),
                            );
                            ui.label(format!("Model catalog: {}", connection_catalog_status(entry.phase)));
                            let count = active_counts
                                .iter()
                                .filter(|(key, _)| key == &entry.key)
                                .map(|(_, count)| *count)
                                .sum::<usize>();
                            ui.label(format!("Connected conversations: {count}"));
                            if let Some(error) = &entry.error {
                                ui.add(egui::Label::new(error).wrap());
                            }
                            if let Some(issue) = &entry.cleanup_issue {
                                let message = if entry.snapshot.is_some() {
                                    "Models loaded; temporary-session cleanup failed."
                                } else {
                                    "Temporary-session cleanup needs attention."
                                };
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(message)
                                            .color(theme::colors(ui.visuals().dark_mode).warning),
                                    )
                                    .wrap(),
                                );
                                ui.add(egui::Label::new(issue).wrap());
                            }
                            ui.add_space(8.0);
                        }
                if total > 0 {
                    ui.separator();
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "Disconnecting ends observation and may interrupt an owned local agent process. Remote continuation depends on the provider.",
                            )
                            .small()
                            .color(theme::muted(ui)),
                        )
                        .wrap(),
                    );
                    if ui
                        .button(format!("Disconnect all conversations ({total})…"))
                        .on_hover_text("Review the effect before disconnecting.")
                        .clicked()
                    {
                        action = Some(ConnectionAction::DisconnectAll);
                    }
                }
                    });
            });
        if !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.connections_open = false;
        }
        if !self.connections_open {
            if let Some(invoker) = self.connections_invoker.take() {
                ctx.memory_mut(|memory| memory.request_focus(invoker));
            }
        }
        action
    }
}

/// Draw the separate advertised reasoning menu; absent options create no control.
pub(super) fn effort(
    ui: &mut egui::Ui,
    current: Option<&str>,
    options: &[EffortOption],
    enabled: bool,
) -> Option<String> {
    if options.is_empty() {
        return None;
    }
    let label = options
        .iter()
        .find(|option| Some(option.id.as_str()) == current)
        .map(|option| option.name.as_str())
        .unwrap_or("Reasoning");
    let picker = ui
        .add_enabled(
            enabled,
            egui::Button::new(format!("{label} ⌄"))
                .frame(false)
                .truncate(),
        )
        .on_hover_text("Choose reasoning effort");
    picker.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            enabled,
            format!("Reasoning: {label}"),
        )
    });
    let popup_id = egui::Popup::default_response_id(&picker);
    let was_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let escape = was_open && ui.input(|input| input.key_pressed(egui::Key::Escape));
    if !enabled && was_open {
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    let width = (ui.ctx().content_rect().width() - 40.0).clamp(32.0, 240.0);
    let mut choice = None;
    egui::Popup::menu(&picker).width(width).show(|ui| {
        ui.set_width(width);
        ui.set_max_width(width);
        for option in options {
            let selected = Some(option.id.as_str()) == current;
            let response = selectable_row(
                ui,
                if selected { "✓" } else { "" },
                option.name.clone(),
                selected,
            );
            let response = if option.description.trim().is_empty() {
                response.on_hover_text(&option.name)
            } else {
                response.on_hover_text(&option.description)
            };
            if response.clicked() {
                choice = Some(option.id.clone());
                ui.close();
            }
        }
    });
    if escape {
        picker.request_focus();
    }
    choice
}

#[cfg(all(test, feature = "kimi"))]
mod tests {
    use super::*;
    use crate::{
        kimi::KimiConfig,
        model_catalog::{ModelCatalog, ProviderConfig},
    };

    #[test]
    fn connections_stays_inside_viewport_after_resize() {
        let ctx = egui::Context::default();
        let mut picker = ModelPickerUi::default();
        picker.open_connections();
        let workspace = std::path::PathBuf::from(
            "/tmp/long-fixture-workspace/research-project-with-a-very-long-name/another-directory",
        );
        let config = ProviderConfig::Kimi(KimiConfig {
            executable: workspace.join("kimi"),
            cwd: workspace.clone(),
            mcp_bridge: workspace.join("bridge"),
            socket: workspace.join("socket"),
            resume_session: None,
        });
        let key = config.key();
        let mut catalog = ModelCatalog::default();
        catalog.enable(config);
        let render = |ctx: &egui::Context, picker: &mut ModelPickerUi, size: egui::Vec2| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                picker.connections(ctx, &catalog.entries, &[(key.clone(), 1)], true);
            });
        };
        render(&ctx, &mut picker, egui::vec2(1200.0, 800.0));
        render(&ctx, &mut picker, egui::vec2(390.0, 780.0));
        render(&ctx, &mut picker, egui::vec2(390.0, 780.0));
        let rect = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("Connections")))
            .expect("connections area");
        assert!(rect.left() >= 0.0 && rect.top() >= 0.0, "{rect:?}");
        assert!(rect.right() <= 390.0 && rect.bottom() <= 780.0, "{rect:?}");
    }
}
