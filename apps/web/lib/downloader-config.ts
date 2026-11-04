export class DownloaderConfig {
  constructor(
    readonly releaseBase: string,
    readonly cacheName: string,
    readonly cachePrefix: string,
    readonly checksumKey: string,
    readonly glueFile: string,
    readonly wasmFile: string,
    readonly basePath: string,
    readonly reloadKey: string,
    readonly serviceWorkerFile: string,
    readonly resourceProxy: string,
    readonly checksumPattern: RegExp,
    readonly attempts = 3,
    readonly retryDelayMs = 500,
    readonly concurrency = 32,
    readonly reportIntervalMs = 100,
    readonly controllerTimeoutMs = 3000,
  ) {}

  static fromEnv(): DownloaderConfig {
    return new DownloaderConfig(
      "https://api.karasu256.com/api/proxy/github/Karasu-Lab/BlockWorldRenderer/releases/latest/download",
      "block-world-renderer-wasm-v1",
      "block-world-renderer-",
      "block-world-renderer-wasm-checksum",
      "block_world_renderer.js",
      "block_world_renderer.wasm",
      process.env.NEXT_PUBLIC_BASE_PATH ?? "",
      "block-world-renderer-sw-reloaded",
      "sw.js",
      "https://api.karasu256.com/api/proxy/mojang",
      /^[0-9a-f]{64}$/,
    );
  }

  get checksumFile(): string {
    return `${this.wasmFile}.sha256`;
  }

  get serviceWorkerScope(): string {
    return `${this.basePath}/`;
  }

  get serviceWorkerUrl(): string {
    const query = new URLSearchParams({
      wasmCache: this.cacheName,
      cachePrefix: this.cachePrefix,
      resourceProxy: this.resourceProxy,
    });
    return `${this.basePath}/${this.serviceWorkerFile}?${query}`;
  }

  replayUrl(path: string): string {
    return `${this.basePath}/replays/${path}`;
  }

  publicUrl(path: string): string {
    return path.startsWith("/")
      ? `${this.basePath}${path}`
      : `${this.basePath}/${path}`;
  }

  releaseUrl(name: string): string {
    return `${this.releaseBase}/${name}`;
  }

  packagePath(version: string, path: string): string {
    return `${this.basePath}/wasm/${version}/${path}`;
  }

  packageUrl(version: string, path: string): string {
    return `${location.origin}${this.packagePath(version, path)}`;
  }

  packagePrefix(version: string): string {
    return this.packageUrl(version, "");
  }
}
