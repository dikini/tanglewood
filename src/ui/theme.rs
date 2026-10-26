//! Small application theme over native egui controls and trusted bundled assets.
use eframe::egui;

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
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(25, 25, 25)
    } else {
        egui::Color32::from_rgb(250, 250, 250)
    }
}
pub(super) fn sidebar(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(22, 22, 22)
    } else {
        egui::Color32::from_rgb(242, 242, 242)
    }
}
pub(super) fn muted(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(153, 153, 153)
    } else {
        egui::Color32::from_rgb(115, 115, 115)
    }
}
pub(super) fn palette(ctx: &egui::Context) {
    ctx.style_mut(|style| {
        let dark = style.visuals.dark_mode;
        let color = |d, l| egui::Color32::from_gray(if dark { d } else { l });
        style.visuals.override_text_color = Some(color(223, 41));
        style.visuals.panel_fill = color(22, 242);
        style.visuals.window_fill = color(34, 255);
        style.visuals.extreme_bg_color = color(34, 255);
        style.visuals.faint_bg_color = color(28, 246);
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, color(48, 229));
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, color(48, 229));
        style.visuals.widgets.hovered.weak_bg_fill = color(43, 229);
        style.visuals.widgets.active.weak_bg_fill = color(48, 220);
        style.visuals.selection.bg_fill = color(43, 229);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, color(223, 41));
        style.visuals.window_corner_radius = egui::CornerRadius::same(10);
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(7.0, 7.0);
        style.spacing.interact_size = egui::vec2(
            34.0,
            if cfg!(target_os = "android") {
                44.0
            } else {
                34.0
            },
        );
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(18.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    });
}

pub(super) enum Icon {
    Notes,
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
        .fit_to_exact_size(egui::vec2(18.0, 18.0))
        .tint(ui.visuals().text_color())
}
pub(super) fn control(ui: &egui::Ui, icon: Icon) -> egui::Button<'static> {
    let size = if cfg!(target_os = "android") {
        44.0
    } else {
        34.0
    };
    egui::Button::image(image(ui, icon))
        .frame(false)
        .min_size(egui::vec2(size, size))
}
pub(super) fn button(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> egui::Response {
    selected_button(ui, icon, label, enabled, false)
}
pub(super) fn selected_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    let response = ui
        .add_enabled(enabled, control(ui, icon).selected(selected))
        .on_hover_text(label);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response
}
