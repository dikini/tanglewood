# Capability evidence

This table describes implemented behavior separately from GUI exposure and runtime acceptance. It is updated at integration checkpoints.

| Capability | Application API | GUI | Targets / automation | Acceptance |
| --- | --- | --- | --- | --- |
| Create/read/edit/rename/trash/restore | Implemented headless; persistent UUID, opaque revision, UTF-8 byte edits | Editor/list/actions | Host core; trusted read-only/read-write policy | C0 focused scenario passed; independent review ready |
| Search/list | Linear path/body search; optional trash inclusion | Search/quick switch | Host core; read-only allowed | Included C0 scenario |
| Durable save/restart | SQLite bounded worker; explicit flush/shutdown; batch-safe renames | Autosave/error/retry/close guard | Host Linux; Android build open | Restart/lock/stale ack/40-note cycle scenarios passed; Linux window restart passed |
| Markdown reader/filter/render contracts | Implemented derived views, source origins/fence attrs | Preview, internal links/backlinks | Native reader and callout example; external runner deferred | Fixture passed; independent review ready |
| Markdown import/export/mirror | Local-only async commands; durable outbox/baselines/conflicts | Separate export status; CLI actions | Linux filesystem paths; Android SAF source integrated | Recovery scenarios and real CLI import/rescan/export/restart passed; independent review ready |
| Android input/lifecycle/storage | NativeActivity/app-private store, SAF and foreground controls integrated in source | Mobile editor/drawers | Android emulator provisioning; cross-build and acceptance pending | Host JNI type check and source review passed; APK/input/lifecycle/provider gates open |
| CRDT/LAN pairing/reconnect | Authoritative migration/persistence, explicit pinned pairing, empty join and per-document Automerge | Manual Sync controls | Optional lan feature; Android source integrated | Two-App offline/reconnect/restart and actual CLI loopback passed; Android emulator/network workflow open; physical wireless untested |
| MCP single-writer bridge | rmcp stdio -> private Unix socket -> App dispatch/durability | Optional owner attachment | Linux mcp; read-only default, explicit write allowlist | Socket policy/revision fixture and live stdio persisted create/restart passed; review ready |
| Codex conversation/tools | App-server adapter, explicit approvals, cancel/disconnect | Streaming conversation panel | Linux optional codex feature, CLI 0.163.0 | Live scoped note tools/cancel/restart and panel responsiveness passed; see codex-live-evidence.md |
| D-Bus / external filter process | Deferred optional tracks | None | Unavailable | No support claimed |

Accepted in-memory mutations are not durable saves. A caller identity is constructed by the trusted adapter; request payloads cannot grant permission. Runtime gates remain open until their actual workflows run.
