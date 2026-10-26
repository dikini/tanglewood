# Contextual block editing first pass

**Goal:** Apply the user's requested in-place ordinary Markdown editing and bounded floating source editor for illustration/comic blocks, preserving surrounding document context.

**Architecture:** Reuse the existing per-note Draft, widget identity, IME guards and revision-checked Ready/Cancel path. Cache the original Markdown document in the draft. A verified ordinary top-level block replaces only that block in the original-source reading projection; custom fences and ranges without a suitable inline slot open a bounded floating window over the unchanged reader. Whole-document source mode stays explicit.

**Tech stack:** Existing Rust/egui, native TextEdit, no dependency or domain-command change.

1. `src/ui/editor.rs`: retain immutable original context; select inline versus floating from original block kind/range, not processed output. Keep draft source-byte semantics unchanged.
2. `src/ui/processing.rs`: allow one original native block to be replaced by an editor callback. Preserve other prose/artwork, disable competing source pencils during editing, do not infer filter origins.
3. `src/ui.rs`: extract shared source-field/actions rendering; ordinary text uses proportional type and grows with content, with a viewport bound. Render custom source in a bounded modal; closing explicitly cancels. Keep the same reading ScrollArea identity across transitions.
4. Extend one focused egui layout/source scenario for surrounding blocks plus existing Ready/Cancel/IME tests. Run formatting, focused UI tests, relevant Clippy, one real Linux visual smoke for paragraph and illustration. Shared Android layout warrants a relevant emulator smoke if available; report any unverified input/layout limit.

No new editor engine, formatting toolbar, docking, popup framework or Markdown dialect. No changes to canonical text until Ready. First-pass visual tuning may follow user feedback.

## Implemented / evidence

- One native plain-text field throughout; no block-widget registry added. Ordinary source replaces its original block in place; custom fences use a bounded egui modal. Ready/Cancel and canonical revision/IME checks are shared.
- Root cause of the reported clipping: ScrollArea inside a zero-height measurement child selected its default ~64-point minimum viewport. The viewport now uses measured text height for inline editing and an explicit larger height for source/modal editing. A five-line paragraph and zero-height parent regression exercise this boundary.
- `cargo fmt --check`: PASS. `RUSTC_WRAPPER= cargo test --locked --lib --features gui,lan,codex --offline ui::`: PASS, 6 tests. Relevant all-target Clippy: PASS with nonblocking warnings.
- Independent changed-risk review: READY after retaining selectable local drafts when a remote trash disables Ready.
- Actual Linux X11 window: full lesson paragraph stays visible, grows as text is inserted, retains before/after context. Temporary Cancel draft left canonical bytes unchanged; Ready inserted exactly `Test: ` and preserved every other byte in the isolated test copy. Comic modal displays the entire brief in a ~376-point field with actions visible and reader behind. Captures: `docs/design/contextual-inline-linux.png`, `docs/design/contextual-modal-linux.png`.
- Copied `learn/choosing-a-result/README.md` (6491 bytes) and `learn/working-with-text/README.md` (5900 bytes) from `/home/dikini/Projects/ash-doc/content` into the local demo as `Lessons/Choosing a result.md` and `Lessons/Working with text.md`. Original files and imported canonical bytes verified identical. The original four notes remain. Demo artifacts are under ignored `target/`; no lesson files were added to the repository.
- Initial affected Android build succeeded in 24s; installed without clearing data. Actual native inline paragraph, surrounding heading/list, and accept/cancel remained visible above the on-screen keyboard. Cancel returned to the reader. Final recovery-only field enablement does not change this layout; final APK rebuild recorded below.

Final reviewed Linux binary build: PASS (4.44s). Final Android APK rebuild: PASS (17s). Android custom-fence modal was not separately exercised: the retained artwork fixture has the previously documented malformed fence boundary; this pass did not rewrite it. Linux exercised that modal with valid source. No physical-device or new networking matrix claimed.
