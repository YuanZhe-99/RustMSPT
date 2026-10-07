# Placement state and geometry reference

Read [the design](../algorithms/placement-checkpoints-and-memory.md) first.

## Index

| Item | Source | Contract |
|---|---|---|
| `CheckpointSpec::default` | `src/config/placement.rs` | Final/interrupt saves enabled; time=60 s, accepted count=1000, no recovery/extension. |
| `PlacementMemorySpec::default` | `src/config/placement.rs` | 256 MiB estimated retained cache; conservative triangle proxy screen disabled. |
| `IndividualCursor` | `src/pipeline/placement_checkpoint.rs` | Primary plan, active population/index/attempts, phase, top-up accounting and extension count. |
| `SavedState` / `Snapshot` / `Envelope` | same | Ordered records/accumulators plus cursor, mode, compatibility identity, RNG position and checksum. |
| `Store::new` | same | Hash actual resolved physical config/input bytes/executable; create periodic-save bookkeeping. |
| `Store::due` | same | Time/count trigger, evaluated only at safe points. |
| `Store::load` | same | Verify checksum/schema/compatibility/budgets/target and fallback on corrupt/missing current generation. |
| `Store::save` | same | Durable temporary/previous/current file commits; errors propagate. |
| `snapshot` | same | Capture records and exact accumulators without constructing world meshes. |
| `save` | same | Save a forced/due snapshot; no JSON payload allocation when disabled/not due. |
| `restore` | same | Validate source/order/ranges/counter lengths, restore RNG and reconstruct ordered spatial index with cold geometry handles. |
| `place_resumable` | `src/pipeline/placement.rs` | Resume primary/top-up draws at batch boundaries; preserve per-draw attempt consumption. |
| `AggregateCursor` / `save_aggregate` | `src/pipeline/placement_aggregates.rs` | Completed-template catalog and global mixed-stage/attempt cursor; serialize only when due. |
| `GeometryCache::new` | `src/pipeline/placement_geometry.rs` | Byte budget, including valid zero-retention mode. |
| `GeometryCache::get` | same | Serialized construction, LRU hits, idle-only eviction and active immutable pins. |
| `GeometryCache::summary` | same | Estimated retained bytes and hit/reconstruction/eviction diagnostic counters. |
| `GeometryHandle::new` | same | Store shared canonical source and exact immutable world transform. |
| `GeometryHandle::get` | same | Acquire/pin world mesh and exact `TriMesh`. |
| `GeometryHandle::reconstruct` | same | f64 transform identical to original placement acceptance. |
| `GeometryHandle::proxy` | same | Conservative 12-triangle oriented outer box; no particle/VF rescaling. |
| `PlacedParticle::prepared` | `src/pipeline/placement_feasibility.rs` | Cached production geometry or standalone direct-fixture geometry. |
| `write_particles_stl` | `src/pipeline/placement_geometry.rs` | Stream exact per-particle geometry without merged mesh; checked u32 STL triangle count. |
| `particle_at_tile` | `src/pipeline/placement_labels.rs` | Tile-local pinned query views with acceptance-order ownership. |

Checkpoint files are separate from progress summaries. Runtime caches/QBVHs are
not serialized. Explicit completed extension retains original placement records
and adds a new plan; saved failed-size history remains visible.


## Initial assembly import

`placement_initial::load` validates record/report hashes, schema, frame, source
identities, pore identity and every inherited real particle, then initializes
ordered acceptance/material/class counts. The plan contains inherited plus new
draws but the placement cursor attempts only the deficit. The import is a new
task and never accepts incompatible old RNG checkpoints. `Store::new` hashes
the initial record/report bytes, so changing either refuses subsequent recovery.

## Free-space guidance contracts

Read [the algorithm](../algorithms/free-space-guided-placement.md) first.

| Item | Source | Contract |
|---|---|---|
| `FreeSpaceSpec::default` | `src/config/placement.rs` | Optional bounded search controls; uniform sampling remains default. |
| `free_space::Index::new` | `src/pipeline/placement_free_space.rs` | Allocate unknown coarse lattice within cell/estimated memory caps. |
| `Index::score` | same | Real mesh/pore signed centre clearance; AABBs only query/cull. |
| `Index::propose` | same | Seeded centre/jitter or exploration; None on cooperative stop during lazy scoring. Caller rewinds proposal RNG on None. |
| `Index::feedback` | same | Commit one proposal outcome, bounded subdivision; never hard-exclude ambiguous cells. |
| `Index::inserted` | same | Invalidate local scores after accepted real material. |
| `Index::begin_draw` | same | Reset temporary penalties per logical size identity. |
| `Index::rebuild` | same | Rebuild heap/memberships exactly after recovery. |
| `Index::summary` | same | Estimated index memory and separate score/proposal statistics. |
| `checkpoint::import_remaining_plan` | `src/pipeline/placement_checkpoint.rs` | Verify schema1/2 checksum/report/accepted records; preserve pending then optional failed size multiset, no new draws. |

Snapshot schema 2 stores guided cells and failed draws. Derived heap/cache state
is omitted. Individual guided proposals commit serially across all thread counts.
