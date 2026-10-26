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
