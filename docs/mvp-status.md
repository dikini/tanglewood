# MVP execution status

Required MVP increments 0–4 are delivered and independently reviewed, with actual Linux, Android emulator, LAN and live Codex evidence. Goal authority: `plan/tanglewood-mvp.md` and `mvp-goal.md`. Implementation checkpoints before the final Android integration: **ddc8fb3**, **499c870**, **2dcef01**, **5a40e2c**, all unsigned. The final delivery commit is the Git commit containing this record.

| Scope | State | Acceptance evidence |
| --- | --- | --- |
| Shared core / SQLite / Linux editor | done | Stable IDs/revisions, single ECS owner, exact durable acknowledgement, restart/lock/batch recovery; actual Linux window edit/save/restart |
| Markdown / extensions | done | Derived reader/filter/render/source-origin/fence contracts, callout and Pandoc-compatible example, canonical fallback |
| Import/export / Linux mirror | done | Durable outbox/baselines/recoverable conflicts, actual CLI import/rescan/export/restart |
| MCP / Codex | done | Private single-writer socket, shared validation, persisted replies; live scoped read/edit/read, stream/cancel/disconnect/restart and panel responsiveness |
| CRDT / authenticated LAN | done | Backup/migration, authoritative histories, generation barriers, pinned TLS, empty join, offline merge/conflicts; actual Android/Linux pairing/reconnect/both-owner restart |
| Android packaging / SAF / lifecycle | done | Java-capable API35 x86_64 APK, exact-byte SAF round trip/cancel, system/keyboard insets, rotation/background/restart |
| Android text input | done | Framework Editable/InputConnection mirror; real AOSP preedit/commit/accent/emoji, selection, system cut/paste, cancel/handoff and exact Saved/kill/restart |
| Delivery / review | done | Reproducible build/launch/configuration docs and capability table; scoped independent reviews and relevant integration checks |

## Final integration evidence

`integration-evidence.md` records commands/results. Combined 15 domain/adapter scenarios passed before final LAN corrections; focused post-correction scenarios, final host build and real CLI smoke passed. The Android input fixture passed UTF-16/surrogate/session/coalescing/epoch/handoff/menu regressions. Combined host all-target Clippy (`gui,codex,lan`) and final formatting passed. Actual final x86_64 Rust+Java APK build passed; SHA-256 `7b2897d9fbbf46e7267164f573432e3c3b20bf8ba9f219f146210be53fdf0ddb`.

Final Android input changes were independently reviewed after composition recovery, monotonic owner epochs, designated Edit-menu closing-frame protection and bounded Java publication ordering fixes. No findings remain. Exact final committed text `caf\nAndroid offline\ncafé 😀\nOK \n` (35 UTF-8 bytes) survived force-stop/relaunch, SHA-256 `f2efe33381886d4578cc977b363a5f65da5d08b17540c1cfa24a2e600f6fecbb`. See `android-emulator-evidence.md`; cross-platform IDs/actors/history/restart evidence is in `android-linux-sync-evidence.md`.

## Scope and limitations

Optional D-Bus and the external filter process runner were explicitly deferred; native contracts/example are delivered. Block editing/general plugin management and other plan-deferred features remain outside this MVP. Physical wireless LAN, ARM64 runtime and other Android devices/keyboards were not tested. A representative API35 emulator plus Linux owner satisfies the planned platform/two-device gate; it does not prove a broad hardware matrix. Android sync remains explicit and foreground-only. Wait for SQLite Saved before terminating; unsaved drafts/preedit are not durable acknowledgements.

The user accepted the SDK license and chose the emulator. Temporary SDK/NDK: `/tmp/tanglewood-android-sdk`; AVD `Tanglewood_API35`, serial `emulator-5580`. No external publication is authorized or performed. Only PM stages/commits, unsigned. No required implementation or acceptance task remains.
