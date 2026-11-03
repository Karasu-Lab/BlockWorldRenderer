// Provides the wasm package in public/wasm, where the app and its service
// worker load it from. The package comes from the GitHub Release of
// Karasu-Lab/BlockWorldRenderer (asset block-world-renderer-wasm.tar.gz),
// unless the repository root holds a locally built pkg/ from
// `just web-wasm`, which takes precedence so CI and local renderer work use
// the build they just made. Set WASM_RELEASE_TAG to pin a release tag; the
// default is the latest release. The worker helpers under snippets/ come
// along either way, since the thread pool starts them.
//
// Whether an installed package is current is decided by the SHA-256 of
// its wasm file: each release carries block_world_renderer.wasm.sha256
// next to the archive, and the archive is downloaded only when the
// installed file's hash differs from the release's. Releases published
// before the checksum existed fall back to the .release stamp.
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

const REPO = "Karasu-Lab/BlockWorldRenderer";
const ASSET = "block-world-renderer-wasm.tar.gz";
const WASM_FILE = "block_world_renderer.wasm";
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

async function releaseWasmHash(tag) {
  const url = `https://github.com/${REPO}/releases/download/${tag}/${CHECKSUM_ASSET}`;
  const response = await fetch(url);
  if (!response.ok) return null;
  const hash = (await response.text()).trim().split(/\s+/)[0]?.toLowerCase();
  return /^[0-9a-f]{64}$/.test(hash ?? "") ? hash : null;
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
  const url = `https://github.com/${REPO}/releases/download/${tag}/${ASSET}`;
  console.log(`Downloading ${url}`);
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Download failed: ${response.status} ${response.statusText} (${url})`);
  }
  const staging = join(tmpdir(), `block-world-renderer-wasm-${process.pid}`);
  await rm(staging, { recursive: true, force: true });
  await mkdir(staging, { recursive: true });
  try {
    const archive = join(staging, ASSET);
    await writeFile(archive, Buffer.from(await response.arrayBuffer()));
    const extracted = join(staging, "pkg");
    await mkdir(extracted, { recursive: true });
    await execFileAsync("tar", ["-xzf", archive, "-C", extracted]);
    if (!(await hasWasm(extracted))) {
      throw new Error(`Release asset ${ASSET} of ${tag} contains no wasm package.`);
    }
    if (expectedHash && (await wasmHash(extracted)) !== expectedHash) {
      throw new Error(`The wasm file of ${tag} did not match ${CHECKSUM_ASSET}.`);
    }
    await rm(target, { recursive: true, force: true });
    await mkdir(target, { recursive: true });
    await cp(extracted, target, { recursive: true });
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
