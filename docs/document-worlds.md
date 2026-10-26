# Parsed document sub-worlds

Markdown remains authoritative. The application ECS note entity owns a lazy `ParsedDocument` component whose value is an immutable shared `DocumentWorld`. It is derived, is never saved as ECS data, and is rebuilt when the accepted source revision changes.

```text
Application World
└── Note entity
    ├── NoteRecord / SaveState
    └── ParsedDocument → Arc<DocumentWorld>
        ├── root: DocumentInfo + ordered Children
        └── block entities
            ├── NodeId
            ├── Block / InlineRuns / NodeAttributes
            ├── optional SourceBinding
            └── Parent / ordered Children
```

`App::parsed_document(note_id)` returns the shared generation. Renderer, original-block outline and draft context use handles to that same world. A dirty draft or last-good preview may retain an older immutable generation while the application publishes a complete replacement. That preserves its original revision and source ranges instead of modifying a tree under an active editor.

`NodeId` is an opaque, document-local semantic ID. Uniquely identified blocks and unique unchanged source blocks retain their IDs across reparsing, even when byte offsets move. Ambiguous duplicate blocks receive fresh IDs. ECS `Entity` handles are local to the specific world and must not cross persistence, commands, networking or world boundaries; use the owning note and `NodeId` for semantic references.

Blocks are ECS entities; inline runs stay together in an `InlineRuns` component. Fence IDs/classes/key-value attributes are also exposed in the common `NodeAttributes` component. The source parser still produces an ordinary document DTO, which is consumed into ECS storage. The current parser's unsupported Markdown structures remain source fallbacks; this change does not introduce a new Markdown dialect. Derived/filter trees can be built through `DocumentWorld::from_document`; read-only or invalid origins do not become editable bindings. The optional Pandoc renderer still consumes its existing JSON projection separately.

The document world contains no egui layout, focus, selection, scrolling, IME or draft buffers. These belong to each view. Styling remains presentation; formatting remains revision-checked changes to Markdown markup. This pass supplies the semantic ECS structure, not a CSS engine, resolved-style system or new keyboard-navigation behavior.

Native rendering traverses ordered child entities and borrows their semantic components. It never serializes the graph back into Markdown. Source changes continue through existing `EditNote` commands using the original note ID, expected revision and validated UTF-8 source range.
