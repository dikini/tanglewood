# Wasm extension acceptance evidence — 2026-10-28

Scope: native Linux host, optional `wasm-extensions` feature. Baseline commit:
`850d230`. Terra 6.1 max implemented the runtime and guests; Sol 6.1 max
independently reviewed the frozen combined changes.

## Commands and results

| Command | Result |
| --- | --- |
| `./scripts/build-wasm-extensions.sh` | Passed; four Rust 1.94 native wasm32-wasip2 components built from locked guest workspace |
| `cargo fmt --manifest-path examples/wasm-extensions/Cargo.toml --all -- --check` | Passed |
| `cargo clippy --locked --release --manifest-path examples/wasm-extensions/Cargo.toml --workspace --target wasm32-wasip2 --target-dir target/wasm-extensions -- -D warnings` | Passed; fixture-only Clippy also passed after adding the sleep mode |
| `bash -n scripts/build-wasm-extensions.sh` | Passed |
| `CARGO_BUILD_JOBS=2 cargo test --locked --lib --no-default-features -- --test-threads=1` | 44 passed, 1 explicit live-network test ignored |
| `CARGO_BUILD_JOBS=2 cargo clippy --locked --all-targets` | Passed with existing warnings |
| `CARGO_BUILD_JOBS=2 cargo test --locked --features wasm-extensions --test wasm_extensions` | 1 combined scenario passed in 59.45 s before the isolated count-classification review fix |
| `cargo test --locked --features wasm-extensions --test wasm_cli` | 1 real-vault CLI scenario passed in 114.40 s before the isolated count-classification review fix |
| `rustfmt --check --edition 2024 --config skip_children=true src/lib.rs src/main.rs src/wasm_extensions.rs tests/wasm_extensions.rs tests/wasm_cli.rs` | Passed on frozen implementation before the isolated review fix; final scoped check recorded below |
| `cargo test --locked --features wasm-extensions --test wasm_extensions component_instance_count_limit_is_resource_exhaustion` | 1 focused actual-component regression passed after reproducing the classification bug |
| `CARGO_BUILD_JOBS=2 cargo clippy --locked --all-targets --features wasm-extensions` | Passed after final lint-only cleanup; no warnings in changed Wasm files, existing project warnings remain |
| Final scoped Rust formatting command above | Passed after the count-classification and lint-only deltas |
| `git diff --check` | Passed |

The runtime scenario loads both real guests, changes Rhai source without
rebuilding, exercises script errors, denied note/log capabilities, host quota
exhaustion, unsupported imports and wrong world, traps, fuel exhaustion, memory
growth rejection, rejected WASI sleep at subscription, fresh instance state, Busy,
cancellation, stale-handle cancellation, and recovery. The CLI scenario imports
an actual note through the normal application path, invokes the compiled guest,
then evaluates both shipped Rhai scripts from the same interpreter component.
It checks outputs and captured log diagnostics.

The initial parallel baseline library run had 43 passed, 1 failed and 1 ignored:
`automation_permission_requires_local_durable_grant_and_revokes_immediately`
received `Locked` reopening its vault at `src/app.rs:3962`. A targeted rerun
passed, then the complete serial run passed. No application/storage changes were
made for this intermittent baseline result.

Repository-wide `cargo fmt --check` fails on existing formatting in `src/app.rs`,
`src/ui.rs`, and `src/ui/codex_context.rs`. The same failures were confirmed on
an untouched `git archive HEAD` copy. Changed Rust files and the guest workspace
are checked separately; unrelated files were preserved.

## Component imports

Actual top-level imports were inspected using `wasmparser 0.258.3` over the built
components. The inspection program traversed `Parser::parse_all` and printed
`ComponentImportSection` names only at nesting depth one. PM and reviewer also
ran the resulting executable directly and reviewed its parser source. The guide
provides the equivalent optional `wasm-tools component wit` inspection command.

All guests import WASI 0.2.6 IO/CLI signatures. The compiled and Rhai examples
import `tanglewood:extensions/host@0.1.0`. Rhai additionally imports monotonic and
wall clocks, filesystem types/preopens, and insecure-seed; the sleep fixture
also imports monotonic-clock. The unauthorized fixture additionally imports
`tanglewood:extensions/unregistered@0.1.0`. No other top-level imports were
reported. The runtime validates its allowlist and supplies only documented,
budgeted operations; allowed unused signatures resolve to traps.

## Review and limitations

Independent review found one error-classification issue: valid components
exceeding configured resource counts were rejected but reported as Interface.
The fix recognizes the pinned Wasmtime count-limit causes before interface
fallback. A valid component with 33 additional core instances first reproduced
the wrong Interface error; the focused regression passed after normalization.
The reviewer approved this delta and the final equivalent input-validation
lint cleanup. No other material findings remain. Passing unchanged guest,
runtime and CLI scenarios were reused rather than rerun for these narrow deltas.
Final Clippy completed after both changes.

Cold Rhai compilation in the debug host took roughly 50 seconds per fresh CLI
process here. The 10-second execution deadline begins after compilation;
compilation remains a worker operation and an in-progress compile must finish
before cancellation takes effect. The CLI and runtime API are implemented;
there is no new GUI control or document-filter wiring. Android and GUI-runtime
execution were not tested. Guest failure does not roll back completed host
operations (the current host surface reads note snapshots and logs only).
