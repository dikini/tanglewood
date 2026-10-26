# Offline merge engine and integration contract

The isolated engine in `src/sync.rs` is preparatory Y3a work. It is not integrated into App or SQLite yet and does not complete the sync delivery gate. There is no network transport, pairing UI, migration, or advertised sync command.

## Histories and identities

One Automerge catalog contains the vault UUID and a map of note UUID membership. One Automerge document per note contains a shared Text object, path register, tombstone register, and note UUID. Note UUIDs survive migration and renames; catalog membership is retained after trash. Existing vault creation bootstraps once. Pairing calls `Replica::join` with the existing catalog and document histories, installs a fresh device actor, and must durably persist that actor before any edits. Peers must never independently create matching documents from Markdown. The engine rejects merging histories from another vault or note/catalog objects with independent origins.

`HistoryBundle` contains the version, vault UUID, device UUID/actor, catalog history, and document histories. `Replica::save` produces full Automerge history bytes; `load` resumes the saved device actor. Loading uses `load_with_options` with explicit UnicodeCodePoint encoding, complete-load errors and verified heads, then reinstalls the saved actor. The pinned source shows that `load_incremental` into an empty document instead restores platform-default encoding, so it is deliberately avoided. Joining uses the same histories with a different persisted device actor. A bundle is owned data suitable for the existing storage worker. The owner must commit history bytes and all changed SQL projections in one transaction. Persist local history before advertising changes; commit received histories before acknowledging remote durability. Sync protocol state can be regenerated from durable histories on restart.

## Source editing and indexing

The dependency is pinned to Automerge 0.7.4. Its [AutoCommit API](https://docs.rs/automerge/0.7.4/automerge/struct.AutoCommit.html) supports explicit text encoding, history loading, saving and merging. Its [Transactable API](https://docs.rs/automerge/0.7.4/automerge/transaction/trait.Transactable.html) defines `splice_text` with an index and signed deletion count.

All documents explicitly select `TextEncoding::UnicodeCodePoint`; do not rely on platform-default Cargo features. App commands continue to use UTF-8 byte offsets. The engine reuses shared range validation, converts each original range start/deletion to `str::chars().count()`, and applies splices from the end. An astral character is one unit; combining marks are separate units; these are neither UTF-8 bytes nor grapheme clusters. Source outside edited ranges remains unchanged. Imported whole text uses a common prefix/suffix diff that ends on character boundaries and emits one splice. App still validates caller policy, expected revision, path collision and trash state before invoking the engine; the engine is not a parallel command dispatcher.

## Conflict projection

Text merges as CRDT operations. The engine reads every concurrent path and tombstone value with `get_all`, retaining them in `MergeConflicts` and in persistent histories. The lexicographically first path is the desired path; notes are processed in note UUID order. A desired path already assigned to another note, including an ancestor/descendant collision, receives a deterministic `recovered-<note UUID>-<counter>.md` projection. Generated paths avoid all desired and assigned paths. The original conflicting path values remain recoverable rather than being overwritten with the projection.

Any concurrent `true` tombstone keeps the note trashed while preserving merged text. A later restore which observes that deletion can replace the register with false. Concurrent edit/delete retains the deleted note and recoverable edited content. Path resolution is independent of filesystem mirror baselines and does not permit silently replacing external files.

App integration must compare projected state with its accepted record, generate a fresh opaque revision for a changed remote projection, and retain/reset editor drafts and undo state through the existing source-change behavior. After migration histories become authoritative; ECS strings and SQL note text become projections. Unknown or malformed remote histories must leave the current replica unchanged.

## Remaining integration

A later task must create a recoverable pre-migration SQLite backup, transactionally seed histories/identities for existing UUID notes, load/save histories in the worker, route current commands through engine operations, and persist remote histories/projections before durable acknowledgement. Only then can authenticated QUIC transport and pairing be implemented. Offline engine convergence is not evidence for network, filesystem, Android, or live two-device readiness.
