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
- **The playground's keys on real phones** ("The playground", Its keys on a touch screen). Kris has used the lab on his
  iPhone, unframed. Not yet tried: Android at all, and the site's framed page on a real phone, where the frame is held to
  what's visible. Not yet reported: the selection handles, Copy and Paste from a long press, and the **system** key, on
  either. Only a device can settle: a phone held sideways, the system keyboard over a small phone, a key pressed while
  Gboard composes, an iPad's hardware keyboard closing the keys, and a pinch while they're up.
- **A figure after its GPU is lost, on Kris's devices** ("The element", A lost GPU). Built and tested headless on
  2026-11-18. Still to see: Chrome on the Mac after sleep or a GPU switch, which is how Kris met it, and Safari on the
  iPhone after the phone has locked, which no test can lose a GPU in.

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

   **Decided with Kris on 2026-11-17, for the blog:** Cloudflare builds its preview on every push, where there's no
   `pencil` and no GPU, so the blog keeps its built figures in its repo. The integration's `kept` names the folder: a
   figure is built on a machine that has both and committed, and a build uses it while its sources hold what they did.
   The blog installs the package from npm, as any other site would. Not chosen: building the blog on GitHub as this
   site is, which makes a preview take minutes, and teaching `pencil` to draw on the CPU, which is the most work.
3. **`pencil-lang.dev`:** the docs, the playground and the corpus as demos. An Astro site in `site/`, so it's the first user of
   the integration, before the blog. **Deployed on 2026-11-17** (`site/README.md`, Deploying): a landing page, the language
   reference rendered from `docs/language.md`, the corpus and the explainer drawn live, and the playground in a frame.
   Since 2026-11-15 its figures come through the package's `<Figure>`: compiled at build, each with a poster, and no
   sources shipped. CI builds it, since a build needs `pencil` and a GPU, and Cloudflare publishes what CI built.
4. **The move itself is tracked in `docs/migration.md`:** the order, what's done, and each of the site's 31 figures with the
   corpus figure that covers it and what Pencil lacks for it. **The Doyle series goes first** (Kris, 2026-11-14), in its own
   order, and the shader post's three GLSL figures stay GLSL, since that post is about writing a shader. Keyboard use comes
   with the host, before the posts: handles a reader can tab to, and arrow keys. 8 figures are done (the shader
   post's `ScaledCopy`, `LogNearest` and `UnwrapMorph` on 2026-11-15, drawn on the site's demos page at 1633×919 and
   375×812, light and dark), 6
   are ports with what exists, 9 need one or two features, 5 need design and 3 stay. What most figures lack is around the figure
   and not in it: arrow keys (12 figures), dragging a point in two dimensions (5, built 2026-11-18: a handle that sets two
   params), Newton's steps as values (4) and panels of different sizes (4).

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
- **How should a `.pen` file name fonts?** `font: math | sans` are roles, and since 2026-11-16 both are fonts the package
  ships (Libertinus, `packages/figure/fonts`), the same in the command and on a page, with the math font behind each. A
  document or a sheet still can't choose a face, a weight or a bold, and `--font` is the command's only way to another.
- **Should a span take TeX too?** A label's mathematics is Pencil's own notation in `$…$` (`language.md`, Mathematics in
  labels). Kris is on the fence about TeX inside the span, which a LaTeX or manim user types from memory. Its braces collide
  with `{expr}`, and it would be a second notation.
- **Should a reader's step play the move?** Open since 2026-11-21, and Kris's to decide. A reader's step never plays a
  move, whatever the beat: the element seeks to the end of the next beat and pauses (in the square, six steps land at
  1.2, 2.2, 3.2, 4.1, 6.2 and 7.5 s). So a reader who steps through the square sees 15 stills, and no term travels,
  copies off `z`, or factors, though every one of those moves has a length.

  What's settled around it. The question began as how a state should arrive on a beat with no length, which had no
  "during" at all. Kris: "If a beat has no length and there is no duration maybe just make it 1 second by default?"
  Tried and kept the same day, for named beats (`decisions.md`): a named beat that leaves `for` out is a move of a
  second, or of its share of what a stated `duration` has left, so it has progress and a state `on` it plays. The
  square with every length taken off compiles as it is: 15 beats of a second and its 8.4 s of pauses, 23.4 s where its
  author's lengths made 18.8, with the algebra playing through each second exactly as if every beat said `for 1`
  (`tests/transitions.rs`). That gives every named beat a move to play. It doesn't play it on a step.

  Recommended, not built: the element's forward step goes to where the next beat starts (its end, less its length),
  plays, and pauses at its end, for a figure whose beats time drives, and a step back still jumps. `web/stepped.html`
  does that in page script on the square with no lengths: each press of Next plays one second and stops (0.4 to 1.4 s,
  then 1.7 to 2.7 s), and the same page jumps, as now, for comparison. It changes how every figure with moves steps,
  the explainer included, so it wants watching there, and its keys and what it says to a screen reader want their
  tests run. The other way is to leave the element seeking, and a stepping reader keeps seeing stills.

  Not done, and a bigger change: making the older steps moves too, so that the two kinds mix and bounds' ten steps
  become moves of their 3 s share. Measured on a copy of bounds rewritten that way: `beat` runs 0 to 10 where it ran 0
  to 9 and follows a timeline where it was a floor, 54 of its 55 marks change in the IR (each gets a `life`, or its
  `focus` shifts by a beat), and 116 of 121 frames at four a second render differently. Its sheet reads
  `reveal(self.age, now, later)`, which turns over at an age of a half, so every mark would go to its `later` look
  halfway through its own 3 s. Bounds' sheet would have to be rewritten on `self.life` and `self.focus` first. Steps
  are as they were, so bounds and its golden are too. A second way to give steps a clock, a second each when no
  `duration` is stated while they stay whole steps, would change no figure here or on the blog, since the only one with
  steps states its `duration`. It isn't built either.
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
- **Does a `_headers` file's `Access-Control-Allow-Origin` reach a cross-origin `.pen` embed?** Cloudflare Workers
  static assets take the file: the site sets how long a browser keeps each kind of file in one (`site/public/_headers`,
  2026-11-17). The header an embed from another origin needs hasn't been tried (`design-notes.md`, "Hosting").
- **Where is `SPEC/18`?** The July 9 chat mentions it, and the plan's list stops at 17.
- **A consistency pass on ranges (Kris, 2026-11-18).** He wants it done once more of the language is filled in, and
  raised these as questions while two-dimensional dragging was decided. Nothing here is answered yet:
  - A range sometimes means every value between its ends and sometimes only whole numbers, even with no `step`: a family
    seems to need whole numbers. Should they read the same?
  - Is a collection tied to `for … in` over whole numbers, or can it run over something else? A collection's variable can
    now count over a list, `for s in {-1, 1}` (`language.md`, Collections, 2026-11-18).
  - Can a parametric curve take a ±? Desmos needs a list for it, `[-1, 1]`, and does support it. Built on 2026-11-18 for
    collections: `±` draws both branches there, and is an error anywhere else. And as a whole range, `med ± dev` is the
    interval `(med - abs(dev))..(med + abs(dev))`, never reversed, in every place a range is written (Kris, 2026-11-18).
  - He found it confusing which space a range is in. How should a range say it?
  - Still open: zipping a range used as a value, Desmos's `([1...5], [1...5]²)`. The only lists an expression zips are
    `±` and `∓`.
  - **A candidate, discussed by Kris and the project session on 2026-11-18, not decided, and none of it built.** Kris's
    observation: `a..b` is programming notation. Mathematics writes an interval with brackets that say whether each end
    is included, `[a, b]`, `(a, b)`, `[a, b)`, and a count as `k = a, …, b` or `{a, …, b}`. Pencil's bare `a..b` is
    always inclusive and says neither which kind it is nor whether an end is included. The candidate: brackets say
    whether an end is included, `[a..b]`, `[a..b)`, `(a..b]`, `(a..b)`, for a continuum; braces say it's discrete,
    `{a..b}` for the whole numbers from a to b, beside the list `{-1, 1}` that exists; and `..` stays the separator
    inside, because `(a, b)` is already a point in Pencil. Questions with it:
    - What would a bare `a..b` mean then, or would it go?
    - Would `param n in {1..12}` replace `step 1`?
    - What is `med ± dev` in each bracket?
    - Where does an open end matter in practice: a curve's parameter approaching an asymptote, a param that must not
      reach 0?
    - Today's error for `{1..3}`, that a range goes outside the braces, is the opposite of this candidate, and would be
      reversed.

  What `a..b` means today, each cell checked by compiling or drawing a probe on 2026-11-18:

  | Place | Meaning | `step` | Ends can be |
  |---|---|---|---|
  | `param a = 1 in 0..2·π` | every real value; with `step`, the start plus whole steps, up to the last at or below the end | yes | numbers, constants such as π, and params, declared before or after it since statements are put in dependency order, in any expression: `0..2·q - 1`. Not a `let`, and not `time` |
  | a curve's or a region's parameter, `for t in a..b` | every real value, sampled smoothly; it runs from `a`, so down when `b` is below `a` | no | anything: params, `let`s, `time` |
  | a collection's variable, `for i in a..b` | whole numbers only, from the start rounded up to the end rounded down, both included (`0.5..2.7` is 1 and 2); none when `b` is below `a` | no | anything: params, `let`s, `time`, and earlier variables |
  | `viewport { x: -2..2 }` | every real value | no | constants only: numbers and constants such as π, not a `let`, a param or `time` |

  Every one of these can be written `med ± dev` instead, which is `(med - abs(dev))..(med + abs(dev))` with that place's rules for
  its ends. Other differences: only `param` takes `step`; `for` means "over a range" in a collection and "for this long" in a beat;
  and `0..q` on a param means it can't pass q, while on a collection it means count up to q rounded down, which the
  reference doesn't say.
- **The specs' own open questions** (SPEC/16 §8, SPEC/50 §7) are about relations, rigs and extensions, none of which a figure has
  needed yet.

### Known limits

Built, and wrong or short in a known way. None has blocked a figure.

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
- **Mathematics in labels** (2026-11-16; `language.md`, Mathematics in labels) has no matrices, aligned lines, limits above a
  sum, accents or bold. A tall root or bracket is the glyph stretched, not the font's larger variant, and scripts are the
  full-size glyphs scaled, not the font's `ssty` alternates: a page can't ask for a glyph that has no character, and both
  renderers have to draw the same thing. Text has no kerning in either renderer. `pencil svg` writes a label's mathematics
  as its plain words, `x^2/2`, not laid out. Checked in Chromium and WebKit on a Mac and by the command on Linux; not in
  Firefox or on Windows, where a page's line box could sit differently.
- **A trimmed circle starts due east,** since circles keep only a center and radius.
- **SVG export** (`pencil svg`, 2026-11-16; `language.md`, A figure as SVG) leaves out two things that do have a vector form:
  a dashed stroke whose width varies, and the side of a curve that leaves the frame, each with a line saying so. A label's
  baseline is set 0.355 em below its center, which suits the fonts a page uses, and isn't read from the font that placed it.
  No test rasterizes an SVG: it was compared with `pencil render` once, by hand, in Chromium.
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

- **Phones:** a touch drag, the sides benchmark (`lab/sides`), and label springs. The playground's undo on an iPhone:
  whether iOS's three-finger tap and swipe reach the page's history, and whether a three-finger swipe still scrolls
  ("Undo on a phone", 2026-11-20).
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
  overrides that binding, which is the same mechanism as time and the slider in `Unroll`. Setting `q` or the
  case hands θ back to its binding. The page did that in script at first, and since 2026-11-18 the figure does, for
  every page: a param goes back to its binding when the reader sets one its binding reads (`decisions.md`). The same
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
   - **States on named beats, 2026-11-21.** Kris's note: in the square nothing read `shorten` by name, and its job was
     to be the fifth beat, so that `work by beat` reached its fifth row. Making it a pause would have moved every later
     row onto the wrong beat. An expression's states can now each say the beat that brings them, `on factor: …`
     (`language.md`, Expressions that move; `decisions.md`), and a beat no state names is a hold that isn't written.
     The square's `work` went from 16 rows to 13 states with nothing else changed. Checked by number: every label of
     the figure, 22,400 of them over 4,561 frames at 240 a second, is in the same place at the same strength as with
     the rows, to the bit, and a test keeps that, writing the rows back out from the states (`tests/transitions.rs`).
     The driver is the sum of the named beats' progress, which the explainer's roulette already wrote by hand as
     `by said`, so the IR has nothing new, and only the square's golden changed: its `by`, and three states fewer.
     `by` a number stays, for a param a reader scrubs.
   - **A state writes the places it changes, 2026-11-21.** Kris, on seeing naming beats alone: the note's examples have
     one place and the square has six, so most of the repetition he wanted gone was inside the states. Its 16 rows wrote
     `"z" at at_z` eleven times, and whole states on named beats still wrote it eight. Now a state `on` a beat sets the
     places it names and leaves the rest, a term written at a new place leaves its old one, and `empty` clears them all
     (`language.md`; `decisions.md`). The square's `work` is 13 lines of one piece each, and `"z" at at_z` is written
     once. Checked as the first stage was: the sketch Kris approved, with `shorten` still a beat, compiles to the
     IR of that stage exactly, and lays out as main's 16 rows do at all 4,561 frames, every difference 0. No state of
     it differs from the rows, so neither the sketch nor the rule needed a correction. With the cleanup on top, the
     square's golden didn't change at all. A test works the states out a second time, on its own from the rule, and
     holds the figure to them. `by` a number still writes whole states.
     What it costs, as found. What `work` holds on `slide` is on no line of the source: it's `z` from `measure`,
     `z.1 a` from `scale_a` and what `slide` writes. `pencil ir` lists every state whole, and that's where to read it.
     And a term written at the wrong place, when that place exists, is no error: `on factor: … at at_z` in the square
     would put the factored sum in `z`'s place, with `z` gone, and leave a lone `z.2` at the old sum. Nothing can know which place was
     meant. A place that doesn't exist was already an error, and now says the nearest name. Two checks that would
     catch slips near this one, neither built: a state that changes nothing (a hold written by habit), and a place
     that's written and never cleared or written again before the end, which is usually right, as `z` is.
   - **The roulette on named beats, 2026-11-21.** `lab/explainer/limacon/roulette.pen` drove its two expressions
     `by said`, with `let said = name_hub + name_slow + name_fast + squaring + quiet`: the sum `on` works out, written by
     hand, with which beat goes with which row left to their order. A sum with a beat more or a beat fewer than the rows
     compiles without a word. Both expressions now say their beats, and `said` is gone, since nothing else read it.
     `defs` was five rows, three of them `empty` only to stay in step with `work`, and is two states: `on squaring` and
     `on quiet: empty`. `work` is five states as before: it's one line that grows, so each state still writes the
     whole line, and writing per place saves it nothing. Nothing drawn changed: 661 frames as SVG over the whole 65.6 s
     and 142 rendered from 39 to 58.5 s are the same bytes. In the IR, `said` is gone, each `by` is its sum, `defs` has
     two states, and one state of `work` lists its two places in the other order, the one already there first.
   - **After review, 2026-11-21: an expression above its beats.** A state's beat was looked up where the expression
     was written, so its beats were brought early, as for a value that reads a beat's name. A beat with a block can't
     come early, so an expression above one was an error, with no line, and a wrong name was only checked against the
     beats read so far, so `on mvoe` above `beat move` got no "did you mean". The beats are now looked up once all are
     read. An expression can be written anywhere, its terms drawn where it's written, and a wrong name is checked
     against every beat. The ordering errors that printed no line (a label or any other mark reading what can't come
     early) now say the line of the statement that needed it.
   - **After review: nothing written at a place.** `on b: "" at p` emptied `p`, by accident of how an empty place is
     dropped, while the reference said one place can't be cleared. It's an error now, so how to clear one place is
     still to be chosen and not settled by a side effect. No figure wrote it. The square's 16 rows as main wrote them
     are typed out in a test, compiled `by beat`, and the per-place states are held to them state by state: the check
     that doesn't go through the rule.
   - **After review: lengths past the `duration`.** With a second for a named beat that writes no length, `duration 1`, `beat a for 2`, `beat b`
     said the lengths "leave nothing" for `b`. They run past the duration by themselves, and now say that.
   - **A named beat with no length is kept, 2026-11-21.** Asked whether to keep the trial where a named beat with no
     length takes one second, Kris: "Keep it". So it's the rule (`decisions.md`; `language.md`, Beats), and what still
     said that only a beat with a length has a name is corrected: three errors about a state's beat, the one for a
     figure with no `duration` to read, and the one for mixing the two kinds, which now says a figure's beats are all
     moves or all steps and that a name makes a move. Each is pinned by a test. What's open is whether a reader's step
     plays the move (Questions).
   - **The square's `shorten` beat is a pause, 2026-11-21.** The cleanup the note proposed, as its own change, since
     it changes what a reader sees: `shorten` did nothing but shorten the equation, so the beat is gone, the 1.6 s
     after `name` is one pause, and `z = a + b i` becomes `z` on `measure`, as its angle and length are written. The
     terms `= a + b i` now leave from 5.7 to 6.2 s where they left from 5.0 to 5.5 s, so the long form is read for
     1.6 s and not 0.9 s. Nothing else moved: over the same 4,561 frames no stroke, region or arrow differs at all,
     and no other label by more than 1e-11 px. The figure is 19 s as before, with 15 beats where it had 16, so a reader
     stepping through has one stop fewer, and the fifth step shows the short form and the measurements together.
   - **A term is what's between spaces, shown, 2026-11-21.** Two figures in `lab/expressions`, `joined.pen` and
     `spaced.pen`, with the same two states, written `2ab` and `2 a b`, where the second state writes `b` at another
     place. Spaced, `2` and `a` don't move at all and `b` travels solid. Joined, `2ab` fades where it is while `2a` and
     `b` fade in where they end, their strengths adding to 1, and nothing travels. A test holds both to that, and
     `web/terms.html` plays them on one clock. They're in the lab and not the corpus, since no figure of the site's
     needs them: they show a rule of the language, which `language.md` now says next to it.
   - **A beat's progress, eased once, 2026-11-21.** In the explainer's limaçon (`lab/explainer/limacon/circles.pen`;
     the corpus's `012-limacon` has no beats) the angle C is at and the count of copies it leaves each wrote
     `smoothstep(0, 1, sweep)`. They agreed, and only because the two were typed alike. `let swept` says it once and
     both read it. Nothing drawn changed: 661 frames as SVG over the whole 65.6 s, and 81 rendered through the sweep,
     are the same bytes before and after. A test now holds what the name is for: at every sample of the sweep the
     count of copies is how many 24ths of the turn C has come, so each appears as the moving circle reaches it.
   - **Copies drawn early, fixed the same day.** On a whole step an expression was drawn as the start of its next move:
     the state it holds, plus the next state's copies at full strength on what they copy. So `z` was drawn twice, and
     darker, through the two pauses before `scale_a` and `scale_b` (7.5 to 7.8 s and 8.9 to 9.2 s), and nowhere else:
     of 381 frames written as SVG by the command before and after, those 14 differ, each by one doubled `z`. A whole
     step now draws the state it holds alone. This had to go first, since with states on named beats the square holds
     on a whole step through `measure` and `unmeasure` too.
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
for a phone. (Since 2026-11-18 the narrow sheet keeps the panels two by two, smaller, as the blog keeps its facets on a
phone: one column was 1269 px tall on the demos page at 375×812, and the grid is 367.) Run against the file as it stood:

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

## What's drawn with nothing said

On 2026-11-19 Kris found that a figure showed nothing until a sheet styled it: `corpus/005-tangents/tangents.pen` with an
empty sheet was its five letters on white, and his hearts labels under the glow look came out as raw `z_b = 1.48e^(1.12i)` in
sans. "I think it is too confusing not to have default styles." So a mark that would paint nothing now gets its type's
default paint in the default ink, `stroke: none` and `fill: none` hide a paint, and a value's mathematics is laid out
whatever the label's font (`language.md`, "What's drawn with nothing said"; decisions of 2026-11-19).

- **A census first, from the compiler.** Every look of every figure, the explainer's, and the sheets the element's test pages
  load were resolved and each mark's paint read back. Two corpus marks painted nothing, both in the hearts' glow look: the
  ties to the foci, which the plain look draws dashed. The glow sheet says `stroke: none` for them now. Four test pages
  (leader, panels, plane, keyed) had 11 unstyled marks, which now draw. With no sheet at all, the 17 documents' 231 marks
  besides text were all invisible.
- **Held by tests.** `crates/pencil-core/tests/defaults.rs` resolves every look twice, once behind a sheet that marks what no
  rule writes, and fails if any corpus mark gets a paint its sheets didn't write, or text whose ink isn't the black it was.
  It also builds every corpus document with no sheet and requires every mark to paint.
- **Reviewed.** An independent review rendered every corpus look at three times, and the square and the scaled copy frame by
  frame, against the commit before: pixel-identical. It found the guard blind to `curves`, which no type selector reached;
  `curves` is a type now, the guard checks the light and dark variants, the highlights and captions, and a test shows it
  flags one unstyled mark of every type the corpus has. Marks that `show: 0` keeps out are left out of it.
- **A paper that turns over.** The CPU decides the ink in single precision, in the shader's order, but the shader's time is
  single precision too, a step of 2.4e-7 s at t = 3.28. So when a paper's luminance passes the line, the two can be a frame
  apart for about 1e-7 s either side (measured on `mix(white, black, time/4)`, 5 samples 2e-8 s apart fail `pencil check`).
- **Labels settle by default.** A value's label on its own dot, `label z at z`, had the dot drawn over it once dots drew.
  `settle` as the default moved the labels of unroll's two looks and bounds' blog look by up to 22 px, so those sheets say
  `place: fixed` now, as Kris suggested, and render as before. Bounds under the theme as well as its own sheet, which only
  the element's test pages load, settled by the theme's rule and now stays fixed, since its own sheet comes after the
  theme: one of the two had to change, and the site's look is the one kept. A label on its own point starts up and to the
  right, since a bump has no direction at its center.
- **Left to look at:** a `fill` on a mark that never draws one (a segment, an arrow, a parametric curve, an outline) is
  accepted and does nothing, and a mark with only that gets its default stroke. It says nothing about it yet.

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

**`retime` stops on a length it can't read, 2026-11-21.** `retime` finds the story by pattern: a pause with a number, or a
beat that says `for` and a number. A named beat can now leave `for` out, and such a beat matched nothing, so it was
invisible: anchored, it was "no beat named …", and unanchored, every beat after it landed a second early with nothing
said. A length that's an expression, `for duration / 3` or `pause 1/2`, had the same hole before today, and `pause 1/2`
was read as a pause of 1. Any beat or pause whose length isn't a plain number on its line now stops the run, before a
take or any audio is read, with the file and line it's on and what to write. The script doesn't learn the compiler's
rule, a second or a share of the `duration`: that arithmetic would then be kept twice. The older `beat "caption" { … }`
step is left alone, and a beat with a length and no name, which the pattern also missed, is now counted. No beat in the
explainer is written any of these ways, and retiming it from the draft take writes the same `.pen`, audio and prompt
timings as before, to the byte.

## The playground

`web/playground.html`, built on 2026-11-11 so Kris can react to the syntax while narration waits for a take. A figure's `.pen` and
each of its sheets are tabs in an editor beside the figure, compiled again 120 ms after typing stops. A menu loads any corpus figure
or the explainer, with the sheets its own page or video uses. Edits are kept in the browser per file, and Revert reads the file
from disk again. Nothing is written to disk. Play, a time scrubber, beat stepping, a slider for each param, and dragging handles
all set the same bindings the other pages do. All 16 entries compile and draw on WebGPU and on `?gl`, checked in the desktop
app's browser pane. The editor is a textarea over a highlighted copy of its text, with no library.

**Reworked on 2026-11-15, for a reader and not only for Kris.** His words: education is a big motivator, and the playground
"doesn't let you load and save, or add/remove pen or style files", and its one menu was confusing, since a figure and its style
are separate and an edit to the circle stayed when the menu changed. A built figure ships no sources, so the playground is also
where a reader of a page goes to see how a figure is written.

- **Two menus:** Figure lists documents and Style that document's sheets, so changing the style keeps the document and what
  was typed in it. `?figure=<id>` takes the ids it did.
- **A figure in a link.** What follows `#pen=` is a whole figure: its document, its sheets and every file they use, as JSON,
  deflated, in base64 (`packages/figure/src/pack.js`, `@pencil-lang/figure/pack`). Nothing after a `#` is sent to a server.
  Share writes one for what's in the editor, and the address drops it at the next edit, since it no longer says what's open.
  A link that can't be read says so where errors show.
- **A figure of your own,** one at a time, kept in the browser: what a link or opened files gave, or an example whose files
  were added to or removed. The example is left as it was. Replacing one with changes that are nowhere else asks first.
- **Open, Save, add and remove.** Open takes `.pen` and `.style` files, picked or dropped. Save downloads the file in the
  open tab. A sheet or a file to `use` is added from the end of the tabs and removed from its own tab. Tabs are buttons with
  arrow keys, and each dialog keeps the focus and gives it back.
- **A built figure links in.** With `playground` set on the Astro integration, `build()` gives each figure a link that
  carries its sources: 3.1 KB for the tube, and 0.4 to 8 KB across the corpus. Built in a project outside the repo and
  opened in the playground alone and framed by the site, in Chromium and WebKit: it opens as "Yours: tube" with its three
  files, an edit clears the link from the site's address, and Share puts it back.
- **Left as they were:** an edit to a sheet examples share, as `theme.style`, still carries to every example that loads it,
  with the dot on its tab to say so. Save is a file at a time, and Share is how a whole figure is kept.
- **Not built:** a link that names a URL to fetch the sources from, which keeps a page small and needs the page's host to
  allow the fetch (Kris: a Cloudflare Worker can send that header). **Not checked:** a real clipboard, a real drag and drop,
  and a screen reader. 197 Playwright tests pass, on the source files and on the minified bundle.

**Its bars on a phone, 2026-11-15.** Kris, of a portrait window: "Almost half the height when portrait is menus and
navigation", and of Desmos on his phone: "It is mostly the viewport and expression list. One small bar at top." At 375×812
on the site the editor started 390 px down, under 13 rows: the site's header on two, a note, the playground's header on
four, the tabs on four and the file's buttons, and the figure was a scroll below it. Now the site's header and one bar
take 75 px, and the figure and the editor have the rest, both on screen.

- **Tabs are one line** at every width. The line scrolls when there are more than fit and fades at an end that has more
  past it. A file that comes into view brings its tab, and typing leaves the line where the reader put it. The button that
  adds a file stays beside the line.
- **A narrow window has one bar:** the Figure and Style menus and a button. Open, Share and the help are behind that
  button, and what's done to the file in view (Copy, Save, Revert, Revert all, under its path) is behind one at the end of
  the tabs. Each is a popover that a wide window shows in place, so there's one of each button. A menu opens under its
  button with focus on its first choice, arrows move through it, a choice or Esc closes it, and focus goes back to the
  button. At 480 px and under the two menus drop the words Figure and Style, which a screen reader still gets.
- **What a button did** is said for a few seconds over the bottom corner at every width, since the row it was in is now
  inside a menu on a phone.
- **The site's page adds one line.** Its header keeps to one on a phone, and the note under it is gone: framed, the
  playground offers "Open on its own" itself, at the address it has.
- **The figure is over the editor, and the page doesn't scroll.** The figure's part takes the height it needs, to 60% of
  the window, with the figure itself kept to 38% of the window's height, and the editor has the rest, 9rem at least. A
  part that needs more scrolls within itself. The editor is moved in the page to follow the figure, so a screen reader and
  Tab meet them in the order they're seen, and it keeps its caret and focus when a window crosses 800 px. The figure isn't
  moved, so it doesn't start again. Measured for all 18 examples at 375×812, 375×667 and 412×839: the page never scrolls,
  the editor's text has 172 to 530 px, and the figure's part scrolls within itself only for `bounds` and `mobius`, which
  have the most controls. The line of what a figure was drawn with keeps to one line there, which scrolls.
- **Not handled then:** a phone's keyboard, which covered the editor's half. Handled since by the Pencil keys (below).
  **Seen:** `007-lattice` is 304 px wide at 375×812 where it was 359, and its panel titles are cut
  at the top there. Its labels are an em of page text whatever the figure's size. Whether they were cut at 359 wasn't
  checked. Found since: the band above a panel for its title is in design pixels and shrinks with the figure, while the
  title is an em, so a figure drawn at less than half its size cuts its titles. The narrow sheet now draws the lattice
  at about its own size on a phone, with smaller titles, and the demos page uses it; the playground loads the lattice
  without it.

**Its keys on a touch screen, 2026-11-18.** Kris tried three ways of typing on his iPhone in `lab/keyboard`, through a
tunnel: the system keyboard with a row of symbols over it, keys of Pencil's own, and those keys with a row of words that
finish what's typed. His verdict: the lab fixed what made editing on the phone hard, the Pencil keys are much better than
the system keyboard (he doesn't see why he'd use that one), and the words are good. He found one defect, that the row of
words changed height when it emptied and moved the editor's text, which was fixed in the lab first. The playground now has
the third way:

- **On a touch screen the Pencil keys come up when the editor is touched,** as Desmos's do, and the system keyboard
  stays down (`inputmode="none"`). They go when focus goes elsewhere, and not while a finger is still down, so a drag
  on a handle doesn't move the figure under it. ▾ closes them for the visit, and so did the first key typed on a
  hardware keyboard, which an iPad or a phone may have, until 2026-11-20 (below: it drops them to the words and the
  symbols now); **keys**, by the tabs, brings them back. Their pages are numbers and operators,
  letters, Pencil's words and functions, and Greek with a label's symbols. A held ⌫ or caret key repeats. The row of
  words finishes what's at the caret by where it is: statements and the figure's names in a `.pen`, and in a `.style`
  the statements a sheet has, then selectors, properties and values in its rules. **system** hands the editor to the
  system keyboard, with a row of symbols over it and a key back. The choice is kept for the visit.
- **One copy of the code.** `web/keys.js` is what the keys type, `web/keyboard.js` the keys themselves, `web/keyboard.css`
  their look, and `web/viewport.js` the hold described next. The lab loads the same files, so it stays a bench for what
  the playground has. The site copies all four with the playground.
- **The page holds still.** On a touch screen the playground is what's visible of the page, which a keyboard makes
  smaller (Safari shrinks only the visual viewport, and scrolls the page to the caret even when nothing can scroll), and
  only the editor, the tabs and the keys' rows scroll. The site's page holds itself and the frame the same way, since
  inside a frame what's visible is the frame. The figure is as big as the room left for it, centred in it. While the keys
  are up on a phone, the editor shows five lines at least and the figure has its part to itself, with any error under it.
  Without that the explainer's figure was 40 px tall at 393×659. (Its controls have their own panel now: below.)
  The words row, the four pages and the line under the editor each keep one height, so nothing above them moves. A
  pinch zoom lets go of the hold until the page is back to its size, so a finger can pan it. A tablet that gains a
  mouse turns the keys and the hold off, and on again when it loses it. (Since 2026-11-20 a mouse has the words and
  the symbols: below.)
- **A short screen,** under 500 px of what's visible: a phone held sideways, or one with the system keyboard up, which
  no media query can see, since Safari's keyboard leaves the layout viewport as it was. With the keys up, the editor
  gives up its own room first: no line under it, no row of the file's tools, and no lines it must show. The keys are
  30 px tall, and the bars are the phone's one small bar with the two columns kept. Measured in Playwright: held
  sideways, every key is in view, and the editor shows 3 lines at 852×393 and 4 at 839×412. With 260 px visible over
  the system keyboard, the row with the way back to the Pencil keys is in view, and the editor has one line and the
  figure 33 px.
- **The editor's text is 16px on a touch screen,** below which Safari zooms the page when a field takes focus, and so is
  the name in Add a file. It was 13px.
- **The keyboard reaches what a finger does.** Keys are buttons with names. In the editor, Esc goes to the keys while
  they're up, the arrows move among them, Enter presses one and focus stays on it, and Esc again leaves for the figure. A
  hardware keyboard types into the editor as before, and pressing a key never takes focus from it.
- **Found while moving it:** showing the keys as the editor was pressed moved it before the press placed the caret, so in
  Chrome the editor never got focus; they come up on focus now. Chrome on Android focuses a tapped button even when its
  press was cancelled, so focus that a tap brings to a key goes back to the editor. A tap that takes focus away has still
  to click, so the keys go after its click, and a tap on an error still reaches it.
- **Found in review:** a held key now stops when the finger slides off it as well as when it lifts, and the click a
  long hold ends with doesn't type once more. A key ends a word the system keyboard is composing before it types, as
  Gboard composes every word. ‹, › and ⌫ go a character at a time as a reader sees one: an emoji, a flag, or a letter
  with its accent. The menu's hint says what Esc does on a touch screen.
- **With a mouse or a trackpad, nothing changed,** then. Every box of the page matched what it was, in Chromium and
  WebKit, at 1633×919 and 700 px wide. Since 2026-11-20 a mouse has the words and the symbols under the editor (below).
- **Checked** in Playwright on the iPhone 15 and Pixel 7 projects, and in the desktop app's pane at 375×812 and 1633×919,
  alone and framed by the built site: the keys type and the figure follows, a suggestion inserts, the rows keep their
  heights and the editor its box and scroll, the page and the frame don't scroll with the keys up or down, the system key
  switches and back, an error stays in view above the keys, Share and the menus work with the keys up, and with a mouse
  there are no keys. Then each item of the review: keys in view sideways, the way back in a 260 px view, a real touch
  drag on a handle (Chromium only, through CDP), a hardware key and ▾ remembered, a mouse paired and unpaired, a pinch,
  a held key, a composition, characters of more than one code unit, and the keyboard path into the keys and out.

**The text or the controls, 2026-11-19.** Kris, on his phone: the controls and the editor should take turns, "not try to
show at the same time", and the controls didn't scroll, so a figure with many params left the editor little room.

- **One panel under the figure, picked in the bar.** Where the bars are compact (a window 800 px wide or less, and a
  touch screen under 500 px tall, as a phone held sideways), the bar has a switch, Text and Controls, beside the Figure
  and Style menus. The figure's time, beats and params are then a panel of their own in the editor's place, and the
  switch shows one or the other. Held sideways, the figure is on the right and the panel on the left. A wide window with
  a mouse has no switch and the controls under the figure, as before.
- **The controls scroll in their panel,** and nothing else moves: the figure, the bar and the page stay where they are.
  Tried with twenty params, more than any figure in the corpus has (Möbius has five).
- **Controls puts the keys away,** Pencil's and the system's. Text brings the editor back with its caret and scroll
  where they were for the file it shows, and no keys until it's touched.
- **Only the reader takes the text away:** by the switch, or by opening another figure, which opens on the panel last
  chosen. A recompile, a resize or a rotation never does, and touching the text makes it the choice. A review found
  the text taken from under a reader typing a param into a figure that had none, as the recompile showed the controls
  remembered from before; and on a wide touch screen a keyboard that made the window short would have hidden the text
  it was typing into. Going from wide to narrow shows whichever has focus. A control keeps its focus as a resize
  moves it between its panel and the figure, and a recompile keeps the sliders it had when its params are the same,
  so one being dragged isn't replaced.
- **A figure with nothing to control shows no switch,** rather than a Controls that leads to an empty panel or one
  that's disabled, since a choice of one isn't a choice. Its place in the bar stays, empty, so the menus beside it don't
  move as figures change or while a figure loads, which they did when it was taken out. Ring, wave and the lattice are
  such figures. The choice is kept for the visit, across figures and a reload, and one with nothing to control shows
  the text without forgetting it.
- **Errors and what it was drawn with stay under the figure,** in view with either panel, so an edit that failed is
  seen while a slider is dragged. Going to an error shows the text.
- **The switch is a tablist:** two tabs, one tab stop, the arrows, Home and End move and show, and each panel is named
  by its tab.
- **The figure is larger,** since its controls aren't under it any more: on a touch screen at 375×812 the Möbius figure
  is drawn at 189×264 with either panel, where it was 74×104, and at 812×375 at 169×236, where it was 52×72. At
  1633×919 with a mouse it's 499×699 and every box of the page is where it was.

**Undo on a phone, and the words with a mouse, 2026-11-20.** Two things Kris met using the playground. On his iPhone
there was no way to undo: Revert only goes back to the file on disk, and iOS's own gestures are no answer (a
three-finger swipe undoes and scrolls too, a three-finger tap took the wrong file, and few readers know either). And
on the desktop he missed the keys: "the word selection would be helpful for people learning/experimenting, and having
the math symbols easily reachable is nice."

- **Undo and Redo are keys** at the right end of the words row, each a drawn arrow (an arrowhead and a shaft that turns
  back, mirrored for redo) rather than a character, which drew as a thin hook in some fonts, outside the part that scrolls. That row is
  up with the Pencil keys and over the system keyboard, so they're one tap away in both, and with a mouse too. They
  act as they're pressed, once (no repeat), keep focus in the editor, are buttons named Undo and Redo in the row's
  arrow keys, and say when there's nothing to do with `aria-disabled`, so focus stays on one through its last step.
  Measured at 375 px: the words have 283 px of the row, where they had the whole of it, and the row scrolls as before.
  The symbol strip has ▾ at its end as well, for the hardware keyboard below.
- **What was found: the browser's undo crosses files.** One textarea shows each file, and its text is set from script
  when the file changes. The browser keeps one stack of edits for the page, and the old file's stay on it. On his
  iPhone a three-finger tap on the sheet's tab undid an edit in the `.pen` in the sheet, and a redo put
  `label d label d at d`, typed in the `.pen` and undone, on line 7 of `circle.style`, which stopped compiling.
  Playwright's WebKit and Chromium don't do that: there a stale step does nothing, though it's still counted, so an
  Undo is offered with nothing to undo. Both do apply a step to another file when the two texts are the same, since
  setting a value to what it is changes nothing (seen with a redo). WebKit also groups every insertion from script
  with the typing before it, so a Revert by select-all and insert was undone together with the last typing, and lost
  it. So the fix couldn't be in the keys alone.
- **Each file has its own history, kept by the page** (`web/history.js`): what a reader expects, an undo on the
  sheet's tab undoes the sheet. Every way in goes there: the keys; Cmd+Z and Shift+Cmd+Z on a Mac, Ctrl elsewhere,
  and Ctrl+Y off a Mac, caught as keys since the browser has nothing to redo once its own undo was stopped; and the
  system's own undo (the Edit menu, iOS's gestures), a `beforeinput` of `historyUndo`, which is stopped when it can
  be. An undo the browser makes anyway is put back and done from the file's history. The page's own rule groups the
  steps, not the browser's: a run of typed characters, or of deletes, each where the last left the caret and within
  two seconds of it, is one step; so is one composition from an input method, or a run of them each where the last
  ended; a text dragged within the editor is one; a word from the row, a symbol written for its name, a paste and a
  LaTeX paste are each a step of their own. The browser's own stack still records every edit and is never applied.
  It's still what offers the system's undo, which it does only while it has something, so on a page just opened the
  gestures may offer nothing. **Redo is the Redo key or the shortcut.** Once the page stops the browser's undo, the
  browser has nothing to redo, so the Edit menu's Redo and iOS's redo gesture aren't offered after a shortcut or a
  key; a `historyRedo` the browser does send is handled the same way, as a guard. Enabled and disabled come from the
  page's history, so they're right in both engines; the browser's `queryCommandEnabled` wasn't, for the stale steps
  above.
- **Switching files, opening another figure or a link (loaded, or opened in the page by its `#pen=`), the recompile,
  and the text or the controls** leave each history as it was: a file shown again has its own undo and redo, another
  figure's files start with none, and neither the compile nor the panel touches the text. Adding a file to an example
  or removing one copies every file into your figure, and each copy takes its file's history; a sheet the document had
  in a look before, with the same text, has its history again when that look comes back.
- **No undo while a modal dialog is open or the editor is hidden.** The browser sends its undo to the textarea's last
  edit wherever focus is: in Chromium, Cmd+Z in the Add a file dialog's empty field undid the file behind it. Now
  nothing is undone then, nor with the controls shown in the text's place. With focus on the figure and the editor
  shown, Cmd+Z is still the editor's undo, as it was before any of this (checked on main: it undid the text from the
  figure in both engines, and from the dialog in WebKit; from a slider with the controls shown it didn't).
- **After an undo or a redo, by any way, the caret is brought into view,** and the scroll stays where it is when the
  caret already is.
- **Revert can be undone.** The file shown takes the file from disk as a step of its history. Revert all does the same
  for every file, each as it's shown next; a file that was never shown in this visit has no history to go back to.
- **With a mouse the editor has the words row and the symbol strip under it,** the two rows a phone has over the system
  keyboard, there while the editor is and not only while it has focus, so its box doesn't change as focus comes and
  goes. The hardware keyboard is the keyboard, and typing on it closes nothing. The words follow the caret, and a click
  on a word or a symbol types it where the caret was and leaves focus in the editor. **all keys**, at the start of the
  strip, opens the pages as a palette (numbers, letters, the f(x) words page with every function and Pencil's words,
  and Greek), which is where the whole Greek set is; **symbol row** goes back. "system" and "Pencil keys" said nothing
  with a mouse, so with one the two keys are named for what they show. Which one is up is kept for the visit. ▾ puts it
  all away for the visit and **keys** by the tabs brings it back, as on touch. `inputmode` is never set with a mouse,
  and the page's hold stays a touch screen's. The palette's keys are 30 px tall with a mouse.
- **A hardware keyboard on a touch screen** (an iPad) no longer closes the keys. The first key typed drops them to the
  words and the symbols, for the same reason, and that's kept for the visit as the choice of the system keyboard is:
  a keyboard taken away later leaves the system's keyboard, with the strip's key back to the Pencil keys. Only ▾ puts it
  all away.
- **The keyboard reaches the rows with a mouse as on touch:** in the editor Esc goes to them, the arrows move, Enter
  presses and focus stays (on the key that goes back, after all keys or symbol row), and Esc again leaves for the
  figure. Tab in the editor is as it was. The ☰ menu's hint says this for each.
- **The figure is where it was.** The rows are in the editor's column, and their height comes from its text. Drawn, by
  the status line, before and after: at 1633×919, the circle 699×699, Möbius 499×699 and the explainer 793×446, with
  the palette open too; at 700×919, 349×349, 249×349 and 621×349; at 375×812 on the iPhone 15, 264×264, 189×264 and
  359×202. Each row keeps one height with words or none. At 700 px they belong to the Text panel and go with it when
  Controls is shown.
- **A move of three fingers is cancelled while the page holds,** since a three-finger swipe scrolled the editor or
  the page as it undid. A move of two isn't, so a pinch is left to the browser as before; a test shows that much, not
  that iOS lets a page stop its own gesture.
- **Found in review, the same day,** and fixed with a test each that failed before: adding or removing a file, or
  another look, dropped every file's history, since histories were kept by the file's object and those are copied; a
  `historyUndo` the browser can't stop, followed by its own undo, took two steps; a `beforeinput` with no edit after it
  (a Backspace at the start, in Chromium) left its selection for the next edit, whose undo put the caret at the top of
  the file; two compositions in different places within two seconds were one step; Cmd+Z in the dialog undid the file
  behind it; Ctrl+Y was taken on a Mac, where it's Cocoa's yank; a drag within the editor was two steps; only the Undo
  key brought the caret into view. The strip's keys had lost their padding to a later rule, so `style` and `curve` ran
  edge to edge, and a few words alone in the row stretched across it.
- **Checked** in Playwright, on Chromium and WebKit with a mouse at 1633×919 and 700 px, and on the iPhone 15 and Pixel
  7: each kind of edit undone and redone; an undo and a redo by the key, the shortcut, a `beforeinput` made by hand as
  the system's undo sends one, and the browser's own `execCommand`, after a switch with and without an edit, never
  reaching another file (redo too); an undo the browser makes anyway with another file's text; Revert and Revert all
  undone; another figure, a link loaded and one opened in the page, the recompile and the Text and Controls swap; and
  each item of the review. A full run after the review: 516 passed, 136 skipped (each only where its project has no
  such thing, a phone or a mouse), none failed, on the source files and on the minified bundle alike. Looked
  at in screenshots at 1633×919 and 375×812, and framed by the built site (served as its Worker serves it) with a mouse in
  Chromium and WebKit: the rows there, a symbol typed and undone, focus kept, no `inputmode`, and the page not scrolled. The keyboard lab
  loads the same files and has the same keys.
- **Only a device can say:** whether iOS Safari's three-finger tap and swipe send a `beforeinput` the page can stop,
  so they undo the file shown (Playwright's WebKit is the desktop's, which didn't reproduce the crossing); whether a
  page can keep iOS's three-finger swipe from scrolling; and whether `execCommand('insertText')` from a key and a
  gesture's undo agree on an iPad with a keyboard.

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
- **A lost GPU, 2026-11-18.** Kris found figures dead after a page had been open a while, in Chrome on the Mac: the
  playground said "drawing failed at 0.77 s: unreachable", and fragcoord.xyz broke at the same time, which is Chrome's GPU
  process resetting every tab's. Reproduced headless by crashing Chromium's GPU process over the DevTools protocol
  (`Browser.crashGpuProcess`). On WebGPU the device was lost, and the next draw trapped in wgpu, at
  `wgpu-30.0.1/src/backend/webgpu.rs:2461:63`: "called `Result::unwrap()` on an `Err` value: RangeError: … createBuffer
  failed, size (48) is too large for the implementation when mappedAtCreation == true". After a trap the player throws
  "recursive use of an object" on every call. On WebGL2 the context was lost and nothing trapped: draws went on drawing
  nothing. Destroying the device from the page doesn't reproduce it, since a destroyed device takes a mapped buffer.
  - **What changed.** The renderer makes no buffer mapped, writing through the queue, so a lost device can't throw there.
    The player records GPU errors instead of panicking on them, knows when its device is lost, and says so at once
    (`on_lost`), and a draw on a lost device is an error and not an attempt. The element starts the GPU again when it hears
    the device is lost, when WebGL says its context is, or when a draw fails on the GPU's side, trap included: a new player
    and canvas, with the figure's sources, time, held params and pause kept, and its poster meanwhile. It tries at once,
    after a second and after five, then sends `error` with `detail.gpu`, and tries once more each time the page is shown;
    one that draws again then sends `recover`, which the site's controls and the playground take their message down on.
    A GPU error on a device that isn't lost is the figure's, and an `error` at once. A figure that has drawn for 30 s
    since, or that the reader has come back to, has its tries back. With no player its handles take no pointer, so a
    touch there scrolls, and in a figure of several panels a handle's reach stops at its panel's edge.
  - **Contexts.** A trapped player can't be freed: its call never returned, so what it holds stays referenced. Its
    device is let go of by its number (`let_go`), and so is what it was to say when lost. On WebGL2 the canvas keeps its
    context, lost with `WEBGL_lose_context` and brought back for the next player, since WebKit counts a context against
    its cap of 16 until its canvas is collected: making a new canvas each time, it lost the page's other figure's context
    after 14 restarts, plain or trapped, where Chromium frees the slot at once. A context the browser lost and doesn't
    bring back within 2 s is given up on, for a new canvas.
  - **Other figures.** A trap leaves only its own player unusable. On a page with two figures the other draws on, and a
    new player made after a trap works, on both backends and in WebKit. wgpu's state on WebGL2 is per player. Rust counts
    the panic and never uncounts it, since a trap ends it, so a second panic anywhere on the page aborts without the
    "panicked at" line. It still traps the same way, and a figure gets past it the same way.
  - **Tested** (`tests/lost.spec.js`): losing the GPU and resizing, on WebGPU (the GPU process crashed in Chromium, the
    device destroyed in WebKit) and on WebGL2 (`WEBGL_lose_context`), draws again at the same time with the same held
    params, no `error` and no panic in the console. Two figures, one lost, the other draws on. Every device lost at once,
    both come back. Traps one after another: at once, after 1 s, after 5 s, then an `error`, and `recover` once it draws
    again. Starts that keep failing are an `error` in about six seconds with the page in view. A start some other way
    (scrolled back, put back) isn't dropped by the next tab switch, and one put back sends `load`. A playing figure plays
    on. A handle with no player takes no pointer, and in two panels its reach stops at the edge. Twenty losses and
    twenty traps in a row on WebGL2 keep the other figure's context, in Chromium and WebKit.
  - **Not covered:** the exact race of a draw before the word that the device is lost ("no surface texture" before,
    "GPU surface texture unavailable" now, which starts it again) can't be made to happen on purpose, nor a GPU
    validation error on a device that isn't lost.
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

**Built 2026-11-14,** and shown then by `web/built.html`, a page written by hand around what `pencil build` wrote for
`corpus/013-tube` under its light and dark sheets. **That page is gone (2026-11-15).** Kris: it was too confusing to keep.
A figure is built into a page by something like the Astro component, and a page that loads sources as it runs is the
playground's case, which matters as much and is a different one. The site's pages are what a build is checked on:

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
- **Not checked:** a phone, a browser without WebGPU, and print. The poster's labels and the page's are the same fonts since
  2026-11-16, at the same widths (`formulas.spec.js` holds a page to the command's), where before the page's were the
  browser's own.

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
  0.08 s when nothing changed. What a reader fetches is named for what's in it, a hash of its files' bytes
  (`_pencil/tube-a5c1afca6244/tube.pfig`), so a copy they kept is never a stale one, and a host can tell a browser to
  keep it for good. The site does, in `site/public/_headers`.
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
- **The demos are drawn at the size the blog would give them (2026-11-18).** Kris: the side figures were drawn big, so
  the squaring figure's labels were small for it and the scaled copy took too much height, and the lattice ran past a
  phone's height. Each demo has a tier, named as the blog's `docs/figures.md` names them (`hero`, `primary`,
  `supporting`, `aside`), recorded once in `site/src/lib/figures.ts` and taken from the blog's use of the figure it was
  ported from. A figure is as wide as the smallest of three: its tier's width (60rem, 42rem, or 17rem for an aside), a
  share of the screen's height for its tier turned into a width by its shape (more for two rows of panels), and the
  width its sheets were designed at, since its labels are the page's text size at any width. The shape and design width
  are those of the sheets in use, under the same media queries as the figure's sources, and a look picked from the
  controls brings its own. Not matched to the blog: one aside width for two figures (the blog gives `SquareZ` 17rem
  and `ScaledCopy` 13rem, and at 13rem the squaring figure's labels would crowd it); primary figures stop at their
  design width where the blog's go up to 60rem; and the lattice's panel titles are drawn in the figure, so they can't
  wrap under their formulas on a phone as the blog's do, and the narrow sheet makes them smaller instead.
- **A figure in view when the page opens is marked `priority` (2026-11-15).** Its poster is the page's largest first
  paint, so it's fetched eagerly with `fetchpriority="high"`, where every other poster is lazy. A build knows what the
  figure will fetch, so it also writes a line of script that starts on the WASM and on the compiled figure its media
  picks, at low priority behind the poster, and leaves them for the element to take (`ahead`, in `host.js`). On the
  site's home page the figure and the WASM start with the page and are downloaded once each, in Chromium and in WebKit.
  **It was `<link rel="preload" as="fetch">` first, and WebKit downloaded both again** when the script asked for
  them, which Chromium didn't: two responses for the 4.6 MB WASM. A test counts the downloads now, and that a fetch
  which failed is made again. A site that forbids inline scripts can't have this, and loses only the head start.
- **Controls are in the page before the figure is (2026-11-15).** Kris: the binding info should be taken at build, so
  controls are built in their first state and disabled until the figure is ready, since building them on load moved the
  page, and an `<output>` should have a width from its range so a changing value doesn't wrap its row.
  - `pencil build` writes every param's value and range at the poster's moment (`poster.values`, `poster.ranges`), and
    `build()` gives them as `moment`. The poster's moment is where a page that holds the same params starts the figure,
    so those are the controls' first values: the lattice built with `q = 4` and `p = 9` held says `p = 4` in `0..4`.
  - On the site one function writes the controls (`site/src/scripts/controls-markup.js`), for the build and for the
    page. `Figure.astro` writes them disabled, and `<figure-controls>` turns them on at `load`, and writes them again
    only when the figure is another kind of thing than they were made for, as when a reader picks another look.
  - An output is as many `ch` wide as the longest value its range allows, and every caption of a figure lies in one grid
    cell with only the beat's own showing, so the row is as tall as the longest from the start. That leaves blank lines
    under a short caption on a phone.
  - Measured in headless Chromium on the demos page, at 1633×919 and 375×812: no layout shift while its 17 figures load
    one after another, and none while stepping through `004-bounds`' beats and moving its slider. Its controls are
    602×201 and 343×240 with scripts off and the same once ready.
  - **Seen while checking:** on a phone a poster's labels are smaller than the figure's, since the poster is a picture
    scaled down and a live label is an em of the page at any size. They change size when the figure takes over.
  - Controls are still the page's and not the package's. The blog's own would use `info` and `moment` the same way.
- **Posters are WebP too (2026-11-15).** The home page's poster at twice the size was a 1.2 MB PNG, which is what that
  first paint waits for. `pencil build` now writes each poster as lossy WebP as well, through libwebp at quality 90 with
  sharp chroma, since a figure is thin colored lines on flat paper, and keeps it when it's the smaller. It was for all 58
  of the site's posters: 1.9 MB against 9.0 MB, and 146 KB for that one. The component offers the WebP first and the PNG
  behind it. A crop of the lattice at twice the size shows no difference. The CLI builds libwebp from C, which is its
  first dependency that isn't Rust. The card stays PNG.
- **What's in the WASM (2026-11-15),** since a page of compiled figures still fetches all of it. It was 5.35 MB, 1.69 MB
  gzipped, 1.19 MB as brotli. Of its 4.06 MB of code, naga is 21%, wgpu's core 15%, Rust's `core` and `alloc` 24%, and
  the Pencil core 20%. What only a page that reads sources needs (`compile`, `parse`, `style`, `latex`, `diff`,
  `suggest`, `inspect`, `files`) is 0.46 MB, 11% of the code, so a WASM for compiled figures alone would be about a
  tenth smaller. A seventh of the file was function names, a 0.73 MB section that only shows in a
  panic's stack. `scripts/build-web.sh` now leaves it out, unless `PENCIL_WASM_NAMES=1` asks for it to read one: the
  WASM is 4.62 MB, 1.51 MB gzipped and 1.06 MB as brotli. The WASM for compiled figures alone isn't built.
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

**`--param` names a param, and a video holds it, 2026-11-21.** Kris took it that `pencil video` set a param time doesn't
drive, and it didn't: the flag was read and dropped, so every video was the figure with its params where the document left
them. A video now holds each for all its frames (`pencil video … --param n=5`). One that follows time, itself, through what
it reads or through the range it's kept in, is an error there, since each frame already has its value: a still takes it, as a
reader's hand does. Kris, the same day: time and a hand take turns driving a param, so a still that's given `--t` is time's
turn, and `--t` with a `--param` that follows time is an error on `render`, `svg`, `build` and `check` as well. Either
alone is taken. For the same reason `--t` hands back a time-driven param the document's `poster` holds, which used to stay
held, so the time changed nothing it drew. A document's `poster` has the rule too, in the core: `time` and a param that
follows time, both in one poster, is an error (`decisions.md`). No figure here or on the blog had one. On every command a name that is no param is an error too. It used to change nothing, and a `let`'s name
stood in for the `let`, which the language lets nothing but the document set. A name's subscript can be written any of a
document's ways (`r1` for `r_1`). The check is the command's, in `held_params`, and the core still takes whatever inputs
it's handed, so the page's `set_param` is as it was.

**`duration` from the beats, 2026-11-21.** Kris wants `duration` readable wherever the figure has one. It was an error
without a `duration` statement, even where beats with lengths added up to the number a video and the page's clock already
used. Now it reads that sum (`language.md`, Expressions; `decisions.md`), above the beats or below. A beat's length or a pause
that reads it, itself or through a `let`, would be part of the sum, and gets an error saying so at the line that reads it.
The explainer's limaçon, which has no statement, can now read its own 65.6 s. No golden changed, and the IR lost the flag that
said whether a duration was stated, which only this refusal read.

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
