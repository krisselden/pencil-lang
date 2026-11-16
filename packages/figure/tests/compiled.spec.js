import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { open, drawn, events, onFigure, colorsIn } from './helpers.js';

const built = resolve(dirname(fileURLToPath(import.meta.url)), '.built/tube/tube.pfig');

test('a compiled figure loads and draws, with no .pen or .style fetched', async ({ page }) => {
  test.skip(!existsSync(built), 'target/release/pencil is missing (or its build failed), so tests/.built/tube/tube.pfig was not made');
  const sources = [];
  const pfig = [];
  page.on('request', (r) => {
    const path = new URL(r.url()).pathname;
    if (/\.(pen|style)$/.test(path)) sources.push(path);
    if (path.endsWith('.pfig')) pfig.push(path);
  });
  await open(page, 'compiled');
  await drawn(page);
  expect((await events(page)).errors).toEqual([]);
  expect(pfig.length).toBeGreaterThan(0);
  expect(sources).toEqual([]);
  expect(await onFigure(page, (f) => f.backend)).toBeTruthy();
  const [w, h] = await onFigure(page, (f) => f.info.size);
  const box = await page.locator('#figure canvas').boundingBox();
  expect(box.width / box.height).toBeCloseTo(w / h, 1);
  await expect.poll(() => colorsIn(page, page.locator('#figure canvas'))).toBeGreaterThan(1);
});

test.describe('what a page fetched ahead', () => {
  test.beforeEach(() => {
    test.skip(!existsSync(built), 'target/release/pencil is missing (or its build failed), so tests/.built/tube/tube.pfig was not made');
  });

  // A page that knows what a figure needs starts on it before the element's script runs. The
  // element takes those responses, so each file is downloaded once. `<link rel="preload"
  // as="fetch">` didn't do this in WebKit, which fetched each again.
  test('is used, so the WASM and the figure are each downloaded once', async ({ page }) => {
    const fetched = [];
    page.on('response', (r) => {
      const path = new URL(r.url()).pathname;
      if (/\.(wasm|pfig)$/.test(path)) fetched.push(`${r.status()} ${path.split('/').pop()}`);
    });
    await open(page, 'ahead');
    await drawn(page);
    expect((await events(page)).errors).toEqual([]);
    expect(fetched.sort()).toEqual(['200 pencil_web_bg.wasm', '200 tube.pfig']);
    expect(await page.evaluate(() => Object.keys(self.__pencilAhead))).toEqual([]);
    await expect.poll(() => colorsIn(page, page.locator('#figure canvas'))).toBeGreaterThan(1);
  });

  test('that failed is fetched again, and the figure still draws', async ({ page }) => {
    const fetched = [];
    page.on('response', (r) => {
      const path = new URL(r.url()).pathname;
      if (/\.(wasm|pfig)(\.missing)?$/.test(path)) fetched.push(`${r.status()} ${path.split('/').pop()}`);
    });
    await open(page, 'ahead', 'broken');
    await drawn(page);
    expect((await events(page)).errors).toEqual([]);
    expect(fetched.sort()).toEqual([
      '200 pencil_web_bg.wasm', '200 tube.pfig', '404 pencil_web_bg.wasm.missing', '404 tube.pfig.missing',
    ]);
  });
});
