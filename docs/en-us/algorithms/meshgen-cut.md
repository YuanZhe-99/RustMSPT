# The Cut (S8): Making a Lattice Conform to a Surface

S8 is where the mesh starts to follow the geometry. Its input is a background tetrahedral lattice
that was built without looking at the input (S5), a classification of every lattice vertex against
every solid (S6), and the same lattice with some nodes snapped onto surfaces, curves and corners
(S7). Its output is a conforming tetrahedral mesh whose material boundaries lie on the input
surfaces, with every interface face declared. The normative description is
`SPEC_meshgen_geometry.md` §5–§7; this page explains how the implementation (`src/meshgen/cut.rs`,
`src/meshgen/cdt.rs`, `src/meshgen/facecache.rs`) realises it and why its rules are shaped the way
they are. Function-level contracts are in `docs/en-us/reference/meshgen.md`.

## What the surface is, for the cut

The cut does not see the input STL's triangles. `PointClassifier::merge_coplanar` turns each
component's active surface into **convex polygons**: coplanar neighbours within the weld quantum
(Invariant M1, `0.1 eps`) merge, so an STL rectangle's diagonal is not a constraint the mesh has to
carry. Two kinds of triangle never merge: one that touches an edge another component uses, and one
with a vertex on an intersection curve (a transversal junction where one body's wedge can be a
fraction of `eps` thick). Faces S6 found buried inside their own component's union are not part of
it (`restrict_to_active`). Every trace and clip site asks `polygons_near` for the polygons whose box
meets a query box, in index order, so the answer is the same as from the whole surface.

## Nodes the cut adds

* **Cut nodes**, one per `(lattice edge, component)` crossing, from S7's crossings, keyed by the
  edge so the two cells around it share one node. A second crossing of the same edge by the same
  component (Invariant K1) gets its own node.
* **Contact nodes**: where two solids touch exactly, their coincident crossings on an edge share one
  node (`contact_edges`), or the contact plane would be two coincident surfaces with a crack.
* **Rim nodes**: a gap below `t_sheet` is collapsed at the cut node (G7-2): the two walls' crossings
  on one edge become one shared node, so no cell ever sees the gap.
* **Trace points**: where a surface polygon crosses a lattice face, `trace_on_face` returns the
  chord; its endpoints are interned on the face's edges (as a function of the edge, never of the
  face) or inside the face.

Ordering uses a key on `1e-6 x` the weld quantum (`KEY_ORDER_REFINEMENT`), never the weld grid
itself: two crossings can share a weld key, and an ordering key must separate what it orders.

## Every lattice face is triangulated once

Two cells share each interior lattice face, and they must agree on its triangulation (Invariant
J1). The rule that guarantees it: **a face's triangulation is a pure function of the face** - its
three corners, the cut nodes and trace points on it, and the chords that must be its edges.

* An ordinary face takes §5.2's table (`uncut`, `split_2`, `split_3`, `split_4`, `split_R`).
* A face carrying trace chords is triangulated by `constrained_face_triangulation`: a Delaunay
  triangulation seeded with the face itself (no super-triangle - sliver circumcircles reach far
  beyond any fixed multiple of the span), then each chord recovered by Anglada flips. Points on the
  face's own rim are decided by membership of the edge (`rim_of`), not by a predicate on a
  quantity of order `1e-19`, so the two faces sharing an edge cannot decide differently. No flip is
  taken that would lay a triangle along one rim (collinear trace points on an edge lying in an input
  plane otherwise become flat triangles - a12, 2026-10-19). A constraint through a vertex is split
  there first.
* A face a locked curve pierces where one component leaves it two cut nodes (a crease) is fanned
  from the pierce point, so the crease is a mesh edge.
* `FaceTriCache` stores one canonical triangulation per face and each cell re-winds it outward from
  itself. It has run with zero conflicts on every case since it became authoritative.

## Every cell takes one of three paths

1. **§6's table** when one component crosses the cell's edges once each: a frozen case table splits
   the cell along the plane through its cut nodes. It is exact, fast and covers most cut cells. The
   table's answer is checked against an interior sample of each child; a disagreement (a convex
   edge the plane chamfers) escalates the cell.
2. **§7.4, the local PLC mesher**, for every cell the table cannot express (two components, a
   multiply-crossed edge, a crease, a trace that changed one of its faces). The cell's boundary is
   the four face triangulations above, frozen. Its constraints are the surface polygons clipped to
   the tet (`fragment_facets_in_cell`), with their rim vertices adopted from what the faces and edges
   already decided. `constrained_tets` builds a Delaunay tetrahedralisation of the boundary and
   facet nodes, recovers the hull by flips and edge removals (lexicographic progress measure), then
   each facet, adding Steiner points **on the constraints, never on a cell face** when flips stall.
   A facet that lies on a domain-box plane is not handed to the kernel - that face has no neighbour
   and its triangulation already carries it. Every refusal is named; `[PLC-PASS]` prints the counts.
3. **Fallbacks when §7.4 declines.** The **facet-split fan** splits the frozen boundary soup by each
   facet's cap, closes the pieces and fills each one: from its centroid if it is star-shaped, from a
   point of its kernel found by a small search, by hull carving if the kernel is empty, or by
   tetrahedralising the piece itself. It keeps every boundary triangle, so it is conforming and
   keeps the surface as the shared face. If that is refused too, §7.4 runs once more allowed to
   remove the slab between a bent facet's two triangulations (a facet is planar only to its own
   trace snap), and keeps the result only if every node on a component's surface - recorded or a
   facet vertex - still has a tet of that component (`rescue_keeps_bodies`). Only then does the cell
   take the **whole-cell fan** (cone the boundary to its centroid, label each tet by a sample). The
   whole-cell fan abandons conformity to the interface by construction and is slated for deletion
   (plan M-2.3) once nothing reaches it.

## Which side a node is on

The facet-split fan and §7.4's labelling ask, per component, which side of the surface a node lies
on. Three rules keep that from being a coin toss:

* A node the cut **recorded** as on a component's surface (a crossing, an on-cut corner, a trace
  point on its facet or its face trace, or any node a trace endpoint of that component landed on -
  new or welded onto an existing node) has no side of it.
* A node inside a solid of **strictly higher precedence** than the component has no side of it:
  S6 deactivated that part of the surface, so no trace exists there to split by
  (`PointClassifier::hidden_by_precedence`; a3r's 74 whole-cell fans, 2026-10-18).
* Otherwise the classifier answers, and an uncertain predicate answer is treated as no side, not as
  a guess.

A node merely *near* the surface (within `eps`) is not on it - treating it so was measured and
refuted (a3r and a6a lost `[V5]`).

## Labels: settled, never inherited

A piece the cut creates has no S6 answer. §7.5 settles it by an interior sample: per §7.4 sub-region
at its largest tet, per facet-split piece, per table child where another component's surface runs
through the cell's vertices. A record entry S6 left `Ambiguous` is never carried into the output.
For the meshed cells the samples are computed in parallel before the serial assembly
(`region_records`).

## What the cut declares

* **Interfaces**: every face between two different region sets is declared with its side elements
  `(elem+, elem-)` and a per-member orientation (`derive_interface`); a cap that separates nothing is
  dropped rather than emitted with a side of `-1`.
* **Curves**: every mesh edge lying on a locked curve, endpoints and midpoint, once per edge, with the
  curve's components (read by `[V9]`).
* **Contacts**: faces lying on a coincident arranged patch carry every component of it.

## Repairs and determinism

After assembly a T-junction pass finds nodes lying on a face they are not a vertex of (with `[V3]`'s
own test) and replaces the flat sliver and its neighbour by three tets, to a fixed point that stops on
a repeated state. Unreferenced nodes are compacted away.

Per-cell work runs in parallel into a buffer indexed by cell; everything order-dependent (node
interning, face Steiner points, the assembly) runs serially in cell order. The output is
byte-identical at any thread count (R-P2), checked on the matrix with `check_determinism.py`.

## Diagnostics

All print-only. `RUSTMSPT_TIME_STAGES=1` prints `[S8-TIME]` laps with the resident set;
`RUSTMSPT_PLC_DIAG=1` runs the per-path censuses (leaks, T-junctions, `[V6]` steps, off-surface area
by path, degenerate elements, `[TJ-NODE]` locations); `RUSTMSPT_SPLIT_DIAG=1` names the facet-split
fan's refusal per cell; `RUSTMSPT_PLC_CELL=<i>` dumps one cell's PLC attempt;
`RUSTMSPT_FACE_PROBE=a,b,c` prints one lattice face's points, edge points, inputs and triangulation;
`RUSTMSPT_TRACE_CELL=<i>` traces one cell's table decision; `RUSTMSPT_CUT_DIAG=1` adds the debug cell
arrays (`parent_cell`, `plc_path`, `escalation_reason`).
