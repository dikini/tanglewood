# Research Flow and Artifacts Implementation Plan

**Goal:** Capture ongoing, multi-thread investigations and retain their evidence/artifacts, with research reports requested on demand.

**Architecture:** Extend the existing revision-checked Research workspace records and SQLite persistence. A bounded asset worker retains original files and optional web Markdown copies; the existing Codex panel contributes discussion snapshots and round results through shared commands. Native UI presents the investigation, associated threads, chronological rounds and artifacts without replacing normal notes or conversations.

**Tech Stack:** Existing Rust/Serde/SQLite/egui/Codex adapter; local curl/Pandoc only for explicitly requested web archival, with honest unavailable errors. No new database writer or agent orchestrator.

## Product and compatibility decisions

- A workspace is an investigation; multiple provider/project/thread references participate. Agent identity is separately optional. Linking does not merge transcripts or start turns.
- Editable brief holds purpose/questions/scope/assumptions/open questions. A round is an immutable checkpoint with timestamp, intent, relevant discussion/result snapshot, thread/turn provenance when known, findings, open questions and referenced artifact IDs. Summaries are checkpoints, not completion events.
- Existing stored workspace JSON loads with defaulted new fields. Existing source/note/citation behavior remains valid. Revision checks and durable acknowledgements still apply.
- Artifacts can be references or retained originals: PDFs, images/infographics, arbitrary bounded files, supplied Markdown and web URLs. Web archival is explicit; retain URL/capture timestamp and Markdown only after successful conversion. Never claim an external link has been archived.
- Store originals under vault-owned research-assets with generated identities. Worker snapshots input, enforces bounds/timeouts, atomically publishes files, and leaves originals unchanged. Failed/stale workspace registration preserves the retained result for explicit retry; no automatic deletion of referenced artifacts. Assets/metadata remain local-only.
- Discussion captures are user context, not evidence. Report requests combine brief, checkpoints and selected evidence/artifacts, distinguish unsupported/inferred findings and external-only links, and use existing agent/MCP note commands. Do not overwrite an existing report.
- Support both existing-sources-only continuation and explicit external gathering requests using available Codex capabilities. No invented search backend or claim of autonomous orchestration. Persist round checkpoints across sessions; no paid turn during tests unless explicitly necessary.

## F1: Domain / persistence / dispatch

Owner: flow_core. Files: src/research.rs, src/app.rs, src/commands.rs, src/store.rs, src/lib.rs. Root owns dependency adjustments.

1. Add defaulted brief/thread/round/artifact records and commands; publish exact API for UI/worker before parallel integration.
2. Validate IDs, lengths, provenance, existing associations, CAS, caller policy, artifact URLs/managed paths and immutable checkpoints.
3. Integrate artifact worker requests/completions with App ownership and existing io_status/durability handling; no file reads/network on UI thread.
4. Build source-free initial/continuation/report context including selected source snapshots, relevant rounds and artifacts under explicit budget. Preserve existing selected-only context behavior.
5. Extend a focused real App/SQLite scenario for old-record loading, multi-thread/round/artifact restart, stale writes and report context. Cargo runs serialized with root.

## F2: Retained artifact worker

Owner: flow_assets. File: new src/research_assets.rs. Contracts coordinated with F1; no shared/index/dependency changes.

1. Bounded worker API for explicit local file retention, URL-only reference, supplied Markdown and optional URL-to-Markdown capture.
2. Snapshot files with generated paths and bounded bytes; retain originals for PDF/images/other files.
3. For web archival use direct subprocess args (curl HTTP(S), bounded bytes, redirects/time, followed by Pandoc html->gfm), preserving final/source URL and captured timestamp. No browser scripts or shell command interpolation. Empty/failed conversion remains error, never archived success.
4. Cancel, shut down, clean incomplete outputs; avoid UI joins and pipe deadlocks. No SQLite writes. Tests cover bytes/path/lifecycle and a real local HTML fixture; integration uses real installed tools when available.

## F3: Native chat / workspace workflow

Owner: flow_ui. Files: src/ui.rs, src/ui/research.rs. No edits to core/worker files.

1. Add Start/associate research from current conversation, using bounded relevant discussion capture and an editable initial brief. Associate multiple project-listed threads; selecting a research thread uses existing draft-preserving thread switching.
2. Present/edit brief, chronological checkpoint rounds, associated threads and artifact collection in contextual sections. Preserve existing note drafts, question edits and import ownership.
3. Capture a conversation result as a new round with provenance and explicit summary/open questions; keep previous checkpoints unchanged. Support manual checkpoint without Codex.
4. Retain local artifact paths and web links, optional web copy, Markdown captures; show reference vs retained state, errors and pending actions honestly. Open/preview artifacts using existing safe rendering boundaries where practical; never execute HTML/scripts.
5. Continue/report prepares a readable agent draft plus evidence/context attachment; existing drafts remain untouched. Explicitly choose source-only vs external-gathering scope; normal Send starts the work. Reports are ordinary new notes via MCP; offer linking returned notes rather than claim automatic completion.
6. Focused tests for round provenance/context, draft safety, context-budget errors and worker correlation; actual Linux workflow check at integration.

## F4: Review / acceptance / records

Owner: root and independent Sol 6.1 xhigh review. Developers use Sol 6.1 medium. Shared checkout, disjoint ownership, serialized Cargo/index, no recursive agents.

Freeze completed lanes for independent review while other lanes progress. Correct data-loss, stale-state, path/URL, resource/cancellation and contract findings. Run relevant headless and gui,codex tests, formatting and all-target Clippy once at combined checkpoint, then focused reruns only for corrections. Build final Linux app and exercise real workspace/thread/checkpoint/artifact/report-preparation flow, including actual retained file and optional local web-copy fixture; restart and inspect saved records. Record commands/results/limitations in docs/research.md and status/capability docs. Android, LAN attachment sync, live AI success and multi-agent orchestration remain unproven/deferred unless actually exercised.

## Task board

| Task | Owner | State | Next dependency |
| --- | --- | --- | --- |
| F1 | flow_core | done | Old JSON/CAS/restart, bounded reads and retained receipts |
| F2 | flow_assets | done | Exact original bytes; real local curl/Pandoc archival |
| F3 | flow_ui | done | Native artifacts, two round-based reports and restart |
| F4 | root/reviewer | done | Independent READY; exact evidence in docs/research.md |
