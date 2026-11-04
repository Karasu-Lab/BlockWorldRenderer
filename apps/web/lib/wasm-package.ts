import type { DownloaderConfig } from "@/lib/downloader-config";
import type { Notifier } from "@/lib/downloader";
import { TranslatedError } from "@/lib/errors";
import { digest, fetchBytes } from "@/lib/http";

export class WasmPackageDownloader {
  private ready: Promise<void> | undefined;

  constructor(
    private readonly config: DownloaderConfig,
    private readonly notifier: Notifier,
  ) {}

  ensure(): Promise<void> {
    this.ready ??= this.run().catch((error: unknown) => {
      this.ready = undefined;
      throw error;
    });
    return this.ready;
  }

  private async run(): Promise<void> {
    const { config } = this;
    const cache = await caches.open(config.cacheName);
    const cached = async () =>
      (await cache.match(config.packageUrl(config.wasmFile))) !== undefined;
    let checksum: string | null = null;
    try {
      checksum = new TextDecoder()
        .decode(await fetchBytes(config.releaseUrl(config.checksumFile)))
        .trim();
    } catch {}
    if (checksum === null) {
      if (await cached()) return;
      throw new TranslatedError("errors.packageUnavailable");
    }
    if (
      localStorage.getItem(config.checksumKey) === checksum &&
      (await cached())
    ) {
      return;
    }
    try {
      await this.install(cache, checksum);
    } catch (error) {
      if (await cached()) return;
      throw error;
    }
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
      const put = (path: string, bytes: Uint8Array, type: string) =>
        cache.put(
          config.packageUrl(path),
          new Response(bytes as BodyInit, { headers: { "Content-Type": type } }),
        );
      await put(config.glueFile, glue, "text/javascript");
      await put(config.wasmFile, wasm, "application/wasm");
      for (const [index, snippet] of snippets.entries()) {
        const name = snippet.split("/").pop() ?? snippet;
        start(2 + index, name);
        await put(
          snippet,
          await this.download(name, report(2 + index)),
          "text/javascript",
        );
      }
      localStorage.setItem(config.checksumKey, checksum);
      notifier.update({ fraction: 1, fileFraction: 1 });
    } finally {
      notifier.end();
    }
  }
}
