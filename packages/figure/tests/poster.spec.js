import { test, expect } from '@playwright/test';
import { open, drawn, figureOf, events, onFigure } from './helpers.js';

test.describe('a poster', () => {
  test('shows until the first draw, in the same box', async ({ page }) => {
    // Hold the .pen back, so the poster is on screen for as long as the test wants to look.
    let release;
    const held = new Promise((resolve) => { release = resolve; });
    await page.route('**/*.pen', async (route) => {
      await held;
      await route.continue();
    });
    const figure = await open(page, 'poster');
    const poster = page.locator('#poster');
    await expect(poster).toBeVisible();
    await expect(poster).toHaveJSProperty('complete', true);
    const before = await figure.boundingBox();
    expect(before.height).toBeGreaterThan(50);
    expect((await events(page)).draw).toBe(0);

    release();
    await drawn(page);
    await expect(poster).toBeHidden();
    const after = await figure.boundingBox();
    expect(after.width).toBeCloseTo(before.width, 0);
    expect(Math.abs(after.height - before.height)).toBeLessThanOrEqual(1);
    const canvas = await page.locator('#figure canvas').boundingBox();
    expect(Math.abs(canvas.height - before.height)).toBeLessThanOrEqual(1);
  });

  test('a figure far below the fold fetches nothing until it is near', async ({ page }) => {
    const requested = [];
    page.on('request', (r) => { if (/\.(pen|style)$/.test(new URL(r.url()).pathname)) requested.push(new URL(r.url()).pathname); });
    await open(page, 'far');
    await expect(page.locator('#poster')).toBeVisible();
    // Quiet for a while is the thing being tested; give the element a few frames to be wrong.
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.waitForLoadState('networkidle');
    expect(requested).toEqual([]);
    expect((await events(page)).load).toBe(0);

    await figureOf(page).scrollIntoViewIfNeeded();
    await drawn(page);
    expect(requested.some((p) => p.endsWith('circle.pen'))).toBe(true);
    await expect(page.locator('#poster')).toBeHidden();
  });
});
