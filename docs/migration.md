# Moving the site's figures to Pencil

This is where the move is tracked: the order to work in, what's done, and under that every figure source in the site's posts,
what each one is as mathematics, what a reader does with it, what Pencil already has for it, and what it lacks. `goals.md` holds
what's open in the language and the host in general, and points here for the site.

The inventory was written on 2026-11-14 by a session that read the site's components against Pencil as of `e62e58e`. The order
and the log below are kept by hand since. A figure's entry is updated when it's ported or when what it lacks is built, and
nothing else in the inventory has been reread.

## Order

Set with Kris on 2026-11-14. **The Doyle series goes first:** it's the furthest along and likely the first to be
published. Its four posts are worked in the series' order: `doyle-spiral-shader`, `doyle-spiral-period`, `doyle-spiral-touch`,
`doyle-spiral-solve`.

**The shader post's three GLSL figures stay GLSL:** `doyle.glsl` (`DoyleSpiral`, the hero), `gradient.glsl` and
`circle-distance.glsl`. The post is about writing a shader, so the shader is the subject and not a way to draw one. They keep the
site's `Shader.astro`, beside Pencil figures on the same page. That takes two features off the list, since nothing else in the
posts needs them: a fill that reads the pixel's position, and a field that names its nearest circle.

A post doesn't have to move all at once. A figure that isn't ported keeps drawing through the site's own system, so each step
below can ship when it's done.

0. **The host, before any post:** the element as a package and the Astro component (`goals.md`, The host). Tried on
   `pencil-lang.dev`, then on a figure that's already done. `SquareZ` in the shader post is the smallest, and `LatticeMapping`
   and `RollTube` in the period post are the ones with controls.

   **Built 2026-11-15:** `@pencil-lang/figure` in `packages/figure`, with `<Figure>` for Astro, and `pencil-lang.dev` draws
   through it. Left: the blog installs it and one done figure goes through it. The blog is on the same Astro, 7.
1. **Keyboard use, with the host.** Kris: accessibility matters, so a ported figure keeps the keys it has on the site, and none
   is ported without them. Twelve figures have them (see "Keyboard, hover and linked panels").
   1. Each handle is something a reader can tab to, named for its point, with a visible focus.
   2. Arrow keys move the focused handle, a small step, and a larger one with Shift, as the site does. The core takes the nudge
      through the handle's own inverse, which `drag_value` already does for a pointer, and keeps the param in its range.
   3. ← and → step beats.

   **Built 2026-11-14** in the element, and tried on `004-bounds`, which has both a handle and steps (`goals.md`, The
   element). Left: a screen reader hasn't been tried on it.
2. **`doyle-spiral-shader`.** Of its 12 sources, 3 stay GLSL and `SquareZ` is done. The other 8:
   1. Ports with what exists, done on 2026-11-15: `ScaledCopy` (`014-scaled-copy`), `LogNearest` (`015-log-nearest`) and
      `UnwrapMorph` (`016-unwrap`), whose one circle per period turned out to be a range.
   2. A color per instance in `curves` and `dots`: `ArmColors`, which the period post places too.
   3. Dragging a point in two dimensions (see below for its shape): `FloorCell`, as two figures.
   4. Panels of different sizes, which with the drag ports `FollowPixel` and `PowerViews`.
   5. `ZoomLoop`, which needs design: a clock that doesn't loop, and a chart whose axis rescales. Its spiral half is a port.
3. **`doyle-spiral-period`.** `LatticeMapping` and `RollTube` are done, and `ArmColors` comes from step 2. That leaves one port,
   `CameraLog`. It's the first post that can be all Pencil, and it can be done before the shader post's later steps.
4. **`doyle-spiral-touch`.** `BoundsConstruction` is done, and `ScaledCopy` comes from step 2.
   1. Port: `LimaconTangency`.
   2. Newton's steps as values: `DoyleFlower`.
   3. A view that pitches and that the reader turns: `LevelCurve`. Turning it needs keys too.
   4. `SkewSliders`, which needs design: a button that runs a step and writes params. It needs Newton's steps too.
5. **`doyle-spiral-solve`.** Nothing is done and nothing is a port.
   1. `NewtonLine`, once unequal axis scales are settled one way or the other.
   2. `DigitsChart`: Newton's steps from step 4, scaling y in the document, and without hover until that's decided. What hover
      shows has to be reachable by keys.
   3. `NewtonDoyle` last, since it needs Newton's steps, the turning view, panels of different sizes and moves between views.
6. **After the series,** by what's closest:
   - `limacon`: `MedianConstruction` and `TriangleLayers` are ports. `LimaconConstruction` needs beats per panel, or linked
     elements.
   - `circumcenter`: `circumcircle.glsl` is a port by extending `011-circumcircle`, which was written from it and is close.
     `CircumCot` is a port. `CircumcircleDemo` and `CircumNormals` are ports once step 2's drag exists.
   - `involute`: `Unroll` is done, so the post only needs the host.

**Not being ported:** the shader post's three GLSL figures, and the home page's hero (`hero.glsl`), which Kris wouldn't bother
with either. The logo can stay the site's own.

**How a point is dragged in two dimensions.** From Kris, on what Desmos does and how it feels:

- **A point written from two params is dragged by setting them.** Desmos allows this only for a point that is `(a, b)` or
  `a + b·i`, and that's the rule to take: the point's coordinates are the params, so there's nothing to invert, and each param
  stops at its own range. `FloorCell`'s free point, `CircumcircleDemo`'s and `CircumNormals`' corners, and `FollowPixel`'s w are
  all this.
- **Anything else keeps an explicit inverse,** as `handle P sets a = …` has now, extended to give a point for two params:
  `FollowPixel`'s z in the spiral sets w through the inverse map, and `PowerViews`' z is dragged in two panels.
- **No inverse is solved for.** Desmos also drags a point that's a function of one param by solving for it, which is convenient,
  and has two faults Kris has met: the param can jump out of its range, and when several handles share a param, some one-to-one
  and some many-to-one, dragging a many-to-one handle makes the others jump between its solutions. The explicit conversion is
  easier to understand. So "automatic inverses" (`decisions.md`, Not built from the specs) stays not built.

**A field, and what a style can read of it.** The inventory asked for "a field that names its nearest circle" so that
`DoyleSpiral`'s arms could be colored. Kris's point: that shader isn't a list of circles drawn fast. It samples a pixel and asks
which circle of infinitely many is nearest, and nothing in it could be traced, so it doesn't reconcile with styling a mark by its
instances. Nothing in the posts needs this now. If something does, the direction is that what a style can read of a mark depends
on the mark's form, as it already does in practice: a field offers `self.dist`, a curve `self.t`, a curve in space `self.z`, a
dot `self.x` and `self.y`. A field would offer the pixel's position and values it computes there, such as which cell it's in,
and a form that can't offer a name says so. That's a set of names per form, and not every form has every one.

**Done so far**

| Date | What | For |
|---|---|---|
| 2026-11-05 | `corpus/003-unroll`, `004-bounds` | `Unroll`, `BoundsConstruction` |
| 2026-11-06 | `corpus/006-square`, `007-lattice` | `SquareZ`, `LatticeMapping` |
| 2026-11-14 | Points in space, a viewport's `yaw`, `self.z` along a curve, and `corpus/013-tube` | `RollTube` |
| 2026-11-14 | A range that follows other params: `param p = 2 in 0..q step 1` | The p ≤ q of `LatticeMapping`, `ArmColors`, `DoyleFlower` |
| 2026-11-14 | `pencil build`, posters, and the element's poster and `<source>` children | Every figure (`goals.md`, The host) |
| 2026-11-14 | Handles a reader can tab to, arrow keys that move them, and ← → for beats | The 12 figures with keys on the site |
| 2026-11-15 | `corpus/014-scaled-copy` | `ScaledCopy` |
| 2026-11-15 | `corpus/015-log-nearest` | `LogNearest` |
| 2026-11-15 | `corpus/016-unwrap`, and one circle per period as a range | `UnwrapMorph` |

**How it was checked.** Every component was read, with the libraries it draws through (`plot.ts`, `construction.ts`, `wire3d.ts`,
`doyle.ts`, `Shader.astro`). On the Pencil side, every "lacks" claim below was checked against `language.md` and the code, and the
ones marked *(tried)* were written as small `.pen` files outside the repo and run through `pencil ir` or `pencil render`. The
error a probe gave is quoted where it's the evidence.

**Counting.** The posts place 33 figures from 31 sources: `ScaledCopy` is in two posts, and so is `ArmColors`, with different props.
The home page's hero canvas and the header's logo are listed apart, at the end. Tiers are the site's `size` prop (`hero`, `primary`,
`supporting`), and `aside` for a figure floated beside the text in a `SideFigure`.

**Sizes.**

- **Done:** a Pencil figure is already this figure.
- **Port:** everything it needs exists today.
- **Feature:** a port once one named feature exists (or two, where said).
- **Design:** needs a decision about the language or the model before it can be built.
- **Stays:** stays the site's own shader, by Kris's choice (see Order).

## The whole list

| Component | Post | Tier | Pencil today | Lacks | Size |
|---|---|---|---|---|---|
| `CircumcircleDemo` | circumcenter | supporting | Related: `011-circumcircle` | Dragging a point in two dimensions | Feature |
| `circumcircle.glsl` | circumcenter | supporting | Part: `011-circumcircle` | Nothing, without its hue drift | Port |
| `CircumNormals` | circumcenter | supporting | None | Dragging a point in two dimensions | Feature |
| `CircumCot` | circumcenter | supporting | None | Nothing | Port |
| `Unroll` | involute | primary | Same: `003-unroll` | Nothing | Done |
| `LimaconConstruction` | limacon | primary | Related: `lab/explainer` | Separate beats per panel | Design |
| `TriangleLayers` | limacon | primary | None | Nothing | Port |
| `MedianConstruction` | limacon | primary | Related: `lab/explainer/limacon/pedal.pen` | Nothing | Port |
| `DoyleSpiral` (`doyle.glsl`) | doyle-spiral-shader | hero | Related: `007-lattice`'s spiral | Not being ported. It would lack coloring a field by which circle is nearest | Stays |
| `ScaledCopy` | doyle-spiral-shader, doyle-spiral-touch | aside | Same: `014-scaled-copy` | Nothing | Done |
| `FollowPixel` | doyle-spiral-shader | primary | Related: `007-lattice` | Two-dimensional drag, panels of different sizes | Feature (two) |
| `gradient.glsl` | doyle-spiral-shader | aside | None | Not being ported. It would lack a fill that reads the pixel's position | Stays |
| `circle-distance.glsl` | doyle-spiral-shader | supporting | Related: `001-circle` with `bands.style` | Not being ported. It would lack a fill that reads the pixel's position | Stays |
| `SquareZ` | doyle-spiral-shader | aside | Same: `006-square` | Nothing | Done |
| `PowerViews` | doyle-spiral-shader | primary | Related: `010-mobius` | Two-dimensional drag, panels of different sizes | Feature (two) |
| `UnwrapMorph` | doyle-spiral-shader | primary | Same: `016-unwrap` | Nothing | Done |
| `LogNearest` | doyle-spiral-shader | supporting | Same: `015-log-nearest` | Nothing | Done |
| `FloorCell` | doyle-spiral-shader | supporting | None | Dragging a point in two dimensions | Feature |
| `ArmColors` | doyle-spiral-shader, doyle-spiral-period | primary | Related: `007-lattice` | A color per instance in `curves` and `dots` | Feature |
| `ZoomLoop` | doyle-spiral-shader | primary | None | A clock that doesn't loop, and a chart | Design |
| `LatticeMapping` | doyle-spiral-period | primary | Same: `007-lattice` | Nothing | Done |
| `RollTube` | doyle-spiral-period | primary | Same: `013-tube` | Nothing | Done |
| `CameraLog` | doyle-spiral-period | primary | Related: `007-lattice` | Nothing | Port |
| `LimaconTangency` | doyle-spiral-touch | primary | None | Nothing | Port |
| `LevelCurve` | doyle-spiral-touch | supporting | None | 3D: a view the reader turns | Design |
| `DoyleFlower` | doyle-spiral-touch | primary | Related: `007-lattice`'s `solve`; `lab/zoom` (not Pencil) | Newton's steps as values | Feature |
| `SkewSliders` | doyle-spiral-touch | primary | Related: `007-lattice` | A button that runs a step and writes params | Design |
| `BoundsConstruction` | doyle-spiral-touch | primary | Same: `004-bounds` | Nothing | Done |
| `NewtonLine` | doyle-spiral-solve | aside | None | Unequal axis scales | Feature |
| `NewtonDoyle` | doyle-spiral-solve | primary | Related: `lab/zoom` (not Pencil) | Newton's steps, 3D, moves between views, panel sizes | Design |
| `DigitsChart` | doyle-spiral-solve | supporting | None | Newton's steps as values, and hover | Feature (two) |
| Hero canvas (`hero.glsl`) | home page | — | None | Field coloring, pointer as input, a page overlay | Design |
| Logo (`FlowerMark`) | every page | — | None | Output as SVG | Design |

Keyboard use isn't in this table, because no figure needs it to be ported, but twelve of them have it on the site and would lose
it. See "Keyboard, hover and linked panels".

## What every port needs from the host

The site wraps each figure in HTML that Pencil doesn't own, and `<pencil-figure>` doesn't build any of it yet:

- **Controls from params.** Sliders and choices are built from a figure's params in the playground (`web/playground.js`) and by
  `pencil-lang.dev`'s `<figure-controls>` (`site/src/scripts/figure-controls.js`), not in the element. Every interactive figure
  here has them. A slider whose maximum follows another (p ≤ q in `ArmColors`, `DoyleSpiral`, `DoyleFlower`, `LatticeMapping`)
  is said in the document since 2026-11-14: `param p = 2 in 0..q step 1`, which the figure enforces and a page reads through
  `info.params` and `figure.range` (`language.md`, Documents). Five layer checkboxes (`TriangleLayers`) are choices
  `{"off": 0, "on": 1}` read by `show` *(tried)*, so a host only needs to draw a two-option choice as a checkbox.
- **Readouts.** Most figures have a one-line monospace readout of computed values. The element's `value` reads any binding, so the
  numbers are there (`004-bounds` keeps `ring_count` for this). The formatting is the page's: fixed decimals with a sign column,
  exponent notation with superscript exponents (`DigitsChart`, `NewtonDoyle`, `DoyleFlower`), and yes/no text (`CameraLog`).
- **Captions and step counts.** Beat captions and `i / n` exist in the core. `BoundsConstruction`'s port already has them.
- **Legends, colored identities and panel titles with formulas.** `LatticeMapping`'s legend and live identity, `Unroll`'s colored
  sentence, `NewtonDoyle`'s axis names over the plot. A panel's title and formula are Pencil's; the rest stays page HTML, using the
  element's `colors`.
- **Thumbnails.** `VisualToc` clones a plot's SVG or captures a shader's frame for its cards. A figure would need a capture from the
  element's canvas.
- **Tap to pause, pausing offscreen, a still frame for reduced motion.** The element has all three already.

## The five shaders

**Three of the five stay GLSL** (see Order): `gradient.glsl`, `circle-distance.glsl` and `doyle.glsl`, which are the shader post's
subject. What's below about them is what a port would have taken, kept in case that changes. `circumcircle.glsl` is still a
port, and `hero.glsl` was always the site's.

Pencil draws fields and styles them with expressions, and `011-circumcircle` and `012-limacon` are ports of shaders like these. What
it can't do, checked for all five:

- A style value can't read where the pixel is. `fill: rgb(255·x, 0, 0)` is "unknown name `x`", on a facet and on `paper` alike, and
  `paper` can't read `self.dist` either *(tried)*.
- A stroke can't change color along its length: `stroke: hue(self.t)` is "unknown name `self.t`", which only `width` has *(tried)*.
- A segment and a polygon have no sides ("`s` has no sides"), and a region's `::outside` takes no `fill_opacity` *(tried)*, so a
  glow around a straight line has to be its distance written as an `sdf` curve. That works.

**`gradient.glsl`: color = (x, y, 1).** The point of the figure is position as color, so it isn't decoration. Not expressible
honestly. Two half-plane `sdf` curves, `x − 2` and `y − 2`, with `blend: add` and fills from `self.dist`, render the same picture
*(tried)*, but that reads a coordinate as a distance. **Needs:** a fill that reads the pixel's position, for instance `self.x` and
`self.y` in fills as `dots` already have in sizes, or a mark that is the whole plane.

**`circle-distance.glsl`: d = |z| − r, used three ways.** Blue set by the sign of d, a wave `0.05·cos(200(d − 0.05t))`, and a Moffat glow
`h / (h⁴ + 15d⁴)^¼`, added and run through `tanh`. Sliders for r and h. All of the circle is expressible today: `circle(0, r)` with
`::inside` and `::outside` fills from `self.dist` and `time`, `blend: add`, and `paper { tone: tanh }`, as `012-limacon` does, with
params r and h. The wave and glow are the subject (the uses of a distance), not decoration. Only the gradient under it, carried over
from the figure before, needs the same position-in-a-fill feature. Without it, it's a port on a plain background.

**`circumcircle.glsl`: the circumcircle of three points chasing each other along a Lissajous curve.** `011-circumcircle` is a part of
it, restyled: the curve, the triangle, the corners and the circle, with both sides lit additively so the plane doesn't flash as the
circle turns through a line. Still to add, all expressible today:

- The params: speed, glow, x and y frequencies (`step 1`), phases, the offsets of B and C, and the two show toggles as choices.
  Speed is better as the element's `playbackRate` than as a param, since a `duration` can't read a param and the loop is 2π/0.35 s.
- The centroid, its path (a parametric curve of the summed phasors), and dots at the centroid and the circumcenter.
- The triangle tinted by the sign of D = det(B − A, C − A): a region fill from `sign(D)`.
- A glow on the edges: each edge's distance as an `sdf` curve.

**Decoration:** the hue drifting around the circle with angle and time. It has no meaning, and it's the one thing that can't be drawn.

**`doyle.glsl` (`DoyleSpiral`, the shader post's hero): the Doyle spiral, every pixel mapped back to the lattice.** Its parts:

- **The shape is expressible today.** The pixel's lattice point is w = A·(ln|z| + i·arg z)/(2πi). `floor` picks its cell, and the
  distance is the least of its corners' circle distances. Written as one `sdf` curve with real `ln`, `atan2` and `floor`, it renders
  the spiral at q = 7, p = 2 without skew *(tried)*. Complex `ln` isn't allowed in a field ("a curve's field can't use `intersect`,
  `line`, `ray`, `exists` or `otherwise` directly"), since `ln(0)` doesn't exist, so `ln(sqrt(x² + y²))` and `atan2(y, x)` stand in.
- **The skew** would come from `solve`, as in `007-lattice`. Not tried inside a field.
- **The zoom loop** divides z by c(t·q·(1 + ω)), which is a function of `time`. Fields read time.
- **The arm colors are what's missing.** The color is the hue of (row mod n)/n for the nearest circle, and a field can't hand a style
  the index of what it's nearest to. `mod` can't be in a field either ("can't differentiate `Mod` yet") *(tried)*.
- **The other route is a `circles` collection.** It can color each circle's `::inside` by its own variables *(tried)*. But sides are
  capped at 256 a collection (300 gives "`cc` has 300 circles with sides, and the limit is 256" *(tried)*), and they cost about
  0.09 ms a circle. A rough count from the lattice's area in log space puts q = 24 at about 400 circles above half a pixel on a
  page-sized canvas, so it's over the cap and about 35 ms a frame.
- **Needs:** a field that names which circle a pixel is nearest, which a style can color, or instanced sides with culling, which
  `goals.md` already names as the way past the cap.
- **Decoration:** `0.6·tanh(−d·px) + 0.4` darkens each circle toward its rim, and has no meaning.

**`hero.glsl` (home page):**

- **The math:** an unskewed Doyle spiral (q = 25, p = 9), hue by lattice axis, and the same zoom loop.
- **Everything else is decoration with no mathematical meaning:** an embossed relief lit from the pointer (`dFdx` of a height field),
  a fBm noise texture on the fills, a pool of light, and radial and size fades.
- **It also composites** premultiplied over the page's paper, under CSS masks, with parameter sets per theme.
- **Pencil lacks:**
  - the pointer as an input that isn't a drag (`pointer` exists only inside `handle`)
  - screen-space derivatives and noise
  - a transparent overlay
  - the field index above
- **It's a page background, not a figure,** and probably stays a shader the site owns. Pencil could at most supply the spiral's field.

## `DigitsChart`

**A bar chart, computed live.** For the spiral a radio picks ((1,3), (2,7) or (9,25)), Newton's method runs from (0, 1). Each step's
bar is its correct digits, −log₁₀|(F, G)|, capped at 16. It also has:

- guides at 4, 8 and 12, a dashed limit at 16
- a value label on each bar, and "step i" under it
- series that stop at convergence
- hover: the bar under the pointer goes solid, and the readout gives that step's error in exponent notation.

**As marks, nearly all of it exists.**

- A bar is a `region` of four points with a computed height.
- `{digits}` labels its value.
- The guides are segments.
- A bar past convergence hides with `show: e − 1e-14`, which compiles *(tried)*.
- Bindings evaluate in f64 on the CPU, so 16 digits are representable. The GPU only draws.

**What's missing:**

- **Newton's iterates as values.** `solve` gives only the converged answer. A step needs F and G's partial derivatives, which Pencil
  has symbolically but can't name: `D(f)(1)` and `derivative(f(x), x)` are both unknown *(tried)*. The same gap blocks
  `DoyleFlower` and `NewtonDoyle`.
- **Hover.** `:hover` doesn't parse *(tried)*, and nothing gives a frame the pointer's position.
- **Unequal scales:** 7 categories by 0–18 digits at 2:1. A view always has equal scales (decision of 2026-11-06), so the document
  would scale its own y, which for a chart is harmless.
- **Charts in general.** PLAN's charts aren't built, but this one doesn't need them: it is marks over computed values.

## The three 3D figures

None of them uses depth sorting, hidden-line removal or lighting. All three are orthographic, and what each needs beyond a dropped
coordinate:

| Figure | Surface and marks | Reader turns it | Translucent polygons | Grid lines on a surface | Clipping | Drawn over what |
|---|---|---|---|---|---|---|
| `LevelCurve` | h = \|z − 1\| / (\|z\| + 1) over a square around 1: a 17 × 17 mesh, the floor, the level curve (the limaçon) at height 0 dashed and at k, a plane at k | Drag anywhere: yaw free, pitch clamped to 0.1–1.45 | The plane at k, fill 0.22 | Mesh lines in both directions, through 1 so the cone's tip is on them | Points off the square break the line | Fixed order: plane over mesh, level curve over both, whatever the view. A label sits at whichever plane corner is rightmost on screen |
| `RollTube` | A strip of log space bent onto a cylinder by a roll slider (0 to 1), rotated by a turn slider, rows along ω colored by row mod p, lattice dots, the period A as a dashed arrow | Sliders only, no drag | None | The rows are lines on the surface, clipped to the strip | Rows clipped to the strip by parameter | Each line is split per sample into front and back, back drawn faint (opacity 0.25) first, front strong. Dots only in front |
| `NewtonDoyle` | Two height fields, F and G over (μ, ν), as wireframes with the zero contour on the floor, the tangent plane at the current step, a 3 × 2 grid on that plane, its zero line, a normal arrow | Drag, with both views' yaw and pitch linked | The tangent plane, fill 0.16 | Wireframe lines at power-of-ten levels that fade with zoom | The tangent plane clipped to \|z\| ≤ 1.05 (Sutherland–Hodgman), wireframe broken past it | Fixed order |

What Pencil has toward these: a `fn` can project, and `curves` collections can draw a mesh, so `LevelCurve`'s marks could be written
today as 2D curves of projected points, with yaw and pitch as params on sliders. What it lacks:

- **A view the reader turns:** a drag that isn't on a mark and sets two params. A handle is on a mark and sets one (`handle P sets
  a = …`, with no second `sets`, and a `grab` holds one handle).
- **Style that changes along a line** with depth (`RollTube`'s front and back). Built: a curve's `width` and `opacity` read
  `self.z`, and the stroke is cut where they change (`language.md`, Points in space).
- **Linked views:** two panels share params already, so `NewtonDoyle`'s linked yaw and pitch would follow from the first item.
- **A polygon clipped to a box** in 3D. In 2D after projection, a region can be clipped by writing its corners, but not generally.

A 3D form (points with three coordinates and a camera in the viewport) replaces the hand projection. Built: `(x, y, z)`, drawn
looking down z, with a viewport's `yaw`. Still to design for the other two: a pitch, with z up as `wire3d.ts` has it, and a view
the reader turns.

## Keyboard, hover and linked panels

**Keyboard.** The site gives every draggable point arrow keys (a tenth of a grid cell, a whole one with Shift) and every
step-through construction ← and →. It comes with the host, before the posts (see Order). These twelve figures have it:

- **Handles:** `CircumcircleDemo`, `CircumNormals`, `CircumCot`, `TriangleLayers`, `MedianConstruction`, `LimaconConstruction`,
  `FollowPixel`, `PowerViews`, `LogNearest`, `FloorCell`, `LimaconTangency`, `BoundsConstruction`.
- **Steps:** `BoundsConstruction`, `MedianConstruction`, `LimaconConstruction`.

`key` isn't a statement *(tried)*, and doesn't need to be: since 2026-11-14 the element does both (`goals.md`, "The element").
It steps beats on ← and →, and each handle is a tab stop that the arrows move, through the handle's inverse at the point's
place plus a step (`frame::nudge_value`). The site's step is a tenth of a grid cell, and the element's is a hundredth of the
figure's shorter side, since a figure has no grid of its own.

**Hover.**

- `DigitsChart` highlights a bar and changes its readout, and the hero lights from the pointer.
- Every handle's ring also grows on hover and focus, which is cosmetic and the element could do.
- `:hover` isn't built (SPEC/15), and a frame has no pointer input outside a drag.

**Linked panels.** No figure points at a thing in one panel to show it in another. What they link instead:

- **A shared param:** a point dragged in one panel moves its image in the others. `FollowPixel` (w on the grid, z in the spiral)
  and `PowerViews` (z in two panels) do this, and `LimaconConstruction` does it across four panels, each with its own driver handle.
  A handle inside a panel compiles *(tried)*, and panels share params, so this is built, except that their points are free in two
  dimensions.
- **3D rotation:** `NewtonDoyle` links it across its two views.

So "pointing at a thing in one panel and seeing it in the others" (`goals.md`, Not built) unblocks no figure on the site yet.

**Separate beats per panel.** `LimaconConstruction` is four constructions in a 2 × 2 grid, sharing φ and t, each stepped on its own
(8, 6, 5 and 8 beats), with one caption that follows whichever panel moved last. A figure has one `beat`. There are two ways:

- beats per panel, which is a language change
- four `<pencil-figure>` elements that the page links through their `input` events and `value`

## Labels

**No figure sets a label with KaTeX.** KaTeX on the site is in the prose and callouts only. Every label in every figure is plain SVG
text in one of four roles (name, note, tick, display), with a halo, and Unicode for the mathematics:

- subscripts: x₁, z₂, vₖ
- superscripts: z², z⁻¹, eˢ, e⁻²ˢ, eⁱᵗ, 2⁶⁴
- primes and accents: A′, 1/z̄ with a combining macron, |z|, ω, √2

A facet's formula is HTML in italics. Pencil's labels, themes' roles and panel formulas already cover this. What's left:

- **Glyphs.** `goals.md` notes the math font has ᵗ but not ⁱ. Not checked: ˢ, ⁻ and the combining macron in the video font.
- **Text that changes with the figure.** `CircumCot`'s angle reads "C" or "180° − C", and its cotangent "cot C" or "−cot C",
  depending on the side of the chord. A label's text can't choose. Two labels with `show` in the sheet do it.
- **Labels in a collection:**
  - `PowerViews`' "+1 turn", "−2 turns" lines and `ZoomLoop`'s minute ticks vary in number. `labels` isn't a statement *(tried)*.
  - Where the count is fixed, writing each label works: axis numbers in `CircumcircleDemo` and `CircumNormals`.
- **Exponent notation** in readouts is the page's. A label's `digits` writes fixed decimals only.
- **Fixed screen size for right-angle marks** (`TriangleLayers`) and angle arcs at a pixel radius (`DoyleFlower`). `rightangle`'s
  `size` is in the figure's units, in the document.

## Each figure

What the summary table compresses. "Reader" is everything a reader can do besides tap to pause.

### circumcenter

**`CircumcircleDemo`** (supporting)

- **Draws:** triangle ABC, the perpendicular bisectors of AB and AC, the circumcircle about O = A + w, and vectors u = B − A,
  v = C − A, w from A. When D = det(u, v) is near 0 it draws only u and v. Grid and numbered axes.
- **Reader:** drags A, B and C freely, with keys. Readout D and R = |w|.
- **Pencil:** `circle(A, B, C)`, `perpendicular`, `midpoint` and `arrow` exist. `circle(A, B, C)` becomes a line at D = 0 rather than
  vanishing, which is truer than the site.
- **Lacks:** dragging in two dimensions. A handle sets one param; a second `handle A sets …` on the same mark compiles, but a drag
  grabs one handle (`pencil-web`'s `grab`). The grid can be a `curves` collection, and its numbers written out.

**`CircumNormals`** (supporting)

- **Draws:** the triangle, the lines at B and C perpendicular to BA and CA, meeting at A′ opposite A, right-angle marks at B and C,
  and the circle on diameter AA′.
- **Reader:** drags A, B and C, with keys.
- **Lacks:** two-dimensional drag only.

**`CircumCot`** (supporting)

- **Draws:** a circle of radius √(m² + 1) about O, the chord at x = m from A = (m, −1) to B = (m, 1), C on the circle, triangle CAB,
  right triangle O, M, B, and angle arcs at O and C. Labels cot C and csc C, which change with C's side of the chord.
- **Reader:** drags B along y = 1 (m clamped) and C around the circle, with keys. Readout of C, cot C and csc C.
- **Pencil:** two one-dimensional handles. Moving B keeps C's angle, which a param φ does by itself.
- **Lacks:** nothing. The changing label text is two labels and `show`.

**`circumcircle.glsl`** (supporting): see "The five shaders".

### involute

**`Unroll`** (primary): done as `003-unroll`. The site's slider is in degrees and seeks through the inverse of the easing. The
element's play hands back at the matching time, as `goals.md` describes.

### limacon

**`LimaconConstruction`** (primary)

- **Draws:** four ruler-and-compass constructions of the limaçon, each in its own panel: squaring, pedal, conchoid, roulette.
  - All start from C = e^{iφ}, x₁ = sec φ and the orthogonal circle of radius tan φ.
  - They include inversion and reflection, a compass, Pascal's circle, and three angle marks.
  - The roulette adds 36 faint circles through 1, whose envelope is the curve.
- **Reader:**
  - Steps each panel on its own, with buttons and keys.
  - Drags C (φ, 15° to 45°) and z₁ (t) in every panel, plus a driver handle in three panels that appears from its beat.
  - A readout checks the four panels agree on |z₂ − 1| / (|z₂| + 1).
  - The views refit when a drag ends.
- **Pencil:** `lab/explainer` tells the same four constructions as one story for video, from the central circle in hyperbolic
  terms, so it's related, not the same. Handles in panels compile. `fit` ignores a param a handle has taken over, which is close to
  refitting at the end of a drag. Not checked in a browser.
- **Lacks:** separate beats per panel (see above).

**`TriangleLayers`** (primary)

- **Draws:** with a = eˢ:
  - xy = 1 and x² − y² = 1, in two branches each
  - the circle about (sinh s, 0) of radius cosh s
  - the circle about (cosh s, 0) of radius sinh s
  - the semicircle of radius e⁻ˢ
  - two filled triangles and right-angle marks
  - k = tanh s
- **Reader:** drags eˢ along the axis (1.4 to 2.3) and toggles five layers.
- **Lacks:** nothing in the language. The layers are choices read by `show`. The marks' fixed screen size becomes a world size.

**`MedianConstruction`** (primary)

- **Draws:** seven beats: Pascal's circle, a tangent meeting the ray at angle β in m, the pedal curve |m| = cosh² s·cos β + sinh² s
  of Pascal's circle seen from 0, a Thales circle, and the foot z with 1/z̄.
- **Reader:**
  - Steps, with keys. Drags C (φ) and z (β, clamped to ±(2φ − 1.5°)).
  - z appears from beat 6. Readout of β, |z|, 1/|z| and |m|.
- **Pencil:** `004-bounds` already has a pedal curve, captioned beats and a handle. β's clamp depends on φ, so it goes in a
  binding with `clamp`, not the param's range.
- **Lacks:** nothing.

### doyle-spiral-shader

**`DoyleSpiral`** (hero): stays GLSL. See "The five shaders". Controls: q 3–30, p 0–q, arms along {ω, 1, 1 + ω}, which is the choice that
`param axis in {ω, 1, 1 + ω}` was built from.

**`ScaledCopy`** (aside, in two posts): done as `014-scaled-copy`.

- **Draws:** the log spiral zᵗ with z = R·e^{iθ}, R = 1.45 and θ = 40°, eight circles on it at n = −6…1 of radius K·Rⁿ that touch
  their neighbors, a radial tick in each showing its turn, and a copy at a continuous s moving from circle 0 to circle 1, with a
  dashed trail.
- **Reader:** watches a 4.2 s loop.
- **Pencil:** zᵗ is `exp(t·log z)`, so circle n and its tick are circle 0's multiplied by zⁿ, in a `circles` collection and a
  `curves` one, and the copy is circle 0 multiplied by zˢ. The loop's timing is the site's, as beats with lengths (arrive, glide,
  leave) whose `self.life` fades the copy, and `param s` is bound to the glide, eased as the site eases it.
- **Differs from the site:** a reader can also hold s, and step the three beats with ← and →, where the site only loops. The
  poster is the site's still under reduced motion, at 1.8 s.
- **Checked:** the copy's circle, through its own field, is circle 0's image under w ↦ zˢw for s across 0 to 1, and its tick is
  too; the eight circles are that image at whole n and touch their neighbors to 1e-12; time eases s from 0 to 1 over the glide
  (`crates/pencil-core/tests/corpus.rs`). `pencil check` agrees with the oracle to 1/255 at five times and three held s.

**`FollowPixel`** (primary): three panels, two side by side and a wide one under them.

- **Draws:**
  - The spiral and the grid of `doyleMap(4, 12)`.
  - The cell and bounding triangle of a point w, carried into the spiral as curved shapes, and the three candidate circles with the
    nearest filled.
  - The distance circle about z = c(w), touching the nearest one.
  - A red marker when the nearest corner on the flat grid isn't the nearest circle.
  - A close-up that follows z, sized by the distance.
- **Reader:** drags w in a window on the grid, or z in the spiral (through the inverse map), with keys. Readout of the three
  distances.
- **Pencil:**
  - The map, its panels and `fit` on a computed circle exist (`007-lattice`).
  - A region can't go through a map ("a group drawn through `f` can hold lets, functions, parametric curves, segments, dots, labels…"
    *(tried)*), but a region can be bounded by arcs written in the spiral's coordinates directly.
  - Which candidate wins is `min` in a style's `show` and `opacity`.
- **Lacks:** two-dimensional drag, and panels of different sizes (`goals.md`, Known limits).

**`gradient.glsl`** and **`circle-distance.glsl`**: stay GLSL. See "The five shaders".

**`SquareZ`** (aside): done as `006-square`. The site builds each label from pieces measured one by one; Pencil's expressions
replaced that.

**`PowerViews`** (primary): three panels, a wide one over two.

- **Draws:**
  - The steps a·w and bi·w for each power w = zᵖ, p from −n to n.
  - Circles at the powers with radius k|zᵖ|.
  - The spiral zᵗ, log z as its tangent at 1, and |z| and arg z.
  - The same steps in log space as a staircase, with lines at whole turns.
- **Reader:** slider n (1–10). Drags z in either of two panels, with keys. Readout of z, |z|, arg z and the slope.
- **Pencil:**
  - Arrows per instance are a `curves` collection with `head`.
  - `w^m` in a collection works (`010-mobius`).
  - The first step's stronger color is a second collection.
  - Log space refits to n with `fit`.
  - The turn labels vary in number and would be written for the most there can be.
- **Lacks:** two-dimensional drag, and panels of different sizes.

**`UnwrapMorph`** (primary): done as `016-unwrap`.

- **Draws:** one period of the lattice's circles and the rows along 1 and ω, through fₜ(u) = (e^{tu} − 1)/t + 1. At t = 0 that's the
  flat strip, and at t = 1 it's the spiral. Copies a turn above and below fade out as t grows, and the rows are folded mod 2π.
- **Reader:** slider t.
- **Pencil:**
  - Circles map their centers in `with`, with radius k·e^{t·Re u}, which is k·|fₜ′(u)|. Rows are parametric curves of fₜ.
  - **One circle per period is a range,** not a condition. The site's test keeps the grid point whose N = 2aα + 2bβ − aβ − bα,
    twice its inner product with A, is in −|A|² ≤ N < |A|². For each a, N rises with b, so those b are a run of whole numbers,
    and so are the b whose Re u is in the window, since Re u rises with b too (for 0 < p ≤ q). So `for a in -16..16, b in
    first_b(a)..last_b(a)`, with `floor` for the ceilings. It's a whole-number test, as the site's is, so nothing ties at ±π.
  - The fold is `u − 2πi·floor(Im u / 2π + ½)`, and a curve breaks at the jump, so flat a row comes in pieces, and wrapped they
    join.
  - fₜ is 0/0 at t = 0. The slider runs from 0, and the map bends by `max(t, 0.0001)`, which is off the flat limit by about
    bend·|u|²/2, under a thousandth here and far under a pixel.
  - The view fits how far the circles reach at the ends of the window, a quarter turn apart, which is the spiral's bounds once
    wrapped, and room for the copies while it's flat. `fit` can't name a `circles` collection, which the site fits by every circle.
- **Differs from the site:** the framing, above. The site switches to u + 1 below a ten-thousandth, and Pencil has no piecewise
  values, so it bends by that much instead.
- **Checked** (`crates/pencil-core/tests/corpus.rs`):
  - At t = 0, ½ and 1, the drawn (a, b), read from the collection's own ranges, are each in the window and in the turn −π to π,
    none twice, and every lattice point of the window in a 161 × 161 box has exactly one copy drawn, a whole period apart. The
    copies above and below are the same pairs.
  - Each circle is (e^{tu} − 1)/t + 1 with radius k·e^{t·Re u} at t = 0.3 and 0.55, eᵘ with k|eᵘ| at t = 1, where each touches its
    six neighbors to 1e-9, and u + 1 with radius k at t = 0 to within the bound above. 1 stays at 1. Wrapped, the rows run through
    the circles' centers, and flat, they stay in the turn.
  - `pencil check` agrees with the oracle at t = 0, 0.2, 0.55, 0.8 and 1, and under the dark sheet at 0.3.

**`LogNearest`** (supporting): done as `015-log-nearest`.

- **Draws:** two number lines, the real line and a log line with its own scale. Dots at 1, e, e² and at 0, 1, 2, the nearest one on
  each line highlighted, and shaded intervals where the two disagree.
- **Reader:** drags x along the top line, with keys. Readout of each nearest and whether they disagree.
- **Pencil:** segments, dots, `dots` and `curves` collections, and one handle. Log's nearest is `clamp(floor(ln x + 0.5), 0, 2)`.
  The line's is the nearer end of the cell x is in, `floor(ln x)` held to 0 or 1, by whether x is past the cell's arithmetic
  mean. The highlighted dots are dots of their own, drawn over the plain ones, since a `dots` collection has one fill.
- **Differs from the site:** the readout stays page HTML, from the bindings `near_line`, `near_log` and `disagree`. `ln x` is
  labeled, which the site leaves unnamed.
- **Checked:** across 4001 values of x, each highlighted dot is the nearest by its line's distance, found by trying all three; ln x
  sits on the log line in the scale its dots are on; and x is in a shaded stretch, on each line, exactly where the two choices
  differ (`crates/pencil-core/tests/corpus.rs`). `pencil check` agrees with the oracle at six x across the range, and under the
  dark sheet.

**`FloorCell`** (supporting): two plots stacked, 560 × 90 and 560 × 380.

- **Draws:** a number line with ⌊x⌋ highlighted, and the Eisenstein lattice with a point's cell, its triangle half, the steps +1, +ω
  and +1 + ω, and the nearest corner.
- **Reader:** drags x on the line, and drags a free point on the lattice, with keys. Readout of the floors.
- **Lacks:** two-dimensional drag. The two sizes can be two figures.

**`ArmColors`** (primary, in two posts): two panels.

- **Draws:**
  - The spiral's circles, one per period, colored by arm: hue = (row mod n)/n, with n arms along the chosen direction (or rings when
    n = 0).
  - Each arm's exact spiral through its circles.
  - On the grid, each row's segments and dots in its arm's color.
  - The second placement (`period p={2} q={7}`) starts at those values and adds the period A as an arrow with a clause in the
    readout.
- **Reader:** sliders q (3–16) and p (0–q), arms along ω, 1 or 1 + ω.
- **Pencil:** one circle per period is a range, as in `016-unwrap`, whose window in b divides by how fast Re u rises with b,
  which is 0 at p = 0, so that end needs its own range. Circles take per-instance fills on their sides *(tried)*. A `curves`
  stroke and a `dots` fill can't read their variables: "unknown name `s`" and "unknown name `n`" *(tried)*. There's one curve and one row of dots per arm, and the number of
  arms follows p.
- **Lacks:** a color per instance in `curves` and `dots`.

**`ZoomLoop`** (primary): two panels.

- **Draws:**
  - The spiral of p = 4, q = 8 zooming forever: a step along 1 + ω is a pure scaling, so scale ZOOM^{frac(t/2.5)} loops seamlessly.
    Circles are colored by their 1 + ω arm.
  - A chart of doublings against time: the sawtooth with the reset, and the line without it that reaches float32's 2⁶⁴ after some
    minutes. Its time axis doubles as the page stays open, with ticks in minutes and hours and a "now" marker.
- **Reader:** watches. Time is the page's clock.
- **Pencil:** the spiral half is a port, with fills per circle on sides. Whether a frame's circles fit under the 256 cap wasn't
  counted.
- **Lacks:**
  - A clock that doesn't loop. A figure without a `duration` loops on 10 s.
  - Tick labels that vary in number.
  - A chart's axis that rescales with time. A `fit` view could do the rescaling, since it is a function of time.

### doyle-spiral-period

**`LatticeMapping`** (primary): done as `007-lattice`, four panels. The legend and the live identity line stay page HTML.

**`RollTube`** (primary)

- **Draws:** the strip 0 ≤ Im < 2π of log space, bent onto a cylinder of radius 1/roll and turned, with rows along ω colored by
  row mod p, lattice dots, and the period A as a dashed arrow. Back faint, front strong.
- **Reader:** sliders p (1–3), roll (0–1) and turn (0–360°).
- **Pencil:** done as `013-tube` (`web/tube.html`). Each row is a curve of points in space, color by row mod p is three
  collections, and the sheet fades and thins the far side from `self.z`. The view `fit`s the strip in place of the site's zoom,
  and `roll` stops at 0.01, since Pencil can't write the flat limit of a cylinder of radius 1/roll.
- **Lacks:** nothing.

**`CameraLog`** (primary): two panels.

- **Draws:**
  - In log space, the camera's square as the wavy curve x = c₀ + ln(H / max(|cos(y − c₁)|, |sin(y − c₁)|)), filled to the left.
  - The camera moves by t steps of 1 + ω, or by a pure zoom.
  - What the camera sees: the circles under z ↦ e^{−c}z over a faint copy of where they started, colored by arm (two colors).
- **Reader:** a choice of path, and a slider for t. Readout of whether circles and colors line up.
- **Pencil:**
  - A region bounded by an arc and the view's corners.
  - A `circles` collection drawn twice.
  - Two collections for even and odd arms.
  - The yes/no readout from bindings, on the page.
- **Lacks:** nothing.

### doyle-spiral-touch

**`LimaconTangency`** (primary)

- **Draws:**
  - The limaçon |z − 1| = k(|z| + 1), with r(θ) = 2k(1 + k cos θ)/(1 − k²) about 1.
  - The circle at 1 of radius k, and the circle at z of radius k|z| touching it.
  - The triangle 0, 1, z through the touch point, with length labels k, k|z|, |z| and 1 placed clear of the lines.
  - Crossings with the unit circle at e^{±iφ}, φ = 2 asin k.
- **Reader:** slider k. Drags z along the limaçon by its angle about 1, which it keeps as k changes. Keys. Readout of the ratio.
- **Pencil:** a param for the angle, `handle z sets angle = atan2(pointer.y, pointer.x − 1)`, labels that `settle`, and `fit` that
  refits on k but not on a drag, which is what the site does.
- **Lacks:** nothing.

**`LevelCurve`** (supporting): see "The three 3D figures". Slider k, drag to turn.

**`DoyleFlower`** (primary)

- **Draws:**
  - The circle at 1 and its six neighbors c(±1), c(±ω), c(±(1 + ω)) under the skew after n Newton steps from (0, 1).
  - The target limaçon, and the three limaçons through each pair.
  - The arms exp(t log z), and the tangent at 1.
  - The three angles between the tangent directions, labeled in degrees, which add to 180°.
- **Reader:** sliders q (3–30), p (0–q) and Newton step (0–5). Readout of k, the residual and the angles. The view is locked to the
  solved k.
- **Pencil:** `solve` gives the converged skew, so n = 5 is `007-lattice`'s.
- **Lacks:** Newton's steps as values, so a step can be chosen. A `fn` can't recurse, and a step needs derivatives that can't be named
  *(tried)*. One shape this could take is a step count on `solve`. `lab/zoom`'s choice for this figure's view (locked to the circles)
  is in JavaScript, not Pencil.

**`SkewSliders`** (primary)

- **Draws:** p = 2, q = 7, the circle at 1 and six neighbors under a skew (μ, ν) set by hand, with k from the 1 + ω pair, so that
  pair touches by construction. The limaçon of k.
- **Reader:**
  - Sliders ν and μ, continuous.
  - A button that takes one Newton step and writes the result back into both sliders, and a button that resets them.
  - Readout of F, G and whether they're zero.
- **Pencil:** the drawing is a port. Reset is the element releasing params.
- **Lacks:** a button that runs an algorithm and writes params, which `goals.md`'s table counts as Desmos's actions and says a frame
  stays a function of its inputs. The step itself also needs Newton's steps as values.

**`BoundsConstruction`** (primary): done as `004-bounds`. The site's arrow keys (steps, and moving P) are the only thing missing.

**`ScaledCopy`**: under doyle-spiral-shader.

### doyle-spiral-solve

**`NewtonLine`** (aside)

- **Draws:** f(x) = x² − 2, and five Newton guesses from 2.8. Each tangent grows down to the axis in turn, and √2 is marked. An
  8.8 s loop.
- **Pencil:** the guesses are five `let`s, since f′ = 2x is written by hand. The tangents use `trim` and beats with lengths.
- **Lacks:** unequal scales. The view is 2.9 wide by 10 tall at 320 × 300, and Pencil always has equal scales, so the document
  would have to scale y and say so.

**`NewtonDoyle`** (primary): three panels, a wide one over two 3D views.

- **Draws:**
  - In the (μ, ν) plane, the zero curves F = 0 and G = 0, each step's two tangent-plane zero lines, and the arrow from vₖ to vₖ₊₁,
    over a power-of-ten grid that fades with zoom.
  - The 3D views are the surfaces F and G (see "The three 3D figures").
- **Reader:**
  - Slider: step 0–4.
  - Moving it flies the view along the van Wijk and Nuij path, across about 10⁷ of zoom, and the subject is the shallowest step in
    frame.
  - Drag to turn both 3D views together.
  - Readout of vₙ to 7 decimals and |f| in exponent notation.
- **Pencil:**
  - Zero curves are `implicit` curves, which trace.
  - The fading grid is `curves` with `show` reading `view.width`.
  - `lab/zoom` chose this figure's motion ("subject held, with a quick zoom-out") in JavaScript.
- **Lacks:**
  - Newton's steps as values.
  - Moving between two beats' views along a path (`goals.md`, Views: not built in Pencil).
  - 3D, panels of different sizes.
  - Not checked: whether single precision on the GPU holds across 10⁷ of zoom.

**`DigitsChart`** (supporting): see its section above.

### Home page and header

**Hero canvas** (`HeroCanvas.astro`, `hero.glsl`): see "The five shaders".

**Logo** (`FlowerMark.astro`)

- **Draws:** seven circles of a Doyle flower: z ∈ {1, A, B, B/A, A/B, 1/A, 1/B}, radius k|z| with k = 0.25. A sits on the limaçon,
  and B is solved by Newton's method so it touches A. They are sampled at build time into CSS keyframes, a 7 s ping-pong with the
  easing baked in, paused offscreen and off under reduced motion. An inline SVG at 1.6 rem, in `currentColor`.
- **Pencil:** the math is a port: `solve` for B, a `circles` collection, a param on a timeline.
- **Lacks:** an SVG output, or a still or keyframed export. Pencil draws to a GPU canvas, a PNG or an MP4, and a GPU context in every
  page's header for a logo would be the wrong trade.

## Summary

**Coverage today, of 31 sources:**

- **Done: 8.** `Unroll`, `BoundsConstruction`, `SquareZ`, `LatticeMapping`, `RollTube`, `ScaledCopy`, `LogNearest` and
  `UnwrapMorph`, which are corpus figures.
- **Port: 6.** Everything they need exists, and they aren't written yet: `circumcircle.glsl` (by extending `011`), `CircumCot`,
  `TriangleLayers`, `MedianConstruction`, `CameraLog` and `LimaconTangency`.
- **One feature: 6.** `CircumcircleDemo`, `CircumNormals`, `FloorCell`, `ArmColors`, `NewtonLine` and `DoyleFlower`.
- **Two features: 3.** `FollowPixel`, `PowerViews` and `DigitsChart`.
- **Design: 5.** `LimaconConstruction`, `ZoomLoop`, `LevelCurve`, `NewtonDoyle` and `SkewSliders`. The hero and the logo
  are design too.
- **Stays GLSL: 3.** `DoyleSpiral`, `gradient.glsl` and `circle-distance.glsl`.

**In the Doyle series,** which goes first: 23 sources. 7 are done (`SquareZ`, `LatticeMapping`, `RollTube`,
`BoundsConstruction`, `ScaledCopy`, `LogNearest`, `UnwrapMorph`), 3 stay GLSL, 2 are ports (`CameraLog`, `LimaconTangency`), 4
need one feature (`FloorCell`, `ArmColors`, `DoyleFlower`, `NewtonLine`), 3 need two (`FollowPixel`, `PowerViews`,
`DigitsChart`), and 4 need design (`ZoomLoop`, `LevelCurve`, `SkewSliders`, `NewtonDoyle`).

**Missing features, by how many figures need each.** The table counts the 31 sources plus the hero and logo. "Alone" means the figure
is ported once this exists and nothing else does.

| Feature | Needed by | Alone | Kind |
|---|---|---|---|
| Keys: handles a reader can tab to, arrows that move them and step beats | 12 (listed under Keyboard) | 0 to draw, and all 12 to ship: no figure is ported without them (see Order) | Element, plus a nudge through the handle's inverse |
| Dragging a point in two dimensions | 5: `CircumcircleDemo`, `CircumNormals`, `FloorCell`, `FollowPixel`, `PowerViews` | 3 | Language: a point that's `(a, b)` or `a + b·i` of two params is dragged by setting them, and an explicit inverse can give a point (see Order) |
| Newton's steps as values (or derivatives a figure can name) | 4: `DoyleFlower`, `DigitsChart`, `NewtonDoyle`, `SkewSliders` | 1 | Language |
| Panels of different sizes and layouts (a wide strip over two) | 4: `FollowPixel`, `PowerViews`, `NewtonDoyle`, `FloorCell` (or two figures) | 0 | Sheet: a known limit |
| 3D: a view that pitches and that the reader turns | 2: `LevelCurve`, `NewtonDoyle` | 1 | Design |
| A fill that reads the pixel's position | 0, now that `gradient.glsl` and `circle-distance.glsl` stay GLSL | 0 | Style: not planned |
| A field that names its nearest circle, or sides past the 256 cap | 0, now that `DoyleSpiral` stays GLSL and the hero is the site's (maybe `ZoomLoop`, if its circles pass the cap) | 0 | Core and shader: not planned |
| Unequal axis scales | 2: `NewtonLine`, `DigitsChart` | 1 | Viewport: decided against on 2026-11-06, unless asked for |
| Hover, or the pointer as an input outside a drag | 2: `DigitsChart`, hero | 0 | Design: a frame stays a function of its inputs |
| A color per instance in `curves` and `dots` | 1: `ArmColors` (and `RollTube`, `CameraLog` more simply) | 1 | Style |
| Separate beats per panel | 1: `LimaconConstruction` | 1 | Design, or linked elements |
| A clock that doesn't loop, and axes whose ticks vary | 1: `ZoomLoop` | 0 | Design |
| Moving between two views along a path | 1: `NewtonDoyle` | 0 | Not built, chosen in `lab/zoom` |
| Buttons that run a step and write params | 1: `SkewSliders` | 0 | Design: actions |
| SVG output | 1: the logo | 0 | Host |
| Stroke color along a line | 1: `circumcircle.glsl`'s hue drift, which is decoration | 0 | Style |

Host work that isn't a language feature but every port needs, in rough count:

- sliders and choices built from params: about 20 figures (`pencil-lang.dev` has them; the element and the blog don't)
- readouts: about 16
- a two-option choice drawn as a checkbox: `TriangleLayers`

**Not needed by any figure:** pointing at a thing in one panel to show it in another, KaTeX in labels, charts as a general
feature (`DigitsChart` is marks), clipping (`in clip(…)`), and general lists.
