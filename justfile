# The worker threads of the wasm build share one memory, which needs atomics, a
# shared imported memory and exported thread-local storage. tokio builds its
# OS-backed features for wasm only under `tokio_unstable` (the client's azalea-auth
# asks for `fs`). The dev shell sets
# RUSTFLAGS for the native libraries, so these are set inside it with `env`.
wasm_rustflags := '--cfg getrandom_backend="wasm_js" --cfg tokio_unstable -C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=--shared-memory -C link-arg=--import-memory -C link-arg=--max-memory=1073741824 -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base'

# Builds the wasm package into pkg/. The web app picks this local build up
# through its fetch-wasm script; without it, the app downloads the package
# from the latest GitHub Release instead.
web-wasm:
    env RUSTFLAGS='{{wasm_rustflags}}' cargo build --release --target wasm32-unknown-unknown -Zbuild-std=std,panic_abort
    wasm-bindgen --target web --out-dir pkg target/wasm32-unknown-unknown/release/block_world_renderer.wasm

web-dev:
    pnpm --dir apps/web run dev

web-build:
    pnpm --dir apps/web run build
