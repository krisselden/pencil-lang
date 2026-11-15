# Pencil goals

Revised with Kris on 2026-11-05.

**The baseline is the July design in `docs/spec/`:** `PLAN.md`, `SPEC-15-identity.md`,
`SPEC-16-relations.md` and `SPEC-50-extensions.md`. Those files are unmodified originals.
This file records what has changed since, and it wins wherever they disagree.

The design chat is in Kris's private claude.ai pencil-lang Project.

## Purpose

Give visualization a declarative artifact: a `.pen` file that can carry style,
decorations, highlights and composition.

It's the tool Kris wants for his own math explainer videos and interactive figures, and it should be ambitious, polished
and high quality. It's public on purpose. A declarative text format with open expressions is meant to invite participation, and to
mean that what people make with it isn't locked in, including being able to export their own video.

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

## Open

Everything still open, from every doc, gathered on 2026-11-09. The sections below are the record of how each figure went,
and this is the one list to read for what's left. When something here is settled, remove it and write the decision where it
belongs: `language.md` for the language, the section below for a figure.

### Now

- **The explainer, phases 5 and 6** (`plan-explainer.md`): narration over the drafted cut, then a write-up here and in the
  language reference. `lab/narration` times beats from a script, drafted with a synthesized voice ("Narration timing" below).
  The whole script is drafted, with the glow left silent. Next: a take in Kris's voice.
- **Names and notation**, agreed with Kris on 2026-11-12, in this order. `web/ast.html` shows how text reads at each step.
  Subscripts, implicit products, constants a figure can take the name of, and pasting LaTeX are done (`language.md`,
  decisions of 2026-11-12). Next:
  1. **Files and libraries.** What Kris wants from it (2026-11-13), which is four things with one mechanism under them:
     - **Split a long figure.** The limaçon explainer is 223 lines and 32 beats in seven numbered sections, and it's the
       figure that shapes this.
     - **Helpers written once.** The corpus defines 25 functions, some twice (`dir`), and two different things are both
       called `turned`. Sheets want effects the same way: glows, blurs.
     - **Libraries by area of mathematics,** as TypeScript's `"lib": ["ES2022", "WebWorker"]` names an environment: which
       built-ins and constants a figure can expect. Written in Pencil. A letter can be a variable in one field and a
       constant in another, and Desmos gives `r`, `θ`, `t` and `τ` meanings that a figure loses once it assigns them.
     - **Animated constructions** to import, such as a perpendicular bisector that draws its two arcs and then its line:
       useful, and too specific to be global. A `group` with arguments can't hold beats yet, so this needs that too.

     **Built 2026-11-14: `use "file"`,** which is the first two. A used file's statements join the figure where the line
     is, once, in the figure's one set of names (`language.md`, Documents). The core says which files it needs and a host
     reads them: the CLI from disk, and `<pencil-figure>` and the playground by fetching, where each used file gets a tab.
     The explainer is now `lab/explainer/limacon.pen` and a file to each of its seven parts, and its frames are the same
     as before. A sheet can use a file too, which is where shared effects would go. Still to do:
     - **Share the corpus's helpers.** `dir` is the only function two corpus figures both define (`004-bounds` and
       `005-tangents`). Nothing was moved, since a corpus figure and its golden IR are one file each today.
     - **Named libraries:** `lib complex, hyperbolic`, shipped with Pencil and written in it, whose names are defaults a
       figure can take. A figure that names its libraries gets only those, so `lib geometry` leaves `i` and `e` free.
     - **Groups that hold beats,** for constructions that animate.
  2. **Labels from expressions:** a label that shows its own expression, as ggplot2 takes axis titles from the unevaluated
     arguments R passes it. The IR keeps expressions symbolic, so the text is there. A style's `text: expr = value` (2026-11-13) writes the
     expression as typed and its value as a number; what's left is writing it as math (`σ(2)` set as notation, not code) and
     an exact value, `−½ − (√3/2)i`, where one exists.

### The blog on Pencil

Planned with Kris on 2026-11-14. The goal is the site's figures drawn by Pencil, and `pencil-lang.dev` as the project's own
site. The site has 33 figure placements from 31 sources: 20 interactive, 9 animated, 1 still and 3 in 3D. All but the five
GLSL shaders draw through one SVG system (`src/lib/plot.ts`), and nothing is made at build time: each figure ships as an empty
`<svg>` or `<canvas>`, with no poster and no fallback, and its social image is captured by hand in dev and committed.

1. **3D, as far as the site's three figures go.** All three (`RollTube`, `LevelCurve`, `NewtonDoyle`) are orthographic, and
   only strokes and flat fills: no lighting, no perspective, no depth sorting.
   - **Built 2026-11-14: points in space, and `RollTube` as `corpus/013-tube`** (`web/tube.html`; `language.md`, Points in
     space). `(x, y, z)` is drawn looking down z, a viewport's `yaw` turns the view, and a sheet reads `self.z` along a
     curve, in `width` and `opacity`, and per dot, in `radius`.
     - **What "behind" means while it rolls:** the tube's axis is the x axis and the strip is wrapped on a cylinder of
       radius 1/roll about it, so the side facing away is `z < 0` at every stage, and nothing switches. Half rolled, what
       fades is the part of the sheet seen from its back. Turning the tube slides the strip around it, and the view `fit`s
       the strip, so it stays in frame as it rolls, in place of the site's zoom of 1 + 0.4·roll.
     - **What the site's did,** from its source: a row was cut at the nearest of 48 samples, so the cut jumped; front and
       back was the sign of `z` after the turn, which is only the tube's back when it's fully rolled; everything counted as
       front below `roll = 0.05`, so rows faded at once as it passed; dots were drawn on the front only, so they popped; and
       the period arrow was never cut. Here the cut is at the crossing, to a quarter of a pixel, the dots shrink away at
       the edge, and the arrow and its head fade with the rest.
     - **Checked:** a row off the top comes back as the row p along for p = 1, 2 and 3, and the seam closes only when fully
       rolled; the GPU matches the oracle within 1/255 at four settings; 183 frames across every `p`, `roll` and `turn` draw
       in a browser at 1633×919 and at phone size. **Not checked by Kris:** how it looks moving.
     - **Left out:** `roll` stops at 0.01, since the cylinder's radius is 1/roll and Pencil has no way to write the flat
       limit. A color can't follow depth, only width and opacity.
   - **Next, a view that turns:** pitch as well as yaw, bound like any param so a drag orbits, for `LevelCurve` and
     `NewtonDoyle` (the site's `wire3d.ts`, which has z up and turns about it). Their planes are translucent polygons, so
     what's drawn over what needs an answer there, and their surfaces are grid lines, which `curves` already draws.
2. **The host: what a page and a build do with a figure.** Designed on 2026-11-14 ("The host" below). Built: the compiled
   figure, `pencil build`, the `poster` statement, the element's poster, `<source>` children and `compiled` attribute, and
   (2026-11-15) the element as a package, `@pencil-lang/figure` in `packages/figure`, with the Astro integration and
   `<Figure>` in it. In short: a build makes a compiled figure, a poster and what the page needs to know about the figure,
   for each set of sheets a page can pick. The element shows the poster until its first frame and picks the set by media
   query. The Astro component does both for a page. Not done: trying it on the blog.
3. **`pencil-lang.dev`:** the docs, the playground and the corpus as demos. An Astro site in `site/`, so it's the first user of
   the integration, before the blog. **A first version is built (2026-11-14)** and not deployed: a landing page, the language
   reference rendered from `docs/language.md`, the corpus and the explainer drawn live, and the playground in a frame
   (`site/README.md`). Since 2026-11-15 its figures come through the package's `<Figure>`: compiled at build, each with a
   poster, and no sources shipped. Kris hasn't looked at it.
4. **The move itself is tracked in `docs/migration.md`:** the order, what's done, and each of the site's 31 figures with the
   corpus figure that covers it and what Pencil lacks for it. **The Doyle series goes first** (Kris, 2026-11-14), in its own
   order, and the shader post's three GLSL figures stay GLSL, since that post is about writing a shader. Keyboard use comes
   with the host, before the posts: handles a reader can tab to, and arrow keys. 8 figures are done (the shader
   post's `ScaledCopy`, `LogNearest` and `UnwrapMorph` on 2026-11-15, drawn on the site's demos page at 1633×919 and
   375×812, light and dark), 6
   are ports with what exists, 9 need one or two features, 5 need design and 3 stay. What most figures lack is around the figure
   and not in it: arrow keys (12 figures), dragging a point in two dimensions (5), Newton's steps as values (4) and panels of
   different sizes (4).

### Proposed order after that

Proposed by Claude on 2026-11-09, not yet agreed:

1. **A custom element:** built as `<pencil-figure src sheets>` ("The element" below), and the playground and every figure
   page in `web/` draw through it. Next: the site.
2. **A highlighting grammar** for `.pen` and `.style`. Cheap, and it doesn't fix the syntax in place.
3. **A playground with an editor:** the element, an editor, and the WASM check, for people who want to try it. A first one for
   Kris is built ("The playground" below), ahead of the element and the grammar, so he can react to the syntax now.
4. **A syntax pass,** before anything hardens the syntax. Split `compile_flat` first (below).
5. **A language server,** diagnostics first, since errors already carry a line and column. Hover and completion once the syntax
   settles.
6. **MCP:** not until someone needs it. The CLI already renders and checks.

### Questions

- **Is WebGPU needed?** Not for strokes or fields: both ran the same on WebGL2. Text is the untested part. Vello relies on compute
  shaders, which wgpu's WebGL2 backend lacks, and the winding integral as written needs storage buffers.
- **How should a `.pen` file name fonts?** `font: math | sans` are roles. Video uses system fonts, which can't be redistributed,
  so someone else's render looks different. The math font also has `ᵗ` but not `ⁱ`, with no fallback.
- **How should a moving loop's dashes behave?** A loop gets a whole number of dashes, so they jump as it grows: about 25 times a
  cycle on the breathing circle, by arithmetic, not watched. The blog's hysteresis depends on history, which breaks seeking. A
  related one: dashes on a moving range ride with its start, and a real string's would stay put as it unwinds.
- **Should still labels snap?** Chrome's crisper text comes mostly from whole-pixel baselines. The IR can tell which labels never
  move. Not built.
- **How far should MathJSON go?** It stays, since open expressions are the point. But the IR only goes one way and is the compiled
  form, so what's truly open today is the `.pen` text.
- **Should a `.pen` file know its narration?** Today `lab/narration` rewrites the pauses, so the anchors live in the script
  and the `.pen` gets numbers. A beat that says which word it lands on would keep that link, at the cost of the figure needing
  a take to have a length.
- **Should `pencil check` hold captions to their labels?** A caption that names a point the figure doesn't label is now a mistake
  by Kris's rule, and `lab/narration` checks it by matching letters and numbers. The compiler could check it exactly, if a caption
  could refer to a point by its name rather than spelling it.
- **Should E have a label of its own** before it's called z²? The narration avoids saying it for now.
- **How much of a control should a `.pen` say (Kris)?** It says a param's range, its `step`, and now a choice of values,
  `param axis in {ω, 1, 1 + ω}`, and the playground builds sliders and choices from those, as the site builds a shader's
  controls from `//default, [min, max] {a, b, c}` comments. What's left is whether it should say a control's label, or
  whether that's a sheet's. A choice of names that aren't values, such as `{clip, tanh}`, waits for a figure that needs one.
- **Can a figure's accessible text come from the document (Kris)?** The IR knows what each mark is and what it's built from,
  so a reading like "a circle about C through 1" could be generated, with no alt text written by hand.
- **Should a `fit` view ease** when something it fits vanishes? It jumps, which is correct.
- **Where do easing and themes live?** The power in moves is a language default, and a theme is where it belongs. Sheets are
  picked by the page, since the language has no media query.
- **Does Cloudflare Workers static assets take a `_headers` file** for cross-origin `.pen` embeds? Unconfirmed
  (`design-notes.md`, "Hosting").
- **Where is `SPEC/18`?** The July 9 chat mentions it, and the plan's list stops at 17.
- **The specs' own open questions** (SPEC/16 §8, SPEC/50 §7) are about relations, rigs and extensions, none of which a figure has
  needed yet.

### Known limits

Built, and wrong or short in a known way. None has blocked a figure.

- **Copies drawn early:** in `006-square`, a copy of `z` was drawn on top of `z` a state early, so it was darker. A bug in how
  copies are drawn, not fixed.
- **Roots can swap** if a construction reverses its line's direction, as `line(A, B)` does when A passes through B. Branch labels and
  a tracker would fix it.
- **Marks pop in and out:** no `:appearing`, `:vanishing` or `on appear: fade`. Rim-only disks also pop a half-plane on as a circle
  straightens.
- **Dashes:** a straight line through exactly three points has none for that moment, and a collection's dashed rims are chords on
  enormous circles.
- **Sides on collections** cost about 0.09 ms a circle, so they're capped at 256. Instanced quads with culling are the way past it.
- **Regions** assume one straight edge per pixel, so sharp tips are approximate, and the 512-entry uniform array caps their vertices.
- **Distance to an implicit curve** only counts the curve as traced: the view, and one cell or the stroke's half width past it.
  Near the frame's edge, a part of the curve just out of view can be nearer than the distance says.
- **A traced curve's dashes and taper** measure along the line as traced, so they depend on how far past the frame it runs.
- **Tracing** uses a fixed 6-pixel cell, so it misses fine detail and is the least accurate path at small sizes. It still evaluates
  through `eval`, about 6 ms a frame for one curve in WASM, since `fast.rs` only serves curves and collections.
- **Terms:** a term that isn't an operator sets tight (`cosh s` printed as `coshs`), and a label is always centered on its point.
- **A trimmed circle starts due east,** since circles keep only a center and radius.
- **A Möbius pole exactly on one of the three points** loses that circle for the frame.
- **Panels** all have to be one size, and a sheet can't hide or move one.
- **Older figures:** those before the lattice still use `offset`, which a sheet can't move, and their pages keep their own palettes.
  `lab/explainer/proof.style` sizes its labels in `px`, so they shrink with the figure.
- **`sin`, `cos` and the rest** take numbers only.
- **A font names only a role,** so the lattice's bold title isn't there.

### Not built

What a figure has pointed at but not needed yet. `decisions.md` lists what the specs have that isn't built.

- Statements like `zbi = zb · i` that drive an arrow and its label from one line.
- Pointing at a thing in one panel and seeing it in the others.
- Lists as general values (39% of Kris's Desmos graphs), and piecewise values beyond `where` and `otherwise` (26%).
- `polar` and function graphs as forms. Both reduce to parametric.
- Dashes anchored at a range's far end.

### Not checked

- **Phones:** a touch drag, the sides benchmark (`lab/sides`), and label springs.
- **Other browsers and devices.** Everything has been measured on one M1 Pro, mostly in Chrome.
- **GPU time,** anywhere. Every timing is CPU.
- **Label springs on a Doyle figure,** and an in-between spot in a video that holds still while its label is still gliding.
- **Against SVG:** text, arrowheads, sharp corners, zooming, self-intersections and high curvature.
- **The explainer in a browser.** It was only rendered natively.

### Code

- **`compile.rs` is 4,257 lines, and `compile_flat` alone is 889.** Every language feature has gone through it. It's the one place
  where changes cost more than they should, so split it before the syntax pass. The parser's `statement` (360 lines) is next.
- The pairs that must agree are each held by tests: `eval` and the WGSL emitter, stroke coverage and its shader, and `eval` and
  `fast.rs`, to the bit.

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
- **Not done:** a touch check on a real phone. Moving P with the keyboard, which the blog has, was built on 2026-11-14
  ("The element").

Syntax and built-ins now live in `docs/language.md`, and language decisions in `docs/decisions.md`.

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
   - **Movement has budgets (Kris, 2026-11-06):** a cognitive budget and a time budget, and movement has to aid understanding
     before it exceeds either. Nothing should come out of nowhere or have too much going on, and once a reader has the idea, it should
     complete in a timely manner. As long as a reader can read what's happening, the punchier the better. This is the rule the options
     below were judged by.
   - **Distortion spends that budget fast (Kris, 2026-11-06):** a view stretched far out of square, to fit more range, looked wrong in a way
     that took him a moment to name, and he thinks excess translation made a zoom confusing around the same time. Prefer equal scales,
     and treat a stretch as a decision.
   - **Views (not built in Pencil, chosen in `lab/zoom`, committed on the site):** the site moves between a step's views along the van Wijk and Nuij path
     (`src/lib/zoom.ts`, used by `NewtonDoyle`). The `beat` param can carry this: a fraction between two beats
     is a point on the path between their two `fit` views, so a page tweens the param, and a video binds it. Kris named
     auto-framing with interpolated zoom as a long struggle, and the lab ports the two figures he pointed to. In both, the
     camera's path is right, and what's wrong is what moves with it: the camera is interpolated, and the subject isn't.
     - **Slider drags (`DoyleFlower`), chosen: the view locked to the circles.** On the site, the circles change on each step while
       the view eases after them, so the center circle shrinks in a frame and grows back: 18 reversals at 25 ms a step, where Kris sees it
       vibrate, and 0 when locked. Easing `q` also has 0, and Kris kind of likes it, but locked feels cleaner, and easing also looked wrong
       to him when `p` changed. The drawn circles still touch while `p` eases at Newton step 5 (under 0.001 px), so that isn't a failure of
       tangency there, and what he saw isn't pinned down. The unit circle is the zoom indicator, so it's solid.
     - **Newton steps (`NewtonDoyle`), chosen: subject held, with a quick zoom-out.** The path is its own inverse within 3e-10 of a view
       width, but the subject changes as a move starts: going in it's already in frame, and coming out it isn't, for 1.5 s of 2.2 s. The
       paper assumes every view is equally interesting and sets cognitive aspects aside, which doesn't hold near a root where nothing is
       happening. Holding shows the shallowest requested step that's in frame, a function of the view alone, and it reads best when a
       reader interrupts by playing with the steps. Zoom-out is quintic, at 800 ms in the lab, and still reads. Zoom-in stays as it was.
     - **Not chosen: a continuous step.** It looks good, and it's true at every value, but these are discrete steps, and discreteness
       wasn't the problem. A figure's motion should match the problem it shows. Being without a subject for a moment in a quick zoom-out
       wasn't a problem either.
     - **What reads as a zoom:** something fixed to the world. Some options felt "more a reshape than zoom" before the plot had a grid. Each
       line has one level and fades with how many pixels its cell covers, so nothing pops, and Kris reports none of the original
       issues in any option. In the site's code, a coarser level arrives at full opacity on top of its lines at each decade. The 3D lines are
       fixed to the surface for the same reason, and Kris says they look good.

## What Kris's own graphs ask for

`krisselden_com/docs/desmos-catalog.md` catalogs his 583 Desmos graphs by topic, with the states saved in `design_examples/desmos-states/`.
The catalog treats each graph as a sketch of the math, not a design, so they're a source for corpus figures, not something to convert
wholesale. Counted across all the saved states (about 15,000 expressions, a median of 20 per graph), by regular expression, so roughly:

| Share of graphs | Uses | In Pencil |
|---|---|---|
| 53% | Parametric curves | Yes |
| 50% | Complex mode | Yes, since a point is a complex number |
| 39% | Lists | As `dots`, `circles` and `curves`, though not as general values |
| 33% | Dashed lines | Yes |
| 26% | Piecewise values | Only as `where` and `otherwise` |
| 20% | Polygons | Yes |
| 16% | Animated sliders | Yes, as params bound to time |
| 12% | Draggable points | Yes |
| 7% | Derivatives | Inside the language only |
| 6% | Actions | No, and a frame stays a function of its inputs |
| 4% | 3D | No |

**Two figures so far come straight from the catalog, chosen by Kris:**

- **`corpus/008-hearts` ("Bipolar Hearts", `web/hearts.html`):** the whole figure is `z = coth((τ − σi) / 2)`, as the graph writes it. It only lacked `coth` and
  `arctanh`, now built in for numbers and complex numbers. Its coordinate circle becomes a line and flips over as τ crosses 0, which found a real bug: just beside
  that moment, samples either side of the pole could be joined by a chord across the frame. A curve now breaks where a chord still isn't flat at the deepest
  subdivision. The test checks every curve against dense sampling, on both sides of the degenerate moment, and at the graph's poles.
- **`corpus/009-squaring` ("Squaring map"):** a circle through n and 1/n and a grid inside it, through `z²` and a morphing `z^p`. `use … through` fits it exactly. Its
  square is tested against the limaçon, with k = tanh(ln n). **It showed the list gap plainly.** Desmos writes `s = [-3...3] / 3` once, and this file wrote 14 `use` lines. A `curves` collection replaced them with two
  lines, and every pixel the CPU oracle draws stayed the same, at the three times compared. The figure went from 47 marks, 45 coverage layers and 50 bindings to 11, 9 and 8, its shader from 21 KB to 9 KB, and a frame's CPU work stayed at about 1.5 ms.

- **`corpus/010-mobius` ("Coxeter's loxodromic sequence", `web/mobius.html`):** circles at powers of w, through `f(z) = (az − b) / (cz + d)`. Kris made this one to be written in neither
  re/im nor abs/arg, and it is. It found four things:
  - **A stretched view.** At Kris's values, two circles go out to nearly lines, about 26,000 px across. A panel 0.07% off the shape of its view moved them by tens of pixels, so a line cut through
    circles it should touch. He spotted it side by side with Desmos, and tests of the formula couldn't. A view now always has equal scales, and a test holds what's drawn to circles fitted independently.
  - **`bind` could fail at runtime** because of where its `param` was written. Bindings now run in dependency order.
  - **A collection could send NaN to the GPU** at a pole. An instance with no finite center or radius is skipped.
  - **A wrong determinant in the figure itself** (`ad + bc`, since the numerator is `az − b`), which independent tests caught and the picture didn't.

  Kris hid the graph's Spiral folder on purpose, since it's a Doyle spiral and well covered, so this figure doesn't draw it either. `through` refuses `circles`, since the image of a circle usually isn't one, but a
  Möbius map is exactly where it is, so the figure sends three points of each circle through the map, in order. That's its image, with the image of its disk on the negative side, with no closed form to get wrong. A disk whose
  circle holds the pole has the outside as its true image. Kris likes those as a rim only, which `plain.style` does in one rule, and `outside.style` draws the true image.

Both export video directly, in about 7 to 10 seconds for 8 seconds at 1080 wide and 60 fps.

## Which side is inside

From Kris, 2026-11-06, on the Möbius figure's rim-only disks. Which side of a circle is its inside can differ from which side has the natural handedness, and it's useful to target
either side, or both at once. The blog's circumcircle shader tints by the sign of det(B − A, C − A), which flips as vertices pass each other, as the circle goes through a line. His limaçon shader
(fragcoord.xyz/s/wbmgmtwv) lights both sides of each circle at the same time, warm on one side and cool on the other, which warms the new steps and cools the old ones. So this isn't a color for one side:
it's two sides, both drawn. A Möbius map is the same thing: the sign of `bottom` is the circle's handedness, and when it flips the disk's image is what was the outside.

That's built now, and `plan-sides.md` has the plan and what it found. A circle carries a handedness and can be a line, and both of its sides can be drawn. Two figures prove it:

- **`corpus/011-circumcircle` (`web/circumcircle.html`):** his shader's triangle and Lissajous curve, tinted by side. As the corners pass, the circle goes through a line and comes back inside out, and the plane doesn't flash. A test finds the moment by bisection and compares
  every pixel either side of it, and shows that styling the disk instead would swap them.
- **`corpus/012-limacon` (`web/limacon.html`):** 24 circles, each with both sides lit at once, adding up. A test holds it to the shader's formula, written out by hand, away from the rims, where the shader has a hard edge and this has an antialiased one.

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
  the name is the space's. So each thing is one declaration in the lattice's coordinates, each panel draws it `through` its
  map, and `text` says what the panel calls it. `zero` and `end_point` are dots in every panel, and the spiral's ring around 1 is
  `zero` itself, in a style for that panel. Not built yet: pointing at something in one panel and seeing it in the others, which
  this makes possible, since marks in every panel now share one declaration and one name.
- **What complex numbers cost, and what sharing bought back:** inlining copies arguments, so `coth` of something is about 400 nodes per coordinate, and Bipolar Hearts
  took 18 ms a frame. Now `fast.rs` works out each repeated piece once per sample, which took it to 2.4 ms at 1520×1292. The lattice, written as `T(w) = 2πi·w / A`, is
  now 7.0 ms a frame, 8.3 ms on a q or p change, and 10.4 ms at q = 12, p = 7, at 1580×1603. The old hand-written form was 7.6, 7.8 and 11.8 before sharing. On a change, the
  panels also stopped solving one system four times, and literal zeros from `i` are folded. `eval` is still the definition, held to the bit on every expression in the corpus.
- **What one declaration cost:** nothing visible. On the page, a frame takes about 7.5 ms, a q or p change 8.8 ms, and q = 12, p = 7
  11.6 ms, at 1480×1502. That's a smaller canvas than the earlier numbers, so it only shows no slowdown large enough to see. The flat
  panel now computes the same lattice points as log space and culls what's off its frame.
- **Log space's copies are there for the crossings:** Kris's point is to show the cylinder wrapping, so the crossings there
  connect to the same ones in the spiral, with 0 and 2πi becoming 1. How light they are is one rule in the style.
- **Not like the site:** the panels have no frames, since a panel's border is the page's business, and the title's bold isn't
  there, since a font names only a role for now.

## The second style sheet

Pencil's purpose is a document that can be styled separately, and none of the last four figures had a second sheet. So
`corpus/dark.style` and `corpus/007-lattice/narrow.style` restyle `LatticeMapping`: the site's dark palette, and one column
for a phone. Run against the file as it stood:

- **The dark sheet did nothing, with no error.** A sheet's `let` didn't reach the rules before it. Now the last definition
  holds everywhere, as a CSS variable does, and the dark theme is only its colors.
- **The narrow sheet couldn't be written.** Columns, panel sizes, `pad` and label distances were all in the document, along
  with a `·16` pixel constant. They're a sheet's now, and the document says only which way a label sits, with `away`, in the
  figure's own coordinates. The file lost its `on_screen` helper and its `start_side` parameter.
- **An unknown property blamed the wrong thing,** saying `distance` "needs a Color". It now says the property is unknown, whatever its value.
- **The page copied the palette.** `web/lattice.html` now takes its colors from the figure's sheets and picks its sheets from the
  reader's theme and width, with `?dark`, `?light`, `?narrow` and `?wide` to pin one.

Tests hold every mark in the IR to the same value under all the sheets, the same labels to exist, and every label to at least
4.5:1 on its paper in both themes.

- **What this didn't reach:** figures before the lattice still use `offset`, which a sheet can't move, and their pages keep
  their own palettes. A panel can't be hidden or moved by a sheet. Sheets are picked by the page, since the language has no
  media query.

## Narration timing

`lab/narration`, run on 2026-11-10 against the explainer (`limacon.txt`), first sections 1 to 3 and then all of it. The question was whether a synthesized
draft and a real recording can give the same timing data, so the words can be edited as text and recorded last.

- **They can, by aligning both the same way.** The Montreal Forced Aligner takes a take plus its script and gives every word and
  phoneme a time, and ARPAbet marks each vowel's stress. A script's anchors, such as `[mirror] mirror`, land a beat on the onset
  of that word's stressed vowel. Neither the voice nor the format matters, so a phone's m4a goes through the same steps as the draft.
- **Kokoro's own word times would nearly do:** 21 to 25 ms from the aligner's on average, and 65 ms at worst, a frame or four at
  60 fps. They're only compared, so a draft and a take never depend on different tools.
- **Recognition isn't a timing source.** Whisper's word starts were 68 ms off on average and 400 ms at worst, with no phonemes,
  and it heard "limousine" for limaçon and "cinch" for sinh. It's the lab's `hear` check instead, since the aligner fits every
  word somewhere even when a take says something else. That's how it caught Kokoro saying words twice: two spaces left where an
  anchor was removed confused Kokoro's markup.
- **One pronunciation for both:** Kokoro reads `sinh` as "sin" and `tanh` as "tan", and the aligner's dictionary lacks limaçon,
  conchoid, cardioid, sinh and tanh. `words.dict` gives them in ARPAbet, and the draft voice gets the same, converted. The
  dictionary already has `cosh` as rhyming with "gosh".
- **Retiming keeps each beat's length,** since that's how fast a move reads, and changes only the waits between beats. At the
  draft voice's pace, section 3 goes from 5.5 s to 11.7 s before `trace` starts. The waits grow from 0 to 0.9 s to between 1.2 and
  3.3 s. The silent cut was punchy, and words set a slower pace than the motion did.
- **The voice has to wait for the figure too.** In the first draft, "at E" ran straight into "That's where its circle" while E was
  still landing. So each line of a script is now a clip. Within a line the voice sets the time, and a line starts late when a beat
  it anchors couldn't arrive in time, or to hold a landing. The hold is the pause written after the beat in the `.pen`, so the
  figure's own sense of how long a landing needs keeps meaning that, and a 5 s sweep keeps the voice waiting until it's done.
- **What's said should be seen (Kris).** Any point a caption or the narration names is labeled in the figure. C was named only in
  a caption, and the draft voice's C sounded like E. A label that appears as the letter is said settles it, since the eye tells the
  ear which letter was meant. So C has a label now, in a beat of its own anchored on the spoken "C". E isn't said for now, since
  its only label comes later and reads z². The lab's `labels` check holds a figure and its script to this, and on the figure before
  the label it flags every line that names C.
- **An arrival lands on the word for it:** `flip` on "lands", not on the place it lands.
- **The whole draft runs 102.6 s,** against the silent cut's 65.6 s, with 26 anchored beats. Every beat lands on its word except
  `name_slow`, 0.07 s late: "hub, slow arm, fast arm" is said 0.6 s apart, and each beat is 0.6 s long.
- **A number before a letter is a coefficient,** as in "two t", so the `labels` check doesn't ask for a point called 2.
- **Not checked:** a take in Kris's voice, and whether a vowel's onset is where an arrival should land. The lab's docstring renders
  a draft to judge that by.

## The playground

`web/playground.html`, built on 2026-11-11 so Kris can react to the syntax while narration waits for a take. A figure's `.pen` and
each of its sheets are tabs in an editor beside the figure, compiled again 120 ms after typing stops. A menu loads any corpus figure
or the explainer, with the sheets its own page or video uses. Edits are kept in the browser per file, and Revert reads the file
from disk again. Nothing is written to disk. Play, a time scrubber, beat stepping, a slider for each param, and dragging handles
all set the same bindings the other pages do. All 16 entries compile and draw on WebGPU and on `?gl`, checked in the desktop
app's browser pane. The editor is a textarea over a highlighted copy of its text, with no library.

- **Errors now say which file they're in.** A sheet's error came back with the sheet's line and no file, and `pencil` printed it
  after the `.pen`'s path. `Error` has a `sheet` now, so the page marks the right tab, line and column, and the CLI names the
  sheet. A broken edit leaves the last figure that compiled on screen. An edit that compiles but can't draw goes back to the
  last sources that drew, but no edit tried has reached that path.
- **Style errors only know their rule's line.** Every one says column 1, so the page can only mark the whole line. The parser
  doesn't keep where each declaration is.
- **One error at a time.** The parser and compiler stop at the first. A language server would want them all.
- **Parse errors printed Rust's own token forms,** such as `found Punct("+")`. They now print the token as it was written:
  ``found `+` ``.
- **Compiling is cheap, and WebGL2's pipelines aren't.** On WebGPU a whole load takes 1 to 20 ms, the lattice the slowest. On
  `?gl` it takes 90 to 580 ms. For the explainer, compiling is 2 ms of a 48 ms load there, so the rest is making the pipelines.
  Typing on WebGL2 stalls that long whenever it pauses. Keeping a pipeline whose shader didn't change would help, if it matters.
- **Highlighting stops where a grammar would start.** It colors comments, strings, numbers with their units, and a line's first
  word when a space follows it, which picks out statements and `style`. Property names, selectors, built-ins and operators wait
  for the highlighting grammar.
- **Stepping seeks time when `beat` follows time,** and sets `beat` otherwise. It finds the moment a beat is reached by halving,
  since `time_for` samples and landed short of whole beats. At first only beats with lengths counted as following time, so
  stepping the bounds figure set `beat` and left time behind: Next said 3/10 at 3.2 s, and playing on kept them apart. The core
  now says which bindings time drives ("The element" below).
- **Not checked:** Kris's own use of it, a phone, other browsers, and the page with the pane showing. The pane was hidden for
  most of the checks, so frames were drawn by hand rather than by the animation loop.

**How the caret's expression reads** (2026-11-12): a strip under the editor writes the expression at the caret back out with
nothing implicit, so `1/2πr` reads `1/(2·π·r)` and `x1` reads `x_1`, as GeoGebra respaces what it parsed. It finds the expression
from the line's tokens, between the places one can start and end, and `web/reading.js` does it for the AST page too. Checked in
the desktop app's pane. The pane is short when narrow, where the editor gets half the height and the header and tabs take most
of it.

## The element

`packages/figure/src/element.js` (it was `web/pencil-figure.js` until 2026-11-15), built on 2026-11-11, first in the
proposed order. `<pencil-figure src="…pen" sheets="a.style b.style">`
owns the canvas, its labels, the GPU and the clock, so a page only says which figure and which sheets. The playground and
every figure page in `web/` draw through it, and their controls only set and read the element. Checked in headless Chrome on
the M1 Pro, on WebGPU and `?gl`.

- **The clock is a Web Animations `Animation`** on the element, `animate([], { duration, iterations: Infinity })`, so pausing,
  seeking and `playbackRate` are the browser's, and DevTools' Animations panel lists it as `pencil <file>`. The element reads it
  each frame while it runs, as `progress × duration`. While paused it keeps its own time, since a time at the very end of a loop
  would read back as 0. A figure with no `duration` loops on 10 s, as before.
- **The core says what time drives.** `clock::time_driven` lists the bindings whose values follow time, through the bindings
  they read and through a `solve`'s inputs. `clock::uses_time` says whether anything does, counting `time` read directly by a mark
  (the limaçon's rings read it in a `with`), a style (`bands.style`) or the paper. `Player.info()` carries both, as `timeDriven`
  and `usesTime`. Ring, wave and the lattice use no time, so the playground shows them no clock. The square uses it only through
  its beats.
- **Stepping and seeking follow the core's answer.** Setting `beat` seeks time when time drives it, and holds it otherwise.
  Seeking hands a held `beat` back to time. Play hands back only the params time drives, so a param with no time binding keeps
  the value a reader gave it, where before, play reset it to its default.
- **Nothing runs while nothing changes.** The loop asks for a frame only while the clock runs, after a change, or while a label
  spring is settling. Paused, a figure asked for no frames in 600 ms. The tangents asked for 14 as its labels settled, then
  none. Before, the playground asked for every frame and drew only when something had changed.
- **The GPU starts near the viewport and is let go on removal.** An `IntersectionObserver` with a 200px margin starts the
  player, and the clock pauses offscreen and runs again where it was. Removing the element destroys the device, loses or
  unconfigures the context, and gives it a new canvas, since a canvas keeps its context for life. A move, which disconnects
  and connects at once, keeps it all. Forty figures in turn on WebGL2 all started, with no context warnings.
- **The shape comes before the GPU.** Once the sources are fetched, `figure_size` sets the canvas's aspect ratio, so an
  offscreen figure already takes its height. A page built ahead of time can set `--pencil-aspect` itself.
- **`sheets`, not `style`,** since `style` is HTML's own attribute. `params="a: 0.5; beat: 2"` holds params as a reader would,
  and `hold`, `release`, `value`, `params`, `time`, `beat`, `step`, `play` and `pause` are the script side. A drag fires `input`
  with the param's name, which `Player.drag` now returns. `paused` starts it paused, and so does `prefers-reduced-motion`.
- **The labels were drawn at twice their distance** whenever the device pixel ratio changed after the page loaded. The label layer
  kept the ratio it was made with, and every page made it once. The element reads the ratio each frame, so zooming and moving
  a window between screens are covered too.
- **A hidden pane doesn't render,** so it runs no animation frames, no `IntersectionObserver` callbacks and no document timeline.
  The desktop app's pane was hidden, so these checks drove headless Chrome over the DevTools protocol instead, with a counter
  wrapped around `requestAnimationFrame`.
- **The pages moved onto it on 2026-11-12:** circle, circumcircle, hearts, lattice, limaçon, Möbius, proof, square, tangents
  and unroll, after bounds, the playground and the prompter. Their own loops, `fit`/`follow` and label code are gone, and
  `fit.js` keeps only `match`, which the element uses. What the pages share around the element (a play button that only shows
  when `info.usesTime`, a time slider, the status line, `?gl`, and reloading as files change) is `web/controls.js`, and is only
  calls on the element and its events. `strokes.html` and `text.html` stay as they are: the first renders offscreen to compare
  pixels against SVG, and the second shows images from `lab/text` with no player at all. Checked in the desktop app's pane on
  WebGPU and `?gl`, with no console errors. What moving them asked of the element, and of the figures:
  - **Sheets picked in script fetch once.** Hearts, Möbius and the lattice pick sheets by the reader's theme (and the lattice by
    width), so they set `sheets` from script. The element now fetches a microtask after `src` or `sheets` changes, so a page that
    sets them just after the element is parsed fetches once, not once with the markup's and once with its own.
  - **`colors`** gives the named colors the figure's sheets define, which the lattice page sets on its root so its legend and
    background match. Empty until the figure loads.
  - **Loop lengths come from the figures.** Each page had its own period. The circle had none, so it says `duration 2·π`
    (one breath), as the limaçon does, and loops without a jump. The proof loops on its beats' 9.5 s rather than the page's 10.
  - **What changed for a reader:** the tangents page gets a play button, since `d` follows time, and starts held at d = 3 as
    before. Playing Möbius after moving b, c or d carries on from the time that gives that value, as the element does
    everywhere, rather than from where it paused. Circumcircle and limaçon now start paused under `prefers-reduced-motion`.
  - **A param held before the figure loads** stays held when play is pressed before the load too, since the element doesn't
    know yet what time drives. Only a reader faster than the GPU's start could reach it. Not fixed.
- **Not built:** a `ScrollTimeline` or `ViewTimeline` as the clock, which would let scrolling scrub a figure. The element's
  `iterations: Infinity` doesn't suit a scroll range, which wants one pass.
- **Keys are the element's (2026-11-14).** Kris: accessibility matters, so what a reader can do with a pointer they can do
  without one, and a page doesn't have to write it.
  - **Each handle is a tab stop.** Something a reader can focus sits over each handle's mark, with a focus ring of two tones
    that shows on any paper. It's a slider to a screen reader: named for its mark, with the param's value and the range as it
    stands, read as `theta = 1.047`. The pointer passes through it to the canvas, and grabbing a handle with a pointer
    focuses it, so the keys carry on from there.
  - **Arrow keys move it the way they point,** by a hundredth of the figure's shorter side, or a tenth with Shift, through the
    handle's own inverse (`frame::nudge_value`), as a drag that far would. So on the bounds figure ← takes P counterclockwise
    at the top of its circle, and ↑ there does nothing, since P can't leave the circle. A param with steps goes to its next
    step that way: the move is doubled until the param changes. Home and End go to the ends of the param's range, and
    Page Up and Page Down move it a tenth of the range.
  - **A figure with beats is a tab stop itself,** unless the page gave it a `tabindex`. ← and → step its beats, and Home and
    End go to the first and last. What it came to is said for a screen reader, with the beat's caption: "Step 3 of 10: …".
  - **A figure with handles or beats is a `group`,** so the page's `aria-label` for it is read.
  - **Checked** with real key presses at 1633×919 and 375×812 on `004-bounds` (a handle and steps), `005-tangents` (a handle)
    and `006-square` (beats with lengths, one step a key). `web/bounds.html` and the playground no longer step in page script.
    **Not checked:** a screen reader, a touch screen, and a browser other than the app's.
  - **Not built:** keys for a figure that only plays (Space to pause), which the playground does in page script, and a nicer
    name for a handle than its mark's, such as its label's text.
- **Not checked:** a phone, other browsers, the site, which still sets itself up, the pages with the pane showing (it was
  hidden, so a screenshot drew each frame), and a draw that fails after compiling, which the playground handles from the element's `error` event but no edit has reached.

## The host

What a page and a build do with a figure. Designed on 2026-11-14 for moving Kris's site to Pencil. Kris chose the build's side
and the element's to be built first, with the poster's moment in the document and the build on his machine.

**Built 2026-11-14,** and shown by `web/built.html`, which loads only what `pencil build` wrote for `corpus/013-tube` under its
light and dark sheets:

- **`pencil build DOC.pen --style … [--card] --out DIR`** writes the compiled figure, a poster at the design size and at twice
  it, a 1200×630 card, and a `.json` with their names and what the page needs to know. The tube's figure is 13 KB.
- **`poster { time: 3.5, roll: 0.6 }`** in the document (`language.md`, Posters). `010-mobius` has one, for the moment its
  page set in script.
- **The element shows its children until its first frame,** and keeps them where it can't run. With a poster it fetches
  nothing until it's near the viewport: below the fold, the page fetched only the `.json`, then the WASM and the `.pfig` once
  scrolled to, and never a `.pen` or a sheet. Its box was 640×524 before and after the swap.
- **`<source media compiled>` children pick the figure,** and changing the theme swapped light for dark while it ran. A file
  from another build is an error that says to compile again, and the figure on screen stays.
- **A compiled figure carries the colors its sheets name,** so `figure.colors` works without sheets, and a page can have them
  from the `.json` before any script runs.
- **A param says where it stops, and a page can ask.** `param p = 2 in 0..q step 1` in `007-lattice` replaces the `p ≤ q` its
  page kept in script, which the site's controls never had. `info.params` gives each param's widest `min` and `max`, its `step`
  and what its range `follows`, and `figure.range(name)` gives the range this frame. The site's `<figure-controls>`, the
  playground and `web/lattice.html` set their sliders from those, and the core keeps the value in range whatever a page does.
- **Not checked:** a phone, a browser without WebGPU, and print. **Known:** the poster's labels are drawn by the CLI's fonts
  and the page's by the browser's, so text can shift a little when the figure takes over. The tube has one label and it
  wasn't noticeable.

**Built 2026-11-15: the package and the Astro component,** and `pencil-lang.dev` moved onto them.

- **One package, `@pencil-lang/figure`, in `packages/figure`,** with three ways in: `.` defines the element, `./host` is the
  WASM as a page calls it (`ready`, `Player`, `gather`, `inspect` and the rest), and `./astro` with `./Figure.astro` are the
  integration and the component. One package and not three, because they can't be versioned apart: a compiled figure is only
  read by the WASM built from the same source. The repo is a pnpm workspace of it and `site/`, and the pages in `web/` load its
  files as they are. Packed and installed into an Astro project outside the repo, it built a page with a figure.
- **`<Figure src sheets dark narrow alt>`** runs `pencil build` for each set of sheets a reader can get, in the order
  `<picture>` wants them (narrow and dark, dark, narrow, then the rest), and writes the element with a `<source>` for each
  and a `<picture>` of their posters. `build()` is the same call for a page that wants a figure's `info` or its card.
- **Nothing is made twice.** `pencil build` now lists every file that went into a figure, as `sources` in its `.json`, and
  a figure is built again only when one of them, or `pencil`, is newer. The site's 23 builds take 3.4 s from nothing and
  0.08 s when nothing changed. What a reader fetches is named for what's in it
  (`_pencil/tube-a5c1afca6244/tube.pfig`), so a copy they kept is never a stale one.
- **A build stops if `pencil` and the WASM are different builds.** The WASM exports the core's fingerprint, and the
  integration compares it with each figure's. This happened while building it: the release binary was older than the WASM.
- **The poster has the figure's exact shape.** Its pixels only round to it, which moved the page a pixel when the figure
  took over, so the image takes its `aspect-ratio` from the figure's size, under the same media queries when a narrow
  sheet reshapes it. On the site's demos page, all 14 figures have the same box before and after, at 1633×919 and at
  375×812. A build can now give a tall figure its width before it loads, which the site does.
- **`pencil build` drew a poster 11 pixels wide** for a figure whose sheet gives a width and no height, as `003-unroll`'s
  does, since it took the figure's shape for its size. Fixed: a lone figure's width is its sheet's.
- **`astro dev` hands the element the sources,** as designed, and the poster too. Saving a source reloads each figure
  where it is, through a new `reload()` on the element: on the circle, paused at 1.25 s, a touched sheet gave one `load`,
  the same page, and the same time.
- **A figure nobody operates is an image to a screen reader,** when the page names it: its role was only set for one with
  handles or beats.
- **On `pencil-lang.dev`:** the demos fetch 14 compiled figures and no `.pen` or sheet. The choice of looks swaps one
  compiled figure for another, and the script that picked sheets by theme and width is gone, since `<source media>` does
  it. The playground still loads the package's files as they are, and a build points it at the WASM the pages already
  ship, so there is one copy.
- **Sizes to know:** the site's `_pencil/` is 13 MB, mostly posters at twice the size and the lattice's four compiled
  figures at 970 KB each before compression. Whether a host compresses `.pfig` decides what a reader pays for one.
- **The element has tests of its own (2026-11-15),** Playwright's, in `packages/figure/tests`: 131 pass on headless
  Chromium and WebKit, each at 1633×919 and as an emulated phone, on WebGPU and again with WebGL2 forced. They press every
  key on a handle and on beats, drag with a mouse and with touch, and check a range that follows a param, reduced motion,
  sheets by theme, the poster's swap to the pixel, and a compiled figure fetching no source. Headless WebKit has WebGPU
  as it is, and Chromium has it on SwiftShader with flags. An emulated phone isn't Mobile Safari: `safaridriver` can drive
  Safari on a phone over USB, which is untried.
- **Not checked:** `astro dev` from an installed copy and not a linked one, a site with a `base`, and the blog itself.

What follows is the design as written, for what's left.

**What a reader gets today.** `<pencil-figure src sheets>` fetches the document, each sheet and each used file, then the 5.2 MB
of WASM, and starts the GPU when it's near the viewport. Until its first frame it's an empty box of the right shape, and it
stays one where there's no GPU, no WASM or no script. The site is the same today: each figure ships as an empty `<svg>`.

**What a build makes,** for a figure and each set of sheets a page can pick, such as light, dark and narrow:

- **A compiled figure,** one file. One request instead of the explainer's 12, and a mistake stops the build and not a reader.
  It isn't for speed: compiling the explainer in a browser takes 19 ms. **Built 2026-11-14:** `pencil compile DOC.pen --style …
  --out FILE` writes it, the other commands take it in place of a document, and `Player::load_compiled` reads it in a page.
  - **Only the build that wrote a file can read it.** Its header holds a fingerprint of the core's source, so there's no
    version to forget, and any change to the core makes old files an error that says to compile again. The element has to
    show that, and not retry.
  - **It's faster to read and no smaller.** Natively, reading takes a third to a twentieth of compiling: 0.4 ms against 1.8 ms
    for the explainer. Before compression it's bigger than its sources, up to 970 KB for the lattice against 9 KB, since every
    expression is written out as a tree, and derivatives and inlined functions multiply them. After brotli it's about the
    size of its sources: 5.0 KB against 5.3 KB for the explainer, and 5.2 KB against 2.9 KB for the lattice. Naming each
    head once, or sharing equal subtrees, would shrink it. Not done, while the IR is still moving.
  - **The WASM grew 1.6%.** A page that only loads compiled figures still carries the parser.
- **A poster,** a still at the design width and at twice it.
- **What the page needs to know:** the figure's shape, its duration, its params with their ranges and steps, its beats and
  captions, and the colors its sheets name. That's what `info` gives a script today, as a file, so a page can lay out its
  controls and captions and take its colors before any script runs.
- **A card for sharing,** 1200×630, the figure on its paper, and on request a clip, which `pencil video` already makes.

One command writes them, with a manifest, into a folder keyed by what went in, so nothing is made twice.

- **A poster needs a GPU.** The CPU oracle is the referee and not a renderer: a 1280-pixel still of the tube takes it 10 s,
  one frame of the explainer 108 s, and it doesn't draw panels at all, where the GPU takes 0.13 to 0.21 s for any of
  them. So the build runs where there's a GPU, which is Kris's machine, where the site is deployed from today. **His call:**
  if the site should ever build somewhere without one, the folder of outputs gets committed, or the build needs a software
  GPU, which is untried.
- **Which moment is the poster. His call, since it's syntax.** A figure knows its most telling state, and it's a binding of
  time and params like any other, so the document says: `poster { time: 3.5, roll: 0.6 }`. Without one: the end of the story
  for a figure whose beats have lengths, where everything has arrived, and time 0 otherwise. The alternative is a sheet's
  `style figure { poster: 3.5 }`, which lets a video's sheet and a page's differ, and can't set a param.

**The element.**

- **Its children are the poster.** `<pencil-figure compiled="tube.penc"><img src="tube.png" alt="…"></pencil-figure>` shows the
  image until the first frame is drawn, and keeps it where a figure can't run, or is printed. Nothing moves when it swaps,
  since both have the figure's shape.
- **`<source>` children pick the sheets, as in `<picture>`:** `<source media="(prefers-color-scheme: dark)" compiled="tube.dark.penc">`,
  or `sheets="…"` with a `src`. Four pages in `web/` pick sheets in script today, by theme and by width, and each wrote it again.
- **It waits for the reader.** The WASM is fetched when a figure is near the viewport, not when the page loads, and a figure
  starts paused under `prefers-reduced-motion`, which each page does for itself today.
- **It's a package** a site can import: the element, its labels, and the WASM.
- Controls and captions stay the page's, as they are.

**Astro.** A `<Figure src sheets dark narrow alt>` component, in this repo beside the element. At build it runs the command
above and writes the element with its sources and poster. In `astro dev` it hands the element the sources, so a figure
reloads as it's edited. A function gives a post its card, in place of the site's dev-only capture. The site's tiers and
captions wrap it, in the site.

**Order.** The compiled figure, the poster and what the page needs to know, and the element's children and `<source>` are
built, and so are the element as a package and the component, tried on `pencil-lang.dev`. Then the Doyle series, which goes first
(`migration.md`, Order): the component is tried there on a figure that's already done, `SquareZ` in the shader post or
`LatticeMapping` and `RollTube` in the period post.

## Distance to a curve

Fixed on 2026-11-11, after Kris drew the heart `(x² + y² − 1)³ − x²y³ = 0` with `bands.style` in the playground and found
`self.dist` a poor approximation. It was `f / |∇f|`, which is only right next to the curve.

- **Where it failed:** `f / |∇f|` goes infinite wherever the gradient vanishes. The heart's gradient is zero at five points
  inside the curve, which drew the bullseye and four swirls, and also at three points on the curve itself. Far out, f grows as
  ρ⁶, so the estimate ran about six times short and the bands came out wide. The breathing circle's moiré at its center was
  the same failure.
- **Now it's measured to the curve as traced,** with the field's sign. An implicit curve whose distance is read, by a fill or a
  stroke drawn as a field, is traced once, and the stroke uses the same trace. Circles, dots and regions keep their exact
  distances. Bands are now even offsets of the curve, folding where two parts of it are equally near.
- **The oracle looks at every segment,** and the GPU at those its 32-pixel tile lists: every segment within `d + 2h` of the tile's
  center, where `d` is the nearest one's distance from there and `h` half the tile's diagonal. No other segment can be nearest to
  any pixel of the tile. A test checks every pixel of a spiral, and `pencil check` agrees within 1/255 on the heart, Kris's three
  further curves (the devil's curve, the astroid and a bifolium) and every corpus figure with a field.
- **The astroid found a tracer bug:** its gradient is infinite at its cusps, and a Newton step from there gave NaN vertices. The
  oracle went black, while the GPU skipped them. A step that would leave the finite numbers now stops instead.
- **A wide stroke showed its end (Kris):** a traced curve stopped one 6-pixel cell past the frame, and a stroke ends square
  across its line, so a 40px stroke leaving at a slant was cut off inside the frame. A curve is now traced past the frame by its
  stroke's half width and a pixel, sampled along `self.t` for a taper, and a test holds an open curve's ends that far out. Strokes
  under about 10 device pixels still trace one cell past, as before.
- **Cost:** in WASM at 1812², with the tiles listed on the CPU, a frame went from 18–46 ms to 31–59 ms across the five curves.
  16-pixel tiles cost more on the CPU than they saved on the GPU.

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
  whether a figure is right: that's the geometry. What a lab here settles can go straight to its figures, and has.
- Pencil is open source under the MIT license, and it isn't being built as a product.
- Third-party embeds and the attribution footer are deferred.

## Notes on the open questions

The questions themselves are under "Open" above. Detail that didn't fit there:

- **WebGPU and text:** Windfoil's README describes a fragment shader with CPU-built bands, and it needs a WebGPU-capable GPU today.
  It takes quadratic Bézier contours and gives winding and coverage, not the distance `self.dist` needs.
- **MathJSON:** today the IR's expressions are about 99.5% MathJSON-style heads, and the rest are Pencil's (`Where`, `Exists`,
  `Solved`, `Snap`, `Timeline`, `Smoothstep`, `Mix`, `IfNonnegative`). Functions are inlined and points are split into scalars, so
  the lattice's IR is about 930 KB from a 150-line file, most of it the exact derivatives its `solve` needs.

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
  where a true distance would give even rings. This is the styling problem Kris predicted. Fixed on
  2026-11-11 ("Distance to a curve").

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
with one param `a` driven by `2·π·cycle(mod(time, 13))` or by a slider:

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
- **Dashes on a moving range ride with its start.** `wound` runs `a..2·π`, so its dashes move with
  P. They're continuous now, since a moment when a moving range's ends meet no longer counts as a
  loop, but a real string's marks would stay put as it unwinds. Anchoring dashes at the far end
  would need a declarative option, and it isn't built.
- **Not checked:** other browsers or devices, and a drag on a real phone. The slider resumes at the
  earliest matching time, so a value the unwind and rewind share always resumes on the unwind.
