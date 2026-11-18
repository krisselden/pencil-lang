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

// A page with a theme toggle pins a scheme over the reader's system, and says so with
// `color-scheme` in its CSS. The fixture's is `:root[data-theme]`.
test.describe('a scheme the page pins', () => {
  const poster = (page) => page.locator('#poster').evaluate((img) => new URL(img.currentSrc).search);
  const pin = (page, theme) =>
    page.evaluate((to) => {
      if (to) document.documentElement.dataset.theme = to;
      else delete document.documentElement.dataset.theme;
    }, theme);

  test('picks the sheets and the poster, whatever the system says', async ({ page }) => {
    const seen = {};
    for (const [system, pinned] of [['light', ''], ['dark', ''], ['light', 'dark'], ['dark', 'light']]) {
      await page.emulateMedia({ colorScheme: system });
      await open(page, 'pinned', pinned && `theme=${pinned}`);
      await drawn(page);
      seen[`${system} ${pinned}`.trim()] = { paper: await paper(page), poster: await poster(page) };
    }
    expect(seen.light.paper).not.toBe(seen.dark.paper);
    expect(seen.light.poster).toBe('');
    expect(seen.dark.poster).toBe('?dark');
    expect(seen['light dark']).toEqual(seen.dark);
    expect(seen['dark light']).toEqual(seen.light);
  });

  test('changes with the pin without a reload, and goes back to the system with it', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await open(page, 'pinned');
    await drawn(page);
    const light = await paper(page);
    await page.evaluate(() => { window.__marker = 'same document'; });

    await pin(page, 'dark');
    await expect.poll(() => paper(page)).not.toBe(light);
    const dark = await paper(page);
    await expect.poll(() => poster(page)).toBe('?dark');

    // With the pin gone the system answers again, and what was written is back as it was.
    await pin(page, null);
    await expect.poll(() => paper(page)).toBe(light);
    await expect.poll(() => poster(page)).toBe('');
    expect(await page.locator('#figure source').evaluateAll((all) => all.map((s) => [s.media, s.dataset.media ?? null]))).toEqual([
      ['(prefers-color-scheme: dark)', null],
      ['(prefers-color-scheme: dark)', null],
    ]);

    await page.emulateMedia({ colorScheme: 'dark' });
    await expect.poll(() => paper(page)).toBe(dark);
    await pin(page, 'light');
    await expect.poll(() => paper(page)).toBe(light);
    expect(await page.evaluate(() => window.__marker)).toBe('same document');
  });
});
