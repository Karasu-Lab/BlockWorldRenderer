wasm_rustflags := '--cfg getrandom_backend="wasm_js" --cfg tokio_unstable -C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=--shared-memory -C link-arg=--import-memory -C link-arg=--max-memory=4294967296 -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base'

web-wasm:
    env RUSTFLAGS='{{wasm_rustflags}}' cargo build --release --target wasm32-unknown-unknown -Zbuild-std=std,panic_abort
    wasm-bindgen --target web --out-dir pkg target/wasm32-unknown-unknown/release/block_world_renderer.wasm
    mv pkg/block_world_renderer_bg.wasm pkg/block_world_renderer.wasm
    mv pkg/block_world_renderer_bg.wasm.d.ts pkg/block_world_renderer.wasm.d.ts
    sed -i 's/block_world_renderer_bg\.wasm/block_world_renderer.wasm/g' pkg/block_world_renderer.js
    rm -rf apps/web/public/wasm
    cp -r pkg apps/web/public/wasm

web-dev:
    pnpm --dir apps/web run dev

web-build:
    pnpm --dir apps/web run build
