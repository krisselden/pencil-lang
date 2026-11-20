// Labels in the package's own fonts, and the mathematics in them: a page sets what the core laid
// out, in the files the `pencil` command has built in, so a poster and the live figure agree.
import { test, expect } from '@playwright/test';
import { open, drawn, onFigure } from './helpers.js';

// A thousand times the width of each in ems, which `crates/pencil-cli/src/text.rs` holds the
// command to (`the_widths_a_page_is_held_to`): the same numbers from the same fonts.
const ARG_Z = 2006;
const PENCIL = 2492;
const TURN = 2798;
const SIN = 1203;

/** Every font the figure's text needs is in, the math font included. */
async function ready(page) {
  await open(page, 'formulas');
  await drawn(page);
  await page.evaluate(() => Promise.all([...document.fonts].map((face) => face.load())));
  await expect.poll(() => onFigure(page, (f) => f.shadowRoot.querySelectorAll('.label.laid').length)).toBeGreaterThan(0);
}

/** A label's pieces as the page set them: each run and rule with its box, in CSS pixels. */
const pieces = (page, id) =>
  onFigure(
    page,
    (f, id) => {
      const label = f.shadowRoot.querySelector(`.label[data-label="${id}"]`);
      if (!label) return null;
      const box = (el) => {
        const r = el.getBoundingClientRect();
        return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height };
      };
      const style = getComputedStyle(label);
      return {
        text: label.textContent,
        family: style.fontFamily,
        size: parseFloat(style.fontSize),
        box: box(label),
        runs: [...label.querySelectorAll('.run')].map((run) => ({
          text: run.textContent,
          formula: run.classList.contains('formula'),
          italic: getComputedStyle(run).fontStyle === 'italic',
          size: parseFloat(getComputedStyle(run).fontSize),
          stretched: run.style.transform !== '',
          ...box(run),
        })),
        rules: [...label.querySelectorAll('.rule')].map(box),
      };
    },
    id,
  );

test.describe('labels', () => {
  test('are set in the package fonts, as wide as the command sets them', async ({ page }) => {
    await ready(page);
    const loaded = await page.evaluate(() => [...document.fonts].filter((f) => f.status === 'loaded').map((f) => f.family.replaceAll('"', '')));
    expect(loaded).toEqual(expect.arrayContaining(['Pencil Sans', 'Pencil Serif', 'Pencil Math']));

    const plain = await pieces(page, 'plain');
    expect(plain.family).toContain('Pencil Sans');
    // Within a fiftieth: a browser may round each glyph's advance to its own grid.
    expect((plain.box.width / plain.size) * 1000).toBeGreaterThan(PENCIL * 0.98);
    expect((plain.box.width / plain.size) * 1000).toBeLessThan(PENCIL * 1.02);

    const note = await pieces(page, 'note');
    expect(note.family).toContain('Pencil Serif');
    expect((note.box.width / note.size) * 1000).toBeGreaterThan(ARG_Z * 0.98);
    expect((note.box.width / note.size) * 1000).toBeLessThan(ARG_Z * 1.02);
  });

  test('show a superscript, a subscript and a fraction', async ({ page }) => {
    await ready(page);
    // a₁ ≤ a₂ ≠ aₙ²: scripts are smaller, a subscript below its letter and a superscript above.
    const order = await pieces(page, 'order');
    expect(order.text).not.toContain('$');
    const letter = order.runs.find((r) => r.text === '𝑎');
    const one = order.runs.find((r) => r.text === '1');
    const two = order.runs.findLast((r) => r.text === '2' && r.size < letter.size);
    expect(one.size).toBeLessThan(letter.size);
    expect(one.bottom).toBeGreaterThan(letter.bottom);
    expect(two.top).toBeLessThan(letter.top);
    expect(letter.formula && !letter.italic).toBe(true);

    // area = x²/2: words in the label's own italic, then a numerator, a bar and a denominator.
    const area = await pieces(page, 'area');
    const words = area.runs.find((r) => r.text === 'area = ');
    expect(words.formula).toBe(false);
    expect(words.italic).toBe(true);
    expect(area.rules).toHaveLength(1);
    const [bar] = area.rules;
    const x = area.runs.find((r) => r.text === '𝑥');
    const below = area.runs.find((r) => r.text === '2' && r.top > bar.top);
    expect(bar.height).toBeGreaterThan(0.5);
    // The line boxes overlap the bar, so it's their middles that are either side of it.
    expect((x.top + x.bottom) / 2).toBeLessThan(bar.top);
    expect((below.top + below.bottom) / 2).toBeGreaterThan(bar.bottom);
    expect(Math.abs((below.left + below.right) / 2 - (bar.left + bar.right) / 2)).toBeLessThan(0.75);
    expect(bar.left).toBeGreaterThan(words.right - 1);

    // A value in a span is laid out with it: z to the n, with n at 3.
    const power = await pieces(page, 'power');
    expect(power.runs.some((r) => r.text === '3' && r.size < power.size)).toBe(true);
  });

  test('lay out a value as mathematics in a sans label, beside its sans words', async ({ page }) => {
    await ready(page);
    // `turn = e^(2πi/3)`: the words in the label's own sans, the value in the math font.
    const turn = await pieces(page, 'v');
    expect(turn.family).toContain('Pencil Sans');
    expect(turn.text).not.toContain('$');
    const words = turn.runs.find((r) => r.text === 'turn = ');
    expect(words.formula).toBe(false);
    expect(words.italic).toBe(false);
    expect((words.width / words.size) * 1000).toBeGreaterThan(TURN * 0.98);
    expect((words.width / words.size) * 1000).toBeLessThan(TURN * 1.02);
    const e = turn.runs.find((r) => r.text === '𝑒');
    expect(e.formula).toBe(true);
    expect(e.left).toBeGreaterThan(words.right - 1);
    // Its exponent is a real superscript: smaller, and above the line.
    const power = turn.runs.find((r) => r.formula && r.size < e.size);
    expect(power.top).toBeLessThan(e.top);
  });

  test('set a run as wide as the core measured it', async ({ page }) => {
    await ready(page);
    const trig = await pieces(page, 'trig');
    const sin = trig.runs.find((r) => r.text === 'sin');
    expect((sin.width / sin.size) * 1000).toBeGreaterThan(SIN * 0.98);
    expect((sin.width / sin.size) * 1000).toBeLessThan(SIN * 1.02);
    // What follows starts a thin space after it ends, where the core put it: three eighteenths of
    // an em, less what the superscript between them takes.
    const theta = trig.runs.find((r) => r.text === '𝜃');
    expect(theta.left).toBeGreaterThan(sin.right);
  });

  test('draw a root and tall brackets from the glyphs, made taller', async ({ page }) => {
    await ready(page);
    const root = await pieces(page, 'root');
    const sign = root.runs.find((r) => r.text === '√');
    const [bar] = root.rules;
    const last = root.runs.at(-1);
    // The bar starts at the sign's ink and covers what's under it.
    expect(bar.left).toBeGreaterThan(sign.left);
    expect(bar.left).toBeLessThan(sign.right + sign.size * 0.1);
    expect(bar.right).toBeGreaterThan(last.right - 0.5);

    const quarter = await pieces(page, 'quarter');
    const open = quarter.runs.find((r) => r.text === '(');
    expect(open.stretched).toBe(true);
    expect(open.height).toBeGreaterThan(open.size * 1.1);
  });
});
