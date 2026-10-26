//! Keyboard focus belongs to the view, never to the document ECS.
use eframe::egui;

pub(super) fn block_id(id: impl std::hash::Hash) -> egui::Id {
    egui::Id::new(("reader-block", id))
}

/// F6 transfers focus deliberately, including out of sidebar text fields.
pub(super) fn enter_reader(ctx: &egui::Context, allowed: bool) -> bool {
    allowed
        && !egui::Popup::is_any_open(ctx)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F6))
}

pub(super) fn navigate(ctx: &egui::Context, blocks: &[egui::Id]) {
    if egui::Popup::is_any_open(ctx) {
        return;
    }
    let Some(index) = ctx
        .memory(|m| m.focused())
        .and_then(|id| blocks.iter().position(|b| *b == id))
    else {
        return;
    };
    let target = ctx.input_mut(|input| {
        for (key, target) in [
            (egui::Key::ArrowUp, index.saturating_sub(1)),
            (egui::Key::ArrowDown, (index + 1).min(blocks.len() - 1)),
            (egui::Key::Home, 0),
            (egui::Key::End, blocks.len() - 1),
        ] {
            if input.consume_key(egui::Modifiers::NONE, key) {
                return Some(target);
            }
        }
        None
    });
    if let Some(target) = target {
        ctx.memory_mut(|m| {
            m.move_focus(egui::FocusDirection::None);
            m.request_focus(blocks[target]);
        });
        ctx.request_repaint();
    }
}

pub(super) fn panel_focus(ctx: &egui::Context, id: egui::Id, right: bool, label: &str) {
    let Some(response) = ctx.read_response(id.with("__resize")) else {
        return;
    };
    if !response.has_focus() {
        return;
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, label));
    let inset = egui::vec2(if right { 2.0 } else { -2.0 }, 0.0);
    ctx.layer_painter(response.layer_id).line_segment(
        [
            response.rect.center_top() + inset,
            response.rect.center_bottom() + inset,
        ],
        ctx.style().visuals.selection.stroke,
    );
    ctx.memory_mut(|m| {
        m.set_focus_lock_filter(
            response.id,
            egui::EventFilter {
                horizontal_arrows: true,
                ..Default::default()
            },
        )
    });
    let delta = ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
            16.0
        } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
            -16.0
        } else {
            0.0
        }
    });
    if delta != 0.0 {
        ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        if let Some(mut state) = egui::containers::panel::PanelState::load(ctx, id) {
            let width =
                (state.rect.width() + if right { -delta } else { delta }).clamp(180.0, 360.0);
            if right {
                state.rect.min.x = state.rect.max.x - width;
            } else {
                state.rect.max.x = state.rect.min.x + width;
            }
            ctx.data_mut(|d| d.insert_persisted(id, state));
            ctx.request_repaint();
        }
    }
}

pub(super) fn scroll_focus<R>(
    ctx: &egui::Context,
    scroll: &egui::scroll_area::ScrollAreaOutput<R>,
) {
    let Some(response) = ctx.read_response(scroll.id.with(1usize)) else {
        return;
    };
    if !response.has_focus() {
        return;
    }
    ctx.layer_painter(response.layer_id).rect_stroke(
        response.rect,
        2.0,
        ctx.style().visuals.selection.stroke,
        egui::StrokeKind::Inside,
    );
    ctx.memory_mut(|m| {
        m.set_focus_lock_filter(
            response.id,
            egui::EventFilter {
                vertical_arrows: true,
                ..Default::default()
            },
        )
    });
    let delta = ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
            40.0
        } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            -40.0
        } else {
            0.0
        }
    });
    if delta != 0.0 {
        ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        let mut state = scroll.state.clone();
        state.offset.y = (state.offset.y + delta).clamp(
            0.0,
            (scroll.content_size.y - scroll.inner_rect.height()).max(0.0),
        );
        state.store(ctx, scroll.id);
        ctx.request_repaint();
    }
}

pub(super) fn lock_arrows(response: &egui::Response) {
    if response.has_focus() {
        response.ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_divider_has_keyboard_resize_with_bounds() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("notes");
        let resize = id.with("__resize");
        ctx.data_mut(|d| {
            d.insert_persisted(
                id,
                egui::containers::panel::PanelState {
                    rect: egui::Rect::from_min_size(
                        egui::pos2(48.0, 0.0),
                        egui::vec2(228.0, 760.0),
                    ),
                },
            )
        });
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.memory_mut(|m| m.request_focus(resize));
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.interact(
                    egui::Rect::from_min_size(egui::pos2(276.0, 0.0), egui::vec2(6.0, 760.0)),
                    resize,
                    egui::Sense::drag(),
                );
            });
        });
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            panel_focus(ctx, id, false, "Resize notes pane");
            assert_eq!(
                egui::containers::panel::PanelState::load(ctx, id)
                    .unwrap()
                    .rect
                    .width(),
                244.0
            );
        });
    }

    #[test]
    fn f6_enters_reader_from_sidebar_but_does_not_leave_drafts_or_dialogs() {
        let ctx = egui::Context::default();
        for allowed in [true, false] {
            let input = egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::F6,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                assert_eq!(enter_reader(ctx, allowed), allowed);
                assert_eq!(ctx.input(|i| i.key_pressed(egui::Key::F6)), !allowed);
            });
        }
    }

    #[test]
    fn movement_is_bounded_and_does_not_capture_text_field_keys() {
        let ctx = egui::Context::default();
        let blocks = [block_id(1usize), block_id(2usize), block_id(3usize)];
        let field = egui::Id::new("search-field");
        let step = |focused, key, expected, consumed: bool| {
            ctx.memory_mut(|m| m.request_focus(focused));
            let input = egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                navigate(ctx, &blocks);
                assert_eq!(ctx.memory(|m| m.focused()), expected);
                assert_eq!(ctx.input(|i| i.key_pressed(key)), !consumed);
                egui::CentralPanel::default().show(ctx, |ui| {
                    for n in 1..=3 {
                        let selected =
                            crate::ui::document::editable_block(ui, n, Some(n..n + 1), |ui| {
                                ui.label(format!("Block {n}"));
                            });
                        if key == egui::Key::Enter && expected == Some(block_id(n)) {
                            assert_eq!(selected, Some(n..n + 1));
                        }
                    }
                    ui.add(egui::TextEdit::singleline(&mut String::new()).id(field));
                });
            });
            assert_eq!(ctx.memory(|m| m.focused()), expected);
        };
        step(blocks[0], egui::Key::ArrowDown, Some(blocks[1]), true);
        step(blocks[0], egui::Key::ArrowUp, Some(blocks[0]), true);
        step(blocks[1], egui::Key::End, Some(blocks[2]), true);
        step(blocks[2], egui::Key::Home, Some(blocks[0]), true);
        step(field, egui::Key::ArrowDown, Some(field), false);
        step(blocks[1], egui::Key::Enter, Some(blocks[1]), false);
        step(blocks[1], egui::Key::Escape, None, false);
    }
}
