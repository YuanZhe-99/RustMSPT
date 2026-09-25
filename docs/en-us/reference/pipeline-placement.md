# Pipeline: Placement Reference

This page documents the seeded, recorded, void-aware packing engine: `src/pipeline/placement.rs`
(the loop and the outputs), `placement_sizes.rs` (what to place), `placement_library.rs` (what shapes
are available), `placement_feasibility.rs` (whether a proposal is allowed), `placement_outputs.rs`
(the record and report types), `placement_labels.rs` (the optional voxel field), and the void index
in `src/geometry/void_index.rs`.

The engine is selected by a `pack` config carrying a top-level `placement:` block. A `packing:` block
selects the original loop instead, which is documented in
[pipeline-packing.md](pipeline-packing.md) and is unchanged.

> **Algorithm:** the decisions behind this code, and the arguments for them, are in
> [void-aware-placement.md](../algorithms/void-aware-placement.md). Configuration is in
> [config.md](config.md). Worked runs are in [pack-placement.md](../examples/pack-placement.md) and
> [pack-void.md](../examples/pack-void.md).

## Index

| Item | Location | Summary |
|---|---|---|
| `VoidVolumeMethod` | `src/geometry/void_index.rs:18` | Which method produced a void's in-domain volume. |
| `VoidIndex` | `src/geometry/void_index.rs:32` | A frozen void, indexed for the queries a placement run makes. |
| `VoidIndex::build` | `src/geometry/void_index.rs:56` | Validates a void mesh and builds its index; refuses mixed orientation. |
| `VoidIndex::bbox` | `src/geometry/void_index.rs:118` | The void's bounding box. |
| `VoidIndex::shells` | `src/geometry/void_index.rs:123` | How many closed shells the void has. |
| `VoidIndex::is_outward` | `src/geometry/void_index.rs:128` | Whether the void's shells wind outward. |
| `VoidIndex::total_volume` | `src/geometry/void_index.rs:133` | The void's total closed volume over shells with agreeing signs. |
| `VoidIndex::contains_point` | `src/geometry/void_index.rs:150` | Ray-parity point-in-void test; a bbox pre-check, then `trimesh_contains_point`. |
| `VoidIndex::near_box` | `src/geometry/void_index.rs:166` | Box prefilter: false means far from the void and not nested in it. |
| `VoidIndex::intersects` | `src/geometry/void_index.rs:186` | Whether a particle's surface intersects the void's. |
| `VoidIndex::min_distance_to` | `src/geometry/void_index.rs:202` | Minimum surface-to-surface distance from a particle to the void. |
| `VoidIndex::surface_distance` | `src/geometry/void_index.rs:219` | Unsigned distance from a point to the void surface. |
| `VoidIndex::any_vertex_inside` | `src/geometry/void_index.rs:233` | Whether any of a mesh's vertices lies inside the void. |
| `VoidIndex::any_void_vertex_inside` | `src/geometry/void_index.rs:245` | Whether any void vertex lies inside a particle. |
| `VoidIndex::volume_in_domain` | `src/geometry/void_index.rs:264` | The void's volume inside a domain, and which method produced it. |
| `VoidIndex::sample_surface_point` | `src/geometry/void_index.rs:288` | Area-weighted point on the void surface with its outward normal. |
| `VoidIndex::overlap_volume` | `src/geometry/void_index.rs:330` | Volume of a particle inside the void, by domain-anchored voxel count. |
| `point_inside_mesh_local` | `src/geometry/void_index.rs:375` | Ray-parity point-in-mesh test for a small mesh with no hierarchy. |
| `PlacementPipeline` | `src/pipeline/placement.rs:38` | Pipeline struct holding a validated `ResolvedPlacement`. |
| `PHASE_MATRIX` | `src/pipeline/placement_labels.rs:13` | Phase code 0 in the written label field. |
| `VoxelLabelsHeader` | `src/pipeline/placement_labels.rs:22` | What the label stacks are: spacing, origin, layout, phase table. |
| `PhaseLabel` | `src/pipeline/placement_labels.rs:39` | One phase code and its name. |
| `write_voxel_labels` | `src/pipeline/placement_labels.rs:60` | Writes the three-phase label field and the per-voxel particle id field. |
| `particle_at` | `src/pipeline/placement_labels.rs:306` | Finds which placed particle, if any, contains a point. |
| `point_in_particle` | `src/pipeline/placement_labels.rs:318` | Ray-parity containment for one particle mesh. |
| `VoidReport` | `src/pipeline/placement_outputs.rs:278` | What the run did with the frozen void, and how it measured it. |
| `build_void_report` | `src/pipeline/placement.rs:1170` | Describes the frozen void for the report, including its volume method. |
| `PlacementPipeline` | `src/pipeline/placement.rs:38` | Pipeline struct holding a validated `ResolvedPlacement`. |
| `PlacementOutcome` | `src/pipeline/placement.rs:61` | What a completed run produced, for in-process callers. |
| `run_placement` | `src/pipeline/placement.rs:76` | Runs the engine in a dedicated configured Rayon pool and writes every output file. |
| `with_placement_pool` | `src/pipeline/placement.rs:81` | Creates and installs a dedicated Rayon pool for one operation, propagating creation/work errors. |
| `run_placement_in_pool` | `src/pipeline/placement.rs:90` | Runs all placement stages in the active pool and records its actual worker count. |
| `resolve_threads` | `src/pipeline/placement.rs:267` | Turns a thread setting into a worker count, at least 1. |
| `EngineState` | `src/pipeline/placement.rs:279` | Everything the placement loop accumulates. |
| `place_all` | `src/pipeline/placement.rs:361` | Attempts every planned size in order, accepting what fits. |
| `try_place_one` | `src/pipeline/placement.rs:400` | Tries one size within its per-particle attempt budget. |
| `accept` | `src/pipeline/placement.rs:558` | Commits an accepted candidate into the geometry and the record. |
| `run_top_up` | `src/pipeline/placement.rs:619` | Draws further batches when clipping alone left the target short. |
| `decide_stop` | `src/pipeline/placement.rs:802` | Decides which of the four stop reasons a run ended with. |
| `write_outputs` | `src/pipeline/placement.rs:802` | Writes the geometry, the per-particle record and the size CSV. |
| `entity_id` | `src/pipeline/placement.rs:947` | The stable id of a placed particle. |
| `particle_record` | `src/pipeline/placement.rs:952` | Turns one placed particle into its record entry. |
| `size_class_rows` | `src/pipeline/placement.rs:1000` | Builds the per-class target-against-actual rows. |
| `blank_report` | `src/pipeline/placement.rs:1026` | The report as it stands before placement starts. |
| `describe_input` | `src/pipeline/placement.rs:1205` | Describes an input file with its digest for the report. |
| `finish_report` | `src/pipeline/placement.rs:1224` | Fills in everything the finished run knows. |
| `summary` (placement.rs) | `src/pipeline/placement.rs:1288` | Builds the human-readable stdout summary. |
| `read_record` | `src/pipeline/placement.rs:1349` | Reads a written per-particle record back. |
| `read_report` | `src/pipeline/placement.rs:1357` | Reads a written run report back. |
| `RejectReason` | `src/pipeline/placement_feasibility.rs:17` | Why a proposed placement was not accepted; the report's keys. Ten variants. |
| `RejectReason::as_str` | `src/pipeline/placement_feasibility.rs:44` | The stable report key for a rejection reason. |
| `PlacedParticle` | `src/pipeline/placement_feasibility.rs:76` | A particle that cleared every check, with its cached shape. |
| `PlacedParticle::volume_in_domain_solid` | `src/pipeline/placement_feasibility.rs:106` | The particle volume counting toward the solid phase. |
| `FeasibilityContext` | `src/pipeline/placement_feasibility.rs:112` | Everything a feasibility check reads. |
| `Candidate` | `src/pipeline/placement_feasibility.rs:132` | A proposed placement with its cheap quantities precomputed. |
| `Accepted` | `src/pipeline/placement_feasibility.rs:146` | What a passing check worked out along the way. |
| `check_placement` | `src/pipeline/placement_feasibility.rs:171` | Runs every feasibility rule in order, returning the one that stopped it. |
| `PAIR_PARALLEL_MIN` | `src/pipeline/placement_feasibility.rs:15` | Pairs needing an exact distance at which those distances run in parallel; `usize::MAX` (serial) by default because dense end-to-end runs measured no gain (0-10 % slower on a shared 4-core host), although `pair_threshold_benchmark` shows 1.4-2x from two clear pairs. |
| `pair_needs_exact_test` | `src/pipeline/placement_feasibility.rs` | Centre-sphere and box separation tests for one neighbour; true when the exact tests must run. |
| `first_pair_rejection` | `src/pipeline/placement_feasibility.rs` | Serial overlap/enclosure scan to the first failure, then ordered (`find_map_first`) parallel distances for the pairs before it; returns exactly the serial reason. |
| `solid_pair_rejection` | `src/pipeline/placement_feasibility.rs` | Overlap then enclosure test for one pair. |
| `retained_depth` | `src/pipeline/placement_feasibility.rs:382` | How far a straddling particle still reaches inside the domain. |
| `ToolRecord` | `src/pipeline/placement_outputs.rs:15` | The build identity as it appears in a record or report. |
| `StopReason` | `src/pipeline/placement_outputs.rs:53` | The fixed four-word vocabulary a run may stop with. |
| `ParticleRecord` | `src/pipeline/placement_outputs.rs:133` | One placed particle's entry in the record file. |
| `RecordFile` | `src/pipeline/placement_outputs.rs:152` | The per-particle record file's top-level shape. |
| `conventions` | `src/pipeline/placement_outputs.rs:173` | States every convention a reader needs to reconstruct a particle. |
| `ReportFile` | `src/pipeline/placement_outputs.rs:246` | The run report file's top-level shape. |
| `SizeClassRow` | `src/pipeline/placement_outputs.rs:226` | One class's target, drawn, placed, shortfall and top-up counts. |
| `write_json` | `src/pipeline/placement_outputs.rs:380` | Writes a JSON value to disk, creating parent directories. |
| `describe_output` | `src/pipeline/placement_outputs.rs:400` | Describes a written output file for the report's manifest. |
| `write_size_distribution_csv` | `src/pipeline/placement_outputs.rs:424` | Writes the per-size-class comparison CSV. |
| `inverse_normal_cdf` | `src/pipeline/placement_sizes.rs:21` | Wichura AS241 standard-normal quantile, error below 1e-15. |
| `poly` (placement_sizes.rs) | `src/pipeline/placement_sizes.rs:133` | Horner evaluation of highest-degree-first coefficients. |
| `normal_cdf` (placement_sizes.rs) | `src/pipeline/placement_sizes.rs:142` | Standard normal CDF via the complementary error function. |
| `erfc` | `src/pipeline/placement_sizes.rs:153` | Complementary error function, used to turn truncation bounds into probabilities. |
| `SizeDraw` | `src/pipeline/placement_sizes.rs:175` | One drawn diameter with its reporting class and draw order. |
| `SizeClass` | `src/pipeline/placement_sizes.rs:185` | A diameter band and the share of particles it should hold. |
| `SizeSource` | `src/pipeline/placement_sizes.rs:192` | A prepared target number distribution: truncated lognormal or histogram. |
| `SizeSource::prepare` | `src/pipeline/placement_sizes.rs:218` | Prepares a source, loading the histogram CSV and precomputing truncation. |
| `SizeSource::sample` | `src/pipeline/placement_sizes.rs:272` | Draws one diameter, consuming exactly one u64. |
| `SizeSource::support` | `src/pipeline/placement_sizes.rs:309` | The smallest and largest diameter the source can produce. |
| `SizeSource::mass_between` | `src/pipeline/placement_sizes.rs:381` | The share of the target distribution between two diameters. |
| `build_classes` | `src/pipeline/placement_sizes.rs:329` | Builds the reporting classes target and actual are compared over. |
| `build_equal_width` | `src/pipeline/placement_sizes.rs:355` | Splits a source's support into equal-width classes with their shares. |
| `class_for_diameter` | `src/pipeline/placement_sizes.rs:430` | Finds a diameter's reporting class; the top edge is inclusive. |
| `SizePlan` | `src/pipeline/placement_sizes.rs:449` | The sizes a run intends to place, drawn before any placement. |
| `plan_size_multiset` | `src/pipeline/placement_sizes.rs:473` | Draws the whole multiset, stopping at whichever count lands closer to the target. |
| `order_for_placement` | `src/pipeline/placement_sizes.rs:534` | Orders a drawn multiset largest-first, or back into draw order. |
| `ShapeShell` | `src/pipeline/placement_library.rs:13` | One closed shell: measured, centred, and digested. |
| `ShapeSource` | `src/pipeline/placement_library.rs:41` | A source file the library was built from, with its digest and shell counts. |
| `RejectedShell` | `src/pipeline/placement_library.rs:53` | A shell read but not kept, and why. |
| `ShapeLibrary` | `src/pipeline/placement_library.rs:61` | Every shape a run may draw from, plus what was read and not kept. |
| `load_shape_library` | `src/pipeline/placement_library.rs:85` | Loads, splits, measures and filters the shape files. |
| `filter_reason` | `src/pipeline/placement_library.rs:234` | Says which library filter a shell failed, if any. |
| `shell_geometry_sha256` | `src/pipeline/placement_library.rs:274` | Digests a shell's geometry so a re-ordered file is detectable. |
| `particle_at_prepared` | `src/pipeline/placement_labels.rs:326` | First particle in ordered cached candidates. |
| `LABEL_SLAB_VOXELS` | `src/pipeline/placement_labels.rs:46` | Target voxels per label slab (4,194,304). |
| `label_dims` | `src/pipeline/placement_labels.rs:111` | Label grid dimensions with overflow check. |
| `LabelQuery` | `src/pipeline/placement_labels.rs:124` | Prepared particle queries, bbox grid and void shared by all slabs. |
| `LabelQuery::new` | `src/pipeline/placement_labels.rs:136` | Prepare the per-run query context once. |
| `LabelQuery::centre` | `src/pipeline/placement_labels.rs:170` | Voxel-centre world position. |
| `LabelQuery::fill_slab` | `src/pipeline/placement_labels.rs:184` | Classify one z-slab into phase/id buffers. |
| `write_label_stacks` | `src/pipeline/placement_labels.rs:257` | Stream both label TIFF stacks slab by slab. |

## Reading order

The engine is one loop with everything else hanging off it. `run_placement` is the entry point and
the only function that writes anything; `PlacementPipeline::run` is a thin wrapper so that tests can
drive a run without going through stdout.

```
run_placement
├── load_shape_library          what shapes exist, measured and centred
├── VoidIndex::build            the frozen void, validated and indexed
├── SizeSource::prepare         the target number distribution
├── build_classes               the reporting classes
├── plan_size_multiset          every size, drawn before anything is placed
├── blank_report + write_json   the report, written once as `running`
├── place_all
│   └── try_place_one           per size: propose, then check
│       └── check_placement     every rule, in one fixed order
├── run_top_up                  only if nothing failed
├── decide_stop                 which of the four words
├── write_outputs               geometry, record, size CSV, void copy, labels
└── finish_report + write_json  the report again, as `finished`
```

## Four contracts worth reading before the code

**The RNG consumption schedule is fixed.** Every attempt draws all of its variates before any check
runs. A check that short-circuited before a draw would make the stream depend on which check fired,
so reordering the checks later would silently move every placement.

**The volume accumulator is sequential.** A parallel `f64` sum depends on the reduction tree and so
on the thread count, and it is the number that gates the stop. The report emits that same
accumulator rather than re-summing, so it cannot disagree with the decision it describes.

**Expensive work is deferred behind cheap rejections.** The `parry` `TriMesh`, the exact in-box
volume and the exact pair distance are each reached only when the cheaper tests have failed to settle
the question. Writing them eagerly cost a factor of sixteen on a small run.

**Parallel pair checks never change the answer.** The cheap neighbour tests run serially; overlap and
enclosure then run serially up to the first failing survivor, and only the pairs before it need the
exact distance, which is the whole cost. When `FeasibilityContext::pair_parallel_min` (default
`PAIR_PARALLEL_MIN`) or more of them do, the distances run in parallel with the ordered
`find_map_first`, so the returned reason is the first one in neighbour order and per-pair check order,
exactly as a serial scan would return. Counters are still incremented only by the sequential caller.
`forced_parallel_pair_checks_match_serial_attempt_by_attempt` replays a fixed candidate sequence
forced serial and forced parallel on 1/2/4/8 workers and compares every attempt.

**Stopping short is a result.** A run that places nothing writes its report, writes no STL, and exits
zero. Only an unusable config or an unwritable output is an error.

## Detailed entries

Every function and type below carries an `AI-FUNC-SUMMARY` comment in the source stating its purpose,
inputs, returns, side effects and the assumptions behind it. Rather than duplicate them here, this
page indexes them; the summaries are the contract, and they are kept in step with this file.

The entries most worth reading in the source first, and why:

| Item | Why |
|---|---|
| `check_placement` | Every placement rule in one place, in one fixed order, with the completeness argument for the void predicate written out above it. |
| `plan_size_multiset` | The closest-sum stopping rule and the reason a failed size is never replaced. |
| `decide_stop` | The stop-reason precedence, including why unattainable must outrank budget-exhausted. |
| `conventions` | The sentences a consumer needs to reconstruct a particle, written into the record itself rather than left to documentation. |
| `VoidIndex::contains_point` | Why the inside test is ray parity rather than a pseudo-normal test. |
| `mesh_volume_in_bbox_exact` | Why caps are built plane by plane, and what the legacy cap gets wrong. |

## Output schemas

| File | Schema | Written by |
|---|---|---|
| `particles.json` | `rustmspt.placement.record/1` | `RecordFile`, `write_json` |
| `run_report.json` | `rustmspt.placement.report/1` | `ReportFile`, `write_json` |
| `size_distribution.csv` | header row, ten columns | `write_size_distribution_csv` |
| `voxel_labels/voxel_labels.json` | `rustmspt.placement.voxel_labels/1` | `VoxelLabelsHeader` |

`StopReason` is a fixed four-word vocabulary: `target_reached`, `budget_exhausted`,
`distribution_unattainable`, `no_feasible_placement`. A consumer's adapter is written against that
list, so nothing may emit a fifth value.

## Cross-cutting notes

- `PlacedParticle` keeps its `parry` `TriMesh` once built, so an accepted particle never has one
  rebuilt for it. The original engine rebuilds one per comparison, which dominates its collision
  loop.
- `RejectReason::ALL` is the report's emission order and reads as a funnel. The exact clip check is
  *evaluated* after the neighbour tests, for cost; the tally order is the documented rule order.
- `particle_overlap` and `particle_enclosed` are two different failures and the second is the one
  v0.2.0 was missing. Surfaces that cross give the first; one particle wholly inside another gives
  the second, and the surfaces in that arrangement never cross while the gap test reads the space
  between them as clearance. The void arm had both halves of the argument from the start; the
  particle arm did not, which is how a run came to place 28 of 146 particles inside another and
  report `target_reached`. See
  [void-aware-placement.md](../algorithms/void-aware-placement.md#61-the-predicate-and-why-it-is-complete).
- `VoidIndex` has a deliberately terse `Debug`: the mesh and its hierarchy would fill a screen and
  say nothing a reader wants.
- The report lists itself in `outputs` with a null digest. A file cannot contain its own hash, and an
  adapter that verifies every listed digest would otherwise choke on the one entry that cannot have
  one.

### run_placement / run_placement_in_pool — PERF-02

`run_placement(config: &ResolvedPlacement) -> Result<PlacementOutcome>` creates a dedicated
Rayon pool using `resolve_threads`, then installs the private
`run_placement_in_pool(config: &ResolvedPlacement) -> Result<PlacementOutcome>` for the whole run,
including source preparation, geometry, void queries and label output. Pool creation errors return
`InvalidConfig` before any input/output work. The private runner retains the sequential RNG and
volume accumulation and records `rayon::current_num_threads()` from the executing pool.
`runtime.threads` therefore measures the active pool rather than echoing a requested setting.
Explicit positive counts are honored; nonpositive values use available parallelism. This fixes the
resource contract; parallelizing the acceptance loop is separate work.

`with_placement_pool<T: Send>(threads: i32, work: impl FnOnce() -> Result<T> + Send) -> Result<T>`
is the private execution wrapper. It starts/joins the workers and returns the operation result;
its unit test observes pool size and worker indices inside nested `par_iter` work at 1/2/8 workers.

### Label preparation (PERF-12)

`voxel_labels` prepares immutable per-particle mesh queries and a bounded spatial grid once. Parallel 1024-voxel tiles query their enclosing box once, sort candidate slice indices, and preserve original particle ownership order and void-first classification. Worker scratch retains ray hits. `particle_at_prepared` returns the first acceptance id and bbox-test count, reduced without shared atomics. The test-only original particle scan is the differential oracle. Output phase/id arrays and file schema are unchanged; output is now slab-streamed (below).

### Slab-streamed label output (PERF-12, 2026-09-25)

`write_voxel_labels` no longer allocates the phase and particle-id volumes whole. `write_label_stacks` opens both TIFFs, prepares `LabelQuery` once, and for each z-slab of `max(1, LABEL_SLAB_VOXELS / (nx*ny))` slices fills two reused slab buffers with `fill_slab` (same 1024-voxel tiles, void first, lowest candidate index wins) and appends them through `TiffPageEncoder`, the page loop `save_tiff_or_folder` uses. Peak label memory is two slab buffers (at most ~64 MiB of `i64` for large slices, or one slice each when a slice alone exceeds the target) instead of `2 * 8 * nx*ny*nz` bytes. File bytes, header, spacing/origin and manifest order are unchanged: `slab_label_stacks_are_byte_identical_to_whole_volume_output` compares against a whole-volume `Volume3D` + `save_tiff_or_folder` reference for slab sizes 1, 2, 3, 4, 7, 29, 30, 31 and 1000 on a 28x20x30 grid with a void overlapping a particle, and the 1/2/8-worker placement label tests still pass. The printed `bbox_tests` count now also reports `slab_slices`; with a slab boundary not aligned to 1024 voxels the tile partition, and therefore that diagnostic count, can differ slightly from a whole-volume pass. On an error mid-stream both TIFFs may be left partially written.

Shape library input now uses `load_stl_hashed` so geometry and source digest come from one byte stream. Binary raw-file buffering is bounded; ASCII is parsed line by line from the same stream. Source/shell order and digests are unchanged.

### Stage timing (PERF-00)

`run_placement_in_pool` prints `[Timing] placement stage=<load|plan|place|write_outputs|report|total_in_pool> seconds=<f>`, `[Timing] placement workers=<n>` and `[Timing] placement peak_rss_bytes=<n|unavailable>` to stdout. They are never written into the record, report or CSV, so the byte-identical output comparisons across thread counts are unaffected; the report's own `elapsed` field is unchanged.
