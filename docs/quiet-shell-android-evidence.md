# Quiet-shell Android smoke — 2026-10-26

The reviewed quiet egui shell was built and installed as an actual APK on the retained AOSP API 35 x86_64 emulator `emulator-5580` (720×1280, density 280, KVM/software GPU). No app data was cleared; no production source or index edits occurred during this smoke. This is representative emulator evidence, not physical-phone or broad IME/API/ABI coverage.

## Artifact and command

From `android/`:

```sh
env RUSTC_WRAPPER= ANDROID_HOME=/tmp/tanglewood-android-sdk \
  ANDROID_NDK_ROOT=/tmp/tanglewood-android-sdk/ndk/29.0.14206865 \
  GRADLE_USER_HOME=/tmp/tanglewood-android-gradle \
  ./gradlew :app:assembleDebug -PandroidAbi=x86_64 --no-daemon
```

**BUILD SUCCESSFUL in 31s**; actual Rust Android `gui,lan` library compiled in 13.55s, followed by Java/DEX/APK packaging. Expected unused/style/platform-only warnings, no errors. APK `android/app/build/outputs/apk/debug/app-debug.apk`, SHA-256 **`af9f859d30443baec54b7eab27733d537113a6e4fac8bba7345973e60e9b4a04`**. Installed with the existing SDK `adb -s emulator-5580 install -r`; Activity launched explicitly. ADB used `ANDROID_USER_HOME=/home/dikini/Projects/tanglewood/target/android-user`.

Screenshots and complete read-only SQLite/WAL snapshots are retained under `/tmp/tanglewood-quiet-android/`.

## Observed checks

- **Quiet mobile layout:** `01-start.png` shows the narrow rail, filename header with source/menu icons, centered prose and contextual pencils, plus quiet bottom status. No permanent search/create/rename/trash form, debug block list or processing controls. `03-drawer.png` shows a full-width mobile Notes drawer with full-width subtle selected row and contextual New icon; selecting a note returns to the document.
- **Settings:** cog opens actual processing fields directly (`02-settings.png`), including explicit local-artwork configuration and backend limitation. No fake configuration success was asserted. With no resource directory applied, `06-fixture-fallback.png` shows readable heading/prose/callout, quiet Artwork unavailable placeholders, original-source pencils and an explicit Preview needs attention status. This is fallback, not successful processed artwork; unchanged image proof is in the earlier [MVP-2 Android record](mvp-2-android-evidence.md).
- **New/Rename lifetime:** opened New, focused its native field (`04-new-dialog.png`, `05-new-ime.png`), tapped an on-screen `q`, then closed without creating. Opened Rename for the selected fixture; `08-rename-target.png` shows correct `mvp2-source.md`, not the discarded New input. Focused and closed Rename without submitting. No extra note or rename occurred. This small device check supplements the reviewed per-instance/target IDs and existing stale-widget/epoch regressions; it is not an exhaustive callback-race matrix.
- **Contextual source and Cancel:** heading pencil opened exact original `# Saved artwork preparation` source (`10-original-editor.png`). With AOSP IME visible, the bounded source area and Cancel/Ready edge remained reachable (`11-editor-ime.png`). On-screen `z` plus space made a local Draft (`12-local-draft.png`); one Cancel returned reading mode (`13-cancel.png`). Complete SQLite/WAL comparison after these dialog/Cancel actions matched **all three baseline notes**, including revisions, paths and trash flags.
- **Ready:** paragraph pencil opened original `hi ` source (`14-paragraph-original.png`); actual on-screen `q` plus space was accepted with the contextual Ready icon (`16-ready-draft.png`, `17-ready-reading.png`). Persisted source changed by exactly those two bytes, with all adjacent bytes retained. This is a real APK edit, not a DB write or native CLI harness.

## Test-note outcome and limits

Only the fresh previously owned fixture note **`mvp2-source.md`**, UUID `9bcdc027-4304-4d6b-8626-395a23615e61`, was edited. Its baseline was 1742 bytes. The literal `q ` insertion at the editable paragraph boundary joined the following fence onto that line, visibly changing Markdown structure; the source editor does not invent a separating newline. An attempted explicit newline placement subsequently inserted at the active span's end rather than the intended fence boundary. **The intended source repair was not completed, and fine-grained caret placement in that attempt is not claimed as passing.** No production fix or additional input matrix was attempted in this bounded smoke.

The final accepted test note is **1745 UTF-8 bytes**, revision `ae464c2d-903d-4e30-8fce-271c7e83b8e3`, SHA-256 **`540c336d0abedcc20bb23387404289e4d33d19c8259c4234567aeeaded4e689c`**. Full local source is `/tmp/tanglewood-quiet-android/final-source.md`; `final.tar` contains the SQLite/WAL snapshot. Compared to baseline, only `q ` at the paragraph boundary and one newline after `Only original source is editable.` were added. The note is a changed raw-Markdown test fixture, not a claim of successfully processed artwork at that final revision. Its missing-artwork/processing status remains explicit.

Both older notes stayed byte-for-byte unchanged, with their IDs/revisions/paths/trash flags preserved and total note count still three:

| Note | UTF-8 bytes | SHA-256 |
| --- | ---: | --- |
| `Untitled.md` | 35 | `f2efe33381886d4578cc977b363a5f65da5d08b17540c1cfa24a2e600f6fecbb` |
| `Linux-reconnected.md` | 148 | `85d61ec2b1995714b960bb88a122c8e80a20cf3b6e8c6dd8a52b9899458c21d6` |

The app/emulator and staged local fixture resources remain retained. No unsubmitted-draft crash recovery, new LAN/provider/clipboard matrix or arbitrary Android Pandoc support is implied. Existing unchanged-platform evidence is reused; actual Linux visual comparison and review are recorded separately in the root-owned quiet-shell evidence.
