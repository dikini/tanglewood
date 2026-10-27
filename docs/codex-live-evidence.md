# Codex live acceptance evidence

Checked 2026-10-26 with installed `codex-cli 0.163.0`, existing ChatGPT login,
and temporary vaults only. No authentication files were read or logged. This
proves Linux integration; it supplies no Android or LAN evidence.

## Live protocol and note tools

`target/debug/examples/codex_smoke --live` passed after the captured MCP approval
request was implemented. The opt-in harness:

- initialized a real app-server child and started a thread with confirmed read-only process sandbox and user approvals;
- observed streamed messages and three separate `mcpServer/elicitation/request` approvals for a temporary note's read/edit/read;
- accepted only the known note ID and exact current-revision byte edit (other requests decline), without granting session/always permissions;
- verified the resulting source `# Live\nafter\n` and persisted revision;
- interrupted a second real turn and received terminal `interrupted`;
- disconnected, then created and saved another note through the same App;
- reopened the vault and verified the tool edit survived restart.

Successful live thread: `01a1776c-40b0-7db2-8c11-135bb6105099`. Harness stdout
reported all four PASS boundaries. These are scoped harness approval responses,
not a claim that a human clicked the GUI approval controls.

Earlier diagnostic turns streamed successfully but rejected the MCP read. A
transparent protocol trace isolated the request shape: an empty-object form
elicitation with `_meta.codex_approval_kind=mcp_tool_call`. The old adapter
returned unsupported-method; it was corrected to display explicit one-action
approvals and serialize MCP `action` responses. Focused fixture covers these
responses, resolved/stale requests, capabilities, and unsupported URL/nonempty
forms. Failure diagnostics were retained in owned `/tmp/tanglewood-codex-live-*`
directories; the successful smoke removes its temporary vault.

## Actual panel visual workflow

Built `--features gui,codex` and launched the real editor on isolated Xvfb `:97`:
`--vault /tmp/tanglewood-codex-ui-01a1776c/vault --mcp-socket /tmp/tanglewood-codex-ui-01a1776c/owner.sock --codex`.
MCP remained read-only. No turn was sent from the panel.

Inspected the source/editor/conversation layout, clicked Connect, observed Ready
and a resume thread ID, typed source during connection and observed Saved,
clicked Disconnect, typed more source and observed Saved. A SQLite query
confirmed both edits. Screenshots inspected at `/tmp/tanglewood-codex-panel.png`,
`/tmp/tanglewood-codex-connected.png`, and `/tmp/tanglewood-codex-disconnected.png`.
Stopped the identified temporary editor/display after the check.

The live harness proves streaming/tools/cancel/approval wire behavior; the window
check proves layout, connection/disconnection and concurrent source autosave.
Protocol failures, unknown requests, deadline bounds, process cleanup and async
invalid-cwd reporting also have the focused fixture evidence in `codex.md`.

## Vault project/sidebar follow-up (2026-10-26)

Using CLI 0.163.0 and an isolated `/tmp/tanglewood-codex-project-ui-eYHt3A/vault`,
the real app-server created a project, renamed it to `Vault workflow smoke`, and
listed its threads. SQLite recorded the actual project ID plus a creation key;
a read-only query after restart confirmed both remained unchanged and the new
name was durable. No unrelated project was modified.

One scoped model prompt asked for exactly `Vault project workflow verified.`
without tools or modifications. That response streamed successfully. Creating
another empty conversation and selecting the persisted thread reloaded its
You/Codex history without another model turn. Empty Codex threads are not
materialized until their first prompt; the UI therefore starts them with an
empty transcript and protects unsent provisional drafts against switching away.
Switching Notes/Codex retained an unsubmitted note-block draft; Cancel restored
the original. A read-only SQLite comparison after restart matched the imported
`docs/fixtures/mvp-2/code-and-comments.md` source exactly.

After rebuilding the final icon/status UI, the sidebar header connection button
reconnected to the same saved project. Selecting the existing thread restored
history and its idle marker; its prior unloaded marker was distinct. The
header places connect/disconnect before new thread in left-to-right focus order.
Inspected native capture: [Codex project pane](design/codex-vault-project-linux.png).
Working/approval/input/error notification handling and foreign-thread filtering
are covered by the protocol fixture; this final visual run did not generate
additional model turns or live approval requests. Status is pushed over JSONL
for loaded/subscribed threads; there is no periodic refresh timer.

Final combined `RUSTC_WRAPPER= cargo test --locked --offline --lib --features gui,codex`:
34 passed, 1 optional external Pandoc fixture ignored. The Unix-socket fixture
ran outside the filesystem sandbox. `cargo build --locked --offline --bins
--example codex_smoke --features gui,codex` and `cargo clippy --locked --offline
--all-targets --features gui,codex` completed (warnings present). Metadata restart,
CAS/identity constraints, injected SQLite save failure, bounded protocol/history,
nonselected status routing and failed-switch prompt ownership have focused
regression evidence. This follow-up does not establish Android Codex support or
cross-machine project/history synchronization.

Final independent delta review: READY. Prompt ownership and known-thread status
routing were rechecked against the final frozen source. The final header plus
button also opened a fresh Ready conversation without generating a model turn.

## Thread actions and model picker (2026-10-26)

The thread row now supports both right-click and a focusable ellipsis menu.
Real CLI 0.163.0 checks used only the previously created disposable conversation
`01a179b6-6a02-7d00-9da4-a7c53bb10802` in the isolated vault project above:
renamed it to `Thread menu smoke`, archived it through confirmation, verified
that the active sidebar/selection cleared, restored it via scoped thread/unarchive
for the next check, then removed it through permanent-delete confirmation.
A separate read-only app-server query confirmed that exact ID was absent from
both active and archived project listings. No unrelated conversation was changed.

The same native session loaded the real model/list catalog, displayed the
thread's effective model/effort, selected another advertised model (resetting
effort to its supported default), and dragged the discrete effort slider from
Extra high to Medium. The popup remains open while interacting with the slider.
These menu changes sent no model turn; actual override wire fields and unsupported
model/effort rejection are covered by the protocol fixture. This follow-up made
no additional generation calls. Reconnect reloads the catalog, and the sidebar
width stays stable instead of growing to accommodate header spacing.

Screenshots: [thread menu](design/codex-thread-context-linux.png),
[model/effort catalog](design/codex-model-picker-linux.png).

Combined gui,codex library suite: 37 passed, 1 optional external Pandoc fixture
ignored. After review-driven recovery corrections, focused thread-action/UI
regressions passed (2 tests), including selected/cached/off-page prompt recovery,
external active-thread removal cleanup and acknowledgement ordering. Final
`cargo build --locked --offline --bins --example codex_smoke --features gui,codex`
completed with existing unused-function warnings; whitespace checks passed.
Independent review marked the recovery/contract delta READY. The final popup
close behavior was additionally verified by native slider interaction.

Archive/remove are deliberately unavailable while local prompts or known active
threads could be affected by descendant operations. External removal retains
nonempty prompts as recovery rows; copy/clear before leaving them. Archived
browsing/restoring remains a Codex operation, rather than new Tanglewood scope.

## Context conveniences follow-up (2026-10-26)

The isolated project/vault above was reused with fresh owner sockets. No new
model turns or goal mutations were run. The real app-server loaded an empty
thread, returned no current goal, and discovered skills and configured MCP
connections. The initial all-server/full inventory exceeded the1MiB transport
frame limit and visibly disconnected; the implementation was corrected to fetch
one server/page with toolsAndAuthOnly detail. The corrected native session stayed
Ready and displayed the actual inventory, including the Tanglewood-adjacent
configured servers. Composer + showed source, skill, goal, plan and sketch
controls. Attach current note added the exact Code and comments.md snapshot chip
without sending it. [Native context capture](design/codex-context-panel-linux.png).

Combined gui,codex library run before the final turn-rejection correlation delta:
42 passed,1 optional Pandoc fixture ignored (including the Unix socket scenario).
Updated combined run after that delta, excluding the already-passing unchanged
socket fixture:42 passed,1 ignored,1 filtered. The added typed turn-rejection
fixture passed along with stale goal/read/mutation/error correlation, bounded
inventory pagination, snapshot/skill allowlist and loader ownership/limits tests.
Final binaries and codex_smoke example were rebuilt. Existing unused-function
warnings remain. Independent review closed all context/preview findings.

Worker preview checks cover an actual small PNG texture, exact UTF-8 text,
oversized input rejection, SVG-as-source (no external href resolution), external
retirement of an in-flight snapshot and cancellation after generation changes.
Live image generation, live goal mutation and an additional paid turn were not
needed for this convenience/UI pass and are not claimed. Automatic goal looping,
full plugin management, generic binary attachments and a drawing canvas remain
outside its scope. No note source was changed by context inspection/attachment.

## Composer profiles, goal banner and execution follow-up (2026-10-26)

The isolated Linux test vault/project was reused on display :94. The final
build connected, opened an empty thread, showed the named Context button and
right panel, and offered Work/Plan/Research/Goal, approvals, Access and the actual
model/effort catalog. Selecting Research checked network access; closing Context
retained the profile. The composer sits near the bottom and wraps controls when
the panel narrows the conversation. Profile and model remain adjacent in the
wrapped footer. Screenshot: [Research controls](design/codex-composer-profiles-linux.png).

A temporary objective, "Verify composer layout only; do not start work.", was
explicitly set through the UI to inspect the real goal banner, then paused and
cleared. The service reported Working and later persisted one completed turn
containing assistant/reasoning, commandExecution and MCP item types. Thus absence
of a client turn/start did **not** establish metadata-only operation. The dialog
now explains that active goals can continue work, and the adapter accepts turns
scheduled for confirmed active goals with normal correlation, approvals and
bounds. No separate Research generation was requested. Read-only owner queries
confirmed one unchanged source note and no remaining goal. Only this disposable
completed test thread was deleted afterward; the project association remains.

Final combined host command:
`RUSTC_WRAPPER= cargo test --locked --offline --lib --features gui,codex --
--skip mcp::tests::owner_socket_routes_policy_revision_and_durable_mutations
--test-threads=1`: **48 passed, 1 ignored, 1 filtered**. The filtered socket scenario
passed at the previous checkpoint; it was not repeated for composer changes.
One prior parallel run hit an existing Store reopen Locked error; the serial
run passed that same test. Final bins/example build passed with existing unused
helper warnings. Whitespace checks passed. Independent state/protocol review
marked READY after goal mutation/refresh ordering, reconnect freshness, nullable
command-output retention and automatic-goal/client-submission races were fixed.
The final wrap-only adjustment was checked natively.

Actual Research note generation/network access, automatic approval decisions,
and live command cards in this final build were not exercised. Their wire/
lifecycle behavior has focused fixtures; this is not proof of service-side
permission acceptance under every managed configuration. Historical command
cards and arbitrary configurable profiles remain outside this pass.

## Rich native conversation follow-up (2026-10-26)

Implemented native Markdown response presentation and compact structured activity,
using the existing prose/highlighter primitives. A project remains a vault;
development remains Work mode. Response checklist markers and reported PlanStep
states are display-only. There is no vault task store, editable task manager or
repository-manager feature in this update.

Combined host check after state/copy corrections:
`RUSTC_WRAPPER= cargo test --locked --offline --lib --features gui,codex --
--skip mcp::tests::owner_socket_routes_policy_revision_and_durable_mutations
--test-threads=1`: **50 passed, 1 ignored, 1 filtered**. An expanded renderer
fixture first failed CRLF preservation; after correction it passed. A further
review fixture reproduced synthesized partial-tab whitespace loss before its
fix. Cache/reference-expansion bounds, nested code, source preservation, scoped
activity lifecycle and terminal retirement are covered. The final keyboard/
spacing delta passed the focused `transcript` filter: **2 passed** (renderer plus
linked-paragraph keyboard chooser). No optional Pandoc or already-proven owner
socket scenario was repeated for presentation-only changes.

`RUSTC_WRAPPER= cargo build --locked --offline --bins --example transcript_demo
--features gui,codex` passed, with existing unused helper warnings. Independent
review marked READY after response Copy, terminal activity repaint, synthesized
code whitespace and keyboard-link changes. Whitespace checks passed.

Ran `target/debug/examples/transcript_demo` on isolated display :94. This debug
example uses the actual conversation widgets with explicitly labeled local
fixture content and no vault, owner socket or agent runtime. Inspected headings,
inline code, lists/nesting, adaptive tables, quote, code card, checklist, inline
link, expandable activity and reported plan. The composer stays at the bottom;
scrolling up retained position without being pulled back to the latest output.
Removed the extra standalone link-menu row after native inspection. The final
link chooser is reached through focused paragraph Enter/Space or right-click;
keyboard fixture proves focus/chooser do not navigate until explicit activation.

On the isolated X11 clipboard, the real code Copy button matched the exact Bash
fence body including Unicode and final newline. The response Copy icon matched
all bytes of `fixtures/mvp-2/codex-response.md`. Screenshots:
[upper transcript](design/codex-rich-transcript-linux.png) and
[code/checklist/plan](design/codex-rich-transcript-code-linux.png).

No new paid model turn, live research generation or durable Research -> note ->
Work handoff was run for this presentation increment. Existing explicit source
attachments and note-write permission remain unchanged. Historical activity
cards, image rendering, math and arbitrary HTML/filters are outside this first
pass. Huge/pathological Markdown uses a readable literal fallback; retained
presentation cache is bounded independently from the conversation budget.


## Structured plan and approvals correction — 2026-10-27

The reported real thread contained numbered assistant progress lists but no typed
plan items. Pinned CLI source defaults tools.update_plan.enabled off. The adapter
now enables it for its child process and directs Work multi-step tasks to update
one structured plan. Plan collaboration remains the distinct proposal mode.
Read queries use a new honestly annotated MCP read tool; runtime validation
rejects mutation/local-only payloads before forwarding to the owner. Existing
execute remains available for mutations with independent owner permissions.

Combined host check:
`RUSTC_WRAPPER= cargo test --locked --offline --lib --features gui,codex --
--skip mcp::tests::owner_socket_routes_policy_revision_and_durable_mutations
--test-threads=1`: **53 passed, 1 ignored, 1 filtered**. This includes forged-read
rejection and actual process argument/turn-field assertions for all four modes.
A first live attempt exposed missing experimentalApi capability without project
binding: rejected before model execution; the cleanup API confirmed no rollout
existed. After correction, the focused
`child_plan_tool_and_explicit_approval_modes_have_real_fields` fixture passed
(**1 passed, 54 filtered**), and bins plus `codex_plan_smoke` rebuilt successfully.
Existing unused helper/test-field warnings remain.

Ran `target/debug/examples/codex_plan_smoke --live` against the authenticated
installed CLI with its configured model/effort in a private disposable vault.
The helper created one seed note, a read-write owner socket, and one test thread;
Work/No prompts/network false used real update_plan and the new MCP read tool.
Observed **4 typed plan updates**, final three steps completed, **2 successful
read calls**, **0 approval requests**, **0 numbered assistant progress messages**,
and an unchanged single source note. Final response matched the seed's last line.
The test disconnected; its exact remote thread was deleted successfully through
thread/delete. No user vault was modified. The helper retains a thread-id for
explicit scoped cleanup on future runs; it does not automatically delete threads.

Native inspection on isolated X11 :94 used the rebuilt actual app and owned test
vault. All four picker options were present. Explicit Full access displayed
unrestricted access and network included; returning to Ask restored the previous
unchecked network preference. No conversation message was sent in this UI test;
the connection was disconnected and test process stopped. Screenshot:
[approval picker](design/codex-approval-modes-linux.png).

Independent frozen-file review: READY after qualifying command vs MCP approval
copy and reviewing the capability fix/test harness. Whitespace check passed.
No live unrestricted command or network operation was needed to verify mapping;
those combinations are wire-fixture/native-UI evidence. MCP mutation approval
policy was not disabled or tested as approval-free. Research generation remains
outside this check. Restart/reconnect the rebuilt app to load the new child/tool
catalog; existing authored progress lists remain historical messages.
