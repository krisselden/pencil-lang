// An import map, so the pages here import the package as a site that installed it would:
//   import '@pencil-lang/figure';  import { gather } from '@pencil-lang/figure/host';
// Include it first in <head>, as a classic script. It maps both names to the source files, or to
// the one minified bundle (scripts/build-web.sh writes it) when the page's URL has `?min`, to
// try what a site ships.
(() => {
  const me = document.currentScript;
  const at = (path) => new URL(path, me.src).href;
  const min = new URLSearchParams(location.search).has('min');
  const bundle = at('../packages/figure/pkg/pencil-figure.min.js');
  const map = document.createElement('script');
  map.type = 'importmap';
  map.textContent = JSON.stringify({
    imports: {
      '@pencil-lang/figure': min ? bundle : at('../packages/figure/src/element.js'),
      '@pencil-lang/figure/host': min ? bundle : at('../packages/figure/src/host.js'),
      '@pencil-lang/figure/controls': min ? bundle : at('../packages/figure/src/controls.js'),
      // Small and on its own, so it's the same file either way.
      '@pencil-lang/figure/pack': at('../packages/figure/src/pack.js'),
    },
  });
  me.after(map);
})();
