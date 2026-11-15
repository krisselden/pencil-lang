# @pencil-lang/figure

Pencil figures on a page. One package with three ways in, since they have to be the same build: a figure a build
compiles is only read by the WASM built from the same source.

| Import | What it is |
|--------|------------|
| `@pencil-lang/figure` | Defines `<pencil-figure>`, and exports `PencilFigure`. |
| `@pencil-lang/figure/host` | The WASM as a page calls it: `ready()`, `Player`, `gather`, `uses`, `inspect`, `from_latex` and the rest. |
| `@pencil-lang/figure/astro` | The Astro integration (the default export) and `build()`. |
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

A bundler has to keep the WASM beside the script that loads it. Vite does, once the package is left out of its
dependency pre-bundling (`optimizeDeps: { exclude: ['@pencil-lang/figure'] }`), which the Astro integration sets.

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
- **The poster is an image with the figure's size,** so the page is laid out before any script runs. It shows until the
  figure's first frame, and stays where a figure can't run. Nothing moves when the figure takes over. `poster={{ time,
  params }}` picks another moment than the document's own `poster`.
- **`astro dev` hands the element the sources,** served from where they are, and a figure reloads in place when one is
  saved, keeping its time and what a reader holds.
- **`alt` is required.** It's the poster's text and the figure's name.
- Anything else, such as `id`, `class`, `params`, `paused` or `data-*`, goes on `<pencil-figure>`.

`build({ src, sheets, time, params, size, card })` is what the component calls. A page can call it too, for a
figure's `info` (its size, params with their ranges, beats and captions, and the colors its sheets name) to lay out
controls before any script runs, or with `card: true` for a 1200×630 image for sharing.

The integration's options are `root`, where `src` and `sheets` paths start (the Astro project by default), and `bin`,
the `pencil` command. Without `bin` it tries `PENCIL_BIN`, then `target/release/pencil` in the checkout around `root`
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
poster's swap, and a compiled figure. `PENCIL_GL=1 pnpm test` runs them all on WebGL2 in place of WebGPU.
[`playwright.config.js`](playwright.config.js) says which GPU path each browser has headless. An emulated phone is a
desktop engine at a phone's size with touch, not Mobile Safari: a real iPhone isn't covered.
