# Codex adapter protocol preparation

Installed CLI checked 2026-10-26: `codex-cli 0.163.0`.
Official reference: [Codex App Server](https://learn.chatgpt.com/docs/app-server).
The required adapter is not implemented or live-verified yet.

`codex app-server generate-json-schema --out /tmp/tanglewood-codex-0.163-schema`
succeeded. This local generated bundle is the pinned implementation reference
for request/response fields, alongside the official lifecycle documentation.
It contains client/server requests and notifications plus approval types; do not
copy the entire generated protocol into the application when a small boundary
model suffices. The temporary bundle can be regenerated with that exact CLI.

Use child stdio JSONL: initialize response, then initialized notification,
thread/start or thread/resume, turn/start, incremental agent events, terminal
turn status, and turn/interrupt. Treat server-initiated approval IDs as distinct
from client request IDs; respond explicitly and scope pending state to thread/turn.
Unknown requests require a protocol error, not implicit approval. Disconnect must
fail pending work visibly while editing/autosave continue. Credentials stay with
the installed CLI; do not read or log auth files.

MCP document access must use the local single-writer bridge. Its permissions
remain independent from Codex process/filesystem permissions. A live smoke must
show streamed output and note-tool revision handling, cancel/approval behavior,
and a usable editor after disconnect; protocol schema generation is not that gate.
