# Pencil goals

Revised with Kris on 2026-11-05.

**The baseline is the July design in `docs/spec/`:** `PLAN.md`, `SPEC-15-identity.md`,
`SPEC-16-relations.md` and `SPEC-50-extensions.md`. Those files are unmodified originals.
This file records what has changed since, and it wins wherever they disagree.

The design chat is in the claude.ai pencil-lang Project:
https://claude.ai/chat/0107bcf4-4411-4d45-a15c-741285344ed7

## Purpose

Give visualization a declarative artifact: a `.pen` file that can carry style,
decorations, highlights and composition.

## The model

These hold from the plan and were reconfirmed:

- **A figure is a composition of functions:** `Scene : (t, inputs) → Frame`.
- **Animation is a free time param. Interaction is a free or constrained param** with some
  degree of freedom. Video export and interactive embed are profiles of one source.
- **Things have names, and styles target those names.** Only named bindings can be styled
  (SPEC/15).
- **Styles follow CSS, with math-specific properties.** Winding order gives a signed
  distance, so a style can set inside and outside colors, with counterclockwise positive,
  outside positive and inside negative. SPEC/15 does replace CSS specificity with ordered
  layers.
- **`.pen` files hold symbolic expressions.** The IR keeps them symbolic, and evaluating
  them is what "compute" means here.

Blog figures are where this starts, and they must not foreclose on animation or video.

## Quality bar

**SVG's line and text quality on the blog is the bar.** Kris expected implicit rendering
without tracing to be hard to style well. The stroke test below bears that out: traced
strokes reach SVG's look, and they're what makes dashes and tapers possible. So implicits
find curves, traced polylines draw them, and fields stay for fills and shading.

## Forms and hints

Decided with Kris on 2026-11-05, replacing the plan's invariant 6 ("every object has a
canonical scalar field; constructions lower to fields"):

- **Nothing is normalized to one representation.** A mark keeps the form it was written in:
  `implicit`, `sdf`, `parametric`, `segment`/`path`/`polygon`, `region`, `dot`, `label`. Going
  from parametric to implicit and tracing back would throw away the fast path. A field is
  only available where the form has one, or from a region's boundary.
- **The form goes in the document, and the hint goes in the style.** `draw: auto | line |
  field` picks how a stroke is drawn. `auto` takes the form's fast path: an exact SDF per
  pixel, a parametric curve by sampling, an implicit curve by tracing. Dashes and tapers need
  position along the curve, so they always use a line.
- **A hint can't change what a figure means.** `every_form_and_hint_draws_the_same_circle`
  enforces this, so a fast path can't quietly drift from the others. `sdf` is itself a
  declared guarantee, in the same spirit as SPEC/50's `field_quality`.

The same circle at 1024px on an M1 Pro, CPU per frame in WASM (GPU time not measured):

| Form | CPU per frame | RMS coverage error vs exact, 2px stroke |
|---|---|---|
| `implicit`, traced | 6.07 ms | 0.014 |
| `parametric`, sampled | 0.12 ms | 0.018 |
| `sdf`, per pixel | none | 0.010 |

So tracing costs about 50× what sampling does for the same result. `polar` and function
graphs aren't forms yet. Both reduce to parametric, so they'd be cheap additions.

**Step 3: `BoundsConstruction` from the site. Done 2026-11-05.** `corpus/004-bounds/bounds.pen` and
`web/bounds.html`: ten beats, a constrained drag, a `q` slider and a case choice that set θ, and a view
that refits on those but not during a drag.

- **The param model held.** A beat is a discrete param, `q` sets θ through its binding, and a drag
  overrides that binding, which is the same mechanism as time and the slider in `Unroll`. The same
  file also steps through its beats in an MP4, because `beat` is bound to time.
- **Constructions stay symbolic.** `circle`, `ray`, `intersect`, `mirror` and `perpendicular` lower to
  named scalars, and circles get exact distance fields. The pedal curve is written in the language.
- **The blog's figure is a comparison, not the source.** That post is a draft, and Kris reports its
  radio button between p = 0 and q = 2p has an issue that this version doesn't. At q = 8, q = 2p,
  step 10 the two agree on the readout, and opacities, widths and dashes come from its computed styles.
  The authority for this figure is the math, in the tests below.
- **The geometry is tested, not just the drawing:** neighbors touch the circle at 1 within 1e-9 at six
  angles, every circle keeps the ratio k to its distance from 0, the closing formula is right for q = 4
  to 12, and every binding stays finite across θ's whole range.
- **GPU matches the CPU oracle** within 1/255 at six times.
- **CPU per frame is about 1.1 ms in WASM** at 1530px, with 237 segments, up from about 0.85 ms before values
  could be absent. The two weren't measured at the same state. GPU time wasn't measured, and
  hidden marks still cost shader time because their opacity is computed per pixel.
- **Absent values came next**, with `corpus/005-tangents` as the test figure (below). This figure's numbers
  didn't change, to 12 places across θ's whole range, and it's still defined throughout.
- **Not done:** moving P with the keyboard, which the blog has, and a touch check on a real phone.

Syntax, built-ins and language decisions now live in `docs/language.md`.

## Values that may not exist

`corpus/005-tangents`, on `web/tangents.html`: a point has two tangents to a circle from outside, and
a chord through it from inside. Drag it across the circle and each construction takes over from the other.

- **What's built:** SPEC's `Option`, as an explicit condition on each value, with `exists` and `otherwise`.
  `docs/language.md` has the rules.
- **The geometry is tested:** tangent points and chord ends exist on the correct side of the circle at 300
  distances, sit on the circle, have a radius square to the tangent, and have length √|d² − 1| within 1e-9.
  At d = 1 all four land on P. Tangents by construction stay defined at 2000 angles, which caught a
  wrong tolerance in the first version.
- **GPU matches the CPU oracle** within 1/255 on both sides of the circle, at d = 1 and within 1e-4 of it.
- **No teleports:** PLAN's M3 test passes on this figure at dense steps, without a tracker. Each root is one
  formula, and existence depends only on the frame's inputs, so seeking stays pure.
- **Not built: branch labels and a tracker.** A root is stable while the line's direction is, but it swaps if
  the construction reverses that direction, such as `line(A, B)` as A passes through B. No figure needs that
  yet. SPEC's `:appearing`, `:vanishing` and `on appear: fade` also wait, so marks pop in and out.
- **A `fit` view can jump** when something it fits vanishes, as a slider moves. It's correct, and it may want
  easing or a rule of its own once a figure hits it.

## Decisions before the Doyle figures

The Doyle series is the main test from here. These came out of reading its figures on 2026-11-05, and
each one changes the language or the frame's order, so they come before more figures.

1. **Views and collections are the biggest gap.** Its figures are linked panels over lattices built in loops,
   and a Pencil figure has one viewport and no `for`. `LatticeMapping` needs both, and "which labels show"
   there is per view, so it's the next port. Reading the whole figure, it needs:
   - **Panels:** the same marks drawn through a different map in each, with their own `fit`.
   - **Collections:** up to about 500 circles and hundreds of dots. These can't each be a uniform and a block of
     shader, so they need to be one mark with a count that follows `q`, drawn through coverage layers.
   - **`solve`:** the map needs the exact skew, which `doyle.ts` finds by Newton's method. The IR has symbolic
     derivatives, so this could be declared and stay pure.
   - **Sizes that know the view:** dots and rings capped in screen pixels, such as `3 / plot.scale`.
   - **Arrowheads, and label text from expressions,** such as `−7`.
2. **Labels get roles, placement and visibility, all in style.** The document tags a role (`name`, `note`,
   `tick`, `display`), and a shared sheet holds the sizes, as the site's design system does. Style files also
   need shared color names, like its palette. `place: dodge` and `show: expr` are the new properties.
3. **One source of text widths.** Placement needs them, and today a page and a video measure differently.
4. **One way to be "not there".** `show` uses the same per-frame flag as absent values, so a hidden label stops
   blocking others. Beats could too, and stop paying shader time for hidden marks.
5. **Continuity comes from pure transitions, not memory.** A frame still depends only on its inputs. A discrete
   choice that changes (a label's spot, a step's view, what a `fit` includes) gets a smooth path that's a
   function of those inputs, not state carried between frames. Two cases so far:
   - **Labels (chosen 2026-11-05):** where springs settle, with a spring on top. Kris's test: it's the only one
     that "doesn't wiggle or jump without an obvious reason, and it does so smoothly". The placer is too jumpy.
     With a spring it's smooth, but it picks spots that are about to be trouble, so it makes "a quick series of
     corrections that look like someone being indecisive". Settling alone is good, but wiggly. Built in
     `place.rs`, with `corpus/theme.style` for roles and colors. A critically damped spring is a filter over the past, so a video and a seek can
     compute it from a window of earlier times. `lab/labels` measured this on a deliberately hostile figure:

     | Over one 24 s loop | Jumps | Biggest step | Touching something | Dodging motion |
     |---|---|---|---|---|
     | The blog's placer | 700 | 79 px | 1.1% | 114 px/s |
     | The placer with a spring on top | 0 | 5.8 px | 15% | 38 px/s |
     | Where springs settle, firm | 100 | 61 px | 11% | 39 px/s |
     | Where springs settle, soft | 26 | 31 px | 25% | 16 px/s |
     | Where springs settle, firm, with a spring on top | 0 | 5.8 px | 16% | 21 px/s |

     Counting how often a visible dodge turns back on itself matches Kris's words: 652 for the placer, 197 with
     a spring ("indecisive"), 1155 settling alone ("wiggly"), and 110 for settling with a spring.

     Settling alone doesn't stop the jumps: as a label's home crosses a line, the answer still leaps to the other
     side. The spring on top is what removes them, on either target. Settling then gives about half the motion
     for the same overlap, and a label's spot no longer depends on placement order. A page runs the same spring over its own clock, which a real drag on `web/tangents.html` confirms: as the
     distance sweeps, a spot leaps up to 41 px in a frame and its label moves at most 2.5 px. Not tried yet: a Doyle
     figure, phones, and an in-between spot in a video that holds still while its label is still gliding.
   - **Expressions (placement chosen 2026-11-05, in `lab/expressions`):** `SquareZ` on the blog draws 13 pieces of
     text and matches them by hand. Manim's rule is the aim: what matches moves, and only what doesn't fades. Manim
     finds matches by diffing, and here they're declared: a term's identity is how it's written, `z.1` is a copy of
     `z`, and an expression is a list of states. Kris on the placement: "beautiful". Now in core (`terms.rs`), with
     `corpus/006-square` and `web/square.html` as the figure. Its test checks the math itself: F.O. + I.L. is z², and
     its triangle is the first one scaled by |z| and turned. GPU matches the oracle within 1/255 at six times.
     Not built: statements like `zbi = zb · i` that would drive an arrow and its label from one line.
   - **What these transitions are for:** making the math feel physical, at a middle school level. `SquareZ` is F.O.I.L.
     on z²: F.O. is z·a and I.L. is z·bi, which build a right triangle similar to the one of a and bi, scaled by |z|
     and turned. Numbers have length, negative turns 180°, and i turns 90°, which gives lateral space, any direction,
     and so right triangles and trig. So a term should keep its color through the steps: blue stays the a side and
     green the bi side, and the color is what ties the big triangle to the small one.
   - **Moves are punchy:** a move gets near its destination fast, then eases in, leaving time to take it in, with
     little in between. Kris chose `1 − (1 − x)⁵`, since anything punchier goes by too fast to see `za` join `+zbi`. The
     blog's `snap` is a power of 4. The power is a language default for now, though a theme is where it belongs.
     The balance, from Kris: a reader needs a sense of where a thing came from, so it doesn't seem to come out of
     nowhere, but shouldn't have to wait for it, especially when watching a few times to learn. A reader's eye may
     also be on the last thing to move, so what started earlier has to stay readable as it arrives.
   - **Views (not built):** the site moves between a step's views along the van Wijk and Nuij path
     (`src/lib/zoom.ts`, used by `NewtonDoyle`). The `beat` param can carry this: a fraction between two beats
     is a point on the path between their two `fit` views, so a page tweens the param, and a video binds it.

## Collections and panels

`corpus/007-lattice`, on `web/lattice.html`: one lattice through the maps that make a Doyle spiral, in four panels
from one file. `docs/language.md` has the rules.

- **The geometry is tested:** every circle touches its six neighbors within 1e-9 of its size, at 169 grid points, and
  the period lands on 2πi in log space and on 1 in the spiral. That checks the site's solved constants.
- **GPU matches the CPU oracle** within 1/255 in each panel, cropped from the shared canvas, so where panels sit is
  tested too. It runs on WebGPU and WebGL2.
- **Speed:** tuned by profiling on 2026-11-06, and nothing had to be undone. A frame of this figure went from 20 ms to under 8 ms
  natively. Most of the gain came from two things. A circle much bigger than the frame only samples the arc that can be
  on it, which saved about 6 ms and is also more accurate. And curves and collections run through `fast.rs`, which resolves
  names and functions up front, instead of looking them up for every evaluation. `eval` stays the definition, and a test
  requires the two to agree to the bit on every expression in the corpus. In WASM at 1720×1745 that's about 8.6 ms a frame,
  from 19 ms, and about 10 ms when q or p changes. The worst case tried, q = 12, p = 7, is about 12 ms. Most of what's left
  is the bindings, and the settling and strokes of a large spiral.
- **`solve` gives it real q and p.** The skew comes from two equations in the file, using exact derivatives. Every circle
  touches its neighbors for all 70 of the q and p the blog's sliders allow, and at q = 7, p = 2 it agrees with the site's
  own solver within 1e-9. Solving doesn't remember, so seeking stays pure, and no solution is absent.
- **It's `LatticeMapping` now,** as far as I can compare it: edges with arrowheads, step dots that shrink with their circles,
  signed counts on the flat panels and plain ones on the spiral's, the wrapped copies in log space, and the marker box. In
  the flat panels the name of 0 goes outside the triangle, since every edge leaves it on the inside.
- **A thing is the same thing in every panel.** Kris's point: 0 and 1 in the spiral aren't two things but one point, and only
  the name is the space's. So `start` and `end_point` have those names in every panel, and a label keeps its identity across
  them while its text changes, which a test holds it to. This is a connection to keep and build on, such as pointing at
  something in one panel and seeing it in the others.
- **Log space's copies are there for the crossings:** Kris's point is to show the cylinder wrapping, so the crossings there
  connect to the same ones in the spiral, with 0 and 2πi becoming 1. How light they are is one rule in the style.
- **Not like the site:** the panels have no frames, since a panel's border is the page's business, and the title's bold isn't
  there, since a font names only a role for now.

## Text without a page

Compared on 2026-11-05 in `lab/text`, with results on `web/text.html`. 269 glyphs at 1024×460 on an
M1 Pro, the same outlines and positions, scored against 16×16 point samples per pixel:

| Renderer | RMS vs exact | Position error | Frame |
|---|---|---|---|
| Whole-pixel `fontdue` (the old stopgap) | 0.241 | 0.33 px | 0.33 ms |
| `ab_glyph`, CPU | 0.022 | 0.003 px | 0.43 ms |
| Winding integral, CPU | 0.005 | 0.005 px | 16 ms |
| Winding integral, GPU | 0.009 | 0.005 px | 2.2 ms |
| Vello, area | 0.079 | 0.002 px | 3.3 ms |
| Vello, area, outlines flattened first | 0.009 | 0.005 px | 6.6 ms |
| Vello, MSAA16 | 0.087 | 0.012 px | 3.5 ms |

GPU and Vello frames include reading the image back, and CPU frames don't composite anything.

- **Decided for now: `ab_glyph` for video labels.** It fixes the visible flaw, whole-pixel jumps,
  and it's the cheapest accurate option. The magnified crops are hard to tell apart.
- **Vello's error comes from curve flattening.** Flattening first brings it level with the integral.
- **The winding integral is what to reach for if text needs to scale or animate heavily,** or take
  per-pixel effects such as outline normals. It's written from windfoil's published formula, in
  `lab/text`. That version needs storage buffers, so it isn't on WebGL2 as written.
- **Chrome's crispness comes mostly from positioning.** Snapped to whole baselines and quarter-pixel
  x, the difference from Chrome's canvas falls from about 0.18 to 0.05–0.07, with `ab_glyph` closest.
- **Not compared:** Chrome's DOM text directly, since a page can't read its own pixels, as well as
  shaping, hinting and other fonts. Only Charter Italic and Helvetica were tested.

## Changes from the plan

| Topic | Plan | Now | Status |
|---|---|---|---|
| Parser, IR, compute | TypeScript packages | Rust crates | Decided |
| Stability | Kernel frozen first; IR changes need a schema bump and migration note | Iterate before locking. Semantic invariants stay as the design's intent, and the process waits until the design settles | Decided |
| GPU rendering | Three.js and TSL | wgpu, on the web through WASM and natively for video | Proposed |
| Shader generation | IR → TSL node graph | IR → WGSL, with the IR staying canonical | Proposed |
| Geometry | Every object has a canonical field | Each form keeps its own representation | Decided, see above |
| Strokes | Not specified | Lines of instanced segments with max-blended coverage, or per pixel for an exact SDF. Works on WebGPU and WebGL2 | Tested, see below |
| Text and vector fills | Not specified | Windfoil and Vello are candidates | Open |
| Edge blending | Not specified | Gamma-encoded sRGB by default, as SVG does. `blend: linear` is available | Decided for now |
| Symbolic math | Compute Engine (a JS library) for LaTeX input and automatic inverses | LaTeX input deferred. Automatic inverses for linear equations written in Rust | Proposed |
| Extensions | `.ts` reference plus a TSL `Fn` | Rust reference plus WGSL. SPEC/50's principles stay | Proposed |

**The CPU interpreter is the test oracle, not the renderer.** It stays because it makes the
semantics testable without a GPU and referees GPU output. Production rendering is on the
GPU, in the browser and for video.

## Draft assumptions

Proposed by Claude and not contested so far. Change them freely.

- `.pen` files are text, written by Kris and Claude. The plan does also want an editor
  eventually.
- `krisselden_com` is the corpus and the quality bar for look and feel, but not the authority on
  whether a figure is right: that's the geometry. Its figures stay untouched until Pencil has
  proved itself.
- Pencil isn't being built as a product for now.
- Third-party embeds and the attribution footer are deferred.

## Open questions

- **Is WebGPU needed?** Not for strokes or fields: both ran identically on WebGL2. Text is
  the untested part, and it's the likelier reason. Vello relies on compute shaders, which
  wgpu's WebGL2 backend lacks. Windfoil's README describes a fragment shader with CPU-built
  bands, and it needs a WebGPU-capable GPU today. It takes quadratic Bézier contours and
  gives winding and coverage, not the distance `self.dist` needs.
- **How should a moving loop's dashes behave?** A loop gets a whole number of dashes, and
  that number changes as the loop grows, so dashes jump. On the breathing circle that would
  be about 25 jumps a cycle by arithmetic; it hasn't been watched in motion. The blog hides
  this with hysteresis, which depends on history and so breaks seeking.
- **What should `self.dist` mean?** It's still the first-order estimate. Tracing could give
  a true distance to the curve near it, at a cost.
- **What in the kernel still holds?** Invariant 4 (MathJSON) still deserves a fresh look.
- **What do `px` mean without a design width?** With `viewport { width: 640 }` sizes scale with
  the output, like an SVG's viewBox. The blog keeps its labels a fixed screen size while lines
  scale, which this can't express yet.
- **How should a `.pen` file name fonts?** `font: math | sans` are roles, and each renderer maps
  them to whatever it has. Video currently uses system fonts, which can't be redistributed.
- **Should still labels snap?** Chrome's crisper text comes mostly from whole-pixel baselines.
  Video could snap a label whose position can't change, which the IR can tell symbolically, and
  keep fractions for moving ones. Not built.
- **Where is `SPEC/18`?** The July 9 chat mentions it, but the plan's list stops at 17.

## First milestone

**Step 1: `circle.pen`. Done 2026-11-05.** One file with an implicit circle, a radius that
depends on `time`, and `::inside` / `::outside` colors in a separate style file. Rust parses
it, builds the IR, and evaluates it, and it renders:

- in a browser through WASM and wgpu, reloading when the file is edited;
- natively through wgpu to a PNG at a chosen `time`;
- on the CPU interpreter, which referees both.

Measured on an M1 Pro at 1024px, in the desktop app's browser pane:

| Compared | `circle.style` | `bands.style` |
|---|---|---|
| Browser WebGPU vs CPU | 0/255 | at most 1/255, on 152 pixels |
| Browser WebGL2 vs CPU | 0/255 | at most 1/255, on 152 pixels |

Native wgpu vs CPU at 512px, at four times: 0/255 and at most 1/255. A deliberately wrong
stroke width fails `pencil check` at 84/255, so the check can fail.

Findings for what comes next:

- **WebGL2 isn't the constraint for per-pixel fields.** This path ran on it unchanged. The
  WebGL2 question is really about the vector layer.
- **`self.dist` isn't a true distance.** It's the first-order estimate `f / |∇f|`, which
  distorts where the gradient shrinks: `bands.style` shows moiré at the circle's center,
  where a true distance would give even rings. This is the styling problem Kris predicted.

**Stroke test. Done 2026-11-05.** Marching squares with Newton refinement, drawn as
instanced segments with an exact box filter and max blending. `web/strokes.html` compares it
with SVG at 1024px on an M1 Pro. Error is RMS coverage error against a 16×16 supersampled
exact shape, for black on white:

| Case | SVG | Traced, gamma | Traced, linear | Field, gamma |
|---|---|---|---|---|
| Ink, 2px | 0.079 | 0.010 | 0.192 | 0.008 |
| Ink, 1px | 0.074 | 0.010 | 0.194 | 0.005 |
| Half a device pixel | 0.078 | 0.009 | 0.169 | 0.004 |
| Dashed, 2px | 0.091 | 0.012 | 0.188 | not possible |
| Tapered, 0.5px to 12px | 0.032 | 0.014 | 0.193 | not possible |

- **Traced with gamma blend looks like SVG** in the magnified crops, and blend space was the
  main difference from SVG: the mean difference on differing pixels falls from about 46 to
  16 on the 2px line. This confirms the guess from Step 1.
- **Closer to exact doesn't mean better looking.** SVG's rasterizer isn't a box filter, and
  it was rasterized through an image here, which may differ from inline SVG on a page. Kris's
  eye decides this one.
- **GPU matches the CPU oracle** within 1/255 on all six traced cases, and browser results
  were the same on WebGPU and WebGL2.
- **Tracing costs about 6 ms a frame in WASM** at 1024px for one curve, using the naive
  interpreter over a 173² grid. That's most of the drag budget of well under 10 ms, so it
  needs interval pruning or a compiled evaluator before figures get busier. Neither has been
  tried. GPU time wasn't measured.
- **Not tested against SVG:** text, arrowheads, sharp corners, zooming, self-intersections and
  high curvature. Tracing uses a fixed 6-pixel cell, which will miss fine detail.

**Step 2: `involute/Unroll` from the site. Done 2026-11-05.** `corpus/003-unroll/unroll.pen`,
with one param `a` driven by `tau·cycle(mod(time, 13))` or by a slider:

- **Interactive:** `web/unroll.html`. The slider takes `a` over, play hands it back at the
  matching time, and style files can be switched live. Runs on WebGPU and WebGL2.
- **Video:** `pencil video` writes a 13-second, 60 fps MP4. Each frame comes from its own time
  alone, at about 8 ms a frame at 1280×886 including text and encoding.
- **Restyled:** `night.style` adds dashes, a taper and distance-banded fill, and a test confirms
  the IR doesn't change.

Compared with the blog's figure at the same angle, the geometry and readout match, and sizes,
colors, caps and label positions come from the page's computed styles.

- **GPU matches the CPU oracle** within 1/255 at 640 and 1280 wide, in both styles, at six times.
- **CPU per frame is about 0.2 ms in WASM** for the whole figure at 1530px, with 204 segments and
  two regions. The drag budget is well under 10 ms. GPU time wasn't measured, and regions test
  every boundary edge for every pixel, which is the most likely cost as figures grow.
- **Text is two systems.** In the browser, labels are DOM text, so they match the page's quality.
  In video, they're CPU-rasterized from outlines. See the text comparison below.
- **Traced lines are the least accurate path** at small sizes, at most 26/255 on edges at 256px
  against 7/255 for sampled curves, because of tracing's fixed 6-pixel grid.
- **Known limits of regions:** coverage assumes one straight edge per pixel, so sharp tips are
  approximate, and the 512-entry uniform array caps the vertices a figure's regions can use.
- **Dashes on a moving range ride with its start.** `wound` runs `a..tau`, so its dashes move with
  P. They're continuous now, since a moment when a moving range's ends meet no longer counts as a
  loop, but a real string's marks would stay put as it unwinds. Anchoring dashes at the far end
  would need a declarative option, and it isn't built.
- **Not checked:** other browsers or devices, and a drag on a real phone. The slider resumes at the
  earliest matching time, so a value the unwind and rewind share always resumes on the unwind.
