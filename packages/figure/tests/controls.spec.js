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

/** The menu's button on a phone, by either of its names: "More controls" beside the bar's row, and
 * "Controls" in a bar with none. */
const menuButton = (box) => box.getByRole('button', { name: /^(More controls|Controls)$/ });

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
    // Its words are mathematics as a screen reader says it.
    await expect(box.locator('[data-readout="z"]')).toContainText('z squared = 1');
    await onFigure(page, (f) => f.hold('q', 12));
    await expect(ratio).toContainText('ratio 4');
    // Time moves one.
    await onFigure(page, (f) => (f.time = 2));
    await expect(box.locator('[data-readout="turn"]')).toContainText('turn = 0.50');
    // And the element says what each says.
    const said = await onFigure(page, (f) => f.readouts.map((r) => r.words));
    expect(said).toEqual(['ratio 4', 'turn = 0.50', 'z squared = 1']);
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
    await expect(controls(page)).toBeHidden();
    await expect(controls(page).getByRole('slider', { name: 'arms' })).toHaveCount(0);
    await onFigure(page, (f) => f.setAttribute('controls', ''));
    await expect(controls(page).getByRole('slider', { name: 'arms' })).toBeVisible();
  });

  test('keep the focus where it was when they\'re written again', async ({ page }) => {
    const box = controls(page);
    // Each kind of control, found again by what it's for: none of these has an id.
    for (const target of [box.getByRole('button', { name: 'Play' }), box.getByRole('radio', { name: 'i', exact: true }), box.getByRole('slider', { name: 'arms' })]) {
      await target.focus();
      // Controls made for another kind of figure are written again when it loads, as after a
      // reader picks a look.
      const loads = (await events(page)).load;
      await controls(page).evaluate((host) => (host.dataset.shape = 'another'));
      await onFigure(page, (f) => f.reload());
      await expect.poll(async () => (await events(page)).load).toBeGreaterThan(loads);
      await expect.poll(() => controls(page).evaluate((host) => host.dataset.shape)).not.toBe('another');
      await expect(target).toBeFocused();
    }
  });
});

test.describe('the beats\' controls', () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test('take their own arrows, so a slider\'s arrow doesn\'t step the figure', async ({ page }) => {
    await open(page, 'controlled', 'figure=beats');
    await drawn(page);
    const box = controls(page);
    await box.getByRole('button', { name: 'Next beat' }).click();
    await expect.poll(() => value(page, 'beat')).toBe(1);
    const arms = box.getByRole('slider', { name: 'arms' });
    await arms.focus();
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => value(page, 'q')).toBe(4);
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.press('ArrowLeft');
    await expect.poll(() => value(page, 'q')).toBe(2);
    // The figure's own arrows step it, and these didn't.
    expect(await value(page, 'beat')).toBe(1);
    await figureOf(page).focus();
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => value(page, 'beat')).toBe(2);
    await expect(box.locator('.captions:visible .caption[data-on]')).toHaveText('And the point they share.');
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
    // One row, of one line's height.
    const height = (await bar.boundingBox()).height;
    expect(height).toBeLessThanOrEqual(40);
    const middles = await bar.evaluate((el) => [...el.querySelectorAll(':scope > .row > *, :scope > button')].map((c) => c.getBoundingClientRect()).filter((r) => r.width > 0).map((r) => r.top + r.height / 2));
    expect(Math.max(...middles) - Math.min(...middles)).toBeLessThan(2);
    await more.click();
    const arms = box.getByRole('slider', { name: 'arms' });
    await expect(arms).toBeVisible();
    await arms.fill('10');
    await expect.poll(() => value(page, 'q')).toBe(10);
    // Escape closes it, and the focus goes back to the button that opened it.
    await arms.focus();
    await page.keyboard.press('Escape');
    await expect(arms).toBeHidden();
    await expect(more).toBeFocused();
  });

  test('a beat\'s caption is a line of its own with the bar, as tall as the longest, and a new beat is said', async ({ page }) => {
    // Steps with no clock, whose stepper is the bar's, and steps that follow time, whose stepper is
    // in the menu and whose bar is the time's.
    for (const figure of ['beats', 'timed']) {
      await open(page, 'controlled', `figure=${figure}`);
      await drawn(page);
      const box = controls(page);
      const line = box.locator('.row.phone');
      const shown = line.locator('.caption[data-on]');
      await expect(shown).toBeVisible();
      await expect(shown).toHaveText('One circle.');
      expect(await fits(page), figure).toEqual([]);
      const before = await boxes(page);
      await onFigure(page, (f) => f.step(1));
      await expect(shown).toHaveText('Then as many as the arms say, each a little smaller than the last, around the first.');
      // The live region says where it is and what the caption says.
      await expect(box.locator('[data-said]')).toHaveText('Step 2 of 3: Then as many as the arms say, each a little smaller than the last, around the first.');
      await onFigure(page, (f) => f.step(1));
      await expect(shown).toHaveText('And the point they share.');
      await expect(box.locator('[data-said]')).toHaveText('Step 3 of 3: And the point they share.');
      // Nothing moved: the line is as tall as the longest caption from the start.
      expect(await boxes(page)).toEqual(before);
      expect(await fits(page), figure).toEqual([]);
      // With the menu open it's still there.
      await box.getByRole('button', { name: 'More controls' }).click();
      await expect(shown).toBeVisible();
      await page.keyboard.press('Escape');
    }
  });

  test('the bar keeps the playback, or else the readouts, or else a figure\'s one param, and never splits its params', async ({ page }) => {
    // Each fixture, the row its bar keeps, and the params its menu has, all of them together.
    const cases = [
      ['controls', 'time', ['q', 'along', 'glow', 'many']],
      ['timed', 'time', ['q']],
      ['beats', 'beats', ['q']],
      ['period', 'readouts', ['q', 'p']],
      ['room', 'readouts', ['t', 'theta']],
      ['lone', 'param', []],
      ['grid', null, ['many', 'q', 'r', 's']],
      ['bar', null, ['mode', 'q']],
    ];
    for (const [figure, kept, menu] of cases) {
      await open(page, 'controlled', `figure=${figure}`);
      await drawn(page);
      const box = controls(page);
      const said = await box.evaluate((host) => {
        const root = host.shadowRoot;
        return {
          bar: [...root.querySelectorAll('.bar > .row')].map((row) => row.className),
          barParams: [...new Set([...root.querySelectorAll('.bar [data-param]')].map((el) => el.dataset.param))],
          menu: [...new Set([...root.querySelectorAll('.rest [data-param]')].map((el) => el.dataset.param))],
        };
      });
      expect(said.bar, figure).toEqual(kept ? [expect.stringContaining(kept)] : []);
      expect(said.menu, figure).toEqual(menu);
      // A param is in the bar only when it's the figure's one.
      expect(said.barParams, figure).toEqual(kept === 'param' ? ['q'] : []);
      const more = menuButton(box);
      const words = more.locator('.more-words');
      if (kept === 'param') {
        // Nothing else to show, so no menu.
        await expect(box.locator('.more')).toBeHidden();
      } else if (kept) {
        await expect(box.getByRole('button', { name: 'More controls', exact: true })).toBeVisible();
        await expect(more).toHaveAttribute('title', 'More controls');
        await expect(words).toBeHidden();
      } else {
        // A bar with no row of its own is the menu's button, which says what it opens, and is
        // called that.
        await expect(box.getByRole('button', { name: 'Controls', exact: true })).toBeVisible();
        await expect(more).toHaveAttribute('title', 'Controls');
        await expect(words).toBeVisible();
        await expect(more).toHaveText('Controls ⋯', { useInnerText: true });
        for (const name of menu) await expect(box.locator(`[data-param="${name}"]`).first()).toBeHidden();
        await more.click();
        for (const name of menu) await expect(box.locator(`[data-param="${name}"]`).first()).toBeVisible();
        await box.locator(`[data-param="${menu[0]}"]`).first().focus();
        await page.keyboard.press('Escape');
        await expect(more).toBeFocused();
      }
      expect(await fits(page), figure).toEqual([]);
    }
  });

  test('the menu drops down under its bar, as wide as the controls, and scrolls with the page', async ({ page }) => {
    // Under the bar, or under the beats' caption below it, which shows while the menu is open.
    for (const figure of ['period', 'timed']) {
      await open(page, 'menus', `figure=${figure}`);
      await drawn(page);
      const box = controls(page);
      await box.getByRole('button', { name: 'More controls' }).click();
      await expect(box.locator('.rest')).toBeVisible();
      const before = await placed(page);
      expect(before.open, figure).toBe(true);
      expect(Math.abs(before.menu.top - before.above.bottom), `${figure} ${JSON.stringify(before)}`).toBeLessThanOrEqual(2);
      expect(Math.abs(before.menu.left - before.controls.left), figure).toBeLessThanOrEqual(1);
      expect(Math.abs(before.menu.width - before.controls.width), figure).toBeLessThanOrEqual(1);
      if (figure === 'timed') expect(before.above.bottom, figure).toBeGreaterThan(before.bar.bottom + 10);
      // The page scrolls, and the menu goes with it, under its bar, with nothing to catch up.
      await page.evaluate(() => window.scrollBy({ top: 200, behavior: 'instant' }));
      const after = await placed(page);
      expect(after.scrollY - before.scrollY).toBe(200);
      expect(after.open).toBe(true);
      expect(after.bar.top).toBeCloseTo(before.bar.top - 200, 0);
      expect(Math.abs(after.menu.top - after.above.bottom), `${figure} ${JSON.stringify(after)}`).toBeLessThanOrEqual(2);
      expect(Math.abs(after.menu.left - after.controls.left)).toBeLessThanOrEqual(1);
      // Escape still closes it and gives the focus back.
      await box.locator('.rest input').first().focus();
      await page.keyboard.press('Escape');
      await expect(box.locator('.rest')).toBeHidden();
      await expect(box.getByRole('button', { name: 'More controls' })).toBeFocused();
    }
  });

  test('a menu opened low on the screen scrolls the page just enough to show it, and keeps its bar on screen', async ({ page }) => {
    await open(page, 'menus', 'figure=controls');
    await drawn(page);
    // The second figure, far enough down the page to be scrolled to with its bar just above the
    // bottom of the screen.
    await page.evaluate(() => document.getElementById('other').scrollIntoView({ block: 'center', behavior: 'instant' }));
    const box = page.locator('#other pencil-controls');
    await expect(box.getByRole('button', { name: 'More controls' })).toBeVisible({ timeout: 30_000 });
    await page.evaluate(() => {
      const bar = document.querySelector('#other pencil-controls').shadowRoot.querySelector('.bar').getBoundingClientRect();
      window.scrollBy({ top: bar.bottom + 24 - innerHeight, behavior: 'instant' });
    });
    const start = await placed(page, '#other');
    expect(start.bar.bottom).toBeGreaterThan(start.height - 40);
    await box.getByRole('button', { name: 'More controls' }).click();
    // It may glide there: wait until the menu is on the screen, and the page is still.
    await expect.poll(async () => {
      const now = await placed(page, '#other');
      return now.menu.bottom <= now.height;
    }).toBe(true);
    let last = -1;
    await expect.poll(async () => {
      const now = await page.evaluate(() => scrollY);
      const still = now === last;
      last = now;
      return still;
    }, { intervals: [250] }).toBe(true);
    const shown = await placed(page, '#other');
    expect(shown.open).toBe(true);
    expect(shown.scrollY).toBeGreaterThan(start.scrollY);
    expect(shown.menu.bottom, JSON.stringify(shown)).toBeLessThanOrEqual(shown.height);
    // Just enough: the menu's bottom is near the screen's.
    expect(shown.menu.bottom, JSON.stringify(shown)).toBeGreaterThan(shown.height - 20);
    expect(shown.bar.top).toBeGreaterThanOrEqual(0);
  });

  test('opening one figure\'s menu closes another\'s', async ({ page }) => {
    await open(page, 'menus', 'figure=period');
    await drawn(page);
    const first = controls(page);
    await first.getByRole('button', { name: 'More controls' }).click();
    await expect(first.locator('.rest')).toBeVisible();
    // The second is a little further down: scrolled up the screen, with the first's bar still on it.
    const second = page.locator('#other pencil-controls');
    const more = second.getByRole('button', { name: 'More controls' });
    await expect(more).toBeVisible({ timeout: 30_000 });
    await page.evaluate(() => {
      const bar = document.querySelector('#other pencil-controls').shadowRoot.querySelector('.bar').getBoundingClientRect();
      window.scrollBy({ top: bar.bottom + 16 - innerHeight, behavior: 'instant' });
    });
    const before = await placed(page);
    expect(before.bar.top, JSON.stringify(before)).toBeGreaterThan(0);
    expect(before.open).toBe(true);
    await more.click();
    await expect(second.locator('.rest')).toBeVisible();
    await expect(first.locator('.rest')).toBeHidden();
    const where = await placed(page, '#other');
    expect(Math.abs(where.menu.top - where.above.bottom), JSON.stringify(where)).toBeLessThanOrEqual(2);
  });

  test('the menu follows its bar wherever the page moves it, and closes when its bar leaves the screen', async ({ page }) => {
    await open(page, 'menus', 'figure=period');
    await drawn(page);
    const box = controls(page);
    await box.getByRole('button', { name: 'More controls' }).click();
    await expect(box.locator('.rest')).toBeVisible();
    const under = async () => {
      const now = await placed(page);
      return now.open && Math.abs(now.menu.top - now.above.bottom) <= 1 && Math.abs(now.menu.left - now.controls.left) <= 1;
    };
    // Something above the figure grows and moves it down, though nothing it holds changes size.
    const top = (await placed(page)).bar.top;
    await page.evaluate(() => (document.querySelector('main > p').style.paddingTop = '120px'));
    await expect.poll(async () => (await placed(page)).bar.top).toBeGreaterThan(top + 100);
    await expect.poll(under).toBe(true);
    // A figure that stays put as the page scrolls, as a sticky one does: the menu stays with it.
    await page.evaluate(() => Object.assign(document.getElementById('figure').style, { position: 'sticky', top: '0px' }));
    await page.evaluate(() => window.scrollBy({ top: 300, behavior: 'instant' }));
    await expect.poll(under).toBe(true);
    // Its bar scrolled off the screen, it closes.
    await page.evaluate(() => {
      document.getElementById('figure').style.position = '';
      window.scrollBy({ top: 1000, behavior: 'instant' });
    });
    await expect.poll(async () => (await placed(page)).open).toBe(false);
  });

  test('a row the bar gave up comes back once it fits, as when the phone turns, and after a look switch', async ({ page }) => {
    // Readouts too wide together for a phone held upright, which fit it turned. (Radios won't come
    // back in WebKit, which sets their legend on a line of its own, so they're too tall for the bar.)
    await open(page, 'controlled', 'figure=ro');
    await drawn(page);
    const box = controls(page);
    const inBar = () => box.evaluate((host) => host.shadowRoot.querySelectorAll('.bar > .row').length);
    // A bar without its row says what its button opens, and the button is called that.
    const words = box.locator('.more-words');
    await expect.poll(inBar).toBe(0);
    await expect(words).toBeVisible();
    await expect(box.getByRole('button', { name: 'Controls', exact: true })).toBeVisible();
    // Turned, a phone is wide enough for it, and it's back, cut off nowhere.
    await page.setViewportSize({ width: 812, height: 375 });
    await expect.poll(inBar).toBe(1);
    await expect(words).toBeHidden();
    await expect(box.locator('[data-readout="third"]')).toBeVisible();
    expect(await fits(page)).toEqual([]);
    await page.setViewportSize({ width: 375, height: 812 });
    await expect.poll(inBar).toBe(0);
    // Another look, another figure: its own controls, with a bar that fits, and then this one's again.
    await onFigure(page, (f) => f.setAttribute('src', 'controls.pen'));
    await expect(box.getByRole('slider', { name: 'Time' })).toBeVisible();
    await expect.poll(inBar).toBe(1);
    await expect(words).toBeHidden();
    await onFigure(page, (f) => f.setAttribute('src', 'ro.pen'));
    await expect.poll(inBar).toBe(0);
    await expect(words).toBeVisible();
  });

  test('an open menu stays open, with the focus where it was, when the controls are written again', async ({ page }) => {
    await open(page, 'controlled', 'figure=controls');
    await drawn(page);
    const box = controls(page);
    await box.getByRole('button', { name: 'More controls' }).click();
    const arms = box.getByRole('slider', { name: 'arms' });
    await arms.focus();
    const before = await placed(page);
    const loads = (await events(page)).load;
    await box.evaluate((host) => (host.dataset.shape = 'another'));
    await onFigure(page, (f) => f.reload());
    await expect.poll(async () => (await events(page)).load).toBeGreaterThan(loads);
    await expect.poll(() => box.evaluate((host) => host.dataset.shape)).not.toBe('another');
    await expect(arms).toBeVisible();
    await expect(arms).toBeFocused();
    // Where it was, under its bar, and the page hasn't scrolled to find it.
    const after = await placed(page);
    expect(after.open).toBe(true);
    expect(after.scrollY).toBe(before.scrollY);
    expect(after.menu).toEqual(before.menu);
  });

  test('an open menu written again where it runs off the screen stays open there, and the page doesn\'t scroll', async ({ page }) => {
    await open(page, 'menus', 'figure=controls');
    await drawn(page);
    const box = page.locator('#other pencil-controls');
    const more = box.getByRole('button', { name: 'More controls' });
    await expect(more).toBeVisible({ timeout: 30_000 });
    // Its bar low on the screen.
    const low = () =>
      page.evaluate(() => {
        const bar = document.querySelector('#other pencil-controls').shadowRoot.querySelector('.bar').getBoundingClientRect();
        window.scrollBy({ top: bar.bottom + 60 - innerHeight, behavior: 'instant' });
      });
    const settled = async () => {
      let last = -1;
      await expect.poll(async () => {
        const now = await page.evaluate(() => scrollY);
        const still = now === last;
        last = now;
        return still;
      }, { intervals: [250] }).toBe(true);
    };
    await low();
    // A reader opens it, and the page scrolls to show it; then scrolls back, so it runs off the bottom.
    await more.click();
    const arms = box.getByRole('slider', { name: 'arms' });
    await expect(arms).toBeVisible();
    await expect.poll(async () => {
      const now = await placed(page, '#other');
      return now.menu.bottom <= now.height;
    }).toBe(true);
    await settled();
    await low();
    await arms.focus();
    const before = await placed(page, '#other');
    expect(before.menu.bottom, JSON.stringify(before)).toBeGreaterThan(before.height);
    // Written again, as for another look.
    const loads = (await events(page)).load;
    await box.evaluate((host) => (host.dataset.shape = 'another'));
    await page.locator('#other').evaluate((f) => f.reload());
    await expect.poll(async () => (await events(page)).load).toBeGreaterThan(loads);
    await expect.poll(() => box.evaluate((host) => host.dataset.shape)).not.toBe('another');
    await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 400)))));
    const after = await placed(page, '#other');
    expect(after.scrollY).toBe(before.scrollY);
    expect(after.open).toBe(true);
    expect(after.menu).toEqual(before.menu);
    await expect(arms).toBeFocused();
  });

  test('a row too long or too tall for the bar goes behind the menu, and nothing in the bar is cut off', async ({ page }) => {
    // Radios with long words, readouts too wide together, and a fraction at a page's larger size.
    for (const query of ['figure=wide', 'figure=ro', 'figure=tall&style=--pencil-readout-size:1.5rem']) {
      await open(page, 'controlled', query);
      await drawn(page);
      const box = controls(page);
      expect(await box.evaluate((host) => host.shadowRoot.querySelectorAll('.bar > .row').length), query).toBe(0);
      const more = box.getByRole('button', { name: 'Controls', exact: true });
      await expect(more).toBeVisible();
      await expect(more).toHaveText('Controls ⋯', { useInnerText: true });
      expect(await fits(page), query).toEqual([]);
      await more.click();
      const shown =
        query === 'figure=wide' ? box.getByRole('radio', { name: 'gnomonic' })
        : query === 'figure=ro' ? box.locator('[data-readout="third"]')
        : box.locator('[data-readout="ratio"]');
      await expect(shown).toBeVisible();
      expect(await fits(page), query).toEqual([]);
    }
    // At its own size the fraction fits, and the bar keeps it.
    await open(page, 'controlled', 'figure=tall');
    await drawn(page);
    expect(await controls(page).evaluate((host) => host.shadowRoot.querySelectorAll('.bar > .row').length)).toBe(1);
    expect(await fits(page)).toEqual([]);
  });

  test('a long caption in the menu wraps, and leaves its slider room', async ({ page }) => {
    await open(page, 'controlled', 'figure=bar');
    await drawn(page);
    const box = controls(page);
    await box.getByRole('button', { name: 'Controls', exact: true }).click();
    const slider = await box.getByRole('slider', { name: 'the number of arms on the rolled tube' }).boundingBox();
    expect(slider.width).toBeGreaterThan(120);
    expect(await fits(page)).toEqual([]);
  });
});

test.describe('on a desktop', () => {
  test.use({ viewport: { width: 1633, height: 919 } });

  test('every row is in place, with no bar and no menu, and the readouts after the controls', async ({ page }) => {
    for (const figure of ['period', 'grid', 'lone', 'controls']) {
      await open(page, 'controlled', `figure=${figure}`);
      await drawn(page);
      const box = controls(page);
      await expect(box.locator('.more')).toBeHidden();
      const rows = await box.evaluate((host) =>
        [...host.shadowRoot.querySelectorAll('.row:not(.phone)')].map((row) => {
          const r = row.getBoundingClientRect();
          // A row the grid lays out by its cells has no box of its own.
          const cells = r.height > 0 ? [r] : [...row.children].map((cell) => cell.getBoundingClientRect());
          return { kind: row.className, shown: cells.some((c) => c.height > 0), top: Math.min(...cells.map((c) => c.top)) };
        }),
      );
      expect(rows.every((row) => row.shown), `${figure} ${JSON.stringify(rows)}`).toBe(true);
      // Top to bottom: time, beats, the params, then the readouts.
      const order = [...rows].sort((a, b) => a.top - b.top).map((row) => row.kind.split(' ')[1]);
      const rank = { time: 0, beats: 1, param: 2, choice: 2, readouts: 3 };
      expect(order.map((kind) => rank[kind]), `${figure} ${order}`).toEqual(order.map((kind) => rank[kind]).sort());
    }
  });
});

/** Where a figure's controls, their bar, what the menu drops from (the bar, or the beats' caption
 * line under it) and the menu are on the screen, how far the page is scrolled, and how tall the
 * screen is. */
const placed = (page, figure = '#figure') =>
  page.evaluate((id) => {
    const root = document.querySelector(`${id} pencil-controls`).shadowRoot;
    const box = (el) => {
      const { top, bottom, left, width } = el.getBoundingClientRect();
      return { top, bottom, left, width };
    };
    const menu = root.querySelector('.rest');
    return {
      scrollY,
      height: innerHeight,
      open: menu.matches(':popover-open'),
      controls: box(root.querySelector('.controls')),
      bar: box(root.querySelector('.bar')),
      above: box(root.querySelector('.row.phone') ?? root.querySelector('.bar')),
      menu: box(menu),
    };
  }, figure);

/** Where the figure, its controls' bar, the caption's line and the menu are, and how big each is. */
const boxes = (page) =>
  page.evaluate(() => {
    const figure = document.getElementById('figure');
    const root = figure.querySelector('pencil-controls').shadowRoot;
    return [figure, ...root.querySelectorAll('.bar, .row.phone, .rest')].map((el) => {
      const r = el.getBoundingClientRect();
      return [r.x, r.y, r.width, r.height].map((v) => Math.round(v * 4) / 4);
    });
  });

/**
 * What doesn't fit its box in the controls in view: an output or a readout whose text runs past
 * it, something in the bar that runs past the bar's row, a row taller than the bar, a menu's button
 * before the row, or a control's label that isn't on the same line as its control.
 */
const fits = (page) =>
  page.evaluate(() => {
    const root = document.querySelector('#figure pencil-controls').shadowRoot;
    const shown = (el) => {
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden';
    };
    const wrong = [];
    for (const out of root.querySelectorAll('output:not(.readout)')) {
      if (shown(out) && out.scrollWidth > out.clientWidth + 0.5) wrong.push(`output ${out.dataset.param ?? ''} "${out.textContent}" runs past its box`);
    }
    for (const box of root.querySelectorAll('.readout')) {
      if (!shown(box)) continue;
      const right = box.getBoundingClientRect().right;
      const ink = Math.max(...[...box.querySelectorAll('[aria-hidden] span')].map((s) => s.getBoundingClientRect().right));
      if (ink > right + 0.5) wrong.push(`readout ${box.dataset.readout} runs ${(ink - right).toFixed(1)} px past its box`);
    }
    const row = root.querySelector('.bar > .row');
    if (row && shown(row)) {
      if (row.scrollWidth > row.clientWidth + 0.5) wrong.push('the bar\'s row is cut off');
      // On a phone, where the bar has a box of its own.
      const bar = root.querySelector('.bar');
      const tall = row.getBoundingClientRect().height - bar.getBoundingClientRect().height;
      if (shown(bar) && tall > 0.5) wrong.push(`the bar's row is ${tall.toFixed(1)} px taller than the bar`);
      const edge = row.getBoundingClientRect().right;
      for (const item of row.children) {
        if (shown(item) && item.getBoundingClientRect().right > edge + 0.5) wrong.push(`${item.localName} in the bar is cut off`);
      }
      // The menu's button ends the bar, after its row.
      const more = root.querySelector('.bar > .more');
      if (more && shown(more) && more.getBoundingClientRect().left < edge - 0.5) wrong.push('the menu\'s button is before the bar\'s row');
    }
    // A caption's line holds every caption, the one showing included.
    for (const line of root.querySelectorAll('.row.phone')) {
      if (!shown(line)) continue;
      const edge = line.getBoundingClientRect();
      if (line.scrollWidth > line.clientWidth + 0.5) wrong.push('a caption runs past its line');
      for (const caption of line.querySelectorAll('.caption')) {
        const r = caption.getBoundingClientRect();
        if (r.bottom > edge.bottom + 0.5 || r.right > edge.right + 0.5) wrong.push(`"${caption.textContent}" runs out of its line`);
      }
    }
    for (const label of root.querySelectorAll('label[for]')) {
      const control = root.getElementById(label.htmlFor);
      if (!control || !shown(label) || !shown(control)) continue;
      const [a, b] = [label.getBoundingClientRect(), control.getBoundingClientRect()];
      if (Math.abs(a.top + a.height / 2 - (b.top + b.height / 2)) > 4) wrong.push(`"${label.textContent}" isn't on its control's line`);
    }
    return wrong;
  });

for (const [width, height] of [[1633, 919], [375, 812]]) {
  test.describe(`everything fits its box, at ${width}×${height}`, () => {
    test.use({ viewport: { width, height } });

    test('in every fixture, with values between the ends of their ranges', async ({ page }) => {
      for (const figure of ['controls', 'grid', 'room', 'bar', 'ro', 'beats', 'period', 'lone', 'wide', 'tall']) {
        await open(page, 'controlled', `figure=${figure}`);
        await drawn(page);
        const box = controls(page);
        const more = menuButton(box);
        const check = async (moment) => {
          if (moment) await onFigure(page, (f, m) => (f.params = m), moment);
          await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
          expect(await fits(page), `${figure} ${JSON.stringify(moment)}`).toEqual([]);
          if (await more.isVisible()) {
            await more.click();
            expect(await fits(page), `${figure}'s menu ${JSON.stringify(moment)}`).toEqual([]);
            await page.keyboard.press('Escape');
          }
        };
        await check(null);
        if (figure === 'room') {
          for (const [t, theta] of [[0.37, 2], [0.73, 3.3], [0.111, 4.5], [0.999, 1.7]]) await check({ t, theta });
        }
        if (figure === 'grid' && width > 600) {
          // Every row keeps to the grid's three columns: each slider starts where the others do.
          const starts = await box.evaluate((host) =>
            [...host.shadowRoot.querySelectorAll('input[type=range]')].filter((el) => el.getBoundingClientRect().width > 0).map((el) => Math.round(el.getBoundingClientRect().left)),
          );
          expect(new Set(starts).size, String(starts)).toBe(1);
        }
      }
    });
  });
}

test.describe('a new beat', () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test('is said once, by a key on the figure, a button, play, or a page setting it', async ({ page }) => {
    await open(page, 'controlled', 'figure=timed');
    await drawn(page);
    // Every word either live region says: the figure's own, for its keys, and the controls'.
    await page.evaluate(() => {
      const figure = document.getElementById('figure');
      const regions = [figure.shadowRoot.querySelector('.said'), figure.querySelector('pencil-controls').shadowRoot.querySelector('[data-said]')];
      window.__said = [];
      const seen = new MutationObserver(() => regions.forEach((region, i) => {
        const text = region.textContent;
        if (text && text !== region.dataset.heard) window.__said.push([i, text]);
        region.dataset.heard = text;
      }));
      for (const region of regions) seen.observe(region, { childList: true, characterData: true, subtree: true });
    });
    const said = () => page.evaluate(() => window.__said.splice(0));
    const beat = () => value(page, 'beat');
    const settle = () => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 100)))));
    const box = controls(page);

    // Steps share the clock evenly, two seconds each, and the first is beat 0.
    const second = 'Step 2 of 3: Then as many as the arms say, each a little smaller than the last, around the first.';
    // A key on the figure.
    await figureOf(page).focus();
    await page.keyboard.press('ArrowRight');
    await expect.poll(beat).toBe(1);
    await settle();
    expect(await said()).toEqual([[1, second]]);
    // A button.
    await box.getByRole('button', { name: 'Next beat' }).click();
    await expect.poll(beat).toBe(2);
    await settle();
    expect(await said()).toEqual([[1, 'Step 3 of 3: And the point they share.']]);
    // A page setting it.
    await onFigure(page, (f) => (f.beat = 0));
    await expect.poll(beat).toBe(0);
    await settle();
    expect(await said()).toEqual([[1, 'Step 1 of 3: One circle.']]);
    // Play, from just before a beat ends, until it has: the same beat, said once.
    await onFigure(page, (f) => {
      f.time = 1.9;
      f.play();
    });
    await expect.poll(beat).toBe(1);
    await onFigure(page, (f) => f.pause());
    await settle();
    expect(await said()).toEqual([[1, second]]);
  });
});

test('a readout\'s room holds what it says where the page starts the figure', async ({ page }) => {
  // Near a pole none of the places tried along `a` comes near: only the page's own moment does.
  await open(page, 'controlled', 'figure=pole&params=a:0.3');
  await drawn(page);
  await expect(controls(page).locator('[data-readout="n"]')).toContainText('= −10000.00');
  expect(await fits(page)).toEqual([]);
  const [room, width] = await onFigure(page, (f) => [JSON.parse(f.player.readout_room(f.time))[0][0], f.readouts[0].width]);
  expect(room).toBeGreaterThanOrEqual(width);
});

test.describe('the defaults, and a page\'s own content', () => {
  test('without `controls`, only what the page put in `<pencil-controls>` shows', async ({ page }) => {
    await page.route('**/tests/pages/extra.html*', (route) =>
      route.fulfill({
        contentType: 'text/html',
        body: `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>extra</title><style>pencil-figure { display: block; }</style></head><body>
<pencil-figure id="figure" src="controls.pen" sheets="controls.style" paused aria-label="x">
  <pencil-controls slot="controls"><label id="look">Look <select><option>one</option></select></label></pencil-controls>
</pencil-figure>
<script type="module" src="./boot.js"></script></body></html>`,
      }),
    );
    await page.goto(`/packages/figure/tests/pages/extra.html${process.env.PENCIL_GL ? '?gl' : ''}`);
    await drawn(page);
    await expect(page.locator('#look')).toBeVisible();
    await expect(controls(page).getByRole('slider')).toHaveCount(0);
    // Asked for, the defaults come, with the page's after them; and taken away, the page's stay.
    await onFigure(page, (f) => f.setAttribute('controls', ''));
    await expect(controls(page).getByRole('slider', { name: 'arms' })).toBeVisible();
    await expect(page.locator('#look')).toBeVisible();
    await onFigure(page, (f) => f.removeAttribute('controls'));
    await expect(controls(page).getByRole('slider')).toHaveCount(0);
    await expect(page.locator('#look')).toBeVisible();
  });

  test('a declarative shadow root a script put in the page, which isn\'t read, still gets the controls', async ({ page }) => {
    test.skip(!existsSync(manifest), 'target/release/pencil is missing (or its build failed), so tests/.built/controls was not made');
    const built = JSON.parse(readFileSync(manifest, 'utf8'));
    const markup = declarative({ info: built.info, moment: built.poster, paused: true, disabled: true });
    const fragment = `<pencil-figure id="figure" controls compiled="../.built/controls/controls.pfig" paused aria-label="x"><pencil-controls slot="controls" data-shape="${shape(built.info)}">${markup}</pencil-controls></pencil-figure>`;
    await page.route('**/tests/pages/inserted.html*', (route) =>
      route.fulfill({
        contentType: 'text/html',
        body: `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>inserted</title></head><body><div id="here"></div>
<script>document.getElementById('here').innerHTML = ${JSON.stringify(fragment)};</script>
<script type="module" src="./boot.js"></script></body></html>`,
      }),
    );
    await page.goto(`/packages/figure/tests/pages/inserted.html${process.env.PENCIL_GL ? '?gl' : ''}`);
    await drawn(page);
    await expect(controls(page).getByRole('slider', { name: 'arms' })).toBeVisible();
    // The template left behind isn't kept as the page's own content.
    expect(await controls(page).evaluate((host) => host.children.length)).toBe(0);
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
  // The menu's button, which mustn't open the menu before the element is there to place it.
  window.__menuOff = document.querySelector('#figure pencil-controls').shadowRoot.querySelector('.more').disabled;
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
      expect(await page.evaluate(() => window.__menuOff)).toBe(true);
      await drawn(page);
      // The build's controls are the figure's, so they're kept and turned on, the menu's button too.
      const box = controls(page);
      await expect(box.getByRole('slider', { name: 'Time' })).toBeEnabled();
      expect(await box.evaluate((host) => host.shadowRoot.querySelector('.more').disabled)).toBe(false);
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
      } else {
        // On a phone the params are in the menu: opened, nothing in it or under it moves as one is
        // dragged, and the readouts it changes don't either.
        await box.getByRole('button', { name: 'More controls' }).click();
        const arms = box.getByRole('slider', { name: 'arms' });
        await expect(arms).toBeVisible();
        const opened = await page.evaluate(() => window.__measure());
        const slid = await arms.boundingBox();
        await page.mouse.move(slid.x + 4, slid.y + slid.height / 2);
        await page.mouse.down();
        for (let k = 1; k <= 8; k++) {
          await page.mouse.move(slid.x + (slid.width * k) / 8, slid.y + slid.height / 2);
          expect(moved(opened, await page.evaluate(() => window.__measure()))).toEqual([]);
        }
        await page.mouse.up();
        await expect.poll(() => value(page, 'q')).toBe(12);
        await expect(box.locator('[data-readout="ratio"]')).toContainText('ratio 4');
        expect(moved(opened, await page.evaluate(() => window.__measure()))).toEqual([]);
        await page.keyboard.press('Escape');
        await expect(arms).toBeHidden();
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
