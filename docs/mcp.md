# Linux MCP bridge

Build the optional `mcp` feature. The stdio executable uses the official rmcp
3.7.0 SDK for MCP initialization, discovery and tool calls. It does not implement
an alternate JSON-line protocol and never opens a vault. The running Tanglewood
App remains the sole ECS and vault owner.

## Launch and authorization

Use an existing private directory owned by your user (mode 0700):

```sh
install -d -m 700 "$XDG_RUNTIME_DIR/tanglewood"
cargo run --bin tanglewood --features gui,mcp -- \
  --vault /path/to/vault --mcp-socket "$XDG_RUNTIME_DIR/tanglewood/app.sock"
```

Configure an MCP client to launch the built bridge executable with arguments
`--socket /absolute/private/directory/app.sock`. Its stdout is exclusively MCP
protocol output. Example client configuration:

```json
{
  "mcpServers": {
    "tanglewood": {
      "command": "/absolute/path/to/target/debug/tanglewood-mcp",
      "args": ["--socket", "/run/user/1000/tanglewood/app.sock"]
    }
  }
}
```

Build both binaries with `cargo build --features gui,mcp` first; `cargo run` does
not build the bridge binary automatically. Substitute your actual user runtime
directory; do not copy user ID 1000 blindly. A headless owner uses `--headless`
with the same socket flag and continuously polls the same App dispatch.

The default owner policy is read-only. Mutations require the owner launch flag
`--mcp-write`, not a tool argument. Caller identity comes from the trusted owner
setting; the socket envelope rejects unknown fields. Both discovery and dispatch
use the shared command descriptors' automation allowlist, which excludes
local-only filesystem/mirror commands. Read-only exposes read/search; read-write
also exposes create, revision-checked edit/rename/trash/restore. The adapter does
not provide arbitrary filesystem, process, SQL or ECS access.

The socket is mode 0600 inside the private directory, and both endpoints verify
Linux SO_PEERCRED has the current effective UID. This is a same-user local
boundary, not a sandbox against malicious processes already running as that user.
Existing socket paths are refused. Orderly shutdown removes only the path with
the inode created by that owner. A killed owner can leave a stale socket: verify
no owner is running before manually removing it; the bridge never deletes it or
starts a second writable vault runtime.

## Tools and shared schema

`capabilities` returns protocol version, request ID and the running owner's
allowed command names and policy. `execute` accepts `{"command": ...}` using the
same serde Command representation as the application. Tool input schema is
produced by schemars from shared request types, then unauthorized variants are
removed using those same descriptors. A changed/unknown schema-generator layout
fails closed. A policy change between discovery and invocation does not bypass
owner enforcement.

Examples (UUIDs and revision tokens must come from actual read/create results):

```json
{"command":{"SearchNotes":{"query":"meeting","include_trash":false}}}
```

```json
{"command":{"EditNote":{"id":"NOTE_UUID","expected":"REVISION_UUID","edits":[{"start":0,"end":0,"replacement":"New text\n"}]}}}
```

Read/search results expose current accepted content and save status. A mutating
success is returned only after the exact accepted revision has a persisted
acknowledgement, and its returned NoteView reflects that acknowledgement for the requested snapshot,
even if the owner has already accepted a newer dirty edit. Save
errors, supersession before confirmation and deadlines produce `UnknownOutcome`;
never blindly retry such a mutation. Re-read the note and reconcile the expected
revision. Request IDs correlate replies; they are not durable idempotency keys.
Stale revisions and unauthorized operations return the shared typed error.

A missing owner fails explicitly through the MCP tool error result. If no owner
is available during discovery, tools/list reports an error rather than
advertising success-shaped offline tools. MCP cancellation/disconnect does not
roll back an accepted command; the same unknown-outcome rule applies.

## Transport and ownership limits

The internal Unix protocol is a four-byte big-endian length followed by one
version 1 JSON request or response. Each connection carries one request with a
fresh UUID; a null command requests capabilities. This internal protocol is
separate from rmcp's stdio MCP transport.

Limits are 16 bridge operations, 16 owner socket workers, a 16-request channel,
and at most 16 persistence-waiting owner requests. Requests are limited to 1 MiB,
responses to 8 MiB using bounded serialization. Stdio incoming lines are limited
to 2 MiB before parsing. Oversized outputs do not truncate note source; they fail
with an explicit outcome warning. Socket connect has a 10-second deadline;
request/response I/O has a separate 10-second deadline. Owner dispatch and
persistence confirmation share a 10-second request deadline. Failure near a
transport deadline can mask a committed mutation.

Socket workers hold only owned messages and reply channels. The UI/headless
owner calls `OwnerSocket::poll(&mut App)` to drain bounded requests, invoke shared
dispatch and examine save acknowledgements without blocking. A socket arrival
requests an egui repaint; waiting persistence polls only until it resolves.
No worker locks/shares World or calls `App::open`. Mutations still use shared
policy, path validation and UTF-8/revision checks. Filesystem operations remain
local-only and are absent from automation discovery.

## Validation scope

The focused `mcp::tests::owner_socket_routes_policy_revision_and_durable_mutations`
scenario exercises the real Unix socket, private permissions, read-only rejection,
capabilities/schema filtering, write opt-in, stale revision and acknowledged
content surviving SQLite reopen. It reuses shared dispatch rather than
reimplementing command-domain tests. The integration check includes GUI + MCP
startup wiring. The focused scenario passed (1 test, 7 filtered) with `RUSTC_WRAPPER= cargo test
--lib --features mcp --offline
mcp::tests::owner_socket_routes_policy_revision_and_durable_mutations`. The
sandbox denied Unix socket creation, so the successful run used approved
escalation with only a temporary private directory/vault. The fixture also
checks that an older persisted acknowledgement cannot return a newer dirty
snapshot as though that snapshot were saved.

A real MCP stdio initialize/tools-list/tools-call smoke is also
required before claiming adapter readiness; record the actual result in the
capability/status record. This file describes implemented behavior, not a claim
that Android, sync or live Codex integration has been validated.

Primary SDK reference: [official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk).
