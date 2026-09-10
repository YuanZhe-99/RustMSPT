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
// Notes: A thin wrapper over mesh_closedness, which says *why* a mesh fails. Callers that report a
// rejection to a user should use that instead: "shell 3 of particles.stl has 12 boundary edges" is
// actionable where "not closed" is not.
pub fn mesh_is_closed(mesh: &Mesh) -> bool {
    mesh_closedness(mesh).is_ok()
}

// AI-FUNC-SUMMARY:
// Purpose: Decide whether a mesh is a consistently oriented closed manifold, and if not, say why.
// Inputs: candidate mesh.
// Returns: Ok(()) when closed; Err(reason) naming the first violation found, with counts where they help.
// Side effects: None.
// Notes: The reason is written to be shown to a user, so it names quantities they can look for in
// their own file. Checks run in the order they can be decided cheaply: emptiness and non-finite
// coordinates, then face validity and duplication, then edge incidence and winding, then per-shell
// signed volume. Only the first failure is reported; a mesh with several problems is fixed one at
// a time anyway.
pub fn mesh_closedness(mesh: &Mesh) -> std::result::Result<(), String> {
    if mesh.is_empty() {
        return Err("the mesh has no vertices or no faces".to_string());
    }
    if let Some(index) = mesh
        .vertices
        .iter()
        .position(|v| !v.x.is_finite() || !v.y.is_finite() || !v.z.is_finite())
    {
        return Err(format!("vertex {index} has a non-finite coordinate"));
    }

    let mut edges: HashMap<(usize, usize), (usize, i32, Vec<usize>)> = HashMap::new();
    let mut faces = HashSet::new();
    for (face_index, face) in mesh.faces.iter().enumerate() {
        if face.a >= mesh.vertices.len()
            || face.b >= mesh.vertices.len()
            || face.c >= mesh.vertices.len()
        {
            return Err(format!(
                "face {face_index} indexes a vertex that does not exist"
            ));
        }
        if face.a == face.b || face.b == face.c || face.c == face.a {
            return Err(format!("face {face_index} is degenerate: it repeats a vertex"));
        }
        let mut face_key = [face.a, face.b, face.c];
        face_key.sort_unstable();
        if !faces.insert(face_key) {
            return Err(format!(
                "face {face_index} duplicates an earlier face on the same three vertices"
            ));
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

    if edges.is_empty() {
        return Err("the mesh has no edges".to_string());
    }
    let boundary = edges.values().filter(|(n, _, _)| *n == 1).count();
    let non_manifold = edges.values().filter(|(n, _, _)| *n > 2).count();
    let inconsistent = edges
        .values()
        .filter(|(n, dir, _)| *n == 2 && *dir != 0)
        .count();
    if boundary > 0 {
        return Err(format!(
            "the mesh is open: {boundary} edge(s) belong to only one face"
        ));
    }
    if non_manifold > 0 {
        return Err(format!(
            "the mesh is non-manifold: {non_manifold} edge(s) belong to more than two faces"
        ));
    }
    if inconsistent > 0 {
        return Err(format!(
            "face winding is inconsistent across {inconsistent} edge(s): the two faces sharing an \
             edge traverse it the same way instead of opposite ways"
        ));
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
            return Err(
                "a connected shell encloses zero volume, so it is a surface rather than a solid"
                    .to_string(),
            );
        }
    }
    Ok(())
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
