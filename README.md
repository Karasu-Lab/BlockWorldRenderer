# BlockWorldRenderer

Block world renderer written in Rust, built to WebAssembly.

All tools needed for development (the Rust toolchain with the
`wasm32-unknown-unknown` target, rustfmt, clippy, and the wasm-bindgen CLI)
are provided by `flake.nix`. No other toolchain setup is required.

## Build

Enter the development shell and build the WebAssembly module:

```sh
nix develop
cargo build --release --target wasm32-unknown-unknown
```

Generate the JavaScript bindings with wasm-bindgen:

```sh
wasm-bindgen --target web --out-dir pkg target/wasm32-unknown-unknown/release/block_world_renderer.wasm
```

## Test

```sh
nix develop -c cargo test
```
