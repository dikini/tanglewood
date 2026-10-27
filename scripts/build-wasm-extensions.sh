#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

cargo build \
    --locked \
    --release \
    --workspace \
    --manifest-path examples/wasm-extensions/Cargo.toml \
    --target wasm32-wasip2 \
    --target-dir "$repo_root/target/wasm-extensions"

printf 'Components: %s\n' "$repo_root/target/wasm-extensions/wasm32-wasip2/release"
