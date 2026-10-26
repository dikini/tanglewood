# Tanglewood

A Rust notes application being implemented against [the MVP plan](docs/plan/tanglewood-mvp.md).
Current progress and unresolved acceptance gates are recorded in [MVP status](docs/mvp-status.md)
and [capabilities](docs/capabilities.md).

## Development

Rust 1.94 is available in the current development environment. The lockfile records
resolved dependencies. The core is independent of graphics:

```sh
cargo test --locked --lib --no-default-features
```

If this environment's configured sccache cannot start inside the sandbox, use
`RUSTC_WRAPPER= cargo test --locked --lib --no-default-features`.
Dependency downloads require network access; an offline run needs a populated cache.

Launch the current Linux source editor with a selected vault:

```sh
cargo run --locked --features gui -- --vault /path/to/vault
```

Without `--vault`, storage defaults to `$XDG_DATA_HOME/tanglewood/vault` or
`$HOME/.local/share/tanglewood/vault`. Use `--headless` to open and list a vault
without graphics. One process may write a vault at a time. Source edits autosave;
“Saved” means SQLite acknowledged that revision. Save failures retain accepted
content in memory and show a retry action. Wait for Saved before terminating.
The editor also provides search, rename/trash/restore, links/backlinks and preview.
Markdown import/export and directory mirroring are still being integrated.

Linux build prerequisites include a C compiler for bundled SQLite, a linker,
and X11/Wayland/OpenGL runtime support. An isolated X11 window workflow passed;
see [Linux evidence](docs/linux-smoke.md). Android launch remains unverified.
See [Android feasibility](docs/android-feasibility.md) for actual prerequisites,
planned device checks, and the distinction between build and input/lifecycle evidence.

Optional D-Bus and external filter execution are deferred. Canonical Markdown,
single-writer command routing, persistence acknowledgement, and recoverable conflicts
are required boundaries throughout implementation.
