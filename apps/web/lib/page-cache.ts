import type { DownloaderConfig } from "@/lib/downloader-config";

export type CacheEntry = [key: string, bytes: Uint8Array];

export class PageCache {
  constructor(private readonly config: DownloaderConfig) {}

  async loadCache(cacheName: string, keys?: string[]): Promise<CacheEntry[]> {
    const cache = await caches.open(cacheName);
    const requests = keys ?? (await cache.keys()).map((request) => request.url);
    const entries = await this.pooled(requests, async (key) => {
      const response = await cache.match(key);
      if (!response) return null;
      return [key, new Uint8Array(await response.arrayBuffer())] as CacheEntry;
    });
    return entries.filter((entry): entry is CacheEntry => entry !== null);
  }

  async saveCache(cacheName: string, entries: CacheEntry[]): Promise<void> {
    const cache = await caches.open(cacheName);
    await this.pooled(entries, ([key, bytes]) =>
      cache.put(key, new Response(bytes as BodyInit)),
    );
  }

  private async pooled<T, R>(
    items: T[],
    task: (item: T) => Promise<R>,
  ): Promise<R[]> {
    const results = new Array<R>(items.length);
    let next = 0;
    const worker = async () => {
      while (next < items.length) {
        const index = next++;
        results[index] = await task(items[index]);
      }
    };
    await Promise.all(
      Array.from(
        { length: Math.min(this.config.concurrency, items.length) },
        worker,
      ),
    );
    return results;
  }
}
