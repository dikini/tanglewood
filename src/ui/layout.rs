//! View-local responsive policy; never owns source, drafts or application records.
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WidthClass {
    Compact,
    Regular,
    Expanded,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pane {
    Navigation,
    Outcomes,
    Context,
    Conversations,
}
impl Pane {
    pub fn title(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Outcomes => "Outcomes",
            Self::Context => "Context",
            Self::Conversations => "Conversations",
        }
    }
    pub fn right(self) -> bool {
        matches!(self, Self::Outcomes | Self::Context)
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Plan {
    pub class: WidthClass,
    pub rail: f32,
    pub left: bool,
    pub right: bool,
}
impl Plan {
    pub fn compact(self) -> bool {
        self.class == WidthClass::Compact
    }
    pub fn gutter(self) -> i8 {
        match self.class {
            WidthClass::Compact => 16,
            WidthClass::Regular => 24,
            WidthClass::Expanded => 48,
        }
    }
}
#[derive(Default)]
pub(super) struct Responsive {
    class: Option<WidthClass>,
    pub overlay: Option<Pane>,
    pub focus_before_frame: Option<egui::Id>,
    return_focus: Option<egui::Id>,
    pub restore_focus: Option<egui::Id>,
    pub newly_opened: bool,
    pub left_rect: Option<egui::Rect>,
    pub right_rect: Option<egui::Rect>,
    pub previous: Option<Plan>,
}
impl Responsive {
    pub fn plan(
        &mut self,
        width: f32,
        touch: bool,
        left_width: Option<f32>,
        right_width: Option<f32>,
    ) -> Plan {
        let width = width.max(0.0);
        let raw = if width < 600.0 {
            WidthClass::Compact
        } else if width < 1200.0 {
            WidthClass::Regular
        } else {
            WidthClass::Expanded
        };
        let class = match (self.class, raw) {
            (Some(WidthClass::Compact), WidthClass::Regular) if width < 624.0 => {
                WidthClass::Compact
            }
            (Some(WidthClass::Regular), WidthClass::Expanded) if width < 1224.0 => {
                WidthClass::Regular
            }
            _ => raw,
        };
        self.class = Some(class);
        let rail = if class == WidthClass::Compact {
            0.0
        } else if touch {
            64.0
        } else {
            48.0
        };
        let left =
            class != WidthClass::Compact && left_width.is_some_and(|w| rail + w + 480.0 <= width);
        let right = class == WidthClass::Expanded
            && right_width.is_some_and(|w| {
                rail + if left { left_width.unwrap_or(0.0) } else { 0.0 } + w + 480.0 <= width
            });
        Plan {
            class,
            rail,
            left,
            right,
        }
    }
    pub fn open(&mut self, pane: Pane) {
        if self.overlay == Some(pane) {
            return;
        }
        if self.overlay.is_none() {
            self.return_focus = self.focus_before_frame;
        }
        self.overlay = Some(pane);
        self.newly_opened = true;
    }
    pub fn toggle(&mut self, pane: Pane) {
        if self.overlay == Some(pane) {
            self.close();
        } else {
            self.open(pane);
        }
    }
    pub fn close(&mut self) {
        self.overlay = None;
        self.restore_focus = self.return_focus.take();
        self.newly_opened = false;
    }
}

pub(super) struct Pullout<R> {
    pub inner: R,
    pub close: bool,
    pub switch: Option<Pane>,
    pub dock: bool,
    pub focus: egui::Id,
}
pub(super) fn pullout<R>(
    ctx: &egui::Context,
    pane: Pane,
    width: f32,
    panes: &[Pane],
    can_dock: bool,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Pullout<R> {
    let viewport = ctx.content_rect();
    let width = width
        .min(if viewport.width() < 360.0 {
            viewport.width()
        } else {
            viewport.width() - 32.0
        })
        .max(1.0);
    let mut switch = None;
    let mut close = false;
    let mut dock = false;
    let mut focus = None;
    let area = egui::Modal::default_area(egui::Id::new("shell-pullout")).anchor(
        if pane.right() {
            egui::Align2::RIGHT_TOP
        } else {
            egui::Align2::LEFT_TOP
        },
        egui::Vec2::ZERO,
    );
    let result = egui::Modal::new(egui::Id::new("shell-pullout"))
        .area(area)
        .backdrop_color(egui::Color32::from_black_alpha(55))
        .frame(
            egui::Frame::new()
                .fill(super::theme::colors(ctx.style().visuals.dark_mode).navigation)
                .inner_margin(16),
        )
        .show(ctx, |ui| {
            ui.set_width((width - 32.0).max(1.0));
            ui.set_height((viewport.height() - 32.0).max(1.0));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("pullout-selector")
                    .selected_text(pane.title())
                    .width((width - 146.0).max(60.0))
                    .show_ui(ui, |ui| {
                        for candidate in panes {
                            if ui
                                .selectable_label(*candidate == pane, candidate.title())
                                .clicked()
                            {
                                switch = Some(*candidate);
                            }
                        }
                    });
                if can_dock
                    && ui
                        .add(egui::Button::new("⇥").frame(false))
                        .on_hover_text("Dock pane")
                        .clicked()
                {
                    dock = true;
                }
                let button =
                    super::theme::button(ui, super::theme::Icon::Cancel, "Close pane", true);
                focus = Some(button.id);
                close = button.clicked();
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt(("pullout-scroll", pane.title()))
                .auto_shrink([false, false])
                .show(ui, contents)
                .inner
        });
    let close = close || result.should_close();
    Pullout {
        inner: result.inner,
        close,
        switch,
        dock,
        focus: focus.expect("pullout header"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn responsive_budgets_hysteresis_and_overlay_preserve_preferences() {
        let mut state = Responsive::default();
        assert!(state.plan(1440.0, false, Some(228.0), Some(260.0)).right);
        let regular = state.plan(900.0, false, Some(228.0), Some(260.0));
        assert!(regular.left);
        assert!(!regular.right);
        assert!(!state.plan(650.0, false, Some(228.0), None).left);
        assert!(state.plan(599.0, false, Some(228.0), Some(260.0)).compact());
        assert!(state.plan(610.0, false, Some(228.0), None).compact());
        assert!(!state.plan(624.0, false, Some(228.0), None).compact());
        assert!(!state.plan(1200.0, false, Some(228.0), Some(260.0)).right);
        assert!(state.plan(1224.0, false, Some(228.0), Some(260.0)).right);
        assert!(!state.plan(1224.0, false, Some(360.0), Some(360.0)).right);
        state.focus_before_frame = Some(egui::Id::new("composer"));
        state.open(Pane::Navigation);
        state.open(Pane::Outcomes);
        assert_eq!(state.overlay, Some(Pane::Outcomes));
        state.close();
        assert_eq!(state.restore_focus, Some(egui::Id::new("composer")));
    }
}
