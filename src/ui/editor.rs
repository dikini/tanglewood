//! Shared per-note draft state and source edits, independent of pane layout.
use crate::notes::{NoteView, Revision, TextEdit};
use std::ops::Range;

pub(super) struct Draft {
    pub(super) text: String,
    pub(super) base_text: String,
    pub(super) revision: Revision,
    pub(super) widget_generation: u64,
    pub(super) conflict: bool,
    pub(super) composing: bool,
    pub(super) range: Option<Range<usize>>,
    pub(super) highlight: Option<(String, bool, eframe::egui::text::LayoutJob)>,
    pub(super) context: std::sync::Arc<crate::document_world::DocumentWorld>,
}

impl Draft {
    #[cfg(test)]
    pub(super) fn new(view: &NoteView, widget_generation: u64) -> Self {
        let context = std::sync::Arc::new(crate::document_world::DocumentWorld::parse(
            view.note.id,
            view.note.revision,
            &view.note.text,
            None,
        ));
        Self::with_context(view, widget_generation, context)
    }
    pub(super) fn with_context(
        view: &NoteView,
        widget_generation: u64,
        context: std::sync::Arc<crate::document_world::DocumentWorld>,
    ) -> Self {
        Self {
            text: view.note.text.clone(),
            base_text: view.note.text.clone(),
            revision: view.note.revision,
            widget_generation,
            conflict: false,
            composing: false,
            range: None,
            highlight: None,
            context,
        }
    }
    pub(super) fn dirty(&self) -> bool {
        self.text != self.base_text[self.range.clone().unwrap_or(0..self.base_text.len())]
    }
    pub(super) fn unsubmitted(&self) -> bool {
        self.dirty() || self.composing || self.conflict
    }
    pub(super) fn can_change_mode(&self) -> bool {
        !self.dirty() && !self.composing && !self.conflict
    }
    pub(super) fn choose_range(&mut self, range: Option<Range<usize>>) -> bool {
        if !self.can_change_mode() {
            return false;
        }
        let bytes = range.clone().unwrap_or(0..self.base_text.len());
        let Some(text) = self.base_text.get(bytes) else {
            return false;
        };
        self.text = text.to_owned();
        self.range = range;
        self.widget_generation += 1;
        self.highlight = None;
        true
    }
    /// Only an exact original top-level block has an in-place editing slot.
    /// Custom fences need more room than their rendered illustration/callout.
    pub(super) fn inline_block(&self) -> Option<crate::document_world::Node<'_>> {
        let range = self.range.as_ref()?;
        self.context
            .roots()
            .iter()
            .filter_map(|entity| self.context.node(*entity))
            .find(|block| {
                block.origin.is_some_and(|origin| origin.bytes == *range)
                    && !matches!(block.kind, crate::markdown::BlockKind::Fence(fence)
                    if !fence.attributes.classes.is_empty() || !fence.attributes.values.is_empty())
            })
    }
    pub(super) fn edit(&self) -> Option<TextEdit> {
        if !self.dirty() || self.conflict || self.composing {
            return None;
        }
        let range = self.range.clone().unwrap_or(0..self.base_text.len());
        let mut edit = contiguous_edit(&self.base_text[range.clone()], &self.text);
        edit.start += range.start;
        edit.end += range.start;
        Some(edit)
    }
    pub(super) fn proposed_text(&self) -> String {
        let mut text = self.base_text.clone();
        text.replace_range(self.range.clone().unwrap_or(0..text.len()), &self.text);
        text
    }
    pub(super) fn layout(&mut self, ctx: &eframe::egui::Context) -> eframe::egui::text::LayoutJob {
        let dark = ctx.style().visuals.dark_mode;
        if self
            .highlight
            .as_ref()
            .is_none_or(|(text, theme, _)| text != &self.text || *theme != dark)
        {
            let theme = egui_extras::syntax_highlighting::CodeTheme::from_style(&ctx.style());
            let job = egui_extras::syntax_highlighting::highlight(
                ctx,
                &ctx.style(),
                &theme,
                &self.text,
                "md",
            );
            self.highlight = Some((self.text.clone(), dark, job));
        }
        self.highlight.as_ref().unwrap().2.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditorAction {
    Ready,
    Cancel,
}

/// One native field and one action row in every presentation. Domain acceptance,
/// cancellation and widget retirement remain owned by the shell.
pub(super) fn source_editor(
    ui: &mut eframe::egui::Ui,
    draft: &mut Draft,
    note: crate::notes::NoteId,
    trashed: bool,
    inline: bool,
    height: f32,
) -> Option<EditorAction> {
    use eframe::egui;
    let ctx = ui.ctx().clone();
    let source_id = egui::Id::new(("source", note.0, draft.widget_generation));
    #[cfg(target_os = "android")]
    {
        let change = crate::android_input::before(&ctx, source_id, &mut draft.text);
        if change.active {
            draft.composing = change.composing;
        }
        draft.conflict |= change.conflict;
    }
    let job = if inline {
        let heading = draft.inline_block().and_then(|block| match block.kind {
            crate::markdown::BlockKind::Heading(level) => Some(level),
            _ => None,
        });
        let size = match heading {
            Some(1) => 28.0,
            Some(2) => 23.0,
            Some(_) => 19.0,
            None => super::theme::READING_FONT_SIZE,
        };
        let line_height = if heading.is_some() {
            (size * 1.3).max(27.0)
        } else {
            super::theme::READING_LINE_HEIGHT
        };
        let mut job = egui::text::LayoutJob::simple(
            draft.text.clone(),
            egui::FontId::proportional(size),
            ui.visuals().text_color(),
            ui.available_width(),
        );
        for section in &mut job.sections {
            section.format.line_height = Some(line_height);
        }
        job
    } else {
        draft.layout(&ctx)
    };
    let mut measured = job.clone();
    measured.wrap.max_width = (ui.available_width() - if inline { 0.0 } else { 24.0 }).max(1.0);
    let field_height = if inline {
        ui.fonts_mut(|fonts| fonts.layout_job(measured))
            .size()
            .y
            .max(28.0)
            .min(height)
    } else {
        (ui.fonts_mut(|fonts| fonts.layout_job(measured)).size().y + 24.0)
            .clamp(60.0_f32.min(height), height)
    };
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
        let mut layout = if job.text == text.as_str() {
            job.clone()
        } else {
            let format = job
                .sections
                .first()
                .map(|section| section.format.clone())
                .unwrap_or_default();
            let mut changed = egui::text::LayoutJob::default();
            changed.append(text.as_str(), 0.0, format);
            changed
        };
        layout.wrap.max_width = width;
        ui.fonts_mut(|fonts| fonts.layout_job(layout))
    };
    // Allocate a real field height: a max-height alone inside the document's
    // zero-height layout allocation can otherwise collapse to a few clipped lines.
    let output = egui::ScrollArea::vertical()
        .id_salt(("active-source", source_id))
        .max_height(height)
        // Document blocks are measured from a zero-height child. Without an
        // explicit viewport minimum, ScrollArea uses its 64-point fallback and
        // clips even a correctly sized multiline TextEdit to three lines.
        .min_scrolled_height(field_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            // A remote trash must not prevent selecting/copying a retained draft.
            // Only Ready is disabled; local draft text is never canonical here.
            egui::TextEdit::multiline(&mut draft.text)
                .id(source_id)
                .layouter(&mut layouter)
                .frame(!inline)
                .margin(if inline {
                    egui::Vec2::ZERO
                } else {
                    egui::vec2(12.0, 12.0)
                })
                .min_size(egui::vec2(0.0, if inline { 28.0 } else { field_height }))
                .desired_width(f32::INFINITY)
                .desired_rows(1)
                .show(ui)
        })
        .inner;
    super::name_field(output.response.clone(), &draft.text, "Markdown source");
    // Only the active draft receives a boundary; ordinary reader blocks stay quiet.
    ui.painter().line_segment(
        [
            output.response.interact_rect.left_top() - egui::vec2(5.0, 0.0),
            output.response.interact_rect.left_bottom() - egui::vec2(5.0, 0.0),
        ],
        egui::Stroke::new(2.0, super::theme::colors(ui.visuals().dark_mode).accent),
    );
    let focus_key = source_id.with("initial-focus");
    if !ctx.data_mut(|data| data.get_temp::<bool>(focus_key).unwrap_or(false)) {
        output.response.request_focus();
        ctx.data_mut(|data| data.insert_temp(focus_key, true));
    }
    #[cfg(target_os = "android")]
    crate::android_input::after(&ctx, &output, &draft.text);
    if output.response.has_focus() {
        ctx.input(|input| {
            for event in &input.events {
                match event {
                    egui::Event::Ime(egui::ImeEvent::Preedit(text)) => {
                        draft.composing = !text.is_empty()
                    }
                    egui::Event::Ime(egui::ImeEvent::Commit(_))
                    | egui::Event::Ime(egui::ImeEvent::Disabled) => draft.composing = false,
                    _ => {}
                }
            }
        });
    }
    let mut action = None;
    ui.horizontal(|ui| {
        #[cfg(target_os = "android")]
        crate::android_input::menu(ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if super::theme::button(
                ui,
                super::theme::Icon::Ready,
                "Apply changes",
                !trashed && !draft.composing && !draft.conflict,
            )
            .clicked()
            {
                action = Some(EditorAction::Ready);
            }
            if super::theme::button(ui, super::theme::Icon::Cancel, "Cancel changes", true)
                .clicked()
            {
                action = Some(EditorAction::Cancel);
            }
        });
    });
    ui.add_space(if inline { 12.0 } else { 0.0 });
    action
}

/// Small contiguous edit with byte offsets that remain UTF-8 boundaries.
pub(super) fn contiguous_edit(before: &str, after: &str) -> TextEdit {
    let prefix = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let suffix = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    TextEdit {
        start: prefix,
        end: before.len() - suffix,
        replacement: after[prefix..after.len() - suffix].to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        App,
        commands::{Caller, Command, CommandResult},
    };
    #[test]
    fn ready_block_preserves_adjacent_bytes_and_cancel_conflict() {
        let mut app = App::new();
        let CommandResult::Note(view) = app
            .execute(
                Command::CreateNote {
                    path: "A.md".into(),
                    text: "# Header\n\nα café\n\n```rust\nlet x=1;\n```\n".into(),
                },
                Caller::Local,
            )
            .unwrap()
        else {
            panic!()
        };
        let mut draft = Draft::new(&view, 0);
        let document = crate::markdown::read(view.note.id, view.note.revision, &view.note.text);
        let origin = document.blocks[1].origin.as_ref().unwrap();
        let (start, end) = (origin.bytes.start, origin.bytes.end);
        assert!(draft.choose_range(Some(start..end)));
        assert!(draft.inline_block().is_some());
        draft.text = "β 🌲".into();
        assert_eq!(app.note(view.note.id).unwrap().note.text, view.note.text);
        assert!(!draft.choose_range(None));
        draft.composing = true;
        assert!(draft.edit().is_none());
        draft.composing = false;
        draft.conflict = true;
        assert!(draft.unsubmitted());
        assert!(!draft.can_change_mode());
        assert!(draft.edit().is_none());
        draft.conflict = false;
        let edit = draft.edit().unwrap();
        let mut expected = view.note.text.clone();
        expected.replace_range(start..end, "β 🌲");
        assert_eq!(draft.proposed_text(), expected);
        let CommandResult::Note(accepted) = app
            .execute(
                Command::EditNote {
                    id: view.note.id,
                    expected: draft.revision,
                    edits: vec![edit],
                },
                Caller::Local,
            )
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(accepted.note.text, expected);
        assert!(
            app.execute(
                Command::EditNote {
                    id: view.note.id,
                    expected: draft.revision,
                    edits: vec![draft.edit().unwrap()]
                },
                Caller::Local
            )
            .is_err()
        );
        // Cancel adopts only accepted source, with a fresh widget identity.
        let cancelled = Draft::new(&accepted, draft.widget_generation + 1);
        assert!(!cancelled.dirty());
        assert_eq!(cancelled.text, expected);
    }
}
