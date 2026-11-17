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
pnpm preview                  # serves site/dist on http://localhost:4322
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
`'true'` since 2026-11-16. That first build on CI hasn't run yet: until it has, `pnpm deploy` from a Mac is the way
that's known to work.

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

`src/lib/figures.ts` lists the demos, with the sheets each figure's page in `web/` loads.
`src/scripts/controls-markup.js` writes each demo's controls from what the figure declares: into the page at build,
disabled and in their first state, so nothing moves when the figure loads. `src/scripts/figure-controls.js` turns them
on. Add
`?gl` to a page's address to draw with WebGL2.
