//! Small application theme over native egui controls and trusted bundled assets.
use eframe::egui;

pub(super) const READING_WIDTH: f32 = 730.0;
pub(super) const READING_FONT_SIZE: f32 = 16.0;
pub(super) const READING_LINE_HEIGHT: f32 = 26.0;
pub(super) const HEADER_HEIGHT: f32 = 56.0;
pub(super) const SIDEBAR_WIDTH: f32 = 228.0;
pub(super) const CONTEXT_WIDTH: f32 = 260.0;
pub(super) const PARAGRAPH_GAP: f32 = 16.0;
pub(super) const CODE_FONT_SIZE: f32 = 13.0;
pub(super) const CODE_LINE_HEIGHT: f32 = 21.0;

/// Semantic presentation tokens shared by shell, notes and conversation.
pub(super) struct Colors {
    pub content: egui::Color32,
    pub navigation: egui::Color32,
    pub raised: egui::Color32,
    pub hover: egui::Color32,
    pub selected: egui::Color32,
    pub text: egui::Color32,
    pub secondary: egui::Color32,
    pub border: egui::Color32,
    pub accent: egui::Color32,
    pub success: egui::Color32,
    pub warning: egui::Color32,
    pub error: egui::Color32,
}
pub(super) fn colors(dark: bool) -> Colors {
    let gray = |d, l| egui::Color32::from_gray(if dark { d } else { l });
    let rgb = |d: [u8; 3], l: [u8; 3]| {
        let [r, g, b] = if dark { d } else { l };
        egui::Color32::from_rgb(r, g, b)
    };
    Colors {
        content: gray(25, 250),
        navigation: gray(22, 242),
        raised: gray(34, 255),
        hover: gray(43, 229),
        selected: gray(48, 222),
        text: gray(223, 41),
        secondary: gray(170, 97),
        border: gray(48, 222),
        accent: rgb([138, 180, 248], [36, 93, 179]),
        success: rgb([131, 201, 149], [36, 106, 55]),
        warning: rgb([229, 186, 115], [121, 81, 0]),
        error: rgb([239, 153, 153], [163, 34, 34]),
    }
}

/// Input density is independent of OS. Fonts and user zoom are untouched.
pub(super) fn set_touch_density(ctx: &egui::Context, touch: bool) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("touch-density"), touch));
    ctx.style_mut(|style| {
        style.spacing.interact_size = egui::Vec2::splat(if touch { 48.0 } else { 34.0 });
    });
}
fn touch(ui: &egui::Ui) -> bool {
    ui.ctx().data(|data| {
        data.get_temp::<bool>(egui::Id::new("touch-density"))
            .unwrap_or(false)
    })
}
pub(super) fn control_size(ui: &egui::Ui) -> f32 {
    if touch(ui) { 48.0 } else { 34.0 }
}

pub(super) fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (
            "ui-sans",
            include_bytes!("../../assets/ui/fonts/dejavu-2.37/DejaVuSans.ttf").as_slice(),
        ),
        (
            "ui-serif",
            include_bytes!("../../assets/ui/fonts/dejavu-2.37/DejaVuSerif.ttf").as_slice(),
        ),
        (
            "ui-mono",
            include_bytes!("../../assets/ui/fonts/dejavu-2.37/DejaVuSansMono.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), egui::FontData::from_static(bytes).into());
    }
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "ui-sans".into());
    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .insert(0, "ui-mono".into());
    fonts.families.insert(
        egui::FontFamily::Name("document-serif".into()),
        vec!["ui-serif".into(), "ui-sans".into()],
    );
    ctx.set_fonts(fonts);
    palette(ctx);
    egui_extras::install_image_loaders(ctx);
}

pub(super) fn background(ui: &egui::Ui) -> egui::Color32 {
    colors(ui.visuals().dark_mode).content
}
pub(super) fn sidebar(ui: &egui::Ui) -> egui::Color32 {
    colors(ui.visuals().dark_mode).navigation
}
pub(super) fn muted(ui: &egui::Ui) -> egui::Color32 {
    colors(ui.visuals().dark_mode).secondary
}
pub(super) fn palette(ctx: &egui::Context) {
    ctx.style_mut(|style| {
        let c = colors(style.visuals.dark_mode);
        style.visuals.override_text_color = Some(c.text);
        style.visuals.panel_fill = c.navigation;
        style.visuals.window_fill = c.raised;
        style.visuals.extreme_bg_color = c.raised;
        style.visuals.faint_bg_color = c.hover;
        style.visuals.hyperlink_color = c.accent;
        style.visuals.warn_fg_color = c.warning;
        style.visuals.error_fg_color = c.error;
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, c.border);
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, c.border);
        style.visuals.widgets.hovered.weak_bg_fill = c.hover;
        style.visuals.widgets.hovered.bg_fill = c.hover;
        style.visuals.widgets.active.weak_bg_fill = c.selected;
        style.visuals.widgets.active.bg_fill = c.selected;
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, c.accent);
        style.visuals.selection.bg_fill = c.selected;
        style.visuals.selection.stroke = egui::Stroke::new(1.0, c.text);
        style.visuals.window_corner_radius = egui::CornerRadius::same(12);
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(7.0, 7.0);
        style.spacing.interact_size = egui::Vec2::splat(34.0);
        for (role, size) in [
            (egui::TextStyle::Body, 14.0),
            (egui::TextStyle::Button, 14.0),
            (egui::TextStyle::Small, 12.0),
            (egui::TextStyle::Heading, 19.0),
        ] {
            style
                .text_styles
                .insert(role, egui::FontId::proportional(size));
        }
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::monospace(CODE_FONT_SIZE),
        );
    });
}

pub(super) enum Icon {
    Notes,
    Folder,
    Search,
    Links,
    Codex,
    Connections,
    Settings,
    Sidebar,
    New,
    Source,
    Ready,
    Cancel,
    More,
    Copy,
}
impl Icon {
    fn bytes(&self) -> (&'static str, &'static [u8]) {
        macro_rules! asset {
            ($name:literal) => {
                (
                    $name,
                    include_bytes!(concat!(
                        "../../assets/ui/icons/lucide-0.575.0/",
                        $name,
                        ".svg"
                    ))
                    .as_slice(),
                )
            };
        }
        match self {
            Self::Notes => asset!("file-text"),
            Self::Folder => asset!("folder-open"),
            Self::Search => asset!("search"),
            Self::Links => asset!("link"),
            Self::Codex => asset!("sparkles"),
            Self::Connections => asset!("refresh-cw"),
            Self::Settings => asset!("settings"),
            Self::Sidebar => asset!("panel-left"),
            Self::New => asset!("plus"),
            Self::Source => asset!("code-xml"),
            Self::Ready => asset!("check"),
            Self::Cancel => asset!("x"),
            Self::More => asset!("ellipsis"),
            Self::Copy => asset!("copy"),
        }
    }
}
pub(super) fn image(ui: &egui::Ui, icon: Icon) -> egui::Image<'static> {
    let (name, bytes) = icon.bytes();
    // Trusted bundled XML only. Stable URI and source bytes allow loader reuse.
    let bytes = String::from_utf8_lossy(bytes)
        .replace("currentColor", "#ffffff")
        .replace("stroke-width=\"2\"", "stroke-width=\"1.6\"")
        .into_bytes();
    egui::Image::from_bytes(format!("bytes://quiet-{name}.svg"), bytes)
        .fit_to_exact_size(egui::Vec2::splat(if touch(ui) { 20.0 } else { 18.0 }))
        .tint(ui.visuals().text_color())
}
pub(super) fn control(ui: &egui::Ui, icon: Icon) -> egui::Button<'static> {
    let size = control_size(ui);
    egui::Button::new(image(ui, icon))
        .frame(true)
        .corner_radius(6)
        .min_size(egui::vec2(size, size))
}
pub(super) fn button(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> egui::Response {
    icon_button(ui, icon, label, enabled, None)
}
pub(super) fn selected_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    icon_button(ui, icon, label, enabled, Some(selected))
}
fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    selected: Option<bool>,
) -> egui::Response {
    let size = control_size(ui);
    let response = ui
        .scope(|ui| {
            // Equal padding centres the 18px icon in the square hit target,
            // independently of the surrounding panel's left-aligned layout.
            ui.spacing_mut().button_padding =
                egui::Vec2::splat((size - if touch(ui) { 20.0 } else { 18.0 }) * 0.5);
            ui.add_enabled(
                enabled,
                control(ui, icon)
                    .selected(selected.unwrap_or(false))
                    .frame_when_inactive(selected.unwrap_or(false)),
            )
        })
        .inner
        .on_hover_text(label);
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            6.0,
            egui::Stroke::new(2.0, colors(ui.visuals().dark_mode).accent),
            egui::StrokeKind::Inside,
        );
    }
    response.widget_info(|| match selected {
        Some(selected) => {
            egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label)
        }
        None => egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label),
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_follows_input_without_resetting_typography() {
        let ctx = egui::Context::default();
        palette(&ctx);
        ctx.style_mut(|style| {
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(19.0));
        });
        for (touch, expected) in [(true, 48.0), (false, 34.0)] {
            set_touch_density(&ctx, touch);
            assert_eq!(
                ctx.style().spacing.interact_size,
                egui::Vec2::splat(expected)
            );
            assert_eq!(ctx.style().text_styles[&egui::TextStyle::Body].size, 19.0);
        }
        for dark in [true, false] {
            let c = colors(dark);
            assert_ne!(c.selected, c.hover);
            assert_ne!(c.selected, c.accent);
        }
    }
}
