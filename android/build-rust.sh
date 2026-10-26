#!/usr/bin/env bash
set -euo pipefail
abi=${1:?ABI required}
profile=${2:?debug or release required}
output=${3:?absolute output directory required}
case "$abi" in
    arm64-v8a) rust_target=aarch64-linux-android; clang_target=aarch64-linux-android ;;
    x86_64) rust_target=x86_64-linux-android; clang_target=x86_64-linux-android ;;
    *) printf '%s\n' 'Unsupported ABI; use arm64-v8a or x86_64' >&2; exit 2 ;;
esac
case "$profile" in debug|release) ;; *) exit 2 ;; esac
if [[ $(uname -s) != Linux ]]; then
    printf '%s\n' 'This Rust packaging script currently requires a Linux host.' >&2; exit 2
fi
project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
sdk_root=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}
ndk_root=${ANDROID_NDK_ROOT:-${sdk_root:+$sdk_root/ndk/29.0.14206865}}
if [[ -z "$ndk_root" ]]; then
    printf '%s\n' 'Set ANDROID_HOME or ANDROID_NDK_ROOT to installed Android tools.' >&2; exit 2
fi
compiler_bin="$ndk_root/toolchains/llvm/prebuilt/linux-x86_64/bin"
compiler="$compiler_bin/${clang_target}23-clang"
if [[ ! -x "$compiler" ]]; then
    printf 'Missing Android NDK compiler: %s\n' "$compiler" >&2; exit 2
fi
cd -- "$project_root"
export "CC_${rust_target//-/_}=$compiler"
export "AR_${rust_target//-/_}=$compiler_bin/llvm-ar"
linker_name="CARGO_TARGET_${rust_target^^}_LINKER"
export "${linker_name//-/_}=$compiler"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$project_root/target}
arguments=(build --locked --lib --features gui,lan --target "$rust_target")
if [[ "$profile" == release ]]; then arguments+=(--release); fi
cargo "${arguments[@]}"
mkdir -p -- "$output/$abi"
cp -- "$CARGO_TARGET_DIR/$rust_target/$profile/libtanglewood.so" "$output/$abi/libtanglewood.so"
