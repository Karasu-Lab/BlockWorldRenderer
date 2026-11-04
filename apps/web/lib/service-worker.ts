import type { DownloaderConfig } from "@/lib/downloader-config";

export class ServiceWorkerGate {
  private settling: Promise<void> | undefined;

  constructor(private readonly config: DownloaderConfig) {}

  ready(): Promise<void> {
    this.settling ??= this.settle().catch((error: unknown) => {
      this.settling = undefined;
      throw error;
    });
    return this.settling;
  }

  private settled(container: ServiceWorkerContainer): boolean {
    return container.controller !== null && window.crossOriginIsolated;
  }

  private controllerChange(container: ServiceWorkerContainer): Promise<void> {
    return new Promise((resolve) => {
      const finish = () => {
        container.removeEventListener("controllerchange", finish);
        clearTimeout(timer);
        resolve();
      };
      const timer = setTimeout(finish, this.config.controllerTimeoutMs);
      container.addEventListener("controllerchange", finish);
    });
  }

  private async settle(): Promise<void> {
    if (!("serviceWorker" in navigator)) return;
    const { config } = this;
    const container = navigator.serviceWorker;
    await container.register(config.serviceWorkerUrl, {
      scope: config.serviceWorkerScope,
      updateViaCache: "none",
    });
    await container.ready;
    if (!container.controller) await this.controllerChange(container);
    if (this.settled(container)) {
      sessionStorage.removeItem(config.reloadKey);
      return;
    }
    if (sessionStorage.getItem(config.reloadKey)) return;
    sessionStorage.setItem(config.reloadKey, "1");
    window.location.reload();
    await new Promise<never>(() => {});
  }
}
