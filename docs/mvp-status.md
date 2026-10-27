# MVP execution status

Required MVP increments 0–4 are delivered and independently reviewed, with actual Linux, Android emulator, LAN and live Codex evidence. Goal authority: `plan/tanglewood-mvp.md` and `mvp-goal.md`. Implementation checkpoints before the final Android integration: **ddc8fb3**, **499c870**, **2dcef01**, **5a40e2c**, all unsigned. The final delivery commit is the Git commit containing this record.

| Scope | State | Acceptance evidence |
| --- | --- | --- |
| Shared core / SQLite / Linux editor | done | Stable IDs/revisions, single ECS owner, exact durable acknowledgement, restart/lock/batch recovery; actual Linux window edit/save/restart |
| Markdown / extensions | done | Derived reader/filter/render/source-origin/fence contracts, callout and Pandoc-compatible example, canonical fallback |
| Import/export / Linux mirror | done | Durable outbox/baselines/recoverable conflicts, actual CLI import/rescan/export/restart |
| MCP / Codex | done | Private single-writer socket, shared validation, persisted replies; live scoped read/edit/read, stream/cancel/disconnect/restart and panel responsiveness |
| CRDT / authenticated LAN | done | Backup/migration, authoritative histories, generation barriers, pinned TLS, empty join, offline merge/conflicts; actual Android/Linux pairing/reconnect/both-owner restart |
| Android packaging / SAF / lifecycle | done | Java-capable API35 x86_64 APK, exact-byte SAF round trip/cancel, system/keyboard insets, rotation/background/restart |
| Android text input | done | Framework Editable/InputConnection mirror; real AOSP preedit/commit/accent/emoji, selection, system cut/paste, cancel/handoff and exact Saved/kill/restart |
| Delivery / review | done | Reproducible build/launch/configuration docs and capability table; scoped independent reviews and relevant integration checks |

## Final integration evidence

`integration-evidence.md` records commands/results. Combined 15 domain/adapter scenarios passed before final LAN corrections; focused post-correction scenarios, final host build and real CLI smoke passed. The Android input fixture passed UTF-16/surrogate/session/coalescing/epoch/handoff/menu regressions. Combined host all-target Clippy (`gui,codex,lan`) and final formatting passed. Actual final x86_64 Rust+Java APK build passed; SHA-256 `7b2897d9fbbf46e7267164f573432e3c3b20bf8ba9f219f146210be53fdf0ddb`.

Final Android input changes were independently reviewed after composition recovery, monotonic owner epochs, designated Edit-menu closing-frame protection and bounded Java publication ordering fixes. No findings remain. Exact final committed text `caf\nAndroid offline\ncafé 😀\nOK \n` (35 UTF-8 bytes) survived force-stop/relaunch, SHA-256 `f2efe33381886d4578cc977b363a5f65da5d08b17540c1cfa24a2e600f6fecbb`. See `android-emulator-evidence.md`; cross-platform IDs/actors/history/restart evidence is in `android-linux-sync-evidence.md`.

## Scope and limitations

Optional D-Bus and the external filter process runner were explicitly deferred; native contracts/example are delivered. Block editing/general plugin management and other plan-deferred features remain outside this MVP. Physical wireless LAN, ARM64 runtime and other Android devices/keyboards were not tested. A representative API35 emulator plus Linux owner satisfies the planned platform/two-device gate; it does not prove a broad hardware matrix. Android sync remains explicit and foreground-only. Wait for SQLite Saved before terminating; unsaved drafts/preedit are not durable acknowledgements.

The user accepted the SDK license and chose the emulator. Temporary SDK/NDK: `/tmp/tanglewood-android-sdk`; AVD `Tanglewood_API35`, serial `emulator-5580`. No external publication is authorized or performed. Only PM stages/commits, unsigned. No required implementation or acceptance task remains.

## Codex vault-project follow-up

CP1 protocol and CP2 durable metadata handoffs are integrated with CP3 sidebar/
main-pane UI. Header connection/new-thread icons, project rename, paginated
project-only threads/history and pushed status indicators are implemented.
Final combined host checks and isolated real project/reply/resume workflow are
recorded in `codex-live-evidence.md`; note source remained byte-exact. This is
an optional Linux follow-up; the required MVP acceptance scope is unchanged.

The optional Codex follow-up also includes composer profiles (Work/Plan/Research/
Goal), independent approval/network controls, a goal banner and bounded live
command cards. Research may create topic-folder notes through the existing MCP
write policy; source-file access stays separately restricted. Evidence and
remaining live-validation limits are in `codex-live-evidence.md`.

## Rich conversation follow-up task board

| Task | Owner / paths | State | Next evidence |
| --- | --- | --- | --- |
| RT1 native reply Markdown/code | renderer; ui/codex_markdown.rs, ui/code.rs, compact ui/document.rs helper | done | Bounded parser/cache and keyboard fixtures; exact native Copy |
| RT2 structured activity | context_protocol; codex.rs | done | Scoped lifecycle/bounds fixture passed |
| RT3 integration / reported plan display / docs | root; ui.rs, docs | done | Combined host, native layout and full-response/code clipboard |
| RT4 independent review | reviewer; read-only frozen delta | done | READY after Copy, activity termination and code whitespace fixes |

## Plan / approval correction task board

| Task | Owner / paths | State | Next evidence |
| --- | --- | --- | --- |
| MT1 truthful read-only MCP path | mcp_read_tools; mcp.rs | done | Forged-call regression + live two reads, zero approvals |
| CP1 structured plan setup / permission combinations | context_protocol; codex.rs | done | Four live plan updates; permission/capability fixture |
| UI1 stronger mode picker / documentation | root; ui.rs, docs | done | Combined host/build and native four-mode/access check |
| RV1 frozen delta review | reviewer | done | READY; command/MCP wording and capability fix reviewed |

## Research workspace follow-up (2026-10-27)

Approved design: `plans/2026-10-27-research-workspaces-design.md`. Initial scope
is explicitly added sources; discovery, OCR and rich output exporters are later
increments. Developer settings follow the user's Sol 6.1 medium request;
independent reviews use Sol 6.1 xhigh.

| Task | Owner / paths | State | Next evidence |
| --- | --- | --- | --- |
| R1 workspace commands / durability | research_core; research.rs, app.rs, commands.rs, store.rs, lib.rs | done | Real SQLite/CAS/restart/save-before-link and failure retention |
| R2 bounded text/PDF worker | research_import; research_import.rs | done | Page/bounds/text fixtures and actual native PDF import |
| R3 quiet workspace/source/agent UI | research_ui; ui.rs, ui/research.rs | done | Actual Linux workspace/note/citation flow; guarded exact evidence attachment |
| R4 integration / independent review | root / research_review | done | Independent READY; exact commands/limits in research.md |

## Research flow and artifact follow-up (2026-10-27)

Plan: `plans/2026-10-27-research-flow-and-artifacts.md`. User prioritized capturing
ongoing investigation flow, many associated threads, retained files/web Markdown,
and new report notes on demand. Parallel Sol 6.1 medium implementation; xhigh review.

| Task | Owner / files | State | Evidence / next |
| --- | --- | --- | --- |
| F1 flow records / dispatch | flow_core; research.rs, app.rs, commands.rs, lib.rs | done | Old JSON/restart/CAS/receipt retry; bounded context + index/paged reads |
| F2 retained artifact worker | flow_assets; research_assets.rs | done | Exact originals, real native HTTP/Pandoc archive and restart |
| F3 conversation/workspace flow | flow_ui; ui.rs, ui/research.rs | done | Native retained artifacts/two report versions + focused draft/provenance tests |
| F4 integration / review | root / flow_review | done | Independent READY; test/build/native evidence in research.md |

## Quiet research UI (2026-10-27)

Scope: presentation/navigation only, following the conversation-centered direction.
No source discovery, citation pipeline, retrieval or new output generators added.
Plan and capability gaps: `plans/2026-10-27-quiet-research-ui.md`.

| Task | Owner / paths | State | Evidence / next |
| --- | --- | --- | --- |
| Q1 design review and gap record | root / ux_design_review; plan/docs | done | Conversation/sources/outcomes direction; explicit deferred gaps |
| Q2 source/outcome panes and contextual windows | quiet_research_ui; ui/research.rs | done | Quiet source list, readable findings, guarded report preparation |
| Q3 existing chat integration and responsive shell | root; ui.rs, examples/research_demo.rs | done | Wide/empty/narrow native layout; successful preparation and draft refusal |
| Q4 independent review and acceptance | ux_design_review / root | done | Independent READY; 26 UI + 5 no-Codex GUI tests; native evidence in research.md |

## Vault agent permission (2026-10-27)

Plan: `plans/2026-10-27-vault-agent-permissions.md`. Replace the launch-flag-only
experience with a saved local permission; preserve headless compatibility.

| Task | Owner / paths | State | Evidence |
| --- | --- | --- | --- |
| P1 durable local-only preference | vault_permission_state; app.rs, store.rs, commands.rs | done | Grant/revoke, SQLite failure/retry, restart and unauthorized caller regression |
| P2 live policy and cached tools | live_mcp_permission; mcp.rs, codex.rs | done | Same-socket queued revocation, accepted acknowledgments, stable schema, no self-grant |
| P3 controls and integration | root; ui.rs, docs | done | Native Settings grant and conversation-menu revoke; build passed |
| P4 independent review | ux_design_review | done | READY; combined library suite 79 passed, 1 optional external-tool case ignored |

## Desktop research completion (2026-10-27)

Plan: `plans/2026-10-27-desktop-research-completion.md`. Sync/mobile and multi-agent
research orchestration explicitly deferred.

| Slice | State | Evidence |
| --- | --- | --- |
| References, discovery, native import | done | Explicit candidate collection/dedup; native chooser retained Markdown; live Crossref passed |
| Acquisition, reading, OCR | done | Durable batch/retry/restart tests; actual PDF and OCR; native PDF/text/search/citation navigation |
| Retrieval and assessments | done | Selected-source lexical preview bound to workspace/revision/query; contextual history and gap suggestions |
| Output roles and generators | done | Immutable real HTML/SVG outputs with source links; legacy Unclassified role; browser/SVG renders |
| Integration and UX review | done | Final focused gui/headless tests and build passed; Sol6.1 xhigh review; native evidence/limits in research.md |


## Desktop UX priorities (2026-10-27)

Scope: UX0–UX5 and UX9 only; UX6–UX8 remain later follow-ups. Shared checkout;
existing dirty work preserved; the completed, independently reviewed priority pass
is included in the local commit requested by the user. No publication.

| Task | Owner / paths | State | Evidence / next |
| --- | --- | --- | --- |
| UX0 baseline | ux_pm; evidence/manifest | done | fe40a2a archive; comparable stripped baseline43,528,848 bytes; native clipping classified |
| UX1 bridge/semantics | ux_pm manifest; ux_developer UI files | done | Native AT-SPI/Orca labels/states; raw whole-source Click passes; native header Click gap remains qualified |
| UX2 pane reachability | inherited reviewed shell | done | fe40a2a; design-system/adoption.md; native600×700 top message fully visible, scroll clipping intentional |
| UX3 source identity/states | ux_developer; ui/research.rs, theme.rs | done | Full identity/state cues, single hover/focus tooltip, measured contrast |
| UX4 editing/copy | ux_developer; ui/editor.rs, ui.rs | done | Clipped active marker; short/long editor regression; truthful apply/export copy |
| UX5 recovery wording | ux_developer; ui/research.rs | done | Effects inventoried; latest context review retains input, does not retry/save |
| UX9 review/evidence | ux_pm + independent reviewer | done | Independent review;34 UI +1 raw Click regression; native evidence/limits and accepted3.294MiB release growth |

Evidence: [desktop UX record](design/2026-10-27-desktop-ux-accessibility-evidence.md).
Native whole-source AT-SPI Click remains a specific adapter/render limitation;
keyboard Enter exposes the whole-source editor and the raw egui handler preserves
exact source. Android/device and full screen-reader certification are unverified.
