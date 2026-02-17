# RustMSPT

RustMSPT is a standalone Rust toolkit for STL-based microstructure processing.

## Features

- End-to-end pipelines:
  - `forging`
  - `measurement`
  - `optimization`
  - `packing`
  - `scale`
- STL I/O:
  - Load ASCII and Binary STL (auto-detect)
  - Save Binary STL by default
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
./target/release/rustmspt measurement
./target/release/rustmspt packing --input data/input/particles.stl --output data/output/packed_result.stl
```

## CLI Usage

Run with default config paths (`data/input/*.yaml`):

```bash
cargo run -- forging
cargo run -- measurement
cargo run -- optimization
cargo run -- packing
cargo run -- scale
```

Override config path:

```bash
cargo run -- measurement --config data/input/measurement_config.yaml
```

Override input/output paths from CLI:

```bash
cargo run -- packing --input data/input/particles.stl --output data/output/packed_result.stl
```

## Configuration

Default config files:

- `data/input/forging_config.yaml`
- `data/input/measurement_config.yaml`
- `data/input/optimization_config.yaml`
- `data/input/packing_config.yaml`
- `data/input/scale_config.yaml`

### Measurement `mc_method`

`data/input/measurement_config.yaml` supports:

- `monte_carlo`: Monte Carlo S2 estimation
- `exact`: exact S2 using voxel occupancy correlation
- `both`: run both methods and report `L2 error [exact vs monte_carlo]`

## Outputs

- Each pipeline writes output files to paths defined in its config.
- Runtime logs are concise English messages with `[Info]`/`[Warning]`/`[Error]` style.

## Notes

- For large voxel grids, exact S2 may fallback for memory/performance safety.
- Keep `voxel_pitch` and `r_max` balanced against runtime and memory budget.
