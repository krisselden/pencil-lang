# SPEC/16 — Relations, Rigs, and Selectable Causality

Status: **kernel-adjacent.** The invariant amendment in §1 is kernel tier
(sign-off required). The rest of this document is design tier, implementable as
its own milestone **after M3** (inverse solving through multivalued edges
requires the continuity tracker).

## 0. Motivation

A recurring authoring pattern: the same geometric relationship demonstrated
twice, once with each side as the driver ("two demos to show one invariant").
The relationship is the content; the choice of driver is presentation. This spec
makes that literal: **relations** are declared undirected, **rigs** select
causality, and each (relation set, rig) pair compiles to an ordinary pure
derived-state DAG.

Prior art, deliberately borrowed: Modelica (acausal equations oriented by the
compiler), Mercury (one predicate, many modes), bidirectional lenses
(Foster/Pierce; minimal-put), Apparatus (dragging derived geometry solves back
to roots), Sketchpad (the original ambition).

**Non-goals:** general simultaneous constraint solving, optimization-based
layout (Penrose-style), soft constraints. If ever added, these enter as a
foreign tier under SPEC/50 discipline. Note that `input P : drag point on
circle(O, 2)` already covers the most common "constraint" (projection onto a
locus) with no solver.

---

## 1. Kernel amendment (SPEC/00 invariant #3)

Current:

> **Derived-only state.** Roots are typed (`const`/`param`/`input`); everything
> else is `let` (pure, memoized). There is no assignment.

Amended:

> **Derived-only state, rig-selected roots.** Roots are typed
> (`const`/`param`/`input`); everything else is derived. There is no
> assignment. A document may declare undirected **relations** among terminals;
> a **rig** selects which terminals are roots, and the compiler orients each
> relation set under each rig into a well-founded acyclic derivation covering
> all terminals (§4). Every rig thus induces an ordinary pure DAG; rigs change
> which values are state, never how much state exists.

Corollary (purity taxonomy, referenced by SPEC/00 #1):

> - **Point-pure:** evaluation at `t` depends only on `(t, inputs(t))`.
>   Rig-static documents are point-pure; seeking is O(1).
> - **Stream-pure:** evaluation depends deterministically on the input stream
>   up to `t` (e.g. least-change inverse dragging, §5.3). Replay is exact;
>   seeking replays the fold. Documents that avoid stream-pure features retain
>   O(1) seek. Tooling must report which class a document is in.

---

## 2. Declaring relations

```
relation apollonius(A: point, B: point, P: point, k: real) {
  eq dist(P, A) = k · dist(P, B)

  inverse for k = dist(P, A) / dist(P, B)          // explicit closed form
  inverse for P : underdetermined circle(...)       // resolution policy, §5.3
  // A, B: no inverse declared → not solvable-for under any rig
}
```

- **Terminals** are the relation's parameters. Relations may reference outer
  scope (charts, constants) but only terminals participate in orientation.
- **`eq`** bodies are ordinary expression-tier equations (MathJSON), which makes
  the relation self-testing: the equations are the oracle for every rig (§7).
- Multiple `eq` lines are conjunctive. Systems that cannot be oriented into a
  sequence of single-unknown solves under any rig are a compile error (this is
  the "no simultaneous solving" line; see §8 open questions for block solves).

### 2.1 Inverse declaration grades

| Grade | Declaration | Compiler behavior |
|---|---|---|
| **Automatic** | none needed | If an `eq` is linear (or otherwise symbolically solvable by the Compute Engine) in the unknown, the inverse is derived automatically. `M = (A+B)/2` solved for `A` ⇒ `2M − B`. |
| **Explicit** | `inverse for x = expr` | Author supplies closed form; compiler verifies round-trip at test time (§7). |
| **Multivalued** | `inverse for x : branches (l₁, l₂, …)` with per-branch forms or a root system | Results are `Option`-valued, branch-labeled per SPEC/15 §4, tracked by the M3 continuity tracker. Rigs routing through this inverse require branch resolution. |
| **Underdetermined** | `inverse for x : underdetermined <family>` + policy (§5.3) | Solution is a family (circle, line, …); a rig using it must supply a parameterization or a least-change policy. |
| **Absent** | nothing, and not automatic | Terminal is not solvable-for; rigs requiring it fail orientation with a diagnostic naming the missing inverse. |

---

## 3. Rigs

```
rig show_locus  { drive A, B, k }     // P derived
rig show_ratio  { drive A, B, P }     // k derived

viewport left  { rig: show_locus, … }
viewport right { rig: show_ratio, … }

clip flip : 4s..6s { rig: show_ratio }    // clips may select rigs too
```

- `drive` lists the terminals that are roots under this rig. Driven terminals
  bind to `param`/`input` declarations of matching name/type (or inline
  declarations inside the rig block).
- Every terminal not driven must be derivable (§4). The compiler reports the
  orientation it chose (`pencil explain-rig show_ratio` prints the solve order and
  which inverses were used).
- **Rig selection is static per (viewport | clip).** Two viewports may run
  different rigs over shared relations simultaneously — this is the one-document
  two-demos case. Rig changes at clip boundaries are discrete (no interpolation
  between rigs; a rig is not a value) but **stateful-feeling by design**: they
  use capture-on-switch handoff (§5.5), never a reset to declared initial
  values. Declared initial values apply only at a terminal's first activation
  at document start.
- Default: a document with relations but no rig declarations gets an implicit
  rig driving all `param`/`input` terminals; orientation failure is then an
  ordinary compile error.

---

## 4. Orientation and well-foundedness (the compiler check)

Given a rig R over relation set 𝓡 with terminal set T:

1. Build the bipartite graph of equations and unknowns: unknowns = T \ drive(R);
   each `eq` (or declared inverse) connects to the terminals it can produce.
2. Find a matching that assigns every unknown exactly one producing
   equation/inverse (Modelica-style causalization; Hopcroft–Karp on the
   bipartite graph).
3. Check the induced directed graph (producer → consumer) is **acyclic**.
   Cycles = simultaneous system ⇒ compile error with the cycle printed.
4. Check every multivalued edge on a used path has **branch resolution**
   (a `branches` clause at the use site, per SPEC/15 §4) and every
   underdetermined edge has a **policy** (§5.3).
5. Emit the oriented DAG. Memoization, dirty propagation, codegen proceed
   exactly as for hand-written `let` chains — downstream stages cannot tell a
   rig-derived value from an authored one.

Failure diagnostics are part of the contract: unreachable terminal (name the
missing inverse), cycle (print it), ambiguous matching (print both candidate
orientations, require a `prefer` annotation).

Determinism note: when multiple valid matchings exist, selection must be
deterministic (lexicographic over declaration order) so builds are reproducible.

---

## 5. Semantics of derivation

### 5.1 Unique inverses
Compile to ordinary expressions. Nothing further.

### 5.2 Multivalued inverses
Compile to `Option`-valued branch-labeled results feeding the continuity
tracker. Identity, styling, appearance/vanishing behavior are exactly SPEC/15
§4 — relations introduce no new mechanism here, by design. First-sample label
assignment uses the declared deterministic rule; thereafter labels follow the
tracker.

### 5.3 Underdetermined inverses
The solution is a family. A rig routing through one must choose:

- **Parameterization:** `solve P via on_circle(center, r, φ)` introduces a new
  free real `φ` which the rig must drive (it becomes a root). Point-pure.
- **Least-change:** `solve P via least_change` selects the family member
  minimizing `|ΔP|` from the previous input-stream sample (lens minimal-put;
  Apparatus drag behavior). **Stream-pure:** deterministic as a fold over the
  recorded input stream; scrubbing replays the fold. The compiler marks the
  document stream-pure and tooling surfaces it (PLAN §M2 seek-invariance tests
  split into point-pure and stream-pure variants).

### 5.4 Interaction: dragging derived terminals
A drag on a non-root terminal X is interpreted as an **inverse solve to roots**:
find the update to R's driven terminals that produces the dragged value of X,
routing through the oriented inverse path (all multivalued steps
branch-resolved by the tracker; underdetermined steps by their policy).

- **State never migrates.** X does not become a root mid-session; the drag event
  records as a root-valued update. Recordings therefore contain only root
  updates and replay unambiguously (M5 harness unchanged).
- Note: because roots never move, alternating drags between different terminals
  requires **no rig switch** — grabbing A then B inverse-solves to the same
  roots and all state is trivially preserved. Rig switches (§5.5) are for
  genuinely changing the causal structure, not for changing which handle is
  held.
- Grabbability is decidable statically per rig: X is grabbable iff an oriented
  inverse path from X to roots exists with all edges resolvable. Editors use
  this to render handles.
- Under-/over-shoot: if the drag target is outside the reachable set (e.g.
  dragging P off the Apollonius circle), the solve projects onto the reachable
  set (least-squares on the residual of the `eq` system, single Newton step
  bounded by declared iteration limits — SPEC/50 discipline applies if foreign).

### 5.5 Rig handoff: capture-on-switch

When evaluation context switches from rig R₁ to rig R₂ at time t₀ (clip
boundary, or a recorded interactive switch event):

1. **Capture.** Every terminal driven under R₂ but not under R₁ initializes its
   root value to *its own value at t₀⁻ evaluated under R₁*. Terminals driven
   under both rigs keep their values unchanged. Terminals newly derived simply
   stop being roots; their prior root values are not retained separately (a
   later switch back re-captures from the then-current configuration, so
   round-trip switches are continuous, not restorative).
2. **Seamlessness (C⁰) is guaranteed.** Both rigs orient the same relation set,
   and the captured configuration satisfies every `eq` (residual ≈ 0 under R₁),
   so the full configuration at t₀ is identical under R₁ and R₂. Position never
   jumps at a switch; velocity may (C¹ discontinuity is permitted and expected).
3. **Tracker seeding.** The continuity tracker for any multivalued path in R₂'s
   orientation seeds from the captured configuration: branch labels are chosen
   to match current values, NOT by re-running the first-sample assignment rule.
   A rig switch must never visibly relabel branches.
4. **Purity.** Timeline-scheduled switches preserve point-purity: capture is an
   ordinary evaluation at t₀ under the outgoing rig, so a cold seek performs
   nested evaluation at each prior switch time (cost proportional to the number
   of switches, independent of input-stream length). Interactive switches are
   input-stream events and inherit stream determinism from M5 recording.
5. **Tracks from-current.** A keyframe track on a newly driven terminal may omit
   its start value — `k: → 2 over 1s` — meaning "ramp from the captured value."
   This is the idiomatic form for animating a terminal after taking ownership
   of it; an explicit start value is honored but lints if it disagrees with the
   captured value by more than tolerance (it would reintroduce a visible jump).
6. **Editor policy (non-normative).** A grab on a terminal may be realized
   either as inverse-solve-to-roots (§5.4, default) or as an explicit
   capture-and-switch to a rig driving that terminal ("take ownership") — the
   latter is preferable when the inverse path is underdetermined or
   ill-conditioned near a degeneracy. Both are ordinary recorded events; the
   choice is editor UX, not semantics.

---

## 6. Milestone placement

New milestone **M3.5 — Relations & rigs**, after M3 (continuity tracker is a
dependency), before M4 (viewport rig selection lands with viewports in M4, so
M3.5 delivers relation/rig semantics with the implicit-rig and clip-rig forms
first).

Scope for M3.5:
- `relation`/`eq`/`inverse`/`rig` grammar + IR (schema bump)
- Automatic inverses for linear-in-unknown equations (Compute Engine solve)
- Orientation checker with the four diagnostics of §4
- Unique + multivalued inverse compilation; underdetermined-parameterization
- `pencil explain-rig`
- Deferred within the milestone: least-change (needs M5 input streams to be
  meaningful; land the stream-pure taxonomy and reject `least_change` with
  "requires M5" until then), drag-to-roots (M5).

---

## 7. Testing obligations

The undirected equations are a built-in oracle; use them relentlessly.

- **Rig-consistency property (the big one):** for every rig, for sampled
  driven-terminal values across sampled t: evaluate all terminals under the
  rig's orientation, substitute into every original `eq`, assert residual ≈ 0
  within tolerance. This single property validates automatic solving, explicit
  inverses, and orientation simultaneously.
- **Round-trip per explicit inverse:** `forward ∘ inverse ≈ id` on sampled
  domains (tolerance in the relation's meta, SPEC/50 style).
- **Cross-rig agreement:** for configurations reachable under two rigs, deriving
  the same terminal via either orientation agrees within tolerance.
- **Branch stability under rigs:** multivalued inverse paths get the M3
  velocity-bound property test instantiated per rig.
- **Orientation determinism:** compile twice / permute declaration order of
  non-semantic elements ⇒ identical chosen orientation.
- **Stream-pure replay:** (post-M5) least-change documents: record → replay ⇒
  identical frame hashes; shuffled-order *point* seeks are expected to differ
  from stream evaluation and the harness must assert they're rejected/replayed,
  not silently wrong.
- **Handoff continuity (§5.5):** for corpus docs with rig switches, assert the
  full configuration at t₀⁻ (under R₁) equals t₀⁺ (under R₂) within tolerance,
  residuals remain ≈ 0 across the switch, and branch labels are preserved.
  Cold-seek check: rendering t > t₀ directly (nested capture evaluation) equals
  rendering after continuous playback through t₀.
- Corpus: apollonius two-rig document (the motivating case) with goldens per
  viewport; midpoint auto-inverse case; tangency-through-rig bifurcation case;
  rig-switch handoff case with an omitted-start track.

---

## 8. Open questions (tracked, not blocking)

- **Block solves:** allow small simultaneous linear systems (2×2, 3×3) during
  orientation (Modelica does)? Leaning no until a demo demands it; if yes, only
  closed-form sizes, never iterative.
- **Rig interpolation:** rigs are deliberately not values (no lerp between
  causal structures). Is there a principled crossfade for *presentation* during
  a clip-boundary rig switch (render both, blend styles)? Presentation-only if
  ever.
- **Inequality constraints** (P inside region): currently expressible only as
  drag-input domain projection. Whether relations should admit `ineq` terms
  with projection semantics is deferred until interaction work (M5) surfaces
  real cases.
- **Relation composition:** can relations invoke relations? Likely yes as
  plain inlining (flatten terminals), but scoping/expose rules need a pass in
  SPEC/15 terms.
