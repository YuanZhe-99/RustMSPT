# Closest Point on Triangles or Segments

The `closest` subcommand answers, for many query points at once, which
primitive of a set of triangles (a surface) or of segments (a set of lines) is
nearest, the closest point on it, and the distance. A caller that repairs a
tetrahedral mesh against its input surfaces asks this for every node or every
cell centre of a mesh of a million cells, many times over (simHelper's
`meshing/nearest.py`, used by its closure repairs); one call here replaces what
VTK's `find_closest_cell` or an exhaustive numpy search did there.

## The rule

For each query point `p`:

1. The exact `f64` distance from `p` to every primitive is the distance to its
   closest point: on a triangle by Voronoi regions (Ericson, *Real-Time
   Collision Detection* 5.1.5), on a segment by clamped projection. A triangle
   with no area gives the nearest of its three edges.
2. The least such distance wins. Primitives within `DISTANCE_TIE` = `1e-12`
   (relative) of it tie, and the lowest primitive index among them wins.
3. The result is that primitive, its closest point to `p` and the distance.
   An empty primitive set gives `-1`, `NaN` and `NaN`.

The tie band is the point of rule 2. A point nearest an edge two triangles
share, or a vertex several share, is equally far from all of them; without a
band the last bit of each distance would choose, and the choice matters to a
caller that reads something per primitive (the normal of the triangle found).
VTK keeps no stated rule for that case, so its primitive index and this one
differ there, while the distance and the closest point agree to rounding.

## The search

`PrimBvh` is a static hierarchy over each primitive's exact axis-aligned box,
built as `TetBvh` is (`state-map`): median split on the longest axis of the
box centres, ordered by centre then index, leaves of at most four primitives.
A query is branch and bound in two passes: the first finds the least distance,
nearer child first, skipping any box farther than the best so far; the second
visits only boxes within the tie band and keeps the lowest index. A box's
distance never exceeds the distance to anything inside it, so no pass can skip
the answer.

The first pass may start from a **seed**: a primitive whose exact distance
bounds the search from the start. A seed only prunes; the bound is a real
distance, so it is never below the least one, and the answer is the same with
any seed or none.

## Parallelism and the GPU

Queries are independent: the CPU path is a rayon parallel map over them inside
the run's own pool (`cpu_max`). The answer does not depend on the worker count.

With the `gpu` feature, `GpuClosestSeedPipeline` walks the same hierarchy on
the GPU in `f32` and records, for every point, the primitive it found nearest.
Coordinates are taken relative to the centre of the root box in `f64` before
they are rounded to `f32`, so a mesh far from the origin keeps its precision.
That primitive becomes the point's seed on the host, which then runs the exact
search above. Because the seed's distance is recomputed in `f64` and only
bounds the search, `f32` rounding can never change an answer: CPU and GPU runs
give identical primitives, points and distances. The batch size follows the
device's storage-binding limit.

`acceleration.mode` (or `RUSTMSPT_ACCELERATION`) picks the backend as for
`state-map`: `cpu`; `gpu`, which errors on a GPU failure unless
`acceleration.cpu_fallback`; and `auto`, which tries the GPU and falls back to
the CPU. The report names the backend, the adapter and any fallback reason.

## Inputs and outputs

See [`examples/closest.md`](../examples/closest.md) for a run and
[`reference/pipeline-closest.md`](../reference/pipeline-closest.md) for the
functions. Arrays travel as `.npy` files (C order, little endian): `vertices`
`(n, 3)` f64, exactly one of `triangles` `(m, 3)` or `segments` `(m, 2)`
integer corner indices, and `queries` `(..., 3)` f64. The outputs are
`primitive.npy` (int64), `closest.npy` (`(..., 3)` f64) and `distance.npy`
(f64), shaped as the query's leading axes, and `report.json`
(`method_version` `closest-point-v1`, the tool's build identity, the rule, the
tie band, the kind, the vertex, primitive and query counts, the backend, the
worker count, the hierarchy's depth and wall times for read, build, seed,
search and write).
