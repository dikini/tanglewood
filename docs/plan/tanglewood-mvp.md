# Tanglewood: Incremental Rust Notes App Plan

## Goal and feasibility decisions

Build an Obsidian-inspired notes application for Linux and Android with shared Rust application logic, an immediate-mode egui UI, and an ECS data model. Deliver a useful local editor first. LAN synchronization, Linux automation, and document plugins are independently deliverable extensions. Reserve their interfaces now without requiring their runtimes for editing notes.

The original plan combines several substantial projects: a mobile text editor, lossless block editing, database/file reconciliation, collaborative editing, authenticated networking, and a Codex client. Keep the product direction, but reduce the initial implementation:

| Original choice | Revised decision and reason |
| --- | --- |
| Separate crates for every layer | One Cargo package with a library and thin platform entry points. Modules support concurrent development without workspace/API overhead. |
| Background runtime owns all data; UI consumes snapshots | One ECS owner on the application/UI thread. Borrow data for rendering; send only I/O jobs and results across threads. |
| Command registry with schemas upfront | Typed command enums and explicit dispatch. Add serialization and schema generation at the automation boundary. |
| Pandoc-style AST and block editing first | Markdown source editing and a derived preview first. Block editing is a later experiment behind the same edit commands. |
| Plugins deferred without an interface | Reserve document-filter and rendering boundaries now, targeting actual Pandoc protocols. Implement runners incrementally. |
| Turso plus Automerge immediately | Start with `rusqlite` and bundled SQLite; introduce Automerge when implementing sync. Database-level sync is unnecessary. |
| All features in the first milestone | A local notes release, followed by independent sync and AI increments. Android input feasibility is checked early. |

A plain struct model would be smaller for basic CRUD, but standalone `bevy_ecs` supports the intended composition of notes, derived document data, and background-job status. Use that small subset; a full Bevy application/game loop adds no value here. [Bevy ECS documentation](https://docs.rs/bevy_ecs/latest/bevy_ecs/)

Assume one active vault, one writable application process per vault, trusted personal devices, foreground-only Android sync, and Linux-only AI. Start with a working envelope of roughly 1,000 notes and 100 KiB per actively edited note. These are profiling targets, not reasons to reject or truncate imported content; larger notes can fall back to source-only display.

## Minimal Rust structure

Start with these files, creating each only when its slice needs it:

```text
Cargo.toml
src/lib.rs          shared application API and module declarations
src/main.rs         Linux startup
src/app.rs          World ownership, command processing, worker completions
src/commands.rs     shared request/result/error types and dispatch policy
src/notes.rs        note components, note commands, editing rules
src/store.rs        concrete SQLite connection, migrations, persistence worker
src/ui.rs           egui panels and transient interaction state
src/markdown.rs     parsing, preview, link extraction
src/android.rs     Android entry point and lifecycle/storage integration
```

Reserve a small `extensions.rs` boundary alongside `markdown.rs` when defining preview types. Add `files.rs`, `sync.rs`, `mcp.rs`, `codex.rs`, and Linux-only `dbus.rs` as their subsystems arrive. Split a large module by feature when needed; extract a crate only for a demonstrated packaging or dependency boundary. Keep internal APIs `pub(crate)` and use concrete structs, enums, and functions before introducing traits.

- Initial dependencies: `eframe`/`egui`, standalone `bevy_ecs`, `rusqlite` with `bundled`, `uuid`, and a small typed error layer such as `thiserror`. Add `pulldown-cmark` for preview/navigation.
- Use a `gui` feature to keep headless library development independent of graphics. Add optional `sync`, `mcp`, `codex`, `dbus`, and external-filter features when implemented; gate Linux-only adapters by target. Do not create a combinatorial feature matrix.
- Use one ordinary persistence thread initially. Introduce a Tokio runtime for networking/MCP/D-Bus/process I/O only when needed; it never becomes a second ECS owner.
- Pin a compatible toolchain and dependency set after the platform spike. Keep the lockfile. Avoid custom graphics backends, generic repositories, dynamic Rust library loading, reflection-based persistence, and generated application frameworks. A small explicit list of document extensions is sufficient before a plugin manager is warranted.

Choose rusqlite's synchronous API for the single storage worker so the first slice does not need an async runtime just for persistence. Its bundled build removes the requirement for a device-installed SQLite library; the Android toolchain still needs an actual build/run check. The database is local storage, not the synchronization mechanism. [rusqlite API](https://docs.rs/rusqlite/latest/rusqlite/), [build options](https://github.com/rusqlite/rusqlite#notes-on-building-rusqlite-and-libsqlite3-sys)

## ECS and immediate-mode UI

The shared application owns a `World`. In the desktop/mobile app it is driven by the UI event loop; a headless runner drives the same application without egui. There is no `Arc<Mutex<World>>` shared among subsystems.

Use one entity per note initially:

- `NoteId(Uuid)` is the persistent identity; ECS `Entity` handles never cross persistence, command, or network boundaries.
- `NoteMeta` holds the vault-relative path and trash state. `NoteText` holds source text and a revision token.
- `SaveState` tracks accepted and persisted revisions. `ParsedMarkdown` and extracted links are optional, rebuildable components.
- Resources hold the ID lookup, vault configuration, command queue, and worker state. Selection, tabs, drafts, cursor, IME state, and panel visibility belong in `UiState`, outside persistent note components.

Do not make every paragraph, inline token, widget, or task checkbox an entity. Add block entities only if a later editing feature demonstrably needs stable block identity.

The event-loop sequence is explicit:

1. Drain a bounded amount of worker completions and external command requests.
2. Apply commands in order, validate first, and update the affected ECS components.
3. Refresh dirty derived data needed by the visible UI. Parse changed notes, not the whole vault on every frame.
4. Render egui directly from borrowed read-only application data and mutable `UiState`; widgets enqueue commands.
5. After UI borrows end, dispatch its commands and submit pending I/O work without blocking.

This gives Rust short, clear borrow scopes and keeps UI and command handling in one process. Copy only small display rows or active editor buffers where borrowing is inconvenient. A vault-wide immutable snapshot/subscription layer is unnecessary. Start with ordinary functions or a small explicitly ordered ECS schedule; enable parallel systems only after a measured need.

Keep filesystem access, database writes, networking, and child processes off the UI thread. Jobs carry owned data and a revision/generation; completions are applied by the owner. Use bounded queues and nonblocking submission: retain dirty work when the queue is full, and return `Busy` for excess external requests. Reject stale derived results, request repaint on completion, and sleep when idle.

## One command contract across subsystems

GUI buttons, keyboard shortcuts, the palette, headless callers, MCP, Linux D-Bus, and AI adapters use the same typed domain commands. ECS handlers implement the behavior once; storage persists their resulting state. Any future interactive plugin actions use this boundary too. No adapter writes note rows or components through a separate business-logic path.

Start with a `Command` enum, a `CommandResult`, and `CommandError`, dispatched through a synchronous application method such as `execute(command, caller)`. Group variants into subsystem enums, for example `Command::Note(NoteCommand)` and later `Command::Sync(SyncCommand)`, so subsystem development does not require every edit to touch one giant enum. Keep requests independent of egui, SQL, Automerge, and transport types.

Initial note operations are create, read/list, edit, rename, trash, and restore. Add search, import/export, and sync requests with their implementations. Prefer explicit IDs and revision tokens over implicit GUI selection. The GUI supplies its selected note ID; an AI caller supplies an ID returned by read/search. Revision tokens are opaque, change with note mutations, and cannot be reused for different content after restart or sync.

For editing, use a request shaped like `EditNote { note_id, expected_revision, edits }`, where each edit replaces a source range. Define ranges as UTF-8 byte offsets on character boundaries, relative to the stated revision; validate that edits do not overlap and apply them from the end. Convert egui cursor positions and later CRDT indices at their boundaries. Reject stale revisions with a structured conflict response containing the current revision; never overwrite intervening edits silently.

Each command family provides a small static descriptor with a stable name, availability, and exposure policy per caller class. Initially these can be enum methods/static tables. Add a versioned Serde request/result envelope when the first external adapter lands; MCP and D-Bus reuse those types, validation, and errors. Generate MCP schemas from the same request types. Caller identity and permissions are supplied by the trusted adapter, not accepted from a serialized request. Check policy in dispatch, even when a command is hidden from discovery.

**Command implementation, automation exposure, and GUI exposure are separate decisions.** A subsystem is complete when its contract works through the application API; it does not need a button, menu, or palette item. For example:

| Operation | Application command | Initial GUI | MCP/AI exposure |
| --- | --- | --- | --- |
| Create/edit/read note | Implemented in local slice | Editor and note list | Allowed when adapter and policy are ready |
| Restore trashed note | Can ship with storage | May arrive later | May be exposed before a trash panel exists |
| Rebuild derived links | Can ship with indexing | No dedicated control required | Optional maintenance capability |
| Run document filter for preview/export | Ships with extension runner | May have no dedicated control | Available to permitted automation independently of GUI |
| Pair/unpair device | Ships with sync | Pairing flow when ready | Local-only by default |
| Toggle sidebar/focus editor | Local `UiAction` | UI only | Not a domain command |

Maintain a compact capability table as commands ship: implementation status, supported targets/features, automation policy, and optional GUI entry point. Discovery reports available authorized commands, not just GUI-visible ones. An unavailable subsystem returns a typed `Unavailable` result; avoid success-shaped stubs.

Distinguish an accepted in-memory mutation from a durably saved revision. Reads may return the current accepted state with its save status. GUI edits can return immediately; external mutations normally await the corresponding persistence acknowledgement outside the UI thread. Request IDs correlate replies but do not imply durable exactly-once execution. Do not blindly retry a mutation after a transport timeout.

Worker completions and authenticated remote CRDT messages are private typed inputs to the same application owner. They are not publicly callable commands that an AI can forge. CRDT synchronization exchanges changes, not a replay of GUI commands.

## Notes, persistence, and editing

### Local source editor first

Use `egui::TextEdit::multiline` for the active note and an optional preview. Keep a local draft based on a known revision, and translate committed changes to the shared edit request. A simple prefix/suffix diff is sufficient for contiguous local typing; do not build a rope or custom text widget initially. Preserve IME composition and retain the draft if a command is rejected. [egui TextEdit](https://docs.rs/egui/latest/egui/widgets/text_edit/struct.TextEdit.html)

Use the widget's local undo/redo initially; its changes still become edit commands. When another source changes the note, preserve any pending draft and reset incompatible widget undo history before adopting the new text. This prevents undo from restoring a stale whole-note buffer over AI/import/remote changes. Selective collaborative undo is a separate later feature, not an assumed Automerge capability.

Parse Markdown with `pulldown-cmark` events/source ranges. Store only what preview, outline, and backlinks need, retaining fence information and attributes for extension dispatch. The source text is authoritative; never regenerate an entire note from a custom AST. Preserve unsupported syntax and untouched bytes. Define wikilink behavior explicitly and exclude code spans/blocks from link extraction. Keep the parser behind a small reader boundary: an AST filter cannot recover custom syntax that the reader has already discarded. [pulldown-cmark documentation](https://docs.rs/pulldown-cmark/latest/pulldown_cmark/)

Start with a note list, editor, save/error status, and create/rename/trash actions. Then add quick switching, palette, links/backlinks, and content search. A linear title/body search is adequate for the initial working envelope; introduce SQLite FTS only after a measured need. On Linux, evolve toward the activity rail, explorer, tabs, and optional side panels; on Android use one editor and drawers. Presentation work must not block new domain commands.

Block editing follows as an isolated experiment: render supported blocks and edit one block's original source range using `EditNote`. Begin with paragraphs/headings; nested lists, cross-block selection, cursor remapping, and collaborative anchors need separate work. Keep whole-note source editing available. Do not make this experiment a release gate for sync or AI.

### Durable storage and Markdown portability

Use a versioned SQLite schema containing note IDs, paths, source text, trash state, and persistence bookkeeping. Plain text is canonical in the local-only stage. Serialize explicit records, not the ECS world. The persistence worker owns the connection; application startup loads records into ECS. Enforce a single writable runtime with a vault lock.

Autosave short batches of accepted revisions. The UI remains dirty until the worker acknowledges the matching commit; an older acknowledgement cannot mark a newer edit saved. Coalesce unsent full-state saves per note, but never discard queued operation deltas. Failures retain dirty data and surface a retryable error. Flush on explicit save and orderly shutdown; Android suspension is best effort, so correctness cannot depend on a shutdown callback. A process kill may lose changes still marked unsaved.

First deliver explicit Markdown import/export. Then add the Linux vault-directory mirror as its own slice; Android initially uses app-owned storage with explicit import/export through the platform picker. Keep layout/preferences device-local.

For directory mirroring:

- Commit note changes and a pending export record in the same database transaction. Export afterward using temporary files and atomic replacement where supported; retry pending exports after restart. Show database save status separately from a failed file export.
- Persist the last exported text/hash and path. Before replacing a file, compare it with that baseline. Import external-only edits; if both sides changed, retain both versions for explicit resolution. Do not silently overwrite external edits. A fully race-free protocol with arbitrary external editors is outside this milestone.
- Begin with an explicit rescan command. Add a filesystem watcher only if it improves actual use; route rescan/import changes through shared commands.
- Preserve UUIDs across app renames. External rename detection is deferred: treat it as a missing old path plus a new import, and retain the old note recoverably. Missing files do not silently hard-delete notes.
- Validate vault-relative paths, reject traversal/symlink escapes, and prevent collisions. Trash retains content; hard deletion/garbage collection is deferred.

The database is the app's recovery source; Markdown files are its portable representation. Database loss can recover exported text, but cannot reconstruct IDs, pending edits, or future CRDT history from plain Markdown alone.

## Document filters and rendering plugins

Support document transformation and specialized rendering as explicit extension points. **Interface accommodation is part of the initial design; a general plugin host is not part of the local-editor milestone.** Keep Markdown as the source of truth and the filtered result as a derived view.

```text
Markdown at revision -> selected reader -> document filters -> render model -> egui
                                             |
                                             +-> export writer
```

### Small Rust boundaries

Define owned request/result data, with direct functions initially, in `markdown.rs`/`extensions.rs`:

- **Reader:** returns a derived document with blocks/inlines, metadata, attributes, and optional source origins. The native reader uses `pulldown-cmark`. Reserve an alternate reader for Pandoc Markdown or another syntax extension; avoid pre-processing the original text in place.
- **Document filter:** transforms that document for preview or export. Its context includes the requested target format and plugin configuration. Filters run in an explicit user-defined order; they receive a whole document unless they explicitly declare block-local scope.
- **Fence renderer:** receives a code fence's body, raw info string, parsed attributes, note revision, and source span. It returns standard render blocks or a static image artifact with alternative text and a source fallback. This accommodates diagrams/math/specialized code fences without handing a plugin `egui::Ui` or ECS access.
- **Render model:** contains owned, renderer-independent data. egui draws it in direct style; it does not run external tools while rendering. Unsupported nodes retain a visible source/diagnostic fallback. Static image decoding can be added with the first image-producing plugin; HTML/JavaScript output is not an egui widget.

Preserve Pandoc-style attributes as identifier, classes, and key/value pairs. Normalize a plain fence language such as `mermaid` into a class while retaining its original info string; support an explicitly documented subset of `{#id .class key="value"}` when the native adapter arrives. Do not equate CommonMark parsing with full Pandoc Markdown compatibility. [Pandoc fenced-code attributes](https://pandoc.org/MANUAL.html#extension-fenced_code_attributes)

Keep source origins outside the Pandoc wire AST. They are revision-scoped references, not durable block IDs. A whole-document filter may reorder, replace, or create nodes, so its output has no reliable editable source mapping by default. Display it read-only unless an origin is explicitly known; source editing always remains available. Never write filtered output back automatically. An explicit apply-to-note action must produce reviewed, revision-checked `EditNote` requests through the shared dispatcher.

### Compatibility levels

Target compatibility with existing filters at the adapter boundary rather than inventing a similar-looking protocol:

| Extension | Planned interface | Compatibility limit |
| --- | --- | --- |
| Native Rust filter/fence renderer | Functions over owned document/render data, initially registered explicitly | Internal Rust API; no stable dynamic-library ABI or arbitrary widget plugins |
| Pandoc JSON filter | One standard Pandoc JSON document on stdin, one on stdout; real target format as the argument; diagnostics on stderr | Preserve `pandoc-api-version`, `meta`, `blocks`, and unhandled nodes; check supported schema versions. No Tanglewood envelope inside that JSON. |
| Pandoc Lua filter | Optional installed Pandoc process using `--lua-filter` | Use Pandoc's actual Lua runtime and APIs; embedding a generic Lua interpreter is not equivalent. |
| New Markdown syntax | Reader option/adapter, followed by the same filter pipeline | Document which syntax the native reader recognizes; optional Pandoc reader provides Pandoc's dialect. |

Pandoc defines both JSON-process filters and Lua filters. Existing dependencies, target-format assumptions, and generated media still matter: protocol compatibility does not guarantee that every filter's output can be displayed natively. [Pandoc filter protocol](https://pandoc.org/filters.html#technical-details-of-json-filters), [Pandoc Lua filters](https://pandoc.org/lua-filters.html)

For the first compatibility runner, prefer invoking installed Pandoc on Linux to read Markdown, apply existing JSON/Lua filters, and return JSON. Native preview uses the actual `json` target; export passes its real writer target. Convert only supported output nodes to the render model, preserving unknown JSON rather than silently dropping it. A direct native-reader-to-Pandoc-JSON bridge can follow for the supported subset; it is not necessary to reimplement the full Pandoc AST first. Raw HTML/LaTeX results remain export-only or get a clear unsupported preview. An external runner is optional on Android; unavailable plugins fall back to original source without preventing editing or sync.

Schedule filters on a worker when document/configuration changes, with debounce and cancellation. Key results by note revision, reader/options, ordered plugin versions/configuration, and target; declare asset dependencies or avoid caching their results. Whole-document filters invalidate with the whole document, while declared fence-local renderers can cache by fence content. Discard stale completions. Use limits on time/output and preserve source on errors. External processes run only when locally enabled and are not a sandbox; syncing a note or plugin reference does not authorize executable installation or execution. Resolve generated assets inside a per-job directory and retain them for the lifetime of the render result.

### Small compatible example

[examples/callout.lua](examples/callout.lua) is a real Pandoc Lua filter: it recognizes a `.callout` code fence, parses its body as Markdown, and returns a standard `Div` containing a `BlockQuote`, preserving the fence attributes. An egui renderer only needs normal blockquote/inline rendering; optional class styling can come later. Other code fences pass through unchanged. [Example input](examples/callout.md)

Run from the repository root with Pandoc installed:

```sh
pandoc docs/plan/examples/callout.md --from=markdown --to=json \
  --lua-filter=docs/plan/examples/callout.lua
```

The same filter works with `--to=html` for export. This is a compatibility example, not an implemented Tanglewood plugin loader. When the loader is built, use this one fixture to check the adapter/render boundary; do not create a separate plugin testing framework.

## Sync and Linux integration extensions

### CRDT and LAN synchronization

First prove two-replica merging in memory, independently of UI and transport. Introduce Automerge behind the existing note operations. Use a shared vault catalog for note membership and one document per note containing its text, path, and tombstone state. Persist device actor identity, vault identity, catalog, and document histories. Pairing joins an existing vault; do not independently bootstrap matching documents from Markdown on both peers.

Migrate existing local notes transactionally into CRDT documents with the same note IDs, retaining a recoverable pre-migration database. After migration, Automerge is authoritative and ECS strings/SQL search text are projections. Use explicit text splices for ordinary edits and a diff for imported text. Check the pinned Rust library's indexing rules rather than assuming its offsets match egui or UTF-8. Automerge provides text operations, history save/load, and sync messages; application conflict policy remains our responsibility. [Automerge text operations](https://docs.rs/automerge/latest/automerge/transaction/trait.Transactable.html), [Automerge save/load and sync](https://automerge.org/automerge/automerge/struct.AutoCommit.html)

Persist local changes before advertising them to a peer. Persist received changes before advancing the durable sync acknowledgement; after restart regenerate synchronization state from durable histories. An active editor must retain its draft or receive mapped changes safely; never apply a stale whole-note replacement. Begin with manual/foreground sync and the conservative undo reset described above. Concurrent edit/delete keeps the tombstone and recoverable edited content; conflicting paths resolve deterministically using note IDs. Convergence alone does not ensure valid Markdown.

Then connect the proven merge engine over direct QUIC using `quinn`/`rustls`. Keep one bounded, versioned message format carrying vault/document IDs and Automerge sync messages. Pair with an explicitly exchanged ticket containing an address and public-key fingerprint; confirm locally, persist/pin peer identities, authenticate both sides, and reject unpaired peers. Bound message sizes and work per tick. Reconnect and duplicate-message handling use the sync protocol, not a new generic replication framework. No public discovery, relay, accounts, attachment sync, or Android background service.

### Linux automation and Codex

MCP can ship as soon as shared note commands and persistence work. It does not depend on block editing, complete GUI exposure, or sync.

Add an `rmcp` stdio bridge that forwards authorized typed commands over a user-restricted Unix socket to the running application. It never opens a second writable vault runtime. Derive tool schemas from shared request types and translate results/errors at the edge. Start with read/search, then revision-checked edits. Fail clearly when the application is absent. [Rust MCP SDK](https://docs.rs/rmcp/latest/rmcp/)

Develop the Codex adapter separately against that boundary: launch `codex app-server` over stdio, initialize, start/resume a thread, stream turns, and support cancellation, approvals, and disconnect handling. Use existing Codex authentication. Register the MCP bridge for document access; add the conversation panel after the adapter works. Keep the Codex protocol model at this boundary and pin the supported server version. [Codex App Server](https://learn.chatgpt.com/docs/app-server)

Automation exposure is allowlisted per command; it is independent of GUI visibility. Codex process/filesystem permissions remain separate from MCP permissions. An adapter failure leaves editing and saving usable.

### Optional Linux D-Bus messaging

Add a Linux-only `dbus` feature and local enable setting using `zbus` on the **session bus**. It can ship independently of MCP and Codex. The existing application owns the service and forwards requests to the same bounded command queue; it never opens another vault writer. If async adapters share a Tokio runtime, use zbus's compatible runtime integration. [zbus documentation](https://docs.rs/zbus/latest/zbus/)

Reserve a versioned service interface, provisionally bus name `org.tanglewood.Tanglewood`, object path `/org/tanglewood/Tanglewood`, and interface `org.tanglewood.Tanglewood1`. Keep the initial surface small:

| D-Bus member | Contract |
| --- | --- |
| `GetCapabilities() -> s` | JSON containing protocol version and commands available to this caller, regardless of GUI coverage |
| `Execute(request: s) -> s` | Shared versioned JSON command/result envelope; same revision checks, errors, and durability semantics as MCP |
| `NoteChanged(note_id: s, revision: s)` signal | Optional invalidation hint after persistence; consumers re-read through commands. No note bodies broadcast. |

Using JSON inside standard D-Bus strings avoids maintaining a second typed representation for every command. D-Bus introspection describes the transport surface; shared command schemas describe payloads. If typed convenience methods become useful, they remain wrappers over dispatch.

Construct `Caller::Dbus` from the actual bus sender and checked session credentials, never payload fields. Apply a local D-Bus command allowlist independently of MCP exposure. Session access does not grant all commands. Handle limits, cancellation/timeouts, and service loss at the adapter boundary; a timed-out mutation may already have committed and must not be blindly retried. Signals are best-effort hints, so reconnecting clients re-read state. [D-Bus specification](https://dbus.freedesktop.org/doc/dbus-specification.html)

Defer system-bus service, automatic activation, and additional daemons. A missing session bus disables this adapter without affecting the editor or other integrations. Validate with one session-bus round-trip and signal smoke when implemented; reuse core command tests.

## Concurrent subsystem development

Concurrent implementation does not require concurrent mutation of application state. Agree on a small compilable contract first, then work in separate modules against it.

**Shared contract checkpoint:** define `NoteId`, revision/range semantics, command/result/error types, caller policy, note records, persistence job/acknowledgement types, and the application dispatch entry point. Reserve reader/filter/render result types and source-origin semantics with the first preview slice. Include one create/read/edit path runnable without a window. Assign one integrator for `commands.rs`, `app.rs`, shared schema changes, and `Cargo.toml`; subsystem owners propose small additive changes to that contract. Version the shared external envelope when MCP or D-Bus first needs it.

| Workstream | Owned area | Can develop against | Integration dependency |
| --- | --- | --- | --- |
| Core/ECS | `app.rs`, `commands.rs`, `notes.rs` | Real in-memory World and typed commands | Shared contract checkpoint |
| Persistence/files | `store.rs`, later `files.rs` | Owned note records and save jobs; temporary vault | Contract; no GUI dependency |
| UI/document views | `ui.rs`, `markdown.rs` | In-memory app and seeded notes | Contract; no storage/network dependency for layout work |
| Document extensions | `extensions.rs`; agreed reader/render types in `markdown.rs` | Callout fixture, revision-tagged filter jobs, owned output | Reader/render contract; can develop without GUI, sync, or AI |
| Android | `android.rs` and packaging | Small shared TextEdit app and storage spike | Platform gate, then local slice |
| Sync | `sync.rs` | Two in-memory replicas and defined note operations | Persistence migration before shipping; no AI dependency |
| Automation/AI | `mcp.rs`, `codex.rs` | Shared commands and a headless app runner | Persistence for durable mutations; no sync dependency |
| Linux messaging | `dbus.rs` | Shared external envelope and headless app runner | Command/persistence contracts; no MCP, Codex, or GUI dependency |

Use small concrete fixtures at these boundaries, not a mock framework or duplicate domain implementation. Integrate small usable changes regularly. Subsystems can land working commands before GUI affordances; track any absent GUI explicitly rather than marking the whole capability unfinished. Only advertise commands that are actually implemented and authorized.

## Delivery increments and gates

These are dependency gates, not a requirement to finish one workstream before starting all others.

| Increment | Deliverable | Exit evidence |
| --- | --- | --- |
| 0a. Contracts | Headless ECS app with create/read/edit and stable IDs/revisions | One focused command scenario |
| Preview contract, alongside 1/2 | Reader/filter/render boundaries, fence attributes, source-origin rules | Small compatible callout example; no plugin host required |
| 0b. Platform spike, parallel with 0a | Linux window; Android APK with multiline input and a database write/read | One representative device/emulator check for IME, selection, clipboard, suspend/resume, restart; pin working versions |
| 1. Local editor | Core, storage, and UI integrated; source editing, save status, trash, explicit Markdown import/export | Create/edit/save/restart on Linux; acknowledged content survives |
| 2. Useful notes and Android | Links/backlinks, search, quick switch/palette; Android local editor; Linux directory mirror | One workflow per platform plus a targeted export/recovery scenario |
| 3a. Merge engine | CRDT migration, offline replicas, conflict policy | Deterministic merge/restart scenario without network or GUI |
| 3b. LAN sync | Pairing, authenticated transport, foreground synchronization | Two-device edit/reconnect and unknown-peer rejection |
| 4a. MCP, parallel with 2/3 | Existing commands usable through external tools, including commands without GUI controls | One bridge round-trip plus policy/revision failure coverage |
| 4b. Codex, after 4a | Streaming conversation, document tools, cancellation and approvals | One live integration smoke; editor remains usable on disconnect |
| Optional D-Bus, parallel with 2/3/4 | Session-bus commands and invalidation signal | One round-trip/signal smoke; shared policy/error behavior |
| Optional filters, after preview contract | Linux Pandoc runner and specialized fence output | Callout fixture renders; failure/unavailable runner preserves source |
| Later. Block editing | Source-range block editor experiment | Demonstrate improved editing without source loss before expanding |

The Android spike checks feasibility, not just whether an APK compiles: eframe offers Android activity backends, but acceptable text input must be demonstrated. If blocked, report the concrete limitation early; headless and Linux work can continue without claiming Android readiness. [eframe platform features](https://docs.rs/eframe/latest/eframe/)

Defer graph view, arbitrary docking, a general plugin manager/marketplace, arbitrary plugin UI, custom rich-text infrastructure, selective collaborative undo, attachment sync, automatic external rename tracking, and advanced Markdown rendering. Preserve document-extension interfaces now; choose their runtime implementations and other later increments by user value.

## Testing policy: protect behavior with little overhead

Optimize for useful evidence per unit of development time. Test our invariants at the lowest effective boundary, once. Shared commands eliminate the need to repeat domain tests through GUI, MCP, D-Bus, AI, plugins, and ECS entry points. No coverage quota, mandatory test-per-function rule, or exhaustive permutation suite.

### During development

| Change | Default validation |
| --- | --- |
| Documentation, wording, or plan changes | Read the diff and check formatting/links as relevant; no Cargo runs |
| Layout/theme or straightforward widget wiring | One focused visual check when useful; reuse existing domain coverage; no screenshot suite |
| Domain behavior or bug fix | Run the affected module/scenario; add a focused regression only for a meaningful uncovered behavior |
| Persistence, migration, or export ordering | Focused real SQLite/temp-directory integration test covering the changed durability boundary |
| CRDT mapping or merge policy | Deterministic in-memory replica scenario; no network/UI dependency |
| IPC/network/D-Bus/Codex adapter | Targeted encoding, policy, or lifecycle checks; live smoke only when the integration boundary changes |
| Reader/filter/render adapter | One representative compatible fixture and focused fallback coverage; no suite per plugin or Pandoc AST node |
| Android input, lifecycle, packaging, or dependencies | One relevant device/emulator smoke; do not rebuild APKs for unrelated core changes |

Start by extending an existing scenario or table-driven test. Add a test only if it catches a plausible regression not already covered. Do not test trivial accessors, every enum variant, third-party internals, or implementation details. Use a regression-first loop for a reproduced bug or risky invariant; do not require a failing test ceremony for every low-risk edit.

### Small set of high-value invariants

- Commands reject stale revisions and invalid paths; Unicode edits preserve source bytes outside the edited range.
- A persisted acknowledgement survives restart; stale acknowledgements cannot clear newer dirty edits. A pending export survives interruption and does not silently overwrite externally changed content.
- CRDT replicas converge through disconnect/restart while retaining recoverable rename/delete conflicts. Reuse a few representative scenarios; do not re-test Automerge's entire algorithm.
- Automation cannot bypass command policy or create a second vault writer. One representative adapter parity check establishes routing; domain behavior stays covered at dispatch.

Add these checks when the corresponding subsystem is implemented. A single scenario may cover several invariants; this list is not a demand for a separate suite per bullet. Prefer real in-memory ECS/SQLite and temporary files over layers of mocks. Keep fuzzing, benchmarks, broad GUI automation, and large device matrices out until a concrete failure or performance question justifies them.

### Run cadence and stopping rule

1. For a normal code change, run the smallest relevant command, for example `cargo test --lib --no-default-features notes::tests`. Run `cargo check` instead when compilation is the only useful automated evidence; do not run it redundantly before a test build.
2. At an integration checkpoint, one integrator runs formatting, the fast library tests, and Clippy once for the combined changes. Use `cargo fmt --check`, `cargo test --lib --no-default-features`, and `cargo clippy --all-targets` for the baseline; add only the feature build affected by the increment. Do not run every feature combination or a full release build by default.
3. Run a Linux/Android/live-AI smoke at the relevant delivery gate, or after changes that could invalidate its previous result. A backend-only command does not require a GUI smoke.
4. Once relevant checks pass, stop. Re-run only after a relevant change, a failure, or new evidence of risk. Concurrent developers report their command, scope, and result so the integrator does not repeat unchanged work.

Aim for warm-cache local feedback measured in seconds; keep slow/device/live-service checks outside the edit loop. Report missing validation plainly instead of substituting repeated broad suites. This document is a plan; feasibility gates become verified only when their implementation is built and exercised.
