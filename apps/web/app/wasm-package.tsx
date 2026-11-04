"use client";

import { useEffect } from "react";

// The wasm package is published as individual files on the latest
// GitHub Release, and the client downloads it from there at runtime,
// so the site build has no dependency on the release. Release
// downloads send no CORS headers, so a direct fetch would be an
// opaque response whose bytes the page cannot read — the download
// goes through the API's proxy instead, which relays the release
// with CORS headers, the same route the game resources take. The
// files are put in the cache the service worker serves /wasm/ from,
// under the paths the package's own files reference.
const RELEASE_BASE =
  "https://api.karasu256.com/api/proxy/github/Karasu-Lab/BlockWorldRenderer/releases/latest/download";
// Must match WASM_CACHE in public/sw.js.
const WASM_CACHE = "block-world-renderer-wasm-v1";
const CHECKSUM_KEY = "block-world-renderer-wasm-checksum";
const GLUE_FILE = "block_world_renderer.js";
const WASM_FILE = "block_world_renderer.wasm";
const CHECKSUM_FILE = `${WASM_FILE}.sha256`;

const BASE_PATH = process.env.NEXT_PUBLIC_BASE_PATH ?? "";

function packageUrl(path: string): string {
  return `${location.origin}${BASE_PATH}/wasm/${path}`;
}

async function download(name: string): Promise<ArrayBuffer> {
  const response = await fetch(`${RELEASE_BASE}/${name}`);
  if (!response.ok) {
    throw new Error(`downloading ${name} failed with ${response.status}`);
  }
  return response.arrayBuffer();
}

async function sha256(bytes: ArrayBuffer): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

let ready: Promise<void> | undefined;

/**
 * Downloads the wasm package into the service worker's cache unless
 * the cached copy already matches the latest release, which the
 * release's checksum asset decides: the SHA-256 of the wasm file.
 * Single-flight; the renderer awaits this before importing the
 * package, and the component below starts it at mount.
 */
export function ensureWasmPackage(): Promise<void> {
  ready ??= (async () => {
    const cache = await caches.open(WASM_CACHE);
    let checksum: string | null = null;
    try {
      checksum = new TextDecoder()
        .decode(await download(CHECKSUM_FILE))
        .trim();
    } catch {
      // Offline or unreachable: fall back to whatever is cached.
    }
    if (
      checksum !== null &&
      localStorage.getItem(CHECKSUM_KEY) === checksum &&
      (await cache.match(packageUrl(WASM_FILE)))
    ) {
      return;
    }
    if (checksum === null) {
      if (await cache.match(packageUrl(WASM_FILE))) return;
      throw new Error("the wasm package is not cached and cannot be downloaded");
    }
    try {
      const glue = await download(GLUE_FILE);
      const glueText = new TextDecoder().decode(glue);
      const snippets = [
        ...glueText.matchAll(/["'](\.\/snippets\/[^"']+)["']/g),
      ].map((match) => match[1].replace(/^\.\//, ""));
      const wasm = await download(WASM_FILE);
      if ((await sha256(wasm)) !== checksum) {
        throw new Error(
          "the downloaded wasm does not match the release checksum",
        );
      }
      const put = (path: string, bytes: ArrayBuffer, type: string) =>
        cache.put(
          packageUrl(path),
          new Response(bytes, { headers: { "Content-Type": type } }),
        );
      await put(GLUE_FILE, glue, "text/javascript");
      await put(WASM_FILE, wasm, "application/wasm");
      for (const snippet of snippets) {
        const name = snippet.split("/").pop() ?? snippet;
        await put(snippet, await download(name), "text/javascript");
      }
      localStorage.setItem(CHECKSUM_KEY, checksum);
    } catch (error) {
      // A release whose files cannot be used — one that predates the
      // individual files, for example — must not break a cached
      // package that works: keep serving the cache.
      if (await cache.match(packageUrl(WASM_FILE))) return;
      throw error;
    }
  })().catch((error: unknown) => {
    ready = undefined;
    throw error;
  });
  return ready;
}

/** Starts the wasm package download as soon as the app mounts. */
export function WasmPackage() {
  useEffect(() => {
    ensureWasmPackage().catch((error: unknown) =>
      console.error("wasm package download failed", error),
    );
  }, []);

  return null;
}
