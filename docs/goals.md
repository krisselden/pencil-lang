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

### Proposed order after that

Proposed by Claude on 2026-11-09, not yet agreed:

1. **A custom element**, `<pencil-figure src style>`, built from `web/fit.js`. It's how Pencil gets onto the site, it can take its
   aspect ratio from an Astro build so nothing shifts, and it replaces the setup each page in `web/` repeats.
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
- **Distance to an implicit curve** only counts the curve as traced, which is the view and one cell past it. Near the frame's
  edge, a part of the curve just out of view can be nearer than the distance says.
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

What a figure has pointed at but not needed yet. `language.md` lists what the specs have that isn't built.

- Statements like `zbi = zb · i` that drive an arrow and its label from one line.
- Pointing at a thing in one panel and seeing it in the others.
- Moving a point with the keyboard, which the blog's bounds figure has.
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
- **Stepping a timed figure seeks time.** It finds the moment a beat is done by halving, since `time_for` samples and landed
  short of whole beats. An untimed figure's step sets `beat`, as the bounds page does.
- **Not checked:** Kris's own use of it, a phone, other browsers, and the page with the pane showing. The pane was hidden for
  most of the checks, so frames were drawn by hand rather than by the animation loop.

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
