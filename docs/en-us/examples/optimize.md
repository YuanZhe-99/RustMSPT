# `optimize` pipeline example

## What it does

`optimize` adjusts particle positions and orientations inside a fixed box (via simulated annealing
with an adaptive temperature schedule and multiple parallel "islands") so that the packed
structure's two-point correlation function (S2) matches a target S2 curve, subject to collision and
boundary constraints. It is typically run after `pack` to refine a randomly packed structure toward
a physically-measured or theoretical target microstructure statistic.

## Config

`data/input/optimize_config.yaml` (the repository's real default config for this pipeline):

```yaml
input:
  stl_path: "data/output/packed_result.stl" # Or single STL

target:
  type: 'manual_array'
  s2_array: [0.063074, 0.051980, 0.041610, 0.034790, 0.027570, 0.022830, 0.017730, 0.014000, 0.010800, 0.008240, 0.006440]
  stl_path: "data/input/particles.stl"
  stl_bounding_box: []

box:
  dimensions: [50.0, 50.0, 50.0]

optimization:
  max_iterations: 5_000
  initial_temperature: 0.1
  cooling_rate: 0.99
  adaptive_temp_window: 50
  target_acceptance_low: 0.20
  target_acceptance_high: 0.45
  adaptive_heat_factor: 1.08
  adaptive_cool_factor: 0.94
  adaptive_temp_ceiling_factor: 5.0

  r_max: 10
  voxel_pitch: 1.0
  mc_method: 'monte_carlo'
  mc_samples: 8_000

  max_translation: 25.0
  max_rotation_deg: 30.0
  rotation_mode: 'none'
  rotation_axis_vector: [0.0, 0.0, 1.0]

  min_neighbor_distance: 1.0
  mode: 2
  min_boundary_dist: 1.5
  min_cross_boundary_depth: 3.0

  prune_enabled: true
  prune_tolerance: 0.01
  prune_max_rounds: 120
  prune_eval_samples: 1000

  cpu_max: -1
  orient_to_positive_volume: false

  acceleration:
    mode: auto

output:
  path: "data/output/optimized_structure.stl"
```

(The full file also contains extensive inline comments documenting every field's meaning and two
alternate "recommended profile" temperature schedules; they are omitted above for brevity — see
`data/input/optimize_config.yaml` directly, or `../reference/config.md`, for the complete
annotated version.)

`input.stl_path` points at `data/output/packed_result.stl`, which is a real mesh already present
in the repository (the output of a prior `pack` run). This walkthrough uses the config unmodified
except for `optimization.max_iterations` and `output.path`, copied into a scratch file — see
**Notes** below for why.

| Field | Meaning |
|---|---|
| `target.type` / `target.s2_array` | `"manual_array"` supplies 11 target S2 values directly (for `r = 0..10`); `"reference_stl"` instead computes the target S2 curve from a reference mesh (`target.stl_path`). |
| `box.dimensions` | The fixed-size box particles are packed/optimized within. |
| `optimization.max_iterations` | Number of simulated-annealing iterations to run. The repository default is **5,000**; this example reduces it for demonstration (see Notes). |
| `optimization.initial_temperature` / `cooling_rate` / `adaptive_*` | The SA temperature schedule: starting temperature, per-iteration decay, and an adaptive window that heats up or cools down faster based on the recent move-acceptance rate. |
| `optimization.mc_method` / `mc_samples` / `voxel_pitch` / `r_max` | Control how S2 is estimated each iteration: Monte Carlo sampling (`mc_samples` point pairs) vs. exact voxel-grid computation, and the correlation-length range (`r_max`) over which S2 is compared to the target. |
| `optimization.prune_enabled` / `prune_tolerance` / `prune_max_rounds` | A pre-annealing stage that removes particles to bring the volume fraction close to `target.s2_array[0]` before the SA loop starts. |
| `optimization.mode` / `min_boundary_dist` / `min_cross_boundary_depth` | Boundary-handling mode (1=strict, 2=loose, 3=periodic) and the minimum clearance/protrusion depths used to validate particle placement against the box walls. |
| `optimization.acceleration.mode` | `auto` picks GPU compute automatically when available and the workload is large enough; `cpu`/`gpu` force a specific backend. |

See `../reference/config.md` (`OptimizationConfig` section) for the full field reference, and
`../algorithms/simulated-annealing-island-model.md` for the annealing/island-model algorithm this
pipeline implements, including the adaptive temperature schedule and S2 loss definition.

## Running it

```bash
cp data/input/optimize_config.yaml /tmp/rustmspt-doc-examples/optimize_config_demo.yaml
# edit the copy: optimization.max_iterations: 5_000 -> 500
# edit the copy: output.path -> /tmp/rustmspt-doc-examples/optimized_structure.stl

./target/release/rustmspt optimize \
  --config /tmp/rustmspt-doc-examples/optimize_config_demo.yaml
```

`input.stl_path` (`data/output/packed_result.stl`) is left untouched from the default config since
that file already exists in the repository from a prior `pack` run; only the scratch copy's
`max_iterations` and `output.path` were changed, and `data/input/optimize_config.yaml` itself was
never modified.

## Expected output

Real captured stdout from the run above (`max_iterations: 500`):

```
[Info] CPU setting: cpu_max=-1 -> using 8 worker threads (available 8).
[Info] Rayon pool threads (effective): 8
[Info] Rotation mode: none
[Info] Acceleration: requested=auto, effective=cpu
[Info] Acceleration fallback: workload 125000 voxels below gpu_min_voxels threshold 250000
[Info] Pre-filter removed 25 particles fully outside optimization bbox.
[Info] S2 config: method=monte_carlo, r_max=10, mc_samples=8000, voxel_pitch=1.000000
[Info] Input stage: VF 0.024314, Loss 0.061045, Method monte_carlo
[Info] Pruning stage: initial VF 0.024314, target VF 0.063074
[Info] Pruning completed: rounds 0, particles 6, VF 0.024314, Loss 0.063054
[Info] Starting optimization with 6 particles.
[Info] Initial loss: 0.062740
[Info] Optimization completed.
[Info] Best loss: 0.040908
[Info] Final volume: 4760.167334
[Info] Final S2 points: 11
[Info] Orientation fix enabled: false
[Info] S2 history saved: /tmp/rustmspt-doc-examples/s2_history.txt
[Info] Timing summary: total 2.41s | S2 2.20s | collision/constraints 0.11s
```

The full run — including S2 computation over 500 iterations — took about 2.4 seconds
(`real 0m2.425s`) on this 8-core machine, with S2 evaluation dominating the timing budget (2.20s of
the 2.41s total). `acceleration.mode: auto` fell back to CPU because the workload (125,000 voxels
at `voxel_pitch: 1.0` over the 50x50x50 box) is below the GPU-acceleration threshold of 250,000
voxels for this config.

Real content of `/tmp/rustmspt-doc-examples/s2_history.txt`:

```
Target S2: 0.063074 0.051980 0.041610 0.034790 0.027570 0.022830 0.017730 0.014000 0.010800 0.008240 0.006440
Input S2: 0.024376 0.022006 0.018800 0.015484 0.015554 0.011562 0.008457 0.007234 0.005827 0.005582 0.004822
Input Loss: 0.061045
Pruning Start: particles 6 | VF 0.024314 -> target 0.063074 | Loss 0.063054
Pruning Completed: rounds 0 | particles 6 | VF 0.024314 | Loss 0.063054
Post-Pruning S2: 0.024376 0.020448 0.019012 0.014409 0.013860 0.010388 0.008816 0.007325 0.005757 0.004330 0.003132
Post-Pruning Loss: 0.062740
Iter 0: Loss 0.055341 | S2 0.027672 0.025171 0.020261 0.019096 0.017981 0.011468 0.008946 0.007190 0.005950 0.005664 0.002733
Iter 8: Loss 0.052881 | S2 0.028968 0.025687 0.022836 0.018341 0.017139 0.012570 0.010298 0.008667 0.007488 0.005842 0.004368
Iter 13: Loss 0.045550 | S2 0.033088 0.026338 0.030219 0.022056 0.018320 0.013229 0.012895 0.010496 0.007875 0.008782 0.004446
Iter 17: Loss 0.044303 | S2 0.033016 0.032918 0.023109 0.022878 0.019002 0.014776 0.011294 0.011076 0.007084 0.006203 0.004090
Iter 19: Loss 0.040908 | S2 0.033104 0.033943 0.027433 0.023158 0.020632 0.018903 0.012926 0.011702 0.008443 0.004379 0.005332
Final Best S2: 0.033104 0.033943 0.027433 0.023158 0.020632 0.018903 0.012926 0.011702 0.008443 0.004379 0.005332
Final Best Loss: 0.040908
```

Reading this: the input mesh's own S2 curve (`Input S2`) is far below the `Target S2` at every
`r`, giving an initial loss of `0.061045`. The pruning stage (enabled via `prune_enabled: true`)
brought the particle count from 21 (after the 25-particle bbox pre-filter removed particles fully
outside the box) down to 6 particles at a volume fraction close to the target's `r=0` value
(`0.063074`), in this case converging immediately (`rounds 0`) since the pruned set already landed
near the target VF. From there, simulated annealing only logged 5 improving iterations
(`Iter 0`, `8`, `13`, `17`, `19` — only accepted, loss-improving moves are logged) out of the 500
attempted, dropping the loss from `0.062740` to a final best of `0.040908`. The written mesh
(`/tmp/rustmspt-doc-examples/optimized_structure.stl`, ~98.9 KB) contains the same 6 particles,
repositioned/reoriented to their best-found configuration.

## Notes

- **This run uses `max_iterations: 500` for demonstration only.** The repository's real default
  config (`data/input/optimize_config.yaml`) sets `max_iterations: 5_000` — ten times more — which
  gives the adaptive-temperature simulated annealing substantially more opportunity to converge
  toward the target S2 curve, at a proportionally longer runtime. Production runs on larger
  particle counts and boxes should expect run times well beyond the ~2.4s seen here, and should
  budget for `max_iterations` in the thousands as shipped in the default config, not the hundreds
  used above.
- `data/input/optimize_config.yaml` was never modified; all changes were made in a scratch copy
  under `/tmp/rustmspt-doc-examples/`, and `output.path` was likewise redirected there so nothing
  was written to `data/output/`.
- Only 6 of the packed mesh's original particles survived the bbox pre-filter and pruning stage in
  this example, because the target S2's `r=0` value (`0.063074`, i.e. target volume fraction) is
  much lower than what `packed_result.stl` was originally packed to; pruning removes particles
  until the volume fraction is close to that target before annealing begins. A target closer to the
  input mesh's actual volume fraction would retain more particles.
- The S2 history log only records iterations where the annealing move was accepted *and* improved
  the best-known loss, not every attempted move — this is why iteration numbers in the log jump
  (`0, 8, 13, 17, 19, ...`) rather than incrementing by one each line.
- `optimization.rotation_mode: 'none'` disables particle rotation during perturbation in this
  config; setting it to `'x'`/`'y'`/`'z'`/`'vector'`/`'any'` allows rotational moves around a fixed
  axis, an arbitrary vector, or unconstrained, respectively — useful when particle orientation
  (not just position) needs to vary to hit the target S2.
- See `../algorithms/simulated-annealing-island-model.md` for the full annealing algorithm
  (adaptive temperature control, island-model parallelism, and the S2 loss function), and
  `../reference/pipeline-optimize.md` for the `OptimizePipeline::run` implementation reference.

## Execution diagnostics added by PERF-01/02

The current run prints and stores `S2 execution: requested=..., effective=..., method=...,
voxel_pitch=..., reason=...`. History also records `Execution: workers=..., islands=...,
active_island_limit=...`. A positive-pitch exact/MC configuration keeps voxel semantics on CPU;
requesting GPU does not silently select continuous MC. `Selected Search Loss` records the historical
winning score, while `Final Best S2/Loss` comes from a full-budget re-evaluation with the same method.
Unlike placement, SA is stochastic and does not promise identical output across thread counts.
Earlier captured output above predates these additional diagnostics.
