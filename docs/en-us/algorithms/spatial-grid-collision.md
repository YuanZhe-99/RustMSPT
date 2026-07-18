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

The two consumers of this machinery differ in how (and whether) they use `SpatialGrid`:

- **`pipeline/optimize.rs::run_sa_island`** builds a `SpatialGrid` once at the start of each SA
  island run, sized via `estimate_cell_size` over all particle bounding boxes, and queries it with
  `query_neighbors_with_margin` before falling back to `mesh_collision_exact_prepared` /
  `mesh_distance_exact_prepared` for the narrowed candidate set. Because SA repeatedly perturbs
  particle positions (translate/rotate, move-toward-neighbor, or random reposition), the grid is
  invalidated whenever a move is accepted and is rebuilt (`SpatialGrid::build`) at those points, as
  well as periodically elsewhere in the loop. Ghost meshes from `generate_periodic_ghosts` are
  folded into the same broad/narrow-phase treatment for boundary mode 3. This is the case the
  `SpatialGrid` was built for: many repeated neighbor queries against a population whose positions
  keep changing, where amortizing an `O(N)` grid rebuild against many `O(k)` queries is a clear win
  over `O(N)` linear scans per query.

- **`pipeline/pack.rs::PackPipeline::run`**, by contrast, does **not** use `SpatialGrid` at all.
  Its collision-check loop against `placed` particles (and, in mode 3, their periodic ghosts) calls
  `mesh_collision_exact` / `mesh_distance_exact` directly over the full `collision_set`, using
  Rayon's `par_iter` to parallelize the scan across CPU cores, with an inline `bbox_distance`
  short-circuit per pair before falling back to the exact mesh test. This is consistent with
  packing's placement model: particles are placed once, sequentially (one accept/reject decision
  at a time, growing the placed set incrementally), so there is no repeated-query-against-a-fixed-
  population pattern to amortize a grid rebuild against — each newly placed particle only ever
  needs to be checked once, and the `Rayon`-parallelized brute-force scan over the (potentially
  large but not quadratically revisited) `placed` list is the actual code path in production, not
  a grid-backed one.

## Cross-references

- [geometry-core.md#spatial.rs](../reference/geometry-core.md#spatialrs)
- [geometry-volume-collision.md](../reference/geometry-volume-collision.md)
- [pipeline-optimize.md#run_sa_island](../reference/pipeline-optimize.md#run_sa_island)
- [pipeline-packing.md](../reference/pipeline-packing.md)
