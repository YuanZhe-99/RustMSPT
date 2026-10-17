# Closest Point — `closest`

Reference for the `closest` subcommand: the primitives, their bounding-volume
hierarchy and the search (`src/geometry/closest.rs`), the GPU seed pass
(`src/gpu/closest_candidates.rs`, `src/gpu/shaders/closest_candidates.wgsl`),
the pipeline (`src/pipeline/closest.rs`) and its configuration
(`src/config/closest.rs`). The `.npy` I/O is `state-map`'s
([`pipeline-state-map.md`](pipeline-state-map.md)). The algorithm is
[`algorithms/closest-point.md`](../algorithms/closest-point.md).

## Index

| Item | Source | Summary |
|---|---|---|
| `DISTANCE_TIE` | `src/geometry/closest.rs:4` | `1e-12`: distances within this fraction of the least tie; the lowest primitive index wins. |
| `Kind` | `src/geometry/closest.rs:10` | `Triangles` or `Segments`. |
| `Kind::corners` | `src/geometry/closest.rs:17` | Corners per primitive: 3 or 2. |
| `Primitives` | `src/geometry/closest.rs:26` | Vertices and corner indices; a segment stored as `[a, b, b]`. |
| `Primitives::new` | `src/geometry/closest.rs:39` | Check and pack the arrays; refuses ragged arrays, non-finite vertices, indices out of range. |
| `Primitives::len` / `is_empty` | `src/geometry/closest.rs:64` | Primitive count. |
| `Primitives::corner_points` | `src/geometry/closest.rs:74` | A primitive's corner coordinates. |
| `Primitives::closest` | `src/geometry/closest.rs:80` | Closest point of one primitive and its exact distance. |
| `closest_on_segment` | `src/geometry/closest.rs:125` | Clamped projection onto a segment. |
| `closest_on_triangle` | `src/geometry/closest.rs:141` | Voronoi-region closest point on a triangle; a triangle with no area gives its nearest edge. |
| `PrimBvh` | `src/geometry/closest.rs:208` | Static hierarchy over exact primitive boxes, laid out so the GPU walks the same arrays. |
| `PrimBvh::build` | `src/geometry/closest.rs:225` | Deterministic median-split build. |
| `PrimBvh::nearest` | `src/geometry/closest.rs:335` | The rule for one point, from an optional seed. |
| `Nearest` | `src/geometry/closest.rs:362` | Primitive (`-1` none), closest point, distance per query. |
| `nearest_all` | `src/geometry/closest.rs:374` | Every query in parallel over the current pool, from optional seeds. |
| `GpuClosestSeedPipeline` | `src/gpu/closest_candidates.rs:8` | The compiled seed kernel on the shared device. |
| `GpuClosestSeedPipeline::new` | `src/gpu/closest_candidates.rs:22` | Compile or reuse the kernel. |
| `GpuClosestSeedPipeline::adapter` | `src/gpu/closest_candidates.rs:90` | Adapter name and backend, for the report. |
| `GpuClosestSeedPipeline::seeds` | `src/gpu/closest_candidates.rs:100` | Per-point nearest primitive in `f32`, batched to device limits. |
| `ClosestParams` | `src/config/closest.rs:11` | The `closest` YAML block. |
| `ClosestConfig` | `src/config/closest.rs:25` | The document: `closest` plus optional `acceleration`. |
| `METHOD_VERSION` | `src/pipeline/closest.rs:10` | `closest-point-v1`, written in every report. |
| `ClosestPipeline` | `src/pipeline/closest.rs:12` | The subcommand's pipeline. |
| `ClosestPipeline::run` | `src/pipeline/closest.rs:32` | Read, build, seed, search, write primitive/closest/distance/report. |

## `src/geometry/closest.rs`

### `Primitives::new(kind: Kind, vertices: &[f64], corners: &[u32]) -> Result<Primitives>`
- **Purpose:** check and pack flat vertex coordinates and corner indices.
- **Parameters:** `kind`; `vertices` flat xyz; `corners` flat, three per triangle or two per segment.
- **Returns:** the primitives, or `InvalidConfig` for a vertex array not a multiple of 3, corners not a multiple of the kind's count, a non-finite vertex or an index at or above the vertex count.
- **Side effects:** none.
- **Notes:** a segment is stored as `[a, b, b]` so both kinds share one layout (the GPU reads nine floats per primitive).

### `Primitives::closest(&self, i: usize, p: &[f64; 3]) -> ([f64; 3], f64)`
- **Purpose:** the closest point of primitive `i` to `p` and the exact distance to it.
- **Returns:** the point and its Euclidean distance from `p`.
- **Side effects:** none.

### `closest_on_segment(p, a, b) -> [f64; 3]` / `closest_on_triangle(p, a, b, c) -> [f64; 3]`
- **Purpose:** the closest point to `p` on a segment (clamped projection; a point segment gives `a`) or on a triangle (Ericson's Voronoi regions).
- **Returns:** the point.
- **Side effects:** none.
- **Notes:** a triangle whose face-region denominator is not positive (no area) gives the nearest of its three edges.

### `PrimBvh::build(prims: &Primitives) -> PrimBvh`
- **Purpose:** the hierarchy over each primitive's exact box.
- **Returns:** nodes (`node_min`, `node_max`, `link`), the primitive `order`, per-slot boxes and the depth. `link[n]` is `[first, count]` for a leaf (slots `first..first+count`) and `[left, 0]` for an internal node whose children are `left` and `left + 1`.
- **Side effects:** none.
- **Notes:** median split on the longest axis of the box centres, ordered by centre then index, leaves of at most four; deterministic.

### `PrimBvh::nearest(&self, prims, p, seed: Option<u32>) -> (u32, [f64; 3], f64)`
- **Purpose:** the module's rule for one point: least exact distance, ties within `DISTANCE_TIE` to the lowest index.
- **Parameters:** `seed` — a primitive whose exact distance bounds the first pass; out of range or `None` starts unbounded.
- **Returns:** primitive, closest point, distance; `u32::MAX`, `NaN`s when there are no primitives.
- **Side effects:** none.
- **Notes:** two branch-and-bound passes (least distance, then lowest index within the band). A seed changes the work, never the answer.

### `nearest_all(prims, bvh, queries: &[f64], seeds: Option<&[u32]>) -> Result<Nearest>`
- **Purpose:** every query point on every core.
- **Parameters:** `queries` flat xyz; `seeds` one per point, `u32::MAX` for none.
- **Returns:** `Nearest` (`primitive` int64 with `-1` for none, `closest` flat xyz, `distance`), or `InvalidConfig` for ragged or non-finite queries or a seed array of the wrong length.
- **Side effects:** none.
- **Notes:** rayon over the current pool; the answer is independent of the worker count and of the seeds.

## `src/gpu/closest_candidates.rs`

### `GpuClosestSeedPipeline::seeds(&self, prims, bvh, points: &[f64]) -> Result<Vec<u32>, String>`
- **Purpose:** for every point, the primitive the GPU finds nearest in `f32`, as a seed for `nearest_all`.
- **Returns:** one index per point (`u32::MAX` when none was found), or the reason it could not run: a hierarchy deeper than the shader's stack (60), buffers beyond the device's limits, a device failure.
- **Side effects:** uploads the node boxes, links, primitive corners (nine `f32` per slot) and order once; then dispatches and reads back one batch of points at a time.
- **Notes:** coordinates are made relative to the root box's centre in `f64` before rounding. Box distances are not rounded outward: the seed only bounds the host's exact search.

## `src/pipeline/closest.rs`

### `ClosestPipeline::run(&self) -> Result<()>`
- **Purpose:** the subcommand.
- **Parameters:** `self.config` — `closest.vertices`, exactly one of `closest.triangles` and `closest.segments`, `closest.queries`, `closest.output_dir`, `closest.cpu_max`, and `acceleration`.
- **Returns:** `Ok`, or an error for an unreadable or ragged array, both or neither of triangles and segments, corners not shaped `(m, 3)` / `(m, 2)`, a GPU failure with fallback forbidden, or an I/O failure.
- **Side effects:** reads three `.npy` files; creates `output_dir` and writes `primitive.npy`, `closest.npy`, `distance.npy` and `report.json`; prints one summary line.

## `src/config/closest.rs`

### `ClosestParams`
- **Fields:** `vertices`, `triangles` (optional), `segments` (optional), `queries`, `output_dir` — paths; `cpu_max` — worker cap (`-1` or absent for every core).
