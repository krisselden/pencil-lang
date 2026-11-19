// What the Pencil keys type: their pages, the symbol strip that rides over the system keyboard,
// and the words a suggestion can be. The words are from docs/language.md, and the functions come
// from the core itself at start (`functions()` in the host), so a built-in added later shows up
// here without an edit. Counts in the comments are from the corpus and lab/explainer, all .pen and
// .style files (lab/keyboard/README.md). The playground and the keyboard lab both load this file.

/** A key is a character it types, or `@name` for one that does something. An object says more:
 * `t` is what it shows, `ins` what it types, `name` what a screen reader says, `w` how wide. */

// Numbers and operators, arranged as Desmos arranges its keypad: what an expression needs on the
// left, the digits in the middle with an operator at the end of each row, and the editing keys on
// the right. Pencil's operators are its own: `/`, `·` and `-`, where Desmos shows ÷, × and −.
const NUMBERS = [
  ['x', 'y', 't', '²', { gap: 0.25 }, '7', '8', '9', '/', { gap: 0.25 }, '@back'],
  ['(', ')', '^', ',', { gap: 0.25 }, '4', '5', '6', '·', { gap: 0.25 }, '@left'],
  ['{', '}', ':', '"', { gap: 0.25 }, '1', '2', '3', '-', { gap: 0.25 }, '@right'],
  ['π', '_', ';', '..', { gap: 0.25 }, '0', '.', '=', '+', { gap: 0.25 }, '@enter'],
];

// Letters, with the punctuation a line of words needs on the row under them, so `style c { fill:
// ink; }` and `let r = 1` can be typed without leaving the page.
const LETTERS = [
  ['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p'],
  ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', '@back'],
  ['@shift', 'z', 'x', 'c', 'v', 'b', 'n', 'm', '@left', '@right'],
  ['{', '}', '(', ')', '_', '.', ',', ':', ';', '=', '@enter'],
];

// Greek, the full lowercase set GeoGebra's keyboard has but omicron, which is an o. Shifted, the
// capitals that aren't Latin letters, the constants' own glyphs and the number sets. The last row
// is what only a label's text can hold: an expression has no →, ≤ or ∈ (docs/language.md, Typing
// the symbols), so they're kept to one row.
const GREEK = [
  ['α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', { gap: 0.25 }, '@back'],
  ['ι', 'κ', 'λ', 'μ', 'ν', 'ξ', 'π', 'ρ', { gap: 0.25 }, '@left'],
  ['σ', 'τ', 'υ', 'φ', 'χ', 'ψ', 'ω', '@shift', { gap: 0.25 }, '@right'],
  ['→', '↦', '≤', '≥', '≠', '±', '∞', '°', { gap: 0.25 }, '@enter'],
];
const GREEK_SHIFTED = [
  ['Γ', 'Δ', 'Θ', 'Λ', 'Ξ', 'Π', 'Σ', 'Υ', { gap: 0.25 }, '@back'],
  ['Φ', 'Ψ', 'Ω', 'ℯ', 'ⅈ', '∂', '∇', '√', { gap: 0.25 }, '@left'],
  ['ℝ', 'ℂ', 'ℤ', 'ℕ', 'ℚ', '∑', '∫', '@shift', { gap: 0.25 }, '@right'],
  ['∈', '∉', '⊂', '∪', '∩', '×', '÷', '≈', { gap: 0.25 }, '@enter'],
];

/** The pages, in the order the bottom row offers them. `words` is filled in from WORDS: it's a
 * list that scrolls, with the editing keys beside it. */
export const PAGES = [
  { id: 'numbers', label: '123', name: 'Numbers and operators', rows: NUMBERS },
  { id: 'letters', label: 'abc', name: 'Letters', rows: LETTERS },
  { id: 'words', label: 'f(x)', name: 'Functions and Pencil’s words', rows: null, side: ['@back', '@left', '@right', '@enter'] },
  { id: 'greek', label: 'αβγ', name: 'Greek and symbols', rows: GREEK, shifted: GREEK_SHIFTED },
];

/** Under every page: the pages, a space bar, and the keys that leave the Pencil keys. */
export const BOTTOM = ['@page:numbers', '@page:letters', '@page:words', '@page:greek', { action: 'space', w: 2 }, { action: 'system', w: 1.5 }, '@hide'];

// What the strip over the system keyboard holds, most used first. The system keyboard has letters
// and digits on its first two pages; what it lacks, or hides two pages down, is here: `·` (317 in
// the corpus) and `π` (73) are on none of its pages, `²` (40) needs a long press of 2 on iOS, and
// `{ } = _ ^` are on the third page of iOS's. Greek follows, by use, then a label's symbols.
export const STRIP = [
  '·', '²', 'π', '(', ')', '{', '}', '=', ':', ';', ',', '.', '..', '_', '"', '+', '-', '/', '^', '³',
  'θ', 'τ', 'σ', 'ω', 'α', 'β', 'φ', 'μ', 'ν', 'λ', 'ρ', 'ε', 'δ',
  '→', '↦', '≤', '≥', '≠', '±', '∞', '°',
];

/** What a screen reader says for a key that's a character, where the character isn't a word. */
const SPOKEN = {
  '·': 'times', '²': 'squared', '³': 'cubed', '^': 'power', '(': 'open parenthesis', ')': 'close parenthesis',
  '{': 'open brace', '}': 'close brace', '"': 'quote', ',': 'comma', ':': 'colon', ';': 'semicolon',
  '=': 'equals', '_': 'underscore', '-': 'minus', '+': 'plus', '/': 'divide', '.': 'point', '..': 'range, two dots',
  'π': 'pi', 'ℯ': 'e, Euler’s number', 'ⅈ': 'i, the imaginary unit', '√': 'root',
  '→': 'arrow, in a label', '↦': 'maps to, in a label', '≤': 'at most, in a label', '≥': 'at least, in a label',
  '≠': 'not equal, in a label', '±': 'plus or minus', '∞': 'infinity, in a label', '°': 'degree, in a label',
  '∈': 'in, in a label', '∉': 'not in, in a label', '⊂': 'subset, in a label', '∪': 'union, in a label', '∩': 'intersection, in a label',
  '×': 'times sign, in a label', '÷': 'divided by, in a label', '≈': 'about equal, in a label', '∂': 'partial', '∇': 'nabla',
  '∑': 'sum', '∫': 'integral', 'ℝ': 'the reals', 'ℂ': 'the complex numbers', 'ℤ': 'the integers', 'ℕ': 'the naturals', 'ℚ': 'the rationals',
};

/** Names the core gives the Greek letters (`symbols()`), so a key says "theta", not "θ". */
let greekNames = new Map();
export function nameGreek(pairs) {
  greekNames = new Map(pairs.map(([name, letter]) => [letter, name]));
}

const ACTIONS = {
  back: { t: '⌫', name: 'Delete', repeat: true },
  left: { t: '‹', name: 'Caret left', repeat: true },
  right: { t: '›', name: 'Caret right', repeat: true },
  enter: { t: '↵', name: 'New line' },
  space: { t: 'space', name: 'Space', repeat: true },
  shift: { t: '⇧', name: 'Shift' },
  system: { t: 'system', name: 'Use the system keyboard', click: true },
  hide: { t: '▾', name: 'Hide the keys', click: true },
};

/** A key's full description from what a row says. */
export function key(spec) {
  if (typeof spec === 'object') {
    if (spec.gap !== undefined || spec.ins !== undefined) return { w: 1, ...spec };
    return { w: 1, ...key(`@${spec.action}`), ...spec };
  }
  if (spec.startsWith('@page:')) {
    const page = PAGES.find((p) => p.id === spec.slice(6));
    return { action: 'page', page: page.id, t: page.label, name: page.name, w: 1, click: true };
  }
  if (spec.startsWith('@')) return { action: spec.slice(1), w: 1, ...ACTIONS[spec.slice(1)] };
  return { ins: spec, t: spec, name: SPOKEN[spec] ?? greekNames.get(spec) ?? spec, w: 1 };
}

// ---- Words ----

/** What starts a statement in a `.pen` (docs/language.md, the Statement table). */
export const STATEMENTS = [
  'let', 'style', 'curve', 'label', 'dot', 'fn', 'param', 'beat', 'segment', 'use', 'pause', 'viewport', 'doc',
  'duration', 'circles', 'curves', 'dots', 'arrow', 'region', 'polygon', 'path', 'rightangle', 'bind', 'solve',
  'handle', 'group', 'panel', 'place', 'expression', 'text', 'outline', 'poster',
];

/** Words that come inside a statement. */
export const CLAUSES = [
  'in', 'for', 'at', 'tag', 'away', 'with', 'offset', 'step', 'from', 'through', 'as', 'show', 'drop', 'focus',
  'sets', 'if', 'by', 'hang', 'end', 'size', 'implicit', 'parametric', 'sdf', 'fit', 'pad', 'yaw', 'empty',
];

/** Names that are always there. */
export const CONSTANTS = ['time', 'pi', 'e', 'i', 'pointer.x', 'pointer.y', 'GoldenRatio', 'Degrees', 'white', 'black'];

/** A sheet's properties (docs/language.md, Styles), most used first. */
export const PROPERTIES = [
  'width', 'stroke', 'fill', 'opacity', 'radius', 'size', 'dash', 'gap', 'fill_opacity', 'trim', 'show', 'font',
  'head', 'halo', 'place', 'text', 'distance', 'digits', 'format', 'leader', 'leader_gap', 'cap', 'draw', 'blend',
  'tone', 'ticks', 'tick', 'head_span', 'height', 'em', 'columns', 'title', 'pad',
];

/** What a property's value can read, besides names. */
export const STYLE_VALUES = [
  'self.dist', 'self.t', 'self.life', 'self.focus', 'self.age', 'self.pixel', 'self.curvature', 'self.z', 'px', 'em', 'none',
  'round', 'butt', 'field', 'line', 'add', 'linear', 'tanh', 'math', 'sans', 'settle', 'complex', 'polar', 'caption', 'name', 'expr', 'value',
];

/** What a selector can name besides the document's marks. */
export const SELECTORS = [
  'figure', 'paper', 'curve', 'circle', 'line', 'ray', 'segment', 'dot', 'label', 'arrow', 'region', 'dots', 'circles',
  'curves', 'decoration', 'term', 'outline', 'caption', 'title', 'formula', 'panels', 'panel',
];

/** The functions, from the core when it's ready; this is what it said on 2026-11-18, for a start. */
export let FUNCTIONS = [
  'sin', 'cos', 'sqrt', 'exp', 'abs', 'mix', 'smoothstep', 'snap', 'intersect', 'circle', 'line', 'ray', 'midpoint',
  'mirror', 'oklch', 'rgb', 'hue', 'arg', 'real', 'imag', 'conj', 'ln', 'tan', 'atan2', 'min', 'max', 'clamp', 'mod',
  'floor', 'ceil', 'round', 'dist', 'length', 'dot', 'exists', 'otherwise', 'reveal', 'timeline', 'perpendicular', 'flip',
];
export function knowFunctions(names) {
  // The corpus's order first, since it's the order they're reached for; the rest after.
  const known = new Set(names);
  FUNCTIONS = [...FUNCTIONS.filter((f) => known.has(f)), ...names.filter((f) => !FUNCTIONS.includes(f))];
}
