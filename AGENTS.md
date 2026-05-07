# AGENTS.md

## Project Overview

RustMSPT (Rust Microstructure Processing Toolbox) is a standalone Rust toolkit for STL-based microstructure processing. It provides end-to-end pipelines for splitting, packing, optimizing, forging, scaling, measuring, and cropping microstructural geometries.

## Build & Run

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

## Testing

```bash
# All tests (unit + integration)
cargo test

# Clippy lint
cargo clippy

# Run actual data pipelines (requires data/input/ files)
cargo build --release
./target/release/rustmspt split-filter --config data/input/split_filter_config.yaml
./target/release/rustmspt pack --config data/input/pack_config.yaml
./target/release/rustmspt optimize --config data/input/optimize_config.yaml
./target/release/rustmspt measure --config data/input/measure_config.yaml
./target/release/rustmspt forge --config data/input/forge_config.yaml
./target/release/rustmspt scale --config data/input/scale_config.yaml
./target/release/rustmspt crop --config data/input/crop_config.yaml
```

## File Structure

```
src/
  main.rs              CLI entry point (clap derive)
  lib.rs               Crate root, re-exports
  types.rs             Core types: Vec3, Mesh, Triangle, BoundingBox, Volume3D
  error.rs             Error type (thiserror)

  config/              YAML config deserialization
    mod.rs             Re-exports all config structs
    deserialize.rs     Helper deserializers for flexible usize/i32 parsing
    crop.rs            CropConfig, CropInput, CropOutput, CropRawParams
    forging.rs         ForgingConfig, ForgingParams
    measurement.rs     MeasurementConfig, MeasurementParams
    optimization.rs    OptimizationConfig, OptimizationParams, TargetConfig
    packing.rs         PackingConfig, PackingParams, PackingFilters
    scale.rs           ScaleConfig, ScalingParams
    split_filter.rs    SplitFilterConfig, SplitFilterOutput, SplitFilterRules

  geometry/            Geometry and computation kernels
    mod.rs             Re-exports public API
    bbox.rs            Bounding box ops: mesh_bbox, bbox_overlaps, bbox_distance, check_boundary_constraints_mode
    collision.rs       Collision/distance: to_parry_trimesh, mesh_collision_exact_prepared, mesh_distance_exact_prepared, generate_periodic_ghosts
    forging.rs         FFD forging simulation: simulate_forging_ffd, simulate_forging_ffd_with_tracking
    mesh_ops.rs        Mesh utilities: split_mesh_into_granules, merge_meshes, mesh_centroid, rotate_mesh_around_center, move_mesh_to_target_center, scale_mesh, translate_mesh, wrap_mesh_centroid_to_box, box_mesh, mesh_surface_area, vec_norm
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
    pack.rs            PackPipeline: sequential particle placement
    rotation.rs        Shared: RotationMode, parse_rotation_mode, sample_rotation_axis
    scale.rs           ScalePipeline: unit conversion / factor scaling
    split_filter.rs    SplitFilterPipeline: connected-component split + geometric filtering

tests/
  core_tests.rs        Unit tests for geometry kernels
  io_tests.rs          I/O roundtrip tests (STL, TIFF, RAW)
  pipeline_smoke_tests.rs  Integration tests: all 7 pipelines with synthetic data

data/
  input/               Default YAML configs and sample input files
  output/              Pipeline outputs (gitignored)
```

## Architecture

### Pipeline Trait
All pipelines implement `src/pipeline/mod.rs::Pipeline` with a single `fn run(&self) -> Result<()>`. Each pipeline reads its config, performs computation, and writes output.

### Parallelism
- **Rayon** is the primary parallelism framework. Thread pools are created per-pipeline via `ThreadPoolBuilder`.
- **S2 computation**: `calculate_s2` parallelizes over radii with `into_par_iter`. FFT-based exact S2 parallelizes all three axes with `par_chunks_mut`. Monte Carlo S2 parallelizes per-radius samples.
- **Voxelization**: `build_bbox_occupancy` parallelizes over x-slabs with `par_chunks_mut`. Uses parry3d `TriMesh::contains_local_point()` with internal QBVH for O(log F) point queries instead of O(F) ray casting.
- **Volume fraction**: `volume_fraction_of_meshes_in_bbox` uses `par_iter` over meshes.
- **Crop rotation**: `rotate_and_crop` parallelizes over z-slices with `par_chunks_mut`.
- **Optimize collision**: Uses `SpatialGrid` for O(k) neighbor queries instead of O(N) full scan.
- **Island model**: When `optimization.islands > 1`, multiple independent SA instances run in parallel via `std::thread::scope`, sharing `Arc<Mutex<GlobalBest>>` for periodic migration.

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
| `serde` + `serde_yaml` | YAML config deserialization |
| `indicatif` | Progress bars |

## Code Conventions

- No comments in code unless explicitly requested.
- Follow existing patterns: use `crate::types::{Mesh, Vec3, BoundingBox}` for core types.
- Geometry functions live in `src/geometry/`, pipeline logic in `src/pipeline/`, config in `src/config/`.
- Public API is re-exported through `mod.rs` files in each module.
- Error handling uses `crate::error::{Result, RustMsptError}`.
- All new parallel code must use rayon and be safe for concurrent access (no data races on mutable state).
- Config fields use `serde` deserialization with optional `deserialize_with` for flexible numeric parsing.

## Common Pitfalls

- `ThreadPool` is not `Clone`. Use references or create separate pools per island.
- `parry3d::TriMesh::new()` can return `Err` for degenerate meshes; always handle with `Option`.
- `rustfft::Fft` is `Send + Sync` but not `Clone`. For parallel FFT, each thread creates its own `FftPlanner`.
- `Volume3D.data` stores voxel values as `i64`. The `numeric_type` field tracks original bit depth.
- `split_mesh_into_granules` returns an owned `Vec<Mesh>`; each granule is a connected component.
- SA optimization state (temperature, acceptance window) is per-island; do not share mutable SA state across threads.
