# jgnes-web

An experimental WASM+WebGL2 frontend for jgnes that runs in the browser.

This frontend does not have as many configuration features as the native version, but the emulation core is identical.

## Requirements

### Rust Nightly

The WASM frontend requires a nightly version of the Rust toolchain because the stable standard library does not support
sharing memory between the main thread and worker threads in WASM. (Presumably because this isn't supported in all WASM
runtimes, although all major browsers support it.)

To install the latest nightly toolchain, including the standard library source (required for the build):
```shell
rustup toolchain add nightly --component rust-src
```

As nightly is unstable, the project (or its dependencies) may not always build on the latest nightly version. To install a specific nightly version (e.g. the 2023-06-01 version):
```shell
rustup toolchain add nightly-2023-06-01 --component rust-src
```

### wasm-pack

wasm-pack is required to build a WASM/JavaScript package that can run in the browser. It's possible
to do so manually using `wasm-bindgen-cli` and `wasm-opt` directly, but
wasm-pack makes it more convenient and includes a `wasm-bindgen` version safety check.

To install:
```shell
cargo install wasm-pack
```

## Build

Building requires the following incantation in order to enable shared memory in WASM during the build:
```shell
RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals" \
rustup run nightly \
wasm-pack build --target web . -- -Z build-std=panic_abort,std
```

Alternatively, you can just run the provided `build.sh` script which runs that command:
```shell
./build.sh
```

By default `build.sh` will use the toolchain `nightly`. To use a different toolchain, set the `JGNES_WEB_TOOLCHAIN` environment variable:
```shell
JGNES_WEB_TOOLCHAIN=nightly-2023-06-01 ./build.sh
```

For local testing, the `--dev` flag (passed on to wasm-pack) will disable link-time optimizations and skip running
`wasm-opt`, which leads to _significantly_ shorter build times:
```shell
./build.sh --dev
```

## Run

Host `index.html`, the `js` directory, and the `pkg` directory in the webserver of your choice.

For the simplest option, you can use the provided `webserver.py`:
```shell
./webserver.py localhost:8080
```

This script extends Python's `http.server` builtin to additionally set the `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp` HTTP headers on every request, as the WASM frontend will not work in some browsers if
these headers are not set.
