# Wasm extensions

Tanglewood can run a compiled WebAssembly component or a scripting interpreter
inside a component. This optional Linux/desktop headless path uses the Component
Model contracts in [`wit/extensions.wit`](../wit/extensions.wit); a native Rhai
interpreter is not linked into the application.

## Build the sample components

The repository pins Rust 1.94.0. Install its component target once and build the
separate, locked guest workspace:

```sh
rustup target add --toolchain 1.94.0 wasm32-wasip2
./scripts/build-wasm-extensions.sh
```

The script uses the native `wasm32-wasip2` component workflow and writes to
`target/wasm-extensions/wasm32-wasip2/release/`. It produces
`tanglewood_compiled_guest.wasm`, `tanglewood_rhai_guest.wasm`, and two test-only
fixtures. The guest workspace pins `wit-bindgen` and Rhai and has its own
`Cargo.lock`. Rebuilding does not change the application's dependencies.

The pinned dependencies and commands make the build repeatable from a clean
checkout. Rhai uses a compile-time hash seed, so independently built binaries
are not guaranteed to be byte-identical.

To inspect a component's actual imports and exports, install the optional
inspection tool and print its component world:

```sh
cargo install --locked wasm-tools --version 1.258.3
wasm-tools component wit target/wasm-extensions/wasm32-wasip2/release/tanglewood_rhai_guest.wasm
```

After building the guests, run the focused component and CLI integration
checks with:

```sh
cargo test --locked --features wasm-extensions --test wasm_extensions --test wasm_cli
```

## Try the compiled guest

Pick a vault and an existing note UUID. `--headless` lists UUIDs, or create a
small temporary example:

```sh
DEMO_DIR=$(mktemp -d)
printf '# Demo\nA short example note.\n' > "$DEMO_DIR/note.md"
cargo run --locked --bin tanglewood -- --headless --vault "$DEMO_DIR/vault" \
  --import "$DEMO_DIR/note.md" Notes/demo.md
```

Copy the UUID printed by the last command into `NOTE_UUID`, then run:

```sh
NOTE_UUID=replace-with-the-printed-uuid
cargo run --locked --features wasm-extensions --bin tanglewood -- \
  --headless --vault "$DEMO_DIR/vault" \
  --wasm-component target/wasm-extensions/wasm32-wasip2/release/tanglewood_compiled_guest.wasm \
  --wasm-note "$NOTE_UUID" --wasm-input 'Summary: '
```

The guest logs that it is summarizing the note and returns its path and
whitespace-delimited word count. The result goes to stdout and diagnostics go
to stderr. The CLI authorizes logging and a read of the selected note only. If
`--wasm-note` is omitted, a guest attempting to read a note receives an error.

## Try Rhai inside Wasm

Pass a script file to select the interpreter world:

```sh
cargo run --locked --features wasm-extensions --bin tanglewood -- \
  --headless --vault "$DEMO_DIR/vault" \
  --wasm-component target/wasm-extensions/wasm32-wasip2/release/tanglewood_rhai_guest.wasm \
  --wasm-script examples/wasm-extensions/scripts/note-summary.rhai \
  --wasm-note "$NOTE_UUID" --wasm-input 'Script: '
```

To change behavior without rebuilding the interpreter, rerun the same command
with `--wasm-script examples/wasm-extensions/scripts/note-text.rhai`, or edit a
copy of either script. Each script gets the string variable `input` and two
functions: `log(message)` and `read_note()`. The latter returns a Rhai map with
`id`, `path`, `revision`, and `text` string fields. The last expression is
converted to the result string. A failed host call or invalid script returns
an application error rather than terminating Tanglewood.

## Contracts and capabilities

The versioned package `tanglewood:extensions@0.1.0` has one shared `host`
interface and two worlds:

- `compiled-extension` exports `compiled.run(input) -> result<string, string>`.
- `interpreter-extension` exports
  `interpreter.evaluate(source, input) -> result<string, string>`.

The host interface provides `log(level, message)` and `read-note()`. Logging
levels are a WIT enum, errors are a WIT variant, and snapshots are WIT records.
Strings for input and source are intentionally opaque to the host. There is no
generic RPC or write capability. `--wasm-note` explicitly selects one note; the
owner reads it using the existing read-only command and passes a bounded
immutable snapshot to the guest worker. The guest receives values, not references
to application objects. A snapshot describes the selected revision at
invocation time; a later edit does not change it.

Rust's `wasm32-wasip2` standard library and Rhai produce WASI interface imports.
The runtime accepts only an allowlist of those interfaces. Budgeted wall-clock
and monotonic-clock reads and an insecure random seed are available. Clock
subscriptions and polling trap immediately. Other allowlisted WASI interfaces,
including CLI, streams, and filesystem interfaces, link to trap stubs: their
operations are not available to the guest. There are no ambient filesystem,
environment, standard-stream, network, or process handles. Unknown imports,
including network interfaces, are rejected before linking; an unauthorized
Tanglewood capability call returns a host error.

## State, limits, and errors

Each independent invocation uses a fresh store and guest instance; mutable guest
state is not shared between invocations. Loading changed component bytes starts
with fresh state. The interpreter creates a fresh Rhai engine and scope for
each evaluation, so script globals do not carry over. Component compilation and
execution happen on a worker while the application owner continues to tick.
A `Runtime` and its clones allow one active job and share an engine and a
one-component compiled-code cache. Each headless CLI run creates a new runtime,
so this cache does not persist between CLI processes.

Cold Rhai component compilation in the measured debug-host CLI took about
50 seconds on the test machine; the time varies by machine and build. This
compilation occurs before the execution deadline, and it cannot be forcibly
cancelled while in progress.

Current per-invocation limits are:

- Component file: 16 MiB. Input and script source: 1 MiB each. Result or
  application error: 1 MiB.
- Selected note: 256 KiB text, 4 KiB path, and 128 bytes each for ID and
  revision.
- Guest memory: 64 MiB per memory, at most four memories. Tables: 10,000
  elements per table, at most eight tables. There are at most 32 instances and
  1 MiB of Wasm stack.
- Execution: 100 million fuel units and a 10-second deadline for instantiation
  and execution. Component compilation happens on the worker before this
  deadline; cancellation takes effect after an in-progress compilation returns.
- Host calls: 128 calls, including permitted WASI clock and seed operations,
  and 1 MiB of cumulative transferred data, with a 4 KiB maximum per log message.

Errors distinguish loading and interface mismatches, guest or script errors,
traps, resource exhaustion, and cancellation. Guest failure does not roll back
host operations that have already completed; the currently exposed host API is
read-only apart from logging.

The two additional built components are test fixtures, not examples to use as
extensions: `tanglewood_wasm_fixture.wasm` accepts `trap`, `spin`, `grow`,
`sleep`, and `bump` inputs, while `tanglewood_wasm_unauthorized_fixture.wasm`
requests an unregistered interface. Integration tests use them to check
isolation and limits.

## Extending the interface

To add a capability, update the versioned WIT and both host and guest bindings,
then implement explicit authorization, bounded host work, and error handling in
the runtime. Route application operations through the existing commands and
owner thread, with revision checks for any future mutations. Update guest
examples and focused integration coverage together; change the interface
version when compatibility requires it.

A new compiled guest implements the `compiled-extension` world with
`wit-bindgen` and builds as a `cdylib` for `wasm32-wasip2`. Another interpreter
can instead implement the `interpreter-extension` world: compile the language
runtime into the component, pass source to `evaluate`, and expose only the
same authorized host capabilities. JavaScript and Python guests are not part
of this milestone. The runtime and invocation path have not been verified on
Android or in the GUI.
