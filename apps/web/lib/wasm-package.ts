import type { DownloaderConfig } from "@/lib/downloader-config";
import type { Notifier } from "@/lib/downloader";
import { TranslatedError } from "@/lib/errors";
import { digest, fetchBytes } from "@/lib/http";

export interface WasmPackage {
  moduleUrl: string;
  wasmUrl: string;
}

interface PackageFile {
  path: string;
  bytes: Uint8Array;
  type: string;
}

export class WasmPackageDownloader {
  private ready: Promise<WasmPackage> | undefined;

  constructor(
    private readonly config: DownloaderConfig,
    private readonly notifier: Notifier,
  ) {}

  ensure(): Promise<WasmPackage> {
    this.ready ??= this.run().catch((error: unknown) => {
      this.ready = undefined;
      throw error;
    });
    return this.ready;
  }

  private locate(version: string): WasmPackage {
    const { config } = this;
    return {
      moduleUrl: config.packagePath(version, config.glueFile),
      wasmUrl: config.packagePath(version, config.wasmFile),
    };
  }

  private async complete(cache: Cache, version: string): Promise<boolean> {
    const { config } = this;
    const files = [config.glueFile, config.wasmFile];
    const found = await Promise.all(
      files.map((file) => cache.match(config.packageUrl(version, file))),
    );
    return found.every((response) => response !== undefined);
  }

  private async latestChecksum(): Promise<string | null> {
    try {
      const bytes = await fetchBytes(
        this.config.releaseUrl(this.config.checksumFile),
      );
      const checksum = new TextDecoder().decode(bytes).trim();
      return this.config.checksumPattern.test(checksum) ? checksum : null;
    } catch {
      return null;
    }
  }

  private async run(): Promise<WasmPackage> {
    const { config } = this;
    const cache = await caches.open(config.cacheName);
    const stored = localStorage.getItem(config.checksumKey);
    const latest = await this.latestChecksum();
    if (latest !== null && latest !== stored) {
      try {
        await this.install(cache, latest);
        return this.locate(latest);
      } catch (error) {
        if (stored !== null && (await this.complete(cache, stored))) {
          return this.locate(stored);
        }
        throw error;
      }
    }
    const current = latest ?? stored;
    if (current !== null && (await this.complete(cache, current))) {
      return this.locate(current);
    }
    if (latest === null) {
      throw new TranslatedError("errors.packageUnavailable");
    }
    await this.install(cache, latest);
    return this.locate(latest);
  }

  private download(
    name: string,
    onFile?: (fraction: number) => void,
  ): Promise<Uint8Array> {
    return fetchBytes(
      this.config.releaseUrl(name),
      onFile && ((received, total) => onFile(Math.min(received / total, 1))),
    );
  }

  private async install(cache: Cache, checksum: string): Promise<void> {
    const { config, notifier } = this;
    notifier.begin();
    try {
      notifier.update({ step: "file", file: config.glueFile });
      const glue = await this.download(config.glueFile, (fileFraction) =>
        notifier.update({ fileFraction }),
      );
      const snippets = [
        ...new TextDecoder()
          .decode(glue)
          .matchAll(/["'](\.\/snippets\/[^"']+)["']/g),
      ].map((match) => match[1].replace(/^\.\//, ""));
      const totalFiles = 2 + snippets.length;
      const start = (index: number, file: string) =>
        notifier.update({
          step: "file",
          file,
          fraction: index / totalFiles,
          fileFraction: 0,
        });
      const report = (index: number) => (fileFraction: number) =>
        notifier.update({
          fileFraction,
          fraction: (index + fileFraction) / totalFiles,
        });
      start(1, config.wasmFile);
      const wasm = await this.download(config.wasmFile, report(1));
      notifier.update({ step: "verify" });
      if ((await digest("SHA-256", wasm)) !== checksum) {
        throw new TranslatedError("errors.checksumMismatch", {
          file: config.wasmFile,
        });
      }
      const files: PackageFile[] = [];
      for (const [index, snippet] of snippets.entries()) {
        const name = snippet.split("/").pop() ?? snippet;
        start(2 + index, name);
        files.push({
          path: snippet,
          bytes: await this.download(name, report(2 + index)),
          type: "text/javascript",
        });
      }
      files.push({ path: config.glueFile, bytes: glue, type: "text/javascript" });
      files.push({ path: config.wasmFile, bytes: wasm, type: "application/wasm" });
      for (const file of files) {
        await cache.put(
          config.packageUrl(checksum, file.path),
          new Response(file.bytes as BodyInit, {
            headers: { "Content-Type": file.type },
          }),
        );
      }
      localStorage.setItem(config.checksumKey, checksum);
      await this.prune(cache, checksum);
      notifier.update({ fraction: 1, fileFraction: 1 });
    } finally {
      notifier.end();
    }
  }

  private async prune(cache: Cache, keep: string): Promise<void> {
    const prefix = this.config.packagePrefix(keep);
    const requests = await cache.keys();
    await Promise.all(
      requests
        .filter((request) => !request.url.startsWith(prefix))
        .map((request) => cache.delete(request)),
    );
  }
}
