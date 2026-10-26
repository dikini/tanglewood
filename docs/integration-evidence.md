# Integration evidence after Linux editor checkpoint

Base: unsigned `5a40e2c`. The next checkpoint integrates mirror recovery, MCP,
Android source preparation and the isolated CRDT engine. Runtime claims remain
limited to the exercised boundary.

- `cargo fmt --check`: passed.
- `RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --features sync,mcp --offline`: 10 passed, 0 failed. Approved escalation permits the temporary Unix-socket scenario. Includes mirror interruption/conflict recovery and identical-content rename swaps, SQLite restart/lock/stale acknowledgements, core byte/revision/policy behavior, derived-source fallback, offline replica histories and socket routing/durability.
- `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,mcp,sync --offline`: exit 0. Warnings: collapsible conditionals, a test range loop, large concrete queue-error variants, and currently unused CRDT operations awaiting App integration. No warnings-as-errors gate is claimed.
- `RUSTC_WRAPPER= cargo build --locked --bins --features gui,mcp,sync --offline`: passed.
- Real CLI smoke (`/tmp/tanglewood-files-smoke.py`): imported Unicode Markdown, enabled durable mirror, rescanned an external-only edit, exported the same source, reopened and verified identical bytes.
- Real MCP smoke (`/tmp/tanglewood-mcp-smoke.py`): actual rmcp stdio initialize/tools-list/create through private Unix owner, exact accepted=persisted revision response, SQLite byte equality and restart recovery. Temporary vault only. Socket fixture additionally checks read-only denial, schema filtering, revision conflict, original acknowledged snapshot and file modes.
- Independent review: F2 and its ownership fix ready; A4a ready; Android preparation ready. Isolated CRDT engine review tracked in mvp-status.md.

Existing Linux GUI create/multiline edit/preview/save/process-restart observation
is in [linux-smoke.md](linux-smoke.md). Android compilation/device lifecycle,
SAF picker, CRDT storage migration, LAN pairing/reconnect and live Codex remain
open. A library feature build is not evidence for those gates.

The subsequent isolated catalog-identity fix rejects noncanonical UUID aliases and verifies malformed merge rollback without changing saved histories. Targeted `sync::tests` passed after this delta; the broader unchanged scenarios were not repeated.
