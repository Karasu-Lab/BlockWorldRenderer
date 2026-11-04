"use client";

import { useEffect } from "react";
import { useDownloader } from "@/lib/downloader";

export function ServiceWorkerRegistration() {
  const { serviceWorker } = useDownloader();

  useEffect(() => {
    serviceWorker
      .ready()
      .catch((error: unknown) => console.error("service worker failed", error));
  }, [serviceWorker]);

  return null;
}
