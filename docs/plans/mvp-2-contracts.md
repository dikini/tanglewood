# MVP-2 shared contracts and P0 decisions

Implementation record for the authorized [updated plan](2026-10-26-mvp-2-proposal.md). The user subsequently reduced scope: the proposal’s controlling scope overrides this original contract backlog. Multi-pane ownership, durable recovery storage, bundle publication and asset transport below are deferred designs, not requirements implemented in this increment. The concrete delivered processing contract is in `mvp-2-processing.md`; observed P0 proof is in `mvp-2-platform-feasibility.md`. No capability is declared complete by this design document.

## Source and view ownership

- One `App`/ECS owner, existing persistence worker and shared revision-checked mutation dispatch remain authoritative. No source entity per rendered block, second writable note store or UI-held `World` mutex.
- One shared editor session per note, referenced by every tab/pane displaying it. Only the focused pane owns text input; other panes show the same transaction or its accepted presentation read-only.
- An edit transaction captures base note revision, exact source text, selected whole-source/block ranges and a fresh widget/input generation. Keep original UTF-8 offsets; do not remap against an unseen new revision.
- Ready is disabled during IME composition. It builds one non-overlapping `EditNote` batch against the original revision. Both block and whole-source modes use Ready/Cancel; no keystroke autosubmit survives adoption of the controller.
- Cancel removes only the unsubmitted transaction. A stale Ready retains base and draft and offers copy/recovery/reload/reapply; it never retries under a refreshed revision silently.
- Block/mode switches in a note require Ready or Cancel first; switching tabs/panes/notes preserves the transaction and invalidates the old focus token. A reused note has no independent competing draft.
- Per-note device-local recovery contains base revision/source, draft, mode/ranges and generation. A bounded asynchronous latest-state writer publishes complete recovery files atomically. A checkpoint acknowledgement is distinct from canonical Saved; full/error queues retain dirty recovery work visibly. Recovery is reopened as an unsubmitted draft.
- Ready acceptance does not retire acknowledged recovery: retain the transaction through Accepted/Unsaved until its exact accepted revision, or a confirmed covering canonical history generation, is durable. Correlate retirement with that recovery transaction/generation; an old save acknowledgement or delayed cleanup cannot remove a newer checkpoint. Explicit Cancel/discard queues an ordered retirement barrier after any in-flight checkpoint so discarded drafts cannot reappear after restart.
- Whole-source mode supplies full-document selection and clipboard; block mode supplies selection in one active source editor and adjacent-block navigation. Rich-render cross-block drag selection and accepted collaborative undo are out of scope.

## Immutable processing input and result

The concrete snapshot handed to a worker owns note ID, accepted revision, logical vault-relative source path, exact source bytes and an optional selected asset-manifest identity. A job carries a requester/session generation, target (`json` native preview or `html` export), reader/options and the locally configured ordered pipeline identity/configuration.

An asset snapshot is a complete immutable manifest plus readable immutable blob handles/paths. It is not a pointer to mutable live files. Materialize the logical source/resource layout in a job-owned workspace so filters receive their expected relative paths. Never make a workspace pathname canonical.

One bounded processing worker coalesces newer waiting requests per note/target. The current request may be cancelled; children are terminated and reaped without blocking frames. A result includes the complete job key, preserved Pandoc JSON envelope or published export artifact, owned asset lifetime and bounded diagnostics. The owner accepts it only if note revision, path, manifest, configuration, target and requesting session still match.

Linux uses actual Pandoc with declared JSON/Lua order and writer target. Nonstandard `illustration-md.prepare --source` is an explicit stage shim, not misrepresented as a normal positional-argument filter. Ordinary display never executes illustration authoring code.

Native Android investigates a named offline Rust preparation subset consuming the same immutable source/resources. Required semantic/rejection behavior is compared with the pinned upstream Python stage and observed on the emulator. It is not arbitrary Pandoc compatibility. Remote processing is not selected or authorized as a substitute.

## Source bindings and rendering

Maintain source indices separately from the Pandoc wire AST. Native parser offsets are valid only for their exact note revision. After a general transformation, output is unbound unless a specific adapter proves identity. Neither rendered-text equality nor output order proves an origin.

The illustration adapter validates unique matching figure/fence/metadata IDs. A rendered figure may bind to more than one source range (fence and frontmatter). Initially frontmatter is edited as exact source. Clicking an obsolete result never applies its old ranges; open current source instead.

The UI owns textures/layout/cursor caches; owned semantic data and source indices contain no egui handles. Reuse one vertical-flow document renderer for Notes, Academic and Tufte-inspired profiles. Unknown nodes remain explicit diagnostic/source presentation, not silently omitted or rewritten into source.

## Assets, causality and durable publication

Proposed storage: per-vault immutable blobs addressed by SHA-256 and versioned canonical manifests mapping safe document-relative paths to hash/length/media type. A note/bundle is identified by note UUID, not its current display path. No garbage collection of accepted/recoverable versions in MVP-2.

Publishing assets is local/explicit and checks expected note revision and expected manifest identity. First stage, hash, validate and fsync all referenced blobs. Then commit the selected manifest and any compound source/history mutation at one SQLite durability boundary. Acknowledgement means old or new complete publication, never a sidecar/artwork half-pair.

For synced notes, the selected manifest identity is carried inside the note's authoritative Automerge history. Compound text/manifest changes are committed in the same note change; source-only edits intentionally retain the current artwork. Local opaque revisions are not used as cross-device causal clocks. Concurrent values of the manifest selection are retained and exposed as a conflict. Explicit resolution selects a complete retained version; image bytes are never text-merged.

The durable projection can distinguish selected/conflicted/pending manifest references from locally complete renderable bundles. A history update may refer to a not-yet-downloaded bundle; source stays editable, but processing reports pending/conflicted resources instead of combining it with a different manifest. Complete-bundle acknowledgement is separate from text-history durability.

Extend the existing authenticated session protocol with bounded capability negotiation, manifest and chunked blob messages. Verify counts/sizes/hash/path metadata; persist verified blobs before installing complete bundle availability. Persist pending missing-hash state for restart/retry. An older peer remains text-capable and is never told that it received asset support. Retired sessions cannot satisfy another session's transfer receipt.

Use a small explicit portable bundle format for import/export, with source plus manifest/blobs and versioned limits. Validate all entries before owner-controlled publication; no path traversal, duplicate aliases, symlink escapes or replacing unrelated destination files. Android consumes an explicitly selected bundle through the picker; no expanded storage permission is assumed. Choose the exact wire/archive layout and quotas before A1/A2 implementation.

## Dependency and ownership decisions

- Keep egui/eframe 0.33.3 unless R0 finds a required incompatibility that warrants an explicit coordinated update.
- Do not add docking or a full component kit merely for simple panels/tabs. R0 owns the reuse/adapt/build evidence and exact icon/font/highlighter choices.
- Introduce a small optional `documents` feature for the actual headless processing/asset backend; GUI will depend on it only when integrated. JSON/hash/parser/image dependencies are chosen from the demonstrated backend needs, not speculative framework architecture.
- Root owns manifest/lock/shared modules and integration. Research writers currently own only their assigned docs/fixtures. Production module ownership and exact APIs are transferred explicitly after the proof handoff.
- Linux/Android actual acceptance, narrow independent review and relevant feature checks close gates. Documentation, mock output or host-only compilation cannot close a device/runtime gate.
