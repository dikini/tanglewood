# Wasm extensions

Tanglewood can run a compiled WebAssembly component or a scripting interpreter
inside a component. The optional `wasm-extensions` feature supports the original
read-only snapshot interface in [`wit/extensions.wit`](../wit/extensions.wit)
and the registered, permission-scoped command interface in
[`wit/v2/extensions.wit`](../wit/v2/extensions.wit). Rhai runs in a component;
the application does not link a native Rhai interpreter.

## Build the sample components

The repository pins Rust 1.94.0. Install its component target once and build the
separate, locked guest workspace:

```sh
rustup target add --toolchain 1.94.0 wasm32-wasip2
./scripts/build-wasm-extensions.sh
```

The script uses the native `wasm32-wasip2` component workflow and writes to
`target/wasm-extensions/wasm32-wasip2/release/`. It produces
`tanglewood_compiled_guest.wasm`, `tanglewood_rhai_guest.wasm`, the v2
`tanglewood_compiled_v2_guest.wasm` and `tanglewood_rhai_v2_guest.wasm`, and
two test-only fixtures. The guest workspace pins `wit-bindgen` and Rhai and has
its own `Cargo.lock`. Rebuilding does not change the application's dependencies.

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
cargo test --locked --features wasm-extensions,mcp --test wasm_extensions --test wasm_cli \
  --test wasm_command_contract --test wasm_runtime_v2 --test wasm_plugin_cli \
  --test wasm_plugin_commands --test wasm_research_commands -- --test-threads=1
```

## Registered plugins and scripts

Version 2 plugins are installed as locally controlled bundles outside the vault.
Each bundle has a `manifest.json` and the component named in that manifest.
Startup loads the explicitly named IDs; automation selects an ID and cannot
choose the component bytes or supply a manifest. The runtime uses the bytes
whose SHA-256 matched the manifest when the bundle was loaded. The registry
root must be absolute and must not overlap the vault. This secure registry
implementation currently requires Linux.

For a temporary example, build the guests above, then create two bundles:

```sh
DEMO_DIR=$(mktemp -d)
PLUGIN_ROOT="$DEMO_DIR/plugins"
mkdir -p "$PLUGIN_ROOT/compiled" "$PLUGIN_ROOT/rhai"
cp target/wasm-extensions/wasm32-wasip2/release/tanglewood_compiled_v2_guest.wasm \
  "$PLUGIN_ROOT/compiled/component.wasm"
cp target/wasm-extensions/wasm32-wasip2/release/tanglewood_rhai_v2_guest.wasm \
  "$PLUGIN_ROOT/rhai/component.wasm"
COMPILED_SHA=$(sha256sum "$PLUGIN_ROOT/compiled/component.wasm" | cut -d ' ' -f 1)
RHAI_SHA=$(sha256sum "$PLUGIN_ROOT/rhai/component.wasm" | cut -d ' ' -f 1)

cat > "$PLUGIN_ROOT/compiled/manifest.json" <<EOF
{
  "manifest_version": 1,
  "id": "compiled",
  "version": "1",
  "kind": "compiled",
  "wit_version": "0.2.0",
  "component": "component.wasm",
  "component_sha256": "$COMPILED_SHA",
  "invocation": {"local": true, "automation": true},
  "permissions": {"rules": [
    {"action": "note.create", "note_paths": {"mode": "only", "values": ["Notes"]}},
    {"action": "note.edit", "notes": {"mode": "all"}},
    {"action": "note.read", "notes": {"mode": "all"}}
  ]}
}
EOF

cat > "$PLUGIN_ROOT/rhai/manifest.json" <<EOF
{
  "manifest_version": 1,
  "id": "rhai",
  "version": "1",
  "kind": "interpreter",
  "languages": ["rhai"],
  "wit_version": "0.2.0",
  "component": "component.wasm",
  "component_sha256": "$RHAI_SHA",
  "invocation": {
    "local": true, "local_source": true,
    "automation": true, "automation_source": true
  },
  "permissions": {"rules": [
    {"action": "note.create", "note_paths": {"mode": "only", "values": ["Notes"]}},
    {"action": "note.edit", "notes": {"mode": "all"}},
    {"action": "note.read", "notes": {"mode": "all"}}
  ]}
}
EOF
```

Invoke either registered ID locally. The invoked ID is loaded automatically;
`--plugin ID` may be repeated to load additional IDs for a GUI or MCP session.

```sh
cargo run --locked --features wasm-extensions --bin tanglewood -- \
  --headless --vault "$DEMO_DIR/vault" --plugin-registry "$PLUGIN_ROOT" \
  --plugin-run compiled --wasm-input Notes/from-component.md

cargo run --locked --features wasm-extensions --bin tanglewood -- \
  --headless --vault "$DEMO_DIR/vault" --plugin-registry "$PLUGIN_ROOT" \
  --plugin-evaluate rhai \
  --wasm-script examples/wasm-extensions/scripts/note-workflow-v2.rhai \
  --wasm-input Notes/from-rhai.md
```

To start the headless MCP server with both IDs registered:

```sh
cargo run --locked --features wasm-extensions,mcp --bin tanglewood -- \
  --headless --vault "$DEMO_DIR/vault" --plugin-registry "$PLUGIN_ROOT" \
  --plugin compiled --plugin rhai --mcp-socket "$DEMO_DIR/mcp.sock"
```

The same registry and repeated `--plugin` options work at GUI startup with the
`gui` feature. Stop the server before running another process on the same vault.

These examples create, edit and read separate notes. The CLI prints the guest
result, diagnostics, and effect receipts; a later failure does not undo an
earlier accepted effect. The MCP command path instead returns a run ID promptly
for status and cancellation while the owner continues ticking.

Manifest permissions default to deny. Each rule couples one action to its
resource scope: `only` note paths are normalized vault-relative roots; `only`
note IDs are exact UUIDs; `all` is explicit. A grant to read all notes and edit
one note does not permit editing every note. The manifest's command and resource
grant is intersected with the invoking caller's existing rights. For automation,
the vault's agent-write setting still blocks mutations when disabled; local
invocations are also bounded by the manifest. Source evaluation needs the
separate `local_source` or `automation_source` flag in addition to invocation
permission. Unknown fields, actions, versions or mismatched component digests
are rejected. Consult [`plugin_permissions::Action` and `Rule`](../src/plugin_permissions.rs)
for the available action names and applicable scopes. An optional `limits`
record can lower `max_calls`, `max_host_bytes`, `timeout_ms` and
`max_downstream_jobs`; it cannot raise the host ceilings. Reloading or revoking a
bundle invalidates active bindings instead of expanding their grants.

MCP currently authenticates a same-user automation origin class, not an
individual agent identity. Plugin IDs, run IDs and names supplied by a client
do not establish individual agent ownership. Local registry setup is separate
from remote invocation.

Queued research acquisitions store their originating plugin ID, component digest,
caller class and complete permission rules in the same persisted workspace as
the job. Later work and retries require that component to remain registered,
check both original and current rules, and keep the narrower caller authority.
A restart without the registry fails closed. File reads use pinned descriptors;
every HTTP redirect and discovered PDF URL is checked before contact. Accepted
work may finish after a guest stops; retained effect receipts describe what was
accepted and whether durable confirmation succeeded.

Execution admission and completion are separate:

| Request | Completion |
| --- | --- |
| Note/research query or authorized local LAN getter | Immediate domain result |
| Note mutation, export or persisted research/settings job | Owner waits for its persistence/I/O acknowledgement before returning `Persisted` to the guest |
| Markdown import or mirror rescan | Owner waits for domain I/O completion and returns `Accepted`; resulting note storage is not durably confirmed by that receipt |
| Other authorized local control action | Domain acceptance, without claiming durable storage |
| Plugin run or script evaluation | Immediate run ID; inspect status for eventual output and receipts |
| Run cancellation | Immediate acknowledgement; accepted domain effects are retained |

The v2 Rhai interpreter offers `capabilities()`, `log(message)`,
`command(value)` and `read_result(token, path, offset, limit)`, plus convenience
functions for note workflows. Capability discovery returns complete scoped
rules, keeping each action paired with its own restrictions. `command` accepts a typed WIT command represented
as a Rhai value and returns its typed result, including errors and completion
state. For example, with a note UUID as `input`:

```rhai
let reply = command(#{ ReadNote: #{ id: input } });
let note = reply.Ok.value.Note.note;
note.path + ":" + note.text
```

See [`note-read-v2.rhai`](../examples/wasm-extensions/scripts/note-read-v2.rhai)
and the versioned WIT for command, result and error shapes. When a result is
too large, `reply.Err.TooLarge` contains a run-bound token and the actual
completion state. `read_result` retrieves bounded typed nodes by field/index
path and offset; a changed source can return `ResultChanged`. A cursor does not
replay a mutation. Calls are dispatched through the native owner command path,
which enforces the effective scope, resource limits and normal validation.

## Try the original read-only compiled guest

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

## Try the original read-only Rhai guest

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

## Original snapshot contract

The versioned package `tanglewood:extensions@0.1.0` has one shared `host`
interface and two worlds:

- `compiled-extension` exports `compiled.run(input) -> result<string, string>`.
- `interpreter-extension` exports
  `interpreter.evaluate(source, input) -> result<string, string>`.

The host interface provides `log(level, message)` and `read-note()`. Logging
levels are a WIT enum, errors are a WIT variant, and snapshots are WIT records.
Strings for input and source are intentionally opaque to this host. Its snapshot
interface has no command or write capability. `--wasm-note` explicitly selects
one note; the owner reads it using the existing read-only command and passes a bounded
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
one-component compiled-code cache. A headless CLI process does not retain this
cache after it exits.

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
- Guarded Markdown imports and comparisons with existing destination text read
  at most 8 MiB, using both a metadata check and a bounded read. When a protected
  plugin registry is configured, native explicit exports and mirror comparisons
  use the same ceiling. Larger existing content produces a recoverable error and
  is preserved. This filesystem limit is separate from the WIT host-call budget.

Errors distinguish loading and interface mismatches, guest or script errors,
traps, resource exhaustion, and cancellation. Guest failure does not roll back
host operations that have already completed. For v2, manifest limits can lower
the host call, transferred byte and deadline ceilings; accepted mutations have
explicit completion and effect receipts.

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

A new compiled guest implements the desired version's `compiled-extension`
world with `wit-bindgen` and builds as a `cdylib` for `wasm32-wasip2`. Another
interpreter can implement that version's `interpreter-extension` world: compile
the language runtime into the component, pass source to `evaluate`, and expose
only the same authorized host capabilities. JavaScript and Python guests are
not part of this milestone. Registry configuration is available at GUI startup;
the runtime and invocation path have not been verified on Android or in the GUI.
