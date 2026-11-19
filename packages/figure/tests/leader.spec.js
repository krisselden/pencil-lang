import { test, expect } from '@playwright/test';
import { open, drawn, onFigure } from './helpers.js';

/** The leaders in the figure's shadow DOM: whether their layer is there, and each line's ends. */
const leaders = (page) =>
  onFigure(page, (f) => ({
    layers: f.shadowRoot.querySelectorAll('svg').length,
    lines: [...f.shadowRoot.querySelectorAll('svg line')].map((line) =>
      ['x1', 'y1', 'x2', 'y2'].map((name) => Number(line.getAttribute(name) ?? NaN)),
    ),
    labels: f.shadowRoot.querySelectorAll('.label[data-label]').length,
  }));

test.describe('label leaders', () => {
  test('only a label that draws a leader has a line, with both its ends', async ({ page }) => {
    await open(page, 'leader');
    await drawn(page);
    await expect.poll(async () => (await leaders(page)).lines.length).toBe(1);
    const { layers, lines, labels } = await leaders(page);
    expect(labels).toBe(2);
    expect(layers).toBe(1);
    const [x1, y1, x2, y2] = lines[0];
    expect(lines[0].every(Number.isFinite)).toBe(true);
    // From the point up and to the right, toward where the label sits.
    expect(x2).toBeGreaterThan(x1);
    expect(y2).toBeLessThan(y1);
  });

  test('the line and its layer go when the leader has nothing to draw, and come back', async ({ page }) => {
    await open(page, 'leader');
    await drawn(page);
    await expect.poll(async () => (await leaders(page)).lines.length).toBe(1);
    await onFigure(page, (f) => f.hold('near', 1));
    await expect.poll(() => leaders(page)).toMatchObject({ layers: 0, lines: [], labels: 2 });
    await onFigure(page, (f) => f.hold('near', 0));
    await expect.poll(async () => (await leaders(page)).lines.length).toBe(1);
    expect((await leaders(page)).layers).toBe(1);
  });
});
