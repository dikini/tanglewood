# Tanglewood

A Rust notes application being implemented against [the MVP plan](docs/plan/tanglewood-mvp.md).
Current progress and unresolved acceptance gates are recorded in [MVP status](docs/mvp-status.md)
and [capabilities](docs/capabilities.md).

## Development

Rust 1.94.0 is pinned for reproducible host builds. The lockfile records
resolved dependencies. The core is independent of graphics:

```sh
cargo test --locked --lib --no-default-features
```

If this environment's configured sccache cannot start inside the sandbox, use
`RUSTC_WRAPPER= cargo test --locked --lib --no-default-features`.
Dependency downloads require network access; an offline run needs a populated cache.

Launch the current Linux source editor with a selected vault:

```sh
cargo run --locked --bin tanglewood --features gui -- --vault /path/to/vault
```

Without `--vault`, storage defaults to `$XDG_DATA_HOME/tanglewood/vault` or
`$HOME/.local/share/tanglewood/vault`. Use `--headless` to open and list a vault
without graphics. One process may write a vault at a time. Source edits autosave;
“Saved” means SQLite acknowledged that revision. Save failures retain accepted
content in memory and show a retry action. Wait for Saved before terminating.
The editor also provides search, rename/trash/restore, links/backlinks and preview.
Explicit Markdown I/O and Linux directory mirroring are available through the shared
API and CLI. Imports/exports use local filesystem paths; note paths remain relative
to the vault. The mirror lives in `<vault>/markdown`:

```sh
cargo run --locked --bin tanglewood -- --headless --vault /path/to/vault --import /path/to/source.md Notes/source.md --mirror
cargo run --locked --bin tanglewood -- --headless --vault /path/to/vault --rescan
cargo run --locked --bin tanglewood -- --headless --vault /path/to/vault --export NOTE_UUID /path/to/export.md
```

The headless list prints note UUIDs. Mirror enablement persists across launches;
`--no-mirror` disables new exports and retains pending work. File export failures
are separate from database Saved status. External conflicts retain both versions;
see [mirror protocol and recovery](docs/mirror-protocol.md).

Linux build prerequisites include a C compiler for bundled SQLite, a linker,
and X11/Wayland/OpenGL runtime support. An isolated X11 window workflow passed;
see [Linux evidence](docs/linux-smoke.md). Android launch remains unverified.
See [Android feasibility](docs/android-feasibility.md) for actual prerequisites,
planned device checks, and the distinction between build and input/lifecycle evidence.

Optional D-Bus and external filter execution are deferred. Canonical Markdown,
single-writer command routing, persistence acknowledgement, and recoverable conflicts
are required boundaries throughout implementation.

Linux automation uses an optional [MCP stdio bridge](docs/mcp.md) into the running
app; it never opens a second vault writer. Enable it with `--features gui,mcp`,
`--mcp-socket /private/directory/owner.sock`, and optionally `--mcp-write`.
Default MCP policy is read-only. The Linux Codex adapter and conversation panel are implemented. Build both
binaries with `cargo build --bins --features gui,codex`, then add `--codex`
to an editor launch with `--mcp-socket`. Use `--mcp-write` only when note mutation
is intended. The panel connects lazily and requires explicit one-action approvals.
See [Codex setup](docs/codex.md) and [actual live evidence](docs/codex-live-evidence.md).
The optional `sync` feature provides explicit authoritative CRDT migration:

```sh
cargo run --locked --features sync -- --headless --vault /path/to/vault --enable-sync
```

Migration first makes a recoverable SQLite backup and preserves note IDs.
Afterward use a binary with `sync` support; an unsupported binary refuses the
vault rather than editing projections. Authenticated QUIC transport is prepared
behind `lan`, but App joining/synchronization is still being integrated.
