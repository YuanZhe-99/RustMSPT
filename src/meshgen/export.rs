//! S11 export (plan M-6.2, B.7): the Abaqus INP written from the final contract document.
//!
//! The INP is a *view* of the welded contract document, never a second meshing: nodes and
//! elements in VTU order (node `i` is INP node `i + 1`, tet cell `c` is element `c + 1`), so
//! `[V10]` can cross-check the two files by identity and not merely by count. Sets:
//!
//! - `ELSET=REGION_<k>` per region key `k` (the `RegionSet*` tables name its components),
//! - `ELSET=PARTITION_<p>` per S10 partition,
//! - `NSET=COMPONENT_<X>` per component, from each node's `N_ID` set,
//! - `*SURFACE, NAME=IFACE_<X>_POS / _NEG` per tagged component, element-based faces on the
//!   `(elem+, elem-)` sides `FaceTagSideElems` records - the adjacency a solver needs to insert a
//!   cohesive or contact treatment without re-deriving it (B.6).
//!
//! Material mapping (B.7): with no `materials:` block the INP carries sets and no sections, which
//! is a complete mesh for a user who assigns sections downstream. With one, every region key must
//! resolve - a singleton `{X}` through `by_component`, a multi-ID key through `by_region_key` as
//! `"X+Y"` - and an unmapped key is a hard error naming it, unless `unmapped: elset-only`, which
//! exports the set without a section and says so. There is no silent single-component assignment.

use crate::config::meshgen::{MeshGenMaterials, UnmappedPolicy};
use crate::error::{Result, RustMsptError};
use crate::io::vtu::{VtuDoc, VTK_TETRA, VTK_TRIANGLE};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Abaqus C3D4 faces by local node slots: S1 (1,2,3), S2 (1,4,2), S3 (2,4,3), S4 (3,4,1).
pub const C3D4_FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 3, 1], [1, 3, 2], [2, 3, 0]];

/// What the writer produced: the INP text and anything it had to report.
#[derive(Debug, Clone, Default)]
pub struct InpExport {
    pub text: String,
    pub warnings: Vec<String>,
    pub n_nodes: usize,
    pub n_elements: usize,
}

// AI-FUNC-SUMMARY: A cell or field array of the document read as i64 values; returns Option<Vec<i64>>; side effects: none.
fn ints(array: Option<&crate::io::vtu::DataArray>) -> Option<Vec<i64>> {
    array.map(|a| (0..a.data.len()).map(|i| a.data.get_i64(i)).collect())
}

// AI-FUNC-SUMMARY: Split a CSR pair (offsets = exclusive ends) into its member lists; returns Vec<Vec<i64>>; side effects: none.
fn csr(offsets: &[i64], members: &[i64]) -> Vec<Vec<i64>> {
    let mut out = Vec::with_capacity(offsets.len());
    let mut start = 0usize;
    for end in offsets {
        let end = (*end as usize).min(members.len());
        out.push(members[start.min(end)..end].to_vec());
        start = end;
    }
    out
}

// AI-FUNC-SUMMARY: Write one Abaqus data-line list, at most 16 integers per line; returns nothing; side effects: appends to `out`.
fn id_lines(out: &mut String, ids: &[usize]) {
    for chunk in ids.chunks(16) {
        let line: Vec<String> = chunk.iter().map(|i| i.to_string()).collect();
        let _ = writeln!(out, "{}", line.join(", "));
    }
}

// AI-FUNC-SUMMARY: The C3D4 face number (1..=4) of a tet whose three nodes are `face`; returns Option<usize>; side effects: none.
pub fn c3d4_face_of(tet: &[i64], face: [i64; 3]) -> Option<usize> {
    let mut want = face;
    want.sort_unstable();
    C3D4_FACES.iter().position(|slots| {
        let mut got = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
        got.sort_unstable();
        got == want
    })
    .map(|k| k + 1)
}

// AI-FUNC-SUMMARY:
// Purpose: Write the final contract document as an Abaqus INP (S11, plan M-6.2).
// Inputs: the stage-11 contract document (world coordinates) and the run's material mapping.
// Returns: the INP text with counts and warnings, or Err naming every unmapped region key.
// Side effects: None (the caller writes the file).
// Notes: Every tet of the document is positively oriented (`orient_positively`), which is C3D4's
//   positive-Jacobian order, so nodes are written as they are. Set and surface order is sorted by
//   id throughout, so the text is a function of the document (R-P2).
pub fn write_inp(doc: &VtuDoc, materials: &MeshGenMaterials) -> Result<InpExport> {
    let mut export = InpExport::default();
    let tets: Vec<usize> = (0..doc.types.len()).filter(|c| doc.types[*c] == VTK_TETRA).collect();
    if tets.iter().enumerate().any(|(at, c)| at != *c) {
        return Err(RustMsptError::InvalidMesh(
            "S11: the contract document must list its tets first".to_string(),
        ));
    }
    let region_key = ints(doc.cell_array("region_key")).unwrap_or_default();
    let partition = ints(doc.cell_array("partition_id")).unwrap_or_default();
    let sets = csr(
        &ints(doc.field_array("RegionSetOffsets")).unwrap_or_default(),
        &ints(doc.field_array("RegionSetComponents")).unwrap_or_default(),
    );
    let n_id_key = ints(doc.point_array("n_id_key")).unwrap_or_default();
    let n_id_sets = csr(
        &ints(doc.field_array("NIdSetOffsets")).unwrap_or_default(),
        &ints(doc.field_array("NIdSetComponents")).unwrap_or_default(),
    );
    let face_tag_key = ints(doc.cell_array("face_tag_key")).unwrap_or_default();
    let tag_sets = csr(
        &ints(doc.field_array("FaceTagOffsets")).unwrap_or_default(),
        &ints(doc.field_array("FaceTagComponents")).unwrap_or_default(),
    );
    let side_elems = ints(doc.field_array("FaceTagSideElems")).unwrap_or_default();

    let out = &mut export.text;
    let _ = writeln!(out, "*HEADING");
    let _ = writeln!(out, "rustmspt mesh - S11 export (C3D4), node i = VTU point i-1, element c = VTU tet cell c-1");
    let _ = writeln!(out, "*NODE");
    for (i, p) in doc.points.iter().enumerate() {
        let _ = writeln!(out, "{}, {:.17e}, {:.17e}, {:.17e}", i + 1, p.x, p.y, p.z);
    }
    let _ = writeln!(out, "*ELEMENT, TYPE=C3D4, ELSET=ALL_TETS");
    for c in &tets {
        let n = doc.cell(*c);
        let _ = writeln!(out, "{}, {}, {}, {}, {}", c + 1, n[0] + 1, n[1] + 1, n[2] + 1, n[3] + 1);
    }
    export.n_nodes = doc.points.len();
    export.n_elements = tets.len();

    // Regions.
    let mut by_region: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for c in &tets {
        by_region.entry(region_key.get(*c).copied().unwrap_or(0)).or_default().push(c + 1);
    }
    let name_of = |key: i64| -> String {
        let members = sets.get(key as usize).cloned().unwrap_or_default();
        members.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("+")
    };
    for (key, elements) in &by_region {
        let _ = writeln!(out, "** region {key} = {{{}}}", name_of(*key));
        let _ = writeln!(out, "*ELSET, ELSET=REGION_{key}");
        id_lines(out, elements);
    }
    // Partitions.
    let mut by_partition: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for c in &tets {
        by_partition.entry(partition.get(*c).copied().unwrap_or(0)).or_default().push(c + 1);
    }
    for (p, elements) in &by_partition {
        let _ = writeln!(out, "*ELSET, ELSET=PARTITION_{p}");
        id_lines(out, elements);
    }
    // Components, from N_ID.
    let mut by_component: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (node, key) in n_id_key.iter().enumerate() {
        for x in n_id_sets.get(*key as usize).into_iter().flatten() {
            if *x > 0 {
                by_component.entry(*x).or_default().push(node + 1);
            }
        }
    }
    for (x, nodes) in &by_component {
        let _ = writeln!(out, "*NSET, NSET=COMPONENT_{x}");
        id_lines(out, nodes);
    }
    // Interfaces: element-based surfaces on each tagged component's two sides.
    let mut surfaces: BTreeMap<(i64, bool), BTreeSet<(usize, usize)>> = BTreeMap::new();
    for c in 0..doc.types.len() {
        if doc.types[c] != VTK_TRIANGLE {
            continue;
        }
        let key = face_tag_key.get(c).copied().unwrap_or(-1);
        if key < 0 {
            continue;
        }
        let n = doc.cell(c);
        let face = [n[0], n[1], n[2]];
        let pair = [
            side_elems.get(2 * key as usize).copied().unwrap_or(-1),
            side_elems.get(2 * key as usize + 1).copied().unwrap_or(-1),
        ];
        for x in tag_sets.get(key as usize).into_iter().flatten() {
            for (side, elem) in [(true, pair[0]), (false, pair[1])] {
                if elem < 0 || (elem as usize) >= tets.len() {
                    continue;
                }
                if let Some(k) = c3d4_face_of(doc.cell(elem as usize), face) {
                    surfaces.entry((*x, side)).or_default().insert((elem as usize + 1, k));
                }
            }
        }
    }
    for ((x, positive), faces) in &surfaces {
        let _ = writeln!(
            out,
            "*SURFACE, TYPE=ELEMENT, NAME=IFACE_{x}_{}",
            if *positive { "POS" } else { "NEG" }
        );
        for (elem, k) in faces {
            let _ = writeln!(out, "{elem}, S{k}");
        }
    }

    // Sections.
    let mapped = !materials.by_component.is_empty() || !materials.by_region_key.is_empty();
    if !mapped {
        export.warnings.push(
            "[S11] no `materials:` block - the INP carries element sets and no sections".to_string(),
        );
        return Ok(export);
    }
    let by_x: BTreeMap<i64, &String> = materials.by_component.iter().map(|(x, m)| (*x, m)).collect();
    let mut unmapped: Vec<String> = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut sections = String::new();
    for key in by_region.keys() {
        let members = sets.get(*key as usize).cloned().unwrap_or_default();
        let name = name_of(*key);
        let material = match members.as_slice() {
            [x] => by_x.get(x).map(|m| m.to_string()),
            _ => materials.by_region_key.get(&name).cloned(),
        };
        match material {
            Some(m) => {
                used.insert(m.clone());
                let _ = writeln!(sections, "*SOLID SECTION, ELSET=REGION_{key}, MATERIAL={m}");
            }
            None => unmapped.push(format!("REGION_{key} {{{name}}}")),
        }
    }
    if !unmapped.is_empty() {
        match materials.unmapped {
            UnmappedPolicy::Error => {
                return Err(RustMsptError::InvalidConfig(format!(
                    "S11: region key(s) with no material mapping: {} - map each through \
                     materials.by_component (a single component) or materials.by_region_key \
                     (\"X+Y\"), or set materials.unmapped: elset-only",
                    unmapped.join(", ")
                )));
            }
            UnmappedPolicy::ElsetOnly => export.warnings.push(format!(
                "[S11] WARN: exported without a section (unmapped: elset-only): {}",
                unmapped.join(", ")
            )),
        }
    }
    out.push_str(&sections);
    for m in &used {
        let _ = writeln!(out, "*MATERIAL, NAME={m}");
        let _ = writeln!(out, "** properties of {m} are the user's to supply");
    }
    Ok(export)
}

/// An INP as `[V10]` reads it: only the keywords S11 writes.
#[derive(Debug, Clone, Default)]
pub struct ParsedInp {
    pub nodes: Vec<(usize, [f64; 3])>,
    pub elements: Vec<(usize, [usize; 4])>,
    pub elsets: BTreeMap<String, Vec<usize>>,
    pub nsets: BTreeMap<String, Vec<usize>>,
    pub surfaces: BTreeMap<String, Vec<(usize, usize)>>,
    /// ELSET name -> material, one per `*SOLID SECTION`.
    pub sections: BTreeMap<String, String>,
}

// AI-FUNC-SUMMARY: The value of `KEY=value` in a keyword line (case-insensitive key); returns Option<String>; side effects: none.
fn keyword_value(line: &str, key: &str) -> Option<String> {
    line.split(',').skip(1).find_map(|part| {
        let (k, v) = part.split_once('=')?;
        k.trim().eq_ignore_ascii_case(key).then(|| v.trim().to_string())
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Parse the subset of Abaqus INP that S11 writes, for `[V10]`'s cross-check.
// Inputs: the INP text.
// Returns: ParsedInp, or Err naming the first line it could not read.
// Side effects: None.
pub fn parse_inp(text: &str) -> Result<ParsedInp> {
    enum Block {
        None,
        Node,
        Element,
        Elset(String),
        Nset(String),
        Surface(String),
    }
    let mut out = ParsedInp::default();
    let mut block = Block::None;
    let bad = |n: usize, line: &str| RustMsptError::InvalidMesh(format!("INP line {}: cannot read '{line}'", n + 1));
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("**") {
            continue;
        }
        if let Some(keyword) = line.strip_prefix('*') {
            let head = keyword.split(',').next().unwrap_or("").trim().to_ascii_uppercase();
            block = match head.as_str() {
                "NODE" => Block::Node,
                "ELEMENT" => Block::Element,
                "ELSET" => Block::Elset(keyword_value(line, "ELSET").ok_or_else(|| bad(n, line))?),
                "NSET" => Block::Nset(keyword_value(line, "NSET").ok_or_else(|| bad(n, line))?),
                "SURFACE" => Block::Surface(keyword_value(line, "NAME").ok_or_else(|| bad(n, line))?),
                "SOLID SECTION" => {
                    let set = keyword_value(line, "ELSET").ok_or_else(|| bad(n, line))?;
                    let material = keyword_value(line, "MATERIAL").ok_or_else(|| bad(n, line))?;
                    out.sections.insert(set, material);
                    Block::None
                }
                _ => Block::None,
            };
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(|f| f.trim()).filter(|f| !f.is_empty()).collect();
        match &block {
            Block::Node => {
                if fields.len() != 4 {
                    return Err(bad(n, line));
                }
                let id = fields[0].parse().map_err(|_| bad(n, line))?;
                let mut p = [0.0; 3];
                for k in 0..3 {
                    p[k] = fields[k + 1].parse().map_err(|_| bad(n, line))?;
                }
                out.nodes.push((id, p));
            }
            Block::Element => {
                if fields.len() != 5 {
                    return Err(bad(n, line));
                }
                let mut v = [0usize; 5];
                for k in 0..5 {
                    v[k] = fields[k].parse().map_err(|_| bad(n, line))?;
                }
                out.elements.push((v[0], [v[1], v[2], v[3], v[4]]));
            }
            Block::Elset(name) | Block::Nset(name) => {
                let ids: Vec<usize> = fields
                    .iter()
                    .map(|f| f.parse().map_err(|_| bad(n, line)))
                    .collect::<Result<_>>()?;
                let target = if matches!(block, Block::Elset(_)) { &mut out.elsets } else { &mut out.nsets };
                target.entry(name.clone()).or_default().extend(ids);
            }
            Block::Surface(name) => {
                if fields.len() != 2 {
                    return Err(bad(n, line));
                }
                let elem = fields[0].parse().map_err(|_| bad(n, line))?;
                let face = fields[1]
                    .trim_start_matches(['S', 's'])
                    .parse()
                    .map_err(|_| bad(n, line))?;
                out.surfaces.entry(name.clone()).or_default().push((elem, face));
            }
            Block::None => {}
        }
    }
    Ok(out)
}
