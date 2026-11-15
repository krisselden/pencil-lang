import { test, expect } from '@playwright/test';
import { open, drawn, handleOf } from './helpers.js';

test.describe('on a phone', () => {
  test.beforeEach(({ isMobile }) => {
    test.skip(!isMobile, 'phone projects only');
  });

  test('the figure is no wider than the viewport', async ({ page }) => {
    const figure = await open(page, 'bounds');
    await drawn(page);
    const viewport = page.viewportSize();
    const box = await figure.boundingBox();
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(viewport.width + 0.5);
    const scroll = await page.evaluate(() => ({ scroll: document.documentElement.scrollWidth, inner: window.innerWidth }));
    expect(scroll.scroll).toBeLessThanOrEqual(scroll.inner);
  });

  test('the handle\'s focus target is at least 24 by 24 CSS px', async ({ page }) => {
    await open(page, 'bounds');
    await drawn(page);
    const box = await handleOf(page, 'P').boundingBox();
    expect(box.width).toBeGreaterThanOrEqual(24);
    expect(box.height).toBeGreaterThanOrEqual(24);
  });

  test('the handle sits over its mark and keeps up when the figure is resized by rotation', async ({ page }) => {
    await open(page, 'bounds');
    await drawn(page);
    const handle = handleOf(page, 'P');
    const canvas = await page.locator('#figure canvas').boundingBox();
    const box = await handle.boundingBox();
    expect(box.x + box.width / 2).toBeGreaterThan(canvas.x);
    expect(box.x + box.width / 2).toBeLessThan(canvas.x + canvas.width);
    expect(box.y + box.height / 2).toBeGreaterThan(canvas.y);
    expect(box.y + box.height / 2).toBeLessThan(canvas.y + canvas.height);
  });
});
