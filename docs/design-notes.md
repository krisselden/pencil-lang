# Pencil design notes

Background only. `goals.md` and then `spec/` win wherever these notes disagree.

**`goals.md` replaces the "implicits lower to fields" direction here and in `spec/`:** see Forms
and hints there.

**The stroke test in `goals.md` replaces the stroke choices below where they differ:** it uses
an exact box filter rather than the linear ramp, and gamma blending by default.

**Decided in the July 8 chat but not in any spec file**, so these only live in that chat
for now: `::negative` / `::positive` as primitive facets with `::inside` / `::outside` as
aliases, `self.dist` in fill expressions, and the winding convention. The plan also lists
`SPEC/00`, `10`, `17`, `20` and `30`, none of which exist as files.

## The core idea

Pencil is a **declarative way to describe a figure, so it can be styled and rendered
separately.** One description should be able to take a different style, or a different
renderer, without being rewritten.

## Direction (Kris, 2026-11-05)

- **The parser, IR and compute are Rust, and the web is supported through WASM.** "Compute"
  means evaluating the symbolic expressions held in the IR. This replaces the chat's plan
  to extract curves in JS and to write the math twice by hand.
- **Video is a goal from the start**, not a later add-on. The core has to run outside the
  browser as well as through WASM.
- **Iterate before locking the design.** Syntax and IR are expected to change until real
  figures stop forcing changes, so nothing gets a stability promise yet.

## Overlap with krisselden_com

Surveyed 2026-11-05. The site has since grown a working figure system, and most of what
the July chat planned to build exists there in some form, on SVG.

| Pencil concern | Site today | Separate from the figure? |
|---|---|---|
| Declarative description | `lib/construction.ts`: beats and named steps, with GeoGebra's vocabulary | Partly. Steps are data, but they hold closures, and `curve` takes a JS function, so they can't be serialized |
| Evaluation | `evaluate(beats, params)` returns values | Yes |
| Rendering | `drawConstruction(g, …)` through `Painter`, a concrete SVG class in `lib/plot.ts` | One backend only |
| Style | Label roles, size tiers, palette names, `show: main/aux/faint`, set in CSS | Largely yes, and this is the most finished part |
| Linked views | `Facets` / `Facet` | Layout only. Each figure script does its own linking |
| Parameters and controls | `input`, `checked`, `Plot` handles in `lib/ui.ts` and `lib/plot.ts` | No. Wired by hand in each script |
| Animation | `lib/animate.ts`: `loop`, the easings, `seek` | Partly. Each script binds time to its own variables |
| Shaders | `Shader`, with UI declared in GLSL uniform annotations | Yes, by a different mechanism |

**The main gap is that most figures aren't declarative.** There are 28 figure components
in posts. Nearly all use `plot.draw` with imperative `Painter` calls, and only a handful
use constructions. Constructions also only cover ruler-and-compass figures.

**Params are bound by hand today.** Five figures use `loop` for time, and three use
`playback` for a discrete step. `involute/Unroll` already binds one value to both time and
a slider, which is the model in `goals.md` written out imperatively.

**Decided for now (easy to change):**
- The site is Pencil's corpus and first consumer. Pencil doesn't compete with it.
- `construction.ts` is the seed: description, then `evaluate`, then a renderer.
- `involute/Unroll` is the first real figure (see the milestone in `goals.md`).
- `BoundsConstruction` comes next: it adds a constrained param (P on a circle, one degree
  of freedom), a discrete step and multiple beats.
- The site's performance rules (`docs/figures.md`) apply to any SVG renderer, since they
  came from traces.

## From the July chat

Distilled from a private claude.ai conversation on 2026-07-09, before the site's figure system.

These are conclusions from that discussion, and none of them has been built or tested
here. The chat builds on the 2026-07-08 design chat, whose plan and specs are in `spec/`.

**Out of date:** the chat makes the embeddable figure the v0 instead of video, and defers
video export. `goals.md` makes video a goal from the start.

## What sets it apart

- The v0 is the **embeddable interactive figure**, not a video language. Something like
  `<pencil-figure src="doyle-spiral.pen">`, with a baked-in attribution footer. Desmos
  links carry no attribution, and here the credit travels with the artifact.
- The idea others lack is **linked views over a shared parameter store**: one store, N viewports.
  Desmos is single-viewport, and Manim has no shared reactive state.
- Pedagogy thesis: drill from a big picture into building blocks, rather than retaining
  building blocks and hoping to build up. Equations aren't wrong to show, only wrong to
  show alone and dead, so a live-substituted typeset panel is itself a view.

## v0 contract

- **2D:** implicit curves, parametric curves, conformal/circline rendering.
- **3D:** parametric surfaces and function graphs (tessellated), IR-expressible
  raymarched implicits (interval-compiled), tangent planes, and z-plane slice curves.
  Intersections so far are only with a z plane, so a slice is the 2D evaluator with one
  variable pinned.
- **Reactivity:** one parameter store (scalars, complex, ints, ranges), draggable points
  and scrub parameters.
- **Views:** each view declares its own object list against the shared params.
- **Finish layer:** about 15 semantic decoration types (`angle A B C`, `arrow from P to Q
  along γ`) and KaTeX labels, as projected SVG and HTML overlays. Expect this layer to
  take longer than the raymarcher.
- **Delivery:** `<pencil-scene>` / `<pencil-view>` web components. The host page owns
  layout through CSS grid.

## Rendering decisions

These predate the Rust and wgpu direction in `goals.md`. Treat them as ideas for the
stroke and vector layer, not the architecture. The Three.js parts no longer apply if wgpu
is adopted, and the windfoil and Vello question is open there.

- **Three.js + TSL**, for WebGL2 reach. This replaced the earlier WebGPU-first idea in
  the build order below. TSL compute only exists on the WebGPU backend, so nothing
  load-bearing can live in compute.
- **Implicits find curves; polylines draw them.** The pipeline is:
  1. CPU extraction: marching squares + Newton, interval-pruned.
  2. Polylines with arc-length attributes.
  3. An instanced-quad stroke renderer.
  4. Styling (dashes, arrows, width) as functions of arc length.
- **Fields stay per-pixel.** SDF and field shaders are for backgrounds, domain coloring
  and region shading only.
- **Stroke coverage:** the linear ramp `clamp(0.5 - d/pixelWidth, 0, 1)`, not smoothstep
  (idea taken from windfoil).
- **Joins:** render each curve's coverage to a single-channel target with max blending,
  then composite. That keeps joins and overlaps idempotent.
- **Screen-space extraction** for v0, at the cost of coupling extraction to the camera.
- **Tangential contact** (kissing circles at solved μ, ν) is invisible to sign-based
  methods. Draw those as separate objects from the known analytic condition.
- **Later:** Manson–Schaefer dual vertices for CSG creases; Keeter's MPR and fidget for
  interval tape work; windfoil for region fills and in-canvas glyphs (video export has
  no DOM).

### Tapered strokes

- Segments are uneven capsules, so `sdUnevenCapsule` with two radii.
- Width is an expression of the curve's natural parameter. For `z^t` the true taper is
  `w(t) ∝ |z|^t`, the conformal scale factor.
- Below 1px, clamp the width and move the deficit into coverage:
  `alpha *= w / max(w, 1px)`.

### Color

- Keep the chrome quiet and let figures carry full spectral color. **Hue is a
  coordinate:** the same hue identifies the same curve across linked views.
- Make it a language feature: `color: hue(k/q)` through a perceptual (OKLCH) ramp rather
  than raw HSV.

## Build order: Newton figure first

**Superseded as the starting point.** This builds a runtime from scratch and hand-writes
each scene, and the site has since covered much of that ground. It's kept as a useful
checklist for a GPU renderer. Its deferral of the language also conflicts with the core
idea unless scenes are written as data from the start.

It exercises every subsystem at a small content size. The chat's estimate was about
three weeks. Phase 0 predates the Three.js decision above, so it needs revisiting.

0. **Scaffold:** Vite + TS, custom-element skeleton, correct DPR and resize handling.
1. **Reactive spine:** tiny signals (`param`, `derived`, `subscribe`), render on dirty.
2. **First pixels:** hand-write the math in both shader and JS, then a 2D implicit pass
   with a coordinate grid.
3. **Alive:** draggable z₀, Newton orbit on the CPU, step scrubber.
4. **Finish layer, minimal:** SVG overlay with arrows and KaTeX labels, typeset panel
   with live values.
5. **3D:** orbit camera, heightfield of |f₁|, tangent plane at the iterate, slice curve,
   second surface view.
6. **Ship:** four-view CSS grid, a few hundred words of thesis, attribution footer.

**Milestones:** by the end of week 1, a draggable z₀ with a live Newton orbit in 2D. By
the end of week 2, a linked surface and tangent plane responding to that drag. If week 1
slips, cut the typeset panel, not the drag interaction.

**Deferred:** `.pen` parser, IR compiler and interval arithmetic, label collision
avoidance, general decorations, timeline beyond the scrub param, video export.

## Next figure: Doyle flagship

Three linked viewports, all the same construction:

- **Log space:** line families and the period parallelogram, hue by index.
- **Image space:** `exp` of the same, with spiral arcs and packing circles.
- **Period/parameter view:** the (p, q) choice, or the μ, ν solve, showing why it closes.

The existing Desmos graph overlays the first two in one plane, which makes it the best
before/after candidate. It also draws a variable-width stroke by hand as two offset
curves, which in Pencil would be one attribute on one curve.

## Hosting `.pen` files

- Same-origin `fetch().text()` works on any static host, whatever the extension.
- Third-party embeds are cross-origin and need the CORS header. Cloudflare Pages can set
  it with a `_headers` file, while GitHub Pages offers no header control:

  ```
  /figures/*.pen
    Content-Type: text/plain; charset=utf-8
    Access-Control-Allow-Origin: *
  ```

- The krisselden_com site has since landed on Cloudflare Workers static assets rather
  than Pages, so confirm this mechanism applies there.

## Open questions

- Do any Desmos pieces need to display the tangential configuration itself, or is the
  solve always done before anything is drawn? (from the chat)
- What is the smallest Rust IR that can describe a curve, now that it replaces closures?
- What does crossing the WASM boundary cost on each drag tick, against the site's budget
  of well under 10 ms on an older Mac and an iPhone? Measure this in a trace.
- How much of an imperative figure's logic (linking, drag constraints, animation) can
  move into a description, and which of the 28 figures would resist it?
- Which math properties can each renderer honor? The July 8 chat has path and tag
  selectors, `::inside` / `::outside` facets, and `self.dist` in fill expressions. SVG can fill the inside of a closed curve
  by winding, but anything that varies with signed distance needs a field shader. What
  should a renderer do with a property it can't draw?
- Can a style value depend on a param? Highlights over time need it. `Unroll` pops labels
  in at `a > 0.3`, and constructions fade older beats.
- Is the IR interpreted or compiled to a fast evaluator? Symbolic form also makes
  derivatives available for tangents and normals. The drag budget decides how far to go.
- How are constrained params expressed, and who maps a pointer back onto the constraint?
  The site does this with a hand-written `constrain` on each handle.
