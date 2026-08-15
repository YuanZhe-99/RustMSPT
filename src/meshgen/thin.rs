//! S8b - thin regimes: the band templates, the rim collapse, and the FEM-aware
//! ladder (G7-1).
//!
//! S8 cuts a cell wherever *one* active patch passes through it. A **band region**
//! is the case that defeats it: two walls of one gap pass through the same cell,
//! closer to each other than the cell is wide, so the generic path sees a junction
//! and hands the cell to the conforming fan - which is valid, but chamfers the gap
//! away. A band region is meshed instead by a single layer of elements spanning the
//! gap, built from matched wall-vertex pairs rather than from the lattice.
//!
//! Two frozen sources govern this module:
//!
//! - **`SPEC_meshgen_geometry.md` §8** - the `k = 0..3` template table, Invariant B1
//!   (collapse is a property of the vertex *pair*, so neighbouring cells with
//!   different `k` still agree on their shared quads), and the referral of every
//!   emitted tet to the §4.4 runtime ladder;
//! - **`PLAN_mesh_generation.md` §10.11** - the FEM-aware ladder, which decides
//!   *whether* a region is meshed as a band at all, from element quality predicted
//!   before anything is meshed.
//!
//! The one structure the whole module rests on is a band cell: three matched pairs
//! `(a_i, b_i)`, each either surviving or collapsed to a single rim node `r_i`. Its
//! **boundary** is a pure function of those three pairs and of one diagonal flag per
//! pair-edge - cap A, cap B, and one quad per pair-edge, with collapsed vertices
//! substituted - and every template, the Steiner fallback and the volume check are
//! derived from that one construction. That is what makes a band layer conform to
//! itself with no negotiation: two cells sharing a pair-edge build the same quad
//! from the same nodes under the same rule, and two cells sharing a pair see the
//! same collapse.
//!
//! **The one place negotiation is unavoidable is the §4.4 ladder's flip step**, and
//! it is handled here rather than skipped: a diagonal flip is not a pure function of
//! the quad, so it is decided by `mesh_band_layer` for *both* cells incident to the
//! quad at once - which is what the reference thin-feature design §3.8 means by
//! "attempted only pairwise" - and rejected unless both cells accept it.

use crate::meshgen::cut::{
    orient_positively, prism_tets_with_diagonals, snk_diagonal_is_02, snk_split_quad, NodeKey,
};
use crate::meshgen::predicates::{tet_quality, tet_signed_volume};
use crate::types::Vec3;
use rayon::prelude::*;
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet};

/// The §8.2 / §4.4 runtime dihedral floor for a band tet, in degrees.
pub const BAND_MIN_DIHEDRAL_DEG: f64 = 8.0;

/// The §10.11 predicted-aspect-ratio gate.
pub const BAND_MAX_AR: f64 = 20.0;

/// The `explicit` FEM profile's altitude floor, as a fraction of `h_local`.
pub const BAND_EXPLICIT_ALTITUDE_RATIO: f64 = 0.05;

/// The share of a band's cells that may take the Steiner fallback before the whole
/// region is demoted to volumetric (`[THIN-SKIP]`, reference thin-feature design §3.8).
pub const BAND_REGIONAL_FAILURE_SHARE: f64 = 0.05;

/// The band cell's volume tolerance: the emitted tets must reproduce the volume
/// enclosed by the cell's own boundary triangulation to within this fraction.
pub const BAND_VOLUME_TOLERANCE: f64 = 0.01;

/// The three pair-edges of a band cell, in the order their diagonal flags are stored.
pub const BAND_EDGES: [(usize, usize); 3] = [(0, 1), (1, 2), (2, 0)];

/// One matched wall-vertex pair - the atom of a band layer.
///
/// `a` lies on wall A, `b` on wall B, and `collapsed` carries the rim node `r` when
/// the pair's separation fell below `t_sheet(x)`. Collapse is stored **here**, on
/// the pair, and not on the cell: that is Invariant B1, and it is the entire reason
/// neighbouring cells with different `k` need no negotiation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandPair {
    pub a: u32,
    pub b: u32,
    pub collapsed: Option<u32>,
}

impl BandPair {
    // AI-FUNC-SUMMARY: The node this pair contributes on wall A - the rim node once collapsed; returns u32; side effects: none.
    pub fn wall_a(&self) -> u32 {
        self.collapsed.unwrap_or(self.a)
    }

    // AI-FUNC-SUMMARY: The node this pair contributes on wall B - the rim node once collapsed; returns u32; side effects: none.
    pub fn wall_b(&self) -> u32 {
        self.collapsed.unwrap_or(self.b)
    }

    // AI-FUNC-SUMMARY: Whether this pair is collapsed to a rim node; returns bool; side effects: none.
    pub fn is_collapsed(&self) -> bool {
        self.collapsed.is_some()
    }
}

/// Per pair-edge, which diagonal its quad takes: `true` = the diagonal joining the
/// **first** pair's wall-A node to the **second** pair's wall-B node, in the edge
/// order of `BAND_EDGES`. Rule SNK computes the frozen choice; the ladder's flip
/// step is the only thing that ever overrides it.
pub type CellDiagonals = [bool; 3];

/// Which row of the frozen §8.2 table a cell took.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BandTemplate {
    /// `k = 0` - a prism, three tets.
    Prism,
    /// `k = 1` - a pyramid over the surviving quad, two tets.
    Pyramid,
    /// `k = 2` - a single tet.
    Tet,
    /// `k = 3` - no volume; the cell *is* the sheet triangle.
    Sheet,
    /// The §4.4 ladder's last rung: the cell's own boundary coned to a Steiner point.
    Steiner,
}

impl BandTemplate {
    // AI-FUNC-SUMMARY: The `regime` cell-array code of `SPEC_meshgen_contracts.md` §2.1 (1 band, 2 band-Steiner); returns u8; side effects: none.
    pub fn regime_code(&self) -> u8 {
        match self {
            BandTemplate::Steiner => 2,
            _ => 1,
        }
    }

    // AI-FUNC-SUMMARY: The template's name for reports and snapshots; returns &'static str; side effects: none.
    pub fn name(&self) -> &'static str {
        match self {
            BandTemplate::Prism => "prism",
            BandTemplate::Pyramid => "pyramid",
            BandTemplate::Tet => "tet",
            BandTemplate::Sheet => "sheet",
            BandTemplate::Steiner => "steiner",
        }
    }
}

/// Why a band cell could not be meshed by its table row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BandFailure {
    /// Two of the cell's three pairs contribute the same node: the pairs do not
    /// describe a cell.
    Degenerate,
    /// The decomposition came out cyclic - unreachable under SNK (§4.2 Theorem T2),
    /// reachable only through a flip.
    Cyclic,
    /// A tet came out inverted or exactly flat.
    Inverted,
    /// A tet came out below the §4.4 dihedral floor.
    Dihedral,
    /// The children did not reproduce the volume the cell's boundary encloses.
    Volume,
}

/// One band cell, meshed.
#[derive(Clone, Debug, PartialEq)]
pub struct BandCellMesh {
    pub tets: SmallVec<[[u32; 4]; 8]>,
    pub template: BandTemplate,
    /// The `k = 3` sheet triangle, when the cell collapsed entirely.
    pub sheet_triangle: Option<[u32; 3]>,
    /// The rung of the §4.4 ladder that produced this mesh: 0 = the table row under
    /// Rule SNK, 1 = a negotiated diagonal flip, 2 = the Steiner fan.
    pub rung: u8,
    /// The worst min-dihedral over the emitted tets, in degrees (`INFINITY` when the
    /// cell emitted none).
    pub min_dihedral_deg: f64,
    /// The worst aspect ratio over the emitted tets (0 when the cell emitted none).
    pub max_aspect_ratio: f64,
    /// The relative error between the children's volume and the boundary's.
    pub volume_error: f64,
}

/// A boundary facet list of a band cell, in emission order.
pub type BandFacets = SmallVec<[[u32; 3]; 8]>;

/// A band cell's tets, at most the eight of a Steiner fan.
pub type BandTets = SmallVec<[[u32; 4]; 8]>;

/// What the frozen §8.2 table returns: the tets, the row taken, and the `k = 3`
/// sheet triangle.
pub type BandTableRow = (BandTets, BandTemplate, Option<[u32; 3]>);

/// What a checked candidate returns: the oriented tets, the worst min dihedral, the
/// worst aspect ratio, and the relative volume error.
type CheckedCandidate = (BandTets, f64, f64, f64);

// ---------------------------------------------------------------------------
// The boundary - the one construction every template is derived from
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Rule SNK's diagonal choice on each of a band cell's three pair-edge quads.
// Inputs: the three pairs and the key table.
// Returns: the three flags, in `BAND_EDGES` order; a collapsed pair-edge's flag is unused and
//   reported as false.
// Side effects: None.
// Notes: A pure function of node keys, so both cells incident to a quad compute the same flag -
//   Invariant C in miniature, and the reason a band layer conforms without communication.
pub fn snk_cell_diagonals(pairs: [BandPair; 3], keys: &[NodeKey]) -> CellDiagonals {
    let mut diagonals = [false; 3];
    for (slot, &(i, j)) in BAND_EDGES.iter().enumerate() {
        if pairs[i].is_collapsed() || pairs[j].is_collapsed() {
            continue;
        }
        diagonals[slot] = snk_diagonal_is_02([pairs[i].a, pairs[j].a, pairs[j].b, pairs[i].b], keys);
    }
    diagonals
}

// AI-FUNC-SUMMARY: Split a quad on the requested diagonal; returns the two triangles in the quad's winding; side effects: none.
fn split_quad(quad: [u32; 4], diagonal_02: bool) -> [[u32; 3]; 2] {
    if diagonal_02 {
        [
            [quad[0], quad[1], quad[2]],
            [quad[0], quad[2], quad[3]],
        ]
    } else {
        [
            [quad[1], quad[2], quad[3]],
            [quad[1], quad[3], quad[0]],
        ]
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The closed boundary triangulation of one band cell, as a pure function of its pairs and
//   diagonal flags.
// Inputs: the three matched pairs (collapsed ones carry their rim node) and the per-edge diagonals.
// Returns: consistently oriented triangles: cap A reversed, cap B, and one quad per pair-edge - the
//   quad split on its flag, degenerating to a triangle when exactly one of its two pairs is
//   collapsed and to nothing when both are.
// Side effects: None.
// Notes: This is the module's conformity mechanism, and it is deliberately shared by the templates,
//   the Steiner fan and the volume check rather than re-derived by each. Two cells sharing a
//   pair-edge compute that edge's quad from the same two pairs under the same flag, so they agree on
//   it whatever their own `k` is (Invariant B1); the volumetric mesh on the far side of a wall sees
//   the cap triangle, which is likewise pair-determined. The winding is outward for a cell whose
//   wall-A cap `(a0,a1,a2)` winds counter-clockwise seen from wall B - the natural orientation of a
//   wall triangle whose normal points into the gap - and consistently inward otherwise, which is all
//   the volume check and the Steiner fan need.
pub fn band_cell_boundary(pairs: [BandPair; 3], diagonals: CellDiagonals) -> BandFacets {
    let mut facets: BandFacets = SmallVec::new();
    let cap_a = [pairs[0].wall_a(), pairs[1].wall_a(), pairs[2].wall_a()];
    let cap_b = [pairs[0].wall_b(), pairs[1].wall_b(), pairs[2].wall_b()];
    if !pairs.iter().all(|p| p.is_collapsed()) {
        facets.push([cap_a[0], cap_a[2], cap_a[1]]);
        facets.push(cap_b);
    }
    for (slot, &(i, j)) in BAND_EDGES.iter().enumerate() {
        let (p, q) = (pairs[i], pairs[j]);
        match (p.collapsed, q.collapsed) {
            (Some(_), Some(_)) => {}
            (Some(r), None) => facets.push([r, q.a, q.b]),
            (None, Some(r)) => facets.push([p.a, r, p.b]),
            (None, None) => {
                for triangle in split_quad([p.a, q.a, q.b, p.b], diagonals[slot]) {
                    facets.push(triangle);
                }
            }
        }
    }
    facets
}

// AI-FUNC-SUMMARY:
// Purpose: The volume enclosed by a closed, consistently oriented triangulation (divergence theorem).
// Inputs: the facets and the node table.
// Returns: the signed volume; positive when the facets wind outward.
// Side effects: None.
// Notes: This is what the band cell's dry-run measures against, and it is deliberately independent
//   of the interior decomposition - a mis-assembled table row still emits positively oriented tets,
//   and only a volume computed from the *boundary* catches it.
pub fn enclosed_volume(facets: &[[u32; 3]], nodes: &[Vec3]) -> f64 {
    let origin = Vec3::new(0.0, 0.0, 0.0);
    facets
        .iter()
        .map(|f| {
            tet_signed_volume(
                origin,
                nodes[f[0] as usize],
                nodes[f[1] as usize],
                nodes[f[2] as usize],
            )
        })
        .sum()
}

// ---------------------------------------------------------------------------
// §8.2 - the frozen k-case table
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: The frozen `SPEC_meshgen_geometry.md` §8.2 band table, indexed by `k` = the number of
//   collapsed pairs in the cell.
// Inputs: the three pairs and the per-edge diagonal flags.
// Returns: the tets, the template row, and the `k = 3` sheet triangle - or the failure that stopped
//   the table.
// Side effects: None.
// Notes: `(i, j, l)` range over the pair indices, collapsed ones first, exactly as the frozen text
//   writes them. `k = 3` legitimately emits no tets - the cell *is* the sheet triangle `(r0,r1,r2)`
//   and its faces come from the sheet cut, not from here. Nothing in this function looks at
//   geometry: every row is checked afterwards by the runtime ladder.
pub fn band_cell_table(
    pairs: [BandPair; 3],
    diagonals: CellDiagonals,
) -> Result<BandTableRow, BandFailure> {
    let mut order: [usize; 3] = [0, 1, 2];
    order.sort_by_key(|&i| (!pairs[i].is_collapsed(), i));
    let k = pairs.iter().filter(|p| p.is_collapsed()).count();
    let mut tets: SmallVec<[[u32; 4]; 8]> = SmallVec::new();
    match k {
        0 => {
            let a = [pairs[0].a, pairs[1].a, pairs[2].a];
            let b = [pairs[0].b, pairs[1].b, pairs[2].b];
            let Some(split) = prism_tets_with_diagonals(a, b, diagonals) else {
                return Err(BandFailure::Cyclic);
            };
            tets.extend(split);
            Ok((tets, BandTemplate::Prism, None))
        }
        1 => {
            let (i, j, l) = (order[0], order[1], order[2]);
            let apex = pairs[i].wall_a();
            let (slot, quad) = edge_quad(pairs, j, l);
            for triangle in split_quad(quad, diagonals[slot]) {
                tets.push([triangle[0], triangle[1], triangle[2], apex]);
            }
            Ok((tets, BandTemplate::Pyramid, None))
        }
        2 => {
            let (i, j, l) = (order[0], order[1], order[2]);
            tets.push([pairs[i].wall_a(), pairs[j].wall_a(), pairs[l].a, pairs[l].b]);
            Ok((tets, BandTemplate::Tet, None))
        }
        _ => {
            let sheet = [pairs[0].wall_a(), pairs[1].wall_a(), pairs[2].wall_a()];
            Ok((tets, BandTemplate::Sheet, Some(sheet)))
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The pair-edge slot joining two pairs of one cell, and that edge's quad in its stored
//   winding.
// Inputs: the cell's pairs and the two pair indices.
// Returns: the `BAND_EDGES` slot and the quad `(a_i, a_j, b_j, b_i)` for that slot's own ordering.
// Side effects: None.
// Notes: The *slot's* ordering is used, not the caller's, so the diagonal flag means the same thing
//   here as it does in the boundary and in both incident cells.
fn edge_quad(pairs: [BandPair; 3], x: usize, y: usize) -> (usize, [u32; 4]) {
    let slot = BAND_EDGES
        .iter()
        .position(|&(i, j)| (i == x && j == y) || (i == y && j == x))
        .unwrap_or(0);
    let (i, j) = BAND_EDGES[slot];
    (slot, [pairs[i].a, pairs[j].a, pairs[j].b, pairs[i].b])
}

// ---------------------------------------------------------------------------
// §4.4 - the runtime ladder, per cell
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Check one candidate decomposition of a band cell: orientation, the dihedral floor, and
//   the boundary-volume dry-run.
// Inputs: the candidate tets, the cell's boundary facets, the node table, and the floor in degrees.
// Returns: Ok((oriented tets, worst min dihedral, worst aspect ratio, relative volume error)) or the
//   failure.
// Side effects: None.
// Notes: The volume test is the one that catches a wrong table row rather than a bad geometry, so it
//   runs even when every tet is comfortably shaped.
fn check_candidate(
    tets: &[[u32; 4]],
    facets: &[[u32; 3]],
    nodes: &[Vec3],
    min_dihedral_deg: f64,
) -> Result<CheckedCandidate, BandFailure> {
    let mut oriented: SmallVec<[[u32; 4]; 8]> = SmallVec::new();
    let mut worst_dihedral = f64::INFINITY;
    let mut worst_ar: f64 = 0.0;
    let mut volume = 0.0;
    for &tet in tets {
        let Some(tet) = orient_positively(tet, nodes) else {
            return Err(BandFailure::Inverted);
        };
        let quality = tet_quality([
            nodes[tet[0] as usize],
            nodes[tet[1] as usize],
            nodes[tet[2] as usize],
            nodes[tet[3] as usize],
        ]);
        if quality.min_dihedral_deg < min_dihedral_deg {
            return Err(BandFailure::Dihedral);
        }
        worst_dihedral = worst_dihedral.min(quality.min_dihedral_deg);
        worst_ar = worst_ar.max(quality.aspect_ratio);
        volume += quality.volume.abs();
        oriented.push(tet);
    }
    let target = enclosed_volume(facets, nodes).abs();
    let error = if target > 0.0 {
        ((volume - target) / target).abs()
    } else if volume > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };
    if error > BAND_VOLUME_TOLERANCE {
        return Err(BandFailure::Volume);
    }
    Ok((oriented, worst_dihedral, worst_ar, error))
}

// AI-FUNC-SUMMARY:
// Purpose: Mesh one band cell by the frozen table under a given set of diagonals, and check it.
// Inputs: the three pairs, the diagonals, the node table, the dihedral floor, and the ladder rung to
//   record.
// Returns: the meshed cell, or the failure the check found.
// Side effects: None.
// Notes: `k = 3` returns immediately with no tets and no checks - there is nothing to check, the
//   cell is the sheet triangle. Two pairs contributing the same node is rejected here rather than
//   deeper, because such a cell has no interior at all.
pub fn mesh_band_cell(
    pairs: [BandPair; 3],
    diagonals: CellDiagonals,
    nodes: &[Vec3],
    min_dihedral_deg: f64,
    rung: u8,
) -> Result<BandCellMesh, BandFailure> {
    let corners = [pairs[0].wall_a(), pairs[1].wall_a(), pairs[2].wall_a()];
    if corners[0] == corners[1] || corners[1] == corners[2] || corners[2] == corners[0] {
        return Err(BandFailure::Degenerate);
    }
    let (tets, template, sheet) = band_cell_table(pairs, diagonals)?;
    if template == BandTemplate::Sheet {
        return Ok(BandCellMesh {
            tets: SmallVec::new(),
            template,
            sheet_triangle: sheet,
            rung,
            min_dihedral_deg: f64::INFINITY,
            max_aspect_ratio: 0.0,
            volume_error: 0.0,
        });
    }
    let facets = band_cell_boundary(pairs, diagonals);
    let (tets, dihedral, ar, error) = check_candidate(&tets, &facets, nodes, min_dihedral_deg)?;
    Ok(BandCellMesh {
        tets,
        template,
        sheet_triangle: None,
        rung,
        min_dihedral_deg: dihedral,
        max_aspect_ratio: ar,
        volume_error: error,
    })
}

// AI-FUNC-SUMMARY:
// Purpose: The §4.4 ladder's last rung - cone the cell's own boundary to a Steiner point.
// Inputs: the pairs, the diagonals, the node table, and the apex node the caller allocated.
// Returns: the meshed cell (8 tets for a full prism), or the failure.
// Side effects: None.
// Notes: Still exactly one geometric layer across the gap, which is the property the whole stage
//   exists to preserve, and conforming because the boundary it cones is the same pure function of
//   the pairs the neighbours use. No dihedral floor is applied: this rung exists precisely because
//   the floor could not be met, and its output is valid (positive, filling) rather than good. It is
//   counted, and a region that needs it too often is demoted instead.
pub fn steiner_band_cell(
    pairs: [BandPair; 3],
    diagonals: CellDiagonals,
    nodes: &[Vec3],
    apex: u32,
    apex_position: Vec3,
) -> Result<BandCellMesh, BandFailure> {
    let mut global: SmallVec<[u32; 7]> = SmallVec::new();
    for pair in pairs {
        for node in [pair.wall_a(), pair.wall_b()] {
            if !global.contains(&node) {
                global.push(node);
            }
        }
    }
    let local_apex = global.len() as u32;
    let mut positions: SmallVec<[Vec3; 8]> =
        global.iter().map(|&id| nodes[id as usize]).collect();
    positions.push(apex_position);
    let to_local = |id: u32| global.iter().position(|&g| g == id).unwrap_or(0) as u32;
    let local_pairs = pairs.map(|pair| BandPair {
        a: to_local(pair.a),
        b: to_local(pair.b),
        collapsed: pair.collapsed.map(to_local),
    });
    let facets = band_cell_boundary(local_pairs, diagonals);
    let fan: SmallVec<[[u32; 4]; 8]> = facets
        .iter()
        .map(|f| [f[0], f[1], f[2], local_apex])
        .collect();
    let (local_tets, dihedral, ar, error) = check_candidate(&fan, &facets, &positions, 0.0)?;
    let tets: SmallVec<[[u32; 4]; 8]> = local_tets
        .iter()
        .map(|tet| {
            tet.map(|node| {
                if node == local_apex {
                    apex
                } else {
                    global[node as usize]
                }
            })
        })
        .collect();
    Ok(BandCellMesh {
        tets,
        template: BandTemplate::Steiner,
        sheet_triangle: None,
        rung: 2,
        min_dihedral_deg: dihedral,
        max_aspect_ratio: ar,
        volume_error: error,
    })
}

// ---------------------------------------------------------------------------
// §10.11 - the FEM-aware ladder
// ---------------------------------------------------------------------------

/// Which quality preset the ladder's gates are read at (`PLAN_mesh_generation.md`
/// §10.11). `Explicit` adds the altitude floor as a stable-time-step proxy - a
/// documented *geometric* proxy, because a true `Δt` needs material data the mesher
/// does not have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FemProfile {
    #[default]
    Implicit,
    Explicit,
}

impl FemProfile {
    // AI-FUNC-SUMMARY: The altitude floor this profile imposes, as a fraction of `h_local` (0 = off); returns f64; side effects: none.
    pub fn altitude_ratio(&self) -> f64 {
        match self {
            FemProfile::Implicit => 0.0,
            FemProfile::Explicit => BAND_EXPLICIT_ALTITUDE_RATIO,
        }
    }
}

/// The element quality a band region is predicted to produce, measured *before*
/// anything is meshed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandPrediction {
    /// The gap the band would span.
    pub t: f64,
    /// The in-plane element size at the region.
    pub h: f64,
    pub min_dihedral_deg: f64,
    pub max_aspect_ratio: f64,
    pub min_radius_ratio: f64,
    pub min_altitude: f64,
}

// AI-FUNC-SUMMARY:
// Purpose: Predict the element quality of a band across a gap `t` at in-plane size `h` (§10.11).
// Inputs: the gap thickness and the local element size.
// Returns: the worst metrics over the nominal band cell of `SPEC_meshgen_geometry.md` §8.2 - a
//   right-isoceles cap of legs `h` extruded by `t` - taken over all six non-cyclic diagonal patterns
//   of §4.3.
// Side effects: None.
// Notes: The plan states the prediction as closed forms ("altitude ~ t, min dihedral ~ atan(t/h)").
//   Those are the right scalings but not the right *numbers*, and the numbers are what the gates
//   compare against, so this measures the model cell with the same `tet_quality` the verifier uses
//   instead of evaluating a formula. Taking the worst over all six patterns keeps the prediction
//   independent of node keys - a prediction that changed with the key table could not be made before
//   the nodes exist, which is the whole point of predicting.
//
//   The cap is right-isoceles rather than equilateral for two reasons: it is the shape §8.2 measured
//   its nominal cell on (18.281 deg at `t/h = 0.35`, reproduced by the acceptance test), and it is
//   the *worse* of the two by about three degrees - a gate read on it does not admit a band that the
//   real geometry then fails.
pub fn predict_band_quality(t: f64, h: f64) -> BandPrediction {
    let mut prediction = BandPrediction {
        t,
        h,
        min_dihedral_deg: f64::INFINITY,
        max_aspect_ratio: 0.0,
        min_radius_ratio: f64::INFINITY,
        min_altitude: f64::INFINITY,
    };
    if t <= 0.0 || h <= 0.0 || !t.is_finite() || !h.is_finite() {
        return BandPrediction {
            min_dihedral_deg: 0.0,
            max_aspect_ratio: f64::INFINITY,
            min_radius_ratio: 0.0,
            min_altitude: 0.0,
            ..prediction
        };
    }
    let cap = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(h, 0.0, 0.0),
        Vec3::new(0.0, h, 0.0),
    ];
    let nodes: Vec<Vec3> = cap
        .iter()
        .copied()
        .chain(cap.iter().map(|p| p.add(Vec3::new(0.0, 0.0, t))))
        .collect();
    for mask in 0..8u8 {
        let diagonals = [mask & 1 != 0, mask & 2 != 0, mask & 4 != 0];
        let Some(tets) = prism_tets_with_diagonals([0, 1, 2], [3, 4, 5], diagonals) else {
            continue;
        };
        for tet in tets {
            let quality = tet_quality([
                nodes[tet[0] as usize],
                nodes[tet[1] as usize],
                nodes[tet[2] as usize],
                nodes[tet[3] as usize],
            ]);
            prediction.min_dihedral_deg = prediction.min_dihedral_deg.min(quality.min_dihedral_deg);
            prediction.max_aspect_ratio = prediction.max_aspect_ratio.max(quality.aspect_ratio);
            prediction.min_radius_ratio = prediction.min_radius_ratio.min(quality.radius_ratio);
            prediction.min_altitude = prediction.min_altitude.min(quality.min_altitude);
        }
    }
    prediction
}

/// What §10.11's ladder decided for one thin region.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LadderOutcome {
    /// Rung 1 - the predicted band passes the gates.
    Band,
    /// Rung 2 - one extra refinement level would pass; re-run with the finer `h`.
    RefineLocally,
    /// Rung 3 - the gap is at or below `t_sheet`; collapse it.
    Sheet,
    /// Rung 4 - representable but poor, and the user did not forbid it: keep the
    /// region volumetric and warn.
    Volumetric,
    /// Rung 5 - the profile marks the metrics unacceptable and no option remains.
    Reject,
}

/// One region's ladder decision, with everything the report needs to name it.
#[derive(Clone, Debug, PartialEq)]
pub struct LadderDecision {
    pub outcome: LadderOutcome,
    pub prediction: BandPrediction,
    /// The prediction one refinement level finer, when rung 2 was reached.
    pub refined: Option<BandPrediction>,
    /// The gate that failed, or why the outcome was taken.
    pub reason: String,
}

/// The tunables of S8b.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThinOptions {
    /// The §8.2 / §4.4 runtime and predicted dihedral floor, in degrees.
    pub min_dihedral_deg: f64,
    /// The §10.11 predicted aspect-ratio ceiling.
    pub max_aspect_ratio: f64,
    pub fem_profile: FemProfile,
    /// The altitude floor as a fraction of `h_local`; 0 disables it. Defaults to the
    /// profile's own ratio.
    pub altitude_ratio: f64,
    /// The share of a band's cells that may take the Steiner fallback before the
    /// region is demoted to volumetric.
    pub regional_failure_share: f64,
    /// Whether rung 4 (keep volumetric and warn) is available; `true` forces rung 5
    /// instead, which is the "user forbade it" branch of the frozen ladder.
    pub forbid_volumetric: bool,
    /// The sizing floor - a refinement below it is not available to rung 2.
    pub h_min: f64,
}

impl Default for ThinOptions {
    // AI-FUNC-SUMMARY: The frozen §10.11 defaults (8 deg, AR 20, implicit profile); returns ThinOptions; side effects: none.
    fn default() -> Self {
        ThinOptions {
            min_dihedral_deg: BAND_MIN_DIHEDRAL_DEG,
            max_aspect_ratio: BAND_MAX_AR,
            fem_profile: FemProfile::Implicit,
            altitude_ratio: 0.0,
            regional_failure_share: BAND_REGIONAL_FAILURE_SHARE,
            forbid_volumetric: false,
            h_min: 0.0,
        }
    }
}

impl ThinOptions {
    // AI-FUNC-SUMMARY:
    // Purpose: The effective altitude floor in model units for a region at size `h`.
    // Returns: `altitude_ratio * h` where the ratio is set, else the profile's own ratio times `h`.
    // Side effects: None.
    pub fn altitude_floor(&self, h: f64) -> f64 {
        let ratio = if self.altitude_ratio > 0.0 {
            self.altitude_ratio
        } else {
            self.fem_profile.altitude_ratio()
        };
        ratio * h
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Which §10.11 gate a prediction fails, if any.
    // Inputs: the prediction and the local size the altitude floor is measured against.
    // Returns: None when every gate passes, else the failing gate's message.
    // Side effects: None.
    pub fn failing_gate(&self, prediction: &BandPrediction) -> Option<String> {
        if prediction.min_dihedral_deg < self.min_dihedral_deg {
            return Some(format!(
                "predicted min dihedral {:.3} deg below the {:.3} deg gate",
                prediction.min_dihedral_deg, self.min_dihedral_deg
            ));
        }
        if prediction.max_aspect_ratio > self.max_aspect_ratio {
            return Some(format!(
                "predicted aspect ratio {:.3} above the {:.3} gate",
                prediction.max_aspect_ratio, self.max_aspect_ratio
            ));
        }
        let floor = self.altitude_floor(prediction.h);
        if floor > 0.0 && prediction.min_altitude < floor {
            return Some(format!(
                "predicted min altitude {:.6} below the {:?}-profile floor {:.6} ({:.3} * h)",
                prediction.min_altitude,
                self.fem_profile,
                floor,
                floor / prediction.h
            ));
        }
        None
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The `PLAN_mesh_generation.md` §10.11 decision ladder for one thin region.
// Inputs: the region's measured gap `t`, its local element size `h`, the sheet threshold at the
//   region, and the options.
// Returns: the outcome, the prediction it was taken on, and an actionable reason.
// Side effects: None.
// Notes: The rungs are taken strictly in the frozen order, and each one is only reached because the
//   one above it was refused - so the `reason` of a rung-4 or rung-5 decision names the gate that
//   sent it there, which is what makes a rejection actionable rather than merely final. Rung 2 is
//   available only while `h / 2` stays at or above the sizing floor: below it there is no finer mesh
//   to re-run with.
pub fn band_ladder(t: f64, h: f64, t_sheet: f64, options: &ThinOptions) -> LadderDecision {
    let prediction = predict_band_quality(t, h);
    let Some(gate) = options.failing_gate(&prediction) else {
        return LadderDecision {
            outcome: LadderOutcome::Band,
            prediction,
            refined: None,
            reason: format!(
                "predicted min dihedral {:.3} deg, AR {:.3}, altitude {:.6}",
                prediction.min_dihedral_deg, prediction.max_aspect_ratio, prediction.min_altitude
            ),
        };
    };
    let finer = h * 0.5;
    if finer >= options.h_min {
        let refined = predict_band_quality(t, finer);
        if options.failing_gate(&refined).is_none() {
            return LadderDecision {
                outcome: LadderOutcome::RefineLocally,
                prediction,
                refined: Some(refined),
                reason: format!("{gate}; one level finer (h {h:.6} -> {finer:.6}) passes"),
            };
        }
    }
    if t <= t_sheet {
        return LadderDecision {
            outcome: LadderOutcome::Sheet,
            prediction,
            refined: None,
            reason: format!("{gate}; t {t:.6} is at or below t_sheet {t_sheet:.6}"),
        };
    }
    if options.forbid_volumetric {
        return LadderDecision {
            outcome: LadderOutcome::Reject,
            prediction,
            refined: None,
            reason: format!(
                "{gate}; t {t:.6} exceeds t_sheet {t_sheet:.6}, refinement is at the floor h_min {:.6}, and the volumetric fallback is forbidden",
                options.h_min
            ),
        };
    }
    LadderDecision {
        outcome: LadderOutcome::Volumetric,
        prediction,
        refined: None,
        reason: format!("{gate}; kept volumetric"),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The Steiner point the §4.4 fallback cones to - the band cell's centroid.
// Inputs: the three pairs and the node table.
// Returns: the mean of the cell's distinct corner positions.
// Side effects: None.
// Notes: Allocated by the caller and shared by nothing: unlike a *face* centroid, a cell centroid is
//   interior, so no neighbour ever sees it and a coincident duplicate is impossible.
pub fn band_cell_centroid(pairs: [BandPair; 3], nodes: &[Vec3]) -> Vec3 {
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    let mut count = 0.0;
    for pair in pairs {
        match pair.collapsed {
            Some(r) => {
                sum = sum.add(nodes[r as usize]);
                count += 1.0;
            }
            None => {
                sum = sum.add(nodes[pair.a as usize]).add(nodes[pair.b as usize]);
                count += 2.0;
            }
        }
    }
    sum.scale(1.0 / count)
}

// ---------------------------------------------------------------------------
// The layer - cells, negotiation, regional demotion
// ---------------------------------------------------------------------------

/// One band cell, as three indices into the layer's pair list.
///
/// The indices are what makes a pair-edge *nameable* across cells: two cells share
/// a quad exactly when they share two pair indices, and that is how the ladder's
/// flip step finds the neighbour it has to agree with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandCell {
    /// The thin region this cell belongs to - the unit regional demotion works on.
    pub region: usize,
    pub pairs: [usize; 3],
}

/// The input to S8b's meshing half: the matched pairs and the cells over them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BandLayer {
    pub pairs: Vec<BandPair>,
    pub cells: Vec<BandCell>,
}

/// S8b diagnostics.
#[derive(Clone, Debug, PartialEq)]
pub struct ThinStats {
    pub n_cells: usize,
    pub n_prism: usize,
    pub n_pyramid: usize,
    pub n_tet: usize,
    pub n_sheet: usize,
    pub n_steiner: usize,
    /// Cells the ladder's flip step recovered, and the quads it flipped.
    pub n_flipped_cells: usize,
    pub n_flipped_edges: usize,
    pub n_failed: usize,
    pub n_tets: usize,
    pub n_collapsed_pairs: usize,
    pub n_demoted_regions: usize,
    pub min_dihedral_deg: f64,
    pub max_aspect_ratio: f64,
    pub worst_volume_error: f64,
}

impl Default for ThinStats {
    // AI-FUNC-SUMMARY: Empty statistics with the extremes at their identity values; returns ThinStats; side effects: none.
    fn default() -> Self {
        ThinStats {
            n_cells: 0,
            n_prism: 0,
            n_pyramid: 0,
            n_tet: 0,
            n_sheet: 0,
            n_steiner: 0,
            n_flipped_cells: 0,
            n_flipped_edges: 0,
            n_failed: 0,
            n_tets: 0,
            n_collapsed_pairs: 0,
            n_demoted_regions: 0,
            min_dihedral_deg: f64::INFINITY,
            max_aspect_ratio: 0.0,
            worst_volume_error: 0.0,
        }
    }
}

/// The result of meshing a band layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BandLayerMesh {
    /// Steiner apex positions, in allocation order; their node ids continue the
    /// caller's table from `first_steiner_id`.
    pub steiner_nodes: Vec<Vec3>,
    pub first_steiner_id: u32,
    /// Per input cell, its mesh - `None` where the cell was dropped, either because
    /// the ladder ran out or because its region was demoted.
    pub cells: Vec<Option<BandCellMesh>>,
    /// The `k = 3` sheet triangles, in cell order.
    pub sheet_triangles: Vec<[u32; 3]>,
    /// Regions demoted to volumetric because too many of their cells needed the
    /// Steiner fallback (`[THIN-SKIP]`).
    pub demoted_regions: BTreeSet<usize>,
    pub failures: Vec<(usize, BandFailure)>,
    pub warnings: Vec<String>,
    pub stats: ThinStats,
}

// AI-FUNC-SUMMARY:
// Purpose: The canonical name of the quad two band cells share - an unordered pair of pair indices.
// Returns: the two indices, smaller first.
// Side effects: None.
fn edge_key(pairs: [usize; 3], slot: usize) -> (usize, usize) {
    let (i, j) = BAND_EDGES[slot];
    let (a, b) = (pairs[i], pairs[j]);
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

// AI-FUNC-SUMMARY:
// Purpose: One cell's three diagonal flags - Rule SNK, with any negotiated flip applied.
// Inputs: the cell's pairs, the key table, and the flip set keyed by `edge_key`.
// Returns: the flags in `BAND_EDGES` order.
// Side effects: None.
// Notes: The flip is applied as an XOR rather than as an absolute value, and that is what makes it
//   safe to share between the two cells incident to a quad: they may list the quad's corners in
//   opposite order, which flips the *meaning* of the raw SNK flag but not the geometric diagonal it
//   selects, so only a relative override transfers correctly.
fn cell_diagonals(
    cell: &BandCell,
    pairs: [BandPair; 3],
    keys: &[NodeKey],
    flips: &BTreeSet<(usize, usize)>,
) -> CellDiagonals {
    let mut diagonals = snk_cell_diagonals(pairs, keys);
    for (slot, diagonal) in diagonals.iter_mut().enumerate() {
        if flips.contains(&edge_key(cell.pairs, slot)) {
            *diagonal = !*diagonal;
        }
    }
    diagonals
}

// AI-FUNC-SUMMARY:
// Purpose: Mesh a whole band layer: the frozen table on every cell, then the §4.4 ladder over the
//   cells that failed, then the regional demotion.
// Inputs: the layer, the node table, the key table and the options.
// Returns: the meshed cells, the Steiner nodes allocated for them, the demoted regions and the stats.
// Side effects: None (the caller appends `steiner_nodes` to its own table).
// Notes: Three rungs, in the order the reference thin-feature design §3.8 froze them, and the middle
//   one is the reason this function exists rather than a per-cell loop. A diagonal flip is *not* a
//   pure function of the quad, so a cell cannot take one alone without leaving a hanging edge on the
//   neighbour that shares it; here the flip is proposed for both incident cells at once and accepted
//   only if both still mesh. That is what "attempted only pairwise" means, and it is why the flip
//   rung is implemented here where the cut's §4.4 ladder skips it.
//
//   Determinism: pass 0 is a parallel map (R-P1) over an indexed collection, and every later pass
//   walks cells and edges in ascending order through ordered sets, so the output is independent of
//   thread count (R-P2).
pub fn mesh_band_layer(
    layer: &BandLayer,
    nodes: &[Vec3],
    keys: &[NodeKey],
    options: &ThinOptions,
) -> BandLayerMesh {
    let pairs_of = |cell: &BandCell| {
        [
            layer.pairs[cell.pairs[0]],
            layer.pairs[cell.pairs[1]],
            layer.pairs[cell.pairs[2]],
        ]
    };
    let mut flips: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut results: Vec<Result<BandCellMesh, BandFailure>> = layer
        .cells
        .par_iter()
        .map(|cell| {
            let pairs = pairs_of(cell);
            let diagonals = cell_diagonals(cell, pairs, keys, &flips);
            mesh_band_cell(pairs, diagonals, nodes, options.min_dihedral_deg, 0)
        })
        .collect();

    let mut incident: BTreeMap<(usize, usize), SmallVec<[usize; 2]>> = BTreeMap::new();
    for (index, cell) in layer.cells.iter().enumerate() {
        for slot in 0..3 {
            incident.entry(edge_key(cell.pairs, slot)).or_default().push(index);
        }
    }

    let failing: Vec<usize> = (0..layer.cells.len())
        .filter(|&index| results[index].is_err())
        .collect();
    let mut locked: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut flipped_cells = 0usize;
    for &index in &failing {
        if results[index].is_ok() {
            continue;
        }
        for slot in 0..3 {
            let key = edge_key(layer.cells[index].pairs, slot);
            if locked.contains(&key) {
                continue;
            }
            let affected = incident.get(&key).cloned().unwrap_or_default();
            flips.insert(key);
            let candidates: Vec<(usize, Result<BandCellMesh, BandFailure>)> = affected
                .iter()
                .map(|&other| {
                    let cell = &layer.cells[other];
                    let pairs = pairs_of(cell);
                    let diagonals = cell_diagonals(cell, pairs, keys, &flips);
                    (
                        other,
                        mesh_band_cell(pairs, diagonals, nodes, options.min_dihedral_deg, 1),
                    )
                })
                .collect();
            let accept = candidates.iter().all(|(_, result)| result.is_ok())
                && candidates.iter().any(|(other, _)| *other == index);
            if accept {
                locked.insert(key);
                for (other, result) in candidates {
                    if results[other].is_err() || other == index {
                        flipped_cells += 1;
                    }
                    results[other] = result;
                }
                break;
            }
            flips.remove(&key);
        }
    }

    let mut mesh = BandLayerMesh {
        first_steiner_id: nodes.len() as u32,
        cells: vec![None; layer.cells.len()],
        ..BandLayerMesh::default()
    };
    let mut steiner_per_region: BTreeMap<usize, usize> = BTreeMap::new();
    let mut cells_per_region: BTreeMap<usize, usize> = BTreeMap::new();
    for (index, cell) in layer.cells.iter().enumerate() {
        *cells_per_region.entry(cell.region).or_default() += 1;
        let pairs = pairs_of(cell);
        let diagonals = cell_diagonals(cell, pairs, keys, &flips);
        let result = match std::mem::replace(&mut results[index], Err(BandFailure::Degenerate)) {
            Ok(meshed) => Ok(meshed),
            Err(reason) => {
                let apex_position = band_cell_centroid(pairs, nodes);
                let apex = mesh.first_steiner_id + mesh.steiner_nodes.len() as u32;
                match steiner_band_cell(pairs, diagonals, nodes, apex, apex_position) {
                    Ok(meshed) => {
                        mesh.steiner_nodes.push(apex_position);
                        *steiner_per_region.entry(cell.region).or_default() += 1;
                        Ok(meshed)
                    }
                    Err(_) => Err(reason),
                }
            }
        };
        match result {
            Ok(meshed) => mesh.cells[index] = Some(meshed),
            Err(reason) => {
                mesh.failures.push((index, reason));
            }
        }
    }

    for (&region, &steiner) in &steiner_per_region {
        let total = cells_per_region.get(&region).copied().unwrap_or(0).max(1);
        if steiner as f64 > options.regional_failure_share * total as f64 {
            mesh.demoted_regions.insert(region);
            mesh.warnings.push(format!(
                "[THIN-SKIP] region {region}: {steiner} of {total} band cells needed the Steiner fallback ({:.1} % > {:.1} %); the region falls back to volumetric",
                100.0 * steiner as f64 / total as f64,
                100.0 * options.regional_failure_share
            ));
        }
    }
    if !mesh.demoted_regions.is_empty() {
        let demoted: Vec<usize> = mesh.demoted_regions.iter().copied().collect();
        mesh.steiner_nodes.clear();
        let mut reallocated = 0u32;
        for (index, cell) in layer.cells.iter().enumerate() {
            if demoted.contains(&cell.region) {
                mesh.cells[index] = None;
                continue;
            }
            if let Some(meshed) = &mut mesh.cells[index] {
                if meshed.template == BandTemplate::Steiner {
                    let pairs = pairs_of(cell);
                    let diagonals = cell_diagonals(cell, pairs, keys, &flips);
                    let apex_position = band_cell_centroid(pairs, nodes);
                    let apex = mesh.first_steiner_id + reallocated;
                    if let Ok(remeshed) =
                        steiner_band_cell(pairs, diagonals, nodes, apex, apex_position)
                    {
                        *meshed = remeshed;
                        mesh.steiner_nodes.push(apex_position);
                        reallocated += 1;
                    }
                }
            }
        }
        mesh.failures.retain(|(index, _)| {
            !demoted.contains(&layer.cells[*index].region)
        });
    }

    mesh.stats.n_cells = layer.cells.len();
    mesh.stats.n_flipped_cells = flipped_cells;
    mesh.stats.n_flipped_edges = flips.len();
    mesh.stats.n_failed = mesh.failures.len();
    mesh.stats.n_demoted_regions = mesh.demoted_regions.len();
    mesh.stats.n_collapsed_pairs = layer.pairs.iter().filter(|p| p.is_collapsed()).count();
    for meshed in mesh.cells.iter().flatten() {
        match meshed.template {
            BandTemplate::Prism => mesh.stats.n_prism += 1,
            BandTemplate::Pyramid => mesh.stats.n_pyramid += 1,
            BandTemplate::Tet => mesh.stats.n_tet += 1,
            BandTemplate::Sheet => mesh.stats.n_sheet += 1,
            BandTemplate::Steiner => mesh.stats.n_steiner += 1,
        }
        mesh.stats.n_tets += meshed.tets.len();
        if meshed.min_dihedral_deg.is_finite() {
            mesh.stats.min_dihedral_deg = mesh.stats.min_dihedral_deg.min(meshed.min_dihedral_deg);
        }
        mesh.stats.max_aspect_ratio = mesh.stats.max_aspect_ratio.max(meshed.max_aspect_ratio);
        mesh.stats.worst_volume_error = mesh.stats.worst_volume_error.max(meshed.volume_error);
        if let Some(triangle) = meshed.sheet_triangle {
            mesh.sheet_triangles.push(triangle);
        }
    }
    mesh
}

// ---------------------------------------------------------------------------
// The doubly-cut face-split rule, and the three-slab split it enables
// ---------------------------------------------------------------------------

/// Which slab of a band cell a boundary triangle belongs to.
///
/// The three are ordered across the gap: `NearSide` is the piece the first wall
/// cuts off, `Band` is the gap itself, `FarSide` is everything past the second wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slab {
    NearSide,
    Band,
    FarSide,
}

/// One doubly-cut face, triangulated and tagged by slab.
pub type BandFaceSplit = SmallVec<[(Slab, [u32; 3]); 6]>;

/// A doubly-cut face's triangles, plus the two matched `(near, far)` crossing pairs
/// it spans - the `BandPair` atoms the §8.2 templates are written in terms of.
pub type BandFaceParts = (BandFaceSplit, [(u32, u32); 2]);

// AI-FUNC-SUMMARY:
// Purpose: The doubly-cut face-split rule - how a parent face carrying *two* cut nodes per crossed
//   edge, one per component, is triangulated.
// Inputs: the face's boundary loop in traversal order (parent nodes with their cut nodes inserted in
//   edge order), the id below which a node is a parent node, and the key table for Rule SNK.
// Returns: the triangles tagged by slab and the two `(near, far)` crossing pairs the face spans, or
//   None when the face is not in the canonical doubly-cut configuration (which sends the caller back
//   to the generic loop fan).
// Side effects: None.
// Notes: This is the piece `SPEC_meshgen_geometry.md` §5.2 does not have and §7's loop fan gets
//   wrong. §5.2 covers a face cut once; a band cell's faces are cut *twice*, and the loop fan cones
//   the whole face to one centroid, producing triangles that span the gap - which makes the cell's
//   three-slab split impossible and is why the gap was being chamfered away.
//
//   The rule is small, and like §5.2 it is a pure function of the *face*: a face whose two crossed
//   edges share a vertex `c` splits into the corner triangle at `c`, the strip between the two
//   walls, and the remainder, the two quads taken on Rule SNK. Both cells incident to the face
//   compute it identically whether or not either is a band cell - which is the same conformity
//   argument §5.2 rests on, and the reason this can be added without renegotiating anything.
//
//   The loop is walked *from* `c` in both directions, so all three rotations of the crossed-edge
//   pair are handled by one piece of code rather than three table rows.
pub fn band_face_split(
    loop_nodes: &[u32],
    first_cut_node: u32,
    keys: &[NodeKey],
) -> Option<BandFaceParts> {
    let n = loop_nodes.len();
    if n != 7 {
        // Three parent nodes, two crossed edges, two cut nodes on each: any other
        // length is a configuration this rule does not claim.
        return None;
    }
    let is_parent = |node: u32| node < first_cut_node;
    let parents: SmallVec<[usize; 4]> = (0..n).filter(|&i| is_parent(loop_nodes[i])).collect();
    if parents.len() != 3 {
        return None;
    }
    // `c` is the parent whose two loop neighbours are both cut nodes: the vertex the
    // two crossed edges share.
    let corner = parents
        .iter()
        .copied()
        .find(|&i| !is_parent(loop_nodes[(i + 1) % n]) && !is_parent(loop_nodes[(i + n - 1) % n]))?;
    let c = loop_nodes[corner];
    let near1 = loop_nodes[(corner + 1) % n];
    let far1 = loop_nodes[(corner + 2) % n];
    let near2 = loop_nodes[(corner + n - 1) % n];
    let far2 = loop_nodes[(corner + n - 2) % n];
    if is_parent(near1) || is_parent(far1) || is_parent(near2) || is_parent(far2) {
        return None;
    }
    // Everything strictly between `far1` and `far2` going forward is the remainder's
    // parent side; the canonical configuration leaves exactly the two other parents.
    let mut rest: SmallVec<[u32; 2]> = SmallVec::new();
    let mut walk = (corner + 3) % n;
    while loop_nodes[walk] != far2 {
        rest.push(loop_nodes[walk]);
        walk = (walk + 1) % n;
    }
    if rest.len() != 2 || !rest.iter().all(|&node| is_parent(node)) {
        return None;
    }
    let mut out: BandFaceSplit = SmallVec::new();
    out.push((Slab::NearSide, [c, near1, near2]));
    for triangle in snk_split_quad([near1, far1, far2, near2], keys) {
        out.push((Slab::Band, triangle));
    }
    for triangle in snk_split_quad([far1, rest[0], rest[1], far2], keys) {
        out.push((Slab::FarSide, triangle));
    }
    Some((out, [(near1, far1), (near2, far2)]))
}

// AI-FUNC-SUMMARY:
// Purpose: Close an open, consistently oriented triangulation by triangulating the cycle its
//   unmatched directed edges form.
// Inputs: the open surface's triangles and the key table for Rule SNK.
// Returns: the closing triangles, or None when the opening is not a single cycle of 3 or 4 edges.
// Side effects: None.
// Notes: This is how a slab of a band cell gets its lid without any geometric reasoning about which
//   way the wall faces. A consistently oriented closed surface uses every directed edge exactly once
//   in each direction, so the edges left unmatched *are* the opening, and reversing them gives the
//   lid already correctly oriented. The two slabs on either side of one wall therefore receive lids
//   that are each other's reverse, built from the same nodes under the same rule - which is what
//   makes the wall a shared face of the mesh rather than two coincident ones.
pub fn close_open_surface(triangles: &[[u32; 3]], keys: &[NodeKey]) -> Option<BandFacets> {
    let mut directed: BTreeSet<(u32, u32)> = BTreeSet::new();
    for triangle in triangles {
        for slot in 0..3 {
            let edge = (triangle[slot], triangle[(slot + 1) % 3]);
            if !directed.insert(edge) {
                return None;
            }
        }
    }
    let open: Vec<(u32, u32)> = directed
        .iter()
        .copied()
        .filter(|&(from, to)| !directed.contains(&(to, from)))
        .collect();
    let mut out: BandFacets = SmallVec::new();
    if open.is_empty() {
        return Some(out);
    }
    // The lid runs *against* the opening, so the walk is over the reversed edges.
    let next: BTreeMap<u32, u32> = open.iter().map(|&(from, to)| (to, from)).collect();
    if next.len() != open.len() {
        return None;
    }
    // A slab can be open at both ends - the gap itself is - so the opening is walked
    // as however many disjoint cycles it contains, not assumed to be one.
    let mut visited: BTreeSet<u32> = BTreeSet::new();
    for &(_, start) in &open {
        if visited.contains(&start) {
            continue;
        }
        let mut cycle: SmallVec<[u32; 4]> = SmallVec::new();
        let mut node = start;
        loop {
            if !visited.insert(node) {
                return None;
            }
            cycle.push(node);
            node = *next.get(&node)?;
            if node == start {
                break;
            }
            if cycle.len() > 4 {
                return None;
            }
        }
        match cycle.len() {
            3 => out.push([cycle[0], cycle[1], cycle[2]]),
            4 => {
                for triangle in snk_split_quad([cycle[0], cycle[1], cycle[2], cycle[3]], keys) {
                    out.push(triangle);
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The three closed slab boundaries of a band cell, ordered across the gap.
pub type BandSlabs = [Vec<[u32; 3]>; 3];

/// A sandwiched cell, split: the three closed slabs and the three matched pairs the
/// gap slab spans, which are exactly the §8.2 templates' input.
#[derive(Clone, Debug, PartialEq)]
pub struct BandCellPlan {
    pub slabs: BandSlabs,
    pub pairs: [BandPair; 3],
}

/// Why a cell is not a sandwich the doubly-cut rule covers. Counted and reported, so
/// "the rule did not fire" is always answerable without a rebuild.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BandDecline {
    /// A face carries neither the doubly-cut pattern nor no cut at all.
    FaceShape,
    /// More than one uncut face, or none: the two walls do not isolate one vertex.
    UncutFaces,
    /// The doubly-cut faces do not all meet at the same parent vertex.
    Corner,
    /// A slab's opening is not a cycle of three or four edges.
    Unclosable,
}

// AI-FUNC-SUMMARY:
// Purpose: Split one cell's boundary into the three closed slabs a band cell has - the piece the
//   first wall cuts off, the gap itself, and everything past the second wall.
// Inputs: the cell's four face loops in traversal order, the id below which a node is a parent node,
//   the key table, and the node positions (used only to orient the loops consistently).
// Returns: the three closed boundaries and the gap slab's three matched pairs, or the reason the
//   cell is not a sandwich the rule covers.
// Side effects: None.
// Notes: This is what the doubly-cut face rule is *for*. The generic escalation path cones the whole
//   cell to one centroid, so a cell with a thin gap running through it comes out as one solid blob
//   and the gap is chamfered away - the single largest geometric loss in S8. Here each face
//   contributes its corner / strip / remainder triangles to the matching slab, the uncut face joins
//   the far slab, and each slab is closed by `close_open_surface`, whose lid is the wall itself.
//   Both slabs on either side of a wall get lids that are exact reverses, so the wall becomes one
//   shared face rather than two coincident ones.
//
//   The canonical cell has three doubly-cut faces meeting at one parent vertex and one uncut face
//   opposite it - the configuration a pair of near-parallel walls separating one vertex from the
//   other three always produces. Anything else returns None and takes the existing fan, so this is
//   strictly an improvement on the cells it does claim.
pub fn split_band_cell(
    loops: [&[u32]; 4],
    first_cut_node: u32,
    keys: &[NodeKey],
    points: &[Vec3],
) -> Result<BandCellPlan, BandDecline> {
    // The caller's face loops are key-sorted, so they are *not* consistently oriented
    // as a closed surface - the generic fan never needed them to be, because it
    // orients every piece it emits on its own. Closing a slab does need it, so the
    // loops are turned outward here first. This is a per-cell decision with no
    // conformity content: a neighbour sees the triangles, never the loop they came
    // from, and each emitted tet is oriented again downstream.
    let mut interior = Vec3::new(0.0, 0.0, 0.0);
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    for face in loops {
        for &node in face {
            if seen.insert(node) {
                interior = interior.add(points[node as usize]);
            }
        }
    }
    interior = interior.scale(1.0 / seen.len().max(1) as f64);
    let oriented: Vec<Vec<u32>> = loops
        .iter()
        .map(|face| {
            let mut normal = Vec3::new(0.0, 0.0, 0.0);
            let mut centroid = Vec3::new(0.0, 0.0, 0.0);
            for slot in 0..face.len() {
                let current = points[face[slot] as usize];
                let next = points[face[(slot + 1) % face.len()] as usize];
                normal = normal.add(current.cross(next));
                centroid = centroid.add(current);
            }
            centroid = centroid.scale(1.0 / face.len() as f64);
            if normal.dot(centroid.sub(interior)) >= 0.0 {
                face.to_vec()
            } else {
                face.iter().rev().copied().collect()
            }
        })
        .collect();
    let loops: [&[u32]; 4] = [&oriented[0], &oriented[1], &oriented[2], &oriented[3]];
    let mut slabs: BandSlabs = [Vec::new(), Vec::new(), Vec::new()];
    let mut pairs: SmallVec<[(u32, u32); 3]> = SmallVec::new();
    let mut uncut: Option<[u32; 3]> = None;
    let mut split_faces = 0usize;
    let mut corner: Option<u32> = None;
    for face in loops {
        if face.len() == 3 && face.iter().all(|&node| node < first_cut_node) {
            if uncut.is_some() {
                return Err(BandDecline::UncutFaces);
            }
            uncut = Some([face[0], face[1], face[2]]);
            continue;
        }
        let Some((split, face_pairs)) = band_face_split(face, first_cut_node, keys) else {
            return Err(BandDecline::FaceShape);
        };
        for pair in face_pairs {
            if !pairs.contains(&pair) {
                pairs.push(pair);
            }
        }
        split_faces += 1;
        for (slab, triangle) in split {
            if slab == Slab::NearSide {
                match corner {
                    None => corner = Some(triangle[0]),
                    Some(existing) if existing == triangle[0] => {}
                    Some(_) => return Err(BandDecline::Corner),
                }
            }
            slabs[slab as usize].push(triangle);
        }
    }
    let corner = corner.ok_or(BandDecline::Corner)?;
    let uncut = uncut.ok_or(BandDecline::UncutFaces)?;
    if split_faces != 3 {
        return Err(BandDecline::UncutFaces);
    }
    if uncut.contains(&corner) {
        return Err(BandDecline::Corner);
    }
    slabs[Slab::FarSide as usize].push(uncut);
    for slab in slabs.iter_mut() {
        let lid = close_open_surface(slab, keys).ok_or(BandDecline::Unclosable)?;
        if lid.is_empty() {
            return Err(BandDecline::Unclosable);
        }
        slab.extend(lid);
    }
    if pairs.len() != 3 {
        return Err(BandDecline::Corner);
    }
    Ok(BandCellPlan {
        slabs,
        pairs: [
            BandPair {
                a: pairs[0].0,
                b: pairs[0].1,
                collapsed: None,
            },
            BandPair {
                a: pairs[1].0,
                b: pairs[1].1,
                collapsed: None,
            },
            BandPair {
                a: pairs[2].0,
                b: pairs[2].1,
                collapsed: None,
            },
        ],
    })
}

// ---------------------------------------------------------------------------
// G7-2 - the S3 -> S8b bridge
// ---------------------------------------------------------------------------

/// What S8b needs to know about S3's thin regions, reduced to lookups by
/// *arranged face*.
///
/// S3 owns the regions; S8 works in cut nodes, and every cut node knows the
/// arranged face it was found on (`EdgeCrossing::face`). This is the whole
/// translation between the two, and it is deliberately a plain table rather than
/// a borrow of `GapField`: S8b needs three facts per face and nothing else, and
/// keeping it flat is what lets the lookups stay pure functions of a node - which
/// is what Invariant B1 requires of the collapse decision.
#[derive(Clone, Debug, Default)]
pub struct ThinContext {
    /// Per arranged triangle, the thin region owning it (**either** wall), `-1` none.
    ///
    /// A region owns both walls of its gap, so wall A and wall B map to the same id
    /// and a band cell resolves the same region from either of its lids.
    pub region_of_face: Vec<i32>,
    /// Per region, the regime in force *after* the S3/S4 coupling and the §10.11
    /// ladder - not `ThinRegion::regime`, which is what the thresholds alone said.
    pub regime: Vec<ThinRegime>,
    /// Per region, its S3 pair class, as the `SPEC_meshgen_contracts.md` §2.3 code.
    pub pair_class: Vec<u8>,
    /// The converged sheet threshold, in the same units as the node coordinates.
    pub t_sheet: f64,
    /// `meshgen.thin.collapse_sheets`. Off means S8 measures and reports thin regions
    /// but never fuses their walls - see the config's note for what is not finished.
    pub collapse_sheets: bool,
}

/// The thin regime of a region, mirrored here so `thin.rs` does not depend on S3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ThinRegime {
    #[default]
    Normal,
    Band,
    Sheet,
}

impl ThinContext {
    // AI-FUNC-SUMMARY:
    // Purpose: The thin region owning an arranged face; returns the region id or None; side effects: none.
    pub fn region_of(&self, face: u32) -> Option<usize> {
        match self.region_of_face.get(face as usize).copied() {
            Some(id) if id >= 0 => Some(id as usize),
            _ => None,
        }
    }

    // AI-FUNC-SUMMARY: The effective regime of a region; returns ThinRegime (Normal when unknown); side effects: none.
    pub fn regime_of(&self, region: usize) -> ThinRegime {
        self.regime.get(region).copied().unwrap_or_default()
    }

    // AI-FUNC-SUMMARY: The §2.3 pair-class code of a region; returns u8 (0 = unpaired when unknown); side effects: none.
    pub fn pair_class_of(&self, region: usize) -> u8 {
        self.pair_class.get(region).copied().unwrap_or(0)
    }
}
