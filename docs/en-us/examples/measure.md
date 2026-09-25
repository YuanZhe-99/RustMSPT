# `measure` pipeline example

## What it does

`measure` characterizes a packed microstructure mesh by computing its solid volume fraction (VF)
and its S2 two-point correlation curve — the probability that two points a distance `r` apart both
land in solid material — over a chosen bounding box, using an exact voxel-grid method, a Monte
Carlo estimate, or both. Results (VF, backend used, and the full S2 series) are printed to stdout
and written to a text report. See `../algorithms/s2-two-point-correlation.md` for the full
derivation of the ray-casting/voxelization methods and the exact-vs-Monte-Carlo tradeoffs.

## Config

`data/input/measure_config.yaml` (the repository's real default config for this pipeline):

```yaml
measurement:
  stl_path: "data/output/optimized_structure.stl"
  stl_bounding_box: [0.0, 0.0, 0.0, 50.0, 50.0, 50.0] # Optional: [min_x, min_y, min_z, max_x, max_y, max_z]
  r_max: 20
  voxel_pitch: 1.0 # Larger pitch = faster but less accurate
  mc_method: 'both' # 'monte_carlo' or 'exact' or 'both'
  mc_samples: 40_000 # Samples for Monte Carlo estimation
  cpu_max: -1 # -1 uses all available cores
  output_path: "data/output/measured_s2.txt"
  # Acceleration: auto selects GPU when available and workload is large enough
  acceleration:
    mode: auto # auto | cpu | gpu
```

As with the other two examples, `stl_path` in the default config points at
`data/output/optimized_structure.stl` (the expected output of `optimize`), which is absent in a
fresh checkout. This walkthrough instead measures the real, already-present
`data/output/packed_result.stl` via `--input`, and writes its report to a scratch path via
`--output` (which overrides `measurement.output_path`).

| Field | Meaning |
|---|---|
| `stl_bounding_box` / `bounding_box` | Region to measure over. Precedence is `bounding_box` (explicit) > `stl_bounding_box` > mesh-derived bbox > unit-cube fallback. Here it restricts measurement to the `[0,0,0]`–`[50,50,50]` corner of the packed domain rather than the whole ~105-unit mesh. |
| `r_max` | Maximum correlation radius (in voxel units) the S2 curve is evaluated out to; produces `r_max + 1` points (`r = 0..=r_max`). |
| `voxel_pitch` | Voxel edge length for discretization; smaller = finer/slower. `1.0` over a `50x50x50` box gives 125,000 voxels. |
| `mc_method` | `"exact"` (voxel-grid ray casting), `"monte_carlo"` (random sampling), or `"both"` (runs both and reports their L2 distance). `"exact"` is never downgraded: it is refused with an error when no CPU exact kernel fits the 768 MiB working-set budget (see `[Info] CPU exact working-set plan` on stdout). |
| `mc_samples` | Number of Monte Carlo sample points per radius; accepts underscore-separated literals like `40_000`. |
| `cpu_max` | Thread count for the dedicated Rayon pool; `-1` uses all available cores. |
| `acceleration.mode` | `auto`/`cpu`/`gpu`. `auto` only selects GPU when the workload (voxel count) clears `gpu_min_voxels` (default 250,000) — below that, it silently falls back to CPU, as seen in this run's 125,000-voxel workload. |

See `../reference/config.md` (`measurement.rs` section: `MeasurementParams`,
`MeasurementConfig`) for the full field reference, and `../reference/pipeline-core.md`'s
`MeasurePipeline::run` entry for the exact backend-selection and bbox-resolution logic.

## Running it

```bash
./target/release/rustmspt measure \
  --config data/input/measure_config.yaml \
  --input data/output/packed_result.stl \
  --output /tmp/rustmspt-doc-examples/measured_s2.txt
```

## Expected output

Real captured stdout from the run above:

```
[Info] CPU setting: cpu_max=-1 -> using 8 worker threads (available 8).
[Info] Rayon pool threads (effective): 8
[Info] STL file(s) loaded from: data/output/packed_result.stl
[Info] Measurement bbox: min=(0.0000,0.0000,0.0000), max=(50.0000,50.0000,50.0000)
[Info] S2 config: method=both, r_max=20, mc_samples=40000, voxel_pitch=1.000000
[Info] Acceleration: requested=auto, effective=cpu
[Info] Acceleration fallback: workload 125000 voxels below gpu_min_voxels threshold 250000
[Info] Measurement completed.
[Info] Volume fraction: 0.024314
[Info] VF detail: particles=31 (in-box clipped-volume sum)
[Info] Compute backend: cpu
[Info] Method: both
[Info] S2(0)-VF diff [exact]: 0.000062
[Info] S2(0)-VF diff [monte_carlo]: 0.000062
[Info] L2 error [exact vs monte_carlo]: 0.001406
[Info] S2 points [exact]: 21
[Info] S2 points [monte_carlo]: 21
[Info] Output written: /tmp/rustmspt-doc-examples/measured_s2.txt
```

Real content of `/tmp/rustmspt-doc-examples/measured_s2.txt`:

```
Volume Fraction: 0.024314
Compute Backend: cpu
Method: both
S2(0)-VF diff [exact]: 0.000062
S2(0)-VF diff [monte_carlo]: 0.000062
L2 error [exact vs monte_carlo]: 0.001406
S2 Values [exact]:
0: 0.024376
1: 0.020591
2: 0.017944
3: 0.015522
4: 0.013235
5: 0.010934
6: 0.008934
7: 0.007353
8: 0.005984
9: 0.004693
10: 0.003537
11: 0.002585
12: 0.001797
13: 0.001115
14: 0.000658
15: 0.000450
16: 0.000411
17: 0.000412
18: 0.000427
19: 0.000441
20: 0.000448
S2 Values [monte_carlo]:
0: 0.024376
1: 0.021255
2: 0.017952
3: 0.015765
4: 0.012640
5: 0.011458
6: 0.008620
7: 0.007115
8: 0.005997
9: 0.004140
10: 0.003350
11: 0.002259
12: 0.002145
13: 0.001226
14: 0.000751
15: 0.000457
16: 0.000258
17: 0.000178
18: 0.000418
19: 0.000485
20: 0.000608
```

This ran in `real 0m0.093s` — the 50x50x50 ROI at `voxel_pitch: 1.0` is only 125,000 voxels, well
within the exact-method 768 MiB working-set budget and under the 250,000-voxel GPU
threshold, so it ran fully "exact" on CPU alongside the 40,000-sample Monte Carlo estimate.

## Notes

- `S2(0)` (`0.024376`) is slightly above the reported whole-run volume fraction (`0.024314`) by
  `0.000062` in both methods — this is the "S2(0)-VF diff" line, and is expected: `S2(0)` is
  computed from the voxel occupancy grid (a `voxel_pitch: 1.0` discretization), while VF is
  computed as an exact in-box clipped-volume sum over the 31 particles intersecting the ROI, so
  small quantization differences between the two are normal, not a bug.
- The exact and Monte Carlo S2 curves track closely at small `r` and diverge more (relatively) at
  large `r` where the underlying counts are small — e.g. at `r=17` exact gives `0.000412` vs. Monte
  Carlo's `0.000178`, more than 2x apart in relative terms though both are tiny in absolute terms.
  The reported `L2 error [exact vs monte_carlo]: 0.001406` summarizes this over the whole curve.
  Raising `mc_samples` well above `40_000` would tighten the Monte Carlo curve's agreement with the
  exact curve, at proportional cost; see
  `../algorithms/s2-two-point-correlation.md` for the full accuracy/cost tradeoff between the two
  methods.
- Because the measured region (125,000 voxels) is far below `gpu_min_voxels: 250_000`, this
  particular config always runs on CPU even with `acceleration.mode: auto` and even on a machine
  with a working GPU backend — increase the ROI or decrease `voxel_pitch` to exceed the threshold
  if you want to exercise the GPU path (see `../reference/gpu.md`).
- `mc_method: 'exact'` alone (or as part of `'both'`) is never replaced by Monte Carlo. Before
  voxelizing, the pipeline prints `[Info] CPU exact working-set plan: ...` with the FFT/direct
  choice, modeled times and working sets; if no kernel fits the 768 MiB budget (e.g. a much larger
  bbox or much finer `voxel_pitch`) the run fails with an explicit error instead. Output values do
  not depend on which kernel is chosen.

### Timing lines (added 2026-09-25)

Captured output above predates the shared stage timer. Current builds also print `[Timing] measure stage=<name> seconds=<f>` for each completed stage, then `[Timing] measure workers=<n>` and `[Timing] measure peak_rss_bytes=<n|unavailable>`. Stage names are listed in `../reference/pipeline-core.md` (`pipeline/timing.rs`); output files are unchanged.
