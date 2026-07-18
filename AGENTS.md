# AGENTS.md

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Documentation Map](#2-documentation-map)
   - [Mandatory reading order](#mandatory-reading-order)
3. [Algorithm Overview](#3-algorithm-overview)
   - [S2 Two-Point Correlation](#s2-two-point-correlation)
   - [Simulated Annealing and the Island Model](#simulated-annealing-and-the-island-model)
   - [FFD Forging](#ffd-forging)
   - [Packing Target Diameter Distribution](#packing-target-diameter-distribution)
   - [PCA Volume Alignment and Crop](#pca-volume-alignment-and-crop)
   - [Spatial Grid Collision Detection](#spatial-grid-collision-detection)
   - [Mesh Clipping and Volume-Fraction Accounting](#mesh-clipping-and-volume-fraction-accounting)
4. [Build & Run](#4-build--run)
5. [Testing](#5-testing)
6. [File Structure](#6-file-structure)
7. [Architecture](#7-architecture)
   - [Pipeline Trait](#pipeline-trait)
   - [Parallelism](#parallelism)
   - [Packing Targets](#packing-targets)
   - [GPU Acceleration Roadmap](#gpu-acceleration-roadmap)
   - [Key Dependencies](#key-dependencies)
8. [Code Conventions](#8-code-conventions)
9. [Function Reading Policy for AI Agents](#9-function-reading-policy-for-ai-agents)
10. [Common Pitfalls](#10-common-pitfalls)

## 1. Project Overview

RustMSPT (Rust Microstructure Processing Toolbox) is a standalone Rust toolkit for STL-based microstructure processing. It provides end-to-end pipelines for splitting, packing, optimizing, forging, scaling, measuring, and cropping microstructural geometries.

`PLAN.md` tracks implementation progress for portable GPU acceleration and target-aware packing. Feature-gated `wgpu` paths coexist with CPU `rayon`/`rustfft`/`parry3d-f64` reference implementations and CPU fallback.

The codebase has a full documentation tree under `docs/en-us/` (conceptual algorithm docs, per-function reference docs, and captured-output pipeline walkthroughs). See [Documentation Map](#2-documentation-map) below for how it's organized and, critically, the order in which it must be consulted.

## 2. Documentation Map

Documentation lives in three tiers, each answering a different question:

| Tier | Location | Answers |
|---|---|---|
| Conceptual / design | `docs/en-us/algorithms/*.md` (7 files) | *Why* does this algorithm exist and *how* does it work, end to end? |
| Function-level contract | `docs/en-us/reference/*.md` (11 files + `function-index.md`) | What exactly does *this function* take, return, and mutate? |
| Implementation | `src/*.rs`, with inline `// AI-FUNC-SUMMARY` comments above almost every function | What does the code actually do, line by line? |

**Master lookup table:** [`docs/en-us/reference/function-index.md`](docs/en-us/reference/function-index.md) lists every documented function, struct, enum, and constant (246 rows) with its source location and a one-line summary, compiled from the `## Index` table at the top of each reference doc. If you know a function's name but not which file documents it, search this file first.

**Topic → doc mapping.** If you don't know the function name either, start from what you're trying to understand:

| Topic | Algorithm doc | Reference doc(s) |
|---|---|---|
| S2 / two-point correlation | `algorithms/s2-two-point-correlation.md` | `reference/geometry-analysis.md` |
| Simulated annealing / island model / `optimize` pipeline | `algorithms/simulated-annealing-island-model.md` | `reference/pipeline-optimize.md` |
| FFD forging / `forge` pipeline | `algorithms/ffd-forging.md` | `reference/geometry-volume-collision.md`, `reference/pipeline-core.md` |
| Packing target diameter distribution / sphericity steering | `algorithms/packing-target-diameter-distribution.md` | `reference/pipeline-packing.md` |
| PCA volume alignment / `crop` pipeline | `algorithms/pca-volume-alignment-crop.md` | `reference/pipeline-crop-and-splitfilter.md` |
| Spatial grid / collision detection / periodic ghosts | `algorithms/spatial-grid-collision.md` | `reference/geometry-core.md`, `reference/geometry-volume-collision.md` |
| Mesh clipping / volume-fraction accounting | `algorithms/mesh-clipping-volume-fraction.md` | `reference/geometry-volume-collision.md` |
| GPU compute pipelines (wgpu/WGSL) | (covered within each algorithm doc's own GPU section) | `reference/gpu.md` |
| Config / YAML deserialization | — | `reference/config.md` |
| STL / TIFF / RAW I/O | — | `reference/io.md` |
| CLI entry point, core types, compute backend selection | — | `reference/core-and-compute.md` |

Per-pipeline walkthroughs with real captured CLI output live in `docs/en-us/examples/` (one per subcommand, plus a dedicated packing-target-distribution walkthrough); the top-level index is [`docs/en-us/README.md`](docs/en-us/README.md).

### Mandatory reading order

This is a hard requirement, not a suggestion: **only descend to a deeper level when the current level doesn't answer the question.** Do not jump straight to reading source code when a conceptual or reference doc already covers it.

1. **Algorithm documentation** (`docs/en-us/algorithms/*.md`) — read this first, to understand the conceptual "why" and "how" before touching anything else.
2. **Function-level documentation** (`docs/en-us/reference/*.md`, using `function-index.md` to locate the right entry if needed, or the inline `AI-FUNC-SUMMARY` comment directly above a function in source) — read this second, to understand a specific function's contract: signature, purpose, parameters, returns, side effects.
3. **Specific function implementation** (the actual Rust source body) — read this only when steps 1 and 2 are insufficient: you are about to modify the function, you are debugging behavior the docs don't explain, or you need to verify an edge case the docs don't cover.

## 3. Algorithm Overview

One subsection per file in `docs/en-us/algorithms/`. These are high-level summaries only — read the linked full doc before implementing or debugging anything in the area.

### S2 Two-Point Correlation

S2(r) is the probability that two points a distance `r` apart both land in solid particle material; at `r=0` it equals volume fraction, decaying toward VF² as `r` grows for a random medium. Every CPU method reduces to a ray-casting point-in-mesh test (Möller–Trumbore, fixed non-axis-aligned ray direction, hit-distance deduplication within `1e-8`), and `calculate_s2` dispatches between three strategies — direct mesh Monte Carlo, voxelized Monte Carlo, and exact FFT-autocorrelation or direct shell-pair enumeration (chosen by a 24,000,000-cell padded-grid-size threshold). GPU counterparts mirror all three via wgpu compute kernels and fall back to the CPU path on GPU init failure. It powers `measure` (S2 reported as a diagnostic, exact-vs-Monte-Carlo comparison via `l2_norm`) and `optimize` (S2 L2 distance to a target curve as the simulated-annealing loss function). [Full doc](docs/en-us/algorithms/s2-two-point-correlation.md)

### Simulated Annealing and the Island Model

`OptimizePipeline` rearranges a loaded particle assembly's positions/orientations so its S2 curve matches a target curve, using simulated annealing: random perturbations (60% local translate/rotate, 30% move-toward-neighbor, 10% full random reposition) are accepted via the Metropolis criterion, with adaptive temperature nudging to keep the acceptance rate inside a `[0.20, 0.45]` target band. Before SA starts, `selective_prune_to_target_vf` cheaply thins the particle set toward the target volume fraction so SA doesn't spend its budget removing particles one accept/reject decision at a time. When `optimization.islands > 1`, multiple independent SA searches run in parallel threads, each keeping its own temperature/cooling state, periodically migrating the better of "mine vs. global best" through a shared `Arc<Mutex<GlobalBest>>`. It powers the `optimize` pipeline end to end (`run_sa_island`, `selective_prune_to_target_vf`, `OptimizePipeline::run`). [Full doc](docs/en-us/algorithms/simulated-annealing-island-model.md)

### FFD Forging

Forging in RustMSPT is a closed-form kinematic approximation of axial-compression forging — no material model, plasticity, stress/strain field, or FEA — implemented as a per-vertex affine scaling around a chosen center point, with an extra radial-scaling pass for "void" meshes. `simulate_forging_ffd_with_tracking` generalizes this with a configurable compression axis, a deformation center taken from a caller-supplied lattice bbox (not the mesh's own bbox), void densification (extra centroid-ward pull modeling pore collapse), and optional region-of-interest bounding-box tracking through the same transform. `bulge_factor` interpolates the lateral scale between "no bulge" (height only shrinks) and "volume-conserving bulge." It powers the `forge` pipeline (`ForgePipeline::run`), which also reports volume fraction inside the ROI before/after compression. [Full doc](docs/en-us/algorithms/ffd-forging.md)

### Packing Target Diameter Distribution

Standard sequential packing has no control over the size distribution of particles it places; this feature adds that control by steering accepted candidates toward under-represented bins of a user-supplied diameter histogram (e.g. `data/input/gu2019_fig7b_pore_distribution.csv`) while keeping target volume fraction as the primary objective. `TargetDistribution::choose_bin` picks the bin with the largest positive "debt" (`frequency × next_count − observed_count`), falling back to a total-variation-distance-minimizing choice when no bin is under-represented; candidates are rescaled to a bin's midpoint diameter via `scale_mesh_to_equivalent_diameter` and re-checked against geometry filters, with up to `TARGET_BIN_PROBES = 4` retries per bin before the algorithm falls back to whatever bin can currently accept a particle. An independent, optional soft control (`SphericityState`) steers the running mean sphericity toward a target band without ever rejecting a candidate solely for its sphericity. It powers the `pack` pipeline's target-steering path (`PackPipeline::run` + the `src/pipeline/pack_targets.rs` engine). [Full doc](docs/en-us/algorithms/packing-target-diameter-distribution.md)

### PCA Volume Alignment and Crop

Raw CT reconstructions are never aligned to the scan axes, so a naive axis-aligned crop either wastes volume or clips the sample. The `crop` pipeline fixes this by detecting background from the volume's outer boundary shell (`detect_background_mode`), running PCA on the foreground voxel point cloud's covariance matrix (`nalgebra::SymmetricEigen`, sorted eigenvectors forced into a right-handed frame) to find the sample's natural orientation, then inverse-mapping every output voxel back into the original volume to resample it (nearest or trilinear interpolation) into a new axis-aligned, tightly-cropped frame — on CPU via rayon `par_chunks_mut` over z-slices, or on GPU (feature-gated) above a 100,000-voxel output threshold, falling back to CPU on GPU failure. A final heuristic pass trims stray reconstruction-artifact voxels off the XY border. It powers the `crop` pipeline (`CropPipeline::run`, `estimate_pca_bbox`, `rotate_and_crop`/`rotate_and_crop_gpu`). [Full doc](docs/en-us/algorithms/pca-volume-alignment-crop.md)

### Spatial Grid Collision Detection

Brute-force pairwise mesh collision checking is `O(N)` per candidate and `O(N²)` per full pass, dominated by expensive `parry3d` triangle-mesh queries. `SpatialGrid` (`src/geometry/spatial.rs`) is a uniform bucket grid sized to the largest particle's bounding-box extent, giving `O(k)` neighbor queries instead; exact resolution then layers an AABB reject before falling back to `parry3d`-backed intersection/distance tests, and `generate_periodic_ghosts` handles boundary-mode-3 wraparound by checking a candidate against up to 26 shifted copies of neighboring particles. The two consumers differ: `optimize`'s `run_sa_island` builds and repeatedly rebuilds a `SpatialGrid` (its intended use case — many queries against a population whose positions keep changing), while `pack`'s `PackPipeline::run` deliberately skips the grid and instead does a rayon-parallelized brute-force scan over the `placed` list, since packing only ever checks each newly placed particle once. [Full doc](docs/en-us/algorithms/spatial-grid-collision.md)

### Mesh Clipping and Volume-Fraction Accounting

Volume-fraction accounting needs the volume of each particle mesh restricted to the packing box (not its total volume), since particles are allowed to straddle a domain boundary under modes 2/3 — and the clipped result must stay watertight for the divergence-theorem volume formula to be meaningful. `src/geometry/volume.rs` implements a Sutherland-Hodgman single-plane clipper that re-caps the hole it cuts (via cross-plane segment extraction and fan-triangulation of the resulting ring), a six-plane box clip built from that primitive, and quantized-key vertex deduplication (`(x * 1e6).round()`) to merge coincident cap vertices. This VF computation (`particle_volume_in_bbox`, `volume_fraction_of_meshes_in_bbox`) is the shared core metric that packing, optimization, and measurement pipelines all build on. [Full doc](docs/en-us/algorithms/mesh-clipping-volume-fraction.md)

## 4. Build & Run

```bash
export PATH="$HOME/.cargo/bin:$PATH"

# Debug build
cargo build

# Release build (recommended for real data)
cargo build --release

# Run a pipeline
cargo run --release -- <subcommand> [--config <path>] [--input <path>] [--output <path>]
./target/release/rustmspt <subcommand> --config data/input/<subcommand>_config.yaml
```

Available subcommands: `split-filter`, `pack`, `optimize`, `measure`, `forge`, `scale`, `crop`

## 5. Testing

### 1. Unit & Integration Tests
```bash
cargo test
cargo clippy
```

### 2. Release Build
```bash
cargo build --release
```

### 2b. GPU Feature Build & Test
```bash
cargo build --features gpu          # compile with wgpu backend
cargo test --features gpu           # run all tests including GPU adapter detection
cargo clippy --features gpu         # lint with GPU feature active
```

To use the real Intel/AMD/NVIDIA GPU (not software fallback), ensure the user has DRI access:
```bash
sudo usermod -aG render $USER       # then re-login
```

Environment variables for GPU selection:
- `RUSTMSPT_GPU_DEVICE=<name or index>` — filter adapter by name substring or index
- `RUSTMSPT_ACCELERATION=cpu|gpu|auto` — override config acceleration mode

### 3. Data Pipeline Validation (requires `data/input/` files)

Pipelines must be run in dependency order (pack depends on split-filter output; optimize depends on pack output; measure/forge/scale depend on optimize output):

```bash
./target/release/rustmspt split-filter --config data/input/split_filter_config.yaml
./target/release/rustmspt pack --config data/input/pack_config.yaml
./target/release/rustmspt optimize --config data/input/optimize_config.yaml
./target/release/rustmspt measure --config data/input/measure_config.yaml
./target/release/rustmspt forge --config data/input/forge_config.yaml
./target/release/rustmspt scale --config data/input/scale_config.yaml
./target/release/rustmspt crop --config data/input/crop_config.yaml
```

After running, verify key outputs:
- `data/output/s2_history.txt` must contain entries: `Pruning Start:`, `Pruning Round`, `Pruning Completed:`, `Post-Pruning S2:`, `Final Best S2:`
- `data/output/measured_s2.txt` must have non-zero exact S2 values

## 6. File Structure

```
src/
  main.rs              CLI entry point (clap derive)
  lib.rs               Crate root, re-exports
  types.rs             Core types: Vec3, Mesh, Triangle, BoundingBox, Volume3D
  error.rs             Error type (thiserror)

  config/              YAML config deserialization
    mod.rs             Re-exports all config structs
    acceleration.rs    AccelerationConfig (mode, gpu_min_voxels, gpu_memory_limit_mb, etc.)
    deserialize.rs     Helper deserializers for flexible usize/i32 parsing
    crop.rs            CropConfig, CropInput, CropOutput, CropRawParams
    forging.rs         ForgingConfig, ForgingParams
    measurement.rs     MeasurementConfig, MeasurementParams
    optimization.rs    OptimizationConfig, OptimizationParams, TargetConfig
    packing.rs         PackingConfig, PackingParams, PackingFilters
    scale.rs           ScaleConfig, ScalingParams
    split_filter.rs    SplitFilterConfig, SplitFilterOutput, SplitFilterRules

  compute/             Compute backend abstraction (always compiled)
    mod.rs             Re-exports, unit tests for backend selection
    backend.rs         AccelerationMode enum, ComputeBackend enum, BackendCaps struct
    policy.rs          select_backend(): auto/cpu/gpu dispatch with workload threshold and memory guard

  gpu/                 GPU implementation (feature-gated: cargo feature "gpu")
    mod.rs             Re-exports
    context.rs         GpuContext, try_init_gpu(): wgpu adapter/device init with env-var device filter
    s2.rs              GpuS2Pipeline: wgpu compute pipeline for Monte Carlo S2 (with update_mesh for SA reuse)
    s2_shell.rs        GpuShellS2Pipeline: wgpu compute pipeline for direct shell pair S2
    voxel.rs           GpuVoxelPipeline: wgpu compute pipeline for mesh voxelization
    volume_transform.rs GpuVolumeTransformPipeline: wgpu compute pipeline for volume rotate-and-crop
    shaders/
      s2_monte_carlo.wgsl  WGSL compute shader for MC S2 (ray-casting point containment)
      voxelize.wgsl        WGSL compute shader for voxelization (ray-casting per voxel)
      s2_shell_pairs.wgsl  WGSL compute shader for direct shell pair counting
      volume_transform.wgsl WGSL compute shader for volume rotate-and-crop transform

  geometry/            Geometry and computation kernels
    mod.rs             Re-exports public API
    bbox.rs            Bounding box ops: mesh_bbox, bbox_overlaps, bbox_distance, check_boundary_constraints_mode
    collision.rs       Collision/distance: to_parry_trimesh, mesh_collision_exact_prepared, mesh_distance_exact_prepared, generate_periodic_ghosts
    forging.rs         FFD forging simulation: simulate_forging_ffd, simulate_forging_ffd_with_tracking
    mesh_ops.rs        Mesh utilities: split_mesh_into_granules, merge_meshes, mesh_centroid, rotate_mesh_around_center, move_mesh_to_target_center, scale_mesh, translate_mesh, wrap_mesh_centroid_to_box, box_mesh, mesh_surface_area, vec_norm
    metrics.rs         Unified closed-mesh metrics: volume, surface area, equivalent-volume diameter, sphericity, target-diameter scaling
    s2.rs              S2 (two-point correlation): calculate_s2, approximate_s2, l2_norm, build_bbox_occupancy, FFT-based exact S2, Monte Carlo S2
    spatial.rs         SpatialGrid for O(k) neighbor queries in collision detection
    volume.rs          Volume ops: mesh_volume, mesh_signed_volume, clip_mesh_by_bbox, particle_volume_in_bbox, volume_fraction_in_bbox, volume_fraction_of_meshes_in_bbox, orient_components_to_positive_volume

  io/                  File I/O
    mod.rs             Re-exports: load_stl, save_stl, load_folder_stls, load_tiff_or_folder_with_range, save_tiff_or_folder_with_ext, load_raw_folder, Volume3D, ByteOrder, RawFolderSpec
    stl.rs             STL ASCII/binary load and binary save
    volume.rs          TIFF and RAW volume I/O

  pipeline/            Pipeline implementations (all implement Pipeline trait)
    mod.rs             Pipeline trait, create_progress_bar helper
    crop.rs            CropPipeline: CT volume PCA-alignment and crop
    forge.rs           ForgePipeline: FFD-based mesh deformation
    measure.rs         MeasurePipeline: S2 measurement of STL
    optimize.rs        OptimizePipeline: simulated annealing with island model
    pack.rs            PackPipeline: sequential particle placement with optional target diameter distribution and mean-sphericity steering
    pack_targets.rs    Packing target CSV parser, diameter-bin debt controller, sphericity scoring, and distribution summaries
    rotation.rs        Shared: RotationMode, parse_rotation_mode, sample_rotation_axis
    scale.rs           ScalePipeline: unit conversion / factor scaling
    split_filter.rs    SplitFilterPipeline: connected-component split + geometric filtering

tests/
  core_tests.rs        Unit tests for geometry kernels
  io_tests.rs          I/O roundtrip tests (STL, TIFF, RAW)
  pack_target_tests.rs Packing target CSV, diameter-bin controller, and sphericity scoring tests
  pipeline_smoke_tests.rs  Integration tests: all 7 pipelines with synthetic data

data/
  input/               Default YAML configs, sample inputs, and gu2019_fig7b_pore_distribution.csv
  output/              Pipeline outputs (gitignored)

docs/
  README.md            Top-level docs index; notes docs/zh-cn/ status
  TRANSLATION_GUIDE.md  EN->ZH terminology glossary and translation conventions
  en-us/
    README.md           English docs landing page
    reference/           Per-module function/struct reference (11 files) + function-index.md
    algorithms/           Conceptual algorithm docs (S2, SA/island model, FFD, packing targets, PCA crop, spatial grid, mesh clipping)
    examples/              One walkthrough per CLI subcommand, plus a dedicated packing-target-distribution walkthrough
  zh-cn/                Chinese mirror of en-us/, structurally identical (see TRANSLATION_GUIDE.md)

PLAN.md                Portable GPU/CPU co-execution roadmap and implementation plan
```

## 7. Architecture

### Pipeline Trait
All pipelines implement `src/pipeline/mod.rs::Pipeline` with a single `fn run(&self) -> Result<()>`. Each pipeline reads its config, performs computation, and writes output.

### Parallelism
See [Algorithm Overview](#3-algorithm-overview) above — particularly [Simulated Annealing and the Island Model](#simulated-annealing-and-the-island-model) and [Spatial Grid Collision Detection](#spatial-grid-collision-detection) — and `docs/en-us/algorithms/simulated-annealing-island-model.md` / `docs/en-us/algorithms/spatial-grid-collision.md` for the full picture. Load-bearing facts not covered there:

- **Rayon** is the primary parallelism framework project-wide; thread pools are created per-pipeline via `ThreadPoolBuilder`.
- **Voxelization**: `build_bbox_occupancy` parallelizes over x-slabs with `par_chunks_mut`, using ray-casting `point_inside_mesh` for correct 3D solid containment (parry3d `TriMesh::contains_local_point()` is unreliable without pseudo normals).
- **Island model**: when `optimization.islands > 1`, independent SA instances run in parallel via `std::thread::scope`, each with its own dedicated `ThreadPool` (`ThreadPool` is not `Clone` — see [Common Pitfalls](#10-common-pitfalls)).

### Packing Targets
See [Packing Target Diameter Distribution](#packing-target-diameter-distribution) above and `docs/en-us/algorithms/packing-target-diameter-distribution.md` for the full picture. Load-bearing facts not covered there:

- `packing.target_diameter_distribution_csv` and `packing.target_mean_sphericity` are both optional, best-effort/soft controls — target volume fraction always has priority, and neither ever blocks placement outright.
- Target-aware packing writes `<output_stem>_diameter_distribution.csv` beside the output STL, with per-bin target/actual frequencies, counts, errors, and attempt totals.
- Diameter and sphericity metrics always use the full closed mesh, while volume-fraction accounting (boundary modes 2/3) always uses the in-box clipped volume — these are computed independently and can diverge for particles that straddle the box boundary.

### GPU Acceleration Roadmap
See [Algorithm Overview](#3-algorithm-overview) above (each algorithm doc has its own GPU section) and `docs/en-us/reference/gpu.md` for the full picture; `PLAN.md` tracks implementation progress. Load-bearing facts not covered there:

- CUDA must not be used as the primary acceleration path — portability (a feature-gated `wgpu`/WGSL backend behind a compute abstraction, with CPU fallback and CPU reference tests) is a project requirement.
- The `AccelerationMode` enum (`auto`/`cpu`/`gpu`) and `select_backend()` in `src/compute/policy.rs` handle runtime dispatch; the `measure` and `optimize` pipelines report the effective backend used and any fallback reason.
- The config field `acceleration.mode` (default `auto`) is supported in `MeasurementParams` and `OptimizationParams`.

### Key Dependencies
| Crate | Purpose |
|-------|---------|
| `parry3d-f64` | Collision detection, BVH-accelerated point queries, distance computation |
| `nalgebra` | Linear algebra (Matrix3, Vector3, PCA via SymmetricEigen) |
| `rustfft` | FFT for exact S2 computation |
| `rayon` | Data parallelism |
| `rand` | Random number generation for SA and Monte Carlo |
| `tiff` | TIFF image I/O |
| `clap` | CLI argument parsing |
| `csv` | Target pore-diameter distribution parsing |
| `serde` + `serde_yaml` | YAML config deserialization |
| `indicatif` | Progress bars |
| `wgpu` | GPU compute abstraction (optional, feature `gpu`) |
| `pollster` | Async-to-sync bridge for wgpu init (optional, feature `gpu`) |
| `bytemuck` | Zero-cost POD casting for GPU buffer uploads (optional, feature `gpu`) |

## 8. Code Conventions

- No comments in code unless explicitly requested.
- **AI-FUNC-SUMMARY comments are the exception**: every function must have an `AI-FUNC-SUMMARY` comment (see [Function Reading Policy](#9-function-reading-policy-for-ai-agents) below).
- Program comments, configuration-file comments, and all `AI-FUNC-SUMMARY` text must be written in English.
- Follow existing patterns: use `crate::types::{Mesh, Vec3, BoundingBox}` for core types.
- Geometry functions live in `src/geometry/`, pipeline logic in `src/pipeline/`, config in `src/config/`.
- Public API is re-exported through `mod.rs` files in each module.
- Error handling uses `crate::error::{Result, RustMsptError}`.
- All new parallel code must use rayon and be safe for concurrent access (no data races on mutable state).
- Config fields use `serde` deserialization with optional `deserialize_with` for flexible numeric parsing.
- **Every code modification must update `AGENTS.md`** to reflect any changes to architecture, file structure, testing steps, conventions, or pitfalls.
- **If `PLAN.md` exists, every code modification must also update `PLAN.md`** to reflect current progress, decisions, and next steps.
- GPU-related changes must keep CPU fallback behavior intact and update `PLAN.md` with implementation progress, test coverage, and any changed architectural decisions.
- **Every code modification must also update the corresponding documentation under `docs/en-us/`** (and mirror the change into the corresponding `docs/zh-cn/` file per `docs/TRANSLATION_GUIDE.md`, or flag it there as pending translation if a full translation isn't feasible in the same change): update the function's entry in the matching `docs/en-us/reference/*.md` file (signature, purpose, params, returns, side effects, notes) whenever its behavior, signature, or `AI-FUNC-SUMMARY` changes; add a new entry and a `function-index.md` row for every new function; remove entries for deleted functions; update the relevant `docs/en-us/algorithms/*.md` file when algorithmic behavior changes; and update the relevant `docs/en-us/examples/*.md` file when CLI flags, config fields, or example output change.

## 9. Function Reading Policy for AI Agents

This section describes practice at **reading-order level 3** (specific function implementation) from the [Documentation Map](#2-documentation-map) above: it applies once you've descended past levels 1–2 because the algorithm docs and reference docs weren't enough, and you're now reading or navigating Rust source directly. Within source, this codebase uses `AI-FUNC-SUMMARY` comments as a lightweight semantic index for functions — the bridge between level 2 (`docs/en-us/reference/*.md`) and level 3 (the function body itself). These comments appear immediately above each function definition (or inside the existing docstring for Rust `///` style where already established) and follow one of two formats:

**Full format** (for non-trivial functions):
```
// AI-FUNC-SUMMARY:
// Purpose: <one short sentence describing what this function is responsible for>
// Inputs: <important parameters only; omit obvious ones if trivial>
// Returns: <what the caller receives, or "None">
// Side effects: <state changes, file/network/database/UI effects, logging, mutation, or "None">
// Notes: <important assumptions, edge cases, invariants, or when the function should be used; omit if unnecessary>
```

**Compact one-line format** (for very small/trivial functions):
```
// AI-FUNC-SUMMARY: <short purpose>; returns <result>; side effects: <none or brief description>.
```

When inspecting code:
1. Read the file-level structure, function names, signatures, and `AI-FUNC-SUMMARY` comments first.
2. Do not read entire function bodies unless the summary/signature is insufficient, the function is directly relevant to the requested change, or debugging requires implementation details.
3. Treat summaries as navigation aids, not as a substitute for source code when making behavior-changing edits.
4. When modifying a function, update its `AI-FUNC-SUMMARY` if the purpose, inputs, return value, side effects, assumptions, or error behavior changed.
5. When adding a new function, add an `AI-FUNC-SUMMARY` comment at the same time.
6. If a summary appears stale or inconsistent with the implementation, correct the summary before relying on it.
7. `AI-FUNC-SUMMARY` comments and `docs/en-us/reference/*.md` entries must stay in sync: when you update a function's `AI-FUNC-SUMMARY`, update its corresponding entry in the matching `docs/en-us/reference/*.md` file (and its `function-index.md` row) in the same change.

## 10. Common Pitfalls

- `ThreadPool` is not `Clone`. Use references or create separate pools per island.
- `parry3d::TriMesh::new()` can return `Err` for degenerate meshes; always handle with `Option`.
- `rustfft::Fft` is `Send + Sync` but not `Clone`. For parallel FFT, each thread creates its own `FftPlanner`.
- `Volume3D.data` stores voxel values as `i64`. The `numeric_type` field tracks original bit depth.
- `split_mesh_into_granules` returns an owned `Vec<Mesh>`; each granule is a connected component.
- SA optimization state (temperature, acceptance window) is per-island; do not share mutable SA state across threads.
- wgpu's `enumerate_adapters()` returns a `Vec`, not an iterator — do not call `.collect()` on it.
- `wgpu::Limits::max_storage_buffer_binding_size` is `u32`; cast to `u64` when comparing with `BackendCaps`.
- When `gpu` feature is disabled, `ComputeBackend::Gpu` variant does not exist — use `#[cfg(feature = "gpu")]` guards in match arms and `is_gpu()` instead of `matches!`.
- The `acceleration` config field uses `#[serde(default)]` so existing YAML configs without it continue to work. Tests constructing config structs manually must include `acceleration: Default::default()`.
- `Volume3D` uses z-major indexing: `idx = z * width * height + y * width + x`. GPU shaders must match this layout, not x-major.
- Equivalent-volume diameter and sphericity require a closed mesh with positive finite volume and surface area; malformed/open candidates are skipped when target controls are active.
- Packing diameter frequencies are count frequencies over successfully placed full components, not volume-weighted frequencies. Failed placement attempts must never update bin or sphericity state.
- Tests constructing `PackingParams` directly must include `target_diameter_distribution_csv`, `target_mean_sphericity`, and `mean_sphericity_tolerance`.
