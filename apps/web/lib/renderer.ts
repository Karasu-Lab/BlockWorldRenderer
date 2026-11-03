/** What `wasm-bindgen` exports for the renderer's `WorldHandle`. */
export interface WorldHandle {
  stop(): void;
  set_ui_enabled(enabled: boolean): void;
  debug_lines(): string[];
  free(): void;
}

interface RendererModule {
  default(input?: { module_or_path: string }): Promise<unknown>;
  /** Starts the Web Workers that chunk generation runs on. */
  initThreadPool(threads: number): Promise<unknown>;
  start(canvas: HTMLCanvasElement): Promise<WorldHandle>;
}

const MODULE_URL = "/wasm/block_world_renderer.js";
const WASM_URL = "/wasm/block_world_renderer_bg.wasm";

export function hasWebGpu(): boolean {
  return typeof navigator !== "undefined" && "gpu" in navigator;
}

/**
 * Loads the wasm package from `public/wasm`, where `scripts/sync-wasm.mjs`
 * puts it, and starts the fixed world on `canvas`.
 */
export async function startWorld(canvas: HTMLCanvasElement): Promise<WorldHandle> {
  const renderer: RendererModule = await import(
    /* webpackIgnore: true */ /* turbopackIgnore: true */ MODULE_URL
  );
  await renderer.default({ module_or_path: WASM_URL });
  await renderer.initThreadPool(Math.max(2, navigator.hardwareConcurrency || 2));
  return renderer.start(canvas);
}
