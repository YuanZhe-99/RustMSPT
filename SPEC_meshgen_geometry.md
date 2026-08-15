# SPEC — Mesh generation: geometry and topology freeze (subtask G0-1)

**Status:** frozen (rev 1.4; §1.2 gained Rule K-O on 2026-07-31 — an ordering key must separate the nodes it orders, which the weld grid does not for constructed crossings — see §14 [10] and §15 D-16; G2-3 degeneracy boundary and C10 ownership clarified 2026-07-28; §3.7's informative quality prediction corrected 2026-07-30 — see the note there and §14 [8]; §8.2's informative measured band-cell row corrected 2026-07-31 — see the note there and §14 [9]). Normative for all `src/meshgen/` work.
**Date:** 2026-07-31
**Subtask:** G0-1 (Phase G0, tier T3) of [`PLAN_mesh_generation.md`](PLAN_mesh_generation.md).
**Scope (from the plan's G0-1 acceptance):** Freudenthal node orderings +
signed-volume table; centroid-fan face rules; kirigami split tables; junction
local-PLC spec incl. the shared-face-cache invariant; band k-cases; `resolve()`
truth table; coincidence policy table; the S3↔S4 monotonicity argument; the
arbitration failure catalog. *Acceptance: every template/case has node lists +
a positivity argument; one catalog entry per producer.*
**Companion freezes:** G0-2 (predicates, error bounds, GPU margin certificates —
every "exact predicate" and "error bound" referenced here is specified there),
G0-3 (VTU schema, check catalog, accuracy table).

Every numeric table in this document was machine-checked before being written;
the verification record, with the programs' actual output, is §14. Where this
document deviates from the reference implementation or sharpens the plan's prose, the
deviation is listed in §15 — nothing is changed silently.

---

## 0. Normativity and change control

- **MUST/MUST NOT** statements are binding on the implementation. A conflict
  between this document and `PLAN_mesh_generation.md` prose is resolved in favour
  of this document; a conflict with the plan's *decisions* (§2 review record) is a
  defect in this document and must be raised, not worked around.
- Every table here is a *frozen table*: implementations MUST reproduce it exactly
  (there is a named unit test per table, §13), and MUST NOT re-derive it at
  runtime with a heuristic.
- Two items remain deliberately **not** frozen here because a prototype gate owns
  them: transition-fan element quality in production geometry (gate G4-3) and
  junction-cell CDT viability (gate G6-0). G2-3 completed on 2026-07-28 and its
  arrangement boundary/fallback decision is now frozen in §10 and §14.

---

## 1. Global conventions

### 1.1 Orientation

A tet `(n0, n1, n2, n3)` is **positively oriented** iff

```
orient3d(n0,n1,n2,n3) = det[ n1-n0, n2-n0, n3-n0 ] > 0
```

which is the VTK_TETRA / Abaqus C3D4 convention (`6·V = orient3d`). Every
template in this document lists nodes in an order that is positive for the
geometry it is defined on; where a template is instantiated on data whose
handedness is not known a priori, the implementation MUST apply the **canonical
orientation fix**: emit `(n0, n1, n2, n3)`, and if `orient3d ≤ 0`, swap `n1` and
`n2`. `orient3d == 0` after the fix is a degeneracy, never an emitted element
(§4.4 ladder).

Outward-oriented faces of a positively oriented tet, `F_i` opposite `n_i`:

| Face | Nodes (outward) |
|---|---|
| `F0` | `(n1, n2, n3)` |
| `F1` | `(n0, n3, n2)` |
| `F2` | `(n0, n1, n3)` |
| `F3` | `(n0, n2, n1)` |

### 1.2 Node keys and the total order

`NodeKey(p) = (round(p.x/q), round(p.y/q), round(p.z/q))` on the weld grid
`q = 0.1·ε` (§10.1 of the plan), ordered **lexicographically**. Two distinct mesh
nodes MUST have distinct keys (the weld invariant; violation is verifier [V2]).

> **Rule K-O (rev 1.4, added by G7-1).** The weld invariant is a statement about
> *welded* nodes, and S7's crossings are **constructed** points that are not welded
> against each other: two of them can land closer together than `q` and therefore
> share a key. A stage that orders nodes MUST therefore build its ordering key on a
> grid fine enough to separate the nodes it actually holds - S8 uses `1e-6 · q` - and
> MUST NOT assume the weld grid separates them. Ordering on a grid that does not is a
> silent conformity failure, not a rounding nuisance: "smallest node key" stops being
> a total order, the tie falls to whichever node the caller happened to list first,
> and two cells sharing a quad list it in opposite orders and split it on opposite
> diagonals. Welding the tied pair instead is **not** the remedy - it also merges
> crossings on different edges and collapses the cells between them (measured: 290
> multi-shared faces and 580 non-manifold edges where there had been 0 and 1).
> Verified in §14 [10].

Every rule in this document that says "smallest node" means smallest `NodeKey`.
Node *indices* MUST NOT be used for any such rule: indices depend on emission
order, keys depend only on position, and the conformity arguments below all rest
on two different cells computing the same answer from the same geometry.

Lattice nodes additionally lie exactly on the integer grid of step `h_min/2`
(§3.2), so lattice keys are exact and no lattice decision ever consults a
tolerance.

### 1.3 Canonical frame — the conformity mechanism

> **Invariant C (canonical frame).** Any table indexed by an *unordered* entity —
> a face, a quad, a prism, a junction face — MUST be evaluated after sorting that
> entity's nodes by `NodeKey`, and its output MUST be independent of the calling
> cell's winding.

Consequence, used by §3 (lattice faces), §4 (quads), §5 (kirigami faces), §7
(junction faces) and §8 (band quads): **two cells sharing an entity produce
identical sub-entities without communicating.** This replaces negotiation,
ordering conventions, and cache coherency with a pure function. The cache in §7
is then an optimisation, not a correctness mechanism — and that is exactly why
the mismatch check in §7.3 can be an assertion.

### 1.4 Emission order

Cells are processed in ascending Morton key order (lattice) or ascending
`(Y, X)` then smallest-node-key order (cut stages). Within a cell, template rows
are emitted in table order. Together with Invariant C this gives the
strict-determinism contract (§12 of the plan) a deterministic element ordering
before any canonical re-sort at export.

---

## 2. Background lattice — Freudenthal–Kuhn 6-tet decomposition

### 2.1 Cube corner numbering

For a cell with integer origin index `(i, j, k)` and step `s`, corner `v_m`,
`m ∈ [0,8)`, is

```
v_m = ( i + (m & 1), j + ((m >> 1) & 1), k + ((m >> 2) & 1) ) · s        (index space)
```

i.e. `m`'s bits are `(bx, by, bz)`. `v0` is the componentwise-minimum corner,
`v7` the maximum; `v0–v7` is the main diagonal shared by all six tets.

### 2.2 The frozen table

Each tet is the monotone lattice walk `v0 → +e_{σ1} → +e_{σ1}+e_{σ2} → v7` for one
permutation σ of the axes; equivalently the Kuhn simplex
`{0 ≤ x_{σ1} ≤ x_{σ2} ≤ x_{σ3} ≤ h}`.

| # | Walk σ | Node list (positively oriented) | `orient3d` (unit cube) | Volume |
|---|---|---|---|---|
| K0 | x y z | `(v0, v1, v3, v7)` | `+1` | `h³/6` |
| K1 | x z y | `(v0, v1, v7, v5)` | `+1` | `h³/6` |
| K2 | y x z | `(v0, v2, v7, v3)` | `+1` | `h³/6` |
| K3 | y z x | `(v0, v2, v6, v7)` | `+1` | `h³/6` |
| K4 | z x y | `(v0, v4, v5, v7)` | `+1` | `h³/6` |
| K5 | z y x | `(v0, v4, v7, v6)` | `+1` | `h³/6` |

Note K1, K2 and K5 have their last two walk nodes swapped relative to the raw
walk order — that swap is what makes them positive, and it is part of the frozen
table, not an implementation detail.

**Positivity and exact tiling.** Each row's `orient3d` is `+1` on the unit cube
and `+s³` in general, hence `V = h³/6 > 0` for every cell size. The six volumes
sum to `h³` exactly (integer arithmetic, no tolerance), and the six simplices
have pairwise disjoint interiors because a point with coordinate ordering
`x_{σ1} < x_{σ2} < x_{σ3}` lies strictly inside exactly the tet for σ — ties are
the shared boundary faces and are measure-zero. Machine-checked: §14 [1].

### 2.3 Face diagonal rule (translation invariance)

> **Rule D (lattice diagonal).** On any axis-aligned lattice face, the diagonal
> joins the **componentwise-minimum corner to the componentwise-maximum corner**.

The six cube faces induced by the table obey Rule D exactly:

| Face | Sub-triangles | Diagonal |
|---|---|---|
| `x = lo` | `(v0,v2,v6)`, `(v0,v4,v6)` | `v0–v6` |
| `x = hi` | `(v1,v3,v7)`, `(v1,v5,v7)` | `v1–v7` |
| `y = lo` | `(v0,v1,v5)`, `(v0,v4,v5)` | `v0–v5` |
| `y = hi` | `(v2,v3,v7)`, `(v2,v6,v7)` | `v2–v7` |
| `z = lo` | `(v0,v1,v3)`, `(v0,v2,v3)` | `v0–v3` |
| `z = hi` | `(v4,v5,v7)`, `(v4,v6,v7)` | `v4–v7` |

Rule D is stated in **global** index space, so it is translation-invariant: two
cells sharing a face compute the same diagonal from the same four global corner
coordinates. This is the whole reason the primary lattice is Freudenthal and not
the reference 5-tet scheme — the 5-tet decomposition needs an XOR-parity checkerboard
to make neighbouring cubes agree, and that bookkeeping does not survive contact
with octree level transitions (plan §2 topic A). Rule D is a special case of
Invariant C: the min corner is the smallest `NodeKey` of the face.

### 2.4 Applicability

The Freudenthal template applies to a leaf **iff none of its 6 faces and none of
its 12 edges is split** (§3.1). Every other leaf uses §3.

---

## 3. Transition cells — centroid fan

### 3.1 Split state (the only input the templates need)

For a leaf `C` at level `L`:

- an **edge** `E` of `C` is *split* iff `midpoint(E)` is a lattice corner node;
- a **face** `F` of `C` is *split* iff `centre(F)` is a lattice corner node.

("Lattice corner node" = a corner of some leaf.) These two predicates are exact
integer lookups; no neighbour walk and no tolerance is involved, and both cells
sharing an entity evaluate the same predicate on the same point.

Required octree property: **strong 2:1 balance** — any two leaves whose closed
boxes intersect (face, edge, *or* vertex contact) differ by at most one level.
Face-only balance is insufficient: a leaf one level finer that touches `C` on an
*edge* alone still puts a node in the interior of `C`'s edge. That is the case
that silently produces hanging nodes, and it is why the split predicate is stated
per edge and not per face.

Under strong balance:

- **L1.** `split(F) ⟹` all four edges of `F` are split, and the four sub-faces of
  `F` seen from the finer side have no split edges of their own (an `L+2` leaf
  touching them would be edge- or face-adjacent to `C` two levels apart).
- **L2.** If `C`'s neighbour across `F` is *coarser*, then no edge of `F` is
  split. (Any leaf touching an edge of `F` touches the coarser neighbour on an
  edge or a face; balance caps it at level `L`.)

**Cell classification.** `C` is a *Freudenthal cell* iff no face and no edge is
split; otherwise it is a *fan cell*. Note this is stronger than "has a finer face
neighbour" — a single split edge is enough. That strengthening is load-bearing:
it is what guarantees that the cell on the other side of a case-E face (§3.3) is
never a Freudenthal cell computing a plain diagonal.

### 3.2 New nodes and their exactness

A fan cell introduces at most two kinds of node: the **centroid** `c` of the
cell, and a **face centre** `m_F` for each case-E face. In index space both are
half-steps of a level-`L` cell, hence integer multiples of `h_min/2` — so the
whole lattice, including these, lives on the `h_min/2` integer grid and all keys
are exact (§1.2). `c` is interior to exactly one cell; `m_F` is computed
identically by both cells sharing `F` and deduplicated by key.

### 3.3 Face rule `f(F)` — frozen

`f` is a pure function of the face's four global corner coordinates and of the
split state of its centre and four edges. Let the corners in cyclic order be
`p0, p1, p2, p3` and let `k` = number of split edges.

| Case | Condition | Output | Count |
|---|---|---|---|
| **P** (plain) | centre not split, `k = 0` | Rule D: `(lo, o1, hi)`, `(lo, hi, o2)` where `lo`/`hi` are the min/max corners and `o1`, `o2` the other two in cyclic order | 2 |
| **E** (edge fan) | centre not split, `k ≥ 1` | insert `m_F`; walk the boundary polygon (4 corners with the `k` split midpoints inserted in cyclic order) and emit `(m_F, q_t, q_{t+1})` per boundary segment | `4 + k` |
| **Q** (quadrant) | centre split | for each of the 4 quadrant sub-faces, apply case **P** at level `L+1` | 8 |

Case Q's quadrants are exactly the faces of the four finer neighbours, and by L1
those neighbours evaluate case P on them — the same two triangles by Rule D.

### 3.4 Cell rule

A fan cell emits, for every face `F` and every triangle `t ∈ f(F)`, the tet
`(t0, t1, t2, c)` under the canonical orientation fix (§1.1).

**Positivity.** Every emitted triangle lies in one of the six face planes of the
cell, and `c` is at distance `h/2` from each of those planes, so

```
V = (1/3) · area(t) · (h/2) = area(t) · h / 6 > 0
```

with a uniform lower bound from the smallest template triangle
(`area = h²/8`, cases E and Q):

> **Bound P1.** Every lattice tet satisfies `h³/48 ≤ V ≤ h³/6`.

Volume closes exactly: each face contributes total area `h²`, so
`Σ_F Σ_t area(t)·h/6 = 6·h²·h/6 = h³`.

### 3.5 Template inventory

| Cell state | Faces (P/E/Q) | Tets | min V | max V |
|---|---|---|---|---|
| no split (Freudenthal) | 6 P | 6 | `h³/6` | `h³/6` |
| one split edge | 4 P, 2 E(k=1) | 18 | `h³/48` | `h³/12` |
| one split face | 1 Q, 4 E(k=1), 1 P | 30 | `h³/48` | `h³/12` |
| all six faces split | 6 Q | 48 | `h³/48` | `h³/48` |

General count: `Σ_F t(F)` with `t(P) = 2`, `t(E) = 4 + k_F`, `t(Q) = 8`; the range
is 18…48 for fan cells.

### 3.6 Conformity theorem

> **Theorem T1.** A strongly 2:1-balanced octree, tetrahedralised by §2.2 for
> Freudenthal cells and §3.4 for fan cells, is conforming: every interior
> triangular face is shared by exactly two tets, and no node lies in the interior
> of another tet's face or edge.

*Proof.* All emitted triangles lie in cell face planes, so the only entities two
cells can disagree about are their shared faces. For a shared face `F`:
(i) if both cells are at the same level, both evaluate `f(F)` on the same global
data and by Invariant C obtain the same triangles;
(ii) if one is finer, `split(F)` holds for the coarser cell, which emits case Q =
the union of the four quadrant triangulations, and by L1 each finer cell emits
case P on its quadrant — the same two triangles;
(iii) the case-E/coarser-neighbour combination cannot occur (L2).
A hanging node would be a node interior to some face triangle; since every split
edge midpoint and every split face centre is a vertex of the triangulation on
both sides (cases E and Q both include them) and no other new node touches a cell
boundary (§3.2), none exists. ∎

Machine-checked on seven octree configurations including the edge-only and
vertex-only refinement patterns: §14 [2] — zero non-paired interior faces, zero
hanging nodes, exact volume in all cases.

### 3.7 Predicted quality (input to gate G4-3)

Measured over **every** tet each template class emits (exact interior dihedral,
`AR = R/(3·r_in)`):

| Template class | min dihedral | max dihedral | AR | V |
|---|---|---|---|---|
| Freudenthal tet | 45.000° | 90.000° | 1.3938 | `h³/6` |
| fan over a plain (P) face | 45.000° | 120.000° | 1.5607 | `h³/12` |
| fan over a quadrant (Q) triangle | **35.264°** | **125.264°** | **1.6052** | `h³/48` |
| fan, centre–corner–midpoint (E) | 45.000° | 90.000° | 1.3938 | `h³/48` |
| fan, centre–corner–corner (E) | 45.000° | 90.000° | 1.4268 | `h³/24` |

> **Correction (rev 1.2, 2026-07-30, G4-2 implementation).** The Q row previously
> read `45.000° / 90.000° / 1.3938` — the centre–corner–midpoint row's numbers,
> evidently carried across rather than measured. The true worst case over all
> eight quadrant triangles is `arctan(1/√2) = 35.2644°`, attained on the two
> quadrants whose Rule-D diagonal runs away from the cell centroid; the other two
> give 45°. **No rule changed** — Rule D and §3.4 are as frozen, and the value is
> forced by them — only this prediction did. Verified twice and independently:
> by exhaustive enumeration of all 8 quadrant triangles × 6 faces, and by the
> shipped verifier's [V4] on a 287,830-tet lattice of real geometry, which
> reports `min_dihedral_deg = 35.264389682751705` and
> `worst_aspect_ratio = 1.6051717155225624` — agreeing to every printed digit.

So the transition fans **do** degrade the minimum dihedral, from 45° to 35.264°
(a 22% reduction), and cost 15% in aspect ratio. Both are still comfortably clear
of any usable FEM gate — 35.26° is a healthy dihedral, and the [V4] default
`low_dihedral_deg` gate is 5° — so this is a correction to a claim, not a change
in the go/no-go outlook. G4-3 still tests the *post-snap* quality of transition
neighbourhoods rather than the templates themselves, but it now measures against
35.264° rather than 45°: a snap that erodes the dihedral by a further 20° starts
from a worse baseline than the frozen text assumed. A G4-3 failure would indicate
that snapping into the smaller `h³/48` tets is the problem, and the recorded
fallback (reference-verbatim 5-tet + SAMR, plan §10.7) remains available.

---

## 4. Quads and prisms — the one diagonal rule

Quads and 3-sided prisms appear in the cut cases (§6), the band templates (§8)
and case-E faces. They are governed by one rule so that the validity argument is
global.

### 4.1 Rule SNK (smallest node key)

> **Rule SNK.** A quad's diagonal is the one **incident to the quad's
> smallest-`NodeKey` vertex**. A quad is always split into
> `(lo, next(lo), opp(lo))` and `(lo, opp(lo), prev(lo))`.

Rule SNK is an instance of Invariant C: it depends only on the quad's four node
keys, so both cells sharing a quad choose the same diagonal, and the same quad
reached through two different templates (e.g. a parent-tet face reached from the
face-split table and from a prism's quad) gets the same diagonal.

### 4.2 Prism decomposability

A prism with caps `(a0,a1,a2)`, `(b0,b1,b2)` (`a_i ↔ b_i`) and quads
`Q_ij = (a_i, a_j, b_j, b_i)` has a 3-tet decomposition **iff** its three chosen
diagonals are not *cyclic*, the two cyclic sets being
`{a0–b1, a1–b2, a2–b0}` and `{a1–b0, a2–b1, a0–b2}`.

> **Theorem T2.** Rule SNK never produces a cyclic set.

*Proof.* Let `v` be the prism's globally smallest node key. `v` lies on exactly
two of the three quads, and being globally smallest it is also the smallest key
within each of those quads, so SNK selects a diagonal incident to `v` on both —
`v` is an endpoint of two diagonals. In either cyclic set every vertex is an
endpoint of exactly one diagonal. ∎ (Machine-checked over 20 000 random key
orders and 4 000 random prisms including deliberately twisted ones: §14 [7],
[8].)

This is PLAN 2's assumption B-4, now with the full enumeration behind it.

### 4.3 The six non-cyclic patterns (frozen)

| Diagonals | Tets |
|---|---|
| `a1–b0, a2–b1, a2–b0` | `(a0,a1,a2,b0)`, `(a1,a2,b0,b1)`, `(a2,b0,b1,b2)` |
| `a1–b0, a1–b2, a0–b2` | `(a0,a1,a2,b2)`, `(a0,a1,b0,b2)`, `(a1,b0,b1,b2)` |
| `a1–b0, a1–b2, a2–b0` | `(a0,a1,a2,b0)`, `(a1,a2,b0,b2)`, `(a1,b0,b1,b2)` |
| `a0–b1, a2–b1, a0–b2` | `(a0,a1,a2,b1)`, `(a0,a2,b1,b2)`, `(a0,b0,b1,b2)` |
| `a0–b1, a2–b1, a2–b0` | `(a0,a1,a2,b1)`, `(a0,a2,b0,b1)`, `(a2,b0,b1,b2)` |
| `a0–b1, a1–b2, a0–b2` | `(a0,a1,a2,b2)`, `(a0,a1,b1,b2)`, `(a0,b0,b1,b2)` |
| `a1–b0, a2–b1, a0–b2` | **cyclic — unreachable under SNK** |
| `a0–b1, a1–b2, a2–b0` | **cyclic — unreachable under SNK** |

Emit under the canonical orientation fix. A prism's boundary is *not* independent
of the diagonal choice (its quads are generally non-planar), which is why the
decomposition is defined by its boundary face set and not by a volume test.

### 4.4 Validity ladder (geometric, as opposed to combinatorial)

Combinatorial validity (T2) does not imply positive volumes: near-degenerate or
twisted pairs are a geometric failure. Every emitted prism/quad tet MUST be
checked at runtime for `orient3d > 0` and `min dihedral ≥ 8°`. On failure:

1. **pairwise flip** — try the alternate diagonal on the offending quad,
   re-splitting the single neighbour that shares it (legal only pairwise, and only
   if the neighbour's own check then passes);
2. **Steiner fallback** — insert the piece's centroid and fan its boundary
   (8 tets for a prism); still one geometric layer for band cells, counted in the
   thin-feature report;
3. **escalate the cell** to the junction local-PLC mesher (§7);
4. **refine** the cell one level (within `h_min`) and re-cut.

Regional failure (> 5 % of a band region's cells) demotes the whole region to
volumetric with `[THIN-SKIP]` (plan §10.11 ladder outcome 4).

---

## 5. Kirigami face-split tables (S8, single-patch cells)

### 5.1 Inputs and invariants

For a parent tet face, the cut state is: the set of **cut edges** (each carrying
exactly one cut node from the S7 exact edge–surface intersection), the set of
**on-cut vertices** (parent nodes snapped exactly onto the patch), and, for open
sheets only, an interior **rim endpoint**.

> **Invariant K1 (one crossing per edge).** An active patch crosses a lattice
> edge at most once. An edge with two or more crossings escalates its incident
> cells: refine one level, and if the sizing floor is reached, hand the cells to
> §7. This is checked, not assumed.

> **Invariant K2 (no degenerate cut nodes).** A cut node within the weld tolerance
> of a parent node is **promoted** to that parent node, which becomes an on-cut
> vertex; a cut node is never emitted coincident with a parent node.

K2 replaces the reference implementation's behaviour of marking such an element degenerate and *removing*
it (`kirigami_divide` phase 2), which leaves a hole in the mesh. Promotion keeps
the mesh closed and moves the configuration to a legal row of the table below.

### 5.2 The frozen table

Evaluate in the canonical frame: sort the face's three parent nodes by `NodeKey`
into `(a, b, c)`, apply the row, then re-orient each sub-triangle to the calling
cell's winding.

| Case | Cut edges | On-cut vertices | Rim | Sub-triangles | Count |
|---|---|---|---|---|---|
| `uncut` | 0 | 0–3 | – | `(a, b, c)` | 1 |
| `split_2` | 1: `(u,v)` → `m` | 1: `w` (the third vertex) | – | `(u, m, w)`, `(m, v, w)` | 2 |
| `split_3` | 2 | 0 | – | `(n3, x1, x2)` + SNK split of quad `(n1, n2, x2, x1)` | 3 |
| `split_4` | 3 | 0 | – | `(a, m_ab, m_ca)`, `(b, m_bc, m_ab)`, `(c, m_ca, m_bc)`, `(m_ab, m_bc, m_ca)` | 4 |
| `split_R` | 1: `(u,v)` → `m` | 0 | `r` | `(u, m, r)`, `(m, v, r)`, `(v, w, r)`, `(w, u, r)` | 4 |

`split_3` notation: `(n1, n2)` is the uncut edge, `n3` the opposite vertex, `x1`
the cut node on `(n3,n1)`, `x2` the cut node on `(n2,n3)`. The apex triangle
`(n3, x1, x2)` is unconditional; the quad `(n1, n2, x2, x1)` is split by Rule SNK
— **not** by the reference implementation's distance comparison with a `1e7/1e4/1e0` coordinate
tie-break, because SNK is what makes §4.2 hold globally (see §15, D-3).

`split_4` is the medial split. It arises only when a patch meets one face in more
than one segment, which is non-generic; the implementation MUST log `[CUT-3EDGE]`
and prefer escalation (§7) when the medial centre triangle fails the §4.4 quality
floor.

`split_R` is the open-sheet rim-termination case (R-C1): the cut front ends at an
interior point `r` of the face. `r` is a shared node of both incident cells, so
both produce the same four triangles. A `(1 cut edge, 0 on-cut vertices, no rim)`
state is **illegal** — a cut segment with a dangling end — and escalates.

### 5.3 Verified properties

Over 2 000 randomised faces per case, evaluated from both incident cells with
opposite winding and different rotations: identical triangle sets (500/500 on the
dedicated adjacency test) and exact area tiling of the parent face. §14 [4], [5].

---

## 6. Single-patch cut of a tet — the complete case table (S8)

Classify the parent tet's four nodes as `I` (strictly inside the patch's
positive side), `O` (strictly outside) or `C` (on-cut). A cut exists iff
`|I| ≥ 1` and `|O| ≥ 1`, which leaves exactly six configurations. `m(i,j)` is the
cut node on edge `(i,j)`, `i ∈ I`, `j ∈ O`.

| Case | `(#I, #O, #C)` | Node lists | Tets |
|---|---|---|---|
| **A** | `(1,1,2)` | `(I, C1, C2, m)`; `(O, C1, C2, m)` where `m = m(I,O)` | 2 |
| **B** | `(1,2,1)` | inside `(I, m1, m2, C)`; outside = SNK split of quad `(m1, O1, O2, m2)`, each triangle coned to `C` | 3 |
| **B′** | `(2,1,1)` | B with `I`/`O` exchanged | 3 |
| **C** | `(1,3,0)` | inside `(I, m1, m2, m3)`; outside = prism `(m1,m2,m3)/(O1,O2,O3)` via §4.3 | 4 |
| **C′** | `(3,1,0)` | C with `I`/`O` exchanged | 4 |
| **D** | `(2,2,0)` | inside prism `(I1,m11,m12)/(I2,m21,m22)`; outside prism `(O1,m11,m21)/(O2,m12,m22)`, both via §4.3 | 6 |

`m_ij = m(I_i, O_j)`. Configurations with `|I| = 0` or `|O| = 0` are uncut for
this patch; a face whose three nodes are all on-cut is emitted as a tagged face
(no split).

**Positivity.** Cases A and C are exact: the cut surface is a single triangle, so
`V(I,C1,C2,m) = λ·V(parent)` with `λ = |Im|/|IO| ∈ (0,1)` (case A), and
`V(I,m1,m2,m3) = λ1λ2λ3·V(parent)` (case C). Cases B and D reduce to §4 (SNK
non-cyclicity + the §4.4 runtime ladder). In every case the pieces partition the
parent volume **exactly**, because the cut surface is triangulated once and both
sides consume the same triangles.

**Conformity with §5.** Every boundary triangle of the emitted pieces that lies
on a parent face equals exactly what `f`'s counterpart in §5.2 produces for that
face — verified, zero violations over 4 800 randomised cuts (§14 [5]). This is
what makes the cut conforming across cell boundaries without any inter-cell
communication.

**Guarded dry-run (ported).** Before committing, the cut MUST be validated: all
children positively oriented, `|Σ V_child − V_parent| ≤ 1 % · V_parent`, and no
node other than parent nodes, registry nodes, or the cut nodes of this cut.
Failure → `[CUT-GUARD]` → §4.4 ladder step 3.

**Raw quality (pre-S9), measured.** Worst per-cut minimum dihedral, cutting a
Freudenthal-shaped parent at random positions: median 14.0° / p5 6.1° (A),
12.3° / 4.2° (B), 8.4° / 2.6° (C), 7.3° / 1.8° (D). Cutting produces slivers by
construction — these numbers are the input budget for S9 (IQD + Phase-5.5), not
an acceptance gate, and they justify keeping S9 mandatory rather than optional.

---

## 7. Junction cells — local PLC mesher (S8)

### 7.1 Triage

After S7, a cell is a **junction cell** iff it is crossed by ≥ 2 active patches
**or** contains a segment of an intersection curve. Everything else uses §5/§6.
The escalation targets of §4.4, §5.2 and Invariant K1 also land here.

### 7.2 Inputs

The cell's tet, its clipped surface fragments, and its curve segments. **Every
input vertex is either a registry entity (§10.3 of the plan) or a snapped lattice
node** — the junction mesher creates no new geometric *identities*, only Steiner
points (§7.4). This is what keeps junctions consistent with neighbouring cells.

### 7.3 The shared-face-cache invariant

> **Invariant J1.** The triangulation of a tet face is a pure function of the
> face's node keys and of the constraint segments crossing it, computed **once**
> and consumed identically by both incident cells.

Implementation: `FaceTriCache: FaceKey → (constraint fingerprint, triangles)`,
with `FaceKey` = the sorted triple of the face's corner `NodeKey`s and the
fingerprint = the sorted list of constraint entity ids. A cache hit whose
fingerprint differs from the caller's constraint set is a **hard error**, not a
recompute — a fingerprint mismatch means the two cells disagree about the
geometry on their shared face, which is precisely the defect verifier [V9]
exists to catch.

> **Invariant J2.** The generic constrained face triangulator MUST reproduce
> §5.2's tables exactly on the inputs those tables cover. There is one shared
> routine; the kirigami tables are its closed-form values, not a parallel
> implementation. (Test obligation §13, row T-J2.)

### 7.4 Interior meshing and determinism

- Constrained incremental tetrahedralisation, constraint vertices inserted in
  ascending `NodeKey` order.
- All sign decisions use exact predicates (G0-2); cocircular/coplanar ties are
  broken by smallest `NodeKey`.
- Steiner points are permitted off the constraints only. A Steiner point's
  coordinates MUST be computed as an exact average of existing node coordinates
  (reproducible bit-for-bit) and quantised to a key; a Steiner point landing
  within the weld tolerance of an existing node snaps to it instead of being
  inserted.
- Steiner points MUST NOT be inserted on a shared face — faces are frozen by J1
  before the interior is meshed.

### 7.5 Region assignment

Seed-based: each local sub-region (a connected component of the cell's tets not
separated by a constraint face) is classified **once** at an interior sample by
the 5-ray `robust_inside` test, and the result is written to every tet of that
sub-region. Accumulated per-cut votes are *not* used inside junction cells —
they are order-dependent exactly where several patches meet, which is the failure
mode the junction path exists to remove.

### 7.6 Fallback (gate G6-0)

If G6-0 finds the local CDT untenable: sequential kirigami with **curve-node
pinning** — cuts are constrained to pass through the snapped curve nodes shared
by both surfaces. The result is a valid conforming mesh whose junction geometry
may chamfer by at most one cell (`≤ h`), logged per cell and reported in the
accuracy contract. This is the degraded mode, not the design.

---

## 8. Band templates — the k-cases (S8b)

### 8.1 Setup

Matched wall-triangle pairs (the pairing map Φ from S3) span prism cells: cap
`(a0,a1,a2)` on wall A, cap `(b0,b1,b2)` on wall B, `a_i ↔ b_i`. A pair whose
local separation is below `t_sheet(x)` is **collapsed per vertex pair** to a
single node `r_i`. `k` = number of collapsed pairs in the cell.

> **Invariant B1 (per-pair collapse ⇒ automatic conformity).** Collapse is a
> property of the *vertex pair*, not of the cell, so a pair collapsed in one cell
> is collapsed in every cell that references it. Neighbouring cells with different
> `k` therefore still agree on their shared quads with no negotiation. (PLAN 2
> B-5; verified on a mixed-k strip, §14 [6].)

### 8.2 The frozen table

| `k` | Cell | Node lists | Tets |
|---|---|---|---|
| 0 | prism | §4.3 pattern for the SNK diagonals of the three quads | 3 |
| 1 | pyramid, apex `r_i` | SNK split of the surviving quad `(a_j, a_l, b_l, b_j)`, each triangle coned to `r_i` | 2 |
| 2 | single tet | `(r_i, r_j, a_l, b_l)` | 1 |
| 3 | degenerate | none — the cell *is* the sheet triangle `(r0, r1, r2)`; its faces come from the sheet cut | 0 |

`(i, j, l)` range over the pair indices, collapsed ones first. Emit under the
canonical orientation fix.

**Positivity.** `k = 2` and `k = 1` are direct: the tet/pyramid is non-degenerate
whenever the surviving pair separations are positive, and the pyramid's apex is
off the surviving quad's (possibly non-planar) surface. `k = 0` reduces to §4.
All cases are subject to the §4.4 runtime ladder — a band cell that fails is
exactly the case PLAN 2 designed the Steiner fallback (prism centroid + face fan,
8 tets, still one geometric layer) and the regional `[THIN-SKIP]` demotion for.

**Measured on the nominal band cell (rev 1.3, corrected — informative).** The
nominal cell is a **right-isoceles cap of legs `h`** extruded by `t`, with a
collapsed pair taken at the pair's midpoint; metrics under `[V4]`'s conventions
(`AR = R / (3·r_in)`). At `t/h = 0.35`:

| `k` | min dihedral | max AR |
|---|---|---|
| 0 | 18.281° | 2.0953 |
| 1 | 19.561° | 2.0165 |
| 2 | 19.853° | 1.9662 |

The dihedral column is what rev 1.2 recorded, to the digit it was quoted at
(18.3 / 19.4 / 19.9). **The AR column is not**: rev 1.2 read 1.89 / 1.80 / 1.74,
which no cell of this family produces under the `[V4]` convention — the ratios to
the measured values are 1.109 / 1.120 / 1.130, so it is not a convention rescale
either. No frozen *rule* changes; the numbers a gate is read against do. §14 [9]
and §15 D-14.

**Band elements** carry the parent component's material, PLAN 1 records
`inside_s(parent), from_cut_p`, and are excluded from generic kirigami for that
patch (they are conforming by construction).

---

## 9. `resolve()` — the label truth table (S10)

### 9.1 Definition

Given a tet's sparse ownership record (entries `(X, side, provenance)`,
**absent ≡ outside**):

```
S      = { X : side_of(X) = inside }
Y_min  = min { Y(X) : X ∈ S }
resolve(record) = { 0 }                                   if S = ∅
                = { X ∈ S : Y(X) = Y_min }                otherwise
```

Only `Solid` components may enter `S`; `Sheet` components never claim volume
regardless of any winding-number evidence (the hard guard of plan §10.4). A
region key is the sorted X-list; every X in a key shares one Y.

### 9.2 Truth table

| # | `S` (with priorities) | `resolve` | Governing requirement |
|---|---|---|---|
| 1 | `∅` | `{0}` | R-A1 (background) |
| 2 | `{A(Y=1)}` | `{A}` | single ownership |
| 3 | `{A(1), B(2)}` | `{A}` | R-A4 — smaller Y wins, B is replaced in the overlap |
| 4 | `{A(1), A′(1)}` | `{A, A′}` | R-A3 — same-priority overlap keeps every X |
| 5 | `{A(1), A′(1), B(2)}` | `{A, A′}` | R-A3 + R-A4 together |
| 6 | `{V(0), P(1)}` | `{V}` | void inside particle (V mapped to a void material at export) |
| 7 | `{A(2), B(3), C(3)}` | `{A}` | unique minimum priority |
| 8 | `{A(3), B(3), C(2)}` | `{C}` | minimum need not be the first-listed |
| 9 | `{A(1), B(1), C(2), D(2)}` | `{A, B}` | ties at the minimum only |
| 10 | any `S` containing a `Sheet` X | `Sheet` X is never in `S` | plan §10.4 hard guard |
| 11 | record with an `ambiguous` entry | **error** | records must be complete before S10 (§12, ARB-23) |

### 9.3 Node ID sets

`N_ID(node) = ⋃ {resolve(t) : t incident tet} ∪ {X of every tagged face incident}`.
`N_ID` stores component X values only; Y is an attribute of X in the component
table and is never stored per node.

| Situation | `N_ID` |
|---|---|
| interior node of a `{3}` region | `{3}` |
| interface node between `{3}` and background | `{0, 3}` |
| node on the intersection curve of components 3 and 5 | `{0, 3, 5}` (0 only if background-adjacent) |
| welded-sheet node, sheet X=7 embedded in background | `{0, 7}` |
| lens-edge curve node of same-priority overlap `A, A′` | `{0, A, A′}` |

---

## 10. Coincidence policy table — frozen (S2)

Config `coincidence: merge | reject | warn`, default `merge`. In `warn` mode
behaviour is identical to `merge` plus one deterministic aggregate WARN line per
case (occurrence count + sorted entity list); in `reject` mode any row marked ✗
aborts the run with the offending entity pair list.

| # | Case | Geometry (merge mode) | Tags | Curves / features recorded | `reject` |
|---|---|---|---|---|---|
| C1 | Fully coincident patches, same orientation | one merged patch | all participating components | – | ✗ |
| C2 | Fully coincident patches, opposite orientation | one merged patch | all components; **per-tag orientation recorded** | – | ✗ |
| C3 | Partial coplanar overlap | 2D overlay in the common plane: shared + exclusive sub-patches | shared sub-patch carries all tags; exclusive sub-patches keep their own | overlay boundary → intersection curve | ✗ |
| C4 | Shared edge only | patches stay distinct | unchanged | contact recorded as a degenerate intersection curve | ✓ |
| C5 | Shared vertex only | patches stay distinct | unchanged | contact recorded as a point feature | ✓ |
| C6 | Tangential point contact | entities welded by registry identity | unchanged | point feature | ✓ |
| C7 | Near-coincident within `ε` | merged as coincident | all tags preserved | – | ✗ |
| C8 | Coincident, different priorities | geometry identical; volume labels resolve per §9 | every identity kept | – | ✗ |
| C9 | Sheet coincident with a solid boundary | merged patch | sheet X **and** solid X | sheet identity lives in tags | ✗ |
| C10 | Sheet coincident with a domain face | clipped to the box; patch tagged both `sheet` and `box` | both | box-clip curve | ✓ |

> **Invariant M1 (merging never erases identity).** Geometric merging always
> preserves every participating component's tag. Two physically distinct
> interfaces closer than `ε` cannot be separated at mesh scale *by definition*;
> they survive as one multi-tagged patch, and the documented escape hatch is
> lowering `ε` — not a special case in the mesher.

> **G2-3 operational boundary (frozen 2026-07-28).** Triangle-level C7 requires
> an otherwise disjoint pair with a mutual one-to-one vertex match under
> `distance² <= ε²`. Matches form deterministic complete-link clusters: every
> pair in one cluster must remain within `ε`, so a chain at `0`, `0.75ε`, and
> `1.5ε` cannot transitively collapse its endpoints. The representative is the
> lowest canonical triangle/`NodeKey` rank. At q-scale, compatible symbolic
> provenances alias one registry node while remaining listed as aliases. DD-floor,
> unresolved q-order, collapsed contact, residual crossing, and radial
> coplanarity create durable degraded-neighborhood records for S7 alternating
> projection; they are never silently skipped.

> **C10 stage ownership.** G2-2 classifies C10 and applies its merge/warn/reject
> policy. G2-5 owns the geometry part of the same row: clipping, the `box` tag,
> and the box-clip curve. Therefore G2-2 can close while `s02_arranged` and C10's
> geometric acceptance remain withheld until G2-5.

Rows C1, C2, C3, C7–C9 are the ones that make a region key multi-valued and are
therefore exactly the rows that trigger the INP material-mapping requirement
(plan §10.14): an unmapped multi-ID key is a hard export error.

---

## 11. S3 ↔ S4 coupling — monotonicity and termination

### 11.1 Frozen update rule

```
h⁽⁰⁾      = min(h_max, c_curv, c_feat, c_lfs)                    # geometry only
h⁽ⁿ⁺¹⁾    = max( h_min, min( h⁽ⁿ⁾, C(R⁽ⁿ⁾) ) )                    # running minimum
R⁽ⁿ⁺¹⁾    = regimes from thresholds  t_sheet = τ_s·h⁽ⁿ⁺¹⁾,  t_layer = τ_l·h⁽ⁿ⁺¹⁾
```

with `0 < τ_s < τ_l ≤ 1`. The `min(h⁽ⁿ⁾, ·)` is **normative**: `h` is a running
minimum and is never allowed to rise, even if a regime change would relax its
constraint. Without it the argument below is false.

### 11.2 The frozen measurement

> **Rule S3-M.** A region's separation `t_r` used for the *regime decision* is the
> exact closest-pair distance over the region's surface pairs (BVH triangle–
> triangle, vertex–face and edge–edge), computed **once** at the first S3 pass
> and **not** re-measured across coupling iterations. The ray battery and the
> `|∇t| > 0.5` densification supply the pairing map Φ, the field for
> segmentation, and the snapshot visualisation — not the threshold quantity.

Rule S3-M is what makes `t_r` a fixed geometric constant. Re-measuring it would
let both sides of the comparison fall together and destroy monotonicity, because
finer sampling can only *lower* a measured minimum.

> **Rule S3-S.** Across coupling iterations regions may only **split**, never
> merge. A child's `t_r` is a minimum over a subset, hence `≥` its parent's, so
> splitting moves regimes in the same direction as falling thresholds. A merge, if
> ever required, counts as a regime change and is caught by the oscillation guard.

### 11.3 The argument

1. `h⁽ⁿ⁾` is non-increasing in `n` (by construction) and bounded below by
   `h_min > 0`, hence convergent.
2. `t_sheet = τ_s·h` and `t_layer = τ_l·h` are therefore non-increasing.
3. `t_r` is constant (S3-M) or non-decreasing (S3-S).
4. A region's regime is `Sheet` if `t_r ≤ t_sheet`, `Band` if
   `t_sheet < t_r ≤ t_layer`, `Normal` otherwise. With `t_r` non-decreasing and
   both thresholds non-increasing, a region can only move along the chain
   `Sheet → Band → Normal`, never backwards.
5. Hence each region changes regime **at most twice**, the assignment `R⁽ⁿ⁾`
   stabilises, and with it `C(R⁽ⁿ⁾)` and `h`.

Hysteresis (enter a tighter regime at `0.9·threshold`, leave at `1.1·threshold`)
adds a dead band around each comparison. Because transitions are one-way, the
dead band cannot create a cycle — it only delays a transition by at most one
iteration.

### 11.4 Termination and guards

- Stop when no regime changed **and** `max |Δh|/h < 5 %`; hard cap **5**
  iterations; the suite is expected to converge in ≤ 3.
- **Oscillation detection**: a region whose regime differs from its value two
  iterations earlier. Action: **lock to volumetric, never to sheet**, emit WARN.
  A region also locks after its first regime downgrade.
- **Post-loop assertion (G-8)**: `ε ≪ t_sheet(x) < t_layer(x) ≤ h(x)` for every
  region, plus `ε < 0.5·τ_s·h_min` re-checked on the realised field (the
  parse-time check is on the configured field and is not sufficient).
- Hitting the iteration cap is a WARN with the unstable region list, not a
  failure: the last assignment is used and every unstable region is locked
  volumetric.

---

## 12. Arbitration failure catalog — one entry per producer

"Arbitration" = any site where the combinatorial/exact path cannot determine an
answer and a geometric sample, a fallback classifier, or an escalation decides.
Every site MUST log its own tag so the rate can be attributed. Global gate:
**arbitration < 0.5 % of cut-produced children** (a higher rate means the
combinatorial path is leaking); counters reported as `[OWN-STATS]`.

| ID | Stage / producer | Trigger | Action | Log tag | Record provenance |
|---|---|---|---|---|---|
| ARB-1 | S2 registry construction | forward error bound > `0.25·q` | escalate that construction to double-double (G0-2) | `[ARR-PREC]` | – |
| ARB-2 | S2 post-corefinement validation | residual crossing / non-manifold patch after staged precision | flag neighbourhood degraded; S7 uses alternating projection as the curve target | `[ARR-RESID]` | – |
| ARB-3 | S2 coincidence policy | any §10 row marked ✗ in `reject` mode | abort with the entity-pair list | `[ARR-COINC]` | – |
| ARB-4 | S2b closure re-check | declared solid still open beyond ε-repair | GWN becomes its inside test; WARN + closure-defect report | `[TOPO-GWN]` | – |
| ARB-5 | S2b GWN evaluation | GPU f32 value inside the margin band around 0.5 | recompute in f64 on CPU; still in band → classify outside + WARN | `[TOPO-GWN-BAND]` | – |
| ARB-6 | S3 pairing battery | confidence < 0.9 | decline conversion; region stays volumetric | `[THIN-SKIP]` | – |
| ARB-7 | S3↔S4 loop | oscillating regime (§11.4) | lock region to volumetric | `[THIN-LOCK]` | – |
| ARB-8 | S6 classification | GPU parity within margin `δ_gpu` | CPU 5-ray exact `robust_inside` | `[CLS-BAND]` | `from_lattice` |
| ARB-9 | S6 ray battery | non-unanimous parity / ray through a degenerate feature | deterministic re-shoot down the fixed direction sequence; exhausted → GWN + WARN | `[CLS-RESHOOT]` | `arbitrated` |
| ARB-10 | S7 snap | move would invert a tet (exact sign) | reject the move; if it was a required constraint target, record an unsatisfied constraint + WARN | `[SNAP-REJ]` | – |
| ARB-11 | S7 snap | 30 % shortest-incident-edge cap reached | clamp; mark under-snapped for the [V5] gate | `[SNAP-CAP]` | – |
| ARB-12 | S8 `assign_cut_sides` step 1 | a child has both strictly-inside and strictly-outside corners | `ambiguous` → `arbitrate` (5-ray at centroid) | `[OWN-DIAG]` | `arbitrated` |
| ARB-13 | S8 `assign_cut_sides` step 4 | union-find component with no sided member; oriented probe fails | `arbitrate` | `[OWN-PROBE]` | `arbitrated` |
| ARB-14 | S8 record write policy | `from_cut` vs `from_cut`, different side | reject the write → `ambiguous` → caller arbitrates | `[OWN-CONFLICT]` | `arbitrated` |
| ARB-15 | S8 guarded dry-run | negative child volume or > 1 % volume error | escalate the cell to §7, then §4.4 step 4 | `[CUT-GUARD]` | – |
| ARB-16 | S8 face-split dispatch | illegal state (dangling cut, K1 violation) or `split_4` failing the quality floor | escalate the cell to §7 | `[CUT-CASE]` / `[CUT-3EDGE]` | – |
| ARB-17 | S8 prism/quad emission | runtime positivity or 8° dihedral failure | §4.4 ladder: flip → Steiner → escalate → refine | `[CUT-PRISM]` | – |
| ARB-18 | S8 junction mesher | local CDT fails or G6-0 was a no-go | curve-pinned sequential cutting; chamfer ≤ h logged per cell | `[JCT-FALLBACK]` | – |
| ARB-19 | S8 junction seeding | 5-ray tie at a sub-region seed | GWN; else majority of face-adjacent classified tets + WARN | `[JCT-SEED]` | `arbitrated` |
| ARB-20 | S8b FEM-aware ladder | predicted band quality fails gates | ladder outcomes 2–5 (refine / sheet / volumetric+WARN / reject with report) | `[THIN-LADDER]` | – |
| ARB-21 | S9 IQD collapse guard | a collapse would fuse elements on opposite sides of a patch sharing a face | reject the collapse (evaluated on records, no tag lookup) | `[Q-COLLAPSE]` | – |
| ARB-22 | S9 hole-fill | cavity with empty record majority | `arbitrate` per section | `[Q-HOLEFILL]` | `arbitrated` |
| ARB-23 | S10 pre-derivation sweep | any `ambiguous` entry survives to labelling | debug: abort with coordinates; release: arbitrate + log | `[OWN-FIX]` | `arbitrated` |
| ARB-24 | S10 partition fill | partition volume below tolerance / suspected pinhole | WARN + [V8] pinhole diagnostic with the boundary-edge list | `[PART-PINHOLE]` | – |
| ARB-25 | S11 INP export | region key with no material mapping | hard error listing the keys (`unmapped: elset-only` → ELSET without a section + WARN) | `[EXP-UNMAPPED]` | – |

Producers ARB-12…ARB-14 are the ones the 0.5 % gate measures. ARB-15…ARB-19 are
*escalations*, not samples: they never guess, they hand the cell to a stronger
mesher, and their counters gate G6-0's go/no-go.

---

## 13. Test obligations (feeding plan §17.3)

Each frozen table gets one unit test that fails if the table is edited without
re-deriving it.

| ID | Test | Asserts | Lands with |
|---|---|---|---|
| T-K1 | Freudenthal table | 6 rows, each `orient3d > 0`; volumes sum exactly to `h³`; no interior overlap | G4-2 |
| T-K2 | Rule D | induced diagonals equal min→max corner on all 6 faces; invariance under cell translation | G4-2 |
| T-F1 | Fan face rule | `f` reproduces the P/E/Q counts 2 / 4+k / 8; identical output from both incident cells | G4-2 |
| T-F2 | Lattice conformity | randomised balanced octrees: every interior face shared exactly twice, zero hanging nodes, exact total volume | G4-2 |
| T-F3 | Bound P1 | every emitted lattice tet has `h³/48 ≤ V ≤ h³/6` | G4-2 |
| T-Q1 | Rule SNK / T2 | randomised key orders never produce a cyclic prism; the 6 patterns are reproduced | G4-2 |
| T-C1 | Face-split tables | all 5 rows; both windings identical; exact area tiling | G6-2 |
| T-C2 | Cut case table | all 6 cases: positivity, exact volume partition, boundary triangles equal the face-split output | G6-2 |
| T-C3 | Invariant K2 | a cut node within weld tolerance of a parent node is promoted, never emitted | G6-2 |
| T-J1 | Shared-face cache | both incident cells read the identical triangulation; fingerprint mismatch is an error | G6-4 |
| T-J2 | Table/triangulator agreement | the generic constrained face triangulator reproduces §5.2 exactly | G6-4 |
| T-B1 | Band k-cases | k = 0..3 counts 3/2/1/0, positivity; mixed-k strip is conforming | G7-1 |
| T-R1 | `resolve()` | all 11 rows of §9.2 | G9-1 |
| T-M1 | Coincidence table | all 10 rows in all 3 modes; C10 policy/classification at G2-2, C10 clipping/tag/curve geometry at G2-5 | G2-2 / G2-5 |
| T-S1 | Coupling loop | monotone `h`; ≤ 3 iterations on the suite; oscillation fixture locks volumetric; ε-ordering assertion fires | G3-2 |
| T-A1 | Arbitration catalog | every ARB-id is reachable by a fixture and logs its own tag; `[OWN-STATS]` counts them | G6-3 |

---

## 14. Verification record

The tables above were derived and checked with three programs (exact integer
arithmetic for every topological decision; floating point only for the
informative quality statistics). Results, verbatim:

**[1] Freudenthal table** — 6 tets, each `orient3d = +1` on the unit cube, signed
volume sum `6/6 h³` (exact tiling); interior-sample multiplicity histogram
`{0: 341, 1: 990}` over an 11³ lattice of strict-interior probes → no overlap;
all 6 induced face diagonals equal min-corner→max-corner.

**[2] Lattice conformity** — 7 configurations (uniform L1, uniform L2, one octant
refined, corner-touch refinement, diagonal/edge-only refinement, spherical shell
refinement, checker blob), 48…3 024 tets each: **exact** volume, **0** non-paired
interior faces, **0** hanging nodes in every case. 653 Freudenthal cells and 82
fan cells emitted; face cases P = 4 074, E = 266, Q = 70 — the edge-fan and
quadrant cases are genuinely exercised, not dead rows.

**[3] Template inventory** — tets per cell 6 / 18 / 30 / 48 for no-split /
one split edge / one split face / all faces split; `min V = h³/48`,
`max V = h³/6`; volume exact in each.

**[8] Transition-fan quality (rev 1.2, G4-2)** — all 5 template classes enumerated
exhaustively (6 Freudenthal rows; 2 P triangles, 8 Q triangles and all 15 non-empty
split-edge subsets of case E per face, over all 6 faces): worst dihedral
`35.2644°` and worst `AR = 1.6052`, both on case Q. Cross-checked against the
shipped `[V4]` implementation on the 287,830-tet lattice of the
`TestCaseIntersect1` reference dataset: `min_dihedral_deg = 35.264389682751705`,
`max_dihedral_deg = 125.26438968275662`, `worst_aspect_ratio = 1.6051717155225624`,
`aspect_ratio_over_gate = 0`, `below_low_dihedral = 0`. This is what corrected the
Q row of §3.7.

**[9] Conformity in the shipped implementation (rev 1.2, G4-2)** — Theorem T1
re-checked on the real build rather than the derivation program: 12 randomized
sizing fields (each spanning ≥ 2 octree levels, ≥ 100 fan cells in total) and the
287,830-tet `TestCaseIntersect1` lattice, all through `mesh-verify`'s `[V3]`:
`multi_shared_faces = 0`, `boundary_leaks = 0`, `hanging_nodes = 0`,
`non_manifold_edges = 0` in every case.

**[4] Face-split tables** — counts 1 / 2 / 3 / 4 / 4 for uncut / `split_2` /
`split_3` / `split_4` / `split_R`; 2 000 randomised faces evaluated from both
incident cells with opposite winding: identical triangulations and exact area
tiling in every case.

**[5] Cut cases** — 400 random-parent and 400 lattice-parent trials per case:
tets 2 / 3 / 3 / 4 / 4 / 6 for A / B / B′ / C / C′ / D, **400/400 valid in all
12 runs**, zero combinatorial failures in 4 800 randomised cuts, zero parent-face
conformity violations, 500/500 shared-face agreement across adjacent cut cells.
Worst min-dihedral (lattice parent): median 14.0° / 12.3° / 13.6° / 8.4° /
10.3° / 7.3°.

**[6] Band templates** — 3 / 2 / 1 / 0 tets for k = 0..3, all positive; min
dihedral 18.3° / 19.4° / 19.9°; a 24-cell mixed-k strip (half the pairs collapsed)
produced 27 tets with no face shared more than twice.

**[7] SNK stress test** — 4 000 random prisms including deliberately twisted ones:
4 000 decomposable, 0 not. Geometric validity still requires the §4.4 runtime
check — combinatorial validity is not geometric validity, and that distinction is
preserved, not papered over.

**[8] Prism patterns** — all 8 diagonal configurations enumerated; exactly the 2
cyclic sets have no 3-tet decomposition and exactly those are unreachable under
SNK (0/20 000 random key orders produced one). The other 6 are §4.3.

**[9] Template quality** — §3.7's table, with the interior-dihedral formula
(projection perpendicular to the shared edge), not a normal-angle proxy.

**[10] G2-2/G2-3 arrangement (2026-07-28)** — 43 arrangement acceptance tests
plus 2 private unit tests cover C1-C10 policy semantics in merge/warn/reject,
same/opposite orientation, retessellated full patches, partial and three-way
overlay, conforming edge/point/tangent contacts, sheet/solid and priority tags,
C10 classification with exact bounded overlap, epsilon boundary/tilt/transitive
clustering, Spade refusal/insertion propagation, cancelled-determinant DD
resolution, provenance-scoped subdivision, patch-local C7 coexistence,
symbolic registry ordering, EdgeEdge owner retention, S1 curve ownership, and
typed fallbacks. The 250-case seeded degeneracy corpus ran twice per case with
identical structures: 500 DD escalations, zero floor routes/degraded cases/hard
failures. The frozen totals are asserted in-test. Decision: **GO** on G-4;
retain per-neighborhood S7 fallback. C10 geometry remains a G2-5 acceptance
item as stated above.

Programs: `verify_lattice.py`, `verify_cut.py`, `verify_patterns.py` (session
scratchpad; they are the derivation, and §13 is their permanent form as Rust
unit tests).

---

**[9] Nominal band cell (rev 1.3, G7-1)** — the §8.2 cap family enumerated over all
six non-cyclic diagonal patterns of §4.3 at `t/h = 0.35`, and the resulting tets
measured with the shipped `tet_quality` (`[V4]`'s implementation). Right-isoceles
cap: worst dihedral `18.280994°`, max `AR = 2.095307` (`k = 0`); `19.561073°` /
`2.016547` (`k = 1`); `19.852538°` / `1.966193` (`k = 2`). Equilateral cap, for
contrast: `21.596°` / `1.7038` at `k = 0` — so the recorded 18.3° identifies the
right-isoceles cap as the cell rev 1.2 measured, and the AR column of that row does
not belong to either cap. Cross-checked by an independent exact recomputation of the
three `k = 0` tets in rational arithmetic (volume, `r_in` from the exact face-area
sum, circumcentre by exact Cramer solve, dihedrals by projection onto the edge's
normal plane): `R/(3·r_in) = 2.014973 / 2.095307 / 1.953465` and min dihedrals
`19.2900° / 18.2810° / 26.3342°`, agreeing with the shipped implementation to every
digit printed. Pinned by `nominal_band_cell_matches_the_corrected_spec_row`
(`tests/meshgen_band_tests.rs`).

**[10] Ordering-key resolution (rev 1.4, G7-1)** — on `TestCaseIntersect1` at
`h_max_frac = 0.05`, S7 produced crossings `139679` and `139681` at
`(0.20297470401197784, 0.36535446722156012, 0.33392350843706070)` and
`(0.20297470401197784, 0.36543704720828446, 0.33386320436197681)`, which differ by
`8.3e-5` and `6.0e-5` and share the weld-grid key `(2030, 3654, 3339)`. Cells 557437
`[8868, 8869, 8897, 7691]` and 557477 `[8868, 8869, 10376, 8897]`, both case C on
component 1, share face `(8868, 8869, 8897)` and hand that face's quad to Rule SNK in
opposite orders; with the keys tied they chose opposite diagonals. Measured on
`s08_cut` under `[V3]`, over the three fixes of this subtask:

| | multi-shared | hanging | non-manifold | leaks |
|---|---|---|---|---|
| before | 0 | 4,853 | 326 | 2,224 |
| after the edge-order and triage fixes | 0 | 0 | 1 | 60 |
| welding the tied pair (rejected) | 290 | 4 | 580 | 8 |
| **ordering on `1e-6 · q`** | **0** | **0** | **0** | **0** |

The last row is `[V3]` clean, and the whole report PASSes, on both that dataset and a
two-plate gap fixture; byte-identical at `RAYON_NUM_THREADS` 1 and 8.

---

## 15. Deviations and open items

Deviations from the reference implementation or from the plan's rev-2 prose, each with
its reason:

| # | Deviation | Reason |
|---|---|---|
| D-1 | **Strong** (face + edge + vertex) 2:1 balance required; the plan said "any finer neighbour" makes a transition cell | Face-only balance leaves edge-adjacent finer leaves free to put a node inside an edge of an unsplit face — a hanging node the face rule would not see. §3.1 L1/L2 depend on strong balance |
| D-2 | A cell with **any** split edge is a fan cell, not only one with a split face | Otherwise a Freudenthal neighbour would emit a plain diagonal across a face whose edge carries a midpoint (crack). This is the non-obvious half of Theorem T1 |
| D-3 | `split_3`'s quad diagonal uses Rule SNK, not the reference implementation's shorter-distance test with a `1e7/1e4/1e0` coordinate tie-break | The distance rule gives no global non-cyclicity guarantee; SNK does (T2), and it is what lets §6's prisms be provably decomposable. The distance rule survives only as the §4.4 pairwise flip, where it is a *quality* choice guarded by a validity check |
| D-4 | `split_4` is the medial split; the reference implementation's long-side/right-triangle variants are dropped | Those variants are quality heuristics for the SAMR hanging-node path, which this module does not have (no hanging nodes by construction). Retaining them would add order-dependent branches to a case that should mostly escalate anyway |
| D-5 | Invariant K2: cut nodes coincident with parent nodes are **promoted**, not grounds for deleting the element | The reference implementation removes the element, leaving a hole. Promotion is closed-mesh-preserving and moves the state to a legal table row |
| D-6 | `split_R` (rim termination) added to the face-split table | R-C1 requires a sheet's cut front to terminate inside a cell; the reference table has no such row. Needed by G6-5 |
| D-7 | Explicit 6-case cut table (§6) replaces the reference implementation's search-based `connect_and_fold` + `validate_internal_cuts` + fan-pivot recovery for single-patch cells | The single-patch cut of a tet has exactly six configurations; enumerating them is deterministic, provably volume-exact, and removes the `[KIRI-WARN] NEW CASE NEEDED` force-fold path that knowingly produces mesh cracks. Fan-pivot recovery is superseded by the §4.4 ladder, whose last two rungs (escalate, refine) always terminate |
| D-8 | `h` is a **running minimum** across coupling iterations (§11.1) | The plan asserted monotonicity; without the explicit `min(h⁽ⁿ⁾, ·)` it does not hold, because a regime change can relax a constraint |
| D-9 | Rule S3-M: the regime threshold quantity is the closest-pair distance measured **once**, not the re-sampled field | Re-measurement lowers `t_r` as sampling densifies, which breaks the monotonicity argument on both sides of the comparison |
| D-10 | Rule S3-S: regions may split but not merge across coupling iterations | Merging takes a minimum over a union and can lower `t_r`, reversing the regime direction |
| D-11 | C7 uses complete-link epsilon clusters, not transitive pairwise union | Pairwise union can merge endpoints farther apart than epsilon; complete-link preserves the policy's geometric-resolution bound |
| D-13 | §3.7's Q row corrected from `45.000°/90.000°/1.3938` to `35.264°/125.264°/1.6052` (rev 1.2) | The original row repeated the centre–corner–midpoint row's numbers instead of measuring all eight quadrant triangles. The value is forced by Rule D and §3.4, so no rule changed; the *prediction* G4-3 measures against did |
| D-16 | S8 orders nodes on `1e-6 · q` rather than on the weld grid `q` (Rule K-O, rev 1.4) | §1.2's weld invariant does not cover constructed crossings, which are not welded against each other; ordering on a grid that does not separate them makes Rule SNK's "smallest key" not a total order. Nothing about the *rule* changed - it still depends only on position, never on node indices, which is what §1.2 requires of it |
| D-14 | §8.2's informative measured row corrected: the AR column `1.89 / 1.80 / 1.74` replaced by `2.0953 / 2.0165 / 1.9662`, and the nominal cell's shape stated explicitly (rev 1.3) | The AR numbers matched no cell of the §8.2 family under `[V4]`'s convention, and the row did not say which cap it was measured on — so the numbers could not be reproduced, which is what a *measured* row exists for. Two independent computations agree (§14 [9]). The dihedral column was correct and is unchanged; no frozen rule moved |
| D-15 | The §4.4 ladder's flip step is implemented for band cells as a **pairwise** decision taken by the layer, not by the cell | A diagonal flip is not a pure function of the quad, so a cell taking one alone leaves the neighbour that shares the quad non-conforming. The layer owns both incident cells and accepts the flip only if both still mesh — which is what the reference thin-feature design §3.8's "attempted only pairwise" requires and what `cut.rs`'s §4.4 ladder skips |
| D-12 | A split parent edge forbids CDT chords that bypass an intermediate registry node | Snap-rounded C3 coordinates can be microscopically off the analytic parent edge; allowing the bypass chord emits a sliver and makes retessellated coincident patches nonconforming |

Open items explicitly **not** frozen here, with their owners:

- Error bounds, escalation thresholds, `δ_gpu`, ray-direction constants and the
  exact-predicate inventory → **G0-2**.
- Post-snap transition-cell quality in production geometry → **gate G4-3**
  (§3.7 gives the pre-snap prediction the gate is measured against).
- Junction CDT viability and the go/no-go on §7.6 → **gate G6-0**.
- G2-3 is closed by §10/§14. In-situ fallback rates remain diagnostics; changing
  the boundary or trigger list requires a new spec revision.
