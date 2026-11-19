import { test, expect } from '@playwright/test';
import { open, drawn, value, handleOf, centerOf, onFigure } from './helpers.js';

// A param with a binding has two drivers, its binding and the reader, and the reader's last action
// picks. In Bounds θ is bound to the case, whose options read q (corpus/004-bounds/bounds.pen):
//
//   param case in {"p = 0": 2·π / q, "q = 2p": 2·acos((sqrt(5 + 4·cos(2·π / q)) - 1) / 2)}
//   bind theta = case
//
// Dragging P holds θ, and setting q or the case hands it back to the binding, with no page script.
const closes = {
  'p = 0': (q) => (2 * Math.PI) / q,
  'q = 2p': (q) => 2 * Math.acos((Math.sqrt(5 + 4 * Math.cos((2 * Math.PI) / q)) - 1) / 2),
};

const held = (page) => onFigure(page, (f) => [...f.held.keys()]);

/** Drags P a little way, so θ leaves where its binding has it. */
async function dragP(page) {
  // The handle is placed where P is drawn, after the frame: wait for it to say θ as it is now.
  const handle = handleOf(page, 'P');
  const theta = await value(page, 'theta');
  await expect.poll(() => handle.evaluate((el) => Number(el.getAttribute('aria-valuenow')))).toBeCloseTo(theta, 9);
  const from = await centerOf(handle);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  for (let i = 1; i <= 8; i++) await page.mouse.move(from.x - i * 6, from.y + i * 3);
  await page.mouse.up();
}

test.describe('a param goes back to its binding', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'bounds');
    await drawn(page);
  });

  test('when the reader sets a param its binding reads', async ({ page }) => {
    // What the core says θ's binding reads.
    const reads = await onFigure(page, (f) => f.info.params.find((p) => p.id === 'theta').reads);
    expect([...reads].sort()).toEqual(['case.index', 'q']);
    expect(await value(page, 'theta')).toBeCloseTo(closes['p = 0'](6), 9);

    await dragP(page);
    await expect.poll(() => held(page)).toContain('theta');
    const dragged = await value(page, 'theta');
    expect(Math.abs(dragged - closes['p = 0'](6))).toBeGreaterThan(0.01);

    // q: θ closes the flower for 7 again.
    await onFigure(page, (f) => f.hold('q', 7));
    expect(await held(page)).toEqual(['q']);
    expect(await value(page, 'theta')).toBeCloseTo(closes['p = 0'](7), 9);

    // The case: the same, by the other option's formula.
    await dragP(page);
    await expect.poll(() => held(page)).toContain('theta');
    expect(Math.abs((await value(page, 'theta')) - closes['p = 0'](7))).toBeGreaterThan(0.01);
    await onFigure(page, (f) => f.hold('case.index', 1));
    expect((await held(page)).sort()).toEqual(['case.index', 'q']);
    expect(await value(page, 'theta')).toBeCloseTo(closes['q = 2p'](7), 9);

    // Setting θ itself keeps what it reads.
    await onFigure(page, (f) => f.hold('theta', 1.2));
    expect((await held(page)).sort()).toEqual(['case.index', 'q', 'theta']);
    expect(await value(page, 'theta')).toBeCloseTo(1.2, 9);
  });

  test('a key on P holds θ, and q hands it back', async ({ page }) => {
    await handleOf(page, 'P').focus();
    await page.keyboard.press('PageUp');
    await expect.poll(() => held(page)).toContain('theta');
    await onFigure(page, (f) => f.hold('q', 9));
    expect(await value(page, 'theta')).toBeCloseTo(closes['p = 0'](9), 9);
  });

  test('params held together keep each other, in either order', async ({ page }) => {
    await onFigure(page, (f) => f.setAttribute('params', 'q: 8; theta: 1'));
    expect((await held(page)).sort()).toEqual(['q', 'theta']);
    expect(await value(page, 'q')).toBe(8);
    expect(await value(page, 'theta')).toBeCloseTo(1, 9);

    await onFigure(page, (f) => f.setAttribute('params', 'theta: 1.3; q: 5'));
    expect((await held(page)).sort()).toEqual(['q', 'theta']);
    expect(await value(page, 'q')).toBe(5);
    expect(await value(page, 'theta')).toBeCloseTo(1.3, 9);

    // And through the script side, the same.
    await onFigure(page, (f) => (f.params = { theta: 0.9, q: 10 }));
    expect(await value(page, 'theta')).toBeCloseTo(0.9, 9);
    expect(await value(page, 'q')).toBe(10);
  });
});
