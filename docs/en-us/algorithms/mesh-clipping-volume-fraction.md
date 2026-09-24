# Mesh Clipping and Volume-Fraction Accounting

Packing pipelines need to know how much solid material actually sits inside the packing box —
not the total volume of every particle mesh, but the volume of each particle *restricted to the
box*, since particles are routinely allowed to straddle a domain boundary (boundary modes 2 and
3). Getting that number right requires clipping each particle mesh against the box and measuring
the volume of what's left, while keeping the result watertight enough that a subsequent volume
computation via the divergence theorem is meaningful. `src/geometry/volume.rs` implements this
end to end: a Sutherland-Hodgman-based mesh/plane clipper, a box-clipping wrapper built from six
successive plane clips, and the volume-fraction (VF) metrics built on top of it.

## Clipping a mesh against a single plane

The atomic operation is `clip_mesh_by_plane_with_cap`, which clips an entire mesh against one
half-space (defined by a plane `origin` and outward `normal`) and returns a new, still-closed
mesh. It is built from several smaller pieces:

1. **Signed-distance classification** (`clip_plane_signed_distance`). Every vertex is classified
   by `(p - origin) · normal`: non-negative means "inside" (kept side), negative means "outside"
   (clipped away).
2. **Sutherland-Hodgman polygon clipping** (`clip_polygon_with_plane`). Each triangle is walked as
   a 3-vertex polygon; for every edge `(c, n)` of the polygon, the four classic Sutherland-Hodgman
   cases apply based on whether each endpoint is inside or outside the half-space:
   - inside → inside: keep the second vertex.
   - inside → outside: emit the interpolated intersection point only.
   - outside → inside: emit the interpolated intersection point, then the second vertex.
   - outside → outside: emit nothing.

   The interpolated point comes from `clip_segment_plane_intersection`, which linearly interpolates
   along the segment using the two endpoints' signed distances (`t = da / (da - db)`, clamped to
   `[0, 1]`) — the standard plane/segment intersection formula. Because a triangle can clip down to
   anywhere between 0 and 4 vertices, the output polygon is triangulated as a simple fan
   (`for i in 1..(clipped_poly.len() - 1)`) when reassembling the clipped mesh body.
3. **Cross-plane segment extraction** (`collect_triangle_plane_segment`). For each original
   triangle, this independently finds where the plane actually crosses the triangle's boundary
   (vertices near-zero distance, or edges with endpoints on opposite sides) and records that as a
   two-point segment. These segments are the raw material for repairing the hole the clip just cut
   into the mesh's surface.
4. **Cap triangulation** (`plane_basis`, `triangulate_cap_from_segments`). Simply discarding the
   outside portion of a mesh leaves an open boundary where the plane sliced through it — the mesh
   is no longer closed, and volume computed via the divergence-theorem formula in `mesh_volume`
   would be meaningless on an open surface. To keep results watertight, all the cross-plane
   segments collected in step 3 are stitched into a **cap**: an in-plane 2D basis `(u, v)`
   orthogonal to the clip normal is computed by `plane_basis`, the segments are merged into an
   adjacency graph over deduplicated endpoint indices, and `triangulate_cap_from_segments` walks
   that graph to find closed rings (polygon loops bounding the hole), sorts each ring's vertices
   angularly around their centroid in the `(u, v)` plane, and fan-triangulates from the centroid
   with a winding order chosen to match the clip normal's orientation. The clipped body and its cap
   are merged (`merge_meshes`) into one closed mesh.

## From one plane to a box: `clip_mesh_by_bbox`

`clip_mesh_by_bbox` clips a mesh to an axis-aligned box by applying `clip_mesh_by_plane_with_cap`
six times in sequence, once per box face (`min.x`/`max.x`, `min.y`/`max.y`, `min.z`/`max.z`), each
with the appropriate inward-facing normal. Because each individual plane clip re-caps its own open
boundary, the mesh stays watertight after every one of the six passes — not just at the end — so
intermediate states are always valid closed meshes, and the function can short-circuit early
(`if out.is_empty() { break; }`) if the mesh is clipped away to nothing partway through the six
planes. The result is the portion of the input mesh that lies within the box, as a closed,
volume-computable mesh.

## Volume-fraction accounting

Volume fraction is built directly on top of box-clipping:

- `particle_volume_in_bbox(mesh, bbox)` = `mesh_volume(clip_mesh_by_bbox(mesh, bbox))` — clip the
  particle to the box, then measure its volume via the signed-tetrahedra divergence-theorem sum in
  `mesh_volume`.
- `volume_fraction_of_meshes_in_bbox(meshes, bbox)` computes the aggregate VF over a whole particle
  population. It runs in parallel over meshes with `rayon`'s `par_iter`, and for each mesh it first
  splits into connected components (`split_mesh_into_granules`) — a single input "mesh" can
  represent several disjoint granules — and sums `particle_volume_in_bbox` across each granule
  independently (falling back to treating the whole mesh as one piece if splitting finds nothing).
  All per-mesh clipped volumes are summed, divided by the box volume (`bbox.volume()`, floored at
  `1e-12` to avoid division by zero), and the result is clamped to `[0, 1]`.
- `volume_fraction_in_bbox` is the single-mesh convenience wrapper.

This VF computation is the shared core metric that packing, optimization, and measurement
pipelines all build on: packing pipelines use it to track progress toward
`target_volume_fraction` and decide when the box is full; optimization pipelines fold VF proximity
into the loss/objective function being minimized by simulated annealing; and measurement pipelines
report VF as a headline output statistic. Per `AGENTS.md`, for boundary modes 2 and 3, diameter and
sphericity metrics are computed from the full, unclipped mesh, while VF specifically always
accounts using the in-box clipped volume — so a particle poking out of the box contributes its full
size to shape statistics but only its clipped fraction to VF.

## Vertex deduplication via quantized keys

Cap triangulation needs to merge segment endpoints that are computed independently (once per
triangle) but represent the same physical point on the clip plane, and floating-point equality is
unreliable for that purpose. `quantize_point_key` sidesteps this by rounding each coordinate to a
fixed-precision integer key (`(x * 1e6).round() as i64`, similarly for `y`/`z`) and using that
tuple as a hash-map key — two points that agree to within roughly `1e-6` units collapse to the same
vertex. This is the same tolerant-deduplication trick used independently by the STL mesh loader
(`src/io/stl.rs`'s `quantize_key` / `dedup_vertex`) to merge coincident vertices when parsing
triangle soups into indexed meshes — both are instances of a recurring "snap-to-grid" pattern for
turning floating-point geometry into consistent topology.

## Cross-references

- [geometry-volume-collision.md](../reference/geometry-volume-collision.md)
- [pipeline-packing.md](../reference/pipeline-packing.md)
- [pipeline-optimize.md](../reference/pipeline-optimize.md)

### No-cut volume fast paths

The private plane clipper consumes its mesh. Classification uses referenced face vertices with the existing `distance >= -1e-9` predicate: when all are inside (including a touching plane), it returns the same mesh allocation without generating a cap; when all are outside, it returns empty. Mixed cases retain the existing polygon/cap algorithm. This fixes spurious caps on an entirely contained mesh touching a domain face. Unreferenced outside vertices cannot create a cut. `clip_mesh_by_bbox` moves each intermediate mesh into the next pass. `particle_volume_in_bbox` directly sums the original mesh when all stored vertices are inclusively inside the box, avoiding a clone and all six clipping passes. Tests cover both windings, translated boxes, coincident faces, partial outward box intersections, and unused outside vertices. This does not replace the legacy partial-cut cap algorithm with placement’s exact signed-volume routine or establish correctness for arbitrary nonconvex/nested cap loops.
