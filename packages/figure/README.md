# @pencil-lang/figure

Pencil figures on a page. One package with three ways in, since they have to be the same build: a figure a build
compiles is only read by the WASM built from the same source.

| Import | What it is |
|--------|------------|
| `@pencil-lang/figure` | Defines `<pencil-figure>`, and exports `PencilFigure`. |
| `@pencil-lang/figure/host` | The WASM as a page calls it: `ready()`, `Player`, `gather`, `uses`, `inspect`, `from_latex` and the rest. |
| `@pencil-lang/figure/pack` | `pack` and `unpack`: a figure's sources as a string for a link's `#pen=`, which the playground opens. |
| `@pencil-lang/figure/astro` | The Astro integration (the default export), `build()` and `alternates()`. |
| `@pencil-lang/figure/Figure.astro` | The Astro component. |

It isn't published yet. From another project, install it from a checkout, after building the WASM into `pkg/` here:

```bash
scripts/build-web.sh                        # in the Pencil repo
pnpm add ../pencil-lang/packages/figure     # in the other project, with its own path to the checkout
```

## The element

```html
<script type="module">import '@pencil-lang/figure';</script>
<pencil-figure src="tube.pen" sheets="theme.style tube.style"></pencil-figure>
```

It owns the canvas, the labels over it, the GPU and the clock. It starts the GPU when it's near the viewport, pauses
under `prefers-reduced-motion`, and every handle and beat can be worked from the keyboard. Its attributes, methods and
events are described at the top of [`src/element.js`](src/element.js).

Labels are set in the package's own fonts, in [`fonts/`](fonts/README.md): Libertinus, under the SIL Open Font License.
They're the files the `pencil` command draws a poster with, so the figure that takes over from a poster sets the same
glyphs in the same places. The element fetches the two text faces when it loads, and the math font the first time a
label needs it.

A bundler has to keep the WASM and the fonts where the scripts that load them look, `../pkg` and `../fonts` from `src/`. Vite does, once the package is left out of its
dependency pre-bundling (`optimizeDeps: { exclude: ['@pencil-lang/figure'] }`), which the Astro integration sets.

`scripts/build-web.sh` ends by writing `pkg/pencil-figure.min.js`, the element and the host bundled and minified into
one ES module with esbuild (`pnpm install` first). It exports what `@pencil-lang/figure` and `@pencil-lang/figure/host`
do, and finds its WASM beside itself, so it has to stay in `pkg/`. The pages in `web/` load it with `?min` on the URL
(`web/imports.js` is an import map for that), and `PENCIL_MIN=1 pnpm test` runs the tests on it. It's for measuring
the JavaScript a site would ship, with Lighthouse for one.

## Astro

```js
// astro.config.mjs
import pencil from '@pencil-lang/figure/astro';

export default defineConfig({
  integrations: [pencil({ root: './figures' })],
});
```

```astro
---
import Figure from '@pencil-lang/figure/Figure.astro';
---
<Figure src="tube.pen" sheets="theme.style tube.style" dark="dark.style" alt="The strip rolling up into a tube." />
```

- **A build compiles each figure** with `pencil build`, once for each set of sheets a reader can get: `sheets`, with
  `dark` added under a dark theme, and `narrow` added when `narrowMedia` matches (`(max-width: 600px)` unless said).
  The page ships the compiled figures and their posters under `_pencil/`, and no sources. A figure that doesn't
  compile stops the build with the compiler's message.
- **The poster is an image with the figure's size,** so the page is laid out before any script runs. It's offered as
  WebP, with the PNG behind it. It shows until the
  figure's first frame, and stays where a figure can't run. Nothing moves when the figure takes over. `poster={{ time,
  params }}` picks another moment than the document's own `poster`.
- **`astro dev` hands the element the sources,** served from where they are, and a figure reloads in place when one is
  saved, keeping its time and what a reader holds.
- **`priority` is for a figure in view when the page opens.** Its poster is fetched at once with `fetchpriority="high"`,
  since it's likely the page's largest first paint. Every other poster is lazy. Mark one or two a page. A build also
  writes a line of inline script that starts fetching what that figure will need, the WASM and the compiled figure its
  media picks, at low priority, so they start as the page is read and not after the element's script has run. The
  element takes them over, and each is downloaded once. (`<link rel="preload" as="fetch">` isn't used: WebKit fetches
  the file again.)
- **`alt` is required.** It's the poster's text and the figure's name.
- Anything else, such as `id`, `class`, `params`, `paused` or `data-*`, goes on `<pencil-figure>`.

`build({ src, sheets, time, params, size, card })` is what the component calls. A page can call it too, for a
figure's `info` (its size, params with their ranges, beats and captions, and the colors its sheets name) and its
`moment` (the time, and every param's value and range, where the poster shows it), to write its controls in their
first state before any script runs, or with `card: true` for a 1200×630 image for sharing.

`alternates({ sheets, dark, narrow, narrowMedia })` gives the sets of sheets the component builds a figure under: its
own as `sheets`, and as `others` each `{ media, sheets }` it writes a `<source>` for. A page that swaps one figure
for another, as a choice of styles does, swaps those `<source>`s too: one whose media matches is used before the
element's own attributes.

With `playground` set on the integration to a playground's address, as `https://pencil-lang.dev/playground/`,
`build()` also gives `playground`: a link that opens the figure's sources there, for a reader to read and change. A
built page ships no sources, so the link carries them all, compressed, after its `#`. It's a few thousand characters
a figure. Where the link goes on the page is the page's to say.

The integration's options are `root`, where `src` and `sheets` paths start (the Astro project by default),
`playground`, and `bin`, the `pencil` command. Without `bin` it tries `PENCIL_BIN`, then `target/release/pencil` in the checkout around `root`
or around this package, then `pencil` on the `PATH`. Drawing a poster takes a GPU, so a site is built where there is
one. A build stops if `pencil` and the package's WASM are different builds of the core.

## Tests

```bash
pnpm install
pnpm exec playwright install chromium webkit   # once
pnpm test
```

Playwright's, in [`tests/`](tests), against the corpus in this repo, headless, on its own Chromium and WebKit: each at
a desktop size and as a phone (a Pixel 7 and an iPhone 15, emulated). They cover drawing, every key on a handle and on
beats, a mouse drag and a touch drag, a range that follows another param, reduced motion, sheets picked by theme, the
poster's swap, and a compiled figure. `PENCIL_GL=1 pnpm test` runs them all on WebGL2 in place of WebGPU, and `PENCIL_MIN=1 pnpm test` on the minified bundle.
[`playwright.config.js`](playwright.config.js) says which GPU path each browser has headless. An emulated phone is a
desktop engine at a phone's size with touch, not Mobile Safari: a real iPhone isn't covered.
