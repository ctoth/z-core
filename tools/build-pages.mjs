import { copyFile, mkdir, rm } from 'node:fs/promises';
import { resolve, relative, sep } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const target = resolve(root, 'target');
const output = resolve(target, 'pages');
if (relative(target, output) !== 'pages' || !output.startsWith(target + sep)) {
  throw new Error('Pages output must remain inside target/pages');
}
await rm(output, { recursive: true, force: true });
await mkdir(resolve(output, 'demo'), { recursive: true });
await mkdir(resolve(output, 'pkg-web'), { recursive: true });

// Publish only the site and runtime assets, never the repository or local ROMs.
for (const [source, destination] of [
  ['docs/site/index.html', 'index.html'],
  ['LICENSE-MIT', 'LICENSE-MIT'],
  ['LICENSE-APACHE', 'LICENSE-APACHE'],
  ['crates/z180-wasm/demo/index.html', 'demo/index.html'],
  ['crates/z180-wasm/demo/demo.js', 'demo/demo.js'],
  ['crates/z180-wasm/demo/worker.js', 'demo/worker.js'],
  ['crates/z180-wasm/pkg-web/z180_wasm.js', 'pkg-web/z180_wasm.js'],
  ['crates/z180-wasm/pkg-web/z180_wasm_bg.wasm', 'pkg-web/z180_wasm_bg.wasm'],
]) {
  await copyFile(resolve(root, source), resolve(output, destination));
}
console.log(`Built GitHub Pages site: ${output}`);
