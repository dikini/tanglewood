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

## Authoritative histories, Codex, and LAN transport checkpoint

- `cargo fmt --check`: passed.
- `RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --features gui,codex,lan --offline`: 14 passed with approved Unix/UDP loopback access. After the reviewed LAN revocation/frame-size fixes, focused `lan::tests` passed again; unchanged scenarios were not repeated.
- `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,codex,lan --offline`: exit 0; existing style, large queue error, and pending integration dead-code warnings remain.
- `RUSTC_WRAPPER= cargo build --locked --bins --example codex_smoke --features gui,codex,lan --offline`: passed.
- Actual CLI import → enable-sync → restart → repeated enable-sync preserved note UUID/revision/Unicode bytes, schema 3 and vault/device identity, and produced a recoverable pre-CRDT backup (`/tmp/tanglewood-migration-cli-geu25coz`). A no-sync executable refused the migrated fixture without changing its database hash.
- Authoritative history migration/commands/remote merge generation scenario passed; independent review ready. See `crdt-migration.md`.
- Actual authenticated Codex streamed read/edit/read, three individually scoped tool approvals, persisted edit, interrupted turn, disconnect and subsequent editing/restart passed. Actual panel Connect/Ready/Disconnect left source editing/autosave responsive. See `codex-live-evidence.md` for exact boundaries. Independent review ready.
- LAN transport fixture passed real mutual-TLS UDP pairing, unknown peer rejection, persisted unpair/session race regression, Automerge message transport and maximum serialized payload. Independent review ready. This is transport preparation: App owner sync, join/durable receipts, hardware LAN and Android networking remain open.

Android picker/packaging source is under development outside this checkpoint. No Android APK/device validation is claimed.
