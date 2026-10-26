# Android emulator acceptance evidence

Recorded 2026-10-26 on private `Tanglewood_API35`, serial `emulator-5580`: AOSP
API 35 x86_64, 720×1280 at density 280, KVM/software GPU, bundled AOSP Latin IME
and DocumentsUI Downloads provider. This is one representative emulator result,
not physical-phone, Wi-Fi, minimum-API or broad IME/ABI evidence. All app notes and
its app-private vault are retained.

## Final artifact and checks

Final debug APK: `android/app/build/outputs/apk/debug/app-debug.apk`.
SHA-256: `7b2897d9fbbf46e7267164f573432e3c3b20bf8ba9f219f146210be53fdf0ddb`.
The [recorded Gradle command](android-build.md) selects `gui,lan` and x86_64;
final build passed in 22 seconds, compiling Rust, Java, DEX and APK. Installation
with `adb install -r` and launch succeeded without clearing app data.

The focused regression passed (1 passed, 17 filtered):

```sh
RUSTC_WRAPPER= cargo test --lib --features gui,lan --offline android_input::tests::utf16_boundaries_and_epoch_updates_preserve_source
```

It covers surrogate-interior rejection, exact scalar conversion, complete-state
coalescing, stale epoch rejection, old-field recovery, monotonic epoch exhaustion
and the designated Edit menu's closing-frame authorization. Shared source/domain
tests were not repeated through this adapter.

## Native layout and IME

First launch exposed target-35 edge-to-edge overlap: system bars covered the
Notes toolbar. NativeActivity reported the full screen as its content rectangle.
A live-Activity, coherent WindowInsets snapshot now supplies system/cutout/IME
bounds, intersected with the actual viewport to avoid resize double counting.
Actual toolbar taps, Notes drawer, keyboard visibility, portrait/landscape and
Home/background→resume passed. The editor viewport ends above the keyboard.

Pinned NativeActivity/winit originally delivered ASCII/Return but dropped actual
accent/emoji commits. A framework Editable input mirror now supplies complete
UTF-16 state to the focused Rust field; Rust remains draft/App owner. Actual AOSP
keyboard checks passed:

- Typed `caf`, selected `é` through the held-key accent popup and committed with
  Space. Preedit was an unaccepted draft; commit reached Saved.
- Selected `😀` through the keyboard emoji panel. Backspace removed the entire
  surrogate-pair emoji while preserving café; reinsertion and Return worked.
- Typed composing `zz`, then cancelled it with two keyboard backspaces. Previously
  committed Unicode remained unchanged; cancelled text was not persisted.
- Switched composing `z` to the rename field. The old source draft retained `z`
  and a recovery/conflict notice; the rename field retained `Untitled.md` and did
  not inherit old uncertainty. Explicit Adopt discarded only this test preedit.
- Edit → Select all/Cut/Paste used Android's system clipboard. The Android system
  clipboard overlay displayed the Unicode content; paste restored it. The final
  APK also pasted that clipboard after process replacement, exercising ordered
  snapshot/action delivery rather than egui's internal fallback.

The intermediate popup-focus, epoch-reuse and old-composition handoff findings
were fixed and independently reviewed. Java outgoing snapshots coalesce behind
one main-thread drain, with at most eight ordered jobs; endInput invalidates
queued old work. Resource failure is explicit, never silent truncation.

On the final APK, actual onscreen `ok` plus Space was autocorrected visibly to
`OK`, followed by Return. Saved → force-stop → relaunch → database/WAL verification
retained original note ID `df94c6b1-f27e-4512-b8f2-a5df7fb535aa` and exactly (escaped newlines):

```text
"caf\nAndroid offline\ncafé 😀\nOK \n"
```

There is a trailing space after `OK` and a final newline: 35 UTF-8 bytes, SHA-256
`f2efe33381886d4578cc977b363a5f65da5d08b17540c1cfa24a2e600f6fecbb`.
U+00E9 and U+1F600 were verified explicitly; cancelled `zz` and interrupted `z`
were absent. Both the stopped and reopened database snapshots matched. The other
148-byte synced note and its original ID remained unchanged.

## SAF and foreground sync

Actual ACTION_OPEN_DOCUMENT selected a 105-byte Markdown fixture (Cyrillic,
café, 🌲, list and fence) from Downloads into `Android-source.md`. Notes navigation
showed Unicode source and Saved; preview rendered its derived list/fence.
ACTION_CREATE_DOCUMENT exported the selected canonical snapshot to Downloads.
Pulled export bytes equalled the fixture, SHA-256
`4820bf25598728368c56a00b00f2cdbf9cc58f9e930c4db59be028d8ff63634d`.
Opening a second picker and pressing Android Back reported cancellation without
changing source or revision.

The [cross-OS evidence](android-linux-sync-evidence.md) records explicit GUI sync
migration/listen, ticket verification, Android-initiated authenticated connection,
independent offline edits, rename, pause/explicit resume and exact convergence
with Linux followed by both process restarts. Actual emulator `wlan0` was
`10.0.2.16`; Android connected to host alias `10.0.2.2:43101`. Distinct device
actors and original note IDs survived. No private key was published.

## Limits

No physical phone/Wi-Fi, other IMEs, minimum API 23, arm64 runtime, background
service, concurrent stale-picker UI manipulation or broad provider fault matrix
is claimed. Shared revision validation and adapter source review cover the stale
command boundary separately. One complete emulator workflow satisfied this
representative acceptance gate; it does not establish universal device support.

Transient screenshots remain under `/tmp/tanglewood-emulator-*.png`, including
`system-cut`, `system-paste`, `compose-cancel-before/after`, `composition-handoff`,
`final-paste`, `final-saved` and `final-ime-restarted`. They are session artifacts,
not packaged assets. No emulator shutdown or data wipe was performed.
