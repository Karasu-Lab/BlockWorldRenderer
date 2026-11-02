# Builds the wasm package and copies it into the web app.
web-wasm:
    cargo build --release --target wasm32-unknown-unknown
    wasm-bindgen --target web --out-dir pkg target/wasm32-unknown-unknown/release/block_world_renderer.wasm
    pnpm --dir apps/web run sync-wasm

web-dev: web-wasm
    pnpm --dir apps/web run dev

web-build: web-wasm
    pnpm --dir apps/web run build
