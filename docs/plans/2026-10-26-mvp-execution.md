# Tanglewood MVP Implementation Plan

> Execute within this session using delegated developers and independent combined reviews, under `.agents/workflow.md`.

**Goal:** Deliver the required increments in `../mvp-goal.md`, preserving every required acceptance gate.

**Architecture:** One Rust package, one application-owned ECS World, typed revision-checked commands, asynchronous concrete persistence and I/O workers. Source Markdown remains authoritative until CRDT migration; filters operate only on derived views. The already approved `../plan/tanglewood-mvp.md` specifies the design and supersedes generic workflow choices.

**Tech Stack:** Rust, standalone bevy_ecs, egui/eframe, rusqlite, pulldown-cmark; add Automerge, QUIC/rustls, rmcp and Codex process protocol at their dependency gates. Pin versions after feasibility evidence.

## Execution sequence

1. C0: `Cargo.toml`, `src/{lib,main,app,commands,notes}.rs`. Define UUID note/revision types, records, errors, byte-range edits, caller policy and a headless command scenario. Run `cargo test --lib --no-default-features`. In parallel P0 inventories Android tooling/device and writes `docs/android-feasibility.md`.
2. S1: `src/store.rs`, coordinated app integration. SQLite schema, single-writer vault lock, bounded worker, durable acknowledgements, startup/retry/shutdown. Extend one real temporary-vault scenario for restart/stale acknowledgement.
3. U1: `src/{ui,markdown,extensions,android}.rs`, platform startup/packaging. Source drafts and safe revision conflicts, navigation/search/preview/links/backlinks, source origins/fence attributes/callout fallback. Linux visual workflow; Android actual input/clipboard/lifecycle/storage workflow when available.
4. F2: `src/files.rs`, coordinated store/app commands. Explicit import/export first, then transactional mirror outbox and persisted baselines, rescan, external conflict recovery, safe relative paths and symlink rejection. Focused interruption/external-change recovery evidence.
5. Y3: `src/sync.rs`, coordinated schema/command integration. Preserve pre-migration database, CRDT authoritative state, Unicode splices, persist-before-advertise, recoverable rename/delete conflicts. Deterministic replicas/restart then authenticated QUIC pairing/reconnect/unknown-peer scenario; foreground Android only.
6. A4: `src/{mcp,codex}.rs` and bridge entry point. User-restricted Unix socket into bounded owner queue, schemas and caller policy, await durability externally. Codex app-server initialization/thread/turn stream/tools/cancel/approvals/disconnect and conversation panel; targeted transport scenario and live integration evidence.
7. At each coherent increment freeze scope, independent correctness/scope review, bounded fixes, then relevant combined `cargo fmt --check`, `cargo test --lib --no-default-features`, `cargo clippy --all-targets` and only affected feature checks. PM stages explicit paths, inspects staged diff, commits unsigned, verifies `%G? = N`.
8. Delivery: `README.md`, configuration/permission/build/launch instructions, capability table and concise actual evidence in `docs/mvp-status.md`. Required missing device/service gates stay open; never substitute mocks for live evidence.

The PM task board holds current assignments, exact evidence and resume point. Optional D-Bus and external filter process runner are deferred; no implementation claims for either.
