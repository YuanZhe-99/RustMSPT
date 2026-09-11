# Pipeline: Optimize Reference

This page documents `src/pipeline/optimize.rs` (1141 lines — the heaviest file in the codebase). It implements the particle-rearrangement optimizer: a pre-annealing pruning stage that removes particles to approach a target volume fraction, followed by a simulated-annealing (SA) search — optionally run as multiple parallel "islands" that periodically exchange their best solution — that perturbs particle positions/orientations to match a target two-point correlation function `S2(r)`.

**See also:** [geometry-core.md](geometry-core.md) for `SpatialGrid` and the mesh/bbox/collision primitives used throughout the SA loop. [geometry-analysis.md](geometry-analysis.md) for `calculate_s2`, `l2_norm`, and the GPU S2 pipeline used to score candidate configurations.

## Index

| Item | Location | Summary |
|---|---|---|
| `OptimizePipeline` | `src/pipeline/optimize.rs:27` | Pipeline entry-point struct wrapping the parsed `OptimizationConfig`. |
| `ParticlePrepared` | `src/pipeline/optimize.rs:32` | Per-particle cache of mesh + precomputed bbox + parry3d collision shape. |
| `IslandResult` | `src/pipeline/optimize.rs:39` | Outcome of one SA island run: best particles/loss/S2 plus profiling durations. |
| `GlobalBest` | `src/pipeline/optimize.rs:47` | Cross-island shared best solution, guarded by `Arc<Mutex<GlobalBest>>`. |
| `prepare_particle` | `src/pipeline/optimize.rs:53` | Builds a `ParticlePrepared` (bbox + parry3d shape) from a raw mesh. |
| `format_s2_series` | `src/pipeline/optimize.rs:60` | Formats an S2 vector as a fixed-precision, space-separated string. |
| `push_history_s2` | `src/pipeline/optimize.rs:69` | Appends a labeled S2 snapshot line to the run's history log. |
| `prune_progress_message` | `src/pipeline/optimize.rs:74` | Builds the progress-bar message string for the pruning stage. |
| `selective_prune_to_target_vf` | `src/pipeline/optimize.rs:86` | Pre-annealing stage: iteratively removes particles to approach the target volume fraction while minimizing S2-loss increase. |
| `run_sa_island` | `src/pipeline/optimize.rs:285` | The core simulated-annealing loop for one island; the single most important function in the codebase. |
| `OptimizePipeline::run` | `src/pipeline/optimize.rs:797` | Top-level `Pipeline::run` orchestration: load, target computation, pruning, single/multi-island SA, save. |

---

## Types

#### OptimizePipeline

- **Kind:** `pub struct OptimizePipeline`
- **Source:** `src/pipeline/optimize.rs:27-29`
- **Purpose:** Holds the fully parsed `OptimizationConfig` and implements the `Pipeline` trait via `OptimizePipeline::run`.

| Field | Type | Meaning |
|---|---|---|
| `config` | `OptimizationConfig` | Parsed configuration: input/output paths, target S2 spec, box dimensions, and the `OptimizationParams` block (SA hyperparameters, pruning settings, island-model settings, acceleration mode). |

#### ParticlePrepared

- **Kind:** `struct ParticlePrepared` (derives `Clone`, private to this module)
- **Source:** `src/pipeline/optimize.rs:31-36`
- **Purpose:** Bundles a particle's mesh with precomputed acceleration data so bbox and collision tests inside the hot SA loop avoid recomputing them every iteration.

| Field | Type | Meaning |
|---|---|---|
| `mesh` | `crate::types::Mesh` | The particle's triangle mesh, in world coordinates. |
| `bbox` | `Option<BoundingBox>` | Axis-aligned bounding box, or `None` if the mesh is degenerate (e.g. empty). |
| `shape` | `Option<TriMesh>` | Precomputed `parry3d_f64::shape::TriMesh` used for exact mesh-mesh collision/distance queries via `mesh_collision_exact_prepared` / `mesh_distance_exact_prepared`. |

- **Notes:** Built by `prepare_particle`. Only the one particle being perturbed each iteration is re-prepared (`prepared[idx] = prepare_particle(candidate)`); all others are reused unchanged, which is what makes the per-iteration cost of `run_sa_island` tractable.

#### IslandResult

- **Kind:** `struct IslandResult` (private to this module; `#[allow(dead_code)]` on some fields)
- **Source:** `src/pipeline/optimize.rs:38-45`
- **Purpose:** The value returned by one call to `run_sa_island`: the best solution that island found, plus profiling data for the final timing summary.

| Field | Type | Meaning |
|---|---|---|
| `best_particles` | `Vec<crate::types::Mesh>` | Particle meshes at the best (lowest-loss) configuration seen during the run. |
| `best_loss` | `f64` | L2 loss of `best_particles`' S2 curve against the target. |
| `best_s2` | `Vec<f64>` | The S2 curve corresponding to `best_particles`. |
| `s2_time` | `Duration` | Cumulative wall time spent computing candidate S2 curves. |
| `collision_time` | `Duration` | Cumulative wall time spent on boundary/collision checks. |

- **Notes:** `OptimizePipeline::run` selects the overall winner across islands with `min_by` on `best_loss`, and sums/reports `s2_time`/`collision_time` from the winning island in the final timing summary line.

#### GlobalBest

- **Kind:** `struct GlobalBest` (private to this module)
- **Source:** `src/pipeline/optimize.rs:47-50`
- **Purpose:** The shared state exchanged between islands during migration. Wrapped in `Arc<Mutex<GlobalBest>>` and passed to every `run_sa_island` call as `global_best: Option<&Arc<Mutex<GlobalBest>>>` when `num_islands > 1`.

| Field | Type | Meaning |
|---|---|---|
| `loss` | `f64` | Best loss known across all islands so far (initialized to `f64::MAX`). |
| `particles` | `Vec<crate::types::Mesh>` | Particle configuration achieving that loss. |

- **Notes:** Migration is a compare-and-swap under the mutex: whichever side (the island or the global record) is worse adopts the other's solution. See the "Island migration" subsection of `run_sa_island` below.

---

## Functions

#### prepare_particle

- **Signature:** `fn prepare_particle(mesh: crate::types::Mesh) -> ParticlePrepared`
- **Source:** `src/pipeline/optimize.rs:53`
- **Purpose:** Precompute the bbox and parry3d collision shape for a single particle mesh.
- **Parameters:** `mesh` — the particle's triangle mesh (consumed by value).
- **Returns:** A `ParticlePrepared` wrapping the mesh, its `mesh_bbox`, and its `to_parry_trimesh` shape.
- **Side effects:** None.

#### format_s2_series

- **Signature:** `fn format_s2_series(values: &[f64]) -> String`
- **Source:** `src/pipeline/optimize.rs:60`
- **Purpose:** Render an S2 curve as a human-readable, fixed-precision string for logs.
- **Parameters:** `values` — S2 samples, one per radius bin.
- **Returns:** Space-separated string with each value formatted to 6 decimal places (`{v:.6}`).
- **Side effects:** None.

#### push_history_s2

- **Signature:** `fn push_history_s2(history_log: &mut Vec<String>, label: &str, values: &[f64])`
- **Source:** `src/pipeline/optimize.rs:69`
- **Purpose:** Append a labeled S2 snapshot (e.g. `"Target S2"`, `"Input S2"`, `"Final Best S2"`) to the run's history log, which is ultimately written to `s2_history.txt`.
- **Parameters:** `history_log` — mutable log buffer; `label` — line prefix; `values` — the S2 curve to format via `format_s2_series`.
- **Returns:** Nothing.
- **Side effects:** Pushes one formatted line onto `history_log`.

#### prune_progress_message

- **Signature:** `fn prune_progress_message(current_loss: f64, current_vf: f64, target_vf: f64, particles: usize) -> String`
- **Source:** `src/pipeline/optimize.rs:74`
- **Purpose:** Build the single-line status string shown on the pruning-stage progress bar.
- **Parameters:** `current_loss` — S2 L2 loss at the current particle set; `current_vf` — current volume fraction; `target_vf` — target volume fraction; `particles` — current particle count.
- **Returns:** A string of the form `"Loss {current_loss:.6} | VF {current_vf:.6}->{target_vf:.6} | particles {particles}"`.
- **Side effects:** None.

#### selective_prune_to_target_vf

- **Signature:** `fn selective_prune_to_target_vf(particles: &mut Vec<crate::types::Mesh>, box_bounds: BoundingBox, target_s2: &[f64], params: &crate::config::OptimizationParams, r_max: usize, s2_method: &str, voxel_pitch: f64, thread_pool: &ThreadPool, history_log: &mut Vec<String>)`
- **Source:** `src/pipeline/optimize.rs:86`
- **Purpose:** Pre-annealing pruning stage. Removes particles from the input set, batch by batch, until the volume fraction (VF) is within tolerance of the target VF (`target_s2[0]`, since `S2(0) = VF`), while trying to keep the S2 loss against the target as low as possible along the way.
- **Parameters:**
  - **Input state:** `particles` — mutated in place; the particle set to prune.
  - **Target:** `target_s2` — full target S2 curve (only `target_s2[0]` is used as the VF target).
  - **Config:** `params` — reads `prune_enabled` (default `true`), `prune_tolerance` (default `0.01`), `prune_max_rounds` (default `200`), `prune_eval_samples` (default `max(mc_samples/4, 1000)`), and `mc_samples`.
  - **Box/S2 evaluation:** `box_bounds`, `r_max`, `s2_method`, `voxel_pitch`, `thread_pool` — used to recompute VF and S2 loss after each candidate removal.
  - **Logging:** `history_log` — mutable log buffer.
- **Returns:** Nothing; `particles` is pruned in place.
- **Side effects:** Mutates `particles` and `history_log`; prints `[Info]` progress lines to stdout; drives a progress bar; performs many `calculate_s2` evaluations (this stage can be expensive because each candidate removal is separately scored).
- **Early exits:** Returns immediately (no-op) if `particles.len() < 2`, if `prune_enabled` is `false`, or if the target VF (`target_s2[0]`, clamped to `[0,1]`) is `<= 0.0`.
- **Algorithm:**
  1. Compute the current VF (`volume_fraction_of_meshes_in_bbox`) and current S2 loss (`l2_norm` against `target_s2`, evaluated over `min(r_max, target_s2.len()-1)` radius bins using `prune_eval_samples` Monte Carlo samples — a reduced sample count relative to the main SA loop, since the pruning stage runs many more S2 evaluations per particle removed).
  2. Loop while `current_vf > target_vf * (1.0 + tol)` and there is more than one particle left and the round count is under `prune_max_rounds`:
     - Compute `vf_gap = current_vf - target_vf` and scale the batch size to the gap: `vf_gap > 0.05` → sample 30 candidate particles, remove the best 10; `vf_gap > 0.025` → sample 15, remove the best 3; otherwise → sample 6, remove the best 1. This makes early rounds (far from target) aggressive and later rounds (near target) fine-grained.
     - Randomly sample `n_candidates` distinct particle indices (`rand::seq::index::sample`).
     - For each sampled index, build a temporary particle set with that one particle removed, and evaluate its resulting `(vf, loss)`.
     - Score and sort candidates: candidates whose removal keeps VF **at or above** the target are preferred over candidates that would drop VF below target; within each group, ties are broken by (for "still above target") lowest S2 loss then closest VF-to-target, and (for "drops below target") closest VF-to-target then lowest S2 loss. This means the pruner will not overshoot below the target VF as long as an above-target option exists.
     - Remove the top `k_remove` scored candidates (by index, removed in descending index order to keep remaining indices valid) from `particles`.
     - Recompute `current_vf`/`current_loss` on the now-smaller set, update the progress bar, and log a `"Pruning Round N"` history line every round (with an `[Info]` stdout line every 5 rounds or on convergence).
  3. On loop exit (target reached, round cap hit, or too few particles left), log a `"Pruning Completed"` line with the final round count, particle count, VF, and loss.
- **Notes:** This is a greedy, sampled local-search heuristic, not exact — it never evaluates removing *all* particles, only randomly sampled subsets each round, so it can converge to a locally-good but not globally optimal VF/loss tradeoff. It runs entirely on the CPU thread pool regardless of the configured acceleration backend (no GPU path).

> **Algorithm:** For further context on why pruning precedes annealing and how it interacts with the SA loss landscape, see `../algorithms/simulated-annealing-island-model.md`.

#### run_sa_island

- **Signature:**
  ```rust
  #[allow(clippy::too_many_arguments)]
  fn run_sa_island(
      island_id: usize,
      num_islands: usize,
      prepared_init: Vec<ParticlePrepared>,
      target: &[f64],
      params: &crate::config::OptimizationParams,
      box_bounds: BoundingBox,
      mode: u8,
      d1: f64,
      d2: f64,
      min_neighbor: f64,
      rotation_mode: &RotationMode,
      thread_pool: &ThreadPool,
      global_best: Option<&Arc<Mutex<GlobalBest>>>,
      migration_interval: usize,
      history_log: &mut Vec<String>,
      #[cfg(feature = "gpu")] mut gpu_pipeline: Option<crate::gpu::s2::GpuS2Pipeline>,
  ) -> IslandResult
  ```
- **Source:** `src/pipeline/optimize.rs:285`
- **Purpose:** Run one full simulated-annealing search over particle positions/orientations, perturbing one randomly chosen particle per iteration, to match a target S2 curve. This is the computational core of the entire optimizer and, by far, the most important function in the codebase.
- **Parameters (grouped):**
  - **Input state:** `prepared_init` — initial `ParticlePrepared` set (consumed and mutated as `prepared` across iterations); `target` — target S2 curve.
  - **Config:** `params` — the full `OptimizationParams` block, from which nearly every SA hyperparameter below is read (temperature schedule, move sizes, acceptance-window tuning, S2 sample counts).
  - **Box/boundary:** `box_bounds` — optimization domain; `mode` — boundary mode (`3` enables periodic wrap + ghost-particle collision checks); `d1`, `d2` — boundary distance parameters passed to `check_boundary_constraints_mode`; `min_neighbor` — minimum allowed inter-particle surface gap.
  - **Rotation:** `rotation_mode` — constrains sampled rotation axes (see `pipeline::rotation::RotationMode`).
  - **Execution:** `thread_pool` — rayon pool this island's (CPU) S2 evaluations run on.
  - **Island model:** `island_id`, `num_islands` — identify this island for logging; `global_best` — shared cross-island best (`None` when running a single island); `migration_interval` — iterations between migration attempts.
  - **Logging:** `history_log` — mutable log buffer for S2/loss snapshots.
  - **GPU:** `gpu_pipeline` — `#[cfg(feature = "gpu")]`-gated optional persistent GPU S2 pipeline; when `Some`, S2 is computed on the GPU instead of the CPU.
- **Returns:** `IslandResult` — the best particle configuration/loss/S2 seen, plus cumulative `s2_time` and `collision_time`.
- **Side effects:** Mutates `global_best` through its mutex at migration checkpoints; pushes lines to `history_log` on new-best acceptances and on migration events; prints `[Info]` startup lines and drives a per-iteration progress bar; performs GPU dispatch when `gpu_pipeline` is `Some`.

> **Feature-gated (partial):** The `gpu_pipeline` parameter and every branch that reads it are compiled only under `#[cfg(feature = "gpu")]`; the corresponding `#[cfg(not(feature = "gpu"))]` branches always take the CPU (`thread_pool.install(|| calculate_s2(...))`) path. `run_sa_island` itself compiles and behaves correctly in both configurations — only the GPU S2 code path is conditionally present.

- **Algorithm — setup:**
  - Reads the SA hyperparameters from `params`: `initial_temperature` (floored at `1e-8`), `cooling_rate` (clamped to `[0.8, 0.99999]`), `adaptive_temp_window` (default `clamp(max_iterations/40, 20, 100)`), `target_acceptance_low`/`target_acceptance_high` (defaults `0.20`/`0.45`, with `high` forced above `low`), `adaptive_heat_factor` (default `1.08`), `adaptive_cool_factor` (default `0.94`), `adaptive_temp_ceiling_factor` (default `5.0`, giving `temp_ceiling = initial_temperature * factor`); `temp_floor` is fixed at `1e-9`.
  - Builds the initial merged mesh and computes the starting S2 curve, either via the GPU pipeline (`gpu.update_mesh` + `gpu.calculate_s2_gpu`, with `s2[0]` overwritten by the exact CPU-computed volume fraction) or via CPU `calculate_s2` with `mc_samples.max(2000)` samples. Computes `current_loss = l2_norm(current_s2, target)` and logs it as `"Post-Pruning S2"` / `"Post-Pruning Loss"`.
  - Initializes `best_particles`/`best_loss`/`best_s2` to the starting configuration.
  - Builds an initial `SpatialGrid` over all particle bboxes (`estimate_cell_size` for cell sizing) for neighbor-limited collision queries.
- **Algorithm — per-iteration loop** (up to `params.max_iterations`, one particle perturbed per iteration):
  1. **Pick a particle:** uniformly random index `idx` into `prepared`; clone its mesh as `original`/`candidate`.
  2. **Pick a move type** via a uniform roll, scaled by `scale = clamp(temperature / initial_temperature, 0.05, 1.0)` (so move magnitude shrinks as the system cools):
     - **60% — local translate + rotate:** random translation in `[-max_translation, max_translation] * scale` per axis, applied to the particle's centroid; plus, if `max_rotation_deg.to_radians() * scale > 1e-6`, a random rotation about a sampled axis (`sample_rotation_axis`, respecting `rotation_mode`) by an angle in `[-rot_limit, rot_limit]`.
     - **30% — move toward a random neighbor:** picks another random particle, steps the candidate's centroid toward it by `min(dist * 0.3, max_translation * scale * 2.0)` along the direction vector, plus a smaller random rotation (`±10° * scale`).
     - **10% — full random reposition:** teleports the centroid to a uniformly random point inside `box_bounds`, plus a fully random rotation in `[0, 2π)`.
  3. **Periodic wrap:** if `mode == 3`, wraps the candidate's centroid back into the box (`wrap_mesh_centroid_to_box`).
  4. **Boundary check:** `check_boundary_constraints_mode(candidate, box_bounds, mode, d1, d2)`; if it fails, the move is rejected without ever computing S2 — temperature still cools and the adaptive-window accept-rate counters still advance (see step 8), then `continue`.
  5. **Collision check (own particle):** queries `SpatialGrid` for neighbors within `min_neighbor` margin of the candidate's bbox, falling back to checking every other particle for any missing bbox. For each neighbor, first uses a cheap bbox-distance short-circuit, then exact `mesh_collision_exact_prepared` (parry3d) for true overlap — which since 0.2.1 means the *solids* overlap, so a particle wholly inside another is rejected rather than treated as comfortably clear — and, when `min_neighbor > 0.0`, exact `mesh_distance_exact_prepared` to enforce the minimum gap. An input packing that already contains nested particles will therefore read as blocked from the first sweep; that is the correct reading of an assembly that was never legal. Any overlap or under-gap sets `blocked = true` and aborts the candidate on rejection cooldown (same as step 4).
  6. **Periodic ghost collision (mode 3 only):** if boundary mode is `3`, generates periodic ghost copies of the candidate (`generate_periodic_ghosts`) and repeats the same bbox/exact-collision/min-gap checks for each ghost against all other particles; any ghost collision blocks the move.
  7. **Accept the perturbed particle into `prepared`:** replaces `prepared[idx]` with `prepare_particle(candidate)`, rebuilds the `SpatialGrid`, remerges the mesh, and computes the candidate S2 curve. The Monte Carlo sample count for this evaluation is **iteration/temperature-scaled**: `adaptive_samples = round(mc_samples * (0.3 + 0.7*scale))`, clamped to `[1000, mc_samples.max(1000)]` — hot (early/high-temperature) iterations use fewer samples for speed, cooling iterations use progressively more for precision. S2 is computed via the GPU pipeline if present, else CPU `calculate_s2` on the thread pool. `candidate_loss = l2_norm(candidate_s2, target)`; `delta = candidate_loss - current_loss`.
  8. **Metropolis acceptance:** accept unconditionally if `delta <= 0.0` (candidate is no worse); otherwise accept with probability `exp(-delta / temperature)` (clamped to `[0,1]`) via `rng.gen_bool`. On accept: commit `current_s2`/`current_loss`, and if `current_loss < best_loss`, update `best_particles`/`best_loss`/`best_s2` and log a `"Iter N: Loss ... | S2 ..."` history line. On reject: restore `prepared[idx]` to `original` and rebuild the `SpatialGrid`.
  9. **Temperature update:** every iteration, `temperature *= cooling_rate` (floored at `temp_floor`). Additionally, once every `adaptive_window` iterations (trial/accept counters `window_trials`/`window_accepts`), the *acceptance rate* over that window is compared against `[target_accept_low, target_accept_high]`: below the window → heat up (`temperature *= heat_factor`, capped at `temp_ceiling`); above the window → cool faster (`temperature *= cool_factor`, floored at `temp_floor`); inside the window → leave temperature's exponential-decay trajectory unchanged. This adaptive-window logic is applied identically at every early-`continue` point (boundary reject, collision reject, ghost-collision reject) and at the end of a full iteration, so the acceptance-rate accounting is consistent regardless of how far into an iteration a rejection occurs.
  10. **Island migration** (only when `global_best.is_some()` and `migration_interval > 0` and `(iter+1) % migration_interval == 0`): locks the shared `GlobalBest` mutex and compares `best_loss` against `gb_lock.loss`. If this island is better, it **pushes** its `best_particles`/`best_loss` into the global record. If the global record is better, this island **pulls** the global solution: replaces `best_particles`/`best_loss`, re-derives `prepared` (via `prepare_particle` on each incoming mesh), rebuilds the `SpatialGrid`, recomputes `current_s2`/`current_loss` at full `mc_samples.max(2000)` precision, and logs a `"migrated best loss"` line. This is the mechanism by which islands share progress without ever synchronizing per-iteration state.
  11. Updates the progress bar with current loss, best loss, temperature, and running acceptance percentage. Breaks out of the loop early if `temperature < 1e-9` (effectively frozen).
- **Returns:** an `IslandResult` with the best configuration found and cumulative `s2_time`/`collision_time` profiling durations (measured via `Instant::now()`/`.elapsed()` around the S2-computation and boundary/collision-check code paths respectively).

> **Algorithm:** For the full simulated-annealing schedule, move-selection rationale, and island-model migration design, see `../algorithms/simulated-annealing-island-model.md`.

**See also:** [geometry-core.md](geometry-core.md) documents `SpatialGrid`, `bbox_distance`, `bbox_overlaps`, `mesh_collision_exact_prepared`, `mesh_distance_exact_prepared`, and the mesh transform helpers (`move_mesh_to_target_center`, `rotate_mesh_around_center`, `wrap_mesh_centroid_to_box`, `generate_periodic_ghosts`) used throughout this loop. [geometry-analysis.md](geometry-analysis.md) documents `calculate_s2` and `l2_norm`, and `gpu.md` documents `GpuS2Pipeline` (the `calculate_s2_gpu`/`update_mesh` GPU path used when the `gpu` feature is enabled and an accelerator is selected).

#### OptimizePipeline::run

- **Signature:** `fn run(&self) -> Result<()>` (implementation of the `Pipeline` trait for `OptimizePipeline`)
- **Source:** `src/pipeline/optimize.rs:797`
- **Purpose:** Top-level orchestration of the full optimize pipeline: configuration setup, target-S2 acquisition, pre-annealing pruning, single- or multi-island simulated annealing, and result persistence.
- **Parameters:** `&self` — reads `self.config: OptimizationConfig`.
- **Returns:** `Result<()>` — `Ok(())` on success; `RustMsptError::InvalidConfig`/`InvalidMesh` on configuration or input errors, or propagated I/O errors.
- **Side effects:** Reads STL input from disk (single file or folder); writes the optimized STL to `self.config.output.path`; writes `s2_history.txt` alongside it; prints extensive `[Info]`/`[Warning]` progress and timing output to stdout.
- **Algorithm:**
  1. **Setup:** parses `box_bounds`, boundary mode/params (`mode`, `d1`, `d2`, `min_neighbor`), and `rotation_mode`. Builds a rayon `ThreadPool` sized from `params.cpu_max` (`-1` = use all available cores) clamped to `available_parallelism()`.
  2. **Backend selection:** estimates the voxel grid size from `box_bounds` and `voxel_pitch`, then calls `select_backend` (from `crate::compute::policy`) to decide CPU vs GPU acceleration, logging the requested/effective backend and any fallback reason.
  3. **Load input:** loads STL(s) from `self.config.input.stl_path` (directory → `load_folder_stls`, else `load_stl`), splitting each loaded mesh into individual particles via `split_mesh_into_granules`. Pre-filters out particles whose bbox doesn't overlap `box_bounds` (`bbox_overlaps`), logging how many were removed. Errors if no particles remain.
  4. **Target S2:** resolves the target curve from `self.config.target.r#type`: `"manual_array"` reads `target.s2_array` directly; `"reference_stl"` loads a reference mesh and computes its S2 curve (using `target.stl_bounding_box` if given, else the mesh's own bbox); any other value is a config error.
  5. **Input-stage measurement:** merges the loaded particles, computes their S2 curve and volume fraction, logs the input loss against target, and records `"Target S2"`/`"Input S2"`/`"Input Loss"` history lines.
  6. **Pruning:** calls `selective_prune_to_target_vf` on the particle set (see above) to bring the volume fraction down toward the target before annealing begins.
  7. **Prepare particles:** converts the pruned particle `Vec<Mesh>` into `Vec<ParticlePrepared>` via `prepare_particle`.
  8. **Single vs. multi-island dispatch:** reads `params.islands` (default `1`) and `params.migration_interval` (default `100`).
     - **Single island (`num_islands <= 1`):** optionally initializes a persistent `GpuS2Pipeline` (feature-gated, only if the acceleration mode isn't `Cpu`, with a CPU fallback on GPU init failure), then calls `run_sa_island` once with `global_best = None`.
     - **Multi-island (`num_islands > 1`):** splits `thread_count` evenly across islands (`threads_per_island = max(thread_count / num_islands, 1)`), creates a shared `Arc<Mutex<GlobalBest>>` initialized to `loss: f64::MAX`, and spawns one thread per island inside `std::thread::scope`, each running its own `ThreadPoolBuilder`-built pool, its own optional GPU pipeline instance, and `run_sa_island` with `global_best = Some(&gb)`. Joins all island threads, then picks the overall winner via `min_by` on `best_loss`.
  9. **Post-processing:** logs the final best S2/loss; merges `best_particles` into one mesh; if `params.orient_to_positive_volume` is enabled, calls `orient_components_to_positive_volume` to flip inverted-normal components (logging how many of how many components were flipped); saves the result via `save_stl`.
  10. **History + timing:** writes the accumulated `history_log` to `s2_history.txt` next to the output STL; prints final loss, volume, S2 point count, orientation-fix status, and a timing summary line breaking down total wall time vs. cumulative S2-computation time vs. cumulative collision/constraint-checking time (both sourced from the winning island's `IslandResult`).
- **Notes:** Target S2 acquisition, pruning, and the single-island path all reuse the same top-level `thread_pool`; each island in the multi-island path gets its own independently sized pool instead, so total CPU usage across islands is bounded by `thread_count` (approximately — per-island pools are independent and not globally coordinated beyond the even split).
