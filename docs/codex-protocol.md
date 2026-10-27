# Codex adapter protocol reference

Installed CLI checked 2026-10-26: `codex-cli 0.163.0`.
Official reference: [Codex App Server](https://learn.chatgpt.com/docs/app-server).
The adapter and vault-project workflow are implemented; see [live evidence](codex-live-evidence.md).

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

Project mode additionally uses the same CLI's experimental schema:
`codex app-server generate-json-schema --experimental --out /tmp/tanglewood-codex-project-schema`.
It enables experimental capability, calls `project/create`/`read`/`update`, filters
`thread/list` by projectId (including appServer source kind), assigns projectId on
`thread/start`, loads paginated `thread/turns/list` history, and handles pushed
`thread/status/changed` for known project threads. Creation uses the durable
vault idempotency key. The legacy project-less adapter path remains for the
existing scoped smoke harness. These experimental fields are pinned to 0.163.0.

## Controlled turns and command events

SendPromptWithControls adds TurnControls: Work/Plan/Research/Goal,
Ask/AutoReview/NoPrompts/FullAccess, and explicit network_access. It requires an
available selected model and supported reasoning effort. Work/Goal use default
collaboration, Plan uses plan, Research uses default with research instructions.
Work guidance uses real update_plan updates instead of repeated progress lists.
The child starts with process-local `-c tools.update_plan.enabled=true`; this
pinned runtime setting defaults off. Plan collaboration disallows that tool.
Initialization enables experimentalApi even without a project binding because
controlled turns use experimental collaboration fields.

Ask maps to on-request/user; AutoReview to on-request/auto_review; NoPrompts to
never/user. All three use readOnly with explicit networkAccess. FullAccess maps
to never/user/dangerFullAccess; the UI requires explicit selection. MCP tool
approval policy remains independent. Legacy sends and initial thread start/resume
keep readOnly/networkAccess:false and on-request/user. No global config is changed.
The bridge publishes truthful read-only capabilities/read annotations; read uses
shared AutomationReadOnly command descriptors for both schema and validation,
rejecting forged mutations before owner forwarding. Execute remains compatible.

See [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference),
[MCP policy](https://learn.chatgpt.com/docs/extend/mcp), and pinned
[ToolsV2 source](https://github.com/openai/codex/blob/rust-v0.163.0/codex-rs/config/src/config_toml.rs).

GoalInfo records server timeUsedSeconds; the UI does not infer autonomous work
or elapsed time locally. CommandExecution items started/completed plus outputDelta
notifications are accepted only for the current selected thread and active turn.
Their identity/output fields are bounded; output is UTF-8-safe at64KiB per item
and tracked items at32 per turn. Final aggregate output replaces streamed text.
Historical command events remain outside the current plain-message history API.

Goal mutations emit GoalMutationFinished only on their correlated acknowledgement;
live updates supersede stale reads, not mutations. Each mutation acknowledgement
is followed by a fresh read. Ready/reconnect invalidates UI goal freshness.
A selected thread with a confirmed active goal may accept a server-started turn
without a client turn/start. It uses the normal turn deadline and routing; other
unsolicited/foreign turns are ignored. Nullable command aggregatedOutput is
marked output_complete:false so the UI keeps streamed output and truncation;
authoritative empty strings deliberately replace previous output.

TurnAccepted is emitted only for a successful correlated client turn/start reply;
TurnStarted includes independently scheduled goal turns and does not acknowledge
queued input. Client rejection preserves an independently active turn/deadline/
approvals. A conflicting accepted turn ID reports uncertain outcome and retains
draft context rather than attributing acceptance to the unrelated active turn.

Activity emits live item/started and item/completed for mcpToolCall, webSearch and
fileChange, after the same selected-thread and active-turn checks as commands.
Retained item count is32 per turn; title512 UTF-8bytes, status64 and selected
arguments/result/error details8192. Event queue byte accounting includes these
fields. Private raw reasoning is not included. Historical activity reconstruction
is not added. Plan steps retain their existing reported status contract and
are read-only conversation data, not a new vault task schema.
