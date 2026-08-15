//! S1 - Feature detection and curve chaining (PLAN §10.2, SPEC_meshgen_numerics §2).
//!
//! Detects sharp edges (dihedral deviation > `feature_angle_deg`),
//! rim and non-manifold edges (source-component-local incident-face count), chains them into
//! polylines with corners at endpoints, junctions (valence >= 3), and
//! high-turn vertices, and accepts optional reference-style explicit point-list
//! overrides. The `s01_features` snapshot is produced from the result.

use crate::io::vtu::VtuDoc;
use crate::meshgen::surface::{
    default_surface_components, surface_stage_to_doc, ConditionedSurface, SurfaceComponent,
    SurfaceCurveCell,
};
use crate::types::Vec3;
use std::collections::{BTreeMap, BTreeSet};

// AI-FUNC-SUMMARY: Classification of a feature edge; side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeatureEdgeKind {
    Sharp,
    Rim,
    NonManifold,
}

// AI-FUNC-SUMMARY: One chained feature polyline with sorted persistent source-component incidence; side effects: none.
#[derive(Debug, Clone)]
pub struct FeatureCurve {
    pub vertices: Vec<usize>,
    pub kind: FeatureEdgeKind,
    pub components: Vec<i32>,
}

// AI-FUNC-SUMMARY:
// Purpose: The S1 feature-set: component-aware chained curves, junction vertices, corner vertices.
// Notes: Rim edges (boundary, 1 incident face) and non-manifold edges (>=3 faces) are
//   always features; sharp edges are dihedral-gated. Chaining breaks at junctions
//   (valence >= 3), corners (high turn angle), and endpoints (valence 1). Coincident
//   geometric curves are merged only after per-component classification and chaining.
#[derive(Debug, Clone, Default)]
pub struct FeatureSet {
    pub curves: Vec<FeatureCurve>,
    pub junctions: Vec<usize>,
    pub corners: Vec<usize>,
}

// AI-FUNC-SUMMARY:
// Purpose: Detect feature edges per persistent source component and chain them into polylines.
// Inputs: the conditioned surface, feature_angle_deg (sharp-edge threshold).
// Returns: FeatureSet with curves, junctions, corners.
// Side effects: None (pure computation).
// Notes: Uses cos² comparison on squared dot/norm products (never `acos`) per
//   SPEC_meshgen_numerics §2 (class A). Rim/non-manifold incidence never crosses
//   source components; coincident curves merge with sorted component incidence.
pub fn detect_features(cs: &ConditionedSurface, feature_angle_deg: f64) -> FeatureSet {
    let verts = &cs.vertices;
    let faces = &cs.faces;

    // 1. Build source-component + undirected edge -> incident face list.
    let mut edge_faces: BTreeMap<(i32, usize, usize), Vec<usize>> = BTreeMap::new();
    for (i, face) in faces.iter().enumerate() {
        let component = cs.source_component[i];
        for k in 0..3 {
            let a = face[k];
            let b = face[(k + 1) % 3];
            let key = if a < b {
                (component, a, b)
            } else {
                (component, b, a)
            };
            edge_faces.entry(key).or_default().push(i);
        }
    }

    // 2. Classify edges.
    let cos2_threshold = feature_angle_deg.to_radians().cos().powi(2);
    let mut feature_edges: Vec<(i32, (usize, usize), FeatureEdgeKind)> = Vec::new();

    for (&(component, a, b), incident) in &edge_faces {
        let kind = if incident.len() == 1 {
            FeatureEdgeKind::Rim
        } else if incident.len() >= 3 {
            FeatureEdgeKind::NonManifold
        } else if incident.len() == 2 {
            // Sharp edge: dihedral deviation > feature_angle_deg via cos² comparison
            // (SPEC_meshgen_numerics §2: class A, never acos). When the dot product
            // of normals is negative (deviation > 90°), the edge is always sharp.
            let f0 = &faces[incident[0]];
            let f1 = &faces[incident[1]];
            let n0 = face_normal(verts[f0[0]], verts[f0[1]], verts[f0[2]]);
            let n1 = face_normal(verts[f1[0]], verts[f1[1]], verts[f1[2]]);
            let dot = n0.dot(n1);
            let cos2 = dot * dot / (n0.dot(n0) * n1.dot(n1));
            if dot < 0.0 || cos2 < cos2_threshold {
                FeatureEdgeKind::Sharp
            } else {
                continue;
            }
        } else {
            continue;
        };
        feature_edges.push((component, (a, b), kind));
    }

    // 3. Compute vertex valence on the feature-edge subgraph.
    let mut valence: BTreeMap<(i32, usize), Vec<(usize, FeatureEdgeKind)>> = BTreeMap::new();
    for (component, (a, b), kind) in &feature_edges {
        valence
            .entry((*component, *a))
            .or_default()
            .push((*b, *kind));
        valence
            .entry((*component, *b))
            .or_default()
            .push((*a, *kind));
    }

    // 4. Identify junctions (valence >= 3) and endpoints (valence 1).
    let mut junction_keys = BTreeSet::new();
    let mut endpoints = BTreeSet::new();
    for (&key, neighbours) in &valence {
        if neighbours.len() >= 3 {
            junction_keys.insert(key);
        } else if neighbours.len() == 1 {
            endpoints.insert(key);
        }
    }

    // 5. Chain edges into polylines.
    let mut used_edges: BTreeSet<(i32, usize, usize)> = BTreeSet::new();
    let mut curves: Vec<FeatureCurve> = Vec::new();

    let make_key = |component: i32, a: usize, b: usize| {
        if a < b {
            (component, a, b)
        } else {
            (component, b, a)
        }
    };

    // Start from endpoints first (open chains), then junctions, then any remaining.
    let mut start_candidates: Vec<(i32, usize)> = Vec::new();
    start_candidates.extend(endpoints.iter().copied());
    start_candidates.extend(junction_keys.iter().copied());
    for (component, (a, b), _) in &feature_edges {
        if !start_candidates.contains(&(*component, *a)) {
            start_candidates.push((*component, *a));
        }
        if !start_candidates.contains(&(*component, *b)) {
            start_candidates.push((*component, *b));
        }
    }

    for &(component, start) in &start_candidates {
        let neighbours = match valence.get(&(component, start)) {
            Some(n) => n,
            None => continue,
        };
        for (next, kind) in neighbours {
            let key = make_key(component, start, *next);
            if used_edges.contains(&key) {
                continue;
            }
            // Walk the chain.
            let mut chain: Vec<usize> = vec![start];
            let mut current = start;
            let mut prev_v: usize;
            let mut next_v = *next;
            let mut chain_kind = *kind;

            loop {
                let key = make_key(component, current, next_v);
                if used_edges.contains(&key) {
                    break;
                }
                used_edges.insert(key);
                chain.push(next_v);
                prev_v = current;
                current = next_v;

                // Check if we should stop: junction, endpoint, or loop closed.
                if junction_keys.contains(&(component, current))
                    || endpoints.contains(&(component, current))
                {
                    break;
                }
                if current == start {
                    break;
                }

                // Find next unused neighbour.
                let nbrs = match valence.get(&(component, current)) {
                    Some(n) => n,
                    None => break,
                };
                let mut found = false;
                for (n, k) in nbrs {
                    let key = make_key(component, current, *n);
                    if !used_edges.contains(&key) && *n != prev_v {
                        next_v = *n;
                        if *k != chain_kind {
                            chain_kind = *k;
                        }
                        found = true;
                        break;
                    }
                }
                if !found {
                    break;
                }
            }

            if chain.len() >= 2 {
                curves.push(FeatureCurve {
                    vertices: chain,
                    kind: chain_kind,
                    components: vec![component],
                });
            }
        }
    }

    // Coincident component curves have identical canonical walks; merge their
    // geometry while retaining every persistent source-component identity.
    curves.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.vertices.cmp(&right.vertices))
            .then_with(|| left.components.cmp(&right.components))
    });
    let mut merged_curves: Vec<FeatureCurve> = Vec::with_capacity(curves.len());
    for curve in curves {
        if let Some(previous) = merged_curves.last_mut() {
            if previous.kind == curve.kind && previous.vertices == curve.vertices {
                previous.components.extend(curve.components);
                previous.components.sort_unstable();
                previous.components.dedup();
                continue;
            }
        }
        merged_curves.push(curve);
    }

    // 6. Detect corners: high-turn vertices on chains (turn angle > 60° via cos²).
    let corner_threshold = 60.0f64.to_radians().cos().powi(2);
    let mut corner_nodes = BTreeSet::new();
    for curve in &merged_curves {
        if curve.vertices.len() < 3 {
            continue;
        }
        for i in 1..curve.vertices.len() - 1 {
            let prev = curve.vertices[i - 1];
            let curr = curve.vertices[i];
            let next = curve.vertices[i + 1];
            let d1 = verts[curr].sub(verts[prev]);
            let d2 = verts[next].sub(verts[curr]);
            let dot = d1.dot(d2);
            let cos2 = dot * dot / (d1.dot(d1) * d2.dot(d2));
            if cos2 < corner_threshold {
                corner_nodes.insert(curr);
            }
        }
    }
    let junctions = junction_keys
        .into_iter()
        .map(|(_, node)| node)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let corners = corner_nodes.into_iter().collect();

    FeatureSet {
        curves: merged_curves,
        junctions,
        corners,
    }
}

// AI-FUNC-SUMMARY: Component-scaled face-normal direction without a literal magnitude tolerance; returns Vec3 (zero only when the f64 cross product is zero); side effects: none.
fn face_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let n = b.sub(a).cross(c.sub(a));
    let scale = n.x.abs().max(n.y.abs()).max(n.z.abs());
    if scale > 0.0 && scale.is_finite() {
        n.scale(1.0 / scale)
    } else {
        n
    }
}

// AI-FUNC-SUMMARY: Build a schema-v1 s01 surface document with inferred source-component metadata; returns VtuDoc; side effects: none.
pub fn features_to_doc(cs: &ConditionedSurface, fs: &FeatureSet) -> VtuDoc {
    let components = default_surface_components(cs);
    features_to_doc_with_components(cs, fs, &components)
}

// AI-FUNC-SUMMARY: Build a schema-v1 s01 surface document using caller-supplied component metadata and each feature curve's exact component incidence; returns VtuDoc; side effects: none.
pub(crate) fn features_to_doc_with_components(
    cs: &ConditionedSurface,
    fs: &FeatureSet,
    components: &[SurfaceComponent],
) -> VtuDoc {
    let curves: Vec<SurfaceCurveCell> = fs
        .curves
        .iter()
        .map(|curve| SurfaceCurveCell {
            nodes: curve.vertices.clone(),
            kind: match curve.kind {
                FeatureEdgeKind::Sharp | FeatureEdgeKind::NonManifold => 0,
                FeatureEdgeKind::Rim => 1,
            },
            components: curve.components.clone(),
        })
        .collect();
    let corner_nodes = fs.corners.iter().chain(&fs.junctions).copied().collect();
    surface_stage_to_doc(cs, &curves, components, &corner_nodes)
}
