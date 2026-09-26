# PCA-Based Volume Alignment and Crop

The crop pipeline (`src/pipeline/crop.rs`) takes a raw CT (computed tomography) reconstruction —
a 3D grid of voxels containing both the scanned sample and the mounting/background material
around it — and produces a tightly cropped, axis-aligned volume containing just the sample. This
document explains the algorithm behind that transformation: why a naive axis-aligned crop doesn't
work, and how the pipeline uses Principal Component Analysis (PCA) on the foreground voxel cloud
to find the sample's true orientation before cropping.

## The problem: samples are never aligned to the scan axes

A CT scanner produces a volume whose `(x, y, z)` axes are defined by the scanner geometry, not by
the sample. In practice a sample sits in its mount at some arbitrary tilt — a few degrees off in
one case, an elongated rod lying diagonally across the field of view in another. If you crop that
volume with an axis-aligned bounding box, one of two things happens: either the box is inflated to
contain the tilted sample (wasting a large amount of the output volume on background voxels that
downstream analysis has to skip over), or a tight box clips corners of the sample outright.

The fix is to first estimate the sample's *natural* orientation — the axes along which its mass is
most and least spread out — and rotate the volume so those axes become the coordinate axes. Once
the sample is axis-aligned in this sense, an axis-aligned bounding box is also the *tightest*
possible box, and cropping to it wastes little space. `CropPipeline` implements exactly this
pipeline: detect background → estimate orientation via PCA → rotate into the new frame while
cropping → trim any reconstruction artifacts left at the new border → save.

## Step 1: background detection (`detect_background_mode`)

Before anything else, the pipeline needs to know which voxel value means "not sample." It doesn't
assume a specific value (e.g. 0) because the actual background/mounting-material intensity depends
on the scan and its numeric encoding. Instead it exploits a structural assumption: **the outer
boundary shell of the volume (the six faces, one voxel thick) is overwhelmingly background**, since
samples are mounted away from the scan volume's edges to avoid exactly the clipping problem this
pipeline solves. `detect_background_mode` tallies a histogram of voxel values found only on that
boundary shell and returns the mode (most frequent value). This becomes the reference `background`
value used by every subsequent step — foreground is simply "any voxel whose value differs from
`background`."

See the reference entry: [`detect_background_mode`](../reference/pipeline-crop-and-splitfilter.md#detect_background_mode).

## Step 2: PCA orientation estimation (`estimate_pca_bbox`)

This is the core of the algorithm. Once background is known, every non-background voxel's
`(x, y, z)` position is treated as a 3D data point, and PCA is applied to that point cloud to find
its dominant axes of spatial variance.

### Intuition

Picture the foreground voxels as a cloud of points shaped like the sample — for instance, an
elongated, slightly tilted blob. PCA answers the question "if I could rotate this cloud freely,
which orientation makes it look most naturally axis-aligned?" It does this by finding the three
mutually perpendicular directions along which the point cloud's spread (variance) is, respectively,
largest, second-largest, and smallest. Rotating the cloud so these directions coincide with the
x/y/z axes is precisely the rotation that minimizes the axis-aligned bounding box around the
points — because any axis-aligned box has to be at least as wide as the true spread of the data
along each axis, and PCA's axes are exactly where that spread is decomposed into independent,
non-redundant directions.

### Mechanics

1. **Centroid.** Sum the `(x, y, z)` positions of all foreground voxels and divide by the count to
   get the mean position — the "center of mass" of the sample in voxel-index space.
2. **Covariance matrix.** For every foreground voxel, compute its offset from the centroid and
   accumulate the outer product of that offset with itself into a running 3×3 sum, then divide by
   the voxel count. The result is the covariance matrix of the foreground point cloud: its diagonal
   entries are the variances along x, y, z, and its off-diagonal entries capture how those axes
   co-vary (i.e., how tilted the cloud is relative to the raw scan axes).
3. **Eigendecomposition.** The covariance matrix is symmetric by construction, so
   `nalgebra::SymmetricEigen` is used to compute its eigenvectors and eigenvalues directly (this is
   both faster and numerically more robust than a general eigensolver for symmetric matrices). Each
   eigenvector is a direction in 3D space; its corresponding eigenvalue is the variance of the point
   cloud along that direction.
4. **Sort by eigenvalue, descending.** The three eigenvector/eigenvalue pairs are reordered so the
   first eigenvector points along the direction of greatest spread (the sample's "long axis"), the
   second along the next-greatest, and the third along the least.
5. **Force a right-handed frame.** The three sorted eigenvectors are assembled as columns into a
   3×3 rotation matrix. Because eigenvectors are only defined up to sign, the resulting matrix could
   come out as a reflection (determinant −1) rather than a proper rotation. The code checks
   `rot.determinant() < 0.0` and, if so, flips the sign of the third column — this guarantees a
   right-handed coordinate system without disturbing the (already-largest/second-largest) first two
   axes.
6. **Compute the rotated bounding box.** Using the *inverse* of this rotation (its transpose, since
   rotation matrices are orthonormal), every foreground voxel's centroid-relative position is
   transformed into the new, PCA-aligned frame. The running min/max of those transformed
   coordinates across all three axes gives the tight bounding box of the sample in the new frame.

The function returns the rotation matrix, the centroid, the rotated-frame min/max bounds, and the
foreground voxel count — everything the next step needs to actually resample the volume.

See the reference entry: [`estimate_pca_bbox`](../reference/pipeline-crop-and-splitfilter.md#estimate_pca_bbox).

## Step 3: rotate and crop (`rotate_and_crop` / `rotate_and_crop_gpu`)

With the rotation and bounding box known, the pipeline builds a brand-new, axis-aligned output
volume sized to exactly the rotated bounding box (plus a small floating-point stabilization step —
`stabilize_bound`/`float_bounds_to_inclusive_i64` — that snaps near-integer bounds to exact
integers so the output box doesn't grow by a spurious voxel from floating-point noise).

For every voxel in this *new* output grid, the algorithm works backwards: it applies the forward
rotation (`rot`, not its inverse) to the output voxel's centroid-relative local coordinates to find
where that point falls in the *original*, unrotated source volume, then samples the source volume
there. This is the standard inverse-mapping approach to image/volume resampling — it guarantees
every output voxel gets a value (no holes), unlike a forward-mapping approach that could leave gaps
where source voxels don't land exactly on output grid points.

Two interpolation modes control how a non-integer source coordinate is sampled
(config field `interpolation`, values `"nearest"` or `"trilinear"`; **default is `trilinear`** —
see `parse_interpolation_mode`):

- **Nearest-neighbor** (`sample_nearest`) rounds the source coordinate to the closest integer voxel
  and copies its value directly. It is cheap and preserves exact original voxel values, but
  produces blockier, staircase-like edges after rotation.
- **Trilinear** (`sample_trilinear`) reads the eight surrounding integer voxels and blends them
  with weights proportional to distance along each axis, producing smoother, more accurate output
  at the cost of ~8x the sampling work per voxel.

Any sampled coordinate that falls outside the source volume's bounds is treated as `background`
(`sample_voxel_or_background`), so the new axis-aligned box is padded with background wherever the
tilted original volume doesn't cover it.

**CPU path.** Resampling uses slice tasks when depth supplies enough work, otherwise row-aligned tiles targeting 4096 voxels in the configured Rayon pool, including single-slice outputs.

**GPU path.** `acceleration` and `RUSTMSPT_ACCELERATION` select CPU, GPU or auto. Auto uses `gpu_min_voxels` (default 250,000 output voxels); explicit GPU bypasses that threshold. The GPU transform runs as output tiles planned against the logical budget (`gpu_memory_limit_mb`) and the device's single-buffer limit: the whole output when it fits, otherwise the largest full-xy z slabs, then row groups of one slice, then x runs of one row. Each tile inverse-maps its 8 output corners into source space, takes that AABB, widens it by the interpolation halo (floor..floor+1, covering trilinear neighbours and nearest rounding) plus an f32 rounding margin, clamps it to the volume and uploads only that sub-block — rotations cross source slices, so the source is never cut by the output z range. The shader recomputes each voxel from its absolute output index and tests bounds against the full source, so tiled output is byte-identical to the single dispatch for both interpolations; a guard word turns any read outside the uploaded block into an error. The working-set estimate given to the policy is the plan's peak (2 × max block + max tile output + staging + guard + 160 parameter bytes); the budget causes CPU fallback (auto) or an error (gpu) only when even a single-voxel tile cannot fit. Unsupported values, missing devices, budget limits and runtime errors fall back only when `cpu_fallback` permits it. Nearest sampling requires signed i32 values; trilinear additionally requires input/background integers exactly representable in f32. Wide unsigned labels remain on CPU or produce an explicit error when fallback is forbidden. GPU half-integer rounding follows CPU's away-from-zero rule; arbitrary f32 transforms are not claimed bit-identical to f64 CPU transforms. Checked two-dimensional dispatch covers outputs exceeding one dispatch row. Adapter selection honors `RUSTMSPT_GPU_DEVICE`.

See the reference entries:
[`rotate_and_crop`](../reference/pipeline-crop-and-splitfilter.md#rotate_and_crop),
[`rotate_and_crop_gpu`](../reference/pipeline-crop-and-splitfilter.md#rotate_and_crop_gpu).

## Step 4: edge-artifact trimming (`infer_trim_pixels` / `resolve_trim_pixels` / `trim_volume_border`)

CT reconstruction algorithms can leave stray non-background artifact voxels right at the volume's
outer border (reconstruction noise, truncation artifacts, etc.), even after background detection
and PCA cropping. Left in place, these border artifacts would themselves look like "foreground" to
downstream steps and to a subsequent PCA pass, subtly biasing measurements. The pipeline addresses
this with an optional post-crop trim of the outermost 0, 1, or 2 voxels in x/y (the XY border faces
only — z/depth is left untouched).

- **`boundary_non_bg_ratio(volume, background, thickness)`** measures contamination: it scans the
  shell of voxels within `thickness` voxels of any of the volume's faces and returns the fraction
  of that shell that is *not* the background value. A higher ratio means more artifact voxels are
  sitting right at the border.
- **`infer_trim_pixels`** applies a simple heuristic to two shell thicknesses (1 voxel: `r1`, 2
  voxels: `r2`): if `r1 > 0.08 && r2 > 0.04`, trim 2 voxels; else if `r1 > 0.03`, trim 1 voxel;
  otherwise trim 0. The two-thickness check avoids over-trimming on a volume whose contamination is
  confined to a single thin shell.
- **`resolve_trim_pixels`** turns the config field `edge_trim` into an actual trim count:
  `-1` means "auto" (call `infer_trim_pixels`), `0`/`1`/`2` are explicit user-forced values, and any
  other value is a config error. Whatever value results is clamped to
  `min(requested, floor((width-1)/2), floor((height-1)/2), 2)` so a trim request can never consume
  the entire volume or exceed the hard cap of 2 voxels.
- **`trim_volume_border`** performs the actual crop: it slices `trim` voxels off each of the four
  XY-plane edges (all z-slices), producing a smaller volume with the same depth.

See the reference entries:
[`boundary_non_bg_ratio`](../reference/pipeline-crop-and-splitfilter.md#boundary_non_bg_ratio),
[`infer_trim_pixels`](../reference/pipeline-crop-and-splitfilter.md#infer_trim_pixels),
[`resolve_trim_pixels`](../reference/pipeline-crop-and-splitfilter.md#resolve_trim_pixels),
[`trim_volume_border`](../reference/pipeline-crop-and-splitfilter.md#trim_volume_border).

## Orchestration (`CropPipeline::run`)

`CropPipeline::run` ties the above steps together in order:

1. **Load** the input volume, either from a folder of RAW slices (`input.type = "raw"`, using
   `input.raw` for width/height/bits/signed/byte order) or from a TIFF file/folder
   (`input.type = "tiff"`/`"tif"`), optionally restricted to a slice range
   (`input.slice_start`/`input.slice_end`).
2. **Detect background** via `detect_background_mode`.
3. **Estimate orientation** via `estimate_pca_bbox`, obtaining the rotation, centroid, and rotated
   bounding box; logs the foreground voxel count and both the floating-point and stabilized integer
   rotated bounding box for diagnostics.
4. **Rotate and crop** according to acceleration, threshold, budget and fallback policy.
5. **Resolve and apply edge trim** via `resolve_trim_pixels` (respecting `config.edge_trim`) and
   `trim_volume_border`.
6. **Save** the final cropped, trimmed, axis-aligned volume as a TIFF (single file or folder of
   slices, per `output.path`/`output.folder_prefix`/`output.folder_extension`).

Every stage prints an `[Info]` diagnostic line (shape, background value, interpolation mode,
foreground count, bounding box, trim pixel count, final shape), which is useful for sanity-checking
that PCA found a sensible orientation and that the trim heuristic didn't over- or under-correct.

Config fields referenced above are defined in `src/config/crop.rs`: `CropConfig` (`input`, `output`,
`interpolation`, `edge_trim`), `CropInput` (`type`, `path`, `slice_start`, `slice_end`, `raw`), and
`CropOutput` (`path`, `folder_prefix`, `folder_extension`).

See the reference entry: [`CropPipeline::run`](../reference/pipeline-crop-and-splitfilter.md#croppipelinerun).

## Cross-references

- [pipeline-crop-and-splitfilter.md](../reference/pipeline-crop-and-splitfilter.md) — full
  per-function reference for `pipeline/crop.rs`, including
  [`estimate_pca_bbox`](../reference/pipeline-crop-and-splitfilter.md#estimate_pca_bbox),
  [`rotate_and_crop`](../reference/pipeline-crop-and-splitfilter.md#rotate_and_crop), and
  [`CropPipeline::run`](../reference/pipeline-crop-and-splitfilter.md#croppipelinerun).
- [gpu.md](../reference/gpu.md) — reference for `GpuVolumeTransformPipeline` (`src/gpu/volume_transform.rs`),
  the WGSL compute pipeline `rotate_and_crop_gpu` dispatches to.


### Execution and allocation contract (2026-09-18)

`cpu_max` bounds the pool for the entire pipeline, including CPU fallback. `trim_volume_border` consumes its input: zero trim returns the same allocation; positive trim compacts retained rows in place and truncates the buffer. The retained capacity is not a second allocation and is released with the output. PCA uses fixed-block ordered reductions.


### Fixed-block statistics and boundary counting (PERF-14)

Background detection visits only the boundary faces, counting each edge/corner once even when a dimension is one. Ties select the smallest integer value; earlier hash-iteration-dependent ties were not reproducible. PCA originally kept three passes (centroid, centered covariance, projected bounds), using fixed 65,536-voxel blocks independent of worker count; the first two are now one moment pass (see below). Each block scans in original voxel order; indexed partial results are merged in ascending block order. Floating-point grouping differs from the old whole-volume serial reduction, so arbitrary inputs are not promised byte-identical to it. Tests compare an asymmetric sample against the serial oracle and require identical results across worker counts for symmetric, planar, linear and single-point foregrounds. The canonical basis for nearly repeated eigenspaces and the online centered covariance were added later (see below). PCA aligns principal variance axes; it does not generally compute the globally minimum-volume oriented bounding box.

For scheduling, volumes below 1,048,576 voxels and single-worker pools process the same fixed blocks serially; larger volumes use the existing pool. This changes scheduling only, preserving block boundaries and merge order.

PCA parallel scheduling groups fixed blocks with a minimum grain derived from `min(pool_workers, ceil(voxel_count / 1,048,576))`. This limits scheduling overhead without changing any block statistic or its merge order; it does not create another pool.

### Dense background counts (2026-09-23)

`for_each_boundary_value` visits only faces, counting corners/edges once. Background mode uses 256 `usize` counters for U8/I8 and 65,536 counters for U16/I16 volumes with at least 65,536 voxels (at most 512 KiB on a 64-bit host). Smaller 16-bit and all 32-bit volumes retain the HashMap path. A checked index sends values outside the declared dense range into a sparse spill map; metadata is not used to truncate/reject arbitrary i64 values. Dense and sparse counts share a running mode with the original smallest-value tie break; no final full histogram scan is needed. Counters are local and released before PCA. Independent full-grid ordered-map oracles cover collapsed dimensions, both integer extrema, signed ranges and metadata mismatches. Real-input end-to-end evidence is tracked in PLAN.Performance.md@1349c46 §62.

## One-pass moments and canonical PCA frame (PERF-14)

`estimate_pca_bbox` now makes one statistics pass plus the projected-bounds pass. `foreground_row_blocks` walks the same fixed 65,536-voxel blocks and hands each contiguous row segment `(x0, y, z, values)` to the accumulator. For a row, the foreground count, `Σx` and `Σx²` are exact integers (`u64`/`u128`), so `MomentState::from_row` builds that row's exact mean and centered second moment (only the xx entry is nonzero, since y and z are constant). Rows are merged into the block's running `(count, mean, M2)` with Chan's parallel formula (`MomentState::merge`: `mean += δ·nb/n`, `M2 += M2b + δδᵀ·na·nb/n`), and block states are merged in ascending block order, so results are identical for any worker count and avoid the `E[x²]-E[x]²` cancellation. The covariance is `M2/count`. This is Welford's online update applied at row granularity (Welford is the `nb = 1` case of Chan's merge). `estimate_pca_bbox_three_pass` keeps the previous centroid + centered covariance + bounds implementation as a test oracle; `online_moments_match_three_pass_and_workers` requires frame < 1e-10, centroid < 1e-11, bounds < 1e-8 and identical nearest crop output on oblique samples, and bit-identical results at 1/2/8 workers.

`pca_frame(cov)` sorts eigenpairs by descending eigenvalue and groups adjacent eigenvalues whose gap is at most `PCA_DEGENERATE_REL_TOL = 1e-3` times the largest magnitude. A three-fold group yields exactly the scan axes. A two-fold group uses the scan axes x, y, z in order, projected into the eigenspace and Gram-Schmidt orthonormalized, accepting an axis only when its residual norm is at least 0.5 (two such axes always exist), so the basis no longer depends on solver noise. A non-degenerate column k is sign-fixed so its component along scan axis k is positive, which picks the frame closest to the scan axes; only when that component is below `PCA_SIGN_DIAGONAL_TOL = 1e-6` does the largest-magnitude component decide (lowest axis on exact ties). A negative determinant flips the non-degenerate column with the smallest diagonal magnitude (the least rotation given up), or column 2 when every column is degenerate. Tests: cube and sphere give the identity frame with and without an added voxel; cylinders along z and x give `[z, x, y]` and the identity with and without an added voxel; a tilted two-fold covariance with 1e-9 noise keeps the same frame.

The non-degenerate sign convention was needed because `SymmetricEigen`'s eigenvector signs flipped between the three-pass and one-pass covariance of an ordinary oblique sample (last-bit differences). The first version fixed each column's largest-magnitude component positive; on the repository's real CT RAW stack (principal axis 50 degrees from x in the scan plane) that flipped column 1, the right-handedness fix then sent column 2 to -z, and the crop came out turned 180 degrees with its slice axis reversed. The diagonal rule returns the pure 50-degree rotation about z, and the crop output is byte-identical to the pre-2026-09-25 baseline (PLAN.Performance.md@1349c46 §68). Test: `frame_keeps_the_scan_orientation_of_a_sample_rotated_about_z`.

Release benchmark (`pca_online_benchmark`, medians of five samples, each 10 repeats, three-pass -> one-pass seconds): 205,349 voxels 1/2/4 workers 0.005596->0.003662, 0.005614->0.003663, 0.004742->0.003084; 1,642,792 voxels 0.043084->0.027811, 0.025081->0.018758, 0.034447->0.022170; 13,142,336 voxels 0.580528->0.402514, 0.609000->0.408049, 0.214946->0.146319. The one-pass form was faster in every case measured.
