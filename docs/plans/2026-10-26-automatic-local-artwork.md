# Automatic local artwork

**Goal:** Opening a note with saved image references renders its local artwork without configuring a directory or pressing Process.

**Architecture:** The App exposes its opened vault directory to the UI. With no explicit processing-directory override, the existing background snapshot worker reads only image files referenced by saved `diagrams` metadata, relative to the note's logical parent inside the vault. Explicit overrides and advanced filters remain optional. Reuse byte quotas, path/regular-file checks and last-good-image staging; do not scan the vault recursively, follow symlinks or change source.

**Steps:** expose the existing vault root independently of LAN; initialize Processing with it; extract saved image references through the current metadata parser; snapshot exact referenced files; place demo assets at vault-relative paths; extend the native rendering fixture with a no-settings startup scenario; run focused tests/build, independent review and a real Linux fresh-start image check. No new asset sync, generation, filesystem watcher or automatic source rewrite.

## Evidence

- `RUSTC_WRAPPER= cargo test --locked --lib --features gui --offline`: PASS, 21 tests, 1 ignored. New startup scenario opens an actual vault/App/UI coordinator with saved nested-note references, only the two image files and no sidecars/settings. It waits for real decoding/staging, verifies both resources/figures and unchanged source, ignores an unrelated symlink, rejects a referenced directory symlink and missing image, and leaves ordinary metadata alone.
- `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,lan,codex --offline`: PASS with existing nonblocking warnings.
- `RUSTC_WRAPPER= cargo build --locked --bin tanglewood --features gui --offline`: PASS. Formatting and diff checks passed.
- Real isolated Linux X11 fresh start: `/tmp/tanglewood-auto-artwork-smoke-6ga9d5di` contains the note plus its two saved image files at vault-relative paths. Both SVG and comic render automatically with Saved status; no Settings/Process interaction occurred. Capture: `docs/design/automatic-artwork-linux.png`.
- Existing demo `target/quiet-demo-vault` now has the exact image bytes at `assets/fig-add-inputs/figure.svg` and `assets/fig-choosing-a-result/strip.png`, matching its saved metadata. The earlier `artwork/` directory remains available for compatibility, but is no longer needed for this note. No source changes in this pass.
- No new Android emulator or physical-device claim for this resource-loading change. Asset synchronization, generation and external-file watching remain outside scope.

Independent changed-risk review: READY, no blocking findings.
