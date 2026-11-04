export interface WorldHandle {
  stop(): void;
  set_ui_enabled(enabled: boolean): void;
  set_camera(
    x: number,
    y: number,
    z: number,
    yawDeg: number,
    pitchDeg: number,
  ): void;
  debug_lines(): string[];
  free(): void;
}

/** One entity of a replay or a world save, as the wasm side reports it. */
export interface EntityInfo {
  id: number | null;
  uuid: string | null;
  name: string | null;
  isPlayer: boolean;
  position: [number, number, number] | null;
  yaw: number;
  pitch: number;
}

export interface ReplayHandle {
  duration_ms(): number;
  position_ms(): number;
  mc_version(): string;
  protocol(): number | undefined;
  apply_until(targetMs: number): void;
  entities_json(): string;
  free(): void;
}

export interface SaveHandle {
  seed(): bigint;
  players_json(): string;
  free(): void;
}

interface RendererModule {
  default(input?: { module_or_path: string }): Promise<unknown>;
  initThreadPool(threads: number): Promise<unknown>;
  start(
    canvas: HTMLCanvasElement,
    versionManifest?: string,
    resourcesBase?: string,
    seed?: bigint,
    camera?: Float64Array,
  ): Promise<WorldHandle>;
  start_replay(
    canvas: HTMLCanvasElement,
    versionManifest: string | undefined,
    resourcesBase: string | undefined,
    seed: bigint | undefined,
    camera: Float64Array | undefined,
    replay: ReplayHandle,
  ): Promise<WorldHandle>;
  start_save(
    canvas: HTMLCanvasElement,
    versionManifest: string | undefined,
    resourcesBase: string | undefined,
    camera: Float64Array | undefined,
    save: SaveHandle,
  ): Promise<WorldHandle>;
  open_replay(bytes: Uint8Array): ReplayHandle;
  open_save(bytes: Uint8Array): SaveHandle;
  load_progress(): number;
  load_file_progress(): number;
  load_status(): string;
  renderer_available(): boolean;
}

export interface LoadProgress {
  available: boolean;
  /** Overall progress of the load, as a fraction. */
  fraction: number;
  status: string;
  /**
   * The progress of the one file currently downloading, as a
   * fraction, or null while no file download is in flight.
   */
  fileFraction: number | null;
}

/** A camera the providers describe: position plus look, in degrees. */
export interface CameraSpec {
  position: [number, number, number];
  yaw: number;
  pitch: number;
}

const BASE_PATH = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
const MODULE_URL = `${BASE_PATH}/wasm/block_world_renderer.js`;
const WASM_URL = `${BASE_PATH}/wasm/block_world_renderer.wasm`;

/** The URL a file under `public/replays` is served at. */
export function replayUrl(path: string): string {
  return `${BASE_PATH}/replays/${path}`;
}

/** The URL a user-supplied file path (a world save zip) is served at. */
export function publicUrl(path: string): string {
  return path.startsWith("/") ? `${BASE_PATH}${path}` : `${BASE_PATH}/${path}`;
}

export function hasGraphicsApi(): boolean {
  if (typeof navigator === "undefined") return false;
  if ("gpu" in navigator) return true;
  return document.createElement("canvas").getContext("webgl2") !== null;
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

/**
 * The service worker serves the game resources under the site's own
 * origin, so the world must not start until it controls the page;
 * without one, start anyway rather than wait forever.
 */
async function waitForServiceWorker(): Promise<void> {
  if (!("serviceWorker" in navigator) || navigator.serviceWorker.controller) {
    return;
  }
  await new Promise<void>((resolve) => {
    const finish = () => {
      navigator.serviceWorker.removeEventListener("controllerchange", finish);
      resolve();
    };
    navigator.serviceWorker.addEventListener("controllerchange", finish);
    setTimeout(finish, 3000);
  });
}

export interface StartOptions {
  seed?: bigint;
  camera?: CameraSpec;
  onProgress?: (progress: LoadProgress) => void;
}

function cameraArray(camera?: CameraSpec): Float64Array | undefined {
  if (!camera) return undefined;
  return new Float64Array([
    camera.position[0],
    camera.position[1],
    camera.position[2],
    camera.yaw,
    camera.pitch,
  ]);
}

async function startWith(
  onProgress: ((progress: LoadProgress) => void) | undefined,
  start: (renderer: RendererModule, resourcesBase: string) => Promise<WorldHandle>,
): Promise<WorldHandle> {
  const renderer = await loadRenderer();
  await waitForServiceWorker();
  const resourcesBase =
    process.env.NEXT_PUBLIC_RESOURCES_BASE_URL ??
    new URL(`${BASE_PATH}/resources`, location.origin).href;
  const poll = setInterval(() => {
    const fileProgress = renderer.load_file_progress();
    onProgress?.({
      available: renderer.renderer_available(),
      fraction: renderer.load_progress(),
      status: renderer.load_status(),
      fileFraction: fileProgress >= 0 ? fileProgress : null,
    });
  }, 200);
  try {
    return await start(renderer, resourcesBase);
  } finally {
    clearInterval(poll);
  }
}

export async function startWorld(
  canvas: HTMLCanvasElement,
  options: StartOptions = {},
): Promise<WorldHandle> {
  return startWith(options.onProgress, (renderer, resourcesBase) =>
    renderer.start(
      canvas,
      process.env.NEXT_PUBLIC_VERSION_MANIFEST_URL,
      resourcesBase,
      options.seed,
      cameraArray(options.camera),
    ),
  );
}

export async function startReplayWorld(
  canvas: HTMLCanvasElement,
  replay: ReplayHandle,
  options: StartOptions = {},
): Promise<WorldHandle> {
  return startWith(options.onProgress, (renderer, resourcesBase) =>
    renderer.start_replay(
      canvas,
      process.env.NEXT_PUBLIC_VERSION_MANIFEST_URL,
      resourcesBase,
      options.seed,
      cameraArray(options.camera),
      replay,
    ),
  );
}

export async function startSaveWorld(
  canvas: HTMLCanvasElement,
  save: SaveHandle,
  options: StartOptions = {},
): Promise<WorldHandle> {
  return startWith(options.onProgress, (renderer, resourcesBase) =>
    renderer.start_save(
      canvas,
      process.env.NEXT_PUBLIC_VERSION_MANIFEST_URL,
      resourcesBase,
      cameraArray(options.camera),
      save,
    ),
  );
}

/** Opens a replay file's bytes; throws with the parser's message. */
export async function openReplay(bytes: Uint8Array): Promise<ReplayHandle> {
  const renderer = await loadRenderer();
  return renderer.open_replay(bytes);
}

/** Opens a world save zip's bytes; throws with the parser's message. */
export async function openSave(bytes: Uint8Array): Promise<SaveHandle> {
  const renderer = await loadRenderer();
  return renderer.open_save(bytes);
}

export function parseEntities(json: string): EntityInfo[] {
  return JSON.parse(json) as EntityInfo[];
}
