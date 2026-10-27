//! Lossless-source reader boundary. Render data is disposable; Markdown stays canonical.
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};

use crate::notes::{NoteId, Revision};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceOrigin {
    pub note: NoteId,
    pub revision: Revision,
    pub bytes: Range<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attributes {
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub values: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Break,
    EmphasisStart,
    EmphasisEnd,
    StrongStart,
    StrongEnd,
    Link {
        target: String,
        label: String,
        wiki: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fence {
    pub body: String,
    pub raw_info: String,
    pub attributes: Attributes,
    /// Exact fenced source, including delimiters, for unavailable renderers.
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Quote,
    Div,
    Fence(Fence),
    /// Unsupported structures remain visibly available as their exact source.
    Source(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderBlock {
    pub kind: BlockKind,
    pub inlines: Vec<Inline>,
    pub children: Vec<RenderBlock>,
    pub attributes: Attributes,
    pub origin: Option<SourceOrigin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub note: NoteId,
    pub revision: Revision,
    pub blocks: Vec<RenderBlock>,
    pub metadata: Vec<(String, String)>,
    pub links: Vec<String>,
    pub read_only: bool,
}

impl Document {
    pub fn clear_origins(&mut self) {
        fn clear(block: &mut RenderBlock) {
            block.origin = None;
            for child in &mut block.children {
                clear(child);
            }
        }
        for block in &mut self.blocks {
            clear(block);
        }
        self.read_only = true;
    }
}

/// Reads CommonMark plus pulldown's wikilinks. No text is rewritten.
pub fn read(note: NoteId, revision: Revision, source: &str) -> Document {
    let events: Vec<_> = Parser::new_ext(source, Options::ENABLE_WIKILINKS)
        .into_offset_iter()
        .collect();
    let mut html_regions: Vec<Range<usize>> = Vec::new();
    for (event, range) in &events {
        if matches!(event, Event::Html(_) | Event::InlineHtml(_)) {
            if let Some(previous) = html_regions.last_mut().filter(|r| r.end == range.start) {
                previous.end = range.end;
            } else {
                html_regions.push(range.clone());
            }
        }
    }
    let comments: Vec<_> = html_regions
        .into_iter()
        .flat_map(|range| {
            html_comment_ranges(&source[range.clone()])
                .into_iter()
                .map(move |comment| range.start + comment.start..range.start + comment.end)
        })
        .collect();
    let visible = |range: Range<usize>| {
        let mut text = String::new();
        let mut cursor = range.start;
        let first = comments.partition_point(|comment| comment.end <= range.start);
        for comment in &comments[first..] {
            if comment.start >= range.end {
                break;
            }
            text.push_str(&source[cursor..comment.start.max(cursor)]);
            cursor = comment.end.min(range.end);
        }
        text.push_str(&source[cursor..range.end]);
        text
    };
    let mut links = Vec::new();
    for (event, _) in &events {
        if let Event::Start(Tag::Link { dest_url, .. }) = event {
            links.push(dest_url.to_string());
        }
    }
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < events.len() {
        let (event, range) = &events[index];
        if let Event::Start(tag) = event {
            let start = index;
            let mut depth = 1;
            index += 1;
            while index < events.len() && depth > 0 {
                match events[index].0 {
                    Event::Start(_) => depth += 1,
                    Event::End(_) => depth -= 1,
                    _ => {}
                }
                index += 1;
            }
            let end = index;
            let bytes = range.start..events[end - 1].1.end;
            let raw = &source[bytes.clone()];
            let presentation = visible(bytes.clone());
            if presentation.trim().is_empty() {
                continue;
            }
            let origin = Some(SourceOrigin {
                note,
                revision,
                bytes,
            });
            let kind = match tag {
                Tag::Paragraph => BlockKind::Paragraph,
                Tag::Heading { level, .. } => BlockKind::Heading(*level as u8),
                Tag::CodeBlock(CodeBlockKind::Fenced(_)) => BlockKind::Fence(Fence {
                    raw_info: raw_fence_info(raw).to_owned(),
                    attributes: fence_attributes(raw_fence_info(raw)).unwrap_or_default(),
                    body: events[start + 1..end - 1]
                        .iter()
                        .filter_map(|(event, _)| match event {
                            Event::Text(text) => Some(text.as_ref()),
                            _ => None,
                        })
                        .collect(),
                    source: raw.to_owned(),
                }),
                _ => BlockKind::Source(presentation.clone()),
            };
            let kind = if matches!(kind, BlockKind::Paragraph | BlockKind::Heading(_))
                && events[start + 1..end - 1].iter().any(|(event, range)| {
                    matches!(event, Event::Start(Tag::Image { .. }))
                        || (matches!(event, Event::InlineHtml(_) | Event::Html(_))
                            && !visible(range.clone()).trim().is_empty())
                }) {
                BlockKind::Source(presentation)
            } else {
                kind
            };
            let inlines = if matches!(kind, BlockKind::Paragraph | BlockKind::Heading(_)) {
                read_inlines(&events[start + 1..end - 1])
            } else {
                Vec::new()
            };
            blocks.push(RenderBlock {
                kind,
                inlines,
                children: Vec::new(),
                attributes: Attributes::default(),
                origin,
            });
        } else {
            let presentation = visible(range.clone());
            if presentation.trim().is_empty() {
                index += 1;
                continue;
            }
            blocks.push(RenderBlock {
                kind: BlockKind::Source(presentation),
                inlines: Vec::new(),
                children: Vec::new(),
                attributes: Attributes::default(),
                origin: Some(SourceOrigin {
                    note,
                    revision,
                    bytes: range.clone(),
                }),
            });
            index += 1;
        }
    }
    Document {
        note,
        revision,
        blocks,
        metadata: Vec::new(),
        links,
        read_only: false,
    }
}

/// Scan only parser-designated HTML, respecting quoted tags and raw-text elements.
/// Regions never extend into subsequent Markdown or fenced/inline code.
fn html_comment_ranges(html: &str) -> Vec<Range<usize>> {
    let bytes = html.as_bytes();
    let mut comments = Vec::new();
    let mut cursor = 0;
    let mut raw_element: Option<Vec<u8>> = None;
    while cursor < bytes.len() {
        if let Some(name) = raw_element.as_ref() {
            if bytes[cursor..].starts_with(b"</")
                && bytes
                    .get(cursor + 2..cursor + 2 + name.len())
                    .is_some_and(|tag| tag.eq_ignore_ascii_case(name))
                && bytes
                    .get(cursor + 2 + name.len())
                    .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
            {
                raw_element = None;
            } else {
                cursor += 1;
                continue;
            }
        }
        if bytes[cursor..].starts_with(b"<!--") {
            let end = bytes[cursor + 2..]
                .windows(3)
                .position(|w| w == b"-->")
                .map_or(bytes.len(), |end| cursor + 2 + end + 3);
            comments.push(cursor..end);
            cursor = end;
        } else if bytes[cursor..].starts_with(b"<![CDATA[") {
            cursor = bytes[cursor + 9..]
                .windows(3)
                .position(|w| w == b"]]>")
                .map_or(bytes.len(), |end| cursor + 9 + end + 3);
        } else if bytes[cursor] == b'<' {
            let start = cursor;
            cursor += 1;
            let name_start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_alphanumeric() {
                cursor += 1;
            }
            let name = &bytes[name_start..cursor];
            let exact_tag_name = bytes
                .get(cursor)
                .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'>' | b'/'));
            let mut quote = None;
            while cursor < bytes.len() {
                let byte = bytes[cursor];
                cursor += 1;
                match (quote, byte) {
                    (Some(q), b) if q == b => quote = None,
                    (None, b'\'' | b'"') => quote = Some(byte),
                    (None, b'>') => break,
                    _ => {}
                }
            }
            if exact_tag_name
                && !bytes[start..cursor].ends_with(b"/>")
                && [
                    b"script".as_slice(),
                    b"style",
                    b"textarea",
                    b"title",
                    b"xmp",
                    b"iframe",
                    b"noembed",
                    b"noframes",
                    b"plaintext",
                ]
                .iter()
                .any(|raw| name.eq_ignore_ascii_case(raw))
            {
                raw_element = Some(name.to_vec());
            }
        } else {
            cursor += 1;
        }
    }
    comments
}

/// Used for raw HTML emitted by optional processors, never on code text.
pub(crate) fn only_html_comments(text: &str) -> bool {
    let mut cursor = 0;
    for comment in html_comment_ranges(text) {
        if !text[cursor..comment.start].trim().is_empty() {
            return false;
        }
        cursor = comment.end;
    }
    text[cursor..].trim().is_empty()
}

fn read_inlines(events: &[(Event<'_>, Range<usize>)]) -> Vec<Inline> {
    let mut result = Vec::new();
    let mut link: Option<(String, String, bool)> = None;
    for (event, _) in events {
        match event {
            Event::Start(Tag::Link {
                dest_url,
                link_type,
                ..
            }) => {
                link = Some((
                    dest_url.to_string(),
                    String::new(),
                    matches!(link_type, LinkType::WikiLink { .. }),
                ));
            }
            Event::End(TagEnd::Link) => {
                if let Some((target, label, wiki)) = link.take() {
                    result.push(Inline::Link {
                        target,
                        label,
                        wiki,
                    });
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, label, _)) = &mut link {
                    label.push_str(text);
                } else if matches!(event, Event::Code(_)) {
                    result.push(Inline::Code(text.to_string()));
                } else {
                    result.push(Inline::Text(text.to_string()));
                }
            }
            Event::Start(Tag::Emphasis) => result.push(Inline::EmphasisStart),
            Event::End(TagEnd::Emphasis) => result.push(Inline::EmphasisEnd),
            Event::Start(Tag::Strong) => result.push(Inline::StrongStart),
            Event::End(TagEnd::Strong) => result.push(Inline::StrongEnd),
            Event::SoftBreak | Event::HardBreak => result.push(Inline::Break),
            _ => {}
        }
    }
    result
}

fn raw_fence_info(source: &str) -> &str {
    let first = source.lines().next().unwrap_or_default().trim_start();
    let delimiter = first.chars().next().unwrap_or('`');
    first.trim_start_matches(delimiter)
}

/// Supported subset: plain language, or `{#id .class key="value" key=value}`.
/// Quotes may contain spaces; escapes, nested braces and duplicate ids are rejected.
/// Raw info remains available even when this returns None.
pub fn fence_attributes(info: &str) -> Option<Attributes> {
    let info = info.trim();
    if !info.starts_with('{') {
        let mut attrs = Attributes::default();
        if let Some(language) = info.split_whitespace().next() {
            attrs.classes.push(language.to_owned());
        }
        return Some(attrs);
    }
    let inner = info.strip_prefix('{')?.strip_suffix('}')?;
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    for ch in inner.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                token.push(ch);
            }
            '\\' | '{' | '}' => return None,
            ch if ch.is_whitespace() && !quoted => {
                if !token.is_empty() {
                    tokens.push(std::mem::take(&mut token));
                }
            }
            _ => token.push(ch),
        }
    }
    if quoted {
        return None;
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    let mut attrs = Attributes::default();
    for token in tokens {
        if let Some(id) = token.strip_prefix('#') {
            if id.is_empty() || attrs.id.is_some() {
                return None;
            }
            attrs.id = Some(id.to_owned());
        } else if let Some(class) = token.strip_prefix('.') {
            if class.is_empty() {
                return None;
            }
            attrs.classes.push(class.to_owned());
        } else {
            let (key, value) = token.split_once('=')?;
            if key.is_empty() || value.is_empty() {
                return None;
            }
            let value = if value.starts_with('"') {
                value.strip_prefix('"')?.strip_suffix('"')?
            } else {
                if value.contains('"') {
                    return None;
                }
                value
            };
            attrs.values.push((key.to_owned(), value.to_owned()));
        }
    }
    Some(attrs)
}

#[cfg(test)]
mod reader_comment_tests {
    use super::*;
    #[test]
    fn html_comments_are_hidden_without_touching_code_or_source_ranges() {
        let source = "<!-- source: lesson.ash -->\n\nHello <!-- hidden --> world.\n\n- item <!-- list comment -->\n\n`<!-- literal -->`\n\n```html\n<!-- code literal -->\n```\n";
        let doc = read(
            NoteId(uuid::Uuid::new_v4()),
            Revision(uuid::Uuid::new_v4()),
            source,
        );
        for html in [
            "<div title=\"<!--\">kept</div>\n\n# Kept heading\n",
            "<script>let marker = \"<!--\";</script>\n\n# Kept heading\n",
        ] {
            let parsed = read(doc.note, doc.revision, html);
            assert!(matches!(
                parsed.blocks.last().unwrap().kind,
                BlockKind::Heading(1)
            ));
            assert!(
                matches!(&parsed.blocks[0].kind, BlockKind::Source(text) if text.contains("<!--"))
            );
        }
        assert_eq!(
            html_comment_ranges("<div title=\"<!--\">keep <!-- real --> text</div>"),
            vec![23..36]
        );
        for short in ["<!-->", "<!--->"] {
            assert!(only_html_comments(short));
            assert!(!only_html_comments(&format!("{short}kept")));
        }
        let custom = "<script-example><!-- hidden --></script-example>";
        let ranges = html_comment_ranges(custom);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&custom[ranges[0].clone()], "<!-- hidden -->");
        assert!(only_html_comments("<!-- first -->\n<!-- second -->"));
        assert!(!only_html_comments(
            "<!-- first --><div>visible</div><!-- last -->"
        ));
        assert_eq!(doc.blocks.len(), 4);
        assert_eq!(
            doc.blocks[0].inlines,
            vec![
                Inline::Text("Hello ".into()),
                Inline::Text(" world.".into())
            ]
        );
        assert!(matches!(&doc.blocks[1].kind, BlockKind::Source(s) if !s.contains("list comment")));
        assert_eq!(
            doc.blocks[2].inlines,
            vec![Inline::Code("<!-- literal -->".into())]
        );
        assert!(
            matches!(&doc.blocks[3].kind, BlockKind::Fence(f) if f.body.contains("<!-- code literal -->"))
        );
        let range = &doc.blocks[0].origin.as_ref().unwrap().bytes;
        assert_eq!(
            source[range.clone()].trim_end(),
            "Hello <!-- hidden --> world."
        );
    }
}
