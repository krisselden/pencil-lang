import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { pack } from '../src/pack.js';

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
        figureTop: figure.top,
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
    expect(await order()).toEqual(['header', 'editor', 'view']);
    const source = page.locator('#source');
    await source.focus();
    await source.evaluate((t) => t.setSelectionRange(12, 20));
    await page.evaluate(() => {
      window.loads = 0;
      document.getElementById('figure').addEventListener('load', () => (window.loads += 1));
    });

    await page.setViewportSize({ width: 375, height: 812 });
    await expect.poll(order).toEqual(['header', 'view', 'editor']);
    await expect(source).toBeFocused();
    expect(await source.evaluate((t) => [t.selectionStart, t.selectionEnd])).toEqual([12, 20]);
    await expect(page.locator('#figure-more')).toBeVisible();

    await page.setViewportSize({ width: 1633, height: 919 });
    await expect.poll(order).toEqual(['header', 'editor', 'view']);
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
