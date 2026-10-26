//! Explicit derived-view transforms; no access to ECS, dispatch, or persistence.
use crate::markdown::{self, Attributes, BlockKind, Document, Fence, RenderBlock, SourceOrigin};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reader {
    Native,
    /// Reserved adapter; unavailable until a real Pandoc process runner exists.
    PandocMarkdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterContext {
    /// Actual writer target: `json` for native preview, `html` for HTML export.
    pub target: String,
    pub configuration: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentFilter {
    /// Native example with the same fence recognition as docs/plan/examples/callout.lua.
    Callout,
}

/// Runs filters in the supplied order. Whole-document results deliberately lose origins.
pub fn filter_document(
    mut document: Document,
    filters: &[DocumentFilter],
    context: &FilterContext,
) -> Document {
    for filter in filters {
        match filter {
            DocumentFilter::Callout => {
                document.blocks = document
                    .blocks
                    .into_iter()
                    .map(|block| callout(block, document.note, document.revision, context))
                    .collect();
            }
        }
        document.clear_origins();
    }
    document
}

fn callout(
    mut block: RenderBlock,
    note: crate::notes::NoteId,
    revision: crate::notes::Revision,
    context: &FilterContext,
) -> RenderBlock {
    if let BlockKind::Fence(fence) = &block.kind {
        if fence
            .attributes
            .classes
            .iter()
            .any(|class| class == "callout")
        {
            let children = markdown::read(note, revision, &fence.body).blocks;
            // A normal blockquote is supported by native preview and export consumers.
            // Target/configuration belong to the request even when this filter is target-neutral.
            let _ = context;
            return RenderBlock {
                kind: BlockKind::Div,
                inlines: Vec::new(),
                children: vec![derived_block(
                    BlockKind::Quote,
                    children,
                    Attributes::default(),
                )],
                attributes: fence.attributes.clone(),
                origin: None,
            };
        }
    }
    block.children = block
        .children
        .into_iter()
        .map(|child| callout(child, note, revision, context))
        .collect();
    block
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FenceRequest {
    pub fence: Fence,
    pub origin: SourceOrigin,
    pub context: FilterContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FenceOutput {
    Blocks {
        blocks: Vec<RenderBlock>,
        source_fallback: String,
    },
    /// Assets must be retained by the eventual job owner for this result's lifetime.
    Image {
        path: std::path::PathBuf,
        alt: String,
        source_fallback: String,
    },
    SourceFallback {
        source: String,
        diagnostic: String,
    },
}

/// Unavailable plugins preserve the exact fence, and never fabricate rendered success.
pub fn unavailable_fence(request: &FenceRequest, diagnostic: impl Into<String>) -> FenceOutput {
    FenceOutput::SourceFallback {
        source: request.fence.source.clone(),
        diagnostic: diagnostic.into(),
    }
}

/// Construct a standard read-only render block for an adapter's supported output.
pub fn derived_block(
    kind: BlockKind,
    children: Vec<RenderBlock>,
    attributes: Attributes,
) -> RenderBlock {
    RenderBlock {
        kind,
        inlines: Vec::new(),
        children,
        attributes,
        origin: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{Inline, read};
    use crate::notes::{NoteId, Revision};
    use uuid::Uuid;

    #[test]
    fn reader_callout_and_unavailable_renderer_preserve_source() {
        let note = NoteId(Uuid::new_v4());
        let revision = Revision(Uuid::new_v4());
        let source = "[[Target|Label]] [web](https://example.org) `[[no]]`\n\n```{#tip .callout title=\"A title\"}\nHello **world**.\n```\n\n```text\n[[also-no]]\n```\n";
        let original = source.to_owned();
        let document = read(note, revision, source);
        assert_eq!(document.links, ["Target", "https://example.org"]);
        assert!(
            document.blocks[0]
                .inlines
                .iter()
                .any(|inline| matches!(inline,
            Inline::Link { target, label, wiki: true } if target == "Target" && label == "Label"))
        );
        let BlockKind::Fence(fence) = &document.blocks[1].kind else {
            panic!("fence missing")
        };
        assert_eq!(fence.attributes.id.as_deref(), Some("tip"));
        assert_eq!(
            fence.attributes.values,
            [("title".to_owned(), "A title".to_owned())]
        );
        let origin = document.blocks[1].origin.clone().unwrap();
        assert_eq!(&source[origin.bytes.clone()], fence.source);
        let context = FilterContext {
            target: "json".into(),
            configuration: Vec::new(),
        };
        assert_eq!(
            unavailable_fence(
                &FenceRequest {
                    fence: fence.clone(),
                    origin,
                    context: context.clone()
                },
                "disabled"
            ),
            FenceOutput::SourceFallback {
                source: fence.source.clone(),
                diagnostic: "disabled".into()
            }
        );
        let filtered = filter_document(document, &[DocumentFilter::Callout], &context);
        assert!(filtered.read_only);
        assert!(matches!(filtered.blocks[1].kind, BlockKind::Div));
        assert_eq!(filtered.blocks[1].attributes.id.as_deref(), Some("tip"));
        assert!(filtered.blocks.iter().all(|block| block.origin.is_none()));
        assert!(
            filtered.blocks[1]
                .children
                .iter()
                .all(|block| block.origin.is_none())
        );
        assert_eq!(source, original);
        assert!(markdown::fence_attributes("{.callout bad=\"unfinished}").is_none());
    }
}
