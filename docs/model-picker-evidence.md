# Model selection and conversation client evidence

## Scope and baseline

Work starts from main `b495042` in `codex/model-picker`. The user requested Sol
6.1 xhigh design, independent Sol 6.1 xhigh review and GPT-6.1 Terra max development.
The design is `plans/2026-10-27-unified-model-picker.md`.

Before changes, the combined `gui,codex,kimi` UI filter passed 38 tests using the
shared warm target and `RUSTC_WRAPPER=`. No broad domain suite was repeated.

The supplied native screenshot shows a composer model menu limited to the selected
provider, nested below another popup. Selecting another provider required a distant
header selector, connection and session setup. This is the before evidence; it is
not a browser render or a measured contrast audit.

## Governing responsibility

Tanglewood manages vault resources, local drafts, user-chosen retained material and
partial observed conversation records. Provider agents own execution and expose
only the state their protocols support. The shell projects observations and emits
explicit targeted commands. Navigation must not stop work or end observation.
Connectivity, observation freshness and last-observed remote activity are separate.
Conversation domain state uses ECS entities in the existing App World, as the user
clarified during implementation. Adapter handles are separate transport machinery;
there is no authoritative UI-owned copy of a conversation record.

Existing CLI flags define enabled providers. This change does not add credentials,
a provider registry, a scheduler, a daemon, comprehensive cross-app live observation
or a new conversation persistence schema. User-controlled memory is architectural
context, not an implicit implementation gate for automatic memory collection.

## Implementation and review

Design independently approved with focused validation and safe temporary-session
cleanup corrections. Final implementation is independently READY in round 4/4.
The checkpoints below preserve the findings and their subsequent resolution.

## Acceptance to record

- One flat searchable model picker shows both enabled providers without manual
  per-provider connection steps and adjacent server-valid reasoning.
- Metadata discovery sends no prompts and touches only its own temporary sessions.
- Independent same-provider conversations continue observation across chat, note
  and research navigation; Stop and approvals target only their named chat.
- Exact drafts and explicit attachments survive preparation, failures and safe
  cross-provider new-conversation transfer; old history and other drafts remain.
- Connections distinguishes catalog availability from live handles and exposes
  batch refresh/retry and explicit scoped disconnect consequences.
- Native regular/compact layout, keyboard focus/Escape and real installed metadata
  are checked separately from deterministic protocol/background fixtures.

No paid model prompt is authorized or required for this work. Existing live Kimi
save/read/cancel evidence is documented separately and will not be reused as proof
of the new navigation or discovery behavior.


## Development checkpoints (not final acceptance)

The primary Terra developer reports these focused checks passing:

- Restricted Codex catalog initialization (no project/thread/prompt).
- Catalog helper scenarios, 5 tests on the expanded local fixture: owned cleanup,
  cancellation, malformed configuration, uncertain creation diagnostics and
  bounded no-progress pagination.
- App owner scenarios: two same-provider clients continue without a view, stale
  generation rejected, targeted Stop/approval isolation; asynchronous attachment
  goes to its original chat; cross-provider new-record transfer preserves exact
  explicit context and other drafts with no prompt; Kimi settled model snapshot
  replaces available effort choices.
- Kimi-only GUI compilation and actual view recreation while Notes is rendered:
  output continues, draft retained and no navigation-triggered cancel.
- Relevant combined UI regression filter: 37 tests pass after retiring the old
  widget-owned ConversationPanel/Parked lifetime path and adapting owner coverage.

Catalog review round 1 identified unbounded fresh-cursor paging and lost unknown
creation cleanup outcomes. Fixes have developer test evidence; independent delta
approval and the frozen combined implementation review remain pending.

No native or installed-provider result for the new flow is claimed by these local
protocol tests. All provider work in the fixture is simulated without network.


## First frozen native pass and full review

The combined binary ran on isolated Xvfb101 with the local JSONL fixture and a
disposable vault, never a real provider prompt. Opening Conversations automatically
discovered both providers. Audit showed Codex initialize/initialized/model-list,
and Kimi initialize/new/close/delete for its temporary catalog session. The flat
picker exposed both groups immediately; choosing Kimi A prepared a Ready session
and adjacent High reasoning without provider/connect/session UI steps.

A simulated Kimi message streamed all chunks 0–19 while the user navigated to
Notes, then returned. The audit contained no navigation cancel. Central Connections
showed catalog availability separately from client counts; its `Active` count label
needs to say connected/client handles rather than imply ongoing agent work.

Explicit disconnect then reconnect reproduced the reviewer's bug: the client only
initialized/listed, never loaded its validated session, and remained Connected but
not Ready. At 390 logical points the picker and keyboard provider-name search fit,
but resizing with Connections open left its left side clipped. The compact route
selector also retained the misleading Codex label for the shared conversation area.
The regular sidebar header overlapped its utility buttons. These are concrete native
findings, not passing responsive acceptance. Model rows were unnecessarily centered
and completion copy exposed `end_turn`; both are bounded polish requests.

Normal WM_DELETE_WINDOW shutdown exited successfully and promptly, without the
X-window-destroy panic seen in earlier unrelated testing. Fixture files and captured
screenshots are under `/tmp/tanglewood-model-picker-native-6wtoglrw/`.

Full independent review round 2 accepted both catalog fixes and required six
corrections: session reconnect, shared project-rename propagation, Goal→Plan/Research
ordering, busy-chat disconnect consequences, hidden-error indicators and header
collision. Root additionally requires the observed compact-dialog clipping and
generic navigation label correction. The same primary Terra worker is fixing the
scoped findings; final approval and native rechecks remain pending.


## Final acceptance and limits

The same Sol 6.1 xhigh reviewer approved the frozen implementation in round 4/4.
Round 3 accepted all six recovery/coordination/UX fixes and the native layout
corrections. Its remaining replay finding was fixed and approved in round 4:
ordered repeated messages are preserved, real supplied identities are respected,
partial observations are reconciled one-to-one, unmatched cached material remains
explicitly uncertain, and absent/failed/oversized replay preserves the previous
history and draft. Automatic research capture excludes uncertain rows.

Focused final checks passed: replay regressions 4/4; reconnect 3/3; Kimi lifecycle
1/1; Codex project/history-no-turn 1/1; research-capture exclusion 1/1. Earlier
coherent integration checks passed: App owner 8/8, catalog 5/5, UI 39/39. Combined
GUI bins build and both Kimi-only and Codex-only GUI compilation passed. The root
rechecked Codex-only compilation after the final replay delta; it emits existing
and feature-specific warnings, so this is not a warning-free claim.

A second isolated native fixture pass showed two Kimi chats awaiting approval
while Notes/Research were visited. Allowing the second affected only that session;
Stop targeted the first. Disconnect displayed its named consequence confirmation.
Reconnect audited initialize/list/load of the same session, with exactly two total
fixture prompts and no repeated prompt. Cached text and the draft survived;
activity unknown was shown honestly. Connections reports two connected clients
separately from catalog availability. Header controls and left-aligned model rows
fit. At 390x780, Connections stays on screen, controls wrap and the composer remains
usable. These are local simulated-agent checks, not paid provider execution.
The second test process was no longer present when final acceptance resumed; no
normal-shutdown result is claimed for that run. The first run closed normally.

The final rebuilt binary was separately launched with the real installed Codex
and Kimi executables, a disposable vault/context and isolated Xvfb103. Opening
Conversations automatically populated both catalogs. Native screenshots show real
Codex models and a keyboard search for `kimi` showing K2.8 Preview, K2.7 Code
Highspeed, K3 and other configured models. Escape dismissed the picker and Notes
navigation remained usable. No foreground model was selected and no model prompt
was submitted. Normal WM_DELETE_WINDOW shutdown exited 0. This proves installed
metadata discovery and search, not live model turns or comprehensive external-chat
subscriptions. Arrow/Enter selection was not separately certified in this final
installed-provider pass. Earlier fixture native checks cover compact layouts.

Native captures remain in `/tmp/tanglewood-model-picker-native-6wtoglrw/`, notably
`final-models.png`, `final-two-approvals.png`, `final-targeted-approval.png`,
`final-reconnect-ready.png`, `final-compact-connections.png`,
`installed-models.png` and `installed-kimi-search.png`.

Conversation observations remain in memory for this slice; no new durable memory
schema, external-provider completeness, Android or mobile acceptance is claimed.
The user authorized the feature commit and subsequently a local squash merge
onto main, followed by worktree/branch cleanup. No remote publication is included.

Integration validation on main: combined-provider GUI regression group 39/39 and
ordered replay regressions 4/4 passed after applying the squash. Existing warnings
remain. The managed worktree is archived recoverably during final cleanup.
