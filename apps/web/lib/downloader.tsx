"use client";

import {
  createContext,
  useContext,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { AssetDownloader } from "@/lib/asset-downloader";
import { DownloaderConfig } from "@/lib/downloader-config";
import { PageCache } from "@/lib/page-cache";
import { ServiceWorkerGate } from "@/lib/service-worker";
import { WasmPackageDownloader } from "@/lib/wasm-package";

export interface DownloadProgress {
  active: boolean;
  fraction: number;
  fileFraction: number;
  step: DownloadStep | null;
  file: string;
  done: number;
  total: number;
  wasmVersion: string | null;
}

export type DownloadStep =
  | "file"
  | "verify"
  | "info"
  | "assets"
  | "client"
  | "extract"
  | "save";

const IDLE: DownloadProgress = {
  active: false,
  fraction: 0,
  fileFraction: 0,
  step: null,
  file: "",
  done: 0,
  total: 0,
  wasmVersion: null,
};

const RESET = {
  fraction: 0,
  fileFraction: 0,
  step: null,
  file: "",
  done: 0,
  total: 0,
} satisfies Partial<DownloadProgress>;

export class Notifier {
  private snapshot = IDLE;
  private runs = 0;
  private listeners = new Set<() => void>();

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): DownloadProgress => this.snapshot;

  update(change: Partial<DownloadProgress>): void {
    const next = { ...this.snapshot, ...change };
    const same = (Object.keys(next) as (keyof DownloadProgress)[]).every(
      (key) => next[key] === this.snapshot[key],
    );
    if (same) return;
    this.snapshot = next;
    this.listeners.forEach((listener) => listener());
  }

  begin(): void {
    if (this.runs++ === 0) {
      this.update({ active: true, ...RESET });
    }
  }

  end(): void {
    if (--this.runs === 0) {
      this.update({ active: false, ...RESET });
    }
  }
}

export interface Downloader {
  notifier: Notifier;
  config: DownloaderConfig;
  serviceWorker: ServiceWorkerGate;
  wasmPackage: WasmPackageDownloader;
  assets: AssetDownloader;
  cache: PageCache;
}

const DownloaderContext = createContext<Downloader | null>(null);

export function DownloaderProvider({
  config: injected,
  children,
}: {
  config?: DownloaderConfig;
  children: ReactNode;
}) {
  const [downloader] = useState<Downloader>(() => {
    const config = injected ?? DownloaderConfig.fromEnv();
    const notifier = new Notifier();
    return {
      config,
      notifier,
      serviceWorker: new ServiceWorkerGate(config),
      wasmPackage: new WasmPackageDownloader(config, notifier),
      assets: new AssetDownloader(config, notifier),
      cache: new PageCache(config),
    };
  });

  return (
    <DownloaderContext.Provider value={downloader}>
      {children}
    </DownloaderContext.Provider>
  );
}

export function useDownloader(): Downloader {
  const value = useContext(DownloaderContext);
  if (!value) throw new Error("useDownloader outside DownloaderProvider");
  return value;
}

export function useDownloadProgress(): DownloadProgress {
  const { notifier } = useDownloader();
  return useSyncExternalStore(
    notifier.subscribe,
    notifier.getSnapshot,
    notifier.getSnapshot,
  );
}
