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
