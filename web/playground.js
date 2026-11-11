// The playground: a figure's `.pen` and its sheets in an editor beside the figure, compiled again
// as they change. Built on the same host as every other page (`Player`), and nothing here works out
// a value of the figure's own: the page only sets and reads bindings.
import init, { Player } from './pkg/pencil_web.js';
import { LabelLayer } from './labels.js';
import { fit, follow } from './fit.js';

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

// How long time runs before it loops, for a figure that declares no `duration`.
const LOOP = 10;
// How long typing has to stop before compiling: short enough to feel live, long enough not to
// compile a figure like the lattice on every key.
const SETTLE_MS = 120;
const STORE = 'pencil-playground:';

const $ = (id) => document.getElementById(id);
const query = new URLSearchParams(location.search);
const dpr = window.devicePixelRatio || 1;
const canvas = $('figure');
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
    canvas.focus();
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

await init();
// The first figure's sheets shape the canvas before the player takes it, as every page does.
const pick = $('figure-pick');
for (const figure of FIGURES) pick.add(new Option(figure.name, figure.id));
const chosen = FIGURES.find((f) => f.id === query.get('figure')) ?? FIGURES.find((f) => f.id === 'explainer');
pick.value = chosen.id;
fit(canvas, [16, 9]);
const player = await Player.create(canvas, query.has('gl'));
const layer = new LabelLayer($('stage'), dpr);
player.set_measure(layer.measure);

let info = null;
let lastGood = null; // the sources that last drew, to go back to when an edit compiles but can't draw
let fresh = false; // whether the loaded figure has yet to draw
let time = 0;
let last = performance.now();
let playing = !matchMedia('(prefers-reduced-motion: reduce)').matches;
let dirty = true;
let compiles = 0;
let compiled = "doesn't compile yet";
// Params a reader has taken over, by name, and kept across compiles.
const held = new Map();

const duration = () => info?.duration ?? LOOP;
const timed = () => (info?.beatLengths?.length ?? 0) > 0;

function shape(size) {
  // A tall figure fits the window's height rather than running off it.
  $('stage').style.maxWidth = `calc((100vh - 220px) * ${size[0] / size[1]})`;
  fit(canvas, size);
  player.resize(canvas.width, canvas.height);
}

/** Compiles the editor's sources. A failed compile leaves the last good figure on screen. */
function compile() {
  const [doc, ...sheets] = files.map((f) => f.text);
  const started = performance.now();
  try {
    player.load(doc, sheets);
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
  fresh = true;
  compiles += 1;
  afterLoad();
  compiled = `compiled ${compiles} time${compiles === 1 ? '' : 's'}, the last in ${ms.toFixed(0)} ms`;
  paintErrors();
  paintTabs();
  paintEditor();
  return true;
}

/** Brings the page up to what's loaded: its shape, its params, and what the reader held. */
function afterLoad() {
  info = JSON.parse(player.info());
  shape(info.size);
  for (const [name, value] of held) {
    if (info.params.some((p) => p.id === name)) player.set_param(name, value);
    else held.delete(name);
  }
  if (time > duration()) time %= duration();
  paintControls();
  dirty = true;
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

function paintControls() {
  const slider = $('time');
  slider.max = duration();
  const params = $('params');
  params.replaceChildren();
  const beats = info.beats ?? [];
  $('beat-row').hidden = beats.length === 0;
  for (const param of info.params) {
    // An untimed figure's beats are stepped, not slid.
    if (param.id === 'beat' && beats.length) continue;
    const label = document.createElement('label');
    label.textContent = param.id;
    label.htmlFor = `param-${param.id}`;
    const input = document.createElement('input');
    Object.assign(input, { type: 'range', id: `param-${param.id}`, min: param.min, max: param.max, step: param.step ?? ((param.max - param.min) / 1000 || 0.001) });
    const out = document.createElement('output');
    input.addEventListener('input', () => {
      setPlaying(false);
      held.set(param.id, Number(input.value));
      player.set_param(param.id, Number(input.value));
      dirty = true;
    });
    params.append(label, input, out);
  }
}

/** Reads the params and beat back from the figure, so the controls show what's drawn. */
function readControls() {
  $('time').value = time;
  $('time-value').textContent = `${time.toFixed(2)} s`;
  for (const param of info?.params ?? []) {
    const input = $(`param-${param.id}`);
    if (!input) continue;
    let value;
    try { value = player.value(param.id, time); } catch { continue; }
    if (document.activeElement !== input) input.value = value;
    // A stepped param shows as many decimals as its step has, so a whole number shows as one.
    const decimals = param.step == null ? 3 : (String(param.step).split('.')[1] ?? '').length;
    input.nextElementSibling.textContent = Number.isFinite(value) ? value.toFixed(decimals) : '—';
    input.previousElementSibling.classList.toggle('held', held.has(param.id));
  }
  const beats = info?.beats ?? [];
  if (beats.length) {
    let beat = 0;
    try { beat = player.value('beat', time); } catch {}
    // A timed figure's `beat` counts beats done, from 0 to n; an untimed one's is which beat it's on.
    const shown = timed() ? beat : beat + 1;
    const whole = Math.abs(shown - Math.round(shown)) < 1e-6;
    $('beat-value').textContent = `${whole ? Math.round(shown) : shown.toFixed(2)} / ${beats.length}`;
    const index = timed() ? Math.min(beats.length - 1, Math.max(0, Math.ceil(beat) - 1)) : Math.round(beat);
    $('caption').textContent = beats[index] ?? '';
    $('back').disabled = beat <= 0;
    $('next').disabled = beat >= (timed() ? beats.length : beats.length - 1);
  }
}

function setPlaying(on) {
  if (on === playing) return;
  playing = on;
  $('play').textContent = on ? 'Pause' : 'Play';
  if (on) {
    // Hand every param back to time, and carry on from where the reader left the one they moved.
    const moved = [...held].find(([name]) => name !== 'beat' || timed());
    if (moved) {
      try { time = player.time_for(moved[0], moved[1]); } catch {}
    }
    for (const param of info?.params ?? []) {
      if (param.id === 'beat' && !timed() && held.has('beat')) continue;
      player.release_param(param.id);
      held.delete(param.id);
    }
  }
  last = performance.now();
  dirty = true;
}

/** Steps a beat. A timed figure seeks to the moment that beat is done, so its beats stay bound to
 * time; an untimed one sets `beat`, as a reader stepping through does. */
function step(by) {
  const beats = info?.beats ?? [];
  if (!beats.length) return;
  setPlaying(false);
  let beat = 0;
  try { beat = player.value('beat', time); } catch {}
  if (timed()) {
    const target = Math.max(0, Math.min(beats.length, by > 0 ? Math.floor(beat + 1e-6) + 1 : Math.ceil(beat - 1e-6) - 1));
    // Timed beats only count up, so the moment one is done is found exactly by halving.
    // `time_for` samples, and can land just short of it.
    let [lo, hi] = [0, duration()];
    for (let k = 0; k < 40; k++) {
      const mid = (lo + hi) / 2;
      if (player.value('beat', mid) >= target - 1e-9) hi = mid;
      else lo = mid;
    }
    time = hi;
  } else {
    const target = Math.max(0, Math.min(beats.length - 1, Math.round(beat) + by));
    held.set('beat', target);
    player.set_param('beat', target);
  }
  dirty = true;
}

$('play').addEventListener('click', () => setPlaying(!playing));
$('time').addEventListener('input', () => {
  setPlaying(false);
  time = Number($('time').value);
  // Scrubbing is time's, so a timed figure's beats follow it again.
  if (timed()) {
    held.delete('beat');
    player.release_param('beat');
  }
  dirty = true;
});
$('back').addEventListener('click', () => step(-1));
$('next').addEventListener('click', () => step(1));
document.addEventListener('keydown', (e) => {
  if (e.target === textarea || e.target instanceof HTMLSelectElement || e.metaKey || e.ctrlKey || e.altKey) return;
  if (e.key === 'ArrowRight') step(1);
  else if (e.key === 'ArrowLeft') step(-1);
  else if (e.key === ' ' && !(e.target instanceof HTMLButtonElement)) setPlaying(!playing);
  else return;
  e.preventDefault();
});

// Handles: a drag sets its param through the figure's own inverse, as on the bounds page.
const device = (e) => {
  const box = canvas.getBoundingClientRect();
  return [(e.clientX - box.left) * (canvas.width / box.width), (e.clientY - box.top) * (canvas.height / box.height)];
};
canvas.addEventListener('pointerdown', (e) => {
  const [x, y] = device(e);
  try {
    if (player.grab(time, x, y, 24 * dpr)) {
      setPlaying(false);
      canvas.setPointerCapture(e.pointerId);
      canvas.style.cursor = 'grabbing';
      player.drag(time, x, y);
      dirty = true;
    }
  } catch {}
});
canvas.addEventListener('pointermove', (e) => {
  const [x, y] = device(e);
  try {
    if (!canvas.hasPointerCapture(e.pointerId)) {
      canvas.style.cursor = player.grab(time, x, y, 24 * dpr) ? 'grab' : '';
      player.release();
      return;
    }
    player.drag(time, x, y);
    dirty = true;
  } catch {}
});
const end = (e) => {
  if (canvas.hasPointerCapture(e.pointerId)) canvas.releasePointerCapture(e.pointerId);
  player.release();
  canvas.style.cursor = '';
};
canvas.addEventListener('pointerup', end);
canvas.addEventListener('pointercancel', end);

// ---- Drawing ----

const paint = () => {
  dirty = false;
  try {
    layer.update(JSON.parse(player.draw(time, dpr)));
    if (fresh) {
      fresh = false;
      lastGood = files.map((f) => f.text);
    }
  } catch (e) {
    // It compiled but can't draw. Go back to what last drew, so the figure stays on screen.
    warning = `It compiled, but drawing failed at ${time.toFixed(2)} s: ${e.message}`;
    if (fresh && lastGood) {
      fresh = false;
      try {
        const [doc, ...sheets] = lastGood;
        player.load(doc, sheets);
        afterLoad();
        warning += '\nShowing the last figure that drew.';
      } catch {}
    } else {
      setPlaying(false);
    }
    paintErrors();
  }
  readControls();
  $('status').textContent = `${player.backend}, ${canvas.width}×${canvas.height}, ${compiled}`;
};

const frame = (now) => {
  if (playing) {
    time = (time + (now - last) / 1000) % duration();
    dirty = true;
  }
  last = now;
  if (dirty) paint();
  layer.tick(now);
  requestAnimationFrame(frame);
};

// ---- Loading a figure ----

async function open(figure) {
  const paths = [figure.pen, ...figure.sheets];
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
  held.clear();
  lastGood = null;
  info = null;
  warning = null;
  time = 0;
  textarea.value = '';
  show(0);
  compiled = "doesn't compile yet";
  compile();
  setPlaying(!matchMedia('(prefers-reduced-motion: reduce)').matches);
}

pick.addEventListener('change', () => {
  const figure = FIGURES.find((f) => f.id === pick.value);
  const url = new URL(location.href);
  url.searchParams.set('figure', figure.id);
  history.replaceState(null, '', url);
  open(figure);
});

await open(chosen);
requestAnimationFrame((now) => {
  last = now;
  frame(now);
});
follow(canvas, player, paint);
window.pencil = { player, layer, compile, paint, files: () => files, redraw() { dirty = true; } };
