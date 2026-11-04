export class DownloaderConfig {
  constructor(
    readonly releaseBase: string,
    readonly cacheName: string,
    readonly checksumKey: string,
    readonly glueFile: string,
    readonly wasmFile: string,
    readonly basePath: string,
    readonly attempts = 3,
    readonly retryDelayMs = 500,
    readonly concurrency = 32,
    readonly reportIntervalMs = 100,
  ) {}

  static fromEnv(): DownloaderConfig {
    return new DownloaderConfig(
      "https://api.karasu256.com/api/proxy/github/Karasu-Lab/BlockWorldRenderer/releases/latest/download",
      "block-world-renderer-wasm-v1",
      "block-world-renderer-wasm-checksum",
      "block_world_renderer.js",
      "block_world_renderer.wasm",
      process.env.NEXT_PUBLIC_BASE_PATH ?? "",
    );
  }

  get checksumFile(): string {
    return `${this.wasmFile}.sha256`;
  }

  get moduleUrl(): string {
    return `${this.basePath}/wasm/${this.glueFile}`;
  }

  get wasmUrl(): string {
    return `${this.basePath}/wasm/${this.wasmFile}`;
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

  packageUrl(path: string): string {
    return `${location.origin}${this.basePath}/wasm/${path}`;
  }
}
