# The S2 Two-Point Correlation Function

S2(r) is a standard microstructure-characterization statistic: the probability that two points
separated by a distance `r`, dropped at random into the packed volume, both land in the same
phase (here, both inside solid particle material). At `r = 0` it necessarily equals the volume
fraction (VF), since a point trivially agrees with itself; as `r` grows it decays toward VF²
for a fully random (uncorrelated) medium, and the shape of the decay in between encodes
particle size, shape, and spatial arrangement. RustMSPT uses S2 for two distinct purposes:

- **`measure`** (`src/pipeline/measure.rs`) computes S2 for a packed geometry as a diagnostic —
  reporting it alongside VF so a user can characterize a finished pack or compare methods
  (exact vs. Monte Carlo) against each other via their L2 distance.
- **`optimize`** (`src/pipeline/optimize.rs`) treats S2 as an optimization *target*: given a
  target S2 curve (e.g. measured from a reference microstructure), its simulated-annealing
  islands (`run_sa_island`) perturb particle positions to minimize the L2 distance between the
  current pack's S2 curve and the target curve, effectively reconstructing a microstructure that
  reproduces the target's two-point statistics.

All of the logic described here lives in `src/geometry/s2.rs`, with GPU-accelerated counterparts
in `src/gpu/s2.rs` and `src/gpu/s2_shell.rs`.

## Point-in-mesh: ray casting

Every CPU-side S2 method ultimately needs to answer "is this point inside the solid?" for
arbitrary query points. `point_inside_mesh` answers this with a ray-casting parity test: after a
cheap bounding-box rejection, it fires a ray from the query point in a **fixed, non-axis-aligned
direction** through every triangle of the mesh, using the Möller–Trumbore
ray/triangle-intersection algorithm (`ray_intersects_triangle`) to find intersection distances
`t`. The point is inside the mesh if and only if the ray crosses the surface an odd number of
times.

Two details make this parity count robust in practice:

- The ray direction is a fixed unit vector, `(0.9428090415820634, 0.2705980500730985,
  0.19611613513818402)`, chosen to avoid axis-aligned edge cases where a ray might graze along a
  triangle edge or lie in a triangle's plane. The same constant is exported as
  `RAY_DIR_GPU` and reused verbatim by the GPU Monte Carlo kernel (packed into shader params in
  `src/gpu/s2.rs`'s `pack_params`), so CPU and GPU point-in-mesh tests agree.
- Raw hit distances are sorted and then **deduplicated within `1e-8`** before counting parity.
  Without this, a ray that passes exactly through a shared vertex or edge between two adjacent
  triangles would be counted as two intersections instead of one, corrupting the odd/even parity
  and misclassifying points near mesh seams.

## Voxelization

The exact and voxelized-Monte-Carlo methods both need a discretized occupancy grid rather than
repeated mesh queries. `build_bbox_occupancy` samples the mesh onto a boolean grid at a given
`voxel_pitch`: it computes grid dimensions `nx, ny, nz` from the bounding box size and pitch, then
tests the center of every voxel with `point_inside_mesh`. Two optimizations keep this tractable
on granular packs with many particles:

- The mesh is split into per-particle granules (`split_mesh_into_granules`) up front, and each
  granule's own bounding box is intersected with the voxel grid so that only the voxel range a
  given particle could plausibly occupy is tested against that particle's triangles, rather than
  testing every voxel against the whole mesh.
- The x-dimension is parallelized with rayon (`par_chunks_mut` over x-slabs), since each x-slab
  writes to disjoint output.

## Three computation methods

`calculate_s2` dispatches to one of three underlying strategies depending on the requested
`method` string and, for `"exact"`, the resulting grid size. Each trades accuracy against cost
differently, which is why the codebase keeps all three rather than picking one:

### 1. Monte Carlo directly on the mesh

`calculate_s2_monte_carlo_mesh` never voxelizes at all. For each radius `r` it draws random
point/direction pairs — a uniformly random origin point `p` inside the bounding box and a
uniformly random unit direction `dir` — forms `q = p + r * dir`, discards pairs where `q` falls
outside the bounding box, and otherwise tests both `p` and `q` with `point_inside_mesh`. The
fraction of valid pairs where both points hit the solid phase estimates S2(r). Because it needs
no voxel grid, this is the cheapest method when only a handful of large radii are needed, or as a
model-free sanity baseline to check that a voxelized computation is not introducing
discretization bias.

### 2. Voxelized Monte Carlo

This is the fallback branch of `calculate_s2`'s `match method` for any non-`"exact"` value once a
voxel_pitch has been resolved: it builds the occupancy grid once via `build_bbox_occupancy`, then
for each radius samples random voxel pairs — a random voxel plus a random offset drawn from that
radius's **shell offset set** (see below) — and looks up occupancy directly in the boolean array
instead of re-testing the mesh. Because occupancy lookups are O(1) array reads rather than
ray/triangle intersections, this amortizes the one-time voxelization cost across many radii and
many samples per radius, making it the default/cheap choice for iterative use (e.g. inside the
`optimize` pipeline's per-iteration S2 evaluation, where the same pack is re-evaluated on every
accepted perturbation).

### 3. Exact (FFT or direct shell-pair enumeration)

The `"exact"` method computes S2(r) exhaustively — every valid voxel pair at the shell distance
for `r`, not a random sample — via one of two equivalent algorithms:

- **FFT-based** (`calculate_s2_exact_fft` / `autocorrelation_counts_fft`). The autocorrelation of
  the occupancy grid gives, for every integer offset `(dx, dy, dz)`, the exact count of voxel
  pairs `(v, v+offset)` that are both occupied — precisely what a shell-pair sum needs, computed
  in one shot via `FFT → power spectrum (|F|²) → inverse FFT` instead of a triple-nested loop over
  offsets and grid positions. The grid is **zero-padded to `2N-1`** along each axis before
  transforming:

  ```
  fx = 2*nx - 1, fy = 2*ny - 1, fz = 2*nz - 1
  ```

  This padding is required because FFT-based autocorrelation is inherently *circular*: without
  it, an offset near one edge of the grid would wrap around and pick up spurious "pairs" from the
  opposite edge. Padding to `2N-1` per axis guarantees no wraparound contaminates any offset in
  range `[-(N-1), N-1]`.
- **Direct shell-pair enumeration** (`calculate_s2_exact_direct`). For each radius, iterate every
  offset in that radius's shell (see below), and for each offset scan every valid `(x, y, z)` grid
  position, checking occupancy at both `(x,y,z)` and `(x,y,z)+offset` directly — no FFT, but O(shell
  size × grid size) per radius, parallelized over radii via rayon.

`calculate_s2` picks between them automatically based on the **padded FFT grid size**: it
computes `fx * fy * fz` and compares against a fixed threshold of **24,000,000 cells**. Below the
threshold, the FFT path is used (fast, and its cost is independent of `r_max` once computed).
Above it — i.e. for large voxel grids where the padded `2N-1` cube would blow up memory and FFT
runtime — it falls back to direct enumeration, which is slower per radius but has no large
upfront memory allocation.

## Shell offsets: `shell_offsets_for_distance`

Both exact methods and the voxelized Monte Carlo method need the same primitive: given a target
radius (in voxel units) and a half-width (fixed at `0.5 / voxel_pitch`, i.e. one half-voxel),
enumerate every integer voxel offset `[dx, dy, dz]` whose Euclidean length falls within that
annulus (`shell_offsets_for_distance`). This turns "all voxel pairs separated by physical distance
r" into a concrete, finite offset list that direct enumeration and Monte Carlo sampling can both
iterate/sample from, and that the exact FFT path can look up directly in the padded correlation
grid. The special case `distance_vox <= 1e-12` returns just `[[0,0,0]]` (i.e. r = 0, a point
against itself). The same offset-enumeration logic is reused by the GPU exact path
(`calculate_s2_gpu_exact` builds an offset list with `shell_offsets_for_distance` and hands it to
`GpuShellS2Pipeline`), so CPU and GPU exact results are computed over identical shells.

## Filling unsupported radii: smooth interpolation

Not every radius has voxel pairs available at a given shell — small grids, or radii close to the
box's diagonal, can end up with zero valid offsets. `fill_missing_s2_with_smooth_interpolation`
patches these gaps: for exactly two known neighbors it uses a smoothstep-blended linear
interpolation; for three or more known points it fits a natural cubic spline (solved via a
tridiagonal system for the second derivatives) through the supported radii and evaluates it at the
missing ones. All filled values are clamped to `[0, 1]`, since S2 is a probability, and `values[0]`
is always forced to the volume fraction regardless of interpolation. This is called at the end of
every voxel-grid-based method (both exact variants and voxelized Monte Carlo) as a shared
post-processing step.

## The r = 0 special case

Every method special-cases `r = 0` rather than running the general sampling/enumeration logic on
it: `calculate_s2_monte_carlo_mesh`, `calculate_s2_exact_direct`, `calculate_s2_exact_fft`, and the
voxelized-Monte-Carlo branch of `calculate_s2` all return the precomputed volume fraction `vf`
immediately when `r == 0`, and every result array is forced to `out[0] = vf` again after
interpolation as a final safeguard. This is both a definitional shortcut (S2(0) is VF by
definition, no sampling needed) and a numerical-stability measure (the shell-offset degenerate
case at `r = 0` is a single self-pair, which would otherwise need special handling in every
downstream computation). The GPU paths follow the same convention: `calculate_s2_with_gpu`
overwrites `result[0]` with `volume_fraction_in_bbox(mesh, bbox)` after the Monte Carlo kernel
returns, and `calculate_s2_gpu_exact` does the same before returning.

## GPU acceleration

The GPU paths implement the same three-method taxonomy as the CPU code, replacing the
inner ray-casting or shell-pair loops with wgpu compute kernels while keeping the surrounding
per-radius/shell structure identical:

- **`calculate_s2_with_gpu`** dispatches Monte Carlo sampling to `GpuS2Pipeline::calculate_s2_gpu`
  (in `src/gpu/s2.rs`) when `gpu_pipeline` is `Some` and the method is `"monte_carlo"` or `"both"`.
  The kernel uploads a normalized f32 triangle buffer once and, in a single dispatch, runs
  `samples_per_radius` independent point-in-mesh-pair trials per radius using the same
  `RAY_DIR_GPU` ray direction as the CPU path, accumulating hit/valid counts that are read back and
  reduced into an S2 curve on the CPU side. This is the GPU counterpart of "Monte Carlo directly on
  the mesh" (method 1 above) — it operates on raw triangle data, not a voxel grid.
- **`calculate_s2_gpu_exact`** implements the GPU counterpart of the exact method: it first
  voxelizes the mesh on the GPU (`GpuVoxelPipeline::voxelize`, ray-casting per voxel much like
  `build_bbox_occupancy` but on-device), computes the volume fraction from the occupied voxel
  count, builds the full shell-offset list for every radius via `shell_offsets_for_distance`, and
  hands the occupancy grid plus offset list to `GpuShellS2Pipeline::compute_s2_shell` (in
  `src/gpu/s2_shell.rs`), which counts occupied pairs per shell offset directly on the GPU. The
  result still goes through `fill_missing_s2_with_smooth_interpolation` on the CPU afterward.

Both entry points **fall back to the CPU path on GPU initialization failure**: `calculate_s2_gpu_exact`
catches errors from either `GpuVoxelPipeline::new` or `GpuShellS2Pipeline::new` and calls
`calculate_s2(..., "exact", ...)` instead, printing a `[Warning]` line; `calculate_s2_with_gpu`
simply falls through to `calculate_s2` whenever `gpu_pipeline` is `None`. This makes the GPU
feature purely additive — every consumer (`measure`, `optimize`) can request GPU acceleration
without needing a fallback code path of its own.

## Comparing S2 curves: `l2_norm`

`l2_norm(a, b)` is the Euclidean distance between two S2 vectors — `sqrt(sum((a[i] - b[i])^2))`
over their common length prefix. `measure` uses it (via its own `l2_error` wrapper) to report how
far the exact and Monte Carlo methods agree with each other on the same geometry; `optimize` uses
it as the simulated-annealing loss function, computing the L2 distance between the current
candidate pack's S2 curve and the target S2 curve inside `run_sa_island` and its supporting
`selective_prune_to_target_vf` pass, and accepting/rejecting perturbations via the Metropolis
criterion on that loss.

## Cross-references

- [geometry-analysis.md](../reference/geometry-analysis.md) — full function-level reference for
  `point_inside_mesh`, `shell_offsets_for_distance`, `calculate_s2`, `approximate_s2`, `l2_norm`,
  `calculate_s2_with_gpu`, `calculate_s2_gpu_exact`, `fill_missing_s2_with_smooth_interpolation`,
  `autocorrelation_counts_fft`, `calculate_s2_exact_direct`, `calculate_s2_exact_fft`, and
  `calculate_s2_monte_carlo_mesh`.
- [gpu.md](../reference/gpu.md) — `GpuS2Pipeline` (Monte Carlo S2) and `GpuShellS2Pipeline` (exact
  shell-pair S2) pipeline construction, buffer layout, and dispatch details.
- [pipeline-core.md#MeasurePipeline::run](../reference/pipeline-core.md#measurepipelinerun) —
  where S2 is computed and reported as a diagnostic during the `measure` pipeline.
- [pipeline-optimize.md#run_sa_island](../reference/pipeline-optimize.md#run_sa_island) — where the
  S2 L2 loss drives the simulated-annealing acceptance criterion.
