// Serves the wasm package and game resources from a cache, and adds the
// cross-origin isolation headers to what it serves, for hosts that cannot.

const VERSION = "v1";
const WASM_CACHE = `block-world-renderer-wasm-${VERSION}`;
const RESOURCE_CACHE = `block-world-renderer-resources-${VERSION}`;
const CACHES = [WASM_CACHE, RESOURCE_CACHE];

// Resource files are addressed by their hash, so a cached copy never goes stale.
const RESOURCE_HOSTS = new Set([
  "resources.download.minecraft.net",
  "piston-data.mojang.com",
  "piston-meta.mojang.com",
  "launchermeta.mojang.com",
]);

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      await Promise.all(
        names
          .filter((name) => name.startsWith("block-world-renderer-") && !CACHES.includes(name))
          .map((name) => caches.delete(name)),
      );
      await self.clients.claim();
    })(),
  );
});

function isolate(response) {
  // Opaque responses cannot be rebuilt, and carry no readable headers.
  if (response.status === 0 || response.type === "opaque") return response;
  const headers = new Headers(response.headers);
  headers.set("Cross-Origin-Embedder-Policy", "require-corp");
  headers.set("Cross-Origin-Opener-Policy", "same-origin");
  headers.set("Cross-Origin-Resource-Policy", "cross-origin");
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

async function cacheFirst(request, cacheName) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) await cache.put(request, response.clone());
  return response;
}

async function staleWhileRevalidate(request, cacheName) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(request);
  const update = fetch(request).then(async (response) => {
    if (response.ok) await cache.put(request, response.clone());
    return response;
  });
  if (cached) {
    update.catch(() => {});
    return cached;
  }
  return update;
}

// The path the app is served under ("" at a root, "/BlockWorldRenderer"
// on the GitHub Pages project site), derived from this worker's own URL.
const BASE_PATH = self.location.pathname.replace(/\/sw\.js$/, "");

async function respond(request) {
  const url = new URL(request.url);
  if (url.origin === self.location.origin) {
    if (url.pathname.startsWith(`${BASE_PATH}/wasm/`)) {
      return isolate(await staleWhileRevalidate(request, WASM_CACHE));
    }
    if (url.pathname.startsWith(`${BASE_PATH}/resources/`)) {
      // The app addresses game resources under the site's own origin; the
      // files themselves are Mojang's, keyed by hash. Mojang's resource
      // host sends no CORS headers, so the worker cannot fetch it
      // directly — the API relays the same path with CORS headers.
      const object = url.pathname.slice(`${BASE_PATH}/resources/`.length);
      const upstream = new Request(
        `https://api.karasu256.com/api/proxy/mojang/${object}`,
        { headers: request.headers },
      );
      return isolate(await cacheFirst(upstream, RESOURCE_CACHE));
    }
    return isolate(await fetch(request));
  }
  if (RESOURCE_HOSTS.has(url.hostname)) {
    return cacheFirst(request, RESOURCE_CACHE);
  }
  return fetch(request);
}

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  // `only-if-cached` is only valid for same-origin requests.
  if (request.cache === "only-if-cached" && request.mode !== "same-origin") return;
  // Range requests need partial responses, which the cache cannot give.
  if (request.headers.has("range")) return;
  event.respondWith(respond(request));
});
