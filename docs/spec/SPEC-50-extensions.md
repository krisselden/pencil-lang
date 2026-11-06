# SPEC/50 — Extensions and Foreign Primitives

Status: **must exist before M3** (the foreign-field ABI shapes how `interp` and
`codegen-wgsl` communicate; retrofitting after both exist is expensive).

## 0. Principles

1. **Purity is non-negotiable.** Extensions are capability-free: no I/O, no
   clocks, no randomness without a declared seed parameter. `Scene(t, inputs)`
   remains a pure function in the presence of any extension.
2. **The oracle requirement extends to extensions.** Every foreign implementation
   ships a CPU reference implementation. No reference, no load. Differential
   testing (reference vs WGSL, within declared tolerance) applies to extension
   code identically to core code.
3. **Declared, not folklore.** Iteration counts, seeding strategies, derivative
   availability, distance-field quality, branch behavior — anything the compiler
   or renderer relies on is metadata in the manifest, not a comment.
4. **Versioned against the IR.** Every extension declares the IR schema version
   range it targets. Loading an extension outside its range is an error, not a
   warning.
5. **TSL is realization, not semantics.** GPU implementations are Three.js TSL
   `Fn` node factories, giving each extension WGSL and GLSL targets from one
   implementation and letting the compiler compose it (inline it, wrap it in
   folds, feed it derived uniforms) rather than paste it. TSL node types never
   cross into the IR or any persisted format; the three/TSL version is part of
   the reproducibility hash (§5). An extension `Fn` may reference **only** its
   arguments and uniform nodes handed to it by the compiler — closing over
   renderer state, textures, or module globals violates capability-freedom and
   is rejected by lint/review (JS cannot enforce this structurally; the rule is
   contractual).

---

## 1. Extension tiers

From least to most invasive. Prefer the lowest tier that works.

| Tier | Mechanism | Needs foreign code? |
|---|---|---|
| T0 — Library | `fn`, `expose`, `use` — ordinary in-language functions | no |
| T1 — Notation | LaTeX/plain-notation dictionary entries mapping to existing symbols | no |
| T2 — Foreign field | scalar/vector function needing iteration or opaque numerics | yes |
| T2b — Foreign material | `(p, uniforms) → style attributes` (payload channels: albedo, hue, alpha shaping); composes with styling/layers | yes |
| T2c — Foreign pass | `(p) → color`, full takeover of a layer; forfeits styling, selectors, identity, per-viewport theming | yes (last resort) |
| T3 — Chart | coordinate chart bundle (may be in-language or foreign) | sometimes |
| T4 — Construction primitive | new symbolic node with lowering + continuity class | yes (rarely granted) |

T0/T1 need no ABI: MathJSON's open vocabulary plus the Compute Engine dictionary
already cover them. This document is about T2–T4. T2b/T2c share T2's contract
(reference implementation, purity, tolerance, versioning) and differ only in
result type and composition point; as TSL nodes, a T2b material plugs into the
style resolution output and a T2c pass registers as a layer. Prefer T2b over
T2c: a material keeps the object addressable and themeable. T2c retains the
reference-implementation obligation (frame-level goldens still apply) but loses
most numeric differential leverage — treat every proposed T2c as a design smell
and check whether a family + decorated field (SPEC/17) covers it first.

---

## 2. T2 — Foreign fields

### Motivation

The in-language expression tier is deliberately loop-free (clean WGSL lowering,
total evaluation, trivial differentiation). Anything requiring iteration —
numeric Bézier SDFs, Newton-refined spiral distance, fractal/orbit-trap fields,
special functions beyond the builtin set — is a foreign field.

### Contract

A foreign field provides:

```jsonc
// manifest entry
{
  "kind": "field",
  "symbol": "SpiralDistance",             // MathJSON head it implements
  "signature": {
    "params": [
      { "name": "p",      "type": "vec2" },
      { "name": "lambda", "type": "complex" },   // log z = λ, branch-safe form
      { "name": "seedPeriods", "type": "int", "const": true }  // must be compile-time constant
    ],
    "result": "real"
  },
  "field_quality": { "class": "lipschitz", "k": 1.0 },
      // one of: "exact" | "lipschitz" (with bound k) | "implicit"
      // consumed by AA width calculation and any raymarcher
  "derivatives": { "mode": "analytic", "provides": ["grad_p"] },
      // or { "mode": "finite_difference", "h": "auto" }
      // or { "mode": "none" }  → gradient-dependent styling is a compile error
  "continuity": "C1_except_cuts",
  "iteration": { "max": 24, "fixed": true },
      // fixed = true ⇒ identical trip count on all inputs (GPU-friendly,
      // and required: data-dependent early-out breaks cross-device determinism)
  "determinism": { "seeded": false },
  "ir_schema": ">=0.4 <0.6",
  "impl": {
    "reference": "./ref/spiral_distance.ts",    // CPU oracle, f64, mandatory
    "tsl":       "./gpu/spiral_distance.tsl.js" // exports a TSL Fn factory:
                                                //   (compilerCtx) => Fn — receives
                                                //   declared uniform nodes only
  },
  "tolerance": { "abs": 1e-5, "rel": 1e-4, "notes": "atan2 divergence near cut" }
}
```

Notes on specific fields:

- **`field_quality`** is what makes foreign fields safe to compose. `exact` and
  `lipschitz(k)` fields may be offset, shelled, and (later) raymarched; `implicit`
  fields get conservative AA and reject raymarching. SDF `min`/`smoothmin` of a
  `lipschitz(k₁)` and `lipschitz(k₂)` field is `lipschitz(max(k₁,k₂))` — the
  compiler propagates this.
- **`derivatives`**: gradients feed AA (`fwidth`-equivalent), normal computation,
  and gradient-descent styling effects. Analytic when you have it; declared
  finite-difference otherwise, so the cost is visible.
- **`iteration.fixed`**: uniform trip counts sidestep both GPU divergence and the
  determinism hazard of convergence-based exits differing across drivers. Maps
  onto TSL `Loop` with compile-time-constant bounds; data-dependent `Break` is
  disallowed in extension `Fn`s.
- **Purity of the `Fn` factory** (principle 5): the factory receives a compiler
  context exposing exactly the declared params/uniforms; referencing anything
  else (renderer, textures, module state, `Date`, `Math.random`) is a lint
  error and a review rejection.
- **`tolerance`** feeds the differential-test harness; per-op caveats (e.g.
  `atan2`, `pow` driver variance) are documented here, mirroring conformance
  `meta.json` conventions. Tolerances apply to **both** emitted targets; the
  harness also compares WGSL vs GLSL directly with the reference as referee.

### Compile-time constants

Parameters marked `"const": true` must resolve at compile time (e.g. seed-point
counts, unroll factors). This is how a Newton solver's structure stays static
while its inputs animate.

---

## 3. T3 — Charts

A chart is a bundle, not a function. Declarable in-language when the maps are
closed-form; foreign when they need iteration.

```
chart bipolar(a: real) {
  forward(σ, τ) = ...            // chart → cartesian
  inverse(x, y) = ...            // cartesian → chart (may be foreign)
  jacobian(σ, τ) = ...           // optional if forward is in-language
                                 //   (compiler differentiates); mandatory if foreign
  cuts = { τ: none, σ: period 2π }   // cut/period structure per coordinate
  domain = ...                   // optional validity predicate
}
```

Contract obligations:

- **Jacobian** drives automatic AA scaling (generalizing the closed-form
  `length(uv)/length(p)` factor) and correct stroke widths under the chart.
  In-language forward maps get it by symbolic differentiation; foreign charts
  must supply it.
- **Cut structure** is consumed by:
  - the chart-interpolation checker (M4): interpolating between charts whose cuts
    don't align warns, and may route through a declared covering chart
    (`covering: logpolar` on `polar`);
  - curve tracing (draw-on parameterization) to avoid stitching across a cut.
- **Inverse** is required for hit-testing and drag inputs in that chart's
  viewports. A chart without an inverse is display-only (lint at viewport use).

### Chart interpolation

A chart may declare how it interpolates toward another:

```
chart polar { ..., interp cartesian via logpolar_lerp }
```

Absent a declaration, `lerp` of forward maps is used with a compiler warning if
cut structures differ.

---

## 4. T4 — Construction primitives

The most invasive tier: a new symbolic node participates in the construction DAG,
the field lowering, *and* the continuity tracker. Expected to be rare; the bar is
intentionally high.

A construction primitive declares:

```jsonc
{
  "kind": "construction",
  "symbol": "TangentLines",
  "signature": { "params": [ {"name":"P","type":"point"}, {"name":"c","type":"circle"} ],
                 "result": "line" },
  "multiplicity": { "max": 2, "labels_default": ["cw", "ccw"] },
      // how many Option-valued results; default branch labels
  "continuity": {
    "class": "algebraic",            // roots of a declared polynomial system —
                                     // tracker uses proximity tracking + this system
    "bifurcations": "tangency",      // documented degeneracy conditions
    "tiebreaker": "required"         // authors must supply `branches (...)` or accept defaults
  },
  "lowering": {
    "reference": "./ref/tangents.ts",
    "field": "per-result exact line field"
  },
  "ir_schema": ">=0.5"
}
```

Obligations beyond T2:

- **Multiplicity + labels** integrate with the identity model (SPEC/15 §4):
  results are branch-labeled, tracked, `Option`-valued.
- **Continuity class** tells the M3 tracker what it may assume (algebraic root
  systems get polynomial homotopy tracking; black-box gets proximity-only with a
  documented velocity bound).
- **Conformance cases ship with the extension**: at minimum one bifurcation case
  and one velocity-bound property test instantiation. The harness runs extension
  corpora exactly like core corpora.

---

## 5. Packaging and loading

```
use "pkg:spirals@1.2" as spirals          // manifest-bearing package
use "./local/fields.pen" as f             // T0 library, no manifest needed
```

- A package = manifest (`pencil.ext.json`) + reference impls + optional TSL impl +
  conformance cases + docs. Content-hashed; the hash appears in any recording or
  golden produced with it (reproducibility). The reproducibility hash of a
  document additionally pins the exact three/TSL version of the emitter —
  a golden or recording is only meaningful against the emitter that produced it.
- **Resolution is static.** All `use` targets resolve at compile time; no dynamic
  loading. The document + lockfile fully determine behavior.
- Namespacing: extension symbols live under their import alias (`spirals.distance`);
  they never inject into global scope. Notation (T1) entries are similarly scoped
  and activated per-document (`notation use spirals`).

---

## 6. Testing obligations (summary)

| Tier | Mandatory tests shipped with extension |
|---|---|
| T0 | none beyond normal corpus use |
| T1 | notation round-trip vectors (input form → canonical → display) |
| T2 | differential ref-vs-{WGSL, GLSL} on sampled domain within declared tolerance, plus WGSL-vs-GLSL cross-check with reference as referee; gradient check if analytic derivatives claimed (vs finite difference) |
| T2b | T2 obligations per payload channel; composition corpus case (material under two themes/viewports) |
| T2c | reference implementation + frame-level goldens on both backends (numeric differential where feasible); justification note for why T2b/families don't suffice |
| T3 | forward∘inverse ≈ id on domain samples; Jacobian vs finite difference; cut-structure corpus case |
| T4 | T2 obligations + bifurcation corpus case + velocity-bound property instantiation |

CI treats extension test failures as build failures for documents that `use` them.

---

## 7. Open questions (tracked, not blocking)

- Whether T2 fields may be *polymorphic* over precision (f32 GPU / f64 reference
  is implicit today; f16 targets later?).
- A capability for **compile-time tracing** (e.g. an extension that runs marching
  squares at compile time to produce a parameterization buffer) — likely needed
  for implicit `trim` (PLAN §M2/M3); would be a fifth tier ("compile-time
  producer") with purity enforced by hashing inputs.
- Whether construction primitives (T4) can ever be authored in-language once the
  expression tier grows a bounded-iteration form.
