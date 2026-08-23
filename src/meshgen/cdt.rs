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
// Returns: the subdivision, or the named reason the kernel declined it.
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
) -> Result<CellSubdivision, &'static str> {
    subdivide_cell_by(boundary, nodes, quantum, tol, &[], |arena, to_local| {
        let mut planes: Vec<(Plane, i32)> = Vec::new();
        for (component, ids) in cut_nodes {
            let mapped: Vec<u32> =
                ids.iter().filter_map(|id| to_local.get(id).copied()).collect();
            for plane in planes_through_cut_nodes(&mapped, arena, tol) {
                planes.push((plane, *component));
            }
        }
        planes
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Subdivide one escalated cell by the planes its surface FRAGMENT lies in, rather than by
//   the planes its edge crossings fit - §7.2's stated input, and the one case crossings cannot
//   express.
// Inputs: the cell's closed boundary soup, the fragment's triangles per component, the global node
//   table, the key quantum and the tolerance.
// Returns: the subdivision, or the named reason it declined - the refusal rate per reason is
//   what says which capability is still missing, so it is data rather than a bool.
// Side effects: None.
// Notes: Identical to `subdivide_cell` in everything except where the planes come from, which is
//   the whole point: a body that crosses no edge of the cell leaves no crossing to fit a plane to
//   (PLAN §6.14), while its triangles lie in perfectly good planes. The refusals are the same and
//   they are what keep it conforming - most importantly, **the boundary must come back triangle for
//   triangle**. A fragment's plane cuts the face it passes through, so this will decline every
//   sub-cell body until the face's own triangulation already carries the trace as an edge
//   (`trace_on_face` + `chain_trace` + `triangulate_with_hole`). That refusal is the honest
//   statement of what is still missing, not a limitation to work around.
pub fn subdivide_cell_by_fragment(
    boundary: &[[u32; 3]],
    fragment: &[(i32, Vec<[Vec3; 3]>)],
    nodes: &[Vec3],
    quantum: f64,
    tol: f64,
) -> Result<CellSubdivision, &'static str> {
    let patch: Vec<[Vec3; 3]> = fragment.iter().flat_map(|(_, tris)| tris.iter().copied()).collect();
    subdivide_cell_by(boundary, nodes, quantum, tol, &patch, |arena, _| {
        let mut planes: Vec<(Plane, i32)> = Vec::new();
        for (component, tris) in fragment {
            for plane in planes_from_fragment(tris, arena.quantum) {
                planes.push((plane, *component));
            }
        }
        planes
    })
}

// AI-FUNC-SUMMARY:
// Purpose: The body both entry points share - everything except where the constraint planes come
//   from.
// Inputs: the boundary soup, the node table, the quantum, the tolerance, and a closure producing
//   the planes from the local arena and the global-to-local map.
// Returns: the subdivision, or the refusal that fired, named.
// Side effects: None.
// Notes: Factored out when the fragment-driven entry point landed, so the two cannot drift. Every
//   refusal below is load-bearing and each was added in response to a measured failure; duplicating
//   them into a second function is how one of them would quietly go missing.
fn subdivide_cell_by(
    boundary: &[[u32; 3]],
    nodes: &[Vec3],
    quantum: f64,
    tol: f64,
    patch: &[[Vec3; 3]],
    make_planes: impl FnOnce(&NodeArena, &BTreeMap<u32, u32>) -> Vec<(Plane, i32)>,
) -> Result<CellSubdivision, &'static str> {
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

    let planes = make_planes(&arena, &to_local);
    if planes.is_empty() {
        return Err("no constraint plane");
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
        return Err("boundary references a node outside the cell");
    }
    let only_planes: Vec<Plane> = planes.iter().map(|(p, _)| *p).collect();
    let pieces = subdivide(&cell, &only_planes, &mut arena, tol);
    // **Every new node the clip wants is on this cell's SHARED boundary - measured, not assumed.**
    // The guard refuses any arena growth, on the argument that a node invented here exists in this
    // cell and not in the neighbour. That argument is exactly right about a node on the boundary
    // and says nothing about one strictly INSIDE the cell, which no neighbour can see - and interior
    // nodes arise by construction once there are two planes, since the second crosses the face the
    // first one cut. So the test looked over-strict, and was relaxed to "no new node in the plane of
    // any boundary triangle" and measured: **the refusal counts did not move by one cell** on a8
    // (3,764), a3 (975) or a6a (459). Every blocked cell needs nodes on a face it shares.
    //
    // That is worth more than the relaxation would have been. It says the only way to reach these
    // cells is to put those nodes on the face FIRST, once, so both cells receive them - §7.3's
    // `FaceTriCache` and the `trace_on_face` -> `chain_trace` -> `triangulate_with_hole` chain - and
    // it rules out loosening this guard as a route. The simple test is kept because it is
    // equivalent here and cheaper.
    if arena.points.len() != before {
        // **Where the missing node is decides which mechanism has to supply it**, so the refusal
        // says which. The cell's boundary lies in four planes; a point in exactly one of them is
        // strictly inside a shared FACE, and one in two of them is on a shared lattice EDGE. A face
        // is shared by two cells and its triangulation is §7.3's `FaceTriCache` to fix; an edge is
        // shared by every cell around it and a node on one is `cut_index`'s business. Conflating
        // the two is how a face-side fix gets built for a population that needed an edge-side one.
        let mut boundary_planes: Vec<Plane> = Vec::new();
        for t in boundary {
            let Some(plane) = Plane::of_triangle([
                nodes[t[0] as usize],
                nodes[t[1] as usize],
                nodes[t[2] as usize],
            ]) else {
                continue;
            };
            // Deduplicated: a tet's boundary is four planes however many triangles carry them, and
            // counting one plane twice would read every face-interior point as an edge point.
            if !boundary_planes.iter().any(|seen| {
                seen.normal.sub(plane.normal).dot(seen.normal.sub(plane.normal)) <= tol
                    && seen.distance(plane.point).abs() <= tol
            }) {
                boundary_planes.push(plane);
            }
        }
        // **And whether the surface actually reaches that point.** A supporting plane is infinite
        // while the patch that produced it is not, so a plane can cross a lattice edge the surface
        // never touches - the OVER-CUT. The two have completely different fixes: an over-cut needs
        // the cut bounded to the patch (a PLC mesher), while a node the surface really does reach
        // means a crossing that should already have been interned and was not. Distinguishing them
        // is the difference between building the right thing and the wrong one.
        let reaches = |point: Vec3| -> bool {
            patch.iter().any(|t| {
                let (u, v) = (t[1].sub(t[0]), t[2].sub(t[0]));
                let normal = u.cross(v);
                let area2 = normal.dot(normal);
                if area2 <= 0.0 {
                    return false;
                }
                let rel = point.sub(t[0]);
                if normal.dot(rel).abs() > tol * area2.sqrt() {
                    return false;
                }
                // Barycentric, by the areas of the three sub-triangles against the whole.
                let a = u.cross(rel).dot(normal) / area2;
                let b = rel.cross(v).dot(normal) / area2;
                a >= -1.0e-9 && b >= -1.0e-9 && a + b <= 1.0 + 1.0e-9
            })
        };
        // The cell's own scale, so "close to an existing node" means the same thing at every
        // element size. A wanted node that sits on top of one the cell already has is a
        // near-duplicate the arena's quantum failed to absorb - a numerical question with a cheap
        // answer - while one standing alone is a crossing the pipeline never interned, which is
        // not. Reporting them as one number would hide a cheap fix inside an expensive diagnosis.
        let mut scale: f64 = 0.0;
        for a in 0..before {
            for b in (a + 1)..before {
                let d = arena.points[a].sub(arena.points[b]);
                scale = scale.max(d.dot(d).sqrt());
            }
        }
        let (mut on_face, mut on_edge, mut off_patch, mut duplicate) = (false, false, false, false);
        for id in before..arena.points.len() {
            let point = arena.points[id];
            let planes = boundary_planes
                .iter()
                .filter(|plane| plane.distance(point).abs() <= tol)
                .count();
            match planes {
                0 => {}
                1 => on_face = true,
                _ => on_edge = true,
            }
            if !patch.is_empty() && !reaches(point) {
                off_patch = true;
            }
            let nearest = (0..before)
                .map(|other| {
                    let d = arena.points[other].sub(point);
                    d.dot(d).sqrt()
                })
                .fold(f64::INFINITY, f64::min);
            if scale > 0.0 && nearest < scale * 1.0e-6 {
                duplicate = true;
            }
        }
        return Err(match (on_face, on_edge, off_patch) {
            _ if duplicate => "a NEAR-DUPLICATE of a node the cell already has",
            (_, _, true) => "the plane OVER-CUT: a node where the surface does not reach",
            (true, true, false) => "the clip wanted nodes on both a shared face and a shared edge",
            (false, true, false) => "the clip wanted a node on a shared EDGE",
            (true, false, false) => "the clip wanted a node inside a shared FACE",
            (false, false, false) => "the clip wanted a node strictly inside the cell",
        });
    }
    if pieces.len() < 2 {
        return Err("the planes do not separate the cell");
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
                return Err("a piece face outside every plane is not a triangle");
            }
            let mut k = [back(face[0]), back(face[1]), back(face[2])];
            k.sort_unstable();
            if !original.contains(&k) {
                return Err("the cut re-split a shared boundary triangle");
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
            return Err("a piece has fewer than four faces");
        }
        out.pieces.push(soup);
    }
    Ok(out)
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
// Purpose: The Delaunay tetrahedralisation of a point set — the substrate `SPEC_meshgen_geometry.md`
//   §7.4's constrained incremental tetrahedralisation is built on.
// Inputs: the points and their quantised keys, indexed alike.
// Returns: the tets as index quadruples, each positively oriented, in a canonical order; None when
//   the points are degenerate or an insertion produced an inconsistent cavity.
// Side effects: None — the four super-tet vertices are local and every tet touching one is dropped.
// Notes: **Bowyer-Watson, with the two determinism rules §7.4 states.** Points are inserted in
//   ascending `NodeKey` order, so the sequence is a function of the geometry and not of the
//   caller's indexing; and a point exactly ON a circumsphere is treated as NOT in the cavity, which
//   is the strict form of the in-sphere test. Both matter for the same reason: a cospherical set —
//   the eight corners of a cube, which is the ordinary case here, not a contrived one — admits
//   several valid triangulations, and R-P2 needs the same one every run. Strict exclusion makes the
//   earliest-inserted configuration win, and insertion order is key order, which is §7.4's
//   "ties broken by smallest NodeKey" in the only form that is also transitive.
//
//   **The cavity is flood-filled from the tet containing the point, never collected by scanning.**
//   Scanning finds the same set in exact arithmetic and a *disconnected* one the moment a predicate
//   disagrees with itself, and a disconnected cavity re-triangulates into overlapping tets that no
//   later check would obviously catch. Flooding through shared faces cannot leave the region
//   reachable from the seed, so the failure mode becomes "the cavity is smaller than it should be",
//   which the orientation check below does catch.
//
//   Every emitted tet is verified positively oriented before it is kept, and the whole
//   tetrahedralisation is refused if any is not. This is a kernel that must decline rather than
//   hand back something subtly wrong (§7.2's integration rule).
pub fn delaunay_tets(points: &[Vec3], keys: &[NodeKey]) -> Option<Vec<[u32; 4]>> {
    if points.len() < 4 || points.len() != keys.len() {
        return None;
    }
    // A super-tet that strictly contains every point, far enough out that it does not decide any
    // in-sphere question among the real points. Its vertices are appended to a LOCAL copy of the
    // point list and every tet touching one is dropped at the end, so no identity escapes (§7.2).
    let (mut lo, mut hi) = (points[0], points[0]);
    for p in points {
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    let centre = lo.add(hi).scale(0.5);
    let span = hi.sub(lo);
    let reach = span.dot(span).sqrt().max(f64::MIN_POSITIVE) * 1000.0;
    let mut work: Vec<Vec3> = points.to_vec();
    let base = work.len() as u32;
    for corner in [
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
    ] {
        work.push(centre.add(corner.scale(reach)));
    }
    let oriented = |t: [u32; 4], points: &[Vec3]| -> Option<[u32; 4]> {
        let (a, b, c, d) = (
            points[t[0] as usize],
            points[t[1] as usize],
            points[t[2] as usize],
            points[t[3] as usize],
        );
        match crate::meshgen::predicates::orient3d_filtered(a, b, c, d).0 {
            0 => None,
            s if s > 0 => Some(t),
            _ => Some([t[1], t[0], t[2], t[3]]),
        }
    };
    let mut tets: Vec<[u32; 4]> = vec![oriented([base, base + 1, base + 2, base + 3], &work)?];

    // Ascending key order, so the sequence is a function of the geometry (§7.4).
    let mut order: Vec<u32> = (0..points.len() as u32).collect();
    order.sort_by_key(|id| keys[*id as usize]);
    order.dedup_by_key(|id| keys[*id as usize]);

    for id in order {
        let point = work[id as usize];
        // The cavity: tets whose circumsphere strictly contains the point, flooded from one that
        // does. Any tet containing the point on its boundary also has it on its circumsphere's
        // interior or surface, so the seed search and the in-sphere test agree about where to start.
        let seed = tets.iter().position(|t| {
            crate::meshgen::predicates::insphere(
                work[t[0] as usize],
                work[t[1] as usize],
                work[t[2] as usize],
                work[t[3] as usize],
                point,
            ) > 0
        });
        let Some(seed) = seed else {
            // Outside every circumsphere: the point is already a vertex of the triangulation or
            // coincides with one, and there is nothing to insert.
            continue;
        };
        let mut cavity: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        let mut frontier = vec![seed];
        while let Some(at) = frontier.pop() {
            if !cavity.insert(at) {
                continue;
            }
            for other in 0..tets.len() {
                if cavity.contains(&other) || !share_a_face(tets[at], tets[other]) {
                    continue;
                }
                let t = tets[other];
                if crate::meshgen::predicates::insphere(
                    work[t[0] as usize],
                    work[t[1] as usize],
                    work[t[2] as usize],
                    work[t[3] as usize],
                    point,
                ) > 0
                {
                    frontier.push(other);
                }
            }
        }
        // The cavity's boundary: a face carried by exactly one cavity tet. Counted on the sorted
        // triple so two tets naming the same face in opposite windings are recognised as one.
        let mut faces: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for at in &cavity {
            for face in tet_faces(tets[*at]) {
                let mut key = face;
                key.sort_unstable();
                *faces.entry(key).or_insert(0) += 1;
            }
        }
        let mut next: Vec<[u32; 4]> = Vec::new();
        for (at, tet) in tets.iter().enumerate() {
            if !cavity.contains(&at) {
                next.push(*tet);
            }
        }
        for at in &cavity {
            for face in tet_faces(tets[*at]) {
                let mut key = face;
                key.sort_unstable();
                if faces.get(&key).copied().unwrap_or(0) != 1 {
                    continue;
                }
                // A face of the cavity's boundary, wound outward from the cavity by construction,
                // so the new tet closes onto the point with a consistent orientation.
                let Some(tet) = oriented([face[0], face[1], face[2], id], &work) else {
                    // The point is coplanar with a boundary face: a flat tet, which must never
                    // enter the triangulation. Nothing else can be salvaged from this insertion.
                    return None;
                };
                next.push(tet);
            }
        }
        tets = next;
    }

    // Drop everything touching the super-tet, then canonicalise: each tet rotated to start at its
    // smallest vertex with the orientation preserved, and the list sorted. R-P2 needs the output to
    // be a function of the input, and the insertion order alone does not give that.
    let mut out: Vec<[u32; 4]> = Vec::new();
    for tet in tets {
        if tet.iter().any(|id| *id >= base) {
            continue;
        }
        // Slot 0 is the smallest vertex and slot 1 the smallest of the rest; the remaining two are
        // then ordered by requiring positive orientation. That is unique given the four vertices,
        // so the same tet written any of its 24 ways canonicalises to one form - and swapping the
        // LAST two is the only orientation fix that leaves slots 0 and 1 alone.
        let mut rest: Vec<u32> = tet.iter().copied().filter(|id| *id != tet_min(tet)).collect();
        rest.sort_unstable();
        if rest.len() != 3 {
            // A repeated vertex: a degenerate tet that must not be emitted.
            return None;
        }
        let mut rotated = [tet_min(tet), rest[0], rest[1], rest[2]];
        match crate::meshgen::predicates::orient3d_filtered(
            points[rotated[0] as usize],
            points[rotated[1] as usize],
            points[rotated[2] as usize],
            points[rotated[3] as usize],
        )
        .0
        {
            0 => return None,
            s if s < 0 => rotated.swap(2, 3),
            _ => {}
        }
        out.push(rotated);
    }
    out.sort_unstable();
    out.dedup();
    // An empty result means every tet touched the super-tet, which means the real points span no
    // volume - they are coplanar or worse. That is a refusal, not an answer: an empty list reads as
    // "nothing to do" at the call site and would let a degenerate cell pass silently.
    if out.is_empty() {
        return None;
    }
    Some(out)
}

// AI-FUNC-SUMMARY:
// Purpose: Triangulate one lattice face so that every given segment is an edge of the result —
//   the face-side half of `SPEC_meshgen_geometry.md` §7.4, and what lets a cell's constrained
//   tetrahedralisation meet its neighbour's.
// Inputs: the face's three corners, every vertex on it (corners included), their keys, and the
//   constraint segments as index pairs.
// Returns: the triangles as index triples wound with the face's normal, or the named reason it
//   refused - the refusal rate per reason is what aims the next piece of work, so it is data.
// Side effects: None — it adds no points.
// Notes: **In 2D a constrained Delaunay triangulation always exists without Steiner points**, which
//   is why this half of §7.4 has no refusal case worth designing around while the 3D half does. A
//   segment is recovered by flipping the edges that cross it (Anglada): pop a crossing edge, flip
//   it if its quad is convex and push it back if not, and the loop terminates because every flip
//   strictly reduces the number of crossings that remain.
//
//   **The result is a function of the face and the surface, not of the cell asking.** The points
//   come from `trace_on_face`, which reads only the face and the surface; insertion is in ascending
//   `NodeKey` order; cocircular ties fall to the earliest-inserted configuration; and the crossing
//   list is sorted canonically before it is worked. So the two cells sharing a face derive the same
//   triangulation without communicating, which is invariant J1 and the thing four earlier attempts
//   got wrong by computing the equivalent quantity per cell (PLAN §6.23).
//
//   Every point must lie within the face: the triangulation fills the convex hull of the points,
//   and the face is that hull only if nothing sticks out of it.
pub fn constrained_face_triangulation(
    face: [Vec3; 3],
    points: &[Vec3],
    keys: &[NodeKey],
    segments: &[[u32; 2]],
    weld: f64,
) -> Result<Vec<[u32; 3]>, &'static str> {
    if points.len() < 3 || points.len() != keys.len() {
        return Err("the face has fewer than three points");
    }
    let axis = crate::meshgen::predicates::best_projection_axis(face[0], face[1], face[2]);
    let reference = face[1].sub(face[0]).cross(face[2].sub(face[0]));
    if reference.dot(reference) <= 0.0 {
        return Err("the face is degenerate");
    }
    // The face is convex, so "inside it" is the same sign against all three edges. A point beyond
    // one would put area in the result that is not part of the face.
    //
    // **Tolerated relatively, and the reason is not laziness.** A trace endpoint is produced by
    // clipping a chord to the face in floating point, so it lands within rounding of the edge it
    // ends on - often a few ULP on the wrong side. The exactness that matters here is not that the
    // point is mathematically inside; it is that BOTH CELLS COMPUTE THE SAME POINT, which the
    // quantised key gives. Refusing on an exact test measured 535 of a6a's 970 escalated cells and
    // 427 of a3's 1,088 - the largest face-side refusal by far, and every one of them a rounding
    // artefact rather than a geometry.
    //
    // **Bounded by the pipeline's own coincidence tolerance, not by a private constant.** This test
    // used to refuse at a barycentric coordinate below -1e-9 - a relative bound this one routine
    // invented. On a8 that rejected **two faces** whose worst point is 1.869e-9 of the face outside,
    // which at that case's element size is about 1e-11 in absolute terms: seven orders of magnitude
    // inside `eps`, the tolerance `coincidence: merge` uses to decide that two points ARE the same
    // point. So the pipeline's own rule says the point is on the face and only this line disagreed.
    //
    // The cost of disagreeing is not two faces. A face that will not triangulate excludes its
    // owners from §7.4, the exclusion spreads across augmented faces to their neighbours, and on a8
    // **3 seed cells become 46,596** - which then read §5.2, leave 8,930 split lattice edges whole,
    // and produce 113,859 hanging nodes and 2,268 leaks. Removing every exclusion takes a8 to 6
    // leaks and 66 hanging nodes, and its off-surface area from 0.07089 to 0.01089 against 0.11511
    // shipped, so the amplification is the whole of the failure.
    //
    // The test is now a distance: `|side| / |edge|` is how far the point lies off that edge's line
    // in the face's own projection, and `weld` is the distance below which this pipeline calls two
    // points one. It is the same quantity §6.35 found being used for the ORDERING quantum's job,
    // now supplied where a private relative constant had been standing in for it.
    let outward = crate::meshgen::predicates::orient2d_axis(face[0], face[1], face[2], axis);
    if outward == 0.0 {
        return Err("the face is degenerate");
    }
    use crate::meshgen::predicates::ProjectionAxis;
    let flat = |p: Vec3| match axis {
        ProjectionAxis::X => (p.y, p.z),
        ProjectionAxis::Y => (p.z, p.x),
        ProjectionAxis::Z => (p.x, p.y),
    };
    for point in points {
        for slot in 0..3 {
            let (a, b) = (face[slot], face[(slot + 1) % 3]);
            let side = crate::meshgen::predicates::orient2d_axis(a, b, *point, axis);
            if side / outward >= 0.0 {
                continue;
            }
            let (fa, fb) = (flat(a), flat(b));
            let length = ((fb.0 - fa.0).powi(2) + (fb.1 - fa.1).powi(2)).sqrt();
            if length <= 0.0 || side.abs() / length > weld {
                return Err("a point lies outside the face");
            }
        }
    }

    // --- Delaunay, seeded with the FACE itself ---
    //
    // **No super-triangle.** The usual trick wraps the points in a huge triangle and deletes
    // whatever touches it at the end, which is right only if that triangle is far enough away to
    // lose every in-circle test against the real points. "Far enough" is not a fixed multiple of
    // the span: a sliver triangle's circumradius grows as its height shrinks, and on a3 a face
    // 1e-2 across carried points 1e-7 apart, whose circumcircles have radius some 1e5 times the
    // face. At the customary 1000x span the super-triangle sat well inside them, so a corner
    // inserted late did not invalidate the outer triangles built before it, and the triangulation
    // came back covering a PENTAGON instead of the face - 220 of a3's 24,021 augmented faces,
    // 1,258 boundary leaks behind them (PLAN §6.39).
    //
    // None of that is needed here. Every point is already checked to lie inside the face, so the
    // face IS the convex hull and can be the initial triangulation. Coverage then holds by
    // construction rather than by a distance being large enough, and no test involves a point that
    // is not the caller's.
    // **A constraint that runs through a vertex is two constraints.** The flip recovery can never
    // produce an edge that passes through a third point - no triangulation has one - so a segment
    // whose interior contains a vertex of the face must be split there before recovery starts.
    // Measured on a6a: 216 of the 267 faces that failed to triangulate at all, and every one of
    // them a hanging node in the mesh (PLAN §6.32).
    let mut segments: Vec<[u32; 2]> = segments.to_vec();
    let mut split = 0usize;
    while split < segments.len() {
        let [a, b] = segments[split];
        let (p, q) = (points[a as usize], points[b as usize]);
        let along = q.sub(p);
        let len2 = along.dot(along);
        let through = if len2 > 0.0 {
            (0..points.len() as u32).find(|id| {
                if *id == a || *id == b {
                    return false;
                }
                let rel = points[*id as usize].sub(p);
                let t = rel.dot(along) / len2;
                let off = rel.sub(along.scale(t));
                off.dot(off) <= 1.0e-18 * len2 && (1.0e-9..=1.0 - 1.0e-9).contains(&t)
            })
        } else {
            None
        };
        match through {
            Some(id) => {
                segments[split] = [a, id];
                segments.push([id, b]);
            }
            None => split += 1,
        }
    }
    let segments = &segments[..];
    let work: &[Vec3] = points;
    let base = points.len() as u32;
    // The face's own corners, which the caller supplies among the points; without them there is no
    // hull to seed and the invariant this function rests on does not hold.
    let corner_at = |want: Vec3| -> Option<u32> {
        points
            .iter()
            .position(|p| p.x == want.x && p.y == want.y && p.z == want.z)
            .map(|slot| slot as u32)
    };
    let corners = [corner_at(face[0]), corner_at(face[1]), corner_at(face[2])];
    let (Some(c0), Some(c1), Some(c2)) = (corners[0], corners[1], corners[2]) else {
        return Err("the face's corners are not among its points");
    };
    let mut seed = [c0, c1, c2];
    if crate::meshgen::predicates::orient2d_axis(
        points[c0 as usize],
        points[c1 as usize],
        points[c2 as usize],
        axis,
    ) < 0.0
    {
        seed.swap(1, 2);
    }
    let mut tris: Vec<[u32; 3]> = vec![seed];
    // Which of the face's three edges each point lies on, as a bit per edge - a corner lies on two.
    // Relative to the edge's own length, which is the only scale at which "on it" means anything.
    let rim_of: Vec<u8> = points
        .iter()
        .map(|p| {
            let mut mask = 0u8;
            for slot in 0..3 {
                let (a, b) = (face[slot], face[(slot + 1) % 3]);
                let along = b.sub(a);
                let len2 = along.dot(along);
                if len2 <= 0.0 {
                    continue;
                }
                let rel = p.sub(a);
                let t = rel.dot(along) / len2;
                let off = rel.sub(along.scale(t));
                if off.dot(off) <= 1.0e-18 * len2 && (-1.0e-9..=1.0 + 1.0e-9).contains(&t) {
                    mask |= 1 << slot;
                }
            }
            mask
        })
        .collect();

    let mut order: Vec<u32> = (0..points.len() as u32)
        .filter(|id| *id != c0 && *id != c1 && *id != c2)
        .collect();
    order.sort_by_key(|id| keys[*id as usize]);
    order.dedup_by_key(|id| keys[*id as usize]);
    for id in order {
        let point = work[id as usize];
        // **Flooded from a seed, not filtered over the whole triangulation.** Bowyer-Watson's
        // cavity must be a connected, star-shaped region around the point: its boundary is then a
        // simple polygon and joining the point to each boundary edge retriangulates it exactly.
        // Taking every triangle whose circumcircle contains the point breaks that the moment one of
        // them is a sliver - a sliver's circumcircle is enormous, so a triangle on the far side of
        // the face joins the cavity, the cavity is no longer connected, and the "boundary" read off
        // it is two loops rather than one. The retriangulation then leaves a HOLE: on a3 the
        // triangles came back covering a pentagon instead of the face, 220 of 24,021 faces, and
        // every one of them handed both its owners an open boundary that the fan then coned into a
        // leak - 1,258 of them (PLAN §6.39).
        //
        // `delaunay_tets` has always flooded; this is the 2D half catching up with it.
        let inside = |at: usize| -> bool {
            let t = tris[at];
            crate::meshgen::predicates::incircle_axis(
                work[t[0] as usize],
                work[t[1] as usize],
                work[t[2] as usize],
                point,
                axis,
            ) > 0
        };
        let Some(seed) = (0..tris.len()).find(|at| inside(*at)) else {
            continue;
        };
        let mut cavity: Vec<usize> = Vec::new();
        let mut frontier = vec![seed];
        while let Some(at) = frontier.pop() {
            if cavity.contains(&at) {
                continue;
            }
            cavity.push(at);
            for other in 0..tris.len() {
                if cavity.contains(&other) || !share_an_edge(tris[at], tris[other]) {
                    continue;
                }
                if inside(other) {
                    frontier.push(other);
                }
            }
        }
        cavity.sort_unstable();
        if cavity.is_empty() {
            continue;
        }
        let mut carried: BTreeMap<[u32; 2], usize> = BTreeMap::new();
        for at in &cavity {
            for edge in tri_edges(tris[*at]) {
                *carried.entry(sorted_edge(edge)).or_insert(0) += 1;
            }
        }
        let mut next: Vec<[u32; 3]> = tris
            .iter()
            .enumerate()
            .filter(|(at, _)| !cavity.contains(at))
            .map(|(_, t)| *t)
            .collect();
        for at in &cavity {
            for edge in tri_edges(tris[*at]) {
                if carried.get(&sorted_edge(edge)).copied().unwrap_or(0) != 1 {
                    continue;
                }
                if edge[0] == id || edge[1] == id {
                    continue;
                }
                // **A triangle whose three points all lie on one edge of the face is skipped.**
                // Inserting a point that lies on a boundary edge puts that edge on the cavity's
                // rim; joining it to the point gives a triangle of no area, and dropping it is what
                // refines the rim from `(a,b)` into `(a,p)` and `(p,b)` - the two neighbouring new
                // triangles already carry those.
                //
                // **The test is membership of the edge, not a predicate on the triangle.** Asking
                // `orient2d` whether the three are collinear asks about a quantity of order 1e-19,
                // and a trace point that cut.rs has already placed ON an edge is only within
                // rounding of it: measured at 1.4e-17 off, which an exact predicate calls inside.
                // Each face would then decide for itself whether the point splits the rim - and the
                // two faces sharing that edge project along different axes and hold different other
                // points, so they decide differently and the boundary cracks. Membership is a
                // function of the EDGE, so they cannot (invariant J1).
                if (rim_of[edge[0] as usize] & rim_of[edge[1] as usize] & rim_of[id as usize]) != 0
                {
                    continue;
                }
                next.push([edge[0], edge[1], id]);
            }
        }
        tris = next;
    }
    if tris.is_empty() {
        return Err("the face triangulated to nothing");
    }

    // --- segment recovery by flipping (Anglada) ---
    for segment in segments {
        let (a, b) = (segment[0], segment[1]);
        if a == b || a >= base || b >= base {
            return Err("a segment names a point the face does not have");
        }
        let mut guard = 0usize;
        loop {
            if has_edge(&tris, a, b) {
                break;
            }
            let mut crossing: Vec<[u32; 2]> = Vec::new();
            for t in &tris {
                for edge in tri_edges(*t) {
                    let key = sorted_edge(edge);
                    if crossing.contains(&key) {
                        continue;
                    }
                    if crosses(key, [a, b], &work, axis) {
                        crossing.push(key);
                    }
                }
            }
            // Canonical order, so the flip sequence is a function of the geometry.
            crossing.sort_unstable();
            let Some(flipped) = crossing
                .iter()
                .find_map(|edge| flip_edge(&mut tris, *edge, &work, axis).then_some(*edge))
            else {
                // Nothing crossing the segment can be flipped: every quad is non-convex, which for
                // a planar straight-line graph means the segment passes through a vertex. That is a
                // caller error - the point should have been supplied - not a geometry the flip
                // algorithm has to handle.
                return Err("a segment runs through a vertex and cannot be flipped to");
            };
            let _ = flipped;
            guard += 1;
            if guard > tris.len() * tris.len() + 16 {
                return Err("the flip loop did not terminate");
            }
        }
    }

    // --- wind every triangle with the face ---
    let mut out: Vec<[u32; 3]> = Vec::with_capacity(tris.len());
    for t in tris {
        let (p, q, r) = (
            points[t[0] as usize],
            points[t[1] as usize],
            points[t[2] as usize],
        );
        let normal = q.sub(p).cross(r.sub(p));
        if normal.dot(normal) <= 0.0 {
            return Err("a triangle came out degenerate");
        }
        let mut t = t;
        if normal.dot(reference) < 0.0 {
            t.swap(1, 2);
        }
        let at = (0..3).min_by_key(|slot| t[*slot]).unwrap_or(0);
        out.push([t[at], t[(at + 1) % 3], t[(at + 2) % 3]]);
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

// AI-FUNC-SUMMARY: A triangle's three edges in order; returns them; side effects: none.
// AI-FUNC-SUMMARY: Whether two triangles share an edge; returns bool; side effects: none.
fn share_an_edge(a: [u32; 3], b: [u32; 3]) -> bool {
    a.iter().filter(|id| b.contains(id)).count() >= 2
}

fn tri_edges(t: [u32; 3]) -> [[u32; 2]; 3] {
    [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]]
}

// AI-FUNC-SUMMARY: An edge with its endpoints in ascending order; returns it; side effects: none.
fn sorted_edge(e: [u32; 2]) -> [u32; 2] {
    if e[0] <= e[1] { e } else { [e[1], e[0]] }
}

// AI-FUNC-SUMMARY: Whether some triangle carries the edge (a, b); returns bool; side effects: none.
fn has_edge(tris: &[[u32; 3]], a: u32, b: u32) -> bool {
    tris.iter()
        .any(|t| tri_edges(*t).iter().any(|e| sorted_edge(*e) == sorted_edge([a, b])))
}

// AI-FUNC-SUMMARY:
// Purpose: Whether two segments cross properly — sharing an endpoint does not count.
// Inputs: the two edges as index pairs, the point table, and the projection axis.
// Returns: bool.
// Side effects: None.
// Notes: Proper crossing only. Touching at an endpoint is how a segment meets the edges of the
//   triangles it ends in, and treating that as a crossing would put those edges on the flip list
//   forever.
fn crosses(e: [u32; 2], f: [u32; 2], points: &[Vec3], axis: crate::meshgen::predicates::ProjectionAxis) -> bool {
    if e.iter().any(|id| f.contains(id)) {
        return false;
    }
    let at = |id: u32| points[id as usize];
    let side = |p: u32, q: u32, r: u32| {
        crate::meshgen::predicates::orient2d_axis(at(p), at(q), at(r), axis)
    };
    let (d1, d2) = (side(e[0], e[1], f[0]), side(e[0], e[1], f[1]));
    let (d3, d4) = (side(f[0], f[1], e[0]), side(f[0], f[1], e[1]));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

// AI-FUNC-SUMMARY:
// Purpose: Flip one interior edge, if the quad around it is convex.
// Inputs: the triangles (modified in place), the edge, the point table and the projection axis.
// Returns: whether the flip happened.
// Side effects: Replaces the two triangles sharing the edge with the two across the other diagonal.
// Notes: A non-convex quad cannot be flipped without inverting a triangle, so it is left alone and
//   the caller tries another edge - which is what makes Anglada's loop terminate rather than thrash.
fn flip_edge(
    tris: &mut Vec<[u32; 3]>,
    edge: [u32; 2],
    points: &[Vec3],
    axis: crate::meshgen::predicates::ProjectionAxis,
) -> bool {
    let carrying: Vec<usize> = (0..tris.len())
        .filter(|at| tri_edges(tris[*at]).iter().any(|e| sorted_edge(*e) == edge))
        .collect();
    if carrying.len() != 2 {
        return false;
    }
    let apex = |t: [u32; 3]| -> Option<u32> {
        t.iter().copied().find(|id| !edge.contains(id))
    };
    let (Some(e), Some(f)) = (apex(tris[carrying[0]]), apex(tris[carrying[1]])) else {
        return false;
    };
    if e == f {
        return false;
    }
    let at = |id: u32| points[id as usize];
    let side = |p: u32, q: u32, r: u32| {
        crate::meshgen::predicates::orient2d_axis(at(p), at(q), at(r), axis)
    };
    // Convex exactly when the old diagonal's endpoints straddle the new one.
    if side(e, f, edge[0]) * side(e, f, edge[1]) >= 0.0 {
        return false;
    }
    let (first, second) = (carrying[0].max(carrying[1]), carrying[0].min(carrying[1]));
    tris.remove(first);
    tris.remove(second);
    tris.push([e, f, edge[0]]);
    tris.push([f, e, edge[1]]);
    true
}

// AI-FUNC-SUMMARY:
// Purpose: Tetrahedralise one cell so that its frozen boundary and its surface facets are both
//   faces of the result — `SPEC_meshgen_geometry.md` §7.4's constrained tetrahedralisation.
// Inputs: the points and keys, the frozen boundary triangulation, and the constraint facets as
//   polygons of point indices.
// Returns: the tets, or the named reason the cell could not be meshed this way.
// Side effects: None — it adds no points, so §7.4's Steiner rule is satisfied vacuously.
// Notes: **This lands the VERIFYING half of recovery and refuses the rest, deliberately.** §7.4
//   permits Steiner points off the constraints, and recovering a missing constraint by flips or by
//   insertion is a large machine. Building it before knowing how often the plain Delaunay already
//   respects the constraints would be guessing at the size of the problem — the same mistake the
//   plane-driven route made twice (PLAN §6.24, §6.26). So this checks, and reports which check
//   failed, and the refusal histogram on real cells is what says whether a recovery machine is
//   needed and which kind.
//
//   Two constraints, and they fail differently. The **boundary** is frozen by J1, so the
//   tetrahedralisation's own outer faces must be exactly the triangles handed in — not merely cover
//   the same region, since a neighbour holding the other triangulation of the same quad is a crack.
//   A **facet** need only be a union of faces, because how it is triangulated is this cell's own
//   business; that is checked by area, which is the only test that catches both a facet the mesh
//   cuts through and one it covers twice.
//
//   The Delaunay of the vertex set fills their convex hull, and the cell is a tet with its corners
//   among the points, so "fills the cell" needs no separate check.
// AI-FUNC-SUMMARY:
// Purpose: Remove one edge from a tetrahedralisation by retriangulating the fan around it.
// Inputs: the tets, the points, and the edge as a node pair.
// Returns: the replacement tets, or None when no valid retriangulation exists.
// Side effects: None.
// Notes: The fan around a hull edge is an open strip whose link is a path from one hull
//   face's apex to the other's. Closing that path with the edge the flip creates gives a
//   polygon; every triangulation of it yields tets `(triangle, c)` and `(triangle, d)`,
//   and the triangulation is chosen by a dynamic program that only accepts triangles with
//   `c` and `d` strictly on opposite sides - which is what makes both tets non-degenerate
//   and interior. With a fan of two this is the ordinary 2-2 flip; the general case is the
//   reason a fan of three or more is recoverable at all.
fn remove_edge(
    tets: &[[u32; 4]],
    points: &[Vec3],
    edge: [u32; 2],
) -> Result<Vec<[u32; 4]>, &'static str> {
    let (c, d) = (edge[0], edge[1]);
    let mut fan: Vec<usize> = Vec::new();
    let mut link: BTreeMap<u32, smallvec::SmallVec<[u32; 2]>> = BTreeMap::new();
    for (at, tet) in tets.iter().enumerate() {
        if !tet.contains(&c) || !tet.contains(&d) {
            continue;
        }
        fan.push(at);
        let rest: Vec<u32> = tet.iter().copied().filter(|x| *x != c && *x != d).collect();
        if rest.len() != 2 {
            return Err("a tet on the edge has the wrong shape");
        }
        link.entry(rest[0]).or_default().push(rest[1]);
        link.entry(rest[1]).or_default().push(rest[0]);
    }
    if fan.len() < 2 {
        return Err("only one tet carries the edge");
    }
    // **Two ends means an open strip (a hull edge); no ends means a closed ring (an interior one).**
    // Both are the same operation on the same polygon - the link, closed by the edge the removal
    // creates - and the only difference is where the walk starts. An open fan of n tets has a link
    // of n+1 vertices and becomes 2(n-1) tets; a closed ring of n has a link of n and becomes
    // 2(n-2), which at n = 3 is the ordinary 3-2 flip. Boundary recovery only ever needed the first;
    // recovering a facet's edges needs the second, because the segment to be recovered runs through
    // the interior.
    let ends: Vec<u32> = link
        .iter()
        .filter(|(_, next)| next.len() == 1)
        .map(|(node, _)| *node)
        .collect();
    let closed = ends.is_empty() && link.values().all(|next| next.len() == 2);
    if ends.len() != 2 && !closed {
        return Err("the link is neither an open strip nor a closed ring");
    }
    let start = if closed {
        match link.keys().next() {
            Some(k) => *k,
            None => return Err("the link is empty"),
        }
    } else {
        ends[0]
    };
    let mut path: Vec<u32> = vec![start];
    let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    seen.insert(start);
    loop {
        let Some(last) = path.last().copied() else {
            return Err("the link walk ran out");
        };
        let previous = if path.len() >= 2 { Some(path[path.len() - 2]) } else { None };
        let Some(neighbours) = link.get(&last) else {
            return Err("the link walk ran out");
        };
        let Some(next) = neighbours.iter().copied().find(|x| Some(*x) != previous) else {
            break;
        };
        if next == start {
            break;
        }
        if !seen.insert(next) {
            return Err("the link repeats a vertex");
        }
        path.push(next);
        if !closed && next == ends[1] {
            break;
        }
    }
    let want = if closed { fan.len() } else { fan.len() + 1 };
    if path.len() != want || path.len() < 3 {
        return Err("the link does not close into a polygon");
    }
    let n = path.len();
    let (pc, pd) = (points[c as usize], points[d as usize]);
    // A triangle is usable only if `c` and `d` fall strictly on opposite sides of it: then both
    // `(triangle, c)` and `(triangle, d)` are non-degenerate and lie on the two sides the fan
    // already occupies, so the pair covers the fan's region and nothing else. The score is the
    // rounder of the two tets it makes, measured as volume over longest edge cubed - scale-free, so
    // it means the same thing in a cell of any size.
    let score = |i: usize, k: usize, j: usize| -> f64 {
        let t = [
            points[path[i] as usize],
            points[path[k] as usize],
            points[path[j] as usize],
        ];
        let sc = crate::meshgen::predicates::orient3d_filtered(t[0], t[1], t[2], pc).0;
        let sd = crate::meshgen::predicates::orient3d_filtered(t[0], t[1], t[2], pd).0;
        if sc == 0 || sd == 0 || sc == sd {
            return f64::NEG_INFINITY;
        }
        let mut worst = f64::INFINITY;
        for apex in [pc, pd] {
            let p = [t[0], t[1], t[2], apex];
            let volume = p[1].sub(p[0]).cross(p[2].sub(p[0])).dot(p[3].sub(p[0])).abs() / 6.0;
            let mut longest = 0.0f64;
            for a in 0..4 {
                for b in a + 1..4 {
                    let d = p[b].sub(p[a]);
                    longest = longest.max(d.dot(d).sqrt());
                }
            }
            worst = worst.min(volume / longest.powi(3).max(f64::MIN_POSITIVE));
        }
        worst
    };
    // **Max-min over the whole retriangulation, not the first split that works.** Every feasible
    // triangulation is equally correct, so choosing among them is free - and taking the first one
    // costs real quality: recovery flips quads that are coplanar only to rounding, so a careless
    // split makes a tet whose volume underflows in double even though its exact orientation is
    // positive. `[V1]` reads that as a negative volume, and it went 3 -> 7 before this.
    // `choice[i][j]` is the apex splitting the sub-polygon `path[i..=j]`, `usize::MAX` infeasible.
    let mut choice = vec![vec![usize::MAX; n]; n];
    let mut value = vec![vec![f64::NEG_INFINITY; n]; n];
    for i in 0..n - 1 {
        choice[i][i + 1] = i;
        value[i][i + 1] = f64::INFINITY;
    }
    for span in 2..n {
        for i in 0..n - span {
            let j = i + span;
            for k in i + 1..j {
                if choice[i][k] == usize::MAX || choice[k][j] == usize::MAX {
                    continue;
                }
                let here = score(i, k, j).min(value[i][k]).min(value[k][j]);
                if here > value[i][j] {
                    value[i][j] = here;
                    choice[i][j] = k;
                }
            }
        }
    }
    if choice[0][n - 1] == usize::MAX || !value[0][n - 1].is_finite() {
        return Err("the link polygon has no valid triangulation");
    }
    let mut triangles: Vec<(usize, usize, usize)> = Vec::new();
    let mut stack = vec![(0usize, n - 1)];
    while let Some((i, j)) = stack.pop() {
        if j <= i + 1 {
            continue;
        }
        let k = choice[i][j];
        triangles.push((i, k, j));
        stack.push((i, k));
        stack.push((k, j));
    }
    // **The replacement must occupy exactly the region it replaces, and that is checked, not
    // assumed.** The per-triangle test only asks that `c` and `d` fall either side; for an open fan
    // that is enough, but a closed ring's link polygon can be non-convex, and then a triangulation
    // the test admits can carry tets outside the fan. The two consequences are a hole and an
    // overlap, and both show up as nodes sitting on faces they are not vertices of - 33 hanging
    // nodes on a6a, where there had been none (PLAN §6.44). So: the boundary of the removed region
    // and the boundary of the added one must be the same set of faces, and the volumes must agree.
    let region_boundary = |group: &[[u32; 4]]| -> std::collections::BTreeSet<[u32; 3]> {
        let mut count: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for tet in group {
            for face in tet_faces(*tet) {
                let mut key = face;
                key.sort_unstable();
                *count.entry(key).or_insert(0) += 1;
            }
        }
        count
            .into_iter()
            .filter(|(_, n)| *n == 1)
            .map(|(face, _)| face)
            .collect()
    };
    let volume_of = |group: &[[u32; 4]]| -> f64 {
        group
            .iter()
            .map(|t| {
                let p = [
                    points[t[0] as usize],
                    points[t[1] as usize],
                    points[t[2] as usize],
                    points[t[3] as usize],
                ];
                p[1].sub(p[0]).cross(p[2].sub(p[0])).dot(p[3].sub(p[0])).abs() / 6.0
            })
            .sum()
    };
    let removed: Vec<[u32; 4]> = fan.iter().map(|at| tets[*at]).collect();
    let mut added: Vec<[u32; 4]> = Vec::with_capacity(triangles.len() * 2);
    let mut out: Vec<[u32; 4]> = Vec::with_capacity(tets.len() + triangles.len());
    for (at, tet) in tets.iter().enumerate() {
        if !fan.contains(&at) {
            out.push(*tet);
        }
    }
    for (i, k, j) in triangles {
        let t = [path[i], path[k], path[j]];
        for apex in [c, d] {
            let mut piece = [t[0], t[1], t[2], apex];
            match crate::meshgen::predicates::orient3d_filtered(
                points[piece[0] as usize],
                points[piece[1] as usize],
                points[piece[2] as usize],
                points[piece[3] as usize],
            )
            .0
            {
                0 => return Err("a replacement tet is degenerate"),
                s if s < 0 => piece.swap(0, 1),
                _ => {}
            }
            added.push(piece);
        }
    }
    // The boundary test applies to an INTERIOR edge only. Removing one leaves the fan's boundary
    // faces `(c, vi, vi+1)` and `(d, vi, vi+1)` exactly where they were, so any change is the
    // triangulation escaping the region. Removing a HULL edge deliberately retriangulates the two
    // faces on the hull - that is the whole operation boundary recovery needs - so the same test
    // there would forbid the thing it is for.
    if closed && region_boundary(&removed) != region_boundary(&added) {
        return Err("the replacement does not cover the same region");
    }
    let (before, after) = (volume_of(&removed), volume_of(&added));
    if (before - after).abs() > before.max(after) * 1.0e-9 {
        return Err("the replacement does not preserve the volume");
    }
    out.extend(added);
    Ok(out)
}

// AI-FUNC-SUMMARY:
// Purpose: The 2-3 flip - replace the two tets sharing an interior face with three sharing a new edge.
// Inputs: the tets, the points, and the shared face.
// Returns: the replacement tets, or None when the flip is not valid there.
// Side effects: None.
// Notes: **The inverse of edge removal, and the move the recovery was missing.** `remove_edge`
//   only ever takes edges away; a hull that stalls one or two faces short of the prescribed
//   boundary usually needs one put in, and on a6a *every* cell in that class stalls at one or two
//   faces (PLAN §6.48). Validity is the same question as everywhere else here: the three new tets
//   must each be non-degenerate and must together occupy exactly what the two replaced, which is
//   checked rather than argued.
fn flip_two_three(tets: &[[u32; 4]], points: &[Vec3], face: [u32; 3]) -> Option<Vec<[u32; 4]>> {
    let mut carriers: Vec<usize> = Vec::new();
    for (at, tet) in tets.iter().enumerate() {
        if face.iter().all(|id| tet.contains(id)) {
            carriers.push(at);
        }
    }
    if carriers.len() != 2 {
        return None;
    }
    let apex = |at: usize| -> Option<u32> {
        tets[at].iter().copied().find(|id| !face.contains(id))
    };
    let (d, e) = (apex(carriers[0])?, apex(carriers[1])?);
    if d == e {
        return None;
    }
    let mut added: Vec<[u32; 4]> = Vec::with_capacity(3);
    for slot in 0..3 {
        let (x, y) = (face[slot], face[(slot + 1) % 3]);
        let mut piece = [x, y, d, e];
        match crate::meshgen::predicates::orient3d_filtered(
            points[piece[0] as usize],
            points[piece[1] as usize],
            points[piece[2] as usize],
            points[piece[3] as usize],
        )
        .0
        {
            0 => return None,
            s if s < 0 => piece.swap(0, 1),
            _ => {}
        }
        added.push(piece);
    }
    let removed = [tets[carriers[0]], tets[carriers[1]]];
    let boundary_of = |group: &[[u32; 4]]| -> std::collections::BTreeSet<[u32; 3]> {
        let mut count: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for tet in group {
            for f in tet_faces(*tet) {
                let mut key = f;
                key.sort_unstable();
                *count.entry(key).or_insert(0) += 1;
            }
        }
        count.into_iter().filter(|(_, n)| *n == 1).map(|(f, _)| f).collect()
    };
    let volume_of = |group: &[[u32; 4]]| -> f64 {
        group
            .iter()
            .map(|t| {
                let p = [
                    points[t[0] as usize],
                    points[t[1] as usize],
                    points[t[2] as usize],
                    points[t[3] as usize],
                ];
                p[1].sub(p[0]).cross(p[2].sub(p[0])).dot(p[3].sub(p[0])).abs() / 6.0
            })
            .sum()
    };
    if boundary_of(&removed) != boundary_of(&added) {
        return None;
    }
    let (before, after) = (volume_of(&removed), volume_of(&added));
    if (before - after).abs() > before.max(after) * 1.0e-9 {
        return None;
    }
    let mut out: Vec<[u32; 4]> = Vec::with_capacity(tets.len() + 1);
    for (at, tet) in tets.iter().enumerate() {
        if !carriers.contains(&at) {
            out.push(*tet);
        }
    }
    out.extend(added);
    Some(out)
}

// AI-FUNC-SUMMARY:
// Purpose: Flip a tetrahedralisation's hull until it carries a prescribed boundary.
// Inputs: the tets, the points, and the frozen boundary as sorted node triples.
// Returns: the recovered tets, or None when some prescribed edge cannot be recovered.
// Side effects: None.
// Notes: The cell's region is a convex tet, so a boundary that uses only hull nodes covers
//   the same surface as the hull and differs from it only in how each planar facet is split
//   - measured on a6a, where every one of 2,028 declines was of that kind and none was a
//   node off the hull. Recovery is then 2D edge recovery per facet: for a prescribed edge
//   the hull lacks, remove a hull edge that properly crosses it. Each removal strictly
//   reduces the number of hull edges crossing that prescribed edge, which is what makes the
//   loop terminate rather than merely usually stop.
fn recover_boundary(
    tets: &[[u32; 4]],
    points: &[Vec3],
    frozen: &std::collections::BTreeSet<[u32; 3]>,
) -> Result<Vec<[u32; 4]>, &'static str> {
    let faces_of = |tets: &[[u32; 4]]| -> BTreeMap<[u32; 3], usize> {
        let mut carried: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for tet in tets {
            for face in tet_faces(*tet) {
                let mut key = face;
                key.sort_unstable();
                *carried.entry(key).or_insert(0) += 1;
            }
        }
        carried
    };
    let wrong = |tets: &[[u32; 4]]| -> usize {
        let carried = faces_of(tets);
        let hull: std::collections::BTreeSet<[u32; 3]> = carried
            .iter()
            .filter(|(_, n)| **n == 1)
            .map(|(face, _)| *face)
            .collect();
        hull.symmetric_difference(frozen).count()
    };
    let mut tets = tets.to_vec();
    let mut mismatch = wrong(&tets);
    // Every step strictly reduces the number of hull faces that disagree with the frozen boundary,
    // so the budget is a bound on a decreasing quantity rather than a guess at how long a `loop`
    // might run.
    for _round in 0..mismatch.max(1) * 8 {
        if mismatch == 0 {
            return Ok(tets);
        }
        let carried = faces_of(&tets);
        let hull: std::collections::BTreeSet<[u32; 3]> = carried
            .iter()
            .filter(|(_, n)| **n == 1)
            .map(|(face, _)| *face)
            .collect();

        // **Shaving, which is what the nearly-flat quads need.** A face of the cell carries points
        // that are only within rounding of its plane - 1.7e-14 of the shortest edge, measured - and
        // `delaunay_tets` decides coplanarity with an exact predicate. So a quad the frozen
        // boundary splits one way and the hull splits the other is not a flat quad at all but a
        // real, microscopic tet (orient3d = -1.4e-22 on a6a's first failing cell), and the two
        // boundaries bound genuinely different regions: the hull takes the convex side and the
        // frozen boundary the other. No flip crosses that gap, because a flip preserves volume.
        //
        // A tet every one of whose faces is either a hull face the frozen boundary does NOT want,
        // or a face it does, lies outside the frozen region: deleting it removes only unwanted hull
        // faces and exposes only wanted ones. That is the operation, and it is exact - no tolerance
        // decides it, only membership.
        let shave = (0..tets.len()).find(|at| {
            let mut removes = 0usize;
            for face in tet_faces(tets[*at]) {
                let mut key = face;
                key.sort_unstable();
                if frozen.contains(&key) {
                    continue;
                }
                if hull.contains(&key) {
                    removes += 1;
                    continue;
                }
                return false;
            }
            removes > 0
        });
        if let Some(at) = shave {
            let mut next = tets.clone();
            next.remove(at);
            let after = wrong(&next);
            if after < mismatch {
                tets = next;
                mismatch = after;
                continue;
            }
        }

        // **Then the flips.** A hull edge the frozen boundary does not want is a candidate; the one
        // that is taken is the first, in key order, whose removal leaves fewer hull faces wrong.
        // Selecting by the outcome rather than by an in-plane crossing test is what lets a quad
        // that is coplanar only to rounding be flipped at all - and it is the progress measure that
        // makes the loop terminate, so nothing is lost by dropping the geometric test.
        let mut frozen_edges: std::collections::BTreeSet<[u32; 2]> =
            std::collections::BTreeSet::new();
        for face in frozen {
            for slot in 0..3 {
                let (x, y) = (face[slot], face[(slot + 1) % 3]);
                frozen_edges.insert(if x <= y { [x, y] } else { [y, x] });
            }
        }
        let mut hull_edges: std::collections::BTreeSet<[u32; 2]> =
            std::collections::BTreeSet::new();
        for face in &hull {
            for slot in 0..3 {
                let (x, y) = (face[slot], face[(slot + 1) % 3]);
                hull_edges.insert(if x <= y { [x, y] } else { [y, x] });
            }
        }
        let mut moved = false;
        // Why each candidate failed, so the refusal can say whether the removal was impossible or
        // merely unhelpful. Those want different work: the first is a gap in `remove_edge`, the
        // second a gap in the search.
        let (mut refused, mut unhelpful) = (0usize, 0usize);
        let mut why: Option<&'static str> = None;
        for edge in hull_edges.iter().filter(|e| !frozen_edges.contains(*e)) {
            let next = match remove_edge(&tets, points, *edge) {
                Ok(next) => next,
                Err(reason) => {
                    refused += 1;
                    why = Some(reason);
                    continue;
                }
            };
            let after = wrong(&next);
            if after < mismatch {
                tets = next;
                mismatch = after;
                moved = true;
                break;
            }
            unhelpful += 1;
        }
        if !moved {
            let interior: Vec<[u32; 3]> = carried
                .iter()
                .filter(|(_, n)| **n == 2)
                .map(|(face, _)| *face)
                .collect();
            for face in interior {
                let Some(next) = flip_two_three(&tets, points, face) else { continue };
                let after = wrong(&next);
                if after < mismatch {
                    tets = next;
                    mismatch = after;
                    moved = true;
                    break;
                }
            }
        }
        if !moved {
            // **Which way the hull is wrong, because the two want opposite operations.** The hull
            // and the frozen boundary cover the same nodes, so their disagreement is either faces
            // the hull HAS that the boundary does not want - the hull is too big, and shaving is
            // the operation - or faces the boundary WANTS that the hull has not got, where the
            // hull is too small and nothing can be shaved off. Reporting "no flip" for both puts a
            // class the shave was built for and a class it can never touch under one name, and on
            // a6a that name carries 61.2 % of all the interface area the gated path strands
            // (PLAN §6.48).
            // Both directions are non-empty on every declining cell measured, so neither shaving
            // alone nor adding alone is the missing operation. What is left to ask is HOW FAR the
            // two are apart when the search stalls: a mismatch of one or two faces is a local
            // configuration the flip set does not happen to cover, while a mismatch of dozens means
            // the recovery never got started and the fault is upstream of it.
            let extra = hull.difference(frozen).count();
            let wanted = frozen.difference(&hull).count();
            let _ = (extra, wanted);
            return Err(if refused == 0 && unhelpful == 0 {
                "no hull edge is a candidate for removal at all"
            } else if refused > 0 && unhelpful == 0 {
                // The last refusal reason stands for the class: with every candidate refused it is
                // the operation that is missing, not the choice among candidates.
                why.unwrap_or("every removable hull edge was refused by the fan retriangulation")
            } else if refused == 0 {
                "every hull edge removed cleanly and none brought the hull closer"
            } else {
                "some hull edges refuse to be removed and the rest do not help"
            });
        }
    }
    if mismatch == 0 {
        return Ok(tets);
    }
    Err("boundary recovery did not converge")
}

// AI-FUNC-SUMMARY:
// Purpose: Whether an open segment properly crosses a triangle's interior.
// Inputs: the segment's ends and the triangle's three vertices.
// Returns: true only for a transversal crossing - touching a vertex, an edge or the plane is not one.
// Side effects: None.
// Notes: Exact throughout. The segment must straddle the triangle's plane strictly, and the three
//   `orient3d` values that place the segment's line against the triangle's edges must agree in sign,
//   which is the standard segment-triangle test written so that every degeneracy answers "no"
//   rather than guessing.
fn segment_crosses_triangle(a: Vec3, b: Vec3, p: Vec3, q: Vec3, r: Vec3) -> bool {
    let o = |w: Vec3, x: Vec3, y: Vec3, z: Vec3| {
        crate::meshgen::predicates::orient3d_filtered(w, x, y, z).0
    };
    let (sa, sb) = (o(p, q, r, a), o(p, q, r, b));
    if sa == 0 || sb == 0 || sa == sb {
        return false;
    }
    let (d1, d2, d3) = (o(a, b, p, q), o(a, b, q, r), o(a, b, r, p));
    d1 != 0 && d1 == d2 && d2 == d3
}

// AI-FUNC-SUMMARY:
// Purpose: The faces of a tetrahedralisation whose interiors a segment properly crosses.
// Inputs: the tets, the points, and the segment's two nodes.
// Returns: the crossed faces, deduplicated and in key order.
// Side effects: None.
// Notes: This is the progress measure segment recovery runs on: a segment is an edge of the mesh
//   exactly when nothing crosses it, so a flip that reduces this count has moved towards recovering
//   it and one that does not has not. Counting FACES rather than tets makes the measure independent
//   of how the region either side happens to be filled.
fn crossed_faces(tets: &[[u32; 4]], points: &[Vec3], a: u32, b: u32) -> Vec<[u32; 3]> {
    let (pa, pb) = (points[a as usize], points[b as usize]);
    let mut out: std::collections::BTreeSet<[u32; 3]> = std::collections::BTreeSet::new();
    for tet in tets {
        for face in tet_faces(*tet) {
            if face.contains(&a) || face.contains(&b) {
                continue;
            }
            let mut key = face;
            key.sort_unstable();
            if out.contains(&key) {
                continue;
            }
            if segment_crosses_triangle(
                pa,
                pb,
                points[face[0] as usize],
                points[face[1] as usize],
                points[face[2] as usize],
            ) {
                out.insert(key);
            }
        }
    }
    out.into_iter().collect()
}

// AI-FUNC-SUMMARY:
// Purpose: Make one segment an edge of the tetrahedralisation by removing what crosses it.
// Inputs: the tets, the points, the segment, and the edges that must not be removed.
// Returns: the recovered tets, or None when no removal makes progress.
// Side effects: None.
// Notes: The first half of facet recovery - a facet cannot be a union of mesh faces until its own
//   edges are edges of the mesh, and on a3 that is 853 of the declines. The move is the same edge
//   removal boundary recovery uses, now that it also handles the closed ring an interior edge has;
//   what changes is the choice of victim and the progress measure. A victim is an edge of a face
//   the segment crosses - anything else is not in the way - and a removal is kept only if it leaves
//   strictly fewer crossed faces, which is what makes the loop terminate rather than wander.
//
//   The boundary's own edges are protected. Removing one would retriangulate the cell's outer
//   surface, which its neighbours have already been promised (invariant J1), so a recovery that
//   needed it must fail instead.
fn recover_segment(
    tets: &[[u32; 4]],
    points: &[Vec3],
    a: u32,
    b: u32,
    protected: &std::collections::BTreeSet<[u32; 2]>,
) -> Option<Vec<[u32; 4]>> {
    let key_of = |x: u32, y: u32| if x <= y { [x, y] } else { [y, x] };
    let mut tets = tets.to_vec();
    let mut crossings = crossed_faces(&tets, points, a, b).len();
    for _round in 0..crossings.max(1) * 8 {
        if tets.iter().any(|t| t.contains(&a) && t.contains(&b)) {
            return Some(tets);
        }
        let faces = crossed_faces(&tets, points, a, b);
        if faces.is_empty() {
            // Nothing is in the way and the segment is still not an edge, which means the two nodes
            // are not connected through the region at all. No flip fixes that.
            return None;
        }
        let mut victims: std::collections::BTreeSet<[u32; 2]> =
            std::collections::BTreeSet::new();
        for face in &faces {
            for slot in 0..3 {
                let edge = key_of(face[slot], face[(slot + 1) % 3]);
                if !protected.contains(&edge) {
                    victims.insert(edge);
                }
            }
        }
        let mut moved = false;
        for edge in &victims {
            let Ok(next) = remove_edge(&tets, points, *edge) else { continue };
            let after = crossed_faces(&next, points, a, b).len();
            if after < crossings {
                tets = next;
                crossings = after;
                moved = true;
                break;
            }
        }
        if !moved {
            return None;
        }
    }
    None
}

// AI-FUNC-SUMMARY:
// Purpose: Fan a declined cell in pieces the facets separate, instead of over the whole cell.
// Inputs: the cell's boundary soup, its facets, the point list (which grows by one node per piece),
//   and the cell's relative tolerance.
// Returns: the tets and the piece each belongs to, or None when the facets do not separate the cell.
// Side effects: appends one centroid per piece to `points`.
// Notes: **The fallback is where the damage is, not the kernel.** Measured by path on A-3, the cells
//   §7.4 meshes carry 0.4 % of their interface area off the surface and the cells it declines carry
//   87 %; the declined arm is 95 % of ALL the off-surface area the gated path produces. The reason
//   is not that those cells are hard - it is that the fallback fans the whole cell over its boundary
//   soup and never looks at the surface, so the material boundary comes out as whatever the fan's
//   spokes happen to cut, up to half a cell off. §7.6 has said so in its own comment since P-3.2 and
//   splits escalated cells for exactly this reason; §7.4's decline path never got the same treatment.
//
//   Every piece is convex, which is what makes the fan exact rather than hopeful: the cell is a tet,
//   each facet is planar, and a convex body intersected with halfspaces stays convex - so a piece is
//   star-shaped about any interior point and the centroid fan is a genuine tetrahedralisation of it.
//   The construction is checked rather than argued: a boundary triangle that straddles a facet's
//   plane, a piece whose surface does not close, a degenerate fan tet, or pieces whose volumes do
//   not sum to the cell's, all decline and leave the caller its whole-cell fan.
//
//   `[R1]` is the rule this serves - "no fallback may abandon conformity to the interface" - and the
//   whole-cell fan abandons it by construction.
// Why the split fan declined, counted by reason. Print-only, and filled only when
// `RUSTMSPT_PLC_DIAG` is set: the split is the fallback that keeps the surface, so what stops it is
// exactly the ranking that says where the remaining off-surface area comes from.
static SPLIT_FAN_WHY: std::sync::Mutex<Option<BTreeMap<&'static str, u64>>> =
    std::sync::Mutex::new(None);
static SPLIT_FAN_DIAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

// AI-FUNC-SUMMARY:
// Purpose: Read the per-reason histogram of `facet_split_fan` outcomes.
// Inputs: none.
// Returns: reason -> count, most frequent first; empty unless the diagnostic is on.
// Side effects: None.
pub fn split_fan_refusals() -> Vec<(&'static str, u64)> {
    let guard = SPLIT_FAN_WHY.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<(&'static str, u64)> = guard
        .as_ref()
        .map(|m| m.iter().map(|(k, v)| (*k, *v)).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

fn note_split(reason: &'static str) {
    if *SPLIT_FAN_DIAG.get_or_init(|| std::env::var_os("RUSTMSPT_PLC_DIAG").is_some()) {
        let mut guard = SPLIT_FAN_WHY.lock().unwrap_or_else(|e| e.into_inner());
        *guard.get_or_insert_with(BTreeMap::new).entry(reason).or_insert(0) += 1;
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Orient a closed triangle soup consistently, so its signed volume means something.
// Inputs: the soup.
// Returns: the same triangles with a consistent winding, or None when it is not a closed manifold.
// Side effects: None.
// Notes: **The volume test needs this and the closure test comes free with it.** A piece is built by
//   taking part of the cell's boundary and capping it with the surface's own triangles, and those
//   two arrive with unrelated windings - so summing `p0 . (p1 x p2)` over the raw soup measures
//   `outer - cap` on one side and `outer + cap` on the other, and the partition test then rejects
//   every split (1,314 of A-3's 1,835 cells, before this). Propagating one orientation across shared
//   edges fixes the winding and, in the same pass, refuses a soup that is not a closed manifold:
//   every edge must be walked once each way, which is exactly what a closed surface means.
fn orient_soup(soup: &[[u32; 3]]) -> Option<Vec<[u32; 3]>> {
    if soup.is_empty() {
        return None;
    }
    let mut by_edge: BTreeMap<[u32; 2], smallvec::SmallVec<[usize; 2]>> = BTreeMap::new();
    for (at, t) in soup.iter().enumerate() {
        for slot in 0..3 {
            let (x, y) = (t[slot], t[(slot + 1) % 3]);
            by_edge
                .entry(if x <= y { [x, y] } else { [y, x] })
                .or_default()
                .push(at);
        }
    }
    if by_edge.values().any(|on| on.len() != 2) {
        return None;
    }
    let mut out: Vec<[u32; 3]> = soup.to_vec();
    let mut fixed = vec![false; soup.len()];
    let mut queue = vec![0usize];
    fixed[0] = true;
    while let Some(at) = queue.pop() {
        let here = out[at];
        for slot in 0..3 {
            let (x, y) = (here[slot], here[(slot + 1) % 3]);
            let key = if x <= y { [x, y] } else { [y, x] };
            let Some(on) = by_edge.get(&key) else { return None };
            let Some(other) = on.iter().copied().find(|o| *o != at) else {
                return None;
            };
            // The neighbour must walk this edge the other way round. If it is already fixed and
            // does not, the soup is not orientable.
            let walks = |t: [u32; 3], a: u32, b: u32| {
                (0..3).any(|slot| t[slot] == a && t[(slot + 1) % 3] == b)
            };
            if fixed[other] {
                if walks(out[other], x, y) == walks(here, x, y) {
                    return None;
                }
                continue;
            }
            if walks(out[other], x, y) {
                out[other].swap(0, 1);
            }
            fixed[other] = true;
            queue.push(other);
        }
    }
    fixed.iter().all(|f| *f).then_some(out)
}

pub fn facet_split_fan(
    boundary: &[[u32; 3]],
    caps: &[Vec<[u32; 3]>],
    side_of: &dyn Fn(usize, u32) -> Option<bool>,
    side_of_face: &dyn Fn(usize, [u32; 3]) -> Option<bool>,
    points: &mut Vec<Vec3>,
    keys: &[NodeKey],
    tol: f64,
) -> Option<(Vec<[u32; 4]>, Vec<u32>)> {
    note_split(match caps.len() {
        0 => "the cell has no facet",
        1 => "offered: one surface",
        _ => "offered: two or more surfaces",
    });
    let mut pieces: Vec<Vec<[u32; 3]>> = vec![boundary.to_vec()];
    for (group, cap) in caps.iter().enumerate() {
        if cap.is_empty() {
            continue;
        }
        let mut next: Vec<Vec<[u32; 3]>> = Vec::with_capacity(pieces.len() + 1);
        // **A cap may cut at most one piece.** The cap is a whole surface patch, and adding all of
        // it to both halves of two different pieces puts each of its triangles into four - which
        // `[V3]` reads exactly as it is, a face shared by four tets and six non-manifold edges
        // around it. Where a second surface really does cross both pieces, only the part of it
        // inside each belongs there, and clipping the patch per piece is a different piece of work;
        // until it exists the cell declines and keeps its whole-cell fan.
        let mut cuts = 0usize;
        for piece in pieces {
            let (mut above, mut below) = (Vec::new(), Vec::new());
            let mut refused: Option<&'static str> = None;
            for triangle in &piece {
                let mut side: Option<bool> = None;
                for node in triangle {
                    match side_of(group, *node) {
                        None => {}
                        Some(here) => match side {
                            None => side = Some(here),
                            Some(there) if there == here => {}
                            Some(_) => {
                                refused =
                                    Some("a boundary triangle straddles the surface with no node on it");
                                break;
                            }
                        },
                    }
                }
                if refused.is_some() {
                    break;
                }
                match side {
                    Some(true) => above.push(*triangle),
                    Some(false) => below.push(*triangle),
                    // Every corner on the surface does not mean the triangle has no side - a face
                    // the surface crosses twice is triangulated with both chords as edges, and the
                    // strip between them has all its corners on the surface while lying squarely in
                    // the material between. §7.6 learned this the same way; the thing being placed
                    // is the triangle's own interior, so ask about that.
                    None => match side_of_face(group, *triangle) {
                        Some(true) => above.push(*triangle),
                        Some(false) => below.push(*triangle),
                        None => {
                            refused = Some("a boundary triangle lies wholly on the surface");
                            break;
                        }
                    },
                }
            }
            if let Some(reason) = refused {
                note_split(reason);
                return None;
            }
            if above.is_empty() || below.is_empty() {
                next.push(piece);
                continue;
            }
            // The cap is the surface's own triangles, so the interface this fallback emits IS the
            // input surface rather than whatever a spoke happened to cut. Winding does not matter:
            // every fan tet is oriented positively below, and both the closure test and the volume
            // test are orientation-free.
            cuts += 1;
            if cuts > 1 {
                note_split("a second surface cuts more than one piece");
                return None;
            }
            above.extend(cap.iter().copied());
            below.extend(cap.iter().copied());
            next.push(above);
            next.push(below);
        }
        pieces = next;
    }
    if pieces.len() < 2 {
        note_split("no surface separates the cell into two pieces");
        return None;
    }
    let soup_volume = |soup: &[[u32; 3]], points: &[Vec3]| -> f64 {
        let mut sum = 0.0;
        for t in soup {
            let p = [
                points[t[0] as usize],
                points[t[1] as usize],
                points[t[2] as usize],
            ];
            sum += p[0].cross(p[1]).dot(p[2]) / 6.0;
        }
        sum.abs()
    };
    // **The cell's own volume must be read from an ORIENTED soup too.** The boundary arrives as the
    // four faces' triangulations with no shared winding, and `p0 . (p1 x p2)` summed over an
    // unoriented soup is not a volume - it was the reference this test compared against, and it made
    // the comparison meaningless.
    let Some(oriented_boundary) = orient_soup(boundary) else {
        note_split("the cell's own boundary is not a closed manifold");
        return None;
    };
    let whole = soup_volume(&oriented_boundary, points);
    let mut tets: Vec<[u32; 4]> = Vec::new();
    let mut regions: Vec<u32> = Vec::new();
    let mut summed = 0.0;
    for (region, piece) in pieces.iter().enumerate() {
        // Closed and consistently wound, or the cap did not cover the cross-section and the piece
        // is not a body. Both questions are the one walk.
        let Some(piece) = orient_soup(piece) else {
            note_split("a piece's surface does not close - the cut does not span the cell");
            return None;
        };
        let piece = &piece;
        let mut nodes: Vec<u32> = piece.iter().flatten().copied().collect();
        nodes.sort_unstable();
        nodes.dedup();
        // **A piece is CONVEX with a prescribed boundary and no interior constraint, which is the
        // easy case of the very kernel that declined the cell.** The cell declined because of its
        // facets; here the facet has become part of the piece's own boundary, so the only thing left
        // to recover is the hull - and that succeeds far more often. The reward is threefold: no new
        // node, fewer tets, and a Delaunay tetrahedralisation instead of a fan of slivers. A fan
        // cones a flat-ish piece to one point and every tet it makes is a pancake, which is where
        // both the element count and the dihedral floor were going.
        let filled = (|| {
            if nodes.iter().any(|n| *n as usize >= keys.len()) {
                return None;
            }
            let local: BTreeMap<u32, u32> = nodes
                .iter()
                .enumerate()
                .map(|(slot, node)| (*node, slot as u32))
                .collect();
            let sub_points: Vec<Vec3> = nodes.iter().map(|n| points[*n as usize]).collect();
            let sub_keys: Vec<NodeKey> = nodes.iter().map(|n| keys[*n as usize]).collect();
            let sub_boundary: Vec<[u32; 3]> = piece
                .iter()
                .map(|t| [local[&t[0]], local[&t[1]], local[&t[2]]])
                .collect();
            constrained_tets(&sub_points, &sub_keys, &sub_boundary, &[], tol)
                .map_err(|reason| {
                    note_split(match reason {
                        "the boundary is split differently, but on the same nodes" => {
                            "piece refused: the hull splits the boundary differently"
                        }
                        "the boundary uses a node that is not on the hull" => {
                            "piece refused: a boundary node is not on the hull"
                        }
                        "the hull carries a node the boundary has never heard of" => {
                            "piece refused: the hull carries an extra node"
                        }
                        "a tet is thinner than the node quantum" => {
                            "piece refused: a tet is thinner than the node quantum"
                        }
                        "the points have no tetrahedralisation" => {
                            "piece refused: the points have no tetrahedralisation"
                        }
                        _ => "piece refused: some other reason",
                    })
                })
                .ok()
                .map(|sub| {
                    sub.iter()
                        .map(|t| {
                            [
                                nodes[t[0] as usize],
                                nodes[t[1] as usize],
                                nodes[t[2] as usize],
                                nodes[t[3] as usize],
                            ]
                        })
                        .collect::<Vec<[u32; 4]>>()
                })
        })();
        match filled {
            Some(sub) => {
                note_split("a piece was tetrahedralised, not fanned");
                for piece_tet in sub {
                    tets.push(piece_tet);
                    regions.push(region as u32);
                }
            }
            None => {
                let mut centre = Vec3::new(0.0, 0.0, 0.0);
                for node in &nodes {
                    centre = centre.add(points[*node as usize]);
                }
                let centre = centre.scale(1.0 / nodes.len() as f64);
                let apex = points.len() as u32;
                points.push(centre);
                for t in piece {
                    let mut piece_tet = [t[0], t[1], t[2], apex];
                    match crate::meshgen::predicates::orient3d_filtered(
                        points[piece_tet[0] as usize],
                        points[piece_tet[1] as usize],
                        points[piece_tet[2] as usize],
                        points[piece_tet[3] as usize],
                    )
                    .0
                    {
                        0 => {
                            note_split("a fan tet is degenerate");
                            return None;
                        }
                        s if s < 0 => piece_tet.swap(0, 1),
                        _ => {}
                    }
                    tets.push(piece_tet);
                    regions.push(region as u32);
                }
            }
        }
        summed += soup_volume(piece, points);
    }
    // The pieces must partition the cell: neither overlapping nor leaving a hole. This is also the
    // test that catches a piece the centroid does not see all of - a non-convex piece whose fan
    // folds over itself sums to more than it occupies.
    if (summed - whole).abs() > whole.max(summed) * 1.0e-9 {
        note_split("the pieces do not partition the cell");
        return None;
    }
    let _ = tol;
    note_split("BUILT");
    Some((tets, regions))
}

// AI-FUNC-SUMMARY:
// Purpose: A tet's smallest dihedral angle, in degrees.
// Inputs: its four points.
// Returns: the least angle between two of its faces, 0 for a degenerate tet.
// Side effects: None.
// Notes: The same quantity `[V4]` reports, computed the same way - the angle on edge (a, b) is
//   between the two other vertices once the edge direction is projected out. Written here so the
//   optimiser and the verifier are scoring the same thing; an optimiser tuned on a different measure
//   improves a number nobody checks.
fn min_dihedral_deg(p: [Vec3; 4]) -> f64 {
    let mut least = 180.0f64;
    for (a, b, c, d) in [
        (0, 1, 2, 3),
        (0, 2, 1, 3),
        (0, 3, 1, 2),
        (1, 2, 0, 3),
        (1, 3, 0, 2),
        (2, 3, 0, 1),
    ] {
        let axis = p[b].sub(p[a]);
        let length = axis.dot(axis).sqrt();
        if length <= 0.0 {
            return 0.0;
        }
        let axis = axis.scale(1.0 / length);
        let drop = |q: Vec3| {
            let rel = q.sub(p[a]);
            rel.sub(axis.scale(rel.dot(axis)))
        };
        let (u, v) = (drop(p[c]), drop(p[d]));
        let (nu, nv) = (u.dot(u).sqrt(), v.dot(v).sqrt());
        if nu <= 0.0 || nv <= 0.0 {
            return 0.0;
        }
        least = least.min((u.dot(v) / (nu * nv)).clamp(-1.0, 1.0).acos().to_degrees());
    }
    least
}

// AI-FUNC-SUMMARY:
// Purpose: The worst dihedral among the tets that carry a given face or edge.
// Inputs: the tets, the points, and a predicate selecting which tets count.
// Returns: the least dihedral over the selected tets, 180 when none.
// Side effects: None.
fn worst_dihedral(tets: &[[u32; 4]], points: &[Vec3], pick: &dyn Fn(&[u32; 4]) -> bool) -> f64 {
    tets.iter()
        .filter(|t| pick(t))
        .map(|t| {
            min_dihedral_deg([
                points[t[0] as usize],
                points[t[1] as usize],
                points[t[2] as usize],
                points[t[3] as usize],
            ])
        })
        .fold(180.0f64, f64::min)
}

// AI-FUNC-SUMMARY:
// Purpose: Flip slivers out of a finished cell without moving its boundary or its facets.
// Inputs: the tets, the points, the prescribed boundary, the facets, and a round budget.
// Returns: the improved tets, or the input when nothing helped.
// Side effects: None.
// Notes: **The third goal property, and the only one the gated path still loses on.** Charging bad
//   elements to the arm that emitted them says §5.2's table produces 0.02 % of tets below 10° on a1
//   and §7.4 produces 19.6 %, so this is the kernel's own problem and not the fallback's. The cause
//   is structural rather than a bug: every point of a PLC cell lies on its boundary or on a facet
//   that spans it, so there are NO interior points, and a Delaunay tetrahedralisation of points in
//   convex position is where slivers live.
//
//   Flips first, because they cost no elements - `[R2]` is a gate, and a Steiner point pays for
//   quality in the currency the goal also counts. A move is taken only when the least dihedral among
//   the tets it touches strictly improves, so the cell's own worst element is the measure and every
//   accepted move raises it. Both the boundary and the facets are protected: a 2-3 flip is offered
//   only interior faces, and edge removal only edges neither the hull nor a facet owns, so nothing
//   the recovery established can be undone here.
fn improve_dihedral(
    tets: &[[u32; 4]],
    points: &[Vec3],
    facets: &[Vec<u32>],
    rounds: usize,
) -> Vec<[u32; 4]> {
    let mut tets = tets.to_vec();
    let key_of = |x: u32, y: u32| if x <= y { [x, y] } else { [y, x] };
    // A face all of whose corners belong to one facet may lie IN that facet and carry the interface;
    // flipping it away would undo the recovery. Over-protecting here is free, since such faces are
    // few and a flip elsewhere is always available.
    let facet_nodes: Vec<std::collections::BTreeSet<u32>> = facets
        .iter()
        .map(|f| f.iter().copied().collect())
        .collect();
    let on_facet = |t: &[u32; 3]| {
        facet_nodes
            .iter()
            .any(|set| t.iter().all(|node| set.contains(node)))
    };
    let mut protected_edges: std::collections::BTreeSet<[u32; 2]> =
        std::collections::BTreeSet::new();
    for facet in facets {
        for slot in 0..facet.len() {
            protected_edges.insert(key_of(facet[slot], facet[(slot + 1) % facet.len()]));
        }
    }
    for _round in 0..rounds {
        let mut carried: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for tet in &tets {
            for face in tet_faces(*tet) {
                let mut key = face;
                key.sort_unstable();
                *carried.entry(key).or_insert(0) += 1;
            }
        }
        let mut hull_edges: std::collections::BTreeSet<[u32; 2]> =
            std::collections::BTreeSet::new();
        for (face, count) in &carried {
            if *count != 1 {
                continue;
            }
            for slot in 0..3 {
                hull_edges.insert(key_of(face[slot], face[(slot + 1) % 3]));
            }
        }
        let mut moved = false;
        // 2-3 first: it is the move that puts an edge IN, and a sliver is usually short of one.
        for (face, count) in &carried {
            if *count != 2 || on_facet(face) {
                continue;
            }
            let before = worst_dihedral(&tets, points, &|t| face.iter().all(|n| t.contains(n)));
            let Some(next) = flip_two_three(&tets, points, *face) else { continue };
            let touched: std::collections::BTreeSet<u32> = face.iter().copied().collect();
            let after = worst_dihedral(&next, points, &|t| {
                touched.iter().filter(|n| t.contains(n)).count() >= 2
                    && t.iter().any(|n| !touched.contains(n))
            });
            if after > before {
                tets = next;
                moved = true;
                break;
            }
        }
        if moved {
            continue;
        }
        // Then edge removal, which takes an edge OUT. Only interior edges neither the hull nor a
        // facet owns, so the cell's outer surface and its interface both stay exactly as recovered.
        let mut edges: std::collections::BTreeSet<[u32; 2]> = std::collections::BTreeSet::new();
        for tet in &tets {
            for pair in [[0usize, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]] {
                edges.insert(key_of(tet[pair[0]], tet[pair[1]]));
            }
        }
        for edge in &edges {
            if hull_edges.contains(edge) || protected_edges.contains(edge) {
                continue;
            }
            let before = worst_dihedral(&tets, points, &|t| {
                t.contains(&edge[0]) && t.contains(&edge[1])
            });
            let Ok(next) = remove_edge(&tets, points, *edge) else { continue };
            let ends: [u32; 2] = *edge;
            let after = worst_dihedral(&next, points, &|t| {
                t.contains(&ends[0]) || t.contains(&ends[1])
            });
            if after > before {
                tets = next;
                moved = true;
                break;
            }
        }
        if !moved {
            break;
        }
    }
    tets
}

// AI-FUNC-SUMMARY:
// Purpose: The mesh edges that properly cross a facet's interior.
// Inputs: the tets, the points, and one facet polygon.
// Returns: the crossing edges, deduplicated and in key order.
// Side effects: None.
// Notes: A facet is a union of mesh faces exactly when nothing crosses its interior, so this is the
//   progress measure interior recovery runs on - the same shape of measure `crossed_faces` is for a
//   segment, with the roles of edge and face exchanged. The facet is a triangle clipped by a tet,
//   so it is CONVEX and the fan from its first vertex covers it; every test is
//   `segment_crosses_triangle`, which is exact and answers "no" to every degeneracy, so an edge
//   lying IN the facet's plane or touching its rim is not counted - which is right, since neither
//   obstructs the facet.
fn facet_crossing_edges(
    tets: &[[u32; 4]],
    points: &[Vec3],
    facet: &[u32],
) -> std::collections::BTreeSet<[u32; 2]> {
    let mut out: std::collections::BTreeSet<[u32; 2]> = std::collections::BTreeSet::new();
    if facet.len() < 3 {
        return out;
    }
    let fan: Vec<[u32; 3]> = (1..facet.len() - 1)
        .map(|at| [facet[0], facet[at], facet[at + 1]])
        .collect();
    for tet in tets {
        for pair in [[0usize, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]] {
            let (x, y) = (tet[pair[0]], tet[pair[1]]);
            let key = if x <= y { [x, y] } else { [y, x] };
            if out.contains(&key) {
                continue;
            }
            for tri in &fan {
                if tri.contains(&x) || tri.contains(&y) {
                    continue;
                }
                if segment_crosses_triangle(
                    points[x as usize],
                    points[y as usize],
                    points[tri[0] as usize],
                    points[tri[1] as usize],
                    points[tri[2] as usize],
                ) {
                    out.insert(key);
                    break;
                }
            }
        }
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Make one facet's interior a union of mesh faces, its edges already being mesh edges.
// Inputs: the tets, the points, the facet, and the edges that may not be removed.
// Returns: the recovered tets, or None when no removal reduces the crossings.
// Side effects: None.
// Notes: **The second half of facet recovery, and the half that had never been built.** Recovering
//   a facet's EDGES leaves its interior still crossed, and on A-3 that residue is the largest single
//   decline class - 42.3 % of all the interface area the gated path strands, against 31.0 % for the
//   link polygon and 17.1 % for the edges themselves. The operation is the one already here: an edge
//   crossing the facet is removed, and the removal is kept only when strictly fewer edges cross
//   afterwards, so the measure is a decreasing integer and the loop's budget bounds it rather than
//   guesses at it. The hull's edges and every facet's edges are protected, so this cannot undo the
//   boundary recovery or the edge recovery that ran before it.
//
//   **It converts 14 of A-3's 1,849 declining cells and no more, and the reason is worth keeping.**
//   Almost every removal it wants is refused by `remove_edge`, on the one line that is 90.4 % of
//   ALL its refusals - "the link polygon has no valid triangulation". So this is not a weak
//   recovery, it is a recovery starved of its one operation, and what it measures is how much of
//   the facet-interior class sits behind that single refusal (PLAN §6.49).
fn recover_facet_interior(
    tets: &[[u32; 4]],
    points: &[Vec3],
    facet: &[u32],
    protected: &std::collections::BTreeSet<[u32; 2]>,
) -> Option<Vec<[u32; 4]>> {
    let mut tets = tets.to_vec();
    let mut crossings = facet_crossing_edges(&tets, points, facet).len();
    for _round in 0..crossings.max(1) * 8 {
        if crossings == 0 {
            return Some(tets);
        }
        let mut moved = false;
        for edge in facet_crossing_edges(&tets, points, facet) {
            if protected.contains(&edge) {
                continue;
            }
            let Ok(next) = remove_edge(&tets, points, edge) else { continue };
            let after = facet_crossing_edges(&next, points, facet).len();
            if after < crossings {
                tets = next;
                crossings = after;
                moved = true;
                break;
            }
        }
        if !moved {
            return None;
        }
    }
    None
}

// AI-FUNC-SUMMARY:
// Purpose: Make every facet's edges edges of the tetrahedralisation.
// Inputs: the tets, the points, the facets, and the boundary's edges, which are protected.
// Returns: the recovered tets, or None if any facet edge cannot be recovered.
// Side effects: None.
// Notes: All or nothing per call: a half-recovered facet is not a constraint the caller can use,
//   and `constrained_tets` re-checks everything afterwards anyway.
fn recover_facet_edges(
    tets: &[[u32; 4]],
    points: &[Vec3],
    facets: &[Vec<u32>],
    protected: &std::collections::BTreeSet<[u32; 2]>,
) -> Option<Vec<[u32; 4]>> {
    let key_of = |x: u32, y: u32| if x <= y { [x, y] } else { [y, x] };
    let mut tets = tets.to_vec();
    for facet in facets {
        if facet.len() < 3 {
            continue;
        }
        for slot in 0..facet.len() {
            let (a, b) = (facet[slot], facet[(slot + 1) % facet.len()]);
            if a == b {
                continue;
            }
            let mut present = tets.iter().any(|t| t.contains(&a) && t.contains(&b));
            if present {
                continue;
            }
            let mut guard = protected.clone();
            // The facet's own edges are protected too, so recovering one does not undo another.
            for other in 0..facet.len() {
                guard.insert(key_of(facet[other], facet[(other + 1) % facet.len()]));
            }
            tets = recover_segment(&tets, points, a, b, &guard)?;
            present = tets.iter().any(|t| t.contains(&a) && t.contains(&b));
            if !present {
                return None;
            }
        }
    }
    Some(tets)
}

// AI-FUNC-SUMMARY:
// Purpose: Remove tets thinner than a bound by edge removal, without touching protected edges.
// Inputs: the tets, the points, the thinness bound as a height, and the edges that must not go.
// Returns: the tets with as many thin ones removed as edge removal can manage.
// Side effects: None.
// Notes: **A conforming refusal is the correct fallback and a poor destination.** Declining a cell
//   for one unusable tet hands the whole cell to the centroid fan, and the fan is measured at four
//   thousand times the off-surface area per interface face - on a6a that one refusal became 47.6 %
//   of all the P3 area the gated path strands, from 88 cells (PLAN §6.47). Removing the tet keeps
//   the cell, and the machinery is the same edge removal facet recovery uses.
//
//   A removal is kept only if it strictly reduces the number of too-thin tets, so the loop cannot
//   trade one for another and cannot fail to terminate.
fn remove_thin_tets(
    tets: &[[u32; 4]],
    points: &[Vec3],
    height: f64,
    protected: &std::collections::BTreeSet<[u32; 2]>,
) -> Vec<[u32; 4]> {
    let thin = |t: &[u32; 4]| -> bool {
        let p = [
            points[t[0] as usize],
            points[t[1] as usize],
            points[t[2] as usize],
            points[t[3] as usize],
        ];
        let volume = crate::meshgen::predicates::tet_signed_volume(p[0], p[1], p[2], p[3]).abs();
        let mut longest = 0.0f64;
        for a in 0..4 {
            for b in a + 1..4 {
                let d = p[b].sub(p[a]);
                longest = longest.max(d.dot(d).sqrt());
            }
        }
        volume <= longest * longest * height
    };
    let mut tets = tets.to_vec();
    let mut count = tets.iter().filter(|t| thin(t)).count();
    for _round in 0..count.max(1) * 4 {
        if count == 0 {
            return tets;
        }
        let Some(at) = tets.iter().position(thin) else { return tets };
        let victim = tets[at];
        let mut moved = false;
        for pair in [[0usize, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]] {
            let (x, y) = (victim[pair[0]], victim[pair[1]]);
            let edge = if x <= y { [x, y] } else { [y, x] };
            if protected.contains(&edge) {
                continue;
            }
            let Ok(next) = remove_edge(&tets, points, edge) else { continue };
            let after = next.iter().filter(|t| thin(t)).count();
            if after < count {
                tets = next;
                count = after;
                moved = true;
                break;
            }
        }
        if !moved {
            return tets;
        }
    }
    tets
}

pub fn constrained_tets(
    points: &[Vec3],
    keys: &[NodeKey],
    boundary: &[[u32; 3]],
    facets: &[Vec<u32>],
    tol: f64,
) -> Result<Vec<[u32; 4]>, &'static str> {
    let Some(tets) = delaunay_tets(points, keys) else {
        return Err("the points have no tetrahedralisation");
    };
    // **A facet edge that runs through a node is two facet edges.** The same rule
    // `constrained_face_triangulation` applies to its 2D constraints (§6.32), now applied in 3D
    // because the facet's rim comes from the faces and can therefore run along a lattice edge that a
    // trace point has split. Recovering such an edge *creates* it, which spans the split point and
    // puts a T-junction back into a cell that had none - 8 of them on a6a, and 33 hanging nodes
    // behind them (PLAN §6.44). Splitting first is a pure function of the facet and the points, so
    // it costs nothing and cannot disagree between cells.
    let facets: Vec<Vec<u32>> = facets
        .iter()
        .map(|facet| {
            if facet.len() < 3 {
                return facet.clone();
            }
            let mut out: Vec<u32> = Vec::with_capacity(facet.len());
            for slot in 0..facet.len() {
                let (a, b) = (facet[slot], facet[(slot + 1) % facet.len()]);
                out.push(a);
                let (p, q) = (points[a as usize], points[b as usize]);
                let along = q.sub(p);
                let len2 = along.dot(along);
                if len2 <= 0.0 {
                    continue;
                }
                let mut between: Vec<(f64, u32)> = (0..points.len() as u32)
                    .filter(|id| *id != a && *id != b && !facet.contains(id))
                    .filter_map(|id| {
                        let rel = points[id as usize].sub(p);
                        let t = rel.dot(along) / len2;
                        let off = rel.sub(along.scale(t));
                        (off.dot(off) <= 1.0e-18 * len2 && (1.0e-9..=1.0 - 1.0e-9).contains(&t))
                            .then_some((t, id))
                    })
                    .collect();
                between.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
                out.extend(between.into_iter().map(|(_, id)| id));
            }
            out
        })
        .collect();
    let facets = &facets[..];
    let frozen: std::collections::BTreeSet<[u32; 3]> = boundary
        .iter()
        .map(|t| {
            let mut k = *t;
            k.sort_unstable();
            k
        })
        .collect();
    // Faces carried by exactly one tet are the outer boundary; by two, interior.
    let faces_of = |tets: &[[u32; 4]]| -> BTreeMap<[u32; 3], usize> {
        let mut carried: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for tet in tets {
            for face in tet_faces(*tet) {
                let mut key = face;
                key.sort_unstable();
                *carried.entry(key).or_insert(0) += 1;
            }
        }
        carried
    };
    let hull_of = |carried: &BTreeMap<[u32; 3], usize>| -> std::collections::BTreeSet<[u32; 3]> {
        carried
            .iter()
            .filter(|(_, count)| **count == 1)
            .map(|(face, _)| *face)
            .collect()
    };
    let mut carried = faces_of(&tets);
    let mut outer = hull_of(&carried);
    let edges_of = |faces: &std::collections::BTreeSet<[u32; 3]>| {
        let mut out: std::collections::BTreeSet<[u32; 2]> = std::collections::BTreeSet::new();
        for face in faces {
            for slot in 0..3 {
                let (x, y) = (face[slot], face[(slot + 1) % 3]);
                out.insert(if x <= y { [x, y] } else { [y, x] });
            }
        }
        out
    };
    // **The Delaunay hull is not the prescribed boundary, and that is expected.** Both triangulate
    // the same convex surface on the same nodes; they disagree wherever a constraint edge on a face
    // is not the Delaunay diagonal. Recovering it is a flip problem, not a reason to give the cell
    // to the fan - and the fan is what carries every remaining conformity risk (PLAN §6.36).
    let mut tets = tets;
    let mut recovery_failed: Option<&'static str> = None;
    if outer != frozen {
        match recover_boundary(&tets, points, &frozen) {
            Ok(recovered) => {
                tets = recovered;
                carried = faces_of(&tets);
                outer = hull_of(&carried);
            }
            // Kept and reported instead of collapsed into one refusal: the three ways recovery can
            // stop want three different follow-ups, and a single name would hide which is the
            // population.
            Err(reason) => recovery_failed = Some(reason),
        }
    }
    // **Then the facet's edges, which is the first half of facet recovery.** A facet cannot be a
    // union of mesh faces until its own edges are edges of the mesh; that is 52 of a6a's declines
    // and 853 of a3's, against 216 and 602 where the edges are all present and only the interior is
    // uncovered (PLAN §6.44). The two halves need different machinery, and this is the one edge
    // removal already does. Attempted only when something is actually missing, so the common case
    // pays a set lookup.
    {
        let protected = edges_of(&outer);
        let missing = facets.iter().any(|facet| {
            facet.len() >= 3
                && (0..facet.len()).any(|slot| {
                    let (a, b) = (facet[slot], facet[(slot + 1) % facet.len()]);
                    a != b && !tets.iter().any(|t| t.contains(&a) && t.contains(&b))
                })
        });
        if missing {
            if let Some(recovered) = recover_facet_edges(&tets, points, facets, &protected) {
                tets = recovered;
                carried = faces_of(&tets);
                outer = hull_of(&carried);
            }
        }
    }
    // **And then the facet's INTERIOR, which is the other half.** Its edges being mesh edges does
    // not make a facet a union of mesh faces; something can still cross the middle of it, and on
    // A-3 that residue strands more interface area than any other refusal. Every facet is offered,
    // in order; a facet nothing crosses costs one pass over the edges and returns immediately, and
    // a recovery that cannot finish leaves the tets it was given rather than a half-recovered
    // complex. Protection is recomputed here because the edge recovery above may have moved the
    // hull.
    {
        let mut protected = edges_of(&outer);
        for facet in facets {
            for slot in 0..facet.len() {
                let (a, b) = (facet[slot], facet[(slot + 1) % facet.len()]);
                protected.insert(if a <= b { [a, b] } else { [b, a] });
            }
        }
        for facet in facets {
            if facet.len() < 3 || facet_crossing_edges(&tets, points, facet).is_empty() {
                continue;
            }
            if let Some(recovered) = recover_facet_interior(&tets, points, facet, &protected) {
                tets = recovered;
                carried = faces_of(&tets);
                outer = hull_of(&carried);
            }
        }
    }
    // **Then the slivers, which are the third goal property.** Nothing above this line looks at
    // element shape at all, and the measurement says it should: §5.2's table puts 0.02 % of a1's
    // tets below 10° and §7.4 puts 19.6 %. The pass only flips, so it costs no elements, and it is
    // guarded rather than trusted - if it moved the hull the cell keeps the tets it had, and the
    // facet checks below still run on whatever it produced.
    {
        let improved = improve_dihedral(&tets, points, facets, 24);
        let after = hull_of(&faces_of(&improved));
        if after == outer {
            tets = improved;
            carried = faces_of(&tets);
        }
    }
    // **Both checks always run, and the reason names the combination.** Returning on the first
    // failure would have made the second one unmeasurable: on a8 only 24 of 4,377 cells reach the
    // facet test if the boundary test can return early, so "facet recovery is never needed" would
    // have been a statement about 24 cells dressed up as one about the population.
    let boundary_ok = outer == frozen;
    // **Which KIND of mismatch, because they need different cures.** The cell's region is a convex
    // tet, so `delaunay_tets` puts on its hull exactly the points that lie on the tet's surface. If
    // every node the frozen boundary uses is still a hull node, the two boundaries cover the same
    // surface and differ only in how they split it - a combinatorial difference, and the constraint
    // edge that forces it is recoverable by flips. If a node is MISSING from the hull, the point
    // rounded to the inside of its face plane and `delaunay_tets` decided that with an exact
    // predicate, which has no rounding level to hide in; no amount of flipping puts an interior
    // point back on the hull. Naming the two separately is what stops the second being attacked
    // with the first's tools.
    let mut hull_nodes: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for face in &outer {
        hull_nodes.extend(face.iter().copied());
    }
    let structural = frozen
        .iter()
        .flat_map(|face| face.iter())
        .any(|node| !hull_nodes.contains(node));
    // **And the mismatch in the other direction, which had never been asked.** A node the HULL
    // carries and the frozen boundary does not is just as unflippable: the cell's outer surface has
    // a vertex on it that its neighbours have never heard of, and no flip or shave removes a point
    // from the convex hull of the points it was given. Those points come from the surface fragment,
    // whose rim is clipped to the tet and therefore lands ON the cell's faces - the same curve the
    // face's trace already put there, computed a second way. Naming it separately is what
    // distinguishes "the two boundaries split one surface differently" from "they are not the same
    // point set at all".
    let mut frozen_nodes: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for face in &frozen {
        frozen_nodes.extend(face.iter().copied());
    }
    let intruding = hull_nodes.iter().filter(|node| !frozen_nodes.contains(node)).count();

    let mut facets_ok = true;
    let mut facet_edges_missing = 0usize;
    let mut facet_interior_uncovered = 0usize;
    for facet in facets {
        if facet.len() < 3 {
            return Err("a facet has fewer than three vertices");
        }
        let corner = |slot: usize| points[facet[slot] as usize];
        let mut want = Vec3::new(0.0, 0.0, 0.0);
        for slot in 1..facet.len() - 1 {
            want = want.add(corner(slot).sub(corner(0)).cross(corner(slot + 1).sub(corner(0))));
        }
        let want_area = want.dot(want).sqrt() * 0.5;
        if want_area <= 0.0 {
            return Err("a facet has no area");
        }
        let normal = want.scale(1.0 / (want.dot(want).sqrt()));
        let offset = normal.dot(corner(0));
        // Every face of the mesh lying in the facet's plane and inside its outline. Summed by area,
        // because a facet the mesh cuts through is short and one it covers twice is long, and only
        // an area test sees both.
        let mut covered = 0.0;
        for face in carried.keys() {
            let p = [
                points[face[0] as usize],
                points[face[1] as usize],
                points[face[2] as usize],
            ];
            if p.iter().any(|q| (normal.dot(*q) - offset).abs() > tol) {
                continue;
            }
            let centre = p[0].add(p[1]).add(p[2]).scale(1.0 / 3.0);
            if !inside_polygon(centre, facet, points, normal, tol) {
                continue;
            }
            let area2 = p[1].sub(p[0]).cross(p[2].sub(p[0]));
            covered += area2.dot(area2).sqrt() * 0.5;
        }
        if (covered - want_area).abs() > want_area * 1.0e-9 {
            facets_ok = false;
            // **Which half of facet recovery is missing.** A constraint facet is recovered in two
            // stages: its EDGES must be edges of the tetrahedralisation, and then its interior must
            // be covered by faces. They need different machinery - edges are recovered by removing
            // what crosses them, interiors by flipping or by a Steiner point - so a single refusal
            // naming both is a refusal that cannot be acted on. Counted here, and reported as two
            // reasons below.
            let mut mesh_edges: std::collections::BTreeSet<[u32; 2]> =
                std::collections::BTreeSet::new();
            for tet in &tets {
                for pair in [[0usize, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]] {
                    let (x, y) = (tet[pair[0]], tet[pair[1]]);
                    mesh_edges.insert(if x <= y { [x, y] } else { [y, x] });
                }
            }
            let missing = (0..facet.len())
                .filter(|slot| {
                    let (x, y) = (facet[*slot], facet[(slot + 1) % facet.len()]);
                    let key = if x <= y { [x, y] } else { [y, x] };
                    !mesh_edges.contains(&key)
                })
                .count();
            if missing > 0 {
                facet_edges_missing += missing;
            } else {
                facet_interior_uncovered += 1;
            }
        }
    }
    // **A tet whose volume cannot be computed in double is not an element.** `orient3d_filtered`
    // reports whether its own static error bound held; when it did not, the floating-point
    // determinant is inside the noise and only the exact fallback knows the sign. Those are exactly
    // the tets `[V1]` reads as a signed volume of +-0 - it computes in double, as any solver
    // assembling a Jacobian would. This is the predicate's own bound rather than a quality
    // threshold: there is no number here to tune.
    //
    // §7.4 declines the cell rather than emitting one, which sends it to the fan - a conforming
    // refusal, and the trade this project's own rule asks for. Three steps in a row bought
    // acceptance and paid for it in `[V1]` (3 -> 7 -> 19 -> 20), each recorded as "the same trade as
    // before" and never re-examined; this is where that stops (PLAN §6.45).
    // Thin tets are REMOVED before they are counted, and the cell is declined only for those that
    // survive. Protected: the boundary's edges and every facet's edges, so removal cannot undo
    // either recovery.
    {
        let mut protected = edges_of(&outer);
        for facet in facets {
            for slot in 0..facet.len() {
                let (a, b) = (facet[slot], facet[(slot + 1) % facet.len()]);
                protected.insert(if a <= b { [a, b] } else { [b, a] });
            }
        }
        tets = remove_thin_tets(&tets, points, tol, &protected);
        carried = faces_of(&tets);
        outer = hull_of(&carried);
    }
    let unmeasurable = tets.iter().any(|t| {
        let p = [
            points[t[0] as usize],
            points[t[1] as usize],
            points[t[2] as usize],
            points[t[3] as usize],
        ];
        // **Thinner than the node quantum is not an element.** Measured at emit time with the exact
        // predicate, §7.4 produces NO inverted tet - and `[V1]` finds sixteen in the written file,
        // so they are tipped over downstream rather than born that way. The bound is therefore not
        // about computing the volume here but about surviving a later nudge: a tet's volume changes
        // by about its face area times any displacement of a vertex, so one whose volume is below
        // `longest² × tol` has a height under `tol` and its orientation is at the mercy of whatever
        // moves a node next. `tol` is the cell's own relative tolerance, already a parameter - this
        // adds no number of its own.
        let volume = crate::meshgen::predicates::tet_signed_volume(p[0], p[1], p[2], p[3]).abs();
        let mut longest = 0.0f64;
        for a in 0..4 {
            for b in a + 1..4 {
                let d = p[b].sub(p[a]);
                longest = longest.max(d.dot(d).sqrt());
            }
        }
        volume <= longest * longest * tol
    });
    match (boundary_ok, facets_ok, structural || intruding > 0) {
        (true, true, _) if unmeasurable => {
            Err("a tet is thinner than the node quantum")
        }
        (true, true, _) => Ok(tets),
        (false, true, false) => Err(recovery_failed
            .unwrap_or("the boundary is split differently, but on the same nodes")),
        (false, true, true) => Err(if structural {
            "the boundary uses a node that is not on the hull"
        } else {
            "the hull carries a node the boundary has never heard of"
        }),
        (true, false, _) => Err(if facet_edges_missing > 0 {
            "a facet edge is not an edge of the tetrahedralisation"
        } else {
            "a facet's edges are all there but its interior is not covered"
        }),
        (false, false, false) => Err("neither the boundary nor the facets survive"),
        (false, false, true) => Err("a boundary node is off the hull, and the facets fail too"),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Group a cell's tets into `SPEC_meshgen_geometry.md` §7.5's sub-regions — the connected
//   components of the tets NOT separated by a constraint face.
// Inputs: the tets, the constraint facets as index polygons, the point table and a tolerance.
// Returns: a region id per tet, numbered by the order the regions are first met.
// Side effects: None.
// Notes: **This is where the over-cut stops mattering, and it is worth stating plainly.** A facet
//   that does not separate the cell - a strut face ending at a rim inside it - leaves ONE region,
//   because material flows around the rim and the flood fill goes with it. The plane-driven route
//   could not express that at all: an infinite plane always separates, so a rim became a spurious
//   material boundary and the cell was refused rather than meshed (PLAN §6.26). Here the same
//   geometry simply produces one region and one label.
//
//   A face is a constraint exactly when it lies in a facet's plane AND its centroid is inside that
//   facet's outline. The outline test is what distinguishes a face on the facet from a face merely
//   coplanar with it somewhere else - which is the same distinction `fragment_in_cell` makes for
//   triangles, for the same reason.
pub fn regions_by_constraint(
    tets: &[[u32; 4]],
    facets: &[Vec<u32>],
    points: &[Vec3],
    tol: f64,
) -> Vec<u32> {
    // Each facet's plane once, so the per-face test is a handful of dot products.
    let planes: Vec<(Vec3, f64, &Vec<u32>)> = facets
        .iter()
        .filter_map(|facet| {
            if facet.len() < 3 {
                return None;
            }
            let corner = |slot: usize| points[facet[slot] as usize];
            let mut normal = Vec3::new(0.0, 0.0, 0.0);
            for slot in 1..facet.len() - 1 {
                normal =
                    normal.add(corner(slot).sub(corner(0)).cross(corner(slot + 1).sub(corner(0))));
            }
            let length = normal.dot(normal).sqrt();
            if length <= 0.0 {
                return None;
            }
            let normal = normal.scale(1.0 / length);
            Some((normal, normal.dot(corner(0)), facet))
        })
        .collect();
    let blocked = |face: [u32; 3]| -> bool {
        let p = [
            points[face[0] as usize],
            points[face[1] as usize],
            points[face[2] as usize],
        ];
        let centre = p[0].add(p[1]).add(p[2]).scale(1.0 / 3.0);
        planes.iter().any(|(normal, offset, facet)| {
            p.iter().all(|q| (normal.dot(*q) - offset).abs() <= tol)
                && inside_polygon(centre, facet, points, *normal, tol)
        })
    };

    let mut carried: BTreeMap<[u32; 3], Vec<usize>> = BTreeMap::new();
    for (at, tet) in tets.iter().enumerate() {
        for face in tet_faces(*tet) {
            let mut key = face;
            key.sort_unstable();
            carried.entry(key).or_default().push(at);
        }
    }

    let mut region: Vec<u32> = vec![u32::MAX; tets.len()];
    let mut next = 0u32;
    for seed in 0..tets.len() {
        if region[seed] != u32::MAX {
            continue;
        }
        let mut frontier = vec![seed];
        while let Some(at) = frontier.pop() {
            if region[at] != u32::MAX {
                continue;
            }
            region[at] = next;
            for face in tet_faces(tets[at]) {
                let mut key = face;
                key.sort_unstable();
                if blocked(key) {
                    continue;
                }
                for other in carried.get(&key).into_iter().flatten() {
                    if region[*other] == u32::MAX {
                        frontier.push(*other);
                    }
                }
            }
        }
        next += 1;
    }
    region
}

// AI-FUNC-SUMMARY:
// Purpose: Whether a coplanar point lies inside a convex polygon.
// Inputs: the point, the polygon's indices, the point table, the polygon's unit normal, and a
//   tolerance.
// Returns: bool.
// Side effects: None.
// Notes: Convex by construction - the facets are triangles clipped to a tet - so the sign of the
//   cross product against the normal is the whole test, and a point on an edge counts as inside so
//   that two faces meeting along one are both credited.
fn inside_polygon(point: Vec3, facet: &[u32], points: &[Vec3], normal: Vec3, tol: f64) -> bool {
    for slot in 0..facet.len() {
        let a = points[facet[slot] as usize];
        let b = points[facet[(slot + 1) % facet.len()] as usize];
        if b.sub(a).cross(point.sub(a)).dot(normal) < -tol {
            return false;
        }
    }
    true
}

// AI-FUNC-SUMMARY: The smallest vertex id of a tet; returns u32; side effects: none.
fn tet_min(t: [u32; 4]) -> u32 {
    t.iter().copied().min().unwrap_or(t[0])
}

// AI-FUNC-SUMMARY: The four faces of a tet, each wound outward; returns them; side effects: none.
fn tet_faces(t: [u32; 4]) -> [[u32; 3]; 4] {
    [
        [t[1], t[2], t[3]],
        [t[0], t[3], t[2]],
        [t[0], t[1], t[3]],
        [t[0], t[2], t[1]],
    ]
}

// AI-FUNC-SUMMARY: Whether two tets share three vertices; returns bool; side effects: none.
fn share_a_face(a: [u32; 4], b: [u32; 4]) -> bool {
    a.iter().filter(|id| b.contains(id)).count() == 3
}

// AI-FUNC-SUMMARY:
// Purpose: The surface fragment inside one lattice cell — the last of §7.2's three stated inputs
//   that the pipeline has never produced.
// Inputs: the cell's four corners, the candidate triangles, and a tolerance.
// Returns: those candidates whose intersection with the tet has positive area, in the order given.
// Side effects: None.
// Notes: **Measure-zero contact is not fragment, and excluding it is geometry rather than tuning.**
//   A triangle that meets the tet only along an edge or at a corner has no material inside it, and
//   `planes_from_fragment` would still hand back its supporting plane — which cuts the whole cell,
//   because a supporting plane is infinite while the triangle is not. One tangent triangle can
//   therefore double the piece count for nothing. The fragment is the part of the surface *in* the
//   cell, so a contact of zero area is not part of it.
//
//   The originals come back rather than the clipped polygons: the only consumer is
//   `planes_from_fragment`, a clipped piece has the same supporting plane as the triangle it came
//   from, and a sliver of a clipped piece has a numerically worse normal than the whole triangle
//   does. Input order is preserved and the caller's input order is fixed, which is what R-P2 needs.
pub fn fragment_in_cell(tet: [Vec3; 4], tris: &[[Vec3; 3]], tol: f64) -> Vec<[Vec3; 3]> {
    let Some(halfspaces) = tet_halfspaces(tet) else {
        return Vec::new();
    };
    tris.iter()
        .filter(|triangle| clip_to_halfspaces(triangle, &halfspaces, tol).is_some())
        .copied()
        .collect()
}

// AI-FUNC-SUMMARY:
// Purpose: The surface fragment inside one cell as BOUNDED facets — the clipped polygons
//   themselves, which is what `SPEC_meshgen_geometry.md` §7.2 means by "clipped surface fragments"
//   and what a constrained tetrahedralisation takes as its constraints.
// Inputs: the cell's four corners, the candidate triangles, and a tolerance.
// Returns: one convex polygon per triangle that meets the cell, in the order given.
// Side effects: None.
// Notes: **This is what makes the over-cut impossible rather than merely smaller** (PLAN §6.26). A
//   supporting plane is infinite, so cutting by one crosses the cell's edges where no surface is;
//   a clipped facet stops where the surface stops, so a strut whose face ends inside the cell
//   constrains only the part of the cell it actually passes through.
//
//   **Every vertex on the cell's boundary is shared with the neighbour by construction.** A facet
//   vertex is either a vertex of the original triangle - the same point in both cells, since both
//   read the same arranged surface - or the crossing of one of its edges with a face plane, which
//   is a function of that edge and that plane and of nothing cell-local. This is the property the
//   plane-driven route could not have: there the cut point depended on the cell's own plane SET,
//   which the neighbour does not share. Clipping a triangle by four half-spaces always yields a
//   convex polygon, so the facet needs no triangulation to be well defined.
pub fn fragment_facets_in_cell(
    tet: [Vec3; 4],
    tris: &[[Vec3; 3]],
    tol: f64,
) -> Vec<Vec<Vec3>> {
    let Some(halfspaces) = tet_halfspaces(tet) else {
        return Vec::new();
    };
    tris.iter()
        .filter_map(|triangle| clip_to_halfspaces(triangle, &halfspaces, tol))
        .collect()
}

// AI-FUNC-SUMMARY:
// Purpose: A tet's four faces as inward half-spaces.
// Inputs: the four corners.
// Returns: `(inward unit normal, offset)` per face, or None when the tet is degenerate.
// Side effects: None.
// Notes: Built from the cell's own corners, so a degenerate or inverted tet keeps nothing rather
//   than keeping everything - the failure that turns a clip into a no-op nobody notices.
pub fn tet_halfspaces_of(tet: [Vec3; 4]) -> Option<Vec<(Vec3, f64)>> {
    tet_halfspaces(tet)
}

fn tet_halfspaces(tet: [Vec3; 4]) -> Option<Vec<(Vec3, f64)>> {
    let mut out: Vec<(Vec3, f64)> = Vec::with_capacity(4);
    for skip in 0..4 {
        let face: Vec<Vec3> = (0..4).filter(|s| *s != skip).map(|s| tet[s]).collect();
        let normal = face[1].sub(face[0]).cross(face[2].sub(face[0]));
        let length = normal.dot(normal).sqrt();
        if length <= 0.0 {
            return None;
        }
        let normal = normal.scale(1.0 / length);
        // Point it at the corner that is not on this face, so "inside" is positive.
        let inward = if normal.dot(tet[skip].sub(face[0])) < 0.0 {
            normal.scale(-1.0)
        } else {
            normal
        };
        out.push((inward, inward.dot(face[0])));
    }
    Some(out)
}

// AI-FUNC-SUMMARY:
// Purpose: Clip one triangle to a set of half-spaces, keeping it only if what survives has area.
// Inputs: the triangle, the half-spaces, and a length tolerance.
// Returns: the clipped convex polygon, or None when the intersection has no area.
// Side effects: None.
// Notes: **Measure-zero contact is not fragment.** A triangle meeting the cell at a corner or along
//   an edge has no material inside it, and admitting it would hand the caller a degenerate facet -
//   and, on the plane-driven path, an infinite supporting plane that cuts the whole cell for
//   nothing. `tol` is a length, so it is squared to compare against an area.
fn clip_to_halfspaces(
    triangle: &[Vec3; 3],
    halfspaces: &[(Vec3, f64)],
    tol: f64,
) -> Option<Vec<Vec3>> {
    let mut polygon: Vec<Vec3> = triangle.to_vec();
    for (normal, offset) in halfspaces {
        if polygon.is_empty() {
            return None;
        }
        let mut next: Vec<Vec3> = Vec::with_capacity(polygon.len() + 1);
        for slot in 0..polygon.len() {
            let (a, b) = (polygon[slot], polygon[(slot + 1) % polygon.len()]);
            let (da, db) = (normal.dot(a) - offset, normal.dot(b) - offset);
            if da >= 0.0 {
                next.push(a);
            }
            // A crossing, and only a strict one: an endpoint exactly on the plane is already
            // carried by the branch above and adding it twice makes a duplicate vertex.
            if (da > 0.0 && db < 0.0) || (da < 0.0 && db > 0.0) {
                let t = da / (da - db);
                next.push(a.add(b.sub(a).scale(t)));
            }
        }
        polygon = next;
    }
    if polygon.len() < 3 {
        return None;
    }
    let mut area2 = Vec3::new(0.0, 0.0, 0.0);
    for slot in 1..polygon.len() - 1 {
        area2 = area2.add(polygon[slot].sub(polygon[0]).cross(polygon[slot + 1].sub(polygon[0])));
    }
    (area2.dot(area2).sqrt() * 0.5 > tol * tol).then_some(polygon)
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

// AI-FUNC-SUMMARY:
// Purpose: Chain a face's trace segments into ordered closed loops.
// Inputs: the segments from `trace_on_face`, and the quantum that decides when two endpoints are
//   the same point.
// Returns: one ring per closed loop, each starting at its lowest-keyed point; None if any segment
//   fails to close into one.
// Side effects: None.
// Notes: `trace_on_face` returns one chord per surface triangle, in canonical order but otherwise
//   unrelated — `triangulate_with_hole` needs a ring. Endpoints are matched on the quantised key
//   rather than on exact equality because two adjacent surface triangles compute their shared
//   crossing from different arithmetic and agree only to within it; that is the same reason the
//   node arena quantises.
//
//   **Refusing to return an open chain is a correctness requirement, not tidiness.** A trace that
//   stays strictly inside a face is closed — it cannot reach the face's boundary, since reaching it
//   would mean crossing a lattice edge and the whole population is defined by not doing that
//   (PLAN §6.17). An open chain therefore means the trace is incomplete, and triangulating a hole
//   from it would put a spurious edge across the face. The caller must fall back, not patch it.
//
//   A face may carry several loops - two struts passing through one face - so this returns a Vec.
//   Each ring starts at its lowest-keyed point and runs in the direction of its lower-keyed
//   neighbour, so the ring is a function of the geometry and both cells derive it identically.
pub fn chain_trace(segments: &[[Vec3; 2]], quantum: f64) -> Option<Vec<Vec<Vec3>>> {
    if segments.is_empty() {
        return Some(Vec::new());
    }
    let q = quantum.max(f64::MIN_POSITIVE);
    let key = |p: Vec3| -> (i64, i64, i64) {
        (
            (p.x / q).round() as i64,
            (p.y / q).round() as i64,
            (p.z / q).round() as i64,
        )
    };
    let mut points: std::collections::BTreeMap<(i64, i64, i64), Vec3> =
        std::collections::BTreeMap::new();
    let mut links: std::collections::BTreeMap<(i64, i64, i64), Vec<(i64, i64, i64)>> =
        std::collections::BTreeMap::new();
    for segment in segments {
        let (a, b) = (key(segment[0]), key(segment[1]));
        if a == b {
            continue;
        }
        points.entry(a).or_insert(segment[0]);
        points.entry(b).or_insert(segment[1]);
        let ends = links.entry(a).or_default();
        if !ends.contains(&b) {
            ends.push(b);
        }
        let ends = links.entry(b).or_default();
        if !ends.contains(&a) {
            ends.push(a);
        }
    }
    if links.is_empty() {
        return Some(Vec::new());
    }
    // Every point on a closed loop has exactly two neighbours. Anything else is an open chain or a
    // junction, and neither can be triangulated as a hole.
    if links.values().any(|ends| ends.len() != 2) {
        return None;
    }
    let mut visited: std::collections::BTreeSet<(i64, i64, i64)> = std::collections::BTreeSet::new();
    let mut rings: Vec<Vec<Vec3>> = Vec::new();
    let starts: Vec<(i64, i64, i64)> = links.keys().copied().collect();
    for start in starts {
        if visited.contains(&start) {
            continue;
        }
        let mut ring: Vec<Vec3> = Vec::new();
        let mut current = start;
        // Of the two neighbours, take the lower-keyed one, so the direction is the geometry's.
        let mut previous: Option<(i64, i64, i64)> = None;
        loop {
            visited.insert(current);
            ring.push(points[&current]);
            let ends = &links[&current];
            let next = match previous {
                None => ends.iter().min().copied()?,
                Some(prev) => *ends.iter().find(|end| **end != prev)?,
            };
            if next == start {
                break;
            }
            if visited.contains(&next) {
                // Re-entered the loop somewhere other than its start: not a simple ring.
                return None;
            }
            previous = Some(current);
            current = next;
        }
        if ring.len() < 3 {
            return None;
        }
        rings.push(ring);
    }
    Some(rings)
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUANTUM: f64 = 1.0e-12;
    const TOL: f64 = 1.0e-12;

    // Helpers for the tetrahedralisation tests: keys at the module's usual quantum, and the volume
    // of a tet list read straight off the points.
    fn keys_of(points: &[Vec3]) -> Vec<NodeKey> {
        points.iter().map(|p| node_key(*p, QUANTUM)).collect()
    }

    fn tets_volume(tets: &[[u32; 4]], points: &[Vec3]) -> f64 {
        tets.iter()
            .map(|t| {
                let (a, b, c, d) = (
                    points[t[0] as usize],
                    points[t[1] as usize],
                    points[t[2] as usize],
                    points[t[3] as usize],
                );
                b.sub(a).cross(c.sub(a)).dot(d.sub(a)).abs() / 6.0
            })
            .sum()
    }

    // Nothing constrains the cell, so it is one region however many tets it happens to have.
    #[test]
    fn a_cell_with_no_constraint_is_one_region() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.2, 0.2, 0.2),
        ];
        let tets = delaunay_tets(&points, &keys_of(&points)).expect("tetrahedralisable");
        let regions = regions_by_constraint(&tets, &[], &points, 1.0e-9);
        assert!(regions.iter().all(|r| *r == 0), "one region, got {regions:?}");
    }

    // A facet that cuts right across the cell separates it into two, which is the ordinary case and
    // the one that produces a material boundary.
    #[test]
    fn a_facet_across_the_cell_gives_two_regions() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, 0.5),
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(0.0, 0.5, 0.5),
        ];
        let tets = delaunay_tets(&points, &keys_of(&points)).expect("tetrahedralisable");
        let facet = vec![vec![4u32, 5, 6]];
        let regions = regions_by_constraint(&tets, &facet, &points, 1.0e-9);
        let distinct: std::collections::BTreeSet<u32> = regions.iter().copied().collect();
        assert_eq!(distinct.len(), 2, "the facet separates the cell, got {regions:?}");
        // And the split is the geometry's: everything above z = 0.5 on one side of it.
        for (at, tet) in tets.iter().enumerate() {
            let above = tet
                .iter()
                .all(|id| points[*id as usize].z >= 0.5 - 1.0e-12);
            if above {
                assert_ne!(
                    regions[at], regions[0],
                    "a tet above the facet must not share the region below it"
                );
            }
        }
    }

    // **The case the plane-driven route could not express at all.** A facet that ENDS inside the
    // cell does not separate it: material flows around the rim, so the flood fill goes with it and
    // the cell is one region with one label. An infinite plane always separates, which is why the
    // same geometry was a spurious material boundary there and a refusal here (PLAN §6.26).
    #[test]
    fn a_facet_that_ends_inside_the_cell_leaves_one_region() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.30, 0.05, 0.05),
            Vec3::new(0.05, 0.30, 0.05),
            Vec3::new(0.05, 0.05, 0.30),
        ];
        let tets = delaunay_tets(&points, &keys_of(&points)).expect("tetrahedralisable");
        let facet = vec![vec![4u32, 5, 6]];
        // The facet really is a face of this mesh - otherwise the test would pass vacuously.
        assert!(
            tets.iter().any(|t| tet_faces(*t).iter().any(|f| {
                let mut k = *f;
                k.sort_unstable();
                k == [4, 5, 6]
            })),
            "the facet must be present for this test to mean anything"
        );
        let regions = regions_by_constraint(&tets, &facet, &points, 1.0e-9);
        let distinct: std::collections::BTreeSet<u32> = regions.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            1,
            "a facet with a free rim must not separate the cell, got {regions:?}"
        );
    }

    // A bare face is one triangle, and that is the case the cut takes 250,258 times on a8 - so it
    // must not gain a vertex, a diagonal, or an opinion.
    #[test]
    fn a_face_with_no_constraints_is_itself() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let points = face.to_vec();
        let tris = constrained_face_triangulation(face, &points, &keys_of(&points), &[], 0.0)
            .expect("a bare face triangulates");
        assert_eq!(tris.len(), 1);
    }

    // **The property the whole face side exists for.** A chord across the face must come back as an
    // EDGE of the triangulation - not merely covered by it - because that is what lets the cell's
    // constrained tetrahedralisation stop on the surface instead of straddling it.
    #[test]
    fn a_chord_across_the_face_comes_back_as_an_edge() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let mut points = face.to_vec();
        points.push(Vec3::new(0.5, 0.0, 0.0));
        points.push(Vec3::new(0.0, 0.5, 0.0));
        let tris = constrained_face_triangulation(
            face,
            &points,
            &keys_of(&points),
            &[[3, 4]],
            0.0,
        )
        .expect("a chord is recoverable");
        assert!(
            tris.iter().any(|t| {
                let mut on = t.iter().filter(|id| **id == 3 || **id == 4).count();
                on = on.min(2);
                on == 2
            }),
            "the chord (3, 4) must be an edge of some triangle"
        );
        // And the triangulation still covers the face exactly - no gap, no overlap.
        let area: f64 = tris
            .iter()
            .map(|t| {
                let (a, b, c) = (points[t[0] as usize], points[t[1] as usize], points[t[2] as usize]);
                let n = b.sub(a).cross(c.sub(a));
                n.dot(n).sqrt() * 0.5
            })
            .sum();
        assert!((area - 0.5).abs() < 1.0e-12, "the face's area is 0.5, got {area}");
    }

    // The interior loop, which `triangulate_with_hole` handled as a special case and this handles
    // as an ordinary one: a closed ring of constraint segments strictly inside the face. Every ring
    // edge must survive, and the face must still be covered exactly - the ring is a constraint, not
    // a hole to be removed.
    #[test]
    fn an_interior_loop_survives_as_edges_of_the_face() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(0.0, 4.0, 0.0),
        ];
        let mut points = face.to_vec();
        for corner in [
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(2.0, 1.0, 0.0),
            Vec3::new(1.0, 2.0, 0.0),
        ] {
            points.push(corner);
        }
        let segments = [[3u32, 4], [4, 5], [5, 3]];
        let tris = constrained_face_triangulation(face, &points, &keys_of(&points), &segments, 0.0)
            .expect("an interior loop is recoverable");
        for segment in &segments {
            assert!(
                tris.iter().any(|t| {
                    tri_edges(*t)
                        .iter()
                        .any(|e| sorted_edge(*e) == sorted_edge(*segment))
                }),
                "loop edge {segment:?} must survive"
            );
        }
        let area: f64 = tris
            .iter()
            .map(|t| {
                let (a, b, c) = (points[t[0] as usize], points[t[1] as usize], points[t[2] as usize]);
                let n = b.sub(a).cross(c.sub(a));
                n.dot(n).sqrt() * 0.5
            })
            .sum();
        assert!((area - 8.0).abs() < 1.0e-12, "the face's area is 8, got {area}");
    }

    // **Invariant J1, as a test.** The two cells sharing a face pass their own point lists in their
    // own orders; the triangulation they get must be the same one, compared by COORDINATES since
    // the indices are what differ. Four earlier attempts in this project failed by computing the
    // equivalent quantity per cell (PLAN §6.23), and this is the property they were missing.
    #[test]
    fn both_cells_derive_the_same_face_triangulation() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(0.0, 4.0, 0.0),
        ];
        let base = vec![
            face[0],
            face[1],
            face[2],
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ];
        let geometric = |tris: &[[u32; 3]], points: &[Vec3]| -> Vec<[NodeKey; 3]> {
            let mut out: Vec<[NodeKey; 3]> = tris
                .iter()
                .map(|t| {
                    let mut k = [
                        node_key(points[t[0] as usize], QUANTUM),
                        node_key(points[t[1] as usize], QUANTUM),
                        node_key(points[t[2] as usize], QUANTUM),
                    ];
                    k.sort_unstable();
                    k
                })
                .collect();
            out.sort_unstable();
            out
        };
        let reference = geometric(
            &constrained_face_triangulation(face, &base, &keys_of(&base), &[[3, 5], [5, 4]], 0.0)
                .expect("triangulable"),
            &base,
        );
        // The neighbour presents the same face with its points in a different order, and names the
        // same two segments by its own indices.
        for shift in 1..base.len() {
            let shuffled: Vec<Vec3> =
                (0..base.len()).map(|slot| base[(slot + shift) % base.len()]).collect();
            let find = |p: Vec3| {
                shuffled
                    .iter()
                    .position(|q| q.sub(p).dot(q.sub(p)) < 1.0e-24)
                    .expect("the same points") as u32
            };
            let segments = [[find(base[3]), find(base[5])], [find(base[5]), find(base[4])]];
            let tris =
                constrained_face_triangulation(face, &shuffled, &keys_of(&shuffled), &segments, 0.0)
                    .expect("triangulable");
            assert_eq!(
                geometric(&tris, &shuffled),
                reference,
                "the neighbour's ordering (shift {shift}) gave a different face"
            );
        }
    }

    // A point outside the face is refused rather than triangulated: the result fills the convex
    // hull of the points, and a point beyond the face would put area in it that the face does not
    // have - which the cell either side would then disagree about.
    #[test]
    fn a_point_outside_the_face_is_refused() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let mut points = face.to_vec();
        points.push(Vec3::new(1.0, 1.0, 0.0));
        assert_eq!(
            constrained_face_triangulation(face, &points, &keys_of(&points), &[], 0.0).err(),
            Some("a point lies outside the face")
        );
    }

    // **Two points at the same place on one edge become a zero-area triangle, and the caller's
    // obligation is to not present them.** Distinct keys mean distinct points by this function's
    // contract, so points a few times 1e-8 apart on a straight edge are legal input and get
    // triangulated - into a sliver whose three vertices are collinear. That sliver is the whole
    // of a6a's residual: it lies on the EDGE, so both faces sharing the edge emit the same one,
    // the cell's boundary lists a triangle twice, and the fan cones it into two tets carrying one
    // face - 18 faces carried by four tets, plus the holes beside them (PLAN §6.35). The cure is
    // upstream, where a trace point coincident with an existing node is interned AS that node;
    // this pins why that is required rather than merely tidy.
    #[test]
    fn coincident_points_on_an_edge_triangulate_to_a_sliver() {
        let face = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let mut points = face.to_vec();
        // Three all-but-identical points partway along the edge from (0,0) to (1,0).
        points.push(Vec3::new(0.5, 0.0, 0.0));
        points.push(Vec3::new(0.5 + 3.0e-8, 0.0, 0.0));
        points.push(Vec3::new(0.5 + 5.0e-8, 3.0e-9, 0.0));
        let tris = constrained_face_triangulation(face, &points, &keys_of(&points), &[], 0.0)
            .expect("near-coincident points are legal input and are triangulated");
        let area = |t: &[u32; 3]| -> f64 {
            let (a, b, c) = (
                points[t[0] as usize],
                points[t[1] as usize],
                points[t[2] as usize],
            );
            b.sub(a).cross(c.sub(a)).dot(Vec3::new(0.0, 0.0, 1.0)).abs() / 2.0
        };
        let slivers = tris.iter().filter(|t| area(t) < 1.0e-12).count();
        assert!(
            slivers > 0,
            "the coincident points must produce a degenerate triangle - if this ever stops being \
             true the upstream snap is no longer load-bearing and this test should say so"
        );
        // And it is a sliver ON THE EDGE, which is what makes it appear in both owners' boundaries.
        assert!(
            tris.iter().any(|t| {
                area(t) < 1.0e-12
                    && t.iter().all(|id| points[*id as usize].y.abs() <= 1.0e-8)
            }),
            "the degenerate triangle lies along the face edge, so both faces sharing it emit it"
        );
    }

    // **A triangulation must cover its face.** Taken verbatim from a3, where 220 of 24,021
    // augmented faces came back covering less than the face they were given - here a pentagon
    // instead of the triangle, missing a sliver strip worth 1.7e-5 of the area along one edge.
    // Points 3, 4 and 5 all lie strictly inside the face (barycentric 1.7e-5, 5.6e-6 and 8.7e-16
    // against the edge they crowd), so the convex hull of the input IS the face and there is no
    // question of what the answer should be.
    //
    // The consequence is not a small area error. A face that covers less than itself hands both of
    // its owners a boundary whose edges do not pair, the fan cones the hole, and a3 read 1,258
    // boundary leaks - every one of them charged to the fanned arm (PLAN §6.39).
    #[test]
    fn a_face_crowded_along_one_edge_is_still_covered() {
        let points = vec![
            Vec3::new(0.279654036638725, 0.18042195912175807, 0.18042195912175807),
            Vec3::new(0.2886751345948129, 0.18944305707784598, 0.18042195912175807),
            Vec3::new(0.2886762250124125, 0.18944305707784598, 0.18944305707784598),
            Vec3::new(0.2886756228037549, 0.18944290329192393, 0.1857332257025385),
            Vec3::new(0.288675909965015, 0.18944300690253296, 0.1872517525529728),
            Vec3::new(0.2886757765896766, 0.18944305707784598, 0.1857332256994687),
        ];
        let face = [points[0], points[1], points[2]];
        let segments = [[3u32, 5], [3, 4], [4, 2]];
        let tris = constrained_face_triangulation(face, &points, &keys_of(&points), &segments, 0.0)
            .expect("the face triangulates");
        let area = |a: Vec3, b: Vec3, c: Vec3| {
            let n = b.sub(a).cross(c.sub(a));
            n.dot(n).sqrt() / 2.0
        };
        let whole = area(face[0], face[1], face[2]);
        let covered: f64 = tris
            .iter()
            .map(|t| {
                area(
                    points[t[0] as usize],
                    points[t[1] as usize],
                    points[t[2] as usize],
                )
            })
            .sum();
        assert!(
            (covered - whole).abs() <= whole * 1.0e-12,
            "the triangles must cover the face: {covered:e} of {whole:e}, short by {:.3e} of it",
            (whole - covered) / whole
        );
        // And covering it is not enough - it has to be a triangulation. Every edge is shared by two
        // triangles unless it is on the face's rim, where it is shared by one. A face that fails
        // this hands both its owners a boundary whose edges do not pair.
        let mut edges: BTreeMap<[u32; 2], usize> = BTreeMap::new();
        for t in &tris {
            for slot in 0..3 {
                let (x, y) = (t[slot], t[(slot + 1) % 3]);
                *edges.entry(if x <= y { [x, y] } else { [y, x] }).or_insert(0) += 1;
            }
        }
        for (edge, count) in &edges {
            let on_rim = (0..3).any(|slot| {
                let (p, q) = (face[slot], face[(slot + 1) % 3]);
                let along = q.sub(p);
                let len2 = along.dot(along);
                edge.iter().all(|node| {
                    let rel = points[*node as usize].sub(p);
                    let t = rel.dot(along) / len2;
                    let off = rel.sub(along.scale(t));
                    off.dot(off) <= 1.0e-18 * len2 && (-1.0e-9..=1.0 + 1.0e-9).contains(&t)
                })
            });
            assert_eq!(
                *count,
                usize::from(!on_rim) + 1,
                "edge {edge:?} (on_rim={on_rim}) is carried {count} time(s) in {tris:?}"
            );
        }
    }

    // The simplest cell there is: a tet with nothing crossing it. One element, and its four faces
    // are exactly the frozen boundary.
    #[test]
    fn a_cell_with_no_constraints_is_one_tet() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let boundary = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let tets = constrained_tets(&points, &keys_of(&points), &boundary, &[], 1.0e-9)
            .expect("a tet with no constraints is one tet");
        assert_eq!(tets.len(), 1);
    }

    // **The J1 check, which is the one that makes a cell's mesh usable by its neighbour.** The
    // frozen boundary here splits the base across its diagonal one way; a tetrahedralisation whose
    // own outer faces split it the other way covers the same region and is still a crack, because
    // the neighbour holds the first triangulation.
    //
    // Both diagonals are now ACCEPTED, and that is the point: the Delaunay will choose one of them
    // and boundary recovery flips it to whichever was asked for. The test therefore checks the
    // thing that matters - that the tetrahedralisation handed back really does carry the boundary
    // it was given - rather than that one of the two is refused.
    #[test]
    fn either_diagonal_of_a_frozen_base_is_recovered() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.5, 0.5, 1.0),
        ];
        // A pyramid, with its square base deliberately split along the WRONG diagonal for whatever
        // the Delaunay will choose - one of the two must disagree, and the test asserts the refusal
        // names the boundary either way by trying both.
        let one = [[0, 2, 1], [0, 3, 2], [0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]];
        let other = [[0, 3, 1], [1, 3, 2], [0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]];
        let keys = keys_of(&points);
        let first = constrained_tets(&points, &keys, &one, &[], 1.0e-9);
        let second = constrained_tets(&points, &keys, &other, &[], 1.0e-9);
        for (asked, got) in [(&one[..], &first), (&other[..], &second)] {
            let tets = got.as_ref().expect("both diagonals must be recovered");
            let mut carried: BTreeMap<[u32; 3], usize> = BTreeMap::new();
            for tet in tets {
                for face in tet_faces(*tet) {
                    let mut key = face;
                    key.sort_unstable();
                    *carried.entry(key).or_insert(0) += 1;
                }
            }
            let hull: std::collections::BTreeSet<[u32; 3]> = carried
                .iter()
                .filter(|(_, n)| **n == 1)
                .map(|(face, _)| *face)
                .collect();
            let want: std::collections::BTreeSet<[u32; 3]> = asked
                .iter()
                .map(|t| {
                    let mut k = *t;
                    k.sort_unstable();
                    k
                })
                .collect();
            assert_eq!(hull, want, "the mesh must carry the boundary it was given");
        }
    }

    // A facet the Delaunay already respects is accepted — and this is the case the whole approach
    // is betting on. A small triangle strictly inside a tet comes out as a face of the
    // tetrahedralisation without any recovery at all, which is why the verifying half is worth
    // landing before the recovering half is built.
    #[test]
    fn a_facet_the_delaunay_already_respects_is_accepted() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.30, 0.05, 0.05),
            Vec3::new(0.05, 0.30, 0.05),
            Vec3::new(0.05, 0.05, 0.30),
        ];
        let boundary = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let facet = vec![vec![4u32, 5, 6]];
        let tets = constrained_tets(&points, &keys_of(&points), &boundary, &facet, 1.0e-9)
            .expect("this facet is already a face of the Delaunay");
        // And it really is a face, not merely tolerated: some tet carries exactly those three.
        assert!(
            tets.iter().any(|t| {
                tet_faces(*t).iter().any(|f| {
                    let mut k = *f;
                    k.sort_unstable();
                    k == [4, 5, 6]
                })
            }),
            "the facet must appear as a face"
        );
    }

    // A facet the mesh CUTS THROUGH is RECOVERED. Two points straddling the facet's centre and
    // close enough to be joined by an edge put that edge across it, so no set of faces covers the
    // facet as the Delaunay leaves it - and interior recovery removes the crossing edge and makes
    // the facet a union of faces. Until that recovery existed this case was refused by name, and
    // the refusal is what this test used to assert; the case is now the one the second half of
    // facet recovery is for, so the test asserts the cure rather than the symptom.
    #[test]
    fn a_facet_an_edge_crosses_is_recovered() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.50, 0.05, 0.05),
            Vec3::new(0.05, 0.50, 0.05),
            Vec3::new(0.05, 0.05, 0.50),
            Vec3::new(0.19, 0.19, 0.19),
            Vec3::new(0.21, 0.21, 0.21),
        ];
        let boundary = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let facet = vec![vec![4u32, 5, 6]];
        let tets = constrained_tets(&points, &keys_of(&points), &boundary, &facet, 1.0e-9)
            .expect("interior recovery must remove the edge that crosses the facet");
        assert!(
            facet_crossing_edges(&tets, &points, &facet[0]).is_empty(),
            "nothing may cross the facet once it is recovered"
        );
        // The crossing edge itself is gone, which is the operation that did it.
        assert!(
            !tets.iter().any(|t| t.contains(&7) && t.contains(&8)),
            "the edge across the facet must have been removed"
        );
    }

    #[test]
    fn four_points_tetrahedralise_to_one_tet() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let tets = delaunay_tets(&points, &keys_of(&points)).expect("a tet is tetrahedralisable");
        assert_eq!(tets.len(), 1);
        assert!((tets_volume(&tets, &points) - 1.0 / 6.0).abs() < 1.0e-15);
    }

    // The honest test of a tetrahedralisation is that it fills the hull: too little is a gap, too
    // much is an overlap. A cube is the case worth using because its eight corners are COSPHERICAL,
    // which is where a naive in-sphere test and an arbitrary tie rule both fall over.
    #[test]
    fn the_cube_is_filled_exactly_despite_its_corners_being_cospherical() {
        let mut points = Vec::new();
        for x in [0.0, 1.0] {
            for y in [0.0, 1.0] {
                for z in [0.0, 1.0] {
                    points.push(Vec3::new(x, y, z));
                }
            }
        }
        let tets = delaunay_tets(&points, &keys_of(&points)).expect("a cube is tetrahedralisable");
        assert!(
            (tets_volume(&tets, &points) - 1.0).abs() < 1.0e-15,
            "the tets must fill the cube exactly, got {}",
            tets_volume(&tets, &points)
        );
    }

    // Every tet positively oriented, and no tet degenerate. A flat tet has no interior, contributes
    // no volume, and breaks every downstream orientation argument.
    #[test]
    fn every_tet_comes_back_positively_oriented() {
        let mut points = vec![Vec3::new(0.5, 0.5, 0.5)];
        for x in [0.0, 1.0] {
            for y in [0.0, 1.0] {
                for z in [0.0, 1.0] {
                    points.push(Vec3::new(x, y, z));
                }
            }
        }
        let tets = delaunay_tets(&points, &keys_of(&points)).expect("tetrahedralisable");
        for t in &tets {
            let sign = crate::meshgen::predicates::orient3d_filtered(
                points[t[0] as usize],
                points[t[1] as usize],
                points[t[2] as usize],
                points[t[3] as usize],
            )
            .0;
            assert_eq!(sign, 1, "tet {t:?} is not positively oriented");
        }
        assert!((tets_volume(&tets, &points) - 1.0).abs() < 1.0e-15);
    }

    // The Delaunay property itself, stated as the test: no point of the set lies strictly inside
    // any tet's circumsphere. Points ON a circumsphere are permitted and are the cospherical case.
    #[test]
    fn no_point_lies_strictly_inside_another_tets_circumsphere() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.9, 0.8, 0.7),
            Vec3::new(0.3, 0.1, 0.6),
            Vec3::new(0.7, 0.2, 0.1),
        ];
        let tets = delaunay_tets(&points, &keys_of(&points)).expect("tetrahedralisable");
        for t in &tets {
            for (id, probe) in points.iter().enumerate() {
                if t.contains(&(id as u32)) {
                    continue;
                }
                assert!(
                    crate::meshgen::predicates::insphere(
                        points[t[0] as usize],
                        points[t[1] as usize],
                        points[t[2] as usize],
                        points[t[3] as usize],
                        *probe
                    ) <= 0,
                    "point {id} is inside tet {t:?}'s circumsphere"
                );
            }
        }
    }

    // **R-P2, and the reason §7.4 inserts in ascending NodeKey order.** The same point set handed
    // over in a different order must give the same tetrahedralisation - compared by COORDINATES,
    // since the indices are exactly what changed.
    #[test]
    fn the_result_is_a_function_of_the_points_not_of_their_order() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.9, 0.8, 0.7),
            Vec3::new(0.3, 0.1, 0.6),
            Vec3::new(0.7, 0.2, 0.1),
        ];
        let geometric = |tets: &[[u32; 4]], points: &[Vec3]| -> Vec<[NodeKey; 4]> {
            let mut out: Vec<[NodeKey; 4]> = tets
                .iter()
                .map(|t| {
                    let mut k = [
                        node_key(points[t[0] as usize], QUANTUM),
                        node_key(points[t[1] as usize], QUANTUM),
                        node_key(points[t[2] as usize], QUANTUM),
                        node_key(points[t[3] as usize], QUANTUM),
                    ];
                    k.sort_unstable();
                    k
                })
                .collect();
            out.sort_unstable();
            out
        };
        let reference = geometric(
            &delaunay_tets(&points, &keys_of(&points)).expect("tetrahedralisable"),
            &points,
        );
        // Every rotation of the input is a different indexing of the same geometry.
        for shift in 1..points.len() {
            let shuffled: Vec<Vec3> = (0..points.len())
                .map(|slot| points[(slot + shift) % points.len()])
                .collect();
            let tets = delaunay_tets(&shuffled, &keys_of(&shuffled)).expect("tetrahedralisable");
            assert_eq!(
                geometric(&tets, &shuffled),
                reference,
                "rotating the input by {shift} changed the tetrahedralisation"
            );
        }
    }

    // Fewer than four points, or four coplanar ones, have no tetrahedralisation - and that is a
    // refusal rather than an empty answer, because an empty answer reads as "nothing to do".
    #[test]
    fn a_degenerate_point_set_is_refused() {
        let flat = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ];
        assert!(delaunay_tets(&flat, &keys_of(&flat)).is_none());
        let too_few = flat[..3].to_vec();
        assert!(delaunay_tets(&too_few, &keys_of(&too_few)).is_none());
    }

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

    // The cell-level entry point, with the boundary already carrying the trace as an edge - which
    // is the state the face side has to deliver. Then the fragment drives the whole cut: two
    // pieces, the plane is a face of both, the boundary comes back triangle for triangle, and the
    // volumes are the exact 1/48 and 7/48 the geometry says. This is the composition PLAN §6.24
    // says has to work before escalating anything into it.
    #[test]
    fn a_fragment_cuts_the_cell_when_the_boundary_already_carries_the_trace() {
        // A unit tet whose faces are pre-split along z = 0.5, as the two cells sharing each face
        // would have split it. 4..6 are the crossings on AD, BD and CD.
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, 0.5),
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(0.0, 0.5, 0.5),
        ];
        let boundary = [
            [0, 2, 1],
            [5, 3, 4],
            [0, 1, 5],
            [0, 5, 4],
            [4, 3, 6],
            [0, 4, 6],
            [0, 6, 2],
            [6, 3, 5],
            [1, 2, 6],
            [1, 6, 5],
        ];
        let fragment = vec![(
            7,
            vec![[points[4], points[5], points[6]]],
        )];
        let out = subdivide_cell_by_fragment(&boundary, &fragment, &points, QUANTUM, 1.0e-9)
            .expect("the fragment must cut a cell whose boundary already carries its trace");
        assert_eq!(out.pieces.len(), 2, "one plane through the cell gives two pieces");

        let volume = |soup: &[[u32; 3]]| -> f64 {
            soup.iter()
                .map(|t| {
                    let (a, b, c) = (
                        points[t[0] as usize],
                        points[t[1] as usize],
                        points[t[2] as usize],
                    );
                    a.dot(b.cross(c)) / 6.0
                })
                .sum()
        };
        let mut volumes: Vec<f64> = out.pieces.iter().map(|p| volume(p).abs()).collect();
        volumes.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            (volumes[0] - 1.0 / 48.0).abs() < 1.0e-12 && (volumes[1] - 7.0 / 48.0).abs() < 1.0e-12,
            "the plane z = 0.5 cuts the unit tet into 1/48 and 7/48, got {volumes:?}"
        );

        // Every cap is in the fragment's plane and carries the component it came from - that is
        // what the caller tags as an interface triangle.
        assert!(!out.caps.is_empty(), "the cut must produce cap triangles");
        for (tri, component) in &out.caps {
            assert_eq!(*component, 7, "a cap carries its fragment's component");
            for id in tri {
                assert!(
                    (points[*id as usize].z - 0.5).abs() < 1.0e-12,
                    "a cap must lie in the fragment's plane"
                );
            }
        }
    }

    // The refusal, and the reason for it, pinned so nobody weakens the check to make the sub-cell
    // body pass. A fragment strictly inside a cell whose faces are WHOLE is declined - not because
    // the kernel cannot cut it, which the same fragment on the same cell proves it can, but because
    // the cut would need NODES THAT DO NOT EXIST, and the refusal says exactly where: on a shared
    // lattice EDGE. That is the over-cut in its sharpest form - the strut's supporting plane is
    // INFINITE while the strut is not, so the plane crosses the tet's edges even though the surface
    // never does. A node invented there exists in this cell and not in the five others around that
    // edge, which is a crack. Naming the refusal is what identified this: the guard that fires is
    // the arena's, one earlier than the shared-boundary check this comment used to claim, and the
    // node it wants is on an edge rather than inside a face - so the face's own triangulation would
    // not supply it either.
    #[test]
    fn the_subcell_body_is_refused_until_the_face_carries_its_trace() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let boundary = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let tris = vec![[
            Vec3::new(0.05, 0.05, 0.25),
            Vec3::new(0.20, 0.05, 0.25),
            Vec3::new(0.05, 0.20, 0.25),
        ]];
        assert_eq!(
            subdivide_cell_by_fragment(&boundary, &[(1, tris.clone())], &points, QUANTUM, 1.0e-9)
                .err(),
            Some("the plane OVER-CUT: a node where the surface does not reach"),
            "a fragment needing a node the neighbour does not have must be refused, and the \
             refusal must name that rather than some later guard"
        );
        // ... and the refusal is the boundary check, not an inability to cut: the same fragment
        // subdivides the same cell perfectly well when nothing outside it has to agree.
        let mut arena = NodeArena::new(points.clone(), QUANTUM);
        let cell = cell_of_tet([0, 1, 2, 3], &arena);
        assert!(
            subdivide_by_fragment(&cell, &tris, &mut arena, TOL).len() >= 2,
            "the kernel itself cuts this cell; only the shared boundary stops the wrapper"
        );
    }

    // The three inputs SPEC §7.2 names are the tet, the fragment and the curve segments. The tet
    // the pipeline has; the fragment it has never produced. A triangle beyond one of the cell's own
    // face planes is no part of it.
    // A facet stops where the cell does. Every vertex of it is inside the tet or on its boundary -
    // which is the difference between a bounded constraint and an infinite supporting plane.
    #[test]
    fn a_facet_stops_where_the_cell_does() {
        let tet = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let tris = [[
            Vec3::new(-1.0, -1.0, 0.25),
            Vec3::new(3.0, -1.0, 0.25),
            Vec3::new(-1.0, 3.0, 0.25),
        ]];
        let facets = fragment_facets_in_cell(tet, &tris, TOL);
        assert_eq!(facets.len(), 1);
        for point in &facets[0] {
            assert!(
                point.x >= -1.0e-12
                    && point.y >= -1.0e-12
                    && point.z >= -1.0e-12
                    && point.x + point.y + point.z <= 1.0 + 1.0e-12,
                "facet vertex {point:?} is outside the cell"
            );
        }
        // The cross-section of the unit tet at z = 0.25 is a triangle of legs 0.75, area 0.28125.
        let mut area2 = Vec3::new(0.0, 0.0, 0.0);
        for slot in 1..facets[0].len() - 1 {
            area2 = area2.add(
                facets[0][slot]
                    .sub(facets[0][0])
                    .cross(facets[0][slot + 1].sub(facets[0][0])),
            );
        }
        assert!((area2.dot(area2).sqrt() * 0.5 - 0.28125).abs() < 1.0e-12);
    }

    // **The rim, which is the whole reason for bounding the facet.** A patch that ends inside the
    // cell comes back with its own boundary, touching none of the cell's faces - so it constrains
    // only where the surface is, and material flows round it. The plane through the same triangle
    // would have cut the cell in two (PLAN §6.26).
    #[test]
    fn a_facet_that_ends_inside_the_cell_keeps_its_rim() {
        let tet = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let tris = [[
            Vec3::new(0.05, 0.05, 0.25),
            Vec3::new(0.20, 0.05, 0.25),
            Vec3::new(0.05, 0.20, 0.25),
        ]];
        let facets = fragment_facets_in_cell(tet, &tris, TOL);
        assert_eq!(facets.len(), 1);
        assert_eq!(facets[0].len(), 3, "an interior triangle is its own facet");
        for point in &facets[0] {
            let on_boundary = point.x.abs() < 1.0e-12
                || point.y.abs() < 1.0e-12
                || point.z.abs() < 1.0e-12
                || (point.x + point.y + point.z - 1.0).abs() < 1.0e-12;
            assert!(!on_boundary, "the rim must not touch the cell's boundary");
        }
    }

    // **The conformity argument, as a test.** Where a facet meets a shared face, its boundary edge
    // there must be exactly the trace that face computes for itself - because the neighbour will
    // triangulate the face from its own `trace_on_face` and the two have to agree without talking.
    // The facet is clipped from the cell's side and the trace is computed from the face's side; if
    // they ever disagreed, a constrained tetrahedralisation would be conforming in one cell and not
    // in the other.
    #[test]
    fn the_facets_edge_on_a_face_is_exactly_that_faces_own_trace() {
        let tet = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let face = [tet[0], tet[1], tet[2]];
        // A surface standing on the plane x = 0.2, wide enough to cross the whole cell.
        let tris = [[
            Vec3::new(0.2, -1.0, -1.0),
            Vec3::new(0.2, 3.0, -1.0),
            Vec3::new(0.2, -1.0, 3.0),
        ]];
        let facets = fragment_facets_in_cell(tet, &tris, TOL);
        assert_eq!(facets.len(), 1);

        // The facet's edges lying in the face's plane, as unordered endpoint pairs.
        let mut from_cell: Vec<[Vec3; 2]> = Vec::new();
        for slot in 0..facets[0].len() {
            let (a, b) = (facets[0][slot], facets[0][(slot + 1) % facets[0].len()]);
            if a.z.abs() < 1.0e-12 && b.z.abs() < 1.0e-12 {
                from_cell.push([a, b]);
            }
        }
        let from_face = trace_on_face(face, &tris, TOL);
        assert_eq!(from_cell.len(), 1, "the facet crosses this face once");
        assert_eq!(from_face.len(), 1, "and the face sees one chord");

        let same = |p: Vec3, q: Vec3| p.sub(q).dot(p.sub(q)).sqrt() < 1.0e-12;
        let matched = (same(from_cell[0][0], from_face[0][0])
            && same(from_cell[0][1], from_face[0][1]))
            || (same(from_cell[0][0], from_face[0][1]) && same(from_cell[0][1], from_face[0][0]));
        assert!(
            matched,
            "the cell's facet edge {:?} and the face's own trace {:?} must be the same segment",
            from_cell[0], from_face[0]
        );
    }

    // A triangle beyond the cell contributes no facet, exactly as it contributes no plane.
    #[test]
    fn a_triangle_outside_the_cell_yields_no_facet() {
        let tet = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let tris = [[
            Vec3::new(0.0, 0.0, 2.0),
            Vec3::new(1.0, 0.0, 2.0),
            Vec3::new(0.0, 1.0, 2.0),
        ]];
        assert!(fragment_facets_in_cell(tet, &tris, TOL).is_empty());
    }

    #[test]
    fn a_triangle_outside_the_cell_is_no_part_of_its_fragment() {
        let tet = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let tris = [[
            Vec3::new(0.0, 0.0, 2.0),
            Vec3::new(1.0, 0.0, 2.0),
            Vec3::new(0.0, 1.0, 2.0),
        ]];
        assert!(fragment_in_cell(tet, &tris, TOL).is_empty());
    }

    // A triangle that passes through the cell comes back WHOLE, not clipped: the only consumer is
    // `planes_from_fragment`, a clipped piece has the same supporting plane, and a sliver of one
    // has a numerically worse normal than the triangle it came from.
    #[test]
    fn a_triangle_through_the_cell_comes_back_whole() {
        let tet = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let tris = [[
            Vec3::new(-1.0, -1.0, 0.25),
            Vec3::new(2.0, -1.0, 0.25),
            Vec3::new(-1.0, 2.0, 0.25),
        ]];
        let fragment = fragment_in_cell(tet, &tris, TOL);
        assert_eq!(fragment.len(), 1);
        assert_eq!(fragment[0], tris[0], "the original triangle, not a clipped piece");
    }

    // **Measure-zero contact is not fragment**, and this is the test that keeps it that way. A
    // triangle meeting the tet only at one corner has no material inside it, but its supporting
    // plane is infinite and would cut the whole cell - one tangent triangle doubling the piece
    // count for nothing. Excluding it is geometry, not a threshold: the fragment is the part of the
    // surface IN the cell.
    #[test]
    fn a_triangle_touching_only_a_corner_is_not_fragment() {
        let tet = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let tris = [[
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
        ]];
        assert!(
            fragment_in_cell(tet, &tris, TOL).is_empty(),
            "a corner touch has zero area and contributes no plane"
        );
    }

    // Why the clip has to exist at all, stated as a measurement rather than an assertion: the
    // kernel is driven by the fragment's PLANES, and handing it the whole surface hands it a plane
    // per distant patch. Every one of those cuts the cell, because a supporting plane is infinite.
    #[test]
    fn clipping_to_the_cell_is_what_makes_the_plane_set_small() {
        let tet = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        // Two triangles of one strut crossing the cell, coplanar at z = 0.3 ...
        let mut tris = vec![
            [
                Vec3::new(-1.0, -1.0, 0.3),
                Vec3::new(2.0, -1.0, 0.3),
                Vec3::new(2.0, 2.0, 0.3),
            ],
            [
                Vec3::new(-1.0, -1.0, 0.3),
                Vec3::new(2.0, 2.0, 0.3),
                Vec3::new(-1.0, 2.0, 0.3),
            ],
        ];
        // ... and six more patches of the same surface, each far away and in its own plane.
        for (axis, at) in [(0usize, 5.0), (0, 6.0), (1, 5.0), (1, 6.0), (2, 5.0), (2, 6.0)] {
            let mut corner = [Vec3::new(0.0, 0.0, 0.0); 3];
            for (slot, point) in corner.iter_mut().enumerate() {
                let mut p = [0.0, 0.0, 0.0];
                p[axis] = at;
                p[(axis + 1) % 3] = slot as f64;
                p[(axis + 2) % 3] = (slot % 2) as f64;
                *point = Vec3::new(p[0], p[1], p[2]);
            }
            tris.push(corner);
        }
        let whole = planes_from_fragment(&tris, QUANTUM).len();
        let fragment = fragment_in_cell(tet, &tris, TOL);
        let clipped = planes_from_fragment(&fragment, QUANTUM).len();
        assert_eq!(fragment.len(), 2, "only the strut's two triangles meet the cell");
        assert_eq!(clipped, 1, "and they are coplanar, so they are one constraint");
        assert!(
            whole > clipped,
            "the unclipped surface must offer more planes ({whole}) than the fragment ({clipped})"
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


    // The segments come back one per surface triangle, in canonical but otherwise unrelated order.
    // Chaining has to recover the ring regardless.
    #[test]
    fn scrambled_segments_chain_into_one_ring() {
        let p = |x: f64, y: f64| Vec3::new(x, y, 0.0);
        let square = [
            [p(0.4, 0.2), p(0.4, 0.4)],
            [p(0.2, 0.2), p(0.4, 0.2)],
            [p(0.2, 0.4), p(0.2, 0.2)],
            [p(0.4, 0.4), p(0.2, 0.4)],
        ];
        let rings = chain_trace(&square, QUANTUM).expect("a closed square chains");
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].len(), 4, "four distinct corners");
    }

    // Two struts through one face give two loops, and they must come back separately - merging them
    // would put an edge across the face between two unrelated bodies.
    #[test]
    fn two_bodies_through_one_face_give_two_rings() {
        let p = |x: f64, y: f64| Vec3::new(x, y, 0.0);
        let tri = |ox: f64| {
            [
                [p(ox, 0.1), p(ox + 0.1, 0.1)],
                [p(ox + 0.1, 0.1), p(ox, 0.2)],
                [p(ox, 0.2), p(ox, 0.1)],
            ]
        };
        let mut segments = tri(0.1).to_vec();
        segments.extend(tri(0.5));
        let rings = chain_trace(&segments, QUANTUM).expect("two disjoint loops chain");
        assert_eq!(rings.len(), 2, "one ring per body");
        assert!(rings.iter().all(|r| r.len() == 3));
    }

    // An open chain means the trace is incomplete. Triangulating a hole from it would put a
    // spurious edge across the face, so the caller must be told to fall back rather than handed a
    // ring that closes a gap that is not there.
    #[test]
    fn an_open_chain_is_refused_rather_than_closed() {
        let p = |x: f64, y: f64| Vec3::new(x, y, 0.0);
        let open = [[p(0.2, 0.2), p(0.4, 0.2)], [p(0.4, 0.2), p(0.4, 0.4)]];
        assert!(chain_trace(&open, QUANTUM).is_none(), "an open chain must be refused");
    }

    // Endpoints computed by two adjacent surface triangles agree only to within the quantum, so
    // matching has to be quantised - exact equality would shatter every ring into open chains.
    #[test]
    fn endpoints_that_differ_below_the_quantum_still_chain() {
        let p = |x: f64, y: f64| Vec3::new(x, y, 0.0);
        let eps = QUANTUM * 0.25;
        let ring = [
            [p(0.2, 0.2), p(0.4, 0.2)],
            [p(0.4 + eps, 0.2), p(0.4, 0.4)],
            [p(0.4, 0.4 + eps), p(0.2, 0.2 - eps)],
        ];
        let rings = chain_trace(&ring, QUANTUM).expect("near-equal endpoints must still chain");
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].len(), 3);
    }

}
