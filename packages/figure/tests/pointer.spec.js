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
