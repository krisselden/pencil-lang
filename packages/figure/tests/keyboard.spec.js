import { test, expect } from '@playwright/test';
import { open, drawn, onFigure, value, tabToHandle, handleOf, figureOf } from './helpers.js';

const now = (handle) => handle.evaluate((el) => Number(el.getAttribute('aria-valuenow')));
const hold = (page, name, v) => onFigure(page, (f, [n, x]) => f.hold(n, x), [name, v]);

test.describe('handles by keyboard on 004-bounds', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'bounds');
    await drawn(page);
  });

  test('Tab reaches the handle, which is a named slider', async ({ page }) => {
    const handle = await tabToHandle(page, 'P');
    await expect(handle).toBeFocused();
    await expect(handle).toHaveAttribute('role', 'slider');
    await expect(handle).toHaveAttribute('aria-label', 'P');
    await expect(handle).toHaveAttribute('aria-valuemin', '0.2');
    await expect(handle).toHaveAttribute('aria-valuemax', '1.85');
    await expect(handle).toHaveAttribute('aria-valuetext', /^theta = \d/);
  });

  test('aria-valuetext says the param and three decimals', async ({ page }) => {
    const handle = handleOf(page, 'P');
    await hold(page, 'theta', Math.PI / 3);
    await expect(handle).toHaveAttribute('aria-valuetext', 'theta = 1.047');
  });

  test('an arrow changes theta, and aria-valuenow follows', async ({ page }) => {
    const handle = await tabToHandle(page, 'P');
    await hold(page, 'theta', 1);
    await expect.poll(() => now(handle)).toBeCloseTo(1, 6);
    await page.keyboard.press('ArrowLeft');
    await expect.poll(() => value(page, 'theta')).not.toBeCloseTo(1, 4);
    const theta = await value(page, 'theta');
    await expect.poll(() => now(handle)).toBeCloseTo(theta, 6);
    await expect(handle).toBeFocused();
  });

  test('Shift moves further than a plain arrow', async ({ page }) => {
    const handle = await tabToHandle(page, 'P');
    await hold(page, 'theta', 1);
    await expect.poll(() => now(handle)).toBeCloseTo(1, 6);
    await page.keyboard.press('ArrowLeft');
    await expect.poll(() => value(page, 'theta')).not.toBeCloseTo(1, 4);
    const plain = Math.abs((await value(page, 'theta')) - 1);

    await hold(page, 'theta', 1);
    await expect.poll(() => now(handle)).toBeCloseTo(1, 6);
    await page.keyboard.press('Shift+ArrowLeft');
    await expect.poll(() => value(page, 'theta')).not.toBeCloseTo(1, 4);
    const shifted = Math.abs((await value(page, 'theta')) - 1);

    expect(plain).toBeGreaterThan(0);
    expect(shifted).toBeGreaterThan(plain * 2);
  });

  test('Home and End give the ends of the range', async ({ page }) => {
    const handle = await tabToHandle(page, 'P');
    await page.keyboard.press('End');
    await expect.poll(() => value(page, 'theta')).toBeCloseTo(1.85, 6);
    await expect.poll(() => now(handle)).toBeCloseTo(1.85, 6);
    await page.keyboard.press('Home');
    await expect.poll(() => value(page, 'theta')).toBeCloseTo(0.2, 6);
    await expect.poll(() => now(handle)).toBeCloseTo(0.2, 6);
  });

  test('PageUp and PageDown move a tenth of the range', async ({ page }) => {
    const handle = await tabToHandle(page, 'P');
    await hold(page, 'theta', 1);
    await expect.poll(() => now(handle)).toBeCloseTo(1, 6);
    const tenth = (1.85 - 0.2) / 10;
    await page.keyboard.press('PageUp');
    await expect.poll(() => value(page, 'theta')).toBeCloseTo(1 + tenth, 6);
    await page.keyboard.press('PageDown');
    await page.keyboard.press('PageDown');
    await expect.poll(() => value(page, 'theta')).toBeCloseTo(1 - tenth, 6);
  });

  test('an arrow off the handle\'s path changes little and never throws', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    const handle = await tabToHandle(page, 'P');
    // Near the top of the circle, up is straight out from it.
    await hold(page, 'theta', 1.5);
    await expect.poll(() => now(handle)).toBeCloseTo(1.5, 6);
    for (const key of ['ArrowUp', 'Shift+ArrowUp', 'ArrowDown']) await page.keyboard.press(key);
    const theta = await value(page, 'theta');
    expect(Number.isFinite(theta)).toBe(true);
    expect(theta).toBeGreaterThanOrEqual(0.2);
    expect(theta).toBeLessThanOrEqual(1.85);
    expect(errors).toEqual([]);
    await expect(handle).toBeFocused();
  });

  test('the page does not step a beat while a handle has the keys', async ({ page }) => {
    await tabToHandle(page, 'P');
    const beat = await onFigure(page, (f) => f.beat);
    await page.keyboard.press('ArrowRight');
    expect(await onFigure(page, (f) => f.beat)).toBe(beat);
  });
});

test.describe('a stepped param', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'keyed');
    await drawn(page);
  });

  test('one arrow always reaches the next step', async ({ page }) => {
    const handle = handleOf(page, 'N');
    await expect(handle).toBeVisible();
    await handle.focus();
    await expect(handle).toHaveAttribute('aria-valuemin', '-1');
    await expect(handle).toHaveAttribute('aria-valuemax', '1');
    expect(await value(page, 'n')).toBe(0);

    // The steps are a quarter of the figure apart, far more than one arrow's few pixels.
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => value(page, 'n')).toBe(1);
    await expect(handle).toHaveAttribute('aria-valuetext', 'n = 1');
    await page.keyboard.press('ArrowLeft');
    await expect.poll(() => value(page, 'n')).toBe(0);
    await page.keyboard.press('ArrowLeft');
    await expect.poll(() => value(page, 'n')).toBe(-1);
    // At the end of the range it stays.
    await page.keyboard.press('ArrowLeft');
    expect(await value(page, 'n')).toBe(-1);
  });

  test('every value it takes is a whole step', async ({ page }) => {
    const handle = handleOf(page, 'N');
    await handle.focus();
    for (const key of ['ArrowRight', 'Shift+ArrowLeft', 'ArrowRight', 'PageUp', 'PageDown']) {
      await page.keyboard.press(key);
      const n = await value(page, 'n');
      expect(Number.isInteger(n)).toBe(true);
    }
  });

  test('a handle that is not there is hidden', async ({ page }) => {
    await onFigure(page, (f) => f.hold('shown', 0));
    await expect(handleOf(page, 'Q')).toBeHidden();
    await expect(handleOf(page, 'N')).toBeVisible();
  });
});

test.describe('beats by keyboard on 004-bounds', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'bounds');
    await drawn(page);
  });

  const beat = (page) => onFigure(page, (f) => Math.round(f.beat));

  test('the figure is a tab stop and a group', async ({ page }) => {
    await page.keyboard.press('Tab');
    await expect(figureOf(page)).toBeFocused();
    await expect(figureOf(page).locator('.said')).toHaveAttribute('aria-live', 'polite');
    expect(await onFigure(page, (f) => f.info.beats.length)).toBe(10);
  });

  test('right and left step one beat, End and Home go to the ends', async ({ page }) => {
    await figureOf(page).focus();
    expect(await beat(page)).toBe(0);

    await page.keyboard.press('ArrowRight');
    await expect.poll(() => beat(page)).toBe(1);
    await expect(figureOf(page).locator('.said')).toHaveText(/^Step 2 of 10/);
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => beat(page)).toBe(2);
    await expect(figureOf(page).locator('.said')).toHaveText(/^Step 3 of 10: .+/);
    await page.keyboard.press('ArrowLeft');
    await expect.poll(() => beat(page)).toBe(1);
    await expect(figureOf(page).locator('.said')).toHaveText(/^Step 2 of 10/);

    await page.keyboard.press('End');
    await expect.poll(() => beat(page)).toBe(9);
    await expect(figureOf(page).locator('.said')).toHaveText(/^Step 10 of 10/);
    await page.keyboard.press('Home');
    await expect.poll(() => beat(page)).toBe(0);
    await expect(figureOf(page).locator('.said')).toHaveText(/^Step 1 of 10/);
  });

  test('stepping pauses, and the caption is the beat\'s own', async ({ page }) => {
    await figureOf(page).focus();
    await page.keyboard.press('ArrowRight');
    const caption = await onFigure(page, (f) => f.info.beats[1]);
    await expect(figureOf(page).locator('.said')).toHaveText(`Step 2 of 10: ${caption}`);
    expect(await onFigure(page, (f) => f.paused)).toBe(true);
  });

  test('the ends hold', async ({ page }) => {
    await figureOf(page).focus();
    await page.keyboard.press('ArrowLeft');
    expect(await beat(page)).toBe(0);
    await page.keyboard.press('End');
    await expect.poll(() => beat(page)).toBe(9);
    await page.keyboard.press('ArrowRight');
    expect(await beat(page)).toBe(9);
  });
});
