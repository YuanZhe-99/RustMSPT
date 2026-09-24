# SPEC — Mesh generation: numerical robustness freeze (subtask G0-2)

**Status:** frozen (rev 1.3, 2026-09-23 — audited against the code at `891badc`; §1.3 names every as-built constant outside the ladder; §1.5 freezes the four roles of a coordinate — identity key, ordering key, geometric coordinate, error metric; §6.1's certificate G1 is qualified for converted inputs (D-13); §10 gains the audit record; §11's open items are re-stated. rev 1.2; G2-3 boundary/default/C3 dual-ratio record added 2026-07-28). Normative for all `src/meshgen/` and
`src/gpu/meshgen_*` work.
**Date:** 2026-07-24 (rev 1.3: 2026-09-23)
**Subtask:** G0-2 (Phase G0, tier T3) of [`PLAN_mesh_generation.md`](PLAN_mesh_generation.md); the rev 1.3 audit is that plan's M-0.1.
**Scope (from the plan's G0-2 acceptance):** predicate inventory; registry
provenance keys; construction error bounds + double-double escalation
thresholds; GPU margin certificates; strict/fast determinism rules; crate
evaluations (`robust`, `spade`). *Acceptance: every decision site lists its
predicate; bounds derived; choices recorded.*
**Companion freezes:** [`SPEC_meshgen_geometry.md`](SPEC_meshgen_geometry.md)
(G0-1 — templates, tables, conformity; this document supplies the arithmetic
every one of its predicates runs on), G0-3 (VTU schema, check catalog, accuracy
table).

> **Plan cross-references (rev 1.3, 2026-09-23).** This document was frozen against the rev-2
> plan, whose section numbers it cites as "plan §N" / "§N of the plan". That document was deleted
> on 2026-09-01 (plan D-2) and replaced by `PLAN_mesh_generation.md` v3 (rev 3.1). Every such
> citation resolves through the plan's **Appendix B.0** (a map from the old section numbers to
> where the content lives now) and **Appendix A** (the record's findings, keyed by the old section
> numbers). The plan's own ids are `M-n.m` (subtasks), `D-n` (owner decisions), `MG-nn` (the
> 2026-09-11 review's findings) and `X-n` (measured-and-closed routes).

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
  `src/meshgen/` is a review failure. *Rev 1.3:* the audit found the further named constants of
  §1.3; each is now listed with its role, and the rule applies to anything not in that list.
- **Frozen is a statement about change control, not about correctness** (plan R8): a clause an
  audit shows to be insufficient is amended with a §11 row and a §10 record. Rev 1.3 is the
  first such audit; its one substantive finding against this document is D-13.

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

> **As built (rev 1.3).** The pipeline applies `x̂ = (x − domain_min)/diag` once, so the domain
> box becomes a box at the origin whose **diagonal** is 1 — not the unit cube (a cubic domain
> maps to `[0, 1/√3]³`); rev 1.2's "becomes the unit cube" was loose wording and the formula was
> always right. S2 then builds its own `Normalization` from that already-normalized box, whose
> scale is `1` only up to rounding — one ulp off for about 38 % of domains (`(1, 1, 0.35)` gives
> `0.9999999999999999`) — so S2's frame is not bit-identical to S0's and S3's; harmless for the
> predicates, recorded so that nobody reads it as a coordinate-frame bug. A stage receiving
> normalized input SHOULD NOT renormalize (plan M-3.3).

### 1.2 Constants

| Symbol | Value | Meaning |
|---|---|---|
| `u` | `2⁻⁵³ ≈ 1.11e-16` | f64 unit roundoff |
| `u₃₂` | `2⁻²⁴ ≈ 5.96e-8` | f32 unit roundoff. *As built:* used by no live code; the only f32 roundoff in the tree is `topo::gwn_margin_band`'s `2⁻²³` (= `2·u₃₂`), a helper with no caller |
| `u_dd` | `≈ 2⁻¹⁰⁶ ≈ 1.23e-32` | double-double unit roundoff |
| `ε` | `eps_frac · 1` (normalized) — implemented default `1e-4` | envelope / coincidence tolerance |
| `q` | `0.1 · ε` — default `1e-5` | weld quantization step; `NodeKey` grid (G0-1 §1.2). *As built:* S0 and S2 only; S7 keys and welds at `ε`, S8 orders at `1e-6·ε` (geometry D-35). `q` quantises **identity** everywhere and **coordinates** only for S2-constructed registry vertices (committed at `key·q`); input vertices keep their raw coordinates (§1.5) |
| `κ_esc` | `4·c·u/q`, `c = 7` for C1 and C3 — default `3.1e-10`; `c = 25` for C2 — `1.1e-9` (D-20) | escalation trigger on the stability ratio (§4.3); written inline three times in `predicates.rs`, not as a named constant |
| `c_flt` | `(7 + 56u)·u = 7.772e-16` (f64), `4.172e-7` (f32) | orient3d static-filter constant (§6.1) |
| `δ_bp` | `2⁻²³ · max\|coord\|` | broad-phase AABB inflation (§6.2) — GPU only, **not implemented**; the CPU broad phase inflates by `ε` in f64 |

Load-bearing ordering, checked at parse time **and** re-asserted on the realized
field (G0-1 §11.4): `q ≪ ε ≪ t_sheet < t_layer ≤ h`. *As built (rev 1.3):* the parse-time
check is `ε < 0.5·τ_s·h_min` with `0 < τ_s < τ_l`; the realised re-assertion covers
`ε < 0.5·t_sheet < t_layer ≤ h` on the converged scalar `h` only (`q ≪ ε` is `q = 0.1·ε` by
construction and is not re-asserted; `τ_l ≤ 1` is caught only there — geometry §11 as-built
note).

### 1.3 Named constants outside the ladder — as built (rev 1.3)

§0 says the constants of §1.2 are the only tolerances the mesher may contain. The audit
(python scan of every non-test literal in `src/meshgen/*.rs`, 222 hits, then read site by site)
found the following further constants and literals in the meshing stages, each with a purpose.
They are recorded so that §0's rule has a complete list to be checked against; a constant with a
**decision** consequence is class **T** or **A** in §2's terms and is marked, the rest bound
searches or reports. Config fields are given with their defaults. Verifier tolerances live in
contracts §4, not here (§0's rule is about the meshing stages).

| constant | value | stage / site | role |
|---|---|---|---|
| `KEY_ORDER_REFINEMENT` | `1e-6` (× `ε`) | S8 ordering key and arena weld | Rule K-O grid — geometry D-35 (`1e-6·ε`, not `1e-6·q`) |
| §7.6 fragment-kernel arena quantum; clip side `tol` | `CUT_VOLUME_TOLERANCE · 1e-6 = 1e-8`; `1e-9` — both **absolute** in the normalized frame | S8 default junction path | **unit error, open item:** a volume fraction reused as a length; neither scales with the cell (plan M-3.3) |
| `CUT_VOLUME_TOLERANCE` | `0.01` | §6 guarded dry-run; §7.6 volume guard (`max(·, 1e-12)`) | relative volume closure — **T** |
| `BAND_VOLUME_TOLERANCE` | `0.01` | §8.2 band rows against the cell's own boundary | relative volume closure — **T** |
| `CUT_MIN_DIHEDRAL_DEG` / `meshgen.thin.band_min_dihedral_deg` | `8°` (config, `(0°, 70.5°)`), compared in **degrees via `acos`** | band rows only (geometry D-29) | quality floor — **A** (not the frozen `cos²` form) |
| `BAND_MAX_AR`, `BAND_EXPLICIT_ALTITUDE_RATIO`, `BAND_REGIONAL_FAILURE_SHARE` | `20`, `0.05`, `0.05` | geometry §8.4 ladder; the share has no pipeline caller | quality gates — **A** |
| `SNAP_MOTION_CAP` | `0.30` (× shortest incident lattice edge, total displacement from the S5 position) | S7 | a target beyond it is **ignored**, not clamped — **A** |
| `SNAP_RECHECK_LOW` | `0.025` (of the edge) | S7 re-check band (K2) | promote the endpoint — **T** |
| `WEIGHT_CORNER` / `WEIGHT_CURVE` / `WEIGHT_SURFACE` | `1e7` / `1e4` / `1` | S7 target priority | rank by kind, then nearest by f64 distance, then `NodeKey` at `ε` — **I**/**A** |
| `hysteresis_enter` / `hysteresis_leave`; `h_tolerance`; `max_iterations` | `0.9` / `1.1`; `0.05`; `5` | S3 segmentation, S3↔S4 loop | regime dead band — **T**; stop rule; cap |
| `meshgen.sizing.gap_cells` | `4` (config; parse-time floor 4) | `C(R)`'s LFS term and `lfs_floor = max(ε, gap_cells·h_min)` | representability threshold, not a tuning |
| `curve_cells`, `feature_angle_deg` | config (`45°`, compared as `cos²`) | S4 curve sources; S1 sharp edges | sizing / classification — **A** |
| S1 corner turn threshold | `60°` (**literal**, `cos²` compare, **no sign guard**) | S1 corner detection | a reversal of more than 120° is *not* a corner — **A**, defect (D-21) |
| `GRADIENT_LIMIT`; `CROSSING_SHARE`; `MUTUAL_RATIO`; `MUTUAL_RADIUS_FACTOR` | `0.5`; `0.05`; `0.3`; `0.5` | S3 densification; pairing battery | **A** |
| `COS_OPPOSING`; `COS_OPPOSING_INTRA` | `0.5` (a 60° cone, cross-component); `0.9` (same-component) | S3 facing tests on unit normals | **A** — §2's "`dot < 0` with a margin" is a cone test as built |
| S3 densification floor; `cos2_limit` | `max(1e-3·h_bootstrap, ε)`; `0.25` | S3 | **A** (literals) |
| `LFS_COVER_TOLERANCE` | `0.5` (the realised field within `1.5×` of the request) | S4 gap sources | documented bound |
| `K1_MAX_PASSES` | `3` | the pipeline's K1 loop around S4–S7 | pass cap |
| `LATTICE_MAX_TETS` | `20 000 000` | S5 | budget (geometry §3.5) |
| `RAY_DIRECTIONS` | five fixed directions | S6 | the re-shoot sequence (ARB-9) |
| S6 plane-zero perturbation filter | `4·EPSILON` relative (`8u`, on an f64 normal) | S6 `sign(n·dir)` | **A** inside an **X** decision (§2 as-built row) |
| `ACTIVE_FACE_PROBE` | `1e-3` (× `√\|n\|`, off four barycentric samples) | S6 active-face test on a self-intersecting own component | winding-number probe — **A** |
| S2b GWN | `\|w\| > 0.5`, sequential f64 sum, `denom < 1e-30` guard | S2b open-solid classification | **A** (transcendental `atan2`); no band |
| `ON_FACE_FRAC`, `ON_EDGE_FRAC`, `PLANE_FRAC`, `INSIDE_FRAC` | `1e-9`, `1e-6`, `1e-5`, `1e-9` (relative) | §7.6 cap and conformity guards; `segment_pierces_triangle` | side / containment tests — **T** (geometry D-26) |
| fragment area threshold | `edge · 1e-6` (squared) | §7.7 B4 | measure-zero contact is not fragment — **T** |
| §7.4 kernel slacks | `1e-9` barycentric point-in-facet; `scale · 1e-6` duplicate node; `1e-18·len²` on-segment; `1e-9` relative volume/area conservation | `cdt.rs` (gated path and the default fragment kernel) | **T** — the exact predicates are used for `orient`/`insphere`/`incircle` only |
| `trace_on_face` tolerance | `shortest_edge · 1e-9` | §7.7 C1 | on-plane test — **A** |
| T-junction detector bound | `1e-9 · diag` | §7.7 C6 (gated path) | `[V3]`'s own `hanging_tol_frac` |

The list is what the audit found by reading; plan M-3.3's refactor of `cut_lattice` is where each
is either named in code or retired. §0's rule stands: a new literal in the meshing stages without a
row here is a review failure.

### 1.5 The four roles of a coordinate — frozen (rev 1.3, plan MG-11)

Four things are done with a point's coordinates in this module, and they are **not** the same
quantity. An argument that moves a point on the strength of a tolerance from one role is invalid
in every other role.

| role | what it is | who owns it | may it move the point? |
|---|---|---|---|
| **identity key** | `NodeKey(p)` on the weld grid `q` (§1.2; G0-1 §1.2) and the provenance keys of §5. Decides whether two points are *the same node* | S0 (weld), S2 (registry), S7/S8 (interning by provenance) | **No.** S0 merges vertices whose keys coincide and keeps the **first** vertex's raw coordinates; a quantised key is never written back as a coordinate. "Mesh nodes lie on the q grid" is therefore not an invariant of this module — only their *keys* do — and Rule K-O already admits constructed points closer than `q` |
| **ordering key** | the key a stage sorts or tie-breaks on: the weld grid for welded nodes, `1e-6 · q` for S8's constructed crossings (Rule K-O), the Morton key for lattice cells | each stage, per G0-1 §1.2–§1.4 | **No.** An ordering key is read, never applied |
| **geometric coordinate** | the f64 (or DD-constructed, §4) position a predicate is evaluated on and a file records | S0 (input), S2 (constructions C1–C3), S5 (the exact `h_min/2` integer grid), S7 (snap targets C4), S8 (averages C5) | **Only by a construction with a provenance** (C1–C5) or by an S7 snap that passes the exact inversion test. Never by rounding to a key, and never by a distance that some *other* role tolerates |
| **error metric** | `ε` (coincidence), `interface_on_surface_frac` and `interface_offset_frac` (contracts §5, fractions of the **local** edge), the verifier's `duplicate_node_tol_frac` / `hanging_tol_frac` / `plane_tol_frac` (fractions of the diagonal) | the verifier and the coincidence policy | **No.** A metric says whether a check alarms; it says nothing about whether the point is on the surface, on the lattice, or distinct from its neighbour |

Three consequences, each of which the plan's earlier text got wrong (plan MG-11):

- **The q grid is not the lattice grid.** Lattice nodes lie on the integer grid of step `h_min/2`
  in the normalized frame; the weld grid is `q = 0.1·ε` in the same frame; the two are
  incommensurate. An input plane on a lattice plane at `x = 0.5` (model units, unit-cube domain)
  is `0.5/√3 = 0.28867513…` normalized and, rounded to `q = 1e-5` and mapped back, is
  `0.5000084271289835` — no longer on the lattice. Quantising input coordinates to `q` therefore
  *breaks* the alignment plan D-3 keeps; it is not a rule this document permits.
- **A normalized length and a fraction of a local edge are not comparable without the edge.**
  `q/2 = 5e-6` of the diagonal against `[V13]`'s `0.02` of a face's edge is `(q/2)/(0.02·h_local)`,
  which is 4,000 only when `h_local = 1/16` of the diagonal and shrinks with every smaller cut face;
  and per-axis rounding by `q/2` displaces a point by up to `√3·q/2` in 3-D.
- **"Invisible to the verifier" is not "on the surface".** A trace point moved to a nearby edge
  because `[V13]` would not alarm has been moved off the constraining surface by an unmeasured
  amount, and can merge two entities the arrangement keeps distinct. Any operation that changes a
  geometric coordinate MUST justify the new position under the geometric role — by a construction
  with a provenance or by an exact test against the constraining entity — and MUST report the move.

> **Rule N14.** No operation may write a quantised or ordering key back as a geometric coordinate,
> and no operation may move a geometric coordinate on the strength of an error-metric tolerance.
> Conditioning transforms (repair, decimation — plan M-4.5) are the exception and are **declared**:
> they record the original and transformed geometry and the displacement, and P3 is then measured
> against the transformed surface (plan D-6).

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
| S1 | curve junction / corner | valence `≥ 3`; turn angle by `cos²` comparison (as built: against a literal `cos²(60°)` with **no sign guard**, so a turn sharper than 120° reads as straight — D-21) | **I** / **A** | — |
| S2 | triangle-pair crossing | 6× `orient3d` sign pattern | **X** | — |
| S2 | coplanarity | 4× `orient3d` all `= 0` | **X** | routes to the 2D overlay path |
| S2 | 2D overlay orientation, point-in-triangle | `orient2d` | **X** | — |
| S2 | segment ordering along an intersection line | compare exact barycentric parameters on the owning edge (as built: along a *source* edge, the sign of a DD determinant-ratio cross-difference (`DeterminantRatio::compare`), falling back to the f64 dominant-coordinate key of the committed point when either side has no ratio; along a *pair* segment and a clipped edge, the f64 dominant coordinate of the snap-rounded points; a gap `≤ q` → `QuantizedOrderAmbiguity` — D-18) | **X** (as built **A**/DD) | — |
| S2 | registry entity identity | provenance key equality (§5) | **I** | — |
| S2 | snap-round of a registry vertex | `NodeKey` quantization | **I** | — |
| S2 | coincidence within `ε` | distance² vs `ε²` | **T** | policy table (G0-1 §10) |
| S2b | component connectivity, closure | combinatorial (edge adjacency, Euler) | **I** | — |
| S2b | radial patch ordering around a curve | `orient2d` in the plane ⟂ to the curve tangent (as built: `orient3d(a, b, ref, third)` about the axis, which is that predicate) | **X** | a zero in the half-assignment test → a `RadiallyCoplanar` degraded neighbourhood (ARB-2), not a tie-break; a tie *inside* one half → smallest `NodeKey`, then face id |
| S2b | GWN inside test | `w > 0.5` with the §6.3 band (as built: CPU f64 only, `\|w\| > 0.5` at the component's vertex-mean centroid, a **sequential** sum, no band; `\|w\| ≤ 0.5` reclassifies the component `Sheet` silently) | **F** (as built **A**, `atan2`) | CPU f64 → WARN + defect report |
| S3 | separation comparisons | squared distances | **A** | — |
| S3 | opposing-normal condition | `dot < 0` with a relative margin (as built: a facing **cone** on unit normals — `\|d·n\| ≥ COS_OPPOSING = 0.5` cross-component, `≥ COS_OPPOSING_INTRA = 0.9` same-component — and the outward signs must oppose when both clear the cone) | **A** | pairing battery / confidence gate |
| S3 | regime thresholds | `t_r` vs `τ·h`, hysteresis 0.9/1.1 | **T** | G0-1 §11 ladder |
| S4 | sizing constraints, 2:1 gradation | f64 min; integer level compare | **A** / **I** | — |
| S5 | split(face) / split(edge) | lattice-node membership of centre/midpoint (G0-1 §3.1) | **I** | — |
| S5 | Morton order, balance | integer | **I** | — |
| S5 | template positivity | `orient3d > 0` (bounded below by `h³/48`, G0-1 §3.4) — as built the exact `i64` determinant `orient3d_index` | **I** | no runtime assertion: the orientation fix swaps on `≤ 0` and would emit a zero-volume tet; positivity rests on the frozen tables and their tests, and the integer grid makes zero unreachable |
| S6 | parity classification | `orient3d` × 4 per ray-triangle test (as built: **five** — two plane, three line — through the f64 static filter, then `robust`; no GPU tier) | **X** | filter-uncertain → exact, counted `n_filter_uncertain` |
| S6 | ray degeneracy — vertex on a triangle's *plane* | filter returns exact `0` in a plane test | **X** with an **A** resolution | symbolic perturbation `p + δ·dir`: `sign(n·dir)` on an f64 normal, certified by a `4·EPSILON` relative filter on the dot product only (D-16, D-23); undecided → re-shoot |
| S6 | ray degeneracy — ray through an *edge or vertex* | exact `0` in a line test | **X** | abandon the ray; re-shoot over `RAY_DIRECTIONS[0..5]`; exhausted → winding number (ARB-9, `[CLS-BAND]`) |
| S6 | active surface patch *(as built)* | centroid parity against higher-priority solids; for a self-intersecting own component, the winding number at `±ACTIVE_FACE_PROBE·√\|n\|` off four barycentric samples, all of which must be buried | **X** / **A** | the face stays active (conservative) |
| S7 | edge crossing exists | `orient3d` sign change along the edge | **X** | — |
| S7 | snap target priority | corner > curve > surface, then ascending `NodeKey` (as built: kind weight `1e7 / 1e4 / 1`, then the **nearest by f64 distance** within a kind, then `NodeKey` at `ε` on an exact tie — geometry D-35; surface targets are taken only through the 2.5 % re-check band, K2) | **I** / **A** | — |
| S7 | K2 weld; duplicate crossing; re-check band *(as built)* | `t·len ≤ ε` promotes a crossing onto its endpoint; `\|Δt\|·len ≤ ε` merges two crossings of one component; `t < 0.025` or `> 0.975` promotes the endpoint onto the patch | **T** | — |
| S7 | snap would invert | `orient3d > 0` on every incident tet | **X** | reject move (ARB-10) |
| S7 | motion cap | `‖Δ‖ ≤ 0.3·L_min` (total displacement from the S5 position, unsquared) | **A** | as built **no clamp**: a feature target beyond the cap is ignored and a surface target is dropped as a re-check residual; ARB-11's clamp branch is unreachable (D-22) |
| S8 | cell triage, face-split dispatch | counting | **I** | — |
| S8 | quad diagonal (Rule SNK) | `NodeKey` comparison | **I** | — |
| S8 | emitted-tet validity | `orient3d > 0`; `cos²` dihedral vs `cos²(8°)` (as built: the dihedral floor is applied to band rows only; §6 pieces are checked for orientation and 1 % volume closure — geometry D-29) | **X** / **A** | §4.4 ladder (ARB-17) |
| S8 | corner votes / cut sides | cached `side_of` lookups (as built: S6's vertex sides plus S7's on-cut set; a side/crossing disagreement **escalates** the cell rather than arbitrating — geometry §6 inputs, D-31) | **I** | ambiguous → arbitrate (ARB-12; not built) |
| S8 | oriented probe | `robust_inside(c + η·n)` (as built: the table-vs-interior check samples each child's **centroid** with the exact classifier and escalates on a certain disagreement — geometry §6) | **X** | arbitrate (ARB-13; not built) |
| S8 | junction sub-region seeding | 5-ray `robust_inside` (as built: one sample per region on the gated Meshed arm, one per **tet** on every fan arm, every solid component asked — geometry §7.5 as-built note) | **X** | an uncertain answer is *no side* and is reported (`[JCT-SEED]`), never guessed |
| S8 | guarded dry-run | all-positive + `\|ΣV − V_parent\| ≤ 1%·V_parent` | **X** / **A** | escalate (ARB-15) |
| S2 | point-in-triangle band *(as built)* | `\|orient2d\| ≤ q·\|edge\|` on the magnitude `robust` returns | **T** (D-18) | C2 point accepted / rejected on the constructed point |
| S8 | junction kernel Delaunay tests *(as built)* | `insphere`, `incircle_axis` (winding-normalised wrappers) | **X** | — |
| S8 | junction kernel membership / conservation *(as built)* | relative slacks `1e-9` (barycentric), `scale·1e-6` (duplicate node), `1e-18·len²` (on-segment), `1e-9` (volume / area conservation) | **T** | decline the cell (geometry §7.7 C4) |
| S8 | junction piece acceptance *(as built)* | `Σ fan_volume` vs `V_parent` within `CUT_VOLUME_TOLERANCE` | **A** | decline (geometry §7.7 B4) |
| S8 | curve pierce point *(as built)* | plane-normal ray/triangle with `INSIDE_FRAC` slack — decides whether a face gets a Steiner node | **T** | no pierce → the face centroid apex |
| S8 | cap in a parent face plane; fan swallows a loop vertex; cell-fan conformity *(as built)* | `ON_FACE_FRAC`, `ON_EDGE_FRAC`, `PLANE_FRAC` | **T** | another apex; decline (geometry §7.7 B4 f) |
| S8b | band `k`, template choice | counting | **I** | — |
| S8b | predicted band quality gates | algebraic (altitude, AR, `cos²` dihedral) — as built the dihedral is compared in **degrees via `acos`** | **A** | ladder (ARB-20) |
| S9 | IQD defect classification | algebraic metrics *(S9 not built — plan M-5.4)* | **A** | — |
| S9 | smoothing acceptance | `orient3d > 0` + algebraic quality, recomputed in f64 (strict) | **X** / **A** | reject move |
| S9 | collapse guard | record-level side comparison | **I** | reject (ARB-21) |
| S9 | Phase-5.5 snap | distance vs `2%·h`; `AR < 10`; `cos²` dihedral vs `cos²(10°)` | **T** / **A** | reject move |
| S10 | `resolve()` | integer set operations on X-values (as built: runs at S6 for the preliminary key and in S8's `cut_to_doc` for the final one; an `ambiguous` entry is **dropped**, not an error — geometry D-33) | **I** | ambiguous → error (ARB-23; not built) |
| S10 | partition flood fill, numbering | combinatorial; ascending `NodeKey` *(not built — `partition_id` is written as `0`; plan M-6.1)* | **I** | — |
| S11 | VTU ascii round-trip | shortest round-trip f64 formatting (Rust `{}`) *(built, in `io/vtu.rs`; the ascii mode is the fixture/debug encoding)* | **I** | — |
| S11 | INP material mapping | region-key lookup *(not built — plan M-6.2)* | **I** | hard error (ARB-25) |

> **Rule N2.** No topologically load-bearing decision may be class **A** or **T**.
> Classes **A**/**T** appear only where a wrong answer costs *quality or a
> classification that a later exact check re-validates*, never mesh validity.

> **Rule N2 as built (rev 1.3).** Two sites decide a *side* with floating-point arithmetic:
> the §7.7 B4 clipping kernel classifies a vertex against a plane by signed distance against
> `tol`, and the §7.2 kernel's `orient` is a float triple product (geometry D-26, D-36). Both are
> covered by an exact check downstream — every emitted tet passes the exact `orient3d`, every
> split piece must close and its volumes must sum — so validity is not at stake; they are
> recorded so the claim "every sign decision is exact" is not made of code that does not meet it.
> The §7.3 cache's canonical winding and outward re-winding are float sign tests as well.

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

> **As built (rev 1.3, D-18).** Tier 1 is used where the caller needs the f64 value and
> permanent as well as the sign (`orient3d_filtered`, 22 call sites); the other `orient3d` sites
> and every `orient2d` site go straight to tier 2 — tier 1 is an optimisation, never a
> correctness requirement. Rule N3 is relaxed in four places: `tet_signed_volume` returns
> `robust`'s adaptive magnitude / 6 and is used as a number by `[V4]`'s quality, the band
> dry-run's enclosed volume and the gated kernel's thin-tet refusal; `classify_point_in_triangle`
> compares `|orient2d|` to a `q·|edge|` band; and two S2 orderings take their sign from a
> constructed value (§2's S2 ordering row). Each is a tolerance test or a `q`-bounded ordering,
> not a sign decision, which is why validity does not rest on it.

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

*As built (rev 1.3):* `add_dd` is the accurate double-double sum (two `two_sum` passes plus
renormalisation); `mul_dd` is `two_prod` of the high words plus the f64 cross terms
`hi·lo + lo·hi + lo·lo`, renormalised with `two_sum`; `abs_dd` and `is_zero` are exact.
`DeterminantRatio { numerator, denominator }` (DD, denominator made non-negative) is retained
for every committed C1/C3 point for ordering along its edge, and `compare` is the sign of the DD
cross-difference `n1·d2 − n2·d1` collapsed to f64 (D-18). The primitives themselves have no
direct error-free test (T-N4, §9).

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
| **C6** *(rev 1.3, as built)* | downstream crossings — S7 lattice edge × patch, G2-5 edge × box plane, S8 curve × lattice face, the §7.4 kernel's clip and trace points | f64 only, never escalated, identified by `NodeKey` (or lattice edge + `NodeKey`) with no provenance key; S7 uses the C1 determinant ratio clamped to `[0, 1]` (`t = 0.5` on a zero denominator), the others plane-distance ratios; validity is re-established by the exact `orient3d` and `[V1]`/`[V3]` (D-19) |

> **Rule N4.** Intersection parameters are always **ratios of determinants**,
> never plane-normal expressions. `x` is always formed by an affine combination
> of the *defining* points, never assembled from independently computed
> components.
>
> *Scope (rev 1.3, D-19).* N4 governs the S2 registry constructions C1–C3 and S7's edge
> crossing. C6's constructions use plane-normal forms. And C2 as frozen assembles `x` per
> coordinate by Cramer in the local frame of its first triangle's first vertex, so the second
> sentence holds for C1 and C3 only.

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

> **As built (rev 1.3).** (1) C1 and C3 compute their DD determinants on **every**
> construction, because the committed ordering ratio is always DD; the tier rule decides only
> which value produces the coordinate, and the "vanishing fraction" cost claim applies to C2.
> (2) A DD-floor route does **not** go to the coincidence path: it produces a typed
> `DegradedNeighborhood { PrecisionFloor, ρ }` — for C1 the whole triangle pair contributes no
> registry vertex or segment, for C3 the relation is marked unresolved and its proposals rolled
> back, for C2 the triple is skipped — which S7 resolves by alternating projection. A construction
> is also routed (`Deferred`) when `q` is not finite and positive, when a selected-tier denominator
> or its DD recomputation is exactly zero, or when the parameter or point is non-finite, so a
> route may carry `ρ ≥ κ`. (3) `[ARR-PREC]` is printed once per `PrecisionFloor` or
> `QuantizedOrderAmbiguity` neighbourhood; DD escalations are counted in
> `ArrangementStats.precision_escalations` and not logged (geometry §12 ARB-1). (4) The committed
> coordinate of a registry vertex is **snap-rounded**: the input vertex sharing its `NodeKey` if one
> exists, otherwise `key·q`; the `0.25·q` accuracy bound guarantees that the key is right, not that
> the constructed point is stored. Measured (§10 [A1]): the G2-3 corpus's 500 escalations, 0 floor
> routes; on the acceptance cases `[ARR-PREC]` fires 1 time on A-6a and 12 on A-7a, all
> `QuantizedOrderAmbiguity`.

Derivation of `κ_esc`. The construction's position error is bounded by
`L · c · u / ρ` with `L ≤ 1` after Rule N1 and `c = 7` (Shewchuk's orient3d
constant, measured worst case `3.23·u·permanent`, §10 [N4]). Requiring
`≤ 0.25·q = 0.025·ε` gives `ρ ≥ 4·c·u/q`.

| `eps_frac` | `q` | escalate below `ρ =` | equivalently, condition number above |
|---|---|---|---|
| `1e-3` (non-default; rejected by the default sizing/gap ordering) | `1e-4` | `3.1e-11` | `1.4e10` |
| `1e-4` (implemented default) | `1e-5` | `3.1e-10` | `1.4e9` |
| C2 at the implemented default (`c = 25`, D-20) | `1e-5` | `1.1e-9` | `3.9e8` |

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

The registry (the record's S2 design, plan Appendix B.0) is what guarantees two triangles
never disagree about "the same" point. That guarantee is **symbolic**, so the keys must be canonical
and their containers order-independent.

| Entity | Key | Canonical form |
|---|---|---|
| intersection vertex, edge × triangle | `EdgeTri { e, t }` | `e = (min(v₀,v₁), max(v₀,v₁))` post-weld vertex ids; `t` = stable input triangle id |
| intersection vertex, coplanar edge × edge | `EdgeEdge { first, second }` | both edges canonical as above, then sorted ascending as a pair |
| intersection vertex, 3 triangles | `TriTriTri([t₀,t₁,t₂])` | the three ids **sorted ascending** |
| intersection segment | `Seg { va, vb, ta, tb }` | `(va,vb)` sorted registry-vertex ids; `(ta,tb)` sorted triangle ids |
| coplanar overlay sub-patch | `Overlay { tris }` | sorted id list of all participating triangles. *As built:* **no such key** — a coplanar relation is recorded per triangle pair; an atomic child face is identified by its sorted output-node triple, and faces sharing one merge into a single `ArrangedFace` carrying the sorted `source_triangles` and per-tag orientations (`merge_atomic_faces`) |
| welded node | `NodeKey` | quantized integer triple (G0-1 §1.2) |

> **Rule N6.** Identity is symbolic first (provenance key), quantized second
> (`NodeKey`), and **never** floating-point equality. A construction that
> produces a point without a provenance key is a defect.

> *Scope (rev 1.3, D-19).* Rule N6 governs the S2 registry. Points constructed after S2 (C6) are
> identified by `NodeKey` alone, or by lattice edge plus `NodeKey`; the §7.4 kernel's `NodeArena`
> interns every clip or trace point by key, and its own note says the quantised key "is the
> whole conformity mechanism on the vertex side". A future revision that wants N6 downstream must
> give those points a provenance (edge id, patch id, curve id).

> **Rule N7 (container determinism).** Registry maps and every other map whose
> iteration can influence output MUST be a `BTreeMap`, or a `HashMap` with a
> fixed-seed deterministic hasher **plus** a sort before any iteration that
> emits. Rust's default `HashMap` uses a randomly seeded `RandomState`; its
> iteration order changes between runs of the same binary and would silently
> break the strict determinism contract.

> *As built (rev 1.3, D-17).* Every mesh-producing module satisfies N7 (`arrange.rs` imports only
> `BTreeMap`/`BTreeSet`; S3–S8 carry no hash container). The **verifier** does not: two of its
> `[V6]` loops iterate a default-hasher `HashMap`, and the report is output (§8.1 rule 3 applies
> to it) — measured non-deterministic on A-3, contracts §4.1 as-built note.

> **Rule N8 (id assignment).** Registry indices are assigned in ascending
> canonical-key order in a single pass after collection, never in first-touch
> order.

---

## 6. GPU margin certificates

The strict determinism contract (plan Appendix B.2) requires that GPU output is a
*conservative proposal*, never a decision. *Rev 1.3:* no meshgen GPU kernel exists; every
certificate below is a contract for plan M-7. Each kernel therefore ships a
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

> **Qualification (rev 1.3, plan MG-09, D-13).** Certificate G1 bounds the arithmetic on the f32
> operands *as given*. When the operands are f64 coordinates converted to f32 for the kernel, the
> conversion itself moves each coordinate by up to `u₃₂/(1−u₃₂)·|x|` (§6.2), and for a
> near-coplanar configuration that displacement can put the fourth point on the other side of the
> plane — a change G1's bound does not contain, because it is not arithmetic error. Measured:
> `a = (0.1, 0.1, 0.5 − 2e-8)`, `b = (0.5, 0.1, 0.5 + 2e-8)`, `c = (0.1, 0.5, 0.5 + 2e-8)`,
> `p = (0.15, 0.15, 0.5 − 1.1e-8)` (all inside the normalized range): the f32 determinant in G1's
> own expansion order is `−3.5763e-9`, the bound `c_flt·perm = 1.5542e-15`, the filter **accepts**
> — and the exact determinant of the f64 inputs is `+1.6000e-10` (§10 [MG-09]). G2 inflates only
> the broad-phase boxes and does not extend to G1. Therefore, for converted inputs, **G1 certifies
> the sign of the f32 configuration, not of the f64 one**, and a kernel that uses it for a
> topological decision is not a conservative proposal. Before any meshgen kernel lands (plan M-7,
> GK-1), rev 1.4 MUST either (a) add the conversion term to the filter — an interval of half-width
> `u₃₂·|x|` on each converted coordinate, propagated through the same expansion into the permanent —
> or (b) restrict every GPU emission to a candidate **superset** that can never exclude, with every
> sign-bearing decision re-made on the original f64 coordinates; and if the kernel uploads
> local-origin coordinates, the local subtraction's own error is bounded the same way. Certificate
> G3 must likewise separate conversion, per-triangle solid-angle and reduction error. Parity tests
> compare against the **original** f64 geometry, never against a reference itself rounded to f32.
> No meshgen GPU kernel exists at rev 1.3, so nothing shipped depends on the unqualified claim.

### 6.2 Broad phase — conservative inflation (S2)

The GPU grid works in f32 while the geometry is f64.

> **Certificate G2.** Converting an f64 coordinate to f32 changes it by at most
> `u₃₂/(1−u₃₂)·|x|`; measured worst case `5.9422e-8` against the bound
> `5.9605e-8` (§10 [N6]). Inflating every f32 AABB by
> `δ_bp = 2⁻²³·max|coord|` (2 ulps, both sides) therefore makes the candidate
> pair set a **superset** of the exact one.

A superset is all the contract requires: the CPU narrow phase is exact and
discards false positives. The pair-count explosion guard (plan Appendix B.4) applies to
the inflated set.

*As built (rev 1.3):* the shipped S2 broad phase is the CPU f64 hybrid grid
(`arrange.rs` `triangle_candidate_pairs`: a median-extent uniform grid with an oversized side
list, switching to full pairwise when `p95/p50 > 10` or the oversized share exceeds 5 %) with an
explicit f64 `inflation`. No f32 grid and no `δ_bp` constant exist — `2⁻²³` appears in
`src/meshgen/` only inside `topo.rs`'s unused `gwn_margin_band`. G2 binds a future GK-1 grid.


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

*As built (rev 1.3, D-25):* there is no GPU GWN kernel. `topo.rs`'s `compute_gwn_for_component`
is a **sequential** f64 sum of `atan2` solid angles at the vertex mean, accumulates no `S`, and
commits `SolidDefective` against `Sheet` on `|w| > 0.5` with **no band** — `|w|` rather than `w`
on purpose, because S0 does not fix a component's global sign; `classify.rs`'s `winding_inside`
is the same form for defective and self-intersecting solids and for exhausted ray sequences.
`gwn_margin_band` exists and is exported but is called by nothing, and it uses `u₃₂ = 2⁻²³` (f32
machine epsilon) where §1.2 defines `2⁻²⁴` — twice the table's `δ_gwn`, to be reconciled when it
is wired. Its `AI-FUNC-SUMMARY` ("pairwise reduction, accumulates S") describes the kernel this
section specifies, not the body it annotates (plan M-3.3).


### 6.4 What the GPU may never emit

Under `strict` determinism, no GPU kernel may produce a committed coordinate,
a committed classification, or an ordering that survives into the output. Its
legal outputs are: candidate sets (supersets), rankings, uncertainty flags, and
diagnostics. Under `fast`, smoothing positions and metric-driven choices may
commit within the plan Appendix B.2 tolerances; topology, labels, and tables are identical
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
  > *As built (rev 1.3, D-24):* the project sign convention is applied at three sites, all in
  > `predicates.rs` — the `robust` wrapper `orient3d`, the f64 filter form
  > `orient3d_value_permanent` and the DD form `orient3d_dd_value_permanent`, each negating
  > Shewchuk's `det[a−d, b−d, c−d]` — and `insphere` calls `robust::orient3d` **raw** solely to
  > normalise winding inside `robust`'s own convention. No other module names `robust::orient3d`;
  > the rule is restated as "the convention lives in `predicates.rs`", and the T-N1 lint that
  > enforces it covers the S0–S2 modules only (D-26).
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
  precedent (`src/io/vtu.rs`; plan Appendix B.0).
- *As built:* `two_prod` is `(p, a.mul_add(b, −p))` — a correctly rounded FMA, with a software
  fallback on targets without hardware FMA — so DD results are bit-identical across targets;
  Dekker splitting is not used. §8.3's `fma` hazard concerns GPU kernels, not this path.

### 7.4 Dependency delta

`robust` and `spade` are added; `smallvec` as already planned (`Cargo.toml`). No
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
   before export so byte comparison is meaningful ([V11] strict). *As built (rev 1.3):* the
   order is the lattice's Morton cell order carried through S8 with no re-sort (geometry
   D-37); R-P2's byte-identity is measured on that order by `sha256sum`, and `[V11]` does not
   exist (contracts §4).
6. **ASCII output** uses Rust's shortest round-trip f64 formatting, so
   write → read → write is byte-stable (the GA-1 contract).
7. Two runs, any backend mix, any device, any thread count → **bit-identical**
   primary VTU.

> **As built (rev 1.3).** Rules 1, 3, 6 and 7 hold as far as the module reaches: no kernel
> exists for rule 1, and rule 7's byte-identity is measured stage by stage (T-N11) and on the
> acceptance suite by `sha256sum` (plan §2.6). **Rule 2:** no float `par_iter().sum()` or
> adaptive reduction exists in `src/meshgen/`; every parallel stage collects per-item work into an
> indexed buffer (plan R-P1). **Rule 4:** the transcendental sites are — `acos` to *report*
> dihedrals in degrees (`tet_quality`, `[V4]`, S8 diagnostics; the gates themselves compare
> cosines); `cos` of a configured angle evaluated once as the threshold constant (`features.rs`,
> `gapfield.rs`, `sizing.rs`, `verify.rs`); `atan2` in the generalized winding number (`topo.rs`,
> `verify.rs`) — the S2b fallback classifier §2 already classes as non-primary; and one `acos`
> inside the gated kernel's `improve_dihedral` (`cdt.rs`), which compares the monotone transform
> of a cosine to choose a flip — order-preserving except at exact ties, and recorded as the one
> committed decision that reads a transcendental (gated path only; plan M-3.3). **Rule 5:** no
> canonical re-sort exists (geometry D-37).

### 8.2 Fast mode (opt-in)

GPU numeric results may commit. Guaranteed identical: topology, region and
partition labels, tag tables, conformance. Coordinates within `1e-6·diag`,
quality metrics within 1 % ([V11] topology mode). Rules 3–6 above still apply —
fast mode relaxes *precision*, never *reproducibility of structure*.

*As built (rev 1.3):* `determinism: fast` is parsed and stamped into snapshot metadata
(`DeterminismMode`) and changes no computation — there is no GPU kernel for it to release. `[V11]`
is emitted `SKIPPED` unconditionally (contracts §4).


### 8.3 Cross-device

Cross-device GPU comparison runs under the fast contract only. f32 arithmetic is
IEEE-754 on all supported adapters, but reduction order, `fma` availability, and
transcendental implementations are not portable; strict mode is unaffected
because no GPU number is committed.

---

## 9. Test obligations

| ID | Test | Asserts | Lands with |
|---|---|---|---|
| T-N1 | orientation wrapper | `orient3d` wrapper sign matches G0-1 §1.1 on a known positive tet; a raw `robust::orient3d` call in `src/meshgen/` fails review | G2-1 — met: `orient3d_sign_test`, `orientation_wrapper_matches_the_frozen_convention`; the raw-call lint is `emitting_s0_s1_s2_modules_have_no_random_state_hashmap_or_raw_orient3d` (S0–S2 modules only — D-26; the one raw call elsewhere is `insphere`'s winding normalisation inside `predicates.rs`, D-24) |
| T-N2 | static filter | on adversarial inputs, no case where the filter passes and the sign is wrong; measured error `≤ c_flt·perm` | G2-1 — **partly**: `static_filter_and_dd_construction_keep_exact_sign_contract` and `near_degenerate_filter_and_dd_signs_match_exact_predicate` assert sign agreement over 10⁵ near-degenerate cases; neither asserts `≤ c_flt·perm`, and there is no f32 case against the unrounded f64 ground truth (D-26); GK-1's f32 half not started |
| T-N3 | escalation | constructions with `ρ < κ_esc` escalate; DD result within `0.25·q` of exact; the `ρ < κ_esc·u_dd/u` case routes to typed coincidence/degradation; C3 checks both edge ratios | G2-1/G2-3 — met: `coplanar_segment_construction_obeys_rule_n5`, `rule_n5_recomputes_cancelled_determinants_before_applying_the_dd_floor`, `precision_floor_degradation_is_public_and_deterministic`, `c3_validates_both_selected_tier_edge_ratios_independently` |
| T-N4 | DD primitives | `two_sum`/`two_prod` are error-free on randomized input; DD `orient3d` sign matches `robust` on 10⁵ near-degenerate cases | G2-1 — **partly**: the DD-vs-`robust` sign check is `near_degenerate_filter_and_dd_signs_match_exact_predicate` (10⁵ cases); `two_sum`/`two_prod` themselves have **no direct test** (grep: only their definitions) |
| T-N5 | normalization | the same geometry translated by `1e6` and normalized produces bit-identical output (this is the scale-invariance fixture of plan Appendix B.8 with translation added) | G1-2 — met: `production_normalization_is_translation_invariant_before_s0`, `coplanar_segment_construction_respects_normalized_frames` |
| T-N6 | registry identity | same provenance ⇒ same vertex; adjacent triangles agree; ids are assigned in canonical-key order | G2-1 — met: `adjacent_triangles_share_one_registry_vertex`, `registry_ids_are_symbolic_group_order_not_spatial_order`, `registry_and_children_are_deterministic_under_face_order_reversal` |
| T-N7 | container determinism | no `HashMap` with default hasher reaches an emitting iteration (enforced by a lint/test over the module) | G2-1 — met for S0–S2 (`emitting_s0_s1_s2_modules_have_no_random_state_hashmap_or_raw_orient3d`); S3–S8 carry no `HashMap`/`HashSet` at all (grep 2026-09-23: only `verify.rs` and `render_scene.rs`; `render_scene.rs` sorts before emission, `verify.rs` does **not** — D-17); the lint itself covers `arrange`/`surface`/`features` only (D-26) |
| T-N8 | broad-phase superset | inflated f32 candidate set ⊇ exact f64 candidate set over randomized scenes | GK-1 — not started (no kernel) |
| T-N9 | GWN certificate | pairwise reduction; classification outside the band matches f64; band shrinks as predicted with `n` | G2-4 / GK-1 — not started on the GPU side; the CPU GWN is `topo.rs`'s `atan2` form |
| T-N10 | no transcendentals | angle gates give identical results to a `cos²` reference; a grep-level test forbids `acos`/`atan2` in decision paths | G6-2 — **partly**: the gates are `cos²` comparisons, but no grep-level test exists and §8.1's as-built note lists the transcendental sites that remain |
| T-N11 | reduction determinism | float reductions give identical results across 1, 2, 8, 16 threads | GK-3 — **partly**: per-stage default-pool-against-one-thread bit-identity tests (`the_lattice_is_bit_identical_across_runs_and_thread_counts`, `the_classification_is_bit_identical_across_runs_and_thread_counts`, `the_snap_is_deterministic`, `the_cut_is_deterministic`, `multiway_coplanar_overlay_and_source_order_are_deterministic`); the 2/8/16 matrix is not run and the tests are not reduction-specific (D-26); there is no float `par_iter().sum()` in `src/meshgen/` to test (grep 2026-09-23) |
| T-N12 | spade guards | crossing constraints refused via `try_add_constraint`, never panicking; insertion errors propagated | G2-2 — met: `crossing_spade_constraints_are_refused_without_panicking`, `spade_insertion_errors_are_propagated` |

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

**[MG-09] Certificate G1 against converted inputs (2026-09-11, re-run 2026-09-23)** — the
review's counterexample, `struct`-based f32 rounding step by step in G1's own expansion order
against `fractions.Fraction` on the f64 inputs (the `gpu_g1` block of the plan's Appendix C):
`a = (0.1, 0.1, 0.5 − 2e-8)`, `b = (0.5, 0.1, 0.5 + 2e-8)`, `c = (0.1, 0.5, 0.5 + 2e-8)`,
`p = (0.15, 0.15, 0.5 − 1.1e-8)` → f32 `det = −3.5762786e-9`, `c_flt·perm = 1.5541910e-15`,
filter **accepts**; exact f64 `det = +1.6000000436e-10`; signs **opposite**. The bound holds for
the arithmetic (as [N4] measured) and the sign is still wrong, because the inputs moved. D-13.

**[A1] As-built audit (rev 1.3, 2026-09-23, `891badc`)** — every rule, inventory row, constant
and test obligation of this document read against `src/meshgen/predicates.rs`, `arrange.rs`,
`cut.rs`, `cdt.rs`, `sizing.rs`, `gapfield.rs`, `topo.rs` and `tests/`. The independent re-reading of
findings (the plan's §13) covered geometry findings only before the pass was stopped on cost; every
finding of this document is single-pass, carrying the lines and measurements its auditor read, and
was spot-checked by grep. Rules N1, N3–N8, N10–N13
hold as written; N2 holds for validity with the two float side decisions of its as-built note;
N9/G1–G3 are unexercised (no kernel). The §2 inventory's S9–S11 rows describe stages that are not
built, and four S8 rows are re-stated as built. The named constants outside the ladder are §1.3.
No frozen constant or bound changed.

---

## 11. Deviations and open items

| # | Deviation | Reason |
|---|---|---|
| D-1 | The plan's GPU classification margin **`δ_gpu = 4·ε` is not a parity certificate** and is replaced by the per-test static filter (§6.1) | Parity errors come from a ray passing near a triangle *edge*, which can happen at any distance from the surface; a band around the surface cannot bound it. The static filter is a true certificate, and measured at `3.3e-6` uncertain it is also cheaper. The `4·ε` band is **retained** as an additional unconditional routing rule for vertices near a surface, where the geometric answer is meaningless anyway and snapping will move the node. *(rev 1.3: the band is not implemented — S6 decides by exact parity at every distance, and there is no GPU to route from; it stays a GK-1 requirement)* |
| D-2 | Escalation triggers on a per-construction **stability ratio `ρ`**, not on a post-hoc error estimate of the constructed coordinate | Measured: the determinant-ratio form with f64 determinants is as wrong as the naive form in the near-parallel family (`4.3e-4`). The decision must be made on the denominators *before* the value is formed |
| D-3 | **Coordinate normalization (Rule N1) is mandatory**, not advisory | Measured 5–6 orders of magnitude of accuracy at realistic coordinate offsets — larger than every other effect in this document combined |
| D-4 | `robust::orient3d`'s sign is **opposite** to G0-1 §1.1; a single negating wrapper is mandated | Measured. Direct use inverts every tet, and the failure is silent until a volume check |
| D-5 | `spade::add_constraint` banned (panics); `add_constraint_and_split` banned outright | A mesher must not abort on a geometric input; and split-constructed intersection points would bypass the registry, reintroducing the cross-triangle disagreement the registry exists to eliminate |
| D-6 | Double-double is hand-rolled; `twofloat` rejected | Bit-identical results, 1.5× faster, ~40 lines, zero new dependencies |
| D-7 | GWN **must** use pairwise reduction and accumulate `Σ\|ω\|` | Sequential f32 is ~300× less accurate and degrades with `n`; the certificate needs `S`, which only the kernel can produce cheaply. *(rev 1.3: binds the GPU kernel only; the shipped CPU fallback is sequential and accumulates no `S` — §6.3 as-built, D-25)* |
| D-8 | Angle gates are algebraic (`cos²`), never `acos`/`atan2` | Transcendentals are not correctly rounded and vary by platform and libm version; the plan's 8°/10°/45° thresholds would otherwise be non-deterministic decision sites. *(rev 1.3: the sites that still read a transcendental are listed in §8.1's as-built note; D-25)* |
| D-9 | Float reductions may not use `rayon`'s adaptive splitting | Its split points depend on thread count and work stealing, which breaks bit-identical strict mode |
| D-10 | Registry containers must be `BTreeMap` or fixed-seed + sorted | Rust's default `HashMap` iteration order varies between runs of the same binary |
| D-11 | The implemented/default `eps_frac` is `1e-4`, not the original sketch's `1e-3` | `1e-3` violates the frozen parse-time cap under the default sizing/gap factors; all q/kappa/DD-floor examples now distinguish the actual default from the optional configured value |
| D-12 | C3 applies Rule N5 to both defining-edge parameter ratios | Either ratio is later consumed for exact edge order; checking only the coordinate-producing edge can commit an unstable ordering ratio |
| D-13 | **Certificate G1 is qualified**: it certifies the sign on the f32 operands as given, not on the f64 geometry they were converted from (§6.1) | The review's counterexample (plan MG-09): a filter-accepted f32 sign opposite to the exact f64 sign. The rev-1 text's "sign(det) is exact" was true of the arithmetic and silently assumed exact inputs. Repaired at rev 1.4 before GK-1 |
| D-14 | **Rule N14 and the four roles (§1.5)** are added; two plan subtasks that argued from an error-metric tolerance to a geometric move (M-4.1's "quantise to `q`", M-5.3's "invisible to `[V13]`") are refuted on paper and re-scoped (plan M-1.6, M-5.3) | The review (plan MG-11): quantised keys are not quantised coordinates in this module, the q grid is not the lattice grid, and a fraction of a local edge is not a normalized length |
| D-15 | **The mesher carries named constants outside §1.2's ladder** (§1.3), several of them decision-bearing (class **A**/**T**), and two absolute ones (the fragment kernel's key quantum and clip `tol`) | Found by the audit; listed rather than removed, because each has a measured reason at its site; plan M-3.3 is where each is named in code or retired. §0's rule now has a complete list to be checked against |
| D-16 | **Two S8 sites decide a side in floating point** (Rule N2 as-built note) and the §7.3 cache winds faces by float sign | Covered by exact downstream checks; recorded so N2's claim is not made of code that does not meet it |
| D-17 | **The verifier's report is not deterministic on a large mesh** (Rule N7 / §8.1 rule 2 breached in `verify.rs`): `[V6]`'s region-adjacency items are pushed in `HashMap` iteration order and truncated at the cap, and `undeclared_boundary_area` is a float sum in that order — measured 2026-09-23 on A-3's `s08_cut_contract.vtu`: two runs with identical configs list different `V6.region_adjacency` items (218 violations against a cap of 50) and report `0.1971761865274474` against `0.19717618652744737` | `report_is_deterministic_across_runs` passes on the fixtures because no section exceeds the cap there. Sort the keys (as `check_v3` does) and sum in index order — plan M-1.0 (e); contracts D-17 |
| D-18 | **Rule N3 is relaxed in four places:** `tet_signed_volume` (= `robust`'s adaptive magnitude / 6) is used as a number by `[V4]`'s quality, the band dry-run volume and the gated kernel's thin-tet refusal; `classify_point_in_triangle` compares `\|orient2d\|` to a `q·\|edge\|` band; and two S2 orderings take their sign from a DD or f64 *constructed* value (the `DeterminantRatio` cross-difference along a source edge; the dominant coordinate along a pair segment), bounded by the `QuantizedOrderAmbiguity` route | `robust`'s non-zero result is within its own error bound of the true value, and each use is a tolerance test rather than a sign decision; the orderings are covered by the `q`-band degradation. Recorded, not accepted: plan M-3.3 |
| D-19 | **Rules N4 and N6 govern the S2 registry only.** Downstream constructions — S7's edge crossing (the C1 ratio, unescalated, clamped to `[0, 1]`, `t = 0.5` on a zero denominator), G2-5's box-clip points, S8's curve pierce point and the §7.4 kernel's clip, half-space clip and trace points — use f64 plane-normal forms, never escalate, and are identified by `NodeKey` (or lattice edge + `NodeKey`) with no provenance key. N4's second sentence also cannot hold for C2 as frozen: Cramer assembles `x` per coordinate | Each is a pure function of data shared by every cell that consults it, so conformity holds; validity is re-established by the exact `orient3d` and `[V1]`/`[V3]`. §4.1 gains C6 for them |
| D-20 | **`κ_esc` has two values:** `c = 7` for C1 and C3, `c = 25` for C2 (`1.1e-9` at the default `q`); the §1.2 row and the §4.3 table carried only `c = 7`; and C1/C3 compute their DD determinants on **every** construction (the committed ordering ratio is always DD), so the "vanishing fraction" cost claim applies to C2 only | The C2 constant was in §4.3's prose ("the `c = 25` used for safety") and not in the tables; corrected. The DD-always cost was measured acceptable (S2 under 1 s on every acceptance case) |
| D-21 | **S1's corner test has no sign guard**: `cos² < cos²(60°)` marks a 90° turn a corner and misses a 150° hairpin (`cos² = 0.75`), which is sharper | A defect found by the audit, not a design choice; the sharp-edge test has the guard and the corner test does not. Plan M-4.6 (the S1 sources) fixes it; recorded here because §2 froze the row |
| D-22 | **S7's motion cap does not clamp** — a target beyond `SNAP_MOTION_CAP · l_min` is ignored (features) or dropped (surface re-check), and `under_snapped` is filled and read by nothing | ARB-11's "clamp; mark under-snapped for `[V5]`" describes a design that was not built; a clamped snap would put the node off the target, which is the P3 violation the cap exists to avoid — dropping is the right behaviour, and the row is corrected to say so |
| D-23 | **S6's plane-test perturbation is certified for the dot product only:** `plane_side` resolves an exact `0` by `sign(n·dir)` under a `4·EPSILON·Σ\|nᵢdᵢ\|` filter, but `n` is itself a rounded f64 cross product whose cancellation the bound does not contain (§2 S6 row) | A code-reading finding, not demonstrated numerically. Plan M-3.3 either evaluates `n·d` as the 3×3 determinant `det[b−a, c−a, d]` through the orient3d filter/`robust`, or records the gap as accepted |
| D-24 | **Rule N10's "sole place" is three places** in `predicates.rs` (wrapper, f64 filter form, DD form) plus a raw `robust::orient3d` inside `insphere` for winding normalisation (§7.1 as-built note) | The convention is still confined to one module; the wording is corrected and T-N1's lint must be scoped to "outside `predicates.rs`" |
| D-25 | **The GWN as built** (§6.3 as-built note): CPU, sequential `atan2` sum, `\|w\| > 0.5`, no `S`, no band, committing S2b closure classification and S6 fallback labels; `gwn_margin_band` is dead code at `2⁻²³` | Accepted as the fallback classifier §2 already classes as non-primary; cross-libm bit-identity is not claimed for `\|w\| ≈ 0.5`. Rule N9 and Certificate G3 bind the GK-1 kernel (plan M-7) |
| D-26 | **Test obligations as built:** T-N1's and T-N7's lints cover `arrange`/`surface`/`features` only; T-N2 asserts sign agreement but not `≤ c_flt·perm` and has no f32 case (which is how the plan's MG-09 went uncaught); T-N4's `two_sum`/`two_prod` error-free test does not exist; T-N10's grep guard does not exist; T-N11 compares the default pool against one thread, not the 2/8/16 matrix (§9) | Each row now says what is met; plan M-1.0 extends the lints to every `src/meshgen/*.rs` except `predicates.rs` and `render_scene.rs`, and adds the missing assertions |

Open items handed on (re-stated at rev 1.3):

- **Predicate performance at scale** — the `1.3–1.5×` cost of `robust` was
  measured on 2 M synthetic calls; the S2/S6 in-situ ratios are a **GK-1**
  acceptance measurement.
- **Junction-cell 3D constrained tetrahedralisation** — no crate was adopted (spade is 2D) and a
  hand-rolled kernel now exists (`src/meshgen/cdt.rs`: constrained incremental insertion, boundary
  recovery by flips and edge removal, exact predicates through the §7.1 wrapper, DD constructions
  where §4.3 escalates); G0-1 rev 1.6 §0 records the gate's answer. Its arithmetic is inside this
  document's inventory (§2 rows S8); the Steiner-on-facet rule plan M-2.1 will need is the rev 1.4
  item G0-1 D-24 names.
- **The identity rule for two constructed points one float32 ULP apart** (plan M-1.6) is chosen by
  measurement under §1.5's constraints and frozen at rev 1.4. "Quantise the input to `q`" is not a
  candidate (D-14).
- **Certificate G1 for converted inputs** (D-13) is repaired at rev 1.4, before GK-1.
- **The GWN certificate's wiring** (`gwn_margin_band`, `u₃₂`, `S`), the T-N lint scope and the
  S6 plane-filter bound (D-23..D-26) → plan M-3.3 for the CPU items, M-7 for the kernel.
- **In-situ DD/fallback rates beyond the G2-3 corpus** remain reporting metrics,
  not correctness gates. The boundary and named fallback triggers are frozen by
  the [G2-3] record above.
