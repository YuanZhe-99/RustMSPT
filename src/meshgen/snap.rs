//! S7 - snap (G6-1).
//!
//! The background lattice of S5 knows nothing about the geometry and S6 only
//! *labels* it, so up to here the mesh follows the input in staircases. S7 is the
//! first stage that moves geometry: every lattice vertex close enough to an active
//! patch is pulled onto it, corners and feature curves first, so that S8's cut
//! meets the surface at nodes that are already exact instead of slicing arbitrarily
//! close to them.
//!
//! Frozen inputs (`SPEC_meshgen_numerics.md` §5, the S7 rows):
//!
//! | Decision | Predicate | Class |
//! |---|---|---|
//! | edge crossing exists | `orient3d` sign change along the edge | **X** exact |
//! | snap target priority | corner > curve > surface, then ascending `NodeKey` | **I** |
//! | snap would invert | `orient3d > 0` on every incident tet | **X** exact |
//! | motion cap | `‖Δ‖² ≤ (0.3·L_min)²` | **A** |
//!
//! and the two arbitration rows of `SPEC_meshgen_geometry.md` §12: a move that
//! would invert a tet is rejected (**ARB-10**, `[SNAP-REJ]`), a move longer than the
//! cap is clamped and the node marked under-snapped for the [V5] gate (**ARB-11**,
//! `[SNAP-CAP]`).
//!
//! The crossing point of an edge is a *construction* (class C4 in the numerics
//! freeze): its accuracy is f64, and its validity is re-established by the exact
//! inversion test rather than assumed. Only the existence of the crossing is exact.

use crate::io::vtu::{ArrayData, DataArray, VtuDoc, VTK_TETRA};
use crate::meshgen::arrange::{ArrangeComponent, ArrangedCurveKind, ArrangedSurface};
use crate::meshgen::classify::Classification;
use crate::meshgen::lattice::Lattice;
use crate::meshgen::predicates::{
    best_projection_axis, node_key, orient2d_axis, orient3d, orient3d_filtered,
};
use crate::types::Vec3;
use rayon::prelude::*;
use smallvec::SmallVec;
use std::collections::BTreeMap;

/// The generalized motion cap: no snap moves a node further than this fraction of
/// its shortest incident lattice edge. The reference implementation applied it to
/// one pass only; PLAN §4 hardens it to every accepted move.
pub const SNAP_MOTION_CAP: f64 = 0.30;

/// The re-check band. A cut node landing within 2.5 % of an edge's end would give
/// S8 a degenerate sub-tet, so the endpoint is promoted onto the patch instead
/// (`SPEC_meshgen_geometry.md` §5.1, invariant K2).
pub const SNAP_RECHECK_LOW: f64 = 0.025;
/// The upper end of the same band.
pub const SNAP_RECHECK_HIGH: f64 = 1.0 - SNAP_RECHECK_LOW;

/// Alternating-projection passes used as the curve target inside a degraded
/// arrangement neighbourhood (ARB-2), where no explicit curve exists to snap to.
pub const ALTERNATING_PROJECTION_PASSES: usize = 15;

/// Deterministic target-priority weights. They are a numeric encoding of the
/// frozen order "corner > curve > surface"; nothing divides by them.
pub const WEIGHT_CORNER: f64 = 1.0e7;
/// Feature-curve weight.
pub const WEIGHT_CURVE: f64 = 1.0e4;
/// Surface-patch weight.
pub const WEIGHT_SURFACE: f64 = 1.0e0;

/// What a node ended up bound to, in the `constraint_kind` encoding of
/// `SPEC_meshgen_contracts.md` §2.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum TargetKind {
    Free = 0,
    Surface = 1,
    Polyline = 2,
    Corner = 3,
    BoxFace = 4,
}

impl TargetKind {
    // AI-FUNC-SUMMARY: The frozen priority weight of a target kind; returns f64; side effects: none.
    pub fn weight(self) -> f64 {
        match self {
            TargetKind::Corner => WEIGHT_CORNER,
            TargetKind::Polyline => WEIGHT_CURVE,
            TargetKind::Surface => WEIGHT_SURFACE,
            TargetKind::Free | TargetKind::BoxFace => 0.0,
        }
    }
}

/// One exact edge-patch crossing: where S8 will cut, and on which patch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeCrossing {
    /// The lattice edge, ascending node index.
    pub edge: [u32; 2],
    /// The arranged face carrying the crossing.
    pub face: u32,
    /// The component the face belongs to.
    pub component: i32,
    /// Parameter along `edge[0] -> edge[1]`.
    pub t: f64,
    /// The constructed crossing point (class C4: accuracy-only).
    pub point: Vec3,
}

/// S7 diagnostics.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapStats {
    pub n_nodes: usize,
    pub n_edges: usize,
    pub n_crossed_edges: usize,
    pub n_crossings: usize,
    /// Edges an active patch crosses more than once - invariant K1's escalation set.
    pub n_multi_crossing_edges: usize,
    pub n_candidates: usize,
    pub n_snapped_corner: usize,
    pub n_snapped_curve: usize,
    pub n_snapped_surface: usize,
    /// ARB-11: moves clamped at the cap, i.e. nodes left under-snapped.
    pub n_capped: usize,
    /// ARB-10: moves rejected because they would invert an incident tet.
    pub n_rejected: usize,
    /// Nodes whose move was restricted to a domain face, edge, or corner.
    pub n_box_constrained: usize,
    /// Endpoints promoted onto a patch by the 97.5/2.5 % re-check (K2).
    pub n_recheck_promoted: usize,
    /// Crossings still inside the re-check band after the pass (the promotion was
    /// rejected by the inversion test).
    pub n_recheck_residual: usize,
    /// Curve targets produced by alternating projection instead of an explicit curve.
    pub n_alternating_projection: usize,
    /// Locked curve segments the stage was asked to reproduce (§5.1 invariant K1's set).
    pub n_curve_segments: usize,
    /// Of those, the ones a chain of mesh edges actually covers.
    pub n_curve_segments_covered: usize,
    /// Parent nodes lying exactly on an active patch after the stage.
    pub n_on_cut: usize,
    pub n_filter_uncertain: usize,
    pub max_motion: f64,
    pub mean_motion: f64,
}

/// The result of S7.
#[derive(Clone, Debug)]
pub struct Snapped {
    /// The moved lattice nodes; the connectivity is S5's, unchanged.
    pub nodes: Vec<Vec3>,
    /// Per node, `SPEC_meshgen_contracts.md` §2.2 `constraint_kind`.
    pub constraint_kind: Vec<u8>,
    /// Per node, the component X or curve id the constraint binds to; `-1` free.
    pub constraint_ref: Vec<i32>,
    /// Per node, the total distance moved from the position S5 gave it. Bounded by
    /// `SNAP_MOTION_CAP` times the node's shortest incident lattice edge.
    pub motion: Vec<f64>,
    /// Nodes clamped by the cap, for the [V5] gate (ARB-11).
    pub under_snapped: Vec<u32>,
    /// Where S8 cuts: one entry per surviving edge-patch crossing.
    pub crossings: Vec<EdgeCrossing>,
    /// Parent nodes lying exactly on a patch - the "on-cut vertices" of the
    /// kirigami face-split tables (`SPEC_meshgen_geometry.md` §5.1).
    pub on_cut: Vec<(u32, i32)>,
    /// **Invariant K1's remedy set** (`SPEC_meshgen_geometry.md` §5.1: "refine one level,
    /// and if the sizing floor is reached, hand the cells to §7"). Each entry is a world
    /// point and the length scale that produced it: a doubly-crossed edge's midpoint with
    /// its own length, and an uncovered locked curve segment's midpoint with the segment's
    /// length. The caller asks the sizing field for **half** of that scale there and reruns
    /// S5-S7; at the `h_min` floor the escalation stands and S8 takes the cells. Empty
    /// means the mesh already reproduces every locked curve and no edge is doubly crossed.
    pub refine_requests: Vec<(Vec3, f64)>,
    pub warnings: Vec<String>,
    pub stats: SnapStats,
}

/// Inputs to S7.
#[derive(Clone, Debug)]
pub struct SnapOptions {
    pub domain_min: Vec3,
    pub domain_max: Vec3,
    /// The weld tolerance, in the same (normalized) frame as the lattice.
    pub eps: f64,
}

impl Default for SnapOptions {
    // AI-FUNC-SUMMARY: The unit domain with a 1e-9 weld tolerance; returns SnapOptions; side effects: none.
    fn default() -> Self {
        SnapOptions {
            domain_min: Vec3::new(0.0, 0.0, 0.0),
            domain_max: Vec3::new(1.0, 1.0, 1.0),
            eps: 1.0e-9,
        }
    }
}

// ---------------------------------------------------------------------------
// A uniform bucket grid over axis-aligned boxes
// ---------------------------------------------------------------------------

/// Items bucketed by their AABB, so both "what does this edge touch" and "what is
/// near this point" are one bucket sweep. It is only ever a *superset* filter -
/// every candidate goes through the exact or closed-form test afterwards - so its
/// resolution affects speed and nothing else.
pub(crate) struct BoxGrid {
    origin: Vec3,
    cell: f64,
    dims: [i64; 3],
    buckets: Vec<Vec<u32>>,
}

impl BoxGrid {
    // AI-FUNC-SUMMARY:
    // Purpose: Bucket a list of AABBs into a uniform grid sized from the item count.
    // Inputs: the items' bounds.
    // Returns: BoxGrid (empty-safe).
    // Side effects: None.
    pub(crate) fn build(items: &[(Vec3, Vec3)]) -> BoxGrid {
        let mut min = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (lo, hi) in items {
            min = Vec3::new(min.x.min(lo.x), min.y.min(lo.y), min.z.min(lo.z));
            max = Vec3::new(max.x.max(hi.x), max.y.max(hi.y), max.z.max(hi.z));
        }
        if items.is_empty() {
            min = Vec3::new(0.0, 0.0, 0.0);
            max = Vec3::new(1.0, 1.0, 1.0);
        }
        let span = max.sub(min);
        let longest = span.x.max(span.y).max(span.z).max(f64::MIN_POSITIVE);
        let side = (items.len() as f64).cbrt().ceil().max(1.0) as i64;
        let side = side.clamp(1, 128);
        let cell = longest / side as f64;
        let dim = |s: f64| -> i64 { ((s / cell).ceil() as i64 + 1).clamp(1, side) };
        let dims = [dim(span.x), dim(span.y), dim(span.z)];
        let mut grid = BoxGrid {
            origin: min,
            cell,
            dims,
            buckets: vec![Vec::new(); (dims[0] * dims[1] * dims[2]) as usize],
        };
        for (index, (lo, hi)) in items.iter().enumerate() {
            for key in grid.cell_range(*lo, *hi) {
                grid.buckets[key].push(index as u32);
            }
        }
        grid
    }

    // AI-FUNC-SUMMARY: Bucket indices whose cells overlap an AABB; returns Vec<usize>; side effects: none.
    fn cell_range(&self, lo: Vec3, hi: Vec3) -> Vec<usize> {
        let axis = |value: f64, base: f64, dim: i64| -> i64 {
            (((value - base) / self.cell).floor() as i64).clamp(0, dim - 1)
        };
        let lo_i = [
            axis(lo.x, self.origin.x, self.dims[0]),
            axis(lo.y, self.origin.y, self.dims[1]),
            axis(lo.z, self.origin.z, self.dims[2]),
        ];
        let hi_i = [
            axis(hi.x, self.origin.x, self.dims[0]),
            axis(hi.y, self.origin.y, self.dims[1]),
            axis(hi.z, self.origin.z, self.dims[2]),
        ];
        let mut out = Vec::new();
        for z in lo_i[2]..=hi_i[2] {
            for y in lo_i[1]..=hi_i[1] {
                for x in lo_i[0]..=hi_i[0] {
                    out.push((z * self.dims[1] * self.dims[0] + y * self.dims[0] + x) as usize);
                }
            }
        }
        out
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Ascending, deduplicated item indices whose bucket overlaps an AABB.
    // Side effects: Clears and fills `out`.
    // Notes: The caller owns the buffer so a hot parallel loop allocates once per thread rather
    //   than once per query.
    pub(crate) fn query_into(&self, lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        for key in self.cell_range(lo, hi) {
            out.extend_from_slice(&self.buckets[key]);
        }
        out.sort_unstable();
        out.dedup();
    }
}

// AI-FUNC-SUMMARY: Bounds of a triangle; returns (min, max); side effects: none.
fn tri_bounds(tri: [Vec3; 3]) -> (Vec3, Vec3) {
    let lo = Vec3::new(
        tri[0].x.min(tri[1].x).min(tri[2].x),
        tri[0].y.min(tri[1].y).min(tri[2].y),
        tri[0].z.min(tri[1].z).min(tri[2].z),
    );
    let hi = Vec3::new(
        tri[0].x.max(tri[1].x).max(tri[2].x),
        tri[0].y.max(tri[1].y).max(tri[2].y),
        tri[0].z.max(tri[1].z).max(tri[2].z),
    );
    (lo, hi)
}

// AI-FUNC-SUMMARY: Bounds of a segment; returns (min, max); side effects: none.
fn seg_bounds(a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    (
        Vec3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
        Vec3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
    )
}

// AI-FUNC-SUMMARY: An AABB grown by a radius in every direction; returns (min, max); side effects: none.
fn ball_bounds(p: Vec3, radius: f64) -> (Vec3, Vec3) {
    (
        Vec3::new(p.x - radius, p.y - radius, p.z - radius),
        Vec3::new(p.x + radius, p.y + radius, p.z + radius),
    )
}

// AI-FUNC-SUMMARY: Euclidean length; returns f64; side effects: none.
fn norm(v: Vec3) -> f64 {
    v.dot(v).sqrt()
}

// ---------------------------------------------------------------------------
// Closed-form closest points (class C4: accuracy-only constructions)
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: The closest point of a segment to `p`; returns Vec3; side effects: none.
fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b.sub(a);
    let denominator = ab.dot(ab);
    if denominator <= 0.0 {
        return a;
    }
    let t = (p.sub(a).dot(ab) / denominator).clamp(0.0, 1.0);
    a.add(ab.scale(t))
}

// AI-FUNC-SUMMARY:
// Purpose: The closest point of a triangle to `p` (Ericson's region test).
// Returns: Vec3 on the triangle - inside the face, on an edge, or at a vertex.
// Side effects: None.
fn closest_on_triangle(p: Vec3, tri: [Vec3; 3]) -> Vec3 {
    let (a, b, c) = (tri[0], tri[1], tri[2]);
    let ab = b.sub(a);
    let ac = c.sub(a);
    let ap = p.sub(a);
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p.sub(b);
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let denominator = d1 - d3;
        let v = if denominator != 0.0 { d1 / denominator } else { 0.0 };
        return a.add(ab.scale(v));
    }
    let cp = p.sub(c);
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let denominator = d2 - d6;
        let w = if denominator != 0.0 { d2 / denominator } else { 0.0 };
        return a.add(ac.scale(w));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let denominator = (d4 - d3) + (d5 - d6);
        let w = if denominator != 0.0 {
            (d4 - d3) / denominator
        } else {
            0.0
        };
        return b.add(c.sub(b).scale(w));
    }
    let denominator = va + vb + vc;
    if denominator == 0.0 {
        return a;
    }
    let v = vb / denominator;
    let w = vc / denominator;
    a.add(ab.scale(v)).add(ac.scale(w))
}

// ---------------------------------------------------------------------------
// The exact edge-patch crossing test
// ---------------------------------------------------------------------------

/// What one lattice edge does to one triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
enum EdgeHit {
    /// No transversal crossing (including a coplanar edge, which S8 handles as a
    /// face-coincidence case rather than a cut).
    None,
    /// The edge crosses the triangle's interior or boundary at parameter `t`.
    Cross { t: f64, point: Vec3 },
    /// The first endpoint lies exactly on the triangle.
    OnStart,
    /// The second endpoint lies exactly on the triangle.
    OnEnd,
}

// AI-FUNC-SUMMARY:
// Purpose: Whether a point known to lie in a triangle's plane lies within the triangle.
// Returns: bool (boundary counts as inside).
// Side effects: None.
// Notes: Exact - `orient2d` after dropping the axis the triangle's normal dominates, which is the
//   projection that cannot collapse the triangle.
fn planar_point_in_triangle(p: Vec3, tri: [Vec3; 3]) -> bool {
    let axis = best_projection_axis(tri[0], tri[1], tri[2]);
    let mut positive = false;
    let mut negative = false;
    for (a, b) in [
        (tri[0], tri[1]),
        (tri[1], tri[2]),
        (tri[2], tri[0]),
    ] {
        let value = orient2d_axis(a, b, p, axis);
        if value > 0.0 {
            positive = true;
        } else if value < 0.0 {
            negative = true;
        }
    }
    !(positive && negative)
}

// AI-FUNC-SUMMARY:
// Purpose: The exact relationship between one lattice edge and one arranged face.
// Inputs: the edge endpoints, the triangle, and a counter for static-filter escalations.
// Returns: EdgeHit.
// Side effects: Increments `uncertain` when the §6.1 static filter could not certify a sign.
// Notes: Existence is exact (`orient3d` signs only); the parameter `t` and the point are f64
//   constructions, class C4 in the numerics freeze - their accuracy matters, their validity does
//   not, because the inversion test and S8's guarded dry-run re-establish that from the moved
//   coordinates.
//
//   A zero in the *line* tests means the edge passes exactly through a triangle edge or vertex.
//   That is a real crossing of the patch, not a degeneracy to reject: the same point is reported
//   by both faces sharing that edge, and the caller deduplicates by parameter. Rejecting it would
//   leave S8 an uncut cell whose vertices straddle the surface.
fn edge_face_hit(p: Vec3, q: Vec3, tri: [Vec3; 3], uncertain: &mut usize) -> EdgeHit {
    let (a, b, c) = (tri[0], tri[1], tri[2]);
    let (sign_p, certified_p, dp, _) = orient3d_filtered(a, b, c, p);
    let (sign_q, certified_q, dq, _) = orient3d_filtered(a, b, c, q);
    if !certified_p {
        *uncertain += 1;
    }
    if !certified_q {
        *uncertain += 1;
    }
    if sign_p == 0 && sign_q == 0 {
        return EdgeHit::None;
    }
    if sign_p == 0 {
        return if planar_point_in_triangle(p, tri) {
            EdgeHit::OnStart
        } else {
            EdgeHit::None
        };
    }
    if sign_q == 0 {
        return if planar_point_in_triangle(q, tri) {
            EdgeHit::OnEnd
        } else {
            EdgeHit::None
        };
    }
    if sign_p == sign_q {
        return EdgeHit::None;
    }
    let mut sign_of = |x: Vec3, y: Vec3, z: Vec3, w: Vec3| -> i8 {
        let (sign, certified, _, _) = orient3d_filtered(x, y, z, w);
        if !certified {
            *uncertain += 1;
        }
        sign
    };
    let s0 = sign_of(p, q, a, b);
    let s1 = sign_of(p, q, b, c);
    let s2 = sign_of(p, q, c, a);
    let mut positive = false;
    let mut negative = false;
    for sign in [s0, s1, s2] {
        if sign > 0 {
            positive = true;
        } else if sign < 0 {
            negative = true;
        }
    }
    if positive && negative {
        return EdgeHit::None;
    }
    if !positive && !negative {
        // The edge lies in the plane of all three side tests at once, which the
        // straddle test above has already excluded; nothing left to cross.
        return EdgeHit::None;
    }
    let denominator = dp - dq;
    let t = if denominator != 0.0 {
        (dp / denominator).clamp(0.0, 1.0)
    } else {
        0.5
    };
    EdgeHit::Cross {
        t,
        point: p.add(q.sub(p).scale(t)),
    }
}

// ---------------------------------------------------------------------------
// The geometry S7 snaps to
// ---------------------------------------------------------------------------

/// The active, non-box patches, the feature curves, and the corner nodes, each
/// with the grid its queries run against.
struct SnapGeometry {
    triangles: Vec<[Vec3; 3]>,
    face_index: Vec<u32>,
    face_component: Vec<i32>,
    face_grid: BoxGrid,
    corners: Vec<Vec3>,
    corner_grid: BoxGrid,
    segments: Vec<(Vec3, Vec3)>,
    segment_curve: Vec<i32>,
    segment_grid: BoxGrid,
    degraded: Vec<Vec3>,
    degraded_grid: BoxGrid,
}

// AI-FUNC-SUMMARY:
// Purpose: Gather everything S7 may snap to, once, and index it.
// Inputs: the clipped arranged surface and S6's active-patch filter.
// Returns: SnapGeometry.
// Side effects: None.
// Notes: Three exclusions, all deliberate. **Inactive patches** (PLAN §5.2) separate nothing, so
//   snapping to them would move nodes for a surface that is never cut. **Box-tagged faces and box
//   curves** are the domain boundary itself: the lattice already lies on it exactly, and pulling a
//   node onto a box cap would fight the box constraint that keeps the domain a box.
//
//   The third needs an explanation. `ArrangedSurface::corner_nodes` is *not* the sharp-corner set:
//   S2 unions S1's corners and junctions with the node of **every** point feature, and the C6
//   "tangential point contact" case fires for any two triangles of one component that meet at a
//   single vertex - which is ordinary mesh adjacency around a vertex fan, not a feature. On a plain
//   UV sphere at 12 bands that is 2688 C6 events over 266 vertices: S1 reports 0 corners and 0
//   curves, and S2's `corner_nodes` still contains every vertex of the sphere. Snapping to that set
//   captures a smooth surface at every node and leaves S8 with nothing to cut - 74 crossings became
//   2 on the first measurement. So a node whose only claim is a single-component point feature is
//   dropped here, unless it also lies on a feature curve, where its claim is real. Nothing is lost
//   by that: a genuine self-touching contact makes the shared vertex non-manifold, and S1 reports
//   non-manifold vertices as junctions on their own account.
//
//   Recorded rather than fixed upstream: whether S2 should emit C6 for same-component vertex
//   adjacency at all is a G2-2 question, and the surviving events are harmless everywhere else -
//   they change no geometry (`SPEC_meshgen_geometry.md` §10, row C6 "unchanged").
fn gather_geometry(surface: &ArrangedSurface, classification: &Classification) -> SnapGeometry {
    let mut triangles = Vec::new();
    let mut face_index = Vec::new();
    let mut face_component = Vec::new();
    for (index, face) in surface.faces.iter().enumerate() {
        if face.box_tagged {
            continue;
        }
        if !classification.active_face.get(index).copied().unwrap_or(true) {
            continue;
        }
        // A face two solids share in exact contact belongs to **both** of them, and S2
        // records that in `components`. Everything downstream used to read the single
        // `component` field, so the shared plane was crossed for one body only: the other
        // body's boundary there produced no crossing, S8 never cut it, and the contact
        // plane never became a face of the mesh at all. That is A-6a/A-6b's 927/3,352
        // `[V6]` violations - measured as a sliver of two-component steps straddling
        // x = 0.5817 with *no* mesh face on the plane itself. Emitting the triangle once
        // per component gives each body its own crossing at the same point; `cut_lattice`
        // then gives those coincident crossings a single shared node.
        let mut owners: SmallVec<[i32; 2]> = face.components.clone();
        owners.sort_unstable();
        owners.dedup();
        if owners.is_empty() {
            owners.push(face.component);
        }
        for component in owners {
            triangles.push([
                surface.vertices[face.nodes[0]],
                surface.vertices[face.nodes[1]],
                surface.vertices[face.nodes[2]],
            ]);
            face_index.push(index as u32);
            face_component.push(component);
        }
    }
    let face_boxes: Vec<(Vec3, Vec3)> = triangles.iter().map(|tri| tri_bounds(*tri)).collect();
    let face_grid = BoxGrid::build(&face_boxes);

    let mut segments = Vec::new();
    let mut segment_curve = Vec::new();
    let mut corner_nodes: Vec<usize> = surface.corner_nodes.iter().copied().collect();
    for (id, curve) in surface.curves.iter().enumerate() {
        if curve.kind == ArrangedCurveKind::Box {
            continue;
        }
        for window in curve.nodes.windows(2) {
            let a = surface.vertices[window[0]];
            let b = surface.vertices[window[1]];
            if a == b {
                continue;
            }
            segments.push((a, b));
            segment_curve.push(id as i32);
        }
    }
    // A node lying on a feature curve earned its place there; a node whose only
    // claim is a single-component tangential-contact point feature did not - see
    // the note on this function.
    let mut on_curve: Vec<usize> = Vec::new();
    for curve in &surface.curves {
        if curve.kind == ArrangedCurveKind::Box {
            continue;
        }
        on_curve.extend(curve.nodes.iter().copied());
    }
    on_curve.sort_unstable();
    on_curve.dedup();
    let mut adjacency_only: Vec<usize> = surface
        .point_features
        .iter()
        .filter(|feature| feature.components.len() < 2)
        .map(|feature| feature.node)
        .collect();
    adjacency_only.sort_unstable();
    adjacency_only.dedup();
    corner_nodes.retain(|node| {
        adjacency_only.binary_search(node).is_err() || on_curve.binary_search(node).is_ok()
    });
    for feature in &surface.point_features {
        if feature.components.len() >= 2 {
            corner_nodes.push(feature.node);
        }
    }
    corner_nodes.sort_unstable();
    corner_nodes.dedup();
    let corners: Vec<Vec3> = corner_nodes
        .iter()
        .map(|node| surface.vertices[*node])
        .collect();
    let corner_grid = BoxGrid::build(
        &corners
            .iter()
            .map(|point| (*point, *point))
            .collect::<Vec<_>>(),
    );
    let segment_grid = BoxGrid::build(
        &segments
            .iter()
            .map(|(a, b)| seg_bounds(*a, *b))
            .collect::<Vec<_>>(),
    );
    let degraded: Vec<Vec3> = surface
        .degraded
        .iter()
        .flat_map(|neighborhood| neighborhood.points.iter().copied())
        .collect();
    let degraded_grid = BoxGrid::build(
        &degraded
            .iter()
            .map(|point| (*point, *point))
            .collect::<Vec<_>>(),
    );

    SnapGeometry {
        triangles,
        face_index,
        face_component,
        face_grid,
        corners,
        corner_grid,
        segments,
        segment_curve,
        segment_grid,
        degraded,
        degraded_grid,
    }
}

// ---------------------------------------------------------------------------
// Lattice combinatorics
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Measure how much of the locked curve set the snapped mesh actually reproduces as chains of mesh edges.
// Inputs: the locked curve segments (S2's, box curves already excluded), the snapped node coordinates, the lattice edge list, and the weld tolerance.
// Returns: (segments, covered, uncovered) - the segment count, how many a chain of mesh edges covers end to end, and the uncovered ones as (midpoint, segment length) for K1's refine-and-retry.
// Side effects: None.
// Notes: Capture used to be **opportunistic and unmeasured**: S7 snapped what happened to be in
//   reach and nothing checked the result, so "the curve is conforming" was an assumption. Refining
//   toward curves alone made it *worse* - A-3's curve snaps fell 40 -> 16 once `curve_sources`
//   landed, because the motion cap is `SNAP_MOTION_CAP * l_min` and shrinks with `h`. A count is
//   the precondition for fixing that; this is the count.
//
//   A mesh edge **covers** part of a locked segment when both its endpoints lie within `tol` of the
//   segment and it runs along the segment rather than across it - tested as the edge's own length
//   matching the span of its endpoints' projections, which rejects a chord that leaves the curve and
//   comes back. The covering intervals are unioned in parameter space and the segment counts as
//   covered when the union spans it, so a segment carried by several shorter mesh edges counts,
//   which is the normal case once refinement puts more than one element along it.
fn curve_coverage(
    segments: &[(Vec3, Vec3)],
    nodes: &[Vec3],
    edges: &[[u32; 2]],
    tol: f64,
) -> (usize, usize, Vec<(Vec3, f64)>) {
    if segments.is_empty() {
        return (0, 0, Vec::new());
    }
    let edge_boxes: Vec<(Vec3, Vec3)> = edges
        .iter()
        .map(|edge| seg_bounds(nodes[edge[0] as usize], nodes[edge[1] as usize]))
        .collect();
    let grid = BoxGrid::build(&edge_boxes);
    let per_segment: Vec<bool> = segments
        .par_iter()
        .map_init(Vec::new, |scratch: &mut Vec<u32>, (a, b)| {
            let axis = b.sub(*a);
            let length2 = axis.dot(axis);
            if length2 <= 0.0 {
                return true;
            }
            let (lo, hi) = seg_bounds(*a, *b);
            let pad = Vec3::new(tol, tol, tol);
            grid.query_into(lo.sub(pad), hi.add(pad), scratch);
            let mut spans: Vec<(f64, f64)> = Vec::new();
            for index in scratch.iter() {
                let edge = edges[*index as usize];
                let (p, q) = (nodes[edge[0] as usize], nodes[edge[1] as usize]);
                let t_p = p.sub(*a).dot(axis) / length2;
                let t_q = q.sub(*a).dot(axis) / length2;
                if norm(closest_on_segment(p, *a, *b).sub(p)) > tol
                    || norm(closest_on_segment(q, *a, *b).sub(q)) > tol
                {
                    continue;
                }
                // Along the segment, not across it: the edge's own length must match the
                // span it claims, or it is a chord that left the curve in between.
                let span = (t_q - t_p).abs() * length2.sqrt();
                if (norm(q.sub(p)) - span).abs() > tol {
                    continue;
                }
                let (s, e) = if t_p <= t_q { (t_p, t_q) } else { (t_q, t_p) };
                spans.push((s.max(0.0), e.min(1.0)));
            }
            spans.sort_by(|l, r| l.0.total_cmp(&r.0));
            let mut reach = 0.0f64;
            for (s, e) in spans {
                if s > reach + f64::EPSILON {
                    break;
                }
                reach = reach.max(e);
            }
            reach >= 1.0 - f64::EPSILON
        })
        .collect();
    let covered = per_segment.iter().filter(|hit| **hit).count();
    // An uncovered segment is K1's other escalation set: report its midpoint and its
    // length so the caller can ask the sizing field for elements small enough that the
    // next pass has nodes to snap onto it.
    let uncovered = segments
        .iter()
        .zip(per_segment.iter())
        .filter(|(_, hit)| !**hit)
        .map(|((a, b), _)| (a.add(*b).scale(0.5), norm(b.sub(*a))))
        .collect();
    (segments.len(), covered, uncovered)
}

// AI-FUNC-SUMMARY:
// Purpose: The lattice's unique edge set, each as an ascending node pair.
// Returns: Vec<[u32; 2]> in ascending order.
// Side effects: None.
// Notes: Parallel map to the six pairs of every tet, then a parallel sort and dedup - a permitted
//   shape under PLAN §12.5, because the sort key is total and the result is independent of how the
//   work was split.
pub fn unique_edges(lattice: &Lattice) -> Vec<[u32; 2]> {
    let mut edges: Vec<[u32; 2]> = lattice
        .tets
        .par_iter()
        .flat_map_iter(|tet| {
            [
                [tet[0], tet[1]],
                [tet[0], tet[2]],
                [tet[0], tet[3]],
                [tet[1], tet[2]],
                [tet[1], tet[3]],
                [tet[2], tet[3]],
            ]
            .into_iter()
            .map(|[a, b]| if a <= b { [a, b] } else { [b, a] })
        })
        .collect();
    edges.par_sort_unstable();
    edges.dedup();
    edges
}

/// Tets incident to each node, in CSR form.
struct NodeTets {
    offsets: Vec<u32>,
    tets: Vec<u32>,
}

impl NodeTets {
    // AI-FUNC-SUMMARY: Build the node -> incident tets CSR index; returns NodeTets; side effects: none.
    fn build(lattice: &Lattice) -> NodeTets {
        let mut counts = vec![0u32; lattice.nodes.len() + 1];
        for tet in &lattice.tets {
            for node in tet {
                counts[*node as usize + 1] += 1;
            }
        }
        for index in 1..counts.len() {
            counts[index] += counts[index - 1];
        }
        let mut cursor = counts.clone();
        let mut tets = vec![0u32; lattice.tets.len() * 4];
        for (index, tet) in lattice.tets.iter().enumerate() {
            for node in tet {
                let slot = &mut cursor[*node as usize];
                tets[*slot as usize] = index as u32;
                *slot += 1;
            }
        }
        NodeTets {
            offsets: counts,
            tets,
        }
    }

    // AI-FUNC-SUMMARY: The tets incident to one node; returns a slice; side effects: none.
    fn of(&self, node: u32) -> &[u32] {
        let lo = self.offsets[node as usize] as usize;
        let hi = self.offsets[node as usize + 1] as usize;
        &self.tets[lo..hi]
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Whether moving one node to `target` keeps every incident tet positively oriented.
// Inputs: the current node positions, the tet list, the node's incident tets, the node, and the target.
// Returns: bool - false means the move would invert or flatten a tet (ARB-10).
// Side effects: None.
// Notes: Exact: `orient3d` signs only, per the numerics freeze's S7 "snap would invert" row. A tet
//   that becomes exactly degenerate counts as a rejection - zero volume is not a mesh.
pub fn move_preserves_orientation(
    nodes: &[Vec3],
    tets: &[[u32; 4]],
    incident: &[u32],
    node: u32,
    target: Vec3,
) -> bool {
    for index in incident {
        let tet = tets[*index as usize];
        let corner = |slot: usize| -> Vec3 {
            if tet[slot] == node {
                target
            } else {
                nodes[tet[slot] as usize]
            }
        };
        if orient3d(corner(0), corner(1), corner(2), corner(3)) <= 0.0 {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Crossings
// ---------------------------------------------------------------------------

/// One endpoint a crossing was welded to (invariant K2): the node, the crossing
/// point it should sit at, that point's component, and how far away it currently is.
type Weld = (u32, Vec3, i32, f64);

/// What one lattice edge contributes to a crossing sweep: its surviving crossings,
/// its endpoints found lying exactly on a patch, its K2 welds, and its
/// static-filter escalations.
type EdgeReport = (
    SmallVec<[EdgeCrossing; 2]>,
    SmallVec<[(u32, i32); 2]>,
    SmallVec<[Weld; 2]>,
    usize,
);

// AI-FUNC-SUMMARY:
// Purpose: Every exact edge-patch crossing of one lattice edge, deduplicated.
// Inputs: the edge, the node positions, the geometry, a scratch candidate buffer, and the weld tolerance.
// Returns: (crossings, on-cut endpoints, K2 welds, filter escalations).
// Side effects: Reuses and overwrites the scratch buffer.
// Notes: Two adjacent faces sharing the crossed edge both report the same point; the dedup keeps the
//   lowest face index, which makes the result independent of the grid's bucket order. More than one
//   surviving crossing per component is invariant K1's escalation case
//   (`SPEC_meshgen_geometry.md` §5.1) - counted here, acted on by S8.
fn crossings_of_edge(
    edge: [u32; 2],
    nodes: &[Vec3],
    geometry: &SnapGeometry,
    scratch: &mut Vec<u32>,
    eps: f64,
) -> EdgeReport {
    let p = nodes[edge[0] as usize];
    let q = nodes[edge[1] as usize];
    let length = norm(q.sub(p));
    let (lo, hi) = seg_bounds(p, q);
    geometry.face_grid.query_into(lo, hi, scratch);
    let mut crossings: SmallVec<[EdgeCrossing; 2]> = SmallVec::new();
    let mut on_cut: SmallVec<[(u32, i32); 2]> = SmallVec::new();
    let mut welds: SmallVec<[Weld; 2]> = SmallVec::new();
    let mut uncertain = 0usize;
    for candidate in scratch.iter() {
        let slot = *candidate as usize;
        let component = geometry.face_component[slot];
        match edge_face_hit(p, q, geometry.triangles[slot], &mut uncertain) {
            EdgeHit::None => {}
            EdgeHit::OnStart => on_cut.push((edge[0], component)),
            EdgeHit::OnEnd => on_cut.push((edge[1], component)),
            // Invariant K2: a cut node within the weld tolerance of a parent node is
            // *promoted* to that parent node rather than emitted next to it. The
            // constructed crossing point of a node S7 has already moved onto the
            // patch is within a few ulps of it but is not exactly on it - `orient3d`
            // says so - and without this the stage would report an endless supply of
            // `t = 1e-17` crossings and never converge.
            EdgeHit::Cross { t, point } => {
                if t * length <= eps {
                    welds.push((edge[0], point, component, t * length));
                } else if (1.0 - t) * length <= eps {
                    welds.push((edge[1], point, component, (1.0 - t) * length));
                } else {
                    crossings.push(EdgeCrossing {
                        edge,
                        face: geometry.face_index[slot],
                        component,
                        t,
                        point,
                    });
                }
            }
        }
    }
    // The shared-edge duplicate: same component, same point within the weld
    // tolerance. Keep the lowest face index.
    crossings.sort_by(|a, b| {
        (a.component, a.t, a.face)
            .partial_cmp(&(b.component, b.t, b.face))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut deduped: SmallVec<[EdgeCrossing; 2]> = SmallVec::new();
    for crossing in crossings {
        let duplicate = deduped.last().is_some_and(|previous: &EdgeCrossing| {
            previous.component == crossing.component
                && (previous.t - crossing.t).abs() * length <= eps
        });
        if !duplicate {
            deduped.push(crossing);
        }
    }
    on_cut.sort_unstable();
    on_cut.dedup();
    (deduped, on_cut, welds, uncertain)
}

/// One full crossing sweep over the lattice's edges.
struct CrossingPass {
    crossings: Vec<EdgeCrossing>,
    on_cut: Vec<(u32, i32)>,
    welds: Vec<Weld>,
    multi: usize,
    /// Invariant K1's escalation set: the edges one component crosses more than once.
    multi_edges: Vec<[u32; 2]>,
    uncertain: usize,
}

// AI-FUNC-SUMMARY:
// Purpose: Run `crossings_of_edge` over every edge and concatenate the results in edge order.
// Returns: CrossingPass.
// Side effects: None.
// Notes: Parallel map into an indexed buffer, concatenated in index order (PLAN §12.5) - the output
//   is bit-identical whatever the thread count.
fn crossing_pass(
    edges: &[[u32; 2]],
    nodes: &[Vec3],
    geometry: &SnapGeometry,
    eps: f64,
) -> CrossingPass {
    let per_edge: Vec<EdgeReport> = edges
        .par_iter()
        .map_init(Vec::new, |scratch, edge| {
            crossings_of_edge(*edge, nodes, geometry, scratch, eps)
        })
        .collect();
    let mut pass = CrossingPass {
        crossings: Vec::new(),
        on_cut: Vec::new(),
        welds: Vec::new(),
        multi: 0,
        multi_edges: Vec::new(),
        uncertain: 0,
    };
    for (index, (crossings, on_cut, welds, uncertain)) in per_edge.into_iter().enumerate() {
        let mut per_component: BTreeMap<i32, usize> = BTreeMap::new();
        for crossing in &crossings {
            *per_component.entry(crossing.component).or_insert(0) += 1;
        }
        if per_component.values().any(|count| *count > 1) {
            pass.multi += 1;
            pass.multi_edges.push(edges[index]);
        }
        pass.crossings.extend(crossings);
        pass.on_cut.extend(on_cut);
        pass.welds.extend(welds);
        pass.uncertain += uncertain;
    }
    pass.on_cut.sort_unstable();
    pass.on_cut.dedup();
    pass
}

// ---------------------------------------------------------------------------
// Target selection
// ---------------------------------------------------------------------------

/// A move S7 wants to make, before the inversion test has had a say.
#[derive(Clone, Copy, Debug)]
struct Proposal {
    node: u32,
    target: Vec3,
    kind: TargetKind,
    reference: i32,
    capped: bool,
    box_constrained: bool,
    alternating: bool,
}

// AI-FUNC-SUMMARY:
// Purpose: Restrict a move so a node on the domain boundary stays on the same face, edge, or corner.
// Inputs: the node's current position, the proposed target, and the domain.
// Returns: (constrained target, whether any axis was frozen).
// Side effects: None.
// Notes: Exact equality is the right test: the lattice is built on a dyadic grid over the domain
//   box, so a node on a domain face has that coordinate exactly. A node that merely rounds to the
//   face is interior and free to move.
fn apply_box_constraint(point: Vec3, target: Vec3, min: Vec3, max: Vec3) -> (Vec3, bool) {
    let mut out = target;
    let mut constrained = false;
    if point.x == min.x || point.x == max.x {
        out.x = point.x;
        constrained = true;
    }
    if point.y == min.y || point.y == max.y {
        out.y = point.y;
        constrained = true;
    }
    if point.z == min.z || point.z == max.z {
        out.z = point.z;
        constrained = true;
    }
    (out, constrained)
}

// AI-FUNC-SUMMARY:
// Purpose: The alternating-projection curve target used where the arrangement is degraded (ARB-2).
// Inputs: the start point, the candidate faces grouped by component, and the pass count.
// Returns: Some(point) once both projections agree to within the weld tolerance, else None.
// Side effects: None.
// Notes: Without an explicit intersection curve there is nothing to project onto, so the two
//   patches' closest-point maps are alternated instead; the fixed point of that alternation is a
//   point on both patches, which is what the curve would have been. A fixed pass count keeps it
//   deterministic - convergence is not assumed, it is checked.
fn alternating_projection(
    start: Vec3,
    first: &[[Vec3; 3]],
    second: &[[Vec3; 3]],
    eps: f64,
) -> Option<Vec3> {
    if first.is_empty() || second.is_empty() {
        return None;
    }
    let project = |point: Vec3, set: &[[Vec3; 3]]| -> Vec3 {
        let mut best_point = closest_on_triangle(point, set[0]);
        let mut best_distance = norm(best_point.sub(point));
        for tri in set.iter().skip(1) {
            let candidate = closest_on_triangle(point, *tri);
            let distance = norm(candidate.sub(point));
            if distance < best_distance {
                best_distance = distance;
                best_point = candidate;
            }
        }
        best_point
    };
    let mut point = start;
    for _ in 0..ALTERNATING_PROJECTION_PASSES {
        let a = project(point, first);
        let b = project(a, second);
        if norm(b.sub(a)) <= eps {
            return Some(b);
        }
        point = b;
    }
    let a = project(point, first);
    let b = project(a, second);
    if norm(b.sub(a)) <= eps {
        Some(b)
    } else {
        None
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The surface target of every node the cut would otherwise pass too close to.
// Inputs: one crossing sweep.
// Returns: node -> (the point to move it to, that point's component), one entry per node.
// Side effects: None.
// Notes: Two sources, one rule. A crossing **inside the re-check band** (within 2.5 % of an end)
//   would hand S8 a sliver, so the endpoint is pulled onto it. A crossing **within the weld
//   tolerance** of an end is invariant K2's promotion case (`SPEC_meshgen_geometry.md` §5.1): the
//   cut node *is* the parent node, and moving the parent the last `eps` onto the patch is what makes
//   that true of the coordinates and not just of the bookkeeping. Leaving the weld unmoved is what
//   the first implementation did, and on the review scene it left 1354 nodes declared on-cut while
//   sitting up to `eps` off the surface.
//
//   This is the *only* way a node acquires a surface target, in both the snap pass and the re-check
//   pass. A node with several candidates keeps the closest; the map is ordered, so the acceptance
//   loop that consumes it runs in node order whatever the sweep's scheduling was.
fn band_targets(pass: &CrossingPass) -> BTreeMap<u32, (Vec3, i32)> {
    let mut best: BTreeMap<u32, (f64, Vec3, i32)> = BTreeMap::new();
    let mut record = |node: u32, offset: f64, point: Vec3, component: i32| {
        let entry = best.entry(node).or_insert((f64::INFINITY, point, component));
        if offset < entry.0 {
            *entry = (offset, point, component);
        }
    };
    for crossing in &pass.crossings {
        let (node, offset) = if crossing.t < SNAP_RECHECK_LOW {
            (crossing.edge[0], crossing.t)
        } else if crossing.t > SNAP_RECHECK_HIGH {
            (crossing.edge[1], 1.0 - crossing.t)
        } else {
            continue;
        };
        record(node, offset, crossing.point, crossing.component);
    }
    for (node, point, component, offset) in &pass.welds {
        record(*node, *offset, *point, *component);
    }
    best.into_iter()
        .map(|(node, (_, point, component))| (node, (point, component)))
        .collect()
}

/// A target still in the running for one node.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    kind: TargetKind,
    distance: f64,
    key: (i64, i64, i64),
    target: Vec3,
    reference: i32,
}

// AI-FUNC-SUMMARY:
// Purpose: Keep the better of an incoming target and the current best, under the frozen order.
// Inputs: the running best, the node's position, its cap, the key quantum, and the candidate.
// Returns: None.
// Side effects: Replaces `best` when the candidate wins.
// Notes: The order is (kind weight descending, distance ascending, `NodeKey` ascending). The last
//   term is what makes the choice independent of the order the grid produced candidates in: two
//   targets exactly equidistant from the node - the two sides of a symmetric corner, say - would
//   otherwise be decided by bucket layout.
fn consider(
    best: &mut Option<Candidate>,
    point: Vec3,
    cap: f64,
    quantum: f64,
    kind: TargetKind,
    target: Vec3,
    reference: i32,
) {
    let distance = norm(target.sub(point));
    if !distance.is_finite() || distance > cap {
        return;
    }
    let key = node_key(target, quantum);
    let wins = match best {
        None => true,
        Some(current) => {
            if kind.weight() != current.kind.weight() {
                kind.weight() > current.kind.weight()
            } else if distance != current.distance {
                distance < current.distance
            } else {
                key < current.key
            }
        }
    };
    if wins {
        *best = Some(Candidate {
            kind,
            distance,
            key,
            target,
            reference,
        });
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Choose one node's *feature* target - a corner or a feature curve within its cap.
// Inputs: the node, its position, its cap, the geometry, a scratch buffer, and the domain.
// Returns: Some(Proposal) when a feature is in range.
// Side effects: Reuses the scratch buffer.
// Notes: Corner beats curve *whenever both are in range* - a corner two thirds of the way out still
//   wins over a curve underfoot, because a corner is the feature a mesh most visibly fails to
//   represent. Within a kind the nearest target wins, and an exact tie is broken by ascending
//   `NodeKey` so the choice never depends on the order the grid produced candidates in.
//   `1e7/1e4/1e0` are that order written as numbers.
//
//   **Surfaces are deliberately not a capture target here.** Pulling every node within the cap onto
//   the nearest patch looks like a stronger snap and is in fact a destructive one: on a lattice
//   whose element size is comparable to the feature, *both* endpoints of most crossed edges are
//   inside the cap, every crossing turns into an on-cut vertex, and the surface S8 was going to cut
//   disappears into the lattice. Measured on a sphere at `h = 0.25`: 25 crossed edges before, 2
//   after. A node takes a surface target only when a crossing already sits within 2.5 % of it -
//   `SNAP_RECHECK_LOW`, the frozen re-check band - where the alternative is handing S8 a sliver.
fn propose(
    node: u32,
    point: Vec3,
    cap: f64,
    geometry: &SnapGeometry,
    scratch: &mut Vec<u32>,
    options: &SnapOptions,
) -> Option<Proposal> {
    if !cap.is_finite() || cap <= 0.0 {
        return None;
    }
    let (lo, hi) = ball_bounds(point, cap);
    let quantum = options.eps.max(f64::MIN_POSITIVE);
    let mut best: Option<Candidate> = None;

    geometry.corner_grid.query_into(lo, hi, scratch);
    for candidate in scratch.iter() {
        consider(
            &mut best,
            point,
            cap,
            quantum,
            TargetKind::Corner,
            geometry.corners[*candidate as usize],
            -1,
        );
    }

    if best.is_none() {
        geometry.degraded_grid.query_into(lo, hi, scratch);
        let degraded = scratch
            .iter()
            .any(|candidate| norm(geometry.degraded[*candidate as usize].sub(point)) <= cap);
        let mut alternating = None;
        if degraded {
            geometry.face_grid.query_into(lo, hi, scratch);
            let mut by_component: BTreeMap<i32, Vec<[Vec3; 3]>> = BTreeMap::new();
            for candidate in scratch.iter() {
                let slot = *candidate as usize;
                by_component
                    .entry(geometry.face_component[slot])
                    .or_default()
                    .push(geometry.triangles[slot]);
            }
            if by_component.len() >= 2 {
                let mut sets = by_component.values();
                let first = sets.next().unwrap().clone();
                let second = sets.next().unwrap().clone();
                alternating = alternating_projection(point, &first, &second, options.eps);
            }
        }
        if let Some(target) = alternating {
            let distance = norm(target.sub(point));
            if distance <= cap {
                let (target, box_constrained) = apply_box_constraint(
                    point,
                    target,
                    options.domain_min,
                    options.domain_max,
                );
                if norm(target.sub(point)) <= f64::MIN_POSITIVE {
                    return None;
                }
                return Some(Proposal {
                    node,
                    target,
                    kind: TargetKind::Polyline,
                    reference: -1,
                    capped: false,
                    box_constrained,
                    alternating: true,
                });
            }
        }

        geometry.segment_grid.query_into(lo, hi, scratch);
        for candidate in scratch.iter() {
            let slot = *candidate as usize;
            let (a, b) = geometry.segments[slot];
            consider(
                &mut best,
                point,
                cap,
                quantum,
                TargetKind::Polyline,
                closest_on_segment(point, a, b),
                geometry.segment_curve[slot],
            );
        }
    }

    let Candidate {
        kind,
        distance,
        target,
        reference,
        ..
    } = best?;
    if distance <= 0.0 {
        // Already exactly on the target; nothing to move, and the constraint is
        // recorded by the on-cut sweep instead.
        return None;
    }
    // ARB-11: clamp rather than refuse, and mark the node under-snapped.
    let (target, capped) = if distance > cap {
        (point.add(target.sub(point).scale(cap / distance)), true)
    } else {
        (target, false)
    };
    let (target, box_constrained) =
        apply_box_constraint(point, target, options.domain_min, options.domain_max);
    if target == point {
        return None;
    }
    Some(Proposal {
        node,
        target,
        kind,
        reference,
        capped,
        box_constrained,
        alternating: false,
    })
}

// ---------------------------------------------------------------------------
// The stage
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Run S7 - pull the lattice vertices near an active patch onto its corners, curves and
//   faces, then re-check the crossings the cut will use.
// Inputs: the S5 lattice, the clipped arranged surface, S6's classification, and the domain.
// Returns: Snapped - moved nodes, per-node constraints, the surviving crossings, and diagnostics.
// Side effects: None (warnings are collected into the result, not printed).
// Notes: Determinism (R-P2) comes from the shape, not from luck. Every move *proposal* is a pure
//   function of the pre-pass state and is computed in parallel; the *acceptance* is a serial sweep
//   in ascending node index, because whether a move inverts a tet depends on whether that tet's
//   other corners have already moved. Accepting in parallel would make the outcome depend on the
//   scheduler. The serial half is cheap - a handful of `orient3d` per candidate node - and only
//   candidate nodes take part.
//
//   The stage runs two crossing sweeps. The first finds the candidate set (an endpoint of a crossed
//   edge is near a patch by definition). The second, after snapping, feeds the 97.5/2.5 % re-check:
//   a crossing that now sits within 2.5 % of an end would hand S8 a sliver, so the endpoint is
//   promoted the rest of the way onto the patch (invariant K2 - promote, never delete). Promotion
//   removes that crossing, so the affected edges are swept once more for the final list.
pub fn snap_lattice(
    lattice: &Lattice,
    surface: &ArrangedSurface,
    classification: &Classification,
    options: &SnapOptions,
) -> Snapped {
    let geometry = gather_geometry(surface, classification);
    let edges = unique_edges(lattice);
    let incidence = NodeTets::build(lattice);
    let mut nodes = lattice.nodes.clone();

    let mut stats = SnapStats {
        n_nodes: nodes.len(),
        n_edges: edges.len(),
        ..Default::default()
    };
    let mut warnings = Vec::new();

    // The shortest incident edge of every node, which sets its motion cap.
    let mut l_min = vec![f64::INFINITY; nodes.len()];
    for edge in &edges {
        let length = norm(nodes[edge[1] as usize].sub(nodes[edge[0] as usize]));
        for node in edge {
            let slot = &mut l_min[*node as usize];
            if length < *slot {
                *slot = length;
            }
        }
    }

    // --- pass 1: where does the surface cross the lattice? ---
    let first = crossing_pass(&edges, &nodes, &geometry, options.eps);
    stats.n_filter_uncertain += first.uncertain;
    let mut candidates: Vec<u32> = Vec::with_capacity(first.crossings.len() * 2);
    for crossing in &first.crossings {
        candidates.push(crossing.edge[0]);
        candidates.push(crossing.edge[1]);
    }
    candidates.par_sort_unstable();
    candidates.dedup();
    stats.n_candidates = candidates.len();

    // --- the snap pass: propose in parallel, accept in node order ---
    let features: Vec<Option<Proposal>> = candidates
        .par_iter()
        .map_init(Vec::new, |scratch, node| {
            propose(
                *node,
                nodes[*node as usize],
                SNAP_MOTION_CAP * l_min[*node as usize],
                &geometry,
                scratch,
                options,
            )
        })
        .collect();

    let mut constraint_kind = vec![TargetKind::Free as u8; nodes.len()];
    let mut constraint_ref = vec![-1i32; nodes.len()];
    let mut motion = vec![0.0f64; nodes.len()];
    let mut under_snapped = Vec::new();

    // A node with no feature in reach takes a surface target only where a crossing
    // already sits inside the re-check band - that is the whole surface rule, and
    // the reason S8 still has a surface to cut afterwards.
    let band = band_targets(&first);
    let mut proposals: Vec<Proposal> = Vec::with_capacity(candidates.len());
    for (node, feature) in candidates.iter().zip(features) {
        if let Some(proposal) = feature {
            proposals.push(proposal);
            continue;
        }
        let Some((target, component)) = band.get(node).copied() else {
            continue;
        };
        let (target, box_constrained) = apply_box_constraint(
            nodes[*node as usize],
            target,
            options.domain_min,
            options.domain_max,
        );
        if target == nodes[*node as usize] {
            continue;
        }
        if norm(target.sub(nodes[*node as usize])) > SNAP_MOTION_CAP * l_min[*node as usize] {
            stats.n_recheck_residual += 1;
            continue;
        }
        proposals.push(Proposal {
            node: *node,
            target,
            kind: TargetKind::Surface,
            reference: component,
            capped: false,
            box_constrained,
            alternating: false,
        });
    }

    for proposal in proposals {
        let node = proposal.node;
        if !move_preserves_orientation(
            &nodes,
            &lattice.tets,
            incidence.of(node),
            node,
            proposal.target,
        ) {
            stats.n_rejected += 1;
            continue;
        }
        nodes[node as usize] = proposal.target;
        constraint_kind[node as usize] = proposal.kind as u8;
        constraint_ref[node as usize] = proposal.reference;
        motion[node as usize] = norm(proposal.target.sub(lattice.nodes[node as usize]));
        match proposal.kind {
            TargetKind::Corner => stats.n_snapped_corner += 1,
            TargetKind::Polyline => stats.n_snapped_curve += 1,
            _ => stats.n_snapped_surface += 1,
        }
        if proposal.capped {
            stats.n_capped += 1;
            under_snapped.push(node);
        }
        if proposal.box_constrained {
            stats.n_box_constrained += 1;
        }
        if proposal.alternating {
            stats.n_alternating_projection += 1;
        }
    }

    // --- pass 2 + the 97.5/2.5 % re-check ---
    let second = crossing_pass(&edges, &nodes, &geometry, options.eps);
    stats.n_filter_uncertain += second.uncertain;
    let mut promoted_nodes: Vec<u32> = Vec::new();
    for (node, (target, component)) in band_targets(&second) {
        // A node already pinned to a corner or a curve is not pulled off it by a
        // surface promotion: the higher-priority feature is the one that matters.
        if constraint_kind[node as usize] >= TargetKind::Polyline as u8 {
            continue;
        }
        let (target, box_constrained) = apply_box_constraint(
            nodes[node as usize],
            target,
            options.domain_min,
            options.domain_max,
        );
        if target == nodes[node as usize] {
            continue;
        }
        // The cap is on the node's *total* displacement from where S5 put it, not
        // on this move alone: a promotion that pushed the total past the cap would
        // silently break the one bound the rest of the pipeline is allowed to
        // assume. A promotion cannot be clamped instead - a partial promotion does
        // not reach the patch, which is the entire point of it - so it is dropped
        // and the crossing stays in the band for S8 to deal with.
        if norm(target.sub(lattice.nodes[node as usize]))
            > SNAP_MOTION_CAP * l_min[node as usize]
        {
            stats.n_recheck_residual += 1;
            continue;
        }
        if !move_preserves_orientation(&nodes, &lattice.tets, incidence.of(node), node, target) {
            stats.n_rejected += 1;
            stats.n_recheck_residual += 1;
            continue;
        }
        nodes[node as usize] = target;
        motion[node as usize] = norm(target.sub(lattice.nodes[node as usize]));
        constraint_kind[node as usize] = TargetKind::Surface as u8;
        constraint_ref[node as usize] = component;
        stats.n_recheck_promoted += 1;
        if box_constrained {
            stats.n_box_constrained += 1;
        }
        promoted_nodes.push(node);
    }

    // --- the final crossing list, consistent with the emitted coordinates ---
    let final_pass = if promoted_nodes.is_empty() {
        second
    } else {
        crossing_pass(&edges, &nodes, &geometry, options.eps)
    };
    stats.n_crossings = final_pass.crossings.len();
    stats.n_multi_crossing_edges = final_pass.multi;
    stats.n_crossed_edges = {
        let mut edges: Vec<[u32; 2]> = final_pass
            .crossings
            .iter()
            .map(|crossing| crossing.edge)
            .collect();
        edges.dedup();
        edges.len()
    };
    stats.n_recheck_residual += final_pass
        .crossings
        .iter()
        .filter(|crossing| crossing.t < SNAP_RECHECK_LOW || crossing.t > SNAP_RECHECK_HIGH)
        .count();
    // A welded crossing *is* an on-cut vertex - K2 promotes it to the parent node -
    // so S8 must receive it as one. After the snap pass the two are the same set
    // seen from two sides: the node sits on the patch, and whether `orient3d` calls
    // that exactly zero or one ulp off decides only which list the sweep put it in.
    let mut final_on_cut = final_pass.on_cut.clone();
    final_on_cut.extend(
        final_pass
            .welds
            .iter()
            .map(|(node, _, component, _)| (*node, *component)),
    );
    // A node this stage deliberately placed on a feature curve or a corner lies on the
    // boundary of every component that curve belongs to - by construction, not by luck.
    // `on_cut` is otherwise derived from `EdgeHit::OnStart/OnEnd`, i.e. re-discovered from
    // the intersection predicate afterwards, and the predicate can round the other way on
    // a node the snapper put there on purpose. When it does, S8 asks S6 for the node's
    // side instead, gets a label computed at the *pre-snap* position, and reports an edge
    // that straddles with no crossing on it. That is 77.6% of the S6/S7 disagreement, and
    // it scales exactly with the number of nodes this stage moves (PLAN §0.2).
    for (node, kind) in constraint_kind.iter().enumerate() {
        let reference = constraint_ref[node];
        if reference < 0 {
            continue;
        }
        if *kind == TargetKind::Polyline as u8 || *kind == TargetKind::Corner as u8 {
            let Some(curve) = surface.curves.get(reference as usize) else {
                continue;
            };
            for component in &curve.components {
                final_on_cut.push((node as u32, *component));
            }
        } else if *kind == TargetKind::Surface as u8 {
            // A surface target's reference is the component itself. These are the nodes
            // this stage moved *onto* a patch, so they are on that component's boundary by
            // construction - the largest group of them, and the one omitted when this was
            // first written.
            final_on_cut.push((node as u32, reference));
        }
    }
    // **The disagreement is not a boundary-tolerance problem.** Marking every node within
    // the numerical envelope (`eps`, 1.7e-4 here - not tiny) of a patch as `on_cut` was
    // tried on 2026-08-06 and changes *nothing*: on-cut stays 2,060 of 2,060 and the
    // disagreement stays 30,852. No node is near a patch beyond those already found, so
    // the edges S6 calls straddling with no crossing on them have endpoints that are
    // genuinely far from the surface. **S7 is missing crossings on edges that genuinely
    // change sign** - a search or filter problem in the crossing pass, not a predicate
    // tolerance. Reverted; the pass cost a full nearest-surface sweep for no effect.

    final_on_cut.sort_unstable();
    final_on_cut.dedup();
    stats.n_on_cut = final_on_cut.len();

    // A node lying exactly on a patch is bound to it even if it never moved.
    for (node, component) in &final_on_cut {
        if constraint_kind[*node as usize] == TargetKind::Free as u8 {
            constraint_kind[*node as usize] = TargetKind::Surface as u8;
            constraint_ref[*node as usize] = *component;
        }
    }
    // Every remaining node on the domain boundary carries the box-face constraint,
    // which is what keeps S8 and S9 from moving the domain.
    for (index, point) in nodes.iter().enumerate() {
        if constraint_kind[index] != TargetKind::Free as u8 {
            continue;
        }
        let on_box = point.x == options.domain_min.x
            || point.x == options.domain_max.x
            || point.y == options.domain_min.y
            || point.y == options.domain_max.y
            || point.z == options.domain_min.z
            || point.z == options.domain_max.z;
        if on_box {
            constraint_kind[index] = TargetKind::BoxFace as u8;
        }
    }

    let moved: Vec<f64> = motion.iter().copied().filter(|value| *value > 0.0).collect();
    stats.max_motion = moved.iter().copied().fold(0.0, f64::max);
    stats.mean_motion = if moved.is_empty() {
        0.0
    } else {
        moved.iter().sum::<f64>() / moved.len() as f64
    };

    if stats.n_rejected > 0 {
        warnings.push(format!(
            "[SNAP-REJ] {} move(s) rejected because they would invert an incident tet",
            stats.n_rejected
        ));
    }
    if stats.n_capped > 0 {
        warnings.push(format!(
            "[SNAP-CAP] {} node(s) reached the {:.0}% motion cap and stayed under-snapped",
            stats.n_capped,
            SNAP_MOTION_CAP * 100.0
        ));
    }
    let (n_curve_segments, n_curve_segments_covered, uncovered) =
        curve_coverage(&geometry.segments, &nodes, &edges, options.eps);
    stats.n_curve_segments = n_curve_segments;
    stats.n_curve_segments_covered = n_curve_segments_covered;
    // K1's remedy set: the doubly-crossed edges and the uncovered curve segments, both
    // asking the sizing field for elements half the size that failed here.
    let mut refine_requests: Vec<(Vec3, f64)> = final_pass
        .multi_edges
        .iter()
        .map(|edge| {
            let (a, b) = (nodes[edge[0] as usize], nodes[edge[1] as usize]);
            (a.add(b).scale(0.5), norm(b.sub(a)))
        })
        .collect();
    refine_requests.extend(uncovered);
    if n_curve_segments_covered < n_curve_segments {
        warnings.push(format!(
            "[SNAP-CURVE] {} of {} locked curve segment(s) are not covered by a chain of mesh edges; S8 cuts them instead of conforming to them",
            n_curve_segments - n_curve_segments_covered,
            n_curve_segments
        ));
    }
    if stats.n_multi_crossing_edges > 0 {
        warnings.push(format!(
            "[CUT-CASE] {} edge(s) are crossed more than once by one component (invariant K1); S8 must refine or escalate them",
            stats.n_multi_crossing_edges
        ));
    }

    Snapped {
        nodes,
        constraint_kind,
        constraint_ref,
        motion,
        under_snapped,
        crossings: final_pass.crossings,
        on_cut: final_on_cut,
        refine_requests,
        warnings,
        stats,
    }
}

// ---------------------------------------------------------------------------
// s07_snapped
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Encode the snapped lattice as the `s07_snapped` snapshot VTU.
// Inputs: the lattice (for connectivity), S7's result, S6's classification, and the run's components.
// Returns: VtuDoc of `VTK_TETRA` cells with S6's region keys on the moved coordinates.
// Side effects: None.
// Notes: The connectivity is still S5's - S7 moves nodes and cuts nothing - so the region keys are
//   S6's and still preliminary. `constraint_kind`/`constraint_ref` are the stage's own output and
//   carry the frozen §2.2 encoding. `snap_motion` is a debug point array in the spirit of
//   `separation_t` and `sizing_h`: recorded as an additive amendment to
//   `SPEC_meshgen_contracts.md` §2.2 because the distance a node moved is the one quantity a
//   reviewer needs to see to judge the stage and it cannot be recomputed from the snapshot alone.
pub fn snapped_to_doc(
    lattice: &Lattice,
    snapped: &Snapped,
    classification: &Classification,
    components: &[ArrangeComponent],
) -> VtuDoc {
    let mut doc = VtuDoc {
        points: snapped.nodes.clone(),
        ..Default::default()
    };
    for tet in &lattice.tets {
        doc.connectivity.extend(tet.iter().map(|node| *node as i64));
        doc.offsets.push(doc.connectivity.len() as i64);
        doc.types.push(VTK_TETRA);
    }
    let cells = lattice.tets.len();
    let priority_of: BTreeMap<i32, u32> = components
        .iter()
        .map(|component| (component.x, component.priority))
        .collect();

    doc.cell_data
        .push(DataArray::scalar("cell_kind", ArrayData::U8(vec![0; cells])));
    doc.cell_data.push(DataArray::scalar(
        "region_key",
        ArrayData::I32(classification.region_key.clone()),
    ));
    doc.cell_data.push(DataArray::scalar(
        "partition_id",
        ArrayData::I32(vec![0; cells]),
    ));
    doc.cell_data
        .push(DataArray::scalar("regime", ArrayData::U8(vec![0; cells])));
    doc.cell_data.push(DataArray::scalar(
        "face_tag_key",
        ArrayData::I32(vec![-1; cells]),
    ));
    doc.cell_data.push(DataArray::scalar(
        "curve_id",
        ArrayData::I32(vec![-1; cells]),
    ));
    doc.cell_data.push(DataArray::scalar(
        "provenance",
        ArrayData::U8(
            classification
                .records
                .iter()
                .map(|record| record.provenance as u8)
                .collect(),
        ),
    ));

    let points = doc.points.len();
    doc.point_data.push(DataArray::scalar(
        "n_id_key",
        ArrayData::I32(vec![0; points]),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_kind",
        ArrayData::U8(snapped.constraint_kind.clone()),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_ref",
        ArrayData::I32(snapped.constraint_ref.clone()),
    ));
    doc.point_data.push(DataArray::scalar(
        "snap_motion",
        ArrayData::F32(snapped.motion.iter().map(|value| *value as f32).collect()),
    ));

    let mut offsets: Vec<i64> = Vec::with_capacity(classification.region_sets.len());
    let mut flat: Vec<i32> = Vec::new();
    let mut priorities: Vec<u32> = Vec::with_capacity(classification.region_sets.len());
    for key in &classification.region_sets {
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
    push_field(&mut doc, "NIdSetOffsets", 1, ArrayData::I64(vec![1]));
    push_field(&mut doc, "NIdSetComponents", 1, ArrayData::I32(vec![0]));
    push_field(&mut doc, "FaceTagOffsets", 1, ArrayData::I64(Vec::new()));
    push_field(&mut doc, "FaceTagComponents", 1, ArrayData::I32(Vec::new()));
    push_field(&mut doc, "FaceTagOrientation", 1, ArrayData::I32(Vec::new()));
    push_field(&mut doc, "FaceTagKind", 1, ArrayData::U8(Vec::new()));
    push_field(&mut doc, "FaceTagSideElems", 2, ArrayData::I32(Vec::new()));
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
    push_field(&mut doc, "CurveKind", 1, ArrayData::U8(Vec::new()));
    push_field(&mut doc, "CurveCompOffsets", 1, ArrayData::I64(Vec::new()));
    push_field(
        &mut doc,
        "CurveCompComponents",
        1,
        ArrayData::I32(Vec::new()),
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
