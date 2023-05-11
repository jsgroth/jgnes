# jgnes-web

An experimental WASM+WebGL2 frontend for jgnes that runs in the browser.

Audio and persistent save files are not implemented, nor is any form of video customization (e.g. aspect ratio / overscan), but the emulation core is identical to the native version.

## Requirements

[wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

## Build

```shell
wasm-pack build --target web
```

## Run

Host `index.html` and the `pkg` directory in the webserver of your choice.

For the simplest option, you can run a local PHP server in the `jgnes-web` directory:
```shell
php -S localhost:8080
```

If you don't have PHP installed:
```shell
sudo apt install php8.1-cli
```
