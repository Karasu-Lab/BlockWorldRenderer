import type { NextConfig } from "next";

const isolationHeaders = [
  { key: "Cross-Origin-Opener-Policy", value: "same-origin" },
  { key: "Cross-Origin-Embedder-Policy", value: "require-corp" },
];

const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
const staticExport = process.env.STATIC_EXPORT === "1";

const nextConfig: NextConfig = {
  devIndicators: false,
  reactStrictMode: false,
  basePath: basePath || undefined,
  output: staticExport ? "export" : undefined,
  cacheComponents: !staticExport,
  partialPrefetching: !staticExport,
  async rewrites() {
    if (staticExport) return [];
    return [
      {
        source: "/resources/:path*",
        destination: "https://resources.download.minecraft.net/:path*",
      },
    ];
  },
  async headers() {
    return [
      { source: "/:path*", headers: isolationHeaders },
      {
        source: "/sw.js",
        headers: [
          {
            key: "Content-Type",
            value: "application/javascript; charset=utf-8",
          },
          {
            key: "Cache-Control",
            value: "no-cache, no-store, must-revalidate",
          },
          { key: "Service-Worker-Allowed", value: "/" },
        ],
      },
      {
        source: "/wasm/:path*",
        headers: [
          { key: "Cache-Control", value: "public, max-age=0, must-revalidate" },
        ],
      },
    ];
  },
};

export default nextConfig;
