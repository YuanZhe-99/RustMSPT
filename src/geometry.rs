use crate::types::{BoundingBox, Mesh, Vec3};
use parry3d_f64::math::{Isometry, Point};
use parry3d_f64::query;
use parry3d_f64::shape::TriMesh;
use rand::Rng;
use rayon::prelude::*;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::collections::{HashMap, VecDeque};

pub fn mesh_bbox(mesh: &Mesh) -> Option<BoundingBox> {
    // Purpose: Compute axis-aligned bounding box of a mesh.
    // Inputs: mesh vertices/faces.
    // Outputs: bounding box or None for empty vertex list.
    if mesh.vertices.is_empty() {
        return None;
    }

    let mut min = mesh.vertices[0];
    let mut max = mesh.vertices[0];
    for v in &mesh.vertices {
        min.x = min.x.min(v.x);
        min.y = min.y.min(v.y);
        min.z = min.z.min(v.z);
        max.x = max.x.max(v.x);
        max.y = max.y.max(v.y);
        max.z = max.z.max(v.z);
    }

    Some(BoundingBox { min, max })
}

pub fn mesh_centroid(mesh: &Mesh) -> Vec3 {
    // Purpose: Compute arithmetic centroid of mesh vertices.
    // Inputs: mesh vertices.
    // Outputs: centroid point.
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    let count = mesh.vertices.len() as f64;
    for v in &mesh.vertices {
        sum = sum.add(*v);
    }
    if count > 0.0 {
        sum.scale(1.0 / count)
    } else {
        sum
    }
}

pub fn vec_norm(v: Vec3) -> f64 {
    // Purpose: Compute Euclidean norm of a 3D vector.
    // Inputs: vector value.
    // Outputs: non-negative length.
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

pub fn bbox_overlaps(a: BoundingBox, b: BoundingBox) -> bool {
    // Purpose: Test overlap between two axis-aligned bounding boxes.
    // Inputs: two bounding boxes.
    // Outputs: true when overlap exists.
    a.min.x < b.max.x
        && a.max.x > b.min.x
        && a.min.y < b.max.y
        && a.max.y > b.min.y
        && a.min.z < b.max.z
        && a.max.z > b.min.z
}

pub fn bbox_distance(a: BoundingBox, b: BoundingBox) -> f64 {
    // Purpose: Compute shortest distance between two axis-aligned bounding boxes.
    // Inputs: two bounding boxes.
    // Outputs: non-negative distance.
    let dx = if a.max.x < b.min.x {
        b.min.x - a.max.x
    } else if b.max.x < a.min.x {
        a.min.x - b.max.x
    } else {
        0.0
    };

    let dy = if a.max.y < b.min.y {
        b.min.y - a.max.y
    } else if b.max.y < a.min.y {
        a.min.y - b.max.y
    } else {
        0.0
    };

    let dz = if a.max.z < b.min.z {
        b.min.z - a.max.z
    } else if b.max.z < a.min.z {
        a.min.z - b.max.z
    } else {
        0.0
    };

    (dx * dx + dy * dy + dz * dz).sqrt()
}

pub fn mesh_volume(mesh: &Mesh) -> f64 {
    // Purpose: Compute absolute mesh volume from triangle tetrahedralization.
    // Inputs: triangle mesh.
    // Outputs: non-negative volume.
    let mut total = 0.0;
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        total += a.dot(b.cross(c)) / 6.0;
    }
    total.abs()
}

/// Compute signed volume from triangle winding.
/// Input: mesh with triangle faces. Output: signed volume (negative means inverted orientation).
pub fn mesh_signed_volume(mesh: &Mesh) -> f64 {
    let mut total = 0.0;
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        total += a.dot(b.cross(c)) / 6.0;
    }
    total
}

/// Enforce positive orientation for each connected component independently.
/// Input: mesh. Output: (reoriented mesh, flipped component count, total component count).
pub fn orient_components_to_positive_volume(mesh: &Mesh) -> (Mesh, usize, usize) {
    let mut parts = split_mesh_into_granules(mesh);
    if parts.is_empty() {
        return (mesh.clone(), 0, 0);
    }

    let mut flipped = 0usize;
    for part in &mut parts {
        if mesh_signed_volume(part) < 0.0 {
            for face in &mut part.faces {
                std::mem::swap(&mut face.b, &mut face.c);
            }
            flipped += 1;
        }
    }

    (merge_meshes(&parts), flipped, parts.len())
}

pub fn merge_meshes(meshes: &[Mesh]) -> Mesh {
    // Purpose: Merge multiple meshes into one mesh with remapped indices.
    // Inputs: mesh slice.
    // Outputs: merged mesh.
    let mut out = Mesh::empty();
    for m in meshes {
        let offset = out.vertices.len();
        out.vertices.extend(m.vertices.iter().copied());
        out.faces.extend(m.faces.iter().map(|f| crate::types::Triangle {
            a: f.a + offset,
            b: f.b + offset,
            c: f.c + offset,
        }));
    }
    out
}

pub fn split_mesh_into_granules(mesh: &Mesh) -> Vec<Mesh> {
    // Purpose: Split mesh into connected components by shared vertices.
    // Inputs: source mesh.
    // Outputs: vector of component meshes.
    if mesh.faces.is_empty() || mesh.vertices.is_empty() {
        return Vec::new();
    }

    let mut vertex_to_faces: Vec<Vec<usize>> = vec![Vec::new(); mesh.vertices.len()];
    for (face_index, face) in mesh.faces.iter().enumerate() {
        vertex_to_faces[face.a].push(face_index);
        vertex_to_faces[face.b].push(face_index);
        vertex_to_faces[face.c].push(face_index);
    }

    let mut visited = vec![false; mesh.faces.len()];
    let mut parts: Vec<Mesh> = Vec::new();

    for start_face in 0..mesh.faces.len() {
        if visited[start_face] {
            continue;
        }

        let mut queue: VecDeque<usize> = VecDeque::new();
        let mut component_faces: Vec<usize> = Vec::new();
        visited[start_face] = true;
        queue.push_back(start_face);

        while let Some(current) = queue.pop_front() {
            component_faces.push(current);
            let f = &mesh.faces[current];
            for &vertex_index in &[f.a, f.b, f.c] {
                for &adjacent_face in &vertex_to_faces[vertex_index] {
                    if !visited[adjacent_face] {
                        visited[adjacent_face] = true;
                        queue.push_back(adjacent_face);
                    }
                }
            }
        }

        let mut local_vertices: Vec<Vec3> = Vec::new();
        let mut local_faces = Vec::new();
        let mut remap: HashMap<usize, usize> = HashMap::new();

        for face_index in component_faces {
            let f = &mesh.faces[face_index];
            let map_vertex = |global: usize,
                              local_vertices: &mut Vec<Vec3>,
                              remap: &mut HashMap<usize, usize>| {
                if let Some(&idx) = remap.get(&global) {
                    idx
                } else {
                    let idx = local_vertices.len();
                    local_vertices.push(mesh.vertices[global]);
                    remap.insert(global, idx);
                    idx
                }
            };

            let a = map_vertex(f.a, &mut local_vertices, &mut remap);
            let b = map_vertex(f.b, &mut local_vertices, &mut remap);
            let c = map_vertex(f.c, &mut local_vertices, &mut remap);
            local_faces.push(crate::types::Triangle { a, b, c });
        }

        parts.push(Mesh {
            vertices: local_vertices,
            faces: local_faces,
        });
    }

    parts
}

pub fn translate_mesh(mesh: &mut Mesh, delta: Vec3) {
    // Purpose: Translate every vertex by a delta vector.
    // Inputs: mutable mesh and translation delta.
    // Outputs: mesh updated in place.
    for v in &mut mesh.vertices {
        *v = v.add(delta);
    }
}

pub fn move_mesh_to_target_center(mesh: &mut Mesh, target: Vec3) {
    // Purpose: Move mesh so centroid matches target point.
    // Inputs: mutable mesh and target center.
    // Outputs: mesh translated in place.
    let center = mesh_centroid(mesh);
    translate_mesh(mesh, target.sub(center));
}

pub fn wrap_mesh_centroid_to_box(mesh: &mut Mesh, box_bounds: BoundingBox) {
    // Purpose: Wrap mesh centroid into periodic box range.
    // Inputs: mutable mesh and periodic box bounds.
    // Outputs: mesh translated so centroid lies in base box.
    let center = mesh_centroid(mesh);
    let size = box_bounds.size();

    let wrap_axis = |value: f64, min: f64, len: f64| -> f64 {
        if len <= 0.0 {
            return value;
        }
        min + (value - min).rem_euclid(len)
    };

    let wrapped = Vec3::new(
        wrap_axis(center.x, box_bounds.min.x, size.x),
        wrap_axis(center.y, box_bounds.min.y, size.y),
        wrap_axis(center.z, box_bounds.min.z, size.z),
    );

    move_mesh_to_target_center(mesh, wrapped);
}

pub fn generate_periodic_ghosts(mesh: &Mesh, box_bounds: BoundingBox) -> Vec<Mesh> {
    // Purpose: Build periodic image meshes that overlap current box.
    // Inputs: base mesh and simulation box bounds.
    // Outputs: translated periodic ghost meshes.
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

pub fn scale_mesh(mesh: &mut Mesh, factor: f64) {
    // Purpose: Uniformly scale mesh vertices around origin.
    // Inputs: mutable mesh and scalar factor.
    // Outputs: scaled mesh in place.
    for v in &mut mesh.vertices {
        *v = v.scale(factor);
    }
}

pub fn mesh_surface_area(mesh: &Mesh) -> f64 {
    // Purpose: Estimate mesh surface area from triangle faces.
    // Inputs: triangle mesh.
    // Outputs: total surface area.
    let mut area = 0.0;
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        let ab = b.sub(a);
        let ac = c.sub(a);
        area += 0.5 * vec_norm(ab.cross(ac));
    }
    area
}

pub fn rotate_mesh_around_center(mesh: &mut Mesh, axis: Vec3, angle: f64) {
    // Purpose: Rotate mesh around its centroid with Rodrigues formula.
    // Inputs: mutable mesh, axis, angle in radians.
    // Outputs: mesh vertices updated in place.
    let axis_len = vec_norm(axis);
    if axis_len <= 1e-12 {
        return;
    }

    let k = axis.scale(1.0 / axis_len);
    let center = mesh_centroid(mesh);
    let cos_t = angle.cos();
    let sin_t = angle.sin();

    for v in &mut mesh.vertices {
        let p = v.sub(center);
        let term1 = p.scale(cos_t);
        let term2 = k.cross(p).scale(sin_t);
        let term3 = k.scale(k.dot(p) * (1.0 - cos_t));
        *v = center.add(term1.add(term2).add(term3));
    }
}

pub fn box_mesh(bbox: BoundingBox) -> Mesh {
    // Purpose: Build triangulated box mesh from bounding box corners.
    // Inputs: axis-aligned bounding box.
    // Outputs: closed box mesh.
    let min = bbox.min;
    let max = bbox.max;
    let v = vec![
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(max.x, max.y, max.z),
        Vec3::new(min.x, max.y, max.z),
    ];

    let idx = [
        (0, 1, 2), (0, 2, 3),
        (4, 6, 5), (4, 7, 6),
        (0, 4, 5), (0, 5, 1),
        (1, 5, 6), (1, 6, 2),
        (2, 6, 7), (2, 7, 3),
        (3, 7, 4), (3, 4, 0),
    ];

    let faces = idx
        .iter()
        .map(|(a, b, c)| crate::types::Triangle {
            a: *a,
            b: *b,
            c: *c,
        })
        .collect();

    Mesh { vertices: v, faces }
}

pub fn simulate_forging_ffd(mesh: &Mesh, compression_ratio: f64, bulge_factor: f64) -> Mesh {
    // Purpose: Apply simplified forging deformation to mesh.
    // Inputs: source mesh, compression ratio, bulge factor.
    // Outputs: deformed mesh.
    let bbox = mesh_bbox(mesh).unwrap_or(BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    });
    let center = Vec3::new(
        (bbox.min.x + bbox.max.x) * 0.5,
        (bbox.min.y + bbox.max.y) * 0.5,
        (bbox.min.z + bbox.max.z) * 0.5,
    );

    let mut out = mesh.clone();
    let axis_scale = (1.0 - compression_ratio).clamp(0.01, 1.0);
    let lateral_scale = (1.0 / axis_scale.sqrt()).powf(bulge_factor.clamp(0.0, 1.0));

    for v in &mut out.vertices {
        let local = v.sub(center);
        *v = Vec3::new(
            center.x + local.x * lateral_scale,
            center.y + local.y * lateral_scale,
            center.z + local.z * axis_scale,
        );
    }

    out
}

/// Apply simplified forging affine transform with optional ROI tracking.
/// Inputs: source mesh, lattice bbox (deformation frame), optional ROI bbox,
/// compression, compression axis (0=x,1=y,2=z), bulge, mesh type, void densification factor.
/// Outputs: deformed mesh and optional transformed ROI bbox.
pub fn simulate_forging_ffd_with_tracking(
    mesh: &Mesh,
    lattice_bbox: BoundingBox,
    track_bbox: Option<BoundingBox>,
    compression_ratio: f64,
    compression_axis: usize,
    bulge_factor: f64,
    mesh_type: &str,
    void_densification: f64,
) -> (Mesh, Option<BoundingBox>) {
    let center = Vec3::new(
        (lattice_bbox.min.x + lattice_bbox.max.x) * 0.5,
        (lattice_bbox.min.y + lattice_bbox.max.y) * 0.5,
        (lattice_bbox.min.z + lattice_bbox.max.z) * 0.5,
    );

    let axis_scale = (1.0 - compression_ratio).clamp(0.01, 1.0);
    let lateral_scale = (1.0 / axis_scale.sqrt()).powf(bulge_factor.clamp(0.0, 1.0));

    let transform_point = |p: Vec3| {
        let local = p.sub(center);
        match compression_axis {
            0 => Vec3::new(
                center.x + local.x * axis_scale,
                center.y + local.y * lateral_scale,
                center.z + local.z * lateral_scale,
            ),
            1 => Vec3::new(
                center.x + local.x * lateral_scale,
                center.y + local.y * axis_scale,
                center.z + local.z * lateral_scale,
            ),
            _ => Vec3::new(
                center.x + local.x * lateral_scale,
                center.y + local.y * lateral_scale,
                center.z + local.z * axis_scale,
            ),
        }
    };

    let mut out = mesh.clone();
    for v in &mut out.vertices {
        *v = transform_point(*v);
    }

    if mesh_type.eq_ignore_ascii_case("void") {
        let closure = (1.0 - 0.05 * compression_ratio * void_densification).clamp(0.85, 1.0);
        let c = mesh_centroid(&out);
        for v in &mut out.vertices {
            let local = v.sub(c);
            *v = c.add(local.scale(closure));
        }
    }

    let tracked = track_bbox.map(|tb| {
        let corners = [
            Vec3::new(tb.min.x, tb.min.y, tb.min.z),
            Vec3::new(tb.min.x, tb.min.y, tb.max.z),
            Vec3::new(tb.min.x, tb.max.y, tb.min.z),
            Vec3::new(tb.min.x, tb.max.y, tb.max.z),
            Vec3::new(tb.max.x, tb.min.y, tb.min.z),
            Vec3::new(tb.max.x, tb.min.y, tb.max.z),
            Vec3::new(tb.max.x, tb.max.y, tb.min.z),
            Vec3::new(tb.max.x, tb.max.y, tb.max.z),
        ];

        let mut min_p = transform_point(corners[0]);
        let mut max_p = min_p;
        for p in corners.iter().skip(1).copied().map(transform_point) {
            min_p.x = min_p.x.min(p.x);
            min_p.y = min_p.y.min(p.y);
            min_p.z = min_p.z.min(p.z);
            max_p.x = max_p.x.max(p.x);
            max_p.y = max_p.y.max(p.y);
            max_p.z = max_p.z.max(p.z);
        }
        BoundingBox { min: min_p, max: max_p }
    });

    (out, tracked)
}

pub fn check_boundary_constraints_mode(
    mesh: &Mesh,
    box_bounds: BoundingBox,
    mode: u8,
    d1: f64,
    d2: f64,
) -> bool {
    // Purpose: Enforce strict/loose/periodic boundary constraints under selected mode.
    // Inputs: candidate mesh, box bounds, mode, d1 and d2 thresholds.
    // Outputs: true when constraints are satisfied.
    let Some(bounds) = mesh_bbox(mesh) else {
        return false;
    };

    let local_min = bounds.min.sub(box_bounds.min);
    let local_max = bounds.max.sub(box_bounds.min);
    let size = box_bounds.size();

    let is_fully_inside = local_min.x >= 0.0
        && local_min.y >= 0.0
        && local_min.z >= 0.0
        && local_max.x <= size.x
        && local_max.y <= size.y
        && local_max.z <= size.z;

    if is_fully_inside {
        if local_min.x < d1
            || local_min.y < d1
            || local_min.z < d1
            || local_max.x > (size.x - d1)
            || local_max.y > (size.y - d1)
            || local_max.z > (size.z - d1)
        {
            return false;
        }
        return true;
    }

    if mode == 1 {
        return false;
    }

    let min_arr = [local_min.x, local_min.y, local_min.z];
    let max_arr = [local_max.x, local_max.y, local_max.z];
    let size_arr = [size.x, size.y, size.z];

    for i in 0..3 {
        if min_arr[i] < 0.0 {
            if min_arr[i].abs() < d2 || max_arr[i] < d2 {
                return false;
            }
        }
        if max_arr[i] > size_arr[i] {
            if (size_arr[i] - min_arr[i]) < d2 || (max_arr[i] - size_arr[i]) < d2 {
                return false;
            }
        }
        if min_arr[i] >= 0.0 && max_arr[i] <= size_arr[i] {
            if min_arr[i] < d1 || max_arr[i] > (size_arr[i] - d1) {
                return false;
            }
        }
    }

    true
}

pub fn to_parry_trimesh(mesh: &Mesh) -> Option<TriMesh> {
    // Purpose: Convert internal mesh to Parry TriMesh for exact queries.
    // Inputs: triangle mesh.
    // Outputs: Parry mesh or None when conversion is invalid.
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

pub fn mesh_collision_exact_prepared(
    a_bbox: Option<BoundingBox>,
    a_shape: Option<&TriMesh>,
    b_bbox: Option<BoundingBox>,
    b_shape: Option<&TriMesh>,
) -> bool {
    // Purpose: Perform collision test using precomputed bbox and TriMesh handles.
    // Inputs: optional bboxes and collision shapes for two particles.
    // Outputs: true when collision occurs or data is invalid conservatively.
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

pub fn mesh_distance_exact_prepared(
    a_bbox: Option<BoundingBox>,
    a_shape: Option<&TriMesh>,
    b_bbox: Option<BoundingBox>,
    b_shape: Option<&TriMesh>,
) -> f64 {
    // Purpose: Compute minimum distance using precomputed data.
    // Inputs: optional bboxes and collision shapes for two particles.
    // Outputs: non-negative distance.
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

pub fn mesh_collision_exact(a: &Mesh, b: &Mesh) -> bool {
    // Purpose: Perform exact mesh collision test with broad-phase bbox filtering.
    // Inputs: two meshes.
    // Outputs: true when meshes intersect or conversion fails conservatively.
    let a_bbox = mesh_bbox(a);
    let b_bbox = mesh_bbox(b);
    let a_shape = to_parry_trimesh(a);
    let b_shape = to_parry_trimesh(b);
    mesh_collision_exact_prepared(a_bbox, a_shape.as_ref(), b_bbox, b_shape.as_ref())
}

pub fn mesh_distance_exact(a: &Mesh, b: &Mesh) -> f64 {
    // Purpose: Compute exact minimum distance between two meshes.
    // Inputs: two meshes.
    // Outputs: non-negative distance.
    let a_bbox = mesh_bbox(a);
    let b_bbox = mesh_bbox(b);
    let a_shape = to_parry_trimesh(a);
    let b_shape = to_parry_trimesh(b);
    mesh_distance_exact_prepared(a_bbox, a_shape.as_ref(), b_bbox, b_shape.as_ref())
}

fn clip_plane_signed_distance(p: Vec3, origin: Vec3, normal: Vec3) -> f64 {
    // Purpose: Evaluate signed distance from point to plane.
    // Inputs: point, plane origin, plane normal.
    // Outputs: signed distance value.
    p.sub(origin).dot(normal)
}

fn clip_segment_plane_intersection(a: Vec3, b: Vec3, da: f64, db: f64) -> Vec3 {
    // Purpose: Compute segment-plane intersection using endpoint signed distances.
    // Inputs: segment endpoints and their plane distances.
    // Outputs: intersection point on segment.
    let denom = da - db;
    if denom.abs() <= 1e-12 {
        return a;
    }
    let t = (da / denom).clamp(0.0, 1.0);
    a.add(b.sub(a).scale(t))
}

fn clip_polygon_with_plane(poly: &[Vec3], origin: Vec3, normal: Vec3, eps: f64) -> Vec<Vec3> {
    // Purpose: Clip polygon against half-space defined by a plane.
    // Inputs: polygon vertices, plane origin/normal, tolerance.
    // Outputs: clipped polygon vertices.
    if poly.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for i in 0..poly.len() {
        let c = poly[i];
        let n = poly[(i + 1) % poly.len()];
        let dc = clip_plane_signed_distance(c, origin, normal);
        let dn = clip_plane_signed_distance(n, origin, normal);
        let in_c = dc >= -eps;
        let in_n = dn >= -eps;

        match (in_c, in_n) {
            (true, true) => out.push(n),
            (true, false) => out.push(clip_segment_plane_intersection(c, n, dc, dn)),
            (false, true) => {
                out.push(clip_segment_plane_intersection(c, n, dc, dn));
                out.push(n);
            }
            (false, false) => {}
        }
    }

    if out.len() <= 2 {
        return out;
    }

    let mut dedup = Vec::with_capacity(out.len());
    for p in out {
        let keep = dedup
            .last()
            .map(|q: &Vec3| p.sub(*q).dot(p.sub(*q)) > 1e-20)
            .unwrap_or(true);
        if keep {
            dedup.push(p);
        }
    }
    if dedup.len() > 1 {
        let first = dedup[0];
        let last = *dedup.last().unwrap_or(&first);
        if first.sub(last).dot(first.sub(last)) <= 1e-20 {
            dedup.pop();
        }
    }
    dedup
}

fn quantize_point_key(v: Vec3) -> (i64, i64, i64) {
    // Purpose: Quantize point coordinates for tolerant hashing.
    // Inputs: 3D point.
    // Outputs: integer key tuple.
    const SCALE: f64 = 1_000_000.0;
    (
        (v.x * SCALE).round() as i64,
        (v.y * SCALE).round() as i64,
        (v.z * SCALE).round() as i64,
    )
}

fn collect_triangle_plane_segment(
    tri: [Vec3; 3],
    origin: Vec3,
    normal: Vec3,
    eps: f64,
) -> Option<(Vec3, Vec3)> {
    // Purpose: Extract triangle-plane intersection segment when available.
    // Inputs: triangle vertices, plane origin/normal, tolerance.
    // Outputs: optional segment endpoints.
    let mut pts: Vec<Vec3> = Vec::new();
    for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
        let a = tri[i];
        let b = tri[j];
        let da = clip_plane_signed_distance(a, origin, normal);
        let db = clip_plane_signed_distance(b, origin, normal);

        if da.abs() <= eps {
            pts.push(a);
        }
        if (da > eps && db < -eps) || (da < -eps && db > eps) {
            pts.push(clip_segment_plane_intersection(a, b, da, db));
        }
    }

    let mut uniq: Vec<Vec3> = Vec::new();
    for p in pts {
        if !uniq.iter().any(|q| p.sub(*q).dot(p.sub(*q)) <= 1e-16) {
            uniq.push(p);
        }
    }

    if uniq.len() >= 2 {
        Some((uniq[0], uniq[1]))
    } else {
        None
    }
}

fn plane_basis(normal: Vec3) -> (Vec3, Vec3) {
    // Purpose: Build orthonormal tangent basis on a plane.
    // Inputs: plane normal.
    // Outputs: two tangent unit vectors.
    let tangent = if normal.x.abs() < 0.5 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u_raw = normal.cross(tangent);
    let un = (u_raw.dot(u_raw)).sqrt();
    let u = if un > 1e-12 {
        u_raw.scale(1.0 / un)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let v_raw = normal.cross(u);
    let vn = (v_raw.dot(v_raw)).sqrt();
    let v = if vn > 1e-12 {
        v_raw.scale(1.0 / vn)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    (u, v)
}

fn triangulate_cap_from_segments(segments: &[(Vec3, Vec3)], normal: Vec3) -> Mesh {
    // Purpose: Triangulate clipping cap from plane intersection segments.
    // Inputs: unordered boundary segments and cap normal.
    // Outputs: cap mesh.
    if segments.is_empty() {
        return Mesh::empty();
    }

    let mut points: Vec<Vec3> = Vec::new();
    let mut point_map: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut edges: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

    let add_point = |p: Vec3,
                     points: &mut Vec<Vec3>,
                     point_map: &mut HashMap<(i64, i64, i64), usize>| {
        let k = quantize_point_key(p);
        if let Some(&idx) = point_map.get(&k) {
            idx
        } else {
            let idx = points.len();
            points.push(p);
            point_map.insert(k, idx);
            idx
        }
    };

    for (a, b) in segments {
        let ia = add_point(*a, &mut points, &mut point_map);
        let ib = add_point(*b, &mut points, &mut point_map);
        if ia == ib {
            continue;
        }
        let e = if ia < ib { (ia, ib) } else { (ib, ia) };
        if edges.insert(e) {
            adjacency.entry(ia).or_default().push(ib);
            adjacency.entry(ib).or_default().push(ia);
        }
    }

    let mut used: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let (u_axis, v_axis) = plane_basis(normal);
    let mut cap = Mesh::empty();

    for (a, b) in edges {
        if used.contains(&(a, b)) {
            continue;
        }

        let mut ring = vec![a, b];
        used.insert((a, b));
        let mut prev = a;
        let mut curr = b;
        loop {
            if curr == a {
                break;
            }
            let Some(neigh) = adjacency.get(&curr) else {
                break;
            };
            let mut next_opt = None;
            for &n in neigh {
                if n == prev {
                    continue;
                }
                let e = if curr < n { (curr, n) } else { (n, curr) };
                if !used.contains(&e) {
                    next_opt = Some(n);
                    break;
                }
            }
            let Some(next) = next_opt else {
                break;
            };
            let e = if curr < next { (curr, next) } else { (next, curr) };
            used.insert(e);
            ring.push(next);
            prev = curr;
            curr = next;
        }

        if ring.len() < 4 || *ring.last().unwrap_or(&usize::MAX) != a {
            continue;
        }
        ring.pop();
        if ring.len() < 3 {
            continue;
        }

        let mut center = Vec3::new(0.0, 0.0, 0.0);
        for &i in &ring {
            center = center.add(points[i]);
        }
        center = center.scale(1.0 / ring.len() as f64);

        let mut ordered = ring.clone();
        ordered.sort_by(|&i, &j| {
            let pi = points[i].sub(center);
            let pj = points[j].sub(center);
            let ai = pi.dot(v_axis).atan2(pi.dot(u_axis));
            let aj = pj.dot(v_axis).atan2(pj.dot(u_axis));
            ai.partial_cmp(&aj).unwrap_or(std::cmp::Ordering::Equal)
        });

        let center_idx = cap.vertices.len();
        cap.vertices.push(center);
        let mut ring_idx = Vec::with_capacity(ordered.len());
        for &pid in &ordered {
            ring_idx.push(cap.vertices.len());
            cap.vertices.push(points[pid]);
        }

        let mut area_sign = 0.0;
        for i in 0..ordered.len() {
            let p = points[ordered[i]].sub(center);
            let q = points[ordered[(i + 1) % ordered.len()]].sub(center);
            area_sign += p.cross(q).dot(normal);
        }
        let ccw = area_sign >= 0.0;

        for i in 0..ring_idx.len() {
            let a = ring_idx[i];
            let b = ring_idx[(i + 1) % ring_idx.len()];
            if ccw {
                cap.faces.push(crate::types::Triangle { a: center_idx, b, c: a });
            } else {
                cap.faces.push(crate::types::Triangle {
                    a: center_idx,
                    b: a,
                    c: b,
                });
            }
        }
    }

    cap
}

fn clip_mesh_by_plane_with_cap(mesh: &Mesh, origin: Vec3, normal: Vec3) -> Mesh {
    // Purpose: Clip mesh by plane and close open boundary with cap triangles.
    // Inputs: mesh, plane origin, plane normal.
    // Outputs: clipped watertight mesh approximation.
    let eps = 1e-9;
    let mut body = Mesh::empty();
    let mut segments: Vec<(Vec3, Vec3)> = Vec::new();

    for f in &mesh.faces {
        let tri = [mesh.vertices[f.a], mesh.vertices[f.b], mesh.vertices[f.c]];
        let clipped_poly = clip_polygon_with_plane(&tri, origin, normal, eps);
        if clipped_poly.len() >= 3 {
            let base = body.vertices.len();
            body.vertices.extend(clipped_poly.iter().copied());
            for i in 1..(clipped_poly.len() - 1) {
                body.faces.push(crate::types::Triangle {
                    a: base,
                    b: base + i,
                    c: base + i + 1,
                });
            }
        }

        if let Some(seg) = collect_triangle_plane_segment(tri, origin, normal, eps) {
            segments.push(seg);
        }
    }

    if segments.is_empty() {
        body
    } else {
        merge_meshes(&[body, triangulate_cap_from_segments(&segments, normal)])
    }
}

pub fn clip_mesh_by_bbox(mesh: &Mesh, bbox: BoundingBox) -> Mesh {
    // Purpose: Clip mesh by all six bbox planes with capping.
    // Inputs: source mesh and target bounding box.
    // Outputs: clipped mesh.
    let planes = [
        (Vec3::new(bbox.min.x, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
        (Vec3::new(bbox.max.x, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)),
        (Vec3::new(0.0, bbox.min.y, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        (Vec3::new(0.0, bbox.max.y, 0.0), Vec3::new(0.0, -1.0, 0.0)),
        (Vec3::new(0.0, 0.0, bbox.min.z), Vec3::new(0.0, 0.0, 1.0)),
        (Vec3::new(0.0, 0.0, bbox.max.z), Vec3::new(0.0, 0.0, -1.0)),
    ];

    let mut out = mesh.clone();
    for (origin, normal) in planes {
        out = clip_mesh_by_plane_with_cap(&out, origin, normal);
        if out.is_empty() {
            break;
        }
    }
    out
}

/// Compute volume of one particle within bbox by clipping then measuring.
/// Inputs: particle mesh and bbox.
/// Outputs: positive in-box volume.
pub fn particle_volume_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64 {
    let clipped = clip_mesh_by_bbox(mesh, bbox);
    mesh_volume(&clipped)
}

pub fn volume_fraction_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64 {
    // Purpose: Compute true in-box VF for one mesh.
    // Inputs: mesh and bbox.
    // Outputs: volume fraction in [0, 1].
    volume_fraction_of_meshes_in_bbox(std::slice::from_ref(mesh), bbox)
}

pub fn volume_fraction_of_meshes_in_bbox(meshes: &[Mesh], bbox: BoundingBox) -> f64 {
    // Purpose: Compute true in-box VF for multiple meshes by per-granule clipping.
    // Inputs: mesh list and bbox.
    // Outputs: volume fraction in [0, 1].
    let box_volume = bbox.volume().max(1e-12);
    let mut in_box_volume = 0.0;

    for mesh in meshes {
        let parts = split_mesh_into_granules(mesh);
        if parts.is_empty() {
            in_box_volume += particle_volume_in_bbox(mesh, bbox);
        } else {
            for part in &parts {
                in_box_volume += particle_volume_in_bbox(part, bbox);
            }
        }
    }

    (in_box_volume / box_volume).clamp(0.0, 1.0)
}

fn index_3d_to_flat(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize {
    // Purpose: Convert 3D voxel index to flat array index.
    // Inputs: x/y/z index and grid strides.
    // Outputs: flat index.
    x * ny * nz + y * nz + z
}

fn ray_intersects_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    // Purpose: Ray-triangle intersection using Moller-Trumbore method.
    // Inputs: ray origin/direction and triangle vertices.
    // Outputs: hit distance t when intersecting.
    let eps = 1e-10;
    let edge1 = b.sub(a);
    let edge2 = c.sub(a);
    let h = dir.cross(edge2);
    let det = edge1.dot(h);
    if det.abs() <= eps {
        return None;
    }

    let inv_det = 1.0 / det;
    let s = origin.sub(a);
    let u = inv_det * s.dot(h);
    if !(0.0 - eps..=1.0 + eps).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = inv_det * dir.dot(q);
    if v < -eps || (u + v) > 1.0 + eps {
        return None;
    }

    let t = inv_det * edge2.dot(q);
    if t > eps {
        Some(t)
    } else {
        None
    }
}

fn point_inside_mesh(mesh: &Mesh, point: Vec3) -> bool {
    // Purpose: Test point inclusion in mesh by ray casting.
    // Inputs: mesh and query point.
    // Outputs: true when point is inside.
    let Some(bb) = mesh_bbox(mesh) else {
        return false;
    };
    let eps = 1e-9;
    if point.x < bb.min.x - eps
        || point.y < bb.min.y - eps
        || point.z < bb.min.z - eps
        || point.x > bb.max.x + eps
        || point.y > bb.max.y + eps
        || point.z > bb.max.z + eps
    {
        return false;
    }

    let dir = Vec3::new(0.9428090415820634, 0.2705980500730985, 0.19611613513818402);
    let mut ts: Vec<f64> = Vec::new();
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        if let Some(t) = ray_intersects_triangle(point, dir, a, b, c) {
            ts.push(t);
        }
    }

    if ts.is_empty() {
        return false;
    }

    ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mut unique_hits = 0usize;
    let mut last_t = f64::NEG_INFINITY;
    for t in ts {
        if (t - last_t).abs() > 1e-8 {
            unique_hits += 1;
            last_t = t;
        }
    }

    unique_hits % 2 == 1
}

fn build_bbox_occupancy(mesh: &Mesh, bbox: BoundingBox, voxel_pitch: f64) -> (Vec<bool>, [usize; 3]) {
    // Purpose: Build voxel occupancy grid within bbox.
    // Inputs: mesh, bbox, voxel pitch.
    // Outputs: occupancy flags and grid dimensions.
    let size = bbox.size();
    let nx = ((size.x / voxel_pitch).ceil() as usize).max(1);
    let ny = ((size.y / voxel_pitch).ceil() as usize).max(1);
    let nz = ((size.z / voxel_pitch).ceil() as usize).max(1);
    let mut occ = vec![false; nx * ny * nz];

    let parts = split_mesh_into_granules(mesh);
    let mut part_ranges = Vec::new();
    for p in &parts {
        let Some(pb) = mesh_bbox(p) else {
            continue;
        };

        let x0 = (((pb.min.x - bbox.min.x) / voxel_pitch).floor() as isize).max(0) as usize;
        let y0 = (((pb.min.y - bbox.min.y) / voxel_pitch).floor() as isize).max(0) as usize;
        let z0 = (((pb.min.z - bbox.min.z) / voxel_pitch).floor() as isize).max(0) as usize;

        let x1 = (((pb.max.x - bbox.min.x) / voxel_pitch).ceil() as isize).min(nx as isize) as usize;
        let y1 = (((pb.max.y - bbox.min.y) / voxel_pitch).ceil() as isize).min(ny as isize) as usize;
        let z1 = (((pb.max.z - bbox.min.z) / voxel_pitch).ceil() as isize).min(nz as isize) as usize;

        if x0 < x1 && y0 < y1 && z0 < z1 {
            part_ranges.push((p, x0, x1, y0, y1, z0, z1));
        }
    }

    occ.par_chunks_mut(ny * nz)
        .enumerate()
        .for_each(|(x, slab)| {
            let cx = bbox.min.x + (x as f64 + 0.5) * voxel_pitch;

            for (p, x0, x1, y0, y1, z0, z1) in &part_ranges {
                if x < *x0 || x >= *x1 {
                    continue;
                }

                for y in *y0..*y1 {
                    let cy = bbox.min.y + (y as f64 + 0.5) * voxel_pitch;
                    for z in *z0..*z1 {
                        let idx = y * nz + z;
                        if slab[idx] {
                            continue;
                        }

                        let center = Vec3::new(
                            cx,
                            cy,
                            bbox.min.z + (z as f64 + 0.5) * voxel_pitch,
                        );
                        if point_inside_mesh(p, center) {
                            slab[idx] = true;
                        }
                    }
                }
            }
        });

    (occ, [nx, ny, nz])
}

fn shell_offsets_for_distance(distance_vox: f64, half_width_vox: f64) -> Vec<[isize; 3]> {
    // Purpose: Enumerate integer offsets near a voxel-space shell distance.
    // Inputs: shell center distance and half-width in voxel units.
    // Outputs: voxel offset vectors.
    if distance_vox <= 1e-12 {
        return vec![[0, 0, 0]];
    }

    let low2 = (distance_vox - half_width_vox).max(0.0).powi(2);
    let high2 = (distance_vox + half_width_vox).powi(2);
    let lim = (distance_vox + half_width_vox + 1.0).ceil() as isize;
    let mut offsets = Vec::new();

    for dx in -lim..=lim {
        for dy in -lim..=lim {
            for dz in -lim..=lim {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                let d2 = (dx * dx + dy * dy + dz * dz) as f64;
                if d2 >= low2 && d2 < high2 {
                    offsets.push([dx, dy, dz]);
                }
            }
        }
    }

    offsets
}

fn fill_missing_s2_with_smooth_interpolation(values: &mut [f64], has_support: &[bool], vf: f64) {
    // Purpose: Smoothly fill unsupported S2 radii using known points.
    // Inputs: mutable S2 values, support flags per radius, and S2(0)=VF.
    // Outputs: in-place interpolation for unsupported interior radii.
    if values.is_empty() || values.len() != has_support.len() {
        return;
    }

    values[0] = vf;

    let mut x_known: Vec<usize> = Vec::new();
    let mut y_known: Vec<f64> = Vec::new();
    for i in 0..values.len() {
        if i == 0 || has_support[i] {
            x_known.push(i);
            y_known.push(values[i]);
        }
    }

    if x_known.len() < 2 {
        return;
    }

    if x_known.len() == 2 {
        let xl = x_known[0];
        let xr = x_known[1];
        if xr <= xl + 1 {
            return;
        }
        let yl = y_known[0];
        let yr = y_known[1];
        let width = (xr - xl) as f64;
        for i in (xl + 1)..xr {
            if has_support[i] {
                continue;
            }
            let t = (i - xl) as f64 / width;
            let s = t * t * (3.0 - 2.0 * t);
            values[i] = (yl + (yr - yl) * s).clamp(0.0, 1.0);
        }
        return;
    }

    let m = x_known.len();
    let xs: Vec<f64> = x_known.iter().map(|&x| x as f64).collect();
    let ys = y_known;

    let mut h = vec![0.0f64; m - 1];
    for i in 0..(m - 1) {
        h[i] = (xs[i + 1] - xs[i]).max(1e-12);
    }

    let mut a = vec![0.0f64; m];
    let mut b = vec![0.0f64; m];
    let mut c = vec![0.0f64; m];
    let mut d = vec![0.0f64; m];

    b[0] = 1.0;
    b[m - 1] = 1.0;
    for i in 1..(m - 1) {
        a[i] = h[i - 1];
        b[i] = 2.0 * (h[i - 1] + h[i]);
        c[i] = h[i];
        d[i] = 6.0 * ((ys[i + 1] - ys[i]) / h[i] - (ys[i] - ys[i - 1]) / h[i - 1]);
    }

    for i in 1..m {
        let denom = if b[i - 1].abs() < 1e-12 { 1e-12 } else { b[i - 1] };
        let w = a[i] / denom;
        b[i] -= w * c[i - 1];
        d[i] -= w * d[i - 1];
    }

    let mut m2 = vec![0.0f64; m];
    let last_denom = if b[m - 1].abs() < 1e-12 { 1e-12 } else { b[m - 1] };
    m2[m - 1] = d[m - 1] / last_denom;
    for i in (0..(m - 1)).rev() {
        let denom = if b[i].abs() < 1e-12 { 1e-12 } else { b[i] };
        m2[i] = (d[i] - c[i] * m2[i + 1]) / denom;
    }

    for seg in 0..(m - 1) {
        let xl = x_known[seg];
        let xr = x_known[seg + 1];
        if xr <= xl + 1 {
            continue;
        }

        let x0 = xs[seg];
        let x1 = xs[seg + 1];
        let y0 = ys[seg];
        let y1 = ys[seg + 1];
        let hseg = (x1 - x0).max(1e-12);
        let m20 = m2[seg];
        let m21 = m2[seg + 1];

        for i in (xl + 1)..xr {
            if has_support[i] {
                continue;
            }
            let x = i as f64;
            let acoef = (x1 - x) / hseg;
            let bcoef = (x - x0) / hseg;
            let y = acoef * y0
                + bcoef * y1
                + ((acoef * acoef * acoef - acoef) * m20
                    + (bcoef * bcoef * bcoef - bcoef) * m21)
                    * (hseg * hseg / 6.0);
            values[i] = y.clamp(0.0, 1.0);
        }
    }
}

fn fft_index_3d(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize {
    // Purpose: Convert 3D FFT-grid index to flat index.
    // Inputs: x/y/z and y/z strides.
    // Outputs: flat index.
    x * ny * nz + y * nz + z
}

fn fft_3d_in_place(data: &mut [Complex<f64>], nx: usize, ny: usize, nz: usize, inverse: bool) {
    // Purpose: Perform separable 3D FFT/IFFT in place.
    // Inputs: complex data buffer, dimensions, inverse flag.
    // Outputs: transformed data buffer.
    let mut planner = FftPlanner::<f64>::new();
    let fft_z = if inverse {
        planner.plan_fft_inverse(nz)
    } else {
        planner.plan_fft_forward(nz)
    };
    data.par_chunks_mut(nz).for_each(|line| {
        fft_z.process(line);
    });

    let fft_y = if inverse {
        planner.plan_fft_inverse(ny)
    } else {
        planner.plan_fft_forward(ny)
    };
    data.par_chunks_mut(ny * nz).for_each(|x_slab| {
        let mut tmp_y = vec![Complex::<f64>::new(0.0, 0.0); ny];
        for z in 0..nz {
            for y in 0..ny {
                tmp_y[y] = x_slab[y * nz + z];
            }
            fft_y.process(&mut tmp_y);
            for y in 0..ny {
                x_slab[y * nz + z] = tmp_y[y];
            }
        }
    });

    let fft_x = if inverse {
        planner.plan_fft_inverse(nx)
    } else {
        planner.plan_fft_forward(nx)
    };
    let mut tmp_x = vec![Complex::<f64>::new(0.0, 0.0); nx];
    for y in 0..ny {
        for z in 0..nz {
            for x in 0..nx {
                tmp_x[x] = data[fft_index_3d(x, y, z, ny, nz)];
            }
            fft_x.process(&mut tmp_x);
            for x in 0..nx {
                data[fft_index_3d(x, y, z, ny, nz)] = tmp_x[x];
            }
        }
    }

    if inverse {
        let norm = (nx * ny * nz) as f64;
        for v in data.iter_mut() {
            *v /= norm;
        }
    }
}

fn autocorrelation_counts_fft(occ: &[bool], nx: usize, ny: usize, nz: usize) -> (Vec<f64>, [usize; 3]) {
    // Purpose: Compute occupancy autocorrelation counts via FFT convolution.
    // Inputs: occupancy grid and dimensions.
    // Outputs: correlation grid and padded FFT dimensions.
    let fx = (2 * nx).saturating_sub(1).max(1);
    let fy = (2 * ny).saturating_sub(1).max(1);
    let fz = (2 * nz).saturating_sub(1).max(1);
    let mut grid = vec![Complex::<f64>::new(0.0, 0.0); fx * fy * fz];

    for x in 0..nx {
        for y in 0..ny {
            for z in 0..nz {
                if occ[index_3d_to_flat(x, y, z, ny, nz)] {
                    grid[fft_index_3d(x, y, z, fy, fz)] = Complex::new(1.0, 0.0);
                }
            }
        }
    }

    fft_3d_in_place(&mut grid, fx, fy, fz, false);
    for v in grid.iter_mut() {
        *v *= v.conj();
    }
    fft_3d_in_place(&mut grid, fx, fy, fz, true);

    let corr = grid.iter().map(|v| v.re.max(0.0)).collect::<Vec<_>>();
    (corr, [fx, fy, fz])
}

fn calculate_s2_exact_direct(
    occ: &[bool],
    nx: usize,
    ny: usize,
    nz: usize,
    r_max: usize,
    voxel_pitch: f64,
    vf: f64,
) -> Vec<f64> {
    // Purpose: Compute exact S2 by direct offset pair enumeration.
    // Inputs: occupancy grid, dimensions, max radius, VF at r=0.
    // Outputs: S2 values for r=0..r_max.
    let results: Vec<(f64, bool)> = (0..=r_max)
        .into_par_iter()
        .map(|r| {
            if r == 0 {
                return (vf, true);
            }

            let r_vox = r as f64 / voxel_pitch;
            let half_width_vox = 0.5 / voxel_pitch;
            let offsets = shell_offsets_for_distance(r_vox, half_width_vox);
            let mut shell_sum = 0.0;
            let mut used_offsets = 0usize;

            for off in &offsets {
                let dx = off[0];
                let dy = off[1];
                let dz = off[2];

                let x_start = if dx < 0 { (-dx) as usize } else { 0 };
                let y_start = if dy < 0 { (-dy) as usize } else { 0 };
                let z_start = if dz < 0 { (-dz) as usize } else { 0 };

                let x_end = if dx > 0 { nx.saturating_sub(dx as usize) } else { nx };
                let y_end = if dy > 0 { ny.saturating_sub(dy as usize) } else { ny };
                let z_end = if dz > 0 { nz.saturating_sub(dz as usize) } else { nz };

                if x_start >= x_end || y_start >= y_end || z_start >= z_end {
                    continue;
                }

                let mut valid_pairs = 0usize;
                let mut hit_pairs = 0usize;

                for x in x_start..x_end {
                    for y in y_start..y_end {
                        for z in z_start..z_end {
                            let x2 = (x as isize + dx) as usize;
                            let y2 = (y as isize + dy) as usize;
                            let z2 = (z as isize + dz) as usize;

                            let i1 = index_3d_to_flat(x, y, z, ny, nz);
                            let i2 = index_3d_to_flat(x2, y2, z2, ny, nz);
                            valid_pairs += 1;
                            if occ[i1] && occ[i2] {
                                hit_pairs += 1;
                            }
                        }
                    }
                }

                if valid_pairs > 0 {
                    shell_sum += hit_pairs as f64 / valid_pairs as f64;
                    used_offsets += 1;
                }
            }

            if used_offsets == 0 {
                (0.0, false)
            } else {
                (shell_sum / used_offsets as f64, true)
            }
        })
        .collect();

    let mut out = vec![0.0; r_max + 1];
    let mut has_support = vec![false; r_max + 1];
    for (r, (value, supported)) in results.into_iter().enumerate() {
        out[r] = value;
        has_support[r] = supported;
    }
    fill_missing_s2_with_smooth_interpolation(&mut out, &has_support, vf);
    out[0] = vf;
    out
}

fn calculate_s2_exact_fft(
    occ: &[bool],
    nx: usize,
    ny: usize,
    nz: usize,
    r_max: usize,
    voxel_pitch: f64,
    vf: f64,
) -> Vec<f64> {
    // Purpose: Compute exact S2 using FFT-based autocorrelation.
    // Inputs: occupancy grid, dimensions, max radius, VF at r=0.
    // Outputs: S2 values for r=0..r_max.
    let (corr, fdims) = autocorrelation_counts_fft(occ, nx, ny, nz);
    let [fx, fy, fz] = fdims;

    let get_corr = |dx: isize, dy: isize, dz: isize| {
        let ix = if dx >= 0 { dx as usize } else { (fx as isize + dx) as usize };
        let iy = if dy >= 0 { dy as usize } else { (fy as isize + dy) as usize };
        let iz = if dz >= 0 { dz as usize } else { (fz as isize + dz) as usize };
        corr[fft_index_3d(ix, iy, iz, fy, fz)]
    };

    let results: Vec<(f64, bool)> = (0..=r_max)
        .into_par_iter()
        .map(|r| {
            if r == 0 {
                return (vf, true);
            }

            let r_vox = r as f64 / voxel_pitch;
            let half_width_vox = 0.5 / voxel_pitch;
            let offsets = shell_offsets_for_distance(r_vox, half_width_vox);
            let mut shell_sum = 0.0;
            let mut used = 0usize;

            for off in &offsets {
                let dx = off[0];
                let dy = off[1];
                let dz = off[2];

                let adx = dx.unsigned_abs();
                let ady = dy.unsigned_abs();
                let adz = dz.unsigned_abs();
                if adx >= nx || ady >= ny || adz >= nz {
                    continue;
                }

                let valid_pairs = (nx - adx) * (ny - ady) * (nz - adz);
                if valid_pairs == 0 {
                    continue;
                }

                let hits = get_corr(dx, dy, dz);
                shell_sum += hits / valid_pairs as f64;
                used += 1;
            }

            if used == 0 {
                (0.0, false)
            } else {
                (shell_sum / used as f64, true)
            }
        })
        .collect();

    let mut out = vec![0.0; r_max + 1];
    let mut has_support = vec![false; r_max + 1];
    for (r, (value, supported)) in results.into_iter().enumerate() {
        out[r] = value;
        has_support[r] = supported;
    }
    fill_missing_s2_with_smooth_interpolation(&mut out, &has_support, vf);
    out[0] = vf;
    out
}

fn calculate_s2_monte_carlo_mesh(
    mesh: &Mesh,
    bbox: BoundingBox,
    r_max: usize,
    samples: usize,
) -> Vec<f64> {
    // Purpose: Compute S2 with direct mesh point-inclusion Monte Carlo at real distances.
    // Inputs: mesh, bbox, max real distance, and MC samples per radius.
    // Outputs: S2 values for r=0..r_max.
    let vf = volume_fraction_in_bbox(mesh, bbox);
    let mc_samples = samples.max(200);
    let min = bbox.min;
    let max = bbox.max;

    let mut out: Vec<f64> = (0..=r_max)
        .into_par_iter()
        .map(|r| {
            if r == 0 {
                return vf;
            }

            let rr = r as f64;
            let mut valid = 0usize;
            let mut hits = 0usize;
            let mut rng = rand::thread_rng();

            for _ in 0..mc_samples {
                let p = Vec3::new(
                    rng.gen_range(min.x..max.x),
                    rng.gen_range(min.y..max.y),
                    rng.gen_range(min.z..max.z),
                );

                let dir = loop {
                    let x = rng.gen_range(-1.0f64..1.0f64);
                    let y = rng.gen_range(-1.0f64..1.0f64);
                    let z = rng.gen_range(-1.0f64..1.0f64);
                    let n2: f64 = x * x + y * y + z * z;
                    if n2 > 1e-12 && n2 <= 1.0 {
                        let inv = 1.0 / n2.sqrt();
                        break Vec3::new(x * inv, y * inv, z * inv);
                    }
                };

                let q = p.add(dir.scale(rr));
                if q.x < min.x
                    || q.x > max.x
                    || q.y < min.y
                    || q.y > max.y
                    || q.z < min.z
                    || q.z > max.z
                {
                    continue;
                }

                valid += 1;
                if point_inside_mesh(mesh, p) && point_inside_mesh(mesh, q) {
                    hits += 1;
                }
            }

            if valid == 0 {
                0.0
            } else {
                hits as f64 / valid as f64
            }
        })
        .collect();
    out[0] = vf;
    out
}

pub fn calculate_s2(
    mesh: &Mesh,
    bbox: BoundingBox,
    r_max: usize,
    voxel_pitch: f64,
    method: &str,
    samples: usize,
) -> Vec<f64> {
    // Purpose: Compute S2 by configured method (exact or monte_carlo).
    // Inputs: mesh, bbox, r_max, voxel pitch, method name, sample count.
    // Outputs: S2 values indexed by radius.
    if method != "exact" && voxel_pitch <= 0.0 {
        return calculate_s2_monte_carlo_mesh(mesh, bbox, r_max, samples);
    }

    let effective_pitch = if voxel_pitch <= 0.0 {
        println!(
            "[Warning] exact S2 requested with voxel_pitch <= 0; falling back to voxel_pitch=1.0"
        );
        1.0
    } else {
        voxel_pitch
    };

    let (occ, dims) = build_bbox_occupancy(mesh, bbox, effective_pitch.max(1e-9));
    let [nx, ny, nz] = dims;
    let total_vox = (nx * ny * nz).max(1);
    let occupied_count = occ.iter().filter(|&&v| v).count();
    let vf = occupied_count as f64 / total_vox as f64;

    if occupied_count == 0 {
        return vec![0.0; r_max + 1];
    }

    match method {
        "exact" => {
            let fx = (2 * nx).saturating_sub(1).max(1);
            let fy = (2 * ny).saturating_sub(1).max(1);
            let fz = (2 * nz).saturating_sub(1).max(1);
            let fft_cells = fx.saturating_mul(fy).saturating_mul(fz);
            let max_fft_cells = 24_000_000usize;

            if fft_cells > max_fft_cells {
                calculate_s2_exact_direct(&occ, nx, ny, nz, r_max, effective_pitch.max(1e-9), vf)
            } else {
                calculate_s2_exact_fft(&occ, nx, ny, nz, r_max, effective_pitch.max(1e-9), vf)
            }
        }
        _ => {
            let mc_samples = samples.max(200);
            let pitch = effective_pitch.max(1e-9);
            let half_width_vox = 0.5 / pitch;
            let shells: Vec<Vec<[isize; 3]>> = (1..=r_max)
                .map(|r| shell_offsets_for_distance(r as f64 / pitch, half_width_vox))
                .collect();
            let results: Vec<(f64, bool)> = (0..=r_max)
                .into_par_iter()
                .map(|r| {
                    if r == 0 {
                        return (vf, true);
                    }

                    let offsets = &shells[r - 1];
                    if offsets.is_empty() {
                        return (0.0, false);
                    }
                    let mut valid = 0usize;
                    let mut hits = 0usize;
                    let mut rng = rand::thread_rng();

                    for _ in 0..mc_samples {
                        let x = rng.gen_range(0..nx);
                        let y = rng.gen_range(0..ny);
                        let z = rng.gen_range(0..nz);
                        let off = &offsets[rng.gen_range(0..offsets.len())];

                        let x2 = x as isize + off[0];
                        let y2 = y as isize + off[1];
                        let z2 = z as isize + off[2];
                        if x2 < 0 || y2 < 0 || z2 < 0 {
                            continue;
                        }
                        let x2u = x2 as usize;
                        let y2u = y2 as usize;
                        let z2u = z2 as usize;
                        if x2u >= nx || y2u >= ny || z2u >= nz {
                            continue;
                        }

                        valid += 1;
                        let i1 = index_3d_to_flat(x, y, z, ny, nz);
                        let i2 = index_3d_to_flat(x2u, y2u, z2u, ny, nz);
                        if occ[i1] && occ[i2] {
                            hits += 1;
                        }
                    }

                    if valid == 0 {
                        (0.0, false)
                    } else {
                        (hits as f64 / valid as f64, true)
                    }
                })
                .collect();

            let mut out = vec![0.0; r_max + 1];
            let mut has_support = vec![false; r_max + 1];
            for (r, (value, supported)) in results.into_iter().enumerate() {
                out[r] = value;
                has_support[r] = supported;
            }
            fill_missing_s2_with_smooth_interpolation(&mut out, &has_support, vf);
            out[0] = vf;
            out
        }
    }
}

pub fn approximate_s2(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64> {
    // Purpose: Convenience wrapper for Monte Carlo S2 estimation.
    // Inputs: mesh, bbox, max radius, sample count.
    // Outputs: S2 values.
    calculate_s2(mesh, bbox, r_max, 1.0, "monte_carlo", samples)
}

pub fn l2_norm(a: &[f64], b: &[f64]) -> f64 {
    // Purpose: Compute L2 norm between two vectors over common length.
    // Inputs: two numeric slices.
    // Outputs: non-negative L2 distance.
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let sum = (0..n).map(|i| {
        let d = a[i] - b[i];
        d * d
    });
    sum.sum::<f64>().sqrt()
}
