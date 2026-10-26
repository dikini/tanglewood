//! Named offline saved-artwork preparation subset of illustration-md 9f38098.
//! Inputs are immutable owned bytes, not filesystem paths or authoring programs.
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use quick_xml::{
    events::Event as XmlEvent,
    name::{Namespace, ResolveResult},
    reader::NsReader,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::LazyLock,
};
use yaml_rust2::{Yaml, YamlLoader};

static CSS_ESCAPES: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\\([0-9a-fA-F]{1,6})\s?|\\([^\r\n])").expect("static CSS escapes")
});
static CSS_IMPORTS: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)@import\b|@font-face\b").expect("static CSS imports"));
static CSS_URLS: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?is)url\(\s*([^)]*)\)").expect("static CSS URLs"));
const SOURCE_LIMIT: usize = 1024 * 1024;
const META_LIMIT: usize = 64 * 1024;
const FILE_LIMIT: usize = 64 * 1024 * 1024;
const TOTAL_LIMIT: usize = 80 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct AssetFile {
    pub relative_path: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct PreparationInput {
    pub source: String,
    pub logical_path: String,
    pub assets: Vec<AssetFile>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtworkKind {
    Illustration,
    Comic,
}
impl ArtworkKind {
    pub(crate) fn media_type(self) -> &'static str {
        match self {
            Self::Illustration => "image/svg+xml",
            Self::Comic => "image/png",
        }
    }
    fn extension(self) -> &'static str {
        match self {
            Self::Illustration => "svg",
            Self::Comic => "png",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Illustration => "illustration",
            Self::Comic => "comic",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreparedFigure {
    pub id: String,
    pub kind: ArtworkKind,
    pub caption: String,
    pub description: String,
    pub width_mm: Option<String>,
    pub source_bytes: Range<usize>,
    pub asset_path: String,
    pub asset_sha256: String,
    pub dimensions: [f64; 2],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreparedIllustrations {
    pub figures: Vec<PreparedFigure>,
    pub metadata: IllustrationMetadata,
}

/// Derived processor output, never serialized back over authored front matter.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IllustrationMetadata {
    pub illustrations: BTreeMap<String, RenderedImage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedImage {
    /// Exact document-relative path, not a directory-discovery hint or URL.
    pub image: String,
    #[serde(rename = "media-type")]
    pub media_type: String,
}

impl IllustrationMetadata {
    pub fn from_pandoc(document: &serde_json::Value) -> Result<Option<Self>, PreparationError> {
        let Some(value) = document
            .get("meta")
            .and_then(|meta| meta.get("illustrations"))
        else {
            return Ok(None);
        };
        fn map(
            value: &serde_json::Value,
        ) -> Result<&serde_json::Map<String, serde_json::Value>, PreparationError> {
            if value["t"] != "MetaMap" {
                return Err(metadata("illustrations requires MetaMap"));
            }
            value["c"]
                .as_object()
                .ok_or_else(|| metadata("invalid MetaMap"))
        }
        fn string(value: &serde_json::Value) -> Result<String, PreparationError> {
            if value["t"] == "MetaString" {
                return value["c"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| metadata("invalid MetaString"));
            }
            if value["t"] == "MetaInlines" {
                let mut text = String::new();
                for inline in value["c"]
                    .as_array()
                    .ok_or_else(|| metadata("invalid MetaInlines"))?
                {
                    match inline["t"].as_str() {
                        Some("Str") => text.push_str(
                            inline["c"]
                                .as_str()
                                .ok_or_else(|| metadata("invalid Str"))?,
                        ),
                        Some("Space") => text.push(' '),
                        _ => return Err(metadata("image references require literal metadata")),
                    }
                }
                return Ok(text);
            }
            Err(metadata("image references require string metadata"))
        }
        let values = map(value)?;
        if values.len() > 64 {
            return Err(quota("image references"));
        }
        let mut result = Self::default();
        for (id, value) in values {
            if !stable_id(id) {
                return Err(metadata("stable fig- identifier required"));
            }
            let fields = map(value)?;
            let image = string(
                fields
                    .get("image")
                    .ok_or_else(|| metadata("image reference required"))?,
            )?;
            safe_path(&image)?;
            let media_type = string(
                fields
                    .get("media-type")
                    .ok_or_else(|| metadata("media-type required"))?,
            )?;
            if !matches!(media_type.as_str(), "image/svg+xml" | "image/png") {
                return Err(metadata("unsupported image media-type"));
            }
            result
                .illustrations
                .insert(id.clone(), RenderedImage { image, media_type });
        }
        Ok(Some(result))
    }

    /// Retain unrelated metadata and additional fields on existing image entries.
    pub fn write_pandoc(&self, document: &mut serde_json::Value) {
        use serde_json::json;
        if self.illustrations.is_empty() {
            return;
        }
        let meta = &mut document["meta"];
        if meta.get("illustrations").is_none() {
            meta["illustrations"] = json!({"t":"MetaMap","c":{}});
        }
        for (id, image) in &self.illustrations {
            let entry = &mut meta["illustrations"]["c"][id];
            if entry.is_null() {
                *entry = json!({"t":"MetaMap","c":{}});
            }
            entry["c"]["image"] = json!({"t":"MetaString","c":image.image});
            entry["c"]["media-type"] = json!({"t":"MetaString","c":image.media_type});
        }
    }
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PreparationError {
    #[error("preparation quota exceeded: {0}")]
    Quota(String),
    #[error("unsupported preparation syntax: {0}")]
    Unsupported(String),
    #[error("invalid illustration metadata: {0}")]
    Metadata(String),
    #[error("invalid asset path: {0}")]
    Path(String),
    #[error("missing saved artwork: {0}")]
    Missing(String),
    #[error("invalid saved artwork: {0}")]
    Invalid(String),
}
fn invalid(error: impl std::fmt::Display) -> PreparationError {
    PreparationError::Invalid(error.to_string())
}
fn metadata(error: impl std::fmt::Display) -> PreparationError {
    PreparationError::Metadata(error.to_string())
}
fn quota(what: &str) -> PreparationError {
    PreparationError::Quota(what.into())
}

/// Prepare only saved illustrations/comics. Does not mutate source or assets.
/// Filesystem containment/symlink validation belongs to the snapshot materializer;
/// this pure adapter validates logical map keys and reference aliases only.
pub fn prepare(input: PreparationInput) -> Result<PreparedIllustrations, PreparationError> {
    prepare_with_metadata(input, None)
}

/// An explicit reference map is authoritative. Never fall back to sidecars when
/// a processor supplied an incomplete, missing or invalid image reference.
pub fn prepare_with_metadata(
    input: PreparationInput,
    references: Option<&IllustrationMetadata>,
) -> Result<PreparedIllustrations, PreparationError> {
    if input.source.len() > SOURCE_LIMIT || input.assets.len() > 128 {
        return Err(quota("source or file count"));
    }
    safe_path(&input.logical_path)?;
    let parent = input
        .logical_path
        .rsplit_once('/')
        .map(|(parent, _)| format!("{parent}/"))
        .unwrap_or_default();
    let mut assets = BTreeMap::new();
    let mut total = 0usize;
    for file in input.assets {
        safe_path(&file.relative_path)?;
        total = total
            .checked_add(file.bytes.len())
            .ok_or_else(|| quota("asset bytes"))?;
        if file.bytes.len() > FILE_LIMIT || total > TOTAL_LIMIT {
            return Err(quota("asset bytes"));
        }
        if assets
            .insert(file.relative_path.clone(), file.bytes)
            .is_some()
        {
            return Err(PreparationError::Path(format!(
                "duplicate {}",
                file.relative_path
            )));
        }
    }
    let descriptions = descriptions(&input.source)?;
    let mut figures = Vec::new();
    let mut output_metadata = IllustrationMetadata::default();
    let mut used = BTreeSet::new();
    let mut current = None;
    // Offset events traverse nested lists/quotes too; top-level render wrappers
    // cannot hide supported fences from this scan.
    let options = Options::all();
    for (event, range) in Parser::new_ext(&input.source, options).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let attributes = crate::markdown::fence_attributes(&info)
                    .ok_or_else(|| PreparationError::Unsupported("fence attributes".into()))?;
                let kinds: Vec<_> = attributes
                    .classes
                    .iter()
                    .filter_map(|class| match class.as_str() {
                        "illustration" => Some(ArtworkKind::Illustration),
                        "comic" => Some(ArtworkKind::Comic),
                        _ => None,
                    })
                    .collect();
                if kinds.len() > 1 {
                    return Err(metadata("a fence cannot be both illustration and comic"));
                }
                if let Some(kind) = kinds.first() {
                    current = Some((attributes.id.unwrap_or_default(), *kind, range.start));
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((id, kind, start)) = current.take() {
                    if !stable_id(&id) {
                        return Err(metadata("stable fig- identifier required"));
                    }
                    if !used.insert(id.clone()) {
                        return Err(metadata(format!("duplicate illustration fence: {id}")));
                    }
                    let description = descriptions.get(&id).ok_or_else(|| {
                        metadata(format!("illustration fence has no matching metadata: {id}"))
                    })?;
                    if description.kind != kind {
                        return Err(metadata(format!("fence and metadata kind disagree: {id}")));
                    }
                    let image = if let Some(references) = references {
                        Some(references.illustrations.get(&id).ok_or_else(|| {
                            metadata(format!("image reference required for {id}"))
                        })?)
                    } else {
                        description.image.as_ref()
                    };
                    let asset_path = if let Some(image) = image {
                        safe_path(&image.image)?;
                        if image.media_type != kind.media_type() {
                            return Err(metadata(format!("image media-type disagrees with {id}")));
                        }
                        image.image.clone()
                    } else {
                        let directory = format!("{parent}assets/{id}/");
                        let sidecar = format!("{directory}{}.json", kind.name());
                        let bytes = assets
                            .get(&sidecar)
                            .ok_or_else(|| PreparationError::Missing(sidecar.clone()))?;
                        let state: serde_json::Value =
                            serde_json::from_slice(bytes).map_err(invalid)?;
                        if state.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
                            return Err(invalid("sidecar needs integer version 1"));
                        }
                        let reference = state
                            .get(kind.extension())
                            .and_then(serde_json::Value::as_str)
                            .ok_or_else(|| invalid("missing kind-specific artwork reference"))?;
                        let target = reference_path(reference)?;
                        if !target
                            .rsplit('.')
                            .next()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case(kind.extension()))
                        {
                            return Err(PreparationError::Path(reference.into()));
                        }
                        format!("assets/{id}/{target}")
                    };
                    let snapshot_path = format!("{parent}{asset_path}");
                    let data = assets
                        .get(&snapshot_path)
                        .ok_or_else(|| PreparationError::Missing(snapshot_path.clone()))?;
                    let dimensions = match kind {
                        ArtworkKind::Illustration => validate_svg(data)?,
                        ArtworkKind::Comic => validate_png(data)?.map(f64::from),
                    };
                    output_metadata.illustrations.insert(
                        id.clone(),
                        RenderedImage {
                            image: asset_path.clone(),
                            media_type: kind.media_type().into(),
                        },
                    );
                    figures.push(PreparedFigure {
                        id,
                        kind,
                        caption: description.caption.clone(),
                        description: description.description.clone(),
                        width_mm: description.width.clone(),
                        source_bytes: start..range.end,
                        asset_path,
                        asset_sha256: Sha256::digest(data)
                            .iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect(),
                        dimensions,
                    });
                    if figures.len() > 64 {
                        return Err(quota("figures"));
                    }
                }
            }
            _ => {}
        }
    }
    if used.len() != descriptions.len() {
        return Err(metadata("unused illustration metadata"));
    }
    if references.is_some_and(|references| references.illustrations.len() != used.len()) {
        return Err(metadata("unused illustration image references"));
    }
    Ok(PreparedIllustrations {
        figures,
        metadata: output_metadata,
    })
}
fn stable_id(id: &str) -> bool {
    id.strip_prefix("fig-").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.split('-').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            })
    })
}
fn safe_path(path: &str) -> Result<(), PreparationError> {
    if path.is_empty()
        || path.len() > 1024
        || path
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | ':' | '?' | '#' | '%'))
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(PreparationError::Path(path.into()));
    }
    Ok(())
}
fn reference_path(reference: &str) -> Result<String, PreparationError> {
    // Decode once then reject aliases rather than normalizing multiple keys.
    let decoded = percent_encoding::percent_decode_str(reference)
        .decode_utf8()
        .map_err(|_| PreparationError::Path(reference.into()))?;
    safe_path(&decoded)?;
    Ok(decoded.into_owned())
}
struct Description {
    kind: ArtworkKind,
    caption: String,
    description: String,
    width: Option<String>,
    image: Option<RenderedImage>,
}

/// Saved references only; callers decide how to obtain the explicitly named bytes.
pub fn saved_image_references(source: &str) -> Result<IllustrationMetadata, PreparationError> {
    Ok(IllustrationMetadata {
        illustrations: descriptions(source)?
            .into_iter()
            .filter_map(|(id, description)| description.image.map(|image| (id, image)))
            .collect(),
    })
}
fn literal(value: &Yaml) -> Result<String, PreparationError> {
    let value = value
        .as_str()
        .ok_or_else(|| PreparationError::Unsupported("literal string metadata required".into()))?;
    let mut text = String::new();
    for event in Parser::new(value) {
        match event {
            Event::Text(s) => text.push_str(&s),
            Event::SoftBreak | Event::HardBreak => text.push(' '),
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) => {}
            _ => {
                return Err(PreparationError::Unsupported(
                    "formatted metadata is not literal text".into(),
                ));
            }
        }
    }
    Ok(text)
}
fn descriptions(source: &str) -> Result<BTreeMap<String, Description>, PreparationError> {
    let mut result = BTreeMap::new();
    let Some(rest) = source.strip_prefix("---\n") else {
        return Ok(result);
    };
    let Some(end) = rest.find("\n---\n") else {
        return Err(metadata("unterminated frontmatter"));
    };
    let header = &rest[..end];
    if header.len() > META_LIMIT {
        return Err(quota("frontmatter"));
    }
    // This deliberately narrow reader does not expand aliases/tags or interpret
    // arbitrary Markdown in YAML; unsupported input stays available as source.
    if header.chars().any(|c| matches!(c, '&' | '*' | '!')) {
        return Err(PreparationError::Unsupported(
            "YAML anchors, aliases, tags and formatted metadata are outside subset".into(),
        ));
    }
    let documents = YamlLoader::load_from_str(header).map_err(metadata)?;
    if documents.len() != 1 {
        return Err(metadata("one frontmatter mapping required"));
    }
    let diagrams = &documents[0]["diagrams"];
    if diagrams.is_badvalue() {
        return Ok(result);
    }
    let diagrams = diagrams
        .as_vec()
        .ok_or_else(|| metadata("diagrams metadata must be a list"))?;
    if diagrams.len() > 64 {
        return Err(quota("diagram descriptions"));
    }
    for item in diagrams {
        let fields = item
            .as_hash()
            .ok_or_else(|| metadata("each diagram description must be a map"))?;
        let kind = match literal(&item["kind"])?.as_str() {
            "illustration" => ArtworkKind::Illustration,
            "comic" => ArtworkKind::Comic,
            _ => continue,
        };
        // Upstream requires every selected description field to be literal.
        for (key, value) in fields {
            if matches!(key.as_str(), Some("image" | "media-type")) {
                value
                    .as_str()
                    .ok_or_else(|| metadata("image references require strings"))?;
            } else {
                literal(value)?;
            }
        }
        let id = literal(&item["id"])?;
        let caption = literal(&item["caption"])?;
        let description = literal(&item["description"])?;
        let image = match (
            item["image"].is_badvalue(),
            item["media-type"].is_badvalue(),
        ) {
            (true, true) => None,
            (false, false) => {
                let image = item["image"]
                    .as_str()
                    .ok_or_else(|| metadata("image requires a string"))?;
                safe_path(image)?;
                let media_type = item["media-type"]
                    .as_str()
                    .ok_or_else(|| metadata("media-type requires a string"))?;
                if media_type != kind.media_type() {
                    return Err(metadata(
                        "saved image media-type disagrees with diagram kind",
                    ));
                }
                Some(RenderedImage {
                    image: image.into(),
                    media_type: media_type.into(),
                })
            }
            _ => return Err(metadata("saved image requires both image and media-type")),
        };
        if id.trim().is_empty() || caption.trim().is_empty() || description.trim().is_empty() {
            return Err(metadata("id, caption and description required"));
        }
        let width = if item["width"].is_badvalue() {
            None
        } else {
            let width = literal(&item["width"])?;
            let valid = regex::Regex::new(r"^(?:\d+(?:\.\d+)?|\.\d+)mm$")
                .map_err(metadata)?
                .is_match(&width)
                && width[..width.len() - 2]
                    .parse::<f64>()
                    .is_ok_and(|n| n.is_finite() && n > 0.0);
            if !valid {
                return Err(metadata("positive finite millimetre width required"));
            }
            Some(width)
        };
        if result
            .insert(
                id.clone(),
                Description {
                    kind,
                    caption,
                    description,
                    width,
                    image,
                },
            )
            .is_some()
        {
            return Err(metadata(format!("duplicate illustration metadata: {id}")));
        }
    }
    Ok(result)
}
fn css_resources(css: &str) -> Result<(), PreparationError> {
    let decoded = CSS_ESCAPES.replace_all(css, |captures: &regex::Captures<'_>| {
        captures
            .get(1)
            .and_then(|m| u32::from_str_radix(m.as_str(), 16).ok())
            .and_then(char::from_u32)
            .map(|c| c.to_string())
            .unwrap_or_else(|| {
                captures
                    .get(2)
                    .map(|m| m.as_str().to_owned())
                    .unwrap_or_default()
            })
    });
    if CSS_IMPORTS.is_match(&decoded) {
        return Err(invalid("CSS imports or reader fonts"));
    }
    for resource in CSS_URLS.captures_iter(&decoded) {
        if !resource[1]
            .trim()
            .trim_matches(['\'', '"'])
            .trim()
            .starts_with('#')
        {
            return Err(invalid("external CSS resource"));
        }
    }
    Ok(())
}
fn validate_svg(bytes: &[u8]) -> Result<[f64; 2], PreparationError> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(quota("SVG bytes"));
    }
    let mut reader = NsReader::from_reader(bytes);
    let mut depth = 0usize;
    let mut dimensions = None;
    let mut styles = String::new();
    let mut in_style = false;
    let mut roots = 0usize;
    loop {
        let event = reader.read_event().map_err(invalid)?;
        let empty = matches!(&event, XmlEvent::Empty(_));
        match event {
            XmlEvent::Start(element) | XmlEvent::Empty(element) => {
                let (namespace, local) = reader.resolver().resolve_element(element.name());
                if namespace != ResolveResult::Bound(Namespace(b"http://www.w3.org/2000/svg")) {
                    return Err(invalid("SVG namespace"));
                }
                let name = String::from_utf8(local.as_ref().to_vec()).map_err(invalid)?;
                if ![
                    "svg",
                    "g",
                    "defs",
                    "symbol",
                    "use",
                    "path",
                    "rect",
                    "line",
                    "circle",
                    "ellipse",
                    "polyline",
                    "polygon",
                    "title",
                    "desc",
                    "style",
                    "clipPath",
                    "mask",
                    "linearGradient",
                    "radialGradient",
                    "stop",
                ]
                .contains(&name.as_str())
                {
                    return Err(invalid(format!("unsupported SVG element {name}")));
                }
                let mut attrs = BTreeMap::new();
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(invalid)?;
                    let key =
                        String::from_utf8(attribute.key.as_ref().to_vec()).map_err(invalid)?;
                    if !key.starts_with("xmlns")
                        && matches!(
                            reader.resolver().resolve_attribute(attribute.key).0,
                            ResolveResult::Unknown(_)
                        )
                    {
                        return Err(invalid("unbound SVG attribute prefix"));
                    }
                    let value = attribute
                        .decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )
                        .map_err(invalid)?
                        .into_owned();
                    if key == "xml:base"
                        || key
                            .rsplit(':')
                            .next()
                            .is_some_and(|name| name.to_ascii_lowercase().starts_with("on"))
                    {
                        return Err(invalid("SVG base or event handler"));
                    }
                    if key.rsplit(':').next() == Some("href") && !value.trim().starts_with('#') {
                        return Err(invalid("external href resource"));
                    }
                    if key == "xmlns" && value != "http://www.w3.org/2000/svg" {
                        return Err(invalid("SVG namespace"));
                    }
                    css_resources(&value)?;
                    attrs.insert(key, value);
                }
                if depth == 0 {
                    roots += 1;
                    if roots != 1 || name != "svg" {
                        return Err(invalid("SVG root and namespace required"));
                    }
                    let viewport: Vec<_> = attrs
                        .get("viewBox")
                        .map(String::as_str)
                        .unwrap_or_default()
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .filter(|s| !s.is_empty())
                        .map(str::parse::<f64>)
                        .collect::<Result<_, _>>()
                        .map_err(invalid)?;
                    if viewport.len() != 4
                        || viewport.iter().any(|n| !n.is_finite())
                        || viewport[2] <= 0.0
                        || viewport[3] <= 0.0
                    {
                        return Err(invalid("positive finite SVG viewBox required"));
                    }
                    dimensions = Some([viewport[2], viewport[3]]);
                }
                if name == "style" && !empty {
                    in_style = true;
                }
                if !empty {
                    depth += 1;
                    if depth > 256 {
                        return Err(quota("SVG nesting"));
                    }
                }
            }
            XmlEvent::End(element) => {
                if depth == 0 {
                    return Err(invalid("SVG nesting"));
                }
                depth -= 1;
                if element.name().local_name().as_ref() == b"style" {
                    in_style = false;
                }
            }
            XmlEvent::Text(text) => {
                let text = text
                    .xml_content(quick_xml::XmlVersion::Implicit1_0)
                    .map_err(invalid)?;
                if in_style {
                    styles.push_str(&text);
                } else if depth == 0 && !text.trim().is_empty() {
                    return Err(invalid("text outside SVG root"));
                }
            }
            XmlEvent::CData(text) => {
                if in_style {
                    styles.push_str(&text.decode().map_err(invalid)?);
                }
            }
            XmlEvent::DocType(_) | XmlEvent::PI(_) | XmlEvent::GeneralRef(_) => {
                return Err(invalid("SVG declaration, processing instruction or entity"));
            }
            XmlEvent::Eof => break,
            _ => {}
        }
    }
    css_resources(&styles)?;
    if depth != 0 {
        return Err(invalid("unclosed SVG"));
    }
    dimensions.ok_or_else(|| invalid("SVG root required"))
}
fn validate_png(bytes: &[u8]) -> Result<[u32; 2], PreparationError> {
    if bytes.len() > FILE_LIMIT || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(invalid("PNG signature or bytes"));
    }
    let mut offset = 8usize;
    let mut header = None;
    let mut compressed = Vec::new();
    let mut ended_data = false;
    let mut saw_data = false;
    let mut ended = false;
    while offset < bytes.len() {
        let chunk = bytes
            .get(offset..offset + 8)
            .ok_or_else(|| invalid("truncated PNG chunk"))?;
        let length = u32::from_be_bytes(chunk[..4].try_into().map_err(invalid)?) as usize;
        let kind = &chunk[4..8];
        let end = offset
            .checked_add(12)
            .and_then(|n| n.checked_add(length))
            .ok_or_else(|| invalid("PNG length overflow"))?;
        let entire = bytes
            .get(offset + 4..end)
            .ok_or_else(|| invalid("truncated PNG payload"))?;
        let payload = &entire[4..entire.len() - 4];
        let expected = u32::from_be_bytes(entire[entire.len() - 4..].try_into().map_err(invalid)?);
        if crc32fast::hash(&entire[..entire.len() - 4]) != expected {
            return Err(invalid("PNG chunk checksum"));
        }
        if header.is_none() && kind != b"IHDR" {
            return Err(invalid("PNG IHDR must be first"));
        }
        match kind {
            b"IHDR" => {
                if header.is_some() || payload.len() != 13 {
                    return Err(invalid("PNG duplicate or invalid IHDR"));
                }
                let width = u32::from_be_bytes(payload[..4].try_into().map_err(invalid)?);
                let height = u32::from_be_bytes(payload[4..8].try_into().map_err(invalid)?);
                if width == 0
                    || height == 0
                    || u64::from(width) * u64::from(height) > 32_000_000
                    || payload[8] != 8
                    || ![2, 6].contains(&payload[9])
                    || payload[10..] != [0, 0, 0]
                {
                    return Err(invalid(
                        "requires static noninterlaced 8-bit RGB/RGBA PNG <=32M pixels",
                    ));
                }
                header = Some((width, height, if payload[9] == 2 { 3usize } else { 4usize }));
            }
            b"IDAT" => {
                if ended_data {
                    return Err(invalid("noncontiguous PNG IDAT"));
                }
                saw_data = true;
                compressed.extend_from_slice(payload);
            }
            b"IEND" => {
                if !payload.is_empty() || !saw_data || end != bytes.len() {
                    return Err(invalid("PNG IEND or trailing bytes"));
                }
                ended = true;
                break;
            }
            _ => {
                if saw_data {
                    ended_data = true;
                }
                if [b"acTL".as_slice(), b"fcTL", b"fdAT"].contains(&kind) || kind[0] & 32 == 0 {
                    return Err(invalid("PNG animation or unsupported critical chunk"));
                }
            }
        }
        offset = end;
    }
    if !ended {
        return Err(invalid("missing PNG IEND"));
    }
    let (width, height, channels) = header.ok_or_else(|| invalid("PNG header"))?;
    let stride = 1 + width as usize * channels;
    let expected = stride * height as usize;
    let mut decoder = flate2::Decompress::new(true);
    let mut raw = Vec::with_capacity(expected + 1);
    let status = decoder
        .decompress_vec(&compressed, &mut raw, flate2::FlushDecompress::Finish)
        .map_err(invalid)?;
    if status != flate2::Status::StreamEnd
        || raw.len() != expected
        || decoder.total_in() != compressed.len() as u64
        || raw.chunks_exact(stride).any(|row| row[0] > 4)
    {
        return Err(invalid("invalid PNG compressed scanlines"));
    }
    Ok([width, height])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PreparationInput {
        PreparationInput {
            source: include_str!("../docs/fixtures/mvp-2/source.md").into(),
            logical_path: "source.md".into(),
            assets: vec![
                AssetFile {
                    relative_path: "assets/fig-add-inputs/illustration.json".into(),
                    bytes: include_bytes!(
                        "../docs/fixtures/mvp-2/assets/fig-add-inputs/illustration.json"
                    )
                    .to_vec(),
                },
                AssetFile {
                    relative_path: "assets/fig-add-inputs/figure.svg".into(),
                    bytes: include_bytes!(
                        "../docs/fixtures/mvp-2/assets/fig-add-inputs/figure.svg"
                    )
                    .to_vec(),
                },
                AssetFile {
                    relative_path: "assets/fig-choosing-a-result/comic.json".into(),
                    bytes: include_bytes!(
                        "../docs/fixtures/mvp-2/assets/fig-choosing-a-result/comic.json"
                    )
                    .to_vec(),
                },
                AssetFile {
                    relative_path: "assets/fig-choosing-a-result/strip.png".into(),
                    bytes: include_bytes!(
                        "../docs/fixtures/mvp-2/assets/fig-choosing-a-result/strip.png"
                    )
                    .to_vec(),
                },
            ],
        }
    }
    #[test]
    fn explicit_metadata_bypasses_conventions_and_never_falls_back() {
        let input = fixture();
        let original = input.source.clone();
        let prepared = prepare(input.clone()).unwrap();
        let expected: IllustrationMetadata = serde_json::from_str(include_str!(
            "../docs/fixtures/mvp-2/expected-derived-metadata.json"
        ))
        .unwrap();
        assert_eq!(prepared.metadata, expected);
        let saved_source = include_str!("../docs/fixtures/mvp-2/with-image-references.md");
        let mut saved = input.clone();
        saved.source = saved_source.into();
        saved
            .assets
            .retain(|asset| !asset.relative_path.ends_with(".json"));
        let from_file = prepare(saved.clone()).unwrap();
        assert_eq!(from_file.metadata, expected);
        assert_eq!(saved.source, saved_source);
        assert!(from_file.figures.iter().all(|figure| {
            saved_source[figure.source_bytes.clone()].contains(&format!("#{}", figure.id))
        }));
        // Even with conventional sidecars present, a bad saved reference is
        // authoritative and must not silently fall back.
        for bad_source in [
            saved_source.replace(
                "image: assets/fig-add-inputs/figure.svg",
                "image: missing.svg",
            ),
            saved_source.replace(
                "image: assets/fig-add-inputs/figure.svg",
                "image: ../escape.svg",
            ),
            saved_source.replace("    media-type: image/svg+xml\n", ""),
            saved_source.replace("media-type: image/svg+xml", "media-type: image/png"),
        ] {
            let mut bad = input.clone();
            bad.source = bad_source;
            assert!(prepare(bad).is_err());
        }

        let mut tree =
            serde_json::json!({"meta": {"title":{"t":"MetaString","c":"Keep title"}}, "blocks":[]});
        expected.write_pandoc(&mut tree);
        tree["meta"]["illustrations"]["c"]["fig-add-inputs"]["c"]["custom"] =
            serde_json::json!({"t":"MetaString","c":"keep extra"});
        let roundtrip = IllustrationMetadata::from_pandoc(&tree).unwrap().unwrap();
        roundtrip.write_pandoc(&mut tree);
        assert_eq!(roundtrip, expected);
        assert_eq!(tree["meta"]["title"]["c"], "Keep title");
        assert_eq!(
            tree["meta"]["illustrations"]["c"]["fig-add-inputs"]["c"]["custom"]["c"],
            "keep extra"
        );
        let mut relocated = input.clone();
        relocated.logical_path = "chapter/source.md".into();
        // No sidecars and no assets/<block-id>/ directory layout.
        relocated
            .assets
            .retain(|asset| !asset.relative_path.ends_with(".json"));
        let mut references = roundtrip;
        for (index, figure) in prepared.figures.iter().enumerate() {
            let image = references.illustrations.get_mut(&figure.id).unwrap();
            let file = relocated
                .assets
                .iter_mut()
                .find(|asset| asset.relative_path == figure.asset_path)
                .unwrap();
            image.image = format!("rendered/chosen-{index}.{}", figure.kind.extension());
            file.relative_path = format!("chapter/{}", image.image);
        }
        let result = prepare_with_metadata(relocated.clone(), Some(&references)).unwrap();
        assert_eq!(result.metadata, references);
        assert_eq!(relocated.source, original);
        for (figure, before) in result.figures.iter().zip(&prepared.figures) {
            assert_eq!(figure.source_bytes, before.source_bytes);
            assert_eq!(figure.asset_sha256, before.asset_sha256);
            assert_eq!(
                figure.asset_path,
                references.illustrations[&figure.id].image
            );
        }
        // Valid conventional files/sidecars must not mask an explicit failure.
        for bad_path in [
            "missing.svg",
            "../escape.svg",
            "/absolute.svg",
            "https://host/image.svg",
            "assets/%2e%2e/image.svg",
        ] {
            let mut bad = expected.clone();
            bad.illustrations.get_mut("fig-add-inputs").unwrap().image = bad_path.into();
            assert!(
                prepare_with_metadata(input.clone(), Some(&bad)).is_err(),
                "{bad_path}"
            );
        }
        let mut bad = expected.clone();
        bad.illustrations
            .get_mut("fig-add-inputs")
            .unwrap()
            .media_type = "image/png".into();
        assert!(prepare_with_metadata(input.clone(), Some(&bad)).is_err());
        assert!(prepare_with_metadata(input, Some(&IllustrationMetadata::default())).is_err());
        tree["meta"]["illustrations"]["c"]["fig-add-inputs"]["c"]["image"]["c"] =
            serde_json::json!("../escape.svg");
        assert!(IllustrationMetadata::from_pandoc(&tree).is_err());
    }

    #[test]
    fn pinned_upstream_semantics_assets_and_missing_artwork_rejection() {
        let input = fixture();
        let original = input.source.clone();
        let result = prepare(input.clone()).unwrap();
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../docs/fixtures/mvp-2/expected-semantic.json"
        ))
        .unwrap();
        for (figure, expected) in result.figures.iter().zip(expected.as_array().unwrap()) {
            assert_eq!(figure.id, expected["id"]);
            assert_eq!(figure.caption, expected["caption"]);
            assert_eq!(figure.description, expected["image"][1][0]["c"]);
            assert_eq!(figure.asset_path, expected["image"][2][0]);
            assert_eq!(
                figure.width_mm.as_deref(),
                expected["image"][0][2][1][1].as_str()
            );
            assert!(original[figure.source_bytes.clone()].contains(&format!("#{}", figure.id)));
        }
        assert_eq!(result.figures.len(), 2);
        assert_eq!(result.figures[1].dimensions, [1536.0, 1024.0]);
        let mut relocated = input.clone();
        relocated.logical_path = "chapter/source.md".into();
        for asset in &mut relocated.assets {
            asset.relative_path = format!("chapter/{}", asset.relative_path);
        }
        assert_eq!(prepare(relocated).unwrap(), result);
        let mut missing = input;
        missing
            .assets
            .retain(|asset| !asset.relative_path.ends_with("figure.svg"));
        assert!(matches!(
            prepare(missing),
            Err(PreparationError::Missing(_))
        ));
        // The all-offset scan finds fences nested in a quote; no render wrapper
        // may silently hide a metadata-backed figure from preparation.
        let mut nested = fixture();
        let marker = "```{#fig-add-inputs .illustration}";
        let start = nested.source.find(marker).unwrap();
        let end = start + nested.source[start..].find("\n```\n").unwrap() + 5;
        let quoted = nested.source[start..end]
            .lines()
            .map(|line| format!("> {line}\n"))
            .collect::<String>();
        nested.source.replace_range(start..end, &quoted);
        assert_eq!(prepare(nested).unwrap().figures.len(), 2);
    }
}
