// A page that pins a theme, and a figure's sheets and poster that are picked by one.
//
// A figure's dark sheets and its dark poster are picked by `(prefers-color-scheme: dark)`, in a
// `<source>`'s `media`, which is the reader's system and nothing the page can answer. A page with
// a theme toggle pins a scheme over the system's, and says so in CSS, as it has to for its form
// controls and scrollbars to follow:
//
//   :root[data-theme='dark'] { color-scheme: dark; }
//
// So that's what a figure reads: `color-scheme` where it sits. Listing one of `light` and `dark`
// pins it, and listing both or neither leaves it to the reader's system.

/**
 * Answers `prefers-color-scheme` in the `media` of every `<source>` in a figure, its own and its
 * poster's, with the scheme the page pins there, and gives that scheme, or `null` with none
 * pinned. What was written is kept in `data-media` while it's answered, and put back when the
 * pin goes. A page changing a `media` that's being answered changes `data-media`. The answer is
 * a condition that always holds, or one that never does, in the query's place.
 *
 * It stands alone, without anything from this module, because `Figure.astro` writes it into the
 * page as text: there it runs as the figure is read, before a poster in the other theme is
 * fetched and painted, which the element's own script would be too late for.
 */
export function settle(figure) {
  const listed = getComputedStyle(figure).colorScheme.split(/\s+/);
  const dark = listed.includes('dark');
  const pinned = dark === listed.includes('light') ? null : dark ? 'dark' : 'light';
  for (const source of figure.querySelectorAll('source[media], source[data-media]')) {
    const written = source.dataset.media ?? source.getAttribute('media');
    const media = pinned
      ? written.replace(/\(\s*prefers-color-scheme\s*:\s*(light|dark)\s*\)/g, (_, scheme) =>
          scheme === pinned ? '(min-width: 0px)' : '(not (min-width: 0px))',
        )
      : written;
    if (media === written) delete source.dataset.media;
    else source.dataset.media = written;
    if (source.getAttribute('media') !== media) source.setAttribute('media', media);
  }
  return pinned;
}

// A toggle pins a scheme by changing an attribute on `<html>` or `<body>` that the page's CSS
// reads, so those are watched, once for every figure on the page.
const figures = new Set();
let watcher = null;

/** Settles a figure now and whenever the page may have pinned another scheme. */
export function follow(figure) {
  settle(figure);
  figures.add(figure);
  watcher ??= new MutationObserver(() => {
    for (const each of figures) settle(each);
  });
  watcher.observe(document.documentElement, { attributes: true });
  if (document.body) watcher.observe(document.body, { attributes: true });
}

/** Stops following a figure that has left the page. */
export function unfollow(figure) {
  figures.delete(figure);
  if (!figures.size) watcher?.disconnect();
}
