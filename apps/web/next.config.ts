import type { NextConfig } from "next";

// Cross-origin isolation is required for SharedArrayBuffer, which the wasm
// worker threads share memory through.
const isolationHeaders = [
  { key: "Cross-Origin-Opener-Policy", value: "same-origin" },
  { key: "Cross-Origin-Embedder-Policy", value: "require-corp" },
];

// Set for the GitHub Pages demo, which serves the app from a project
// subpath as plain static files. Other builds leave both unset.
const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
const staticExport = process.env.STATIC_EXPORT === "1";

const nextConfig: NextConfig = {
  basePath: basePath || undefined,
  output: staticExport ? "export" : undefined,
  // Partial prerendering is not available to a static export.
  cacheComponents: !staticExport,
  partialPrefetching: !staticExport,
  async headers() {
    return [
      { source: "/:path*", headers: isolationHeaders },
      {
        source: "/sw.js",
        headers: [
          { key: "Content-Type", value: "application/javascript; charset=utf-8" },
          { key: "Cache-Control", value: "no-cache, no-store, must-revalidate" },
          { key: "Service-Worker-Allowed", value: "/" },
        ],
      },
      {
        source: "/wasm/:path*",
        headers: [{ key: "Cache-Control", value: "public, max-age=0, must-revalidate" }],
      },
    ];
  },
};

export default nextConfig;
