import { test, expect } from '@playwright/test';
import { open, drawn, events, value, handleOf, centerOf, onFigure } from './helpers.js';

const now = (handle) => handle.evaluate((el) => Number(el.getAttribute('aria-valuenow')));

test.describe('pointer', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'bounds');
    await drawn(page);
  });

  test('a mouse drag on the handle changes theta, sends input, and leaves it focused', async ({ page }) => {
    const handle = handleOf(page, 'P');
    await expect(handle).toBeVisible();
    await onFigure(page, (f) => f.hold('theta', 1));
    await expect.poll(() => now(handle)).toBeCloseTo(1, 6);
    const from = await centerOf(handle);

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    for (let i = 1; i <= 8; i++) await page.mouse.move(from.x - i * 8, from.y + i * 2);
    await page.mouse.up();

    await expect.poll(() => value(page, 'theta')).not.toBeCloseTo(1, 2);
    const log = await events(page);
    expect(log.input.length).toBeGreaterThan(0);
    expect(log.input.at(-1).name).toBe('theta');
    expect(log.input.at(-1).value).toBeCloseTo(await value(page, 'theta'), 6);
    await expect(handle).toBeFocused();
    await expect.poll(() => now(handle)).toBeCloseTo(await value(page, 'theta'), 6);

    // The keys carry on from where the pointer left it.
    const before = await value(page, 'theta');
    // A drag can end at either end of the range, so go the way that has room.
    await page.keyboard.press(before > 1 ? 'PageDown' : 'PageUp');
    await expect.poll(() => value(page, 'theta')).not.toBeCloseTo(before, 3);
  });

  test('a press away from any handle takes nothing', async ({ page }) => {
    const canvas = page.locator('#figure canvas');
    const box = await canvas.boundingBox();
    await page.mouse.move(box.x + 4, box.y + 4);
    await page.mouse.down();
    await page.mouse.move(box.x + 30, box.y + 30);
    await page.mouse.up();
    expect((await events(page)).input).toEqual([]);
  });

  test('a touch drag through CDP moves the handle', async ({ page, browserName }, testInfo) => {
    test.skip(testInfo.project.name !== 'pixel-7', 'CDP touch input is Chromium only; run on the Chromium phone');
    const cdp = await page.context().newCDPSession(page);
    const handle = handleOf(page, 'P');
    await expect(handle).toBeVisible();
    await onFigure(page, (f) => f.hold('theta', 1));
    await expect.poll(() => now(handle)).toBeCloseTo(1, 6);
    const from = await centerOf(handle);

    const touch = (type, x, y) =>
      cdp.send('Input.dispatchTouchEvent', { type, touchPoints: type === 'touchEnd' ? [] : [{ x, y, id: 1 }] });
    await touch('touchStart', from.x, from.y);
    for (let i = 1; i <= 8; i++) await touch('touchMove', from.x - i * 8, from.y + i * 2);
    await touch('touchEnd');

    await expect.poll(() => value(page, 'theta')).not.toBeCloseTo(1, 2);
    expect((await events(page)).input.length).toBeGreaterThan(0);
    await expect(handle).toBeFocused();
    expect(await page.evaluate(() => window.scrollY)).toBe(0);
  });
});

// A page as a site has it, with no CSS for the figure: text beside it, and tall enough to scroll.
test.describe('pointer on a page that scrolls', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'scroll');
    await drawn(page);
    await onFigure(page, (f) => f.hold('theta', 1));
    await expect.poll(() => now(handleOf(page, 'P'))).toBeCloseTo(1, 6);
  });

  /** Touches through CDP, which go through Chromium's own input pipeline, so `touch-action` and
   * scrolling are the browser's, as on a phone. */
  const touches = async (page) => {
    const cdp = await page.context().newCDPSession(page);
    return async (path) => {
      const [first, ...rest] = path;
      const send = (type, p) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: p ? [{ x: p.x, y: p.y, id: 1 }] : [] });
      await send('touchStart', first);
      for (const p of rest) {
        await send('touchMove', p);
        await page.waitForTimeout(16);
      }
      await send('touchEnd');
    };
  };

  /** A finger moving up the screen from `from`, as a swipe that scrolls the page down. */
  const upFrom = (from, by = 240, steps = 12) =>
    Array.from({ length: steps + 1 }, (_, i) => ({ x: from.x, y: from.y - (by * i) / steps }));

  test('a touch drag on a handle moves it and doesn\'t scroll the page', async ({ page }, testInfo) => {
    test.skip(testInfo.project.name !== 'pixel-7', 'CDP touch input is Chromium only; run on the Chromium phone');
    const handle = handleOf(page, 'P');
    const swipe = await touches(page);
    expect(await page.evaluate(() => document.scrollingElement.scrollHeight > innerHeight)).toBe(true);
    await swipe(upFrom(await centerOf(handle), 120));

    await expect.poll(() => value(page, 'theta')).not.toBeCloseTo(1, 2);
    expect((await events(page)).input.length).toBeGreaterThan(0);
    await expect(handle).toBeFocused();
    expect(await page.evaluate(() => window.scrollY)).toBe(0);
  });

  test('a touch swipe that starts off the handles scrolls the page and moves nothing', async ({ page }, testInfo) => {
    test.skip(testInfo.project.name !== 'pixel-7', 'CDP touch input is Chromium only; run on the Chromium phone');
    const handle = await centerOf(handleOf(page, 'P'));
    const box = await page.locator('#figure canvas').boundingBox();
    // The point on the figure farthest from the handle, well outside its reach.
    const from = [
      [0.1, 0.9], [0.9, 0.9], [0.1, 0.5], [0.9, 0.5], [0.5, 0.9],
    ]
      .map(([u, v]) => ({ x: box.x + u * box.width, y: box.y + v * box.height }))
      .reduce((a, b) => (Math.hypot(b.x - handle.x, b.y - handle.y) > Math.hypot(a.x - handle.x, a.y - handle.y) ? b : a));
    expect(Math.hypot(from.x - handle.x, from.y - handle.y)).toBeGreaterThan(72);
    const swipe = await touches(page);
    await swipe(upFrom(from));

    await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(50);
    expect((await events(page)).input).toEqual([]);
    expect(await value(page, 'theta')).toBeCloseTo(1, 6);
  });

  test('a mouse drag from a handle out over the page\'s text selects nothing', async ({ page }) => {
    const handle = handleOf(page, 'P');
    const from = await centerOf(handle);
    const to = await centerOf(page.locator('#text'));
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    for (let i = 1; i <= 12; i++) await page.mouse.move(from.x + ((to.x - from.x) * i) / 12, from.y + ((to.y - from.y) * i) / 12);
    await page.mouse.up();

    await expect.poll(() => value(page, 'theta')).not.toBeCloseTo(1, 2);
    expect(await page.evaluate(() => getSelection().toString())).toBe('');
    await expect(handle).toBeFocused();
  });

  test('nothing in the figure can be selected', async ({ page }) => {
    // What a phone's long press reads, which no headless browser does: its labels, canvas and
    // handles are none of them text to select.
    const found = await onFigure(page, (f) => {
      const root = f.shadowRoot;
      const parts = [f, root.querySelector('canvas'), root.querySelector('[data-label]'), root.querySelector('.handle')];
      return parts.map((el) => {
        const style = getComputedStyle(el);
        return el && (style.userSelect || style.webkitUserSelect);
      });
    });
    expect(found).toEqual(['none', 'none', 'none', 'none']);
  });
});
