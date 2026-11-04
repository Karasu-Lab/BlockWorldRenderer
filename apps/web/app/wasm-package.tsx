"use client";

import { useEffect } from "react";
import { useDownloader } from "@/lib/downloader";

export function WasmPackage() {
  const { wasmPackage } = useDownloader();

  useEffect(() => {
    wasmPackage
      .ensure()
      .catch((error: unknown) =>
        console.error("wasm package download failed", error),
      );
  }, [wasmPackage]);

  return null;
}
