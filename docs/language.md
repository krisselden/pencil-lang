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
| `beat spin "caption" for 2 show P drop c focus P, .arms` | One move of a figure's story, with a length (see below). |
| `pause 0.4` | A hold between moves, in seconds. |
| `curve c = …` | A drawn curve, in one of the forms below. |
| `segment s = A, B` | A straight line between points. |
| `path p = A, B, C` | An open line through points. |
| `polygon p = A, B, C` | A closed line through points. |
| `rightangle m = A, V, B size 0.2` | A right-angle mark at `V`, a world size along each side. |
| `region r = A, f(t) for t in 0..1` | A filled area bounded by points and arcs. |
| `dot P, Q` | Draws point bindings. A dot takes its point's name, so a style can target it. |
| `label n = "−{q}" at P away D` | Text at a point, on the side `D` points to, in the figure's own coordinates. Its style's `distance` says how far. `{expr}` in the text is a value, and a whole number prints as one. Without text, each panel gives it (see Panels). |
| `label n = "P" at P offset (12, -12)` | The same, at a fixed shift in `px` with y down. A sheet can't change it, so prefer `away`. |
| `arrow v = A, B` | A shaft and a head, as one line. |
| `dots d = P(i) for i in 1..n with z = f(i)` | One mark drawn many times (see below). |
| `circles c = circle(C(i), T(i)) for i in 1..n` | The same, as circles. |
| `curves c = parametric (t): P(t, i) for t in a..b for i in 1..n` | The same, as curves. |
| `group g(k) { … }` | Things to draw in more than one place, declared once in their own coordinates. |
| `use g(2)`, `use g(2) through f as up` | Draws a group here, optionally through a function and under a prefix (see Panels). |
| `text n = "1"` | What a label says in this panel (see Panels). |
| `outline o = detail` | The rectangle another panel is looking at. |
| `panel p "Title" { … }` | One view of the figure, with its own marks (see below). |
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

- `x`, `y`: a fixed world window. It always has one scale for both axes, so at a different shape it shows more, rather than stretching.
  Its bounds are constant, so they can't read `time` or a param. To follow something that moves, `fit` it.
- `width`, `height`: the design size. With `width`, `px` sizes scale with the output, as an
  SVG's do with its `viewBox`. Without it, a `px` is a CSS pixel.
- `fit: a b c`: bounds these points and circles each frame instead of a fixed window, ignoring
  any param a handle has taken over, so a drag doesn't rescale the figure under the pointer.
- `pad: 0.04`: extra space around a `fit`, as a fraction of its size.

### Beats

A `beat` block groups marks into a discrete step. Any `beat` block declares the param `beat`,
ranging over the beats, and its marks get the block's tags. A `self.age` in a style is that
mark's own `beat − index`, so one rule can fade every earlier beat.

**A beat can have a length,** which makes it one move of a figure's story. Beats and pauses run in
the order they're written, and their lengths add up to the `duration`, so making one longer moves
everything after it instead of everything having to be retimed:

```
pause 0.4
beat grow "A circle." for 1 show c
beat spin "A point goes round." for 2 show P focus P
pause 1
beat gone for 0.5 drop c, P
```

- **`beat` is a number,** whole while the figure holds and between two whole numbers while a beat
  moves: 1.5 is halfway through the second beat. Time drives it, and a reader stepping through
  or a `bind beat` can drive it instead, and everything below follows.
- **A beat's name is its progress,** 0 until it starts, 1 once it's done, and moving evenly in
  between. So `let r = grow` grows with that beat, and `snap(grow)` eases it.
- **`show` and `drop` say when marks are there,** by name or `.tag`, and a beat's block shows
  what it declares. A mark that's ever shown isn't there before it, a dropped one isn't there
  after, and while it isn't there it isn't drawn at all. A mark no beat names is always there.
  How it arrives and leaves is the sheet's, with `self.life`.
- **`focus` says what a beat is about,** by name or `.tag`. It holds until a later beat names
  something else, and before any does, everything is in focus. A sheet reads it as `self.focus`.
- **A caption holds until a beat has another,** and `""` clears it. Captions are drawn in the
  figure only when a sheet asks, with `style caption { … }`, since a page can show them as text.
- Beats with lengths and beats without can't be mixed in one figure.

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

`dots`, `circles` and `curves` draw one mark many times, once for each combination of their variables. A variable
counts in whole numbers and includes both ends of its range, and a later range can use an earlier variable:

`dots grid_points = grid(a, b) for a in -q..q, b in -2..q + 2`

A curve has its own parameter too, which runs smoothly, and comes first. So what Desmos writes as `s = [-3...3] / 3` is:

`curves across = parametric (t): x0 + s·r / 3 + t·i for t in -r..r for s in -3..3`

Its range can read what counts it, and `with` names can be used in both. `curves` can go `through` a map, and a viewport can `fit` one, which fits all of them. It's lines, so it has a `stroke`
and no `fill`, and that `stroke` is one color for all of them. Its `width` can vary per curve, and along each with `self.t`, and each gets its own dashes and head. Where curves in one collection cross, they don't darken each other, as they do when they're separate marks.

- **It's one mark,** so one rule styles it all, and the count can follow a param from frame to frame.
- **`with` names a value per instance:** `circles c = circle(z, z + (r, 0)) for a in 0..q with z = spiral(a)` works `z` out
  once per circle. It can be a number or a point, and can use the names before it.
- **An instance that doesn't exist is skipped,** and one off the frame or too small to see costs nothing further.
- **Sizes can vary per instance:** `radius` and `width` can use the variables, `self.scale`, the style pixels per world
  unit, and for `dots`, `self.x` and `self.y`, where the dot ended up: `radius: min(0.3·k·self.x·self.scale·1px, 3px)`.
- **`circles` can be circles through three points,** so `circles c = circle(f(p), f(q), f(s)) for …` is what a Möbius map makes of a family. Any of them can be a line this frame, and is drawn as one.
- **Circles have sides, per instance:** `style c::inside { fill: … }`. A side's `fill` and `fill_opacity` can use the variables, the `with` names, `self.dist`, `self.pixel` and `self.curvature`. Every pixel adds up every circle,
  so this costs about 0.09 ms per circle at 1080² on an M1 Pro, and it's limited to 256 per collection, which is an error past that. A collection with only rims doesn't pay for it and has no limit.
- **A collection is a backdrop:** labels don't try to avoid it.

### Panels

A `panel` is its own view of the figure, with its own marks, its own `viewport` saying what it looks at, and its own shader.
How panels sit is a sheet's (see Styles). What's outside any panel is shared, so every panel sees the same params and bindings.

**A thing is declared once, and each panel draws it through a map.** A `group` holds things in their own coordinates, and
`use … through` sends them through a function:

```
fn to_turn(w) = …
let zero = (0, 0)
group edges() {
  curve side = parametric (t): corner·t for t in 0..1
  dot zero
  label name_zero at zero away (-1, 0)
}
panel flat "Lattice" {
  viewport { fit: side }
  use edges()
  text name_zero = "0"
}
panel log "Log space" "T(w) = 2πi·w / A" {
  viewport { fit: side }
  use edges() through above as up
  use edges() through to_turn
  text name_zero = "0"
}
```

- **What goes through:** parametric curves, two-point segments, dots, labels, and `dots` and `curves` collections. A segment becomes the map's
  image of it, so it can bend. A construction or a `circles` collection can't, since the image of a circle isn't a circle: map a
  center in its `with` instead.
- **Only marks move.** A `let` stays what was written, in the group's coordinates, so one point has one value, and what a panel
  shows is its image.
- **A label's `away` goes through too,** exactly, by the map's derivative at that point. A direction worked out once in the
  group's coordinates keeps its side of a curve in every panel, as long as the map doesn't flip orientation.
- **Maps compose:** a group can `use` another `through` a map, and the outer one applies last.
- **`use … as up` draws a group again:** what it declares becomes `up_side`, and each mark is tagged with its plain name and `up`,
  so `style side` reaches every copy and `style .up` reaches one set.
- **A group's parameters are values,** and they're its own, so two groups can both have a `k`.

**What a thing is called is up to each panel.** `label n at P` declares where a name goes, and `text n = "1"` says what it is here,
with `{expr}` for values. A panel with no text for a label doesn't show it, since that space has no name for it. `text` for a
label that doesn't exist is a mistake. `label n = "P" at P` still writes both at once.

After a panel's title can come a formula, styled by `style formula { fill: muted }`.

`outline o = detail` draws the rectangle another panel is looking at, to mark a close-up. `radius` keeps it big enough to read
as a marker, even when what it marks is only a few pixels across.

A mark carries its panel's name as a tag, so `style steps.whole { … }` styles it in that panel alone, and `style steps`
styles it in all of them. A rule that names a mark has to find it in at least one panel. `style title { fill: ink; size: 14px }`
styles panel titles. `fit` can name parametric curves and `curves` collections as well as points and circles.

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
- A term is in one place, so a state can't write the same one twice: `a + b +.1 c` has two plus signs. A number can't have a copy yet, since `2.1` is a number.
- When a copy goes, it moves into a term of the same name at the same place: factoring.

Each term is a mark named `work.z.1`, with the tags `term`, `work` and `z`, so `style term.a { fill: blue }`
colors every `a`. A renderer measures the text with its own font, and core says where each term goes.

## Expressions

Numbers, names, `+ - * / ^`, `·` for multiply, `²` and `³`, unary `-`, and calls. `(a, b)` is a
point, which is also a complex number (see below), and `P.x` and `P.y` read one. Names can be Unicode. `time`, `pi`, `π` and `tau` are
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
| `circle(C, T)` | About `C` through `T`. Fields `.x`, `.y`, `.r`. Its disk is its negative side. |
| `circle(A, B, C)` | Through three points, in order (below). Fields `.a`, which is its signed curvature, and `.x`, `.y`, `.r`, which don't exist on a line. |
| `flip(c)` | The same circle running the other way round, so its sides trade signs. |
| `line(A, B)`, `ray(A, B)` | Through two points. A ray starts at `A`. Two points in one place make no line. |
| `perpendicular(P, l)` | The line through `P` at right angles to `l`. |
| `intersect(u, v, which)` | Where lines, rays and circles meet, if they do (below). `which` is 0 or 1, and defaults to 0. |
| `mirror(s, across)` | A point across a line or point, or a circle across a line. |
| `exists(v)` | 1 if all of `v` exists this frame, and 0 if not. |
| `otherwise(v, fallback)` | `v`, or all of `fallback` where `v` doesn't exist. They must be the same kind. |

A circle through three points has a handedness. Going counterclockwise, its negative side is its disk,
and its curvature is positive. Going clockwise, its negative side is everything else, and its
curvature is negative. Three points in a line are a line, which runs through 0 curvature without
any value going to infinity, and has no center or radius. Two points in one place make no circle.

`which` names a root, not a place among the points that remain. Along a line, 0 and 1 are in
order of its direction. For two circles, 0 is left of the line from the first center to the
second. A ray only has the points ahead of its start, so from inside a circle only its root 1
exists, and root 0 is absent, rather than root 1 taking its place and jumping.

### Complex numbers

**A point is a complex number.** `(x, y)` and `x + y·i` are the same value, there's no second type, and `i` is the point `(0, 1)`:

```
let omega = -1 / 2 + sqrt(3) / 2·i
fn to_turn(w) = tau·i·w / A
fn spiral(t) = exp(t)
```

- **A number is a point on the real axis,** wherever a point is wanted, so `curve c = parametric (t): -t for t in 0..q`, `label n = "1" at 1`,
  `circle(0, 1)` and `1 + omega` all work, and a number answers `.x` and `.y`.
- **Arithmetic is complex:** `·` and `/` between two points multiply and divide them as complex numbers, and `+`, `-` and scaling are what they
  were. `dot(a, b)` is still the dot product.
- **Powers:** `z²` and `z^-3` multiply out, up to a power of 8. Anything else, such as `z^2.5` or `z^i`, is `exp(w·ln(z))`, on the principal branch.
- **Functions:** `real(z)`, `imag(z)`, `conj(z)`, `abs(z)` and `arg(z)` read it. `exp`, `ln` and `sqrt` take one too, with principal values.
  `ln(0)` and `arg(0)` don't exist, and a figure with one draws what it can (see below). `sin`, `cos` and the rest take numbers only, for now.
- **Hyperbolic functions:** `sinh(x)`, `cosh(x)`, `tanh(x)`, `coth(x)`, `sech(x)`, `csch(x)` and `arctanh(x)` take numbers and complex numbers. They compile to `exp` and `ln`.
- **`i` gives way to anything named `i`,** so `for i in 1..q` still counts, and inside it `i` is the count. It's clearer to use another name there.
  It also means a stray `i` outside a loop isn't an unknown name any more.
- **Exact derivatives see through all of it,** so `solve` and a label's `away` work on complex values. It all compiles to plain numbers, so a
  shader and an evaluator don't know it's there.

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

**A style sheet can name values** with `let ink = rgb(28, 27, 25)`. The last sheet to define a name decides what it
means everywhere, including in rules written before it, as a CSS variable does. So `corpus/theme.style` holds a theme's
colors and label roles, and `corpus/dark.style` only redefines the colors. A name can use another defined in any order, but
not itself. A document picks a role by tag: `label n = "|z|" at M tag note`, and it never picks a number. A rule that names a mark has to find
it, but a rule by type or tag may match nothing, since a shared sheet can't know what each figure uses.

**Selectors** combine a name or type with tags: `c`, `circle`, `.aux`, `circle.blue`, and
`c::inside` for a facet. A mark's type is one of its tags: `circle`, `line`, `ray`, `curve`,
`segment`, `decoration`, `region`, `dot`, `label`, `arrow`, `term`, `dots`, `circles`, `outline`. `paper` is the background.

**Facets** are the two sides of a mark. `::negative` and `::positive` are the field's sign. On a circle, `::inside` and
`::outside` are its true disk and everything else, whichever way round it runs, and on a line, where there's no disk, they follow the sign. Where both name the same part of the plane,
`::inside` and `::outside` win. On anything else that has sides, `::inside` and `::outside` are aliases for the sign, with the negative side inside.

So a circle through three points that turns over goes through a line and comes back with its sides' signs where they were, and its disk on the other one. Style `::negative` and the plane doesn't flash, and style `::inside` and
the color stays with the disk. A style can tell which it is from `self.curvature`.

**Values are expressions**, so they can use `time`, params and bindings. A few more names exist only here:

| Name | Meaning |
|---|---|
| `self.dist` | Distance to the mark, in `fill`, `stroke` and `fill_opacity`, on marks that have one. |
| `self.pixel` | How big a device pixel is in world units, wherever `self.dist` is, so that a glow can be a number of pixels across. |
| `self.curvature` | A circle's signed curvature: above 0 counterclockwise, below 0 clockwise, and 0 on a line. |
| `self.t` | Position along a stroke, 0 to 1, in `width`. |
| `self.age` | See Beats. |
| `self.life` | How present the mark is, from 0 before a beat shows it to 1, and back to 0 as one drops it, moving with those beats. 1 for a mark no beat names. |
| `self.focus` | How much the beats are about the mark, from 0 to 1, changing as a beat that names something moves. 1 until a beat names anything. |

**Colors can be scaled and added:** `white·4`, `mix(a, b, t) + c / 3`. They aren't clipped until the end, and `rgb` can go past 255. `sinh`, `cosh` and `tanh` also work in a style. So a glow reads
`fill_opacity: tanh(2·self.pixel / abs(self.dist))`.

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
| `blend` | Edges, sides | `gamma` (default, as SVG does), `linear`, or `add`, which adds to what's there in display values and never covers it. |
| `tone` | `paper` | `clip` (default) or `tanh`, which runs the finished picture through `tanh` in display values, so that what `blend: add` piles up stays in range. |
| `opacity` | Any mark | Scales the whole mark. |
| `radius` | Dots, circles, outlines | In `px`. A dot's size, a cap on a collection's circles, or an outline's least half-size. |
| `size` | Labels | In `px`. |
| `halo` | Labels | Full width in `px`, in the paper's color. |
| `font` | Labels | A role: `sans` (default) or `math`. |
| `place` | Labels | `fixed` (default) at its `offset`, or `settle`, tied there by a spring and pushed off what's drawn and other labels. |
| `head` | Arrows | The head's length in `px`. Defaults to 14. |
| `distance` | Labels | In `px`. How far a label written with `away` sits from its point. Defaults to 16. |
| `show` | Any mark | Draws it only while this is above 0, decided per frame. Can also read `view.width`, the view's width in world units, for zoom levels. |
| `trim` | Curves, lines, arrows, circles | How much of the stroke is drawn, from its start: 0 to 1 of a curve's parameter, or of a straight line's length. An arrow grows from its tail. A circle starts due east of its center and runs counterclockwise, and is drawn as a line to be trimmed. A collection of curves can trim each one with its variables. A traced curve, a region, dots and circles in a collection have no start, so they can't. |

**Panels are laid out by two rules.** Their values hold for the whole figure, so they can't read a param, and a rule for a panel
that doesn't exist is a mistake. A panel's `width` and `height` also win over its viewport's, and all panels have to be one size, for now.

| Rule | Property | Meaning |
|---|---|---|
| `style panels` | `columns` | How many columns. Defaults to 1. |
| | `gap` | In `px`, between panels. |
| | `title` | In `px`, the band above each panel for its title. |
| `style panel`, `style panel.whole` | `width`, `height` | In `px`, a panel's design size. Defaults to 400. |

**A beat's caption is drawn by one more rule,** and only when a sheet has it, centered along the bottom of the canvas. As a beat
with a new caption starts to move, the old one fades out and then the new one fades in, each over a fifth of a second, or less if
the move is shorter.

| Rule | Property | Meaning |
|---|---|---|
| `style caption` | `fill`, `size`, `halo` | As a label's. `size` defaults to 18 px. |
| | `font` | `sans` (default) or `math`. |
| | `distance` | In `px`, from the bottom of the canvas to the caption's center. Defaults to 28. |
| | `pad` | How much room a `fit` leaves around what it fits, as a fraction of its size. Defaults to 0.1. |

## Decisions

Newest first. "Why" is the reason at the time, so these can be revisited.

| Date | Decision | Why |
|---|---|---|
| 2026-11-08 | `trim` cuts a stroke where it becomes segments, and a circle with one is drawn as a line | The GPU and the CPU oracle share those segments, so they can't disagree, and the shader needs nothing new. A curve is cut along its parameter, since that's the order its math goes in, and fully drawn it's the curve as it was, so a loop's dashes don't jump as it closes. A circle keeps only its center and radius, so it can't start at the point it was drawn through yet: a compass circle can be written as a parametric curve that does |
| 2026-11-08 | Beats can have lengths, and `show`, `drop`, `focus` and captions all follow `beat` | `006-square` kept absolute times in two places, and its sheet had to know the document's own names for its fades. Lengths compose, so one change moves what follows, and narration can retime them later. Working everything out from `beat`, not seconds, means a reader stepping, a video and a pause all agree: nothing moves while the figure holds (`docs/plan-explainer.md`) |
| 2026-11-08 | A value that holds for the whole figure can't read `time`, and a state can't write a term twice | Both used to fail without a word. A viewport bound was worked out once, at time 0, so `x: -time..time` sat still, and a second `+` took the first's place, so `a + b + c` drew `a b + c` (`docs/plan-explainer.md`) |
| 2026-11-06 | A view keeps equal scales, showing more rather than stretching | Kris: distortion spends a reader's budget fast. A 0.07% stretch also moved a near-straight circle by tens of pixels, since its center and radius were scaled differently. Unequal axes would have to be asked for, and nothing asks yet |
| 2026-11-06 | Bindings run in dependency order, not the order they're written | `bind b = grown` failed at runtime, since a bound param ran at its `param` line, before a later `let`. A real cycle is now an error when compiled. A name still has to be written before it's read |
| 2026-11-06 | `fast.rs` works out a repeated piece once per sample | Inlining copies arguments, so a complex figure repeats itself many times over, and Bipolar Hearts took 18 ms a frame. It's 2.4 ms now. Setting any value starts a new generation, so nothing stale can be read |
| 2026-11-06 | A curve doesn't join two samples across a jump | Bipolar Hearts' coordinate circle goes through infinity as it turns into a line. At the deepest subdivision a chord can still be far from flat, and then it isn't the curve: it's the two sides of a pole, and drawing it flashed a false line across the figure |
| 2026-11-06 | A segment's normal is its own, except at a cap | The way from a pixel to a line was also its direction, but with the pixel center almost on the line that's only rounding error in the shader's single precision. A thin stroke's coverage depends on direction, so the GPU drew those pixels too dark, by up to 18/255 in a test made to show it, and the CPU oracle matched a 400 × 400 supersample. Squaring map's square grid had two such pixels. |
| 2026-11-06 | A point is a complex number, and a number is one on the real axis | Half of Kris's 583 Desmos graphs use complex mode, and `T(w) = 2πi·w / A` should be writable that way. It also means i turns a thing 90°, as he teaches it. No second type, and it compiles to plain numbers. The cost: a stray `i` isn't an error |
| 2026-11-06 | Panels with the same systems share their answers | They solve one question, and solving it four times was most of a q or p change |
| 2026-11-06 | A thing is declared once, and `use … through` draws it through a map | The lattice wrote every mark as `map(…)` and worked out sides again in every space, so `start` being one point was only a convention. Now it's one declaration. This replaces a `group` taking functions as arguments, which nothing needed after that |
| 2026-11-06 | A direction goes through a map by its exact derivative | A label's side is geometry in the group's coordinates, and the IR already has derivatives, so no step to tune. The lattice's differencing went away |
| 2026-11-06 | `text` gives a label's words per panel, and no text means no label | Kris: it's the same point, and the name is the space's. The spiral has one name for 0 and A, so A's label simply isn't there |
| 2026-11-06 | Where panels sit, and how far a label sits from its point, are a sheet's | A second sheet for the lattice, dark and one column, couldn't express them. The document keeps what's geometry: what a view looks at, and which way is out of the triangle |
| 2026-11-06 | The last definition of a sheet's name holds everywhere, even in rules before it | The dark sheet went last and silently did nothing. Now a theme only has to redefine its colors |
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
| 2026-11-05 | A `group` takes functions as arguments, replaced by `through` on 2026-11-06 | "A figure is a composition of functions": the same marks through a different map is what a panel usually is |
| 2026-11-05 | Expressions are lists of states, with identity in what's written | Manim's result, where what matches moves and only the rest fades, without its diff. Kris on the placement in `lab/expressions`: "beautiful" |
| 2026-11-05 | Moves use `snap`, a power of 5 | Kris wants a move to get there fast and leave time to take it in. Above 5, `za` joining `+zbi` goes by too quickly to see. The blog's is 4 |
| 2026-11-05 | Core lays out terms with widths from the renderer | The matching is the language's, and the fonts are the renderer's. It comes out as ordinary labels |
| 2026-11-05 | A step is a param, and `timeline` only one way to drive it | Same model as beats and sliders, so the arrows and the labels can bind to one number |
| 2026-11-05 | Labels move by settling springs, with a second spring on top | Kris chose it by eye in `lab/labels`: the placer jumps, a spring on the placer looks indecisive, and settling alone is wiggly |
| 2026-11-05 | Placement estimates text width from its length | It's soft anyway, and a page and a video then agree without sharing fonts |
| 2026-11-05 | A label's spring runs over earlier times in video, and over the clock on a page | A drag has no time axis, and a video's frames must not depend on order |
| 2026-11-05 | `show` uses the same per-frame flag as absent values | One way to be not there, and a hidden mark skips its shader work |
| 2026-11-05 | A construction's circle follows its own path when it needs a line | Tracing one for its dashes cost 12.7 ms a frame, for a path it already knew |
| 2026-11-05 | A circle through three points has a handedness. Going counterclockwise, its negative side is its disk,
and its curvature is positive. Going clockwise, its negative side is everything else, and its
curvature is negative. Three points in a line are a line, which runs through 0 curvature without
any value going to infinity, and has no center or radius. Two points in one place make no circle.

`which` names a root, not a place among the points left | One formula per root, so nothing jumps when a ray's start enters a circle. This replaced "a ray from inside has one point, and it's both 0 and 1" |
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
