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
```

Override config path:

```bash
cargo run -- measure --config data/input/measure_config.yaml
```

Override input/output paths from CLI:

```bash
cargo run -- pack --input data/input/particles.stl --output data/output/packed_result.stl
```

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
  - `packing.cpu_max`

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
