# MVP-2 integrated Android acceptance

Status: **passed the reduced representative Android slice on 2026-10-26**. Actual integrated APK build/install, SAF fixture import, native SVG/comic display, explicit missing-artwork failure, source-backed Ready/Cancel and acknowledged exact-byte restart passed. Initial image evidence was reused after the independently reviewed input-transition correction; no broad device/input matrix was run.

## Environment and artifacts

Existing private AOSP API 35 x86_64 emulator `emulator-5580`, 720×1280, density 280, software GPU/KVM. Existing app data and notes retained. Physical phone/Wi-Fi, other IMEs/APIs/ABIs and arbitrary external Android filters are not covered.

Executed from `android/`:

```sh
env RUSTC_WRAPPER= ANDROID_HOME=/tmp/tanglewood-android-sdk \
  ANDROID_NDK_ROOT=/tmp/tanglewood-android-sdk/ndk/29.0.14206865 \
  GRADLE_USER_HOME=/tmp/tanglewood-android-gradle \
  ./gradlew :app:assembleDebug -PandroidAbi=x86_64 --no-daemon
```

`BUILD SUCCESSFUL in 41s`; actual Rust Android `gui,lan` (GUI includes documents), Java/DEX and APK packaging, not a host CLI substitute. First APK `android/app/build/outputs/apk/debug/app-debug.apk` SHA-256 `12261a62dfdc53c95a3267cb362dc6d770b3b4df1b934cbdb792b4c11fa37e62`. Installed with `adb install -r`; no app-data clear. Warnings: SDK XML parser version, unused imports and existing `durable_history`.

The same command rebuilt the final reviewed delta: **BUILD SUCCESSFUL in 24s**, Rust target build 9.41s. Final installed APK SHA-256 **`b8ac724774377fd7c6ccd530cdf2e8ae8b098e3be767a582a096102010b23724`**. This supersedes the first APK hash above. Final warnings were unused `Ordering` and existing `durable_history`; no compile errors. Source remained frozen during acceptance.

ADB used the existing SDK binary with `ANDROID_USER_HOME=/home/dikini/Projects/tanglewood/target/android-user`, serial fixed to `emulator-5580`. Screenshots and SQLite/WAL snapshot evidence are retained under `/tmp/tanglewood-mvp2-android/`.

## Observed integrated workflow

- Pushed repository `docs/fixtures/mvp-2/source.md` to Downloads, then used the **real system SAF picker** to import a new root note `mvp2-source.md`, UUID `9bcdc027-4304-4d6b-8626-395a23615e61`. Saved displayed; force-stop/relaunch and a read-only SQLite/WAL snapshot confirmed exact **1739 UTF-8 bytes**, SHA-256 `3920d7763c21cc5bb0959f0ab90b85bfa59846e7cd4912186c97935b17f70efd`.
- Before selecting resources the app explicitly reported `illustration preparation: missing saved artwork: assets/fig-add-inputs/illustration.json`; original source edit actions remained available (screenshot `16-fixture.png`). No silent successful placeholder.
- Staged only fixture resources with `adb exec-in run-as ... tar`, into `/data/user/0/org.tanglewood.Tanglewood/files/mvp2/assets`. Entered that explicit resource root under Document processing and chose Process / refresh artwork. The **actual APK** displayed the three blue Int-square SVG and function-signature caption, and the four-panel PNG comic/caption (`21-svg.png`, `22-svg-caption.png`). Native Android subset, local resources; no arbitrary Pandoc/Lua claim and no artwork sync claim.
- Opened exact original heading block through the native source outline. Highlighted source editor showed `# Saved artwork preparation`; on-screen AOSP keyboard input changed only the local draft. A complete SQLite/WAL snapshot before submission still matched the original fixture byte-for-byte. This preliminary check does not close the transition/Ready gate below.
- Existing notes remained exact: `Untitled.md` UUID `df94c6b1-f27e-4512-b8f2-a5df7fb535aa`, 35 bytes, SHA-256 `f2efe33381886d4578cc977b363a5f65da5d08b17540c1cfa24a2e600f6fecbb`; `Linux-reconnected.md` UUID `f5012443-5ae3-40d5-aa2b-cf1ebd51957a`, 148 bytes, SHA-256 `85d61ec2b1995714b960bb88a122c8e80a20cf3b6e8c6dd8a52b9899458c21d6`.

## Final affected Ready/Cancel and restart gate

The final binary includes the independently reviewed correction: normal transitions drain pending native input into its owning draft under the recorded text guard, uncertain transitions pause with retained conflict, unchanged Ready uses the same guard, close drains pending input, and explicit Cancel discards composition before adoption. The existing pure transition regression and root's coherent checks supplement, rather than replace, this actual-device check.

1. Opened the fixture's **original heading source block** from its accepted native source outline. The editor displayed exact original source, with cached syntax colors and the real AOSP keyboard. Scrolling the source heading into the keyboard-visible area was necessary with the compact viewport; no custom input injection or source DB writes were used.
2. **Cancel:** tapped actual on-screen `x` and space, committed input and hid the keyboard. UI showed the changed heading `preparationx ` and **Draft · not submitted** (`43-cancel-local.png`). Complete read-only SQLite/WAL snapshots before Cancel and after one Cancel remained exactly the original 1739-byte fixture (`final-unsubmitted.tar`, `final-cancel.tar`). Cancel returned the source outline (`44-cancel-complete.png`); no second press was needed and no source was submitted.
3. **Ready:** reopened the same original block, tapped actual AOSP `h`, `i`, space, committed and hid the keyboard. The local block buffer became `# Saved artwork preparation\nhi ` (`47-ready-draft.png`). Pressed Ready once. It returned to reading mode and displayed **Saved** (`49-acknowledged.png`). The resource setting was intentionally empty in this restart session, so the independent missing-artwork diagnostic remained explicit; it did not block source durability.
4. **Exact source and restart:** compared the complete accepted source with repository `source.md`, allowing only insertion of the three UTF-8 bytes `hi ` immediately after the original heading's newline. All original surrounding bytes, metadata and fences remained exact. After Saved, force-stopped and relaunched the app. A fresh complete SQLite/WAL copy matched the accepted source exactly. Fixture UUID remains `9bcdc027-4304-4d6b-8626-395a23615e61`, accepted revision `15eca108-f90d-4187-99e6-22485525c12c`, **1742 bytes**, SHA-256 **`2365c848ae53e7709b9f80fcda7418684fad36113068c467b7905f20f16c87de`**. Source retained at `/tmp/tanglewood-mvp2-android/final-ready-source.md`; snapshots `final-ready.tar` and `final-restarted.tar`.
5. Compared both pre-existing note IDs/path/text against the baseline again after restart: unchanged hashes/lengths listed above. App data, staged local artwork and all notes remain available for user trial.

## Limits and retained evidence

No production source changes occurred during final acceptance. No artwork generation, external arbitrary Android Pandoc/Lua, artwork synchronization, physical phone/Wi-Fi, assistive-technology or broad IME/API/ABI claim follows. Unsubmitted drafts remain memory-only; no crash recovery is claimed for them. The resource directory/backend settings apply explicitly through Process / refresh artwork and are session-local; they must be selected again after restart. Initial source-field routing diagnostics from exploratory path/shortcut input are not presented as final editor success; the final scoped on-screen Ready/Cancel workflow completed without those errors.

Actual artwork proof is screenshots `21-svg.png`/`22-svg-caption.png` from the initial integrated binary; this display path was reused rather than rerunning unchanged image acceptance after the input-only review fixes. Final Ready/Cancel/restart proof is from the final APK above. The root's processing/settings and Linux checks cover their separately recorded changes. There was no additional provider, sync, clipboard or lifecycle matrix; existing unchanged-platform evidence remains in [MVP Android evidence](android-emulator-evidence.md).
