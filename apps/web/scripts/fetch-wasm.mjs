// Provides the wasm package in public/wasm, where the app and its service
// worker load it from. The package comes from the GitHub Release of
// Karasu-Lab/BlockWorldRenderer, as individual file assets — the wasm
// file, its JavaScript glue, and the worker helpers under snippets/ that
// the glue imports — unless the repository root holds a locally built
// pkg/ from `just web-wasm`, which takes precedence so CI and local
// renderer work use the build they just made. Set WASM_RELEASE_TAG to pin
// a release tag; the default is the latest release. There is no archive
// to unpack: each file is downloaded on its own, and the snippet files
// are placed at the paths the downloaded glue itself references.
//
// Whether an installed package is current is decided by the SHA-256 of
// its wasm file: each release carries block_world_renderer.wasm.sha256
// next to the package files, and the package is downloaded only when the
// installed file's hash differs from the release's. Releases published
// before the checksum existed fall back to the .release stamp.
import { createHash } from "node:crypto";
import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "Karasu-Lab/BlockWorldRenderer";
const WASM_FILE = "block_world_renderer.wasm";
const GLUE_FILE = "block_world_renderer.js";
const CHECKSUM_ASSET = `${WASM_FILE}.sha256`;

const localPkg = fileURLToPath(new URL("../../../pkg/", import.meta.url));
const target = fileURLToPath(new URL("../public/wasm/", import.meta.url));
const stampPath = join(target, ".release");

async function hasWasm(dir) {
  const names = await readdir(dir).catch(() => []);
  return names.some((name) => name.endsWith(".wasm"));
}

async function currentStamp() {
  return readFile(stampPath, "utf8").then((text) => text.trim()).catch(() => null);
}

async function wasmHash(dir) {
  const bytes = await readFile(join(dir, WASM_FILE)).catch(() => null);
  return bytes ? createHash("sha256").update(bytes).digest("hex") : null;
}

function assetUrl(tag, name) {
  return `https://github.com/${REPO}/releases/download/${tag}/${name}`;
}

async function download(tag, name) {
  const url = assetUrl(tag, name);
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Download failed: ${response.status} ${response.statusText} (${url})`);
  }
  return Buffer.from(await response.arrayBuffer());
}

async function releaseWasmHash(tag) {
  const response = await fetch(assetUrl(tag, CHECKSUM_ASSET));
  if (!response.ok) return null;
  const hash = (await response.text()).trim().split(/\s+/)[0]?.toLowerCase();
  return /^[0-9a-f]{64}$/.test(hash ?? "") ? hash : null;
}

// The paths of the snippet files the glue loads, relative to the package
// root ("snippets/<crate>-<hash>/src/<file>").
function snippetPaths(glue) {
  const paths = new Set();
  for (const match of glue.matchAll(/["'](\.\/snippets\/[^"']+)["']/g)) {
    paths.add(match[1].slice(2));
  }
  return [...paths];
}

async function installFromLocal() {
  const wanted = (path) => {
    const name = basename(path);
    return !(name === "package.json" || name === ".gitignore" || name.endsWith(".d.ts"));
  };
  await rm(target, { recursive: true, force: true });
  await mkdir(target, { recursive: true });
  await cp(localPkg, target, { recursive: true, filter: wanted });
  await writeFile(stampPath, "local\n");
  console.log(`Copied local wasm package ${localPkg} to ${target}`);
}

async function installFromRelease(tag, expectedHash) {
  console.log(`Downloading wasm package ${tag}`);
  const staging = join(tmpdir(), `block-world-renderer-wasm-${process.pid}`);
  await rm(staging, { recursive: true, force: true });
  await mkdir(staging, { recursive: true });
  try {
    const glue = await download(tag, GLUE_FILE);
    await writeFile(join(staging, GLUE_FILE), glue);
    const files = [WASM_FILE, ...snippetPaths(glue.toString("utf8"))];
    for (const path of files) {
      const bytes = await download(tag, basename(path));
      await mkdir(dirname(join(staging, path)), { recursive: true });
      await writeFile(join(staging, path), bytes);
    }
    if (!(await hasWasm(staging))) {
      throw new Error(`Release ${tag} contains no wasm package.`);
    }
    if (expectedHash && (await wasmHash(staging)) !== expectedHash) {
      throw new Error(`The wasm file of ${tag} did not match ${CHECKSUM_ASSET}.`);
    }
    await rm(target, { recursive: true, force: true });
    await mkdir(target, { recursive: true });
    await cp(staging, target, { recursive: true });
    await writeFile(stampPath, `${tag}\n`);
    console.log(`Installed wasm package ${tag} to ${target}`);
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

async function ensureFromRelease(tag) {
  const expected = await releaseWasmHash(tag);
  if (expected) {
    if ((await wasmHash(target)) === expected) {
      console.log(`wasm package ${tag} is already installed (the wasm file hash matches).`);
      return;
    }
  } else if ((await currentStamp()) === tag && (await hasWasm(target))) {
    console.log(`wasm package ${tag} already installed.`);
    return;
  }
  await installFromRelease(tag, expected);
}

const pinned = process.env.WASM_RELEASE_TAG;
if (pinned) {
  await ensureFromRelease(pinned);
} else if (await hasWasm(localPkg)) {
  await installFromLocal();
} else {
  // The token only raises the API rate limit in CI; the release itself is
  // public, so the lookup works without one.
  const token = process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN;
  const latest = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
    headers: token ? { Authorization: `Bearer ${token}` } : undefined,
  });
  if (!latest.ok) {
    throw new Error(
      `Could not resolve the latest release of ${REPO}: ${latest.status} ${latest.statusText}. ` +
        "Build the package locally with `just web-wasm` or set WASM_RELEASE_TAG.",
    );
  }
  const { tag_name: tag } = await latest.json();
  await ensureFromRelease(tag);
}
