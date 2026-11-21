# pencil-lang

Pencil: a declarative artifact for mathematical visualization that can carry style,
decorations, highlights and composition, and renders to both interactive figures and
video.

Read in this order, with earlier files winning where they disagree:

1. `docs/goals.md`: current goals, everything still open (one list, under "Open"), the changes from
   the July design, and how each figure went.
2. `docs/spec/`: the July plan and specs, unmodified. Their TypeScript, Three.js and TSL
   references are superseded as `goals.md` describes.
3. `docs/design-notes.md`: background, including the overlap with the site.

`docs/language.md` is the reference for the syntax and built-ins. Add to it when the language changes: a
test fails if a built-in, property or statement goes undocumented. `docs/decisions.md` has a row for each
language decision and why, and the site doesn't render it: add the row there.

## Commands

```bash
cargo test --workspace
cargo fmt --all && cargo clippy --workspace --all-targets --locked -- -D warnings   # before committing Rust: CI checks both first, and skips the tests when either fails
UPDATE_GOLDENS=1 cargo test -p pencil-core --test corpus   # after an intended IR change
cargo run -p pencil-cli -- check corpus/001-circle/circle.pen --style corpus/001-circle/circle.style
cargo run -p pencil-cli -- render corpus/001-circle/circle.pen --style corpus/001-circle/bands.style --t 1.25 --out out/frame.png
cargo run -p pencil-cli -- svg corpus/005-tangents/tangents.pen --style corpus/theme.style --style corpus/005-tangents/plain.style --out out/tangents.svg   # one frame as vector graphics; what a style works out per pixel is left out, and stderr names each mark
cargo run -p pencil-cli -- compile corpus/001-circle/circle.pen --style corpus/001-circle/circle.style --out out/circle.pfig   # one file a page loads without parsing; only this build can read it
cargo run --release -p pencil-cli -- build corpus/013-tube/tube.pen --style corpus/theme.style --style corpus/013-tube/plain.style --card --out out/tube   # what a page needs: the compiled figure, its poster as PNG and WebP, a card and a .json. The Astro component in packages/figure runs this for a site; a page isn't written around it by hand
cargo run -p pencil-cli -- render out/circle.pfig --t 1.25 --out out/frame.png   # render, check, video, ir and wgsl take a compiled file too
cargo run -p pencil-cli -- latex formulas.tex   # LaTeX from Desmos or MathLive, a formula per line, as Pencil text
scripts/build-web.sh           # the WASM, into packages/figure/pkg
pnpm install && pnpm test:web  # the element's and the playground's Playwright tests, in packages/figure/tests (after scripts/build-web.sh); PENCIL_MIN=1 runs them on the minified bundle, PENCIL_GL=1 on WebGL2
cargo run --release -p pencil-cli -- video corpus/003-unroll/unroll.pen --style corpus/003-unroll/blog.style --size 1280 --out out/unroll.mp4
scripts/serve.py              # port 8741, no caching; then open /web/playground.html, /web/unroll.html, /web/bounds.html, /web/tube.html, /web/strokes.html, /web/text.html, /web/terms.html, /web/stepped.html or /web/ast.html (add ?gl for WebGL2)
node scripts/logo.mjs --touch   # the site's icon and its touch icon, from the hearts' formula; change the weight or gap there, not in the SVG
cd site && pnpm install && pnpm build   # the pencil-lang.dev site into site/dist, after scripts/build-web.sh and cargo build --release -p pencil-cli, from the same source; pnpm dev serves it on 4321
```

## Layout

- `crates/pencil-core`: parser, IR, symbolic derivatives, styles, tracing, strokes, CPU oracle,
  WGSL emitter, `svg` (a frame as vector graphics, from what a frame prepares for the renderers), `formula` (a label's mathematics, `$…$`, laid out from the math font's metrics, which `build.rs` reads from the package's fonts), and `compiled` (a Composition as bytes, stamped with a hash of the core's source by `build.rs`).
- `crates/pencil-render`: wgpu pipeline shared by the native and browser paths.
- `crates/pencil-cli`: the `pencil` command, including `check` (GPU against the oracle) and
  `video`.
- `crates/pencil-web`: the WASM host.
- `packages/figure`: `@pencil-lang/figure`, what a site installs. `.` is the `<pencil-figure>` element, `./host` the
  WASM as a page calls it, and `./astro` with `./Figure.astro` the Astro integration and component, which run
  `pencil build` and write the element around a compiled figure and its poster. Its tests are Playwright's. Its `fonts/`
  are the fonts labels are set in, and the only copies: the element loads them, the command has them built in, and the
  core's build reads the math font's metrics. The repo is a pnpm workspace of it and `site/`.
- `web/`: the playground, where a reader opens, changes and shares a figure's sources (a link's `#pen=` carries a whole
  figure), and a live-reloading page for most figures. They load the package's files as they are, through the import map
  `web/imports.js` writes, or its minified bundle with `?min`. A figure on a site is built by the package's Astro
  component, not by a page here.
- `corpus/`: figures and their golden IR, which are the contract.
- `site/`: the pencil-lang.dev site, in Astro with static output. It renders `docs/language.md`, draws
  the corpus through the package's `<Figure>`, and frames `web/playground.html`, reading and serving those
  files from the repo rather than copying them in (`site/README.md`).
- `lab/text`: its own workspace, a test bench for text renderers. `cargo run --release` there writes
  images and results for `/web/text.html`.

Two pairs must stay in step, and `pencil check` compares them: `eval::Frame::shade` with
`wgsl::emit`, and `stroke::segment_coverage` with `wgsl::COVERAGE_SHADER`. A third is held by tests: the command's
`text.rs` and the element's `labels.js` draw a label from the same layout at the same widths (`formulas.spec.js`, and the
widths `text.rs` pins beside it).

## Working rules

- **The math is the authority, not the blog.** The site sets the bar for look and feel, but its posts
  can be drafts and its construction figures have had issues. When porting one, test the geometry
  itself, and treat any match with the site as a comparison.
- **Marks keep their form.** Don't normalize to one representation, because each form has its
  own fast path. Rendering hints go in styles, and can't change what a figure means.
- **Description, style and rendering stay separate.** A scene says what is in the figure
  and what each thing means. It holds no drawing calls, no colors or sizes as values, and
  nothing specific to one renderer. Styles target names.
- **Animation and interaction are bindings.** Time and reader input both bind params of
  the same figure. Don't build either as a separate feature, and don't let a web-only
  choice rule out video.
- **Rust core.** The parser, IR and compute are Rust. Compute means evaluating the symbolic
  expressions in the IR. The core runs through WASM on the web and natively for video.
- **The CPU interpreter is the oracle, not the renderer.** Keep it simple and correct, and
  put performance work in the GPU path.
- **Iterate before locking.** Syntax and IR can change freely for now. Don't add stability
  promises or compatibility layers.
- **Artifact first.** Nothing gets built unless a real figure needs it. The site's
  existing figures are the corpus.
- **Mathematics in the output.** Hue indexes a curve family, and a taper follows the
  real scale factor. Decoration should encode the math, not approximate it.

## Related

- `~/src/krisselden/krisselden_com`: the Astro site with the working figure system, which
  overlaps heavily with this project. Read its `docs/figures.md` and
  `src/lib/construction.ts` before designing anything here. It has no Pencil references
  yet (checked 2026-11-05).
- `~/Downloads/krisselden-home.html`: the July design mockup for the site.
