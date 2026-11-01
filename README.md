# BlockWorldRenderer

Block world renderer written in Rust, built to WebAssembly.

## Build

Build the WebAssembly module:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
```

Generate the JavaScript bindings with
[wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen):

```sh
wasm-bindgen --target web --out-dir pkg target/wasm32-unknown-unknown/release/block_world_renderer.wasm
```

## Test

```sh
cargo test
```
