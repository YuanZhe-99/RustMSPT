# RustMSPT

RustMSPT is a standalone Rust toolkit for STL-based microstructure processing.

## Features

- End-to-end pipelines:
  - `forge`
  - `measure`
  - `optimize`
  - `pack`
  - `scale`
  - `crop`
  - `split-filter`
  - `render`
  - `mesh-render`
  - `mesh-verify`
  - `mesh`
  - `version`
- STL I/O:
  - Load ASCII and Binary STL (auto-detect)
  - Save Binary STL by default
- CT image I/O:
  - RAW folder input (file sequence) with configurable `width/height/bits/signed/byte_order`
  - TIFF input/output for file and folder modes with `.tif` and `.tiff` support
  - Shared slice range `slice_start/slice_end` for both RAW and TIFF input (`-1` means begin/end)
- Geometry kernels:
  - Mesh splitting/merging
  - Bounding box clipping
  - Volume fraction computation
  - S2 (two-point correlation) with `monte_carlo`, `exact`, and `both` (measurement)
- GPU acceleration (optional, via `gpu` feature):
  - Monte Carlo S2 on GPU (WGSL compute shader)
  - Exact S2 via GPU voxelization + shell pair counting
  - Volume rotate-and-crop transform on GPU
  - Offscreen STL rendering with depth-buffered rasterization
  - `acceleration.mode` config: `auto` | `cpu` | `gpu`
  - Graceful CPU fallback when GPU is unavailable
- Performance controls:
  - CPU worker cap in optimization (`cpu_max`)
  - FFT-based exact S2 path with memory-aware fallback

## Project Layout

- `src/main.rs`: CLI entry point
- `src/config.rs`: YAML config models and parsers
- `src/io.rs`: STL loading and saving
- `src/geometry.rs`: geometry and S2 computation kernels
- `src/pipeline/*.rs`: pipeline implementations
- `data/input/*.yaml`: default pipeline configs
- `tests/*.rs`: unit/integration/smoke tests

## Build and Test

```bash
cargo check
cargo test
```

With GPU support (requires `wgpu` and Vulkan/Metal/DX12 backend):

```bash
cargo build --features gpu
cargo test --features gpu
cargo clippy --features gpu
```

To use the real GPU (not software fallback on Linux):

```bash
sudo usermod -aG render $USER  # then re-login
```

## Build Binary

Build release binary:

```bash
cargo build --release
```

Run binary directly:

```bash
./target/release/rustmspt measure
./target/release/rustmspt pack --input data/input/particles.stl --output data/output/packed_result.stl
```

## CLI Usage

Run with default config paths (`data/input/*.yaml`):

```bash
cargo run -- forge
cargo run -- measure
cargo run -- optimize
cargo run -- pack
cargo run -- scale
cargo run -- crop
cargo run -- split-filter
cargo run -- render
```

Override config path:

```bash
cargo run -- measure --config data/input/measure_config.yaml
```

Override input/output paths from CLI:

```bash
cargo run -- pack --input data/input/particles.stl --output data/output/packed_result.stl
```

### Pack: two engines

`pack` selects an engine from the config it is given.

- A top-level **`placement:`** block runs the seeded, recorded, void-aware engine: a reproducible
  run, a per-particle transform record, packing around a frozen void, and a JSON report that says
  why it stopped. See `data/input/placement_config.yaml` and
  `data/input/placement_void_config.yaml`.
- A top-level **`packing:`** block runs the original packing loop, unchanged. See
  `data/input/pack_config.yaml`.

Exactly one must be present.

```bash
cargo run --release -- pack --config data/input/placement_config.yaml
cargo run --release -- pack --config data/input/placement_void_config.yaml
```

Relative paths inside a config resolve against that config file's directory, never against the
working directory. Paths given on the command line resolve against the working directory, as shell
arguments do.

`--seed` and `--threads` apply to the placement engine only; passing either with a `packing:` config
is an error rather than a silent no-op.

Ask the binary what it is:

```bash
rustmspt --version
rustmspt version --json
```

Both report the same build: package version, git commit, whether the worktree was dirty when the
build script last ran, the enabled cargo features, and the build target/host/profile. Anything the
build could not determine is `null` rather than a fabricated default, so a checkout without git
still builds and still answers.

## Configuration

Default config files:

- `data/input/forge_config.yaml`
- `data/input/measure_config.yaml`
- `data/input/optimize_config.yaml`
- `data/input/pack_config.yaml`
- `data/input/scale_config.yaml`
- `data/input/crop_config.yaml`
- `data/input/split_filter_config.yaml`

### Forge pipeline (`forge_config.yaml`)

- Root key: `forging`
- Core fields:
  - `input_stl_path`, `output_stl_path`
  - `compression_ratio`, `compression_axis` (`x|y|z`)
  - `bulge_factor`
  - optional ROI: `roi_bounding_box: [minx,miny,minz,maxx,maxy,maxz]`
  - `mesh_type` (`particle|void`) and `void_densification` (for `void`)

### Measure pipeline (`measure_config.yaml`)

- Root key: `measurement`
- Core fields:
  - `stl_path`
  - bounding box via `bounding_box` or `stl_bounding_box` (optional)
  - `r_max`, `voxel_pitch`
  - `mc_method` (`monte_carlo|exact|both`)
  - `mc_samples`, `cpu_max`, `output_path`
  - `acceleration.mode` (`auto|cpu|gpu`, default `auto`)
- `mc_method=both` runs exact + Monte Carlo and reports `L2 error [exact vs monte_carlo]`.

### Optimize pipeline (`optimize_config.yaml`)

- Top-level sections: `input`, `target`, `box`, `optimization`, `output`
- Core fields:
  - `input.stl_path`
  - `target.type` (`manual_array|reference_stl`)
  - `target.s2_array` or `target.stl_path`
  - `box.dimensions` (`[sx,sy,sz]` or `[minx,miny,minz,maxx,maxy,maxz]`)
  - SA controls: `max_iterations`, `initial_temperature`, `cooling_rate`, adaptive parameters
  - S2 controls: `r_max`, `voxel_pitch`, `mc_method`, `mc_samples`
  - placement/constraint controls: translation, rotation, neighbor and boundary distances, `mode`
  - optional prune stage and `cpu_max`
  - `acceleration.mode` (`auto|cpu|gpu`, default `auto`)
  - `output.path`

### Pack pipeline (`pack_config.yaml`)

- Top-level sections: `input`, `output`, `box`, `packing`
- Core fields:
  - `input.path` (single STL or folder)
  - `output.path`
  - `box.dimensions`
  - `packing.target_volume_fraction`, `mode`, `max_attempts`
  - geometric constraints: `min_neighbor_distance`, `min_boundary_dist`, `min_cross_boundary_depth`
  - optional `packing.filters` (`min_volume`, `max_aspect_ratio`, `max_sharpness_ratio`)
  - optional `packing.target_diameter_distribution_csv` for count-frequency target void scaling
  - optional `packing.target_mean_sphericity` and `mean_sphericity_tolerance` as a soft shape target
  - `packing.cpu_max`

`target_diameter_distribution_csv` uses unitless diameters in STL units:

```csv
bin,right,frequency
5,6,0.12328767
6,7,0.12924360
```

`bin` is the interval left edge and `frequency` is a normalized count frequency. Missing `right` values are inferred from the next row, and the final width is inferred from the previous row. A single-row CSV requires an explicit `right`. Packing uses equivalent-volume diameter and scales a selected shape to the target interval midpoint when needed. If a target bin repeatedly cannot be placed, Packing may fall back to other bins to prioritize reaching `target_volume_fraction`; the final report prints target/actual bin counts, distribution error, scale statistics, and any relaxation warning.

When a target distribution is enabled, Packing also writes `<output_stem>_diameter_distribution.csv` beside the output STL. The comparison contains target/actual frequency and count, signed errors, and placement attempts for every bin.

When target diameter scaling is enabled, `filters.min_volume` is evaluated on the final scaled candidate. Aspect-ratio and sharpness filters are applied to source candidates and rechecked after scaling.

`target_mean_sphericity` compares the running arithmetic mean of successfully placed void sphericities with the target. It softly selects among a small random candidate window but never rejects a valid placement solely for sphericity, so an infeasible shape target does not block the volume-fraction target.

`data/input/gu2019_fig7b_pore_distribution.csv` is the converted example used by `pack_config.yaml`. It retains the digitized Figure 7b interval edges as unitless values and converts `frequency_normalized_pct` to fractional frequencies by dividing by 100.

### Scale pipeline (`scale_config.yaml`)

- Top-level sections: `input`, `output`, `scaling`
- Core fields:
  - `input.stl_path`, `output.stl_path`
  - `scaling.type` (`mm_per_voxel|voxel_per_mm|factor`)
  - `scaling.value`

### Crop pipeline (`crop_config.yaml`)

`crop` reads a 3D CT volume from RAW folder or TIFF file/folder, detects boundary background,
aligns foreground by PCA, crops the effective cuboid, and writes TIFF.

- Top-level sections: `input`, `output` plus optional `interpolation`, `edge_trim`
- Input fields:
  - `input.type` (`raw|tiff`)
  - `input.path`
  - shared range: `input.slice_start` / `input.slice_end` (inclusive, `-1` as begin/end)
  - for RAW: `input.raw.width/height/bits/signed/byte_order`
- Processing/output fields:
  - `interpolation` (`trilinear|nearest`, default `trilinear`)
  - `edge_trim` on XY border (`-1` auto infer 0~2, `0` off, `1/2` manual)
  - output TIFF file (`.tif/.tiff`) or folder with `output.folder_prefix` and `output.folder_extension`

### Split-Filter pipeline (`split_filter_config.yaml`)

`split-filter` accepts an STL file or STL folder and outputs split particle STL files.

- Top-level sections: `input`, `output`, optional `filter`
- Core fields:
  - `input.path`
  - `output.folder`, `output.prefix`, optional `output.report_path`
  - `filter.enabled`
  - optional geometric filters: `max_aspect_ratio`, `max_sharpness_ratio`
  - optional volume filter:
    - mode `range` with `min/max` (`-1` means no bound)
    - mode `lognormal_rebalance` with `bins/over_factor`
- If `output.report_path` is omitted, report defaults to `parent(output.folder)/split_filter_report.txt`.

## Outputs

- Each pipeline writes output files to paths defined in its config.
- Runtime logs are concise English messages with `[Info]`/`[Warning]`/`[Error]` style.

## Notes

- For large voxel grids, exact S2 may fallback for memory/performance safety.
- Keep `voxel_pitch` and `r_max` balanced against runtime and memory budget.
- GPU acceleration (`acceleration.mode: gpu`) uses `wgpu` + WGSL compute shaders. When set to `auto`, GPU is used when available and workload exceeds the `gpu_min_voxels` threshold (default 250k).
- Environment variables: `RUSTMSPT_GPU_DEVICE=<name|index>` to select GPU adapter.
