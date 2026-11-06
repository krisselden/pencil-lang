# The Pencil language

What exists today, and the decisions behind it. A test in
`crates/pencil-core/tests/language_doc.rs` fails if a built-in function, style property or
statement goes undocumented here. The syntax can still change freely (`goals.md`), so treat
this as a description of the current language, not a promise.

The corpus in `corpus/` has working examples of everything below.

## Documents

A `.pen` file is a list of statements. Comments run from `//` to the end of the line. Style
rules can sit in the document or in separate style files.

| Statement | Meaning |
|---|---|
| `doc name` | Names the document. |
| `duration 13` | How long one pass of the time bindings takes, for looping and video. |
| `viewport { … }` | The world window (see below). |
| `let x = expr` | A named value: a number, point, circle or line. |
| `fn f(a, b) = expr` | A function, inlined where it's called. No recursion. |
| `param a = 1 in 0..tau` | A root that time or a reader can drive. The range constrains drags. |
| `solve (x, y) from 0, 1: f(x, y) = 0, g(x, y) = 2` | Unknowns defined by equations (see below). |
| `bind a = expr` | What drives a param when no reader has taken it over. |
| `beat "caption" tag t { … }` | A discrete step of a construction (see below). |
| `curve c = …` | A drawn curve, in one of the forms below. |
| `segment s = A, B` | A straight line between points. |
| `path p = A, B, C` | An open line through points. |
| `polygon p = A, B, C` | A closed line through points. |
| `rightangle m = A, V, B size 0.2` | A right-angle mark at `V`, a world size along each side. |
| `region r = A, f(t) for t in 0..1` | A filled area bounded by points and arcs. |
| `dot P, Q` | Draws point bindings. A dot takes its point's name, so a style can target it. |
| `label n = "−{q}" at P offset (12, -12)` | Text at a point, offset in `px` with y down. `{expr}` in the text is a value, and a whole number prints as one. |
| `arrow v = A, B` | A shaft and a head, as one line. |
| `dots d = P(i) for i in 1..n with z = f(i)` | One mark drawn many times (see below). |
| `circles c = circle(C(i), T(i)) for i in 1..n` | The same, as circles. |
| `group g(map, k) { … }` | Marks to draw in more than one place. A parameter can be a function. |
| `use g(f, 2)` | Draws a group here, with these arguments. |
| `outline o = detail` | The rectangle another panel is looking at. |
| `panel p "Title" { … }` | One view of the figure, with its own marks (see below). |
| `panels { columns: 2, gap: 14, title: 26 }` | How panels are laid out, in design pixels. |
| `place p = P offset (8, 18) hang end` | Where an expression can sit (see below). |
| `expression e by step { … }` | A list of states whose matching parts move (see below). |
| `handle P sets a = expr` | Dragging `P` sets param `a`. `expr` can use `pointer.x` and `pointer.y`. |
| `style selector { … }` | A style rule (see below). |
| `tag a, b` | After a statement, tags the marks it made. |

### Curve forms

Each form keeps its own representation (`goals.md`).

| Form | Example | Drawn by default |
|---|---|---|
| Implicit | `curve c = implicit (x, y): x² + y² = r²` | Traced to a line |
| Exact distance | `curve c = sdf (x, y): sqrt(x² + y²) - r` | Per pixel |
| Parametric | `curve c = parametric (t): (cos(t), sin(t)) for t in 0..tau` | Sampled to a line |
| Construction | `curve c = circle(O, A)`, `curve g = ray(O, B)` | A circle per pixel, a line or ray as a line |

### Viewport

`viewport { x: -2..2, y: -2..2, width: 640, height: 400 }`

- `x`, `y`: a fixed world window.
- `width`, `height`: the design size. With `width`, `px` sizes scale with the output, as an
  SVG's do with its `viewBox`. Without it, a `px` is a CSS pixel.
- `fit: a b c`: bounds these points and circles each frame instead of a fixed window, ignoring
  any param a handle has taken over, so a drag doesn't rescale the figure under the pointer.
- `pad: 0.04`: extra space around a `fit`, as a fraction of its size.

### Beats

A `beat` block groups marks into a discrete step. Any `beat` block declares the param `beat`,
ranging over the beats, and its marks get the block's tags. A `self.age` in a style is that
mark's own `beat − index`, so one rule can fade every earlier beat.

### Equations

`solve` defines unknowns by equations, and they're then ordinary names:

```
solve (mu, nu) from 0, exp(pi² / (6·n2)):
  tangency(w, mu, nu) = tangency(w1, mu, nu),
  tangency(one, mu, nu) = tangency(w1, mu, nu)
let k = sqrt(tangency(w, mu, nu))
```

It runs Newton's method each frame, using exact symbolic derivatives, on as many unknowns as there are equations.

- **It starts from `from` every time,** with no memory of the last answer, so a frame still depends only on its inputs.
  Which solution it finds is the start's business, and the start can use params.
- **No answer means absent.** A system that doesn't settle on a solution, within 50 steps and 1e-9, is absent, so whatever
  depends on it isn't drawn, and `exists` can ask.
- **The unknowns can't use functions Pencil can't differentiate.** That's a compile error, and it names the function.

### Collections

`dots` and `circles` draw one mark many times, once for each combination of their variables. A variable
counts in whole numbers and includes both ends of its range, and a later range can use an earlier variable:

`dots grid_points = grid(a, b) for a in -q..q, b in -2..q + 2`

- **It's one mark,** so one rule styles it all, and the count can follow a param from frame to frame.
- **`with` names a value per instance:** `circles c = circle(z, z + (r, 0)) for a in 0..q with z = spiral(a)` works `z` out
  once per circle. It can be a number or a point, and can use the names before it.
- **An instance that doesn't exist is skipped,** and one off the frame or too small to see costs nothing further.
- **Sizes can vary per instance:** `radius` and `width` can use the variables, and `self.scale`, the style pixels
  per world unit: `radius: min(0.3·k·self.scale·1px, 3px)`.
- **A collection is a backdrop:** labels don't try to avoid it.

### Panels

A `panel` is its own view of the figure, with its own marks, its own `viewport` and its own shader. What's outside
any panel is shared, so every panel sees the same params and bindings. A `group` draws the same marks through
different maps, since a parameter can be a function:

```
fn plain(w) = w
group edges(map) {
  curve side = parametric (t): map(corner·t) for t in 0..1
}
panel flat "Lattice" {
  viewport { fit: side, pad: 0.12, width: 400, height: 380 }
  use edges(plain)
}
panel log "Log space" "T(w) = 2πi·w / A" {
  viewport { fit: side, pad: 0.12, width: 400, height: 380 }
  use edges(above) as up
  use edges(to_turn)
}
```

A group's parameters are its own, so two groups can both have a `map`. `use … as up` draws a group again: what it declares
becomes `up_side`, and each mark is tagged with its plain name and `up`, so `style side` reaches every copy and `style .up`
reaches one set. After a panel's title can come a formula, styled by `style formula { fill: muted }`.

`outline o = detail` draws the rectangle another panel is looking at, to mark a close-up. `radius` keeps it big enough to read
as a marker, even when what it marks is only a few pixels across.

A mark carries its panel's name as a tag, so `style steps.whole { … }` styles it in that panel alone, and `style steps`
styles it in all of them. A rule that names a mark has to find it in at least one panel. `style title { fill: ink; size: 14px }`
styles panel titles. `fit` can name parametric curves as well as points and circles.

### Expressions that move

An expression is a list of states, driven by a number: at 3.4 it's on its way from state 3 to state 4.
State 0 is the empty one before the first that's written, and `empty` writes another.

```
place at_tip = tip offset (8, 18)
place at_sum = Z2 offset (-8, -12) hang end

expression work by step {
  "z" at at_z, "z.1 a" at at_tip
  "z" at at_z, "z.1 a + z.2 b i" at at_sum
  "z" at at_z, "z.1 ( a + b i )" at at_sum
  "z" at at_z, "z.1 ²" at at_sum
}
```

Terms are separated by spaces. Nothing is diffed: identity is what's written.

- The same term in the next state moves to its new spot, and stays solid.
- A new term fades in, and a term that's gone fades out, riding along with its neighbors.
- A copy is written `z.1`, and prints as `z`. When it's new, it starts on `z`.
- When a copy goes, it moves into a term of the same name at the same place: factoring.

Each term is a mark named `work.z.1`, with the tags `term`, `work` and `z`, so `style term.a { fill: blue }`
colors every `a`. A renderer measures the text with its own font, and core says where each term goes.

## Expressions

Numbers, names, `+ - * / ^`, `·` for multiply, `²` and `³`, unary `-`, and calls. `(a, b)` is a
point, and `P.x` and `P.y` read one. Names can be Unicode. `time`, `pi`, `π` and `tau` are
always available. Coordinates only exist inside the curve that names them.

Points add and subtract, scale by a number, and negate. All of this expands to scalars at
compile time, so the IR stays symbolic and bindings are evaluated once per frame.

### Numbers

| Function | Meaning |
|---|---|
| `sqrt(x)`, `exp(x)`, `ln(x)`, `abs(x)`, `sign(x)`, `floor(x)` | The usual. `sign(0)` is 0. |
| `sin(x)`, `cos(x)`, `tan(x)`, `acos(x)`, `atan2(y, x)` | Radians. |
| `min(a, b)`, `max(a, b)`, `clamp(x, lo, hi)` | Limits. |
| `mod(x, y)` | Floored, so the result takes `y`'s sign. |
| `mix(a, b, t)` | `a` at 0 and `b` at 1. Also mixes two colors, in linear light. |
| `smoothstep(a, b, t)` | 0 before `a`, 1 after `b`, eased between. |
| `reveal(age, now, later)` | 0 before a beat, `now` during it, `later` after it. |
| `snap(x)`, `snap(x, power)` | 0 at or before 0, and 1 at or after 1: gets there fast, then eases in. `power` defaults to 5. |
| `timeline(time, start, end, …)` | Which step of a list of moves a time is in: whole while holding, and between two, linearly, while moving. |

### Points and constructions

| Function | Meaning |
|---|---|
| `length(P)`, `dist(A, B)`, `dot(A, B)` | Numbers from points. |
| `midpoint(A, B)` | Halfway between. |
| `circle(C, T)` | About `C` through `T`. Fields `.x`, `.y`, `.r`. |
| `line(A, B)`, `ray(A, B)` | Through two points. A ray starts at `A`. Two points in one place make no line. |
| `perpendicular(P, l)` | The line through `P` at right angles to `l`. |
| `intersect(u, v, which)` | Where lines, rays and circles meet, if they do (below). `which` is 0 or 1, and defaults to 0. |
| `mirror(s, across)` | A point across a line or point, or a circle across a line. |
| `exists(v)` | 1 if all of `v` exists this frame, and 0 if not. |
| `otherwise(v, fallback)` | `v`, or all of `fallback` where `v` doesn't exist. They must be the same kind. |

`which` names a root, not a place among the points that remain. Along a line, 0 and 1 are in
order of its direction. For two circles, 0 is left of the line from the first center to the
second. A ray only has the points ahead of its start, so from inside a circle only its root 1
exists, and root 0 is absent, rather than root 1 taking its place and jumping.

### When a value doesn't exist

Geometry is partial, so a value can be absent on some frames. Only constructions make one:

- A line or ray that misses a circle, or a circle that's apart from, inside or the same as another.
- Parallel lines.
- A point behind the start of a ray.
- A line through two points in one place, or a `rightangle` with a side of no length.

Touching counts as meeting, within rounding, so a tangent by construction doesn't flicker.

Whatever is built from an absent value is absent too, and only `exists` and `otherwise` can see
the difference. What that means for each thing that uses one:

| Use | When its value is absent |
|---|---|
| A mark | Isn't drawn, along with its label, which pages must then hide. |
| A style value | Hides the mark. To respond instead, `let shown = exists(P)` and use `shown`. |
| A parametric curve's point | Leaves a gap there, and the curve can come in pieces. |
| A region's boundary | Hides the region: with a gap it doesn't enclose anything. |
| `fit` | Skips it, and if nothing is left, uses the declared `x` and `y`. |
| `handle` | Can't be grabbed. An inverse with no answer leaves the param where it is. |
| `paper` | A compile error: there has to be something to draw on. |
| A curve's field | Has to name the value with `let` first: a field runs per pixel, and existence is decided per frame. |

On a page, `player.value` reads an absent binding as `NaN`.

### Colors

| Function | Meaning |
|---|---|
| `oklch(l, c, h)` | Perceptual, with `h` in degrees. |
| `rgb(r, g, b)` | sRGB, 0 to 255. |
| `hue(k)` | A perceptual ramp, with `k` from 0 to 1 once around. |
| `white`, `black` | Names. |

## Styles

`style selector, selector { property: value; … }`. Later rules win, across files, with the
document's own rules last. There's no specificity (SPEC/15).

**A style sheet can name values** with `let ink = rgb(28, 27, 25)`, for itself and the sheets after it,
so `corpus/theme.style` holds a theme's colors and label roles. A document picks a role by tag:
`label n = "|z|" at M tag note`, and it never picks a number. A rule that names a mark has to find
it, but a rule by type or tag may match nothing, since a shared sheet can't know what each figure uses.

**Selectors** combine a name or type with tags: `c`, `circle`, `.aux`, `circle.blue`, and
`c::inside` for a facet. A mark's type is one of its tags: `circle`, `line`, `ray`, `curve`,
`segment`, `decoration`, `region`, `dot`, `label`, `arrow`, `term`, `dots`, `circles`, `outline`. `paper` is the background.

**Facets** are the two sides of a mark: `::negative` and `::positive` are the field's sign, and
`::inside` and `::outside` are aliases. The negative side is inside.

**Values are expressions**, so they can use `time`, params and bindings. Three more names exist
only here: `self.dist` (distance to the mark, in `fill` and `stroke`, on marks that have one),
`self.t` (position along a stroke, 0 to 1, in `width`), and `self.age` (see Beats).

| Property | Applies to | Meaning |
|---|---|---|
| `fill` | Regions, dots, labels, facets | A color. A field's `fill` needs a facet. |
| `fill_opacity` | Anything filled | Scales only the fill, on top of `opacity`. |
| `stroke` | Curves, lines, regions, dots | A color. |
| `width` | Strokes | In `px`. Can vary along the stroke with `self.t`. |
| `head`, `head_span` | Curves | With `head: 13px`, a curve ends in an arrowhead. `head_span: 1` holds it to 60% of the last 1 of the curve's parameter, so it can't dwarf a spiral's tight turns. |
| `dash`, `gap` | Lines | In `px`. A loop gets a whole number of dashes, stretched to fit. |
| `cap` | Lines | `butt` (default) or `round`. |
| `draw` | Strokes | A hint: `auto` (default), `line` or `field`. |
| `blend` | Edges | `gamma` (default, as SVG does) or `linear`. |
| `opacity` | Any mark | Scales the whole mark. |
| `radius` | Dots, circles, outlines | In `px`. A dot's size, a cap on a collection's circles, or an outline's least half-size. |
| `size` | Labels | In `px`. |
| `halo` | Labels | Full width in `px`, in the paper's color. |
| `font` | Labels | A role: `sans` (default) or `math`. |
| `place` | Labels | `fixed` (default) at its `offset`, or `settle`, tied there by a spring and pushed off what's drawn and other labels. |
| `head` | Arrows | The head's length in `px`. Defaults to 14. |
| `show` | Any mark | Draws it only while this is above 0, decided per frame. Can also read `view.width`, the view's width in world units, for zoom levels. |

## Decisions

Newest first. "Why" is the reason at the time, so these can be revisited.

| Date | Decision | Why |
|---|---|---|
| 2026-11-06 | Hot loops run through `fast.rs`, with `eval` as the definition | A frame evaluates thousands of times, and resolving names first took this figure from 20 ms to under 8. A test requires them to agree to the bit |
| 2026-11-06 | A thing keeps its name across panels, and only its label's text is the space's | Kris: 0 and 1 are one point in different spaces, and connections like that are worth keeping |
| 2026-11-06 | A group's parameters are scoped to each use, and `use … as` prefixes what it declares | Log space draws the same edges three times, and two groups in one panel both wanted a `map` |
| 2026-11-06 | A curve's arrowhead is a style, with its stroke stopping inside the head | The same mark as the curve, and the blog's construction: the head points along the chord, so it stays on the line where the curve bends |
| 2026-11-06 | A label's text holds values as `{expr}` | The counts on the edges follow q and p, and the text is otherwise as written |
| 2026-11-05 | `solve` starts over each frame, and failure is absence | A frame stays a function of its inputs, and a figure with no solution shows nothing instead of garbage |
| 2026-11-05 | Newton's method uses symbolic derivatives | The IR already has them, so no finite-difference step to tune. The site's uses central differences |
| 2026-11-05 | A figure remembers a system's last answer, keyed by everything it depends on | A frame reads its bindings several times, and this can't change what's shown |
| 2026-11-05 | A collection is one mark, drawn through coverage layers | Hundreds of circles can't each be a block of shader and a uniform, and the count has to be able to change |
| 2026-11-05 | A variable's range includes both ends | Lattices are written that way, and `1..n` should have n in it |
| 2026-11-05 | A panel is its own scene, on one shared canvas | WebGL2 can't share a device across canvases, and a video needs one canvas anyway |
| 2026-11-05 | A `group` takes functions as arguments | "A figure is a composition of functions": the same marks through a different map is what a panel usually is |
| 2026-11-05 | Expressions are lists of states, with identity in what's written | Manim's result, where what matches moves and only the rest fades, without its diff. Kris on the placement in `lab/expressions`: "beautiful" |
| 2026-11-05 | Moves use `snap`, a power of 5 | Kris wants a move to get there fast and leave time to take it in. Above 5, `za` joining `+zbi` goes by too quickly to see. The blog's is 4 |
| 2026-11-05 | Core lays out terms with widths from the renderer | The matching is the language's, and the fonts are the renderer's. It comes out as ordinary labels |
| 2026-11-05 | A step is a param, and `timeline` only one way to drive it | Same model as beats and sliders, so the arrows and the labels can bind to one number |
| 2026-11-05 | Labels move by settling springs, with a second spring on top | Kris chose it by eye in `lab/labels`: the placer jumps, a spring on the placer looks indecisive, and settling alone is wiggly |
| 2026-11-05 | Placement estimates text width from its length | It's soft anyway, and a page and a video then agree without sharing fonts |
| 2026-11-05 | A label's spring runs over earlier times in video, and over the clock on a page | A drag has no time axis, and a video's frames must not depend on order |
| 2026-11-05 | `show` uses the same per-frame flag as absent values | One way to be not there, and a hidden mark skips its shader work |
| 2026-11-05 | A construction's circle follows its own path when it needs a line | Tracing one for its dashes cost 12.7 ms a frame, for a path it already knew |
| 2026-11-05 | `which` names a root, not a place among the points left | One formula per root, so nothing jumps when a ray's start enters a circle. This replaced "a ray from inside has one point, and it's both 0 and 1" |
| 2026-11-05 | A ray means a ray when intersecting lines too | A point behind its start isn't on it. `bounds.pen` came out identical, so nothing relied on the old reading |
| 2026-11-05 | A value's existence is an explicit condition in the IR (`Where`), not NaN | NaN can't tell a missing point from a bug, `min` and `max` swallow it, and WGSL leaves it undefined |
| 2026-11-05 | Existence is decided on the CPU per frame, and the shader gets a flag per mark | No partial values reach the GPU, and a vanished mark skips its shader work |
| 2026-11-05 | `exists` and `otherwise` are the only way to see absence | Everything else propagates it, so a figure can't half-use a value that isn't there |
| 2026-11-05 | `self.age` is substituted per mark at compile time | One style rule fades every beat, and the runtime needs nothing new |
| 2026-11-05 | A beat is a discrete param, not a separate timeline feature | Same model as time and sliders, so video and buttons both just bind it |
| 2026-11-05 | A dragged param uses `bind` plus an override, like time and a slider | Sliders and drags need no new mechanism, and `q` can set θ through its binding |
| 2026-11-05 | `handle` takes an explicit inverse, and the param's range is the constraint | SPEC/16's explicit grade. Automatic inverses wait until something needs them |
| 2026-11-05 | Constructions lower to named scalars | Stays symbolic and costs little per frame, and circles get exact distance fields for free |
| 2026-11-05 | A `fit` view ignores handle-driven params | A drag mustn't rescale the figure under the pointer, as on the blog |
| 2026-11-05 | `font` names a role, not a typeface | Fonts differ per renderer, and naming real fonts is still open |
| 2026-11-05 | `viewport { width }` makes `px` scale with the output | The blog's sizes are in drawing units, and video needs the same look at any size |
| 2026-11-05 | Marks keep their written form, and `draw` is a hint | Normalizing costs about 50× on the same circle, and a hint can't change meaning (tested) |
| 2026-11-05 | Edges blend in gamma space by default | SVG's look is the bar, and linear visibly thins dark lines |
| 2026-07-08 | `::inside` is the negative side, with counterclockwise positive | Kris's convention, matching SDFs and Green's theorem. From the July 8 chat |
| 2026-07-08 | Style values are expressions | Fills can animate, and `self.dist` needs no special mechanism. From the July 8 chat |
| 2026-07-08 | Layers and order, with no specificity | SPEC/15 |
| 2026-07-08 | Only named bindings can be styled | SPEC/15 |

## Not built from the specs

`SPEC/15`: path selectors, `in clip(…)`, `:hover`, styling one instance of a collection by its key, branch labels with continuity
tracking, `:appearing`, `:vanishing` and `on appear: fade`. `SPEC/16`: relations, rigs, automatic inverses. `PLAN`: charts, modules (`use`,
`expose`), timelines with clips, profiles, the print round-trip, Unicode name normalization.
Also not built: `orient ccw` and `reverse`.
