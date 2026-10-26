# Linux local editor smoke

2026-10-26, Rust 1.94.0, eframe/egui 0.33.3, Xvfb isolated display `:97`.

- `RUSTC_WRAPPER= cargo build --locked --features gui --offline` passed.
- Approved display escalation allowed Xvfb and the app to run; unset the host's `WAYLAND_DISPLAY` for the isolated X11 session.
- Launched `target/debug/tanglewood --vault /tmp/tanglewood-linux-smoke-vault` on the test display.
- Inspected the rendered window, clicked Create note, focused the multiline source widget, entered the following using actual keyboard events, and enabled Preview:

```markdown
# Linux smoke
Saved from the actual editor.
[[Second#part]]
```

- The UI displayed Saved; a read-only SQLite query confirmed identical newline-preserving source.
- Terminated only the identified temporary app process after acknowledgement, rebuilt the reviewed UI delta, reopened the same vault, and visually confirmed the identical multiline source with Saved status.
- Screenshots inspected in `/tmp/tanglewood-smoke-edited.png` and `/tmp/tanglewood-smoke-restart.png`; temporary evidence is not a release artifact.

This checks rendering, source typing, preview, save acknowledgement and restart recovery on Linux. It does not prove interactive IME composition, Android input/lifecycle, external file recovery, LAN transport or live Codex. The note linked to a nonexistent target; link resolution is covered by a focused shared resolver scenario, not this smoke.
