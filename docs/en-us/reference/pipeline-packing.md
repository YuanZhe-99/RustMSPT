# Pipeline: Packing

Sequential random-packing pipeline (`src/pipeline/pack.rs`) and its target-diameter-distribution /
mean-sphericity steering engine (`src/pipeline/pack_targets.rs`).

## Index

| Item | Location | Summary |
|---|---|---|
| `TARGET_BIN_PROBES` | `src/pipeline/pack.rs:27` | Max consecutive placement failures tolerated for a chosen bin before it is excluded from this round's re-selection. |
| `PackPipeline` | `src/pipeline/pack.rs:95` | Pipeline struct wrapping a `PackingConfig`; implements `Pipeline`. |
| `CandidateProposal` | `src/pipeline/pack.rs:34` | One drawn candidate mesh plus its optional precomputed `MeshMetrics`. |
| `validate_sphericity_target` | `src/pipeline/pack.rs:110` | Validates `target_mean_sphericity`/`mean_sphericity_tolerance` config before packing starts. |
| `check_geometry_filters` | `src/pipeline/pack.rs:141` | Applies configured `min_volume`, `max_aspect_ratio`, `max_sharpness_ratio` filters to a candidate mesh. |
| `PackPipeline::run` | `src/pipeline/pack.rs:124` | Core sequential random packing loop with optional target-diameter-distribution and mean-sphericity steering. |
| `DiameterBin` | `src/pipeline/pack_targets.rs:8` | Half-open (closed at the final bin) diameter interval with a target frequency. |
| `DiameterBin::midpoint` | `src/pipeline/pack_targets.rs:16` | Arithmetic midpoint of the interval. |
| `TargetDistribution` | `src/pipeline/pack_targets.rs:22` | Parsed, normalized target diameter distribution (ordered, non-overlapping bins). |
| `DistributionState` | `src/pipeline/pack_targets.rs:27` | Mutable per-run counters tracked against a `TargetDistribution`. |
| `BinChoiceKind` | `src/pipeline/pack_targets.rs:40` | `Natural` / `Scaled` / `Fallback` classification of a bin choice. |
| `BinChoice` | `src/pipeline/pack_targets.rs:47` | Chosen bin index plus its `BinChoiceKind`. |
| `DistributionSummary` | `src/pipeline/pack_targets.rs:53` | Aggregate error metrics computed at the end of a run. |
| `SphericityState` | `src/pipeline/pack_targets.rs:61` | Running sum/count of accepted sphericity values. |
| `TargetDistribution::state` | `src/pipeline/pack_targets.rs:68` | Builds a zeroed `DistributionState` sized to the bins. |
| `TargetDistribution::bin_for_diameter` | `src/pipeline/pack_targets.rs:81` | Maps a diameter to its containing bin index. |
| `TargetDistribution::choose_bin` | `src/pipeline/pack_targets.rs:98` | Core allocation heuristic: picks the bin with greatest debt, falling back to minimum-TVD choice. |
| `TargetDistribution::record_attempt` | `src/pipeline/pack_targets.rs:165` | Increments the attempt counter for a bin. |
| `TargetDistribution::record_success` | `src/pipeline/pack_targets.rs:176` | Commits a successful placement's bin, kind, and scale factor. |
| `TargetDistribution::summarize` | `src/pipeline/pack_targets.rs:212` | Computes max absolute error, total variation distance, and best-achievable rounding error. |
| `SphericityState::mean` | `src/pipeline/pack_targets.rs:259` | Accepted arithmetic mean sphericity. |
| `SphericityState::projected_error` | `src/pipeline/pack_targets.rs:272` | Distance of the mean-if-accepted from the target band; used to rank/steer candidates. |
| `SphericityState::record_success` | `src/pipeline/pack_targets.rs:298` | Commits a candidate's sphericity into the running mean. |
| `load_target_distribution_csv` | `src/pipeline/pack_targets.rs:309` | Parses and validates a `bin[,right],frequency` CSV into a `TargetDistribution`. |
| `write_distribution_comparison_csv` | `src/pipeline/pack_targets.rs:495` | Writes the `<output_stem>_diameter_distribution.csv` target-vs-actual report. |
| `parse_csv_f64` | `src/pipeline/pack_targets.rs:564` | Parses a required finite CSV float cell with row/column error context. |
| `parse_optional_csv_f64` | `src/pipeline/pack_targets.rs:587` | Parses an optional CSV float cell where a blank means "absent". |
| `PackCollider` | `src/pipeline/pack.rs:27` | Cached collider bbox and shape. |
| `PackCollider::new` | `src/pipeline/pack.rs:34` | Prepare collision shape once. |
| `PackCollider::blocks` | `src/pipeline/pack.rs:42` | Cached overlap or clearance predicate. |
| `pack_blocked` | `src/pipeline/pack.rs:68` | Check incremental spatial candidates. |
| `PackPipeline::run_in_pool` | `src/pipeline/pack.rs:221` | Packing work under configured pool. |

**See also:** [geometry-analysis.md](geometry-analysis.md) for `MeshMetrics` and `scale_mesh_to_equivalent_diameter`, used throughout this pipeline.

---

## `src/pipeline/pack.rs`

#### TARGET_BIN_PROBES

- **Signature:** `const TARGET_BIN_PROBES: usize = 4`
- **Source:** `src/pipeline/pack.rs:27`
- **Purpose:** Caps the number of consecutive placement failures tolerated for a single chosen target bin within one packing round before `choose_bin` excludes it from further consideration in that round.
- **Notes:** Consulted inside `PackPipeline::run`'s `reject_candidate!()` macro: a bin's failure counter is incremented on rejection, and only once it reaches `TARGET_BIN_PROBES` does the bin stay in `attempted_bins` (permanently excluded for the round); below that threshold it is removed from `attempted_bins` so it can be retried.

#### PackPipeline

- **Kind:** struct
- **Source:** `src/pipeline/pack.rs:95`
- **Fields:**

| Field | Type | Description |
|---|---|---|
| `config` | `PackingConfig` | Full packing configuration (input/output/box/packing settings, filters, target distribution, sphericity target). |

- **Purpose:** Implements the `Pipeline` trait for the `pack` command; `run` is the entire packing algorithm.

#### CandidateProposal

- **Kind:** struct (private, `Debug, Clone`)
- **Source:** `src/pipeline/pack.rs:34`
- **Fields:**

| Field | Type | Description |
|---|---|---|
| `mesh` | `Mesh` | The drawn candidate mesh, prior to any target-bin rescaling, rotation, or placement. |
| `metrics` | `Option<MeshMetrics>` | Precomputed metrics (volume, surface area, equivalent diameter, sphericity), present only when a target distribution or sphericity target is active. |

- **Purpose:** Bundles a proposal mesh with its metrics so multiple draws per attempt can be ranked before any expensive placement/collision work.

#### validate_sphericity_target

- **Signature:** `fn validate_sphericity_target(config: &PackingConfig) -> Result<Option<(f64, Option<f64>)>>`
- **Source:** `src/pipeline/pack.rs:110`
- **Purpose:** Validates the optional `packing.target_mean_sphericity` / `packing.mean_sphericity_tolerance` configuration before any packing work begins.
- **Parameters:** `config` — the packing configuration.
- **Returns:** `Ok(Some((target, tolerance)))` when a target is configured; `Ok(None)` when neither field is set; `Err(InvalidConfig)` when `target_mean_sphericity` is outside `(0, 1]`, `mean_sphericity_tolerance` is outside `[0, 1]`, or a tolerance is given without a target.
- **Side effects:** None.

#### check_geometry_filters

- **Signature:** `fn check_geometry_filters(mesh: &Mesh, config: &PackingConfig, check_min_volume: bool) -> bool`
- **Source:** `src/pipeline/pack.rs:141`
- **Purpose:** Validates a candidate mesh against the configured `packing.filters` (`min_volume`, `max_aspect_ratio`, `max_sharpness_ratio`).
- **Parameters:**
  - `mesh` — candidate mesh to check.
  - `config` — packing configuration holding the optional `filters` block.
  - `check_min_volume` — whether the `min_volume` filter is enforced for this call; callers skip it on the *source* mesh when a target diameter distribution is active (since that mesh will be rescaled before its final volume is meaningful) and re-enforce it (`true`) once the candidate is at its final scale.
- **Returns:** `true` if all enabled filters pass (or no filters are configured); `false` on the first failing filter.
- **Side effects:** None.
- **Notes:** Aspect ratio is `max_extent / min_extent` of the mesh's bounding box (with `min_extent` floored at `1e-12`). Sharpness uses the isoperimetric-style quantity `log_sharpness = 3*ln(area) - ln(36*pi) - 2*ln(volume)`, compared against `ln(max_sharpness_ratio)`; non-finite volume/area/log_sharpness or non-positive inputs fail the filter.

#### PackPipeline::run

- **Signature:** `fn run(&self) -> Result<()>` (impl of `Pipeline for PackPipeline`)
- **Source:** `src/pipeline/pack.rs:124`
- **Purpose:** Executes the full sequential random-packing algorithm: draws particle candidates, optionally steers their diameter distribution and mean sphericity toward configured targets, places them without collision inside the packing box until the target volume fraction or attempt budget is reached, then saves the result and reports statistics.
- **Parameters:** none beyond `&self` (uses `self.config`).
- **Returns:** `Ok(())` on success; `Err(InvalidConfig)` for a non-positive box volume, invalid sphericity target config, or thread-pool build failure; `Err(InvalidMesh)` when no candidate mesh/STL is found or no particle could be placed at all.
- **Side effects:** Reads the input STL file or directory and the optional target-diameter CSV from disk; builds a rayon thread pool; writes the packed STL to `config.output.path`; optionally writes `<output_stem>_diameter_distribution.csv`; prints `[Info]`/`[Warning]` progress and summary lines to stdout; renders a progress bar via `create_progress_bar`.

**Walkthrough:**

1. **Setup and validation.** Parses `box.dimensions` into `box_bounds` and rejects non-positive volume. Calls `validate_sphericity_target`. If `packing.target_diameter_distribution_csv` is set, loads it via `load_target_distribution_csv` and prints the bin count; otherwise `target_distribution` is `None`.
2. **Input loading — two modes:**
   - **Lazy directory mode** (`input.path` is a directory): indexes every `.stl` file path into `lazy_files` without loading mesh data; each packing attempt loads one file on demand via `load_stl`. Suited to large particle libraries that don't fit in memory at once.
   - **Preloaded single-STL mode** (`input.path` is a file): loads the STL once, splits it into granules via `split_mesh_into_granules`, and keeps every granule passing `check_geometry_filters` (with `check_min_volume = target_distribution.is_none()`) in `preloaded_pool`.
   Errors if the resolved mode yields zero candidates/files.
3. **Thread pool.** Builds a dedicated rayon `ThreadPool` sized from `packing.cpu_max` (`-1` means all available cores, otherwise clamped to `[1, available_cores]`); this pool is used for all parallel collision/distance checks (via `thread_pool.install(...)`), keeping them independent of the global rayon pool.
4. **Main loop** — `while (current_volume / box_volume) < target && attempts < max_attempts`:
   - **Proposal draws.** Draws 1 candidate normally, or 4 (`proposal_draws`) when a sphericity target is active, from the lazy file list or the preloaded pool. When target metrics are needed (target distribution or sphericity target active), computes `mesh_metrics` for each draw and skips draws that fail metric computation. When a sphericity target is active, each draw's rank key is `SphericityState::projected_error` (candidates that would push the projected mean sphericity outside/further from the target band rank worse); otherwise the rank key is `0.0` (order-preserving). Proposals are sorted ascending by this error so the most favorable candidate is tried first.
   - **Placement attempt loop** (inner `loop`), for each proposal in rotation (`proposal_index % proposals.len()`, or, when a target distribution is active, continuing indefinitely by cycling through proposals until a bin choice is exhausted or `max_attempts` is hit):
     - If a target distribution is active: computes the candidate's *natural* bin from its as-drawn equivalent diameter (`bin_for_diameter`), then calls `TargetDistribution::choose_bin` to pick the bin to *target* for this placement (which may differ from the natural bin — see `choose_bin` below). `record_attempt` is called immediately. If the chosen bin's kind is not `Natural`, the mesh is rescaled to that bin's midpoint via `scale_mesh_to_equivalent_diameter`, re-measured, and rejected if the rescaled diameter no longer maps back into the chosen bin (guards against rescaling failures near boundaries). When a target distribution is active, `check_geometry_filters` is re-run with `check_min_volume = true` on the (possibly rescaled) candidate.
     - Applies a random rotation (via `sample_rotation_axis`/`rotate_mesh_around_center`, respecting `rotation_mode`) and a uniformly random position within `box_bounds` (`move_mesh_to_target_center`).
     - Rejects candidates failing `check_boundary_constraints_mode` (mode-specific boundary rules, `min_boundary_dist`, `min_cross_boundary_depth`).
     - Builds a `collision_set` = all placed meshes, extended with `generate_periodic_ghosts` of each placed mesh when `packing.mode == 3` (periodic boundary mode). Runs a parallel (`thread_pool.install`) bounding-box-pruned exact collision check (`mesh_collision_exact`); when `min_neighbor_distance` is set, bbox-distance pruning uses that threshold instead of plain overlap. Since 0.2.1 that check also rejects a candidate that lies wholly inside a placed particle, or would swallow one — two closed surfaces in that arrangement never cross, so through v0.2.0 this loop accepted them and `min_neighbor_distance` read the space between the two surfaces as clearance. See [spatial-grid-collision.md](../algorithms/spatial-grid-collision.md#why-nesting-needs-its-own-test).
     - If `min_neighbor_distance > 0`, runs a second parallel pass computing the minimum exact surface distance (`mesh_distance_exact`) to any existing mesh, rejecting if it undercuts the threshold.
     - In mode 3, additionally checks the candidate's own periodic ghosts against the collision set and neighbor-distance threshold.
     - Computes `particle_volume_in_bbox` (the clipped in-box volume) and rejects non-finite/non-positive results.
     - **On success:** records the bin choice (`record_success`, including the scale factor if rescaled) and sphericity (`SphericityState::record_success`) when applicable, adds the clipped volume to `current_volume`, pushes the mesh to `placed`, resets `attempts` to 0, and breaks out to continue the outer loop.
     - **On any rejection:** the `reject_candidate!()` macro increments `attempts`, updates the progress bar, and (if a bin was chosen) increments that bin's failure counter, removing it from `attempted_bins` only if the counter is still below `TARGET_BIN_PROBES` (so a bin gets several tries before it's abandoned for the round).
   - After the inner loop: if no placement succeeded and a target distribution was active with a bin choice made (`had_bin_choice`) and `attempts` has hit `max_attempts`, the outer loop breaks entirely (packing stops). If a target distribution is active but `choose_bin` returned `None` immediately (`!had_bin_choice`, i.e. no bin could be selected at all), `attempts` is bumped once and the loop continues (allows redrawing fresh proposals).
5. **Finalization.** Errors if `placed` is empty. Merges all placed meshes (`merge_meshes`), optionally applies `orient_components_to_positive_volume` when `packing.orient_to_positive_volume` is set, and saves the result via `save_stl`. Prints final count and volume fraction, warning if the target volume fraction wasn't reached. If a target distribution was used, calls `summarize`, writes the comparison CSV via `write_distribution_comparison_csv`, and prints a full per-bin table (left, right, target frequency, ideal count, actual count, actual frequency, error, attempts) plus aggregate errors, natural/scaled/fallback counts, and scale-factor min/mean/max; warns if any fallback placements occurred (distribution was relaxed to prioritize volume fraction). If a sphericity target was used, prints the final mean sphericity and error, and whether the tolerance was met (warning if not). Always prints whether orientation-fixing was enabled and, if so, how many components were flipped.

- **Notes:** Supports lazy directory loading for datasets too large to preload. Target-bin failures always fall back toward whichever bin remains placeable, so overall volume fraction is prioritized over exact distribution fidelity. Mode 3 (periodic boundary) adds ghost-particle collision checks on both sides (existing particles' ghosts vs. candidate, and candidate's ghosts vs. existing particles). All collision/distance computations run on the pipeline's dedicated rayon thread pool sized from `cpu_max`.

---

## `src/pipeline/pack_targets.rs`

> **Algorithm:** For a full walkthrough of the bin-debt allocation heuristic and the CSV distribution format with worked examples, see `../algorithms/packing-target-diameter-distribution.md`.

> **See also:** [geometry-analysis.md](geometry-analysis.md) documents `MeshMetrics` (the `volume`/`surface_area`/`equivalent_diameter`/`sphericity` struct consumed throughout this file) and `scale_mesh_to_equivalent_diameter`, which `PackPipeline::run` calls to rescale a candidate toward a chosen bin's midpoint.

### Types

#### DiameterBin

- **Kind:** struct (`Debug, Clone, Copy, PartialEq`)
- **Source:** `src/pipeline/pack_targets.rs:8`
- **Fields:**

| Field | Type | Description |
|---|---|---|
| `left` | `f64` | Inclusive lower bound of the diameter interval. |
| `right` | `f64` | Upper bound; exclusive except for the final bin in a `TargetDistribution`, which is inclusive. |
| `frequency` | `f64` | Target fraction of placements that should fall in this bin, normalized so all bins sum to 1.0. |

#### DiameterBin::midpoint

- **Signature:** `pub fn midpoint(&self) -> f64`
- **Source:** `src/pipeline/pack_targets.rs:16`
- **Purpose:** Returns `left + (right - left) * 0.5`, the diameter used as the rescale target when a candidate is assigned to this bin.
- **Side effects:** None.

#### TargetDistribution

- **Kind:** struct (`Debug, Clone`)
- **Source:** `src/pipeline/pack_targets.rs:22`
- **Fields:**

| Field | Type | Description |
|---|---|---|
| `bins` | `Vec<DiameterBin>` | Strictly ordered, non-overlapping bins, normalized so frequencies sum to 1.0. Produced by `load_target_distribution_csv`. |

#### DistributionState

- **Kind:** struct (`Debug, Clone, Default`)
- **Source:** `src/pipeline/pack_targets.rs:27`
- **Fields:**

| Field | Type | Description |
|---|---|---|
| `counts` | `Vec<usize>` | Accepted placement count per bin, indexed like `TargetDistribution::bins`. |
| `attempts` | `Vec<usize>` | Total attempts (successful or not) recorded per bin. |
| `natural` | `usize` | Count of successes whose kind was `Natural` (candidate's as-drawn diameter already matched the chosen bin). |
| `scaled` | `usize` | Count of successes whose kind was `Scaled` (candidate was rescaled to a debt-driven bin, on the first pass of a round). |
| `fallback` | `usize` | Count of successes whose kind was `Fallback` (chosen on a retry pass, after at least one earlier bin choice failed to place in this round, or via the minimum-TVD tie-break). |
| `scale_factor_min` / `scale_factor_max` / `scale_factor_sum` / `scale_factor_count` | `f64`/`f64`/`f64`/`usize` | Running min, max, sum, and count of positive finite scale factors applied across successful rescaled placements (used to report mean/min/max scale factor). |

#### BinChoiceKind

- **Kind:** enum (`Debug, Clone, Copy, PartialEq, Eq`)
- **Source:** `src/pipeline/pack_targets.rs:40`
- **Variants:**
  - `Natural` — the debt-driven bin choice coincided with the candidate's own as-drawn diameter bin; no rescale is applied.
  - `Scaled` — the debt-driven bin choice differs from the natural bin (or there was no natural bin), on the first (non-relaxed) selection pass; the candidate is rescaled to the bin's midpoint.
  - `Fallback` — the choice was made after `attempted` already contained at least one earlier failed bin for this round (relaxed selection), or via the minimum-total-variation-distance tie-break used when no bin currently has positive debt.

#### BinChoice

- **Kind:** struct (`Debug, Clone, Copy, PartialEq`)
- **Source:** `src/pipeline/pack_targets.rs:47`
- **Fields:** `index: usize` (chosen bin), `kind: BinChoiceKind`.

#### DistributionSummary

- **Kind:** struct (`Debug, Clone, Copy, PartialEq, Default`)
- **Source:** `src/pipeline/pack_targets.rs:53`
- **Fields:**

| Field | Type | Description |
|---|---|---|
| `count` | `usize` | Total accepted placements across all bins. |
| `max_absolute_error` | `f64` | Largest per-bin `\|observed_frequency - target_frequency\|`. |
| `total_variation_distance` | `f64` | `0.5 * sum(\|observed_frequency - target_frequency\|)` over all bins — the standard TVD between the observed and target distributions. |
| `rounding_max_absolute_error` | `f64` | Best-achievable max absolute error given `count` is an integer, computed via largest-remainder (Hamilton) rounding of the target frequencies — a lower bound the actual placement error can never beat for that `count`. |

#### SphericityState

- **Kind:** struct (`Debug, Clone, Default`)
- **Source:** `src/pipeline/pack_targets.rs:61`
- **Fields:** `sum: f64` (sum of accepted sphericity values), `count: usize` (number of accepted values).

### impl TargetDistribution

#### TargetDistribution::state

- **Signature:** `pub fn state(&self) -> DistributionState`
- **Source:** `src/pipeline/pack_targets.rs:68`
- **Purpose:** Builds a fresh `DistributionState` with `counts` and `attempts` zeroed and sized to `self.bins.len()`; all other fields use `Default`.
- **Side effects:** None.

#### TargetDistribution::bin_for_diameter

- **Signature:** `pub fn bin_for_diameter(&self, diameter: f64) -> Option<usize>`
- **Source:** `src/pipeline/pack_targets.rs:81`
- **Purpose:** Finds the index of the bin containing a given equivalent diameter.
- **Parameters:** `diameter` — candidate equivalent diameter.
- **Returns:** `None` if `diameter` is non-finite or falls outside every bin (including past the final bin's right edge, or in an explicit gap between non-adjacent bins). Otherwise the matching bin index.
- **Side effects:** None.
- **Notes:** Intervals are `[left, right)` except the very last bin, which is closed (`[left, right]`), confirmed by `tests/pack_target_tests.rs::classifies_shared_edges_and_final_right` (diameter exactly equal to the last bin's `right` maps to that bin; one ULP past it maps to `None`).

#### TargetDistribution::choose_bin

- **Signature:** `pub fn choose_bin(&self, state: &DistributionState, natural_bin: Option<usize>, attempted: &mut BTreeSet<usize>) -> Option<BinChoice>`
- **Source:** `src/pipeline/pack_targets.rs:98`
- **Purpose:** Selects which bin the next candidate placement should be steered toward, prioritizing bins with the greatest "debt" against their target frequency, and falling back to a minimum-total-variation-distance choice once no bin has positive debt.
- **Parameters:**
  - `state` — current `DistributionState` (read-only; not mutated by this call).
  - `natural_bin` — the bin the candidate would fall in without rescaling, used only to classify the choice as `Natural` vs `Scaled`.
  - `attempted` — mutable set of bin indices already tried (and failed to place) in the current round; bins in this set are skipped in the debt search, and the chosen bin's index is inserted into it before returning.
- **Returns:** `Some(BinChoice)` naming the selected bin and its kind, or `None` if every bin has zero/negative frequency or is already in `attempted`.
- **Side effects:** None on `self`/`state`; mutates `attempted` (inserts the chosen index) so the caller's retry loop won't pick the same bin again this round.
- **Notes:**
  - **Debt formula.** For each bin not yet in `attempted` with `frequency > 0`, computes `debt = frequency * next_count - counts[index]`, where `next_count = sum(counts) + 1` is the total accepted count *if this next placement succeeds*. This is the bin's ideal count at that future total minus its currently observed count — i.e., how far behind its target share the bin currently sits. The bin with the largest debt strictly greater than `1e-12` is selected (ties broken by iteration order, i.e. lowest index first, since `>` requires strict improvement).
  - **Natural vs. Scaled vs. Fallback classification** (first pass, `attempted` empty at call time): if a debt-positive bin is found, the choice is `Fallback` if `attempted` was already non-empty when the call started (meaning at least one earlier bin failed this round — a *relaxed* re-selection), otherwise `Natural` if the chosen index equals `natural_bin`, otherwise `Scaled`. In other words, `Natural`/`Scaled` only ever occur on the *first* selection attempt of a round; every subsequent retry within the same round is `Fallback`, even if it lands back on a debt-positive bin (confirmed by `tests/pack_target_tests.rs::attempted_deficit_bin_falls_back_to_other_bin`).
  - **Fallback-to-minimum-TVD.** If no bin has positive debt (e.g., all target-eligible bins are already saturated or in `attempted`), the method instead evaluates, for every remaining eligible bin, the total variation distance the *overall* distribution would have if that bin's count were incremented by one (`variation = sum over all bins of |observed_with_increment/accepted_count - target_frequency|`). It picks the bin minimizing this projected TVD, breaking ties by smaller midpoint (i.e., preferring smaller diameters). This choice is always classified `Fallback`.
  - Confirmed by `tests/pack_target_tests.rs::strict_choice_follows_largest_deficit`, which drives a 3-bin (0.5/0.3/0.2) distribution through 10 sequential calls and asserts the exact deficit-driven index sequence `[0,1,2,0,0,1,0,2,1,0]`, all classified `Scaled` (each call is the *first* attempt of its own round, with a fresh empty `attempted` set), ending at `counts == [5,3,2]`.

#### TargetDistribution::record_attempt

- **Signature:** `pub fn record_attempt(&self, state: &mut DistributionState, index: usize)`
- **Source:** `src/pipeline/pack_targets.rs:165`
- **Purpose:** Increments `state.attempts[index]` for the given bin, if `index` is in range.
- **Side effects:** Mutates `state.attempts`.

#### TargetDistribution::record_success

- **Signature:** `pub fn record_success(&self, state: &mut DistributionState, index: usize, kind: BinChoiceKind, scale_factor: Option<f64>)`
- **Source:** `src/pipeline/pack_targets.rs:176`
- **Purpose:** Commits one successful placement into the bin's accepted count and updates aggregate kind/scale-factor statistics.
- **Parameters:** `state` (mutated); `index` — bin that received the placement; `kind` — `Natural`/`Scaled`/`Fallback`, tallied into the matching `state` field; `scale_factor` — the mesh rescale factor applied (if any), folded into `scale_factor_min`/`max`/`sum`/`count` when finite and positive.
- **Returns:** none (no-op if `index >= state.counts.len()`).
- **Side effects:** Mutates `state.counts`, one of `state.natural`/`scaled`/`fallback`, and the scale-factor aggregates.

#### TargetDistribution::summarize

- **Signature:** `pub fn summarize(&self, state: &DistributionState) -> DistributionSummary`
- **Source:** `src/pipeline/pack_targets.rs:212`
- **Purpose:** Computes final distribution-fidelity metrics from the accepted counts.
- **Parameters:** `state` — final `DistributionState`.
- **Returns:** `DistributionSummary::default()` (all zero) if no placements were accepted; otherwise a populated summary.
- **Side effects:** None.
- **Notes:** `rounding_max_absolute_error` is computed via a **largest-remainder allocation**: each bin's ideal integer target is `floor(frequency * count)`, then the `count - sum(floor(...))` leftover units are handed out one each to the bins with the largest fractional remainder (`frequency * count - floor(frequency * count)`), ties broken by lower bin index. This gives the best possible per-bin integer allocation for that exact `count`, and `rounding_max_absolute_error` is its max absolute error against the target frequencies — a floor that `max_absolute_error` (the *actual* achieved error) can approach but not usually beat, since achieving it exactly would require every placement to land in its debt-optimal bin. `total_variation_distance` uses the standard `0.5 * sum(|observed - target|)` normalization (bounded in `[0, 1]`). Confirmed by `tests/pack_target_tests.rs::integer_rounding_baseline_preserves_total_count` and `strict_choice_follows_largest_deficit` (which asserts `max_absolute_error <= rounding_max_absolute_error + 1e-12`, i.e. the debt-driven heuristic achieves error at or below the theoretical rounding floor).

### impl SphericityState

#### SphericityState::mean

- **Signature:** `pub fn mean(&self) -> Option<f64>`
- **Source:** `src/pipeline/pack_targets.rs:259`
- **Purpose:** Returns the arithmetic mean sphericity of all accepted placements, or `None` if none have been accepted yet.
- **Side effects:** None.

#### SphericityState::projected_error

- **Signature:** `pub fn projected_error(&self, metrics: MeshMetrics, target: f64, tolerance: Option<f64>) -> Option<f64>`
- **Source:** `src/pipeline/pack_targets.rs:272`
- **Purpose:** Scores a candidate by how far the accepted mean sphericity *would* land if this candidate were accepted next, relative to a target value or tolerance band — used by `PackPipeline::run` to rank multiple candidate draws and prefer whichever one steers the running mean toward the target.
- **Parameters:** `metrics` — candidate's `MeshMetrics` (only `sphericity` is used); `target` — target mean sphericity; `tolerance` — optional absolute half-width defining an acceptance band `[target - tolerance, target + tolerance]`.
- **Returns:** `None` if `target` or `metrics.sphericity` is non-finite, or the projected mean is non-finite. Otherwise `0.0` if the projected mean falls inside the tolerance band (or exactly at `target` when `tolerance` is `None`, since it defaults to `0.0`), otherwise the absolute distance from the nearest band edge.
- **Side effects:** None (does not mutate `self`; the projection is `(self.sum + metrics.sphericity) / (self.count + 1)`, purely hypothetical).
- **Notes:** Confirmed by `tests/pack_target_tests.rs::sphericity_projection_selects_direction_that_repairs_mean`: given a running mean below target, a higher-sphericity candidate projects to a smaller error than a lower-sphericity one, so sorting proposals by ascending `projected_error` (as `PackPipeline::run` does) prefers candidates that pull the mean toward the target.

#### SphericityState::record_success

- **Signature:** `pub fn record_success(&mut self, metrics: MeshMetrics)`
- **Source:** `src/pipeline/pack_targets.rs:298`
- **Purpose:** Commits one successfully placed candidate's sphericity into the running mean.
- **Side effects:** Mutates `self.sum` and `self.count`.

### Free functions

#### load_target_distribution_csv

- **Signature:** `pub fn load_target_distribution_csv(path: &Path) -> Result<TargetDistribution>`
- **Source:** `src/pipeline/pack_targets.rs:309`
- **Purpose:** Parses and validates a target diameter distribution CSV into a normalized `TargetDistribution`.
- **Parameters:** `path` — CSV file path.
- **Returns:** `TargetDistribution` with strictly increasing, non-overlapping bins and frequencies renormalized to sum exactly to 1.0.
- **Side effects:** Reads the file from disk.

> **Algorithm:** See `../algorithms/packing-target-diameter-distribution.md` for the CSV format specification with worked examples.

- **CSV format:**
  - Header row selects columns from `{bin, right, frequency}` only (unknown columns, duplicate columns, or missing `bin`/`frequency` are errors). `right` is optional.
  - Each row: `bin` (the interval's `left`, must be finite, `>= 0`, and strictly greater than the previous row's `bin`), optional `right` (must be finite and `> bin` if present), and `frequency` (finite, `>= 0`).
  - **Right-edge inference** when `right` is blank/absent: uses the *next* row's `bin` value as this row's `right`; for the last row, extrapolates using the previous bin's width (`lefts[index] + previous_width`); a single-row distribution with no explicit `right` is an error (nothing to infer width from) — confirmed by `tests/pack_target_tests.rs::rejects_uninferable_single_row`.
  - Validates each resulting interval's midpoint is finite and `> 0`, and that intervals don't overlap (`lefts[index] >= previous.right`).
  - **Frequency normalization:** sums all row frequencies; the raw sum must be finite, `> 0`, and within `1e-6` of `1.0`, or the CSV is rejected (`rejects_invalid_frequency_sum` test: a sum of `0.4` fails). Passing sums are then divided through so bins sum to exactly `1.0`. At least one bin must end up with positive frequency.
- **Notes:** Confirmed against `tests/pack_target_tests.rs::parses_explicit_and_inferred_bin_rights`, `parses_two_column_distribution`, `rejects_overlapping_explicit_intervals`, `finite_large_edges_produce_finite_midpoint`, `rejects_interval_with_zero_underflowed_midpoint`, and `example_paper_distribution_is_valid_and_normalized` (validates the bundled 25-bin `data/input/gu2019_fig7b_pore_distribution.csv` fixture, spanning `5..30`).

#### write_distribution_comparison_csv

- **Signature:** `pub fn write_distribution_comparison_csv(output_stl: &Path, distribution: &TargetDistribution, state: &DistributionState) -> Result<PathBuf>`
- **Source:** `src/pipeline/pack_targets.rs:495`
- **Purpose:** Writes a per-bin target-vs-actual diameter frequency comparison CSV alongside the packed STL output.
- **Parameters:** `output_stl` — path the packed STL was (or will be) saved to, used to derive the sibling CSV's name/directory; `distribution` — the target distribution; `state` — final counters.
- **Returns:** `Path` to the written CSV, named `<output_stem>_diameter_distribution.csv` in the same directory as `output_stl` (falls back to stem `packed_result` if `output_stl` has no file stem).
- **Side effects:** Creates or overwrites the CSV file on disk.
- **Notes:** Columns: `bin, right, target_frequency, target_count, actual_count, actual_frequency, frequency_error, count_error, attempts`, each float formatted to 12 decimal places. `target_count = frequency * total_accepted_count` (a real number, not rounded). Confirmed by `tests/pack_target_tests.rs::writes_target_actual_comparison_csv_beside_output_stl`.

#### parse_csv_f64

- **Signature:** `fn parse_csv_f64(value: Option<&str>, column: &str, path: &Path, row: usize) -> Result<f64>`
- **Source:** `src/pipeline/pack_targets.rs:564`
- **Purpose:** Parses a required CSV cell as `f64`, with descriptive `InvalidConfig` errors including file path, row number, and column name.
- **Returns:** `Err(InvalidConfig)` if the cell is absent, blank after trimming, or fails to parse as a float; otherwise the parsed value (not checked for finiteness here — callers validate that separately).
- **Side effects:** None.

#### parse_optional_csv_f64

- **Signature:** `fn parse_optional_csv_f64(value: Option<&str>, column: &str, path: &Path, row: usize) -> Result<Option<f64>>`
- **Source:** `src/pipeline/pack_targets.rs:587`
- **Purpose:** Parses an optional CSV cell (used for the `right` column) where an absent or blank value means "not provided" rather than an error.
- **Returns:** `Ok(None)` if the cell is absent or blank; otherwise delegates to `parse_csv_f64` and wraps in `Some`.
- **Side effects:** None.

### Cached legacy packing feasibility (PERF-11)

`PackCollider::new(&Mesh)` retains bbox and optional parry TriMesh without another raw mesh copy. `blocks(other, gap)` keeps the old bbox rejection, uses cached solid collision at zero gap and cached solid distance below a positive gap. `pack_blocked` directly scans fewer than 32 colliders, otherwise queries an incremental grid plus every bbox-less collider. Candidate images are prepared once per proposal; accepted particle/image shapes are appended once and indexed. This replaces the previous `placed.clone()`, repeated ghost generation, shape construction and separate minimum-distance pass. The grid has at most eight cells on the longest domain axis. Void/nesting semantics and gap equality remain governed by the same prepared collision/distance functions. Periodic checks still include candidate ghosts against accepted real particles and ghosts. `PackPipeline::run` installs the worker pool around `run_in_pool`, including loading and finalization; proposal RNG remains sequential.
