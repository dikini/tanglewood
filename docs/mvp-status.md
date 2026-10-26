# MVP execution status

Active goal from 2026-10-26. Authority: `plan/tanglewood-mvp.md`; scope: `mvp-goal.md`.
Latest integrated checkpoint: **5a40e2c**, unsigned (`%G? = N`), Linux source editor/core/storage/document contracts.

## Scope

Required increments 0–4 remain intact: Linux + Android notes, durable portability/mirror, CRDT/LAN, MCP/Codex, extension contracts and delivery evidence. Optional D-Bus and external filter process execution deferred to prioritize required work. Block editing/general plugins and other plan-deferred features remain deferred.

| Task | Ownership / state | Evidence and next dependency |
| --- | --- | --- |
| C0 shared contracts | done; PM integrated | Stable UUID/revision, UTF-8 commands/policy, single ECS owner; reviewed |
| S1 SQLite durability | done; PM integrated | Restart, lock, stale acknowledgements, atomic 40-note path cycle; reviewed |
| D1 reader/extensions | done; PM integrated | Native source origins/fence attrs/callout/source fallback fixture; reviewed |
| U1 Linux editor | done; PM integrated | Source drafts, nav/search/preview/links/backlinks/save/error/close; real window edit/save/restart |
| P0/P1 Android | source preparation reviewed; runtime blocked | NativeActivity/app-private path prepared. No SDK/NDK/adb/device/target. APK/input/clipboard/lifecycle/restart unverified; required SAF picker unimplemented |
| F2 mirror/portability | reviewed; next checkpoint pending | Async local-only import/export/rescan, durable outbox/baselines/conflicts. Identical-text rename cleanup bug fixed with conservative ownership conflicts; recovery through public renames tested |
| A4a MCP | reviewed; next checkpoint pending | rmcp stdio -> private Unix socket -> sole App. Shared schema/policy; exact durable replies; actual stdio create/restart smoke passed |
| Y3a engine | isolated preparation reviewed | Automerge0.7.4 UnicodeCodePoint histories/merge scenario passed. Duplicate catalog UUID spellings now reject; targeted scenario passed. Not yet a sync milestone |
| Y3b migration/LAN | required, not implemented | Backup + authoritative histories/projections/outbox; command integration; then pinned QUIC pairing/reconnect/two-device evidence |
| A4b Codex | adapter preparation; review | codex-cli0.163.0 schema pinned, focused protocol fixture passed. Panel/live turn/tools/cancel/approvals/disconnect evidence still required |

## Latest evidence

See `integration-evidence.md` and `linux-smoke.md` for exact scope.

- `cargo fmt --check`: passed before new Codex module; rerun at next relevant checkpoint.
- `RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --features sync,mcp --offline`: **10 passed**, Unix socket scenario with approved escalation. Combined run preceded tiny catalog-identity fix; targeted `sync::tests` passed after that delta.
- `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,mcp,sync --offline`: exit 0, style/large concrete queue error and unused pre-integration CRDT operation warnings.
- `RUSTC_WRAPPER= cargo build --locked --bins --features gui,mcp,sync --offline`: passed.
- Actual Linux GUI create/multiline Markdown/preview/Saved/process restart recovered identical source. Isolated Xvfb requires approved escalation; temporary processes stopped.
- Actual CLI import -> mirror -> external-only rescan -> export -> restart retained Unicode source bytes.
- Actual rmcp stdio initialize/list/create -> Unix owner returned accepted=persisted revision; SQLite bytes and restart verified. Socket fixture covers read-only/policy/schema/stale/original acknowledgement.
- Codex adapter fixture: one passed (feature codex); no live generation. Login status reports ChatGPT; no credentials read or logged.

## Current ownership and resume point

PM owns manifest/lock/main/status/index; only PM stages/commits. Core owns migration proposal only in new docs; sync.rs identity fix is frozen and independently ready. F2 app/store/files/commands/notes are frozen and reviewed. Platform handed back mcp/bridge/UI and is holding Codex source frozen for reviewer. Reviewer covers isolated engine delta and Codex preparation. No Cargo overlap: core targeted identity fix passed; PM checkpoint checks already recorded; coordinate any further check.

Next: commit reviewed F2/A4a/Android/engine preparation unsigned, then release App/store to authoritative CRDT migration and UI to Codex panel. Codex unreviewed source stays outside that checkpoint. Preserve pending work; no reset/stash/clean. No external publication authorized.

Android target-availability question is pending asynchronously; finish independent work. Host compilation does not close Android, networking or live AI gates. Goal remains active and incomplete.
