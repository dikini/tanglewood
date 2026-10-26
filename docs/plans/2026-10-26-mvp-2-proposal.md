# MVP-2: egui UI and source-backed document editing proposal plan

**Status:** Reviewed and revised plan, implementation authorized by the user goal on 2026-10-26. Execution is tracked in `../mvp-2-status.md`; individual capabilities remain unimplemented/unverified until their gates pass. The completed original MVP record remains unchanged.

**Goal:** Turn Tanglewood into a comfortable Linux/Android writing application with a minimal pane-based egui interface, source-backed block editing, required Pandoc/custom filter support, syntax highlighting and richer themed document rendering.

**Architecture:** Preserve the existing single Rust package, application-owned ECS World, revision-checked shared commands, SQLite/Automerge persistence and adapter boundaries. Edit original Markdown only; processing produces disposable derived views. GUI interaction state and source-block bindings remain outside persistent note components.

**Tech stack:** Retain Rust, egui/eframe and the existing core. Research and select compatible Rust widget/icon/layout/highlighting libraries before adding dependencies. Add actual Pandoc JSON/Lua execution and concrete asset/rendering adapters where the selected workflows require them.

**Execution:** The active goal authorizes implementation of this updated plan. Follow [AGENTS.md](../../AGENTS.md), the [team workflow](../../.agents/workflow.md) and the [original architectural plan](../plan/tanglewood-mvp.md). Generic planning or testing templates do not override this repository's ownership, incremental delivery or lean testing policy. No new framework migration or browser assembly is part of MVP-2.

## Controlling MVP scope — reduced 2026-10-26

The user requested ruthless scope control after the 35–69 person-day estimate. The following small delivery slice supersedes broader release requirements and phase gates below. Retain the detailed sections as design/reference backlog; they are not all obligations for this increment. Preserve the existing architecture and completed MVP capabilities.

**Ship one complete writing loop:** open a note, view rendered blocks, edit the exact source of a block (or the whole document), choose Ready/Cancel, and see the refreshed result. Keep source safe on errors and revision conflicts.

Required for this slice:

- A quiet egui shell using existing controls, the selected icons/fonts, one resizable navigation sidebar and a compact source/preview action row. One active document; tabs, split views and an additional inspector can follow after the loop is usable.
- One active source transaction per note using existing TextEdit/IME support, explicit Ready/Cancel, cached syntax highlighting, retained draft on processing failure and revision-checked submission. Avoid a new general editor/session framework.
- One bounded background processing path: real ordered Pandoc/filter execution on Linux; reuse the demonstrated small Rust illustration preparation subset on Android. No plugin discovery/marketplace or universal runtime abstraction.
- The real illustration/comic fixture, saved SVG/PNG and explicit missing-artwork errors. Start with explicitly selected local document directories/resources. Snapshot processing inputs; do not process a changing live document. No new attachment sync protocol or atomic compound source/asset publication in this slice. Existing text sync remains; clearly identify that artwork is local and not transferred.
- Extend the existing native flow renderer only enough for the fixture and ordinary notes: text, lists, code, figures/captions and basic tables where needed. One polished Notes style first. Academic/Tufte profiles and themed HTML export are follow-up work.
- One integrated Linux workflow and one Android smoke of the changed editor/required illustration path, plus focused failure/stale-result checks and independent review. Reuse existing persistence, LAN and IME evidence where behavior is unchanged.

Deferred: tabs/splits, multi-view focus/session coordination, new durable draft-checkpoint subsystem, content-addressed bundle store, manifest CRDT/conflict model, asset transfer/resumption, portable bundle import/export, extra publication styles/export and broad semantic-renderer coverage. Preserve extension boundaries without implementing these systems speculatively. Ordinary source durability remains essential; do not claim crash recovery for an unsubmitted draft that is only in memory.

**Stopping rule:** deliver and try this slice before adding another subsystem. A reusable small helper is acceptable; a framework or new protocol needs a demonstrated blocker in this writing loop. The earlier A1/A2 and full I1/D1 gates apply only if explicitly scheduled later.

## 1. Agreed direction and current baseline

The framework decision is **keep egui**. The visual references are Obsidian's minimalism, the Codex application's quiet surfaces and contextual panels, and VS Code's activity rail, sidebars, tabs, split panes and compact toolbars. Reuse those interaction patterns without reproducing an entire IDE.

The completed MVP checkpoint is `8aaa704`. It provides durable notes, the shared command API, Linux and Android source editing, directory mirroring, CRDT/LAN synchronization, MCP/Codex integration and reader/filter/render interfaces. See [capabilities](../capabilities.md) and [MVP evidence](../integration-evidence.md).

Current limitations relevant to this proposal:

- `src/ui.rs` combines the application shell, source editor, simple preview and integration panels.
- The current whole-source editor automatically dispatches accepted non-composing changes. Ready/Cancel is a deliberate behavior change, not a button added to the existing autosubmit path.
- `src/markdown.rs` preserves source origins but has a deliberately small render vocabulary; native frontmatter interpretation and rich document layout are not implemented.
- `src/extensions.rs` contains contracts and a native callout example. The reserved Pandoc reader is not a real process runner; arbitrary JSON/Lua execution is not available.
- The Android input bridge has actual representative IME/clipboard evidence and must be preserved during UI changes.
- Existing LAN synchronization carries note histories, not illustration asset/sidecar bundles.

These are implementation starting points, not reasons to replace the main architecture.

Review basis: local source at the completed checkpoint, the pinned `eframe = 0.33.3` dependency, and `illustration-md` source at commit `9f38098d2355d4d77e41a538b48354fa8145d92a`. This review inspected contracts; it did not run new GUI, Android or filter acceptance checks. The highest-risk additions are multi-view draft ownership, Android processing and atomic source/asset publication, not pane styling. Address those before widening the renderer.

## 2. Required outcome and boundaries

### Required

1. Minimal, consistent Linux/Android UI with resizable desktop panes, document tabs, contextual action rows/columns and switching between sidebar/panel views.
2. A source-backed block editor plus whole-document source mode. Users edit original text, then signal Ready; they never edit processed filter output.
3. Syntax-highlighted Markdown/code editing with existing revision, draft, undo and IME protections.
4. Actual ordered Pandoc JSON/Lua execution on Linux and a concrete custom-plugin workflow on both supported platforms, not only documented extension interfaces. Android's required subset and execution backend must pass the P0 gate below; arbitrary external-filter parity is not assumed.
5. A real `illustration-md` integration scenario using its source, metadata, sidecars and saved artwork.
6. A native document renderer with initial Notes, Academic and Tufte-inspired style profiles and a themed HTML export path.
7. Explicit processing/progress/error/retry behavior, stale-result rejection and bounded background work.
8. Asset ownership, consistent processing snapshots and bounded authenticated Linux/Android asset delivery for the supported illustration workflow. Local-only assets do not close this gate.
9. Actual Linux and representative Android acceptance for the agreed editor/plugin capabilities, plus accurate build/configuration/capability documentation.

### Outside this increment

- Flutter, Electron, Tauri or another application-shell migration.
- Browser/WASM implementation, browser storage and a browser sync connector. A later target may use a different assembly behind the same architectural boundaries.
- Reverse conversion of arbitrary processed output into source, a second persistent rich-document model, or a Word-like page editor.
- Compatibility with arbitrary Obsidian JavaScript plugins, a plugin marketplace, a stable dynamic Rust ABI, or unrestricted third-party widget code.
- A new general-purpose typesetting engine, unrestricted CSS/HTML execution inside egui, or exact screen/print equivalence.
- Selective collaborative undo, background Android sync, unrestricted attachment/media synchronization and broad platform/feature matrices.
- Free-form docking, simultaneous independent edit buffers for the same note, and a custom rich-text selection/IME engine. Existing MCP/Codex behavior stays supported; D-Bus remains an optional original-plan track, not a new MVP-2 gate.

Optional paginated PDF/Typst/LaTeX output must be selected explicitly before becoming a delivery gate. Its absence does not remove the required native styles or themed HTML output.

## 3. Mandatory research before widget implementation

**R0 is a required predecessor to production shell, editor-component and renderer work.** Run it alongside P0's headless asset/filter/platform investigation, not as a prerequisite for all subsystems. Do not assume that egui defaults are the only available appearance or start hand-building controls without evaluating reusable Rust components.

Create `docs/plans/mvp-2-widget-research.md` during R0. Record the exact release/commit inspected, upstream links, compatibility with the currently pinned egui/eframe/toolchain, supported platforms and known gaps. A package being written in Rust does not make its widgets compatible with egui.

| Research area | Required investigation | Candidate starting points, not selections |
| --- | --- | --- |
| Widget/design systems | Themes, typography, menus, dialogs, popovers, tooltips, command/search pickers, trees, tabs, panels and controls; default and custom styling | egui itself, `egui_extras`, `egui-shadcn`/shadcn-style egui kits |
| Layout and navigation | Resizable/collapsible panes, tab groups, keyboard focus, reorder/split behavior, layout persistence and mobile alternatives | `egui_tiles`, `egui_dock`, ordinary egui panels; flex/layout helpers only if needed |
| Icons | SVG and/or bundled icon fonts, consistent stroke/size/alignment, scaling, state colors, tooltips, accessible labels, license and subset/bundle cost | Lucide, Phosphor or comparable egui-compatible integrations |
| Fonts | Prose/code families, Cyrillic/Greek and required scientific glyph coverage, fallback, scaling, bundling/license and font-loading cost | Existing egui font hooks plus deliberately selected font assets |
| Editing and highlighting | Source editing, syntax spans, selection/undo/IME hooks, large active buffers, cached highlighting and custom layouts | Existing `TextEdit` and Android bridge; `egui_extras`/Syntect and source-oriented editor crates |
| Document/media rendering | Rich text, lists, tables, footnotes, figures/captions, SVG/PNG, image sizing, links, source bindings and layout limits | Existing render model, `egui_commonmark`, egui image loaders and compatible SVG renderers |
| Interaction/accessibility | Focus indication/order, shortcuts, discoverability, touch targets, selection, tooltips and semantics | Existing framework capabilities plus actual Linux/Android behavior |

For each shortlisted candidate, distinguish **advertised**, **source-inspected**, **compiled** and **observed** support. Check maintenance, API stability, transitive dependencies and licenses. Inspect examples for the actual visual/interaction target; a screenshot is not evidence for focus, selection or Android input behavior. Start with the existing egui controls and at most two plausible alternatives per unresolved need. Stop when a small compatible choice satisfies the requirement; this is not an ecosystem survey or a prototype of every candidate. In particular, `shadcn-rs` advertises an unstable API, so it is a candidate, not an assumed dependency ([upstream notice](https://github.com/FerrisMind/shadcn-rs)).

The R0 decision matrix must end with **reuse / adapt / build** for each needed capability, including the reason and missing behavior. Prefer one coherent component/style system over mixing incompatible kits. Compare existing egui panels against docking libraries: ordinary resizing and tabs do not automatically justify arbitrary docking complexity.

Use one representative release-mode assembly when implementation is authorized to record startup, whole-process memory, idle CPU and observed resize/typing behavior. Identify editor/filter/asset costs only where there is a concrete problem; do not build a benchmark harness or infer footprint from binary size. Repeat measurements only after changes that invalidate them.

R0 exit: select a small compatible dependency set, a visual token/icon/font proposal and a minimal desktop/mobile layout. Record unresolved editor/platform limitations and choose focused feasibility checks. Current research links below are starting evidence, not completed R0 acceptance.

## 4. UI and interaction proposal

### Desktop shell

- Narrow activity rail switches Notes, Search, Links and AI views. Keep settings and secondary actions out of the writing surface.
- Resizable/collapsible primary sidebar contains the selected navigation view.
- Central document area has tabs, optional horizontal/vertical splits and an unambiguous active pane. Reordering and keyboard switching must retain draft/focus state.
- Optional secondary sidebar hosts Outline, Backlinks, document properties or AI. Closed by default when unnecessary.
- Compact contextual toolbars expose relevant actions; secondary operations use menus. Avoid moving every implemented command into the main toolbar.
- Quiet status area distinguishes Saved/Unsaved/Save failed, sync state and render processing/failure. Errors provide actionable detail.
- Store pane widths, open tabs, selected views and style preferences locally; they are not CRDT note content.

Ship the single-document shell first, then tabs and a simple two-pane split. Arbitrary nested docking is unnecessary. A tab or pane references a note's shared editor session; it does not own a second copy of that note's draft. Only the focused view owns text input. Other views of the same note display the same draft/read-only presentation until focus transfers.

### Android

Use one active document and drawers/sheets or a dedicated editing surface. Do not shrink the desktop pane arrangement onto a phone. Preserve the existing coherent insets, IME composition, system clipboard and lifecycle behavior. Android may use different components for the same block workflow.

### Source-backed block workflow

1. Display a rendered block with discoverable source-edit and relevant custom actions.
2. Open the exact original source in an appropriate inline, expanded, modal or panel editor.
3. Hold one active source transaction per note against a known revision and exact original ranges. Edits are not accepted merely because the draft changed. Both block and whole-source mode use this policy; remove the old autosubmit path when the controller takes ownership.
4. Ready validates and submits one shared revision-checked `EditNote`, with non-overlapping ranges into the same base source for compound fence/metadata edits. Cancel discards only this unsubmitted transaction; it does not undo an accepted or remote edit. Do not submit while IME composition is active.
5. On acceptance, schedule processing and show the previous good rendering with a processing indicator. SQLite durability and rendering completion have separate status.
6. Publish the matching new result when ready. On failure retain the accepted source and last good presentation, label the latter as stale, and expose diagnostics/Edit/Retry.

Changing notes or panes retains the draft; switching edit mode/block in the same note requires Ready or Cancel first. Closing a dirty tab offers Keep editing, Ready or explicit discard. Focus transfer invalidates the previous input session so a delayed Android callback cannot edit another block. Whole-source mode provides full-document selection/copy; block mode provides selection inside its active source editor and navigation to adjacent blocks. Cross-block rich-render drag selection is not a prerequisite for MVP-2.

A stale Ready retains the base and draft, offers copy/export of the draft and explicit reload/reapply against current source, and never retries with a refreshed revision automatically. Undo operates within the active draft; resetting to a new accepted/remote base retires old widget undo identity. Undo of previously accepted collaborative history remains outside scope. Opening an edit action on a stale rendering must validate the binding revision first; otherwise open current whole-source mode, never apply old byte ranges to new text.

Keep unsubmitted recovery drafts in local-only storage before acknowledged suspension/close where possible; recovery includes the base revision and opens as a draft, not an automatic command. This is not another canonical document model. Distinguish Draft, Accepted/Unsaved, Saved and Processing in the UI; do not claim abrupt-kill durability for text that has not reached either a recovery checkpoint or canonical storage.

Ready acceptance must not delete an acknowledged recovery checkpoint before the exact accepted revision or a confirmed covering canonical history generation is durable. Recovery retirement is correlated with its transaction/generation so a late save/cleanup cannot delete newer drafts. Cancel/discard retires queued and in-flight recovery in order, preventing an old checkpoint from reappearing after restart.

| Block family | Default editing workflow | Extra actions |
| --- | --- | --- |
| Paragraph/heading/list/quote | Lightweight inline or expanded original-source editor | Source formatting commands may be added without creating another document model |
| Code fence | Expanded source editor, language-aware highlighting | Copy, language/attribute editing |
| Table | Original Markdown editor initially | Specialized cell controls only after a concrete need |
| Illustration/custom fence | Original fence editor with rendered presentation | Plugin-specific properties or explicit authoring actions |
| Frontmatter/document metadata | Source/properties editor | Compound edits may update known metadata and a fence together |
| Unmapped generated output | Rendered result plus access to original document/source outline | No guessed source range or inverse filter |

Specific block workflows can be designed incrementally. The shared recognition/edit/Ready/Cancel/process/present contract must not require every Pandoc filter to implement a custom UI.

## 5. Editing, processing and renderer boundaries

### Source and editor model

Keep canonical Markdown and shared `EditNote` operations. A derived source index records note ID, revision, UTF-8 ranges and block kinds; any logical identity used for focus/reconciliation is UI state, not a persisted ECS block entity by default.

The rendered document is never serialized back into canonical Markdown. Source formatting actions insert or replace source text. Unsupported syntax and untouched bytes remain intact.

Use a small document-level controller for source selection, the active transaction, focus and undo transitions. Begin with one `TextEdit` for the chosen block or whole source, not a live editor for every paragraph. Retain stable widget/session IDs and the Android bridge. Cache highlighting by draft generation, language and theme; do not rehighlight the entire document every frame or force a framework upgrade to obtain an editor component.

The existing native reader and Pandoc Markdown do not have identical syntax or source-position guarantees. P0 selects reader/options explicitly, preserves existing wikilinks/backlinks and proves bindings for the supported edit subset. Extend offset-aware recognition only where needed; do not implement a second complete Pandoc parser or guess offsets by matching rendered text. Unknown/ambiguous structures remain accessible in whole-source mode. Initially edit frontmatter as exact source; a properties form is optional and must not round-trip/reformat unrelated YAML.

General filters may reorder, duplicate or replace nodes. Source bindings are sidecar data outside the standard Pandoc AST. Preserve existing conservative origin clearing for untrusted transformations; restore edit actions only through verified bindings/adapters. `illustration-md` IDs can bind a Figure to its original fence and metadata after duplicate/missing-ID validation. Do not infer mappings from equal text, output order or arbitrary filter-preserved attributes.

### Small Rust and command boundaries

Keep the current single package and one owner of `App`/ECS. Use concrete owned request/result types and bounded channels; a worker never receives `World`, a database connection, GUI state or an unrestricted dispatch handle. Keep persistence on its existing worker. Avoid an async rewrite of the application, a generic plugin framework, a second command bus or one entity per rendered block. UI caches/textures stay in the UI; source indices and processing data are renderer-independent.

Extend shared command descriptors, typed replies and caller policy only for actual new operations such as process/retry/status, bundle publication and themed export. They must work headlessly before every GUI action exists. Local widget focus, panel state and draft keystrokes are not domain commands; Ready is. MCP/AI and any later D-Bus/plugin action use the same dispatch and revision checks, but need not expose every new operation immediately. Executable configuration, filesystem access and asset publication retain explicit local/approval restrictions rather than inheriting generic note-write permission.

### Processing host

Run the selected Pandoc reader, ordered JSON/Lua filters and selected writer on a worker using a consistent document/metadata/assets snapshot. Prefer letting Pandoc execute its mixed filter list in declared order, with a thin wrapper for nonstandard stages. Use its Lua environment, not a generic Lua interpreter. Native preview uses actual writer target `json`; HTML export uses `html`. Run a target-specific pipeline for export rather than treating `json`-target filter output as interchangeable. These semantics follow the [Pandoc filter protocols](https://pandoc.org/filters.html) and [mixed JSON/Lua ordering](https://pandoc.org/lua-filters.html).

Preserve the full Pandoc JSON envelope, API version, metadata and unknown nodes at the process boundary, for example with `serde_json::Value` and a small checked projection into the native render model. Do not recreate the entire Pandoc type hierarchy in Rust or attach private source fields to its wire format. Detect unsupported API versions explicitly. Record exact executable/filter versions, reader extensions and configuration; require local installation/configuration on Linux rather than downloading executables from document content.

Whole-document processing after Ready is the default. A plugin may declare a genuinely block-local path later. Do not run Pandoc on every keypress or while drawing a frame. Syntax highlighting is a separately cached editor service.

Start with one processing worker and a bounded latest-request queue; coalesce superseded requests per document/target. Jobs have bounded input/output/diagnostic sizes, cancellation and deadlines. Drain stdout/stderr without pipe deadlock, terminate/reap the child process tree on timeout/cancellation, and clean up abandoned workspaces. Do not block an egui frame while waiting for a process or image decode.

Keys include note revision, logical source path, asset manifest/hash, reader/options, ordered filter versions/configuration and target. Include document style/template/font configuration when it affects processing; pure layout changes reuse semantic output. Publish only when the whole key and requesting session remain current, including after rename, asset-only changes and pane retirement. Bound parsed-document/image/texture caches and decoded image dimensions, and retain generated assets for the displayed result's lifetime.

Enable executable filters only through local configuration; note text or synchronization does not authorize execution. Launch explicit executable/argument arrays, not interpolated shell commands, with controlled working directory/environment. A worker process, temporary workspace or Pandoc flag is **not a security sandbox**: enabled external code may run with the user's OS privileges. State that trust requirement; stronger process isolation is separate work. Validate generated paths/assets before loading or exporting them, reject traversal/symlink escapes and unsolicited network resources, and bound image decoding. Custom authoring actions dispatch separately under existing validation/approval rules.

### Native rendering and document styles

Extend the current renderer-independent model incrementally: first text/inline styles, headings, lists/quotes and highlighted fences; then simple tables, figures/images/captions, internal references and footnotes. Complex tables, arbitrary HTML, math typesetting and bibliography automation need explicit source/diagnostic fallback unless separately selected. Preserve unknown output rather than silently dropping it. A compatible third-party viewer may supply components but must not reparse rendered Markdown in place of the authoritative reader or lose source/extension semantics.

App styling and document styling are independent. Provide Notes, Academic and Tufte-inspired profiles over the **same** semantic document/layout code: compact reading; wider scholarly hierarchy with numbered notes; and restrained wide-screen margin notes/figures with a bounded non-overlap policy and inline notes on phones. Start with vertical flow, not pagination. Three profiles do not mean three renderer implementations, arbitrary CSS compatibility or identical print layout.

Themed HTML export uses Pandoc templates/CSS and the same explicit document/filter configuration. Export accepted source and report its revision; never silently include or submit a draft. Embed allowed local resources in a standalone HTML file, or publish a complete HTML/resource directory with an explicit lifetime boundary; do not leave temporary workspace links. Export failure retains the old destination. A later publication backend does not replace the editable source-backed view. Switching styles never regenerates saved illustration artwork.

## 6. Concrete illustration and asset requirements

Use [illustration-md](https://github.com/dikini/illustration-md) as the real integration fixture, not a similar invented filter. Its preparation resolves stable fence IDs and metadata, validates saved sidecar/artwork, and produces Pandoc Figures. It does not interpret prose as a drawing language or regenerate artwork when the brief changes.

The inspected preparation entrypoint is a JSON stdin/stdout **stage** invoked as `python3 -m illustration_md.prepare --source <snapshot-source.md>`. It does not accept the positional writer argument of a normal Pandoc `--filter` executable. Use a thin wrapper or explicit stage adapter that supplies the immutable snapshot path, preserves ordering and handles the actual writer target. Keep preparation's Python/Pandoc dependencies separate from optional authoring's Node/browser/font toolchain; do not install authoring tools to display saved artwork.

The required small fixture contains one `.illustration` fence with `illustration.json` plus SVG and one `.comic` fence with `comic.json` plus PNG, both using matching `diagrams` metadata and unique stable `fig-` IDs. SVG and PNG are not interchangeable illustration inputs. Preserve the upstream validation boundaries and opaque construction data; source metadata supplies captions/alt text. Pin fixture provenance/license and upstream version; preserve the existing small Lua callout example as the inexpensive real-Lua smoke. This is preparation compatibility, not implementation of comic-series authoring.

- Retain source-relative metadata, sidecars and saved SVG/PNG assets in an app-owned document bundle. Prefer immutable content-addressed blobs and a small versioned manifest mapping safe relative paths to content hashes/lengths; keep large bytes out of Automerge. P0 selects the concrete format, size/count limits and version negotiation, not an attachment abstraction framework.
- Materialize a consistent job workspace with the logical Markdown path and matching relative assets. Do not point processing at a changing live directory or treat temporary paths as canonical references.
- Publish through owner-controlled operations with expected note revision and expected manifest identity. Stage and verify complete blobs first; then atomically persist the selected manifest and any compound source/history change at the existing SQLite durability boundary. Readers must see an old or new complete bundle, never half a sidecar/artwork pair. Retain old accepted versions on failure; avoid destructive garbage collection in this increment.
- Track asset dependencies so a changed SVG/sidecar invalidates rendering even when note text is unchanged.
- Synchronize manifest identity with note history, with explicit causal association for compound text/asset changes. Local opaque `Revision` tokens are not portable CRDT clocks. Concurrent asset publications retain both immutable versions and expose a recoverable manifest conflict; do not silently overwrite bytes or attempt to CRDT-merge images. An ordinary brief edit may intentionally retain the same artwork.
- Reuse the paired authenticated transport for bounded manifest/blob delivery with hashes, quotas, cancellation and a small protocol capability/version handshake. An older peer can still sync supported text without being told it received artwork. Keep incoming manifests pending until every referenced blob is validated and durable; acknowledge complete bundle availability only then. Source can remain editable while assets are pending/conflicted, but processing must show that state rather than combine it with arbitrary old artwork. Resume/retry uses durable missing-hash state, not a second general-purpose sync engine.
- Renames preserve note/bundle identity and invalidate logical-path-dependent processing. Begin with explicit bundle import/export; define a portable directory or bounded archive format and traversal/duplicate-entry limits in P0. A Markdown-only mirror/picker round trip does not transport its sidecars. Android uses an explicit selected bundle or granted tree; no implicit permission expansion or silent replacement of existing files. Updating live mirror semantics is not a prerequisite for explicit portable bundles.
- Missing/invalid assets give an explicit processing error with retained source; they are never an automatic generation trigger.
- Brief editing and artwork authoring are distinct workflows. Optional authoring integration must be explicit and keep its tools/dependencies out of ordinary preparation.

### Platform execution decision required in P0

P0 must **run a narrow feasibility spike**, not merely choose a policy. Linux demonstrates real Pandoc JSON/Lua execution and the Python preparation stage. On representative Android, prove the required fixture reaches a usable semantic result/assets through the proposed backend before committing to dependent renderer/plugin work. Decide native-source reader behavior for extended metadata/fences as part of that proof.

| Android option | Evidence and limits |
| --- | --- |
| Bundled Pandoc and required filter runtimes | Actual packaged execution, ABI/install-size/lifecycle constraints and licenses; do not assume a Linux binary works in an APK |
| Small Rust adapter for the required preparation subset | Recommended first investigation for local/offline use; compare the fixture's semantic output and representative rejection behavior to upstream. This is named-subset compatibility, **not** arbitrary Pandoc JSON/Lua execution |
| Host-assisted processing | Only with explicit user approval of connectivity/privacy requirements; authenticated peer, snapshot transfer, timeout/cancel and offline-unavailable states. This adds work and is not a silent fallback |

Keep source editing available without a backend, but source fallback alone does not close the required plugin gate. Publish a matrix separating Linux external JSON/Lua support, Android's selected required-plugin support and optional arbitrary Android filters. If no acceptable Android path is demonstrated within P0's agreed timebox, stop that dependent lane, report choices/costs and obtain a scope decision; continue independent shell/core work. Do not quietly defer the required workflow or spend the entire increment porting a runtime. Browser execution remains out of scope.

## 7. Delivery phases and file ownership

File splits below are proposed, not instructions to scaffold empty modules. Keep small concrete modules and the existing single package. The PM/integrator owns `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/app.rs`, `src/commands.rs`, persistence/schema changes and UI composition glue unless explicitly transferred. Use the existing PM/developer/reviewer roles, but track this work in `docs/mvp-2-status.md`; preserve the completed `docs/mvp-status.md` record. Assign one writer per file before concurrent work.

| ID / dependency | Deliverable and proposed files | Smallest meaningful acceptance |
| --- | --- | --- |
| R0 / first, alongside P0 | Bounded component/icon/font/layout/editor research; create `docs/plans/mvp-2-widget-research.md` | Short evidence matrix, reuse/adapt/build decisions, pinned compatibility and concrete unresolved risks |
| P0 / first, converges with R0 | Source-binding/draft design, real Linux/Android processing spike, asset causality/transfer contract and small fixture; record decisions here or in one focused design record | Observed required-platform path; explicit source dialect/selection/undo rules and snapshot/publication boundaries; no hidden runtime assumption |
| S0 / R0 + relevant P0 contracts | Short serial integration step: extract existing shell/editor/render responsibilities only as needed into `src/ui/shell.rs`, `src/ui/editor.rs`, `src/ui/document.rs`; establish concrete owned snapshot/result types | Existing behavior still usable; compileable handoff with disjoint ownership. No throwaway framework or empty subsystem scaffolding |
| U1 / S0 | Shell/theme lane: `src/ui/shell.rs`, `src/ui/theme.rs` and selected UI assets; PM wires `src/ui.rs` | Single-pane layout first, then tabs/two-pane split; real Linux resize/navigation and Android drawer/focus; no draft duplication |
| E1 / S0 + editor decisions | Editor lane: `src/ui/editor.rs` and source-index module (or exclusively assigned `src/markdown.rs`); Android bridge only by explicit ownership transfer | Source-backed Ready/Cancel and cached highlighting; adjacent bytes preserved; stale recovery, shared-note focus, local recovery checkpoint and representative IME behavior |
| A1 / P0 asset contract + S0 types | Local-bundle lane: `src/assets.rs`, immutable snapshots and explicit import/export; PM integrates `src/store.rs`, `src/files.rs`, commands and schema | Headless publication/restart with old-or-new complete bundles; matching source/history/manifest; safe missing-asset state and portable import |
| F1 / P0 runtime proof + S0 types; integrates A1 | Processing lane: `src/extensions.rs`, `src/document_jobs.rs`/`src/pandoc.rs`; use a real immutable fixture snapshot while A1 completes | Real ordered JSON/Lua and illustration stage; target correctness, bounded cancellation/stale results and selected Android backend; no GUI dependency |
| R1 / S0 render contract; integrates E1 + F1 | Renderer lane: `src/ui/document.rs`, exclusively assigned render-model projection and document style assets | Implement basic native document first; add figures/tables/notes and all three profiles against real F1 output, keeping source bindings conservative |
| A2 / A1 + P0 wire contract | Asset-delivery lane: exclusive `src/sync.rs`, `src/lan.rs`, `src/lan_owner.rs`; PM integrates durable receipts and command/status plumbing | Paired Linux/Android bundle transfer, interrupted receive/restart and recoverable concurrent manifest conflict; no partial-complete acknowledgement |
| I1 / U1 + E1 + A1 + F1 + R1; joins A2 | Integrated illustration/Ready workflow and portable themed HTML export; reuse `docs/fixtures/mvp-2/` | Fence/metadata edit, source-preserving processing, saved SVG/comic PNG, asset-only refresh and failure recovery; HTML works after workspace cleanup |
| D1 / integrated increments including A2 | Update `README.md`, extension/capability/Android/build/configuration docs and `docs/mvp-2-status.md` / `docs/mvp-2-evidence.md` | Required Linux/Android gates evidenced, blocking review findings resolved, reproducible configuration and honest per-platform support table |

### Parallel execution discipline

- Publish the small source/snapshot/job/result/manifest contracts before dispatching consumers. A pending field or signature is a PM decision, not a reason for two workers to invent competing APIs. Freeze each handed-off contract for the current increment; negotiate changes explicitly.
- After S0, U1 and E1 can run concurrently; A1 and F1 can run independently against the same concrete snapshot fixture. R1 can develop with representative native data while F1 completes. A2 need not wait for GUI exposure. With the default two developer slots, schedule ready lanes in bounded waves rather than spawning a worker for every table row.
- `src/ui.rs` and `src/markdown.rs` are **not** shared writable rendezvous points. S0 extracts real responsibilities or the PM assigns them serially. Shared command/store/schema/dependency edits go through the PM; workers submit their exact requested changes. Queue any worker needing the same file, even if its subsystem has a different name.
- Track backend implementation, GUI binding and runtime evidence separately. A headless command/runner with a real fixture can complete its own gate before its button exists; a mock preview does not complete the backend gate. Required end-to-end workflows still close at I1/D1.
- Preserve a runnable increment after each merge of work: shell with old editor, new editor with native preview, local bundles with headless processing, then richer renderer and device delivery. Do not wait for a single all-subsystem integration at the end.
- Serialize dependency/lockfile changes, broad formatting, Cargo jobs sharing a build directory and Git-index operations. Prefer shared-workspace file ownership; use PM-assigned worktrees only when needed for isolation.

At each coherent increment: handoff, freeze changed paths, one independent correctness/integration review, bounded corrections, relevant combined validation and an authorized unsigned checkpoint. Re-review only affected risks after fixes; style preferences and speculative tests are not blockers. Workers never stage/commit in the shared checkout. Every authorized commit uses `git -c commit.gpgsign=false commit --no-gpg-sign`; inspect explicit staged paths and verify `%G? = N`. No external publication is implied.

## 8. Validation and completion criteria

Keep the original plan's stopping rules: smallest meaningful scenario, regression only for uncovered behavior, one combined integration check when relevant, and no duplicate domain suites through every adapter. No Cargo runs for this proposal or other prose-only edits.

Default feedback loop: one focused scenario for the changed risk, or a compilation check if compilation is the only useful evidence. Extend an existing fixture instead of creating test-per-widget, test-per-command or test-per-Pandoc-node suites. Do not run check, build, test and Clippy consecutively on every patch.

At coherent integration checkpoints, the PM selects one relevant feature set and runs formatting, the affected library tests and Clippy once. For example, when only the baseline is affected:

```sh
cargo fmt --check
cargo test --locked --lib --no-default-features
cargo clippy --locked --all-targets --no-default-features
```

Add only affected features to that selection, such as `gui` for editor work or `lan` for A2; do not copy the old `gui,codex,lan` combination into every task. Include a headless feature-boundary check when new optional dependencies could leak into the core. New feature names are a dependency decision, not declared here. Clear `RUSTC_WRAPPER` only for a documented environment problem, not by default. Reuse workers' unchanged evidence; rerun only when integration or later changes invalidate it.

Run an Android target build and relevant device/emulator smoke after input, lifecycle, packaging, processing-backend or other changes that affect that platform. Reuse unchanged MVP domain/LAN/Codex evidence; a GUI change does not require another paid AI turn or full network matrix. Record exact commands/features, platform, fixture and result. Stop after meaningful checks pass; extra testing needs a named uncovered risk.

MVP-2 acceptance includes:

- One real Linux pane/tab/navigation/edit/save/restart workflow and one representative Android keyboard/selection/system clipboard/Ready/Cancel/lifecycle workflow.
- Source byte preservation around edited blocks, source-mode fallback, stale draft/binding recovery and no processed-output writeback. Cover duplicate-note panes and one IME handoff in the same editor scenario, not a UI permutation suite.
- A real ordered Pandoc JSON/Lua scenario using the existing callout example plus the actual illustration stage, a focused timeout/stale-completion check and evidence for the selected Android backend. Do not rerun the upstream plugin's full suite.
- One reusable illustration/comic fixture covering fence + metadata + correct sidecar + saved SVG/PNG, refresh after Ready/asset change, and missing-artwork failure without generation or source loss.
- One focused real-storage scenario at the new atomic publication boundary and one bounded Linux/Android document-assets exchange with interrupted receive/restart. Exercise a concurrent manifest conflict at the lowest effective boundary, reusing existing note merge tests rather than repeating the domain matrix.
- Native Notes/Academic/Tufte-inspired layouts at representative desktop and phone widths, plus themed HTML export. Inspect the actual output rather than relying on successful compilation.
- A supported/unsupported capability table, one representative release resource observation and resolved blocking correctness/data-preservation findings. Record nonblocking polish separately rather than making every review preference a release gate.

These are risk coverage obligations, not one automated suite per bullet. Reuse the same small fixture and combine manual desktop/phone acceptance with the relevant feature demonstration. No snapshot farm, synthetic performance project, broad fuzzing or multi-device/IME matrix without a demonstrated need.

MVP-2 is not complete if required plugin execution, source editing or asset handling is replaced by an interface-only stub. Optional publication backends and later browser assembly do not become gates without an explicit scope change.

## 9. Revised planning estimate

The earlier 35–69 person-day total described the expanded backlog, including a new asset storage/sync system, multi-view editor coordination and several publishing targets. It is withdrawn as the estimate for the reduced MVP above.

After the completed widget research and Linux/Android preparation proofs, use **5–10 additional focused engineering person-days** as a rough planning range for the reduced slice, with a **first usable Linux loop targeted within 2–3 person-days**. These are effort estimates, not elapsed agent-time or delivery-date promises. The main remaining uncertainty is integrating native artwork presentation and the existing Android input behavior with explicit Ready/Cancel. Re-estimate at the first usable loop, rather than expanding the scope to consume the range.

Reuse the selected widgets/assets, existing persistence/sync and demonstrated preparation adapter. Do not implement deferred systems to satisfy the old phase table. If local resource access or an input limitation blocks the slice, report the specific blocker and smallest remedy.

## 10. Research references

References support design investigation, not a claim that a dependency has been tested in this repository. Recheck the selected release at R0.

- [egui/eframe](https://github.com/emilk/egui), [egui-shadcn](https://github.com/FerrisMind/shadcn-rs), [egui_tiles](https://github.com/rerun-io/egui_tiles), [egui_dock](https://github.com/anhosh/egui_dock).
- [egui_commonmark viewer](https://github.com/lampsitter/egui_commonmark), [TextEdit layout hooks at the current pin](https://docs.rs/egui/0.33.3/egui/widgets/text_edit/struct.TextEdit.html), [egui_extras syntax highlighting](https://docs.rs/egui_extras/latest/egui_extras/syntax_highlighting/). Check the actual pinned crate source when documentation is unavailable; a `latest` example is not compatibility evidence for egui 0.33.3.
- [VS Code UI architecture](https://code.visualstudio.com/api/ux-guidelines/overview), [Tufte CSS concepts](https://edwardtufte.github.io/tufte-css/). The user's supplied Obsidian/Codex/VS Code references establish visual direction, not literal feature parity.
- [Pandoc filters](https://pandoc.org/filters.html), [Lua filter API](https://pandoc.org/lua-filters.html), [Pandoc manual](https://pandoc.org/MANUAL.html).
- [illustration-md inspected revision](https://github.com/dikini/illustration-md/tree/9f38098d2355d4d77e41a538b48354fa8145d92a), [source/asset contract](https://github.com/dikini/illustration-md/blob/9f38098d2355d4d77e41a538b48354fa8145d92a/docs/contract.md), [comic/PNG contract](https://github.com/dikini/illustration-md/blob/9f38098d2355d4d77e41a538b48354fa8145d92a/docs/comics.md), [preparation implementation](https://github.com/dikini/illustration-md/blob/9f38098d2355d4d77e41a538b48354fa8145d92a/illustration_md/prepare.py), [worked document](https://github.com/dikini/illustration-md/blob/9f38098d2355d4d77e41a538b48354fa8145d92a/examples/custom/README.md).
- Existing local boundaries: [document extensions](../document-extensions.md), [Android input adapter](../android-ime-bridge.md), [LAN protocol](../sync-protocol.md), [team workflow](../../.agents/workflow.md).
