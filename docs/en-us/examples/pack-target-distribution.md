# Packing with a target diameter distribution

## What it does

This walkthrough runs `pack` with `packing.target_diameter_distribution_csv` set, so that instead
of accepting candidate particles at whatever size the input library happens to produce, the
pipeline steers each accepted placement toward an under-represented bin of a user-supplied
diameter histogram — rescaling candidates as needed — so the *placed* particles' equivalent-volume
diameters approximate a target distribution. It also demonstrates the paired, optional
`target_mean_sphericity` / `mean_sphericity_tolerance` soft-steering knobs. This is the newest
packing feature (`git log` shows it landed in commit `cc905c1`, "feat: add target-aware void
packing"); the full algorithm derivation lives in
[`../algorithms/packing-target-diameter-distribution.md`](../algorithms/packing-target-diameter-distribution.md)
and this document focuses on running it end-to-end and reading its output.

The target distribution used here is the repository's real bundled example,
`data/input/gu2019_fig7b_pore_distribution.csv` — a digitized pore-size histogram (Figure 7b from
Gu et al., 2019) with 25 one-unit-wide bins spanning diameters 5–30, confirmed by
`tests/pack_target_tests.rs::example_paper_distribution_is_valid_and_normalized` (`bins.len() ==
25`, first bin `[5, 6)`, last bin's `right == 30.0`, frequencies summing to `1.0`).

## Config

A scratch copy of `data/input/pack_config.yaml` at
`/tmp/rustmspt-doc-examples/pack_target_distribution.yaml`, with the output path redirected to
scratch, `target_volume_fraction` raised slightly to `0.03` (still fast, but produces more placed
particles than the plain example's `0.02` for a richer histogram), and the previously-commented
sphericity-steering fields enabled:

```yaml
input:
  path: "data/input/particles.stl"

output:
  path: "/tmp/rustmspt-doc-examples/pack_target_distribution_result.stl"

box:
  dimensions: [100.0, 100.0, 100.0] # X, Y, Z size

packing:
  target_volume_fraction: 0.03

  mode: 2
  max_attempts: 2000
  min_neighbor_distance: 1.0

  rotation_mode: 'none'
  rotation_axis_vector: [0.0, 0.0, 1.0]

  min_boundary_dist: 1.5
  min_cross_boundary_depth: 3.0

  cpu_max: -1
  orient_to_positive_volume: false

  # Optional target void diameter count-frequency CSV (unitless, matching STL units).
  # Columns: bin (left edge), right, frequency. Frequencies are normalized and sum to 1.
  # Also writes <output_stem>_diameter_distribution.csv beside the packed STL.
  target_diameter_distribution_csv: "data/input/gu2019_fig7b_pore_distribution.csv"

  # Optional soft target for the count-weighted arithmetic mean sphericity of placed voids.
  # Uniform diameter scaling preserves sphericity. Packing still prioritizes target_volume_fraction.
  target_mean_sphericity: 0.82
  mean_sphericity_tolerance: 0.02

  # Geometry filters. With a target diameter CSV, min_volume is checked after scaling.
  filters:
    min_volume: 15.0
    max_aspect_ratio: 3.0
    max_sharpness_ratio: 2.0
```

| Field | Meaning |
|---|---|
| `target_diameter_distribution_csv` | Path to a `bin[,right],frequency` CSV (see below) describing the target diameter histogram; setting this activates bin-debt steering and rescaling in `PackPipeline::run`. |
| `target_mean_sphericity` | Optional soft target for the count-weighted running-mean Wadell sphericity of accepted placements. |
| `mean_sphericity_tolerance` | Half-width of the acceptance band around `target_mean_sphericity`; `0.0`/unset means an exact-value target. |
| `filters.min_volume` | Still enforced, but — per the config comment and confirmed in the algorithm doc — checked against the **scaled** candidate volume when a target CSV is active, not the natural volume. |

Full field reference: [`../reference/config.md`](../reference/config.md) (`packing.rs` section,
`PackingParams` rows for `target_diameter_distribution_csv`, `target_mean_sphericity`,
`mean_sphericity_tolerance`).

## Running it

```bash
./target/release/rustmspt pack --config /tmp/rustmspt-doc-examples/pack_target_distribution.yaml
```

## Expected output

Real captured stdout from the run above (`real 0m0.235s`):

```
[Info] Target diameter distribution: 25 bins from 'data/input/gu2019_fig7b_pore_distribution.csv'
[Info] Target mean sphericity: 0.820000 +/- 0.020000
[Info] Packing input mode: preloaded single STL (14 candidate particles)
[Info] CPU setting: cpu_max=-1 -> using 8 worker threads (available 8).
[Info] Rayon pool threads (effective): 8
[Info] Rotation mode: none
[Info] Packing completed.
[Info] Final count: 44
[Info] Final volume fraction: 0.030078
[Info] Diameter distribution comparison CSV: /tmp/rustmspt-doc-examples/pack_target_distribution_result_diameter_distribution.csv
[Info] Diameter distribution bins: left,right,target,ideal_count,actual_count,actual,error,attempts
[Info] Diameter bin 0: 5.000000,6.000000,0.12328767,5.425,5,0.11363636,-0.00965131,6
[Info] Diameter bin 1: 6.000000,7.000000,0.12924360,5.687,6,0.13636364,+0.00712004,6
[Info] Diameter bin 2: 7.000000,8.000000,0.11673615,5.136,5,0.11363636,-0.00309979,8
[Info] Diameter bin 3: 8.000000,9.000000,0.10422871,4.586,5,0.11363636,+0.00940766,8
[Info] Diameter bin 4: 9.000000,10.000000,0.10780226,4.743,5,0.11363636,+0.00583410,7
[Info] Diameter bin 5: 10.000000,11.000000,0.08814771,3.878,4,0.09090909,+0.00276138,4
[Info] Diameter bin 6: 11.000000,12.000000,0.06491959,2.856,3,0.06818182,+0.00326222,6
[Info] Diameter bin 7: 12.000000,13.000000,0.04883859,2.149,2,0.04545455,-0.00338405,3
[Info] Diameter bin 8: 13.000000,14.000000,0.03692674,1.625,2,0.04545455,+0.00852780,4
[Info] Diameter bin 9: 14.000000,15.000000,0.03633115,1.599,2,0.04545455,+0.00912340,2
[Info] Diameter bin 10: 15.000000,16.000000,0.02918404,1.284,1,0.02272727,-0.00645677,1
[Info] Diameter bin 11: 16.000000,17.000000,0.01905896,0.839,1,0.02272727,+0.00366831,1
[Info] Diameter bin 12: 17.000000,18.000000,0.01548541,0.681,1,0.02272727,+0.00724186,2
[Info] Diameter bin 13: 18.000000,19.000000,0.01727219,0.760,1,0.02272727,+0.00545509,1
[Info] Diameter bin 14: 19.000000,20.000000,0.01131626,0.498,1,0.02272727,+0.01141101,1
[Info] Diameter bin 15: 20.000000,21.000000,0.00952948,0.419,0,0.00000000,-0.00952948,0
[Info] Diameter bin 16: 21.000000,22.000000,0.01012507,0.446,0,0.00000000,-0.01012507,0
[Info] Diameter bin 17: 22.000000,23.000000,0.00595593,0.262,0,0.00000000,-0.00595593,0
[Info] Diameter bin 18: 23.000000,24.000000,0.00536033,0.236,0,0.00000000,-0.00536033,0
[Info] Diameter bin 19: 24.000000,25.000000,0.00536033,0.236,0,0.00000000,-0.00536033,0
[Info] Diameter bin 20: 25.000000,26.000000,0.00416915,0.183,0,0.00000000,-0.00416915,0
[Info] Diameter bin 21: 26.000000,27.000000,0.00476474,0.210,0,0.00000000,-0.00476474,0
[Info] Diameter bin 22: 27.000000,28.000000,0.00238237,0.105,0,0.00000000,-0.00238237,0
[Info] Diameter bin 23: 28.000000,29.000000,0.00178678,0.079,0,0.00000000,-0.00178678,0
[Info] Diameter bin 24: 29.000000,30.000000,0.00178678,0.079,0,0.00000000,-0.00178678,0
[Info] Diameter distribution error: max_abs 0.01141101, total_variation 0.07381288, integer_rounding_max 0.01141101
[Info] Placement target kinds: natural 3, scaled 41, fallback 0
[Info] Scale factors: min 0.488649, mean 1.317943, max 3.683837
[Info] Final mean sphericity: 0.918922
[Info] Mean sphericity error: 0.098922
[Info] Mean sphericity tolerance met: false
[Warning] Target mean sphericity not reached; volume fraction was prioritized.
[Info] Orientation fix enabled: false
```

> **Every number on this page is one run's.** The `packing:` engine takes no seed, so the count, the
> per-bin actuals and the attempt counts all move between runs of this same config; only the volume
> fraction is pinned, and only from below, by the stopping rule. The shape of the comparison is what
> the page is about, not the digits. See the same note in [pack.md](pack.md). Since 0.2.1 the loop
> also rejects a candidate that would sit wholly inside a placed particle.

44 particles were placed at a final volume fraction of `0.030078` (just past the `0.03` target).
Of those 44, 3 were used at their **natural** size (their unscaled equivalent diameter already fell
in the bin `choose_bin` picked for them), 41 were **scaled** (rescaled to a target bin's midpoint
via `scale_mesh_to_equivalent_diameter`), and 0 fell back to a relaxed bin choice — the run never
needed `Fallback` placements at this modest volume fraction and bin count. Scale factors applied
ranged from `0.488649` (candidates shrunk to roughly half size) to `3.683837` (nearly quadrupled),
averaging `1.317943`.

The bins above diameter 20 (bins 15–24) all landed at 0 actual count despite nonzero target
frequency and nonzero attempts for bins 15–19 — 44 particles simply isn't enough to populate all 25
bins of a distribution whose upper tail carries very little probability mass (e.g. bin 24's target
frequency is `0.00178678`, i.e. under 0.2%). The reported `max_abs 0.01141101` error sits exactly at
`integer_rounding_max 0.01141101` — the theoretical best-possible error for packing exactly 44
discrete particles into this histogram's shape — confirming the bin-debt algorithm hit the rounding
floor rather than leaving avoidable slack.

Sphericity steering did not reach its `0.82 ± 0.02` target: the final count-weighted mean sphericity
was `0.918922` (particles in `data/input/particles.stl` are apparently rounder than the requested
target), giving an error of `0.098922` outside tolerance, hence the `[Warning]` line. This is
expected, documented behavior — sphericity steering is soft and ranks only among up-to-4 candidate
draws per attempt; it never rejects a candidate outright, and volume-fraction/diameter-bin
objectives take priority (see Notes).

### Real content of `pack_target_distribution_result_diameter_distribution.csv`

This is the key artifact of the feature: `write_distribution_comparison_csv` writes
`<output_stem>_diameter_distribution.csv` beside the packed STL. Full real content from this run:

```
bin,right,target_frequency,target_count,actual_count,actual_frequency,frequency_error,count_error,attempts
5.000000000000,6.000000000000,0.123287671233,5.424657534247,5,0.113636363636,-0.009651307597,-0.424657534247,6
6.000000000000,7.000000000000,0.129243597379,5.686718284693,6,0.136363636364,0.007120038984,0.313281715307,6
7.000000000000,8.000000000000,0.116736152472,5.136390708755,5,0.113636363636,-0.003099788835,-0.136390708755,8
8.000000000000,9.000000000000,0.104228707564,4.586063132817,5,0.113636363636,0.009407656072,0.413936867183,8
9.000000000000,10.000000000000,0.107802263252,4.743299583085,5,0.113636363636,0.005834100384,0.256700416915,7
10.000000000000,11.000000000000,0.088147706968,3.878499106611,4,0.090909090909,0.002761383941,0.121500893389,4
11.000000000000,12.000000000000,0.064919594997,2.856462179869,3,0.068181818182,0.003262223185,0.143537820131,6
12.000000000000,13.000000000000,0.048838594401,2.148898153663,2,0.045454545455,-0.003384048947,-0.148898153663,3
13.000000000000,14.000000000000,0.036926742108,1.624776652770,2,0.045454545455,0.008527803346,0.375223347230,4
14.000000000000,15.000000000000,0.036331149494,1.598570577725,2,0.045454545455,0.009123395961,0.401429422275,2
15.000000000000,16.000000000000,0.029184038118,1.284097677189,1,0.022727272727,-0.006456765391,-0.284097677189,1
16.000000000000,17.000000000000,0.019058963669,0.838594401429,1,0.022727272727,0.003668309058,0.161405598571,1
17.000000000000,18.000000000000,0.015485407981,0.681357951161,1,0.022727272727,0.007241864746,0.318642048839,2
18.000000000000,19.000000000000,0.017272185825,0.759976176295,1,0.022727272727,0.005455086902,0.240023823705,1
19.000000000000,20.000000000000,0.011316259678,0.497915425849,1,0.022727272727,0.011411013049,0.502084574151,1
20.000000000000,21.000000000000,0.009529481834,0.419297200715,0,0.000000000000,-0.009529481834,-0.419297200715,0
21.000000000000,22.000000000000,0.010125074449,0.445503275759,0,0.000000000000,-0.010125074449,-0.445503275759,0
22.000000000000,23.000000000000,0.005955926147,0.262060750447,0,0.000000000000,-0.005955926147,-0.262060750447,0
23.000000000000,24.000000000000,0.005360333532,0.235854675402,0,0.000000000000,-0.005360333532,-0.235854675402,0
24.000000000000,25.000000000000,0.005360333532,0.235854675402,0,0.000000000000,-0.005360333532,-0.235854675402,0
25.000000000000,26.000000000000,0.004169148303,0.183442525313,0,0.000000000000,-0.004169148303,-0.183442525313,0
26.000000000000,27.000000000000,0.004764740917,0.209648600357,0,0.000000000000,-0.004764740917,-0.209648600357,0
27.000000000000,28.000000000000,0.002382370459,0.104824300179,0,0.000000000000,-0.002382370459,-0.104824300179,0
28.000000000000,29.000000000000,0.001786777844,0.078618225134,0,0.000000000000,-0.001786777844,-0.078618225134,0
29.000000000000,30.000000000000,0.001786777844,0.078618225134,0,0.000000000000,-0.001786777844,-0.078618225134,0
```

Each row mirrors one `[Info] Diameter bin N` stdout line, but at full `f64` precision rather than
the truncated `.12328767` seen on the console; `target_count` and `count_error` are the fractional
(non-rounded) ideal count and its actual-vs-ideal difference, useful for downstream analysis where
the integer stdout counts lose precision.

The output STL, `/tmp/rustmspt-doc-examples/pack_target_distribution_result.stl`, is a real binary
STL (841,584 bytes, 16,830 triangles) — smaller in triangle count than the plain-packing example
despite a higher target volume fraction (`0.03` vs `0.02`), because many candidates here were
rescaled *down* toward smaller target bins (minimum scale factor `0.488649`) rather than kept at
natural size.

## Known-good sanity check (from `tests/pack_target_tests.rs`)

Rather than re-deriving expected values from this specific run, `tests/pack_target_tests.rs`
provides deterministic, hand-checkable fixtures for the same algorithm. For example,
`strict_choice_follows_largest_deficit` asserts that for a synthetic three-bin distribution with
frequencies `[0.5, 0.3, 0.2]`, ten sequential `choose_bin`+`record_success` calls (starting from an
empty state) select bins in the exact order `[0, 1, 2, 0, 0, 1, 0, 2, 1, 0]`, ending at accepted
counts `state.counts == vec![5, 3, 2]` — the exact 5:3:2 ratio implied by the frequencies for ten
particles — and that the resulting `summary.max_absolute_error <=
summary.rounding_max_absolute_error + 1e-12`, i.e. the bin-debt algorithm cannot do better than the
integer-rounding floor, and in this case meets it exactly. That same
"`max_absolute_error` at or below the rounding floor" relationship is what this live run's stdout
line (`max_abs 0.01141101` vs `integer_rounding_max 0.01141101` — equal) independently reproduces
on 44 real placed particles, not just the synthetic fixture.

The same test file also asserts, against the real bundled CSV
(`example_paper_distribution_is_valid_and_normalized`), that
`data/input/gu2019_fig7b_pore_distribution.csv` parses to exactly 25 bins spanning `[5, 30]` with
frequencies summing to `1.0` within `1e-12` — the same file and the same 25-bin structure this
walkthrough's live `[Info] Target diameter distribution: 25 bins` line confirms.

## Notes

- Target-distribution steering does not replace or override `target_volume_fraction` — it is a
  secondary, best-effort objective layered on top. When a target bin can't be filled after
  `TARGET_BIN_PROBES = 4` failed probes, the pipeline falls back to whichever bin's debt/TVD score
  is next-best rather than stalling; the run above had `fallback 0`, meaning it never had to invoke
  that escalation at 44 placements, but larger runs or trickier candidate libraries commonly will.
- `filters.min_volume` is checked against the **scaled** candidate after rescaling, not the natural
  volume — a candidate that would pass filters at its original size can still be rejected once
  scaled toward a small target bin, and vice versa.
- Sphericity steering (`target_mean_sphericity`/`mean_sphericity_tolerance`) is soft: it ranks up to
  4 candidate draws per attempt by projected running-mean error but never rejects a candidate for
  being out of range, and a missed tolerance band at the end of the run only produces a `[Warning]`,
  never a hard failure — exactly what this run demonstrated (`0.918922` mean vs. `0.82 ± 0.02`
  target, still completed successfully).
- Only successful placements mutate `DistributionState`/`SphericityState` — failed attempts still
  increment each bin's `attempts` counter (visible in both the stdout table and the CSV's `attempts`
  column) but never `counts`, which is why `attempts` can exceed `actual_count` per bin (e.g. bin 3:
  5 accepted out of 8 attempts).
- See [`../algorithms/packing-target-diameter-distribution.md`](../algorithms/packing-target-diameter-distribution.md)
  for the full bin-debt/TVD-fallback derivation, the `Natural`/`Scaled`/`Fallback` classification
  rules, and the retry-escalation state machine in `PackPipeline::run`; see
  [`../reference/pipeline-packing.md`](../reference/pipeline-packing.md) for the per-function API
  reference of everything in `src/pipeline/pack_targets.rs` and `src/pipeline/pack.rs`; see
  [`../reference/geometry-analysis.md`](../reference/geometry-analysis.md) for `mesh_metrics` and
  `scale_mesh_to_equivalent_diameter`, the equivalent-diameter/sphericity/rescaling primitives this
  feature is built on.
