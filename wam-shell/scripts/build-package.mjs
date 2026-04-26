import { copyFile, mkdir, rm, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageDir = resolve(scriptDir, "..");
const repoRoot = resolve(packageDir, "..");
const distDir = resolve(packageDir, "dist");
const wasmSource = resolve(repoRoot, "target/wasm32-unknown-unknown/release/spectral_freeze_wam.wasm");

async function copyTextFile(source, dest) {
  const text = await readFile(source, "utf8");
  await writeFile(dest, text);
}

await rm(distDir, { recursive: true, force: true });
await mkdir(distDir, { recursive: true });

await copyFile(wasmSource, resolve(distDir, "spectral_freeze_wam.wasm"));
await copyTextFile(resolve(packageDir, "js/SpectralFreezeWamNode.js"), resolve(distDir, "SpectralFreezeWamNode.js"));
await copyTextFile(resolve(packageDir, "js/SpectralFreezeWamProcessor.js"), resolve(distDir, "SpectralFreezeWamProcessor.js"));
await copyTextFile(resolve(packageDir, "js/index.js"), resolve(distDir, "index.js"));

console.log(`Built npm package files in ${distDir}`);
