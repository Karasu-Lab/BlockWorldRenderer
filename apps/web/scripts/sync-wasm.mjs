// Copies the wasm-bindgen package built at the repository root into
// public/wasm, where the app and its service worker load it from. The worker
// helpers under snippets/ come along, since the thread pool starts them.
import { cp, mkdir, readdir, rm } from "node:fs/promises";
import { basename } from "node:path";
import { fileURLToPath } from "node:url";

const source = fileURLToPath(new URL("../../../pkg/", import.meta.url));
const target = fileURLToPath(new URL("../public/wasm/", import.meta.url));

const names = await readdir(source).catch(() => []);
if (!names.some((name) => name.endsWith("_bg.wasm"))) {
  console.error(`No wasm package in ${source}. Run \`just web-wasm\` first.`);
  process.exit(1);
}

const wanted = (path) => {
  const name = basename(path);
  return !(name === "package.json" || name === ".gitignore" || name.endsWith(".d.ts"));
};

await rm(target, { recursive: true, force: true });
await mkdir(target, { recursive: true });
await cp(source, target, { recursive: true, filter: wanted });
console.log(`Copied ${source} to ${target}`);
