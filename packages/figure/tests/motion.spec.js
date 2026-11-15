import { test, expect } from '@playwright/test';
import { open, drawn, onFigure } from './helpers.js';

test.describe('prefers-reduced-motion: reduce', () => {
  test.use({ reducedMotion: 'reduce' });

  test('starts a figure that reads time paused', async ({ page }) => {
    await open(page, 'circle');
    await drawn(page);
    expect(await onFigure(page, (f) => f.info.usesTime)).toBe(true);
    expect(await onFigure(page, (f) => f.paused)).toBe(true);
    const t = await onFigure(page, (f) => f.time);
    await page.waitForTimeout(400); // negative check: nothing should move, so there is nothing to poll for
    expect(await onFigure(page, (f) => f.time)).toBe(t);
  });

  test('a reader can still press play', async ({ page }) => {
    await open(page, 'circle');
    await drawn(page);
    await onFigure(page, (f) => f.play());
    await expect.poll(() => onFigure(page, (f) => f.time)).toBeGreaterThan(0.1);
  });
});

test.describe('no motion preference', () => {
  test.use({ reducedMotion: 'no-preference' });

  test('a figure that reads time plays, and its time advances', async ({ page }) => {
    await open(page, 'circle');
    await drawn(page);
    expect(await onFigure(page, (f) => f.paused)).toBe(false);
    const first = await onFigure(page, (f) => f.time);
    await expect.poll(() => onFigure(page, (f) => f.time)).toBeGreaterThan(first + 0.1);
    await expect.poll(async () => (await page.evaluate(() => window.__pencil.draw))).toBeGreaterThan(3);
  });

  test('pausing stops the clock', async ({ page }) => {
    await open(page, 'circle');
    await drawn(page);
    await onFigure(page, (f) => f.pause());
    await expect.poll(() => onFigure(page, (f) => f.paused)).toBe(true);
    const t = await onFigure(page, (f) => f.time);
    await page.waitForTimeout(300); // negative check
    expect(await onFigure(page, (f) => f.time)).toBe(t);
  });
});
