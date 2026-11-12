// The playground: a figure's `.pen` and its sheets in an editor beside the figure, compiled again
// as they change. The figure is a `<pencil-figure>`, which owns the GPU, the clock and the handles,
// and nothing here works out a value of the figure's own: the page only sets and reads bindings.
import { PencilFigure } from './pencil-figure.js';

// Each figure with the sheets its own page or video loads, from the repo root, in order.
const THEME = 'corpus/theme.style';
const FIGURES = [
  ['circle', '001 circle', 'corpus/001-circle/circle.pen', ['corpus/001-circle/circle.style']],
  ['circle-bands', '001 circle, bands', 'corpus/001-circle/circle.pen', ['corpus/001-circle/bands.style']],
  ['ring', '002 strokes, ring', 'corpus/002-strokes/ring.pen', ['corpus/002-strokes/filled.style']],
  ['wave', '002 strokes, wave', 'corpus/002-strokes/wave.pen', ['corpus/002-strokes/taper.style']],
  ['unroll', '003 unroll', 'corpus/003-unroll/unroll.pen', ['corpus/003-unroll/blog.style']],
  ['unroll-night', '003 unroll, night', 'corpus/003-unroll/unroll.pen', ['corpus/003-unroll/night.style']],
  ['bounds', '004 bounds', 'corpus/004-bounds/bounds.pen', ['corpus/004-bounds/blog.style']],
  ['tangents', '005 tangents', 'corpus/005-tangents/tangents.pen', [THEME, 'corpus/005-tangents/plain.style']],
  ['square', '006 square', 'corpus/006-square/square.pen', [THEME, 'corpus/006-square/plain.style']],
  ['lattice', '007 lattice', 'corpus/007-lattice/lattice.pen', [THEME, 'corpus/007-lattice/plain.style']],
  ['hearts', '008 hearts', 'corpus/008-hearts/hearts.pen', [THEME, 'corpus/008-hearts/plain.style']],
  ['squaring', '009 squaring', 'corpus/009-squaring/squaring.pen', [THEME, 'corpus/009-squaring/plain.style']],
  ['mobius', '010 Möbius', 'corpus/010-mobius/mobius.pen', [THEME, 'corpus/010-mobius/plain.style']],
  ['circumcircle', '011 circumcircle', 'corpus/011-circumcircle/circumcircle.pen', ['corpus/011-circumcircle/plain.style']],
  ['limacon', '012 limaçon', 'corpus/012-limacon/limacon.pen', ['corpus/012-limacon/plain.style']],
  ['explainer', 'Explainer: limaçon', 'lab/explainer/limacon.pen',
    [THEME, 'corpus/dark.style', 'lab/explainer/limacon.style', 'lab/explainer/caption.style']],
].map(([id, name, pen, sheets]) => ({ id, name, pen, sheets }));

// How long typing has to stop before compiling: short enough to feel live, long enough not to
// compile a figure like the lattice on every key.
const SETTLE_MS = 120;
const STORE = 'pencil-playground:';

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

/** One file: what's on disk, and what's being edited. Edits are kept per path, so a shared sheet
 * like `theme.style` carries its edits to every figure that loads it. */
let files = [];
let active = 0;
let failure = null; // { file, line, col, message }
let warning = null; // a draw error, which has no place in a file

const stored = (path) => localStorage.getItem(STORE + path);
const store = (file) => {
  if (file.text === file.disk) localStorage.removeItem(STORE + file.path);
  else localStorage.setItem(STORE + file.path, file.text);
};
const baseName = (path) => path.split('/').pop();

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

function paintTabs() {
  const tabs = $('tabs');
  tabs.replaceChildren(
    ...files.map((file, i) => {
      const tab = document.createElement('button');
      tab.type = 'button';
      tab.className = 'tab';
      tab.role = 'tab';
      tab.textContent = baseName(file.path);
      tab.title = file.path;
      tab.setAttribute('aria-selected', String(i === active));
      tab.classList.toggle('edited', file.text !== file.disk);
      tab.classList.toggle('failed', failure?.file === i);
      tab.addEventListener('click', () => show(i));
      return tab;
    }),
  );
  const file = files[active];
  $('path').textContent = file ? `${file.path}${file.text !== file.disk ? ' (edited here, not saved to disk)' : ''}` : '';
  $('revert').disabled = !file || file.text === file.disk;
  $('revert-all').disabled = !files.some((f) => f.text !== f.disk);
}

function show(i, caret) {
  const old = files[active];
  if (old) Object.assign(old, { scroll: [textarea.scrollTop, textarea.scrollLeft], selection: [textarea.selectionStart, textarea.selectionEnd] });
  active = i;
  const file = files[i];
  textarea.value = file.text;
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
  store(file);
  paintEditor();
  paintTabs();
  clearTimeout(settle);
  settle = setTimeout(compile, SETTLE_MS);
});
textarea.addEventListener('scroll', syncScroll);
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
  store(file);
};
$('revert').addEventListener('click', () => {
  revert(files[active]);
  show(active);
  compile();
});
$('revert-all').addEventListener('click', () => {
  files.forEach(revert);
  show(active);
  compile();
});

// ---- The figure ----

await PencilFigure.ready;
const pick = $('figure-pick');
for (const entry of FIGURES) pick.add(new Option(entry.name, entry.id));
const chosen = FIGURES.find((f) => f.id === query.get('figure')) ?? FIGURES.find((f) => f.id === 'explainer');
pick.value = chosen.id;

let lastGood = null; // the sources that last drew, to go back to when an edit compiles but can't draw
let compiles = 0;
let compiled = "doesn't compile yet";
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

const info = () => figure.info;
// Beats with lengths count beats done, from 0 to n. Untimed ones say which beat it's on.
const timed = () => (info()?.beatLengths?.length ?? 0) > 0;

/** Compiles the editor's sources. A failed compile leaves the last good figure on screen. */
function compile() {
  const [doc, ...sheets] = files.map((f) => f.text);
  const started = performance.now();
  try {
    figure.load(doc, sheets);
  } catch (e) {
    const file = e.sheet === null || e.sheet === undefined ? 0 : e.sheet + 1;
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
  // A tall figure fits the window's height rather than running off it.
  figure.style.maxWidth = `calc((100vh - 220px) * ${size[0] / size[1]})`;
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
  if (e.target === textarea || e.target instanceof HTMLSelectElement || e.metaKey || e.ctrlKey || e.altKey) return;
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
  if (e.detail.fresh) lastGood = files.map((f) => f.text);
  readControls();
  const canvas = figure.shadowRoot.querySelector('canvas');
  $('status').textContent = `${figure.backend}, ${canvas.width}×${canvas.height}, ${compiled}`;
});
figure.addEventListener('error', (e) => {
  const { error, time, fresh } = e.detail;
  // It compiled but can't draw. Go back to what last drew, so the figure stays on screen.
  warning = `It compiled, but drawing failed at ${time.toFixed(2)} s: ${error.message}`;
  if (fresh && lastGood) {
    try {
      const [doc, ...sheets] = lastGood;
      figure.load(doc, sheets);
      warning += '\nShowing the last figure that drew.';
    } catch {}
  }
  paintErrors();
});

// ---- Loading a figure ----

async function open(entry) {
  const paths = [entry.pen, ...entry.sheets];
  let disks;
  try {
    disks = await Promise.all(paths.map(fetchText));
  } catch (e) {
    warning = String(e);
    paintErrors();
    return;
  }
  files = paths.map((path, i) => {
    const text = stored(path) ?? disks[i];
    return { path, disk: disks[i], text };
  });
  active = 0;
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

pick.addEventListener('change', () => {
  const entry = FIGURES.find((f) => f.id === pick.value);
  const url = new URL(location.href);
  url.searchParams.set('figure', entry.id);
  history.replaceState(null, '', url);
  open(entry);
});

await open(chosen);
window.pencil = { figure, compile, files: () => files };
