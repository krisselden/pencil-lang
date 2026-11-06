# Geometry Language — Project Plan

A declarative language for geometric constructions, implicit/polar curves and surfaces,
with deterministic timeline animation, styling, multiple viewports, and recordable interaction.

**Product shape:** the primary artifact is Manim-class explainer video, rendered
deterministically from the timeline. Interactivity serves two roles: in the
editor, direct manipulation is an *authoring* instrument (drag-to-keyframe
through rigs, performance capture baked to tracks); in the published player,
selected documents ship a *demo profile* as a supplementary interactive embed.
One source document produces both artifacts.

This document is the working plan for building the language with Claude Code.
The **Kernel Invariants** section is the constitution: changes to it require deliberate
human sign-off. Everything else is negotiable per-slice.

---

## 1. Kernel Invariants (freeze first, change rarely)

1. **Purity.** A document denotes `Scene : (t: Time, inputs: Inputs) → Frame`.
   No hidden state. Seeking to any `t` from any prior state yields identical output.
   Simulation (ODE/physics), if ever added, must be declared fixed-step + seeded so
   purity is preserved.
2. **Everything is a signal.** All values are functions of time; constants are
   degenerate signals. `time` is a builtin signal; `t` is never reserved.
3. **Derived-only state.** Roots are typed: `const` (compile-time folded),
   `param` (design-time tunable, serialized, keyframeable), `input` (runtime
   interaction stream, recordable/replayable). Everything else is `let` (pure,
   memoized). There is no assignment.
4. **MathJSON is the expression tier of the IR.** Surface math notation (plain-text
   canonical + LaTeX as input/display method) parses to MathJSON; codegen consumes
   MathJSON. IR is versioned with a JSON Schema.
5. **No reserved identifiers.** Full Unicode identifiers, NFC-normalized, ASCII
   aliases folded (`θ` ≡ `theta`, `x₁` ≡ `x_1`). Subscripts are part of the
   identifier. Coordinates are lambda-bound at chart use sites, never ambient.
   Identifier ≠ display label.
6. **Dual geometry representation.** Every object has a symbolic construction-DAG
   node and a canonical scalar field (exact or approximate SDF). Constructions
   lower to fields; fields compose via SDF algebra.
7. **Geometry is partial.** Multivalued/vanishing results (intersections, tangencies)
   are `Option<T>`-valued signals with continuity-preferring branch resolution and
   explicit `branch` tiebreakers.
8. **Timelines are scoped values.** Clips have local time, duration, and lexical
   object lifetime. Timelines are first-class (can be returned from functions).
   Property tracks are signals in the same DAG; overlap resolves by layer order.
9. **Charts, not curve types.** Polar/log-polar/etc. are coordinate charts objects
   are defined *in* and viewports render *through*. Display charts are animatable
   values.
10. **Identity.** Only named bindings mint identity; identities are paths
    extended by branch labels / collection keys, stable over time and diffable
    over edits. Full text in SPEC/15. (Invariant #3 is amended by SPEC/16 —
    relations & rigs — once adopted.)

---

## 2. Strategy

- **Kernel-first, then vertical slices.** The invariants above are cheap to write
  and expensive to retrofit. Everything else — surface syntax, styling vocabulary,
  stdlib, chart interpolation semantics — iterates inside slices.
- **Executable spec.** The conformance corpus *is* the contract. Prose docs explain
  intent; golden files define behavior. Feature discussions end by adding corpus
  cases; coding sessions start from failing corpus cases.
- **CPU reference interpreter first, kept forever.** Double-precision, naive,
  headless. It is (a) the oracle for differential testing against GPU codegen,
  (b) the way every milestone is testable before any GPU work exists.
- **TSL is the GPU backend; MathJSON stays canonical.** Codegen emits a
  Three.js TSL node graph rather than shader source, gaining WGSL (WebGPU) and
  GLSL (WebGL2 fallback) from one emitter, plus three's renderer plumbing
  (uniform binding, render targets, viewports, passes) instead of a bespoke
  runtime. Guardrails: TSL node types never appear in the IR or any persisted
  format; TSL churn is absorbed entirely inside `codegen-tsl`; the three/TSL
  version is part of the reproducibility hash (§5). One oracle + two emitted
  targets also upgrades differential testing to triangulation: interpreter as
  referee between WGSL and GLSL results.
- **Small packages, one grammar file, versioned IR schema.** Agents do best with
  crisp module boundaries and machine-checkable interfaces.

### Explicitly deferred / not nailed down
- Surface syntax sugar and final keyword choices
- Full styling property set (grow it per-slice)
- Chart interpolation semantics beyond "must declare cut structure"
- Editor/GUI, LSP, hot-reload UX (but the author/runtime mode split and
  auto-key semantics land with M5 — the editor builds on them, not vice versa)
- Video export container/encoding details; motion blur ships first as temporal
  supersampling (`--fps`, `--shutter` — trivially correct under point-purity),
  with analytic per-object blur as a candidate later T2b extension
- 3D surfaces and cameras (Slice 7)

---

## 3. Repo layout

```
pencil-lang/
  CLAUDE.md                  # agent conventions (see §6)
  SPEC/
    00-kernel.md             # §1 of this file, expanded
    10-names.md              # normalization rules + test vectors
    15-identity.md           # identity model, selectors, layers   [drafted]
    16-relations.md          # relations, rigs, causality          [drafted]
    17-families.md           # lattice families, decorated fields  [planned]
    20-ir-schema/            # JSON Schema per IR version
    30-grammar.ebnf          # single source of truth for surface syntax
    50-extensions.md         # foreign primitive ABI               [drafted]
    4x-*.md                  # per-slice design notes (written as slices land)
  packages/
    parser/                  # surface → MathJSON + doc AST
    ir/                      # IR types, schema validation, normalization passes
                             #   (MUST NOT import three/TSL — enforced by lint)
    interp/                  # CPU reference interpreter (the oracle; no three dep)
    codegen-tsl/             # IR → TSL node graph (three/tsl); sole home of
                             #   TSL knowledge; absorbs upstream TSL churn
    runtime/                 # three WebGPURenderer host (WebGL2 fallback),
                             #   viewports, timeline scheduler, uniform bridge
    testkit/                 # golden-file harness, perceptual diff, property
                             #   gens, dual-target differential runner
  conformance/
    001-implicit-circle/
      doc.pen
      expected.ast.json
      expected.ir.json
      frames/t0.000.png      # rendered by reference interpreter
      meta.json              # tolerances, sampled t values, tags
    002-.../
  demos/                     # one runnable demo doc per slice
```

---

## 4. Milestones (vertical slices)

Each slice ships: grammar delta → IR delta (schema bump if needed) → reference
interpreter support → WGSL codegen → conformance cases → a demo doc.
Acceptance = all corpus green + demo renders + differential tests within tolerance.

### M0 — Skeleton & oracle (no language yet)
- MathJSON evaluator over a fixed expression set (arith, trig, comparisons, vectors)
- CPU rasterizer: evaluate a hardcoded scalar field per pixel, sign→coverage AA
- Golden-frame harness with perceptual diff (SSIM/flip, per-case thresholds)
- **No three/TSL dependency in this milestone** — the oracle stands alone
- **Accept:** hardcoded circle SDF renders; goldens stable across runs/platforms

### M1 — Static implicit & polar curves, styled, one viewport
- Grammar: `doc`, `const`, `let`, `curve … = implicit …`, `in polar(r,θ): …`,
  `style` blocks (stroke/fill/width/opacity), one `viewport`
- Name normalization per SPEC/10 (with its own unit-test vector file)
- Parser round-trip: `parse → print → parse` is identity on AST
- TSL codegen for scalar fields; runtime hosts three's WebGPURenderer with the
  WebGL2 fallback path wired from day one
- **Accept:** lemniscate + cardioid demo; corpus ~15 cases; fuzz parser
  (fast-check) with no panics; **differential tests pass on both backends**
  (interpreter vs WGSL and interpreter vs GLSL, per-case eps/ULP budgets)

### M2 — Time, params, timeline clips
- `param` with ranges/steps; `time` builtin; expression-driven motion
- `timeline` / `clip` with local time, layer order, lexical lifetime, fades;
  keyframe tracks limited to declared properties (styles, params, trim)
- Property resolution = fold over clip stack (topmost wins; `blend:` for numerics)
- Headless render-at-t CLI: `pencil render doc.pen --t 1.25 --out frame.png`
- **Accept:** seek-invariance property tests; goldens at sampled t; clip-overlap
  corpus cases (enter/exit, override, blend)

### M3 — Constructions, intersections, branch continuity  ← hardest semantics
- Construction DAG: point/line/circle/midpoint/perp/tangent; lower to fields
- `intersect` returns `Option`-valued multiresults; continuity-preferring root
  tracker; `branch` annotation as deterministic tiebreaker
- **Accept:** velocity-bound property test (dense t sampling; no teleports),
  bifurcation corpus cases (tangency appear/vanish with style fade),
  determinism: tracker state derived only from (t, inputs) — verify by
  rendering t-samples in shuffled order

### M3.5 — Relations & rigs (SPEC/16)
- `relation`/`eq`/`inverse`/`rig` grammar + IR (schema bump); orientation
  checker with diagnostics; automatic inverses (Compute Engine solve);
  unique + multivalued inverse compilation; `pencil explain-rig`
- Least-change and drag-to-roots deferred to M5 (need input streams)
- **Accept:** rig-consistency property (residual ≈ 0 on original `eq`s under
  every rig); apollonius two-rig corpus doc; orientation determinism

### M4 — Charts & multiple viewports
- Chart values (cartesian, polar, log-polar), viewports with chart + view rect +
  layer filters; animatable display chart; chart cut-structure declarations,
  compiler warning on interpolation across cuts
- **Accept:** same scene through two charts corpus; grid-morph demo; goldens per
  viewport

### M5 — Inputs, interaction, recording
- `input` decls (slider, drag point, constrained drag); input stream recording
  to file; replay = deterministic timeline
- **Mode split:** runtime drags are input-stream events and never modify the
  document; author-mode drags are *document edits* (the editor is the sole
  writer). Auto-key: a drag at time t writes keyframes onto the tracks of the
  current rig's **roots** via inverse solve ("keyframing through the rig") —
  dragging derived geometry keys its drivers.
- **Bake:** fit simplified keyframe tracks to a recorded input stream
  (performance capture → editable point-pure tracks). This is the canonical
  stream-pure → point-pure converter.
- **Profiles:** named export configurations over one document; a profile may
  promote `param`s to `input`s and select a start clip
  (`profile demo { promote A, B to input; start at reveal }`). Video export
  and interactive embed are profiles of the same source.
- **Accept:** record/replay round-trip produces byte-identical frame hashes;
  bake round-trip reproduces the recording within declared tolerance; auto-key
  on a derived terminal produces root-track keys whose playback matches the
  performed drag at the keyed times; profile promotion round-trips (demo
  profile with untouched inputs renders identically to video profile)

### M6 — Modules & functions
- `fn`, `expose`, `use "…" as`, path selectors in styles (`fig1/O`)
- **Accept:** euler-line library + consuming doc corpus; shadowing lint tests

### M7 — 3D surfaces (deferred until 2D is solid)
- `surface … = implicit …`, raymarch renderer, orbit camera as chart

---

## 5. Test taxonomy

| Layer | Technique | Oracle |
|---|---|---|
| Names | unit vectors in SPEC/10 | hand-written table (θ/theta, NFC forms, subscripts) |
| Parser | round-trip + fuzz (fast-check grammar-directed gen) | idempotence; no panics |
| Editor grammar | every corpus doc parses under the tree-sitter grammar with zero ERROR nodes (drift detector between canonical grammar and editor grammar; highlighting/LSP tooling never blocks language work) | corpus |
| IR | golden `expected.ir.json` + JSON Schema validation | schema + snapshots |
| Interp | golden numeric samples; property tests on algebraic identities | closed forms; CAS-checked values |
| Codegen | **differential triangulation: TSL-emitted WGSL and GLSL, each vs interpreter, and vs each other** on random points & pixels | interpreter as referee, per-case eps/ULP budgets (document GPU fast-math caveats per op); WGSL↔GLSL disagreement beyond budget is a first-class failure |
| Emitted source | snapshots of TSL-emitted WGSL/GLSL are **informational only** — never contractual (they churn with three versions) | n/a; behavior goldens are the contract |
| Frames | golden PNGs, perceptual diff (SSIM/ꟻLIP), per-case threshold | reference renders committed to repo |
| Timeline | seek-invariance, shuffled-order rendering, clip-overlap goldens | purity invariant |
| Continuity | velocity-bound property tests across dense t | kernel invariant 7 |
| Recording | record→replay frame-hash equality | determinism |

Conventions:
- Every bug fix adds a corpus case before the fix (regression-as-spec).
- Tolerances live in each case's `meta.json`, never hardcoded in the harness.
- CI runs interpreter goldens always; GPU differential tests behind a flag/label
  (run on machines with WebGPU; record adapter info into failures).

---

## 6. CLAUDE.md (starter content)

```markdown
# Project conventions for Claude Code

- SPEC/00-kernel.md is authoritative. If a task conflicts with it, stop and ask.
- The conformance corpus defines behavior. Start every feature by writing/updating
  corpus cases; finish when `pnpm test:conformance` is green.
- Never edit `expected.*` or `frames/*` goldens to make tests pass without
  explaining the semantic change in the PR description and bumping meta.json.
- IR changes require a JSON Schema bump in SPEC/20-ir-schema and a migration note.
- packages/interp is the oracle: keep it simple and obviously correct; performance
  work goes in codegen-tsl/runtime only. interp and ir must never import three/TSL.
- TSL node types must never appear in the IR, the corpus, or any persisted format.
  All TSL knowledge lives in packages/codegen-tsl. If a task seems to require
  leaking TSL types across that boundary, stop and ask.
- Emitted WGSL/GLSL snapshots are informational; never treat them as goldens.
  Behavior (frames + numeric differentials) is the contract.
- Prefer adding a property test over a single example when the invariant is
  quantifiable (continuity, round-trips, seek-invariance).
- Run `pnpm test:fast` before proposing changes; `pnpm test:all` before merge.
```

---

## 7. Sequencing notes & risks

- **Do M3 with machinery, not vibes.** Branch continuity is the highest-risk
  semantics; the velocity-bound property test must exist before the tracker does.
- **GPU float drift** is the main differential-testing annoyance: keep per-op
  tolerance notes (e.g., `atan2`, `pow` vary by driver), and quantize goldens'
  AA comparisons via perceptual diff rather than exact pixels.
- **TSL/three version drift**: TSL evolves faster than a language spec should.
  Contained by the boundary rule (all TSL knowledge in codegen-tsl; MathJSON
  canonical) and by pinning: the exact three version is part of the
  reproducibility hash embedded in recordings and golden `meta.json`. Upgrading
  three is a deliberate PR that re-baselines informational snapshots and re-runs
  the full differential suite; behavior goldens should survive unchanged or the
  upgrade is rejected.
- **LaTeX ambiguity** is contained by making the plain-text notation canonical and
  LaTeX an input/display method (Compute Engine handles the mapping); corpus cases
  should include LaTeX-in → canonical-out normalization pairs.
- **Scope discipline:** the styling system will try to eat the project. Grow it
  only as demos require.
