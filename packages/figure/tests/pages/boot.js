// Shared by the fixture pages. It records the figures' events on `window.__pencil` so a test can
// poll them, defines the element, and takes `?gl` from the query to skip WebGPU.
const log = (window.__pencil = { load: 0, draw: 0, resize: 0, play: 0, pause: 0, input: [], errors: [] });

// `load` and `draw` don't bubble, but a capturing listener on the document still sees them.
for (const type of ['load', 'draw', 'resize', 'play', 'pause']) {
  document.addEventListener(type, (e) => { if (e.target?.localName === 'pencil-figure') log[type] += 1; }, true);
}
document.addEventListener('input', (e) => { if (e.target?.localName === 'pencil-figure') log.input.push(e.detail); }, true);
document.addEventListener('error', (e) => {
  if (e.target?.localName === 'pencil-figure') log.errors.push(String(e.detail?.error?.message ?? e.detail?.error));
}, true);

await import('../../src/element.js');

if (new URLSearchParams(location.search).has('gl')) {
  for (const figure of document.querySelectorAll('pencil-figure')) figure.setAttribute('gl', '');
}
