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
composer's searchable model picker groups both enabled providers. Discovery is
automatic when the conversation surface opens and can be refreshed or stopped in
Connections. Kimi discovery creates one temporary blank session and attempts to
close and delete that same session; any unconfirmed cleanup is shown separately.

Choose a model to prepare a connection and session; only an explicit Send starts
a prompt. Opening a listed session loads available history without resending a
prompt. Kimi stores its own session history; this MVP replays user and assistant
text, without reconstructing historical tool or plan cards. Explicit note and
file attachments are text snapshots. Unconfirmed input remains available after
an error or disconnect; inspect the session before retrying an uncertain send.

Model, thinking and mode choices use Kimi's native ACP option IDs. A model change
waits for its settled response and replaces the whole option snapshot. If the
access mode changes unexpectedly, review it before sending. Switching providers
from a conversation with history asks before creating a new chat; only the exact
prompt and explicit attachments are copied, and the original can keep working.

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
