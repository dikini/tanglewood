# MVP execution status

Active goal from 2026-10-26. Authority: `plan/tanglewood-mvp.md`; scope: `mvp-goal.md`. Latest committed checkpoint: **2dcef01**, unsigned. Next checkpoint ready: authoritative CRDT migration, Codex integration, authenticated LAN transport preparation.

## Scope and evidence

Required increments 0–4 remain intact. Optional D-Bus and external filter process runner are deferred to prioritize required delivery. Block editing/general plugin management remain deferred.

| Task | State | Evidence / remaining dependency |
| --- | --- | --- |
| Shared core, SQLite, Linux editor | Integrated, reviewed | Stable IDs/revisions, single ECS owner, restart/lock/batch durability; actual Linux edit/save/restart |
| Markdown/extensions | Integrated, reviewed | Derived views/source origins/fence attributes/callout/fallback and Pandoc example |
| Mirror/import/export | Integrated, reviewed | Durable outbox/baselines/recoverable conflicts; actual CLI import/rescan/export/restart |
| MCP | Integrated, reviewed | Single-writer Unix bridge, shared schema/policy, exact persisted replies; actual stdio create/restart |
| Authoritative CRDT migration | Ready for checkpoint, reviewed | Backup, schema-3 histories/projections/outbox, generation acknowledgement; CLI migration/restart preserves IDs/revisions/identity |
| Codex | Ready for checkpoint, reviewed | Streamed live note read/edit/read with explicit scoped approvals; cancel/disconnect/restart; actual panel responsiveness |
| LAN | Transport ready, reviewed; owner integration next | Pinned mutual TLS/UDP, pairing/revocation/frame bounds passed. Join, per-document owner protocol, durability barriers, reconnect and integrated scenario required |
| Android | Picker/packaging source in progress | NativeActivity/app-private storage prepared. No SDK/NDK/adb/target/device; APK, input/clipboard/lifecycle/restart/network acceptance open |

Exact checks and boundaries: `integration-evidence.md`, `linux-smoke.md`, `codex-live-evidence.md`, `crdt-migration.md`, `lan-sync.md`. Combined 14 library scenarios passed; focused LAN delta passed after fixes; combined build and Clippy exit 0. No host check proves Android or hardware LAN.

## Ownership and resume point

PM owns manifest/lock/main/status/index and checkpoint integration. Core's reviewed migration/transport source is frozen pending commit; its docs-only `lan-owner-integration.md` plan is ready. Next transfer: core owns App/store/commands/sync/lan/lib for that implementation, PM wires CLI after public contracts stabilize. Platform owns new Android picker/packaging files only until review; UI/android entry integration requires explicit transfer. Reviewer available for the next coherent scope. Only PM stages/commits; no publication authorized.

Next: commit reviewed checkpoint, implement durable LAN owner/join, integrate Android picker/packaging and obtain available platform evidence. Android device-availability question remains pending; finish independent work meanwhile. Goal remains active and incomplete.
