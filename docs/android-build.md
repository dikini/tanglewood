# Android build and launch

The NativeActivity, app-private vault, SAF picker, mobile editor, native Editable
IME bridge and foreground LAN controls passed one AOSP API 35 x86_64 emulator
workflow, including installation, Unicode input/clipboard, SAF round trip,
restart durability and Android/Linux foreground sync. See
[emulator evidence](android-emulator-evidence.md) and [picker details](android-picker.md).
Physical phone/Wi-Fi and other ABI/API/IME combinations remain unverified.

## Packaging

Use the Java-capable `android/` Gradle project. A pure-native cargo-apk invocation
does not package `TanglewoodActivity.java` and cannot deliver this picker.
The project still builds the existing single Rust package's library.

Build pins: Rust 1.94.0, eframe 0.33.3, JNI 0.22.4, Gradle 8.11.1,
Android Gradle Plugin 8.9.2, API/build-tools 35/35.0.0, NDK 29.0.14206865,
minimum API 23, Java 17 or compatible newer runtime. This combination passed the x86_64 build and representative emulator workflow;
other ABI/API/IME runtime combinations remain unverified.

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

## Build and runtime evidence

On 2026-10-26, the initial authorized temporary SDK build passed with no source fixes:

```sh
cd android
env RUSTC_WRAPPER= ANDROID_HOME=/tmp/tanglewood-android-sdk ANDROID_NDK_ROOT=/tmp/tanglewood-android-sdk/ndk/29.0.14206865 GRADLE_USER_HOME=/tmp/tanglewood-android-gradle ./gradlew :app:assembleDebug -PandroidAbi=x86_64 --no-daemon
```

The initial cross-build reported BUILD SUCCESSFUL in 1m 26s; the final input-adapter
build reported BUILD SUCCESSFUL in 22s. This compiled the Android Rust
`gui,lan` library, Java Activity, DEX and debug APK. The APK contains
`lib/x86_64/libtanglewood.so` and the Activity DEX. Output:
`android/app/build/outputs/apk/debug/app-debug.apk`, SHA-256
`7b2897d9fbbf46e7267164f573432e3c3b20bf8ba9f219f146210be53fdf0ddb`.
Warnings were an SDK XML parser-version mismatch and unused `durable_history`;
neither blocked the build. Installation and the representative emulator workflow subsequently passed;
see the linked emulator evidence for exact operations and final source hashes. Earlier host JNI/wrapper checks remain
useful preparation evidence, separate from this actual target build.

The representative emulator workflow exercised: Unicode/composed IME
input, selection/clipboard, keyboard layout, drawer composition recovery,
background/resume/rotation, acknowledged SQLite restart, SAF import/cancel/export,
and paired foreground LAN reconnect. Concurrent stale-picker manipulation and
provider fault injection were not performed; shared revision validation was checked separately.
The linked evidence records API/ABI/IME/provider and results; no physical-device
or broad compatibility claim follows from it.
