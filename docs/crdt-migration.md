# CRDT migration and App integration

Y3b now implements explicit migration and shared-command integration. Local schema-2 vaults remain string-backed until a trusted local caller requests EnableSync. Compiling the `sync` feature does not migrate a vault. Schema 3 stores authoritative histories with SQL projections.

## Explicit transition and recovery copy

The local-only `Command::EnableSync`, returning the existing `Accepted { request_id }` envelope. Its descriptor is available only with the `sync` feature. An already migrated vault completes idempotently with its existing identities; it never creates another catalog or actor. No disable/downgrade command is offered because converting authoritative history back into independently editable projections would fork the vault.

The owner uses a concrete migration state: `Local`, `WaitingForSaves`, `Seeding`, `Committing`, or `Ready`. Before accepting EnableSync, reject Busy if an import/rescan source-changing I/O request is outstanding. While migration is in flight, read/search/status and ordinary mirror export completions continue; new source mutations, imports and rescans return Busy. Retain GUI drafts on that error. Already accepted notes continue through the existing save queue. Once all source revisions are acknowledged, freeze that exact note set and bootstrap the catalog/documents on the owner. If necessary, seed a small number of notes per tick rather than moving the mutable Replica into the storage worker. This short, explicit mutation pause avoids silently losing edits accepted during migration.

Submit an owned `MigrationJob { request_id, history: HistoryBundle, projections: Vec<NoteRecord> }`. The storage worker first runs `VACUUM INTO ?1` outside a transaction, using a fresh `<vault>/pre-crdt-<UUID>.sqlite3` path which must not exist. The worker remains the only writer and retains the vault lock. SQLite builds this backup from its current database view, including committed WAL content; do not copy only `notes.sqlite3` with ordinary filesystem APIs. Sync the completed backup file and its directory before committing the migration marker. No new rusqlite feature is required. Record the backup path in migration metadata and leave it untouched on later failure. A failed backup aborts migration. A failed migration transaction retains the schema-2 database and backup; App discards the candidate engine, returns to Local, and exposes a failed request.

A recovery copy is a complete independent SQLite database. Recovery is explicit and offline: stop the application, retain the migrated vault, then open a copy of the backup as a separate local vault. Never overwrite a running vault or silently downgrade it.

## Schema 3 and opaque worker records

These tables are created only in the successful migration transaction:

```sql
CREATE TABLE sync_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    history_version INTEGER NOT NULL,
    vault_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    catalog BLOB NOT NULL,
    backup_path TEXT NOT NULL
);
CREATE TABLE sync_documents (
    note_id TEXT PRIMARY KEY NOT NULL,
    history BLOB NOT NULL
);
CREATE TABLE sync_conflicts (
    note_id TEXT PRIMARY KEY NOT NULL,
    metadata BLOB NOT NULL
);
```

`sync_conflicts.metadata` is a versioned Serde representation of current path alternatives, tombstone alternatives and deterministic path resolution metadata. Conflict metadata uses Serde JSON, with serde_json enabled by the sync feature. The full histories retain resolved historical alternatives even after current conflict metadata disappears. The existing `notes.revision` remains the persistence token for the projection; no new revision column is needed. The schema-2 settings, export baselines, pending exports and mirror conflict history remain intact.

The migration transaction verifies that the passed projection IDs/revisions match the current committed note set, inserts the exact engine vault/device identities, catalog and document histories, writes conflict metadata, and retains existing projected source/paths/trash/revisions unchanged. If mirror mode is on, upsert the corresponding pending exports in that same transaction without discarding baselines or conflict history. Finish with `PRAGMA user_version=3` and commit. Only its acknowledgement changes App to Ready and completes EnableSync. A crash before commit reopens local schema 2; a crash after commit reopens authoritative histories with the original identities and preserved UUID/revision tokens.

`StoreStartup.history: Option<HistoryBundle>` carries history bytes in the owned startup handshake; current conflict metadata is rebuilt from authoritative histories. Startup reads these bytes on the worker, then the owner loads Replica with the persisted device actor and validates that its projected IDs/text/path/trash agree with SQL. A disagreement fails clearly rather than choosing whichever side is convenient. `sync_status` exposes migration, accepted/persisted generations and history errors independently of note save/export status. After migration, SQL text/path/trash are projections and never seed another independent engine.

A binary without the `sync` feature must inspect the migration marker before any journal-mode changes, schema writes, or note command execution. If present, return a typed `StoreError::Unavailable("vault requires CRDT support")` and release the lock. It must never open that vault as a writable local-string App. Normal schema-2 vaults continue working without the feature. Keep capability unavailability separate from caller authorization errors in dispatch.

## Same command boundary, one owner

The existing policy, expected-revision, range, path and trash validation remains in `App::execute`. After validation, a Ready App applies the corresponding operation to its sole mutable Replica candidate:

- Create inserts the original new UUID into catalog membership and creates its document.
- Edit applies the existing byte-range request using Unicode-code-point splices.
- Rename updates the path register; trash/restore update the tombstone register.
- Explicit import and external-only rescan already become shared Create/Edit commands, so they require no second CRDT adapter.

Install the candidate only after the operation and projected-state validation succeed. Source changes receive the same fresh opaque revisions used today. A local projection which is unchanged keeps its revision. Bootstrap preserves all existing note IDs and revisions. Received state which changes a note's text/path/trash/conflict metadata gets a fresh revision, including a path affected by deterministic collision resolution. Preserve drafts and reset incompatible undo state through the existing source-change handling. Store current conflict metadata alongside the projected records.

A Ready App replaces the local-only save payload with `SaveHistoryJob { generation, history: HistoryBundle, projections: Vec<NoteRecord>, conflicts }`. The worker handles opaque owned bytes and SQL records; it never owns or mutates Replica. In one transaction, release participating projected paths using the current batch staging mechanism, write projections, write catalog/device/document histories, update conflict metadata, and upsert mirror outbox work when enabled. A projection-only save against a migrated database is rejected by the worker as a defensive invariant. Schema-2 App continues using the current save payload.

At most one history save is in flight. Coalesce only unsent full snapshots: a newer history snapshot contains all earlier accepted operations, so no operation delta is thrown away. The owner retains its accepted engine and dirty revisions if submission or persistence fails. An older acknowledgement records only its matching persisted revisions and cannot clear newer edits. Migration completion, database persistence and filesystem export remain distinct statuses.

## Remote durability boundary for the next increment

Do not expose a public MergeHistory command to automation. A future authenticated transport provides a private owner input carrying a paired-peer identity and bundle/sync messages. The owner validates vault/catalog membership and merges into a candidate; invalid data leaves accepted state unchanged. Changed remote projections and histories use the same atomic persistence job and mirror outbox path as local edits.

Assign an owner-local generation to each accepted engine state and retain remote receipt tokens awaiting that generation. Advertise local history and issue a durable remote receipt only after the corresponding history snapshot commits. A later coalesced snapshot can acknowledge earlier receipts because it contains those histories; verify this through the owned pending-generation bookkeeping. A transport timeout does not authorize blindly retrying a source command. After restart, rebuild sync state from committed histories and their heads rather than trusting an ephemeral pre-crash receipt. Actual authenticated network protocol and two-device evidence remain a later gate.

## Lean acceptance scenario

Extend one real SQLite/temp-vault App scenario: create Unicode notes through ordinary commands, flush, request EnableSync, wait for its durable acknowledgement, and verify the recovery database opens with the pre-migration IDs/text/revisions. Restart the migrated App and confirm persisted vault/device identity and history-derived projections match. Exercise ordinary create/edit/import/rename/trash/restore through the same dispatcher, save/restart, and confirm text and identities survive. Reject a stale edit as before. Exercise one received joined-replica merge that changes text/path and a tombstone conflict; verify a fresh source revision, durable history/projection/outbox acknowledgement, restart convergence and retained conflict metadata. A deliberately failed save must leave the accepted state dirty. Check that a no-sync startup inspection returns typed Unavailable for the migrated vault.

Use the existing offline engine scenario for merge internals; do not duplicate it through every adapter. Run the focused migration/App scenario under `--features sync`, then the smallest no-sync feature-gate check. No GUI, Android, network or live Codex check is implied. The PM runs combined checks once at the coherent integration checkpoint.

## Recorded Y3b evidence

The focused `app::tests::explicit_migration_commands_remote_ack_and_restart_preserve_authority` scenario passed with `--features sync`. It verifies a readable pre-migration backup with preserved UUID/revision/source, shared command edits/import/create/rename/trash/restore, identity/history restart, concurrent remote path/tombstone metadata, fresh remote revisions, save-generation acknowledgement, atomic mirror outbox projections, and rejection of projection-only writes. An injected actor-identity mismatch fails persistence while retaining accepted dirty source and the previous durable history. It is a persistence failure fixture, not a disk-full or power-loss simulation.

A closed real migrated fixture was opened with a binary built by `cargo run --bin tanglewood --no-default-features --offline -- --vault /tmp/tanglewood-migrated-feature-check --headless`. It returned `capability unavailable: vault requires CRDT support` with exit 1; SHA-256 of the database was identical before and after. Authenticated networking, live peer acknowledgements, and device testing remain open.
