# Explicit illustration metadata

**Goal:** Processors emit explicit, block-ID-keyed image references in derived metadata. Renderers resolve only those references; authored Markdown/prompt bytes remain unchanged.

**Architecture:** Add typed `IllustrationMetadata` containing `illustrations: {id: {image, media-type}}` to native preparation and `ProcessedDocument`. Pandoc uses the same fields in its standard `meta` representation. Explicit metadata is authoritative: validation failure must not trigger a conventional-sidecar fallback. Native saved-artwork preparation retains its existing sidecar input for compatibility, but emits the same validated metadata for rendering. Preserve other Pandoc metadata and original source bindings.

**Work:**
1. `src/illustration.rs`: metadata types/Pandoc conversion; explicit-reference preparation path with safe relative paths, supported media types and actual image validation. Output includes resolved metadata.
2. `src/document_jobs.rs`: carry derived metadata, consume Pandoc metadata when supplied, emit validated metadata back into the derived AST, and resolve renderer resources from metadata.
3. `src/ui/processing.rs`: decode using explicit media type; update affected fixtures without weakening last-good-result regression.
4. `docs/fixtures/mvp-2`: add expected derived metadata; adapt the example preparation shim to emit it; explain the distinction from authored front matter and retain historical platform evidence unchanged.
5. Focused real-asset regression: explicit nonconventional path and no sidecars succeeds; missing/unsafe/mistyped explicit reference fails without fallback; source unchanged; metadata/native/Pandoc representations agree. Reuse existing source/revision and native image staging tests. Run formatting, affected tests and relevant GUI build once, independent changed-risk review. No new generation, source serialization, asset sync or plugin runtime.

## Validation

- `RUSTC_WRAPPER= cargo test --locked --lib --features gui --offline`: PASS, 20 tests and 1 explicitly ignored real-Pandoc scenario. Covers explicit relocated paths without sidecars; unsafe/missing references and MIME mismatch without fallback; original source ranges/hashes; metadata preservation; native processor output to renderer resolution; existing UI staging/source guards.
- The earlier all-feature library run had 26 passing tests, 1 ignored, and 3 failures caused by sandbox-denied sockets in existing LAN/MCP scenarios. No networking implementation changed. The focused GUI suite above avoids those unrelated restricted adapters.
- `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,lan,codex --offline`: PASS with existing nonblocking warnings.
- `python3 docs/fixtures/mvp-2/prove-linux.py --upstream /home/dikini/Projects/illustration-md`: PASS with real Pandoc 3.7.0.2 and pinned upstream `9f38098d2355d4d77e41a538b48354fa8145d92a`; both JSON and HTML output maps match `expected-derived-metadata.json`, embedded images remain valid, missing SVG is rejected, authored source SHA-256 remains `3920d7763c21cc5bb0959f0ab90b85bfa59846e7cd4912186c97935b17f70efd`. Runtime evidence `/tmp/tanglewood-p0-filter-7g2qhss9`.
- No Android APK/emulator rerun for this backend metadata contract. The updated Android probe is for future runs; existing platform evidence remains historical.

- Linux demo binary build (`RUSTC_WRAPPER= cargo build --locked --bin tanglewood --features gui --offline`): PASS (9.15s). Formatting and diff checks passed.
- Independent changed-risk review: READY, no blocking findings; explicit references control lookup/decoding, invalid maps do not fall back, unrelated metadata and authored source remain preserved.

## Clarified persisted-file contract

The initial derived-only implementation did not meet the user's expectation that references appear in the Markdown file. Tanglewood now also reads `image` and `media-type` from YAML `diagrams` entries. `with-image-references.md` is the concrete saved-file example; `source.md` stays unchanged for legacy/upstream proof. Explicit processor metadata takes precedence for its derived view; otherwise saved references take precedence over legacy sidecars. Incomplete/invalid saved references fail explicitly. Preview processing does not automatically rewrite authored Markdown. Updating external `illustration-md` to write references is separate work and is not performed here.

Saved-reference correction validation: focused GUI-feature library suite PASS (20 tests, 1 ignored); covers saved YAML with no sidecars, incomplete/unsafe/mismatched saved fields, and source-range preservation. Linux binary rebuilt. Formatting and diff checks passed. Changed-risk review READY. The demo's `Writing with blocks.md` was updated via an exclusive App runtime and revision-checked source command; its canonical text differs from its backup only by the four new image/media-type lines. Export to `target/quiet-demo-with-image-references.md` matches the accepted source byte-for-byte. No external `illustration-md` files changed.
