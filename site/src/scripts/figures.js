// What the site's pages do around a `<pencil-figure>` that isn't the figure's own: a choice between
// a demo's looks, saying why a figure couldn't be drawn, `?gl`, fitting a tall figure to the
// window, and starting one at a moment. The controls are the package's (`components/Figure.astro`).
//
// A choice of looks is a `<select data-look data-for="id" data-looks="…">`, whose looks are a JSON
// list of `{ name, compiled }` or `{ name, src, sheets }`, with the `<source>`s each brings, as
// `{ media, compiled }` or `{ media, src, sheets }`. Picking one says `look`, with it, on the select.

// `?gl` on the page's address draws every figure with WebGL2, as on the pages in `web/`.
if (new URLSearchParams(location.search).has('gl')) {
  for (const figure of document.querySelectorAll('pencil-figure')) figure.setAttribute('gl', '');
}

document.addEventListener('change', (e) => {
  const select = e.target;
  if (!(select instanceof HTMLSelectElement) || !select.matches('[data-look]')) return;
  const figure = document.getElementById(select.dataset.for);
  const look = JSON.parse(select.dataset.looks ?? '[]')[Number(select.value)];
  if (!figure || !look) return;
  // A build gives each look as a compiled figure, and `astro dev` as its sources.
  for (const name of ['src', 'sheets', 'compiled']) {
    if (look[name] == null) figure.removeAttribute(name);
    else figure.setAttribute(name, look[name]);
  }
  // The figure's `<source>`s are for the look it had, and one whose media matches is used before
  // the figure's own attributes: a reader in a dark theme would keep the first look's dark sheets
  // whatever they picked. So this look's take their place.
  for (const source of figure.querySelectorAll(':scope > source')) source.remove();
  figure.prepend(
    ...(look.sources ?? []).map((each) => {
      const source = document.createElement('source');
      for (const [name, value] of Object.entries(each)) source.setAttribute(name, value);
      return source;
    }),
  );
  // A page that sizes the figure by its sheets has their sizes for each look, under the same media
  // queries as its sources (components/Figure.astro), and this look's replace them.
  const sizes = document.querySelector(`style[data-sizes-for="${CSS.escape(figure.id)}"]`);
  if (sizes && look.sizes != null) sizes.textContent = look.sizes;
  select.dispatchEvent(new CustomEvent('look', { detail: look, bubbles: true }));
});

for (const figure of document.querySelectorAll('pencil-figure')) {
  // Why a figure couldn't be drawn, under it. One that lost its GPU and said so can come back, and
  // then what it said is gone.
  const note = document.querySelector(`[data-error-for="${CSS.escape(figure.id)}"]`);
  if (note) {
    figure.addEventListener('error', (e) => {
      const error = e.detail?.error;
      note.textContent = `This figure couldn’t be drawn here: ${error?.message ?? error}`;
      note.hidden = false;
    });
    figure.addEventListener('recover', () => {
      note.textContent = '';
      note.hidden = true;
    });
  }
  // A tall figure fits the window's height on a wide screen rather than running off it, as in the
  // playground. On a narrow one it takes the width, since a sheet like the lattice's narrow one is
  // drawn tall on purpose. The stylesheet says when `--fit-width` applies.
  figure.addEventListener('load', () => {
    const [w, h] = figure.designSize ?? [0, 0];
    if (w && h) figure.style.setProperty('--fit-width', `calc(85vh * ${w / h})`);
    else figure.style.removeProperty('--fit-width');
  });
}

// A figure whose page starts it at a moment, as Möbius's starts at the graph's own map, gets
// `data-start` in seconds, set once when it first loads.
for (const figure of document.querySelectorAll('pencil-figure[data-start]')) {
  figure.addEventListener('load', () => (figure.time = Number(figure.dataset.start)), { once: true });
}
