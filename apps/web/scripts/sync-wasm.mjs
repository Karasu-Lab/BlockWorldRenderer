// Copies the wasm-bindgen package built at the repository root into
// public/wasm, where the app and its service worker load it from.
import { cp, mkdir, readdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const source = fileURLToPath(new URL("../../../pkg/", import.meta.url));
const target = fileURLToPath(new URL("../public/wasm/", import.meta.url));

const files = (await readdir(source).catch(() => [])).filter((name) =>
  /\.(js|wasm)$/.test(name),
);
if (!files.some((name) => name.endsWith("_bg.wasm"))) {
  console.error(`No wasm package in ${source}. Run \`just web-wasm\` first.`);
  process.exit(1);
}

await rm(target, { recursive: true, force: true });
await mkdir(target, { recursive: true });
for (const name of files) {
  await cp(source + name, target + name);
}
console.log(`Copied ${files.length} files to ${target}`);
