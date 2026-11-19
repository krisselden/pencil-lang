// A handle that sets two params: z = a + b·i, with a in −1..1 and b in −0.5..0.5 (pages/plane.pen).
import { test, expect } from '@playwright/test';
import { open, drawn, events, value, onFigure, handleOf, tabToHandle, centerOf } from './helpers.js';

const hold = (page, pairs) => onFigure(page, (f, p) => (f.params = p), pairs);
const held = (page) => onFigure(page, (f) => Object.fromEntries(f.held));
const values = async (page) => ({ a: await value(page, 'a'), b: await value(page, 'b') });

/** Where a point of the figure is on the page. Its view is −2..2 across its shorter side, about the
 * middle, since a view keeps one scale for both axes. */
async function at(page, x, y) {
  const box = await page.locator('#figure canvas').boundingBox();
  const unit = Math.min(box.width, box.height) / 4;
  return { x: box.x + box.width / 2 + x * unit, y: box.y + box.height / 2 - y * unit };
}

/** A path through points of the figure, a few steps between each. */
async function path(page, points, steps = 6) {
  const out = [];
  let last;
  for (const [x, y] of points) {
    if (last) for (let i = 1; i <= steps; i++) out.push([last[0] + ((x - last[0]) * i) / steps, last[1] + ((y - last[1]) * i) / steps]);
    else out.push([x, y]);
    last = [x, y];
  }
  return Promise.all(out.map(([x, y]) => at(page, x, y)));
}

test.describe('a handle that sets two params', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'plane');
    await drawn(page);
    await expect(handleOf(page, 'z')).toBeVisible();
  });

  test('is a 2D slider that says both values', async ({ page }) => {
    const handle = await tabToHandle(page, 'z');
    await expect(handle).toHaveAttribute('role', 'slider');
    await expect(handle).toHaveAttribute('aria-roledescription', '2D slider');
    await expect(handle).toHaveAttribute('aria-valuetext', 'a = 0, b = 0');
    await hold(page, { a: 0.25, b: -0.125 });
    await expect(handle).toHaveAttribute('aria-valuetext', 'a = 0.25, b = -0.125');
    // Its number is the first param's, in that param's range.
    await expect(handle).toHaveAttribute('aria-valuenow', '0.25');
    await expect(handle).toHaveAttribute('aria-valuemin', '-1');
    await expect(handle).toHaveAttribute('aria-valuemax', '1');
  });

  test('a mouse drag sets both, and input says both', async ({ page }) => {
    const [from, ...rest] = await path(page, [[0, 0], [0.4, 0.2]]);
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    for (const p of rest) await page.mouse.move(p.x, p.y);
    await page.mouse.up();

    const now = await values(page);
    expect(now.a).toBeCloseTo(0.4, 1);
    expect(now.b).toBeCloseTo(0.2, 1);
    const last = (await events(page)).input.at(-1);
    expect(last.name).toBe('a');
    expect(last.value).toBeCloseTo(now.a, 9);
    expect(Object.keys(last.values)).toEqual(['a', 'b']);
    expect(last.values.a).toBeCloseTo(now.a, 9);
    expect(last.values.b).toBeCloseTo(now.b, 9);
    // Both are held, and the handle has the focus for the keys to carry on.
    expect(Object.keys(await held(page)).sort()).toEqual(['a', 'b']);
    await expect(handleOf(page, 'z')).toBeFocused();
  });

  test('each param stops at its own range while the other goes on', async ({ page }) => {
    const points = await path(page, [[0, 0], [1.6, 0.3], [1.6, -0.2], [0.3, 1.4], [-1.7, 1.4], [-1.7, -1.8]]);
    // Where the drag was at each corner of the path, after its steps.
    const corners = [6, 12, 18, 24, 30];
    const seen = [];
    await page.mouse.move(points[0].x, points[0].y);
    await page.mouse.down();
    for (let i = 1; i < points.length; i++) {
      await page.mouse.move(points[i].x, points[i].y);
      if (corners.includes(i)) seen.push(await values(page));
    }
    await page.mouse.up();

    // Past a's end on the right, a stays at 1 and b follows the pointer up and down.
    expect(seen[0].a).toBe(1);
    expect(seen[0].b).toBeCloseTo(0.3, 1);
    expect(seen[1].a).toBe(1);
    expect(seen[1].b).toBeCloseTo(-0.2, 1);
    // Past b's top, b stays at 0.5 and a follows the pointer.
    expect(seen[2].b).toBe(0.5);
    expect(seen[2].a).toBeCloseTo(0.3, 1);
    // Past both, it's the corner.
    expect(seen[3]).toEqual({ a: -1, b: 0.5 });
    expect(seen[4]).toEqual({ a: -1, b: -0.5 });
  });

  test('a touch drag sets both', async ({ page }, testInfo) => {
    test.skip(testInfo.project.name !== 'pixel-7', 'CDP touch input is Chromium only; run on the Chromium phone');
    const cdp = await page.context().newCDPSession(page);
    const touch = (type, p) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: p ? [{ x: p.x, y: p.y, id: 1 }] : [] });
    const [from, ...rest] = await path(page, [[0, 0], [-0.5, -0.3]]);
    await touch('touchStart', from);
    for (const p of rest) await touch('touchMove', p);
    await touch('touchEnd');

    await expect.poll(() => value(page, 'a')).toBeCloseTo(-0.5, 1);
    expect(await value(page, 'b')).toBeCloseTo(-0.3, 1);
    const last = (await events(page)).input.at(-1);
    expect(Object.keys(last.values)).toEqual(['a', 'b']);
    expect(await page.evaluate(() => window.scrollY)).toBe(0);
  });

  test('the four arrows move it on the page', async ({ page }) => {
    const handle = await tabToHandle(page, 'z');
    const moved = async (key, a, b) => {
      const before = await values(page);
      await page.keyboard.press(key);
      await expect.poll(async () => {
        const now = await values(page);
        return [Math.sign(Number((now.a - before.a).toFixed(9))), Math.sign(Number((now.b - before.b).toFixed(9)))];
      }).toEqual([a, b]);
    };
    await moved('ArrowRight', 1, 0);
    await moved('ArrowUp', 0, 1);
    await moved('ArrowLeft', -1, 0);
    await moved('ArrowDown', 0, -1);
    await expect(handle).toBeFocused();
    // Each key says both, to the page and to a screen reader.
    const last = (await events(page)).input.at(-1);
    expect(Object.keys(last.values)).toEqual(['a', 'b']);
    const now = await values(page);
    await expect(handle).toHaveAttribute('aria-valuetext', `a = ${Number(now.a.toFixed(3))}, b = ${Number(now.b.toFixed(3))}`);
  });

  test('Shift, Home, End and the Page keys are the long moves', async ({ page }) => {
    await tabToHandle(page, 'z');
    const from = async (key) => {
      await hold(page, { a: 0, b: 0 });
      await expect.poll(() => value(page, 'a')).toBe(0);
      await page.keyboard.press(key);
      await expect.poll(() => values(page)).not.toEqual({ a: 0, b: 0 });
      return values(page);
    };
    const short = (await from('ArrowRight')).a;
    const long = (await from('Shift+ArrowRight')).a;
    expect(short).toBeGreaterThan(0);
    expect(long).toBeCloseTo(10 * short, 6);
    expect(await from('End')).toEqual({ a: long, b: 0 });
    expect((await from('Home')).a).toBeCloseTo(-long, 9);
    // Up the page is up in b. b's range is half a's, so a long move up stops at its end.
    const up = await from('PageUp');
    expect(up.a).toBe(0);
    expect(up.b).toBeCloseTo(Math.min(long, 0.5), 9);
    expect((await from('PageDown')).b).toBeCloseTo(-Math.min(long, 0.5), 9);
  });

  test('a drag hands back a held param whose binding reads what it sets, and only that one', async ({ page }) => {
    await onFigure(page, (f) => {
      f.hold('c', 1.5);
      f.hold('d', 3);
    });
    await expect.poll(() => value(page, 'c')).toBe(1.5);
    const [from, ...rest] = await path(page, [[0, 0], [0.5, 0.25]]);
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    for (const p of rest) await page.mouse.move(p.x, p.y);
    await page.mouse.up();

    // c's binding reads a, so it's a's again. d's reads time, so it stays held.
    const now = await held(page);
    expect(Object.keys(now).sort()).toEqual(['a', 'b', 'd']);
    expect(now.d).toBe(3);
    await expect.poll(() => value(page, 'c')).toBeCloseTo(await value(page, 'a'), 9);
    expect(await value(page, 'c')).toBeCloseTo(0.5, 1);
  });
});
