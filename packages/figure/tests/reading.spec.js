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
});
