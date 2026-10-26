# Android emulator / Linux sync acceptance

Date: 2026-10-26. This record distinguishes actual emulator/Linux execution from prior Linux loopback tests. The scoped cross-platform workflow below completed; Android native Unicode composition remains a separate gate.

## Linux destination preparation

The Linux worker owns no ADB or Android UI actions. The Android worker will seed original note histories in the emulator, enable sync, and provide its public source ticket. Linux will prepare an empty destination from that ticket, listen on `127.0.0.1:43101`, and advertise `10.0.2.2:43101` (the emulator's host alias). Android initiates the connection; Linux does not attempt to route directly to emulator address `10.0.2.16`.

Temporary evidence/vault root: `/tmp/tanglewood-android-linux-ZycnzH`. It is retained for handoff. No source database is copied between platforms. Both tickets must be explicitly confirmed locally. Linux source edits will use the actual `tanglewood-mcp` rmcp stdio bridge and private Unix socket attached to the single CLI owner, preserving shared revision checks and exact persistence acknowledgement.

Current host binaries:

- `target/debug/tanglewood`: SHA-256 `2cd9a6c6a3ca78e550d5b9ea09e640178486451d57757eb97f0617016970979e`.
- `target/debug/tanglewood-mcp`: SHA-256 `636d4ad7b428ae103a6f9459f13643d49e4a28a33d9e4690c23257882411df32`.

No Cargo job or production source change was needed for Linux preparation. The destination was subsequently prepared and Android was confirmed as described below.

## Actual run

Android provided its source ticket for vault `e74e1b56-a1a0-4c89-9ae5-94b80dbd66a6`, advertised `10.0.2.16:43102`, fingerprint `4db3a1ba89b089a51145c0ad05aff0d9c377b9b4653d5afc5ed2dbc342ff1756`. Linux ran the actual binary with `--prepare-join`, `--lan-listen 127.0.0.1:43101 --lan-advertise 10.0.2.2 --lan-ticket`. The database still contained zero notes after preparation; it persisted destination actor `41b48055-cda5-4093-9832-26234875abad`.

Linux public ticket fingerprint: `ff8a9fdb46f335eaaf40c8a0d63f6ccf38c71ad4881d48472008b23245c4e9ca`, address `10.0.2.2:43101`, same vault ID. Public ticket file: `/tmp/tanglewood-android-linux-ZycnzH/linux-public-ticket.txt`. Linux then started foreground `--lan-serve` with the Android ticket explicitly confirmed and `--mcp-socket /tmp/tanglewood-android-linux-ZycnzH/owner.sock --mcp-write`; PID recorded in `linux-owner.pid`. UDP and private Unix socket access required sandbox escalation.

Android's actual GUI parsed and confirmed the Linux ticket, initiated Connect, and reported `connected:true` with receipt `602f854c-4c6d-4fc4-be15-03d99cf51e65` (platform screenshot `/tmp/tanglewood-emulator-connected.png`). Linux then verified the committed SQLite projection and actual rmcp `ReadNote`:

- Original UUID `f5012443-5ae3-40d5-aa2b-cf1ebd51957a`, path `Android-source.md`, exact 105 UTF-8 bytes matching `/tmp/tanglewood-android-saf-seed.md`, SHA-256 `4820bf25598728368c56a00b00f2cdbf9cc58f9e930c4db59be028d8ff63634d`.
- Original UUID `df94c6b1-f27e-4512-b8f2-a5df7fb535aa`, path `Untitled.md`, exact `caf\n` (4 bytes), SHA-256 `f1d47294f2ed8953b27c50844643ac4fad91104e5e391995ddd4ad4f9f240bda`.
- Same vault UUID and the prepared destination actor retained. The preparation row was deleted atomically with the join. MCP reported accepted revision equal to persisted revision for the joined source (`e1323e76-2c8e-4478-abef-ddd2848ea9ca`). Full Linux snapshot: `joined-snapshot.json` in the evidence root.

The source Unicode bytes were introduced through Android's SAF import, not proven Android Unicode keyboard composition. Disconnected edits, distinct actors and durable restart are recorded below. An emulator run does not prove physical Android hardware, wireless LAN behavior, or a background service.

## Disconnected changes

The Android worker sent Home, triggering the actual foreground Pause hook without force-stopping. Read-only Android SQLite identified source actor `596f933e-e1f6-46c1-ab3c-4ad3f53af924`, distinct from Linux actor `41b48055-cda5-4093-9832-26234875abad`.

While Android networking was paused, Linux used the actual rmcp bridge for `ReadNote`, revision-checked `EditNote`, then `RenameNote`. The original source UUID stayed `f5012443-5ae3-40d5-aa2b-cf1ebd51957a`; its path became `Linux-reconnected.md`, and its text gained `\nLinux offline 🌲 café Ελληνικά\n`. Both mutation responses confirmed accepted revision equal to persisted revision. Final persisted revision: `3226873a-99ef-4e32-a08e-5d71d832880c`; exact 148 UTF-8 bytes, SHA-256 `85d61ec2b1995714b960bb88a122c8e80a20cf3b6e8c6dd8a52b9899458c21d6`. Retained evidence: `linux-offline-mcp.json` and `linux-offline-expected.md` in the temporary root.

Android's separate offline change and reconnection are recorded below. Linux's Unicode edit proves cross-platform transport only once Android receives it; it does not prove Android keyboard Unicode composition.

Android then saved an offline GUI edit to `Untitled.md`: exact `caf\nAndroid offline\n` (20 UTF-8 bytes), screenshot `/tmp/tanglewood-emulator-offline-edited.png`. After explicit Resume and emulator-initiated Connect, Linux's committed SQLite projection contained that exact text, SHA-256 `e49f3fbab16b134a72a1baab0b658a9eb7fb54510432e051887fd690c89ea0bf`, with original UUID `df94c6b1-f27e-4512-b8f2-a5df7fb535aa`. The renamed source retained its original UUID and exact 148-byte Linux Unicode text. Linux snapshot: `reconnected-snapshot.json`.

## Both-owner restart result

After Saved/receipt, the Android worker force-stopped the emulator app and checked its SQLite/WAL snapshot read-only. Both converged records matched Linux exactly: `Linux-reconnected.md` was 148 UTF-8 bytes with SHA-256 `85d61ec2b1995714b960bb88a122c8e80a20cf3b6e8c6dd8a52b9899458c21d6`; `Untitled.md` was 20 bytes with SHA-256 `e49f3fbab16b134a72a1baab0b658a9eb7fb54510432e051887fd690c89ea0bf`. Relaunch retained both notes. LAN did not automatically listen after Android process restart, consistent with explicit foreground activation.

Linux terminated only its owned foreground process after all mutation acknowledgements and converged snapshots, then reopened the same vault and fixed UDP port using the current binary. The restarted owner loaded the persisted Android pin without a new Confirm. Read-only SQLite assertions passed for exact UUIDs, paths, text bytes, trash flags and persisted revisions from the pre-restart snapshot; vault/actor identities stayed unchanged and two per-note CRDT histories remained. A fresh actual rmcp `ReadNote` confirmed the Android offline edit's accepted/persisted revision equality. Retained Linux snapshot: `restarted-snapshot.json`.

The owner remains available for later acceptance work: PID `3186573` (current PID file is authoritative), UDP `127.0.0.1:43101`, private MCP socket `/tmp/tanglewood-android-linux-ZycnzH/owner-restarted.sock`. The prior socket pathname is retained as evidence after process termination; it is not the active endpoint. Private vaults and logs remain under the evidence root. No production source changes, Cargo jobs, direct SQL writes, or ADB actions were performed by the Linux worker.

Result: actual Android emulator GUI / Linux CLI+MCP existing-history join, distinct actors, bidirectional disconnected edits, Unicode byte preservation, rename, explicit reconnect and both-owner durable restart passed. This was not two physical devices. Android Unicode keyboard/composition remains a separate acceptance gate; the source Unicode entered via SAF and the later Unicode append entered through Linux MCP. Trash/restore and conflict policy were not repeated through this adapter because existing domain/real-two-App scenarios already cover them.
