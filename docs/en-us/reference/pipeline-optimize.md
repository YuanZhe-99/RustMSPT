# Pipeline: Optimize Reference

[SA algorithm](../algorithms/simulated-annealing-island-model.md) · [S2 definitions](../algorithms/s2-two-point-correlation.md) · [configuration](config.md).

## Index

| Item | Location | Summary |
|---|---|---|
| `OptimizePipeline` | `src/pipeline/optimize.rs:29` | Pipeline configuration and bounded execution entry point. |
| `ParticlePrepared` | `src/pipeline/optimize.rs:34` | Cached mesh, bbox and collision shape. |
| `IslandResult` | `src/pipeline/optimize.rs:41` | Best geometry/loss/S2 snapshot and candidate-stage timings. |
| `GlobalBest` | `src/pipeline/optimize.rs:47` | Mutex-protected coherent geometry/loss/S2 migration snapshot. |
| `prepare_particle` | `src/pipeline/optimize.rs:70` | Prepare one particle for collision queries. |
| `format_s2_series` | `src/pipeline/optimize.rs:139` | Format a curve at six decimal places. |
| `push_history_s2` | `src/pipeline/optimize.rs:148` | Append a labeled curve to history. |
| `prune_progress_message` | `src/pipeline/optimize.rs:153` | Format pruning loss, VF and particle count. |
| `selective_prune_to_target_vf` | `src/pipeline/optimize.rs:165` | Prune with the run-wide S2 definition under the installed pool. |
| `run_sa_island` | `src/pipeline/optimize.rs:359` | Run one SA island with the fixed evaluator and coherent migration. |
| `stage_rng` / `fixed_eval_seed` | `src/pipeline/optimize.rs` | Per-stage ChaCha12 stream fixed by (`optimization.seed`, stage) or seeded from `thread_rng`; one-off S2 seeds for target/input/final. |
| `OptimizePipeline::run` | `src/pipeline/optimize.rs:842` | Install every optimize stage in one configured Rayon pool. |
| `OptimizePipeline::run_in_pool` | `src/pipeline/optimize.rs:872` | Resolve execution, load/prepare, prune, batch islands and verify/save the winner. |
| `S2Method` | `src/pipeline/optimize_execution.rs:13` | Internal voxel_exact, voxel_mc and mesh_mc definitions. |
| `S2Method::resolve` | `src/pipeline/optimize_execution.rs:21` | Preserve existing exact/non-exact and pitch routing. |
| `S2Method::name` | `src/pipeline/optimize_execution.rs:32` | Return the actual method name for diagnostics. |
| `resolve_mode` | `src/pipeline/optimize_execution.rs:42` | Resolve the optional environment override above YAML; reject invalid values. |
| `select_s2_backend` | `src/pipeline/optimize_execution.rs:58` | Apply method/CPU/auto/capacity gates before any GPU probe; enforce fallback policy. |
| `OptimizeS2` | `src/pipeline/optimize_execution.rs:142` | Fixed per-run method, pitch and optional shared GPU evaluator. |
| `OptimizeS2::new` | `src/pipeline/optimize_execution.rs:158` | Resolve execution once and initialize at most one persistent GPU MC instance. |
| `OptimizeS2::evaluate` | `src/pipeline/optimize_execution.rs:248` | Evaluate any stage consistently; serialize GPU buffers and release the lock before VF work. |
| `run_island_batches` | `src/pipeline/optimize_execution.rs:346` | Run ordered batches bounded by active Rayon worker count. |
| `merge_prepared_particles` | `src/pipeline/optimize.rs:123` | Merge geometry and assign stable particle vertex ranges. |

## Types

`OptimizePipeline { config: OptimizationConfig }` implements `Pipeline`.
`ParticlePrepared` caches `mesh: Mesh`, `bbox: Option<BoundingBox>` and `shape: Option<TriMesh>`.
`IslandResult` contains `best: Arc<GlobalBest>`, `s2_time` and `collision_time`.
The two durations cover candidate scoring/constraint checks in that island, not total pipeline work.
`GlobalBest { loss: f64, particles: Vec<Mesh>, s2: Vec<f64> }` is protected by
`Arc<Mutex<Arc<GlobalBest>>>`; all three values must describe the same evaluated configuration.

## Functions in optimize.rs

#### prepare_particle

`fn prepare_particle(mesh: Mesh) -> ParticlePrepared` computes bbox and parry collision shape.
It consumes the mesh and returns cached geometry; no I/O.

#### format_s2_series

`fn format_s2_series(values: &[f64]) -> String` joins six-decimal values with spaces; no side effects.

#### push_history_s2

`fn push_history_s2(history_log: &mut Vec<String>, label: &str, values: &[f64])` appends
`label: formatted_curve` to the supplied buffer; no I/O.

#### prune_progress_message

`fn prune_progress_message(current_loss: f64, current_vf: f64, target_vf: f64, particles: usize) -> String`
formats pruning progress; no side effects.

#### selective_prune_to_target_vf

```rust
fn selective_prune_to_target_vf(
    particles: &mut Vec<Mesh>, box_bounds: BoundingBox, target_s2: &[f64],
    params: &OptimizationParams, r_max: usize, evaluator: &OptimizeS2,
    history_log: &mut Vec<String>,
) -> Result<()>
```

Runs under the caller's installed pool. Evaluates all removal candidates through `evaluator`
with stage `prune`, the original reduced sample budget and radius capped to the target length.
Returns `Result<()>` and mutates particles/history; evaluator errors propagate. Disabled pruning, fewer than two particles or a
nonpositive target VF return immediately. Batch sizing, removal ordering, RNG and geometric VF
accounting are unchanged; it never constructs another backend or pool.

#### run_sa_island

```rust
fn run_sa_island(
    island_id: usize, num_islands: usize, prepared_init: Vec<ParticlePrepared>,
    target: &[f64], params: &OptimizationParams, box_bounds: BoundingBox,
    mode: u8, d1: f64, d2: f64, min_neighbor: f64, rotation_mode: &RotationMode,
    evaluator: &OptimizeS2, global_best: Option<&Arc<Mutex<Arc<GlobalBest>>>>,
    migration_interval: usize, history_log: &mut Vec<String>,
) -> Result<IslandResult>
```

Owns one island's particles, RNG, temperature and acceptance window. Initialization, candidate
scoring and migration re-evaluation use the same evaluator (`initial`, `candidate`, `migration`).
Boundary/collision rejection, periodic ghosts, rollback, grid rebuilds and Metropolis rules remain
as described in the algorithm document. Candidate MC samples are
`round(mc_samples * (0.3 + 0.7 * scale))`, clamped to `[1000, max(mc_samples,1000)]`:
hot iterations use more samples, cold iterations fewer. Initialization/migration use at least 2000.

Migration publishes/adopts geometry, loss **and S2** together. After adopting the saved best snapshot,
the current walk is re-evaluated at the full sample budget. The saved historical best remains tied
to its own curve, which can differ from the re-evaluation under MC noise. The global lock is released
before geometry preparation/S2 work, so nested Rayon work cannot wait on that same held lock.
Returns the best snapshot and candidate-stage timings; mutates history/global state and prints progress.


**Seeded runs (PLAN.Performance.md@1349c46 §79).** `optimization.seed` makes a single-island run reproducible on any worker count. `stage_rng(seed, stage)` gives pruning (stage 1) and each island (stage 100 + id) a ChaCha12 stream; every S2 evaluation takes a seed drawn from that stream (only when seeded, so unseeded streams are unchanged) and passes it to `calculate_s2_seeded`, `VoxelS2::calculate_seeded` or `calculate_s2_gpu_seeded`; target, input and final evaluations use `fixed_eval_seed`. Voxel MC seeds one generator per radius, so parallel scheduling cannot change a curve. Several islands exchange snapshots on thread timing and stay non-reproducible. Test: `a_seeded_single_island_optimize_is_reproducible_on_any_worker_count` (voxel MC and mesh MC, 1 and 4 workers, and a different seed must differ).
#### OptimizePipeline::run

`fn run(&self) -> Result<()>` resolves `cpu_max` using the existing clamp to available parallelism,
creates one Rayon pool and installs `run_in_pool`. Pool creation failure returns `InvalidConfig`.
All input/reference preparation, VF, pruning, island work and output processing execute inside it.

#### OptimizePipeline::run_in_pool

`fn run_in_pool(&self, thread_pool: &ThreadPool) -> Result<()>` requires that pool to be installed.
Loads/splits/prefilters geometry, resolves `OptimizeS2`, measures the target/input and prunes.
Single-island execution consumes the prepared vector directly. Multi-island execution uses
`run_island_batches`; initial geometry is cloned only when each bounded task starts. Histories are
collected in island order and the smallest historical loss selects the winner.

Re-evaluates the winner using the same method at `max(mc_samples,2000)`, records
`Selected Search Loss`, `Final Best S2` and `Final Best Loss`, after optional orientation, then saves STL
and history. Thus final MC loss can differ from the historical selection score. Exact final curves
are checked against the saved geometry in integration tests. History includes actual method,
backend, effective pitch, reason, worker count, island count and active-island limit.
Returns configuration/backend/I/O errors; no per-island threads, pools or GPU constructors remain.

## Execution contracts in optimize_execution.rs

#### S2Method::resolve

`fn resolve(params: &OptimizationParams) -> S2Method`: `exact` → `VoxelExact`; otherwise
nonpositive pitch → `MeshMc`, positive pitch → `VoxelMc`. Legacy non-exact strings (including `both`)
retain MC routing. No side effects. `S2Method::name(self) -> &'static str` returns
`voxel_exact`, `voxel_mc` or `mesh_mc`.

#### resolve_mode

`fn resolve_mode(configured: AccelerationMode, override_value: Option<&str>) -> Result<AccelerationMode>`
accepts `cpu`, `gpu`, `auto`, or no override; any other override is `InvalidConfig`. No I/O.

#### select_s2_backend

`fn select_s2_backend(params: &OptimizationParams, requested: AccelerationMode, workload: usize,
probe: impl FnOnce() -> BackendSelection) -> Result<BackendSelection>`:

- CPU and below-threshold auto return CPU before calling the probe. Below-threshold auto is a normal
  policy choice, even with `cpu_fallback: false`.
- GPU MC is compatible only with `mesh_mc`. Voxel methods stay on CPU with a reason; forbidden
  fallback returns an error rather than changing the S2 definition.
- The current optimizer GPU supports `backend: wgpu`, `gpu_precision: f32`,
  `gpu_prefer_power: false`; incompatible options on a GPU-eligible method are rejected.
- An explicit GPU cap checks a conservative logical working-set estimate before probing and again for each evaluation, including retained capacities, old-plus-new growth and pending uploads. It is independent of single-binding device limits.
- Radius count above 128, overflowing invocation counts, or total calls above `65535*256` reject the
  GPU route before probing; budgets include pruning and the 2000-sample evaluation floor.
- Otherwise calls the probe once; unavailable GPU honors `cpu_fallback`.

#### OptimizeS2::new

`fn new(params: &OptimizationParams, bbox: BoundingBox, mesh: &Mesh) -> Result<OptimizeS2>` reads
`RUSTMSPT_ACCELERATION` once (environment overrides YAML for optimize), validates finite pitch,
and resolves method, effective pitch and backend. Exact nonpositive pitch becomes 1.0 once.
Mesh MC's auto workload estimate retains the legacy pitch-1 grid estimate. After a GPU selection,
creates at most one persistent MC instance shared by all stages/islands; initialization failure
falls back with an accurate diagnostic or errors when prohibited. The existing capability probe
still creates a temporary device separately: this is not complete GPU-context reuse.

#### OptimizeS2::evaluate

`fn evaluate(&self, mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize,
stage: &'static str) -> Result<Vec<f64>>` uses the fixed method/pitch in the current Rayon pool.
The optional GPU mutex covers upload, dispatch and readback; **it is dropped before CPU VF work**,
which can use nested Rayon. Mesh MC uses the CPU mesh reference's geometric VF convention;
voxel methods retain occupancy VF. Test builds observe stage, worker count and index at this boundary.
The stages are `target`, `input`, `prune`, `initial`, `candidate`, `migration`, `final`.
MC mapping/captured device errors now propagate under the policy below. Numerical certification and complete workload budgets remain PERF-04/05 work.

#### run_island_batches

`fn run_island_batches<T: Send>(islands: usize, run: impl Fn(usize) -> T + Sync) -> Vec<T>`
runs contiguous batches no larger than the current pool size, with indexed Rayon tasks inside each
batch. It returns every result in island order. Even nested work stealing cannot activate more than
one batch's island contexts; surplus islands queue. Result retention still scales with island count.
No new pools or OS threads are created. Callers must install the intended pool first.

## Verification

`cargo test --offline --lib pipeline::optimize` observes actual evaluation/worker boundaries,
checks compatible backend gates without a GPU probe, and tests migration snapshot coherence.
`cargo test --offline --test optimize_execution_tests --test pipeline_smoke_tests` exercises the CLI,
environment overrides, reference targets, saved exact curves and more islands than workers.
Repeat with `--features gpu`; the shared GPU MC test prints its adapter or an explicit skip.
Tests of already-shipped pipelines do not imply hardware GPU performance validation.

### Runtime GPU fallback (2026-09-13)

`OptimizeS2` stores `Option<Mutex<Option<GpuS2Pipeline>>>` and `cpu_fallback`. After an upload/dispatch/readback error, allowed fallback removes the failed pipeline while holding the mutex, releases the lock, logs the stage and reason, then recomputes CPU mesh MC. All later stages stay on CPU. Forbidden fallback returns `RustMsptError::Gpu` with the stage and cause. `evaluate`, pruning and each island now return `Result`; the pipeline propagates errors before writing final STL/history. Island batches already running are joined before their errors are propagated. The startup description records the initial backend; the warning records a later switch.

### Incremental grid acceptance (PERF-10/13)

`SpatialGrid` retains reverse item-to-cell membership. `remove(idx)` removes all insertions of that id and preserves remaining bucket order; `update(idx, Option<BoundingBox>)` replaces membership or removes the item. Query order remains first encounter, but moving an item appends it in its new buckets, so consumers must not assume rebuild order. Candidate sets match a full rebuild. Optimize queries the current grid before evaluating a proposal, updates one membership only after acceptance, and restores the original prepared particle directly on rejection. Whole-population migration still rebuilds the grid. Repeated inserts retain their old semantics and removal clears every copy. Reverse membership consumes additional memory proportional to inserted cell references.

### Persistent merged geometry (PERF-10)

`merge_prepared_particles` directly assembles prepared meshes and records each particle's vertex range without temporary particle clones. Rigid candidates overwrite only their range; rejection restores its original vertices and prepared collider. Faces and other vertex ranges stay resident. Population migration recreates the merged mesh/ranges. GPU MC triangle uploads now diff the new merged mesh against the host shadow of the resident buffer and write only changed face runs (one moved particle = that particle's faces), falling back to a full write on a triangle-count change, a grown buffer, more than 64 runs or more than half the faces changed; `GpuS2Pipeline::upload_stats` records the bytes. Voxel occupancy remains a full update; geometric VF uses the island-local cache described below.

### Optimize MC memory budget

The optimizer starts its shared GPU MC pipeline with empty geometry; each stage uploads the mesh it actually evaluates. `mc_evaluation_peak` bounds the triangle buffer, four output/readback buffers, 608-byte parameter storage, the uncertain-sample list and its staging, pending queue uploads and the next mesh/parameter uploads. Growth conservatively counts old plus new allocations. Startup uses the input face count and maximum configured stage sample count; every actual evaluation rechecks current retained capacities under the same GPU mutex before upload. A larger reference mesh or retained high-water capacity can therefore trigger the existing same-method fallback (or a stage-labelled strict error). Pending upload bytes reset only after successful readback. Driver internals and CPU mesh/readback vectors are outside this logical GPU budget. Batch splitting/automatic high-water trimming remain separate work; `release_output_capacity` provides explicit release.

### Immutable migration snapshots

Each island stores its evaluated best as `Arc<GlobalBest>` containing geometry, loss and S2 together; `IslandResult` retains that Arc plus timings. Improvements build a new snapshot outside the migration mutex. The shared slot is `Arc<Mutex<Arc<GlobalBest>>>`. `exchange_best_snapshot` compares strict losses and swaps/clones only Arc references under the lock, returning a better incoming snapshot if present. Replaced snapshots are dropped after unlocking, so releasing their geometry cannot lengthen the critical section. Ties preserve the incumbent. Receivers rebuild their mutable prepared geometry/grid and re-evaluate the current walk outside the lock; the saved best curve remains paired with its original evaluated geometry/loss. Final selection borrows the winning shared snapshot rather than cloning it. This removes migration payload copies; creating a new local best and preparing a received mutable walk still copy geometry.

| `exchange_best_snapshot` | `src/pipeline/optimize.rs:54` | Exchange immutable best Arc snapshots; release retired payload outside the lock. |

Mesh-MC islands now cache each particle’s connected-component clipped-volume contributions in `IslandVolumes`. Feasible candidates replace only their particle’s entries; rejection restores the saved entries, migration rebuilds the cache, and every 64 evaluated candidates refresh all entries. VF still sums all cached component scalars in merged source order and applies the original denominator/clamp, avoiding running-delta drift. The cache is limited to continuous mesh MC: voxel methods keep voxel VF. CPU fixed-seed sampling and GPU failure fallback can accept the validated VF without recomputing geometry, and GPU mutex boundaries stay unchanged. Fully contained transformed particles are still recomputed to preserve floating-point reference behavior; no rigid-volume shortcut is assumed. Pruning and final verification retain full reference evaluation.

| Function | Source | Contract |
|---|---|---|
| `IslandVolumes::new` | `src/pipeline/optimize_volume.rs:12` | Build ordered connected-component contributions for a population. |
| `IslandVolumes::contributions` | `src/pipeline/optimize_volume.rs:20` | Clip connected components using the existing geometric volume definition. |
| `IslandVolumes::replace` | `src/pipeline/optimize_volume.rs:33` | Update one particle and return prior entries for rollback. |
| `IslandVolumes::restore` | `src/pipeline/optimize_volume.rs:41` | Restore entries after rejection. |
| `IslandVolumes::fraction` | `src/pipeline/optimize_volume.rs:46` | Sum cached scalars in merged component order, then clamp. |
| `OptimizeS2::evaluate_with_vf` | `src/pipeline/optimize_execution.rs:262` | Evaluate mesh MC with optional validated VF; reject geometric cache for voxel methods. |
| `calculate_s2_mesh_mc_seeded_with_vf` | `src/geometry/s2.rs:1279` | Preserve fixed-seed mesh MC samples while using caller-provided VF. |

### Grid statistics and query counters (PERF-13 observability)

At the end of each island `run_sa_island` prints `SpatialGrid::stats()` as `[GridStats] optimize island=<id> buckets=..` and `[GridStats] optimize island=<id> queries grid_queries=.. grid_candidates=.. bbox_rejects=.. narrow_phase=.. distance_checks=..`, counting candidate and ghost queries, bbox rejections, exact overlap tests and exact distance tests in the serial collision loop. The counters are plain local integers incremented beside the existing branches; no decision, RNG draw or history entry changes.

### GPU MC certification summary

`OptimizeS2::gpu_certification_summary() -> Option<String>` (`src/pipeline/optimize_execution.rs`) briefly locks the shared GPU MC mutex and describes its cumulative `GpuCertificationStats` (valid samples, samples recomputed on the CPU, ratio). It returns `None` on CPU or after a fallback removed the GPU instance. `OptimizePipeline::run` prints it as `[Info] Optimize GPU mesh_mc f32 certification: ...` before `Optimization completed.`
