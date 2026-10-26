# Authenticated foreground LAN sync

`src/lan.rs` implements an owned-message QUIC controller, separate from App and all mutable Automerge state. `lan_owner` and App integrate empty-vault joining and durable per-document synchronization. Two hardware devices and Android networking remain unverified. No command replay, public discovery, relay, accounts, or background Android service is included.

## Dependencies and authentication

The `lan` feature extends `sync` with Quinn 0.11.9, rustls 0.23.43, rcgen 0.14.7, SHA-256 and the existing optional Tokio runtime. Quinn uses runtime-tokio/rustls-ring; rustls disables its default provider and explicitly selects ring. Tokio is shared with other optional I/O adapters and never owns ECS. See the pinned [Quinn endpoint model](https://docs.rs/quinn/0.11.9/quinn/) and [rustls provider documentation](https://docs.rs/rustls/0.23.43/rustls/).

A ticket contains protocol version, numeric SocketAddr, vault UUID, self-signed public certificate and SHA-256 of its subject-public-key-info bytes. The user explicitly exchanges and confirms tickets on both peers. The public certificate is needed to establish a trust anchor; the fingerprint is computed from its parsed public key and validated before storing the pin. Tickets reject unspecified/multicast addresses, port zero, unsupported versions, nil vault IDs, oversized input and mismatched key fingerprints. A wildcard bind needs an explicitly selected advertised LAN address.

Both directions require TLS 1.3 client certificates. The server uses WebPkiClientVerifier with only locally confirmed certificate roots. The client trusts only the selected ticket's certificate root and supplies its own certificate; it verifies the fixed certificate DNS identity `tanglewood.local`. Standard rustls verification checks real handshake signatures, certificate validity and trust. After handshake, the controller rechecks the actual peer SPKI fingerprint against the current pin set and the selected outgoing peer. Unknown peers never deliver owner packets. No permissive signature verifier or encryption-only anonymous mode is used.

Device-local `<vault>/lan/identity.json` contains certificate and PKCS#8 key, with mode 600 and a mode-700 directory on Unix. It is outside shared CRDT histories, notes and mirror files. Startup rejects unsafe file permissions/symlinks and malformed existing identities instead of silently rotating them. The public certificate fingerprint remains stable across restart. `peers.json` stores confirmed tickets atomically, syncs the file and parent directory, and keeps mode 600. Pair/unpair perform this I/O on blocking workers; Confirmed/Unpaired events occur after persistence. Unpair removes the trust root and closes existing sessions. In-flight handshakes recheck the current pins before delivering data. The owner must also discard already-queued packets from a peer it has locally revoked.

## Bounded I/O API

`Controller::bind(vault_directory, vault_id, bind_address).await` initializes local assets and binds inside the caller's existing Tokio runtime. Its handle exposes `ticket(advertised_address)`, nonblocking `try_send(Request)` and `try_recv()`. Requests are Confirm(ticket), Unpair(fingerprint), Connect(ticket), and Send(peer, packet). Events report persisted pairing, authenticated connections, owned packets, disconnection, rejection and failures. Connection/packet/disconnection events include a local session ID; the owner must ignore an old session’s delayed events after reconnect. Duplicate live connections for one fingerprint are rejected. SendRejected returns the original packet when the per-peer queue is full or disconnected, allowing owner retention/retry. They contain no World, Replica or Automerge sync State.

There are at most eight peers/connection tasks, sixteen controller requests/events, eight outbound packets per peer, one bidirectional stream per connection and no unidirectional streams. Frames use a four-byte big-endian length plus versioned JSON, capped at 4 MiB + 1024 bytes. Sync payloads are capped at 1 MiB; a larger history remains usable locally and reports an unavailable/oversized transfer rather than truncating source. Payload reads and writes time out; QUIC has a bounded receive window and idle timeout. Full queues retain work on the owner or cause an explicit transport failure; receipt frames are never automatically fabricated. Dropping the controller closes foreground sessions and aborts its I/O task group.

Each Packet contains version, vault UUID, Catalog/Note(UUID) document identity, message UUID and a body: catalog Manifest(note UUIDs), Sync(encoded Automerge message), or DurableReceipt(received message UUID). The controller validates version/vault/size and handles bytes only. The owner decodes and applies actual `automerge::sync::Message` values with per-peer, per-document `State`; a receipt is emitted only after the application persistence gate below.

## Owner integration

Local-only pair/unpair/connect/listen commands use existing Accepted request/status semantics. Correlate pairing completion events to request IDs and keep unavailable capability distinct from policy rejection. Confirm pins before starting connections; do not auto-trust an incoming public certificate. Persisted pin removal must close sessions and invalidate queued owner work from that peer. App startup/session lifetime owns one optional Controller driven on its I/O runtime; Android uses only the foreground lifetime.

The owner holds `(peer fingerprint, document identity) -> automerge::sync::State` and private receive inputs. No network task generates or applies CRDT changes. On connect/reconnect, choose a single initiator (for example by fingerprint ordering), track the active session ID, construct fresh protocol states from durable histories, enumerate durable catalog membership, and generate sync messages from the last committed document snapshots. Accepted unsaved local changes are never advertised.

For a ready vault, apply an incoming sync message to an owner candidate, validate catalog/document identity and projection rules, and use the existing history-generation save path. Hold any next protocol acknowledgement/response whose contents imply receipt of those changes until that generation commits. A DurableReceipt merely correlates a committed receive; it is not proof of exactly-once command execution. Persistence failure keeps accepted dirty data and suppresses success receipts. Reconnect regenerates sync states from durable histories and uses Automerge's duplicate handling rather than a custom replay log.

For pairing into an empty vault, select the existing peer's vault UUID from its explicitly confirmed ticket. The initial join transfers the complete original histories using SHA-256-checked ordered 64 KiB chunks, capped at 32 MiB encoded history. It uses a distinct persisted destination actor and never independently seeds a matching Markdown catalog. Subsequent exchanges use actual Automerge sync messages. Wait for a complete, validated catalog/document set before installing projections and identities in one initial-join transaction. Refuse replacing a nonempty unrelated local vault. Persist the new local actor before its first edit. The existing pre-migration backup and no-sync feature guard must cover the join transition too.

Integration validation must use two real temporary vault Apps, close/reopen them, and demonstrate offline local edits, reconnect convergence, durable acknowledgements, retained rename/delete conflicts and unknown-peer rejection. Loopback checks establish transport behavior on one machine, not two hardware devices or Android readiness.

## Recorded transport evidence

`RUSTC_WRAPPER= cargo test --lib --no-default-features --features lan --offline lan::tests` passed one focused loopback scenario. It exchanges real Automerge sync messages on owner-side test documents through mutually authenticated QUIC, validates a tampered fingerprint, rejects an unpinned TLS client, rejects an oversized inbound frame length before body allocation, rejects an oversized outbound payload, proves unpair closes an existing connection, and reopens persisted identity/pins. The sandbox denied UDP creation on the initial run; the recorded passing run used approved local-network escalation. This is one-machine loopback evidence, not two hardware devices or integrated durable vault synchronization.

Owner integration is implemented in `lan_owner` and `App`: persisted empty-peer preparation, atomic existing-history join and per-document Automerge message exchange are available through shared local-only LAN commands. See `lan-owner-integration.md` for exact scoped evidence and remaining device/fault-injection limits. This supersedes the earlier standalone transport-only implementation boundary.

## Linux launch workflow

Build `cargo build --locked --bins --features gui,codex,lan`. The GUI's collapsed
Sync controls expose explicit migration, listen/ticket, empty join preparation,
confirmation, connect/unpair, pause/resume and operation status. Compare the
public fingerprint, address and vault ID with the other device before confirming.
No incoming ticket automatically grants trust.

For a CLI-only exchange, use two vault directories and fixed UDP ports. Actions
run in argument order. First the source prints its ticket and exits:

```sh
target/debug/tanglewood --headless --vault /path/to/source --enable-sync --lan-listen 192.168.1.10:43101 --lan-ticket
```

On an empty destination, paste that ticket as SOURCE_TICKET and print its own:

```sh
target/debug/tanglewood --headless --vault /path/to/destination --prepare-join "$SOURCE_TICKET" --lan-listen 192.168.1.11:43102 --lan-ticket
```

Exchange/compare tickets through a trusted channel. Restart both at the same
addresses, passing `--confirm-peer "$OTHER_TICKET"` after `--lan-listen`. Start
the peer with the lexicographically larger fingerprint first, adding `--lan-serve`
to retain its owner loop. Start the smaller fingerprint peer with
`--connect-peer OTHER_FINGERPRINT --lan-serve`. Use a single connection initiator.
Both peers must confirm one another; confirmation persists before connect.
The GUI can perform the same actions while its owner is running.

`--lan-status` prints structured JSON. `--lan-advertise IP` immediately after a
wildcard `--lan-listen 0.0.0.0:PORT` selects the advertised interface. Port zero
is useful only while that exact listener remains alive; its ticket cannot be
reused after restart. No discovery, relay or external service is used.

Actual binary smoke passed finite tickets, explicit confirmation, foreground
serve, empty join, original note UUID/Unicode bytes, distinct actor and restart
on UDP loopback (`/tmp/tanglewood-lan-cli-u29b7zbh`). The two-App scenario also
covers offline rename/trash and reconnect. These are one-machine checks; they
do not prove two hardware devices, wireless behavior or Android lifecycle.
