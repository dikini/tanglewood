# Vault agent permissions

**Goal:** Let the user allow or revoke agent updates in the app, without restarting the MCP owner or an existing conversation.

**Architecture:** A local-only shared command persists a vault preference through the existing SQLite worker. The running MCP owner consults that preference for every dispatch. The GUI exposes the same setting in Connections and the conversation's action menu. Existing process/network permissions remain separate.

**Accepted design:** The user approved replacing the launch-flag-only experience with an in-app vault permission. Keep the read-only default; do not automatically grant agent writes or restart the user's running process. No new research collection or generation capabilities.

## Permission behavior

- Missing preference: use the existing session launch policy, preserving `--mcp-write` for headless/legacy use.
- Explicit vault preference: overrides the launch policy, including an explicit disable.
- Grant only after durable persistence acknowledgment. A pending or failed grant stays read-only.
- Revocation takes effect immediately for requests not yet accepted; already accepted writes still finish their normal persistence acknowledgments. A failed revocation save remains revoked in this process and exposes an actionable error.
- Only a local user action may change the setting. Agents cannot enable their own permission, including with automation write permission already granted.
- One policy applies to all connected agent/automation clients of this vault's socket. Label it accordingly, rather than suggesting it changes only one chat.
- This setting does not change process sandbox, command approval, network access or local-only command restrictions.

## Existing connections

`execute` publishes a stable schema of potentially authorized commands, including mutations, so clients caching their initial tool list can use newly granted permissions. Its annotation always identifies it as potentially mutating. Schema visibility is not authorization: capabilities reports the current permission and the owner validates every request. Read queries stay non-mutating. Do not require a Codex reconnect to refresh a policy-filtered cached schema.

Update misleading note-only tool guidance to include research workspaces and source commands. A source collection does not require a specially named vault note. The agent should inspect available research workspaces rather than assume the conversation's historical topic identifies the target.

## Work and validation

1. Medium Sol state worker owns `src/app.rs`, `src/store.rs`, `src/commands.rs`: durable setting, local-only validation, restart/failure/dispatch regression.
2. Medium Sol adapter worker owns `src/mcp.rs`, `src/codex.rs`: live policy, stable schema, research guidance, same-socket grant/revoke regression.
3. Root owns `src/ui.rs`, documentation and integration: quiet contextual control, settings control, explicit errors, consistent effective state.
4. Xhigh Sol independent review of frozen files: grants, revocation races, durable acknowledgments, client caching, feature gates and draft preservation.
5. Root serializes relevant tests and native build. Verify native setting and live capabilities/mutation policy across grant/revoke and restart using disposable vaults. No paid model turn required. Keep the user's existing process and drafts intact.

## Acceptance evidence

- Independent Sol 6.1 xhigh review: READY, no blockers.
- `RUSTC_WRAPPER= cargo test --locked --offline --lib --features gui,codex`: 79 passed, 1 optional external-tool test ignored. Includes grant/revoke durability, failure/retry, restart, no self-grant, cached schema and queue-boundary regressions.
- `RUSTC_WRAPPER= cargo build --locked --offline --bins --features gui,codex`: passed; two pre-existing unused theme-helper warnings.
- `cargo fmt --check` and `git diff --check`: passed.
- Native disposable Linux vault started without `--mcp-write`: capabilities reported ReadOnly. Settings → Connections checkbox saved `1`; the same running socket reported ReadWrite and durably created `Research/Permission smoke.md`.
- Conversation actions → Vault updates revoked permission on that same process/socket. Capabilities returned ReadOnly; a subsequent create and an attempted automation permission grant both returned Unauthorized. SQLite stored `0`.
- Restarted the disposable vault with `--mcp-write`: saved disable still reported ReadOnly, and the earlier note survived. No provider conversation/turn was started.
- Native setting screenshot: `docs/design/vault-agent-permission-linux.png`.

All test processes were stopped. No existing user process was restarted or its
permission changed. The updated executable must be reopened once to load this
code; subsequent permission changes do not restart the owner or conversation.
