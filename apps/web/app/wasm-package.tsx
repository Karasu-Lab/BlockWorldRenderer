"use client";

import { useEffect } from "react";
import { useDownloader } from "@/lib/downloader";

export function WasmPackage() {
  const { serviceWorker, wasmPackage } = useDownloader();

  useEffect(() => {
    serviceWorker
      .ready()
      .then(() => wasmPackage.ensure())
      .catch((error: unknown) =>
        console.error("wasm package download failed", error),
      );
  }, [serviceWorker, wasmPackage]);

  return null;
}
