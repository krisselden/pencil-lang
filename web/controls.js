// What the figure pages in `web/` share around a `<pencil-figure>`: a play button, a time slider,
// a status line, and reloading as the files change. Each is only calls on the element and its
// events, so a page shows what a reader of the site could do with the same element.
import './pencil-figure.js';

/** `?gl` on the page's URL tests the WebGL2 path. Set before the GPU starts. */
export function fromQuery(figure) {
  if (new URLSearchParams(location.search).has('gl')) figure.setAttribute('gl', '');
}

/** Shows the backend, and how often the figure compiled, in `status`, and what went wrong in
 * `error`. A compile error leaves the last figure on screen. */
export function report(figure, status, error) {
  let compiles = 0;
  figure.addEventListener('load', () => {
    compiles += 1;
    error.textContent = '';
    status.textContent = `${figure.backend}, compiled ${compiles} time${compiles === 1 ? '' : 's'}`;
  });
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

/** Compiles the figure again when its `.pen` or any sheet changes on disk, while editing. */
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
      const [doc, ...texts] = await Promise.all([src, ...sheets].map(read));
      const key = [doc, ...texts].join('\0');
      if (urls !== now) return;
      if (loaded === null) loaded = key;
      else if (key !== loaded) {
        loaded = key;
        figure.load(doc, texts);
      }
    } catch (e) {
      figure.dispatchEvent(new CustomEvent('error', { detail: { error: e } }));
    }
  }, every);
}
