import { test, expect } from '@playwright/test';

// web/imports.js maps the package's two names to the source files, or to the bundle with `?min`.
// The pages there are served by this suite's server, from the repo root.
for (const [query, label] of [['', 'source files'], ['?min', 'the bundle']]) {
  test(`a page in web/ draws from ${label}`, async ({ page }) => {
    const files = [];
    const errors = [];
    page.on('request', (r) => files.push(new URL(r.url()).pathname));
    page.on('pageerror', (e) => errors.push(e.message));
    page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
    await page.goto(`/web/circle.html${query}`);
    const figure = page.locator('pencil-figure').first();
    await expect(figure.locator('canvas')).toBeVisible();
    await expect.poll(() => figure.evaluate((f) => f.backend)).toBeTruthy();
    expect(errors).toEqual([]);
    const own = files.filter((path) => path.startsWith('/packages/figure/'));
    if (query) {
      expect(own.filter((path) => path.startsWith('/packages/figure/src/'))).toEqual([]);
      expect(own).toContain('/packages/figure/pkg/pencil-figure.min.js');
      expect(own).not.toContain('/packages/figure/pkg/pencil_web.js');
    } else {
      expect(own).toContain('/packages/figure/src/element.js');
      expect(own).toContain('/packages/figure/src/host.js');
      expect(own).not.toContain('/packages/figure/pkg/pencil-figure.min.js');
    }
    expect(own).toContain('/packages/figure/pkg/pencil_web_bg.wasm');
  });
}
