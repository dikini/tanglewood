//! Derived, read-only conversation presentation. Never resolves files or images.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_fixture_preserves_source_and_bounds_cache() {
        let source = "# Héllo\n\n**Strong** and `code` [safe](https://example.com) [local](file:///secret)\n\n- [x] done\n  - nested\n\n| A | B |\n|---|---|\n| one | two |\n\n> quote\n\n<!-- hidden -->\n\n```rust\nlet x = 42;\n";
        let parsed = parse(source);
        assert!(parsed.iter().any(|r| matches!(r.kind, Kind::Code { .. })));
        assert!(parsed.iter().any(|r| matches!(r.kind, Kind::Table(_))));
        assert!(parsed.iter().any(|r| r.prefix.contains('☑')));
        assert!(!format!("{parsed:?}").contains("secret"));
        assert!(!format!("{parsed:?}").contains("hidden"));
        let code = parsed
            .iter()
            .find_map(|r| match &r.kind {
                Kind::Code { source, .. } => Some(source.as_str()),
                _ => None,
            })
            .unwrap();
        assert_eq!(code, "let x = 42;\n");
        for (markdown, expected) in [
            ("```text\r\nline\r\n```\r\n", "line\r\n"),
            ("> ```rust\n> let x = 42;\n> ```\n", "let x = 42;\n"),
            ("  ```text\n\tvalue\n  ```\n", "  value\n"),
        ] {
            let rows = parse(markdown);
            let body = rows
                .iter()
                .find_map(|r| match &r.kind {
                    Kind::Code { source, .. } => Some(source.as_str()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(body, expected);
        }
        let amplified = format!(
            "{}\n\n[x]: https://example.com/{}",
            "[x] ".repeat(4000),
            "a".repeat(10000)
        );
        let bounded = parse(&amplified);
        assert!(
            matches!(&bounded[..], [Row { kind: Kind::Prose(text, None), .. }] if text == &vec![Inline::Text(amplified)])
        );
        let mut cache = Cache::default();
        cache.prepare(1, source);
        assert_eq!(cache.entries[0].source, source);
        cache.prepare(1, "changed");
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.entries[0].source, "changed");
        for id in 2..100 {
            cache.prepare(id, &"x".repeat(12_000));
        }
        assert!(cache.entries.len() <= MAX_ENTRIES);
        assert!(cache.entries.iter().map(Entry::bytes).sum::<usize>() <= MAX_BYTES);
        let old = cache.entries.len();
        cache.prepare(999, &"x".repeat(MAX_REPLY + 1));
        assert_eq!(cache.entries.len(), old);
    }
}

use crate::markdown::Inline;
use eframe::egui;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::{collections::VecDeque, hash::Hash};

const MAX_REPLY: usize = 256 * 1024;
const MAX_BYTES: usize = 512 * 1024;
const MAX_ENTRIES: usize = 64;
const MAX_ROWS: usize = 8192;
const MAX_DEPTH: usize = 32;

#[derive(Debug)]
enum Kind {
    Prose(Vec<Inline>, Option<u8>),
    Code { language: String, source: String },
    Table(Vec<Vec<Vec<Inline>>>),
    Rule,
}
#[derive(Debug)]
struct Row {
    kind: Kind,
    depth: usize,
    quote: bool,
    prefix: String,
}
struct Entry {
    id: egui::Id,
    source: String,
    rows: Vec<Row>,
}

impl Entry {
    fn bytes(&self) -> usize {
        fn inline_bytes(inlines: &Vec<Inline>) -> usize {
            inlines.capacity() * std::mem::size_of::<Inline>()
                + inlines
                    .iter()
                    .map(|i| match i {
                        Inline::Text(s) | Inline::Code(s) => s.capacity(),
                        Inline::Link { target, label, .. } => target.capacity() + label.capacity(),
                        _ => 0,
                    })
                    .sum::<usize>()
        }
        std::mem::size_of::<Self>()
            + self.source.capacity()
            + self.rows.capacity() * std::mem::size_of::<Row>()
            + self
                .rows
                .iter()
                .map(|r| {
                    r.prefix.capacity()
                        + match &r.kind {
                            Kind::Prose(inlines, _) => inline_bytes(inlines),
                            Kind::Code { language, source } => {
                                language.capacity() + source.capacity()
                            }
                            Kind::Table(rows) => {
                                rows.capacity() * std::mem::size_of::<Vec<Vec<Inline>>>()
                                    + rows
                                        .iter()
                                        .map(|cells| {
                                            cells.capacity() * std::mem::size_of::<Vec<Inline>>()
                                                + cells.iter().map(inline_bytes).sum::<usize>()
                                        })
                                        .sum::<usize>()
                            }
                            Kind::Rule => 0,
                        }
                })
                .sum::<usize>()
    }
}

/// Source and derived rows are retained only for the most recently used replies.
#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<Entry>,
}
impl Cache {
    fn prepare(&mut self, id: impl Hash, source: &str) -> Option<&[Row]> {
        if source.len() > MAX_REPLY {
            return None;
        }
        let id = egui::Id::new(id);
        if let Some(index) = self.entries.iter().position(|entry| entry.id == id) {
            let mut entry = self.entries.remove(index).expect("existing entry");
            if entry.source != source {
                entry.rows = parse(source);
                entry.source = source.to_owned();
            }
            self.entries.push_back(entry);
        } else {
            self.entries.push_back(Entry {
                id,
                source: source.to_owned(),
                rows: parse(source),
            });
        }
        while self.entries.len() > MAX_ENTRIES
            || self.entries.iter().map(Entry::bytes).sum::<usize>() > MAX_BYTES
        {
            self.entries.pop_front();
        }
        self.entries.back().map(|entry| entry.rows.as_slice())
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, id: impl Hash, source: &str) {
        ui.push_id(id, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            if let Some(rows) = self.prepare(ui.id(), source) {
                for (index, row) in rows.iter().enumerate() {
                    ui.push_id(index, |ui| show_row(ui, row));
                }
            } else {
                // Large replies stay readable without constructing a second parse tree.
                ui.add(egui::Label::new(source).wrap().selectable(true));
            }
        });
    }
}

fn show_row(ui: &mut egui::Ui, row: &Row) {
    let indent = row.depth.min(8) as f32 * 18.0;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if indent > 0.0 {
            ui.add_space(indent);
        }
        if row.quote {
            ui.label(egui::RichText::new("│").color(super::theme::muted(ui)));
        }
        if !row.prefix.is_empty() {
            ui.label(&row.prefix);
        }
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width().max(1.0));
            match &row.kind {
                Kind::Prose(inlines, heading) => {
                    super::document::transcript_prose(ui, inlines, *heading)
                }
                Kind::Code { language, source } => super::code::show_copy(ui, source, language),
                Kind::Rule => {
                    ui.separator();
                    ui.add_space(8.0);
                }
                Kind::Table(rows) => {
                    let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                    let cell_width = ((ui.available_width()
                        - (columns.saturating_sub(1) as f32 * 20.0))
                        / columns as f32)
                        .clamp(120.0, 360.0);
                    egui::ScrollArea::horizontal()
                        .id_salt("table-scroll")
                        .show(ui, |ui| {
                            egui::Grid::new("table")
                                .striped(true)
                                .spacing([20.0, 4.0])
                                .show(ui, |ui| {
                                    for (r, cells) in rows.iter().enumerate() {
                                        for cell in cells {
                                            ui.vertical(|ui| {
                                                ui.set_width(cell_width);
                                                super::document::transcript_prose(
                                                    ui,
                                                    cell,
                                                    if r == 0 { Some(6) } else { None },
                                                );
                                            });
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                    ui.add_space(10.0);
                }
            }
        });
    });
}

fn flush(
    rows: &mut Vec<Row>,
    inlines: &mut Vec<Inline>,
    heading: Option<u8>,
    depth: usize,
    quote: usize,
    prefix: &mut String,
) {
    if !inlines.is_empty() {
        rows.push(Row {
            kind: Kind::Prose(std::mem::take(inlines), heading),
            depth: depth.saturating_sub(1),
            quote: quote > 0,
            prefix: std::mem::take(prefix),
        });
    }
}

/// Iterative event folding: nesting has a fixed bound and the source is never rewritten.
fn parse(source: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut inlines = Vec::new();
    let mut heading = None;
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut quote = 0usize;
    let mut prefix = String::new();
    let mut code: Option<(String, String)> = None;
    let mut link: Option<(String, String)> = None;
    let mut table: Option<Vec<Vec<Vec<Inline>>>> = None;
    let mut cells = Vec::new();
    let options = Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let mut derived_bytes = 0usize;
    // Include expanded reference targets and event overhead before any owned clones.
    // This also bounds table cell/row allocations even though one table is one Row.
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        derived_bytes = derived_bytes.saturating_add(
            64 + match &event {
                Event::Start(Tag::Link { dest_url, .. }) => dest_url.len(),
                Event::Text(text) | Event::Code(text) => text.len(),
                _ => 0,
            },
        );
        if derived_bytes > MAX_REPLY * 2
            || rows.len() >= MAX_ROWS
            || lists.len() > MAX_DEPTH
            || quote > MAX_DEPTH
        {
            return vec![Row {
                kind: Kind::Prose(vec![Inline::Text(source.to_owned())], None),
                depth: 0,
                quote: false,
                prefix: String::new(),
            }];
        }
        if let Some((language, body)) = code.as_mut() {
            match event {
                Event::End(TagEnd::CodeBlock) => {
                    let kind = Kind::Code {
                        language: std::mem::take(language),
                        source: std::mem::take(body),
                    };
                    rows.push(Row {
                        kind,
                        depth: lists.len().saturating_sub(1),
                        quote: quote > 0,
                        prefix: std::mem::take(&mut prefix),
                    });
                    code = None;
                }
                Event::Text(text) => {
                    if range.is_empty() {
                        // A partly consumed structural tab leaves synthesized
                        // spaces with no authored byte range. Keep their meaning.
                        body.push_str(&text);
                        continue;
                    }
                    // pulldown emits LF as a separate range after discarding CR.
                    // Restore only that adjacent authored CR; gaps elsewhere may
                    // be structural quote/list indentation and must stay omitted.
                    let raw = &source[range.clone()];
                    if raw.starts_with('\n')
                        && range.start > 0
                        && source.as_bytes()[range.start - 1] == b'\r'
                        && !body.ends_with('\r')
                    {
                        body.push('\r');
                    }
                    body.push_str(raw);
                }
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
                let language = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(80)
                        .collect(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((language, String::new()));
            }
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(level as u8);
            }
            Event::End(TagEnd::Heading(_)) | Event::End(TagEnd::Paragraph) if table.is_none() => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
                heading = None;
            }
            Event::Start(Tag::List(start)) => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
                lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
                prefix = match lists.last_mut() {
                    Some(Some(n)) => {
                        let text = format!("{n}.");
                        *n = n.saturating_add(1);
                        text
                    }
                    _ => "•".into(),
                };
            }
            Event::End(TagEnd::Item) => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
            }
            Event::TaskListMarker(done) => {
                prefix = if done { "☑" } else { "☐" }.into();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
                quote += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
                quote = quote.saturating_sub(1);
            }
            Event::Start(Tag::Table(_)) => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
                table = Some(Vec::new());
            }
            Event::End(TagEnd::TableCell) => {
                cells.push(std::mem::take(&mut inlines));
            }
            Event::End(TagEnd::TableRow) | Event::End(TagEnd::TableHead) => {
                if let Some(table) = &mut table {
                    table.push(std::mem::take(&mut cells));
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(table) = table.take() {
                    rows.push(Row {
                        kind: Kind::Table(table),
                        depth: 0,
                        quote: quote > 0,
                        prefix: String::new(),
                    });
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                link = Some((dest_url.to_string(), String::new()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((target, label)) = link.take() {
                    let lower = target.to_ascii_lowercase();
                    inlines.push(
                        if lower.starts_with("https://")
                            || lower.starts_with("http://")
                            || lower.starts_with("source://")
                        {
                            Inline::Link {
                                target,
                                label,
                                wiki: false,
                            }
                        } else {
                            Inline::Text(label)
                        },
                    );
                }
            }
            Event::Text(text) | Event::Code(text) if link.is_some() => {
                if let Some((_, label)) = &mut link {
                    label.push_str(&text);
                }
            }
            Event::Text(text) => inlines.push(Inline::Text(text.to_string())),
            Event::Code(text) => inlines.push(Inline::Code(text.to_string())),
            Event::SoftBreak => inlines.push(Inline::Text(" ".into())),
            Event::HardBreak => inlines.push(Inline::Break),
            Event::Start(Tag::Emphasis) if link.is_none() => inlines.push(Inline::EmphasisStart),
            Event::End(TagEnd::Emphasis) if link.is_none() => inlines.push(Inline::EmphasisEnd),
            Event::Start(Tag::Strong) if link.is_none() => inlines.push(Inline::StrongStart),
            Event::End(TagEnd::Strong) if link.is_none() => inlines.push(Inline::StrongEnd),
            Event::Rule => {
                flush(
                    &mut rows,
                    &mut inlines,
                    heading,
                    lists.len(),
                    quote,
                    &mut prefix,
                );
                rows.push(Row {
                    kind: Kind::Rule,
                    depth: 0,
                    quote: false,
                    prefix: String::new(),
                });
            }
            // Image alt text remains readable; HTML and comments never execute or fetch.
            Event::Html(_) | Event::InlineHtml(_) => {}
            _ => {}
        }
    }
    flush(
        &mut rows,
        &mut inlines,
        heading,
        lists.len(),
        quote,
        &mut prefix,
    );
    rows
}
