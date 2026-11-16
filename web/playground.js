// The playground: a figure's `.pen` and its sheets in an editor beside the figure, compiled again
// as they change. The figure is a `<pencil-figure>`, which owns the GPU, the clock and the handles,
// and nothing here works out a value of the figure's own: the page only sets and reads bindings.
//
// What's open is one of the examples, a document in one of its looks, or the reader's own figure
// ("yours"): one that came from a link, from files, or from adding or removing a file. There is one
// of those at a time, kept whole in this browser, apart from the examples' edits.
import { PencilFigure } from '@pencil-lang/figure';
import { ready as readingReady, readingAt, pasteLatex } from './reading.js';
import { viewing } from './controls.js';
import { uses } from '@pencil-lang/figure/host';
import { pack, unpack } from '@pencil-lang/figure/pack';

/** A file's name without its folder or its extension, as the menus show it. */
function styleName(path) {
  return path.split('/').pop().replace(/\.(style|pen)$/, '');
}

// Each document with the looks its own pages or videos load: sheets from the repo root, in order.
// A look's id is what `?figure=` names, and the site links here with them.
const THEME = 'corpus/theme.style';
const FIGURES = [
  ['001 circle', 'corpus/001-circle/circle.pen', [
    ['circle', ['corpus/001-circle/circle.style']],
    ['circle-bands', ['corpus/001-circle/bands.style']]]],
  ['002 strokes, ring', 'corpus/002-strokes/ring.pen', [['ring', ['corpus/002-strokes/filled.style']]]],
  ['002 strokes, wave', 'corpus/002-strokes/wave.pen', [['wave', ['corpus/002-strokes/taper.style']]]],
  ['003 unroll', 'corpus/003-unroll/unroll.pen', [
    ['unroll', ['corpus/003-unroll/blog.style']],
    ['unroll-night', ['corpus/003-unroll/night.style']]]],
  ['004 bounds', 'corpus/004-bounds/bounds.pen', [['bounds', ['corpus/004-bounds/blog.style']]]],
  ['005 tangents', 'corpus/005-tangents/tangents.pen', [['tangents', [THEME, 'corpus/005-tangents/plain.style']]]],
  ['006 square', 'corpus/006-square/square.pen', [['square', [THEME, 'corpus/006-square/plain.style']]]],
  ['007 lattice', 'corpus/007-lattice/lattice.pen', [['lattice', [THEME, 'corpus/007-lattice/plain.style']]]],
  ['008 hearts', 'corpus/008-hearts/hearts.pen', [['hearts', [THEME, 'corpus/008-hearts/plain.style']]]],
  ['009 squaring', 'corpus/009-squaring/squaring.pen', [['squaring', [THEME, 'corpus/009-squaring/plain.style']]]],
  ['010 Möbius', 'corpus/010-mobius/mobius.pen', [['mobius', [THEME, 'corpus/010-mobius/plain.style']]]],
  ['011 circumcircle', 'corpus/011-circumcircle/circumcircle.pen', [['circumcircle', ['corpus/011-circumcircle/plain.style']]]],
  ['012 limaçon', 'corpus/012-limacon/limacon.pen', [['limacon', ['corpus/012-limacon/plain.style']]]],
  ['013 tube', 'corpus/013-tube/tube.pen', [['tube', [THEME, 'corpus/013-tube/plain.style']]]],
  ['014 scaled copy', 'corpus/014-scaled-copy/scaled-copy.pen', [['scaled-copy', [THEME, 'corpus/014-scaled-copy/plain.style']]]],
  ['015 log nearest', 'corpus/015-log-nearest/log-nearest.pen', [['log-nearest', [THEME, 'corpus/015-log-nearest/plain.style']]]],
  ['016 unwrap', 'corpus/016-unwrap/unwrap.pen', [['unwrap', [THEME, 'corpus/016-unwrap/plain.style']]]],
  ['Explainer: limaçon', 'lab/explainer/limacon.pen', [
    ['explainer', [THEME, 'corpus/dark.style', 'lab/explainer/limacon.style', 'lab/explainer/caption.style']]]],
].map(([name, pen, looks]) => {
  const doc = { name, pen };
  // A look is named for its sheets, without the theme every figure shares.
  doc.looks = looks.map(([id, sheets]) => ({ id, doc, sheets, name: sheets.filter((s) => s !== THEME).map(styleName).join(' + ') }));
  doc.id = doc.looks[0].id;
  return doc;
});
const LOOKS = FIGURES.flatMap((doc) => doc.looks);
// What the Figure menu calls the reader's own figure, and what `?figure=` names it by.
const YOURS = 'yours';

// How long typing has to stop before compiling: short enough to feel live, long enough not to
// compile a figure like the lattice on every key.
const SETTLE_MS = 120;
// Example edits are kept under this and their path. Your figure is kept whole under the other,
// which no path can make.
const STORE = 'pencil-playground:';
const STORE_YOURS = 'pencil-playground-yours';

const $ = (id) => document.getElementById(id);
const query = new URLSearchParams(location.search);
const figure = $('figure');
// Before anything waits, so the element starts the right GPU.
if (query.has('gl')) figure.setAttribute('gl', '');
const textarea = $('source');

const fetchText = async (path) => {
  const response = await fetch(`../${path}`, { cache: 'no-store' });
  if (!response.ok) throw new Error(`${path}: ${response.status}`);
  return response.text();
};

// ---- Sources and the editor ----

/** One file: what's on disk, and what's being edited. An example's edits are kept per path, so a
 * shared sheet like `theme.style` carries its edits to every figure that loads it. A `root` is the
 * document or one of its sheets, in order. The others are files those `use`, each after the file
 * that first uses it.
 *
 * In your figure, `disk` is the file as it was when the figure was opened, and `kept` as it was
 * last shared or saved, so the page knows when an edit exists nowhere else. A `pinned` file is one
 * you added or opened, which stays while nothing uses it (then it's `unused`). */
let files = [];
let active = 0;
let inEditor = null; // the file the textarea shows, to keep its caret and scroll when another is shown
let failure = null; // { file, line, col, message }
let warning = null; // a draw error, which has no place in a file
let notice = null; // a link that couldn't be read, until another figure is opened

/** What's open: one of LOOKS, or YOURS. */
let showing = null;
/** Your figure: `{ name, link, restructured, files }`. `link` is the packed form it came from or
 * was last shared as, and `restructured` says a file was added or removed since then. */
let yours = null;
/** Your files that a `use` stopped naming, kept for this page in case it comes back. */
let spare = new Map();

const stored = (path) => localStorage.getItem(STORE + path);
const store = (file) => {
  if (file.text === file.disk) localStorage.removeItem(STORE + file.path);
  else localStorage.setItem(STORE + file.path, file.text);
};
const baseName = (path) => path.split('/').pop();
const folderOf = (path) => path.slice(0, path.lastIndexOf('/') + 1);

function saveYours() {
  if (!yours) return localStorage.removeItem(STORE_YOURS);
  const plain = ({ path, disk, text, kept, root, pinned }) => ({ path, disk, text, kept, root: !!root, pinned: !!pinned });
  try {
    localStorage.setItem(STORE_YOURS, JSON.stringify({ ...yours, files: yours.files.map(plain) }));
  } catch {
    say('This browser’s storage is full, so your figure won’t be here after a reload.');
  }
}

function loadYours() {
  try {
    const saved = JSON.parse(localStorage.getItem(STORE_YOURS));
    const text = (v) => typeof v === 'string';
    if (!saved || !Array.isArray(saved.files) || !saved.files[0]?.root) return null;
    if (!saved.files.every((f) => text(f.path) && text(f.disk) && text(f.text))) return null;
    for (const f of saved.files) f.kept ??= f.disk;
    return { name: String(saved.name ?? ''), link: saved.link ?? null, restructured: !!saved.restructured, files: saved.files };
  } catch {
    return null;
  }
}

/** Whether your figure has changes that aren't in a link it came from or was shared as, or in
 * files it was opened from or saved to. */
const unsaved = (y) => !!y && (y.restructured || y.files.some((f) => f.text !== f.kept));

/** Keeps an edit: in your figure, or under the example file's path. */
function persist(file) {
  if (showing === YOURS) {
    yours.files = files;
    saveYours();
  } else if (file) store(file);
}

// What the page says back about what a button did, in a polite live region.
let saidTimer = 0;
function say(text) {
  const out = $('said');
  out.textContent = '';
  clearTimeout(saidTimer);
  // Emptied first, so the same words are announced again.
  setTimeout(() => {
    out.textContent = text;
    saidTimer = setTimeout(() => (out.textContent = ''), 8000);
  }, 50);
}

const escape = (s) => s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]);

/** Splits a line into colored runs. Only what needs no grammar: comments, strings, numbers with
 * their units, and a line's first word when a statement or rule could start with it. */
function tokens(line) {
  const runs = [];
  let rest = line;
  const lead = /^(\s*)([A-Za-z_]+)(?=\s)/.exec(rest);
  if (lead) {
    if (lead[1]) runs.push(['', lead[1]]);
    runs.push(['t-word', lead[2]]);
    rest = rest.slice(lead[0].length);
  }
  const pattern = /(\/\/.*$)|("(?:[^"\\]|\\.)*"?)|((?<![A-Za-z_\d])\d+(?:\.\d+)?(?:px|em|%)?)/g;
  let at = 0;
  for (const m of rest.matchAll(pattern)) {
    if (m.index > at) runs.push(['', rest.slice(at, m.index)]);
    runs.push([m[1] ? 't-comment' : m[2] ? 't-string' : 't-number', m[0]]);
    at = m.index + m[0].length;
  }
  if (at < rest.length) runs.push(['', rest.slice(at)]);
  return runs;
}

/** A line as HTML, with the error's spot underlined: from its column to the end of that word, or
 * the whole line when the error only knows the line. Columns count characters, as the parser does. */
function lineHtml(line, bad) {
  const runs = tokens(line);
  if (!bad) return runs.map(([cls, text]) => (cls ? `<span class="${cls}">${escape(text)}</span>` : escape(text))).join('');
  const chars = Array.from(line);
  let from = Math.max(0, bad.col - 1);
  if (bad.col <= 1) from = chars.findIndex((c) => !/\s/.test(c));
  if (from < 0 || from >= chars.length) from = Math.max(0, chars.length - 1);
  let to = from + 1;
  if (bad.col <= 1) to = chars.length;
  else while (to < chars.length && /[\w.]/.test(chars[to]) && /[\w.]/.test(chars[from])) to++;
  let html = '';
  let index = 0;
  for (const [cls, text] of runs) {
    const runChars = Array.from(text);
    runChars.forEach((c, k) => {
      const i = index + k;
      const classes = [cls, i >= from && i < to ? 'at-bad' : ''].filter(Boolean).join(' ');
      html += classes ? `<span class="${classes}">${escape(c)}</span>` : escape(c);
    });
    index += runChars.length;
  }
  return html || '<span class="at-bad"> </span>';
}

function paintEditor() {
  const file = files[active];
  if (!file) return;
  const bad = failure && failure.file === active ? failure : null;
  const lines = file.text.split('\n');
  $('highlight').innerHTML = lines
    .map((line, i) => {
      const here = bad && bad.line === i + 1;
      return `<div${here ? ' class="line-bad"' : ''}>${lineHtml(line, here ? bad : null) || ' '}</div>`;
    })
    .join('');
  $('gutter').innerHTML = `<div class="inner">${lines.map((_, i) => `<div${bad && bad.line === i + 1 ? ' class="bad"' : ''}>${i + 1}</div>`).join('')}</div>`;
  syncScroll();
}

function syncScroll() {
  const { scrollTop, scrollLeft } = textarea;
  $('highlight').style.transform = `translate(${-scrollLeft}px, ${-scrollTop}px)`;
  const gutter = $('gutter').firstElementChild;
  if (gutter) gutter.style.transform = `translateY(${-scrollTop}px)`;
}

/** Whether a file's tab offers to remove it: a sheet, or a file you added. The document can't go,
 * and a file that's there because another uses it goes when the `use` line does. */
const removable = (file) => file !== files[0] && (file.root || file.pinned);

function button(className, text, label) {
  const b = document.createElement('button');
  b.type = 'button';
  b.className = className;
  b.textContent = text;
  if (label) b.setAttribute('aria-label', label);
  return b;
}

/** A tab per file. Focus stays on the same control when they're redrawn. */
function paintTabs() {
  const list = $('tabs');
  const focused = list.contains(document.activeElement) ? document.activeElement : null;
  const again = focused && [focused.dataset.path, focused.className];
  const items = files.map((file, i) => {
    const tab = document.createElement('li');
    tab.className = 'tab';
    tab.classList.toggle('active', i === active);
    tab.classList.toggle('edited', file.text !== file.disk);
    tab.classList.toggle('used', !file.root);
    tab.classList.toggle('unused', !!file.unused);
    tab.classList.toggle('failed', failure?.file === i);
    const pick = button('pick', baseName(file.path));
    pick.dataset.path = file.path;
    pick.title = file.unused ? `${file.path}: nothing uses this file yet` : file.path;
    if (i === active) pick.setAttribute('aria-current', 'true');
    // What the dot and the dashes say, for a screen reader.
    const states = [file.text !== file.disk && 'edited', file.unused && 'nothing uses it yet', failure?.file === i && 'has an error'].filter(Boolean);
    if (states.length) {
      const sr = document.createElement('span');
      sr.className = 'sr';
      sr.textContent = `, ${states.join(', ')}`;
      pick.append(sr);
    }
    pick.addEventListener('click', () => show(i));
    pick.addEventListener('keydown', moveAmongTabs);
    tab.append(pick);
    if (removable(file)) {
      const remove = button('remove', '×', `Remove ${baseName(file.path)}`);
      remove.dataset.path = file.path;
      remove.title = `Remove ${file.path}`;
      remove.addEventListener('click', () => removeFile(file.path));
      tab.append(remove);
    }
    return tab;
  });
  const scrolled = list.scrollLeft;
  list.replaceChildren(...items);
  list.scrollLeft = scrolled;
  if (again) [...list.querySelectorAll('button')].find((b) => b.dataset.path === again[0] && b.className === again[1])?.focus();
  const file = files[active];
  // The tabs are one line that scrolls. A file that comes into view brings its tab with it, and
  // otherwise the line stays where the reader left it.
  if (list.dataset.shown !== (file?.path ?? '')) {
    list.dataset.shown = file?.path ?? '';
    const tab = list.querySelector('.tab.active');
    if (tab && tab.offsetLeft < list.scrollLeft) list.scrollLeft = tab.offsetLeft - 24;
    else if (tab && tab.offsetLeft + tab.offsetWidth > list.scrollLeft + list.clientWidth) list.scrollLeft = tab.offsetLeft + tab.offsetWidth - list.clientWidth + 24;
  }
  tabEdges();
  $('path').textContent = file ? `${file.path}${file.text !== file.disk ? ' (edited here, not saved to disk)' : ''}` : '';
  $('revert').disabled = !file || file.text === file.disk;
  $('revert-all').disabled = !files.some((f) => f.text !== f.disk);
}

/** Which ends of the tabs have more past them, for the fade there. */
function tabEdges() {
  const list = $('tabs');
  list.classList.toggle('cut-left', list.scrollLeft > 1);
  list.classList.toggle('cut-right', list.scrollLeft + list.clientWidth < list.scrollWidth - 1);
}
$('tabs').addEventListener('scroll', tabEdges, { passive: true });
new ResizeObserver(tabEdges).observe($('tabs'));
$('add').addEventListener('click', () => askToAdd());

/** Left and right, Home and End, move between the tabs; Enter or Space shows one. */
function moveAmongTabs(e) {
  const picks = [...$('tabs').querySelectorAll('.pick')];
  const at = picks.indexOf(e.currentTarget);
  const to = { ArrowLeft: at - 1, ArrowRight: at + 1, Home: 0, End: picks.length - 1 }[e.key];
  if (to === undefined || e.metaKey || e.ctrlKey || e.altKey) return;
  e.preventDefault();
  picks[(to + picks.length) % picks.length].focus();
}

function show(i, caret) {
  if (inEditor) Object.assign(inEditor, { scroll: [textarea.scrollTop, textarea.scrollLeft], selection: [textarea.selectionStart, textarea.selectionEnd] });
  active = i;
  const file = files[i];
  inEditor = file;
  textarea.value = file.text;
  textarea.setAttribute('aria-label', `Source of ${file.path}`);
  if (caret !== undefined) {
    textarea.focus();
    textarea.setSelectionRange(caret, caret);
    // Put the line a third of the way down.
    const line = file.text.slice(0, caret).split('\n').length - 1;
    textarea.scrollTop = Math.max(0, line * 20 - textarea.clientHeight / 3);
  } else {
    textarea.setSelectionRange(...(file.selection ?? [0, 0]));
    [textarea.scrollTop, textarea.scrollLeft] = file.scroll ?? [0, 0];
  }
  paintTabs();
  paintEditor();
}

/** Where a line and column are in a text, as an offset, for the caret. */
function offsetOf(text, line, col) {
  const lines = text.split('\n');
  let offset = 0;
  for (let i = 0; i < Math.min(line - 1, lines.length); i++) offset += lines[i].length + 1;
  const chars = Array.from(lines[line - 1] ?? '');
  return offset + chars.slice(0, Math.max(0, col - 1)).join('').length;
}

let settle = 0;
textarea.addEventListener('input', () => {
  const file = files[active];
  file.text = textarea.value;
  persist(file);
  forget();
  paintEditor();
  paintTabs();
  clearTimeout(settle);
  settle = setTimeout(compile, SETTLE_MS);
});
textarea.addEventListener('scroll', syncScroll);

// How the expression at the caret reads, as the caret moves or the text changes.
function showReading() {
  const at = textarea.selectionStart === textarea.selectionEnd ? readingAt(textarea.value, textarea.selectionStart) : null;
  const out = $('reading');
  out.replaceChildren();
  if (!at) return;
  const read = document.createElement('b');
  read.textContent = at.read;
  out.append('reads as  ', read);
  out.title = JSON.stringify(at.mathjson);
}
// What a LaTeX paste became, until the next edit.
let pasting = false;
pasteLatex(textarea, (text) => {
  $('pasted').textContent = text;
  pasting = true;
  // A paste that stays as written still edits, after this, in the same task.
  setTimeout(() => (pasting = false));
});
textarea.addEventListener('input', () => {
  if (!pasting) $('pasted').textContent = '';
});
readingReady.then(() => {
  document.addEventListener('selectionchange', () => {
    if (document.activeElement === textarea) showReading();
  });
  textarea.addEventListener('input', showReading);
});
textarea.addEventListener('keydown', (e) => {
  if (e.key === 'Tab' && !e.metaKey && !e.ctrlKey && !e.altKey) {
    // Two spaces, through the browser's own editing so undo still works.
    e.preventDefault();
    document.execCommand('insertText', false, '  ');
  } else if (e.key === 'Escape') {
    textarea.blur();
    figure.focus();
  }
});
$('copy').addEventListener('click', () => navigator.clipboard.writeText(files[active].text));
const revert = (file) => {
  file.text = file.disk;
  persist(file);
};
$('revert').addEventListener('click', () => {
  revert(files[active]);
  forget();
  show(active);
  compile();
});
$('revert-all').addEventListener('click', () => {
  files.forEach(revert);
  forget();
  show(active);
  compile();
});
// Save downloads the file in the active tab, under its own name.
$('save').addEventListener('click', () => {
  const file = files[active];
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([file.text], { type: 'text/plain' }));
  a.download = baseName(file.path);
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
  if (showing === YOURS) {
    file.kept = file.text;
    saveYours();
  }
  say(`Saved ${baseName(file.path)} to your downloads`);
});

// ---- The figure ----

await PencilFigure.ready;
const pick = $('figure-pick');
const stylePick = $('style-pick');

let lastGood = null; // the sources that last drew, to go back to when an edit compiles but can't draw
let compiles = 0;
let compiled = "doesn't compile yet";
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

const info = () => figure.info;
// Beats with lengths count beats done, from 0 to n. Untimed ones say which beat it's on.
const timed = () => (info()?.beatLengths?.length ?? 0) > 0;

const roots = () => files.filter((f) => f.root);
/** What `figure.load` takes: the document, its sheets, and the files they use, by path. */
const sources = () => {
  const [doc, ...sheets] = roots();
  const used = Object.fromEntries(files.filter((f) => !f.root).map((f) => [f.path, f.text]));
  return [doc.text, sheets.map((f) => f.text), { names: roots().map((f) => f.path), files: used }];
};

/** Opens a tab for each file the sources `use`, after the file that uses it, and closes the tab
 * of one that's no longer used. Says whether that changed what's open. */
let following = 0;
async function follow() {
  const turn = ++following;
  const mine = showing === YOURS;
  const next = [];
  const seen = new Set();
  const visit = async (file, reached) => {
    file.unused = !reached;
    next.push(file);
    for (const path of uses(file.path, file.text)) {
      if (seen.has(path)) continue;
      seen.add(path);
      let used = files.find((f) => !f.root && f.path === path) ?? (mine ? spare.get(path) : undefined);
      if (!used) {
        try {
          const disk = await fetchText(path);
          used = mine ? { path, disk, text: disk, kept: disk } : { path, disk, text: stored(path) ?? disk };
        } catch {
          // No such file. Compiling says which line wanted it.
          continue;
        }
      }
      await visit(used, reached);
    }
  };
  for (const root of roots()) await visit(root, true);
  // Your own files stay while nothing uses them, after the rest.
  for (const file of files.filter((f) => f.pinned && !f.root)) {
    if (seen.has(file.path)) continue;
    seen.add(file.path);
    await visit(file, false);
  }
  if (turn !== following) return false;
  if (next.length === files.length && next.every((f, i) => f === files[i])) {
    paintTabs();
    return false;
  }
  if (mine) {
    for (const f of files) if (!next.includes(f)) spare.set(f.path, f);
    for (const f of next) spare.delete(f.path);
  }
  const shown = files[active];
  files = next;
  if (mine) {
    yours.files = files;
    saveYours();
  }
  active = files.indexOf(shown);
  if (active < 0) show(0);
  else paintTabs();
  return true;
}

/** Compiles the editor's sources. A failed compile leaves the last good figure on screen. */
function compile() {
  // An edit can add a `use` or take one away. Once its file is here, or gone, compile again.
  follow().then((changed) => changed && compile());
  const started = performance.now();
  try {
    figure.load(...sources());
  } catch (e) {
    // In a used file, by its name. Otherwise in a sheet, by its place, or in the document.
    const root = roots()[e.sheet === null || e.sheet === undefined ? 0 : e.sheet + 1];
    const file = Math.max(0, files.indexOf(files.find((f) => !f.root && f.path === e.file) ?? root));
    failure = { file, line: e.line ?? 0, col: e.col ?? 0, message: e.text ?? e.message };
    paintErrors();
    paintTabs();
    paintEditor();
    return false;
  }
  const ms = performance.now() - started;
  failure = null;
  warning = null;
  compiles += 1;
  compiled = `compiled ${compiles} time${compiles === 1 ? '' : 's'}, the last in ${ms.toFixed(0)} ms`;
  paintErrors();
  paintTabs();
  paintEditor();
  return true;
}

function paintErrors() {
  const list = $('errors');
  const items = [];
  if (notice) {
    const li = document.createElement('li');
    li.className = 'stale';
    li.textContent = notice;
    items.push(li);
  }
  if (failure) {
    const li = document.createElement('li');
    const where = failure.line ? `${baseName(files[failure.file].path)}:${failure.line}:${failure.col}` : baseName(files[failure.file].path);
    li.textContent = `${where}: ${failure.message}`;
    const note = document.createElement('span');
    note.className = 'note';
    note.textContent = lastGood ? '\nShowing the last figure that compiled.' : '';
    li.append(note);
    li.title = 'Go to it';
    li.addEventListener('click', () => {
      const file = files[failure.file];
      show(failure.file, failure.line ? offsetOf(file.text, failure.line, Math.max(1, failure.col)) : undefined);
    });
    items.push(li);
  }
  if (warning) {
    const li = document.createElement('li');
    li.className = 'stale';
    li.textContent = warning;
    items.push(li);
  }
  list.replaceChildren(...items);
}

// ---- Controls: time, beats and params, the same bindings every page uses ----

/** Builds the controls for what's loaded. A figure that never reads time gets no clock. */
function paintControls() {
  const { size, params: declared, beats = [], usesTime } = info();
  // Its shape, which the page's styles keep it to the window's height with.
  figure.style.setProperty('--shape', String(size[0] / size[1]));
  $('clock').hidden = !usesTime;
  $('time').max = figure.duration;
  $('beat-row').hidden = beats.length === 0;
  const params = $('params');
  params.replaceChildren();
  for (const param of declared) {
    // Beats are stepped, not slid.
    if (param.id === 'beat' && beats.length) continue;
    const label = document.createElement('label');
    // A choice is named for its value, and its param is which option.
    label.textContent = param.choice?.of ?? param.id;
    label.htmlFor = `param-${param.id}`;
    let input;
    if (param.choice) {
      input = document.createElement('select');
      input.id = `param-${param.id}`;
      param.choice.options.forEach((text, i) => input.append(new Option(text, String(i))));
    } else {
      input = document.createElement('input');
      Object.assign(input, { type: 'range', id: `param-${param.id}`, min: param.min, max: param.max, step: param.step ?? ((param.max - param.min) / 1000 || 0.001) });
    }
    const out = document.createElement('output');
    input.addEventListener(param.choice ? 'change' : 'input', () => {
      figure.pause();
      figure.hold(param.id, Number(input.value));
    });
    params.append(label, input, out);
  }
  paintPlay();
}

function paintPlay() {
  $('play').textContent = figure.paused ? 'Play' : 'Pause';
}

/** Reads the params and beat back from the figure, so the controls show what's drawn. */
function readControls() {
  const time = figure.time;
  $('time').value = time;
  $('time-value').textContent = `${time.toFixed(2)} s`;
  const held = figure.held;
  for (const param of info()?.params ?? []) {
    const input = $(`param-${param.id}`);
    if (!input) continue;
    // A range that follows another param is read as it stands.
    if (param.follows?.length) [input.min, input.max] = figure.range(param.id);
    const value = figure.value(param.id);
    if (document.activeElement !== input) input.value = String(value);
    // A stepped param shows as many decimals as its step has, so a whole number shows as one. A
    // choice shows its option in the control itself.
    const decimals = param.step == null ? 3 : (String(param.step).split('.')[1] ?? '').length;
    input.nextElementSibling.textContent = param.choice ? '' : Number.isFinite(value) ? value.toFixed(decimals) : '—';
    input.previousElementSibling.classList.toggle('held', held.has(param.id));
  }
  const beats = info()?.beats ?? [];
  const range = info()?.params.find((p) => p.id === 'beat');
  if (beats.length && range) {
    const beat = Number.isFinite(figure.beat) ? figure.beat : 0;
    const shown = timed() ? beat : beat + 1;
    const whole = Math.abs(shown - Math.round(shown)) < 1e-6;
    $('beat-value').textContent = `${whole ? Math.round(shown) : shown.toFixed(2)} / ${beats.length}`;
    const index = timed() ? Math.min(beats.length - 1, Math.max(0, Math.ceil(beat) - 1)) : Math.round(beat);
    $('caption').textContent = beats[index] ?? '';
    $('back').disabled = beat <= range.min;
    $('next').disabled = beat >= range.max;
  }
}

$('play').addEventListener('click', () => (figure.paused ? figure.play() : figure.pause()));
$('time').addEventListener('input', () => {
  figure.pause();
  figure.time = Number($('time').value);
});
$('back').addEventListener('click', () => figure.step(-1));
$('next').addEventListener('click', () => figure.step(1));
document.addEventListener('keydown', (e) => {
  // The figure takes its own keys when it or one of its handles is focused, and a slider keeps its arrows.
  if (e.defaultPrevented || e.target === figure || e.target === textarea || e.target instanceof HTMLSelectElement || e.target instanceof HTMLInputElement || e.metaKey || e.ctrlKey || e.altKey) return;
  if (e.key === 'ArrowRight') figure.step(1);
  else if (e.key === 'ArrowLeft') figure.step(-1);
  else if (e.key === ' ' && !(e.target instanceof HTMLButtonElement) && info()?.usesTime) figure.paused ? figure.play() : figure.pause();
  else return;
  e.preventDefault();
});

// ---- What the figure says back ----

figure.addEventListener('load', paintControls);
figure.addEventListener('play', paintPlay);
figure.addEventListener('pause', paintPlay);
figure.addEventListener('draw', (e) => {
  if (e.detail.fresh) lastGood = sources();
  readControls();
  $('status').textContent = `${figure.backend} · ${viewing(figure)} · ${compiled}`;
});
figure.addEventListener('error', (e) => {
  const { error, time, fresh } = e.detail;
  // It compiled but can't draw. Go back to what last drew, so the figure stays on screen.
  warning = `It compiled, but drawing failed at ${time.toFixed(2)} s: ${error.message}`;
  if (fresh && lastGood) {
    try {
      figure.load(...lastGood);
      warning += '\nShowing the last figure that drew.';
    } catch {}
  }
  paintErrors();
});

// ---- Loading a figure ----

/** The address names what's open, by the ids the site links with, and holds a link's figure after
 * `#pen=` only while it's what's open. In a frame on the same origin, such as the site's page,
 * that page's address follows too, since it's the one a reader sees and copies. */
function outerWindow() {
  try {
    if (window.top !== window && window.top.location.href) return window.top;
  } catch {}
  return null;
}

function address(link = null) {
  const url = new URL(location.href);
  url.searchParams.set('figure', showing === YOURS ? YOURS : showing.id);
  url.hash = link ? `pen=${link}` : '';
  history.replaceState(null, '', url);
  const outer = outerWindow();
  if (!outer) return url.href;
  const there = new URL(outer.location.href);
  if (link) there.hash = `pen=${link}`;
  else if (there.hash.startsWith('#pen=')) there.hash = '';
  else return url.href;
  outer.history.replaceState(outer.history.state, '', there);
  return there.href;
}

/** The packed figure in this page's address, or the framing page's. */
function linkInAddress() {
  for (const at of [window, outerWindow()]) {
    const hash = at?.location.hash ?? '';
    if (hash.startsWith('#pen=') && hash.length > 5) return hash.slice(5);
  }
  return null;
}

/** Drops a link from the address once what's open has changed from it. */
function forget() {
  if (linkInAddress() && showing) address();
}

function paintMenus() {
  pick.replaceChildren(
    ...(yours ? [new Option(`Yours: ${yours.name}`, YOURS)] : []),
    ...FIGURES.map((doc) => new Option(doc.name, doc.id)),
  );
  pick.value = showing === YOURS ? YOURS : showing.doc.id;
  if (showing === YOURS) {
    const sheets = files.filter((f) => f.root).slice(1).map((f) => styleName(f.path));
    stylePick.replaceChildren(new Option(sheets.join(' + ') || 'no sheets', ''));
    stylePick.disabled = true;
    stylePick.title = 'Your figure’s sheets are its tabs: add or remove them there';
  } else {
    stylePick.replaceChildren(...showing.doc.looks.map((look) => new Option(look.name, look.id)));
    stylePick.value = showing.id;
    stylePick.disabled = showing.doc.looks.length < 2;
    stylePick.title = stylePick.disabled ? 'This figure has one style' : '';
  }
}

/** Shows what's in `files` from the start: its first file, at time 0. */
async function start(link = null) {
  active = 0;
  inEditor = null;
  spare = new Map();
  paintMenus();
  address(link);
  await follow();
  lastGood = null;
  warning = null;
  textarea.value = '';
  show(0);
  compiled = "doesn't compile yet";
  figure.release();
  figure.time = 0;
  compile();
  if (reduced) figure.pause();
  else figure.play();
}

/** Opens an example: a document in one of its looks. */
async function open(look) {
  const paths = [look.doc.pen, ...look.sheets];
  let disks;
  try {
    disks = await Promise.all(paths.map(fetchText));
  } catch (e) {
    warning = String(e);
    paintErrors();
    return;
  }
  showing = look;
  files = paths.map((path, i) => ({ path, disk: disks[i], text: stored(path) ?? disks[i], root: true }));
  await start();
}

/** Puts the open example's document in another of its looks, keeping the document, its edits and
 * the time. */
async function restyle(look) {
  let disks;
  try {
    disks = await Promise.all(look.sheets.map(fetchText));
  } catch (e) {
    warning = String(e);
    paintErrors();
    return;
  }
  const shown = files[active]?.path;
  showing = look;
  files = [files[0], ...look.sheets.map((path, i) => ({ path, disk: disks[i], text: stored(path) ?? disks[i], root: true }))];
  paintMenus();
  address();
  await follow();
  lastGood = null;
  warning = null;
  show(Math.max(0, files.findIndex((f) => f.path === shown)));
  compile();
}

async function openYours(link = null) {
  showing = YOURS;
  files = yours.files;
  await start(link);
}

/** Your figure from a link's sources. A file the document and sheets don't reach is one you
 * added, and stays while nothing uses it. */
function fromSources({ name, doc, sheets, files: texts }, link) {
  const roots = [doc, ...sheets.filter((path) => path !== doc)];
  const reached = new Set();
  const visit = (path) => {
    for (const used of uses(path, texts[path] ?? '')) {
      if (reached.has(used) || !(used in texts)) continue;
      reached.add(used);
      visit(used);
    }
  };
  roots.forEach(visit);
  const file = (path, root) => {
    const text = texts[path];
    return { path, disk: text, text, kept: text, root, pinned: !root && !reached.has(path) };
  };
  const others = Object.keys(texts).filter((path) => !roots.includes(path));
  return {
    name: name || styleName(doc),
    link,
    restructured: false,
    files: [...roots.map((path) => file(path, true)), ...others.map((path) => file(path, false))],
  };
}

/** Opens a link's figure as yours. Says whether it opened one; a link that can't be read leaves
 * its error in `notice`. */
async function openLink(link) {
  if (yours?.link === link) {
    await openYours(link);
    return true;
  }
  let sources;
  try {
    sources = await unpack(link);
  } catch (e) {
    notice = e.message;
    paintErrors();
    return false;
  }
  if (!(await mayReplace())) {
    // Kept: show the one being kept, and drop the link from the address.
    await openYours();
    return true;
  }
  notice = null;
  yours = fromSources(sources, link);
  saveYours();
  await openYours(link);
  return true;
}

/** Whether your figure may be replaced: at once when it has nothing that exists only here, or
 * when the reader says so. */
async function mayReplace() {
  if (!unsaved(yours)) return true;
  $('replace-text').textContent =
    `“${yours.name}” has changes that aren’t in a link or a saved file. Opening another figure of your own replaces it, and they’ll be gone.`;
  return (await modal($('replace-dialog'))) === 'replace';
}

/** Turns an open example into your figure, a copy of every file as it stands, before its files are
 * added or removed. The example is left as it was. Says whether it did, or yours already was. */
async function adopt() {
  if (showing === YOURS) return true;
  if (!(await mayReplace())) return false;
  notice = null;
  const at = active;
  const copy = files.map((f) => ({
    path: f.path, disk: f.text, text: f.text, kept: f.text, root: !!f.root, pinned: false, scroll: f.scroll, selection: f.selection,
  }));
  // The example's file keeps the caret it had; the copy takes it from here.
  if (inEditor) Object.assign(inEditor, { scroll: [textarea.scrollTop, textarea.scrollLeft], selection: [textarea.selectionStart, textarea.selectionEnd] });
  yours = { name: showing.doc.name, link: null, restructured: true, files: copy };
  showing = YOURS;
  files = copy;
  spare = new Map();
  active = at;
  inEditor = files[at];
  saveYours();
  paintMenus();
  address();
  return true;
}

/** After a file is added or removed: what's used, the tabs, the menus, the address, and the figure. */
async function restructure(path) {
  yours.restructured = true;
  await follow();
  yours.files = files;
  saveYours();
  paintMenus();
  address();
  const at = path ? files.findIndex((f) => f.path === path) : -1;
  show(at >= 0 ? at : Math.min(active, files.length - 1));
  compile();
}

// ---- Dialogs ----

/** Opens a modal dialog, and resolves with its `returnValue` once it closes, focus back where it
 * was. A modal dialog keeps the rest of the page out of reach; Tab also wraps within it, so focus
 * never leaves it for the browser's own controls. */
const waiting = new Map(); // dialog → how to settle the one still waiting for its `close`
function modal(dialog) {
  // A `close` comes as a task of its own, so a dialog closed and opened again at once can still
  // have the last one's waiting: settle that first, with what it closed with. Its late `close`
  // then finds the dialog open, and is let go.
  waiting.get(dialog)?.(false);
  const back = document.activeElement;
  dialog.returnValue = '';
  dialog.showModal();
  return new Promise((resolve) => {
    const closed = () => !dialog.open && settle(true);
    const settle = (refocus) => {
      dialog.removeEventListener('close', closed);
      waiting.delete(dialog);
      if (refocus && back?.isConnected) back.focus();
      resolve(dialog.returnValue);
    };
    waiting.set(dialog, settle);
    dialog.addEventListener('close', closed);
  });
}
for (const dialog of document.querySelectorAll('dialog')) {
  dialog.addEventListener('keydown', (e) => {
    if (e.key !== 'Tab') return;
    const stops = [...dialog.querySelectorAll('button, input, select, textarea, a[href]')].filter((el) => !el.disabled);
    const [first, last] = [stops[0], stops.at(-1)];
    if (e.shiftKey && document.activeElement === first) last.focus();
    else if (!e.shiftKey && document.activeElement === last) first.focus();
    else return;
    e.preventDefault();
  });
}

// ---- Adding, removing and opening files ----

/** What's wrong with a name for a new file, or nothing. */
function nameProblem(name) {
  if (!/\.(pen|style)$/.test(name) || /^\.(pen|style)$/.test(name)) return 'The name ends in .style or .pen, as in mine.style.';
  if (/\s|\\|:|^\/|(^|\/)\.\.?(\/|$)|\/\//.test(name)) return 'Name it from this figure’s folder, without spaces, as in mine.style or parts/arm.pen.';
  const path = folderOf(files[0].path) + name;
  if (files.some((f) => f.path === path)) return `There’s already a file named ${name}.`;
  return null;
}

async function askToAdd() {
  const dialog = $('add-dialog');
  const input = $('add-name');
  input.value = '';
  $('add-bad').textContent = '';
  input.removeAttribute('aria-invalid');
  const answer = modal(dialog);
  input.focus();
  if ((await answer) !== 'add') return;
  await addFile(input.value.trim());
}
$('add-form').addEventListener('submit', (e) => {
  const problem = nameProblem($('add-name').value.trim());
  if (!problem) return;
  e.preventDefault();
  $('add-bad').textContent = problem;
  $('add-name').setAttribute('aria-invalid', 'true');
});
$('add-cancel').addEventListener('click', () => $('add-dialog').close('cancel'));

/** Adds an empty file: a `.style` as the last sheet, a `.pen` as one the document can `use`. */
async function addFile(name) {
  if (!(await adopt())) return;
  const path = folderOf(files[0].path) + name;
  const sheet = name.endsWith('.style');
  const text = sheet
    ? '// A sheet of your own. It comes after the others, so its rules win.\n'
    : `// The document can read this file with: use "${name}"\n`;
  files.push({ path, disk: text, text, kept: text, root: sheet, pinned: !sheet });
  await restructure(path);
  textarea.focus();
  textarea.setSelectionRange(text.length, text.length);
}

async function removeFile(path) {
  if (!(await adopt())) return;
  const at = files.findIndex((f) => f.path === path);
  if (at <= 0 || !removable(files[at])) return;
  files.splice(at, 1);
  if (active >= at) active = Math.max(0, active - 1);
  await restructure();
  say(`Removed ${baseName(path)}`);
  // The button that was pressed is gone: focus the tab that's shown now.
  $('tabs').querySelector('.tab.active .pick')?.focus();
}

/** Opens files from the reader's computer. With a `.pen` among them they become your figure: the
 * first `.pen` is the document, other `.pen` files are files it can `use`, and `.style` files are
 * its sheets in name order. Without one, they're added to the figure that's open. */
async function openFiles(list) {
  const read = await Promise.all([...list].map(async (f) => ({ name: f.name, text: await f.text() })));
  const pens = read.filter((f) => f.name.endsWith('.pen'));
  const sheets = read.filter((f) => f.name.endsWith('.style')).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  const skipped = read.length - pens.length - sheets.length;
  if (pens.length) {
    if (!(await mayReplace())) return;
    const file = (f, root) => ({ path: f.name, disk: f.text, text: f.text, kept: f.text, root, pinned: !root });
    const [doc, ...others] = pens;
    notice = null;
    yours = {
      name: styleName(doc.name),
      link: null,
      restructured: false,
      files: [file(doc, true), ...sheets.map((f) => file(f, true)), ...others.map((f) => file(f, false))],
    };
    saveYours();
    await openYours();
  } else if (sheets.length) {
    if (!(await adopt())) return;
    const folder = folderOf(files[0].path);
    for (const f of sheets) {
      const path = folder + f.name;
      const have = files.find((x) => x.path === path);
      if (have) Object.assign(have, { disk: f.text, text: f.text, kept: f.text });
      else files.push({ path, disk: f.text, text: f.text, kept: f.text, root: true, pinned: false });
    }
    await restructure(folder + sheets[0].name);
  }
  if (skipped) say(`Open takes .pen and .style files: ${skipped === 1 ? 'one other was' : `${skipped} others were`} left out`);
}

$('open').addEventListener('click', () => $('open-files').click());
$('open-files').addEventListener('change', async (e) => {
  const list = [...e.target.files];
  e.target.value = '';
  if (list.length) await openFiles(list);
});
// Files dropped on the editor open the same way.
const editor = document.querySelector('.editor');
const carriesFiles = (e) => [...(e.dataTransfer?.types ?? [])].includes('Files');
editor.addEventListener('dragover', (e) => {
  if (!carriesFiles(e)) return;
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
  editor.classList.add('dropping');
});
editor.addEventListener('dragleave', (e) => {
  if (!editor.contains(e.relatedTarget)) editor.classList.remove('dropping');
});
editor.addEventListener('drop', (e) => {
  editor.classList.remove('dropping');
  if (!carriesFiles(e)) return;
  e.preventDefault();
  openFiles(e.dataTransfer.files);
});

// ---- Sharing ----

/** Packs every open file as it stands into the address, and copies the link. */
async function share() {
  const roots = files.filter((f) => f.root);
  const link = await pack({
    name: showing === YOURS ? yours.name : showing.doc.name,
    doc: roots[0].path,
    sheets: roots.slice(1).map((f) => f.path),
    files: Object.fromEntries(files.map((f) => [f.path, f.text])),
  });
  if (showing === YOURS) {
    yours.link = link;
    yours.restructured = false;
    for (const f of files) f.kept = f.text;
    saveYours();
  }
  const href = address(link);
  try {
    await navigator.clipboard.writeText(href);
    say('Link copied');
  } catch {
    say('The link is in the address bar: copy it from there');
  }
}
$('share').addEventListener('click', share);

// ---- What a narrow window keeps behind a button ----

// The buttons of the top bar and of the file in view are each a popover, which a wide window
// shows in place. Opened, one sits under its button, and is worked like a menu: the arrows move
// through it, a choice closes it, and Esc or a touch outside closes it as any popover.
const isOpen = (menu) => {
  try {
    return menu.matches(':popover-open');
  } catch {
    return false;
  }
};
for (const menu of document.querySelectorAll('.menu')) {
  const opener = document.querySelector(`[popovertarget="${menu.id}"]`);
  const items = () => [...menu.querySelectorAll('button:not(:disabled), a[href]')].filter((item) => !item.hidden);
  menu.addEventListener('beforetoggle', (e) => {
    if (e.newState !== 'open') {
      // Focus goes back to the button that opened it. A browser does that only when the button
      // had focus, and Safari's buttons don't take it from a click.
      if (menu.contains(document.activeElement)) opener.focus();
      return;
    }
    const at = opener.getBoundingClientRect();
    menu.style.top = `${at.bottom + 4}px`;
    menu.style.right = `${Math.max(8, innerWidth - at.right)}px`;
  });
  menu.addEventListener('toggle', (e) => {
    if (e.newState === 'open') items()[0]?.focus();
  });
  menu.addEventListener('click', (e) => {
    if (e.target.closest('button, a') && isOpen(menu)) menu.hidePopover();
  });
  menu.addEventListener('keydown', (e) => {
    if (!isOpen(menu) || e.metaKey || e.ctrlKey || e.altKey) return;
    const all = items();
    const at = all.indexOf(document.activeElement);
    const to = { ArrowUp: at - 1, ArrowDown: at + 1, Home: 0, End: all.length - 1 }[e.key];
    if (to === undefined || !all.length) return;
    e.preventDefault();
    all[(to + all.length) % all.length].focus();
  });
}
// A narrow window has the figure over the editor, and a wide one the editor beside it and first.
// The styles place them, and this puts them in the page in the order they're seen, so they're
// read and reached with Tab in that order. The editor is the one moved, since moving the figure
// would start it again, and it keeps its place, its selection and the focus.
const narrow = matchMedia('(max-width: 800px)');
function arrange() {
  const app = document.querySelector('.app');
  const editor = app.querySelector('.editor');
  const view = app.querySelector('.view');
  const before = narrow.matches ? null : view;
  if (editor.nextElementSibling === before) return;
  const focused = editor.contains(document.activeElement) ? document.activeElement : null;
  const kept = [textarea.scrollTop, textarea.scrollLeft, textarea.selectionStart, textarea.selectionEnd, textarea.selectionDirection];
  const tabsAt = $('tabs').scrollLeft;
  app.insertBefore(editor, before);
  textarea.setSelectionRange(kept[2], kept[3], kept[4]);
  textarea.scrollTop = kept[0];
  textarea.scrollLeft = kept[1];
  $('tabs').scrollLeft = tabsAt;
  focused?.focus({ preventScroll: true });
}
arrange();
narrow.addEventListener('change', () => {
  // A window that's widened shows the menus' buttons in place, so nothing is left open over it.
  for (const menu of document.querySelectorAll('.menu')) if (isOpen(menu)) menu.hidePopover();
  arrange();
});

// Framed by another page, such as the site's, it offers itself alone, at the address it has now.
if (window.top !== window) {
  const alone = $('alone');
  const here = () => (alone.href = location.href);
  alone.hidden = false;
  here();
  for (const event of ['pointerdown', 'focus', 'click']) alone.addEventListener(event, here);
}

// ---- The menus and the address ----

// A page that frames this one hears the Figure menu's `change` to follow the address. A style
// changes the address too, so it hears one then as well.
let echoing = false;
pick.addEventListener('change', async () => {
  if (echoing) return;
  notice = null;
  if (pick.value === YOURS) await openYours();
  else await open(FIGURES.find((doc) => doc.id === pick.value).looks[0]);
});
stylePick.addEventListener('change', async () => {
  await restyle(LOOKS.find((look) => look.id === stylePick.value));
  echoing = true;
  pick.dispatchEvent(new Event('change'));
  echoing = false;
});

// Another link pasted into this tab, or the framing page's, opens in place. One that can't be read
// says so and leaves what's open.
async function hashChanged() {
  const link = linkInAddress();
  if (!link || (showing === YOURS && yours?.link === link)) return;
  await openLink(link);
}
window.addEventListener('hashchange', hashChanged);
outerWindow()?.addEventListener('hashchange', hashChanged);

yours = loadYours();
const asked = query.get('figure');
showing = LOOKS.find((look) => look.id === asked) ?? LOOKS.find((look) => look.id === 'explainer');
const link = linkInAddress();
if (!(link && (await openLink(link)))) {
  if (asked === YOURS && yours) await openYours();
  else await open(showing);
}
window.pencil = { figure, compile, files: () => files, yours: () => yours };
