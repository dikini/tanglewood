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
}

impl Draft {
    pub(super) fn new(view: &NoteView, widget_generation: u64) -> Self {
        Self {
            text: view.note.text.clone(),
            base_text: view.note.text.clone(),
            revision: view.note.revision,
            widget_generation,
            conflict: false,
            composing: false,
            range: None,
            highlight: None,
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
