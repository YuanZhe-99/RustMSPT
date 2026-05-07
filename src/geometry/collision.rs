use crate::types::{BoundingBox, Mesh, Vec3};
use super::bbox::{bbox_distance, bbox_overlaps, mesh_bbox};
use super::mesh_ops::translate_mesh;
use parry3d_f64::math::{Isometry, Point};
use parry3d_f64::query;
use parry3d_f64::shape::TriMesh;

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

pub fn mesh_collision_exact_prepared(
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

pub fn mesh_collision_exact(a: &Mesh, b: &Mesh) -> bool {
    let a_bbox = mesh_bbox(a);
    let b_bbox = mesh_bbox(b);
    let a_shape = to_parry_trimesh(a);
    let b_shape = to_parry_trimesh(b);
    mesh_collision_exact_prepared(a_bbox, a_shape.as_ref(), b_bbox, b_shape.as_ref())
}

pub fn mesh_distance_exact(a: &Mesh, b: &Mesh) -> f64 {
    let a_bbox = mesh_bbox(a);
    let b_bbox = mesh_bbox(b);
    let a_shape = to_parry_trimesh(a);
    let b_shape = to_parry_trimesh(b);
    mesh_distance_exact_prepared(a_bbox, a_shape.as_ref(), b_bbox, b_shape.as_ref())
}

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
