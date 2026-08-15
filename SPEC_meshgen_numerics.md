# SPEC — Mesh generation: numerical robustness freeze (subtask G0-2)

**Status:** frozen (rev 1.2; G2-3 boundary/default/C3 dual-ratio record added 2026-07-28). Normative for all `src/meshgen/` and
`src/gpu/meshgen_*` work.
**Date:** 2026-07-24
**Subtask:** G0-2 (Phase G0, tier T3) of [`PLAN_mesh_generation.md`](PLAN_mesh_generation.md).
**Scope (from the plan's G0-2 acceptance):** predicate inventory; registry
provenance keys; construction error bounds + double-double escalation
thresholds; GPU margin certificates; strict/fast determinism rules; crate
evaluations (`robust`, `spade`). *Acceptance: every decision site lists its
predicate; bounds derived; choices recorded.*
**Companion freezes:** [`SPEC_meshgen_geometry.md`](SPEC_meshgen_geometry.md)
(G0-1 — templates, tables, conformity; this document supplies the arithmetic
every one of its predicates runs on), G0-3 (VTU schema, check catalog, accuracy
table).

Every bound and rate below was measured against exact rational arithmetic or
executed against the real crates before being written; the verification record
is §10. Deviations from the plan's prose are in §11 — including one place where
the plan's stated GPU certificate is not a certificate.

---

## 0. Normativity

- **MUST/MUST NOT** are binding. This document wins over `PLAN_mesh_generation.md`
  prose on arithmetic, predicates, and determinism.
- Any decision site not listed in §2 is a **defect in this document** — raise it,
  do not invent a predicate locally.
- Constants named here (`ε`, `q`, `κ_esc`, `δ_bp`, filter constants) are the only
  tolerances the mesher may contain. A literal tolerance anywhere else in
  `src/meshgen/` is a review failure.

---

## 1. Coordinate frame, scale, and the tolerance ladder

### 1.1 Mandatory normalization

> **Rule N1.** At load time the input is affinely mapped so the domain box
> becomes the unit cube: `x̂ = (x − domain_min) / diag`. **All** meshing runs in
> normalized coordinates; the inverse map is applied only at export. Every
> construction MUST additionally be evaluated after translating to a local
> origin (the owning cell's minimum corner, or the first vertex of the defining
> entity) so operand magnitudes are `O(local extent)`.

This is not hygiene, it is the largest single error term. Measured forward error
of an edge–triangle intersection, relative to segment length, as the geometry is
translated away from the origin (§10 [N1]):

| Coordinate offset | Error (any formulation) |
|---|---|
| 0 | `2.3e-16` |
| `1e3` | `1.6e-13` |
| `1e6` | `1.2e-10` |

The formula does not matter at `1e6`: the *inputs* no longer resolve the answer,
because f64 spacing at magnitude `10⁶` is `≈1.2e-10`. Real STL data in
millimetres on a machine-coordinate origin sits exactly here. Rule N1 recovers
five to six decimal digits before any predicate runs.

### 1.2 Constants

| Symbol | Value | Meaning |
|---|---|---|
| `u` | `2⁻⁵³ ≈ 1.11e-16` | f64 unit roundoff |
| `u₃₂` | `2⁻²⁴ ≈ 5.96e-8` | f32 unit roundoff |
| `u_dd` | `≈ 2⁻¹⁰⁶ ≈ 1.23e-32` | double-double unit roundoff |
| `ε` | `eps_frac · 1` (normalized) — implemented default `1e-4` | envelope / coincidence tolerance |
| `q` | `0.1 · ε` — default `1e-5` | weld quantization step; `NodeKey` grid (G0-1 §1.2) |
| `κ_esc` | `4·c·u/q` with `c = 7` — default `3.1e-10` | escalation trigger on the stability ratio (§4.3) |
| `c_flt` | `(7 + 56u)·u = 7.772e-16` (f64), `4.172e-7` (f32) | orient3d static-filter constant (§6.1) |
| `δ_bp` | `2⁻²³ · max\|coord\|` | broad-phase AABB inflation (§6.2) |

Load-bearing ordering, checked at parse time **and** re-asserted on the realized
field (G0-1 §11.4): `q ≪ ε ≪ t_sheet < t_layer ≤ h`.

---

## 2. Predicate inventory — every decision site

Arithmetic classes: **X** = exact predicate (`robust`, adaptive, sign-only);
**I** = exact integer/symbolic (no rounding possible); **A** = algebraic f64
comparison of quantities whose sign is not topologically load-bearing;
**F** = f32 filtered on GPU with an exact CPU resolution path (§6);
**T** = tolerance comparison against a named constant of §1.2.

| Stage | Decision site | Predicate | Class | On failure / ambiguity |
|---|---|---|---|---|
| S0 | vertex identity (weld) | `NodeKey` equality | **I** | — |
| S0 | degenerate triangle | `orient2d` in the best-conditioned projection = 0 | **X** | drop per `repair` level |
| S0 | duplicate face | sorted `NodeKey` triple equality | **I** | — |
| S0 | consistent orientation | edge-parity walk over the adjacency graph | **I** | majority vote (`permissive`) |
| S0 | pinhole closure | boundary-loop diameter `< ε` | **T** | logged repair action |
| S1 | sharp edge | `cos²θ` vs `cos²(feature_angle)` on squared dot/norm products — **never `acos`** | **A** | — |
| S1 | rim / non-manifold edge | incident-face count | **I** | — |
| S1 | curve junction / corner | valence `≥ 3`; turn angle by `cos²` comparison | **I** / **A** | — |
| S2 | triangle-pair crossing | 6× `orient3d` sign pattern | **X** | — |
| S2 | coplanarity | 4× `orient3d` all `= 0` | **X** | routes to the 2D overlay path |
| S2 | 2D overlay orientation, point-in-triangle | `orient2d` | **X** | — |
| S2 | segment ordering along an intersection line | compare exact barycentric parameters on the owning edge | **X** | — |
| S2 | registry entity identity | provenance key equality (§5) | **I** | — |
| S2 | snap-round of a registry vertex | `NodeKey` quantization | **I** | — |
| S2 | coincidence within `ε` | distance² vs `ε²` | **T** | policy table (G0-1 §10) |
| S2b | component connectivity, closure | combinatorial (edge adjacency, Euler) | **I** | — |
| S2b | radial patch ordering around a curve | `orient2d` in the plane ⟂ to the curve tangent | **X** | tie → smallest `NodeKey` |
| S2b | GWN inside test | `w > 0.5` with the §6.3 band | **F** | CPU f64 → WARN + defect report |
| S3 | separation comparisons | squared distances | **A** | — |
| S3 | opposing-normal condition | `dot < 0` with a relative margin | **A** | pairing battery / confidence gate |
| S3 | regime thresholds | `t_r` vs `τ·h`, hysteresis 0.9/1.1 | **T** | G0-1 §11 ladder |
| S4 | sizing constraints, 2:1 gradation | f64 min; integer level compare | **A** / **I** | — |
| S5 | split(face) / split(edge) | lattice-node membership of centre/midpoint (G0-1 §3.1) | **I** | — |
| S5 | Morton order, balance | integer | **I** | — |
| S5 | template positivity | `orient3d > 0` (bounded below by `h³/48`, G0-1 §3.4) | **X** | assertion |
| S6 | parity classification | `orient3d` × 4 per ray-triangle test | **F** → **X** | filter-uncertain → CPU exact (ARB-8) |
| S6 | ray degeneracy | filter returns exact `0` | **X** | deterministic re-shoot, then arbitrate (ARB-9) |
| S7 | edge crossing exists | `orient3d` sign change along the edge | **X** | — |
| S7 | snap target priority | corner > curve > surface, then ascending `NodeKey` | **I** | — |
| S7 | snap would invert | `orient3d > 0` on every incident tet | **X** | reject move (ARB-10) |
| S7 | motion cap | `‖Δ‖² ≤ (0.3·L_min)²` | **A** | clamp (ARB-11) |
| S8 | cell triage, face-split dispatch | counting | **I** | — |
| S8 | quad diagonal (Rule SNK) | `NodeKey` comparison | **I** | — |
| S8 | emitted-tet validity | `orient3d > 0`; `cos²` dihedral vs `cos²(8°)` | **X** / **A** | §4.4 ladder (ARB-17) |
| S8 | corner votes / cut sides | cached `side_of` lookups | **I** | ambiguous → arbitrate (ARB-12) |
| S8 | oriented probe | `robust_inside(c + η·n)` | **X** | arbitrate (ARB-13) |
| S8 | junction sub-region seeding | 5-ray `robust_inside` | **X** | GWN, then majority (ARB-19) |
| S8 | guarded dry-run | all-positive + `\|ΣV − V_parent\| ≤ 1%·V_parent` | **X** / **A** | escalate (ARB-15) |
| S8b | band `k`, template choice | counting | **I** | — |
| S8b | predicted band quality gates | algebraic (altitude, AR, `cos²` dihedral) | **A** | ladder (ARB-20) |
| S9 | IQD defect classification | algebraic metrics | **A** | — |
| S9 | smoothing acceptance | `orient3d > 0` + algebraic quality, recomputed in f64 (strict) | **X** / **A** | reject move |
| S9 | collapse guard | record-level side comparison | **I** | reject (ARB-21) |
| S9 | Phase-5.5 snap | distance vs `2%·h`; `AR < 10`; `cos²` dihedral vs `cos²(10°)` | **T** / **A** | reject move |
| S10 | `resolve()` | integer set operations on X-values | **I** | ambiguous → error (ARB-23) |
| S10 | partition flood fill, numbering | combinatorial; ascending `NodeKey` | **I** | — |
| S11 | VTU ascii round-trip | shortest round-trip f64 formatting (Rust `{}`) | **I** | — |
| S11 | INP material mapping | region-key lookup | **I** | hard error (ARB-25) |

> **Rule N2.** No topologically load-bearing decision may be class **A** or **T**.
> Classes **A**/**T** appear only where a wrong answer costs *quality or a
> classification that a later exact check re-validates*, never mesh validity.

---

## 3. Arithmetic tiers

Three tiers, in strict order of use:

1. **f64 with a static filter** — the default for every predicate. Cheap; when
   `|value| > c_flt · permanent` the sign is *proved* correct (§6.1).
2. **Exact predicate** (`robust`, adaptive expansions) — used when the filter is
   inconclusive. Sign-exact, unconditionally. **Its returned magnitude is not
   exact and MUST NOT be used as a number** (§7.1).
3. **Double-double (DD) constructions** — used when a *value* (not a sign) needs
   more than f64, per the escalation rule of §4.3.

> **Rule N3.** Signs come from tiers 1–2, values from tiers 1 and 3. A sign is
> never inferred from a constructed value, and a value is never taken from an
> exact predicate's return.

### 3.1 The frozen DD primitives

Hand-rolled (§7.3 records why), built from the classical error-free
transformations:

```rust
fn two_sum(a: f64, b: f64) -> (f64, f64) {           // Knuth, no branch
    let s = a + b; let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}
fn two_prod(a: f64, b: f64) -> (f64, f64) {          // Dekker via FMA
    let p = a * b;
    (p, a.mul_add(b, -p))
}
```

- `f64::mul_add` MUST be used for `two_prod`: it is a single correctly-rounded
  fused operation on every Rust target, which makes the residual exact and the
  result platform-independent.
- The compiler MUST NOT be allowed to contract `a*b + c` into an FMA on its own.
  Rust/LLVM do not do this without opt-in; the rule is that no code outside these
  primitives may rely on either behaviour.
- DD add/sub/mul only. **No DD division and no DD square root**: every place that
  would need them (parameter ratios, lengths) divides *once*, in f64, after the
  DD numerator and denominator are formed (§4.1).

---

## 4. Constructions: error bounds and the escalation rule

### 4.1 The frozen construction forms

| ID | Construction | Frozen form |
|---|---|---|
| **C1** | edge × triangle intersection (registry `EdgeTri`) | `t = d_p / (d_p − d_q)` with `d_p = orient3d(a,b,c,p)`, `d_q = orient3d(a,b,c,q)`; `x = p + t·(q − p)` |
| **C2** | triangle × triangle × triangle point (registry `TriTriTri`) | Cramer on the three plane equations; numerator and determinant formed at the same precision tier, one f64 division |
| **C3** | 2D segment × segment (coplanar overlay) | form both edge parameters with the same determinant-ratio formula; construct the 3-D point once as `p + t·(q-p)` on the canonical second edge |
| **C4** | snap target (projection onto a patch/curve) | f64; accuracy-only — validity is re-established by the exact inversion test (S7) |
| **C5** | midpoint / centroid / Steiner point | exact average of existing coordinates; on the lattice these are exact on the `h_min/2` integer grid (G0-1 §3.2) |

> **Rule N4.** Intersection parameters are always **ratios of determinants**,
> never plane-normal expressions. `x` is always formed by an affine combination
> of the *defining* points, never assembled from independently computed
> components.

### 4.2 Measured behaviour — and why the form alone is not enough

Forward error relative to segment length, exact-rational ground truth, 3 000
cases per family (§10 [N1]):

| Family | normal-vector form | determinant ratio, f64 dets | determinant ratio, **exact dets** |
|---|---|---|---|
| generic | `6.9e-16` | `1.3e-15` | `2.3e-16` |
| near-parallel | `2.0e-4` | `4.3e-4` | `2.4e-15` |
| near-vertex | `1.1e-15` | `1.2e-15` | `2.6e-16` |

The determinant-ratio form is **not** more accurate than the naive form when its
determinants are evaluated in f64 — it is 11 orders of magnitude off in the
near-parallel family, same as the naive form. All of its benefit comes from
evaluating `d_p`, `d_q` accurately. Rule N4 therefore exists to make escalation
*possible* (there is one scalar per operand to escalate); §4.3 is what makes it
*happen*.

### 4.3 The escalation rule

Every construction defines a dimensionless **stability ratio** `ρ` = the
magnitude of its critical denominator divided by the permanent (the sum of
absolute products) of the same expression:

| Construction | `ρ` |
|---|---|
| C1 | `\|d_p − d_q\| / (perm_p + perm_q)` |
| C2 | `\|det(n₁,n₂,n₃)\| / perm(n₁,n₂,n₃)` |
| C3 | minimum of the two defining-edge ratios `\|o_p − o_q\| / (perm_p + perm_q)`; both must clear the selected precision tier |

> **Rule N5 (escalation).** Evaluate in f64. If `ρ ≥ κ_esc` the f64 result is
> within `0.25·q` and is committed. If `ρ < κ_esc`, recompute *that construction*
> in DD. If `ρ < κ_esc · u_dd/u` (i.e. DD also cannot resolve it), the entities
> are degenerate at mesh scale: route to the coincidence/weld path (G0-1 §10) and
> log `[ARR-PREC]` (ARB-1).

Derivation of `κ_esc`. The construction's position error is bounded by
`L · c · u / ρ` with `L ≤ 1` after Rule N1 and `c = 7` (Shewchuk's orient3d
constant, measured worst case `3.23·u·permanent`, §10 [N4]). Requiring
`≤ 0.25·q = 0.025·ε` gives `ρ ≥ 4·c·u/q`.

| `eps_frac` | `q` | escalate below `ρ =` | equivalently, condition number above |
|---|---|---|---|
| `1e-3` (non-default; rejected by the default sizing/gap ordering) | `1e-4` | `3.1e-11` | `1.4e10` |
| `1e-4` (implemented default) | `1e-5` | `3.1e-10` | `1.4e9` |

DD extends coverage by `u/u_dd ≈ 9e15`, i.e. down to `ρ ≈ 3.5e-26` at the
implemented default (`≈3.5e-27` for configured `eps_frac=1e-3`) — beyond
which "these two things are the same at mesh scale" is the *correct* answer, not
a precision failure. Escalation is therefore rare and cost-bounded (assumption
G-12 discharged): DD `orient3d` costs **6–8×** a naive one and **~5×** an exact
`robust` call (§10 [E1]), applied to a vanishing fraction of constructions.

Independently measured for C2 (§10 [N2]): position error tracks `≈ 22·u/ρ`
across six decades of conditioning, confirming the `c·u/ρ` model with a constant
inside the `c = 25` used for safety.

### 4.4 What is *not* escalated

C4 (snap targets) and C5 (averages) never escalate. C4's validity is re-decided
by exact tests; C5 is exact on the lattice grid and `≤ γ_k·max|p|` elsewhere,
always far under `0.25·q`.

---

## 5. Registry provenance keys

The registry (plan §10.3) is what guarantees two triangles never disagree about
"the same" point. That guarantee is **symbolic**, so the keys must be canonical
and their containers order-independent.

| Entity | Key | Canonical form |
|---|---|---|
| intersection vertex, edge × triangle | `EdgeTri { e, t }` | `e = (min(v₀,v₁), max(v₀,v₁))` post-weld vertex ids; `t` = stable input triangle id |
| intersection vertex, coplanar edge × edge | `EdgeEdge { first, second }` | both edges canonical as above, then sorted ascending as a pair |
| intersection vertex, 3 triangles | `TriTriTri([t₀,t₁,t₂])` | the three ids **sorted ascending** |
| intersection segment | `Seg { va, vb, ta, tb }` | `(va,vb)` sorted registry-vertex ids; `(ta,tb)` sorted triangle ids |
| coplanar overlay sub-patch | `Overlay { tris }` | sorted id list of all participating triangles |
| welded node | `NodeKey` | quantized integer triple (G0-1 §1.2) |

> **Rule N6.** Identity is symbolic first (provenance key), quantized second
> (`NodeKey`), and **never** floating-point equality. A construction that
> produces a point without a provenance key is a defect.

> **Rule N7 (container determinism).** Registry maps and every other map whose
> iteration can influence output MUST be a `BTreeMap`, or a `HashMap` with a
> fixed-seed deterministic hasher **plus** a sort before any iteration that
> emits. Rust's default `HashMap` uses a randomly seeded `RandomState`; its
> iteration order changes between runs of the same binary and would silently
> break the strict determinism contract.

> **Rule N8 (id assignment).** Registry indices are assigned in ascending
> canonical-key order in a single pass after collection, never in first-touch
> order.

---

## 6. GPU margin certificates

The strict determinism contract (plan §12) requires that GPU output is a
*conservative proposal*, never a decision. Each kernel therefore ships a
certificate: a condition under which its f32 answer is provably the exact one.

### 6.1 Orientation tests — the static filter (S2 broad phase, S3, S6)

Evaluate `orient3d` in Shewchuk's expansion order and accumulate the matching
permanent in the same pass:

```
det  = adz·(bdx·cdy − cdx·bdy) + bdz·(cdx·ady − adx·cdy) + cdz·(adx·bdy − bdx·ady)
perm = (|bdx·cdy|+|cdx·bdy|)·|adz| + (|cdx·ady|+|adx·cdy|)·|bdz| + (|adx·bdy|+|bdx·ady|)·|cdz|
```

> **Certificate G1.** `|det| > c_flt · perm` ⟹ `sign(det)` is exact, where
> `c_flt = (7 + 56u)·u` evaluated at the working precision:
> `7.772e-16` for f64, `4.172e-7` for f32. Otherwise the test is **uncertain**
> and the vertex/pair is routed to the CPU exact path.

The expansion order is part of the certificate: the constant is derived for it
and does not transfer to another factorisation.

Verified over 40 000 adversarial cases per precision (70% forced near-degenerate,
coplanar offsets down to `1e-18`): **zero** bound violations, worst measured
error `3.23·u·perm` (f64) and `3.54·u·perm` (f32), and **zero** wrong signs
passed the filter (§10 [N4]).

Cost of the certificate, measured on a realistic S6 workload (lattice vertices on
a `1/64` grid, fixed irrational ray, triangles of edge `≈ h`): **3.3e-6** of edge
tests are uncertain. At `10⁸` tests that is a few hundred CPU resolutions — the
"GPU proposes, CPU decides" split is essentially free.

Filter returning exactly `0` (a true degeneracy) → deterministic re-shoot down
the fixed direction sequence, then arbitrate (ARB-9).

### 6.2 Broad phase — conservative inflation (S2)

The GPU grid works in f32 while the geometry is f64.

> **Certificate G2.** Converting an f64 coordinate to f32 changes it by at most
> `u₃₂/(1−u₃₂)·|x|`; measured worst case `5.9422e-8` against the bound
> `5.9605e-8` (§10 [N6]). Inflating every f32 AABB by
> `δ_bp = 2⁻²³·max|coord|` (2 ulps, both sides) therefore makes the candidate
> pair set a **superset** of the exact one.

A superset is all the contract requires: the CPU narrow phase is exact and
discards false positives. The pair-count explosion guard (plan §14) applies to
the inflated set.

### 6.3 Generalized winding number (S2b)

`w = (1/4π)·Σ ωᵢ`, inside iff `w > 0.5`.

> **Rule N9.** The GPU GWN kernel MUST use a **pairwise (tree) reduction** and
> MUST accumulate `S = Σ|ωᵢ|` alongside `w`.
>
> **Certificate G3.** `|w − 0.5| > δ_gwn` with
> `δ_gwn = c·log₂(n)·u₃₂·S/(8π)`, `c = 4`, ⟹ the f32 classification equals the
> f64 one. Otherwise recompute in f64 on CPU (ARB-5).

Measured on closed triangulated spheres with an interior query point (§10 [N5]):

| n | sequential f32 | pairwise f32 | `δ_gwn` (pairwise) |
|---|---|---|---|
| 6 400 | `1.6e-6` | `4.8e-8` | `3.8e-7` |
| 57 600 | `7.2e-6` | `2.8e-8` | `4.7e-7` |
| 360 000 | `8.0e-6` | `2.8e-8` | `5.5e-7` |

Pairwise reduction is ~300× more accurate than sequential accumulation and,
unlike it, does not degrade with `n` — which is what keeps `δ_gwn` a usable band
(sub-`1e-6`) instead of a threshold comparable to the decision itself.

### 6.4 What the GPU may never emit

Under `strict` determinism, no GPU kernel may produce a committed coordinate,
a committed classification, or an ordering that survives into the output. Its
legal outputs are: candidate sets (supersets), rankings, uncertainty flags, and
diagnostics. Under `fast`, smoothing positions and metric-driven choices may
commit within the plan §12 tolerances; topology, labels, and tables are identical
in both modes.

---

## 7. Crate evaluations (executed)

### 7.1 `robust` 1.2.0 — **adopted** for exact predicates

- Provides `orient2d`, `orient3d`, `incircle`, `insphere` over `Coord`/`Coord3D`.
- Exactness: **0 wrong signs** in 200 000 near-degenerate `orient3d` cases where
  naive f64 got **33 879 wrong (17 %)**.
- Cost: `1.3–1.5×` a naive f64 `orient3d` over 2 M calls (16.5 ms vs 12.5 ms) —
  cheap enough that the filter is an optimisation, not a necessity, on the CPU
  path.
- **Sign convention hazard (measured):** `robust::orient3d(a,b,c,d)` is
  **opposite in sign** to G0-1's `orient3d = det[b−a, c−a, d−a]`. Wiring it in
  directly would invert every emitted tet.
  > **Rule N10.** All call sites use one wrapper,
  > `fn orient3d(a,b,c,d) -> f64 { -robust::orient3d(...) }`, which is the sole
  > place the negation appears, with a unit test pinning the sign against a
  > known positive tetrahedron.
- Its return value is a *filtered/adaptive* magnitude, not an exact one: sign
  only (Rule N3).

### 7.2 `spade` 2.15.1 — **adopted, restricted**, for planar CDT only

Executed findings:

| Property | Result | Consequence |
|---|---|---|
| Non-intersecting constraints | works as expected | our only case — corefinement pre-splits everything at registry vertices |
| Crossing constraint via `add_constraint` | **panics** | **Rule N11:** `add_constraint` is banned. Use `can_add_constraint` / `try_add_constraint`, which refuse cleanly (returned empty vec, no mutation) |
| `add_constraint_and_split` | constructs intersection points; its own docs carry a precision warning that they may not lie exactly on the line | **Rule N12:** banned outright — it would create geometric identities outside the registry, the exact failure mode the registry exists to prevent |
| Coordinate preservation | **bit-exact** in/out | safe to feed registry coordinates and read them back as identities |
| Duplicate insertion | returns the **same** handle | dedup is compatible with `NodeKey` welding |
| Insertion-order determinism | forward vs reversed insertion of 200 points → **identical** triangle sets (385 each) | satisfies the determinism contract for the face triangulator |
| Coordinate range | `[1.794e-43, 3.214e60]`; outside → `Err(InsertionError::TooSmall/TooLarge)` | **Rule N13:** insertion results are always propagated, never `unwrap`ed; Rule N1's normalization keeps inputs far inside the range |
| Exactly collinear input | accepted, 0 inner faces | degenerate patches produce no triangles rather than failing |
| Dimensionality | strictly 2D (`Point2`) | usable for the coplanar overlay (§10.3) and for per-face constrained triangulation in a projected plane; **not** usable for the junction-cell interior, which needs 3D constrained tetrahedralisation and stays hand-rolled (G0-1 §7) |

### 7.3 Double-double: hand-rolled — **adopted** over `twofloat` 0.8.4

- Correctness: hand-rolled and `twofloat` agree **bit-for-bit** on 200 000
  `x·y + 1` evaluations (0 differences).
- Speed: hand-rolled is **1.5×** faster on a 10⁶-term fused multiply-add chain
  (5.2 ms vs 7.7 ms).
- Scope: we need `two_sum`/`two_prod`/add/sub/mul only — about 40 lines. The
  crate's transcendental surface is unused weight.
- Decision: **hand-rolled**, zero new dependencies, consistent with the VTU
  precedent (plan §7.3).

### 7.4 Dependency delta

`robust` and `spade` are added; `smallvec` as already planned (plan §16). No
double-double crate, no bignum crate, no `rug`/GMP (C dependency, licence, and
build-complexity cost for a path DD already covers).

---

## 8. Determinism rules

### 8.1 Strict mode (default)

1. Every committed number is computed on the CPU in f64 (or DD) with exact
   predicates. GPU results enter only as candidate sets, rankings, and flags.
2. **Reduction order is fixed.** Float reductions MUST NOT use `rayon`'s adaptive
   splitting (`par_iter().sum()`): its split points depend on work stealing and
   thus on thread count and timing. Use a deterministic tree over a
   compile-time-constant chunking, or accumulate per-chunk into an indexed buffer
   and combine in index order.
3. **No iteration-order leakage** (Rule N7).
4. **No transcendentals in any committed decision.** `sin`/`cos`/`tan`/`atan2`/
   `exp`/`log` are not correctly rounded and vary across libm versions and
   targets. Angle gates are algebraic: compare `cos²θ` against a precomputed
   constant using squared dot products and squared norms. `sqrt` is exempt — IEEE
   754 requires it correctly rounded — but is still avoided where a squared
   comparison works.
5. **Element and node ordering** follow G0-1 §1.4, with a canonical re-sort
   before export so byte comparison is meaningful ([V11] strict).
6. **ASCII output** uses Rust's shortest round-trip f64 formatting, so
   write → read → write is byte-stable (the GA-1 contract).
7. Two runs, any backend mix, any device, any thread count → **bit-identical**
   primary VTU.

### 8.2 Fast mode (opt-in)

GPU numeric results may commit. Guaranteed identical: topology, region and
partition labels, tag tables, conformance. Coordinates within `1e-6·diag`,
quality metrics within 1 % ([V11] topology mode). Rules 3–6 above still apply —
fast mode relaxes *precision*, never *reproducibility of structure*.

### 8.3 Cross-device

Cross-device GPU comparison runs under the fast contract only. f32 arithmetic is
IEEE-754 on all supported adapters, but reduction order, `fma` availability, and
transcendental implementations are not portable; strict mode is unaffected
because no GPU number is committed.

---

## 9. Test obligations

| ID | Test | Asserts | Lands with |
|---|---|---|---|
| T-N1 | orientation wrapper | `orient3d` wrapper sign matches G0-1 §1.1 on a known positive tet; a raw `robust::orient3d` call in `src/meshgen/` fails review | G2-1 |
| T-N2 | static filter | on adversarial inputs, no case where the filter passes and the sign is wrong; measured error `≤ c_flt·perm` | G2-1 / GK-1 |
| T-N3 | escalation | constructions with `ρ < κ_esc` escalate; DD result within `0.25·q` of exact; the `ρ < κ_esc·u_dd/u` case routes to typed coincidence/degradation; C3 checks both edge ratios | G2-1/G2-3 |
| T-N4 | DD primitives | `two_sum`/`two_prod` are error-free on randomized input; DD `orient3d` sign matches `robust` on 10⁵ near-degenerate cases | G2-1 |
| T-N5 | normalization | the same geometry translated by `1e6` and normalized produces bit-identical output (this is the scale-invariance fixture of plan §17.3 with translation added) | G1-2 |
| T-N6 | registry identity | same provenance ⇒ same vertex; adjacent triangles agree; ids are assigned in canonical-key order | G2-1 |
| T-N7 | container determinism | no `HashMap` with default hasher reaches an emitting iteration (enforced by a lint/test over the module) | G2-1 |
| T-N8 | broad-phase superset | inflated f32 candidate set ⊇ exact f64 candidate set over randomized scenes | GK-1 |
| T-N9 | GWN certificate | pairwise reduction; classification outside the band matches f64; band shrinks as predicted with `n` | G2-4 / GK-1 |
| T-N10 | no transcendentals | angle gates give identical results to a `cos²` reference; a grep-level test forbids `acos`/`atan2` in decision paths | G6-2 |
| T-N11 | reduction determinism | float reductions give identical results across 1, 2, 8, 16 threads | GK-3 |
| T-N12 | spade guards | crossing constraints refused via `try_add_constraint`, never panicking; insertion errors propagated | G2-2 |

---

## 10. Verification record

Programs: `verify_numerics.py` (exact-rational ground truth via
`fractions.Fraction`) and the `crate_eval` Rust project (real crates, release
build). Session scratchpad; §9 is their permanent form.

**[N1] C1 error, relative to segment length** — table in §4.2, plus the
translation study in §1.1. 3 000 cases per family per offset.

**[N2] C2 conditioning** — worst absolute position error per decade of
`1/|det|`, coordinates `O(1)`: `2.5e-15` at `1e0`, `2.3e-13` at `1e1`,
`2.7e-12` at `1e2`, `2.1e-11` at `1e3`, `1.5e-10` at `1e4`, `1.8e-9` at `1e5` —
i.e. error `≈ 22·u·cond`, linear in conditioning as modelled.

**[N3] Escalation arithmetic** — §4.3's table.

**[N4] Static filter** — 40 000 adversarial cases per precision: f64 bound
violations 0, worst `err/(u·perm)` = 3.23, filter routed 13.7 % to exact, wrong
signs passed 0. f32: violations 0, worst 3.54, routed 63.4 %, wrong signs passed
0. Realistic S6 workload: 600 000 edge tests, 2 uncertain (`3.3e-6`).

**[N5] GWN** — §6.3's table; closed spheres of 6 400 / 57 600 / 360 000
triangles, interior query, `w_exact = −1.000000` recovered.

**[N6] f32 conversion** — worst relative `|f32(x) − x|` over 2·10⁵ samples
spanning six decades: `5.9422e-8` vs the bound `5.9605e-8`.

**[E1] `robust`** — 200 000 near-degenerate `orient3d`: naive wrong 33 879,
`robust` wrong 0, DD wrong 0. Sign convention measured opposite to G0-1's.
Timings over 2 M calls: naive 12.5 ms, `robust` 16.5 ms (1.3×), DD 80.2 ms
(6.4×); on near-degenerate input 11.0 / 16.2 (1.5×) / 84.3 ms (7.6×).

**[E2] DD vs `twofloat`** — 200 000 evaluations, 0 differences; 10⁶-term chain
5.17 ms vs 7.72 ms (1.49×).

**[E3] `spade`** — the table in §7.2, all rows executed.

**[G2-3] arrangement degeneracy gate (2026-07-28)** — 250 seeded
near-degenerate arrangements were each executed twice: 250/250 bit-structurally
identical, 500 DD escalations, 0 DD-floor routes, 0 degraded cases, 0 hard
failures, and 0 G2-2/G2-3 `NotAvailable` routes. Separate fixtures exercise the
DD floor, q-order/collapsed contact, residual/radial fallback records, epsilon
boundary, tilted C7, and a transitive C7 chain. Gate decision: **GO**. The gate is
validity/determinism based rather than a workload-independent maximum DD rate;
adversarial input is allowed to use DD or a typed fallback, but never to guess or
silently disappear.

---

## 11. Deviations and open items

| # | Deviation | Reason |
|---|---|---|
| D-1 | The plan's GPU classification margin **`δ_gpu = 4·ε` is not a parity certificate** and is replaced by the per-test static filter (§6.1) | Parity errors come from a ray passing near a triangle *edge*, which can happen at any distance from the surface; a band around the surface cannot bound it. The static filter is a true certificate, and measured at `3.3e-6` uncertain it is also cheaper. The `4·ε` band is **retained** as an additional unconditional routing rule for vertices near a surface, where the geometric answer is meaningless anyway and snapping will move the node |
| D-2 | Escalation triggers on a per-construction **stability ratio `ρ`**, not on a post-hoc error estimate of the constructed coordinate | Measured: the determinant-ratio form with f64 determinants is as wrong as the naive form in the near-parallel family (`4.3e-4`). The decision must be made on the denominators *before* the value is formed |
| D-3 | **Coordinate normalization (Rule N1) is mandatory**, not advisory | Measured 5–6 orders of magnitude of accuracy at realistic coordinate offsets — larger than every other effect in this document combined |
| D-4 | `robust::orient3d`'s sign is **opposite** to G0-1 §1.1; a single negating wrapper is mandated | Measured. Direct use inverts every tet, and the failure is silent until a volume check |
| D-5 | `spade::add_constraint` banned (panics); `add_constraint_and_split` banned outright | A mesher must not abort on a geometric input; and split-constructed intersection points would bypass the registry, reintroducing the cross-triangle disagreement the registry exists to eliminate |
| D-6 | Double-double is hand-rolled; `twofloat` rejected | Bit-identical results, 1.5× faster, ~40 lines, zero new dependencies |
| D-7 | GWN **must** use pairwise reduction and accumulate `Σ\|ω\|` | Sequential f32 is ~300× less accurate and degrades with `n`; the certificate needs `S`, which only the kernel can produce cheaply |
| D-8 | Angle gates are algebraic (`cos²`), never `acos`/`atan2` | Transcendentals are not correctly rounded and vary by platform and libm version; the plan's 8°/10°/45° thresholds would otherwise be non-deterministic decision sites |
| D-9 | Float reductions may not use `rayon`'s adaptive splitting | Its split points depend on thread count and work stealing, which breaks bit-identical strict mode |
| D-10 | Registry containers must be `BTreeMap` or fixed-seed + sorted | Rust's default `HashMap` iteration order varies between runs of the same binary |
| D-11 | The implemented/default `eps_frac` is `1e-4`, not the original sketch's `1e-3` | `1e-3` violates the frozen parse-time cap under the default sizing/gap factors; all q/kappa/DD-floor examples now distinguish the actual default from the optional configured value |
| D-12 | C3 applies Rule N5 to both defining-edge parameter ratios | Either ratio is later consumed for exact edge order; checking only the coordinate-producing edge can commit an unstable ordering ratio |

Open items handed on:

- **Predicate performance at scale** — the `1.3–1.5×` cost of `robust` was
  measured on 2 M synthetic calls; the S2/S6 in-situ ratios are a **GK-1**
  acceptance measurement.
- **Junction-cell 3D constrained tetrahedralisation** has no adopted crate
  (spade is 2D); its viability is **gate G6-0**, with G0-1 §7.6's curve-pinned
  fallback if it fails.
- **In-situ DD/fallback rates beyond the G2-3 corpus** remain reporting metrics,
  not correctness gates. The boundary and named fallback triggers are frozen by
  the [G2-3] record above.
