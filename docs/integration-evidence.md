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

## Integrated LAN owner and Android source checkpoint

- `cargo fmt --check`: passed.
- `RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --features gui,codex,lan --offline`: **15 passed**, approved temporary Unix/UDP sockets. Adds two real App/SQLite owners, original-history join/distinct actor, Unicode Automerge messages, offline rename/trash, restart/reconnect and pending listener pause cancellation.
- `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,codex,lan --offline`: exit 0; style, concrete queue error size, argument count and unused helper warnings remain.
- `RUSTC_WRAPPER= cargo build --locked --bins --example codex_smoke --features gui,codex,lan --offline`: passed.
- Actual binary smoke `/tmp/tanglewood-lan-cli-smoke.py`: passed finite ticket exchange, explicitly confirmed foreground serve, empty join, identical UUID/Unicode source, distinct destination actor and restart (`/tmp/tanglewood-lan-cli-772uhd7z`). Temporary local UDP required approved escalation.
- Android source integrates SAF/JNI, mobile drawer composition recovery, manual LAN controls and nonblocking foreground runtime teardown. Temporary host JNI API check passed after the callback ownership fix; platform source independent review ready. No Android-specific entry/UI/Java cross-build, APK or device behavior is claimed.
- LAN core review identified a P1 staged-receive receipt bug and P2 retry ordering, retired control outcomes/pin-write serialization, and pending revocation issues. Fixes/regressions and delta review are required before checkpoint. No injected persistence-failure/incomplete-join fault check, hardware LAN or Android networking is claimed.

### LAN review corrections

The four concrete findings were corrected: event draining/receive generation
barriers preserve later staging; one in-flight packet per peer preserves retry
order; retiring controllers settles control IDs with unknown outcomes; owner
revocation survives pause/relisten until current explicit confirmation. A shared
async asset gate retains serialization through unabortable blocking pin writes;
subsequent loads/read-modify-writes cannot overtake them.

Focused post-delta commands (all with `RUSTC_WRAPPER=` and `--offline`):

- `cargo test --lib --no-default-features --features lan lan_owner::tests`: 3 passed.
- Same flags, filter `retired_pin_writer_finishes_before_next_asset_load`: 1 passed.
- Same flags, filter `app::tests::lan_two_apps_join_and_live_unicode_history`: 1 passed with approved local UDP.
- Same flags, filter `lan::tests::loopback_mutual_pins_real_sync_frames_unknown_peer_and_revocation`: 1 passed with approved local UDP.
- Final `cargo fmt --check`: passed.
- Final `cargo build --locked --bins --features gui,codex,lan --offline`: passed.
- Actual CLI workflow passed again after the fixes (`/tmp/tanglewood-lan-cli-2m2hpowc`).

The combined 15-test/Clippy run above preceded these fixes; focused regressions
and final binary rebuild cover the affected delta. The unchanged full suite was
not repeated. Delta review approved receive barriers, asset serialization and revocation. It found two P2 remnants: send requests/outcomes need exact session/message correlation across reconnect, and CancelJoin must settle cancelled listener startup. Those final fixes and focused checks remain pending.

Final two P2 corrections passed re-review: send requests and outcomes carry the
exact session/message identity, and CancelJoin settles pending listener startup.
Post-delta `cargo test --lib --no-default-features --features lan --offline lan_`
passed 4 tests; the actual TLS replacement-session transport scenario passed 1.
Final formatting, union-feature binary rebuild, and CLI smoke passed again
(`/tmp/tanglewood-lan-cli-u29b7zbh`). Independent LAN review is **ready**.

The user subsequently accepted the SDK license and selected a local emulator.
KVM exists and is readable/writable outside the sandbox; earlier absence was
sandbox visibility. Official command-line tools15859902 SHA-256 was verified;
SDK/NDK installation is underway in `/tmp/tanglewood-android-sdk`. Android Rust
targets are being installed. No APK/emulator behavior result is claimed yet.
