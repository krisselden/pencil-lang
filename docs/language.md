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
| `bind a = expr` | What drives a param when no reader has taken it over. |
| `beat "caption" tag t { … }` | A discrete step of a construction (see below). |
| `curve c = …` | A drawn curve, in one of the forms below. |
| `segment s = A, B` | A straight line between points. |
| `path p = A, B, C` | An open line through points. |
| `polygon p = A, B, C` | A closed line through points. |
| `rightangle m = A, V, B size 0.2` | A right-angle mark at `V`, a world size along each side. |
| `region r = A, f(t) for t in 0..1` | A filled area bounded by points and arcs. |
| `dot P, Q` | Draws point bindings. A dot takes its point's name, so a style can target it. |
| `label n = "P" at P offset (12, -12)` | Text at a point, offset in `px` with y down. |
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

### Points and constructions

| Function | Meaning |
|---|---|
| `length(P)`, `dist(A, B)`, `dot(A, B)` | Numbers from points. |
| `midpoint(A, B)` | Halfway between. |
| `circle(C, T)` | About `C` through `T`. Fields `.x`, `.y`, `.r`. |
| `line(A, B)`, `ray(A, B)` | Through two points. A ray starts at `A`. |
| `perpendicular(P, l)` | The line through `P` at right angles to `l`. |
| `intersect(u, v, which)` | Where lines, rays and circles meet (order below). `which` is 0 or 1, and defaults to 0. |
| `mirror(s, across)` | A point across a line or point, or a circle across a line. |

Along a line, intersections come in order of its direction. A ray keeps only the points ahead
of its start, so from inside a circle it has one point, and that's both 0 and 1. For two
circles, 0 is left of the line from the first center to the second.

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

**Selectors** combine a name or type with tags: `c`, `circle`, `.aux`, `circle.blue`, and
`c::inside` for a facet. A mark's type is one of its tags: `circle`, `line`, `ray`, `curve`,
`segment`, `decoration`, `region`, `dot`, `label`. `paper` is the background.

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
| `dash`, `gap` | Lines | In `px`. A loop gets a whole number of dashes, stretched to fit. |
| `cap` | Lines | `butt` (default) or `round`. |
| `draw` | Strokes | A hint: `auto` (default), `line` or `field`. |
| `blend` | Edges | `gamma` (default, as SVG does) or `linear`. |
| `opacity` | Any mark | Scales the whole mark. |
| `radius` | Dots | In `px`. |
| `size` | Labels | In `px`. |
| `halo` | Labels | Full width in `px`, in the paper's color. |
| `font` | Labels | A role: `sans` (default) or `math`. |

## Decisions

Newest first. "Why" is the reason at the time, so these can be revisited.

| Date | Decision | Why |
|---|---|---|
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

`SPEC/15`: path selectors, `in clip(…)`, `:hover`, keyed collections, branch labels with continuity
tracking. `SPEC/16`: relations, rigs, automatic inverses. `PLAN`: charts, modules (`use`,
`expose`), timelines with clips, profiles, the print round-trip, Unicode name normalization.
Also not built: `orient ccw` and `reverse`.
