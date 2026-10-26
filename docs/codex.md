# Linux Codex app-server adapter

The optional `codex` feature includes MCP. The adapter supports installed
**codex-cli 0.163.0** only, checked by `codex --version` before starting the child.
The protocol reference is the CLI-generated schema recorded in
[codex-protocol.md](codex-protocol.md), plus the
[official app-server documentation](https://learn.chatgpt.com/docs/app-server).
A different CLI version fails visibly instead of silently changing protocol.

The worker owns a single `codex app-server` child and its stdio JSONL pipes.
It waits for the initialize response, sends initialized, then starts or resumes
a thread. It waits for the thread response before accepting a prompt. No turn
starts automatically. The model is the user's configured CLI default; the app
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
The conversation panel shows Connecting/Ready, a selectable resume thread ID,
streamed messages, terminal turn status, full scrollable approval details and
errors. It preserves editing/autosave while the worker fails or disconnects.
Connect is lazy; Send is disabled until Ready and while a turn is active. The
panel offers Interrupt, Disconnect, reconnect/resume, and a new thread after
disconnect. Clearing displayed messages does not erase server thread context.
The display is capped at 256 messages/2 MiB; exceeding it disconnects visibly.
A live turn and visual smoke must be recorded separately from compilation.

Requests support prompt, interrupt, a specific pending approval answer, and
Disconnect. Events identify thread/turn/item for deltas and completed messages.
Consumers should reconcile completed message text by item ID rather than append
it again after deltas. Notifications for other threads or inactive turns are
ignored. Start/resume responses must confirm the requested read-only sandbox,
network restriction, on-request approval policy, user reviewer and canonical
working directory before Ready is emitted. Resume requests exclude historical
turn hydration; the server retains context but the adapter does not load an
unbounded old transcript into the panel.

## Build and launch

```sh
cargo build --bins --example codex_smoke --features gui,codex
install -d -m 700 "$XDG_RUNTIME_DIR/tanglewood"
target/debug/tanglewood --vault /path/to/vault \
  --mcp-socket "$XDG_RUNTIME_DIR/tanglewood/app.sock" --codex
```

The default bridge policy is read-only. Add `--mcp-write` only when note mutations
are intended; the process sandbox remains read-only. Optional `--codex-cwd`
selects an existing working directory; the default is an empty `codex-context`
subdirectory created below the vault directory. `--codex-resume THREAD_ID`
selects an existing thread, then requires the same confirmed policy on resume.
The MCP bridge executable is resolved next to the running Tanglewood binary.
Open the Codex panel and Connect, then Send a prompt explicitly. The optional
`codex_smoke --live` example is a PM-owned live acceptance harness and can invoke
paid model turns; building it does not run it.

## Permissions, approvals and authentication

The child receives an MCP configuration override for the `tanglewood` stdio
bridge pointing at the same owner socket. It neither writes global Codex config
nor opens a second vault. The bridge owner still decides read-only versus
read-write automation policy; Codex's process sandbox does not grant note-tool
mutations. Conversely, an MCP command allowlist does not authorize arbitrary
process/filesystem operations. The adapter instructs the thread to use MCP for
note access, with a read-only, network-restricted process sandbox.

Command-execution, file-change and the narrowly supported Tanglewood MCP tool
confirmation requests are held pending
with their opaque server request ID, thread/turn IDs and full bounded details.
No approval is automatic. A user may accept one action, decline it or cancel,
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
