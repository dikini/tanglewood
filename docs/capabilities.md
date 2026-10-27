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
| Vault agent write permission | Local-only durable preference; grants after save, immediate runtime revoke | Settings → Connections and conversation actions | All connected MCP clients; explicit preference overrides startup fallback; stable tool schema, per-request validation | App failure/restart and same-socket grant/revoke regressions |
| Codex conversation/tools | App-server adapter, explicit approvals, cancel/disconnect | Streaming conversation panel | Linux optional codex feature, CLI 0.163.0 | Live scoped note tools/cancel/restart and panel responsiveness passed; see codex-live-evidence.md |
| Vault Codex project / thread states | Durable local CAS binding and bounded project/thread protocol | Sidebar switch, header connection/new-thread icons, rename, history/status, thread context actions and dynamic model/effort picker | Linux optional codex; local-only metadata mutation | Real project create/rename, one scoped reply and history resume; protocol/status and metadata restart/failure fixtures |
| Codex context conveniences | Correlated goal/plan/file events, source snapshots, skill allowlist and paged MCP inventory | Context tabs and composer + menu, PNG/text previews, reviewed plan/sketch requests | Linux optional codex; explicit Send/goal actions | Combined host fixtures and actual metadata/catalog/note-attachment UI; no new generation or live goal mutation |
| Codex profiles / execution | Real collaboration presets, explicit network flag, reviewer routing, bounded command events | Work/Plan/Research/Goal, Access/approvals, goal banner, live command cards | Linux optional codex; Research note creation requires saved vault write permission (legacy --mcp-write fallback) | Protocol/UI regressions; native menu inspection recorded in codex-live-evidence.md; no live research generation claimed |
| Rich native conversation | Exact reply text plus bounded presentation cache and live activity events | Markdown, highlighted code/response Copy, lists/tables/links, read-only checklists and reported plan | Linux optional codex; local preview example; no vault task records | Combined host and native fixture/clipboard checks; no new live model turn required |
| Source-backed MVP-2 editor | Exact original-span revision-checked edits | Ready/Cancel, whole-source and block mode, cached highlighting | Linux/Android egui; in-memory drafts | Focused source/input checks; actual Linux filter/artwork loop and final Android Ready/Cancel/Saved/restart passed |
| External filters / saved artwork | Bounded owned snapshots, ordered Pandoc Lua/JSON/preparation worker | Explicit local resources/configuration; native SVG/PNG preview | Linux external runtimes; Android named Rust subset; no asset sync | Actual Linux pipeline and Android artwork rendering; see MVP-2 evidence/status |
| D-Bus | Deferred optional track | None | Unavailable | No support claimed |

Accepted in-memory mutations are not durable saves. A caller identity is constructed by the trusted adapter; request payloads cannot grant permission. Runtime evidence describes the representative workflows actually run, not a device/IME/ABI matrix. See `android-emulator-evidence.md` and `android-linux-sync-evidence.md`. Physical wireless LAN, ARM64 runtime and other Android keyboards remain unverified.

Codex plan/approval correction (2026-10-27): child update_plan tool enabled;
Work guidance updates one structured plan. Read-only MCP queries have their own
validated tool/annotations. Four explicit command approval modes include No
prompts and Full access; MCP approval policy remains independent. Live disposable
check: four plan updates, two reads, zero approvals, unchanged source. See
[captured evidence](codex-live-evidence.md) and [mode combinations](codex.md).

Research workspaces (2026-10-27): shared revision-checked Research commands and
SQLite acknowledgements; local immutable note/PDF/text source snapshots; source
selection, separate contextual human/agent assessment records, ordinary linked
research notes, citation/page lookup and bounded grounded prompt preparation.
Quiet Research destination prepares a readable question plus evidence attachment
for the existing optional conversation; it never automatically sends a turn.
Report notes can now be created locally from saved checkpoints, or requested as
agent synthesis. Desktop discovery, OCR and output rendering are now implemented as described below;
research attachment sync remains deferred. See
[research usage and evidence](research.md) for limits and actual acceptance.


Research flow/artifacts follow-up: persisted editable briefs, multiple thread
associations, immutable checkpoint rounds and retained local originals/Markdown
with URL/time/provenance. Native explicit HTTP->Markdown archival uses installed
curl/Pandoc. Continue/report supports source-free research context with disclosed
excerpts, a body-free index and exact paginated artifact text reads. Local reports
create new normal notes and link only after save; earlier reports remain unchanged.
Acceptance: combined71 passed1ignored; focused final context/index/headless checks;
actual native PDF/image byte retention, local HTTP archival, two report versions
and exact workspace/file restart. Independent Sol6.1 xhigh review READY. Thread
capture/navigation provenance exercised with controller fixtures; no new live AI
research or multi-agent orchestration claimed. See [research.md](research.md).

Paper save/read (Linux): revision-checked SavePaper upgrades an existing reference,
retains immutable original PDF bytes and extracts page text off the owner thread.
ReadSource paginates exact UTF-8 with physical page numbers. Agent URL downloads
require vault write authorization; local paths remain local-only. Malformed/scanned
PDFs stay retained with explicit extraction error. Real 61-page PDF MCP save/read/
owner-restart check passed; see [research evidence](research.md). Explicit desktop OCR is now available; asset sync remains deferred.

Desktop research completion: reference candidates/Add selected/all, native import/drop,
PDF landing-page resolution and durable batch acquisition/retry, central text/PDF
reader with exact citation/search navigation and explicit English OCR. Crossref
metadata discovery and opt-in lexical evidence previews complement the existing agent.
Contextual assessments/history, explicit artifact roles, report notes and local HTML
slides/SVG infographic/mind-map outputs are available. See [current limits and
validation](research.md). Mobile/sync and multi-agent orchestration are deferred.
