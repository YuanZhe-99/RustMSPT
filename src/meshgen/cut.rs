//! S8 - the cut (G6-2, G6-3, G6-5).
//!
//! S7 moved the lattice onto the geometry's features; S8 is what makes the mesh
//! *conform*. Every cell an active patch passes through is replaced by pieces that
//! stop exactly at the patch, so the surface becomes a union of element faces.
//!
//! Three frozen tables do the work, all from `SPEC_meshgen_geometry.md`:
//!
//! - **§4** the one diagonal rule (SNK) for quads, and the six non-cyclic prism
//!   patterns it guarantees;
//! - **§5.2** the kirigami face-split table - what a *face* looks like after the cut;
//! - **§6** the six single-patch tet cases - what a *cell* becomes.
//!
//! **Two frozen remedies are not implemented, by decision rather than oversight.**
//! §5.1's K1 remedy is "refine one level, and if the sizing floor is reached, hand
//! the cells to §7"; the refine step is skipped and such cells go straight to §7.
//! §4.4's ladder is likewise entered at step 3 - a piece that fails the floor
//! escalates rather than trying a pairwise diagonal flip (step 1) or a per-piece
//! Steiner fan (step 2). Both shortcuts are safe in the same way: §7's path is
//! total, so the mesh is always valid and conforming, and the cost is a chamfer of
//! at most one cell in the affected cells - 0.53 % of them on the review scene,
//! logged per cell. Implementing the skipped rungs would recover geometry in those
//! cells; they are recorded in `PLAN_mesh_generation.md` §0.2 as open work.
//!
//! The reason the cut conforms with no communication between cells is Invariant C
//! (`SPEC_meshgen_geometry.md` §1.3): every choice is a function of node keys and of
//! the cut state, both of which two cells sharing a face agree on. §6's tables are
//! built so that each piece's boundary triangles on a parent face are exactly what
//! §5.2 produces for that face - and that is checked here, not assumed.

use crate::io::vtu::{ArrayData, DataArray, VtuDoc, VTK_POLY_LINE, VTK_TETRA, VTK_TRIANGLE};
use crate::meshgen::arrange::ArrangeComponent;
use crate::meshgen::classify::{
    resolve, Classification, OwnershipRecord, PointClassifier, Provenance, Side,
};
use crate::meshgen::lattice::Lattice;
use crate::meshgen::predicates::{node_key, orient3d, tet_quality};
use crate::meshgen::junction::{
    chord_meeting_point, crossed_face_mesh, face_centroid, face_mesh, fan_cell, fan_polygon,
    loop_fan, soup_is_closed, split_soup_by_surface, split_soup_components, CrossedFace, FaceMesh,
    TET_FACES,
};
use crate::meshgen::snap::{EdgeCrossing, Snapped};
use crate::meshgen::thin::{
    band_face_split, mesh_band_cell, snk_cell_diagonals, split_band_cell, BandCellPlan,
    BandDecline, BandPair, BandTemplate, Slab, ThinContext, ThinRegime,
};
use crate::types::Vec3;
use rayon::prelude::*;
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet};

/// The guarded dry-run's volume tolerance: the children of one cut must reproduce
/// the parent's volume to within this fraction (`SPEC_meshgen_geometry.md` §6).
pub const CUT_VOLUME_TOLERANCE: f64 = 0.01;

/// The §4.4 runtime dihedral floor for a quad- or prism-derived tet, in degrees.
pub const CUT_MIN_DIHEDRAL_DEG: f64 = 8.0;

/// One slab of an escalated cell: its closed boundary, and the matched pairs when it
/// is the gap slab a §8.2 row may mesh.
type CellSlabs = SmallVec<[(Vec<[u32; 3]>, Option<[BandPair; 3]>); 3]>;

/// The `regime` code of an ordinary element (`SPEC_meshgen_contracts.md` §2.1).
pub const REGIME_NORMAL: u8 = 0;

/// How much finer than the weld grid S8's ordering key is (see `cut_lattice`).
pub const KEY_ORDER_REFINEMENT: f64 = 1.0e-6;

/// A node key in the canonical frame - the total order every tie-break uses.
pub type NodeKey = (i64, i64, i64);

/// The four faces of one cell: each as the §5.2 state where that is expressible,
/// and as an ordered boundary loop always.
type CellFaces = [(Option<FaceCutState>, SmallVec<[u32; 8]>, Option<CrossedFace>); 4];


/// Where a parent node sits relative to the patch being cut (`SPEC_meshgen_geometry.md` §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeSide {
    /// Strictly inside the patch's positive side.
    Inside,
    /// Strictly outside.
    Outside,
    /// On the patch - S7 put it there.
    OnCut,
}

/// Why a cell could not take the single-patch path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Escalation {
    /// >= 2 active patches, or an intersection-curve segment: a junction cell (§7.1).
    Junction,
    /// Invariant K1: an edge this component crosses more than once.
    MultiCrossing,
    /// An I-O edge with no crossing, or a crossing on an edge that does not straddle:
    /// S6 and S7 disagree about this cell.
    Inconsistent,
    /// The guarded dry-run rejected the children (ARB-15).
    DryRun,
    /// A prism or quad piece failed the §4.4 floor and the ladder ran out.
    Quality,
}

/// One tagged interface triangle: a face of the cut that lies on a patch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InterfaceFace {
    pub nodes: [u32; 3],
    /// The component whose patch this is.
    pub component: i32,
    /// `SPEC_meshgen_contracts.md` §2.3 `FaceTagKind`: 0 interface, 1 sheet, 2 box cap.
    pub kind: u8,
    /// `(elem⁺, elem⁻)`. For a solid patch that is (inside `component`, outside it).
    /// A **sheet** has no inside, so the pair is ordered geometrically instead - see
    /// `derive_interface` - and `-1` marks a side with no adjacent tet.
    pub side_elems: [i32; 2],
}

/// `CurveKind` value for a rim (`SPEC_meshgen_contracts.md` §2.3).
pub const CURVE_KIND_RIM: u8 = 1;

/// `FaceTagKind` values (`SPEC_meshgen_contracts.md` §2.3).
pub const FACE_TAG_INTERFACE: u8 = 0;
pub const FACE_TAG_SHEET: u8 = 1;
pub const FACE_TAG_BOX_CAP: u8 = 2;

/// S8 diagnostics.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CutStats {
    pub n_parents: usize,
    pub n_cut_cells: usize,
    pub n_uncut_cells: usize,
    pub n_cut_nodes: usize,
    pub n_tets: usize,
    pub n_interface_faces: usize,
    /// Cells `SPEC_meshgen_geometry.md` §6's table could not decide from their vertices -
    /// every vertex *on* the patch - and an interior sample gave to the body. S7's snapping
    /// makes this configuration systematic near a corner or a sharp edge.
    pub n_interior_sampled_cells: usize,
    pub n_case_a: usize,
    pub n_case_b: usize,
    pub n_case_c: usize,
    pub n_case_d: usize,
    /// Cells cut by a welded sheet rather than by a solid patch (G6-5).
    pub n_welded_sheet_cells: usize,
    /// Cells handed to the junction path or left uncut, by reason.
    pub n_escalated: BTreeMap<Escalation, usize>,
    /// Escalated cells re-meshed as a centroid fan over a conforming boundary (G6-4).
    pub n_steiner_fans: usize,
    /// Escalated cells the G7-1 doubly-cut face rule split into three slabs instead,
    /// so their thin gap survives as elements rather than being chamfered away.
    pub n_band_cells: usize,
    /// G7-2: lattice edges whose two wall crossings collapsed to one shared rim node
    /// (`SPEC_meshgen_geometry.md` §8.2, the `k = 3` row).
    pub n_collapsed_pairs: usize,
    /// Escalated cells the rule declined, by reason - so "it did not fire" is always
    /// answerable from a run's own output.
    pub n_band_declined: BTreeMap<BandDecline, usize>,
    /// Faces the doubly-cut rule triangulated (counted per incident cell).
    pub n_doubly_cut_faces: usize,
    /// Gap slabs by the §8.2 row that meshed them.
    pub n_band_templates: BTreeMap<BandTemplate, usize>,
    /// Worst min dihedral over the band elements the table produced.
    pub band_min_dihedral_deg: f64,
    /// Faces of those cells whose state the frozen §5.2 table does not cover, so the
    /// deterministic boundary fan was used instead.
    pub n_generic_faces: usize,
    /// Faces two patches cross, triangulated with both chords as edges (§7.6's precondition).
    pub n_crossed_faces: usize,
    /// §7.6: escalated cells the sequential cut split instead of fanning whole.
    pub n_junction_splits: usize,
    /// The material pieces those splits produced.
    pub n_junction_split_pieces: usize,
    /// Fan pieces that came out exactly degenerate.
    pub n_degenerate_fan_pieces: usize,
    /// Escalated pieces whose ownership was settled by a §7.5 interior sample rather
    /// than inherited from an unresolved parent record.
    pub n_seeded_pieces: usize,
    /// Of those, the ones the §6.1 static filter could not certify, so the exact
    /// predicate ran.
    pub n_seed_uncertain: usize,
    pub min_dihedral_deg: f64,
    pub worst_volume_error: f64,
}

/// One locked curve of the arrangement, as S8 needs it (`SPEC_meshgen_contracts.md` §2.3).
#[derive(Clone, Debug, PartialEq)]
pub struct LockedCurve {
    /// §2.3 `CurveKind`: 0 sharp, 1 rim, 2 intersection, 3 box.
    pub kind: u8,
    /// The components that meet along the curve - the set `[V9]` requires every mesh node
    /// on the curve to carry in its `N_ID`.
    pub components: SmallVec<[i32; 2]>,
    /// How many arranged patches S2's `radial_patch_order` put around the curve. The
    /// number of material sectors the mesh must reproduce around an edge lying on it.
    pub radial_patches: u32,
    /// The polyline, in lattice coordinates.
    pub segments: Vec<(Vec3, Vec3)>,
}

/// The result of S8.
#[derive(Clone, Debug)]
pub struct CutMesh {
    /// S7's nodes followed by the cut nodes, in that order - a node index below
    /// `n_snapped` still means the same node it did in S5.
    pub nodes: Vec<Vec3>,
    /// How many of `nodes` are S5/S7 lattice nodes. Everything at or above this index was
    /// interned by the cut — an edge crossing, a face Steiner point, a piece centroid. Phase
    /// P-4 needs the distinction on the *nodes*, not on the cells: a material boundary whose
    /// corners are all unsnapped lattice nodes is a staircase, and one whose corners are cut
    /// nodes is a cut in the wrong place. `provenance` is per element and cannot say which.
    pub n_lattice_nodes: u32,
    pub tets: Vec<[u32; 4]>,
    /// Per tet, its ownership record (`Provenance::Cut` where the cut wrote it).
    pub records: Vec<OwnershipRecord>,
    /// Per tet, the S5 cell it came from.
    pub parent_of: Vec<u32>,
    /// Per tet, its `regime` code (`SPEC_meshgen_contracts.md` §2.1): 0 normal,
    /// 1 band, 2 band-Steiner.
    pub regime: Vec<u8>,
    /// Per tet, the S3 thin region it belongs to (`SPEC_meshgen_contracts.md` §2.1
    /// `band_region`); `-1` for every element that is not a band element, and for a
    /// band element whose walls S3 never attributed to a region.
    pub band_region: Vec<i32>,
    /// Per S3 thin region, its `ThinRegionRegime` and `ThinRegionPairClass` codes
    /// (`SPEC_meshgen_contracts.md` §2.3), so a `band_region` in the mesh can be
    /// resolved to the gap it names without also carrying s03. Empty when the cut ran
    /// with no `ThinContext`.
    pub thin_regime: Vec<u8>,
    pub thin_pair_class: Vec<u8>,
    /// G7-2: the rim of the collapsed sheet, as mesh edges - `SPEC_meshgen_contracts.md`
    /// §2.3 `CurveKind = 1`. Emitted as line cells so a sheet's boundary is a declared
    /// curve and `[V8]`'s pinhole check measures what it is meant to.
    pub rim_curve: Vec<[u32; 2]>,
    /// Per node, S7's `SPEC_meshgen_contracts.md` §2.2 `constraint_kind` / `constraint_ref`,
    /// extended with `Free`/`-1` for the cut and Steiner nodes S8 adds. `cut_to_doc` wrote
    /// both as stubs until 2026-08-13, which is the same defect `n_id_key` carried: an array
    /// the contract declares, emitted with no content, so anything reading it reads zeros.
    pub constraint_kind: Vec<u8>,
    pub constraint_ref: Vec<i32>,
    /// The locked curves as they survive **in the mesh**: per curve, the mesh edges that
    /// lie along it. This is what makes `[V9]` non-vacuous - a curve with no edges here is
    /// declared and not carried, which is a finding rather than an absence of one.
    pub curve_edges: Vec<(u32, [u32; 2])>,
    /// The curve table itself, in the order `curve_edges` indexes.
    pub curves: Vec<LockedCurve>,
    /// The cut surface, as tagged triangles with their two side elements.
    pub interfaces: Vec<InterfaceFace>,
    /// Cells the single-patch path could not take, with the reason - G6-4's input.
    pub escalated: Vec<(u32, Escalation)>,
    pub warnings: Vec<String>,
    pub stats: CutStats,
}

/// Inputs to S8.
#[derive(Clone, Debug)]
pub struct CutOptions {
    pub eps: f64,
    /// The guarded dry-run's volume tolerance (default `CUT_VOLUME_TOLERANCE`).
    pub volume_tolerance: f64,
    /// The §4.4 dihedral floor in degrees (default `CUT_MIN_DIHEDRAL_DEG`).
    pub min_dihedral_deg: f64,
    /// Whether S8b's doubly-cut face rule may split a sandwiched cell into three
    /// slabs instead of fanning it whole (`meshgen.thin.enabled`).
    pub bands: bool,
    /// The arranged **rim** curves (`ArrangedCurveKind::Rim`), as polyline segments in
    /// lattice coordinates: the boundary of every open sheet. S8 declares a sheet's
    /// boundary edge a rim only when both its endpoints lie on one of these, so the
    /// declaration is a geometric fact and `[V8]`'s pinhole check keeps its teeth.
    pub rim_segments: Vec<(Vec3, Vec3)>,
    /// S2's **coincident** arranged patches: the triangles whose `ArrangedFace::components`
    /// carries more than one component, in lattice coordinates, each with that set. Two
    /// solids in exact face contact produce one arranged face belonging to both, and S2
    /// records it (A-6's cube and limb share the plane x = 0.5817, where S2 emits five
    /// faces tagged `{1, 2}`); everything downstream then works from the single
    /// `ArrangedFace::component`, so the shared boundary reaches S8 as one component's and
    /// the mesh face between the two bodies ends up declared for neither. Empty is the
    /// ordinary case and costs nothing.
    pub contact_patches: Vec<([Vec3; 3], SmallVec<[i32; 2]>)>,
    /// Gate **G6-0**: every locked curve of the arrangement as polyline segments in
    /// lattice coordinates - sharp edges, intersection curves, rims, non-manifold edges.
    /// A lattice face one of these pierces is **not** expressible by §5.2, whatever its
    /// cut state: the material boundary runs along the curve, and a triangulation with no
    /// vertex on it can only chamfer past it. S8 interns the piercing point per face and
    /// hands the face to §7.6. Empty is the ordinary case and costs nothing.
    pub curve_segments: Vec<(Vec3, Vec3)>,
    /// The same curves, kept whole, with what `SPEC_meshgen_contracts.md` §2.3's curve
    /// table has to declare about each: its kind, the components that meet along it, and
    /// how many arranged patches S2 ordered radially around it. `curve_segments` is the
    /// flattened geometry of exactly these and is what the piercing tests read; this is
    /// what the *table* is written from and what `[V9]` reads back. Empty leaves the table
    /// carrying rims alone, which is what every unit fixture with no S2 stage wants.
    pub locked_curves: Vec<LockedCurve>,
    /// S3's thin regions, reduced to lookups by arranged face (G7-2). `None` leaves
    /// every band element unattributed (`band_region = -1`) and disables collapse,
    /// which is what every unit fixture that has no S3 stage wants.
    pub thin: Option<ThinContext>,
}

impl Default for CutOptions {
    // AI-FUNC-SUMMARY: The frozen tolerances; returns CutOptions; side effects: none.
    fn default() -> Self {
        CutOptions {
            eps: 1.0e-9,
            volume_tolerance: CUT_VOLUME_TOLERANCE,
            min_dihedral_deg: CUT_MIN_DIHEDRAL_DEG,
            bands: true,
            rim_segments: Vec::new(),
            curve_segments: Vec::new(),
            locked_curves: Vec::new(),
            contact_patches: Vec::new(),
            thin: None,
        }
    }
}

// ---------------------------------------------------------------------------
// §4 - the one diagonal rule
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Rule SNK - split a quad on the diagonal incident to its smallest-key vertex.
// Inputs: the quad's four nodes **in cyclic order**, and the key table.
// Returns: two triangles, `(lo, next, opp)` and `(lo, opp, prev)`.
// Side effects: None.
// Notes: This is Invariant C in miniature: the choice depends only on the four node keys, so both
//   cells sharing the quad pick the same diagonal, and so does the same quad reached through a
//   different template. `SPEC_meshgen_geometry.md` §4.1 - and §4.2's Theorem T2 (a prism is always
//   decomposable) holds *because* of it, which is why the reference implementation's distance-based
//   tie-break was not ported (§15 row D-3).
pub fn snk_split_quad(quad: [u32; 4], keys: &[NodeKey]) -> [[u32; 3]; 2] {
    let mut lo = 0usize;
    for slot in 1..4 {
        if keys[quad[slot] as usize] < keys[quad[lo] as usize] {
            lo = slot;
        }
    }
    let next = (lo + 1) % 4;
    let opp = (lo + 2) % 4;
    let prev = (lo + 3) % 4;
    [
        [quad[lo], quad[next], quad[opp]],
        [quad[lo], quad[opp], quad[prev]],
    ]
}

// AI-FUNC-SUMMARY:
// Purpose: Which diagonal Rule SNK chooses on one quad.
// Returns: true when the diagonal joins `quad[0]`-`quad[2]`, false when it joins `quad[1]`-`quad[3]`.
// Side effects: None.
pub fn snk_diagonal_is_02(quad: [u32; 4], keys: &[NodeKey]) -> bool {
    let mut lo = 0usize;
    for slot in 1..4 {
        if keys[quad[slot] as usize] < keys[quad[lo] as usize] {
            lo = slot;
        }
    }
    lo.is_multiple_of(2)
}

// AI-FUNC-SUMMARY:
// Purpose: Decompose a 3-sided prism into three tets under Rule SNK (§4.2, §4.3).
// Inputs: the two caps `(a0,a1,a2)` and `(b0,b1,b2)` with `a_i <-> b_i`, and the key table.
// Returns: three tets, or None if the diagonals came out cyclic.
// Side effects: None.
// Notes: Theorem T2 says None is unreachable under SNK - the globally smallest key lies on two of
//   the three quads and is the smallest key within each, so it is an endpoint of two diagonals,
//   while in a cyclic set every vertex is an endpoint of exactly one. It is still returned rather
//   than asserted, because a caller that passes non-SNK diagonals (the §4.4 ladder's flip step
//   does exactly that) can reach it.
pub fn prism_tets(a: [u32; 3], b: [u32; 3], keys: &[NodeKey]) -> Option<[[u32; 4]; 3]> {
    // Quad Q_ij = (a_i, a_j, b_j, b_i); `true` = the diagonal through a_i.
    let d01 = snk_diagonal_is_02([a[0], a[1], b[1], b[0]], keys);
    let d12 = snk_diagonal_is_02([a[1], a[2], b[2], b[1]], keys);
    let d20 = snk_diagonal_is_02([a[2], a[0], b[0], b[2]], keys);
    prism_tets_with_diagonals(a, b, [d01, d12, d20])
}

// AI-FUNC-SUMMARY:
// Purpose: The frozen six-pattern prism table (§4.3), indexed by the three diagonal choices.
// Inputs: the caps and, per quad `Q01/Q12/Q20`, whether the diagonal runs through the *first* cap
//   vertex of that quad (`a_i-b_j` form).
// Returns: three tets, or None for the two cyclic sets.
// Side effects: None.
pub fn prism_tets_with_diagonals(
    a: [u32; 3],
    b: [u32; 3],
    diagonals: [bool; 3],
) -> Option<[[u32; 4]; 3]> {
    let (a0, a1, a2) = (a[0], a[1], a[2]);
    let (b0, b1, b2) = (b[0], b[1], b[2]);
    // `true` on Q01 means the diagonal is a0-b1, `false` means a1-b0, and so on
    // around the prism; all three equal is one of the two cyclic sets.
    Some(match diagonals {
        [true, true, true] | [false, false, false] => return None,
        [false, false, true] => [
            [a0, a1, a2, b0],
            [a1, a2, b0, b1],
            [a2, b0, b1, b2],
        ],
        [false, true, false] => [
            [a0, a1, a2, b2],
            [a0, a1, b0, b2],
            [a1, b0, b1, b2],
        ],
        [false, true, true] => [
            [a0, a1, a2, b0],
            [a1, a2, b0, b2],
            [a1, b0, b1, b2],
        ],
        [true, false, false] => [
            [a0, a1, a2, b1],
            [a0, a2, b1, b2],
            [a0, b0, b1, b2],
        ],
        [true, false, true] => [
            [a0, a1, a2, b1],
            [a0, a2, b0, b1],
            [a2, b0, b1, b2],
        ],
        [true, true, false] => [
            [a0, a1, a2, b2],
            [a0, a1, b1, b2],
            [a0, b0, b1, b2],
        ],
    })
}

// ---------------------------------------------------------------------------
// §5.2 - the kirigami face-split table
// ---------------------------------------------------------------------------

/// One parent face's cut state, in the canonical frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FaceCutState {
    /// The face's three nodes, **sorted by `NodeKey`** - the canonical frame.
    pub nodes: [u32; 3],
    /// Per edge `(nodes[i], nodes[(i+1)%3])`, the cut node on it, if any.
    pub cut: [Option<u32>; 3],
    /// Per node, whether it lies on the patch.
    pub on_cut: [bool; 3],
    /// An interior rim endpoint, for open sheets (case `split_R`).
    pub rim: Option<u32>,
}

// AI-FUNC-SUMMARY:
// Purpose: Check one face's triangulation against `SPEC_meshgen_geometry.md` §7.3's cache, so a
//   disagreement between the two cells sharing it is caught here instead of surfacing as a crack.
// Inputs: the cache and its conflict counter, the face's boundary walk, the parent-node cutoff, the
//   key table, the face's crossing record, and the triangles this cell produced for it.
// Returns: nothing.
// Side effects: Inserts on first sight of a face; increments the counter on disagreement.
// Notes: A free function rather than an inline block because the face loop leaves by three
//   different routes - the doubly-cut band rule, the curve-node hub fan, and the ordinary match -
//   and a check that covered only the last would be blind to exactly the crease-carrying paths the
//   cache exists for. The key is the face's three parent corners and the fingerprint is the
//   components crossing it, both pure functions of the face (invariant J1).
#[allow(clippy::too_many_arguments)]
fn check_face_cache(
    cache: &mut crate::meshgen::facecache::FaceTriCache,
    conflicts: &mut usize,
    loop_nodes: &[u32],
    n_parent_nodes: u32,
    keys: &[NodeKey],
    points: &[Vec3],
    crossed: Option<&CrossedFace>,
    crease: bool,
    mine: &[[u32; 3]],
) -> Option<Vec<[u32; 3]>> {
    if mine.is_empty() || loop_nodes.is_empty() {
        return None;
    }
    let mut corners = [loop_nodes[0]; 3];
    let mut slot = 0usize;
    for node in loop_nodes {
        if (*node as usize) < n_parent_nodes as usize && slot < 3 {
            corners[slot] = *node;
            slot += 1;
        }
    }
    if slot != 3 {
        return None;
    }
    corners.sort_by_key(|node| keys[*node as usize]);
    let mut fingerprint: Vec<i64> = crossed
        .map(|face| face.components.iter().map(|x| *x as i64).collect())
        .unwrap_or_default();
    fingerprint.sort_unstable();
    fingerprint.dedup();
    // P-3.3's constraint set. A crease-fanned face is triangulated from a point no component's
    // chords name, so the components alone no longer describe what the face is constrained by;
    // without this a cell that fanned and a cell that took §5.2's table would present the same
    // fingerprint and their disagreement would be reported as a plain triangle mismatch instead
    // of §7.3's hard error. Out of band from any component id on purpose.
    if crease {
        fingerprint.push(i64::MIN);
    }
    // **Canonical winding, so the comparison covers orientation and not just membership.**
    // §5.2 re-orients each sub-triangle to the *calling cell's* winding, so the two cells sharing
    // a face legitimately emit opposite windings and a naive triple comparison would report every
    // shared face as a conflict. Both are put into the face's own frame first: the reference
    // normal is `(b-a) x (c-a)` over the face's key-sorted corners - a pure function of the face -
    // and every triangle is flipped to agree with it, then rotated to start at its smallest node.
    // What survives is the triangulation *as the face sees it*, which is what the cache must store
    // if it is ever to hand triangles to the second cell rather than merely check them.
    let canonical = |tris: &[[u32; 3]]| -> Vec<[u32; 3]> {
        let (a, b, c) = (
            points[corners[0] as usize],
            points[corners[1] as usize],
            points[corners[2] as usize],
        );
        let reference = b.sub(a).cross(c.sub(a));
        let mut out: Vec<[u32; 3]> = tris
            .iter()
            .map(|t| {
                let (p, q, r) = (
                    points[t[0] as usize],
                    points[t[1] as usize],
                    points[t[2] as usize],
                );
                let mut t = if q.sub(p).cross(r.sub(p)).dot(reference) < 0.0 {
                    [t[1], t[0], t[2]]
                } else {
                    *t
                };
                // Rotate to the smallest node, preserving the cycle, so the same triangle
                // written from two different starting corners compares equal.
                let at = (0..3).min_by_key(|slot| t[*slot]).unwrap_or(0);
                t = [t[at], t[(at + 1) % 3], t[(at + 2) % 3]];
                t
            })
            .collect();
        out.sort_unstable();
        out
    };
    let key = crate::meshgen::facecache::face_key(corners, keys);
    let owned = canonical(mine);
    match cache.get_or_insert(key, fingerprint, || owned.clone()) {
        Ok(cached) => {
            if cached != owned.as_slice() {
                *conflicts += 1;
                return None;
            }
            Some(cached.to_vec())
        }
        // §7.3's hard error: the two cells disagree about what crosses the face they share.
        // Counted rather than raised, because the caller falls back to its own triangles.
        Err(_) => {
            *conflicts += 1;
            None
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Re-orient a face's canonical triangles to one cell's outward winding.
// Inputs: the canonical triangles, the cell's four nodes, and the coordinates.
// Returns: the triangles wound outward from this cell.
// Side effects: None.
// Notes: The cache stores each face once, in the face's own frame; the boundary soup needs it in
//   the *cell's*, because `fan_volume` and `polygon_soup_centroid` read the winding and a flipped
//   face makes a piece's volume negative. The apex - the cell node not on this face - decides it:
//   a face triangle is outward when its normal points away from the apex. That is a pure function
//   of the cell and the face, so the two cells sharing a face take the same stored triangles and
//   independently arrive at their own correct, opposite windings.
fn orient_face_outward(tris: &[[u32; 3]], tet: [u32; 4], points: &[Vec3]) -> Vec<[u32; 3]> {
    tris.iter()
        .map(|t| {
            let Some(&apex) = tet.iter().find(|node| !t.contains(node)) else {
                return *t;
            };
            let (p, q, r) = (
                points[t[0] as usize],
                points[t[1] as usize],
                points[t[2] as usize],
            );
            let outward = q.sub(p).cross(r.sub(p)).dot(points[apex as usize].sub(p)) < 0.0;
            if outward {
                *t
            } else {
                [t[1], t[0], t[2]]
            }
        })
        .collect()
}

/// P-3.9's counters: cap-loop nodes inside §7.6's own on-surface set against those outside it.
static CAP_LOOP_LATTICE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CAP_LOOP_INTERNED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

// AI-FUNC-SUMMARY:
// Purpose: The frozen §5.2 face-split table - how one parent face is triangulated after the cut.
// Inputs: the face's cut state in the canonical frame, and the key table for Rule SNK.
// Returns: the sub-triangles, or None for an illegal state (a dangling cut end).
// Side effects: None.
// Notes: The whole point of this function is that it is a *pure function of the face*: two cells
//   sharing a face compute it independently and get the same triangles, which is what makes the cut
//   conforming with no inter-cell communication. It is also the reference §7's generic constrained
//   triangulator must reproduce (Invariant J2).
pub fn face_split(state: &FaceCutState, keys: &[NodeKey]) -> Option<SmallVec<[[u32; 3]; 4]>> {
    let [a, b, c] = state.nodes;
    let cuts: SmallVec<[usize; 3]> = (0..3).filter(|edge| state.cut[*edge].is_some()).collect();
    let on_cut_count = state.on_cut.iter().filter(|flag| **flag).count();
    let mut out: SmallVec<[[u32; 3]; 4]> = SmallVec::new();
    match (cuts.len(), state.rim) {
        (0, _) => out.push([a, b, c]),
        (1, Some(rim)) => {
            // split_R: the cut front ends at an interior point of the face (R-C1).
            let edge = cuts[0];
            let (u, v) = (state.nodes[edge], state.nodes[(edge + 1) % 3]);
            let w = state.nodes[(edge + 2) % 3];
            let m = state.cut[edge].unwrap();
            out.push([u, m, rim]);
            out.push([m, v, rim]);
            out.push([v, w, rim]);
            out.push([w, u, rim]);
        }
        (1, None) => {
            // split_2: one cut edge and the opposite vertex on the patch.
            let edge = cuts[0];
            let w_slot = (edge + 2) % 3;
            if !state.on_cut[w_slot] {
                // A cut segment with a dangling end - illegal (§5.2).
                return None;
            }
            let (u, v) = (state.nodes[edge], state.nodes[(edge + 1) % 3]);
            let w = state.nodes[w_slot];
            let m = state.cut[edge].unwrap();
            out.push([u, m, w]);
            out.push([m, v, w]);
        }
        (2, _) => {
            // split_3: `(n1, n2)` is the uncut edge, `n3` the opposite vertex.
            let uncut = (0..3).find(|edge| state.cut[*edge].is_none()).unwrap();
            let n1 = state.nodes[uncut];
            let n2 = state.nodes[(uncut + 1) % 3];
            let n3 = state.nodes[(uncut + 2) % 3];
            // `x1` on `(n3,n1)` is the cut of the edge ending at n1; `x2` on `(n2,n3)`.
            let x1 = state.cut[(uncut + 2) % 3].unwrap();
            let x2 = state.cut[(uncut + 1) % 3].unwrap();
            out.push([n3, x1, x2]);
            for triangle in snk_split_quad([n1, n2, x2, x1], keys) {
                out.push(triangle);
            }
        }
        (3, _) => {
            // split_4: the medial split. Non-generic - the caller logs [CUT-3EDGE].
            let m_ab = state.cut[0].unwrap();
            let m_bc = state.cut[1].unwrap();
            let m_ca = state.cut[2].unwrap();
            out.push([a, m_ab, m_ca]);
            out.push([b, m_bc, m_ab]);
            out.push([c, m_ca, m_bc]);
            out.push([m_ab, m_bc, m_ca]);
        }
        _ => return None,
    }
    let _ = on_cut_count;
    Some(out)
}

// ---------------------------------------------------------------------------
// §6 - the single-patch tet case table
// ---------------------------------------------------------------------------

/// One cut cell's replacement: the tets, whether each is inside the patch, and the
/// interface triangles between them.
#[derive(Clone, Debug, Default)]
pub struct CellCut {
    pub tets: SmallVec<[[u32; 4]; 6]>,
    pub inside: SmallVec<[bool; 6]>,
    pub interface: SmallVec<[[u32; 3]; 2]>,
    pub case: u8,
}

// AI-FUNC-SUMMARY:
// Purpose: The frozen §6 table - replace one tet cut by one patch with its pieces.
// Inputs: the parent's four nodes, each node's side, the cut node on each `(inside, outside)` edge
//   (indexed by the parent's node slots), and the key table.
// Returns: the pieces, or None when the configuration is not one of the six legal cases.
// Side effects: None.
// Notes: The interface triangles are emitted **once** and consumed by both sides, which is why the
//   pieces partition the parent volume exactly rather than to within a tolerance. Every quad and
//   prism goes through §4's SNK rule, so a boundary triangle of a piece that lies on a parent face
//   is exactly what `face_split` produces for that face - the conformity claim of §6, checked by
//   `tests/meshgen_cut_tests.rs` rather than assumed.
pub fn cut_tet(
    tet: [u32; 4],
    sides: [NodeSide; 4],
    cut_node: &dyn Fn(usize, usize) -> Option<u32>,
    keys: &[NodeKey],
) -> Option<CellCut> {
    let slots = |want: NodeSide| -> SmallVec<[usize; 4]> {
        (0..4).filter(|slot| sides[*slot] == want).collect()
    };
    let inside_slots = slots(NodeSide::Inside);
    let outside_slots = slots(NodeSide::Outside);
    let on_cut_slots = slots(NodeSide::OnCut);
    if inside_slots.is_empty() || outside_slots.is_empty() {
        return None;
    }
    let node = |slot: usize| tet[slot];
    let mut out = CellCut::default();

    match (
        inside_slots.len(),
        outside_slots.len(),
        on_cut_slots.len(),
    ) {
        // Case A - (1, 1, 2)
        (1, 1, 2) => {
            let i = inside_slots[0];
            let o = outside_slots[0];
            let m = cut_node(i, o)?;
            let (c1, c2) = (node(on_cut_slots[0]), node(on_cut_slots[1]));
            out.tets.push([node(i), c1, c2, m]);
            out.inside.push(true);
            out.tets.push([node(o), c1, c2, m]);
            out.inside.push(false);
            out.interface.push([c1, c2, m]);
            out.case = b'A';
        }
        // Case B - (1, 2, 1) and its mirror B' - (2, 1, 1)
        (1, 2, 1) | (2, 1, 1) => {
            let mirrored = inside_slots.len() == 2;
            let (single, pair) = if mirrored {
                (outside_slots[0], [inside_slots[0], inside_slots[1]])
            } else {
                (inside_slots[0], [outside_slots[0], outside_slots[1]])
            };
            let m1 = cut_node(single, pair[0])?;
            let m2 = cut_node(single, pair[1])?;
            let c = node(on_cut_slots[0]);
            out.tets.push([node(single), m1, m2, c]);
            out.inside.push(!mirrored);
            for triangle in snk_split_quad([m1, node(pair[0]), node(pair[1]), m2], keys) {
                out.tets
                    .push([triangle[0], triangle[1], triangle[2], c]);
                out.inside.push(mirrored);
            }
            out.interface.push([m1, m2, c]);
            out.case = if mirrored { b'b' } else { b'B' };
        }
        // Case C - (1, 3, 0) and its mirror C' - (3, 1, 0)
        (1, 3, 0) | (3, 1, 0) => {
            let mirrored = inside_slots.len() == 3;
            let (single, triple) = if mirrored {
                (
                    outside_slots[0],
                    [inside_slots[0], inside_slots[1], inside_slots[2]],
                )
            } else {
                (
                    inside_slots[0],
                    [outside_slots[0], outside_slots[1], outside_slots[2]],
                )
            };
            let m = [
                cut_node(single, triple[0])?,
                cut_node(single, triple[1])?,
                cut_node(single, triple[2])?,
            ];
            out.tets.push([node(single), m[0], m[1], m[2]]);
            out.inside.push(!mirrored);
            let far = [node(triple[0]), node(triple[1]), node(triple[2])];
            for piece in prism_tets(m, far, keys)? {
                out.tets.push(piece);
                out.inside.push(mirrored);
            }
            out.interface.push([m[0], m[1], m[2]]);
            out.case = if mirrored { b'c' } else { b'C' };
        }
        // Case D - (2, 2, 0)
        (2, 2, 0) => {
            let (i1, i2) = (inside_slots[0], inside_slots[1]);
            let (o1, o2) = (outside_slots[0], outside_slots[1]);
            let m11 = cut_node(i1, o1)?;
            let m12 = cut_node(i1, o2)?;
            let m21 = cut_node(i2, o1)?;
            let m22 = cut_node(i2, o2)?;
            for piece in prism_tets([node(i1), m11, m12], [node(i2), m21, m22], keys)? {
                out.tets.push(piece);
                out.inside.push(true);
            }
            for piece in prism_tets([node(o1), m11, m21], [node(o2), m12, m22], keys)? {
                out.tets.push(piece);
                out.inside.push(false);
            }
            // The interface is the quad the two prisms share, split by the same
            // rule from both sides.
            for triangle in snk_split_quad([m11, m12, m22, m21], keys) {
                out.interface.push(triangle);
            }
            out.case = b'D';
        }
        _ => return None,
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// The guarded dry-run (§6, ARB-15)
// ---------------------------------------------------------------------------


// AI-FUNC-SUMMARY:
// Purpose: Attribute every conformity defect in the cut mesh to the cell and node class that
//   produced it, so `[V3]` failures can be chased to a cause instead of counted.
// Inputs: the finished cut mesh and the two node-id boundaries (parent nodes below the first, cut
//   nodes below the second, Steiner points above it).
// Returns: nothing.
// Side effects: Prints a `[CUT-DIAG]` block. Enabled by `RUSTMSPT_CUT_DIAG`.
// Notes: `[V3]` runs on the *written* VTU, where provenance is gone - a leaking face is just three
//   node ids. Here the same defects are recomputed while `parent_of` and the escalation list are
//   still in hand, which is the difference between "136 boundary leaks" and "136 boundary leaks, all
//   on cells escalated for reason R".
fn diagnose_conformity(
    mesh: &CutMesh,
    first_cut: u32,
    first_steiner: u32,
    face_centroids: &BTreeSet<u32>,
) {
    let escalation_of: BTreeMap<u32, Escalation> = mesh.escalated.iter().copied().collect();
    let mut owners: BTreeMap<[u32; 3], SmallVec<[usize; 2]>> = BTreeMap::new();
    for (index, tet) in mesh.tets.iter().enumerate() {
        for face in TET_FACES {
            let mut key = [tet[face[0]], tet[face[1]], tet[face[2]]];
            key.sort_unstable();
            owners.entry(key).or_default().push(index);
        }
    }
    let tagged: BTreeSet<[u32; 3]> = mesh
        .interfaces
        .iter()
        .map(|face| {
            let mut key = face.nodes;
            key.sort_unstable();
            key
        })
        .collect();
    let class = |node: u32| -> &'static str {
        if node < first_cut {
            "parent"
        } else if node < first_steiner {
            "cut"
        } else if face_centroids.contains(&node) {
            "face-centroid"
        } else {
            "cell-centroid"
        }
    };
    // The outer hull is a legitimate single-sided boundary; only faces off it leak.
    let mut lo = mesh.nodes[0];
    let mut hi = mesh.nodes[0];
    for point in &mesh.nodes {
        lo = Vec3::new(lo.x.min(point.x), lo.y.min(point.y), lo.z.min(point.z));
        hi = Vec3::new(hi.x.max(point.x), hi.y.max(point.y), hi.z.max(point.z));
    }
    let diagonal = hi.sub(lo);
    let tol = 1.0e-9 * (diagonal.dot(diagonal)).sqrt();
    let on_hull = |face: &[u32; 3]| -> bool {
        let p: [Vec3; 3] = [
            mesh.nodes[face[0] as usize],
            mesh.nodes[face[1] as usize],
            mesh.nodes[face[2] as usize],
        ];
        for axis in 0..3 {
            let value = |v: Vec3| match axis {
                0 => v.x,
                1 => v.y,
                _ => v.z,
            };
            let (min, max) = (value(lo), value(hi));
            if p.iter().all(|v| (value(*v) - min).abs() <= tol)
                || p.iter().all(|v| (value(*v) - max).abs() <= tol)
            {
                return true;
            }
        }
        false
    };
    let mut leaks_by_reason: BTreeMap<String, usize> = BTreeMap::new();
    let mut leak_node_classes: BTreeMap<String, usize> = BTreeMap::new();
    let mut edge_use: BTreeMap<[u32; 2], usize> = BTreeMap::new();
    for (face, tets) in &owners {
        for slot in 0..3 {
            let mut edge = [face[slot], face[(slot + 1) % 3]];
            edge.sort_unstable();
            *edge_use.entry(edge).or_insert(0) += tets.len();
        }
        if tets.len() != 1 || tagged.contains(face) || on_hull(face) {
            continue;
        }
        let parent = mesh.parent_of[tets[0]];
        let reason = match escalation_of.get(&parent) {
            Some(reason) => format!("{reason:?}"),
            None => "not-escalated".to_string(),
        };
        *leaks_by_reason.entry(reason).or_insert(0) += 1;
        let mut classes: SmallVec<[&str; 3]> = face.iter().map(|node| class(*node)).collect();
        classes.sort_unstable();
        *leak_node_classes
            .entry(classes.join("+"))
            .or_insert(0) += 1;
    }
    // Leaks come in mismatched pairs on one parent face, so printing a few together
    // with both parents' reasons says what disagreed rather than how much.
    let mut examples = 0usize;
    for (face_a, tets_a) in &owners {
        if examples >= 4 {
            break;
        }
        if tets_a.len() != 1 || tagged.contains(face_a) || on_hull(face_a) {
            continue;
        }
        for (face_b, tets_b) in &owners {
            if face_b <= face_a || tets_b.len() != 1 || tagged.contains(face_b) || on_hull(face_b) {
                continue;
            }
            let shared = face_a.iter().filter(|n| face_b.contains(n)).count();
            if shared < 2 {
                continue;
            }
            let parent_a = mesh.parent_of[tets_a[0]];
            let parent_b = mesh.parent_of[tets_b[0]];
            if parent_a == parent_b {
                continue;
            }
            println!(
                "[CUT-DIAG] mismatch: cell {parent_a} ({:?}) made {:?} [{}] vs cell {parent_b} ({:?}) made {:?} [{}]",
                escalation_of.get(&parent_a),
                face_a,
                face_a.iter().map(|n| class(*n)).collect::<SmallVec<[&str; 3]>>().join(","),
                escalation_of.get(&parent_b),
                face_b,
                face_b.iter().map(|n| class(*n)).collect::<SmallVec<[&str; 3]>>().join(","),
            );
            examples += 1;
            break;
        }
    }
    println!("[CUT-DIAG] untagged single-sided faces by escalation reason: {leaks_by_reason:?}");
    println!("[CUT-DIAG] ... by node class: {leak_node_classes:?}");
}

// AI-FUNC-SUMMARY:
// Purpose: The centroid of a closed triangle soup - the apex a slab's fan cones to.
// Inputs: the slab's triangles and the node table.
// Returns: the mean of its distinct corner positions.
// Side effects: None.
// Notes: Unlike a *face* centroid this node is interior to one slab, so no neighbour ever sees it
//   and a coincident duplicate is impossible. Each slab gets its own, which is what keeps the three
//   pieces of a band cell separate rather than fanned together.
pub fn polygon_soup_centroid(triangles: &[[u32; 3]], nodes: &[Vec3]) -> Vec3 {
    let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for triangle in triangles {
        for node in triangle {
            seen.insert(*node);
        }
    }
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    for node in &seen {
        sum = sum.add(nodes[*node as usize]);
    }
    sum.scale(1.0 / seen.len().max(1) as f64)
}

// AI-FUNC-SUMMARY:
// Purpose: Orient one tet positively by swapping two nodes when `orient3d` says it is inverted.
// Returns: the tet, or None when it is exactly degenerate.
// Side effects: None.
pub fn orient_positively(tet: [u32; 4], nodes: &[Vec3]) -> Option<[u32; 4]> {
    let value = orient3d(
        nodes[tet[0] as usize],
        nodes[tet[1] as usize],
        nodes[tet[2] as usize],
        nodes[tet[3] as usize],
    );
    if value > 0.0 {
        Some(tet)
    } else if value < 0.0 {
        Some([tet[1], tet[0], tet[2], tet[3]])
    } else {
        None
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The guarded dry-run of `SPEC_meshgen_geometry.md` §6 - validate a cut before committing it.
// Inputs: the parent tet, the proposed children, the node table, and the tolerances.
// Returns: Ok((oriented children, worst dihedral, relative volume error)) or Err(the reason).
// Side effects: None.
// Notes: Three conditions, all from the frozen text: every child positively oriented, the children's
//   volumes summing to the parent's within 1 %, and - implicitly - no child degenerate. The volume
//   test is the one that catches a *wrong table row*, because a mis-assembled case still produces
//   positively oriented tets; it just does not fill the parent.
/// The children one cell's table row produces. Eight is the largest §6 row (case D).
pub type ChildTets = SmallVec<[[u32; 4]; 8]>;

pub fn guarded_dry_run(
    parent: [u32; 4],
    children: &[[u32; 4]],
    nodes: &[Vec3],
    options: &CutOptions,
) -> Result<(ChildTets, f64, f64), Escalation> {
    let volume_of = |tet: [u32; 4]| -> f64 {
        orient3d(
            nodes[tet[0] as usize],
            nodes[tet[1] as usize],
            nodes[tet[2] as usize],
            nodes[tet[3] as usize],
        ) / 6.0
    };
    let parent_volume = volume_of(parent);
    let mut oriented: ChildTets = SmallVec::new();
    let mut total = 0.0;
    let mut worst = f64::INFINITY;
    for child in children {
        let Some(fixed) = orient_positively(*child, nodes) else {
            return Err(Escalation::DryRun);
        };
        let volume = volume_of(fixed);
        if volume <= 0.0 {
            return Err(Escalation::DryRun);
        }
        total += volume;
        let quality = tet_quality([
            nodes[fixed[0] as usize],
            nodes[fixed[1] as usize],
            nodes[fixed[2] as usize],
            nodes[fixed[3] as usize],
        ]);
        worst = worst.min(quality.min_dihedral_deg);
        oriented.push(fixed);
    }
    if parent_volume <= 0.0 {
        return Err(Escalation::DryRun);
    }
    let error = (total - parent_volume).abs() / parent_volume;
    if error > options.volume_tolerance {
        return Err(Escalation::DryRun);
    }
    Ok((oriented, worst, error))
}

// ---------------------------------------------------------------------------
// The stage
// ---------------------------------------------------------------------------

/// What one cell produced, before the results are concatenated in cell order.
#[derive(Clone, Debug, Default)]
struct CellResult {
    tets: ChildTets,
    inside: SmallVec<[bool; 8]>,
    interface: SmallVec<[[u32; 3]; 2]>,
    component: i32,
    case: u8,
    /// A welded sheet cut: the geometry is split but the ownership is not, because
    /// both sides of a welded sheet are the same material (PLAN §10.13, C0).
    welded: bool,
    /// G7-2: the second component of a welded pair - a cell whose only crossings are
    /// collapsed edges, so one surface settles both solids. A child outside
    /// `component` is *inside* this one, there being no void left between them.
    welded_with: Option<i32>,
    escalation: Option<Escalation>,
    min_dihedral: f64,
    volume_error: f64,
}

// AI-FUNC-SUMMARY:
// Purpose: Run S8 - replace every cell one active patch crosses with pieces that stop at the patch.
// Inputs: the S5 lattice (connectivity), S7's snap (coordinates + crossings + on-cut set), S6's
//   classification (per-vertex ownership and the records to inherit), and the tolerances.
// Returns: CutMesh - nodes, tets, records, the tagged interface, and the escalation list.
// Side effects: None (warnings are collected into the result).
// Notes: **Cut nodes are assigned global ids before any cell is cut.** That is what makes the
//   per-cell work independent: two cells sharing an edge look up the same node id for the crossing
//   on it, so their pieces meet, and the cells can therefore be cut in parallel and concatenated in
//   cell order (PLAN §12.5). Nothing here communicates between cells - conformity comes from
//   Invariant C, not from negotiation.
//
//   A cell crossed by two or more components, or one whose configuration is not a legal §6 row, is
//   **not** cut: it is emitted whole and listed in `escalated` for the junction path (§7). Cutting
//   it sequentially would be order-dependent, which is the failure mode the junction path exists to
//   remove.
pub fn cut_lattice(
    lattice: &Lattice,
    snapped: &Snapped,
    classification: &Classification,
    classifier: &PointClassifier,
    components: &[ArrangeComponent],
    options: &CutOptions,
) -> CutMesh {
    // Sheets never own volume, so S6 gives them no ownership entry and the solid
    // path never sees them - but they are active patches, S7 recorded their
    // crossings, and a welded sheet has to become a union of element faces all the
    // same (PLAN §10.13). They are cut here by the same §6 table, with the sides
    // taken from the cut itself rather than from an inside/outside test.
    let sheets: Vec<i32> = components
        .iter()
        .filter(|component| component.kind == 1)
        .map(|component| component.x)
        .collect();
    let mut all_components: Vec<i32> = classification.solid_components.clone();
    all_components.extend(sheets.iter().copied());
    all_components.sort_unstable();
    all_components.dedup();
    let quantum = options.eps.max(f64::MIN_POSITIVE);
    let mut nodes = snapped.nodes.clone();

    // --- global cut-node ids, one per (edge, component) ---
    //
    // G7-2's production rim collapse happens **here**, at the cut node, and not later
    // on the elements. The first implementation welded the two walls after the cells
    // were meshed and it cost 27,336 hanging nodes: by then the two walls had already
    // triangulated their shared faces as a doubly-cut pair, and fusing their nodes
    // afterwards left every neighbouring cell that had split the same face only once
    // disagreeing about it. Collapsing the *node* instead means no cell ever sees the
    // gap: an edge both walls cross carries a single crossing, every face over it takes
    // an ordinary §5.2 row, and conformity is the single-patch argument again rather
    // than something to re-establish. §8.2's `k = 3` row - "the cell *is* the sheet
    // triangle; its faces come from the sheet cut" - is exactly this, read forwards.
    let mut first_crossing: BTreeMap<([u32; 2], i32), &EdgeCrossing> = BTreeMap::new();
    let mut second_crossing: BTreeMap<([u32; 2], i32), &EdgeCrossing> = BTreeMap::new();
    let mut edge_components: BTreeMap<[u32; 2], SmallVec<[i32; 2]>> = BTreeMap::new();
    for crossing in &snapped.crossings {
        let key = (crossing.edge, crossing.component);
        // Invariant K2, applied where the crossing list is turned into cut nodes. An edge
        // with an endpoint already **on** this component's patch has its intersection
        // represented by that node; a second cut node on the same edge is the coincident
        // pair K2 forbids ("a cut node is never emitted coincident with a parent node").
        // Left in, it makes the edge carry a crossing while S6 correctly reports both
        // endpoints on one side, and the cell escalates to the conforming fan - which is
        // every remaining S6/S7 disagreement now that the GWN gate has removed the other
        // direction (5,178 of 5,178 on the strut lattice).
        if snapped
            .on_cut
            .binary_search(&(crossing.edge[0], crossing.component))
            .is_ok()
            || snapped
                .on_cut
                .binary_search(&(crossing.edge[1], crossing.component))
                .is_ok()
        {
            continue;
        }
        if first_crossing.contains_key(&key) {
            // Invariant K1: this component crosses the edge more than once. The first
            // crossing keeps the id and the §6 table cannot express the rest, so both
            // incident cells escalate - but the *second* crossing is kept too, because
            // it is a real point of the surface and the escalation path can use it. An
            // edge piercing a thin plate crosses it entering and leaving; dropping the
            // second crossing left the plate's far wall with no node on that edge at
            // all, so no cut could separate the gap and every such cell was coned to a
            // centroid, which is the "band declared, no band elements" behaviour. Only
            // the second is kept: a third crossing is a feature below one element that
            // refinement, not cutting, has to answer.
            second_crossing.entry(key).or_insert(crossing);
            continue;
        }
        first_crossing.insert(key, crossing);
        let list = edge_components.entry(crossing.edge).or_default();
        if !list.contains(&crossing.component) {
            list.push(crossing.component);
        }
    }
    // Ids for the second crossings, after every first crossing has one so the table
    // path's node numbering is untouched.
    let mut cut_index: BTreeMap<([u32; 2], i32), u32> = BTreeMap::new();
    let mut collapsed_edges: BTreeSet<[u32; 2]> = BTreeSet::new();
    if let Some(thin) = options.thin.as_ref().filter(|thin| thin.collapse_sheets) {
        for (edge, components) in &edge_components {
            if components.len() != 2 {
                continue;
            }
            let (Some(left), Some(right)) = (
                first_crossing.get(&(*edge, components[0])),
                first_crossing.get(&(*edge, components[1])),
            ) else {
                continue;
            };
            // The decision is a pure function of the two crossings - same thin region,
            // that region converted to `Sheet`, separation within `t_sheet` - so no
            // cell votes and no two cells can disagree. That is Invariant B1.
            let (Some(region), Some(other)) = (thin.region_of(left.face), thin.region_of(right.face))
            else {
                continue;
            };
            if region != other || thin.regime_of(region) != ThinRegime::Sheet {
                continue;
            }
            let delta = right.point.sub(left.point);
            if delta.dot(delta).sqrt() > thin.t_sheet {
                continue;
            }
            let rim = left.point.add(right.point).scale(0.5);
            let id = nodes.len() as u32;
            nodes.push(rim);
            cut_index.insert((*edge, components[0]), id);
            cut_index.insert((*edge, components[1]), id);
            collapsed_edges.insert(*edge);
        }
    }
    // Two solids in exact face contact are crossed by one edge at **one** point. S7 now
    // reports that crossing once per component (`gather_geometry`), so without this the
    // two would take two node ids at the same coordinate - a duplicate node, and a crack
    // between the bodies rather than a shared boundary. Sharing the id instead makes the
    // per-edge maps identical, which is exactly what `face_is_single_patch` reads to say
    // "one surface reached twice", so the face takes an ordinary §5.2 row and the contact
    // plane becomes a face of the mesh. Distinct from G7-2's collapse above: that fuses
    // the two *walls of a gap* and declares a rim, this records a contact that was always
    // one surface.
    let mut contact_edges: BTreeSet<[u32; 2]> = BTreeSet::new();
    for (edge, components) in &edge_components {
        if components.len() < 2 || collapsed_edges.contains(edge) {
            continue;
        }
        let mut ordered = components.clone();
        ordered.sort_unstable();
        let Some(first) = first_crossing.get(&(*edge, ordered[0])) else {
            continue;
        };
        let coincident = ordered[1..].iter().all(|component| {
            first_crossing
                .get(&(*edge, *component))
                .is_some_and(|other| {
                    let delta = other.point.sub(first.point);
                    delta.dot(delta) <= quantum * quantum
                })
        });
        if !coincident {
            continue;
        }
        let id = nodes.len() as u32;
        nodes.push(first.point);
        for component in &ordered {
            cut_index.insert((*edge, *component), id);
        }
        contact_edges.insert(*edge);
    }
    for (key, crossing) in &first_crossing {
        if cut_index.contains_key(key) {
            continue;
        }
        cut_index.insert(*key, nodes.len() as u32);
        nodes.push(crossing.point);
    }
    // The second crossing of a K1 edge, in its own index. Kept out of `cut_index` on
    // purpose: everything reading that map is either the §6 table, which has no row for
    // two cuts on one edge, or a conformity test that would change meaning if an edge
    // suddenly answered twice. Only the escalation path consults this one, and only
    // cells that already escalated can reach it.
    let mut second_index: BTreeMap<([u32; 2], i32), u32> = BTreeMap::new();
    // Invariant K1's second node, and what lets a cell straddling a plate be cut on
    // **both** walls: on the sheet fixture junction cuts go 66 -> 145 cells and 132 -> 372
    // material pieces, on the band fixture 1,089 -> 2,706 cells. It shipped gated for one
    // pass because it cost conformity, and the two defects behind that are now fixed and
    // worth recording, because each was found only after a wrong guess:
    //   - An edge carrying two cut nodes contributes three collinear points to the face
    //     walk, and a chord lying *along* a walk edge fans to zero-area triangles shared by
    //     every cell around the edge (408 multi-shared faces). `crossed_face` refuses such
    //     a chord. Testing the area for exact zero does not find them - the three points are
    //     only numerically collinear - and *dropping* the slivers is worse than making them,
    //     because their edges then match nothing (leaks 852 -> 1,488).
    //   - A split piece that is closed can still fail to *fan*: a boundary triangle
    //     coplanar with the piece's centroid gives a flat tet, which was silently dropped
    //     and punched a hole through it. 588 dropped pieces, 588 interface cracks, one for
    //     one. `fan_is_sound` declines the split instead.
    // Two hypotheses tested and refuted along the way: the cells need no new escalation rule
    // (all 288 already escalate on `multi_crossing`), and it is not the four-position chord
    // pairing (disabling that leaves every count bit-identical).
    // `RUSTMSPT_NO_K1_SECOND_CUT=1` turns it back off as a bisection handle.
    let second_cuts = std::env::var_os("RUSTMSPT_NO_K1_SECOND_CUT").is_none();
    for (key, crossing) in &second_crossing {
        if !second_cuts {
            continue;
        }
        // A collapsed or contact edge already answers with one shared node by design;
        // a second node there would re-open the gap the collapse just welded shut.
        if collapsed_edges.contains(&key.0) || contact_edges.contains(&key.0) {
            continue;
        }
        let Some(first) = cut_index.get(key).copied() else {
            continue;
        };
        let delta = crossing.point.sub(nodes[first as usize]);
        if delta.dot(delta) <= quantum * quantum {
            continue;
        }
        second_index.insert(*key, nodes.len() as u32);
        nodes.push(crossing.point);
    }
    // Distinct cut nodes, which is no longer `cut_index.len()`: a collapsed edge has
    // two entries pointing at one node.
    let nodes_pushed = nodes.len() - snapped.nodes.len();
    // Both kinds of edge carry one node for two components, so both make a cell's two
    // components "one surface reached twice" for §6's purposes. They are kept apart
    // everywhere else: only a *collapsed* edge is a sheet with a rim to declare.
    let welded_edges: BTreeSet<[u32; 2]> = collapsed_edges
        .iter()
        .chain(contact_edges.iter())
        .copied()
        .collect();
    let rim_nodes: BTreeSet<u32> = collapsed_edges
        .iter()
        .filter_map(|edge| cut_index.get(&(*edge, edge_components[edge][0])).copied())
        .collect();
    // Where the collapsed sheet *ends*. A collapsed region is bounded by the locus
    // where the gap reopens past `t_sheet`, and that locus is a rim in §2.3's sense
    // (`CurveKind = 1`): the sheet legitimately stops there and continues as band or
    // volumetric elements. `[V8]` requires a sheet's boundary edges to be declared
    // curves precisely so that the boundary it *cannot* explain - a pinhole, a hole in
    // the middle of a sheet - still fails.
    //
    // So the rim is derived from the collapse **decision**, not from the faces that
    // came out of it. A cell that has at least one collapsed edge and at least one
    // crossed edge that did not collapse is straddling the boundary of the region; the
    // rim nodes it carries are on the rim. Reading the emitted faces instead would
    // declare every boundary edge of the sheet a rim, pinholes included, and turn
    // `[V8]` into the third check in this file that passes by measuring nothing.
    let mut rim_boundary_nodes: BTreeSet<u32> = BTreeSet::new();
    if !collapsed_edges.is_empty() {
        for tet in &lattice.tets {
            let mut collapsed_here: SmallVec<[u32; 6]> = SmallVec::new();
            let mut open_crossing = false;
            for a in 0..4 {
                for b in (a + 1)..4 {
                    let (x, y) = (tet[a], tet[b]);
                    let edge = if x <= y { [x, y] } else { [y, x] };
                    if collapsed_edges.contains(&edge) {
                        if let Some(list) = edge_components.get(&edge) {
                            if let Some(node) = cut_index.get(&(edge, list[0])) {
                                collapsed_here.push(*node);
                            }
                        }
                    } else if all_components
                        .iter()
                        .any(|component| cut_index.contains_key(&(edge, *component)))
                    {
                        open_crossing = true;
                    }
                }
            }
            if open_crossing {
                rim_boundary_nodes.extend(collapsed_here);
            }
        }
    }
    // Which component each cut node belongs to - S8b needs it to tag the walls of a
    // band cell's gap slab, which are cut surfaces like any other and must appear in
    // the interface index or `[V7]`'s one-layer check sees band elements floating
    // free of any wall.
    let component_of_cut: BTreeMap<u32, i32> = cut_index
        .iter()
        .map(|((_, component), node)| (*node, *component))
        .collect();
    // G7-2: and which S3 thin region, through the arranged face the crossing was
    // found on. This is the only link between the two stages, and it is one hop:
    // cut node -> `EdgeCrossing::face` -> `ThinContext::region_of`. The first
    // crossing on an (edge, component) is the one that keeps the node id above, so
    // it is the one that attributes it here too.
    let mut region_of_cut: BTreeMap<u32, i32> = BTreeMap::new();
    if let Some(thin) = options.thin.as_ref() {
        for crossing in &snapped.crossings {
            let Some(node) = cut_index.get(&(crossing.edge, crossing.component)) else {
                continue;
            };
            let region = thin.region_of(crossing.face).map(|id| id as i32).unwrap_or(-1);
            region_of_cut.entry(*node).or_insert(region);
        }
    }
    let mut multi_crossing: BTreeMap<([u32; 2], i32), usize> = BTreeMap::new();
    for crossing in &snapped.crossings {
        *multi_crossing
            .entry((crossing.edge, crossing.component))
            .or_insert(0) += 1;
    }
    multi_crossing.retain(|_, count| *count > 1);

    let mut on_cut: BTreeMap<(u32, i32), ()> = BTreeMap::new();
    for (node, component) in &snapped.on_cut {
        on_cut.insert((*node, *component), ());
    }

    // The key table S8 orders nodes by is built on a **finer grid than the weld
    // grid**, and that is a correctness requirement rather than a refinement.
    // `SPEC_meshgen_geometry.md` §1.2 asks two things of keys: that they depend only
    // on position, and that distinct nodes have distinct ones - the second is what
    // makes "smallest node key" a total order, and every §4 conformity argument rests
    // on it. Lattice nodes satisfy it by construction, but S7's crossings are
    // *constructed* points and two of them can land closer together than the weld
    // quantum. Two such nodes tie, the tie is resolved by whichever the quad happens
    // to list first, the two cells sharing that quad list it in opposite orders, and
    // they split it on opposite diagonals: 60 single-sided faces on the reference
    // dataset, from one collision class. Welding the pair instead is worse - it also
    // merges crossings on *different* edges and collapses the cells between them.
    // A finer grid keeps the order a pure function of position, keeps lattice keys
    // exact, and makes the collision vanish.
    let order_quantum = quantum * KEY_ORDER_REFINEMENT;
    let keys: Vec<NodeKey> = nodes
        .par_iter()
        .map(|point| node_key(*point, order_quantum))
        .collect();

    // --- G6-0: the point where a locked curve pierces a lattice face ---
    let curve_pierce = curve_pierce_points(lattice, &nodes, &keys, &options.curve_segments);
    let curve_nodes = nodes_on_curve(&nodes, &options.curve_segments, options.eps);
    let slots = classification.solid_components.len();
    let inside_of = |node: u32, slot: usize| -> bool {
        classification.vertex_inside[node as usize * slots.max(1) + slot]
    };
    // An interior sample, for the one configuration the §6 table cannot decide from its
    // vertices: every vertex *on* the patch. `None` where the component is not a classified
    // solid or the predicate is not sure, which leaves the old answer standing.
    let sample_inside = |point: Vec3, component: i32| -> Option<bool> {
        let slot = classifier.slot_of(component)?;
        let mut uncertain = 0usize;
        let inside = classifier.inside(point, slot, &mut uncertain);
        (uncertain == 0).then_some(inside)
    };

    // --- per cell, in parallel ---
    let per_cell: Vec<CellResult> = lattice
        .tets
        .par_iter()
        .enumerate()
        .map(|(index, tet)| {
            cut_one_cell(
                index,
                *tet,
                classification,
                &sheets,
                &all_components,
                &cut_index,
                &second_index,
                &multi_crossing,
                &on_cut,
                &keys,
                &nodes,
                &inside_of,
                &sample_inside,
                &welded_edges,
                options,
            )
        })
        .collect();

    // P-3.3: for each face a locked curve pierces, how many of its (at most two) owning
    // cells escalated. Only a face escalated on *both* sides can have its triangulation
    // changed by the junction path alone - the other side would keep §5.2's straight chord
    // and the two would stop matching, which is how the two earlier crease attempts cracked
    // the mesh. Scoped to `curve_pierce`'s keys, so this is a handful of lookups per cell and
    // not a map over every face in the lattice.
    // **Which curve pierced the face, and whose it is.** A crease is a sharp edge of *one*
    // surface, and that is the whole population P-3.3 targets. An intersection curve looks
    // identical to `curve_pierce` - both are locked polylines - but putting a node on one
    // where only a single component cuts the face declares a junction the mesh has no material
    // for: `[V9]`'s `curve_node_id` then reads "node 20347 lies on curve 13, where components
    // [1, 2] meet, but its N_ID is [0, 2]", six of them on A-3, and it is right to. The node
    // really is on the 1-2 curve and really has no component-1 element touching it. Restricting
    // the fan to curves whose component set the face's own cut can carry removes the claim
    // rather than papering over it in the label.
    let mut pierce_components: BTreeMap<[u32; 3], SmallVec<[i32; 2]>> = BTreeMap::new();
    for curve in &options.locked_curves {
        if curve.segments.is_empty() {
            continue;
        }
        for (face, _) in curve_pierce_points(lattice, &nodes, &keys, &curve.segments) {
            let entry = pierce_components.entry(face).or_default();
            for component in &curve.components {
                if !entry.contains(component) {
                    entry.push(*component);
                }
            }
        }
    }
    for members in pierce_components.values_mut() {
        members.sort_unstable();
    }

    // **§7.1 as amended (rev 1.5): every cell the surface's trace crosses is offered §7.2 first.**
    // Two passes, and the second is the whole reason there are two. Pass A asks each cell, on its
    // own, whether §7.4's constrained tetrahedralisation exists for it. Pass B is a fixed point over
    // the lattice: a cell may only take the path if every cell sharing a traced face with it also
    // does, because an augmented face triangulation must be used by both owners or neither. Dropping
    // one cell can therefore drop its neighbour, which is why this iterates rather than filters once.
    // --- concatenate in cell order ---
    // Serial, in ascending cell order, so the §7.5 samples are taken in a fixed
    // sequence and R-P2 holds whatever the thread count is.
    let mut seed_uncertain = 0usize;
    let mut seeded_pieces = 0usize;
    let mut hidden_recovered = 0usize;
    // Off by default: it costs a classifier query per node of every fanned piece, and it
    // exists to answer one question once (see `SpokeProbe`), not to run in production.
    let probing_spokes = std::env::var_os("RUSTMSPT_SPOKE_PROBE").is_some();
    let mut spoke_probe = SpokeProbe::default();
    let mut mesh = CutMesh {
        n_lattice_nodes: nodes.len() as u32,
        nodes,
        tets: Vec::new(),
        records: Vec::new(),
        parent_of: Vec::new(),
        regime: Vec::new(),
        band_region: Vec::new(),
        thin_regime: options
            .thin
            .as_ref()
            .map(|thin| thin.regime.iter().map(|r| *r as u8).collect())
            .unwrap_or_default(),
        thin_pair_class: options
            .thin
            .as_ref()
            .map(|thin| thin.pair_class.clone())
            .unwrap_or_default(),
        rim_curve: Vec::new(),
        constraint_kind: Vec::new(),
        constraint_ref: Vec::new(),
        curve_edges: Vec::new(),
        curves: Vec::new(),
        interfaces: Vec::new(),
        escalated: Vec::new(),
        warnings: Vec::new(),
        stats: CutStats {
            n_parents: lattice.tets.len(),
            n_cut_nodes: nodes_pushed,
            n_collapsed_pairs: collapsed_edges.len(),
            band_min_dihedral_deg: f64::INFINITY,
            min_dihedral_deg: f64::INFINITY,
            ..Default::default()
        },
    };
    let n_parent_nodes = snapped.nodes.len() as u32;
    let n_cut_and_parent_nodes = mesh.nodes.len() as u32;
    let mut pending_interfaces: Vec<(usize, [u32; 3], i32)> = Vec::new();
    let mut keys = keys;
    let mut face_steiner: BTreeMap<[u32; 3], u32> = BTreeMap::new();
    // What §7.4 decided for each cell, consumed by the assembly loop below.
    let mut plc: Vec<Option<PlcPlan>> = (0..lattice.tets.len()).map(|_| None).collect();
    // Kept past the gate so the emitted mesh can be checked against what the faces promised.
    let mut plc_face_tris: BTreeMap<[u32; 3], Vec<[u32; 3]>> = BTreeMap::new();
    let mut plc_face_owners_n: BTreeMap<[u32; 3], usize> = BTreeMap::new();
    let mut plc_boundary: Vec<Option<Vec<[u32; 3]>>> =
        (0..lattice.tets.len()).map(|_| None).collect();
    let mut plc_edge_points: BTreeMap<[u32; 2], Vec<u32>> = BTreeMap::new();
    let plc_pass = std::env::var_os("RUSTMSPT_PLC_PASS").is_some();
    if plc_pass {
        // The arena quantum has to be the one the node keys were built with, or a point that
        // coincides with an existing node interns as a second node at the same place.
        let tol = order_quantum;
        // Every lattice face the surface crosses, with its trace, and every crossing node already
        // on it. Both are functions of the face alone - which is what lets the two cells sharing it
        // derive the same triangulation without communicating (invariant J1).
        let mut chords_of: BTreeMap<[u32; 3], SmallVec<[[Vec3; 2]; 4]>> = BTreeMap::new();
        let mut face_nodes: BTreeMap<[u32; 3], SmallVec<[u32; 4]>> = BTreeMap::new();
        let mut owners: BTreeMap<[u32; 3], SmallVec<[u32; 2]>> = BTreeMap::new();
        let mut edge_nodes: BTreeMap<[u32; 2], SmallVec<[u32; 6]>> = BTreeMap::new();
        let mut seen_faces: BTreeSet<[u32; 3]> = BTreeSet::new();
        for (index, tet) in lattice.tets.iter().enumerate() {
            for slots in TET_FACES {
                let raw = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                let mut face = raw;
                face.sort_by_key(|node| keys[*node as usize]);
                let entry = owners.entry(face).or_default();
                if !entry.contains(&(index as u32)) {
                    entry.push(index as u32);
                }
                if !seen_faces.insert(face) {
                    continue;
                }
                let geometry = [
                    mesh.nodes[raw[0] as usize],
                    mesh.nodes[raw[1] as usize],
                    mesh.nodes[raw[2] as usize],
                ];
                // **Relative to the face, not absolute.** `trace_on_face` clips the chord to the
                // face within its tolerance, so an absolute one lets an endpoint sit outside a
                // small face by whatever fraction of it that tolerance happens to be: measured at
                // 2.9e-2 of the face - three percent - with `options.eps` on a6a, which is not a
                // rounding artefact and no threshold downstream should be widened to swallow it.
                let shortest = (0..3)
                    .map(|slot| {
                        let d = geometry[(slot + 1) % 3].sub(geometry[slot]);
                        d.dot(d).sqrt()
                    })
                    .fold(f64::INFINITY, f64::min);
                let mut chords: SmallVec<[[Vec3; 2]; 4]> = SmallVec::new();
                for component in &all_components {
                    let Some(slot) = classifier.slot_of(*component) else { continue };
                    for chord in crate::meshgen::cdt::trace_on_face(
                        geometry,
                        classifier.triangles_of(slot),
                        shortest * 1.0e-9,
                    ) {
                        chords.push(chord);
                    }
                }
                let mut on_face: SmallVec<[u32; 4]> = SmallVec::new();
                for slot in 0..3 {
                    let (x, y) = (raw[slot], raw[(slot + 1) % 3]);
                    let key = if x <= y { [x, y] } else { [y, x] };
                    // The same crossings kept per EDGE as well. A trace endpoint that coincides
                    // with one is that node, and the decision has to be a function of the edge
                    // rather than of the face, or the two faces sharing the edge make it
                    // differently.
                    let entry = edge_nodes.entry(key).or_default();
                    if !entry.contains(&key[0]) {
                        entry.push(key[0]);
                    }
                    if !entry.contains(&key[1]) {
                        entry.push(key[1]);
                    }
                    for component in &all_components {
                        for map in [&cut_index, &second_index] {
                            if let Some(node) = map.get(&(key, *component)) {
                                if !on_face.contains(node) {
                                    on_face.push(*node);
                                }
                                if !entry.contains(node) {
                                    entry.push(*node);
                                }
                            }
                        }
                    }
                }
                if !chords.is_empty() {
                    chords_of.insert(face, chords);
                }
                if !on_face.is_empty() {
                    face_nodes.insert(face, on_face);
                }
            }
        }

        let mut global: BTreeMap<NodeKey, u32> = BTreeMap::new();
        for (id, key) in keys.iter().enumerate() {
            global.entry(*key).or_insert(id as u32);
        }

        // **A trace point on a lattice EDGE belongs to six cells, not two.** Delivering a face's
        // trace to that face's two owners is right for a point strictly inside it and wrong for one
        // on its boundary: the edge is shared by every cell around it, and the four that never see
        // this face keep the edge whole, leaving the node sitting on their faces with nothing
        // referencing it. That is 1,437 boundary leaks and 7,025 hanging nodes on a6a, and the
        // count was invariant under every other bisection precisely because it depends on the
        // augmented FACE SET rather than on how any cell is meshed (PLAN §6.32).
        //
        // So an edge point is interned per EDGE, exactly as `cut_index` interns a crossing, and
        // every face carrying that edge takes it - including faces the surface never touches.
        let mut chord_id: BTreeMap<NodeKey, u32> = BTreeMap::new();
        let mut edge_points: BTreeMap<[u32; 2], SmallVec<[u32; 4]>> = BTreeMap::new();
        let mut face_interior: BTreeMap<[u32; 3], SmallVec<[u32; 4]>> = BTreeMap::new();
        for (face, chords) in &chords_of {
            // Copied out before the loop: interning below takes `mesh.nodes` mutably.
            let corners = [
                mesh.nodes[face[0] as usize],
                mesh.nodes[face[1] as usize],
                mesh.nodes[face[2] as usize],
            ];
            let corner = |slot: usize| corners[slot];
            for chord in chords {
                for point in chord {
                    // **Which of the face's three edges it lies on, and if it does, it is put
                    // exactly there.** A trace endpoint is a clipped chord and lands within
                    // rounding of the edge it ends on; leaving it a few ULP off means the face on
                    // the other side of that edge reads it as lying OUTSIDE itself and refuses to
                    // triangulate - 38 of the 51 faces that failed, each one leaving its cell's
                    // boundary open. Projecting onto the edge is a function of the point and the
                    // edge, so both faces get the same point and neither can disagree.
                    let mut on_edge = None;
                    let mut placed = *point;
                    for slot in 0..3 {
                        let (a, b) = (corner(slot), corner((slot + 1) % 3));
                        let along = b.sub(a);
                        let len2 = along.dot(along);
                        if len2 <= 0.0 {
                            continue;
                        }
                        let rel = point.sub(a);
                        let t = rel.dot(along) / len2;
                        let off = rel.sub(along.scale(t));
                        // Relative to the edge's own length. `tol` here is the arena quantum,
                        // which is 1e-6 of eps and so tight that a point produced by clipping a
                        // chord never reads as being on the edge it plainly lies on.
                        if off.dot(off) <= 1.0e-18 * len2 && (-1.0e-9..=1.0 + 1.0e-9).contains(&t)
                        {
                            let (x, y) = (face[slot], face[(slot + 1) % 3]);
                            on_edge = Some(if x <= y { [x, y] } else { [y, x] });
                            placed = a.add(along.scale(t.clamp(0.0, 1.0)));
                            break;
                        }
                    }
                    // **A trace point that coincides with a node already there IS that node.**
                    // The pass interns on `order_quantum`, which is 1e-6 of the coincidence
                    // tolerance the rest of the pipeline merges nodes with; a trace endpoint and
                    // the §5.2 crossing at the same place are two computations of one intersection
                    // and land a few times 1e-8 apart, so at that quantum they become two nodes at
                    // one point. Four of them on a6a's edge (10788, 10795), and the face
                    // triangulation then had three collinear points to triangulate: it emitted a
                    // zero-area sliver, both faces sharing the edge emitted the SAME sliver, and
                    // the cell's boundary listed one triangle twice. That is the whole of a6a's
                    // residual - 18 faces carried by four tets, and the holes beside them.
                    //
                    // The candidates are the nodes on the point's own EDGE, not on its face:
                    // restricted that way the decision is a function of the edge, so the two faces
                    // sharing it cannot decide differently (invariant J1). `quantum` is
                    // `options.eps`, the tolerance `coincidence: merge` already uses - the rule is
                    // the pipeline's own, applied where the pass had been ignoring it.
                    let mut snapped = None::<u32>;
                    {
                        let candidates: &[u32] = match &on_edge {
                            Some(edge) => edge_nodes.get(edge).map(|v| &v[..]).unwrap_or(&[]),
                            None => &[],
                        };
                        let mut best = quantum;
                        for other in candidates
                            .iter()
                            .chain(match &on_edge {
                                Some(edge) => {
                                    edge_points.get(edge).map(|v| &v[..]).unwrap_or(&[])
                                }
                                None => &[],
                            })
                            .chain(match on_edge {
                                Some(_) => &[][..],
                                None => face_nodes.get(face).map(|v| &v[..]).unwrap_or(&[]),
                            })
                            .chain(match on_edge {
                                Some(_) => &[][..],
                                None => &face[..],
                            })
                            .chain(match on_edge {
                                Some(_) => &[][..],
                                None => face_interior.get(face).map(|v| &v[..]).unwrap_or(&[]),
                            })
                        {
                            let d = mesh.nodes[*other as usize].sub(placed);
                            let d = d.dot(d).sqrt();
                            // `<` and not `<=`, plus the id tie-break, so the nearest node wins and
                            // an exact tie resolves the same way on every run.
                            if d < best || (d == best && snapped.is_some_and(|s| *other < s)) {
                                best = d;
                                snapped = Some(*other);
                            }
                        }
                    }
                    let key = node_key(placed, order_quantum);
                    let id = match snapped {
                        Some(id) => id,
                        None => *global.entry(key).or_insert_with(|| {
                            let id = mesh.nodes.len() as u32;
                            mesh.nodes.push(placed);
                            keys.push(key);
                            id
                        }),
                    };
                    chord_id.insert(node_key(*point, order_quantum), id);
                    match on_edge {
                        Some(edge) => {
                            let entry = edge_points.entry(edge).or_default();
                            if !entry.contains(&id) {
                                entry.push(id);
                            }
                        }
                        None => {
                            let entry = face_interior.entry(*face).or_default();
                            if !entry.contains(&id) {
                                entry.push(id);
                            }
                        }
                    }
                }
            }
        }

        // **How many of the interned points are, by the pipeline's own coincidence tolerance,
        // an existing node?** The pass interns on `order_quantum`, which is 1e-6 of `eps` - fine
        // enough to preserve ordering and far too fine to decide identity. A trace endpoint and
        // the §5.2 crossing at the same place are computed by different routes and land microns
        // apart in the last digits; at this quantum they become two nodes.
        {
            let coarse = quantum.max(f64::MIN_POSITIVE);
            let cell = |p: Vec3| {
                (
                    (p.x / coarse).floor() as i64,
                    (p.y / coarse).floor() as i64,
                    (p.z / coarse).floor() as i64,
                )
            };
            let created: Vec<u32> = edge_points
                .values()
                .flatten()
                .chain(face_interior.values().flatten())
                .copied()
                .collect();
            let mut grid: BTreeMap<(i64, i64, i64), Vec<u32>> = BTreeMap::new();
            for (id, point) in mesh.nodes.iter().enumerate() {
                grid.entry(cell(*point)).or_default().push(id as u32);
            }
            let mut collides = 0usize;
            let mut worst = f64::INFINITY;
            let mut decades = [0usize; 12];
            for id in &created {
                let point = mesh.nodes[*id as usize];
                let home = cell(point);
                let mut near = None::<f64>;
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            let key = (home.0 + dx, home.1 + dy, home.2 + dz);
                            for other in grid.get(&key).into_iter().flatten() {
                                if other == id {
                                    continue;
                                }
                                let d = mesh.nodes[*other as usize].sub(point);
                                let d = d.dot(d).sqrt();
                                if d <= coarse {
                                    near = Some(near.map_or(d, |n: f64| n.min(d)));
                                }
                            }
                        }
                    }
                }
                if let Some(d) = near {
                    collides += 1;
                    worst = worst.min(d);
                    let decade = if d <= 0.0 {
                        0
                    } else {
                        ((d.log10().floor() as i64) + 14).clamp(0, 11) as usize
                    };
                    decades[decade] += 1;
                }
            }
            println!(
                "[PLC-PASS] {collides} of {} interned point(s) sit within eps of another node - \
                 the same point by the pipeline's own coincidence rule, two nodes by the pass's \
                 ordering quantum; closest pair {:.3e} against eps {:.3e}",
                created.len(),
                worst,
                coarse
            );
            println!(
                "[PLC-PASS]   by separation, 1e-14 up to eps: {decades:?} - a gap in this row is \
                 an identity scale the geometry itself names, a smear is a threshold being tuned"
            );
        }

        // Every face's full point set: its corners, the crossings already on it, the points its own
        // trace put inside it, and every edge point on any of its three edges - whoever put them
        // there.
        //
        // A note on what makes a face's point set hard: points that are distinct but nearly
        // collinear triangulate into slivers, and a sliver on the boundary forces a flat TET on
        // whichever cell owns it. The census after `face_tris` measures exactly that.
        let mut face_ids: BTreeMap<[u32; 3], Vec<u32>> = BTreeMap::new();
        for face in owners.keys() {
            let mut ids: Vec<u32> = face.to_vec();
            for id in face_nodes.get(face).into_iter().flatten() {
                if !ids.contains(id) {
                    ids.push(*id);
                }
            }
            for id in face_interior.get(face).into_iter().flatten() {
                if !ids.contains(id) {
                    ids.push(*id);
                }
            }
            for slot in 0..3 {
                let (x, y) = (face[slot], face[(slot + 1) % 3]);
                let edge = if x <= y { [x, y] } else { [y, x] };
                for id in edge_points.get(&edge).into_iter().flatten() {
                    if !ids.contains(id) {
                        ids.push(*id);
                    }
                }
            }
            if ids.len() > 3 || chords_of.contains_key(face) {
                face_ids.insert(*face, ids);
            }
        }

        // **Augmented only where it actually differs from §5.2.** Marking a face on the mere
        // presence of a trace forced every cell the surface grazes onto the fan - 44,324 of a8's
        // declines were "no facet in this cell", a cell with nothing for §7.4 to constrain and
        // every reason to keep taking §6's table.
        let mut face_why: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut failed_faces: BTreeSet<[u32; 3]> = BTreeSet::new();
        let mut face_outside_worst = 0.0f64;
        let mut face_tris: BTreeMap<[u32; 3], Vec<[u32; 3]>> = BTreeMap::new();
        let mut trace: BTreeMap<[u32; 3], SmallVec<[[Vec3; 2]; 4]>> = BTreeMap::new();
        for (face, ids) in &face_ids {
            let raw = *face;
            let geometry = [
                mesh.nodes[raw[0] as usize],
                mesh.nodes[raw[1] as usize],
                mesh.nodes[raw[2] as usize],
            ];
            let mut segments: Vec<[u32; 2]> = Vec::new();
            for chord in chords_of.get(face).into_iter().flatten() {
                let mut ends = [0u32; 2];
                for (slot, point) in chord.iter().enumerate() {
                    // The endpoint may have been snapped onto an edge when it was interned, so the
                    // raw point's key need not find it; `chord_id` records where each one went.
                    ends[slot] = chord_id
                        .get(&node_key(*point, order_quantum))
                        .copied()
                        .unwrap_or(raw[0]);
                }
                if ends[0] != ends[1] {
                    segments.push(ends);
                }
            }
            let points: Vec<Vec3> = ids.iter().map(|id| mesh.nodes[*id as usize]).collect();
            let face_keys: Vec<NodeKey> = ids.iter().map(|id| keys[*id as usize]).collect();
            let at = |id: u32| ids.iter().position(|x| *x == id).unwrap_or(0) as u32;
            let local: Vec<[u32; 2]> = segments.iter().map(|s| [at(s[0]), at(s[1])]).collect();
            let tris = match crate::meshgen::cdt::constrained_face_triangulation(
                geometry,
                &points,
                &face_keys,
                &local,
            ) {
                Ok(tris) => tris,
                Err(reason) => {
                    *face_why.entry(reason).or_insert(0) += 1;
                    failed_faces.insert(raw);
                    if reason == "a point lies outside the face" {
                        // How far outside, in the face's own barycentric units - the only scale
                        // where "outside" means anything. A few ULP is a rounding artefact and a
                        // threshold fixes it; anything larger is a point that has no business on
                        // this face and a threshold would only hide it.
                        let axis = crate::meshgen::predicates::best_projection_axis(
                            geometry[0], geometry[1], geometry[2],
                        );
                        let outward = crate::meshgen::predicates::orient2d_axis(
                            geometry[0], geometry[1], geometry[2], axis,
                        );
                        let mut worst = 0.0f64;
                        for point in &points {
                            for slot in 0..3 {
                                let side = crate::meshgen::predicates::orient2d_axis(
                                    geometry[slot],
                                    geometry[(slot + 1) % 3],
                                    *point,
                                    axis,
                                );
                                worst = worst.min(side / outward).min(0.0);
                            }
                        }
                        face_outside_worst = face_outside_worst.min(worst);
                    }
                    continue;
                }
            };
            let mine: Vec<[u32; 3]> = tris
                .iter()
                .map(|t| [ids[t[0] as usize], ids[t[1] as usize], ids[t[2] as usize]])
                .collect();
            let frozen = frozen_face_tris(
                raw,
                &cut_index,
                &second_index,
                &on_cut,
                &all_components,
                &keys,
            );
            let canon = |tris: &[[u32; 3]]| -> Vec<[u32; 3]> {
                let mut out: Vec<[u32; 3]> = tris
                    .iter()
                    .map(|t| {
                        let mut k = *t;
                        k.sort_unstable();
                        k
                    })
                    .collect();
                out.sort_unstable();
                out.dedup();
                out
            };
            let differs = match &frozen {
                Some(frozen) => canon(&mine) != canon(frozen),
                None => true,
            };
            if differs {
                face_tris.insert(raw, mine);
                if let Some(chords) = chords_of.get(face) {
                    trace.insert(raw, chords.clone());
                }
            }
        }

        // A face that carries points §5.2 has not got and could not be triangulated is the one hole
        // left in the argument: its owners read §5.2 for it and miss those points.
        // **Is every point the pass puts on a face actually ON it?** The cell's region is a
        // convex tet, so `delaunay_tets` puts on its hull exactly the points that lie on the tet's
        // surface. A trace point a hair INSIDE becomes an interior point instead, the hull keeps
        // the bare face, and the prescribed boundary cannot be matched at all - which would be a
        // structural mismatch rather than the combinatorial one flip recovery addresses. Measured
        // in the face's own units, as §6.34 had to learn to do.
        {
            let mut worst_off = 0.0f64;
            let mut off_decades = [0usize; 8];
            let mut total = 0usize;
            for (face, ids) in &face_interior {
                let corners = [
                    mesh.nodes[face[0] as usize],
                    mesh.nodes[face[1] as usize],
                    mesh.nodes[face[2] as usize],
                ];
                let normal = corners[1].sub(corners[0]).cross(corners[2].sub(corners[0]));
                let length = normal.dot(normal).sqrt();
                if length <= 0.0 {
                    continue;
                }
                let normal = normal.scale(1.0 / length);
                let shortest = (0..3)
                    .map(|slot| {
                        let d = corners[(slot + 1) % 3].sub(corners[slot]);
                        d.dot(d).sqrt()
                    })
                    .fold(f64::INFINITY, f64::min);
                for id in ids {
                    total += 1;
                    let off = normal.dot(mesh.nodes[*id as usize].sub(corners[0])).abs() / shortest;
                    worst_off = worst_off.max(off);
                    let decade = if off <= 0.0 {
                        0
                    } else {
                        ((off.log10().floor() as i64) + 16).clamp(0, 7) as usize
                    };
                    off_decades[decade] += 1;
                }
            }
            println!(
                "[PLC-PASS] {total} face-interior point(s); off the face's own plane by at worst \
                 {worst_off:.3e} of its shortest edge, by decade from 1e-16: {off_decades:?} - a \
                 point off the plane is not on the convex hull, so the prescribed boundary cannot \
                 be matched however the interior is flipped"
            );
        }

        // How flat the emitted face triangles are, in the face's own units. A tet cannot be
        // rounder than the boundary triangle it stands on, so a sliver here is a flat element
        // there - which is what `[V1]` reads as a signed volume of +-0.
        {
            let mut slivers = 0usize;
            let mut total = 0usize;
            let mut worst = f64::INFINITY;
            for tris in face_tris.values() {
                for triangle in tris {
                    total += 1;
                    let p = [
                        mesh.nodes[triangle[0] as usize],
                        mesh.nodes[triangle[1] as usize],
                        mesh.nodes[triangle[2] as usize],
                    ];
                    let cross = p[1].sub(p[0]).cross(p[2].sub(p[0]));
                    let area = cross.dot(cross).sqrt() * 0.5;
                    let longest = (0..3)
                        .map(|slot| {
                            let d = p[(slot + 1) % 3].sub(p[slot]);
                            d.dot(d).sqrt()
                        })
                        .fold(0.0f64, f64::max);
                    let shape = area / longest.powi(2).max(f64::MIN_POSITIVE);
                    if shape < 1.0e-6 {
                        slivers += 1;
                        worst = worst.min(shape);
                    }
                }
            }
            println!(
                "[PLC-PASS] {slivers} of {total} emitted face triangle(s) are slivers \
                 (area/longest^2 < 1e-6), worst {worst:.3e} - a tet cannot be rounder than the \
                 boundary triangle it stands on"
            );
        }

        // Faces that FAILED to triangulate - not merely faces `face_tris` has no entry for. A face
        // whose triangulation came out equal to §5.2's is deliberately left out of `face_tris`, and
        // counting those as unusable reported 8,868 broken faces on a6a in a run where every face
        // triangulated. A measured row that says the opposite of the truth is worse than no row.
        let unusable = failed_faces.len();
        if unusable > 0 {
            println!(
                "[PLC-PASS] {unusable} face(s) of {} carry points §5.2 has not got and could not \
                 be triangulated - their owners read §5.2 for them and miss those points, which is \
                 a hanging node each",
                face_ids.len()
            );
            for (reason, count) in &face_why {
                println!("[PLC-PASS]   face declined {count}: {reason}");
            }
            println!(
                "[PLC-PASS]   worst point-outside-face, in barycentric units: {face_outside_worst:.3e}"
            );
        }

        // Pass A, in parallel: each cell answered from its own geometry and the shared faces.
        let has_augmented_face = |tet: &[u32; 4]| -> bool {
            TET_FACES.iter().any(|slots| {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_by_key(|node| keys[*node as usize]);
                face_tris.contains_key(&face)
            })
        };
        let cell_boundary = |tet: &[u32; 4]| -> Option<Vec<[u32; 3]>> {
            // **The un-augmented faces come from `face_states`, the very function §6 and §7.6 use.**
            // Not augmented does not mean bare: §5.2 splits a face wherever its edges carry
            // crossings, and handing this cell the whole triangle presents a face its neighbour has
            // split in two - 75,797 boundary leaks on the first emitting run, every one of them on
            // a face §7.4 left alone. Reconstructing §5.2 by hand got most of the way and still
            // differed on second crossings and rims, so the pipeline's own function does it.
            let states = face_states(
                *tet,
                &cut_index,
                &second_index,
                &on_cut,
                &all_components,
                &keys,
                &mesh.nodes,
                &curve_pierce,
            );
            let mut out: Vec<[u32; 3]> = Vec::new();
            let mut any = false;
            for (slot, slots) in TET_FACES.iter().enumerate() {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_by_key(|node| keys[*node as usize]);
                if let Some(tris) = face_tris.get(&face) {
                    any = true;
                    out.extend(tris.iter().copied());
                    continue;
                }
                let (state, _, _) = &states[slot];
                let Some(state) = state.as_ref() else {
                    // A face the frozen table does not express, and not one the trace changed
                    // either - this cell is not one §7.4 can be handed.
                    return None;
                };
                let Some(tris) = face_split(state, &keys) else {
                    return None;
                };
                out.extend(tris.iter().copied());
            }
            any.then_some(out)
        };
        // **The 117 faces that could not be triangulated poison their owners, and the poison
        // spreads exactly one way.** A cell that cannot build a boundary takes §6's table, whose
        // faces are §5.2's - so any neighbour of it across an AUGMENTED face would be presenting a
        // triangulation that cell does not have. The neighbour therefore cannot take §7.4 either,
        // and so on. Iterated to a fixed point, which is affordable here precisely because the seed
        // is 117 faces rather than the whole traced shell (contrast §6.30, where seeding the same
        // iteration with 12 % of cells erased all of them).
        //
        // **And a cell that will still ESCALATE is not a neighbour §7.4 can share a face with.**
        // The escalated path does not use §5.2's chord on every face: P-3.3's crease fan
        // triangulates a curve-pierced face from the pierce point instead, and the FaceTriCache
        // makes that authoritative for both its owners. A §7.4 cell next door, reading §5.2 for
        // the same face, would present a different split - which is a crack, and was 1,437
        // boundary leaks on a6a. A cell that would have escalated but carries an augmented face is
        // not in this set: it takes §7.4 instead of escalating, so there is nothing to disagree
        // with.
        let will_escalate = |index: usize, tet: &[u32; 4]| -> bool {
            per_cell[index].escalation.is_some() && !has_augmented_face(tet)
        };
        let mut excluded: Vec<bool> = lattice
            .tets
            .par_iter()
            .enumerate()
            .map(|(index, tet)| {
                if !has_augmented_face(tet) {
                    return false;
                }
                if cell_boundary(tet).is_none() {
                    return true;
                }
                // **A face that could not be triangulated leaves this cell's boundary OPEN.** It
                // carries points §5.2 has not got, so the cell reads §5.2 for it and keeps an edge
                // whole that the neighbouring face has split - and a boundary whose edges are not
                // each shared by two triangles is not a closed surface. `constrained_tets` catches
                // that and declines, which sends the cell to the fan, and the fan cones an open
                // surface into a cell with holes in it: every one of a6a's 464 leaks came from the
                // fanned arm and none from any other (PLAN §6.33).
                if TET_FACES.iter().any(|slots| {
                    let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                    face.sort_by_key(|node| keys[*node as usize]);
                    failed_faces.contains(&face)
                }) {
                    return true;
                }
                TET_FACES.iter().any(|slots| {
                    let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                    face.sort_by_key(|node| keys[*node as usize]);
                    // Only a CURVE-PIERCED face is at issue. That is the one P-3.3's crease fan
                    // triangulates from the pierce point instead of §5.2's chord; every other face
                    // an escalated cell owns it splits by §5.2, which is what §7.4 reads too.
                    if !curve_pierce.contains_key(&face) {
                        return false;
                    }
                    owners
                        .get(&face)
                        .map(|list| {
                            list.iter().any(|other| {
                                *other as usize != index
                                    && will_escalate(
                                        *other as usize,
                                        &lattice.tets[*other as usize],
                                    )
                            })
                        })
                        .unwrap_or(false)
                })
            })
            .collect();
        for _round in 0..64 {
            let mut spread = 0usize;
            for (index, tet) in lattice.tets.iter().enumerate() {
                if excluded[index] || !has_augmented_face(tet) {
                    continue;
                }
                let poisoned = TET_FACES.iter().any(|slots| {
                    let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                    face.sort_by_key(|node| keys[*node as usize]);
                    if !face_tris.contains_key(&face) {
                        return false;
                    }
                    owners
                        .get(&face)
                        .map(|list| list.iter().any(|other| excluded[*other as usize]))
                        .unwrap_or(false)
                });
                if poisoned {
                    excluded[index] = true;
                    spread += 1;
                }
            }
            if spread == 0 {
                break;
            }
        }
        let poisoned = excluded.iter().filter(|e| **e).count();
        if poisoned > 0 {
            println!("[PLC-PASS] {poisoned} cell(s) excluded by an untriangulable face");
        }

        // The boundary first, so a declined cell still has one: it is fanned over exactly the same
        // augmented faces, and that is what keeps it conforming with a neighbour that took §7.4.
        let boundaries: Vec<Option<Vec<[u32; 3]>>> = lattice
            .tets
            .par_iter()
            .zip(excluded.par_iter())
            .map(|(tet, out)| (!out).then(|| cell_boundary(tet)).flatten())
            .collect();
        let attempt: Vec<Option<Result<PlcCell, &'static str>>> = lattice
            .tets
            .par_iter()
            .zip(boundaries.par_iter())
            .map(|(tet, boundary)| {
                boundary.as_ref().map(|boundary| {
                    plc_attempt(
                        *tet,
                        boundary,
                        &mesh.nodes,
                        &all_components,
                        classifier,
                        tol,
                    )
                })
            })
            .collect();

        let attempted = attempt.iter().filter(|a| a.is_some()).count();
        let taken = attempt.iter().filter(|a| matches!(a, Some(Ok(_)))).count();
        let tets: usize = attempt
            .iter()
            .flatten()
            .filter_map(|a| a.as_ref().ok())
            .map(|c| c.tets.len())
            .sum();
        let mut why: BTreeMap<&'static str, usize> = BTreeMap::new();
        for outcome in attempt.iter().flatten() {
            if let Err(reason) = outcome {
                *why.entry(reason).or_insert(0) += 1;
            }
        }
        println!(
            "[PLC-PASS] {attempted} traced cell(s) of {}; §7.4 takes {taken} of them and emits \
             {tets} tet(s) for those, {:.2} per cell - the rest keep the augmented faces and are \
             fanned",
            lattice.tets.len(),
            tets as f64 / taken.max(1) as f64
        );
        for (reason, count) in &why {
            println!("[PLC-PASS]   {count} declined: {reason}");
        }
        // **What the fan is actually handed.** The fan cones its boundary and asks nothing of it,
        // so every property the cell ends up with is a property the boundary already had: a
        // triangle listed twice becomes two tets carrying the same face, and an edge with one
        // triangle on it becomes a hole. `constrained_tets` refuses both, which is why the meshed
        // arm is clean and the fanned arm carries all of the residual - the defect is in
        // `cell_boundary`, and this counts it there rather than at the far end.
        // **Does a face-interior point predict the decline?** 2,393 of them against 2,028
        // declines is close enough to be worth asking directly rather than inferring. A point on
        // a lattice EDGE stays on the tet's surface however it rounds - the edge is the
        // intersection of two faces - but a point in a face's INTERIOR is on the hull only if it
        // rounds to the outside of that one plane, and `delaunay_tets` decides that with an exact
        // predicate, which has no rounding level to hide in.
        let mut has_interior_point: Vec<bool> = vec![false; lattice.tets.len()];
        for face in face_interior.keys() {
            for owner in owners.get(face).into_iter().flatten() {
                has_interior_point[*owner as usize] = true;
            }
        }
        let mut table = [[0usize; 2]; 2];
        for (index, outcome) in attempt.iter().enumerate() {
            let Some(outcome) = outcome else { continue };
            table[usize::from(has_interior_point[index])][usize::from(outcome.is_err())] += 1;
        }
        println!(
            "[PLC-PASS] cells by whether any of their faces carries an interior trace point, \
             against whether §7.4 declined: no point {} took / {} declined, has one {} took / {} \
             declined",
            table[0][0], table[0][1], table[1][0], table[1][1]
        );

        let mut open_taken = 0usize;
        let mut open_fanned = 0usize;
        let mut dup_taken = 0usize;
        let mut dup_fanned = 0usize;
        for (boundary, outcome) in boundaries.iter().zip(attempt.iter()) {
            let (Some(boundary), Some(outcome)) = (boundary, outcome) else { continue };
            let mut seen: BTreeSet<[u32; 3]> = BTreeSet::new();
            let mut duplicated = false;
            let mut edges: BTreeMap<[u32; 2], usize> = BTreeMap::new();
            for triangle in boundary {
                let mut key = *triangle;
                key.sort_unstable();
                if !seen.insert(key) {
                    duplicated = true;
                }
                for slot in 0..3 {
                    let (x, y) = (triangle[slot], triangle[(slot + 1) % 3]);
                    *edges.entry(if x <= y { [x, y] } else { [y, x] }).or_insert(0) += 1;
                }
            }
            let open = edges.values().any(|n| *n != 2);
            match outcome.is_ok() {
                true => {
                    open_taken += usize::from(open);
                    dup_taken += usize::from(duplicated);
                }
                false => {
                    open_fanned += usize::from(open);
                    dup_fanned += usize::from(duplicated);
                }
            }
        }
        if open_taken + open_fanned + dup_taken + dup_fanned > 0 {
            println!(
                "[PLC-PASS] the boundaries themselves: {open_fanned} fanned cell(s) and \
                 {open_taken} meshed one(s) were handed a boundary that is not closed, \
                 {dup_fanned} fanned and {dup_taken} meshed one(s) a boundary listing some \
                 triangle twice"
            );
        }
        plc_boundary = boundaries.clone();
        for (edge, ids) in &edge_points {
            plc_edge_points.insert(*edge, ids.to_vec());
        }
        for (face, tris) in &face_tris {
            plc_face_tris.insert(*face, tris.clone());
            plc_face_owners_n.insert(*face, owners.get(face).map(|o| o.len()).unwrap_or(0));
        }

        // **Out of the per-cell arenas and into the mesh.** A tet's vertices below `seed.len()` are
        // nodes the boundary already had; the rest are the facet's own vertices, private to this
        // cell, and interned now that the cell is known to be kept. Interning them earlier would
        // leave a node behind for every cell that declined, and a node nothing references is what
        // `[V3]` calls a hanging node.
        for (index, outcome) in attempt.into_iter().enumerate() {
            let Some(outcome) = outcome else { continue };
            plc[index] = Some(match outcome {
                Ok(cell) => {
                    let mut map: Vec<u32> = Vec::with_capacity(cell.points.len());
                    for (local, point) in cell.points.iter().enumerate() {
                        if local < cell.seed.len() {
                            map.push(cell.seed[local]);
                            continue;
                        }
                        let key = node_key(*point, order_quantum);
                        let id = *global.entry(key).or_insert_with(|| {
                            let id = mesh.nodes.len() as u32;
                            mesh.nodes.push(*point);
                            keys.push(key);
                            id
                        });
                        map.push(id);
                    }
                    PlcPlan::Meshed {
                        tets: cell
                            .tets
                            .iter()
                            .map(|t| {
                                [
                                    map[t[0] as usize],
                                    map[t[1] as usize],
                                    map[t[2] as usize],
                                    map[t[3] as usize],
                                ]
                            })
                            .collect(),
                        regions: cell.regions,
                        caps: cell
                            .caps
                            .iter()
                            .map(|(t, component)| {
                                (
                                    [map[t[0] as usize], map[t[1] as usize], map[t[2] as usize]],
                                    *component,
                                )
                            })
                            .collect(),
                    }
                }
                Err(_) => PlcPlan::Fan {
                    boundary: boundaries[index].clone().unwrap_or_default(),
                },
            });
        }
    }

    // §7.3's `FaceTriCache`, wired in as a **consistency check first** (P-3, face-first
    // integration). Every escalated cell triangulates the faces it shares with its neighbours,
    // and today each cell computes that independently - they agree only because `face_mesh` is a
    // pure function of the face. The cache makes that sharing explicit and, more usefully,
    // *detects* any face where the two cells disagree, which is invariant J1 measured on real
    // meshes rather than argued from the signature. Nothing consumes its triangles yet: this run
    // must be a no-op, and `face_cache_conflicts` is the number that says whether the switch is
    // safe to make.
    // **P-3.13 step 1 (`RUSTMSPT_FACE_TRACE=1`): intern the surface's trace on each lattice face.**
    // §7.2's mesher needs the fragment; conformity needs the *face* to own the nodes the fragment
    // puts on it. `trace_on_face` is a pure function of the face and the surface (cdt.rs), so both
    // cells derive the same segments - this map turns them into interned nodes once, keyed by the
    // face's three corners exactly as `face_steiner` and `curve_pierce` are. Nothing consumes it
    // yet: this step must be a **no-op**, and the numbers it reports are what say the population is
    // real and the keying is sound before anything is wired to it.
    // Stored as POINTS, not node ids. Interning a node the mesh does not yet reference leaves it
    // sitting on a lattice face with nothing using it, and `[V3]` rightly calls that a hanging
    // node - measured: a8 fails `[V3]` outright with 561,929 such orphans. The interning happens
    // when a consumer takes the trace, not when it is computed.
    let face_trace_on = std::env::var_os("RUSTMSPT_FACE_TRACE").is_some();
    let mut face_trace: BTreeMap<[u32; 3], SmallVec<[Vec3; 4]>> = BTreeMap::new();
    let mut face_trace_inside: BTreeSet<[u32; 3]> = BTreeSet::new();
    let mut face_trace_faces = 0usize;
    let mut face_trace_reaching = 0usize;
    let mut face_trace_interior = 0usize;
    let mut face_trace_nodes = 0usize;
    if face_trace_on {
        let mut seen: BTreeSet<[u32; 3]> = BTreeSet::new();
        for tet in lattice.tets.iter() {
            for slots in TET_FACES {
                let mut corners = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                corners.sort_by_key(|node| keys[*node as usize]);
                if !seen.insert(corners) {
                    continue;
                }
                let face = [
                    mesh.nodes[corners[0] as usize],
                    mesh.nodes[corners[1] as usize],
                    mesh.nodes[corners[2] as usize],
                ];
                let mut ids: SmallVec<[Vec3; 4]> = SmallVec::new();
                let mut seen_keys: SmallVec<[NodeKey; 4]> = SmallVec::new();
                for component in &all_components {
                    let Some(slot) = classifier.slot_of(*component) else { continue };
                    let tris = classifier.triangles_of(slot);
                    if tris.is_empty() {
                        continue;
                    }
                    for segment in crate::meshgen::cdt::trace_on_face(face, tris, options.eps) {
                        for point in segment {
                            // Deduplicated per face by node key, so the two cells sharing it get
                            // the same point set whatever order their triangles arrive in.
                            let key = node_key(point, order_quantum);
                            if !seen_keys.contains(&key) {
                                seen_keys.push(key);
                                ids.push(point);
                            }
                        }
                    }
                }
                if !ids.is_empty() {
                    // **Which traces need new machinery, and which are already expressible.** A
                    // trace that reaches the face's boundary terminates on a lattice edge, so that
                    // edge carries a crossing and §5.2's table already has the chord as an edge -
                    // those faces are handled and re-triangulating them would rework most of the
                    // mesh for nothing. A trace that stays strictly INSIDE the face is the
                    // sub-cell body seen from the face (PLAN §6.17): no edge is crossed, so no
                    // table row expresses it, and this is the population §7.2's mesher exists for.
                    let on_boundary = |p: Vec3| -> bool {
                        let normal = face[1].sub(face[0]).cross(face[2].sub(face[0]));
                        let scale = normal.dot(normal).sqrt();
                        (0..3).any(|k| {
                            let (u, v) = (face[k], face[(k + 1) % 3]);
                            let along = v.sub(u);
                            let len2 = along.dot(along);
                            if len2 <= 0.0 {
                                return false;
                            }
                            let rel = p.sub(u);
                            let t = rel.dot(along) / len2;
                            if !(-0.001..=1.001).contains(&t) {
                                return false;
                            }
                            let off = rel.sub(along.scale(t));
                            off.dot(off) <= 1.0e-12 * scale
                        })
                    };
                    if ids.iter().any(|p| on_boundary(*p)) {
                        face_trace_reaching += 1;
                    } else {
                        face_trace_interior += 1;
                        face_trace_inside.insert(corners);
                    }
                    face_trace_nodes += ids.len();
                    face_trace_faces += 1;
                    face_trace.insert(corners, ids);
                }
            }
        }
    }
    // P-3.3: for each face a locked curve pierces, how many of its (at most two) owning cells
    // escalated. Only a face escalated on *both* sides can have its triangulation changed by the
    // junction path alone - the other side would keep §5.2's straight chord and the two would stop
    // matching, which is how the two earlier crease attempts cracked the mesh. Built once the
    // per-cell results are final, so the condition reads the escalation the mesh will actually
    // have - P-3.13 step 3 escalated cells here and this had to see it (§6.24).
    let mut pierced_owners: BTreeMap<[u32; 3], (u8, u8)> = BTreeMap::new();
    if !curve_pierce.is_empty() {
        for (index, tet) in lattice.tets.iter().enumerate() {
            let escalated = per_cell[index].escalation.is_some();
            for slots in TET_FACES {
                let mut corners = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                corners.sort_by_key(|node| keys[*node as usize]);
                if !curve_pierce.contains_key(&corners) {
                    continue;
                }
                let entry = pierced_owners.entry(corners).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += u8::from(escalated);
            }
        }
    }

    // **P-3.13 step 2, the census that decides whether the face-side wiring can be conforming.**
    // A face carrying an interior-only loop may only be re-triangulated if *every* cell owning it
    // derives the same triangulation - the condition P-3.3 proved for the crease fan and the one
    // P-3.12 violated, which is why it cracked `[V3]` on two of three cases. The sufficient version
    // of it here is that both owners are in the population the new path will claim: a cell with no
    // crossed edge that nevertheless carries a trace on one of its faces. Such a cell has nothing
    // for §5.2's table to cut, so it is emitted whole today and is free to be re-triangulated; a
    // cell *with* a crossed edge takes the table and would keep the face's plain triangle.
    // Print-only. The number that matters is `both`, because the difference between it and
    // `interior` is the part of the population the guard has to decline.
    let mut subcell_cells = 0usize;
    let mut subcell_clean = 0usize;
    let mut subcell_ready = 0usize;
    // Every interior face classified by what owns it, exhaustively - the four buckets sum to the
    // interior count, so nothing is inferred by subtraction.
    let mut face_all_clean = 0usize;
    let mut face_all_escalated = 0usize;
    let mut face_mixed = 0usize;
    let mut face_has_table = 0usize;
    let mut cells_with_a_trace = 0usize;
    let mut escalated_with_traces = 0usize;
    let mut escalated_fully_surrounded = 0usize;
    let mut cells_interior_escalated = 0usize;
    let mut cells_interior_uncut = 0usize;
    let mut cells_interior_table = 0usize;
    if face_trace_on {
        let traced_faces = |tet: &[u32; 4]| -> SmallVec<[[u32; 3]; 4]> {
            TET_FACES
                .iter()
                .filter_map(|slots| {
                    let mut corners = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                    corners.sort_by_key(|node| keys[*node as usize]);
                    face_trace.contains_key(&corners).then_some(corners)
                })
                .collect()
        };
        let uncut_cell = |tet: &[u32; 4]| -> bool {
            (0..4).all(|a| {
                ((a + 1)..4).all(|b| {
                    let (x, y) = (tet[a], tet[b]);
                    let edge = if x <= y { [x, y] } else { [y, x] };
                    !all_components
                        .iter()
                        .any(|component| cut_index.contains_key(&(edge, *component)))
                })
            })
        };
        let mut clean: Vec<bool> = vec![false; lattice.tets.len()];
        for (index, tet) in lattice.tets.iter().enumerate() {
            let faces = traced_faces(tet);
            if !faces.is_empty() {
                // Every cell the surface passes through - the population §7.1's triage would have
                // to cover for §7.4's face triangulation to be deliverable everywhere it differs.
                cells_with_a_trace += 1;
            }
            if faces.is_empty() || !uncut_cell(tet) {
                continue;
            }
            subcell_cells += 1;
            // A cell all of whose traced faces stay interior is one the new path can take whole.
            // One face reaching the boundary means the surface leaves through a lattice edge that
            // carries no crossing - which happens when S7 snapped the crossing onto a vertex, so
            // the cell is handled by the on-cut path and keeps its faces plain. Mixing the two on
            // one cell is what would leave a loop unrepresented on the face it exits through.
            if faces.iter().all(|face| face_trace_inside.contains(face)) {
                clean[index] = true;
                subcell_clean += 1;
            }
        }
        // **Every interior face, by what owns it.** Two owner conditions can deliver a shared
        // triangulation: both owners clean (nothing cut, so the new path controls both), or both
        // escalated (P-3.3's proven condition, already served by §7.3's face cache). Anything else
        // has an owner that takes §5.2's table, which reads the face's *edge crossings* - and an
        // interior loop crosses no edge, so that owner keeps the plain triangle whatever the other
        // one does. Counted exhaustively rather than by subtraction, because the interesting
        // possibility is the fourth bucket being the large one.
        let mut owners: BTreeMap<[u32; 3], (u8, u8, u8)> = BTreeMap::new();
        for (index, tet) in lattice.tets.iter().enumerate() {
            let escalated = per_cell[index].escalation.is_some();
            for slots in TET_FACES {
                let mut corners = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                corners.sort_by_key(|node| keys[*node as usize]);
                if !face_trace_inside.contains(&corners) {
                    continue;
                }
                let entry = owners.entry(corners).or_insert((0, 0, 0));
                entry.0 += 1;
                entry.1 += u8::from(escalated);
                entry.2 += u8::from(clean[index] && !escalated);
            }
        }
        for (all, escalated, clean_owners) in owners.values() {
            if clean_owners == all {
                face_all_clean += 1;
            } else if escalated == all {
                face_all_escalated += 1;
            } else if escalated + clean_owners == *all {
                face_mixed += 1;
            } else {
                face_has_table += 1;
            }
        }
        // **Can §7.4's face triangulation ever be delivered, given who owns the faces?** An
        // augmented face - one carrying the surface's trace as edges - may only be used where
        // EVERY owner will use the same one. A face carrying a trace is traced from both sides by
        // construction, so the condition reduces to "both owners take the new path". Counted here
        // against the population that takes it today, the escalated cells, because if almost none
        // of them are surrounded by escalated cells then restricting §7.4 to escalation is dead and
        // the path has to be offered to every cell the surface passes through instead.
        //
        // Indexed once rather than scanned per face: the obvious nested loop is quadratic in the
        // lattice and would not finish on a8's 924,636 cells.
        let mut traced_owners: BTreeMap<[u32; 3], (u16, u16)> = BTreeMap::new();
        for (index, tet) in lattice.tets.iter().enumerate() {
            let escalated = per_cell[index].escalation.is_some();
            for slots in TET_FACES {
                let mut corners = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                corners.sort_by_key(|node| keys[*node as usize]);
                if !face_trace.contains_key(&corners) {
                    continue;
                }
                let entry = traced_owners.entry(corners).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += u16::from(escalated);
            }
        }
        for (index, tet) in lattice.tets.iter().enumerate() {
            if per_cell[index].escalation.is_none() {
                continue;
            }
            let mut traced = 0usize;
            let mut all_escalated = true;
            for slots in TET_FACES {
                let mut corners = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                corners.sort_by_key(|node| keys[*node as usize]);
                let Some((owners, escalated)) = traced_owners.get(&corners) else { continue };
                traced += 1;
                if owners != escalated {
                    all_escalated = false;
                }
            }
            if traced == 0 {
                continue;
            }
            escalated_with_traces += 1;
            if all_escalated {
                escalated_fully_surrounded += 1;
            }
        }

        // **The cells behind the fourth bucket, and the rule that would reach them.** A cell that
        // takes §5.2's table has a crossed edge - so a component crossing its edges is cut - while
        // a *different* component passes through one of its faces without crossing any edge of the
        // cell at all. `crossing_components` sees only the first, so no junction is declared and
        // the table represents the second nowhere. That is computable from the cell alone: a
        // component whose surface enters the cell but crosses none of its six edges cannot be
        // expressed by any row of the table. Counted here as the size of that escalation trigger.
        for (index, tet) in lattice.tets.iter().enumerate() {
            let interior = TET_FACES.iter().any(|slots| {
                let mut corners = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                corners.sort_by_key(|node| keys[*node as usize]);
                face_trace_inside.contains(&corners)
            });
            if !interior {
                continue;
            }
            if per_cell[index].escalation.is_some() {
                cells_interior_escalated += 1;
            } else if uncut_cell(tet) {
                cells_interior_uncut += 1;
            } else {
                cells_interior_table += 1;
            }
        }
        // **The set the wiring can actually claim.** A clean cell may still be unreachable: if one
        // of its faces is shared with a cell that keeps the plain triangle, the loop on that face
        // cannot become an edge, and a piece boundary crossing it is exactly the hanging node
        // P-3.12 produced. So the claimable cell is one that is clean *and* every one of its
        // traced faces has clean owners on both sides. This is the honest size of the step.
        for (index, tet) in lattice.tets.iter().enumerate() {
            if !clean[index] {
                continue;
            }
            if traced_faces(tet).iter().all(|face| {
                owners.get(face).map(|(all, _, mine)| mine == all).unwrap_or(false)
            }) {
                subcell_ready += 1;
            }
        }
    }
    // For the §7.4 diagnostic only: who owns each lattice face, and how many of them escalated.
    // An augmented face triangulation may only be used where every owner uses the same one, so this
    // is what says whether §7.1's triage can stay as frozen or has to widen.
    let mut plc_face_owners: BTreeMap<[u32; 3], (u16, u16)> = BTreeMap::new();
    if std::env::var_os("RUSTMSPT_PLC_DIAG").is_some() {
        for (index, tet) in lattice.tets.iter().enumerate() {
            let escalated = per_cell[index].escalation.is_some();
            for slots in TET_FACES {
                let mut corners = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                corners.sort_by_key(|node| keys[*node as usize]);
                let entry = plc_face_owners.entry(corners).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += u16::from(escalated);
            }
        }
    }
    let mut face_cache = crate::meshgen::facecache::FaceTriCache::new();
    let mut face_cache_conflicts = 0usize;
    // P-3.3's census, print-only: how many faces a locked curve pierces are nevertheless
    // *expressible*, i.e. take §5.2's table and get a straight chord drawn across the kink.
    // That is the crease damage's face-level signature, and its size decides whether the
    // face-first route is worth the change. Keyed by face so a shared one counts once.
    let mut crease_faces: BTreeSet<[u32; 3]> = BTreeSet::new();
    let mut crease_faces_expressible: BTreeSet<[u32; 3]> = BTreeSet::new();
    let mut crease_faces_both_escalated: BTreeSet<[u32; 3]> = BTreeSet::new();
    let mut crease_faces_fanned: BTreeSet<[u32; 3]> = BTreeSet::new();
    let mut plc_meshed = 0usize;
    let mut plc_fanned = 0usize;
    // Which path each cell took, so a leak can be charged to one instead of guessed at.
    let mut path_of: Vec<u8> = vec![0; lattice.tets.len()];
    for (index, cell) in per_cell.into_iter().enumerate() {
        // **§7.1 as amended (rev 1.5): a cell the surface's trace changes is meshed by §7.2/§7.4
        // ahead of §6's table.** Both arms keep the augmented faces, which is what lets a cell that
        // took the mesher and a cell that fell back to the fan share one and still meet.
        if let Some(plan) = plc[index].take() {
            let parent_record = &classification.records[index];
            match plan {
                PlcPlan::Meshed {
                    tets,
                    regions,
                    caps,
                } => {
                    // Tagged before the tets are pushed, so the recorded index is this cell's
                    // first - the same convention §7.6's path uses.
                    for (triangle, component) in caps {
                        pending_interfaces.push((mesh.tets.len(), triangle, component));
                    }
                    // **§7.5: one classification per sub-region, not per tet.** A region is a
                    // connected run of tets no constraint face separates, so every tet in it holds
                    // the same material; sampling each one over again would spend classifier
                    // queries to re-derive that, and would disagree with itself wherever a
                    // centroid lands on the surface.
                    let mut region_record: BTreeMap<u32, OwnershipRecord> = BTreeMap::new();
                    for (tet, region) in tets.iter().zip(regions.iter()) {
                        if region_record.contains_key(region) {
                            continue;
                        }
                        let record = seed_record(
                            parent_record,
                            *tet,
                            &all_components,
                            &mesh.nodes,
                            classifier,
                            &mut seed_uncertain,
                        );
                        region_record.insert(*region, record);
                    }
                    for (tet, region) in tets.iter().zip(regions.iter()) {
                        let Some(oriented) = orient_positively(*tet, &mesh.nodes) else {
                            mesh.stats.n_degenerate_fan_pieces += 1;
                            continue;
                        };
                        mesh.tets.push(oriented);
                        mesh.records
                            .push(region_record.get(region).cloned().unwrap_or_default());
                        seeded_pieces += 1;
                        mesh.parent_of.push(index as u32);
                        mesh.regime.push(REGIME_NORMAL);
                        mesh.band_region.push(-1);
                    }
                    plc_meshed += 1;
                    path_of[index] = 1;
                }
                PlcPlan::Fan { boundary } => {
                    let centroid = polygon_soup_centroid(&boundary, &mesh.nodes);
                    let centroid_id = mesh.nodes.len() as u32;
                    mesh.nodes.push(centroid);
                    keys.push(node_key(centroid, quantum));
                    let fanned = fan_cell(&boundary, centroid_id);
                    for piece in &fanned.tets {
                        let Some(oriented) = orient_positively(*piece, &mesh.nodes) else {
                            mesh.stats.n_degenerate_fan_pieces += 1;
                            continue;
                        };
                        mesh.tets.push(oriented);
                        mesh.records.push(seed_record(
                            parent_record,
                            oriented,
                            &all_components,
                            &mesh.nodes,
                            classifier,
                            &mut seed_uncertain,
                        ));
                        seeded_pieces += 1;
                        mesh.parent_of.push(index as u32);
                        mesh.regime.push(REGIME_NORMAL);
                        mesh.band_region.push(-1);
                    }
                    plc_fanned += 1;
                    path_of[index] = 2;
                }
            }
            continue;
        }
        if let Some(reason) = cell.escalation {
            path_of[index] = 3;
            mesh.escalated.push((index as u32, reason));
            *mesh.stats.n_escalated.entry(reason).or_insert(0) += 1;
            // The cell cannot take §6's path, but it still has to *conform*: its
            // neighbours have split the faces they share with it. G6-0's outcome is
            // to re-mesh it as a fan from its centroid over the same face
            // triangulations its neighbours use (`junction.rs`).
            let tet = lattice.tets[index];
            let faces = face_states(
                tet,
                &cut_index,
                &second_index,
                &on_cut,
                &all_components,
                &keys,
                &mesh.nodes,
                &curve_pierce,
            );
            let mut boundary: SmallVec<[[u32; 3]; 16]> = SmallVec::new();
            // Where a face's two chords meet, the node sits on **both** surfaces - it is
            // a point of the intersection curve. The split below has to know that, or it
            // asks the classifier for a side at a point exactly on the surface, gets an
            // arbitrary answer, and the cap triangles come out with mixed sides.
            let mut meeting_nodes: Vec<(u32, [i32; 2])> = Vec::new();
            for (state, loop_nodes, crossed) in &faces {
                // Where this face's triangles start, so they can be lifted back out below
                // whichever branch produced them and checked against the cache.
                let face_begin = boundary.len();
                if let Some(state) = state.as_ref() {
                    if curve_pierce.contains_key(&state.nodes) {
                        crease_faces.insert(state.nodes);
                        crease_faces_expressible.insert(state.nodes);
                        if let Some((owners, escalated)) = pierced_owners.get(&state.nodes) {
                            if owners == escalated {
                                crease_faces_both_escalated.insert(state.nodes);
                            }
                        }
                    }
                } else if let Some(face) = crossed.as_ref() {
                    let mut corners = [face.loop_nodes[0]; 3];
                    let mut slot = 0usize;
                    for node in &face.loop_nodes {
                        if (*node as usize) < n_parent_nodes as usize && slot < 3 {
                            corners[slot] = *node;
                            slot += 1;
                        }
                    }
                    corners.sort_by_key(|node| keys[*node as usize]);
                    if curve_pierce.contains_key(&corners) {
                        crease_faces.insert(corners);
                    }
                }
                // G7-1's doubly-cut face rule is applied here, to the *face*, before
                // anything cell-level is decided - and that is load-bearing rather
                // than tidy. A face two walls cross is shared by two cells, and only
                // one of them may turn out to be a sandwich the cell-level split
                // covers; if the rule ran only inside that split, the two cells would
                // triangulate their shared face differently and the mesh would grow
                // hanging nodes exactly where the gap is. Run per face, both cells
                // get the same triangles whatever either of them does next.
                if let Some((split, _)) = band_face_split(loop_nodes, n_parent_nodes, &keys) {
                    mesh.stats.n_doubly_cut_faces += 1;
                    boundary.extend(split.into_iter().map(|(_, triangle)| triangle));
                    check_face_cache(
                        &mut face_cache,
                        &mut face_cache_conflicts,
                        loop_nodes,
                        n_parent_nodes,
                        &keys,
                        &mesh.nodes,
                        crossed.as_ref(),
                        false,
                        &boundary[face_begin..],
                    );
                    continue;
                }
                // **P-3.3: the kink becomes an edge.** A face a locked curve pierces is still
                // *expressible* whenever one component leaves it two cut nodes - which is what a
                // crease looks like, since both patches belong to the same surface - so §5.2 draws
                // one straight chord between them and the material boundary chamfers across the
                // sharp edge. That is 85.7 % of the P3 damage (§6.1), and no amount of
                // tetrahedralising the cell can repair it: the crease has to be an *edge*, so the
                // pierce point has to be a *node* first.
                //
                // The condition is `every owning cell escalated`, and it is not timidity. A face
                // shared with a cell that took §6's table would keep the straight chord on that
                // side while this side fanned, and the two triangulations of one shared face is
                // precisely how the two earlier crease attempts cracked the mesh. It is a pure
                // function of the *face* - both owners compute the same predicate from the same
                // map - so J1 survives it, and §6's path can be brought along later without
                // reworking this. Of the expressible pierced faces it admits a8 626 of 1,035,
                // a7b 107 of 113, a6b 62 of 72, a6a 37 of 84, a3 109 of 285, a7a 3 of 10.
                let crease_hub = state.as_ref().and_then(|state| {
                    let point = curve_pierce.get(&state.nodes)?;
                    let (owners, escalated) = pierced_owners.get(&state.nodes)?;
                    if owners != escalated {
                        return None;
                    }
                    // A crease only: every component meeting along the piercing curve has to
                    // actually **cut an edge** of this face, or the new node claims a junction
                    // the mesh has no material for (see `pierce_components`). `on_cut` does not
                    // count towards it: a corner S7 snapped onto a patch says the node is on that
                    // surface, not that the surface crosses this face, and admitting it let A-3's
                    // cube-sphere intersection curves through - all six of `[V9]`'s
                    // `curve_node_id` failures and a fourfold rise in `[V5]`'s misattributed
                    // slivers, for +0.002 of P3.
                    let cutting: SmallVec<[i32; 2]> = all_components
                        .iter()
                        .copied()
                        .filter(|component| {
                            (0..3).any(|slot| {
                                let (a, b) = (state.nodes[slot], state.nodes[(slot + 1) % 3]);
                                let edge = if a <= b { [a, b] } else { [b, a] };
                                cut_index.contains_key(&(edge, *component))
                                    || second_index.contains_key(&(edge, *component))
                            })
                        })
                        .collect();
                    let members = pierce_components.get(&state.nodes)?;
                    if !members.iter().all(|component| cutting.contains(component)) {
                        return None;
                    }
                    Some((state.nodes, *point))
                });
                if let Some((corners, point)) = crease_hub {
                    // Interned per face by the same map the crossed and centroid paths use, so
                    // the second cell finds the node rather than making a coincident duplicate.
                    let id = match face_steiner.get(&corners) {
                        Some(id) => *id,
                        None => {
                            let id = mesh.nodes.len() as u32;
                            mesh.nodes.push(point);
                            keys.push(node_key(point, order_quantum));
                            face_steiner.insert(corners, id);
                            id
                        }
                    };
                    // On a locked curve, so on every surface that meets along it - the same
                    // statement the crossed and loop-fan paths make about a pierce point, and
                    // §7.6 needs it or its split asks the classifier for a side at a point
                    // exactly on a surface it was not told about. Narrowing this to the
                    // components that actually cut the face was tried and measured **neutral**
                    // on every case, so the broader statement stands as the simpler one.
                    for component in &all_components {
                        meeting_nodes.push((id, [*component, *component]));
                    }
                    crease_faces_fanned.insert(corners);
                    boundary.extend(loop_fan(loop_nodes, id));
                    if let Some(shared) = check_face_cache(
                        &mut face_cache,
                        &mut face_cache_conflicts,
                        loop_nodes,
                        n_parent_nodes,
                        &keys,
                        &mesh.nodes,
                        crossed.as_ref(),
                        true,
                        &boundary[face_begin..],
                    ) {
                        boundary.truncate(face_begin);
                        boundary.extend(orient_face_outward(&shared, tet, &mesh.nodes));
                    }
                    continue;
                }
                match face_mesh(state.as_ref(), loop_nodes, &keys, crossed.as_ref()) {
                    FaceMesh::Table(triangles) => boundary.extend(triangles),
                    FaceMesh::Crossed(face) => {
                        mesh.stats.n_crossed_faces += 1;
                        // A meeting point is needed only where the chords actually cross;
                        // when they do it is interned per *face* like the centroid Steiner
                        // point below, and for the same reason: the two cells sharing this
                        // face must cone to the same node or the shared face stops
                        // matching. The key is the face's three parent corners, which both
                        // cells derive from the face alone.
                        let mut on_curve = false;
                        let id = match face.point {
                            None => u32::MAX,
                            Some(point) => {
                                // G6-0: the chords' closest approach is where the two cut
                                // *chords* meet, which is only an estimate of where the two
                                // surfaces do. When the curve itself pierces this face, that
                                // is the answer, and it is exact.
                                let mut corners = [face.loop_nodes[0]; 3];
                                let mut slot = 0usize;
                                for node in &face.loop_nodes {
                                    if (*node as usize) < snapped.nodes.len() && slot < 3 {
                                        corners[slot] = *node;
                                        slot += 1;
                                    }
                                }
                                corners.sort_by_key(|node| keys[*node as usize]);
                                let point = match curve_pierce.get(&corners) {
                                    Some(pierce) => {
                                        on_curve = true;
                                        *pierce
                                    }
                                    None => point,
                                };
                                match face_steiner.get(&corners) {
                                    Some(id) => *id,
                                    None => {
                                        let id = mesh.nodes.len() as u32;
                                        mesh.nodes.push(point);
                                        keys.push(node_key(point, order_quantum));
                                        face_steiner.insert(corners, id);
                                        id
                                    }
                                }
                            }
                        };
                        // A point on a locked curve lies on **every** surface that meets along
                        // it, not just on the two whose chords happened to name it. Telling
                        // the split only the guessed pair is what leaves `mixed sides on one
                        // triangle` on a triangle whose corner is exactly on a third surface:
                        // A-6a's `[42299, 27586, 11041]`, where 42299 sits on the limb's rim
                        // at x = 0.5817 *and* z = 0.2977 and the chords that found it both
                        // belong to component 2. Where the point is only the chords' closest
                        // approach it is on those two surfaces and no others, so the pair
                        // stands. `LoopFan` already draws this distinction; the two branches
                        // differing on it was an oversight, not a decision.
                        if id != u32::MAX {
                            if on_curve {
                                for component in &all_components {
                                    meeting_nodes.push((id, [*component, *component]));
                                }
                            } else {
                                meeting_nodes.push((id, [face.components[0], face.components[1]]));
                            }
                        }
                        boundary.extend(crossed_face_mesh(&face, &keys, id));
                    }
                    FaceMesh::LoopFan(loop_nodes) => {
                        mesh.stats.n_generic_faces += 1;
                        // One Steiner point per *face*, not per cell: the two cells
                        // sharing it must use the same node, or the coincident
                        // duplicates make the shared face non-conforming again.
                        let mut corners = [loop_nodes[0]; 3];
                        let mut slot = 0usize;
                        for node in &loop_nodes {
                            if (*node as usize) < snapped.nodes.len() && slot < 3 {
                                corners[slot] = *node;
                                slot += 1;
                            }
                        }
                        corners.sort_by_key(|node| keys[*node as usize]);
                        // G6-0, the second way a curve reaches a face: through one of the
                        // walk's own nodes rather than through the interior. S7 snaps nodes
                        // onto locked curves on purpose, so this is the *common* case near a
                        // rim, and `segment_pierces_triangle` reports nothing for it by
                        // design. Every trace on such a face terminates at that node, so a
                        // fan from it keeps all of them as edges and interns nothing - both
                        // cells derive the same apex from the face alone, so J1 holds exactly
                        // as it does for the piercing point. Read off A-6a's remaining 21:
                        // face `(11057, 27364, 42321)`, where 11057 sits at x = 0.5817 *and*
                        // z = 0.2977 - on the rim - while 42321 is the face centroid the fan
                        // used instead, 3.2e-4 off the contact plane. More than one such node
                        // means the curve runs *along* a walk edge, which is a different
                        // shape and is left to the centroid.
                        let hub = if curve_pierce.contains_key(&corners) {
                            None
                        } else {
                            let on: SmallVec<[usize; 2]> = loop_nodes
                                .iter()
                                .enumerate()
                                .filter(|(_, node)| curve_nodes.contains(node))
                                .map(|(slot, _)| slot)
                                .collect();
                            (on.len() == 1)
                                .then(|| fan_from_walk_node(&loop_nodes, on[0], &mesh.nodes))
                                .flatten()
                                .map(|fan| (loop_nodes[on[0]], fan))
                        };
                        if let Some((hub_id, fan)) = hub {
                            for component in &all_components {
                                meeting_nodes.push((hub_id, [*component, *component]));
                            }
                            boundary.extend(fan);
                            check_face_cache(
                                &mut face_cache,
                                &mut face_cache_conflicts,
                                &loop_nodes,
                                n_parent_nodes,
                                &keys,
                                &mesh.nodes,
                                crossed.as_ref(),
                                false,
                                &boundary[face_begin..],
                            );
                            continue;
                        }
                        let id = match face_steiner.get(&corners) {
                            Some(id) => *id,
                            None => {
                                // G6-0: on the curve where one pierces this face, and at the
                                // face's own centre otherwise. Both are pure functions of the
                                // face, so the two cells sharing it still agree; the
                                // difference is that the first puts a mesh node on locked
                                // geometry, which is the only way the material boundary can
                                // follow the curve instead of chamfering past it.
                                let point = curve_pierce
                                    .get(&corners)
                                    .copied()
                                    .unwrap_or_else(|| face_centroid(corners, &mesh.nodes));
                                let id = mesh.nodes.len() as u32;
                                mesh.nodes.push(point);
                                keys.push(node_key(point, order_quantum));
                                face_steiner.insert(corners, id);
                                id
                            }
                        };
                        // A point on a locked curve lies on **every** surface that meets
                        // along it, and §7.6's split has to be told so for each of them, not
                        // for a guessed pair. Otherwise the split asks the classifier for a
                        // side at a point exactly on a surface it was not told about, gets
                        // whichever way the predicate rounds, and declines.
                        if curve_pierce.contains_key(&corners) {
                            for component in &all_components {
                                meeting_nodes.push((id, [*component, *component]));
                            }
                        }
                        boundary.extend(loop_fan(&loop_nodes, id));
                    }
                }
                // **The cache is authoritative from here (§7.3).** Its triangles replace this
                // cell's own, re-wound outward from this cell. Today that is provably a no-op -
                // zero conflicts across the nine cases, winding included - and that is the point:
                // the switch is made while it changes nothing, so that when crease chords are
                // added to the *cache* they reach both cells identically. Adding them per cell is
                // what cracked the mesh twice.
                if let Some(shared) = check_face_cache(
                    &mut face_cache,
                    &mut face_cache_conflicts,
                    loop_nodes,
                    n_parent_nodes,
                    &keys,
                    &mesh.nodes,
                    crossed.as_ref(),
                    false,
                    &boundary[face_begin..],
                ) {
                    boundary.truncate(face_begin);
                    boundary.extend(orient_face_outward(&shared, tet, &mesh.nodes));
                }
            }
            // G7-1: a cell whose two walls sandwich a thin gap is *not* one blob. If
            // the doubly-cut face rule covers all four of its faces, it splits into
            // three closed slabs and the gap survives as elements of its own; the
            // single fan below would have chamfered it away.
            let band: Option<BandCellPlan> = if options.bands {
                let loops: [&[u32]; 4] = [
                    &faces[0].1,
                    &faces[1].1,
                    &faces[2].1,
                    &faces[3].1,
                ];
                match split_band_cell(loops, n_parent_nodes, &keys, &mesh.nodes) {
                    Ok(plan) => Some(plan),
                    Err(reason) => {
                        *mesh.stats.n_band_declined.entry(reason).or_insert(0) += 1;
                        // "Declined" says the rule did not claim the cell and not which
                        // shape it was offered. The signature is (loop length, cut nodes)
                        // per face, which is exactly what a missing §8.2 row looks like.
                        if std::env::var_os("RUSTMSPT_THIN_DIAG").is_some() {
                            let signature: Vec<(usize, usize)> = loops
                                .iter()
                                .map(|face| {
                                    (
                                        face.len(),
                                        face.iter()
                                            .filter(|node| **node >= n_parent_nodes)
                                            .count(),
                                    )
                                })
                                .collect();
                            eprintln!("[S8b-DIAG] band declined {reason:?}: {signature:?}");
                        }
                        None
                    }
                }
            } else {
                None
            };
            // G7-2: which thin region this cell's gap belongs to. Taken as the lowest
            // id any of the six wall nodes is attributed to rather than, say, wall A's
            // - a region owns both walls, so the two lids agree whenever S3 attributed
            // them at all, and `min` makes the answer independent of which lid the
            // three-slab split happened to orient first.
            let band_region_id: i32 = band
                .as_ref()
                .map(|plan| {
                    plan.pairs
                        .iter()
                        .flat_map(|pair| [pair.a, pair.b])
                        .filter_map(|node| region_of_cut.get(&node).copied())
                        .filter(|id| *id >= 0)
                        .min()
                        .unwrap_or(-1)
                })
                .unwrap_or(-1);
            let parent_record = &classification.records[index];
            // Each slab is a closed boundary; the *gap* slab is additionally a band
            // cell, so it goes to the frozen §8.2 table first and falls back to a fan
            // only when the table or the §4.4 floor refuses it - which is the ladder's
            // last rung, and is what `regime = 2` means in the contract arrays.
            let mut slabs: CellSlabs = SmallVec::new();
            // Which slabs §7.6's split produced, and the nodes each component's surface is
            // known to pass through - both only for the spoke probe below, which is off
            // unless `RUSTMSPT_SPOKE_PROBE` is set.
            let mut split_slabs: std::ops::Range<usize> = 0..0;
            let mut on_surface: BTreeMap<i32, BTreeSet<u32>> = BTreeMap::new();
            if probing_spokes {
                for component in &all_components {
                    let mut here: BTreeSet<u32> = BTreeSet::new();
                    for a in 0..4 {
                        for b in (a + 1)..4 {
                            let (x, y) = (lattice.tets[index][a], lattice.tets[index][b]);
                            let edge = if x <= y { [x, y] } else { [y, x] };
                            if let Some(node) = cut_index.get(&(edge, *component)) {
                                here.insert(*node);
                            }
                            if let Some(node) = second_index.get(&(edge, *component)) {
                                here.insert(*node);
                            }
                        }
                    }
                    for node in &lattice.tets[index] {
                        if on_cut.contains_key(&(*node, *component)) {
                            here.insert(*node);
                        }
                    }
                    for (node, owners) in &meeting_nodes {
                        if owners.contains(component) {
                            here.insert(*node);
                        }
                    }
                    on_surface.insert(*component, here);
                }
            }
            match band {
                Some(plan) => {
                    mesh.stats.n_band_cells += 1;
                    for (slot, slab) in plan.slabs.into_iter().enumerate() {
                        let pairs = (slot == Slab::Band as usize).then_some(plan.pairs);
                        slabs.push((slab, pairs));
                    }
                }
                None => {
                    // §7.6, the cut *inside* an escalated cell. The centroid fan below
                    // is total but gives the patch up as a face of the mesh, which is
                    // what chamfers the material boundary by up to one cell and what
                    // leaves adjacent tets stepping two components across an undeclared
                    // face. Split the conforming boundary soup by each crossing
                    // component first, cap both halves with the surface polygon, and
                    // fan the pieces instead; every rejection path falls straight back
                    // to the single fan, and the whole split is discarded unless the
                    // pieces' volumes still sum to the parent's.
                    // §7.6 is about **junctions** - cells where surfaces meet. Two walls
                    // crossing one cell without ever meeting inside it is a *gap*, and a
                    // gap belongs to S8b's band ladder, which knows its regime and its
                    // thickness. Requiring at least one face whose chords actually cross
                    // is what tells the two apart, and the acceptance data separates
                    // cleanly on it: A-3's junction cells carry crossing chords, while
                    // A-7a's 512 parallel-plate cells carry none and splitting them cost
                    // 0.36/0.10 % -> 0.57/0.44 % of volume.
                    // Only *two* surfaces sharing a cell raise the gap-versus-junction
                    // question; a cell one surface crosses has no gap to mistake, and
                    // there the cut is simply the one §6 refused - which is the whole
                    // 4.1 % A-8 loses, 3,687 cells chamfered by the fan because nothing
                    // ever cut them.
                    // "Two surfaces" means two *components*, not two chords. One component
                    // crossing a face twice is a lattice face straddling a thin plate -
                    // the plate's own two walls - and that is a gap S8b owns rather than
                    // two bodies approaching each other, so the meeting-point test must
                    // not be asked of it.
                    // **Every escalated cell is offered to the split (P-3.2).** Until
                    // 2026-08-15 a two-surface cell was offered only when the two surfaces
                    // *met inside it* - the reasoning being that two walls crossing a cell
                    // without meeting is a *gap*, and a gap belongs to S8b's band ladder,
                    // which knows its regime and its thickness. The reasoning is sound and
                    // the gate was wrong anyway, because it fires on cells S8b **declined**:
                    // A-7a's 0.006 gap is below the sheet threshold, so the ladder passes,
                    // and its 2,176 parallel-plate cells then fell through to the fan with
                    // nothing to catch them.
                    //
                    // It survived two reviews because both measured `[V6]` and volume error,
                    // and neither can see P3 - a boundary displaced half a cell off the
                    // surface scores exactly like one lying on it. Measured with `[V13]`,
                    // removing the gate takes A-7a from **73.4 % to 88.7 %** of material-
                    // boundary area on the surface (off-surface area -58 %) and A-3 from
                    // 89.4 % to 91.5 %, for +3.6 % / +0.9 % elements. Six of the nine cases
                    // are bit-identical - no cell reaches the path. Volume error stays inside
                    // its 1 % gate, `[V1]`/`[V3]` are unchanged, `[V6]`'s adjacency count
                    // improves, min dihedral is identical to three decimals, and R-P2 holds.
                    //
                    // Nothing replaces it, and nothing may: which cells get a conforming cut
                    // is not a setting (R3). The split's own guards decline safely where it
                    // cannot work - on A-7a they hard-fail zero times - and every rejection
                    // path still falls back to the fan.
                    let split = Some(split_escalated_cell(
                        index,
                        &boundary,
                        tet,
                        &cut_index,
                        &second_index,
                        &on_cut,
                        &all_components,
                        classifier,
                        &mesh.nodes,
                        &keys,
                        options.volume_tolerance,
                        &meeting_nodes,
                        &plc_face_owners,
                    ))
                    .flatten();
                    match split {
                        Some((pieces, caps)) => {
                            mesh.stats.n_junction_splits += 1;
                            mesh.stats.n_junction_split_pieces += pieces.len();
                            for (triangle, component) in caps {
                                for node in triangle {
                                    on_surface.entry(component).or_default().insert(node);
                                }
                                pending_interfaces.push((mesh.tets.len(), triangle, component));
                            }
                            split_slabs = slabs.len()..slabs.len() + pieces.len();
                            for piece in pieces {
                                slabs.push((piece.to_vec(), None));
                            }
                        }
                        None => {
                            mesh.stats.n_steiner_fans += 1;
                            slabs.push((boundary.to_vec(), None));
                        }
                    }
                }
            }
            // The gap slab's two lids are the walls themselves: tag them, with the
            // component whose crossings they are built from.
            if let Some(pairs) = slabs.iter().find_map(|(_, pairs)| *pairs) {
                for wall in [
                    [pairs[0].a, pairs[1].a, pairs[2].a],
                    [pairs[0].b, pairs[1].b, pairs[2].b],
                ] {
                    if let Some(component) = component_of_cut.get(&wall[0]) {
                        pending_interfaces.push((mesh.tets.len(), wall, *component));
                    }
                }
            }
            for (slot, (slab, pairs)) in slabs.iter().enumerate() {
                if probing_spokes && pairs.is_none() {
                    probe_spoke_cut(
                        slab,
                        &mesh.nodes,
                        &all_components,
                        classifier,
                        &on_surface,
                        split_slabs.contains(&slot),
                        &mut spoke_probe,
                    );
                }
                if let Some(pairs) = pairs {
                    let diagonals = snk_cell_diagonals(*pairs, &keys);
                    if let Ok(meshed) =
                        mesh_band_cell(*pairs, diagonals, &mesh.nodes, options.min_dihedral_deg, 0)
                    {
                        *mesh
                            .stats
                            .n_band_templates
                            .entry(meshed.template)
                            .or_insert(0) += 1;
                        mesh.stats.band_min_dihedral_deg = mesh
                            .stats
                            .band_min_dihedral_deg
                            .min(meshed.min_dihedral_deg);
                        for piece in &meshed.tets {
                            mesh.tets.push(*piece);
                            mesh.records.push(seed_record(
                                parent_record,
                                *piece,
                                &all_components,
                                &mesh.nodes,
                                classifier,
                                &mut seed_uncertain,
                            ));
                            seeded_pieces += 1;
                            mesh.parent_of.push(index as u32);
                            mesh.regime.push(meshed.template.regime_code());
                            mesh.band_region.push(band_region_id);
                        }
                        continue;
                    }
                }
                let centroid = polygon_soup_centroid(slab, &mesh.nodes);
                let centroid_id = mesh.nodes.len() as u32;
                mesh.nodes.push(centroid);
                keys.push(node_key(centroid, quantum));
                let fanned = fan_cell(slab, centroid_id);
                if pairs.is_some() {
                    *mesh
                        .stats
                        .n_band_templates
                        .entry(BandTemplate::Steiner)
                        .or_insert(0) += 1;
                }
                for piece in &fanned.tets {
                    let oriented = orient_positively(*piece, &mesh.nodes);
                    let Some(oriented) = oriented else {
                        mesh.stats.n_degenerate_fan_pieces += 1;
                        continue;
                    };
                    mesh.tets.push(oriented);
                    mesh.records.push(seed_record(
                        parent_record,
                        oriented,
                        &all_components,
                        &mesh.nodes,
                        classifier,
                        &mut seed_uncertain,
                    ));
                    seeded_pieces += 1;
                    mesh.parent_of.push(index as u32);
                    mesh.regime
                        .push(if pairs.is_some() { 2 } else { REGIME_NORMAL });
                    mesh.band_region
                        .push(if pairs.is_some() { band_region_id } else { -1 });
                }
            }
            mesh.stats.n_uncut_cells += 1;
            continue;
        }
        if cell.tets.len() == 1 && cell.case == 0 {
            mesh.stats.n_uncut_cells += 1;
        } else {
            mesh.stats.n_cut_cells += 1;
            match cell.case {
                b'A' => mesh.stats.n_case_a += 1,
                b'B' | b'b' => mesh.stats.n_case_b += 1,
                b'C' | b'c' => mesh.stats.n_case_c += 1,
                b'D' => mesh.stats.n_case_d += 1,
                b'i' => mesh.stats.n_interior_sampled_cells += 1,
                _ => {}
            }
            if cell.welded {
                mesh.stats.n_welded_sheet_cells += 1;
            }
            mesh.stats.min_dihedral_deg = mesh.stats.min_dihedral_deg.min(cell.min_dihedral);
            mesh.stats.worst_volume_error = mesh.stats.worst_volume_error.max(cell.volume_error);
        }
        let parent_record = &classification.records[index];
        // The §6 table decides the **driving** component's side exactly and `cut_record`
        // inherits every other component's entry verbatim - including an `Outside` S6 read
        // off vertices that were all snapped *onto* that other body's surface. At an edge or
        // a corner that is the ordinary case, and the child then carries background where it
        // holds material: A-6a's 321 and A-7b's 90 remaining misattributed cells, all
        // `provenance = 1`, after the same defect was closed on the §7.6 path.
        //
        // Scoped by `on_cut`, which is the signal that the other body's surface runs through
        // this cell's vertices at all. Without it this would sample every component on every
        // cut tet - millions of classifier queries on A-4 - to change a few hundred.
        let hidden: SmallVec<[i32; 2]> = all_components
            .iter()
            .filter(|component| {
                **component != cell.component
                    && Some(**component) != cell.welded_with
                    && !parent_record
                        .entries
                        .iter()
                        .any(|(x, side)| x == *component && *side == Side::Inside)
                    && lattice.tets[index]
                        .iter()
                        .any(|node| on_cut.contains_key(&(*node, **component)))
            })
            .copied()
            .collect();
        for (tet, inside) in cell.tets.iter().zip(cell.inside.iter()) {
            let mut record = cut_record(
                parent_record,
                cell.component,
                cell.welded_with,
                *inside,
                if cell.welded { 0 } else { cell.case },
            );
            if !hidden.is_empty() {
                let centroid = tet
                    .iter()
                    .fold(Vec3::new(0.0, 0.0, 0.0), |acc, node| {
                        acc.add(mesh.nodes[*node as usize])
                    })
                    .scale(0.25);
                for component in &hidden {
                    let Some(slot) = classifier.slot_of(*component) else {
                        continue;
                    };
                    if !classifier.inside(centroid, slot, &mut seed_uncertain) {
                        continue;
                    }
                    match record.entries.iter_mut().find(|(x, _)| x == component) {
                        Some(entry) => entry.1 = Side::Inside,
                        None => record.entries.push((*component, Side::Inside)),
                    }
                    hidden_recovered += 1;
                }
                record.entries.sort_by_key(|(x, _)| *x);
            }
            mesh.records.push(record);
            mesh.tets.push(*tet);
            mesh.parent_of.push(index as u32);
            mesh.regime.push(REGIME_NORMAL);
            mesh.band_region.push(-1);
        }
        for triangle in &cell.interface {
            pending_interfaces.push((mesh.tets.len(), *triangle, cell.component));
        }
    }
    mesh.stats.n_tets = mesh.tets.len();
    if !mesh.stats.min_dihedral_deg.is_finite() {
        mesh.stats.min_dihedral_deg = 0.0;
    }

    // --- the interface index (G6-3): side elements, derived not stored ---
    mesh.interfaces = derive_interface(
        &mesh,
        &pending_interfaces,
        &keys,
        &sheets,
        &rim_nodes,
        &options.contact_patches,
        options.eps,
    );
    // Where a sheet may legitimately end: the boundary of a collapsed region (G7-2's
    // own rim) and the arranged rim curves of an open sheet (S2's, which S7 has already
    // snapped nodes onto). Both are "the sheet stops here for a reason".
    let mut terminates_on_rim = rim_boundary_nodes;
    terminates_on_rim.extend(nodes_on_rim(&mesh.nodes, &options.rim_segments, options.eps));
    mesh.rim_curve = collapsed_sheet_rim(&mesh.interfaces, &terminates_on_rim);
    mesh.constraint_kind = snapped.constraint_kind.clone();
    mesh.constraint_kind.resize(mesh.nodes.len(), 0u8);
    mesh.constraint_ref = snapped.constraint_ref.clone();
    mesh.constraint_ref.resize(mesh.nodes.len(), -1i32);
    mesh.curves = options.locked_curves.clone();
    mesh.curve_edges = curve_mesh_edges(&mesh.tets, &mesh.nodes, &mesh.curves, options.eps);
    if !mesh.curves.is_empty() {
        let carried: BTreeSet<u32> = mesh.curve_edges.iter().map(|(curve, _)| *curve).collect();
        println!(
            "[S8/G6-4] curve table: {} locked curve(s), {} carried by a chain of mesh edges, \
             {} mesh edge(s) on a curve",
            mesh.curves.len(),
            carried.len(),
            mesh.curve_edges.len()
        );
    }
    mesh.stats.n_interface_faces = mesh.interfaces.len();
    mesh.stats.n_seeded_pieces = seeded_pieces;
    mesh.stats.n_seed_uncertain = seed_uncertain;

    {
        let lat = CAP_LOOP_LATTICE.swap(0, std::sync::atomic::Ordering::Relaxed);
        let int = CAP_LOOP_INTERNED.swap(0, std::sync::atomic::Ordering::Relaxed);
        if lat + int > 0 {
            mesh.warnings.push(format!(
                "[CAP-LOOP] {} cap loop node(s): {int} on the cut's own surface set, {lat} NOT \
                 ({:.1} %) - a loop node outside that set is one the split had no on-surface reason \
                 to route through, so this is how often the cap is forced onto the mesh rather than \
                 the geometry (P-3.9)",
                lat + int,
                100.0 * lat as f64 / (lat + int) as f64
            ));
        }
    }
    if plc_meshed + plc_fanned > 0 {
        // **Did the mesh actually take the faces the faces promised?** Every triangle of an
        // augmented face should be carried by exactly as many tets as the face has owners - two
        // inside the lattice, one on the domain boundary. A triangle carried by fewer is a leak
        // whose cause is on the FACE side; one the mesh never produced at all means an owner
        // ignored the triangulation it was handed. Counting it here rather than inferring it from
        // `[V3]`'s totals is what separates the two.
        let mut carried: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for tet in &mesh.tets {
            for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_unstable();
                *carried.entry(face).or_insert(0) += 1;
            }
        }
        let (mut short, mut absent, mut over) = (0usize, 0usize, 0usize);
        for (face, tris) in &plc_face_tris {
            let expected = plc_face_owners_n.get(face).copied().unwrap_or(2);
            for triangle in tris {
                let mut key = *triangle;
                key.sort_unstable();
                match carried.get(&key).copied().unwrap_or(0) {
                    0 => absent += 1,
                    n if n < expected => short += 1,
                    n if n > expected => over += 1,
                    _ => {}
                }
            }
        }
        mesh.warnings.push(format!(
            "[PLC] of the augmented faces' triangles: {absent} appear in no tet at all, {short} in \
             fewer tets than the face has owners, {over} in more. The first two are the face side's \
             leak and the third is a face emitted twice by one cell"
        ));
        // The same question of the boundary a §7.4 cell was actually handed, which is the augmented
        // faces plus the ones taken from §5.2. A triangle short here and sound above means the
        // mismatch is on a face §7.4 left alone - between `face_split` and whatever the neighbour
        // that took §6's table or escalated actually emitted for it.
        let mut lone: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for boundary in plc_boundary.iter().flatten() {
            for triangle in boundary {
                let mut key = *triangle;
                key.sort_unstable();
                *lone.entry(key).or_insert(0) += 1;
            }
        }
        let mut unmatched = 0usize;
        let mut unmatched_augmented = 0usize;
        for (triangle, promised) in &lone {
            let got = carried.get(triangle).copied().unwrap_or(0);
            if got >= *promised {
                continue;
            }
            unmatched += 1;
            if plc_face_tris.values().any(|tris| {
                tris.iter().any(|t| {
                    let mut k = *t;
                    k.sort_unstable();
                    k == *triangle
                })
            }) {
                unmatched_augmented += 1;
            }
        }
        // **Which path emitted each leaking face.** A face carried by exactly one tet, whose three
        // nodes are not all on one domain plane, is `[V3]`'s boundary leak; charging it to the path
        // its tet's parent took says which of them is wrong, and stops the diagnosis being a
        // sequence of guesses.
        let mut face_tet: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for (at, tet) in mesh.tets.iter().enumerate() {
            for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_unstable();
                face_tet.entry(face).or_insert(at);
            }
        }
        let (mut lo, mut hi) = (mesh.nodes[0], mesh.nodes[0]);
        for p in &mesh.nodes {
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        let span = hi.sub(lo).dot(hi.sub(lo)).sqrt().max(f64::MIN_POSITIVE);
        let on_domain = |face: &[u32; 3]| -> bool {
            let p = [
                mesh.nodes[face[0] as usize],
                mesh.nodes[face[1] as usize],
                mesh.nodes[face[2] as usize],
            ];
            (0..3).any(|axis| {
                let get = |q: Vec3| match axis {
                    0 => (q.x, lo.x, hi.x),
                    1 => (q.y, lo.y, hi.y),
                    _ => (q.z, lo.z, hi.z),
                };
                p.iter().all(|q| {
                    let (v, a, b) = get(*q);
                    (v - a).abs() <= span * 1.0e-9 || (v - b).abs() <= span * 1.0e-9
                })
            })
        };
        let mut leak_by_path = [0usize; 5];
        for (face, count) in &carried {
            if *count != 1 || on_domain(face) {
                continue;
            }
            let at = face_tet.get(face).copied().unwrap_or(0);
            let parent = mesh.parent_of.get(at).copied().unwrap_or(0) as usize;
            leak_by_path[path_of.get(parent).copied().unwrap_or(4).min(4) as usize] += 1;
        }
        mesh.warnings.push(format!(
            "[PLC] leaking faces by the path that emitted them: {} §6's table, {} §7.4-meshed, \
             {} §7.4-fanned, {} escalated, {} other",
            leak_by_path[0], leak_by_path[1], leak_by_path[2], leak_by_path[3], leak_by_path[4]
        ));
        // **The same question asked of the hanging nodes, and asked as an EDGE question.**
        // A leak is a face two cells disagree about; a T-junction need not be. A cell can carry a
        // lattice edge whole while the point another cell put on that edge sits in the middle of
        // it - every face involved still has two owners, so `carried` is balanced and the leak
        // count says nothing. The test is therefore not about faces at all: an interned edge point
        // is a hanging node exactly when some tet still has both of that edge's endpoints as
        // vertices. Charging that tet to its parent's path says which arm kept the edge.
        let mut tet_edges: BTreeMap<[u32; 2], usize> = BTreeMap::new();
        for (at, tet) in mesh.tets.iter().enumerate() {
            for pair in [[0usize, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]] {
                let (x, y) = (tet[pair[0]], tet[pair[1]]);
                tet_edges.entry(if x <= y { [x, y] } else { [y, x] }).or_insert(at);
            }
        }
        let mut referenced = vec![false; mesh.nodes.len()];
        for tet in &mesh.tets {
            for node in tet {
                referenced[*node as usize] = true;
            }
        }
        let mut tjunction_by_path = [0usize; 5];
        let (mut split_edges, mut orphan_points) = (0usize, 0usize);
        for (edge, ids) in &plc_edge_points {
            let interior: Vec<u32> =
                ids.iter().copied().filter(|id| !edge.contains(id)).collect();
            if interior.is_empty() {
                continue;
            }
            split_edges += 1;
            orphan_points += interior
                .iter()
                .filter(|id| !referenced[**id as usize])
                .count();
            let Some(at) = tet_edges.get(edge).copied() else { continue };
            let parent = mesh.parent_of.get(at).copied().unwrap_or(0) as usize;
            tjunction_by_path[path_of.get(parent).copied().unwrap_or(4).min(4) as usize] +=
                interior.len();
        }
        mesh.warnings.push(format!(
            "[PLC] of {split_edges} lattice edge(s) an interned point splits, the tets that still \
             carry the whole edge are: {} §6's table, {} §7.4-meshed, {} §7.4-fanned, {} \
             escalated, {} other - and {orphan_points} of those points are in no tet at all",
            tjunction_by_path[0],
            tjunction_by_path[1],
            tjunction_by_path[2],
            tjunction_by_path[3],
            tjunction_by_path[4]
        ));
        // Faces carried by three or more tets, by the paths that carried them. A face emitted by
        // both of its owners AND by a third cell is not a tolerance question: one of the three
        // built a triangle on a face that is not its own.
        let mut over_by_path = [0usize; 5];
        let mut over_faces = 0usize;
        for (face, count) in &carried {
            if *count < 3 {
                continue;
            }
            over_faces += 1;
            for (at, tet) in mesh.tets.iter().enumerate() {
                for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                    let mut key = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                    key.sort_unstable();
                    if key == *face {
                        let parent = mesh.parent_of.get(at).copied().unwrap_or(0) as usize;
                        over_by_path
                            [path_of.get(parent).copied().unwrap_or(4).min(4) as usize] += 1;
                    }
                }
            }
        }
        mesh.warnings.push(format!(
            "[PLC] {over_faces} face(s) carried by three or more tets, whose carriers are: {} \
             §6's table, {} §7.4-meshed, {} §7.4-fanned, {} escalated, {} other",
            over_by_path[0],
            over_by_path[1],
            over_by_path[2],
            over_by_path[3],
            over_by_path[4]
        ));
        // **`[V6]`'s adjacency violations, charged the same way.** A face between two tets whose
        // records name different materials is a material boundary and must carry an interface tag.
        // Whether the two tets are in ONE cell or in two says which half is at fault: inside a cell
        // the regions are separated by a constraint face and the cap list should already name it,
        // while across a lattice face the two cells classified independently and disagreed.
        {
            let mut tagged: BTreeSet<[u32; 3]> = BTreeSet::new();
            for (_, triangle, _) in &pending_interfaces {
                let mut key = *triangle;
                key.sort_unstable();
                tagged.insert(key);
            }
            let mut sides: BTreeMap<[u32; 3], SmallVec<[usize; 2]>> = BTreeMap::new();
            for (at, tet) in mesh.tets.iter().enumerate() {
                for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                    let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                    face.sort_unstable();
                    sides.entry(face).or_default().push(at);
                }
            }
            let inside_set = |at: usize| -> Vec<i32> {
                let mut out: Vec<i32> = mesh
                    .records
                    .get(at)
                    .map(|r| {
                        r.entries
                            .iter()
                            .filter(|(_, side)| *side == Side::Inside)
                            .map(|(x, _)| *x)
                            .collect()
                    })
                    .unwrap_or_default();
                out.sort_unstable();
                out
            };
            let (mut same_cell, mut cross_cell) = (0usize, 0usize);
            let mut cross_by_path = [0usize; 5];
            let mut same_by_path = [0usize; 5];
            for (face, at) in &sides {
                if at.len() != 2 || tagged.contains(face) {
                    continue;
                }
                // **`[V6]`'s own rule, not a broader one.** A face may change the inside-set by as
                // many components as it is tagged with, and an untagged face is allowed one - a
                // single body's surface passing through. Counting every disagreement instead reads
                // 9,368 where `[V6]` reads 792, and a diagnostic that does not agree with the check
                // it is diagnosing is worse than none.
                let (a, b) = (inside_set(at[0]), inside_set(at[1]));
                let difference = a.iter().filter(|x| !b.contains(x)).count()
                    + b.iter().filter(|x| !a.contains(x)).count();
                if difference <= 1 {
                    continue;
                }
                let (a, b) = (
                    mesh.parent_of.get(at[0]).copied().unwrap_or(0) as usize,
                    mesh.parent_of.get(at[1]).copied().unwrap_or(0) as usize,
                );
                if a == b {
                    same_cell += 1;
                    same_by_path[path_of.get(a).copied().unwrap_or(4).min(4) as usize] += 1;
                } else {
                    cross_cell += 1;
                    for parent in [a, b] {
                        cross_by_path
                            [path_of.get(parent).copied().unwrap_or(4).min(4) as usize] += 1;
                    }
                }
            }
            mesh.warnings.push(format!(
                "[PLC] `[V6]` steps of two or more components across an untagged face: \
                 {same_cell} inside one cell ({} table / {} meshed / {} fanned / {} escalated / \
                 {} other) and {cross_cell} across a lattice face ({} / {} / {} / {} / {} by side)",
                same_by_path[0],
                same_by_path[1],
                same_by_path[2],
                same_by_path[3],
                same_by_path[4],
                cross_by_path[0],
                cross_by_path[1],
                cross_by_path[2],
                cross_by_path[3],
                cross_by_path[4]
            ));
        }

        // **And the same charge for the degenerate elements.** `[V1]` reads a signed volume and
        // says nothing about where it came from; normalising by the tet's own longest edge cubed
        // makes "flat" a scale-free statement, and the path histogram says which arm to look in.
        let mut flat_by_path = [0usize; 5];
        let mut inverted_by_path = [0usize; 5];
        let mut worst_flat = f64::INFINITY;
        for (at, tet) in mesh.tets.iter().enumerate() {
            let p = [
                mesh.nodes[tet[0] as usize],
                mesh.nodes[tet[1] as usize],
                mesh.nodes[tet[2] as usize],
                mesh.nodes[tet[3] as usize],
            ];
            let volume = p[1].sub(p[0]).cross(p[2].sub(p[0])).dot(p[3].sub(p[0])) / 6.0;
            let mut longest = 0.0f64;
            for a in 0..4 {
                for b in a + 1..4 {
                    let d = p[b].sub(p[a]);
                    longest = longest.max(d.dot(d).sqrt());
                }
            }
            let shape = volume / longest.powi(3).max(f64::MIN_POSITIVE);
            let parent = mesh.parent_of.get(at).copied().unwrap_or(0) as usize;
            let path = path_of.get(parent).copied().unwrap_or(4).min(4) as usize;
            if shape <= 0.0 {
                inverted_by_path[path] += 1;
            } else if shape < 1.0e-9 {
                flat_by_path[path] += 1;
                worst_flat = worst_flat.min(shape);
            }
        }
        mesh.warnings.push(format!(
            "[PLC] tets with volume/longest-edge^3 at or below zero: {} §6's table, {} \
             §7.4-meshed, {} §7.4-fanned, {} escalated, {} other; and merely flat (< 1e-9): {} / \
             {} / {} / {} / {}, worst {worst_flat:.3e}",
            inverted_by_path[0],
            inverted_by_path[1],
            inverted_by_path[2],
            inverted_by_path[3],
            inverted_by_path[4],
            flat_by_path[0],
            flat_by_path[1],
            flat_by_path[2],
            flat_by_path[3],
            flat_by_path[4]
        ));
        mesh.warnings.push(format!(
            "[PLC] of the boundaries §7.4 cells were handed: {unmatched} triangle(s) are carried by \
             fewer tets than the cells that promised them, {unmatched_augmented} of those on an \
             augmented face - so the rest are faces §7.4 left to §5.2"
        ));
    }
    if plc_meshed + plc_fanned > 0 {
        mesh.warnings.push(format!(
            "[PLC] §7.1 rev 1.5: {plc_meshed} cell(s) meshed by §7.2/§7.4 against their own \
             surface fragment, {plc_fanned} fanned over the same augmented faces where it \
             declined. Both keep the faces the trace changed, which is what lets the two meet"
        ));
    }
    if face_trace_faces > 0 {
        mesh.warnings.push(format!(
            "[FACE-TRACE] {face_trace_faces} lattice face(s) carry a surface trace, {face_trace_nodes} \
             point(s) on them - a pure function of the face, so both cells derive the same set. Held \
             as points, not interned: a node nothing references is a hanging node. {face_trace_reaching} \
             reach the face's boundary (§5.2 already has the chord as an edge); {face_trace_interior} \
             stay strictly inside it - the sub-cell body, and the population §7.2 exists for"
        ));
        mesh.warnings.push(format!(
            "[FACE-TRACE] {subcell_cells} cell(s) carry a trace with no crossed edge at all, \
             {subcell_clean} of them on interior-only faces throughout - the population the \
             fragment mesher claims - and {subcell_ready} of those are enclosed by faces whose \
             other owner is also clean, which is the set the wiring can claim without leaving a \
             hanging node (P-3.3's condition, P-3.12's failure)"
        ));
        mesh.warnings.push(format!(
            "[FACE-TRACE] every interior face by owner: {face_all_clean} all-clean, \
             {face_all_escalated} all-escalated, {face_mixed} mixed clean/escalated, and \
             {face_has_table} with an owner that takes §5.2's table. The table reads the face's \
             EDGE crossings and an interior loop crosses no edge, so that owner keeps the plain \
             triangle whatever its neighbour does - a face-side fix cannot reach the fourth bucket"
        ));
        mesh.warnings.push(format!(
            "[FACE-TRACE] the cells owning an interior face: {cells_interior_table} take §5.2's \
             table, {cells_interior_escalated} escalate, {cells_interior_uncut} have nothing cut. \
             The first group is the one no mechanism reaches today: a component crosses their \
             edges and is cut, while ANOTHER component passes through a face without crossing any \
             edge, so `crossing_components` never sees it and no junction is declared. Escalating \
             on 'a component enters this cell and crosses none of its edges' is the rule that \
             would reach them, and this is its size"
        ));
        mesh.warnings.push(format!(
            "[FACE-TRACE] of {escalated_with_traces} escalated cell(s) carrying a trace, \
             {escalated_fully_surrounded} have every traced face owned only by escalated cells. \
             That is the population §7.4's face triangulation could be delivered to if the path \
             stays restricted to escalation; the rest need it offered to every cell the surface \
             passes through - which is {cells_with_a_trace} cell(s) of {} in this lattice"
             , lattice.tets.len()
        ));
        // Measured behind a prototype gate and removed once it had answered (§6.24): escalating
        // these cells is CONFORMING - `[V1]`/`[V3]`/`[V9]` pass on both cases - and still loses,
        // because the centroid fan cannot mesh the population it delivers. a8 92.011 -> 90.829 %
        // with `[JCT-FALLBACK]` 397 -> 4,141; a6a 99.539 -> 99.488 % with 158 -> 210. The delivery
        // mechanism is sound and the mesher behind it is not, which is why §7.2's mesher has to
        // replace the fan for these cells rather than be added beside it.
    }
    if hidden_recovered > 0 {
        println!(
            "[S8/G6-3] {hidden_recovered} cut child(ren) recovered a body whose surface runs \
             through their cell's vertices; §6's table had inherited S6's `Outside` for it"
        );
    }
    if mesh.stats.n_interior_sampled_cells > 0 {
        println!(
            "[S8/G6-2] {} cell(s) had every vertex on the patch, so §6's table could not read a \
             side from them; an interior sample gave them to the body rather than to background",
            mesh.stats.n_interior_sampled_cells
        );
    }
    if std::env::var("RUSTMSPT_CUT_DIAG").is_ok() {
        let centroid_ids: BTreeSet<u32> = face_steiner.values().copied().collect();
        diagnose_conformity(&mesh, n_parent_nodes, n_cut_and_parent_nodes, &centroid_ids);
    }
    if probing_spokes {
        let p = spoke_probe;
        let cuttable = p.base_straddle + p.apex_only;
        let share = |n: usize| -> f64 {
            if cuttable == 0 {
                0.0
            } else {
                100.0 * n as f64 / cuttable as f64
            }
        };
        println!(
            "[SPOKE-PROBE] pieces={} straddling={} (from_split={}) fan_tets={} straddling_tets={}",
            p.pieces, p.straddling, p.from_split, p.fan_tets, p.straddling_tets
        );
        println!(
            "[SPOKE-PROBE] base_straddle={} ({:.1} %)  apex_only={} ({:.1} %)  \
             - apex_only is §6 Case C, cuttable on spokes alone; base_straddle is not",
            p.base_straddle,
            share(p.base_straddle),
            p.apex_only,
            share(p.apex_only)
        );
    }
    if face_cache.len() > 0 {
        mesh.warnings.push(format!(
            "[FACE-CACHE] {} shared face(s) triangulated, {} reused by the second cell, \
             {} conflict(s) - a conflict is two cells disagreeing on a face they share (J1)",
            face_cache.len(),
            face_cache.hits,
            face_cache_conflicts
        ));
    }
    if !crease_faces.is_empty() {
        mesh.warnings.push(format!(
            "[CREASE-FACE] {} face(s) a locked curve pierces, of which {} are expressible \
             (§5.2 draws one straight chord across the kink on those - the crease damage \
             P-3.3 has to remove) and {} of those have every owning cell escalated, so the \
             junction path alone can re-triangulate them without stranding a neighbour - \
             {} were fanned from the pierce point, so the kink is an edge of the mesh there",
            crease_faces.len(),
            crease_faces_expressible.len(),
            crease_faces_both_escalated.len(),
            crease_faces_fanned.len()
        ));
    }
    for (reason, count) in &mesh.stats.n_escalated {
        let (tag, text) = match reason {
            Escalation::Junction => (
                "[JCT-CELL]",
                "crossed by 2+ active patches or an intersection curve; the junction mesher (G6-4) owns them",
            ),
            Escalation::MultiCrossing => (
                "[CUT-3EDGE]",
                "have an edge one component crosses more than once (invariant K1)",
            ),
            Escalation::Inconsistent => (
                "[CUT-CASE]",
                "have a side/crossing configuration that is not a legal §6 row",
            ),
            Escalation::DryRun => (
                "[CUT-GUARD]",
                "failed the guarded dry-run and were left uncut",
            ),
            Escalation::Quality => (
                "[CUT-PRISM]",
                "produced a piece below the §4.4 dihedral floor after the ladder",
            ),
        };
        mesh.warnings
            .push(format!("{tag} {count} cell(s) {text}"));
    }
    mesh
}

// AI-FUNC-SUMMARY:
// Purpose: The ownership record of one child, inherited from its parent under the PLAN 1 write policy.
// Inputs: the parent's record, the component being cut, whether this child is inside it, and the case.
// Returns: OwnershipRecord.
// Side effects: None.
// Notes: The parent's *definite* entries carry over unchanged - a cell inside component 1 that is
//   being cut by component 2 stays inside component 1 on both sides of the new cut. Only the
//   `Ambiguous` entry for the cut component is settled, and it is settled by the table rather than
//   by a vote: §6 says which side each piece is on, which is why `assign_cut_sides`' corner-vote and
//   union-find machinery is not needed on this path (it is, in the junction path - §7.5).
fn cut_record(
    parent: &OwnershipRecord,
    component: i32,
    welded_with: Option<i32>,
    inside: bool,
    case: u8,
) -> OwnershipRecord {
    let mut entries: SmallVec<[(i32, Side); 2]> = SmallVec::new();
    for (x, side) in &parent.entries {
        if *x == component && case != 0 {
            if inside {
                entries.push((*x, Side::Inside));
            }
            continue;
        }
        // G7-2: the far side of a welded pair. The gap between the two solids
        // collapsed, so there is no void left for a child to be outside *both* of
        // them: whichever side of the sheet a child is on, it belongs to exactly one,
        // and the table's answer for the first component determines the second.
        if Some(*x) == welded_with && case != 0 {
            if !inside {
                entries.push((*x, Side::Inside));
            }
            continue;
        }
        entries.push((*x, *side));
    }
    OwnershipRecord {
        entries,
        provenance: if case == 0 {
            parent.provenance
        } else {
            Provenance::Cut
        },
    }
}

// AI-FUNC-SUMMARY:
// Purpose: `SPEC_meshgen_geometry.md` §7.5 - settle one escalated piece's ownership by classifying
//   an interior sample, rather than inheriting the parent's unresolved record.
// Inputs: the parent's record, the piece's four nodes, the node positions, and the shared classifier.
// Returns: OwnershipRecord with no `Ambiguous` entry left.
// Side effects: increments `uncertain` per exact-predicate escalation.
// Notes: The parent's *definite* entries carry over - a piece of a cell wholly inside component 1
//   is inside it too. Only the `Ambiguous` entries are settled, and settled by a 5-ray sample at the
//   piece's centroid, because an escalated cell's pieces do not follow the surface and so no table
//   says which side they are on.
//
//   **This is the fix for material being lost outright.** Until 2026-08-07 every escalated piece
//   took `parent_record.clone()`. A cell straddling a surface is `Ambiguous` for it, `resolve()`
//   drops `Ambiguous` entries (its own doc says "S10 must refuse a record that still carries one"),
//   so the piece resolved to `{0}` and its material went to *background*. Measured on A-3, that put
//   521 faces between region key `{1,2}` and `{0}` - an interface that cannot exist, since the lens
//   of two intersecting solids is interior to both.
//
//   A centroid sample is a majority verdict, not an exact one: the piece's boundary is the cell's
//   conforming boundary, which does not follow the patch, so the material boundary still chamfers by
//   at most one cell. Chamfering is what §7.6 costs; dropping to background was not.
fn seed_record(
    parent: &OwnershipRecord,
    piece: [u32; 4],
    all_components: &[i32],
    nodes: &[Vec3],
    classifier: &PointClassifier,
    uncertain: &mut usize,
) -> OwnershipRecord {
    let mut entries: SmallVec<[(i32, Side); 2]> = SmallVec::new();
    let mut centroid: Option<Vec3> = None;
    for (x, side) in &parent.entries {
        match side {
            Side::Inside => entries.push((*x, Side::Inside)),
            Side::Outside => {}
            Side::Ambiguous => {
                let Some(slot) = classifier.slot_of(*x) else {
                    // Not a classified solid: keep the parent's word rather than
                    // inventing one.
                    entries.push((*x, *side));
                    continue;
                };
                let point = *centroid.get_or_insert_with(|| {
                    let sum = piece
                        .iter()
                        .fold(Vec3::new(0.0, 0.0, 0.0), |acc, node| {
                            acc.add(nodes[*node as usize])
                        });
                    sum.scale(0.25)
                });
                // A single interior sample. Voting over four - the midpoints of the centroid
                // and each corner - was tried on the theory that a sliver piece lying against
                // a surface sits in the predicate's ambiguous band, and it is **exactly
                // neutral**: `[JCT-SEED]` reports `0 exact-predicate escalation(s)` on every
                // acceptance case, so the uncertain branch never runs. The seeding is not
                // where `[V6]`'s residue comes from.
                if classifier.inside(point, slot, uncertain) {
                    entries.push((*x, Side::Inside));
                }
            }
        }
    }
    // **Every component, not only the ones S6 left `Ambiguous`.** The loop above settles the
    // entries the record *gives* it and drops `Outside` outright, so a body S6 called
    // definitely-outside - or never mentioned - can never be recovered here however far
    // inside it the piece actually lies. At a body's **edge or corner** that is the common
    // case, not the exception: S6's side is a per-vertex test, and where every vertex of a
    // cell has been snapped onto the surface no vertex is inside, so the record says
    // `Outside` while the cell's interior is material. `[V5]`'s misattribution metric
    // measures exactly the residue - A-4's 2,184 cells were **1,908 on an edge, 276 on a
    // corner and none on a flat face**, every one a §7.6 piece.
    //
    // The sample is the same test `[V5]` uses to call a cell misattributed, so agreeing with
    // it is not curve-fitting to the check: the check and the mesher are now asking the one
    // question, and a piece whose centroid is inside a body carries that body.
    for component in all_components {
        if entries.iter().any(|(x, _)| x == component) {
            continue;
        }
        let Some(slot) = classifier.slot_of(*component) else {
            continue;
        };
        let point = *centroid.get_or_insert_with(|| {
            let sum = piece
                .iter()
                .fold(Vec3::new(0.0, 0.0, 0.0), |acc, node| {
                    acc.add(nodes[*node as usize])
                });
            sum.scale(0.25)
        });
        if classifier.inside(point, slot, uncertain) {
            entries.push((*component, Side::Inside));
        }
    }
    entries.sort_by_key(|(x, _)| *x);
    OwnershipRecord {
        entries,
        provenance: Provenance::Junction,
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Cut one cell, or decide it cannot take the single-patch path.
// Returns: CellResult - the pieces, or the parent unchanged plus an escalation reason.
// Side effects: None.
// Notes: The triage is §7.1's: a cell is a junction cell if two or more active patches cross it.
//   Otherwise the one component's `Ambiguous` entry names the patch, S6's `vertex_inside` gives the
//   sides, S7's crossings give the cut nodes, and §6 does the rest - behind the guarded dry-run,
//   which can still send the cell to the escalation list.
#[allow(clippy::too_many_arguments)]
fn cut_one_cell(
    index: usize,
    tet: [u32; 4],
    classification: &Classification,
    sheets: &[i32],
    all_components: &[i32],
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    second_index: &BTreeMap<([u32; 2], i32), u32>,
    multi_crossing: &BTreeMap<([u32; 2], i32), usize>,
    on_cut: &BTreeMap<(u32, i32), ()>,
    keys: &[NodeKey],
    nodes: &[Vec3],
    inside_of: &dyn Fn(u32, usize) -> bool,
    sample_inside: &dyn Fn(Vec3, i32) -> Option<bool>,
    collapsed_edges: &BTreeSet<[u32; 2]>,
    options: &CutOptions,
) -> CellResult {
    let classifier_slot = |component: i32| -> Option<usize> {
        classification
            .solid_components
            .iter()
            .position(|x| *x == component)
    };
    let uncut = |escalation: Option<Escalation>| CellResult {
        tets: SmallVec::from_slice(&[tet]),
        inside: SmallVec::from_slice(&[false]),
        interface: SmallVec::new(),
        component: 0,
        case: 0,
        welded: false,
        welded_with: None,
        escalation,
        min_dihedral: f64::INFINITY,
        volume_error: 0.0,
    };
    // Does any edge of this cell carry a cut node at all? A cell whose *record* has
    // no ambiguous entry is normally left alone - but if one of its edges is
    // crossed, its neighbour will split the face they share, and leaving this one
    // whole puts a hanging node on it. S6's sides and S7's crossings are derived
    // independently (parity per vertex vs. exact per edge), so they can disagree on
    // a cell without either being wrong, and that disagreement is precisely this
    // case. It escalates to the conforming fan rather than being ignored.
    let edge_of = |a: usize, b: usize| -> [u32; 2] {
        let (x, y) = (tet[a], tet[b]);
        if x <= y {
            [x, y]
        } else {
            [y, x]
        }
    };
    // **A cell any of whose edges carries a second cut node may never take §6's table.**
    // The table emits a face from `state.cut[edge]`, which holds one node per edge, so a
    // cell that took it would leave the second node a vertex of its neighbour's face and
    // not of its own. The test is over *every* component, not the one driving the cut: the
    // face is shared with cells this component does not even reach, and conformity is a
    // property of the face, never of the cell's own view of it. Escalating costs nothing,
    // because §7.6's split handles exactly this shape. It fires 576 times on A-4 and
    // accounts for **every** escalated cell there.
    for a in 0..4 {
        for b in (a + 1)..4 {
            let edge = {
                let (x, y) = (tet[a], tet[b]);
                if x <= y {
                    [x, y]
                } else {
                    [y, x]
                }
            };
            if all_components
                .iter()
                .any(|component| second_index.contains_key(&(edge, *component)))
            {
                return uncut(Some(Escalation::MultiCrossing));
            }
        }
    }
    let mut any_cut_edge = false;
    let mut crossing_sheets: SmallVec<[i32; 2]> = SmallVec::new();
    let mut crossing_components: SmallVec<[i32; 4]> = SmallVec::new();
    for a in 0..4 {
        for b in (a + 1)..4 {
            let edge = edge_of(a, b);
            for component in all_components {
                if cut_index.contains_key(&(edge, *component)) {
                    any_cut_edge = true;
                    crossing_components.push(*component);
                    if sheets.contains(component) {
                        crossing_sheets.push(*component);
                    }
                }
            }
        }
    }
    crossing_sheets.sort_unstable();
    crossing_sheets.dedup();
    crossing_components.sort_unstable();
    crossing_components.dedup();

    // Two components leaving cut nodes on this cell's edges makes it a junction, even
    // when S6 records only one of them as ambiguous here. The two facts are derived
    // independently and the *cut nodes* are what the face tables read: a cell that
    // took §6's path on its single ambiguous component would split a shared face
    // using that component's cut nodes alone, while the neighbour - which does see
    // both - splits the same face differently, and the mesh gains a pair of
    // single-sided faces at exactly that spot. Triaging on the crossings rather than
    // on the record keeps the face a pure function of the face, which is the whole
    // conformity argument. This is the same lesson as G6-4's "escalate on any cut
    // edge", one level up: the record is not the authority on what the cut sees.
    // G7-2's one exception, and it is the reason the collapse is worth doing at all.
    // When *every* crossed edge of this cell is a collapsed edge, the two components
    // are not two surfaces passing through the cell - they are the two sides of one
    // welded sheet, and they cross at literally the same node ids. The conformity
    // argument above is about neighbours disagreeing on a shared face; here there is
    // nothing to disagree about, because both components induce the same face split.
    // Without this, every cell in a collapsed region escalates to the fan and the
    // sheet is chamfered away - which is the whole defect the collapse exists to fix.
    let welded_pair = crossing_components.len() == 2
        && (0..4).all(|a| {
            ((a + 1)..4).all(|b| {
                let edge = edge_of(a, b);
                collapsed_edges.contains(&edge)
                    || !all_components
                        .iter()
                        .any(|component| cut_index.contains_key(&(edge, *component)))
            })
        });
    if crossing_components.len() > 1 && !welded_pair {
        return uncut(Some(Escalation::Junction));
    }


    // P-4.4: trace one cell end to end. `RUSTMSPT_TRACE_CELL=<lattice index>` prints what the
    // classification handed the cut and what the cut is about to decide, which is the only way to
    // tell a §6 table row from a welded-sheet path from a recovery when all of them can produce
    // "one tet, inside".
    if std::env::var("RUSTMSPT_TRACE_CELL").ok().and_then(|v| v.parse::<usize>().ok())
        == Some(index)
    {
        println!(
            "[TRACE {index}] nodes {:?} record {:?} crossing_components {:?} any_cut_edge {}",
            tet,
            classification.records[index].entries,
            crossing_components,
            any_cut_edge
        );
        for node in &tet {
            let on: SmallVec<[i32; 2]> = all_components
                .iter()
                .filter(|x| on_cut.contains_key(&(*node, **x)))
                .copied()
                .collect();
            println!("[TRACE {index}]   node {node} at {:?} on_cut {on:?}", nodes[*node as usize]);
        }
    }
    let record = &classification.records[index];
    let ambiguous: SmallVec<[i32; 2]> = record
        .entries
        .iter()
        .filter(|(_, side)| *side == Side::Ambiguous)
        .map(|(x, _)| *x)
        .collect();
    match (ambiguous.len(), crossing_sheets.len()) {
        // A welded sheet on its own: cut it, but change no ownership.
        (0, 1) => {
            return cut_welded_sheet(
                tet,
                crossing_sheets[0],
                cut_index,
                multi_crossing,
                on_cut,
                keys,
                nodes,
                options,
            );
        }
        (0, 0) => {
            if any_cut_edge && std::env::var_os("RUSTMSPT_S67_DIAG").is_some() {
                println!("[S67-SITE] no-ambiguous-component");
            }
            return uncut(any_cut_edge.then_some(Escalation::Inconsistent));
        }
        // **P-4.4, measured and settled.** One ambiguous component and no crossed edge means
        // the body passes through the cell's *interior* without touching an edge — a strut thinner
        // than the cell. §6's table has nothing to cut and claims the whole cell, which is what
        // makes A-8's mesh fatter than its input. The opposite extreme was tried behind a
        // bisection handle — claim nothing — and it is worth **0.014 points** on A-8 (82.411 ->
        // 82.425) and exactly nothing on A-3: the material boundary is a raw lattice face either
        // way, it merely moves to the cell's other faces. So the label is not the damage and no
        // choice here can be. Only a cut that puts the boundary ON the surface helps, and for a
        // body that touches no edge that means SPEC §7.2's fragment-driven mesher. The handle was
        // removed once it had answered.
        (1, 0) => {}
        // Both sides of a welded sheet: two ambiguous solids, one surface. Settled
        // below, by the same table, with the second component taking the complement.
        (2, 0) if welded_pair => {}
        // A solid patch and a sheet in one cell, or two sheets: a junction (§7.1).
        _ => return uncut(Some(Escalation::Junction)),
    }
    // The cut is driven by the lower component; on a welded pair the other one is
    // settled from it, since a child outside the sheet's first side is inside its
    // second - there is no void left between them to be outside of both.
    let mut ambiguous = ambiguous;
    ambiguous.sort_unstable();
    let component = ambiguous[0];
    let welded_with = (welded_pair && ambiguous.len() == 2).then(|| ambiguous[1]);
    // The component S6 called ambiguous here must be the one S7 actually crossed.
    // When it is not, every guard below passes vacuously - the ambiguous component
    // has no crossings, so no edge straddles and none carries a cut - and the cell is
    // emitted whole while its edges carry *another* component's cut nodes. Its
    // neighbour splits the face they share, and the two single-sided faces that
    // leaves are indistinguishable from a hole. The guard below tests the cut against
    // the sides; this one tests that they are even talking about the same surface.
    if crossing_components
        .first()
        .is_some_and(|crossing| *crossing != component)
    {
        if std::env::var_os("RUSTMSPT_S67_DIAG").is_some() { println!("[S67-SITE] crossing-not-ambiguous"); }
        return uncut(Some(Escalation::Inconsistent));
    }
    let Some(slot) = classification
        .solid_components
        .iter()
        .position(|x| *x == component)
    else {
        if std::env::var_os("RUSTMSPT_S67_DIAG").is_some() { println!("[S67-SITE] component-not-solid"); }
        return uncut(Some(Escalation::Inconsistent));
    };

    // Sides, and the cut node on every inside-outside edge.
    let sides: [NodeSide; 4] = std::array::from_fn(|slot_index| {
        let node = tet[slot_index];
        // On a welded pair the face's cutting set is *both* components, which is what
        // `face_states` scopes `on_cut` to; scoping only to `component` here would make
        // `split_2` fire on this cell and not on the neighbour that shares the face.
        let on_patch = on_cut.contains_key(&(node, component))
            || welded_with.is_some_and(|other| on_cut.contains_key(&(node, other)));
        if on_patch {
            NodeSide::OnCut
        } else if inside_of(node, slot) {
            NodeSide::Inside
        } else {
            NodeSide::Outside
        }
    });

    for a in 0..4 {
        for b in (a + 1)..4 {
            if multi_crossing.contains_key(&(edge_of(a, b), component)) {
                return uncut(Some(Escalation::MultiCrossing));
            }
        }
    }
    // Every inside-outside edge must carry exactly one crossing, and no other edge
    // may carry one: S6's sides and S7's crossings have to agree about this cell or
    // the case table is being fed a fiction.
    //
    // **Do not "repair" this by deriving the sides from the crossings.** It looks sound -
    // S7's intersections are exact where S6's parity is a per-vertex ray, and a union-find
    // over the uncut edges recovers the partition, exactly as `cut_welded_sheet` does for
    // a surface with no inside. Tried twice on 2026-08-06, once applied only to the
    // disagreeing cells and once applied unconditionally to every cell, on the theory that
    // a conditional rule was what broke neighbour agreement. **Both produce the identical
    // damage**: on the strut lattice, component volume error improves 32.3% -> 24.8% and
    // the mesh gains **5,698 hanging nodes and 7,454 boundary leaks**, to the element.
    //
    // That the two variants damage the mesh identically is the useful finding: the cause
    // is not neighbours using different rules, it is that something downstream of here
    // depends on S6's labels specifically, and it was not found. The disagreement has to
    // be fixed where it is created - in S6's classification or S7's crossing filter - and
    // the first step is to establish which of the two is wrong, by taking a sample of the
    // disagreeing cells and testing each edge against an independent inside/outside test.
    for a in 0..4 {
        for b in (a + 1)..4 {
            let straddles = matches!(
                (sides[a], sides[b]),
                (NodeSide::Inside, NodeSide::Outside) | (NodeSide::Outside, NodeSide::Inside)
            );
            let present = cut_index.contains_key(&(edge_of(a, b), component));
            if straddles != present {
                // Which stage is wrong? Report the direction, because the two have
                // different culprits: an edge S6 calls straddling with no crossing means
                // S7 missed one (or S6 invented the sign change), while a crossing on an
                // edge S6 calls one-sided means S7 emitted one where the surface does not
                // separate the endpoints. Counting them is the diagnostic that has to
                // come before any further repair.
                if std::env::var_os("RUSTMSPT_S67_DIAG").is_some() {
                    let a_in = sides[a] == NodeSide::Inside;
                    let b_in = sides[b] == NodeSide::Inside;
                    let (pa, pb) = (nodes[tet[a] as usize], nodes[tet[b] as usize]);
                    println!(
                        "[S67] {} comp={component} A={:.9},{:.9},{:.9} B={:.9},{:.9},{:.9} sides=({:?},{:?}) inside=({a_in},{b_in})",
                        if straddles { "straddles-no-crossing" } else { "crossing-no-straddle" },
                        pa.x, pa.y, pa.z, pb.x, pb.y, pb.z,
                        sides[a], sides[b]
                    );
                }
                if std::env::var_os("RUSTMSPT_S67_DIAG").is_some() { println!("[S67-SITE] straddle-vs-crossing"); }
                return uncut(Some(Escalation::Inconsistent));
            }
        }
    }

    if let Ok(list) = std::env::var("RUSTMSPT_CUT_CELL") {
        if list.split(',').any(|id| id.trim().parse::<usize>() == Ok(index)) {
            println!(
                "[CUT-CELL] {index}: tet {tet:?} component {component} sides {sides:?} ambiguous {ambiguous:?} crossings {crossing_components:?} record {:?}",
                classification.records[index].entries
            );
            for other in all_components {
                let per_node: Vec<&str> = tet
                    .iter()
                    .map(|node| {
                        if on_cut.contains_key(&(*node, *other)) {
                            "on"
                        } else if classifier_slot(*other)
                            .is_some_and(|slot| inside_of(*node, slot))
                        {
                            "in"
                        } else {
                            "out"
                        }
                    })
                    .collect();
                println!("[CUT-CELL] {index}: component {other} per-node {per_node:?}");
            }
            for a in 0..4 {
                for b in (a + 1)..4 {
                    if let Some(node) = cut_index.get(&(edge_of(a, b), component)) {
                        println!(
                            "[CUT-CELL] {index}: edge ({}, {}) -> cut node {node}",
                            tet[a], tet[b]
                        );
                    }
                }
            }
            for node in tet {
                let p = nodes[node as usize];
                println!(
                    "[CUT-CELL] {index}: node {node} = ({}, {}, {}) key {:?}",
                    p.x, p.y, p.z, keys[node as usize]
                );
            }
        }
    }
    let cut_node = |a: usize, b: usize| -> Option<u32> {
        cut_index.get(&(edge_of(a, b), component)).copied()
    };
    let Some(cell) = cut_tet(tet, sides, &cut_node, keys) else {
        // Not a legal row: either uncut for this patch (no I or no O), which is
        // normal, or a configuration §6 does not cover, which is not.
        let has_inside = sides.contains(&NodeSide::Inside);
        let has_outside = sides.contains(&NodeSide::Outside);
        // A cell with `Inside` nodes and no `Outside` node - every other vertex sitting
        // *on* the patch - is not "not cut by this patch", it is **entirely inside it**.
        // §6 has no row for it because there is nothing to cut, and the fall-through
        // emitted it through `uncut`, which hard-codes `inside: false` and leaves the
        // parent's `Ambiguous` entry unsettled, so the cell resolved to background and its
        // material was simply gone. That is invisible to every topological check: the mesh
        // stays watertight and conforming, it just contains less of the body than it
        // should.
        //
        // It matters because a planar face lying on a lattice node plane creates a whole
        // plane of on-cut nodes at once, so this configuration goes from rare to
        // systematic - measured at 12.9 % of a box's volume, against 0.03 % for the same
        // box moved a quarter of an element (PLAN §0.2).
        // The mirror of the case above, and the one S7's snapping makes systematic: **no**
        // vertex is `Inside` because every vertex near the feature was pulled *onto* it, so
        // the table sees only `OnCut` and `Outside` and calls the cell background - while
        // its interior lies inside the body. Read off A-7b's lower-plate corner
        // (0.2017, 0.2017, 0.4217): all fourteen 1-ring neighbours carry a snap constraint
        // and not one of them is strictly inside the plate, so the corner's own material has
        // no vertex to be keyed on and is simply dropped. The vertices cannot answer this,
        // so ask the interior - the same interior sample §7.5 uses for an escalated piece,
        // and the same answer `has_inside && !has_outside` reaches combinatorially when the
        // vertices *can* answer.
        if !has_inside && !any_cut_edge && sides.contains(&NodeSide::OnCut) {
            let centroid = tet
                .iter()
                .fold(Vec3::new(0.0, 0.0, 0.0), |acc, node| {
                    acc.add(nodes[*node as usize])
                })
                .scale(0.25);
            if sample_inside(centroid, component) == Some(true) {
                return CellResult {
                    tets: SmallVec::from_slice(&[tet]),
                    inside: SmallVec::from_slice(&[true]),
                    interface: SmallVec::new(),
                    component,
                    case: b'i',
                    welded: false,
                    welded_with: None,
                    escalation: None,
                            min_dihedral: f64::INFINITY,
                    volume_error: 0.0,
                };
            }
        }
        if has_inside && !has_outside {
            return CellResult {
                tets: SmallVec::from_slice(&[tet]),
                inside: SmallVec::from_slice(&[true]),
                interface: SmallVec::new(),
                component,
                case: b'I',
                welded: false,
                welded_with: None,
                escalation: None,
                    min_dihedral: f64::INFINITY,
                volume_error: 0.0,
            };
        }
        return uncut(if (has_inside && has_outside) || any_cut_edge {
            if std::env::var_os("RUSTMSPT_S67_DIAG").is_some() { println!("[S67-SITE] cut-tet-no-row"); }
            Some(Escalation::Inconsistent)
        } else {
            None
        });
    };

    // **§6's answer has to agree with the interior, and where it does not the cell is a
    // junction the triage missed.** The table fits *one* cut to the component's crossings.
    // Where a cell straddles a convex **edge** of that body, S7's crossings for it come from
    // two different planes, the single fitted cut chamfers the corner, and the child on its
    // outside is still inside the true solid - labelled background while holding material.
    // That is A-6a's remaining 321 cells (229 against an edge, 35 against a corner, up to
    // 67 % of an element deep) and A-7b's 90, and it is what makes A-6a's limb render with a
    // visible sawtooth.
    //
    // It cannot be repaired by relabelling the child: the table also emitted the interface
    // face *between* the two children, and `derive_interface` records its `(inside, outside)`
    // side elements, so a child that changes side turns that face into an interface with
    // material on both sides. Declining the table instead sends the cell to §7.6, which cuts
    // it by each surface in turn and lets §7.5's sample settle every piece - the path that
    // already handles this shape correctly.
    //
    // **Asked of every cut cell**, because the scope this started with - only cells carrying
    // an `on_cut` vertex - was a cost bound and not a principle, and the cost turned out not
    // to be there. Measured: one classifier query per child of every cut cell costs A-4
    // 9.6 s -> 8.9 s (the declined cells no longer pay for a dry-run) and A-6a 1.67 -> 1.44,
    // for 0.01-2.1 % more tets. What the scope was hiding is the rest of the defect: a cell
    // straddling a body's edge whose vertices S7 did *not* snap onto that surface was never
    // examined. Unscoped, `[V5]`'s misattribution goes to **zero on eight of nine cases**
    // (A-1 83, A-4 655, A-6a 190, A-7b 140, A-8 731 -> 0) and A-3 442 -> 14, with volume
    // A-8 1.925 % -> 1.620 %, A-6a component 2 0.622 % -> 0.136 %, A-1 0.911 % -> 0.773 %.
    {
        let disagrees = cell.tets.iter().zip(cell.inside.iter()).any(|(child, inside)| {
            let centroid = child
                .iter()
                .fold(Vec3::new(0.0, 0.0, 0.0), |acc, node| {
                    acc.add(nodes[*node as usize])
                })
                .scale(0.25);
            sample_inside(centroid, component) == Some(!*inside)
        });
        if disagrees {
            if std::env::var_os("RUSTMSPT_S67_DIAG").is_some() {
                println!("[S67-SITE] table-vs-interior");
            }
            return uncut(Some(Escalation::Junction));
        }
    }
    match guarded_dry_run(tet, &cell.tets, nodes, options) {
        Ok((oriented, min_dihedral, volume_error)) => CellResult {
            tets: oriented,
            inside: cell.inside.iter().copied().collect(),
            interface: cell.interface,
            component,
            case: cell.case,
            welded: false,
            welded_with,
            escalation: None,
            min_dihedral,
            volume_error,
        },
        Err(reason) => uncut(Some(reason)),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Cut one cell by one welded sheet (G6-5).
// Inputs: the parent tet, the sheet's component, the cut map, the K1 map, the on-cut set, the keys,
//   the coordinates, and the tolerances.
// Returns: CellResult.
// Side effects: None.
// Notes: A sheet has no inside, so §6's sides cannot come from S6 - it never classified one, and it
//   must not: a sheet claiming volume is the error the classification's guard exists to prevent.
//   They come from the cut instead. Two parent nodes are on the same side iff the edge between them
//   is *not* crossed, so a union-find over the uncut edges recovers the two sides directly, and the
//   labelling is fixed by taking the class of the smallest node id as "inside". Which class gets
//   that label does not matter, because a welded sheet is C0: both sides keep the parent's record
//   and only the tagged interface distinguishes them (PLAN §10.13).
//
//   Exactly two classes is the condition for a clean sheet cut. One class means the sheet ends
//   inside the cell - an interior rim, `split_R`'s case, which needs a rim node S7 does not yet
//   produce - and three means the crossings do not describe a single surface. Both escalate to the
//   conforming fan, so the mesh stays valid and the sheet is chamfered there, logged.
#[allow(clippy::too_many_arguments)]
fn cut_welded_sheet(
    tet: [u32; 4],
    component: i32,
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    multi_crossing: &BTreeMap<([u32; 2], i32), usize>,
    on_cut: &BTreeMap<(u32, i32), ()>,
    keys: &[NodeKey],
    nodes: &[Vec3],
    options: &CutOptions,
) -> CellResult {
    let uncut = |escalation: Option<Escalation>| CellResult {
        tets: SmallVec::from_slice(&[tet]),
        inside: SmallVec::from_slice(&[false]),
        interface: SmallVec::new(),
        component: 0,
        case: 0,
        welded: false,
        welded_with: None,
        escalation,
        min_dihedral: f64::INFINITY,
        volume_error: 0.0,
    };
    let edge_of = |a: usize, b: usize| -> [u32; 2] {
        let (x, y) = (tet[a], tet[b]);
        if x <= y {
            [x, y]
        } else {
            [y, x]
        }
    };
    for a in 0..4 {
        for b in (a + 1)..4 {
            if multi_crossing.contains_key(&(edge_of(a, b), component)) {
                return uncut(Some(Escalation::MultiCrossing));
            }
        }
    }
    let mut side: [NodeSide; 4] = std::array::from_fn(|slot| {
        if on_cut.contains_key(&(tet[slot], component)) {
            NodeSide::OnCut
        } else {
            NodeSide::Inside
        }
    });
    // Union-find over the uncut edges of the non-on-cut nodes.
    let mut parent = [0usize, 1, 2, 3];
    fn find(parent: &mut [usize; 4], node: usize) -> usize {
        let mut node = node;
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    for a in 0..4 {
        for b in (a + 1)..4 {
            if side[a] == NodeSide::OnCut || side[b] == NodeSide::OnCut {
                continue;
            }
            if cut_index.contains_key(&(edge_of(a, b), component)) {
                continue;
            }
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            parent[ra] = rb;
        }
    }
    let mut classes: SmallVec<[usize; 4]> = SmallVec::new();
    for (slot, kind) in side.iter().enumerate() {
        if *kind == NodeSide::OnCut {
            continue;
        }
        let root = find(&mut parent, slot);
        if !classes.contains(&root) {
            classes.push(root);
        }
    }
    if classes.len() != 2 {
        if std::env::var_os("RUSTMSPT_S67_DIAG").is_some() { println!("[S67-SITE] sheet-classes"); }
        return uncut(Some(Escalation::Inconsistent));
    }
    classes.sort_unstable();
    let roots: [usize; 4] = std::array::from_fn(|slot| find(&mut parent, slot));
    for (slot, kind) in side.iter_mut().enumerate() {
        if *kind == NodeSide::OnCut {
            continue;
        }
        *kind = if roots[slot] == classes[0] {
            NodeSide::Inside
        } else {
            NodeSide::Outside
        };
    }
    for a in 0..4 {
        for b in (a + 1)..4 {
            let straddles = matches!(
                (side[a], side[b]),
                (NodeSide::Inside, NodeSide::Outside) | (NodeSide::Outside, NodeSide::Inside)
            );
            if straddles != cut_index.contains_key(&(edge_of(a, b), component)) {
                if std::env::var_os("RUSTMSPT_S67_DIAG").is_some() { println!("[S67-SITE] sheet-straddle"); }
                return uncut(Some(Escalation::Inconsistent));
            }
        }
    }
    let cut_node = |a: usize, b: usize| -> Option<u32> {
        cut_index.get(&(edge_of(a, b), component)).copied()
    };
    let Some(cell) = cut_tet(tet, side, &cut_node, keys) else {
        if std::env::var_os("RUSTMSPT_S67_DIAG").is_some() { println!("[S67-SITE] sheet-no-row"); }
        return uncut(Some(Escalation::Inconsistent));
    };
    match guarded_dry_run(tet, &cell.tets, nodes, options) {
        Ok((oriented, min_dihedral, volume_error)) => CellResult {
            tets: oriented,
            inside: cell.inside.iter().copied().collect(),
            interface: cell.interface,
            component,
            case: cell.case,
            welded: true,
            welded_with: None,
            escalation: None,
            min_dihedral,
            volume_error,
        },
        Err(reason) => uncut(Some(reason)),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The four faces of one cell, each as the §5.2 state where that is expressible and as an
//   ordered boundary loop always.
// Inputs: the parent tet, the cut-node map, the on-cut set, the solid components, and the key table.
// Returns: per face (in `TET_FACES` order) the optional state and the loop.
// Side effects: None.
// Notes: Built from the *global* cut map rather than from one component's view, so a face carrying
//   cut nodes from two patches is described completely. The loop inserts each edge's cut nodes
//   between its endpoints, ordered along the edge from the endpoint that comes first in the
//   canonical frame - an order both incident cells compute identically, which is what J1 needs. An
//   edge carrying more than one cut node makes the §5.2 state inexpressible, and the caller falls
//   back to fanning the loop; that is a property of the *face*, so again both cells agree.
#[allow(clippy::too_many_arguments)]
fn face_states(
    tet: [u32; 4],
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    second_index: &BTreeMap<([u32; 2], i32), u32>,
    on_cut: &BTreeMap<(u32, i32), ()>,
    components: &[i32],
    keys: &[NodeKey],
    points: &[Vec3],
    curve_pierce: &BTreeMap<[u32; 3], Vec3>,
) -> CellFaces {
    std::array::from_fn(|face| {
        let mut slots = TET_FACES[face];
        slots.sort_by_key(|slot| keys[tet[*slot] as usize]);
        let nodes = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
        let mut state = FaceCutState {
            nodes,
            ..Default::default()
        };
        let mut expressible = true;
        let mut loop_nodes: SmallVec<[u32; 8]> = SmallVec::new();
        // The components that actually cut this face. `on_cut` is scoped to them
        // below, which is what makes this agree with a neighbour that took §6's
        // path: that neighbour sees the face through the one component it is
        // cutting, and a node "on the patch" means on *that* patch. Scoping to
        // every component instead would call a node on-cut for a patch this face
        // has nothing to do with, and split_2 would fire on one side only - which
        // is exactly the 1,033 hanging nodes the first version left behind.
        let mut cutting: SmallVec<[i32; 2]> = SmallVec::new();
        // Per component, the cut node it puts on each of the face's three edges.
        let mut per_component: BTreeMap<i32, [Option<u32>; 3]> = BTreeMap::new();
        // Per component, where its cut nodes sit on the boundary walk.
        let mut walk_positions: BTreeMap<i32, SmallVec<[usize; 2]>> = BTreeMap::new();
        let mut corner_at = [0usize; 3];
        for edge in 0..3 {
            let (a, b) = (nodes[edge], nodes[(edge + 1) % 3]);
            let key = if a <= b { [a, b] } else { [b, a] };
            let mut cuts: SmallVec<[u32; 2]> = SmallVec::new();
            for component in components {
                if let Some(node) = cut_index.get(&(key, *component)).copied() {
                    cuts.push(node);
                    cutting.push(*component);
                    per_component.entry(*component).or_insert([None; 3])[edge] = Some(node);
                }
                // Invariant K1's second crossing. It joins the walk so the face is
                // described completely; `per_component` deliberately does not record it,
                // because that map is what `face_is_single_patch` reads to decide whether
                // the face takes a §5.2 row, and a doubly-crossed edge never does.
                if let Some(node) = second_index.get(&(key, *component)).copied() {
                    cuts.push(node);
                    cutting.push(*component);
                }
            }
            cuts.sort_unstable();
            cuts.dedup();
            // Order the cuts *along the edge*, not by node id. `loop_nodes` is a
            // boundary walk, and a face two components cut carries two nodes on one
            // edge; taking them in id order puts them in whichever order the
            // crossings happened to be indexed, which walks the boundary backwards
            // over that stretch. Distance from `a` is a pure function of the two
            // nodes, so both cells incident to the face still agree - the old order
            // was conforming but geometrically wrong, and the pieces it produced on
            // such a face could come out inverted (they were dropped as degenerate).
            if cuts.len() > 1 {
                let origin = points[a as usize];
                cuts.sort_by(|left, right| {
                    let dl = points[*left as usize].sub(origin);
                    let dr = points[*right as usize].sub(origin);
                    dl.dot(dl)
                        .partial_cmp(&dr.dot(dr))
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(left.cmp(right))
                });
            }
            corner_at[edge] = loop_nodes.len();
            loop_nodes.push(a);
            if cuts.len() == 1 {
                state.cut[edge] = Some(cuts[0]);
            } else if cuts.len() > 1 {
                expressible = false;
            }
            // Record each cut node's position on the walk, per component. `per_component`
            // cannot serve: it is keyed by edge, so a component crossing one edge *twice*
            // - invariant K1's case - overwrites its own first entry and the second
            // crossing disappears. Positions keep both, which is what lets a K1 face be
            // described by its chord instead of coned to a centroid.
            for node in &cuts {
                let slot = loop_nodes.len();
                for component in components {
                    if cut_index.get(&(key, *component)) == Some(node)
                        || second_index.get(&(key, *component)) == Some(node)
                    {
                        walk_positions.entry(*component).or_default().push(slot);
                    }
                }
                loop_nodes.push(*node);
            }
        }
        cutting.sort_unstable();
        cutting.dedup();
        // A corner S7 snapped **onto** a patch is an endpoint of that patch's trace on this
        // face, exactly as a cut node is. Counting only cut nodes leaves such a trace with
        // one endpoint, `crossed_face` cannot pair it into a chord, and the face cones to a
        // centroid whose fan then straddles the surface: A-3's 84 faces with no locked
        // curve through them and A-7a's 464, every one. The scope is `cutting` for the same
        // reason `state.on_cut` uses it - a node on some *other* patch says nothing about
        // this face - so a face no component cuts is unaffected.
        for (slot, node) in nodes.iter().enumerate() {
            for component in cutting.iter() {
                if on_cut.contains_key(&(*node, *component)) {
                    let positions = walk_positions.entry(*component).or_default();
                    if !positions.contains(&corner_at[slot]) {
                        positions.push(corner_at[slot]);
                    }
                }
            }
        }
        for positions in walk_positions.values_mut() {
            positions.sort_unstable();
        }
        // Two components on one face usually means two patches cross it and §5.2 cannot
        // express the result - but "two components" is the wrong test for that, and it
        // was the last thing standing between G7-2's collapse and a conforming mesh.
        // After a rim collapse both walls of a gap cross the same edges at the *same*
        // node ids, so a collapsed face carries two components and is still an ordinary
        // single-patch face. Comparing what each component actually contributes tells
        // the two cases apart: identical per-edge maps are one surface reached twice,
        // differing maps are two surfaces. Counting components instead sent every
        // collapsed face to the loop fan while the cell that cut it took the table, and
        // the two triangulations of the shared face disagreed - 1,152 boundary leaks.
        if !face_is_single_patch(&per_component) {
            expressible = false;
        }
        for (slot, node) in nodes.iter().enumerate() {
            state.on_cut[slot] = cutting
                .iter()
                .any(|component| on_cut.contains_key(&(*node, *component)));
        }
        let crossed = if expressible {
            None
        } else {
            crossed_face(
                &walk_positions,
                &loop_nodes,
                points,
                curve_pierce.get(&nodes).copied(),
            )
        };
        (expressible.then_some(state), loop_nodes, crossed)
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Describe a face two patches cross, so it can be triangulated with both chords as edges instead of coned to its centroid.
// Inputs: each component's cut nodes per face edge, the face's boundary walk, the key table, and the node coordinates.
// Returns: a CrossedFace when exactly two components each leave exactly two cut nodes on the walk and their chords interleave around it; None otherwise.
// Side effects: None.
// Notes: Interleaving around the boundary walk is what "the two chords cross inside the face" means,
//   and it is a combinatorial test on the walk rather than a geometric one - so it cannot disagree
//   between the two cells sharing the face, which is Invariant J1's whole requirement. Chords that
//   do not interleave need no new point and are left to the centroid fan; that case is rarer and
//   splitting it buys nothing the fan does not already do safely.
//
//   The components are taken in ascending id and each chord's endpoints in ascending walk position,
//   so `chord_meeting_point` receives its four coordinates in one canonical order however the two
//   cells happened to enumerate their faces.
fn crossed_face(
    walk_positions: &BTreeMap<i32, SmallVec<[usize; 2]>>,
    loop_nodes: &[u32],
    points: &[Vec3],
    pierce: Option<Vec3>,
) -> Option<CrossedFace> {
    let diag = std::env::var_os("RUSTMSPT_JCT_DIAG").is_some();
    // A component whose trace enters and leaves this face is described by one chord
    // between its two cut nodes - wherever they sit on the walk, including both on the
    // *same* edge, which is invariant K1's face and the shape that leaves A-8's 3,543
    // uncuttable cells. More than two cut nodes means the trace visits the face more than
    // once and one chord does not describe it.
    let mut chords: SmallVec<[[usize; 2]; 2]> = SmallVec::new();
    let mut owners: SmallVec<[i32; 2]> = SmallVec::new();
    for (component, positions) in walk_positions {
        // Four cut nodes means the surface crosses this face **twice** - a lattice face
        // straddling a thin plate, cut once by each wall - and it is described by two
        // chords, not refused. The two walls cannot cross each other inside the face, so
        // of the three ways to pair four positions exactly the non-interleaving ones are
        // admissible, and pairing across the two crossed edges leaves only one. That test
        // is combinatorial, so the two cells sharing the face still agree (Invariant J1).
        if positions.len() == 4 {
            let mut sorted: SmallVec<[usize; 4]> = positions.iter().copied().collect();
            sorted.sort_unstable();
            let (p0, p1, p2, p3) = (sorted[0], sorted[1], sorted[2], sorted[3]);
            // (p0,p1)+(p2,p3) and (p0,p3)+(p1,p2) both avoid crossing; the walls each cut
            // *one* node from each crossed edge, and consecutive positions sit on the same
            // edge, so the pairing that separates them is the nested one.
            chords.push([p0, p3]);
            chords.push([p1, p2]);
            owners.push(*component);
            owners.push(*component);
            continue;
        }
        if positions.len() != 2 {
            if diag {
                println!(
                    "[JCT-FACE-DIAG] a trace leaves {} cut node(s), not 2 or 4; locked curve through this face: {}",
                    positions.len(),
                    pierce.is_some()
                );
            }
            return None;
        }
        let (lo, hi) = (positions[0].min(positions[1]), positions[0].max(positions[1]));
        if lo == hi {
            return None;
        }
        // A chord whose endpoints are **adjacent** on the walk lies along a walk edge:
        // both cut nodes sit on the same lattice edge, so the chord bounds no area and
        // splitting the walk by it leaves a run of collinear points that any vertex fan
        // triangulates into zero-area slivers. Every cell around the edge emits the same
        // sliver and it surfaces as a face shared by six tets - A-4's
        // `(61918, 66209, 303387)`. Refusing here sends the face to `LoopFan`, whose apex
        // is the face centroid and therefore off the line, so the triangulation is
        // non-degenerate *and* the cell boundary stays closed. Dropping the slivers
        // afterwards instead cleared the shared faces and cracks but opened 1,488
        // boundary leaks, because their edges then matched nothing.
        // Keeping the face's *other* chord instead of abandoning the face - `continue` rather
        // than `return None` - was tried and is neutral (A-6b 1,021 -> 1,022, everything else
        // unmoved), so the `mixed sides` triangles do not come from here.
        if hi == lo + 1 || (lo == 0 && hi + 1 == loop_nodes.len()) {
            if diag {
                println!("[JCT-FACE-DIAG] chord lies along a walk edge");
            }
            return None;
        }
        chords.push([lo, hi]);
        owners.push(*component);
    }
    if chords.is_empty() || chords.len() > 2 {
        if diag {
            println!("[JCT-FACE-DIAG] {} chord(s) on the face", chords.len());
        }
        return None;
    }
    if chords.len() == 1 {
        // One surface: the chord splits the face in two, and no new node is needed.
        return Some(CrossedFace {
            loop_nodes: loop_nodes.iter().copied().collect(),
            chords,
            components: owners,
            point: None,
            hub: None,
        });
    }
    let [a, b] = [chords[0], chords[1]];
    // Two chords meeting at a cut node **on the walk** rather than inside the face. Two
    // solids in exact face contact make this the common case, not the exception: S7 gives
    // the coincident crossings one shared node (`contact_edges`), so the contact plane's
    // chord and the limb's own wall chord both run out of it, and the face carries three
    // regions - cube, limb and void - around that one hub. Fanning the walk from the hub
    // keeps both chords as edges and interns nothing, so Invariant J1 is untouched. This
    // was A-6a's 516 refusals and A-6b's, every one falling to a centroid apex whose fan
    // then straddled the contact plane (`mixed sides on one triangle
    // [42302, 27586, 11041]`). Grouping the walk positions by *which* components cross
    // there was tried first and is wrong for the same geometry: it splits one component's
    // own chord in two, because the shared node carries both components and the far end
    // carries one.
    let shared: SmallVec<[usize; 2]> = a
        .iter()
        .filter(|position| b.contains(position))
        .copied()
        .collect();
    if shared.len() == 1 {
        let hub = shared[0];
        let far: SmallVec<[usize; 2]> = a
            .iter()
            .chain(b.iter())
            .filter(|position| **position != hub)
            .copied()
            .collect();
        // A hub adjacent on the walk to a chord's far end has that chord lying along a walk
        // edge, which is the degenerate shape the single-chord branch already refuses.
        let adjacent = |x: usize, y: usize| -> bool {
            let n = loop_nodes.len();
            (x + 1) % n == y || (y + 1) % n == x
        };
        // Only where no locked curve pierces the face. Where one does, its point is on
        // *every* surface that meets along it and the fan from it is strictly better -
        // measured, A-6a 37 against 269 and A-6b 60 against 554 with the hub preferred.
        // The hub is the answer for the faces that would otherwise cone to their centroid,
        // which is A-3's 84 and A-7a's 464, not for the ones the curve already reaches.
        if pierce.is_none() && far.len() == 2 && !far.iter().any(|end| adjacent(hub, *end)) {
            return Some(CrossedFace {
                loop_nodes: loop_nodes.iter().copied().collect(),
                chords,
                components: owners,
                point: None,
                hub: Some(hub),
            });
        }
    }
    if !shared.is_empty() {
        if diag {
            let n = loop_nodes.len();
            let far: SmallVec<[usize; 4]> = a
                .iter()
                .chain(b.iter())
                .filter(|position| !shared.contains(position))
                .copied()
                .collect();
            let adjacent = shared.len() == 1
                && far
                    .iter()
                    .any(|end| (shared[0] + 1) % n == *end || (*end + 1) % n == shared[0]);
            println!(
                "[JCT-FACE-DIAG] chords share {} endpoint(s); curve {}; far {}; hub adjacent {adjacent}",
                shared.len(),
                pierce.is_some(),
                far.len()
            );
        }
        return None;
    }
    // Interleaved: exactly one of the second chord's endpoints lies strictly between
    // the first chord's, walking the boundary. A combinatorial test, so the two cells
    // sharing the face cannot disagree about it.
    let between = |p: usize| p > a[0] && p < a[1];
    let point = (between(b[0]) != between(b[1])).then(|| {
        chord_meeting_point(
            points[loop_nodes[a[0]] as usize],
            points[loop_nodes[a[1]] as usize],
            points[loop_nodes[b[0]] as usize],
            points[loop_nodes[b[1]] as usize],
        )
    });
    Some(CrossedFace {
        loop_nodes: loop_nodes.iter().copied().collect(),
        chords,
        components: owners,
        point,
        hub: None,
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Build the interface index - each cut triangle with the element on each side of it.
// Inputs: the cut mesh, the interface triangles with the tet count at the point they were emitted,
//   and the key table.
// Returns: the tagged faces, sorted canonically.
// Side effects: None.
// Notes: **Derived, never stored** (PLAN §10.10, PLAN 1's rebuildable index). The side elements are
//   recovered by looking for the two tets that carry the triangle's three nodes, then asking each
//   one's record which side of the component it is on - so the index can be rebuilt from the mesh
//   alone at any time, and a stale index is impossible by construction. `side_elems[0]` is the
//   element inside the component, `[1]` the one outside, which is the export contract §10.13
//   reserves for cohesive/split-node tooling.
// AI-FUNC-SUMMARY: Squared distance from a point to a segment; returns f64; side effects: none.
fn point_segment_distance_sq(point: Vec3, a: Vec3, b: Vec3) -> f64 {
    let axis = b.sub(a);
    let length_sq = axis.dot(axis);
    let t = if length_sq > 0.0 {
        (point.sub(a).dot(axis) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let delta = point.sub(a.add(axis.scale(t)));
    delta.dot(delta)
}

// AI-FUNC-SUMMARY:
// Purpose: The mesh nodes that lie on an open sheet's rim - where a sheet is *allowed* to end (G7-2).
// Inputs: the node table, the arranged rim polylines, and the on-rim tolerance.
// Returns: the node ids within tolerance of some rim segment.
// Side effects: None.
// Notes: S7 already snaps lattice nodes onto rim curves, so an open sheet's cut front terminates on
//   the rim geometrically - measured on the solid-plus-sheet fixture, 19 of 22 boundary-edge
//   midpoints sit *exactly* on it, the other three being the chords that cut the square rim's
//   corners. What was missing was never the geometry, only the declaration: `cut_to_doc` emitted no
//   curve at all, so `[V8]` had nothing to match a sheet boundary against and called every rim edge
//   a pinhole. Membership is tested against the rim's own geometry rather than read off S7's
//   `constraint_ref`, for two reasons: a node that was *already* exactly on the curve gets no
//   proposal and so no `Polyline` constraint recorded, and a geometric test cannot quietly become a
//   rubber stamp the way "trust the producer's flag" can.
pub fn nodes_on_rim(nodes: &[Vec3], rim_segments: &[(Vec3, Vec3)], tolerance: f64) -> BTreeSet<u32> {
    if rim_segments.is_empty() {
        return BTreeSet::new();
    }
    let tolerance_sq = tolerance * tolerance;
    nodes
        .par_iter()
        .enumerate()
        .filter(|(_, point)| {
            rim_segments
                .iter()
                .any(|(a, b)| point_segment_distance_sq(**point, *a, *b) <= tolerance_sq)
        })
        .map(|(index, _)| index as u32)
        .collect()
}

// AI-FUNC-SUMMARY:
// Purpose: Whether the components cutting one face describe a single surface, so §5.2's table applies.
// Inputs: per component, the cut node it contributes on each of the face's three edges.
// Returns: true when every component present contributes the identical map.
// Side effects: None.
// Notes: The obvious test - "more than one component means more than one patch" - is wrong after a
//   G7-2 rim collapse, because both walls of a collapsed gap cross the same edges at the *same*
//   node ids and the face is still an ordinary single-patch face. Using the component count sent
//   every collapsed face to the loop fan while the cell that cut it took the table, and the two
//   triangulations of the shared face disagreed: 1,152 boundary leaks on the two-plate fixture.
pub fn face_is_single_patch(per_component: &BTreeMap<i32, [Option<u32>; 3]>) -> bool {
    let mut contributions = per_component.values();
    let Some(first) = contributions.next() else {
        return true;
    };
    contributions.all(|other| other == first)
}

// AI-FUNC-SUMMARY:
// Purpose: The rim curve of a collapsed sheet - its boundary edges that the collapse's own geometry
//   explains (G7-2).
// Inputs: the derived interface index, and the rim nodes that lie where the collapsed region ends.
// Returns: the rim edges, ascending and deduplicated.
// Side effects: None.
// Notes: A boundary edge qualifies only when **both** endpoints sit on the boundary of the collapsed
//   region, which is a property of the collapse decision rather than of the emitted faces. That is
//   deliberate: an edge bounding a hole in the *middle* of a sheet has interior endpoints, is not
//   declared here, and so still reaches `[V8]` as the pinhole it is.
pub fn collapsed_sheet_rim(
    interfaces: &[InterfaceFace],
    rim_boundary_nodes: &BTreeSet<u32>,
) -> Vec<[u32; 2]> {
    if rim_boundary_nodes.is_empty() {
        return Vec::new();
    }
    let mut uses: BTreeMap<[u32; 2], usize> = BTreeMap::new();
    for face in interfaces {
        if face.kind != FACE_TAG_SHEET {
            continue;
        }
        for slot in 0..3 {
            let (a, b) = (face.nodes[slot], face.nodes[(slot + 1) % 3]);
            let edge = if a <= b { [a, b] } else { [b, a] };
            *uses.entry(edge).or_insert(0) += 1;
        }
    }
    uses.into_iter()
        .filter(|(edge, count)| {
            *count == 1
                && rim_boundary_nodes.contains(&edge[0])
                && rim_boundary_nodes.contains(&edge[1])
        })
        .map(|(edge, _)| edge)
        .collect()
}

fn derive_interface(
    mesh: &CutMesh,
    pending: &[(usize, [u32; 3], i32)],
    keys: &[NodeKey],
    sheets: &[i32],
    rim_nodes: &BTreeSet<u32>,
    contact_patches: &[([Vec3; 3], SmallVec<[i32; 2]>)],
    eps: f64,
) -> Vec<InterfaceFace> {
    // node -> the tets touching it, so the search for a triangle's owners is local.
    let mut touching: BTreeMap<u32, SmallVec<[u32; 8]>> = BTreeMap::new();
    for (index, tet) in mesh.tets.iter().enumerate() {
        for node in tet {
            touching.entry(*node).or_default().push(index as u32);
        }
    }
    let mut out = Vec::with_capacity(pending.len());
    let mut dropped_one_sided = 0usize;
    for (_, triangle, component) in pending {
        let mut sorted = *triangle;
        sorted.sort_by_key(|node| keys[*node as usize]);
        // A declared sheet component owns no volume, so its sides cannot come from the
        // ownership record and are taken geometrically below.
        let is_sheet = sheets.contains(component);
        // The *tag* is broader than that. G7-2's collapse fuses the two walls of a gap
        // along the edges it welds, and what is left where every corner is a rim node
        // is an embedded welded sheet however the bodies on either side were declared
        // (§10.13). Its sides still come from ownership - the two solids are still
        // there, one on each side - so only the kind changes.
        let collapsed_sheet = !is_sheet
            && !rim_nodes.is_empty()
            && triangle.iter().all(|node| rim_nodes.contains(node));
        let mut inside_elem = -1i32;
        let mut outside_elem = -1i32;
        let Some(candidates) = touching.get(&triangle[0]) else {
            continue;
        };
        for candidate in candidates {
            let tet = mesh.tets[*candidate as usize];
            if !triangle.iter().all(|node| tet.contains(node)) {
                continue;
            }
            if is_sheet {
                // G7-2. A sheet owns no volume, so S6 wrote it no ownership entry and
                // `side_of` answers `Outside` for *both* neighbours - which silently
                // collapsed `(elem⁺, elem⁻)` to `(-1, one of them)` and threw away the
                // adjacency §10.13 reserves the pair to carry. The sides are geometric
                // instead: the neighbour whose fourth node is on the positive side of
                // the face's own plane is `elem⁺`. `sorted` is keyed, so both cells
                // incident to the face compute the same normal and agree on which of
                // them is which - the same argument Rule SNK rests on.
                let Some(apex) = tet.iter().find(|node| !triangle.contains(node)) else {
                    continue;
                };
                let side = orient3d(
                    mesh.nodes[sorted[0] as usize],
                    mesh.nodes[sorted[1] as usize],
                    mesh.nodes[sorted[2] as usize],
                    mesh.nodes[*apex as usize],
                );
                if side > 0.0 {
                    inside_elem = *candidate as i32;
                } else if side < 0.0 {
                    outside_elem = *candidate as i32;
                }
                continue;
            }
            let record = &mesh.records[*candidate as usize];
            if record.side_of(*component) == Side::Inside {
                inside_elem = *candidate as i32;
            } else {
                outside_elem = *candidate as i32;
            }
        }
        // A face whose two elements agree about the component is **not** a material
        // boundary, and recording it as one leaves a side element underivable (`-1`) - the
        // contract reserves `(elem+, elem-)` to carry the adjacency, so half a pair is worse
        // than no entry. It arises where §7.6 caps a piece and §7.5's sample then puts both
        // pieces on the same side of that cap: the cap is a real face of the mesh, just an
        // internal one. Dropping it is what `[V6]` asks for too - every *material boundary*
        // must be declared, not every surface the cut happened to draw.
        if !is_sheet && (inside_elem < 0 || outside_elem < 0) {
            dropped_one_sided += 1;
            continue;
        }
        out.push(InterfaceFace {
            nodes: sorted,
            component: *component,
            kind: if is_sheet || collapsed_sheet {
                FACE_TAG_SHEET
            } else {
                FACE_TAG_INTERFACE
            },
            side_elems: [inside_elem, outside_elem],
        });
    }
    if dropped_one_sided > 0 && std::env::var_os("RUSTMSPT_CUT_DIAG").is_some() {
        println!(
            "[G6-3] {dropped_one_sided} cut face(s) had both elements on the same side of their \
             own component and were not declared interfaces"
        );
    }
    out.sort_by_key(|face| (face.component, face.nodes));
    out.dedup_by_key(|face| (face.component, face.nodes));
    declare_contact_components(mesh, &mut out, contact_patches, eps);
    out.sort_by_key(|face| (face.component, face.nodes));
    out.dedup_by_key(|face| (face.component, face.nodes));
    out
}

// AI-FUNC-SUMMARY: Whether a point lies within `eps` of a triangle, plane distance and barycentric containment together; side effects: none.
fn point_on_triangle(point: Vec3, tri: [Vec3; 3], eps: f64) -> bool {
    let ab = tri[1].sub(tri[0]);
    let ac = tri[2].sub(tri[0]);
    let normal = ab.cross(ac);
    let area2 = normal.dot(normal);
    if area2 <= 0.0 {
        return false;
    }
    let rel = point.sub(tri[0]);
    let signed = normal.dot(rel);
    if signed * signed > eps * eps * area2 {
        return false;
    }
    // Barycentric, with the same `eps` widened into the plane so a node exactly on the
    // patch's own edge counts as on it.
    let slack = eps * area2.sqrt();
    let u = normal.dot(ab.cross(rel));
    let v = normal.dot(rel.cross(ac));
    u >= -slack && v >= -slack && u + v <= area2 + slack
}

// AI-FUNC-SUMMARY:
// Purpose: Declare every mesh face lying on a coincident arranged patch for ALL the components that patch belongs to.
// Inputs: the cut mesh (records and tets), the faces derived so far, S2's coincident patches, and the envelope tolerance.
// Returns: nothing; appends the missing `(component, nodes)` tags in place.
// Side effects: None beyond the push.
// Notes: Two solids in exact face contact share one mesh face that is both their boundaries. S2 sees
//   this and tags the arranged face with both components, but everything downstream reads the single
//   `ArrangedFace::component`, so the shared face reaches S8 as one body's boundary - and where the
//   cut declined it entirely, as **neither's**. A-6a/A-6b's 544/3,156 `[V6]` violations were all
//   exactly that: adjacent tets stepping `{1} -> {2}` across a face carrying no interface tag at all.
//
//   The rule here is geometric, not record-based, and that distinction is the point. Declaring any
//   face whose neighbours' inside-sets differ would make `[V6]`'s coincidence exemption vacuous -
//   every escalation chamfer would declare itself legal. A face is declared only where two input
//   surfaces are **genuinely coincident**, which is the condition the exemption was written for;
//   a two-component step anywhere else still fails, as it should.
fn declare_contact_components(
    mesh: &CutMesh,
    out: &mut Vec<InterfaceFace>,
    patches: &[([Vec3; 3], SmallVec<[i32; 2]>)],
    eps: f64,
) {
    if patches.is_empty() {
        return;
    }
    let bounds: Vec<(Vec3, Vec3)> = patches
        .iter()
        .map(|(tri, _)| {
            let lo = Vec3::new(
                tri[0].x.min(tri[1].x).min(tri[2].x) - eps,
                tri[0].y.min(tri[1].y).min(tri[2].y) - eps,
                tri[0].z.min(tri[1].z).min(tri[2].z) - eps,
            );
            let hi = Vec3::new(
                tri[0].x.max(tri[1].x).max(tri[2].x) + eps,
                tri[0].y.max(tri[1].y).max(tri[2].y) + eps,
                tri[0].z.max(tri[1].z).max(tri[2].z) + eps,
            );
            (lo, hi)
        })
        .collect();
    let in_any = |p: Vec3| -> bool {
        bounds.iter().any(|(lo, hi)| {
            p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y && p.z >= lo.z && p.z <= hi.z
        })
    };

    // Interior faces near a patch, and the two tets that share them.
    // Filter by the **face**, never by the tet. A contact patch is a triangle in a plane,
    // so its inflated box has no thickness; a tet touching the plane necessarily has an
    // apex `h` away and would be rejected outright. That mistake made this whole pass
    // silently inert while 62,216 mesh faces sat exactly on A-6b's contact plane.
    const TET_FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];
    let mut shared: BTreeMap<[u32; 3], SmallVec<[u32; 2]>> = BTreeMap::new();
    for (index, tet) in mesh.tets.iter().enumerate() {
        for slots in TET_FACES {
            let mut key = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
            if !key.iter().all(|node| in_any(mesh.nodes[*node as usize])) {
                continue;
            }
            key.sort_unstable();
            shared.entry(key).or_default().push(index as u32);
        }
    }

    let mut tagged: BTreeSet<([u32; 3], i32)> = BTreeSet::new();
    for face in out.iter() {
        tagged.insert((face.nodes, face.component));
    }
    let mut extra: Vec<InterfaceFace> = Vec::new();
    for (key, owners) in shared {
        if owners.len() != 2 {
            continue;
        }
        let corners = [
            mesh.nodes[key[0] as usize],
            mesh.nodes[key[1] as usize],
            mesh.nodes[key[2] as usize],
        ];
        // Each corner need only lie on *some* patch, not all three on the same one: the
        // contact region is triangulated by S2 into several faces, and a mesh face
        // straddling two of them is just as much on the contact as one inside a single
        // patch. Requiring one patch to hold all three missed ~190 of A-6b's on-plane
        // faces. The component sets must agree, which is what keeps this from unioning
        // across two unrelated contacts that happen to touch.
        let mut components: Option<&SmallVec<[i32; 2]>> = None;
        let mut covered = true;
        for corner in &corners {
            let hit = patches
                .iter()
                .find(|(tri, _)| point_on_triangle(*corner, *tri, eps));
            match (hit, components) {
                (None, _) => {
                    covered = false;
                    break;
                }
                (Some((_, here)), None) => components = Some(here),
                (Some((_, here)), Some(there)) if here == there => {}
                (Some(_), Some(_)) => {
                    covered = false;
                    break;
                }
            }
        }
        let (true, Some(components)) = (covered, components) else {
            continue;
        };
        let left = &mesh.records[owners[0] as usize];
        let right = &mesh.records[owners[1] as usize];
        for component in components.iter().copied() {
            let here = left.side_of(component) == Side::Inside;
            let there = right.side_of(component) == Side::Inside;
            if here == there || tagged.contains(&(key, component)) {
                continue;
            }
            extra.push(InterfaceFace {
                nodes: key,
                component,
                kind: FACE_TAG_INTERFACE,
                side_elems: if here {
                    [owners[0] as i32, owners[1] as i32]
                } else {
                    [owners[1] as i32, owners[0] as i32]
                },
            });
        }
    }
    out.extend(extra);
}

// ---------------------------------------------------------------------------
// s08_cut
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Encode the cut mesh as the `s08_cut` snapshot VTU.
// Inputs: the cut mesh and the run's components.
// Returns: VtuDoc of `VTK_TETRA` cells followed by the tagged interface triangles.
// Side effects: None.
// Notes: This is the first snapshot whose *cells* follow the geometry, and the first to carry
//   tagged faces, so it exercises the parts of the contract ([V6] region keys, [V7] sheet metrics,
//   the `FaceTag*` tables) that earlier stages could only leave empty.
pub fn cut_to_doc(mesh: &CutMesh, components: &[ArrangeComponent]) -> VtuDoc {
    let mut doc = VtuDoc {
        points: mesh.nodes.clone(),
        ..Default::default()
    };
    for tet in &mesh.tets {
        doc.connectivity.extend(tet.iter().map(|node| *node as i64));
        doc.offsets.push(doc.connectivity.len() as i64);
        doc.types.push(VTK_TETRA);
    }
    // One cell per *face*, not per tag. Two solids in exact contact share one mesh face
    // that is both their boundaries, and `derive_interface` rightly derives it once per
    // component - but emitting a triangle for each wrote the same face twice, which is a
    // duplicate cell (A-6a's `[V1]` reported 50, all on the contact plane) and leaves every
    // tag set a singleton, so `[V6]`'s coincidence exemption has nothing to read. The
    // `FaceTag*` tables are a set per face precisely so this case has a representation.
    // Grouped by node *set*. `derive_interface` orders a face's nodes by quantised key,
    // and two distinct nodes can share a key - the sort is stable, so the same face
    // reached from two components can come out `[a, b, c]` one time and `[a, c, b]` the
    // next. Comparing the arrays missed exactly those, which is the pair A-6a reports.
    let identity = |face: &InterfaceFace| -> [u32; 3] {
        let mut key = face.nodes;
        key.sort_unstable();
        key
    };
    let mut order: Vec<usize> = (0..mesh.interfaces.len()).collect();
    order.sort_by_key(|index| {
        let face = &mesh.interfaces[*index];
        (identity(face), face.component)
    });
    let mut tagged: Vec<([u32; 3], SmallVec<[usize; 2]>)> = Vec::new();
    for index in order {
        let face = &mesh.interfaces[index];
        match tagged.last_mut() {
            Some((nodes, tags)) if identity(&mesh.interfaces[tags[0]]) == identity(face) => {
                let _ = nodes;
                tags.push(index);
            }
            _ => tagged.push((face.nodes, smallvec::smallvec![index])),
        }
    }
    for (nodes, _) in &tagged {
        doc.connectivity.extend(nodes.iter().map(|node| *node as i64));
        doc.offsets.push(doc.connectivity.len() as i64);
        doc.types.push(VTK_TRIANGLE);
    }
    // G7-2: the collapsed sheet's rim, as line cells, so the sheet's boundary is a
    // declared curve (`CurveKind = 1`) rather than something `[V8]` must call a leak.
    for edge in &mesh.rim_curve {
        doc.connectivity.extend(edge.iter().map(|node| *node as i64));
        doc.offsets.push(doc.connectivity.len() as i64);
        doc.types.push(VTK_POLY_LINE);
    }
    // G6-4: the locked curves the mesh actually carries, as line cells whose `curve_id`
    // points at the table entry that declares the curve's kind, components and radial
    // patch count. Without these `[V9]` has nothing to read and skips - which is how a
    // gate came to be marked done against a check that never ran.
    for (_, edge) in &mesh.curve_edges {
        doc.connectivity.extend(edge.iter().map(|node| *node as i64));
        doc.offsets.push(doc.connectivity.len() as i64);
        doc.types.push(VTK_POLY_LINE);
    }
    let tets = mesh.tets.len();
    let faces = tagged.len();
    let rims = mesh.rim_curve.len();
    let locked = mesh.curve_edges.len();
    let cells = tets + faces + rims + locked;
    let priority_of: BTreeMap<i32, u32> = components
        .iter()
        .map(|component| (component.x, component.priority))
        .collect();

    let mut cell_kind = vec![0u8; tets];
    cell_kind.extend(std::iter::repeat_n(1u8, faces));
    cell_kind.extend(std::iter::repeat_n(2u8, rims + locked));
    doc.cell_data
        .push(DataArray::scalar("cell_kind", ArrayData::U8(cell_kind)));

    // Region keys, recomputed from the records the cut wrote.
    let keys: Vec<Vec<i32>> = mesh
        .records
        .par_iter()
        .map(|record| resolve(record, &priority_of))
        .collect();
    let mut region_sets: Vec<Vec<i32>> = keys.clone();
    region_sets.par_sort_unstable();
    region_sets.dedup();
    let index_of: BTreeMap<Vec<i32>, i32> = region_sets
        .iter()
        .enumerate()
        .map(|(index, key)| (key.clone(), index as i32))
        .collect();
    let mut region_key: Vec<i32> = keys.iter().map(|key| index_of[key]).collect();
    // `-1`, not `0`: a face or rim cell owns no volume and therefore no region, and `0`
    // is a *real* region key (the background). Padding with `0` made every tagged face
    // and rim line claim background material, so colouring by `region_key` in a viewer
    // painted the whole cut surface as background - which reads as the solid having
    // vanished. The not-applicable sentinel is `-1` here, as it is for `band_region` and
    // `face_tag_key`.
    region_key.resize(cells, -1i32);
    doc.cell_data
        .push(DataArray::scalar("region_key", ArrayData::I32(region_key)));
    doc.cell_data.push(DataArray::scalar(
        "partition_id",
        ArrayData::I32(vec![0; cells]),
    ));
    // P-4's instrument, on the points rather than the cells: 0 is a lattice node S5 placed and
    // S7 may have snapped, 1 is a node the cut interned. `provenance` answers "was this element
    // written by the cut", which is a different question and the one that misled §6.1 - a cell
    // can be cut on one face and still present a raw lattice face as its material boundary
    // elsewhere, and then reads as `Cut` on both owners.
    let node_origin: Vec<u8> = (0..doc.points.len())
        .map(|node| u8::from(node as u32 >= mesh.n_lattice_nodes))
        .collect();
    doc.point_data
        .push(DataArray::scalar("node_origin", ArrayData::U8(node_origin)));
    // The real per-element regime, not a placeholder: S8b's gap slabs are band
    // elements (1) or band-Steiner elements (2), and `[V7]`'s one-layer check reads
    // exactly this array. Writing zeros made that check vacuous on pipeline output.
    let mut regime = mesh.regime.clone();
    regime.resize(tets, REGIME_NORMAL);
    regime.extend(std::iter::repeat_n(255u8, cells - tets));
    doc.cell_data
        .push(DataArray::scalar("regime", ArrayData::U8(regime)));
    // G7-2: the thin region each band element belongs to. Without it `[V7]`'s
    // one-layer check has no way to group band elements by the gap they span, so it
    // reported `band_regions = 0` on every mesh S8b had ever produced.
    let mut band_region = mesh.band_region.clone();
    band_region.resize(cells, -1);
    doc.cell_data.push(DataArray::scalar(
        "band_region",
        ArrayData::I32(band_region),
    ));

    let mut face_tag_key = vec![-1i32; tets];
    face_tag_key.extend((0..faces).map(|index| index as i32));
    face_tag_key.resize(cells, -1);
    doc.cell_data.push(DataArray::scalar(
        "face_tag_key",
        ArrayData::I32(face_tag_key),
    ));
    let mut curve_id = vec![-1i32; tets + faces];
    curve_id.extend((0..rims).map(|index| index as i32));
    curve_id.extend(
        mesh.curve_edges
            .iter()
            .map(|(curve, _)| rims as i32 + *curve as i32),
    );
    doc.cell_data
        .push(DataArray::scalar("curve_id", ArrayData::I32(curve_id)));
    let mut provenance: Vec<u8> = mesh
        .records
        .iter()
        .map(|record| record.provenance as u8)
        .collect();
    provenance.resize(cells, 0u8);
    doc.cell_data
        .push(DataArray::scalar("provenance", ArrayData::U8(provenance)));
    // Which lattice cell each tet came from. Off by default because it is not part of the
    // contract and every golden VTU would change; under `RUSTMSPT_CUT_DIAG` it is what
    // separates a defect *inside* one escalated cell from a disagreement between two.
    if std::env::var_os("RUSTMSPT_CUT_DIAG").is_some() {
        let mut parent: Vec<i32> = mesh.parent_of.iter().map(|index| *index as i32).collect();
        parent.resize(cells, -1);
        doc.cell_data
            .push(DataArray::scalar("parent_cell", ArrayData::I32(parent)));
        // *Why* the parent cell escalated, per tet: the `Escalation` ordinal, or -1 when the
        // cell took the §6 table. `provenance` already says a tet came from the fallback;
        // this says which gap in the table sent it there, and gate P-3.1 turns on that
        // distinction - the answer differs per reason (a new table row, an upstream fix in
        // §5.2/S7, or genuine local recovery with Steiner insertion), so the design effort
        // has to go where the P3 damage actually is rather than where the cell count is.
        let escalation_of: BTreeMap<u32, Escalation> = mesh.escalated.iter().copied().collect();
        let mut reason: Vec<i32> = mesh
            .parent_of
            .iter()
            .map(|cell| escalation_of.get(cell).map_or(-1, |e| *e as i32))
            .collect();
        reason.resize(cells, -1);
        doc.cell_data
            .push(DataArray::scalar("escalation_reason", ArrayData::I32(reason)));
    }

    // `N_ID` per `PLAN_mesh_generation.md` §5.3: the union of the resolved element labels
    // of the incident tets and the component of every tagged face incident to the node.
    // It was a stub of zeros, which made `[V9]`'s first clause - "a curve node's `N_ID`
    // contains the curve's components" - unanswerable, and would have made it *pass*
    // vacuously for a background-only set. Background is the sentinel 0 and is carried, so
    // an interface node between `{3}` and the void reads `{0, 3}` as §5.3's example says.
    let points = doc.points.len();
    let mut n_id: Vec<BTreeSet<i32>> = vec![BTreeSet::new(); points];
    for (index, tet) in mesh.tets.iter().enumerate() {
        let label = &keys[index];
        for node in tet {
            n_id[*node as usize].extend(label.iter().copied());
        }
    }
    for (nodes, tags) in &tagged {
        for node in nodes {
            for tag in tags {
                n_id[*node as usize].insert(mesh.interfaces[*tag].component);
            }
        }
    }
    let mut n_id_sets: Vec<Vec<i32>> = Vec::new();
    let mut n_id_index: BTreeMap<Vec<i32>, i32> = BTreeMap::new();
    let mut n_id_key: Vec<i32> = Vec::with_capacity(points);
    for set in &n_id {
        let members: Vec<i32> = set.iter().copied().collect();
        let next = n_id_sets.len() as i32;
        let key = *n_id_index.entry(members.clone()).or_insert_with(|| {
            n_id_sets.push(members);
            next
        });
        n_id_key.push(key);
    }
    doc.point_data
        .push(DataArray::scalar("n_id_key", ArrayData::I32(n_id_key)));
    let mut constraint_kind = mesh.constraint_kind.clone();
    constraint_kind.resize(points, 0u8);
    doc.point_data.push(DataArray::scalar(
        "constraint_kind",
        ArrayData::U8(constraint_kind),
    ));
    let mut constraint_ref = mesh.constraint_ref.clone();
    constraint_ref.resize(points, -1i32);
    doc.point_data.push(DataArray::scalar(
        "constraint_ref",
        ArrayData::I32(constraint_ref),
    ));

    let mut offsets: Vec<i64> = Vec::with_capacity(region_sets.len());
    let mut flat: Vec<i32> = Vec::new();
    let mut priorities: Vec<u32> = Vec::with_capacity(region_sets.len());
    for key in &region_sets {
        flat.extend(key.iter().copied());
        offsets.push(flat.len() as i64);
        priorities.push(if key == &[0] {
            u32::MAX
        } else {
            key.iter()
                .filter_map(|x| priority_of.get(x).copied())
                .min()
                .unwrap_or(u32::MAX)
        });
    }
    push_field(&mut doc, "RegionSetOffsets", 1, ArrayData::I64(offsets));
    push_field(&mut doc, "RegionSetComponents", 1, ArrayData::I32(flat));
    push_field(&mut doc, "RegionSetPriority", 1, ArrayData::U32(priorities));
    let mut n_id_offsets: Vec<i64> = Vec::with_capacity(n_id_sets.len());
    let mut n_id_flat: Vec<i32> = Vec::new();
    for set in &n_id_sets {
        n_id_flat.extend(set.iter().copied());
        n_id_offsets.push(n_id_flat.len() as i64);
    }
    push_field(&mut doc, "NIdSetOffsets", 1, ArrayData::I64(n_id_offsets));
    push_field(&mut doc, "NIdSetComponents", 1, ArrayData::I32(n_id_flat));

    // The face-tag tables: one set per interface face, carrying its component, the
    // orientation of the tag, its kind, and the (inside, outside) element pair.
    let mut tag_offsets: Vec<i64> = Vec::with_capacity(faces);
    let mut tag_components: Vec<i32> = Vec::with_capacity(faces);
    let mut tag_orientation: Vec<i32> = Vec::with_capacity(faces);
    let mut tag_kind: Vec<u8> = Vec::with_capacity(faces);
    let mut tag_side_elems: Vec<i32> = Vec::with_capacity(faces * 2);
    for (_, tags) in &tagged {
        for index in tags {
            tag_components.push(mesh.interfaces[*index].component);
        }
        tag_offsets.push(tag_components.len() as i64);
        let first = &mesh.interfaces[tags[0]];
        tag_orientation.push(1);
        tag_kind.push(first.kind);
        tag_side_elems.push(first.side_elems[0]);
        tag_side_elems.push(first.side_elems[1]);
    }
    push_field(&mut doc, "FaceTagOffsets", 1, ArrayData::I64(tag_offsets));
    push_field(
        &mut doc,
        "FaceTagComponents",
        1,
        ArrayData::I32(tag_components),
    );
    push_field(
        &mut doc,
        "FaceTagOrientation",
        1,
        ArrayData::I32(tag_orientation),
    );
    push_field(&mut doc, "FaceTagKind", 1, ArrayData::U8(tag_kind));
    push_field(
        &mut doc,
        "FaceTagSideElems",
        2,
        ArrayData::I32(tag_side_elems),
    );
    // G7-2: the thin-region tables, indexed by `band_region`. s03 emits them from the
    // gap field; carrying regime and pair class forward is what lets a consumer of the
    // *mesh* say what kind of gap a band element spans - a solid-sheet contact, two
    // sheets, or two faces of one body - without holding on to the s03 snapshot.
    push_field(
        &mut doc,
        "ThinRegionRegime",
        1,
        ArrayData::U8(mesh.thin_regime.clone()),
    );
    push_field(
        &mut doc,
        "ThinRegionPairClass",
        1,
        ArrayData::U8(mesh.thin_pair_class.clone()),
    );
    push_field(
        &mut doc,
        "ComponentX",
        1,
        ArrayData::I32(components.iter().map(|c| c.x).collect()),
    );
    push_field(
        &mut doc,
        "ComponentY",
        1,
        ArrayData::U32(components.iter().map(|c| c.priority).collect()),
    );
    push_field(
        &mut doc,
        "ComponentKind",
        1,
        ArrayData::U8(components.iter().map(|c| c.kind).collect()),
    );
    push_field(
        &mut doc,
        "ComponentClosed",
        1,
        ArrayData::U8(components.iter().map(|c| u8::from(c.closed)).collect()),
    );
    // One table entry per collapsed-sheet rim edge, then one per locked curve - including
    // the curves the mesh carries no edge for, because "declared and not carried" is a
    // finding `[V9]` must be able to make and an absent row cannot say it.
    let mut curve_kind: Vec<u8> = vec![CURVE_KIND_RIM; rims];
    let mut curve_comp_offsets: Vec<i64> = Vec::with_capacity(rims + mesh.curves.len());
    let mut curve_comps: Vec<i32> = Vec::new();
    let mut curve_radial: Vec<i32> = vec![0; rims];
    for _ in 0..rims {
        curve_comp_offsets.push(curve_comps.len() as i64);
    }
    for curve in &mesh.curves {
        curve_kind.push(curve.kind);
        curve_comps.extend(curve.components.iter().copied());
        curve_comp_offsets.push(curve_comps.len() as i64);
        curve_radial.push(curve.radial_patches as i32);
    }
    push_field(&mut doc, "CurveKind", 1, ArrayData::U8(curve_kind));
    push_field(
        &mut doc,
        "CurveCompOffsets",
        1,
        ArrayData::I64(curve_comp_offsets),
    );
    push_field(
        &mut doc,
        "CurveCompComponents",
        1,
        ArrayData::I32(curve_comps),
    );
    // Additive to `SPEC_meshgen_contracts.md` §2.3 (recorded 2026-08-13): the radial patch
    // count S2 ordered around each curve, which `[V9]`'s second clause compares the mesh's
    // material sectors against. The check is specified against "the curve table" and the
    // frozen table carried no column that could answer it.
    push_field(
        &mut doc,
        "CurveRadialPatches",
        1,
        ArrayData::I32(curve_radial),
    );
    doc
}

// AI-FUNC-SUMMARY: Append one field-data array with an explicit component count; side effects: mutates VtuDoc field_data.
fn push_field(doc: &mut VtuDoc, name: &str, components: usize, data: ArrayData) {
    doc.field_data.push(DataArray {
        name: name.to_string(),
        components,
        data,
    });
}



// AI-FUNC-SUMMARY:
// Purpose: Cut one escalated cell along the surfaces crossing it (`SPEC_meshgen_geometry.md` §7.6) instead of giving the cut up, returning the material pieces and the cap triangles to declare.
// Inputs: the cell's conforming boundary soup, its parent tet, the cut-node index, the components in play, the point classifier, the mesh's node and key arrays to intern cap Steiner points into, the key quantum, and the dry-run's volume tolerance.
// Returns: (pieces, caps) - each piece a closed soup ready for the same fan the whole cell would have taken, each cap a triangle lying on a named component's surface; None whenever the split is not provably sound.
// Side effects: Appends cap Steiner nodes to `nodes` and `keys`. On None those nodes stay unreferenced, which costs a coordinate and changes nothing.
// Notes: The fan this replaces is **unconditionally** valid - a parent tet is convex and its
//   centroid sees its whole boundary - and that is precisely what a sequential cut cannot promise,
//   so the guard is the point of this function rather than a detail of it. Two independent checks
//   must pass. `split_soup_by_component` refuses any soup it cannot cleanly separate and cap with
//   one simple loop agreed by both halves. Then the fan volumes over every piece must still sum to
//   the parent's, which is what catches a piece whose centroid fell outside it and whose fan folds
//   over itself - the one way a non-convex piece can pass the first check and still be wrong.
//   Either failure discards the whole split and the caller keeps the single fan.
//
//   Only cells crossed by two or more components are attempted. A single-component escalation is
//   §6 refusing a legal row, not a junction; splitting it buys nothing the table would not have
//   done better, and every extra path here is a path the fan's totality no longer covers.
/// What the spoke-cut probe counts, per (piece, component) that straddles.
///
/// The question it exists to answer, before any of §7.6's fan is rebuilt: **can the surface
/// be recovered inside an escalated cell by cutting interior spokes only?** That rests on
/// one claim - a fan tet's *base* never straddles, so every straddling edge is a spoke from
/// a boundary node to the piece centroid, interior to one cell and shared with no
/// neighbour, and invariant J1 survives without an agreement protocol.
///
/// `split_soup_by_surface` documents the case that claim assumes away. Its `mixed sides`
/// relaxation exists for "a triangle with corners on both sides and no cut node between
/// them ... a **face** that was not split where this surface crosses it", and that
/// relaxation is load-bearing - it took `[V6]` A-6a 37 -> 21. Such a triangle *is* a fan
/// tet's base, and cutting its edges is the J1 problem the derivation claimed to escape.
///
/// So `base_straddle` against `apex_only` decides it. `apex_only` is §6's Case C
/// `(1,3,0)`/`(3,1,0)`: base uniform, apex opposite, all three crossings on spokes - the
/// cuttable case. `base_straddle` is the derivation failing.
#[derive(Default, Clone, Copy, Debug)]
struct SpokeProbe {
    /// Fanned pieces examined, whatever they are made of.
    pieces: usize,
    /// (piece, component) pairs where the component's surface crosses the piece.
    straddling: usize,
    /// ...of which the piece came from a §7.6 split rather than the whole-cell fallback fan.
    from_split: usize,
    /// Fan tets in straddling pieces.
    fan_tets: usize,
    /// ...that the surface actually crosses.
    straddling_tets: usize,
    /// ...whose base carries both sides. **The derivation is false for these.**
    base_straddle: usize,
    /// ...whose base is uniform with the apex opposite: §6 Case C, cuttable on spokes alone.
    apex_only: usize,
}

// AI-FUNC-SUMMARY:
// Purpose: Count how a component's surface meets the fan tets of one piece, to decide whether
//   §7.6's fan can be cut on interior spokes alone (see `SpokeProbe`).
// Inputs: the piece's closed soup, node positions, the components in play, the classifier, the
//   nodes each component's surface is known to pass through, whether the piece came from a split.
// Returns: nothing; accumulates into `probe`.
// Side effects: Mutates `probe` only. **Never touches the mesh** - the fan tets are not built,
//   because a fan tet is exactly one boundary triangle plus the piece centroid, so its four sides
//   are computable from the soup alone. That is what keeps this probe unable to change output.
// Notes: The side rule is `split_soup_by_surface`'s, deliberately: a node the surface is known to
//   pass through has no side, and so does a node whose predicate answer comes back uncertain -
//   which is the geometric statement of the same thing, and matters wherever two bodies are in
//   exact contact.
fn probe_spoke_cut(
    slab: &[[u32; 3]],
    nodes: &[Vec3],
    all_components: &[i32],
    classifier: &PointClassifier,
    on_surface: &BTreeMap<i32, BTreeSet<u32>>,
    from_split: bool,
    probe: &mut SpokeProbe,
) {
    if slab.is_empty() {
        return;
    }
    probe.pieces += 1;
    let centroid = polygon_soup_centroid(slab, nodes);
    for component in all_components {
        let Some(slot) = classifier.slot_of(*component) else {
            continue;
        };
        let empty = BTreeSet::new();
        let surface = on_surface.get(component).unwrap_or(&empty);
        // One classifier query per node, not per incidence: a node sits in about six of the
        // piece's triangles and the query is a five-ray parity test.
        let mut cache: BTreeMap<u32, Option<bool>> = BTreeMap::new();
        let mut side_of = |node: u32| -> Option<bool> {
            if let Some(hit) = cache.get(&node) {
                return *hit;
            }
            let side = if surface.contains(&node) {
                None
            } else {
                let mut uncertain = 0usize;
                let inside = classifier.inside(nodes[node as usize], slot, &mut uncertain);
                if uncertain > 0 {
                    None
                } else {
                    Some(inside)
                }
            };
            cache.insert(node, side);
            side
        };
        let (mut saw_inside, mut saw_outside) = (false, false);
        for triangle in slab {
            for node in triangle {
                match side_of(*node) {
                    Some(true) => saw_inside = true,
                    Some(false) => saw_outside = true,
                    None => {}
                }
            }
        }
        if !(saw_inside && saw_outside) {
            continue;
        }
        probe.straddling += 1;
        if from_split {
            probe.from_split += 1;
        }
        let apex = {
            let mut uncertain = 0usize;
            let inside = classifier.inside(centroid, slot, &mut uncertain);
            if uncertain > 0 {
                None
            } else {
                Some(inside)
            }
        };
        for triangle in slab {
            probe.fan_tets += 1;
            let sides = [
                side_of(triangle[0]),
                side_of(triangle[1]),
                side_of(triangle[2]),
            ];
            let base_inside = sides.contains(&Some(true));
            let base_outside = sides.contains(&Some(false));
            let has_inside = base_inside || apex == Some(true);
            let has_outside = base_outside || apex == Some(false);
            if !(has_inside && has_outside) {
                continue;
            }
            probe.straddling_tets += 1;
            if base_inside && base_outside {
                probe.base_straddle += 1;
            } else {
                probe.apex_only += 1;
            }
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Does §7.4's constrained triangulation of one face differ from the frozen §5.2 one?
// Inputs: the face's key-sorted nodes, its trace, the crossing nodes on it, the cut/on-cut indices,
//   the components, the node table and keys, and the arena quantum.
// Returns: true when the two differ, so the face has to be delivered as an augmented one.
// Side effects: None - a local arena, discarded.
// Notes: **This is the difference between 172,691 cells and the ones that actually need §7.4.** The
//   first version marked a face augmented wherever a trace existed, which forced every cell the
//   surface merely GRAZES onto the fan: 44,324 of a8's declines were "no facet in this cell", a
//   cell with nothing for §7.4 to constrain that should simply take §6's table as it does today.
//   A grazing contact whose trace is exactly §5.2's chord changes nothing and constrains nobody.
//
//   Two ways to differ, and the first is decisive on its own: if the constrained triangulation
//   needs a vertex the face does not already carry, no §5.2 row can produce it. Otherwise both are
//   triangulations of the same vertex set and the triangles are compared directly.
//
//   §5.2 is reconstructed here rather than borrowed from `face_states`, which needs a whole cell.
//   The scoping rule is that function's: a face cut by more than one component is not expressible
//   by the frozen table at all, so it counts as changed without further inspection.
#[allow(clippy::too_many_arguments)]
fn face_needs_augmenting(
    face: [u32; 3],
    trace: &[[Vec3; 2]],
    crossings: &[u32],
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    second_index: &BTreeMap<([u32; 2], i32), u32>,
    on_cut: &BTreeMap<(u32, i32), ()>,
    all_components: &[i32],
    nodes: &[Vec3],
    keys: &[NodeKey],
    quantum: f64,
) -> bool {
    let geometry = [
        nodes[face[0] as usize],
        nodes[face[1] as usize],
        nodes[face[2] as usize],
    ];
    let mut ids: Vec<u32> = face.to_vec();
    for id in crossings {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    let mut arena = crate::meshgen::cdt::NodeArena::new(
        ids.iter().map(|id| nodes[*id as usize]).collect(),
        quantum,
    );
    let before = arena.points.len();
    let mut segments: Vec<[u32; 2]> = Vec::new();
    for chord in trace {
        let a = arena.intern(chord[0]);
        let b = arena.intern(chord[1]);
        if a != b {
            segments.push([a, b]);
        }
    }
    if arena.points.len() != before {
        // The trace wants a vertex the face has not got; no §5.2 row can produce that.
        return true;
    }
    let Ok(mine) = crate::meshgen::cdt::constrained_face_triangulation(
        geometry,
        &arena.points,
        &arena.keys,
        &segments,
    ) else {
        // If §7.4 cannot triangulate the face there is nothing to deliver, so nothing changes.
        return false;
    };

    // §5.2's answer for the same face.
    let Some(frozen) = frozen_face_tris(face, cut_index, second_index, on_cut, all_components, keys)
    else {
        // Not expressible by the frozen table: the face is a junction face and §7.4's
        // triangulation is the only one on offer.
        return true;
    };
    let canon = |tris: &[[u32; 3]]| -> Vec<[u32; 3]> {
        let mut out: Vec<[u32; 3]> = tris
            .iter()
            .map(|t| {
                let mut k = *t;
                k.sort_unstable();
                k
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    };
    let mine_global: Vec<[u32; 3]> = mine
        .iter()
        .map(|t| [ids[t[0] as usize], ids[t[1] as usize], ids[t[2] as usize]])
        .collect();
    canon(&mine_global) != canon(&frozen)
}

// AI-FUNC-SUMMARY:
// Purpose: §5.2's frozen triangulation of one face, reconstructed from the face alone.
// Inputs: the face's key-sorted nodes, the cut/second/on-cut indices, the components and keys.
// Returns: the triangles, or None where the frozen table does not cover the face.
// Side effects: None.
// Notes: **A face §7.4 does not augment is still not always a bare triangle** - §5.2 splits it
//   wherever its edges carry crossings, and a cell handed the bare triangle instead presents a face
//   its neighbour has split in two. That was 75,797 boundary leaks on a6a's first emitting run: the
//   augmented faces were right and the *untouched* ones were wrong.
//
//   `face_states` builds the same thing but needs a whole cell. The scoping is that function's: a
//   face cut by more than one component is not expressible by the frozen table at all.
fn frozen_face_tris(
    face: [u32; 3],
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    second_index: &BTreeMap<([u32; 2], i32), u32>,
    on_cut: &BTreeMap<(u32, i32), ()>,
    all_components: &[i32],
    keys: &[NodeKey],
) -> Option<SmallVec<[[u32; 3]; 4]>> {
    let cutting: SmallVec<[i32; 2]> = all_components
        .iter()
        .copied()
        .filter(|component| {
            (0..3).any(|slot| {
                let (x, y) = (face[slot], face[(slot + 1) % 3]);
                let edge = if x <= y { [x, y] } else { [y, x] };
                cut_index.contains_key(&(edge, *component))
                    || second_index.contains_key(&(edge, *component))
            })
        })
        .collect();
    match cutting.len() {
        0 => Some(SmallVec::from_slice(&[face])),
        1 => {
            let component = cutting[0];
            let mut state = FaceCutState {
                nodes: face,
                ..Default::default()
            };
            for slot in 0..3 {
                let (x, y) = (face[slot], face[(slot + 1) % 3]);
                let edge = if x <= y { [x, y] } else { [y, x] };
                state.cut[slot] = cut_index.get(&(edge, component)).copied();
            }
            for slot in 0..3 {
                state.on_cut[slot] = on_cut.contains_key(&(face[slot], component));
            }
            face_split(&state, keys)
        }
        // More than one component cuts it, which §5.2 does not cover.
        _ => None,
    }
}

/// What §7.4 decided for one cell, in the mesh's own node ids.
enum PlcPlan {
    /// The constrained tetrahedralisation, with §7.5's sub-region per tet and the faces between
    /// sub-regions - the cell's material boundary.
    Meshed {
        tets: Vec<[u32; 4]>,
        regions: Vec<u32>,
        caps: Vec<([u32; 3], i32)>,
    },
    /// §7.4 declined, so the cell is fanned over the very same augmented boundary. That is what
    /// keeps it conforming with a neighbour that did take §7.4, and it is why widening §7.1's
    /// triage did not need a fixed point over the lattice (PLAN §6.30).
    Fan { boundary: Vec<[u32; 3]> },
}

/// One cell meshed by §7.2/§7.4, in the arena's own numbering until the caller maps it out.
struct PlcCell {
    /// The cell's boundary in GLOBAL ids - kept because a declined cell is fanned over exactly it.
    boundary: Vec<[u32; 3]>,
    /// Tets in LOCAL arena ids.
    tets: Vec<[u32; 4]>,
    /// The arena's points; ids below `seed.len()` are the boundary's, the rest are new.
    points: Vec<Vec3>,
    /// Global id per arena id below its length.
    seed: Vec<u32>,
    /// §7.5's sub-region per tet.
    regions: Vec<u32>,
    /// Faces separating two sub-regions, with the component whose facet they lie in - the cell's
    /// material boundary, in LOCAL ids.
    caps: Vec<([u32; 3], i32)>,
}

// AI-FUNC-SUMMARY:
// Purpose: Mesh one cell by `SPEC_meshgen_geometry.md` §7.2/§7.4 against a boundary it is given.
// Inputs: the cell's tet, its boundary triangulation in global ids, the node table, the components
//   and classifier, and a tolerance.
// Returns: the meshed cell, or the named reason it declined.
// Side effects: None - it interns into a LOCAL arena, so a decline leaves no node behind.
// Notes: **The boundary is an input, not something this derives.** It is built once per face from
//   the face and its own trace, so both cells sharing a face are handed identical node ids and
//   their meshes meet without either knowing the other exists (invariant J1). Deriving it per cell
//   is the mistake this phase recorded four times (PLAN §6.23).
//
//   A decline is not a failure of the cell: the caller fans it over this same boundary, which keeps
//   it conforming with a neighbour that did take §7.4. That is why the boundary comes back too.
fn plc_attempt(
    tet: [u32; 4],
    boundary: &[[u32; 3]],
    nodes: &[Vec3],
    all_components: &[i32],
    classifier: &PointClassifier,
    quantum: f64,
) -> Result<PlcCell, &'static str> {
    let corners = [
        nodes[tet[0] as usize],
        nodes[tet[1] as usize],
        nodes[tet[2] as usize],
        nodes[tet[3] as usize],
    ];
    let mut edge = f64::INFINITY;
    for a in 0..4 {
        for b in (a + 1)..4 {
            let d = corners[b].sub(corners[a]);
            edge = edge.min(d.dot(d).sqrt());
        }
    }
    if !edge.is_finite() || edge <= 0.0 {
        return Err("the cell is degenerate");
    }
    let tol = edge * 1.0e-9;

    let mut seed: Vec<u32> = boundary.iter().flatten().copied().collect();
    seed.sort_unstable();
    seed.dedup();
    let mut arena = crate::meshgen::cdt::NodeArena::new(
        seed.iter().map(|id| nodes[*id as usize]).collect(),
        quantum,
    );
    let local_of: BTreeMap<u32, u32> = seed
        .iter()
        .enumerate()
        .map(|(slot, id)| (*id, slot as u32))
        .collect();
    let boundary_local: Option<Vec<[u32; 3]>> = boundary
        .iter()
        .map(|t| {
            Some([
                *local_of.get(&t[0])?,
                *local_of.get(&t[1])?,
                *local_of.get(&t[2])?,
            ])
        })
        .collect();
    let Some(boundary_local) = boundary_local else {
        return Err("the boundary references a node outside the cell");
    };

    let mut facets: Vec<Vec<u32>> = Vec::new();
    let mut facet_of: Vec<i32> = Vec::new();
    for component in all_components {
        let Some(slot) = classifier.slot_of(*component) else { continue };
        for facet in crate::meshgen::cdt::fragment_facets_in_cell(
            corners,
            classifier.triangles_of(slot),
            edge * 1.0e-6,
        ) {
            let ids: Vec<u32> = facet.iter().map(|p| arena.intern(*p)).collect();
            // **A facet that collapses under the arena's quantum constrains nothing.** Interning
            // welds vertices closer than the quantum, so a sliver of surface arrives with three
            // points and leaves with two - below the mesh's own resolution, and not a reason to
            // refuse the cell.
            let mut distinct = ids.clone();
            distinct.sort_unstable();
            distinct.dedup();
            if distinct.len() < 3 {
                continue;
            }
            facets.push(ids);
            facet_of.push(*component);
        }
    }
    // **An empty facet list is not a refusal.** A cell whose faces the trace changed but whose
    // interior the surface never enters still has to be meshed against those faces, and a
    // constrained tetrahedralisation with no interior constraint is exactly that.
    let tets = crate::meshgen::cdt::constrained_tets(
        &arena.points,
        &arena.keys,
        &boundary_local,
        &facets,
        tol,
    )?;
    let regions = crate::meshgen::cdt::regions_by_constraint(&tets, &facets, &arena.points, tol);
    // **The material boundary is where two sub-regions meet, and nowhere else.** Not every facet
    // face is one: a facet with a free rim has the same region on both sides (§7.5), and tagging it
    // would declare an interface the mesh has no material change across - which is exactly what
    // `[V9]` is right to fail. So the interface is read off the regions and the component is read
    // off the facet the face lies in.
    let mut carried: BTreeMap<[u32; 3], SmallVec<[usize; 2]>> = BTreeMap::new();
    for (at, tet) in tets.iter().enumerate() {
        for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
            let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
            face.sort_unstable();
            carried.entry(face).or_default().push(at);
        }
    }
    let mut caps: Vec<([u32; 3], i32)> = Vec::new();
    for (face, on) in &carried {
        if on.len() != 2 || regions[on[0]] == regions[on[1]] {
            continue;
        }
        let p = [
            arena.points[face[0] as usize],
            arena.points[face[1] as usize],
            arena.points[face[2] as usize],
        ];
        let centre = p[0].add(p[1]).add(p[2]).scale(1.0 / 3.0);
        for (slot, facet) in facets.iter().enumerate() {
            if facet.len() < 3 {
                continue;
            }
            let corner = |at: usize| arena.points[facet[at] as usize];
            let mut normal = Vec3::new(0.0, 0.0, 0.0);
            for at in 1..facet.len() - 1 {
                normal = normal.add(corner(at).sub(corner(0)).cross(corner(at + 1).sub(corner(0))));
            }
            let length = normal.dot(normal).sqrt();
            if length <= 0.0 {
                continue;
            }
            let normal = normal.scale(1.0 / length);
            let offset = normal.dot(corner(0));
            if p.iter().any(|q| (normal.dot(*q) - offset).abs() > tol) {
                continue;
            }
            let _ = centre;
            caps.push((*face, facet_of[slot]));
            break;
        }
    }
    Ok(PlcCell {
        boundary: boundary.to_vec(),
        tets,
        points: arena.points,
        seed,
        regions,
        caps,
    })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn split_escalated_cell(
    index: usize,
    boundary: &[[u32; 3]],
    tet: [u32; 4],
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    second_index: &BTreeMap<([u32; 2], i32), u32>,
    on_cut: &BTreeMap<(u32, i32), ()>,
    all_components: &[i32],
    classifier: &PointClassifier,
    nodes: &[Vec3],
    keys: &[NodeKey],
    volume_tolerance: f64,
    meeting_nodes: &[(u32, [i32; 2])],
    face_owners: &BTreeMap<[u32; 3], (u16, u16)>,
) -> Option<(Vec<SmallVec<[[u32; 3]; 16]>>, Vec<([u32; 3], i32)>)> {
    let mut crossing: Vec<(i32, BTreeSet<u32>)> = Vec::new();
    for component in all_components {
        let mut here: BTreeSet<u32> = BTreeSet::new();
        for a in 0..4 {
            for b in (a + 1)..4 {
                let (x, y) = (tet[a], tet[b]);
                let edge = if x <= y { [x, y] } else { [y, x] };
                if let Some(node) = cut_index.get(&(edge, *component)) {
                    here.insert(*node);
                }
                if let Some(node) = second_index.get(&(edge, *component)) {
                    here.insert(*node);
                }
            }
        }
        // A face's meeting point is a point of the intersection curve, so it lies on
        // both of that face's surfaces. Without this the second component's split asks
        // the classifier for a side exactly on its own surface, gets whichever answer
        // the predicate rounds to, and the cap triangles report `mixed sides`.
        for (node, owners) in meeting_nodes {
            if owners.contains(component) {
                here.insert(*node);
            }
        }
        // **A surface that runs through the cell's *vertices* leaves no edge crossing**, so
        // it is absent from `cut_index` and this loop never sees it - and §7.6 then splits
        // the cell by every *other* surface and leaves a piece that body passes straight
        // through. The piece's fan tets are seeded one by one, so the material boundary
        // inside it becomes a chain of **fan faces**: untagged, off the surface, and at
        // element scale. That is the serration - 2,482 of A-6a's 2,628 undeclared
        // material-boundary faces lie between two §7.6 pieces.
        //
        // An `on_cut` vertex *is* the trace of that surface on the cell's boundary, which is
        // exactly what `split_soup_by_surface` needs: it reads sides from the classifier and
        // takes the loop where they change, and these nodes sit on that loop already. So the
        // cut needs no new geometry - only for the component to be named. Where the surface
        // does not in fact separate the piece, the existing `one side empty` guard declines
        // and nothing changes.
        for node in &tet {
            if on_cut.contains_key(&(*node, *component)) {
                here.insert(*node);
            }
        }
        if !here.is_empty() {
            crossing.push((*component, here));
        }
    }
    // `RUSTMSPT_NO_JCT_CUT=1` falls the whole cell back to the centroid fan, which is what
    // shipped before §7.6 was implemented - kept as a bisection handle, not as a default.
    if crossing.is_empty() || std::env::var_os("RUSTMSPT_NO_JCT_CUT").is_some() {
        if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
            println!("[JCT-DIAG] cell {index}: only {} crossing component(s)", crossing.len());
        }
        return None;
    }
    let parent_volume = fan_volume(boundary, polygon_soup_centroid(boundary, nodes), nodes);
    if parent_volume <= 0.0 {
        return None;
    }

    // **P-3.13 step 6, diagnostic only (`RUSTMSPT_PLC_DIAG=1`): what §7.4's constrained
    // tetrahedralisation would make of this cell.** Nothing is consumed - the result is measured
    // and dropped - because the caller consumes region SOUPS and this produces TETS, and
    // restructuring that before knowing the acceptance rate would be building for a population
    // whose size is unmeasured. The refusal reasons are the point: "the boundary is not the frozen
    // one" says the face's own triangulation has to carry the trace first, and "a facet is not a
    // union of faces" says a recovery machine is needed. Those are different pieces of work and
    // this is what says which one.
    if std::env::var_os("RUSTMSPT_PLC_DIAG").is_some() {
        let corners = [
            nodes[tet[0] as usize],
            nodes[tet[1] as usize],
            nodes[tet[2] as usize],
            nodes[tet[3] as usize],
        ];
        let mut edge = f64::INFINITY;
        for a in 0..4 {
            for b in (a + 1)..4 {
                let d = corners[b].sub(corners[a]);
                edge = edge.min(d.dot(d).sqrt());
            }
        }
        let mut local: Vec<u32> = boundary.iter().flatten().copied().collect();
        local.sort_unstable();
        local.dedup();
        let quantum = volume_tolerance.max(f64::MIN_POSITIVE) * 1.0e-6;
        let mut arena = crate::meshgen::cdt::NodeArena::new(
            local.iter().map(|id| nodes[*id as usize]).collect(),
            quantum,
        );
        let to_local: BTreeMap<u32, u32> = local
            .iter()
            .enumerate()
            .map(|(slot, id)| (*id, slot as u32))
            .collect();
        let boundary_local: Option<Vec<[u32; 3]>> = boundary
            .iter()
            .map(|t| {
                Some([
                    *to_local.get(&t[0])?,
                    *to_local.get(&t[1])?,
                    *to_local.get(&t[2])?,
                ])
            })
            .collect();
        let before = arena.points.len();
        let mut facets_local: Vec<Vec<u32>> = Vec::new();
        for component in all_components {
            let Some(slot) = classifier.slot_of(*component) else { continue };
            for facet in crate::meshgen::cdt::fragment_facets_in_cell(
                corners,
                classifier.triangles_of(slot),
                edge * 1.0e-6,
            ) {
                facets_local.push(facet.iter().map(|p| arena.intern(*p)).collect());
            }
        }
        let outcome = match boundary_local {
            None => "the boundary references a node outside the cell".to_string(),
            Some(_) if facets_local.is_empty() => {
                "no facet in this cell".to_string()
            }
            Some(boundary_local) => match crate::meshgen::cdt::constrained_tets(
                &arena.points,
                &arena.keys,
                &boundary_local,
                &facets_local,
                edge * 1.0e-9,
            ) {
                Ok(tets) => format!("TAKEN {} tet(s)", tets.len()),
                Err(reason) => reason.to_string(),
            },
        };
        // **And the same question again with the face side supplied.** The frozen triangulation is
        // rebuilt per face by `constrained_face_triangulation`, given that face's own trace as
        // constraints - which is exactly what §7.3's cache would deliver to both cells. Measured
        // beside the unaugmented answer so the difference is the face side's worth, and nothing
        // else's. Simulated rather than wired, because the cache is shared state and this is a
        // per-cell diagnostic; what it measures is whether wiring it converts the refusals.
        let augmented: Result<Vec<[u32; 3]>, &'static str> = (|| {
            let Some(halfspaces) = crate::meshgen::cdt::tet_halfspaces_of(corners) else {
                return Err("the cell is degenerate");
            };
            let mut out: Vec<[u32; 3]> = Vec::new();
            for (slot, (normal, offset)) in halfspaces.iter().enumerate() {
                let face_corners: Vec<Vec3> =
                    (0..4).filter(|s| *s != slot).map(|s| corners[s]).collect();
                let face = [face_corners[0], face_corners[1], face_corners[2]];
                let on_face = |id: u32| -> bool {
                    (normal.dot(arena.points[id as usize]) - offset).abs() <= edge * 1.0e-9
                };
                // This face's share of the frozen soup, and every vertex it already carries.
                let mut ids: Vec<u32> = Vec::new();
                let mut carried = 0usize;
                for triangle in boundary {
                    let local: Vec<u32> =
                        triangle.iter().filter_map(|id| to_local.get(id).copied()).collect();
                    if local.len() != 3 || !local.iter().all(|id| on_face(*id)) {
                        continue;
                    }
                    carried += 1;
                    for id in local {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
                if carried == 0 {
                    continue;
                }
                // The face's own trace, which both cells compute identically from the face alone.
                let mut segments: Vec<[u32; 2]> = Vec::new();
                for component in all_components {
                    let Some(cslot) = classifier.slot_of(*component) else { continue };
                    for chord in crate::meshgen::cdt::trace_on_face(
                        face,
                        classifier.triangles_of(cslot),
                        edge * 1.0e-9,
                    ) {
                        let a = arena.intern(chord[0]);
                        let b = arena.intern(chord[1]);
                        if a == b {
                            continue;
                        }
                        for id in [a, b] {
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                        }
                        segments.push([a, b]);
                    }
                }
                let points: Vec<Vec3> = ids.iter().map(|id| arena.points[*id as usize]).collect();
                let keys: Vec<NodeKey> = ids.iter().map(|id| arena.keys[*id as usize]).collect();
                let index = |id: u32| ids.iter().position(|x| *x == id).unwrap_or(0) as u32;
                let local_segments: Vec<[u32; 2]> =
                    segments.iter().map(|s| [index(s[0]), index(s[1])]).collect();
                let tris = match crate::meshgen::cdt::constrained_face_triangulation(
                    face,
                    &points,
                    &keys,
                    &local_segments,
                ) {
                    Ok(tris) => tris,
                    Err(reason) => return Err(reason),
                };
                for t in tris {
                    out.push([ids[t[0] as usize], ids[t[1] as usize], ids[t[2] as usize]]);
                }
            }
            Ok(out)
        })();
        // **How much of the augmented boundary actually DIFFERS from the frozen one, and who owns
        // those faces.** In the ordinary case the surface's trace on a face is the very chord
        // §5.2's table already draws between that face's two crossings, so the augmented
        // triangulation is the frozen one and there is nothing to deliver. It differs only where
        // the trace is something the table cannot express - a loop strictly inside the face, or a
        // polyline with interior vertices because several surface triangles cross it. That much
        // smaller set is what decides whether §7.1's triage can stay as frozen.
        let mut faces_changed = 0usize;
        let mut faces_changed_shared_with_unescalated = 0usize;
        if let Ok(augmented) = &augmented {
            let canon = |t: &[u32; 3]| {
                let mut k = *t;
                k.sort_unstable();
                k
            };
            let frozen_here: std::collections::BTreeSet<[u32; 3]> = boundary
                .iter()
                .filter_map(|t| {
                    Some(canon(&[
                        *to_local.get(&t[0])?,
                        *to_local.get(&t[1])?,
                        *to_local.get(&t[2])?,
                    ]))
                })
                .collect();
            let augmented_here: std::collections::BTreeSet<[u32; 3]> =
                augmented.iter().map(canon).collect();
            if frozen_here != augmented_here {
                // Which of the cell's four lattice faces the difference falls on.
                if let Some(halfspaces) = crate::meshgen::cdt::tet_halfspaces_of(corners) {
                    for (slot, (normal, offset)) in halfspaces.iter().enumerate() {
                        let on_face = |t: &[u32; 3]| {
                            t.iter().all(|id| {
                                (normal.dot(arena.points[*id as usize]) - offset).abs()
                                    <= edge * 1.0e-9
                            })
                        };
                        let a: std::collections::BTreeSet<[u32; 3]> =
                            frozen_here.iter().filter(|t| on_face(t)).copied().collect();
                        let b: std::collections::BTreeSet<[u32; 3]> =
                            augmented_here.iter().filter(|t| on_face(t)).copied().collect();
                        if a == b {
                            continue;
                        }
                        faces_changed += 1;
                        let mut key = [tet[0]; 3];
                        for (at, s) in (0..4).filter(|s| *s != slot).enumerate() {
                            key[at] = tet[s];
                        }
                        key.sort_by_key(|node| keys[*node as usize]);
                        if let Some((owners, escalated)) = face_owners.get(&key) {
                            if owners != escalated {
                                faces_changed_shared_with_unescalated += 1;
                            }
                        }
                    }
                }
            }
        }
        let with_face = match (&augmented, facets_local.is_empty()) {
            (Err(reason), _) => format!("the face side refused: {reason}"),
            (Ok(_), true) => "no facet in this cell".to_string(),
            (Ok(boundary_local), false) => match crate::meshgen::cdt::constrained_tets(
                &arena.points,
                &arena.keys,
                boundary_local,
                &facets_local,
                edge * 1.0e-9,
            ) {
                Ok(tets) => format!("TAKEN {} tet(s)", tets.len()),
                Err(reason) => reason.to_string(),
            },
        };
        // **A new vertex ON the cell's boundary and one strictly inside it are different
        // problems.** An interior one is a rim vertex: private to this cell, and no obstacle at
        // all. One on a shared face is the surface's trace crossing that face, and the frozen
        // triangulation does not contain it - which is the face-side work, and the only thing that
        // work fixes. Counting them together would credit the face side with rim vertices it has
        // nothing to do with.
        let mut on_boundary = 0usize;
        let mut interior = 0usize;
        if let Some(halfspaces) = crate::meshgen::cdt::tet_halfspaces_of(corners) {
            for id in before..arena.points.len() {
                let point = arena.points[id];
                if halfspaces
                    .iter()
                    .any(|(normal, offset)| (normal.dot(point) - offset).abs() <= edge * 1.0e-9)
                {
                    on_boundary += 1;
                } else {
                    interior += 1;
                }
            }
        }
        println!(
            "[PLC] cell {index}: {} facet(s), {on_boundary} new on the boundary, {interior} new \
             inside, {faces_changed} face(s) changed of which \
             {faces_changed_shared_with_unescalated} shared with an unescalated cell, frozen: \
             {outcome} | with the face side: {with_face}",
            facets_local.len()
        );
    }

    // **P-3.13 step 4: an escalated cell is cut by its own surface FRAGMENT first.** §6.24's
    // measurements fixed the order - §7.2's mesher has to replace the centroid fan for escalated
    // cells before the sub-cell population is escalated into it - and this is the first half. It
    // differs from the handle below in one thing: the planes come from the surface clipped to this
    // cell rather than from its edge crossings, which is §7.2's stated input and the only one that
    // can express a body crossing no edge.
    //
    // **Default, not a handle, because it is strictly better or identical on all nine cases.**
    // a8 92.011 -> 92.038 %, a3 92.743 -> 92.784 %, the other seven identical to the digit, for
    // +52 elements across the whole suite (0.001 %) with `[V1]`/`[V3]`/`[V9]` PASS throughout and
    // `undecl` down on both cases that moved. R3 forbids a switch that chooses between a better
    // mesh and a worse one, which is exactly what leaving this gated would be. It costs a8 12.8 %
    // wall time - `fragment_in_cell` scans the component per cell - and that is an index away from
    // being free, but an optimisation rather than a correctness question.
    //
    // Every refusal is the wrapper's, so a cell it turns down still gets the soup split below;
    // 86-90 % of them are the one refusal the face side removes (§6.25).
    {
        let corners = [
            nodes[tet[0] as usize],
            nodes[tet[1] as usize],
            nodes[tet[2] as usize],
            nodes[tet[3] as usize],
        ];
        // The clip's area threshold in the cell's own units: a contact below 1e-12 of the cell's
        // cross-section is the measure-zero touch `fragment_in_cell` exists to drop, and an
        // absolute tolerance would mean something different at every element size.
        let mut edge = f64::INFINITY;
        for a in 0..4 {
            for b in (a + 1)..4 {
                let d = corners[b].sub(corners[a]);
                edge = edge.min(d.dot(d).sqrt());
            }
        }
        let mut fragment: Vec<(i32, Vec<[Vec3; 3]>)> = Vec::new();
        for component in all_components {
            let Some(slot) = classifier.slot_of(*component) else { continue };
            let tris = crate::meshgen::cdt::fragment_in_cell(
                corners,
                classifier.triangles_of(slot),
                edge * 1.0e-6,
            );
            if !tris.is_empty() {
                fragment.push((*component, tris));
            }
        }
        let taken = if fragment.is_empty() {
            Err("no fragment in this cell")
        } else {
            crate::meshgen::cdt::subdivide_cell_by_fragment(
                boundary,
                &fragment,
                nodes,
                volume_tolerance.max(f64::MIN_POSITIVE) * 1.0e-6,
                1.0e-9,
            )
        };
        // The refusal rate PER REASON is what says which capability is still missing, so it is
        // reported as the reason rather than as a count of failures.
        let mut outcome = match &taken {
            Err(reason) => format!("REFUSED {reason}"),
            Ok(_) => String::new(),
        };
        let mut accepted = None;
        if let Ok(subdivision) = taken {
            let pieces: Vec<SmallVec<[[u32; 3]; 16]>> = subdivision
                .pieces
                .iter()
                .map(|soup| soup.iter().copied().collect())
                .collect();
            // The same guard §6 puts on every cut: the pieces must add up to the parent. Reported
            // separately from the wrapper's refusals, because "the kernel produced a subdivision
            // that does not conserve volume" and "the kernel declined" are different failures and
            // conflating them is how a gate reads as working when nothing takes it.
            let total: f64 = pieces
                .iter()
                .map(|piece| fan_volume(piece, polygon_soup_centroid(piece, nodes), nodes))
                .sum();
            if (total - parent_volume).abs() <= volume_tolerance * parent_volume {
                outcome = format!("TAKEN {} piece(s)", pieces.len());
                accepted = Some((pieces, subdivision.caps));
            } else {
                outcome = format!(
                    "VOLUME {} piece(s) sum {:.6e} against {:.6e}",
                    pieces.len(),
                    total,
                    parent_volume
                );
            }
        }
        if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
            let planes: usize = fragment
                .iter()
                .map(|(_, tris)| {
                    crate::meshgen::cdt::planes_from_fragment(
                        tris,
                        volume_tolerance.max(f64::MIN_POSITIVE) * 1.0e-6,
                    )
                    .len()
                })
                .sum();
            // An edge crossed MORE than twice was the obvious explanation for a mesher wanting a
            // node at a point the surface really does reach - `cut_index` and `second_index` hold
            // at most two crossings per (edge, component), so a third would have no node at all.
            // Measured and refuted: ZERO of a8's 4,377 escalated cells have such an edge. The probe
            // and the parameter it needed were removed once it had answered.
            println!(
                "[CDT-FRAG] cell {index}: {} component(s) {planes} plane(s) in the fragment, \
                 {outcome}",
                fragment.len()
            );
        }
        if let Some(out) = accepted {
            return Some(out);
        }
    }

    // **P-3 gate handle (`RUSTMSPT_CDT=1`): route the cell through the §7.2-§7.4 kernel.**
    // Not a setting - which mesher a cell gets is not a matter of taste (R3) - and it must not
    // survive the gate. It sits here rather than replacing the path below because the kernel
    // declines any cell that would need a node the neighbour does not have, so the soup split
    // stays the answer for everything it turns down.
    if std::env::var_os("RUSTMSPT_CDT").is_some() {
        let cut_nodes: Vec<(i32, Vec<u32>)> = crossing
            .iter()
            .map(|(component, set)| (*component, set.iter().copied().collect()))
            .collect();
        if let Ok(subdivision) = crate::meshgen::cdt::subdivide_cell(
            boundary,
            &cut_nodes,
            nodes,
            volume_tolerance.max(f64::MIN_POSITIVE) * 1.0e-6,
            1.0e-9,
        ) {
            let pieces: Vec<SmallVec<[[u32; 3]; 16]>> = subdivision
                .pieces
                .iter()
                .map(|soup| soup.iter().copied().collect())
                .collect();
            // The same guard §6 puts on every cut: the pieces must add up to the parent.
            let total: f64 = pieces
                .iter()
                .map(|piece| fan_volume(piece, polygon_soup_centroid(piece, nodes), nodes))
                .sum();
            if (total - parent_volume).abs() <= volume_tolerance * parent_volume {
                if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
                    println!(
                        "[CDT] cell {index}: {} piece(s), {} cap triangle(s)",
                        pieces.len(),
                        subdivision.caps.len()
                    );
                }
                return Some((pieces, subdivision.caps));
            }
        }
    }

    let mut pieces: Vec<SmallVec<[[u32; 3]; 16]>> = vec![boundary.iter().copied().collect()];
    let mut caps: Vec<([u32; 3], i32)> = Vec::new();
    for (component, surface) in &crossing {
        let slot = classifier.slot_of(*component)?;
        let mut split_any = false;
        let mut next: Vec<SmallVec<[[u32; 3]; 16]>> = Vec::new();
        for piece in &pieces {
            let outcome = {
                let side_of = |node: u32| -> Option<bool> {
                    if surface.contains(&node) {
                        return None;
                    }
                    // An **uncertain** answer means the point is on the surface as far as the
                    // predicate can tell, and taking it as a side is how a triangle ends up
                    // with corners on both sides and no cut node between them - the
                    // `mixed sides` refusal. It is the same statement `surface` makes for a
                    // cut node, arrived at geometrically instead of combinatorially, and it
                    // matters wherever two bodies are in exact contact: a node on the shared
                    // plane is genuinely on both surfaces.
                    let mut uncertain = 0usize;
                    let inside = classifier.inside(nodes[node as usize], slot, &mut uncertain);
                    if uncertain > 0 {
                        return None;
                    }
                    Some(inside)
                };
                let side_of_face = |triangle: &[u32; 3]| -> Option<bool> {
                    let centre = nodes[triangle[0] as usize]
                        .add(nodes[triangle[1] as usize])
                        .add(nodes[triangle[2] as usize])
                        .scale(1.0 / 3.0);
                    let mut uncertain = 0usize;
                    Some(classifier.inside(centre, slot, &mut uncertain))
                };
                split_soup_by_surface(piece, &side_of, &side_of_face)
            };
            let Some((inside, outside, cycles)) = outcome else {
                if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
                    println!("[JCT-DIAG] cell {index}: piece not split by component {component}");
                }
                next.push(piece.clone());
                continue;
            };
            // Every cap this surface leaves on the piece. One where it passes through
            // once; two where it passes through twice, which is a lattice cell straddling
            // a thin plate - the material between the walls is bounded by both of them.
            let mut all_caps: SmallVec<[[u32; 3]; 16]> = SmallVec::new();
            // **P-3.9: what are cap loops made of?** §7.6's split partitions whole triangles, so a
            // cap can only follow edges that already exist — and P-3.8 measured the consequence:
            // 93.7 % of a8's structural residue stands on lattice corners, half of it inside a
            // single parent cell where a cap's corners should be on the surface by construction.
            // This counts the loop nodes directly. A cut node is on the surface by root-finding; a
            // lattice node is only on it if S7 snapped it there. The ratio says whether the fix is
            // small (loops are nearly all cut nodes, with occasional strays) or structural (loops
            // are routinely forced onto the lattice).
            for cycle in &cycles {
                for node in cycle {
                    // `surface` is what §7.6 considers on this component: its cut nodes, the
                    // meeting points, and the vertices S7 put on the patch. A loop node outside
                    // that set is one the split had no on-surface reason to route through.
                    if surface.contains(node) {
                        CAP_LOOP_INTERNED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        CAP_LOOP_LATTICE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
            for cycle in &cycles {
                // The cap is this component's surface inside the cell, and the *other*
                // surface crosses it along the intersection curve - through the meeting
                // points, which sit on this cap's own boundary. Coning the cap to a fresh
                // centre would put a triangle across that curve, and the next component's
                // split then reports `mixed sides` on it and declines - which is why the
                // cell came out in two pieces instead of four. Splitting the cap along its
                // two meeting points instead keeps the curve an edge, needs no new node, and
                // lets the second cut through.
                let stops: SmallVec<[usize; 2]> = cycle
                    .iter()
                    .enumerate()
                    .filter(|(_, node)| {
                        meeting_nodes
                            .iter()
                            .any(|(meeting, owners)| meeting == *node && owners.contains(component))
                    })
                    .map(|(slot, _)| slot)
                    .collect();
                let cap: SmallVec<[[u32; 3]; 8]> = if stops.len() == 2 && cycle.len() >= 4 {
                    let (lo, hi) = (stops[0], stops[1]);
                    let near: SmallVec<[u32; 8]> = cycle[lo..=hi].iter().copied().collect();
                    let mut far: SmallVec<[u32; 8]> = cycle[hi..].iter().copied().collect();
                    far.extend(cycle[..=lo].iter().copied());
                    let mut both = fan_polygon(&near, keys);
                    both.extend(fan_polygon(&far, keys));
                    both
                } else if cycle.len() == 3 {
                    // A triangular cap needs no Steiner point, and interning one would put
                    // a node on the surface that no neighbour shares.
                    let mut one: SmallVec<[[u32; 3]; 8]> = SmallVec::new();
                    one.push([cycle[0], cycle[1], cycle[2]]);
                    one
                } else {
                    // A fan from one of the cap's own vertices, *not* a fresh centre. A
                    // centre is a node only this cell knows about, and a cap can be
                    // coplanar with one of the cell's four faces - invariant K1's case,
                    // where a surface enters and leaves through the same face - so the
                    // centre lands on that face and the neighbour across it, which
                    // triangulated the face without it, grows a hanging node. That was all
                    // 180 left on A-4, 871 on A-6a and 4,417 on A-6b, every one a cap
                    // centre. Testing the centre against the face planes does *not* find
                    // them: the centre is an average, so it is only numerically on the
                    // plane and the exact predicate rightly says otherwise. Removing the
                    // node removes the question. The apex is chosen by key, so the cap is
                    // still a pure function of its cycle and both pieces agree on it.
                    let Some(fan) = fan_cap(cycle, keys, nodes, &|triangle| {
                        cap_lies_in_parent_face(triangle, tet, nodes)
                    }) else {
                        if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
                            println!("[JCT-DIAG] cell {index}: no apex fans this cap cleanly");
                        }
                        return None;
                    };
                    fan
                };
                if !fan_is_simple(&cap, nodes) {
                    if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
                        println!("[JCT-DIAG] cell {index}: a cap does not fan without folding over itself");
                    }
                    return None;
                }
                for triangle in &cap {
                    caps.push((*triangle, *component));
                }
                all_caps.extend(cap.iter().copied());
            }
            // Both sides close over the *same* triangles - the pieces share the cut, and
            // `orient_positively` gives each fan tet its own winding afterwards. Each side
            // is then separated into its edge-connected parts: with two walls the outside
            // is two disjoint solids, one before the near wall and one past the far one.
            // A cap triangle the piece already carries is not added again - and "carries"
            // means anywhere in the piece, not just on the side being closed. Where two
            // components' surfaces **coincide**, the second one's cap is built over the same
            // loop as the first's and comes out as the very same triangles; adding them to
            // the side that does not already hold them leaves a flap hanging off the piece
            // by two triangles, open along three edges. That is A-6a's 115 unclosed pieces,
            // read straight off one cell's soup: caps `[42196, 27550, 27551]` and
            // `[42196, 27551, 42200]` appear under **both** components, and piece 2 carries
            // both plus 27550/27551, which appear nowhere else in it.
            // Testing only the side being closed is what missed them, and it is also why
            // three earlier variants measured exactly neutral: deduplicating after
            // `split_soup_components`, and dropping the side's copy instead of the cap's,
            // both ask the same too-narrow question one stage later.
            let sorted = |triangle: &[u32; 3]| -> [u32; 3] {
                let mut key = *triangle;
                key.sort_unstable();
                key
            };
            let held: BTreeSet<[u32; 3]> = piece.iter().map(sorted).collect();
            // A component **any** of whose cap triangles is already in the piece does not cut
            // it: its surface coincides, in part or in whole, with one already cut through
            // here. Re-cutting along a wall that is already a face of this piece claims the
            // same material twice - measured directly, parent 4.779550e-10 against pieces
            // summing to 4.893833e-10, a real 2.4 % overlap and not a measurement artefact
            // (all three pieces orient cleanly and `soup_volume` agrees with the cone sum).
            // Requiring the *whole* cap to be present was the first version and it missed
            // exactly these: a partly coincident surface contributes one genuinely new cap
            // triangle alongside two it shares, so the test passed and the overlap went to
            // the volume guard, which rejected the cell outright. Widening it to `any` takes
            // A-6a's §7.6 cuts 189 -> 414 and its `[V6]` 679 -> 670, A-6b 3,285 -> 3,532 and
            // 1,037 -> 1,021, and empties the volume guard's rejections entirely.
            if !all_caps.is_empty()
                && all_caps.iter().any(|triangle| held.contains(&sorted(triangle)))
            {
                if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
                    println!("[JCT-DIAG] cell {index}: component {component} coincides with a cut already made");
                }
                next.push(piece.clone());
                continue;
            }
            let mut closed_inside: SmallVec<[[u32; 3]; 16]> = inside;
            closed_inside.extend(
                all_caps
                    .iter()
                    .filter(|triangle| !held.contains(&sorted(triangle)))
                    .copied(),
            );
            let mut closed_outside: SmallVec<[[u32; 3]; 16]> = outside;
            closed_outside.extend(
                all_caps
                    .iter()
                    .filter(|triangle| !held.contains(&sorted(triangle)))
                    .copied(),
            );
            next.extend(split_soup_components(&closed_inside));
            next.extend(split_soup_components(&closed_outside));
            split_any = true;
        }
        if split_any {
            pieces = next;
        }
    }
    if pieces.len() < 2 {
        return None;
    }

    let volumes: Vec<f64> = pieces
        .iter()
        .map(|piece| {
            soup_volume(piece, nodes)
                .unwrap_or_else(|| fan_volume(piece, polygon_soup_centroid(piece, nodes), nodes))
        })
        .collect();
    // A piece far smaller than the cell it came from is a sliver, and a sliver is
    // exactly where seeding a piece's ownership from an interior sample is least
    // reliable: its fan tets are thin, their centroids sit within the envelope of a
    // neighbouring surface, and `seed_record` can hand the sliver to the wrong side.
    // Measured on A-7a, whose plates are one sheet-thickness apart: splitting its 512
    // junction cells moved the volume error from 0.36/0.10 % to 0.57/0.44 %, while on
    // A-3, whose pieces are all substantial, the same code is neutral. Nothing is given
    // up by declining - the fan's chamfer on such a cell is bounded by the sliver.
    // Every piece must be a closed surface. A piece with a hole cannot be fanned without
    // leaving the volume open along it, which is exactly `[V3]`'s boundary leaks and
    // interface cracks - measured as the pair `(c1, c2, S1)` tagged and `(c1, c2, S2)` not,
    // two pieces coning the same cap to their own Steiner centres.
    // A cap triangle can attach to the wrong piece. Both sides are closed with the same
    // cap and then separated by edge connectivity, so a cap triangle that happens to share
    // nodes with another piece is pulled into it and hangs there as a flap - while the piece
    // that actually needed it is left open by the same two triangles. Read off one A-6a cell:
    // piece 1 is short of `[42196, 27550, 27551]` and `[42196, 27551, 42200]` by four odd
    // edges, and piece 2 carries exactly those two with nodes 27550/27551 appearing nowhere
    // else in it. A flap is precisely a triangle whose removal leaves the piece closed, so
    // peel dangling triangles off and keep the result only when it closes. Eroding a
    // genuinely open piece cannot make things worse: it stays open and the caller declines,
    // as it did before.
    let mut orphans: SmallVec<[[u32; 3]; 8]> = SmallVec::new();
    for piece in pieces.iter_mut() {
        if soup_is_closed(piece) {
            continue;
        }
        let mut peeled: SmallVec<[[u32; 3]; 16]> = piece.clone();
        loop {
            let mut count: BTreeMap<[u32; 2], usize> = BTreeMap::new();
            for triangle in peeled.iter() {
                for slot in 0..3 {
                    let (a, b) = (triangle[slot], triangle[(slot + 1) % 3]);
                    *count.entry(if a <= b { [a, b] } else { [b, a] }).or_insert(0) += 1;
                }
            }
            let before = peeled.len();
            peeled.retain(|triangle| {
                !(0..3).any(|slot| {
                    let (a, b) = (triangle[slot], triangle[(slot + 1) % 3]);
                    count[&if a <= b { [a, b] } else { [b, a] }] == 1
                })
            });
            if peeled.len() == before || peeled.is_empty() {
                break;
            }
        }
        if !peeled.is_empty() && soup_is_closed(&peeled) {
            let kept: BTreeSet<[u32; 3]> = peeled
                .iter()
                .map(|triangle| {
                    let mut key = *triangle;
                    key.sort_unstable();
                    key
                })
                .collect();
            for triangle in piece.iter() {
                let mut key = *triangle;
                key.sort_unstable();
                if !kept.contains(&key) {
                    orphans.push(*triangle);
                }
            }
            *piece = peeled;
        }
    }
    // The flap belongs to whichever piece its loop was cut from, and that piece is open by
    // exactly it. Hand the peeled triangles to the open pieces rather than dropping them:
    // dropping alone rescues nothing, because the piece that needed them is still open.
    if !orphans.is_empty() {
        for piece in pieces.iter_mut() {
            if soup_is_closed(piece) {
                continue;
            }
            let mut mended: SmallVec<[[u32; 3]; 16]> = piece.clone();
            mended.extend(orphans.iter().copied());
            if soup_is_closed(&mended) {
                *piece = mended;
            }
        }
    }
    if !pieces.iter().all(|piece| soup_is_closed(piece)) {
        if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
            let open: Vec<usize> = pieces
                .iter()
                .map(|piece| {
                    let mut count: BTreeMap<[u32; 2], usize> = BTreeMap::new();
                    for triangle in piece.iter() {
                        for slot in 0..3 {
                            let (a, b) = (triangle[slot], triangle[(slot + 1) % 3]);
                            let key = if a <= b { [a, b] } else { [b, a] };
                            *count.entry(key).or_insert(0) += 1;
                        }
                    }
                    count.values().filter(|n| **n % 2 == 1).count()
                })
                .collect();
            println!("[JCT-DIAG] cell {index}: a piece is not closed: {} piece(s), odd edges {:?}", pieces.len(), open);
        }
        return None;
    }
    // Closed is not enough: the piece has to *fan*. A boundary triangle coplanar with
    // the piece's own centroid gives a flat tet, and a flat tet is dropped rather than
    // emitted - which punches a hole through a soup the check above just called closed.
    // That drop is not a rounding detail, it is the defect: 588 dropped pieces against
    // 588 interface cracks on A-4, exactly one for one. Declining the split here costs
    // the cell its cut and nothing else, because the fallback fan is unconditionally
    // valid; dropping the tet costs conformity, which is not recoverable downstream.
    if !pieces.iter().all(|piece| fan_is_sound(piece, nodes)) {
        if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
            println!("[JCT-DIAG] cell {index}: a piece does not fan without a flat tet");
        }
        return None;
    }
    // A cap that lands **on** the cell's own outer boundary is a face the neighbour also
    // fans, so it ends up carrying four tets - `[V3]`'s `multi_shared_face`, A-3's
    // `(22297, 23023, 23024)`. The cut has to stay strictly inside the cell.
    {
        let sorted = |triangle: &[u32; 3]| -> [u32; 3] {
            let mut key = *triangle;
            key.sort_unstable();
            key
        };
        let outer: BTreeSet<[u32; 3]> = boundary.iter().map(sorted).collect();
        if caps.iter().any(|(triangle, _)| outer.contains(&sorted(triangle))) {
            if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
                println!("[JCT-DIAG] cell {index}: a cap lands on the cell's own boundary");
            }
            return None;
        }
    }
    if !cell_fan_is_conforming(&pieces, boundary, nodes) {
        if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
            println!("[JCT-DIAG] cell {index}: the pieces' fans are not conforming among themselves");
        }
        return None;
    }
    let measured: f64 = volumes.iter().sum();
    let ok = (measured - parent_volume).abs() <= volume_tolerance.max(1.0e-12) * parent_volume;
    if !ok && std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
        println!("[JCT-DIAG] cell {index}: volume guard rejected: {measured:.6e} vs {parent_volume:.6e}");
    }
    ok.then_some((pieces, caps))
}

// AI-FUNC-SUMMARY: Fan a cap polygon from whichever of its own vertices triangulates it cleanly; returns the triangles, or None when no vertex does; side effects: none.
// Notes: The apex matters, and the reason is invariant K1's meeting point. Where the other surface
//   crosses this cap, its crossing node sits **in the middle of a straight run** of the cap's
//   boundary - the cap turns by nothing there. Fanning from a vertex two steps away then draws the
//   edge straight past it, and that node is left lying on a face it is not a vertex of: A-3's 24
//   hanging nodes were all meeting points, every one on a cap edge. Trying the vertices in key order
//   and keeping the first that swallows nothing is a pure function of the cycle, so both pieces of
//   the cell still agree on the cap; when no vertex works the caller declines the split.
//   Two ways of rescuing the 235 re-entrant caps A-6a declines were tried, and **both are worse**.
//   Coning the cap to its own centre - the shape this code originally had - puts back A-8's `[V1]`
//   and `[V3]` failures, and it does not even fire on A-6a. Guarding the centre against the cell's
//   four face planes by a *relative* tolerance rather than the exact predicate does not save it:
//   the exact test was the wrong tool, but the node itself is the problem, not how it is tested.
//   Ear clipping is the other, and it is A-3 134 -> 165 with fewer cells cut, and A-6a and A-8 both pick up `[V3]` failures. A
//   cap is a piece of a *curved* surface, so it is not planar, and an ear test that works in one
//   fitted plane cuts ears that are not ears in space. A non-planar cap needs a triangulation that
//   never leaves the surface, which is not a polygon-triangulation problem at all.
fn fan_cap(
    cycle: &[u32],
    keys: &[NodeKey],
    nodes: &[Vec3],
    forbidden: &dyn Fn(&[u32; 3]) -> bool,
) -> Option<SmallVec<[[u32; 3]; 8]>> {
    let count = cycle.len();
    if count < 3 {
        return None;
    }
    let mut order: SmallVec<[usize; 16]> = (0..count).collect();
    order.sort_by_key(|slot| keys[cycle[*slot] as usize]);
    for apex in order {
        let mut fan: SmallVec<[[u32; 3]; 8]> = SmallVec::new();
        for step in 1..(count - 1) {
            fan.push([
                cycle[apex],
                cycle[(apex + step) % count],
                cycle[(apex + step + 1) % count],
            ]);
        }
        if fan_is_simple(&fan, nodes)
            && !fan_swallows_vertex(&fan, cycle, nodes)
            && !fan.iter().any(forbidden)
        {
            return Some(fan);
        }
    }
    None
}

// AI-FUNC-SUMMARY:
// Purpose: Whether a cap triangle lies **in** one of the parent cell's own four face planes.
// Inputs: the triangle, the parent tet, and the coordinates.
// Returns: true when all three of its nodes are on one parent face plane.
// Side effects: None.
// Notes: The existing guard beside the volume check compares a cap against the boundary soup as
//   *exact triangles*, and that is not the whole of the condition. A cap can lie in a face's plane
//   while matching none of that face's triangles - it cuts across them. A-6a's
//   `(19130, 33794, 35636)` is the case: the shared face {19129, 19130, 19331} is fanned from the
//   crease hub 35636 into four triangles, and the cap runs from 33794 to 19130 straight across two
//   of them. The neighbour's cap does the same, so the triangle carries four tets - `[V3]`'s
//   `multi_shared_face`, with three non-manifold edges behind it. Read as a plane test the guard
//   covers both: an equal triangle is coplanar too.
//
//   The response is not to decline the cell but to choose a different apex - `fan_cap` tries them
//   in key order and this is one more reason to reject one. On A-6a's quad the apex 33794 puts
//   (33794, 35636, 19130) in the shared face while the apex 35636 does not, and the cell keeps its
//   cut. Declining is left to the case where no apex avoids it.
fn cap_lies_in_parent_face(triangle: &[u32; 3], tet: [u32; 4], nodes: &[Vec3]) -> bool {
    const ON_FACE_FRAC: f64 = 1.0e-9;
    for slots in TET_FACES {
        let (a, b, c) = (
            nodes[tet[slots[0]] as usize],
            nodes[tet[slots[1]] as usize],
            nodes[tet[slots[2]] as usize],
        );
        let normal = b.sub(a).cross(c.sub(a));
        let area2 = normal.dot(normal);
        if area2 <= 0.0 {
            continue;
        }
        // `normal` is twice the face's area, so its square root is the face's own length
        // scale - the tolerance is relative to the cell and not to the model's units.
        let scale = area2.sqrt();
        let limit = ON_FACE_FRAC * scale.sqrt() * scale;
        if triangle
            .iter()
            .all(|node| normal.dot(nodes[*node as usize].sub(a)).abs() <= limit)
        {
            return true;
        }
    }
    false
}

// AI-FUNC-SUMMARY: Whether any edge of a fan runs through a polygon vertex that is not one of that triangle's own; returns bool; side effects: none.
// Notes: Relative to the edge's own length, because the point being tested was computed as a
//   closest approach and lies on the line only to within its own arithmetic. `[V3]`'s hanging-node
//   test is absolute at 1e-9 of the box diagonal, and 1e-6 of a cut edge is the same order.
fn fan_swallows_vertex(fan: &[[u32; 3]], cycle: &[u32], nodes: &[Vec3]) -> bool {
    const ON_EDGE_FRAC: f64 = 1.0e-6;
    for triangle in fan {
        for slot in 0..3 {
            let (a, b) = (triangle[slot], triangle[(slot + 1) % 3]);
            let (pa, pb) = (nodes[a as usize], nodes[b as usize]);
            let along = pb.sub(pa);
            let length2 = along.dot(along);
            if length2 <= 0.0 {
                continue;
            }
            for node in cycle {
                if triangle.contains(node) {
                    continue;
                }
                let rel = nodes[*node as usize].sub(pa);
                let t = rel.dot(along) / length2;
                if !(0.0..=1.0).contains(&t) {
                    continue;
                }
                let off = rel.sub(along.scale(t));
                if off.dot(off) <= ON_EDGE_FRAC * ON_EDGE_FRAC * length2 {
                    return true;
                }
            }
        }
    }
    false
}

// AI-FUNC-SUMMARY:
// Purpose: Gate G6-0 - the point where a locked curve pierces each lattice face.
// Inputs: the lattice, the snapped node table, the key table, and the curve polyline segments.
// Returns: face (its three corners, ordered by key) -> the piercing point.
// Side effects: None.
// Notes: The key is the face's own three corners, so the two cells sharing it derive the same entry
//   from the face alone - invariant J1 survives, exactly as it does for the meeting point. Where a
//   segment pierces one face more than once, or several segments pierce one face, the lowest point
//   in canonical order wins, so the map is a function of the geometry and not of the traversal or
//   the thread count.
fn curve_pierce_points(
    lattice: &Lattice,
    nodes: &[Vec3],
    keys: &[NodeKey],
    segments: &[(Vec3, Vec3)],
) -> BTreeMap<[u32; 3], Vec3> {
    if segments.is_empty() {
        return BTreeMap::new();
    }
    let bounds: Vec<(Vec3, Vec3)> = segments
        .iter()
        .map(|(a, b)| {
            (
                Vec3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
                Vec3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
            )
        })
        .collect();
    let grid = crate::meshgen::snap::BoxGrid::build(&bounds);
    let mut found: Vec<([u32; 3], Vec3)> = lattice
        .tets
        .par_iter()
        .map_init(Vec::new, |scratch: &mut Vec<u32>, tet| {
            let mut lo = nodes[tet[0] as usize];
            let mut hi = lo;
            for node in tet.iter().skip(1) {
                let point = nodes[*node as usize];
                lo = Vec3::new(lo.x.min(point.x), lo.y.min(point.y), lo.z.min(point.z));
                hi = Vec3::new(hi.x.max(point.x), hi.y.max(point.y), hi.z.max(point.z));
            }
            grid.query_into(lo, hi, scratch);
            let mut here: Vec<([u32; 3], Vec3)> = Vec::new();
            if scratch.is_empty() {
                return here;
            }
            for slots in TET_FACES {
                let mut corners = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                corners.sort_by_key(|node| keys[*node as usize]);
                let triangle = [
                    nodes[corners[0] as usize],
                    nodes[corners[1] as usize],
                    nodes[corners[2] as usize],
                ];
                for index in scratch.iter() {
                    let (a, b) = segments[*index as usize];
                    if let Some(point) = segment_pierces_triangle(a, b, triangle) {
                        here.push((corners, point));
                    }
                }
            }
            here
        })
        .flatten()
        .collect();
    found.sort_by(|left, right| {
        left.0.cmp(&right.0).then_with(|| {
            let key = |point: &Vec3| (point.x.to_bits(), point.y.to_bits(), point.z.to_bits());
            key(&left.1).cmp(&key(&right.1))
        })
    });
    let mut out: BTreeMap<[u32; 3], Vec3> = BTreeMap::new();
    for (corners, point) in found {
        out.entry(corners).or_insert(point);
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Per locked curve, the mesh edges that lie along it - the chain `[V9]` measures.
// Inputs: the tets, the node table, the curve table, and the on-curve tolerance.
// Returns: (curve index, edge) pairs, sorted, each edge ascending and listed once.
// Side effects: None.
// Notes: An edge is on the curve when **both endpoints and its midpoint** are within
//   tolerance of one of that curve's segments. Testing the endpoints alone would accept a
//   chord that leaves the curve and comes back - which is exactly the shape a chamfer takes
//   near a corner, and the one thing this must not call a carried curve. Candidate edges are
//   restricted to those whose endpoints are both on *some* curve first, so the scan is over a
//   small set rather than over every edge of every tet.
fn curve_mesh_edges(
    tets: &[[u32; 4]],
    nodes: &[Vec3],
    curves: &[LockedCurve],
    tolerance: f64,
) -> Vec<(u32, [u32; 2])> {
    if curves.is_empty() || tolerance <= 0.0 {
        return Vec::new();
    }
    let all: Vec<(Vec3, Vec3)> = curves
        .iter()
        .flat_map(|curve| curve.segments.iter().copied())
        .collect();
    let on_any = nodes_on_curve(nodes, &all, tolerance);
    if on_any.is_empty() {
        return Vec::new();
    }
    let mut candidates: BTreeSet<[u32; 2]> = BTreeSet::new();
    for tet in tets {
        for a in 0..4 {
            for b in (a + 1)..4 {
                let (x, y) = (tet[a], tet[b]);
                if on_any.contains(&x) && on_any.contains(&y) {
                    candidates.insert(if x <= y { [x, y] } else { [y, x] });
                }
            }
        }
    }
    let tolerance_sq = tolerance * tolerance;
    let out: Vec<(u32, [u32; 2])> = candidates
        .par_iter()
        .flat_map_iter(|edge| {
            let (a, b) = (nodes[edge[0] as usize], nodes[edge[1] as usize]);
            let middle = a.add(b).scale(0.5);
            let mut here: Vec<(u32, [u32; 2])> = Vec::new();
            for (index, curve) in curves.iter().enumerate() {
                let on = |point: Vec3| {
                    curve
                        .segments
                        .iter()
                        .any(|(p, q)| point_segment_distance_sq(point, *p, *q) <= tolerance_sq)
                };
                if on(a) && on(b) && on(middle) {
                    here.push((index as u32, *edge));
                }
            }
            here
        })
        .collect();
    // One cell per **edge**, not per (curve, edge). Two S2 curves can run along the same
    // geometry - a contact rim is both bodies' own sharp edge - and emitting the edge under
    // each of them writes the same line cell twice, which `[V1]` rightly calls a duplicate
    // cell. `curve_id` is a per-cell attribute and a cell has one, so the lowest curve index
    // wins; the multiplicity is reported rather than silently dropped, because a node there
    // is then checked against one of the bodies that meet along it and not both.
    let mut sorted: Vec<(u32, [u32; 2])> = out;
    sorted.sort_unstable_by_key(|(curve, edge)| (*edge, *curve));
    sorted.dedup_by_key(|(_, edge)| *edge);
    sorted.sort_unstable();
    sorted
}

// AI-FUNC-SUMMARY:
// Purpose: Gate G6-0 - the mesh nodes that already lie **on** a locked curve.
// Inputs: the snapped node table, the curve polyline segments, and the on-curve tolerance.
// Returns: the node ids within tolerance of some segment.
// Side effects: None.
// Notes: The companion to `curve_pierce_points`, and needed for the same reason. A curve crosses a
//   lattice face either through its interior - where the piercing point is the answer - or through
//   one of the face's own walk nodes, which S7 has usually snapped onto it deliberately. The second
//   case yields no piercing point at all (`segment_pierces_triangle` is strictly interior, and
//   rightly so: a node there would duplicate one both cells already share), so without this the face
//   falls back to its centroid and the fan straddles the surface. Broad-phased over the segments'
//   boxes because a strut lattice carries thousands of them.
fn nodes_on_curve(nodes: &[Vec3], segments: &[(Vec3, Vec3)], tolerance: f64) -> BTreeSet<u32> {
    if segments.is_empty() || tolerance <= 0.0 {
        return BTreeSet::new();
    }
    let bounds: Vec<(Vec3, Vec3)> = segments
        .iter()
        .map(|(a, b)| {
            (
                Vec3::new(
                    a.x.min(b.x) - tolerance,
                    a.y.min(b.y) - tolerance,
                    a.z.min(b.z) - tolerance,
                ),
                Vec3::new(
                    a.x.max(b.x) + tolerance,
                    a.y.max(b.y) + tolerance,
                    a.z.max(b.z) + tolerance,
                ),
            )
        })
        .collect();
    let grid = crate::meshgen::snap::BoxGrid::build(&bounds);
    let tolerance_sq = tolerance * tolerance;
    nodes
        .par_iter()
        .enumerate()
        .map_init(Vec::new, |scratch: &mut Vec<u32>, (index, point)| {
            grid.query_into(*point, *point, scratch);
            scratch
                .iter()
                .any(|slot| {
                    let (a, b) = segments[*slot as usize];
                    point_segment_distance_sq(*point, a, b) <= tolerance_sq
                })
                .then_some(index as u32)
        })
        .flatten()
        .collect()
}

// AI-FUNC-SUMMARY:
// Purpose: Triangulate a face's boundary walk as a fan from one of the walk's own nodes.
// Inputs: the walk and the position to fan from.
// Returns: the triangles, or None when any of them is degenerate.
// Side effects: None.
// Notes: The apex is a node both incident cells already carry, so unlike the centroid Steiner point
//   there is nothing to intern and nothing to agree on. Used where a locked curve runs through a
//   walk node: every trace on the face terminates there, so a fan from it keeps all of them as
//   edges. A walk node is collinear with the two corners of its own lattice edge, which is why the
//   degenerate case has to be tested rather than assumed away.
fn fan_from_walk_node(loop_nodes: &[u32], hub: usize, nodes: &[Vec3]) -> Option<SmallVec<[[u32; 3]; 8]>> {
    let count = loop_nodes.len();
    if count < 4 {
        return None;
    }
    let mut out: SmallVec<[[u32; 3]; 8]> = SmallVec::new();
    for slot in 0..count {
        let (a, b) = (slot, (slot + 1) % count);
        if a == hub || b == hub {
            continue;
        }
        let triangle = [loop_nodes[hub], loop_nodes[a], loop_nodes[b]];
        let (pa, pb, pc) = (
            nodes[triangle[0] as usize],
            nodes[triangle[1] as usize],
            nodes[triangle[2] as usize],
        );
        let normal = pb.sub(pa).cross(pc.sub(pa));
        if normal.dot(normal) <= 0.0 {
            return None;
        }
        out.push(triangle);
    }
    (out.len() + 2 == count).then_some(out)
}

// AI-FUNC-SUMMARY:
// Purpose: Where a segment crosses the interior of a triangle, if it does.
// Inputs: the segment's endpoints and the triangle's three corners.
// Returns: the crossing point, or None when the segment misses it, only touches its boundary, or
//   lies in its plane.
// Side effects: None.
// Notes: Strictly interior on both counts. A crossing on the triangle's own edge is already a node
//   both cells share, so inserting one there would duplicate it; and a crossing at the very end of
//   the segment is the curve's own vertex, which the next segment reports as an interior point of
//   whichever face actually contains it.
fn segment_pierces_triangle(a: Vec3, b: Vec3, triangle: [Vec3; 3]) -> Option<Vec3> {
    const INSIDE_FRAC: f64 = 1.0e-9;
    let edge1 = triangle[1].sub(triangle[0]);
    let edge2 = triangle[2].sub(triangle[0]);
    let direction = b.sub(a);
    let normal = edge1.cross(edge2);
    let denominator = normal.dot(direction);
    let scale = normal.dot(normal).sqrt() * direction.dot(direction).sqrt();
    if scale <= 0.0 || denominator.abs() <= INSIDE_FRAC * scale {
        return None;
    }
    let t = normal.dot(triangle[0].sub(a)) / denominator;
    // Endpoints included, and that is the whole of A-6a's residue. A locked curve reaches
    // this function as a *polyline*, and where it crosses a face's plane at one of its own
    // vertices - which is not the coincidence it sounds like, because S2 puts a vertex
    // wherever the arrangement needs one and the lattice is full of diagonal planes - the
    // segment before it reports `t = 1` and the segment after it reports `t = 0`. Excluding
    // both, on the reasoning that "the next segment reports it as an interior point", drops
    // the crossing entirely: there is no next segment that contains it. Measured on A-6a's
    // face `(10866, 11041, 11046)`, whose plane is `z = y`: the limb's rim is broken at
    // (0.5817, 0.2977, 0.2977), exactly on that plane, and the face fell to a centroid apex
    // whose fan then straddled the contact plane - `mixed sides on one triangle
    // [42302, 27586, 11041]`, and 49 of A-6a's `[V6]`. The duplicate the two segments now
    // both report is removed by the canonical-order `or_insert` below.
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let point = a.add(direction.scale(t));
    let rel = point.sub(triangle[0]);
    let area2 = normal.dot(normal);
    let u = normal.dot(edge1.cross(rel));
    let v = normal.dot(rel.cross(edge2));
    let slack = INSIDE_FRAC * area2;
    if u <= slack || v <= slack || u + v >= area2 - slack {
        return None;
    }
    Some(point)
}

// AI-FUNC-SUMMARY: Whether a polygon's fan triangulation covers it once - every triangle non-degenerate and all of them wound the same way; returns bool; side effects: none.
// Notes: The test a planar fan needs and the volume guard cannot supply. A fan over a re-entrant
//   polygon folds back on itself: the folded triangles are wound the other way, so their volumes
//   still sum but the cap is covered twice in one place and not at all in another.
fn fan_is_simple(cap: &[[u32; 3]], nodes: &[Vec3]) -> bool {
    if cap.is_empty() {
        return false;
    }
    let normal_of = |triangle: &[u32; 3]| -> Vec3 {
        let a = nodes[triangle[0] as usize];
        nodes[triangle[1] as usize]
            .sub(a)
            .cross(nodes[triangle[2] as usize].sub(a))
    };
    // Degeneracy only. There is no orientation-consistency test here, and that is the
    // measured answer rather than an omission. A cap is a piece of a *curved* surface and
    // can bend through a right angle inside one cell - A-6a's caps straddle the rim where
    // the limb's side meets the contact plane, so a five-node cycle has no triangulation
    // whose triangles all agree, and demanding one rejects every such cap. Two successively
    // weaker versions were tried and each was an improvement on the last: against the first
    // triangle's normal (rejects a cap as soon as the surface turns 90 degrees - 235 of
    // A-6a's cells), then against the cap's own area-weighted normal (235 -> 142, A-3's
    // `[V6]` 134 -> 132), then none at all (142 -> **0**, A-3 132 -> **124**, and A-3 and
    // A-8 both cut more cells). Nothing is given up, because the guards that decide
    // correctness are downstream and exact: every piece must be closed, and the pieces'
    // volumes must sum to the parent's. Those catch the genuinely folded caps this used to
    // pre-empt - the volume guard's rejections rise 52 -> 283 on A-6a, which is the same
    // population arriving at a test that can actually tell.
    for triangle in cap {
        if normal_of(triangle).dot(normal_of(triangle)) <= 0.0 {
            return false;
        }
    }
    true
}

// AI-FUNC-SUMMARY: Whether every triangle of a soup makes a non-degenerate tet with the soup's own centroid; returns bool; side effects: none.
// Notes: The exact predicate, not a tolerance. `orient_positively` drops a fan tet on `orient3d == 0`
//   and nothing else, so this asks precisely the question that drop asks - a tolerance here would
//   either accept a piece that still loses a tet or decline pieces that fan perfectly well.
fn fan_is_sound(soup: &[[u32; 3]], nodes: &[Vec3]) -> bool {
    let centre = polygon_soup_centroid(soup, nodes);
    soup.iter().all(|triangle| {
        orient3d(
            nodes[triangle[0] as usize],
            nodes[triangle[1] as usize],
            nodes[triangle[2] as usize],
            centre,
        ) != 0.0
    })
}

// AI-FUNC-SUMMARY:
// Purpose: The volume a **closed** polygon soup encloses, exactly, whatever its shape.
// Inputs: the soup and the node table.
// Returns: the volume, or None when the soup cannot be consistently oriented.
// Side effects: None.
// Notes: The soups here are assembled per face and carry no agreed winding, which is why the
//   original measure coned them to an interior point and summed **unsigned**. That is exact only
//   for a star-shaped piece, and §7.6's pieces stopped being star-shaped as soon as it began cutting
//   the cells it used to decline: a non-convex sliver's centroid falls outside it and the unsigned
//   sum **overshoots**, so the guard rejected correct splits. Measured on A-6a: parent 4.779550e-10
//   against 4.893833e-10, a 2.4 % overshoot on three pieces that are individually fine.
//   Orienting first removes the assumption instead of loosening the tolerance. Every edge of a
//   closed soup is shared by exactly two triangles, so a breadth-first walk fixes every triangle's
//   winding relative to the first; the signed sum is then the volume up to overall sign, and its
//   magnitude is exact for any shape. A soup that will not orient is not closed, and the caller
//   already refuses those.
fn soup_volume(soup: &[[u32; 3]], nodes: &[Vec3]) -> Option<f64> {
    if soup.is_empty() {
        return None;
    }
    let mut incident: BTreeMap<[u32; 2], SmallVec<[usize; 2]>> = BTreeMap::new();
    for (index, triangle) in soup.iter().enumerate() {
        for slot in 0..3 {
            let (a, b) = (triangle[slot], triangle[(slot + 1) % 3]);
            let key = if a <= b { [a, b] } else { [b, a] };
            incident.entry(key).or_default().push(index);
        }
    }
    if incident.values().any(|list| list.len() != 2) {
        return None;
    }
    let mut flipped: Vec<Option<bool>> = vec![None; soup.len()];
    flipped[0] = Some(false);
    let mut queue: Vec<usize> = vec![0];
    while let Some(index) = queue.pop() {
        let here = flipped[index]?;
        let triangle = soup[index];
        for slot in 0..3 {
            let (a, b) = (triangle[slot], triangle[(slot + 1) % 3]);
            let key = if a <= b { [a, b] } else { [b, a] };
            let list = incident.get(&key)?;
            let other = if list[0] == index { list[1] } else { list[0] };
            if other == index {
                return None;
            }
            // The neighbour must traverse the shared edge the other way round. If it already
            // walks it in the same direction, it is the one that needs flipping.
            let mate = soup[other];
            let same = (0..3).any(|k| mate[k] == a && mate[(k + 1) % 3] == b);
            let want = if same { !here } else { here };
            match flipped[other] {
                None => {
                    flipped[other] = Some(want);
                    queue.push(other);
                }
                Some(found) if found == want => {}
                Some(_) => return None,
            }
        }
    }
    if flipped.iter().any(|flip| flip.is_none()) {
        return None;
    }
    let mut total = 0.0;
    for (index, triangle) in soup.iter().enumerate() {
        let (a, b, c) = (
            nodes[triangle[0] as usize],
            nodes[triangle[1] as usize],
            nodes[triangle[2] as usize],
        );
        let signed = a.dot(b.cross(c)) / 6.0;
        total += if flipped[index] == Some(true) {
            -signed
        } else {
            signed
        };
    }
    Some(total.abs())
}

// AI-FUNC-SUMMARY:
// Purpose: Whether the tets §7.6's pieces would fan to are conforming **among themselves**.
// Inputs: the cell's pieces as boundary soups, and the node table.
// Returns: false when two pieces' fans would share a face three or more ways, or when one piece's
//   centroid lands inside a face of another's fan.
// Side effects: None.
// Notes: `[V3]`'s two tests, asked of one cell before it commits. A piece is fanned from its own
//   centroid, and nothing so far stops one piece's centroid from landing exactly on a *sibling's*
//   interior fan face - both centroids are averages over node sets that share most of their
//   members, so an exact coplanarity is not the coincidence it looks like. Measured: A-8's cell
//   61991 puts both its pieces' centroids on the plane `2x - z = 0.34375` through the lattice edge
//   `(8889, 8919)`, and node 46361 then lies in the interior of face `(8889, 8919, 46362)` to
//   within 5e-17. Declining costs the cell its cut and nothing else, because the fallback fan is
//   unconditionally valid; emitting it costs conformity, which is not recoverable downstream.
//   The plane tolerance is relative to the triangle rather than absolute so it does not depend on
//   which frame S8 is running in, and it is set well above `[V3]`'s `1e-9 * diag` so a cell this
//   declines is one `[V3]` would have failed.
fn cell_fan_is_conforming(
    pieces: &[SmallVec<[[u32; 3]; 16]>],
    boundary: &[[u32; 3]],
    nodes: &[Vec3],
) -> bool {
    let why = |reason: &str| {
        if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
            println!("[JCT-CONFORM] {reason}");
        }
        false
    };
    let sorted = |triangle: &[u32; 3]| -> [u32; 3] {
        let mut key = *triangle;
        key.sort_unstable();
        key
    };
    // The cell's own outer boundary is **partitioned** by the split: a triangle on it faces
    // the neighbouring cell, which fans it exactly once, so a piece either owns it or does
    // not. Two pieces owning the same one puts four tets on that face, and the neighbour
    // cannot see the difference because the face triangulation is shared - `[V3]`'s
    // `multi_shared_face`. Caps are exempt by construction: they are not on the boundary.
    let held: Vec<BTreeSet<[u32; 3]>> = pieces
        .iter()
        .map(|piece| piece.iter().map(sorted).collect())
        .collect();
    for triangle in boundary {
        let key = sorted(triangle);
        if held.iter().filter(|piece| piece.contains(&key)).count() != 1 {
            return why("a boundary triangle is not owned by exactly one piece");
        }
    }
    const PLANE_FRAC: f64 = 1.0e-5;
    // Centroids are not mesh nodes yet, so they take synthetic ids from the top of the range;
    // a cell has a handful of pieces and the parent lattice never reaches these values.
    let apex_id = |piece: usize| -> u32 { u32::MAX - piece as u32 };
    let centroids: SmallVec<[Vec3; 4]> = pieces
        .iter()
        .map(|piece| polygon_soup_centroid(piece, nodes))
        .collect();
    let first_apex = apex_id(pieces.len().saturating_sub(1));
    let point_of = |id: u32| -> Vec3 {
        if id >= first_apex {
            centroids[(u32::MAX - id) as usize]
        } else {
            nodes[id as usize]
        }
    };
    let mut faces: BTreeMap<[u32; 3], usize> = BTreeMap::new();
    let mut corners: BTreeSet<u32> = BTreeSet::new();
    for (index, piece) in pieces.iter().enumerate() {
        let apex = apex_id(index);
        corners.insert(apex);
        for triangle in piece.iter() {
            corners.extend(triangle.iter().copied());
            let tet = [triangle[0], triangle[1], triangle[2], apex];
            for slot in 0..4 {
                let mut face = [
                    tet[(slot + 1) % 4],
                    tet[(slot + 2) % 4],
                    tet[(slot + 3) % 4],
                ];
                face.sort_unstable();
                *faces.entry(face).or_insert(0) += 1;
            }
        }
    }
    if faces.values().any(|count| *count > 2) {
        return why("a face inside the cell carries more than two tets");
    }
    for (face, _) in faces.iter() {
        let (a, b, c) = (point_of(face[0]), point_of(face[1]), point_of(face[2]));
        let normal = b.sub(a).cross(c.sub(a));
        let area2 = normal.dot(normal);
        if area2 <= 0.0 {
            return why("a fan face is degenerate");
        }
        let scale = area2.sqrt();
        for node in corners.iter() {
            if face.contains(node) {
                continue;
            }
            let point = point_of(*node);
            if normal.dot(point.sub(a)).abs() > PLANE_FRAC * scale * scale.sqrt() {
                continue;
            }
            let rel = point.sub(a);
            let u = normal.dot(b.sub(a).cross(rel));
            let v = normal.dot(rel.cross(c.sub(a)));
            if u > 0.0 && v > 0.0 && u + v < area2 {
                return why("a node lies inside a fan face it is not a vertex of");
            }
        }
    }
    true
}

// AI-FUNC-SUMMARY: The volume a closed polygon soup encloses, summed as unsigned tets over an interior point; returns f64; side effects: none.
// Notes: Unsigned on purpose. The boundary soups here are assembled per face by `face_mesh` and
//   carry no agreed winding, so a signed sum would cancel rather than measure. Over a soup whose
//   cone point is genuinely interior the unsigned sum *is* the volume; where it is not, the sum
//   overshoots, which is exactly the failure the caller tests for.
fn fan_volume(soup: &[[u32; 3]], centre: Vec3, nodes: &[Vec3]) -> f64 {
    soup.iter()
        .map(|triangle| {
            let a = nodes[triangle[0] as usize].sub(centre);
            let b = nodes[triangle[1] as usize].sub(centre);
            let c = nodes[triangle[2] as usize].sub(centre);
            a.dot(b.cross(c)).abs() / 6.0
        })
        .sum()
}
