# State-Map Point Location — `state-map`

Reference for the `state-map` subcommand: the `.npy` I/O (`src/transfer/npy.rs`),
the tet mesh, its bounding-volume hierarchy and the location rule
(`src/transfer/locate.rs`), the GPU candidate pass (`src/gpu/tet_candidates.rs`,
`src/gpu/shaders/tet_candidates.wgsl`), the pipeline (`src/pipeline/state_map.rs`)
and its configuration (`src/config/state_map.rs`). The algorithm, and why it is
built this way, is [`algorithms/state-map-location.md`](../algorithms/state-map-location.md).

## Index

| Item | Source | Summary |
|---|---|---|
| `NpyArray` | `src/transfer/npy.rs:42` | One `.npy` array: shape, stored dtype, raw little-endian payload. |
| `NpyArray::to_f64` | `src/transfer/npy.rs:60` | Elements as f64 (f4 widened exactly); an integer array is refused. |
| `NpyArray::to_index` | `src/transfer/npy.rs:83` | Elements as u32 indices; a float array, a negative value or one above `u32::MAX` is refused. |
| `read_npy` | `src/transfer/npy.rs:128` | Read a version 1/2/3 `.npy` (C order; `<f8 <f4 <i8 <i4 <u4 <u8`). |
| `write_npy_f64` | `src/transfer/npy.rs:213` | Write an f64 array as `.npy` v1.0. |
| `write_npy_i64` | `src/transfer/npy.rs:227` | Write an i64 array as `.npy` v1.0. |
| `BARYCENTRIC_TOLERANCE` | `src/transfer/locate.rs:5` | `1e-12`: how far outside a tet a barycentric coordinate may sit and still count (simHelper's value). |
| `BOX_RELATIVE_PAD` | `src/transfer/locate.rs:6` | `1e-9`: each tet box's padding, as a fraction of its largest extent. |
| `LEAF_SIZE` | `src/transfer/locate.rs:7` | `4`: most tets per hierarchy leaf. |
| `DISTANCE_TIE` | `src/transfer/locate.rs:8` | `1e-12`: fallback distances within this fraction of the nearest tie; the lowest index wins (simHelper's value). |
| `TetMesh` | `src/transfer/locate.rs:11` | Source tets with corners, centroid, inverse edge matrix and invertibility. |
| `TetMesh::new` | `src/transfer/locate.rs:25` | Gather corners and precompute centroids and inverses; refuses ragged arrays and missing points. |
| `TetMesh::contains` | `src/transfer/locate.rs:99` | Barycentric containment within the tolerance; a singular tet contains nothing. |
| `TetMesh::centroid_distance2` | `src/transfer/locate.rs:118` | Squared distance to a tet's centroid (rule 1's tie-break). |
| `TetMesh::distance` | `src/transfer/locate.rs:128` | Exact point-to-tet distance, simHelper's formula. |
| `TetBvh` | `src/transfer/locate.rs:199` | Static hierarchy over padded tet boxes, laid out so the GPU walks the same arrays. |
| `TetBvh::build` | `src/transfer/locate.rs:216` | Deterministic median-split build. |
| `TetBvh::containing` | `src/transfer/locate.rs:288` | Rule 1: the containing tet with the nearest centroid. |
| `TetBvh::nearest` | `src/transfer/locate.rs:317` | Rule 2: the lowest-index tet within the tie band of the nearest distance, inside the bound. |
| `LocateCounts` | `src/transfer/locate.rs:407` | Points, contained, fallback, unlocated, GPU overflow. |
| `Located` | `src/transfer/locate.rs:416` | Donors (`-1` none), distances (`NaN` none), counts. |
| `locate_cpu` | `src/transfer/locate.rs:439` | Every point on the CPU, in parallel over the current pool. |
| `locate_from_candidates` | `src/transfer/locate.rs:454` | Finish from GPU candidate slots; identical to `locate_cpu`. |
| `GpuTetCandidatePipeline` | `src/gpu/tet_candidates.rs:38` | The compiled candidate kernel on the shared device. |
| `GpuTetCandidatePipeline::new` | `src/gpu/tet_candidates.rs:52` | Compile or reuse the kernel. |
| `GpuTetCandidatePipeline::adapter` | `src/gpu/tet_candidates.rs:120` | Adapter name and backend, for the report. |
| `GpuTetCandidatePipeline::candidates` | `src/gpu/tet_candidates.rs:130` | Per-point leaf slots whose outward-rounded box holds the point, batched to device limits. |
| `StateMapParams` | `src/config/state_map.rs:11` | The `state_map` YAML block. |
| `StateMapConfig` | `src/config/state_map.rs:30` | The document: `state_map` plus optional `acceleration`. |
| `METHOD_VERSION` | `src/pipeline/state_map.rs:10` | `state-map-locate-v2` (v2: the fallback tie band), written in every report. |
| `StateMapPipeline` | `src/pipeline/state_map.rs:12` | The subcommand's pipeline. |
| `StateMapPipeline::run` | `src/pipeline/state_map.rs:32` | Read, build, locate, write donors/distances/report. |

## `src/transfer/npy.rs`

### `read_npy(path: &Path) -> Result<NpyArray>`
- **Purpose:** read one numpy array file.
- **Parameters:** `path` — a `.npy` file.
- **Returns:** the array, or `InvalidConfig` for a bad magic, an unsupported version, Fortran order, an unsupported dtype or a payload shorter than its shape; `Io` for a read failure.
- **Side effects:** reads the whole file into memory.
- **Notes:** only the six little-endian numeric dtypes are read; numpy writes these by default on x86 and ARM.

### `NpyArray::to_f64(&self) -> Result<Vec<f64>>` / `NpyArray::to_index(&self) -> Result<Vec<u32>>`
- **Purpose:** the elements in the type the location needs.
- **Returns:** a flat vector in C order; `to_f64` refuses integer arrays, `to_index` refuses float arrays and any value outside `0..=u32::MAX`.
- **Side effects:** none.

### `write_npy_f64(path, shape, values)` / `write_npy_i64(path, shape, values)`
- **Purpose:** write the outputs numpy reads with `np.load`.
- **Returns:** `Ok`, or `InvalidConfig` when the shape's product is not `values.len()`.
- **Side effects:** creates or replaces `path`.

## `src/transfer/locate.rs`

### `TetMesh::new(points: &[f64], cells: &[u32]) -> Result<TetMesh>`
- **Purpose:** prepare a source mesh for location.
- **Parameters:** `points` flat `(n, 3)`; `cells` flat `(m, 4)`.
- **Returns:** the mesh, or `InvalidMesh` for ragged arrays or a cell naming a point that does not exist.
- **Side effects:** none (parallel over the current pool).
- **Notes:** centroids are summed in numpy's order and inverses are of the matrix with columns `v1-v0, v2-v0, v3-v0`, so a point on a shared face is decided as simHelper decides it up to the last bit of the inverse.

### `TetMesh::contains(&self, t, p) -> bool`
- **Purpose:** barycentric containment of `p` in tet `t`.
- **Returns:** whether every coordinate is `>= -BARYCENTRIC_TOLERANCE` and their sum `<= 1 + BARYCENTRIC_TOLERANCE`; `false` for a singular tet.

### `TetMesh::distance(&self, t, p) -> f64`
- **Purpose:** the exact distance from an outside point to tet `t`.
- **Returns:** the minimum over the six edge segments and the face feet that land inside their face.
- **Notes:** the same arithmetic as simHelper's `_distance_to_tetra`; meaningful for a point outside the tet (rule 2 only asks about such points).

### `TetBvh::build(mesh: &TetMesh) -> TetBvh`
- **Purpose:** the hierarchy rule 1, rule 2 and the GPU walk.
- **Returns:** nodes (`node_min`, `node_max`, `link`), leaf slots (`order`, `slot_min`, `slot_max`) and the depth.
- **Side effects:** none.
- **Notes:** `link[n]` is `[first, count]` for a leaf (`count > 0`) and `[left, 0]` for an internal node whose children are `left` and `left + 1`. A range whose box centres all coincide becomes one leaf whatever its size.

### `TetBvh::containing(&self, mesh, p) -> Option<u32>`
- **Purpose:** rule 1.
- **Returns:** the containing tet with the smallest squared centroid distance, ties to the lower index; `None` if no tet contains `p`.

### `TetBvh::nearest(&self, mesh, p, bound) -> Option<(u32, f64)>`
- **Purpose:** rule 2.
- **Returns:** among the tets whose exact distance is `<= bound` and within `DISTANCE_TIE` (relative) of the smallest such distance, the lowest index, with its own distance; `None` when none is within the bound.
- **Notes:** two branch-and-bound passes over the padded boxes: the first finds the smallest distance, the second visits only boxes within the tie band. The band is what makes simHelper's Python search, which applies the same one, choose the same tet when a point is equally far from several.

### `locate_cpu(mesh, bvh, points, bound) -> Result<Located>`
- **Purpose:** locate every point on the CPU.
- **Parameters:** `points` flat `(k, 3)`; `bound` the fallback distance.
- **Returns:** `Located`, or `InvalidConfig` for a ragged point array or a bound that is negative or not finite.
- **Side effects:** none; runs in the caller's rayon pool.
- **Notes:** the result is independent of the worker count.

### `locate_from_candidates(mesh, bvh, points, bound, counts, slots, k) -> Result<Located>`
- **Purpose:** finish a location from a GPU candidate pass.
- **Parameters:** `counts[i]` the number of leaf slots found for point `i`; `slots[i*k..i*k+min(counts[i],k)]` those slots.
- **Returns:** `Located` equal to `locate_cpu`'s; `counts.gpu_overflow` counts the points whose candidates did not fit.
- **Notes:** correct only when the slots are a superset of the leaf slots whose padded box holds the point, which the outward rounding guarantees.

## `src/gpu/tet_candidates.rs`

### `GpuTetCandidatePipeline::candidates(&self, bvh, points, k) -> Result<(Vec<u32>, Vec<u32>), String>`
- **Purpose:** propose candidates on the GPU.
- **Returns:** `(counts, slots)` as `locate_from_candidates` reads them, or an error for a hierarchy deeper than the shader's stack (60), a box array above the device's storage-binding limit, or a device failure.
- **Side effects:** uploads the hierarchy once, then one batch of points at a time (up to 2^20, fewer if the device's limits require), and reads counts and slots back.
- **Notes:** boxes are rounded outward to f32 and points to nearest, so the result is a superset; the host's f64 test decides.

## `src/pipeline/state_map.rs`, `src/config/state_map.rs`

### `StateMapPipeline::run(&self) -> Result<()>`
- **Purpose:** the `state-map` subcommand.
- **Parameters (YAML):** `state_map.source_points`, `source_cells`, `query_points` (paths), `fallback_distance`, `output_dir`, optional `gpu_slots` (default 24) and `cpu_max` (default all workers); optional `acceleration` block (`mode: auto|cpu|gpu`, `cpu_fallback`).
- **Returns:** `Ok`, or an error for unreadable or ragged inputs, a bad bound, a GPU failure under `mode: gpu` with `cpu_fallback: false`, or an I/O failure.
- **Side effects:** writes `donors.npy`, `distances.npy` and `report.json` under `output_dir`; prints one `[state-map]` summary line.
- **Notes:** `RUSTMSPT_ACCELERATION` overrides the mode, as for every pipeline. Under `auto` a GPU failure falls back to the CPU and the report's `backend.fallback_reason` says why.
