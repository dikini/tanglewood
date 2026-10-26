# Capability evidence

This table describes implemented behavior separately from GUI exposure and runtime acceptance. It is updated at integration checkpoints.

| Capability | Application API | GUI | Targets / automation | Acceptance |
| --- | --- | --- | --- | --- |
| Create/read/edit/rename/trash/restore | Implemented headless; persistent UUID, opaque revision, UTF-8 byte edits | Editor/list/actions | Host core; trusted read-only/read-write policy | C0 focused scenario passed; independent review ready |
| Search/list | Linear path/body search; optional trash inclusion | Search/quick switch | Host core; read-only allowed | Included C0 scenario |
| Durable save/restart | SQLite bounded worker; explicit flush/shutdown; batch-safe renames | Ready acceptance then save/error/retry/close guard | Linux and Android app-private SQLite | Restart/lock/stale ack/40-note cycle scenarios; Linux window and Android Saved/kill/restart passed |
| Markdown reader/filter/render contracts | Implemented derived views, source origins/fence attrs | Preview, internal links/backlinks | Native reader/callout plus optional documents processing path | Fixture passed; independent review ready |
| Markdown import/export/mirror | Local-only async commands; durable outbox/baselines/conflicts | Separate export status; CLI actions | Linux filesystem paths; Android SAF source integrated | Recovery scenarios and real CLI import/rescan/export/restart passed; independent review ready |
| Android input/lifecycle/storage | NativeActivity/app-private store, framework Editable mirror, SAF and foreground controls | Mobile editor/drawers | API35 x86_64 emulator; Java-capable Gradle APK | API35 AOSP keyboard accent/preedit/emoji/selection/system cut-paste/cancel/handoff and exact saved restart passed; SAF round trip/cancel/lifecycle passed; review ready |
| CRDT/LAN pairing/reconnect | Authoritative migration/persistence, explicit pinned pairing, empty join and per-document Automerge | Manual Sync controls | Optional lan feature; Android source integrated | Two-App and actual Android emulator/Linux join/offline/reconnect/both-owner restart passed; physical wireless untested |
| MCP single-writer bridge | rmcp stdio -> private Unix socket -> App dispatch/durability | Optional owner attachment | Linux mcp; read-only default, explicit write allowlist | Socket policy/revision fixture and live stdio persisted create/restart passed; review ready |
| Codex conversation/tools | App-server adapter, explicit approvals, cancel/disconnect | Streaming conversation panel | Linux optional codex feature, CLI 0.163.0 | Live scoped note tools/cancel/restart and panel responsiveness passed; see codex-live-evidence.md |
| Source-backed MVP-2 editor | Exact original-span revision-checked edits | Ready/Cancel, whole-source and block mode, cached highlighting | Linux/Android egui; in-memory drafts | Focused source/input checks; actual Linux filter/artwork loop and final Android Ready/Cancel/Saved/restart passed |
| External filters / saved artwork | Bounded owned snapshots, ordered Pandoc Lua/JSON/preparation worker | Explicit local resources/configuration; native SVG/PNG preview | Linux external runtimes; Android named Rust subset; no asset sync | Actual Linux pipeline and Android artwork rendering; see MVP-2 evidence/status |
| D-Bus | Deferred optional track | None | Unavailable | No support claimed |

Accepted in-memory mutations are not durable saves. A caller identity is constructed by the trusted adapter; request payloads cannot grant permission. Runtime evidence describes the representative workflows actually run, not a device/IME/ABI matrix. See `android-emulator-evidence.md` and `android-linux-sync-evidence.md`. Physical wireless LAN, ARM64 runtime and other Android keyboards remain unverified.
