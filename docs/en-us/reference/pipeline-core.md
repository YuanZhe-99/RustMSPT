# Pipeline Core Reference

Covers the `Pipeline` trait infrastructure and the simpler/support pipelines: `rotation`, `scale`, `forge`, `measure`, and `render`. The `pack`, `optimize`, `crop`, and `split_filter` pipelines are documented elsewhere.

## Index

| Item | Location | Summary |
|---|---|---|
| `RenderPipeline::run_in_pool` | `src/pipeline/render.rs:59` | Execute render stages and fallback within the configured pool. |
| `Pipeline::run` (trait) | `src/pipeline/mod.rs:17` | Trait method every pipeline struct implements to execute end-to-end. |
| `create_progress_bar` | `src/pipeline/mod.rs:53` | Builds a tty-aware indicatif progress bar with a given template and fill characters. |
| `run_in_cpu_pool` | `src/pipeline/mod.rs:37` | Runs work in a dedicated Rayon pool sized from a `cpu_max` setting (absent/-1: all workers); used by forge and scale so every parallel section shares one budget. |
| `RotationMode` (enum) | `src/pipeline/rotation.rs:6` | Represents no rotation, a fixed axis, or a random axis. |
| `parse_rotation_mode` | `src/pipeline/rotation.rs:17` | Parses `none/x/y/z/vector/any` config strings into a `RotationMode`. |
| `sample_rotation_axis` | `src/pipeline/rotation.rs:56` | Draws a concrete rotation axis vector for a given `RotationMode`. |
| `ScalePipeline` (struct) | `src/pipeline/scale.rs:8` | Holds `ScaleConfig` for the scaling pipeline. |
| `ScalePipeline::run` | `src/pipeline/scale.rs:116` | Loads an STL, applies unit-conversion/factor scaling, optionally fixes orientation, saves output. |
| `ForgePipeline` (struct) | `src/pipeline/forge.rs:12` | Holds `ForgingConfig` for the FFD forging pipeline. |
| `ForgePipeline::parse_roi_bbox` | `src/pipeline/forge.rs:18` | Parses an optional 6-element ROI bounding box from config. |
| `ForgePipeline::parse_compression_axis` | `src/pipeline/forge.rs:37` | Parses the compression axis string (`x`/`y`/`z`) into an index and label. |
| `ForgePipeline::run` | `src/pipeline/forge.rs:259` | Runs FFD-based compression/forging, tracks ROI, writes forged STL and a text report. |
| `MeasurePipeline` (struct) | `src/pipeline/measure.rs:12` | Holds `MeasurementConfig` for the S2/volume-fraction measurement pipeline. |
| `MeasurePipeline::parse_optional_bbox` | `src/pipeline/measure.rs:18` | Parses an optional bounding box (3-element size or 6-element min/max) from config. |
| `MeasurePipeline::l2_error` | `src/pipeline/measure.rs:27` | Computes the L2 distance between two S2 value vectors over their common prefix length. |
| `MeasurePipeline::run` | `src/pipeline/measure.rs:49` | Loads an STL, computes volume fraction and S2 correlation (exact/MC/both, CPU or GPU), writes a report. |
| `RenderPipeline` | `src/pipeline/render.rs:14` | Holds `RenderConfig`. |
| `RenderPipeline::run` | `src/pipeline/render.rs:28` | Loads STL, builds camera, selects CPU/GPU, and writes PNG. |
| `MeasurePipeline::run_in_pool` | `src/pipeline/measure.rs:83` | Method-specific measurement in configured pool. |

---

## `pipeline/mod.rs`

#### Pipeline::run (trait)

- **Signature:** `fn run(&self) -> Result<()>`
- **Source:** `src/pipeline/mod.rs:17`
- **Purpose:** Common execution entry point implemented by every pipeline in this crate.
- **Parameters:** `&self` — the concrete pipeline struct's own config.
- **Returns:** `Ok(())` on success, or a `RustMsptError` on failure.
- **Side effects:** Varies entirely by implementer — typically reads an input STL, performs geometry work, writes output file(s), and prints `[Info]`/`[Warning]` progress lines to stdout/stderr.
- **Notes:** Implemented by all 8 pipeline structs in this crate, including `RenderPipeline`. `main.rs` constructs the selected pipeline and calls `.run()` once per invocation.
- **See also:** `src/main.rs` for pipeline dispatch.

#### create_progress_bar

- **Signature:** `pub fn create_progress_bar(length: u64, template: &str, chars: &str) -> ProgressBar`
- **Source:** `src/pipeline/mod.rs:37`
- **Purpose:** Build a standardized `indicatif` progress bar, automatically hidden when output isn't an interactive terminal.
- **Parameters:**
  - `length` — total number of steps/units the bar represents.
  - `template` — indicatif template string (e.g. `"{bar} {pos}/{len}"`).
  - `chars` — the fill/head/empty characters passed to `progress_chars`.
- **Returns:** A configured `ProgressBar`. If the template string fails to parse, falls back silently to `ProgressStyle::default_bar()`.
- **Side effects:** Checks `std::io::stderr().is_terminal()`; if stderr is not a TTY (e.g. redirected to a file, or running in CI), the bar's draw target is set to `hidden()` so no progress output is emitted.
- **Notes:** Used by the longer-running pipelines (pack, optimize) to report iteration/placement progress without corrupting non-interactive logs.

---

## `pipeline/rotation.rs`

Shared rotation-axis utilities used by the `pack` and `optimize` pipelines when randomly or deterministically orienting particles/components.

#### RotationMode (enum)

- **Source:** `src/pipeline/rotation.rs:6`
- **Purpose:** Represents the rotation strategy applied to a placed object.
- **Variants:**
  - `None` — no rotation is applied.
  - `Axis(Vec3)` — rotation is constrained to a single fixed axis (unit axes `x`/`y`/`z`, or an arbitrary user-supplied vector).
  - `Any` — rotation axis is sampled randomly per placement.
- **Notes:** Derives `Clone` only; no `Debug`/`Copy`.

#### parse_rotation_mode

- **Signature:** `pub fn parse_rotation_mode(prefix: &str, mode: Option<&str>, axis_vec: Option<&Vec<f64>>) -> Result<RotationMode>`
- **Source:** `src/pipeline/rotation.rs:18`
- **Purpose:** Parse a rotation-mode configuration string into a `RotationMode` value.
- **Parameters:**
  - `prefix` — config-section name used only to build readable error messages (e.g. `"pack"` or `"optimize"`).
  - `mode` — the raw mode string: one of `none`, `x`, `y`, `z`, `vector`, `any` (case-insensitive, whitespace-trimmed). Defaults to `"any"` when `None`.
  - `axis_vec` — required only when `mode == "vector"`; must have exactly 3 elements and non-zero magnitude.
- **Returns:** `Ok(RotationMode)` on success.
- **Side effects:** None (pure parsing).
- **Notes:** Returns `RustMsptError::InvalidConfig` when: the mode string is unrecognized; `"vector"` is requested but `axis_vec` is missing or not length 3; or the supplied vector's norm is `<= 1e-12` (effectively zero, checked via `geometry::vec_norm`).

#### sample_rotation_axis

`pub fn sample_rotation_axis(rng: &mut rand::rngs::ThreadRng, mode: &RotationMode) -> Option<Vec3>` — `src/pipeline/rotation.rs:57`. Returns `None` for `RotationMode::None`, the fixed axis unchanged for `RotationMode::Axis`, and for `RotationMode::Any` returns a freshly sampled `Vec3` with each component drawn uniformly from `[-1.0, 1.0)`. No side effects beyond consuming RNG state.
> **Doc note:** the AI-FUNC-SUMMARY says the "Any" vector is "random in cube," which matches the code — it is *not* normalized to the unit sphere before being returned, so downstream code must normalize it if a unit rotation axis is required.

---

## `pipeline/scale.rs`

#### ScalePipeline (struct)

- **Source:** `src/pipeline/scale.rs:8`
- **Fields:** `config: ScaleConfig` — the full scaling configuration (input/output paths, scaling type/value, orientation flag).

#### ScalePipeline::run

- **Signature:** `fn run(&self) -> Result<()>`
- **Source:** `src/pipeline/scale.rs:19`
- **Purpose:** Load an STL mesh, rescale it by a unit-conversion or explicit factor, optionally fix component orientation to positive signed volume, and save the result.
- **Parameters:** `&self` — reads `self.config.input.stl_path`, `self.config.output.stl_path`, `self.config.scaling.{type, value, orient_to_positive_volume}`.
- **Returns:** `Ok(())` on success; `Err(RustMsptError::InvalidConfig(..))` for a non-positive `mm_per_voxel`/`voxel_per_mm` value or an unrecognized `scaling.type`.
- **Side effects:** Reads the STL (or folder of STLs, merged) from disk via `load_stl_or_merge_folder`; writes the scaled mesh to `output.stl_path` via `save_stl`; prints `[Info]` lines to stdout reporting mode, value, original/scaled bounds, volumes, and (if enabled) how many mesh components were flipped for orientation.
- **Notes:** Three scaling modes are supported:
  - `"factor"` — `value` is used directly as the multiplicative scale factor.
  - `"mm_per_voxel"` — `value` (must be `> 0`) is used directly as the factor.
  - `"voxel_per_mm"` — `value` (must be `> 0`) is inverted (`1.0 / value`) to obtain the factor.

  When `orient_to_positive_volume` is enabled, `orient_components_to_positive_volume` is applied post-scale and the number of flipped/total components is reported; otherwise the mesh is used unmodified (cloned).

---

## `pipeline/forge.rs`

#### ForgePipeline (struct)

- **Source:** `src/pipeline/forge.rs:12`
- **Fields:** `config: ForgingConfig` — forging parameters (input/output paths, compression ratio/axis, bulge factor, ROI bounding box, mesh type, void densification, orientation flag).

#### ForgePipeline::parse_roi_bbox

- **Signature:** `fn parse_roi_bbox(values: &Option<Vec<f64>>) -> Option<BoundingBox>`
- **Source:** `src/pipeline/forge.rs:18`
- **Purpose:** Parse an optional 6-element `[min_x, min_y, min_z, max_x, max_y, max_z]` region-of-interest box from config.
- **Parameters:** `values` — `Option<Vec<f64>>` from `config.forging.roi_bounding_box`.
- **Returns:** `Some(BoundingBox)` if the vector has exactly 6 elements; `None` otherwise (missing config or wrong length — the wrong-length case is silently treated as "no ROI" rather than an error).
- **Side effects:** None.

#### ForgePipeline::parse_compression_axis

- **Signature:** `fn parse_compression_axis(axis: Option<&str>) -> Result<(usize, &'static str)>`
- **Source:** `src/pipeline/forge.rs:37`
- **Purpose:** Resolve the configured compression axis string to a numeric mesh-axis index and a display label.
- **Parameters:** `axis` — optional string, defaults to `"z"` when `None`; matched case-insensitively after trimming.
- **Returns:** `Ok((0, "x"))`, `Ok((1, "y"))`, or `Ok((2, "z"))`.
- **Side effects:** None.
- **Notes:** Returns `RustMsptError::InvalidConfig` for any string other than `x`/`y`/`z`.

#### ForgePipeline::run

- **Signature:** `fn run(&self) -> Result<()>`
- **Source:** `src/pipeline/forge.rs:58`
- **Purpose:** Execute the free-form deformation (FFD) forging pipeline: compress a mesh along one axis (with bulge and optional void densification), track how the region of interest (ROI) moves through the deformation, realign the output so the ROI lands where it was requested, and write both the forged STL and a human-readable report.
- **Parameters:** `&self` — reads `self.config.forging.*`: `input_stl_path`, `output_stl_path` (defaults to `data/output/forged_mesh.stl`), `roi_bounding_box`, `compression_ratio` (default `0.2`), `compression_axis` (default `"z"`), `orient_to_positive_volume`, `bulge_factor` (default `0.5`), `mesh_type` (default `"particle"`), `void_densification` (default `1.0`).
- **Returns:** `Ok(())` on success, or a propagated I/O / config error.
- **Side effects:**
  - Reads the input STL (or merged folder) from disk.
  - Calls `simulate_forging_ffd_with_tracking` to perform the actual FFD deformation and obtain the post-deformation ROI location.
  - Optionally re-orients mesh components to positive signed volume (`orient_components_to_positive_volume`).
  - Translates the output mesh (`translate_mesh`) so the tracked ROI's minimum corner realigns with the originally requested ROI minimum corner, when both are available and the shift is non-negligible (`> f64::EPSILON` on any axis).
  - Writes the forged mesh via `save_stl`.
  - Writes a text report to `<output>.txt` (same path, extension replaced) containing before/after bounding boxes (mesh and ROI), before/after ROI volume fraction, compression axis, orientation-fix stats, and the output translation vector. Creates the parent directory if needed.
  - Prints the same information as `[Info]` lines to stdout.
- **Notes:** Volume fraction is computed over the ROI bbox if supplied, otherwise over the whole mesh's bounding box (`lattice_bbox`). The "before" volume and "after" volume (`_before`, `_after` via `mesh_volume`) are computed but not directly used beyond being discarded — see `> **Doc note:**` below.
  > **Doc note:** `_before` and `_after` (whole-mesh volumes) are computed via `mesh_volume` but bound to underscore-prefixed variables and never surfaced in the report or console output; only the ROI volume fractions (`before_roi_vf`, `after_roi_vf`) are reported. This looks intentional (silencing an "unused" warning while keeping the computation available for future use or debugging) rather than a bug, but the values are not currently visible to users.
- **See also:** [../algorithms/ffd-forging.md](../algorithms/ffd-forging.md)

---

## `pipeline/measure.rs`

#### MeasurePipeline (struct)

- **Source:** `src/pipeline/measure.rs:12`
- **Fields:** `config: MeasurementConfig` — measurement parameters (STL path, bounding box(es), S2 method/samples/pitch, acceleration policy, output path, CPU cap).

#### MeasurePipeline::parse_optional_bbox

- **Signature:** `fn parse_optional_bbox(values: &Option<Vec<f64>>) -> Result<Option<BoundingBox>>`
- **Source:** `src/pipeline/measure.rs:18`
- **Purpose:** Parse an optional bounding box from config, accepting either a 3-element size vector or a 6-element min/max vector (delegating the actual shape-dependent parsing to `config::parse_box_dimensions`).
- **Parameters:** `values` — `Option<Vec<f64>>`.
- **Returns:** `Ok(None)` if `values` is `None` or an empty vector; otherwise `Ok(Some(BoundingBox))` from `parse_box_dimensions`, or a propagated `Err` if that parse fails.
- **Side effects:** None.

#### MeasurePipeline::l2_error

- **Signature:** `fn l2_error(a: &[f64], b: &[f64]) -> f64`
- **Source:** `src/pipeline/measure.rs:27`
- **Purpose:** Compute the L2 (Euclidean) distance between two S2 correlation value vectors, used to compare "exact" vs. "monte_carlo" results when `method = "both"`.
- **Parameters:** `a`, `b` — S2 value slices, potentially of different lengths.
- **Returns:** `sqrt(sum((a[i]-b[i])^2))` over `i` in `0..min(a.len(), b.len())`; `0.0` if either slice is empty.
- **Side effects:** None.
- **Notes:** This duplicates the core logic of `geometry::l2_norm` (sum-of-squared-differences then square root over a shared-length prefix). It is kept as a private, pipeline-local helper rather than reusing the geometry module's function — the two implementations should be kept in sync if either changes, since there is currently no shared code path between them.

#### MeasurePipeline::run / run_in_pool

`run() -> Result<()>` installs the whole measurement in a configured Rayon pool, including loading, preparation, GPU errors and CPU fallback. `run_in_pool() -> Result<()>` resolves `RUSTMSPT_ACCELERATION` once, validates finite pitch/dimensions, and executes exact/MC/both using method-specific decisions. Positive-pitch MC remains voxel MC and does not use the continuous GPU kernel. Only requested methods allocate pipelines. GPU errors obey `cpu_fallback`; output records each method's actual backend after fallback. Reports are written only when all requested methods succeed. Exact (alone or in `both`) is refused with an explicit error, never substituted by MC, when the grid's occupancy bytes alone exceed `DEFAULT_CPU_EXACT_BUDGET_BYTES` (768 MiB) or when `plan_exact_cpu` finds no CPU kernel whose working set fits that budget; the plan line (`[Info] CPU exact working-set plan: ...`) is printed first. This replaced the old fixed 1,500,000-cell ceiling (PERF-07). The budget bounds memory only: voxelization time and a direct kernel's runtime on a large grid are not capped, and the plan line reports the modeled kernel time. Worker count/index are observed inside the execution pool.


### Owned CPU transforms (PERF-16)

`forge_owned(mesh, lattice_bbox, track_bbox, compression_ratio, compression_axis, bulge_factor, mesh_type, void_densification)` consumes the mesh and applies the existing tracked FFD mapping. The public borrowed wrapper clones once and delegates. ForgePipeline moves its input into this entry, retains the already computed input bbox, removes unused whole-mesh volume scans, and moves the output when orientation is disabled. ScalePipeline likewise moves its transformed mesh when orientation is disabled. Both log transform-only seconds separately from I/O.

`map_vertices` keeps small slices serial and maps disjoint 8192-vertex blocks on the current Rayon pool only with multiple workers and at least max(131072, workers * 65536) vertices. Scale, translate and both FFD variants use it. Each vertex retains its arithmetic order; the void centroid keeps the serial index-order accumulation of `mesh_centroid` but, when the affine pass is serial, is summed inside that pass (`map_vertices_centroid`, bit-identical). Fusing the ROI output translation into the FFD pass was not done: `vf_after` and the orientation fix read the unshifted mesh, and clipping at shifted coordinates is not bit-identical. Scale's before/after volume and bbox scans are kept: `|f|^3 * V` is not the same float sum, and a scaled-bbox shortcut would only save one vertex pass. ROI remains unaffected by void closure. Clipped ROI VF and output bbox are still measured from actual geometry; no determinant approximation is substituted.


### Render execution policy

`RenderPipeline::run` installs the whole pipeline in the configured worker pool. `run_in_pool` loads geometry, resolves the environment override and method budget, renders and writes only after success. Both selection and runtime failures honor `cpu_fallback`. GPU work estimates include vertices, color/depth, aligned staging and uniforms. Actual CPU fallback executes in the same pool.

## `pipeline/timing.rs` — stage timing and peak RSS (PERF-00)

| Item | Location | Summary |
|---|---|---|
| `StageTimer` | `src/pipeline/timing.rs:3` | Per-pipeline total and current-stage clocks. |
| `StageTimer::start` | `src/pipeline/timing.rs:11` | Start both clocks for a named pipeline. |
| `StageTimer::restart` | `src/pipeline/timing.rs:17` | Reset the stage clock without printing (excludes untimed work). |
| `StageTimer::stage` | `src/pipeline/timing.rs:22` | Print the time since the last mark and restart the stage clock. |
| `StageTimer::report` | `src/pipeline/timing.rs:30` | Print an externally accumulated duration as a stage. |
| `StageTimer::total` | `src/pipeline/timing.rs:35` | Print the time since start under a given stage name. |
| `StageTimer::report_resources` | `src/pipeline/timing.rs:42` | Print workers and peak RSS lines. |
| `format_stage_line` | `src/pipeline/timing.rs:49` | Format `[Timing] <pipeline> stage=<name> seconds=<f>` (nine decimals). |
| `report_workers` | `src/pipeline/timing.rs:54` | Print `[Timing] <pipeline> workers=<rayon::current_num_threads()>`. |
| `report_peak_rss` | `src/pipeline/timing.rs:59` | Print `[Timing] <pipeline> peak_rss_bytes=<n|unavailable>`. |
| `peak_rss_bytes` | `src/pipeline/timing.rs:67` | VmHWM from `/proc/self/status` in bytes, `None` when unavailable. |
| `parse_vm_hwm` | `src/pipeline/timing.rs:73` | Parse a `VmHWM: <n> kB` line; `None` when absent, malformed or not kB. |

Every non-meshgen pipeline prints wall-clock stage lines in one format, `[Timing] <pipeline> stage=<name> seconds=<f>`, then `[Timing] <pipeline> workers=<n>` (the Rayon worker count of the pool the stages ran in — the configured pool for pipelines that build one, the global pool for `forge`/`scale`, which honour `RAYON_NUM_THREADS`) and `[Timing] <pipeline> peak_rss_bytes=<n|unavailable>`. Peak RSS is the process high-water mark read from `VmHWM`; it covers the whole process, not only the timed stages, and is never estimated: a platform without `/proc` prints `unavailable`. Stage lines are emitted only for stages that completed; an error returns before any fabricated measurement.

| Pipeline | Stages (in order) | Final line |
|---|---|---|
| `split-filter` | `load`, `split`, `metrics`, `filter`, `write_stl`, `write_report` | `total_in_pool` |
| `pack` (legacy `packing:`) | `load` (targets, input, split, filters), `pack_loop`, `merge_orient`, `write_stl`, `report` (summary and distribution CSV) | `total_in_pool` |
| `pack` (`placement:`) | `load` (library, sizes, void), `plan` (multiset and first report), `place` (placement and top-up), `write_outputs`, `report` | `total_in_pool` |
| `optimize` | `load`, `prepare_evaluator`, `target_s2`, `input_s2`, `prune`, `anneal`, `final_s2`, `write_stl`, `write_history` | `total_in_pool` |
| `measure` | `load`, `split_vf`, `s2_exact` and/or `s2_monte_carlo`, `write` | `total_in_pool` |
| `forge` | `load`, `vf_before`, `transform`, `vf_after`, `orient_shift`, `write_stl`, `write_report` | `total` |
| `scale` | `load`, `stats_before`, `transform`, `orient_stats_after`, `write_stl` | `total` |
| `render` | `load`, `prepare`, `render_and_backend`, `encode_write` | `total_in_pool` |
| `mesh-render` | `load`, `scene`, `cameras`, then `gpu_render_write` when a GPU attempt was made, and `cpu_prepare`, `cpu_render`, `encode_write` (per-view sums; they overlap because view i renders while view i-1 is written) and `cpu_render_write_wall` (their wall time) when the CPU renderer ran | `total_in_pool` |
| `crop` | `load`, `background`, `pca`, `transform_and_backend`, `trim`, `encode_write` (unchanged names) | `total_in_pool` |

`total_in_pool` excludes CLI parsing, config loading and pool creation; `process_wall` in `scripts/perf_matrix.py` measures the whole process. Timing lines go to stdout only and never into output files, records or reports, so outputs and placement determinism are unchanged. Legacy `pack` and `optimize` additionally print `[GridStats]` lines (see `pipeline-packing.md` and `pipeline-optimize.md`).

### Benchmark matrix runner

`scripts/perf_matrix.py` (python3, standard library only) runs selected pipelines at worker counts (default 1/2/4/8) with `--warmup` cold runs (default 1, reported separately) and `--repeats` warm runs (default 5). Each run gets its own config copy (worker field set through `cpu_max`, `packing.cpu_max`, `optimization.cpu_max`, `measurement.cpu_max`, `render.cpu_max` or `--threads` for placement; `RAYON_NUM_THREADS` is also set, and `RUSTMSPT_ACCELERATION=cpu`) and its own output directory under `--work`, so nothing under `data/` is overwritten. Optimize, measure, forge and scale consume a small chain (pack then optimize, run once) produced in the work directory. Output: `perf_matrix_raw.json` (every run's stages, reported workers, peak RSS, grid lines, return code, process wall time) and `perf_matrix_summary.md` (per stage and worker count: cold, warm median/min/max, S(p) = median T1 / median Tp, E(p) = S(p) / reported workers, median peak RSS). Worker counts above the core count are clamped by pipelines that clamp `cpu_max`; the reported `workers` column shows what actually ran. "Cold" means the first process of a configuration, not a dropped page cache. `--optimize-iterations` reduces `optimization.max_iterations` for shorter runs. `--large` raises the work per run so worker scaling is measurable (the shipped inputs finish most pipelines in 3-90 ms): legacy pack target 0.15 with 20,000 attempts, placement VF 0.30, measure pitch 0.5 with 400,000 MC samples, render 4096^2, mesh-render 3072^2, and split-filter/forge/scale on `dense_particles.stl`, a seeded placement (200^3 domain, VF 0.30, ~34 MB) built once as a chain step. It changes sizes and targets only, never a method. `--chain DIR` reuses an existing chain directory instead of regenerating it: pack is unseeded, so two matrices only compare optimize or measure fairly on a shared chain.
