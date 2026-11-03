// Provides the wasm package in public/wasm, where the app and its service
// worker load it from. The package comes from the GitHub Release of
// Karasu-Lab/BlockWorldRenderer (asset block-world-renderer-wasm.tar.gz),
// unless the repository root holds a locally built pkg/ from
// `just web-wasm`, which takes precedence so CI and local renderer work use
// the build they just made. Set BLOCK_WORLD_RENDERER_WASM_TAG to pin a release tag; the
// default is the latest release. The worker helpers under snippets/ come
// along either way, since the thread pool starts them.
import { execFile } from "node:child_process";
import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

const REPO = "Karasu-Lab/BlockWorldRenderer";
const ASSET = "block-world-renderer-wasm.tar.gz";

const localPkg = fileURLToPath(new URL("../../../pkg/", import.meta.url));
const target = fileURLToPath(new URL("../public/wasm/", import.meta.url));
const stampPath = join(target, ".release");

async function hasWasm(dir) {
  const names = await readdir(dir).catch(() => []);
  return names.some((name) => name.endsWith("_bg.wasm"));
}

async function currentStamp() {
  return readFile(stampPath, "utf8").then((text) => text.trim()).catch(() => null);
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

async function installFromRelease(tag) {
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
    await rm(target, { recursive: true, force: true });
    await mkdir(target, { recursive: true });
    await cp(extracted, target, { recursive: true });
    await writeFile(stampPath, `${tag}\n`);
    console.log(`Installed wasm package ${tag} to ${target}`);
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

const pinned = process.env.BLOCK_WORLD_RENDERER_WASM_TAG;
if (pinned) {
  if ((await currentStamp()) === pinned && (await hasWasm(target))) {
    console.log(`wasm package ${pinned} already installed.`);
  } else {
    await installFromRelease(pinned);
  }
} else if (await hasWasm(localPkg)) {
  await installFromLocal();
} else {
  const latest = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`);
  if (!latest.ok) {
    throw new Error(
      `Could not resolve the latest release of ${REPO}: ${latest.status} ${latest.statusText}. ` +
        "Build the package locally with `just web-wasm` or set BLOCK_WORLD_RENDERER_WASM_TAG.",
    );
  }
  const { tag_name: tag } = await latest.json();
  if ((await currentStamp()) === tag && (await hasWasm(target))) {
    console.log(`wasm package ${tag} (latest) already installed.`);
  } else {
    await installFromRelease(tag);
  }
}
