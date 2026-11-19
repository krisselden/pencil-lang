// The playground's reading line: how Pencil read an expression, written back out (web/reading.js).
import { test, expect } from '@playwright/test';

test.describe('the reading line', () => {
  test('writes `±` and `∓` as they were typed, in both forms', async ({ page }) => {
    await page.goto('/web/ast.html');
    const read = (mathjson) =>
      page.evaluate(async (v) => {
        const reading = await import('/web/reading.js');
        await reading.ready;
        return reading.infix(v);
      }, mathjson);
    expect(await read(['PlusMinus', 'a', 'b'])).toBe('a ± b');
    expect(await read(['MinusPlus', 'a', ['Multiply', 'b', 'c']])).toBe('a ∓ b·c');
    expect(await read(['PlusMinus', ['Cosh', 't']])).toBe('±cosh(t)');
    expect(await read(['Tuple', ['PlusMinus', 'a'], ['MinusPlus', 'b']])).toBe('(±a, ∓b)');
    // Parentheses where they decide the order, as for + and -.
    expect(await read(['Multiply', 2, ['PlusMinus', 1, 'x']])).toBe('2·(1 ± x)');
    expect(await read(['Power', ['MinusPlus', 'x'], 2])).toBe('(∓x)^2');
  });

  test('reads a range written `med ± dev` as it was written', async ({ page }) => {
    await page.goto('/web/ast.html');
    // What the line shows with the caret on `word` in `text`.
    const at = (text, word) =>
      page.evaluate(async ([text, word]) => {
        const reading = await import('/web/reading.js');
        await reading.ready;
        const found = reading.readingAt(text, text.lastIndexOf(word) + 1);
        return found && { written: found.written, read: found.read };
      }, [text, word]);
    const same = (text) => ({ written: text, read: text });
    expect(await at('param a = 1 in 2 ± 1 step 0.25', '±')).toEqual(same('2 ± 1'));
    expect(await at('dots d = (k, ±1) for k in 2 ± 1', '2')).toEqual(same('2 ± 1'));
    expect(await at('curve c = parametric (t): (t, 0) for t in 0 ± π', 'π')).toEqual(same('0 ± π'));
    expect(await at('viewport { x: 0 ± 2, y: -2..2 }', '0')).toEqual(same('0 ± 2'));
    // A range's ends are read one at a time.
    expect(await at('param a in 0..q + 1', 'q')).toEqual(same('q + 1'));
  });
});
