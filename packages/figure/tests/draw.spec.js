import { test, expect } from '@playwright/test';
import { open, drawn, events, onFigure, figureOf, colorsIn } from './helpers.js';

test.describe('a figure loads and draws', () => {
  for (const [name, aspectFrom] of [['bounds', null], ['circle', null], ['lattice', null]]) {
    test(`${name}`, async ({ page }) => {
      const errors = [];
      page.on('pageerror', (e) => errors.push(e.message));
      await open(page, name);
      await drawn(page);
      const log = await events(page);
      expect(log.errors).toEqual([]);
      expect(errors).toEqual([]);
      await expect.poll(() => onFigure(page, (f) => f.backend)).toBeTruthy();

      // The canvas has the figure's shape.
      const [w, h] = await onFigure(page, (f) => f.info.size);
      const canvas = page.locator('#figure canvas');
      const box = await canvas.boundingBox();
      expect(box.width / box.height).toBeCloseTo(w / h, 1);
      expect(box.width).toBeGreaterThan(100);

      // Something is drawn on it: more than one color.
      await expect.poll(() => colorsIn(page, canvas), { message: 'the canvas should not be blank' }).toBeGreaterThan(1);
    });
  }

  // `plane.pen` has three dots and no sheet. Each gets its default paint, a dot in the ink.
  test('a figure with no sheet draws its marks', async ({ page }) => {
    await open(page, 'plane');
    await drawn(page);
    expect(await onFigure(page, (f) => f.getAttribute('sheets'))).toBeNull();
    const canvas = page.locator('#figure canvas');
    await expect.poll(() => colorsIn(page, canvas), { message: 'the canvas should not be blank' }).toBeGreaterThan(1);
  });

  test('a figure that cannot load says so with an error event', async ({ page }) => {
    await page.goto('/packages/figure/tests/pages/bounds.html');
    await drawn(page);
    const message = await figureOf(page).evaluate(
      (f) =>
        new Promise((resolve) => {
          f.addEventListener('error', (e) => resolve(String(e.detail.error.message)), { once: true });
          f.setAttribute('src', '/packages/figure/tests/pages/missing.pen');
        }),
    );
    expect(message).toContain('404');
  });
});
