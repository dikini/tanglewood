# Reading Comfort Implementation Plan

> **For Claude:** Use the existing verification workflow for this bounded styling follow-up.

**Goal:** Apply the user-accepted B1 typography: current DejaVu Sans at 16 points, 29-point line height and a 560-point text measure.

**Architecture:** Reuse the existing theme constants and egui wrapping. The text measure excludes responsive gutters. Shared prose and captured-source text use the same cap; source bytes and command behavior remain unchanged.

**Tech Stack:** Existing Rust, egui/eframe 0.33.3 and bundled DejaVu fonts.

---

## Accepted direction

The user preferred B in the native six-way comparison, then accepted either baseline B or B1. Choose B1 to meet the requested 45–75-character reading range more comfortably. Four sample English passages at 16/29 and 560 points averaged 65.2 characters on full lines (57–69); this is a calibration, not a per-line guarantee for arbitrary languages or paragraph endings.

Keep current contrast, kerning, tracking and antialiasing. Code fonts, syntax rendering and heading typography remain unchanged. No new assets, dependencies, preference system, rasterizer, background work or source edits.

## Work

1. `src/ui/theme.rs`: set the text measure to 560 and body line height to 29. Keep the existing body font size of 16.
2. `src/ui.rs`: add responsive gutters and the existing 32-point desktop block-action gutter outside that measure when centering the document.
3. `src/ui/document.rs` and `src/ui/research.rs`: cap body wrapping at the shared measure, retaining available-width wrapping on smaller surfaces. Preserve headings, inline markup, source search and selection.
4. `src/ui/editor.rs`: align ordinary inline paragraph drafts with the reading body size/leading, preserving heading and whole-source editor styling.
5. Remove the temporary `examples/typography_probe.rs` after preserving its source and generated comparisons under `/tmp/tanglewood-reading-comparison`.

## Validation and stopping rule

- Run `cargo test --locked --offline --lib --features gui,codex ui::` once for the combined change; reuse existing source-preservation and editor scenarios.
- Build the native binary with `cargo build --locked --offline --features gui,codex --bin tanglewood`.
- Inspect the actual app with a disposable note vault at regular and expanded desktop widths, checking the centered measure, mixed prose/code and inline editor. Preserve original Markdown bytes.
- Run `cargo fmt --check` and `git diff --check`; review the small diff independently.
- No new tests for constants, domain suite, release-size rebuild or Android build. No new assets/dependencies or runtime loops; mobile/device reading comfort is not established by host evidence.
- Record results here. Leave changes uncommitted until requested.

## Results

- Implemented the five-file styling change. Existing body font remains DejaVu Sans 16; prose and ordinary inline paragraph drafts use 29-point line height. Notes center a 560-point body measure, with page gutters and the existing contextual action alongside it. Shared transcript prose and captured-source text use the same maximum measure; smaller surfaces wrap to available width.
- `RUSTC_WRAPPER= cargo test --locked --offline --lib --features gui,codex ui::`: **35 passed**, 0 failed. Existing exact-source, Unicode, editor, responsive and reader-search scenarios reused; no new tests.
- The native check exposed the separate 32-point block-action allocation while confirming the width budget. After accounting for it outside the measure, `cargo test --locked --offline --lib --features gui,codex responsive_frames_preserve_source_draft_and_use_one_pane_host`: **1 passed**, 0 failed. Other reviewed source remained unchanged.
- `RUSTC_WRAPPER= cargo build --locked --offline --features gui,codex --bin tanglewood`: **passed** after the final layout adjustment. Existing warning debt remains.
- Actual Linux app inspected in a private Xvfb display with an imported disposable note at **1080×1000** and **1440×1000**: matching body wrapping, centered quiet document, mixed inline code and fenced Ash code. F6, Down, Enter opened the first paragraph with matching text rhythm and adjacent Apply/Cancel. Imported canonical Markdown bytes matched exactly after preview, opening the draft and shutdown. [Saved native result](../design/reading-comfort-linux.png).
- Comparison code removed from `examples/`; its source and temporary comparisons remain under `/tmp/tanglewood-reading-comparison`. The initial capture reached an unmapped window; waiting for a visible window corrected the capture script without a product change.
- No new font assets, dependencies, rasterizer, preference storage, background work or repaint loop. Code typography, palette and text antialiasing are unchanged. This does not establish subjective comfort on Android, at other display scales or for other readers; the English line-length range is a calibration.
- `cargo fmt --check` and `git diff --check`: **passed**. Independent Sol xhigh review of the final delta and all three native captures: **READY**, no material findings. User accepted the notes result and authorized a local commit on 2026-10-27; chat event/artifact hierarchy is a separate follow-up.
