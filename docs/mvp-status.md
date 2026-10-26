# MVP execution status

Active goal from 2026-10-26. Authority: `plan/tanglewood-mvp.md`; scope: `mvp-goal.md`. Latest committed checkpoint: **499c870**, unsigned (`%G? = N`): authoritative CRDT migration, Codex integration, authenticated LAN transport preparation. Prior checkpoints: **2dcef01**, **5a40e2c**.

## Scope and evidence

Required increments 0–4 remain intact. Optional D-Bus and external filter process runner are deferred to prioritize required delivery. Block editing/general plugin management remain deferred.

| Task | State | Evidence / remaining dependency |
| --- | --- | --- |
| Shared core, SQLite, Linux editor | Integrated, reviewed | Stable IDs/revisions, single ECS owner, restart/lock/batch durability; actual Linux edit/save/restart |
| Markdown/extensions | Integrated, reviewed | Derived views/source origins/fence attributes/callout/fallback and Pandoc example |
| Mirror/import/export | Integrated, reviewed | Durable outbox/baselines/recoverable conflicts; actual CLI import/rescan/export/restart |
| MCP | Integrated, reviewed | Single-writer Unix bridge, shared schema/policy, exact persisted replies; actual stdio create/restart |
| Authoritative CRDT migration | Integrated, reviewed | Backup, schema-3 histories/projections/outbox, generation acknowledgement; CLI migration/restart preserves IDs/revisions/identity |
| Codex | Integrated, reviewed | Streamed live note read/edit/read with explicit scoped approvals; cancel/disconnect/restart; actual panel responsiveness |
| LAN | Owner integration implemented; reviewed | 15 combined scenarios passed incl two-App join/offline/reconnect/restart/init pause. Actual CLI tickets/confirmation/join/Unicode/identity/restart passed on loopback; Android emulator/two-device gates open |
| Android | Picker/packaging, mobile editor and foreground LAN integrated in source; reviewed | NativeActivity/app-private storage prepared. SDK/NDK provisioning active; Android Rust targets installed. APK, input/clipboard/lifecycle/restart/network acceptance open |

Exact checks and boundaries: `integration-evidence.md`, `linux-smoke.md`, `codex-live-evidence.md`, `crdt-migration.md`, `lan-sync.md`. Combined 15 library scenarios passed; combined build and Clippy exit 0; actual LAN CLI workflow passed. No host check proves Android or hardware LAN.

## Ownership and resume point

PM owns manifest/lock/main/status/index and checkpoint integration. Core owns App/store/commands/sync/lan/lib for the approved `lan-owner-integration.md` implementation. PM wires CLI after public contracts stabilize. Platform owns UI/android entry and Android packaging/docs, now adding explicit LAN controls and a retained foreground runtime. Picker JNI host type-check passed. Platform review is ready after callback spool ownership, IME drawer transition, bounded Sync scrolling and nonblocking runtime teardown fixes. Core must guarantee immediate Pause cancellation including pending listener initialization. New source remains uncommitted pending coherent host integration checks and core review. LAN review is ready after receive-generation barriers, ordered/session-correlated retry, cancelled control outcomes, serialized pin persistence and owner revocation fixes. Final focused checks, host binary build and actual CLI workflow passed. Cargo is free for the selected Android emulator cross-build. Platform source review is ready; host UI/CLI compilation passed. PM retains main/docs/status/index. Only PM stages/commits; no publication authorized.

Next: checkpoint the reviewed LAN/Android source increment, finish temporary SDK/NDK provisioning, build the x86_64 APK and run emulator acceptance. The user accepted the SDK license and chose a local emulator. KVM is readable/writable outside the sandbox. A representative Android emulator plus Linux app can satisfy the plan’s platform/two-device workflow; physical wireless testing is additional evidence, not a new release gate. Goal remains active and incomplete.
