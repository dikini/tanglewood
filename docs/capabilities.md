# Capability evidence

This table describes implemented behavior separately from GUI exposure and runtime acceptance. It is updated at integration checkpoints.

| Capability | Application API | GUI | Targets / automation | Acceptance |
| --- | --- | --- | --- | --- |
| Create/read/edit/rename/trash/restore | Implemented headless; persistent UUID, opaque revision, UTF-8 byte edits | Editor/list/actions | Host core; trusted read-only/read-write policy | C0 focused scenario passed; independent review ready |
| Search/list | Linear path/body search; optional trash inclusion | Search/quick switch | Host core; read-only allowed | Included C0 scenario |
| Durable save/restart | SQLite bounded worker; explicit flush/shutdown; batch-safe renames | Autosave/error/retry/close guard | Host Linux; Android build open | Restart/lock/stale ack/40-note cycle scenarios passed; Linux window restart passed |
| Markdown reader/filter/render contracts | Implemented derived views, source origins/fence attrs | Preview, internal links/backlinks | Native reader and callout example; external runner deferred | Fixture passed; independent review ready |
| Markdown import/export/mirror | Pending | Pending | Linux mirror; Android picker required | Not verified |
| Android input/lifecycle/storage | Pending | Pending | Tooling/device absent here | Gate open; see android-feasibility.md |
| CRDT/LAN pairing/reconnect | Pending | Pending | Foreground Android and Linux | Not verified |
| MCP single-writer bridge | Pending | Independent of GUI | Linux optional feature | Not verified |
| Codex conversation/tools | Pending | Pending | Linux optional feature | Not verified |
| D-Bus / external filter process | Deferred optional tracks | None | Unavailable | No support claimed |

Accepted in-memory mutations are not durable saves. A caller identity is constructed by the trusted adapter; request payloads cannot grant permission. Runtime gates remain open until their actual workflows run.
