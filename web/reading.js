// How Pencil read an expression, written back out with nothing implicit: products as `·`, names
// as the document knows them (`x1` is `x_1`), and parentheses where they decide the order. A page
// shows it beside what was typed, so `2πr` visibly reads `2·π·r` and a slip shows before it
// becomes a puzzle.
import { ready as init, inspect, functions, is_latex, from_latex } from '../packages/figure/src/host.js';

// Each built-in's head, back to how it's written: the first spelling, as `acos` for `Arccos`.
const WRITTEN = new Map();
export const ready = init().then(() => {
  for (const [written, head] of JSON.parse(functions())) if (!WRITTEN.has(head)) WRITTEN.set(head, written);
});

// Words that can follow an expression on its line, as the parser has them, and the punctuation
// an expression can start after or end before.
const WORDS = new Set(['as', 'at', 'away', 'by', 'drop', 'focus', 'for', 'from', 'hang', 'if', 'in',
  'offset', 'sets', 'show', 'size', 'step', 'tag', 'through', 'with']);
// And the words that start statements, which are never part of an expression either.
const STATEMENTS = new Set(['arrow', 'beat', 'bind', 'circles', 'curve', 'curves', 'doc', 'dot', 'dots',
  'duration', 'expression', 'fn', 'group', 'handle', 'implicit', 'label', 'let', 'outline', 'panel',
  'param', 'parametric', 'path', 'pause', 'place', 'polygon', 'region', 'rightangle', 'sdf', 'segment',
  'solve', 'style', 'text', 'use', 'viewport']);
const BEFORE = new Set(['=', ':', ',', '(', '{', '..', ';']);
const AFTER = new Set([',', ')', '}', '..', ';']);

const PRECEDENCE = { Add: 1, Subtract: 1, Multiply: 2, Divide: 2, Negate: 3, Power: 4 };

/** MathJSON as Pencil text, with every product and grouping written out. */
export function infix(v, outer = 0) {
  if (!Array.isArray(v)) return String(v);
  const [head, ...args] = v;
  const [a, b] = args;
  let text;
  switch (head) {
    case 'Add': text = `${infix(a, 1)} + ${infix(b, 2)}`; break;
    case 'Subtract': text = `${infix(a, 1)} - ${infix(b, 2)}`; break;
    case 'Multiply': text = `${infix(a, 2)}·${infix(b, 3)}`; break;
    case 'Divide': text = `${infix(a, 2)}/${infix(b, 3)}`; break;
    case 'Negate': text = `-${infix(a, 4)}`; break;
    case 'Power': text = `${infix(a, 5)}^${infix(b, 4)}`; break;
    case 'Tuple': return `(${args.map((x) => infix(x)).join(', ')})`;
    default: return `${WRITTEN.get(head) ?? head}(${args.map((x) => infix(x)).join(', ')})`;
  }
  return (PRECEDENCE[head] ?? 5) < outer ? `(${text})` : text;
}

/**
 * The expression around a caret in `text`, as `{ written, read, mathjson }`, or null where the
 * caret isn't in one. It takes the longest stretch of the caret's line that reads as one
 * expression, starting and ending where an expression can.
 */
export function readingAt(text, caret) {
  const start = text.lastIndexOf('\n', caret - 1) + 1;
  const end = text.indexOf('\n', caret) < 0 ? text.length : text.indexOf('\n', caret);
  const line = text.slice(start, end);
  const chars = Array.from(line);
  const seen = JSON.parse(inspect(line));
  if (!Array.isArray(seen.tokens) || !seen.tokens.length) return null;
  const toks = seen.tokens;
  // Where each token starts and ends, in characters: up to where the next one starts.
  const from = toks.map((t) => t.col - 1);
  const to = toks.map((_, i) => {
    let stop = i + 1 < toks.length ? from[i + 1] : chars.length;
    while (stop > from[i] && /\s/.test(chars[stop - 1])) stop--;
    return stop;
  });
  const at = Array.from(text.slice(start, caret)).length;
  const here = toks.findLastIndex((_, i) => from[i] <= at);
  if (here < 0) return null;
  const word = (i) => toks[i].kind === 'name' && WORDS.has(toks[i].text);
  const keyword = (i) => word(i) || (toks[i].kind === 'name' && STATEMENTS.has(toks[i].text));
  const opens = [], closes = [];
  for (let i = 0; i <= here; i++) {
    if (i === 0 || BEFORE.has(toks[i - 1].text) || keyword(i - 1)) opens.push(i);
  }
  for (let j = here; j < toks.length; j++) {
    if (j === toks.length - 1 || AFTER.has(toks[j + 1].text) || keyword(j + 1)) closes.push(j);
  }
  for (const i of opens) {
    for (const j of closes.slice().reverse()) {
      if (toks.slice(i, j + 1).some((_, k) => keyword(i + k))) continue;
      const written = chars.slice(from[i], to[j]).join('');
      const one = JSON.parse(inspect(written));
      if (one.expression && !one.expression.error) {
        return { written, read: infix(one.expression), mathjson: one.mathjson };
      }
    }
  }
  return null;
}

// The functions a document defines, so a pasted `f\left(x\right)` calls `f` rather than
// multiplying by it.
const defined = (text) => [...text.matchAll(/^\s*fn\s+([\p{L}_][\p{L}\p{N}_]*)/gmu)].map((m) => m[1]);

/**
 * Reads LaTeX pasted into `textarea`, as Desmos and MathLive copy it, as Pencil text, through the
 * browser's own editing so undo takes it back. `note(text)` says what happened: what the LaTeX
 * became, or why it was pasted as written. Anything that isn't LaTeX pastes as usual.
 */
export function pasteLatex(textarea, note) {
  textarea.addEventListener('paste', (e) => {
    const data = e.clipboardData;
    const latex = data?.getData('application/x-latex') || data?.getData('text/plain');
    if (!latex || !is_latex(latex)) return;
    let text;
    try {
      text = from_latex(latex, defined(textarea.value));
    } catch (error) {
      note(`Pasted as written. As LaTeX, line ${error.line}, column ${error.col}: ${error.text ?? error.message}`);
      return;
    }
    e.preventDefault();
    document.execCommand('insertText', false, text);
    note(`Pasted LaTeX as ${text}`);
  });
}
