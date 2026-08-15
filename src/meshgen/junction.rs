//! Junction and escalated cells (G6-0's gate outcome, G6-4).
//!
//! §6's case table covers a cell crossed by **one** patch in one of six
//! configurations. Everything else - two patches, an intersection curve, an edge
//! crossed twice (K1), a state §5.2 calls illegal, a cut the dry-run rejected - lands
//! here. Those cells cannot simply be left alone: a cell that keeps its four flat
//! faces next to a neighbour that split them has hanging nodes on the shared face,
//! and the mesh stops being conforming. On the review scene that is 998 cells and
//! 4,577 hanging nodes.
//!
//! What this module does is decided by gate **G6-0** (`PLAN_mesh_generation.md`
//! §0.4): a full local-PLC constrained tetrahedralisation with Steiner-point
//! insertion and constraint recovery was **not** adopted, and the plan's named
//! fallback was taken instead, in the following form.
//!
//! > **Every cell's boundary is triangulated by a pure function of the face's own
//! > state, and its interior is a fan from a Steiner point at the cell's centroid.**
//!
//! That gives, without any constraint recovery:
//!
//! - **Conformity (J1) by construction.** Two cells sharing a face compute
//!   `face_triangulation` from the same face state and get the same triangles, so
//!   the shared face matches whether either, both, or neither cell was escalated.
//!   No cache and no fingerprint check are needed, because there is nothing to
//!   cache: the function *is* the invariant.
//! - **Validity always.** A parent tet is convex, so its centroid sees every point
//!   of its boundary; coning a boundary triangulation to it yields positively
//!   oriented tets whatever the boundary looks like.
//! - **Determinism.** The centroid is the exact average of four coordinates, which
//!   is reproducible bit for bit (§7.4), and every face choice is a `NodeKey`
//!   comparison.
//!
//! What it gives up is the cut *inside* those cells: the patch is not a face of the
//! mesh there, so the material boundary is chamfered by at most one cell (`<= h`).
//! That is the degraded mode the plan describes for the fallback, and it is logged
//! per cell (`[JCT-FALLBACK]`) and reported, never silent.

use crate::meshgen::cut::{face_split, FaceCutState, NodeKey};
use crate::types::Vec3;
use smallvec::SmallVec;

/// The four faces of a tet, as slot triples. Slot `k` is the face opposite node `k`.
pub const TET_FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];

/// One escalated cell's replacement.
#[derive(Clone, Debug, Default)]
pub struct FannedCell {
    /// The boundary triangulation, in the order the faces were visited.
    pub boundary: SmallVec<[[u32; 3]; 16]>,
    /// The tets of the fan, referring to the Steiner node by the id the caller gave it.
    pub tets: SmallVec<[[u32; 4]; 16]>,
    /// True when a face used the generic fan instead of `face_split`'s table -
    /// i.e. the face state was not one the frozen table covers.
    pub generic_faces: usize,
}

/// How one parent face is triangulated.
pub enum FaceMesh {
    /// The frozen §5.2 table applies, and these are its triangles.
    Table(SmallVec<[[u32; 3]; 4]>),
    /// Two patches cross this face. Both chords are kept as edges of the
    /// triangulation, around a Steiner point at their meeting - the caller allocates
    /// it, shared between the two cells.
    Crossed(CrossedFace),
    /// The face state is not one the table covers and no pair of chords crosses it.
    /// The face is triangulated by coning its boundary loop to a Steiner point at the
    /// face's centroid, which the caller allocates - shared between the two cells.
    LoopFan(SmallVec<[u32; 8]>),
}

// AI-FUNC-SUMMARY:
// Purpose: Decide how one parent face is triangulated - the frozen table where it applies, a
//   centroid fan of its boundary loop where it does not.
// Inputs: the face's cut state in the canonical frame (None when it is not expressible), the face's
//   ordered boundary loop, and the key table.
// Returns: FaceMesh.
// Side effects: None.
// Notes: **This is the conformity mechanism.** It is a pure function of the face's own state, so the
//   two cells sharing a face necessarily agree - Invariant J1 without a cache, and Invariant J2
//   exactly, because where the frozen table applies this *is* the frozen table rather than a second
//   implementation of it.
//
//   The fan is coned to the face's *centroid* rather than to one of the loop's own vertices, and
//   that is not a stylistic choice. Fanning from a vertex emits a triangle for every consecutive
//   pair, and when the apex and that pair all lie on one parent edge - which happens whenever the
//   apex is a corner with a cut node on an incident edge - the triangle has zero area. Those pieces
//   then fail the orientation test, get dropped, and leave a hole in the cell: 8 dropped pieces
//   produced 12 leaking faces on the first measurement. A strictly interior apex cannot be collinear
//   with two boundary points, so the centroid fan has no such case.
pub fn face_mesh(
    state: Option<&FaceCutState>,
    loop_nodes: &[u32],
    keys: &[NodeKey],
    crossed: Option<&CrossedFace>,
) -> FaceMesh {
    if let Some(triangles) = state.and_then(|state| face_split(state, keys)) {
        return FaceMesh::Table(triangles);
    }
    if let Some(face) = crossed {
        return FaceMesh::Crossed(face.clone());
    }
    FaceMesh::LoopFan(loop_nodes.iter().copied().collect())
}

// AI-FUNC-SUMMARY:
// Purpose: Cone a face's boundary loop to a Steiner point at its centroid.
// Inputs: the loop and the centroid's node id.
// Returns: one triangle per loop edge.
// Side effects: None.
pub fn loop_fan(loop_nodes: &[u32], centroid_id: u32) -> SmallVec<[[u32; 3]; 8]> {
    let count = loop_nodes.len();
    (0..count)
        .map(|k| [centroid_id, loop_nodes[k], loop_nodes[(k + 1) % count]])
        .collect()
}

// AI-FUNC-SUMMARY:
// Purpose: The centroid of a face's three corners - the Steiner point `loop_fan` cones to.
// Returns: Vec3.
// Side effects: None.
// Notes: An exact average of the three corner coordinates, taken in the canonical (key-sorted)
//   order, so the two cells sharing the face compute bit-identical coordinates (§7.4).
pub fn face_centroid(corners: [u32; 3], nodes: &[Vec3]) -> Vec3 {
    nodes[corners[0] as usize]
        .add(nodes[corners[1] as usize])
        .add(nodes[corners[2] as usize])
        .scale(1.0 / 3.0)
}

// AI-FUNC-SUMMARY:
// Purpose: Mesh one escalated cell as a fan from its centroid over an already-built conforming boundary.
// Inputs: the cell's boundary triangulation and the id the caller assigned to its centroid Steiner node.
// Returns: FannedCell - the boundary triangles, the tets, and how many faces took the generic path.
// Side effects: None.
// Notes: The tets are emitted with the centroid last and are *not* oriented here; the caller's
//   guarded dry-run does that, as it does for the §6 path. Every tet is non-degenerate whatever the
//   boundary looks like, because the parent is convex and the centroid is strictly interior - which
//   is the property that makes this fallback total, and the reason it was preferred to sequential
//   cutting with curve-node pinning (which is valid only where the pinning succeeds).
pub fn fan_cell(boundary: &[[u32; 3]], centroid_id: u32) -> FannedCell {
    let mut out = FannedCell::default();
    for triangle in boundary {
        out.boundary.push(*triangle);
        out.tets
            .push([triangle[0], triangle[1], triangle[2], centroid_id]);
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: The Steiner point of an escalated cell - the exact average of its four corners.
// Returns: Vec3.
// Side effects: None.
// Notes: §7.4 requires a Steiner coordinate to be an exact average of existing node coordinates so
//   that it is reproducible bit for bit. Summing the four corners and dividing by four is exact in
//   the sense that matters: the same four inputs in the same order always give the same result, on
//   any thread and in any run.
pub fn cell_centroid(tet: [u32; 4], nodes: &[Vec3]) -> Vec3 {
    nodes[tet[0] as usize]
        .add(nodes[tet[1] as usize])
        .add(nodes[tet[2] as usize])
        .add(nodes[tet[3] as usize])
        .scale(0.25)
}

// AI-FUNC-SUMMARY:
// Purpose: Chain the once-used edges of a closed-except-for-one-hole polygon soup into a single cycle - the cap the cutting surface leaves.
// Inputs: the soup's triangles.
// Returns: the cycle's nodes in order, or None when the open boundary is not exactly one simple loop.
// Side effects: None.
// Notes: **Undirected** pairs are counted. The soup is assembled face by face by `face_mesh` and
//   carries no agreed winding - the two cells sharing a face each emit it in their own order - so
//   cancelling directed edges would call an interior edge open whenever its two triangles happened
//   to agree, and reject caps that are perfectly good. An edge used once bounds the hole; used
//   twice it is interior. This was the top refusal (468 of A-3's 884) until it was corrected.
// AI-FUNC-SUMMARY:
// Purpose: Every hole of an open triangle soup, as simple node cycles.
// Inputs: the soup.
// Returns: one cycle per hole in canonical (lowest-node-first) order, or None when any hole is
//   pinched or branching.
// Side effects: None.
// Notes: One surface can bound more than one hole, and assuming otherwise is what kept the band path
//   shut. A cell a component crosses **twice** - the two walls of a thin gap - has its material
//   bounded by two caps, so the single-loop test read it as "not one simple loop" and every such
//   cell fell back to the centroid fan that chamfers the gap away. Counting is undirected because
//   the soup has no agreed winding; the cycles come out in node order so R-P2 sees the same list
//   however the soup was assembled.
fn open_boundary_loops(soup: &[[u32; 3]]) -> Option<Vec<Vec<u32>>> {
    let mut used: std::collections::BTreeMap<[u32; 2], usize> = std::collections::BTreeMap::new();
    for triangle in soup {
        for slot in 0..3 {
            let (a, b) = (triangle[slot], triangle[(slot + 1) % 3]);
            let key = if a < b { [a, b] } else { [b, a] };
            *used.entry(key).or_insert(0) += 1;
        }
    }
    let open: Vec<[u32; 2]> = used
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|(edge, _)| edge)
        .collect();
    if open.len() < 3 {
        return None;
    }
    let mut next: std::collections::BTreeMap<u32, SmallVec<[u32; 2]>> =
        std::collections::BTreeMap::new();
    for edge in &open {
        next.entry(edge[0]).or_default().push(edge[1]);
        next.entry(edge[1]).or_default().push(edge[0]);
    }
    // Every node of a simple loop has exactly two neighbours; anything else is a
    // pinched or branching hole this path must not try to cap.
    if next.values().any(|ends| ends.len() != 2) {
        return None;
    }
    let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut loops: Vec<Vec<u32>> = Vec::new();
    for &start in next.keys() {
        if seen.contains(&start) {
            continue;
        }
        let mut cycle = vec![start];
        seen.insert(start);
        let mut previous = u32::MAX;
        let mut here = start;
        loop {
            let ends = next.get(&here)?;
            let step = if ends[0] == previous { ends[1] } else { ends[0] };
            if step == start {
                break;
            }
            if cycle.len() > open.len() {
                return None;
            }
            if !seen.insert(step) {
                return None;
            }
            cycle.push(step);
            previous = here;
            here = step;
        }
        if cycle.len() < 3 {
            return None;
        }
        loops.push(cycle);
    }
    (seen.len() == open.len()).then_some(loops)
}

// AI-FUNC-SUMMARY:
// Purpose: Whether a triangle soup is a closed surface - every undirected edge used exactly twice.
// Inputs: the soup.
// Returns: true when nothing is left open.
// Side effects: None.
// Notes: A piece that is not closed cannot be fanned into tets without leaving the volume open along
//   its hole, which surfaces as `[V3]`'s boundary leaks and interface cracks. §7.6's contract is that
//   every refusal falls back to the unconditionally valid fan, so this is checked before the split is
//   accepted rather than repaired afterwards.
pub fn soup_is_closed(soup: &[[u32; 3]]) -> bool {
    let mut used: std::collections::BTreeMap<[u32; 2], usize> = std::collections::BTreeMap::new();
    for triangle in soup {
        for slot in 0..3 {
            let (a, b) = (triangle[slot], triangle[(slot + 1) % 3]);
            let key = if a < b { [a, b] } else { [b, a] };
            *used.entry(key).or_insert(0) += 1;
        }
    }
    !used.is_empty() && used.values().all(|count| *count == 2)
}

// AI-FUNC-SUMMARY:
// Purpose: Split a triangle soup into its edge-connected pieces.
// Inputs: the soup.
// Returns: the pieces, each a soup, ordered by their lowest node so the list is canonical.
// Side effects: None.
// Notes: Cutting a cell with two walls leaves the *outside* in two parts - one before the near wall
//   and one past the far wall - which is one soup with two components until they are separated.
//   Emitting it whole would make a single "piece" out of two disjoint solids and give the pair one
//   material tag.
pub fn split_soup_components(soup: &[[u32; 3]]) -> Vec<SmallVec<[[u32; 3]; 16]>> {
    let mut owner: Vec<usize> = (0..soup.len()).collect();
    fn find(owner: &mut [usize], mut index: usize) -> usize {
        while owner[index] != index {
            owner[index] = owner[owner[index]];
            index = owner[index];
        }
        index
    }
    let mut by_edge: std::collections::BTreeMap<[u32; 2], Vec<usize>> =
        std::collections::BTreeMap::new();
    for (index, triangle) in soup.iter().enumerate() {
        for slot in 0..3 {
            let (a, b) = (triangle[slot], triangle[(slot + 1) % 3]);
            let key = if a < b { [a, b] } else { [b, a] };
            by_edge.entry(key).or_default().push(index);
        }
    }
    for members in by_edge.values() {
        for pair in members.windows(2) {
            let (left, right) = (find(&mut owner, pair[0]), find(&mut owner, pair[1]));
            if left != right {
                owner[left] = right;
            }
        }
    }
    let mut groups: std::collections::BTreeMap<u32, SmallVec<[[u32; 3]; 16]>> =
        std::collections::BTreeMap::new();
    let mut root_key: std::collections::BTreeMap<usize, u32> = std::collections::BTreeMap::new();
    for (index, triangle) in soup.iter().enumerate() {
        let root = find(&mut owner, index);
        let lowest = *triangle.iter().min().unwrap_or(&u32::MAX);
        let entry = root_key.entry(root).or_insert(lowest);
        *entry = (*entry).min(lowest);
    }
    for (index, triangle) in soup.iter().enumerate() {
        let root = find(&mut owner, index);
        let key = root_key[&root];
        groups.entry(key).or_default().push(*triangle);
    }
    groups.into_values().collect()
}

// AI-FUNC-SUMMARY: Report why a §7.6 split was refused when `RUSTMSPT_JCT_DIAG` is set; returns None so callers can `return bail(...)`; side effects: writes to stdout under the env var.
fn bail<T>(reason: &str) -> Option<T> {
    if std::env::var_os("RUSTMSPT_JCT_DIAG").is_some() {
        println!("[JCT-DIAG] split refused: {reason}");
    }
    None
}


// AI-FUNC-SUMMARY:
// Purpose: Partition an escalated cell's boundary soup by one surface, reporting **every** cap it
//   leaves rather than assuming there is one.
// Inputs: the soup and a per-node side oracle (`None` means the node lies on the surface).
// Returns: (inside, outside, cap loops) with both sides still open along the caps, or None when the
//   split is not clean and the caller must keep the soup whole.
// Side effects: None.
// Notes: `SPEC_meshgen_geometry.md` §7.6 with the single-cap assumption lifted, which is what a thin
//   gap needs: a lattice cell straddling a plate is crossed by that plate's surface **twice**, so the
//   material between the walls is bounded by two caps and the one-loop test refused it outright.
//   Every such cell then fell back to the centroid fan, which cones the whole cell to one point and
//   chamfers the gap away - the "band declared, no band elements" behaviour, measured as 538 declined
//   cells on the sheet fixture with `FaceShape` for all but 26 of them. The two sides must still
//   report the same caps: that is what makes the cut one shared set of faces rather than a crack.
#[allow(clippy::type_complexity)]
pub fn split_soup_by_surface(
    soup: &[[u32; 3]],
    side_of: &dyn Fn(u32) -> Option<bool>,
    side_of_face: &dyn Fn(&[u32; 3]) -> Option<bool>,
) -> Option<(
    SmallVec<[[u32; 3]; 16]>,
    SmallVec<[[u32; 3]; 16]>,
    Vec<Vec<u32>>,
)> {
    let mut inside: SmallVec<[[u32; 3]; 16]> = SmallVec::new();
    let mut outside: SmallVec<[[u32; 3]; 16]> = SmallVec::new();
    for triangle in soup {
        let mut side: Option<bool> = None;
        for node in triangle {
            match side_of(*node) {
                None => {}
                Some(here) => match side {
                    None => side = Some(here),
                    Some(there) if there == here => {}
                    // A triangle with corners on both sides and no cut node between them is a
                    // **face** that was not split where this surface crosses it, so the cell
                    // split cannot repair it. Declining costs the whole cell its cut, which
                    // chamfers more than classifying the one triangle by its own centroid
                    // does - so classify it, and let the cap guard below catch the case that
                    // actually breaks conformity.
                    //
                    // This relaxation was measured and rejected once before, on the grounds
                    // that it cost `[V3]` (2 faces carrying four tets, 1 hanging node, 6
                    // non-manifold edges). That was the right measurement and the wrong
                    // conclusion: the leak is a **cap landing on the cell's own outer
                    // boundary**, which the neighbour then fans as well, and it is now
                    // refused explicitly. With that guard in place the relaxation is
                    // `[V3]`-clean on all nine acceptance cases and takes `[V6]` A-6a
                    // 37 -> 21 and A-6b 60 -> 47 for A-3 79 -> 80.
                    Some(_) => {
                        if std::env::var_os("RUSTMSPT_NO_MIXED_BY_CENTRE").is_none() {
                            side = None;
                            break;
                        }
                        return bail(&format!("mixed sides on one triangle {triangle:?}"));
                    }
                },
            }
        }
        // Every corner on the surface does not mean the triangle has no side. A face a
        // surface crosses **twice** is triangulated with both chords as edges, and the
        // strip between them has all four corners on the surface while lying squarely in
        // the material between the two walls - which is the whole of a thin gap. Refusing
        // there declined every band cell. The triangle's own interior is the thing being
        // classified, so ask about that.
        let side = match side {
            Some(side) => Some(side),
            None => side_of_face(triangle),
        };
        match side {
            Some(true) => inside.push(*triangle),
            Some(false) => outside.push(*triangle),
            None => return bail("triangle wholly on the surface"),
        }
    }
    if inside.is_empty() || outside.is_empty() {
        return bail(&format!(
            "one side empty ({} in / {} out of {} triangle(s))",
            inside.len(),
            outside.len(),
            soup.len()
        ));
    }
    let Some(cycles) = open_boundary_loops(&inside) else {
        return bail("inside holes are not simple loops");
    };
    let Some(others) = open_boundary_loops(&outside) else {
        return bail("outside holes are not simple loops");
    };
    let sorted = |loops: &[Vec<u32>]| -> Vec<Vec<u32>> {
        let mut out: Vec<Vec<u32>> = loops
            .iter()
            .map(|cycle| {
                let mut nodes = cycle.clone();
                nodes.sort_unstable();
                nodes
            })
            .collect();
        out.sort();
        out
    };
    if sorted(&cycles) != sorted(&others) {
        return bail("the two sides' holes disagree");
    }
    Some((inside, outside, cycles))
}

/// A face two patches cross, described so both incident cells triangulate it identically.
///
/// The `LoopFan` fallback cones such a face to its centroid, and its triangles then
/// straddle both cuts: a boundary triangle ends up with nodes on either side of a
/// surface with no node between them, which is exactly what stops §7.6's cell split
/// (`mixed sides on one triangle` - 840 of A-3's 884 refusals). Here the two chords are
/// kept as **edges** of the triangulation instead, so every triangle lies on one side of
/// both surfaces and the cell above it can be cut.
#[derive(Clone, Debug)]
pub struct CrossedFace {
    /// The face's boundary walk, corners and cut nodes interleaved.
    pub loop_nodes: SmallVec<[u32; 8]>,
    /// The chords, as positions into `loop_nodes`, each ascending. One where a single
    /// surface enters and leaves the face (including invariant K1's same-edge pair), two
    /// where two surfaces cross it.
    pub chords: SmallVec<[[usize; 2]; 2]>,
    /// The component each chord belongs to, in the same order.
    pub components: SmallVec<[i32; 2]>,
    /// Where the chords meet, in world coordinates, when they interleave around the walk
    /// and therefore cross inside the face. `None` when they do not: two non-crossing
    /// diagonals need no new node, only a triangulation that keeps both as edges.
    pub point: Option<Vec3>,
    /// The walk position the chords **share**, when they meet at a cut node on the face's
    /// own boundary instead of inside it. Two solids in exact face contact do exactly this:
    /// the contact plane and the limb's own wall both run out of the same shared crossing
    /// node, so the face carries three regions around one hub. Fanning from that node keeps
    /// both chords as edges and needs no new node at all.
    pub hub: Option<usize>,
}

// AI-FUNC-SUMMARY:
// Purpose: The point where two chords of one face meet - the place the S2 intersection curve pierces that face.
// Inputs: the two chords' endpoints in world coordinates, in canonical order.
// Returns: the midpoint of the closest approach of the two segments.
// Side effects: None.
// Notes: The chords are the traces of two surfaces on one lattice face and are *nearly* coplanar,
//   not exactly: their endpoints are snapped nodes carrying the envelope tolerance. Closest approach
//   is therefore taken rather than a plane intersection, which has no answer when they miss. The
//   arithmetic is a fixed sequence over four coordinates given in canonical order, so the two cells
//   sharing the face get the same bits - the same argument `face_centroid` rests on.
pub fn chord_meeting_point(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> Vec3 {
    let u = a1.sub(a0);
    let v = b1.sub(b0);
    let w = a0.sub(b0);
    let a = u.dot(u);
    let b = u.dot(v);
    let c = v.dot(v);
    let d = u.dot(w);
    let e = v.dot(w);
    let denominator = a * c - b * b;
    let (s, t) = if denominator.abs() <= f64::MIN_POSITIVE {
        (0.5, 0.5)
    } else {
        (
            ((b * e - c * d) / denominator).clamp(0.0, 1.0),
            ((a * e - b * d) / denominator).clamp(0.0, 1.0),
        )
    };
    a0.add(u.scale(s)).add(b0.add(v.scale(t))).scale(0.5)
}

// AI-FUNC-SUMMARY:
// Purpose: The walk positions from `from` to `to` inclusive, forward around a loop of `count` nodes.
// Returns: the arc's positions.
// Side effects: None.
fn arc(from: usize, to: usize, count: usize) -> SmallVec<[usize; 8]> {
    let mut out: SmallVec<[usize; 8]> = SmallVec::new();
    let mut here = from;
    loop {
        out.push(here);
        if here == to {
            break;
        }
        here = (here + 1) % count;
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Fan one sub-polygon of a face from its lowest-key vertex.
// Inputs: the sub-polygon's node ids, in boundary order, and the key table.
// Returns: the sub-triangles, empty for fewer than three nodes.
// Side effects: None.
// Notes: A face polygon here is a parent triangle with extra vertices sitting *on* its edges, so it
//   is convex and a fan is valid from any vertex. Choosing the lowest `NodeKey` rather than the
//   first makes the choice a property of the polygon rather than of the walk's starting point, so
//   the two cells sharing the face emit the same triangles.
pub fn fan_polygon(polygon: &[u32], keys: &[NodeKey]) -> SmallVec<[[u32; 3]; 8]> {
    let mut out: SmallVec<[[u32; 3]; 8]> = SmallVec::new();
    if polygon.len() < 3 {
        return out;
    }
    let apex = (0..polygon.len())
        .min_by_key(|slot| keys[polygon[*slot] as usize])
        .unwrap_or(0);
    let count = polygon.len();
    for step in 1..(count - 1) {
        let b = (apex + step) % count;
        let c = (apex + step + 1) % count;
        out.push([polygon[apex], polygon[b], polygon[c]]);
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Triangulate a face two patches cross so that both chords are edges of the result.
// Inputs: the crossed-face description, the key table, and the node id the caller interned for the meeting point (unused when the chords do not cross).
// Returns: the sub-triangles.
// Side effects: None.
// Notes: This is the precondition §7.6's cell split needs and the centroid fan destroys. `LoopFan`
//   cones the whole face to its centre, so its triangles straddle both chords and a boundary
//   triangle ends up with nodes on either side of a surface with no node between them - which is
//   `mixed sides on one triangle`, 840 of A-3's 884 split refusals. Keeping the chords as edges
//   makes every triangle lie on one side of **both** surfaces.
//
//   Two shapes, and the cheaper one is the common case (A-3: 960 faces against 132). **Crossing**
//   chords cut the walk into four arcs, each closing into a sector through the meeting point, and
//   fanning that point over each arc triangulates it. **Non-crossing** chords are just two disjoint
//   diagonals: split the walk by the first, split whichever half contains the second by it, and fan
//   the three resulting polygons - no new node at all. Determinism is inherited from the walk order
//   and `fan_polygon`'s key-chosen apex, both pure functions of the face.
pub fn crossed_face_mesh(
    face: &CrossedFace,
    keys: &[NodeKey],
    meeting_id: u32,
) -> SmallVec<[[u32; 3]; 8]> {
    let count = face.loop_nodes.len();
    let mut out: SmallVec<[[u32; 3]; 8]> = SmallVec::new();
    if let Some(hub) = face.hub {
        // Every chord runs out of `hub`, so a fan from it carries all of them as edges.
        for slot in 0..count {
            let (a, b) = (slot, (slot + 1) % count);
            if a == hub || b == hub {
                continue;
            }
            out.push([face.loop_nodes[hub], face.loop_nodes[a], face.loop_nodes[b]]);
        }
        return out;
    }
    if face.chords.len() == 1 {
        // One surface crossing the face: its chord cuts the walk in two, and fanning each
        // half keeps the chord an edge. No new node - which matters, because this is the
        // shape §5.2 declares inexpressible (both cut nodes on one edge is invariant K1's
        // face) and the centroid fan would otherwise cone straight across the surface.
        let chord = face.chords[0];
        let near: SmallVec<[u32; 8]> = arc(chord[0], chord[1], count)
            .iter()
            .map(|p| face.loop_nodes[*p])
            .collect();
        let far: SmallVec<[u32; 8]> = arc(chord[1], chord[0], count)
            .iter()
            .map(|p| face.loop_nodes[*p])
            .collect();
        out.extend(fan_polygon(&near, keys));
        out.extend(fan_polygon(&far, keys));
        return out;
    }
    if face.point.is_some() {
        let mut stops = [
            face.chords[0][0],
            face.chords[0][1],
            face.chords[1][0],
            face.chords[1][1],
        ];
        stops.sort_unstable();
        for slot in 0..4 {
            for pair in arc(stops[slot], stops[(slot + 1) % 4], count).windows(2) {
                out.push([
                    meeting_id,
                    face.loop_nodes[pair[0]],
                    face.loop_nodes[pair[1]],
                ]);
            }
        }
        return out;
    }
    let (a, b) = (face.chords[0], face.chords[1]);
    let halves = [arc(a[0], a[1], count), arc(a[1], a[0], count)];
    for half in halves {
        let nodes: SmallVec<[u32; 8]> = half.iter().map(|p| face.loop_nodes[*p]).collect();
        let (Some(x), Some(y)) = (
            half.iter().position(|p| *p == b[0]),
            half.iter().position(|p| *p == b[1]),
        ) else {
            out.extend(fan_polygon(&nodes, keys));
            continue;
        };
        // The second chord lies in this half: split it there and fan both parts.
        let (lo, hi) = if x <= y { (x, y) } else { (y, x) };
        let inner: SmallVec<[u32; 8]> = nodes[lo..=hi].iter().copied().collect();
        let mut outer: SmallVec<[u32; 8]> = nodes[hi..].iter().copied().collect();
        outer.extend(nodes[..=lo].iter().copied());
        out.extend(fan_polygon(&inner, keys));
        out.extend(fan_polygon(&outer, keys));
    }
    out
}
