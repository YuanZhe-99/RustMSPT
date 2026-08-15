//! S0 - Conditioning and repair (PLAN §10.1, SPEC_meshgen_numerics §2).
//!
//! Welds input surface vertices on a scale-relative quantized grid
//! (`q = 0.1·ε`), then applies leveled repair (degenerate-triangle drop,
//! duplicate-face merge, pinhole closure, source-component-isolated minimum-flip
//! orientation repair), classifies provisional components, and records every action in a structured log.
//! The `s00_conditioned` snapshot is produced from the result.

use crate::config::meshgen::RepairLevel;
use crate::error::{Result, RustMsptError};
use crate::io::vtu::{ArrayData, DataArray, VtuDoc, VTK_POLY_LINE, VTK_TRIANGLE};
use crate::meshgen::predicates::{node_key, orient2d_3d};
use crate::types::{Mesh, Vec3};
use std::collections::{BTreeMap, BTreeSet};

// AI-FUNC-SUMMARY: Kind of repair action recorded in the log; side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairActionType {
    VertexWelded,
    DegenerateTriangleDropped,
    DuplicateFaceMerged,
    PinholeClosed,
    OrientationFixed,
    HoleFilled,
}

impl RepairActionType {
    // AI-FUNC-SUMMARY: Stable string used in the repair-log echo; returns &'static str; side effects: none.
    pub fn as_str(self) -> &'static str {
        match self {
            RepairActionType::VertexWelded => "vertex_welded",
            RepairActionType::DegenerateTriangleDropped => "degenerate_triangle_dropped",
            RepairActionType::DuplicateFaceMerged => "duplicate_face_merged",
            RepairActionType::PinholeClosed => "pinhole_closed",
            RepairActionType::OrientationFixed => "orientation_fixed",
            RepairActionType::HoleFilled => "hole_filled",
        }
    }
}

// AI-FUNC-SUMMARY: One structured repair-log entry (SPEC §10.1 / [V12] repair-log echo); side effects: none.
#[derive(Debug, Clone)]
pub struct RepairAction {
    pub component: i64,
    pub action_type: RepairActionType,
    pub size: f64,
    pub area: f64,
    pub tolerance: f64,
    pub delta_euler: i64,
    pub delta_boundary_loops: i64,
}

// AI-FUNC-SUMMARY: The full repair log carried through the pipeline for [V12] echo; side effects: none.
#[derive(Debug, Clone, Default)]
pub struct RepairLog {
    pub actions: Vec<RepairAction>,
}

impl RepairLog {
    // AI-FUNC-SUMMARY: Number of actions of a given type; returns usize; side effects: none.
    pub fn count(&self, t: RepairActionType) -> usize {
        self.actions.iter().filter(|a| a.action_type == t).count()
    }

    // AI-FUNC-SUMMARY: Human-readable one-line-per-action echo for the console and [V12]; returns String; side effects: none.
    pub fn echo(&self) -> String {
        let mut out = String::new();
        for a in &self.actions {
            out.push_str(&format!(
                "  [repair] component={} type={} size={:.2e} area={:.2e} tol={:.2e} dEuler={} dLoops={}\n",
                a.component,
                a.action_type.as_str(),
                a.size,
                a.area,
                a.tolerance,
                a.delta_euler,
                a.delta_boundary_loops,
            ));
        }
        out
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The S0 conditioned-surface result: welded vertices, triangle faces with provisional and persistent source-component ids, and the repair log.
// Notes: Provisional components are re-derived at S2b; S0's classification only routes conditioning decisions (§10.1).
#[derive(Debug, Clone)]
pub struct ConditionedSurface {
    pub vertices: Vec<Vec3>,
    pub faces: Vec<[usize; 3]>,
    pub component: Vec<i64>,
    pub source_component: Vec<i32>,
    pub repair_log: RepairLog,
    pub stats: ConditionStats,
}

// AI-FUNC-SUMMARY: One persistent source-component row used by all surface-stage contract snapshots; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceComponent {
    pub x: i32,
    pub priority: u32,
    pub kind: u8,
    pub closed: bool,
}

// AI-FUNC-SUMMARY: One curve cell consumed by the shared schema-v1 surface-stage VTU builder; side effects: none.
#[derive(Clone, Debug)]
pub(crate) struct SurfaceCurveCell {
    pub nodes: Vec<usize>,
    pub kind: u8,
    pub components: Vec<i32>,
}

// AI-FUNC-SUMMARY: Aggregate counts for quick reporting and [V12] provenance; side effects: none.
#[derive(Debug, Clone, Default)]
pub struct ConditionStats {
    pub n_input_vertices: usize,
    pub n_input_faces: usize,
    pub n_welded: usize,
    pub n_degenerate: usize,
    pub n_duplicate: usize,
    pub n_pinhole: usize,
    pub n_orientation_fixed: usize,
    pub n_components: usize,
}

// AI-FUNC-SUMMARY:
// Purpose: Condition a set of input surface meshes into one welded, repaired surface with provisional components.
// Inputs: input meshes (one per pipeline input), eps (envelope tolerance in model units), repair level.
// Returns: ConditionedSurface or InvalidMesh when the repair level rejects a defect or orientation parity is contradictory.
// Side effects: None (pure computation).
// Notes: Follows the frozen S0 predicate table (SPEC_meshgen_numerics §2):
//   weld = NodeKey equality (I), degenerate = orient2d=0 (X), duplicate = sorted key triple (I),
//   orientation = source-component-isolated edge-parity walk (I), pinhole = boundary-loop diameter < eps (T).
pub fn condition_surface(
    meshes: &[Mesh],
    eps: f64,
    repair_level: RepairLevel,
) -> Result<ConditionedSurface> {
    let q = 0.1 * eps;
    let mut log = RepairLog::default();

    // 1. Collect all vertices with component labels.
    let mut raw_verts: Vec<(Vec3, i64)> = Vec::new();
    let mut raw_faces: Vec<([usize; 3], i64)> = Vec::new();
    for (comp, mesh) in meshes.iter().enumerate() {
        let comp_id = (comp + 1) as i64;
        let base = raw_verts.len();
        for v in &mesh.vertices {
            raw_verts.push((*v, comp_id));
        }
        for f in &mesh.faces {
            if f.a < mesh.vertices.len() && f.b < mesh.vertices.len() && f.c < mesh.vertices.len() {
                raw_faces.push(([f.a + base, f.b + base, f.c + base], comp_id));
            }
        }
    }

    let n_input_vertices = raw_verts.len();
    let n_input_faces = raw_faces.len();

    // 2. Weld: deduplicate vertices by NodeKey.
    let mut key_to_idx: BTreeMap<(i64, i64, i64), usize> = BTreeMap::new();
    let mut welded_verts: Vec<Vec3> = Vec::new();
    let mut vert_remap: Vec<usize> = Vec::with_capacity(raw_verts.len());
    let mut n_welded = 0usize;

    for (v, _comp) in &raw_verts {
        let key = node_key(*v, q);
        if let Some(&idx) = key_to_idx.get(&key) {
            vert_remap.push(idx);
            n_welded += 1;
        } else {
            let idx = welded_verts.len();
            key_to_idx.insert(key, idx);
            welded_verts.push(*v);
            vert_remap.push(idx);
        }
    }

    // 3. Remap faces.
    let mut faces: Vec<([usize; 3], i64)> = raw_faces
        .iter()
        .map(|(f, comp)| {
            (
                [vert_remap[f[0]], vert_remap[f[1]], vert_remap[f[2]]],
                *comp,
            )
        })
        .collect();

    // 4. Detect and drop degenerate triangles (orient2d = 0).
    let mut n_degenerate = 0usize;
    let mut kept_faces: Vec<([usize; 3], i64)> = Vec::with_capacity(faces.len());
    for (face, comp) in &faces {
        let a = welded_verts[face[0]];
        let b = welded_verts[face[1]];
        let c = welded_verts[face[2]];
        let is_degenerate = orient2d_3d(a, b, c) == 0.0;
        if is_degenerate {
            n_degenerate += 1;
            if matches!(repair_level, RepairLevel::Strict) {
                return Err(RustMsptError::InvalidMesh(format!(
                    "S0 strict: degenerate triangle at vertices [{}, {}, {}] (component {})",
                    face[0], face[1], face[2], comp
                )));
            }
            let edge_len = len(b.sub(a)).max(len(c.sub(b))).max(len(a.sub(c)));
            log.actions.push(RepairAction {
                component: *comp,
                action_type: RepairActionType::DegenerateTriangleDropped,
                size: edge_len,
                area: 0.0,
                tolerance: q,
                delta_euler: -1,
                delta_boundary_loops: 0,
            });
        } else {
            kept_faces.push((*face, *comp));
        }
    }
    faces = kept_faces;

    // 5. Detect and merge duplicate faces (sorted NodeKey triple equality).
    let mut seen_faces: BTreeMap<([i64; 3], i64), usize> = BTreeMap::new();
    let mut n_duplicate = 0usize;
    let mut dedup_faces: Vec<([usize; 3], i64)> = Vec::with_capacity(faces.len());
    for (face, comp) in &faces {
        // Use the node index-based key for dedup (welded indices are already unique per key)
        let mut idx_key = [face[0] as i64, face[1] as i64, face[2] as i64];
        idx_key.sort_unstable();
        let semantic_key = (idx_key, *comp);
        if let Some(&_existing) = seen_faces.get(&semantic_key) {
            n_duplicate += 1;
            if matches!(repair_level, RepairLevel::Strict) {
                return Err(RustMsptError::InvalidMesh(format!(
                    "S0 strict: duplicate face [{}, {}, {}] (component {})",
                    idx_key[0], idx_key[1], idx_key[2], comp
                )));
            }
            let area = triangle_area(
                welded_verts[face[0]],
                welded_verts[face[1]],
                welded_verts[face[2]],
            );
            log.actions.push(RepairAction {
                component: *comp,
                action_type: RepairActionType::DuplicateFaceMerged,
                size: 0.0,
                area,
                tolerance: q,
                delta_euler: -1,
                delta_boundary_loops: 0,
            });
        } else {
            seen_faces.insert(semantic_key, dedup_faces.len());
            dedup_faces.push((*face, *comp));
        }
    }
    faces = dedup_faces;

    // 6. Orientation fix: edge-parity walk with minimum-flip partitioning (permissive).
    let (oriented_faces, n_orientation_fixed) =
        fix_orientation(&faces, &welded_verts, q, repair_level, &mut log)?;

    // 7. Pinhole closure: find boundary loops with diameter < eps.
    let (final_faces_with_comp, n_pinhole) =
        close_pinholes(&oriented_faces, &welded_verts, eps, repair_level, &mut log)?;

    // 8. Provisional components: connected components via face adjacency.
    let (component_labels, n_components) = compute_components(&final_faces_with_comp);

    let final_faces: Vec<[usize; 3]> = final_faces_with_comp.iter().map(|(f, _)| *f).collect();
    let source_component: Vec<i32> = final_faces_with_comp
        .iter()
        .map(|(_, component)| *component as i32)
        .collect();

    let stats = ConditionStats {
        n_input_vertices,
        n_input_faces,
        n_welded,
        n_degenerate,
        n_duplicate,
        n_pinhole,
        n_orientation_fixed,
        n_components,
    };

    Ok(ConditionedSurface {
        vertices: welded_verts,
        faces: final_faces,
        component: component_labels,
        source_component,
        repair_log: log,
        stats,
    })
}

// AI-FUNC-SUMMARY: Build a schema-v1 s00 surface document with inferred source-component metadata; returns VtuDoc; side effects: none.
pub fn condition_surface_to_doc(cs: &ConditionedSurface) -> VtuDoc {
    let components = default_surface_components(cs);
    condition_surface_to_doc_with_components(cs, &components)
}

// AI-FUNC-SUMMARY: Build a schema-v1 s00 surface document using caller-supplied component priorities/kinds; returns VtuDoc; side effects: none.
pub(crate) fn condition_surface_to_doc_with_components(
    cs: &ConditionedSurface,
    components: &[SurfaceComponent],
) -> VtuDoc {
    surface_stage_to_doc(cs, &[], components, &BTreeSet::new())
}

// AI-FUNC-SUMMARY: Infer deterministic source-component metadata for standalone S0/S1 document builders; returns rows sorted by X; side effects: none.
pub(crate) fn default_surface_components(cs: &ConditionedSurface) -> Vec<SurfaceComponent> {
    let source_ids: BTreeSet<i32> = cs.source_component.iter().copied().collect();
    source_ids
        .into_iter()
        .map(|x| {
            let closed = source_component_is_closed(cs, x);
            SurfaceComponent {
                x,
                priority: x.saturating_sub(1).max(0) as u32,
                kind: u8::from(!closed),
                closed,
            }
        })
        .collect()
}

// AI-FUNC-SUMMARY: Determine source-component closure from exact undirected edge incidence; returns true only when every edge has incidence two; side effects: none.
pub fn source_component_is_closed(cs: &ConditionedSurface, component: i32) -> bool {
    let mut counts: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for (index, face) in cs.faces.iter().enumerate() {
        if cs.source_component.get(index).copied() != Some(component) {
            continue;
        }
        for edge in [[face[0], face[1]], [face[1], face[2]], [face[2], face[0]]] {
            let key = if edge[0] <= edge[1] {
                (edge[0], edge[1])
            } else {
                (edge[1], edge[0])
            };
            *counts.entry(key).or_default() += 1;
        }
    }
    !counts.is_empty() && counts.values().all(|count| *count == 2)
}

// AI-FUNC-SUMMARY: Build all always-present schema-v1 arrays/tables for an S0-S3 face/curve document; returns VtuDoc; side effects: none.
pub(crate) fn surface_stage_to_doc(
    cs: &ConditionedSurface,
    curves: &[SurfaceCurveCell],
    components: &[SurfaceComponent],
    corner_nodes: &BTreeSet<usize>,
) -> VtuDoc {
    let mut doc = VtuDoc {
        points: cs.vertices.clone(),
        ..Default::default()
    };
    let face_count = cs.faces.len();
    let cell_count = face_count + curves.len();
    let mut cell_kind = Vec::with_capacity(cell_count);
    let mut face_tag_key = Vec::with_capacity(cell_count);
    let mut curve_id = Vec::with_capacity(cell_count);
    let face_components: BTreeSet<i32> = cs.source_component.iter().copied().collect();
    let face_tag_by_component: BTreeMap<i32, i32> = face_components
        .iter()
        .enumerate()
        .map(|(index, component)| (*component, index as i32))
        .collect();
    for (index, face) in cs.faces.iter().enumerate() {
        doc.connectivity
            .extend(face.iter().map(|node| *node as i64));
        doc.offsets.push(doc.connectivity.len() as i64);
        doc.types.push(VTK_TRIANGLE);
        cell_kind.push(1u8);
        face_tag_key.push(face_tag_by_component[&cs.source_component[index]]);
        curve_id.push(-1);
    }
    for (index, curve) in curves.iter().enumerate() {
        doc.connectivity
            .extend(curve.nodes.iter().map(|node| *node as i64));
        doc.offsets.push(doc.connectivity.len() as i64);
        doc.types.push(VTK_POLY_LINE);
        cell_kind.push(2u8);
        face_tag_key.push(-1);
        curve_id.push(index as i32);
    }
    doc.cell_data
        .push(DataArray::scalar("cell_kind", ArrayData::U8(cell_kind)));
    doc.cell_data.push(DataArray::scalar(
        "region_key",
        ArrayData::I32(vec![-1; cell_count]),
    ));
    doc.cell_data.push(DataArray::scalar(
        "partition_id",
        ArrayData::I32(vec![-1; cell_count]),
    ));
    doc.cell_data.push(DataArray::scalar(
        "regime",
        ArrayData::U8(vec![255; cell_count]),
    ));
    doc.cell_data.push(DataArray::scalar(
        "face_tag_key",
        ArrayData::I32(face_tag_key),
    ));
    doc.cell_data
        .push(DataArray::scalar("curve_id", ArrayData::I32(curve_id)));

    let mut incident_components = vec![BTreeSet::new(); cs.vertices.len()];
    for (face_index, face) in cs.faces.iter().enumerate() {
        for node in face {
            incident_components[*node].insert(cs.source_component[face_index]);
        }
    }
    for curve in curves {
        for node in &curve.nodes {
            incident_components[*node].extend(curve.components.iter().copied());
        }
    }
    let node_sets: Vec<Vec<i32>> = incident_components
        .iter()
        .map(|set| set.iter().copied().collect())
        .collect();
    let unique_node_sets: BTreeSet<Vec<i32>> = node_sets.iter().cloned().collect();
    let node_set_keys: BTreeMap<Vec<i32>, i32> = unique_node_sets
        .iter()
        .enumerate()
        .map(|(index, set)| (set.clone(), index as i32))
        .collect();
    let mut node_curve = vec![None; cs.vertices.len()];
    for (curve_index, curve) in curves.iter().enumerate() {
        for node in &curve.nodes {
            node_curve[*node].get_or_insert(curve_index as i32);
        }
    }
    let mut constraint_kind = Vec::with_capacity(cs.vertices.len());
    let mut constraint_ref = Vec::with_capacity(cs.vertices.len());
    for node in 0..cs.vertices.len() {
        if corner_nodes.contains(&node) {
            constraint_kind.push(3u8);
            constraint_ref.push(node_curve[node].unwrap_or(-1));
        } else if let Some(curve) = node_curve[node] {
            constraint_kind.push(2u8);
            constraint_ref.push(curve);
        } else {
            constraint_kind.push(1u8);
            constraint_ref.push(node_sets[node].first().copied().unwrap_or(-1));
        }
    }
    doc.point_data.push(DataArray::scalar(
        "n_id_key",
        ArrayData::I32(node_sets.iter().map(|set| node_set_keys[set]).collect()),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_kind",
        ArrayData::U8(constraint_kind),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_ref",
        ArrayData::I32(constraint_ref),
    ));

    push_surface_field(&mut doc, "RegionSetOffsets", 1, ArrayData::I64(vec![1]));
    push_surface_field(&mut doc, "RegionSetComponents", 1, ArrayData::I32(vec![0]));
    push_surface_field(
        &mut doc,
        "RegionSetPriority",
        1,
        ArrayData::U32(vec![u32::MAX]),
    );
    let mut n_id_offsets = Vec::with_capacity(unique_node_sets.len());
    let mut n_id_components = Vec::new();
    for set in &unique_node_sets {
        n_id_components.extend(set.iter().copied());
        n_id_offsets.push(n_id_components.len() as i64);
    }
    push_surface_field(&mut doc, "NIdSetOffsets", 1, ArrayData::I64(n_id_offsets));
    push_surface_field(
        &mut doc,
        "NIdSetComponents",
        1,
        ArrayData::I32(n_id_components),
    );
    let components_by_x: BTreeMap<i32, SurfaceComponent> = components
        .iter()
        .map(|component| (component.x, *component))
        .collect();
    let mut face_offsets = Vec::with_capacity(face_components.len());
    let mut face_components_flat = Vec::with_capacity(face_components.len());
    let mut face_kinds = Vec::with_capacity(face_components.len());
    for component in &face_components {
        face_components_flat.push(*component);
        face_offsets.push(face_components_flat.len() as i64);
        face_kinds.push(
            components_by_x
                .get(component)
                .map(|metadata| u8::from(metadata.kind == 1))
                .unwrap_or(0),
        );
    }
    push_surface_field(&mut doc, "FaceTagOffsets", 1, ArrayData::I64(face_offsets));
    push_surface_field(
        &mut doc,
        "FaceTagComponents",
        1,
        ArrayData::I32(face_components_flat),
    );
    push_surface_field(&mut doc, "FaceTagKind", 1, ArrayData::U8(face_kinds));
    push_surface_field(
        &mut doc,
        "FaceTagSideElems",
        2,
        ArrayData::I32(vec![-1; face_count * 2]),
    );
    let mut sorted_components = components.to_vec();
    sorted_components.sort_by_key(|component| component.x);
    push_surface_field(
        &mut doc,
        "ComponentX",
        1,
        ArrayData::I32(
            sorted_components
                .iter()
                .map(|component| component.x)
                .collect(),
        ),
    );
    push_surface_field(
        &mut doc,
        "ComponentY",
        1,
        ArrayData::U32(
            sorted_components
                .iter()
                .map(|component| component.priority)
                .collect(),
        ),
    );
    push_surface_field(
        &mut doc,
        "ComponentKind",
        1,
        ArrayData::U8(
            sorted_components
                .iter()
                .map(|component| component.kind)
                .collect(),
        ),
    );
    push_surface_field(
        &mut doc,
        "ComponentClosed",
        1,
        ArrayData::U8(
            sorted_components
                .iter()
                .map(|component| u8::from(component.closed))
                .collect(),
        ),
    );
    let mut curve_offsets = Vec::with_capacity(curves.len());
    let mut curve_components = Vec::new();
    for curve in curves {
        curve_components.extend(curve.components.iter().copied());
        curve_offsets.push(curve_components.len() as i64);
    }
    push_surface_field(
        &mut doc,
        "CurveKind",
        1,
        ArrayData::U8(curves.iter().map(|curve| curve.kind).collect()),
    );
    push_surface_field(
        &mut doc,
        "CurveCompOffsets",
        1,
        ArrayData::I64(curve_offsets),
    );
    push_surface_field(
        &mut doc,
        "CurveCompComponents",
        1,
        ArrayData::I32(curve_components),
    );
    doc
}

// AI-FUNC-SUMMARY: Append one field-data array to a surface-stage document; side effects: mutates VtuDoc field_data.
fn push_surface_field(doc: &mut VtuDoc, name: &str, components: usize, data: ArrayData) {
    doc.field_data.push(DataArray {
        name: name.to_string(),
        components,
        data,
    });
}

// AI-FUNC-SUMMARY: Signed area of a triangle via cross-product magnitude; returns f64; side effects: none.
fn triangle_area(a: Vec3, b: Vec3, c: Vec3) -> f64 {
    b.sub(a)
        .cross(c.sub(a))
        .dot(b.sub(a).cross(c.sub(a)))
        .sqrt()
        * 0.5
}

// AI-FUNC-SUMMARY: Euclidean length of a vector; returns f64; side effects: none.
fn len(v: Vec3) -> f64 {
    v.dot(v).sqrt()
}

// AI-FUNC-SUMMARY:
// Purpose: Solve source-component-local orientation parity and apply the canonical minimum-flip partition in permissive mode.
// Inputs: faces with component labels, welded vertices, weld quantum, repair level, log sink.
// Returns: (oriented faces, count of flipped faces), or InvalidMesh for contradictory parity or a forbidden repair.
// Side effects: Pushes OrientationFixed actions into the log only for permissive repairs.
// Notes: Equal-size parity partitions keep their smallest canonical NodeKey face unflipped, making the result independent of input face order.
fn fix_orientation(
    faces: &[([usize; 3], i64)],
    verts: &[Vec3],
    q: f64,
    repair_level: RepairLevel,
    log: &mut RepairLog,
) -> Result<(Vec<([usize; 3], i64)>, usize)> {
    let n = faces.len();
    if n == 0 {
        return Ok((faces.to_vec(), 0));
    }

    let vertex_keys: Vec<(i64, i64, i64)> =
        verts.iter().map(|vertex| node_key(*vertex, q)).collect();
    let face_keys: Vec<[(i64, i64, i64); 3]> = faces
        .iter()
        .map(|(face, _)| {
            let mut key = [
                vertex_keys[face[0]],
                vertex_keys[face[1]],
                vertex_keys[face[2]],
            ];
            key.sort_unstable();
            key
        })
        .collect();

    let mut edge_faces: BTreeMap<(i64, usize, usize), Vec<(usize, bool)>> = BTreeMap::new();
    for (i, (face, component)) in faces.iter().enumerate() {
        for k in 0..3 {
            let a = face[k];
            let b = face[(k + 1) % 3];
            let (lo, hi, forward) = if a < b { (a, b, true) } else { (b, a, false) };
            edge_faces
                .entry((*component, lo, hi))
                .or_default()
                .push((i, forward));
        }
    }
    for incident in edge_faces.values_mut() {
        incident.sort_unstable_by_key(|(index, forward)| (face_keys[*index], *forward));
    }

    let mut face_order: Vec<usize> = (0..n).collect();
    face_order.sort_unstable_by_key(|index| (faces[*index].1, face_keys[*index]));
    let mut flip_state = vec![None; n];
    let mut patches = Vec::new();
    for start in face_order {
        if flip_state[start].is_some() {
            continue;
        }
        flip_state[start] = Some(false);
        let mut queue = vec![start];
        let mut patch = Vec::new();
        while let Some(fidx) = queue.pop() {
            patch.push(fidx);
            let (face, component) = faces[fidx];
            let current_flip = flip_state[fidx].expect("queued face must have orientation parity");
            for k in 0..3 {
                let a = face[k];
                let b = face[(k + 1) % 3];
                let (lo, hi, forward) = if a < b { (a, b, true) } else { (b, a, false) };
                let Some(incident) = edge_faces.get(&(component, lo, hi)) else {
                    continue;
                };
                if incident.len() != 2 {
                    continue;
                }
                for &(neighbour, neighbour_forward) in incident {
                    if neighbour == fidx {
                        continue;
                    }
                    let required_flip = current_flip ^ (forward == neighbour_forward);
                    match flip_state[neighbour] {
                        None => {
                            flip_state[neighbour] = Some(required_flip);
                            queue.push(neighbour);
                        }
                        Some(actual_flip) if actual_flip != required_flip => {
                            let mut edge_keys = [vertex_keys[lo], vertex_keys[hi]];
                            edge_keys.sort_unstable();
                            let mut conflict_faces = [face_keys[fidx], face_keys[neighbour]];
                            conflict_faces.sort_unstable();
                            return Err(RustMsptError::InvalidMesh(format!(
                                "S0 orientation: contradictory parity assignments in source component {component} on welded edge {:?} -> {:?} between faces {:?} and {:?}; the patch is non-orientable or has incompatible non-manifold incidence",
                                edge_keys[0], edge_keys[1], conflict_faces[0], conflict_faces[1]
                            )));
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        patches.push(patch);
    }

    for patch in &patches {
        let assigned_flips = patch
            .iter()
            .filter(|index| flip_state[**index] == Some(true))
            .count();
        let assigned_kept = patch.len() - assigned_flips;
        let anchor = *patch
            .iter()
            .min_by_key(|index| face_keys[**index])
            .expect("orientation patch cannot be empty");
        let invert = assigned_flips > assigned_kept
            || (assigned_flips == assigned_kept && flip_state[anchor] == Some(true));
        if invert {
            for index in patch {
                flip_state[*index] =
                    Some(!flip_state[*index].expect("patch face must have parity"));
            }
        }
    }

    if !matches!(repair_level, RepairLevel::Permissive) {
        for patch in &patches {
            let required_flips = patch
                .iter()
                .filter(|index| flip_state[**index] == Some(true))
                .count();
            if required_flips == 0 {
                continue;
            }
            let anchor = *patch
                .iter()
                .min_by_key(|index| face_keys[**index])
                .expect("orientation patch cannot be empty");
            let component = faces[anchor].1;
            return Err(RustMsptError::InvalidMesh(format!(
                "S0 {repair_level:?}: source component {component} has inconsistent face winding requiring {required_flips} orientation flip(s) in a {}-face patch; only repair.level=permissive may apply the canonical minimum-flip repair",
                patch.len()
            )));
        }
    }

    let mut result: Vec<([usize; 3], i64)> = faces.to_vec();
    let mut flipped = 0usize;
    for (index, should_flip) in flip_state.into_iter().enumerate() {
        if should_flip != Some(true) {
            continue;
        }
        let (mut face, component) = result[index];
        face.swap(0, 1);
        result[index] = (face, component);
        flipped += 1;
        log.actions.push(RepairAction {
            component,
            action_type: RepairActionType::OrientationFixed,
            size: 0.0,
            area: triangle_area(verts[face[0]], verts[face[1]], verts[face[2]]),
            tolerance: 0.0,
            delta_euler: 0,
            delta_boundary_loops: 0,
        });
    }

    Ok((result, flipped))
}

// AI-FUNC-SUMMARY:
// Purpose: Find and close pinhole boundary loops with diameter < eps.
// Inputs: faces, welded vertices, eps, repair level, log sink.
// Returns: (faces, count of pinholes closed), or InvalidMesh when strict mode detects a pinhole.
// Side effects: In conservative/permissive modes, pushes PinholeClosed actions and adds cap faces.
fn close_pinholes(
    faces: &[([usize; 3], i64)],
    verts: &[Vec3],
    eps: f64,
    repair_level: RepairLevel,
    log: &mut RepairLog,
) -> Result<(Vec<([usize; 3], i64)>, usize)> {
    let mut result: Vec<([usize; 3], i64)> = faces.to_vec();

    // Build boundary edges (edges appearing exactly once).
    let mut edge_count: BTreeMap<(i64, usize, usize), usize> = BTreeMap::new();
    for (face, component) in faces {
        for k in 0..3 {
            let a = face[k];
            let b = face[(k + 1) % 3];
            let (a, b) = if a < b { (a, b) } else { (b, a) };
            let key = (*component, a, b);
            *edge_count.entry(key).or_insert(0) += 1;
        }
    }

    // Build directed boundary-edge adjacency.
    let mut directed: BTreeMap<(i64, usize), usize> = BTreeMap::new();
    for (face, component) in faces {
        for k in 0..3 {
            let a = face[k];
            let b = face[(k + 1) % 3];
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            if edge_count.get(&(*component, lo, hi)).copied().unwrap_or(0) == 1 {
                directed.entry((*component, a)).or_insert(b);
            }
        }
    }

    // Trace boundary loops.
    let mut visited: BTreeSet<(i64, usize)> = BTreeSet::new();
    let mut n_pinhole = 0usize;

    for &(component, start) in directed.keys() {
        if visited.contains(&(component, start)) {
            continue;
        }
        let mut loop_verts: Vec<usize> = vec![start];
        let mut current = start;
        loop {
            visited.insert((component, current));
            let next = match directed.get(&(component, current)) {
                Some(&n) => n,
                None => break,
            };
            if next == start || visited.contains(&(component, next)) {
                break;
            }
            loop_verts.push(next);
            current = next;
            if loop_verts.len() > 10000 {
                break;
            }
        }

        if loop_verts.len() < 3 {
            continue;
        }

        // Compute loop diameter.
        let mut max_dist = 0.0f64;
        for i in 0..loop_verts.len() {
            for j in (i + 1)..loop_verts.len() {
                let d = len(verts[loop_verts[i]].sub(verts[loop_verts[j]]));
                if d > max_dist {
                    max_dist = d;
                }
            }
        }

        if max_dist < eps {
            if matches!(repair_level, RepairLevel::Strict) {
                return Err(RustMsptError::InvalidMesh(format!(
                    "S0 strict: pinhole boundary loop in source component {component} has {} vertices and diameter {max_dist:.6e} < eps {eps:.6e}; strict mode forbids closure, use repair.level=conservative or permissive",
                    loop_verts.len()
                )));
            }
            n_pinhole += 1;
            // Fan-triangulate the loop to close the pinhole.
            for i in 1..loop_verts.len() - 1 {
                result.push(([loop_verts[0], loop_verts[i], loop_verts[i + 1]], component));
            }
            log.actions.push(RepairAction {
                component,
                action_type: RepairActionType::PinholeClosed,
                size: max_dist,
                area: 0.0,
                tolerance: eps,
                delta_euler: 1,
                delta_boundary_loops: -1,
            });
        }
    }

    Ok((result, n_pinhole))
}

// AI-FUNC-SUMMARY:
// Purpose: Compute provisional connected components via face-vertex adjacency.
// Inputs: faces (with component labels).
// Returns: (per-face component id, number of components).
// Side effects: None.
fn compute_components(faces: &[([usize; 3], i64)]) -> (Vec<i64>, usize) {
    let n = faces.len();
    if n == 0 {
        return (Vec::new(), 0);
    }

    // Build vertex -> face adjacency.
    let mut vert_to_faces: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, (face, _)) in faces.iter().enumerate() {
        for &v in face {
            vert_to_faces.entry(v).or_default().push(i);
        }
    }

    // BFS connected components.
    let mut comp_id = vec![0i64; n];
    let mut visited = vec![false; n];
    let mut n_components = 0usize;

    for start in 0..n {
        if visited[start] {
            continue;
        }
        let cid = n_components as i64;
        n_components += 1;
        let mut queue: Vec<usize> = vec![start];
        visited[start] = true;
        while let Some(fidx) = queue.pop() {
            comp_id[fidx] = cid;
            let face = faces[fidx].0;
            for &v in &face {
                if let Some(neighbours) = vert_to_faces.get(&v) {
                    for &nbr in neighbours {
                        if !visited[nbr] {
                            visited[nbr] = true;
                            queue.push(nbr);
                        }
                    }
                }
            }
        }
    }

    (comp_id, n_components)
}
