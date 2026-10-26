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
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 5.0);
        if cfg!(target_os = "android") {
            style.spacing.interact_size.y = 48.0;
        }
    });
    egui_extras::install_image_loaders(ctx);
}

pub(super) enum Icon {
    Notes,
    Source,
    Preview,
    Ready,
    Cancel,
}
impl Icon {
    fn bytes(&self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Notes => (
                "notes",
                include_bytes!("../../assets/ui/icons/lucide-0.575.0/folder-open.svg"),
            ),
            Self::Source => (
                "source",
                include_bytes!("../../assets/ui/icons/lucide-0.575.0/code-xml.svg"),
            ),
            Self::Preview => (
                "preview",
                include_bytes!("../../assets/ui/icons/lucide-0.575.0/eye.svg"),
            ),
            Self::Ready => (
                "ready",
                include_bytes!("../../assets/ui/icons/lucide-0.575.0/check.svg"),
            ),
            Self::Cancel => (
                "cancel",
                include_bytes!("../../assets/ui/icons/lucide-0.575.0/x.svg"),
            ),
        }
    }
}
pub(super) fn button(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> egui::Response {
    let (name, bytes) = icon.bytes();
    // Only trusted bundled XML; keep a stable key. Tint white raster pixels per theme.
    let bytes = String::from_utf8_lossy(bytes)
        .replace("currentColor", "#ffffff")
        .into_bytes();
    let image = egui::Image::from_bytes(format!("bytes://ui-{name}.svg"), bytes)
        .fit_to_exact_size(egui::vec2(18.0, 18.0))
        .tint(ui.visuals().text_color());
    ui.add_enabled(enabled, egui::Button::image_and_text(image, label))
}
