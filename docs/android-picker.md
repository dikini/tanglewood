# Android Storage Access Framework preparation

Status: real picker/ContentResolver/JNI and Gradle packaging source prepared;
**not Android compiled, APK installed, or device tested**. The user has since approved temporary SDK/NDK provisioning and selected a KVM
emulator; installation is underway. The earlier
[platform inventory](android-feasibility.md) and [entry preparation](android-build.md)
remain prerequisite evidence, not an Android readiness claim.

## Concrete picker and ownership boundary

`android/app/src/main/java/org/tanglewood/TanglewoodActivity.java` subclasses the
platform NativeActivity. It launches ACTION_OPEN_DOCUMENT or
ACTION_CREATE_DOCUMENT on Android's main thread with CATEGORY_OPENABLE. The
returned content URI is opened by ContentResolver directly, never converted to
a filesystem path. No external-storage permission or persistable URI grant is
requested. The normal INTERNET permission is present for future foreground LAN
sync, independently of document access.

One pending picker/transfer and a single bounded Java executor own all provider
I/O. AutoClose input/output streams own their ParcelFileDescriptor, including
error cleanup. Activity destruction/cancellation sets CancellationSignal,
closes an active descriptor and interrupts executor work. A late Activity result
retains the original request identity until cancellation returns; it cannot be
mistaken for a new picker using the same Android request code. Completed work
reports exactly once. Stale native owner/request IDs are ignored.

The native `AndroidPicker` creates its own Activity global reference using the
live AndroidApp VM/Activity handles. It exposes:

- `import_create(path)` captures the intended validated vault-relative target.
- `import_replace(&NoteView)` captures note ID, expected revision and original
  source byte count before opening the picker.
- `export(&NoteView)` transfers the exact selected source snapshot, note ID and
  revision; subsequent edits do not alter that export payload.
- `cancel()` retains the pending identity until its result arrives.
- `poll()` returns ImportReady with an ordinary shared CreateNote/EditNote
  command, Exported for the captured revision, Cancelled, or Failed.

The Java I/O worker streams imports into an app-private temporary spool. JNI
passes only completion metadata through a capacity-one channel; one Rust decoder
worker validates UTF-8 off the UI thread. Neither worker owns App/World or applies
edits.
The existing owner must dispatch ImportReady with Caller::Local; expected-revision
and source/path/CRDT validation remain shared. An edit arriving while the picker
is open causes replacement to reject as stale. Cancellation and failure never
modify canonical source. The originating external document is not rewritten.

Export success is a separate selected-snapshot outcome, not a SQLite Saved
acknowledgement. CREATE_DOCUMENT creates a new user-selected document; Android
may adjust a colliding name. Cancellation/failure after document creation can
leave an empty/partial external document and is reported as failure/cancellation,
never success. There is no blind retry or automatic overwrite of an existing URI.

Provider I/O and export JNI arrays use 64 KiB chunks; queue capacities bound
in-flight operations rather than admitted note sizes. Imports have no arbitrary
size cap or silent truncation. The shared String-based command still requires one
complete decoded source allocation; disk/allocation errors and invalid UTF-8 fail
explicitly without changing source. Export reads chunks from an immutable Rust
snapshot. Normal completion/cancellation cleans private spools on the worker;
process death or an unavailable decoder can retain a private spool for recovery.
Provider error messages are bounded separately. No path-entry field substitutes
for the real picker.

## Entry and owner integration

Keep one Rust package. The Android-only JNI dependency is exactly
`jni = "=0.22.4"`, optional through the GUI feature, matching cached APIs.
The shared library must declare `android_picker` for Android GUI builds.

The Android entry now constructs the picker from the same live AndroidApp handles,
passes `internal_data_path()/picker-transfer` as the private spool root, and attaches
an egui repaint callback. `TanglewoodUi` owns the picker and polls completions.
Import commands execute directly through the existing App with Caller::Local;
this avoids the ordinary dispatch helper's selection changes. Dirty drafts remain
retained and become conflicts when the canonical revision changes. Replacements
still use the revision captured before opening the system picker.

Android starts with the editor filling the screen. The Notes toggle opens the
note drawer; selecting/creating a note or following a link closes it. The editor
is hidden while this drawer is open. Document controls start collapsed under
Documents to preserve phone height.

The Android toolbar exposes real document selection for create/import and replace,
canonical selected-snapshot export, cancellation, and explicit completion/errors.
The import path field chooses a vault destination; it never substitutes for a
provider picker or converts a URI into a path. Export explicitly excludes local
unaccepted drafts and labels the captured canonical revision. Its completion is
independent of SQLite Saved. Pending operations request bounded polling repaints.

This integration is source preparation only. The new module, entry and Android
UI branches still require an Android target build and runtime/provider checks.

## Java-capable APK packaging

The new `android/` project is the Java-capable path required for the Activity
subclass. The previous pure-native cargo-apk commands do not include this class
and therefore cannot deliver picker support. Gradle packages the same Rust
library; it does not introduce another Cargo package.

Candidate pinned versions:

| Component | Candidate |
| --- | --- |
| Android Gradle Plugin | 8.9.2 |
| Gradle wrapper | 8.11.1 |
| JDK | 17 minimum; installed JDK 21 is a candidate runtime |
| compile/target SDK | 35 |
| build-tools | 35.0.0 |
| NDK | 29.0.14206865, same candidate as the platform inventory |
| min API | 23 |
| Rust/eframe | Installed 1.94.0 / pinned 0.33.3 |
| Representative ABI | arm64-v8a; explicit x86_64 emulator alternative |

[Official AGP 8.9 compatibility](https://developer.android.com/build/releases/agp-8-9-0-release-notes)
specifies Gradle 8.11.1, build-tools 35.0.0, JDK 17 and maximum API 35. Its default
NDK differs; our explicit NDK is a candidate override and needs a real cross-build.
The [official SAF guide](https://developer.android.com/training/data-storage/shared/documents-files)
describes the system picker, URI grants and ContentResolver I/O used here.

The wrapper JAR and scripts are original Gradle v8.11.1 artifacts. The JAR is
43,583 bytes with published SHA-256
`2db75c40782f5e8ba1fc278a5574bab070adccb2d21ca5a6e5ed840888448046`.
The wrapper properties pin the official distribution SHA-256 as well. Only these
small wrapper artifacts/checksums were downloaded; the Gradle distribution,
AGP, SDK and NDK were not downloaded or installed. Wrapper scripts retain their
upstream license notices.

After provisioning the tools and declaring the module, on Linux:

```sh
export ANDROID_HOME=/absolute/path/to/android-sdk
export ANDROID_NDK_ROOT="$ANDROID_HOME/ndk/29.0.14206865"
rustup target add aarch64-linux-android
cd android
./gradlew :app:assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n org.tanglewood.Tanglewood/org.tanglewood.TanglewoodActivity
```

The wrapper's first run downloads its pinned Gradle distribution and AGP needs
repository access; those downloads are not performed by this preparation. An
x86_64 emulator build uses `-PandroidAbi=x86_64` and the corresponding installed
Rust target. The build script chooses one ABI, runs the existing package's
`cargo build --locked --lib --features gui,lan --target ...`, supplies the NDK
compiler/linker to bundled SQLite, and copies libtanglewood.so to generated
jniLibs. Debug builds use Gradle's normal local debug signing. No release signing
key or external publication is configured.

## Validation and remaining gates

Recorded preparation checks: official wrapper JAR hash matches its published
checksum and contains GradleWrapperMain; `bash -n android/gradlew
android/build-rust.sh` passed; the manifest parsed as well-formed XML. Native
preflight exited 2 with an explicit missing Android tools message before running
Cargo. A temporary Linux crate re-exporting the shared request types and
path-including `src/android_picker.rs`, with `jni = "=0.22.4"`, passed:

```sh
RUSTC_WRAPPER= cargo check --manifest-path /tmp/tanglewood-picker-jni-check/Cargo.toml --target-dir /home/dikini/Projects/tanglewood/target --offline
```

This checks actual cached JNI Rust APIs and shared command types, including the
streaming snapshot and worker boundary. It does not compile an Android target.
These checks do not prove Gradle configuration, Java compilation, JNI runtime,
APK linking or Android provider/lifecycle behavior.

Required device workflow: pick/import a Unicode Markdown document; cancel without
source changes; edit the destination while the picker is open and observe a stale
replacement rejection; export an exact captured snapshot while further edits
remain independently dirty/saved; handle permission/provider errors and invalid
UTF-8 without source mutation; rotate/background/resume/destroy during picker and
I/O; verify descriptor cleanup and app-private SQLite restart durability. Retain
one representative device/API/IME/provider result before closing the Android gate.

## Foreground LAN controls

The candidate Android APK selects `gui,lan` in the existing package. The shared
UI exposes a collapsed Sync section with explicit migration, manual listener
address/advertised IP, ticket exchange and confirmation, fingerprint connect/unpair,
status and pause/resume controls. No action automatically enables sync or trusts
a ticket. Async migration and pairing persistence status remain separate from
SQLite note Saved.

Android retains one Tokio runtime (one worker) for the same App owner. Focus loss
requests PauseLan and ticks existing workers; returning to the app requires user
Resume. Exit requests pause as best effort. There is no background service or
automatic listening. This is source preparation, not Android networking/device
evidence. Opening Notes during IME composition retains uncertain draft text as
a conflict before hiding the editor.
