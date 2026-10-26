# MVP-2: minimal source-backed editing

This increment keeps egui and the existing Rust application/command/persistence architecture. It adds one active document, a resizable notes sidebar, source-backed block editing, explicit Ready/Cancel, cached Markdown highlighting and local processed artwork preview. The controlling scope is the reduced slice at the top of [the proposal](plans/2026-10-26-mvp-2-proposal.md).

## Try Linux

```sh
cargo run --locked --features gui -- --vault /path/to/vault
```

If this environment's sccache fails with `Operation not permitted`, prefix Cargo with `RUSTC_WRAPPER=`. The `gui` feature includes document processing; headless core builds remain independent of graphics.

Read displays the accepted document. Choose an original source block below the preview, or Whole source. Typing changes a local draft. Ready sends one revision-checked source command, then processing refreshes the preview. Cancel discards that unsubmitted draft. A changed remote/imported revision retains the conflicting draft; copy it before explicitly discarding/reloading. Ready is unavailable during composition or conflict. Canonical saving and processing are separate: a processing failure never rolls source back.

Drafts are retained in memory when switching notes. This increment does not add crash-durable unsubmitted drafts. Closing with an unsubmitted draft asks you to preserve or discard it. Undo belongs to the current source buffer; adopting a new base resets its widget identity.

## Local artwork and filters

Expand Document processing. Set Artwork directory to an explicitly selected local directory containing the note-relative `assets/` tree, sidecars and saved SVG/PNG. For a root note such as `source.md`, choose `docs/fixtures/mvp-2` to try the supplied example. For a note named `chapter/source.md`, the selected directory must contain `chapter/assets/...`. Use Process / refresh artwork after changing external resources. Missing artwork reports an error; it never triggers generation.

Resources are copied into an owned, bounded processing snapshot. Files must be regular local files; symlinks and oversized trees are rejected. Keep the selected directory stable while capture runs: capture does not implement an atomic live-directory transaction. Saved artwork remains local; existing note sync does **not** transfer these files.

The default backend is the small Rust illustration/comic preparation subset on Linux and Android. It validates pinned illustration-md metadata/sidecars and renders saved artwork, without bundling Python or claiming arbitrary Pandoc support. Source outside the supported rendering vocabulary stays available in whole-source mode. Artwork with unsupported SVG text/external resources or excessive preview dimensions gives a diagnostic rather than silent omission.

On Linux, enable Use installed Pandoc to use `/usr/bin/pandoc`. Configure ordered filters one per line, then press Process to apply the directory, backend and filters together. Pending control edits do not change the active pipeline:

```text
lua:/absolute/path/filter.lua
json:/absolute/path/executable-json-filter
prepare:["/absolute/path/python3","-m","illustration_md.prepare"]
```

Preparation is deliberately distinct: Tanglewood appends `--source` with the private snapshot path and passes Pandoc JSON on stdin/stdout. The configured Python environment must already contain illustration-md. Filter programs are explicitly selected trusted local code, inherit the application environment, and run without a shell. General transformed output has no inferred source ranges: editing still uses the original source outline.

The ordered Lua/Python example and pinned saved artwork are in [the fixture](fixtures/mvp-2/README.md). [Processing details](plans/mvp-2-processing.md) document cancellation, quotas and tested process cleanup. [Platform feasibility](plans/mvp-2-platform-feasibility.md) distinguishes the Android named subset from Linux external filters.

## Deliberate limits

One active document; no new tabs/splits, general docking or editor engine. One Notes flow style; Academic/Tufte profiles and publication export are deferred. Native Pandoc presentation currently covers paragraphs/headings, code, lists, quotes/divs and the verified illustration figures. Other nodes remain visibly available as unsupported read-only output with source mode available. No reverse conversion of processed output, plugin marketplace, attachment store/transfer protocol, or browser target is introduced.

Integrated validation and remaining work are tracked in [MVP-2 status](mvp-2-status.md). The completed original MVP record is unchanged.
