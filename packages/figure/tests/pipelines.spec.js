import { test, expect as base } from '@playwright/test';
import { open, drawn, events, figureOf } from './helpers.js';

// A figure's render pipelines are made with `createRenderPipelineAsync`. Chrome compiles a
// pipeline made with `createRenderPipeline` on its GPU process's main thread, which composites the
// page, and every frame on the page waited for it. These watch the pipelines a page asks its GPU
// devices for, hold some back or fail them, lose the GPU while they compile, and check what the
// figure shows and says meanwhile. WebGL2 compiles where it draws, so most are for WebGPU.

const expect = base.configure({ timeout: 30_000 });
const webgpu = () => test.skip(!!process.env.PENCIL_GL, 'WebGL2 makes its pipelines as it always did');

/**
 * Spies on every GPU device the page makes, on the device itself, before the page's own scripts
 * run: each render pipeline it's asked for, sync or async, by label. While `window.__fail` is
 * `true` the async ones reject, and while it's a number that many more do. While `window.__hold`
 * is a number of milliseconds they resolve that much later, counted in `__held` and then
 * `__released`. `hold` and `fail` set them from the start.
 */
const spy = (page, hold = 0, fail = false) =>
  page.addInitScript(([hold, fail]) => {
    const calls = (window.__pipelines = []);
    window.__hold = hold;
    window.__fail = fail;
    window.__held = 0;
    window.__released = 0;
    if (!self.GPUAdapter) return;
    const request = GPUAdapter.prototype.requestDevice;
    GPUAdapter.prototype.requestDevice = async function (descriptor) {
      const device = await request.call(this, descriptor);
      const sync = device.createRenderPipeline;
      const async = device.createRenderPipelineAsync;
      device.createRenderPipeline = function (d) {
        calls.push(['sync', d.label]);
        return sync.call(this, d);
      };
      device.createRenderPipelineAsync = function (d) {
        calls.push(['async', d.label]);
        if (window.__fail) {
          if (typeof window.__fail === 'number') window.__fail -= 1;
          return Promise.reject(new TypeError('no pipeline, for a test'));
        }
        const made = async.call(this, d);
        const hold = window.__hold;
        if (!hold) return made;
        window.__held += 1;
        return new Promise((resolve, reject) =>
          setTimeout(() => {
            window.__released += 1;
            made.then(resolve, reject);
          }, hold),
        );
      };
      return device;
    };
  }, [hold, fail]);

const pipelines = (page) => page.evaluate(() => window.__pipelines);
const page$ = (page, name) => page.evaluate((name) => window[name], name);
const BANDS = '../../../../corpus/theme.style ../../../../corpus/001-circle/bands.style';

/** Notes each `error` a figure sends, with its detail. */
const errors = (figure) =>
  figure.evaluate((f) => {
    window.__errors = [];
    f.addEventListener('error', (e) => window.__errors.push({ message: String(e.detail.error?.message), fresh: e.detail.fresh, kept: !!e.detail.kept }));
  });

/** Loses the GPU device under a figure's player, as a browser does, and waits for the player that
 * takes its place. */
async function lose(figure) {
  await figure.evaluate((f) => {
    f.player.__before = true;
    f.player.lose_device();
  });
  await expect.poll(() => figure.evaluate((f) => !!f.player && !f.player.__before)).toBe(true);
}

/** Loses it while its pipelines are held, and holds the next player's no longer. */
async function loseWhileHeld(page, figure) {
  await expect.poll(() => page$(page, '__held')).toBeGreaterThan(0);
  await page.evaluate(() => (window.__hold = 0));
  await lose(figure);
}

/** Waits until the held pipelines have come back, and a frame after. */
async function released(page) {
  const held = await page$(page, '__held');
  await expect.poll(() => page$(page, '__released')).toBe(held);
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 100))));
}

test("a figure's pipelines are made with createRenderPipelineAsync, and never the sync call", async ({ page }) => {
  webgpu();
  await spy(page);
  const figure = await open(page, 'circle');
  await drawn(page);
  expect(await figure.evaluate((f) => f.backend)).toBe('BrowserWebGpu');
  // Another look, which is another load.
  await figure.evaluate((f, sheets) => f.setAttribute('sheets', sheets), BANDS);
  await expect.poll(async () => (await events(page)).load).toBe(2);
  const calls = await pipelines(page);
  expect(calls.filter(([kind]) => kind === 'sync')).toEqual([]);
  const made = calls.filter(([kind]) => kind === 'async').map(([, label]) => label);
  expect(made.filter((label) => label === 'pencil figure')).toHaveLength(2);
  expect(made.filter((label) => label === 'pencil coverage')).toHaveLength(2);
  expect((await events(page)).errors).toEqual([]);
});

test('a figure whose pipelines fail keeps the one before on screen, and says so', async ({ page }) => {
  webgpu();
  await spy(page);
  // A figure with a poster, which would show again if the figure before were let go of.
  const figure = await open(page, 'poster');
  await drawn(page);
  const waiting = () => figure.evaluate((f) => f.shadowRoot.querySelector('.stage').classList.contains('waiting'));
  expect(await waiting()).toBe(false);
  await errors(figure);
  const before = await figure.evaluate((f) => JSON.stringify(f.info));
  await page.evaluate(() => (window.__fail = true));
  await figure.evaluate((f, sheets) => f.setAttribute('sheets', sheets), BANDS);
  await expect.poll(() => page$(page, '__errors')).toHaveLength(1);
  const [error] = await page$(page, '__errors');
  expect(error.message).toMatch(/^GPU error: .*no pipeline, for a test/);
  // What failed is the new figure, and the one on screen stays: no reload is needed.
  expect(error).toMatchObject({ fresh: true, kept: true });
  expect((await events(page)).load).toBe(1);
  // The figure before is still the one there, with no poster over it, and it draws on.
  expect(await figure.evaluate((f) => JSON.stringify(f.info))).toBe(before);
  expect(await waiting()).toBe(false);
  const draws = (await events(page)).draw;
  await figure.evaluate((f) => (f.time = 1.5));
  await expect.poll(async () => (await events(page)).draw).toBeGreaterThan(draws);
  // Once the GPU can, the new look loads.
  await page.evaluate(() => (window.__fail = false));
  await figure.evaluate((f) => f.reload());
  await expect.poll(async () => (await events(page)).load).toBe(2);
  expect(await page$(page, '__errors')).toHaveLength(1);
});

test("a first figure whose pipelines fail keeps nothing, and its poster stays", async ({ page }) => {
  webgpu();
  await spy(page, 0, true);
  await page.addInitScript(() => {
    window.__errors = [];
    document.addEventListener('error', (e) => e.target?.localName === 'pencil-figure' && window.__errors.push({ kept: e.detail.kept, fresh: e.detail.fresh }), true);
  });
  const figure = await open(page, 'poster');
  await expect.poll(async () => (await events(page)).errors.length).toBe(1);
  expect(await page$(page, '__errors')).toEqual([{ kept: false, fresh: true }]);
  expect((await events(page)).errors[0]).toMatch(/^GPU error: /);
  expect(await figure.evaluate((f) => f.shadowRoot.querySelector('.stage').classList.contains('waiting'))).toBe(true);
  expect((await events(page)).load).toBe(0);
  expect(await figure.evaluate((f) => f.player.info())).toBe('null');
});

test.describe('a GPU lost while a figure compiles', () => {
  test.beforeEach(() => webgpu());

  test('before it was ever shown, it starts again and says load once', async ({ page }) => {
    await spy(page, 2000);
    const figure = await open(page, 'poster');
    await loseWhileHeld(page, figure);
    await expect.poll(async () => (await events(page)).load).toBe(1);
    await drawn(page);
    // The lost player's pipelines come back after, and change nothing.
    await released(page);
    expect((await events(page)).load).toBe(1);
    expect((await events(page)).errors).toEqual([]);
  });

  test('a new figure in place of one on screen is shown on the next player, and says load', async ({ page }) => {
    await spy(page);
    const figure = await open(page, 'poster');
    await drawn(page);
    await page.evaluate(() => (window.__hold = 2000));
    await figure.evaluate((f, sheets) => f.setAttribute('sheets', sheets), BANDS);
    await loseWhileHeld(page, figure);
    await expect.poll(async () => (await events(page)).load).toBe(2);
    await released(page);
    expect((await events(page)).load).toBe(2);
    expect((await events(page)).errors).toEqual([]);
  });

  test("a new figure's pipelines failing on the next player bring back the figure that drew", async ({ page }) => {
    await spy(page);
    const figure = await open(page, 'poster');
    await drawn(page);
    await errors(figure);
    const before = await figure.evaluate((f) => JSON.stringify(f.info));
    await page.evaluate(() => (window.__hold = 2000));
    await figure.evaluate((f, sheets) => f.setAttribute('sheets', sheets), BANDS);
    await expect.poll(() => page$(page, '__held')).toBeGreaterThan(0);
    // The next player's first pipelines, the new figure's figure and coverage, fail.
    await page.evaluate(() => {
      window.__hold = 0;
      window.__fail = 2;
    });
    await lose(figure);
    await expect.poll(() => page$(page, '__errors')).toHaveLength(1);
    // Nothing was on the new player to keep, so the figure that drew comes back on it.
    expect((await page$(page, '__errors'))[0]).toMatchObject({ fresh: true, kept: false });
    await expect.poll(() => figure.evaluate((f) => f.player.info() !== 'null')).toBe(true);
    expect(await figure.evaluate((f) => JSON.stringify(f.info))).toBe(before);
    await expect.poll(() => figure.evaluate((f) => f.shadowRoot.querySelector('.stage').classList.contains('waiting'))).toBe(false);
    // It's the figure the page already heard of: no news.
    await released(page);
    expect((await events(page)).load).toBe(1);
    expect(await page$(page, '__errors')).toHaveLength(1);
  });

  test('a beat set on a figure whose beats time does not drive is held, and kept once it is in', async ({ page }) => {
    await spy(page);
    const figure = await open(page, 'controlled', 'figure=beats');
    await drawn(page);
    expect(await figure.evaluate((f) => f.info.timeDriven.includes('beat'))).toBe(false);
    await page.evaluate(() => (window.__hold = 2000));
    await lose(figure);
    await expect.poll(() => page$(page, '__held')).toBeGreaterThan(0);
    expect(await figure.evaluate((f) => f.player.info())).toBe('null');
    await figure.evaluate((f) => (f.beat = 2));
    await page.evaluate(() => (window.__hold = 0));
    await released(page);
    await expect.poll(() => figure.evaluate((f) => f.player.info() !== 'null')).toBe(true);
    expect(await figure.evaluate((f) => f.held.get('beat'))).toBe(2);
    expect(await figure.evaluate((f) => f.beat)).toBe(2);
  });

  test('the keys and handles wait for it, and its beats and time stay as they were', async ({ page }) => {
    await spy(page);
    const figure = await open(page, 'bounds');
    await drawn(page);
    await figure.evaluate((f) => (f.time = 3));
    const time = await figure.evaluate((f) => f.time);
    const beat = await figure.evaluate((f) => f.beat);
    // The next player's pipelines are held back, so it compiles for a while.
    await page.evaluate(() => (window.__hold = 2000));
    await lose(figure);
    await expect.poll(() => page$(page, '__held')).toBeGreaterThan(0);
    // The new player is compiling: nothing to drag, and the keys step nothing.
    expect(await figure.evaluate((f) => f.player.info())).toBe('null');
    expect(await figure.evaluate((f) => f.shadowRoot.querySelector('.stage').classList.contains('live'))).toBe(false);
    await figureOf(page).focus();
    for (const key of ['End', 'Home', 'ArrowRight']) await page.keyboard.press(key);
    expect(await figure.evaluate((f) => f.time)).toBe(time);
    expect(await figure.evaluate((f) => f.shadowRoot.querySelector('.said').textContent)).not.toMatch(/NaN/);
    // Once it's in, it's where it was, and the keys step it.
    await page.evaluate(() => (window.__hold = 0));
    await released(page);
    await expect.poll(() => figure.evaluate((f) => f.player.info() !== 'null')).toBe(true);
    expect(await figure.evaluate((f) => f.time)).toBe(time);
    expect(await figure.evaluate((f) => f.beat)).toBeCloseTo(beat, 9);
    expect(await figure.evaluate((f) => f.shadowRoot.querySelector('.stage').classList.contains('live'))).toBe(true);
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => figure.evaluate((f) => Math.round(f.beat))).toBe(Math.floor(beat + 1e-6) + 1);
    expect((await events(page)).errors).toEqual([]);
  });
});

test('a load asked for before the GPU starts says once it is shown, and one taken over says it was not', async ({ page }) => {
  await open(page, 'circle');
  await drawn(page);
  // A figure of its own, out of view, so it has no GPU yet, and no sources until it's given some.
  const texts = await page.evaluate(() =>
    Promise.all(
      ['../../../../corpus/001-circle/circle.pen', '../../../../corpus/theme.style', '../../../../corpus/001-circle/bands.style', '../../../../corpus/001-circle/circle.style'].map(
        async (url) => (await fetch(url)).text(),
      ),
    ),
  );
  await page.evaluate(([doc, theme, bands, circle]) => {
    const spacer = document.createElement('div');
    spacer.style.height = '300vh';
    const late = document.createElement('pencil-figure');
    late.id = 'late';
    document.body.append(spacer, late);
    window.__shown = {};
    late.load(doc, [theme, bands]).then((shown) => (window.__shown.first = shown));
    late.load(doc, [theme, circle]).then((shown) => (window.__shown.second = shown));
  }, texts);
  await expect.poll(() => page.evaluate(() => window.__shown.first)).toBe(false);
  const late = page.locator('#late');
  expect(await late.evaluate((f) => f.player)).toBeNull();
  expect(await page.evaluate(() => window.__shown.second)).toBeUndefined();
  await late.scrollIntoViewIfNeeded();
  await expect.poll(() => page.evaluate(() => window.__shown.second)).toBe(true);
  expect(await late.evaluate((f) => f.player.info() !== 'null')).toBe(true);
});

/** The circle's texts: its document, the theme, and its two looks. */
const circleTexts = (page) =>
  page.evaluate(() =>
    Promise.all(
      ['../../../../corpus/001-circle/circle.pen', '../../../../corpus/theme.style', '../../../../corpus/001-circle/bands.style', '../../../../corpus/001-circle/circle.style'].map(
        async (url) => (await fetch(url)).text(),
      ),
    ),
  );

/** Adds a figure of its own far down the page, with no sources and so far no GPU, and asks it for
 * the circle, noting when its load resolves and each `load` it sends. */
async function lateFigure(page) {
  const texts = await circleTexts(page);
  await page.evaluate(([doc, theme, , circle]) => {
    const spacer = document.createElement('div');
    spacer.id = 'spacer';
    spacer.style.height = '300vh';
    const late = document.createElement('pencil-figure');
    late.id = 'late';
    document.body.append(spacer, late);
    window.__lateLoads = 0;
    late.addEventListener('load', () => (window.__lateLoads += 1));
    window.__shown = {};
    late.load(doc, [theme, circle]).then((shown) => (window.__shown.late = shown));
  }, texts);
  return page.locator('#late');
}

test("a load whose figure's GPU can't start says so, and a later start still shows it", async ({ page }) => {
  await open(page, 'circle');
  await drawn(page);
  // Starts fail, as on a page with no GPU to use. It's the module the element starts from: the
  // source, or the bundle.
  await page.evaluate(async () => {
    const host = await import(new URLSearchParams(location.search).has('min') ? '../../pkg/pencil-figure.min.js' : '../../src/host.js');
    const create = host.Player.create;
    host.Player.create = async (...args) => {
      if (window.__failStarts) throw new Error('no GPU, for a test');
      return create.apply(host.Player, args);
    };
    window.__failStarts = true;
  });
  const late = await lateFigure(page);
  await late.scrollIntoViewIfNeeded();
  await expect.poll(() => page.evaluate(() => window.__shown.late)).toBe(false);
  expect(await late.evaluate((f) => f.player)).toBeNull();
  // Seen again, with a GPU, it starts from what was asked for, and says so.
  await page.evaluate(() => {
    window.__failStarts = false;
    scrollTo(0, 0);
  });
  await page.waitForTimeout(300);
  await late.scrollIntoViewIfNeeded();
  await expect.poll(() => page.evaluate(() => window.__lateLoads)).toBe(1);
  expect(await late.evaluate((f) => f.player.info() !== 'null')).toBe(true);
});

test("a load whose figure leaves the page says so, and the figure put back still shows it", async ({ page }) => {
  await open(page, 'circle');
  await drawn(page);
  const late = await lateFigure(page);
  expect(await page.evaluate(() => window.__shown.late)).toBeUndefined();
  await late.evaluate((f) => {
    window.__late = f;
    f.remove();
  });
  await expect.poll(() => page.evaluate(() => window.__shown.late)).toBe(false);
  expect(await page.evaluate(() => window.__lateLoads)).toBe(0);
  // Put back, near enough to start, it shows what it was asked for, and says so.
  await page.evaluate(() => document.body.prepend(window.__late));
  await expect.poll(() => page.evaluate(() => window.__lateLoads)).toBe(1);
  expect(await page.evaluate(() => window.__late.player.info() !== 'null')).toBe(true);
});

test("a load listener's own load doesn't take the place of the one just shown", async ({ page }) => {
  const figure = await open(page, 'circle');
  await drawn(page);
  const texts = await circleTexts(page);
  await figure.evaluate((f, [doc, theme, bands, circle]) => {
    window.__shown = {};
    f.addEventListener('load', () => f.load(doc, [theme, circle]).then((shown) => (window.__shown.second = shown)), { once: true });
    f.load(doc, [theme, bands]).then((shown) => (window.__shown.first = shown));
  }, texts);
  await expect.poll(() => page.evaluate(() => window.__shown.second)).toBe(true);
  expect(await page.evaluate(() => window.__shown.first)).toBe(true);
});

/** The playground, with `?min` as the fixture pages add it. */
function playground(query) {
  const params = new URLSearchParams(query);
  if (process.env.PENCIL_MIN) params.set('min', '');
  if (process.env.PENCIL_GL) params.set('gl', '');
  return `/web/playground.html?${params.toString().replace(/\b(min|gl)=(&|$)/g, '$1$2')}`;
}

/** Changes the playground's text as typing would, and lets it compile. */
const edit = (page, from, to) =>
  page.evaluate(([from, to]) => {
    const source = document.getElementById('source');
    source.value = source.value.replace(from, to);
    source.dispatchEvent(new Event('input', { bubbles: true }));
  }, [from, to]);

test("the playground's first figure says when it was ready to draw", async ({ page }) => {
  await page.goto(playground('figure=circle'));
  await expect(page.locator('#status')).toContainText(/compiled 1 time, the last in \d+ ms, ready to draw after \d+ ms/, { timeout: 30_000 });
});

test('in the playground, an edit made while the last one compiles is the one shown', async ({ page }) => {
  webgpu();
  await spy(page);
  await page.goto(playground('figure=circle'));
  await expect(page.locator('#status')).toContainText('compiled', { timeout: 30_000 });
  const figure = page.locator('#figure');
  await figure.evaluate((f) => {
    window.__loads = 0;
    f.addEventListener('load', () => (window.__loads += 1));
  });
  // The first edit's pipelines are held back, so the second's are done first.
  await page.evaluate(() => (window.__hold = 1500));
  await edit(page, 'duration 2·π', 'duration 3');
  await expect.poll(() => page$(page, '__held')).toBeGreaterThan(0);
  await page.evaluate(() => (window.__hold = 0));
  await edit(page, 'duration 3', 'duration 4');
  await expect.poll(() => figure.evaluate((f) => f.duration)).toBe(4);
  // The first edit's pipelines come back after, and change nothing.
  await released(page);
  expect(await figure.evaluate((f) => f.duration)).toBe(4);
  expect(await page$(page, '__loads')).toBe(1);
  expect((await pipelines(page)).filter(([kind]) => kind === 'sync')).toEqual([]);
});

test('in the playground, an edit the GPU cannot make leaves the figure that drew, and nothing loads again', async ({ page }) => {
  webgpu();
  await spy(page);
  await page.goto(playground('figure=circle'));
  await expect(page.locator('#status')).toContainText('ready to draw after', { timeout: 30_000 });
  const figure = page.locator('#figure');
  await figure.evaluate((f) => {
    window.__loads = 0;
    f.addEventListener('load', () => (window.__loads += 1));
  });
  await page.evaluate(() => (window.__fail = true));
  await edit(page, 'duration 2·π', 'duration 3');
  await expect(page.locator('#errors')).toContainText("its shaders couldn't be made");
  // No going back to the last figure that drew, which is still the one there: nothing to load.
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 500)));
  expect(await page$(page, '__loads')).toBe(0);
  expect(await figure.evaluate((f) => f.duration)).toBeCloseTo(2 * Math.PI, 9);
  const asked = (await pipelines(page)).filter(([kind]) => kind === 'async').length;
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 500)));
  expect((await pipelines(page)).filter(([kind]) => kind === 'async').length).toBe(asked);
});
