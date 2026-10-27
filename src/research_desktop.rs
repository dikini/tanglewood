//! Pure, bounded desktop research helpers. Evidence remains authored source text.
use crate::research::ResearchWorkspace;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct ReferenceCandidate {
    pub title: String,
    pub url: String,
    pub author: Option<String>,
    pub date: Option<String>,
}

/// Canonical HTTP provenance; no credentials, fragments or tracking parameters.
pub fn normalize_url(input: &str) -> Option<String> {
    let mut url = Url::parse(input.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    url.set_fragment(None);
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| {
            !key.to_ascii_lowercase().starts_with("utm_")
                && !matches!(key.as_ref(), "fbclid" | "gclid")
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    match url.host_str()? {
        "doi.org" | "dx.doi.org" => {
            let path = url.path().to_lowercase();
            url.set_host(Some("doi.org")).ok()?;
            url.set_scheme("https").ok()?;
            url.set_path(&path);
        }
        "arxiv.org" | "www.arxiv.org" | "export.arxiv.org" => {
            let path = url.path().trim_end_matches(".pdf").to_owned();
            let id = path
                .strip_prefix("/abs/")
                .or_else(|| path.strip_prefix("/pdf/"));
            if let Some(id) = id {
                url.set_host(Some("arxiv.org")).ok()?;
                url.set_scheme("https").ok()?;
                url.set_path(&format!("/abs/{id}"));
                url.set_query(None);
            }
        }
        _ => {}
    }
    Some(url.into())
}

/// Collect references in appearance order; link labels are retained, never inferred metadata.
pub fn references(markdown: &str) -> Vec<ReferenceCandidate> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut link: Option<(String, String)> = None;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                link = Some((dest_url.into_string(), String::new()))
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, label)) = &mut link {
                    label.push_str(&text);
                } else {
                    for token in text.split_whitespace() {
                        let token = token
                            .trim_matches(|c: char| matches!(c, '<' | '>' | '"' | '\'' | '(' | ')'))
                            .trim_end_matches(['.', ',', ';', ':', '!', '?']);
                        add_reference(&mut result, &mut seen, token, token);
                    }
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some((url, label)) = link.take() {
                    add_reference(&mut result, &mut seen, &url, &label);
                }
            }
            _ => {}
        }
        if result.len() == 64 {
            break;
        }
    }
    result
}
fn add_reference(
    out: &mut Vec<ReferenceCandidate>,
    seen: &mut HashSet<String>,
    raw: &str,
    title: &str,
) {
    if out.len() >= 64 || raw.len() > 8192 {
        return;
    }
    if let Some(url) = normalize_url(raw) {
        if seen.insert(url.clone()) {
            out.push(ReferenceCandidate {
                title: prefix(
                    if title.trim().is_empty() {
                        &url
                    } else {
                        title.trim()
                    },
                    512,
                )
                .to_owned(),
                url,
                author: None,
                date: None,
            });
        }
    }
}
fn prefix(text: &str, bytes: usize) -> &str {
    let mut end = bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct Passage {
    pub source_id: Uuid,
    pub page: Option<u32>,
    pub text: String,
    pub citation: String,
    pub score: usize,
}

/// Lexical matches from selected captured pages. Scores measure word overlap, not trust.
pub fn retrieve(workspace: &ResearchWorkspace, query: &str, limit: usize) -> Vec<Passage> {
    let terms: HashSet<_> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .take(64)
        .map(str::to_lowercase)
        .collect();
    if terms.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut passages = Vec::new();
    let mut remaining = 16 * 1024 * 1024;
    for source in workspace
        .sources
        .iter()
        .filter(|s| s.selected && crate::research::source_has_text(s))
        .take(1024)
    {
        for page in source.pages.iter().take(4096) {
            // Non-overlapping UTF-8 chunks preserve exact source bytes and bound work per page.
            let mut rest = prefix(&page.text, remaining.min(2 * 1024 * 1024));
            remaining -= rest.len();
            while !rest.is_empty() {
                let chunk = prefix(rest, 1800);
                let text = if rest.len() > chunk.len() {
                    chunk
                        .rfind(char::is_whitespace)
                        .filter(|end| *end > 900)
                        .map_or(chunk, |end| {
                            &chunk[..end + chunk[end..].chars().next().map_or(0, char::len_utf8)]
                        })
                } else {
                    chunk
                };
                let score = text
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|word| terms.contains(&word.to_lowercase()))
                    .count();
                if score > 0 {
                    let citation = match page.number {
                        Some(n) => format!("[source:{} page:{n}]", source.id),
                        None => format!("[source:{}]", source.id),
                    };
                    passages.push(Passage {
                        source_id: source.id,
                        page: page.number,
                        text: text.to_owned(),
                        citation,
                        score,
                    });
                    passages.sort_by(|a, b| b.score.cmp(&a.score));
                    passages.truncate(limit.min(16));
                }
                rest = &rest[text.len()..];
            }
        }
    }
    passages
}

/// Grounded follow-up suggestions, never inferred verification or agreement.
pub fn evidence_gaps(workspace: &ResearchWorkspace) -> Vec<String> {
    let selected: Vec<_> = workspace.sources.iter().filter(|s| s.selected).collect();
    let mut gaps = Vec::new();
    if selected.is_empty() {
        gaps.push("Select sources to prepare evidence.".into());
        return gaps;
    }
    let empty = selected
        .iter()
        .filter(|s| s.pages.iter().all(|p| p.text.trim().is_empty()))
        .count();
    let extraction = selected
        .iter()
        .filter(|s| {
            s.paper
                .as_ref()
                .is_some_and(|p| p.extraction_error.is_some())
        })
        .count();
    let metadata = selected
        .iter()
        .filter(|s| s.metadata.author.is_none() || s.metadata.date.is_none())
        .count();
    let unassessed = selected
        .iter()
        .filter(|s| s.human_assessment.is_none() && s.agent_assessment.is_none())
        .count();
    let differing = selected
        .iter()
        .filter(|s| matches!((&s.human_assessment,&s.agent_assessment),(Some(a),Some(b)) if a != b))
        .count();
    if empty > 0 {
        gaps.push(format!("{empty} selected sources have no captured text; acquire content before citing their claims."));
    }
    if extraction > 0 {
        gaps.push(format!(
            "{extraction} retained papers report extraction errors; inspect originals or run OCR."
        ));
    }
    if metadata > 0 {
        gaps.push(format!(
            "{metadata} selected sources lack author or date metadata; check provenance."
        ));
    }
    if unassessed > 0 {
        gaps.push(format!("{unassessed} selected sources have no recorded assessment; assess relevance and limitations."));
    }
    if differing > 0 {
        gaps.push(format!("{differing} selected sources have different human and agent assessments; compare those records."));
    }
    gaps.push("Compare cited passages for corroboration and disagreement; source counts alone do not verify claims.".into());
    gaps
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub enum OutputKind {
    Slides,
    Infographic,
    MindMap,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct OutputCard {
    pub heading: String,
    pub body: String,
    pub citations: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct OutputSpec {
    #[serde(default)]
    pub source_urls: BTreeMap<Uuid, String>,
    pub title: String,
    pub kind: OutputKind,
    pub cards: Vec<OutputCard>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct RenderedOutput {
    pub content: String,
    pub extension: String,
    pub mime: String,
}
fn escape(s: &str) -> String {
    s.chars()
        .filter(|c| {
            matches!(c, '\t' | '\n' | '\r') || *c >= ' ' && *c != '\u{fffe}' && *c != '\u{ffff}'
        })
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn citation_parts(citation: &str) -> Option<(Uuid, Option<u32>)> {
    let raw = citation.strip_prefix("[source:")?.strip_suffix(']')?;
    let mut parts = raw.split_whitespace();
    let id = Uuid::parse_str(parts.next()?).ok()?;
    let page = match parts.next() {
        None => None,
        Some(v) => Some(v.strip_prefix("page:")?.parse::<u32>().ok()?),
    };
    if parts.next().is_some() {
        return None;
    }
    Some((id, page))
}
fn valid_target(target: &str) -> bool {
    if target.is_empty() || target.len() > 8192 {
        return false;
    }
    if let Ok(url) = Url::parse(target) {
        return matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none();
    }
    target.len() <= 255
        && target != "."
        && !target.contains("..")
        && target
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_'))
}
fn citation_link(spec: &OutputSpec, citation: &str) -> Option<String> {
    let (id, page) = citation_parts(citation)?;
    let target = spec.source_urls.get(&id)?;
    let is_pdf = Url::parse(target)
        .map(|u| u.path().to_ascii_lowercase().ends_with(".pdf"))
        .unwrap_or_else(|_| target.to_ascii_lowercase().ends_with(".pdf"));
    if let (true, Some(page)) = (is_pdf, page) {
        return Some(format!("{}#page={page}", target.split('#').next()?));
    }
    Some(target.clone())
}
/// Render user/model-supplied data as escaped local artifacts, without remote resources.
pub fn render_output(spec: &OutputSpec) -> Result<RenderedOutput, String> {
    if spec.title.trim().is_empty()
        || spec.title.len() > 512
        || spec.cards.is_empty()
        || spec.cards.len() > 64
    {
        return Err("provide a title and 1–64 cards".into());
    }
    if spec.source_urls.len() > 128
        || spec
            .source_urls
            .values()
            .any(|target| !valid_target(target))
    {
        return Err(
            "source URLs must be credential-free HTTP(S) links or safe local filenames".into(),
        );
    }
    for card in &spec.cards {
        if card.heading.len() > 512
            || card.body.len() > 8192
            || card.citations.len() > 32
            || card
                .citations
                .iter()
                .any(|c| c.len() > 128 || citation_parts(c).is_none())
        {
            return Err(
                "card exceeds text/citation limits or contains an invalid source citation".into(),
            );
        }
    }
    if spec.kind == OutputKind::Slides {
        let mut html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{}</title><style>body{{margin:0;background:#171717;color:#eee;font:24px system-ui}}main{{max-width:1000px;margin:auto}}section{{min-height:85vh;padding:5vh 5vw;border-bottom:1px solid #444;scroll-snap-align:start;box-sizing:border-box}}h1{{font-size:2em}}p{{white-space:pre-wrap;line-height:1.6;overflow-wrap:anywhere}}a{{color:#a7caff;font-size:14px}}@media print{{section{{break-after:page}}}}</style></head><body><main><header><h1>{}</h1></header>",
            escape(&spec.title),
            escape(&spec.title)
        );
        for card in &spec.cards {
            html.push_str(&format!(
                "<section><h2>{}</h2><p>{}</p>",
                escape(&card.heading),
                escape(&card.body)
            ));
            for citation in &card.citations {
                if let Some(link) = citation_link(spec, citation) {
                    html.push_str(&format!(
                        "<p><a href=\"{}\">{}</a></p>",
                        escape(&link),
                        escape(citation)
                    ));
                } else {
                    html.push_str(&format!("<p>{}</p>", escape(citation)));
                }
            }
            html.push_str("</section>");
        }
        html.push_str("</main></body></html>");
        return Ok(RenderedOutput {
            content: html,
            extension: "html".into(),
            mime: "text/html".into(),
        });
    }
    render_svg(spec)
}
fn wrap(text: &str, width: usize) -> Vec<String> {
    // Every source character is retained; wrapping never silently clips output content.
    text.split('\n')
        .flat_map(|line| {
            let chars: Vec<_> = line.chars().collect();
            if chars.is_empty() {
                vec![String::new()]
            } else {
                chars
                    .chunks(width)
                    .map(|part| part.iter().collect())
                    .collect()
            }
        })
        .collect()
}
fn svg_text(out: &mut String, text: &str, x: usize, y: usize, size: usize, width: usize) -> usize {
    let lines = wrap(text, width);
    for (i, line) in lines.iter().enumerate() {
        out.push_str(&format!(
            "<text x=\"{x}\" y=\"{}\" font-size=\"{size}\">{}</text>",
            y + i * (size + 8),
            escape(line)
        ));
    }
    lines.len() * (size + 8)
}
fn render_svg(spec: &OutputSpec) -> Result<RenderedOutput, String> {
    let mindmap = spec.kind == OutputKind::MindMap;
    let x = if mindmap { 380 } else { 40 };
    let width = if mindmap { 1120 } else { 900 };
    let mut content = String::new();
    let title_lines = wrap(&spec.title, if mindmap { 18 } else { 55 }).len();
    let title_height = title_lines * 32 + 60;
    let mut y = if mindmap { 100 } else { title_height };
    for card in &spec.cards {
        let heading = wrap(&card.heading, 42).len() * 30;
        let body = wrap(&card.body, 60).len() * 26;
        let height = heading + body + card.citations.len() * 24 + 48;
        if mindmap {
            content.push_str(&format!(
                "<path d=\"M 300 90 H 340 V {} H {x}\" stroke=\"#7b9ec2\" fill=\"none\"/>",
                y + 24
            ));
        }
        content.push_str(&format!("<rect x=\"{x}\" y=\"{y}\" width=\"700\" height=\"{height}\" rx=\"12\" fill=\"#242830\"/>"));
        let mut line_y = y + 30;
        line_y += svg_text(&mut content, &card.heading, x + 20, line_y, 22, 42);
        line_y += svg_text(&mut content, &card.body, x + 20, line_y, 18, 60);
        for citation in &card.citations {
            if let Some(link) = citation_link(spec, citation) {
                content.push_str(&format!("<a href=\"{}\"><text x=\"{}\" y=\"{line_y}\" font-size=\"12\" fill=\"#a7caff\">{}</text></a>",escape(&link),x+20,escape(citation)));
            } else {
                content.push_str(&format!(
                    "<text x=\"{}\" y=\"{line_y}\" font-size=\"12\">{}</text>",
                    x + 20,
                    escape(citation)
                ));
            }
            line_y += 24;
        }
        y += height + 24;
    }
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{}\" viewBox=\"0 0 {width} {}\"><title>{}</title><rect width=\"100%\" height=\"100%\" fill=\"#171717\"/><g fill=\"#eeeeee\" font-family=\"sans-serif\">",
        (y + 20).max(title_height),
        (y + 20).max(title_height),
        escape(&spec.title)
    );
    svg_text(
        &mut svg,
        &spec.title,
        40,
        40,
        24,
        if mindmap { 18 } else { 55 },
    );
    svg.push_str(&content);
    svg.push_str("</g></svg>");
    Ok(RenderedOutput {
        content: svg,
        extension: "svg".into(),
        mime: "image/svg+xml".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_deduplicate_canonical_provenance_and_reject_credentials() {
        let found = references(
            "[Paper](http://dx.doi.org/10.1/ABC?utm_source=x) <https://doi.org/10.1/abc> https://arxiv.org/pdf/2401.12345v2.pdf https://arxiv.org/abs/2401.12345v2 https://arxiv.org/abs/2401.12345v1 https://user:secret@example.org/a",
        );
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].title, "Paper");
        assert_eq!(found[1].url, "https://arxiv.org/abs/2401.12345v2");
        assert_eq!(found[2].url, "https://arxiv.org/abs/2401.12345v1");
    }
    #[test]
    fn retrieval_keeps_exact_unicode_and_excludes_unselected() {
        use crate::research::*;
        let source = ResearchSource {
            id: Uuid::new_v4(),
            title: "test".into(),
            kind: SourceKind::Text,
            origin: SourceOrigin::Imported,
            metadata: SourceMetadata::default(),
            pages: vec![SourcePage {
                number: Some(4),
                text: format!("{} évidence", "é ".repeat(2000)),
            }],
            paper: None,
            selected: true,
            human_assessment: None,
            agent_assessment: None,
        };
        let mut workspace = ResearchWorkspace {
            id: Uuid::new_v4(),
            revision: crate::notes::Revision(Uuid::new_v4()),
            title: "test".into(),
            question: String::new(),
            sources: vec![source],
            research_notes: vec![],
            brief: ResearchBrief::default(),
            threads: vec![],
            rounds: vec![],
            artifacts: vec![],
            acquisitions: vec![],
            assessment_history: vec![],
        };
        let hits = retrieve(&workspace, "évidence", 99);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.len() <= 1800);
        assert!(workspace.sources[0].pages[0].text.contains(&hits[0].text));
        assert!(hits[0].citation.ends_with("page:4]"));
        workspace.sources[0].selected = false;
        assert!(retrieve(&workspace, "évidence", 16).is_empty());
    }
    #[test]
    fn output_escapes_markup_and_rejects_active_citations() {
        let mut spec = OutputSpec {
            source_urls: BTreeMap::from([(Uuid::nil(), "paper.pdf".into())]),
            title: "<script>".into(),
            kind: OutputKind::Slides,
            cards: vec![OutputCard {
                heading: "<img>".into(),
                body: "A & B".into(),
                citations: vec![format!("[source:{} page:2]", Uuid::nil())],
            }],
        };
        for kind in [
            OutputKind::Slides,
            OutputKind::Infographic,
            OutputKind::MindMap,
        ] {
            spec.kind = kind;
            let rendered = render_output(&spec).unwrap();
            assert!(!rendered.content.contains("<script>"));
            assert!(rendered.content.contains("A &amp; B"));
            assert!(rendered.content.contains("href=\"paper.pdf#page=2\""));
            assert!(!rendered.content.contains("source://"));
        }
        spec.source_urls.clear();
        let text_only = render_output(&spec).unwrap();
        assert!(!text_only.content.contains("href="));
        assert!(text_only.content.contains("[source:"));
        for target in [
            "../paper.pdf",
            "https://user:secret@example.org/p.pdf",
            "javascript:alert(1)",
        ] {
            spec.source_urls.insert(Uuid::nil(), target.into());
            assert!(render_output(&spec).is_err());
        }
        spec.source_urls.clear();
        spec.cards[0].citations = vec!["javascript:alert(1)".into()];
        assert!(render_output(&spec).is_err());
    }
}
