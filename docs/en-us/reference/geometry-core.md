# Geometry Core Reference

This page documents the core geometry primitives in `src/geometry/`: axis-aligned bounding boxes and packing-boundary checks (`bbox.rs`), mesh manipulation utilities (`mesh_ops.rs`), and the uniform spatial grid used for neighbor queries (`spatial.rs`). It also covers `src/geometry/mod.rs`, which serves purely as the module hub for the wider `geometry` package.

## Index

| Function | Location | Summary |
|---|---|---|
| `RenderProjection` | `src/geometry/render.rs` | Orthographic or perspective projection. |
| `RenderCameraSpec` | `src/geometry/render.rs` | User camera/framing inputs. |
| `RenderCamera` | `src/geometry/render.rs` | Validated camera basis, projection, and clipping data. |
| `RenderSettings` | `src/geometry/render.rs` | Shared CPU/GPU appearance settings. |
| `parse_render_vec3` | `src/geometry/render.rs` | Validates a finite three-element config vector. |
| `parse_render_projection` | `src/geometry/render.rs` | Parses orthographic/perspective projection names. |
| `build_render_camera` | `src/geometry/render.rs` | Builds and auto-frames a validated camera. |
| `RenderCamera::ray_for_pixel` | `src/geometry/render.rs` | Generates a world-space pixel-center ray. |
| `RenderCamera::view_proj_matrix` | `src/geometry/render.rs` | Builds a wgpu-compatible view-projection matrix. |
| `render_mesh_cpu` | `src/geometry/render.rs` | Rayon-parallel QBVH nearest-hit renderer. |
| `mesh_bbox` | `src/geometry/bbox.rs:8` | Axis-aligned bounding box of a mesh. |
| `bbox_overlaps` | `src/geometry/bbox.rs:28` | Strict overlap test between two bounding boxes. |
| `bbox_distance` | `src/geometry/bbox.rs:38` | Minimum Euclidean distance between two bounding boxes. |
| `check_boundary_constraints_mode` | `src/geometry/bbox.rs:72` | Validates a mesh's placement against packing boundary mode rules. |
| `mesh_centroid` | `src/geometry/mesh_ops.rs:5` | Arithmetic centroid of mesh vertices. |
| `vec_norm` | `src/geometry/mesh_ops.rs:19` | Euclidean length of a vector. |
| `merge_meshes` | `src/geometry/mesh_ops.rs:24` | Combines multiple meshes into one, remapping face indices. |
| `split_mesh_into_granules` | `src/geometry/mesh_ops.rs:47` | Splits a mesh into connected components (BFS over shared vertices). |
| `translate_mesh` | `src/geometry/mesh_ops.rs:120` | Translates all mesh vertices by a delta vector, in place. |
| `move_mesh_to_target_center` | `src/geometry/mesh_ops.rs:125` | Moves a mesh so its centroid matches a target position. |
| `wrap_mesh_centroid_to_box` | `src/geometry/mesh_ops.rs:136` | Wraps a mesh's centroid into a box under periodic boundary conditions. |
| `scale_mesh` | `src/geometry/mesh_ops.rs:157` | Uniformly scales mesh vertices about the origin, in place. |
| `mesh_surface_area` | `src/geometry/mesh_ops.rs:176` | Total surface area of a mesh (sum of triangle areas). |
| `rotate_mesh_around_center` | `src/geometry/mesh_ops.rs:195` | Rotates a mesh about its centroid using Rodrigues' rotation formula. |
| `box_mesh` | `src/geometry/mesh_ops.rs:216` | Builds a triangulated box mesh from a `BoundingBox`. |
| `SpatialGrid::new` | `src/geometry/spatial.rs:22` | Constructs an empty uniform grid over a box with given cell size. |
| `SpatialGrid::insert` | `src/geometry/spatial.rs:40` | Inserts an item index into every cell its bbox overlaps. |
| `SpatialGrid::build` | `src/geometry/spatial.rs:80` | Constructs and populates a grid from a batch of (index, bbox) pairs. |
| `SpatialGrid::query_neighbors` | `src/geometry/spatial.rs:89` | Finds candidate neighbor indices overlapping a query bbox. |
| `SpatialGrid::query_neighbors_with_margin` | `src/geometry/spatial.rs:99` | Margin query with first-encounter ordering and adaptive linear/hash deduplication. |
| `SpatialGrid::point_to_cell_clamped` | `src/geometry/spatial.rs:157` | Maps a point to grid cell coordinates, clamped to grid bounds. |
| `SpatialGrid::point_to_cell` | `src/geometry/spatial.rs:167` | Maps a point to grid cell coordinates, unclamped. |
| `estimate_cell_size` | `src/geometry/spatial.rs:176` | Heuristically picks a `SpatialGrid` cell size from a set of bboxes. |
| `SpatialGrid::remove` | `src/geometry/spatial.rs:59` | Remove all item cell references. |
| `SpatialGrid::update` | `src/geometry/spatial.rs:72` | Replace one item membership. |
| `SpatialQueryScratch` | `src/geometry/spatial.rs:5` | Retained neighbors and membership storage. |
| `SpatialGrid::query_into` | `src/geometry/spatial.rs:136` | Fill reusable query scratch. |
| `GridStats` | `src/geometry/spatial.rs:11` | Bucket occupancy summary: buckets, non-empty buckets, max/mean occupancy, memberships, items. |
| `GridStats::summary_line` | `src/geometry/spatial.rs:22` | Format a one-line `[GridStats] <label> ...` log line. |
| `SpatialGrid::stats` | `src/geometry/spatial.rs:181` | One pass over buckets returning `GridStats`. |
| `map_vertices_centroid` | `src/geometry/mesh_ops.rs:265` | Vertex map plus post-map centroid, bit-identical to map_vertices + mesh_centroid; serial branch is one fused pass. |
| `map_vertices` | `src/geometry/mesh_ops.rs:162` | Serial or parallel independent vertex mapping. |

## Module role: `geometry/mod.rs`

`src/geometry/mod.rs` declares the submodules of the `geometry` package (`bbox`, `collision`, `forging`, `mesh_ops`, `metrics`, `s2`, `spatial`, `volume`) and re-exports their public items at the `geometry::` path. It contains no functions of its own — it exists purely so callers can write `crate::geometry::mesh_bbox` etc. instead of reaching into individual submodules. The `gpu` feature gate also conditionally re-exports GPU-accelerated `s2` variants here.

## render.rs

`RenderProjection` selects orthographic or perspective projection. `RenderCameraSpec` carries focus/view/up, projection/FOV/distance, padding, and resolution; `build_render_camera(mesh, spec)` validates it, derives an orthonormal basis, auto-frames the mesh bbox, and returns `RenderCamera`. `parse_render_vec3` and `parse_render_projection` convert YAML values with `InvalidConfig` errors.

`RenderCamera::ray_for_pixel` samples top-row-first pixel centers. `view_proj_matrix` returns a row-major right-handed matrix with wgpu's `[0,1]` depth convention. `render_mesh_cpu` converts the mesh to parry3d `TriMesh`, casts the nearest QBVH ray per pixel in bounded Rayon pixel tasks, applies two-sided headlight shading, and returns `RenderedImage`. Private helpers normalize vectors, choose fallback up axes, enumerate bbox corners, multiply matrices, and shade channels without side effects.

See [STL Rendering](../algorithms/stl-rendering.md).

## bbox.rs

These four functions are the core geometric gates used throughout the packing and optimization pipelines: `mesh_bbox` underlies almost every other spatial computation (it's the input to `bbox_overlaps`, `bbox_distance`, `SpatialGrid` insertion, and `check_boundary_constraints_mode`); `bbox_overlaps`/`bbox_distance` provide the coarse collision pre-filter before exact mesh-vs-mesh checks; and `check_boundary_constraints_mode` is the acceptance test a candidate particle placement must pass before it is committed during packing or optimization.

#### mesh_bbox

- **Signature:** `pub fn mesh_bbox(mesh: &Mesh) -> Option<BoundingBox>`
- **Source:** `src/geometry/bbox.rs:8`
- **Purpose:** Computes the axis-aligned bounding box enclosing all vertices of a mesh.
- **Parameters:**
  - `mesh` — the mesh to bound.
- **Returns:** `Some(BoundingBox)` with `min`/`max` corners spanning every vertex, or `None` if the mesh has no vertices.
- **Side effects:** None.
- **Notes:** This is the primary building block for coarse collision detection and boundary checks elsewhere in the geometry module.

#### bbox_overlaps

- **Signature:** `pub fn bbox_overlaps(a: BoundingBox, b: BoundingBox) -> bool`
- **Source:** `src/geometry/bbox.rs:28`
- **Purpose:** Tests whether two bounding boxes overlap on all three axes.
- **Parameters:**
  - `a`, `b` — the two bounding boxes to compare.
- **Returns:** `true` if the boxes have positive overlap on every axis.
- **Side effects:** None.
- **Notes:** The comparison is strict (`<`/`>`), so boxes that merely touch along a face, edge, or corner are not considered overlapping.
- **See also:** `../algorithms/spatial-grid-collision.md` for how this feeds broad-phase collision detection.

#### bbox_distance

- **Signature:** `pub fn bbox_distance(a: BoundingBox, b: BoundingBox) -> f64`
- **Source:** `src/geometry/bbox.rs:38`
- **Purpose:** Computes the minimum Euclidean distance between two bounding boxes.
- **Parameters:**
  - `a`, `b` — the two bounding boxes to measure between.
- **Returns:** The straight-line distance between the nearest points of the two boxes; `0.0` if they overlap or touch on any axis (the per-axis gap is `0.0` whenever the boxes are not separated along that axis).
- **Side effects:** None.
- **Notes:** Computed per-axis (gap or zero) then combined via `sqrt(dx² + dy² + dz²)`, so it is exact for box-to-box separation, not merely an approximation.

#### check_boundary_constraints_mode

- **Signature:** `pub fn check_boundary_constraints_mode(mesh: &Mesh, box_bounds: BoundingBox, mode: u8, d1: f64, d2: f64) -> bool`
- **Source:** `src/geometry/bbox.rs:72`
- **Purpose:** Validates whether a mesh's placement inside (or across) a packing box satisfies the configured boundary mode and clearance distances.
- **Parameters:**
  - `mesh` — the candidate mesh (already positioned in world/box space).
  - `box_bounds` — the packing domain's bounding box.
  - `mode` — boundary mode: `1` = fully-inside only; `2` or `3` = periodic crossing allowed (the function treats both identically — see Notes).
  - `d1` — minimum clearance a particle must keep from the box faces when it is fully inside the box.
  - `d2` — minimum penetration/protrusion depth required when a particle crosses a boundary under periodic modes.
  - Returns `false` immediately if the mesh has no vertices (`mesh_bbox` returns `None`).
- **Returns:** `true` if the mesh satisfies the constraints for the given mode; `false` otherwise.
- **Side effects:** None.
- **Notes — boundary mode semantics:**
  - The mesh's bbox is first translated into box-local coordinates (`local_min`/`local_max` relative to `box_bounds.min`).
  - **If the mesh is fully inside the box** on all three axes: it is accepted only if it also keeps at least `d1` clearance from every face of the box (`local_min >= d1` and `local_max <= size - d1` on each axis). This check applies regardless of `mode`.
  - **If the mesh is not fully inside** (it crosses at least one boundary):
    - **Mode 1** rejects the placement outright — mode 1 permits no boundary crossing at all.
    - **Modes 2 and 3** (treated identically by this function) allow crossing, subject to per-axis checks: on any axis where the mesh crosses the low face, either the protrusion beyond `min` or the remaining depth inside must be at least `d2`, or the placement is rejected; symmetric logic applies to axis crossings at the high face; axes that stay fully inside the box bounds still enforce the `d1` interior clearance.
    - In short: mode 1 = strictly non-periodic packing (particles must sit entirely inside the domain with `d1` margin); modes 2/3 = periodic packing where particles may straddle the domain boundary as long as the crossing depth is at least `d2` on both sides.
- **See also:** `../algorithms/spatial-grid-collision.md`; consult config docs for how `packing.mode` / optimization boundary settings map to the `mode` parameter here.

## mesh_ops.rs

Utilities for constructing, transforming, and measuring `Mesh` values. These are used pervasively by the packing, optimization, and forging pipelines to position, resize, and analyze particle geometry.

#### mesh_centroid

- **Signature:** `pub fn mesh_centroid(mesh: &Mesh) -> Vec3`
- **Source:** `src/geometry/mesh_ops.rs:5`
- **Purpose:** Computes the arithmetic mean (centroid) of all mesh vertex positions.
- **Parameters:**
  - `mesh` — the mesh whose centroid is computed.
- **Returns:** A `Vec3` centroid; the zero vector for an empty mesh.
- **Side effects:** None.
- **Notes:** This is a vertex-average centroid, not a volume- or area-weighted centroid.

#### vec_norm

`#### vec_norm` — `pub fn vec_norm(v: Vec3) -> f64` — `src/geometry/mesh_ops.rs:19`. Euclidean length of a vector. No side effects.

#### merge_meshes

- **Signature:** `pub fn merge_meshes(meshes: &[Mesh]) -> Mesh`
- **Source:** `src/geometry/mesh_ops.rs:24`
- **Purpose:** Combines multiple independent meshes into a single mesh.
- **Parameters:**
  - `meshes` — slice of meshes to merge, in order.
- **Returns:** A new `Mesh` whose vertex buffer is the concatenation of all input vertex buffers, and whose faces are the concatenation of all input faces with indices offset to point into the combined vertex buffer.
- **Side effects:** None (does not mutate inputs).
- **Notes:** Inverse-ish counterpart to `split_mesh_into_granules`, though merging does not reconstruct adjacency — it purely concatenates.

#### split_mesh_into_granules

- **Signature:** `pub fn split_mesh_into_granules(mesh: &Mesh) -> Vec<Mesh>`
- **Source:** `src/geometry/mesh_ops.rs:47`
- **Purpose:** Splits a single mesh into its connected components ("granules"), where connectivity is defined by shared vertices between triangles.
- **Parameters:**
  - `mesh` — the mesh to decompose.
- **Returns:** A `Vec<Mesh>`, one entry per connected component, each with its own compact, independently-indexed vertex buffer (no shared indices with the source mesh or other components). Returns an empty `Vec` if the input mesh has no faces or no vertices.
- **Side effects:** None.
- **Algorithm:**
  1. Build a CSR vertex-to-face adjacency: count each vertex's references, prefix-sum the counts into offsets, then fill one contiguous face-id array in face order (a face referencing a vertex twice appears twice, exactly as the former per-vertex `Vec` did).
  2. Maintain a `visited` flag per face. For each unvisited face, run a breadth-first search over a reused `Vec` with a head cursor: the queue itself, in pop order, is the component's face list. Neighbours are enqueued in CSR order, so the visit order equals the former `VecDeque` BFS.
  3. Remap the component's vertices in face order into a compact local index space using a per-vertex stamp array (stamp = component number) instead of a `HashMap`, and emit a new `Mesh`.
  4. Repeat until every face has been visited; components are pushed into the output vector in the order their starting face was first encountered.
  The former `Vec<Vec>`/`VecDeque`/`HashMap` implementation is kept as the `#[cfg(test)]` oracle `split_mesh_into_granules_reference`; `csr_split_matches_reference` compares whole output meshes (component order, face order, vertex remap) on empty, isolated-vertex, shared-vertex, degenerate, many-small, one-large and face-shuffled inputs. Release benchmark (`split_benchmark`, median of 5): 20,000 shuffled boxes 0.139 s -> 0.048 s, 20,000 ordered boxes 0.043 s -> 0.014 s, one 327,680-face icosphere 0.104 s -> 0.044 s.
- **Notes:** Two triangles are considered connected if they share *any* vertex (not necessarily an edge), so this is vertex-adjacency BFS, not edge-adjacency. This is the standard connected-component splitter used throughout the packing, optimization, and split-filter stages of the pipeline — e.g., after forging or clipping operations that may fracture a single input mesh into multiple disjoint particle bodies, this function is what separates them back into individually trackable granules.
- **See also:** `../algorithms/mesh-clipping-volume-fraction.md` for how downstream volume/clipping operations consume the resulting per-granule meshes.

#### translate_mesh

- **Signature:** `pub fn translate_mesh(mesh: &mut Mesh, delta: Vec3)`
- **Source:** `src/geometry/mesh_ops.rs:120`
- **Purpose:** Translates every vertex of a mesh by a fixed offset.
- **Parameters:**
  - `mesh` — mesh to translate, mutated in place.
  - `delta` — offset vector added to every vertex.
- **Returns:** Nothing (`()`).
- **Side effects:** Mutates `mesh.vertices` in place.

#### move_mesh_to_target_center

- **Signature:** `pub fn move_mesh_to_target_center(mesh: &mut Mesh, target: Vec3)`
- **Source:** `src/geometry/mesh_ops.rs:125`
- **Purpose:** Repositions a mesh so that its centroid lands exactly on `target`.
- **Parameters:**
  - `mesh` — mesh to move, mutated in place.
  - `target` — desired centroid position after the move.
- **Returns:** Nothing (`()`).
- **Side effects:** Mutates `mesh.vertices` in place (computes current centroid via `mesh_centroid`, then calls `translate_mesh` with the difference).

#### wrap_mesh_centroid_to_box

- **Signature:** `pub fn wrap_mesh_centroid_to_box(mesh: &mut Mesh, box_bounds: BoundingBox)`
- **Source:** `src/geometry/mesh_ops.rs:136`
- **Purpose:** Wraps a mesh's centroid back into the packing box under periodic boundary conditions, then moves the mesh to that wrapped position.
- **Parameters:**
  - `mesh` — mesh to wrap, mutated in place.
  - `box_bounds` — the periodic domain's bounding box.
- **Returns:** Nothing (`()`).
- **Side effects:** Mutates `mesh.vertices` in place.
- **Notes:** Uses Euclidean modulo (`rem_euclid`) per axis to fold the centroid coordinate back into `[box_bounds.min, box_bounds.min + size)`; an axis with zero or negative length is left unwrapped (value passed through unchanged) to avoid division-by-zero-like degenerate behavior. Used to keep particle centroids inside the nominal domain when periodic boundary modes (2/3, see `check_boundary_constraints_mode`) are in effect.

#### scale_mesh

- **Signature:** `pub fn scale_mesh(mesh: &mut Mesh, factor: f64)`
- **Source:** `src/geometry/mesh_ops.rs:157`
- **Purpose:** Uniformly scales every vertex of a mesh by `factor`, about the coordinate origin.
- **Parameters:**
  - `mesh` — mesh to scale, mutated in place.
  - `factor` — scalar multiplier applied to every vertex coordinate.
- **Returns:** Nothing (`()`).
- **Side effects:** Mutates `mesh.vertices` in place.
- **Notes:** Scaling is about the world origin, not the mesh centroid — callers that need to scale about the centroid must translate to the origin, scale, then translate back (or use `move_mesh_to_target_center` afterward).

#### mesh_surface_area

- **Signature:** `pub fn mesh_surface_area(mesh: &Mesh) -> f64`
- **Source:** `src/geometry/mesh_ops.rs:176`
- **Purpose:** Computes the total surface area of a triangulated mesh.
- **Parameters:**
  - `mesh` — the mesh to measure.
- **Returns:** Sum of each triangle's area (`0.5 * |ab × ac|`) across all faces.
- **Side effects:** None.
- **Notes:** Assumes triangle faces reference valid vertex indices; does not deduplicate degenerate or overlapping triangles.

#### rotate_mesh_around_center

- **Signature:** `pub fn rotate_mesh_around_center(mesh: &mut Mesh, axis: Vec3, angle: f64)`
- **Source:** `src/geometry/mesh_ops.rs:195`
- **Purpose:** Rotates a mesh about its own centroid by `angle` radians around an arbitrary axis, using Rodrigues' rotation formula.
- **Parameters:**
  - `mesh` — mesh to rotate, mutated in place.
  - `axis` — rotation axis; need not be pre-normalized (the function normalizes it internally).
  - `angle` — rotation angle in radians.
- **Returns:** Nothing (`()`).
- **Side effects:** Mutates `mesh.vertices` in place.
- **Notes:** No-op if `axis` has length `<= 1e-12` (degenerate/zero axis), to avoid dividing by zero during normalization. The rotation is computed relative to the mesh's centroid (`mesh_centroid`), so the mesh's centroid position is preserved; only its orientation changes.

#### box_mesh

- **Signature:** `pub fn box_mesh(bbox: BoundingBox) -> Mesh`
- **Source:** `src/geometry/mesh_ops.rs:216`
- **Purpose:** Constructs a closed, triangulated rectangular box mesh matching the given bounding box.
- **Parameters:**
  - `bbox` — the box's min/max corners.
- **Returns:** A `Mesh` with 8 vertices (box corners) and 12 triangles (2 per face, 6 faces), consistently wound.
- **Side effects:** None.
- **Notes:** Commonly used to materialize the packing domain itself as a mesh, e.g., for clipping or visualization purposes.
- **See also:** `../algorithms/mesh-clipping-volume-fraction.md`.

## spatial.rs

`SpatialGrid` is a uniform (fixed cell size) spatial hash used to accelerate neighbor and collision queries during packing and optimization, avoiding O(n²) pairwise checks against every other particle.

**Fields:**

| Field | Type | Meaning |
|---|---|---|
| `inv_cell` | `f64` | Reciprocal of the cell size (`1.0 / cell_size`); used to convert world coordinates to cell indices via multiplication instead of division. |
| `nx`, `ny`, `nz` | `usize` | Number of grid cells along each axis, each at least 1. |
| `origin` | `Vec3` | World-space origin of the grid, equal to the packing box's `min` corner. |
| `cells` | `Vec<Vec<usize>>` | Flattened 3D array (row-major, size `nx * ny * nz`) of cell buckets; each bucket holds the indices of items whose bbox overlaps that cell. |

#### SpatialGrid::new

- **Signature:** `pub fn new(box_bounds: BoundingBox, cell_size: f64) -> Self`
- **Source:** `src/geometry/spatial.rs:22`
- **Purpose:** Constructs an empty `SpatialGrid` spanning `box_bounds`, partitioned into cubic cells of (approximately) `cell_size`.
- **Parameters:**
  - `box_bounds` — the world-space region the grid covers.
  - `cell_size` — target cell edge length.
- **Returns:** A new `SpatialGrid` with all cell buckets empty. `nx`/`ny`/`nz` are computed as `ceil(size / cell_size)`, each clamped to a minimum of 1.
- **Side effects:** None (allocates a fresh `cells` vector).

#### SpatialGrid::insert

- **Signature:** `pub fn insert(&mut self, idx: usize, bbox: BoundingBox)`
- **Source:** `src/geometry/spatial.rs:40`
- **Purpose:** Registers an item (identified by `idx`) into every grid cell its bounding box overlaps.
- **Parameters:**
  - `idx` — item index to insert (typically a particle/mesh index).
  - `bbox` — the item's world-space bounding box.
- **Returns:** Nothing (`()`).
- **Side effects:** Pushes `idx` into every overlapped cell's bucket in `self.cells`. An item spanning multiple cells is duplicated across all of them.
- **Notes:** Cell range is computed by clamping `bbox.min`/`bbox.max` to grid coordinates via `point_to_cell_clamped`.

#### SpatialGrid::build

- **Signature:** `pub fn build(bboxes: &[(usize, BoundingBox)], box_bounds: BoundingBox, cell_size: f64) -> Self`
- **Source:** `src/geometry/spatial.rs:80`
- **Purpose:** Convenience constructor that creates a grid and inserts a full batch of (index, bbox) pairs in one call.
- **Parameters:**
  - `bboxes` — slice of `(index, bbox)` pairs to insert.
  - `box_bounds` — grid extent, passed to `new`.
  - `cell_size` — cell edge length, passed to `new`.
- **Returns:** A populated `SpatialGrid`.
- **Side effects:** None beyond the allocation and population of the returned grid.

#### SpatialGrid::query_neighbors

- **Signature:** `pub fn query_neighbors(&self, bbox: BoundingBox, exclude: usize) -> Vec<usize>`
- **Source:** `src/geometry/spatial.rs:89`
- **Purpose:** Finds all item indices in cells overlapping `bbox`, excluding a given index.
- **Parameters:**
  - `bbox` — query region.
  - `exclude` — index to omit from results (typically the querying item itself).
- **Returns:** `Vec<usize>` of deduplicated candidate neighbor indices.
- **Side effects:** None.
- **Notes:** Delegates to `query_neighbors_with_margin` with `margin = 0.0`.
- **See also:** `../algorithms/spatial-grid-collision.md`.

#### SpatialGrid::query_neighbors_with_margin

- **Signature:** `pub fn query_neighbors_with_margin(&self, bbox: BoundingBox, margin: f64, exclude: usize) -> Vec<usize>`
- **Source:** `src/geometry/spatial.rs:99`
- **Purpose:** Finds all item indices in cells overlapping `bbox`, expanded outward by `margin`, excluding a given index.
- **Parameters:**
  - `bbox` — query region.
  - `margin` — extra world-space distance to pad the search region by (converted to an integer cell padding via `ceil(margin * inv_cell) + 1`).
  - `exclude` — index to omit from results.
- **Returns:** `Vec<usize>` of deduplicated candidate neighbor indices (first-encounter order; linear deduplication below 256 IDs, then hash membership without iterating the set).
- **Side effects:** None.
- **Notes:** Used wherever a `min_neighbor_distance` (or similar clearance) constraint must be checked, since a neighbor separated by up to `margin` still needs to be considered even if its bbox doesn't directly overlap the query bbox. The result is a superset of true neighbors within the margin — candidates still require an exact geometric check downstream (this is a broad-phase filter only).
- **See also:** `../algorithms/spatial-grid-collision.md`.

#### SpatialGrid::point_to_cell_clamped

- **Signature:** `fn point_to_cell_clamped(&self, p: Vec3) -> (usize, usize, usize)`
- **Source:** `src/geometry/spatial.rs:157`
- **Purpose:** Maps a world-space point to grid cell coordinates, clamped so the result always indexes a valid cell.
- **Parameters:**
  - `p` — world-space point.
- **Returns:** `(cx, cy, cz)` clamped to `[0, nx-1] × [0, ny-1] × [0, nz-1]`.
- **Side effects:** None.
- **Notes:** Private helper (not `pub`); relies on `point_to_cell` for the unclamped conversion.

#### SpatialGrid::point_to_cell

- **Signature:** `fn point_to_cell(&self, p: Vec3) -> (usize, usize, usize)`
- **Source:** `src/geometry/spatial.rs:167`
- **Purpose:** Converts a world-space point into raw (unclamped) grid cell coordinates.
- **Parameters:**
  - `p` — world-space point.
- **Returns:** `(cx, cy, cz)` computed as `floor((p - origin) * inv_cell)`, each axis independently floored at 0 (points below the grid origin map to cell 0 on that axis) but **not** capped at the upper bound — a point beyond the grid's far edge can return an index `>= nx`/`ny`/`nz`.
- **Side effects:** None.
- **Notes:** Private helper (not `pub`); only `point_to_cell_clamped` should be used when indexing into `self.cells`, since raw output from this function can be out of bounds.

#### estimate_cell_size

- **Signature:** `pub fn estimate_cell_size(bboxes: &[BoundingBox]) -> f64`
- **Source:** `src/geometry/spatial.rs:176`
- **Purpose:** Heuristically picks a cell size for constructing a `SpatialGrid`, based on the largest bounding-box extent among a set of items.
- **Parameters:**
  - `bboxes` — bounding boxes of the items that will populate the grid (e.g., all particle meshes in a packing run).
- **Returns:** The maximum extent (`size.x`, `size.y`, or `size.z`) across all input boxes, floored at `1.0`. Returns `1.0` if `bboxes` is empty.
- **Side effects:** None.
- **Notes:** Sizing cells to the largest item's extent ensures that any single item spans at most a small, bounded number of cells, which keeps `insert`/`query_neighbors` calls cheap. Callers typically compute `mesh_bbox` for every particle, pass the resulting boxes here to pick `cell_size`, then build the grid via `SpatialGrid::build`.
- **See also:** `../algorithms/spatial-grid-collision.md`.

### Incremental grid acceptance (PERF-10/13)

`SpatialGrid` retains reverse item-to-cell membership. `remove(idx)` removes all insertions of that id and preserves remaining bucket order; `update(idx, Option<BoundingBox>)` replaces membership or removes the item. Query order remains first encounter, but moving an item appends it in its new buckets, so consumers must not assume rebuild order. Candidate sets match a full rebuild. Optimize queries the current grid before evaluating a proposal, updates one membership only after acceptance, and restores the original prepared particle directly on rejection. Whole-population migration still rebuilds the grid. Repeated inserts retain their old semantics and removal clears every copy. Reverse membership consumes additional memory proportional to inserted cell references.

`SpatialQueryScratch` owns reusable neighbors and hash membership storage. `SpatialGrid::query_into(bbox, margin, exclude, scratch)` clears logical contents and retains capacity, preserving first-encounter deduplication. Placement label tiles reuse it alongside their mesh-query scratch. Extremely large margins saturate cell padding rather than overflowing.

`map_vertices(&mut [Vec3], transform)` applies independent vertex maps with a serial small-input path and disjoint Rayon chunks above the cutoff. Scale and translation preserve their arithmetic and topology. The current pool controls parallel execution.

`merge_meshes` reserves final vertex and face capacities before concatenation, preserving the existing input/index order.


Dense-bucket query contract: hash promotion occurs immediately on the 256th distinct candidate, even inside the first bucket. No bucket is scanned completely with unbounded linear deduplication. The set is never iterated, preserving first-encounter ordering and sparse-ID/exclusion semantics. Both buffers remain query-local reusable scratch.

### CPU pixel task scheduling

Both nearest-hit STL rendering and prepared transparent scene rendering use disjoint contiguous pixel tasks. Images with at least one row per worker and at most 1024 pixels per worker retain row tasks, avoiding loss of parallelism from the minimum tile grain. Other images in a one-worker pool use one task; otherwise the initial grain targets four tasks per worker, clamped to 256..4096 pixels, aligning to complete rows when a row fits. Wide rows can span several tasks and short rows can share one. Each task derives its starting `(x,y)` once and advances the original integer pixel coordinates; ray arithmetic, hit ordering/compositing and serial overlays are unchanged. Scene depth and RGBA use identical task boundaries and reuse task-local hit scratch. Row-grain reference tests compare complete images under 1/2/8 workers for both projections, ragged tasks and extreme aspect ratios. Grain performance acceptance is tracked in PLAN.Performance.md §40.

| `cpu_render_tile_pixels` | `src/geometry/render.rs:381` | Bounded CPU pixel-task scheduling with an explicit row reference. |

| `render_mesh_cpu_with_tiles` | `src/geometry/render.rs:390` | Bounded CPU pixel-task scheduling with an explicit row reference. |

### Grid occupancy statistics (PERF-13 observability)

`SpatialGrid::stats() -> GridStats` walks the bucket array once and returns `buckets`, `non_empty_buckets`, `max_occupancy`, `mean_occupancy_non_empty` (memberships / non-empty buckets, 0 when empty), `memberships` (total bucket entries, counting a multi-bucket item once per bucket) and `items` (distinct inserted ids from the reverse membership map). It reads only; it never changes bucket order. `GridStats::summary_line(label)` formats `[GridStats] <label> buckets=.. non_empty=.. max_occupancy=.. mean_occupancy_non_empty=.. memberships=.. items=..`. Legacy `pack` prints it after the placement loop when the grid was queried (>= 32 colliders), and `optimize` prints it per island at the end of annealing. Covered by `geometry::spatial::tests::stats_count_buckets_and_memberships`.

### Fused void centroid (PERF-16)

`map_vertices_centroid(&mut [Vec3], transform) -> Vec3` applies the same transform as `map_vertices` and returns the arithmetic centroid of the transformed vertices. On the serial branch (one worker, or below `max(131072, workers * 65536)` vertices) it transforms and accumulates in one pass; on the parallel branch it maps in chunks and then sums serially in index order. Either way the accumulation order and scale are those of `mesh_centroid`, so the result is bit-identical (`fused_centroid_matches_map_then_centroid`, 1/2/8 workers, sizes across the cutoff). `forge_owned` uses it for `mesh_type: void`. Release measurement at one worker (`fused_centroid_benchmark`, median of 5): 100,000 vertices 0.000207 s -> 0.000128 s, 2,000,000 vertices 0.0210 s -> 0.0148 s.
