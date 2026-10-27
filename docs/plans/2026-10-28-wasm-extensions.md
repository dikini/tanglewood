# Wasmtime and WIT extensions implementation plan

**Goal:** Deliver compiled Rust and Wasm-hosted Rhai extensions with a shared versioned capability interface, bounded execution, CLI invocation, examples, tests, and documentation.

**Architecture:** Keep the native application as one Rust package. Add an optional `wasm-extensions` runtime module. The owner captures one explicitly granted note through the existing read command, then sends owned values to a dedicated worker. Reuse Engine and compiled Component code; create fresh Stores/instances for independent invocations. The CLI is the initial exposure; GUI exposure and document-filter integration remain independent.

**Tech stack:** Rust 1.94, a compatible pinned Wasmtime release, Component Model bindgen, wit-bindgen, native wasm32-wasip2 guests, Rhai inside Wasm.

The user explicitly authorized implementation choices and continuation through verification; this plan does not start the unrelated MVP goal.

## Tasks and ownership

1. **Runtime / Terra 6.1 max:** own `wit/`, `src/wasm_extensions.rs` (or its module directory), and dedicated runtime integration tests. Establish a versioned shared host with typed logging/read-note and two guest worlds (`run` and `evaluate`). Apply explicit capability grants, bounded callbacks, memory/table/stack/fuel/input/output limits, structured recoverable errors, cancellation, and fresh execution state. Propose root dependencies to PM. Test real guests and malicious fixtures at the runtime boundary.
2. **Guests / Terra 6.1 max:** own `examples/wasm-extensions/`, `scripts/build-wasm-extensions.sh`, and `docs/wasm-extensions.md`. Build compiled Rust and Rhai interpreter cdylibs with wit-bindgen; demonstrate different scripts without rebuilding. Pin compatible tools and inspect imports. Coordinate WIT with runtime owner before writes. Build in a separate guest target directory.
3. **PM integration:** own Cargo manifest/lock, `src/lib.rs`, `src/main.rs`, task board and capabilities. Wire optional native feature, CLI flags for component/script/input/granted note; capture note through dispatch and poll worker while ticking application owner. No guest compilation or execution on the UI/async executor. Preserve existing CLI semantics.
4. **Independent review / Sol 6.1 max:** freeze completed implementation files; review specification compliance, correctness, state isolation, resource/capability boundaries and reproducibility in one combined review. Developers repair concrete findings, then review affected deltas.
5. **Verification / PM:** serialize root Cargo work. Build guests, run focused runtime integration tests, baseline library tests, relevant feature tests/build/lint, formatting and actual headless CLI smokes for both guests and changed Rhai source. Update evidence and limitations; make unsigned local checkpoint commit with explicit staging. Stop after relevant checks pass.

## Acceptance

- Real compiled guest loads, reads the selected application note, logs and returns output.
- Rhai executes supplied source inside the Wasm component and calls the same host API; source can change independently.
- Script errors, traps, loops, excess memory, unauthorized/missing imports and state isolation have focused evidence.
- No ambient WASI filesystem/network/environment/process authority; link only required capabilities.
- Engine/component reuse is distinct from Store/guest state; reload/calls start fresh.
- Resource failures leave application usable. Documentation explains bounds, errors, lifecycle, extending WIT and another interpreter, and no automatic rollback.
- No Android or GUI-runtime claim from headless host checks.
