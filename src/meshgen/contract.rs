//! Contract validation (`SPEC_meshgen_contracts.md` §4.2, plan M-1.0 / MG-08): what a document
//! must satisfy before any check of the catalog is allowed to read it.
//!
//! The strength is chosen from the document's **metadata**, never from which arrays happen to
//! be present - at `891badc` every semantic check degraded to SKIPPED when its array was absent,
//! so removing an array made a broken producer's output verify clean.

use crate::io::vtu::{ArrayData, DataArray, VtuDoc, VTK_POLY_LINE, VTK_TETRA, VTK_TRIANGLE, VTK_VOXEL};
use crate::meshgen::predicates::orient3d;

/// Which of §4.2's three validation strengths a document was held to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractStrength {
    /// No `SchemaVersion`: an external mesh, verified on its points and tets only.
    GeometryOnly,
    /// `SchemaVersion = 1`, tets only, `region_key` present: the delivered volume.
    PrimaryVolume,
    /// `SchemaVersion = 1` with face or curve cells, or `StageIndex >= 8` without being a
    /// primary volume: every **A** array, every table, every sentinel.
    MixedContract,
}

impl ContractStrength {
    // AI-FUNC-SUMMARY: Report spelling of the strength; returns &'static str; side effects: none.
    pub fn as_str(self) -> &'static str {
        match self {
            ContractStrength::GeometryOnly => "external geometry-only",
            ContractStrength::PrimaryVolume => "primary volume",
            ContractStrength::MixedContract => "mixed contract",
        }
    }

    // AI-FUNC-SUMMARY: Numeric code for the report metric (0 geometry-only, 1 primary volume, 2 mixed contract); returns f64; side effects: none.
    pub fn code(self) -> f64 {
        match self {
            ContractStrength::GeometryOnly => 0.0,
            ContractStrength::PrimaryVolume => 1.0,
            ContractStrength::MixedContract => 2.0,
        }
    }
}

/// One violated §4.2 rule: `rule` is a short stable name, `message` says what was found.
#[derive(Clone, Debug)]
pub struct ContractViolation {
    pub rule: &'static str,
    pub message: String,
}

// AI-FUNC-SUMMARY: The contract spelling of an array's element type; returns &'static str; side effects: none.
fn type_name(data: &ArrayData) -> &'static str {
    match data {
        ArrayData::U8(_) => "UInt8",
        ArrayData::I32(_) => "Int32",
        ArrayData::I64(_) => "Int64",
        ArrayData::U32(_) => "UInt32",
        ArrayData::U64(_) => "UInt64",
        ArrayData::F32(_) => "Float32",
        ArrayData::F64(_) => "Float64",
    }
}

// AI-FUNC-SUMMARY: Read any array as i64 values; returns Vec<i64>; side effects: none.
fn values(a: &DataArray) -> Vec<i64> {
    (0..a.data.len()).map(|i| a.data.get_i64(i)).collect()
}

// AI-FUNC-SUMMARY:
// Purpose: Choose §4.2's validation strength from the metadata alone.
// Inputs: the document.
// Returns: the strength.
// Side effects: None.
// Notes: "Tets only" is read off the cell types, never off an array's absence.
pub fn contract_strength(doc: &VtuDoc) -> ContractStrength {
    let schema = doc.field_array("SchemaVersion").map(values).unwrap_or_default();
    if schema.is_empty() {
        return ContractStrength::GeometryOnly;
    }
    let tets_only = !doc.types.is_empty() && doc.types.iter().all(|&t| t == VTK_TETRA);
    if tets_only && doc.cell_array("region_key").is_some() {
        return ContractStrength::PrimaryVolume;
    }
    ContractStrength::MixedContract
}

struct Checker<'a> {
    doc: &'a VtuDoc,
    out: Vec<ContractViolation>,
}

impl<'a> Checker<'a> {
    // AI-FUNC-SUMMARY: Record one violation; side effects: mutates self.out.
    fn fail(&mut self, rule: &'static str, message: String) {
        self.out.push(ContractViolation { rule, message });
    }

    // AI-FUNC-SUMMARY: Require an array with a given type and component count; returns its values when present and well-typed; side effects: records a violation otherwise.
    fn require(
        &mut self,
        kind: &str,
        array: Option<&DataArray>,
        name: &str,
        ty: &str,
        components: usize,
    ) -> Option<Vec<i64>> {
        let Some(a) = array else {
            self.fail("presence", format!("{kind} array '{name}' is absent; it is required (A)"));
            return None;
        };
        if type_name(&a.data) != ty || a.components != components {
            self.fail(
                "type",
                format!(
                    "{kind} array '{name}' is {}x{} but the contract says {ty}x{components}",
                    type_name(&a.data),
                    a.components
                ),
            );
            return None;
        }
        Some(values(a))
    }

    // AI-FUNC-SUMMARY: Require a field array; returns its values; side effects: records a violation when absent or mistyped.
    fn field(&mut self, name: &str, ty: &str, components: usize) -> Option<Vec<i64>> {
        self.require("field", self.doc.field_array(name), name, ty, components)
    }

    // AI-FUNC-SUMMARY: Check a §2.3 end-offset table is monotone and ends inside its component array; returns the decoded sets; side effects: records violations.
    fn table(&mut self, name: &str, offsets: &[i64], members: &[i64]) -> Vec<Vec<i64>> {
        let mut sets = Vec::with_capacity(offsets.len());
        let mut start = 0i64;
        for (k, &end) in offsets.iter().enumerate() {
            if end < start || end as usize > members.len() {
                self.fail(
                    "set_offsets",
                    format!(
                        "{name} set {k}: offset {end} after {start} is not monotone within the \
                         {} members",
                        members.len()
                    ),
                );
                return sets;
            }
            sets.push(members[start as usize..end as usize].to_vec());
            start = end;
        }
        if start as usize != members.len() && !offsets.is_empty() {
            self.fail(
                "set_offsets",
                format!("{name}: the last offset {start} leaves {} members unindexed", members.len()),
            );
        }
        sets
    }
}

// AI-FUNC-SUMMARY:
// Purpose: `SPEC_meshgen_contracts.md` §4.2 - validate a document at the strength its metadata selects.
// Inputs: the document and its StageIndex (if declared).
// Returns: the strength applied and every violated rule (empty when valid).
// Side effects: None.
// Notes: Primary volume: `Counts` restated for this file and `region_key` inside the region-set
//   table. Mixed contract: every A array with its frozen type and component count; sentinels;
//   every set table monotone and in range; every key inside its table; every component in a
//   set present in the component table; no Sheet in any region set; `FaceTagOrientation` one
//   entry per tag member; `FaceTagSideElems` naming tets that share the face, on opposite sides.
//   `CurveRadialPatches` is required from StageIndex 8, where the producer emits it (D-21).
pub fn validate_contract(doc: &VtuDoc, stage: Option<i64>) -> (ContractStrength, Vec<ContractViolation>) {
    let strength = contract_strength(doc);
    let mut c = Checker { doc, out: Vec::new() };
    match strength {
        ContractStrength::GeometryOnly => {}
        ContractStrength::PrimaryVolume => primary_volume(&mut c),
        ContractStrength::MixedContract => mixed_contract(&mut c, stage),
    }
    (strength, c.out)
}

// AI-FUNC-SUMMARY: Primary-volume rules: Counts restated for this file, region_key within the region-set table; side effects: records violations.
fn primary_volume(c: &mut Checker) {
    let doc = c.doc;
    counts_restated(c);
    let region_key = c.require("cell", doc.cell_array("region_key"), "region_key", "Int32", 1);
    let offsets = c.field("RegionSetOffsets", "Int64", 1).unwrap_or_default();
    if let Some(keys) = region_key {
        let n = offsets.len() as i64;
        if let Some((i, k)) = keys.iter().enumerate().find(|(_, &k)| k < 0 || k >= n) {
            c.fail(
                "key_range",
                format!("tet {i} has region_key {k} outside the {n}-entry region-set table"),
            );
        }
    }
}

// AI-FUNC-SUMMARY: `Counts = [points, tets, tagged faces, curve cells]` must describe this file, not the document it was derived from; side effects: records a violation.
fn counts_restated(c: &mut Checker) {
    let doc = c.doc;
    let Some(counts) = c.field("Counts", "Int64", 1) else {
        return;
    };
    let tets = doc.types.iter().filter(|&&t| t == VTK_TETRA).count() as i64;
    let faces = doc.types.iter().filter(|&&t| t == VTK_TRIANGLE).count() as i64;
    let curves = doc
        .types
        .iter()
        .filter(|&&t| t == VTK_POLY_LINE)
        .count() as i64;
    let actual = vec![doc.points.len() as i64, tets, faces, curves];
    if counts != actual {
        c.fail(
            "counts",
            format!("Counts is {counts:?} but this file holds {actual:?} [points, tets, faces, curves]"),
        );
    }
}

// AI-FUNC-SUMMARY: Mixed-contract rules of §4.2 (see validate_contract); side effects: records violations.
fn mixed_contract(c: &mut Checker, stage: Option<i64>) {
    let doc = c.doc;
    for (name, ty, comps, tuples) in [
        ("SchemaVersion", "Int32", 1, 1),
        ("StageIndex", "Int32", 1, 1),
        ("GeneratorVersion", "Int32", 1, 3),
        ("ConfigHash", "UInt64", 1, 1),
        ("DomainMin", "Float64", 3, 1),
        ("DomainMax", "Float64", 3, 1),
        ("Counts", "Int64", 1, 4),
        ("DeterminismMode", "UInt8", 1, 1),
    ] {
        if let Some(v) = c.field(name, ty, comps) {
            if v.len() != comps * tuples {
                c.fail(
                    "type",
                    format!("metadata '{name}' has {} values, the contract says {}", v.len(), comps * tuples),
                );
            }
        }
    }
    counts_restated(c);

    let n_cells = doc.num_cells();
    let cell = |c: &mut Checker, name: &str, ty: &str| {
        c.require("cell", doc.cell_array(name), name, ty, 1)
    };
    let cell_kind = cell(c, "cell_kind", "UInt8");
    let region_key = cell(c, "region_key", "Int32");
    let partition_id = cell(c, "partition_id", "Int32");
    let regime = cell(c, "regime", "UInt8");
    let face_tag_key = cell(c, "face_tag_key", "Int32");
    let curve_id = cell(c, "curve_id", "Int32");
    let n_id_key = c.require("point", doc.point_array("n_id_key"), "n_id_key", "Int32", 1);
    let constraint_kind =
        c.require("point", doc.point_array("constraint_kind"), "constraint_kind", "UInt8", 1);
    let constraint_ref =
        c.require("point", doc.point_array("constraint_ref"), "constraint_ref", "Int32", 1);

    let region_offsets = c.field("RegionSetOffsets", "Int64", 1);
    let region_members = c.field("RegionSetComponents", "Int32", 1);
    let region_priority = c.field("RegionSetPriority", "UInt32", 1);
    let nid_offsets = c.field("NIdSetOffsets", "Int64", 1);
    let nid_members = c.field("NIdSetComponents", "Int32", 1);
    let tag_offsets = c.field("FaceTagOffsets", "Int64", 1);
    let tag_members = c.field("FaceTagComponents", "Int32", 1);
    let tag_kind = c.field("FaceTagKind", "UInt8", 1);
    let side_elems = c.field("FaceTagSideElems", "Int32", 2);
    let comp_x = c.field("ComponentX", "Int32", 1);
    let comp_y = c.field("ComponentY", "UInt32", 1);
    let comp_kind = c.field("ComponentKind", "UInt8", 1);
    let comp_closed = c.field("ComponentClosed", "UInt8", 1);
    let curve_kind = c.field("CurveKind", "UInt8", 1);
    let curve_offsets = c.field("CurveCompOffsets", "Int64", 1);
    let curve_members = c.field("CurveCompComponents", "Int32", 1);
    let radial = if stage.is_some_and(|s| s >= 8) {
        c.field("CurveRadialPatches", "Int32", 1)
    } else {
        None
    };

    // --- the tables ---
    let region_sets = match (&region_offsets, &region_members) {
        (Some(o), Some(m)) => c.table("RegionSet", o, m),
        _ => Vec::new(),
    };
    if let (Some(p), Some(o)) = (&region_priority, &region_offsets) {
        if p.len() != o.len() {
            c.fail(
                "table_length",
                format!("RegionSetPriority has {} entries for {} region sets", p.len(), o.len()),
            );
        }
    }
    let nid_sets = match (&nid_offsets, &nid_members) {
        (Some(o), Some(m)) => c.table("NIdSet", o, m),
        _ => Vec::new(),
    };
    let tag_sets = match (&tag_offsets, &tag_members) {
        (Some(o), Some(m)) => c.table("FaceTag", o, m),
        _ => Vec::new(),
    };
    if let (Some(k), Some(o)) = (&tag_kind, &tag_offsets) {
        if k.len() != o.len() {
            c.fail("table_length", format!("FaceTagKind has {} entries for {} tags", k.len(), o.len()));
        }
    }
    let curve_sets = match (&curve_offsets, &curve_members) {
        (Some(o), Some(m)) => c.table("CurveComp", o, m),
        _ => Vec::new(),
    };
    if let (Some(k), Some(o)) = (&curve_kind, &curve_offsets) {
        if k.len() != o.len() {
            c.fail("table_length", format!("CurveKind has {} rows but CurveCompOffsets {}", k.len(), o.len()));
        }
    }
    if let (Some(r), Some(k)) = (&radial, &curve_kind) {
        if r.len() != k.len() {
            c.fail("table_length", format!("CurveRadialPatches has {} rows for {} curves", r.len(), k.len()));
        }
    }
    if let Some(x) = &comp_x {
        for (name, other) in [("ComponentY", &comp_y), ("ComponentKind", &comp_kind), ("ComponentClosed", &comp_closed)] {
            if let Some(v) = other {
                if v.len() != x.len() {
                    c.fail("table_length", format!("{name} has {} rows for {} components", v.len(), x.len()));
                }
            }
        }
    }

    // --- per-cell kind, sentinels and keys ---
    let kind_of = |t: u8| match t {
        VTK_TETRA => Some(0),
        VTK_TRIANGLE => Some(1),
        VTK_POLY_LINE => Some(2),
        VTK_VOXEL => Some(3),
        _ => None,
    };
    let mut first: std::collections::BTreeMap<&'static str, String> = std::collections::BTreeMap::new();
    let mut counts: std::collections::BTreeMap<&'static str, usize> = std::collections::BTreeMap::new();
    let mut note = |rule: &'static str, message: String| {
        *counts.entry(rule).or_default() += 1;
        first.entry(rule).or_insert(message);
    };
    for i in 0..n_cells {
        let t = doc.types[i];
        let expected = kind_of(t);
        if let (Some(k), Some(e)) = (&cell_kind, expected) {
            if k[i] != e {
                note("cell_kind", format!("cell {i} is VTK type {t} but cell_kind {}", k[i]));
            }
        }
        if expected.is_none() {
            note("cell_type", format!("cell {i} has VTK type {t}, which the contract does not define"));
        }
        let is_tet = t == VTK_TETRA;
        let is_face = t == VTK_TRIANGLE;
        let is_curve = t == VTK_POLY_LINE;
        if let Some(v) = &region_key {
            if is_tet && (v[i] < 0 || v[i] as usize >= region_sets.len()) {
                note("key_range", format!("tet {i} has region_key {} outside the {}-entry table", v[i], region_sets.len()));
            }
            if !is_tet && v[i] != -1 {
                note("sentinel", format!("non-tet cell {i} has region_key {}, the sentinel is -1", v[i]));
            }
        }
        if let Some(v) = &partition_id {
            if !is_tet && v[i] != -1 {
                note("sentinel", format!("non-tet cell {i} has partition_id {}, the sentinel is -1", v[i]));
            }
        }
        if let Some(v) = &regime {
            if !is_tet && v[i] != 255 {
                note("sentinel", format!("non-tet cell {i} has regime {}, the sentinel is 255", v[i]));
            }
        }
        if let Some(v) = &face_tag_key {
            if is_face && (v[i] < 0 || v[i] as usize >= tag_sets.len()) {
                note("key_range", format!("face cell {i} has face_tag_key {} outside the {}-entry table", v[i], tag_sets.len()));
            }
            if !is_face && v[i] != -1 {
                note("sentinel", format!("non-face cell {i} has face_tag_key {}, the sentinel is -1", v[i]));
            }
        }
        if let Some(v) = &curve_id {
            if is_curve && (v[i] < 0 || v[i] as usize >= curve_sets.len()) {
                note("key_range", format!("curve cell {i} has curve_id {} outside the {}-row table", v[i], curve_sets.len()));
            }
            if !is_curve && v[i] != -1 {
                note("sentinel", format!("non-curve cell {i} has curve_id {}, the sentinel is -1", v[i]));
            }
        }
    }
    if let Some(v) = &n_id_key {
        if let Some((i, k)) = v.iter().enumerate().find(|(_, &k)| k < 0 || k as usize >= nid_sets.len()) {
            note("key_range", format!("point {i} has n_id_key {k} outside the {}-entry table", nid_sets.len()));
        }
    }
    if let (Some(kind), Some(r)) = (&constraint_kind, &constraint_ref) {
        if let Some((i, _)) = kind.iter().enumerate().find(|(i, &k)| k == 0 && r[*i] != -1) {
            note("sentinel", format!("free point {i} has constraint_ref {}, the sentinel is -1", r[i]));
        }
    }

    // --- components: existence, and no sheet ever owns volume ---
    if let Some(x) = &comp_x {
        let known: std::collections::BTreeSet<i64> = x.iter().copied().collect();
        let sheets: std::collections::BTreeSet<i64> = match &comp_kind {
            Some(kinds) => x.iter().zip(kinds).filter(|(_, &k)| k == 1).map(|(&x, _)| x).collect(),
            None => Default::default(),
        };
        for (table, sets) in [("RegionSet", &region_sets), ("NIdSet", &nid_sets), ("FaceTag", &tag_sets), ("CurveComp", &curve_sets)] {
            for (k, set) in sets.iter().enumerate() {
                if let Some(m) = set.iter().find(|m| **m != 0 && !known.contains(m)) {
                    note("component", format!("{table} set {k} names component {m}, which the component table does not list"));
                }
            }
        }
        for (k, set) in region_sets.iter().enumerate() {
            if let Some(m) = set.iter().find(|m| sheets.contains(m)) {
                note(
                    "sheet_volume",
                    format!("region set {k} contains component {m}, a Sheet - a sheet never owns volume (geometry §9.1 row 10)"),
                );
            }
        }
    }

    // --- FaceTagOrientation: one entry per flattened member (MG-07) ---
    if let (Some(orientation), Some(members)) = (doc.field_array("FaceTagOrientation"), &tag_members) {
        if orientation.data.len() != members.len() {
            note(
                "orientation_length",
                format!(
                    "FaceTagOrientation has {} entries for {} FaceTagComponents members; it must \
                     carry one orientation per member",
                    orientation.data.len(),
                    members.len()
                ),
            );
        }
    }

    // --- FaceTagSideElems: per face cell, tets that share the face, on opposite sides ---
    if let Some(sides) = &side_elems {
        let faces: Vec<usize> = (0..n_cells).filter(|&i| doc.types[i] == VTK_TRIANGLE).collect();
        if sides.len() != 2 * faces.len() {
            note(
                "side_elems",
                format!("FaceTagSideElems has {} pairs for {} face cells", sides.len() / 2, faces.len()),
            );
        } else {
            for (j, &f) in faces.iter().enumerate() {
                let tri = doc.cell(f);
                let mut signs = [0i8; 2];
                for side in 0..2 {
                    let e = sides[2 * j + side];
                    if e == -1 {
                        continue;
                    }
                    if e < 0 || e as usize >= n_cells || doc.types[e as usize] != VTK_TETRA {
                        note("side_elems", format!("face cell {f}: side element {e} is not a tet of this document"));
                        continue;
                    }
                    let tet = doc.cell(e as usize);
                    if !tri.iter().all(|n| tet.contains(n)) {
                        note("side_elems", format!("face cell {f}: side element {e} does not contain all three of its nodes"));
                        continue;
                    }
                    let apex = tet.iter().find(|n| !tri.contains(n)).copied().unwrap_or(tri[0]);
                    let p = |n: i64| doc.points[n as usize];
                    let o = orient3d(p(tri[0]), p(tri[1]), p(tri[2]), p(apex));
                    signs[side] = if o > 0.0 { 1 } else if o < 0.0 { -1 } else { 0 };
                }
                if signs[0] != 0 && signs[0] == signs[1] {
                    note("side_elems", format!("face cell {f}: elem+ and elem- lie on the same side of it"));
                }
            }
        }
    }

    for (rule, message) in first {
        let n = counts[rule];
        let message = if n > 1 { format!("{message} (and {} more of this rule)", n - 1) } else { message };
        c.fail(rule, message);
    }
}
