# Plan: oriented circles, and drawing both sides

Written 2026-11-06, and built the same day. See "Which side is inside" in `goals.md` for why. What was built differs from this in two places, marked below, and "What it found" is at the end.

## The idea

A circle has a handedness, and its two sides are both things a figure can draw. Three figures need this: the
circumcircle tints by the sign of det(B − A, C − A), the limaçon shader lights both sides of each circle at once, warm and
cool, and a Möbius map turns a disk inside out when its pole crosses the circle.

## What's already been checked

All of this was run on 2026-11-06, in throwaway scripts. None of it is in the code yet.

**The representation works.** A circle is the coefficients of `F(p) = a|p|² − 2Re(b̄p) + c`, scaled so that `|b|² − ac = 1`.
The sign of `F` is the side. `a = 1/r`, so a line is just `a = 0`, and no value goes to infinity on the way there.

| Check | Result |
|---|---|
| `d = F / (1 + sqrt(1 + aF))` against the true signed distance, 4,000 cases, either handedness | within 1.4e-14 |
| The same formula on a line, where a = 0 | exact |
| Through three points, using only determinants: `F` on all three | within 1.8e-14 |
| Handedness | Counterclockwise points have the inside negative, 2,000 of 2,000, which is the existing convention |
| Distance at a fixed point, as C crosses the line AB | 2.0878, 2.0009, 2.0000, 2.0000, 1.9990, 1.9025: continuous through the flip |
| Three points sent through a Möbius map, in order, are its image circle | within 1.1e-12, for four maps including three inside-out cases |
| A point in the disk lands on the same side of the image | every case |
| Kris's near-line at radius 1468, in f32, against an exact 0.292228 | 0.292228 from coefficients, 0.292247 from center and radius |

So the Möbius figure's hand-written closed forms can go. `circle(f(A), f(B), f(C))` is the image, with the right side, as
long as a map keeps orientation.

**What Pencil does today:**

| Try | Result |
|---|---|
| `::inside` and `::outside` on one field | Works |
| A sign flipping a side, by hand, in an implicit curve | Renders |
| `::inside` or `fill` on a `circles` collection | **Accepted, and silently does nothing** |
| `self.dist` on a collection | Error |
| A color times a number | Error |
| `tanh` in a style | Error, since hyperbolics are only compiled in documents |
| Additive blending | Doesn't exist |
| `self.dist` in `fill_opacity`, and `fill_opacity` on a side | Errors |

**Speed, roughly.** (This estimate was wrong, as "What it found" says.) One separate block of shader per circle, at 1080², on an M1 Pro: 24 circles took 6.2 ms a frame and 84 took
6.45, including reading the frame back. So about 0.004 ms per circle, or about 2 ms at 500 by extrapolation. That's one machine, and the
measurement is coarse: an unrelated figure took 15 ms in the same run. Nothing has been tried on a phone.

That path also has **a hard ceiling of about 85 circles**, since each takes 3 of 256 uniform values, and 84 came to 118 KB of WGSL.
So collections can't use it.

**Painting over isn't enough.** In the spike only the last circle shows, since each side paints over what's before it. The
limaçon needs its `col +=`.

## Decisions for Kris

Recommendations first. These are the ones that are hard to change later.

1. **What `::inside` means.** `::negative` and `::positive` follow handedness. `::inside` and `::outside` become the true geometric
   disk and its complement, so a figure can target either meaning, which is what Kris asked for. Today they're aliases. On a line, where there's
   no inside, they follow the sign. For a figure whose circles stay counterclockwise, nothing changes.
2. **A circle's default handedness.** `circle(center, through)` stays counterclockwise, with its disk negative. `circle(A, B, C)` takes
   the handedness of its points. `flip(c)` reverses either.
3. **Rim-only stays a choice.** A style can read `self.curvature`, which is `a`, so Kris's rim-only Möbius disks are one rule: fill only where it's positive.
4. **Additive color.** `blend: add` adds in display values, and `style paper { tone: tanh }` runs the finished picture through `tanh`, which is what the limaçon shader does. It isn't HDR. (This said linear light and clipping until Kris pointed out the shader's tone map.)
5. **`through` stays strict.** It keeps refusing `circles`, since only a Möbius map gives a circle, and writing three points says so plainly.

## Phases

Each ends with everything passing and GPU matching the oracle. Each can stop there and still be useful.

**1. Small fixes, which stand alone.**
- A `fill`, a side or `self.dist` on a collection is an error until phase 3 supports it, never silence.
- A color times a number.
- Hyperbolics in styles and shaders, for `tanh`.
- `self.dist` allowed in `fill_opacity`.

**2. Oriented circles, one at a time.**
- A new value carrying `a`, `b`, `c`. `circle(A, B, C)` and `flip`. `.x`, `.y` and `.r` still read it, and don't exist on a line.
- Fields from the coefficients, using the exact distance above, in the oracle and WGSL.
- A rim drawn from the coefficients, so a line is an ordinary case. It's found from the point nearest the frame, not from angles around a center.
- Tests: distance against true circles, F on the points, continuity through a flip, the line itself drawn, and f32 at a near-line.
- **Proof:** the circumcircle as a figure, with its tint from the sign, through the collinear moment.

**3. Sides on collections.** The design question, and the riskiest part.
- Only a collection whose style asks for sides, `fill` or `self.dist` pays for this. Rims alone stay as they are, so the lattice's speed doesn't change.
- Each instance's `a`, `b`, `c` and style values go in a data texture, which works on WebGPU and WebGL2. The fragment shader loops over them, in draw order, which fits the
  single pass that everything else uses, and the oracle can run the same loop.
- `self.curvature`, `self.dist` and a collection's `with` names in fill expressions, per instance.
- `blend: add`.
- A limit on instances, and measure first: 24, 100, 500 and 2,000 circles, WebGPU and WebGL2. **If this isn't fast enough, stop and rethink before going further.** The fallback
  is an instanced quad per circle, which allows culling but needs more than one pass, and a much bigger change to how a frame is drawn.
- **Proof:** the limaçon, with both sides lit at once. Reading the end of that shader for `damp` and the final color is part of this.

**4. Use it.**
- Möbius in three points, dropping `bottom`, `image_center` and `image_radius`, and its duplicate `dots` and `circles`.
- Kris's choice of rim-only or true outside, as a style.
- Docs: language, decisions, and these results in `goals.md`.

## What this isn't

- `curves`, the list gap the Squaring map showed. That's separate, and it was built right after.
- Ellipses or any other conic. A generalized circle is the one case a Möbius map keeps.
- Non-Euclidean anything, though these coefficients are a natural start for it.
- Sides on parametric curves. A closed curve's winding could give them, and `SPEC-15` has `orient`, but no figure has asked.

## Risks

- **Phase 3's speed on a phone.** Unknown until measured, which is why it's measured first.
- **Style expressions per instance.** Today most are per-frame uniforms. Reading them from a texture touches the WGSL emitter and the oracle, and they have to agree within 1/255.
- **Data texture precision.** An RGBA32F texture needs no filtering, so WebGL2 should be fine with `texelFetch`, but that's untested here.
- **`::inside` changing meaning** could shift existing figures. `corpus/001-circle`, `002-strokes/filled.style` and `004-bounds/blog.style` use sides, so those need checking, though all should stay as they are if their shapes keep their current handedness.

## What it found

All of this was run on 2026-11-06, on an M1 Pro.

**The loop is about 20 times slower per circle than the spike suggested.** The first estimate came from separate blocks of shader, and it said 0.004 ms per circle. A page that does the real thing, `lab/sides/index.html`, says about 0.09 ms, so that estimate shouldn't be used:

| Circles, at 1080² | WebGPU | WebGL2 |
|---|---|---|
| 24 | 2.9 ms | 4.1 ms |
| 100 | 9.4 ms | 8.1 ms |
| 500 | 46.7 ms | 34.1 ms |
| 2,000 | 156 ms | 108 ms |

So this suits a figure's worth of circles, and not a texture of thousands. The figures that wanted it have 24 and 21, so it went ahead, limited to 256 per collection, which is an error rather than a gap. Rims alone don't go through it, and have no limit. **Nothing has been tried on a phone.** Opening that page on one would say, and instanced quads with culling are still the way past this if a figure ever needs it.

**In the real figures:** the limaçon at 1280 × 1280 holds the display's 120 Hz on both backends. The lattice, which only has rims, draws in 6.9 ms at 1580 × 1603 in WASM, so it hasn't slowed down. Every figure in the corpus matches the CPU oracle within 1/255, with each of its styles.

**A straight line with dashes has a gap.** A single circle's dashes come from its center and radius, and exactly three points in a line have neither, so for that moment it has no dashes. A solid stroke is drawn per pixel from the coefficients, and is fine. A collection's dashed rims are chords when a circle is enormous, since their pieces are capped. Neither has come up in a figure.

**`::inside` didn't shift any existing figure,** since a circle from a center and a point always has its disk on the negative side.

**Writing Möbius in three points removed four things:** `bottom`, `image_center`, `image_radius`, and the duplicate `dots`. A pole anywhere on a circle now draws a line, where the closed form lost that circle. A pole exactly on one of the three points still loses it for that frame.

**Rim-only pops at a line.** `max(sign(self.curvature), 0)` switches a whole half-plane on as a circle straightens and its curvature turns positive. That's what the rule says, and the old figure did the same. Near Kris's values, one of the two near-lines is a gray half-plane.
