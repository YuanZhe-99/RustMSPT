# Pipeline Core Reference

Covers the `Pipeline` trait infrastructure and four of the simpler/support pipelines: `rotation`, `scale`, `forge`, and `measure`. The `pack`, `optimize`, `crop`, and `split_filter` pipelines are documented elsewhere.

## Index

| Item | Location | Summary |
|---|---|---|
| `Pipeline::run` (trait) | `src/pipeline/mod.rs:17` | Trait method every pipeline struct implements to execute end-to-end. |
| `create_progress_bar` | `src/pipeline/mod.rs:25` | Builds a tty-aware indicatif progress bar with a given template and fill characters. |
| `RotationMode` (enum) | `src/pipeline/rotation.rs:6` | Represents no rotation, a fixed axis, or a random axis. |
| `parse_rotation_mode` | `src/pipeline/rotation.rs:18` | Parses `none/x/y/z/vector/any` config strings into a `RotationMode`. |
| `sample_rotation_axis` | `src/pipeline/rotation.rs:57` | Draws a concrete rotation axis vector for a given `RotationMode`. |
| `ScalePipeline` (struct) | `src/pipeline/scale.rs:8` | Holds `ScaleConfig` for the scaling pipeline. |
| `ScalePipeline::run` | `src/pipeline/scale.rs:19` | Loads an STL, applies unit-conversion/factor scaling, optionally fixes orientation, saves output. |
| `ForgePipeline` (struct) | `src/pipeline/forge.rs:13` | Holds `ForgingConfig` for the FFD forging pipeline. |
| `ForgePipeline::parse_roi_bbox` | `src/pipeline/forge.rs:19` | Parses an optional 6-element ROI bounding box from config. |
| `ForgePipeline::parse_compression_axis` | `src/pipeline/forge.rs:38` | Parses the compression axis string (`x`/`y`/`z`) into an index and label. |
| `ForgePipeline::run` | `src/pipeline/forge.rs:58` | Runs FFD-based compression/forging, tracks ROI, writes forged STL and a text report. |
| `MeasurePipeline` (struct) | `src/pipeline/measure.rs:16` | Holds `MeasurementConfig` for the S2/volume-fraction measurement pipeline. |
| `MeasurePipeline::parse_optional_bbox` | `src/pipeline/measure.rs:22` | Parses an optional bounding box (3-element size or 6-element min/max) from config. |
| `MeasurePipeline::l2_error` | `src/pipeline/measure.rs:31` | Computes the L2 distance between two S2 value vectors over their common prefix length. |
| `MeasurePipeline::run` | `src/pipeline/measure.rs:53` | Loads an STL, computes volume fraction and S2 correlation (exact/MC/both, CPU or GPU), writes a report. |

---

## `pipeline/mod.rs`

#### Pipeline::run (trait)

- **Signature:** `fn run(&self) -> Result<()>`
- **Source:** `src/pipeline/mod.rs:17`
- **Purpose:** Common execution entry point implemented by every pipeline in this crate.
- **Parameters:** `&self` — the concrete pipeline struct's own config.
- **Returns:** `Ok(())` on success, or a `RustMsptError` on failure.
- **Side effects:** Varies entirely by implementer — typically reads an input STL, performs geometry work, writes output file(s), and prints `[Info]`/`[Warning]` progress lines to stdout/stderr.
- **Notes:** Implemented by all 7 pipeline structs in this crate: `CropPipeline`, `ForgePipeline`, `MeasurePipeline`, `OptimizePipeline`, `PackPipeline`, `ScalePipeline`, and `SplitFilterPipeline`. `main.rs` constructs the pipeline corresponding to the requested subcommand/config and calls `.run()` on it exactly once per invocation; there is no built-in chaining or looping over pipelines within a single process run.
- **See also:** `src/main.rs` for pipeline dispatch.

#### create_progress_bar

- **Signature:** `pub fn create_progress_bar(length: u64, template: &str, chars: &str) -> ProgressBar`
- **Source:** `src/pipeline/mod.rs:25`
- **Purpose:** Build a standardized `indicatif` progress bar, automatically hidden when output isn't an interactive terminal.
- **Parameters:**
  - `length` — total number of steps/units the bar represents.
  - `template` — indicatif template string (e.g. `"{bar} {pos}/{len}"`).
  - `chars` — the fill/head/empty characters passed to `progress_chars`.
- **Returns:** A configured `ProgressBar`. If the template string fails to parse, falls back silently to `ProgressStyle::default_bar()`.
- **Side effects:** Checks `std::io::stderr().is_terminal()`; if stderr is not a TTY (e.g. redirected to a file, or running in CI), the bar's draw target is set to `hidden()` so no progress output is emitted.
- **Notes:** Used by the longer-running pipelines (pack, optimize) to report iteration/placement progress without corrupting non-interactive logs.

---

## `pipeline/rotation.rs`

Shared rotation-axis utilities used by the `pack` and `optimize` pipelines when randomly or deterministically orienting particles/components.

#### RotationMode (enum)

- **Source:** `src/pipeline/rotation.rs:6`
- **Purpose:** Represents the rotation strategy applied to a placed object.
- **Variants:**
  - `None` — no rotation is applied.
  - `Axis(Vec3)` — rotation is constrained to a single fixed axis (unit axes `x`/`y`/`z`, or an arbitrary user-supplied vector).
  - `Any` — rotation axis is sampled randomly per placement.
- **Notes:** Derives `Clone` only; no `Debug`/`Copy`.

#### parse_rotation_mode

- **Signature:** `pub fn parse_rotation_mode(prefix: &str, mode: Option<&str>, axis_vec: Option<&Vec<f64>>) -> Result<RotationMode>`
- **Source:** `src/pipeline/rotation.rs:18`
- **Purpose:** Parse a rotation-mode configuration string into a `RotationMode` value.
- **Parameters:**
  - `prefix` — config-section name used only to build readable error messages (e.g. `"pack"` or `"optimize"`).
  - `mode` — the raw mode string: one of `none`, `x`, `y`, `z`, `vector`, `any` (case-insensitive, whitespace-trimmed). Defaults to `"any"` when `None`.
  - `axis_vec` — required only when `mode == "vector"`; must have exactly 3 elements and non-zero magnitude.
- **Returns:** `Ok(RotationMode)` on success.
- **Side effects:** None (pure parsing).
- **Notes:** Returns `RustMsptError::InvalidConfig` when: the mode string is unrecognized; `"vector"` is requested but `axis_vec` is missing or not length 3; or the supplied vector's norm is `<= 1e-12` (effectively zero, checked via `geometry::vec_norm`).

#### sample_rotation_axis

`pub fn sample_rotation_axis(rng: &mut rand::rngs::ThreadRng, mode: &RotationMode) -> Option<Vec3>` — `src/pipeline/rotation.rs:57`. Returns `None` for `RotationMode::None`, the fixed axis unchanged for `RotationMode::Axis`, and for `RotationMode::Any` returns a freshly sampled `Vec3` with each component drawn uniformly from `[-1.0, 1.0)`. No side effects beyond consuming RNG state.
> **Doc note:** the AI-FUNC-SUMMARY says the "Any" vector is "random in cube," which matches the code — it is *not* normalized to the unit sphere before being returned, so downstream code must normalize it if a unit rotation axis is required.

---

## `pipeline/scale.rs`

#### ScalePipeline (struct)

- **Source:** `src/pipeline/scale.rs:8`
- **Fields:** `config: ScaleConfig` — the full scaling configuration (input/output paths, scaling type/value, orientation flag).

#### ScalePipeline::run

- **Signature:** `fn run(&self) -> Result<()>`
- **Source:** `src/pipeline/scale.rs:19`
- **Purpose:** Load an STL mesh, rescale it by a unit-conversion or explicit factor, optionally fix component orientation to positive signed volume, and save the result.
- **Parameters:** `&self` — reads `self.config.input.stl_path`, `self.config.output.stl_path`, `self.config.scaling.{type, value, orient_to_positive_volume}`.
- **Returns:** `Ok(())` on success; `Err(RustMsptError::InvalidConfig(..))` for a non-positive `mm_per_voxel`/`voxel_per_mm` value or an unrecognized `scaling.type`.
- **Side effects:** Reads the STL (or folder of STLs, merged) from disk via `load_stl_or_merge_folder`; writes the scaled mesh to `output.stl_path` via `save_stl`; prints `[Info]` lines to stdout reporting mode, value, original/scaled bounds, volumes, and (if enabled) how many mesh components were flipped for orientation.
- **Notes:** Three scaling modes are supported:
  - `"factor"` — `value` is used directly as the multiplicative scale factor.
  - `"mm_per_voxel"` — `value` (must be `> 0`) is used directly as the factor.
  - `"voxel_per_mm"` — `value` (must be `> 0`) is inverted (`1.0 / value`) to obtain the factor.

  When `orient_to_positive_volume` is enabled, `orient_components_to_positive_volume` is applied post-scale and the number of flipped/total components is reported; otherwise the mesh is used unmodified (cloned).

---

## `pipeline/forge.rs`

#### ForgePipeline (struct)

- **Source:** `src/pipeline/forge.rs:13`
- **Fields:** `config: ForgingConfig` — forging parameters (input/output paths, compression ratio/axis, bulge factor, ROI bounding box, mesh type, void densification, orientation flag).

#### ForgePipeline::parse_roi_bbox

- **Signature:** `fn parse_roi_bbox(values: &Option<Vec<f64>>) -> Option<BoundingBox>`
- **Source:** `src/pipeline/forge.rs:19`
- **Purpose:** Parse an optional 6-element `[min_x, min_y, min_z, max_x, max_y, max_z]` region-of-interest box from config.
- **Parameters:** `values` — `Option<Vec<f64>>` from `config.forging.roi_bounding_box`.
- **Returns:** `Some(BoundingBox)` if the vector has exactly 6 elements; `None` otherwise (missing config or wrong length — the wrong-length case is silently treated as "no ROI" rather than an error).
- **Side effects:** None.

#### ForgePipeline::parse_compression_axis

- **Signature:** `fn parse_compression_axis(axis: Option<&str>) -> Result<(usize, &'static str)>`
- **Source:** `src/pipeline/forge.rs:38`
- **Purpose:** Resolve the configured compression axis string to a numeric mesh-axis index and a display label.
- **Parameters:** `axis` — optional string, defaults to `"z"` when `None`; matched case-insensitively after trimming.
- **Returns:** `Ok((0, "x"))`, `Ok((1, "y"))`, or `Ok((2, "z"))`.
- **Side effects:** None.
- **Notes:** Returns `RustMsptError::InvalidConfig` for any string other than `x`/`y`/`z`.

#### ForgePipeline::run

- **Signature:** `fn run(&self) -> Result<()>`
- **Source:** `src/pipeline/forge.rs:58`
- **Purpose:** Execute the free-form deformation (FFD) forging pipeline: compress a mesh along one axis (with bulge and optional void densification), track how the region of interest (ROI) moves through the deformation, realign the output so the ROI lands where it was requested, and write both the forged STL and a human-readable report.
- **Parameters:** `&self` — reads `self.config.forging.*`: `input_stl_path`, `output_stl_path` (defaults to `data/output/forged_mesh.stl`), `roi_bounding_box`, `compression_ratio` (default `0.2`), `compression_axis` (default `"z"`), `orient_to_positive_volume`, `bulge_factor` (default `0.5`), `mesh_type` (default `"particle"`), `void_densification` (default `1.0`).
- **Returns:** `Ok(())` on success, or a propagated I/O / config error.
- **Side effects:**
  - Reads the input STL (or merged folder) from disk.
  - Calls `simulate_forging_ffd_with_tracking` to perform the actual FFD deformation and obtain the post-deformation ROI location.
  - Optionally re-orients mesh components to positive signed volume (`orient_components_to_positive_volume`).
  - Translates the output mesh (`translate_mesh`) so the tracked ROI's minimum corner realigns with the originally requested ROI minimum corner, when both are available and the shift is non-negligible (`> f64::EPSILON` on any axis).
  - Writes the forged mesh via `save_stl`.
  - Writes a text report to `<output>.txt` (same path, extension replaced) containing before/after bounding boxes (mesh and ROI), before/after ROI volume fraction, compression axis, orientation-fix stats, and the output translation vector. Creates the parent directory if needed.
  - Prints the same information as `[Info]` lines to stdout.
- **Notes:** Volume fraction is computed over the ROI bbox if supplied, otherwise over the whole mesh's bounding box (`lattice_bbox`). The "before" volume and "after" volume (`_before`, `_after` via `mesh_volume`) are computed but not directly used beyond being discarded — see `> **Doc note:**` below.
  > **Doc note:** `_before` and `_after` (whole-mesh volumes) are computed via `mesh_volume` but bound to underscore-prefixed variables and never surfaced in the report or console output; only the ROI volume fractions (`before_roi_vf`, `after_roi_vf`) are reported. This looks intentional (silencing an "unused" warning while keeping the computation available for future use or debugging) rather than a bug, but the values are not currently visible to users.
- **See also:** [../algorithms/ffd-forging.md](../algorithms/ffd-forging.md)

---

## `pipeline/measure.rs`

#### MeasurePipeline (struct)

- **Source:** `src/pipeline/measure.rs:16`
- **Fields:** `config: MeasurementConfig` — measurement parameters (STL path, bounding box(es), S2 method/samples/pitch, acceleration policy, output path, CPU cap).

#### MeasurePipeline::parse_optional_bbox

- **Signature:** `fn parse_optional_bbox(values: &Option<Vec<f64>>) -> Result<Option<BoundingBox>>`
- **Source:** `src/pipeline/measure.rs:22`
- **Purpose:** Parse an optional bounding box from config, accepting either a 3-element size vector or a 6-element min/max vector (delegating the actual shape-dependent parsing to `config::parse_box_dimensions`).
- **Parameters:** `values` — `Option<Vec<f64>>`.
- **Returns:** `Ok(None)` if `values` is `None` or an empty vector; otherwise `Ok(Some(BoundingBox))` from `parse_box_dimensions`, or a propagated `Err` if that parse fails.
- **Side effects:** None.

#### MeasurePipeline::l2_error

- **Signature:** `fn l2_error(a: &[f64], b: &[f64]) -> f64`
- **Source:** `src/pipeline/measure.rs:31`
- **Purpose:** Compute the L2 (Euclidean) distance between two S2 correlation value vectors, used to compare "exact" vs. "monte_carlo" results when `method = "both"`.
- **Parameters:** `a`, `b` — S2 value slices, potentially of different lengths.
- **Returns:** `sqrt(sum((a[i]-b[i])^2))` over `i` in `0..min(a.len(), b.len())`; `0.0` if either slice is empty.
- **Side effects:** None.
- **Notes:** This duplicates the core logic of `geometry::l2_norm` (sum-of-squared-differences then square root over a shared-length prefix). It is kept as a private, pipeline-local helper rather than reusing the geometry module's function — the two implementations should be kept in sync if either changes, since there is currently no shared code path between them.

#### MeasurePipeline::run

- **Signature:** `fn run(&self) -> Result<()>`
- **Source:** `src/pipeline/measure.rs:53`
- **Purpose:** Execute the S2 two-point correlation and volume-fraction measurement pipeline: load the mesh, resolve the bounding box, select a compute backend (CPU/GPU), compute S2 via the requested method(s), and write a report.
- **Parameters:** `&self` — reads `self.config.measurement.*`: `cpu_max`, `stl_path`, `bounding_box`, `stl_bounding_box`, `mc_method` (`"exact"` / `"both"` / anything else treated as `"monte_carlo"`), `r_max`, `mc_samples` (default `10_000`), `voxel_pitch`, `acceleration` (mode, `gpu_min_voxels`, `gpu_memory_limit_mb`), `output_path`.
- **Returns:** `Ok(())` on success, or a propagated error (e.g. thread-pool build failure wrapped as `InvalidConfig`, I/O errors).
- **Side effects:**
  - Builds a dedicated Rayon thread pool sized from `cpu_max` (or all available cores when `cpu_max == -1`).
  - Reads the input STL (or merged folder).
  - Computes particle count (`split_mesh_into_granules`) and volume fraction (`volume_fraction_in_bbox`) over the resolved bbox.
  - Resolves compute backend via `select_backend(accel.mode, Some(accel.gpu_min_voxels), accel.gpu_memory_limit_mb, voxel_count)`, where `voxel_count` is derived from bbox size divided by `voxel_pitch` (pitch clamped to `1.0` if `<= 0.0`).
  - When the `gpu` feature is enabled and the backend selection is GPU, initializes a `crate::gpu::s2::GpuS2Pipeline`; on init failure, logs a warning and falls back to CPU silently.
  - Runs `calculate_s2` (CPU, inside the custom thread pool) or `calculate_s2_with_gpu`/`calculate_s2_gpu_exact` (GPU, feature-gated) depending on method and backend.
  - Writes a text report to `params.output_path` (creating parent directories as needed) containing volume fraction, compute backend, method, S2(0)-vs-VF difference, and the full S2 value series (and, for `"both"`, the L2 error between exact and Monte Carlo series).
  - Prints extensive `[Info]`/`[Warning]` diagnostics to stdout throughout (thread pool sizing, bbox, S2 config, backend selection/fallback, per-method summaries, final output path).
- **Notes:**
  - Bounding box resolution precedence: explicit `bounding_box` > `stl_bounding_box` > mesh-derived bbox (`mesh_bbox`) > a unit-cube fallback (`BoundingBox::from_size(Vec3::new(1,1,1))`).
  - `method = "exact"` (alone or as part of `"both"`) is automatically downgraded to `"monte_carlo"` when the voxel grid exceeds `exact_voxel_limit = 1_500_000` voxels, with a `[Warning]` printed; for `"both"` this means only the Monte Carlo branch runs and the report notes "(exact skipped by voxel limit)".
  - GPU code paths are entirely absent when the crate is built without the `gpu` feature (`#[cfg(not(feature = "gpu"))]` branches use the CPU thread pool unconditionally); a runtime warning is printed if GPU acceleration was requested but the feature isn't compiled in.
- **See also:** [../algorithms/s2-two-point-correlation.md](../algorithms/s2-two-point-correlation.md), [gpu.md](gpu.md)
