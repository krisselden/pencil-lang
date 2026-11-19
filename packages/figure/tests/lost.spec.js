import { test, expect as base } from '@playwright/test';
import { open, drawn, events, onFigure, figureOf, value } from './helpers.js';

// A busy machine makes everything here slow: a frame can wait seconds, and with it the moment a
// figure finds its player gone. So nothing here waits a set time for something to happen, and every
// poll is patient. What the timing proves is measured in the page (`record`), as the time between
// two things, which a busy machine can only make longer.
const expect = base.configure({ timeout: 30_000 });

// A browser loses a figure's GPU when the machine sleeps or its GPU process restarts, which takes
// every tab's at once. The figure starts its GPU again where it was: the same time, the same held
// params, nothing said to the page, and no panic in the console.

/** Loses the GPU under a figure as a browser would. Chromium's GPU process is crashed outright,
 * through CDP, which takes every figure's. WebKit has no way to, so one figure's is lost:
 * its WebGL2 context through `WEBGL_lose_context`, or its WebGPU device destroyed. */
async function loseGpu(page, browserName, figure = figureOf(page)) {
  if (browserName === 'chromium') {
    const cdp = await page.context().browser().newBrowserCDPSession();
    await cdp.send('Browser.crashGpuProcess');
    return 'crashed';
  }
  await loseOne(figure);
  return 'lost';
}

/** Loses one figure's GPU and no other's. */
const loseOne = (figure) =>
  figure.evaluate((f) => {
    if (f.backend === 'Gl') f.shadowRoot.querySelector('canvas').getContext('webgl2').getExtension('WEBGL_lose_context').loseContext();
    else f.player.lose_device();
  });

/** Marks a figure's player, so a test can tell a new one from it. */
const mark = (figure) => figure.evaluate((f) => (f.player.__before = true));
/** Whether a figure has a new player, drawing on a backend. */
const fresh = (figure) => figure.evaluate((f) => !!f.player && !f.player.__before && !!f.backend);

/** Traps a figure's player, as a panic inside wgpu would, and resizes it, so it draws. */
const trap = (figure) =>
  figure.evaluate((f) => {
    if (!f.player) return;
    f.player.__before = true;
    try {
      f.player.trap();
    } catch {}
    f.style.width = f.style.width === '520px' ? '540px' : '520px';
  });

/** Wraps the start of a GPU, which the element looks up each time it starts one: it's noted, and
 * fails while `failStarts` says so, as while a browser's GPU process is still coming back. It's the
 * module the element starts from: the source, or the bundle. */
const wrapStarts = (page) =>
  page.evaluate(async () => {
    const host = await import(new URLSearchParams(location.search).has('min') ? '../../pkg/pencil-figure.min.js' : '../../src/host.js');
    if (window.__create) return;
    window.__create = host.Player.create;
    window.__record ??= [];
    host.Player.create = async (...args) => {
      window.__record.push(['attempt', performance.now()]);
      if (window.__failStarts) throw new Error('no GPU, for a test');
      return window.__create.apply(host.Player, args);
    };
  });

const failStarts = async (page, failing) => {
  await wrapStarts(page);
  await page.evaluate((failing) => (window.__failStarts = failing), failing);
};

/**
 * Notes, as they happen, when a figure lets go of its GPU (`drop`), when it has a player again
 * (`live`), each start of a GPU (`attempt`), and each `error` and `recover`. The stage's `live`
 * class is watched, which the element changes in the same task as it drops or takes a player.
 * Gives a function that reads the notes.
 */
async function record(page, figure) {
  await wrapStarts(page);
  await figure.evaluate((f) => {
    window.__record ??= [];
    const note = (what) => window.__record.push([what, performance.now()]);
    new MutationObserver((changes) => {
      for (const change of changes) {
        const was = (change.oldValue ?? '').split(/\s+/).includes('live');
        const is = change.target.classList.contains('live');
        if (was && !is) note('drop');
        if (!was && is) note('live');
      }
    }).observe(f.shadowRoot.querySelector('.stage'), { attributes: true, attributeFilter: ['class'], attributeOldValue: true });
    f.addEventListener('error', () => note('error'));
    f.addEventListener('recover', () => note('recover'));
    // On WebGL2 a figure waits for its context to come back before it starts again.
    f.shadowRoot.querySelector('.stage').addEventListener('webglcontextrestored', () => note('restored'), true);
  });
  return () => page.evaluate(() => window.__record);
}

// A drop is noted just after the task that made it, so a gap measured from one can come out
// a little short of the timer behind it, which can't fire early.
const SLACK = 50;

/** The times of one kind of note. */
const times = (notes, what) => notes.filter(([w]) => w === what).map(([, t]) => t);

/** When a figure could start again after `t`: then, or once its WebGL2 context was back, if it
 * came back before `until`. */
const ready = (notes, t, until) => Math.max(t, ...times(notes, 'restored').filter((r) => r >= t && r <= until));

/** Counts an event the figures send. */
async function counter(page, type) {
  await page.evaluate((t) => {
    window.__counts ??= {};
    window.__counts[t] = 0;
    document.addEventListener(t, (e) => { if (e.target.localName === 'pencil-figure') window.__counts[t] += 1; }, true);
  }, type);
  return () => page.evaluate((t) => window.__counts[t], type);
}

const panics = (page) => {
  const lines = [];
  page.on('console', (m) => {
    if (/panicked at|unreachable/.test(m.text())) lines.push(m.text());
  });
  return lines;
};

/** Holds a moment, so it can be checked after: a time and a param a reader set. */
async function setMoment(figure) {
  await figure.evaluate((f) => {
    f.time = 4.5;
    f.hold('theta', 1.3);
  });
}

async function expectMoment(page, figure) {
  expect(await figure.evaluate((f) => f.time)).toBeCloseTo(4.5, 9);
  expect(await figure.evaluate((f) => f.value('theta'))).toBeCloseTo(1.3, 9);
  expect(await figure.evaluate((f) => [...f.held.keys()])).toContain('theta');
}

/** Draws counted for one figure, by its id. */
async function countDraws(page) {
  await page.evaluate(() => {
    window.__draws = {};
    document.addEventListener('draw', (e) => {
      const id = e.target.id;
      window.__draws[id] = (window.__draws[id] ?? 0) + 1;
    }, true);
  });
  return (id) => page.evaluate((i) => window.__draws[i] ?? 0, id);
}

test.describe('a figure whose GPU is lost', () => {
  // Seconds of tries, and frames a busy machine makes wait.
  test.describe.configure({ timeout: 180_000 });
  test('starts again where it was, after its GPU is lost and it is resized', async ({ page, browserName }) => {
    const lines = panics(page);
    const figure = await open(page, 'bounds');
    await drawn(page);
    const draws = await countDraws(page);
    await setMoment(figure);
    await mark(figure);
    await loseGpu(page, browserName);
    await figure.evaluate((f) => (f.style.width = '520px'));

    // A new player, drawing.
    await expect.poll(() => fresh(figure)).toBe(true);
    const now = await draws('figure');
    // A draw that changes nothing it was holding: a new size.
    await figure.evaluate((f) => (f.style.width = '540px'));
    await expect.poll(() => draws('figure')).toBeGreaterThan(now);
    await expectMoment(page, figure);
    expect((await events(page)).errors).toEqual([]);
    expect(lines).toEqual([]);
  });

  test('starts again on WebGL2 when its context is lost', async ({ page }) => {
    const lines = panics(page);
    const figure = await open(page, 'bounds', 'gl');
    await drawn(page);
    const draws = await countDraws(page);
    await setMoment(figure);
    await mark(figure);
    await loseOne(figure);
    await figure.evaluate((f) => (f.style.width = '520px'));
    await expect.poll(() => fresh(figure)).toBe(true);
    const now = await draws('figure');
    // A draw that changes nothing it was holding: a new size.
    await figure.evaluate((f) => (f.style.width = '540px'));
    await expect.poll(() => draws('figure')).toBeGreaterThan(now);
    expect(await figure.evaluate((f) => f.backend)).toBe('Gl');
    await expectMoment(page, figure);
    expect((await events(page)).errors).toEqual([]);
    expect(lines).toEqual([]);
  });

  test('with another figure on the page, that one draws on', async ({ page }) => {
    const lines = panics(page);
    await open(page, 'two');
    await drawn(page);
    const other = page.locator('#other');
    await expect.poll(() => other.evaluate((f) => !!f.backend)).toBe(true);
    const draws = await countDraws(page);
    const figure = figureOf(page);
    await setMoment(figure);
    await mark(figure);
    await mark(other);
    await loseOne(figure);
    await figure.evaluate((f) => (f.style.width = '520px'));

    // The other one keeps its player, and draws as a reader moves it.
    await other.evaluate((f) => f.hold('theta', 0.7));
    await expect.poll(() => draws('other')).toBeGreaterThan(0);
    expect(await other.evaluate((f) => f.player.__before)).toBe(true);
    expect(await other.evaluate((f) => f.value('theta'))).toBeCloseTo(0.7, 9);
    // And the lost one is back, on a new one.
    await expect.poll(() => fresh(figure)).toBe(true);
    const now = await draws('figure');
    // A draw that changes nothing it was holding: a new size.
    await figure.evaluate((f) => (f.style.width = '540px'));
    await expect.poll(() => draws('figure')).toBeGreaterThan(now);
    await expectMoment(page, figure);
    expect((await events(page)).errors).toEqual([]);
    expect(lines).toEqual([]);
  });

  test('both start again when the browser loses every GPU device', async ({ page, browserName }) => {
    test.skip(browserName !== 'chromium', "Only Chromium's GPU process can be crashed from a test");
    const lines = panics(page);
    await open(page, 'two');
    await drawn(page);
    const other = page.locator('#other');
    await expect.poll(() => other.evaluate((f) => !!f.backend)).toBe(true);
    const draws = await countDraws(page);
    const figures = [figureOf(page), other];
    for (const f of figures) await mark(f);
    await loseGpu(page, browserName);
    await figures[0].evaluate((el) => (el.style.width = '520px'));
    for (const f of figures) {
      await expect.poll(() => fresh(f)).toBe(true);
    }
    const now = [await draws('figure'), await draws('other')];
    for (const f of figures) await f.evaluate((el, theta) => el.hold('theta', theta), 1.1);
    await expect.poll(() => draws('figure')).toBeGreaterThan(now[0]);
    await expect.poll(() => draws('other')).toBeGreaterThan(now[1]);
    expect(await value(page, 'theta')).toBeCloseTo(1.1, 9);
    expect((await events(page)).errors).toEqual([]);
    expect(lines).toEqual([]);
  });

  // A panic inside wgpu can't all be ruled out, and traps the call in WASM, which leaves that
  // player unusable. `trap()` is one on purpose.
  test('gets past a trap, tries again after a second and after five, and then says so', async ({ page }) => {
    await open(page, 'two');
    await drawn(page);
    const figure = figureOf(page);
    const other = page.locator('#other');
    await expect.poll(() => other.evaluate((f) => !!f.backend)).toBe(true);
    const draws = await countDraws(page);
    await setMoment(figure);
    await mark(other);
    const recovered = await counter(page, 'recover');
    const notes = await record(page, figure);

    // Once: it starts again, where it was, and the other figure is untouched.
    await trap(figure);
    await expect.poll(() => fresh(figure), { timeout: 60_000 }).toBe(true);
    await expectMoment(page, figure);
    const now = await draws('other');
    await other.evaluate((f) => f.hold('theta', 0.7));
    await expect.poll(() => draws('other')).toBeGreaterThan(now);
    expect(await other.evaluate((f) => f.player.__before)).toBe(true);

    // Twice and three times: it starts again each time, and says nothing.
    for (let i = 0; i < 2; i++) {
      await trap(figure);
      await expect.poll(() => fresh(figure), { timeout: 60_000 }).toBe(true);
      await expectMoment(page, figure);
      expect((await events(page)).errors).toEqual([]);
    }

    // A fourth: that's an error, with no tab switch.
    await trap(figure);
    await expect.poll(async () => (await events(page)).errors.length, { timeout: 60_000 }).toBe(1);
    expect(await figure.evaluate((f) => f.player)).toBeNull();
    expect(await recovered()).toBe(0);

    // What happened, in order. Each drop was followed by one start: the first at once, the next
    // after at least a second, the third after at least five. The fourth drop was followed by no
    // start, and then the error. A busy machine only makes these gaps longer.
    const seen = await notes();
    const [drops, attempts, errors] = ['drop', 'attempt', 'error'].map((w) => times(seen, w));
    expect(drops).toHaveLength(4);
    expect(attempts).toHaveLength(3);
    expect(errors).toHaveLength(1);
    const after = attempts.map((t, n) => t - drops[n]);
    expect(after[0]).toBeGreaterThanOrEqual(0);
    expect(attempts[0] - ready(seen, drops[0], attempts[0])).toBeLessThan(500);
    expect(after[1]).toBeGreaterThanOrEqual(1000 - SLACK);
    expect(after[2]).toBeGreaterThanOrEqual(5000 - SLACK);
    for (let n = 0; n < 3; n++) expect(attempts[n]).toBeLessThan(drops[n + 1]);
    // Said as it dropped the fourth time, in the same task.
    expect(errors[0]).toBeGreaterThan(attempts[2]);
    expect(Math.abs(errors[0] - drops[3])).toBeLessThan(SLACK);

    // Shown again, it tries once more, draws, and says it's back.
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    await expect.poll(() => fresh(figure), { timeout: 60_000 }).toBe(true);
    await expect.poll(() => recovered(), { timeout: 60_000 }).toBe(1);
    await expectMoment(page, figure);
  });

  test('a GPU that stays gone is an error within seconds, on a page that stays in view', async ({ page }) => {
    const figure = await open(page, 'bounds');
    await drawn(page);
    await setMoment(figure);
    const recovered = await counter(page, 'recover');
    const notes = await record(page, figure);
    await failStarts(page, true);
    await trap(figure);
    // Nothing is said until the tries are done, and then it's said without waiting for the page
    // to be shown again, which nothing here does.
    await expect.poll(async () => (await events(page)).errors.length, { timeout: 60_000 }).toBe(1);
    const seen = await notes();
    const [drops, attempts, errors] = ['drop', 'attempt', 'error'].map((w) => times(seen, w));
    // Tried at once when it was dropped, then a second after that try failed, then five after
    // that one, and the error only after the last.
    expect(drops).toHaveLength(1);
    expect(attempts).toHaveLength(3);
    expect(errors).toHaveLength(1);
    expect(attempts[0] - ready(seen, drops[0], attempts[0])).toBeLessThan(500);
    expect(attempts[1] - attempts[0]).toBeGreaterThanOrEqual(1000 - SLACK);
    expect(attempts[2] - attempts[1]).toBeGreaterThanOrEqual(5000 - SLACK);
    expect(errors[0]).toBeGreaterThanOrEqual(attempts[2]);

    // The GPU is back and the reader comes back to the page.
    await failStarts(page, false);
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    await expect.poll(() => fresh(figure)).toBe(true);
    await expect.poll(() => recovered()).toBe(1);
    await expectMoment(page, figure);
  });

  test('once it starts again some other way, the tab coming back leaves it be', async ({ page }) => {
    const figure = await open(page, 'bounds');
    await drawn(page);
    await setMoment(figure);
    await failStarts(page, true);
    await trap(figure);
    await expect.poll(async () => (await events(page)).errors.length, { timeout: 60_000 }).toBe(1);
    await failStarts(page, false);

    // Scrolled away and back: the figure starts as it comes near.
    await figure.evaluate((f) => (f.style.display = 'none'));
    await page.waitForTimeout(100);
    await figure.evaluate((f) => (f.style.display = ''));
    await expect.poll(() => fresh(figure)).toBe(true);
    await mark(figure);
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    await page.waitForTimeout(500);
    expect(await figure.evaluate((f) => f.player?.__before)).toBe(true);

    // Taken out and put back: a new start, and that one is news.
    const loads = (await events(page)).load;
    await page.evaluate(async () => {
      const f = document.querySelector('#figure');
      const parent = f.parentNode;
      f.remove();
      await new Promise((r) => setTimeout(r, 50));
      parent.prepend(f);
    });
    await expect.poll(() => fresh(figure)).toBe(true);
    await expect.poll(async () => (await events(page)).load).toBeGreaterThan(loads);
    await mark(figure);
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    await page.waitForTimeout(500);
    expect(await figure.evaluate((f) => f.player?.__before)).toBe(true);
  });

  test('a figure that was playing plays on after its GPU is lost', async ({ page }) => {
    const figure = await open(page, 'bounds');
    await drawn(page);
    await figure.evaluate((f) => f.play());
    await mark(figure);
    // Its own GPU: crashing a browser's GPU process again and again can leave it with none.
    await loseOne(figure);
    await figure.evaluate((f) => (f.style.width = '520px'));
    await expect.poll(() => fresh(figure)).toBe(true);
    expect(await figure.evaluate((f) => f.paused)).toBe(false);
    const t = await figure.evaluate((f) => f.time);
    await expect.poll(() => figure.evaluate((f) => f.time)).toBeGreaterThan(t);
    expect((await events(page)).errors).toEqual([]);
  });

  for (const how of ['lost', 'trapped']) {
    test(`a WebGL context ${how} again and again is let go of each time`, async ({ page }) => {
      test.setTimeout(600_000);
      const warnings = [];
      page.on('console', (m) => {
        if (/too many|oldest/i.test(m.text())) warnings.push(m.text());
      });
      await open(page, 'two', 'gl');
      await drawn(page);
      const figure = figureOf(page);
      const other = page.locator('#other');
      await expect.poll(() => other.evaluate((f) => !!f.backend)).toBe(true);
      await mark(other);
      const draws = await countDraws(page);
      // More than a browser keeps live on a page, which is 16 in Chromium.
      for (let i = 0; i < 20; i++) {
        if (how === 'lost') {
          await mark(figure);
          await loseOne(figure);
        } else await trap(figure);
        await expect.poll(() => fresh(figure), { timeout: 30_000 }).toBe(true);
        const now = await draws('figure');
        await figure.evaluate((f) => (f.style.width = f.style.width === '520px' ? '540px' : '520px'));
        await expect.poll(() => draws('figure'), { timeout: 30_000 }).toBeGreaterThan(now);
        // Back on the page, having drawn, so its tries start again.
        await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
        if (process.env.DEBUG_LOST) console.log(how, i, JSON.stringify(warnings), await other.evaluate((f) => !!f.player?.__before));
      }
      // The other figure kept its context, and its player.
      expect(await other.evaluate((f) => f.player?.__before)).toBe(true);
      expect(warnings).toEqual([]);
      expect((await events(page)).errors).toEqual([]);
    });
  }

  test('with no GPU, a handle takes no pointer, so a touch there scrolls', async ({ page }) => {
    const figure = await open(page, 'bounds');
    await drawn(page);
    const style = () => figure.evaluate((f) => {
      const h = f.shadowRoot.querySelector('.handle');
      const s = getComputedStyle(h);
      return [s.pointerEvents, s.touchAction];
    });
    expect(await style()).toEqual(['auto', 'none']);
    await failStarts(page, true);
    await trap(figure);
    await expect.poll(() => figure.evaluate((f) => f.player === null)).toBe(true);
    expect(await style()).toEqual(['none', 'auto']);
    await failStarts(page, false);
  });
});

test('a handle reaches no further than its panel', async ({ page }) => {
  const figure = await open(page, 'panels');
  await drawn(page);
  const handle = figure.locator('.handle');
  await expect(handle).toBeVisible();
  const at = await figure.evaluate((f) => {
    const h = f.shadowRoot.querySelector('.handle').getBoundingClientRect();
    const c = f.shadowRoot.querySelector('canvas').getBoundingClientRect();
    const [x, y] = [h.x + h.width / 2, h.y + h.height / 2];
    // The panels sit one over the other, and the edge between them is halfway down.
    const edge = c.y + c.height / 2;
    const hit = (py) => f.shadowRoot.elementFromPoint(x, py)?.classList.contains('handle') ?? false;
    return { gap: edge - y, inside: hit(y - 18), above: hit(edge - 2), below: hit(edge + 3) };
  });
  // The handle is near enough the edge that its reach would cross it.
  expect(at.gap).toBeGreaterThan(0);
  expect(at.gap).toBeLessThan(21);
  expect(at.inside).toBe(true);
  expect(at.above).toBe(true);
  expect(at.below).toBe(false);
});
