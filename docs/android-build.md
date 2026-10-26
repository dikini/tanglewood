# Android packaging preparation

Status: entry-point source prepared; **not Android-build-verified or device-tested**.
See [the prerequisite inventory](android-feasibility.md). This environment has no
Android SDK/NDK, Android Rust target, adb or emulator. Host checks cannot validate
`cfg(target_os = "android")` code, native linking, APK metadata, input or lifecycle.

## Source integration

`src/android.rs` exports the Rust-ABI `android_main(AndroidApp)` entry expected by
android-activity. It uses `winit::platform::android::activity::AndroidApp` rather
than independently selecting an activity crate version. Cached winit 0.30.13
reexports android-activity 0.6.1. Cached eframe 0.33.3 `NativeOptions::android_app`
is the supported event-loop handoff; do not call `poll_events` a second time.

The entry opens `<AndroidApp.internal_data_path()>/vault` with `App::open`, then
runs the same `TanglewoodUi`. Storage/lock/startup failures log an error and do
not silently create an alternate vault. Android logcat tag is `Tanglewood`.

The wrapper delegates all editing/IME behavior to shared UI. It ticks the one App
again after each frame to submit accepted changes promptly, and makes bounded
best-effort ticks on focus loss and exit. Focus loss is not an Android suspend
callback. Winit/eframe own activity lifecycle and rendering suspension; no second
ECS, storage worker, event loop or background service is introduced. No blocking
flush runs in these callbacks. Persisted acknowledgements are still the durability
boundary; OS termination can lose unacknowledged edits. Suspend/resume/restart and
IME behavior remain device gates.

PM-owned manifest/module integration:

```toml
[lib]
crate-type = ["lib", "cdylib"]

# Add within existing Android-only dependencies. eframe's Android feature set
# is default_fonts + glow + android-native-activity, optional behind gui.
[target.'cfg(target_os = "android")'.dependencies]
winit = { version = "0.30.13", default-features = false, features = ["android-native-activity"] }
android_logger = "0.15.1"
log = "0.4"

[package.metadata.android]
package = "org.tanglewood.Tanglewood"
apk_name = "tanglewood"
build_targets = ["aarch64-linux-android"]

[package.metadata.android.sdk]
min_sdk_version = 23
target_sdk_version = 35

[package.metadata.android.application]
label = "Tanglewood"
```

In `lib.rs`: `#[cfg(all(target_os = "android", feature = "gui"))] mod android;`.
Keep Linux X11/Wayland dependencies target-specific and use only one Android
activity backend. Packaging must enable `gui` and select the library target.
The manifest fragment is integration guidance, not a claim that metadata was
validated by cargo-apk.

## Reproduction once tools are provided

Use the candidate provisioning versions and pinned cargo-apk workaround from
[android-feasibility.md](android-feasibility.md), recording what actually works.
For an ARM64 device:

```sh
rustup target add aarch64-linux-android
# Set ANDROID_HOME and ANDROID_NDK_ROOT to the actual installed directories.
adb devices -l
cargo apk build --lib --features gui --target aarch64-linux-android
cargo apk run --lib --features gui --target aarch64-linux-android
adb logcat -s Tanglewood
```

For a single x86_64 emulator, change the installed Rust target, manifest
build_targets and both cargo-apk commands to `x86_64-linux-android`. Do not build
four ABIs by default. Inspect packaged manifest and APK install/launch output;
then execute the input, selection/clipboard, keyboard layout, lifecycle and
acknowledged SQLite restart workflow in the inventory. Retain one real result
with device/API/IME/ABI and exact versions before closing increment 0b.

## Required document picker boundary — still unimplemented

Android import/export must use the Storage Access Framework system picker.
[Android's document-storage guide](https://developer.android.com/training/data-storage/shared/documents-files)
describes `ACTION_OPEN_DOCUMENT` and `ACTION_CREATE_DOCUMENT`, returning a URI
for the user-selected document. This avoids broad storage permission. A content
URI must be accessed through ContentResolver, not treated as a filesystem path.

A viable implementation is a small Java/Kotlin subclass of NativeActivity with
an activity-result callback, JNI request/result bridge, and packaged Java code.
[NativeActivity](https://developer.android.com/reference/android/app/NativeActivity)
is an Activity and supports that subclass boundary. Native android-activity
exposes VM/activity pointers for JNI; it does not provide a document-picker
result API. The pure-native cargo-apk spike therefore needs an explicit build
step for Java glue (or a consciously chosen packaging migration) before picker
support can ship. Merely starting an intent does not collect its result.

The bridge should run picker intents on Android's main thread, match bounded
request IDs, handle cancellation as cancellation, and return a granted URI or
opened descriptor to an I/O worker. Import decodes source, then sends the same
validated create/edit command on the sole App owner; replacement captures and
checks the expected revision. Export snapshots the requested note revision and
writes that exact source through ContentResolver, reporting write failure
separately from database save status. Do not auto-persist filtered output.

Keep grants only as long as needed for explicit one-shot import/export; request
persistable URI access only if a later workflow requires it. A picker cancellation
or inaccessible provider leaves the note unchanged. URI display names remain
untrusted and must pass the shared vault-relative path rules. There is no fake
picker, path-entry substitute, external-storage mirror or picker success claim
in this increment. This remains required Android local-editor work after the
platform spike.

## Validation recorded for this increment

Read cached eframe 0.33.3 (`NativeOptions::android_app`, `App::raw_input_hook`,
`App::on_exit`), winit 0.30.13 Android module and android-activity 0.6.1
`internal_data_path` APIs. Formatted only `src/android.rs`; no APK build, Android
compiler check, device run or SDK installation was performed. The PM owns final
manifest integration and any host regression check; those checks still do not
validate this Android-only module.
