# Approved quiet shell implementation record

User approved the interactive Codex-inspired sketch on 2026-10-26. The durable reference is `docs/design/tanglewood-quiet-desktop.html`; the authoritative acceptance rules and implementation ownership are in `docs/plans/2026-10-26-approved-egui-shell.md`. `AGENTS.md` now directs subsequent GUI work to these files.

## Work board

| Lane | Owned paths | State |
| --- | --- | --- |
| Shell/theme | `src/ui.rs`, `src/ui/theme.rs` | done; actual Linux chrome inspected against approved reference |
| Document presentation | `src/ui/document.rs`, `src/ui/processing.rs` | done; verified pencils, coherent last-good image staging, readable missing-artwork fallback |
| Integration/evidence | PM: docs, shared contracts/dependencies/index | host complete; Android acceptance recorded separately |

The HTML sketch is a design reference, not an embedded browser. The application remains native egui/eframe. No new framework, storage subsystem, plugin runtime or sync protocol is part of this pass.

## Acceptance scope

- Actual egui rail/sidebar/header/centered reading surface visibly matches reference.
- Source edit pencils and whole-source affordance preserve exact original source and existing Ready/Cancel/revision/input guards.
- Current note/search/create/rename/trash/restore/mirror/filter/settings/connection/Codex actions remain reachable where supported.
- No permanent numbered block list, raw processing configuration, rename/trash forms or routine explanatory rows in document surface.
- Focus/tooltips/accessible action labels and mobile targets are preserved; errors remain discoverable and actionable.
- Relevant host checks, independent review and affected Android layout/input smoke; record actual evidence rather than assuming host compilation proves Android.

## Host evidence

- `cargo fmt --check`: PASS.
- `RUSTC_WRAPPER= cargo test --locked --lib --features gui,lan,codex --offline ui::`: PASS, 5 tests, 1.23 seconds runtime. Includes source/revision/IME guards, exact original figure bindings, one-in-flight file operation/close/dialog identities, coherent settings and a real valid 2100×2000 PNG exceeding the preview quota while preserving last-good presentation.
- `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,lan,codex --offline`: PASS with nonblocking style/dead-code warnings; no automated formatting/fix sweep.
- `RUSTC_WRAPPER= cargo build --locked --bin tanglewood --features gui --offline`: PASS.
- Independent combined review and affected-risk corrections: READY; no unresolved blocking findings. Checked source/revision/native field identity, pending I/O close/admission, link keyboard reachability, validated mm sizing, coherent staged image failure and LAN polling while settings are closed.

Actual application: isolated Xvfb `:94`, 1080×760 native egui window, software OpenGL, fresh `/tmp/tanglewood-quiet-shell` vault. Four notes were imported through the existing application commands. Selected reference note `Writing with blocks.md`; observed rail, full sidebar selection, filename header, centered reading column, frameless icons and bottom status. No permanent creation/rename/trash forms, numbered edit list or processing configuration in the reader. Saved actual window: [`quiet-egui-linux.png`](design/quiet-egui-linux.png). This is an egui capture, not the HTML sketch.

With resources unset, accepted heading/prose remain readable and artwork becomes a quiet placeholder; explicit footer attention opens detailed error. A contextual pencil opened the exact original heading. Typed ` - native` inside its draft: read-only SQLite comparison proved source unchanged until the check action. Accepted source then matched exactly the original with this one heading insertion: 1846 bytes, SHA-256 `e1103f98b5bf6a3ff35d92b33e4eebfed66b56ffedf9671c29478a72ad7d0092`. A paragraph draft containing `DISCARD ` was cancelled via the cross action; accepted source remained byte-exact. Native cached highlighting and bounded editor actions were visible.

Settings opened from the cog, with normal 18-point dialog title. Explicit fixture resources were applied; actual SVG and comic PNG rendered without decorative frames, with muted captions and no displayed alt-text explanation. The SVG uses neutral paper so dark strokes remain legible; saved artwork bytes are not recolored. The declared 65mm width now yields the expected smaller image, rather than filling the entire reading column. Supporting captures remain under `/tmp/tanglewood-quiet-*.png`.

Android emulator build/install and affected layout/input checks are recorded in `quiet-shell-android-evidence.md`. Unchanged core persistence/sync/filter/IME evidence remains in the existing MVP records; this pass does not claim a new physical-device, assistive-technology or networking matrix. Native reader's existing unsupported syntax fallback is retained; shell styling does not imply new Markdown dialect support.

The relocated Settings → Files action exported accepted source through the actual GUI to `/tmp/tanglewood-quiet-export.md`; its 1846 bytes matched `/tmp/tanglewood-quiet-accepted.md` exactly.

Context-preserving block editing and the reported field-height correction are recorded in `plans/2026-10-26-contextual-block-editing.md`; that later interaction supersedes the initial source-only pane.

## Inline code typography follow-up

User supplied the `match` / `condition` / `=>` paragraph and a ChatGPT compact-highlight reference. Inline code now reserves view-only thin-space padding on both sides and paints rounded, theme-aware backgrounds from the font bounds rather than the 27-point paragraph line box. Source Markdown is unchanged; paragraph wrapping and link hit ranges use the same padded galley character indices. Native Linux inspection of the supplied paragraph plus Unicode and a following link passed; capture `design/inline-code-spacing-linux.png`. Six focused UI tests and the Linux GUI binary build passed. This small rendering-only change was not rebuilt or rechecked on Android.
