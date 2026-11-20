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

  // `bare.pen`: `dot z` and `label z at z`, and no sheet. The label settles off its dot, and it's
  // there from its first frame: it doesn't glide in from anywhere.
  test('a label with no sheet is off its dot from its first frame', async ({ page }) => {
    await open(page, 'bare');
    // Every frame from the label's first, in CSS pixels, until it has stayed put a while.
    const seen = await onFigure(
      page,
      (f) =>
        new Promise((resolve) => {
          const boxes = [];
          const frame = () => {
            // A label with mathematics is a point, and its runs are placed around it.
            const runs = [...(f.shadowRoot?.querySelectorAll('.label[data-label="z"] .run') ?? [])];
            const rects = runs.map((run) => run.getBoundingClientRect()).filter((r) => r.width > 0);
            if (rects.length) {
              boxes.push({
                left: Math.min(...rects.map((r) => r.left)),
                right: Math.max(...rects.map((r) => r.right)),
                top: Math.min(...rects.map((r) => r.top)),
                bottom: Math.max(...rects.map((r) => r.bottom)),
              });
            }
            if (boxes.length >= 40) resolve(boxes);
            else requestAnimationFrame(frame);
          };
          frame();
        }),
    );
    await drawn(page);
    const last = seen.at(-1);
    for (const box of seen) {
      expect(Math.abs(box.left - last.left)).toBeLessThan(1);
      expect(Math.abs(box.top - last.top)).toBeLessThan(1);
    }
    // The dot is at (0.5, 0.5) in a view from -2 to 2, 3 px across: the label's box clears it.
    const canvas = await page.locator('#figure canvas').boundingBox();
    const dot = { x: canvas.x + canvas.width * 0.625, y: canvas.y + canvas.height * 0.375 };
    const dx = Math.max(last.left - dot.x, dot.x - last.right, 0);
    const dy = Math.max(last.top - dot.y, dot.y - last.bottom, 0);
    expect(Math.hypot(dx, dy)).toBeGreaterThan(3);
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
