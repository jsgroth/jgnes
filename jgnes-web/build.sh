#!/usr/bin/env bash

set -euo pipefail

toolchain=${JGNES_WEB_TOOLCHAIN:-nightly}

echo "Building using toolchain '$toolchain'"

RUSTFLAGS=""
cargo_args=""
if [[ -n "${JGNES_WEBGPU:-}" ]]; then
    echo "Compiling for WebGPU backend"

    RUSTFLAGS="--cfg=web_sys_unstable_apis"
    cargo_args="--no-default-features"
else
    echo "Compiling for WebGL2 backend"
fi

RUSTFLAGS="$RUSTFLAGS --cfg getrandom_backend=\"wasm_js\" -C target-feature=+atomics,+bulk-memory,+mutable-globals" \
rustup run $toolchain \
wasm-pack build --target web . "$@" -- $cargo_args -Z build-std=panic_abort,std
