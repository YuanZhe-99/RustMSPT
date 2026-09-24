# SPEC — Mesh generation: geometry and topology freeze (subtask G0-1)

**Status:** frozen (rev 1.6, 2026-09-23 — audited clause by clause against the code at `891badc`; §0 gains `[R1]` and re-states the two items that were left unfrozen; §1, §3–§9, §11 and §12 gain as-built notes; §5.4, §7.7, §8.3 and §8.4 are new (as built); §13 gains a status column; §14 [11] is the audit record; §15 gains D-17..D-43 and its open-item list is re-stated. No frozen *table* changed. rev 1.5; §7.1's triage widened 2026-08-20 — a cell the surface's trace crosses is offered §7.2 ahead of §6's table, additive, see the note there; rev 1.4: §1.2 gained Rule K-O on 2026-07-31 — an ordering key must separate the nodes it orders, which the weld grid does not for constructed crossings — see §14 [10] and §15 D-16; G2-3 degeneracy boundary and C10 ownership clarified 2026-07-28; §3.7's informative quality prediction corrected 2026-07-30 — see the note there and §14 [8]; §8.2's informative measured band-cell row corrected 2026-07-31 — see the note there and §14 [9]). Normative for all `src/meshgen/` work.
**Date:** 2026-07-31 (rev 1.6: 2026-09-23)
**Subtask:** G0-1 (Phase G0, tier T3) of [`PLAN_mesh_generation.md`](PLAN_mesh_generation.md); the rev 1.6 audit is that plan's M-0.1.
**Scope (from the plan's G0-1 acceptance):** Freudenthal node orderings +
signed-volume table; centroid-fan face rules; kirigami split tables; junction
local-PLC spec incl. the shared-face-cache invariant; band k-cases; `resolve()`
truth table; coincidence policy table; the S3↔S4 monotonicity argument; the
arbitration failure catalog. *Acceptance: every template/case has node lists +
a positivity argument; one catalog entry per producer.*
**Companion freezes:** G0-2 (predicates, error bounds, GPU margin certificates —
every "exact predicate" and "error bound" referenced here is specified there),
G0-3 (VTU schema, check catalog, accuracy table).

> **Plan cross-references (rev 1.6, 2026-09-23).** This document was frozen against the rev-2
> plan, whose section numbers it cites as "plan §N" / "§N of the plan". That document was deleted
> on 2026-09-01 (plan D-2) and replaced by `PLAN_mesh_generation.md` v3 (rev 3.1). Every such
> citation resolves through the plan's **Appendix B.0** (a map from the old section numbers to
> where the content lives now) and **Appendix A** (the record's findings, keyed by the old section
> numbers). The plan's own ids are `M-n.m` (subtasks), `D-n` (owner decisions), `MG-nn` (the
> 2026-09-11 review's findings) and `X-n` (measured-and-closed routes).

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
- Two items were deliberately **not** frozen here at rev 1.0 because a prototype gate owned
  them: transition-fan element quality in production geometry (gate G4-3) and
  junction-cell CDT viability (gate G6-0). G2-3 completed on 2026-07-28 and its
  arrangement boundary/fallback decision is now frozen in §10 and §14. Both open items are
  re-stated below (rev 1.6).

> **Requirement [R1] (moved here from the plan, rev 1.6).** No fallback may abandon conformity to
> the interface. Every cell the surface passes through is subdivided so that the surface is a union
> of element faces. A cell the kernel cannot mesh is a gap in the kernel or in a preceding stage and
> is closed there — never diverted to a construction that gives up the defining property. A cell
> nothing can mesh is a hard error with a per-cell dump, because a wrong mesh delivered quietly is
> worse than no mesh delivered loudly. `cdt.rs` cites `[R1]` by this name.

- **The two "deliberately not frozen" items, re-stated at rev 1.6.** *Gate G6-0 (junction-cell CDT
  viability)* is answered: the local mesher of §7.2–§7.4 exists (`src/meshgen/cdt.rs`, hand-rolled,
  with its own inline tests) and is exact where it runs — but it runs only under the prototype
  handle `RUSTMSPT_PLC_PASS`, and the shipped fallback is a construction §7.6 does not describe
  (§7.7, D-17). The go/no-go the gate asked for is therefore recorded as **go on the kernel, no on
  the fallback**; plan M-2 makes the kernel complete enough to delete the fallback, and M-3 removes
  the handle. *Gate G4-3 (post-snap transition quality)* is measured on fixtures with a non-empty
  transition population (`tests/meshgen_quality_gate_tests.rs`: pre-snap, post-snap, post-cut) and
  not yet on the acceptance/reference matrix (plan M-5.1). Neither item is frozen here; both are
  now *located*.
- **Frozen is a statement about change control, not about correctness** (plan R8). A clause an
  audit shows to be insufficient is amended with a §15 row and a §14 record; "the implementation
  matches the frozen text" closes no finding that says the text is wrong.

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

> **As built (rev 1.6).** Each stage applies *one* transposition, and the emitted node *order*
> differs between stages: S5 (`lattice::oriented`) swaps `n1, n2` as written here; S8
> (`cut::orient_positively`) and the §7.2 kernel (`cdt::orient`) swap `n0, n1`; the kernel's
> canonical tet form fixes slots 0–1 by node id and swaps `n2, n3`. Every choice is an odd
> permutation, so the emitted element is positive in all cases; a consumer that reads a face by
> slot (`F_i` opposite `n_i`) must take the element as emitted. Two implementation notes: the
> §7.2 kernel's `orient` decides the sign with a floating triple product rather than the exact
> predicate (its flat-tet guard, volume below `longest² · tol`, is what excludes the cases where
> the two could differ — D-36), and `junction.rs::TET_FACES` is an *unordered* slot table (rows
> 1 and 3 are not outward) whose every consumer key-sorts the slots first; the frozen outward
> table is `verify.rs::TET_FACES` / `cdt.rs::tet_faces`. Degenerate pieces: a fan tet whose
> `orient3d` is exactly `0` is dropped and counted (`n_degenerate_fan_pieces`), never emitted.

Outward-oriented faces of a positively oriented tet, `F_i` opposite `n_i`:

| Face | Nodes (outward) |
|---|---|
| `F0` | `(n1, n2, n3)` |
| `F1` | `(n0, n3, n2)` |
| `F2` | `(n0, n1, n3)` |
| `F3` | `(n0, n2, n1)` |

### 1.2 Node keys and the total order

`NodeKey(p) = (round(p.x/q), round(p.y/q), round(p.z/q))` on the weld grid
`q = 0.1·ε` (`SPEC_meshgen_numerics.md` §1.2), ordered **lexicographically**. Two distinct
**welded** mesh nodes — S0/S2 vertices and S5 lattice nodes — MUST have distinct keys on `q`
(the weld invariant). Constructed nodes (S7 crossings, S8 cut and Steiner nodes) are not
welded against each other and MAY share a `q`-key; Rule K-O below governs how they are
ordered. Verifier `[V2]` tests coincidence on `duplicate_node_tol_frac · diag` (default
`1e-6`, finer than `q` at the default `ε`): it is the contract's duplicate-node gate, not a
check of this invariant (rev 1.6, D-35).

> **Rule K-O (rev 1.4, added by G7-1).** The weld invariant is a statement about
> *welded* nodes, and S7's crossings are **constructed** points that are not welded
> against each other: two of them can land closer together than `q` and therefore
> share a key. A stage that orders nodes MUST therefore build its ordering key on a
> grid fine enough to separate the nodes it actually holds - S8 uses
> `KEY_ORDER_REFINEMENT · ε = 1e-6 · ε` (which is `1e-5 · q`; rev 1.4 wrote `1e-6 · q`, a
> transcription of the constant's intent rather than of the code — the measurement in §14 [10]
> was made with the grid the code uses, D-35) - and MUST NOT assume the weld grid separates them. Ordering on a grid that does not is a
> silent conformity failure, not a rounding nuisance: "smallest node key" stops being
> a total order, the tie falls to whichever node the caller happened to list first,
> and two cells sharing a quad list it in opposite orders and split it on opposite
> diagonals. Welding the tied pair instead is **not** the remedy - it also merges
> crossings on different edges and collapses the cells between them (measured: 290
> multi-shared faces and 580 non-manifold edges where there had been 0 and 1).
> Verified in §14 [10].

> **As built (rev 1.6, D-35).** The key grid is not one grid across stages. S0 and S2 weld on
> `q = 0.1·ε` and keep the **first** vertex's raw coordinates under a key (a quantised key is
> identity, never a coordinate — numerics §1.5). S7 breaks ties between equidistant snap
> targets on a key taken at `ε`, welding nothing. S8 builds its whole ordering table on
> `1e-6 · ε` and interns the §7.2 arena on the same grid; its two fan centroids are keyed on
> `ε`, which is harmless only because a centroid is interior to one cell and never enters a
> shared-entity comparison. Node *indices* MAY fix an emission order or an identity (the loop
> rotation start in the §7.2 kernel, the canonical tet form, piece grouping, interface-cell
> dedup), because ids are a deterministic function of the run (R-P2); they MUST NOT decide a
> diagonal, an apex, a split or any quantity two cells must agree on. Implementation pointers:
> `predicates::orient3d` (the one negation of `robust::orient3d`, Rule N10, pinned by
> `orient3d_sign_test`), `predicates::node_key`, `cut::NodeKey`, `cut::KEY_ORDER_REFINEMENT`,
> `facecache::face_key`.

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
ordering conventions, and cache coherency with a pure function. For lattice faces (§3), quads
(§4) and §5 faces the pure function alone gives conformity and no cache is needed. For the
faces of escalated cells the §7.3 `FaceTriCache` is, as built, **authoritative**: it stores the
canonical-frame triangulation (including crease chords) once and both cells re-wind it outward;
that is what lets a chord added to the *cache* reach both owners identically where adding it per
cell cracked the mesh twice. The mismatch check in §7.3 is therefore a conformity mechanism and
must be an assertion — which it is not yet (rev 1.6, §7.3 as-built note, D-18).

### 1.4 Emission order

Cells are processed in ascending Morton key order (lattice). **As built (rev 1.6, D-37)** the
cut stages keep that order: S8 processes lattice cells in index (Morton) order and appends each
cell's elements in the order its table row, fan or §7.2 kernel produced them; interface faces
are emitted sorted by `(component, key-sorted nodes)` and deduplicated per node set in
`cut_to_doc`. The `(Y, X)` then smallest-node-key order rev 1.0 stated for cut stages does not
exist, and there is no canonical re-sort at export. Within a cell, template rows are emitted in
table order. Together with Invariant C this gives the strict-determinism contract (plan
Appendix B.2; rules in `SPEC_meshgen_numerics.md` §8) a deterministic element ordering; R-P2's
byte-identity across thread counts is measured on that order.

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

**Domain trimming and balance, as built (rev 1.6, D-38).** The octree root is a cube around
the domain box. Cells wholly outside the box are dropped, but only at levels `≤ forest_level`
(the coarsest level whose cells already satisfy `h_max`), so the dropped set is always a union
of whole subtrees — load-bearing for L1: a face whose centre is a lattice corner then always has
all four finer neighbours present (dropping one finer cell measured 216 hanging nodes and 8
non-manifold edges on a `1×1×0.35` domain). Consequently the lattice may overhang a non-cubic
domain by up to one `forest_level` cell per axis; S8 trims the overhang and `[V3]`'s
boundary-leak rule does not apply to the pre-cut lattice (contracts §4.4). Balance is achieved
by refinement only — a leaf is split, never merged — by a ripple pass over the 26 neighbour
directions to a fixed point; a child created by balance inherits its parent's sizing value `h`,
so balance never changes the field it balances; leaves at level `< 2` are never split by
balance. The balance split count is reported in the S5 stats (`[S5/G4-2]`). Pass one of the
lattice build reads its node set back off the triangles pass two will emit, never from the split
state — deriving it is what broke the trimmed case.

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

**Budget (as built).** Before emission the implementation bounds the tet count by
`6·N_Freudenthal + 48·N_fan` and refuses the build (`InvalidConfig`) when that exceeds
`LatticeOptions::max_tets` (default `LATTICE_MAX_TETS = 20 000 000`). The bound is this
section's inventory maximum, not a measurement.

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
fallback (reference-verbatim 5-tet + SAMR, plan D-5 / Appendix A §10.6–§10.7) remains available.

> **Measured (rev 1.6).** The post-snap / post-cut half of G4-3 was measured as gate G6-6
> (`tests/meshgen_quality_gate_tests.rs::transition_fans_survive_the_snap_and_the_cut`) on a
> synthetic sphere fixture (`h_max 0.2`, `h_min 0.05`): snapping erodes the fans' worst dihedral
> from `35.264°` to `35.073°` while Freudenthal cells stay at `45.000°`, and after the cut the
> fans' p5 (`5.54°`) is no worse than the Freudenthal p5 (`5.27°`) — the fans are not the weak
> link. The gate's acceptance is `snap_fan.worst > 0.5·pre_fan.worst` and
> `cut_fan.p5 > 0.25·cut_freudenthal.p5`; it passes. Measurement on production geometry (the
> acceptance cases) remains open under plan M-5.1. (`cargo test --release --test
> meshgen_quality_gate_tests -- --nocapture`, 2026-09-23.)

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

Implementations return the cyclic outcome as a failure rather than asserting it away; in §6
it is treated as an illegal row (`[CUT-CASE]` escalation), because a non-SNK caller of the
prism routine could reach it.

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
volumetric with `[THIN-SKIP]` (the FEM-aware ladder of §8.4, outcome 4).

> **As built (rev 1.6, D-29).** The ladder above is the design; two shorter ladders ship, and
> neither is 1 → 2 → 3 → 4. **§6 prism/quad pieces (cases B, B′, C, C′, D):** the guarded
> dry-run checks `orient3d > 0` on every child and the 1 % volume closure (§6) — the 8° floor is
> *measured* (`CutStats::min_dihedral_deg`) but not gated, `Escalation::Quality` is declared and
> never constructed, and ARB-17's `[CUT-PRISM]` cannot fire; on failure the cell escalates to §7
> (`[CUT-GUARD]`, step 3) and stops. **§8.2 band slabs:** the SNK row is checked for orientation,
> the floor (`meshgen.thin.band_min_dihedral_deg`, default 8°, accepted range (0°, 70.5°) — a
> quality setting, not a correctness one) and a 1 % boundary-volume closure
> (`BAND_VOLUME_TOLERANCE`); on any failure the slab takes step 2 directly — the mean of its
> distinct corner positions as a fresh apex and one tet per boundary facet (8 for a prism),
> `regime = 2`, exactly-degenerate pieces dropped and counted — with **no** floor, no volume
> check, no flip, no escalation and no refinement. The pairwise flip (step 1, D-15), the
> *checked* Steiner rung and the regional > 5 % demotion exist in `thin.rs::mesh_band_layer`,
> which is unit-tested and has **no pipeline caller**; `[V7]`'s `ThinSkipRegion` array
> therefore has no producer and its `thin_skip_regions` metric reads 0 on every run. Step 4
> (refine and re-cut) does not exist in S8: refinement is the pipeline's Invariant K1 loop around
> S4–S7 (§5.1), which a §4.4 failure never requests. The `[THIN-SKIP]` lines a run prints are
> S3's per-region skip taxonomy (ARB-6: `LowConfidence`, `Speck`, `Undersampled`,
> `MidSurfaceInvalid`, `MidSurfaceUnbuildable`, `IntersectionWedge`), a different mechanism.
> `cut.rs:14–23` records the §6-path shortcut as a decision (plan S-5, D-21).

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
>
> *As built (rev 1.6, D-21).* The remedy runs in the **pipeline**, not in S8: S7 reports a
> doubly-crossed edge (and an uncovered locked-curve segment) as a refine request, the sizing
> field is re-asked for half that length scale there, and S4–S7 rerun, at most `K1_MAX_PASSES = 3`
> times or until the `h_min` floor is reached; cells still carrying such an edge then reach S8,
> which escalates them to §7 (`[CUT-3EDGE]`). S8 itself never refines. The **second** crossing on a
> K1 edge is kept as a distinct cut node (`second_index`), ordered along the edge, visible only to
> the junction path's face description (§5.4) — the §5.2 table never reads it; a third or later
> crossing on one edge is discarded (a feature below one element belongs to refinement); a second
> crossing within the weld quantum of the first collapses to it; `RUSTMSPT_NO_K1_SECOND_CUT` is
> the ablation handle (plan M-3.2 deletes it).

> **Invariant K2 (no degenerate cut nodes).** A cut node within the weld tolerance
> of a parent node is **promoted** to that parent node, which becomes an on-cut
> vertex; a cut node is never emitted coincident with a parent node.

K2 replaces the reference implementation's behaviour of marking such an element degenerate and *removing*
it (`kirigami_divide` phase 2), which leaves a hole in the mesh. Promotion keeps
the mesh closed and moves the configuration to a legal row of the table below.

> **Shared cut nodes (as built, rev 1.6).** Two components whose first crossings on one edge
> coincide within `ε` (exact face contact) receive **one** cut node for both
> (`contact_edges`); the face then carries one cut per edge for both components and takes an
> ordinary §5.2 row (`face_is_single_patch`: two components whose per-edge cut maps are
> identical are one surface reached twice). §8.2's rim collapse shares a node the same way but
> also declares a rim. Both make a two-component cell a *welded pair* that takes §6 with the
> second component settled as the first's complement.

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
floor. *As built (rev 1.6, D-30):* the row cannot arise from the §6 table (a tet face has at
most two straddling edges) and is reachable only through the junction path's face description
(§5.4) on an already-escalated cell; no quality test is applied to the centre triangle; and
`[CUT-3EDGE]` is emitted for cells escalated under Invariant K1, not for this row.

`split_R` is the open-sheet rim-termination case (R-C1): the cut front ends at an
interior point `r` of the face. `r` is a shared node of both incident cells, so
both produce the same four triangles. A `(1 cut edge, 0 on-cut vertices, no rim)`
state is **illegal** — a cut segment with a dangling end — and escalates. *As built (rev
1.6, D-30):* the row is frozen and unit-tested but has **no producer** — S7 does not construct
interior rim endpoints, so `FaceCutState::rim` is always `None` in production; a sheet whose
cut front ends inside a cell reaches S8 as a one-class union-find in `cut_welded_sheet` and
escalates to §7 (`[CUT-CASE]`), where it is chamfered.

### 5.3 Verified properties

Over 2 000 randomised faces per case, evaluated from both incident cells with
opposite winding and different rotations: identical triangle sets (500/500 on the
dedicated adjacency test) and exact area tiling of the parent face. §14 [4], [5]. *In-tree
(rev 1.6, `tests/meshgen_cut_tests.rs`):* 2 000 randomised `split_2` and `split_R` faces tile
the parent exactly; 2 000 shared `split_3` faces evaluated from two cells with opposite apexes
give identical triangle sets; `split_3` and `split_4` are covered by §6's conformity tests
(4 800 cuts and all 81 side assignments × 200). The 500/500 figure was the out-of-tree script's.

### 5.4 Faces outside the table (as built, rev 1.6)

A face carrying two cut nodes on one edge (Invariant K1's second crossing) or cut maps from
two distinct patches is not a §5.2 state. It is described by its **boundary walk** — the three
corners with each edge's cut nodes inserted in order of distance from the canonical first
endpoint (never by node id: sorting by id is still conforming but walks the boundary backwards
over that stretch, and everything built on the loop inherits it) — plus the on-cut corners of
each component that cuts the face, and is triangulated by §7's face rules: the crossed-face
chords (§7.7 B2) or the loop fan. Two components whose per-edge cut maps are identical are one
surface reached twice and take the §5.2 row (`face_is_single_patch`). This is the face
description every escalated cell uses (`face_states`), and it is a pure function of the face's
own state, so both owners derive it identically (Invariant C).

---

## 6. Single-patch cut of a tet — the complete case table (S8)

Classify the parent tet's four nodes as `I` (strictly inside the patch's
positive side), `O` (strictly outside) or `C` (on-cut). A cut exists iff
`|I| ≥ 1` and `|O| ≥ 1`, which leaves exactly six configurations. `m(i,j)` is the
cut node on edge `(i,j)`, `i ∈ I`, `j ∈ O`.

**Inputs (as built, rev 1.6, D-31).** `I`/`O` come from S6's per-vertex classification and
`C` from S7's on-cut set; the cut nodes come from S7's crossings. A cell is handed to the table
only if, on every edge, "the endpoints straddle" equals "the edge carries this component's cut
node"; otherwise it escalates (`[CUT-CASE]`, `Inconsistent`). Sides are never derived from the
crossings (deriving them was tried twice and produced 5,698 hanging nodes). The driving
component is the smallest `Ambiguous` solid of the cell's record; a crossing by a component S6
did not call ambiguous, or by a non-solid, is likewise `Inconsistent`.

| Case | `(#I, #O, #C)` | Node lists | Tets |
|---|---|---|---|
| **A** | `(1,1,2)` | `(I, C1, C2, m)`; `(O, C1, C2, m)` where `m = m(I,O)` | 2 |
| **B** | `(1,2,1)` | inside `(I, m1, m2, C)`; outside = SNK split of quad `(m1, O1, O2, m2)`, each triangle coned to `C` | 3 |
| **B′** | `(2,1,1)` | B with `I`/`O` exchanged | 3 |
| **C** | `(1,3,0)` | inside `(I, m1, m2, m3)`; outside = prism `(m1,m2,m3)/(O1,O2,O3)` via §4.3 | 4 |
| **C′** | `(3,1,0)` | C with `I`/`O` exchanged | 4 |
| **D** | `(2,2,0)` | inside prism `(I1,m11,m12)/(I2,m21,m22)`; outside prism `(O1,m11,m21)/(O2,m12,m22)`, both via §4.3 | 6 |

`m_ij = m(I_i, O_j)`. Configurations with `|I| = 0` or `|O| = 0` emit the parent whole, with
its side settled as follows (rev 1.6, D-31): `|O| = 0` and `|I| ≥ 1` (every other vertex
on-cut) → inside the patch (row **I**); `|I| = 0`, `|O| ≥ 1`, at least one on-cut vertex and no
cut edge → the tet's centroid is classified by the exact point classifier, `Some(true)` → inside
(row **i**, counted and logged `[S8/G6-2] … interior sample`), otherwise outside. S7's
snapping makes rows I and i systematic wherever a planar face lies on a lattice node plane
(A-7b's lower-plate corner is the case that forced them). A configuration with I and O present
that matches no row, or an edge carrying a cut node on a cell the table did not cut, escalates
(`[CUT-CASE]`). A face all of whose nodes are on-cut is not split; it is declared an interface
face by the labelled-boundary pass (`declare_labelled_boundary`), which tags every mesh face
whose two side elements resolve to region keys differing by one component — so an all-on-cut
face is tagged iff it separates material, like any other face. Case D's interface quad
`(m11, m12, m22, m21)` is split by Rule SNK.

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

**Interior check (as built, rev 1.6, D-31).** The table fits one planar cut to the component's
crossings; where the cell straddles a convex edge or corner of the body the far child can still
hold material. After the row is applied, each child's centroid is classified by the exact point
classifier; a child whose *certain* answer contradicts the row's side escalates the whole cell to
§7 (`[JCT-CELL]`), because relabelling it would contradict the interface face the row emitted
between the children. An uncertain answer never escalates. The check is unscoped (asked of every
cut cell: the `on_cut`-only scope was a cost bound that measured no cost and hid the rest of the
defect); it is what took `[V5]`'s misattribution to zero on eight of nine cases.

**Welded sheets (as built).** A declared sheet is cut by the same table with `I`/`O` taken
from the cut itself: a union-find over the cell's uncut edges must yield exactly two classes; the
class containing the lowest slot is labelled `I`; straddle/crossing agreement is checked as
above; ownership is unchanged on both sides (`welded: true`, C0) and only the tagged interface
distinguishes them. One class (the cut front ends inside the cell — `split_R`'s case) or three
classes escalate.

**Guarded dry-run (ported).** Before committing, the cut MUST be validated: all
children positively oriented, `|Σ V_child − V_parent| ≤ 1 % · V_parent`, and no
node other than parent nodes, registry nodes, or the cut nodes of this cut.
Failure → `[CUT-GUARD]` → §4.4 ladder step 3. *As built (rev 1.6, D-31):* every child is
checked positively oriented (`orient3d = 0` or non-positive volume fails), the 1 % closure
(`CUT_VOLUME_TOLERANCE`) is checked, and `V_parent > 0` is required; the node-membership
condition is **not** checked — the rows are built only from parent and cut nodes by
construction, and the escalated path is validated by `cell_fan_is_conforming` instead. The
worst per-cut minimum dihedral is recorded (`CutStats::min_dihedral_deg`) but not gated
(§4.4 as-built note).

**Raw quality (pre-S9), measured.** Worst per-cut minimum dihedral, cutting a
Freudenthal-shaped parent at random positions: median 14.0° / p5 6.1° (A),
12.3° / 4.2° (B), 8.4° / 2.6° (C), 7.3° / 1.8° (D). Cutting produces slivers by
construction — these numbers are the input budget for S9 (IQD + Phase-5.5), not
an acceptance gate, and they justify keeping S9 mandatory rather than optional. *Source:* the
out-of-tree `verify_cut.py` (session script, not in the repository); not reproduced in-tree,
where `CutStats::min_dihedral_deg` reports the run-wide worst value only (R6: informative,
unreproduced).

---

## 7. Junction cells — local PLC mesher (S8)

### 7.1 Triage

After S7, a cell is a **junction cell** iff it is crossed by ≥ 2 active patches
**or** contains a segment of an intersection curve. Everything else uses §5/§6.
The escalation targets of §4.4, §5.2 and Invariant K1 also land here.

> **As built (rev 1.6, D-28).** A cell is a junction cell iff two or more distinct
> **components** leave cut nodes on its six edges (the two components of a welded pair — a
> collapsed or contact edge — count as one), or S6 records more than one `Ambiguous` solid there,
> or a solid and a sheet cross it. Two *patches* of one component crossing a cell do not by
> themselves make it a junction; they reach §7 only through Invariant K1 (`MultiCrossing`) or
> through the checks below. **Containing a segment of an intersection curve is not a trigger**:
> a curve pierce only selects the fan apex on a face of a cell that escalated for another reason
> (§7.7 B2); forcing pierced cells to escalate was measured as identical `[V6]` at 2–9 % more
> elements and reverted. Three further conditions send a cell to §7: (a) *inconsistency* — S6's
> vertex sides and S7's edge crossings disagree (§6 inputs, `[CUT-CASE]`); (b) the
> *table-vs-interior* check (§6, `[JCT-CELL]`); (c) the guarded dry-run (`[CUT-GUARD]`,
> `DryRun`). §4.4's `Quality` target is declared and never produced (D-29). Precedence: S8b's
> band split (§8.3) is tried before the junction split, and the junction split is offered only
> to escalated cells §8 declined.

**Amended 2026-08-20 (rev 1.5), additive.** A cell whose faces the surface's
trace crosses is **also** offered §7.2's mesher, ahead of §6's table, and falls
back to §6 wherever §7.2 declines. The rule above is unchanged for everything
the trace does not touch.

> **As built (rev 1.6, D-23).** This amendment is active only under the prototype handle
> `RUSTMSPT_PLC_PASS`; on the default path no cell is offered §7.2 and the triage is the
> original rule above with its as-built conditions. Where §7.4 declines on the gated path the
> cell is **not** returned to §6: it keeps its augmented faces and is filled by the facet-split
> fan (each facet-separated piece coned to its own centroid) or, where even that cannot be built,
> by a whole-cell centroid fan over the same boundary — §6's faces are §5.2's and would not match
> the neighbour's augmented triangulation. A junction cell the trace touches is likewise offered
> §7.2 first and reaches §7.6 only if excluded; a cell that would escalate is excluded from §7.2
> only where it shares a curve-pierced face with an escalating cell that carries no augmented face.
> Plan M-3 makes this path the only path and deletes the handle.

The reason is **conformity, not expressiveness**. §7.2's constraints are the
surface fragments clipped to the cell, and a fragment's boundary on a shared face
must be an edge of that face's triangulation. §5.2 draws one chord per face,
between that face's two crossings; a curved surface crosses one lattice face with
several triangles, so its trace there is a polyline with interior vertices and the
chord is not the fragment's boundary. Measured on the acceptance suite, the two
triangulations differ on **99.2 %** of A-8's junction cells — so a face
triangulation carrying the trace cannot be delivered to one owner and withheld
from the other, and the triage has to cover both. The population the trace touches
is 18.4 % of A-8's lattice, 19.3 % of A-6a's and 17.8 % of A-3's. *(rev 1.6: the rev 1.5
figures name no runnable source; re-measured 2026-09-23 with `RUSTMSPT_PLC_PASS=1
RUSTMSPT_CUT_DIAG=1 rustmspt mesh --config data/output/acceptance/a3.yaml` at `891badc` the
traced population is **17,111 of 121,764 = 14.05 %** on A-3 — the definition of "traced" is
`has_augmented_face`, a face where the 2-D CDT differs from §5.2, and an earlier definition
counted more. Informative, per R6.)*

§6's table is unchanged and remains normative where it runs: it partitions the
parent exactly and its conformity with §5.2 is verified over 4 800 randomised
cuts (§14 [5]). This amendment changes **which cells reach it**, not what it does.

### 7.2 Inputs

The cell's tet, its clipped surface fragments, and its curve segments. **Every
input vertex is either a registry entity (numerics §5) or a snapped lattice
node** — the junction mesher creates no new geometric *identities*, only Steiner
points (§7.4). This is what keeps junctions consistent with neighbouring cells.

> **As built (rev 1.6, D-28).** The gated kernel takes the cell's tet, its boundary
> triangulation (the augmented faces of the trace pass plus §5.2 for the rest) and the surface
> fragments clipped to the tet (`fragment_facets_in_cell`, on-plane tolerance `edge · 1e-6`).
> **Curve segments are not an input to the mesher**; locked curves enter S8 only through the
> per-face pierce point and the on-curve walk node (§7.7 B2). And S8 does create new geometric
> identities: the trace pass interns the endpoints of a component's trace on a lattice edge or
> in a face interior, once per edge or per face so both owners share them (J1 by sharing), and
> interns the arena points some tet uses (`plc-arena`); the default path interns one face-level
> Steiner node per face that needs one (§7.4 as-built note) and one apex per fan. Measured on
> A-3 gated: 32,414 interned points, 15,091 of them face-interior. None is a registry entity.

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

> **As built (rev 1.6, plan MG-06, M-2.0).** `FaceTriCache` exists (`src/meshgen/facecache.rs`) and
> is authoritative for every escalated cell's faces. Two departures from the text above are
> recorded as **D-18** and are not accepted: (1) the wrapper `check_face_cache` (`cut.rs`) turns a
> `FingerprintMismatch` — and a triangle-set disagreement — into a counter and returns `None`; the
> caller keeps its own triangulation and the run ends with one warning line, so once J1 has failed
> two owners continue with different triangulations of one face; (2) the fingerprint is the set of
> crossing component ids plus a crease flag, not the sorted list of constraint entity ids this
> clause names, so two owners that disagree about the *same* component's trace share a fingerprint.
> The zero conflicts measured on the acceptance suite prove the check never fired, not that its
> error handling is correct. The clause above stands as the requirement.

> **Winding and diagnostics (as built).** The cache stores each face's triangles in the face's
> own frame — wound with the normal of its key-sorted corners, each triangle rotated to its
> smallest node — and a consuming cell re-winds them outward by the side of its opposite vertex;
> both are functions of the face and the cell alone (`orient_face_outward`). Both decisions use
> floating-point sign tests, not the exact predicates of §7.4. The cache is consulted only from
> the escalated-cell face loop, and of that loop's four exits two *adopt* the cached triangles
> (the crease-hub fan and the ordinary match) while two only *verify* (the doubly-cut band rule
> and the on-curve walk-node hub, whose triangulations are pure functions of the face anyway).
> Faces between two §6-table cells never enter the cache — §5.2 is the pure function there. The
> gated pass has a second mechanism: augmented faces are triangulated **once per face** by the 2-D
> CDT and handed to every owner through `cell_boundary`, with no fingerprint. `[FACE-CACHE]`
> reports faces cached / reused / conflicts per run; a non-zero conflict count is a J1
> violation. Measured 2026-09-23 on A-3: default path 3,450 cached / 1,294 reused / 0
> conflicts; gated 88 / 16 / 0.

> **Invariant J2.** The generic constrained face triangulator MUST reproduce
> §5.2's tables exactly on the inputs those tables cover. There is one shared
> routine; the kirigami tables are its closed-form values, not a parallel
> implementation. (Test obligation §13, row T-J2.)
>
> *As built (rev 1.6).* There are three face triangulators, not one, and the table is the
> production implementation rather than a closed form of another: `cut::face_split` (§5.2 as an
> explicit table) is what every path uses for an expressible face; `facecache::triangulate_face`
> (chord split + smallest-key fan, or hub fan) reproduces all five §5.2 rows
> (`j2_reproduces_the_frozen_face_split_table`, §14 [11]) but has **no production caller**; the
> gated pass's 2-D CDT (`cdt::constrained_face_triangulation`) is compared against §5.2 per face
> at run time and replaces it only where they differ, so J2 is enforced for it by construction
> rather than by a table test. `junction::face_mesh` wraps the table and adds the crossed-face
> and loop-fan cases of §7.7 B2.

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

> **As built (rev 1.6, D-24 to D-27).** *Bullet 1:* two kernels exist. The incremental kernel —
> `delaunay_tets` in ascending `NodeKey` order with a local super-tet, then boundary and facet
> recovery by shaving, flips and edge removal (`constrained_tets`) — runs only on the gated path
> (`RUSTMSPT_PLC_PASS`). On the default path an escalated cell is first offered **successive
> convex clipping** by the supporting planes of its clipped surface fragment
> (`subdivide_cell_by_fragment`), which interns no node and refuses otherwise, and whose pieces
> are fanned from their own centroid (§7.7 B4); the module doc of `cdt.rs` says so: "what is
> normative there is the properties, not the algorithm". *Bullet 2:* exact predicates decide the
> Delaunay insertion (`orient3d_filtered`, `insphere`) and every fan tet's orientation
> (`orient3d`); the clipping kernel and the §7.6 cap and conformity guards decide sides by float
> distances with relative tolerances — clip side `tol` (`1e-9` absolute in the normalized frame
> on the default path), `ON_FACE_FRAC 1e-9`, `ON_EDGE_FRAC 1e-6`, `PLANE_FRAC 1e-5`,
> `INSIDE_FRAC 1e-9`, the fragment area threshold `edge · 1e-6` and the fragment kernel's key
> quantum `volume_tolerance · 1e-6` (numerics §1.3 lists them). Cocircular ties fall to the
> earliest-inserted (smallest-key) configuration; the 3-D kernel has no explicit key tie-break
> beyond insertion order. *Bullet 3:* interior Steiner points (piece and slab centroids) lie off
> the constraints and are exact averages of the piece's distinct nodes in node-id order. **Face-level
> Steiner points lie on constraints by design**: where two chords cross a face the node is their
> closest-approach midpoint (on both surfaces); where a locked curve pierces a face the node is the
> pierce point (on the curve); the crease hub is the same pierce point — fixed-sequence float
> constructions over canonically ordered inputs, reproducible bit for bit but not averages, and
> each is registered as lying on every surface that meets there. Every Steiner node is keyed on
> insertion (face-level nodes on `1e-6·ε`, centroids on `ε`), identified by its face (one per face)
> rather than snapped by proximity to an existing node; on the gated path arena nodes are welded
> by ordering-quantum key only. *Bullet 4:* the **interior** mesher inserts no point on a shared
> face — at HEAD the kernel inserts no Steiner point at all: recovery is flips and edge removal
> only, and the only insertions in `cdt.rs` are the local super-tet (dropped) and the facet-split
> fan's per-piece centre. A face's own triangulation MAY introduce one node (its centroid, the
> meeting point, the pierce point), computed from the face alone and interned once per face
> (`face_steiner`, keyed by the face's key-sorted corners) so both incident cells share it — J1 by
> sharing, not by exclusion. Plan M-2.1's facet-Steiner remedy (points *on a facet interior,
> strictly inside the cell*) is not built; when it is measured, rev 1.7 amends bullets 3–4 for it
> (plan S-2, M-0.2). Plan M-5.2's bounded quality refinement of the face cache is J1-legal under
> the same reading: bullet 4 binds the interior mesher, and a face-level refinement is a pure
> function of the face.

### 7.5 Region assignment

Seed-based: each local sub-region (a connected component of the cell's tets not
separated by a constraint face) is classified **once** at an interior sample by
the 5-ray `robust_inside` test, and the result is written to every tet of that
sub-region. Accumulated per-cut votes are *not* used inside junction cells —
they are order-dependent exactly where several patches meet, which is the failure
mode the junction path exists to remove.

> **As built (rev 1.6, D-27).** On the gated *Meshed* arm the sub-region is
> `regions_by_constraint` (a flood fill over faces not lying in a facet's plane inside its
> outline) and one sample serves the region. On **every fan arm** — the whole-cell fan, §7.6's
> pieces, band slabs and band-template tets — each fan tet is sampled at its **own centroid**
> (`seed_record`); a piece the split could not separate therefore carries a material boundary
> along its fan faces (the mixed-label piece of plan A-§6.x / P-3.10). Seeding per piece was
> built, measured much worse (A-7a volume error 0.40 % → 4.17 %; a fan tet's centroid is biased
> toward its base, which is the face the neighbour sees, so per-tet seeding is the better
> cross-cell agreement heuristic) and reverted. The seed sample is asked about **every** solid
> component, not only the ones S6 left `Ambiguous`: the parent's `Inside` entries are kept,
> `Outside` entries are dropped, and a component S6 recorded outside or omitted is recovered
> when the sample lies inside it (a piece can sit inside a body the record called
> definitely-outside). On the §6 path, table children likewise re-decide omitted components with
> an `on_cut` vertex by a centroid sample. An *uncertain* predicate answer is not a side: a node
> on the exact contact plane of two bodies has none, and taking the guess is what produced
> triangles with corners on both sides and no cut node between them.

### 7.6 Fallback (gate G6-0)

If G6-0 finds the local CDT untenable: sequential kirigami with **curve-node
pinning** — cuts are constrained to pass through the snapped curve nodes shared
by both surfaces. The result is a valid conforming mesh whose junction geometry
may chamfer by at most one cell (`≤ h`), logged per cell and reported in the
accuracy contract. This is the degraded mode, not the design.
> **As built (rev 1.6).** Neither this fallback nor a `[JCT-FALLBACK]` chamfer log per cell was
> built. What ships is described in §7.7 and recorded as deviation **D-17**: the sequential split of
> the cell by each surface with a **centroid fan** per piece on the default path, and on the gated
> path the §7.4 kernel first, then the facet-split fan, then a whole-cell centroid fan. The centroid
> fan violates `[R1]` by construction — its material boundary is wherever the spokes cut — and it is
> the arm that carries essentially all remaining P3 damage (plan §2.4). It is a deviation to be
> **removed** (plan M-2.3), not an accepted fallback; the plan's M-0.2 rewrites this section once
> M-2 has decided what the only fallback is.

> **Reporting (as built).** Fallback counts are reported per run (`[JCT-CUT]`,
> `[JCT-FALLBACK]`, `[JCT-SEED]`, `[JCT-CELL]`); per-cell decline reasons print only under
> `RUSTMSPT_JCT_DIAG`, and the `parent_cell` / `plc_path` / `escalation_reason` cell arrays are
> written only under `RUSTMSPT_CUT_DIAG` (contracts §2.5); `node_origin` is always emitted.


### 7.7 As built — the S8 escalated-cell path at `891badc` (rev 1.6, informative)

This section describes what ships, in the order it runs, so that every deviation named in §15
has a location. It is informative: the normative text is §7.1–§7.6 with their as-built notes,
and plan M-2 / M-3 change what is described here. Line references are to `891badc`, in
`src/meshgen/cut.rs` unless stated.

**A. Where an escalated cell comes from** (`cut_one_cell`, in order of evaluation). (1) Any edge
carrying a K1 second crossing for any component → `MultiCrossing`. (2) The set of components
with a first crossing on any of the six edges: more than one, and not a welded pair (both
components crossing only collapsed or contact edges) → `Junction`. (3) By the ownership record
`(ambiguous solids, crossing sheets)`: `(0, 1)` → the welded-sheet cut; `(0, 0)` → `Inconsistent`
iff any edge is cut; `(1, 0)` → proceed; `(2, 0)` → proceed only as a welded pair; anything else →
`Junction`. (4) Driving component = the smallest ambiguous solid; a crossing by another component,
or by a non-solid → `Inconsistent`. (5) K1 multi-crossing on the driving component →
`MultiCrossing`. (6) Per edge, S6 straddle XOR S7 crossing → `Inconsistent`. (7) The §6 table;
no legal row → rows I / i (§6) or `Inconsistent`. (8) The table-vs-interior check → `Junction`.
(9) The guarded dry-run → `DryRun`. `Quality` is never produced. Curve pierces never trigger
escalation.

**B. The default path, per escalated cell** (the assembly loop is serial in ascending cell index
— R-P2). *B1.* `face_states` (§5.4) builds each face's §5.2 state when expressible, its boundary
walk, and a `CrossedFace` when not. *B2, per face, in order:* (i) the doubly-cut band rule
(§8.3) first — if it fires, its triangles are taken; (ii) the **crease hub**: an expressible face
pierced by a locked curve, every owner of which escalated, and every component of whose piercing
curve leaves a `cut_index`/`second_index` node on one of the face's three edges (`on_cut` does
not count — admitting it produced six `[V9]` curve-node failures and a fourfold rise in
misattributed slivers) is fanned from the pierce point, interned once per face in `face_steiner`
and registered as on-surface for every component; (iii) otherwise `face_mesh`: `Table` = §5.2;
`Crossed` = two chords kept as edges, with a per-face meeting node interned at the curve pierce
point when the face is pierced, else at the chords' closest-approach midpoint, registered for
every component when on a curve and for the two chord components otherwise, or with no new node
where the two chords share a cut node (a *hub*); a chord whose endpoints are adjacent on the walk
lies along a walk edge and is refused (it would leave a collinear run any fan turns into
zero-area triangles); `LoopFan` = if the face is not pierced and exactly one walk node lies on a
locked curve, fan from that node with no new node; otherwise fan from a face Steiner node
interned per face at the pierce point or, failing that, the face centroid; (iv)
`check_face_cache` (§7.3 as-built note). *B3.* The band plan (§8.3): a cell three of whose faces
are doubly cut at one corner is split into three slabs; declines are counted. *B4.* If no band
plan, `split_escalated_cell`: (a) each component's *crossing set* = its cut nodes on the six
edges + the meeting nodes it owns + tet vertices `on_cut` for it; none → decline; (b) the parent
volume from a fan over the boundary (`fan_volume`; the exact `soup_volume` where the soup
orients); (c) the **fragment kernel** — `fragment_in_cell` per component (triangles whose
intersection with the tet has positive area, `edge · 1e-6`), then successive convex clipping by
the fragment's canonicalised, quantised supporting planes in a local node arena; the cell is
**refused** whenever a clip would intern a node the boundary does not already have ("a
NEAR-DUPLICATE of a node the cell already has", "the plane OVER-CUT", "the clip wanted a node
on a shared EDGE / inside a shared FACE / strictly inside the cell", …), when the planes do not
separate the cell, when a piece face outside every plane is not an original boundary triangle
("the cut re-split a shared boundary triangle"), or when a piece has fewer than four faces;
accepted pieces (their fan volumes summing to the parent's within `CUT_VOLUME_TOLERANCE`) have
their faces in a constraint plane tagged as caps for the component; (d) `RUSTMSPT_CDT` (ablation
handle): the older `subdivide_cell` by planes through cut nodes; (e) the **sequential soup
split** — for each crossing component in ascending id, for each current piece,
`split_soup_by_surface` with a side oracle that answers `None` for nodes in the component's
crossing set and for an uncertain predicate, and `PointClassifier::inside` otherwise; a triangle
with corners on both sides is classified by its centroid (`RUSTMSPT_NO_MIXED_BY_CENTRE` is the
ablation); declines: "triangle wholly on the surface", "one side empty (`a` in / `b` out of `n`)"
— of which `0 in / N out` is the *correct* decline of a surface that touches the cell's boundary
and never enters it —, "inside/outside holes are not simple loops", "the two sides' holes
disagree"; each cap loop is split along the chord between its two meeting nodes of that component
when it has exactly two and at least four nodes (the as-built curve pinning), a 3-node loop is one
triangle, and otherwise `fan_cap` tries the loop's own vertices in key order and keeps the first
fan that is simple (non-degenerate triangles; **no** orientation-consistency test — a cap is a
piece of a curved surface and may bend through a right angle inside one cell), swallows no loop
vertex on an edge, and puts no triangle in a parent face's plane (`cap_lies_in_parent_face` —
such a cap is a `multi_shared_face` the exact-triangle guard cannot see); a component any of
whose cap triangles the piece already carries does not cut it ("coincides with a cut already
made" — `any`, not `all`: a partly coincident surface contributes one new triangle beside two it
shares); both sides are closed with the caps not already held and separated into edge-connected
parts; (f) guards: fewer than two pieces; flap peeling — a cap triangle pulled into the wrong
piece is one whose removal leaves that piece closed, and it is handed to the open piece it
completes; "a piece is not closed" (odd edges per piece); "a piece does not fan without a flat
tet" (`fan_is_sound`, exact); "a cap lands on the cell's own boundary"; "the pieces' fans are not
conforming among themselves" (`cell_fan_is_conforming`: `[V3]`'s two questions asked of the cell
before it commits — every boundary triangle owned by exactly one piece, no interior face with
more than two tets, no degenerate fan face, no node inside a fan face it is not a vertex of); the
volume guard `|Σ pieces − parent| ≤ max(volume_tolerance, 1e-12) · parent`, with `soup_volume`
(exact for any closed soup by a breadth-first winding walk) and `fan_volume` (exact only for a
star-shaped piece) as its fallback. *B5.* A successful split pushes its caps to the pending
interfaces and its pieces as slabs (`[JCT-CUT]`); a decline pushes the whole boundary as one slab
(`[JCT-FALLBACK]`). Each slab is fanned by `fan_cell` from a fresh apex at the mean of its
distinct nodes (a band plan's gap slab goes to `mesh_band_cell` first, §8), every tet
`orient_positively`-ed, exactly-degenerate ones dropped and counted, and each tet given
`seed_record` (§7.5 as-built note). *B6.* After the loop: `derive_interface` (one interface cell
per face by node **set** — a face's nodes are ordered by quantised key and two distinct nodes
can share a key, so comparing arrays emitted duplicates; a cap both of whose sides seed to one
region is dropped rather than recorded one-sided), `declare_labelled_boundary` (every face
whose two owners resolve to keys differing by one component is tagged for it; a two-component
step is excused only within a fan's chamfer of an S2-declared coincidence,
`contact_chamfered_by`, plan M-2.4), `declare_contact_components` (a face all of whose corners
lie on some patch of a contact region, with the component sets agreeing), `curve_mesh_edges`
(§7.7 D), then the census warnings (`[CAP-LOOP]`, `[FACE-CACHE]`, `[CREASE-FACE]`, `[JCT-CELL]`,
`[CUT-3EDGE]`, `[CUT-CASE]`, `[CUT-GUARD]`).

**C. The gated path** (`RUSTMSPT_PLC_PASS`; plan M-3 makes it the path). *C1.* Per lattice
face, once: `trace_on_face` clips every classified component's triangles to the face plane
(tolerance `shortest_edge · 1e-9`); a trace endpoint on an edge is interned once **per edge**, one
in the face interior once **per face**, published against the nearest edge and welded at `[V2]`'s
bound (a private `1e-9` relative bound at that site was the seed of A-8's 113,859 hanging nodes;
using the contract's own bound closed it). *C2.* A face with extra points or chords is
triangulated by `constrained_face_triangulation` (2-D CDT, insertion in key order, Anglada
flips, no Steiner point, constraints through a vertex split into two); the result is stored only
where it **differs** from §5.2 (that is what "traced" means), and a face the CDT refuses joins
`failed_faces`. *C3.* An **exclusion** set — cells that cannot build a boundary, own a failed
face, or sit across a curve-pierced face from an escalating cell with no augmented face — spreads
across augmented faces to a fixed point (≤ 64 rounds); spreading it the other way was measured to
recruit more cells into the T-junction set and was reverted. *C4.* `plc_attempt` per
non-excluded cell, in parallel: a local arena on the ordering quantum; facets from
`fragment_facets_in_cell` with each rim vertex matched to the boundary node on the same face
plane within `ε` (the rim comes from the faces — "the cut derived twice" is the one defect
behind four symptoms, plan Appendix A §6.43) or interned; `constrained_tets` = `delaunay_tets`, split facet
edges through a node, `recover_boundary` (shave + `remove_edge`), `recover_facet_edges`,
`recover_facet_interior`, `improve_dihedral` (24 rounds, kept only if the hull is unchanged),
`remove_thin_tets` (volume `≤ longest² · tol`, protected boundary and facet edges); refusals, each
a named work item (plan §2.4): "the points have no tetrahedralisation", "a facet has fewer than
three vertices / no area", "a tet is thinner than the node quantum", "the boundary is split
differently, but on the same nodes", "the boundary uses a node that is not on the hull", "the
hull carries a node the boundary has never heard of", "a facet edge is not an edge of the
tetrahedralisation", "a facet's edges are all there but its interior is not covered", "neither
the boundary nor the facets survive", "a boundary node is off the hull, and the facets fail
too". On `Ok`: `regions_by_constraint`. On `Err`: the **facet-split fan** — cap triangles per
component from the facets, excluding any lying in a cell face (`on_cell_face`; removing this
filter breaks `[V3]`); `conform_cap_rim` (a cap's once-used edges split at boundary nodes lying on
them, bounded rounds, no new node); identical-cap merge ("one surface, cut once"); a sequential
split by the side oracle with **cap clipping by cut history** (a cap triangle enters a piece iff
for every earlier cut it is on-cut or its centroid is on the piece's side); pieces deduplicated;
each piece first tried through `constrained_tets` with no facets, else fanned from the mean of its
nodes (the only point insertion in the kernel); partition test `|Σ − whole| ≤ max · 1e-9`. A hard
`Err` yields a whole-cell fan over the same augmented boundary. *C5.* Plans are consumed first
in the assembly: `Meshed` = one `seed_record` per region; `Fan` = the whole-cell centroid fan with
per-tet seeding. *C6.* A post-assembly **T-junction detector and repair** (only on this path): the
detector is `[V3]`'s own test (a node within `1e-9 · diag` of a face it is not a vertex of); up to
12 rounds, stopping on a repeated state; the repair replaces the flat sliver `F + {q}` **and** its
solid neighbour `F + {d}` by the three tets `(a,b,q,d), (b,c,q,d), (c,a,q,d)` — replacing only one
side hands triangles a third owner — refusing on a face rim, a tagged face, or a shape clash;
plan M-4.2 retires it when its count is zero.

**D. Curve carriage.** A mesh edge is declared on a locked curve iff its two endpoints **and its
midpoint** each lie within `ε` of the curve's polyline (endpoints alone accept a chord that
leaves the curve and comes back — the shape a chamfer takes at a corner); candidates are edges
both of whose endpoints are in `nodes_on_curve`; an edge on two curves is declared once, under
the lower curve index (a duplicate cell is a `[V1]` failure), so a node there is checked against
one body. Reported as `[S8/G6-4] curve table: … carried …`; carriage is INFO in `[V9]` until plan
M-2 lands (contracts §4).

**E. Handles that change what is meshed** (R3; plan M-3.2 deletes them):

| handle | effect |
|---|---|
| `RUSTMSPT_PLC_PASS` | enables all of C: the rev 1.5 offer, face augmentation, the §7.4 kernel, the facet-split fan, `plc-arena` nodes, the T-junction repair; unset, none of it runs and the kernel is reached only by the print-only `RUSTMSPT_PLC_DIAG` probe |
| `RUSTMSPT_CDT` | after the fragment kernel refuses, tries `subdivide_cell` by planes through cut nodes (measured worse; the route is deleted with the handle) |
| `RUSTMSPT_NO_JCT_CUT` | `split_escalated_cell` declines before any kernel runs — every escalated non-band cell becomes a whole-cell fan |
| `RUSTMSPT_NO_K1_SECOND_CUT` | the K1 second crossing is not interned |
| `RUSTMSPT_NO_MIXED_BY_CENTRE` | a triangle with corners on both sides is refused instead of classified by its centroid |

Print- or dump-only (they change what is printed, never what is meshed): `RUSTMSPT_JCT_DIAG`,
`RUSTMSPT_PLC_DIAG`, `RUSTMSPT_PLC_DUMP`, `RUSTMSPT_PLC_CSV`, `RUSTMSPT_CUT_DIAG` (adds the
`parent_cell`, `plc_path`, `escalation_reason` arrays and runs `diagnose_conformity`),
`RUSTMSPT_SPOKE_PROBE`, `RUSTMSPT_FACE_TRACE`, `RUSTMSPT_TRACE_CELL`, `RUSTMSPT_CUT_CELL`,
`RUSTMSPT_S67_DIAG`, `RUSTMSPT_THIN_DIAG`, `RUSTMSPT_SNAP_DIAG`, `RUSTMSPT_V13_DIAG`,
`RUSTMSPT_TIME_STAGES`.

**F. Charged to the arm (plan §2.4, 2026-09-01).** §6's table and the §7.4 kernel are exact to
0.2 % of their own interface area; the whole-cell fan carries 91.7 % (A-3), 94.6 % (A-6a) and
98.4 % (A-8) of each case's off-surface area from 332 / 75 / 3,281 cells. That is the number
behind D-17's "not accepted".

---

## 8. Band templates — the k-cases (S8b)

### 8.1 Setup

Matched wall-triangle pairs (the pairing map Φ from S3) span prism cells: cap
`(a0,a1,a2)` on wall A, cap `(b0,b1,b2)` on wall B, `a_i ↔ b_i`. A pair whose
local separation is below `t_sheet(x)` is **collapsed per vertex pair** to a
single node `r_i`. `k` = number of collapsed pairs in the cell.

> **As built (rev 1.6, D-32).** The prism cell is a **lattice** construction, not a Φ-spanned
> one. A band cell is a lattice tet three of whose faces, meeting at one parent vertex, are each
> crossed by both walls of one gap (two cut nodes per crossed edge) and whose fourth face is
> uncut; it is split into three closed slabs (near side, gap, far side) by the doubly-cut face
> rule of §8.3, and the gap slab is this section's prism: cap A = the three near-wall cut nodes,
> cap B = the three far-wall cut nodes, `a_i ↔ b_i` along the lattice edge that carries them. S3's
> pairing map Φ does not reach S8b; S3 reaches it as a per-arranged-face table (`ThinContext`):
> the owning thin region of each wall face (both walls of a gap map to one region, converted
> regions winning ties), each region's *effective* regime after the §8.4 ladder, its pair class,
> the converged scalar `t_sheet` and the `collapse_sheets` flag; a cut node inherits its region
> from the arranged face its crossing was found on, and a band cell's `band_region` is the
> smallest region id among its six wall nodes. **Collapse is per lattice edge**, not per S3
> vertex pair, and is gated four ways: `meshgen.thin.collapse_sheets` (default `false`; an R3
> open item, plan M-4.3), the edge crossed by exactly two components, both crossings resolving to
> one region whose effective regime is `Sheet`, and the chord between the two crossing points
> along the edge at most the scalar `t_sheet`; the rim node is the midpoint of the two crossings
> and both `(edge, component)` keys map to it (`collapsed_edges`). Collapse is a property of the
> edge — a pure function of its two crossings — so every cell sharing it sees the same rim node:
> that is Invariant B1 as built. A vertex pair is never collapsed *inside* a band cell — the gap
> slab's pairs are always open — so the pipeline never produces a mixed-`k` cell: a cell with no
> collapsed edge takes `k = 0`; a cell all of whose crossed edges are collapsed is the `k = 3` case
> and is cut by §6's table as a welded pair; a cell with some collapsed and some open edges
> escalates to §7.6 and is fanned (its faces carry three cut nodes where §8.3 needs four). Rows
> `k = 1` and `k = 2` are frozen and unit-tested (`frozen_table_emits_the_frozen_tet_counts`) and
> unexercised by any pipeline mesh.

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

> **As built (rev 1.6, D-32).** Each band tet's ownership is settled individually by §7.5's
> interior sample (`seed_record`): the parent's `Inside` entries are inherited and every
> `Ambiguous` or unlisted solid is decided at the tet's centroid; band tets carry
> `provenance = 3` (junction); the reserved `Provenance::Band = 4` and `from_cut_p` are never
> written. The gap slab's two lids are declared interface faces for the component whose
> crossings they are built from. Band tets are declared by the cell arrays `regime` (`1` = a
> §8.2 row, `2` = the Steiner fallback) and `band_region` (the S3 region id, `−1` when
> unattributed); `[V7]`'s one-layer clause reads them. Every emitted row is additionally checked
> against the volume enclosed by the cell's own boundary triangulation (relative error ≤ 1 %,
> `BAND_VOLUME_TOLERANCE`), and a cell two of whose pairs share a wall-A node is refused as
> degenerate. A collapsed edge carries one cut node for both components; a face over it is a
> single-patch §5.2 face, and the sheet's rim (`CurveKind = 1`) is the set of collapsed nodes in
> cells that also carry an open crossing — derived from the collapse decision, so `[V8]`'s
> pinhole check is not vacuous.
>
> **Reachability (measured 2026-09-23, `891badc`, `run_acceptance.py`'s settings, `RUSTMSPT_THIN_DIAG=1
> RUSTMSPT_CUT_DIAG=1 rustmspt mesh --config <case>.yaml`).** The band construction claims 98
> cells on A-3 (prism 54, Steiner 44 → 514 tets; worst band dihedral 8.522°) — a lens where two
> *different* solids cross the same lattice edges, not a thin feature — and **0** cells on A-6a,
> A-6b, A-7a and A-7b: A-7a's 312 offered cells decline `FaceShape` because their faces carry two
> cut nodes, not the four §8.3 needs (S3 there: 19 regions, 1 sheet, 18 skipped; ladder
> `RefineLocally: 1`, not honoured); A-6a declines 809 `FaceShape` + 161 `UncutFaces` (ladder
> `Band: 1`). The `k = 3` path cannot fire with `collapse_sheets` off. Plan M-4.3 and M-4.7 own the
> fixtures.

### 8.3 The doubly-cut face rule and the three-slab split (as built, G7-1, rev 1.6)

A lattice face carrying two cut nodes on each of two edges meeting at parent vertex `c` is
triangulated as the corner triangle `(c, n1, n2)`, the strip quad `(n1, f1, f2, n2)` and the
remainder quad `(f1, p, q, f2)`, the two quads on Rule SNK; it is a pure function of the face and
is applied **per face**, in every escalated cell, before any cell-level decision — a face two
walls cross is shared by two cells and only one may be a sandwich, so running the rule per cell
made them disagree and put hanging nodes exactly where the gap is. A cell is a band cell when
three of its faces take this rule at one common corner and the fourth is uncut; it is split into
NearSide / Band / FarSide slabs, each closed by reversing its unmatched directed edges (3- or
4-cycles, SNK for quads). Declines are counted as `FaceShape` / `UncutFaces` / `Corner` /
`Unclosable` and reported under `[S8b/G7-1]` (`RUSTMSPT_THIN_DIAG` prints each decline's face
signature). A band cell's conformity comes from the **pair**: a pair collapsed in one cell is
collapsed in every cell that references it, and the one thing that is not a pure function of a
quad — the §4.4 diagonal flip — is proposed for both incident cells at once in
`mesh_band_layer` and accepted only if both still mesh, never per cell (D-15).

### 8.4 Regime decision — the FEM-aware ladder (as built, rev 1.6)

Before S8b, each unskipped thin region is placed on one rung from the predicted quality of the
nominal §8.2 cell (the right-isoceles cap of legs `h` extruded by `t_r`, measured with
`tet_quality` over all six §4.3 patterns): **Band** if min dihedral ≥ `band_min_dihedral_deg`
(8°), `AR ≤ 20` and, under an explicit FEM profile, altitude ≥ `0.05·h`; else **RefineLocally** if
`h/2 ≥ h_min` would pass — reported only, not honoured ("re-running S4 at a finer `h` is not
built"); else **Sheet** if `t_r ≤ t_sheet`; else **Reject** (the run aborts, `InvalidMesh`) when
`forbid_volumetric_fallback`; else **Volumetric**. The outcome overwrites the region's effective
regime; it is logged as `[S8b/G7-1] FEM-aware ladder over N thin region(s): {…}` (ARB-20's
`[THIN-LADDER]` tag does not exist). The regional > 5 % Steiner demotion is a library rule
(`mesh_band_layer`) with no pipeline caller (§4.4 as-built note).

---

## 9. `resolve()` — the label truth table (S6 preliminary; final in S8's `cut_to_doc` until plan M-6.1 lands S10)

### 9.1 Definition

Given a tet's sparse ownership record (entries `(X, side)` with
`side ∈ {outside, inside, ambiguous}`, one `provenance ∈ {lattice, cut, arbitrated, junction,
band}` per record, **absent ≡ outside** — an explicit `outside` entry is a diagnostic
distinction from "never asked" and resolves identically to absence):

```
S      = { X : side_of(X) = inside }
Y_min  = min { Y(X) : X ∈ S }
resolve(record) = { 0 }                                   if S = ∅
                = { X ∈ S : Y(X) = Y_min }                otherwise
```

Only `Solid` components may enter `S`; `Sheet` components never claim volume
regardless of any winding-number evidence (the hard guard of plan §10.4). A
region key is the sorted X-list; every X in a key shares one Y.

> **As built (rev 1.6).** `resolve(record, priority_of)` takes the priority table as an
> argument; an inside X absent from that table is dropped from `S` rather than reported, and
> duplicate entries for one X are collapsed — in the pipeline the table is built from every
> arranged component, so the drop is unreachable. "Absent ≡ outside" is the *reading* rule of
> `resolve()`: before resolution S8 adds `inside` entries a record omitted — every component for a
> junction piece (§7.5), `on_cut`-scoped components for a table child — by an interior sample, and
> the record is final only after those passes. Of the provenance vocabulary only `lattice`, `cut`
> and `junction` are produced: band pieces are seeded by §7.5 and carry `junction`; `arbitrated`
> is reserved for ARB-23 and never written (D-33). The sheet guard of row 10 lives upstream of
> `resolve()`: the point classifier builds no slot for a `Sheet`, S6 seeds records from classified
> solids only, and `seed_record` skips a component without a slot.

### 9.2 Truth table

| # | `S` (with priorities) | `resolve` | Governing requirement |
|---|---|---|---|
| 1 | `∅` | `{0}` | R-A1 (background) |
| 2 | `{A(Y=1)}` | `{A}` | single ownership |
| 3 | `{A(1), B(2)}` | `{A}` | R-A4 — smaller Y wins, B is replaced in the overlap |
| 4 | `{A(1), A′(1)}` | `{A, A′}` | R-A3 — same-priority overlap keeps every X |
| 5 | `{A(1), A′(1), B(2)}` | `{A, A′}` | R-A3 + R-A4 together |
| 6 | `{V(0), P(1)}` | `{V}` | void inside particle (V mapped to a void material at export — the export mapping is S11's, plan M-6.2, not built) |
| 7 | `{A(2), B(3), C(3)}` | `{A}` | unique minimum priority |
| 8 | `{A(3), B(3), C(2)}` | `{C}` | minimum need not be the first-listed |
| 9 | `{A(1), B(1), C(2), D(2)}` | `{A, B}` | ties at the minimum only |
| 10 | any `S` containing a `Sheet` X | `Sheet` X is never in `S` | plan §10.4 hard guard |
| 11 | record with an `ambiguous` entry | **error** | records must be complete before S10 (§12, ARB-23) |

> **Row 11 as built (rev 1.6, D-33).** Nothing errors on an `ambiguous` entry at labelling:
> `resolve()` filters on `inside`, so the entry is **dropped** and the record resolves from its
> definite entries (`{0}` if none); the unit test
> `an_outside_or_ambiguous_entry_never_claims_the_tet` pins the drop, not an error. At S6 the
> drop is by design (the preliminary key: "the cut will decide"). At S8, an *uncut* cell whose S6
> record is `ambiguous` keeps that entry through `cut_record(case = 0)` and is labelled from its
> definite entries in `cut_to_doc`; escalated pieces and table children are settled by §7.5's
> sample first. ARB-23's pre-derivation sweep, its `[OWN-FIX]` tag and the `arbitrated`
> provenance do not exist. The row stays normative; plan M-6.1 decides whether S10 gets the
> sweep.

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

*As built (rev 1.6):* the definition above is realised by S8's `cut_to_doc` (the union of the
resolved keys of incident tets, `0` included, plus the component of every incident tagged
face, interned into the node-ID-set table in first-seen order). Snapshots before S8
(`s06_classified`) carry a placeholder: every node indexes a single set `{0}`.

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
> and the box-clip curve. *Rev 1.6:* G2-5 has landed — `clip_arranged_to_box` and `rebuild_topology` run before
> `s02_arranged` is emitted (pipeline S2-clip). What remains open of G2-5's half of the row is
> the sheet box-clip curve and the box tag's gating, recorded below (D-41).

Rows C1, C2, C3, C7–C9 are the ones that make a region key multi-valued and are
therefore exactly the rows that trigger the INP material-mapping requirement
(plan Appendix B.7): an unmapped multi-ID key is a hard export error.

> **As built (rev 1.6, audited 2026-09-23 at `891badc`).** The policy machinery is as written:
> `CoincidencePolicy { Merge (default), Reject, Warn }` (`config/meshgen.rs`), the reject set is
> exactly C1/C2/C3/C7/C8/C9 (`arrange.rs` `rejected_by`), `warn` prints one aggregate line per
> case in `BTreeMap` order (`pipeline/meshgen.rs`), C7 is the mutual one-to-one match under
> `distance² ≤ ε²` with complete-link clusters and the `(TriId, NodeKey, node)` representative,
> and all five degraded-neighbourhood reasons are recorded and read by S7. The 48 arrangement
> tests pass. Seven clauses of this section do **not** hold as built, none of them a change to
> the reject column:
>
> - **C1/C2 against C3 (D-40).** `promote_patch_events` classifies a coincident patch as C3
>   whenever any of its edges borders a face carried by only one of the pair — coplanar **or
>   not** — or its per-tag orientation is inconsistent across the patch; C1/C2 are reported only
>   for a coincident patch whose whole boundary is shared or free. Measured: a closed cube plus a
>   sheet identical to one of its faces reports `C3`, two identical isolated sheets report `C1`.
>   By the same rule two solids sharing one identical face (face-to-face contact, opposite
>   orientation) would report C3 rather than C2 — inferred, not run. C3's overlay-boundary
>   curves are then emitted along the patch rim.
> - **C5 against C6 (D-40).** The split is by **coplanarity**, not by shared-vertex against
>   tangency: C5 is a single-point contact of a *coplanar* pair, C6 of a *non-coplanar* pair
>   (shared vertex or tangency alike). Neither row is restricted to distinct patches: a
>   same-component pair is skipped only when it shares a conforming *edge*, so a body's own
>   vertex adjacency fires C5/C6 with a point feature each (an open box alone: 18 `C6 [1]`
>   events; two cubes end to end: `WARN C6 occurrences=112`, mostly single-component). Those
>   point features are what make `ArrangedSurface::corner_nodes` larger than the corner set.
> - **Self-coincidence (D-40).** Rows C1–C3 and C7 also apply to a component's coincidence with
>   itself: the event carries the single component `[X]` and is rejected like any other ✗ row.
> - **C7 declined by complete-link (D-43).** The C7 event is pushed at match time; when the
>   cluster step later declines the merge, the event stays (so `reject` still refuses the pair),
>   the pair remains two faces, and the decline is recorded as a `QuantizedOrderAmbiguity`
>   degraded neighbourhood.
> - **C10 (D-41).** A box-clip curve is built only from **solid** cap loops (`kind == 0`); a
>   C10 sheet gets `box_tagged = true` and no curve (measured: a sheet square on the `z = 0`
>   domain face — 2 C10 events, 2 box-tagged faces, curves `[(Rim, [1], 5)]`, no `Box` curve).
>   The box tag is gated on the face's **primary** tag (`face.component`, the lowest source
>   triangle's), so a sheet merged under a solid's primary tag (C9 ∩ C10) is never box-tagged
>   (measured: a cube with its bottom on `z = 0` plus a sheet identical to that bottom — events
>   `C3`, `C9`, `C10 [2]` ×2, **0** box-tagged faces). "Tagged both `sheet` and `box`" is
>   carried as `FaceTagKind = 2` with the sheet's X kept in the tag set: `FaceTagKind` is
>   single-valued, box wins, and sheet-ness is read from `ComponentKind[X]`.
> - **Invariant M1 past S2 (D-42).** S2 keeps every tag (`merge_atomic_faces`); S2b drops it.
>   `rebuild_topology` selects a component's faces by the primary tag only, and a component whose
>   every face was merged under another component's primary tag gets **no classification**, which
>   S6 defaults to `Sheet` — so its material is lost. Measured end to end (`a2_cube.stl` twice,
>   both priority 0, `coincidence: warn`): `[G2-4] topology rebuild: 1 solid (1 closed, 0
>   defective), 0 sheet`, then `[S6/G5-1] … 1 solid component(s) (1 sheet, 0 defective) … 2
>   region key(s)` — one cube's material carried under one X where R-A3 requires the key
>   `{1, 2}`. The same selection is what makes ARB-4's closure test read another component's
>   faces (D-20). Not accepted; plan M-4.0b now covers both.
> - **The INP paragraph below is unimplemented:** `materials.unmapped` is parsed and hashed into
>   the config hash and read by nothing; S9–S11 return `NotAvailable` (plan M-6.2).
>
> Sources: `cargo test --offline --release --test meshgen_arrange_tests` (48 passed); the
> end-to-end run above from the committed `data/fixtures/meshgen/acceptance/a2_cube.stl`; the
> patch-level probes from a scratch crate against `arrange_surface`, `clip_arranged_to_box` and
> `rebuild_topology` that is **not committed** — plan M-4.0's fixtures A-15/A-16 are where they
> become repeatable (§14 [12]).


---

## 11. S3 ↔ S4 coupling — monotonicity and termination

### 11.1 Frozen update rule

```
h⁽⁰⁾      = min(h_max, c_curv, c_feat, c_lfs)                    # geometry only (as built: h_max — D-34)
h⁽ⁿ⁺¹⁾    = max( h_min, min( h⁽ⁿ⁾, C(R⁽ⁿ⁾) ) )                    # running minimum
R⁽ⁿ⁺¹⁾    = regimes from thresholds  t_sheet = τ_s·h⁽ⁿ⁺¹⁾,  t_layer = τ_l·h⁽ⁿ⁺¹⁾
```

with `0 < τ_s < τ_l ≤ 1`. The `min(h⁽ⁿ⁾, ·)` is **normative**: `h` is a running
minimum and is never allowed to rise, even if a regime change would relax its
constraint. Without it the argument below is false.

> **As built (rev 1.6, D-34).** `h⁽⁰⁾ = h_max` — the S3 bootstrap size — and `R⁽⁰⁾` is
> assigned at `t_sheet = τ_s·h_max`, `t_layer = τ_l·h_max` before any constraint is evaluated;
> the geometry terms first enter at `n = 1` through `C(R⁽⁰⁾)`. So the loop starts from the
> most-converted assignment and only ever loosens it, and the argument below is unchanged because
> `h` still never rises. `0 < τ_s < τ_l` is enforced at parse time; `τ_l ≤ 1` only by the
> post-loop G-8 check (`t_layer > h` → `InvalidConfig`), i.e. after S3 has run.
>
> **`C(R)`, which the rule uses and rev 1.0 never defined.** One scalar for the whole model:
> `C(R) = min( h_max, min_r g_r, min_{r : R_r = Normal, lfs_floor < t_r < ∞} t_r / gap_cells )`,
> where `g_r` is the minimum of the geometry-only sizing field (curvature, feature, corner and
> locked-curve sources) over region `r`'s sample points, `gap_cells` is a config value with
> default **4** and a parse-time floor of 4 (below four no lattice vertex lands inside the gap and
> S8 is never offered a cut — a correctness threshold, not a tuning), and
> `lfs_floor = max(ε, gap_cells·h_min)`. A region S3 declined (`skip` set, ARB-6) enters with
> `t_r = ∞`: it contributes no term to `C(R)`, is `Normal` in every iteration, and cannot
> oscillate or lock — its samples still emit local-feature-size sources into the spatial field
> (except `Speck` / `IntersectionWedge`, suppressed there too), so the field refines around it
> while the model-wide thresholds ignore it. Regions in `Band` or `Sheet` contribute no LFS term
> (their gap is meshed by the §8.2 templates). `C(R)` is a minimum over thin regions only, never
> the global field minimum. The pipeline's constraint closure ignores the loop's `h` argument.

### 11.2 The frozen measurement

> **Rule S3-M.** A region's separation `t_r` used for the *regime decision* is the
> exact closest-pair distance over the region's surface pairs (BVH triangle–
> triangle, vertex–face and edge–edge), computed **once** at the first S3 pass
> and **not** re-measured across coupling iterations.
>
> *As built (rev 1.6, D-34).* `t_r` is the minimum, over the region's member samples, of the
> closest-pair distance `t_exact` where the sweep measured one and of the ray separation `t_raw`
> otherwise; it is computed once in S3 and never re-measured. The sweep is a uniform bucket grid
> (`TriGrid`, within `2·t_layer`), not a BVH — a mechanism choice, not a rule — and evaluates
> edge–edge and vertex–face closest points; it skips triangle pairs sharing an arranged node and
> requires same-component pairs to face each other within 25° (`COS_OPPOSING_INTRA`) and to be
> geodesically far, and contacts are cleared by `reject_geodesic_shortcuts` per component pair. The ray battery and the
> `|∇t| > 0.5` densification supply the pairing map Φ, the field for
> segmentation, and the snapshot visualisation — not the threshold quantity.

Rule S3-M is what makes `t_r` a fixed geometric constant. Re-measuring it would
let both sides of the comparison fall together and destroy monotonicity, because
finer sampling can only *lower* a measured minimum.

> **Rule S3-S.** Across coupling iterations regions may only **split**, never
> merge. A child's `t_r` is a minimum over a subset, hence `≥` its parent's, so
> splitting moves regimes in the same direction as falling thresholds. A merge, if
> ever required, counts as a regime change and is caught by the oscillation guard.
>
> *As built (rev 1.6).* The region set is fixed by the single S3 pass: across coupling
> iterations regions neither split nor merge — only their regime labels change — so the rule
> holds vacuously. Within the S3 pass, before the loop starts, touching fragments of one gap that
> share (component, side, opposite patch, pair class, declared regime) are unioned and the
> union's `t_r` is the minimum over both fragments; that is part of Rule S3-M's once-only
> measurement, not a cross-iteration merge. The same 0.9 / 1.1 factors gate S3's initial
> segmentation at `h_max`: a Sheet (Band) region is seeded only where the frozen separation is
> below `0.9·t_sheet` (`0.9·t_layer`), and a Band region admits only samples whose frozen
> separation exceeds `1.1·t_sheet`, so no region straddles the sheet threshold.

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
- **Oscillation detection**: a region that changes regime in iteration `n` **back to the
  regime it held in iteration `n − 2`** (an A → B → A return; a region that changes once and
  then holds is not oscillating — rev 1.6 wording, D-34). Action: **lock to volumetric, never
  to sheet**, emit WARN. *As built:* a region that moves in the forbidden direction (toward
  `Sheet`, which the argument says cannot happen under a falling `h` and is therefore a symptom
  of hysteresis-boundary noise) is locked immediately **in the regime it just entered**
  (`Tightened`) with a WARN — it is not forced volumetric; moves along the chain
  `Sheet → Band → Normal` never lock. Whether "lock after the first downgrade" meant this is
  an open item (§15).
- **Post-loop assertion (G-8)**: `ε ≪ t_sheet(x) < t_layer(x) ≤ h(x)` for every
  region, plus `ε < 0.5·τ_s·h_min` re-checked on the realised field (the
  parse-time check is on the configured field and is not sufficient). *As built (D-34):* the
  check is on the converged **scalar** `h` — `ε < 0.5·t_sheet`, `t_sheet < t_layer`,
  `t_layer ≤ h` — and failure aborts the run (`InvalidConfig`); because `h ≥ h_min` the
  parse-time `ε < 0.5·τ_s·h_min` implies the realised `ε` clause, so in a pipeline run only the
  `τ_l ≤ 1` branch can fire; `q ≪ ε` is not re-asserted here (numerics §1.2's ordering line
  overstates it).
- Hitting the iteration cap is a WARN with the unstable region list, not a
  failure: the last assignment is used and every unstable region is locked
  volumetric. *As built (D-34):* "unstable" is every region that changed regime **at least
  once** during the run and is not otherwise locked — a region that moved at `n = 1` and held
  for four iterations is reset to `Normal` too; regions that never changed keep their regime.
  Conservative, and recorded as an open item.
- **After the loop (as built).** The coupling runs exactly once per run. Its converged `h` sets
  `t_sheet`/`t_layer` for S8b, and its regimes are the input to the §8.4 ladder, which may demote
  a converted region to volumetric (or move it Band ↔ Sheet) after the loop; that post-loop
  change is not a coupling iteration and does not feed back into `h`. Invariant K1's
  refine-and-retry around S4–S7 adds sizing sources but never re-enters S3 or this loop, so the
  thresholds are fixed for the rest of the run. Log tags: `[S3/S4] coupling: …`, `[S3/S4]
  constraint bound by region …`, `[S3/S4] WARN: … locked (Oscillated | Tightened)`, `[S3/S4]
  WARN: iteration cap reached; regions […] locked volumetric` — ARB-7's `[THIN-LOCK]` does not
  exist.

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
| | | *as built:* stability ratio `ρ < κ` (numerics §4.3; `c = 7` for C1/C3, `25` for C2) → DD recompute, counted in `precision_escalations`, **untagged**; DD floor, a zero denominator, a non-finite result or a quantised-order ambiguity → a typed `DegradedNeighborhood` that S7 resolves by alternating projection, one `[ARR-PREC]` line each. Never the coincidence path | | | |
| ARB-2 | S2 post-corefinement validation | residual crossing / non-manifold patch after staged precision | flag neighbourhood degraded; S7 uses alternating projection as the curve target | `[ARR-RESID]` | – |
| ARB-3 | S2 coincidence policy | any §10 row marked ✗ in `reject` mode | abort with the entity-pair list | `[ARR-COINC]` | – |
| ARB-4 | S2b closure re-check | declared solid still open beyond ε-repair | GWN becomes its inside test; WARN + closure-defect report | `[TOPO-GWN]` | – |
| | | *as built (D-20):* closure is decided from a global edge incidence, so another component's face can close this one's opening; the member set is filtered by the representative component only; incidence > 2 is not itself a defect | | | |
| ARB-5 | S2b GWN evaluation | GPU f32 value inside the margin band around 0.5 | recompute in f64 on CPU; still in band → classify outside + WARN | `[TOPO-GWN-BAND]` | – |
| | | *as built:* **not built** — no GPU GWN; the CPU decision is `\|w\| > 0.5` with no band; `gwn_margin_band` exists and is never called. And ARB-4's other branch: an open declared solid with `\|w\| ≤ 0.5` is reclassified `Sheet` **silently**, with no WARN | | | |
| ARB-6 | S3 pairing battery | confidence < 0.9 | decline conversion; region stays volumetric | `[THIN-SKIP]` | – |
| ARB-7 | S3↔S4 loop | oscillating regime (§11.4: an A → B → A return) | lock region to volumetric | `[S3/S4] WARN: … locked (Oscillated)` (rev 1.6; `[THIN-LOCK]` never existed) | – |
| ARB-7b | S3↔S4 loop | region moved toward `Sheet` (the forbidden direction) | lock in the regime just entered | `[S3/S4] WARN: … locked (Tightened)` | – |
| ARB-7c | S3↔S4 loop | iteration cap reached | lock every region that changed during the run to volumetric | `[S3/S4] WARN: iteration cap reached; …` | – |
| ARB-8 | S6 classification | GPU parity within margin `δ_gpu` | CPU 5-ray exact `robust_inside` | `[CLS-BAND]` | `from_lattice` |
| | | *as built:* no GPU tier; the f64 static filter's uncertain answers go to the exact predicate, counted in `n_filter_uncertain`, untagged (numerics D-1 replaced `δ_gpu`). `[CLS-BAND]` as printed counts vertex decisions that exhausted the five ray directions and fell to the winding number (ARB-9's exhausted branch) | | | |
| ARB-9 | S6 ray battery | non-unanimous parity / ray through a degenerate feature | deterministic re-shoot down the fixed direction sequence; exhausted → GWN + WARN | `[CLS-RESHOOT]` | `arbitrated` |
| | | *as built:* a zero in a **plane** test is not re-shot but resolved by the symbolic perturbation `sign(n·dir)` (an f64 normal with a `4·EPSILON` relative filter — numerics §2 as-built row); a zero in a **line** test abandons the ray and re-shoots over `RAY_DIRECTIONS[0..5]`; exhausted → `winding_inside` (`[CLS-BAND]`). There is no majority vote, and the record provenance stays `lattice` — `arbitrated` is never written | | | |
| ARB-10 | S7 snap | move would invert a tet (exact sign) | reject the move; if it was a required constraint target, record an unsatisfied constraint + WARN | `[SNAP-REJ]` | – |
| | | *as built:* rejected moves are counted (`n_rejected`) with one aggregate WARN; no per-node unsatisfied-constraint record, and corner/curve targets are not distinguished from surface ones | | | |
| ARB-11 | S7 snap | 30 % shortest-incident-edge cap reached | clamp; mark under-snapped for the [V5] gate | `[SNAP-CAP]` | – |
| | | *as built:* nothing is clamped — a feature target beyond `SNAP_MOTION_CAP · l_min` (total displacement from the S5 position) is **ignored**, and a surface target beyond it is dropped as a re-check residual; the `under_snapped` list is filled and read by nothing (`[V5]` has no under-snapped gate) | | | |
| ARB-12 | S8 `assign_cut_sides` step 1 | a child has both strictly-inside and strictly-outside corners | `ambiguous` → `arbitrate` (5-ray at centroid) | `[OWN-DIAG]` | `arbitrated` |
| ARB-13 | S8 `assign_cut_sides` step 4 | union-find component with no sided member; oriented probe fails | `arbitrate` | `[OWN-PROBE]` | `arbitrated` |
| ARB-14 | S8 record write policy | `from_cut` vs `from_cut`, different side | reject the write → `ambiguous` → caller arbitrates | `[OWN-CONFLICT]` | `arbitrated` |
| | | *ARB-12..14 as built:* **not built** — `assign_cut_sides` does not exist; the §6 table assigns every child's side (`cut_record`), a side/crossing disagreement escalates (`[CUT-CASE]`), and no write conflict can arise. The sampled side decisions that do exist are ARB-26..28 | | | |
| ARB-15 | S8 guarded dry-run | negative child volume or > 1 % volume error | escalate the cell to §7, then §4.4 step 4 | `[CUT-GUARD]` | – |
| ARB-16 | S8 face-split dispatch | illegal state (dangling cut, K1 violation) or `split_4` failing the quality floor | escalate the cell to §7 | `[CUT-CASE]` / `[CUT-3EDGE]` | – |
| | | *as built:* the tags map the other way from the text — an illegal §6 state or an S6/S7 disagreement → `[CUT-CASE]`; a K1 edge (one component crossing twice) → `[CUT-3EDGE]`; both escalate. `split_4` is emitted unconditionally with no quality check (D-30) | | | |
| ARB-17 | S8 prism/quad emission | runtime positivity or 8° dihedral failure | §4.4 ladder: flip → Steiner → escalate → refine | `[CUT-PRISM]` | – |
| | | *as built (D-29):* §6 path — positivity or 1 % volume failure → escalate (`[CUT-GUARD]`, shared with ARB-15); band slab failing its row (positivity, 8°, 1 % volume) → the unchecked boundary-centroid fan, `regime = 2`, counted in `n_band_templates[steiner]`, no tag. `[CUT-PRISM]` and `Escalation::Quality` are declared and unreachable | | | |
| ARB-18 | S8 junction mesher | local CDT fails or G6-0 was a no-go | curve-pinned sequential cutting; chamfer ≤ h logged per cell | `[JCT-FALLBACK]` | – |
| | | *as built (D-17):* the §7.7 B4 ladder, ending in the whole-cell centroid fan; `[JCT-FALLBACK]` is a per-run count; per-cell reasons only under `RUSTMSPT_JCT_DIAG` | | | |
| ARB-19 | S8 junction seeding | 5-ray tie at a sub-region seed | GWN; else majority of face-adjacent classified tets + WARN | `[JCT-SEED]` | `arbitrated` |
| | | *as built (D-27):* every escalated piece is seeded (`seed_record`, every solid component sampled, one sample per tet on the fan arms); the classifier takes the first non-degenerate of the five directions, then the winding number; no face-adjacent majority; provenance `junction`. `[JCT-SEED]` reports the seeded population and the count of exact-predicate escalations (0 on the suite) | | | |
| ARB-20 | S8b FEM-aware ladder | predicted band quality fails gates | ladder outcomes 2–5 (refine / sheet / volumetric+WARN / reject with report) | `[S8b/G7-1] FEM-aware ladder …` and `[S8b/G7-1] REJECT` (rev 1.6; `[THIN-LADDER]` never existed; outcome 2 is reported, not honoured — §8.4) | – |
| ARB-21 | S9 IQD collapse guard | a collapse would fuse elements on opposite sides of a patch sharing a face | reject the collapse (evaluated on records, no tag lookup) | `[Q-COLLAPSE]` | – |
| ARB-22 | S9 hole-fill | cavity with empty record majority | `arbitrate` per section | `[Q-HOLEFILL]` | `arbitrated` |
| | | *ARB-21, ARB-22:* pending S9 (plan M-5.4) | | | |
| ARB-23 | S10 pre-derivation sweep | any `ambiguous` entry survives to labelling | debug: abort with coordinates; release: arbitrate + log | `[OWN-FIX]` | `arbitrated` |
| | | *as built (D-33):* **not built** — no sweep, `resolve()` drops the entry (§9.2 row 11 as-built note); no `[OWN-*]` tag exists in `src/` and the `arbitrated` provenance is never written; the s06 `arbitrated` cell array reports `is_ambiguous()` (still unresolved), a different predicate | | | |
| ARB-24 | S10 partition fill | partition volume below tolerance / suspected pinhole | WARN + [V8] pinhole diagnostic with the boundary-edge list | `[PART-PINHOLE]` | – |
| ARB-25 | S11 INP export | region key with no material mapping | hard error listing the keys (`unmapped: elset-only` → ELSET without a section + WARN) | `[EXP-UNMAPPED]` | – |
| | | *ARB-24:* the producer is pending S10 (plan M-6.1); the verifier half, `V8.pinhole_sheet`, runs. *ARB-25:* pending S11 (plan M-6.2) | | | |
| ARB-26 *(rev 1.6)* | S8 §6 table, rows I / i | every vertex on or outside the patch, no cut edge | the cell's centroid is classified by the exact point classifier; `Some(true)` → the cell is the component's | `[S8/G6-2] … interior sample` | `cut` |
| ARB-27 *(rev 1.6)* | S8 table-vs-interior check | a table child's centroid classifies to the other side with certainty | escalate the whole cell to §7 | folded into `[JCT-CELL]` (no tag of its own yet) | – |
| ARB-28 *(rev 1.6)* | S8 hidden-body recovery | a component the record omits, with an `on_cut` vertex on the child, classifies inside at the child's centroid | add the `inside` entry | `[S8/G6-3] N cut child(ren) recovered a body` | `cut` |
| ARB-29 *(rev 1.6)* | S7 → S4 K1 loop | a doubly-crossed edge or an uncovered locked-curve segment | re-ask the field for half that scale and rerun S4–S7, at most `K1_MAX_PASSES = 3` | `[S7/K1] pass n …` | – |

Producers ARB-12…ARB-14 are the ones the 0.5 % gate measures. ARB-15…ARB-19 are
*escalations*, not samples: they never guess, they hand the cell to a stronger
mesher, and their counters gate G6-0's go/no-go. *(rev 1.6: the counters are reported per run —
`[JCT-CELL]`, `[CUT-3EDGE]`, `[CUT-CASE]`, `[CUT-GUARD]` — and nothing gates on them; G6-0's
go/no-go was taken in the plan, §0.)*

> **Tag census (rev 1.6, `grep -rn` over `src/` for each tag as printed, 2026-09-23, D-39).**
> Eleven rows tag what the catalog says, or close to it: `[ARR-RESID]`, `[ARR-COINC]`,
> `[THIN-SKIP]` (S3's skip taxonomy, ARB-6), `[CLS-RESHOOT]`, `[SNAP-REJ]`, `[SNAP-CAP]`,
> `[CUT-GUARD]`, `[CUT-CASE]`, `[CUT-3EDGE]`, `[CUT-PRISM]` (declared, unreachable),
> `[JCT-FALLBACK]`. Three tag a *different* event: `[ARR-PREC]` is printed per degraded
> neighbourhood (`PrecisionFloor` / `QuantizedOrderAmbiguity`), never per DD escalation — those
> are counted in `ArrangementStats.precision_escalations` and not logged; `[CLS-BAND]` counts
> decisions that exhausted the five ray directions and fell to the winding number, not a GPU
> margin band; `[JCT-SEED]` counts every seeded piece, not ties. Eleven do not exist:
> `[TOPO-GWN]` (the behaviour logs as `[G2-4/GWN]`), `[TOPO-GWN-BAND]`, `[THIN-LOCK]` (as
> `[S3/S4] WARN … locked`), `[OWN-DIAG]`, `[OWN-PROBE]`, `[OWN-CONFLICT]`, `[OWN-STATS]`,
> `[OWN-FIX]`, `[THIN-LADDER]` (as `[S8b/G7-1]`), `[Q-COLLAPSE]`, `[Q-HOLEFILL]`,
> `[PART-PINHOLE]`, `[EXP-UNMAPPED]` — the last six because S9–S11 are not built. `[CUT-CASE]`
> has **two producers**: S7's K1 edge census ("N edge(s) are crossed more than once") and S8's
> illegal-state escalation; the S7 line should become `[SNAP-K1]`. ARB-12..14's sites do not
> exist — `cut_record` settles every child's side from the §6 table — so the count of arbitrated
> children is zero by construction, `Provenance::Arbitrated` is never written, and the 0.5 %
> gate measures nothing until ARB-23's sweep exists (D-33). Four live arbitration sites had no
> row and are added below as ARB-26..29. Tags that exist and the catalog does not name:
> `[JCT-CELL]`, `[JCT-CUT]`, `[JCT-FACE]`, `[JCT-DEGENERATE]`, `[FACE-CACHE]`, `[CREASE-FACE]`,
> `[S67-…]`, `[SNAP-CURVE]`, `[SNAP-REACH]`, `[S7/K1]`, `[S8/G6-2]`, `[S8/G6-3]`,
> `[S8b/G7-1]`, `[S8b/G7-2]`, `[S3/S4]`, `[G2-4/GWN]`. Measured on A-3 / A-6a / A-7a
> (`rustmspt mesh --config <case>.yaml`): a3 `[CUT-CASE] 16`, `[JCT-CELL] 1170`,
> `[JCT-FALLBACK] 199`, `[JCT-SEED] 20314`; a6a `[ARR-PREC] 1`, `[CUT-CASE] 139` (S7's),
> `[CUT-3EDGE] 416`, `[JCT-FALLBACK] 158`; a7a `[ARR-PREC] 12`, `[ARR-RESID] 28`; no
> `[CLS-*]`, `[SNAP-REJ]`, `[SNAP-CAP]`, `[CUT-GUARD]` or `[CUT-PRISM]` line on any of the
> three.

---

## 13. Test obligations (feeding plan §17.3)

Each frozen table gets one unit test that fails if the table is edited without
re-deriving it.

| ID | Test | Asserts | Lands with |
|---|---|---|---|
| T-K1 | Freudenthal table | 6 rows, each `orient3d > 0`; volumes sum exactly to `h³`; no interior overlap | G4-2 — met: `lattice::tests::the_frozen_freudenthal_table_is_positive_and_tiles_the_cube` |
| T-K2 | Rule D | induced diagonals equal min→max corner on all 6 faces; invariance under cell translation | G4-2 — met: `rule_d_reproduces_the_frozen_face_table`, `rule_d_is_invariant_under_rotation_of_the_quad` (translation invariance holds by construction in global index space; no separate test) |
| T-F1 | Fan face rule | `f` reproduces the P/E/Q counts 2 / 4+k / 8; identical output from both incident cells | G4-2 — met indirectly: `a_graded_lattice_uses_fan_cells_and_stays_within_the_template_inventory` (per-cell counts), conformity tests (identical output); no per-face count test |
| T-F2 | Lattice conformity | randomised balanced octrees: every interior face shared exactly twice, zero hanging nodes, exact total volume | G4-2 — met: `a_graded_lattice_is_conforming_theorem_t1`, `conformity_holds_over_randomized_sizing_fields`, `the_edge_only_refinement_pattern_conforms`, `a_non_cubic_domain_stays_conforming` |
| T-F3 | Bound P1 | every emitted lattice tet has `h³/48 ≤ V ≤ h³/6` | G4-2 — met: the inventory test's volume bounds, `every_lattice_node_is_distinct_and_in_node_key_order`, `the_templates_match_the_corrected_quality_table` |
| T-Q1 | Rule SNK / T2 | randomised key orders never produce a cyclic prism; the 6 patterns are reproduced | G4-2 — met: `snk_never_produces_a_cyclic_prism`, `only_the_two_cyclic_sets_are_undecomposable`, `the_prism_table_tiles_the_prism`, `a_well_shaped_prism_decomposes_positively`, `snk_is_a_function_of_the_keys_alone` |
| T-C1 | Face-split tables | all 5 rows; both windings identical; exact area tiling | G6-2 — met: `the_face_table_tiles_the_face` (`split_2`, `split_R`), `two_cells_sharing_a_face_agree_on_it` (`split_3`); `split_3`/`split_4` tiling through T-C2's conformity tests (§5.3) |
| T-C2 | Cut case table | all 6 cases: positivity, exact volume partition, boundary triangles equal the face-split output | G6-2 — met: `every_case_partitions_the_parent_volume`, `the_pieces_are_separated_by_the_cut`, `the_cut_agrees_with_the_face_split_table_on_every_parent_face`, `every_side_assignment_agrees_with_the_face_split_table`, `an_uncut_configuration_is_rejected_by_the_table`, `a_dangling_cut_is_refused`, `the_dry_run_catches_a_broken_cut`, `the_cut_is_conforming` |
| T-C3 | Invariant K2 | a cut node within weld tolerance of a parent node is promoted, never emitted | G6-2 — **open**: S7's half is covered (`the_recheck_clears_the_near_endpoint_band`, `crossings_match_the_snapped_coordinates`); S8's promotion of a crossing whose edge already has an on-cut endpoint (§5.1) has no test by that name |
| T-J1 | Shared-face cache | both incident cells read the identical triangulation; a fingerprint mismatch is returned as `Err` by the cache (`the_cache_computes_once_and_rejects_a_changed_fingerprint`) and — as built — **counted** by S8 (`face_cache_conflicts`, `[FACE-CACHE]`); no integration test asserts 0 conflicts; the integration obligation is plan M-2.0 | G6-4 (kernel), M-2.0 (integration) |
| T-J2 | Table/triangulator agreement | the generic constrained face triangulator reproduces §5.2 exactly (`facecache::tests::j2_reproduces_the_frozen_face_split_table`, all five rows; the routine it pins has no production caller — §7.3 as-built note) | G6-4 (met, §14 [11]) |
| T-B1 | Band k-cases | k = 0..3 counts 3/2/1/0, positivity; mixed-k strip is conforming | G7-1 — met: `frozen_table_emits_the_frozen_tet_counts`, `band_cell_boundary_is_closed_for_every_k`, `every_k_case_fills_its_own_boundary`, `invariant_b1_neighbouring_cells_agree_on_a_shared_quad` (two cells; the 24-cell strip of §14 [6] has no in-repo source) |
| T-R1 | `resolve()` | rows 1–9 as a table test (`resolve_reproduces_the_frozen_truth_table`); row 10 by fixture (`a_sheet_never_claims_volume`); row 11 pinned as *drop*, not error (`an_outside_or_ambiguous_entry_never_claims_the_tet`) — D-33 | G5-1 (landed); row 11's error is M-6.1's |
| T-M1 | Coincidence table | all 10 rows in all 3 modes; C10 policy/classification at G2-2, C10 clipping/tag/curve geometry at G2-5 | G2-2 / G2-5 — met: `coincidence_policy_helpers_cover_all_thirty_case_mode_combinations`, `coincidence_geometry_fixtures_cover_all_policy_modes`, the `c1_c2_…`, `c3_…`, `c4_c5_c6_…`, `c7_…`, `c8_c9_and_c10_…`, `c10_…` tests of `meshgen_arrange_tests.rs`; *rev 1.6:* those are policy-helper and per-case tests — none covers C1-against-C3 for a bordered coincident patch, the sheet box-clip curve, or a fully merged component's survival past S2b (§14 [12], D-40..D-42) |
| T-S1 | Coupling loop | monotone `h`; ≤ 3 iterations on the suite; oscillation fixture locks volumetric; ε-ordering assertion fires | G3-2 — met: `the_loop_converges_within_three_iterations_on_a_monotone_constraint`, `the_constraint_alone_cannot_make_a_region_oscillate`, `an_oscillating_region_locks_to_volumetric`, `hitting_the_iteration_cap_locks_the_unstable_regions`, `the_g8_ordering_assertion_fires_on_the_realised_field`, `the_coupling_loop_converges_against_the_real_constraint` |
| T-A1 | Arbitration catalog | every ARB-id is reachable by a fixture and logs its own tag; `[OWN-STATS]` counts them | G6-3 — **not met**: no test walks the catalog; 11 of the 25 tags do not exist in `src/`, three tag a different event, and `[OWN-STATS]` has no producer (§12 census, D-39); the rows that a fixture does reach — ARB-3, ARB-6, ARB-7, ARB-10, ARB-15 — are reached by tests that never assert the tag text |
| T-Q2 *(rev 1.6)* | §3.7 corrected Q row | the transition templates reproduce `35.264° / 125.264° / 1.6052` | G4-2 — met: `the_templates_match_the_corrected_quality_table` |
| T-Q3 *(rev 1.6)* | §3.7 post-snap / post-cut gate (G6-6) | snapping erodes the fans' worst dihedral by less than half; post-cut fan p5 ≥ 0.25 × Freudenthal p5 | G6-6 — met: `transition_fans_survive_the_snap_and_the_cut` |
| T-B2 *(rev 1.6)* | §8.2 measured row (rev 1.3) | the nominal right-isoceles band cell reproduces `18.281° / 19.561° / 19.853°` and `2.0953 / 2.0165 / 1.9662` | G7-1 — met: `nominal_band_cell_matches_the_corrected_spec_row` |

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
produced 27 tets with no face shared more than twice. *(rev 1.6: the tet counts and positivity
are pinned in-tree — `frozen_table_emits_the_frozen_tet_counts`,
`every_k_case_fills_its_own_boundary`; the dihedral column is superseded by [9] rev 1.3
(18.281 / 19.561 / 19.853); the 24-cell strip has **no in-repo source** — the permanent form of
B1's verification is `invariant_b1_neighbouring_cells_agree_on_a_shared_quad`, one `k = 0` and
one `k = 1` cell sharing a quad. R6: informative, unreproduced.)*

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
| **ordering on `1e-6 · ε`** (rev 1.4 wrote `1e-6 · q`; the run used the code's grid — D-35) | **0** | **0** | **0** | **0** |

The last row is `[V3]` clean, and the whole report PASSes, on both that dataset and a
two-plate gap fixture; byte-identical at `RAYON_NUM_THREADS` 1 and 8.

**[11] As-built audit (rev 1.6, 2026-09-23, `891badc`)** — every MUST, table row, constant,
log tag and test obligation of this document read against `src/meshgen/` by nine independent
auditors (one per section group); the plan's §13 names the procedure. Frozen tables: §1.1 outward faces, §2.2, §2.3, §3.3, §3.5,
§3.7, §4.3, §5.2, §6, §8.2 (all rows incl. the rev 1.3 measured row), §9.2 rows 1–10 —
**reproduced by the code verbatim** and pinned by the tests §13 names; §9.2 row 11 — not
reproduced (D-33). Measured during the audit: `cargo test --release` on the lattice, cut, band,
classify, thin, sizing and quality-gate suites all green; G6-6's numbers (§3.7); the band
reachability numbers (§8.2); A-3 default and gated S8 runs (§7.1, §7.3). T-J2 is met
(`j2_reproduces_the_frozen_face_split_table`: uncut, split_2, split_3, split_4, split_R identical
triangle sets). Forty-three of the divergence findings were then read again by an independent verifier — 42 confirmed, one
refuted (the `cdt::orient` claim, corrected in D-36), two corrected in status (the band rows, D-32) —
before the owner stopped the verification pass on cost; the remaining findings are single-pass, each
carrying the lines and measurements its auditor read, spot-checked by grep and not independently
re-read. The deviations found are D-17 … D-43 below; none changes a frozen table.

**[12] §10 coincidence probes (rev 1.6, 2026-09-23, `891badc`)** — the arrangement suite
(`cargo test --offline --release --test meshgen_arrange_tests`, 48 passed); one end-to-end run of
two identical closed cubes at equal priority (`rustmspt mesh`, `a2_cube.stl` listed twice,
`coincidence: warn`, `h` 0.1/0.02): S2b reports one solid and S6 one solid plus one sheet with two
region keys — the D-42 loss; and five patch-level probes through a scratch crate (open box alone
→ `SolidDefective`, 1 closure defect; the same box plus a sheet lid → `SolidClosed` + `Sheet`, 0
defects; a sheet on the `z = 0` domain face → 2 C10 events, no `Box` curve; a cube on `z = 0`
plus an identical sheet → 0 box-tagged faces; cube + coincident sheet → `C3`, two identical sheets
→ `C1`). The probe crate is not committed; plan M-4.0 commits the cases as fixtures.


---

## 15. Deviations and open items

Deviations from the reference implementation or from the plan's rev-2 prose, each with
its reason:

| # | Deviation | Reason |
|---|---|---|
| D-1 | **Strong** (face + edge + vertex) 2:1 balance required; the plan said "any finer neighbour" makes a transition cell | Face-only balance leaves edge-adjacent finer leaves free to put a node inside an edge of an unsplit face — a hanging node the face rule would not see. §3.1 L1/L2 depend on strong balance |
| D-2 | A cell with **any** split edge is a fan cell, not only one with a split face | Otherwise a Freudenthal neighbour would emit a plain diagonal across a face whose edge carries a midpoint (crack). This is the non-obvious half of Theorem T1 |
| D-3 | `split_3`'s quad diagonal uses Rule SNK, not the reference implementation's shorter-distance test with a `1e7/1e4/1e0` coordinate tie-break | The distance rule gives no global non-cyclicity guarantee; SNK does (T2), and it is what lets §6's prisms be provably decomposable. The distance rule survives only as the §4.4 pairwise flip, where it is a *quality* choice guarded by a validity check |
| D-4 | `split_4` is the medial split; the reference implementation's long-side/right-triangle variants are dropped | Those variants are quality heuristics for the SAMR hanging-node path, which this module does not have (no hanging nodes by construction). Retaining them would add order-dependent branches. *(rev 1.6: the row's "a case that should mostly escalate anyway" no longer describes the code — the medial split is emitted unconditionally and the split_4 quality escalation of ARB-16 is not built, D-30.)* |
| D-5 | Invariant K2: cut nodes coincident with parent nodes are **promoted**, not grounds for deleting the element | The reference implementation removes the element, leaving a hole. Promotion is closed-mesh-preserving and moves the state to a legal table row |
| D-6 | `split_R` (rim termination) added to the face-split table | R-C1 requires a sheet's cut front to terminate inside a cell; the reference table has no such row. Needed by G6-5 |
| D-7 | Explicit 6-case cut table (§6) replaces the reference implementation's search-based `connect_and_fold` + `validate_internal_cuts` + fan-pivot recovery for single-patch cells | The single-patch cut of a tet has exactly six configurations; enumerating them is deterministic, provably volume-exact, and removes the `[KIRI-WARN] NEW CASE NEEDED` force-fold path that knowingly produces mesh cracks. Fan-pivot recovery is superseded by escalation to §7's junction path, which always terminates. *(rev 1.6: the row's "the §4.4 ladder, whose last two rungs (escalate, refine) always terminate" is corrected — on the §6 path the ladder has no quality trigger and no refine rung, D-29; refinement is the pipeline's K1 loop, ARB-29.)* |
| D-8 | `h` is a **running minimum** across coupling iterations (§11.1) | The plan asserted monotonicity; without the explicit `min(h⁽ⁿ⁾, ·)` it does not hold, because a regime change can relax a constraint |
| D-9 | Rule S3-M: the regime threshold quantity is the closest-pair distance measured **once**, not the re-sampled field | Re-measurement lowers `t_r` as sampling densifies, which breaks the monotonicity argument on both sides of the comparison |
| D-10 | Rule S3-S: regions may split but not merge across coupling iterations | Merging takes a minimum over a union and can lower `t_r`, reversing the regime direction |
| D-11 | C7 uses complete-link epsilon clusters, not transitive pairwise union | Pairwise union can merge endpoints farther apart than epsilon; complete-link preserves the policy's geometric-resolution bound |
| D-13 | §3.7's Q row corrected from `45.000°/90.000°/1.3938` to `35.264°/125.264°/1.6052` (rev 1.2) | The original row repeated the centre–corner–midpoint row's numbers instead of measuring all eight quadrant triangles. The value is forced by Rule D and §3.4, so no rule changed; the *prediction* G4-3 measures against did |
| D-16 | S8 orders nodes on `1e-6 · ε` (rev 1.4 wrote `1e-6 · q`; corrected at rev 1.6, D-35) rather than on the weld grid `q` (Rule K-O, rev 1.4) | §1.2's weld invariant does not cover constructed crossings, which are not welded against each other; ordering on a grid that does not separate them makes Rule SNK's "smallest key" not a total order. Nothing about the *rule* changed - it still depends only on position, never on node indices, which is what §1.2 requires of it |
| D-14 | §8.2's informative measured row corrected: the AR column `1.89 / 1.80 / 1.74` replaced by `2.0953 / 2.0165 / 1.9662`, and the nominal cell's shape stated explicitly (rev 1.3) | The AR numbers matched no cell of the §8.2 family under `[V4]`'s convention, and the row did not say which cap it was measured on — so the numbers could not be reproduced, which is what a *measured* row exists for. Two independent computations agree (§14 [9]). The dihedral column was correct and is unchanged; no frozen rule moved |
| D-15 | The §4.4 ladder's flip step is implemented for band cells as a **pairwise** decision taken by the layer, not by the cell | A diagonal flip is not a pure function of the quad, so a cell taking one alone leaves the neighbour that shares the quad non-conforming. The layer owns both incident cells and accepts the flip only if both still mesh — which is what the reference thin-feature design §3.8's "attempted only pairwise" requires and what `cut.rs`'s §4.4 ladder skips |
| D-12 | A split parent edge forbids CDT chords that bypass an intermediate registry node | Snap-rounded C3 coordinates can be microscopically off the analytic parent edge; allowing the bypass chord emits a sliver and makes retessellated coincident patches nonconforming |
| D-17 | **The junction fallback as built is a centroid fan, not §7.6's curve-pinned sequential kirigami** — default path: split the cell by each surface (`split_escalated_cell`), fan each piece to its centroid; gated path (`RUSTMSPT_PLC_PASS`): the §7.4 kernel, then the facet-split fan, then a whole-cell fan (§7.7) | An undeclared deviation until rev 1.6 (the plan's record §6.2 named it on 2026-08-15). It is **not accepted**: the centroid fan violates `[R1]` and carries 91.7–98.4 % of each case's off-surface area (plan §2.4). Removed by plan M-2.3; §7.6 is rewritten at M-0.2 |
| D-18 | **J1's fingerprint mismatch is counted, not raised**, and the fingerprint is the component-id set plus a crease flag rather than the constraint entity ids (§7.3 as-built note) | Recorded from the review (MG-06); the clause stands and the code is corrected by plan M-2.0 |
| D-19 | **One X per input file**, not per connected closed component (§9.1 as-built note) | Recorded from the review (MG-04); R-A2 stands; plan M-4.0a |
| D-20 | **Closure is proved from a global edge incidence**, so another component's face can close this one's opening (§12 ARB-4 as-built note) | Recorded from the review (MG-05); plan M-4.0b |
| D-21 | **Two frozen remedies are deliberately unimplemented** and say so in `cut.rs:14–23`: Invariant K1's refine-in-cut (§5.1 — the S8-internal refinement of a doubly-crossed edge's cells; the pipeline's K1 loop around S4–S7 is what exists) and §4.4's ladder steps 1–2 for the single-patch cut (pairwise flip and the Steiner fallback; thin.rs implements both for band cells, D-15) | Recorded so the plan's deviations list is complete (plan S-5). Both stay unimplemented until a measured case needs them; the ladder's steps 3–4 (escalate, refine) are what the cut uses |
| D-22 | **`FaceTriCache` is implemented and authoritative** for escalated-cell faces (2026-08-15), contrary to the module note at `cdt.rs:47` that said it was not | Stale text corrected in the record (plan S-6); the cache stores triangles in a canonical winding and each cell re-winds outward from itself |
| D-23 | **§7.1's rev 1.5 amendment describes the gated path as the path.** As built the trace-driven offer of §7.2 ahead of §6's table exists only when `RUSTMSPT_PLC_PASS` is set; the default path is §6's table for every single-patch cell and the D-17 fan for every escalated cell | A prototype handle that outlived its gate (plan R3); plan M-3 makes the gated path the only path and deletes the handle. Until then the amendment is normative for the gated path and the default path is the deviation |
| D-24 | **§7.4 bullets 3–4 as built:** face-level Steiner nodes (the face centroid, the chord meeting point, the curve pierce point, the crease hub) lie **on** constraints and **on** shared faces, are constructed by fixed-sequence float operations rather than averages, are interned once per face so both owners share them (J1 by sharing), and are never snapped to a neighbour by proximity (the gated path's trace endpoints — constraint vertices, not Steiner points — are, to the nearest candidate within `ε`); the interior mesher inserts no Steiner point at all (recovery is flips and edge removal). Plan M-2.1's facet-Steiner remedy — points on a facet's interior strictly inside the cell — is the pending amendment (plan S-2) | The face-level nodes are what make a crossed or pierced face triangulable at all, and interning them per face is what keeps J1; the interior rule stands as written. Not amended for M-2.1 at rev 1.6: R6 forbids freezing an unmeasured rule; M-0.2 amends after M-2.1's measurement |
| D-25 | **§7.4 bullet 1 as built:** the default path meshes an escalated cell by successive convex clipping on the fragment's planes and per-piece centroid fans; the incremental CDT runs only under the gate | The cell-by-cell CDT cracked the mesh on A-8 (12 leaks) because whether to clip is decided per cell while J1 needs it decided per face; the face-first route is the design and plan M-2/M-3 make it the path |
| D-26 | **§7.4 bullet 2 as built:** the clipping kernel, `cdt::orient` and the §7.6 cap/conformity guards decide sides by float distances with relative tolerances (§7.4 as-built note lists them) | Recorded; numerics §1.3 names the constants; the guards that decide correctness downstream (closure, exact `fan_is_sound`, volume) are exact |
| D-27 | **§7.5 as built:** one sample per region on the gated Meshed arm; one sample **per fan tet** on every fan arm; the sample asks about every solid component | Per-piece seeding measured much worse (A-7a volume error 0.40 % → 4.17 %); recovering omitted components closed the "`Outside` read off snapped vertices" defect |
| D-28 | **§7.1/§7.2 as built:** the triage counts components, not patches; curve containment is not a trigger; three code-only triggers (inconsistency, table-vs-interior, dry-run); the mesher takes no curve segments; S8 creates new identities on edges and faces | Each is recorded with its measured reason in the as-built notes; §7.1's original rule stands as the requirement |
| D-29 | **§4.4 as built:** the 8° floor is not applied on the §6 path (`Quality` / `[CUT-PRISM]` unreachable); band slabs enter the ladder at step 2 with an unchecked fan; steps 1, 3, 4 and the regional demotion are off-path (`mesh_band_layer` has no caller); `[V7]`'s `ThinSkipRegion` has no producer | `cut.rs:14–23` records the §6-path shortcut as a decision (plan S-5); applying the floor there would send the cells to §7's fan, measured worse for P3 than a thin-but-valid table cut. The band-path skip was undocumented until rev 1.6 |
| D-30 | **§5.2 as built:** `split_R` has no producer (S7 constructs no rim endpoint); `split_4` is unreachable from §6 and untested for quality; `[CUT-3EDGE]` tags Invariant K1's cells, not the row | R-C1 termination is handled by escalation until S7 emits rim endpoints; the row stays frozen and tested |
| D-31 | **§6 as built:** rows I and i settle `\|I\| = 0` / `\|O\| = 0` cells (interior sample); the S6/S7 agreement guard; the table-vs-interior check; the dry-run checks orientation, 1 % closure and `V_parent > 0` but not node membership; the all-on-cut face is tagged by the labelled-boundary pass | Rows I/i closed the `[V9]` corner defect (A-7a 3 → 0, A-7b 43 → 0, A-8 69 → 0, a relabelling); the interior check took `[V5]`'s misattribution to 0 on eight of nine cases |
| D-32 | **§8.1/§8.2 as built:** band cells are lattice cells recognised by §8.3's doubly-cut face rule, not Φ-spanned prisms; collapse is per lattice edge, gated by `collapse_sheets` (default off); only the `k = 0` row is reached in the pipeline — an all-collapsed pair takes §6's table on the welded-pair path, and rows `k = 1, 2` are exercised by `frozen_table_emits_the_frozen_tet_counts` alone (verified 2026-09-23); band tets seeded per tet with `junction` provenance; the ladder of §8.4 | S8 works in cut nodes, and a cell-local construction keeps Invariant C without carrying Φ across stages; `collapse_sheets` is an R3 open item (plan M-4.3) |
| D-33 | **§9.2 row 11 / ARB-23 as built:** an `ambiguous` entry is dropped by `resolve()`, no sweep, no `[OWN-*]` tag, `arbitrated` never written; T-R1 pins the drop | The row stays normative; plan M-6.1 decides whether S10 gets the sweep. `[OWN-STATS]`'s 0.5 % gate has no producer either |
| D-34 | **§11 as built:** `h⁽⁰⁾ = h_max`; `C(R)` defined; `τ_l ≤ 1` only post-loop; S3-M falls back to `t_raw` and the sweep is a grid; oscillation = an A → B → A return; the `Tightened` lock keeps the tighter regime; the cap lock covers every region that changed at all; G-8 on the scalar `h` | Each documented in §11's as-built notes; the monotonicity argument is unchanged. Two open items: whether "lock after the first downgrade" meant the `Tightened` behaviour, and whether the cap lock should be narrowed to regions still changing |
| D-35 | **§1.2 as built:** S7 keys on `ε`; S8 orders on `1e-6·ε` (= `1e-5·q`), not `1e-6·q`; `[V2]` tests on `1e-6·diag`; the weld invariant covers welded nodes only; node ids fix emission order and identity | The K-O measurement (§14 [10]) was made with the code's grid; the constant's doc comment ("finer than the weld grid") is the stale text, to be fixed with M-3.3 |
| D-36 | **§1.1 as built:** one transposition per stage (S5 `n1,n2`; S8 and the kernel `n0,n1`; the kernel's canonical form `n2,n3`); `cdt::orient` is a float triple product reachable only through the test-only `tetrahedralise` (no production element takes its sign — verified 2026-09-23); `junction::TET_FACES` is an unordered slot table | All odd permutations, so positivity holds; every production tet of the kernel is fixed by `orient3d_filtered`/`orient3d`; recorded so a slot reader takes the element as emitted |
| D-37 | **§1.4 as built:** cut stages emit in Morton cell order, no `(Y, X)` sort, no canonical re-sort at export | R-P2 byte-identity is measured on that order; a canonical re-sort is S11's (plan M-6.2) if `[V11]` strict needs one |
| D-38 | **§3 as built:** the octree is a domain-trimmed forest (subtree cut at `≤ forest_level`); the node set is read back off emitted triangles; balance children inherit `h`; a 20 M-tet budget refuses the build | Dropping an individual finer cell falsified L1 (216 hanging nodes); the budget is the §3.5 inventory maximum |
| D-39 | **§12 as built:** eleven of the catalog's twenty-five tags do not exist (`[TOPO-GWN]` logs as `[G2-4/GWN]`, `[THIN-LOCK]` as `[S3/S4]`, `[THIN-LADDER]` as `[S8b/G7-1]`; the `[OWN-*]`, `[Q-*]`, `[PART-PINHOLE]` and `[EXP-UNMAPPED]` rows have no code), three tag a different event (`[ARR-PREC]`, `[CLS-BAND]`, `[JCT-SEED]`), `[CUT-CASE]` has two producers, ARB-12..14 and ARB-17 are not built, ARB-5 has no GPU half, ARB-11 drops rather than clamps, and four live sites had no row (ARB-26..29); the 0.5 % gate and `[OWN-STATS]` have no producer | Tags are what the plan's censuses grep; a tag the spec names and the code does not emit is a check that measures nothing. Each row now carries its as-built line, and §12's census is the reference for plan M-3.3's tag cleanup |
| D-40 | **§10 rows C1–C3 and C5–C6 as built:** a fully coincident patch bordered by any single-owner face (coplanar or not), or with inconsistent per-tag orientation, is classified C3, not C1/C2; C5/C6 are split by coplanarity, not by shared-vertex against tangency, and fire on a component's own vertex adjacency; rows C1–C3/C7 apply to self-coincidence (§10 as-built note) | The reject column is unaffected (both ✗); the `warn` label, the extra overlay-boundary curves and the point-feature set differ from the table. Recorded, not accepted: plan M-4.0 decides whether the code counts only coplanar exclusive neighbours and skips same-component vertex adjacency, or the table is rewritten to what ships |
| D-41 | **§10 row C10 as built:** no box-clip curve is emitted for a sheet (only solid cap loops build `Box` curves); the box tag is gated on the primary `face.component`, so a sheet merged under a solid's primary tag (C9 ∩ C10) is not box-tagged; "tagged both" is `FaceTagKind = 2` with the sheet X in the set, sheet-ness read from `ComponentKind` | The existing test checks only `face.box_tagged`. The curve and the gating are G2-5's unfinished half of the row; plan M-4.0 |
| D-42 | **Invariant M1 does not survive S2b:** `rebuild_topology` selects faces by the primary tag, leaves a fully merged component unclassified, and S6 defaults it to `Sheet` — two identical solids at equal priority deliver one solid and lose the other's material (§10 as-built note, §14 [12]) | Not accepted: it contradicts R-A3 and rows C1/C7's "all tags preserved". Plan M-4.0b is widened from closure to the full per-X member set; fixture A-16 |
| D-43 | **A C7 match declined by complete-link keeps its C7 event** (so `reject` refuses it) and is recorded as a `QuantizedOrderAmbiguity` degraded neighbourhood; the pair stays two faces | Consistent with the G2-3 boundary's intent (never silently skipped); the enumerated trigger list now names the decline |

Open items explicitly **not** frozen here, with their owners (re-stated at rev 1.6):

- Error bounds, escalation thresholds, `δ_gpu`, ray-direction constants and the
  exact-predicate inventory → **G0-2** (`SPEC_meshgen_numerics.md`).
- Post-snap transition-cell quality on the acceptance/reference matrix → **plan M-5.1**; the
  fixture-level measurement exists (`tests/meshgen_quality_gate_tests.rs`: one UV sphere
  `r = 0.3` in the unit cube, `h` 0.05–0.2, one curvature source so transition cells exist —
  measured at `891badc` by `cargo test --release --test meshgen_quality_gate_tests -- --nocapture`:
  fan cells pre-snap worst/p5/median `35.264 / 45.000 / 45.000`, post-snap
  `35.073 / 44.165 / 45.000`, post-cut `2.000 / 5.542 / 44.994` against Freudenthal
  `0.243 / 5.268 / 45.000`, populations 4,360 / 5,844, relative gates only), and §3.7 gives the
  pre-snap prediction it is read against. Gate G4-3 as a *gate* is therefore located, not closed.
- Junction-cell CDT viability → **answered** (§0, rev 1.6): the kernel exists and is exact where it
  runs; the fallback as built is D-17 and the go/no-go on §7.6 is *no* — the section is rewritten
  at plan M-0.2 once M-2 has decided the only fallback.
- The facet-Steiner amendment to §7.4 and the trace-point rule for facet vertices on a shared face
  → **plan M-0.2**, after M-2.1 and M-2.2 measure them (D-24).
- The J1 fingerprint's identity, the hard-error path, and the retirement of the counted-conflict
  wrapper → **plan M-2.0** (D-18).
- Component identity per connected closed component and per-component closure → **plan M-4.0**
  (D-19, D-20).
- §10's patch classification, point-contact semantics, the sheet box-clip curve and the box tag's
  gating → **plan M-4.0** (D-40, D-41); Invariant M1 past S2b → **plan M-4.0b** (D-42).
- P3 against which surface, and P4 under input-forced angles (plan §1.3, MG-10) → **plan D-6..D-8**:
  §8.1's collapse to the pair midpoint puts the sheet on neither original wall, and §4.4's 8° floor
  cannot be met inside a 1° material wedge; neither clause changes until the owner decides which
  object P3 and P4 are stated against.
- G2-3 is closed by §10/§14. In-situ fallback rates remain diagnostics; changing
  the boundary or trigger list requires a new spec revision.
