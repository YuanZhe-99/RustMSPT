# Config Reference (`src/config/`)

This module defines the YAML-deserializable configuration structs consumed by each RustMSPT pipeline. Each pipeline (forging, scaling, measurement, optimization, packing, crop, split/filter) has its own top-level `*Config` struct that groups an `input`, `output`, and a `*Params` struct holding the pipeline-specific knobs. All structs derive `serde::Deserialize` and are loaded from YAML via `load_yaml` (see [Functions](#functions)). A handful of fields use custom `deserialize_with` helpers, defined in `deserialize.rs`, to accept both numeric and string YAML representations (e.g. `"1_000_000"` or `1000000`) for integer fields.

Struct definitions are grouped below by source file, in the order listed in the assignment. Field tables note the Rust type, the YAML key when it differs from the field name (via `r#type`/`r#box` raw identifiers or `#[serde(rename)]`), the default when `#[serde(default = ...)]` applies, and a short description of the field's meaning based on how it is consumed elsewhere in the codebase.

## `mod.rs` — shared input/output/box structs

`src/config/mod.rs` re-exports every pipeline's config types and defines a few small structs shared across pipelines that wrap a single path or dimension list.

### `InputStl`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `stl_path` | `String` | `stl_path` | — | Path to the input STL mesh file. |

### `InputPath`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `path` | `String` | `path` | — | Generic input path (used by packing/split-filter, where the input need not be a single STL). |

### `OutputStl`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `stl_path` | `String` | `stl_path` | — | Path to write the output STL mesh file. |

### `OutputPath`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `path` | `String` | `path` | — | Generic output path. |

### `BoxConfig`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `dimensions` | `Vec<f64>` | `dimensions` | — | Either a 3-element size (box placed at origin) or a 6-element `[min_x, min_y, min_z, max_x, max_y, max_z]`. Parsed into a `BoundingBox` by `parse_box_dimensions`. |

## `acceleration.rs` — `AccelerationConfig`

Shared sub-config embedded in `MeasurementParams` and `OptimizationParams` to control CPU/GPU compute backend selection.

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `mode` | `AccelerationMode` | `mode` | `AccelerationMode::Auto` | Compute mode: `auto`, `cpu`, or `gpu` (see `src/compute/backend.rs`). |
| `backend` | `String` | `backend` | `"wgpu"` | Name of the GPU backend to use. |
| `cpu_fallback` | `bool` | `cpu_fallback` | `true` | Whether to fall back to CPU execution if GPU acceleration is unavailable or fails. |
| `gpu_min_voxels` | `usize` | `gpu_min_voxels` | `250_000` | Minimum voxel-grid size below which GPU acceleration is not worth the overhead (used by the `auto` mode heuristic). |
| `gpu_memory_limit_mb` | `Option<u64>` | `gpu_memory_limit_mb` | `None` | Optional cap on GPU memory usage, in megabytes. |
| `gpu_prefer_power` | `bool` | `gpu_prefer_power` | `false` | Whether to prefer a high-power (discrete) GPU adapter over a low-power/integrated one during device selection. |
| `gpu_precision` | `String` | `gpu_precision` | `"f32"` | Floating point precision requested for GPU compute shaders. |

`AccelerationConfig` also implements `Default` (mirroring the same defaults as the `#[serde(default = ...)]` functions), so it can be omitted entirely from YAML.

#### default_backend
`fn default_backend() -> String` — `src/config/acceleration.rs:21`. Serde default for `backend`: returns `"wgpu"`. No side effects.

#### default_true
`fn default_true() -> bool` — `src/config/acceleration.rs:25`. Serde default for `cpu_fallback`: returns `true`. No side effects.

#### default_gpu_min_voxels
`fn default_gpu_min_voxels() -> usize` — `src/config/acceleration.rs:29`. Serde default for `gpu_min_voxels`: returns `250_000`. No side effects.

#### default_gpu_precision
`fn default_gpu_precision() -> String` — `src/config/acceleration.rs:33`. Serde default for `gpu_precision`: returns `"f32"`. No side effects.

#### AccelerationConfig::default
- **Signature:** `fn default() -> Self` (`impl Default for AccelerationConfig`)
- **Source:** `src/config/acceleration.rs:38`
- **Purpose:** Rust-level (non-serde) default matching the same values as the `#[serde(default = ...)]` helper functions above, so `AccelerationConfig::default()` is usable outside deserialization (e.g. in tests constructing config structs manually).
- **Returns:** `AccelerationConfig` with `mode: Auto`, `backend: "wgpu"`, `cpu_fallback: true`, `gpu_min_voxels: 250_000`, `gpu_memory_limit_mb: None`, `gpu_prefer_power: false`, `gpu_precision: "f32"`.
- **Side effects:** None.

## `crop.rs`

### `CropRawParams`

Only present when `CropInput.r#type == "raw"`; describes how to interpret a raw binary volume file.

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `width` | `usize` | `width` | — | Slice width in voxels. |
| `height` | `usize` | `height` | — | Slice height in voxels. |
| `bits` | `u8` | `bits` | — | Bit depth per voxel (e.g. 8, 16, 32). |
| `signed` | `bool` | `signed` | — | Whether voxel values are signed integers. |
| `byte_order` | `Option<String>` | `byte_order` | — | Endianness of the raw data (e.g. `"little"`/`"big"`). |

### `CropInput`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `r#type` | `String` | `type` | — | Input format discriminator (e.g. `"raw"`, image-stack folder, etc.). |
| `path` | `String` | `path` | — | Path to the input volume/image data. |
| `slice_start` | `Option<i32>` | `slice_start` | `None` | First slice index to include (via `deserialize_option_i32_flexible`, accepts numeric or string YAML values, negative indices allowed for "from the end" semantics). |
| `slice_end` | `Option<i32>` | `slice_end` | `None` | Last slice index to include (same flexible parsing as `slice_start`). |
| `raw` | `Option<CropRawParams>` | `raw` | — | Raw-format decoding parameters; required when `r#type == "raw"`. |

### `CropOutput`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `path` | `String` | `path` | — | Output path/folder for cropped slices. |
| `folder_prefix` | `Option<String>` | `folder_prefix` | — | Filename prefix for output slice files. |
| `folder_extension` | `Option<String>` | `folder_extension` | — | File extension for output slice files. |

### `CropConfig`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `input` | `CropInput` | `input` | — | Input volume description. |
| `output` | `CropOutput` | `output` | — | Output destination description. |
| `interpolation` | `Option<String>` | `interpolation` | — | Interpolation method used when resampling during crop, if any. |
| `edge_trim` | `Option<i32>` | `edge_trim` | — | Number of voxels/pixels to trim from each edge after cropping. |

## `deserialize.rs` — flexible-parsing helpers

Not structs — this file provides plain functions and `deserialize_with` helpers used by other config structs to accept both numeric and underscore-separated string representations of integers in YAML (e.g. `1_000_000` as a YAML string). See [Functions](#functions) below for full documentation of each.

## `forging.rs`

### `ForgingParams`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `input_stl_path` | `String` | `input_stl_path` | — | Path to the input STL mesh to forge/deform. |
| `output_stl_path` | `Option<String>` | `output_stl_path` | — | Path to write the forged output STL. |
| `compression_ratio` | `Option<f64>` | `compression_ratio` | — | Fractional compression applied along `compression_axis`. |
| `compression_axis` | `Option<String>` | `compression_axis` | — | Axis along which compression is applied (e.g. `"x"`, `"y"`, `"z"`). |
| `orient_to_positive_volume` | `Option<bool>` | `orient_to_positive_volume` | — | Whether to reorient the mesh so its signed volume is positive before processing. |
| `bulge_factor` | `Option<f64>` | `bulge_factor` | — | Amount of lateral bulging applied to simulate volume-conserving deformation. |
| `roi_bounding_box` | `Option<Vec<f64>>` | `roi_bounding_box` | — | Region-of-interest bounding box (3- or 6-element form, same convention as `BoxConfig.dimensions`) restricting where forging is applied. |
| `mesh_type` | `Option<String>` | `mesh_type` | — | Mesh classification/type hint used to select a forging strategy. |
| `void_densification` | `Option<f64>` | `void_densification` | — | Factor controlling densification of internal voids during forging. |

### `ForgingConfig`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `forging` | `ForgingParams` | `forging` | — | Top-level wrapper; the whole YAML document is a single `forging:` block. |

## `measurement.rs`

### `MeasurementParams`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `stl_path` | `String` | `stl_path` | — | Path to the STL mesh to measure. |
| `bounding_box` | `Option<Vec<f64>>` | `bounding_box` | — | Optional explicit bounding box to measure within (3- or 6-element form). |
| `stl_bounding_box` | `Option<Vec<f64>>` | `stl_bounding_box` | — | Optional explicit bounding box describing the STL's own extents, used instead of recomputing it from mesh geometry. |
| `r_max` | `usize` | `r_max` | — | Maximum radius (in voxel units) for two-point correlation / S2 measurement. |
| `voxel_pitch` | `f64` | `voxel_pitch` | — | Voxel edge length used to discretize the mesh for measurement. |
| `mc_method` | `String` | `mc_method` | — | Monte Carlo sampling method identifier used for S2 estimation. |
| `mc_samples` | `Option<usize>` | `mc_samples` | `None` | Number of Monte Carlo sample points (via `deserialize_option_usize_flexible`, accepts numeric or underscore-separated string). |
| `cpu_max` | `Option<i32>` | `cpu_max` | `None` | Maximum number of CPU threads/cores to use (via `deserialize_option_i32_flexible`). |
| `output_path` | `String` | `output_path` | — | Path to write measurement results. |
| `acceleration` | `AccelerationConfig` | `acceleration` | `AccelerationConfig::default()` | GPU/CPU acceleration settings (see `acceleration.rs`). |

### `MeasurementConfig`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `measurement` | `MeasurementParams` | `measurement` | — | Top-level wrapper; the whole YAML document is a single `measurement:` block. |

## `optimization.rs`

### `TargetConfig`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `r#type` | `String` | `type` | — | Target discriminator, e.g. `"s2_array"` vs. `"stl"`, selecting which of `s2_array`/`stl_path` is used as the optimization target. |
| `s2_array` | `Option<Vec<f64>>` | `s2_array` | — | Explicit target two-point correlation (S2) curve values. |
| `stl_path` | `Option<String>` | `stl_path` | — | Path to a target STL mesh whose S2 statistics are used as the optimization target. |
| `stl_bounding_box` | `Option<Vec<f64>>` | `stl_bounding_box` | — | Optional explicit bounding box for the target STL. |

### `OptimizationParams`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `max_iterations` | `usize` | `max_iterations` | — (required) | Maximum simulated-annealing iterations. Parsed via `deserialize_usize_flexible` (accepts numeric or underscore-separated string), but has no `#[serde(default)]`, so the key is still mandatory. |
| `initial_temperature` | `f64` | `initial_temperature` | — | Starting temperature for simulated annealing. |
| `cooling_rate` | `f64` | `cooling_rate` | — | Multiplicative cooling factor applied per iteration/window. |
| `adaptive_temp_window` | `Option<usize>` | `adaptive_temp_window` | `None` | Window size (in iterations) over which adaptive temperature control measures acceptance rate. |
| `target_acceptance_low` | `Option<f64>` | `target_acceptance_low` | — | Lower bound of the target acceptance-rate band for adaptive cooling/heating. |
| `target_acceptance_high` | `Option<f64>` | `target_acceptance_high` | — | Upper bound of the target acceptance-rate band. |
| `adaptive_heat_factor` | `Option<f64>` | `adaptive_heat_factor` | — | Factor by which temperature is increased when acceptance rate is below `target_acceptance_low`. |
| `adaptive_cool_factor` | `Option<f64>` | `adaptive_cool_factor` | — | Factor by which temperature is decreased when acceptance rate is above `target_acceptance_high`. |
| `adaptive_temp_ceiling_factor` | `Option<f64>` | `adaptive_temp_ceiling_factor` | — | Maximum multiple of the initial temperature that adaptive heating may reach. |
| `r_max` | `usize` | `r_max` | — | Maximum radius for S2/two-point-correlation evaluation. |
| `voxel_pitch` | `f64` | `voxel_pitch` | — | Voxel edge length for discretization during optimization. |
| `mc_method` | `String` | `mc_method` | — | Monte Carlo sampling method identifier. |
| `mc_samples` | `usize` | `mc_samples` | — (required) | Number of Monte Carlo samples per S2 evaluation. Parsed via `deserialize_usize_flexible`. |
| `max_translation` | `f64` | `max_translation` | — | Maximum per-move translation magnitude for the annealing proposal distribution. |
| `max_rotation_deg` | `f64` | `max_rotation_deg` | — | Maximum per-move rotation, in degrees, for the annealing proposal distribution. |
| `min_neighbor_distance` | `Option<f64>` | `min_neighbor_distance` | — | Minimum allowed distance between neighboring particles/features. |
| `mode` | `Option<u8>` | `mode` | — | Numeric mode selector controlling optimizer behavior (semantics defined by the pipeline consuming this config). |
| `min_boundary_dist` | `Option<f64>` | `min_boundary_dist` | — | Minimum allowed distance from the domain boundary. |
| `min_cross_boundary_depth` | `Option<f64>` | `min_cross_boundary_depth` | — | Minimum penetration depth allowed when a feature crosses the boundary. |
| `prune_enabled` | `Option<bool>` | `prune_enabled` | — | Whether periodic pruning of poorly-fitting solution elements is enabled. |
| `prune_tolerance` | `Option<f64>` | `prune_tolerance` | — | Tolerance threshold used to decide whether an element is pruned. |
| `prune_max_rounds` | `Option<usize>` | `prune_max_rounds` | `None` | Maximum number of pruning rounds to run (flexible usize parsing). |
| `prune_eval_samples` | `Option<usize>` | `prune_eval_samples` | `None` | Number of samples used to evaluate candidates during pruning (flexible usize parsing). |
| `cpu_max` | `Option<i32>` | `cpu_max` | `None` | Maximum CPU threads to use (flexible i32 parsing). |
| `orient_to_positive_volume` | `Option<bool>` | `orient_to_positive_volume` | — | Whether to reorient input meshes so signed volume is positive. |
| `rotation_mode` | `Option<String>` | `rotation_mode` | — | Rotation strategy identifier (e.g. free rotation vs. constrained axis rotation). |
| `rotation_axis_vector` | `Option<Vec<f64>>` | `rotation_axis_vector` | — | Fixed rotation axis vector, used when `rotation_mode` constrains rotation to a single axis. |
| `islands` | `Option<usize>` | `islands` | `None` | Number of parallel "island" populations for island-model parallel annealing (flexible usize parsing). |
| `migration_interval` | `Option<usize>` | `migration_interval` | `None` | Number of iterations between migration events across islands (flexible usize parsing). |
| `acceleration` | `AccelerationConfig` | `acceleration` | `AccelerationConfig::default()` | GPU/CPU acceleration settings. |

### `OptimizationConfig`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `input` | `InputStl` | `input` | — | Input STL mesh to optimize. |
| `output` | `OutputPath` | `output` | — | Output destination for the optimized result. |
| `target` | `TargetConfig` | `target` | — | Optimization target (S2 curve or reference STL). |
| `r#box` | `BoxConfig` | `box` | — | Domain bounding box. |
| `optimization` | `OptimizationParams` | `optimization` | — | Optimizer parameters. |

## `packing.rs`

### `PackingFilters`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `min_volume` | `Option<f64>` | `min_volume` | — | Minimum accepted particle volume; smaller particles are filtered out. |
| `max_aspect_ratio` | `Option<f64>` | `max_aspect_ratio` | — | Maximum accepted aspect ratio for a particle. |
| `max_sharpness_ratio` | `Option<f64>` | `max_sharpness_ratio` | — | Maximum accepted sharpness ratio for a particle. |

### `PackingParams`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `target_volume_fraction` | `f64` | `target_volume_fraction` | — | Target solid volume fraction the pack should reach. |
| `mode` | `u8` | `mode` | — | Numeric packing-mode selector. |
| `max_attempts` | `usize` | `max_attempts` | — | Maximum placement attempts per particle before giving up. |
| `min_neighbor_distance` | `Option<f64>` | `min_neighbor_distance` | — | Minimum allowed distance between neighboring particles. |
| `min_boundary_dist` | `Option<f64>` | `min_boundary_dist` | — | Minimum allowed distance from the domain boundary. |
| `min_cross_boundary_depth` | `Option<f64>` | `min_cross_boundary_depth` | — | Minimum penetration depth allowed when a particle crosses the boundary. |
| `filters` | `Option<PackingFilters>` | `filters` | — | Optional particle-acceptance filters (see `PackingFilters` above). |
| `cpu_max` | `Option<i32>` | `cpu_max` | `None` | Maximum CPU threads to use (via `deserialize_option_i32_flexible`). |
| `orient_to_positive_volume` | `Option<bool>` | `orient_to_positive_volume` | — | Whether to reorient particle meshes so signed volume is positive. |
| `rotation_mode` | `Option<String>` | `rotation_mode` | — | Rotation strategy identifier for particle placement. |
| `rotation_axis_vector` | `Option<Vec<f64>>` | `rotation_axis_vector` | — | Fixed rotation axis vector, used when `rotation_mode` constrains rotation to a single axis. |
| `target_diameter_distribution_csv` | `Option<String>` | `target_diameter_distribution_csv` | `None` | Path to a CSV file describing a target particle-diameter (pore-size) distribution that the pack should steer toward. New field added alongside the target-diameter-distribution steering feature. |
| `target_mean_sphericity` | `Option<f64>` | `target_mean_sphericity` | `None` | Target mean sphericity value the packed particle population should approximate. New field, paired with `target_diameter_distribution_csv`. |
| `mean_sphericity_tolerance` | `Option<f64>` | `mean_sphericity_tolerance` | `None` | Allowed deviation from `target_mean_sphericity` before the packer treats the population's sphericity as out of tolerance. New field. |

> **See also:** `../algorithms/packing-target-diameter-distribution.md` for how `target_diameter_distribution_csv`, `target_mean_sphericity`, and `mean_sphericity_tolerance` are consumed by the packing pipeline (`src/pipeline/pack.rs`, `src/pipeline/pack_targets.rs`) to steer particle selection toward a target pore/diameter distribution and sphericity.

### `PackingConfig`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `input` | `InputPath` | `input` | — | Input path (e.g. a folder of candidate particle STLs). |
| `output` | `OutputPath` | `output` | — | Output destination for the packed result. |
| `r#box` | `BoxConfig` | `box` | — | Packing domain bounding box. |
| `packing` | `PackingParams` | `packing` | — | Packing algorithm parameters. |

## `scale.rs`

### `ScalingParams`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `r#type` | `String` | `type` | — | Scaling mode discriminator (e.g. scale by factor vs. scale to target size/volume). |
| `value` | `f64` | `value` | — | Scaling value whose interpretation depends on `r#type`. |
| `orient_to_positive_volume` | `Option<bool>` | `orient_to_positive_volume` | — | Whether to reorient the mesh so its signed volume is positive before/after scaling. |

### `ScaleConfig`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `input` | `InputStl` | `input` | — | Input STL mesh to scale. |
| `output` | `OutputStl` | `output` | — | Output path for the scaled STL. |
| `scaling` | `ScalingParams` | `scaling` | — | Scaling parameters. |

## `split_filter.rs`

### `SplitFilterOutput`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `folder` | `String` | `folder` | — | Output folder for split particle files. |
| `prefix` | `String` | `prefix` | — | Filename prefix for split output files. |
| `report_path` | `Option<String>` | `report_path` | — | Optional path to write a summary report of the split/filter operation. |

### `SplitFilterVolume`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `mode` | `Option<String>` | `mode` | — | Volume-filtering mode (e.g. absolute range vs. binned/histogram-based). |
| `min` | `Option<f64>` | `min` | — | Minimum accepted particle volume. |
| `max` | `Option<f64>` | `max` | — | Maximum accepted particle volume. |
| `bins` | `Option<usize>` | `bins` | `None` | Number of histogram bins when `mode` is bin-based (via `deserialize_option_usize_flexible`). |
| `over_factor` | `Option<f64>` | `over_factor` | — | Oversampling/tolerance factor applied to volume-bin acceptance. |

### `SplitFilterRules`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `enabled` | `Option<bool>` | `enabled` | — | Whether filtering rules are applied at all (vs. splitting only). |
| `max_aspect_ratio` | `Option<f64>` | `max_aspect_ratio` | — | Maximum accepted aspect ratio for a split particle. |
| `max_sharpness_ratio` | `Option<f64>` | `max_sharpness_ratio` | — | Maximum accepted sharpness ratio for a split particle. |
| `volume` | `Option<SplitFilterVolume>` | `volume` | — | Volume-based filtering rules (see `SplitFilterVolume` above). |

### `SplitFilterConfig`

| Field | Rust type | YAML key | Default | Meaning |
|---|---|---|---|---|
| `input` | `InputPath` | `input` | — | Input path (mesh, or folder of meshes, to split). |
| `output` | `SplitFilterOutput` | `output` | — | Output destination for split particles. |
| `filter` | `Option<SplitFilterRules>` | `filter` | — | Optional filtering rules applied to split particles. |

## Index

| Function | Source | Summary |
|---|---|---|
| `load_yaml` | `src/config/mod.rs:47` | Reads a file and deserializes it as YAML into a typed config struct. |
| `parse_box_dimensions` | `src/config/mod.rs:58` | Converts a 3- or 6-element dimensions slice into a `BoundingBox`. |
| `parse_usize_like` | `src/config/deserialize.rs:4` | Parses a `usize` from a string, stripping underscore separators. |
| `deserialize_usize_flexible` | `src/config/deserialize.rs:17` | Serde `deserialize_with` helper: accepts a YAML number or numeric string as `usize`. |
| `deserialize_option_usize_flexible` | `src/config/deserialize.rs:39` | Serde `deserialize_with` helper: accepts a YAML number/string/null as `Option<usize>`. |
| `parse_i32_like` | `src/config/deserialize.rs:61` | Parses an `i32` from a string, stripping underscore separators. |
| `deserialize_option_i32_flexible` | `src/config/deserialize.rs:73` | Serde `deserialize_with` helper: accepts a YAML number/string/null as `Option<i32>`. |
| `default_backend` | `src/config/acceleration.rs:21` | Serde default for `backend`: `"wgpu"`. |
| `default_true` | `src/config/acceleration.rs:25` | Serde default for `cpu_fallback`: `true`. |
| `default_gpu_min_voxels` | `src/config/acceleration.rs:29` | Serde default for `gpu_min_voxels`: `250_000`. |
| `default_gpu_precision` | `src/config/acceleration.rs:33` | Serde default for `gpu_precision`: `"f32"`. |
| `AccelerationConfig::default` | `src/config/acceleration.rs:38` | Rust-level `Default` impl matching the serde defaults. |

## Functions

#### load_yaml

- **Signature:** `pub fn load_yaml<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T>`
- **Source:** `src/config/mod.rs:47`
- **Purpose:** Read a file from disk and deserialize its contents as YAML into any type implementing `Deserialize`.
- **Parameters:**
  - `path` — filesystem path to the YAML config file.
- **Returns:** `Result<T>` — the deserialized config struct, or a `RustMsptError`.
- **Side effects:** Reads the file from disk (`fs::read_to_string`).
- **Notes:** Propagates an `Io` error if the file is missing/unreadable, or a `Yaml` error if the content does not deserialize into `T` (via `?` on `serde_yaml::from_str`).

#### parse_box_dimensions

- **Signature:** `pub fn parse_box_dimensions(dimensions: &[f64]) -> Result<crate::types::BoundingBox>`
- **Source:** `src/config/mod.rs:58`
- **Purpose:** Convert a flat dimensions array from `BoxConfig` into a `BoundingBox`.
- **Parameters:**
  - `dimensions` — slice of `f64`; either 3 elements (`[size_x, size_y, size_z]`, box placed at the origin via `BoundingBox::from_size`) or 6 elements (`[min_x, min_y, min_z, max_x, max_y, max_z]`, explicit min/max corners).
- **Returns:** `Result<BoundingBox>`.
- **Side effects:** None.
- **Notes:** Returns `RustMsptError::InvalidConfig` for any length other than 3 or 6.

#### parse_usize_like

`pub fn parse_usize_like(value: &str) -> std::result::Result<usize, String>` — `src/config/deserialize.rs:4`. Strips `_` separators from the string and parses it as `usize`, returning a formatted error message on failure. No side effects.

#### deserialize_usize_flexible

- **Signature:** `pub fn deserialize_usize_flexible<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<usize, D::Error>`
- **Source:** `src/config/deserialize.rs:17`
- **Purpose:** Serde `deserialize_with` helper allowing a `usize` field to be given in YAML as either a bare integer or a (possibly underscore-separated) string.
- **Parameters:**
  - `deserializer` — the serde `Deserializer` supplied by the derive macro.
- **Returns:** `Result<usize, D::Error>` — the parsed value, or a custom deserialize error.
- **Side effects:** None.
- **Notes:** Internally deserializes into an untagged `enum Value { Num(u64), Str(String) }`; a `Num` is range-checked via `usize::try_from`, a `Str` is parsed via `parse_usize_like`. Used (without `Option`) on `OptimizationParams::max_iterations` and `OptimizationParams::mc_samples`, both of which remain mandatory fields despite the flexible parsing since no `#[serde(default)]` is set.

#### deserialize_option_usize_flexible

- **Signature:** `pub fn deserialize_option_usize_flexible<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<usize>, D::Error>`
- **Source:** `src/config/deserialize.rs:39`
- **Purpose:** Serde `deserialize_with` helper allowing an `Option<usize>` field to be given in YAML as a number, a numeric/underscore string, or omitted/null.
- **Parameters:**
  - `deserializer` — the serde `Deserializer` supplied by the derive macro.
- **Returns:** `Result<Option<usize>, D::Error>`.
- **Side effects:** None.
- **Notes:** Used on fields such as `MeasurementParams::mc_samples`, `OptimizationParams::adaptive_temp_window`/`prune_max_rounds`/`prune_eval_samples`/`islands`/`migration_interval`, and `SplitFilterVolume::bins`. Always paired with `#[serde(default, deserialize_with = "...")]` so the key may be omitted entirely.

#### parse_i32_like

`pub fn parse_i32_like(value: &str) -> std::result::Result<i32, String>` — `src/config/deserialize.rs:61`. Strips `_` separators from the string and parses it as `i32`, returning a formatted error message on failure. No side effects.

#### deserialize_option_i32_flexible

- **Signature:** `pub fn deserialize_option_i32_flexible<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<i32>, D::Error>`
- **Source:** `src/config/deserialize.rs:73`
- **Purpose:** Serde `deserialize_with` helper allowing an `Option<i32>` field to be given in YAML as a number, a numeric/underscore string, or omitted/null.
- **Parameters:**
  - `deserializer` — the serde `Deserializer` supplied by the derive macro.
- **Returns:** `Result<Option<i32>, D::Error>`.
- **Side effects:** None.
- **Notes:** Internally deserializes into an untagged `enum Value { Num(i64), Str(String) }`, so unlike the usize variant it accepts negative numbers (used for fields like `slice_start`/`slice_end` in `CropInput`, and `cpu_max` across measurement/optimization/packing, where negative values may carry pipeline-specific meaning such as "use all but N cores").
