# Document ECS sub-worlds

**Goal:** Implement the approved world-within-a-world model for parsed Markdown without changing source authority or visible editor behavior.

**Architecture:** A lazy `ParsedDocument` component on each application note entity owns an immutable `Arc<DocumentWorld>`. The sub-world contains a document root, ordered block entities, semantic block/inline/attribute components, parent/child relations, and revision-checked source bindings. Inline runs remain inside a block component. Rendering, outlines and draft context borrow the shared sub-world; old generations remain alive only while referenced by views/drafts/last-good previews. Layout, focus, scroll, IME and editing buffers remain in UI state.

**Identity:** Opaque node IDs are document-scoped and independent of ECS Entity and source byte offsets. Reconcile uniquely identified blocks and uniquely unchanged source blocks; ambiguous duplicates receive fresh IDs. Publish complete new worlds, never partially mutate a world being rendered. Entity handles never cross command/persistence/network boundaries.

**Scope:** Native Markdown-derived structure only. Preserve the existing parser dialect and optional Pandoc JSON renderer; do not add CSS, a widget registry, automatic formatting, keyboard navigation or new syntax in this pass. Source edits remain existing revision-checked commands.

1. `src/document_world.rs`, `src/lib.rs`: semantic components, read-only tree access, DTO-to-world builder, safe source bindings and node reconciliation. Focused tree/identity/revision/duplicate tests.
2. `src/app.rs`: lazy cached sub-world component and accessor; verify shared ownership, cache reuse, atomic revision replacement and old snapshot safety through real edit commands.
3. `src/ui.rs`, `src/ui/editor.rs`, `src/ui/processing.rs`, `src/ui/document.rs`: replace UI-owned native parsed trees with shared sub-world handles; walk entity order for rendering/outline; preserve frozen draft context and coherent artwork generations.
4. Update the main plan's original block-entity deferral with the user's approved exception. Run formatting, headless/core and relevant GUI tests, all-target Clippy, Linux build and one real existing demo source/preview smoke. Independent review of source safety and generations. No database migration, asset/protocol changes or new dependencies.

The brainstorming discussion and architecture were approved by the user before this implementation. Existing metadata/artwork work is uncommitted and must be preserved.

## Validation and result

- `RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --offline`: PASS, 10 tests. New scenarios exercise application-owned cache reuse/replacement, semantic components, parent/child order, stable IDs after offset changes, explicit fence identity after prompt edits, ambiguous duplicates, retained old revisions, stale command rejection, derived read-only trees and invalid UTF-8 origin removal.
- `RUSTC_WRAPPER= cargo test --locked --lib --features gui --offline`: PASS, 23 tests, 1 explicitly ignored external-Pandoc scenario. Existing source editing, layout context, automatic real-image loading and last-good preview scenarios now use the shared ECS worlds.
- `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,lan,codex --offline`: PASS with nonblocking warnings (including a redundant closure suggestion in the new node-key builder); no broad auto-fix sweep. Formatting and diff checks passed.
- Linux GUI binary rebuilt. Independent changed-risk review: READY, including fence-attribute normalization, source binding/generation safety and ordered traversal.
- Actual isolated Linux window on Xvfb `:94`, vault `/tmp/tanglewood-auto-artwork-smoke-6ga9d5di`: source prose and both SVG/comic display automatically. A paragraph draft containing `CANCEL ` was cancelled; read-only database comparison showed exact original source. A second draft accepted `ECS ` at the paragraph start; accepted source matched exactly that four-byte insertion, all other bytes unchanged. The reading preview and both images remained coherent with Saved status after the new generation. Captures `/tmp/tanglewood-subworld-preview.png`, `-draft.png`, `-ready.png`.
- No Android/device or new networking matrix was run for this semantic-storage refactor. Keyboard navigation, styling systems, expanded Markdown parsing and Pandoc-to-ECS projection remain separate follow-ups. No database migration or source serialization was introduced.

Usage and ownership are described in `docs/document-worlds.md`. Changes remain uncommitted alongside the earlier metadata/artwork work.
