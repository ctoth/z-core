import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';
import { chromium } from 'playwright';

const root = process.env.PAGES_ROOT ? resolve(process.env.PAGES_ROOT) : resolve(import.meta.dirname, '..');
const prefix = process.env.PAGES_ROOT ? '/z-core' : '';
const server = createServer(async (req, res) => {
  try {
    let pathname = new URL(req.url, 'http://localhost').pathname;
    if (!pathname.startsWith(`${prefix}/`)) { res.writeHead(404).end(); return; }
    pathname = pathname.slice(prefix.length);
    if (pathname.endsWith('/')) pathname += 'index.html';
    const path = resolve(root, `.${pathname}`);
    if (!path.startsWith(root + sep)) { res.writeHead(403).end(); return; }
    res.setHeader('Content-Type', ({ '.js': 'text/javascript', '.wasm': 'application/wasm', '.html': 'text/html' })[extname(path)] ?? 'application/octet-stream');
    res.end(await readFile(path));
  } catch { res.writeHead(404).end(); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
let browser;
try {
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const base = process.env.PAGES_URL ?? `http://127.0.0.1:${server.address().port}${prefix}/`;
  if (process.env.PAGES_ROOT || process.env.PAGES_URL) {
    await page.goto(base);
    await page.getByRole('link', { name: 'Open the firmware workbench' }).click();
  } else {
    await page.goto(`${base}demo/index.html`);
  }
  await page.waitForFunction(() => document.querySelector('#status').textContent.startsWith('Sample loaded'));
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#status').textContent.startsWith('Write at PC'));
  assert.match(await page.locator('#memory').textContent(), /001000: 01/);
  await page.getByRole('button', { name: 'Back', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#status').textContent.startsWith('Stepped back'));
  assert.match(await page.locator('#registers').textContent(), /PC\s+0006/);
  assert.match(await page.locator('#memory').textContent(), /001000: 00/);
  await page.getByRole('button', { name: 'Step', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#memory').textContent.startsWith('001000: 01'));
  const downloadPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export session' }).click();
  const download = await downloadPromise;
  const file = await download.path();
  const recording = JSON.parse(await readFile(file, 'utf8'));
  assert.equal(recording.attempt, 4);
  const forged = structuredClone(recording);
  forged.attempt = 5;
  await page.locator('#session').setInputFiles({ name: 'forged.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(forged)) });
  await page.waitForFunction(() => document.querySelector('#status').textContent.includes('disagrees'));
  assert.match(await page.locator('#history').textContent(), /^Attempt 4;/);
  await page.getByRole('button', { name: 'Reset', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#history').textContent.startsWith('Attempt 0;'));
  await page.locator('#session').setInputFiles({ name: 'recording.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(recording)) });
  await page.waitForFunction(() => document.querySelector('#history').textContent.startsWith('Attempt 4;'));
  assert.match(await page.locator('#memory').textContent(), /001000: 01/);
  await page.locator('#watch').fill('0xffffff');
  await page.locator('#cycles').fill('4294967295');
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === 'Paused.');
  assert.deepEqual(errors, []);
  console.log('Browser sample, watch, reverse step, export/import, and responsive pause: PASS');
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
