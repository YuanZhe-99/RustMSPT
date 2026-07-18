# Simulated Annealing and the Island Model in the Optimize Pipeline

## What this pipeline does

`OptimizePipeline` (`src/pipeline/optimize.rs`) reconstructs a particle
microstructure whose two-point correlation function (S2) matches a target S2
curve. The target either comes directly from a config array
(`target.type = "manual_array"`) or is measured from a reference STL mesh
(`target.type = "reference_stl"`). Starting from an input particle assembly
(loaded from STL and split into individual granules), the pipeline searches
for an arrangement of those particles — positions and orientations — whose S2
curve is as close as possible to the target, subject to boundary and
non-overlap constraints.

The search method is **simulated annealing (SA)**: a stochastic local-search
metaheuristic. At each step it proposes a random perturbation to the current
configuration, evaluates whether the perturbation improves or worsens the fit
to the target S2, and accepts or rejects it using a rule that always accepts
improvements but *sometimes* accepts worsening moves. That willingness to
accept worse solutions — controlled by a temperature parameter that
decreases over the run — lets the search escape local minima early on
(when temperature is high) and converge to a fine-tuned optimum late in the
run (when temperature is low).

Two structural pieces sit around the core SA loop:

- **Pre-annealing pruning** (`selective_prune_to_target_vf`) trims the
  particle set toward the target volume fraction *before* SA starts, because
  removing particles is far cheaper than perturbing them into a matching VF
  through thousands of expensive S2 recomputations.
- **The island model** runs several independent SA searches in parallel
  (`optimization.islands > 1`), each exploring a different region of the
  solution space, with periodic migration of the best solution found so far.

## Pre-annealing pruning: `selective_prune_to_target_vf`

SA's per-iteration cost is dominated by S2 recomputation (a Monte Carlo or
voxelized correlation calculation over the whole assembly). If the loaded
particle set has far more particles than the target volume fraction (VF)
implies — VF is read as `target[0]`, the r=0 point of the target S2 curve,
which equals the target solid fraction — then a large amount of the SA
budget would otherwise be spent gradually thinning the assembly one
accept/reject decision at a time. Pruning does this thinning up front, using
a much cheaper greedy/candidate-scoring strategy instead of full annealing.

The algorithm runs in rounds while the current VF exceeds
`target_vf * (1 + prune_tolerance)`:

1. **Adaptive batch sizing.** The candidate pool size and removal count per
   round scale with how far the current VF is from the target (`vf_gap`):
   - `vf_gap > 0.05`: sample 30 candidates, remove the best 10.
   - `vf_gap > 0.025`: sample 15 candidates, remove the best 3.
   - otherwise: sample 6 candidates, remove the best 1.

   Large gaps call for aggressive batch removal; as the assembly approaches
   the target VF, batches shrink so pruning doesn't overshoot and damage S2
   unnecessarily.

2. **Scoring candidates.** For each of the `n_candidates` randomly sampled
   particle indices, the pruner tentatively removes that one particle,
   recomputes VF and S2 loss (`l2_norm` against the target) for the reduced
   set, and records `(index, still_above_target_vf, loss, vf)`.

3. **Selecting removals.** Candidates are sorted with a two-tier rule:
   candidates that keep VF at or above target are always preferred over ones
   that drop below it (removing too much is worse than removing too little);
   within each tier, ties are broken by lower S2 loss, then by VF proximity
   to the target. The top `k_remove` candidates by this ordering are removed
   together for the round.

4. **Loop control.** This repeats for up to `prune_max_rounds` (default 200)
   rounds, or until the VF tolerance is satisfied. `prune_eval_samples`
   (default `mc_samples / 4`, minimum 1000) controls the Monte Carlo sample
   count used for these intermediate S2 evaluations — deliberately coarser
   than the full-precision S2 used later in SA, since pruning only needs
   enough signal to rank candidates, not publication-quality accuracy.

Pruning can be disabled entirely via `prune_enabled = false`, and is skipped
automatically if there are fewer than 2 particles or the target VF is
non-positive.

## The SA core loop: `run_sa_island`

Each call to `run_sa_island` runs one complete, self-contained simulated
annealing search over the (already pruned) particle set. It returns an
`IslandResult` holding the best particle arrangement found, its loss, its S2
curve, and timing breakdowns for S2 evaluation vs. collision/constraint
checking.

### Initialization

Before the loop starts, the function prepares acceleration structures for
every particle (`ParticlePrepared`: mesh + bounding box + parry3d collision
shape via `prepare_particle`), computes the initial merged-mesh S2 and loss,
and builds a `SpatialGrid` over the particle bounding boxes
(`estimate_cell_size` + `SpatialGrid::build`) for fast neighbor queries
during collision checks. Temperature starts at `initial_temperature`.

### One iteration

Each of the `max_iterations` iterations:

1. **Pick a random particle** (`idx`) to perturb, and save its original mesh
   so the move can be rolled back if rejected.

2. **Propose a move.** A uniform random roll selects one of three move
   types with fixed probabilities:

   - **60% — local translate + rotate.** Random translation within
     `±max_translation` per axis and rotation within `±max_rotation_deg`,
     both scaled by `scale = clamp(temperature / initial_temperature, 0.05, 1.0)`.
     As temperature cools, `scale` shrinks, so these perturbations become
     progressively finer — large exploratory jumps early, small refinements
     late. This is the workhorse move for local fine-tuning of the packing.

   - **30% — move toward a random neighbor.** Picks another random particle,
     computes the direction from the current particle's centroid toward it,
     and steps 30% of the way there (capped at `max_translation * scale * 2.0`),
     plus a small scaled rotation (±10° at full temperature). This biases
     the search toward *clustering* particles together, which is often
     needed to raise short-range correlation and match S2 curves that imply
     particle contact/clustering — a move type pure local jitter would reach
     only slowly.

   - **10% — full random reposition + rotation.** Places the particle at a
     uniformly random position anywhere in the box bounds with a fully
     random rotation. This is the "big jump" move: independent of
     temperature scale, it lets the search escape configurations that local
     and neighbor-directed moves can't get out of, and keeps exploring the
     full solution space even late in the run.

3. **Periodic wrapping.** If boundary `mode == 3` (periodic), the candidate's
   centroid is wrapped back into the box (`wrap_mesh_centroid_to_box`) after
   the move, since periodic mode allows particles to move continuously
   across boundaries.

4. **Constraint checking (cheap rejection before expensive scoring).** Three
   checks happen in order, each able to reject the move immediately —
   before any S2 recomputation, which is the expensive step:

   - `check_boundary_constraints_mode` — box/boundary-depth constraints for
     the configured mode.
   - **Collision checking.** A fresh `SpatialGrid` neighbor query
     (`query_neighbors_with_margin`) narrows candidate neighbors, then exact
     mesh collision (`mesh_collision_exact_prepared`) and, if
     `min_neighbor_distance` is set, exact mesh distance
     (`mesh_distance_exact_prepared`) reject the move if it overlaps another
     particle or violates the minimum-neighbor-distance gap.
   - **Periodic ghost collision** (mode 3 only). Generates periodic ghost
     images of the candidate (`generate_periodic_ghosts`) and runs the same
     collision/distance checks against them, so particles near a periodic
     boundary can't overlap their own or others' wrapped images.

   Any of these failing rejects the move outright: the particle reverts
   implicitly (candidate is simply discarded — `prepared[idx]` is never
   updated), temperature still cools by `cooling_rate`, and the adaptive
   temperature window still counts the trial (as a non-acceptance). The
   loop then `continue`s to the next iteration without ever touching S2.

5. **S2 scoring.** Only if all constraint checks pass: the particle's mesh
   is committed into `prepared[idx]`, the `SpatialGrid` is rebuilt around the
   new configuration, the whole assembly is re-merged, and S2 is recomputed
   for the *candidate* configuration — via the GPU pipeline
   (`gpu.calculate_s2_gpu`) if one was initialized for this island, otherwise
   the CPU path (`calculate_s2` with `s2_method` from `mc_method`). The
   Monte Carlo sample count used for this per-iteration evaluation,
   `iter_samples`, is **scaled by the same `scale` factor used for move
   size**: `((mc_samples as f64) * (0.3 + 0.7 * scale)).round()`, clamped to
   `[1000, mc_samples]`. Concretely: at high temperature (`scale` near 1)
   iterations use close to the full `mc_samples` budget; as temperature cools
   toward zero, iterations use progressively fewer samples, down to 30% of
   `mc_samples`. This is the opposite of "tighter precision as the search
   converges" — the code actually spends *more* Monte Carlo samples (more
   precision) early in the run, when moves are large and coarse noise in the
   S2 estimate matters less to overall exploration but the sample budget
   is being used generously, and pulls back sample count as moves shrink.
   (Verified directly from the `adaptive_samples` formula in `run_sa_island`;
   do not assume the opposite without re-checking this line if the code
   changes.)

   The candidate loss is `l2_norm(candidate_s2, target)`, and
   `delta = candidate_loss - current_loss`.

6. **Metropolis acceptance criterion.**

   ```
   accept = delta <= 0.0
            || rng.gen_bool(exp(-delta / temperature))
   ```

   Improving moves (`delta <= 0`) are always accepted. Worsening moves are
   accepted with probability `exp(-delta / temperature)`: the larger the
   loss increase (`delta`) or the colder the temperature, the smaller this
   probability — accepting worse moves becomes exponentially less likely as
   the run cools, which is the standard Metropolis SA acceptance rule and is
   what lets the search behave like a broad random search early on and a
   strict hill-descending search late on.

   If accepted, the candidate state becomes current; if `current_loss` beats
   `best_loss`, the best-solution snapshot (`best_particles`, `best_loss`,
   `best_s2`) is updated and logged. If rejected, `prepared[idx]` is restored
   to the saved original mesh and the `SpatialGrid` is rebuilt around the
   reverted state.

7. **Cooling and adaptive temperature.** After every iteration (whether
   accepted or not), temperature decays geometrically:
   `temperature = (temperature * cooling_rate).max(temp_floor)`, with
   `cooling_rate` clamped to `[0.8, 0.99999]` and `temp_floor = 1e-9`.

   On top of this steady geometric cooling, the code tracks a rolling
   acceptance window: every iteration increments `window_trials` (and
   `window_accepts` if the move was accepted, including moves rejected by
   constraint checks, which count as trials but never as accepts). Once
   `window_trials` reaches `adaptive_temp_window` (default
   `clamp(max_iterations / 40, 20, 100)`), the window's acceptance rate is
   computed and used to nudge temperature outside the normal cooling
   schedule:

   - if `accept_rate < target_acceptance_low` (default 0.20): temperature is
     *raised* by `adaptive_heat_factor` (default 1.08), capped at
     `temp_ceiling = initial_temperature * adaptive_temp_ceiling_factor`
     (default ceiling factor 5.0) — the search has frozen up (accepting too
     rarely), so temperature is boosted to loosen it back up;
   - if `accept_rate > target_acceptance_high` (default 0.45): temperature is
     *lowered* by `adaptive_cool_factor` (default 0.94), floored at
     `temp_floor` — the search is accepting almost everything (behaving like
     unguided random search), so temperature is cut to sharpen convergence;
   - otherwise temperature is left as-is for this check (the window resets
     either way).

   This checkpoint fires from every rejection branch as well as the main
   accept/reject branch, so `window_trials`/`window_accepts` accumulate
   consistently across constraint rejections, collision rejections, and
   scored accept/reject outcomes. In short: adaptive temperature keeps the
   acceptance rate inside a target band (`[target_acceptance_low,
   target_acceptance_high]`, default `[0.20, 0.45]`) so the search stays
   neither frozen (too cold, rarely accepting) nor purely random (too hot,
   accepting nearly everything), independent of the baseline geometric
   cooling schedule.

8. **Migration** (island model only) — see below.

9. The loop terminates after `max_iterations`, or early if temperature drops
   below `1e-9`.

### Best-solution tracking

`best_particles` / `best_loss` / `best_s2` are updated only when an
*accepted* move's `current_loss` beats the previous best. Because accepted
moves can still be worse than the current state (that's the whole point of
Metropolis acceptance), the current state can wander above the best-known
loss for stretches of the run; the best snapshot is the value ultimately
returned and saved, independent of where the walk ends up.

## The island model

When `optimization.islands` (`OptimizationParams::islands`) is greater than
1, the pipeline runs multiple independent instances of `run_sa_island` in
parallel using `std::thread::scope`, rather than a single search:

- Each island gets its own clone of the initial (post-pruning) particle set,
  its own `RotationMode`, and — critically — **its own temperature,
  cooling schedule, and adaptive-acceptance-window state**. This state is
  never shared across islands; `AGENTS.md` explicitly calls out sharing
  mutable SA state across threads as a pitfall to avoid. Sharing it would
  correlate the islands' search trajectories and defeat the purpose of
  running several independent searches.
- The thread pool's total worker threads are divided evenly across islands
  (`threads_per_island = (thread_count / num_islands).max(1)`), each island
  building its own dedicated `rayon::ThreadPool`.
- If the GPU feature is enabled, each island also gets its own
  `GpuS2Pipeline` instance.
- The only shared state is `global_best: Arc<Mutex<GlobalBest>>`, holding the
  lowest loss and corresponding particle set seen across *all* islands so
  far. Inside `run_sa_island`, every `migration_interval` iterations (default
  100, gated by `(iter + 1) % migration_interval == 0`), each island locks
  the mutex and does one of two things:
  - if its own `best_loss` beats the global best, it **publishes** its best
    solution into `global_best`;
  - otherwise, if the global best beats its own, it **pulls in** the global
    best solution — replacing its own `best_particles`/`best_loss`, rebuilding
    its prepared particles, `SpatialGrid`, merged mesh, and recomputing
    `current_s2`/`current_loss` from the migrated state — so the island's
    *current* walk resumes from the better solution rather than merely
    updating its bookkeeping.

  This is a one-way "adopt the better of mine vs. global" exchange each
  island performs independently, not a broadcast; islands converge toward
  whichever island currently holds the global best, but each keeps its own
  temperature/acceptance trajectory throughout (migration replaces the
  *particle configuration*, not the SA state driving further exploration).

Running several islands in parallel means several independent regions of the
solution space (different sequences of random moves, different local minima
reached) are explored simultaneously instead of just one; periodic migration
lets a good solution found by any one island propagate to the others without
forcing every island to prematurely converge to that island's local optimum
— islands that have wandered elsewhere keep exploring with their own SA
state, only borrowing the *configuration* to resume from a stronger starting
point.

## Top-level orchestration: `OptimizePipeline::run`

1. **Backend and thread setup.** Resolves CPU thread count from `cpu_max`
   (or all available cores) and builds a `rayon::ThreadPool`. Selects the
   compute backend (`select_backend`) based on `acceleration.mode`,
   estimated voxel count, and GPU memory/voxel thresholds.

2. **Load particles.** Loads STL(s) from `input.stl_path` (file or
   directory), splits into individual particle granules
   (`split_mesh_into_granules`), and pre-filters out particles whose bbox
   doesn't overlap the optimization box at all.

3. **Resolve the target S2.** Either taken directly from
   `target.s2_array` (`type = "manual_array"`), or computed by running
   `calculate_s2` on a loaded reference STL against `target.stl_bounding_box`
   or its own bbox (`type = "reference_stl"`).

4. **Baseline logging.** Computes and logs the input assembly's S2, VF, and
   loss against the target before any pruning or SA runs, for the
   `s2_history.txt` trace.

5. **Pruning.** Calls `selective_prune_to_target_vf` on the loaded particle
   set (see above).

6. **Single vs. multi-island dispatch.** Prepares acceleration structures
   for the (pruned) particle set (`prepare_particle`), reads
   `optimization.islands` (default 1) and `optimization.migration_interval`
   (default 100). If `islands <= 1`, runs one `run_sa_island` call directly
   (optionally with a persistent GPU pipeline). If `islands > 1`, spawns one
   thread per island inside `std::thread::scope`, each running its own
   `run_sa_island` with a shared `global_best` handle, then joins all
   threads and picks the island result with the **lowest `best_loss`**
   (`min_by` over `IslandResult::best_loss`).

7. **Output.** Merges the winning island's best particles into one mesh,
   optionally reorients disconnected components to positive signed volume
   (`orient_to_positive_volume`), writes the result to `output.path` as STL,
   and writes the accumulated `s2_history.txt` log (target/input/post-pruning/
   per-improvement/final S2 series and losses) alongside it. Finally prints a
   timing summary breaking total elapsed time into S2-computation time vs.
   collision/constraint-checking time, accumulated from whichever island (or
   the single run) produced the returned result.

## Cross-references

- [pipeline-optimize.md](../reference/pipeline-optimize.md) — per-function
  reference for this pipeline: [`run_sa_island`](../reference/pipeline-optimize.md#run_sa_island),
  [`selective_prune_to_target_vf`](../reference/pipeline-optimize.md#selective_prune_to_target_vf),
  [`OptimizePipeline::run`](../reference/pipeline-optimize.md#optimizepipelinerun).
- [spatial-grid-collision.md](spatial-grid-collision.md) — the `SpatialGrid`
  broad-phase structure and periodic ghost-image collision checking used
  during move validation in `run_sa_island`.
- [s2-two-point-correlation.md](s2-two-point-correlation.md) — how
  `calculate_s2` and the GPU S2 pipeline compute the correlation curve that
  SA's loss function (`l2_norm`) compares against the target.
