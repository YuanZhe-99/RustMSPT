use super::mesh_ops::mesh_surface_area;
use super::volume::mesh_volume;
use crate::types::Mesh;
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshMetrics {
    pub volume: f64,
    pub surface_area: f64,
    pub equivalent_diameter: f64,
    pub sphericity: f64,
}

// AI-FUNC-SUMMARY:
// Purpose: Validate finite indexed triangles, unique faces, manifold edge incidence, and nonzero volume for every edge-connected shell.
// Inputs: candidate mesh.
// Returns: true for consistently oriented closed shells with valid volume.
// Side effects: None.
fn mesh_is_closed(mesh: &Mesh) -> bool {
    if mesh.is_empty()
        || mesh
            .vertices
            .iter()
            .any(|vertex| !vertex.x.is_finite() || !vertex.y.is_finite() || !vertex.z.is_finite())
    {
        return false;
    }

    let mut edges: HashMap<(usize, usize), (usize, i32, Vec<usize>)> = HashMap::new();
    let mut faces = HashSet::new();
    for (face_index, face) in mesh.faces.iter().enumerate() {
        if face.a >= mesh.vertices.len()
            || face.b >= mesh.vertices.len()
            || face.c >= mesh.vertices.len()
            || face.a == face.b
            || face.b == face.c
            || face.c == face.a
        {
            return false;
        }
        let mut face_key = [face.a, face.b, face.c];
        face_key.sort_unstable();
        if !faces.insert(face_key) {
            return false;
        }
        for (from, to) in [(face.a, face.b), (face.b, face.c), (face.c, face.a)] {
            let key = if from < to { (from, to) } else { (to, from) };
            let direction = if from < to { 1 } else { -1 };
            let entry = edges
                .entry(key)
                .or_insert_with(|| (0, 0, Vec::with_capacity(2)));
            entry.0 += 1;
            entry.1 += direction;
            entry.2.push(face_index);
        }
    }

    if edges.is_empty()
        || edges
            .values()
            .any(|(incidence, direction, _)| *incidence != 2 || *direction != 0)
    {
        return false;
    }

    let mut neighbors = vec![Vec::new(); mesh.faces.len()];
    for (_, _, owners) in edges.values() {
        neighbors[owners[0]].push(owners[1]);
        neighbors[owners[1]].push(owners[0]);
    }
    let mut visited = vec![false; mesh.faces.len()];
    for start in 0..mesh.faces.len() {
        if visited[start] {
            continue;
        }
        let mut stack = vec![start];
        visited[start] = true;
        let mut signed_volume = 0.0;
        while let Some(face_index) = stack.pop() {
            let face = &mesh.faces[face_index];
            let a = mesh.vertices[face.a];
            let b = mesh.vertices[face.b];
            let c = mesh.vertices[face.c];
            signed_volume += a.dot(b.cross(c)) / 6.0;
            for &neighbor in &neighbors[face_index] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        if !signed_volume.is_finite() || signed_volume == 0.0 {
            return false;
        }
    }
    true
}

// AI-FUNC-SUMMARY:
// Purpose: Compute full-mesh volume, surface area, equivalent-volume diameter, and sphericity in one pass.
// Inputs: closed candidate mesh.
// Returns: Some(MeshMetrics) for positive finite volume, area, and derived metrics; None otherwise.
// Side effects: None.
pub fn mesh_metrics(mesh: &Mesh) -> Option<MeshMetrics> {
    if !mesh_is_closed(mesh) {
        return None;
    }
    let volume = mesh_volume(mesh);
    let surface_area = mesh_surface_area(mesh);
    if !volume.is_finite() || !surface_area.is_finite() || volume <= 0.0 || surface_area <= 0.0 {
        return None;
    }

    let equivalent_diameter = (6.0 * volume / PI).powf(1.0 / 3.0);
    let sphericity = PI.powf(1.0 / 3.0) * (6.0 * volume).powf(2.0 / 3.0) / surface_area;
    if !equivalent_diameter.is_finite()
        || equivalent_diameter <= 0.0
        || !sphericity.is_finite()
        || sphericity <= 0.0
    {
        return None;
    }

    Some(MeshMetrics {
        volume,
        surface_area,
        equivalent_diameter,
        sphericity,
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Scale a mesh so its equivalent-volume diameter reaches a requested positive finite target.
// Inputs: mutable mesh, source metrics, and target equivalent diameter.
// Returns: Some(validated scale factor) after in-place scaling; None for invalid metrics or target.
// Side effects: Mutates mesh vertex positions.
pub fn scale_mesh_to_equivalent_diameter(
    mesh: &mut Mesh,
    metrics: MeshMetrics,
    target_diameter: f64,
) -> Option<f64> {
    if !target_diameter.is_finite() || target_diameter <= 0.0 {
        return None;
    }
    let factor = target_diameter / metrics.equivalent_diameter;
    if !factor.is_finite() || factor <= 0.0 {
        return None;
    }
    for vertex in &mut mesh.vertices {
        *vertex = vertex.scale(factor);
    }
    Some(factor)
}
