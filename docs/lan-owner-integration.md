# Integrated LAN owner implementation plan

The transport and CRDT migration are independently verified. This plan connects them through the existing App owner and SQLite worker. It does not claim that integrated LAN delivery, two hardware devices, or Android networking are already verified. Keep one package, the current shared command boundary, one mutable ECS owner, and the existing optional Tokio I/O runtime.

## Local command and request contract

Add these request variants to the shared Command enum, its single descriptor table and conditional request schemas:

| Command | Policy and result |
| --- | --- |
| `ListenLan { bind: SocketAddr, advertised_ip: Option<IpAddr> }` | Local-only; Accepted request ID. Bind and load device identity/pins off the owner thread, then report the actual address and ticket in status. Listening does not silently migrate a local vault. |
| `GetLanTicket` | Local-only read; returns the current validated public ticket or typed Unavailable. A wildcard bind requires an explicitly supplied advertised interface address. |
| `PrepareJoin { ticket: String }` | Local-only; validate the remote ticket and prepare its vault identity only if the local vault is empty and unmigrated. Persist that preparation before issuing a matching local ticket. It does not confirm a peer or install histories. |
| `CancelJoin` | Local-only; Accepted request ID. Clear uncommitted prepared/staged state durably; refuse cancellation during its commit transaction. Never downgrade an already joined vault. |
| `ConfirmPeer { ticket: String }` | Local-only; Accepted request ID. Persist the explicitly confirmed key pin, then enable trust and complete the request. User exchanges and confirms both peers' tickets. |
| `UnpairPeer { fingerprint }` | Local-only; Accepted request ID. Immediately stop owner processing/sending for the peer, persist removal, close sessions, discard its ephemeral protocol state and complete the request. Surface persistence failure without claiming durable revocation. |
| `ConnectPeer { fingerprint }` | Local-only; Accepted request ID. Resolve the persisted ticket, connect with mutual TLS, and complete on the matching authenticated connection event. It does not imply synchronization or durability. |
| `GetLanStatus` | Local-only read; reports listener state, selected vault/join phase, peer/session state, pending transfers, errors and accepted versus durable generations. No private key or note body is included. |
| `PauseLan` / `ResumeLan` | Local-only; Accepted request IDs. Pause closes transport sessions while retaining persisted pins, prepared join state and source histories. Resume binds again and reconnects explicitly configured peers with fresh protocol states. |

Commands remain available independently of GUI controls. `lan` disabled returns typed Unavailable before permission checks; automation callers cannot pair, choose interfaces or connect arbitrary sockets. Request IDs correlate outcomes without exactly-once mutation semantics. Extend `io_status` or add a typed I/O outcome payload for public ticket/listener/status data; do not encode structured results inside error strings.

Add `request_id: Uuid` to the transport's state-changing Confirm, Unpair and Connect requests and their success/failure events. Listener startup has its own correlated owner completion. A timeout or a late event completes only the corresponding still-pending request; never clear a newer request by matching fingerprint alone. Keep Source SaveState and mirror export errors independent from LAN operation status.

## Owner fields and trusted inputs

Add a small optional `LanState` outside persistent note components:

- Current Controller, listener configuration, active `(fingerprint, session_id)` set, and bounded unsent network work.
- `(fingerprint, document_id) -> automerge::sync::State` plus a protocol epoch for the active session.
- Pending receive generations/receipt tokens, incomplete document staging, and optional initial-join staging.
- Correlated control requests and foreground paused/resuming state.

All mutable Automerge documents, including temporary incoming documents, remain on the application owner thread. Controller tasks retain only network/configuration data and owned frames. They never access World, Replica, SQL or document sync State. Existing `merge_history` is an owner-only input helper, not an externally serializable command.

Each tick drains a bounded number of controller events and processes bounded document work. Before applying a packet, check the current confirmed peer, selected vault, active session ID and packet/document limits. Unpair or reconnect invalidates old session events, queued packets, unsent responses and old receipt tokens. A delayed Disconnected event cannot close a replacement session. Keep controller pin/session lock order from the reviewed revocation fix.

Use one connection initiator per peer pair, for example the lexicographically smaller key fingerprint, so simultaneous manual resume does not create competing connections. Reconnect creates fresh per-document State; do not persist or reuse stale in-memory protocol state across sessions. Keep accepted source editing usable while transport is absent or paused.

## Existing vaults: real Automerge messages and durability

Extend the engine with concrete owner-only helpers for catalog/note `generate_sync_message` and `receive_sync_message`. Keep document identity checks, Unicode encoding, shared catalog origins, canonical note IDs and projection validation in the engine. Outgoing frames contain actual Automerge message bytes, never EditNote command replay or routine full-vault snapshot replacement.

Generate initial sends from the last committed `durable_history`, using owner-held durable document views and per-peer State. Never generate messages from accepted unsaved changes. A successful history commit makes a newer durable view eligible for generation. A full controller/per-peer queue retains the exact generated Packet in bounded owner work; a SendRejected completion also returns the original packet. Do not generate a second protocol message and silently discard the first state transition. If bounded retention is exhausted, close/restart the peer session and regenerate protocol state from durable histories.

On receive, decode one bounded Automerge message on the owner, clone the relevant accepted document state, and apply it with that session's State. A legitimate catalog update may mention a new note before its document arrives. Keep such incomplete input in bounded owner staging; request/receive the missing document through its own Automerge protocol State. Do not install a catalog projection with missing documents, reject a valid new note merely because its catalog frame arrived first, or create a matching note independently from Markdown.

Local edits continue through the authoritative accepted Replica while receive staging is incomplete. Once the staged catalog/document set validates, merge its histories into the latest accepted Replica candidate, preserving local changes since staging began, then refresh all affected source projections. The existing generation save transaction persists histories, changed projections, conflict metadata and mirror outbox together. Staging is not a second authoritative application state or a second ECS owner.

A received message can modify protocol State even when it changes no source projection. Track the generation/heads whose receipt or next protocol response would imply that those changes are known locally. Hold that response and its `DurableReceipt { received_message_id }` until `persisted_generation` covers the accepted receive state. Duplicate messages already contained in durable histories can be answered immediately. For new changes, merely completing Controller send/receive or observing accepted source is insufficient. Every response generated after receive must use a durable document view that contains those changes.

On persistence failure, keep accepted source and histories dirty, show the existing history error, suppress successful receipts, and retain/retry bounded work. Do not acknowledge received history and then reconstruct it only from uncommitted source strings. After restart, durable histories and fresh protocol State determine what must be resent. Existing transport receipt UUIDs are correlation hints, not a replay/exactly-once database.

## Initial joining: complete original histories, then one commit

An empty peer needs a destination ticket before receiving histories so both sides can confirm keys. Add a small device-local persisted `lan/prepared-join.json` containing selected vault UUID, the explicitly supplied source ticket, and a newly generated local device actor UUID. Use the same atomic file/directory sync and permission discipline as pins. It contains no histories and is not an independent vault catalog. Preparing refuses any nonempty note set or an existing authoritative history, including a migrated empty vault with a different identity. Repeating the same preparation is idempotent; selecting another vault requires cancelling an uncommitted preparation and explicit confirmation.

After PrepareJoin and ListenLan, the local ticket advertises that selected existing vault UUID. Each user confirms the other ticket; only then ConnectPeer can transfer data. The source sends a manifest from a *durable* HistoryBundle. Initial bootstrap may use bounded snapshot chunks: version, vault UUID, transfer UUID, document identity, total byte length, chunk index/count and an integrity digest. Include the original catalog bytes and every original note document history. Device TLS keys, source device actor identity, layout/preferences and peer pins are never copied into the destination identity.

Bound each chunk under the reviewed frame/payload limits and bound total in-flight bytes/documents. Retain unsent chunks under backpressure; reject inconsistent lengths/counts, duplicate chunks with different bytes, digest mismatches, unsupported versions and documents outside the manifest. Handle a document larger than the working envelope with an explicit error or bounded disk-backed staging; never truncate source. A transfer may restart from the source's latest durable manifest after disconnect. Do not combine chunks from different transfer UUIDs or different snapshots.

The owner assembles and validates the *complete* catalog/document set, then joins the original histories with the prepared local actor. Before any accepted source is exposed, verify the local database is still empty/unmigrated and acquire an initial-join mutation pause. Incoming work never overwrites a nonempty or unrelated migrated vault. A user can keep local editing before this pause, but creating a note cancels/refuses join installation rather than discarding that local note.

Add a concrete `StoreRequest::JoinVault { request_id, history, projections, conflicts, expected_empty: true }`. The worker verifies emptiness again, creates/fsyncs the existing recoverable pre-CRDT database backup, and commits schema-3 identities, catalog/document histories, projected records, conflict metadata and mirror outbox in one transaction. The new local actor was durably prepared first and is persisted in sync_meta; source note UUIDs remain unchanged. SQL revision tokens for newly installed projections are generated by the owner and become durable in that transaction.

Before commit, keep the original local database empty and report joining/pending status. Do not emit a semantic receipt for any chunk or manifest. After commit, install the owner Replica/projections, complete the join request and emit the transfer's durable receipt. A crash before commit reopens an empty local vault with an optional prepared join for retry; a crash after commit reopens the same joined histories and actor. The source can safely resend after a lost receipt. Delete/clear prepared-join metadata only after commit; a leftover preparation matching the now-committed vault is a harmless completed preparation. Without `sync`, the existing schema-3 feature guard refuses writable startup.

After this initial join, all ongoing changes use per-document Automerge sync messages. A new transfer snapshot is not a substitute for the ongoing merge protocol.

## CLI workflow and foreground lifecycle

Provide a real workflow through flags/subcommands without requiring a GUI button:

1. Source: create/edit notes, then `--enable-sync`; wait for its durable completion. Start `--lan-listen ADDRESS`, print `--lan-ticket`, and keep the owner event loop running.
2. Destination: use an empty vault and `--prepare-join SOURCE_TICKET`; start its listener and print its ticket for the source. Preparation alone does not trust the source key.
3. Both sides: `--confirm-peer OTHER_TICKET`, wait for persisted confirmation, then `--connect-peer FINGERPRINT` on the selected initiator. `--lan-status` reports authenticated, joining, pending and durable completion distinctly.
4. Keep the process/owner alive while editing. `--pause-lan` closes foreground sessions; `--resume-lan` rebinds and resumes from durable histories. `--unpair-peer` persists revocation and closes connections.

Main/startup wiring belongs to the PM. Implement explicit finite ticket/preparation mode and `--lan-serve` foreground mode, so startup flags form a usable workflow without trying to open a second writable App. For the finite ticket exchange, choose fixed bind ports: source and destination each print their ticket, persist device identity/preparation, then exit. Reopen each at the same address/port with the other ticket confirmed; start the non-initiator in serve mode first, then the initiator with Connect and serve. The identity/ticket remains valid across that restart. An ephemeral ticket port is suitable only when the same listener stays alive; label that restriction clearly. A serve-mode invocation must not exit immediately after Listen or Connect; keep driving the owner or use the GUI event loop. Source and destination may use different directories and bind ports on one host for smoke testing. Printed tickets contain public key material and addresses, never private keys.

Expose the same Pause/Resume owner hooks for Android lifecycle integration later. Suspension closes foreground networking and retains durable history/pins; resume rebuilds sessions on the active foreground runtime. A lifecycle callback is best effort, and source persistence cannot depend on a shutdown callback. Do not advertise a background Android service or claim Android network readiness without a representative device run.

## Single integration scenario and evidence boundary

Extend one focused test using two real temporary vault Apps, actual SQLite workers, and two authenticated UDP loopback endpoints. Seed/migrate the source, prepare an empty destination, explicitly confirm both tickets, join original histories, and verify source UUIDs plus the destination's distinct persisted actor. Assert that no join receipt occurs before its SQLite commit and that an invalid/incomplete transfer leaves the local database empty.

Edit Unicode text, rename and trash/restore through existing shared commands on disconnected peers. Flush each local state, reconnect with fresh States, exchange actual Automerge messages, and verify converged projections plus recoverable path/tombstone metadata. Confirm dirty accepted changes are not advertised and received changes get no success response/receipt until their generation commits. Close/reopen both Apps and verify the same identities/histories converge again. Include an unknown peer rejection and persisted unpair closing a session; reuse the transport fixture for its detailed TLS/frame regression coverage rather than duplicating every case.

Run this one scoped integration scenario with the `lan` feature and approved loopback networking. The PM runs combined formatting/tests/Clippy once at the checkpoint. Record exact commands, whether escalation was needed, and the distinction between loopback and real devices. Hardware LAN, wireless reconnect behavior and foreground Android remain explicit missing evidence until exercised on representative devices.
