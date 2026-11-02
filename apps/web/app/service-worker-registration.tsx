"use client";

import { useEffect } from "react";

const RELOAD_KEY = "block-world-renderer-sw-reloaded";

export function ServiceWorkerRegistration() {
  useEffect(() => {
    if (!("serviceWorker" in navigator)) return;

    navigator.serviceWorker
      .register("/sw.js", { scope: "/", updateViaCache: "none" })
      .then(() => navigator.serviceWorker.ready)
      .then(() => {
        // The worker adds the isolation headers, so a page it did not serve
        // has to load once more.
        if (!window.crossOriginIsolated && !sessionStorage.getItem(RELOAD_KEY)) {
          sessionStorage.setItem(RELOAD_KEY, "1");
          window.location.reload();
        }
      })
      .catch((error: unknown) => console.error("service worker failed", error));
  }, []);

  return null;
}
