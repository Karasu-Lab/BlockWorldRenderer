// Serves the wasm package from the cache the page saves it in, and adds the
// cross-origin isolation headers to what it serves, for hosts that cannot.

const PARAMS = new URL(self.location.href).searchParams;
const WASM_CACHE = PARAMS.get("wasmCache");
const CACHE_PREFIX = PARAMS.get("cachePrefix");
const RESOURCE_PROXY = PARAMS.get("resourceProxy");

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      await Promise.all(
        names
          .filter((name) => CACHE_PREFIX && name.startsWith(CACHE_PREFIX) && name !== WASM_CACHE)
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

async function cached(request, cacheName) {
  if (!cacheName) return fetch(request);
  const cache = await caches.open(cacheName);
  return (await cache.match(request)) ?? fetch(request);
}

// The path the app is served under ("" at a root, "/BlockWorldRenderer"
// on the GitHub Pages project site), derived from this worker's own URL.
const BASE_PATH = self.location.pathname.replace(/\/sw\.js$/, "");

async function respond(request) {
  const url = new URL(request.url);
  if (url.origin === self.location.origin) {
    if (url.pathname.startsWith(`${BASE_PATH}/wasm/`)) {
      return isolate(await cached(request, WASM_CACHE));
    }
    if (RESOURCE_PROXY && url.pathname.startsWith(`${BASE_PATH}/resources/`)) {
      // The app addresses game resources under the site's own origin; the
      // files themselves are Mojang's, keyed by hash. Mojang's resource
      // host sends no CORS headers, so the worker cannot fetch it
      // directly — the API relays the same path with CORS headers.
      const object = url.pathname.slice(`${BASE_PATH}/resources/`.length);
      const upstream = new Request(`${RESOURCE_PROXY}/${object}`, {
        headers: request.headers,
      });
      return isolate(await fetch(upstream));
    }
    return isolate(await fetch(request));
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
