import { test, expect } from '@playwright/test';
import { open, drawn, onFigure, value } from './helpers.js';

test('a range that follows another param keeps the param inside it', async ({ page }) => {
  await open(page, 'lattice');
  await drawn(page);
  const range = (name) => onFigure(page, (f, n) => f.range(n), name);

  await onFigure(page, (f) => f.hold('q', 9));
  await onFigure(page, (f) => f.hold('p', 6));
  await expect.poll(() => value(page, 'p')).toBe(6);
  expect(await range('p')).toEqual([0, 9]);

  await onFigure(page, (f) => f.hold('q', 4));
  await expect.poll(() => value(page, 'p')).toBe(4);
  expect(await range('p')).toEqual([0, 4]);

  await onFigure(page, (f) => f.hold('q', 9));
  await expect.poll(() => range('p')).toEqual([0, 9]);
  // It was held at 6 before, and the figure remembers that.
  await expect.poll(() => value(page, 'p')).toBe(6);

  // info says the widest it gets, and what moves it.
  const info = await onFigure(page, (f) => f.info.params.find((p) => p.id === 'p'));
  expect(info.follows).toContain('q');
  expect(info.min).toBe(0);
});
