import { test, expect } from '@playwright/test';
import { open, drawn, onFigure, events } from './helpers.js';

const paper = (page) => onFigure(page, (f) => f.colors.paper_color);

test.describe('<source media> picks sheets', () => {
  test('light and dark each pick their own', async ({ browser }, testInfo) => {
    const colors = {};
    for (const colorScheme of ['light', 'dark']) {
      const context = await browser.newContext({ ...testInfo.project.use, colorScheme });
      const page = await context.newPage();
      await open(page, 'theme');
      await drawn(page);
      colors[colorScheme] = await paper(page);
      await context.close();
    }
    expect(colors.light).toBeTruthy();
    expect(colors.dark).toBeTruthy();
    expect(colors.light).not.toBe(colors.dark);
  });

  test('switching the scheme while running changes it without a reload', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await open(page, 'theme');
    await drawn(page);
    const light = await paper(page);
    await page.evaluate(() => { window.__marker = 'same document'; });
    const loads = (await events(page)).load;

    await page.emulateMedia({ colorScheme: 'dark' });
    await expect.poll(() => paper(page)).not.toBe(light);
    const dark = await paper(page);
    expect((await events(page)).load).toBeGreaterThan(loads);
    expect(await page.evaluate(() => window.__marker)).toBe('same document');

    await page.emulateMedia({ colorScheme: 'light' });
    await expect.poll(() => paper(page)).toBe(light);
    expect(dark).not.toBe(light);
  });
});
