export interface WorldHandle {
  stop(): void;
  set_ui_enabled(enabled: boolean): void;
  debug_lines(): string[];
  free(): void;
}

interface RendererModule {
  default(input?: { module_or_path: string }): Promise<unknown>;
  initThreadPool(threads: number): Promise<unknown>;
  start(
    canvas: HTMLCanvasElement,
    versionManifest?: string,
    resourcesBase?: string,
  ): Promise<WorldHandle>;
  load_progress(): number;
  load_status(): string;
  renderer_available(): boolean;
}

export interface LoadProgress {
  available: boolean;
  fraction: number;
  status: string;
}

const BASE_PATH = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
const MODULE_URL = `${BASE_PATH}/wasm/block_world_renderer.js`;
const WASM_URL = `${BASE_PATH}/wasm/block_world_renderer.wasm`;

export function hasWebGpu(): boolean {
  return typeof navigator !== "undefined" && "gpu" in navigator;
}

let loading: Promise<RendererModule> | undefined;

function loadRenderer(): Promise<RendererModule> {
  loading ??= (async () => {
    const renderer: RendererModule = await import(
      /* webpackIgnore: true */ /* turbopackIgnore: true */ MODULE_URL
    );
    await renderer.default({ module_or_path: WASM_URL });
    await renderer.initThreadPool(
      Math.max(2, navigator.hardwareConcurrency || 2),
    );
    return renderer;
  })().catch((error: unknown) => {
    loading = undefined;
    throw error;
  });
  return loading;
}

export async function startWorld(
  canvas: HTMLCanvasElement,
  onProgress: (progress: LoadProgress) => void = () => {},
): Promise<WorldHandle> {
  const renderer = await loadRenderer();
  const resourcesBase =
    process.env.NEXT_PUBLIC_RESOURCES_BASE_URL ??
    new URL(`${BASE_PATH}/resources`, location.origin).href;
  const poll = setInterval(
    () =>
      onProgress({
        available: renderer.renderer_available(),
        fraction: renderer.load_progress(),
        status: renderer.load_status(),
      }),
    200,
  );
  try {
    return await renderer.start(
      canvas,
      process.env.NEXT_PUBLIC_VERSION_MANIFEST_URL,
      resourcesBase,
    );
  } finally {
    clearInterval(poll);
  }
}
