# Geometry Volume, Collision & Forging Reference

This page documents three `src/geometry/` submodules: `volume.rs` (mesh volume computation and the Sutherland-Hodgman mesh-clipping pipeline used to compute volume fractions inside a bounding box), `collision.rs` (parry3d-backed exact mesh collision/distance queries, including periodic-boundary ghost generation), and `forging.rs` (free-form-deformation forging simulation).

## Index

| Function | Location | Summary |
|---|---|---|
| `mesh_volume` | `src/geometry/volume.rs:6` | Absolute volume of a closed mesh via the divergence theorem. |
| `mesh_signed_volume` | `src/geometry/volume.rs:18` | Signed volume of a closed mesh (sign reflects face winding). |
| `orient_components_to_positive_volume` | `src/geometry/volume.rs:35` | Flips winding of any connected component with negative signed volume. |
| `clip_plane_signed_distance` | `src/geometry/volume.rs:55` | Signed distance of a point from a plane. |
| `clip_segment_plane_intersection` | `src/geometry/volume.rs:60` | Interpolated intersection point of a segment with a plane. |
| `clip_polygon_with_plane` | `src/geometry/volume.rs:75` | Sutherland-Hodgman clip of a convex polygon against a half-plane. |
| `quantize_point_key` | `src/geometry/volume.rs:125` | Rounds a point to a fixed-precision integer key for dedup/hashing. |
| `collect_triangle_plane_segment` | `src/geometry/volume.rs:139` | Extracts the segment where a triangle crosses a clip plane. |
| `plane_basis` | `src/geometry/volume.rs:175` | Builds an orthonormal (u, v) basis in the plane perpendicular to a normal. |
| `triangulate_cap_from_segments` | `src/geometry/volume.rs:204` | Triangulates a planar cap from cross-plane edge segments (ring-finding + fan). |
| `clip_mesh_by_plane_with_cap` | `src/geometry/volume.rs:345` | Clips a mesh against one plane and caps the resulting opening. |
| `clip_mesh_by_bbox` | `src/geometry/volume.rs:383` | Clips a mesh to an axis-aligned box via six successive plane clips. |
| `particle_volume_in_bbox` | `src/geometry/volume.rs:404` | Volume of a mesh after clipping it to a bounding box. |
| `volume_fraction_in_bbox` | `src/geometry/volume.rs:410` | Volume fraction of a single mesh within a bounding box. |
| `volume_fraction_of_meshes_in_bbox` | `src/geometry/volume.rs:420` | Total volume fraction of multiple meshes within a bounding box (parallel). |
| `to_parry_trimesh` | `src/geometry/collision.rs:29` | Converts a `Mesh` into a parry3d `TriMesh`. |
| `trimesh_contains_point` | `src/geometry/collision.rs:61` | Ray-parity point-in-solid test over a shape's bounding-volume hierarchy. |
| `mesh_surfaces_intersect_prepared` | `src/geometry/collision.rs:99` | Bbox-filtered exact test for whether two mesh *surfaces* cross. |
| `mesh_solids_nested_prepared` | `src/geometry/collision.rs:150` | Whether one closed solid lies wholly inside the other. |
| `mesh_collision_exact_prepared` | `src/geometry/collision.rs:196` | Whether two mesh *solids* overlap: surfaces cross, or one contains the other. |
| `mesh_distance_exact_prepared` | `src/geometry/collision.rs:214` | Bbox-filtered exact distance query given pre-built bboxes/shapes. |
| `mesh_collision_exact` | `src/geometry/collision.rs:259` | Convenience wrapper: builds bbox/shape then tests collision. |
| `mesh_distance_exact` | `src/geometry/collision.rs:268` | Convenience wrapper: builds bbox/shape then computes distance. |
| `generate_periodic_ghosts` | `src/geometry/collision.rs:282` | Generates translated ghost copies of a mesh for periodic boundary collision. |
| `simulate_forging_ffd` | `src/geometry/forging.rs:10` | Simple Z-axis FFD compression with lateral bulge. |
| `simulate_forging_ffd_with_tracking` | `src/geometry/forging.rs:43` | Axis-configurable FFD forging with void densification and ROI bbox tracking. |

## volume.rs

### Public functions

#### mesh_volume

- **Signature:** `pub fn mesh_volume(mesh: &Mesh) -> f64`
- **Source:** `src/geometry/volume.rs:6`
- **Purpose:** Computes the absolute (unsigned) volume of a closed mesh using the divergence theorem, i.e. by summing the signed volumes of tetrahedra formed by each face and the origin.
- **Parameters:**
  - `mesh` — the mesh to measure; assumed closed (watertight) for a correct result.
- **Returns:** The absolute volume as `f64`. For each face `(a, b, c)`, accumulates `a · (b × c) / 6` then takes the absolute value of the total.
- **Side effects:** None.
- **Notes:** If the mesh is not closed/watertight, the result is not a meaningful volume. Face winding does not matter here since the final sum is taken in absolute value (contrast with `mesh_signed_volume`).

#### mesh_signed_volume

- **Signature:** `pub fn mesh_signed_volume(mesh: &Mesh) -> f64`
- **Source:** `src/geometry/volume.rs:18`
- **Purpose:** Computes the signed volume of a closed mesh; the sign depends on face winding (outward-facing normals under the right-hand rule give a positive result, inward-facing gives negative).
- **Parameters:**
  - `mesh` — the mesh to measure.
- **Returns:** Signed `f64` volume; identical tetrahedra-sum formula to `mesh_volume` but without the final `.abs()`.
- **Side effects:** None.
- **See also:** [`orient_components_to_positive_volume`](#orient_components_to_positive_volume), which uses the sign of this function to detect and fix inward-facing components.

#### orient_components_to_positive_volume

- **Signature:** `pub fn orient_components_to_positive_volume(mesh: &Mesh) -> (Mesh, usize, usize)`
- **Source:** `src/geometry/volume.rs:35`
- **Purpose:** Normalizes mesh orientation by splitting the mesh into connected components (granules) and flipping the winding of any component whose signed volume is negative, so all components end up with consistently outward-facing (positive-volume) normals.
- **Parameters:**
  - `mesh` — the mesh to orient.
- **Returns:** A tuple `(oriented_mesh, flipped_count, total_component_count)` — the re-merged mesh, how many components were flipped, and how many components were found in total. If the mesh has no components (`split_mesh_into_granules` returns empty), returns `(mesh.clone(), 0, 0)`.
- **Side effects:** None (operates on and returns clones/new data; does not mutate the input).
- **Notes:** A component is flipped by swapping `face.b` and `face.c` on every one of its faces. Used to normalize orientation before volume/collision computations elsewhere in the pipeline.
- **See also:** [`mesh_signed_volume`](#mesh_signed_volume); `../reference/geometry-core.md` for `split_mesh_into_granules` / `merge_meshes`.

### Private helpers — Sutherland-Hodgman mesh-clipping pipeline

The remaining functions in this file form a single pipeline for clipping a triangle mesh against a plane (and ultimately an axis-aligned box) while keeping the result watertight. Data flows as follows: `clip_plane_signed_distance` classifies points relative to a plane → `clip_segment_plane_intersection` computes exact crossing points → `clip_polygon_with_plane` runs Sutherland-Hodgman clipping per triangle to produce the retained polygon "body" → `collect_triangle_plane_segment` independently records the cross-plane edge of each triangle → `plane_basis` builds a 2D coordinate frame on the plane → `triangulate_cap_from_segments` stitches those edges into closed rings and fan-triangulates them into a cap → `clip_mesh_by_plane_with_cap` combines the clipped body with the generated cap → `clip_mesh_by_bbox` calls the plane-clip six times (once per box face) to produce the final watertight clipped mesh. `quantize_point_key` is a shared utility for deduplicating floating-point points via integer hashing.

`#### clip_plane_signed_distance` — `fn clip_plane_signed_distance(p: Vec3, origin: Vec3, normal: Vec3) -> f64` — `src/geometry/volume.rs:55`. Signed distance of a point from a plane defined by `origin`/`normal` (`(p - origin) · normal`); positive is on the side the normal points toward. No side effects.

`#### clip_segment_plane_intersection` — `fn clip_segment_plane_intersection(a: Vec3, b: Vec3, da: f64, db: f64) -> Vec3` — `src/geometry/volume.rs:60`. Given a segment `(a, b)` and its pre-computed signed distances `da`, `db` to a plane, returns the interpolated point where the segment crosses the plane. If `|da - db| <= 1e-12` (segment nearly parallel to the plane) returns `a` as a degenerate fallback. Otherwise interpolates with `t = clamp(da / (da - db), 0, 1)`. No side effects.

#### clip_polygon_with_plane

- **Signature:** `fn clip_polygon_with_plane(poly: &[Vec3], origin: Vec3, normal: Vec3, eps: f64) -> Vec<Vec3>`
- **Source:** `src/geometry/volume.rs:75`
- **Purpose:** Clips a convex polygon (typically a triangle) against a half-plane using the classic Sutherland-Hodgman algorithm.
- **Parameters:**
  - `poly` — ordered polygon vertices (edges implied cyclically).
  - `origin`, `normal` — the clip plane.
  - `eps` — tolerance used to classify a vertex as "inside" (`signed_distance >= -eps`).
- **Returns:** The clipped polygon's vertices, in order; may be empty if the whole polygon is clipped away, or have as few as 0–2 points in degenerate cases.
- **Side effects:** None.
- **Notes:** For each edge `(c, n)`, keeps `n` if inside, inserts the plane-intersection point on a sign change, and drops points otherwise. After building the raw output, deduplicates consecutive near-coincident vertices (squared distance `<= 1e-20`) and also removes a duplicate closing point if the first and last vertices coincide.

`#### quantize_point_key` — `fn quantize_point_key(v: Vec3) -> (i64, i64, i64)` — `src/geometry/volume.rs:125`. Quantizes a `Vec3` to an integer-tuple key by scaling each coordinate by `1_000_000.0` and rounding, for use as a `HashMap`/`HashSet` key when deduplicating near-identical floating-point points. No side effects.

#### collect_triangle_plane_segment

- **Signature:** `fn collect_triangle_plane_segment(tri: [Vec3; 3], origin: Vec3, normal: Vec3, eps: f64) -> Option<(Vec3, Vec3)>`
- **Source:** `src/geometry/volume.rs:139`
- **Purpose:** Determines the line segment along which a triangle intersects a clip plane, for use in building the cap that closes the hole left by clipping.
- **Parameters:**
  - `tri` — the triangle's three vertices.
  - `origin`, `normal` — the clip plane.
  - `eps` — tolerance for treating a vertex as lying exactly on the plane.
- **Returns:** `Some((point_a, point_b))` if the triangle crosses (or touches) the plane in at least two distinct points; `None` otherwise (e.g. the triangle lies entirely on one side).
- **Side effects:** None.
- **Notes:** Walks the triangle's three edges; a vertex within `eps` of the plane is added directly, and any edge whose endpoints are on strictly opposite sides contributes an interpolated crossing point (via `clip_segment_plane_intersection`). Candidate points are then deduplicated (squared distance `<= 1e-16`) before the first two are returned.

`#### plane_basis` — `fn plane_basis(normal: Vec3) -> (Vec3, Vec3)` — `src/geometry/volume.rs:175`. Builds an orthonormal basis `(u, v)` spanning the plane perpendicular to `normal`, used to project 3D cap points into 2D for angular sorting. Picks a tangent axis (`X` if `|normal.x| < 0.5`, else `Y`), derives `u = normalize(normal × tangent)` and `v = normalize(normal × u)`, with axis-aligned fallbacks if either cross product is near-zero (`<= 1e-12` in magnitude). No side effects.

#### triangulate_cap_from_segments

- **Signature:** `fn triangulate_cap_from_segments(segments: &[(Vec3, Vec3)], normal: Vec3) -> Mesh`
- **Source:** `src/geometry/volume.rs:204`
- **Purpose:** Given the set of edge segments where a mesh's triangles crossed a clip plane, reconstructs the closed boundary loop(s) on that plane and triangulates each as a fan around its centroid, producing a watertight cap mesh.
- **Parameters:**
  - `segments` — unordered `(point_a, point_b)` edge pairs, one per triangle that crossed the plane (from `collect_triangle_plane_segment`).
  - `normal` — the clip plane's normal, used both to build the 2D basis for angular sorting and to determine winding direction.
- **Returns:** A `Mesh` containing the triangulated cap surface (empty `Mesh` if `segments` is empty).
- **Side effects:** None.
- **Notes:** Algorithm in three phases:
  1. **Point/edge graph construction.** Every endpoint is deduplicated via `quantize_point_key` (through a local `add_point` closure) into a shared `points` vector. Each segment becomes an undirected edge in an adjacency map (`HashMap<usize, Vec<usize>>`) and an edge set, skipping degenerate zero-length segments.
  2. **Ring-finding.** For each unused edge `(a, b)`, the code walks the adjacency graph starting from `b`, always choosing an unused neighboring edge that isn't the one just arrived from, until it returns to `a` (closing the ring) or gets stuck (dead end, in which case the ring is discarded). This traces out closed polygonal boundary loops even when the clip plane cuts several disjoint holes in the mesh.
  3. **Fan triangulation.** For each closed ring (length >= 3 after removing the duplicated closing vertex), computes the centroid, projects ring points into the `plane_basis` `(u, v)` frame, and sorts them by `atan2(v, u)` angle around the centroid to get a consistent angular order. It then emits a triangle fan from a newly added center vertex to each consecutive pair of ordered ring points, choosing winding order (`ccw` flag, from the sign of the projected polygon area dotted with `normal`) so the cap's normal is consistent with the clip plane's normal.
- **See also:** [`clip_mesh_by_plane_with_cap`](#clip_mesh_by_plane_with_cap), which is the sole caller.

#### clip_mesh_by_plane_with_cap

- **Signature:** `fn clip_mesh_by_plane_with_cap(mesh: &Mesh, origin: Vec3, normal: Vec3) -> Mesh`
- **Source:** `src/geometry/volume.rs:345`
- **Purpose:** Clips an entire mesh against a single plane and caps the resulting open boundary so the output remains a closed (watertight) solid.
- **Parameters:**
  - `mesh` — the mesh to clip.
  - `origin`, `normal` — the clip plane (points on the side the normal points toward are kept).
- **Returns:** A new `Mesh` consisting of the retained ("kept-side") geometry plus a triangulated cap over the cut.
- **Side effects:** None.
- **Notes:** For every face, runs `clip_polygon_with_plane` on the triangle and fan-triangulates the resulting polygon (if it has >= 3 vertices) into the output "body" mesh; in parallel, calls `collect_triangle_plane_segment` on the same triangle to collect the cross-plane edge, if any. After processing all faces, if any segments were collected, triangulates them into a cap via `triangulate_cap_from_segments` and merges body + cap with `merge_meshes`; otherwise returns the body as-is (the plane didn't intersect the mesh).
- **See also:** [`triangulate_cap_from_segments`](#triangulate_cap_from_segments), [`clip_mesh_by_bbox`](#clip_mesh_by_bbox).

#### clip_mesh_by_bbox

- **Signature:** `pub fn clip_mesh_by_bbox(mesh: &Mesh, bbox: BoundingBox) -> Mesh`
- **Source:** `src/geometry/volume.rs:383`
- **Purpose:** Clips a mesh so that it fits entirely within an axis-aligned bounding box, by successively clipping against each of the box's six face planes.
- **Parameters:**
  - `mesh` — the mesh to clip.
  - `bbox` — the target axis-aligned box.
- **Returns:** The clipped `Mesh`; may be an empty mesh if the input lies entirely outside `bbox`.
- **Side effects:** None.
- **Notes:** Builds six `(origin, normal)` plane pairs, one per box face (`+X`, `-X`, `+Y`, `-Y`, `+Z`, `-Z`, each normal pointing inward), and applies `clip_mesh_by_plane_with_cap` in sequence, short-circuiting early if the intermediate result becomes empty. This is the complete Sutherland-Hodgman pipeline assembled end to end: `clip_plane_signed_distance` and `clip_segment_plane_intersection` provide the geometric primitives, `clip_polygon_with_plane` clips each triangle per plane, `collect_triangle_plane_segment` records the cut edges, `plane_basis` and `triangulate_cap_from_segments` stitch those edges into a watertight cap, and `clip_mesh_by_plane_with_cap` glues body and cap together for one plane — six applications of that (one per box face) yield a mesh clipped to the box that is still watertight, which is what makes `mesh_volume` on the result meaningful (an unclipped/non-watertight partial mesh would not have a well-defined volume via the divergence-theorem formula).

  > **Algorithm:** See `../algorithms/mesh-clipping-volume-fraction.md` for the full design rationale behind this clip-and-cap pipeline and its use in volume-fraction computation.

#### particle_volume_in_bbox

- **Signature:** `pub fn particle_volume_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64`
- **Source:** `src/geometry/volume.rs:404`
- **Purpose:** Computes the volume of the portion of a mesh that lies inside a bounding box.
- **Parameters:**
  - `mesh` — the mesh (typically a single particle/granule).
  - `bbox` — the box to clip against.
- **Returns:** `f64` volume of `clip_mesh_by_bbox(mesh, bbox)`, via `mesh_volume`.
- **Side effects:** None.

#### volume_fraction_in_bbox

- **Signature:** `pub fn volume_fraction_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64`
- **Source:** `src/geometry/volume.rs:410`
- **Purpose:** Computes the fraction of a bounding box's volume occupied by a single mesh.
- **Parameters:**
  - `mesh` — the mesh to measure.
  - `bbox` — the reference box.
- **Returns:** `f64` in `[0, 1]`; delegates to `volume_fraction_of_meshes_in_bbox` with a one-element slice.
- **Side effects:** None.

#### volume_fraction_of_meshes_in_bbox

- **Signature:** `pub fn volume_fraction_of_meshes_in_bbox(meshes: &[Mesh], bbox: BoundingBox) -> f64`
- **Source:** `src/geometry/volume.rs:420`
- **Purpose:** Computes the combined volume fraction that a collection of meshes occupies within a bounding box — the core metric for reporting packing density.
- **Parameters:**
  - `meshes` — the meshes to sum volume over (e.g. all particles in a pack).
  - `bbox` — the reference box (e.g. the packing container).
- **Returns:** `f64` clamped to `[0, 1]`: `(sum of in-box volume) / max(bbox.volume(), 1e-12)`.
- **Side effects:** None (parallel computation only; no mutation).
- **Notes:** Each mesh is first split into connected components via `split_mesh_into_granules` (a single mesh may represent multiple disjoint particle bodies), and each component's in-box volume is computed independently via `particle_volume_in_bbox` before summing; if a mesh yields no components, it falls back to treating the whole mesh as one particle. The per-mesh work is parallelized with `rayon`'s `par_iter`.
- **See also:** `../algorithms/mesh-clipping-volume-fraction.md`.

## collision.rs

This module wraps [parry3d](https://parry.rs/)'s exact triangle-mesh intersection and distance queries with a bounding-box broad-phase filter, forming the exact collision layer used throughout packing and optimization (as opposed to the coarser AABB-only checks in `bbox.rs`, documented in `../reference/geometry-core.md`).

#### to_parry_trimesh

- **Signature:** `pub fn to_parry_trimesh(mesh: &Mesh) -> Option<TriMesh>`
- **Source:** `src/geometry/collision.rs:29`
- **Purpose:** Converts an internal `Mesh` into a `parry3d_f64::shape::TriMesh` for use in exact geometric queries.
- **Parameters:**
  - `mesh` — the mesh to convert.
- **Returns:** `Some(TriMesh)` on success; `None` if the mesh has no faces/vertices, if any vertex index exceeds `u32::MAX`, or if `TriMesh::new` itself fails (e.g. for a degenerate mesh).
- **Side effects:** None.
- **Notes:** parry3d's `TriMesh` uses `u32` indices, so meshes with more than ~4 billion vertices cannot be converted; this is checked explicitly per index.

#### trimesh_contains_point

- **Signature:** `pub fn trimesh_contains_point(shape: &TriMesh, p: Vec3) -> bool`
- **Source:** `src/geometry/collision.rs:61`
- **Purpose:** Decides whether a point lies inside a closed surface, using that surface's bounding-volume hierarchy.
- **Parameters:**
  - `shape` — the indexed surface.
  - `p` — the query point.
- **Returns:** `true` when the point is inside.
- **Side effects:** None.
- **Notes:** Ray parity over the QBVH, using the crate-wide `RAY_DIR` and `HIT_EPS` — the same fixed non-axis-aligned direction and `1e-8` hit tolerance as [`point_inside_mesh`](geometry-analysis.md#point_inside_mesh), so the hierarchy test and the scanning test agree point for point; `tests/collision_tests.rs` asserts as much. Parity, not a pseudo-normal test: parity is correct for a shell nested inside another and does not care which way the faces wind, which matters because `box_mesh` winds inward while real STL data winds outward. `VoidIndex::contains_point` delegates here after its own bbox pre-check.
- **See also:** [`point_inside_mesh`](geometry-analysis.md#point_inside_mesh), [`mesh_solids_nested_prepared`](#mesh_solids_nested_prepared).

#### mesh_surfaces_intersect_prepared

- **Signature:** `pub fn mesh_surfaces_intersect_prepared(a_bbox: Option<BoundingBox>, a_shape: Option<&TriMesh>, b_bbox: Option<BoundingBox>, b_shape: Option<&TriMesh>) -> bool`
- **Source:** `src/geometry/collision.rs:99`
- **Purpose:** Tests whether two mesh *surfaces* cross, given pre-computed bounding boxes and parry3d shapes, using a bbox broad-phase before the exact narrow-phase test.
- **Parameters:**
  - `a_bbox`, `b_bbox` — pre-computed bounding boxes for the two meshes.
  - `a_shape`, `b_shape` — pre-computed parry3d `TriMesh` shapes for the two meshes.
- **Returns:** `true` if the surfaces intersect. Conservatively returns `true` if either bbox is missing, or if either shape is missing (after the bbox check passes), or if `parry3d`'s `query::intersection_test` itself errors (`.unwrap_or(true)`).
- **Side effects:** None.
- **Notes:** First checks `bbox_overlaps(a_bbox, b_bbox)` as a cheap early-out; only then runs the exact parry3d intersection test with identity isometries (meshes are assumed to already be in world-space coordinates). **On its own this is not a collision test**: two closed surfaces with one wholly inside the other never cross. Call `mesh_collision_exact_prepared` unless the surface question is specifically what is wanted.
- **See also:** [`bbox_overlaps`](geometry-core.md#bbox_overlaps), [`mesh_collision_exact_prepared`](#mesh_collision_exact_prepared).

#### mesh_solids_nested_prepared

- **Signature:** `pub fn mesh_solids_nested_prepared(a_bbox: Option<BoundingBox>, a_shape: Option<&TriMesh>, b_bbox: Option<BoundingBox>, b_shape: Option<&TriMesh>) -> bool`
- **Source:** `src/geometry/collision.rs:150`
- **Purpose:** Tests whether one closed solid lies wholly inside the other.
- **Parameters:**
  - `a_bbox`, `b_bbox` — pre-computed bounding boxes.
  - `a_shape`, `b_shape` — pre-computed parry3d shapes.
- **Returns:** `true` when either solid contains the other. Returns `false` — not `true` — when a bbox or shape is missing, because the caller's surface test has already answered conservatively for that case.
- **Side effects:** None.
- **Notes:** The case surface intersection cannot see and surface distance reports as a comfortable clearance: `query::distance` measures across the gap between the two surfaces, so a sphere of radius 1 centred inside one of radius 3 reads as `1.96` apart. The bbox comparison (with a `1e-9` tolerance) comes first and settles almost every pair, since a solid inside another has its box inside the other's; only then is one `trimesh_contains_point` ray cast needed. **One vertex is enough**, given that the surfaces do not intersect: a closed shell strictly inside another has *all* of its vertices inside it. A vertex, never the centroid — a non-convex shell's centroid can sit outside its own solid, in a concavity another particle may legitimately occupy. Callers pair this with `mesh_surfaces_intersect_prepared`, which supplies that premise.
- **See also:** [`trimesh_contains_point`](#trimesh_contains_point), [`mesh_collision_exact_prepared`](#mesh_collision_exact_prepared).

#### mesh_collision_exact_prepared

- **Signature:** `pub fn mesh_collision_exact_prepared(a_bbox: Option<BoundingBox>, a_shape: Option<&TriMesh>, b_bbox: Option<BoundingBox>, b_shape: Option<&TriMesh>) -> bool`
- **Source:** `src/geometry/collision.rs:196`
- **Purpose:** Tests whether two mesh *solids* overlap — either their surfaces cross, or one contains the other.
- **Parameters:**
  - `a_bbox`, `b_bbox` — pre-computed bounding boxes for the two meshes.
  - `a_shape`, `b_shape` — pre-computed parry3d `TriMesh` shapes for the two meshes.
- **Returns:** `true` if the solids overlap; conservatively `true` when data is missing, inherited from `mesh_surfaces_intersect_prepared`.
- **Side effects:** None.
- **Notes:** `mesh_surfaces_intersect_prepared || mesh_solids_nested_prepared`, in that order — the cheap surface test rejects almost every pair before the nesting test costs anything. "Collide" here means the solids share space, not that the surfaces cross; those are different questions, and asking only the first is a real defect. Through v0.2.0 this function *was* only the first, and both packing engines duly placed particles wholly inside other particles: a `placement:` run at a 3-to-45 µm size range put 28 of 146 particles inside another and reported `target_reached` on a fraction that counted 2.1 % of the domain twice.
- **See also:** [`mesh_surfaces_intersect_prepared`](#mesh_surfaces_intersect_prepared), [`mesh_solids_nested_prepared`](#mesh_solids_nested_prepared).

#### mesh_distance_exact_prepared

- **Signature:** `pub fn mesh_distance_exact_prepared(a_bbox: Option<BoundingBox>, a_shape: Option<&TriMesh>, b_bbox: Option<BoundingBox>, b_shape: Option<&TriMesh>) -> f64`
- **Source:** `src/geometry/collision.rs:214`
- **Purpose:** Computes the minimum Euclidean distance between two meshes, given pre-computed bounding boxes and parry3d shapes, using bbox distance as a fast path.
- **Parameters:**
  - `a_bbox`, `b_bbox` — pre-computed bounding boxes.
  - `a_shape`, `b_shape` — pre-computed parry3d shapes.
- **Returns:** A distance `>= 0.0`. Returns `0.0` if either bbox is missing (fallback, not a true "touching" signal), and `0.0` whenever the meshes are found to overlap.
- **Side effects:** None.
- **Notes:** Logic: compute `bbox_d = bbox_distance(a_bbox, b_bbox)`. If `bbox_d > 0.0` (boxes are separated), the exact shape distance is at least `bbox_d`, so it computes `query::distance` between the shapes (falling back to `bbox_d` if shapes are missing or the query errors) — bboxes being separated guarantees the meshes don't overlap, so this skips the collision check. If `bbox_d == 0.0` (boxes touch or overlap), it must check for actual overlap via `mesh_collision_exact_prepared`; if they do overlap, returns `0.0`; otherwise falls through to an exact `query::distance` call (defaulting to `0.0` if shapes are missing or the query fails). A **nested** pair therefore reports `0.0`, not the gap between the two surfaces: nesting implies the boxes overlap, so the fast path cannot bypass the collision call, and that call now answers `true`.
- **See also:** [`bbox_distance`](geometry-core.md#bbox_distance), [`mesh_collision_exact_prepared`](#mesh_collision_exact_prepared).

#### mesh_collision_exact

- **Signature:** `pub fn mesh_collision_exact(a: &Mesh, b: &Mesh) -> bool`
- **Source:** `src/geometry/collision.rs:259`
- **Purpose:** Convenience wrapper that computes bounding boxes and parry3d shapes on the fly for two meshes, then tests collision.
- **Parameters:**
  - `a`, `b` — the two meshes to test.
- **Returns:** `bool`, per `mesh_collision_exact_prepared`.
- **Side effects:** None.
- **Notes:** Recomputes `mesh_bbox` and `to_parry_trimesh` for both meshes on every call; callers doing many repeated queries against the same meshes should prefer `mesh_collision_exact_prepared` with pre-built bboxes/shapes to avoid redundant work.
- **See also:** [`mesh_collision_exact_prepared`](#mesh_collision_exact_prepared).

#### mesh_distance_exact

- **Signature:** `pub fn mesh_distance_exact(a: &Mesh, b: &Mesh) -> f64`
- **Source:** `src/geometry/collision.rs:268`
- **Purpose:** Convenience wrapper that computes bounding boxes and parry3d shapes on the fly for two meshes, then computes their exact distance.
- **Parameters:**
  - `a`, `b` — the two meshes to measure between.
- **Returns:** `f64` distance, per `mesh_distance_exact_prepared`.
- **Side effects:** None.
- **See also:** [`mesh_distance_exact_prepared`](#mesh_distance_exact_prepared).

#### generate_periodic_ghosts

- **Signature:** `pub fn generate_periodic_ghosts(mesh: &Mesh, box_bounds: BoundingBox) -> Vec<Mesh>`
- **Source:** `src/geometry/collision.rs:282`
- **Purpose:** Generates translated "ghost" copies of a mesh, shifted by the packing box's dimensions along each axis combination, to support collision detection under periodic boundary conditions (a particle near one face of the box can collide with particles near the opposite face).
- **Parameters:**
  - `mesh` — the source mesh to generate ghosts of.
  - `box_bounds` — the periodic domain's bounding box; its size defines the shift distances.
- **Returns:** A `Vec<Mesh>` of ghost copies. Returns an empty vector immediately if `mesh_bbox(mesh)` is `None` (empty mesh).
- **Side effects:** None (each ghost is a fresh clone; the input mesh is untouched).
- **Notes:** Iterates over all 27 combinations of `{-1, 0, 1}` shifts on x/y/z (skipping the `(0,0,0)` identity case, leaving 26 candidate ghosts), shifting by `(x, y, z) * box_bounds.size()` per axis. A ghost is kept only if its shifted bounding box overlaps `box_bounds` on all three axes (strict inequality both directions), so only ghosts that could plausibly interact with content inside the box are materialized. Used specifically for periodic boundary mode 3 collision handling.

  > **Algorithm:** See `../algorithms/spatial-grid-collision.md` for the full periodic boundary collision design (mode 3) that this function supports.

## forging.rs

Both functions implement free-form deformation (FFD) style forging simulation: vertices are rescaled relative to a reference center, compressing along a chosen axis while bulging laterally to approximate volume-conserving plastic deformation during a forging step.

#### simulate_forging_ffd

- **Signature:** `pub fn simulate_forging_ffd(mesh: &Mesh, compression_ratio: f64, bulge_factor: f64) -> Mesh`
- **Source:** `src/geometry/forging.rs:10`
- **Purpose:** Applies a simple FFD-style forging deformation: compresses a mesh along the Z axis and bulges it laterally (X/Y) around its own bounding-box center.
- **Parameters:**
  - `mesh` — the mesh to deform.
  - `compression_ratio` — fraction of Z-extent to remove, in `[0, 1)`-ish range (clamped internally); `0` = no compression.
  - `bulge_factor` — controls how much of the volume-conserving lateral expansion to apply, `[0, 1]` (clamped internally); `0` = no lateral bulge (pure squash), `1` = full `1/sqrt(axis_scale)` bulge.
- **Returns:** A new, deformed `Mesh` (clone of the input with transformed vertices).
- **Side effects:** None.
- **Notes:** Uses `mesh_bbox(mesh)` to find the deformation center, falling back to the unit box `[0,0,0]..[1,1,1]` if the mesh is empty. `axis_scale = clamp(1 - compression_ratio, 0.01, 1.0)` (Z scale), `lateral_scale = (1 / axis_scale)^bulge_factor.clamp(0,1)` (X/Y scale). Each vertex is transformed relative to `center` by `(x*lateral_scale, y*lateral_scale, z*axis_scale)`. Fixed to Z-axis compression only — see `simulate_forging_ffd_with_tracking` for a configurable-axis version.
- **See also:** [`simulate_forging_ffd_with_tracking`](#simulate_forging_ffd_with_tracking).

#### simulate_forging_ffd_with_tracking

- **Signature:** `pub fn simulate_forging_ffd_with_tracking(mesh: &Mesh, lattice_bbox: BoundingBox, track_bbox: Option<BoundingBox>, compression_ratio: f64, compression_axis: usize, bulge_factor: f64, mesh_type: &str, void_densification: f64) -> (Mesh, Option<BoundingBox>)`
- **Source:** `src/geometry/forging.rs:43`
- **Purpose:** A more general FFD forging deformation than `simulate_forging_ffd`: the compression axis is configurable, meshes tagged as voids get an additional closure (densification) pass, and an optional region-of-interest (ROI) bounding box is carried through the same transform for tracking purposes.
- **Parameters:**
  - `mesh` — the mesh to deform.
  - `lattice_bbox` — bounding box whose center defines the deformation origin (distinct from the mesh's own bbox — typically the surrounding lattice/void-cell box).
  - `track_bbox` — an optional region-of-interest box to transform alongside the mesh (e.g. to track how a void or feature region moves/deforms).
  - `compression_ratio` — fraction of the compression-axis extent to remove (clamped to `[0.01, 1.0]` scale internally, same formula as `simulate_forging_ffd`).
  - `compression_axis` — which axis is compressed: `0` = X, `1` = Y, anything else (including `2`) = Z.
  - `bulge_factor` — controls lateral expansion on the two non-compressed axes, `[0, 1]` clamped.
  - `mesh_type` — if this string case-insensitively equals `"void"`, an extra centroid-based closure/densification pass is applied after the main FFD transform.
  - `void_densification` — scaling factor controlling how aggressively a void mesh closes up; only used when `mesh_type` is `"void"`.
- **Returns:** A tuple `(deformed_mesh, tracked_bbox)`: the deformed mesh, and — if `track_bbox` was `Some` — the transformed axis-aligned bounding box re-derived from all 8 transformed corners of the original ROI box (`None` if `track_bbox` was `None`).
- **Side effects:** None.
- **Notes:**
  - `center` is derived from `lattice_bbox` (not the mesh's own bbox), so the same deformation origin can be shared consistently across multiple meshes/void cells belonging to one lattice.
  - `axis_scale` / `lateral_scale` use the same clamped formulas as `simulate_forging_ffd`.
  - `transform_point` is a closure capturing `center`, `axis_scale`, `lateral_scale`, and `compression_axis`; it applies `axis_scale` to the compression axis and `lateral_scale` to the other two, matched via a `match compression_axis { 0 => .., 1 => .., _ => .. }` — note the `_` arm (Z) is reached both by `compression_axis == 2` and any other unrecognized value, i.e. Z-compression is the implicit default for unknown axis indices.
  - After the main vertex transform, if `mesh_type.eq_ignore_ascii_case("void")`, a second pass shrinks the mesh toward its own centroid (computed post-transform via `mesh_centroid`) by `closure = clamp(1 - 0.05 * compression_ratio * void_densification, 0.85, 1.0)` — this models void/pore closure under forging pressure, bounded so a void never closes by more than 15%.
  - ROI tracking: if `track_bbox` is `Some`, all 8 corners of the box are individually run through the same `transform_point` closure (ensuring consistency with the mesh deformation, and correctly handling axis-swapping since the box's own AABB corners aren't simply scaled per-axis after a compression-axis change) and the transformed corners' min/max are recomputed to form the new AABB. This corner-transform-then-re-bound approach is necessary rather than directly scaling `track_bbox` because the mapping mixes centering, per-axis scale selection, and (implicitly) is only axis-aligned-preserving because there's no rotation — but re-deriving the AABB from transformed corners is still the robust approach given the per-axis-conditional scale assignment.
  - Void densification is **not** applied to the tracked ROI box (only to mesh vertices), so `track_bbox` reflects the pre-densification FFD transform only.
- **See also:** [`simulate_forging_ffd`](#simulate_forging_ffd) for the simpler, Z-only variant.

  > **Algorithm:** See `../algorithms/ffd-forging.md` for the full design rationale behind the FFD forging model, axis selection, and void-densification heuristic.
