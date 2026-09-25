# Core and Compute Reference

This page documents the crate root and entry points (`src/lib.rs`, `src/main.rs`, `src/error.rs`, `src/types.rs`), the build identity in `src/version.rs` and its build script `build.rs`, the standalone diagnostic binary `src/bin/precision_test.rs`, and the CPU/GPU backend-selection layer in `src/compute/` (`mod.rs`, `backend.rs`, `policy.rs`).

## Index

| Item | Location | Summary |
|---|---|---|
| `RustMsptError` | `src/error.rs:4` | Crate-wide error enum covering I/O, YAML, TIFF, config, mesh, and GPU failures. |
| `Result` | `src/error.rs:27` | Type alias `Result<T> = std::result::Result<T, RustMsptError>` used throughout the crate. |
| `Cli` | `src/main.rs:27` | Top-level clap CLI struct wrapping a `Commands` subcommand, carrying the build identity as its `--version` string. |
| `Commands` | `src/main.rs:33` | Enum of the 12 CLI subcommands, including `version`. |
| `default_config_path` | `src/main.rs:179` | Builds the default config path under `data/input/`. |
| `pick_config_path` | `src/main.rs:184` | Chooses a user-supplied config path or falls back to the default. |
| `main` (main.rs) | `src/main.rs:194` | CLI entry point: parses args, loads config, applies overrides, runs the selected pipeline. |
| `main` (precision_test.rs) | `src/bin/precision_test.rs:6` | Standalone diagnostic binary comparing S2 computation precision/performance across CPU exact, CPU Monte Carlo, and GPU Monte Carlo methods. |
| `BuildIdentity` | `src/version.rs:18` | What this binary is: version, git commit, worktree dirtiness, features, build platform. |
| `build_identity` | `src/version.rs:36` | Returns the compiled-in build identity; the single source of truth for R1. |
| `BuildIdentity::version_detail` | `src/version.rs:64` | Identity as one line without the program name, for clap's `--version`. |
| `BuildIdentity::version_line` | `src/version.rs:92` | Identity as one line including the program name. |
| `identity_json` | `src/version.rs:103` | Serializes the identity as pretty-printed JSON. |
| `non_empty` (version.rs) | `src/version.rs:4` | Maps an empty build-script env string to `None`. |
| `build_identity_version_line` | `src/main.rs:174` | Leaks the version detail as a `&'static str` for clap. |
| `git_output` | `build.rs:11` | Runs a git command, returning `None` on any failure. |
| `rerun_if_exists` | `build.rs:25` | Emits a `rerun-if-changed` line only for paths that exist. |
| `emit_rerun_triggers` | `build.rs:37` | Emits every rerun trigger that can change the recorded identity. |
| `enabled_features` | `build.rs:62` | Reads the enabled cargo features from `CARGO_FEATURE_*`. |
| `main` (build.rs) | `build.rs:80` | Stamps the identity into compile-time environment variables. |
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
| `Triangle` | `src/types.rs:114` | Index triple `(a, b, c)` referencing a mesh's vertex array. |
| `Mesh` | `src/types.rs:121` | Vertex/face container: `vertices: Vec<Vec3>`, `faces: Vec<Triangle>`. |
| `Mesh::empty` | `src/types.rs:128` | Constructs an empty mesh. |
| `Mesh::is_empty` | `src/types.rs:136` | True if the mesh has no vertices or no faces. |
| `RenderedImage` | `src/types.rs` | Top-row-first RGBA8 image buffer. |
| `RenderedImage::new` | `src/types.rs` | Constructs an RGBA8 image from bytes. |
| `RenderedImage::filled` | `src/types.rs` | Allocates a solid-color RGBA8 image. |
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
| `select_backend` | `src/compute/policy.rs:22` | Central CPU/GPU/Auto dispatch policy used by compute-heavy pipelines. |
| `select_backend_for_workload` | `src/compute/policy.rs` | Unit-aware backend selection for pixels or other work items. |
| `configured_mode` | `src/compute/policy.rs:142` | Resolve strict environment override. |
| `resolve_execution` | `src/compute/policy.rs:162` | Resolve method support, workload budget and fallback. |

## Module role: `lib.rs`

`src/lib.rs` is the crate root. It declares the top-level public modules (`compute`, `config`, `error`, `geometry`, `io`, `pipeline`, `types`), conditionally declares `gpu` under the `gpu` cargo feature, and re-exports `Result`/`RustMsptError` from `error` at the crate root (`rustmspt::Result`, `rustmspt::RustMsptError`). It contains no functions of its own.

## Module role: `error.rs`

`src/error.rs` defines the crate-wide error type used by (almost) every fallible function in the library.

- **`RustMsptError`** (`src/error.rs:4`) — a `thiserror`-derived enum with I/O, YAML, TIFF, image, config, mesh, availability, and GPU variants. `Image` wraps `image::ImageError` via `#[from]`.
- **`Result<T>`** (`src/error.rs:27`) — alias for `std::result::Result<T, RustMsptError>`, used as the return type across config loading, geometry, I/O, and pipeline code.

Neither item is a function, so no per-function entry is given per this page's documentation scope.

## main.rs

`src/main.rs` is the `rustmspt` binary's entry point: a clap-based CLI that loads a YAML config for the requested subcommand, applies optional `--input`/`--output` path overrides, and runs the corresponding pipeline.

### CLI structure

`Cli` holds a single `command: Commands` field. `Commands` has eight variants, each carrying the same three optional arguments:

| Subcommand | Config struct loaded | Default config file |
|---|---|---|
| `Forge` | `ForgingConfig` | `forge_config.yaml` |
| `Measure` | `MeasurementConfig` | `measure_config.yaml` |
| `Optimize` | `OptimizationConfig` | `optimize_config.yaml` |
| `Pack` | `PackingConfig` | `pack_config.yaml` |
| `Scale` | `ScaleConfig` | `scale_config.yaml` |
| `Crop` | `CropConfig` | `crop_config.yaml` |
| `SplitFilter` | `SplitFilterConfig` | `split_filter_config.yaml` |
| `Render` | `RenderConfig` | `render_config.yaml` |

Each pipeline variant accepts `--config <PathBuf>`, `--input <PathBuf>`, and `--output <PathBuf>`, all optional. `--config` selects which YAML file to load (see `pick_config_path`); `--input`/`--output`, when present, overwrite the corresponding path field(s) on the loaded config object before the pipeline runs.

`Version { json: bool }` is the exception: it takes no config and reads no file. It prints `build_identity().version_line()`, or `identity_json(...)` with `--json`. The same identity is clap's `--version` string, via `build_identity_version_line`.

#### default_config_path

- **Signature:** `fn default_config_path(file_name: &str) -> PathBuf`
- **Source:** `src/main.rs:179`
- **Purpose:** Builds the default configuration file path under `data/input/`.
- **Parameters:**
  - `file_name` — the config file's base name (e.g. `"pack_config.yaml"`).
- **Returns:** `PathBuf` equal to `data/input/<file_name>`.
- **Side effects:** None (pure path construction; does not check the file exists).

#### pick_config_path

- **Signature:** `fn pick_config_path(config: Option<PathBuf>, file_name: &str) -> PathBuf`
- **Source:** `src/main.rs:184`
- **Purpose:** Resolves the config path to use for a subcommand: the user-supplied `--config` value if given, otherwise the default under `data/input/`.
- **Parameters:**
  - `config` — the optional `--config` CLI argument.
  - `file_name` — the fallback file's base name, passed to `default_config_path` when `config` is `None`.
- **Returns:** The resolved `PathBuf`.
- **Side effects:** None.

#### main

- **Signature:** `fn main() -> anyhow::Result<()>`
- **Source:** `src/main.rs:194`
- **Purpose:** Parses CLI arguments, loads the YAML config for the selected subcommand, applies `--input`/`--output` overrides, and runs the corresponding pipeline.
- **Parameters:** None (reads `std::env::args` via `Cli::parse()`).
- **Returns:** `Ok(())` on success; an `anyhow::Error` if config loading, path resolution, or pipeline execution (`Pipeline::run`) fails.
- **Side effects:** Reads a YAML config file from disk; for each subcommand, constructs the matching pipeline struct (`ForgePipeline`, `MeasurePipeline`, `OptimizePipeline`, `PackPipeline`, `ScalePipeline`, `CropPipeline`, `SplitFilterPipeline`) and calls `.run()`, which in turn reads input mesh/image files and writes output files as directed by the config. Prints progress to stdout via the underlying pipelines.
- **Notes:** This is the sole entry point of the `rustmspt` binary. The `match cli.command { ... }` block is a flat dispatch: each arm repeats the same three-step pattern (resolve path → load & mutate config → construct and run pipeline) for its subcommand; there is no shared helper across arms beyond `pick_config_path`.

## version.rs and build.rs

`src/version.rs` answers one question -- what is this binary? -- and is the only place that answers it. `rustmspt --version`, `rustmspt version [--json]`, and the identity stamped into placement outputs all read the same `BuildIdentity`, so they cannot disagree.

The values come from `build.rs`, which runs at compile time and stamps them into environment variables read by `env!`. Two properties are deliberate:

- **A checkout without git still builds.** Every git call goes through `git_output`, which returns `None` if git is missing, if there is no `.git`, or if the command fails (a repository with no commits). Undetermined values reach Rust as `None` and serialize as JSON `null`.
- **`null` is not `false`.** "We could not determine whether the worktree was dirty" and "the worktree was clean" are different claims, so `git_dirty` is `Option<bool>`. Cleanliness is only reported when a commit could also be named.

The honesty limit is worth stating: `git_dirty` describes the worktree at the moment `build.rs` last ran, which can predate an edit made after the last compile. `emit_rerun_triggers` narrows that window by rerunning the script whenever `build.rs`, `Cargo.toml`, `Cargo.lock`, `src/`, or the git refs change, but it cannot close it entirely.

#### BuildIdentity

- **Definition:** `src/version.rs:18`
- **Purpose:** What this binary is, as far as the build could determine it.

| Field | Type | Meaning |
|---|---|---|
| `name` | `&'static str` | Package name (`CARGO_PKG_NAME`). |
| `version` | `&'static str` | Package version (`CARGO_PKG_VERSION`). |
| `git_commit` | `Option<&'static str>` | Full 40-character commit sha1, or `None`. |
| `git_dirty` | `Option<bool>` | Whether the worktree had uncommitted changes when `build.rs` last ran; `None` when undetermined. |
| `features` | `Vec<&'static str>` | Enabled cargo features, sorted. |
| `target` | `Option<&'static str>` | Compilation target triple. |
| `host` | `Option<&'static str>` | Host triple of the machine that compiled it. |
| `profile` | `Option<&'static str>` | `debug` or `release`. |
| `source_date_epoch` | `Option<&'static str>` | `SOURCE_DATE_EPOCH` when set for a reproducible build. |

#### build_identity

- **Signature:** `pub fn build_identity() -> BuildIdentity`
- **Source:** `src/version.rs:36`
- **Purpose:** Returns the identity compiled into this binary.
- **Parameters:** None.
- **Returns:** A `BuildIdentity` with `None` wherever the build could not determine a value.
- **Side effects:** None -- every value is a compile-time constant; no process is spawned and no file is read at run time.
- **Notes:** `features` is parsed from a comma-separated list; an empty list means no cargo feature was enabled beyond none at all. Because `default = []` is itself a declared feature, a default build reports `["default"]`.

#### BuildIdentity::version_detail

- **Signature:** `pub fn version_detail(&self) -> String`
- **Source:** `src/version.rs:64`
- **Purpose:** Renders the identity as one line **without** the program name.
- **Returns:** For example `0.2.0 (git 0a8eb1c, clean; features: none)`.
- **Side effects:** None.
- **Notes:** clap prepends the program name to its `--version` string, so this must omit it or the name appears twice. An unknown commit renders as `git unknown`; unknown cleanliness renders as `dirt unknown`; an empty feature list renders as `none`.

#### BuildIdentity::version_line

- **Signature:** `pub fn version_line(&self) -> String`
- **Source:** `src/version.rs:92`
- **Purpose:** Renders the identity as one line **including** the program name.
- **Returns:** For example `rustmspt 0.2.0 (git 0a8eb1c, clean; features: none)`.
- **Side effects:** None.
- **Notes:** Exactly `format!("{name} {detail}")`, so `rustmspt version` and `rustmspt --version` print identical text.

#### identity_json

- **Signature:** `pub fn identity_json(identity: &BuildIdentity) -> String`
- **Source:** `src/version.rs:103`
- **Purpose:** Serializes a build identity as pretty-printed JSON.
- **Parameters:**
  - `identity` — the identity to render.
- **Returns:** A JSON object with every field present, `null` for undetermined values.
- **Side effects:** None.
- **Notes:** Falls back to `"{}"` if serialization somehow fails, so the function is total. The same object is embedded as `tool` in the placement record and run report.

#### non_empty (version.rs)

- **Signature:** `fn non_empty(value: &'static str) -> Option<&'static str>`
- **Source:** `src/version.rs:4`
- **Purpose:** Maps an empty build-script environment string to `None`.
- **Returns:** `Some(value)` when non-empty, `None` otherwise.
- **Side effects:** None.
- **Notes:** `build.rs` emits an empty string for every value it could not determine; this is the one place that convention is translated into `Option`.

#### git_output

- **Signature:** `fn git_output(args: &[&str]) -> Option<String>`
- **Source:** `build.rs:11`
- **Purpose:** Runs a git command in the crate directory and returns its trimmed stdout.
- **Parameters:**
  - `args` — arguments appended after `git -C <CARGO_MANIFEST_DIR>`.
- **Returns:** `Some(stdout)` when git exists and exited zero; `None` otherwise.
- **Side effects:** Spawns a git process at build time.
- **Notes:** Never panics. A missing git binary, an absent `.git`, and a repository with no commits are all `None` -- the build succeeds and the identity says it does not know.

#### rerun_if_exists

- **Signature:** `fn rerun_if_exists(path: &Path)`
- **Source:** `build.rs:25`
- **Purpose:** Emits a `cargo:rerun-if-changed` line only when the path exists.
- **Side effects:** Prints a cargo directive.
- **Notes:** Guarding on existence matters for `.git` entries: a worktree or submodule checkout has `.git` as a file, not a directory, and naming a nonexistent path would make cargo rerun the script on every build.

#### emit_rerun_triggers

- **Signature:** `fn emit_rerun_triggers()`
- **Source:** `build.rs:37`
- **Purpose:** Emits every rerun trigger that can change the recorded identity.
- **Side effects:** Prints cargo directives.
- **Notes:** Covers `build.rs`, `Cargo.toml`, `Cargo.lock`, `src/`, `.git/HEAD`, `.git/index`, `.git/packed-refs`, the branch ref named by `.git/HEAD`, and the `SOURCE_DATE_EPOCH` environment variable. `src/` is included so that editing any source file refreshes the dirty flag.

#### enabled_features

- **Signature:** `fn enabled_features() -> Vec<String>`
- **Source:** `build.rs:62`
- **Purpose:** Lists the cargo features enabled for this build, sorted.
- **Returns:** Feature names in cargo spelling (lowercase, hyphenated).
- **Side effects:** None.
- **Notes:** Reads `CARGO_FEATURE_*` from the environment, **not** `cfg!(feature = ...)`. A build script is compiled without the crate's own features, so `cfg!` inside `build.rs` would always report them absent.

#### main (build.rs)

- **Signature:** `fn main()`
- **Source:** `build.rs:80`
- **Purpose:** Stamps the git commit, worktree cleanliness, enabled features and build platform into compile-time environment variables.
- **Side effects:** Prints `cargo:rustc-env` and `cargo:rerun-if-changed` directives.
- **Notes:** Emits every value as a possibly-empty string; empty means "not determined". Cleanliness is only queried once a commit has been named, so `git_dirty` cannot claim `clean` for a tree whose commit is unknown. Nothing here prints `cargo:warning`, which would fail a `-D warnings` pipeline.

## bin/precision_test.rs

`src/bin/precision_test.rs` builds as a separate Cargo binary target (`precision_test`), distinct from the `rustmspt` library and CLI. It is a manual diagnostic tool for comparing S2 (two-point correlation) computation precision and performance across three methods: CPU exact (occupancy grid + FFT), CPU Monte Carlo (occupancy grid sampling), and GPU Monte Carlo (continuous ray-casting, only when built with the `gpu` feature). It is not exercised by the test suite or by any library pipeline.

#### main

- **Signature:** `fn main()`
- **Source:** `src/bin/precision_test.rs:6`
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

`pub fn empty() -> Self` — `src/types.rs:128`. Constructs a `Mesh` with empty `vertices` and `faces` vectors. No side effects.

#### is_empty

`pub fn is_empty(&self) -> bool` — `src/types.rs:136`. Returns `true` if the mesh has no vertices **or** no faces (i.e. `vertices.is_empty() || faces.is_empty()`, not a strict "both empty" check). No side effects.

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
| `max_storage_buffer_binding_size` | `u64` | Per-binding byte limit (0 for CPU); independent of the total task budget. |

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
- **Source:** `src/compute/policy.rs:22`
- **Purpose:** Resolves a requested `AccelerationMode` (`Cpu`/`Gpu`/`Auto`) plus workload/config parameters into a concrete `BackendSelection`, applying GPU availability, memory-limit, and workload-size checks.
- **Parameters:**
  - `requested` — the mode the caller/config asked for.
  - `gpu_min_voxels` — for `Auto` mode only: minimum workload size (in voxels) below which GPU is not attempted; defaults to `250_000` if `None`.
  - `gpu_memory_limit_mb` — legacy budget argument. Without a working-set estimate, a GPU request with any budget returns CPU with an explicit reason before probing; overflowing MiB conversion gets its own reason. Use `resolve_execution` with a validated estimate to enforce budgets.
  - `workload_voxels` — the current workload's voxel count, compared against `gpu_min_voxels` in `Auto` mode.
- **Returns:** A `BackendSelection`:
  - `requested == Cpu`: always `{ backend: Cpu, fallback: None }`.
  - `requested == Gpu`: reject an unvalidated budget as above; otherwise probe GPU availability and return GPU on success, or CPU with the initialization/disabled-feature reason.
  - `requested == Auto`: first compares `workload_voxels` against `gpu_min_voxels.unwrap_or(250_000)`; if below threshold, immediately returns CPU with a fallback reason citing the voxel counts, without attempting GPU init at all. Otherwise, follows the same GPU-init/memory-limit logic as the `Gpu` arm (with `requested: Auto` in any resulting `FallbackReason`).
- **Side effects:** When the `gpu` feature is enabled and `requested` is `Gpu` or `Auto` with a workload at or above the voxel threshold, calls `crate::gpu::try_init_gpu()`, which may initialize a wgpu adapter — documented elsewhere as a potentially heavy first call (adapter/device enumeration and creation).
- **Notes:** CPU and small Auto workloads return before budget checks or probing. Total task budgets and individual device buffer/binding limits are separate constraints. Runtime planners validate concrete buffer sizes; `resolve_execution` validates task bytes and strict fallback.

#### select_backend_for_workload

- **Signature:** `pub fn select_backend_for_workload(requested: AccelerationMode, gpu_min_workload: Option<usize>, gpu_memory_limit_mb: Option<u64>, workload: usize, workload_unit: &str) -> BackendSelection`
- **Purpose:** Generalizes backend selection beyond voxels. `Auto` checks the supplied threshold before probing; fallback text uses `workload_unit` (render passes `pixels`). `select_backend` remains the voxel wrapper.

`RenderedImage` stores `width`, `height`, and top-row-first RGBA8 bytes. `new` constructs from bytes and `filled` allocates a solid image. `RustMsptError::Image` wraps image-encoder failures. `Commands` now includes `Render`, whose CLI arm loads `render_config.yaml` and applies input/output overrides.

### configured_mode / resolve_execution

`configured_mode(&AccelerationConfig) -> Result<AccelerationMode>` reads the strict cpu/gpu/auto environment override once without GPU probing. `resolve_execution(config, requested, workload, threshold, supports_gpu, estimated_gpu_bytes) -> Result<BackendSelection>` first accepts CPU/small auto, then checks method support, implemented GPU options and the estimated task bytes against a checked MB budget before probing. Unsupported options are explicit config errors; forbidden GPU fallback returns an error. The estimate concerns allocated resources, not measured driver VRAM. Measure uses this method-aware policy; legacy selectors remain until other pipelines migrate.

Fresh resident GPU exact evaluations use `ExactMemoryPlan` for both backend selection and execution. With triangle storage T=max(36*faces,4), occupancy M=4*cells, and B partial slots, the conservative logical peak is 2T+M+128+C+80B (C = `exact_cert_bytes(cells)`, the voxel certification list, staging and parameter tail). This includes pending triangle/offset uploads and simultaneous old/new batch buffers; the 128-byte allowance covers fixed parameter/count/placeholder resources. B is reduced from 200,000 to fit an optional MiB budget, with a minimum of one. If even that does not fit, execution rejects before GPU initialization and the caller applies its fallback policy. The model excludes driver/compiler internals and CPU memory, applies to a fresh production direct-shell evaluation, and does not claim to budget experimental tiled/reduced or arbitrary retained pipelines. Existing hard exact-grid limits remain independent.

| Symbol | Source | Contract |
|---|---|---|
| `ExactMemoryPlan` | `src/compute/exact_memory.rs:5` | Fresh resident exact logical GPU peak and budget-selected partial batch. |
| `ExactMemoryPlan::new` | `src/compute/exact_memory.rs:12` | Checked resource arithmetic and batch selection; may still require check_budget for infeasible minima. |
| `ExactMemoryPlan::check_budget` | `src/compute/exact_memory.rs:52` | Enforce configured MiB cap before initialization. |
