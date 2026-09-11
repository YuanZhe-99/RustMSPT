use crate::types::{BoundingBox, Mesh, Vec3};
use super::bbox::{bbox_distance, bbox_overlaps, mesh_bbox};
use super::mesh_ops::translate_mesh;
use parry3d_f64::math::{Isometry, Point};
use parry3d_f64::query;
use parry3d_f64::query::RayCast;
use parry3d_f64::shape::TriMesh;

/// The fixed ray direction used for every point-in-solid parity test in the
/// crate, and the tolerance below which two hits along it count as one crossing.
///
/// Deliberately not axis-aligned: an axis-aligned ray hits far more edges and
/// vertices exactly, which is where parity goes wrong. `s2::point_inside_mesh`
/// uses the same numbers, so the scanning test and the hierarchy test below can
/// never disagree about a point.
pub(crate) const RAY_DIR: Vec3 = Vec3 {
    x: 0.942_809_041_582_063_4,
    y: 0.270_598_050_073_098_5,
    z: 0.196_116_135_138_184_02,
};
pub(crate) const HIT_EPS: f64 = 1e-8;

// AI-FUNC-SUMMARY:
// Purpose: Convert a Mesh into a parry3d TriMesh for collision queries.
// Inputs: mesh reference.
// Returns: Some(TriMesh) on success, None for empty mesh or vertex index overflow (>u32).
// Side effects: None.
// Notes: TriMesh::new can fail for degenerate meshes; returns None in that case.
pub fn to_parry_trimesh(mesh: &Mesh) -> Option<TriMesh> {
    if mesh.faces.is_empty() || mesh.vertices.is_empty() {
        return None;
    }

    let vertices: Vec<Point<f64>> = mesh
        .vertices
        .iter()
        .map(|v| Point::new(v.x, v.y, v.z))
        .collect();

    let mut indices: Vec<[u32; 3]> = Vec::with_capacity(mesh.faces.len());
    for f in &mesh.faces {
        if f.a > u32::MAX as usize || f.b > u32::MAX as usize || f.c > u32::MAX as usize {
            return None;
        }
        indices.push([f.a as u32, f.b as u32, f.c as u32]);
    }

    TriMesh::new(vertices, indices).ok()
}

// AI-FUNC-SUMMARY:
// Purpose: Decide whether a point lies inside a closed surface, using its bounding-volume hierarchy.
// Inputs: the indexed shape and the point.
// Returns: true when the point is inside.
// Side effects: None.
// Notes: Ray parity over the QBVH, with the shared RAY_DIR and HIT_EPS, so it agrees point for
// point with s2::point_inside_mesh - a test asserts as much. Parity, not a pseudo-normal test:
// parity is correct for a shell nested inside another (a pore inside a pore, a particle buried in
// a hollow particle's rind) and does not care which way the faces wind, which matters because
// box_mesh winds inward and real STL data winds outward.
pub fn trimesh_contains_point(shape: &TriMesh, p: Vec3) -> bool {
    let ray = query::Ray::new(
        Point::new(p.x, p.y, p.z),
        parry3d_f64::math::Vector::new(RAY_DIR.x, RAY_DIR.y, RAY_DIR.z),
    );

    let mut hits: Vec<f64> = Vec::new();
    let mut visit = |triangle: &u32| {
        let t = shape.triangle(*triangle);
        if let Some(toi) = t.cast_local_ray(&ray, f64::MAX, false) {
            hits.push(toi);
        }
        true
    };
    let mut visitor = query::visitors::RayIntersectionsVisitor::new(&ray, f64::MAX, &mut visit);
    shape.qbvh().traverse_depth_first(&mut visitor);

    hits.sort_by(|a, b| a.total_cmp(b));
    let mut crossings = 0usize;
    let mut last = f64::NEG_INFINITY;
    for t in hits {
        if t >= 0.0 && (t - last).abs() > HIT_EPS {
            crossings += 1;
            last = t;
        }
    }
    crossings % 2 == 1
}

// AI-FUNC-SUMMARY:
// Purpose: Test whether two mesh SURFACES cross, ignoring the nested case entirely.
// Inputs: bounding boxes and parry3d TriMesh references for both meshes.
// Returns: true if the surfaces intersect; defaults to true if a bbox or shape is missing.
// Side effects: None.
// Notes: This is the old body of mesh_collision_exact_prepared, under a name that says what it
// actually answers. On its own it is not a collision test: two closed surfaces with one wholly
// inside the other never cross. Call mesh_collision_exact_prepared unless you specifically want
// the surface question.
pub fn mesh_surfaces_intersect_prepared(
    a_bbox: Option<BoundingBox>,
    a_shape: Option<&TriMesh>,
    b_bbox: Option<BoundingBox>,
    b_shape: Option<&TriMesh>,
) -> bool {
    let Some(a_bbox) = a_bbox else {
        return true;
    };
    let Some(b_bbox) = b_bbox else {
        return true;
    };
    if !bbox_overlaps(a_bbox, b_bbox) {
        return false;
    }

    let Some(a_shape) = a_shape else {
        return true;
    };
    let Some(b_shape) = b_shape else {
        return true;
    };

    query::intersection_test(
        &Isometry::identity(),
        a_shape,
        &Isometry::identity(),
        b_shape,
    )
    .unwrap_or(true)
}

// AI-FUNC-SUMMARY:
// Purpose: Test whether one closed solid lies wholly inside the other.
// Inputs: bounding boxes and parry3d TriMesh references for both meshes.
// Returns: true when either solid contains the other; false when data is missing.
// Side effects: None.
// Notes: The case surface intersection cannot see and surface distance reports as a comfortable
// clearance - `query::distance` measures across the gap between the two surfaces, so a sphere of
// radius 1 centred inside one of radius 3 reads as 1.96 apart rather than as the illegal placement
// it is.
//
// The box comparison comes first and settles almost every pair: a solid inside another has its box
// inside the other's box, so anything else can return immediately. Only when one box does contain
// the other is a single ray cast needed.
//
// ONE vertex is enough, given that the surfaces do not intersect: a closed shell strictly inside
// another has ALL of its vertices inside it. A vertex, never the centroid - a non-convex shell's
// centroid can sit outside its own solid, in a concavity another particle may legitimately occupy.
// Callers pair this with mesh_surfaces_intersect_prepared, which is what supplies the premise;
// mesh_collision_exact_prepared does both in the right order.
pub fn mesh_solids_nested_prepared(
    a_bbox: Option<BoundingBox>,
    a_shape: Option<&TriMesh>,
    b_bbox: Option<BoundingBox>,
    b_shape: Option<&TriMesh>,
) -> bool {
    let (Some(a_bbox), Some(b_bbox)) = (a_bbox, b_bbox) else {
        return false;
    };
    let (Some(a_shape), Some(b_shape)) = (a_shape, b_shape) else {
        return false;
    };

    let contains = |outer: BoundingBox, inner: BoundingBox| {
        const EPS: f64 = 1e-9;
        inner.min.x >= outer.min.x - EPS
            && inner.min.y >= outer.min.y - EPS
            && inner.min.z >= outer.min.z - EPS
            && inner.max.x <= outer.max.x + EPS
            && inner.max.y <= outer.max.y + EPS
            && inner.max.z <= outer.max.z + EPS
    };
    let probe = |inner: &TriMesh, outer: &TriMesh| match inner.vertices().first() {
        Some(v) => trimesh_contains_point(outer, Vec3::new(v.x, v.y, v.z)),
        None => false,
    };

    if contains(b_bbox, a_bbox) && probe(a_shape, b_shape) {
        return true;
    }
    if contains(a_bbox, b_bbox) && probe(b_shape, a_shape) {
        return true;
    }
    false
}

// AI-FUNC-SUMMARY:
// Purpose: Test whether two mesh SOLIDS overlap, by surface intersection or by one containing the other.
// Inputs: bounding boxes and parry3d TriMesh references for both meshes.
// Returns: true if the solids overlap; defaults to true if a bbox or shape is missing (conservative).
// Side effects: None.
// Notes: "Collide" here means the solids share space, not that the surfaces cross. Those are
// different questions, and asking only the first is a real defect: v0.2.0's placement engine asked
// only `intersection_test` and duly placed particles wholly inside other particles, reporting a
// volume fraction that counted the same space twice. Both halves are needed, and the cheap surface
// test runs first because it rejects almost every pair before the nesting test costs anything.
pub fn mesh_collision_exact_prepared(
    a_bbox: Option<BoundingBox>,
    a_shape: Option<&TriMesh>,
    b_bbox: Option<BoundingBox>,
    b_shape: Option<&TriMesh>,
) -> bool {
    mesh_surfaces_intersect_prepared(a_bbox, a_shape, b_bbox, b_shape)
        || mesh_solids_nested_prepared(a_bbox, a_shape, b_bbox, b_shape)
}

// AI-FUNC-SUMMARY:
// Purpose: Compute the minimum Euclidean distance between two meshes using pre-computed bounding boxes and parry3d shapes.
// Inputs: bounding boxes and parry3d TriMesh references for both meshes.
// Returns: distance >= 0 (0.0 if overlapping); returns 0.0 as fallback when data is missing.
// Side effects: None.
// Notes: Uses AABB distance as fast path; falls back to exact distance only when bboxes overlap.
// A nested pair reports 0.0, not the gap between the two surfaces: nesting implies the boxes
// overlap, so the fast path cannot bypass the collision call below, and that call now says true.
pub fn mesh_distance_exact_prepared(
    a_bbox: Option<BoundingBox>,
    a_shape: Option<&TriMesh>,
    b_bbox: Option<BoundingBox>,
    b_shape: Option<&TriMesh>,
) -> f64 {
    let Some(a_bbox) = a_bbox else {
        return 0.0;
    };
    let Some(b_bbox) = b_bbox else {
        return 0.0;
    };
    let bbox_d = bbox_distance(a_bbox, b_bbox);

    if bbox_d > 0.0 {
        if let (Some(a_shape), Some(b_shape)) = (a_shape, b_shape) {
            return query::distance(
                &Isometry::identity(),
                a_shape,
                &Isometry::identity(),
                b_shape,
            )
            .unwrap_or(bbox_d);
        }
        return bbox_d;
    }

    if mesh_collision_exact_prepared(Some(a_bbox), a_shape, Some(b_bbox), b_shape) {
        return 0.0;
    }

    if let (Some(a_shape), Some(b_shape)) = (a_shape, b_shape) {
        return query::distance(
            &Isometry::identity(),
            a_shape,
            &Isometry::identity(),
            b_shape,
        )
        .unwrap_or(0.0);
    }

    0.0
}

// AI-FUNC-SUMMARY: Convenience wrapper that computes bounding boxes and shapes on-the-fly then tests collision; returns bool; side effects: None.
pub fn mesh_collision_exact(a: &Mesh, b: &Mesh) -> bool {
    let a_bbox = mesh_bbox(a);
    let b_bbox = mesh_bbox(b);
    let a_shape = to_parry_trimesh(a);
    let b_shape = to_parry_trimesh(b);
    mesh_collision_exact_prepared(a_bbox, a_shape.as_ref(), b_bbox, b_shape.as_ref())
}

// AI-FUNC-SUMMARY: Convenience wrapper that computes bounding boxes and shapes on-the-fly then computes exact distance; returns f64; side effects: None.
pub fn mesh_distance_exact(a: &Mesh, b: &Mesh) -> f64 {
    let a_bbox = mesh_bbox(a);
    let b_bbox = mesh_bbox(b);
    let a_shape = to_parry_trimesh(a);
    let b_shape = to_parry_trimesh(b);
    mesh_distance_exact_prepared(a_bbox, a_shape.as_ref(), b_bbox, b_shape.as_ref())
}

// AI-FUNC-SUMMARY:
// Purpose: Generate periodic ghost copies of a mesh shifted by box dimensions to handle periodic boundary collisions.
// Inputs: mesh reference and box bounds defining the periodic domain.
// Returns: Vec<Mesh> of ghost meshes whose bounding boxes overlap the original box.
// Side effects: None.
// Notes: Only ghosts whose shifted bbox overlaps the box_bounds are included. Used for mode 3 periodic boundary conditions.
pub fn generate_periodic_ghosts(mesh: &Mesh, box_bounds: BoundingBox) -> Vec<Mesh> {
    let mut ghosts = Vec::new();
    let Some(bounds) = mesh_bbox(mesh) else {
        return ghosts;
    };

    let size = box_bounds.size();
    for x in [-1.0, 0.0, 1.0] {
        for y in [-1.0, 0.0, 1.0] {
            for z in [-1.0, 0.0, 1.0] {
                if x == 0.0 && y == 0.0 && z == 0.0 {
                    continue;
                }

                let shift = Vec3::new(x * size.x, y * size.y, z * size.z);
                let shifted_bounds = BoundingBox {
                    min: bounds.min.add(shift),
                    max: bounds.max.add(shift),
                };
                if shifted_bounds.min.x < box_bounds.max.x
                    && shifted_bounds.max.x > box_bounds.min.x
                    && shifted_bounds.min.y < box_bounds.max.y
                    && shifted_bounds.max.y > box_bounds.min.y
                    && shifted_bounds.min.z < box_bounds.max.z
                    && shifted_bounds.max.z > box_bounds.min.z
                {
                    let mut ghost = mesh.clone();
                    translate_mesh(&mut ghost, shift);
                    ghosts.push(ghost);
                }
            }
        }
    }

    ghosts
}
