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

`docs/language.md` is the reference for the syntax, built-ins and language decisions. Add to it when
the language changes: a test fails if a built-in, property or statement goes undocumented.

## Commands

```bash
cargo test --workspace
UPDATE_GOLDENS=1 cargo test -p pencil-core --test corpus   # after an intended IR change
cargo run -p pencil-cli -- check corpus/001-circle/circle.pen --style corpus/001-circle/circle.style
cargo run -p pencil-cli -- render corpus/001-circle/circle.pen --style corpus/001-circle/bands.style --t 1.25 --out out/frame.png
scripts/build-web.sh
cargo run --release -p pencil-cli -- video corpus/003-unroll/unroll.pen --style corpus/003-unroll/blog.style --size 1280 --out out/unroll.mp4
scripts/serve.py              # port 8741, no caching; then open /web/playground.html, /web/unroll.html, /web/bounds.html, /web/strokes.html or /web/text.html (add ?gl for WebGL2)
```

## Layout

- `crates/pencil-core`: parser, IR, symbolic derivatives, styles, tracing, strokes, CPU oracle,
  WGSL emitter.
- `crates/pencil-render`: wgpu pipeline shared by the native and browser paths.
- `crates/pencil-cli`: the `pencil` command, including `check` (GPU against the oracle) and
  `video`.
- `crates/pencil-web`, `web/`: the WASM host and live-reloading playground.
- `corpus/`: figures and their golden IR, which are the contract.
- `lab/text`: its own workspace, a test bench for text renderers. `cargo run --release` there writes
  images and results for `/web/text.html`.

Two pairs must stay in step, and `pencil check` compares them: `eval::Frame::shade` with
`wgsl::emit`, and `stroke::segment_coverage` with `wgsl::COVERAGE_SHADER`.

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
