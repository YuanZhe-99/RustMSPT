# RustMSPT Documentation (English)

RustMSPT (Rust Microstructure Processing Toolbox) is a Rust toolkit for STL/CT-volume microstructure processing, providing eight pipelines — `split-filter`, `pack`, `optimize`, `measure`, `forge`, `scale`, `crop`, `render` — built on shared geometry, I/O, and (optionally GPU-accelerated) compute-kernel modules.

This directory documents the codebase at the function level, explains its core algorithms conceptually, and walks through each pipeline with real, captured example output. It complements, and is built from the same source as, the `// AI-FUNC-SUMMARY` comments that already annotate almost every function in `src/` — see [AGENTS.md](../../AGENTS.md) for that convention and the "Function Reading Policy" it defines.

**Snapshot scope:** this documentation was written against the working tree as of 2026-07-18, which includes the then-uncommitted packing target-diameter-distribution feature (`src/geometry/metrics.rs`, `src/pipeline/pack_targets.rs`, `data/input/gu2019_fig7b_pore_distribution.csv`). If those files are later modified, keep the corresponding docs in sync per the requirement in [AGENTS.md](../../AGENTS.md#code-conventions).

## Start here

- **[Function Index](reference/function-index.md)** — every documented function, method, struct, enum, and constant in `src/`, with source location and a one-line summary. The fastest way to find where something is implemented.

## Reference (per-module function/struct documentation)

Mirrors the `src/` module layout. Each document opens with an `## Index` table (used to compile the function index above) followed by full entries — signature, purpose, parameters, returns, side effects, notes — for every function, plus struct/enum field tables where relevant.

| Document | Covers |
|---|---|
| [core-and-compute.md](reference/core-and-compute.md) | `main.rs`, `lib.rs`, `error.rs`, `types.rs`, `bin/precision_test.rs`, `compute/*` |
| [config.md](reference/config.md) | `config/*` — YAML config structs and deserialization helpers |
| [geometry-core.md](reference/geometry-core.md) | `geometry/{mod,bbox,mesh_ops,spatial,render}.rs` |
| [geometry-volume-collision.md](reference/geometry-volume-collision.md) | `geometry/{volume,collision,forging}.rs` |
| [geometry-analysis.md](reference/geometry-analysis.md) | `geometry/{metrics,s2}.rs` |
| [gpu.md](reference/gpu.md) | `gpu/*` (requires `cargo build --features gpu`) |
| [io.md](reference/io.md) | `io/{image,stl,volume}.rs` |
| [pipeline-core.md](reference/pipeline-core.md) | `pipeline/{mod,rotation,scale,forge,measure,render}.rs` |
| [pipeline-crop-and-splitfilter.md](reference/pipeline-crop-and-splitfilter.md) | `pipeline/{crop,split_filter}.rs` |
| [pipeline-packing.md](reference/pipeline-packing.md) | `pipeline/{pack,pack_targets}.rs` |
| [pipeline-optimize.md](reference/pipeline-optimize.md) | `pipeline/optimize.rs` |

## Algorithms (conceptual explanations)

Higher-level explanations of the non-obvious algorithms behind the reference docs, cross-linked to the specific functions that implement them.

| Document | Explains |
|---|---|
| [s2-two-point-correlation.md](algorithms/s2-two-point-correlation.md) | The S2 statistic, CPU exact (FFT/direct) and Monte Carlo methods, GPU acceleration |
| [simulated-annealing-island-model.md](algorithms/simulated-annealing-island-model.md) | The `optimize` pipeline's SA core, adaptive temperature, and multi-island migration |
| [ffd-forging.md](algorithms/ffd-forging.md) | The `forge` pipeline's free-form-deformation compression/bulge model |
| [packing-target-diameter-distribution.md](algorithms/packing-target-diameter-distribution.md) | The `pack` pipeline's target-diameter-histogram bin-allocation and sphericity steering (newest feature) |
| [pca-volume-alignment-crop.md](algorithms/pca-volume-alignment-crop.md) | The `crop` pipeline's PCA-based orientation estimation and rotate/crop |
| [spatial-grid-collision.md](algorithms/spatial-grid-collision.md) | Neighbor-query acceleration and exact/periodic collision detection |
| [mesh-clipping-volume-fraction.md](algorithms/mesh-clipping-volume-fraction.md) | Sutherland-Hodgman mesh clipping and volume-fraction accounting |
| [stl-rendering.md](algorithms/stl-rendering.md) | Shared camera framing, CPU ray casting, and GPU offscreen rasterization |

## Examples (per-pipeline walkthroughs)

Each walkthrough shows a realistic config, the exact CLI command, and output captured from an actual run.

| Document | Pipeline |
|---|---|
| [forge.md](examples/forge.md) | `forge` |
| [measure.md](examples/measure.md) | `measure` |
| [scale.md](examples/scale.md) | `scale` |
| [crop.md](examples/crop.md) | `crop` |
| [split-filter.md](examples/split-filter.md) | `split-filter` |
| [optimize.md](examples/optimize.md) | `optimize` |
| [pack.md](examples/pack.md) | `pack` (plain volume-fraction packing) |
| [pack-target-distribution.md](examples/pack-target-distribution.md) | `pack` with target diameter distribution steering |
| [render.md](examples/render.md) | `render` |

## Conventions used throughout

- Function-entry headings are the bare function/method name (`#### mesh_metrics`, `#### TargetDistribution::choose_bin`) with no backticks, so markdown anchors are predictable for cross-linking.
- `> **Feature-gated:**` callouts mark code that only exists when built with `cargo build --features gpu`.
- `> **Doc note:**` callouts flag a handful of places where source `AI-FUNC-SUMMARY` comments were found to be stale relative to the actual code — the doc text reflects verified behavior, not the stale comment.
- `> **Algorithm:**` and `**See also:**` lines cross-link reference entries to the relevant conceptual algorithm doc and vice versa.

## Chinese translation

This documentation is English-only for now. See [../TRANSLATION_GUIDE.md](../TRANSLATION_GUIDE.md) for the terminology glossary and conventions that will govern the `docs/zh-cn/` mirror.
