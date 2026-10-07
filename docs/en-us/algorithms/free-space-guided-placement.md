# Free-space-guided individual placement

`position.mode: free_space_guided` is an optional geometry-guided search for
individual placement, including fill tasks imported from clustered populations.
`feasible_uniform` stays the default. This first version requires strict walls,
forbidden pore crossing, enabled checkpoints, and disabled aggregates.

## Candidate search and safety

A bounded coarse lattice stores uncertain cells. A stable priority queue ranks
real signed clearance at each cell centre, cell extent and failures for the
current logical size draw. Centre scores use actual particle meshes and frozen
pore surfaces through the bounded geometry cache. AABBs query neighbours; they
are never solid occupancy. Neither a poor centre score nor an insufficient
circumsphere clearance excludes an entire cell or rejects a particle.

Representative proposals compare the centre and six bounded axis-face probes
using real clearance, then use bounded perturbations for other visits. This
helps find thin cavities crossing coarse-cell boundaries. All extra score queries
are counted separately from actual geometry proposals. Long lazy scoring emits
`free_space_search` progress heartbeats. A configurable fraction
uses global exploration. Failed sites can split into eight children, subject to
minimum size and both cell and memory caps. When growth stops, uncertain cells
remain searchable. New particles invalidate nearby capped-distance scores only.
Failure penalties reset at each new logical draw. Every candidate still passes
the ordinary exact intersection, bidirectional containment, wall and gap checks.
No accepted particle moves and no size is replaced by a new random draw.

Guided proposals commit serially; worker count cannot change queue mutations or
acceptance order. Source and orientation retain seeded sampling. Translation
limits use the rotated actual vertex bounds. Reports name the position sampler
`adaptive_real_geometry_cavity_guidance`; this is not uniform RSA. Counters in
`stop_detail.free_space` distinguish scoring work, guided/exploration proposals,
acceptances and refinements. Scoring queries are extra work beyond the configured
actual geometry-proposal budget, so equal attempt counts do not imply equal time.

## Bounds and recovery

`free_space` defaults: coarse cell 8, minimum cell 1, max 250000 cells, estimated
index budget 128 MiB, 8 candidates per location, exploration fraction 0.10, local
refinement enabled. Lengths use the domain's existing coordinate units. Capacity
is the smaller of max_cells and budget bytes / 512 (a conservative per-cell
allowance including derived structures). This is an estimated index-memory cap,
not a process RSS limit. Initial coarse lattice must fit; growth never drops
uncertain cells. Source/active-query geometry uses separate memory.

Checkpoint schema 2 stores cell IDs, bounds, scores, versions, current-draw visits
and failures, statistics, RNG position and the ordinary size/attempt cursor.
Heaps and root memberships rebuild deterministically. Lazy scoring polls stop
every 64 score computations; an interrupted uncommitted proposal rewinds its RNG
variates while retaining completed scores. Save on interruption, budget stop and
completion. Exact recovery still requires the same executable and physical inputs.

## Transferring an original remaining plan

An explicit new fill task may add `initial_particles.pending_checkpoint` and
`retry_failed` (default true) to its frozen record/report import. It validates
checkpoint checksum, original report digest, accepted records and source IDs.
Schema 1 and 2 inputs support an individual primary plan with the same target,
without earlier extensions or top-up batches. This is a new algorithm/task, not
an exact resume of an older executable.

Subtract accepted diameters/classes as a counted multiset from consumed original
draws, verify failed counts, then import the untouched pending tail followed by
original failed sizes when enabled. Duplicate sizes preserve multiplicity. Keep
this order rather than re-sorting or sampling replacements. If reconciliation
fails, reject the transfer. Record/report/checkpoint bytes enter new compatibility
hashing. A global budget stop with an unfinished draw reports budget exhaustion;
failed-size statistics remain separate from unprocessed sizes.

See [configuration](../reference/config.md),
[state contracts](../reference/pipeline-placement-state.md), and
[worked configuration](../examples/pack-placement.md).
