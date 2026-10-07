# Placement checkpoints and bounded geometry

This design applies to `pack` configs with a `placement:` block. Legacy `packing:`
is unchanged. Checkpoints preserve computational state; STL/JSON visualization
exports and progress heartbeats have different roles.

## Durable state and safe points

Default-enabled `checkpoint` writes `checkpoint.json` at the configured elapsed
seconds/accepted-particle count, on cooperative interruption and at normal task
completion. Both periodic triggers can be disabled without disabling the final
snapshot. Save the state **before** large geometry exports. If an export fails,
the checkpoint still permits export regeneration.

A checksummed snapshot includes ordered accepted transforms, exact f64 volume
accumulators, rejection/class/failed-size counts, planned size population,
ChaCha12 word position, current size and consumed attempts, primary/top-up cursor,
and top-up accounting. Aggregate snapshots additionally retain completed template
members/search statistics, global proxies/membership, mixed-stage reservation,
current template-ID plan and consumed cluster attempts. A safe point follows a
fully evaluated speculative batch or a whole accepted cluster. Speculative RNG
rewind is already committed before a snapshot can observe the state.

Completed templates are reusable. An interruption **inside** template generation
exports the best feasible partial template but does not label it complete in the
checkpoint. Recovery re-generates that current template deterministically, then
continues the catalog. FCC intra-template search is not resumable (contact growth is; see below). Earlier
completed templates and accepted global clusters are never recomputed.

`serde_json` uses `float_roundtrip` so restored f64 values retain their bit
patterns. Snapshot files have a schema and SHA256 over serialized state. Write a
temporary file, flush/fsync it, retain a verified previous generation, then rename
atomically and fsync the directory on Unix. A missing/corrupt requested current
file falls back to its `*.previous.json`. Incompatibility is an error, not a reason
to silently use older state. Concurrent writers to one output directory are not
supported; use separate directories.

Periodic checkpoint I/O failure is a hard error, avoiding a misleading claim that
computation is protected. Forced process termination can lose work since the last
committed snapshot, but cannot invalidate the previous committed generation through
an incomplete temporary write. Cooperative stop still finishes the current query
batch and normal geometry export; a long query/export delays process exit.

## Explicit recovery and extension

Set `checkpoint.resume_from` to the saved path; relative paths resolve against the
YAML location. Recovery is never automatic. Remove an old output `STOP` marker.
Use a new output directory to keep previous visualization artifacts.

Compatibility hashes actual resolved physical config, source bytes, histogram,
void and the exact executable. Output destinations/options, thread count and
memory/checkpoint controls may change. Physical inputs, per-particle budgets,
template construction parameters and sampling policy must match. The cumulative
total attempt budget may increase, never decrease. A global-budget stop keeps the
unfinished draw/candidate rather than falsely marking its size exhausted.

A completed checkpoint stays available even if the target was reached. Explicit
`extend: true` permits a higher global VF target and plans from the remaining real
material deficit while retaining all original transforms. Previous failed-size
counts remain reported. Aggregate extension reuses the same catalog and begins a
new primary/fallback cycle. Extending an unfinished snapshot is refused: finish it
first. A lower target is not continuation. Without extension, recovery of a
completed snapshot regenerates outputs and places nothing new.

Old pre-checkpoint `particles.json` files lack the plan/RNG/attempt cursor and
cannot be converted to exact resumable checkpoints. Current reporting elapsed time
is per invocation; attempt/material/class counts are cumulative.

## Geometry residency

Production particle records keep source identity, shared canonical source mesh,
scale/rotation/translation, bounding sphere/AABB, class, triangle range and real
volume. Cold particles hold no world triangles or world QBVH. One immutable source
shell can serve many particles; source-library memory is separate from cache
accounting. The library is re-read and validated on recovery.

Exact world geometry is reconstructed with the same f64 transform as acceptance,
never reloaded from float32 STL. A byte-accounted LRU caches mesh and `TriMesh`.
Concurrent requests serialize construction and share the cached immutable entry.
An `Arc` pins active query geometry; retained entries with active owners are not
chosen for eviction. If no idle entry fits, the new query uses unretained geometry.
Zero cache budget is valid and still performs exact checks.

The byte accounting includes conservative per-vertex/face allowances for geometry,
topology and QBVH. It is an **estimated retained-cache limit**, not a hard RSS
limit. Source meshes, candidate batches, active pins, template search, spatial
indices and output buffers also use memory. `[GeometryCache]` reports estimated
bytes, limit, entries, hits, reconstructions and evictions. No performance/RSS gain
is claimed from counters alone; measure representative release runs.

## Conservative simplified collision layer

The ordinary broad phase keeps sphere and AABB tests. Optional
`memory.simplified_collision` builds a 12-triangle oriented enclosing box from the
source bounds, scaled/rotated/transformed identically. A small outward margin
protects the enclosure. Only proven solid separation plus sufficient clearance
can bypass exact geometry. Proxy overlap/containment is ambiguous and escalates;
it never directly rejects a feasible pair. Ordinary triangle decimation without
an enclosure/error certificate cannot safely accept placement.

The simplified layer is default-off until representative benchmarks establish its
benefit. It does not change source sizes, VF or exported geometry. Full meshes
still determine surface intersection, solid nesting and gap whenever necessary.

Void containment is independent of surface proximity: a deeply enclosed particle
can have no nearby pore triangles. Unconditional centroid/actual-vertex parity
precedes the surface-bbox filter. Reverse enclosure and surface/gap checks remain.

## Outputs and verification

Particle STL streams one reconstructed member at a time in acceptance/face order,
retaining the original header, zero normals, float32 vertices and triangle ranges.
There is no accumulated all-particle merged mesh. Per-particle STL likewise avoids
building collision hierarchies. Voxel labels pin only tile candidate geometry and
build tile-local queries instead of retaining all world meshes/BVHs.

Regression coverage compares uninterrupted versus recovered particles/STL/CSV and
statistics; all three modes; completed-prefix extension; corrupted generation
fallback; modified-input refusal; cache-disabled/proxy-enabled differential results;
active pin safety; and deep-pore rejection. Existing placement/void/label/aggregate
regressions protect independent collision and output contracts.


## Importing an existing assembly for a new fill task

`placement.initial_particles` accepts `record`, `report`, and `existing_gap`.
This starts a new individual-placement task from a frozen flat record; it is not
an exact resume of the earlier RNG/engine cursor. It can retain a population made
by an older executable without weakening checkpoint executable checks.

The record digest must match its original report. Check schema, frame, source
order and hashes, frozen pore hash, finite transforms, source-derived diameter,
volume, triangle ranges, bounds, and counts. Reconstruct each particle from f64
source transforms and independently validate strict wall, pore containment, and
real neighbour gaps at the declared `existing_gap`. Preserve all accepted
transforms and order. New candidates keep the normal particle gap, not the
smaller inherited gap. No proxy is imported as solid material.

The new plan covers only missing true material volume; reporting includes the
inherited particles. Initialization is transactional: no new particles are placed
until the complete inherited set validates. A stop received during validation is
honored after validation, saving the complete inherited population; original
files remain unchanged. Checkpoints created by this new task support exact resume
and completed extension. Initial record/report bytes are included in compatibility.
Use separate output files. This mode requires checkpoints, individual placement,
strict boundaries, feasible_uniform or free_space_guided sampling and forbidden pore crossing.

Schema 2 additionally preserves guided cells and original failed draws. Explicit
remaining-plan transfer can read schema 1/2 while exact recovery retains strict
executable identity. See [free-space guidance](free-space-guided-placement.md).

Contact-growth templates additionally checkpoint the pending member plan, committed/best poses and insertion/relaxation cursors. The unfinished transaction is replayed on resume; completed work inside the template is retained. FCC still rebuilds its interrupted current template.
