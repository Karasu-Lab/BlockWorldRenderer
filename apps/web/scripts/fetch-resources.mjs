// Downloads the game resource objects the web viewer needs into a built
// site directory, under `resources/<hash prefix>/<hash>`, so the site
// serves them from its own origin.
//
// The viewer downloads these objects from Mojang at runtime by default,
// but resources.download.minecraft.net sends no CORS headers, so a
// browser can never read the responses — not directly, and not through
// the service worker either. Fetching them here, where CORS does not
// apply, is the only way a static site can offer them. The client jar
// and the version metadata are unaffected: piston-data.mojang.com and
// piston-meta.mojang.com do send CORS headers, and the viewer keeps
// downloading those at runtime.
//
// Usage: node fetch-resources.mjs <site directory>

import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const MANIFEST_URL =
  "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RESOURCES_BASE = "https://resources.download.minecraft.net";
const CONCURRENCY = 16;
const ATTEMPTS = 3;

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "..",
);

// The viewer keeps every asset index object except sounds and records
// (pomme-client's `is_drawn` filter); mirror that selection here so the
// site carries exactly what the viewer would have downloaded.
function isDrawn(key) {
  return (
    !key.startsWith("minecraft/sounds") && !key.startsWith("minecraft/records")
  );
}

function sha1(bytes) {
  return createHash("sha1").update(bytes).digest("hex");
}

async function fetchBytes(url) {
  let lastError;
  for (let attempt = 0; attempt < ATTEMPTS; attempt++) {
    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`${url}: HTTP ${response.status}`);
      }
      return Buffer.from(await response.arrayBuffer());
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError;
}

async function fetchJson(url) {
  return JSON.parse((await fetchBytes(url)).toString("utf8"));
}

// The game version the viewer installs, read from the Client submodule
// so this script cannot drift away from it.
async function viewerVersion() {
  const steelRs = path.join(
    repoRoot,
    "third_party",
    "Client",
    "pomme-client",
    "src",
    "viewer",
    "steel.rs",
  );
  const source = await readFile(steelRs, "utf8");
  const match = source.match(/pub const VERSION: &str = "([^"]+)"/);
  if (!match) {
    throw new Error(`No VERSION constant found in ${steelRs}`);
  }
  return match[1];
}

async function main() {
  const siteDir = process.argv[2];
  if (!siteDir) {
    throw new Error("Usage: node fetch-resources.mjs <site directory>");
  }
  const version = await viewerVersion();
  const manifest = await fetchJson(MANIFEST_URL);
  const entry = manifest.versions.find((candidate) => candidate.id === version);
  if (!entry) {
    throw new Error(`Version ${version} not found in the version manifest`);
  }
  const versionJson = await fetchJson(entry.url);
  const indexInfo = versionJson.assetIndex;
  const indexBytes = await fetchBytes(indexInfo.url);
  if (sha1(indexBytes) !== indexInfo.sha1) {
    throw new Error("The asset index did not match its hash");
  }
  const index = JSON.parse(indexBytes.toString("utf8"));
  const objects = Object.entries(index.objects)
    .filter(([key]) => isDrawn(key))
    .map(([, object]) => object);
  const totalBytes = objects.reduce((sum, object) => sum + object.size, 0);
  console.log(
    `Fetching ${objects.length} resource objects for ${version} ` +
      `(${(totalBytes / 1048576).toFixed(1)} MB)`,
  );

  let done = 0;
  let cursor = 0;
  async function worker() {
    while (cursor < objects.length) {
      const object = objects[cursor++];
      const bytes = await fetchBytes(
        `${RESOURCES_BASE}/${object.hash.slice(0, 2)}/${object.hash}`,
      );
      if (sha1(bytes) !== object.hash) {
        throw new Error(`Resource ${object.hash} did not match its hash`);
      }
      const file = path.join(
        siteDir,
        "resources",
        object.hash.slice(0, 2),
        object.hash,
      );
      await mkdir(path.dirname(file), { recursive: true });
      await writeFile(file, bytes);
      done += 1;
      if (done % 25 === 0 || done === objects.length) {
        console.log(`Fetched ${done}/${objects.length}`);
      }
    }
  }
  await Promise.all(
    Array.from({ length: Math.min(CONCURRENCY, objects.length) }, worker),
  );
  console.log("All resource objects are in place.");
}

await main();
