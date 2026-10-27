# Linux Codex app-server adapter

The optional `codex` feature includes MCP. The adapter supports installed
**codex-cli 0.163.0** only, checked by `codex --version` before starting the child.
The protocol reference is the CLI-generated schema recorded in
[codex-protocol.md](codex-protocol.md), plus the
[official app-server documentation](https://learn.chatgpt.com/docs/app-server).
A different CLI version fails visibly instead of silently changing protocol.

The worker owns a single `codex app-server` child and its stdio JSONL pipes.
It waits for the initialize response, sends initialized, then starts or resumes
the vault's project. Thread creation/resume is explicit, and the adapter waits
for the thread response before accepting a prompt. Opening a thread does not submit a turn. An explicitly activated persisted
goal may allow the server to continue work; pause or clear it to stop goal pursuit. The model is the user's configured CLI default; the app
does not silently choose a model or change global settings.

## Owner and UI integration

`TanglewoodUi::with_codex(config)` attaches the lazy panel configuration;
`CodexAdapter::spawn(CodexConfig, wake)` starts a dedicated worker after Connect. Configuration
supplies the executable, an absolute existing working directory, the absolute
`tanglewood-mcp` executable, the running App's socket, and an optional resume
thread ID. Use a separate working directory, not the writable note vault. The
App keeps exclusive ECS/vault ownership; the adapter holds no App handle.

The UI sends bounded `CodexRequest`s with `try_send` and drains `CodexEvent`s via
`try_recv`. A wake callback requests a repaint when events or disconnection arrive.
The Codex activity icon replaces the Notes sidebar with the vault's project and
its paginated thread list. Connect/disconnect and new-thread icon buttons sit in
the sidebar header, with tooltips and normal keyboard focus. The project menu
provides rename and refresh. Selecting a thread loads bounded conversation
history into the main area without starting a model turn. Notes selection,
source drafts and reading position survive switching back.

The vault stores its project ID, display name and a stable creation idempotency
key in the existing SQLite owner. Creation intent is saved before connecting;
returned identity and renames receive durable acknowledgements. A missing
server project reports an error rather than silently creating a replacement.
The binding is vault metadata; conversation history remains with Codex. It is
not a cross-machine transcript synchronization feature.

Each thread has a right-click menu and keyboard-focusable ellipsis button:
Rename opens a title field; Archive hides the conversation from the active list;
Remove permanently deletes it. Confirmation explains that archive/delete also
affect spawned child threads. These use the real Codex APIs and update the UI
only after acknowledgement. Server errors retain the conversation. An archived
browser/restore action is outside this small follow-up; restore through Codex.

Archive/remove are disabled while local conversation drafts or known active
conversations could be affected. If another client removes a conversation,
nonempty local prompts remain in visibly marked recovery rows, including drafts
whose thread was outside the current page. Select a recovery row to copy/clear
its source text; sending stays disabled. Removal retires active turn/approval
state rather than leaving the UI busy. Ordinary rows remain capped at 256;
recovery rows are additionally bounded by the 256-entry draft cache.

The rounded composer has a compact model/reasoning popup along its bottom edge.
It reads the current CLI's paginated `model/list` catalog (50/page, 128 total),
shows model descriptions and only that model's advertised effort choices, with
a discrete slider and default-effort reset. Choosing another model resets effort
to a supported default. Start/resume shows the thread's effective model/effort.
Choices are sent with the next explicit `turn/start` and then follow Codex's
thread settings; nothing changes global CLI configuration or runs a model turn
merely by opening/selecting a menu. Reconnect reloads the catalog. Until the user
chooses an override, the adapter preserves CLI/thread defaults.

Thread rows show small shape/colour indicators: blue ring/dot for working,
amber filled/outlined diamonds for approval/input, a hollow circle for idle,
a red cross for error, a dash for unloaded, and a question mark for unknown.
Tooltips and accessibility labels explain the state. On connection or explicit
refresh, `thread/list` supplies a snapshot; `thread/status/changed` then updates
known threads over the existing stdio JSONL stream. No periodic polling or SSE
connection is used. Loaded/subscribed threads can push runtime changes;
unloaded/disconnected threads are never assumed idle.

Send is disabled until Ready and while a turn is active. The panel offers
Interrupt, explicit one-action approvals, disconnect and reconnect/resume.
Clearing displayed messages does not erase server context. The display and
cached prompts share a 2 MiB budget, with at most 256 messages/threads in the UI.
Thread-list pages contain at most 50 entries; history pages contain 20 turns.
An unsent draft belongs to its conversation until a successful switch. A new
thread with an unsent prompt must be sent or cleared before selecting another
thread, because Codex does not persist empty threads for later recovery.

Start/resume responses must confirm the requested read-only sandbox, network
restriction, on-request approval policy, user reviewer and canonical working
directory before Ready. Only IDs returned for the vault's project may resume;
the returned project association is also checked. Foreign status notifications
are ignored. Worker request/event queues, known thread IDs, pending approvals,
transcript history and outgoing data remain bounded.

## Build and launch

```sh
cargo build --bins --example codex_smoke --features gui,codex
install -d -m 700 "$XDG_RUNTIME_DIR/tanglewood"
target/debug/tanglewood --vault /path/to/vault \
  --mcp-socket "$XDG_RUNTIME_DIR/tanglewood/app.sock" --codex
```

The default bridge policy is read-only. Enable **Allow agents to update notes and research**
in Settings → Connections or Conversation actions → Vault updates. This saved
vault permission changes the existing connection without restarting. `--mcp-write`
is a fallback only when no vault preference has been saved. The process sandbox
remains independent. Optional `--codex-cwd`
selects an existing working directory; the default is an empty `codex-context`
subdirectory created below the vault directory. `--codex-resume THREAD_ID`
selects an existing thread within the vault's project, then requires the same
confirmed policy on resume.
The MCP bridge executable is resolved next to the running Tanglewood binary.
Open the Codex pane, use its header link icon to connect, choose or create a
thread, then send a prompt explicitly. The optional
`codex_smoke --live` example is a PM-owned live acceptance harness and can invoke
paid model turns; building it does not run it.

## Permissions, approvals and authentication

The child receives an MCP configuration override for the `tanglewood` stdio
bridge pointing at the same owner socket. It neither writes global Codex config
nor opens a second vault. The bridge owner still decides read-only versus
read-write automation policy; Codex's process sandbox does not grant note-tool
mutations. Conversely, an MCP command allowlist does not authorize arbitrary
process/filesystem operations. The adapter instructs the thread to use MCP for
note access, with a read-only process sandbox and explicit per-turn network permission.

Command-execution, file-change and the narrowly supported Tanglewood MCP tool
confirmation requests are held pending
with their opaque server request ID, thread/turn IDs and full bounded details.
In Ask mode, a user may accept one action, decline it or cancel,
intersected with available decisions when supplied. The adapter does not offer
session-wide approval or policy amendments. Answers for stale/resolved requests
fail locally. Pending approvals clear when the server emits `serverRequest/resolved` for the
matching thread/request ID, when their turn terminates, or when the connection
ends. Resolution notifications have no turn ID and are handled before turn
filtering; a later answer to the resolved request is rejected. Unknown requests (including unsupported permission, dynamic-tool,
authentication-refresh, arbitrary form/URL elicitation and user-input methods) receive an explicit
protocol error, never an implicit grant. Unsupported interactions can fail a
turn; the editor remains usable.

The child uses the installed CLI's existing authentication. No credential files
are read, copied or logged. The inventory `codex login status` reported
“Logged in using ChatGPT” on 2026-10-26; this confirms only local login status,
not model entitlement, network access, or successful generation. No paid turn
was run during adapter development. The PM owns the scoped live smoke.

The live CLI also requests a Tanglewood tool call through
`mcpServer/elicitation/request`. The adapter recognizes only an active-turn form
for server `tanglewood`, `_meta.codex_approval_kind = mcp_tool_call`, the exact
empty object schema, and an object of tool parameters in `_meta.tool_params`
(including the empty parameters of `capabilities`).
It presents the full request details with the same explicit approval controls.
Accept replies with `{"action":"accept","content":{}}`; decline/cancel reply
only with their action. The adapter returns no persistence/session metadata,
even when offered by the server. Nonempty forms, URL requests and other servers remain unsupported. The owner
independently rejects local-only filesystem commands; the approval adapter does
not duplicate that domain allowlist. This confirmation does
not bypass the note owner's independent MCP policy or revision checks.

## Cancellation, limits and failure

Interrupt sends turn/interrupt for the active thread/turn. If cancellation arrives
before the turn-start response, it is retained until the turn ID is known.
An interrupt RPC acknowledgement is not a terminal event; the UI waits for
turn/completed with its actual status. Disconnect queues worker teardown, which kills and reaps the owned child
process group (including inherited MCP bridge children) without affecting the
separate note owner. The panel retains the adapter until teardown reports
Disconnected, then releases it; normal window close waits through CancelClose
while note autosave and rendering continue. The fallback adapter destructor
signals stop; nonblocking version-probe reads honor stop/deadline even if a
descendant retains stdout after the probe parent exits. A request may already have caused an MCP mutation when
cancellation/disconnect occurs; re-read state and obey the bridge's unknown
outcome rules rather than blindly retrying.

Limits: 16 queued UI requests, 16 pending RPCs, 16 pending approvals, 128 queued
events with a 4 MiB byte budget, 256 KiB prompts, 1 MiB JSONL lines, and a
2 MiB/32-message outgoing queue. I/O is nonblocking and work per worker tick is
bounded. Setup/RPCs time out after 30 seconds, turns after 10 minutes; version
checks have a five-second limit. A stalled or overflowing UI channel terminates
the child and records a separate visible disconnect fault, so a dropped approval
or terminal event cannot silently appear successful. Stderr is boundedly drained
to prevent child blockage and is not copied into logs/transcripts.

## Validation scope

The single focused protocol fixture checks initialize ordering, thread readiness,
read-only turn parameters, streamed correlation/stale notifications, explicit
approval/decision restrictions, unsupported-request errors, interrupt/terminal
state, stale approval rejection, timeout and a real short-lived fake-child EOF.
It does not authenticate, make a model call, or clone the full Codex protocol.

The required live gate remains: connect the installed CLI; stream a scoped turn;
show note-tool access using the single-writer bridge and revision behavior;
exercise cancellation and approval handling; disconnect and verify editor/save
usability. Record actual runtime/version/network failures and unfinished work in
the capability/status record instead of claiming readiness from a fixture.

Recorded focused result: `RUSTC_WRAPPER= cargo test --lib --features codex
--offline codex::tests::protocol_orders_initialization_correlates_turns_and_requires_explicit_approval`
passed after the final review fixes (1 test, 9 filtered; 4.24 seconds build).
The fixture also verifies that a resolved approval cannot be answered,
cancellation returns promptly when a fake version-probe descendant holds stdout
open, and an unavailable working directory reports an asynchronous failure. This compiles the adapter and
runs the bounded fixture only; it does not prove network/model or live approval
behavior. Owned Rust source was formatted and the owned diff passed whitespace
checks.

The integrated panel/startup/bridge binaries and live harness compiled with
`RUSTC_WRAPPER= cargo build --bins --example codex_smoke --features gui,codex
--offline` (10.62 seconds). The harness was built without running `--live`.
This is host compilation and fixture evidence; live generation, approval
interaction and visual panel evidence are still separate acceptance steps.

A captured live request exposed the MCP elicitation approval shape; the bounded
adapter fixture was extended to check explicit accept/decline/cancel response
shapes, no automatic approval, resolution/stale-answer handling, and rejection
of nonempty forms and URL requests. Runtime verification of this correction
remains the PM's scoped smoke; no new live turn was run by the adapter developer.

Correction validation: the extended focused adapter fixture passed (1 test,
9 filtered; 3.72 seconds build), including an explicit capabilities-tool empty
form as well as captured command-tool forms. The refreshed binaries and live
harness built with the same `--bins --example codex_smoke --features gui,codex
--offline` command (8.67 seconds). The adapter/UI never auto-accept these requests;
the PM owns the scoped live retry and its acceptance evidence.

## Context conveniences

The header context icon opens a resizable right pane with Goal, Plan, Files and
Plugins tabs. The composer + menu attaches the current note, another vault note,
or a UTF-8 text file; picks an enabled skill; sets a goal; or drafts a planning
or sketch request for review before Send. A sketch request is a prompt for an
available visual capability, not an editable drawing canvas or a guaranteed
image-generation service.

Note attachments capture exact source plus revision at selection time. External
text files load on a bounded worker. Eight snapshots and eight skill selections
share the 256 KiB prompt budget; selected skills must still be enabled in the
current catalog. Attachments and skills follow their conversation's in-memory
draft and remain recoverable through rejected/unknown submissions. They clear
only after a correlated turn-start acknowledgement. They are not durable draft
storage or a generic PDF/image-upload facility.

Goals use thread/goal get/set/pause/clear with explicit user provenance; only
confirming the goal dialog changes state. Saving a goal does not itself send a
model turn or implement autonomous wakeups: Send starts work. Goal responses and
failures are scoped to thread and superseded requests cannot overwrite newer
state. Plans and files are derived from live items and loaded history. Plan and
sketch convenience actions prepare editable prompt text rather than sending it.

Configured capabilities use skills/list plus paged mcpServerStatus/list (one
server/page, toolsAndAuthOnly, maximum128 pages/rows), avoiding the under-development
plugin/list API. This shows available skills and configured MCP connections; it
is not plugin installation, account linking, or a marketplace browser. Skills
are capped at128 entries. Discovery is requested at readiness/explicit refresh,
not periodically polled. Optional metadata failures remain visible without
terminating the chat; the normal transport frame budget still applies.

Files/results open only on explicit selection. Native PNG preview reads at most
8 MiB and decodes at most four million pixels on a worker, retaining one owned
texture. UTF-8 text/SVG source preview is capped at256 KiB; SVG is shown as source
rather than passed to an external-resource-capable renderer. Other raster formats
remain unsupported in this pass. A generation binds file jobs to their original
conversation; external removal preserves a pending input's recovery ownership.

## Composer profiles and command execution

The named **Context** button beside the title opens the right panel. The composer
provides Work, Plan, Research and Goal profiles beside the model/reasoning picker,
an approval reviewer selector, and Access settings. Changing profile/access controls sends no client turn/start; setting or
resuming an active goal uses the server goal API and may schedule work. Work/Plan/Research selection pauses an active saved goal before Send;
Goal selection sets or resumes it. A failed pause blocks inconsistent sending.
The goal banner above the composer remains visible when Context is closed, with
server-reported elapsed time, pause/resume, clear and details controls.

Plan uses the real app-server Plan collaboration preset. Research uses default
collaboration with research instructions so it can save findings as notes:
network enabled by default, attached snapshots as context, cited sources, and
normal CreateNote commands under Research/<topic>/ (or the requested destination).
Summary.md and Sources.md are suggestions, not mandatory empty boilerplate.
Research note creation requires the vault permission **Allow agents to update notes and research**
(or the legacy **--mcp-write** fallback when no preference exists); neither
the profile nor shell/network permissions grant MCP mutations. Existing notes
are preserved unless changes are requested. No configurable profile registry is
introduced in this pass.

Access permits explicitly enabling network access for subsequent messages in the
selected conversation. Opening another conversation resets access and approvals;
Work/Plan default to network off, Research to on. The approval picker offers:

| Mode | Approval policy / reviewer | Command sandbox |
| --- | --- | --- |
| Ask approval | on-request / user | Read-only, explicit network preference |
| Approve for me | on-request / auto_review | Same limits; reviewer decides requests |
| No prompts | never / user | Same limits; blocked actions can fail |
| Full access | never / user | dangerFullAccess; unrestricted files/network |

Full access requires explicit selection and includes network regardless of the
stored network preference. Leaving it restores that preference. These controls
configure command approvals; MCP tools have a separate approval policy. The MCP
bridge now exposes read-only `capabilities` and `read` tools; instructions use
`read` for queries and `execute` for mutations. Read annotations are truthful even
with a writable owner, and forged mutations through `read` are rejected before
forwarding. Mutations still require the owner's write permission and shared
validation. MCP policy can still request approval; Full access does not turn
that policy off. Managed policy may reject requested modes; errors remain visible
without a silent fallback or global configuration changes.

The documented MCP policy values are `auto`, `prompt`, `writes`, and `approve`,
with server-wide `default_tools_approval_mode` and per-tool `approval_mode`.
`writes` uses the tool's read-only annotation. This client does not change those
settings per turn. See [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference),
[MCP permissions](https://learn.chatgpt.com/docs/extend/mcp), and
[automatic review](https://learn.chatgpt.com/docs/sandboxing/auto-review).

The child server explicitly enables `tools.update_plan.enabled`. In Work mode,
structured plan updates replace the current plan view instead of producing new
progress lists in assistant messages. Plan mode is the separate proposal workflow;
its preset does not permit the checklist tool. Existing assistant messages remain
unchanged. Restart/reconnect after rebuilding to load the new tool configuration
and MCP catalog; use Work and ask for the actual plan tool on a multi-step task.

Live command items appear as compact expandable cards with the command, working
directory, status, exit code and output. Output is capped at64KiB per item; at
most32 items are tracked per turn, within existing conversation/event budgets.
Unrelated thread/turn notifications are ignored. Historical command cards are
not reconstructed in this first pass; loaded history remains user/assistant text.

Context snapshots are consumed only after the client turn/start RPC confirms
acceptance. A server-started goal turn cannot consume a different queued draft;
if the client request is rejected, that goal turn stays tracked and interruptible.

## Native conversation presentation

Assistant text is rendered as native Markdown: headings, emphasis, inline code,
ordinary/nested lists, read-only checklist markers, quotes, tables, safe HTTP(S)
links, and highlighted code cards with exact Copy. It uses the existing prose
and syntax styling, not an embedded browser. User messages keep their exact
text in restrained bubbles. Completed messages and loaded text history use the
same renderer; the cache invalidates when streamed text changes. Scrolling up
keeps the reading position; following the bottom continues with new output.

MCP calls, searches and file changes appear as compact expandable activity rows;
command cards keep bounded output. These structured activity rows are live-only
for now. Final response text is still stored intact, so display changes do not
rewrite agent output or source notes. Images, arbitrary HTML, scripts, filters
and automatic file/network fetching are not part of message rendering. Safe
external links are opened only by an explicit click through the link control.

Markdown checkboxes in a response are read-only presentation. The collapsible
Plan shows Codex-reported pending/in-progress/completed steps. **There are no
vault task records or editable task manager in this pass.** A project remains a
vault, and development remains Work mode. Existing explicit note snapshots can
carry a saved Research report to another Work thread; this update does not add
a repository manager or automatic project memory.

For a deterministic local preview with no vault or agent process:

```sh
RUSTC_WRAPPER= cargo run --locked --offline --example transcript_demo --features gui,codex
```

The example is available in debug builds on Linux. It uses the real conversation
widgets with clearly identified fixture content. It performs no agent work.
