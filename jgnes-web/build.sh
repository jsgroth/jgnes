#!/usr/bin/env bash

set -euo pipefail

toolchain=${NIGHTLY_TOOLCHAIN:-nightly}

RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals" \
rustup run $toolchain \
wasm-pack build --target web . "$@" -- -Z build-std=panic_abort,std
