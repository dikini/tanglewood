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

## Chat hierarchy follow-up (2026-10-27)

Approved quiet presentation and bounded error handling: `plans/2026-10-27-chat-hierarchy.md`. Root owns integration and the shared index; the existing MVP scope remains complete.

| Task | Owner / paths | State | Acceptance evidence |
| --- | --- | --- | --- |
| CH1 message/event/artifact presentation | developer; ui.rs, ui/codex_context.rs | done | Native wide/narrow hierarchy, explicit failures, bounded cards and real Preview |
| CH2 structured error correction | root; codex.rs | done | Morning session causes and uncertainty reproduced; 16 parser/protocol tests pass |
| CH3 integration / native evidence | root; docs/captures | done | 35 UI tests, build, scoped Clippy, formatting/diff checks; native Markdown/PNG Preview and View all |
| CH4 independent review | reviewer; read-only | done | READY for frozen combined source and native evidence; no material findings |
| CH5 topical icons | developer; root assets/docs | done | 14pt magnifier/wrench/terminal/document icons, native wide/narrow/disclosure/card checks, independent READY |

Evidence and limits are recorded in the plan. Notes styling is committed at
`f850060`; this chat follow-up is included in the Git commit containing this
record. No new dependencies, bundled fonts or production polling loops were
introduced.

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

## Kimi ACP desktop integration (2026-10-27)

User-requested roles: Sol 6.1 xhigh planner; Sol 6.1 xhigh reviewer; GPT-6.1 Terra
max developer. Reviewer is reused for all fixes, with at most four implementation
review/develop cycles. Worktree: codex/kimi-acp from main 3a20248. The user subsequently authorized local commit and merge. No remote publication
is included.

| Task | Owner | State | Evidence / next |
| --- | --- | --- | --- |
| KA1 bounded ACP plan/contracts | kimi_plan (Sol 6.1 xhigh) | done | docs/plans/2026-10-27-kimi-acp.md frozen |
| KA2 independent plan review | kimi_review (Sol 6.1 xhigh) | done | READY; no blocking plan findings |
| KA3 adapter and shared GUI | kimi_develop (GPT-6.1 Terra max) | done | Final combined bins build; Kimi-only check; focused ACP/provider/MCP regressions pass |
| KA4 review/develop loop | same developer + reviewer | READY, cycle 3/4 | Seven concrete findings resolved; same Sol 6.1 xhigh reviewer approved frozen output |
| KA5 native/live integration evidence | root | done with limits | Kimi2.2 stream/MCP durable edit/restart/cancel pass; native model/provider/draft reconnect/compact overlay/editor pass; live permissions/rejection not observed; docs/kimi-live-evidence.md |


## Unified model selection and provider connections (2026-10-27)

User-approved taste critique: one composer model picker, provider grouping,
model-specific reasoning, automatic preparation and centralized connections.
Codex/Kimi own execution; Tanglewood is a client owning vault resources and
partial observed conversation records. Shell visibility only selects a projection;
hidden conversations continue observation, and disconnected/unknown is not idle.
The user additionally requires ECS-backed conversation state alongside notes,
using the existing App World; UI domain-state ownership is a bug.
Model roles explicitly requested: Sol 6.1 xhigh designer and independent reviewer;
GPT-6.1 Terra max developer. Isolated branch `codex/model-picker`, from main
`b495042`. The user subsequently authorized a local commit, squash merge onto
main and worktree/branch cleanup. No remote publication is included.

| Task | Owner | State | Evidence / next |
| --- | --- | --- | --- |
| UM1 UX/state contracts and bounded plan | model_flow_design (Sol 6.1 xhigh) | done | ECS amendment independently READY; prior reviewed UX unchanged |
| UM2 independent design/code review | model_flow_review (Sol 6.1 xhigh) | done | Final READY round 4/4; catalog, recovery/coordination/UX and ordered replay findings resolved |
| UM3 implementation and focused validation | model_flow_develop (GPT-6.1 Terra max) | done | ECS records and App-owned clients; bounded background polling; focused regressions and feature builds pass |
| UM3a pure model/Connections widgets | model_picker_widgets (GPT-6.1 Terra max) | done | Pure renderer frozen, rustfmt passed; ownership handed to primary for integration, no standalone Cargo |
| UM4 native acceptance and evidence | root | done with limits | Native fixture concurrent/targeted/reconnect/compact pass; real installed catalogs/search and normal close pass; docs/model-picker-evidence.md |

## Wasmtime extension goal (2026-10-28)

Delivered on `codex/wasmtime-wit-extensions`; this record is included in the
unsigned implementation commit. See [guide](wasm-extensions.md),
[plan](plans/2026-10-28-wasm-extensions.md) and
[acceptance evidence](wasm-extensions-evidence.md).

| Task | Owner / owned paths | State | Evidence / next action |
| --- | --- | --- | --- |
| W1 runtime/contracts | Terra 6.1 max; wit/, src/wasm_extensions.rs, runtime tests | done | Real compiled/Rhai calls, capability/resource/cancellation/isolation/recovery scenario; count-limit regression passed |
| W2 guest examples/docs | Terra 6.1 max; examples/wasm-extensions/, build script, guide | done | Four locked guests build; guest formatting/strict Clippy and import inspection passed |
| W3 integration | PM; Cargo.*, lib/main, CLI test, status/capabilities | done | Actual note imported/read through existing commands; both shipped Rhai scripts invoked without rebuilding |
| W4 independent review | Sol 6.1 max; frozen combined changes | done | Count-error classification finding fixed and regression verified; final lint delta reviewed; no remaining material findings |
| W5 acceptance | PM + runtime owner; integration evidence | done | Baseline serial 44 passed/1 ignored; 3 runtime/CLI scenarios passed; host/guest Clippy and scoped formatting passed |

Limits: existing repository-wide formatting drift and an intermittent parallel
baseline vault-lock failure are recorded in the evidence. Debug cold Rhai
compilation takes about 50 seconds here and is outside the execution deadline.
No Android or GUI-runtime claim; GUI controls, document-filter wiring and other
interpreters remain outside this goal.
