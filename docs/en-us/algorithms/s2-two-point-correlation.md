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

`calculate_s2` picks between them with a **cost model under a memory budget** (`plan_exact_cpu`,
PERF-07). FFT time is modeled as `c_fft · P · log2 P` for `P` padded cells; direct time as
`c_pair · W`, where `W` is the exact number of voxel pairs the direct kernel visits (the sum over
in-domain shell offsets of their overlap volumes, at most `K · N`). Only kernels whose peak
working set (checked arithmetic: occupancy, complex grid, one x-axis gather band, per-worker scratch for FFT;
occupancy and per-radius offset lists for direct) fits the default 768 MiB budget are eligible,
and the cheaper one wins. The budget equals the grid-plus-transpose size of the old fixed
24,000,000-padded-cell limit. FFT correlation values are rounded to integer pair counts and both
kernels sum shell terms in the same order, so the two return **bit-identical** curves: the choice
changes only time and memory. The chosen method, both modeled times, both working sets and the
reason are printed once per plan.

In practice FFT wins when `r_max` is large relative to the grid (its cost does not grow with
`r_max`), and direct wins for small `r_max` on large grids, where `W` stays near `K · N` with small
`K`. Padding uses the smallest 2,3,5-smooth length `>= 2N-1` per axis rather than exactly `2N-1`;
any length `>= 2N-1` is wrap-free for every shift `|d| <= N-1`, and smooth lengths avoid slow
prime-size transforms (e.g. 127 -> 128, 199 -> 200).

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

## Optimize execution (PERF-01/02)

The optimizer now resolves its S2 definition once through `OptimizeS2`. Positive-pitch MC stays
voxel MC, exact stays voxel exact, and only nonpositive-pitch non-exact requests can use continuous
GPU MC. Target, pruning, initial/candidate/migration states and final verification share that method.
The continuous path uses geometric VF; the voxel paths use occupancy VF. One GPU MC instance is
shared across the whole run when selected. This does not alter `measure` or the shared GPU wrappers
described above; their separate correctness/resource work remains pending.

### GPU layout and shell boundaries (2026-09-13)

Voxel parameters occupy 48 bytes with the ray direction starting at byte 32. Both
MC and voxel triangle uploads subtract the bbox origin in f64 before narrowing to
f32, preserving local geometry under large translations. Shell counting rejects
out-of-domain offsets before unsigned arithmetic and processes lists in batches
of at most 200,000 offsets. Reduction remains the equal-weight mean of valid
per-offset ratios across every batch; empty lists retain only the supplied VF.
These fixes do not resolve the 64-hit ray limit, MC radius limits, general workload
planning, or runtime error fallback; see `PLAN.Performance.md` §12.

### Overflow ray recovery (2026-09-13)

Both MC and voxel shaders now recover rays with more than 64 raw positive triangle
hits instead of silently truncating them. The ordinary sorted-array path remains;
on overflow, successive full scans find the nearest remaining distinct distance
and count parity using the same anchored `1e-6` GPU tolerance. (Superseded by the f32 certification below: both paths now use the CPU `1e-8` band and prove every gap.) No Monte Carlo trial
is dropped or resampled. Recovery requires constant extra storage but can cost
O(triangles × distinct hits), so this correctness fix may be slow on dense scenes.
CPU/GPU numerical equivalence and runtime device/readback errors remain unresolved.
See the GPU function reference and `PLAN.Performance.md` §13 for tests and limitations.

MC execution now returns checked capacity/readback/device errors. The legacy GPU wrapper retries continuous CPU mesh MC; optimize honors its explicit fallback policy. Voxel/shell runtime propagation remains pending. See the GPU reference and `PLAN.Performance.md` §14.

Measure now keeps positive-pitch voxel MC distinct from continuous mesh MC, performs CPU fallback inside its configured pool, and records actual per-method backends. The fallible GPU exact entry normalizes nonpositive pitch to 1.0 and propagates voxel/shell execution errors; it does not silently select an approximate method.

### CPU preparation and sample blocks

CPU mesh MC and voxelization now cache geometry queries through `PreparedMeshQuery`, preserving the full-scan parity predicate and hit tolerance. MC distributes fixed 2048-sample blocks across workers; a supplied base seed gives identical integer hit/valid reductions across worker counts. The ordinary entry draws a new base seed per evaluation. Measure reuses one lazy CPU `VoxelS2` occupancy for exact and positive-pitch MC, including GPU fallback, avoiding repeated splitting and containment. GPU occupancy sharing remains separate pending work.

### Reusable exact FFT storage

CPU exact FFT now reuses forward/inverse axis plans, complex grid and transpose storage for repeated same-dimension evaluations on a calling thread. It fills and normalizes under the current pool, reads shell counts directly from the complex correlation grid, and releases oversized workspaces after use. Retention is capped at 16 MiB of arrays per thread and padded axes <=4096; opaque FFT plan memory is additional. Nested evaluations take separate owned workspaces without holding a thread-local borrow. The memory/cost planner and smooth padding described above were added later (PERF-07).

### GPU MC workgroup integer reduction

The production MC shader assigns each 256-lane workgroup to one `(radius, sample block)`. Active lanes use the original logical id `radius * samples_per_radius + sample` for the RNG; padded lanes contribute zero. A uniform workgroup reduction emits one hit/valid pair per block. CPU merges these partials in u64 and applies the same ratio, reading `8 * (r_max+1) * ceil(max(samples,200)/256)` bytes instead of per-sample flags. Radius padding cannot mix counts or redraw samples. `dispatch_plan` checks logical-id overflow, the padded workgroup count and partial-buffer sizes independently. `mc_evaluation_peak` uses these partial capacities and still accounts for retained buffers/uploads/growth.

`calculate_s2_gpu_counts` is a private fixed-seed path returning integer totals; the public API still draws one fresh random seed and returns the curve. A frozen pre-reduction shader at `tests/fixtures/s2_monte_carlo_samples.wgsl` is used only in tests to compare exact hit/valid totals under identical seeds, including tail blocks, 128 radii, geometry updates and invalid offsets. BVH traversal, per-radius GPU final reduction and budget-dependent sample batching remain separate work. This change establishes reduction/count semantics, not CPU/GPU f64 equivalence or a hardware speedup claim.

MC workgroups now span two dispatch dimensions, with block index `group.x + group.y * num_workgroups.x`. A uniform guard rejects padded groups before any barrier or write, including when retained output capacity exceeds the live result length. The planner returns samples, partial count and `[x,y]` dispatch; logical sample IDs remain bounded by u32. Optimize startup no longer applies the obsolete one-dimensional invocation ceiling. Forced 3×3/4×2 execution matches the frozen per-sample shader at identical seeds; a spare-capacity sentinel checks that padding never writes beyond live partials. The large 65,536-group case is planner-only coverage, not a large GPU execution benchmark.

GPU shell valid-pair counts are computed analytically as `(nx-|dx|)*(ny-|dy|)*(nz-|dz|)` after rejecting displacements outside any axis. Host grid validation bounds the full product by u32, so every overlap product is safe. The shader still enumerates occupancy pairs for hits and CPU still averages complete per-offset ratios with equal weight; offset tiling and device-resident occupancy remain pending. A raw-count oracle independently enumerates signed coordinates for all small offsets, thin grids, empty/full/mixed occupancy and i32 extreme displacements. This arithmetic change alone does not establish a measured speedup.

The experimental `s2_shell_cooperative.wgsl` assigns one 256-lane workgroup per offset. Lanes stride through the overlap volume, then reduce integer hits in shared memory; valid counts remain analytic. Two-dimensional group flattening rejects padding uniformly before barriers. It retains one output pair per offset, so this is not yet multiple independently scheduled voxel tiles or resident voxel-to-shell dataflow. Production still selects the direct shader pending the workload benchmark. `new_with_shader(source, offsets_per_workgroup)` pairs shader indexing with the dispatch planner: direct uses 256 offsets per group and cooperative uses 1.

The experimental tiled shader uses `(offset, voxel tile)` workgroups with a selectable positive tile width. Each tile reduces integer hit counts and emits its analytic overlap length; empty tiles emit zero. Host code sums all tile counts for one offset in u64 before forming that offset ratio. Batches contain at most 200,000 partial slots, reducing offsets per batch by the tile count; workloads with more than 200,000 tiles per offset currently return an explicit capacity error. This fixed cap is not a complete user-budget planner. Parameter storage is 24 bytes (offset count, dimensions, tiles per offset, tile width); older direct shaders read the first 16 bytes. Production continues to use direct evaluation while the tiled path is validated and benchmarked.

The experimental tiled path can also enable a second device pass (`s2_shell_reduce.wgsl`) that sums tile hit/valid counts into one pair per offset. These sums fit u32 because every offset has at most the checked full-grid cell count. CPU still performs the original per-offset ratio averaging. The reducer and final buffers are initialized lazily and reused; explicit batch release shrinks final buffers while retaining the compiled reducer. Readback is 8 bytes per offset regardless of tile count, but tile buffers and an additional dispatch remain. This is an experimental option, not a production selection or proof of speedup.

Production shell evaluation now filters offsets whose unsigned displacement magnitude reaches any grid dimension before GPU upload. Filtering preserves input order and uses a reusable bounded host batch, not a second full offset list. An unsupported-only request returns the same VF/zero curve without uploading occupancy or allocating result buffers. Test reference constructors can disable filtering so raw invalid-offset shader behavior remains covered. Per-offset ratios and their equal weighting are unchanged; this filter does not remove empty tiles inside otherwise supported offsets.

GPU exact now constructs the shell stage on the voxel stage’s Device/Queue and binds its occupancy buffer directly. Shell construction does not select an adapter or request another device, and the shell does not allocate/upload a duplicate occupancy field. Both stages run sequentially with separate error scopes. Voxel occupancy counting now runs on the device and only a four-byte count is read for VF; upstream backend capability probes remain separate. Standalone host-occupancy shell calls retain their upload behavior, and later host calls cannot overwrite the producer’s borrowed buffer.

`voxelize_count` dispatches voxelization followed by a lazily compiled integer occupancy reducer, leaves the full field on device and reads one u32 count. The reducer uses a single 256-lane workgroup with bounded strided reads; binary occupancy and the checked grid size bound every sum by u32. Its total work remains O(grid cells), so reduced transfer is not a guarantee of lower latency. Occupancy and staging grow independently: count-only execution needs four staging bytes, while subsequent full-readback calls grow staging as required. Switching modes and releasing capacity preserves correctness. GPU exact uses this count for VF and passes the resident field to shell; standalone `voxelize` retains its Vec-returning contract.

GPU exact now lazily generates one shell-radius Vec at a time and passes its offsets through `compute_s2_shell_resident_stream`. Host batches retain at most the existing partial-slot allowance; they can span radius boundaries while preserving offset order. The support flags used for interpolation are captured when each shell is generated, eliminating the former second enumeration. All offsets, including unsupported tails, are consumed on successful evaluation. Memory is bounded by one radius shell plus a batch, not by a constant independent of radius; an individual large-radius shell is still materialized. Count diagnostics use u128 so aggregate generated counts are not silently saturated.

GPU exact now uses `shell_offset_iter`, retaining only nested range cursors even within one radius. It preserves the original x/y/z order, origin special case and half-open squared-distance test; the public Vec API remains unchanged for random-access consumers. Support is detected with a peekable iterator and every generated offset is counted as consumed. Safe ordinary integer norms match the Vec implementation; larger norms use u128 to avoid signed multiplication overflow. Enumeration still scans the enclosing cube, so this reduces allocation without changing its O(radius³) search complexity.

Fresh resident GPU exact evaluations use `ExactMemoryPlan` for both backend selection and execution. With triangle storage T=max(36*faces,4), occupancy M=4*cells, and B partial slots, the conservative logical peak is 2T+M+128+C+80B, where C = `exact_cert_bytes(cells)` covers the voxel certification list, its staging and parameter tail. This includes pending triangle/offset uploads and simultaneous old/new batch buffers; the 128-byte allowance covers fixed parameter/count/placeholder resources. B is reduced from 200,000 to fit an optional MiB budget, with a minimum of one. If even that does not fit, execution rejects before GPU initialization and the caller applies its fallback policy. The model excludes driver/compiler internals and CPU memory, applies to a fresh production direct-shell evaluation, and does not claim to budget experimental tiled/reduced or arbitrary retained pipelines. Existing hard exact-grid limits remain independent.

### f32 certification of GPU ray parity (PERF-05)

The MC shader (`s2_monte_carlo.wgsl`) and the voxelizer (`voxelize.wgsl`) no longer take a raw f32 parity decision. Each query is classified as **certainly outside**, **certainly inside** or **uncertain**, and every uncertain query is re-evaluated on the host by the unchanged CPU f64 predicate (`point_inside_mesh` / `PreparedMeshQuery::contains_point`) at the *same* point. The certified GPU result therefore equals the CPU reference exactly, query for query.

**Reference.** The reference is the CPU predicate applied to the mesh shifted by the GPU origin (`bbox.min`) in f64 (`CertReference`), at the exact f32 query point the GPU used, promoted to f64. MC points are the shader's own `p`/`q` (their bits are returned for every uncertain sample); voxel centers are `(f32(i) + 0.5) * pitch`, two correctly rounded operations that `voxel_center` reproduces bit for bit on the host. The shader mirrors the CPU thresholds exactly: `|det| > 1e-10`, `u in [-1e-10, 1+1e-10]`, `v >= -1e-10`, `u+v <= 1+1e-10`, `t > 1e-10`, anchored `1e-8` hit deduplication, and the `mesh bbox +/- 1e-9` early-out. The former GPU-only `1e-6` tolerances are gone.

**Exact early-out.** The host rounds the f64 thresholds `bb.min - 1e-9` up and `bb.max + 1e-9` down to f32 (`f32_at_least`/`f32_at_most`). For any f32 `p`, `p < ceil32(L)` iff `p < L` and `p > floor32(H)` iff `p > H`, so the bbox test is exact, not bounded.

**Error bound (per triangle, per query).** Let `u = 2^-24`. Inputs: the f32 vertices differ from the f64 reference vertices by at most `u|x|`; the direction by at most `u`; the query point is exact. With `m = max|coordinate of a,b,c| (1+u)` and safe magnitudes `X = |x^|inf + eps_x` (bounding both the computed and the exact vector), first-order forward analysis gives

| quantity | bound |
|---|---|
| `e1 = b-a`, `e2 = c-a` | `eps_e = 4 u m` (two input roundings + one subtraction) |
| `s = o - a` | `eps_s = u (|o|inf + 2m)` |
| `h = d x e2` | `eps_h = 6 u E + 2 eps_e` (cross: `gamma_2 * 2XY` rounding plus input perturbation `2(eps_x Y + X eps_y)`) |
| `q = s x e1` | `eps_q = 4 u S E + 2 (eps_s E + S eps_e)` |
| `det = e1.h` | `9 u E H + 3 (eps_e H + E eps_h)` (dot: `gamma_3 * 3XY` plus `3(eps_x Y + X eps_y)`) |
| `U = s.h`, `V = d.q`, `T = e2.q` | same dot rule with their operands |

and each numerator bound is multiplied by `SAFETY = 2` and gets `+1e-36` (flush-to-zero allowance). For a ratio `r = N/det` computed as `N * (1/det)` (WGSL division is 2.5 ULP = 5u relative, the product u), the bound is

`err_r = (eps_N + 1.01 |r| eps_det) / (|det| - eps_det) + SAFETY * 7u |r|`, valid when `|det| > eps_det` (otherwise uncertain).

`u+v` adds `u|u+v|`. Every comparison `x >= thr` is decided by `cert_ge`: certainly true if `x - err - slack >= thr`, certainly false if `x + err + slack < thr`, where `slack = u(|thr| + |x| + err) + 1e-36` absorbs the f32 representation of the threshold (`1 + 1e-10` is `1.0` in f32) and the rounding of the test itself. NaN or infinite bounds are uncertain. `SAFETY = 2` covers the neglected second-order terms, the `1/(1-ku)` factors of `gamma_k`, FMA contraction (which only removes roundings) and the CPU's own f64 evaluation error, which has the same form with `2^-53` and is therefore below `2^-29` of the f32 bound.

Near-parallel triangles make `|det|` comparable to its bound, so ratios cannot be bounded there. Before dividing, a division-free test proves a miss from the numerators: if `|U^| - eps_U > (|det^| + eps_det)(1 + 16u)` then every admissible CPU determinant gives `|u| > 1 + 1e-10`, and the same for `V` gives `|v| > 1 + 2e-10`; either contradicts a hit (or the CPU rejects `det` outright). Without this test every edge-on triangle made its query uncertain (100 % of `particles.stl` queries); with it the ratio fell to 0.2 %.

A triangle is a certain miss if **any** CPU condition is certainly false (the CPU result is their conjunction), a certain hit only if **all** are certainly true, and uncertain otherwise. A query is uncertain as soon as one triangle is. Hits are certain *distinct* only if, after sorting, every consecutive gap exceeds `1e-8 + 2 max(err_t) + u t`; the CPU then keeps every hit and parity is the hit count. Any closer pair (a ray through a shared edge or vertex, coincident duplicate triangles, a slab thinner than the bound) is uncertain. The >64-hit overflow path performs the same proof with constant storage: one pass for the bound, then per distinct hit one nearest-selection pass and one pass counting hits within the band.

**Host re-evaluation.** Uncertain queries are appended to an atomic list: MC records `(logical sample id, p, q)` as 7 words and contributes `valid = 1, hit = 0` to its workgroup partial; the host decodes the radius as `id / samples_per_radius` and adds a hit when the CPU finds both points inside. Voxels store 0 provisionally and record the flat cell index; the host classifies the center, writes `1` back into the resident occupancy (coalesced `write_buffer` runs, so the resident shell stage sees the certified field) and adds it to the count or to the full readback. The counter always advances, so an overflowed list is detected; the list is regrown to the exact count and the batch re-dispatched (the logical sample ids and cell centers are deterministic). MC lists start at 1024 records; voxel lists are pre-sized to `max(1024, cells/64)` entries (`voxel_uncertain_entries`), which covers the measured ordinary ratios without a re-dispatch. Re-evaluation is serial because the optimizer calls it under its GPU mutex inside a Rayon worker.

**Statistics.** `GpuS2Pipeline::certification_stats` / `GpuVoxelPipeline::certification_stats` return `GpuCertificationStats { queries, uncertain, list_regrowths, cpu_recompute_seconds }` (MC queries are valid samples; voxel queries are cells). `measure` prints the MC ratio after each GPU MC evaluation, GPU exact prints the voxel ratio in its `[Info] GPU exact S2` line, and `optimize` prints the shared instance's cumulative ratio before `Optimization completed.`

**What it does not claim.** The certificate is against the CPU predicate at the GPU's f32 query point in the origin-shifted f64 frame; it is not a statement about the unshifted world-frame CPU path (whose own rounding at `1e9` offsets differs) and not a change of MC sample positions. Growth of an uncertain list beyond its planned capacity is bounded by device limits, not by the logical memory budget (`mc_evaluation_peak` and `ExactMemoryPlan` count the planned list and its staging, and the MC planner counts a retained regrown list on the next evaluation).

**Measured (llvmpipe, release, 1 warmup + 5 alternating samples, shared 4-core host).** Recompute ratios: MC 1.6e-3 (level-3 icosphere, 1280 faces) and 2.2e-3 (`particles.stl`, 5600 faces); voxel 1.4e-3 (icosphere, 64³) and 1.3e-2 (`particles.stl`, 62×59×64). Host recompute is under 2 % of the certified run. The certified shaders cost about twice the ALU per triangle: median MC 0.174 s vs 0.137 s uncertified (icosphere) and 3.98 s vs 1.65 s (particles); voxel 0.450 s vs 0.585 s (icosphere, the certified path was faster in this run) and 4.66 s vs 2.22 s (particles). These are software-rasterizer timings on a loaded host and say nothing about hardware GPUs. On adversarial fixtures the uncertified shader is wrong where the certified one is exact: a 3e-7-thick slab gives 743 wrong voxels of 4096 and MC hit counts of 267/101/29/8 instead of 0.

## CPU exact planner calibration (PERF-07)

Release medians of five samples (`exact_cost_model_calibration`, seconds, FFT vs direct on the same grid; the old fixed rule would have chosen FFT for all of them):

| grid | r_max | workers | FFT | direct | planner choice |
|---|---:|---:|---:|---:|---|
| 32³ | 2 | 1 | 0.006317 | 0.001014 | direct |
| 32³ | 8 | 1 | 0.006575 | 0.025101 | FFT |
| 64³ | 2 | 1 | 0.085595 | 0.007118 | direct |
| 64³ | 6 | 1 | 0.079422 | 0.090754 | FFT |
| 96³ | 3 | 1 | 0.427821 | 0.050650 | direct |
| 128×128×32 | 4 | 1 | 0.197231 | 0.065435 | direct |
| 32³ | 2 | 4 | 0.006848 | 0.000456 | direct |
| 32³ | 8 | 4 | 0.006947 | 0.009020 | FFT |
| 64³ | 2 | 4 | 0.117273 | 0.003751 | direct |
| 64³ | 6 | 4 | 0.139758 | 0.051895 | direct |
| 96³ | 3 | 4 | 0.498781 | 0.033485 | direct |
| 128×128×32 | 4 | 4 | 0.319237 | 0.042846 | direct |

With the fitted constants the planner picks the faster kernel in all twelve cases. The host is shared with other jobs, so the 4-worker FFT figures (no speedup over 1 worker) may understate FFT scaling on an idle machine; the model therefore gives FFT no parallel credit, which errs toward FFT only when direct is close.

Smooth padding (`smooth_padding_benchmark`, 4 workers, medians of five, forward+power+inverse seconds, 2N-1 -> smooth): 50³ 0.024585 -> 0.023407; 64³ 0.086713 -> 0.076775; 71×67×53 0.074856 -> 0.073318; 100×100×20 0.052879 -> 0.036489.

**Radius batching and shared device (PERF-03/05/08).** GPU MC no longer has an `r_max < 128` limit: evaluation runs in radius batches of at most 128 (the WGSL `radii` array size), each batch passing its `radius_base`, and the shader keys its RNG on the global id `radius * samples + sample`, so fixed-seed integer counts are identical for any batch size and independent of `r_max`. Only `(r_max + 1) * samples <= u32::MAX` remains. The CPU merge of workgroup partials is O((r_max+1) * ceil(samples/256)). All GPU pipelines now share one process-wide logical device per `RUSTMSPT_GPU_DEVICE` selector and compile each shader once per device; see `reference/gpu.md` "Shared device and pipeline cache (PERF-03)". Measure and optimize now offer GPU MC for any `r_max`.
