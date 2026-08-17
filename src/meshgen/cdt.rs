//! `SPEC_meshgen_geometry.md` §7.2–§7.4 — the local mesher for one cell, as a kernel.
//!
//! **Why this exists.** Gate G6-0 declined to build this and adopted the conforming centroid fan
//! instead. The fan is unconditionally valid, which is why the mesher is watertight everywhere,
//! but it abandons P3 by construction: it never cuts along the surface, so the material boundary
//! inside every cell it touches is off-surface by up to `h`. Phase P-3 re-opened that decision
//! with a measure that can see the damage (`[V13]`), and every cheaper route has now been tried
//! and closed with numbers — the `meets_inside` gate (fixed, and it was worth 26 % of the damage),
//! S7 curve conformity (unreachable by snapping at any cap), and driving the cap split from the
//! input's curve nodes (no gain, and it broke `[V3]`). What is left is the crease: ~75 % of the
//! remaining off-surface area lies within one element of a sharp edge of the input, and a **planar
//! cap cannot lie on a surface that creases inside the cell**. The crease has to be an edge of the
//! mesh, which means a mesher that takes the geometry as a *constraint* rather than approximating
//! it — this module.
//!
//! **The construction, and why not incremental Delaunay.** §7.4 asks for "constrained incremental
//! tetrahedralisation". What is normative there is the *properties* — determinism, constraints
//! respected, the Steiner rules — not the algorithm, and a Delaunay CDT with Steiner recovery is a
//! large amount of machinery whose failure modes are hard to bound. This kernel gets the same
//! properties from **successive convex clipping**: the cell is a tet, hence convex; clipping a
//! convex polyhedron by a plane yields two convex polyhedra; and a convex polyhedron tetrahedralises
//! by fanning from one of its own vertices with no Steiner point at all. Clip by every constraint
//! plane in a canonical order and every constraint surface comes out as a union of element faces,
//! which is exactly P3. Two patches that crease give two planes, and the crease is their
//! intersection line — an edge of the result by construction, without being special-cased.
//!
//! **What it does not do.** A patch is a bounded fragment and this clips by its whole supporting
//! plane, so a plane that only partly crosses the cell still cuts it all the way through. The extra
//! face is interior — both sides sample to the same region under §7.5 — so it costs elements and
//! never costs correctness. Bounding the cut to the fragment is what a true CDT buys and is the
//! next question this kernel makes askable, not one it answers.
//!
//! **INTEGRATION STATUS (2026-08-15): the kernel is sound; wiring it in cell-by-cell is NOT.**
//! `subdivide_cell` is reachable under `RUSTMSPT_CDT=1`, a gate handle and not a setting. Measured
//! there: a7b +0.249 pp of on-surface area, a7a +0.030, a3 +0.014, a8 +0.020, `[V3]` clean on all
//! of them **except a8**, which fails with 12 boundary leaks, 6 hanging nodes and 2 non-manifold
//! edges out of 555,204 interior faces. Two safety rules were added and neither is sufficient:
//! refusing to intern a new node, then requiring the cell's boundary to come back triangle-for-
//! triangle. The residue is small but it is real, and small conformity failures are the expensive
//! kind.
//!
//! The reason is structural, not a bug to chase. A cell that clips puts an edge on its boundary
//! that its neighbour - which did not clip - does not have, and *whether to clip is decided per
//! cell*. Invariant J1 says the triangulation of a face must be a pure function of the face, so
//! the crease trace has to enter the shared face's triangulation **once, agreed by both cells,
//! before either meshes its interior**. That is exactly what SPEC §7.3's `FaceTriCache` is
//! specified for, and it is not implemented. Until it is, this kernel can only take cells whose
//! boundary it does not need to re-cut - which is a bound, not a tuning parameter, and it is why
//! the gains above are fractions of a point rather than the ~75 % of crease damage on offer.
//!
//! Determinism (R-P2, §7.4): planes are consumed in the caller's order, which is canonical by
//! construction; every vertex is interned through a `NodeKey` so a point computed twice is one
//! node; and each face's fan apex is its smallest key, so a face shared by two pieces is
//! triangulated identically from either side without negotiation (invariant J1).

use crate::meshgen::predicates::node_key;
use crate::meshgen::NodeKey;
use crate::types::Vec3;
use std::collections::BTreeMap;

/// A convex polyhedron: vertices by node id, and faces as node-id loops wound outward.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConvexCell {
    pub faces: Vec<Vec<u32>>,
}

/// Which side of a plane a point is on, decided once and reused, so a vertex can never be
/// classified two ways within one clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Below,
    On,
    Above,
}

/// An oriented plane through `point` with unit-ish `normal`; `Above` is the normal's side.
#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub point: Vec3,
    pub normal: Vec3,
}

impl Plane {
    // AI-FUNC-SUMMARY: The supporting plane of a triangle, or None when it is degenerate; returns Option<Plane>; side effects: none.
    pub fn of_triangle(t: [Vec3; 3]) -> Option<Plane> {
        let normal = t[1].sub(t[0]).cross(t[2].sub(t[0]));
        let length = normal.dot(normal).sqrt();
        (length > 0.0).then(|| Plane {
            point: t[0],
            normal: normal.scale(1.0 / length),
        })
    }

    // AI-FUNC-SUMMARY: Signed distance from the plane to a point; returns f64; side effects: none.
    fn distance(&self, p: Vec3) -> f64 {
        p.sub(self.point).dot(self.normal)
    }
}

/// Interns points as nodes, so a point produced twice - by two clips, or by the two cells
/// sharing a face - is one node and not a coincident pair.
///
/// This is the whole conformity mechanism on the vertex side: the key is quantised
/// (`node_key`), so two cells computing the same intersection independently agree exactly,
/// which is what lets faces match without the cells talking to each other.
pub struct NodeArena {
    pub points: Vec<Vec3>,
    pub keys: Vec<NodeKey>,
    index: BTreeMap<NodeKey, u32>,
    pub quantum: f64,
}

impl NodeArena {
    // AI-FUNC-SUMMARY: Start an arena over existing nodes, adopting their keys; returns NodeArena; side effects: none.
    pub fn new(points: Vec<Vec3>, quantum: f64) -> NodeArena {
        let keys: Vec<NodeKey> = points.iter().map(|p| node_key(*p, quantum)).collect();
        let mut index = BTreeMap::new();
        for (id, key) in keys.iter().enumerate() {
            index.entry(*key).or_insert(id as u32);
        }
        NodeArena {
            points,
            keys,
            index,
            quantum,
        }
    }

    // AI-FUNC-SUMMARY: The node id of a point, inserting it only if its key is new; returns u32; side effects: may push a node.
    pub fn intern(&mut self, p: Vec3) -> u32 {
        let key = node_key(p, self.quantum);
        if let Some(existing) = self.index.get(&key) {
            return *existing;
        }
        let id = self.points.len() as u32;
        self.points.push(p);
        self.keys.push(key);
        self.index.insert(key, id);
        id
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Split one convex cell by a plane, into the part below and the part above it.
// Inputs: the cell, the plane, the node arena, and the on-plane tolerance.
// Returns: `(below, above)`, either of which is None when the cell lies wholly on one side.
// Side effects: Interns the intersection points.
// Notes: Face by face. A face wholly on one side goes there untouched; a face the plane crosses is
//   cut into two, and the two new edges - one per side - are collected. Those edges bound the
//   **cap**, the new face lying in the plane, which is what makes the plane a face of the output
//   rather than something the elements merely straddle. The cap loop is recovered by walking the
//   collected edges end to end; a cap that does not close means the cell was not convex or the
//   predicate disagreed with itself, and the clip declines rather than emitting a broken cell.
pub fn clip(
    cell: &ConvexCell,
    plane: Plane,
    arena: &mut NodeArena,
    tol: f64,
) -> (Option<ConvexCell>, Option<ConvexCell>) {
    let side_of = |id: u32, arena: &NodeArena| -> Side {
        let d = plane.distance(arena.points[id as usize]);
        if d > tol {
            Side::Above
        } else if d < -tol {
            Side::Below
        } else {
            Side::On
        }
    };
    // Decided once per vertex, up front: a vertex classified separately per face can come out
    // Above on one and On on another, and then the two faces disagree about whether an edge
    // was cut. Every conformity bug in a clipper is some version of that.
    let mut sides: BTreeMap<u32, Side> = BTreeMap::new();
    for face in &cell.faces {
        for id in face {
            sides.entry(*id).or_insert_with(|| side_of(*id, arena));
        }
    }
    let any = |want: Side| sides.values().any(|s| *s == want);
    if !any(Side::Above) {
        return (Some(cell.clone()), None);
    }
    if !any(Side::Below) {
        return (None, Some(cell.clone()));
    }

    let mut below_faces: Vec<Vec<u32>> = Vec::new();
    let mut above_faces: Vec<Vec<u32>> = Vec::new();
    // The cut edges, as ordered pairs so the cap can be walked. Stored once; the two caps are
    // the same loop wound opposite ways.
    let mut cut_edges: Vec<(u32, u32)> = Vec::new();

    for face in &cell.faces {
        let mut below: Vec<u32> = Vec::new();
        let mut above: Vec<u32> = Vec::new();
        let mut crossings: Vec<u32> = Vec::new();
        for slot in 0..face.len() {
            let (a, b) = (face[slot], face[(slot + 1) % face.len()]);
            let (sa, sb) = (sides[&a], sides[&b]);
            match sa {
                Side::Below => below.push(a),
                Side::Above => above.push(a),
                Side::On => {
                    below.push(a);
                    above.push(a);
                    crossings.push(a);
                }
            }
            let straddles = matches!(
                (sa, sb),
                (Side::Below, Side::Above) | (Side::Above, Side::Below)
            );
            if straddles {
                let (pa, pb) = (arena.points[a as usize], arena.points[b as usize]);
                let (da, db) = (plane.distance(pa), plane.distance(pb));
                let denominator = da - db;
                if denominator == 0.0 {
                    return (None, None);
                }
                let t = da / denominator;
                let id = arena.intern(pa.add(pb.sub(pa).scale(t)));
                sides.insert(id, Side::On);
                below.push(id);
                above.push(id);
                crossings.push(id);
            }
        }
        if crossings.len() == 2 {
            cut_edges.push((crossings[0], crossings[1]));
        }
        if below.len() >= 3 {
            below_faces.push(below);
        }
        if above.len() >= 3 {
            above_faces.push(above);
        }
    }

    // Two faces sharing an edge that lies *in* the plane both report that edge as their
    // crossing, so the same cut edge arrives twice. Left in, it gives the walk a node with two
    // identical neighbours and the loop never closes - which showed up as "two crossing planes
    // cut the cell in four" failing at three, because a declined clip passes the piece through
    // whole. Undirected dedup, before the walk.
    cut_edges.sort_unstable_by_key(|(a, b)| (*a.min(b), *a.max(b)));
    cut_edges.dedup_by_key(|(a, b)| (*a.min(b), *a.max(b)));
    let Some(cap) = walk_loop(&cut_edges) else {
        return (None, None);
    };
    if cap.len() < 3 {
        return (None, None);
    }
    let mut reversed = cap.clone();
    reversed.reverse();
    below_faces.push(cap);
    above_faces.push(reversed);
    (
        Some(ConvexCell { faces: below_faces }),
        Some(ConvexCell { faces: above_faces }),
    )
}

// AI-FUNC-SUMMARY:
// Purpose: Walk a set of undirected edges into one closed loop.
// Inputs: the edge list.
// Returns: the loop's nodes in order, or None when the edges do not form exactly one closed loop.
// Side effects: None.
// Notes: Returning None rather than a partial loop is deliberate: a cap that does not close is
//   proof the clip went wrong, and emitting it would hand the caller a cell whose boundary has a
//   hole - which every downstream check would then report somewhere else, far from the cause.
fn walk_loop(edges: &[(u32, u32)]) -> Option<Vec<u32>> {
    if edges.len() < 3 {
        return None;
    }
    let mut adjacency: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (a, b) in edges {
        if a == b {
            return None;
        }
        adjacency.entry(*a).or_default().push(*b);
        adjacency.entry(*b).or_default().push(*a);
    }
    if adjacency.values().any(|n| n.len() != 2) {
        return None;
    }
    // Start at the smallest node id, so the loop's rotation is a function of the edge set and
    // not of the order the faces produced it.
    let start = *adjacency.keys().next()?;
    let mut loop_nodes = vec![start];
    let mut previous = start;
    let mut current = adjacency[&start][0];
    while current != start {
        loop_nodes.push(current);
        let neighbours = adjacency.get(&current)?;
        let next = if neighbours[0] == previous {
            neighbours[1]
        } else {
            neighbours[0]
        };
        previous = current;
        current = next;
        if loop_nodes.len() > adjacency.len() {
            return None;
        }
    }
    (loop_nodes.len() == adjacency.len()).then_some(loop_nodes)
}

// AI-FUNC-SUMMARY:
// Purpose: Cut one cell by every constraint plane, so each plane becomes a face of the output.
// Inputs: the starting cell, the planes in canonical order, the arena, and the on-plane tolerance.
// Returns: the convex pieces.
// Side effects: Interns the intersection points.
// Notes: Successive clipping, every piece by every plane. The output is a convex decomposition
//   whose internal faces include all the constraint geometry - which is P3's requirement stated
//   as a construction rather than as a tolerance. The plane order does not change the *set* of
//   pieces (they are the arrangement's cells), only the order they come out in, and the caller's
//   order is canonical, so R-P2 holds either way.
pub fn subdivide(
    cell: &ConvexCell,
    planes: &[Plane],
    arena: &mut NodeArena,
    tol: f64,
) -> Vec<ConvexCell> {
    let mut pieces = vec![cell.clone()];
    for plane in planes {
        let mut next: Vec<ConvexCell> = Vec::with_capacity(pieces.len() + 1);
        for piece in &pieces {
            let (below, above) = clip(piece, *plane, arena, tol);
            match (below, above) {
                (None, None) => next.push(piece.clone()),
                (below, above) => next.extend(below.into_iter().chain(above)),
            }
        }
        pieces = next;
    }
    pieces
}

// AI-FUNC-SUMMARY:
// Purpose: Tetrahedralise a convex cell, with no Steiner point.
// Inputs: the cell and the arena (for keys and coordinates).
// Returns: positively-oriented tets.
// Side effects: None.
// Notes: Fan from the cell's smallest-key vertex, and within each face fan from *that face's*
//   smallest-key vertex. Both choices are functions of node keys alone, so the two cells sharing a
//   face triangulate it identically without negotiating - invariant J1, the same rule §4's SNK
//   states for quads. Faces containing the apex contribute nothing (their tets would be flat) and
//   are skipped, which is what makes the fan a partition rather than an overlap.
pub fn tetrahedralise(cell: &ConvexCell, arena: &NodeArena) -> Vec<[u32; 4]> {
    let mut vertices: Vec<u32> = cell.faces.iter().flatten().copied().collect();
    vertices.sort_unstable();
    vertices.dedup();
    if vertices.len() < 4 {
        return Vec::new();
    }
    let Some(&apex) = vertices.iter().min_by_key(|id| arena.keys[**id as usize]) else {
        return Vec::new();
    };
    let mut tets = Vec::new();
    for face in &cell.faces {
        if face.contains(&apex) || face.len() < 3 {
            continue;
        }
        let Some(&pivot) = face.iter().min_by_key(|id| arena.keys[**id as usize]) else {
            continue;
        };
        let at = face.iter().position(|id| *id == pivot).unwrap_or(0);
        for step in 1..face.len() - 1 {
            let b = face[(at + step) % face.len()];
            let c = face[(at + step + 1) % face.len()];
            if let Some(tet) = orient(&[apex, pivot, b, c], arena) {
                tets.push(tet);
            }
        }
    }
    tets
}

// AI-FUNC-SUMMARY: Order a tet's nodes so its signed volume is positive, or drop it when flat; returns Option<[u32; 4]>; side effects: none.
fn orient(tet: &[u32; 4], arena: &NodeArena) -> Option<[u32; 4]> {
    let p: Vec<Vec3> = tet.iter().map(|id| arena.points[*id as usize]).collect();
    let volume = p[1].sub(p[0]).cross(p[2].sub(p[0])).dot(p[3].sub(p[0]));
    if volume > 0.0 {
        Some(*tet)
    } else if volume < 0.0 {
        Some([tet[1], tet[0], tet[2], tet[3]])
    } else {
        None
    }
}

// AI-FUNC-SUMMARY: Total volume of a tet set, for the volume-conservation guard; returns f64; side effects: none.
pub fn volume_of(tets: &[[u32; 4]], arena: &NodeArena) -> f64 {
    tets.iter()
        .map(|tet| {
            let p: Vec<Vec3> = tet.iter().map(|id| arena.points[*id as usize]).collect();
            p[1].sub(p[0]).cross(p[2].sub(p[0])).dot(p[3].sub(p[0])).abs() / 6.0
        })
        .sum()
}

// AI-FUNC-SUMMARY: The four outward-wound faces of a tet, as a starting `ConvexCell`; returns ConvexCell; side effects: none.
pub fn cell_of_tet(tet: [u32; 4], arena: &NodeArena) -> ConvexCell {
    let p: Vec<Vec3> = tet.iter().map(|id| arena.points[*id as usize]).collect();
    let outward = p[1].sub(p[0]).cross(p[2].sub(p[0])).dot(p[3].sub(p[0])) < 0.0;
    let faces = [[1, 2, 3], [0, 3, 2], [0, 1, 3], [0, 2, 1]];
    ConvexCell {
        faces: faces
            .iter()
            .map(|f| {
                let mut face = vec![tet[f[0]], tet[f[1]], tet[f[2]]];
                if outward {
                    face.reverse();
                }
                face
            })
            .collect(),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The constraint planes of one cell, fitted through the cut nodes a component left on it.
// Inputs: the component's cut-node ids, the arena, and the coplanarity tolerance.
// Returns: one plane per coplanar group, in a canonical order.
// Side effects: None.
// Notes: **The planes are fitted to the cut nodes rather than taken from the input triangles, and
//   that is what makes the result conforming.** A plane through the cut nodes passes exactly
//   through the points where the surface crosses the cell's edges - the points the *neighbouring*
//   cell also has, because a cut node is a function of the edge and the component (invariant K2).
//   So clipping by it re-derives the nodes already on every shared face and interns no new one
//   there, which is the difference between a cut that conforms and a cut that cracks. Taking the
//   plane from the input triangle instead would be the same plane only where the patch is exactly
//   planar through those nodes, and would drift everywhere else.
//
//   Grouping is what handles the crease, which is the whole point: a component whose cut nodes are
//   **not** coplanar has more than one patch in this cell, and the groups are those patches. The
//   greedy pass seeds on the three smallest keys, claims every node within tolerance of their
//   plane, and repeats on the remainder - so a flat patch yields one plane, a creased pair yields
//   two, and the crease is their intersection line, an edge of the output by construction.
//
//   Seeding by smallest key rather than by any geometric preference is deliberate: it is the same
//   total order §4's SNK rule uses, so the grouping is a pure function of the node set (R-P2), and
//   two cells sharing these nodes group them identically.
pub fn planes_through_cut_nodes(nodes: &[u32], arena: &NodeArena, tol: f64) -> Vec<Plane> {
    let mut remaining: Vec<u32> = nodes.to_vec();
    remaining.sort_by_key(|id| arena.keys[*id as usize]);
    remaining.dedup();
    let mut planes: Vec<Plane> = Vec::new();
    // Bounded: every pass either fixes a plane and removes at least three nodes, or gives up.
    while remaining.len() >= 3 {
        let mut seed: Option<Plane> = None;
        // The first non-degenerate triple in key order.
        'outer: for i in 0..remaining.len() {
            for j in (i + 1)..remaining.len() {
                for k in (j + 1)..remaining.len() {
                    let tri = [
                        arena.points[remaining[i] as usize],
                        arena.points[remaining[j] as usize],
                        arena.points[remaining[k] as usize],
                    ];
                    if let Some(plane) = Plane::of_triangle(tri) {
                        seed = Some(plane);
                        break 'outer;
                    }
                }
            }
        }
        let Some(plane) = seed else { break };
        let (on, off): (Vec<u32>, Vec<u32>) = remaining
            .iter()
            .partition(|id| plane.distance(arena.points[**id as usize]).abs() <= tol);
        if on.len() < 3 {
            break;
        }
        planes.push(plane);
        remaining = off;
    }
    planes
}

/// One cell's subdivision, in the shape `split_escalated_cell` already hands downstream:
/// material pieces as closed triangle soups, plus the cap triangles tagged by component.
pub struct CellSubdivision {
    pub pieces: Vec<Vec<[u32; 3]>>,
    pub caps: Vec<([u32; 3], i32)>,
}

// AI-FUNC-SUMMARY:
// Purpose: Subdivide one escalated cell by the planes its crossing components leave on it, and
//   hand back pieces and caps in the shape the existing §7.6 path uses.
// Inputs: the cell's closed boundary soup, the cut nodes per component, the global node table, the
//   key quantum and the coplanarity/on-plane tolerance.
// Returns: the subdivision, or None when the cell is one the kernel must decline.
// Side effects: None - it interns into a *local* arena and returns nothing new.
// Notes: **It declines rather than inventing a node, and that is the integration's safety rule.**
//   A plane fitted through cut nodes re-derives the nodes already on every shared face, so the
//   common case interns nothing; but a plane extends past the bounded patch that defined it, and
//   where it leaves the patch it can cross a face edge somewhere new. A node invented there exists
//   in this cell and not in the neighbour, which is a crack - the one failure mode that would not
//   show up in the kernel's own tests, because those mesh a cell in isolation. So the arena is
//   checked afterwards: if it grew, the cell is refused and the caller's existing path takes it.
//   That keeps the kernel to the cells where it is provably conforming, and makes the refusal rate
//   a number to report rather than a risk to argue about.
pub fn subdivide_cell(
    boundary: &[[u32; 3]],
    cut_nodes: &[(i32, Vec<u32>)],
    nodes: &[Vec3],
    quantum: f64,
    tol: f64,
) -> Option<CellSubdivision> {
    // A local arena over just this cell's nodes, keeping global ids. Building one over the whole
    // mesh per cell would be O(N log N) per cell, and the cell only ever touches its own nodes.
    let mut local: Vec<u32> = boundary.iter().flatten().copied().collect();
    local.sort_unstable();
    local.dedup();
    let mut arena = NodeArena::new(
        local.iter().map(|id| nodes[*id as usize]).collect(),
        quantum,
    );
    let to_local: BTreeMap<u32, u32> = local
        .iter()
        .enumerate()
        .map(|(slot, id)| (*id, slot as u32))
        .collect();
    let before = arena.points.len();

    let mut planes: Vec<(Plane, i32)> = Vec::new();
    for (component, ids) in cut_nodes {
        let mapped: Vec<u32> = ids.iter().filter_map(|id| to_local.get(id).copied()).collect();
        for plane in planes_through_cut_nodes(&mapped, &arena, tol) {
            planes.push((plane, *component));
        }
    }
    if planes.is_empty() {
        return None;
    }

    let cell = ConvexCell {
        faces: boundary
            .iter()
            .filter_map(|t| {
                Some(vec![
                    *to_local.get(&t[0])?,
                    *to_local.get(&t[1])?,
                    *to_local.get(&t[2])?,
                ])
            })
            .collect(),
    };
    if cell.faces.len() != boundary.len() {
        return None;
    }
    let only_planes: Vec<Plane> = planes.iter().map(|(p, _)| *p).collect();
    let pieces = subdivide(&cell, &only_planes, &mut arena, tol);
    if arena.points.len() != before {
        // A plane left the patch that defined it and wanted a node no neighbour has.
        return None;
    }
    if pieces.len() < 2 {
        return None;
    }
    // **Interning no new node is not enough, and a8 proved it: `[V3]` failed anyway.** A plane
    // through existing nodes still *splits an existing boundary triangle* into two, and the
    // neighbour across that face - which did not clip - keeps it whole. Same vertices, different
    // edges, and the face no longer matches: a crack.
    //
    // So the boundary must come back unchanged, triangle for triangle. Every piece face that is
    // not in a constraint plane has to be one of the original boundary triangles; anything else
    // means the clip re-cut the cell's surface, and the cell is refused.
    //
    let back = |id: u32| -> u32 { local[id as usize] };
    // This is the invariant J1 problem in its proper form, and it is *why* SPEC §7.3 specifies a
    // `FaceTriCache` keyed on the face rather than on the cell: the crease trace has to be put
    // into the shared face's triangulation **once, by both cells**, before either meshes its
    // interior. Until that exists, the kernel can only take cells whose surface it does not need
    // to re-cut - which is the honest bound on this integration, not a tuning parameter.
    let original: std::collections::BTreeSet<[u32; 3]> = boundary
        .iter()
        .map(|t| {
            let mut k = *t;
            k.sort_unstable();
            k
        })
        .collect();
    for piece in &pieces {
        for face in &piece.faces {
            let in_plane = planes.iter().any(|(plane, _)| {
                face.iter()
                    .all(|id| plane.distance(arena.points[*id as usize]).abs() <= tol)
            });
            if in_plane {
                continue;
            }
            if face.len() != 3 {
                return None;
            }
            let mut k = [back(face[0]), back(face[1]), back(face[2])];
            k.sort_unstable();
            if !original.contains(&k) {
                return None;
            }
        }
    }

    let mut out = CellSubdivision {
        pieces: Vec::with_capacity(pieces.len()),
        caps: Vec::new(),
    };
    for piece in &pieces {
        let mut soup: Vec<[u32; 3]> = Vec::new();
        for face in &piece.faces {
            if face.len() < 3 {
                continue;
            }
            // Fanned from the face's own smallest key, so the two pieces sharing it - and the
            // two *cells* sharing it, when it is on the boundary - agree (invariant J1).
            let Some(&pivot) = face.iter().min_by_key(|id| arena.keys[**id as usize]) else {
                continue;
            };
            let at = face.iter().position(|id| *id == pivot).unwrap_or(0);
            for step in 1..face.len() - 1 {
                let tri = [
                    back(pivot),
                    back(face[(at + step) % face.len()]),
                    back(face[(at + step + 1) % face.len()]),
                ];
                if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
                    continue;
                }
                soup.push(tri);
                // A face lying in a constraint plane is the material boundary there, and is
                // what the caller tags as an interface triangle.
                if let Some((_, component)) = planes.iter().find(|(plane, _)| {
                    face.iter()
                        .all(|id| plane.distance(arena.points[*id as usize]).abs() <= tol)
                }) {
                    out.caps.push((tri, *component));
                }
            }
        }
        if soup.len() < 4 {
            return None;
        }
        out.pieces.push(soup);
    }
    Some(out)
}

// AI-FUNC-SUMMARY:
// Purpose: The distinct supporting planes of a surface fragment, so §7.2's kernel can be driven by
//   the geometry itself instead of by edge crossings.
// Inputs: the fragment's triangles in world coordinates, and the quantum used to decide when two
//   planes are the same one.
// Returns: one `Plane` per distinct supporting plane, in a canonical order.
// Side effects: None.
// Notes: **This is the input SPEC §7.2 always specified and the kernel has never had.** Its stated
//   inputs are "the cell's tet, its clipped surface fragments, and its curve segments" — edge
//   crossings are absent from that list, which is exactly why it can represent a body that crosses
//   no edge. `planes_through_cut_nodes` builds planes from crossing NODES and therefore cannot:
//   with no crossings there is nothing to fit a plane to.
//
//   Three properties matter and all three come from the same canonicalisation. A plane is stored
//   with its normal pointing to the positive-`d` side and quantised before comparison, so (a) the
//   two triangles of a folded sheet give **one** plane rather than two facing opposite ways,
//   (b) a fragment triangulated differently by two cells still yields the same plane set, and
//   (c) the order is a function of the geometry, not of the traversal — which is what R-P2 needs
//   and what makes the resulting subdivision reproducible.
//
//   Every piece `subdivide` returns from these planes is **convex**, hence star-shaped, hence
//   fannable — which is the property PLAN §6.23 found the centroid fan losing on traced pieces.
pub fn planes_from_fragment(tris: &[[Vec3; 3]], quantum: f64) -> Vec<Plane> {
    let key = |value: f64| -> i64 {
        let q = quantum.max(f64::MIN_POSITIVE);
        (value / q).round() as i64
    };
    let mut seen: std::collections::BTreeMap<[i64; 4], Plane> = std::collections::BTreeMap::new();
    for triangle in tris {
        let Some(plane) = Plane::of_triangle(*triangle) else {
            continue;
        };
        let d = plane.point.dot(plane.normal);
        // Point the normal at the positive-d side, so a plane and its mirror collapse into one.
        // At d == 0 the sign is decided by the first non-zero component instead, on the same rule.
        let flip = if d < 0.0 {
            true
        } else if d > 0.0 {
            false
        } else {
            let n = plane.normal;
            let lead = if n.x != 0.0 {
                n.x
            } else if n.y != 0.0 {
                n.y
            } else {
                n.z
            };
            lead < 0.0
        };
        let normal = if flip { plane.normal.scale(-1.0) } else { plane.normal };
        let d = if flip { -d } else { d };
        let id = [key(normal.x), key(normal.y), key(normal.z), key(d)];
        seen.entry(id).or_insert(Plane {
            point: normal.scale(d),
            normal,
        });
    }
    seen.into_values().collect()
}

// AI-FUNC-SUMMARY:
// Purpose: Subdivide a convex cell by a surface fragment rather than by crossing nodes.
// Inputs: the cell, the fragment's triangles, the shared node arena, and the clip tolerance.
// Returns: the convex pieces the fragment's supporting planes cut the cell into.
// Side effects: Interns clip points into the arena.
// Notes: Over-cuts by construction — a supporting plane is infinite while the triangle that
//   produced it is not — so a fragment that only grazes a corner still splits the whole cell. That
//   is intentional and is why the caller must classify each piece and merge: the pieces are the
//   finest partition the fragment can induce, and any coarser one is a union of them. Convexity is
//   what makes that safe, since a union of convex pieces sharing faces is meshed by meshing each.
pub fn subdivide_by_fragment(
    cell: &ConvexCell,
    tris: &[[Vec3; 3]],
    arena: &mut NodeArena,
    tol: f64,
) -> Vec<ConvexCell> {
    let planes = planes_from_fragment(tris, arena.quantum);
    subdivide(cell, &planes, arena, tol)
}

// AI-FUNC-SUMMARY:
// Purpose: The surface's trace on one lattice face — where a fragment meets the face, as segments.
// Inputs: the face's three corners, the surface triangles to test, and a tolerance.
// Returns: the intersection segments, clipped to the face, in a canonical order.
// Side effects: None.
// Notes: **This is the piece that makes a fragment-driven cut conforming, and it works because of
//   what it does NOT depend on.** The trace on a face is a function of the face and the surface
//   alone — not of which cell you look at it from, not of how either cell clipped its own fragment,
//   not of the traversal. So the two cells sharing a face derive the same trace, intern the same
//   nodes for it, and their cuts meet. That is invariant J1's requirement, and four separate
//   attempts this session failed by computing the equivalent quantity per *cell* instead
//   (PLAN §6.23): the trace has to come from the face.
//
//   Each surface triangle contributes at most one segment: the chord where it crosses the face's
//   plane, clipped to the face's own triangle. A triangle lying *in* the plane contributes nothing
//   — its trace is an area, not a curve, and the coincident-surface case is §6's, not this one.
//   Endpoints are ordered within each segment and the segments among themselves, so the result is a
//   function of the geometry rather than of the order the caller supplies triangles in.
pub fn trace_on_face(face: [Vec3; 3], tris: &[[Vec3; 3]], tol: f64) -> Vec<[Vec3; 2]> {
    let normal = face[1].sub(face[0]).cross(face[2].sub(face[0]));
    let area2 = normal.dot(normal);
    if area2 <= 0.0 {
        return Vec::new();
    }
    let scale = area2.sqrt();
    // Inward half-planes of the face, for clipping a chord to it.
    let inside_face = |p: Vec3| -> bool {
        let edge = |u: Vec3, v: Vec3| normal.dot(v.sub(u).cross(p.sub(u))) >= -tol * scale;
        edge(face[0], face[1]) && edge(face[1], face[2]) && edge(face[2], face[0])
    };
    let mut out: Vec<[Vec3; 2]> = Vec::new();
    for triangle in tris {
        let d: [f64; 3] = [
            normal.dot(triangle[0].sub(face[0])) / scale,
            normal.dot(triangle[1].sub(face[0])) / scale,
            normal.dot(triangle[2].sub(face[0])) / scale,
        ];
        // Wholly on one side, or lying in the plane: no chord.
        if d.iter().all(|x| *x > tol) || d.iter().all(|x| *x < -tol) {
            continue;
        }
        if d.iter().all(|x| x.abs() <= tol) {
            continue;
        }
        let mut hits: Vec<Vec3> = Vec::new();
        for k in 0..3 {
            let (a, b) = (k, (k + 1) % 3);
            if d[a].abs() <= tol {
                hits.push(triangle[a]);
            } else if (d[a] > tol && d[b] < -tol) || (d[a] < -tol && d[b] > tol) {
                let t = d[a] / (d[a] - d[b]);
                hits.push(triangle[a].add(triangle[b].sub(triangle[a]).scale(t)));
            }
        }
        if hits.len() < 2 {
            continue;
        }
        // Clip the chord to the face by walking its parameter against the three edges.
        let (p, q) = (hits[0], hits[hits.len() - 1]);
        let along = q.sub(p);
        if along.dot(along) <= 0.0 {
            continue;
        }
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for k in 0..3 {
            let (u, v) = (face[k], face[(k + 1) % 3]);
            let inward = normal.cross(v.sub(u));
            let denom = inward.dot(along);
            let value = inward.dot(p.sub(u));
            if denom.abs() <= f64::MIN_POSITIVE {
                if value < -tol * scale * scale {
                    lo = 1.0;
                    hi = 0.0;
                }
                continue;
            }
            let t = -value / denom;
            if denom > 0.0 {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
        }
        if lo >= hi {
            continue;
        }
        let (a, b) = (p.add(along.scale(lo)), p.add(along.scale(hi)));
        if !inside_face(a.add(b.sub(a).scale(0.5))) {
            continue;
        }
        // Canonical endpoint order, so the segment is a function of the geometry.
        let key = |v: Vec3| (v.x.to_bits(), v.y.to_bits(), v.z.to_bits());
        out.push(if key(a) <= key(b) { [a, b] } else { [b, a] });
    }
    out.sort_by(|l, r| {
        let key = |v: Vec3| (v.x.to_bits(), v.y.to_bits(), v.z.to_bits());
        (key(l[0]), key(l[1])).cmp(&(key(r[0]), key(r[1])))
    });
    out.dedup_by(|l, r| {
        let key = |v: Vec3| (v.x.to_bits(), v.y.to_bits(), v.z.to_bits());
        key(l[0]) == key(r[0]) && key(l[1]) == key(r[1])
    });
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Triangulate a planar polygon with one interior hole — a lattice face whose surface trace
//   is a closed loop strictly inside it.
// Inputs: the outer ring and the hole ring, both in the same plane, each in order.
// Returns: triangles as indices into `outer` followed by `hole`, or None if the rings are degenerate
//   or the ear clip cannot finish.
// Side effects: None.
// Notes: This is the face-side half of the sub-cell cut. A body that crosses no edge of a cell
//   leaves a **closed loop strictly inside** each face it passes through (PLAN §6.17 measured that
//   shape on every one of a8's and a6a's sub-cell cells), so the face has to carry that loop as
//   edges before any cell-level cut can follow it. 3,711 of a8's faces are in this state against
//   250,258 whose trace reaches the boundary and which §5.2 already expresses.
//
//   Bridge, then ear clip. The bridge joins the outer vertex and hole vertex that are closest,
//   turning the annulus into one simple polygon; ear clipping then finishes it. Ear clipping was
//   rejected for *caps* and correctly so — a cap is a piece of a curved surface and an ear test in
//   a fitted plane cuts ears that are not ears in space — but a lattice face **is** planar, so the
//   objection does not carry over. The hole is emitted with reversed orientation so the bridged
//   polygon is simple.
pub fn triangulate_with_hole(outer: &[Vec3], hole: &[Vec3]) -> Option<Vec<[u32; 3]>> {
    if outer.len() < 3 || hole.len() < 3 {
        return None;
    }
    // Work in the outer ring's plane, in 2D.
    let origin = outer[0];
    let normal = {
        let mut n = Vec3::new(0.0, 0.0, 0.0);
        for k in 0..outer.len() {
            let (a, b) = (outer[k], outer[(k + 1) % outer.len()]);
            n = n.add(a.sub(origin).cross(b.sub(origin)));
        }
        let len = n.dot(n).sqrt();
        if len <= 0.0 {
            return None;
        }
        n.scale(1.0 / len)
    };
    let axis_u = {
        let seed = if normal.x.abs() < 0.9 {
            Vec3::new(1.0, 0.0, 0.0)
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        let u = seed.sub(normal.scale(seed.dot(normal)));
        let len = u.dot(u).sqrt();
        if len <= 0.0 {
            return None;
        }
        u.scale(1.0 / len)
    };
    let axis_v = normal.cross(axis_u);
    let flat = |p: Vec3| -> (f64, f64) {
        let r = p.sub(origin);
        (r.dot(axis_u), r.dot(axis_v))
    };
    let area = |ring: &[Vec3]| -> f64 {
        let mut sum = 0.0;
        for k in 0..ring.len() {
            let (a, b) = (flat(ring[k]), flat(ring[(k + 1) % ring.len()]));
            sum += a.0 * b.1 - b.0 * a.1;
        }
        sum * 0.5
    };
    // Outer counter-clockwise, hole clockwise: that is what makes the bridged polygon simple.
    let mut outer_ids: Vec<u32> = (0..outer.len() as u32).collect();
    if area(outer) < 0.0 {
        outer_ids.reverse();
    }
    let mut hole_ids: Vec<u32> = (0..hole.len() as u32).map(|k| k + outer.len() as u32).collect();
    if area(hole) > 0.0 {
        hole_ids.reverse();
    }
    let point = |id: u32| -> Vec3 {
        if (id as usize) < outer.len() {
            outer[id as usize]
        } else {
            hole[id as usize - outer.len()]
        }
    };
    // The bridge: the closest outer/hole pair, so the cut does not cross either ring.
    let mut best = (f64::INFINITY, 0usize, 0usize);
    for (i, o) in outer_ids.iter().enumerate() {
        for (j, h) in hole_ids.iter().enumerate() {
            let d = point(*o).sub(point(*h));
            let d2 = d.dot(d);
            if d2 < best.0 {
                best = (d2, i, j);
            }
        }
    }
    let (_, oi, hj) = best;
    let mut ring: Vec<u32> = Vec::with_capacity(outer_ids.len() + hole_ids.len() + 2);
    for k in 0..outer_ids.len() {
        ring.push(outer_ids[(oi + k) % outer_ids.len()]);
    }
    ring.push(outer_ids[oi]);
    for k in 0..hole_ids.len() {
        ring.push(hole_ids[(hj + k) % hole_ids.len()]);
    }
    ring.push(hole_ids[hj]);
    // Ear clipping on the bridged simple polygon.
    let cross = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| -> f64 {
        (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
    };
    let mut out: Vec<[u32; 3]> = Vec::new();
    let mut guard = ring.len() * ring.len() + 16;
    while ring.len() > 3 && guard > 0 {
        guard -= 1;
        let n = ring.len();
        let mut clipped = false;
        for k in 0..n {
            let (ia, ib, ic) = (ring[(k + n - 1) % n], ring[k], ring[(k + 1) % n]);
            let (a, b, c) = (flat(point(ia)), flat(point(ib)), flat(point(ic)));
            if cross(a, b, c) <= 0.0 {
                continue;
            }
            // No other vertex inside the candidate ear.
            let mut blocked = false;
            for other in ring.iter() {
                if *other == ia || *other == ib || *other == ic {
                    continue;
                }
                let p = flat(point(*other));
                if cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0 {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                continue;
            }
            out.push([ia, ib, ic]);
            ring.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            return None;
        }
    }
    if ring.len() == 3 {
        out.push([ring[0], ring[1], ring[2]]);
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUANTUM: f64 = 1.0e-12;
    const TOL: f64 = 1.0e-12;

    fn unit_tet() -> (NodeArena, ConvexCell) {
        let arena = NodeArena::new(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            QUANTUM,
        );
        let cell = cell_of_tet([0, 1, 2, 3], &arena);
        (arena, cell)
    }

    // With nothing to respect, the kernel must hand back the cell it was given - one tet,
    // positively oriented, at the parent's volume. A mesher that cannot do nothing correctly
    // cannot be trusted to do something.
    #[test]
    fn no_constraints_reproduces_the_parent_tet() {
        let (arena, cell) = unit_tet();
        let tets = tetrahedralise(&cell, &arena);
        assert_eq!(tets.len(), 1, "an unclipped tet is one tet");
        assert!((volume_of(&tets, &arena) - 1.0 / 6.0).abs() < 1.0e-15);
    }

    // The property P3 is asking for, stated as a test: after clipping, the constraint plane is
    // a *face* of the elements on both sides, not something they straddle.
    #[test]
    fn a_clipped_plane_becomes_a_face_of_both_sides() {
        let (mut arena, cell) = unit_tet();
        let plane = Plane {
            point: Vec3::new(0.0, 0.0, 0.25),
            normal: Vec3::new(0.0, 0.0, 1.0),
        };
        let (below, above) = clip(&cell, plane, &mut arena, TOL);
        let (below, above) = (below.expect("below"), above.expect("above"));

        for piece in [&below, &above] {
            let on_plane = piece
                .faces
                .iter()
                .filter(|face| {
                    face.iter()
                        .all(|id| plane.distance(arena.points[*id as usize]).abs() <= 1.0e-12)
                })
                .count();
            assert_eq!(on_plane, 1, "each side must gain exactly one face in the plane");
        }
        // And no element may cross it: every vertex is on its own side or exactly on the plane.
        for (piece, sign) in [(&below, -1.0), (&above, 1.0)] {
            for face in &piece.faces {
                for id in face {
                    let d = plane.distance(arena.points[*id as usize]) * sign;
                    assert!(d >= -1.0e-12, "vertex on the wrong side: {d}");
                }
            }
        }
    }

    // Volume conservation is the guard the frozen spec puts on every cut (§6's dry-run), and it
    // is what catches a clip that dropped or double-counted a piece.
    #[test]
    fn clipping_conserves_volume_and_orientation() {
        let (mut arena, cell) = unit_tet();
        let plane = Plane {
            point: Vec3::new(0.3, 0.2, 0.1),
            normal: Vec3::new(1.0, 1.0, 1.0).scale(1.0 / 3.0f64.sqrt()),
        };
        let (below, above) = clip(&cell, plane, &mut arena, TOL);
        let mut all = Vec::new();
        for piece in [below, above].into_iter().flatten() {
            all.extend(tetrahedralise(&piece, &arena));
        }
        assert!(!all.is_empty());
        assert!(
            (volume_of(&all, &arena) - 1.0 / 6.0).abs() < 1.0e-14,
            "pieces must sum to the parent: {}",
            volume_of(&all, &arena)
        );
    }

    // **The crease, which is the whole reason this module exists.** Two patches meeting at a
    // sharp edge give two planes; their intersection line must come out as an *edge* of the
    // mesh, because a planar face cannot lie on a creased surface any other way.
    #[test]
    fn two_planes_put_their_crease_on_a_mesh_edge() {
        let mut arena = NodeArena::new(
            vec![
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(0.0, 1.0, -1.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            QUANTUM,
        );
        let cell = cell_of_tet([0, 1, 2, 3], &arena);
        // Two planes crossing inside the cell along the line x = 0, z = 0.
        let first = Plane {
            point: Vec3::new(0.0, 0.0, 0.0),
            normal: Vec3::new(1.0, 0.0, 0.0),
        };
        let second = Plane {
            point: Vec3::new(0.0, 0.0, 0.0),
            normal: Vec3::new(0.0, 0.0, 1.0),
        };
        let pieces = subdivide(&cell, &[first, second], &mut arena, TOL);
        assert!(pieces.len() >= 3, "two crossing planes cut the cell in four");

        let mut tets = Vec::new();
        for piece in &pieces {
            tets.extend(tetrahedralise(piece, &arena));
        }
        assert!(
            (volume_of(&tets, &arena) - volume_of(&tetrahedralise(&cell, &arena), &arena)).abs()
                < 1.0e-14,
            "the subdivision must conserve volume"
        );
        // The crease is the intersection line. Some element edge must run along it.
        let on_crease = |id: u32| {
            let p = arena.points[id as usize];
            p.x.abs() <= 1.0e-12 && p.z.abs() <= 1.0e-12
        };
        let crease_edges = tets
            .iter()
            .flat_map(|t| {
                [[t[0], t[1]], [t[0], t[2]], [t[0], t[3]], [t[1], t[2]], [t[1], t[3]], [t[2], t[3]]]
            })
            .filter(|[a, b]| on_crease(*a) && on_crease(*b) && a != b)
            .count();
        assert!(
            crease_edges > 0,
            "the two planes' crease must be carried by a mesh edge"
        );
    }

    // R-P2: the same input twice is the same output, bit for bit. The kernel's ordering rules -
    // vertices interned by key, loops started at the smallest id, fans pivoted on the smallest
    // key - exist for this and are worth asserting rather than assuming.
    #[test]
    fn subdivision_is_deterministic() {
        let run = || {
            let (mut arena, cell) = unit_tet();
            let planes = [
                Plane {
                    point: Vec3::new(0.0, 0.0, 0.25),
                    normal: Vec3::new(0.0, 0.0, 1.0),
                },
                Plane {
                    point: Vec3::new(0.3, 0.0, 0.0),
                    normal: Vec3::new(1.0, 0.5, 0.0),
                },
            ];
            let pieces = subdivide(&cell, &planes, &mut arena, TOL);
            let tets: Vec<[u32; 4]> = pieces
                .iter()
                .flat_map(|p| tetrahedralise(p, &arena))
                .collect();
            (arena.points.clone(), tets)
        };
        let (points_a, tets_a) = run();
        let (points_b, tets_b) = run();
        assert_eq!(tets_a, tets_b, "connectivity must be reproducible");
        for (a, b) in points_a.iter().zip(points_b.iter()) {
            assert_eq!(a.x.to_bits(), b.x.to_bits(), "coordinates must be bit-identical");
            assert_eq!(a.y.to_bits(), b.y.to_bits());
            assert_eq!(a.z.to_bits(), b.z.to_bits());
        }
    }

    // A point computed twice must be one node. This is the conformity mechanism on the vertex
    // side: two cells clipping their shared face by the same plane arrive at the same
    // intersection independently, and the quantised key is what makes those the same node
    // rather than a coincident pair `[V2]` would have to catch later.
    #[test]
    fn the_arena_interns_a_repeated_point_once() {
        let mut arena = NodeArena::new(vec![Vec3::new(0.0, 0.0, 0.0)], QUANTUM);
        let before = arena.points.len();
        let first = arena.intern(Vec3::new(0.5, 0.25, 0.125));
        let again = arena.intern(Vec3::new(0.5, 0.25, 0.125));
        assert_eq!(first, again);
        assert_eq!(arena.points.len(), before + 1);
        assert_eq!(arena.intern(Vec3::new(0.0, 0.0, 0.0)), 0, "existing nodes are adopted");
    }

    // **Invariant J1, which is what decides whether this can ever be wired in.** Two cells
    // sharing a face must triangulate that face identically, derived from the face alone and
    // without negotiating. Here that rests on one rule: a face is fanned from *its own*
    // smallest-key vertex, never from anything cell-level. Two tets glued on a square-ish
    // quad face, meshed independently, must induce the same triangles on it.
    #[test]
    fn two_cells_triangulate_their_shared_face_identically() {
        // A shared quad face, with a vertex above and a vertex below it.
        let mut arena = NodeArena::new(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.5, 0.5, 1.0),
                Vec3::new(0.5, 0.5, -1.0),
            ],
            QUANTUM,
        );
        let quad = vec![0u32, 1, 2, 3];
        let mut reversed = quad.clone();
        reversed.reverse();
        // The upper cell sees the quad one way round, the lower cell the other - as two real
        // neighbours do.
        let upper = ConvexCell {
            faces: vec![
                quad.clone(),
                vec![0, 1, 4],
                vec![1, 2, 4],
                vec![2, 3, 4],
                vec![3, 0, 4],
            ],
        };
        let lower = ConvexCell {
            faces: vec![
                reversed,
                vec![1, 0, 5],
                vec![2, 1, 5],
                vec![3, 2, 5],
                vec![0, 3, 5],
            ],
        };
        let induced = |cell: &ConvexCell, arena: &NodeArena| {
            let mut on_quad: Vec<[u32; 3]> = tetrahedralise(cell, arena)
                .iter()
                .flat_map(|t| {
                    [
                        [t[1], t[2], t[3]],
                        [t[0], t[3], t[2]],
                        [t[0], t[1], t[3]],
                        [t[0], t[2], t[1]],
                    ]
                })
                .filter(|f| f.iter().all(|id| quad.contains(id)))
                .map(|mut f| {
                    f.sort_unstable();
                    f
                })
                .collect::<Vec<_>>();
            on_quad.sort_unstable();
            on_quad.dedup();
            on_quad
        };
        let from_above = induced(&upper, &arena);
        let from_below = induced(&lower, &arena);
        assert!(!from_above.is_empty(), "the shared face must be triangulated");
        assert_eq!(
            from_above, from_below,
            "J1: both cells must derive the same triangles for the face they share"
        );
    }

    // Plane fitting is the integration's load-bearing step: one flat patch must give exactly one
    // plane, and a creased pair exactly two - that is how the crease survives into the mesh.
    #[test]
    fn cut_nodes_group_into_one_plane_per_patch() {
        let flat = NodeArena::new(
            vec![
                Vec3::new(0.0, 0.0, 0.5),
                Vec3::new(1.0, 0.0, 0.5),
                Vec3::new(0.0, 1.0, 0.5),
                Vec3::new(1.0, 1.0, 0.5),
            ],
            QUANTUM,
        );
        let planes = planes_through_cut_nodes(&[0, 1, 2, 3], &flat, 1.0e-9);
        assert_eq!(planes.len(), 1, "coplanar cut nodes are one patch");

        // Two patches meeting along y = 0: one in z = 0, one rising with y.
        let creased = NodeArena::new(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(0.0, 1.0, 1.0),
                Vec3::new(1.0, 1.0, 1.0),
            ],
            QUANTUM,
        );
        let planes = planes_through_cut_nodes(&[0, 1, 2, 3, 4, 5], &creased, 1.0e-9);
        assert_eq!(planes.len(), 2, "a crease is two patches, hence two planes");
    }

    // A plane that misses the cell must leave it exactly alone - not clone it into two pieces,
    // and not renumber anything. Successive clipping does this once per plane per piece, so a
    // wrong answer here multiplies.
    #[test]
    fn a_plane_that_misses_the_cell_changes_nothing() {
        let (mut arena, cell) = unit_tet();
        let before = arena.points.len();
        let plane = Plane {
            point: Vec3::new(0.0, 0.0, 5.0),
            normal: Vec3::new(0.0, 0.0, 1.0),
        };
        let (below, above) = clip(&cell, plane, &mut arena, TOL);
        assert_eq!(below.as_ref(), Some(&cell));
        assert!(above.is_none());
        assert_eq!(arena.points.len(), before, "a missed plane interns no node");
    }

    // A fragment lying in ONE plane, however many triangles it is cut into, must give exactly one
    // supporting plane - including when some of those triangles face the other way. This is what
    // lets two cells that triangulate the same fragment differently still clip identically.
    #[test]
    fn a_folded_sheet_and_a_split_one_give_the_same_single_plane() {
        let flat = [
            [Vec3::new(0.0, 0.0, 0.5), Vec3::new(1.0, 0.0, 0.5), Vec3::new(0.0, 1.0, 0.5)],
            // Same plane, opposite winding, and a different triangulation of the same region.
            [Vec3::new(1.0, 0.0, 0.5), Vec3::new(0.0, 1.0, 0.5), Vec3::new(1.0, 1.0, 0.5)],
            [Vec3::new(1.0, 1.0, 0.5), Vec3::new(0.0, 1.0, 0.5), Vec3::new(1.0, 0.0, 0.5)],
        ];
        let planes = planes_from_fragment(&flat, QUANTUM);
        assert_eq!(planes.len(), 1, "one plane, whatever the triangulation or winding");
        assert!(
            (planes[0].normal.z.abs() - 1.0).abs() < 1.0e-9,
            "the plane is z = 0.5, got normal {:?}",
            planes[0].normal
        );
    }

    // The property the whole approach turns on: a body crossing NO edge of the cell still cuts it.
    // `planes_through_cut_nodes` cannot do this - with no crossings there are no nodes to fit a
    // plane to - and it is why PLAN §6.14's sub-cell body has been unreachable.
    #[test]
    fn a_fragment_that_touches_no_edge_still_subdivides_the_cell() {
        let (mut arena, cell) = unit_tet();
        // A small triangle strictly inside the tet, near its centroid, touching no edge.
        let fragment = [[
            Vec3::new(0.20, 0.20, 0.20),
            Vec3::new(0.30, 0.22, 0.20),
            Vec3::new(0.22, 0.30, 0.26),
        ]];
        let pieces = subdivide_by_fragment(&cell, &fragment, &mut arena, TOL);
        assert!(
            pieces.len() >= 2,
            "the fragment's plane must cut the cell even though it crosses no edge, got {}",
            pieces.len()
        );
        // And the pieces must add back up to the parent - the same guard §6 puts on every cut.
        let volume = |c: &ConvexCell| -> f64 {
            tetrahedralise(c, &arena)
                .iter()
                .map(|t| {
                    let (a, b, cc, d) = (
                        arena.points[t[0] as usize],
                        arena.points[t[1] as usize],
                        arena.points[t[2] as usize],
                        arena.points[t[3] as usize],
                    );
                    b.sub(a).cross(cc.sub(a)).dot(d.sub(a)).abs() / 6.0
                })
                .sum()
        };
        let total: f64 = pieces.iter().map(volume).sum();
        assert!(
            (total - 1.0 / 6.0).abs() < 1.0e-9,
            "pieces must sum to the unit tet's volume, got {total}"
        );
    }

    // Every piece a fragment induces is convex, so it is star-shaped from any interior point and a
    // fan meshes it. That is precisely the property PLAN §6.23 found the centroid fan losing on
    // traced pieces - 1,628 cells rejected with "a node lies inside a fan face it is not a vertex
    // of" - and it is the reason this route answers that obstacle by construction rather than by a
    // guard.
    #[test]
    fn every_piece_a_fragment_induces_is_convex() {
        let (mut arena, cell) = unit_tet();
        let fragment = [
            [Vec3::new(0.0, 0.0, 0.4), Vec3::new(1.0, 0.0, 0.4), Vec3::new(0.0, 1.0, 0.4)],
            [Vec3::new(0.3, 0.0, 0.0), Vec3::new(0.3, 1.0, 0.0), Vec3::new(0.3, 0.0, 1.0)],
        ];
        let pieces = subdivide_by_fragment(&cell, &fragment, &mut arena, TOL);
        assert!(pieces.len() >= 3, "two crossing planes cut a tet into at least 3, got {}", pieces.len());
        for piece in &pieces {
            let nodes: Vec<u32> = {
                let mut n: Vec<u32> = piece.faces.iter().flatten().copied().collect();
                n.sort_unstable();
                n.dedup();
                n
            };
            for face in &piece.faces {
                let (a, b, c) = (
                    arena.points[face[0] as usize],
                    arena.points[face[1] as usize],
                    arena.points[face[2 % face.len()] as usize],
                );
                let normal = b.sub(a).cross(c.sub(a));
                if normal.dot(normal) <= 0.0 {
                    continue;
                }
                // Convex: every node of the piece is on one side of every face's plane.
                let mut lo = 0.0f64;
                let mut hi = 0.0f64;
                for node in &nodes {
                    let d = normal.dot(arena.points[*node as usize].sub(a));
                    lo = lo.min(d);
                    hi = hi.max(d);
                }
                let scale = normal.dot(normal).sqrt();
                assert!(
                    lo >= -1.0e-9 * scale || hi <= 1.0e-9 * scale,
                    "a piece is not convex: face straddled by {lo} .. {hi}"
                );
            }
        }
    }


    // The conformity property, stated as a test: the trace on a face is the same whichever cell
    // asks for it, because it is computed from the face and the surface and nothing else. Two
    // "cells" here are two different orderings and supersets of the triangle list - the difference
    // a real pair of neighbours would present.
    #[test]
    fn the_trace_on_a_face_is_the_same_from_either_side() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let wall = |y: f64| [
            [Vec3::new(-1.0, y, -1.0), Vec3::new(2.0, y, -1.0), Vec3::new(-1.0, y, 2.0)],
            [Vec3::new(2.0, y, -1.0), Vec3::new(2.0, y, 2.0), Vec3::new(-1.0, y, 2.0)],
        ];
        let mut a = wall(0.4).to_vec();
        // The neighbour sees the same wall plus a triangle far away, in a different order.
        let mut b = vec![[
            Vec3::new(5.0, 5.0, 5.0),
            Vec3::new(6.0, 5.0, 5.0),
            Vec3::new(5.0, 6.0, 5.0),
        ]];
        b.extend(wall(0.4).iter().rev().copied());
        a.reverse();
        let ta = trace_on_face(face, &a, TOL);
        let tb = trace_on_face(face, &b, TOL);
        assert!(!ta.is_empty(), "the wall crosses the face, so there is a trace");
        assert_eq!(ta.len(), tb.len(), "same trace from either side");
        for (l, r) in ta.iter().zip(tb.iter()) {
            for k in 0..2 {
                assert!(
                    l[k].sub(r[k]).dot(l[k].sub(r[k])) < 1.0e-20,
                    "trace endpoints must agree exactly: {:?} vs {:?}",
                    l[k],
                    r[k]
                );
            }
        }
    }

    // A body passing through the face's interior leaves a trace that never reaches the face's
    // boundary - which is what "it crosses no edge of the cell" means, seen from the face. This is
    // the shape PLAN §6.17 measured on a8 and a6a for every one of their sub-cell cells.
    #[test]
    fn a_body_through_the_interior_traces_inside_the_face_only() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        // A thin prism crossing the face well inside it.
        let tris = [
            [Vec3::new(0.2, 0.2, -1.0), Vec3::new(0.3, 0.2, 1.0), Vec3::new(0.2, 0.3, 1.0)],
            [Vec3::new(0.2, 0.2, -1.0), Vec3::new(0.2, 0.3, 1.0), Vec3::new(0.2, 0.3, -1.0)],
        ];
        let trace = trace_on_face(face, &tris, TOL);
        assert!(!trace.is_empty(), "the prism crosses the face");
        for segment in &trace {
            for point in segment {
                assert!(
                    point.x > 1.0e-9 && point.y > 1.0e-9 && point.x + point.y < 1.0 - 1.0e-9,
                    "the trace must stay strictly inside the face, got {point:?}"
                );
            }
        }
    }

    // A surface lying IN the face's plane has an area for a trace, not a curve. That is §6's
    // coincident-surface case and must not be reported here as a chord.
    #[test]
    fn a_coplanar_surface_traces_nothing() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let tris = [[
            Vec3::new(0.1, 0.1, 0.0),
            Vec3::new(0.6, 0.1, 0.0),
            Vec3::new(0.1, 0.6, 0.0),
        ]];
        assert!(trace_on_face(face, &tris, TOL).is_empty());
    }

    // A surface that misses the face entirely, and one that crosses its plane but outside its
    // triangle, must both trace nothing - the clip to the face is what makes the trace local.
    #[test]
    fn a_surface_beside_the_face_traces_nothing() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let away = [[
            Vec3::new(5.0, 5.0, -1.0),
            Vec3::new(6.0, 5.0, 1.0),
            Vec3::new(5.0, 6.0, 1.0),
        ]];
        assert!(trace_on_face(face, &away, TOL).is_empty(), "crosses the plane, misses the triangle");
        let far = [[
            Vec3::new(0.2, 0.2, 1.0),
            Vec3::new(0.3, 0.2, 2.0),
            Vec3::new(0.2, 0.3, 2.0),
        ]];
        assert!(trace_on_face(face, &far, TOL).is_empty(), "never reaches the plane");
    }


    // The area test is the honest one for a triangulation with a hole: the triangles must cover the
    // outer ring MINUS the hole, exactly. Too few and there is a gap; too many and something
    // overlaps; cover the hole as well and the loop is not a hole at all.
    #[test]
    fn a_face_with_an_interior_loop_triangulates_to_the_annulus() {
        let outer = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let hole = [
            Vec3::new(0.2, 0.2, 0.0),
            Vec3::new(0.4, 0.2, 0.0),
            Vec3::new(0.4, 0.4, 0.0),
            Vec3::new(0.2, 0.4, 0.0),
        ];
        let tris = triangulate_with_hole(&outer, &hole).expect("a triangle with a square hole");
        let point = |id: u32| -> Vec3 {
            if (id as usize) < outer.len() { outer[id as usize] } else { hole[id as usize - outer.len()] }
        };
        let total: f64 = tris
            .iter()
            .map(|t| {
                let (a, b, c) = (point(t[0]), point(t[1]), point(t[2]));
                b.sub(a).cross(c.sub(a)).dot(Vec3::new(0.0, 0.0, 1.0)).abs() / 2.0
            })
            .sum();
        let expected = 0.5 - 0.2 * 0.2;
        assert!(
            (total - expected).abs() < 1.0e-9,
            "the triangles must cover the face minus the loop: {total} vs {expected}"
        );
    }

    // Every edge of the hole must survive as an edge of the triangulation - that is the whole point.
    // If the loop is not an edge, the cell-level cut cannot follow it and the boundary chamfers,
    // which is the defect PLAN §6.22 traced to a fan piece carrying mixed labels.
    #[test]
    fn the_interior_loop_survives_as_edges() {
        let outer = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let hole = [
            Vec3::new(0.15, 0.15, 0.0),
            Vec3::new(0.45, 0.20, 0.0),
            Vec3::new(0.25, 0.50, 0.0),
        ];
        let tris = triangulate_with_hole(&outer, &hole).expect("a triangle with a triangular hole");
        let mut edges: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                edges.insert(if a <= b { (a, b) } else { (b, a) });
            }
        }
        for k in 0..hole.len() {
            let a = (outer.len() + k) as u32;
            let b = (outer.len() + (k + 1) % hole.len()) as u32;
            let key = if a <= b { (a, b) } else { (b, a) };
            assert!(edges.contains(&key), "hole edge {key:?} must be an edge of the triangulation");
        }
    }

    // Winding must not matter: the caller gets its rings from `trace_on_face`, whose order comes
    // from the geometry rather than from any orientation convention.
    #[test]
    fn the_hole_triangulates_whichever_way_round_it_is_given() {
        let outer = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let hole = [
            Vec3::new(0.2, 0.2, 0.0),
            Vec3::new(0.4, 0.2, 0.0),
            Vec3::new(0.3, 0.4, 0.0),
        ];
        let mut reversed = hole;
        reversed.reverse();
        let a = triangulate_with_hole(&outer, &hole).expect("forward");
        let b = triangulate_with_hole(&outer, &reversed).expect("reversed");
        let area = |tris: &[[u32; 3]], h: &[Vec3; 3]| -> f64 {
            let point = |id: u32| -> Vec3 {
                if (id as usize) < outer.len() { outer[id as usize] } else { h[id as usize - outer.len()] }
            };
            tris.iter()
                .map(|t| {
                    let (p, q, r) = (point(t[0]), point(t[1]), point(t[2]));
                    q.sub(p).cross(r.sub(p)).dot(Vec3::new(0.0, 0.0, 1.0)).abs() / 2.0
                })
                .sum()
        };
        assert!((area(&a, &hole) - area(&b, &reversed)).abs() < 1.0e-12, "same area either way round");
    }

}
