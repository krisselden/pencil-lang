// What a page with a `<Figure>` loads: the element, and in `astro dev` word from the integration
// that a source changed, so each figure fetches its files again where it is.
import '../src/element.js';

if (import.meta.hot) {
  import.meta.hot.on('pencil:change', () => {
    for (const figure of document.querySelectorAll('pencil-figure')) figure.reload();
  });
}
