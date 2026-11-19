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

A param with a binding goes back to it when a reader sets a param that binding reads, as θ in Bounds does when `q` or
the case is set after a drag: the core says which params each binding reads (`info.params[].reads`), and the element
hands them back, so a page needs no `release` of its own for it. Params held together, by the `params` attribute or
setter, keep each other.

When a browser loses the GPU under a figure, as Chrome does after sleep or when its GPU process restarts, the figure
starts its GPU again where it was: the same sources, time, held params and pause, with its poster showing meanwhile. It
tries at once, after a second and after five, and then sends `error` (with `detail.gpu`), trying once more each time
the page is shown. One that draws again after that sends `recover`, so a page can take its message down. A GPU error
on a device that isn't lost is the figure's, and is an `error` at once. Other figures on the page draw on. `Player` has
`lose_device()` and `trap()` for testing this, and `on_lost(callback)`, which the element uses, and the host has
`let_go(id)`, which lets go of a trapped player's device. On WebGL2 the canvas keeps its context, lost and brought back
for the next player, so restarts don't use up a browser's cap on live contexts.

It owns its touch and selection behaviour too, so a page sets no `touch-action` or `user-select` for it. A touch within
a handle's reach drags the handle and doesn't scroll the page. One anywhere else on the figure scrolls and zooms the
page, as it would over an image, so a figure never traps a reader's scroll. A drag never selects text, in the figure or
on the page around it, and a long press shows no callout.

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
- **A page's own theme toggle is followed.** A page that pins a theme over the reader's system says so in its CSS,
  as `:root[data-theme='dark'] { color-scheme: dark; }`, and a figure there takes its dark sheets and its dark
  poster, from the first paint, and its poster in the other theme isn't fetched. With `color-scheme` listing both or
  neither, the reader's system picks. The element
  does this for any `<source media="(prefers-color-scheme: …)">` among its children, and looks again when `<html>`
  or `<body>` changes an attribute (`src/scheme.js`).
- **The poster is an image with the figure's size,** so the page is laid out before any script runs. It's offered as
  WebP, with the PNG behind it. It shows until the
  figure's first frame, and stays where a figure can't run. Nothing moves when the figure takes over. `poster={{ time,
  params }}` picks another moment than the document's own `poster`.
- **`astro dev` hands the element the sources,** served from where they are, and a figure reloads in place when one is
  saved, keeping its time and what a reader holds.
- **`priority` is for a figure in view when the page opens.** Its poster is fetched at once with `fetchpriority="high"`,
  since it's likely the page's largest first paint. Every other poster is lazy. Mark one or two a page. One with
  `dark` sheets is written lazy and made eager by a line of script after it, once its theme is known: a browser
  reading ahead while the page's stylesheet loads would fetch the poster the reader's system picks, and skips a lazy
  one. So that poster starts once the page's stylesheet is in. A build also
  writes a line of inline script that starts fetching what that figure will need, the WASM and the compiled figure its
  media picks, at low priority, so they start as the page is read and not after the element's script has run. The
  element takes them over, and each is downloaded once. (`<link rel="preload" as="fetch">` isn't used: WebKit fetches
  the file again.)
- **A site can keep its built figures in its repo,** for a build that runs where there's no `pencil` or no GPU, as
  most hosts' builders are. `pencil({ kept: 'figures' })` builds into that folder, and the folder is committed with
  the site. A build uses what's kept for a figure for as long as its sources hold what they did when it was built,
  and builds it again otherwise. Where `pencil` isn't there, it stops and says which figure needs building and why.
  A compiled figure is read only by the build of Pencil that wrote it, so after the package is upgraded every figure
  is built again, with a `pencil` from the same source as the package. A build names the folders nothing asked for,
  and leaves them: one that skips drafts asks for fewer. A figure is built elsewhere and put in the folder only once
  it has built, so a build that fails, or has no `pencil`, never removes what's kept.
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
one. A build stops if `pencil` and the package's WASM are different builds of the core. `kept` is the folder of built
figures above.

The integration lets `astro dev` serve the package's own files (the element, its WASM and fonts) by adding the
package's folder to Vite's `server.fs.allow`, after Vite has worked out its own list, so a site's files are served as
they were. The package has types for the integration and `build()` (`astro/index.d.ts`), and `Figure.astro` declares
its props, so a TypeScript site needs no declarations of its own.

## Tests

```bash
pnpm install
pnpm exec playwright install chromium webkit   # once
pnpm test
```

Playwright's, in [`tests/`](tests), against the corpus in this repo, headless, on its own Chromium and WebKit: each at
a desktop size and as a phone (a Pixel 7 and an iPhone 15, emulated). They cover drawing, every key on a handle and on
beats, a mouse drag and a touch drag, a swipe that scrolls past a figure, a drag that selects no text, a range that follows another param, reduced motion, sheets picked by theme, the
poster's swap, and a compiled figure. `PENCIL_GL=1 pnpm test` runs them all on WebGL2 in place of WebGPU, and `PENCIL_MIN=1 pnpm test` on the minified bundle.
[`playwright.config.js`](playwright.config.js) says which GPU path each browser has headless. An emulated phone is a
desktop engine at a phone's size with touch, not Mobile Safari: a real iPhone isn't covered, and touches are sent
through Chromium's CDP, so the touch tests run on the Pixel 7 only.
