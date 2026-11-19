// The keyboard lab: a figure over an editor of its sources, with three ways of typing them, to try
// on a phone. The figure is a `<pencil-figure>`, compiled again as the text changes, as the
// playground does it (web/playground.js). The shell that keeps the page still and the keys are the
// playground's own files, web/viewport.js and web/keyboard.js, so what's tried here is what it has.
import { PencilFigure } from '@pencil-lang/figure';
import { ready as symbolsReady, typeSymbols, finishSymbol } from '../../web/symbols.js';
import { keyboard } from '../../web/keyboard.js';
import { holdToViewport } from '../../web/viewport.js';

const $ = (id) => document.getElementById(id);
const app = $('app');
const textarea = $('source');
const figure = $('figure');
const query = new URLSearchParams(location.search);
if (query.has('gl')) figure.setAttribute('gl', '');
const numbers = $('numbers');

/** A, B or C: the system keyboard with symbols over it, the Pencil keys, and those with words. */
let variant = (query.get('keys') ?? 'c').toLowerCase();
if (!['a', 'b', 'c'].includes(variant)) variant = 'c';
const ways = { a: { keys: false, words: false }, b: { keys: true, words: false }, c: { keys: true, words: true } };
let keys = null;

// ---- The shell: the app is the visual viewport (web/viewport.js) ----

const vv = window.visualViewport;
holdToViewport(app, { pin: query.get('pin') !== 'off' });
for (const target of [vv, window]) target?.addEventListener('resize', () => paintNumbers());

// What the browser says about its viewports, for trying this on a phone.
function paintNumbers() {
  if (numbers.hidden) return;
  const r = (n) => Math.round(n);
  const box = figure.getBoundingClientRect();
  const s = keys?.state;
  const mode = !s ? '' : !s.keys || s.system ? 'system' : s.shown ? 'pencil' : 'none shown';
  numbers.textContent = [
    `layout  ${innerWidth}×${innerHeight}  scrollY ${r(scrollY)}`,
    vv ? `visual  ${r(vv.width)}×${r(vv.height)} at ${r(vv.offsetTop)}  ×${vv.scale.toFixed(2)}` : 'visual  (no visualViewport)',
    `100dvh  ${$('probe-dvh').offsetHeight}  app ${r(app.offsetHeight)}`,
    `drawn   ${r(box.width)}×${r(box.height)}`,
    `keys    ${mode}  inputmode ${textarea.getAttribute('inputmode') ?? 'text'}`,
  ].join('\n');
}
setInterval(paintNumbers, 500);

// ---- Figures and their files ----

const FIGURES = [
  { id: 'circle', name: '001 circle', pen: 'corpus/001-circle/circle.pen', sheets: ['corpus/001-circle/circle.style'] },
  { id: 'tangents', name: '005 tangents', pen: 'corpus/005-tangents/tangents.pen', sheets: ['corpus/theme.style', 'corpus/005-tangents/plain.style'] },
  { id: 'limacon', name: '012 limaçon', pen: 'corpus/012-limacon/limacon.pen', sheets: ['corpus/012-limacon/plain.style'] },
];
const STORE = 'pencil-keyboard-lab:';
const SETTLE_MS = 120;
const baseName = (path) => path.split('/').pop();

let shown = FIGURES.find((f) => f.id === query.get('figure')) ?? FIGURES[0];
let files = [];
let active = 0;
let failure = null;

const fetchText = async (path) => {
  const response = await fetch(`../../${path}`, { cache: 'no-store' });
  if (!response.ok) throw new Error(`${path}: ${response.status}`);
  return response.text();
};
const stored = (path) => {
  try { return localStorage.getItem(STORE + path); } catch { return null; }
};
function store(file) {
  try {
    if (file.text === file.disk) localStorage.removeItem(STORE + file.path);
    else localStorage.setItem(STORE + file.path, file.text);
  } catch {}
}

async function open(fig) {
  const paths = [fig.pen, ...fig.sheets];
  const disks = await Promise.all(paths.map(fetchText));
  shown = fig;
  files = paths.map((path, i) => ({ path, disk: disks[i], text: stored(path) ?? disks[i], selection: [0, 0], scroll: 0 }));
  $('file').replaceChildren(...files.map((f, i) => new Option(baseName(f.path), String(i))));
  showFile(0);
  address();
  compile();
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) figure.pause();
  else figure.play();
}

function showFile(i) {
  const was = files[active];
  if (was && textarea.value === was.text) Object.assign(was, { selection: [textarea.selectionStart, textarea.selectionEnd], scroll: textarea.scrollTop });
  active = i;
  const file = files[i];
  textarea.value = file.text;
  textarea.setAttribute('aria-label', `Source of ${file.path}`);
  textarea.setSelectionRange(...file.selection);
  textarea.scrollTop = file.scroll;
  $('file').value = String(i);
  keys?.refresh();
}

function compile() {
  try {
    figure.load(files[0].text, files.slice(1).map((f) => f.text), { names: files.map((f) => f.path), files: {} });
    failure = null;
  } catch (e) {
    const file = e.sheet === null || e.sheet === undefined ? 0 : e.sheet + 1;
    failure = { file, line: e.line ?? 0, col: e.col ?? 0, message: e.text ?? e.message };
  }
  const out = $('error');
  out.textContent = failure ? `${baseName(files[failure.file].path)}:${failure.line}:${failure.col}: ${failure.message}` : '';
  out.title = failure ? 'Go to it' : '';
}

$('error').addEventListener('click', () => {
  if (!failure) return;
  if (failure.file !== active) showFile(failure.file);
  const lines = textarea.value.split('\n');
  let at = 0;
  for (let i = 0; i < Math.min(failure.line - 1, lines.length); i++) at += lines[i].length + 1;
  at += Math.max(0, failure.col - 1);
  textarea.focus({ preventScroll: true });
  textarea.setSelectionRange(at, at);
});

figure.addEventListener('load', () => {
  const size = figure.info?.size;
  if (size) figure.style.setProperty('--shape', String(size[0] / size[1]));
});

let settle = 0;
textarea.addEventListener('input', () => {
  const file = files[active];
  file.text = textarea.value;
  store(file);
  clearTimeout(settle);
  settle = setTimeout(compile, SETTLE_MS);
});

// ---- The keys (web/keyboard.js) ----

keys = keyboard({
  textarea,
  root: $('keyboard'),
  opener: $('keys-open'),
  remember: 'pencil-keys-lab',
  files: () => files,
  path: () => files[active]?.path ?? '',
  ...ways[variant],
  leave: () => figure.focus(),
});

// What the playground does as a system keyboard types: `\theta` and a space is θ.
const note = () => {};
typeSymbols(textarea, note);
textarea.addEventListener('keydown', (e) => {
  if (e.key === 'Tab' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
    e.preventDefault();
    if (!finishSymbol(textarea, note)) document.execCommand('insertText', false, '  ');
  } else if (e.key === 'Escape') {
    if (keys.escape()) return e.preventDefault();
    textarea.blur();
    figure.focus();
  }
});

function address() {
  const url = new URL(location.href);
  url.searchParams.set('keys', variant);
  url.searchParams.set('figure', shown.id);
  if ($('show-numbers').checked) url.searchParams.set('numbers', '');
  else url.searchParams.delete('numbers');
  history.replaceState(null, '', url.href.replace('numbers=&', 'numbers&').replace(/numbers=$/, 'numbers'));
}

$('variant').value = variant;
$('variant').addEventListener('change', () => {
  variant = $('variant').value;
  keys.set({ ...ways[variant], system: false });
  address();
});

// ---- The bar and its menu ----

$('file').addEventListener('change', () => showFile(Number($('file').value)));
$('figure-pick').replaceChildren(...FIGURES.map((f) => new Option(f.name, f.id)));
$('figure-pick').addEventListener('change', async () => {
  await open(FIGURES.find((f) => f.id === $('figure-pick').value));
  $('menu').hidePopover?.();
});
$('show-numbers').checked = query.has('numbers');
numbers.hidden = !query.has('numbers');
$('show-numbers').addEventListener('change', () => {
  numbers.hidden = !$('show-numbers').checked;
  paintNumbers();
  address();
});
$('revert').addEventListener('click', () => {
  for (const f of files) {
    f.text = f.disk;
    store(f);
  }
  showFile(active);
  compile();
  $('menu').hidePopover?.();
});
const menu = $('menu');
menu.addEventListener('beforetoggle', (e) => {
  if (e.newState !== 'open') {
    if (menu.contains(document.activeElement)) $('more').focus();
    return;
  }
  const at = $('more').getBoundingClientRect();
  menu.style.top = `${at.bottom + 4}px`;
  menu.style.right = `${Math.max(8, innerWidth - at.right)}px`;
});
menu.addEventListener('toggle', (e) => {
  if (e.newState === 'open') menu.querySelector('select, input, button')?.focus();
});

// ---- Start ----

await Promise.all([PencilFigure.ready, symbolsReady]);
$('figure-pick').value = shown.id;
await open(shown);
window.lab = { keys, files: () => files, compile };
