# MVP-2 Linux integrated evidence

Observed 2026-10-26, isolated Xvfb `:94`, software OpenGL, 1080×760 app window. Source at the current uncommitted MVP-2 increment; final checkpoint/deltas remain in [status](mvp-2-status.md).

## Actual app loop

Built `RUSTC_WRAPPER= cargo build --locked --bin tanglewood --features gui`. Imported the exact pinned fixture through the existing command path:

```sh
target/debug/tanglewood --headless --vault /tmp/tanglewood-mvp2-linux \
  --import /home/dikini/Projects/tanglewood/docs/fixtures/mvp-2/source.md source.md
```

Started the GUI with `WAYLAND_DISPLAY`/`WAYLAND_SOCKET` unset, `DISPLAY=:94 LIBGL_ALWAYS_SOFTWARE=1`. Initial missing-artwork diagnostic was explicit, with source still editable. Selected the fixture resource directory through Document processing. Native preparation displayed the SVG and PNG. Then selected installed Pandoc and configured ordered Lua followed by the actual explicitly located upstream Python preparation stage. The resulting native egui preview displayed the callout quote, three blue `Int` boxes/caption and complete four-panel comic/caption. No HTML/WebView substitute was used.

Opened original source block 7: exact illustration fence, including delimiters and original brief. Inserted `MVP2 check. ` at the start of its first body line. Before Ready, a read-only SQLite query proved the canonical source was still the original 1739-byte fixture, SHA-256 `3920d7763c21cc5bb0959f0ab90b85bfa59846e7cd4912186c97935b17f70efd`.

Clicked Ready. The UI returned to rendered output and Saved. SQLite then contained exactly the original source with that one insertion, all adjacent bytes unchanged: 1751 bytes, SHA-256 `20ca75c58a1b21006a66e441dd58e9a4b162a618dafb9dec7937bbf7a14990c9`. The real configured filter chain refreshed both saved artworks. Editing the brief did not invoke generation or modify artwork.

Reopened that fence, inserted `DISCARD-ME `, and observed Draft · not submitted. Clicked Cancel. SQLite remained byte-identical to the accepted 1751-byte source; the discarded text never reached persistence.

Screenshots retained locally under `/tmp/tanglewood-mvp2-linux-*.png`: `initial`, `controls`, `artwork`, `filtered`, `comic`, `block-edit`, `before-cancel`, `ready`. The original `/tmp/tanglewood-mvp2-linux-accepted.md` is retained for the exact accepted-source comparison. These are local evidence paths, not committed binary screenshot assets.

## Focused checks already run

- Mechanical S0 baseline: `RUSTC_WRAPPER= cargo check --locked --lib --features gui,documents` passed before editor changes.
- Integrated UI controller: `RUSTC_WRAPPER= cargo test --locked --lib --features gui ui::` passed 2 tests, including exact block boundaries/Unicode, no pre-Ready source change, composition/conflict guards and stale revision rejection.
- Actual processing/backend/timeout/cancel evidence is recorded in [processing](plans/mvp-2-processing.md), avoiding another duplicate domain suite.

This record does not prove Android input, arbitrary Pandoc-node presentation, crash recovery for unsubmitted drafts, or asset synchronization. Required changed-platform evidence is separate. Native presentation and input review corrections after the first build require only their affected checks, not a repeat of unrelated backend suites.

## Corrected final build

Rebuilt after the reviewed input/configuration fixes. Reopened the same vault; accepted source remained exactly the 1751-byte saved version. The final GUI displayed the coherent pending-settings notice, applied the resource directory only on Process, and rendered native SVG/comic without exposing leading YAML as document prose (`final-settings.png`, `final.png`). Deliberately applied a nonexistent resource directory: explicit OS error plus “Previous rendering — refresh pending or failed” retained both artworks. A read-only SQLite comparison still matched the accepted source exactly (`final-failure.png`). The renderer does not convert its result back to Markdown.
