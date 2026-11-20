import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { pack } from '../src/pack.js';
import { handleOf } from './helpers.js';

// The playground, web/playground.html, served from the repo root: its two menus, a figure in a
// link, a figure of the reader's own from files, adding and removing files, and Save. Then its
// bars: one line of tabs that scrolls, and on a phone one small bar with the rest in two menus,
// over the figure and then the editor, all on screen.

/** The playground's address, with `?gl` and `?min` as the fixture pages add them. */
function addressOf(query = '', hash = '') {
  const params = new URLSearchParams(query);
  if (process.env.PENCIL_GL) params.set('gl', '');
  if (process.env.PENCIL_MIN) params.set('min', '');
  const text = params.toString().replace(/\b(gl|min)=(&|$)/g, '$1$2');
  return `/web/playground.html${text ? `?${text}` : ''}${hash}`;
}

/** Opens the playground and waits until it has opened a figure. */
async function openPlayground(page, query = '', hash = '') {
  await page.goto(addressOf(query, hash));
  await settled(page);
}

const settled = (page) => expect.poll(() => page.evaluate(() => !!window.pencil), { timeout: 30_000 }).toBe(true);

/** Waits until the figure has compiled what's open and drawn it. It only draws while it's on
 * screen. */
async function drawnHere(page) {
  await page.locator('#figure').scrollIntoViewIfNeeded();
  await expect(page.locator('#status')).toContainText('compiled', { timeout: 30_000 });
  await page.evaluate(() => scrollTo(0, 0));
}

const tabNames = (page) => page.locator('#tabs .pick').evaluateAll((picks) => picks.map((p) => p.firstChild.textContent));
const activeTab = (page) => page.locator('#tabs .tab.active .pick');
const openFiles = (page) => page.evaluate(() => Object.fromEntries(window.pencil.files().map((f) => [f.path, f.text])));
const colors = (page) => page.evaluate(() => window.pencil.figure.colors);

/** Presses one of the bars' buttons. A narrow window keeps them in a menu, which is opened first. */
async function press(within, id) {
  const button = within.locator(`#${id}`);
  if (!(await button.isVisible())) {
    const menu = await button.evaluate((b) => b.closest('.menu').id);
    await within.locator(`[popovertarget="${menu}"]`).click();
  }
  await button.click();
}

/** Whether the window is narrow enough that the bars' buttons are in menus. */
const inMenus = (page) => page.viewportSize().width <= 800;

/** Types at the end of what's in the editor. */
async function typeAtEnd(page, text) {
  const source = page.locator('#source');
  await source.focus();
  await source.evaluate((t) => t.setSelectionRange(t.value.length, t.value.length));
  await page.keyboard.type(text);
}

const DOC = `doc ring
viewport { x: -2..2, y: -2..2 }
use "parts/r.pen"
curve c = implicit (x, y): x² + y² = r²
`;
const SHEET = `let ring_red = rgb(200, 40, 40)
style c::inside { fill: ring_red }
style c::outside { fill: rgb(40, 40, 200) }
`;
const SOURCES = {
  name: 'a ring',
  doc: 'ring.pen',
  sheets: ['look.style'],
  files: { 'ring.pen': DOC, 'look.style': SHEET, 'parts/r.pen': 'let r = 1.25\n', 'spare.pen': '// nothing uses this yet\n' },
};

test.describe('the playground', () => {
  test('?figure= picks a figure and its style, and a style keeps the document and its edits', async ({ page }) => {
    await openPlayground(page, 'figure=circle-bands');
    await expect(page.locator('#figure-pick')).toHaveValue('circle');
    await expect(page.locator('#style-pick')).toHaveValue('circle-bands');
    await expect(page.locator('#style-pick')).toBeEnabled();
    expect(await tabNames(page)).toEqual(['circle.pen', 'bands.style']);
    await drawnHere(page);

    await typeAtEnd(page, '\n// an edit of mine\n');
    await page.locator('#style-pick').selectOption('circle');
    await expect.poll(() => new URL(page.url()).searchParams.get('figure')).toBe('circle');
    await expect.poll(() => tabNames(page)).toEqual(['circle.pen', 'circle.style']);
    await expect(page.locator('#figure-pick')).toHaveValue('circle');
    expect((await openFiles(page))['corpus/001-circle/circle.pen']).toContain('// an edit of mine');
    await expect(page.locator('#source')).toHaveValue(/an edit of mine/);

    // A figure with one style shows it, and it can't be changed.
    await page.locator('#figure-pick').selectOption('bounds');
    await expect.poll(() => new URL(page.url()).searchParams.get('figure')).toBe('bounds');
    await expect(page.locator('#style-pick')).toBeDisabled();
    await expect(page.locator('#style-pick option')).toHaveCount(1);
  });

  test('a link opens its figure as yours, and draws it', async ({ page }) => {
    const link = await pack(SOURCES);
    await openPlayground(page, '', `#pen=${link}`);
    await expect(page.locator('#figure-pick')).toHaveValue('yours');
    await expect(page.locator('#figure-pick option:checked')).toHaveText('Yours: a ring');
    expect(await tabNames(page)).toEqual(['ring.pen', 'r.pen', 'look.style', 'spare.pen']);
    expect(await openFiles(page)).toEqual(SOURCES.files);
    await expect(page.locator('#source')).toHaveValue(DOC);
    // A file nothing uses is marked, and one you added can be removed; one that's used can't.
    await expect(page.locator('#tabs .tab.unused .pick')).toHaveText(/spare\.pen/);
    await expect(page.getByRole('button', { name: 'Remove spare.pen' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Remove r.pen' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Remove ring.pen' })).toHaveCount(0);
    await drawnHere(page);
    await expect(page.locator('#errors')).toHaveText('');
    expect((await colors(page)).ring_red).toBeTruthy();
  });

  test('a damaged link says so where errors show, and the default example opens', async ({ page }) => {
    await openPlayground(page, '', '#pen=this-is-not-a-figure');
    await expect(page.locator('#errors')).toContainText('doesn’t hold a figure');
    await expect(page.locator('#figure-pick')).toHaveValue('explainer');
    expect(await tabNames(page)).toContain('limacon.pen');

    // A link pasted into the same tab opens in place.
    const link = await pack(SOURCES);
    await page.evaluate((l) => { location.hash = `pen=${l}`; }, link);
    await expect(page.locator('#figure-pick')).toHaveValue('yours');
    expect(await openFiles(page)).toEqual(SOURCES.files);
    await expect(page.locator('#errors')).not.toContainText('doesn’t hold a figure');
  });

  test('files dropped on the editor open as Open does', async ({ page }) => {
    await openPlayground(page, 'figure=circle');
    await page.locator('.editor').evaluate((editor, [doc, sheet]) => {
      const data = new DataTransfer();
      data.items.add(new File([doc], 'drop.pen', { type: 'text/plain' }));
      data.items.add(new File([sheet], 'drop.style', { type: 'text/plain' }));
      editor.dispatchEvent(new DragEvent('dragover', { dataTransfer: data, bubbles: true, cancelable: true }));
      editor.dispatchEvent(new DragEvent('drop', { dataTransfer: data, bubbles: true, cancelable: true }));
    }, [DOC.replace('use "parts/r.pen"\n', 'let r = 1\n'), SHEET]);
    await expect(page.locator('#figure-pick option:checked')).toHaveText('Yours: drop');
    expect(await tabNames(page)).toEqual(['drop.pen', 'drop.style']);
  });

  test('Share puts the figure in the address, and that address opens the same files', async ({ page, browser, browserName }, testInfo) => {
    if (browserName === 'chromium') await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    await openPlayground(page, 'figure=circle');
    await typeAtEnd(page, '\n// shared\n');
    await press(page, 'share');
    await expect.poll(() => new URL(page.url()).hash).toMatch(/^#pen=/);
    await expect(page.locator('#said')).toHaveText(/Link copied|address bar/);
    const shared = await openFiles(page);

    const context = await browser.newContext(testInfo.project.use);
    const fresh = await context.newPage();
    await fresh.goto(page.url());
    await settled(fresh);
    await expect(fresh.locator('#figure-pick')).toHaveValue('yours');
    expect(await openFiles(fresh)).toEqual(shared);
    expect(await tabNames(fresh)).toEqual(['circle.pen', 'circle.style']);
    await expect(fresh.locator('#source')).toHaveValue(/\/\/ shared/);
    await context.close();
  });

  test('Open makes your figure from files; a reload keeps it, and Revert goes back to the file as opened', async ({ page }) => {
    await openPlayground(page, 'figure=circle');
    await page.locator('#open-files').setInputFiles([
      { name: 'dot.style', mimeType: 'text/plain', buffer: Buffer.from(SHEET) },
      { name: 'dot.pen', mimeType: 'text/plain', buffer: Buffer.from(DOC.replace('use "parts/r.pen"\n', 'let r = 1\n')) },
    ]);
    await expect(page.locator('#figure-pick')).toHaveValue('yours');
    await expect(page.locator('#figure-pick option:checked')).toHaveText('Yours: dot');
    expect(await tabNames(page)).toEqual(['dot.pen', 'dot.style']);
    await expect.poll(() => new URL(page.url()).searchParams.get('figure')).toBe('yours');
    await typeAtEnd(page, '// changed\n');
    await expect(activeTab(page)).toHaveClass(/pick/);
    await expect(page.locator('#tabs .tab.active')).toHaveClass(/edited/);

    await page.reload();
    await settled(page);
    await expect(page.locator('#figure-pick')).toHaveValue('yours');
    expect(await tabNames(page)).toEqual(['dot.pen', 'dot.style']);
    await expect(page.locator('#source')).toHaveValue(/\/\/ changed/);
    await drawnHere(page);
    expect((await colors(page)).ring_red).toBeTruthy();

    await press(page, 'revert');
    await expect(page.locator('#source')).not.toHaveValue(/\/\/ changed/);
    await expect(page.locator('#source')).toHaveValue(/let r = 1\n/);
    await expect(page.locator('#tabs .tab.active')).not.toHaveClass(/edited/);
    // The examples' own edits weren't touched.
    expect(await page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('pencil-playground:')))).toEqual([]);
  });

  test('a sheet added and edited changes the figure, and removing it changes it back', async ({ page }) => {
    await openPlayground(page, 'figure=circle');
    await drawnHere(page);
    // A pixel inside the circle, a little above the center: WebKit's screenshot of the canvas on
    // an iPhone can leave its lower half blank.
    const center = async () => {
      const png = await page.locator('#figure').screenshot();
      return page.evaluate(async (data) => {
        const bitmap = await createImageBitmap(new Blob([Uint8Array.from(atob(data), (c) => c.charCodeAt(0))], { type: 'image/png' }));
        const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
        const context = canvas.getContext('2d');
        context.drawImage(bitmap, 0, 0);
        return [...context.getImageData(bitmap.width >> 1, Math.round(bitmap.height * 0.42), 1, 1).data.slice(0, 3)];
      }, png.toString('base64'));
    };
    const green = ([r, g, b]) => g > 150 && r < 90 && b < 90;
    const before = await center();
    expect(green(before)).toBe(false);
    expect((await colors(page)).mine_green).toBeUndefined();

    await page.getByRole('button', { name: 'Add a file' }).click();
    await expect(page.locator('#add-dialog')).toBeVisible();
    await page.locator('#add-name').fill('mine.style');
    await page.locator('#add-name').press('Enter');
    await expect(page.locator('#add-dialog')).toBeHidden();
    await expect(page.locator('#figure-pick')).toHaveValue('yours');
    await expect(page.locator('#figure-pick option:checked')).toHaveText('Yours: 001 circle');
    expect(await tabNames(page)).toEqual(['circle.pen', 'circle.style', 'mine.style']);
    await expect(activeTab(page)).toHaveText(/mine\.style/);
    await expect(page.locator('#source')).toBeFocused();

    await page.keyboard.type('let mine_green = rgb(10, 200, 30)\nstyle c::inside { fill: mine_green }\n');
    await expect.poll(() => colors(page).then((c) => c.mine_green)).toBeTruthy();
    await expect.poll(async () => green(await center())).toBe(true);

    await page.getByRole('button', { name: 'Remove mine.style' }).click();
    await expect.poll(() => tabNames(page)).toEqual(['circle.pen', 'circle.style']);
    await expect.poll(() => colors(page).then((c) => c.mine_green)).toBeUndefined();
    await expect.poll(async () => green(await center())).toBe(false);
    // The example is as it was.
    await page.locator('#figure-pick').selectOption('circle');
    await expect.poll(() => tabNames(page)).toEqual(['circle.pen', 'circle.style']);
    await expect(page.locator('#tabs .remove')).toHaveCount(1);
  });

  test('a .pen added stays as unused until the document uses it', async ({ page }) => {
    await openPlayground(page, 'figure=circle');
    await page.getByRole('button', { name: 'Add a file' }).click();
    await page.locator('#add-name').fill('r.pen');
    await page.locator('#add-name').press('Enter');
    await expect.poll(() => tabNames(page)).toEqual(['circle.pen', 'circle.style', 'r.pen']);
    await expect(page.locator('#tabs .tab.unused .pick')).toHaveText(/r\.pen/);
    await typeAtEnd(page, 'let r_used = 1\n');
    await page.locator('#tabs .pick', { hasText: 'circle.pen' }).click();
    await typeAtEnd(page, 'use "r.pen"\n');
    await expect.poll(() => tabNames(page)).toEqual(['circle.pen', 'r.pen', 'circle.style']);
    await expect(page.locator('#tabs .tab.unused')).toHaveCount(0);
    await expect(page.locator('#errors')).toHaveText('');
    // A name that isn't a file's is refused in the dialog.
    await page.getByRole('button', { name: 'Add a file' }).click();
    await page.locator('#add-name').fill('notes.txt');
    await page.locator('#add-name').press('Enter');
    await expect(page.locator('#add-dialog')).toBeVisible();
    await expect(page.locator('#add-bad')).toContainText('.style or .pen');
    await expect(page.locator('#add-name')).toHaveAttribute('aria-invalid', 'true');
    await page.keyboard.press('Escape');
    await expect(page.locator('#add-dialog')).toBeHidden();
  });

  test('Save downloads the active file under its name', async ({ page }) => {
    await openPlayground(page, 'figure=circle-bands');
    await page.locator('#tabs .pick', { hasText: 'bands.style' }).click();
    await typeAtEnd(page, '// saved\n');
    const [download] = await Promise.all([page.waitForEvent('download'), press(page, 'save')]);
    expect(download.suggestedFilename()).toBe('bands.style');
    const text = await readFile(await download.path(), 'utf8');
    expect(text).toBe(await page.locator('#source').inputValue());
    expect(text).toContain('// saved');
  });

  test('replacing your figure with changes asks first, in a dialog', async ({ page }) => {
    await openPlayground(page, 'figure=circle');
    await page.getByRole('button', { name: 'Add a file' }).click();
    await page.locator('#add-name').fill('mine.style');
    await page.locator('#add-name').press('Enter');
    await expect(page.locator('#figure-pick')).toHaveValue('yours');

    // Open is in the bar, or on a phone in the bar's menu, whose button has focus once it's chosen.
    const opener = inMenus(page) ? page.locator('#figure-more') : page.getByRole('button', { name: 'Open…' });
    await opener.focus();
    const other = [{ name: 'other.pen', mimeType: 'text/plain', buffer: Buffer.from(DOC.replace('use "parts/r.pen"\n', 'let r = 1\n')) }];
    await page.locator('#open-files').setInputFiles(other);
    const dialog = page.locator('#replace-dialog');
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole('button', { name: 'Keep mine' })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(opener).toBeFocused();
    expect(await tabNames(page)).toEqual(['circle.pen', 'circle.style', 'mine.style']);

    await page.locator('#open-files').setInputFiles(other);
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Replace it' }).click();
    await expect.poll(() => tabNames(page)).toEqual(['other.pen']);
    await expect(page.locator('#figure-pick option:checked')).toHaveText('Yours: other');

    // Once shared, it can be replaced without asking.
    await press(page, 'share');
    await expect.poll(() => new URL(page.url()).hash).toMatch(/^#pen=/);
    await page.locator('#open-files').setInputFiles([{ ...other[0], name: 'third.pen' }]);
    await expect.poll(() => tabNames(page)).toEqual(['third.pen']);
    await expect(dialog).toBeHidden();
  });

  test('everything added is reached with Tab and works from the keyboard', async ({ page, isMobile, browserName }) => {
    test.skip(isMobile, 'a phone has no Tab key');
    // WebKit's Tab, as Safari's, only reaches fields unless the reader turns that on; Option-Tab
    // reaches everything.
    const TAB = browserName === 'webkit' ? 'Alt+Tab' : 'Tab';
    await openPlayground(page, 'figure=circle');
    await page.locator('#figure-pick').focus();
    const focused = () => page.evaluate(() => {
      const el = document.activeElement;
      return el.id || el.getAttribute('aria-label') || el.textContent.trim();
    });
    const order = [];
    for (let i = 0; i < 9; i++) {
      await page.keyboard.press(TAB);
      order.push(await focused());
    }
    expect(order).toEqual(['style-pick', 'open', 'share', 'circle.pen', 'circle.style', 'Remove circle.style', 'add', 'copy', 'save']);

    // Arrows move between the tabs, and Enter shows one.
    await page.locator('#tabs .pick').first().focus();
    await page.keyboard.press('ArrowRight');
    expect(await focused()).toBe('circle.style');
    await page.keyboard.press('Enter');
    await expect(activeTab(page)).toHaveText(/circle\.style/);
    await expect(activeTab(page)).toBeFocused();

    // The add dialog keeps Tab inside, and gives focus back when it's dismissed.
    await page.locator('#add').focus();
    await page.keyboard.press('Enter');
    const dialog = page.locator('#add-dialog');
    await expect(dialog).toBeVisible();
    await expect(page.locator('#add-name')).toBeFocused();
    await page.keyboard.press(TAB);
    expect(await focused()).toBe('add-cancel');
    await page.keyboard.press(TAB);
    expect(await focused()).toBe('Add');
    await page.keyboard.press(TAB);
    await expect(page.locator('#add-name')).toBeFocused();
    await page.keyboard.press(`Shift+${TAB}`);
    expect(await focused()).toBe('Add');
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(page.locator('#add')).toBeFocused();

    // Space opens it too; Enter adds the file.
    await page.keyboard.press(' ');
    await expect(dialog).toBeVisible();
    await page.keyboard.type('k.style');
    await page.keyboard.press('Enter');
    await expect(dialog).toBeHidden();
    await expect.poll(() => tabNames(page)).toEqual(['circle.pen', 'circle.style', 'k.style']);
    await expect(page.locator('#source')).toBeFocused();

    // Its remove button is reached with Tab, and Space removes it.
    await page.locator('#tabs .pick', { hasText: 'k.style' }).focus();
    await page.keyboard.press(TAB);
    expect(await focused()).toBe('Remove k.style');
    await page.keyboard.press(' ');
    await expect.poll(() => tabNames(page)).toEqual(['circle.pen', 'circle.style']);
    await expect(activeTab(page)).toBeFocused();

    // The Open button opens the file chooser from the keyboard.
    await page.locator('#open').focus();
    const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Enter')]);
    expect(chooser.isMultiple()).toBe(true);
  });

  test('the editor writes a symbol for its name, and for an arrow typed in a label', async ({ page }) => {
    await openPlayground(page, 'figure=circle');
    const source = page.locator('#source');
    // A name after a backslash, done by whatever can't continue it, which stays.
    await typeAtEnd(page, '\nlet \\theta ');
    await expect(source).toHaveValue(/\nlet θ $/);
    // It says what it wrote.
    await expect(page.locator('#pasted')).toHaveText(/θ/);
    await typeAtEnd(page, '= 1\n');
    await expect(source).toHaveValue(/\nlet θ = 1\n$/);
    // In a label's text an arrow is its symbol, and a name still is.
    await typeAtEnd(page, 'label q = "$f: \\R \\to \\R, x |-> x^2, a <= b$" at (0, 0)\n');
    await expect(source).toHaveValue(/label q = "\$f: ℝ → ℝ, x ↦ x\^2, a ≤ b\$" at \(0, 0\)\n$/);
    // Outside one it's what was typed, since there it could be anything.
    await typeAtEnd(page, '// a -> b, and \\nosuchname stays\n');
    await expect(source).toHaveValue(/\/\/ a -> b, and \\nosuchname stays\n$/);
    // Tab finishes a name with nothing after it, and is two spaces anywhere else.
    await typeAtEnd(page, '// \\alpha');
    await page.keyboard.press('Tab');
    await expect(source).toHaveValue(/\/\/ α$/);
    await page.keyboard.press('Tab');
    await expect(source).toHaveValue(/\/\/ α {2}$/);
  });
});

test.describe('the playground in a frame', () => {
  test('Share gives the framing page\'s address, and that address opens the figure in its frame', async ({ page, browser }, testInfo) => {
    const query = addressOf('figure=circle').split('?')[1];
    await page.goto(`/packages/figure/tests/pages/framed.html?${query}`);
    const frame = page.frameLocator('#playground');
    const inner = () => page.frame({ url: /playground\.html/ });
    await expect.poll(() => inner()?.evaluate(() => !!window.pencil).catch(() => false), { timeout: 30_000 }).toBe(true);
    await frame.locator('#source').focus();
    await frame.locator('#source').evaluate((t) => t.setSelectionRange(t.value.length, t.value.length));
    await page.keyboard.type('// from a frame\n');
    await press(frame, 'share');
    await expect.poll(() => new URL(page.url()).hash).toMatch(/^#pen=/);
    expect(new URL(page.url()).pathname).toBe('/packages/figure/tests/pages/framed.html');
    await expect(frame.locator('#said')).toHaveText(/Link copied|address bar/);
    const shared = await inner().evaluate(() => Object.fromEntries(window.pencil.files().map((f) => [f.path, f.text])));

    const context = await browser.newContext(testInfo.project.use);
    const fresh = await context.newPage();
    await fresh.goto(page.url());
    const freshInner = () => fresh.frame({ url: /playground\.html/ });
    await expect.poll(() => freshInner()?.evaluate(() => !!window.pencil).catch(() => false), { timeout: 30_000 }).toBe(true);
    await expect(fresh.frameLocator('#playground').locator('#figure-pick')).toHaveValue('yours');
    expect(await freshInner().evaluate(() => Object.fromEntries(window.pencil.files().map((f) => [f.path, f.text])))).toEqual(shared);
    await context.close();
  });
});

test.describe('the playground\'s bars', () => {
  test('the tabs stay on one line, and a file that comes into view brings its tab', async ({ page }) => {
    await openPlayground(page, 'figure=explainer');
    await expect.poll(() => tabNames(page)).toContain('caption.style');
    const tabs = page.locator('#tabs');
    const line = () => tabs.evaluate((list) => ({
      tops: new Set([...list.children].map((tab) => Math.round(tab.getBoundingClientRect().bottom))).size,
      past: list.scrollWidth - list.clientWidth,
      at: list.scrollLeft,
      cut: [list.classList.contains('cut-left'), list.classList.contains('cut-right')],
    }));
    const first = await line();
    expect(first.tops).toBe(1);
    expect(first.past).toBeGreaterThan(0);
    expect(first.cut).toEqual([false, true]);
    // The button that adds a file stays beside them, in view.
    await expect(page.locator('#add')).toBeInViewport();

    // End moves to the last tab, and showing it scrolls it in.
    await page.locator('#tabs .pick').first().focus();
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await expect(activeTab(page)).toHaveText(/caption\.style/);
    const within = () => tabs.evaluate((list) => {
      const tab = list.querySelector('.tab.active').getBoundingClientRect();
      const box = list.getBoundingClientRect();
      return tab.left >= box.left - 1 && tab.right <= box.right + 1;
    });
    await expect.poll(within).toBe(true);
    await expect.poll(async () => (await line()).cut).toEqual([true, false]);

    // Typing redraws the tabs, and leaves the line where it is.
    const before = (await line()).at;
    await typeAtEnd(page, '\n');
    await expect(page.locator('#tabs .tab.active')).toHaveClass(/edited/);
    expect((await line()).at).toBe(before);
  });

  test('a narrow window has one small bar, with the rest in two menus worked by the keyboard', async ({ page }) => {
    test.skip(!inMenus(page), 'a wide window shows the buttons in place');
    await openPlayground(page, 'figure=explainer');
    const height = (selector) => page.locator(selector).evaluate((el) => el.getBoundingClientRect().height);
    expect(await height('header')).toBeLessThan(48);
    expect(await height('.files')).toBeLessThan(40);
    // The figure is right under the bar and the editor under the figure, both on screen, and the
    // page has nothing to scroll.
    const laid = await page.evaluate(() => {
      const box = (selector) => document.querySelector(selector).getBoundingClientRect();
      const [figure, code] = [box('#figure'), box('.code')];
      return {
        // The figure's room is right under the bar; the figure is in the middle of it.
        figureTop: document.querySelector('.view').getBoundingClientRect().top,
        figureShown: figure.height > 100 && figure.bottom <= innerHeight,
        under: code.top >= figure.bottom,
        codeShown: code.height >= 100 && code.bottom <= innerHeight,
        scrolls: document.documentElement.scrollHeight - innerHeight,
        // And they're in the page in that order, for a screen reader and for Tab.
        inOrder: !!(document.querySelector('.view').compareDocumentPosition(document.querySelector('.editor')) & Node.DOCUMENT_POSITION_FOLLOWING),
      };
    });
    expect(laid.figureTop).toBeLessThan(60);
    expect(laid).toMatchObject({ figureShown: true, under: true, codeShown: true, inOrder: true });
    expect(laid.scrolls).toBeLessThanOrEqual(0);
    await expect(page.locator('#share')).toBeHidden();
    await expect(page.locator('#copy')).toBeHidden();

    // The bar's menu opens under its button with focus on its first choice; arrows move, Esc closes.
    const more = page.locator('#figure-more');
    await more.click();
    await expect(page.locator('#figure-menu')).toBeVisible();
    await expect(page.locator('#open')).toBeFocused();
    const under = await page.evaluate(() => {
      const menu = document.getElementById('figure-menu').getBoundingClientRect();
      const button = document.getElementById('figure-more').getBoundingClientRect();
      return menu.top >= button.bottom && menu.right <= innerWidth && menu.left >= 0;
    });
    expect(under).toBe(true);
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('#share')).toBeFocused();
    await page.keyboard.press('ArrowUp');
    await expect(page.locator('#open')).toBeFocused();
    await page.keyboard.press('End');
    await expect(page.locator('#share')).toBeFocused();
    await expect(page.locator('#figure-menu .hint')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('#figure-menu')).toBeHidden();
    await expect(more).toBeFocused();

    // The file's menu names the file, and a choice closes it and says what it did.
    await typeAtEnd(page, '// mine\n');
    await page.locator('#file-more').click();
    await expect(page.locator('#path')).toContainText('lab/explainer/limacon.pen');
    await expect(page.locator('#copy')).toBeFocused();
    await page.locator('#revert').click();
    await expect(page.locator('#file-menu')).toBeHidden();
    await expect(page.locator('#source')).not.toHaveValue(/\/\/ mine/);
  });

  test('a window made narrow puts the editor under the figure, and keeps the caret and the figure', async ({ page, isMobile }) => {
    test.skip(isMobile, 'a phone starts narrow');
    await openPlayground(page, 'figure=circle');
    await drawnHere(page);
    const order = () => page.evaluate(() => [...document.querySelector('.app').children].map((el) => el.className.split(' ')[0] || el.tagName.toLowerCase()));
    expect(await order()).toEqual(['header', 'editor', 'panel', 'view']);
    const source = page.locator('#source');
    await source.focus();
    await source.evaluate((t) => t.setSelectionRange(12, 20));
    await page.evaluate(() => {
      window.loads = 0;
      document.getElementById('figure').addEventListener('load', () => (window.loads += 1));
    });

    await page.setViewportSize({ width: 375, height: 812 });
    await expect.poll(order).toEqual(['header', 'view', 'editor', 'panel']);
    await expect(source).toBeFocused();
    expect(await source.evaluate((t) => [t.selectionStart, t.selectionEnd])).toEqual([12, 20]);
    await expect(page.locator('#figure-more')).toBeVisible();

    await page.setViewportSize({ width: 1633, height: 919 });
    await expect.poll(order).toEqual(['header', 'editor', 'panel', 'view']);
    await expect(source).toBeFocused();
    expect(await source.evaluate((t) => [t.selectionStart, t.selectionEnd])).toEqual([12, 20]);
    // The figure wasn't moved, so it didn't start again.
    expect(await page.evaluate(() => window.loads)).toBe(0);
  });

  test('framed, it offers to open alone at the address it has', async ({ page }) => {
    const query = addressOf('figure=circle').split('?')[1];
    await page.goto(`/packages/figure/tests/pages/framed.html?${query}`);
    const frame = page.frameLocator('#playground');
    const inner = () => page.frame({ url: /playground\.html/ });
    await expect.poll(() => inner()?.evaluate(() => !!window.pencil).catch(() => false), { timeout: 30_000 }).toBe(true);
    await frame.locator('#style-pick').selectOption('circle-bands');
    await expect.poll(() => new URL(inner().url()).searchParams.get('figure')).toBe('circle-bands');
    if (inMenus(page)) await frame.locator('#figure-more').click();
    const alone = frame.locator('#alone');
    await expect(alone).toBeVisible();
    await alone.click();
    await page.waitForURL(/\/web\/playground\.html/);
    expect(new URL(page.url()).searchParams.get('figure')).toBe('circle-bands');
    await settled(page);
    // Alone, there's nothing to leave.
    await expect(page.locator('#alone')).toBeHidden();
  });


  test('nothing runs off the side, and the Style menu doesn\'t move the rest', async ({ page }) => {
    await openPlayground(page, 'figure=circle');
    const overflow = () => page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(await overflow()).toBeLessThanOrEqual(0);
    const boxes = () => page.evaluate(() => ['figure-pick', 'style-pick', 'open', 'share'].map((id) => {
      const r = document.getElementById(id).getBoundingClientRect();
      return [Math.round(r.x), Math.round(r.y), Math.round(r.width)];
    }));
    const first = await boxes();
    await page.locator('#style-pick').selectOption('circle-bands');
    await expect.poll(() => new URL(page.url()).searchParams.get('figure')).toBe('circle-bands');
    expect(await boxes()).toEqual(first);
    await page.locator('#figure-pick').selectOption('explainer');
    await expect.poll(() => tabNames(page)).toContain('limacon.pen');
    expect(await boxes()).toEqual(first);
    expect(await overflow()).toBeLessThanOrEqual(0);
  });
});

// On a touch screen: the Pencil keys under the editor (web/keyboard.js), and a page that holds
// still at what's visible (web/viewport.js). With a mouse, neither, and the page as it was.
test.describe('the playground\'s Pencil keys', () => {
  const keyboard = (page) => page.locator('#keyboard');
  /** Presses a key by what it types (`=x`), does (`@back`) or finishes (`~curve`), as a finger does. */
  const tapKey = async (page, id) => {
    await page.locator(`#keyboard button[data-id="${id}"]`).filter({ visible: true }).tap();
  };
  const typeKeys = async (page, ...ids) => {
    for (const id of ids) await tapKey(page, id);
  };
  /** Touches the editor and puts the caret at the end of the file. */
  const touchEditor = async (page) => {
    await page.locator('#source').tap();
    await page.locator('#source').evaluate((t) => t.setSelectionRange(t.value.length, t.value.length));
    await expect(keyboard(page)).toBeVisible();
  };
  const box = (page, selector) => page.locator(selector).evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { top: r.top, height: r.height, scrollTop: el.scrollTop };
  });
  const stays = (page) => page.evaluate(() => ({
    scroll: document.documentElement.scrollHeight - innerHeight,
    app: Math.round(document.querySelector('.app').getBoundingClientRect().height) - innerHeight,
    scrolled: scrollY,
  }));

  test.describe('on a touch screen', () => {
    test.beforeEach(({ isMobile }) => {
      test.skip(!isMobile, 'touch projects only');
    });

    test('touching the editor brings the keys up, the system keyboard stays down, and keys type into the figure', async ({ page }) => {
      await openPlayground(page, 'figure=tangents');
      await drawnHere(page);
      await expect(keyboard(page)).toBeHidden();
      const source = page.locator('#source');
      await expect(source).toHaveAttribute('inputmode', 'none');
      await expect(source).toHaveCSS('font-size', '16px');
      await touchEditor(page);
      await expect(source).toBeFocused();
      await expect(source).toHaveAttribute('inputmode', 'none');
      const before = await source.inputValue();
      const compiled = await page.locator('#status').textContent();
      // `let k = 2` on a line of its own, from three pages.
      await typeKeys(page, '@enter', '@page:letters', '=l', '=e', '=t', '@space', '=k', '@space', '@page:numbers', '==', '@space', '=2');
      await expect(source).toHaveValue(`${before}\nlet k = 2`);
      await expect(source).toBeFocused();
      // It compiled again, and drew.
      await expect(page.locator('#status')).not.toHaveText(compiled);
      await expect(page.locator('#status')).toContainText('compiled');
      await expect(page.locator('#errors')).toHaveText('');
      // ⌫ takes back one, and the caret keys move.
      await typeKeys(page, '@back', '@left', '@left');
      await expect(source).toHaveValue(`${before}\nlet k = `);
      expect(await source.evaluate((t) => t.selectionStart)).toBe(before.length + 7);
      // A hardware keyboard still types into it.
      await page.keyboard.type('x');
      await expect(source).toHaveValue(`${before}\nlet k x= `);
    });

    test('a suggestion finishes the word, by what a .pen and a .style have where the caret is', async ({ page }) => {
      await openPlayground(page, 'figure=tangents');
      await touchEditor(page);
      const source = page.locator('#source');
      const before = await source.inputValue();
      await typeKeys(page, '@enter', '@page:letters', '=c', '=u');
      await expect(page.locator('#keyboard .words-row button').first()).toHaveText('curve');
      await tapKey(page, '~curve');
      await expect(source).toHaveValue(`${before}\ncurve `);

      // In a sheet, a rule's properties, then its values, the sheet's own names first.
      await page.locator('#tabs .pick', { hasText: 'plain.style' }).click();
      await touchEditor(page);
      const sheet = await source.inputValue();
      await typeKeys(page, '@enter', '@page:letters', '=s');
      await expect(page.locator('#keyboard .words-row button').first()).toHaveText('style');
      await tapKey(page, '~style');
      await typeKeys(page, '=c', '@space', '={', '@space', '=w');
      await tapKey(page, '~width');
      await expect(source).toHaveValue(`${sheet}\nstyle c { width: `);
    });

    test('the words row and the four pages keep one height, so the editor doesn\'t move', async ({ page }) => {
      await openPlayground(page, 'figure=tangents');
      await touchEditor(page);
      await typeKeys(page, '@enter', '@page:letters');
      const code = () => box(page, '#source');
      const rows = () => page.evaluate(() => [...document.querySelectorAll('#keyboard > :not([hidden])')].map((el) => el.getBoundingClientRect().height));
      const first = await code();
      expect(first.scrollTop).toBeGreaterThan(0);
      const heights = await rows();
      const seen = [];
      // Empty, full, a few, empty again: `z` has no words, ⌫ brings them all, `s` a few, `sq` none.
      for (let i = 0; i < 3; i++) {
        for (const [ids, words] of [[['=z'], 0], [['@back'], 1], [['=s'], 1], [['=q'], 0], [['@back', '@back'], 1]]) {
          await typeKeys(page, ...ids);
          const empty = await page.locator('#keyboard .words-row .none').count();
          expect(empty === 0).toBe(words === 1);
          seen.push(await code());
          expect(await rows()).toEqual(heights);
        }
      }
      for (const id of ['@page:numbers', '@page:words', '@page:greek', '@shift', '@page:letters', '@shift']) {
        await tapKey(page, id);
        seen.push(await code());
        expect(await rows()).toEqual(heights);
      }
      for (const s of seen) expect(s).toEqual(first);
    });

    test('the page never scrolls, with the keys up or down', async ({ page }) => {
      await openPlayground(page, 'figure=explainer');
      await expect(page.locator('html')).toHaveClass(/touch/);
      expect(await stays(page)).toEqual({ scroll: 0, app: 0, scrolled: 0 });
      await touchEditor(page);
      expect(await stays(page)).toEqual({ scroll: 0, app: 0, scrolled: 0 });
      // The figure, the editor and the keys are all on screen, in that order.
      const laid = await page.evaluate(() => {
        const r = (s) => document.querySelector(s).getBoundingClientRect();
        return { figure: r('#figure'), code: r('.code'), keys: r('#keyboard') };
      });
      expect(laid.figure.height).toBeGreaterThan(40);
      expect(laid.code.top).toBeGreaterThanOrEqual(laid.figure.bottom);
      expect(laid.keys.top).toBeGreaterThanOrEqual(laid.code.bottom);
      expect(laid.keys.bottom).toBeLessThanOrEqual(page.viewportSize().height + 0.5);
      await tapKey(page, '@hide');
      await expect(keyboard(page)).toBeHidden();
      expect(await stays(page)).toEqual({ scroll: 0, app: 0, scrolled: 0 });
    });

    test('the system key hands over to the system keyboard and back, and is remembered for the visit', async ({ page }) => {
      await openPlayground(page, 'figure=circle-bands');
      await touchEditor(page);
      const source = page.locator('#source');
      await source.evaluate((t) => t.setSelectionRange(5, 5));
      await tapKey(page, '@system');
      await expect(source).not.toHaveAttribute('inputmode');
      await expect(source).toBeFocused();
      expect(await source.evaluate((t) => t.selectionStart)).toBe(5);
      await expect(page.locator('#keyboard .keys')).toBeHidden();
      await expect(page.locator('#keyboard .symbols')).toBeVisible();
      await expect(page.locator('#keyboard .words-row')).toBeVisible();
      // The symbols type as the keys do.
      await tapKey(page, '=·');
      await expect(source).toHaveValue(/^.{5}·/s);

      await page.reload();
      await settled(page);
      await touchEditor(page);
      await expect(source).not.toHaveAttribute('inputmode');
      await tapKey(page, '@pencil');
      await expect(source).toHaveAttribute('inputmode', 'none');
      await expect(page.locator('#keyboard .keys')).toBeVisible();
      await expect(page.locator('#keyboard .symbols')).toBeHidden();
    });

    test('a compile error stays in view above the keys', async ({ page }) => {
      await openPlayground(page, 'figure=explainer');
      await drawnHere(page);
      await touchEditor(page);
      await typeKeys(page, '@enter', '=)');
      const error = page.locator('#errors li').first();
      await expect(error).toContainText('limacon.pen');
      const where = await page.evaluate(() => {
        const e = document.querySelector('#errors li').getBoundingClientRect();
        const view = document.querySelector('.view').getBoundingClientRect();
        return { top: e.top, bottom: e.bottom, viewBottom: view.bottom, keys: document.getElementById('keyboard').getBoundingClientRect().top };
      });
      expect(where.top).toBeGreaterThanOrEqual(0);
      expect(where.bottom).toBeLessThanOrEqual(where.viewBottom + 0.5);
      expect(where.bottom).toBeLessThanOrEqual(where.keys);
      // Going to it keeps the keys up.
      await error.click();
      await expect(page.locator('#source')).toBeFocused();
      await expect(keyboard(page)).toBeVisible();
    });

    test('Share, the menus and a link work with the keys up', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      await touchEditor(page);
      await typeKeys(page, '@enter', '@page:letters', '=l', '=e', '=t', '@space', '=q', '@space', '@page:numbers', '==', '@space', '=3');
      await page.locator('#figure-more').click();
      await page.locator('#share').click();
      await expect.poll(() => new URL(page.url()).hash).toMatch(/^#pen=/);
      await expect(page.locator('#said')).toHaveText(/Link copied|address bar/);
      const link = page.url();
      const shared = await openFiles(page);
      await page.goto('about:blank');
      await page.goto(link);
      await settled(page);
      await expect(page.locator('#figure-pick')).toHaveValue('yours');
      expect(await openFiles(page)).toEqual(shared);
      await touchEditor(page);
      await page.locator('#file-more').click();
      await expect(page.locator('#path')).toContainText('circle.pen');
      await page.keyboard.press('Escape');
    });

    test('framed, the frame fills what\'s visible, and neither page scrolls', async ({ page }) => {
      const query = addressOf('figure=circle').split('?')[1];
      await page.goto(`/packages/figure/tests/pages/framed.html?${query}`);
      const frame = page.frameLocator('#playground');
      const inner = () => page.frame({ url: /playground\.html/ });
      await expect.poll(() => inner()?.evaluate(() => !!window.pencil).catch(() => false), { timeout: 30_000 }).toBe(true);
      await frame.locator('#source').tap();
      await expect(frame.locator('#keyboard')).toBeVisible();
      await frame.locator('#keyboard button[data-id="=7"]').tap();
      await expect(frame.locator('#source')).toBeFocused();
      const outer = await page.evaluate(() => ({
        scroll: document.documentElement.scrollHeight - innerHeight,
        frameBottom: Math.round(document.getElementById('playground').getBoundingClientRect().bottom) - innerHeight,
      }));
      expect(outer).toEqual({ scroll: 0, frameBottom: 0 });
      const within = await inner().evaluate(() => ({
        scroll: document.documentElement.scrollHeight - innerHeight,
        keysBottom: Math.round(document.getElementById('keyboard').getBoundingClientRect().bottom) - innerHeight,
      }));
      expect(within).toEqual({ scroll: 0, keysBottom: 0 });
    });
  });

  test.describe('on a touch screen, at the edges', () => {
    test.beforeEach(({ isMobile }) => {
      test.skip(!isMobile, 'touch projects only');
    });

    /** Every key on the page that's up is inside the window. */
    const keysInView = (page) => page.evaluate(() => {
      const out = [];
      // A row that scrolls sideways is in view as a row, and so is the words page's list.
      for (const b of document.querySelectorAll('#keyboard button, #keyboard .run, #keyboard .list')) {
        if (!b.offsetParent || (b.matches('button') && b.closest('.run, .list'))) continue;
        const r = b.getBoundingClientRect();
        if (r.top < -0.5 || r.left < -0.5 || r.bottom > innerHeight + 0.5 || r.right > innerWidth + 0.5) out.push(`${b.dataset.id ?? b.className} ${Math.round(r.top)}..${Math.round(r.bottom)}`);
      }
      return out;
    });

    for (const size of [{ width: 852, height: 393 }, { width: 839, height: 412 }]) {
      test(`held sideways at ${size.width}×${size.height}, every key is in view on every page`, async ({ page }) => {
        await page.setViewportSize(size);
        await openPlayground(page, 'figure=explainer');
        await touchEditor(page);
        expect(await page.evaluate(() => document.querySelector('.app').hasAttribute('data-short'))).toBe(true);
        for (const id of ['@page:numbers', '@page:letters', '@page:words', '@page:greek']) {
          await tapKey(page, id);
          expect(await keysInView(page)).toEqual([]);
        }
        expect(await stays(page)).toEqual({ scroll: 0, app: 0, scrolled: 0 });
      });
    }

    test('with the system keyboard up and 260 px visible, the way back to the keys is in view', async ({ page }) => {
      await openPlayground(page, 'figure=explainer');
      await touchEditor(page);
      await tapKey(page, '@system');
      await expect(page.locator('#source')).not.toHaveAttribute('inputmode');
      // As Safari does when its keyboard is up: the visual viewport is smaller, the layout one isn't.
      await page.evaluate(() => {
        Object.defineProperty(visualViewport, 'height', { configurable: true, get: () => 260 });
        visualViewport.dispatchEvent(new Event('resize'));
      });
      await expect.poll(() => page.evaluate(() => Math.round(document.querySelector('.app').getBoundingClientRect().height))).toBe(260);
      const back = await page.locator('#keyboard button[data-id="@pencil"]').boundingBox();
      expect(back.y).toBeGreaterThanOrEqual(0);
      expect(back.y + back.height).toBeLessThanOrEqual(260);
      const words = await page.locator('#keyboard .words-row').boundingBox();
      expect(words.y + words.height).toBeLessThanOrEqual(260);
      // The editor gave up its room first, and still shows a line or more.
      const code = await page.locator('.code').boundingBox();
      expect(code.height).toBeGreaterThanOrEqual(24);
      await tapKey(page, '@pencil');
      await expect(page.locator('#source')).toHaveAttribute('inputmode', 'none');
    });

    test('a drag on a handle doesn\'t move the figure under the finger until it lifts', async ({ page, browserName }) => {
      test.skip(browserName !== 'chromium', 'real touches come through CDP, which only Chromium has');
      await openPlayground(page, 'figure=tangents');
      await drawnHere(page);
      // Held still, so the handle is where it's found.
      await page.evaluate(() => window.pencil.figure.pause());
      await touchEditor(page);
      const stage = () => page.locator('.view > .stage').boundingBox();
      const before = await stage();
      // The keys coming up made the figure smaller, which moves the handle: wait until it's still.
      let handle = null;
      await expect.poll(async () => {
        const was = handle;
        handle = await handleOf(page, 'P').boundingBox();
        await page.waitForTimeout(150);
        return JSON.stringify(was) === JSON.stringify(handle);
      }, { timeout: 15_000 }).toBe(true);
      const [x, y] = [handle.x + handle.width / 2, handle.y + handle.height / 2];
      const cdp = await page.context().newCDPSession(page);
      const touch = (type, points) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points });
      await touch('touchStart', [{ x, y }]);
      for (let i = 1; i <= 8; i++) {
        await touch('touchMove', [{ x: x + i * 3, y }]);
        await page.waitForTimeout(100);
      }
      // 800 ms into the drag: the handle has focus, and the keys and the figure's room are as they were.
      expect(await page.evaluate(() => document.activeElement?.id)).toBe('figure');
      await expect(keyboard(page)).toBeVisible();
      expect(await stage()).toEqual(before);
      await touch('touchEnd', []);
      await expect(keyboard(page)).toBeHidden();
      // Lifted, the keys go and the controls come back, which changes the figure's room.
      await expect.poll(stage).not.toEqual(before);
    });

    test('a hardware keyboard closes the keys for the visit, and so does ▾, until keys brings them back', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      await touchEditor(page);
      const source = page.locator('#source');
      const opener = page.locator('#keys-open');
      await expect(opener).toBeHidden();
      await page.keyboard.type('q');
      await expect(keyboard(page)).toBeHidden();
      await expect(source).toBeFocused();
      await expect(source).not.toHaveAttribute('inputmode');
      await expect(source).toHaveValue(/q$/);
      await expect(opener).toBeVisible();
      await page.reload();
      await settled(page);
      await page.locator('#source').tap();
      await expect(source).toBeFocused();
      await expect(keyboard(page)).toBeHidden();
      await opener.tap();
      await expect(keyboard(page)).toBeVisible();
      await expect(source).toBeFocused();
      await expect(source).toHaveAttribute('inputmode', 'none');
      await expect(opener).toBeHidden();
      // ▾ is remembered the same way.
      await tapKey(page, '@hide');
      await expect(keyboard(page)).toBeHidden();
      await expect(opener).toBeVisible();
      await page.reload();
      await settled(page);
      await page.locator('#source').tap();
      await expect(keyboard(page)).toBeHidden();
      await expect(opener).toBeVisible();
    });

    test('a mouse paired with the tablet turns the keys and the hold off, and unpairing turns them on', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      const pointer = (coarse) => page.evaluate(async (coarse) => {
        const { touch } = await import('/web/viewport.js');
        Object.defineProperty(touch, 'matches', { configurable: true, get: () => coarse });
        touch.dispatchEvent(new Event('change'));
      }, coarse);
      await pointer(false);
      await expect(page.locator('html')).not.toHaveClass(/touch/);
      await expect(page.locator('#source')).not.toHaveAttribute('inputmode');
      await page.locator('#source').tap();
      await expect(keyboard(page)).toBeHidden();
      expect(await page.evaluate(() => document.querySelector('.app').style.getPropertyValue('--app-h'))).toBe('');
      await page.locator('#figure').tap();
      await pointer(true);
      await expect(page.locator('html')).toHaveClass(/touch/);
      await touchEditor(page);
      await expect(page.locator('#source')).toHaveAttribute('inputmode', 'none');
    });

    test('a page zoomed in by a pinch keeps its size, and a finger pans it', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      const result = await page.evaluate(() => {
        const app = document.querySelector('.app');
        const before = app.style.getPropertyValue('--app-h');
        // WebKit can't make a Touch, and leaves that half to Chromium.
        const moved = () => {
          if (typeof Touch !== 'function') return null;
          try {
            new Touch({ identifier: 1, target: document.body });
          } catch {
            return null;
          }
          const e = new TouchEvent('touchmove', { cancelable: true, bubbles: true, touches: [new Touch({ identifier: 1, target: document.querySelector('header') })] });
          document.querySelector('header').dispatchEvent(e);
          return e.defaultPrevented;
        };
        const heldBefore = moved();
        Object.defineProperty(visualViewport, 'scale', { configurable: true, get: () => 2 });
        Object.defineProperty(visualViewport, 'height', { configurable: true, get: () => innerHeight / 2 });
        visualViewport.dispatchEvent(new Event('resize'));
        return { before, after: app.style.getPropertyValue('--app-h'), heldBefore, heldZoomed: moved() };
      });
      expect(result.after).toBe(result.before);
      if (result.heldBefore !== null) expect(result).toMatchObject({ heldBefore: true, heldZoomed: false });
    });

    test('a held key repeats, and stops when the finger lifts or slides off; its click doesn\'t act again', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      await touchEditor(page);
      const source = page.locator('#source');
      const length = () => source.evaluate((t) => t.value.length);
      const space = page.locator('#keyboard button[data-id="@space"]');
      const press = (type, init = {}) => space.evaluate((b, [type, init]) => {
        const Kind = type === 'click' ? MouseEvent : PointerEvent;
        b.dispatchEvent(new Kind(type, { bubbles: true, cancelable: true, button: 0, pointerId: 9, isPrimary: true, pointerType: 'touch', ...init }));
      }, [type, init]);
      // Held for a second: one at once, then more after 420 ms.
      let start = await length();
      await press('pointerdown');
      await page.waitForTimeout(1000);
      await press('pointerup');
      const held = (await length()) - start;
      expect(held).toBeGreaterThan(3);
      // Its click, late as it is, doesn't type another.
      await press('click', { detail: 1 });
      await page.waitForTimeout(200);
      expect((await length()) - start).toBe(held);
      // Slid off the key: it stops there, though the finger is still down.
      start = await length();
      await press('pointerdown');
      await page.waitForTimeout(600);
      const box = await space.boundingBox();
      await press('pointermove', { clientX: box.x + box.width / 2, clientY: box.y - 40 });
      const slid = (await length()) - start;
      await page.waitForTimeout(500);
      expect((await length()) - start).toBe(slid);
      await press('pointerup');
    });

    test('a key ends a word the system keyboard is composing before it types', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      await touchEditor(page);
      await tapKey(page, '@system');
      const source = page.locator('#source');
      await source.evaluate((t) => {
        window.blurs = 0;
        t.addEventListener('blur', () => (window.blurs += 1));
        t.dispatchEvent(new CompositionEvent('compositionstart', { data: '' }));
      });
      const before = await source.inputValue();
      await tapKey(page, '=·');
      await expect(source).toHaveValue(`${before}·`);
      await expect(source).toBeFocused();
      // The composition was ended, by focus leaving the field and coming back, once.
      expect(await page.evaluate(() => window.blurs)).toBe(1);
      await tapKey(page, '=π');
      expect(await page.evaluate(() => window.blurs)).toBe(1);
      await expect(source).toHaveValue(`${before}·π`);
      await tapKey(page, '@pencil');
    });

    test('the caret keys and ⌫ go a character at a time as a reader sees one', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      // The steps themselves: an emoji is two code units, a flag four, and é here is e and an accent.
      const text = 'a😀b🇫🇷céd';
      const steps = await page.evaluate(async (text) => {
        const { step } = await import('/web/keyboard.js');
        const on = [0];
        while (on.at(-1) < text.length) on.push(step(text, on.at(-1), 1));
        const back = [text.length];
        while (back.at(-1) > 0) back.push(step(text, back.at(-1), -1));
        return { on, back };
      }, text);
      expect(steps.on).toEqual([0, 1, 3, 4, 8, 9, 11, 12]);
      expect(steps.back).toEqual([12, 11, 9, 8, 4, 3, 1, 0]);
      // And through the keys, in the editor.
      await touchEditor(page);
      const source = page.locator('#source');
      await source.evaluate((t, text) => {
        t.value = text;
        t.setSelectionRange(text.length, text.length);
      }, text);
      const caret = () => source.evaluate((t) => t.selectionStart);
      await tapKey(page, '@left');
      expect(await caret()).toBe(11);
      await tapKey(page, '@left');
      expect(await caret()).toBe(9);
      await tapKey(page, '@back');
      await expect(source).toHaveValue('a😀b🇫🇷éd');
      await tapKey(page, '@back');
      await expect(source).toHaveValue('a😀béd');
      await tapKey(page, '@back');
      await tapKey(page, '@back');
      await expect(source).toHaveValue('aéd');
      await tapKey(page, '@right');
      expect(await caret()).toBe(3);
    });

    test('a hardware keyboard reaches the keys: Esc into them, arrows among them, Enter presses, Esc out', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      await touchEditor(page);
      const source = page.locator('#source');
      const focused = () => page.evaluate(() => document.activeElement?.dataset?.id ?? document.activeElement?.id);
      const before = await source.inputValue();
      await page.keyboard.press('Escape');
      await expect(keyboard(page)).toBeVisible();
      const first = await focused();
      expect(first).toMatch(/^[=@~]/);
      await page.keyboard.press('ArrowDown');
      const second = await focused();
      expect(second).not.toBe(first);
      expect(second).toMatch(/^[=@~]/);
      // The keys' own page: from the words row down into the keys, and along them to 7.
      await page.locator('#keyboard button[data-id="=7"]').focus();
      await page.keyboard.press('Enter');
      await expect(source).toHaveValue(`${before}7`);
      expect(await focused()).toBe('=7');
      await page.keyboard.press('ArrowRight');
      expect(await focused()).toBe('=8');
      await page.keyboard.press('Enter');
      await expect(source).toHaveValue(`${before}78`);
      // The keys stayed up through all of it, and Esc leaves them for the figure.
      await expect(keyboard(page)).toBeVisible();
      await page.keyboard.press('Escape');
      await expect(page.locator('#figure')).toBeFocused();
      await expect(keyboard(page)).toBeHidden();
    });

    test('the menu says what Esc does on a touch screen', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      await page.locator('#figure-more').click();
      const hint = page.locator('#figure-menu .hint');
      await expect(hint).toContainText('goes to the keys');
      await expect(hint.locator('.if-mouse')).toBeHidden();
    });
  });

  test.describe('with a mouse', () => {
    test.beforeEach(({ isMobile }) => {
      test.skip(isMobile, 'desktop projects only');
    });

    test('no keys, the editor as it was, and the figure laid out as it was', async ({ page }) => {
      await openPlayground(page, 'figure=circle');
      await drawnHere(page);
      const source = page.locator('#source');
      await source.click();
      await page.keyboard.type('// typed');
      await expect(source).toHaveValue(/\/\/ typed$/);
      await expect(keyboard(page)).toBeHidden();
      await expect(source).not.toHaveAttribute('inputmode');
      await expect(source).toHaveCSS('font-size', '13px');
      expect(await page.locator('html').getAttribute('class')).toBeFalsy();
      // The figure's own rule, as before the keys: as wide as its column, to the window's height.
      const laid = await page.evaluate(() => {
        const view = document.querySelector('.view');
        const css = getComputedStyle(view);
        const room = view.clientWidth - parseFloat(css.paddingLeft) - parseFloat(css.paddingRight);
        const shape = parseFloat(getComputedStyle(document.getElementById('figure')).getPropertyValue('--shape')) || 1.778;
        const r = document.getElementById('figure').getBoundingClientRect();
        return { width: r.width, expected: Math.min(room, (innerHeight - 220) * shape), app: getComputedStyle(document.querySelector('.app')).position, stage: getComputedStyle(document.querySelector('.stage')).display };
      });
      expect(Math.abs(laid.width - laid.expected)).toBeLessThan(1);
      expect(laid).toMatchObject({ app: 'static', stage: 'contents' });
      await expect(page.locator('#keys-open')).toBeHidden();
      await expect(page.locator('#figure-menu .hint .if-touch')).toBeHidden();
      await expect(page.locator('#figure-menu .hint')).toContainText('Esc leaves the editor');
    });
  });
});

// On a narrow screen the text and the figure's controls take turns under the figure, picked by a
// switch in the bar; with a mouse in a wide window the controls stay under the figure.
test.describe('the text or the controls', () => {
  // A figure with more params than the corpus has, which has five at most: twenty sliders.
  const NAMES = ['a', 'b', 'c', 'd', 'f', 'g', 'h', 'j', 'k', 'm', 'n', 'q', 's', 'u', 'v', 'w', 'z', 'α', 'β', 'γ'];
  const MANY = {
    name: 'many params',
    doc: 'many.pen',
    sheets: ['many.style'],
    files: {
      'many.pen': `doc many\nviewport { x: -2..2, y: -2..2 }\n${NAMES.map((n) => `param ${n} in 0..1`).join('\n')}\ncurve ring = implicit (x, y): x² + y² = (0.5 + (${NAMES.join(' + ')}) / 20)²\n`,
      'many.style': 'style ring { stroke: white; width: 3px }\n',
    },
  };
  const switcher = (page) => page.locator('#panel-switch');
  const tab = (page, name) => page.getByRole('tab', { name });
  const panelShown = (page) => page.evaluate(() => document.querySelector('.app').dataset.panel);
  const inView = (page, selector) => page.locator(selector).evaluate((el) => {
    const r = el.getBoundingClientRect();
    return r.height > 0 && r.top >= -0.5 && r.bottom <= innerHeight + 0.5;
  });
  const stays = (page) => page.evaluate(() => document.documentElement.scrollHeight - innerHeight);
  /** Counts the figure's loads from here on, in `window.loads`. */
  const countLoads = (page) => page.evaluate(() => {
    window.loads = 0;
    window.pencil.figure.addEventListener('load', () => (window.loads += 1));
  });

  test.describe('on a phone', () => {
    test.beforeEach(({ isMobile }) => {
      test.skip(!isMobile, 'phone projects only');
    });

    test('a switch in the bar swaps the text for the controls under the figure, and back', async ({ page }) => {
      await openPlayground(page, 'figure=mobius');
      await drawnHere(page);
      await expect(switcher(page)).toBeVisible();
      // In the one bar, with the figure's menus.
      expect(await switcher(page).evaluate((el) => el.parentElement.tagName)).toBe('HEADER');
      await expect(tab(page, 'Text')).toHaveAttribute('aria-selected', 'true');
      await expect(page.locator('.editor')).toBeVisible();
      await expect(page.locator('#controls-panel')).toBeHidden();
      // The controls aren't under the figure with the text.
      await expect(page.locator('.view .controls')).toHaveCount(0);
      await tab(page, 'Controls').tap();
      await expect(tab(page, 'Controls')).toHaveAttribute('aria-selected', 'true');
      await expect(page.locator('.editor')).toBeHidden();
      await expect(page.locator('#controls-panel')).toBeVisible();
      await expect(page.locator('#controls-panel')).toHaveAttribute('aria-labelledby', 'tab-controls');
      await expect(page.locator('#controls-panel input[type=range]').first()).toBeVisible();
      expect(await inView(page, '#figure')).toBe(true);
      expect(await inView(page, 'header')).toBe(true);
      expect(await stays(page)).toBe(0);
      // A slider still sets the figure.
      const slider = page.locator('#controls-panel input[type=range]').nth(1);
      await slider.focus();
      await page.keyboard.press('ArrowRight');
      await expect(page.locator('#controls-panel .held')).toHaveCount(1);
      await tab(page, 'Text').tap();
      await expect(page.locator('.editor')).toBeVisible();
      await expect(page.locator('#controls-panel')).toBeHidden();
      await expect(page.locator('.editor')).toHaveAttribute('aria-labelledby', 'tab-text');
    });

    test('twenty params scroll in their panel, and the figure and the bar stay put', async ({ page }) => {
      await openPlayground(page, '', `#pen=${await pack(MANY)}`);
      await drawnHere(page);
      await tab(page, 'Controls').tap();
      const panel = page.locator('#controls-panel');
      await expect(panel.locator('#params input[type=range]')).toHaveCount(20);
      const figure = await page.locator('#figure').boundingBox();
      const sizes = await panel.evaluate((el) => [el.scrollHeight, el.clientHeight]);
      expect(sizes[0]).toBeGreaterThan(sizes[1]);
      await panel.evaluate((el) => (el.scrollTop = el.scrollHeight));
      // The last slider is in the panel's box, and in the window.
      const last = await panel.locator('#params input[type=range]').last().boundingBox();
      const box = await panel.boundingBox();
      expect(last.y + last.height).toBeLessThanOrEqual(box.y + box.height + 0.5);
      expect(last.y + last.height).toBeLessThanOrEqual(page.viewportSize().height + 0.5);
      expect(await page.locator('#figure').boundingBox()).toEqual(figure);
      expect(await inView(page, '#figure')).toBe(true);
      expect(await inView(page, 'header')).toBe(true);
      expect(await stays(page)).toBe(0);
    });

    test('the controls put the keys away, and the text comes back with its caret and no keys', async ({ page }) => {
      // A file longer than any phone's window, with the keys up or not.
      await openPlayground(page, 'figure=explainer');
      const source = page.locator('#source');
      await source.tap();
      // The keys up, and the caret brought into view as they came, which takes a frame or two.
      await expect(page.locator('#keyboard')).toBeVisible();
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      await source.evaluate((t) => {
        // Long enough to scroll on any phone, with the keys up or not.
        t.value += '\n//'.repeat(80);
        t.dispatchEvent(new Event('input'));
        t.setSelectionRange(40, 40);
        t.scrollTop = 60;
      });
      const scrolled = await source.evaluate((t) => t.scrollTop);
      // It did scroll, so the check below can tell a kept place from none.
      expect(scrolled).toBeGreaterThan(0);
      await tab(page, 'Controls').tap();
      await expect(page.locator('#keyboard')).toBeHidden();
      await expect(source).not.toBeFocused();
      expect(await page.evaluate(() => document.documentElement.classList.contains('keys-up'))).toBe(false);
      await tab(page, 'Text').tap();
      await expect(source).toBeVisible();
      await expect(source).not.toBeFocused();
      await expect(page.locator('#keyboard')).toBeHidden();
      expect(await source.evaluate((t) => [t.selectionStart, t.selectionEnd])).toEqual([40, 40]);
      expect(await source.evaluate((t) => t.scrollTop)).toBe(scrolled);
      // A touch in the text brings the keys.
      await source.tap();
      await expect(page.locator('#keyboard')).toBeVisible();
    });

    test('a figure with nothing to control has no switch, and the choice holds for the next that has', async ({ page }) => {
      await openPlayground(page, 'figure=mobius');
      await tab(page, 'Controls').tap();
      await countLoads(page);
      await page.locator('#figure-pick').selectOption({ label: '002 strokes, ring' });
      // Ring itself has loaded, not Möbius still.
      await expect.poll(() => page.evaluate(() => window.loads > 0 && window.pencil.files()[0].path)).toBe('corpus/002-strokes/ring.pen');
      await expect(switcher(page)).toBeHidden();
      await expect(page.locator('.editor')).toBeVisible();
      expect(await panelShown(page)).toBe('text');
      // It keeps its place, so the menus beside it don't move.
      expect((await switcher(page).boundingBox()).width).toBeGreaterThan(40);
      await page.locator('#figure-pick').selectOption({ label: '004 bounds' });
      await expect(switcher(page)).toBeVisible();
      await expect(tab(page, 'Controls')).toHaveAttribute('aria-selected', 'true');
      await expect(page.locator('#controls-panel')).toBeVisible();
      // And across a reload, for the visit.
      await page.reload();
      await settled(page);
      await expect(tab(page, 'Controls')).toHaveAttribute('aria-selected', 'true');
    });

    test('typing a param into a figure that had none leaves the text, its focus and its keys where they are', async ({ page }) => {
      await openPlayground(page, 'figure=mobius');
      await tab(page, 'Controls').tap();
      await countLoads(page);
      await page.locator('#figure-pick').selectOption({ label: '002 strokes, ring' });
      await expect.poll(() => page.evaluate(() => window.loads > 0 && window.pencil.files()[0].path)).toBe('corpus/002-strokes/ring.pen');
      const source = page.locator('#source');
      await source.tap();
      await expect(page.locator('#keyboard')).toBeVisible();
      await page.evaluate(() => (window.loads = 0));
      // Typed as the keys type, through the browser's editing, so the keys stay up.
      await source.evaluate((t) => {
        t.setSelectionRange(t.value.length, t.value.length);
        document.execCommand('insertText', false, '\nparam q in 0..1');
      });
      await expect.poll(() => page.evaluate(() => window.loads)).toBeGreaterThan(0);
      await page.waitForTimeout(300);
      await expect(switcher(page)).toBeVisible();
      await expect(tab(page, 'Text')).toHaveAttribute('aria-selected', 'true');
      await expect(page.locator('.editor')).toBeVisible();
      await expect(source).toBeFocused();
      await expect(page.locator('#keyboard')).toBeVisible();
      // Touching the text made it the choice: the next figure opens on it.
      await page.locator('#figure-pick').selectOption({ label: '004 bounds' });
      await expect(switcher(page)).toBeVisible();
      await expect(tab(page, 'Text')).toHaveAttribute('aria-selected', 'true');
    });

    test('a tap in the text before the figure loads keeps the text, though the controls were chosen', async ({ page }) => {
      await openPlayground(page, 'figure=mobius');
      await tab(page, 'Controls').tap();
      await page.goto('about:blank');
      // Slowed, so the tap comes before the figure's first load.
      await page.route('**/corpus/010-mobius/mobius.pen', async (route) => {
        await new Promise((r) => setTimeout(r, 1500));
        await route.continue();
      });
      await page.goto(addressOf('figure=mobius'));
      // The page's script is running, and the figure's files are still on their way.
      await page.waitForFunction(() => 'panel' in document.querySelector('.app').dataset);
      expect(await page.evaluate(() => !!window.pencil)).toBe(false);
      await page.locator('#source').tap();
      await settled(page);
      await drawnHere(page);
      await expect(page.locator('.editor')).toBeVisible();
      await expect(tab(page, 'Text')).toHaveAttribute('aria-selected', 'true');
    });

    test('the text comes back at the place of the file it shows, after a figure changed under the controls', async ({ page }) => {
      await openPlayground(page, 'figure=mobius');
      const source = page.locator('#source');
      await source.evaluate((t) => (t.scrollTop = 200));
      expect(await source.evaluate((t) => t.scrollTop)).toBeGreaterThan(0);
      await tab(page, 'Controls').tap();
      await page.locator('#figure-pick').selectOption({ label: '004 bounds' });
      await expect(page.locator('#tabs .tab.active .pick')).toHaveText(/bounds\.pen/);
      await tab(page, 'Text').tap();
      expect(await source.evaluate((t) => t.scrollTop)).toBe(0);
      // Its own place is kept the same way.
      await source.evaluate((t) => (t.scrollTop = 120));
      const own = await source.evaluate((t) => t.scrollTop);
      expect(own).toBeGreaterThan(0);
      await tab(page, 'Controls').tap();
      await tab(page, 'Text').tap();
      expect(await source.evaluate((t) => t.scrollTop)).toBe(own);
    });

    test('a tab\'s focus goes to the figure when the next figure has nothing to control', async ({ page }) => {
      await openPlayground(page, 'figure=mobius');
      await drawnHere(page);
      await tab(page, 'Controls').focus();
      const still = await pack({ name: 'still', doc: 'still.pen', sheets: [], files: { 'still.pen': 'doc still\nviewport { x: -2..2, y: -2..2 }\ncurve ring = implicit (x, y): x² + y² = 1\n' } });
      await countLoads(page);
      await page.evaluate((link) => (location.hash = `pen=${link}`), still);
      await expect.poll(() => page.evaluate(() => window.loads)).toBeGreaterThan(0);
      await expect(switcher(page)).toBeHidden();
      await expect(page.locator('#figure')).toBeFocused();
    });

    test('an error stays in view on the controls, and going to it shows the text', async ({ page }) => {
      await openPlayground(page, 'figure=mobius');
      await drawnHere(page);
      await typeAtEnd(page, '\n)');
      await expect(page.locator('#errors li')).toHaveCount(1);
      await tab(page, 'Controls').tap();
      expect(await inView(page, '#errors li')).toBe(true);
      const error = await page.locator('#errors li').boundingBox();
      const panel = await page.locator('#controls-panel').boundingBox();
      expect(error.y + error.height).toBeLessThanOrEqual(panel.y + 0.5);
      await page.locator('#errors li').click();
      await expect(tab(page, 'Text')).toHaveAttribute('aria-selected', 'true');
      await expect(page.locator('#source')).toBeFocused();
    });

    test('held sideways, the figure is on the right and the text or the controls on the left', async ({ page }) => {
      await page.setViewportSize({ width: 812, height: 375 });
      await openPlayground(page, 'figure=mobius');
      await drawnHere(page);
      await expect(switcher(page)).toBeVisible();
      await tab(page, 'Controls').tap();
      const [figure, panel] = await Promise.all([page.locator('#figure').boundingBox(), page.locator('#controls-panel').boundingBox()]);
      expect(panel.x + panel.width).toBeLessThanOrEqual(figure.x + 0.5);
      expect(panel.y + panel.height).toBeLessThanOrEqual(375.5);
      expect(await inView(page, '#figure')).toBe(true);
      expect(await stays(page)).toBe(0);
      // And the text takes the same column.
      await tab(page, 'Text').tap();
      await expect(page.locator('#controls-panel')).toBeHidden();
      const text = await page.locator('.editor').boundingBox();
      expect(text.x).toBeLessThan(1);
      expect(text.x + text.width).toBeLessThanOrEqual(figure.x + 0.5);
      expect(text.y + text.height).toBeLessThanOrEqual(375.5);
    });
  });

  test('a wide touch screen made short, as a keyboard does, keeps the text and its focus', async ({ page, isMobile }) => {
    test.skip(!isMobile, 'a touch screen');
    await page.setViewportSize({ width: 1280, height: 800 });
    await openPlayground(page, 'figure=mobius');
    // The controls were the choice before, on this visit.
    await page.evaluate(() => sessionStorage.setItem('pencil-playground:panel', 'controls'));
    await page.reload();
    await settled(page);
    await drawnHere(page);
    await expect(switcher(page)).toBeHidden();
    const source = page.locator('#source');
    await source.tap();
    await expect(source).toBeFocused();
    await page.setViewportSize({ width: 1280, height: 450 });
    await expect(switcher(page)).toBeVisible();
    await expect(page.locator('.editor')).toBeVisible();
    await expect(source).toBeFocused();
    await expect(tab(page, 'Text')).toHaveAttribute('aria-selected', 'true');
    // And back, with nothing moved.
    await page.setViewportSize({ width: 1280, height: 800 });
    await expect(source).toBeFocused();
  });

  test('the switch is worked with the arrow keys, as tabs are', async ({ page, isMobile }) => {
    test.skip(isMobile, 'a phone has no arrow keys; a narrow window does');
    await page.setViewportSize({ width: 700, height: 800 });
    await openPlayground(page, 'figure=mobius');
    await expect(switcher(page)).toBeVisible();
    await tab(page, 'Text').focus();
    await page.keyboard.press('ArrowRight');
    await expect(tab(page, 'Controls')).toBeFocused();
    await expect(tab(page, 'Controls')).toHaveAttribute('aria-selected', 'true');
    await expect(page.locator('#controls-panel')).toBeVisible();
    // One tab stop: the selected tab.
    expect(await tab(page, 'Text').getAttribute('tabindex')).toBe('-1');
    await expect(page.locator('.editor')).toBeHidden();
    const showing = async (name) => {
      await expect(tab(page, name)).toBeFocused();
      await expect(tab(page, name)).toHaveAttribute('aria-selected', 'true');
      await expect(page.locator('.editor')).toBeVisible({ visible: name === 'Text' });
      await expect(page.locator('#controls-panel')).toBeVisible({ visible: name === 'Controls' });
    };
    await page.keyboard.press('ArrowRight');
    await showing('Text');
    await page.keyboard.press('End');
    await showing('Controls');
    await page.keyboard.press('ArrowLeft');
    await showing('Text');
    await page.keyboard.press('ArrowLeft');
    await showing('Controls');
    await page.keyboard.press('Home');
    await showing('Text');
  });

  test('a control keeps focus when a resize moves it between its panel and the figure', async ({ page, isMobile }) => {
    test.skip(isMobile, 'a window resized with a mouse');
    await page.setViewportSize({ width: 700, height: 800 });
    await openPlayground(page, 'figure=mobius');
    await drawnHere(page);
    await tab(page, 'Controls').click();
    const slider = page.locator('#param-b');
    await slider.focus();
    await page.setViewportSize({ width: 1633, height: 919 });
    await expect(page.locator('.view .controls')).toBeVisible();
    await expect(slider).toBeFocused();
    await page.setViewportSize({ width: 700, height: 800 });
    await expect(page.locator('#controls-panel .controls')).toHaveCount(1);
    await expect(slider).toBeFocused();
    // From the narrow window to the phone's order and back, the arrangement moves the panel too.
    await page.setViewportSize({ width: 375, height: 800 });
    await expect(slider).toBeFocused();
  });

  test('a recompile keeps the sliders it had, so one being dragged or focused stays', async ({ page }) => {
    await openPlayground(page, 'figure=mobius');
    await drawnHere(page);
    await page.evaluate(() => (document.getElementById('param-b').dataset.mine = 'yes'));
    const compiled = await page.locator('#status').textContent();
    await page.evaluate(() => window.pencil.compile());
    await expect.poll(() => page.locator('#status').textContent()).not.toBe(compiled);
    expect(await page.evaluate(() => document.getElementById('param-b').dataset.mine)).toBe('yes');
    // A figure with other params has its own.
    await page.locator('#figure-pick').selectOption({ label: '004 bounds' });
    await expect(page.locator('#param-b')).toHaveCount(0);
  });

  test('a wide window has no switch, and the controls under the figure as before', async ({ page, isMobile }) => {
    test.skip(isMobile, 'desktop projects only');
    await openPlayground(page, 'figure=mobius');
    await drawnHere(page);
    await expect(switcher(page)).toBeHidden();
    await expect(page.locator('.view .controls')).toBeVisible();
    await expect(page.locator('#controls-panel')).toBeHidden();
    await expect(page.locator('.editor')).toBeVisible();
    expect(await page.locator('.editor').getAttribute('role')).toBeNull();
    expect(await panelShown(page)).toBe('');
    // Narrowed and widened again, the controls go to their panel and come back under the figure.
    await page.setViewportSize({ width: 600, height: 800 });
    await expect(page.locator('#controls-panel .controls')).toHaveCount(1);
    await page.setViewportSize({ width: 1633, height: 919 });
    await expect(page.locator('.view .controls')).toBeVisible();
    await expect(switcher(page)).toBeHidden();
  });
});
