//! Mesh verification check catalog — the core shared by in-pipeline stage
//! assertions and the standalone `mesh-verify` subcommand.
//!
//! Frozen contract: `SPEC_meshgen_contracts.md` §4 (catalog, severities, gates)
//! and §4.1 (JSON report schema). Check codes, severities, and the JSON shape are
//! part of that contract — tests assert on them, so they are not free to drift.
//!
//! Scope of this slice (GA-2): [V1]–[V4] complete, [V5] reported SKIPPED because
//! it needs the input surfaces, the VTU-only subsets of [V6]/[V7]/[V8], and [V12].
//! Every deferred check reports SKIPPED naming what it is waiting for, so a report
//! never silently omits a check.

use crate::io::vtu::{VtuDoc, VTK_POLY_LINE, VTK_TETRA, VTK_TRIANGLE};
use crate::meshgen::predicates::{node_key, tet_quality, TetQuality};
use crate::types::Vec3;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

// AI-FUNC-SUMMARY: Severity of a single verifier finding; ordering is Info < Warn < Fail; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    Fail,
}

impl Severity {
    // AI-FUNC-SUMMARY: Contract spelling used in the JSON report and human log; returns &'static str; side effects: none.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "INFO",
            Severity::Warn => "WARN",
            Severity::Fail => "FAIL",
        }
    }
}

// AI-FUNC-SUMMARY: Outcome of one catalog section; SKIPPED always carries a reason; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skipped,
}

impl CheckStatus {
    // AI-FUNC-SUMMARY: Contract spelling used in the JSON report; returns &'static str; side effects: none.
    pub fn as_str(self) -> &'static str {
        match self {
            CheckStatus::Pass => "PASS",
            CheckStatus::Warn => "WARN",
            CheckStatus::Fail => "FAIL",
            CheckStatus::Skipped => "SKIPPED",
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: One verifier finding, carrying enough location data for `mesh-render --highlight-from`.
// Notes: `code` is the stable contract identifier ("V3.hanging_node"); tests assert on it.
#[derive(Clone, Debug)]
pub struct VerifyItem {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub point_ids: Vec<i64>,
    pub cell_ids: Vec<i64>,
    pub coordinates: Vec<Vec3>,
}

impl VerifyItem {
    // AI-FUNC-SUMMARY: Build an item with no attached location; returns VerifyItem; side effects: none.
    fn bare(severity: Severity, code: &str, message: String) -> Self {
        VerifyItem {
            severity,
            code: code.to_string(),
            message,
            point_ids: Vec::new(),
            cell_ids: Vec::new(),
            coordinates: Vec::new(),
        }
    }
}

// AI-FUNC-SUMMARY: One catalog section ([V1]…[V12]) with its status, metrics and bounded item list; side effects: none.
#[derive(Clone, Debug)]
pub struct VerifySection {
    pub id: String,
    pub title: String,
    pub status: CheckStatus,
    pub skipped_reason: Option<String>,
    pub metrics: Vec<(String, f64)>,
    pub items: Vec<VerifyItem>,
    pub items_truncated: bool,
}

impl VerifySection {
    // AI-FUNC-SUMMARY: Start an empty PASS section; returns VerifySection; side effects: none.
    fn new(id: &str, title: &str) -> Self {
        VerifySection {
            id: id.to_string(),
            title: title.to_string(),
            status: CheckStatus::Pass,
            skipped_reason: None,
            metrics: Vec::new(),
            items: Vec::new(),
            items_truncated: false,
        }
    }

    // AI-FUNC-SUMMARY: Start a SKIPPED section naming what the check is waiting for; returns VerifySection; side effects: none.
    fn skipped(id: &str, title: &str, reason: &str) -> Self {
        let mut s = Self::new(id, title);
        s.status = CheckStatus::Skipped;
        s.skipped_reason = Some(reason.to_string());
        s
    }

    // AI-FUNC-SUMMARY: Record a named metric on this section; side effects: mutates self.
    fn metric(&mut self, name: &str, value: f64) {
        self.metrics.push((name.to_string(), value));
    }

    // AI-FUNC-SUMMARY: Append an item, capping the stored list and raising the section status; side effects: mutates self.
    fn push(&mut self, item: VerifyItem, cap: usize) {
        self.status = match (self.status, item.severity) {
            (CheckStatus::Fail, _) | (_, Severity::Fail) => CheckStatus::Fail,
            (CheckStatus::Warn, _) | (_, Severity::Warn) => CheckStatus::Warn,
            (s, _) => s,
        };
        if self.items.len() < cap {
            self.items.push(item);
        } else {
            self.items_truncated = true;
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Configurable gates and tolerances for the catalog (SPEC_meshgen_contracts §4).
// Notes: All length tolerances are fractions of the mesh bounding-box diagonal, so every
//   gate is scale-invariant. Defaults are the contract defaults.
#[derive(Clone, Copy, Debug)]
pub struct VerifyGates {
    pub max_ar_warn: f64,
    pub min_dihedral_deg: f64,
    pub low_dihedral_deg: f64,
    pub low_dihedral_share: f64,
    pub duplicate_node_tol_frac: f64,
    pub plane_tol_frac: f64,
    pub hanging_tol_frac: f64,
    /// [V5]'s surface-conformance gate: the largest interface-node distance to the input
    /// surfaces, as a fraction of that node's **local interface edge length** (PLAN §15,
    /// and the reference implementation's [4]). Local, not global: the mesh is graded, so
    /// one global denominator makes a well-fitted node in a fine region indistinguishable
    /// from a badly-fitted one in a coarse region.
    pub surface_distance_frac: f64,
    pub max_items_per_section: usize,
    pub expected_partitions: Option<i64>,
    pub warn_is_fatal: bool,
}

impl Default for VerifyGates {
    fn default() -> Self {
        VerifyGates {
            max_ar_warn: 20.0,
            min_dihedral_deg: 5.0,
            low_dihedral_deg: 10.0,
            low_dihedral_share: 1.0e-4,
            duplicate_node_tol_frac: 1.0e-6,
            plane_tol_frac: 1.0e-9,
            hanging_tol_frac: 1.0e-9,
            surface_distance_frac: 0.02,
            max_items_per_section: 50,
            expected_partitions: None,
            warn_is_fatal: false,
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Complete verification result for one mesh: metadata echo, section list and totals.
// Notes: `exit_code()` implements the subcommand contract (nonzero on FAIL, or on WARN when gated fatal).
#[derive(Clone, Debug)]
pub struct VerifyReport {
    pub input: String,
    pub schema_version: Option<i64>,
    pub stage_index: Option<i64>,
    pub config_hash: Option<u64>,
    pub determinism_mode: Option<i64>,
    pub generator_version: Option<[i64; 3]>,
    pub sections: Vec<VerifySection>,
    pub fail: usize,
    pub warn: usize,
    pub info: usize,
    pub checks_run: usize,
    pub checks_skipped: usize,
    pub warn_is_fatal: bool,
}

impl VerifyReport {
    // AI-FUNC-SUMMARY: True when nothing failed (and no gated-fatal warning fired); returns bool; side effects: none.
    pub fn passed(&self) -> bool {
        self.fail == 0 && !(self.warn_is_fatal && self.warn > 0)
    }

    // AI-FUNC-SUMMARY: Process exit status for the subcommand (0 pass, 1 otherwise); returns i32; side effects: none.
    pub fn exit_code(&self) -> i32 {
        if self.passed() {
            0
        } else {
            1
        }
    }

    // AI-FUNC-SUMMARY: All item codes that fired, sorted and deduplicated (the unit-test handle); returns Vec<String>; side effects: none.
    pub fn fired_codes(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .sections
            .iter()
            .flat_map(|s| s.items.iter())
            .filter(|i| i.severity >= Severity::Warn)
            .map(|i| i.code.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    // AI-FUNC-SUMMARY: Look up one section by contract id ("V3"); returns Option<&VerifySection>; side effects: none.
    pub fn section(&self, id: &str) -> Option<&VerifySection> {
        self.sections.iter().find(|s| s.id == id)
    }
}

// ----------------------------------------------------------------- internals

struct MeshView<'a> {
    doc: &'a VtuDoc,
    tets: Vec<usize>,
    faces: Vec<usize>,
    curves: Vec<usize>,
    diag: f64,
    domain: Option<(Vec3, Vec3)>,
}

// AI-FUNC-SUMMARY:
// Purpose: `SPEC_meshgen_contracts.md` §5.2 `[V9]` - the junction checks, on the curves the
//   mesh declares it carries.
// Inputs: the mesh view and the item cap.
// Returns: the `[V9]` section.
// Side effects: None.
// Notes: Three clauses, in the contract's order.
//   1. **A curve node's `N_ID` contains the curve's components.** `PLAN_mesh_generation.md`
//      §5.3 defines `N_ID` as the union of incident element labels and incident face tags,
//      so a node on the curve where components 3 and 5 meet must read `{.., 3, 5}`. This is
//      the clause that was unanswerable while `n_id_key` was a stub of zeros - and it would
//      have *passed* vacuously, since an empty declared set is contained in anything.
//   2. **The radial patch count matches.** S2's `radial_patch_order` says how many patches
//      it put around the curve, so the mesh must show that many material sectors around an
//      edge lying on it. Asked only where **two or more components** meet: around a sharp
//      edge of one solid the sectors are inside and outside whatever the patch count is, so
//      the identity does not hold there and asserting it would be a check that fails for a
//      reason other than the one it names.
//   3. **The edge fan closes.** "Multi-surface cell conformity across the shared-face cache"
//      is, in the written mesh, the statement that the tets around a junction edge form a
//      closed fan: every face incident to the edge is shared by exactly two of them. `[V3]`
//      does not cover it - it counts owners per face over the whole mesh, which a fan that
//      is open *along the edge* still satisfies.
//   A curve the mesh carries no edge for is reported, not failed: gate G6-0 decided against
//   constrained edge recovery in favour of the conforming fan, so the mesh is not required
//   to reproduce every curve as a chain of edges, and failing it here would gate a design
//   decision rather than a defect.
fn check_v9(
    view: &MeshView,
    owners: &HashMap<[i64; 3], Vec<usize>>,
    gates: &VerifyGates,
    cap: usize,
) -> VerifySection {
    let mut s = VerifySection::new("V9", "Junctions");
    let Some(curve_id) = cell_i64(view.doc, "curve_id") else {
        s.status = CheckStatus::Skipped;
        s.skipped_reason =
            Some("cell array 'curve_id' absent (produced by the mesh pipeline)".to_string());
        return s;
    };
    let curve_components = decode_set_table(view.doc, "CurveComp");
    let radial = field_i64(view.doc, "CurveRadialPatches");
    // Absent table means an external VTU that never declared its curves - there is nothing
    // to check against. An *empty* table means the arrangement found no curves, which is a
    // fact about the geometry (a lone sphere has none), and reporting that as a skip is how
    // a check comes to look unrun when it has actually answered.
    if view.doc.field_array("CurveCompOffsets").is_none() {
        s.status = CheckStatus::Skipped;
        s.skipped_reason = Some(
            "field array 'CurveCompOffsets' absent; nothing declares what meets along a curve"
                .to_string(),
        );
        return s;
    }
    let n_id_sets = decode_set_table(view.doc, "NIdSet");
    let n_id_key: Vec<i64> = view
        .doc
        .point_array("n_id_key")
        .map(|a| (0..a.data.len()).map(|i| a.data.get_i64(i)).collect())
        .unwrap_or_default();
    let region_key = cell_i64(view.doc, "region_key").unwrap_or_default();
    let plane_tol = gates.plane_tol_frac * view.diag;
    let hull = view.domain.and_then(|(lo, hi)| {
        let mut min = view.doc.points.first().copied()?;
        let mut max = min;
        for p in &view.doc.points {
            min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
            max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
        }
        let overhangs = min.x < lo.x - plane_tol
            || min.y < lo.y - plane_tol
            || min.z < lo.z - plane_tol
            || max.x > hi.x + plane_tol
            || max.y > hi.y + plane_tol
            || max.z > hi.z + plane_tol;
        overhangs.then_some((min, max))
    });

    // The junction edges, and the nodes on them.
    let mut edges: Vec<(usize, [i64; 2])> = Vec::new();
    let mut on_curve: HashSet<i64> = HashSet::new();
    for &c in &view.curves {
        // A curve cell is a *polyline*, so every consecutive pair of its nodes is an edge
        // of the curve. Reading only two-node cells would silently ignore any curve written
        // as a chain, which is how the contract allows them to be written.
        let n = view.doc.cell(c);
        let key = curve_id.get(c).copied().unwrap_or(-1);
        if key < 0 || n.len() < 2 {
            continue;
        }
        for pair in n.windows(2) {
            let (a, b) = (pair[0].min(pair[1]), pair[0].max(pair[1]));
            if a == b {
                continue;
            }
            edges.push((key as usize, [a, b]));
            on_curve.insert(a);
            on_curve.insert(b);
        }
    }
    s.metric("curves_declared", curve_components.len() as f64);
    s.metric("curve_edges", edges.len() as f64);
    let carried: HashSet<usize> = edges.iter().map(|(key, _)| *key).collect();
    s.metric("curves_carried", carried.len() as f64);

    // --- clause 1: a curve node's N_ID contains the curve's components ---
    let mut missing = 0usize;
    let mut reported: HashSet<(usize, i64)> = HashSet::new();
    for (key, edge) in &edges {
        let Some(want) = curve_components.get(*key) else {
            continue;
        };
        for node in edge {
            let set = n_id_key
                .get(*node as usize)
                .copied()
                .and_then(|k| usize::try_from(k).ok())
                .and_then(|k| n_id_sets.get(k));
            let held: Vec<i64> = set.cloned().unwrap_or_default();
            let absent: Vec<i64> = want
                .iter()
                .copied()
                .filter(|x| !held.contains(x))
                .collect();
            if absent.is_empty() || !reported.insert((*key, *node)) {
                continue;
            }
            missing += 1;
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V9.curve_node_id",
                format!(
                    "node {node} lies on curve {key}, where component(s) {want:?} meet, but its \
                     N_ID is {held:?} and does not contain {absent:?}; a node on a curve belongs \
                     to every body that meets along it"
                ),
            );
            item.coordinates.push(view.doc.points[*node as usize]);
            s.push(item, cap);
        }
    }
    s.metric("curve_nodes_missing_id", missing as f64);

    // --- the fan of tets around each junction edge ---
    let mut fan: HashMap<[i64; 2], Vec<usize>> = HashMap::new();
    for &c in &view.tets {
        let n = view.doc.cell(c);
        for i in 0..4 {
            for j in (i + 1)..4 {
                let (a, b) = (n[i].min(n[j]), n[i].max(n[j]));
                if on_curve.contains(&a) && on_curve.contains(&b) {
                    fan.entry([a, b]).or_default().push(c);
                }
            }
        }
    }

    // --- clause 2: the radial patch count ---
    let mut sector_mismatch = 0usize;
    let mut single_component = 0usize;
    for (key, edge) in &edges {
        let want = radial.get(*key).copied().unwrap_or(0);
        let components = curve_components.get(*key).map(|x| x.len()).unwrap_or(0);
        if components < 2 {
            single_component += 1;
            continue;
        }
        if want <= 0 {
            continue;
        }
        let Some(tets) = fan.get(edge) else { continue };
        let mut sectors: Vec<i64> = tets
            .iter()
            .map(|c| region_key.get(*c).copied().unwrap_or(-1))
            .collect();
        sectors.sort_unstable();
        sectors.dedup();
        if sectors.len() as i64 == want {
            continue;
        }
        sector_mismatch += 1;
        let mut item = VerifyItem::bare(
            Severity::Fail,
            "V9.radial_patches",
            format!(
                "edge {edge:?} lies on curve {key}, around which the arrangement ordered {want} \
                 patch(es), but the mesh shows {} material sector(s) ({} tet(s) around it); the \
                 mesh does not reproduce the junction's radial structure",
                sectors.len(),
                tets.len()
            ),
        );
        item.coordinates.push(view.doc.points[edge[0] as usize]);
        s.push(item, cap);
    }
    s.metric("radial_mismatches", sector_mismatch as f64);
    s.metric("single_component_curve_edges", single_component as f64);

    // --- clause 3: the edge fan closes ---
    let mut open_fans = 0usize;
    let mut boundary_edges = 0usize;
    for (_, edge) in &edges {
        let Some(tets) = fan.get(edge) else { continue };
        let mut incident: HashMap<[i64; 3], usize> = HashMap::new();
        // An edge on the **domain's** boundary cannot have a closed fan, and a locked curve
        // sitting there is the ordinary case for a solid whose surface reaches the box. Only
        // an edge in the interior is required to close. (A solid's own sharp edge is interior
        // to the mesh, because the background is meshed too - which is why this exempts
        // nothing on the acceptance cases and everything on a bare-cube fixture.)
        //
        // The test is `on_domain_plane`, the same one `[V3]` uses, and **not** "some incident
        // face has one owner". That weaker form was the first version and it is self-
        // defeating: an open fan *is* a fan with a singly-owned face, so exempting on that
        // condition makes the clause unable to fire in exactly the case it exists for. Found
        // by trying to build the negative fixture for it, which is the point of building one.
        let mut on_domain = false;
        for c in tets {
            let n = view.doc.cell(*c);
            for i in 0..4 {
                let face: Vec<i64> = (0..4).filter(|k| *k != i).map(|k| n[k]).collect();
                if face.contains(&edge[0]) && face.contains(&edge[1]) {
                    let key = face_key(face[0], face[1], face[2]);
                    if owners.get(&key).map(|list| list.len()) == Some(1)
                        && on_domain_plane(view, &key, plane_tol, hull)
                    {
                        on_domain = true;
                    }
                    *incident.entry(key).or_insert(0) += 1;
                }
            }
        }
        if on_domain {
            boundary_edges += 1;
            continue;
        }
        let open: Vec<[i64; 3]> = incident
            .iter()
            .filter(|(_, count)| **count != 2)
            .map(|(face, _)| *face)
            .collect();
        if open.is_empty() {
            continue;
        }
        open_fans += 1;
        let mut item = VerifyItem::bare(
            Severity::Fail,
            "V9.junction_fan",
            format!(
                "the {} tet(s) around junction edge {edge:?} do not close: {} of the face(s) \
                 incident to it are carried by one tet only",
                tets.len(),
                open.len()
            ),
        );
        item.coordinates.push(view.doc.points[edge[0] as usize]);
        s.push(item, cap);
    }
    s.metric("open_junction_fans", open_fans as f64);
    s.metric("boundary_curve_edges", boundary_edges as f64);

    let uncarried = curve_components.len() - carried.len();
    if uncarried > 0 {
        s.push(
            VerifyItem::bare(
                Severity::Info,
                "V9.curve_not_carried",
                format!(
                    "{uncarried} of {} declared curve(s) are carried by no chain of mesh edges; \
                     gate G6-0 adopted the conforming fan over constrained edge recovery, so this \
                     is the cost that decision names and not a failure of this check",
                    curve_components.len()
                ),
            ),
            cap,
        );
    }
    s
}

// AI-FUNC-SUMMARY: Read a field-data array as i64 values; returns Vec<i64> (empty when absent); side effects: none.
fn field_i64(doc: &VtuDoc, name: &str) -> Vec<i64> {
    doc.field_array(name)
        .map(|a| (0..a.data.len()).map(|i| a.data.get_i64(i)).collect())
        .unwrap_or_default()
}

// AI-FUNC-SUMMARY: Read a cell-data array as i64 values; returns Option<Vec<i64>>; side effects: none.
fn cell_i64(doc: &VtuDoc, name: &str) -> Option<Vec<i64>> {
    doc.cell_array(name)
        .map(|a| (0..a.data.len()).map(|i| a.data.get_i64(i)).collect())
}

// AI-FUNC-SUMMARY: Decode a §2.3 end-offset set table into per-key member lists; returns Vec<Vec<i64>>; side effects: none.
fn decode_set_table(doc: &VtuDoc, prefix: &str) -> Vec<Vec<i64>> {
    let offsets = field_i64(doc, &format!("{prefix}Offsets"));
    let comps = field_i64(doc, &format!("{prefix}Components"));
    let mut out = Vec::with_capacity(offsets.len());
    let mut start = 0usize;
    for &end in &offsets {
        let end = (end.max(0) as usize).min(comps.len());
        out.push(if start <= end {
            comps[start..end].to_vec()
        } else {
            Vec::new()
        });
        start = end;
    }
    out
}

impl<'a> MeshView<'a> {
    // AI-FUNC-SUMMARY: Partition cells by kind and derive the scale and domain box; returns MeshView; side effects: none.
    fn new(doc: &'a VtuDoc) -> Self {
        let mut tets = Vec::new();
        let mut faces = Vec::new();
        let mut curves = Vec::new();
        for (i, &t) in doc.types.iter().enumerate() {
            match t {
                VTK_TETRA => tets.push(i),
                VTK_TRIANGLE => faces.push(i),
                VTK_POLY_LINE => curves.push(i),
                _ => {}
            }
        }
        let dmin = field_f64(doc, "DomainMin");
        let dmax = field_f64(doc, "DomainMax");
        let domain = if dmin.len() == 3 && dmax.len() == 3 {
            Some((
                Vec3::new(dmin[0], dmin[1], dmin[2]),
                Vec3::new(dmax[0], dmax[1], dmax[2]),
            ))
        } else {
            None
        };
        let (mut lo, mut hi) = (
            Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        );
        for p in &doc.points {
            if p.x.is_finite() && p.y.is_finite() && p.z.is_finite() {
                lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
        let diag = if lo.x.is_finite() && hi.x.is_finite() {
            vec_len(hi.sub(lo)).max(f64::MIN_POSITIVE)
        } else {
            1.0
        };
        MeshView {
            doc,
            tets,
            faces,
            curves,
            diag,
            domain,
        }
    }

    // AI-FUNC-SUMMARY: Corner points of tet cell `c`; returns [Vec3; 4]; side effects: none.
    fn tet_points(&self, c: usize) -> [Vec3; 4] {
        let n = self.doc.cell(c);
        [
            self.doc.points[n[0] as usize],
            self.doc.points[n[1] as usize],
            self.doc.points[n[2] as usize],
            self.doc.points[n[3] as usize],
        ]
    }
}

// AI-FUNC-SUMMARY: Read a field-data array as f64 values; returns Vec<f64> (empty when absent); side effects: none.
fn field_f64(doc: &VtuDoc, name: &str) -> Vec<f64> {
    doc.field_array(name)
        .map(|a| (0..a.data.len()).map(|i| a.data.get_f64(i)).collect())
        .unwrap_or_default()
}

// AI-FUNC-SUMMARY: Euclidean length of a vector (types::Vec3 exposes only dot/cross); returns f64; side effects: none.
fn vec_len(v: Vec3) -> f64 {
    v.dot(v).sqrt()
}

// AI-FUNC-SUMMARY: Squared Euclidean length of a vector; returns f64; side effects: none.
fn vec_len2(v: Vec3) -> f64 {
    v.dot(v)
}

// AI-FUNC-SUMMARY: Sorted 3-node key identifying a triangular face regardless of winding; returns [i64; 3]; side effects: none.
fn face_key(a: i64, b: i64, c: i64) -> [i64; 3] {
    let mut k = [a, b, c];
    k.sort_unstable();
    k
}

const TET_FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 3, 2], [0, 1, 3], [0, 2, 1]];

// AI-FUNC-SUMMARY: Squared distance from a point to a triangle (closest-point clamp); returns f64; side effects: none.
fn point_triangle_dist2(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> f64 {
    let ab = b.sub(a);
    let ac = c.sub(a);
    let ap = p.sub(a);
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return ap.dot(ap);
    }
    let bp = p.sub(b);
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return bp.dot(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let t = d1 / (d1 - d3);
        return vec_len2(p.sub(a.add(ab.scale(t))));
    }
    let cp = p.sub(c);
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return cp.dot(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let t = d2 / (d2 - d6);
        return vec_len2(p.sub(a.add(ac.scale(t))));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let t = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return vec_len2(p.sub(b.add(c.sub(b).scale(t))));
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    vec_len2(p.sub(a.add(ab.scale(v)).add(ac.scale(w))))
}

// ------------------------------------------------------------------- checks

// AI-FUNC-SUMMARY:
// Purpose: Optional verifier inputs that need out-of-document context.
// Notes: `expected_stage` is parsed from a snapshot filename (`sNN`) by the
//   pipeline and cross-checked against `StageIndex` by [V12] (SPEC §3, T-C6).
//   `None` (the default) skips the filename cross-check, for non-snapshot inputs.
#[derive(Debug, Clone, Default)]
pub struct VerifyOptions {
    pub expected_stage: Option<u8>,
    /// The input surfaces, per component, in the mesh's own coordinates. Empty makes
    /// [V5] report SKIPPED with the reason, exactly as before it was implementable.
    pub surfaces: Vec<SurfaceComponent>,
}

impl VerifyOptions {
    // AI-FUNC-SUMMARY: Build options from a snapshot filename's parsed stage; returns VerifyOptions; side effects: none.
    pub fn from_path(path: &std::path::Path) -> Self {
        VerifyOptions {
            expected_stage: crate::meshgen::snapshot::Stage::from_path(path).map(|s| s.index()),
            surfaces: Vec::new(),
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Run the check catalog over a contract or external VTU.
// Inputs: the loaded document and the configured gates.
// Returns: VerifyReport with one section per catalog entry, in contract order.
// Side effects: None.
// Notes: Checks that need data this document does not carry report SKIPPED with a
//   reason naming the missing array or the producing stage - never silently omitted.
pub fn verify(doc: &VtuDoc, gates: &VerifyGates) -> VerifyReport {
    verify_with_options(doc, gates, VerifyOptions::default())
}

// AI-FUNC-SUMMARY:
// Purpose: Run the check catalog with out-of-document options (e.g. the snapshot
//   stage parsed from the filename, for the [V12] StageIndex cross-check).
// Inputs: the loaded document, the configured gates, and the options.
// Returns: VerifyReport with one section per catalog entry, in contract order.
// Side effects: None.
// Notes: `verify` delegates here with default options for backward compatibility.
pub fn verify_with_options(
    doc: &VtuDoc,
    gates: &VerifyGates,
    options: VerifyOptions,
) -> VerifyReport {
    let view = MeshView::new(doc);
    let cap = gates.max_items_per_section;
    let stage_index = field_i64(doc, "StageIndex").first().copied();
    let surface_stage = matches!(stage_index, Some(0..=3));

    let quality: Vec<TetQuality> = view
        .tets
        .iter()
        .map(|&c| tet_quality(view.tet_points(c)))
        .collect();

    let mut sections = vec![check_v1(&view, &quality, cap), check_v2(&view, gates, cap)];
    let (v3, face_owners) = check_v3(&view, gates, cap, stage_index);
    sections.push(v3);
    sections.push(check_v4(&view, &quality, gates, cap));
    sections.push(check_v5(&view, &options.surfaces, gates, cap));
    sections.push(check_v6(&view, &face_owners, surface_stage, cap));
    if surface_stage {
        sections.push(VerifySection::skipped(
            "V7",
            "Sheets & thin",
            "surface-stage snapshots s00-s03 do not contain the volume cells required by V7",
        ));
        sections.push(VerifySection::skipped(
            "V8",
            "Partitions",
            "surface-stage snapshots s00-s03 do not contain partitions required by V8",
        ));
    } else {
        sections.push(check_v7(&view, &face_owners, cap));
        sections.push(check_v8(&view, &face_owners, gates, cap));
    }
    if surface_stage {
        sections.push(VerifySection::skipped(
            "V9",
            "Junctions",
            "surface-stage snapshots s00-s03 carry no volume cells, so there are no junction edges to check",
        ));
    } else {
        sections.push(check_v9(&view, &face_owners, gates, cap));
    }
    sections.push(VerifySection::skipped(
        "V10",
        "Export completeness",
        "lands with G9-2; needs a companion Abaqus INP export",
    ));
    sections.push(VerifySection::skipped(
        "V11",
        "Compare mode",
        "lands with GK-3; use `mesh-verify --compare a.vtu b.vtu`",
    ));
    sections.push(check_v12(&view, cap, &options));

    let (mut fail, mut warn, mut info) = (0, 0, 0);
    let (mut run, mut skipped) = (0, 0);
    for s in &sections {
        if s.status == CheckStatus::Skipped {
            skipped += 1;
        } else {
            run += 1;
        }
        for i in &s.items {
            match i.severity {
                Severity::Fail => fail += 1,
                Severity::Warn => warn += 1,
                Severity::Info => info += 1,
            }
        }
    }

    let gv = field_i64(doc, "GeneratorVersion");
    VerifyReport {
        input: String::new(),
        schema_version: field_i64(doc, "SchemaVersion").first().copied(),
        stage_index,
        config_hash: field_i64(doc, "ConfigHash").first().map(|&v| v as u64),
        determinism_mode: field_i64(doc, "DeterminismMode").first().copied(),
        generator_version: (gv.len() == 3).then(|| [gv[0], gv[1], gv[2]]),
        sections,
        fail,
        warn,
        info,
        checks_run: run,
        checks_skipped: skipped,
        warn_is_fatal: gates.warn_is_fatal,
    }
}

// AI-FUNC-SUMMARY: [V1] Cells — non-positive volume, repeated nodes, duplicate cells, non-finite coordinates; returns VerifySection; side effects: none.
fn check_v1(view: &MeshView, quality: &[TetQuality], cap: usize) -> VerifySection {
    let mut s = VerifySection::new("V1", "Cells");
    let mut negative = 0usize;

    for (p, i) in view.doc.points.iter().zip(0i64..) {
        if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V1.non_finite",
                format!("point {i} has a non-finite coordinate"),
            );
            item.point_ids.push(i);
            s.push(item, cap);
        }
    }

    for (idx, &c) in view.tets.iter().enumerate() {
        if quality[idx].volume <= 0.0 {
            negative += 1;
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V1.negative_volume",
                format!("tet cell {c} has signed volume {:.6e}", quality[idx].volume),
            );
            item.cell_ids.push(c as i64);
            item.coordinates.push(centroid(view, c));
            s.push(item, cap);
        }
    }

    for &c in view.tets.iter().chain(view.faces.iter()) {
        let nodes = view.doc.cell(c);
        let mut sorted = nodes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() != nodes.len() {
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V1.repeated_node",
                format!("cell {c} references the same node more than once"),
            );
            item.cell_ids.push(c as i64);
            s.push(item, cap);
        }
    }

    let mut seen: HashMap<(u8, Vec<i64>), usize> = HashMap::new();
    for c in 0..view.doc.num_cells() {
        let mut key = view.doc.cell(c).to_vec();
        key.sort_unstable();
        if let Some(&first) = seen.get(&(view.doc.types[c], key.clone())) {
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V1.duplicate_cell",
                format!("cell {c} duplicates cell {first}"),
            );
            item.cell_ids.extend([c as i64, first as i64]);
            s.push(item, cap);
        } else {
            seen.insert((view.doc.types[c], key), c);
        }
    }

    s.metric("tets", view.tets.len() as f64);
    s.metric("negative_volume", negative as f64);
    s
}

// AI-FUNC-SUMMARY: Centroid of a cell's nodes; returns Vec3; side effects: none.
fn centroid(view: &MeshView, c: usize) -> Vec3 {
    let n = view.doc.cell(c);
    let mut acc = Vec3::new(0.0, 0.0, 0.0);
    for &i in n {
        acc = acc.add(view.doc.points[i as usize]);
    }
    acc.scale(1.0 / n.len().max(1) as f64)
}

// AI-FUNC-SUMMARY: [V2] Nodes — coincident duplicates (quantized key) and unreferenced nodes; returns VerifySection; side effects: none.
fn check_v2(view: &MeshView, gates: &VerifyGates, cap: usize) -> VerifySection {
    let mut s = VerifySection::new("V2", "Nodes");
    let q = gates.duplicate_node_tol_frac * view.diag;
    let mut seen: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut dups = 0usize;
    for (i, p) in view.doc.points.iter().enumerate() {
        if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
            continue;
        }
        let key = node_key(*p, q);
        if let Some(&first) = seen.get(&key) {
            dups += 1;
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V2.duplicate_node",
                format!("node {i} is coincident with node {first}"),
            );
            item.point_ids.extend([i as i64, first as i64]);
            item.coordinates.push(*p);
            s.push(item, cap);
        } else {
            seen.insert(key, i);
        }
    }

    let mut used = vec![false; view.doc.points.len()];
    for &n in &view.doc.connectivity {
        if n >= 0 && (n as usize) < used.len() {
            used[n as usize] = true;
        }
    }
    let unref = used.iter().filter(|&&u| !u).count();
    for (i, u) in used.iter().enumerate() {
        if !u {
            let mut item = VerifyItem::bare(
                Severity::Warn,
                "V2.unreferenced_node",
                format!("node {i} is not referenced by any cell"),
            );
            item.point_ids.push(i as i64);
            item.coordinates.push(view.doc.points[i]);
            s.push(item, cap);
        }
    }
    s.metric("points", view.doc.points.len() as f64);
    s.metric("duplicate_nodes", dups as f64);
    s.metric("unreferenced_nodes", unref as f64);
    s
}

// AI-FUNC-SUMMARY:
// Purpose: [V3] Conformity — face sharing, boundary leaks, hanging nodes, non-manifold edges.
// Returns: the section plus the tet-face ownership map, reused by [V7] and [V8].
// Side effects: None.
fn check_v3(
    view: &MeshView,
    gates: &VerifyGates,
    cap: usize,
    stage_index: Option<i64>,
) -> (VerifySection, HashMap<[i64; 3], Vec<usize>>) {
    // Before the cut (S8) the mesh is the background lattice, whose boundary is the
    // octree hull rather than the domain box: the lattice covers a cube around the
    // domain and overhangs it by up to one coarse cell per axis, which is exactly
    // what S8 trims. Reporting that as a leak would fail every pre-cut snapshot on a
    // non-cubic domain. Face sharing, hanging nodes and manifoldness - the real
    // content of Theorem T1 - still apply and are still FAIL.
    let pre_cut = matches!(stage_index, Some(5..=7));
    let mut s = VerifySection::new("V3", "Conformity");
    let mut owners: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for &c in &view.tets {
        let n = view.doc.cell(c).to_vec();
        for f in TET_FACES.iter() {
            owners
                .entry(face_key(n[f[0]], n[f[1]], n[f[2]]))
                .or_default()
                .push(c);
        }
    }

    let tagged: std::collections::HashSet<[i64; 3]> = view
        .faces
        .iter()
        .map(|&c| {
            let n = view.doc.cell(c);
            face_key(n[0], n[1], n[2])
        })
        .collect();

    let plane_tol = gates.plane_tol_frac * view.diag;
    // The background lattice is a cube around the domain and overhangs it by up to
    // one coarse cell per axis, so on such a mesh the *outer boundary* is the octree
    // hull, not the domain box, and a single-sided face out in the overhang is the
    // mesh's own outside rather than a leak. Detected from the mesh instead of
    // assumed from the stage, so a mesh that does not overhang - a final, clipped one
    // - keeps the strict test and nothing genuine is masked.
    let hull = view.domain.and_then(|(lo, hi)| {
        let mut min = view.doc.points.first().copied()?;
        let mut max = min;
        for p in &view.doc.points {
            min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
            max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
        }
        let overhangs = min.x < lo.x - plane_tol
            || min.y < lo.y - plane_tol
            || min.z < lo.z - plane_tol
            || max.x > hi.x + plane_tol
            || max.y > hi.y + plane_tol
            || max.z > hi.z + plane_tol;
        overhangs.then_some((min, max))
    });
    let (mut multi, mut leaks, mut boundary) = (0usize, 0usize, 0usize);
    let mut interface_cracks = 0usize;
    let mut keys: Vec<&[i64; 3]> = owners.keys().collect();
    keys.sort_unstable();
    for k in keys {
        let cells = &owners[k];
        if cells.len() >= 3 {
            multi += 1;
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V3.multi_shared_face",
                format!(
                    "face ({}, {}, {}) is shared by {} tets",
                    k[0],
                    k[1],
                    k[2],
                    cells.len()
                ),
            );
            item.point_ids.extend_from_slice(k);
            item.cell_ids.extend(cells.iter().map(|&c| c as i64));
            item.coordinates.push(face_centroid(view, k));
            s.push(item, cap);
        } else if cells.len() == 1 {
            boundary += 1;
            // A face with one adjacent tet is legitimate only on the domain boundary.
            // Being *tagged* does not excuse it: an interface between two materials sits
            // inside the volume and must have an element on each side. Excusing tagged
            // faces here made every crack along an interface invisible to `[V3]` - the
            // mesh reported "conformity clean" while ParaView's Feature Edges showed the
            // whole interface web, because those faces really were surface boundaries.
            // A cracked mesh is not a mesh a solver can assemble.
            let on_boundary = on_domain_plane(view, k, plane_tol, hull);
            if tagged.contains(k) && !on_boundary {
                interface_cracks += 1;
                if !pre_cut {
                    let mut item = VerifyItem::bare(
                        Severity::Fail,
                        "V3.interface_crack",
                        format!(
                            "tagged face ({}, {}, {}) has only one adjacent tet - the volume is open along the interface here",
                            k[0], k[1], k[2]
                        ),
                    );
                    item.point_ids.extend_from_slice(k);
                    item.cell_ids.push(cells[0] as i64);
                    item.coordinates.push(face_centroid(view, k));
                    s.push(item, cap);
                }
            }
            if !tagged.contains(k) && !on_boundary {
                leaks += 1;
                if pre_cut {
                    continue;
                }
                let mut item = VerifyItem::bare(
                    Severity::Fail,
                    "V3.boundary_leak",
                    format!(
                        "face ({}, {}, {}) has one adjacent tet but is neither tagged nor on a domain plane",
                        k[0], k[1], k[2]
                    ),
                );
                item.point_ids.extend_from_slice(k);
                item.cell_ids.push(cells[0] as i64);
                item.coordinates.push(face_centroid(view, k));
                s.push(item, cap);
            }
        }
    }

    // hanging nodes: a node lying on a face triangle it is not a vertex of
    let tol = gates.hanging_tol_frac * view.diag;
    let hanging = hanging_nodes(view, &owners, tol);
    for (node, face) in hanging.iter().take(cap) {
        let mut item = VerifyItem::bare(
            Severity::Fail,
            "V3.hanging_node",
            format!(
                "node {node} lies on face ({}, {}, {}) without being one of its vertices",
                face[0], face[1], face[2]
            ),
        );
        item.point_ids.push(*node);
        item.coordinates.push(view.doc.points[*node as usize]);
        s.push(item, cap);
    }
    if hanging.len() > cap {
        s.items_truncated = true;
    }

    // non-manifold edges: around an interior edge the incident face count must equal
    // the incident tet count; a boundary edge has exactly one more face than tets
    let mut edge_tets: HashMap<(i64, i64), usize> = HashMap::new();
    let mut edge_faces: HashMap<(i64, i64), std::collections::HashSet<[i64; 3]>> = HashMap::new();
    for &c in &view.tets {
        let n = view.doc.cell(c).to_vec();
        for i in 0..4 {
            for j in (i + 1)..4 {
                let e = (n[i].min(n[j]), n[i].max(n[j]));
                *edge_tets.entry(e).or_insert(0) += 1;
            }
        }
        for f in TET_FACES.iter() {
            let k = face_key(n[f[0]], n[f[1]], n[f[2]]);
            for i in 0..3 {
                for j in (i + 1)..3 {
                    let e = (k[i].min(k[j]), k[i].max(k[j]));
                    edge_faces.entry(e).or_default().insert(k);
                }
            }
        }
    }
    let mut nonmanifold = 0usize;
    let mut edges: Vec<&(i64, i64)> = edge_tets.keys().collect();
    edges.sort_unstable();
    for e in edges {
        let nt = edge_tets[e];
        let nf = edge_faces.get(e).map(|s| s.len()).unwrap_or(0);
        let bnd = edge_faces
            .get(e)
            .map(|fs| {
                fs.iter()
                    .filter(|k| owners.get(*k).map(|v| v.len()) == Some(1))
                    .count()
            })
            .unwrap_or(0);
        let expected = if bnd == 0 { nt } else { nt + 1 };
        if nf != expected {
            nonmanifold += 1;
            let mut item = VerifyItem::bare(
                Severity::Warn,
                "V3.non_manifold_edge",
                format!(
                    "edge ({}, {}) has {nf} incident faces around {nt} tets (expected {expected})",
                    e.0, e.1
                ),
            );
            item.point_ids.extend([e.0, e.1]);
            s.push(item, cap);
        }
    }

    if pre_cut {
        s.push(
            VerifyItem::bare(
                Severity::Info,
                "V3.deferred",
                format!(
                    "{leaks} boundary face(s) off the domain planes are the pre-cut lattice hull, \
                     not leaks; the domain trim is stage S8"
                ),
            ),
            cap,
        );
    }
    s.metric("interior_faces", (owners.len() - boundary) as f64);
    s.metric("boundary_faces", boundary as f64);
    s.metric("multi_shared_faces", multi as f64);
    s.metric("boundary_leaks", leaks as f64);
    s.metric("interface_cracks", interface_cracks as f64);
    s.metric("hanging_nodes", hanging.len() as f64);
    s.metric("non_manifold_edges", nonmanifold as f64);
    (s, owners)
}

// AI-FUNC-SUMMARY: Centroid of a face key's three nodes; returns Vec3; side effects: none.
fn face_centroid(view: &MeshView, k: &[i64; 3]) -> Vec3 {
    view.doc.points[k[0] as usize]
        .add(view.doc.points[k[1] as usize])
        .add(view.doc.points[k[2] as usize])
        .scale(1.0 / 3.0)
}

// AI-FUNC-SUMMARY: True when all three face nodes lie on one domain-box plane within tol, or on one plane of the octree hull when the mesh overhangs the box; returns bool; side effects: none.
fn on_domain_plane(view: &MeshView, k: &[i64; 3], tol: f64, hull: Option<(Vec3, Vec3)>) -> bool {
    let Some((lo, hi)) = view.domain else {
        return false;
    };
    let p: Vec<Vec3> = k.iter().map(|&i| view.doc.points[i as usize]).collect();
    for axis in 0..3 {
        let get = |v: &Vec3| match axis {
            0 => v.x,
            1 => v.y,
            _ => v.z,
        };
        if p.iter().all(|v| (get(v) - get(&lo)).abs() <= tol)
            || p.iter().all(|v| (get(v) - get(&hi)).abs() <= tol)
        {
            return true;
        }
        // The background lattice is a cube *around* the domain and overhangs it by up
        // to one coarse cell per axis, so on such a mesh the outer boundary is the
        // octree hull rather than the domain box, and a single-sided face on that
        // hull is the mesh's own outside rather than a leak - 128 of them on a
        // two-plate fixture whose mesh has no hole in it at all. The hull is taken as
        // the mesh's own bounding box, and only when it actually exceeds the domain;
        // testing "beyond the domain plane" instead would also swallow a detached
        // component sitting outside the box, which is a real defect and is what
        // `bad_detached_component.vtu` is for.
        if let Some((hull_lo, hull_hi)) = hull {
            if p.iter().all(|v| (get(v) - get(&hull_lo)).abs() <= tol)
                || p.iter().all(|v| (get(v) - get(&hull_hi)).abs() <= tol)
            {
                return true;
            }
        }
    }
    false
}

// AI-FUNC-SUMMARY:
// Purpose: Find nodes lying on a tet face they are not a vertex of (T-junctions).
// Inputs: mesh view, the face-ownership map, and an absolute distance tolerance.
// Returns: (node id, face key) pairs, deterministically ordered.
// Side effects: None.
// Notes: Candidate nodes come from a uniform spatial hash sized to the mean face extent,
//   so the scan is near-linear rather than points × faces.
fn hanging_nodes(
    view: &MeshView,
    owners: &HashMap<[i64; 3], Vec<usize>>,
    tol: f64,
) -> Vec<(i64, [i64; 3])> {
    let n_pts = view.doc.points.len();
    if n_pts == 0 || owners.is_empty() {
        return Vec::new();
    }
    let cell = (view.diag / (n_pts as f64).cbrt().max(1.0)).max(f64::MIN_POSITIVE);
    let hash = |p: Vec3| {
        (
            (p.x / cell).floor() as i64,
            (p.y / cell).floor() as i64,
            (p.z / cell).floor() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64, i64), Vec<i64>> = HashMap::new();
    for (i, p) in view.doc.points.iter().enumerate() {
        if p.x.is_finite() && p.y.is_finite() && p.z.is_finite() {
            grid.entry(hash(*p)).or_default().push(i as i64);
        }
    }

    let mut keys: Vec<&[i64; 3]> = owners.keys().collect();
    keys.sort_unstable();
    let mut out = Vec::new();
    for k in keys {
        let (a, b, c) = (
            view.doc.points[k[0] as usize],
            view.doc.points[k[1] as usize],
            view.doc.points[k[2] as usize],
        );
        let lo = Vec3::new(
            a.x.min(b.x).min(c.x) - tol,
            a.y.min(b.y).min(c.y) - tol,
            a.z.min(b.z).min(c.z) - tol,
        );
        let hi = Vec3::new(
            a.x.max(b.x).max(c.x) + tol,
            a.y.max(b.y).max(c.y) + tol,
            a.z.max(b.z).max(c.z) + tol,
        );
        let (l, h) = (hash(lo), hash(hi));
        let mut candidates = Vec::new();
        for gx in l.0..=h.0 {
            for gy in l.1..=h.1 {
                for gz in l.2..=h.2 {
                    if let Some(v) = grid.get(&(gx, gy, gz)) {
                        candidates.extend_from_slice(v);
                    }
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        for p in candidates {
            if k.contains(&p) {
                continue;
            }
            let pt = view.doc.points[p as usize];
            if pt.x < lo.x
                || pt.x > hi.x
                || pt.y < lo.y
                || pt.y > hi.y
                || pt.z < lo.z
                || pt.z > hi.z
            {
                continue;
            }
            if point_triangle_dist2(pt, a, b, c) <= tol * tol {
                out.push((p, *k));
            }
        }
    }
    out
}

// AI-FUNC-SUMMARY: [V4] Quality — aspect ratio, dihedral, scaled Jacobian distributions and gates; returns VerifySection; side effects: none.
fn check_v4(
    view: &MeshView,
    quality: &[TetQuality],
    gates: &VerifyGates,
    cap: usize,
) -> VerifySection {
    let mut s = VerifySection::new("V4", "Quality");
    if quality.is_empty() {
        s.status = CheckStatus::Skipped;
        s.skipped_reason = Some("no tetrahedral cells present".to_string());
        return s;
    }
    let n = quality.len() as f64;
    let mut worst_ar = 0.0f64;
    let mut min_dihedral = 180.0f64;
    let mut max_dihedral: f64 = 0.0;
    let mut min_sj = f64::INFINITY;
    let mut ar_over = 0usize;
    let mut low_dihedral = 0usize;
    // gate on cosine, not on the reported degrees (SPEC_meshgen_numerics §8.1 rule 4)
    let low_cos = gates.low_dihedral_deg.to_radians().cos();
    let floor_cos = gates.min_dihedral_deg.to_radians().cos();
    let mut below_floor = 0usize;
    for q in quality {
        worst_ar = worst_ar.max(q.aspect_ratio);
        min_dihedral = min_dihedral.min(q.min_dihedral_deg);
        max_dihedral = max_dihedral.max(q.max_dihedral_deg);
        min_sj = min_sj.min(q.scaled_jacobian);
        if q.aspect_ratio > gates.max_ar_warn {
            ar_over += 1;
        }
        if q.max_dihedral_cos > low_cos {
            low_dihedral += 1;
        }
        if q.max_dihedral_cos > floor_cos {
            below_floor += 1;
        }
    }

    let mut order: Vec<usize> = (0..quality.len()).collect();
    order.sort_by(|&a, &b| {
        quality[b]
            .aspect_ratio
            .partial_cmp(&quality[a].aspect_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(view.tets[a].cmp(&view.tets[b]))
    });
    for &i in order.iter().take(10) {
        if quality[i].aspect_ratio <= gates.max_ar_warn {
            break;
        }
        let c = view.tets[i];
        let mut item = VerifyItem::bare(
            Severity::Warn,
            "V4.aspect_ratio",
            format!(
                "tet cell {c}: aspect ratio {:.3} exceeds {:.3}",
                quality[i].aspect_ratio, gates.max_ar_warn
            ),
        );
        item.cell_ids.push(c as i64);
        item.coordinates.push(centroid(view, c));
        s.push(item, cap);
    }
    if below_floor > 0 {
        s.push(
            VerifyItem::bare(
                Severity::Warn,
                "V4.min_dihedral",
                format!(
                    "{below_floor} tets have a dihedral angle below the {:.1}° floor (worst {:.3}°)",
                    gates.min_dihedral_deg, min_dihedral
                ),
            ),
            cap,
        );
    }
    let share = low_dihedral as f64 / n;
    if share > gates.low_dihedral_share {
        s.push(
            VerifyItem::bare(
                Severity::Warn,
                "V4.low_dihedral_share",
                format!(
                    "{:.4}% of tets are below {:.1}° (gate {:.4}%)",
                    share * 100.0,
                    gates.low_dihedral_deg,
                    gates.low_dihedral_share * 100.0
                ),
            ),
            cap,
        );
    }

    s.metric("worst_aspect_ratio", worst_ar);
    s.metric("min_dihedral_deg", min_dihedral);
    s.metric("max_dihedral_deg", max_dihedral);
    s.metric("min_scaled_jacobian", min_sj);
    s.metric("aspect_ratio_over_gate", ar_over as f64);
    s.metric("below_low_dihedral", low_dihedral as f64);
    s
}

// AI-FUNC-SUMMARY:
// Purpose: The components whose material boundary crosses a face without the face declaring them.
// Inputs: the inside-sets of the two tets sharing the face, and the components tagged on the face.
// Returns: the components of the symmetric difference the face does not declare, sorted and unique.
// Side effects: None.
// Notes: This is the predicate `[V6]`'s region-adjacency rule cannot express. That rule compares
//   the *size* of the step against the number of components declared on the face and floors the
//   allowance at 1, so a single-component step across an untagged face satisfies it. Here the
//   question is per component and not per count: if crossing this face enters or leaves component
//   x, the face is a boundary of x and must name x.
fn undeclared_components(a: &[i64], b: &[i64], tags: Option<&BTreeSet<i64>>) -> Vec<i64> {
    let mut out: Vec<i64> = a
        .iter()
        .filter(|x| !b.contains(x))
        .chain(b.iter().filter(|x| !a.contains(x)))
        .filter(|x| !tags.is_some_and(|t| t.contains(x)))
        .copied()
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

// AI-FUNC-SUMMARY: Area of the triangle a face key names; returns f64 (0.0 if a node id is out of range); side effects: none.
fn face_area(points: &[Vec3], fk: &[i64; 3]) -> f64 {
    let corner = |i: usize| points.get(fk[i] as usize).copied();
    let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) else {
        return 0.0;
    };
    let n = b.sub(a).cross(c.sub(a));
    0.5 * n.dot(n).sqrt()
}

// AI-FUNC-SUMMARY: [V6] ID semantics (VTU-only subset) — region-key legality and one priority per key; returns VerifySection; side effects: none.
fn check_v6(
    view: &MeshView,
    owners: &HashMap<[i64; 3], Vec<usize>>,
    surface_stage: bool,
    cap: usize,
) -> VerifySection {
    let mut s = VerifySection::new("V6", "ID semantics");
    let Some(region_key) = cell_i64(view.doc, "region_key") else {
        s.status = CheckStatus::Skipped;
        s.skipped_reason =
            Some("cell array 'region_key' absent (produced by the mesh pipeline)".to_string());
        return s;
    };
    let sets = decode_set_table(view.doc, "RegionSet");
    if sets.is_empty() {
        s.status = CheckStatus::Skipped;
        s.skipped_reason =
            Some("field arrays 'RegionSetOffsets'/'RegionSetComponents' absent".to_string());
        return s;
    }
    let priority = field_i64(view.doc, "RegionSetPriority");
    let comp_x = field_i64(view.doc, "ComponentX");
    let comp_y = field_i64(view.doc, "ComponentY");
    let y_of: HashMap<i64, i64> = comp_x.iter().copied().zip(comp_y.iter().copied()).collect();

    let mut illegal = 0usize;
    for &c in &view.tets {
        let k = region_key.get(c).copied().unwrap_or(-1);
        if k < 0 || k as usize >= sets.len() {
            illegal += 1;
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V6.illegal_region_key",
                format!(
                    "tet cell {c} has region_key {k}, outside the {}-entry region-set table",
                    sets.len()
                ),
            );
            item.cell_ids.push(c as i64);
            item.coordinates.push(centroid(view, c));
            s.push(item, cap);
        }
    }

    let mut mismatches = 0usize;
    for (k, members) in sets.iter().enumerate() {
        let declared = priority.get(k).copied();
        let mut ys: Vec<i64> = members
            .iter()
            .filter_map(|x| y_of.get(x).copied())
            .collect();
        ys.sort_unstable();
        ys.dedup();
        if ys.len() > 1 {
            mismatches += 1;
            s.push(
                VerifyItem::bare(
                    Severity::Fail,
                    "V6.priority_mismatch",
                    format!("region set {k} mixes components with priorities {ys:?}"),
                ),
                cap,
            );
        } else if let (Some(d), Some(&y)) = (declared, ys.first()) {
            if d != y && !members.iter().all(|&x| x == 0) {
                mismatches += 1;
                s.push(
                    VerifyItem::bare(
                        Severity::Fail,
                        "V6.priority_mismatch",
                        format!(
                            "region set {k} declares priority {d} but its components carry {y}"
                        ),
                    ),
                    cap,
                );
            }
        }
    }

    // --- region adjacency ---
    // Two tets sharing a face differ by crossing exactly one component's surface, so
    // their inside-sets differ in exactly one member. Where every component in play
    // carries the same priority, the region key *is* the inside-set (`resolve()`'s
    // minimum-priority filter keeps all of them), so the rule is exact and checkable
    // here. Under mixed priorities it is not: crossing out of a high-priority body
    // re-exposes the low-priority label underneath, which changes the key by two
    // members legitimately. Those pairs are counted and reported, not failed.
    //
    // Background is the sentinel key `{0}`, which means "no component owns this" - the
    // empty set, not a set containing component 0. Comparing it literally would make
    // every ordinary material boundary look like a two-member jump.
    let inside_set = |k: i64| -> Vec<i64> {
        match sets.get(k as usize) {
            Some(members) => members.iter().copied().filter(|x| *x != 0).collect(),
            None => Vec::new(),
        }
    };
    // How many distinct components each tagged interface face carries. A step of two
    // is legal exactly where two surfaces are **coincident**: two solids meeting at an
    // exactly shared face have one mesh face that is both their boundaries, so crossing
    // it leaves one body and enters the other with no void in between. A-6's cube and
    // limb share the plane x = 0.5817 and are precisely this case.
    // The tags are per (component, face): two solids in exact contact declare the same
    // mesh face twice, once each. Union them - taking the last write would report one
    // component for a face that carries two, which is the exemption this reads for.
    let face_tag_sets = decode_set_table(view.doc, "FaceTag");
    let mut tags_on_face: HashMap<[i64; 3], BTreeSet<i64>> = HashMap::new();
    if let Some(face_tag_key) = cell_i64(view.doc, "face_tag_key") {
        for &c in &view.faces {
            let key = face_tag_key.get(c).copied().unwrap_or(-1);
            if key < 0 {
                continue;
            }
            let Some(members) = face_tag_sets.get(key as usize) else {
                continue;
            };
            let n = view.doc.cell(c);
            tags_on_face
                .entry(face_key(n[0], n[1], n[2]))
                .or_default()
                .extend(members.iter().copied().filter(|x| *x != 0));
        }
    }
    let components_on_face: HashMap<[i64; 3], usize> = tags_on_face
        .iter()
        .map(|(key, set)| (*key, set.len()))
        .collect();

    let mut bad_adjacent = 0usize;
    let mut skipped_mixed_priority = 0usize;
    if !surface_stage {
        for (fk, cells) in owners.iter() {
            if cells.len() != 2 {
                continue;
            }
            let (left, right) = (cells[0], cells[1]);
            let (ka, kb) = (
                region_key.get(left).copied().unwrap_or(-1),
                region_key.get(right).copied().unwrap_or(-1),
            );
            if ka == kb || ka < 0 || kb < 0 {
                continue;
            }
            let (a, b) = (inside_set(ka), inside_set(kb));
            let mut priorities: Vec<i64> = a
                .iter()
                .chain(b.iter())
                .filter_map(|x| y_of.get(x).copied())
                .collect();
            priorities.sort_unstable();
            priorities.dedup();
            if priorities.len() > 1 {
                skipped_mixed_priority += 1;
                continue;
            }
            let difference = a
                .iter()
                .filter(|x| !b.contains(x))
                .chain(b.iter().filter(|x| !a.contains(x)))
                .count();
            let tagged = components_on_face.get(fk).copied();
            let allowed = tagged.unwrap_or(1).max(1);
            if difference <= allowed {
                continue;
            }
            bad_adjacent += 1;
            let carries = match tagged {
                Some(n) => format!("the face is tagged with {n} component(s)"),
                None => "the face carries no interface tag at all".to_string(),
            };
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V6.region_adjacency",
                format!(
                    "tets {left} and {right} share a face across region keys {a:?} and {b:?}, \
                     which differ by {difference} components, but {carries}; a step may only \
                     change the inside-set by as many components as are coincident on the face \
                     crossed, and every material boundary must be a declared interface"
                ),
            );
            item.cell_ids.push(left as i64);
            item.cell_ids.push(right as i64);
            item.coordinates.push(centroid(view, left));
            s.push(item, cap);
        }
    }

    // --- undeclared material boundary ---
    // The rule above judges a step by its **size** against what the face declares, and floors
    // `allowed` at 1. So a one-component step across a face carrying no tag at all passes it:
    // entering component 2 from the void through an undeclared face is legal to `[V6]`. That is
    // exactly what a §7.6 fan leaves between two pieces of one escalated cell whose material
    // boundary runs through its interior, so the suite had no number for it and the defect was
    // argued about from renders instead. This measures it: a face across which a component is
    // entered or left is a boundary of that component, and every material boundary must be a
    // declared interface.
    //
    // Reported as a metric, not a finding. It is a young measurement - A-2 renders correctly
    // carrying a handful, A-7a carries thousands with no visible artefact - so it earns the right
    // to gate anything only after it has been shown to track something that matters. Mixed
    // priorities are excluded on the same grounds as above: there the inside-set difference is
    // not the material boundary, because a higher-priority label replaces the one beneath it.
    let parent_cell = cell_i64(view.doc, "parent_cell");
    let mut undeclared_faces = 0usize;
    let mut undeclared_area = 0.0f64;
    let mut undeclared_same_cell = 0usize;
    let mut undeclared_mixed_priority = 0usize;
    if !surface_stage {
        for (fk, cells) in owners.iter() {
            if cells.len() != 2 {
                continue;
            }
            let (left, right) = (cells[0], cells[1]);
            let (ka, kb) = (
                region_key.get(left).copied().unwrap_or(-1),
                region_key.get(right).copied().unwrap_or(-1),
            );
            if ka == kb || ka < 0 || kb < 0 {
                continue;
            }
            let (a, b) = (inside_set(ka), inside_set(kb));
            let missing = undeclared_components(&a, &b, tags_on_face.get(fk));
            if missing.is_empty() {
                continue;
            }
            let mut priorities: Vec<i64> = a
                .iter()
                .chain(b.iter())
                .filter_map(|x| y_of.get(x).copied())
                .collect();
            priorities.sort_unstable();
            priorities.dedup();
            if priorities.len() > 1 {
                undeclared_mixed_priority += 1;
                continue;
            }
            undeclared_faces += 1;
            undeclared_area += face_area(&view.doc.points, fk);
            // Both sides coming from one lattice cell is the §7.6 signature: the split handed
            // back a piece spanning the boundary and the fan turned that boundary into a
            // staircase of interior faces. Only available under `RUSTMSPT_CUT_DIAG`.
            if let Some(parents) = &parent_cell {
                if parents.get(left).is_some() && parents.get(left) == parents.get(right) {
                    undeclared_same_cell += 1;
                }
            }
        }
    }

    s.metric("region_sets", sets.len() as f64);
    s.metric("illegal_region_keys", illegal as f64);
    s.metric("priority_mismatches", mismatches as f64);
    s.metric("undeclared_boundary_faces", undeclared_faces as f64);
    s.metric("undeclared_boundary_area", undeclared_area);
    s.metric(
        "undeclared_boundary_mixed_priority",
        undeclared_mixed_priority as f64,
    );
    if parent_cell.is_some() {
        s.metric("undeclared_boundary_same_cell", undeclared_same_cell as f64);
    }
    s.metric("region_adjacency_violations", bad_adjacent as f64);
    s.metric(
        "region_adjacency_mixed_priority",
        skipped_mixed_priority as f64,
    );
    if skipped_mixed_priority > 0 {
        s.push(
            VerifyItem::bare(
                Severity::Info,
                "V6.adjacency_mixed_priority",
                format!(
                    "{skipped_mixed_priority} interface(s) join keys whose components carry \
                     different priorities; a higher-priority body's label replaces the one beneath \
                     it, so the one-component step does not apply and they are not judged here"
                ),
            ),
            cap,
        );
    }
    s.push(
        VerifyItem::bare(
            Severity::Info,
            "V6.deferred",
            "per-component volume error and the sampled priority audit need the input surfaces (stage S11)".to_string(),
        ),
        cap,
    );
    s
}

// AI-FUNC-SUMMARY: [V7] Sheets (VTU-only subset) — sheet faces must be welded to two tets sharing all three nodes; returns VerifySection; side effects: none.
fn check_v7(view: &MeshView, owners: &HashMap<[i64; 3], Vec<usize>>, cap: usize) -> VerifySection {
    let mut s = VerifySection::new("V7", "Sheets & thin features");
    let Some(face_tag_key) = cell_i64(view.doc, "face_tag_key") else {
        s.status = CheckStatus::Skipped;
        s.skipped_reason =
            Some("cell array 'face_tag_key' absent (produced by the mesh pipeline)".to_string());
        return s;
    };
    let kinds = field_i64(view.doc, "FaceTagKind");
    let mut sheets = 0usize;
    let mut unwelded = 0usize;
    for &c in &view.faces {
        let key = face_tag_key.get(c).copied().unwrap_or(-1);
        let kind = if key >= 0 {
            kinds.get(key as usize).copied()
        } else {
            None
        };
        if kind != Some(1) {
            continue;
        }
        sheets += 1;
        let n = view.doc.cell(c);
        let fk = face_key(n[0], n[1], n[2]);
        let adjacent = owners.get(&fk).map(|v| v.len()).unwrap_or(0);
        if adjacent != 2 {
            unwelded += 1;
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V7.unwelded_sheet_face",
                format!(
                    "sheet face cell {c} has {adjacent} adjacent tets sharing all three nodes (expected 2)"
                ),
            );
            item.cell_ids.push(c as i64);
            item.point_ids.extend_from_slice(&fk);
            item.coordinates.push(centroid(view, c));
            s.push(item, cap);
        }
    }
    s.metric("sheet_faces", sheets as f64);
    s.metric("unwelded_sheet_faces", unwelded as f64);
    check_v7_bands(&mut s, view, cap);
    s.push(
        VerifyItem::bare(
            Severity::Info,
            "V7.deferred",
            "sheet-node mid-surface distance and rim conformance land with G7-2".to_string(),
        ),
        cap,
    );
    s
}

// AI-FUNC-SUMMARY:
// Purpose: [V7]'s band half (G7-1) - the one-layer check, the band quality ranges, the Steiner count
//   and the `[THIN-SKIP]` inventory.
// Inputs: the section being built, the mesh view and the item cap.
// Returns: nothing; appends items and metrics to `s`.
// Side effects: None beyond `s`.
// Notes: The one-layer property is checked through the *nodes*, not through element adjacency, and
//   that is what makes it a VTU-only check. A band spans its gap with a single layer exactly when no
//   node of a band element lies strictly inside the gap: every corner is on one wall or the other,
//   and a wall is a tagged face. A second layer necessarily introduces the mid-gap nodes it is built
//   on, which is precisely what `bad_stacked_band.vtu` contains. The Steiner rung
//   (`regime = 2`, `SPEC_meshgen_contracts.md` §2.1) is the one legitimate interior node, so a
//   `regime = 2` element is allowed exactly one - a second would mean the fan was not built on the
//   cell's own boundary.
fn check_v7_bands(s: &mut VerifySection, view: &MeshView, cap: usize) {
    let Some(regime) = cell_i64(view.doc, "regime") else {
        s.push(
            VerifyItem::bare(
                Severity::Info,
                "V7.band_absent",
                "cell array 'regime' absent, so the mesh declares no band elements".to_string(),
            ),
            cap,
        );
        return;
    };
    let band_region = cell_i64(view.doc, "band_region").unwrap_or_default();
    let mut wall_nodes: HashSet<i64> = HashSet::new();
    for &c in &view.faces {
        for &node in view.doc.cell(c) {
            wall_nodes.insert(node);
        }
    }
    let mut n_band = 0usize;
    let mut n_steiner = 0usize;
    let mut stacked = 0usize;
    let mut regions: BTreeSet<i64> = BTreeSet::new();
    let mut worst_dihedral = f64::INFINITY;
    let mut worst_ar: f64 = 0.0;
    for &c in &view.tets {
        let code = regime.get(c).copied().unwrap_or(255);
        if code != 1 && code != 2 {
            continue;
        }
        n_band += 1;
        if code == 2 {
            n_steiner += 1;
        }
        if let Some(region) = band_region.get(c).copied() {
            if region >= 0 {
                regions.insert(region);
            }
        }
        let q = tet_quality(view.tet_points(c));
        worst_dihedral = worst_dihedral.min(q.min_dihedral_deg);
        worst_ar = worst_ar.max(q.aspect_ratio);
        let interior: Vec<i64> = view
            .doc
            .cell(c)
            .iter()
            .copied()
            .filter(|node| !wall_nodes.contains(node))
            .collect();
        let allowance = if code == 2 { 1 } else { 0 };
        if interior.len() > allowance {
            stacked += 1;
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V7.band_layers",
                format!(
                    "band element {c} has {} node(s) strictly inside the gap (at most {allowance} allowed for regime {code}): the band is more than one layer thick",
                    interior.len()
                ),
            );
            item.cell_ids.push(c as i64);
            item.point_ids.extend(interior.iter().copied());
            item.coordinates.push(centroid(view, c));
            s.push(item, cap);
        }
    }
    s.metric("band_elements", n_band as f64);
    s.metric("band_steiner_elements", n_steiner as f64);
    s.metric("band_regions", regions.len() as f64);
    s.metric("band_stacked_elements", stacked as f64);
    if n_band > 0 {
        s.metric("band_min_dihedral_deg", worst_dihedral);
        s.metric("band_max_aspect_ratio", worst_ar);
    }
    let skipped = field_i64(view.doc, "ThinSkipRegion");
    s.metric("thin_skip_regions", skipped.len() as f64);
    if !skipped.is_empty() {
        s.push(
            VerifyItem::bare(
                Severity::Info,
                "V7.thin_skip",
                format!(
                    "[THIN-SKIP] {} region(s) fell back to volumetric: {:?}",
                    skipped.len(),
                    skipped
                ),
            ),
            cap,
        );
    }
}

// AI-FUNC-SUMMARY:
// Purpose: [V8] Partitions — recompute the sheet-blocked flood fill and compare it with `partition_id`;
//   report the pinhole-leak heuristic and the optional expected-partition gate.
// Returns: VerifySection.
// Side effects: None.
fn check_v8(
    view: &MeshView,
    owners: &HashMap<[i64; 3], Vec<usize>>,
    gates: &VerifyGates,
    cap: usize,
) -> VerifySection {
    let mut s = VerifySection::new("V8", "Partitions");
    let stored = cell_i64(view.doc, "partition_id");
    let face_tag_key = cell_i64(view.doc, "face_tag_key");
    let kinds = field_i64(view.doc, "FaceTagKind");

    // adjacency is blocked only by sheet-tagged faces (§5.4)
    let mut blocked: std::collections::HashSet<[i64; 3]> = std::collections::HashSet::new();
    let mut sheet_faces: Vec<usize> = Vec::new();
    if let Some(tags) = &face_tag_key {
        for &c in &view.faces {
            let key = tags.get(c).copied().unwrap_or(-1);
            if key >= 0 && kinds.get(key as usize).copied() == Some(1) {
                let n = view.doc.cell(c);
                blocked.insert(face_key(n[0], n[1], n[2]));
                sheet_faces.push(c);
            }
        }
    }

    let index_of: HashMap<usize, usize> =
        view.tets.iter().enumerate().map(|(i, &c)| (c, i)).collect();
    let mut label = vec![usize::MAX; view.tets.len()];
    let mut next = 0usize;
    for start in 0..view.tets.len() {
        if label[start] != usize::MAX {
            continue;
        }
        let mut stack = vec![start];
        label[start] = next;
        while let Some(t) = stack.pop() {
            let c = view.tets[t];
            let n = view.doc.cell(c).to_vec();
            for f in TET_FACES.iter() {
                let k = face_key(n[f[0]], n[f[1]], n[f[2]]);
                if blocked.contains(&k) {
                    continue;
                }
                for &other in owners.get(&k).into_iter().flatten() {
                    if other == c {
                        continue;
                    }
                    if let Some(&oi) = index_of.get(&other) {
                        if label[oi] == usize::MAX {
                            label[oi] = next;
                            stack.push(oi);
                        }
                    }
                }
            }
        }
        next += 1;
    }

    let mut mismatches = 0usize;
    if let Some(stored) = &stored {
        // the stored numbering may differ from ours only by a bijection
        let mut map: HashMap<i64, usize> = HashMap::new();
        let mut rev: HashMap<usize, i64> = HashMap::new();
        for (i, &c) in view.tets.iter().enumerate() {
            let sid = stored.get(c).copied().unwrap_or(-1);
            let ours = label[i];
            let bad = match (map.get(&sid), rev.get(&ours)) {
                (Some(&m), Some(&r)) => m != ours || r != sid,
                (None, None) => {
                    map.insert(sid, ours);
                    rev.insert(ours, sid);
                    false
                }
                _ => true,
            };
            if bad {
                mismatches += 1;
                let mut item = VerifyItem::bare(
                    Severity::Fail,
                    "V8.partition_mismatch",
                    format!(
                        "tet cell {c} stores partition_id {sid} but the recomputed flood fill puts it in component {ours}"
                    ),
                );
                item.cell_ids.push(c as i64);
                item.coordinates.push(centroid(view, c));
                s.push(item, cap);
            }
        }
    } else {
        s.push(
            VerifyItem::bare(
                Severity::Info,
                "V8.no_stored_partitions",
                "cell array 'partition_id' absent; reporting the recomputed component count only"
                    .to_string(),
            ),
            cap,
        );
    }

    // pinhole heuristic: a sheet's boundary edges must be rim, intersection-curve or
    // box-clip edges; a boundary edge strictly interior to the domain is a leak
    let mut pinholes = 0usize;
    if !sheet_faces.is_empty() {
        let mut edge_count: HashMap<(i64, i64), usize> = HashMap::new();
        for &c in &sheet_faces {
            let n = view.doc.cell(c);
            for i in 0..3 {
                let (a, b) = (n[i], n[(i + 1) % 3]);
                *edge_count.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        let curve_edges = curve_edge_set(view);
        let plane_tol = gates.plane_tol_frac * view.diag;
        let mut open: Vec<(i64, i64)> = edge_count
            .iter()
            .filter(|(_, &n)| n == 1)
            .map(|(&e, _)| e)
            .collect();
        open.sort_unstable();
        for e in open {
            if curve_edges.contains(&e) {
                continue;
            }
            if edge_on_domain_boundary(view, e, plane_tol) {
                continue;
            }
            pinholes += 1;
            let mut item = VerifyItem::bare(
                Severity::Fail,
                "V8.pinhole_sheet",
                format!(
                    "sheet boundary edge ({}, {}) is interior to the domain and is not a declared curve",
                    e.0, e.1
                ),
            );
            item.point_ids.extend([e.0, e.1]);
            item.coordinates.push(
                view.doc.points[e.0 as usize]
                    .add(view.doc.points[e.1 as usize])
                    .scale(0.5),
            );
            s.push(item, cap);
        }
    }

    if let Some(expected) = gates.expected_partitions {
        if next as i64 != expected {
            s.push(
                VerifyItem::bare(
                    Severity::Warn,
                    "V8.expected_partitions",
                    format!("found {next} partitions, expected {expected}"),
                ),
                cap,
            );
        }
    }

    s.metric("partitions", next as f64);
    s.metric("partition_mismatches", mismatches as f64);
    s.metric("sheet_pinholes", pinholes as f64);
    s
}

// AI-FUNC-SUMMARY: Edge set covered by the document's curve cells; returns a sorted-pair set; side effects: none.
fn curve_edge_set(view: &MeshView) -> std::collections::HashSet<(i64, i64)> {
    let mut out = std::collections::HashSet::new();
    for &c in &view.curves {
        let n = view.doc.cell(c);
        for w in n.windows(2) {
            out.insert((w[0].min(w[1]), w[0].max(w[1])));
        }
    }
    out
}

// AI-FUNC-SUMMARY: True when both edge endpoints lie on one domain-box plane; returns bool; side effects: none.
fn edge_on_domain_boundary(view: &MeshView, e: (i64, i64), tol: f64) -> bool {
    let Some((lo, hi)) = view.domain else {
        return false;
    };
    let (a, b) = (view.doc.points[e.0 as usize], view.doc.points[e.1 as usize]);
    for axis in 0..3 {
        let get = |v: &Vec3| match axis {
            0 => v.x,
            1 => v.y,
            _ => v.z,
        };
        for plane in [get(&lo), get(&hi)] {
            if (get(&a) - plane).abs() <= tol && (get(&b) - plane).abs() <= tol {
                return true;
            }
        }
    }
    false
}

// AI-FUNC-SUMMARY: [V12] Provenance & statistics — counts, metadata echo, stage/count agreement, schema/stage checks (T-C6); returns VerifySection; side effects: none.
fn check_v12(view: &MeshView, cap: usize, options: &VerifyOptions) -> VerifySection {
    let mut s = VerifySection::new("V12", "Provenance & statistics");
    s.metric("points", view.doc.points.len() as f64);
    s.metric("cells", view.doc.num_cells() as f64);
    s.metric("tets", view.tets.len() as f64);
    s.metric("tagged_faces", view.faces.len() as f64);
    s.metric("curve_cells", view.curves.len() as f64);
    s.metric("bbox_diagonal", view.diag);

    // Schema version (SPEC §2.4, T-C6): a present-but-wrong value is a named FAIL.
    let schema = field_i64(view.doc, "SchemaVersion");
    if !schema.is_empty() && schema[0] != 1 {
        s.push(
            VerifyItem::bare(
                Severity::Fail,
                "V12.schema_version_mismatch",
                format!(
                    "SchemaVersion is {} but the contract schema is 1",
                    schema[0]
                ),
            ),
            cap,
        );
    }

    // StageIndex range and filename cross-check (SPEC §3, T-C6).
    let stage = field_i64(view.doc, "StageIndex");
    if !stage.is_empty() {
        let idx = stage[0];
        if !(0..=11).contains(&idx) {
            s.push(
                VerifyItem::bare(
                    Severity::Fail,
                    "V12.stage_index_out_of_range",
                    format!("StageIndex {idx} is outside the frozen enumeration 0..=11"),
                ),
                cap,
            );
        }
        if let Some(expected) = options.expected_stage {
            if idx as u8 != expected {
                s.push(
                    VerifyItem::bare(
                        Severity::Fail,
                        "V12.stage_index_filename_mismatch",
                        format!(
                            "StageIndex {idx} disagrees with the filename stage s{:02} ({expected})",
                            expected
                        ),
                    ),
                    cap,
                );
            }
        }
    }

    let counts = field_i64(view.doc, "Counts");
    if counts.len() == 4 {
        let actual = [
            view.doc.points.len() as i64,
            view.tets.len() as i64,
            view.faces.len() as i64,
            view.curves.len() as i64,
        ];
        if counts != actual {
            s.push(
                VerifyItem::bare(
                    Severity::Warn,
                    "V12.count_mismatch",
                    format!("metadata Counts {counts:?} disagree with the mesh {actual:?}"),
                ),
                cap,
            );
        }
    } else {
        s.push(
            VerifyItem::bare(
                Severity::Info,
                "V12.no_metadata",
                "metadata 'Counts' absent; treating the input as an external VTU".to_string(),
            ),
            cap,
        );
    }
    s
}

// ------------------------------------------------------------------ outputs

// AI-FUNC-SUMMARY: Escape a string for JSON string context; returns String; side effects: none.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

// AI-FUNC-SUMMARY: Format an f64 for JSON, mapping non-finite values to null; returns String; side effects: none.
fn json_num(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else {
        "null".to_string()
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Serialize a report as the frozen JSON document (SPEC_meshgen_contracts §4.1).
// Inputs: the report.
// Returns: JSON text.
// Side effects: None.
// Notes: Hand-rolled (the project carries no JSON dependency, matching the VTU writer);
//   item order within a section is the deterministic order they were produced in.
pub fn report_to_json(report: &VerifyReport) -> String {
    let mut out = String::new();
    out.push_str("{\n  \"schema\": 1,\n  \"tool\": \"mesh-verify\",\n");
    let _ = writeln!(out, "  \"input\": \"{}\",", json_escape(&report.input));
    match report.generator_version {
        Some(v) => {
            let _ = writeln!(
                out,
                "  \"generator_version\": [{}, {}, {}],",
                v[0], v[1], v[2]
            );
        }
        None => out.push_str("  \"generator_version\": null,\n"),
    }
    match report.schema_version {
        Some(v) => {
            let _ = writeln!(out, "  \"schema_version\": {v},");
        }
        None => out.push_str("  \"schema_version\": null,\n"),
    }
    match report.stage_index {
        Some(v) => {
            let _ = writeln!(out, "  \"stage_index\": {v},");
        }
        None => out.push_str("  \"stage_index\": null,\n"),
    }
    match report.config_hash {
        Some(v) => {
            let _ = writeln!(out, "  \"config_hash\": \"0x{v:016x}\",");
        }
        None => out.push_str("  \"config_hash\": null,\n"),
    }
    let mode = match report.determinism_mode {
        Some(0) => "\"strict\"".to_string(),
        Some(1) => "\"fast\"".to_string(),
        Some(other) => format!("\"unknown({other})\""),
        None => "null".to_string(),
    };
    let _ = writeln!(out, "  \"determinism_mode\": {mode},");
    let _ = writeln!(
        out,
        "  \"summary\": {{ \"fail\": {}, \"warn\": {}, \"info\": {}, \"checks_run\": {}, \"checks_skipped\": {} }},",
        report.fail, report.warn, report.info, report.checks_run, report.checks_skipped
    );
    out.push_str("  \"sections\": [\n");
    for (si, s) in report.sections.iter().enumerate() {
        out.push_str("    {\n");
        let _ = writeln!(out, "      \"id\": \"{}\",", json_escape(&s.id));
        let _ = writeln!(out, "      \"title\": \"{}\",", json_escape(&s.title));
        let _ = writeln!(out, "      \"status\": \"{}\",", s.status.as_str());
        match &s.skipped_reason {
            Some(r) => {
                let _ = writeln!(out, "      \"skipped_reason\": \"{}\",", json_escape(r));
            }
            None => out.push_str("      \"skipped_reason\": null,\n"),
        }
        out.push_str("      \"metrics\": {");
        for (i, (k, v)) in s.metrics.iter().enumerate() {
            let _ = write!(
                out,
                "{}\"{}\": {}",
                if i == 0 { " " } else { ", " },
                json_escape(k),
                json_num(*v)
            );
        }
        out.push_str(" },\n      \"items\": [\n");
        for (ii, item) in s.items.iter().enumerate() {
            out.push_str("        {");
            let _ = write!(
                out,
                " \"severity\": \"{}\", \"code\": \"{}\", \"message\": \"{}\"",
                item.severity.as_str(),
                json_escape(&item.code),
                json_escape(&item.message)
            );
            let ids = |v: &Vec<i64>| {
                v.iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = write!(out, ", \"point_ids\": [{}]", ids(&item.point_ids));
            let _ = write!(out, ", \"cell_ids\": [{}]", ids(&item.cell_ids));
            let coords: Vec<String> = item
                .coordinates
                .iter()
                .map(|p| format!("[{}, {}, {}]", json_num(p.x), json_num(p.y), json_num(p.z)))
                .collect();
            let _ = write!(out, ", \"coordinates\": [{}] }}", coords.join(", "));
            out.push_str(if ii + 1 == s.items.len() { "\n" } else { ",\n" });
        }
        let _ = writeln!(out, "      ],");
        let _ = writeln!(out, "      \"items_truncated\": {}", s.items_truncated);
        out.push_str(if si + 1 == report.sections.len() {
            "    }\n"
        } else {
            "    },\n"
        });
    }
    out.push_str("  ]\n}\n");
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Render the sectioned human-readable log (reference-style `[PASS]/[WARN]/[FAIL]`).
// Inputs: the report.
// Returns: log text.
// Side effects: None.
pub fn report_to_log(report: &VerifyReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "=== mesh-verify: {} ===", report.input);
    if let Some(v) = report.stage_index {
        let _ = writeln!(
            out,
            "stage index: {v}    schema: {:?}",
            report.schema_version
        );
    }
    for s in &report.sections {
        let tag = match s.status {
            CheckStatus::Pass => "[PASS]",
            CheckStatus::Warn => "[WARN]",
            CheckStatus::Fail => "[FAIL]",
            CheckStatus::Skipped => "[SKIP]",
        };
        let _ = writeln!(out, "\n{tag} [{}] {}", s.id, s.title);
        if let Some(r) = &s.skipped_reason {
            let _ = writeln!(out, "       reason: {r}");
        }
        if !s.metrics.is_empty() {
            let joined: Vec<String> = s
                .metrics
                .iter()
                .map(|(k, v)| format!("{k}={}", json_num(*v)))
                .collect();
            let _ = writeln!(out, "       {}", joined.join("  "));
        }
        for item in &s.items {
            let _ = writeln!(
                out,
                "       {} {}: {}",
                item.severity.as_str(),
                item.code,
                item.message
            );
        }
        if s.items_truncated {
            let _ = writeln!(out, "       … more items omitted (item cap reached)");
        }
    }
    let _ = writeln!(
        out,
        "\nSUMMARY  fail={} warn={} info={}  checks_run={} skipped={}  -> {}",
        report.fail,
        report.warn,
        report.info,
        report.checks_run,
        report.checks_skipped,
        if report.passed() { "PASS" } else { "FAIL" }
    );
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Produce an annotated copy of the document carrying the [V4] quality arrays
//   and the per-cell `verify_flags` bitmask (SPEC_meshgen_contracts §2.1).
// Inputs: the source document and its report.
// Returns: a new VtuDoc with the Ann arrays appended (existing arrays preserved).
// Side effects: None.
// Notes: bit k of `verify_flags` is set when check V(k+1) produced an item naming that cell.
pub fn annotate(doc: &VtuDoc, report: &VerifyReport) -> VtuDoc {
    use crate::io::vtu::{ArrayData, DataArray};
    let view = MeshView::new(doc);
    let n = doc.num_cells();
    let (mut ar, mut rr, mut dih, mut sj) = (
        vec![0.0f32; n],
        vec![0.0f32; n],
        vec![0.0f32; n],
        vec![0.0f32; n],
    );
    for &c in &view.tets {
        let q = tet_quality(view.tet_points(c));
        ar[c] = if q.aspect_ratio.is_finite() {
            q.aspect_ratio as f32
        } else {
            f32::MAX
        };
        rr[c] = q.radius_ratio as f32;
        dih[c] = q.min_dihedral_deg as f32;
        sj[c] = q.scaled_jacobian as f32;
    }
    let mut flags = vec![0u32; n];
    for s in &report.sections {
        let Some(bit) = s.id.strip_prefix('V').and_then(|d| d.parse::<u32>().ok()) else {
            continue;
        };
        let mask = 1u32 << (bit.saturating_sub(1)).min(31);
        for item in &s.items {
            if item.severity < Severity::Warn {
                continue;
            }
            for &c in &item.cell_ids {
                if c >= 0 && (c as usize) < n {
                    flags[c as usize] |= mask;
                }
            }
        }
    }
    let mut out = doc.clone();
    for (name, data) in [
        ("aspect_ratio", ArrayData::F32(ar)),
        ("radius_ratio", ArrayData::F32(rr)),
        ("min_dihedral_deg", ArrayData::F32(dih)),
        ("scaled_jacobian", ArrayData::F32(sj)),
    ] {
        out.cell_data.retain(|a| a.name != name);
        out.cell_data.push(DataArray::scalar(name, data));
    }
    out.cell_data.retain(|a| a.name != "verify_flags");
    out.cell_data
        .push(DataArray::scalar("verify_flags", ArrayData::U32(flags)));
    out
}

// ---------------------------------------------------------------------------
// [V5] Geometric conformance - the mesh measured against the input surfaces
// ---------------------------------------------------------------------------

/// One input component as [V5] sees it: its triangles and the priority that decides
/// which body wins where two overlap (lower number wins, PLAN R-A3/R-A4).
#[derive(Debug, Clone, Default)]
pub struct SurfaceComponent {
    pub priority: u32,
    pub closed: bool,
    pub tris: Vec<[Vec3; 3]>,
}

/// A uniform grid over triangles, for nearest-surface queries.
struct TriIndex {
    tris: Vec<[Vec3; 3]>,
    normals: Vec<Vec3>,
    origin: Vec3,
    cell: f64,
    dims: [i64; 3],
    buckets: Vec<Vec<u32>>,
}

// AI-FUNC-SUMMARY: Unit normal of a triangle, or None when it is degenerate; returns Option<Vec3>; side effects: none.
fn tri_normal(t: [Vec3; 3]) -> Option<Vec3> {
    let n = t[1].sub(t[0]).cross(t[2].sub(t[0]));
    let len = n.dot(n).sqrt();
    (len > 0.0).then(|| n.scale(1.0 / len))
}

// AI-FUNC-SUMMARY:
// Purpose: Squared distance from a point to a triangle (closest point on the closed triangle).
// Inputs: the point and the triangle.
// Returns: squared distance.
// Side effects: None.
// Notes: The standard region decomposition on the triangle's barycentric plane, clamped edge by
//   edge. Degenerate triangles fall through to the vertex/edge cases rather than dividing by zero.
fn point_tri_distance_sq(p: Vec3, t: [Vec3; 3]) -> f64 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let ab = b.sub(a);
    let ac = c.sub(a);
    let ap = p.sub(a);
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    let sq = |v: Vec3| v.dot(v);
    if d1 <= 0.0 && d2 <= 0.0 {
        return sq(ap);
    }
    let bp = p.sub(b);
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return sq(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let denom = d1 - d3;
        let v = if denom != 0.0 { d1 / denom } else { 0.0 };
        return sq(ap.sub(ab.scale(v)));
    }
    let cp = p.sub(c);
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return sq(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let denom = d2 - d6;
        let w = if denom != 0.0 { d2 / denom } else { 0.0 };
        return sq(ap.sub(ac.scale(w)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let denom = (d4 - d3) + (d5 - d6);
        let w = if denom != 0.0 { (d4 - d3) / denom } else { 0.0 };
        return sq(bp.sub(c.sub(b).scale(w)));
    }
    let denom = va + vb + vc;
    if denom == 0.0 {
        return sq(ap);
    }
    let v = vb / denom;
    let w = vc / denom;
    sq(ap.sub(ab.scale(v)).sub(ac.scale(w)))
}

impl TriIndex {
    // AI-FUNC-SUMMARY: Bucket triangles into a uniform grid sized from their count; returns TriIndex; side effects: none.
    fn build(tris: Vec<[Vec3; 3]>) -> TriIndex {
        let mut lo = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut hi = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for t in &tris {
            for p in t {
                lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
        if tris.is_empty() {
            lo = Vec3::new(0.0, 0.0, 0.0);
            hi = Vec3::new(1.0, 1.0, 1.0);
        }
        let span = hi.sub(lo);
        let longest = span.x.max(span.y).max(span.z).max(f64::MIN_POSITIVE);
        let side = ((tris.len() as f64).cbrt().ceil().max(1.0) as i64).clamp(1, 96);
        let cell = longest / side as f64;
        let dim = |s: f64| ((s / cell).ceil() as i64 + 1).clamp(1, side);
        let dims = [dim(span.x), dim(span.y), dim(span.z)];
        let normals = tris
            .iter()
            .map(|t| tri_normal(*t).unwrap_or(Vec3::new(0.0, 0.0, 1.0)))
            .collect();
        let mut index = TriIndex {
            tris,
            normals,
            origin: lo,
            cell,
            dims,
            buckets: vec![Vec::new(); (dims[0] * dims[1] * dims[2]) as usize],
        };
        for i in 0..index.tris.len() {
            let t = index.tris[i];
            let mut tlo = t[0];
            let mut thi = t[0];
            for p in &t {
                tlo = Vec3::new(tlo.x.min(p.x), tlo.y.min(p.y), tlo.z.min(p.z));
                thi = Vec3::new(thi.x.max(p.x), thi.y.max(p.y), thi.z.max(p.z));
            }
            for key in index.cells_in(tlo, thi) {
                index.buckets[key].push(i as u32);
            }
        }
        index
    }

    // AI-FUNC-SUMMARY: Bucket keys overlapping an AABB; returns Vec<usize>; side effects: none.
    fn cells_in(&self, lo: Vec3, hi: Vec3) -> Vec<usize> {
        let ax = |v: f64, o: f64, d: i64| (((v - o) / self.cell).floor() as i64).clamp(0, d - 1);
        let l = [
            ax(lo.x, self.origin.x, self.dims[0]),
            ax(lo.y, self.origin.y, self.dims[1]),
            ax(lo.z, self.origin.z, self.dims[2]),
        ];
        let h = [
            ax(hi.x, self.origin.x, self.dims[0]),
            ax(hi.y, self.origin.y, self.dims[1]),
            ax(hi.z, self.origin.z, self.dims[2]),
        ];
        let mut out = Vec::new();
        for z in l[2]..=h[2] {
            for y in l[1]..=h[1] {
                for x in l[0]..=h[0] {
                    out.push((z * self.dims[1] * self.dims[0] + y * self.dims[0] + x) as usize);
                }
            }
        }
        out
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Nearest triangle to a point, as (distance, triangle index).
    // Side effects: None.
    // Notes: Rings outward one grid shell at a time and stops only once the best distance found is
    //   inside the shell already searched, so the answer is exact and not "whatever the first
    //   non-empty bucket held".
    fn nearest(&self, p: Vec3) -> Option<(f64, usize)> {
        if self.tris.is_empty() {
            return None;
        }
        let ax = |v: f64, o: f64, d: i64| (((v - o) / self.cell).floor() as i64).clamp(0, d - 1);
        let c = [
            ax(p.x, self.origin.x, self.dims[0]),
            ax(p.y, self.origin.y, self.dims[1]),
            ax(p.z, self.origin.z, self.dims[2]),
        ];
        let max_ring = self.dims[0].max(self.dims[1]).max(self.dims[2]);
        let mut best = f64::INFINITY;
        let mut best_tri = 0usize;
        let mut ring = 0i64;
        while ring <= max_ring {
            let mut touched = false;
            for z in (c[2] - ring)..=(c[2] + ring) {
                if z < 0 || z >= self.dims[2] {
                    continue;
                }
                for y in (c[1] - ring)..=(c[1] + ring) {
                    if y < 0 || y >= self.dims[1] {
                        continue;
                    }
                    for x in (c[0] - ring)..=(c[0] + ring) {
                        if x < 0 || x >= self.dims[0] {
                            continue;
                        }
                        let on_shell = ring == 0
                            || (x - c[0]).abs() == ring
                            || (y - c[1]).abs() == ring
                            || (z - c[2]).abs() == ring;
                        if !on_shell {
                            continue;
                        }
                        touched = true;
                        let key = (z * self.dims[1] * self.dims[0] + y * self.dims[0] + x) as usize;
                        for t in &self.buckets[key] {
                            let d = point_tri_distance_sq(p, self.tris[*t as usize]);
                            if d < best {
                                best = d;
                                best_tri = *t as usize;
                            }
                        }
                    }
                }
            }
            // One extra ring past the first hit: the true nearest can sit just outside
            // the shell that produced the current best.
            if best.is_finite() && (ring as f64) * self.cell > best.sqrt() {
                break;
            }
            if !touched && ring > max_ring {
                break;
            }
            ring += 1;
        }
        best.is_finite().then(|| (best.sqrt(), best_tri))
    }
}

// AI-FUNC-SUMMARY:
// Purpose: [V5] geometric conformance - measure the cut surface the mesher produced against the
//   input surfaces it was asked to follow.
// Inputs: the mesh view, the input triangles, the gates, and the report item cap.
// Returns: the [V5] section.
// Side effects: None.
// Notes: Two directions, and they fail differently, which is the whole point of measuring both.
//   **mesh -> input** catches an interface that drifted off the surface; it is bounded by S7's snap
//   and is usually small. **input -> mesh** catches material the mesher *lost* - a patch of input
//   surface with no interface face near it - and that is the direction a chamfered or fragmented
//   result shows up in, the one `[V3]` and `[V4]` both pass. A mesh can be perfectly conforming,
//   perfectly shaped, and simply not be the geometry that was asked for.
//
//   Distances are reported both absolutely and as a fraction of the mesh bbox diagonal, so the
//   numbers are scale-invariant (PLAN §15's convention). The gate is on the *relative* figure.
fn check_v5(
    view: &MeshView,
    surfaces: &[SurfaceComponent],
    gates: &VerifyGates,
    cap: usize,
) -> VerifySection {
    let mut s = VerifySection::new("V5", "Geometric conformance");
    if surfaces.is_empty() {
        s.status = CheckStatus::Skipped;
        s.skipped_reason =
            Some("no input surfaces supplied; pass them as `surfaces:` in the config".to_string());
        return s;
    }
    let kinds = field_i64(view.doc, "FaceTagKind");
    let mut interface: Vec<[u32; 3]> = Vec::new();
    for (slot, &c) in view.faces.iter().enumerate() {
        let n = view.doc.cell(c);
        if n.len() != 3 {
            continue;
        }
        // Box caps are the domain boundary, not input geometry; they have no counterpart.
        if kinds.get(slot).copied() == Some(2) {
            continue;
        }
        interface.push([n[0] as u32, n[1] as u32, n[2] as u32]);
    }
    if interface.is_empty() {
        s.status = CheckStatus::Skipped;
        s.skipped_reason = Some("the mesh carries no tagged interface faces to compare".to_string());
        return s;
    }

    let per_component: Vec<TriIndex> = surfaces
        .iter()
        .map(|c| TriIndex::build(c.tris.clone()))
        .collect();
    let all: Vec<[Vec3; 3]> = surfaces.iter().flat_map(|c| c.tris.clone()).collect();
    let input = TriIndex::build(all.clone());
    let diag = view.diag.max(f64::MIN_POSITIVE);

    // --- [V5].1 interface-node distance, relative to LOCAL interface edge length ---
    //
    // The denominator is the contract's (PLAN §15) and the reference implementation's
    // [4]: the interface edge length at that node, not the bounding-box diagonal. It has
    // to be local, because the whole question is "did the cut land on the surface at the
    // resolution used *here*" - and the mesh is graded, so one global denominator makes a
    // well-fitted node in a fine region look identical to a badly-fitted one in a coarse
    // region. Normalising by the diagonal understated a 3x-refined region by exactly the
    // refinement factor.
    let mut edge_sum: std::collections::BTreeMap<u32, (f64, usize)> = std::collections::BTreeMap::new();
    for f in &interface {
        for k in 0..3 {
            let (a, b) = (f[k], f[(k + 1) % 3]);
            let len = view.doc.points[a as usize]
                .sub(view.doc.points[b as usize])
                .dot(view.doc.points[a as usize].sub(view.doc.points[b as usize]))
                .sqrt();
            for node in [a, b] {
                let e = edge_sum.entry(node).or_insert((0.0, 0));
                e.0 += len;
                e.1 += 1;
            }
        }
    }
    // Bins are the reference implementation's [4]: <1, 1-5, 5-10, 10-25, 25-50, 50-100, >100 %.
    const BIN_EDGES: [f64; 6] = [0.01, 0.05, 0.10, 0.25, 0.50, 1.00];
    let mut bins = [0usize; 7];
    let mut rel_max = 0.0f64;
    let mut abs_max = 0.0f64;
    let mut rel_sum = 0.0f64;
    let mut worst: Vec<(f64, Vec3)> = Vec::new();
    for (node, (sum, count)) in &edge_sum {
        let p = view.doc.points[*node as usize];
        let Some((d, _)) = input.nearest(p) else {
            continue;
        };
        let local = if *count > 0 { sum / *count as f64 } else { diag };
        let rel = if local > 0.0 { d / local } else { 0.0 };
        abs_max = abs_max.max(d);
        rel_max = rel_max.max(rel);
        rel_sum += rel;
        let mut slot = BIN_EDGES.len();
        for (i, edge) in BIN_EDGES.iter().enumerate() {
            if rel < *edge {
                slot = i;
                break;
            }
        }
        bins[slot] += 1;
        if rel >= 0.10 {
            worst.push((rel, p));
        }
    }
    let nodes = edge_sum.len().max(1);
    let over_10 = bins[3] + bins[4] + bins[5] + bins[6];
    let over_10_share = over_10 as f64 / nodes as f64;

    // --- [V5].2 interface face-normal deviation, orientation-agnostic ---
    let mut dev30 = 0usize;
    let mut dev45 = 0usize;
    let mut dev60 = 0usize;
    for f in &interface {
        let tri = [
            view.doc.points[f[0] as usize],
            view.doc.points[f[1] as usize],
            view.doc.points[f[2] as usize],
        ];
        let centroid = tri[0].add(tri[1]).add(tri[2]).scale(1.0 / 3.0);
        if let (Some(n), Some((_, t))) = (tri_normal(tri), input.nearest(centroid)) {
            let cos = n.dot(input.normals[t]).abs().clamp(0.0, 1.0);
            let deg = cos.acos().to_degrees();
            if deg > 30.0 {
                dev30 += 1;
            }
            if deg > 45.0 {
                dev45 += 1;
            }
            if deg > 60.0 {
                dev60 += 1;
            }
        }
    }

    // --- [V5].3 the reverse direction: input surface with no interface near it ---
    //
    // Not part of the reference implementation's report, and added here because it is the
    // only direction that catches material the mesher *lost* - a chamfered or fragmented
    // result passes [V1]-[V4] and the forward direction untouched. It is reported as INFO,
    // not gated, for two reasons: it is sampled (vertices + centroid per input triangle),
    // so a coarse input triangle reports its own sampling error; and a patch of surface
    // that a higher-priority body contains is *supposed* to be absent (R-A3/R-A4), which
    // is masked below rather than counted as loss.
    let mesh_tris: Vec<[Vec3; 3]> = interface
        .iter()
        .map(|f| {
            [
                view.doc.points[f[0] as usize],
                view.doc.points[f[1] as usize],
                view.doc.points[f[2] as usize],
            ]
        })
        .collect();
    let mesh_index = TriIndex::build(mesh_tris);
    let mut lost_max = 0.0f64;
    let mut overridden = 0usize;
    let mut lost_items: Vec<(f64, Vec3)> = Vec::new();
    for (index, component) in surfaces.iter().enumerate() {
        for tri in &component.tris {
            let centroid = tri[0].add(tri[1]).add(tri[2]).scale(1.0 / 3.0);
            for p in tri.iter().chain(std::iter::once(&centroid)) {
                let masked = surfaces.iter().enumerate().any(|(other, rival)| {
                    other != index
                        && rival.closed
                        && rival.priority < component.priority
                        && per_component[other].contains(*p)
                });
                if masked {
                    overridden += 1;
                    continue;
                }
                if let Some((d, _)) = mesh_index.nearest(*p) {
                    if d > lost_max {
                        lost_max = d;
                    }
                    lost_items.push((d, *p));
                }
            }
        }
    }

    // --- [V5].4 per-component volume: did the right material get the right amount? ---
    //
    // The interface can sit exactly on the surface and the mesh still be wrong, because
    // nothing above asks *which side of it got which material*. A feature thinner than
    // the sizing floor is the case that matters: S6 classifies by per-vertex parity, so
    // a body no lattice vertex ever lands inside forms no region at all - the surface
    // around it is perfect and its material is simply absent. Measured on a 0.0180-thick
    // limb against `h_min = 0.0208`: 2 region keys where there should be 3, every other
    // check passing.
    //
    // A body a *higher-priority* body contains is supposed to have no volume (R-A3/R-A4),
    // so it is masked here exactly as the reverse-distance direction masks it.
    let set_offsets = field_i64(view.doc, "RegionSetOffsets");
    let set_components = field_i64(view.doc, "RegionSetComponents");
    let region_key = cell_i64(view.doc, "region_key").unwrap_or_default();
    let mut meshed: std::collections::BTreeMap<i32, f64> = std::collections::BTreeMap::new();
    if !set_offsets.is_empty() {
        for (slot, &c) in view.tets.iter().enumerate() {
            let key = region_key.get(c).copied().unwrap_or(-1);
            if key < 0 {
                continue;
            }
            let k = key as usize;
            if k >= set_offsets.len() {
                continue;
            }
            let start = if k == 0 { 0 } else { set_offsets[k - 1] as usize };
            let end = set_offsets[k] as usize;
            let n = view.doc.cell(c);
            if n.len() != 4 {
                continue;
            }
            let p: Vec<Vec3> = n.iter().map(|i| view.doc.points[*i as usize]).collect();
            let vol = p[1]
                .sub(p[0])
                .cross(p[2].sub(p[0]))
                .dot(p[3].sub(p[0]))
                .abs()
                / 6.0;
            let _ = slot;
            for x in &set_components[start.min(set_components.len())..end.min(set_components.len())]
            {
                if *x != 0 {
                    *meshed.entry(*x as i32).or_insert(0.0) += vol;
                }
            }
        }
    }
    let mut volume_losses = 0usize;
    for (index, component) in surfaces.iter().enumerate() {
        // Signed volume of the closed input surface, by the divergence theorem.
        let stl: f64 = component
            .tris
            .iter()
            .map(|t| t[0].cross(t[1]).dot(t[2]) / 6.0)
            .sum::<f64>()
            .abs();
        if !component.closed || stl <= 0.0 {
            continue;
        }
        // Which X this component is: the surfaces are listed in `meshgen.inputs` order
        // and components are numbered from 1 in that order.
        let x = index as i32 + 1;
        let got = meshed.get(&x).copied().unwrap_or(0.0);
        let centroid = {
            let mut c = Vec3::new(0.0, 0.0, 0.0);
            for t in &component.tris {
                c = c.add(t[0].add(t[1]).add(t[2]).scale(1.0 / 3.0));
            }
            c.scale(1.0 / component.tris.len().max(1) as f64)
        };
        let masked = surfaces.iter().enumerate().any(|(other, rival)| {
            other != index
                && rival.closed
                && rival.priority < component.priority
                && per_component[other].contains(centroid)
        });
        s.metric(&format!("component_{x}_volume_input"), stl);
        s.metric(&format!("component_{x}_volume_meshed"), got);
        if masked {
            continue;
        }
        // The *expected* volume is not the input volume when bodies overlap: a
        // higher-priority body wins where they meet (R-A3/R-A4), so component X is
        // entitled only to the part of itself no higher-priority body covers. Comparing
        // against the raw input volume reported a sphere-overlapped cube as 28.9% short
        // when almost all of that is the priority rule doing its job. Estimated by
        // stratified point sampling, which is enough resolution for a 10% gate and needs
        // no boolean geometry.
        // The divergence-theorem sum is exact only for a surface that does not intersect
        // itself. A component built as a *union of overlapping parts* - a strut lattice,
        // a bolted assembly exported as one body - has every overlap region counted once
        // per part, so `stl` is the sum of the parts while the mesh correctly contains the
        // union. On the 15-box lattice that gap alone read as **12 %** of "lost" volume at
        // every resolution - a plateau that looked like a mesher defect and was entirely
        // the denominator, constant by construction because the overlap does not shrink
        // with `h`.
        //
        // The sampled estimate is union-correct (winding number, per shell), so it is the
        // only sound basis once self-intersection is detected. It stays a fallback rather
        // than the default because it is far too coarse to replace the exact sum on clean
        // input - ~0.4 % at this sample count, against errors of that same order.
        let sampled = sampled_volume(&per_component[index], &component.tris);
        s.metric(&format!("component_{x}_volume_sampled"), sampled);
        let self_intersecting = sampled > 0.0 && (stl - sampled).abs() / sampled > 0.05;
        if self_intersecting {
            s.push(
                VerifyItem::bare(
                    Severity::Warn,
                    "V5.self_intersecting_input",
                    format!(
                        "component {x}: the input's divergence-theorem volume {stl:.6e} disagrees \
                         with a union-correct sampled estimate {sampled:.6e} by {:.1}%, so the \
                         surface intersects itself and its exact volume is the sum of its \
                         overlapping parts, not the union it encloses. `volume_expected` and \
                         `volume_error` for this component are computed from the sampled \
                         estimate instead, and so carry its ~0.5% sampling noise.",
                        (stl - sampled).abs() / sampled * 100.0
                    ),
                ),
                cap,
            );
        }
        let basis = if self_intersecting { sampled } else { stl };
        let expected = expected_volume(surfaces, &per_component, index, basis);
        s.metric(&format!("component_{x}_volume_expected"), expected);
        if expected <= 0.0 {
            continue;
        }
        let error = (got - expected).abs() / expected;
        s.metric(&format!("component_{x}_volume_error"), error);
        if got <= 0.0 {
            volume_losses += 1;
            s.push(
                VerifyItem::bare(
                    Severity::Fail,
                    "V5.component_volume_lost",
                    format!(
                        "component {x} has input volume {stl:.6e} but owns no element in the mesh - \
                         its material is absent. The usual cause is a feature thinner than the \
                         sizing floor: S6 classifies by per-vertex parity, so a body no lattice \
                         vertex lands inside never forms a region."
                    ),
                ),
                cap,
            );
        } else if error > 0.10 {
            volume_losses += 1;
            s.push(
                VerifyItem::bare(
                    Severity::Warn,
                    "V5.component_volume_error",
                    format!(
                        "component {x} meshed volume {got:.6e} differs from its priority-resolved \
                         expected volume {expected:.6e} (raw input {stl:.6e}) by {:.2}%",
                        error * 100.0
                    ),
                ),
                cap,
            );
        }
    }
    s.metric("component_volume_failures", volume_losses as f64);

    // Cross-check on this check itself: the sum over *every* tet, independent of region
    // attribution. If that matches the mesh's own hull volume while the per-component
    // figures do not add up, the defect is in attribution; if it does not match, the
    // per-component figures are measuring a mesh that is already short and the fault is
    // upstream. Reported always, because a checker whose bias has never been bounded is
    // not evidence.
    let mut total = 0.0f64;
    for &c in &view.tets {
        let n = view.doc.cell(c);
        if n.len() != 4 {
            continue;
        }
        let p: Vec<Vec3> = n.iter().map(|i| view.doc.points[*i as usize]).collect();
        total += p[1]
            .sub(p[0])
            .cross(p[2].sub(p[0]))
            .dot(p[3].sub(p[0]))
            .abs()
            / 6.0;
    }
    s.metric("mesh_total_volume", total);
    // Which cells are *misattributed*: centroid strictly inside a component, region set
    // not naming it. This is the defect `mesh_total_volume` proves must exist whenever a
    // component's meshed volume is short while the mesh itself is complete - the material
    // is present and wearing the wrong label. Counting it directly turns a volume
    // discrepancy into a list of cells someone can look at.
    let mut misattributed = 0usize;
    let mut misattributed_volume = 0.0f64;
    // The worst offenders, so the finding names places rather than a number. Ranked by the
    // cell's own volume: the biggest misattributed element is the one worth looking at.
    let mut worst_misattributed: Vec<(f64, Vec3, i32)> = Vec::new();
    for (index, component) in surfaces.iter().enumerate() {
        if !component.closed {
            continue;
        }
        let x = index as i32 + 1;
        for &c in &view.tets {
            let n = view.doc.cell(c);
            if n.len() != 4 {
                continue;
            }
            let key = region_key.get(c).copied().unwrap_or(-1);
            let named = if key < 0 || set_offsets.is_empty() {
                false
            } else {
                let k = key as usize;
                let start = if k == 0 { 0 } else { set_offsets[k - 1] as usize };
                let end = set_offsets[k].min(set_components.len() as i64) as usize;
                set_components[start.min(set_components.len())..end].contains(&(x as i64))
            };
            if named {
                continue;
            }
            let p: Vec<Vec3> = n.iter().map(|i| view.doc.points[*i as usize]).collect();
            let centroid = p[0].add(p[1]).add(p[2]).add(p[3]).scale(0.25);
            if per_component[index].contains(centroid) {
                misattributed += 1;
                let volume = p[1]
                    .sub(p[0])
                    .cross(p[2].sub(p[0]))
                    .dot(p[3].sub(p[0]))
                    .abs()
                    / 6.0;
                misattributed_volume += volume;
                worst_misattributed.push((volume, centroid, x));
            }
        }
    }
    s.metric("misattributed_cells", misattributed as f64);
    s.metric("misattributed_volume", misattributed_volume);
    s.metric(
        "misattributed_volume_share",
        if meshed.values().sum::<f64>() > 0.0 {
            misattributed_volume / meshed.values().sum::<f64>()
        } else {
            0.0
        },
    );
    s.metric("meshed_sum_over_components", meshed.values().sum::<f64>());
    s.metric("interface_nodes", nodes as f64);
    s.metric("rel_distance_max", rel_max);
    s.metric("rel_distance_mean", rel_sum / nodes as f64);
    s.metric("abs_distance_max", abs_max);
    s.metric("bin_under_1pct", bins[0] as f64);
    s.metric("bin_1_to_5pct", bins[1] as f64);
    s.metric("bin_5_to_10pct", bins[2] as f64);
    s.metric("bin_10_to_25pct", bins[3] as f64);
    s.metric("bin_25_to_50pct", bins[4] as f64);
    s.metric("bin_50_to_100pct", bins[5] as f64);
    s.metric("bin_over_100pct", bins[6] as f64);
    s.metric("over_10pct_share", over_10_share);
    s.metric("normal_dev_over_30deg", dev30 as f64);
    s.metric("normal_dev_over_45deg", dev45 as f64);
    s.metric("normal_dev_over_60deg", dev60 as f64);
    s.metric("normal_dev_over_60deg_share", dev60 as f64 / interface.len() as f64);
    s.metric("input_to_interface_max", lost_max);
    s.metric("input_to_interface_max_frac", lost_max / diag);
    s.metric("samples_overridden_by_priority", overridden as f64);
    s.metric("input_triangles", all.len() as f64);
    s.metric("interface_faces", interface.len() as f64);

    // Gates, all from the contract: max relative distance, the >10% share, and the
    // >60-degree normal share. `surface_distance_frac` overrides the max-distance gate.
    let mut fired = false;
    // **Reported, not just measured.** The metric has been computed since G7-3 and never
    // surfaced as a finding, which is exactly why the suite is blind to this failure mode:
    // the nodes are right, the mesh is watertight and conforming, `[V1]`/`[V2]`/`[V3]` all
    // pass, and the only symptom is that elements at a body's edges and corners carry the
    // background label. The aggregate volume error can stay under its gate while the
    // silhouette is visibly notched, because the deficit is a thin shell over a large area.
    //
    // It has to sit *here*, among the gates, because `check_v5` ends with
    // `if !fired { s.status = Pass }` - an unconditional reset that silently discards any
    // finding pushed earlier in the function. Reporting it above the gates emitted the WARN
    // lines into the log while the section header still read PASS.
    if misattributed > 0 {
        fired = true;
        s.push(
            VerifyItem::bare(
                Severity::Warn,
                "V5.misattributed_material",
                format!(
                    "{misattributed} cell(s) have a centroid strictly inside a component and do \
                     not carry it, totalling {misattributed_volume:.6e} of material; the element \
                     exists and the mesh is conforming, so the region label is what is wrong"
                ),
            ),
            cap,
        );
        worst_misattributed
            .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (volume, centroid, x) in worst_misattributed.iter().take(cap.min(8)) {
            let mut item = VerifyItem::bare(
                Severity::Warn,
                "V5.misattributed_material",
                format!(
                    "a cell of volume {volume:.4e} sits inside component {x} and is labelled \
                     without it"
                ),
            );
            item.coordinates.push(*centroid);
            s.push(item, cap);
        }
    }
    if rel_max > gates.surface_distance_frac {
        fired = true;
        worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (rel, p) in worst.iter().take(cap) {
            let mut item = VerifyItem::bare(
                Severity::Warn,
                "V5.interface_off_surface",
                format!(
                    "interface node is {:.1}% of its local interface edge length from the nearest input surface",
                    rel * 100.0
                ),
            );
            item.coordinates.push(*p);
            s.push(item, cap);
        }
    }
    if over_10_share > 0.05 {
        fired = true;
        s.push(
            VerifyItem::bare(
                Severity::Warn,
                "V5.interface_distance_distribution",
                format!(
                    "{:.2}% of interface nodes are more than 10% of a local edge from the surface (gate 5%)",
                    over_10_share * 100.0
                ),
            ),
            cap,
        );
    }
    if dev60 as f64 / interface.len() as f64 > 0.05 {
        fired = true;
        s.push(
            VerifyItem::bare(
                Severity::Warn,
                "V5.normal_deviation",
                format!(
                    "{dev60} interface face(s) ({:.2}%) deviate more than 60 deg from the surface they sit on (gate 5%)",
                    dev60 as f64 / interface.len() as f64 * 100.0
                ),
            ),
            cap,
        );
    }
    if volume_losses > 0 {
        fired = true;
    }
    if !fired {
        s.status = CheckStatus::Pass;
    }
    s
}

impl TriIndex {
    // AI-FUNC-SUMMARY:
    // Purpose: Whether a point is strictly inside this closed surface, by ray-crossing parity.
    // Inputs: the query point.
    // Returns: true when the +X ray crosses an odd number of triangles.
    // Side effects: None.
    // Notes: [V5] needs this for one job only - deciding whether a patch of input surface was
    //   *legitimately* removed because a higher-priority body contains it. A fixed +X ray is enough
    //   at that job's tolerance - *provided the ray is not axis-aligned*. A +X ray grazes every
    //   face of an axis-aligned box and counts those hits inconsistently; see the direction below.
    fn contains(&self, p: Vec3) -> bool {
        // Generalized winding number, not ray parity. Parity counts crossings, so on a
        // surface that intersects itself - a lattice built as overlapping struts, an
        // assembly exported as one body - a ray through an overlap picks up the extra
        // shells and the answer flips. That is the same defect S6 needs the winding
        // number for, and it made this function read 0.01288 against an analytic union
        // of 0.01787 on the strut lattice.
        //
        // Solid angle per triangle by Van Oosterom-Strackee, summed and normalised. A
        // point inside k overlapping shells has winding k, so the test is `>= 0.5`
        // rather than `== 1`, which is exactly what makes a union measurable.
        let mut total = 0.0f64;
        for tri in &self.tris {
            let a = tri[0].sub(p);
            let b = tri[1].sub(p);
            let c = tri[2].sub(p);
            let (la, lb, lc) = (
                a.dot(a).sqrt(),
                b.dot(b).sqrt(),
                c.dot(c).sqrt(),
            );
            if la == 0.0 || lb == 0.0 || lc == 0.0 {
                return true; // the point is a vertex of the surface
            }
            let numerator = a.dot(b.cross(c));
            let denominator =
                la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
            total += 2.0 * numerator.atan2(denominator);
        }
        (total / (4.0 * std::f64::consts::PI)).abs() >= 0.5
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The volume component `index` is entitled to once higher-priority bodies have taken their
//   share (R-A3/R-A4).
// Inputs: every component, their triangle indices, the component of interest, and its raw volume.
// Returns: the priority-resolved expected volume.
// Side effects: None.
// Notes: Stratified sampling over the component's bounding box rather than boolean geometry: the
//   gate it feeds is 10%, and a deterministic lattice of samples is reproducible run to run, which
//   a random one would not be (R-P2). When no higher-priority body overlaps at all the raw volume is
//   returned untouched, so the common single-body case costs nothing and stays exact.
fn expected_volume(
    surfaces: &[SurfaceComponent],
    indices: &[TriIndex],
    index: usize,
    raw: f64,
) -> f64 {
    let me = &surfaces[index];
    let rivals: Vec<usize> = surfaces
        .iter()
        .enumerate()
        .filter(|(other, r)| *other != index && r.closed && r.priority < me.priority)
        .map(|(other, _)| other)
        .collect();
    if rivals.is_empty() {
        return raw;
    }
    let mut lo = me.tris[0][0];
    let mut hi = me.tris[0][0];
    for t in &me.tris {
        for p in t {
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
    }
    let span = hi.sub(lo);
    const N: usize = 48;
    let mut inside_me = 0usize;
    let mut inside_and_free = 0usize;
    for i in 0..N {
        for j in 0..N {
            for k in 0..N {
                let p = Vec3::new(
                    lo.x + span.x * (i as f64 + 0.5) / N as f64,
                    lo.y + span.y * (j as f64 + 0.5) / N as f64,
                    lo.z + span.z * (k as f64 + 0.5) / N as f64,
                );
                if !indices[index].contains(p) {
                    continue;
                }
                inside_me += 1;
                if !rivals.iter().any(|r| indices[*r].contains(p)) {
                    inside_and_free += 1;
                }
            }
        }
    }
    if inside_me == 0 {
        return raw;
    }
    raw * inside_and_free as f64 / inside_me as f64
}


// AI-FUNC-SUMMARY:
// Purpose: Union-correct volume estimate of one component, sampling each connected shell separately.
// Inputs: the component's triangles and an index over all of them.
// Returns: the estimate; 0 when there is nothing to sample.
// Side effects: None.
// Notes: Sampling the *component's* bounding box is what made the first version useless on a strut
//   lattice: a 0.05 strut inside a 0.5366 box gets ~4.5 samples across at any affordable count, and
//   a thin feature at that density is systematically under-counted (measured 19% low). Splitting
//   into connected shells first and sampling each shell's own bounds puts the full sample budget
//   across the feature instead - 48 samples across the same strut rather than 4.5.
//
//   Overlaps are counted once by charging each sample to the *first* shell that contains it, so the
//   result is the union rather than the sum of parts, which is the whole point. Deterministic
//   lattice, so the figure is reproducible run to run (R-P2).
fn sampled_volume(index: &TriIndex, tris: &[[Vec3; 3]]) -> f64 {
    let shells = connected_shells(tris);
    const N: usize = 48;
    let mut volume = 0.0f64;
    for (order, shell) in shells.iter().enumerate() {
        let mut lo = tris[shell[0]][0];
        let mut hi = tris[shell[0]][0];
        for t in shell {
            for p in &tris[*t] {
                lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
        let span = hi.sub(lo);
        if span.x <= 0.0 || span.y <= 0.0 || span.z <= 0.0 {
            continue;
        }
        let own = TriIndex::build(shell.iter().map(|t| tris[*t]).collect());
        let earlier: Vec<TriIndex> = shells[..order]
            .iter()
            .map(|s| TriIndex::build(s.iter().map(|t| tris[*t]).collect()))
            .collect();
        let mut inside = 0usize;
        for i in 0..N {
            for j in 0..N {
                for k in 0..N {
                    let p = Vec3::new(
                        lo.x + span.x * (i as f64 + 0.5) / N as f64,
                        lo.y + span.y * (j as f64 + 0.5) / N as f64,
                        lo.z + span.z * (k as f64 + 0.5) / N as f64,
                    );
                    if own.contains(p) && !earlier.iter().any(|e| e.contains(p)) {
                        inside += 1;
                    }
                }
            }
        }
        volume += span.x * span.y * span.z * inside as f64 / (N * N * N) as f64;
    }
    let _ = index;
    volume
}

// AI-FUNC-SUMMARY: Split a triangle soup into connected shells by shared vertex position; returns one index list per shell; side effects: none.
fn connected_shells(tris: &[[Vec3; 3]]) -> Vec<Vec<usize>> {
    let key = |p: Vec3| {
        (
            (p.x * 1.0e9).round() as i64,
            (p.y * 1.0e9).round() as i64,
            (p.z * 1.0e9).round() as i64,
        )
    };
    let mut at_vertex: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (index, tri) in tris.iter().enumerate() {
        for p in tri {
            at_vertex.entry(key(*p)).or_default().push(index);
        }
    }
    let mut seen = vec![false; tris.len()];
    let mut out: Vec<Vec<usize>> = Vec::new();
    for start in 0..tris.len() {
        if seen[start] {
            continue;
        }
        let mut shell = Vec::new();
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(t) = stack.pop() {
            shell.push(t);
            for p in &tris[t] {
                if let Some(neighbours) = at_vertex.get(&key(*p)) {
                    for n in neighbours {
                        if !seen[*n] {
                            seen[*n] = true;
                            stack.push(*n);
                        }
                    }
                }
            }
        }
        shell.sort_unstable();
        out.push(shell);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // The predicate's semantics, stated case by case. It is per component, not per count:
    // that is the whole difference between it and `[V6]`'s region-adjacency rule.
    #[test]
    fn undeclared_components_is_per_component_not_per_count() {
        let tagged = |xs: &[i64]| -> BTreeSet<i64> { xs.iter().copied().collect() };

        // Nothing changes across the face - no boundary, declared or not.
        assert!(undeclared_components(&[1], &[1], None).is_empty());
        assert!(undeclared_components(&[], &[], None).is_empty());

        // A material boundary across a face carrying no tag at all. This is the case
        // `[V6]` passes and the metric exists for.
        assert_eq!(undeclared_components(&[1], &[], None), vec![1]);
        assert_eq!(undeclared_components(&[], &[2], None), vec![2]);

        // Declared for the component that changes - the ordinary, correct interface.
        assert!(undeclared_components(&[1], &[], Some(&tagged(&[1]))).is_empty());

        // Declared, but for the wrong component: still undeclared for the one that changes.
        assert_eq!(undeclared_components(&[1], &[], Some(&tagged(&[2]))), vec![1]);

        // A two-component step declared for only one of them reports just the other, so the
        // count is of components missing a declaration rather than of faces failing a size rule.
        assert_eq!(
            undeclared_components(&[1, 2], &[], Some(&tagged(&[1]))),
            vec![2]
        );
        assert!(undeclared_components(&[1, 2], &[], Some(&tagged(&[1, 2]))).is_empty());

        // Direction does not matter: entering and leaving are the same boundary.
        assert_eq!(
            undeclared_components(&[1, 3], &[3], None),
            undeclared_components(&[3], &[1, 3], None)
        );
    }

    #[test]
    fn face_area_matches_a_known_triangle() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 3.0, 0.0),
        ];
        assert!((face_area(&points, &[0, 1, 2]) - 3.0).abs() < 1e-12);
        // An out-of-range node id yields 0.0 rather than panicking the verifier.
        assert_eq!(face_area(&points, &[0, 1, 99]), 0.0);
    }
}
