// What the figure pages in `web/` share around a `<pencil-figure>`: a play button, a time slider,
// a status line, and reloading as the files change. Each is only calls on the element and its
// events, so a page shows what a reader of the site could do with the same element.
import { gather } from './pencil-figure.js';

/** `?gl` on the page's URL tests the WebGL2 path. Set before the GPU starts. */
export function fromQuery(figure) {
  if (new URLSearchParams(location.search).has('gl')) figure.setAttribute('gl', '');
}

/** Shows the backend, and how often the figure compiled, in `status`, and what went wrong in
 * `error`. A compile error leaves the last figure on screen. */
/**
 * How big the figure is drawn, against its design, and the page around it: what a reader sees,
 * in numbers two people can compare, as `drawn 620×429 of 340×235 (1.82×) · em 15px · window
 * 1280×900 at 2×`. A label is an em at any size, and a stroke scales with the drawing.
 */
export function viewing(figure) {
  const width = figure.getBoundingClientRect().width;
  const [shapeW, shapeH] = figure.designSize ?? [0, 0];
  const height = shapeW ? Math.round((width * shapeH) / shapeW) : 0;
  // A sheet's width is in design pixels, and its height, when it gives none, follows the shape.
  // A figure with no design width has no design pixels to scale: its `px` is a CSS pixel.
  const info = figure.info;
  const panels = info?.panels.length > 1;
  const designed = info && (panels || info.viewport.width != null);
  const w = panels ? shapeW : (info?.viewport.width ?? shapeW);
  const h = panels ? shapeH : (info?.viewport.height ?? Math.round((w * shapeH) / shapeW));
  const drawn = !w ? ''
    : designed ? `drawn ${Math.round(width)}×${height} of ${w}×${h} (${(width / w).toFixed(2)}×)`
    : `drawn ${Math.round(width)}×${height}, no design width, so 1px is a CSS pixel`;
  const em = `em ${parseFloat(getComputedStyle(figure).fontSize)}px`;
  const window_ = `window ${innerWidth}×${innerHeight} at ${devicePixelRatio}×`;
  return [drawn, em, window_].filter(Boolean).join(' · ');
}

export function report(figure, status, error) {
  let compiles = 0;
  const paint = () => {
    if (!compiles) return;
    status.textContent = [figure.backend, viewing(figure), `compiled ${compiles} time${compiles === 1 ? '' : 's'}`].join(' · ');
  };
  figure.addEventListener('load', () => {
    compiles += 1;
    error.textContent = '';
    paint();
  });
  // The figure's size and em, and the window's, which can change while the figure keeps its size.
  figure.addEventListener('resize', paint);
  addEventListener('resize', paint);
  figure.addEventListener('error', (e) => {
    const { error: cause } = e.detail;
    error.textContent = String(cause?.message ?? cause);
  });
}

/** A play button that plays and pauses the figure and says which it'll do. A figure that never
 * reads time has nothing to play, so the button only shows for one that does. */
export function playButton(figure, button) {
  const paint = () => {
    button.hidden = !figure.info?.usesTime;
    button.textContent = figure.paused ? 'Play' : 'Pause';
  };
  button.hidden = true;
  button.addEventListener('click', () => (figure.paused ? figure.play() : figure.pause()));
  figure.addEventListener('load', paint);
  figure.addEventListener('play', paint);
  figure.addEventListener('pause', paint);
}

/** A slider over the figure's duration. It follows time as the figure draws, and moving it
 * pauses the figure and seeks. */
export function timeSlider(figure, input, output, format = (t) => t.toFixed(2)) {
  const paint = () => {
    input.max = figure.duration;
    input.value = figure.time;
    if (output) output.textContent = format(figure.time);
  };
  input.addEventListener('input', () => {
    figure.pause();
    figure.time = Number(input.value);
  });
  figure.addEventListener('load', paint);
  figure.addEventListener('draw', paint);
}

/** Compiles the figure again when its `.pen`, a sheet, or a file one of them uses changes on
 * disk, while editing. */
export function watch(figure, every = 500) {
  const read = async (url) => {
    const response = await fetch(new URL(url, document.baseURI), { cache: 'no-store' });
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    return response.text();
  };
  let loaded = null;
  let urls = null;
  setInterval(async () => {
    const src = figure.getAttribute('src');
    const sheets = (figure.getAttribute('sheets') ?? '').split(/\s+/).filter(Boolean);
    const now = JSON.stringify([src, sheets]);
    // The element fetches a new `src` or `sheets` itself: only the files' contents are watched.
    if (now !== urls) {
      urls = now;
      loaded = null;
    }
    try {
      const names = [src, ...sheets];
      const [doc, ...texts] = await Promise.all(names.map(read));
      const used = await gather(names, [doc, ...texts], read);
      const key = JSON.stringify([doc, texts, used.files]);
      if (urls !== now) return;
      if (loaded === null) loaded = key;
      else if (key !== loaded) {
        loaded = key;
        figure.load(doc, texts, used);
      }
    } catch (e) {
      figure.dispatchEvent(new CustomEvent('error', { detail: { error: e } }));
    }
  }, every);
}
