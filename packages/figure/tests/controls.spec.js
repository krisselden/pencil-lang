// The element's controls: the defaults `controls` asks for, a page's own in the `controls` slot,
// readouts, the keyboard, what a screen reader is told, and that nothing moves.
import { test, expect } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { open, drawn, events, onFigure, value, figureOf } from './helpers.js';
import { declarative, shape } from '../src/controls.js';

const here = dirname(fileURLToPath(import.meta.url));
const manifest = resolve(here, '.built/controls/controls.json');

/** The defaults, in their shadow root. Playwright's locators go into it. */
const controls = (page) => page.locator('#figure pencil-controls');

test.describe('the default controls', () => {
  // Wide enough that every row is in view: on a phone the rest are in a menu, tested below.
  test.use({ viewport: { width: 1280, height: 900 } });

  test.beforeEach(async ({ page }) => {
    await open(page, 'controls');
    await drawn(page);
  });

  test('say each param by its caption, or its name, with the control its sheets and options give it', async ({ page }) => {
    const box = controls(page);
    await expect(box).toHaveAttribute('slot', 'controls');
    // A caption names its slider, and the value is its text.
    const arms = box.getByRole('slider', { name: 'arms' });
    await expect(arms).toHaveAttribute('aria-valuetext', '5');
    await expect(arms).toBeEnabled();
    // A choice of three is radios, named for its caption.
    const direction = box.getByRole('group', { name: 'direction' });
    await expect(direction.getByRole('radio')).toHaveCount(3);
    await expect(direction.getByRole('radio', { name: '1', exact: true })).toBeChecked();
    // A choice of off and on is a checkbox, named for the param, and of six a select.
    await expect(box.getByRole('checkbox', { name: 'glow' })).not.toBeChecked();
    await expect(box.getByRole('combobox', { name: 'many' })).toHaveValue('0');
    // The param a handle sets has no control: the handle is its control.
    await expect(box.locator('[data-param="s"]')).toHaveCount(0);
    // Time, since the figure follows it, and a play button.
    await expect(box.getByRole('slider', { name: 'Time' })).toHaveAttribute('aria-valuetext', '0.0 s');
    await expect(box.getByRole('button', { name: 'Play' })).toBeVisible();
  });

  test('set the figure, and follow it', async ({ page }) => {
    const box = controls(page);
    await box.getByRole('slider', { name: 'arms' }).fill('9');
    await expect.poll(() => value(page, 'q')).toBe(9);
    await expect(box.locator('output[data-param="q"]')).toHaveText('9');
    await box.getByRole('radio', { name: 'i', exact: true }).check();
    await expect.poll(() => value(page, 'along.index')).toBe(1);
    await box.getByRole('checkbox', { name: 'glow' }).check();
    await expect.poll(() => value(page, 'glow.index')).toBe(1);
    await box.getByRole('combobox', { name: 'many' }).selectOption({ value: '4' });
    await expect.poll(() => value(page, 'many.index')).toBe(4);
    // And the other way: what the page holds shows in the controls.
    await onFigure(page, (f) => f.hold('q', 4));
    await expect(box.getByRole('slider', { name: 'arms' })).toHaveAttribute('aria-valuetext', '4');
    await onFigure(page, (f) => f.hold('along.index', 2));
    await expect(box.getByRole('radio', { name: '-1' })).toBeChecked();
  });

  test('say the readouts, and keep them up to date as the figure changes', async ({ page }) => {
    const box = controls(page);
    const ratio = box.locator('[data-readout="ratio"]');
    await expect(ratio).toContainText('ratio 1.67');
    await expect(box.locator('[data-readout="turn"]')).toContainText('turn = 0');
    await expect(box.locator('[data-readout="z"]')).toContainText('z^2 = 1');
    await onFigure(page, (f) => f.hold('q', 12));
    await expect(ratio).toContainText('ratio 4');
    // Time moves one.
    await onFigure(page, (f) => (f.time = 2));
    await expect(box.locator('[data-readout="turn"]')).toContainText('turn = 0.50');
    // And the element says what each says.
    const said = await onFigure(page, (f) => f.readouts.map((r) => r.words));
    expect(said).toEqual(['ratio 4', 'turn = 0.50', 'z^2 = 1']);
  });

  test('are worked with the keyboard, with a ring where the focus is', async ({ page, browserName }) => {
    const box = controls(page);
    // Safari tabs to every control with Option and Tab, and to text fields alone with Tab.
    const tab = browserName === 'webkit' ? 'Alt+Tab' : 'Tab';
    await box.getByRole('button', { name: 'Play' }).focus();
    await page.keyboard.press('Space');
    await expect.poll(() => onFigure(page, (f) => f.paused)).toBe(false);
    await expect(box.getByRole('button', { name: 'Pause' })).toBeFocused();
    await page.keyboard.press('Enter');
    await expect.poll(() => onFigure(page, (f) => f.paused)).toBe(true);
    // Tab from the play button goes on through the controls, each with its ring.
    await page.keyboard.press(tab);
    await expect(box.getByRole('slider', { name: 'Time' })).toBeFocused();
    await page.keyboard.press(tab);
    const arms = box.getByRole('slider', { name: 'arms' });
    await expect(arms).toBeFocused();
    const ring = await arms.evaluate((el) => getComputedStyle(el).outlineStyle);
    expect(ring).not.toBe('none');
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => value(page, 'q')).toBe(6);
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.press('ArrowLeft');
    await expect.poll(() => value(page, 'q')).toBe(4);
    // An arrow in the controls is theirs, and doesn't reach the figure.
    expect((await events(page)).errors).toEqual([]);
    // Within a group of radios the arrows pick the next.
    await page.keyboard.press(tab);
    await expect(box.getByRole('radio', { name: '1', exact: true })).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => value(page, 'along.index')).toBe(1);
    await page.keyboard.press(tab);
    await expect(box.getByRole('checkbox', { name: 'glow' })).toBeFocused();
    await page.keyboard.press('Space');
    await expect.poll(() => value(page, 'glow.index')).toBe(1);
  });

  test('come and go with the attribute', async ({ page }) => {
    await onFigure(page, (f) => f.removeAttribute('controls'));
    await expect(controls(page)).toHaveCount(0);
    await onFigure(page, (f) => f.setAttribute('controls', ''));
    await expect(controls(page).getByRole('slider', { name: 'arms' })).toBeVisible();
  });
});

test.describe('a page\'s own controls', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'own');
    await drawn(page);
  });

  test('are set from the figure and followed each frame, and set it', async ({ page }) => {
    const q = page.locator('#q');
    // Its range and step are the figure's.
    await expect(q).toHaveAttribute('min', '3');
    await expect(q).toHaveAttribute('max', '12');
    await expect(q).toHaveAttribute('step', '1');
    await expect(q).toHaveValue('5');
    await expect(page.locator('#q-out')).toHaveText('5');
    // A select with no options gets the choice's.
    await expect(page.locator('#many option')).toHaveCount(6);
    await q.fill('7');
    await expect.poll(() => value(page, 'q')).toBe(7);
    await expect(page.locator('#q-out')).toHaveText('7');
    await expect(page.locator('#ratio')).toContainText('ratio 2.33');
    // A radio by the choice's name or its param's, by its place from 0.
    await page.getByRole('radio', { name: 'minus one' }).check();
    await expect.poll(() => value(page, 'along.index')).toBe(2);
    await onFigure(page, (f) => f.hold('along.index', 0));
    await expect(page.getByRole('radio', { name: 'one', exact: true })).toBeChecked();
    await page.getByRole('checkbox', { name: 'Glow' }).check();
    await expect.poll(() => value(page, 'glow.index')).toBe(1);
    await page.locator('#play').click();
    await expect.poll(() => onFigure(page, (f) => f.paused)).toBe(false);
    await expect(page.locator('#play')).toHaveText('Pause');
    // The element writes nothing of its own beside them.
    await expect(page.locator('#figure pencil-controls')).toHaveCount(0);
  });
});

test.describe('on a phone', () => {
  test.use({ viewport: { width: 375, height: 812 } });

  test('they keep to one row, and the rest open from a menu', async ({ page }) => {
    await open(page, 'controls');
    await drawn(page);
    const box = controls(page);
    const bar = box.locator('.bar');
    const more = box.getByRole('button', { name: 'More controls' });
    await expect(more).toBeVisible();
    await expect(box.getByRole('slider', { name: 'arms' })).toBeHidden();
    // One row: as tall as its tallest control.
    const height = (await bar.boundingBox()).height;
    const play = (await box.getByRole('button', { name: 'Play' }).boundingBox()).height;
    expect(height).toBeLessThanOrEqual(play + 1);
    await more.click();
    const arms = box.getByRole('slider', { name: 'arms' });
    await expect(arms).toBeVisible();
    await arms.fill('10');
    await expect.poll(() => value(page, 'q')).toBe(10);
    await page.keyboard.press('Escape');
    await expect(arms).toBeHidden();
  });
});

/** A page as a build writes one: the poster, and the controls in a declarative shadow root, with
 * what the build says of the figure, before any script runs. It measures every box as it's parsed. */
function written() {
  const built = JSON.parse(readFileSync(manifest, 'utf8'));
  const [image] = built.poster.images;
  const moment = { ...built.poster, readouts: built.poster.readouts };
  const markup = declarative({ info: built.info, moment, paused: true, disabled: true });
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>written</title>
<style>
  :root { color-scheme: light dark; }
  body { margin: 0; padding: 8px; }
  main { max-width: 765px; margin: 0 auto; }
  pencil-figure { display: block; }
</style>
</head>
<body>
<main>
  <pencil-figure id="figure" controls compiled="../.built/controls/controls.pfig" paused aria-label="Rings, an arm and a point.">
    <img src="../.built/controls/${image.src}" width="${image.width}" height="${image.height}" alt="Rings, an arm and a point." style="display: block; width: 100%; height: auto">
    <pencil-controls slot="controls" data-shape="${shape(built.info)}">${markup}</pencil-controls>
  </pencil-figure>
</main>
<script>
  window.__measure = () => {
    const figure = document.getElementById('figure');
    const host = figure.querySelector('pencil-controls');
    const parts = host.shadowRoot.querySelectorAll('.bar, .row, input, output, button, select, label, fieldset, .readout');
    // Each by its place, and what it is: a box the element wrote again is a box that moved.
    return [figure, host, ...parts]
      .map((el, i) => {
        const r = el.getBoundingClientRect();
        const name = i + ' ' + el.localName + (el.className ? '.' + el.className : '') + (el.id ? '#' + el.id : '') +
          (el.dataset.param ? '[' + el.dataset.param + ']' : '') + (el.dataset.readout ? '{' + el.dataset.readout + '}' : '') +
          (el.type === 'radio' ? '=' + el.value : '');
        return [name, r.x, r.y, r.width, r.height];
      })
      .filter(([, , , w, h]) => w > 0 || h > 0);
  };
  window.__parsed = window.__measure();
</script>
<script type="module" src="./boot.js"></script>
</body>
</html>`;
}

/** Every box that moved or changed size from `before`, by more than a quarter of a pixel. */
const moved = (before, after) => {
  const now = new Map(after.map(([name, ...box]) => [name, box]));
  return before.filter(([name, ...box]) => {
    const was = now.get(name);
    return !was || was.some((v, i) => Math.abs(v - box[i]) > 0.25);
  }).map(([name, ...box]) => `${name} ${box.map((v) => v.toFixed(1))} → ${now.get(name)?.map((v) => v.toFixed(1)) ?? 'gone'}`);
};

for (const [width, height] of [[1633, 919], [375, 812]]) {
  test.describe(`nothing moves, at ${width}×${height}`, () => {
    test.use({ viewport: { width, height } });

    test.beforeEach(async ({ page }) => {
      test.skip(!existsSync(manifest), 'target/release/pencil is missing (or its build failed), so tests/.built/controls was not made');
      await page.route('**/tests/pages/written.html*', (route) => route.fulfill({ contentType: 'text/html', body: written() }));
      const query = [process.env.PENCIL_GL && 'gl', process.env.PENCIL_MIN && 'min'].filter(Boolean).join('&');
      await page.goto(`/packages/figure/tests/pages/written.html${query ? `?${query}` : ''}`);
    });

    test('when the figure loads, a value changes during a drag, or time plays', async ({ page }) => {
      const parsed = await page.evaluate(() => window.__parsed);
      expect(parsed.length).toBeGreaterThan(4);
      await drawn(page);
      // The build's controls are the figure's, so they're kept and turned on.
      const box = controls(page);
      await expect(box.getByRole('slider', { name: 'Time' })).toBeEnabled();
      expect(moved(parsed, await page.evaluate(() => window.__measure()))).toEqual([]);

      // A drag across the first slider in view, a value at a time.
      const slider = box.locator('input[type=range]:visible').first();
      const track = await slider.boundingBox();
      await page.mouse.move(track.x + 4, track.y + track.height / 2);
      await page.mouse.down();
      for (let k = 1; k <= 8; k++) {
        await page.mouse.move(track.x + (track.width * k) / 8, track.y + track.height / 2);
        expect(moved(parsed, await page.evaluate(() => window.__measure()))).toEqual([]);
      }
      await page.mouse.up();
      if (width > 600) {
        // The value changed under the drag, and the readouts it changes too.
        await box.getByRole('slider', { name: 'arms' }).fill('12');
        await expect(box.locator('[data-readout="ratio"]')).toContainText('ratio 4');
        expect(moved(parsed, await page.evaluate(() => window.__measure()))).toEqual([]);
      }

      // Time plays, frame after frame, and what a readout says changes with it.
      const turn = box.locator('[data-readout="turn"]');
      const before = await turn.textContent();
      await box.getByRole('button', { name: 'Play' }).click();
      await expect.poll(() => onFigure(page, (f) => f.paused)).toBe(false);
      const during = await page.evaluate(
        () =>
          new Promise((done) => {
            const seen = [];
            let frames = 0;
            const look = () => {
              seen.push(window.__measure());
              if (++frames < 30) requestAnimationFrame(look);
              else done(seen);
            };
            requestAnimationFrame(look);
          }),
      );
      for (const frame of during) expect(moved(parsed, frame)).toEqual([]);
      await expect.poll(() => turn.textContent()).not.toBe(before);
    });
  });
}

test('a figure with no controls asked for and none in its slot gets none', async ({ page }) => {
  await open(page, 'bounds');
  await drawn(page);
  await expect(figureOf(page).locator('pencil-controls')).toHaveCount(0);
  expect(await onFigure(page, (f) => f.readouts)).toEqual([]);
});
