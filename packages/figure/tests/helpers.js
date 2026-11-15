import { expect } from '@playwright/test';

const PAGES = '/packages/figure/tests/pages';

/** Opens a fixture page. PENCIL_GL=1 adds `?gl` to run every figure on WebGL2. */
export async function open(page, name, query = '') {
  const params = new URLSearchParams(query);
  if (process.env.PENCIL_GL) params.set('gl', '');
  const text = params.toString().replace(/gl=(&|$)/, 'gl$1');
  await page.goto(`${PAGES}/${name}.html${text ? `?${text}` : ''}`);
  return figureOf(page);
}

/** The figure's locator. */
export const figureOf = (page) => page.locator('#figure');

/** What the page's boot script recorded: counts of events, the `input` details and the errors. */
export const events = (page) => page.evaluate(() => window.__pencil);

/** Runs `fn(figure, arg)` in the page, on the figure. */
export const onFigure = (page, fn, arg) => figureOf(page).evaluate(fn, arg);

/** Waits until the figure has loaded and drawn a frame, and fails at once if it reported an error. */
export async function drawn(page) {
  await expect
    .poll(async () => {
      const log = await events(page);
      if (log.errors.length) throw new Error(`figure error: ${log.errors.join('; ')}`);
      return log.load > 0 && log.draw > 0;
    }, { message: 'the figure should load and draw', timeout: 30_000 })
    .toBe(true);
}

/** A param's value this frame. */
export const value = (page, name) => onFigure(page, (f, n) => f.value(n), name);

/** Tabs from the top of the page until the focused handle in the figure is named `name`. */
export async function tabToHandle(page, name) {
  for (let i = 0; i < 5; i++) {
    await page.keyboard.press('Tab');
    const focused = await onFigure(page, (f) => f.shadowRoot.activeElement?.getAttribute('aria-label') ?? null);
    if (focused === name) return handleOf(page, name);
  }
  throw new Error(`Tab never reached the handle ${name}`);
}

/** The focusable over a handle. */
export const handleOf = (page, name) => figureOf(page).getByRole('slider', { name });

/** How many distinct colors a locator's screenshot has. */
export async function colorsIn(page, locator) {
  const png = (await locator.screenshot()).toString('base64');
  return page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
    const bitmap = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext('2d');
    context.drawImage(bitmap, 0, 0);
    const pixels = context.getImageData(0, 0, bitmap.width, bitmap.height).data;
    const seen = new Set();
    for (let i = 0; i < pixels.length; i += 4) {
      seen.add((pixels[i] << 16) | (pixels[i + 1] << 8) | pixels[i + 2]);
      if (seen.size > 8) break;
    }
    return seen.size;
  }, png);
}

/** The center of an element's box, in viewport CSS pixels. */
export async function centerOf(locator) {
  const box = await locator.boundingBox();
  if (!box) throw new Error('no box');
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}
