# Tanglewood

A Rust notes application for Linux and Android, delivered against [the MVP plan](docs/plan/tanglewood-mvp.md).
Implementation, acceptance evidence and limitations are recorded in [MVP status](docs/mvp-status.md)
and [capabilities](docs/capabilities.md).
The minimal MVP-2 editor/filter increment is described in [the user guide](docs/mvp-2.md)
and tracked separately in [MVP-2 status](docs/mvp-2-status.md).

## Development

Rust 1.94.0 is pinned for reproducible host builds. The lockfile records
resolved dependencies. The core is independent of graphics:

```sh
cargo test --locked --lib --no-default-features
```

If this environment's configured sccache cannot start inside the sandbox, use
`RUSTC_WRAPPER= cargo test --locked --lib --no-default-features`.
Dependency downloads require network access; an offline run needs a populated cache.

Launch the Linux editor with a selected vault:

```sh
cargo run --locked --bin tanglewood --features gui -- --vault /path/to/vault
```

Without `--vault`, storage defaults to `$XDG_DATA_HOME/tanglewood/vault` or
`$HOME/.local/share/tanglewood/vault`. Use `--headless` to open and list a vault
without graphics. One process may write a vault at a time. Source edits remain drafts
until **Ready**; **Cancel** discards the unsubmitted draft. Accepted revisions save
automatically; “Saved” means SQLite acknowledged that revision. Save failures retain accepted
content in memory and show a retry action. Wait for Saved before terminating.
The editor also provides search, rename/trash/restore, links/backlinks and preview.
In the native preview, F6 focuses the first document block directly, including
from search or the notes list. Tab/Shift+Tab move between controls, with a visible
outline on focused icon buttons. Tab also reaches block edit actions; while a block has focus,
↑/↓ move between blocks, Home/End select the first/last, Enter edits its source,
and Escape releases focus. A muted line spanning the block height in the left margin indicates the
current block. The notes-pane divider shows a focus line and uses Left/Right to resize;
focused scrollbars use Up/Down to scroll. Text fields retain their normal cursor keys. Ready/Cancel return
focus to the block. Arbitrary Pandoc output has no inferred block navigation.
Rendered Markdown hides HTML comments while preserving authored source. Ordinary code
fences use syntax colouring, language labels and rounded containers; long lines scroll
horizontally. Ash uses the local VS Code grammar adaptation. The demo vault includes
“Code and comments”; its [source fixture](docs/fixtures/mvp-2/code-and-comments.md) covers
comments, Ash/Bash/Rust, long lines and literal HTML comments in code.
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
see [Linux evidence](docs/linux-smoke.md). The Java-capable x86_64 APK runs on
an API35 Android emulator; Unicode/composed input, selection/system clipboard,
SAF, lifecycle/restart and Linux synchronization passed. See [Android evidence](docs/android-emulator-evidence.md).
Use the [Android build instructions](docs/android-build.md) for the picker and foreground LAN.

Saved illustration/comic `image` references in YAML front matter load automatically from the note’s directory inside the vault. For a root note, `assets/figure.svg` resolves to `<vault>/assets/figure.svg`. An explicit external artwork-directory override remains available in Settings. Artwork bytes are local and are not transferred by note sync. See the [saved-reference example](docs/fixtures/mvp-2/with-image-references.md).

Optional D-Bus remains deferred. MVP-2 adds explicit Linux Pandoc/Lua/JSON processing
and local saved-artwork preview; Android supports the documented Rust illustration
preparation subset. Artwork transfer and publishing/export profiles are deferred. Canonical Markdown,
single-writer command routing, persistence acknowledgement, and recoverable conflicts
are required boundaries throughout implementation.

Linux automation uses an optional [MCP stdio bridge](docs/mcp.md) into the running
app; it never opens a second vault writer. Enable it with `--features gui,mcp`,
`--mcp-socket /private/directory/owner.sock`, and optionally `--mcp-write`.
Default MCP policy is read-only. Enable agent updates per vault in Settings → Connections
or Conversation actions → Vault updates; changes apply without reconnecting.
The Linux conversation surface supports Codex and Kimi. Build both binaries with
`cargo build --bins --features gui,codex,kimi`, then add `--codex` and/or `--kimi`
to an editor launch with `--mcp-socket`. The composer has one searchable model
picker across enabled providers, with supported reasoning choices beside it.
Choosing a model prepares its conversation without sending a message. Connections
provides discovery status, refresh and disconnect controls.

A different provider starts a separate conversation after confirmation when the
source has history. Its prompt and explicit attachment snapshots are copied; the
source and its work remain available. Chats continue to receive updates while
Notes, Research or another chat is visible. Tanglewood holds local drafts and
observed state; the agents remain authoritative for execution. A disconnected
client does not establish that remote work has stopped. See [Codex setup](docs/codex.md)
and [Kimi setup](docs/kimi.md) for configuration and current protocol limits.
The optional `sync` feature provides explicit authoritative CRDT migration:

```sh
cargo run --locked --features sync -- --headless --vault /path/to/vault --enable-sync
```

Migration first makes a recoverable SQLite backup and preserves note IDs.
Afterward use a binary with `sync` support; an unsupported binary refuses the
vault rather than editing projections. The `lan` feature includes authenticated
QUIC pairing, empty-vault join and durable Automerge synchronization, with manual
GUI controls and a CLI workflow. See [LAN setup and evidence](docs/lan-sync.md).
Two-App and actual Android emulator/Linux pairing, offline edits, reconnect and
both-owner restart passed; see [cross-platform evidence](docs/android-linux-sync-evidence.md).
Physical wireless LAN remains untested.

The Linux Codex composer now offers Work, Plan, Research and Goal profiles with
separate approval/network controls. Research can save linked notes under
`Research/<topic>/`; enable the vault agent-update permission to permit note creation through the
owner. See [Codex workflow](docs/codex.md#composer-profiles-and-command-execution).

Preview the rich native conversation UI without connecting Codex:
`RUSTC_WRAPPER= cargo run --locked --offline --example transcript_demo --features gui,codex`.
Response checklists and reported plan steps are display-only; vault tasks are
not implemented.


Research workspaces retain the investigation brief, associated conversations,
checkpoint rounds, sources and artifacts (original files, references and optional
web Markdown copies). Create new report notes from saved research on demand.
See [research workflow, limits and evidence](docs/research.md).
