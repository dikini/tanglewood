# Kimi conversations on Linux

Tanglewood can use the installed Kimi Code CLI 2.2.0 over ACP version 1. Kimi
uses its existing login; run `kimi login` in a terminal if the agent reports that
sign-in is required. Tanglewood does not read or copy credentials.

Build the app and its MCP bridge, then launch with a private socket directory:

```sh
cargo build --bins --features gui,kimi
mkdir -m 700 /tmp/tanglewood-ipc
target/debug/tanglewood --vault /path/to/vault --mcp-socket /tmp/tanglewood-ipc/owner.sock --kimi
```

The default Kimi working directory is `kimi-context` inside the vault directory.
Use `--kimi-cwd DIRECTORY` to select an existing working directory and
`--kimi-resume SESSION` to open a session listed for that directory. The app
canonicalizes the directory and rejects session IDs absent from the confirmed
list. With both `--codex` and `--kimi` and features `gui,codex,kimi`, the
conversation header offers a provider selector; Codex is selected initially.
Provider changes wait for the previous process to exit and retain draft context.

Connect in the conversation sidebar, then create or open a conversation. Kimi
stores its own session history; this MVP replays user and assistant text messages,
without reconstructing historical tool or plan cards. Text
snapshots of notes and selected files may be attached without changing their
source. Unconfirmed messages and attachments remain in the composer after an
error or disconnect. Check the existing session before explicitly retrying an
uncertain submission.

Model, thinking and mode choices come from the current Kimi session. Changing
one waits for the server response and replaces the full set of available choices,
which may change when the model changes. These controls are Kimi's native ACP
options; their labels and descriptions are supplied by the agent. Kimi's modes
are separate from Codex's sandbox, approval and network controls.

Kimi runs with the user's ordinary process privileges; its own tools and network
access are governed by Kimi and its selected mode. A Kimi approval is separate
from Tanglewood's owner policy. The **Allow agents to update notes and research**
setting controls whether *any* connected MCP client may request vault mutations;
changes are checked on every command, including existing connections. Multiple
clients can use the same owner socket concurrently. Revision conflicts and
persistent acknowledgements continue to come from the single vault owner.

Kimi session rename/archive, goals, client-side terminal/filesystem forwarding,
image/audio input, and a Kimi login UI are not included. See [smoke evidence](kimi-live-evidence.md)
for which installed-runtime, native UI and live behaviors have actually been checked.
