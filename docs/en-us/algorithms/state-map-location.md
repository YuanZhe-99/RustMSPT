# State-Map Point Location

The `state-map` subcommand answers the expensive half of carrying a solved
state across a remesh: for every sampling point of every element of a new mesh,
which element of the old (deformed) mesh it sits in — or, for a point just
outside the old mesh, which old element is nearest within a declared bound.
The averaging of the located values, the admissibility gates and the mapping
report stay with the caller (simHelper's `transfer/state_mapper.py`); RustMSPT
returns donors and distances only.

## The rule

For each query point `p`, against the source tetrahedra:

1. **Contained.** If one or more tets contain `p` — every barycentric
   coordinate `>= -1e-12` and their sum `<= 1 + 1e-12` — the donor is the one
   whose centroid is nearest `p` (squared distance), ties to the lower tet
   index. Distance `0`.
2. **Bounded fallback.** Otherwise the donor is the tet at the smallest exact
   point-to-tet distance, if that distance is `<= fallback_distance`. Tets
   within `DISTANCE_TIE` = `1e-12` (relative) of that smallest distance tie,
   and the lowest index among them wins; its own distance is recorded. A point
   outside the mesh is often equally far from several tets — its nearest
   feature is an edge or a corner they share — and without the band the last
   bit of how each distance was computed would choose between them, so two
   correct implementations would disagree.
3. **None.** Otherwise donor `-1`, distance `NaN`. A point beyond the bound is
   not given a value; whether that is acceptable is the caller's decision.

The exact distance is the minimum over the six edge segments and over the four
face feet that land inside their face — the same arithmetic as simHelper's
`_distance_to_tetra`, so the two report the same distance. A centroid is
`((v0+v1)+v2)+v3` over 4, numpy's order; the barycentric coordinates come from
the inverse of the matrix whose columns are `v1-v0`, `v2-v0`, `v3-v0`. A
singular tet never contains a point.

## Why a bounding-volume hierarchy

The first implementation (simHelper's) answered rule 2 with a ball query of
radius `fallback_distance + reach`, where `reach` was the mesh's *largest*
centroid-to-vertex distance. On a graded mesh — 0.5 µm cells at a pore wall
beside 4 µm background cells — one large cell set the radius for every point,
and a 9.65 µm ball near a pore held 30,000–80,000 tets. One remesh of 15.2
million sampling points, 106,818 of them outside the old mesh, spent
2 h 22 min there.

`TetBvh` is a static hierarchy over each tet's own axis-aligned box. Rule 1
descends only into boxes that hold the point. Rule 2 is branch and bound in two
passes: the first finds the nearest distance, skipping any box farther than the
best so far (or than the bound), and the second visits only boxes within the
tie band to pick the lowest index, and each tet is reached through its own box rather than
through a global radius.

Each tet's box is padded by `1e-9` of its largest extent, so it holds every
point the barycentric tolerance admits. The hierarchy splits at the median box
centre along the longest axis, ordering by centre then tet index, with leaves
of at most four tets; the build is deterministic.

## Parallelism and the GPU

Points are independent, so the CPU path is a rayon `par_chunks_exact` over the
query array inside the run's own pool (`cpu_max`). The answer does not depend on
the worker count: each point's donor is chosen by a total order.

With the `gpu` feature, `GpuTetCandidatePipeline` walks the same hierarchy on
the GPU in `f32` and records, per point, every leaf slot whose box holds it (up
to `gpu_slots`, with the full count). The boxes are rounded **outward** to `f32`
and the point to nearest. Rounding is monotone, so a point inside a box in
`f64` is inside the outward-rounded box in `f32`: the GPU's slots are a superset
of the slots the CPU would test, and the host then applies rule 1 to them in
`f64`. A point with more candidates than slots, or with none that contains it,
is settled entirely on the CPU (rule 1 again, then rule 2). So the GPU only
narrows the search; it never decides, and CPU and GPU runs give identical donors
and distances. The batch size follows the device's storage-binding limit.

`acceleration.mode` (or `RUSTMSPT_ACCELERATION`) picks the backend: `cpu`;
`gpu`, which errors on a GPU failure unless `acceleration.cpu_fallback`; and
`auto`, which tries the GPU and falls back to the CPU. The report names the
backend that ran, the adapter, and any fallback reason.

## Inputs and outputs

See [`examples/state-map.md`](../examples/state-map.md) for a run and
[`reference/pipeline-state-map.md`](../reference/pipeline-state-map.md) for the
functions. Arrays travel as `.npy` files (C order, little endian):
`source_points` `(n, 3)` f64, `source_cells` `(m, 4)` integer,
`query_points` `(..., 3)` f64. The outputs are `donors.npy` (int64) and
`distances.npy` (f64), shaped as the query's leading axes, and `report.json`
(`method_version` `state-map-locate-v2`, the tool's build identity, the rule,
the three tolerances, counts of contained, fallback and unlocated points and of GPU
overflows, the backend, the worker count, the hierarchy's depth and node
count, and wall times for read, build, locate and write).
