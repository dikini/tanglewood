# Android build and launch

The NativeActivity, app-private vault, Storage Access Framework picker, mobile
editor layout and foreground LAN controls are integrated in source. **An Android
cross-build, APK installation and device workflow have not passed here.** The user has authorized SDK provisioning and selected a local KVM emulator;
installation is underway.
See [platform inventory](android-feasibility.md) and [picker details](android-picker.md).

## Packaging

Use the Java-capable `android/` Gradle project. A pure-native cargo-apk invocation
does not package `TanglewoodActivity.java` and cannot deliver this picker.
The project still builds the existing single Rust package's library.

Candidate pins: Rust 1.94.0, eframe 0.33.3, JNI 0.22.4, Gradle 8.11.1,
Android Gradle Plugin 8.9.2, API/build-tools 35/35.0.0, NDK 29.0.14206865,
minimum API 23, Java 17 or compatible newer runtime. These Android combinations
need an actual build; wrapper checks and host compilation do not establish that.

Once the SDK/NDK and one authorized target are available, on Linux:

```sh
export ANDROID_HOME=/absolute/path/to/android-sdk
export ANDROID_NDK_ROOT="$ANDROID_HOME/ndk/29.0.14206865"
rustup target add aarch64-linux-android
cd android
./gradlew :app:assembleDebug
adb devices -l
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n org.tanglewood.Tanglewood/org.tanglewood.TanglewoodActivity
adb logcat -s Tanglewood
```

The script builds `--locked --lib --features gui,lan` for arm64-v8a, links bundled
SQLite with the selected NDK and packages `libtanglewood.so`. For one x86_64
emulator, install `x86_64-linux-android` and pass `-PandroidAbi=x86_64` to Gradle.
No ABI matrix or release signing is required. The first Gradle invocation needs
network access for its checksum-pinned distribution and dependencies. SDK tools
require [Google's SDK license acceptance](https://developer.android.com/studio). SDK provisioning began after separate user approval; see the inventory update.

## Ownership and lifecycle

Winit 0.30.13's AndroidApp is handed to eframe; there is no second event loop or
ECS owner. `android_main` opens `<internal_data_path>/vault`; startup/lock errors
never fall back to another writer. Picker transfer files are app-private.
No external-storage permission is requested; the system picker grants individual
URI access. INTERNET is declared for explicitly enabled foreground LAN.

The editor starts full-width, with a Notes drawer and collapsed Documents/Sync
controls. Picker results dispatch shared local commands with captured revision
checks. Export reports its selected canonical snapshot separately from SQLite
Saved, and pending draft changes are excluded. See picker documentation for
cancellation and provider failure behavior.

One optional Tokio runtime supports foreground transport. Focus loss requests
PauseLan, which immediately drops active/pending controllers; resume is explicit.
Runtime teardown uses nonblocking shutdown. Focus loss is best effort rather than
an Android lifecycle guarantee. Winit/eframe control rendering suspension; no
background service is supplied. Bounded App ticks submit edits continuously;
there is no blocking lifecycle flush. Wait for SQLite Saved before termination.

## Evidence still required

Host combined GUI/Codex/LAN compilation passed, and a temporary host JNI API type
check passed. Neither compiles Android-specific UI/entry code or Java. Wrapper
hash, shell syntax, XML and missing-tools preflight checks passed; no Gradle/APK
result is claimed.

Run the representative device workflow in the inventory: Unicode/composed IME
input, selection/clipboard, keyboard layout, drawer composition recovery,
background/resume/rotation, acknowledged SQLite restart, SAF import/cancel/stale
replacement/export/provider failures, and paired foreground LAN reconnect.
Record device/API/ABI/IME/provider and actual results before closing those gates.
