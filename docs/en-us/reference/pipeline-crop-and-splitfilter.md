# Pipeline: Crop and Split-Filter Reference

Covers the `crop` pipeline (`src/pipeline/crop.rs`) — background detection, PCA alignment, rotate+crop, edge-artifact trimming for CT volumes — and the `split_filter` pipeline (`src/pipeline/split_filter.rs`) — connected-component splitting plus geometric/statistical particle filtering.

## Index

| Item | Location | Summary |
|---|---|---|
| `gpu_crop_values_supported` | `src/pipeline/crop.rs:25` | Check exact integer representation for GPU interpolation. |
| `CropPipeline::run_in_pool` | `src/pipeline/crop.rs:621` | Execute crop stages within the configured pool and report completed-stage wall times. |
| `CropPipeline` (struct) | `src/pipeline/crop.rs:14` | Holds `CropConfig` for the crop pipeline. |
| `InterpolationMode` (enum) | `src/pipeline/crop.rs:19` | Nearest vs. trilinear resampling mode used during rotate+crop. |
| `parse_byte_order` | `src/pipeline/crop.rs:34` | Parses `little`/`big` (or `le`/`be`) into a `ByteOrder`. |
| `parse_interpolation_mode` | `src/pipeline/crop.rs:50` | Parses `nearest`/`trilinear` into an `InterpolationMode`, defaulting to trilinear. |
| `load_input_volume` | `src/pipeline/crop.rs:71` | Loads the input CT volume from a raw folder or TIFF/TIFF-folder per config. |
| `voxel_index` | `src/pipeline/crop.rs:105` | Computes the flat data index for `(x, y, z)` voxel coordinates. |
| `sample_voxel_or_background` | `src/pipeline/crop.rs:110` | Reads a voxel at integer coordinates, returning the background value if out of bounds. |
| `sample_nearest` | `src/pipeline/crop.rs:137` | Nearest-neighbor sample at fractional source coordinates. |
| `sample_trilinear` | `src/pipeline/crop.rs:145` | Trilinear-interpolated sample at fractional source coordinates. |
| `stabilize_bound` | `src/pipeline/crop.rs:178` | Snaps a near-integer float to its exact integer within an epsilon. |
| `float_bounds_to_inclusive_i64` | `src/pipeline/crop.rs:188` | Converts float min/max bounds to an inclusive integer `[start, end]` range. |
| `boundary_non_bg_ratio` | `src/pipeline/crop.rs:201` | Fraction of non-background voxels within a boundary shell of given thickness. |
| `infer_trim_pixels` | `src/pipeline/crop.rs:242` | Heuristically infers 0/1/2 pixels of edge trim from boundary artifact intensity. |
| `resolve_trim_pixels` | `src/pipeline/crop.rs:260` | Resolves the effective edge-trim pixel count from config, supporting `-1` for auto. |
| `trim_volume_border` | `src/pipeline/crop.rs:287` | Trims a fixed number of border voxels from the XY faces of a volume. |
| `detect_background_mode` | `src/pipeline/crop.rs:317` | Detects the background value as the modal voxel value on the volume boundary. |
| `estimate_pca_bbox` | `src/pipeline/crop.rs:351` | Computes PCA rotation, centroid, and rotated-frame foreground bounding box. |
| `MomentState` | `src/pipeline/crop.rs` | Running count, mean and centered second-moment matrix. |
| `MomentState::from_row` | `src/pipeline/crop.rs` | Exact moments of one foreground row segment from integer sums. |
| `MomentState::merge` | `src/pipeline/crop.rs` | Chan parallel merge of two moment states. |
| `pca_frame` | `src/pipeline/crop.rs` | Sorted, sign-fixed, right-handed PCA frame with canonical near-degenerate eigenspaces. |
| `projected_bounds` | `src/pipeline/crop.rs` | Fixed-block rotated-frame foreground bounds. |
| `foreground_row_blocks` | `src/pipeline/crop.rs` | Fixed-block scan handing contiguous row segments to an accumulator. |
| `estimate_pca_bbox_three_pass` | `src/pipeline/crop.rs` | Test-only previous three-pass fixed-block PCA oracle. |
| `rotate_and_crop` | `src/pipeline/crop.rs:459` | CPU, rayon-parallel rotate-and-crop of the volume into an axis-aligned output. |
| `rotate_and_crop_gpu` | `src/pipeline/crop.rs` | Budget-planned, output-tiled GPU rotate-and-crop via `GpuVolumeTransformPipeline` (feature `gpu`). |
| `CropSourceBlock` | `src/pipeline/crop.rs` | Clamped source sub-block (origin/dims) one output tile may read. |
| `CropTilePlan` | `src/pipeline/crop.rs` | Chosen tile shape, tile count, retained maxima and peak logical GPU bytes. |
| `CropTilePlanError` | `src/pipeline/crop.rs` | Planning refusal with an optional lower bound on the required bytes. |
| `crop_tile_source_block` | `src/pipeline/crop.rs` | Source AABB of a tile's 8 inverse-mapped corners plus interpolation halo and f32 margin. |
| `crop_gpu_peak_bytes` | `src/pipeline/crop.rs` | Logical GPU peak for retained max block/tile, queued upload, staging, params and guard. |
| `for_each_crop_tile` | `src/pipeline/crop.rs` | Visits whole-output tiles in z, y, x order. |
| `evaluate_crop_tiling` | `src/pipeline/crop.rs` | Checks one tile shape against the budget and device buffer limit. |
| `plan_crop_gpu_tiles` | `src/pipeline/crop.rs` | Largest z-slab / row / x-run tiling that fits the budget and limits. |
| `CropPipeline::run` | `src/pipeline/crop.rs:598` | Orchestrates load → background detect → PCA bbox → rotate+crop (GPU or CPU) → edge trim → save TIFF. |
| `SplitFilterPipeline` (struct) | `src/pipeline/split_filter.rs:13` | Holds `SplitFilterConfig` for the split-filter pipeline. |
| `VolumeStats` (struct) | `src/pipeline/split_filter.rs:18` | Min/max/mean/median summary of kept-particle volumes. |
| `volume_stats_for_kept` | `src/pipeline/split_filter.rs:26` | Computes `VolumeStats` over particles whose `keep` flag is true. |
| `count_kept` | `src/pipeline/split_filter.rs:54` | Counts `true` entries in a `keep` boolean slice. |
| `report_step` | `src/pipeline/split_filter.rs:59` | Appends a before/after/removed summary line for one filter step. |
| `append_volume_histogram` | `src/pipeline/split_filter.rs:71` | Appends a single text histogram of volume values. |
| `append_volume_histogram_comparison` | `src/pipeline/split_filter.rs:119` | Appends a side-by-side before/after text histogram of volume values. |
| `normal_cdf` | `src/pipeline/split_filter.rs:202` | Standard normal CDF, computed via `erf_approx`. |
| `erf_approx` | `src/pipeline/split_filter.rs:208` | Abramowitz & Stegun 7.1.26 approximation of the error function. |
| `apply_lognormal_rebalance` | `src/pipeline/split_filter.rs:225` | Drops excess particles from over-represented log-volume bins relative to a fitted lognormal. |
| `SplitFilterPipeline::run` | `src/pipeline/split_filter.rs:300` | Orchestrates split → aspect-ratio/sharpness/volume filters → save STLs → report. |

---

## `pipeline/crop.rs`

CT-volume crop pipeline. Loads a volume, detects the background intensity, computes a PCA-based rotation that aligns the foreground's principal axes to the coordinate axes, resamples the volume into that rotated+cropped frame, optionally trims residual border artifacts, and saves the result as TIFF.

### Public items

#### CropPipeline (struct)

- **Source:** `src/pipeline/crop.rs:14`
- **Purpose:** Wraps a `CropConfig` and implements `Pipeline` for the crop subcommand.
- **Fields:** `config: CropConfig`.

#### InterpolationMode (enum)

- **Source:** `src/pipeline/crop.rs:19`
- **Purpose:** Selects the resampling scheme used when mapping output voxels back to source coordinates.
- **Variants:** `Nearest`, `Trilinear`.
- **Notes:** `Debug, Clone, Copy, PartialEq, Eq`. Defaults to `Trilinear` when unset in config (see `parse_interpolation_mode`).

#### CropPipeline::run

- **Signature:** `fn run(&self) -> Result<()>`
- **Source:** `src/pipeline/crop.rs:598`
- **Purpose:** Execute the full crop pipeline end to end: load the CT volume, detect its background value, PCA-align the foreground, rotate and crop to an axis-aligned bounding box, optionally trim edge artifacts, and save the result.
- **Parameters:** Reads `self.config: CropConfig` — input type/path (raw or TIFF, with optional slice range and raw layout spec), `interpolation` mode string, `edge_trim` (-1/0/1/2), and output path/folder-prefix/folder-extension.
- **Returns:** `Ok(())` on success; `RustMsptError::InvalidConfig` for bad config values or a too-large edge trim; propagates I/O and mesh/volume errors.
- **Side effects:** Reads the input volume from disk (`load_input_volume`); on the `gpu` feature, may initialize a GPU pipeline and dispatch a compute pass; writes the cropped volume as TIFF (or TIFF folder) via `save_tiff_or_folder_with_ext`; prints `[Info]`/`[Warning]` progress lines to stdout at every stage (input shape, detected background, interpolation mode, foreground voxel count, rotated bbox, integer bbox, edge trim pixels, output shape, output path).
- **Notes:** Pipeline stages, in order:
  1. `load_input_volume` — load raw or TIFF input.
  2. `detect_background_mode` — find the modal boundary voxel value.
  3. `parse_interpolation_mode` — resolve nearest vs. trilinear from config.
  4. `estimate_pca_bbox` — compute PCA rotation, centroid, and rotated-frame foreground bounds.
  5. Rotate+crop using the configured execution policy; failures fall back only when permitted.
  6. `resolve_trim_pixels` + `trim_volume_border` — optional edge-artifact trim (auto-detected when `edge_trim == -1`).
  7. `save_tiff_or_folder_with_ext` — write output.
- **See also:** `estimate_pca_bbox`, `rotate_and_crop`, `rotate_and_crop_gpu`; algorithm details in [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md); GPU dispatch details in [gpu.md](gpu.md).

> **Algorithm:** See [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md) for the full PCA-alignment-and-crop algorithm description (this covers `estimate_pca_bbox`, `rotate_and_crop`, and how `CropPipeline::run` composes them).

### Private helpers

#### parse_byte_order

- **Signature:** `fn parse_byte_order(value: Option<&str>) -> Result<ByteOrder>`
- **Source:** `src/pipeline/crop.rs:34`
- **Purpose:** Parse a raw-volume byte-order config string into `io::ByteOrder`.
- **Parameters:** `value` — `"little"`/`"le"` or `"big"`/`"be"` (case-insensitive, trimmed); defaults to `"little"` when `None`.
- **Returns:** `Ok(ByteOrder::LittleEndian)`, `Ok(ByteOrder::BigEndian)`, or `Err(InvalidConfig)` for any other string.
- **Side effects:** None.

#### parse_interpolation_mode

- **Signature:** `fn parse_interpolation_mode(value: Option<&str>) -> Result<InterpolationMode>`
- **Source:** `src/pipeline/crop.rs:50`
- **Purpose:** Parse the crop config's interpolation-mode string.
- **Parameters:** `value` — `"nearest"` or `"trilinear"` (case-insensitive, trimmed); defaults to `"trilinear"` when `None`.
- **Returns:** `Ok(InterpolationMode)` or `Err(InvalidConfig)` for unrecognized strings.
- **Side effects:** None.

#### load_input_volume

- **Signature:** `fn load_input_volume(config: &CropConfig) -> Result<Volume3D>`
- **Source:** `src/pipeline/crop.rs:71`
- **Purpose:** Load the crop pipeline's input volume according to `config.input.type`.
- **Parameters:** `config` — the full `CropConfig`; reads `input.type` (`"raw"` or `"tiff"`/`"tif"`), `input.path`, `input.slice_start`/`slice_end` (defaulting to -1, meaning "no clamp"), and, for raw input, `input.raw` (width/height/bits/signed/byte_order).
- **Returns:** A loaded `Volume3D`.
- **Side effects:** Reads file(s) from disk (a folder of raw slices, or a TIFF file/folder).
- **Notes:** Returns `RustMsptError::InvalidConfig` if `input.type=raw` but `input.raw` is missing, or if `input.type` is neither `raw` nor `tiff`/`tif`.

#### voxel_index

- **Compact form:** `fn voxel_index(width: usize, height: usize, x: usize, y: usize, z: usize) -> usize` — `src/pipeline/crop.rs:86`. Returns the flat data index `z*width*height + y*width + x` for a `Volume3D`'s row-major/slice-major layout. Pure, no side effects.

#### sample_voxel_or_background

- **Signature:** `fn sample_voxel_or_background(volume: &Volume3D, background: i64, x: isize, y: isize, z: isize) -> i64`
- **Source:** `src/pipeline/crop.rs:110`
- **Purpose:** Read a single voxel at integer coordinates, treating any out-of-bounds coordinate as background.
- **Parameters:** `volume`, `background` (fill value), `x`/`y`/`z` (signed, may be negative or beyond volume extent).
- **Returns:** The voxel value, or `background` if any coordinate is negative or `>=` the corresponding dimension.
- **Side effects:** None.

#### sample_nearest

- **Signature:** `fn sample_nearest(volume: &Volume3D, background: i64, src_x: f64, src_y: f64, src_z: f64) -> i64`
- **Source:** `src/pipeline/crop.rs:137`
- **Purpose:** Nearest-neighbor resampling at fractional source coordinates.
- **Parameters:** fractional `src_x/src_y/src_z` in source-volume space.
- **Returns:** Rounds each coordinate to the nearest integer and delegates to `sample_voxel_or_background`.
- **Side effects:** None.

#### sample_trilinear

- **Signature:** `fn sample_trilinear(volume: &Volume3D, background: i64, src_x: f64, src_y: f64, src_z: f64) -> i64`
- **Source:** `src/pipeline/crop.rs:145`
- **Purpose:** Trilinear-interpolated resampling at fractional source coordinates.
- **Parameters:** fractional `src_x/src_y/src_z`.
- **Returns:** The trilinear blend of the 8 surrounding voxels (each individually falling back to `background` if out of bounds via `sample_voxel_or_background`), rounded to the nearest `i64`.
- **Side effects:** None.
- **Notes:** Standard trilinear formula: interpolate along x for each of the 4 edges, then along y for the 2 resulting values, then along z.

#### stabilize_bound

- **Compact form:** `fn stabilize_bound(value: f64, eps: f64) -> f64` — `src/pipeline/crop.rs:147`. Snaps `value` to `value.round()` when the two differ by no more than `eps`; otherwise returns `value` unchanged. Used to counteract floating-point drift in PCA-rotated bounding-box coordinates that are conceptually integers. Pure.

#### float_bounds_to_inclusive_i64

- **Signature:** `fn float_bounds_to_inclusive_i64(min_v: f64, max_v: f64, eps: f64) -> (isize, isize)`
- **Source:** `src/pipeline/crop.rs:188`
- **Purpose:** Convert a floating-point `[min_v, max_v]` bound into an inclusive integer voxel range.
- **Parameters:** `min_v`/`max_v` — float bounds (e.g. from `estimate_pca_bbox`); `eps` — stabilization tolerance passed to `stabilize_bound`.
- **Returns:** `(start, end)` where `start = floor(stabilize(min_v))` and `end = ceil(stabilize(max_v))`.
- **Side effects:** None.
- **Notes:** Called with `eps = 1e-3` throughout the module.

#### boundary_non_bg_ratio

- **Signature:** `fn boundary_non_bg_ratio(volume: &Volume3D, background: i64, thickness: usize) -> f64`
- **Source:** `src/pipeline/crop.rs:201`
- **Purpose:** Measure how much of a volume's outer shell (of a given thickness) is non-background — a proxy for leftover rotation/resampling edge artifacts.
- **Parameters:** `volume`, `background`, `thickness` — shell thickness in voxels from each face.
- **Returns:** Ratio in `[0, 1]`; `0.0` if `thickness == 0` or the volume has no shell voxels.
- **Side effects:** None. Iterates every voxel in the volume, classifying it as "on shell" if it's within `thickness` of any of the 6 faces.
- **Notes:** Not parallelized; called twice per volume by `infer_trim_pixels` (thickness 1 and 2), so cost is O(volume size) per call.

#### infer_trim_pixels

- **Signature:** `fn infer_trim_pixels(volume: &Volume3D, background: i64) -> usize`
- **Source:** `src/pipeline/crop.rs:242`
- **Purpose:** Heuristically decide how many pixels of XY border to trim, based on boundary-shell artifact intensity.
- **Parameters:** `volume` (typically the rotated+cropped output), `background`.
- **Returns:** `2` if `r1 > 0.08 && r2 > 0.04`; else `1` if `r1 > 0.03`; else `0`, where `r1`/`r2` are `boundary_non_bg_ratio` at thickness 1 and 2 respectively.
- **Side effects:** None.
- **Notes:** This is the `-1` ("auto") behavior for `edge_trim` in config, consumed by `resolve_trim_pixels`.

#### resolve_trim_pixels

- **Signature:** `fn resolve_trim_pixels(config_value: Option<i32>, volume: &Volume3D, background: i64) -> Result<usize>`
- **Source:** `src/pipeline/crop.rs:260`
- **Purpose:** Resolve the effective edge-trim pixel count from the `edge_trim` config value, supporting auto-detection.
- **Parameters:** `config_value` — `-1` (auto via `infer_trim_pixels`), `0`, `1`, or `2`; defaults to `0` when `None`. `volume`/`background` — used both for auto-inference and for clamping.
- **Returns:** `Ok(usize)` clamped to `min(requested, min(width-1, height-1)/2, 2)`; `Err(InvalidConfig)` for any value outside `{-1, 0, 1, 2}`.
- **Side effects:** None.
- **Notes:** The clamp guarantees `trim_volume_border` never receives a trim value that would eliminate the entire XY extent.

#### trim_volume_border

- **Signature:** `fn trim_volume_border(volume: Volume3D, trim: usize) -> Result<Volume3D>`
- **Source:** `src/pipeline/crop.rs:287`
- **Purpose:** Strip `trim` voxels from all four XY-face edges (not the Z/depth faces) of a volume.
- **Parameters:** `volume`, `trim` — pixels to remove from each of the four X/Y sides.
- **Returns:** `Ok(volume)` unchanged when `trim == 0`; otherwise the same allocation compacted into a smaller `Volume3D` with `width -= 2*trim`, `height -= 2*trim`, same `depth`; `Err(InvalidConfig)` if `width <= 2*trim || height <= 2*trim`.
- **Side effects:** None (allocates a new data buffer).
- **Notes:** Depth (Z) is never trimmed — only the X/Y (in-plane) border, matching the fact that edge artifacts in this pipeline arise from XY rotation/resampling, not from slice truncation.

#### detect_background_mode

- **Signature:** `fn detect_background_mode(volume: &Volume3D) -> i64`
- **Source:** `src/pipeline/crop.rs:317`
- **Purpose:** Detect the CT volume's background intensity as the most frequent voxel value on the volume's boundary faces.
- **Parameters:** `volume`.
- **Returns:** The modal boundary-voxel value; `0` if the volume has no boundary voxels (degenerate/empty volume).
- **Side effects:** None. Counts each boundary voxel once using bounded dense counters for eligible 8/16-bit images and a sparse map otherwise; see the dense background contract below.
- **Notes:** Assumes the background dominates the boundary — a reasonable assumption for CT scans where the specimen doesn't touch the volume edges. Ties deterministically choose the smallest voxel value.

#### estimate_pca_bbox

- **Signature:** `fn estimate_pca_bbox(volume: &Volume3D, background: i64) -> Result<(Matrix3<f64>, Vector3<f64>, Vector3<f64>, Vector3<f64>, usize)>`
- **Source:** `src/pipeline/crop.rs:351`
- **Purpose:** Compute a principal-component rotation that aligns the foreground's dominant axes to the coordinate axes, and the foreground's bounding box in that rotated frame.
- **Parameters:** `volume`, `background` — the value to exclude as background.
- **Returns:** `Ok((rot, centroid, min_v, max_v, count))`:
  - `rot: Matrix3<f64>` — orthonormal rotation matrix from `pca_frame` (columns = covariance eigenvectors sorted by descending eigenvalue, sign-fixed, canonical basis for near-degenerate eigenspaces, forced right-handed).
  - `centroid: Vector3<f64>` — mean position of foreground voxels.
  - `min_v`/`max_v: Vector3<f64>` — foreground bounding box in the rotated frame (i.e. `rot^T * (p - centroid)` for every foreground voxel `p`).
  - `count: usize` — number of foreground voxels.
  - `Err(InvalidConfig)` if no foreground voxels are found (all voxels equal `background`).
- **Side effects:** None. Two passes: (1) one-pass count/mean/M2 moments (exact per-row integer moments merged with Chan's formula within fixed blocks, blocks merged in order), (2) `projected_bounds`.
- **Notes:** See *One-pass moments and canonical PCA frame* below. Partitioning and results are independent of worker count.
- **See also:** [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md) for the algorithm write-up.

> **Algorithm:** See [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md).

#### rotate_and_crop

- **Signature:** `fn rotate_and_crop(volume: &Volume3D, background: i64, rot: &Matrix3<f64>, centroid: &Vector3<f64>, min_v: &Vector3<f64>, max_v: &Vector3<f64>, interpolation_mode: InterpolationMode) -> Volume3D`
- **Source:** `src/pipeline/crop.rs:459`
- **Purpose:** CPU rotate-and-crop: resample the source volume into a new axis-aligned volume covering exactly the rotated-frame foreground bounding box.
- **Parameters:** `volume`, `background`; `rot`/`centroid` from `estimate_pca_bbox`; `min_v`/`max_v` rotated-frame bounds; `interpolation_mode` (`Nearest` or `Trilinear`).
- **Returns:** A new `Volume3D` of size `(x1-x0+1, y1-y0+1, z1-z0+1)` (each dimension floored to at least 1), initialized to `background` and filled by inverse-mapping each output voxel back into source space.
- **Side effects:** None (pure compute). Uses `float_bounds_to_inclusive_i64` (`eps = 1e-3`) to fix the integer output extent.
- **Notes:** Parallelized over slice tasks when depth supplies enough work, otherwise row-aligned tiles targeting 4096 voxels in the configured Rayon pool. For each output voxel `(x, y, z)` in the rotated local frame, the corresponding source-space coordinate is `rot * local + centroid`, sampled with `sample_nearest` or `sample_trilinear` depending on `interpolation_mode`.
- **See also:** `rotate_and_crop_gpu` (GPU counterpart, feature `gpu`); [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md).

> **Algorithm:** See [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md).

#### rotate_and_crop_gpu

> **Feature-gated:** compiled only with `--features gpu` (`#[cfg(feature = "gpu")]`).

- **Signature:** `fn rotate_and_crop_gpu(volume: &Volume3D, background: i64, rot: &Matrix3<f64>, centroid: &Vector3<f64>, min_v: &Vector3<f64>, max_v: &Vector3<f64>, interpolation_mode: InterpolationMode, budget: Option<u64>) -> std::result::Result<(Volume3D, usize), String>`
- **Source:** `src/pipeline/crop.rs`
- **Purpose:** GPU rotate-and-crop executed as output tiles chosen by `plan_crop_gpu_tiles`; each tile uploads only the source sub-block its samples can reach.
- **Parameters:** Same as `rotate_and_crop`, plus `budget` — the logical GPU byte budget (`acceleration.gpu_memory_limit_mb × 1 MiB`), or `None` for no budget.
- **Returns:** `Ok((Volume3D, tiles))` with the same shape/semantics as the CPU path and the number of dispatched tiles, or `Err(String)` for initialization, planning (`"... no single minimal tile fits (needs at least N bytes)"`), dispatch, readback or halo-guard failures.
- **Side effects:** Initializes a `GpuVolumeTransformPipeline`, plans with both the budget and the device's single-buffer limit (`min(max_buffer_size, max_storage_buffer_binding_size)`), pre-sizes source and output/staging buffers once to the plan maxima, then per tile converts the block `i64 → i32` (checked), dispatches `transform_tile`, and writes the tile rows into the host output. Prints `"[Info] GPU volume transform: {s}s, {src} -> {out}, tiles=N tile=WxHxD peak_bytes=B"`.
- **Notes:** Output is byte-identical to the single-dispatch GPU path for both interpolations because the shader recomputes every voxel from its absolute output index (`f32(tile_offset + local)`) and bounds-tests against the full source dims; the block only changes where in-volume values are read from. A runtime guard word turns any in-volume read outside the uploaded block into an error instead of a value. With no budget and ample device limits the plan is one tile whose block is the output's source AABB (not necessarily the whole source). Nearest output equals the CPU path except where f32 and f64 coordinates round differently at an exact `.5` tie (oblique rotations); trilinear f32 blending is not claimed bit-identical to CPU f64.
- **See also:** [gpu.md](gpu.md) for `GpuVolumeTransformPipeline` and the underlying WGSL compute shader; `rotate_and_crop` for the CPU fallback path; `CropPipeline::run` for the GPU/CPU selection logic and fallback-on-error behavior.

#### Crop GPU tile planning (`CropSourceBlock`, `CropTilePlan`, `CropTilePlanError`, `crop_tile_source_block`, `crop_gpu_peak_bytes`, `for_each_crop_tile`, `evaluate_crop_tiling`, `plan_crop_gpu_tiles`)

- **Source:** `src/pipeline/crop.rs` (compiled without the `gpu` feature too: `CropPipeline::run_in_pool` uses the plan as the working-set estimate).
- **`crop_tile_source_block(src_dims, rot, centroid, origin, lo, hi) -> Result<CropSourceBlock, String>`** — inverse-maps the tile's 8 corners (`src = rot * (origin + index) + centroid`, the affine image's AABB), expands to `floor(min − m) .. floor(max + m) + 1` per axis (covers both trilinear neighbours and nearest rounding, i.e. a 1-voxel halo), and clamps to the source. `m = 16·ε_f32·(Σ|r_ij|·(max|local_j|+1) + |c_i| + 1) + 1e-6` bounds the shader's f32 rounding of rotation, centroid, origin and products. A tile whose samples all leave the source gets a zero-dim block. Nonfinite coordinates are an error. Rotations cross source slices, so the block is never the output's z range.
- **`crop_gpu_peak_bytes(max_block_voxels, max_tile_voxels) -> Option<u64>`** — `2·4·block` (buffer plus the queued `write_buffer` copy) `+ 4·tile` (output) `+ 4·tile + 4` (staging with the guard word) `+ 160` (params) `+ 4 + 4` (guard), all checked.
- **`for_each_crop_tile(out_dims, tile_dims, visit)`** — visits inclusive `(lo, hi)` bounds in z, y, x order; tail tiles are shorter when sizes do not divide.
- **`evaluate_crop_tiling(...) -> Result<CropTilePlan, CropTilePlanError>`** — accumulates the retained maxima over all tiles (buffers are pre-sized once, so the peak is the maxima, not one tile) and refuses at the first tile that exceeds the budget, the single-buffer limit, or u32 voxel indexing.
- **`plan_crop_gpu_tiles(src_dims, rot, centroid, origin, out_dims, budget, buffer_limit) -> Result<CropTilePlan, CropTilePlanError>`** — tries the whole output, then full-xy z slabs, then y-row groups of one slice, then x runs of one row; within a level it binary-searches the largest extent that evaluates as fitting, and only returns plans evaluated over all their tiles. Refuses only when a single-voxel x run does not fit; the error's `needed` is a lower bound used as the policy estimate, so `resolve_execution` reports the budget refusal and auto falls back / gpu errors.

---

## `pipeline/split_filter.rs`

Connected-component split and geometric/statistical filtering pipeline. Splits one or more input STL meshes into disjoint particle ("granule") meshes, then applies a chain of optional filters (aspect ratio, sharpness, volume range or lognormal rebalancing), saving the surviving particles as individual STL files plus a text report.

### Public items

#### SplitFilterPipeline (struct)

- **Source:** `src/pipeline/split_filter.rs:13`
- **Purpose:** Wraps a `SplitFilterConfig` and implements `Pipeline` for the split_filter subcommand.
- **Fields:** `config: SplitFilterConfig`.

#### SplitFilterPipeline::run

- **Signature:** `fn run(&self) -> Result<()>`
- **Source:** `src/pipeline/split_filter.rs:300`
- **Purpose:** Run the full split-and-filter pipeline: load input mesh(es), split into connected-component particles, apply the configured filter chain, save kept particles as STL files, and write a text report.
- **Parameters:** Reads `self.config: SplitFilterConfig` — `input.path` (a single STL file or a directory of STLs), `output.folder`/`output.prefix`/`output.report_path` (report path defaults to `<output.folder's parent>/split_filter_report.txt`), and an optional `filter` section (`enabled`, `max_aspect_ratio`, `max_sharpness_ratio`, `volume` sub-config with `mode: "range" | "lognormal_rebalance" | "none"`).
- **Returns:** `Ok(())` on success; `RustMsptError::InvalidConfig` if `output.prefix` is empty; `RustMsptError::InvalidMesh` if splitting yields zero particles, or if all particles are removed by filtering; propagates I/O errors from mesh/STL loading and saving.
- **Side effects:** Reads STL file(s) from disk (`load_stl` or `load_folder_stls`); creates `output.folder` (and the report's parent directory) if missing; writes one STL file per kept particle (named `{prefix}{rank+1}.stl`, 1-indexed by kept order); writes the text report to `report_path`; prints `[Info]` summary lines to stdout (particle counts, output folder/prefix, report path).
- **Notes:** Filter chain, applied in order, each stage only affecting particles still marked `keep`:
  1. **Split:** for each input mesh, `split_mesh_into_granules` decomposes it into connected-component sub-meshes ("granules"/particles). All resulting particles from all input files are pooled into one `Vec<Mesh>`.
  2. **Aspect ratio** (`filter.max_aspect_ratio`, optional): computed per-particle from `mesh_bbox` as `max_extent / min_extent.max(1e-12)`; particles exceeding the threshold are dropped.
  3. **Sharpness ratio** (`filter.max_sharpness_ratio`, optional): `sharpness = area^3 / (36 * PI * volume^2)` (isoperimetric-style shape ratio; 1.0 for a perfect sphere, larger for elongated/spiky shapes). Particles with `volume <= 1e-12` are dropped outright (degenerate mesh); others exceeding the threshold are dropped.
  4. **Volume filter** (`filter.volume`, optional): `mode = "range"` drops particles whose volume falls outside `[min, max]` (either bound skipped when negative/unset); `mode = "lognormal_rebalance"` delegates to `apply_lognormal_rebalance`; `mode = "none"` (or any other value) skips this stage.
  Each stage appends a `report_step` line (`before=N, after=M, removed=K`) to the report, or a `"skipped"` line if the stage's config is absent. If `filter` itself is `None`, or `filter.enabled == Some(false)`, all filtering is skipped entirely and every particle is kept.
  The report additionally includes kept-volume statistics (`volume_stats_for_kept`) and two text histograms (`append_volume_histogram`, `append_volume_histogram_comparison`, both fixed at 10 bins).
- **See also:** `apply_lognormal_rebalance`; `crate::geometry::split_mesh_into_granules`, `mesh_bbox`, `mesh_surface_area`, `mesh_volume` (see [geometry-analysis.md](geometry-analysis.md) / [geometry-core.md](geometry-core.md)).

### Private helpers

#### VolumeStats (struct)

- **Source:** `src/pipeline/split_filter.rs:18`
- **Purpose:** Holds min/max/mean/median summary statistics of kept-particle volumes for the report.
- **Fields:** `min: f64`, `max: f64`, `mean: f64`, `median: f64`.
- **Notes:** `Clone, Copy`.

#### volume_stats_for_kept

- **Signature:** `fn volume_stats_for_kept(volumes: &[f64], keep: &[bool]) -> Option<VolumeStats>`
- **Source:** `src/pipeline/split_filter.rs:26`
- **Purpose:** Compute `VolumeStats` over the subset of `volumes` whose parallel `keep` entry is `true`.
- **Parameters:** `volumes` — per-particle volume array; `keep` — parallel boolean keep-mask.
- **Returns:** `None` if no particles are kept; otherwise `Some(VolumeStats)` with `min`/`max` from the sorted extremes, `mean` as the arithmetic mean, and `median` computed from the sorted, kept-only values (average of the two middle elements for even counts).
- **Side effects:** None (sorts a local copy of the kept values).

#### count_kept

- **Compact form:** `fn count_kept(keep: &[bool]) -> usize` — `src/pipeline/split_filter.rs:54`. Returns the number of `true` entries in `keep`. Pure.

#### report_step

- **Signature:** `fn report_step(lines: &mut Vec<String>, name: &str, before: usize, after: usize)`
- **Source:** `src/pipeline/split_filter.rs:59`
- **Purpose:** Append one report line summarizing a filter step's effect on particle count.
- **Parameters:** `lines` — report buffer to append to; `name` — step label; `before`/`after` — kept-particle counts before and after the step.
- **Returns:** Nothing; mutates `lines` in place.
- **Side effects:** Pushes `"{name}: before={before}, after={after}, removed={before-after}"` (removed computed via `saturating_sub`, so it can't underflow).

#### append_volume_histogram

- **Signature:** `fn append_volume_histogram(lines: &mut Vec<String>, title: &str, values: &[f64], bins: usize)`
- **Source:** `src/pipeline/split_filter.rs:71`
- **Purpose:** Append a single ASCII-bar text histogram of `values` to the report.
- **Parameters:** `lines` — report buffer; `title` — heading line; `values` — data to histogram; `bins` — requested bin count, clamped to `[4, 32]`.
- **Returns:** Nothing; mutates `lines`.
- **Side effects:** If `values` is empty, appends a `"{title}: no data"` line and returns. If all values are equal (range `< 1e-12`), appends a single summary line instead of a full histogram. Otherwise bins values into equal-width buckets across `[min, max]` and appends one line per bin with a `#`-repeated bar scaled to a max width of 30 characters (each bar has at least length 1, even for empty bins, due to `.max(1)`).
- **Notes:** The out-of-range clamp (`b < 0` → `0`, `b >= bins` → `bins-1`) guards against floating-point edge cases at the exact max value.

#### append_volume_histogram_comparison

- **Signature:** `fn append_volume_histogram_comparison(lines: &mut Vec<String>, title: &str, before_values: &[f64], after_values: &[f64], bins: usize)`
- **Source:** `src/pipeline/split_filter.rs:119`
- **Purpose:** Append a side-by-side "before vs. after" ASCII-bar histogram, using `before_values`' range to define bin edges for both series.
- **Parameters:** `lines`, `title`; `before_values`/`after_values` — the two data sets (typically all volumes vs. kept volumes); `bins` — clamped to `[4, 32]`.
- **Returns:** Nothing; mutates `lines`.
- **Side effects:** If `before_values` is empty, appends a `"no data"` line and returns. Bin edges (`min_v`/`max_v`) are always derived from `before_values` only, so `after_values` outside that range would fall into the boundary bins via the same clamp logic as `append_volume_histogram`. Bars are scaled independently per series (`max_before`, `max_after`) to a max width of 16 characters each.
- **Notes:** Since `after_values` is always a subset of `before_values` in this pipeline's usage (kept ⊆ all), its range can never actually exceed `before_values`' range in practice.

#### normal_cdf

- **Signature:** `fn normal_cdf(x: f64) -> f64`
- **Source:** `src/pipeline/split_filter.rs:202`
- **Purpose:** Standard normal (mean 0, variance 1) cumulative distribution function.
- **Parameters:** `x` — standard-normal-scaled input.
- **Returns:** `0.5 * (1 + erf(x / sqrt(2)))`, in `[0, 1]`.
- **Side effects:** None. Delegates to `erf_approx`.

#### erf_approx

- **Signature:** `fn erf_approx(x: f64) -> f64`
- **Source:** `src/pipeline/split_filter.rs:208`
- **Purpose:** Approximate the Gauss error function `erf(x)`.
- **Parameters:** `x`.
- **Returns:** `f64` approximation of `erf(x)`, accurate to about `1.5e-7` (the documented error bound of the underlying formula).
- **Side effects:** None.
- **Notes:** Implements Abramowitz & Stegun formula 7.1.26, a rational-polynomial approximation using `t = 1/(1 + 0.3275911*|x|)` and a fixed 5-term polynomial in `t`, combined with `exp(-x^2)` and the sign of `x`. This is a closed-form numerical approximation, not an exact special-function evaluation — adequate for the pipeline's rebalancing heuristic but not bit-exact with a true erf implementation.

#### apply_lognormal_rebalance

- **Signature:** `fn apply_lognormal_rebalance(keep: &mut [bool], volumes: &[f64], cfg: &SplitFilterVolume)`
- **Source:** `src/pipeline/split_filter.rs:225`
- **Purpose:** Rebalance the kept-particle population so its volume distribution more closely follows a fitted lognormal, by randomly dropping particles from bins that are over-represented relative to the fitted model.
- **Parameters:** `keep` — mutable keep-mask, updated in place; `volumes` — per-particle volume array (parallel to `keep`); `cfg: &SplitFilterVolume` — reads `cfg.bins` (default 12, clamped to `[4, 64]`) and `cfg.over_factor` (default 1.25, floored at 1.0).
- **Returns:** Nothing; mutates `keep` (sets excess entries to `false`).
- **Side effects:** Uses `rand::thread_rng()` to shuffle within over-represented bins before dropping, so which particular particles are dropped is nondeterministic across runs (not seeded).
- **Notes:** Algorithm:
  1. Collect currently-kept particles with `volume > 0.0` as candidates. If fewer than 4 candidates, bail out (no-op) — insufficient data to fit a distribution.
  2. Take `log_vals = ln(volume)` for each candidate; fit `mu` (mean) and `sigma` (population std-dev, floored at `1e-9`) — i.e. a maximum-likelihood lognormal fit on the volumes.
  3. If the log-volume range is degenerate (`< 1e-12`), bail out.
  4. Bin candidates into `bins` equal-width buckets over `[min_log, max_log]` by their `ln(volume)`.
  5. For each bin, compute the *expected* fraction of the total population that a normal distribution with `(mu, sigma)` would place in that bin's log-volume range (`normal_cdf(hi) - normal_cdf(lo)`), scaled by the candidate count to get an expected count, then `allowed = ceil(expected * over_factor)`.
  6. If a bin holds more candidates than `allowed`, shuffle that bin's indices and mark all but the first `allowed` as `keep[idx] = false`.
- **Notes (config semantics):** `over_factor` acts as slack above the theoretical lognormal-implied count before pruning kicks in — `over_factor = 1.0` prunes down to exactly the fitted-model expectation per bin; larger values tolerate more over-representation before dropping particles. This is a heuristic rebalancing tool, not a true resampling/rejection-sampling algorithm — it never *adds* particles to under-represented bins, only removes from over-represented ones.


### Crop execution updates (2026-09-18)

`CropPipeline::run` installs a pool bounded by `cpu_max`; `run_in_pool` performs all stages. `gpu_crop_values_supported` rejects lossy integer narrowing before adapter initialization. `acceleration` and the environment override determine the backend, budget and permission to fall back. GPU runtime errors propagate when fallback is forbidden. Resampling uses adaptive slice/row tasks. `trim_volume_border` consumes and compacts the original allocation, preserving depth, numeric type and row order, including zero-copy zero trim.


### Split-filter metric preparation (PERF-15)

`SplitFilterConfig.cpu_max` bounds one pool for loading, splitting, metric preparation, filtering and saving (`-1`/absent: available workers). `prepare_particle_metrics` computes immutable volume/aspect/area records in component order. At least 32 components use indexed parallel collection; smaller sets remain serial. Each component uses the original geometry functions and reduction order. Bbox is only requested by the aspect filter; area is only computed for sharpness candidates that survive the aspect and positive-volume gates. Filtering reads those records in its original order, and lognormal RNG/deletions remain serial. Before/after reporting shares the immutable volume array instead of cloning it. STL writes run in batches of at most two in the same pool only when kept particles average at least `WRITE_PAIR_MIN_FACES` (65,536) faces; smaller outputs are written one at a time in rank order, because each pair pays a pool fork/join that costs more than a small file (a ~3,700-granule split took 0.074 s serial against 0.222 s paired on 8 workers, PLAN.Performance.md §72). Names and errors are consumed in rank order either way; in paired mode an error may leave the other file of its batch written. Outputs are byte-identical between the two modes and across worker counts.


`foreground_blocks` maps foreground voxels in fixed 65,536-voxel chunks, collecting partials in block order. `estimate_pca_bbox` uses `foreground_row_blocks` for moments and `foreground_blocks` (a per-voxel wrapper over it) for projected min/max; the three-pass form survives only as a test oracle. `detect_background_mode` scans boundary faces only and resolves tied counts by smallest value. The original serial PCA is retained only under tests for numerical comparison.

PCA task grain: for the parallel branch, `foreground_blocks` sets a minimum number of blocks per Rayon job using a workload-derived task budget, `min(workers, ceil(N/1,048,576))`; fixed block boundaries and ordered collection remain unchanged.

### Crop stage timings (2026-09-23)

Successful stages emit `[Timing] crop stage=<name> seconds=<wall_seconds>` for `load`, `background`, `pca`, `transform_and_backend`, `trim`, `encode_write`, and `total_in_pool`. `transform_and_backend` includes bounding-box sizing, policy/adapter/device setup, GPU transfers/readback or CPU resampling and any permitted fallback; it is not GPU kernel time. `encode_write` includes the explicit output flush, not fsync. `total_in_pool` excludes CLI/config/thread-pool creation and includes reporting overhead; process-level benchmarks measure those separately. Failed stages do not emit fabricated zero/completion measurements. The real-input CPU 1/2/4/8-worker process/RSS matrix and cache limitations are tracked in PLAN.Performance.md §61.

### Dense background counts (2026-09-23)

`for_each_boundary_value` visits only faces, counting corners/edges once. Background mode uses 256 `usize` counters for U8/I8 and 65,536 counters for U16/I16 volumes with at least 65,536 voxels (at most 512 KiB on a 64-bit host). Smaller 16-bit and all 32-bit volumes retain the HashMap path. A checked index sends values outside the declared dense range into a sparse spill map; metadata is not used to truncate/reject arbitrary i64 values. Dense and sparse counts share a running mode with the original smallest-value tie break; no final full histogram scan is needed. Counters are local and released before PCA. Independent full-grid ordered-map oracles cover collapsed dimensions, both integer extrema, signed ranges and metadata mismatches. Real-input end-to-end evidence is tracked in PLAN.Performance.md §62.

### Shared stage timer (2026-09-25)

Crop now emits its stage lines through `pipeline::timing::StageTimer` with the same names, order, nine-decimal format and stage boundaries as before (`restart` excludes the same untimed gaps), and appends `[Timing] crop workers=<n>` and `[Timing] crop peak_rss_bytes=<n|unavailable>`. Split-filter emits `load`, `split`, `metrics`, `filter`, `write_stl`, `write_report` and `total_in_pool`; a folder input is now loaded completely before splitting (each source mesh is still dropped as soon as it is split). See `pipeline-core.md` for the helper.

### One-pass moments and canonical PCA frame (PERF-14)

`estimate_pca_bbox` now makes one statistics pass plus the projected-bounds pass. `foreground_row_blocks` walks the same fixed 65,536-voxel blocks and hands each contiguous row segment `(x0, y, z, values)` to the accumulator. For a row, the foreground count, `Σx` and `Σx²` are exact integers (`u64`/`u128`), so `MomentState::from_row` builds that row's exact mean and centered second moment (only the xx entry is nonzero, since y and z are constant). Rows are merged into the block's running `(count, mean, M2)` with Chan's parallel formula (`MomentState::merge`: `mean += δ·nb/n`, `M2 += M2b + δδᵀ·na·nb/n`), and block states are merged in ascending block order, so results are identical for any worker count and avoid the `E[x²]-E[x]²` cancellation. The covariance is `M2/count`. This is Welford's online update applied at row granularity (Welford is the `nb = 1` case of Chan's merge). `estimate_pca_bbox_three_pass` keeps the previous centroid + centered covariance + bounds implementation as a test oracle; `online_moments_match_three_pass_and_workers` requires frame < 1e-10, centroid < 1e-11, bounds < 1e-8 and identical nearest crop output on oblique samples, and bit-identical results at 1/2/8 workers.

`pca_frame(cov)` sorts eigenpairs by descending eigenvalue and groups adjacent eigenvalues whose gap is at most `PCA_DEGENERATE_REL_TOL = 1e-3` times the largest magnitude. A three-fold group yields exactly the scan axes. A two-fold group uses the scan axes x, y, z in order, projected into the eigenspace and Gram-Schmidt orthonormalized, accepting an axis only when its residual norm is at least 0.5 (two such axes always exist), so the basis no longer depends on solver noise. A non-degenerate column k is sign-fixed so its component along scan axis k is positive, which picks the frame closest to the scan axes; only when that component is below `PCA_SIGN_DIAGONAL_TOL = 1e-6` does the largest-magnitude component decide (lowest axis on exact ties). A negative determinant flips the non-degenerate column with the smallest diagonal magnitude (the least rotation given up), or column 2 when every column is degenerate. Tests: cube and sphere give the identity frame with and without an added voxel; cylinders along z and x give `[z, x, y]` and the identity with and without an added voxel; a tilted two-fold covariance with 1e-9 noise keeps the same frame.

The non-degenerate sign convention was needed because `SymmetricEigen`'s eigenvector signs flipped between the three-pass and one-pass covariance of an ordinary oblique sample (last-bit differences). The first version fixed each column's largest-magnitude component positive; on the repository's real CT RAW stack (principal axis 50 degrees from x in the scan plane) that flipped column 1, the right-handedness fix then sent column 2 to -z, and the crop came out turned 180 degrees with its slice axis reversed. The diagonal rule returns the pure 50-degree rotation about z, and the crop output is byte-identical to the pre-2026-09-25 baseline (PLAN.Performance.md §68). Test: `frame_keeps_the_scan_orientation_of_a_sample_rotated_about_z`.
