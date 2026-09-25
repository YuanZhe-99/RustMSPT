# Geometry Analysis Reference

This page documents `src/geometry/metrics.rs` (mesh manifold validation and volume/surface-derived shape metrics — a new module supporting target-diameter-distribution packing) and `src/geometry/s2.rs` (the two-point correlation function `S2(r)` engine: ray-casting point-in-mesh tests, voxelization, exact FFT/direct correlation, Monte Carlo estimation, and their GPU-accelerated counterparts).

## Index

| Function | Location | Summary |
|---|---|---|
| `MeshMetrics` | `src/geometry/metrics.rs:8` | Struct holding volume, surface area, equivalent diameter, and sphericity. |
| `mesh_is_closed` | `src/geometry/metrics.rs:23` | Validates that a mesh is a manifold, consistently-oriented, nonzero-volume shell (or set of shells). |
| `mesh_metrics` | `src/geometry/metrics.rs:149` | Computes volume, surface area, equivalent diameter, and sphericity for a closed mesh. |
| `scale_mesh_to_equivalent_diameter` | `src/geometry/metrics.rs:182` | Rescales a mesh in place so its equivalent-volume diameter matches a target. |
| `RAY_DIR_GPU` | `src/geometry/s2.rs:13` | Fixed non-axis-aligned unit ray direction constant, shared with the GPU ray-casting kernels. |
| `index_3d_to_flat` | `src/geometry/s2.rs:16` | Converts a 3D voxel index to a flat array index (y/z-major strides). |
| `ray_intersects_triangle` | `src/geometry/s2.rs:26` | Möller–Trumbore ray-triangle intersection test. |
| `point_inside_mesh` | `src/geometry/s2.rs:63` | Ray-casting point-in-mesh containment test (odd-hit rule). |
| `build_bbox_occupancy` | `src/geometry/s2.rs:112` | Parallel voxelization of a mesh into a boolean occupancy grid. |
| `part_voxel_ranges` | `src/geometry/s2.rs` | Prepared query and clamped voxel range per connected component; the single definition shared by full and incremental voxelization. |
| `particle_voxel_coverage` | `src/geometry/s2.rs` | Voxel indices whose centres lie inside one particle, once per containing component, in component/x/y/z order. |
| `VoxelCoverage` | `src/geometry/s2.rs` | Per-voxel coverage counts (number of components containing the centre) with matching occupancy (count > 0); replace re-queries only the moved particle, restore rolls back without queries. Equals VoxelS2::new on the merged mesh exactly. |
| `shell_offsets_for_distance` | `src/geometry/s2.rs:184` | Enumerates integer voxel offsets lying within a spherical shell annulus. |
| `fill_missing_s2_with_smooth_interpolation` | `src/geometry/s2.rs:217` | Fills unsupported S2 radii via linear or cubic-spline interpolation. |
| `fft_index_3d` | `src/geometry/s2.rs:328` | Converts a 3D FFT-grid index to a flat index (identical logic to `index_3d_to_flat`). |
| `FftWorkspace::transform` | `src/geometry/s2.rs:369` | Separable 3D FFT/IFFT performed in place on a complex buffer. |
| `autocorrelation_counts_fft` | `src/geometry/s2.rs:549` | Computes occupancy autocorrelation counts via FFT convolution. |
| `calculate_s2_exact_direct` | `src/geometry/s2.rs:561` | Exact S2 by direct pair enumeration per shell offset (no FFT). |
| `calculate_s2_exact_fft` | `src/geometry/s2.rs:651` | Exact S2 using FFT-based autocorrelation. |
| `calculate_s2_monte_carlo_mesh` | `src/geometry/s2.rs:730` | Monte Carlo S2 estimation sampling directly on the mesh (no voxelization). |
| `calculate_s2` | `src/geometry/s2.rs:785` | Top-level S2 dispatcher; routes to exact (FFT or direct) or voxelized Monte Carlo. |
| `approximate_s2` | `src/geometry/s2.rs:924` | Convenience wrapper for Monte Carlo S2 estimation with a default voxel pitch. |
| `l2_norm` | `src/geometry/s2.rs:929` | Euclidean distance between two S2 vectors over their common-length prefix. |
| `calculate_s2_with_gpu` | `src/geometry/s2.rs:948` | GPU-accelerated S2 for Monte Carlo/"both" methods, with CPU fallback. *(feature `gpu`)* |
| `calculate_s2_gpu_exact` | `src/geometry/s2.rs:984` | GPU-accelerated exact S2 (GPU voxelization + GPU shell pair counting). *(feature `gpu`)* |
| `PreparedMeshQuery` | `src/geometry/mesh_query.rs:20` | Immutable cached CPU parity query. |
| `PreparedMeshQuery::new` | `src/geometry/mesh_query.rs:29` | Prepare bbox and triangle BVH. |
| `PreparedMeshQuery::bbox` | `src/geometry/mesh_query.rs:68` | Return cached whole-mesh bbox. |
| `PreparedMeshQuery::contains_point` | `src/geometry/mesh_query.rs:73` | Query parity with reusable hit scratch. |
| `MeshQueryScratch` | `src/geometry/mesh_query.rs:7` | Reusable hits and triangle-test counter. |
| `build_nodes` | `src/geometry/mesh_query.rs:144` | Build median BVH with preorder escape links. |
| `ray_reaches_box` | `src/geometry/mesh_query.rs:195` | Conservative positive-ray slab test. |
| `VoxelS2` | `src/geometry/s2.rs:810` | Owned reusable occupancy grid. |
| `VoxelS2::new` | `src/geometry/s2.rs:818` | Prepare CPU occupancy once. |
| `VoxelS2::calculate` | `src/geometry/s2.rs:825` | Compute exact or voxel MC on shared grid. |
| `calculate_s2_mesh_mc_seeded` | `src/geometry/s2.rs:735` | Reproducible sample-block mesh MC. |
| `try_calculate_s2_gpu_exact` | `src/geometry/s2.rs:996` | Fallible GPU exact with checked dimensions. |
| `FftWorkspace` | `src/geometry/s2.rs:334` | Reusable FFT plans and complex arrays. |
| `FftWorkspace::new` | `src/geometry/s2.rs:348` | Construct dimension-specific FFT workspace. |
| `FftWorkspace::array_bytes` | `src/geometry/s2.rs:363` | Report retained complex-array capacities. |
| `with_fft_correlation` | `src/geometry/s2.rs:489` | Evaluate occupancy FFT with bounded cache retention. |
| `FFT_RETAIN_BYTES` | `src/geometry/s2.rs:332` | Maximum retained FFT array bytes per calling thread. |
| `smooth_fft_length` | `src/geometry/s2.rs` | Smallest 2,3,5-smooth length >= a minimum. |
| `padded_fft_dims` | `src/geometry/s2.rs` | Per-axis smooth padding >= 2N-1 for linear autocorrelation. |
| `ExactCpuMethod` | `src/geometry/s2.rs` | CPU exact kernel selector (Fft/Direct). |
| `ExactCpuPlan` | `src/geometry/s2.rs` | Modeled cost, working sets and chosen kernel for one grid. |
| `ExactCpuPlan::selected_bytes` | `src/geometry/s2.rs` | Working set of the selected kernel. |
| `ExactCpuPlan::fits_budget` | `src/geometry/s2.rs` | Whether the selected kernel fits the budget. |
| `ExactCpuPlan::describe` | `src/geometry/s2.rs` | One-line observable plan description. |
| `exact_shell_work` | `src/geometry/s2.rs` | In-domain offsets K, exact pair work W, largest shell. |
| `fft_working_set_bytes` | `src/geometry/s2.rs` | Checked FFT peak-byte estimate. |
| `direct_working_set_bytes` | `src/geometry/s2.rs` | Checked direct peak-byte estimate. |
| `plan_exact_cpu` | `src/geometry/s2.rs` | Cost-model/budget selection between FFT and direct. |
| `cached_exact_plan` | `src/geometry/s2.rs` | Reuse and log the plan once per key. |
| `offset_in_domain` | `src/geometry/s2.rs` | Whether a shift leaves any valid voxel pair. |
| `direct_pair_counts` | `src/geometry/s2.rs` | Integer (hits, valid) for one shift via contiguous z runs. |
| `finish_exact_curve` | `src/geometry/s2.rs` | Assemble, interpolate and pin S2(0). |
| `VoxelS2::calculate_exact_with` | `src/geometry/s2.rs` | Exact S2 with an explicitly chosen CPU kernel. |
| `DEFAULT_CPU_EXACT_BUDGET_BYTES` | `src/geometry/s2.rs` | Default CPU exact working-set budget (768 MiB). |
| `NS_PER_FFT_UNIT` / `NS_PER_DIRECT_PAIR` / `DIRECT_PARALLEL_EFFICIENCY` | `src/geometry/s2.rs` | Calibrated cost-model constants. |

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
- **Source:** `src/geometry/metrics.rs:149`
- **Purpose:** Computes volume, surface area, equivalent-volume diameter, and sphericity for a mesh in one pass, after confirming the mesh is a valid closed manifold.
- **Parameters:**
  - `mesh` — the candidate mesh to measure.
- **Returns:** `Some(MeshMetrics)` when the mesh passes `mesh_is_closed` and both `volume` (via `mesh_volume`) and `surface_area` (via `mesh_surface_area`) are finite and strictly positive, and the derived `equivalent_diameter`/`sphericity` are also finite and strictly positive; `None` otherwise.
- **Side effects:** None.
- **Notes:** `equivalent_diameter = (6V/π)^(1/3)`; `sphericity = π^(1/3)·(6V)^(2/3) / A`. Delegates the actual volume and area computation to `mesh_volume` (`src/geometry/volume.rs`) and `mesh_surface_area` (`src/geometry/mesh_ops.rs`) — this function only adds the manifold gate and the two derived quantities.
- **See also:** `mesh_is_closed` (the validation gate this function calls first), `scale_mesh_to_equivalent_diameter` (typical downstream consumer of the returned `MeshMetrics`).

#### scale_mesh_to_equivalent_diameter

- **Signature:** `pub fn scale_mesh_to_equivalent_diameter(mesh: &mut Mesh, metrics: MeshMetrics, target_diameter: f64) -> Option<f64>`
- **Source:** `src/geometry/metrics.rs:182`
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
- **Source:** `src/geometry/metrics.rs:23`
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
- **Source:** `src/geometry/s2.rs:13`
- **Purpose:** A fixed, non-axis-aligned unit-length ray direction used for ray-casting point-in-mesh tests. Using a direction with no zero or repeated components sidesteps degenerate intersections against axis-aligned mesh faces/edges (a common source of false negatives/positives in naive ray-casting containment tests).
- **Notes:** `point_inside_mesh` (below) hardcodes the same numeric literal locally rather than referencing this constant — the two are kept numerically identical but are not structurally linked. This constant is exported for reuse by the GPU voxelization kernels (see `src/gpu/voxel.rs`), where the CPU-side and GPU-side ray-casting must agree on the sampling direction.
- **See also:** `point_inside_mesh`, [GPU reference](gpu.md).

#### point_inside_mesh

- **Signature:** `pub fn point_inside_mesh(mesh: &Mesh, point: Vec3) -> bool`
- **Source:** `src/geometry/s2.rs:63`
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
- **Source:** `src/geometry/s2.rs:184`
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
- **Source:** `src/geometry/s2.rs:785`
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
  3. **`method == "exact"`** → `VoxelS2::calculate` asks `cached_exact_plan` (see *CPU exact kernel planning* below) to choose between `calculate_s2_exact_fft` and `calculate_s2_exact_direct` by modeled time among kernels whose working set fits `DEFAULT_CPU_EXACT_BUDGET_BYTES`, and logs the choice once per plan key. Both kernels return bit-identical values, so the choice affects only time and memory.
  4. **Any other `method` value** (the voxelized Monte Carlo path) → for each radius `1..=r_max`, precomputes the shell offsets once via `shell_offsets_for_distance`, then draws `samples.max(200)` random voxel-pair samples per radius (random voxel + random offset from that radius's shell) and estimates `S2(r)` as the hit fraction among in-bounds pairs. Unsupported radii (empty shell, or zero valid pairs) are filled in afterward via `fill_missing_s2_with_smooth_interpolation`.
- **See also:** `calculate_s2_monte_carlo_mesh`, `build_bbox_occupancy`, `calculate_s2_exact_direct`, `calculate_s2_exact_fft`, `fill_missing_s2_with_smooth_interpolation`, `calculate_s2_with_gpu` (GPU-accelerated wrapper around this function), [GPU reference](gpu.md).

#### approximate_s2

- **Signature:** `pub fn approximate_s2(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:924`
- **Purpose:** Convenience wrapper for voxelized Monte Carlo S2 estimation with a fixed default voxel pitch of `1.0`.
- **Parameters:** Same as the corresponding subset of `calculate_s2`'s parameters (`mesh`, `bbox`, `r_max`, `samples`).
- **Returns:** `Vec<f64>` of S2 values, identical in shape to `calculate_s2`'s return.
- **Side effects:** None beyond what `calculate_s2` performs (parallel computation via rayon).
- **Notes:** Equivalent to `calculate_s2(mesh, bbox, r_max, 1.0, "monte_carlo", samples)`. The voxel pitch of `1.0` is not adaptive to the mesh's scale — callers with meshes at a different characteristic length should call `calculate_s2` directly with an appropriate pitch instead of this wrapper.
- **See also:** `calculate_s2`.

#### l2_norm

`#### l2_norm` — `pub fn l2_norm(a: &[f64], b: &[f64]) -> f64` — `src/geometry/s2.rs:929`. Euclidean distance between two S2 vectors over their common-length prefix (`n = min(a.len(), b.len())`); returns `0.0` if either slice is empty. No side effects. Used to score how closely a computed/simulated `S2(r)` curve matches a target curve (e.g. from `data/input/gu2019_fig7b_pore_distribution.csv`).

#### calculate_s2_with_gpu

> **Feature-gated:** compiled only with `--features gpu`. CPU fallback: `calculate_s2`.

- **Signature:** `pub fn calculate_s2_with_gpu(mesh: &Mesh, bbox: BoundingBox, r_max: usize, voxel_pitch: f64, method: &str, samples: usize, gpu_pipeline: Option<&mut crate::gpu::s2::GpuS2Pipeline>) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:948`
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
- **Source:** `src/geometry/s2.rs:984`
- **Purpose:** Computes exact S2 entirely on GPU: GPU voxelization followed by GPU shell-offset pair counting.
- **Parameters:**
  - `mesh`, `bbox`, `r_max`, `voxel_pitch` — same meaning as `calculate_s2`, but there is no `method`/`samples` parameter since this function is always the exact method.
- **Returns:** `Vec<f64>` of S2 values for `r = 0..=r_max`.
- **Side effects:** Initializes two GPU pipelines in sequence — `crate::gpu::voxel::GpuVoxelPipeline` (f32 ray-cast voxelization) and `crate::gpu::s2_shell::GpuShellS2Pipeline` (u32 shell pair counting) — and dispatches compute work on each. Prints `[Warning]` on either pipeline's init failure (falling back to CPU `calculate_s2`) and a final `[Info]` timing/summary line on success.
- **Notes:** Generates one radius shell at a time and streams tagged offsets into bounded GPU batches, recording interpolation support during that same enumeration. After the GPU shell pass, still runs `fill_missing_s2_with_smooth_interpolation` on the CPU to patch any radii with empty shells. Uses `f32` precision for the voxelization stage (GPU-side) but `f64` for shell-offset geometry and interpolation (CPU-side) — a precision boundary worth noting when comparing GPU-exact results against `calculate_s2_exact_fft`/`calculate_s2_exact_direct` bit-for-bit.
- **See also:** `calculate_s2` (CPU exact fallback), `calculate_s2_with_gpu` (the Monte Carlo GPU counterpart), `shell_offsets_for_distance`, `fill_missing_s2_with_smooth_interpolation`, [GPU reference](gpu.md).

### Private helpers

#### index_3d_to_flat

`#### index_3d_to_flat` — `fn index_3d_to_flat(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize` — `src/geometry/s2.rs:16`. Converts a 3D voxel index to a flat array index via `x*ny*nz + y*nz + z` (z fastest-varying). No side effects. Used throughout the occupancy-grid code paths (`build_bbox_occupancy`, `calculate_s2_exact_direct`, `calculate_s2`'s voxelized Monte Carlo branch).

#### ray_intersects_triangle

- **Signature:** `fn ray_intersects_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64>`
- **Source:** `src/geometry/s2.rs:26`
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
- **Source:** `src/geometry/s2.rs:112`
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
- **Source:** `src/geometry/s2.rs:217`
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

`#### fft_index_3d` — `fn fft_index_3d(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize` — `src/geometry/s2.rs:328`. Converts a 3D FFT-grid index to a flat index using the same `x*ny*nz + y*nz + z` layout as `index_3d_to_flat`. No side effects.

> **Doc note:** this is a byte-for-byte duplicate of `index_3d_to_flat` (line 13) under a different name, scoped to the FFT-grid code paths (`autocorrelation_counts_fft`, `FftWorkspace::transform`). Not a bug, but worth knowing the two functions are interchangeable — a future cleanup could unify them.

#### FftWorkspace::transform

`FftWorkspace::transform(inverse)` applies cached dimension-specific forward/inverse axis plans to its complex grid. The task count is min(current pool workers, ceil(padded cells / 65536)), at least one. One task uses serial axis gathers and one shared scratch buffer, without allocating a transpose array. Multiple tasks use a lazily allocated persistent transpose buffer and task-local `process_with_scratch` storage, with axis-specific minimum chunk lengths. Filling, power spectrum and normalization use the same task budget. Inverse normalization and the power-spectrum pass execute under the caller's Rayon pool. Padded dimensions are the smallest 2,3,5-smooth lengths >= 2N-1 (`padded_fft_dims`).

#### with_fft_correlation

`with_fft_correlation(occ, dims, consume)` takes exclusive ownership of a thread-local cached workspace, resets/fills its grid, transforms the occupancy, and passes complex correlation storage and padded dimensions to the consumer. Production shell evaluation reads clamped real counts directly, avoiding another full f64 correlation allocation. A workspace is retained only when its two array capacities total at most 16 MiB and every padded axis is <=4096. Plan internals occupy additional memory; this is a cache-admission limit, not a complete process-memory budget. Dimension changes discard the prior workspace before allocating its replacement. Large workspaces are released at return. No TLS borrow survives parallel work or the consumer callback, allowing nested Rayon evaluations safely; concurrent callers have independent workspaces. Kernel selection and the working-set budget are made by `plan_exact_cpu`.

#### autocorrelation_counts_fft

- **Signature:** `fn autocorrelation_counts_fft(occ: &[bool], nx: usize, ny: usize, nz: usize) -> (Vec<f64>, [usize; 3])`
- **Source:** `src/geometry/s2.rs:549`
- **Purpose:** Test-only copying wrapper around `with_fft_correlation`; computes the full occupancy autocorrelation count grid via the FFT convolution theorem (forward FFT → power spectrum → inverse FFT), giving the count of occupied-occupied voxel pairs for every possible integer offset in one pass.
- **Parameters:**
  - `occ` — flat boolean occupancy grid.
  - `nx`, `ny`, `nz` — grid dimensions.
- **Returns:** `(corr, [fx, fy, fz])` — a flat `f64` correlation-count grid over the padded FFT dimensions, and those padded dimensions themselves.
- **Side effects:** None (allocates and returns new buffers; does not mutate `occ`).
- **Notes:** Pads each axis to the smallest 2,3,5-smooth length >= `2N-1` (`padded_fft_dims`) before transforming, to avoid circular-convolution wraparound artifacts that would corrupt correlation counts near the grid boundary. After the round-trip FFT → conjugate-square → IFFT, takes the real part and clamps to `>= 0.0` (autocorrelation counts should be non-negative; clamping guards against tiny negative floating-point noise). Negative offsets are recovered from the padded grid via wraparound indexing in the caller (`calculate_s2_exact_fft`'s `wrap` closure, index `L-|d|`); production rounds each value to the nearest integer count.
- **See also:** `FftWorkspace::transform`, `calculate_s2_exact_fft` (sole caller).

#### calculate_s2_exact_direct

- **Signature:** `fn calculate_s2_exact_direct(occ: &[bool], nx: usize, ny: usize, nz: usize, r_max: usize, voxel_pitch: f64, vf: f64) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:561`
- **Purpose:** Computes exact S2 by directly enumerating and counting voxel pairs for each shell offset at each radius — no FFT, used when the padded FFT grid would be too large to allocate/process efficiently.
- **Parameters:**
  - `occ`, `nx`, `ny`, `nz` — occupancy grid and its dimensions.
  - `r_max` — maximum radius (voxel-grid distance units matching `voxel_pitch`'s scale, but the loop is `0..=r_max` as an integer count of physical-length steps).
  - `voxel_pitch` — voxel edge length, used to convert physical radius `r` to voxel-space `r_vox = r / voxel_pitch`.
  - `vf` — volume fraction, used directly as `S2(0)`.
- **Returns:** `Vec<f64>` of length `r_max + 1`, S2 values per radius, with unsupported radii filled via `fill_missing_s2_with_smooth_interpolation`.
- **Side effects:** None (pure computation); parallel over radii and, within a radius, over offsets.
- **Notes:** For each radius, collects the in-domain offsets of `shell_offset_iter` (same order as `shell_offsets_for_distance`), counts each with `direct_pair_counts` in parallel (indexed collect), and averages `hits/valid` in offset order. Work is `W = Σ valid pairs`; memory is the occupancy plus one offset list per concurrent radius. Returns values bit-identical to `calculate_s2_exact_fft`. `plan_exact_cpu` selects it when its modeled time is lower or the FFT working set exceeds the budget.
- **See also:** `shell_offsets_for_distance`, `fill_missing_s2_with_smooth_interpolation`, `calculate_s2` (sole caller, "exact"-large-grid branch), `calculate_s2_exact_fft` (the FFT alternative for smaller grids).

#### calculate_s2_exact_fft

- **Signature:** `fn calculate_s2_exact_fft(occ: &[bool], nx: usize, ny: usize, nz: usize, r_max: usize, voxel_pitch: f64, vf: f64) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:651`
- **Purpose:** Computes exact S2 by first building the full autocorrelation grid once via FFT (`autocorrelation_counts_fft`), then reading off pair counts for each shell offset at each radius from that precomputed grid.
- **Parameters:** Same as `calculate_s2_exact_direct`.
- **Returns:** `Vec<f64>` of length `r_max + 1`, S2 values per radius, with unsupported radii filled via `fill_missing_s2_with_smooth_interpolation`.
- **Side effects:** None (pure computation); parallelizes the outer radius loop via `rayon`.
- **Notes:** Because the correlation grid is computed once up front (`O(P log P)` with smooth padding) rather than per offset, this is cheaper than `calculate_s2_exact_direct` for large radius ranges, at the cost of the padded-grid memory — the tradeoff `plan_exact_cpu` models. Each correlation value is rounded to the nearest integer count, so results equal the direct kernel's bit for bit. Offsets with `|d| >= N` on any axis are skipped (no valid pair).
- **See also:** `autocorrelation_counts_fft`, `shell_offsets_for_distance`, `fill_missing_s2_with_smooth_interpolation`, `calculate_s2` (sole caller, "exact"-small-grid branch), `calculate_s2_exact_direct` (the direct-enumeration alternative for large grids).

#### calculate_s2_monte_carlo_mesh

- **Signature:** `fn calculate_s2_monte_carlo_mesh(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64>`
- **Source:** `src/geometry/s2.rs:730`
- **Purpose:** Estimates S2 via Monte Carlo sampling of point pairs directly against mesh geometry, with no voxelization step at all.
- **Parameters:**
  - `mesh`, `bbox` — mesh and sampling domain.
  - `r_max` — maximum radius.
  - `samples` — requested samples per radius (floored at 200 via `samples.max(200)`).
- **Returns:** `Vec<f64>` of length `r_max + 1`; `S2(0)` is set to `volume_fraction_in_bbox(mesh, bbox)`.
- **Side effects:** None (pure computation); parallelizes the outer radius loop via `rayon`.
- **Notes:** For each radius `r`, draws `mc_samples` random points `p` uniformly in `bbox`, pairs each with a random unit direction (rejection-sampled from a unit cube, `x²+y²+z² ∈ (1e-12, 1]`), and forms `q = p + dir*r`. Pairs where `q` falls outside `bbox` are discarded entirely (not counted as misses) — so the effective sample count per radius can be lower than `mc_samples`, and shrinks further as `r` approaches the domain size. For surviving pairs, calls `point_inside_mesh` on both `p` and `q` and counts joint-occupancy hits. This is the most geometrically faithful method (no voxel discretization error) but also the slowest, since each sample requires two full `O(faces)` ray-casts against the raw mesh — used when `voxel_pitch <= 0` and `method != "exact"` in `calculate_s2`.
- **See also:** `point_inside_mesh`, `volume_fraction_in_bbox` (`src/geometry/volume.rs`), `calculate_s2` (sole caller, no-voxel-pitch branch).

`calculate_s2_with_gpu` also catches MC execution errors, logs the cause and recomputes continuous CPU mesh MC (pitch 0) rather than changing to voxel MC. Its Vec-returning legacy interface has no strict fallback switch; optimize uses its own Result-based evaluator.

### Prepared CPU S2 queries (PERF-06)

`PreparedMeshQuery::new(&Mesh)` borrows immutable geometry and caches its bbox and a median triangle BVH (more than 32 finite triangles; leaves at most eight). `contains_point(Vec3, &mut MeshQueryScratch)` keeps the original ray, triangle predicate and anchored 1e-8 hit deduplication. Scratch retains hit capacity and exposes the most recent triangle-test count. Small meshes use cached direct queries. Geometry changes require a new prepared query; the original `point_inside_mesh` remains the full-scan reference.

`calculate_s2_mesh_mc_seeded(mesh, bbox, r_max, samples, seed, prepared)` returns continuous MC S2. Radius/sample blocks of 2048 use independent ChaCha12 streams derived from radius and block, with integer reductions. Results are reproducible across worker counts; `prepared=false` uses full-scan containment for differential benchmarks. Ordinary mesh MC chooses a fresh base seed and uses prepared queries. This changes the old unseeded RNG draw protocol, not the point/direction distribution.

`VoxelS2::new(mesh, bbox, pitch)` owns one voxelization at a positive pitch (minimum 1e-9). `calculate(r_max, method, samples)` reuses it for exact or voxel MC, reporting occupancy VF; exact selects its kernel through `cached_exact_plan`. `calculate_exact_with(r_max, method)` (crate-internal) forces FFT or direct. Measure lazily retains this object across CPU methods/fallbacks; continuous MC remains independent. Voxelization prepares per-component queries and reuses per-worker hit scratch. Components below the domain have their upper index clamped before unsigned conversion.

GPU exact now constructs the shell stage on the voxel stage’s Device/Queue and binds its occupancy buffer directly. Shell construction does not select an adapter or request another device, and the shell does not allocate/upload a duplicate occupancy field. Both stages run sequentially with separate error scopes. Voxel occupancy counting now runs on the device and only a four-byte count is read for VF; upstream backend capability probes remain separate. Standalone host-occupancy shell calls retain their upload behavior, and later host calls cannot overwrite the producer’s borrowed buffer.

`voxelize_count` dispatches voxelization followed by a lazily compiled integer occupancy reducer, leaves the full field on device and reads one u32 count. The reducer uses a single 256-lane workgroup with bounded strided reads; binary occupancy and the checked grid size bound every sum by u32. Its total work remains O(grid cells), so reduced transfer is not a guarantee of lower latency. Occupancy and staging grow independently: count-only execution needs four staging bytes, while subsequent full-readback calls grow staging as required. Switching modes and releasing capacity preserves correctness. GPU exact uses this count for VF and passes the resident field to shell; standalone `voxelize` retains its Vec-returning contract.

GPU exact now lazily generates one shell-radius Vec at a time and passes its offsets through `compute_s2_shell_resident_stream`. Host batches retain at most the existing partial-slot allowance; they can span radius boundaries while preserving offset order. The support flags used for interpolation are captured when each shell is generated, eliminating the former second enumeration. All offsets, including unsupported tails, are consumed on successful evaluation. Memory is bounded by one radius shell plus a batch, not by a constant independent of radius; an individual large-radius shell is still materialized. Count diagnostics use u128 so aggregate generated counts are not silently saturated.

GPU exact now uses `shell_offset_iter`, retaining only nested range cursors even within one radius. It preserves the original x/y/z order, origin special case and half-open squared-distance test; the public Vec API remains unchanged for random-access consumers. Support is detected with a peekable iterator and every generated offset is counted as consumed. Safe ordinary integer norms match the Vec implementation; larger norms use u128 to avoid signed multiplication overflow. Enumeration still scans the enclosing cube, so this reduces allocation without changing its O(radius³) search complexity.

| Function | Source | Contract |
|---|---|---|
| `shell_offset_iter` | `src/geometry/s2.rs:213` | Lazy ordered shell enumeration with constant cursor storage; GPU exact streaming, both CPU exact kernels, and tests. |

## CPU exact kernel planning and smooth padding (PERF-07)

`padded_fft_dims(dims)` pads each axis to `smooth_fft_length(2N-1)`, the smallest 2,3,5-smooth length that is at least `2N-1` (`smooth_fft_length` enumerates `2^a·3^b·5^c` with checked arithmetic; `None` on overflow). Any length `L >= 2N-1` keeps the circular correlation of the zero-padded grid equal to the linear one for every shift `|d| <= N-1`, and a negative shift `-d` is still read at index `L-d`. `with_fft_correlation`, `FftWorkspace` and the retention rule use these padded dimensions.

`calculate_s2_exact_fft` rounds each correlation value to the nearest integer pair count before dividing by the analytic valid-pair count, and both CPU kernels enumerate each radius with `shell_offset_iter` filtered by `offset_in_domain`, summing `hits/valid` in the same order. The FFT and direct kernels therefore return bit-identical curves for every grid, padding and worker count (tested with `forced_kernels_are_selection_independent` and the all-offset integer oracle `fft_scratch_matches_integer_pairs`, which includes FFT-unfriendly and long/thin axes). Rounding requires the FFT absolute error to stay below 0.5, which holds by many orders of magnitude for f64 grids of the admitted size.

`calculate_s2_exact_direct` now parallelizes over radii and, inside each radius, over its in-domain offsets (indexed collect, then an ordered sum); `direct_pair_counts` counts each shift over contiguous z runs. `finish_exact_curve` is the shared tail (interpolation, `S2(0)=vf`).

`plan_exact_cpu(dims, r_max, pitch, workers, budget)` returns an `ExactCpuPlan`:

- `exact_shell_work` enumerates one octant of the in-domain offset ball once (sign copies weighted by multiplicity, loops cut at `r_max`) with the shell iterator's half-open float bounds, returning the in-domain offset count `K`, the exact direct work `W = Σ (nx-|dx|)(ny-|dy|)(nz-|dz|)` and the largest per-radius offset count. A unit test checks it against brute-force shell enumeration.
- Modeled times: FFT `NS_PER_FFT_UNIT·P·log2 P` (P = padded cells, no parallel credit); direct `NS_PER_DIRECT_PAIR·W / (1 + DIRECT_PARALLEL_EFFICIENCY·(min(workers,K)-1))`. Constants (2.0 ns, 0.36 ns, 1/3) are single-worker release fits from the ignored `exact_cost_model_calibration` test; at 4 workers that host showed no FFT speedup and about 2x direct speedup.
- Working sets (checked `u64`): FFT = occupancy + complex grid + transpose (when the transform uses more than one task) + per-worker line/scratch + axis plans + output; direct = occupancy + in-domain offsets and per-offset counts for each concurrently processed radius + output. Plans' opaque internals and other threads' retained 16 MiB caches are not included.
- Only kernels whose working set fits the budget are eligible; the cheaper one wins (FFT on ties). If neither fits, direct is reported with reason `no kernel fits budget`.

`DEFAULT_CPU_EXACT_BUDGET_BYTES` is 768 MiB, the size of the old 24,000,000-padded-cell FFT limit's grid plus transpose, so no configuration allocates materially more than before. `VoxelS2::calculate` uses `cached_exact_plan`, which reuses the last plan for an identical `(dims, r_max, pitch, workers, budget)` and prints `[Info] CPU exact S2 plan: ... method=... reason=...` only when the key changes. `VoxelS2::calculate_exact_with` forces a kernel for tests and benchmarks.
