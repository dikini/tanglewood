# MVP execution status

Goal active from 2026-10-26. Architectural authority: `plan/tanglewood-mvp.md`; execution scope: `mvp-goal.md`. Baseline `40f3fc1` is documentation only; clean checkout at kickoff.

## Scope and gates

Required: increments 0–4, Android feasibility/local workflow, Linux mirror, authenticated LAN sync, Linux MCP/Codex, document extension contracts and delivery evidence. Optional D-Bus and external filter execution are deferred to keep required work moving; interfaces/examples remain required. Deferred plan features remain deferred.

| Task | Dependency / ownership | State | Evidence / next action |
| --- | --- | --- | --- |
| C0 core contracts | developer core; PM integration | done | One headless scenario passed; independent C0 review ready |
| P0 platform feasibility | developer platform; PM display probe | blocked | Android SDK/NDK/target/device absent; Linux escalated OpenGL probe succeeded; real workflow open |
| S1 durable local storage | developer core; independent review ready | done | Five combined scenarios pass after atomic batch fix; real Linux saved/restart workflow passed |
| U1 source editor / documents | developer platform; independent review ready | done | Linux editor/navigation/preview implemented; focused GUI test passes. Android entry prepared separately, device gate open |
| F2 directory mirror | developer core: files/protocol; S1 dependency | active | Filesystem helpers drafted; transactional outbox/app integration still required |
| Y3 CRDT and LAN | S1; assignment pending | ready | Migration, merge/restart, pairing/reconnect, unknown peer rejection |
| A4 MCP / Codex | S1; assignment pending | ready | Single writer bridge, durable external replies, streamed turns/approvals/cancel |
| R reviews/integration | PM shared index; independent reviewer | ready | Freeze coherent increments; combined checks and unsigned checkpoints |

## Decisions and validation discipline

Use one package and a shared checkout with disjoint ownership. PM owns shared files after explicit worker handback. One Cargo job at a time. Existing approved design and goal authorize implementation; routine choices require no repeated approval. Use focused behavioral scenarios and one relevant combined integration check, not feature matrices.

No implementation or runtime gate verified yet. Device, networking and live AI evidence must be recorded separately. Do not treat host compilation as Android acceptance.

## Evidence and resume point

- `RUSTC_WRAPPER= cargo test --lib --no-default-features --offline`: 4 passed, 0 failed before S1 batch fix (C0, SQLite restart/lock/rollback/backpressure, stale ack/restart, D1 callout/source fallback).
- Independent review: C0 ready; D1 ready; S1 changes needed for accepted path swap whose coalesced per-note writes violate SQL path uniqueness. Fix all pending records atomically; no subset that splits cycles.
- `RUSTC_WRAPPER= cargo check --features gui --offline`: passed; developer refining async save/close, IME, Unicode diff and links before UI handoff.
- GUI dependencies pinned eframe/egui 0.33.3; Rust 1.94.0. `cargo fetch` approved network access succeeded. Sandbox sccache fails; unset wrapper for Cargo.
- Android platform gate remains unverified; no SDK/NDK/adb/device. Linux display works with approved escalation (`glxinfo -B`); actual Tanglewood smoke pending.

Core owns S1 batch fix and files.rs; platform owns ui.rs; D1 source frozen and approved. PM owns main/manifest/status/index. No commits made yet; integrate a reviewed checkpoint after fix. No external publication authorized.

## Checkpoint validation

S1 atomic batch fix and U1 review deltas independently ready. Combined `cargo fmt --check` passed; `RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --offline` passed 5 scenarios. UI targeted test passed 1 (5 filtered). `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui --offline` exits 0 with style warnings (collapsible if / needless range loop). A stricter exploratory `-D warnings` run failed those style warnings; it is not the plan gate. GUI build passed. Actual Linux Xvfb source edit/preview/Saved/process restart recovered exact multiline text; see linux-smoke.md. This does not close Android, mirror, sync, or live AI gates.

Next: checkpoint reviewed C0/S1/D1/U1, then release F2 shared paths; integrate prepared Android entry without claiming APK verification. Optional edge: colon inside a link anchor currently rejected by resolver.
