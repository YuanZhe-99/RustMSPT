# Spatial-Grid-Accelerated Collision Detection

RustMSPT's packing and optimization pipelines repeatedly ask the same question: "does this
candidate particle overlap, or come too close to, any particle already placed in the box?"
Answering that question exactly — via triangle-mesh intersection tests — is the correct but
expensive path. This document explains the two-tier strategy the codebase uses to make that
question cheap to ask thousands or millions of times: a uniform **spatial grid** that narrows
the candidate set down from "all particles" to "particles that could plausibly be close," and an
exact **parry3d**-based narrow phase that only runs on that narrowed set.

## The problem: O(N²) collision checks don't scale

A naive packing or simulated-annealing (SA) optimization loop checks every candidate particle
against every other particle already in the domain. With `N` particles present, a single
placement or move step costs `O(N)` exact mesh intersection tests, and a full pass over `N`
particles (as happens once per SA sweep) costs `O(N²)`. Exact mesh intersection itself is not
free — it invokes `parry3d`'s BVH-accelerated triangle-mesh query — so at packing densities of
hundreds to thousands of particles, brute-force pairwise checking becomes the dominant cost of
the whole pipeline.

The fix is standard in collision-detection literature: reject the overwhelming majority of pairs
cheaply using spatial locality, and only pay for exact geometry tests on the small number of
pairs that are actually close enough to matter.

## `SpatialGrid`: uniform-cell broad phase

`SpatialGrid` (`src/geometry/spatial.rs`) implements a classic uniform grid ("bucket grid") over
the packing domain:

- The domain (`box_bounds`) is divided into a regular lattice of cubic cells of a fixed
  `cell_size`, with grid dimensions `nx × ny × nz` computed by dividing each axis extent by the
  cell size and rounding up (`SpatialGrid::new`).
- Each cell stores a `Vec<usize>` of item indices — a bucket. Particles are inserted with
  `insert`, which maps the particle's bounding box onto the range of cells it overlaps (via
  `point_to_cell_clamped` on both corners) and pushes the particle's index into every one of those
  cells. A particle whose bounding box spans multiple cells is registered in all of them.
- `build` is a convenience constructor that inserts a whole batch of `(index, bbox)` pairs at once
  — this is what the optimize pipeline calls to (re)build the grid from scratch each time particle
  positions have moved enough to invalidate it.
- Neighbor lookup is `query_neighbors` (exact bbox overlap) or, more commonly,
  `query_neighbors_with_margin`, which pads the query bbox's cell range by enough cells to cover
  an additional `margin` distance (used to satisfy `min_neighbor_distance` constraints, not just
  strict overlap). Both walk only the cells the query bbox (plus margin) touches, collect the
  indices found there, and deduplicate — an `O(k)` operation where `k` is the number of particles
  actually near the query, independent of `N`.

Query deduplication preserves the first encounter in the original x/y/z cell and bucket order.
Small results use a linear scan; immediately upon reaching 256 unique IDs, including within the first bucket, a query-local hash membership set takes over.
The set is never iterated, supports sparse `usize` IDs, and is not shared between concurrent queries.
For larger results the expected deduplication work is linear in visited bucket references, including
duplicates, rather than their count times the number of unique neighbors. Reverse memberships support incremental updates, and caller-owned scratch retains query capacities.

### Choosing the cell size

Cell size controls the grid's effectiveness: too small and a single particle spans many cells
(inflating `insert` cost and bucket duplication); too large and each cell holds many unrelated
particles (degrading toward brute force). `estimate_cell_size` uses a simple, robust heuristic: it
scans every particle's bounding box and takes the **maximum extent along any axis across all
particles** (floored at `1.0`). Sizing cells to the largest particle's footprint guarantees that
any two particles that actually touch will always be found within the query bbox's immediate cell
neighborhood — no particle can "hide" outside the search window because no particle is bigger than
a cell.

## Two-tier collision: broad phase, then narrow phase

The grid only answers "who might be close?" — it says nothing about actual mesh geometry. Exact
collision resolution is layered on top in `src/geometry/collision.rs`, and it is itself two-tiered
internally:

1. **AABB reject.** `mesh_collision_exact_prepared` first checks `bbox_overlaps` on the two
   particles' precomputed axis-aligned bounding boxes. If the boxes don't overlap, the meshes
   cannot possibly intersect, and the function returns `false` immediately — no `parry3d` call is
   made. `mesh_distance_exact_prepared` does the analogous thing with `bbox_distance`: if the boxes
   are already separated by a known distance, that lower bound is usable directly, and exact
   distance computation is only invoked when the boxes overlap (distance would otherwise be `0.0`
   trivially or require a tighter bound).
2. **Exact narrow phase.** Only when the cheap AABB test cannot resolve the answer does the code
   fall back to `parry3d_f64::query::intersection_test` / `query::distance` on the actual
   `TriMesh` shapes (built via `to_parry_trimesh`), which perform triangle-level BVH-accelerated
   queries. Missing bounding boxes or shapes are treated conservatively — the functions return
   `true` (possible collision) / a permissive distance rather than silently ignoring the pair.
3. **Nesting.** `mesh_solids_nested_prepared` asks whether one closed solid lies wholly inside the
   other, which neither stage above can see.

### Why nesting needs its own test

`intersection_test` answers a question about *surfaces*, and two closed surfaces with one wholly
inside the other never cross. `query::distance` then measures across the space between them and
reports a comfortable clearance: a sphere of radius 1 centred inside one of radius 3 reads as
`1.96` apart. A packing loop that asks only those two accepts the arrangement, and duly places
particles inside other particles — which is what `pack` did through v0.2.0, on both engines, until
the placement engine's own outputs were re-measured downstream.

`mesh_solids_nested_prepared` costs a box comparison for almost every pair: a solid inside another
has its bounding box inside the other's, so anything else returns immediately. Only when one box
does contain the other is a single ray cast needed — `trimesh_contains_point`, the same parity walk
`VoidIndex::contains_point` uses, with the same ray direction and hit tolerance as
`s2::point_inside_mesh`. **One vertex settles it**, given that the surfaces do not intersect: a
closed shell strictly inside another has *all* of its vertices inside it. A vertex, never the
centroid — a non-convex shell's centroid can sit outside its own solid, in a concavity another
particle may legitimately occupy.

`mesh_collision_exact_prepared` is the surface test **or** the nesting test, in that order, and it
is what "these two meshes collide" means everywhere in the crate: the solids share space, not that
the surfaces cross.

So the full pipeline for any given candidate-vs-existing pair is: **grid cell lookup (find
candidates) → AABB overlap/distance test (cheap reject) → exact parry3d intersection/distance
(authoritative answer)**. Each stage filters out the pairs the next, more expensive stage doesn't
need to see.

`mesh_collision_exact` / `mesh_distance_exact` are simply convenience wrappers that compute the
bbox and `TriMesh` on the fly and call the `_prepared` variants — useful when shapes aren't already
cached, at the cost of rebuilding the `TriMesh` on every call.

## Periodic boundaries: ghost images

Boundary mode 3 (periodic crossing) allows a particle to straddle a domain edge and effectively
wrap around to the opposite side. A naive check against neighbors' primary positions alone would
miss collisions that only become apparent once you account for wraparound — for example, a
particle near the box's `+x` face may actually overlap a neighbor near the `-x` face once you
consider that they are adjacent under periodic boundary conditions.

`generate_periodic_ghosts` (`src/geometry/collision.rs`) addresses this directly: for each of the
26 non-zero combinations of `{-1, 0, +1}` shifts along x, y, and z (a full 3×3×3 neighborhood minus
the identity shift), it translates a copy of the mesh by `± box_size` along the corresponding axes
and keeps the copy only if its shifted bounding box still overlaps the original domain
(`box_bounds`). In practice this means a particle only spawns ghost images near the specific
face(s), edge(s), or corner it is close to — a particle deep in the box's interior produces no
ghosts at all, since none of the 26 shifted copies would overlap the domain.

These ghost meshes are then treated as ordinary collidable geometry: a candidate is checked against
both the real neighbor and its relevant ghost images, and a candidate's own ghost images are
checked against real neighbors, so that any collision arising purely from periodic wraparound is
caught.

## How this composes in the pipelines

Optimize and legacy pack both use cached geometry and spatial candidates:

- **`run_sa_island`** builds the initial grid, updates only the accepted particle's membership, and leaves it unchanged on rejection. Whole-population migration rebuilds the grid. Prepared collision geometry and merged vertex ranges are retained between proposals.
- **`PackPipeline::run`** caches each accepted particle as a `PackCollider` and each mode-3 periodic image as a `(particle_id, shift)` descriptor whose collider is built only when a query's bbox can reach it. Small populations use a cached direct scan; larger populations query the incremental grid, then apply bbox and exact solid checks. The broad phase never changes a decision: a shifted bbox equals the translated mesh's bbox bit for bit. Positive clearance uses a fused solid-distance predicate. Proposal RNG and acceptance remain sequential.

## Cross-references

- [geometry-core.md#spatial.rs](../reference/geometry-core.md#spatialrs)
- [geometry-volume-collision.md](../reference/geometry-volume-collision.md)
- [pipeline-optimize.md#run_sa_island](../reference/pipeline-optimize.md#run_sa_island)
- [pipeline-packing.md](../reference/pipeline-packing.md)

### Incremental grid acceptance (PERF-10/13)

`SpatialGrid` retains reverse item-to-cell membership. `remove(idx)` removes all insertions of that id and preserves remaining bucket order; `update(idx, Option<BoundingBox>)` replaces membership or removes the item. Query order remains first encounter, but moving an item appends it in its new buckets, so consumers must not assume rebuild order. Candidate sets match a full rebuild. Optimize queries the current grid before evaluating a proposal, updates one membership only after acceptance, and restores the original prepared particle directly on rejection. Whole-population migration still rebuilds the grid. Repeated inserts retain their old semantics and removal clears every copy. Reverse membership consumes additional memory proportional to inserted cell references.

### Legacy pack cache update

The earlier full-scan description above records the original implementation. Pack now caches accepted particle/periodic-image bbox and TriMesh data and incrementally indexes them. Small populations use a cached direct scan; larger populations use spatial candidates. Positive clearance uses one cached solid-distance predicate instead of separate collision/minimum-distance scans. No accepted geometry or ghosts are cloned per proposal. Candidate order, steering and clipped-volume acceptance remain unchanged.


PERF-13 dense-bucket correction: membership switches within a bucket at 256 unique candidates, bounding the initial linear phase even for a single extremely dense bucket. Order and exclusions are unchanged.
