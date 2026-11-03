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
  start(
    canvas: HTMLCanvasElement,
    versionManifest?: string,
    resourcesBase?: string,
  ): Promise<WorldHandle>;
  /** How far the assets' download has got, 0 to 1. */
  load_progress(): number;
  /** What the download is doing. */
  load_status(): string;
}

/** How far the assets' download has got. */
export interface LoadProgress {
  fraction: number;
  status: string;
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
export async function startWorld(
  canvas: HTMLCanvasElement,
  onProgress: (progress: LoadProgress) => void = () => {},
): Promise<WorldHandle> {
  const renderer: RendererModule = await import(
    /* webpackIgnore: true */ /* turbopackIgnore: true */ MODULE_URL
  );
  await renderer.default({ module_or_path: WASM_URL });
  await renderer.initThreadPool(Math.max(2, navigator.hardwareConcurrency || 2));
  // The asset servers do not answer other origins, so a deployment points
  // these at a mirror; unset, the game's own servers are used.
  const poll = setInterval(
    () => onProgress({ fraction: renderer.load_progress(), status: renderer.load_status() }),
    200,
  );
  try {
    return await renderer.start(
      canvas,
      process.env.NEXT_PUBLIC_VERSION_MANIFEST_URL,
      process.env.NEXT_PUBLIC_RESOURCES_BASE_URL,
    );
  } finally {
    clearInterval(poll);
  }
}
