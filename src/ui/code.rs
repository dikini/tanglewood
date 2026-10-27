//! Cached syntax presentation; authored source remains untouched.
use eframe::egui;
use egui_extras::syntax_highlighting::{CodeTheme, SyntectSettings};
use std::sync::OnceLock;

fn settings() -> &'static SyntectSettings {
    static SETTINGS: OnceLock<SyntectSettings> = OnceLock::new();
    SETTINGS.get_or_init(|| {
        let defaults = SyntectSettings::default();
        let mut builder = defaults.ps.into_builder();
        let syntax = syntect::parsing::SyntaxDefinition::load_from_str(
            include_str!("../../assets/syntax/ash.sublime-syntax"),
            true,
            None,
        )
        .expect("bundled Ash syntax must be valid");
        builder.add(syntax);
        SyntectSettings {
            ps: builder.build(),
            ts: defaults.ts,
        }
    })
}

fn layout(ui: &egui::Ui, source: &str, language: &str) -> egui::text::LayoutJob {
    let settings = settings();
    let language = settings
        .ps
        .find_syntax_by_token(language)
        .map(|syntax| syntax.name.as_str())
        .unwrap_or(language);
    let mut job = egui_extras::syntax_highlighting::highlight_with(
        ui.ctx(),
        ui.style(),
        &CodeTheme::from_style(ui.style()),
        source,
        language,
        settings,
    );
    for section in &mut job.sections {
        section.format.font_id = egui::FontId::monospace(super::theme::CODE_FONT_SIZE);
        section.format.line_height = Some(super::theme::CODE_LINE_HEIGHT);
    }
    job
}

pub(super) fn show(ui: &mut egui::Ui, source: &str, language: &str) {
    show_inner(ui, source, language, false);
}

/// Code copy always uses the untouched body, including its final newline.
pub(super) fn show_copy(ui: &mut egui::Ui, source: &str, language: &str) {
    show_inner(ui, source, language, true);
}

fn show_inner(ui: &mut egui::Ui, source: &str, language: &str, copy: bool) {
    let colors = super::theme::colors(ui.visuals().dark_mode);
    egui::Frame::new()
        .fill(colors.raised)
        .stroke(egui::Stroke::new(1.0, colors.border))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(match language {
                        "" => "Code",
                        "ash" => "Ash",
                        "sh" | "bash" => "Bash",
                        "rust" => "Rust",
                        "python" | "py" => "Python",
                        _ => language,
                    })
                    .size(12.0)
                    .color(super::theme::muted(ui)),
                );
                if copy
                    && super::theme::button(ui, super::theme::Icon::Copy, "Copy code", true)
                        .clicked()
                {
                    ui.ctx().copy_text(source.to_owned());
                }
            });
            ui.add_space(6.0);
            let job = layout(ui, source.trim_end_matches('\n'), language);
            let scroll = egui::ScrollArea::horizontal()
                .id_salt("code-scroll")
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(job)
                            .selectable(true)
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
            super::navigation::scroll_focus(ui.ctx(), &scroll);
        });
    ui.add_space(super::theme::PARAGRAPH_GAP);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ash_grammar_and_builtin_languages_colour_without_rewriting() {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for (language, source) in [
                    (
                        "ash",
                        "pub main : Unit -> Int\nmain () = match true { true => 42 }\n",
                    ),
                    ("bash", "echo \"hello world\"\n"),
                ] {
                    let job = layout(ui, source, language);
                    assert_eq!(job.text, source);
                    let colours: std::collections::HashSet<_> =
                        job.sections.iter().map(|s| s.format.color).collect();
                    assert!(colours.len() >= 3, "{language}: expected syntax colours");
                }
                let rust = layout(ui, "let message = \"hello\";", "rust");
                assert!(
                    rust.sections
                        .iter()
                        .map(|s| s.format.color)
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        >= 3
                );
                let source = "🦀 untouched <code>";
                assert_eq!(layout(ui, source, "unknown-language").text, source);
            });
        });
    }
}
