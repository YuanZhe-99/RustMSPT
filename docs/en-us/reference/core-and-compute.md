# Core and Compute Reference

This page documents the crate root and entry points (`src/lib.rs`, `src/main.rs`, `src/error.rs`, `src/types.rs`), the standalone diagnostic binary `src/bin/precision_test.rs`, and the CPU/GPU backend-selection layer in `src/compute/` (`mod.rs`, `backend.rs`, `policy.rs`).

## Index

| Item | Location | Summary |
|---|---|---|
| `RustMsptError` | `src/error.rs:4` | Crate-wide error enum covering I/O, YAML, TIFF, config, mesh, and GPU failures. |
| `Result` | `src/error.rs:27` | Type alias `Result<T> = std::result::Result<T, RustMsptError>` used throughout the crate. |
| `Cli` | `src/main.rs:19` | Top-level clap CLI struct wrapping a `Commands` subcommand. |
| `Commands` | `src/main.rs:25` | Enum of the 7 CLI subcommands (Forge/Measure/Optimize/Pack/Scale/Crop/SplitFilter). |
| `default_config_path` | `src/main.rs:85` | Builds the default config path under `data/input/`. |
| `pick_config_path` | `src/main.rs:90` | Chooses a user-supplied config path or falls back to the default. |
| `main` (main.rs) | `src/main.rs:100` | CLI entry point: parses args, loads config, applies overrides, runs the selected pipeline. |
| `main` (precision_test.rs) | `src/bin/precision_test.rs:5` | Standalone diagnostic binary comparing S2 computation precision/performance across CPU exact, CPU Monte Carlo, and GPU Monte Carlo methods. |
| `Vec3` | `src/types.rs:2` | 3D vector of `f64` components with basic vector algebra methods. |
| `Vec3::new` | `src/types.rs:10` | Constructs a vector from x/y/z components. |
| `Vec3::add` | `src/types.rs:15` | Vector addition. |
| `Vec3::sub` | `src/types.rs:20` | Vector subtraction. |
| `Vec3::scale` | `src/types.rs:25` | Scalar multiplication. |
| `Vec3::dot` | `src/types.rs:30` | Dot product. |
| `Vec3::cross` | `src/types.rs:35` | Cross product. |
| `BoundingBox` | `src/types.rs:45` | Axis-aligned box defined by `min`/`max` corners. |
| `BoundingBox::from_size` | `src/types.rs:52` | Builds a box from the origin to a given size. |
| `BoundingBox::size` | `src/types.rs:60` | Returns the box's side lengths. |
| `BoundingBox::volume` | `src/types.rs:65` | Returns the box's (non-negative) volume. |
| `BoundingBox::contains_point` | `src/types.rs:71` | Tests whether a point lies inside or on the box boundary. |
| `Triangle` | `src/types.rs:82` | Index triple `(a, b, c)` referencing a mesh's vertex array. |
| `Mesh` | `src/types.rs:89` | Vertex/face container: `vertices: Vec<Vec3>`, `faces: Vec<Triangle>`. |
| `Mesh::empty` | `src/types.rs:96` | Constructs an empty mesh. |
| `Mesh::is_empty` | `src/types.rs:104` | True if the mesh has no vertices or no faces. |
| `AccelerationMode` | `src/compute/backend.rs:5` | Enum of requested compute modes: `Auto` (default), `Cpu`, `Gpu`. |
| `AccelerationMode::fmt` (Display) | `src/compute/backend.rs:12` | Formats the mode as `"auto"`/`"cpu"`/`"gpu"`. |
| `BackendCaps` | `src/compute/backend.rs:23` | Reported capabilities of a selected backend (name, GPU support, buffer size limits). |
| `ComputeBackend` | `src/compute/backend.rs:31` | Enum of concrete backends actually selected: `Cpu`, or `Gpu { .. }` (feature-gated). |
| `ComputeBackend::name` | `src/compute/backend.rs:43` | Human-readable backend name. |
| `ComputeBackend::is_gpu` | `src/compute/backend.rs:52` | Whether the backend is a GPU backend. |
| `ComputeBackend::caps` | `src/compute/backend.rs:64` | Returns a `BackendCaps` summary for the backend. |
| `ComputeBackend::fmt` (Display) | `src/compute/backend.rs:87` | Formats as `"cpu"` or `"gpu/wgpu/{adapter_name}"`. |
| `FallbackReason` | `src/compute/policy.rs:4` | Records why a requested backend could not be honored and what was requested instead. |
| `BackendSelection` | `src/compute/policy.rs:10` | Result of backend selection: chosen `ComputeBackend` plus optional `FallbackReason`. |
| `select_backend` | `src/compute/policy.rs:21` | Central CPU/GPU/Auto dispatch policy used by compute-heavy pipelines. |

## Module role: `lib.rs`

`src/lib.rs` is the crate root. It declares the top-level public modules (`compute`, `config`, `error`, `geometry`, `io`, `pipeline`, `types`), conditionally declares `gpu` under the `gpu` cargo feature, and re-exports `Result`/`RustMsptError` from `error` at the crate root (`rustmspt::Result`, `rustmspt::RustMsptError`). It contains no functions of its own.

## Module role: `error.rs`

`src/error.rs` defines the crate-wide error type used by (almost) every fallible function in the library.

- **`RustMsptError`** (`src/error.rs:4`) — a `thiserror`-derived enum with variants: `Io` (wraps `std::io::Error`, via `#[from]`), `Yaml` (wraps `serde_yaml::Error`, via `#[from]`), `Tiff` (wraps `tiff::TiffError`, via `#[from]`), `InvalidConfig(String)`, `InvalidMesh(String)`, `NotAvailable(String)`, and `Gpu(String)`. The `#[from]` variants let `?` auto-convert `io::Error`/`serde_yaml::Error`/`tiff::TiffError` into `RustMsptError` at call sites.
- **`Result<T>`** (`src/error.rs:27`) — alias for `std::result::Result<T, RustMsptError>`, used as the return type across config loading, geometry, I/O, and pipeline code.

Neither item is a function, so no per-function entry is given per this page's documentation scope.

## main.rs

`src/main.rs` is the `rustmspt` binary's entry point: a clap-based CLI that loads a YAML config for the requested subcommand, applies optional `--input`/`--output` path overrides, and runs the corresponding pipeline.

### CLI structure

`Cli` (`src/main.rs:19`) is the top-level `#[derive(Parser)]` struct; it holds a single `command: Commands` field. `Commands` (`src/main.rs:25`) is a `#[derive(Subcommand)]` enum with seven variants, each carrying the same three optional arguments:

| Subcommand | Config struct loaded | Default config file |
|---|---|---|
| `Forge` | `ForgingConfig` | `forge_config.yaml` |
| `Measure` | `MeasurementConfig` | `measure_config.yaml` |
| `Optimize` | `OptimizationConfig` | `optimize_config.yaml` |
| `Pack` | `PackingConfig` | `pack_config.yaml` |
| `Scale` | `ScaleConfig` | `scale_config.yaml` |
| `Crop` | `CropConfig` | `crop_config.yaml` |
| `SplitFilter` | `SplitFilterConfig` | `split_filter_config.yaml` |

Each variant accepts `--config <PathBuf>`, `--input <PathBuf>`, and `--output <PathBuf>`, all optional. `--config` selects which YAML file to load (see `pick_config_path`); `--input`/`--output`, when present, overwrite the corresponding path field(s) on the loaded config object before the pipeline runs.

#### default_config_path

- **Signature:** `fn default_config_path(file_name: &str) -> PathBuf`
- **Source:** `src/main.rs:85`
- **Purpose:** Builds the default configuration file path under `data/input/`.
- **Parameters:**
  - `file_name` — the config file's base name (e.g. `"pack_config.yaml"`).
- **Returns:** `PathBuf` equal to `data/input/<file_name>`.
- **Side effects:** None (pure path construction; does not check the file exists).

#### pick_config_path

- **Signature:** `fn pick_config_path(config: Option<PathBuf>, file_name: &str) -> PathBuf`
- **Source:** `src/main.rs:90`
- **Purpose:** Resolves the config path to use for a subcommand: the user-supplied `--config` value if given, otherwise the default under `data/input/`.
- **Parameters:**
  - `config` — the optional `--config` CLI argument.
  - `file_name` — the fallback file's base name, passed to `default_config_path` when `config` is `None`.
- **Returns:** The resolved `PathBuf`.
- **Side effects:** None.

#### main

- **Signature:** `fn main() -> anyhow::Result<()>`
- **Source:** `src/main.rs:100`
- **Purpose:** Parses CLI arguments, loads the YAML config for the selected subcommand, applies `--input`/`--output` overrides, and runs the corresponding pipeline.
- **Parameters:** None (reads `std::env::args` via `Cli::parse()`).
- **Returns:** `Ok(())` on success; an `anyhow::Error` if config loading, path resolution, or pipeline execution (`Pipeline::run`) fails.
- **Side effects:** Reads a YAML config file from disk; for each subcommand, constructs the matching pipeline struct (`ForgePipeline`, `MeasurePipeline`, `OptimizePipeline`, `PackPipeline`, `ScalePipeline`, `CropPipeline`, `SplitFilterPipeline`) and calls `.run()`, which in turn reads input mesh/image files and writes output files as directed by the config. Prints progress to stdout via the underlying pipelines.
- **Notes:** This is the sole entry point of the `rustmspt` binary. The `match cli.command { ... }` block is a flat dispatch: each arm repeats the same three-step pattern (resolve path → load & mutate config → construct and run pipeline) for its subcommand; there is no shared helper across arms beyond `pick_config_path`.

## bin/precision_test.rs

`src/bin/precision_test.rs` builds as a separate Cargo binary target (`precision_test`), distinct from the `rustmspt` library and CLI. It is a manual diagnostic tool for comparing S2 (two-point correlation) computation precision and performance across three methods: CPU exact (occupancy grid + FFT), CPU Monte Carlo (occupancy grid sampling), and GPU Monte Carlo (continuous ray-casting, only when built with the `gpu` feature). It is not exercised by the test suite or by any library pipeline.

#### main

- **Signature:** `fn main()`
- **Source:** `src/bin/precision_test.rs:5`
- **Purpose:** Loads a fixed sample mesh, computes its volume fraction and bounding box, then runs `calculate_s2` at several voxel pitches under both `"exact"` and `"monte_carlo"` CPU methods (and, with the `gpu` feature, via `GpuS2Pipeline::calculate_s2_gpu`), printing S2 curve samples, L2 deviation from a reference, and (for GPU) elapsed time.
- **Parameters:** None.
- **Returns:** `()`. Panics (via `.expect(...)`) if `data/input/particles.stl` cannot be loaded or has no bounding box.
- **Side effects:** Reads `data/input/particles.stl` from disk; prints diagnostic tables to stdout; when the `gpu` feature is enabled, attempts to initialize a GPU S2 pipeline (`GpuS2Pipeline::new`) and, on failure, prints an error and returns early instead of panicking.
- **Notes:** This binary hardcodes its input path (`data/input/particles.stl`), pitch sweep (`[0.5, 1.0, 2.0, 4.0, 8.0]`), radius count (`r_max = 10`), and Monte Carlo sample count (`n = 50000`) — it is meant to be run manually during development, not as part of any automated pipeline. It documents in its own printed output the key methodological difference between the three methods: GPU MC is pitch-independent (no voxelization), while CPU MC and CPU exact both depend on the voxel occupancy grid built at the given pitch.

## types.rs

`src/types.rs` defines the crate's basic geometric value types: `Vec3` (3D vector), `BoundingBox` (axis-aligned box), `Triangle` (index triple), and `Mesh` (vertex/face container). These are used pervasively across `geometry`, `pipeline`, `io`, and `compute`.

### Vec3

| Field | Type | Description |
|---|---|---|
| `x` | `f64` | X component. |
| `y` | `f64` | Y component. |
| `z` | `f64` | Z component. |

`Vec3` derives `Debug, Clone, Copy, PartialEq`; all methods take `self` by value (it is `Copy`).

#### new

`pub fn new(x: f64, y: f64, z: f64) -> Self` — `src/types.rs:10`. Constructs a vector from its three components. No side effects.

#### add

`pub fn add(self, other: Self) -> Self` — `src/types.rs:15`. Returns `self + other`, component-wise. No side effects.

#### sub

`pub fn sub(self, other: Self) -> Self` — `src/types.rs:20`. Returns `self - other`, component-wise. No side effects.

#### scale

`pub fn scale(self, factor: f64) -> Self` — `src/types.rs:25`. Returns `self` scaled by `factor` (each component multiplied). No side effects.

#### dot

`pub fn dot(self, other: Self) -> f64` — `src/types.rs:30`. Returns the dot product `x*x' + y*y' + z*z'`. No side effects.

#### cross

`pub fn cross(self, other: Self) -> Self` — `src/types.rs:35`. Returns the cross product `self × other`, a vector orthogonal to both inputs. No side effects.

### BoundingBox

| Field | Type | Description |
|---|---|---|
| `min` | `Vec3` | Minimum corner. |
| `max` | `Vec3` | Maximum corner. |

`BoundingBox` derives `Debug, Clone, Copy, PartialEq`.

#### from_size

- **Signature:** `pub fn from_size(size: Vec3) -> Self`
- **Source:** `src/types.rs:52`
- **Purpose:** Builds an axis-aligned box spanning from the origin `(0,0,0)` to `size`.
- **Parameters:** `size` — the box's extent along each axis.
- **Returns:** A `BoundingBox` with `min = (0,0,0)` and `max = size`.
- **Side effects:** None.
- **Notes:** Does not validate that `size` components are non-negative; a negative component produces an inverted box (see `volume`, which clamps negative extents).

#### size

`pub fn size(&self) -> Vec3` — `src/types.rs:60`. Returns `max - min` (the box's side lengths). No side effects.

#### volume

- **Signature:** `pub fn volume(&self) -> f64`
- **Source:** `src/types.rs:65`
- **Purpose:** Computes the box's volume.
- **Parameters:** None (`&self`).
- **Returns:** The product of the three side lengths, each clamped to `0.0` if negative — so an inverted or degenerate box yields `0.0` rather than a negative volume.
- **Side effects:** None.

#### contains_point

`pub fn contains_point(&self, p: Vec3) -> bool` — `src/types.rs:71`. Returns `true` if `p` lies within `[min, max]` inclusive on all three axes (boundary points count as contained). No side effects.

### Triangle

| Field | Type | Description |
|---|---|---|
| `a` | `usize` | Index of the first vertex in the owning `Mesh::vertices`. |
| `b` | `usize` | Index of the second vertex. |
| `c` | `usize` | Index of the third vertex. |

`Triangle` derives `Debug, Clone, PartialEq` and carries no methods; it exists purely as a `Mesh` field type.

### Mesh

| Field | Type | Description |
|---|---|---|
| `vertices` | `Vec<Vec3>` | All vertex positions. |
| `faces` | `Vec<Triangle>` | Triangles indexing into `vertices`. |

`Mesh` derives `Debug, Clone, PartialEq`.

#### empty

`pub fn empty() -> Self` — `src/types.rs:96`. Constructs a `Mesh` with empty `vertices` and `faces` vectors. No side effects.

#### is_empty

`pub fn is_empty(&self) -> bool` — `src/types.rs:104`. Returns `true` if the mesh has no vertices **or** no faces (i.e. `vertices.is_empty() || faces.is_empty()`, not a strict "both empty" check). No side effects.

## compute/mod.rs

`src/compute/mod.rs` is the module hub for the `compute` package: it declares the `backend` and `policy` submodules and re-exports `AccelerationMode`, `BackendCaps`, `ComputeBackend` (from `backend`) and `select_backend` (from `policy`) at the `compute::` path. It contains no functions of its own.

The file also contains a `#[cfg(test)] mod tests` block with six unit tests (`cpu_backend_is_not_gpu`, `auto_falls_back_for_small_workload`, `default_mode_is_auto`, `display_formats_correctly`, `cpu_caps_show_no_gpu_support`, and the feature-gated `gpu_adapter_detection`) that exercise `select_backend`'s CPU/Auto/Gpu branches, `AccelerationMode`/`ComputeBackend` `Display` formatting, and default-mode behavior. These are test code, not production functions, and are not documented individually here.

## compute/backend.rs

This file defines the backend *type* hierarchy: `AccelerationMode` (what the caller requested), `ComputeBackend` (what was actually selected), and `BackendCaps` (the selected backend's reported capabilities). `compute/policy.rs`'s `select_backend` is the function that turns an `AccelerationMode` into a `ComputeBackend`.

### AccelerationMode

`AccelerationMode` (`src/compute/backend.rs:5`) is a `#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Default)]` enum with three variants: `Auto` (`#[default]`), `Cpu`, `Gpu`. It deserializes from lowercase strings (`#[serde(rename_all = "lowercase")]`) so config YAML can specify `acceleration: auto|cpu|gpu`.

#### fmt (Display for AccelerationMode)

`impl fmt::Display for AccelerationMode` — `src/compute/backend.rs:12`. Formats `Auto`/`Cpu`/`Gpu` as the lowercase strings `"auto"`/`"cpu"`/`"gpu"`. No side effects.

### BackendCaps

`BackendCaps` (`src/compute/backend.rs:23`) — `#[derive(Debug, Clone)]`, reports capabilities of a chosen backend:

| Field | Type | Description |
|---|---|---|
| `name` | `String` | Backend or GPU adapter name (`"cpu"` for the CPU backend). |
| `supports_gpu` | `bool` | Whether this backend is GPU-backed. |
| `max_buffer_size` | `u64` | Adapter's max buffer size in bytes (`0` for CPU). |
| `max_storage_buffer_binding_size` | `u64` | Adapter's max storage-buffer binding size in bytes (`0` for CPU); used by `select_backend` to enforce a configured GPU memory limit. |

### ComputeBackend

`ComputeBackend` (`src/compute/backend.rs:31`) — `#[derive(Debug, Clone)]` enum of concrete, resolved backends:

- `Cpu` — always available.
- `Gpu { adapter_name: String, max_buffer_size: u64, max_storage_buffer_binding_size: u64 }`

> **Feature-gated:** The `Gpu` variant, and the GPU-specific match arms in `name`, `is_gpu`, `caps`, and the `Display` impl, only exist when the crate is built with the `gpu` cargo feature enabled. Without it, `ComputeBackend` is effectively a unit-like enum with only `Cpu`, and `is_gpu()` always returns `false`.

#### name

`pub fn name(&self) -> &str` — `src/compute/backend.rs:43`. Returns `"cpu"` for `ComputeBackend::Cpu`, or the GPU adapter's name for `ComputeBackend::Gpu` (feature-gated). No side effects.

#### is_gpu

`pub fn is_gpu(&self) -> bool` — `src/compute/backend.rs:52`. Returns `true` only for the `Gpu` variant when the `gpu` feature is enabled; unconditionally `false` when the feature is disabled. No side effects.

#### caps

- **Signature:** `pub fn caps(&self) -> BackendCaps`
- **Source:** `src/compute/backend.rs:64`
- **Purpose:** Produces a `BackendCaps` summary describing this backend's capabilities.
- **Parameters:** None (`&self`).
- **Returns:** For `Cpu`: `BackendCaps { name: "cpu", supports_gpu: false, max_buffer_size: 0, max_storage_buffer_binding_size: 0 }`. For `Gpu` (feature-gated): a `BackendCaps` populated from the adapter's stored fields, with `supports_gpu: true`.
- **Side effects:** None (reads already-stored adapter info; does not query the GPU again).

#### fmt (Display for ComputeBackend)

`impl fmt::Display for ComputeBackend` — `src/compute/backend.rs:87`. Formats `Cpu` as `"cpu"`; formats `Gpu` (feature-gated) as `"gpu/wgpu/{adapter_name}"`. No side effects.

## compute/policy.rs

This is the central CPU/GPU/Auto dispatch policy for the crate. Any pipeline stage that can optionally run on the GPU (S2 correlation, packing collision checks, etc.) calls `select_backend` once to decide which `ComputeBackend` to use, rather than duplicating GPU-availability and fallback logic.

### FallbackReason

`FallbackReason` (`src/compute/policy.rs:4`) — `#[derive(Debug, Clone)]`:

| Field | Type | Description |
|---|---|---|
| `requested` | `AccelerationMode` | The mode the caller originally asked for (`Gpu` or `Auto`; `Cpu` never produces a fallback). |
| `reason` | `String` | Human-readable explanation of why the fallback to CPU occurred. |

### BackendSelection

`BackendSelection` (`src/compute/policy.rs:10`) — `#[derive(Debug, Clone)]`, the return type of `select_backend`:

| Field | Type | Description |
|---|---|---|
| `backend` | `ComputeBackend` | The backend actually selected (always `Cpu` unless GPU init succeeded and no limit was exceeded). |
| `fallback` | `Option<FallbackReason>` | `Some` if the requested mode could not be honored and the selection fell back to CPU; `None` if the requested backend was used as-is. |

#### select_backend

- **Signature:** `pub fn select_backend(requested: AccelerationMode, gpu_min_voxels: Option<usize>, gpu_memory_limit_mb: Option<u64>, workload_voxels: usize) -> BackendSelection`
- **Source:** `src/compute/policy.rs:21`
- **Purpose:** Resolves a requested `AccelerationMode` (`Cpu`/`Gpu`/`Auto`) plus workload/config parameters into a concrete `BackendSelection`, applying GPU availability, memory-limit, and workload-size checks.
- **Parameters:**
  - `requested` — the mode the caller/config asked for.
  - `gpu_min_voxels` — for `Auto` mode only: minimum workload size (in voxels) below which GPU is not attempted; defaults to `250_000` if `None`.
  - `gpu_memory_limit_mb` — optional cap (in MB) on GPU memory; if the adapter's `max_storage_buffer_binding_size` is smaller than this limit, the selection falls back to CPU. Unused when the `gpu` feature is disabled (marked `#[allow(unused_variables)]` in that configuration).
  - `workload_voxels` — the current workload's voxel count, compared against `gpu_min_voxels` in `Auto` mode.
- **Returns:** A `BackendSelection`:
  - `requested == Cpu`: always `{ backend: Cpu, fallback: None }`.
  - `requested == Gpu`: with the `gpu` feature enabled, attempts `crate::gpu::try_init_gpu()`; on success, checks `gpu_memory_limit_mb` against the adapter's `max_storage_buffer_binding_size` (falls back to CPU with a `FallbackReason` if the limit is exceeded), otherwise returns `{ backend: Gpu { .. }, fallback: None }`. On GPU init failure, or when the `gpu` feature is disabled, returns `{ backend: Cpu, fallback: Some(FallbackReason { requested: Gpu, reason: "GPU init failed: ..." | "cargo feature 'gpu' is not enabled" }) }`.
  - `requested == Auto`: first compares `workload_voxels` against `gpu_min_voxels.unwrap_or(250_000)`; if below threshold, immediately returns CPU with a fallback reason citing the voxel counts, without attempting GPU init at all. Otherwise, follows the same GPU-init/memory-limit logic as the `Gpu` arm (with `requested: Auto` in any resulting `FallbackReason`).
- **Side effects:** When the `gpu` feature is enabled and `requested` is `Gpu` or `Auto` with a workload at or above the voxel threshold, calls `crate::gpu::try_init_gpu()`, which may initialize a wgpu adapter — documented elsewhere as a potentially heavy first call (adapter/device enumeration and creation).
- **Notes:** `Auto` mode's threshold check happens *before* any GPU probing, so small workloads never pay the GPU-init cost even if a GPU is available. When the `gpu` feature is not compiled in, both the `Gpu` and `Auto` arms always resolve to `Cpu` with a fallback reason of `"cargo feature 'gpu' is not enabled"`, regardless of `workload_voxels` (for `Gpu`) or after the threshold check (for `Auto`). The `gpu_memory_limit_mb` check only triggers when `max_storage_buffer_binding_size > 0`, avoiding a false-positive fallback on adapters that report `0` for this field.
