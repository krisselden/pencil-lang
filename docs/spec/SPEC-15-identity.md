# SPEC/15 — Identity

Status: **kernel tier** (changes require deliberate sign-off, same as SPEC/00).

Identity is the mechanism by which a thing in a document can be referred to from
outside its defining expression. It is load-bearing for four systems:

1. **Style selectors** — stylesheets target identities.
2. **Timeline tracks** — `clip reveal { lemni.trim: 0 → 1 }` is an identity reference.
3. **Interaction & recording** — a drag event names the identity it manipulates;
   input streams serialize identities, so replay correctness depends on them.
4. **Hot-reload / reconciliation** — preserving `input` and editor state across an
   edit is a diff over identities.

Because recorded input streams and external stylesheets persist identities to disk,
identity stability is a *compatibility* concern, not merely an ergonomic one.

---

## 1. Kernel invariant (addition to SPEC/00 as invariant #10)

> **Identity.** Only named bindings mint identity. An identity is a path of
> segments (module / figure / clip / binding), optionally extended by a branch
> label or a collection key. Identities are stable over time (via branch
> continuity) and diffable over edits. Anonymous intermediate values have no
> identity: they cannot be styled, targeted by tracks, hit-tested, or recorded
> against; they inherit presentation from their consuming context.

---

## 2. What mints identity

| Form | Mints identity | Notes |
|---|---|---|
| `let x = …` | yes | local to enclosing scope; addressable within it |
| `expose x` | yes | additionally part of the public surface (addressable from outside the `fn`/module) |
| `param` / `input` | yes | always public within the doc; serialized |
| `const` | no | folded at compile time; nothing to address at runtime |
| anonymous subexpression | **no** | e.g. the line inside `midpoint(intersect(a,b)[0], C)` |
| `fn` body internals w/o `expose` | private | addressable only inside the body |

Rationale for anonymous = inert: it keeps the identity space proportional to what
the author deliberately named, matches the `expose` philosophy, and avoids the
GeoGebra failure mode of hundreds of auto-named objects. If you want to style it,
name it — the lint message says exactly this.

---

## 3. Path structure

```
identity   := root-seg ("/" seg)* selector-ext?
seg        := binding-name | fig-name | clip-name
selector-ext :=
    "[" branch-label "]"        // multivalued results
  | "[" key-literal "]"         // comprehension keys
  | "." property                // property of the object (styling/track target)
```

Examples:

```
lemni                       # top-level binding
fig1/O                      # exposed binding inside a figure
main/intro/axes             # timeline-scoped object (clip path is part of identity)
hits[near]                  # branch-labeled intersection result
orbit/pts[k=3]              # keyed comprehension element
fig1/O.fill                 # property reference (track/style target)
```

Rules:

- **Clip scope is a path segment.** Objects declared inside a clip are addressable
  as `timeline/clip/name`. This is what lets an external stylesheet target an
  object that exists for two seconds.
- **Path ≠ display label.** Identities are canonical (NFC-normalized identifiers
  per SPEC/10); labels are presentation.
- **Serialization** uses the canonical string form above. Input recordings and
  stylesheets store these strings; the IR stores interned identity references.

---

## 4. Multivalued results and branch labels

`intersect(...)` and other multiresult primitives return `Option<T>`-valued
components with **branch labels**, not just indices.

- Default labels are positional (`[0]`, `[1]`) but positional access is a lint
  warning in persisted contexts (stylesheets, recordings) because position is not
  stable under motion.
- Authors may name branches at the binding site:

  ```
  let hits = intersect(ray, circle(F1, k)) branches (near, far)
  ```

  Label assignment at `t = t₀` uses a documented deterministic rule (e.g. `near` =
  smaller distance to a stated reference at first sample); thereafter labels follow
  the **continuity tracker** (M3), not re-evaluation of the rule.
- **Identity continuity and visual continuity are the same mechanism.** If
  `hits[near]` is styled red, it must remain the continuously-tracked branch
  through motion. Bifurcations (tangency, appearance/vanishing) resolve by the
  declared `branch` tiebreaker; styles may respond to appear/vanish transitions
  (`on appear: fade 0.2s`).

Consequence for M3: the root tracker is identity infrastructure, not just
geometry infrastructure. Its determinism requirement (state derived only from
`(t, inputs)`) is what makes identities seekable.

---

## 5. Collections and keys

Comprehensions mint identity per element via an explicit key:

```
let pts = for i in 1..n key i:       point(cos(2πi/n), sin(2πi/n))
let tagged = for p in data key p.id: marker(p)
```

- Element identity = `binding "[" key "]"`, e.g. `pts[k=3]`.
- Keys must be equatable literals (int, string). Keys are the reconciliation
  identity across frames *and across edits*: when `n` changes or `data` reorders,
  styles, animations, and input state follow keys, not positions. (This is
  React's keyed reconciliation, adopted deliberately.)
- Defaulting key to the loop index is allowed but lints in persisted contexts,
  same rationale as positional branch access.

---

## 6. Tags

Orthogonal to paths, any binding may carry tags:

```
let O = circumcenter(A,B,C)  tag construction, helper
curve lemni = implicit …     tag hero
```

- Tags are unordered symbols, inherited by nothing (no cascade through the DAG);
  a figure function may apply tags to its exposed members.
- Tags exist purely for selector matching and editor filtering. They carry no
  semantics in evaluation.

---

## 7. Selector language (styling & viewport filters)

Selectors match identities by any combination of:

| Axis | Syntax | Example |
|---|---|---|
| path glob | `a/b/*`, `**/O` | `fig1/*` |
| type | `curve`, `point`, `surface` | `curve` |
| tag | `.tag` | `.construction` |
| clip scope | `in clip(main/intro)` | style only while intro exists |
| chart | `in chart(polar)` | objects defined in a chart |
| state | `:appearing`, `:vanishing`, `:dragging`, `:hover` | interaction/lifecycle |

Combinators are conjunctive within a selector; comma for alternation. Examples:

```
style curve.hero            { glow: 4px }
style **/O, **/H, **/G      { fill: white; halo: 2px }
style .construction in viewport(left) { opacity: 0.35 }
style hits[near]:appearing  { transition: fade 0.2s }
```

### Resolution: layers, not specificity

CSS specificity arithmetic is explicitly rejected. Resolution is:

1. **Ordered layers** (fixed): `base → theme → document → viewport → clip → inline`.
2. Within a layer: **last declaration wins**, document order.
3. Timeline tracks write into the `clip` layer; overlapping clips resolve by the
   clip stack (topmost wins; `blend:` for numeric properties), per SPEC/00 #8.

No `!important`, no scoring. If two rules fight, the fix is always "move one to a
later layer or later position," which is mechanically explainable by tooling
(`pencil explain-style fig1/O.fill` should print the winning chain).

---

## 8. Stability across edits (versioning)

Recorded input streams and stylesheets reference identities; document edits can
break them. Rules:

- **Renames are refactors.** Tooling must support `pencil rename old new` updating
  doc + stylesheets + recordings together. A plain-text rename that bypasses this
  is detectable (recording references a missing identity) and fails loudly at
  load, never silently rebinds.
- **Key discipline** makes collection edits safe: inserting an element doesn't
  shift others' identities.
- Recordings embed the doc content hash they were captured against; replay against
  a modified doc is best-effort with a manifest of unresolved identities.

---

## 9. Conformance obligations

- Corpus: identity canonical-form vectors (unicode/alias/subscript names → paths).
- Corpus: branch-label stability cases (rotation past vertical; tangency
  bifurcation with declared tiebreaker; style follows label).
- Corpus: keyed-collection reshape (n: 5→7→4) with per-key style persistence.
- Property test: for all sampled t orders, selector resolution is identical
  (identity is a pure function of `(t, inputs)`).
- Property test: `pencil explain-style` chain matches actual resolved value.
