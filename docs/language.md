# The Pencil language

What exists today. `decisions.md` has why each part is the way it is. A test in
`crates/pencil-core/tests/language_doc.rs` fails if a built-in function, style property or
statement goes undocumented here. The syntax can still change freely (`goals.md`), so treat
this as a description of the current language, not a promise.

The corpus in `corpus/` has working examples of everything below.

## Documents

A `.pen` file is a list of statements. Comments run from `//` to the end of the line. Style
rules can sit in the document or in separate style files.

**Values can be written after they're used.** A `let`, `fn`, `param`, choice or `solve` used before it's written is worked out
just before its first use, so a figure can say what it draws first and how its values are found after. What's drawn keeps the
order it's written in, which is what's on top, and so does the timeline. A curve, circle or line used as a value, such as in
`intersect(c, l, 0)`, is something drawn, so it comes first. A value defined in terms of itself, through others or not, is an
error.
A beat's name can be read before its beat is written too. Beats and pauses can't pass each other, so they stay in order, and a
beat with a block holds marks, which stay where they are, so only a beat without one can be read early.

**A figure can be more than one file.** `use "limacon/mirror.pen"` puts that file's statements where the line is, as if
they were written there. Everything is still one figure with one set of names, so a part can show, drop or move what
another part drew, and a name defined twice is an error wherever the two are.

- **The path is from the file that says it,** with `/` between folders and `..` to step out of one: `use "arms.pen"` in
  `limacon/mirror.pen` is `limacon/arms.pen`. A path can't start at a root or be a URL.
- **A file joins once.** A second `use` of it adds nothing, so two parts can each use the helpers they need.
- **A file can't use itself,** directly or through another.
- **A used file is whole statements,** so a `beat { … }` block can't open in one file and close in another. It can be a
  figure by itself too: its `doc` name is ignored when it's used.
- **A sheet can use files the same way,** for names and functions several sheets share.
- **An error says which file it's in,** with the line in that file: `limacon/mirror.pen:12:3: unknown name …`.

The core never opens a file. The command reads them from disk, and `<pencil-figure>` fetches them from beside the file
that names them. `lab/explainer/limacon.pen` is the example: a file to each part of its story.

| Statement | Meaning |
|---|---|
| `doc name` | Names the document. |
| `duration 13` | How long one pass of the time bindings takes, for looping and video. Any constant will do: `duration 2·π` loops once per turn of `time`. |
| `poster { time: 3.5, roll: 0.6 }` | The moment a still of the figure shows: a time, and params held at values, as a reader could set them. A build draws it for a page to show until the figure runs (see Posters). |
| `viewport { … }` | The world window (see below). |
| `let x = expr` | A named value: a number, point, circle or line. |
| `fn f(a, b) = expr` | A function, inlined where it's called. No recursion. |
| `param a = 1 in 0..2·π` | A root that time or a reader can drive. It stays in its range however it's driven: its default, a binding, a reader's control and a drag all stop at the range's ends. |
| `param p = 2 in 0..q step 1` | A range whose end follows other params, here one that says `p` can't pass `q`. An end is numbers and params, and nothing else: not a `let`, and not `time`. The figure keeps `p` inside the range as it stands each frame, so a reader who lowers `q` below `p` brings `p` down with it. What the reader set isn't lost: raise `q` again and `p` goes back to it. A page reads the widest the range gets and which params move it (`info.params`: `min`, `max`, `follows`), and the range as it stands (`figure.range("p")`), so a control built from them stops where the figure does. The widest is found at the ends of the ranges it reads, which is right for an end that only rises or falls with each. A range that ends before it starts leaves its start. |
| `param n = 3 in 1..12 step 1` | A param that only takes its range's start plus whole steps, however it's driven: its default, a binding, a reader or a drag goes to the nearest one. A step of 1 makes it a whole number, and a page's control steps the same way. |
| `param axis in {ω, 1, 1 + ω}` | A param that's one of these values, the first unless `= 1` names another as written. They can be any one kind, and numbers go with points as points on the real axis. The name is the chosen value, so the math can use it directly: `imag(conj(axis)·P)` is the cross product with whichever direction was chosen. The param a reader sets is `axis.index`, which option it is, from 0, and a page labels each option with its text, or with its name: in `{"p = 0": 2·π / q, "q = 2p": …}` a page shows the names and the math gets the values, and `= "q = 2p"` starts there. |
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
| `label n = "−{q}" at P away D` | Text at a point, on the side `D` points to, in the figure's own coordinates. Its style's `distance` says how far. `{expr}` in the text is a value: a number, written to the style's `digits` and whole if it's whole, or a point, written as its style's `format` says. What a label says about a value is its style's `text`. Without text, each panel gives it (see Panels). A figure without panels needs text, unless the label is a value's. |
| `label root at P` | A label named after a value is that value's label, as a dot is its point's, so one rule styles both. It's about that value, and its style's `text` says what it shows of it: with no text of its own, the value. A value has one label. A label whose text is one value, `"{q - 1}"`, is about that value in the same way. |
| `label n = "P" at P offset (12, -12)` | The same, at a fixed shift in `px` with y down. A sheet can't change it, so prefer `away`. |
| `arrow v = A, B` | A shaft and a head, as one line. |
| `dots d = P(i) for i in 1..n with z = f(i)` | One mark drawn many times (see below). |
| `circles c = circle(C(i), T(i)) for i in 1..n` | The same, as circles. |
| `curves c = parametric (t): P(t, i) for t in a..b for i in 1..n` | The same, as curves. |
| `group g(k) { … }` | Things to draw in more than one place, declared once in their own coordinates. |
| `use g(2)`, `use g(2) through f as up` | Draws a group here, optionally through a function and under a prefix (see Panels). |
| `use "parts/mirror.pen"` | Another file's statements, here (see Documents). |
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
| Parametric | `curve c = parametric (t): (cos(t), sin(t)) for t in 0..2·π` | Sampled to a line |
| Construction | `curve c = circle(O, A)`, `curve g = ray(O, B)` | A circle per pixel, a line or ray as a line |

A parametric range can run down as well as up, and the curve is drawn from its first bound, so
`for u in 0..t` sweeps an angle from 0 to `t` whichever sign `t` has. `trim` and a `head` follow that
direction. A collection's `for i in a..b` counts up, and is empty when `b` is below `a`.

### Viewport

`viewport { x: -2..2, y: -2..2 }`

- `x`, `y`: a fixed world window. It always has one scale for both axes, so at a different shape it shows more, rather than stretching.
  Its bounds are constant, so they can't read `time` or a param. To follow something that moves, `fit` it.
- `fit: a b c`: bounds these points and circles each frame instead of a fixed window, ignoring
  any param a handle has taken over, so a drag doesn't rescale the figure under the pointer.
- `pad: 0.04`: extra space around a `fit`, as a fraction of its size.
- `yaw: 0.45·roll`: how far the view is turned about its vertical axis, for points in space (see Points in space). It can
  read params and `time`, so a view can turn as a figure plays.

A viewport says what part of the plane the figure shows, and not how big it's drawn. That depends on where it's drawn, so it's
a sheet's: `style figure { width: 640px; height: 400px }` (see Styles). The same figure can be 16:9 in a video's sheet and fit a
page's column in another's.

### Posters

A still of a figure stands in for it where it can't run, or hasn't started: on a page before its first frame, in a link's
preview, in print. A figure knows which moment tells the most, and it's a binding of time and params like any other, so
the document says:

`poster { time: 3.5, roll: 0.6 }`

- **`time`** is a moment of the clock, and anything else is a param, held there as if a reader had set it. Each is a constant.
- **Without one,** the poster is the end of the story for a figure whose beats have lengths, where everything has arrived,
  and the start of the clock for any other.
- **It changes nothing about the figure as it runs.** A page that wants the figure to start where its poster is holds those
  params itself, from what the build wrote down.

`pencil build DOC.pen --style … --out DIR` draws it, at the figure's design size and at twice it, as PNG and, where it's
smaller, as WebP, beside the compiled figure
and a `.json` of what a page needs to know: the figure's shape, its duration, its params with their ranges, its beats and
captions, its poster's moment with every param's value and range there, the colors its sheets name, and every file that
went into it (`sources`), so a build knows when to make it again. `--card` adds a 1200×630 card for sharing, the figure on its own paper. The Astro component in
`packages/figure` runs this for each figure on a site, and writes the page around what it made.

### Beats

A `beat` block groups marks into a discrete step. Any `beat` block declares the param `beat`,
ranging over the beats, and its marks get the block's tags. A `self.age` in a style is that
mark's own `beat − index`, so one rule can fade every earlier beat. In a figure with a `duration`, beats
with no lengths share it evenly and `beat` follows time, so the timing is said once: ten beats in
`duration 30` are 3 s each. With no duration, a reader steps through them. `bind beat = …` can
still drive them by something else.

**A beat can say what it's about,** with or without a length: `focus P, .arms`, and `self.focus` is 1
on those during that beat and 0 on the rest, until a beat names others. An entry can follow a choice,
so the beat is about what a reader picked: `focus level, .boundary if case = "p = 0", .middle if case
= "q = 2p"`. On a beat with no length, `focus` comes after its tags: `beat "…" tag t focus … {`.

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
- **A term can have a name:** `slow:2ab` prints `2ab`, and is the term `slow`. The name is what moves between states and what a copy is written from, `slow.1:2ab`, and it's a tag, so `style .slow { … }` and `focus .slow` reach the term together with an arrow tagged `slow`. One name ties a symbol to its geometry. A name prints one text, so `slow:2ba` in another state is an error.
- When a copy goes, it moves into a term of the same name at the same place: factoring.

Each term is a mark named `work.z.1`, with the tags `term`, `work` and `z`, so `style term.a { fill: blue }`
colors every `a`. A renderer measures the text with its own font, and core says where each term goes.

## Expressions

Numbers, names, `+ - * / ^`, `·` for multiply, `²` and `³`, unary `-`, and calls. Factors side by side multiply, as `2π`, `2(x + 1)`, `(a + b)(a - b)` and `a b`. Right against
each other they bind tighter than `/`, so `1/2π` is 1/(2π). A space ends that: `1/2 π` is π/2, and `(k - 1)/N π` is the quotient times π, as it
reads in Desmos, where the fraction's bar stops. A product doesn't reach past the end of its line or a word that follows an expression,
such as `for`, `in`, `step` or `tag`. A function's name before `(` is a call. Any other name there multiplies the parentheses, so `π(t - 1)` and
`a (b + c)` are products, and a power after them is the parentheses' own: `a(b + c)²` is a·(b + c)². Letters together are one name, so `ab` is a name and `a b` is a product. The
constants' own glyphs `π`, `ℯ`, `ⅈ` and `ⅇ` never join a name, so `2πi`, `2πr` and `iπ` are products, and `π_1` is still a name. `(a, b)` is a
point, which is also a complex number (see below), and `P.x` and `P.y` read one. Names can be Unicode. A subscript is part of a name, and
`x1`, `x_1`, `x₁` and `x_{1}` are all the name `x_1`, as in Desmos and LaTeX; digits inside a name, as in `p2q`, aren't one. `time` is always available. `pi` and `π` are π, and `e`
and `ℯ` are Euler's number, unless the figure takes the name for something of its own, as bounds does with its curve `e`; then
every `e` in the figure is that, and `exp` is Euler's. In the IR they're MathJSON's `Pi` and `ExponentialE`. `2e` is 2·e, while
`2e3` is 2000. Coordinates only exist inside the curve that names them.

Points add and subtract, scale by a number, and negate. All of this expands to scalars at
compile time, so the IR stays symbolic and bindings are evaluated once per frame.

### MathJSON

Expressions compile to [MathJSON](https://mathlive.io/compute-engine/), and where Pencil has something MathJSON names, it uses
MathJSON's name, as the Compute Engine does: `imag(z)` is `Imaginary`, `conj(z)` is `Conjugate`, `arctanh` is `Artanh`. So are
the constants, which are defaults (above): a figure that takes the name gets its own value.

| Written | MathJSON | Value |
|---|---|---|
| `pi`, `π` | `Pi` | π |
| `e`, `ℯ`, `ⅇ` | `ExponentialE` | Euler's number. `e^w` is `exp(w)`, for a complex `w` too, so `e^(iπ) + 1` is 0. |
| `i`, `ⅈ` | `ImaginaryUnit` | The point (0, 1), which compiles away. |
| `GoldenRatio` | `GoldenRatio` | (1 + √5)/2. Not `φ`, which is an angle as often. |
| `EulerGamma`, `CatalanConstant`, `Degrees` | the same | γ, Catalan's constant, and π/180, so `30·Degrees` is 30° in radians. |

Where Pencil differs on purpose:
- **Names are words,** so `eps` is one name, where the Compute Engine's LaTeX reads it as e·p·s. A space splits a product: `e p s`.
  Nothing splits by what happens to be defined, as GeoGebra does, where a half-typed `ToCo` quietly becomes `T`, `o` and `C`:
  an unknown name is an error. Only the constants' glyphs split, and they're fixed.
- **A document has one scope.** The Compute Engine substitutes a constant as it parses, so a variable named `e` has to be declared
  before anything that uses it. A figure's `let e` applies to every `e` in it, wherever it's written.
- **`exp(x)` stays `Exp`.** The Compute Engine's canonical form is `["Power", "ExponentialE", x]`, which means the same, and Pencil
  turns that into `Exp`, so derivatives and complex values take one path.

### Pasting LaTeX

The playground and the AST page read LaTeX pasted from Desmos or MathLive as Pencil text (`latex.rs`), and say what it became.
Undo takes it back. Text with a backslash, a `^{`, or a leading `$` is LaTeX, and Pencil text never has them.

| Pasted | Becomes |
|---|---|
| `y=\frac{1}{2}x^{2}` | `y = 1/2·x^2` |
| `\frac{1}{2\pi}`, `-\frac{b}{2a}` | `1/(2π)`, `-b/(2a)` |
| `\exponentialE^{\imaginaryI\pi}+1` | `e^(iπ) + 1` |
| `\theta_{1}`, `x_{1}y` | `θ_1`, `x_1·y` |
| `xy`, `2\pi r` | `x·y`, `2πr` |
| `\left|z\right|`, `\sqrt{x}`, `\operatorname{mod}\left(x,2\right)` | `abs(z)`, `sqrt(x)`, `mod(x, 2)` |
| `\sin x\cos x`, `\sin^{2}x` | `sin(x)·cos(x)`, `sin(x)^2` |
| `30^{\circ}` | `30·Degrees` |

- **Letters run together are a product,** as LaTeX means them, so `xy` comes out `x·y`, not the name `xy`. A longer name has to be
  `\operatorname{…}` or `\mathrm{…}` in LaTeX, and is one name here too.
- **A letter before parentheses is a call only if it's a function:** a built-in, a `fn` in the document, or the one a line defines,
  as `f\left(x\right)=x^{2}`. Otherwise it multiplies, so `a\left(x+1\right)` is `a·(x + 1)`, as Desmos reads it.
- **Brackets go only where Pencil needs them,** with one more: a product in a denominator, `1/(2π)`, which Pencil would read the
  same way without them but a reader might not.
- **`\phi` and `\varphi` are both `φ`,** and `\epsilon` and `\varepsilon` both `ε`: the letter a keyboard types, whichever way
  LaTeX draws it.
- **Lines pasted together are one graph:** a function any line defines, as `G\left(u,v\right)=u+v`, is a call on every line. Three
  or more things in parentheses are arguments whatever the name is, since a point has two coordinates.
- **Desmos's own spellings read as what they mean:** `A.x` and `A.y` stay, `P.\operatorname{real}` is `real(P)`, `\tan^{-1}` is
  `arctan`, and `\log` is base 10 while `\log_{2}x` is `ln(x)/ln(2)`.
- **A function LaTeX has and Pencil doesn't yet keeps its name,** as `\gcd`, `\lcm`, `\det` and `\lg` do, so the figure's error names
  it. `pencil latex` writes such a line as a comment that says which.
- **What has no Pencil form is an error,** and the LaTeX pastes as written: Desmos's piecewise braces, `with` and `for`, comparisons, lists, derivatives, `±`, an
  n-th root (`x^(1/n)` differs from it for negative x), 3D points, and infinity.

### Numbers

| Function | Meaning |
|---|---|
| `sqrt(x)`, `exp(x)`, `ln(x)`, `abs(x)`, `sign(x)` | The usual. `sign(0)` is 0. |
| `log(x)`, `log(x, b)` | The logarithm to base 10, as LaTeX's `\log` and Desmos's are, or to base `b`. The natural one is `ln(x)`. |
| `floor(x)`, `ceil(x)`, `round(x)` | The whole number below, above and nearest. A half rounds away from 0, so `round(-2.5)` is −3. |
| `sin(x)`, `cos(x)`, `tan(x)`, `sec(x)`, `csc(x)`, `cot(x)` | Radians. `30·Degrees` is an angle in degrees. |
| `arcsin(x)`, `arccos(x)`, `arctan(x)`, or `asin(x)`, `acos(x)`, `atan(x)` | Their inverses, in radians. `arctan` is between −π/2 and π/2. |
| `arcsec(x)`, `arccsc(x)`, `arccot(x)` | `arccos(1/x)`, `arcsin(1/x)`, and for `arccot` the angle between 0 and π, as Desmos has it, so it has no jump at 0. |
| `atan2(y, x)` | The angle of the point `(x, y)`, between −π and π. |
| `arcsinh(x)`, `arccosh(x)`, `arccoth(x)`, or `arsinh(x)`, `arcosh(x)`, `arcoth(x)` | The inverses of the hyperbolic functions, with `arctanh(x)`, which are under Complex numbers. |
| `min(a, b)`, `max(a, b)`, `clamp(x, lo, hi)` | Limits. |
| `mod(x, y)` | Floored, so the result takes `y`'s sign. |
| `mix(a, b, t)` | `a` at 0 and `b` at 1. Also mixes two colors, in linear light. |
| `smoothstep(a, b, t)` | 0 before `a`, 1 after `b`, eased between. |
| `reveal(age, now, later)` | 0 before a beat, `now` during it, `later` after it. |
| `snap(x)`, `snap(x, power)` | 0 at or before 0, and 1 at or after 1: gets there fast, then eases in. `power` defaults to 5. |
| `timeline(time, start, end, …)` | Which step of a list of moves a time is in: whole while holding, and between two, linearly, while moving. |

### Points in space

`(x, y, z)` is a point in space. A mark draws it looking down z, which comes toward the reader, so it's drawn at its `x`
and `y`, and a figure that never writes a `z` is the same figure it was. The view is orthographic: nothing gets smaller
with distance.

- **It's a vector, and not a complex number.** It adds to and subtracts from another, scales by a number, and has `.x`,
  `.y` and `.z`. Multiplying two, or passing one to `exp`, is an error. A function can build one from a point in the
  plane: `fn rolled(w) = (w.x, r·sin(w.y), r·cos(w.y))`.
- **Any mark can take one where it takes a point:** a dot, a curve's point, a segment's ends, a label's place.
- **A viewport's `yaw` turns the view** about its vertical axis, so `z` comes round toward `x`:
  a point is drawn at `x·cos(yaw) + z·sin(yaw)`, and its `y`.
- **A sheet can read a point's `z`, as `self.z`,** along a `curve` or `curves` of points in space, in `width` and `opacity`,
  and for each of a collection of `dots`, in `radius`. It's the `z` the figure wrote, before the view turns. So a figure
  that puts a tube's axis at `z = 0` has its far side at `z < 0`, and its sheet can say
  `opacity: mix(0.25, 1, smoothstep(-0.04, 0.04, self.z))`. Where that changes along a curve, the stroke is cut there, to a
  quarter of a pixel, and not at the nearest sample, so the cut moves evenly as the figure does.
- **Nothing is hidden behind anything.** Marks are drawn in the order they're written, as in the plane. Where strokes of one
  curve or one collection overlap, the one with more ink shows, so a faded far side never dims a near side that crosses it.

`corpus/013-tube` is the example. Not built: a view that pitches or that a reader turns by dragging, `self.z` on segments,
polygons and single dots, a color that follows depth, and drawing by depth.

### Points and constructions

| Function | Meaning |
|---|---|
| `length(P)`, `dist(A, B)`, `dot(A, B)` | Numbers from points. `length(P)` is `abs(P)`, MathJSON's `Abs`. |
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
fn to_turn(w) = 2·π·i·w / A
fn spiral(t) = exp(t)
```

- **A number is a point on the real axis,** wherever a point is wanted, so `curve c = parametric (t): -t for t in 0..q`, `label n = "1" at 1`,
  `circle(0, 1)` and `1 + omega` all work, and a number answers `.x` and `.y`.
- **Arithmetic is complex:** `·` and `/` between two points multiply and divide them as complex numbers, and `+`, `-` and scaling are what they
  were. `dot(a, b)` is still the dot product.
- **Powers:** `z²` and `z^-3` multiply out, up to a power of 8. Anything else, such as `z^2.5` or `z^i`, is `exp(w·ln(z))`, on the principal branch.
- **Functions:** `real(z)`, `imag(z)`, `conj(z)`, `abs(z)` and `arg(z)` read it. `exp`, `ln` and `sqrt` take one too, with principal values.
  `ln(0)` and `arg(0)` don't exist, and a figure with one draws what it can (see below).
- **The trigonometric functions and their inverses take one too,** all of those under Numbers but `atan2`, `floor`, `ceil` and `round`, on
  the principal branches, as does `log`. A number stays a number: `arcsin(2)` has no answer, as `sqrt(-1)` has none, and
  `arcsin(2 + 0·i)` is complex. On a branch cut the value is the one a formula gives there: `arcsin(z)` is `-i·ln(i·z + sqrt(1 - z²))`.
- **Hyperbolic functions:** `sinh(x)`, `cosh(x)`, `tanh(x)`, `coth(x)`, `sech(x)`, `csch(x)` and `arctanh(x)` (or `artanh(x)`) take numbers and complex numbers. They compile to `exp` and `ln`,
  and the functions added to them compile the same way, to ones every renderer already has, so a derivative sees through each.
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
document's own rules last. There's no specificity (SPEC/15). Any property can be `none`, which
takes back what an earlier rule set, as in CSS: after `style ray { dash: 10px }`,
`style base { dash: none }` draws that one ray solid, and `stroke: none` draws no line.

**A style sheet can name values** with `let ink = rgb(28, 27, 25)`. The last sheet to define a name decides what it
means everywhere, including in rules written before it, as a CSS variable does. So `corpus/theme.style` holds a theme's
colors and label roles, and `corpus/dark.style` only redefines the colors. A name can use another defined in any order, but
not itself. A sheet can define functions too, as a document can: `fn glow(d, w) = exp(-d / w)`, then
`opacity: glow(self.dist, 6px)`. A call is inlined, its parameters hide the sheet's names of the same spelling, the last sheet
to define a function decides what it means, and a function can't call itself. A document picks a role by tag: `label n = "|z|" at M tag note`, and it never picks a number. A rule that names a mark has to find
it, but a rule by type or tag may match nothing, since a shared sheet can't know what each figure uses.

**Selectors** combine a name or type with tags: `c`, `circle`, `.aux`, `circle.blue`, and
`c::inside` for a facet. A mark's type is one of its tags: `circle`, `line`, `ray`, `curve`,
`segment`, `decoration`, `region`, `dot`, `label`, `arrow`, `term`, `dots`, `circles`, `outline`. `paper` is the background.

**Facets** are the two sides of a mark. `::negative` and `::positive` are the field's sign. On a circle, `::inside` and
`::outside` are its true disk and everything else, whichever way round it runs, and on a line, where there's no disk, they follow the sign. Where both name the same part of the plane,
`::inside` and `::outside` win. On anything else that has sides, `::inside` and `::outside` are aliases for the sign, with the negative side inside.

So a circle through three points that turns over straightens into a line and comes back with its sides' signs where they were, and its disk on the other one. Style `::negative` and the plane doesn't flash, and style `::inside` and
the color stays with the disk. A style can tell which it is from `self.curvature`.

**Values are expressions**, so they can use `time`, params and bindings. A few more names exist only here:

| Name | Meaning |
|---|---|
| `self.dist` | Distance to the mark, in `fill`, `stroke` and `fill_opacity`, on marks that have one, and negative inside. It's a true distance: an implicit curve's is measured to the curve as traced in view, so `cos(24·self.dist)` makes even bands. A parametric curve has one for its `stroke` and `opacity` when its stroke is drawn per pixel, with `draw: field`: the distance to the curve as sampled, with no sign, since such a curve has no sides. |
| `self.pixel` | How big a device pixel is in world units, wherever `self.dist` is, so that a glow can be a number of the screen's pixels across. It's what the screen can resolve, not the design: `1px` is a device pixel only at the design size on a 1× screen. |
| `self.curvature` | A circle's signed curvature: above 0 counterclockwise, below 0 clockwise, and 0 on a line. |
| `self.t` | Position along a stroke, 0 to 1, in `width`, and in a curve's `opacity`. |
| `self.z` | A point's third coordinate as the figure wrote it: along a curve of points in space, in `width` and `opacity`, and for each dot of a collection of them, in `radius`. See Points in space. |
| `self.age` | See Beats. |
| `self.life` | How present the mark is, from 0 before a beat shows it to 1, and back to 0 as one drops it, moving with those beats. 1 for a mark no beat names. |
| `self.focus` | How much the beats are about the mark, from 0 to 1, changing as a beat that names something moves. 1 until a beat names anything. |

**A length is in one of three units,** and they're all lengths, so they mix: `width: 1px + 0.01`, and `self.dist / 1px` is
a distance in design pixels.

| Unit | Is | Scales with |
|---|---|---|
| a bare number | The figure's own units, as in the document. `width: 0.02` is that wide in the plane, so it grows as a `fit` zooms in. | The math |
| `px` | A design pixel, as SVG's `px` is inside a `viewBox`. `style figure { width }` says how many there are across. | The figure as it's drawn |
| `em` | The reader's text size. On a page, the page's font size where the figure is, so text matches the text around it at any size the figure is drawn. A reader who changes their text size changes it, and the labels follow at once, so a page sets its text in `rem`, not `px`. In a video, `style figure { em }`, 17 px by default. | The reader's text |

Text goes in `em`, as the theme's does, and strokes, dots and arrows in `px`. A figure's own size and a panel's are design
pixels, so they can't be in `em`.

**Colors can be scaled and added:** `white·4`, `mix(a, b, t) + c / 3`. They aren't clipped until the end, and `rgb` can go past 255. `sinh`, `cosh` and `tanh` also work in a style. So a glow reads
`fill_opacity: tanh(2·self.pixel / abs(self.dist))`.

| Property | Applies to | Meaning |
|---|---|---|
| `fill` | Regions, dots, labels, facets | A color. A field's `fill` needs a facet. |
| `fill_opacity` | Anything filled | Scales only the fill, on top of `opacity`. |
| `stroke` | Curves, lines, regions, dots | A color. |
| `width` | Strokes | A length. Can vary along the stroke with `self.t`, and with `self.z` on a curve of points in space. |
| `head`, `head_span` | Curves | With `head: 13px`, a curve ends in an arrowhead. `head_span: 1` holds it to 60% of the last 1 of the curve's parameter, so it can't dwarf a spiral's tight turns. |
| `dash`, `gap` | Lines | Lengths. A loop gets a whole number of dashes, stretched to fit. `dash: 0`, like `dash: none`, is solid. |
| `cap` | Lines | `butt` (default) or `round`. |
| `draw` | Strokes | A hint: `auto` (default), `line` or `field`. `field` draws an implicit or a parametric curve's stroke per pixel from its distance, so a parametric curve's `stroke` and `opacity` can fall off with `self.dist`: `draw: field; width: 8; stroke: white; opacity: tanh(2·self.pixel / abs(self.dist))` is a line of light. A stroke drawn that way has no dashes, taper or `trim`. |
| `blend` | Edges, sides | `gamma` (default, as SVG does), `linear`, or `add`, which adds to what's there in display values and never covers it. |
| `tone` | `paper` | `clip` (default) or `tanh`, which runs the finished picture through `tanh` in display values, so that what `blend: add` piles up stays in range. Or a number from 0 to 1 between them, which can change, so a figure can turn into one that adds light: `tone: glow` with a beat named `glow`. |
| `opacity` | Any mark | Scales the whole mark. On a `curve` or `curves` it can vary along the stroke, with `self.t` and `self.z`: each part of the stroke has that much ink, and where parts overlap the most ink shows. |
| `ticks`, `tick` | Segments | How many short marks cross its middle, saying it's as long as another with as many, and how long each is, 10 px by default. Drawn in its stroke without its dashes, once a trimmed segment reaches its middle. |
| `leader`, `leader_gap` | Labels | A line from its point to its text, this wide, in the text's color. It starts `leader_gap` from the point, so it can meet a ring, and stops a little short of the text, wherever the label settles. |
| `radius` | Dots, circles, outlines | A length. A dot's size, a cap on a collection's circles, or an outline's least half-size. |
| `size` | Labels | A length, usually in `em`. Defaults to 1 em. |
| `halo` | Labels | Its full width, in the paper's color. |
| `font` | Labels | A role: `sans` (default) or `math`. |
| `place` | Labels | `fixed` (default) at its `offset`, or `settle`, tied there by a spring and pushed off what's drawn and other labels. |
| `format` | Labels | How a point in its text is written: `point` (default) as `(−0.50, −0.87)`, `complex` as `−0.50 − 0.87i`, or `polar` as `e^(−2πi/3)`, with an angle that's a fraction of π up to twelfths written as one. A point and a complex number are one thing in Pencil, so which it reads as is the sheet's. |
| `text` | Labels | What a label says about the value it's about, as parts joined by `=`: `caption` (its own words, or the value's name if it has none), `name`, `expr` (the expression as written: a `let`'s right side, or what's between the braces) and `value`. `text: expr = value` is `σ(2) = −0.50 − 0.87i`, `text: name = value` is `root = −0.50 − 0.87i`, and `text: caption` is the words alone. Parts that come out the same are said once, so a param's `expr = value` is `q = 3`. Without it, a label says its own words, or its value if it has none. A label that isn't about one value says its words whatever this is. |
| `digits` | Labels | How many decimal places its values are written to. Defaults to 2. A whole number is written whole, and a part that rounds to zero is left out of a complex number. |
| `head` | Arrows | The head's length. Defaults to 14 px. |
| `distance` | Labels | How far a label written with `away` sits from its point. Defaults to 16/17 em. |
| `show` | Any mark | Draws it only while this is above 0, decided per frame. Can also read `view.width`, the view's width in world units, for zoom levels. |
| `trim` | Curves, lines, arrows, circles | How much of the stroke is drawn, from its start: 0 to 1 of a curve's parameter, or of a straight line's length. An arrow grows from its tail. A circle starts due east of its center and runs counterclockwise, and is drawn as a line to be trimmed. A collection of curves can trim each one with its variables. A traced curve, a region, dots and circles in a collection have no start, so they can't. |

**A figure's size is a sheet's, and so is how panels are laid out.** These values hold for the whole figure, so they can't read a
param, and a rule for a panel that doesn't exist is a mistake. All panels have to be one size, for now.

| Rule | Property | Meaning |
|---|---|---|
| `style figure` | `width`, `height` | In `px`, a figure's design size, for one without panels. With `width`, `px` sizes scale with the output, as an SVG's do with its `viewBox`, and without it a `px` is a CSS pixel. With both, they give its shape, and without `height` the shape is its viewport's. A page sizes the figure's canvas to its column, keeping that shape. |
| | `em` | In `px`, how big an `em` is where there's no page to say, as in a video. Defaults to 17. |
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
| | `distance` | From the bottom of the canvas to the caption's center. Defaults to 28 px. |
| | `pad` | How much room a `fit` leaves around what it fits, as a fraction of its size. Defaults to 0.1. |
