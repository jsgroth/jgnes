# jgnes-web

An experimental WASM+WebGL2 frontend for jgnes that runs in the browser.

Audio and persistent save files are not implemented, nor is any form of video customization (e.g. aspect ratio / overscan),
but the emulation core is identical to the native version.

## Requirements

### Rust Nightly

The WASM frontend requires a nightly version of the Rust toolchain because the stable standard library does not support
sharing memory between the main thread and worker threads in WASM. (Presumably because that isn't supported in all WASM
runtimes, although all major browsers support it.)

To install the latest nightly toolchain, including the standard library source (required for the build):
```shell
rustup toolchain add nightly --component rust-src
```

### wasm-pack

[wasm-pack](https://rustwasm.github.io/wasm-pack/installer/) is required to build a WASM/JavaScript package that can run
in the browser.

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

## Run

Host `index.html` and the `pkg` directory in the webserver of your choice.

For the simplest option, you can use the provided `webserver.py`:
```shell
./webserver.py localhost:8080
```

This script extends Python's `http.server` builtin to additionally set the `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp` HTTP headers on every request, as the WASM frontend will not work in some browsers if
these headers are not set.
