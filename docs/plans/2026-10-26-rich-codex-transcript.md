# Rich Codex transcript implementation plan

**Goal:** Make the existing native egui conversation resemble Codex's readable Markdown and quiet progress presentation.

**Architecture:** Keep exact reply text and structured app-server events authoritative. Add a view-local, bounded Markdown presentation cache; reuse native prose/code styling. Keep the vault as the project, with Work (including development), Research, Plan and Goal profiles using one runtime and thread lifecycle. No new project database, repository abstraction, automatic memory, scheduler or backend.

**Tech stack:** Existing egui, pulldown-cmark, syntect; no new dependencies or WebView.

## Accepted design and scope

The user accepted the prior proposal: Markdown headings/emphasis/lists/links/tables/inline code, highlighted code cards with language and Copy, quiet expandable command/tool activity, restrained bubbles/spacing and stable streaming reading position. Use the native renderer rather than HTML/webview or a new external widget stack. Current note renderer supports prose/code but leaves lists/tables as source; a small transcript presentation parser can reuse its primitives without changing canonical note parsing.

A project is a vault. Development is Work mode. Existing explicit snapshots provide the research-note-to-work-thread handoff; creating notes remains through MCP with --mcp-write and revision/durable-save rules. Research should save useful cited notes under Research/<topic>/. Document the desired live handoff without claiming it was proven by UI fixtures.

Todos: render Markdown task markers as read-only state and show the existing structured Codex plan steps in the conversation. Pending/in-progress/completed plans are agent-reported, not a durable editable task manager. No new task entity store or note changes from a response checkbox.

## Tasks and ownership

1. Renderer developer owns only src/ui/codex_markdown.rs (new), src/ui/code.rs and the minimal shared prose visibility change in src/ui/document.rs. Implement bounded cached Markdown presentation, ordinary lists/nesting, task markers, horizontal-scroll tables, quotes/links, highlighted code with exact Copy. Handle partial streamed Markdown and unknown constructs as readable text. No automatic external/local image fetch, HTML execution or filters. Use safe explicit http/https clicks only; file/vault links need root dispatch, not automatic file access. Focused meaningful parser/cache/copy fixtures. No Cargo until root grants the slot.
2. Protocol developer owns only src/codex.rs. Add bounded scoped Activity events for MCP calls, web searches and file changes using existing active-turn guards. Keep command cards intact; never expose raw private reasoning. Focused lifecycle/stale/size fixtures; no paid generation, config or permission changes. No Cargo until root grants the slot.
3. Root owns src/ui.rs, docs and integration. Register renderer, cache UI presentation, show structured activities and reported plan task list, preserve snapshots and thread state, streaming stick-to-bottom behavior and transcript limits. Add a deterministic transcript fixture/demo path for native inspection if a simple existing test mechanism permits; no user data or paid prompts required for layout validation.
4. Freeze code and request independent review of correctness, bounds, link/file boundaries, cache invalidation, thread ownership and source preservation. Root runs one relevant combined host test/build checkpoint, then native UI inspection. Fix concrete findings only. Record actual evidence and limitations. No commit requested.

## Checks and stopping rule

Use existing parser/highlighter tests plus one uncovered transcript fixture with Unicode, emphasis, nested list/task/table, link, HTML/comment and unfinished fenced code, exact source/copy and cache invalidation/bounds. Protocol fixture should prove foreign/stale item rejection and bounded details. Keep Cargo serialized with RUSTC_WRAPPER=. Do not repeat domain scenarios through UI. Native smoke checks layout/wrapping/scroll and Copy; a live Research -> durable note -> Work attachment run remains a distinct acceptance check if performed, never inferred from mocks.

## Delivery checkpoint

RT1–RT4 done; code/state and final presentation delta independently reviewed READY.
Combined host50pass/1ignored/1filtered; final transcript/keyboard2pass; native bins
and debug preview build passed. Native Copy values matched exact response and
code source. See docs/codex-live-evidence.md for commands and actual limits.
No vault task store or live research handoff is claimed. Changes remain
uncommitted as requested by the session's existing integration discipline.
