import type { DownloadStep, Notifier } from "@/lib/downloader";
import type { DownloaderConfig } from "@/lib/downloader-config";
import { TranslatedError } from "@/lib/errors";
import { digest, fetchBytes, sleep } from "@/lib/http";

export interface StoreFile {
  url: string;
  key: string;
  size: number;
  expected: string;
}

export class AssetDownloader {
  constructor(
    private readonly config: DownloaderConfig,
    private readonly notifier: Notifier,
  ) {}

  async fetch(url: string, expected?: string | null): Promise<Uint8Array> {
    const { notifier } = this;
    notifier.update({
      step: "file",
      file: url.split("/").pop() ?? url,
      fileFraction: 0,
    });
    const report = this.throttled((received: number, total: number) =>
      notifier.update({ fileFraction: Math.min(received / total, 1) }),
    );
    const bytes = await this.retry(async () => {
      const bytes = await fetchBytes(url, report);
      await this.verify(url, bytes, expected);
      return bytes;
    });
    notifier.update({ fileFraction: 1 });
    return bytes;
  }

  async store(cacheName: string, files: StoreFile[]): Promise<void> {
    const { notifier, config } = this;
    const cache = await caches.open(cacheName);
    const total = files.reduce((sum, file) => sum + file.size, 0);
    const received = files.map(() => 0);
    const started = files.map(() => false);
    const finished = files.map(() => false);
    let done = 0;
    let failed = false;

    const publish = () => {
      const current = files.findIndex(
        (_, index) => started[index] && !finished[index],
      );
      const size = current >= 0 ? files[current].size : 0;
      notifier.update({
        step: "assets",
        done,
        total,
        fraction: total > 0 ? done / total : 0,
        ...(size > 0 ? { fileFraction: received[current] / size } : {}),
      });
    };
    const publishSoon = this.throttled(publish);
    const receive = (index: number, bytes: number) => {
      const size = files[index].size;
      done += Math.min(bytes, size) - Math.min(received[index], size);
      received[index] = bytes;
      publishSoon();
    };

    const download = async (index: number) => {
      const file = files[index];
      started[index] = true;
      const bytes = await this.retry(async () => {
        receive(index, 0);
        const bytes = await fetchBytes(file.url, (count) =>
          receive(index, count),
        );
        await this.verify(file.url, bytes, file.expected);
        return bytes;
      });
      await cache.put(file.key, new Response(bytes as BodyInit));
      receive(index, file.size);
      finished[index] = true;
      publish();
    };

    publish();
    let next = 0;
    const worker = async () => {
      while (!failed && next < files.length) {
        const index = next++;
        try {
          await download(index);
        } catch (error) {
          failed = true;
          throw error;
        }
      }
    };
    await Promise.all(
      Array.from({ length: Math.min(config.concurrency, files.length) }, worker),
    );
    publish();
  }

  progress(
    stage: DownloadStep,
    done: number,
    total: number,
    _status: string,
    fileDone: number,
    fileTotal: number,
  ): void {
    this.notifier.update({
      step: stage,
      done,
      total,
      fraction: total > 0 ? done / total : 0,
      ...(fileTotal > 0 ? { fileFraction: fileDone / fileTotal } : {}),
    });
  }

  private async verify(
    url: string,
    bytes: Uint8Array,
    expected?: string | null,
  ): Promise<void> {
    if (expected && (await digest("SHA-1", bytes)) !== expected) {
      throw new TranslatedError("errors.hashMismatch", { url });
    }
  }

  private async retry<T>(task: () => Promise<T>): Promise<T> {
    const { attempts, retryDelayMs } = this.config;
    let last: unknown;
    for (let attempt = 0; attempt < attempts; attempt++) {
      if (attempt > 0) await sleep(retryDelayMs * attempt);
      try {
        return await task();
      } catch (error) {
        last = error;
      }
    }
    throw last;
  }

  private throttled<A extends unknown[]>(
    run: (...args: A) => void,
  ): (...args: A) => void {
    let last = 0;
    return (...args) => {
      const now = performance.now();
      if (now - last < this.config.reportIntervalMs) return;
      last = now;
      run(...args);
    };
  }
}
