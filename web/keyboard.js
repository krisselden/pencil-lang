// The Pencil keys: keys of Pencil's own under an editor on a touch screen, with a row of words that
// finish what's being typed, and the system keyboard one key away. With a mouse, the row of words
// and a row of symbols are under the editor, and the keys are a palette one key away. The
// playground uses them for its editor, and lab/keyboard is the bench they were tried on, which
// loads this same file.
//
// They type through the browser's own editing (`execCommand`), so an undo takes back what they
// typed (history.js), and the textarea has `inputmode="none"` while they're up on a touch screen,
// so the system keyboard stays down. A hardware keyboard still types into it. Pressing a key never
// takes focus from the editor.
import { ready as hostReady, functions, symbols } from '@pencil-lang/figure/host';
import {
  PAGES, BOTTOM, STRIP, key, nameGreek, knowFunctions, FUNCTIONS, STATEMENTS, CLAUSES, CONSTANTS,
  PROPERTIES, STYLE_VALUES, SELECTORS,
} from './keys.js';

// The core's own lists: its functions, and the names of the Greek letters for the keys to say.
const known = hostReady().then(() => {
  try {
    knowFunctions(JSON.parse(functions()).map(([written]) => written).filter((w) => /^[a-z][a-z0-9_]*$/i.test(w)));
    nameGreek(JSON.parse(symbols()).names.filter(([, s]) => /\p{Script=Greek}/u.test(s)).reverse());
  } catch {}
});

// ---- Suggestions ----

/** What's being typed at the caret, and what kind of word goes there: a statement, a selector, a
 * property, a value or an expression. Nothing in a comment or a label's text, where the words are
 * the writer's own. */
export function context(text, at) {
  const line = text.slice(text.lastIndexOf('\n', at - 1) + 1, at);
  if (/\/\//.test(line.replace(/"[^"]*"/g, '""'))) return null;
  if ((line.match(/"/g) ?? []).length % 2) return null;
  const prefix = /(?:[\p{L}_][\p{L}\p{Nd}_₀-₉]*(?:\.[\p{L}_]*)?)?$/u.exec(line)[0];
  const before = line.slice(0, line.length - prefix.length);
  const open = text.lastIndexOf('{', at - 1);
  const ruleLine = text.slice(text.lastIndexOf('\n', open - 1) + 1, open);
  if (open > text.lastIndexOf('}', at - 1) && /^\s*style\b/.test(ruleLine)) {
    const since = text.slice(Math.max(open, text.lastIndexOf(';', at - 1)) + 1, at - prefix.length);
    return { prefix, mode: since.includes(':') ? 'value' : 'property' };
  }
  if (/^\s*$/.test(before)) return { prefix, mode: 'statement' };
  if (/^\s*style\s+[^{]*$/.test(before)) return { prefix, mode: 'selector' };
  return { prefix, mode: 'expression' };
}

// A name's letters: `x²` is the name `x` squared, since ² isn't a digit a name can hold, and the
// `px` of `3px` and the `x` of `P.x` aren't names.
const NAME = /(?<![\p{L}\p{Nd}_.])[\p{L}_][\p{L}\p{Nd}_₀-₉]*/gu;
const MARKS = /^\s*(?:curve|segment|path|polygon|rightangle|region|label|arrow|dots|circles|curves|group|outline|panel|place|expression|dot)\s+([^=\n{]*)/gmu;

/** The names a figure's files use, most used first, leaving out the language's own words. Only
 * the sheets' with `sheets`, or only what's drawn and tagged with `marks`. */
function namesIn(files, skip, { sheets = false, marks = false } = {}) {
  const counts = new Map();
  const own = new Set([...STATEMENTS, ...CLAUSES, ...PROPERTIES, ...FUNCTIONS, 'style']);
  for (const f of files) {
    if (sheets && !f.path.endsWith('.style')) continue;
    const text = f.text.replace(/\/\/.*$/gm, '').replace(/"[^"\n]*"/g, '');
    const found = marks
      ? [...text.matchAll(MARKS), ...text.matchAll(/\btag\s+([^\n{]*)/gu)].flatMap((m) => /^(?:dot|tag)\b/.test(m[0].trim()) ? m[1].split(',').map((n) => n.trim()) : [m[1].trim().split(/\s/)[0]])
      : [...text.matchAll(NAME)].map((m) => m[0]);
    for (const name of found) {
      if (/^[\p{L}_][\p{L}\p{Nd}_₀-₉]*$/u.test(name) && !own.has(name)) counts.set(name, (counts.get(name) ?? 0) + 1);
    }
  }
  // The word at the caret counts once for itself.
  if (skip && counts.get(skip) === 1) counts.delete(skip);
  return [...counts].sort((a, b) => b[1] - a[1]).map(([n]) => n);
}

/** The words that could finish what's at `at` in the file at `path`, given all of the figure's
 * files: statements and names in a `.pen`, and in a `.style` the four statements a sheet has,
 * then selectors, properties and values in its rules. */
export function suggest(text, at, path, files) {
  const where = context(text, at);
  if (!where) return [];
  const { prefix, mode } = where;
  const sheet = path.endsWith('.style');
  const word = (w, after = '') => ({ w, after });
  const names = (only) => namesIn(files, prefix, only).map((n) => word(n));
  const lists = {
    statement: sheet ? ['style', 'let', 'fn', 'use'].map((w) => word(w, ' ')) : STATEMENTS.map((w) => word(w, ' ')),
    property: PROPERTIES.map((w) => word(w, ': ')),
    value: [...names({ sheets: true }), ...STYLE_VALUES.map((w) => word(w)), ...FUNCTIONS.map((w) => word(w, '(')), ...CONSTANTS.map((w) => word(w)), ...names()],
    selector: [...names({ marks: true }).map((s) => ({ ...s, after: ' ' })), ...SELECTORS.map((w) => word(w, ' '))],
    expression: [...names(), ...FUNCTIONS.map((w) => word(w, '(')), ...CONSTANTS.map((w) => word(w)), ...CLAUSES.map((w) => word(w, ' '))],
  }[mode];
  const seen = new Set();
  const out = [];
  const take = (test) => {
    for (const s of lists) {
      if (seen.has(s.w) || !test(s.w) || (s.w === prefix && !s.after)) continue;
      seen.add(s.w);
      out.push({ ...s, prefix });
    }
  };
  if (prefix) {
    take((w) => w.startsWith(prefix));
    take((w) => w.toLowerCase().startsWith(prefix.toLowerCase()));
  } else {
    take(() => true);
  }
  return out.slice(0, 24);
}

// ---- Characters as a reader sees them ----

const graphemes = typeof Intl !== 'undefined' && Intl.Segmenter ? new Intl.Segmenter(undefined, { granularity: 'grapheme' }) : null;

/** Where the caret goes from `at` one character back (`by` < 0) or on (`by` > 0), a character
 * being what a reader sees as one: an emoji, a flag of two letters, or a letter with its accents.
 * Without `Intl.Segmenter`, a pair of surrogates and the combining marks after a letter. */
export function step(text, at, by) {
  if (graphemes) {
    // Only the text around the caret: no character a reader sees is near this long.
    const from = Math.max(0, at - 128);
    const near = text.slice(from, at + 128);
    const here = at - from;
    let before = 0;
    for (const { index, segment } of graphemes.segment(near)) {
      const end = index + segment.length;
      if (by < 0 && end >= here) return from + (index < here ? index : before);
      if (by > 0 && end > here) return from + end;
      before = index;
    }
    return by < 0 ? from + before : Math.min(text.length, from + near.length);
  }
  const mark = /[\u0300-\u036f\u1ab0-\u1aff\u20d0-\u20ff\ufe20-\ufe2f\ufe0f\u200d]/;
  const low = (i) => /[\uDC00-\uDFFF]/.test(text[i] ?? '') && /[\uD800-\uDBFF]/.test(text[i - 1] ?? '');
  if (by < 0) {
    let i = Math.max(0, at - 1);
    while (i > 0 && (low(i) || mark.test(text[i]))) i--;
    return i;
  }
  let i = Math.min(text.length, at + 1);
  while (i < text.length && (low(i) || mark.test(text[i]))) i++;
  return i;
}

// ---- The keyboard ----

// Undo and Redo are drawn, so they look the same everywhere: an arrowhead and a shaft that turns
// back, mirrored for redo. The key's name is what's read.
const ARROW = '<path d="M9 5 4 10l5 5"/><path d="M4 10h10a5 5 0 0 1 0 10h-3"/>';
const ICONS = {
  undo: `<svg aria-hidden="true" viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${ARROW}</svg>`,
  redo: `<svg aria-hidden="true" viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><g transform="matrix(-1 0 0 1 24 0)">${ARROW}</g></svg>`,
};

/**
 * Puts the Pencil keys in `root`, for `textarea`.
 *
 * - `files()`: the figure's files, `{ path, text }`, for the names a suggestion can be.
 * - `path()`: the path of the file the textarea shows, which says what kind of words go where.
 * - `keys`: whether there are Pencil keys at all. Without them it's the system keyboard, with the
 *   symbol strip over it.
 * - `words`: whether there's a row of suggested words.
 * - `on`: whether there are any keys. Off, the textarea is a plain one.
 * - `touch`: whether the main pointer is a finger, which a page sets from `touch` (viewport.js).
 * - `history`: the textarea's undo and redo (history.js), for the Undo and Redo keys at the end
 *   of the words row.
 * - `opener`: a button the page shows, which brings the keys back once they were closed.
 * - `remember`: the start of the session storage keys under which the choices of the system
 *   keyboard, of the palette and of closing the keys are kept for the visit, or null to keep them
 *   for this page.
 * - `leave()`: where focus goes when Escape leaves the keys.
 *
 * On a touch screen the keys come up when the textarea takes focus, and go when focus leaves both
 * it and them. The first key typed on a hardware keyboard, which needs no keys on the screen,
 * drops them to the words and the symbols over the system keyboard, with a key back to them.
 * With a mouse, the words and the symbols are there while the editor is, and the hardware keyboard
 * is the keyboard. Either way ▾ closes everything until `opener` is pressed.
 */
export function keyboard({ textarea, root, files, path, on = true, touch = true, history = null, opener = null, keys = true, words = true, remember = 'pencil-keys', leave = () => {} }) {
  const state = { on, touch, keys, words, system: false, palette: false, closed: false, shown: false, page: 'numbers', shift: 0 };
  const recall = (name) => {
    try {
      return remember ? sessionStorage.getItem(`${remember}:${name}`) === '1' : false;
    } catch {
      return false;
    }
  };
  const keep = (name, value) => {
    try {
      if (remember) sessionStorage.setItem(`${remember}:${name}`, value ? '1' : '0');
    } catch {}
  };
  state.system = recall('system');
  state.palette = recall('palette');
  state.closed = recall('closed');
  /** Whether it's the two rows, the words and the symbols, rather than the words and the keys:
   * over the system keyboard on a touch screen, and with a mouse unless the palette is open. */
  const rows = () => !state.keys || (state.touch ? state.system : !state.palette);

  root.classList.add('keyboard');
  const wordsEl = Object.assign(document.createElement('div'), { className: 'strip words-row' });
  wordsEl.setAttribute('role', 'toolbar');
  wordsEl.setAttribute('aria-label', 'Words');
  const symbolsEl = Object.assign(document.createElement('div'), { className: 'strip symbols' });
  symbolsEl.setAttribute('role', 'toolbar');
  symbolsEl.setAttribute('aria-label', 'Symbols');
  const keysEl = Object.assign(document.createElement('div'), { className: 'keys' });
  keysEl.setAttribute('role', 'group');
  root.replaceChildren(wordsEl, symbolsEl, keysEl);
  // The words scroll; Undo and Redo stay at the row's end, in both ways of typing.
  const wordsRun = Object.assign(document.createElement('div'), { className: 'run' });
  wordsEl.append(wordsRun);

  // ---- The caret, kept in view ----

  // Where the caret is, from a copy of the text laid out as the textarea lays it out, so a wrapped
  // line counts as the lines it takes.
  const mirror = document.createElement('div');
  mirror.setAttribute('aria-hidden', 'true');
  mirror.style.cssText = 'position:absolute;left:-10000px;top:0;visibility:hidden;overflow-wrap:break-word;';
  function revealCaret() {
    if (!textarea.isConnected || !textarea.clientHeight) return;
    if (!mirror.isConnected) document.body.append(mirror);
    const css = getComputedStyle(textarea);
    for (const p of ['font', 'padding', 'letterSpacing', 'tabSize', 'boxSizing', 'lineHeight', 'whiteSpace']) mirror.style[p] = css[p];
    mirror.style.width = `${textarea.clientWidth}px`;
    mirror.textContent = textarea.value.slice(0, textarea.selectionEnd);
    const mark = document.createElement('span');
    mark.textContent = '​';
    mirror.append(mark);
    const line = parseFloat(css.lineHeight) || 20;
    const top = mark.offsetTop;
    if (top - line / 2 < textarea.scrollTop) textarea.scrollTop = Math.max(0, top - line);
    else if (top + line * 1.5 > textarea.scrollTop + textarea.clientHeight) textarea.scrollTop = top + line * 2 - textarea.clientHeight;
    // A line that doesn't wrap runs off to the right.
    const left = mark.offsetLeft;
    const pad = parseFloat(css.paddingLeft) || 0;
    if (left - pad < textarea.scrollLeft) textarea.scrollLeft = Math.max(0, left - pad - 24);
    else if (left + 24 > textarea.scrollLeft + textarea.clientWidth) textarea.scrollLeft = left + 24 - textarea.clientWidth;
  }
  new ResizeObserver(() => state.shown && revealCaret()).observe(textarea);

  // ---- Editing, as a key does it ----

  /** The selection the textarea last had, to put back when a key brings focus to it. */
  let kept = [0, 0];
  document.addEventListener('selectionchange', () => {
    if (document.activeElement !== textarea) return;
    kept = [textarea.selectionStart, textarea.selectionEnd];
    paintWords();
  });
  textarea.addEventListener('input', (e) => {
    // An undo or a redo, by a key, a shortcut or the system, shows where it was: the caret is
    // brought into view, and the scroll stays where it is when it already is.
    if (/^history/.test(e.inputType ?? '')) revealCaret();
    paintWords();
  });
  // Not every browser says when a textarea's selection changes, so it's read again as focus leaves.
  textarea.addEventListener('blur', () => (kept = [textarea.selectionStart, textarea.selectionEnd]));

  /** Gives the textarea focus for an edit, where it had it, without scrolling the page. */
  function toEditor() {
    if (document.activeElement === textarea) return;
    textarea.focus({ preventScroll: true });
    textarea.setSelectionRange(...kept);
  }

  // The system keyboard may be composing a word, as Gboard does every word, and a symbol typed
  // into a composition would be taken into the word or lost. A key ends the composition first:
  // focus leaving the field and coming back commits the word as it stands.
  let composing = false;
  textarea.addEventListener('compositionstart', () => (composing = true));
  textarea.addEventListener('compositionend', () => (composing = false));
  function endComposition() {
    if (!composing || document.activeElement !== textarea) return;
    const selection = [textarea.selectionStart, textarea.selectionEnd];
    textarea.blur();
    textarea.focus({ preventScroll: true });
    textarea.setSelectionRange(...selection);
    composing = false;
  }

  /** Types text at the caret through the browser's own editing, so undo takes it back. */
  function insert(text) {
    toEditor();
    endComposition();
    if (!document.execCommand('insertText', false, text)) {
      textarea.setRangeText(text, textarea.selectionStart, textarea.selectionEnd, 'end');
      textarea.dispatchEvent(new InputEvent('input', { inputType: 'insertText', data: text }));
    }
    revealCaret();
  }

  /** Deletes the selection, or the character before the caret as a reader sees it (`step`). */
  function backspace() {
    toEditor();
    endComposition();
    const { selectionStart: s, selectionEnd: e } = textarea;
    if (s === e) {
      if (s === 0) return;
      textarea.setSelectionRange(step(textarea.value, s, -1), s);
    }
    if (!document.execCommand('delete')) {
      textarea.setRangeText('', textarea.selectionStart, textarea.selectionEnd, 'end');
      textarea.dispatchEvent(new InputEvent('input', { inputType: 'deleteContentBackward' }));
    }
    revealCaret();
  }

  function moveCaret(by) {
    toEditor();
    endComposition();
    const { selectionStart: s, selectionEnd: e, value } = textarea;
    const at = s !== e ? (by < 0 ? s : e) : step(value, s, by);
    textarea.setSelectionRange(at, at);
    revealCaret();
    paintWords();
  }

  /** A new line, indented as this one is, and one step more after an open brace. */
  function newLine() {
    toEditor();
    const before = textarea.value.slice(0, textarea.selectionStart);
    const line = before.slice(before.lastIndexOf('\n') + 1);
    insert(`\n${/^\s*/.exec(line)[0]}${/\{\s*$/.test(line) ? '  ' : ''}`);
  }

  /** A word from the row, as a step of its own for undo. */
  function finishWord({ w, after, prefix }) {
    toEditor();
    const at = textarea.selectionStart;
    textarea.setSelectionRange(at - prefix.length, at);
    history?.separate();
    insert(w + (after === '(' && textarea.value[at] === '(' ? '' : after));
    history?.separate();
  }

  /** One step of the file's history back or on, with focus kept in the editor. */
  function undo(way) {
    toEditor();
    endComposition();
    if (way === 'undo') history?.undo();
    else history?.redo();
    revealCaret();
    paintWords();
  }

  // ---- Which keys are up ----

  function apply() {
    if (!state.on || state.closed) state.shown = false;
    // With a mouse they're there while the editor is, not only while it has focus.
    else if (!state.touch) state.shown = true;
    const system = rows();
    // `inputmode="none"` keeps the system keyboard down for a field that has focus. Only on a
    // touch screen: with a mouse it's never set.
    if (!state.touch || !state.on || state.closed || system) textarea.removeAttribute('inputmode');
    else textarea.setAttribute('inputmode', 'none');
    if (opener) {
      opener.hidden = !(state.on && state.closed);
      const label = state.touch ? 'Show the Pencil keys' : 'Show the words and symbols';
      opener.setAttribute('aria-label', label);
      opener.title = label;
    }
    root.hidden = !state.shown;
    // For the page's styles, which can give the keys room while they're up.
    document.documentElement.classList.toggle('keys-up', state.shown);
    symbolsEl.hidden = !system;
    keysEl.hidden = system;
    wordsEl.hidden = !state.words;
    paintStrip();
    paintKeys();
    paintWords();
    if (state.shown) requestAnimationFrame(revealCaret);
  }

  function show() {
    if (state.shown || !state.on || state.closed) return;
    state.shown = true;
    apply();
  }

  /** Closes the keys for the visit, or opens them again and forgets that. */
  function close(now) {
    state.closed = now;
    keep('closed', now);
    apply();
  }
  opener?.addEventListener('click', () => {
    close(false);
    textarea.focus({ preventScroll: true });
    show();
  });

  // A key typed on a hardware keyboard while the Pencil keys are up says there's a keyboard, so
  // they drop to the words and the symbols, which are still worth having with one. That's kept
  // for the visit as the choice of the system keyboard is, so a keyboard taken away later leaves
  // the system's, with the strip's key back to these. Esc, Tab and the arrows don't: Esc goes into
  // the keys, and the others move. With a mouse nothing closes.
  textarea.addEventListener('keydown', (e) => {
    if (!state.touch || !state.shown || rows() || e.isComposing || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key.length === 1 || e.key === 'Backspace' || e.key === 'Delete' || e.key === 'Enter') {
      // No blur and focus, as the system key does: the key being typed goes on into the editor,
      // and a hardware keyboard has the system's down anyway. The next touch reads `inputmode`.
      state.system = true;
      keep('system', true);
      apply();
    }
  });

  function hide() {
    if (!state.shown || !state.touch) return;
    state.shown = false;
    apply();
  }

  // On focus, and not as the editor is pressed: they'd move it before the press placed the caret.
  // ▾ takes focus from the editor as it puts them away, so a touch brings them back.
  textarea.addEventListener('focus', show);
  // They go when focus goes somewhere else in the page: not when a key is pressed, which keeps it
  // in the editor, nor when the window loses focus, which leaves it there.
  // The keys going moves what's under the finger, so they wait for it: for a drag, such as one
  // on a figure's handle, to end, then for the tap's click, or a moment when nothing clicks.
  const down = new Set();
  addEventListener('pointerdown', (e) => {
    // A new first finger means any that were down aren't.
    if (e.isPrimary) down.clear();
    down.add(e.pointerId);
  }, true);
  for (const type of ['pointerup', 'pointercancel']) addEventListener(type, (e) => down.delete(e.pointerId), true);
  const away = () => {
    let done = false;
    let timer = 0;
    const finish = () => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      removeEventListener('click', finish, true);
      removeEventListener('pointerup', lifted, true);
      removeEventListener('pointercancel', lifted, true);
      setTimeout(() => {
        const at = document.activeElement;
        if (at !== textarea && !root.contains(at)) hide();
      });
    };
    const wait = () => {
      clearTimeout(timer);
      timer = setTimeout(finish, 400);
    };
    // Registered after the listener that counts fingers, so it reads the count after this one.
    const lifted = () => !down.size && wait();
    addEventListener('click', finish, true);
    addEventListener('pointerup', lifted, true);
    addEventListener('pointercancel', lifted, true);
    if (!down.size) wait();
  };
  textarea.addEventListener('blur', away);
  root.addEventListener('focusout', away);

  /** Hands the field to the system keyboard, or takes it back. A blur and a focus in the same tap
   * make each browser read the new `inputmode` (lab/keyboard/README.md). */
  function useSystem(on) {
    if (!state.touch) return usePalette(!on);
    state.system = on;
    keep('system', on);
    const selection = [...kept];
    apply();
    textarea.blur();
    textarea.focus({ preventScroll: true });
    textarea.setSelectionRange(...selection);
  }

  /** With a mouse: opens the palette of keys under the words, or goes back to the symbols. A
   * keyboard's focus goes to the key that goes back. */
  function usePalette(on) {
    const inKeys = root.contains(document.activeElement);
    state.palette = on;
    keep('palette', on);
    apply();
    if (inKeys) {
      const back = root.querySelector(`button[data-id="${on ? '@system' : '@pencil'}"]`);
      if (back) {
        rove(back.closest('.keys, .strip'), back.dataset.id);
        back.focus({ preventScroll: true });
      }
    }
  }

  // ---- Drawing the keys ----

  const specs = new WeakMap();

  // With a mouse there's no system keyboard: the two keys say what they show.
  const MOUSE_NAMES = {
    system: { t: 'symbol row', name: 'Back to the row of symbols' },
    pencil: { t: 'all keys', name: 'All the keys: numbers, letters, words and Greek' },
  };

  function makeKey(spec) {
    let k = key(spec);
    if (!state.touch && MOUSE_NAMES[k.action]) k = { ...k, ...MOUSE_NAMES[k.action] };
    if (k.gap !== undefined) {
      const gap = document.createElement('span');
      gap.className = 'gap';
      gap.style.flexGrow = String(k.gap);
      return gap;
    }
    const b = document.createElement('button');
    b.type = 'button';
    b.tabIndex = -1;
    b.className = 'k';
    let ins = k.ins;
    let text = k.t;
    if (state.page === 'letters' && state.shift && ins && /^[a-z]$/.test(ins)) text = ins = ins.toUpperCase();
    b.textContent = text;
    if (ICONS[k.action]) b.innerHTML = ICONS[k.action];
    if (k.name && k.name !== text) b.setAttribute('aria-label', k.name);
    if (k.action) b.classList.add(k.action === 'page' ? 'page-key' : 'action');
    if (k.action === 'space') b.classList.add('wide');
    if (k.action === 'system' || k.action === 'pencil') b.classList.add('words-key');
    if (k.action === 'page' && k.page === state.page) {
      b.classList.add('current');
      b.setAttribute('aria-current', 'true');
    }
    if (k.action === 'shift') {
      b.classList.toggle('on', state.shift > 0);
      b.setAttribute('aria-pressed', String(state.shift > 0));
      if (state.shift === 2) b.setAttribute('aria-label', 'Shift, locked');
    }
    // A key in a row of the pages takes its share of the row. One in a row that scrolls is as wide
    // as what it says: two words alone in the row stretched across the whole of it.
    if (k.w !== undefined && k.w !== 1) b.style.flexGrow = String(k.w);
    else b.style.removeProperty('flex-grow');
    b.dataset.id = k.action ? `@${k.action}${k.page ? `:${k.page}` : ''}` : `=${ins}`;
    specs.set(b, { ...k, ins });
    return b;
  }

  function row(list, className = 'row') {
    const el = document.createElement('div');
    el.className = className;
    el.append(...list.map(makeKey));
    return el;
  }

  /** The words page: Pencil's words and the functions, by where they're used. */
  function wordsPage(side) {
    const wrap = document.createElement('div');
    wrap.className = 'words';
    const list = document.createElement('div');
    list.className = 'list';
    const section = (title, all, after) => {
      const h = document.createElement('h2');
      h.textContent = title;
      list.append(h, ...all.map((w) => makeKey({ ins: w + after, t: w, name: w })));
    };
    section('Statements', STATEMENTS, ' ');
    section('Functions', FUNCTIONS, '(');
    section('In a statement', CLAUSES, ' ');
    section('Names', CONSTANTS, '');
    section('In a sheet', PROPERTIES, ': ');
    section('A sheet’s values', STYLE_VALUES, '');
    wrap.append(list, row(side, 'side'));
    return wrap;
  }

  let focusId = null; // the key that has the keys' one tab stop
  function paintKeys() {
    if (!state.shown || keysEl.hidden) return;
    const page = PAGES.find((p) => p.id === state.page);
    const el = document.createElement('div');
    el.className = 'page';
    if (page.id === 'words') el.append(wordsPage(page.side));
    else el.append(...((state.shift && page.shifted) || page.rows).map((r) => row(r)));
    const hadFocus = keysEl.contains(document.activeElement) ? document.activeElement.dataset.id : null;
    keysEl.replaceChildren(el, row(BOTTOM));
    keysEl.setAttribute('aria-label', `Pencil keys: ${page.name}`);
    rove(keysEl, hadFocus ?? focusId);
    if (hadFocus) keysEl.querySelector('[tabindex="0"]')?.focus();
  }

  /** Gives one key in a set the tab stop: the one named, or the first. */
  function rove(container, id) {
    const all = [...container.querySelectorAll('button')];
    const pick = all.find((b) => b.dataset.id === id) ?? all[0];
    for (const b of all) b.tabIndex = b === pick ? 0 : -1;
  }

  function paintStrip() {
    if (!state.shown || symbolsEl.hidden) return;
    const parts = [];
    if (state.keys) {
      const back = document.createElement('div');
      back.className = 'fixed';
      const b = makeKey({ action: 'pencil', t: 'Pencil keys', name: 'Back to the Pencil keys', click: true });
      b.classList.add('to-keys');
      back.append(b);
      parts.push(back);
    }
    const run = document.createElement('div');
    run.className = 'run';
    run.append(...STRIP.map(makeKey));
    const tail = document.createElement('div');
    tail.className = 'fixed';
    // ▾ is here too, for a reader with a hardware keyboard, whose keys dropped to these rows.
    tail.append(makeKey('@left'), makeKey('@right'), makeKey('@hide'));
    parts.push(run, tail);
    symbolsEl.replaceChildren(...parts);
    rove(symbolsEl, null);
  }

  const suggestions = () =>
    textarea.selectionStart === textarea.selectionEnd ? suggest(textarea.value, textarea.selectionStart, path(), files()) : [];

  // The row is as tall empty as full (keyboard.css), so nothing above it moves as it fills.
  let historyTail = null;
  function paintHistory() {
    if (!history) return;
    if (!historyTail) {
      historyTail = document.createElement('div');
      historyTail.className = 'fixed history';
      historyTail.append(makeKey('@undo'), makeKey('@redo'));
      wordsEl.append(historyTail);
    }
    for (const b of historyTail.children) {
      const can = b.dataset.id === '@undo' ? history.canUndo : history.canRedo;
      b.setAttribute('aria-disabled', String(!can));
    }
  }
  history?.onChange(() => state.shown && paintHistory());

  function paintWords() {
    if (!state.shown || wordsEl.hidden) return;
    paintHistory();
    const found = suggestions();
    const focused = wordsEl.contains(document.activeElement) ? document.activeElement.dataset.id : null;
    const run = wordsRun;
    run.replaceChildren();
    run.scrollLeft = 0;
    if (!found.length) {
      const none = document.createElement('span');
      none.className = 'none';
      none.textContent = 'No words to suggest here';
      run.append(none);
    }
    run.append(...found.map((s) => {
      const b = makeKey({ action: 'word', t: s.w, name: s.w, click: true });
      specs.set(b, { ...specs.get(b), word: s });
      b.dataset.id = `~${s.w}`;
      return b;
    }));
    rove(wordsEl, focused);
    if (focused) wordsEl.querySelector('[tabindex="0"]')?.focus();
  }

  // ---- Pressing keys ----

  // A finger on a key leaves focus in the editor. Chrome on Android focuses a tapped button even
  // when its press was cancelled, so focus that a press brought to a key goes straight back. A
  // hardware keyboard's focus, which Escape brings to the keys, stays.
  let pressedAt = -Infinity;
  root.addEventListener('focusin', (e) => {
    if (performance.now() - pressedAt > 1000 || e.relatedTarget !== textarea) return;
    textarea.focus({ preventScroll: true });
    textarea.setSelectionRange(...kept);
  });

  function act(b) {
    const k = specs.get(b);
    if (!k || b.getAttribute('aria-disabled') === 'true') return;
    switch (k.action) {
      case undefined:
        insert(k.ins);
        if (state.page === 'letters' && state.shift === 1 && /^[A-Za-z]$/.test(k.ins)) {
          state.shift = 0;
          paintKeys();
        }
        return;
      case 'back': return backspace();
      case 'left': return moveCaret(-1);
      case 'right': return moveCaret(1);
      case 'enter': return newLine();
      case 'space': return insert(' ');
      case 'word': return finishWord(k.word);
      case 'shift': {
        // Once for the next letter; twice quickly to keep it.
        const now = performance.now();
        state.shift = state.shift === 1 && now - (act.lastShift ?? 0) < 400 ? 2 : state.shift ? 0 : 1;
        act.lastShift = now;
        return paintKeys();
      }
      case 'page':
        state.page = k.page;
        state.shift = 0;
        return paintKeys();
      case 'undo': return undo('undo');
      case 'redo': return undo('redo');
      case 'system': return useSystem(true);
      case 'pencil': return useSystem(false);
      case 'hide': {
        // Closed for the visit, until the page's opener. From a hardware keyboard, focus leaves
        // with them.
        const inKeys = root.contains(document.activeElement);
        close(true);
        textarea.blur();
        if (inKeys) leave();
        return;
      }
    }
  }

  // A key acts as it's pressed, so typing is quick and a held key repeats. Keys in a row that
  // scrolls act on the click instead, so a swipe through them types nothing, and so do keys that
  // change the field's keyboard, which iOS only does in a tap's own click.
  let pressed = null;
  let repeat = 0;
  // The key a finger's press acted on, whose click that press then makes is let go.
  let acted = null;
  const stop = () => {
    clearTimeout(repeat);
    clearInterval(repeat);
    pressed?.classList.remove('down');
    pressed = null;
  };
  for (const el of [keysEl, symbolsEl, wordsEl]) {
    el.addEventListener('mousedown', (e) => {
      if (e.target.closest('button')) e.preventDefault();
    });
    el.addEventListener('pointerdown', (e) => {
      const b = e.target.closest('button');
      if (!b || e.button > 0) return;
      pressedAt = performance.now();
      stop();
      acted = null;
      pressed = b;
      b.classList.add('down');
      const k = specs.get(b);
      if (b.closest('.run, .list') || k?.click) return;
      e.preventDefault();
      acted = b;
      act(b);
      if (k?.repeat) {
        repeat = setTimeout(() => {
          repeat = setInterval(() => (pressed === b ? act(b) : stop()), 60);
        }, 420);
      }
    });
    el.addEventListener('click', (e) => {
      const b = e.target.closest('button');
      if (!b) return;
      // Acted on already as it was pressed, however long it was held. A click from a hardware
      // keyboard (`detail` 0) isn't a press's.
      if (b === acted && e.detail !== 0) {
        acted = null;
        return;
      }
      const fromKeys = e.detail === 0 && document.activeElement === b ? b : null;
      act(b);
      // Pressed from a hardware keyboard: focus stays on that key, wherever it's drawn now.
      if (fromKeys && state.shown) {
        const again = [...el.querySelectorAll('button')].find((x) => x.dataset.id === fromKeys.dataset.id);
        if (again) {
          rove(el, again.dataset.id);
          again.focus({ preventScroll: true });
        }
      }
    });
    // The arrows move between keys, by where they are on screen; Home and End go to a row's ends.
    el.addEventListener('keydown', (e) => {
      const from = e.target.closest('button');
      if (!from || e.metaKey || e.ctrlKey || e.altKey) return;
      const to = neighbour(el, from, e.key);
      if (to === undefined) return;
      e.preventDefault();
      if (!to) return;
      rove(el, to.dataset.id);
      if (el === keysEl) focusId = to.dataset.id;
      to.focus();
    });
  }
  // A held key stops when the finger lifts, leaves it, or slides off it. A touch is captured by
  // the key it pressed, which then has no `pointerleave` until it lifts, so a move off it counts.
  addEventListener('pointerup', stop);
  addEventListener('pointercancel', stop);
  addEventListener('blur', stop);
  root.addEventListener('pointerleave', (e) => e.target === pressed && stop(), true);
  root.addEventListener('lostpointercapture', (e) => e.target === pressed && stop(), true);
  root.addEventListener('pointermove', (e) => {
    if (!pressed) return;
    const r = pressed.getBoundingClientRect();
    if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) stop();
  });
  // Escape in the keys puts them away and leaves, as Escape in the editor leaves it.
  root.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape' || e.metaKey || e.ctrlKey || e.altKey) return;
    e.preventDefault();
    leave();
    hide();
  });

  apply();
  known.then(() => state.shown && apply());

  return {
    state,
    show,
    hide,
    suggestions,
    /** For a hardware keyboard in the editor, Escape goes to the keys while they're up, so
     * everything a finger can reach a keyboard can too. Says whether it went. */
    escape() {
      if (!state.shown || document.activeElement !== textarea) return false;
      const to = [keysEl, symbolsEl, wordsEl].find((el) => !el.hidden && el.querySelector('button'));
      to?.querySelector('[tabindex="0"]')?.focus();
      return !!to;
    },
    /** After the page changes what the textarea shows. */
    refresh() {
      kept = [textarea.selectionStart, textarea.selectionEnd];
      paintWords();
    },
    /** Changes which keys there are: `touch`, as a mouse comes and goes, and the lab's ways of
     * typing. */
    set(changes) {
      const was = state.touch;
      Object.assign(state, changes);
      // To a touch screen, they're up only while the editor has focus.
      if (state.touch && !was) state.shown = document.activeElement === textarea || root.contains(document.activeElement);
      apply();
    },
  };
}

/** The key the arrow goes to from `from`, null where there's none, undefined for another key. */
function neighbour(container, from, keyName) {
  const dirs = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1], Home: [-9, 0], End: [9, 0] };
  const dir = dirs[keyName];
  if (!dir) return undefined;
  const all = [...container.querySelectorAll('button')].filter((b) => b.offsetParent);
  const at = (b) => {
    const r = b.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2, h: r.height };
  };
  const a = at(from);
  const sameRow = (p) => Math.abs(p.y - a.y) < a.h / 2;
  let best = null;
  let score = Infinity;
  for (const b of all) {
    if (b === from) continue;
    const p = at(b);
    let s;
    if (dir[1] === 0) {
      if (!sameRow(p) || Math.sign(p.x - a.x) !== Math.sign(dir[0])) continue;
      s = Math.abs(dir[0]) > 1 ? -Math.abs(p.x - a.x) : Math.abs(p.x - a.x);
    } else {
      const dy = (p.y - a.y) * dir[1];
      if (dy < a.h / 2) continue;
      s = dy * 4 + Math.abs(p.x - a.x);
    }
    if (s < score) [best, score] = [b, s];
  }
  // Left and right go on to the row before or after.
  if (!best && Math.abs(dir[0]) === 1) best = all[all.indexOf(from) + dir[0]] ?? null;
  return best;
}
