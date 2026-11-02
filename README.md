# BlockWorldRenderer

Block world renderer written in Rust, built to WebAssembly.

All tools needed for development (the Rust toolchain with the
`wasm32-unknown-unknown` target, rustfmt, clippy, the wasm-bindgen CLI, and the
native libraries used by the Client repository) are provided by `flake.nix`. No
other toolchain setup is required.

## Environment

The toolchain is loaded by [direnv](https://direnv.net/) when you enter the
workspace; allow it once:

```sh
direnv allow
```

Without direnv, enter the development shell with `nix develop`.

The Client repository is a submodule at `third_party/Client`, kept as reference
material and excluded from the Cargo workspace. Fetch it, with its own
submodules, using:

```sh
git submodule update --init --recursive
```

## Crates

The workspace root is the `block-world-renderer` WebAssembly entry point. The
engine is split into API crates under `crates/`, prefixed `bwr-`:

| Crate | Responsibility |
| --- | --- |
| `utils` | Small shared helper types |
| `serializer` | Data conversion on top of `From` / `TryFrom` through a `Value` type |
| `registry` | Resource keys and registries |
| `random`, `seed` | Random sources and world seeds |
| `datacomponent` | Data components and patches |
| `data` | Data-driven commands, resource pack resolution and world generation |
| `block`, `item`, `entity` | Blocks, items and entities |
| `storage` | Chunk and world storage |
| `resourcepack`, `shader` | Resource packs and shader programs |
| `downloader` | Fetching and applying Mojang resource packs, with an optional `cli` feature |
| `camera`, `renderer` | Camera and rendering |
| `worldgen`, `worldtick` | World generation and ticking |
| `network`, `server` | Protocol and server |
| `core` | Facade that pulls in the crates above through feature flags |

Each crate defines its own `thiserror` error enum with error codes; `anyhow` is
not used.

Enable crates on `bwr-core` through features; `full` enables all of them and
`downloader-cli` adds the downloader's command line:

```sh
cargo build -p bwr-core --features renderer
```

## Build

Build the WebAssembly module:

```sh
cargo build --release --target wasm32-unknown-unknown
```

Generate the JavaScript bindings with wasm-bindgen:

```sh
wasm-bindgen --target web --out-dir pkg target/wasm32-unknown-unknown/release/block_world_renderer.wasm
```

## Test

```sh
cargo test --workspace
```
