# Android IME adapter decision

The API 35 emulator demonstrated ASCII/Enter input, but accented and emoji
commits did not reach the egui editor. Java Activity key dispatch saw no such
events. Cached android-activity 0.6.1 documents NativeActivity's lack of full IME
support; pinned winit 0.30.13 does not route GameActivity TextEvent either.
Changing only the Activity backend therefore does not solve the observed gate.

Keep NativeActivity, egui rendering, the Rust draft owner and shared commands.
Add a small Android input adapter, not another note model or storage writer.
This is required platform accommodation within the approved MVP input scope.

## Input state contract

Prefer a transparent focusable framework EditText proxy with a wrapped native
InputConnection. Android Editable owns UTF-16 selection/composition semantics,
including replacement, empty composition, cursor positioning, code-unit versus
code-point deletion, batches and clipboard actions. Do not implement deletion as
a count of egui Backspace events or turn finishComposingText into duplicate text.
The proxy remains an input mirror; the visible editor is egui.

Rust publishes the focused field's text/selection/composition snapshot with a
widget token and generation. This covers note source and other editable fields,
including paths and public pairing tickets. It is derived UI state, not source
command dispatch from Java. Native updates contain owned complete text state,
UTF-16 ranges, matching token/generation and monotonically increasing sequence.

The owner validates token, generation and sequence before applying a draft delta.
Convert UTF-16 offsets to valid Rust character/byte boundaries; reject surrogate
interiors or malformed text explicitly. A stale update never replaces newer
source or enters a different widget. Retain uncertain text for visible recovery.
Note changes still use the existing expected-revision EditNote path.

Complete-state updates may coalesce to the latest state of one current session;
that state must include all prior input. Bound queue memory and report resource
failure without truncating text. This is not a note-size admission rule. Never
silently fall back through a second key path that duplicates text.

Acknowledged sequence and current generation govern mirror synchronization.
An older Rust echo must not overwrite Java's newer pending Editable state.
External draft/selection changes invalidate or deliberately resynchronize the
old session. Preedit stays unacknowledged as source until committed; cancellation
removes pending composition without deleting unrelated committed text.

## Lifecycle and clipboard

Use the existing activity-scoped live callback discipline. JNI sends owned UI
state only; it never holds App, World or SQLite. Publish under a short lock and
request repaint after unlocking. Teardown unregisters only its matching owner;
old Activity/InputConnection callbacks cannot mutate the replacement editor.

Use Android ClipboardManager for system copy/paste. The pinned egui-winit
Android fallback only stores an internal clipboard and does not satisfy
across-app behavior. Preserve native physical-key input without duplicate soft
input, and retain the reviewed WindowInsets handling.

## Acceptance boundary

Run one representative AOSP keyboard workflow: real onscreen accent/emoji and
multiline input, composition commit/cancel, selection replacement, backspace
across Unicode boundaries, copy/paste, then Saved/force-stop/restart byte equality.
Reuse shared revision/domain checks. Add only focused regressions for conversion,
generation or queue behavior that would otherwise remain uncovered.

SAF already imported/exported the exact 105-byte Unicode fixture, so the observed
missing characters are an input-adapter failure rather than a Markdown/SQLite
encoding failure. Linux/Android synchronization can be exercised independently
while this adapter is implemented. No physical keyboard/IME/device matrix or
rich text editor is added as a gate. Record exactly the keyboard and operations
actually exercised; do not claim general IME compatibility from compilation.


## Delivered evidence

The adapter is implemented in `src/android_input.rs`, the Activity and UI helper
integration. Independent review approved the final state, including monotonic
owner epochs, old-field recovery, designated-menu action ordering, and bounded
Java publication jobs. The focused boundary/session fixture and real API35
x86_64 APK passed; representative AOSP input, system clipboard and exact saved
restart passed. See `android-emulator-evidence.md` for exact bytes/hash and
limitations. This document records the input contract, not a claim of every
Android keyboard/device combination.
