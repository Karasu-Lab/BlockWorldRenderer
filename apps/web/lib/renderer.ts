import type { AssetDownloader } from "@/lib/asset-downloader";
import type { Downloader } from "@/lib/downloader";
import type { PageCache } from "@/lib/page-cache";

export type RendererDeps = Downloader;

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
    versionManifest: string | undefined,
    resourcesBase: string | undefined,
    seed: bigint | undefined,
    camera: Float64Array | undefined,
    notifier: AssetDownloader,
    cache: PageCache,
  ): Promise<WorldHandle>;
  start_replay(
    canvas: HTMLCanvasElement,
    versionManifest: string | undefined,
    resourcesBase: string | undefined,
    seed: bigint | undefined,
    camera: Float64Array | undefined,
    replay: ReplayHandle,
    notifier: AssetDownloader,
    cache: PageCache,
  ): Promise<WorldHandle>;
  start_save(
    canvas: HTMLCanvasElement,
    versionManifest: string | undefined,
    resourcesBase: string | undefined,
    camera: Float64Array | undefined,
    save: SaveHandle,
    notifier: AssetDownloader,
    cache: PageCache,
  ): Promise<WorldHandle>;
  open_replay(bytes: Uint8Array): ReplayHandle;
  open_save(bytes: Uint8Array): SaveHandle;
  package_version(): string;
}

export interface CameraSpec {
  position: [number, number, number];
  yaw: number;
  pitch: number;
}

export function hasGraphicsApi(): boolean {
  if (typeof navigator === "undefined") return false;
  if ("gpu" in navigator) return true;
  return document.createElement("canvas").getContext("webgl2") !== null;
}

let loading: Promise<RendererModule> | undefined;

function loadRenderer(deps: RendererDeps): Promise<RendererModule> {
  loading ??= (async () => {
    await deps.serviceWorker.ready();
    const { moduleUrl, wasmUrl } = await deps.wasmPackage.ensure();
    const renderer: RendererModule = await import(
      /* webpackIgnore: true */ /* turbopackIgnore: true */ moduleUrl
    );
    await renderer.default({ module_or_path: wasmUrl });
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

export interface StartOptions {
  seed?: bigint;
  camera?: CameraSpec;
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
  deps: RendererDeps,
  start: (
    renderer: RendererModule,
    resourcesBase: string,
    notifier: AssetDownloader,
    cache: PageCache,
  ) => Promise<WorldHandle>,
): Promise<WorldHandle> {
  const renderer = await loadRenderer(deps);
  const { notifier, config, assets, cache } = deps;
  const resourcesBase =
    process.env.NEXT_PUBLIC_RESOURCES_BASE_URL ??
    new URL(`${config.basePath}/resources`, location.origin).href;
  notifier.update({
    wasmVersion:
      typeof renderer.package_version === "function"
        ? renderer.package_version()
        : null,
  });
  return start(renderer, resourcesBase, assets, cache);
}

export async function startWorld(
  deps: RendererDeps,
  canvas: HTMLCanvasElement,
  options: StartOptions = {},
): Promise<WorldHandle> {
  return startWith(deps, (renderer, resourcesBase, notifier, cache) =>
    renderer.start(
      canvas,
      process.env.NEXT_PUBLIC_VERSION_MANIFEST_URL,
      resourcesBase,
      options.seed,
      cameraArray(options.camera),
      notifier,
      cache,
    ),
  );
}

export async function startReplayWorld(
  deps: RendererDeps,
  canvas: HTMLCanvasElement,
  replay: ReplayHandle,
  options: StartOptions = {},
): Promise<WorldHandle> {
  return startWith(deps, (renderer, resourcesBase, notifier, cache) =>
    renderer.start_replay(
      canvas,
      process.env.NEXT_PUBLIC_VERSION_MANIFEST_URL,
      resourcesBase,
      options.seed,
      cameraArray(options.camera),
      replay,
      notifier,
      cache,
    ),
  );
}

export async function startSaveWorld(
  deps: RendererDeps,
  canvas: HTMLCanvasElement,
  save: SaveHandle,
  options: StartOptions = {},
): Promise<WorldHandle> {
  return startWith(deps, (renderer, resourcesBase, notifier, cache) =>
    renderer.start_save(
      canvas,
      process.env.NEXT_PUBLIC_VERSION_MANIFEST_URL,
      resourcesBase,
      cameraArray(options.camera),
      save,
      notifier,
      cache,
    ),
  );
}

export async function openReplay(
  deps: RendererDeps,
  bytes: Uint8Array,
): Promise<ReplayHandle> {
  const renderer = await loadRenderer(deps);
  return renderer.open_replay(bytes);
}

export async function openSave(
  deps: RendererDeps,
  bytes: Uint8Array,
): Promise<SaveHandle> {
  const renderer = await loadRenderer(deps);
  return renderer.open_save(bytes);
}

export function parseEntities(json: string): EntityInfo[] {
  return JSON.parse(json) as EntityInfo[];
}
