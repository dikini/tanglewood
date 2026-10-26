//! Rebuildable semantic sub-world. Published generations are read-only; source is authoritative.
use crate::{
    markdown::{self, Attributes, BlockKind, Document, Inline, RenderBlock, SourceOrigin},
    notes::{NoteId, Revision},
};
use bevy_ecs::prelude::*;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Component, Clone)]
pub(crate) struct ParsedDocument(pub Arc<DocumentWorld>);

/// Local semantic identity, not a source offset or a persistable ECS Entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub uuid::Uuid);
#[derive(Component, Debug)]
pub struct Block(pub BlockKind);
#[derive(Component, Debug)]
pub struct InlineRuns(pub Vec<Inline>);
#[derive(Component, Debug)]
pub struct NodeAttributes(pub Attributes);
#[derive(Component, Debug)]
pub struct SourceBinding(pub SourceOrigin);
#[derive(Component, Debug, Default)]
pub struct Children(pub Vec<Entity>);
#[derive(Component, Clone, Copy, Debug)]
pub struct Parent(pub Entity);
#[derive(Component)]
struct ReconcileKey(String);
#[derive(Component)]
pub struct DocumentInfo {
    pub note: NoteId,
    pub revision: Revision,
    pub source: Arc<str>,
    pub metadata: Vec<(String, String)>,
    pub links: Vec<String>,
    pub read_only: bool,
}

pub struct DocumentWorld {
    world: World,
    root: Entity,
    index: BTreeMap<NodeId, Entity>,
}

pub struct Node<'a> {
    pub entity: Entity,
    pub id: NodeId,
    pub kind: &'a BlockKind,
    pub inlines: &'a [Inline],
    pub attributes: &'a Attributes,
    pub origin: Option<&'a SourceOrigin>,
    pub parent: Entity,
    pub children: &'a [Entity],
}

impl DocumentWorld {
    pub fn parse(note: NoteId, revision: Revision, source: &str, previous: Option<&Self>) -> Self {
        Self::from_document(
            markdown::read(note, revision, source),
            Arc::from(source),
            previous,
        )
    }

    pub fn from_document(document: Document, source: Arc<str>, previous: Option<&Self>) -> Self {
        let mut old: BTreeMap<String, Vec<NodeId>> = BTreeMap::new();
        if let Some(previous) = previous.filter(|previous| previous.note() == document.note) {
            for (id, entity) in &previous.index {
                if let Some(key) = previous.world.get::<ReconcileKey>(*entity) {
                    old.entry(key.0.clone()).or_default().push(*id);
                }
            }
        }
        struct Pending {
            data: RenderBlock,
            key: Option<String>,
            children: Vec<Pending>,
        }
        fn prepare(
            mut block: RenderBlock,
            info: &DocumentInfo,
            counts: &mut BTreeMap<String, usize>,
        ) -> Pending {
            if let BlockKind::Fence(fence) = &block.kind {
                let mut attributes = fence.attributes.clone();
                if block.attributes.id.is_some() {
                    attributes.id = block.attributes.id.take();
                }
                for class in std::mem::take(&mut block.attributes.classes) {
                    if !attributes.classes.contains(&class) {
                        attributes.classes.push(class);
                    }
                }
                for (key, value) in std::mem::take(&mut block.attributes.values) {
                    attributes.values.retain(|(existing, _)| existing != &key);
                    attributes.values.push((key, value));
                }
                block.attributes = attributes;
            }
            block.origin = block.origin.filter(|origin| {
                !info.read_only
                    && origin.note == info.note
                    && origin.revision == info.revision
                    && origin.bytes.start < origin.bytes.end
                    && info.source.get(origin.bytes.clone()).is_some()
            });
            let tag = match &block.kind {
                BlockKind::Paragraph => "paragraph",
                BlockKind::Heading(_) => "heading",
                BlockKind::Fence(_) => "fence",
                BlockKind::Quote => "quote",
                BlockKind::Div => "div",
                BlockKind::Source(_) => "source",
            };
            let explicit = block
                .attributes
                .id
                .as_deref()
                .or_else(|| match &block.kind {
                    BlockKind::Fence(fence) => fence.attributes.id.as_deref(),
                    _ => None,
                });
            let key = if let Some(id) = explicit {
                Some(format!("{tag}\0id:{id}"))
            } else {
                block
                    .origin
                    .as_ref()
                    .and_then(|origin| info.source.get(origin.bytes.clone()))
                    .map(|raw| format!("{tag}\0source:{raw}"))
            };
            if let Some(key) = &key {
                *counts.entry(key.clone()).or_default() += 1;
            }
            let children = std::mem::take(&mut block.children)
                .into_iter()
                .map(|child| prepare(child, info, counts))
                .collect();
            Pending {
                data: block,
                key,
                children,
            }
        }
        let info = DocumentInfo {
            note: document.note,
            revision: document.revision,
            source,
            metadata: document.metadata,
            links: document.links,
            read_only: document.read_only,
        };
        let mut counts = BTreeMap::new();
        let pending: Vec<_> = document
            .blocks
            .into_iter()
            .map(|block| prepare(block, &info, &mut counts))
            .collect();
        let mut world = World::new();
        let root = world.spawn((info, Children::default())).id();
        let mut index = BTreeMap::new();
        fn spawn(
            world: &mut World,
            parent: Entity,
            pending: Pending,
            counts: &BTreeMap<String, usize>,
            old: &BTreeMap<String, Vec<NodeId>>,
            index: &mut BTreeMap<NodeId, Entity>,
        ) -> Entity {
            let id = pending
                .key
                .as_ref()
                .and_then(|key| {
                    let candidates = old.get(key)?;
                    (counts.get(key) == Some(&1) && candidates.len() == 1).then_some(candidates[0])
                })
                .unwrap_or_else(|| NodeId(uuid::Uuid::new_v4()));
            let entity = world
                .spawn((
                    id,
                    Block(pending.data.kind),
                    InlineRuns(pending.data.inlines),
                    NodeAttributes(pending.data.attributes),
                    Parent(parent),
                    Children::default(),
                ))
                .id();
            if let Some(origin) = pending.data.origin {
                world.entity_mut(entity).insert(SourceBinding(origin));
            }
            if let Some(key) = pending.key {
                world.entity_mut(entity).insert(ReconcileKey(key));
            }
            index.insert(id, entity);
            let children = pending
                .children
                .into_iter()
                .map(|child| spawn(world, entity, child, counts, old, index))
                .collect();
            world.entity_mut(entity).insert(Children(children));
            entity
        }
        let children = pending
            .into_iter()
            .map(|block| spawn(&mut world, root, block, &counts, &old, &mut index))
            .collect();
        world.entity_mut(root).insert(Children(children));
        Self { world, root, index }
    }
    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn root(&self) -> Entity {
        self.root
    }
    pub fn info(&self) -> &DocumentInfo {
        self.world
            .get::<DocumentInfo>(self.root)
            .expect("document root")
    }
    pub fn note(&self) -> NoteId {
        self.info().note
    }
    pub fn revision(&self) -> Revision {
        self.info().revision
    }
    pub fn source(&self) -> &str {
        &self.info().source
    }
    pub fn links(&self) -> &[String] {
        &self.info().links
    }
    pub fn roots(&self) -> &[Entity] {
        &self
            .world
            .get::<Children>(self.root)
            .expect("document children")
            .0
    }
    pub fn node(&self, entity: Entity) -> Option<Node<'_>> {
        Some(Node {
            entity,
            id: *self.world.get::<NodeId>(entity)?,
            kind: &self.world.get::<Block>(entity)?.0,
            inlines: &self.world.get::<InlineRuns>(entity)?.0,
            attributes: &self.world.get::<NodeAttributes>(entity)?.0,
            origin: self
                .world
                .get::<SourceBinding>(entity)
                .map(|binding| &binding.0),
            parent: self.world.get::<Parent>(entity)?.0,
            children: &self.world.get::<Children>(entity)?.0,
        })
    }
    pub fn find(&self, id: NodeId) -> Option<Node<'_>> {
        self.node(*self.index.get(&id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        App,
        commands::{Caller, Command, CommandResult},
        notes::TextEdit,
    };

    #[test]
    fn application_owns_shared_generations_and_reconciles_source_nodes() {
        let mut app = App::new();
        let source = "# Heading\n\nα café `match` and [next](B.md).\n\n```{#fig-demo .illustration}\nold prompt\n```\n";
        let CommandResult::Note(view) = app
            .execute(
                Command::CreateNote {
                    path: "A.md".into(),
                    text: source.into(),
                },
                Caller::Local,
            )
            .unwrap()
        else {
            panic!("note")
        };
        let first = app.parsed_document(view.note.id).unwrap();
        assert!(Arc::ptr_eq(
            &first,
            &app.parsed_document(view.note.id).unwrap()
        ));
        assert_eq!(first.source(), source);
        assert_eq!(first.roots().len(), 3);
        let paragraph = first.node(first.roots()[1]).unwrap();
        let id = paragraph.id;
        let bytes = paragraph.origin.unwrap().bytes.clone();
        let figure_id = first.node(first.roots()[2]).unwrap().id;
        assert_eq!(
            first
                .node(first.roots()[2])
                .unwrap()
                .attributes
                .id
                .as_deref(),
            Some("fig-demo")
        );
        assert!(
            first
                .node(first.roots()[2])
                .unwrap()
                .attributes
                .classes
                .iter()
                .any(|class| class == "illustration")
        );
        assert_eq!(paragraph.parent, first.root());
        assert!(
            first
                .world()
                .get::<InlineRuns>(paragraph.entity)
                .unwrap()
                .0
                .iter()
                .any(|inline| matches!(inline,Inline::Code(text) if text=="match"))
        );
        assert_eq!(first.links(), ["B.md"]);
        let prefix = "# Preface\n\n";
        let CommandResult::Note(changed) = app
            .execute(
                Command::EditNote {
                    id: view.note.id,
                    expected: view.note.revision,
                    edits: vec![TextEdit {
                        start: 0,
                        end: 0,
                        replacement: prefix.into(),
                    }],
                },
                Caller::Local,
            )
            .unwrap()
        else {
            panic!("edit")
        };
        let second = app.parsed_document(view.note.id).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(
            &second,
            &app.parsed_document(view.note.id).unwrap()
        ));
        let shifted = second.find(id).unwrap();
        assert_eq!(
            shifted.origin.unwrap().bytes,
            bytes.start + prefix.len()..bytes.end + prefix.len()
        );
        assert_eq!(shifted.origin.unwrap().revision, changed.note.revision);
        assert_eq!(
            first.find(id).unwrap().origin.unwrap().revision,
            view.note.revision
        );
        assert_eq!(first.source(), source); // retained draft generation is untouched
        let start = changed.note.text.find("old prompt").unwrap();
        let CommandResult::Note(edited) = app
            .execute(
                Command::EditNote {
                    id: view.note.id,
                    expected: changed.note.revision,
                    edits: vec![TextEdit {
                        start,
                        end: start + "old prompt".len(),
                        replacement: "new prompt".into(),
                    }],
                },
                Caller::Local,
            )
            .unwrap()
        else {
            panic!("edit")
        };
        let third = app.parsed_document(view.note.id).unwrap();
        let figure = third.find(figure_id).unwrap();
        assert!(matches!(figure.kind,BlockKind::Fence(fence) if fence.body.contains("new prompt")));
        assert_eq!(figure.origin.unwrap().revision, edited.note.revision);
        assert_eq!(third.source(), edited.note.text);
        assert!(
            app.execute(
                Command::EditNote {
                    id: view.note.id,
                    expected: view.note.revision,
                    edits: vec![TextEdit {
                        start: 0,
                        end: 0,
                        replacement: "stale".into()
                    }]
                },
                Caller::Local
            )
            .is_err()
        );
        assert!(Arc::ptr_eq(
            &third,
            &app.parsed_document(view.note.id).unwrap()
        ));
        let other = DocumentWorld::parse(
            NoteId(uuid::Uuid::new_v4()),
            Revision::fresh(),
            source,
            Some(&third),
        );
        assert!(other.find(id).is_none()); // identities do not cross document worlds
    }

    #[test]
    fn hierarchy_duplicates_and_derived_source_bindings_stay_unambiguous() {
        let note = NoteId(uuid::Uuid::new_v4());
        let revision = Revision::fresh();
        let source = "# Header\n\nsame\n\nsame\n";
        let first = DocumentWorld::parse(note, revision, source, None);
        let second = DocumentWorld::parse(note, Revision::fresh(), source, Some(&first));
        assert_eq!(
            first.node(first.roots()[0]).unwrap().id,
            second.node(second.roots()[0]).unwrap().id
        );
        for entity in &first.roots()[1..] {
            assert!(second.find(first.node(*entity).unwrap().id).is_none());
        }
        let source = "# Header\n\n```{#tip .callout}\nHello *there*.\n```\n";
        let document = crate::extensions::filter_document(
            markdown::read(note, revision, source),
            &[crate::extensions::DocumentFilter::Callout],
            &crate::extensions::FilterContext {
                target: "json".into(),
                configuration: Vec::new(),
            },
        );
        let tree = DocumentWorld::from_document(document, Arc::from(source), None);
        assert!(tree.info().read_only);
        let div = tree.node(tree.roots()[1]).unwrap();
        assert!(matches!(div.kind, BlockKind::Div));
        assert_eq!(div.attributes.id.as_deref(), Some("tip"));
        let quote = tree.node(div.children[0]).unwrap();
        assert_eq!(quote.parent, div.entity);
        assert!(matches!(quote.kind, BlockKind::Quote));
        let paragraph = tree.node(quote.children[0]).unwrap();
        assert_eq!(paragraph.parent, quote.entity);
        assert!(
            paragraph
                .inlines
                .iter()
                .any(|inline| matches!(inline, Inline::EmphasisStart))
        );
        assert!(
            tree.index
                .values()
                .all(|entity| tree.node(*entity).unwrap().origin.is_none())
        );
        let mut invalid = markdown::read(note, revision, "α text\n");
        invalid.blocks[0].origin.as_mut().unwrap().bytes = 1..2; // inside UTF-8 character
        let invalid = DocumentWorld::from_document(invalid, Arc::from("α text\n"), None);
        assert!(invalid.node(invalid.roots()[0]).unwrap().origin.is_none());
    }
}
