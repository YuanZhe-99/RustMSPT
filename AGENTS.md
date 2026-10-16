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

**Mesh generation is not on this branch.** The tetrahedral mesher (`mesh`), its verifier (`mesh-verify`), the `SPEC_meshgen_*` specs and `PLAN_mesh_generation.md` live on the `meshgen` branch (worktree `../RustMSPT-meshgen`), which merges `main` in regularly. What both branches share stays on `main` and is changed here first: `io/vtu.rs`, `geometry/render_scene.rs`, the `mesh-render` pipeline and the fixtures under `data/fixtures/vtu/`. Keep those files free of meshgen-only code so the merge into `meshgen` stays clean.

`PLAN.md` tracks implementation progress for portable GPU acceleration and target-aware packing. Feature-gated `wgpu` paths coexist with CPU `rayon`/`rustfft`/`parry3d-f64` reference implementations and CPU fallback.

The codebase has a full documentation tree under `docs/en-us/` (conceptual algorithm docs, per-function reference docs, and captured-output pipeline walkthroughs). See [Documentation Map](#2-documentation-map) below for how it's organized and, critically, the order in which it must be consulted.

## 2. Documentation Map

Documentation lives in three tiers, each answering a different question:

| Tier | Location | Answers |
|---|---|---|
| Conceptual / design | `docs/en-us/algorithms/*.md` (9 files) | *Why* does this algorithm exist and *how* does it work, end to end? |
| Function-level contract | `docs/en-us/reference/*.md` (15 files + `function-index.md`) | What exactly does *this function* take, return, and mutate? |
| Implementation | `src/*.rs`, with inline `// AI-FUNC-SUMMARY` comments above almost every function | What does the code actually do, line by line? |

**Master lookup table:** [`docs/en-us/reference/function-index.md`](docs/en-us/reference/function-index.md) lists every documented function, struct, enum, and constant (688 rows) with its source location and a one-line summary, compiled from the `## Index` table at the top of each reference doc. If you know a function's name but not which file documents it, search this file first.

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
| Mesh rendering / STL-to-image | `algorithms/stl-rendering.md` | `reference/geometry-core.md` (render section), `reference/gpu.md`, `reference/pipeline-core.md` |
| VTU I/O / volume-mesh rendering (`mesh-render`) | — | `reference/mesh-render-and-vtu.md` |
| State-map point location (`state-map`: where a remesh's sampling points sit in the last frame's tets; BVH, rayon, GPU candidate pass) | `algorithms/state-map-location.md` | `reference/pipeline-state-map.md` |
| Seeded, recorded, void-aware placement (`pack` with a `placement:` block) | `algorithms/void-aware-placement.md` | `reference/pipeline-placement.md`, `reference/config.md` (`placement.rs`) |
| Placement recovery, completed extension and bounded geometry caching | `algorithms/placement-checkpoints-and-memory.md` | `reference/pipeline-placement-state.md` |
| Build identity (`version`, `--version`, the `tool` block in placement outputs) | — | `reference/core-and-compute.md` |
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

GPU correctness invariants: voxel parameter bytes 32/36/40 hold `RAY_DIR_GPU`; subtract bbox origins in f64 before f32 triangle upload. Shell offsets are processed in bounded batches without truncation, reject out-of-domain shifts before unsigned subtraction, and retain equal per-offset weighting across batches. Regression coverage: `tests/gpu_s2_boundaries_tests.rs` and GPU module layout/normalization tests. MC and voxel rays use a 64-hit fast path and recover overflow via constant-storage successive nearest-hit scans, preserving anchored GPU deduplication without dropping samples. Recovery can cost O(triangles × distinct hits). MC upload/evaluation return Result with device-capacity checks and checked readback; optimize either disables the failed GPU and preserves CPU mesh-MC semantics or propagates the error when fallback is forbidden. Voxel/shell also return Result with checked two-dimensional dispatch. Measure resolves methods separately, retains same-method fallback under its worker pool, and reports actual per-method backends. Remaining open performance items are listed under "Performance plan (closed 2026-09-25)" below.

S2(r) is the probability that two points a distance `r` apart both land in solid particle material; at `r=0` it equals volume fraction, decaying toward VF² as `r` grows for a random medium. Every CPU method reduces to a ray-casting point-in-mesh test (Möller–Trumbore, fixed non-axis-aligned ray direction, hit-distance deduplication within `1e-8`), and `calculate_s2` dispatches between three strategies — direct mesh Monte Carlo, voxelized Monte Carlo, and exact FFT-autocorrelation or direct shell-pair enumeration (chosen by a 24,000,000-cell padded-grid-size threshold). GPU counterparts mirror all three via wgpu compute kernels and fall back to the CPU path on GPU init failure. It powers `measure` (S2 reported as a diagnostic, exact-vs-Monte-Carlo comparison via `l2_norm`) and `optimize` (S2 L2 distance to a target curve as the simulated-annealing loss function). [Full doc](docs/en-us/algorithms/s2-two-point-correlation.md)

### Simulated Annealing and the Island Model

`OptimizePipeline` rearranges a loaded particle assembly's positions/orientations so its S2 curve matches a target curve, using simulated annealing: random perturbations (60% local translate/rotate, 30% move-toward-neighbor, 10% full random reposition) are accepted via the Metropolis criterion, with adaptive temperature nudging to keep the acceptance rate inside a `[0.20, 0.45]` target band. Before SA starts, `selective_prune_to_target_vf` cheaply thins the particle set toward the target volume fraction so SA doesn't spend its budget removing particles one accept/reject decision at a time. When `optimization.islands > 1`, independent SA searches run in bounded batches within one shared Rayon pool, each keeping its own temperature/cooling state, periodically migrating coherent geometry/loss/S2 snapshots through `Arc<Mutex<GlobalBest>>`. A run-wide `OptimizeS2` preserves the selected S2 definition across all stages. It powers the `optimize` pipeline end to end (`run_sa_island`, `selective_prune_to_target_vf`, `OptimizePipeline::run`). [Full doc](docs/en-us/algorithms/simulated-annealing-island-model.md)

### FFD Forging

Forging in RustMSPT is a closed-form kinematic approximation of axial-compression forging — no material model, plasticity, stress/strain field, or FEA — implemented as a per-vertex affine scaling around a chosen center point, with an extra radial-scaling pass for "void" meshes. `simulate_forging_ffd_with_tracking` generalizes this with a configurable compression axis, a deformation center taken from a caller-supplied lattice bbox (not the mesh's own bbox), void densification (extra centroid-ward pull modeling pore collapse), and optional region-of-interest bounding-box tracking through the same transform. `bulge_factor` interpolates the lateral scale between "no bulge" (height only shrinks) and "volume-conserving bulge." It powers the `forge` pipeline (`ForgePipeline::run`), which also reports volume fraction inside the ROI before/after compression. [Full doc](docs/en-us/algorithms/ffd-forging.md)

### Mesh Rendering (CPU Ray Casting and GPU Offscreen Rasterization)

The `render` pipeline turns an STL mesh into an RGBA image from a configured viewpoint. The camera is defined by a focus point (the image-center target) and a view direction (from the camera toward the focus), with an optional up vector (default `[0,0,1]`, automatic fallback when nearly parallel to the view direction) and auto-fitted framing from the mesh bounding box: orthographic mode fits the bbox extents in the camera plane (aspect-corrected), perspective mode places the camera so the bbox bounding sphere fits the vertical FOV. The CPU path (`render_mesh_cpu`) casts one ray per pixel through parry3d's QBVH-backed `TriMesh::cast_local_ray_and_get_normal` (nearest hit, two-sided Lambert headlight shading), parallelized over rows with rayon; the feature-gated GPU path (`GpuRenderPipeline`) rasterizes expanded triangles to an offscreen `Rgba8Unorm` texture with a `Depth32Float` buffer — no window or surface — and reads pixels back through a 256-byte-aligned staging buffer. Both paths share `RenderCamera`/`RenderSettings` so their outputs match within ~1 LSB per channel and ~1px at triangle edges; GPU failures fall back to CPU. [Full doc](docs/en-us/algorithms/stl-rendering.md)

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

Available subcommands: `split-filter`, `pack`, `optimize`, `measure`, `forge`, `scale`, `crop`, `render`, `mesh-render`, `state-map`, `version`

`pack` selects an engine from the config it is given: a top-level `placement:` block runs the seeded, recorded, void-aware engine; a `packing:` block runs the original one, unchanged. Exactly one must be present. `--seed` and `--threads` apply to the placement engine only, and passing either with a `packing:` config is an error rather than a silent no-op -- ignoring a determinism flag would report a run as reproducible when it is not.

`rustmspt --version` and `rustmspt version [--json]` report the same build identity: package version, git commit, whether the worktree was dirty when `build.rs` last ran, the enabled cargo features, and the build target/host/profile. Undetermined values are `null`, never a fabricated default -- a checkout without git still builds and still answers.

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
./target/release/rustmspt render --config data/input/render_config.yaml
```

After running, verify key outputs:
- `data/output/s2_history.txt` must contain entries: `Pruning Start:`, `Pruning Round`, `Pruning Completed:`, `Post-Pruning S2:`, `Final Best S2:`
- `data/output/measured_s2.txt` must have non-zero exact S2 values
- `data/output/rendered.png` must be a valid PNG with visible shaded particles on the background

## 6. File Structure

```
src/
  main.rs              CLI entry point (clap derive)
  lib.rs               Crate root, re-exports
  types.rs             Core types: Vec3, Mesh, Triangle, BoundingBox, Volume3D
  error.rs             Error type (thiserror)
  version.rs           BuildIdentity: the single source for the version subcommand, --version, and the identity stamped into placement outputs

  config/              YAML config deserialization
    mod.rs             Re-exports all config structs
    acceleration.rs    AccelerationConfig (mode, gpu_min_voxels, gpu_memory_limit_mb, etc.)
    deserialize.rs     Helper deserializers for flexible usize/i32 parsing
    crop.rs            CropConfig, CropInput, CropOutput, CropRawParams
    forging.rs         ForgingConfig, ForgingParams
    measurement.rs     MeasurementConfig, MeasurementParams
    optimization.rs    OptimizationConfig, OptimizationParams, TargetConfig
    packing.rs         PackingConfig, PackingParams, PackingFilters (the original engine)
    placement.rs       The placement: block, its validation and path resolution, and the two-pass probe that decides which pack engine a config selects
    render.rs          RenderConfig and camera/image parameters
    mesh_render.rs     MeshRenderConfig: views, filters, coloring, opacities for mesh-render
    scale.rs           ScaleConfig, ScalingParams
    split_filter.rs    SplitFilterConfig, SplitFilterOutput, SplitFilterRules
    state_map.rs       StateMapConfig, StateMapParams for state-map

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
    render.rs           GpuRenderPipeline: offscreen triangle rasterization and RGBA readback
    tet_candidates.rs   GpuTetCandidatePipeline: state-map's BVH walk proposing candidate tets per point (outward-rounded f32, a superset)
    shaders/
      s2_monte_carlo.wgsl  WGSL compute shader for MC S2 (ray-casting point containment)
      voxelize.wgsl        WGSL compute shader for voxelization (ray-casting per voxel)
      s2_shell_pairs.wgsl  WGSL compute shader for direct shell pair counting
      volume_transform.wgsl WGSL compute shader for volume rotate-and-crop transform
      render.wgsl           WGSL vertex/fragment shader for STL rendering
      tet_candidates.wgsl   WGSL compute shader for state-map's candidate tets

  geometry/            Geometry and computation kernels
    mod.rs             Re-exports public API
    bbox.rs            Bounding box ops: mesh_bbox, bbox_overlaps, bbox_distance, check_boundary_constraints_mode
    collision.rs       Collision/distance: to_parry_trimesh, trimesh_contains_point, mesh_surfaces_intersect_prepared, mesh_solids_nested_prepared, mesh_collision_exact_prepared (= surfaces cross OR one solid nested), mesh_distance_exact_prepared, generate_periodic_ghosts
    forging.rs         FFD forging simulation: simulate_forging_ffd, simulate_forging_ffd_with_tracking
    mesh_ops.rs        Mesh utilities: split_mesh_into_granules, merge_meshes, mesh_centroid, rotate_mesh_around_center, move_mesh_to_target_center, scale_mesh, translate_mesh, wrap_mesh_centroid_to_box, box_mesh, mesh_surface_area, vec_norm
    metrics.rs         Unified closed-mesh metrics: volume, surface area, equivalent-volume diameter, sphericity, target-diameter scaling
    void_index.rs      VoidIndex: parity point-in-void over a hierarchy, box prefilter, surface distance, exact in-domain volume, area-weighted surface sampling, voxel overlap
    quaternion.rs      UnitQuat (scalar-first w,x,y,z), Shoemake uniform-on-SO(3) sampler, and transform_shell - the one definition of the placement transform
    render.rs          Shared camera model and CPU QBVH ray-cast renderer
    render_scene.rs    VTU -> RenderScene extraction: AND-composed filters, boundary-face extraction, colormaps
    scene_render.rs    CPU scene renderer: all-hits transparency compositing, line overlays, markers, named view presets
    s2.rs              S2 (two-point correlation): calculate_s2, approximate_s2, l2_norm, build_bbox_occupancy, FFT-based exact S2, Monte Carlo S2
    spatial.rs         SpatialGrid for O(k) neighbor queries in collision detection
    volume.rs          Volume ops: mesh_volume, mesh_signed_volume, clip_mesh_by_bbox, particle_volume_in_bbox, volume_fraction_in_bbox, volume_fraction_of_meshes_in_bbox, orient_components_to_positive_volume

  io/                  File I/O
    hash.rs            sha256_bytes / sha256_file: content identity for inputs and outputs
    image.rs           PNG output for validated RGBA render buffers
    mod.rs             Re-exports: load_stl, save_stl, load_folder_stls, load_tiff_or_folder_with_range, save_tiff_or_folder_with_ext, load_raw_folder, Volume3D, ByteOrder, RawFolderSpec, load_vtu, save_vtu, VtuDoc, sha256_bytes, sha256_file
    stl.rs             STL ASCII/binary load and binary save
    volume.rs          TIFF and RAW volume I/O
    vtu.rs             Contract VTU (VTK XML UnstructuredGrid) writer + subset reader

  transfer/            Carrying a solved state across a remesh (state-map)
    npy.rs             .npy read (v1-3, little-endian numeric) and write (f64, i64)
    locate.rs          TetMesh, TetBvh, the location rule, locate_cpu, locate_from_candidates

  pipeline/            Pipeline implementations (all implement Pipeline trait)
    mod.rs             Pipeline trait, create_progress_bar helper
    crop.rs            CropPipeline: CT volume PCA-alignment and crop
    forge.rs           ForgePipeline: FFD-based mesh deformation
    measure.rs         MeasurePipeline: S2 measurement of STL
    mesh_render.rs     MeshRenderPipeline: VTU volume-mesh multi-view renderer (mesh-render subcommand)
    state_map.rs       StateMapPipeline: locate sampling points in a tet mesh, write donors/distances/report (state-map subcommand)
    optimize.rs        OptimizePipeline: simulated annealing with bounded island batches and coherent migration
    optimize_execution.rs  Fixed S2 method/backend selection, one optional shared GPU MC instance, and bounded Rayon island scheduling
    pack.rs            PackPipeline: sequential particle placement with optional target diameter distribution and mean-sphericity steering
    pack_targets.rs    Packing target CSV parser, diameter-bin debt controller, sphericity scoring, and distribution summaries
    placement.rs       PlacementPipeline: the seeded, recorded, void-aware packing engine (placement: block)
    placement_feasibility.rs  RejectReason, PlacedParticle, and check_placement: every placement rule in one fixed, cheapest-first order
    placement_outputs.rs  Record and report types, the conventions block, the JSON and CSV writers
    placement_labels.rs   Three-phase voxel label field and the per-voxel particle id field, with a header carrying the spacing and origin Volume3D does not
    placement_library.rs  Shape library: shells in first-face order, closedness and orientation refusals, scale-invariant filters, per-shell geometry digest
    placement_sizes.rs    AS241 normal quantile, truncated-lognormal and histogram samplers, reporting classes, and the size-multiset planner
    render.rs          RenderPipeline: STL viewpoint rendering to PNG
    rng.rs             Seeded ChaCha12 stream (seeded_rng) and the single uniform primitive (u01) every sampler builds on
    rotation.rs        Shared: RotationMode, parse_rotation_mode, sample_rotation_axis (the legacy axis-and-angle sampler, not uniform on SO(3))
    scale.rs           ScalePipeline: unit conversion / factor scaling
    split_filter.rs    SplitFilterPipeline: connected-component split + geometric filtering (optional seed makes the lognormal rebalance reproducible)

tests/
  collision_tests.rs   What "two meshes collide" has to mean: surfaces crossing, one solid nested in another, and the two point-in-solid tests agreeing
  core_tests.rs        Unit tests for geometry kernels
  version_tests.rs     Build identity: version, commit, dirtiness, features, JSON shape
  placement_primitives_tests.rs  Seeded stream, uniform-on-SO(3) sampler, volume centroid, exact in-box volume
  placement_config_tests.rs      placement: parsing, every refusal, config-relative path resolution, engine selection
  placement_sizes_tests.rs       Normal quantile against published values, truncated-lognormal and histogram samplers, multiset planner, shape library
  placement_void_tests.rs        The void index against closed-form geometry, the three-part predicate, crossing modes, the frozen-copy digest check
  placement_pipeline_tests.rs    The engine end to end: determinism across thread counts, reconstruction from the record alone, every stop reason, gaps and clipped faces
  io_tests.rs          I/O roundtrip tests (STL, TIFF, RAW)
  pack_target_tests.rs Packing target CSV, diameter-bin controller, and sphericity scoring tests
  pipeline_smoke_tests.rs  Integration tests for the pre-render pipelines with synthetic data
  render_tests.rs      Camera, CPU/GPU rendering, PNG, and render-pipeline tests
  mesh_render_tests.rs VTU round-trip, scene extraction/filters, transparency compositing, mesh-render pipeline tests
  mesh_visual_regression_tests.rs  GA-5 committed CPU PNG baselines, fixture smoke renders, and CPU/GPU image tolerance checks

data/
  fixtures/vtu/        Contract VTUs (the mesh-render test inputs), their generator, and GA-5 CPU render baselines under render_baselines/cpu/
  fixtures/placement/  make_void.py: regenerates data/input/placement/void_spheres.stl (stdlib only, --check verifies the committed file)
  input/               Default YAML configs, sample inputs, and gu2019_fig7b_pore_distribution.csv
  output/              Pipeline outputs (gitignored)

docs/
  README.md            Top-level docs index; notes docs/zh-cn/ status
  TRANSLATION_GUIDE.md  EN->ZH terminology glossary and translation conventions
  en-us/
    README.md           English docs landing page
    reference/           Per-module function/struct reference (15 files) + function-index.md
    algorithms/           Conceptual algorithm docs (S2, SA/island model, FFD, packing targets, PCA crop, spatial grid, mesh clipping)
    examples/              One walkthrough per CLI subcommand, plus a dedicated packing-target-distribution walkthrough
  zh-cn/                Chinese mirror of en-us/, structurally identical (see TRANSLATION_GUIDE.md)

build.rs               Build script: stamps the git commit, worktree dirtiness, enabled cargo features and build platform into compile-time env vars for src/version.rs
PLAN.md                Portable GPU/CPU co-execution roadmap and implementation plan
PLAN_pack_v2.md        Seeded, recorded, void-aware placement design + implementation status (git-excluded via .git/info/exclude)
```

## 7. Architecture

### Pipeline Trait
All pipelines implement `src/pipeline/mod.rs::Pipeline` with a single `fn run(&self) -> Result<()>`. Each pipeline reads its config, performs computation, and writes output.

### Parallelism

- **PERF-01/02 optimize:** `OptimizeS2` fixes voxel_exact/voxel_mc/mesh_mc once for every stage. Only mesh_mc can use the current GPU MC shader; forbidden method/init fallback errors. `RUSTMSPT_ACCELERATION` overrides YAML for optimize, and CPU/small auto never probe GPU. Explicit GPU memory caps conservatively select CPU until working-set budgeting exists. `tests/optimize_execution_tests.rs` exercises CLI behavior; `cargo test --offline --lib pipeline::optimize` observes actual evaluator workers and migration, with a shared GPU test under `--features gpu`. Final S2 is re-evaluated after optional orientation; historical selection loss is logged separately.

- **PERF-02 placement:** `run_placement` creates a dedicated configured Rayon pool and installs all stages through `run_placement_in_pool`; reports read the active worker count. Records, STL, CSV and voxel outputs are compared at 1/2/8 workers in `placement_pipeline_tests`.
- **PERF-13 spatial queries:** preserve first-encounter neighbor order; use linear membership below 256 unique IDs and a query-local hash set thereafter, never set iteration. `cargo test --offline --lib geometry::spatial::tests` checks the original scan oracle; the ignored `query_benchmark` runs in release for five raw timing samples.
- **PERF-19 scene tests:** GPU fixture literals must initialize both wireframe counters; `SceneSegment` imports are GPU-gated. Image baselines are unchanged.
See [Algorithm Overview](#3-algorithm-overview) above — particularly [Simulated Annealing and the Island Model](#simulated-annealing-and-the-island-model) and [Spatial Grid Collision Detection](#spatial-grid-collision-detection) — and `docs/en-us/algorithms/simulated-annealing-island-model.md` / `docs/en-us/algorithms/spatial-grid-collision.md` for the full picture. Load-bearing facts not covered there:


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

- `ThreadPool` is not `Clone`. Optimize installs one run-wide pool and borrows it; never create per-island pools that exceed `cpu_max`.
- `parry3d::TriMesh::new()` can return `Err` for degenerate meshes; always handle with `Option`.
- `rustfft::Fft` is `Send + Sync` but not `Clone`. For parallel FFT, each thread creates its own `FftPlanner`.
- `Volume3D<T = i64>` stores voxels in `T: Voxel`. The typed loaders (`load_raw_folder_typed`, `load_tiff_or_folder_typed_with_range`) return an `AnyVolume` in the file's own type; the widening loaders still return `Volume3D<i64>` (8 bytes per voxel). Crop matches the `AnyVolume` once and runs `crop_typed::<T>`; new volume code should do the same rather than widen. `numeric_type` still names the file type and is what writers range-check against.
- `split_mesh_into_granules` returns an owned `Vec<Mesh>`; each granule is a connected component.
- SA optimization state (temperature, acceptance window) is per-island; do not share mutable SA state across threads.
- wgpu's `enumerate_adapters()` returns a `Vec`, not an iterator — do not call `.collect()` on it.
- `wgpu::Limits::max_storage_buffer_binding_size` is `u32`; cast to `u64` when comparing with `BackendCaps`.
- `state-map`'s GPU pass must stay a **superset** filter: boxes rounded outward (`round_down`/`round_up`), points to nearest, and every decision taken in f64 on the host. Moving the barycentric test into WGSL (f32) would make GPU and CPU answers differ; `gpu_location_equals_cpu` fails if they do. Its rule (nearest centroid among containing tets; else, within the bound, the lowest index among tets within `DISTANCE_TIE` = 1e-12 relative of the nearest distance) is simHelper's `transfer/state_mapper.py` and must change only together with it, with `METHOD_VERSION` bumped. The tie band is load-bearing: without it, points equally far from two tets (a shared edge or corner) were assigned by the last bit of each implementation's distance arithmetic, and 4 of 1,015,356 elements of a real mapping differed between the two.
- When `gpu` feature is disabled, `ComputeBackend::Gpu` variant does not exist — use `#[cfg(feature = "gpu")]` guards in match arms and `is_gpu()` instead of `matches!`.
- The `acceleration` config field uses `#[serde(default)]` so existing YAML configs without it continue to work. Tests constructing config structs manually must include `acceleration: Default::default()`.
- `Volume3D` uses z-major indexing: `idx = z * width * height + y * width + x`. GPU shaders must match this layout, not x-major.
- Equivalent-volume diameter and sphericity require a closed mesh with positive finite volume and surface area; malformed/open candidates are skipped when target controls are active.
- **The placement engine's path contract has two halves, and both are load-bearing.** Relative paths *in a config* resolve against that config file's directory; relative paths *on the command line* resolve against the working directory, as shell arguments do. `--input`/`--output` are made absolute before substitution so both hold at once. A placement config reached through the legacy working-directory default (`data/input/pack_config.yaml`, when `--config` is omitted) is refused outright rather than honoured.
- **`Volume3D` carries neither spacing nor origin** -- it is pure index space. Anything that writes voxels for a consumer must write a header beside them saying the voxel size, the world position of voxel (0,0,0), and the index formula, or the field cannot be put back where it came from. `placement_labels.rs` does; nothing else in the crate needs to yet.
- **In the voxel label field the void wins every voxel it claims, and a void voxel never carries a particle id.** Same rule as the record's `overlap_owner`, applied per voxel: the void is frozen, so a particle crossing into it does not take that volume out of the void phase. Naming a particle there would contradict the phase code beside it.
- **The void feasibility predicate is three tests, and the third is the one people leave out.** For closed, non-self-intersecting surfaces: (a) no particle vertex inside the void, (b) the surfaces do not intersect and are at least `gap` apart, (c) **no void vertex inside the particle**. Because (b) says the surfaces never cross, each closed solid is wholly inside or wholly outside the other, so (a) rules out particle-inside-void, (c) rules out void-inside-particle, and (b) gives the separation. Without (c) a particle large enough to swallow a whole pore passes every other test. A shell strictly inside another has *all* its vertices inside, so neither vertex test can miss its case. `near_box` bounds the cost: a `false` means far from the void *and* not nested in it, because a nested particle's box necessarily meets the shell's own leaves. **The same predicate governs every pair of solids the engine compares, and v0.2.0 applied it only to the void.** Between two particles there is no "which is the void" asymmetry, so (a) and (c) collapse into one symmetric `mesh_solids_nested_prepared` while (b) is `mesh_surfaces_intersect_prepared` plus the gap test. Leaving it out of the particle arm is how `pack` came to place 28 of 146 particles wholly inside another -- one three deep -- and report `target_reached` on a fraction counting 2.1 % of the domain twice. Found downstream, not by the suite: the shipped examples' 6-to-24 size range makes the nesting region ~500 um3 in a 1e6 um3 domain, so it takes a wide ratio to see. Pinned by `tests/collision_tests.rs` and `tests/placement_pipeline_tests.rs::a_particle_is_never_placed_inside_another`.
- **Point-in-void is ray parity, not a pseudo-normal test.** Parity is correct for a shell nested inside another (a pore inside a pore) and does not care which way faces wind; parry's `contains_local_point` needs `TriMeshFlags::ORIENTED` and gets nesting wrong. `VoidIndex::contains_point` uses the same fixed ray direction and `1e-8` hit tolerance as `s2::point_inside_mesh`, and a test asserts the two agree on thousands of points.
- **`intersection_test` answers a question about surfaces; `query::distance` reports the gap between them.** Neither sees one closed solid wholly inside another: the surfaces never cross, and a sphere of radius 1 centred inside one of radius 3 measures `1.96` apart, which reads as comfortable clearance to any gap test. `mesh_collision_exact_prepared` is therefore the surface test **or** `mesh_solids_nested_prepared`, and "collide" means the solids share space. Do not reach for `mesh_surfaces_intersect_prepared` directly unless the surface question is specifically what is wanted. **One vertex settles nesting** given non-intersecting surfaces -- a vertex, never the centroid, because a non-convex shell's centroid can sit outside its own solid in a concavity another particle may legitimately occupy.
- **A void whose shells disagree about orientation is refused, not repaired.** `mesh_volume` takes the absolute value of the whole sum, so a void of two pores with one inverted would report `|V1 - V2|` instead of `V1 + V2`, and a solid-basis volume fraction computed against it would be quietly wrong. An all-inward void is accepted and reported as inward.
- **Void overlap voxels are anchored to the domain origin, never to each particle's own box.** Two particles reaching into the same pore must agree about the same physical voxel, or their overlaps double-count where they meet.
- **The placement engine defers three expensive things behind cheap rejections, and it matters by more than an order of magnitude.** Writing them eagerly made a small run take 3.4 seconds; deferring them took the same run, placing the same 82 particles, to 0.21. (a) The parry `TriMesh` is built only when a neighbour survives the cheap tests, because building one means building a bounding-volume hierarchy. (b) The exact in-box volume runs only for a particle whose box actually straddles the domain; one wholly inside keeps its full volume by definition. (c) The exact pair distance is reached only after a centre-to-centre sphere test and a box test have both failed to separate the pair. A particle's full volume is `scale^3` times its source shell's, never a fresh tetrahedra sum.
- **`on_unattainable: stop` with the default `descending` order turns one impossible size into an empty result.** Largest goes first, so if the largest cannot fit, the run stops having placed nothing -- and `no_feasible_placement` then outranks `distribution_unattainable`, because a run with no particles has no distribution to describe. Pinned by `tests/placement_pipeline_tests.rs::on_unattainable_stop_ends_at_the_first_failure`.
- **A determinism test must not compare two runs in two different temporary directories.** The record carries each source's `resolved_path`, so the files differ on that alone and the test fails for a reason that has nothing to do with determinism. Run both in one directory with different output subdirectories instead.
- **The size multiset is drawn in full before any placement, and a failed size is never replaced.** That is what stops a run from quietly compensating for large particles that will not fit by adding small ones; the shortfall stays visible per class instead. The plan stops at whichever of the last two counts lands *closer* to the target volume rather than at the first count to exceed it, because the overshooting particle is the largest one drawn and therefore the one most likely to fail.
- **Do not reach for `split_filter.rs`'s `erf_approx` when a quantile is needed.** It is Abramowitz & Stegun 7.1.26, good to about `1.5e-7`, which visibly moves how much mass a lognormal's truncation bounds cut off. `placement_sizes.rs::inverse_normal_cdf` is Wichura AS241, below `1e-15`, and its coefficients are stored **highest degree first** to match the paper -- feeding them to a Horner loop that expects ascending order silently evaluates a different polynomial and returns plausible, wrong quantiles.
- **A shell ordinal says how a shape was found, not which shape it is.** Re-exporting a file with its faces permuted moves every ordinal silently, so `ShapeShell::shell_sha256` digests the shell's own geometry beside it. An open or inward-wound shell **refuses the run** rather than being skipped: volume, centroid and equivalent diameter are what the size distribution is built from, so dropping one would change what the run drew from without saying so. A shell that fails a *filter* is different -- aspect and sharpness ratios are scale-invariant, so it fails at every size and is recorded as rejected.
- **`box_mesh` emits inward-facing triangles.** A unit cube from it has signed volume **-1**, not +1. Real STL data is outward: every shell of `data/input/particles.stl` measures positive, and so does `icosphere_mesh`. `mesh_volume` takes the absolute value, which is why this has never mattered; anything orientation-sensitive (`mesh_volume_in_bbox_exact`, `shell_signed_volumes`, any future boolean) must flip a box fixture first. Pinned by `tests/placement_primitives_tests.rs::box_mesh_is_inward_oriented_and_the_other_fixtures_are_not`.
- **`mesh_volume` returns `total.abs()`** (`src/geometry/volume.rs:14`), so a multi-shell mesh whose shells disagree about orientation reports `|V1 - V2|` rather than `V1 + V2`. Never call it on a void made of several pores; use `shell_signed_volumes` and check the signs agree.
- **`mesh_centroid` is the mean of the vertices, not the centre of mass.** It therefore moves when a region is tessellated more finely. `rotate_mesh_around_center` and `move_mesh_to_target_center` both pivot on it. The placement record's `shell_centroid` and `translation` are `mesh_volume_centroid` instead, and the two visibly disagree on any unevenly tessellated mesh.
- **`triangulate_cap_from_segments` must keep its `BTreeSet`s.** They were `HashSet`s, whose `RandomState` is seeded per process *and* bumped per instance, so ring traversal order - and through it each ring's float centre, and through that the clipped volume's last bits - differed between two calls in the same process. Two known geometric limitations remain and are why the placement engine uses `mesh_volume_in_bbox_exact` instead: a ring that is not star-shaped about its own vertex mean is re-ordered into a different polygon, and a nested ring is emitted with the same winding, so a hole is filled rather than subtracted.
- **`nalgebra` stores quaternions `[i, j, k, w]`; the placement record publishes `[w, x, y, z]`.** Reading one as the other yields a plausible, wrong orientation rather than an error. `UnitQuat` is scalar-first, and a test builds the same numbers through `nalgebra` to prove the orders agree.
- **`save_stl` refuses an empty mesh** (`src/io/stl.rs:259`), so a run that placed nothing must not call it; it writes its report and exits zero instead.
- Packing diameter frequencies are count frequencies over successfully placed full components, not volume-weighted frequencies. Failed placement attempts must never update bin or sphericity state.
- Tests constructing `PackingParams` directly must include `target_diameter_distribution_csv`, `target_mean_sphericity`, and `mean_sphericity_tolerance`; tests constructing `SplitFilterConfig` must include `seed`. None of these config structs uses `#[serde(default)]` on those fields, so YAML omits them freely but a Rust struct literal cannot. The `placement:` structs are the exception on the YAML side too: every one of them sets `deny_unknown_fields`, so a misspelled key there is refused rather than defaulted.
- `RenderedImage.rgba` is top-row-first RGBA8 and must contain exactly `width * height * 4` bytes.
- wgpu texture readback rows must be padded to `COPY_BYTES_PER_ROW_ALIGNMENT` (256 bytes) and unpadded before PNG encoding.
- CPU/GPU render tests must use tolerance at triangle edges; parry3d uses f64 QBVH ray casting while wgpu rasterization uses f32.
- `io/vtu.rs` appended-raw arrays must be collected in EMISSION order (FieldData, Points, Cells, PointData, CellData) — appended offsets are assigned by a running index during tag emission; any order mismatch corrupts every offset after the first out-of-order array.
- The contract `cell_kind` cell array (0 tet / 1 face / 2 curve / 3 voxel) is distinct from VTK cell type codes (10/5/4/11); `SceneFilter::CellKind` filters on the semantic array, falling back to mapping VTK types only when the array is absent.
- `build_scene` frames the camera from the FULL document bbox, not the filtered subset — deliberate, so all views/filters of one input frame identically.
- `mesh-render` uses the CPU path as the transparency reference (exact all-hits compositing); the GPU preview path (PLAN GA-3c, done) is opaque-only and selected via `backend: gpu | auto`. Scene transparency needs all-hits enumeration (`RayIntersectionsVisitor` over `TriMesh::qbvh()`); `cast_local_ray_and_get_normal` returns only the nearest hit.
- GPU scene triangles use `LessEqual` depth comparison because Volume boundaries are uploaded before tagged Face cells; this preserves the CPU Face-over-Volume rule for coincident geometry. GA-5 CPU baselines allow 2% of pixels beyond 2 LSB; full-fixture GPU comparisons allow 5% because line-overlay rasterization differs at edges.
- GA-5 render baselines must only be regenerated deliberately with `RUSTMSPT_UPDATE_RENDER_BASELINES=1 cargo test --test mesh_visual_regression_tests`; normal test runs compare without rewriting committed PNGs. Human visual inspection remains required for multimodal signoff.
- `build_scene` sorts hash-map-backed boundary faces and wireframe edges before emission; keep this ordering deterministic because transparent PNG baselines are sensitive to equal-depth traversal order.
- **Never let a render fail silently.** `build_scene` used to test `wire_edges.len() < max_wireframe_edges` *before* adding each face's edges, on a list sorted by node key, so once full it amputated whole contiguous patches and said nothing. A-6b and A-7b hit it exactly (200,001 / 200,000 segments) and A-7b's picture was reported as a mesh defect that did not exist. The budget is now decided up front, over-budget frames are thinned by a uniform stride, and `RenderScene` reports `wireframe_edges_total`/`wireframe_edges_emitted` so `mesh-render` can warn.
- `separation_t` uses `-1` for "no pairing here"; `mesh-render`'s point-array coloring skips sentinel values when averaging a cell, so a fully unpaired face renders sentinel grey.
- `mesh-render`'s `color_by` accepts point arrays (`separation_t`, `sizing_h`), reducing to the per-cell mean; unlike cell arrays this also colours tagged face cells, which is what makes surface-stage snapshots (no tets) renderable. On a document with **no volume cells**, a named *cell* array also colours faces directly and `array_range` filters them (that is how `thin_role`/`band_region` render); documents with tets keep the old face-tag rule, so GA-5 baselines are unchanged.

## Reporting to the owner (standing rule, set 2026-08-16)

Every substantial reply ends with a status block covering four things, in this order. It is not
optional and not conditional on the work having gone well.

1. **What finished** — the step just completed, with the numbers that say so.
2. **What is next** — the specific next step, not a direction. "Diagnose a8's 0.263 of off-surface
   area" is a next step; "keep improving P3" is not.
3. **Where things stand** — and this field has two forms, depending on whether the turn ends the
   work.

   *If stopping:* say why. Context exhaustion, a measurement that has to run, a decision that is
   the owner's, work genuinely finished, or a blocker — say which. Never let stopping look like
   completion, and never let completion look like stopping.

   *If not stopping:* report the **state**, not a reason. What is committed, what is verified, what
   is in flight. Do not answer the stopping question at all — a turn that continues has no stopping
   reason, and inventing one produces filler. "Commit boundary" was the filler this rule generated
   on six consecutive turns: literally true (tree coherent, tests green, default path unchanged) but
   answering a question nobody had asked, while the same reply said "I haven't stopped". If the
   honest content of this field is "I finished a unit of work and am starting the next", write that
   or write nothing.
4. **What I need from you, if anything** — a decision between named options with their trade-offs,
   or explicitly *nothing, proceeding*.

The reason this is a rule: several times the owner has had to ask "what's next?" after a reply that
reported results and then trailed off. A result without a next step reads as finished when it is
not, and the owner cannot tell the difference between "I ran out of room", "I need you to choose",
and "this phase is done" unless it is stated.


- **Rayon** is the primary parallelism framework project-wide; thread pools are created per-pipeline via `ThreadPoolBuilder`.
- **Voxelization**: `build_bbox_occupancy` parallelizes over x-slabs with `par_chunks_mut`, using ray-casting `point_inside_mesh` for correct 3D solid containment (parry3d `TriMesh::contains_local_point()` is unreliable without pseudo normals).
- **Island model**: `run_island_batches` limits live island contexts to the shared Rayon worker count; nested S2/VF work uses the same pool. Surplus islands queue, and histories retain island order. GPU MC, when compatible, shares one mutex-protected instance; release its lock before parallel VF work. Migration must publish/adopt geometry, loss and S2 together and release its lock before nested work.

### Determinism

Seeded runs promise: same seed, same inputs, same binary, same placement - on any thread count. Four rules keep that promise, and breaking any one of them is silent.

- **One stream, one thread.** Every variate comes from a single `ChaCha12Rng` (`src/pipeline/rng.rs`), drawn only on the calling thread. The seed is expanded with SHA-256 rather than `SeedableRng::seed_from_u64`, whose expansion is not stable across `rand_core` majors.
- **A fixed consumption schedule.** Each attempt draws all of its variates up front, before any check runs. If a check could short-circuit before a draw, reordering the checks later would silently desynchronise the stream.
- **Speculative attempt batches rewind, never skip.** `try_place_one` draws a batch of proposals serially, evaluates them in parallel against the unchanged placed set, and on the first acceptance calls `set_word_pos` back to the end of that attempt, so later draws are discarded and redrawn. Batches grow only after 4 serial attempts per particle, to 8x the workers, and never on one worker. `a_dense_run_is_identical_on_one_two_and_eight_threads` fails if the rewind is removed.
- **One uniform primitive.** Everything is built from `u01`, not `rand`'s `gen_range`/`Uniform`, which are explicitly not value-stable across `rand` minor versions.
- **Parallel sections stay pure and order-free.** `par_iter().any(...)` and `.reduce(f64::INFINITY, f64::min)` are safe: the first is a boolean, the second returns an element of the input. A parallel `.sum()` over `f64` is **not** - its result depends on the reduction tree, hence on the thread count - so volume accumulates sequentially in acceptance order, and the report emits that same accumulator rather than re-summing. `find_any`/`position_any` are forbidden; use `find_first`. Counters are incremented in the sequential caller after a parallel section returns, never inside its closure.
- **No `HashMap`/`HashSet` iteration order may reach an output.** Rejection counters and any map in a record or report are fixed structs or `BTreeMap`. See the `triangulate_cap_from_segments` pitfall for what this looked like when it was violated.

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
| `rand_chacha` | ChaCha12 seeded RNG: the reproducible stream behind seeded runs (named because `StdRng`'s algorithm is not stable across `rand` versions) |
| `sha2` | SHA-256 content identity for config, inputs and outputs |
| `tiff` | TIFF image I/O |
| `image` | PNG encoding/decoding for rendered RGBA images |
| `clap` | CLI argument parsing |
| `csv` | Target pore-diameter distribution parsing |
| `serde` + `serde_yaml` | YAML config deserialization |
| `serde_json` | JSON serialization of the build identity and the placement record/report. New JSON goes through `serde_json`. |
| `indicatif` | Progress bars |
| `wgpu` | GPU compute abstraction (optional, feature `gpu`) |
| `pollster` | Async-to-sync bridge for wgpu init (optional, feature `gpu`) |
| `bytemuck` | Zero-cost POD casting for GPU buffer uploads (optional, feature `gpu`) |

Performance continuation: CPU S2 uses `geometry/mesh_query.rs` prepared bbox/BVH queries and reusable hit scratch. Mesh MC uses seeded radius/sample blocks internally; the seeded diagnostic API preserves worker-count reproducibility. `VoxelS2` owns reusable occupancy for measure exact/voxel MC. Placement labels use spatial candidate tiles with original ownership order. GPU voxel/shell calls are fallible with checked two-dimensional dispatch. Measured results: `git show 1349c46:PLAN.Performance.md`.

Legacy pack now caches accepted collider and periodic-image TriMesh data and uses an incremental grid above 31 colliders; the former full placed-clone scan description is historical. Optimize updates grid membership only on accepted moves and retains the original prepared particle on rejection. `SpatialQueryScratch` supports allocation reuse in tiled labels.

ForgePipeline uses ownership-consuming `forge_owned`; public borrowed FFD wrappers retain clone semantics. Scale/translate/FFD parallelize only sufficiently large independent vertex maps. Binary STL load is streaming; `load_stl_hashed` parses and hashes one stream for placement shape sources. Binary output uses a 64 KiB writer with explicit flush.

CPU exact FFT reuses per-thread owned FftWorkspace plans and arrays, retaining at most 16 MiB of arrays with padded axes <=4096 (plan storage additional). Small transforms are serial; larger transforms use bounded task grains. Consumers read complex-grid real counts directly. Exact-workset planning is complete (`plan_exact_cpu`).

Crop performance contract: `cpu_max` bounds the whole pipeline; `acceleration` and environment overrides enforce threshold, device budget and forbidden fallback. Wide integers never silently narrow on GPU. GPU transform returns Result and uses checked two-dimensional dispatch. CPU resampling uses slices when depth fills the pool, otherwise row-aligned tiles targeting 4096 voxels; owned edge trimming compacts the existing allocation. See tests/crop_execution_tests.rs.

STL render performance contract: the entire pipeline runs in cpu_max pool and uses configured_mode/resolve_execution. GPU working-set estimates include vertices, textures, padded staging and uniforms. Initialization/render scopes and checked map callbacks return errors; forbidden fallback never silently renders on CPU. See tests/render_execution_tests.rs and tests/render_tests.rs.

CPU scene rendering: PreparedScene borrows immutable RenderScene and retains QBVH/materials across views. Hit scratch is Rayon task-local and coincidence dedup compacts in place, preserving Face-over-Volume and transparency rules. Mesh-render consumes GPU output images instead of cloning; bounded batch streaming remains pending.

Mesh-render GPU streaming: render_views_to uploads once and reuses uniform/color/depth/staging across cameras; the CLI saves each owned frame before the next. Output errors propagate without CPU fallback. Existing render_views remains a collecting wrapper. Auto GPU failure after partial output rewrites all views on CPU; strict GPU failure leaves earlier saved views. Encoding/render overlap remains pending.

Split-filter performance contract: cpu_max covers the entire pipeline. Requested volume/aspect/area metrics are prepared in original component order, parallel above 31 components while preserving per-component reduction order. Cheap aspect/volume rejections skip area. Filtering and seeded rebalance remain serial; at most two STL writers share the pool and propagate errors in output-rank order.

Spatial query deduplication promotes to hash membership at the 256th unique candidate within a bucket, not after finishing it; first-encounter ordering is retained and single dense buckets avoid quadratic scans.

Crop PCA now uses fixed 65536-voxel blocks and ascending-index merges for all three passes, independent of worker count. Background scans faces only, counts edges/corners once and chooses the smallest value on tied mode counts. Centered-covariance formula remains unchanged; global minimum oriented bbox and near-degenerate canonical eigenspace behavior are not claimed.

GPU voxelization and shell-S2 constructors now use request_adapter_device, honoring the same case-sensitive name/index selector as volume transforms and rendering. Invalid selectors return errors rather than using a default device. Device/context caching is still pending; try_init_gpu currently probes and requests a device on every call.

GPU initialization lifetime: one OnceLock backend instance is reused with serialized adapter selection. Selected GL adapters use private instances to avoid shared EGL context races. Each fresh adapter creates one logical device; device/queue/compiled pipeline sharing remains pending. Failed selectors and device requests are not cached. Context concurrency/GL isolation regressions live in gpu/context.rs tests.

GPU MC output/staging allocation is lazy: four 4-byte placeholders grow to actual invocations, persist across evaluations and read back only the live prefix. release_output_capacity drops the retained output/staging peak without discarding geometry or pipeline. Measure MC estimates 16 bytes/invocation plus triangles and 576-byte params; explicit optimizer GPU budget support remains pending.

Voxelization and volume transforms reuse staging with occupancy/output capacity and read only the current valid prefix. release_grid_capacity (voxel) and release_output_capacity (transform) reset output/readback peak without discarding input or pipelines. Tests alternate dimensions, pitch and signed values and exercise growth/release/reuse.

GPU shell S2 retains lazy offset/output/staging capacity and maps only the current batch prefix; release_batch_capacity resets one-offset capacity without dropping occupancy or compiled state. Preserve the 200000-offset bound and equal per-offset weighting.

Standalone mesh-render accepts top-level cpu_max (default/-1 available, otherwise clamp 1..available); its complete run and CPU fallback share one Rayon pool. Explicit GPU remains strict opaque preview; CPU retains transparency.

Mesh-render now honors RUSTMSPT_ACCELERATION before input loading using the common mode parser; effective gpu remains strict, auto permits fallback, and invalid environment values are rejected.

Standalone mesh-render plans its logical GPU working set before initialization, including queue geometry uploads, reused targets and staging. gpu_memory_limit_mb applies to gpu/auto; gpu_min_pixels defaults to zero and applies only to auto. GPU scene device limits are checked before host expansion; constructor errors are scoped. Driver internals/host scene memory are outside the logical GPU budget.

Optimize GPU MC now honors explicit logical memory budgets: pre-probe startup estimate plus per-evaluation checks under the GPU mutex include retained capacity, growth overlap and queued uploads. It initializes with empty geometry to avoid duplicate first-stage upload; stage failures retain same-method fallback/strict behavior. Driver internals are not included.

Legacy volume clipping now moves intermediate meshes and skips planes that do not cut referenced face vertices (including tangency); fully contained particle VF uses original mesh_volume directly. Preserve the 1e-9 plane classification tolerance. These fast paths do not repair all legacy nonconvex cap limitations.

Optimize migration shares immutable Arc<GlobalBest> snapshots (geometry/loss/S2 together). The shared mutex protects only loss comparison and Arc exchange; retired payload destruction and mutable receiving-state preparation happen after unlocking. Preserve strict ties and coherent historical S2.

CPU STL/scene renderers use matched contiguous RGBA/depth pixel tasks with unchanged ray coordinates and compositing. Wide rows can split across workers; task-local scene hit storage is reused. Preserve row-reference image equality and benchmark the grain before performance claims.

GPU MC now reduces 256 samples per radius/workgroup to integer hit/valid partials, then merges in CPU u64. Keep logical RNG ids independent of padded lanes, and update MC_BLOCK_SAMPLES, WGSL workgroup/reduction width and memory planning together. The frozen per-sample test shader verifies exact fixed-seed counts.

GPU MC flattens two-dimensional workgroups into radius/sample blocks. Guard padded groups uniformly before barriers and writes, even with spare retained buffer capacity. Optimize startup checks logical u32 sample capacity rather than the obsolete one-dimensional dispatch ceiling; the device planner checks actual workgroup and buffer limits.

GPU shell valid counts use the product of three overlap lengths after out-of-domain rejection; checked host grid size bounds that product by u32. Keep complete per-offset ratios equally weighted when adding voxel tiling. The raw-count enumeration oracle covers thin grids and extreme signed shifts.

The experimental cooperative shell shader uses one 256-lane workgroup per offset and exact integer reduction. Pair it with offsets_per_workgroup=1; the production direct shader uses 256. Keep padded-group rejection uniform before barriers. Initial software-GPU benchmarks favor larger grids but regress small grids; production switching needs workload-threshold evidence.

Experimental shell voxel tiles emit integer hit/valid partials; sum all tiles for each offset in u64 before averaging ratios. Batch capacity is 200000 partial slots, not 200000 offsets when tiled. Params now occupy 24 bytes; direct shaders read the first 16. Production remains direct pending measured selection.

Optional experimental shell tile reduction merges complete per-offset integer counts on the device before readback. Its u32 sums are bounded by the checked full grid. Keep per-offset equal weighting on CPU; do not average tile ratios. Batch release shrinks reducer outputs while retaining its pipeline.

Production GPU shell filters unsupported offsets in stable bounded batches before upload; unsupported-only calls retain the VF/zero result without uploading occupancy. Use unsigned_abs for extreme signed displacements. Raw shader oracles disable the host filter so device boundary coverage is retained.

GPU exact shell now shares the voxel stage Device/Queue and reads its occupancy buffer directly, eliminating a second device request and occupancy re-upload inside exact evaluation. Stages execute sequentially with non-overlapping error scopes. Voxel VF now uses voxelize_count: device integer reduction with only four-byte readback; the full occupancy remains resident. Resident shell input is borrowed, leaving its host-upload buffer independent.

Voxel count-only execution grows occupancy without allocating a full-grid staging buffer. Full-readback mode must independently grow staging even if occupancy already has capacity. Test both mode transitions and release/recompute; the lazy counter sums binary cells with a checked u32 total.

GPU exact enumerates radius shells lazily and records interpolation support in that same pass; do not restore all-radius offset collection or the second support enumeration. Bounded shell batches may cross radii but preserve offset order. GPU exact now uses shell_offset_iter within each radius, retaining only range cursors plus the bounded batch; the public Vec API remains for random-access consumers. Cube-search complexity is unchanged.

Fresh production GPU exact uses compute::exact_memory::ExactMemoryPlan in both Measure selection and execution. Budget controls partial batch size and minimum infeasibility is rejected before initialization. The logical bound includes queued uploads and old/new batch growth, not driver internals or host memory; experimental tiled/reduced and retained arbitrary pipelines need separate accounting.

Legacy select_backend APIs lack a task estimate and explicitly refuse GPU budget requests before probing. Budgeted pipelines must use resolve_execution or their validated planner, then probe without a budget. Never compare a total memory cap to a device single-binding limit; check actual buffers separately.

Mesh-MC SA islands cache per-particle component volumes, update only the moved particle, restore rejected entries, rebuild on migration and refresh every 64 candidate evaluations. Sum cached scalars in merged component order rather than updating a running total. Never substitute this geometric VF for voxel methods; prune/final still use full reference evaluation.

Mesh-render GPU multi-view output uses a rendezvous PNG writer when workers > 1; at most two delivered host frames coexist. Join/drain before fallback; output errors never trigger CPU fallback. CPU views and single-worker/single-view output remain sequential. Evidence: PLAN.Performance.md@1349c46 §56.

CPU PreparedScene uses nearest-distance plus bounded coincidence enumeration for >=192 triangles with all clamped alpha == 1; original sorting/Face priority/overlay depth preserved. Smaller or transparent scenes retain all-hit compositing. Threshold evidence and unresolved runtime observations: PLAN.Performance.md@1349c46 §57.

RAW/TIFF folder loading uses at most two concurrent per-file decoders in the current Rayon pool, ordered Result consumption, and one-file batches with one worker. TIFF folder ranges include every page in each selected file; readers are never shared. Bound is files, not bytes; full output volume remains resident. Evidence: PLAN.Performance.md@1349c46 §59.

For RAW folder decoding, files below 512 KiB stay serial; larger slices use the two-file bound. Single-file batches never dispatch Rayon work. The warm-cache benchmark includes one-file TIFF controls and cannot attribute those timings to parallel decoding.

TIFF folder output borrows slices into at most two current-pool writers with fixed z names; batches join and return the first ordered error, leaving possible current-batch partial files but starting no later batch. Both single-file and folder TIFF output explicitly flush and report flush errors; no fsync durability guarantee. Evidence: PLAN.Performance.md@1349c46 §60.

Crop emits completed-stage wall timings for load/background/PCA/transform_and_backend/trim/encode_write/total_in_pool. Transform timing includes backend initialization/transfers/fallback and must not be labeled GPU kernel time; total_in_pool excludes CLI/pool creation. Encoding includes flush, not fsync. Evidence: PLAN.Performance.md@1349c46 §61.

Crop background counting uses 256 dense counters for 8-bit images and 65,536 counters for >=65,536-voxel 16-bit images, with checked sparse spill for out-of-range values. Smaller 16-bit and all 32-bit inputs retain HashMap counting; every path selects the smallest tied value. Dense allocation is at most 512 KiB on 64-bit and ends before PCA. Evidence: PLAN.Performance.md@1349c46 §62.

RAW folder assembly checks plane/byte/volume products and reserves the final voxel count once, after the first valid slice; allocation failure propagates. This prevents Vec growth copies but does not remove per-slice decode buffers or cap RSS. Evidence: PLAN.Performance.md@1349c46 §63.

2026-09-25 cloud batch (PLAN.Performance.md@1349c46 §66-§67): every pipeline prints `[Timing] <pipeline> stage=... seconds=...`, `workers=` and `peak_rss_bytes=` through `pipeline/timing.rs` (VmHWM, `unavailable` never fabricated); `scripts/perf_matrix.py` runs the 1/2/4/8-worker cold/warm matrix into a temp dir. GPU devices and compiled pipelines are process-wide per `RUSTMSPT_GPU_DEVICE` (`gpu::context::SharedGpuDevice`, failures never cached, device-lost evicts); `runtime::scoped` holds a process-wide error-scope lock because error scopes belong to the shared device; `main` calls `release_shared_gpu_devices()`. GPU MC batches radii by 128 with global sample ids (any r_max); SA uploads only changed triangle runs. GPU ray tests are f32-certified (`gpu/certify.rs`): uncertain queries are recomputed on CPU f64 and counted — never loosen the bounds to cut the recompute ratio. CPU exact S2 picks FFT/direct by `plan_exact_cpu` (cost model + 768 MiB working-set budget) with 2/3/5-smooth padding and integer-rounded FFT counts, so both kernels are bit-identical; measure no longer has the 1.5M-voxel cap. Crop PCA is one-pass exact row moments with a canonical degenerate basis and a sign convention: each non-degenerate column's diagonal entry positive (frame closest to the scan axes), largest component only below 1e-6, and a left-handed frame fixed by flipping the smallest-diagonal free column. The first cloud rule (largest component positive) turned the real CT 180 deg and reversed its slice axis; the diagonal rule restores the pre-2026-09-25 crop output byte for byte (PLAN.Performance.md@1349c46 §68). GPU crop tiles the output with a source halo under the budget instead of falling back. Legacy pack mode 3 stores periodic images lazily; placement labels stream in z-slabs; ASCII STL parses line by line; SA voxel methods use incremental `VoxelCoverage` counts (off switch `INCREMENTAL_VOXEL_OCCUPANCY`); placement parallel pair checks exist but default to serial (`PAIR_PARALLEL_MIN = usize::MAX`).

Gap questions ("is this pair closer than `gap`?") go through `geometry::mesh_closer_than_prepared`, never `mesh_distance_exact_prepared(...) < gap`: a dual-QBVH screen bounds the distance from triangle pairs (one pair clearly below the gap answers true, every pair beyond a margin answers false) and only the narrow band in between reaches the exact distance, so the answer is identical (`tests/collision_tests.rs::the_screened_gap_test_answers_exactly_what_the_distance_answers`) and placement ran 2.2-4.2x faster. Pass `solids_known_apart = true` only right after an overlap/nesting test on the same arguments. Do not use parry 0.19's composite `closest_points` with a margin: it panics when nothing is within the margin and does not prune by it. Under WSLg, run the GPU suite with `env -u DISPLAY -u WAYLAND_DISPLAY ... --no-fail-fast`: with a display set, the GL adapter goes through Xwayland and the two crop GPU CLI tests die with `XIO: fatal IO error 111` after ~3 minutes (environment, not a crop regression; PLAN.Performance.md@1349c46 §68).

Voxel MC S2 (`VoxelS2::calculate`, non-exact) draws from `voxel_mc_rng()` — an unseeded Xoshiro256++ (`rand` feature `small_rng`) through precomputed `Uniform`s — because ChaCha12 `thread_rng` with per-call `gen_range` cost 6-8x the occupancy lookup per sample; it was already unseeded, and nothing seeded may use it. Shell offsets are cached per `(r_max, pitch)`; radii run in parallel only from `VOXEL_MC_PARALLEL_MIN_SAMPLES` (65,536 total samples) and particle coverage queries only from `COVERAGE_PARALLEL_MIN_VOXELS` (1,024), both measured (PLAN.Performance.md@1349c46 §68). Below those, waking the pool cost more than the work. The shipped `data/input` configs are too small to measure scaling for most pipelines (runs of 3-90 ms), and `perf_matrix.py` regenerates an unseeded pack->optimize chain per invocation, so compare optimize only on a shared chain input.

`scripts/perf_matrix.py --large` is the scaling benchmark (the shipped inputs only measure start-up); pass `--chain DIR` to reuse a chain so optimize/measure compare on one input. Never compile or run other work while a matrix runs: two background builds made measure look slower on 8 workers than on 1 (PLAN.Performance.md@1349c46 §72). split-filter writes small outputs serially (`WRITE_PAIR_MIN_FACES`): a pool fork/join per pair of ~20 KB files cost more than the writes. GPU certification-list regrowth is budgeted: MC through `GpuS2Pipeline::set_memory_limit_mb` + `mc_regrowth_peak`, voxel through `set_regrowth_headroom`; callers that check a budget must also set it on the pipeline. STL vertex welding uses `WeldMap` (fast non-SipHash `WeldHasher`): the map is never iterated, so the hasher cannot change indices, and it halved binary STL load time (§73). Do not iterate a `WeldMap` into an output. The parallel CPU FFT gathers x-axis lines in bands of y-planes (`FFT_X_BAND_BYTES`, 8 MiB), never into a whole-grid transpose, which had doubled every parallel exact S2's peak memory (§76); `fft_working_set_bytes` charges one band. Certified GPU ray tests read per-triangle constants (`certify::triangle_constants`, 16 f32 per triangle at MC binding 5 / voxel binding 4): anything that changes the shaders' certified test must change both the WGSL and that function, and any triangle upload must update the constants buffer too (§77). `optimization.seed` / `packing.seed` make single-island optimize and legacy pack reproducible on any worker count (`stage_rng`, seeded S2 via `calculate_s2_seeded` / `VoxelS2::calculate_seeded` / `calculate_s2_gpu_seeded`); draw an evaluation seed only when seeded, so unseeded streams stay as they were, and compare optimize/pack A/B runs with a seed (§79). forge and scale run in a pool sized by `forging.cpu_max` / `scaling.cpu_max` through `pipeline::run_in_cpu_pool`; new CPU pipelines should use the same helper so every parallel section and fallback shares one worker budget (§79). GPU uploads go through `runtime::CountedWrite::write_counted` (never raw `write_buffer`) and mapped readbacks call `runtime::record_readback`, so the `[Timing] gpu` line stays complete; `readback_wait_seconds` is execution plus transfer, never kernel time. `classify_adapter` separates software rasterizers from hardware: never cite a software-adapter timing as GPU performance, and `hardware_gpu_acceptance` (ignored) must be run on real hardware (§79). GPU MC shrinks its radius batch to fit a budget (`mc_largest_batch`) rather than falling back; judge GPU feasibility by the one-radius peak (`mc_evaluation_peak_batched(..., 1)`). The u32 logical sample id is deliberate (it keys the shader RNG) (§79). Crop GPU tiling minimises total uploaded source voxels (`CropTilePlan::upload_voxels`; cubic tiles compete with the slab/row/run search): thin slabs of a rotated output re-upload most of the source, which once cost 72 GB for a 128 MiB volume. `transform_tile` checks only the uploaded block against buffer limits. Crop now keeps the input's own sample type end to end (`Volume3D<T>`, `crop_typed`): peak RSS for a 128 MiB u16 volume fell 616 -> 171 MB with byte-identical output (§79).

Mesh-render CPU prints `[mesh-render] cpu scene_qbvh_builds=<n> views=<n> max_live_images=<n>`: builds come from the process-wide `scene_qbvh_build_count()` difference, live images from `render_and_write_overlapped`'s return value (<=2). The CLI test pins 1 build and 2 live images for two views.

GPU scene preview under a budget renders in horizontal strips: `scene_strip_rows` picks the tallest strip that fits, `GpuSceneOptions::strip_rows` re-targets clip-space y per strip (f64 composition, depth untouched) with a per-strip viewport. Geometry is still uploaded whole; a budget below one row plus geometry is still refused.

CPU exact direct counts pairs on packed rows (`OccupancyBits` + `direct_pair_counts_bits`, shifted AND + popcount, 4-10x the bool scan); the bool scan `direct_pair_counts` is test-only and must stay as the oracle. Rows carry one extra zero word and zero bits past nz, so shifted reads need no range mask; a negative dz swaps the two rows. Cost constants were refit for it (0.115 ns/pair, efficiencies 0.3/0.16); refit again with `exact_cost_model_calibration` whenever either kernel changes.

clippy is **not** at zero: measured 2026-10-16 after mesh generation left `main`, `cargo clippy --release --all-targets` reports 31 warnings (46 with `--features gpu`), in placement, transfer, gpu, `geometry/render_scene.rs`, `io/vtu.rs`, `types.rs` and the render tests - all pre-existing, none from the split. Do not add to them; clear them when touching those files. `!(alpha <= 0.0)`-style NaN-inclusive tests are spelled `alpha > 0.0 || alpha.is_nan()`.

A `Volume3D { ... }` literal whose `data` does not fix the element type (e.g. `vec![]`) needs `let v: Volume3D = ...`: the `T = i64` default does not drive inference in expressions.

### Performance plan (closed 2026-09-25)

`PLAN.Performance.md` was completed and removed. References of the form `PLAN.Performance.md@1349c46 §NN` point to its last version: `git show 1349c46:PLAN.Performance.md`, which holds every measurement behind the thresholds and constants cited in code and docs. Still open, none doable without hardware or a new dependency: GPU kernel timestamp queries; the PERF-08/09 GPU performance acceptance; GPU direct shell counting vs CPU FFT at large radii; release benchmarks on a real integrated and a discrete GPU (`hardware_gpu_acceptance` is ready, `--ignored`); an optional real-valued or GPU FFT experiment. Owner decisions (2026-09-25), all "keep as is": measure exact has no wall-time cap (the 768 MiB budget and the planner's modeled time are the guard); the CPU mesh-render render/write overlap stays (no measured gain, identical output); mesh-render keeps `gpu_min_pixels: 0` so `auto` does not change which image small renders produce; the GPU PNG writer keeps its scoped thread rather than Rayon until equal blocking/fallback order is proven.

### Placement cancellation and aggregate maintenance

- Keep application experiments, external dataset paths, measured results, execution
  logs and delivery records in the owning application workspace. Repository docs
  describe reusable behavior, configuration and maintenance contracts.
- Placement cancellation uses atomic-only Unix signal handlers or `outputs.dir/STOP`.
  Finish an accepted batch or whole cluster, save geometry, then report interruption.
  Repeated signals must not abort saving. Progress and final reports are separate;
  default-enabled checkpoints preserve RNG, current attempt budgets and stage cursors.
  Save final checkpoints even after target success; explicit extend retains all old
  transforms. Refuse changed physical inputs/executables and reduced total budgets.
  Legacy packing is unchanged.
- Aggregates are disabled by default. Keep primary-only and explicit mixed modes,
  positive/auto variant counts and decreasing fallback member counts configurable.
  Template construction and target compaction remain deterministic. Preserve member
  sizes, real-mesh separation, flat transforms and whole-cluster commit boundaries.
- Internal template VF and global placement VF have separate denominators. Count
  real material only. Restore and validate the best feasible geometry after bounded
  target search or cancellation. Apply local rotations consistently to proxy bounds,
  collision geometry, exports and global*local particle reconstruction.
- Reserve proposal budget for mixed fallback stages and report fallback draws and
  original shortfalls. Refuse unsupported boundary/void modes. Read
  `docs/en-us/algorithms/aggregate-placement.md` and
  `docs/en-us/reference/pipeline-aggregates.md` before changes. Shipped templates
  live in `data/input/placement*_config.yaml`; update them when config types change.
- Cold placement particles share source geometry and keep transforms/bounds;
  exact world meshes live in a byte-accounted LRU. Simplified collision models must
  conservatively enclose the original solid. Preserve exact containment/gap tests.
  Stream particle STL output; do not recreate an all-particle merged mesh.
  Always test pore containment independently of the near-surface bbox filter.
- Format changed Rust files only; repository-wide formatting rewrites unrelated
  legacy sources. Run the relevant placement tests for behavioral changes.


- Mixed aggregate `exact_fallback` must refine against actual accepted member
  meshes, including both solid containment directions and pore exclusion.
  Empty proxy interiors are matrix space, not solid material. Initial assembly
  import is a new validated task; do not bypass exact checkpoint executable
  compatibility. Preserve source identity, inherited order/transforms and real
  material accounting; incoming and inherited gaps are distinct.

- Optional `free_space_guided` placement ranks real geometry but never treats
  AABB/proxy overlap, a centre score or circumsphere mismatch as solid occupancy.
  First version is individual-only with strict walls, checkpoints and forbidden
  pore crossing. Keep cell/memory caps, stable queue IDs, serial committed
  feedback and exact recoverable search state. Read
  `docs/en-us/algorithms/free-space-guided-placement.md` and state/config
  references first. Explicit remaining-plan import must conserve original
  pending/failed size multiplicities; never infer replacements or relax exact
  same-executable checkpoint compatibility.

## Aggregate contact-growth maintenance

`src/pipeline/placement_aggregate_contact.rs` owns deterministic contact growth, conservative swept translation/rotation, neighborhood transactions and serializable template progress. Read `docs/en-us/algorithms/aggregate-placement.md` and `reference/pipeline-aggregates.md` before changes. Keep aggregate packing opt-in, keep the FCC constructor selectable, preserve member identities/PSD and do not replace real solid gap checks with proxy acceptance. Validate with release-mode contact unit tests and placement aggregate/checkpoint integration tests; geometry-heavy debug tests can exceed their CLI polling deadlines.
