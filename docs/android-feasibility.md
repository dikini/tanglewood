# Platform feasibility inventory

Checked 2026-10-26 in the development sandbox. This is prerequisite evidence;
**increment 0b remains unverified**. No APK or Tanglewood window was built or run.
No SDK, global package, Rust target, or toolchain was installed.

## Available and missing prerequisites

| Item | Observed result |
| --- | --- |
| Rust | `rustc 1.94.0 (4a4ef493e 2026-03-02)` |
| Installed Rust targets | `x86_64-unknown-linux-gnu`, `wasm32-wasip1`; no Android target |
| Java | Temurin OpenJDK 21.0.11, runnable from PATH |
| SDK/NDK environment | `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `ANDROID_NDK_HOME` unset |
| Android executables | `adb`, `sdkmanager`, `avdmanager`, `emulator`, `cargo-apk`, `cargo-ndk` absent from PATH |
| SDK directories | No `android-sdk*`, `ndk`, `platform-tools`, `cmdline-tools`, or `system-images` directories found beneath `/opt`, `/usr/local`, or `/home/dikini` to depth 5 |
| Android devices | Not enumerable: adb unavailable; no emulator installed. A physical device was not checked. |
| Emulator acceleration | `/dev/kvm` absent in sandbox |
| Cached platform source | No eframe, egui, winit, android-activity, cargo-apk, or cargo-ndk source directory in the current Cargo registry source cache at inventory time |
| Linux display | Session reports Wayland; `/run/user/1000/wayland-0` and `/tmp/.X11-unix/X0` exist |
| Linux graphics tools | Xvfb, xdotool, glxinfo, vulkaninfo installed; EGL, GL, Wayland-client and xkbcommon runtime libraries present |
| Display access | `DISPLAY=:0 glxinfo -B` and `DISPLAY=:0 xdotool getdisplaygeometry` failed to open the display |
| Isolated display attempt | Xvfb `:97` failed to bind listening sockets under sandbox; GL check therefore failed |

Directory search absence is bounded evidence, not a claim that no SDK exists anywhere
on the host. Socket presence and runtime libraries do not prove application rendering.
No Vulkan device enumeration or Cargo builds were needed for this inventory.

## Candidate version set

Use **Rust 1.94.0 and eframe/egui 0.33.3** as the initial candidate, then pin the
actual successful lockfile/toolchain after the device spike. The official
[0.33.3 workspace manifest](https://github.com/emilk/egui/blob/0.33.3/Cargo.toml)
sets MSRV 1.88 and winit 0.30.12. The tagged
[Android sample manifest](https://github.com/emilk/egui/blob/0.33.3/examples/hello_android/Cargo.toml)
uses a `cdylib` plus `lib`, `glow`, `default_fonts`, and
`android-native-activity`, with minimum API 23 and target API 35.
Use that backend first to reduce packaging variables; acceptable IME behavior
still requires observation. Keep Linux X11/Wayland features target-specific.

The tagged [sample README](https://github.com/emilk/egui/blob/0.33.3/examples/hello_android/README.md)
requires cargo-apk from `https://github.com/parasyte/cargo-apk.git` at revision
`282639508eeed7d73f2e1eaeea042da2716436d5` for its documented upstream workaround.
Do not substitute an unpinned cargo-apk install silently.

For SDK provisioning, API 35, build-tools 36.0.0 and NDK 29.0.14206865 are
**candidate inputs**, not a verified combination: those exact components appear
in the current upstream [Android setup instructions](https://github.com/emilk/egui/blob/master/examples/hello_android/README.md).
That current sample requires Rust 1.95, so copying its entire manifest would
exceed this host's compiler. Reconcile SDK/NDK compatibility with the chosen
packager before installing; preserve the working versions in this document.

## Minimal reproduction after prerequisites are supplied

The packaging owner must first add the Android entry point and metadata to the
single package. Reuse shared application commands and the sole UI-thread ECS;
use bundled SQLite through the existing persistence path. Store the Android
vault in app-private internal storage; do not assume a Linux path or require
external-storage permission for this spike.

1. Set `ANDROID_HOME` and `ANDROID_NDK_ROOT` to the provisioned directories;
   put SDK platform-tools and command-line tools on PATH. Record SDK/NDK,
   packager, device API, ABI, IME and Rust versions.
2. Add only the representative ABI: `rustup target add aarch64-linux-android`
   for an ARM64 physical device, or `x86_64-linux-android` for an x86_64 emulator.
3. Confirm `adb devices -l` reports one authorized target. If using an emulator,
   provision one matching system image and usable acceleration first.
4. Once metadata restricts build targets to that ABI, run
   `cargo apk build --lib --features gui --target aarch64-linux-android`
   (substitute the emulator target when appropriate). Then run
   `cargo apk run --lib --features gui --target aarch64-linux-android`.
   These are planned commands, not results; adapt feature flags to the owned
   manifest if it changes. Record the resulting APK path and install/launch logs.
5. On that device run the acceptance workflow below. An APK build alone does
   not close the platform gate.

| Gate | Required observation |
| --- | --- |
| Input | Multiline source editor opens the software keyboard; enter newlines, Unicode including emoji and composed text; composition commit/cancel does not lose or duplicate source |
| Selection and clipboard | Touch select, extend selection, cut/copy/paste within and across the app; verify exact resulting source |
| Layout | Keyboard does not hide the active edit position; navigation/back dismisses keyboard coherently |
| Lifecycle | Background/resume and rotate/recreate activity; restore editor coherently without a second ECS writer or stale whole-note replacement |
| Storage | Create/edit, wait for explicit persisted acknowledgement, terminate/relaunch, and read identical bytes from app-private SQLite |
| Failure boundary | A pending/unacknowledged edit must not be advertised as saved; record what happens when backgrounding during persistence |

For Linux, run the integrated GUI from an environment that can access its display,
then create/edit/save/restart a note and verify acknowledged content. A temporary
Xvfb display may establish rendering only when socket permissions permit it; it
cannot substitute for interactive IME or Android lifecycle evidence. Current
sandbox display access must be resolved by the PM before claiming a Linux window.

## Inventory commands and results

Read-only commands used: `rustc --version`, `rustup target list --installed`,
`command -v` for the listed tools, `printenv` for SDK/display variables,
`java -version`, the bounded `find` described above, registry directory listing,
and socket/library listings. Tool discovery and optional missing-path probes
returned nonzero when absent, as expected.

Linux probes: `DISPLAY=:0 glxinfo -B` failed with “unable to open display :0”;
`DISPLAY=:0 xdotool getdisplaygeometry` failed to open its display. An ephemeral
`Xvfb :97 -screen 0 1024x768x24 -nolisten tcp` probe failed with “Cannot establish
any listening sockets” and a `/tmp/.X11-unix` ownership diagnostic. The probe
process was cleaned up; no server was left running.

No automated behavioral test applies to this documentation inventory. No Cargo
job was run. Next dependency: provision Android tools/device access and working
Linux display access, then assign Android packaging ownership and execute 0b.

## PM follow-up: Linux display access

`DISPLAY=:0 glxinfo -B` with approved sandbox escalation succeeded on
2026-10-26: direct rendering, NVIDIA RTX 4050, OpenGL 4.6. The earlier failures
were sandbox access failures. Linux window smoke can use an approved escalated
launch after GUI integration; an actual Tanglewood workflow remains outstanding.
Android prerequisite findings are unchanged.

## Provisioning update after user approval

The user accepted Google's SDK license and chose a local emulator. A host probe
with sandbox escalation found `/dev/kvm` and confirmed read/write access; the
earlier missing device was sandbox visibility. The representative target is now
x86_64 Android API 35. Official command-line tools15859902 were downloaded and
SHA-256 checked; API35/build-tools35.0.0/NDK29/platform-tools are being installed
in `/tmp/tanglewood-android-sdk`, with Rust Android targets added. This supersedes
the prerequisite blocker; cross-build and device/emulator behavior still require
actual evidence. No physical-device or wireless test is claimed or added as a
release gate beyond the plan's representative emulator/two-device workflow.
