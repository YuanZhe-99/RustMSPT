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

**CPU path (`rotate_and_crop`).** The output volume is filled slice-by-slice along z, with the
slices processed in parallel via `rayon`'s `par_chunks_mut` (one parallel task per z-slice, or
z-chunk sized to `out_w * out_h`). Within a slice the x/y loop runs sequentially.

**GPU path (`rotate_and_crop_gpu`, feature-gated).**

> **Feature-gated:** only compiled and reachable when the crate is built with the `gpu` Cargo
> feature. Without it, `CropPipeline::run` always takes the CPU path above.

When the `gpu` feature is enabled, `CropPipeline::run` chooses between the CPU and GPU paths based
on the *output* voxel count: it computes `out_total = out_w * out_h * out_d` from the rotated
bounding box and dispatches to the GPU only when `out_total > 100_000` voxels. For smaller outputs
it uses the CPU path directly (GPU dispatch overhead isn't worth it at that scale). The GPU path
converts the `i64` volume data to `i32` for upload, initializes/reuses a
`GpuVolumeTransformPipeline` (see [gpu.md](../reference/gpu.md)), dispatches a WGSL compute shader
that performs the same inverse-rotation sampling described above (nearest or trilinear, selected by
a `0`/`1` interpolation flag passed to the shader), and converts the result back to `i64` on
download. If GPU initialization or dispatch fails for any reason, `CropPipeline::run` catches the
error, logs a warning, and **falls back to the CPU path** rather than aborting the pipeline.

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
4. **Rotate and crop**: on GPU-enabled builds, dispatch to GPU when the rotated output would exceed
   100,000 voxels, falling back to CPU automatically on GPU failure; otherwise always use the CPU
   path.
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
