# pencil-lang.dev

The project site: a landing page, the docs, the demos and the playground. An Astro site with static
output and no server code.

```bash
scripts/build-web.sh                    # from the repo root: the WASM, into packages/figure/pkg
cargo build --release -p pencil-cli     # and `pencil`, which builds each figure. Both from the same source.
cd site
pnpm install
pnpm dev                      # http://localhost:4321
pnpm build                    # into site/dist
pnpm preview                  # serves site/dist on http://localhost:4322 as the Worker does, headers and all
```

## Deploying

The site is a Cloudflare Worker of static files (`wrangler.jsonc`), named `pencil-lang`. CI builds the site on
every push and pull request (`.github/workflows/ci.yml`). It builds on GitHub and not on Cloudflare because a build
compiles the Rust and draws each figure's poster on a GPU, which CI has in software and Cloudflare's build machines
don't.

Publishing takes no token. From `main`, once the checks have passed, CI pushes the built files and `wrangler.jsonc`
to the `deploy` branch, as one commit that replaces the last. Cloudflare is connected to the repo (Workers Builds),
and publishes that branch as it is. Its settings for the Worker, under Settings, Build:

- Production branch: `deploy`, with builds for other branches off, so a push to `main` doesn't start one.
- Root directory: `/`.
- Build command: none.
- Deploy command: `npx wrangler deploy`.

`pencil` has its label fonts built in (`packages/figure/fonts`), so a poster with text is drawn on CI's Linux as it is
on a Mac. `CAN_DRAW_TEXT` in the workflow, which held the build and the publish back while the fonts were macOS's, is
`'true'` since 2026-11-16.

The site is at `pencil-lang.dev`. The domain's DNS is on Cloudflare, it's the Worker's custom domain, and a redirect
rule there sends `www` to it. Those were set in Cloudflare's dashboard, and none of it is in `wrangler.jsonc`.

`public/_headers` says how long a browser keeps each kind of file: what's under `_astro/` and `_pencil/` for good,
since each is named for its bytes; the icons for a day; and a page not at all, so a deploy shows at once.

`pnpm deploy` publishes from a machine with `wrangler login`, after the two builds above.

The figures come through `@pencil-lang/figure`, the package in `packages/figure`, which the site is the first user of:

- `astro.config.mjs` adds its integration with the repo as the figures' root, and `src/components/Figure.astro` wraps
  its `<Figure>`. A build runs `pencil build` for each figure under each set of sheets a reader can get, and ships the
  compiled figures and their posters under `_pencil/`, and no sources. It stops if `pencil` and the WASM are different
  builds, since a page couldn't load what it compiled.
- `pnpm dev` hands each figure its sources instead, and a figure reloads where it is when one of them is saved.

Nothing from the rest of the repo is copied into `src/`:

- `docs/language.md` is rendered at build, and the landing page's example is read from `corpus/`.
- The playground is `web/playground.html` itself, in a frame (`src/pages/playground.astro`).
  `integrations/repo-files.mjs` serves it and what it loads (`corpus/`, `lab/explainer/`, the scripts in `web/`,
  `web/ast.html`, and the package's `src/` and `pkg/`) at their repo paths in dev, and copies them into `dist/` on build,
  so it finds them as it does under `scripts/serve.py`. A build points its loader at the WASM the site's pages already
  ship, so a reader fetches that once.

Every page's link preview (`og:image` and `twitter:image`, written by `src/layouts/Base.astro`) is the 1200×630 image
that is the poster of the hearts figure under the glow sheet, the home page's hero, and then
`corpus/008-hearts/card.style`, which reshapes the figure to that size so the poster needs no padding. A pasted address
shows the same image whatever the page.

`src/lib/figures.ts` lists the demos, with the sheets each figure's page in `web/` loads.
Each demo's controls are the package's: `src/components/Figure.astro` asks its `<Figure>` for them, which writes them into
the page at build, disabled and in their first state, so nothing moves when the figure loads, and the element turns them
on. A demo with more than one look adds a choice between them, which the controls show after their own.
`src/scripts/figures.js` does what the site does around a figure: the choice of looks, saying why one can't draw, fitting a
tall one to the window, and starting one at a moment. Add `?gl` to a page's address to draw with WebGL2.
