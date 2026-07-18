# Geometry Analysis Reference

This page documents `src/geometry/metrics.rs` (mesh manifold validation and volume/surface-derived shape metrics — a new module supporting target-diameter-distribution packing) and `src/geometry/s2.rs` (the two-point correlation function `S2(r)` engine: ray-casting point-in-mesh tests, voxelization, exact FFT/direct correlation, Monte Carlo estimation, and their GPU-accelerated counterparts).

## Index

| Function | Location | Summary |
|---|---|---|
| `MeshMetrics` | `src/geometry/metrics.rs:8` | Struct holding volume, surface area, equivalent diameter, and sphericity. |
| `mesh_is_closed` | `src/geometry/metrics.rs:20` | Validates that a mesh is a manifold, consistently-oriented, nonzero-volume shell (or set of shells). |
| `mesh_metrics` | `src/geometry/metrics.rs:105` | Computes volume, surface area, equivalent diameter, and sphericity for a closed mesh. |
| `scale_mesh_to_equivalent_diameter` | `src/geometry/metrics.rs:138` | Rescales a mesh in place so its equivalent-volume diameter matches a target. |
| `RAY_DIR_GPU` | `src/geometry/s2.rs:10` | Fixed non-axis-aligned unit ray direction constant, shared with the GPU ray-casting kernels. |
| `index_3d_to_flat` | `src/geometry/s2.rs:13` | Converts a 3D voxel index to a flat array index (y/z-major strides). |
| `ray_intersects_triangle` | `src/geometry/s2.rs:23` | Möller–Trumbore ray-triangle intersection test. |
| `point_inside_mesh` | `src/geometry/s2.rs:60` | Ray-casting point-in-mesh containment test (odd-hit rule). |
| `build_bbox_occupancy` | `src/geometry/s2.rs:109` | Parallel voxelization of a mesh into a boolean occupancy grid. |
| `shell_offsets_for_distance` | `src/geometry/s2.rs:181` | Enumerates integer voxel offsets lying within a spherical shell annulus. |
| `fill_missing_s2_with_smooth_interpolation` | `src/geometry/s2.rs:214` | Fills unsupported S2 radii via linear or cubic-spline interpolation. |
| `fft_index_3d` | `src/geometry/s2.rs:325` | Converts a 3D FFT-grid index to a flat index (identical logic to `index_3d_to_flat`). |
| `fft_3d_in_place` | `src/geometry/s2.rs:335` | Separable 3D FFT/IFFT performed in place on a complex buffer. |
| `autocorrelation_counts_fft` | `src/geometry/s2.rs:409` | Computes occupancy autocorrelation counts via FFT convolution. |
| `calculate_s2_exact_direct` | `src/geometry/s2.rs:441` | Exact S2 by direct pair enumeration per shell offset (no FFT). |
| `calculate_s2_exact_fft` | `src/geometry/s2.rs:531` | Exact S2 using FFT-based autocorrelation. |
| `calculate_s2_monte_carlo_mesh` | `src/geometry/s2.rs:610` | Monte Carlo S2 estimation sampling directly on the mesh (no voxelization). |
| `calculate_s2` | `src/geometry/s2.rs:685` | Top-level S2 dispatcher; routes to exact (FFT or direct) or voxelized Monte Carlo. |
| `approximate_s2` | `src/geometry/s2.rs:801` | Convenience wrapper for Monte Carlo S2 estimation with a default voxel pitch. |
| `l2_norm` | `src/geometry/s2.rs:806` | Euclidean distance between two S2 vectors over their common-length prefix. |
| `calculate_s2_with_gpu` | `src/geometry/s2.rs:825` | GPU-accelerated S2 for Monte Carlo/"both" methods, with CPU fallback. *(feature `gpu`)* |
| `calculate_s2_gpu_exact` | `src/geometry/s2.rs:855` | GPU-accelerated exact S2 (GPU voxelization + GPU shell pair counting). *(feature `gpu`)* |

---

## src/geometry/metrics.rs

This module computes shape metrics for closed meshes — volume, surface area, equivalent-volume diameter, and sphericity — and provides the manifold-validation gate (`mesh_is_closed`) that other metric and packing code relies on before trusting a mesh's volume calculation. It is a newly added module; `mesh_metrics` and `scale_mesh_to_equivalent_diameter` together form the metric engine that lets the packing pipeline place particles to hit a target equivalent-diameter distribution rather than a raw scale factor.

> **Algorithm:** See [Packing to a target diameter distribution](../algorithms/packing-target-diameter-distribution.md) for the end-to-end algorithm that consumes `MeshMetrics` and `scale_mesh_to_equivalent_diameter`.

### Public API

#### MeshMetrics

- **Kind:** `pub struct MeshMetrics` (derives `Debug, Clone, Copy, PartialEq`)
- **Source:** `src/geometry/metrics.rs:7-13`
- **Purpose:** Bundles the four derived shape metrics produced by `mesh_metrics` for a closed mesh.

| Field | Type | Meaning |
|---|---|---|
| `volume` | `f64` | Enclosed volume of the mesh (signed-volume-of-tetrahedra sum, guaranteed positive after validation). |
| `surface_area` | `f64` | Total surface area (sum of triangle areas). |
| `equivalent_diameter` | `f64` | Diameter of a sphere with the same volume: `(6V/π)^(1/3)`. |
| `sphericity` | `f64` | Wadell sphericity: ratio of the surface area of the volume-equivalent sphere to the mesh's actual surface area, `π^(1/3)(6V)^(2/3) / A`. A value of `1.0` indicates a perfect sphere; smaller values indicate more surface area relative to volume (more irregular shapes). |

- **Notes:** All four fields are guaranteed finite and positive when returned by `mesh_metrics` — invalid combinations produce `None` instead of a `MeshMetrics` with bad data.

#### mesh_metrics

- **Signature:** `pub fn mesh_metrics(mesh: &Mesh) -> Option<MeshMetrics>`
- **Source:** `src/geometry/metrics.rs:105`
- **Purpose:** Computes volume, surface area, equivalent-volume diameter, and sphericity for a mesh in one pass, after confirming the mesh is a valid closed manifold.
- **Parameters:**
  - `mesh` — the candidate mesh to measure.
- **Returns:** `Some(MeshMetrics)` when the mesh passes `mesh_is_closed` and both `volume` (via `mesh_volume`) and `surface_area` (via `mesh_surface_area`) are finite and strictly positive, and the derived `equivalent_diameter`/`sphericity` are also finite and strictly positive; `None` otherwise.
- **Side effects:** None.
- **Notes:** `equivalent_diameter = (6V/π)^(1/3)`; `sphericity = π^(1/3)·(6V)^(2/3) / A`. Delegates the actual volume and area computation to `mesh_volume` (`src/geometry/volume.rs`) and `mesh_surface_area` (`src/geometry/mesh_ops.rs`) — this function only adds the manifold gate and the two derived quantities.
- **See also:** `mesh_is_closed` (the validation gate this function calls first), `scale_mesh_to_equivalent_diameter` (typical downstream consumer of the returned `MeshMetrics`).

#### scale_mesh_to_equivalent_diameter

- **Signature:** `pub fn scale_mesh_to_equivalent_diameter(mesh: &mut Mesh, metrics: MeshMetrics, target_diameter: f64) -> Option<f64>`
- **Source:** `src/geometry/metrics.rs:138`
- **Purpose:** Uniformly rescales a mesh in place so that its equivalent-volume diameter reaches a requested target.
- **Parameters:**
  - `mesh` — mesh to scale, mutated in place.
  - `metrics` — previously computed `MeshMetrics` for `mesh` (supplies the current `equivalent_diameter` used as the scaling baseline).
  - `target_diameter` — desired equivalent diameter; must be finite and strictly positive.
- **Returns:** `Some(factor)` — the validated scale factor applied (`target_diameter / metrics.equivalent_diameter`) — after scaling every vertex; `None` if `target_diameter` is non-finite/non-positive, or if the computed factor is non-finite or non-positive (which can only happen if `metrics.equivalent_diameter` itself is invalid).
- **Side effects:** Mutates `mesh.vertices` in place, scaling each vertex about the origin by `factor` (via `Vec3::scale`).
- **Notes:** Does not recompute or re-validate `metrics` against the current state of `mesh` — callers must supply metrics that correspond to the mesh's pre-scale geometry. Because scaling is about the origin (not the mesh centroid), callers that care about position should re-center the mesh separately if needed.
- **See also:** `mesh_metrics` (supplies the `MeshMetrics` input).

### Private helpers

#### mesh_is_closed

- **Signature:** `fn mesh_is_closed(mesh: &Mesh) -> bool`
- **Source:** `src/geometry/metrics.rs:20`
- **Purpose:** Full manifold-validation gate: determines whether a mesh represents one or more consistently-oriented, watertight, nonzero-volume shells suitable for volume/area computation.
- **Parameters:**
  - `mesh` — candidate mesh.
- **Returns:** `true` only if all of the following hold:
  - The mesh is non-empty and every vertex coordinate is finite.
  - Every face references three distinct, in-bounds vertex indices.
  - No two faces share the same (unordered) vertex triple (no duplicate faces).
  - Every edge is shared by exactly two faces, and the two faces traverse that edge in opposite directions (consistent winding — the sum of edge directions across its two incident faces is zero).
  - Each edge-connected shell (found via BFS/DFS over face adjacency through shared edges) has a finite, nonzero signed volume (signed volume accumulated as `Σ a·(b×c)/6` over its faces).
- **Side effects:** None.
- **Notes:** A mesh may contain multiple disjoint shells (e.g. a mesh with an internal cavity, or multiple disconnected mesh fragments); each is validated independently, and *all* must have nonzero finite signed volume for the whole mesh to pass. This is the gate `mesh_metrics` calls before trusting `mesh_volume`/`mesh_surface_area`.
- **See also:** `mesh_metrics` (sole caller).

---

## src/geometry/s2.rs

This is the two-point correlation function (`S2(r)`) engine — the largest and most computationally dense file in the geometry module. `S2(r)` is the probability that two points separated by distance `r`, sampled at random within the packing domain, both fall inside solid material; it's a standard microstructure-characterization statistic used to compare packed structures against target pore/particle distributions (e.g. `data/input/gu2019_fig7b_pore_distribution.csv`). The file provides three computational strategies — exact voxel-grid correlation (via FFT or direct pair enumeration), Monte Carlo sampling directly on mesh geometry, and voxelized Monte Carlo sampling — plus GPU-accelerated variants of the voxelization and shell-counting steps gated behind the `gpu` feature.

> **Algorithm:** See [S2 two-point correlation](../algorithms/s2-two-point-correlation.md) for the full mathematical background and method-selection rationale behind `calculate_s2`, `point_inside_mesh`, and the FFT autocorrelation functions below.

### Public API

#### RAY_DIR_GPU

- **Kind:** `pub const RAY_DIR_GPU: (f64, f64, f64) = (0.9428090415820634, 0.2705980500730985, 0.19611613513818402)`
- **Source:** `src/geometry/s2.rs:10`
- **Purpose:** A fixed, non-axis-aligned unit-length ray direction used for ray-casting point-in-mesh tests. Using a direction with no zero or repeated components sidesteps degenerate intersections against axis-aligned mesh faces/edges (a common source of false negatives/positives in naive ray-casting containment tests).
- **Notes:** `point_inside_mesh` (below) hardcodes the same numeric literal locally rather than referencing this constant — the two are kept numerically identical but are not structurally linked. This constant is exported for reuse by the GPU voxelization kernels (see `src/gpu/voxel.rs`), where the CPU-side and GPU-side ray-casting must agree on the sampling direction.
- **See also:** `point_inside_mesh`, [GPU reference](gpu.md).

#### point_inside_mesh

- **Signature:** `pub fn point_inside_mesh(mesh: &Mesh, point: Vec3) -> bool`
- **Source:** `src/geometry/s2.rs:60`
- **Purpose:** Determines whether a query point lies inside a closed mesh's volume using ray-casting with the odd-hit (Jordan curve) rule.
- **Parameters:**
  - `mesh` — the mesh to test against.
  - `point` — the query point.
- **Returns:** `true` if the point is inside the mesh (an odd number of unique ray-triangle intersections along the fixed cast direction); `false` if outside, if the mesh has no bounding box (empty mesh), if the point falls outside the mesh's bounding box (with a small `1e-9` tolerance), or if the ray hits no triangles.
- **Side effects:** None.
- **Notes:** Uses the same fixed, non-axis-aligned ray direction as `RAY_DIR_GPU` to avoid axis-aligned degeneracies. Intersection distances (`t` values) within `1e-8` of each other are deduplicated before the odd/even count, which guards against a ray grazing a shared edge between two triangles and being double-counted as two hits. This is an `O(faces)` per-point test with no spatial acceleration structure — it is the hot inner loop of `build_bbox_occupancy` and `calculate_s2_monte_carlo_mesh`, both of which call it per-voxel or per-sample.
- **See also:** `ray_intersects_triangle` (used internally), `build_bbox_occupancy`, `calculate_s2_monte_carlo_mesh`.

#### shell_offsets_for_distance

- **Signature:** `pub fn shell_offsets_for_distance(distance_vox: f64, half_width_vox: f64) -> Vec<[isize; 3]>`
- **Source:** `src/geometry/s2.rs:181`
- **Purpose:** Enumerates all integer voxel-grid offsets `[dx, dy, dz]` whose Euclidean length falls within a spherical annulus `[distance_vox - half_width_vox, distance_vox + half_width_vox)` (clamped at zero on the low end), representing the discrete voxel-grid approximation of "points separated by radius `r`."
- **Parameters:**
  - `distance_vox` — target shell radius, in voxel units.
  - `half_width_vox` — half-width of the annulus (shell thickness), in voxel units — typically half a voxel pitch.
- **Returns:** A `Vec` of integer offset triples on the shell. Returns `[[0, 0, 0]]` when `distance_vox <= 1e-12` (the degenerate `r = 0` case).
- **Side effects:** None.
- **Notes:** Iterates a bounding cube of side `2*lim+1` around the origin (`lim` derived from `distance_vox + half_width_vox`) and tests squared distance against `[low2, high2)`, so cost grows with `r^3` for large radii. Shared by both exact (`calculate_s2_exact_direct`, `calculate_s2_exact_fft`) and Monte Carlo (`calculate_s2`'s voxelized branch, `calculate_s2_gpu_exact`) S2 code paths.
- **See also:** `calculate_s2_exact_direct`, `calculate_s2_exact_fft`, `calculate_s2`, `calculate_s2_gpu_exact`.

#### calculate_s2

- **Signature:** `pub fn calculate_s2(mesh: &Mesh, bbox: BoundingBox, r_max: usize, voxel_pitch: f64, method: &str, samples: usize) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:685`
- **Purpose:** Top-level dispatcher for two-point correlation S2 computation; the primary entry point used by the rest of the codebase to obtain `S2(r)` for `r = 0..=r_max`.
- **Parameters:**
  - `mesh` — the packed/target mesh geometry to characterize.
  - `bbox` — domain bounding box that defines the voxelization/sampling region.
  - `r_max` — maximum radius (in the same length units as `bbox`) to compute, inclusive.
  - `voxel_pitch` — voxel edge length. Required (`> 0`) for `method == "exact"`; ignored (routes to mesh-level Monte Carlo instead) when `<= 0` and `method != "exact"`.
  - `method` — `"exact"` for exact voxel-grid correlation; any other value (conventionally `"monte_carlo"`) for voxelized Monte Carlo sampling.
  - `samples` — number of Monte Carlo samples per radius (used only by non-exact methods; floored at 200 internally).
- **Returns:** `Vec<f64>` of length `r_max + 1`, indexed by integer radius, with `S2(0)` always equal to the volume fraction of `mesh` within `bbox`.
- **Side effects:** Performs parallel computation via `rayon` (voxelization and per-radius work). Prints a `[Warning]` to stdout if `method == "exact"` is requested with a non-positive `voxel_pitch` (falls back to `voxel_pitch = 1.0`).
- **Notes — method routing (the core logic of this function):**
  1. **`voxel_pitch <= 0.0` and `method != "exact"`** → routes directly to `calculate_s2_monte_carlo_mesh`, which samples point pairs directly against the mesh's triangles with no voxelization at all (most accurate, but the slowest per-sample since each sample calls `point_inside_mesh` twice against the full triangle list).
  2. **Otherwise**, the mesh is first voxelized once via `build_bbox_occupancy` (shared by both remaining branches) to produce an occupancy grid and volume fraction `vf`. If the grid has zero occupied voxels, returns an all-zero vector immediately.
  3. **`method == "exact"`** → computes the padded FFT grid size `(2nx-1)(2ny-1)(2nz-1)` and compares it against a fixed threshold of **24,000,000 cells** (`max_fft_cells`). If the padded grid would exceed this threshold, falls back to `calculate_s2_exact_direct` (direct O(shell-size × grid-size) pair enumeration, avoiding the large FFT memory allocation); otherwise uses `calculate_s2_exact_fft` (FFT-based autocorrelation, asymptotically faster for large grids).
  4. **Any other `method` value** (the voxelized Monte Carlo path) → for each radius `1..=r_max`, precomputes the shell offsets once via `shell_offsets_for_distance`, then draws `samples.max(200)` random voxel-pair samples per radius (random voxel + random offset from that radius's shell) and estimates `S2(r)` as the hit fraction among in-bounds pairs. Unsupported radii (empty shell, or zero valid pairs) are filled in afterward via `fill_missing_s2_with_smooth_interpolation`.
- **See also:** `calculate_s2_monte_carlo_mesh`, `build_bbox_occupancy`, `calculate_s2_exact_direct`, `calculate_s2_exact_fft`, `fill_missing_s2_with_smooth_interpolation`, `calculate_s2_with_gpu` (GPU-accelerated wrapper around this function), [GPU reference](gpu.md).

#### approximate_s2

- **Signature:** `pub fn approximate_s2(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:801`
- **Purpose:** Convenience wrapper for voxelized Monte Carlo S2 estimation with a fixed default voxel pitch of `1.0`.
- **Parameters:** Same as the corresponding subset of `calculate_s2`'s parameters (`mesh`, `bbox`, `r_max`, `samples`).
- **Returns:** `Vec<f64>` of S2 values, identical in shape to `calculate_s2`'s return.
- **Side effects:** None beyond what `calculate_s2` performs (parallel computation via rayon).
- **Notes:** Equivalent to `calculate_s2(mesh, bbox, r_max, 1.0, "monte_carlo", samples)`. The voxel pitch of `1.0` is not adaptive to the mesh's scale — callers with meshes at a different characteristic length should call `calculate_s2` directly with an appropriate pitch instead of this wrapper.
- **See also:** `calculate_s2`.

#### l2_norm

`#### l2_norm` — `pub fn l2_norm(a: &[f64], b: &[f64]) -> f64` — `src/geometry/s2.rs:806`. Euclidean distance between two S2 vectors over their common-length prefix (`n = min(a.len(), b.len())`); returns `0.0` if either slice is empty. No side effects. Used to score how closely a computed/simulated `S2(r)` curve matches a target curve (e.g. from `data/input/gu2019_fig7b_pore_distribution.csv`).

#### calculate_s2_with_gpu

> **Feature-gated:** compiled only with `--features gpu`. CPU fallback: `calculate_s2`.

- **Signature:** `pub fn calculate_s2_with_gpu(mesh: &Mesh, bbox: BoundingBox, r_max: usize, voxel_pitch: f64, method: &str, samples: usize, gpu_pipeline: Option<&mut crate::gpu::s2::GpuS2Pipeline>) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:825`
- **Purpose:** GPU-accelerated bridge for the Monte Carlo S2 path; dispatches to a `GpuS2Pipeline` when available and applicable, otherwise defers entirely to the CPU `calculate_s2`.
- **Parameters:**
  - `mesh`, `bbox`, `r_max`, `voxel_pitch`, `method`, `samples` — same as `calculate_s2`.
  - `gpu_pipeline` — optional mutable handle to a pre-initialized `crate::gpu::s2::GpuS2Pipeline` (see `src/gpu/s2.rs`).
- **Returns:** `Vec<f64>` of S2 values, same shape as `calculate_s2`.
- **Side effects:** If `gpu_pipeline` is `Some` and `method` is `"monte_carlo"` or `"both"`, dispatches GPU compute work (`gpu.calculate_s2_gpu`) and prints an `[Info]` timing line to stdout. Otherwise falls through to `calculate_s2` (CPU path), with no GPU side effects.
- **Notes:** `r=0` is always overwritten with the CPU-computed volume fraction (`volume_fraction_in_bbox`) regardless of which path ran, so the GPU kernel itself does not need to compute the exact `r=0` value. When `method == "exact"` or `gpu_pipeline` is `None`, this function is a pure pass-through to `calculate_s2` — it does not attempt a GPU exact path (that is `calculate_s2_gpu_exact`, a separate function).
- **See also:** `calculate_s2` (CPU fallback and the function this wraps), `calculate_s2_gpu_exact` (the GPU exact-method counterpart), [GPU reference](gpu.md).

#### calculate_s2_gpu_exact

> **Feature-gated:** compiled only with `--features gpu`. CPU fallback: `calculate_s2` (called with `method = "exact"` if either GPU pipeline fails to initialize).

- **Signature:** `pub fn calculate_s2_gpu_exact(mesh: &Mesh, bbox: BoundingBox, r_max: usize, voxel_pitch: f64) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:855`
- **Purpose:** Computes exact S2 entirely on GPU: GPU voxelization followed by GPU shell-offset pair counting.
- **Parameters:**
  - `mesh`, `bbox`, `r_max`, `voxel_pitch` — same meaning as `calculate_s2`, but there is no `method`/`samples` parameter since this function is always the exact method.
- **Returns:** `Vec<f64>` of S2 values for `r = 0..=r_max`.
- **Side effects:** Initializes two GPU pipelines in sequence — `crate::gpu::voxel::GpuVoxelPipeline` (f32 ray-cast voxelization) and `crate::gpu::s2_shell::GpuShellS2Pipeline` (u32 shell pair counting) — and dispatches compute work on each. Prints `[Warning]` on either pipeline's init failure (falling back to CPU `calculate_s2`) and a final `[Info]` timing/summary line on success.
- **Notes:** Precomputes all shell offsets for every radius `1..=r_max` up front (`all_offsets`, tagged with their radius) and hands the whole batch to the shell-counting GPU kernel in one dispatch, rather than one dispatch per radius as the CPU exact paths effectively do. After the GPU shell pass, still runs `fill_missing_s2_with_smooth_interpolation` on the CPU to patch any radii with empty shells. Uses `f32` precision for the voxelization stage (GPU-side) but `f64` for shell-offset geometry and interpolation (CPU-side) — a precision boundary worth noting when comparing GPU-exact results against `calculate_s2_exact_fft`/`calculate_s2_exact_direct` bit-for-bit.
- **See also:** `calculate_s2` (CPU exact fallback), `calculate_s2_with_gpu` (the Monte Carlo GPU counterpart), `shell_offsets_for_distance`, `fill_missing_s2_with_smooth_interpolation`, [GPU reference](gpu.md).

### Private helpers

#### index_3d_to_flat

`#### index_3d_to_flat` — `fn index_3d_to_flat(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize` — `src/geometry/s2.rs:13`. Converts a 3D voxel index to a flat array index via `x*ny*nz + y*nz + z` (z fastest-varying). No side effects. Used throughout the occupancy-grid code paths (`build_bbox_occupancy`, `calculate_s2_exact_direct`, `calculate_s2`'s voxelized Monte Carlo branch).

#### ray_intersects_triangle

- **Signature:** `fn ray_intersects_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64>`
- **Source:** `src/geometry/s2.rs:23`
- **Purpose:** Möller–Trumbore ray-triangle intersection test.
- **Parameters:**
  - `origin`, `dir` — ray origin and direction (`dir` need not be normalized; returned `t` scales with `dir`'s magnitude).
  - `a`, `b`, `c` — triangle vertices.
- **Returns:** `Some(t)` — the ray parameter at the intersection point (`origin + t*dir`) — for hits in front of the origin (`t > eps`); `None` for misses, near-parallel rays (`|det| <= eps`), or intersections behind the origin.
- **Side effects:** None.
- **Notes:** Uses a barycentric-tolerance epsilon of `1e-10` on `u`/`v`/`det`. Only forward (`t > eps`) intersections are returned — this is a ray test, not a line test, which matters for `point_inside_mesh`'s odd-hit counting (backward hits must not count).
- **See also:** `point_inside_mesh` (sole caller).

#### build_bbox_occupancy

- **Signature:** `fn build_bbox_occupancy(mesh: &Mesh, bbox: BoundingBox, voxel_pitch: f64) -> (Vec<bool>, [usize; 3])`
- **Source:** `src/geometry/s2.rs:109`
- **Purpose:** Voxelizes a mesh inside a bounding box into a boolean occupancy grid by testing each voxel center for containment.
- **Parameters:**
  - `mesh` — mesh to voxelize.
  - `bbox` — voxelization domain.
  - `voxel_pitch` — voxel edge length.
- **Returns:** `(occ, [nx, ny, nz])` — a flat `Vec<bool>` occupancy grid (indexed via `index_3d_to_flat`/`y*nz+z` layout) and the grid dimensions. Grid dimensions are computed as `ceil(size/voxel_pitch)`, each clamped to at least 1.
- **Side effects:** None (pure computation), but internally parallelizes over x-slabs via `rayon`'s `par_chunks_mut`.
- **Notes:** First splits the mesh into connected components via `split_mesh_into_granules` (falling back to the whole mesh as a single "part" if splitting yields nothing) and computes each granule's local voxel-index bounding range, so per-voxel containment tests (`point_inside_mesh`) only run against the relevant granule's triangles rather than the whole mesh — an important optimization when packing many small particles into one domain. Skips voxels already marked occupied by an earlier granule in the same slab.
- **See also:** `point_inside_mesh` (per-voxel test), `split_mesh_into_granules` (`src/geometry/mesh_ops.rs`), `mesh_bbox` (`src/geometry/bbox.rs`), `calculate_s2` (sole caller, via the exact and voxelized-MC branches).

#### fill_missing_s2_with_smooth_interpolation

- **Signature:** `fn fill_missing_s2_with_smooth_interpolation(values: &mut [f64], has_support: &[bool], vf: f64)`
- **Source:** `src/geometry/s2.rs:214`
- **Purpose:** Fills in S2 values at radii that had no valid sample support (e.g. an empty shell, or all pairs out-of-bounds) using smooth interpolation from the neighboring supported radii.
- **Parameters:**
  - `values` — S2 values array, mutated in place; unsupported entries are overwritten.
  - `has_support` — parallel boolean array; `true` marks radii with a directly computed (trustworthy) value.
  - `vf` — the volume fraction, i.e. the true `S2(0)` value.
- **Returns:** Nothing — mutates `values` in place.
- **Side effects:** Mutates the `values` slice. Always sets `values[0] = vf` regardless of `has_support[0]`.
- **Notes:** Returns early (no-op beyond setting `values[0]`) if `values`/`has_support` length-mismatch, if fewer than 2 knots are known, or if the two known radii are adjacent (nothing to interpolate). With exactly two supported knots, uses smoothstep-blended linear interpolation (`t*t*(3-2t)`); with three or more, fits a natural-ish cubic spline via a tridiagonal (Thomas-algorithm-style) solve over second derivatives, with small epsilon guards (`1e-12`) against near-zero pivots. All interpolated output values are clamped to `[0, 1]`, since S2 is a probability.
- **See also:** Called by `calculate_s2_exact_direct`, `calculate_s2_exact_fft`, `calculate_s2` (voxelized MC branch), and `calculate_s2_gpu_exact`.

#### fft_index_3d

`#### fft_index_3d` — `fn fft_index_3d(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize` — `src/geometry/s2.rs:325`. Converts a 3D FFT-grid index to a flat index using the same `x*ny*nz + y*nz + z` layout as `index_3d_to_flat`. No side effects.

> **Doc note:** this is a byte-for-byte duplicate of `index_3d_to_flat` (line 13) under a different name, scoped to the FFT-grid code paths (`autocorrelation_counts_fft`, `fft_3d_in_place`). Not a bug, but worth knowing the two functions are interchangeable — a future cleanup could unify them.

#### fft_3d_in_place

- **Signature:** `fn fft_3d_in_place(data: &mut [Complex<f64>], nx: usize, ny: usize, nz: usize, inverse: bool)`
- **Source:** `src/geometry/s2.rs:335`
- **Purpose:** Performs a separable 3D FFT (or inverse FFT) in place on a complex data buffer, one axis at a time (z, then y, then x).
- **Parameters:**
  - `data` — flat complex buffer of length `nx*ny*nz`, mutated in place.
  - `nx`, `ny`, `nz` — grid dimensions.
  - `inverse` — `false` for forward FFT, `true` for inverse FFT (with `1/N` normalization applied at the end).
- **Returns:** Nothing — mutates `data` in place.
- **Side effects:** Mutates `data`. Allocates temporary buffers (`tmp_y` per y-slab thread, and a full `nx * ny * nz`-sized transpose buffer `buf` for the x-axis pass).
- **Notes:** The z-axis pass operates directly on contiguous `nz`-length chunks of `data` (cheap, no transpose needed). The y-axis pass processes each x-slab with a small per-iteration `ny`-length scratch buffer, gathering/scattering with stride `nz`. The x-axis pass requires an explicit transpose into `buf` (since x is the slowest-varying axis and not contiguous) before applying the per-line FFT, then transposes back into `data`. Each of the three passes creates its own `FftPlanner` instance rather than sharing one, because `rustfft`'s `Fft` trait object is not `Clone` and per-thread planners are needed for the parallel `par_chunks_mut` calls.
- **See also:** `autocorrelation_counts_fft` (sole caller).

#### autocorrelation_counts_fft

- **Signature:** `fn autocorrelation_counts_fft(occ: &[bool], nx: usize, ny: usize, nz: usize) -> (Vec<f64>, [usize; 3])`
- **Source:** `src/geometry/s2.rs:409`
- **Purpose:** Computes the full occupancy autocorrelation count grid via the FFT convolution theorem (forward FFT → power spectrum → inverse FFT), giving the count of occupied-occupied voxel pairs for every possible integer offset in one pass.
- **Parameters:**
  - `occ` — flat boolean occupancy grid.
  - `nx`, `ny`, `nz` — grid dimensions.
- **Returns:** `(corr, [fx, fy, fz])` — a flat `f64` correlation-count grid over the padded FFT dimensions, and those padded dimensions themselves.
- **Side effects:** None (allocates and returns new buffers; does not mutate `occ`).
- **Notes:** Pads each axis to `2N-1` (`fx = 2*nx-1`, etc.) before transforming, to avoid circular-convolution wraparound artifacts that would corrupt correlation counts near the grid boundary. After the round-trip FFT → conjugate-square → IFFT, takes the real part and clamps to `>= 0.0` (autocorrelation counts should be non-negative; clamping guards against tiny negative floating-point noise). Negative offsets are recovered from the padded grid via wraparound indexing in the caller (`calculate_s2_exact_fft`'s `get_corr` closure).
- **See also:** `fft_3d_in_place`, `calculate_s2_exact_fft` (sole caller).

#### calculate_s2_exact_direct

- **Signature:** `fn calculate_s2_exact_direct(occ: &[bool], nx: usize, ny: usize, nz: usize, r_max: usize, voxel_pitch: f64, vf: f64) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:441`
- **Purpose:** Computes exact S2 by directly enumerating and counting voxel pairs for each shell offset at each radius — no FFT, used when the padded FFT grid would be too large to allocate/process efficiently.
- **Parameters:**
  - `occ`, `nx`, `ny`, `nz` — occupancy grid and its dimensions.
  - `r_max` — maximum radius (voxel-grid distance units matching `voxel_pitch`'s scale, but the loop is `0..=r_max` as an integer count of physical-length steps).
  - `voxel_pitch` — voxel edge length, used to convert physical radius `r` to voxel-space `r_vox = r / voxel_pitch`.
  - `vf` — volume fraction, used directly as `S2(0)`.
- **Returns:** `Vec<f64>` of length `r_max + 1`, S2 values per radius, with unsupported radii filled via `fill_missing_s2_with_smooth_interpolation`.
- **Side effects:** None (pure computation); parallelizes the outer radius loop via `rayon`'s `into_par_iter`.
- **Notes:** For each radius, computes shell offsets via `shell_offsets_for_distance`, then for each offset iterates the full valid overlap region of the grid (`x_start..x_end` etc., accounting for the offset's sign) counting `valid_pairs` and `hit_pairs` (both endpoints occupied), and averages the hit fraction across all offsets in the shell. This is `O(shell_size × grid_size)` per radius — more expensive per-radius than the FFT approach for large radii/grids, but avoids the FFT's `O((2n)^3)` memory footprint, which is why `calculate_s2` selects this path when the padded grid exceeds 24M cells.
- **See also:** `shell_offsets_for_distance`, `fill_missing_s2_with_smooth_interpolation`, `calculate_s2` (sole caller, "exact"-large-grid branch), `calculate_s2_exact_fft` (the FFT alternative for smaller grids).

#### calculate_s2_exact_fft

- **Signature:** `fn calculate_s2_exact_fft(occ: &[bool], nx: usize, ny: usize, nz: usize, r_max: usize, voxel_pitch: f64, vf: f64) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:531`
- **Purpose:** Computes exact S2 by first building the full autocorrelation grid once via FFT (`autocorrelation_counts_fft`), then reading off pair counts for each shell offset at each radius from that precomputed grid.
- **Parameters:** Same as `calculate_s2_exact_direct`.
- **Returns:** `Vec<f64>` of length `r_max + 1`, S2 values per radius, with unsupported radii filled via `fill_missing_s2_with_smooth_interpolation`.
- **Side effects:** None (pure computation); parallelizes the outer radius loop via `rayon`.
- **Notes:** Because the correlation grid is computed once up front (amortized `O((2n)^3 log n)` FFT cost) rather than per-offset, this is asymptotically much cheaper than `calculate_s2_exact_direct` for large radius ranges, at the cost of the padded-grid memory allocation — which is exactly the tradeoff `calculate_s2` evaluates via the 24M-cell threshold before choosing this path. Offsets whose absolute value would exceed the grid dimensions (`adx >= nx` etc.) are skipped as unsupported for that shell entry, since no valid pair exists at that offset.
- **See also:** `autocorrelation_counts_fft`, `shell_offsets_for_distance`, `fill_missing_s2_with_smooth_interpolation`, `calculate_s2` (sole caller, "exact"-small-grid branch), `calculate_s2_exact_direct` (the direct-enumeration alternative for large grids).

#### calculate_s2_monte_carlo_mesh

- **Signature:** `fn calculate_s2_monte_carlo_mesh(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:610`
- **Purpose:** Estimates S2 via Monte Carlo sampling of point pairs directly against mesh geometry, with no voxelization step at all.
- **Parameters:**
  - `mesh`, `bbox` — mesh and sampling domain.
  - `r_max` — maximum radius.
  - `samples` — requested samples per radius (floored at 200 via `samples.max(200)`).
- **Returns:** `Vec<f64>` of length `r_max + 1`; `S2(0)` is set to `volume_fraction_in_bbox(mesh, bbox)`.
- **Side effects:** None (pure computation); parallelizes the outer radius loop via `rayon`.
- **Notes:** For each radius `r`, draws `mc_samples` random points `p` uniformly in `bbox`, pairs each with a random unit direction (rejection-sampled from a unit cube, `x²+y²+z² ∈ (1e-12, 1]`), and forms `q = p + dir*r`. Pairs where `q` falls outside `bbox` are discarded entirely (not counted as misses) — so the effective sample count per radius can be lower than `mc_samples`, and shrinks further as `r` approaches the domain size. For surviving pairs, calls `point_inside_mesh` on both `p` and `q` and counts joint-occupancy hits. This is the most geometrically faithful method (no voxel discretization error) but also the slowest, since each sample requires two full `O(faces)` ray-casts against the raw mesh — used when `voxel_pitch <= 0` and `method != "exact"` in `calculate_s2`.
- **See also:** `point_inside_mesh`, `volume_fraction_in_bbox` (`src/geometry/volume.rs`), `calculate_s2` (sole caller, no-voxel-pitch branch).
