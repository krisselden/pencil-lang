# pencil-lang.dev

The project site: a landing page, the docs, the demos and the playground. An Astro site with static
output and no server code.

```bash
scripts/build-web.sh          # from the repo root, once: builds web/pkg, which the figures need
cd site
pnpm install
pnpm dev                      # http://localhost:4321
pnpm build                    # into site/dist
pnpm preview                  # serves site/dist on http://localhost:4322
```

Nothing from the rest of the repo is copied into `src/`:

- `docs/language.md` is rendered at build, and the landing page's example is read from `corpus/`.
- `integrations/repo-files.mjs` serves `corpus/`, `lab/explainer/`, `web/pkg/`, the scripts in `web/`,
  `web/playground.html` and `web/ast.html` at their repo paths in dev, and copies them into `dist/` on
  build, so `<pencil-figure>` and the playground find them as they do under `scripts/serve.py`.
- The playground is `web/playground.html` itself, in a frame (`src/pages/playground.astro`).

`src/lib/figures.ts` lists the demos, with the sheets each figure's page in `web/` loads.
`src/scripts/figure-controls.js` builds each demo's controls from what the figure declares. Add
`?gl` to a page's address to draw with WebGL2.
