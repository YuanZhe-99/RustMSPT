//! S5 - Background lattice: strong 2:1 balance, the Freudenthal-Kuhn 6-tet
//! decomposition, and the centroid-fan transition templates
//! (PLAN §10.7, SPEC_meshgen_geometry §2 and §3 - both frozen and machine-checked
//! before any of this was written).
//!
//! The whole stage is combinatorial. Every node of the lattice - cell corners,
//! split-edge midpoints, face centres, cell centroids - lies exactly on the
//! integer grid of step `h_min/2` (SPEC §3.2), so this module works in that
//! **doubled index space** throughout: a leaf at level `L` has step
//! `1 << (max_level + 1 - L)` and origin `coord * step`, every midpoint and
//! centre is an integer point, and `orient3d` is an exact `i64` determinant. No
//! lattice decision consults a tolerance, and no lattice decision needs a
//! neighbour walk - `split(edge)` and `split(face)` are membership tests against
//! the set of leaf corners (SPEC §3.1).
//!
//! **Why that matters (Invariant C, SPEC §1.3).** Every table here is indexed by
//! an *unordered* entity evaluated in global coordinates, so two cells sharing a
//! face compute the same triangles without communicating. That is the entire
//! conformity mechanism: Theorem T1 (SPEC §3.6) follows from it plus strong 2:1
//! balance, and this module's acceptance is the verifier's [V3] agreeing.
//!
//! **Strong** balance - face, edge *and* vertex adjacency within one level - is
//! required, not the usual face-only kind: a leaf one level finer touching `C` on
//! an edge alone still puts a node in the interior of `C`'s edge, and that is
//! precisely the case that silently produces hanging nodes.

use crate::error::{Result, RustMsptError};
use crate::io::vtu::{ArrayData, DataArray, VtuDoc, VTK_TETRA};
use crate::meshgen::arrange::ArrangeComponent;
use crate::meshgen::sizing::{SizingField, SizingLeaf};
use crate::types::Vec3;
use rayon::prelude::*;

/// Default ceiling on emitted tets. A fan-heavy octree emits up to 48 tets per
/// leaf, so the leaf budget alone does not bound memory.
pub const LATTICE_MAX_TETS: usize = 20_000_000;

/// Corner `m` of a cell carries bits `(bx, by, bz)` = `(m & 1, (m >> 1) & 1, (m >> 2) & 1)`
/// (SPEC §2.1); `v0` is the componentwise minimum and `v7` the maximum.
const CORNER_BITS: [[u32; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];

/// The frozen Freudenthal-Kuhn table (SPEC §2.2). Each row is a monotone lattice
/// walk `v0 -> v7`; K1, K2 and K5 carry the last-two-node swap that makes them
/// positively oriented, which is part of the frozen table and not a detail.
pub const FREUDENTHAL: [[usize; 4]; 6] = [
    [0, 1, 3, 7], // K0: x y z
    [0, 1, 7, 5], // K1: x z y
    [0, 2, 7, 3], // K2: y x z
    [0, 2, 6, 7], // K3: y z x
    [0, 4, 5, 7], // K4: z x y
    [0, 4, 7, 6], // K5: z y x
];

/// The six cube faces, corners in cyclic order. Each row's componentwise-min and
/// componentwise-max corners are diagonal, which is what makes Rule D (SPEC §2.3)
/// a pure function of the face's global coordinates.
const FACES: [[usize; 4]; 6] = [
    [0, 2, 6, 4], // x = lo
    [1, 3, 7, 5], // x = hi
    [0, 1, 5, 4], // y = lo
    [2, 3, 7, 6], // y = hi
    [0, 1, 3, 2], // z = lo
    [4, 5, 7, 6], // z = hi
];

/// The twelve cube edges as corner pairs (one differing bit).
const EDGES: [[usize; 2]; 12] = [
    [0, 1],
    [2, 3],
    [4, 5],
    [6, 7],
    [0, 2],
    [1, 3],
    [4, 6],
    [5, 7],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];

/// One octree cell identified by its level and its coordinate at that level.
pub type CellId = (u32, [u32; 3]);

/// The 26 face/edge/vertex neighbour directions used by the strong balance pass.
const NEIGHBOURS: [[i64; 3]; 26] = {
    let mut out = [[0i64; 3]; 26];
    let mut index = 0;
    let mut dx = -1i64;
    while dx <= 1 {
        let mut dy = -1i64;
        while dy <= 1 {
            let mut dz = -1i64;
            while dz <= 1 {
                if !(dx == 0 && dy == 0 && dz == 0) {
                    out[index] = [dx, dy, dz];
                    index += 1;
                }
                dz += 1;
            }
            dy += 1;
        }
        dx += 1;
    }
    out
};

// ---------------------------------------------------------------------------
// Strong 2:1 balance
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: Locate the leaf containing a cell, searching from its own level up to the root; returns Some(level) or None when the cell is refined or absent; side effects: none.
fn containing_leaf(leaves: &[CellId], level: u32, coord: [u32; 3]) -> Option<u32> {
    for ancestor in (0..=level).rev() {
        let shift = level - ancestor;
        let key = (
            ancestor,
            [coord[0] >> shift, coord[1] >> shift, coord[2] >> shift],
        );
        if leaves.binary_search(&key).is_ok() {
            return Some(ancestor);
        }
    }
    None
}

// AI-FUNC-SUMMARY:
// Purpose: Refine an octree until it is **strongly** 2:1 balanced (SPEC §3.1) - any two leaves whose
//   closed boxes touch on a face, an edge, or a vertex differ by at most one level.
// Inputs: the G4-1 sizing field.
// Returns: a new SizingField with the same root and the balanced leaf set.
// Side effects: None.
// Notes: The ripple algorithm: for each leaf at level `L`, look at the 26 neighbouring *cells* at
//   level `L`; if the leaf containing one of them sits at a level coarser than `L - 1`, split it.
//   Repeat to a fixed point. This is complete - if two leaves violate the rule, the coarser one
//   contains a level-`L` neighbour cell of the finer one, whichever way they touch - and it
//   terminates, because every pass strictly increases the depth of some leaf and `max_level` caps it.
//
//   A neighbour cell with **no** containing leaf is either refined past `L` (handled from the finer
//   side on a later pass) or outside the domain (nothing there), so both cases are correctly skipped.
//
//   Determinism: each pass scans in canonical leaf order, collects the splits into a sorted set, and
//   applies them together. Nothing depends on the order splits are discovered in, so the parallel
//   scan is free of ordering questions. Split children inherit the parent's `h`; that keeps the
//   "leaf is no larger than the field inside it" invariant (the child is half the size and the field
//   over a sub-box is no smaller), while making balance independent of the field it balances.
pub fn balance_octree(field: &SizingField) -> (SizingField, usize) {
    let mut leaves: Vec<CellId> = field
        .leaves
        .iter()
        .map(|leaf| (leaf.level, leaf.coord))
        .collect();
    let mut sizes: std::collections::BTreeMap<CellId, f64> = field
        .leaves
        .iter()
        .map(|leaf| ((leaf.level, leaf.coord), leaf.h))
        .collect();
    leaves.sort_unstable();
    let mut splits_total = 0usize;

    loop {
        let requests: Vec<CellId> = leaves
            .par_iter()
            .flat_map_iter(|(level, coord)| {
                let mut out: Vec<CellId> = Vec::new();
                if *level < 2 {
                    return out.into_iter();
                }
                for delta in NEIGHBOURS {
                    let limit = 1i64 << level;
                    let candidate = [
                        coord[0] as i64 + delta[0],
                        coord[1] as i64 + delta[1],
                        coord[2] as i64 + delta[2],
                    ];
                    if candidate.iter().any(|value| *value < 0)
                        || candidate.iter().any(|value| *value >= limit)
                    {
                        continue;
                    }
                    let cell = [
                        candidate[0] as u32,
                        candidate[1] as u32,
                        candidate[2] as u32,
                    ];
                    if let Some(owner) = containing_leaf(&leaves, *level, cell) {
                        if owner + 1 < *level {
                            let shift = *level - owner;
                            out.push((
                                owner,
                                [cell[0] >> shift, cell[1] >> shift, cell[2] >> shift],
                            ));
                        }
                    }
                }
                out.into_iter()
            })
            .collect();
        if requests.is_empty() {
            break;
        }
        let mut to_split: Vec<CellId> = requests;
        to_split.par_sort_unstable();
        to_split.dedup();
        splits_total += to_split.len();

        let mut next: Vec<CellId> = Vec::with_capacity(leaves.len() + to_split.len() * 7);
        let mut split_index = 0usize;
        for entry in &leaves {
            let is_split = split_index < to_split.len() && to_split[split_index] == *entry;
            if is_split {
                split_index += 1;
                let parent_h = sizes.remove(entry).unwrap_or(f64::INFINITY);
                for dz in 0..2u32 {
                    for dy in 0..2u32 {
                        for dx in 0..2u32 {
                            let child = (
                                entry.0 + 1,
                                [
                                    entry.1[0] * 2 + dx,
                                    entry.1[1] * 2 + dy,
                                    entry.1[2] * 2 + dz,
                                ],
                            );
                            sizes.insert(child, parent_h);
                            next.push(child);
                        }
                    }
                }
            } else {
                next.push(*entry);
            }
        }
        next.par_sort_unstable();
        leaves = next;
    }

    let mut out: Vec<SizingLeaf> = leaves
        .iter()
        .map(|(level, coord)| SizingLeaf {
            level: *level,
            coord: *coord,
            h: sizes[&(*level, *coord)],
        })
        .collect();
    out.sort_unstable_by_key(|leaf| (leaf.level, leaf.coord));

    let mut stats = field.stats.clone();
    stats.n_leaves = out.len();
    stats.h_smallest = f64::INFINITY;
    stats.h_largest = 0.0;
    stats.max_level_used = 0;
    for leaf in &out {
        stats.h_smallest = stats.h_smallest.min(leaf.h);
        stats.h_largest = stats.h_largest.max(leaf.h);
        stats.max_level_used = stats.max_level_used.max(leaf.level);
    }
    if out.is_empty() {
        stats.h_smallest = 0.0;
    }
    (
        SizingField {
            origin: field.origin,
            root_size: field.root_size,
            max_level: field.max_level,
            leaves: out,
            stats,
        },
        splits_total,
    )
}

// AI-FUNC-SUMMARY:
// Purpose: Check the strong 2:1 balance property directly, for assertions and tests.
// Returns: the offending `(fine, coarse)` leaf pair with the largest level gap, or None when balanced.
// Side effects: None.
pub fn balance_violation(field: &SizingField) -> Option<(CellId, CellId)> {
    let mut leaves: Vec<CellId> = field
        .leaves
        .iter()
        .map(|leaf| (leaf.level, leaf.coord))
        .collect();
    leaves.sort_unstable();
    for (level, coord) in &leaves {
        if *level < 2 {
            continue;
        }
        for delta in NEIGHBOURS {
            let limit = 1i64 << level;
            let candidate = [
                coord[0] as i64 + delta[0],
                coord[1] as i64 + delta[1],
                coord[2] as i64 + delta[2],
            ];
            if candidate.iter().any(|value| *value < 0 || *value >= limit) {
                continue;
            }
            let cell = [
                candidate[0] as u32,
                candidate[1] as u32,
                candidate[2] as u32,
            ];
            if let Some(owner) = containing_leaf(&leaves, *level, cell) {
                if owner + 1 < *level {
                    let shift = *level - owner;
                    return Some((
                        (*level, *coord),
                        (owner, [cell[0] >> shift, cell[1] >> shift, cell[2] >> shift]),
                    ));
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Index space
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: Doubled-index-space step of a cell at the given level; returns u32; side effects: none.
fn index_step(max_level: u32, level: u32) -> u32 {
    1u32 << (max_level + 1 - level)
}

// AI-FUNC-SUMMARY: The eight corners of one leaf in doubled index space, in `v0..v7` order; returns the array; side effects: none.
fn leaf_corners(max_level: u32, leaf: &SizingLeaf) -> [[u32; 3]; 8] {
    let step = index_step(max_level, leaf.level);
    let origin = [
        leaf.coord[0] * step,
        leaf.coord[1] * step,
        leaf.coord[2] * step,
    ];
    let mut out = [[0u32; 3]; 8];
    for (corner, bits) in out.iter_mut().zip(CORNER_BITS.iter()) {
        *corner = [
            origin[0] + bits[0] * step,
            origin[1] + bits[1] * step,
            origin[2] + bits[2] * step,
        ];
    }
    out
}

// AI-FUNC-SUMMARY: Componentwise midpoint of two integer lattice points; returns the point; side effects: none.
fn midpoint(a: [u32; 3], b: [u32; 3]) -> [u32; 3] {
    [
        (a[0] + b[0]) / 2,
        (a[1] + b[1]) / 2,
        (a[2] + b[2]) / 2,
    ]
}

// AI-FUNC-SUMMARY: Centre of a quad given its four corners; returns the point; side effects: none.
fn quad_centre(q: [[u32; 3]; 4]) -> [u32; 3] {
    [
        (q[0][0] + q[1][0] + q[2][0] + q[3][0]) / 4,
        (q[0][1] + q[1][1] + q[2][1] + q[3][1]) / 4,
        (q[0][2] + q[1][2] + q[2][2] + q[3][2]) / 4,
    ]
}

// AI-FUNC-SUMMARY: Morton (Z-order) code of a doubled-index-space point, for SPEC §1.4 emission order; returns u64; side effects: none.
fn morton(point: [u32; 3]) -> u64 {
    fn spread(value: u32) -> u64 {
        let mut x = value as u64 & 0x1f_ffff;
        x = (x | (x << 32)) & 0x001f_0000_0000_ffff;
        x = (x | (x << 16)) & 0x001f_0000_ff00_00ff;
        x = (x | (x << 8)) & 0x100f_00f0_0f00_f00f;
        x = (x | (x << 4)) & 0x10c3_0c30_c30c_30c3;
        x = (x | (x << 2)) & 0x1249_2492_4924_9249;
        x
    }
    spread(point[0]) | (spread(point[1]) << 1) | (spread(point[2]) << 2)
}

// AI-FUNC-SUMMARY: Exact `orient3d` on integer lattice points; returns the signed determinant (`6V` in index units); side effects: none.
fn orient3d_index(a: [u32; 3], b: [u32; 3], c: [u32; 3], d: [u32; 3]) -> i64 {
    let sub = |p: [u32; 3], q: [u32; 3]| -> [i64; 3] {
        [
            p[0] as i64 - q[0] as i64,
            p[1] as i64 - q[1] as i64,
            p[2] as i64 - q[2] as i64,
        ]
    };
    let (u, v, w) = (sub(b, a), sub(c, a), sub(d, a));
    u[0] * (v[1] * w[2] - v[2] * w[1]) - u[1] * (v[0] * w[2] - v[2] * w[0])
        + u[2] * (v[0] * w[1] - v[1] * w[0])
}

// ---------------------------------------------------------------------------
// The face rule f(F) - SPEC §3.3
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: Whether an integer lattice point is a corner of some leaf (SPEC §3.1); returns bool; side effects: none.
fn is_corner(corners: &[[u32; 3]], point: [u32; 3]) -> bool {
    corners.binary_search(&point).is_ok()
}

// AI-FUNC-SUMMARY:
// Purpose: Rule D (SPEC §2.3) on one quad - the diagonal joins the componentwise-minimum corner to
//   the componentwise-maximum corner.
// Inputs: the quad's four corners in cyclic order.
// Returns: two triangles.
// Side effects: None.
// Notes: Stated in **global** index space, so two cells sharing the quad compute the same diagonal
//   from the same four coordinates - that translation invariance is the whole reason the primary
//   lattice is Freudenthal rather than the 5-tet checkerboard.
fn case_plain(quad: [[u32; 3]; 4]) -> [[[u32; 3]; 3]; 2] {
    let mut lo = 0usize;
    for index in 1..4 {
        if quad[index] < quad[lo] {
            lo = index;
        }
    }
    // The componentwise minimum and maximum of an axis-aligned quad are diagonal.
    let hi = (lo + 2) % 4;
    let o1 = (lo + 1) % 4;
    let o2 = (lo + 3) % 4;
    [
        [quad[lo], quad[o1], quad[hi]],
        [quad[lo], quad[hi], quad[o2]],
    ]
}

// AI-FUNC-SUMMARY:
// Purpose: The frozen face rule `f(F)` (SPEC §3.3): case P (plain), E (edge fan) or Q (quadrant).
// Inputs: the face's four corners in cyclic order and the lattice corner set.
// Returns: the face's triangles.
// Side effects: None.
// Notes: A pure function of the face's global coordinates and the split state of its centre and
//   edges - Invariant C (SPEC §1.3). Case Q's quadrants are exactly the faces of the four finer
//   neighbours, which by L1 evaluate case P on them and so produce the same two triangles.
fn face_rule(quad: [[u32; 3]; 4], corners: &[[u32; 3]]) -> Vec<[[u32; 3]; 3]> {
    let centre = quad_centre(quad);
    if is_corner(corners, centre) {
        // Case Q: four quadrant sub-faces, each evaluated as case P at level L+1.
        let mut out = Vec::with_capacity(8);
        let mids = [
            midpoint(quad[0], quad[1]),
            midpoint(quad[1], quad[2]),
            midpoint(quad[2], quad[3]),
            midpoint(quad[3], quad[0]),
        ];
        for index in 0..4 {
            let previous = (index + 3) % 4;
            let quadrant = [quad[index], mids[index], centre, mids[previous]];
            out.extend_from_slice(&case_plain(quadrant));
        }
        return out;
    }

    let mut polygon: Vec<[u32; 3]> = Vec::with_capacity(8);
    let mut split_edges = 0usize;
    for index in 0..4 {
        polygon.push(quad[index]);
        let mid = midpoint(quad[index], quad[(index + 1) % 4]);
        if is_corner(corners, mid) {
            polygon.push(mid);
            split_edges += 1;
        }
    }
    if split_edges == 0 {
        // Case P.
        return case_plain(quad).to_vec();
    }
    // Case E: fan the boundary polygon from the (new) face centre.
    let mut out = Vec::with_capacity(polygon.len());
    for index in 0..polygon.len() {
        out.push([centre, polygon[index], polygon[(index + 1) % polygon.len()]]);
    }
    out
}

// ---------------------------------------------------------------------------
// The lattice
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: How a leaf was tetrahedralized; side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellTemplate {
    /// No face and no edge split: the frozen 6-tet Freudenthal table.
    Freudenthal,
    /// At least one split face or edge: the centroid fan over `f(F)`.
    Fan,
}

// AI-FUNC-SUMMARY: What the lattice build produced; side effects: none.
#[derive(Debug, Clone, Default)]
pub struct LatticeStats {
    pub n_leaves: usize,
    pub n_freudenthal: usize,
    pub n_fan: usize,
    pub n_nodes: usize,
    pub n_tets: usize,
    pub n_balance_splits: usize,
    pub min_volume: f64,
    pub max_volume: f64,
}

// AI-FUNC-SUMMARY:
// Purpose: The tetrahedralized background lattice (SPEC §2 + §3).
// Notes: `nodes` is in `NodeKey` order - lattice coordinates are exact integers, so lexicographic
//   order on the index-space point *is* the frozen node order and needs no quantization. `tets` is
//   in ascending Morton cell order with template rows in table order (SPEC §1.4), and every tet is
//   positively oriented.
#[derive(Debug, Clone)]
pub struct Lattice {
    pub nodes: Vec<Vec3>,
    pub node_index: Vec<[u32; 3]>,
    pub tets: Vec<[u32; 4]>,
    pub cell_of_tet: Vec<u32>,
    pub templates: Vec<CellTemplate>,
    pub stats: LatticeStats,
}

// AI-FUNC-SUMMARY: Build options for the lattice; side effects: none.
#[derive(Debug, Clone)]
pub struct LatticeOptions {
    pub max_tets: usize,
}

impl Default for LatticeOptions {
    // AI-FUNC-SUMMARY: The default tet budget; returns LatticeOptions; side effects: none.
    fn default() -> Self {
        LatticeOptions {
            max_tets: LATTICE_MAX_TETS,
        }
    }
}

/// A leaf's template decision, the triangles its faces produced, and the nodes it
/// introduces beyond the lattice corners.
type CellGeometry = (CellTemplate, Vec<[[u32; 3]; 3]>, Vec<[u32; 3]>);

// AI-FUNC-SUMMARY:
// Purpose: The per-leaf template decision and its emitted triangles, shared by both build passes.
// Returns: CellGeometry.
// Side effects: None.
fn cell_triangles(
    max_level: u32,
    leaf: &SizingLeaf,
    corners: &[[u32; 3]],
) -> CellGeometry {
    let cube = leaf_corners(max_level, leaf);
    let split_edge = EDGES
        .iter()
        .any(|edge| is_corner(corners, midpoint(cube[edge[0]], cube[edge[1]])));
    let split_face = FACES.iter().any(|face| {
        is_corner(
            corners,
            quad_centre([
                cube[face[0]],
                cube[face[1]],
                cube[face[2]],
                cube[face[3]],
            ]),
        )
    });
    if !split_edge && !split_face {
        return (CellTemplate::Freudenthal, Vec::new(), Vec::new());
    }

    let mut triangles = Vec::with_capacity(48);
    for face in FACES {
        let quad = [
            cube[face[0]],
            cube[face[1]],
            cube[face[2]],
            cube[face[3]],
        ];
        triangles.extend(face_rule(quad, corners));
    }
    // The node set is read back off the triangles this cell actually emits, plus
    // its centroid - never derived a second time from the split state. Deriving it
    // is what broke the first version: case Q's quadrant corners are lattice
    // corners *under L1*, but a finer neighbour dropped for lying outside the
    // domain takes its corners with it, and pass two then emitted a node pass one
    // had never heard of. Reading it back makes "pass one collects exactly what
    // pass two emits" structural instead of argued.
    let mut new_nodes = Vec::with_capacity(triangles.len() * 3 + 1);
    new_nodes.push(midpoint(cube[0], cube[7]));
    for triangle in &triangles {
        new_nodes.extend_from_slice(triangle);
    }
    new_nodes.sort_unstable();
    new_nodes.dedup();
    (CellTemplate::Fan, triangles, new_nodes)
}

// AI-FUNC-SUMMARY:
// Purpose: Tetrahedralize a **strongly 2:1-balanced** octree (SPEC §2.2 + §3.4).
// Inputs: the balanced field and the tet budget.
// Returns: the Lattice, or InvalidConfig when the budget would be exceeded.
// Side effects: None.
// Notes: Two parallel passes over the leaves, each in the permitted shape (PLAN §12.5). Pass one
//   collects the nodes every cell needs - its eight corners, its centroid if it is a fan cell, and
//   any case-E face centre - which are sorted and deduplicated into `NodeKey` order; pass two
//   re-evaluates the same templates and emits tets as indices, concatenated in Morton cell order.
//   Doing the template work twice is cheaper than materializing a coordinate quadruple per tet, and
//   both passes are pure functions of the same input, so they cannot disagree.
//
//   Every emitted tet goes through the canonical orientation fix (SPEC §1.1): emit `(n0,n1,n2,n3)`
//   and swap `n1`/`n2` when `orient3d <= 0`. The determinant is exact in `i64` here, so a zero is a
//   real degeneracy - it cannot be emitted, and there is no tolerance to tune.
pub fn build_lattice(field: &SizingField, options: &LatticeOptions) -> Result<Lattice> {
    build_lattice_with_splits(field, options, 0)
}

// AI-FUNC-SUMMARY: `build_lattice` carrying the balance pass's split count into the stats; returns Result<Lattice>; side effects: none.
pub fn build_lattice_with_splits(
    field: &SizingField,
    options: &LatticeOptions,
    balance_splits: usize,
) -> Result<Lattice> {
    let max_level = field.max_level;

    // Morton order (SPEC §1.4). Leaf origins are distinct - a leaf and its
    // ancestor cannot both be leaves - so the code alone is a total order.
    let mut order: Vec<(u64, usize)> = field
        .leaves
        .iter()
        .enumerate()
        .map(|(index, leaf)| {
            let step = index_step(max_level, leaf.level);
            (
                morton([
                    leaf.coord[0] * step,
                    leaf.coord[1] * step,
                    leaf.coord[2] * step,
                ]),
                index,
            )
        })
        .collect();
    order.par_sort_unstable();
    let cells: Vec<&SizingLeaf> = order
        .iter()
        .map(|(_, index)| &field.leaves[*index])
        .collect();

    // The lattice corner set - the only input the split predicates need.
    let mut corners: Vec<[u32; 3]> = cells
        .par_iter()
        .flat_map_iter(|leaf| leaf_corners(max_level, leaf).into_iter())
        .collect();
    corners.par_sort_unstable();
    corners.dedup();

    // Pass one: the node set.
    let per_cell: Vec<(CellTemplate, Vec<[u32; 3]>)> = cells
        .par_iter()
        .map(|leaf| {
            let (template, _, new_nodes) = cell_triangles(max_level, leaf, &corners);
            (template, new_nodes)
        })
        .collect();
    let templates: Vec<CellTemplate> = per_cell.iter().map(|(template, _)| *template).collect();

    let mut node_index: Vec<[u32; 3]> = corners.clone();
    for (_, new_nodes) in &per_cell {
        node_index.extend_from_slice(new_nodes);
    }
    node_index.par_sort_unstable();
    node_index.dedup();

    let estimate: usize = templates
        .iter()
        .map(|template| match template {
            CellTemplate::Freudenthal => 6,
            CellTemplate::Fan => 48,
        })
        .sum();
    if estimate > options.max_tets {
        return Err(RustMsptError::InvalidConfig(format!(
            "lattice would emit up to {estimate} tets, above the budget of {}; \
             raise meshgen.sizing.h_min_frac or lower the refinement that produced \
             {} leaves",
            options.max_tets,
            cells.len()
        )));
    }

    // Pass two: the tets.
    let lookup = |point: [u32; 3]| -> u32 {
        node_index
            .binary_search(&point)
            .expect("every emitted node was collected in pass one") as u32
    };
    let per_cell_tets: Vec<Vec<[u32; 4]>> = cells
        .par_iter()
        .map(|leaf| {
            let cube = leaf_corners(max_level, leaf);
            let (template, triangles, _) = cell_triangles(max_level, leaf, &corners);
            let mut out: Vec<[u32; 4]> = Vec::new();
            match template {
                CellTemplate::Freudenthal => {
                    for row in FREUDENTHAL {
                        out.push(oriented(
                            cube[row[0]],
                            cube[row[1]],
                            cube[row[2]],
                            cube[row[3]],
                            &lookup,
                        ));
                    }
                }
                CellTemplate::Fan => {
                    let centroid = midpoint(cube[0], cube[7]);
                    for triangle in triangles {
                        out.push(oriented(
                            triangle[0],
                            triangle[1],
                            triangle[2],
                            centroid,
                            &lookup,
                        ));
                    }
                }
            }
            out
        })
        .collect();

    let mut tets: Vec<[u32; 4]> = Vec::with_capacity(estimate);
    let mut cell_of_tet: Vec<u32> = Vec::with_capacity(estimate);
    for (index, cell_tets) in per_cell_tets.into_iter().enumerate() {
        for tet in cell_tets {
            tets.push(tet);
            cell_of_tet.push(index as u32);
        }
    }

    let unit = field.root_size / (1u64 << (max_level + 1)) as f64;
    let nodes: Vec<Vec3> = node_index
        .par_iter()
        .map(|point| {
            Vec3::new(
                field.origin.x + point[0] as f64 * unit,
                field.origin.y + point[1] as f64 * unit,
                field.origin.z + point[2] as f64 * unit,
            )
        })
        .collect();

    let mut stats = LatticeStats {
        n_leaves: cells.len(),
        n_freudenthal: templates
            .iter()
            .filter(|template| **template == CellTemplate::Freudenthal)
            .count(),
        n_fan: templates
            .iter()
            .filter(|template| **template == CellTemplate::Fan)
            .count(),
        n_nodes: nodes.len(),
        n_tets: tets.len(),
        n_balance_splits: balance_splits,
        min_volume: f64::INFINITY,
        max_volume: 0.0,
    };
    for tet in &tets {
        let volume = signed_volume(&nodes, tet);
        stats.min_volume = stats.min_volume.min(volume);
        stats.max_volume = stats.max_volume.max(volume);
    }
    if tets.is_empty() {
        stats.min_volume = 0.0;
    }

    Ok(Lattice {
        nodes,
        node_index,
        tets,
        cell_of_tet,
        templates,
        stats,
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Emit one tet under the canonical orientation fix (SPEC §1.1).
// Returns: node indices with `n1`/`n2` swapped when the raw order is not positive.
// Side effects: None.
fn oriented(
    a: [u32; 3],
    b: [u32; 3],
    c: [u32; 3],
    d: [u32; 3],
    lookup: &impl Fn([u32; 3]) -> u32,
) -> [u32; 4] {
    if orient3d_index(a, b, c, d) > 0 {
        [lookup(a), lookup(b), lookup(c), lookup(d)]
    } else {
        [lookup(a), lookup(c), lookup(b), lookup(d)]
    }
}

// AI-FUNC-SUMMARY: Signed volume of one tet in world coordinates; returns f64; side effects: none.
fn signed_volume(nodes: &[Vec3], tet: &[u32; 4]) -> f64 {
    let a = nodes[tet[0] as usize];
    let u = nodes[tet[1] as usize].sub(a);
    let v = nodes[tet[2] as usize].sub(a);
    let w = nodes[tet[3] as usize].sub(a);
    u.cross(v).dot(w) / 6.0
}

// ---------------------------------------------------------------------------
// s05_lattice
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Encode the lattice as the `s05_lattice` snapshot VTU.
// Inputs: the lattice, the per-leaf sizing values (for the `sizing_h` point array), and the run's
//   components (for the component tables).
// Returns: VtuDoc of `VTK_TETRA` cells with `cell_kind = 0`.
// Side effects: None.
// Notes: **Recorded amendment to SPEC_meshgen_contracts §3.** The frozen text says the s04 *and*
//   s05 previews write `cell_kind = 3` voxel cells; s05 here writes the tets instead, because the
//   lattice's whole deliverable is its tetrahedralization and G4-2's acceptance is the verifier's
//   [V3] conformity check, which reads tets. A voxel preview of a stage whose output is tets would
//   make the stage unverifiable. s04 keeps the frozen voxel form.
//
//   Every tet carries `region_key = 0` (the background set `{0}`, priority `0xFFFFFFFF`) and
//   `regime = 0` - classification is S6's and thin regimes are S8b's. `partition_id = 0`: with no
//   sheet-tagged faces the whole lattice is one sheet-blocked partition, which is what [V8]
//   recomputes and compares against.
pub fn lattice_to_doc(
    lattice: &Lattice,
    sizing_h: &[f64],
    components: &[ArrangeComponent],
) -> VtuDoc {
    let mut doc = VtuDoc {
        points: lattice.nodes.clone(),
        ..Default::default()
    };
    for tet in &lattice.tets {
        doc.connectivity.extend(tet.iter().map(|node| *node as i64));
        doc.offsets.push(doc.connectivity.len() as i64);
        doc.types.push(VTK_TETRA);
    }
    let cells = lattice.tets.len();
    doc.cell_data
        .push(DataArray::scalar("cell_kind", ArrayData::U8(vec![0; cells])));
    doc.cell_data
        .push(DataArray::scalar("region_key", ArrayData::I32(vec![0; cells])));
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

    let points = doc.points.len();
    doc.point_data.push(DataArray::scalar(
        "n_id_key",
        ArrayData::I32(vec![0; points]),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_kind",
        ArrayData::U8(vec![0; points]),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_ref",
        ArrayData::I32(vec![-1; points]),
    ));
    // The size the field asked for at each node: the smallest value among the
    // cells touching it, which is what makes a level jump legible in a heatmap.
    let mut sizing = vec![f64::INFINITY; points];
    for (tet, cell) in lattice.tets.iter().zip(lattice.cell_of_tet.iter()) {
        let Some(value) = sizing_h.get(*cell as usize) else {
            continue;
        };
        for node in tet {
            let slot = &mut sizing[*node as usize];
            if *value < *slot {
                *slot = *value;
            }
        }
    }
    doc.point_data.push(DataArray::scalar(
        "sizing_h",
        ArrayData::F32(
            sizing
                .iter()
                .map(|value| if value.is_finite() { *value as f32 } else { -1.0 })
                .collect(),
        ),
    ));

    push_field(&mut doc, "RegionSetOffsets", 1, ArrayData::I64(vec![1]));
    push_field(&mut doc, "RegionSetComponents", 1, ArrayData::I32(vec![0]));
    push_field(
        &mut doc,
        "RegionSetPriority",
        1,
        ArrayData::U32(vec![u32::MAX]),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    // AI-FUNC-SUMMARY: The frozen Freudenthal table must be positive and tile the cube exactly (SPEC §2.2, §14 [1]).
    #[test]
    fn the_frozen_freudenthal_table_is_positive_and_tiles_the_cube() {
        let cube: [[u32; 3]; 8] = CORNER_BITS;
        let mut total = 0i64;
        for row in FREUDENTHAL {
            let volume = orient3d_index(cube[row[0]], cube[row[1]], cube[row[2]], cube[row[3]]);
            assert_eq!(volume, 1, "row {row:?} must have orient3d = +1 on the unit cube");
            total += volume;
        }
        // Six tets of volume 1/6 each: the determinants sum to 6 = 6 * V(cube).
        assert_eq!(total, 6);
    }

    // AI-FUNC-SUMMARY: Rule D must agree with the frozen sub-triangle table of SPEC §2.3.
    #[test]
    fn rule_d_reproduces_the_frozen_face_table() {
        let cube: [[u32; 3]; 8] = CORNER_BITS;
        // (face index, the two frozen sub-triangles as corner-id sets)
        let expected: [[[usize; 3]; 2]; 6] = [
            [[0, 2, 6], [0, 4, 6]],
            [[1, 3, 7], [1, 5, 7]],
            [[0, 1, 5], [0, 4, 5]],
            [[2, 3, 7], [2, 6, 7]],
            [[0, 1, 3], [0, 2, 3]],
            [[4, 5, 7], [4, 6, 7]],
        ];
        for (face, rows) in FACES.iter().zip(expected.iter()) {
            let quad = [cube[face[0]], cube[face[1]], cube[face[2]], cube[face[3]]];
            let mut got: Vec<Vec<[u32; 3]>> = case_plain(quad)
                .iter()
                .map(|triangle| {
                    let mut nodes = triangle.to_vec();
                    nodes.sort_unstable();
                    nodes
                })
                .collect();
            got.sort();
            let mut want: Vec<Vec<[u32; 3]>> = rows
                .iter()
                .map(|row| {
                    let mut nodes: Vec<[u32; 3]> = row.iter().map(|id| cube[*id]).collect();
                    nodes.sort_unstable();
                    nodes
                })
                .collect();
            want.sort();
            assert_eq!(got, want, "face {face:?}");
        }
    }

    // AI-FUNC-SUMMARY: Rule D must be independent of which cyclic rotation the caller supplies.
    #[test]
    fn rule_d_is_invariant_under_rotation_of_the_quad() {
        let quad = [[2u32, 4, 6], [6, 4, 6], [6, 8, 6], [2, 8, 6]];
        let canonical: Vec<Vec<[u32; 3]>> = {
            let mut out: Vec<Vec<[u32; 3]>> = case_plain(quad)
                .iter()
                .map(|t| {
                    let mut n = t.to_vec();
                    n.sort_unstable();
                    n
                })
                .collect();
            out.sort();
            out
        };
        for rotation in 1..4usize {
            let rotated = [
                quad[rotation % 4],
                quad[(rotation + 1) % 4],
                quad[(rotation + 2) % 4],
                quad[(rotation + 3) % 4],
            ];
            let mut got: Vec<Vec<[u32; 3]>> = case_plain(rotated)
                .iter()
                .map(|t| {
                    let mut n = t.to_vec();
                    n.sort_unstable();
                    n
                })
                .collect();
            got.sort();
            assert_eq!(got, canonical, "rotation {rotation}");
        }
    }
}
