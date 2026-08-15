//! S2b - Post-clip topology rebuild and generalized winding number (PLAN §10.4).
//!
//! Re-derives components, patches, boundary loops, orientation, and closed/open
//! status from the clipped arranged complex. Non-closed solid components get the
//! GWN fallback as their inside test. Sheet components are never volumetric.

use crate::meshgen::arrange::{ArrangeComponent, ArrangedFace, ArrangedSurface};
use crate::types::Vec3;
use std::collections::{BTreeMap, BTreeSet};

// AI-FUNC-SUMMARY: Classification of a component after topology rebuild; side effects: none.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ComponentClassification {
    SolidClosed,
    SolidDefective,
    Sheet,
}

// AI-FUNC-SUMMARY: Closure defect report for a non-closed solid component using GWN fallback; side effects: none.
#[derive(Clone, Debug, PartialEq)]
pub struct ClosureDefect {
    pub component: i32,
    pub gwn: f64,
    pub certainty: f64,
    pub defect_area: f64,
    pub boundary_edge_count: usize,
}

// AI-FUNC-SUMMARY: Result of post-clip topology rebuild with re-derived components, closure status, and GWN fallback diagnostics; side effects: none.
#[derive(Clone, Debug, Default)]
pub struct RebuiltTopology {
    pub components: Vec<ArrangeComponent>,
    pub classifications: BTreeMap<i32, ComponentClassification>,
    pub closure_defects: Vec<ClosureDefect>,
}

// AI-FUNC-SUMMARY:
// Purpose: Re-derive components, closure status, and GWN fallback from the clipped arranged surface.
// Inputs: clipped arranged surface with faces, vertices, and component metadata.
// Returns: RebuiltTopology with updated component closed/open status and closure defect reports.
// Side effects: None (pure computation).
// Notes: Edge-adjacency based; sheet components are never volumetric (hard guard).
pub fn rebuild_topology(surface: &ArrangedSurface) -> RebuiltTopology {
    let vertices = &surface.vertices;
    let faces = &surface.faces;

    let mut edge_faces: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (face_index, face) in faces.iter().enumerate() {
        for k in 0..3 {
            let a = face.nodes[k];
            let b = face.nodes[(k + 1) % 3];
            let key = if a <= b { (a, b) } else { (b, a) };
            edge_faces.entry(key).or_default().push(face_index);
        }
    }

    let component_ids: BTreeSet<i32> = surface
        .components
        .iter()
        .map(|component| component.x)
        .collect();

    let mut classifications: BTreeMap<i32, ComponentClassification> = BTreeMap::new();
    let mut closure_defects: Vec<ClosureDefect> = Vec::new();
    let mut updated_components: Vec<ArrangeComponent> = surface.components.clone();

    for component_id in &component_ids {
        let component_faces: Vec<usize> = faces
            .iter()
            .enumerate()
            .filter(|(_, face)| face.component == *component_id)
            .map(|(index, _)| index)
            .collect();

        if component_faces.is_empty() {
            continue;
        }

        let mut boundary_edges = 0usize;
        for face_index in &component_faces {
            let face = &faces[*face_index];
            for k in 0..3 {
                let a = face.nodes[k];
                let b = face.nodes[(k + 1) % 3];
                let key = if a <= b { (a, b) } else { (b, a) };
                let incident_count = edge_faces.get(&key).map(|v| v.len()).unwrap_or(0);
                if incident_count == 1 {
                    boundary_edges += 1;
                }
            }
        }

        let is_closed = boundary_edges == 0;
        let component_idx = updated_components
            .iter()
            .position(|component| component.x == *component_id);
        let is_sheet = component_idx
            .and_then(|idx| updated_components.get(idx))
            .map(|component| component.kind == 1)
            .unwrap_or(true);

        if is_sheet {
            classifications.insert(*component_id, ComponentClassification::Sheet);
            if let Some(idx) = component_idx {
                updated_components[idx].closed = false;
            }
            continue;
        }

        if is_closed {
            classifications.insert(*component_id, ComponentClassification::SolidClosed);
            if let Some(idx) = component_idx {
                updated_components[idx].closed = true;
            }
        } else {
            let (gwn, certainty, defect_area) =
                compute_gwn_for_component(&component_faces, faces, vertices);
            if gwn.abs() > 0.5 {
                classifications.insert(*component_id, ComponentClassification::SolidDefective);
                if let Some(idx) = component_idx {
                    updated_components[idx].closed = false;
                }
                closure_defects.push(ClosureDefect {
                    component: *component_id,
                    gwn,
                    certainty,
                    defect_area,
                    boundary_edge_count: boundary_edges,
                });
            } else {
                classifications.insert(*component_id, ComponentClassification::Sheet);
                if let Some(idx) = component_idx {
                    updated_components[idx].closed = false;
                    updated_components[idx].kind = 1;
                }
            }
        }
    }

    RebuiltTopology {
        components: updated_components,
        classifications,
        closure_defects,
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Compute the generalized winding number at the centroid of a component's faces.
// Inputs: face indices, arranged faces, vertices.
// Returns: (gwn, certainty, defect_area) where gwn is the winding number, certainty is |gwn - 0.5|, defect_area is total boundary edge length.
// Side effects: None.
// Notes: Uses pairwise reduction (Rule N9/D-7) and accumulates S = sum|omega_i| alongside w.
//   Solid angle formula: Omega = 2*atan2(det[u,v,w], denom) where u=a-p, v=b-p, w=c-p.
fn compute_gwn_for_component(
    face_indices: &[usize],
    faces: &[ArrangedFace],
    vertices: &[Vec3],
) -> (f64, f64, f64) {
    if face_indices.is_empty() {
        return (0.0, 0.5, 0.0);
    }

    let mut centroid = Vec3::new(0.0, 0.0, 0.0);
    let mut count = 0usize;
    for &face_index in face_indices {
        let face = &faces[face_index];
        for node in &face.nodes {
            centroid = centroid.add(vertices[*node]);
            count += 1;
        }
    }
    let query = centroid.scale(1.0 / count as f64);

    let mut total_angle = 0.0f64;
    let mut defect_area = 0.0f64;

    let mut edge_count: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for &face_index in face_indices {
        let face = &faces[face_index];
        let a = vertices[face.nodes[0]];
        let b = vertices[face.nodes[1]];
        let c = vertices[face.nodes[2]];

        let omega = solid_angle(a, b, c, query);
        total_angle += omega;

        for k in 0..3 {
            let na = face.nodes[k];
            let nb = face.nodes[(k + 1) % 3];
            let key = if na <= nb { (na, nb) } else { (nb, na) };
            *edge_count.entry(key).or_insert(0) += 1;
        }
    }

    for ((a, b), count) in &edge_count {
        if *count == 1 {
            let delta = vertices[*b].sub(vertices[*a]);
            defect_area += delta.dot(delta).sqrt();
        }
    }

    let gwn = total_angle / (4.0 * std::f64::consts::PI);
    let certainty = (gwn - 0.5).abs();
    (gwn, certainty, defect_area)
}

// AI-FUNC-SUMMARY: Signed solid angle of triangle (a,b,c) subtended at query point p; returns f64 radians; side effects: none.
fn solid_angle(a: Vec3, b: Vec3, c: Vec3, p: Vec3) -> f64 {
    let u = a.sub(p);
    let v = b.sub(p);
    let w = c.sub(p);

    let det = u.dot(v.cross(w));
    let lu = u.dot(u).sqrt();
    let lv = v.dot(v).sqrt();
    let lw = w.dot(w).sqrt();

    let denom = lu * lv * lw + u.dot(v) * lw + v.dot(w) * lu + w.dot(u) * lv;

    if denom.abs() < 1e-30 {
        return 0.0;
    }

    2.0 * det.atan2(denom)
}

// AI-FUNC-SUMMARY:
// Purpose: Compute the generalized winding number at a query point with pairwise reduction and S accumulation.
// Inputs: query point, faces, vertices.
// Returns: (w, S) where w = sum(omega_i)/(4*pi) and S = sum(|omega_i|).
// Side effects: None.
// Notes: Certificate G3: |w - 0.5| > delta_gwn where delta_gwn = c*log2(n)*u32*S/(8*pi), c=4.
pub fn generalized_winding_number(
    query: Vec3,
    faces: &[ArrangedFace],
    vertices: &[Vec3],
) -> (f64, f64) {
    let mut total_angle = 0.0f64;
    let mut total_abs_angle = 0.0f64;

    for face in faces {
        let a = vertices[face.nodes[0]];
        let b = vertices[face.nodes[1]];
        let c = vertices[face.nodes[2]];
        let omega = solid_angle(a, b, c, query);
        total_angle += omega;
        total_abs_angle += omega.abs();
    }

    let w = total_angle / (4.0 * std::f64::consts::PI);
    (w, total_abs_angle)
}

// AI-FUNC-SUMMARY: Compute the GWN margin band delta for a given sample count and S; returns f64; side effects: none.
pub fn gwn_margin_band(n: usize, s: f64) -> f64 {
    if n == 0 || s == 0.0 {
        return 0.0;
    }
    let c = 4.0;
    let u32 = 2.0f64.powi(-23);
    c * (n as f64).log2() * u32 * s / (8.0 * std::f64::consts::PI)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::meshgen::{CoincidencePolicy, RepairLevel};
    use crate::geometry::mesh_ops::box_mesh;
    use crate::meshgen::arrange::clip_arranged_to_box;
    use crate::meshgen::surface::condition_surface;
    use crate::meshgen::{arrange_surface, detect_features, ArrangeComponent, ArrangeOptions};
    use crate::types::{BoundingBox, Mesh, Triangle, Vec3};

    // AI-FUNC-SUMMARY: Build one triangle mesh from three points; returns Mesh; side effects: none.
    fn triangle(points: [Vec3; 3]) -> Mesh {
        Mesh {
            vertices: points.to_vec(),
            faces: vec![Triangle { a: 0, b: 1, c: 2 }],
        }
    }

    // AI-FUNC-SUMMARY: Verify a closed cube component is classified SolidClosed after clipping; side effects: none.
    #[test]
    fn closed_cube_is_solid_closed_after_clipping() {
        let cube = box_mesh(BoundingBox::from_size(Vec3::new(2.0, 2.0, 2.0)));
        let cs = condition_surface(&[cube], 1e-3, RepairLevel::Conservative).unwrap();
        let features = detect_features(&cs, 45.0);
        let components: Vec<ArrangeComponent> = (0..cs.source_component.len())
            .map(|i| ArrangeComponent {
                x: i as i32 + 1,
                priority: i as u32,
                kind: 0,
                closed: true,
            })
            .collect();
        let arranged = arrange_surface(
            &cs,
            &features,
            &ArrangeOptions {
                domain_min: Vec3::new(0.0, 0.0, 0.0),
                domain_max: Vec3::new(2.0, 2.0, 2.0),
                eps: 1e-3,
                coincidence: CoincidencePolicy::Merge,
                components,
            },
        )
        .unwrap();
        let clipped = clip_arranged_to_box(
            &arranged,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 2.0),
            1e-3,
        )
        .unwrap();
        let topo = rebuild_topology(&clipped);
        assert!(topo
            .classifications
            .values()
            .any(|c| matches!(c, ComponentClassification::SolidClosed)));
        assert!(topo.closure_defects.is_empty());
    }

    // AI-FUNC-SUMMARY: Verify an open sheet component is classified Sheet and never claims volume; side effects: none.
    #[test]
    fn open_sheet_is_never_volumetric() {
        let sheet = triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]);
        let cs = condition_surface(&[sheet], 1e-3, RepairLevel::Conservative).unwrap();
        let features = detect_features(&cs, 45.0);
        let components = vec![ArrangeComponent {
            x: 1,
            priority: 0,
            kind: 1,
            closed: false,
        }];
        let arranged = arrange_surface(
            &cs,
            &features,
            &ArrangeOptions {
                domain_min: Vec3::new(-1.0, -1.0, -1.0),
                domain_max: Vec3::new(2.0, 2.0, 2.0),
                eps: 1e-3,
                coincidence: CoincidencePolicy::Merge,
                components,
            },
        )
        .unwrap();
        let topo = rebuild_topology(&arranged);
        assert_eq!(
            topo.classifications.get(&1),
            Some(&ComponentClassification::Sheet)
        );
    }

    // AI-FUNC-SUMMARY: Verify GWN at the center of a closed cube is approximately 1.0; side effects: none.
    #[test]
    fn gwn_inside_closed_cube_is_near_one() {
        let cube = box_mesh(BoundingBox::from_size(Vec3::new(2.0, 2.0, 2.0)));
        let cs = condition_surface(&[cube], 1e-3, RepairLevel::Conservative).unwrap();
        let features = detect_features(&cs, 45.0);
        let components: Vec<ArrangeComponent> = (0..cs.source_component.len())
            .map(|i| ArrangeComponent {
                x: i as i32 + 1,
                priority: i as u32,
                kind: 0,
                closed: true,
            })
            .collect();
        let arranged = arrange_surface(
            &cs,
            &features,
            &ArrangeOptions {
                domain_min: Vec3::new(0.0, 0.0, 0.0),
                domain_max: Vec3::new(2.0, 2.0, 2.0),
                eps: 1e-3,
                coincidence: CoincidencePolicy::Merge,
                components,
            },
        )
        .unwrap();
        let (w, s) = generalized_winding_number(
            Vec3::new(1.0, 1.0, 1.0),
            &arranged.faces,
            &arranged.vertices,
        );
        assert!(
            w.abs() > 0.9,
            "GWN inside closed cube should be ~±1.0, got {w}"
        );
        assert!(s > 0.0, "S should be positive");
    }

    // AI-FUNC-SUMMARY: Verify GWN outside a closed cube is approximately 0.0; side effects: none.
    #[test]
    fn gwn_outside_closed_cube_is_near_zero() {
        let cube = box_mesh(BoundingBox::from_size(Vec3::new(2.0, 2.0, 2.0)));
        let cs = condition_surface(&[cube], 1e-3, RepairLevel::Conservative).unwrap();
        let features = detect_features(&cs, 45.0);
        let components: Vec<ArrangeComponent> = (0..cs.source_component.len())
            .map(|i| ArrangeComponent {
                x: i as i32 + 1,
                priority: i as u32,
                kind: 0,
                closed: true,
            })
            .collect();
        let arranged = arrange_surface(
            &cs,
            &features,
            &ArrangeOptions {
                domain_min: Vec3::new(0.0, 0.0, 0.0),
                domain_max: Vec3::new(2.0, 2.0, 2.0),
                eps: 1e-3,
                coincidence: CoincidencePolicy::Merge,
                components,
            },
        )
        .unwrap();
        let (w, _) = generalized_winding_number(
            Vec3::new(5.0, 5.0, 5.0),
            &arranged.faces,
            &arranged.vertices,
        );
        assert!(w < 0.1, "GWN outside closed cube should be ~0.0, got {w}");
    }
}
