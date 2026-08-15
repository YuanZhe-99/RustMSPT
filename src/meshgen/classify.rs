//! S6 - Classification: which solid component owns each lattice vertex, the
//! ownership records that seeds, and the active-patch filter
//! (PLAN §10.8 and §5.2, SPEC_meshgen_numerics §2/§6.1, SPEC_meshgen_geometry §9
//! and its arbitration entries ARB-8/ARB-9).
//!
//! The decision is a **parity count**: shoot a ray from the vertex along a fixed
//! direction and count how many of the component's faces it crosses; odd means
//! inside. Every crossing test is exact - five `orient3d` calls per candidate
//! triangle, each through the §6.1 static filter with an exact fallback - so the
//! answer is a topological fact, not a tolerance.
//!
//! **What makes it robust is the degeneracy handling, not the arithmetic.** A ray
//! that grazes an edge or a vertex makes some `orient3d` exactly zero, and a
//! parity count is then meaningless rather than merely imprecise. Any zero
//! therefore abandons the whole ray and re-shoots down a **fixed direction
//! sequence** (ARB-9); only when all of them degenerate does the vertex fall
//! through to the generalized winding number with a WARN. Because the sequence is
//! fixed and the tests are exact, two runs - and two machines - take the same
//! branch.
//!
//! Three component kinds are treated differently, and the difference is
//! normative: a `SolidClosed` component is classified by parity, a
//! `SolidDefective` one (S2b could not certify its closure) goes straight to the
//! winding number, and a `Sheet` **never claims volume at all** regardless of any
//! winding evidence - the hard guard of PLAN §10.4, restated in
//! SPEC_meshgen_geometry §9.1 row 10.

use crate::io::vtu::{ArrayData, DataArray, VtuDoc, VTK_TETRA};
use crate::meshgen::arrange::{ArrangeComponent, ArrangedFace, ArrangedSurface};
use crate::meshgen::lattice::Lattice;
use crate::meshgen::predicates::orient3d_filtered;
use crate::meshgen::topo::{generalized_winding_number, ComponentClassification, RebuiltTopology};
use crate::types::Vec3;
use rayon::prelude::*;
use smallvec::SmallVec;
use std::collections::BTreeMap;

/// The frozen ray-direction sequence (ARB-9). The components are deliberately
/// arbitrary and mutually unrelated, so a feature that degenerates one direction is
/// not aligned with the next; the first is the plan's "fixed irrational ray" and the
/// rest are its re-shoots. Normalization is not required - only the direction
/// matters. They are *not* digits of any named constant: a recognisable constant
/// invites someone to "tidy" it into `std::f64::consts::…`, and a direction that
/// happens to be a simple ratio is exactly the kind that lines up with lattice
/// diagonals and face normals.
pub const RAY_DIRECTIONS: [[f64; 3]; 5] = [
    [0.713_250_410_236_1, 0.394_719_638_517_3, 0.172_306_891_402_7],
    [0.284_193_726_508_9, -0.619_374_205_831_6, 0.450_723_189_647_2],
    [-0.193_748_260_147_3, 0.437_291_058_426_9, 0.879_416_302_753_1],
    [0.824_105_396_718_4, 0.157_384_920_635_8, -0.506_274_819_360_2],
    [-0.458_391_702_463_5, -0.253_640_817_529_4, 0.629_473_158_074_6],
];

/// Grid cells per axis in the projection plane used to find candidate triangles.
const PROJECTION_GRID_DIM: i64 = 128;

/// How far off a face to probe when asking whether it is buried inside its own
/// self-intersecting component. Scaled by the square root of the triangle's own size, so
/// the probe stays local to the face at any resolution while clearing the surface itself.
const ACTIVE_FACE_PROBE: f64 = 1.0e-3;

// AI-FUNC-SUMMARY:
// Purpose: Which side of a component a tet (or vertex) is on, for the sparse ownership record.
// Notes: `Outside` is the *absent* state of PLAN §5.2's record - it is stored here only so a
//   diagnostic can tell "known outside" from "never asked". `Ambiguous` means the lattice cell
//   straddles the surface: S8's cut resolves it, and `resolve()` refuses to run on it
//   (SPEC_meshgen_geometry §9.2 row 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Outside,
    Inside,
    Ambiguous,
}

// AI-FUNC-SUMMARY: Where a record entry came from; matches the contract's `provenance` cell array; side effects: none.
// Notes: `Lattice` is the default because S6 is where every record starts; later stages overwrite
//   it as they take ownership of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Provenance {
    #[default]
    Lattice = 0,
    Cut = 1,
    Arbitrated = 2,
    Junction = 3,
    Band = 4,
}

// AI-FUNC-SUMMARY:
// Purpose: One tet's sparse ownership record (PLAN §5.2).
// Notes: Entries are sorted by component and hold only components the tet is *not* plainly outside
//   of - absent is outside, which is what keeps the record sparse on a lattice where almost every
//   cell is background.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OwnershipRecord {
    pub entries: SmallVec<[(i32, Side); 2]>,
    pub provenance: Provenance,
}

impl OwnershipRecord {
    // AI-FUNC-SUMMARY: The side this record records for one component (absent = Outside); returns Side; side effects: none.
    pub fn side_of(&self, component: i32) -> Side {
        self.entries
            .iter()
            .find(|(x, _)| *x == component)
            .map(|(_, side)| *side)
            .unwrap_or(Side::Outside)
    }

    // AI-FUNC-SUMMARY: Whether any entry is still unresolved, which S10 must refuse; returns bool; side effects: none.
    pub fn is_ambiguous(&self) -> bool {
        self.entries.iter().any(|(_, side)| *side == Side::Ambiguous)
    }
}

// AI-FUNC-SUMMARY: How the classification was reached, per vertex-component decision; side effects: none.
#[derive(Debug, Clone, Default)]
pub struct ClassifyStats {
    pub n_vertices: usize,
    pub n_solid_components: usize,
    pub n_sheet_components: usize,
    pub n_defective_components: usize,
    /// Decisions taken by the first ray of the sequence.
    pub n_first_ray: usize,
    /// Decisions that needed a re-shoot because a ray hit a degenerate feature (ARB-9).
    pub n_reshoots: usize,
    /// Decisions routed to the winding number **by design**, because the component
    /// is one S2b could not certify closed. Expected, not a warning.
    pub n_defective_gwn: usize,
    /// Decisions that exhausted the whole ray sequence and fell through to the
    /// winding number. This one *is* a warning.
    pub n_gwn_fallback: usize,
    /// Crossing tests where the §6.1 static filter could not certify the f64 sign
    /// and the exact predicate had to run.
    pub n_filter_uncertain: usize,
    pub n_inside_pairs: usize,
    pub n_tets_background: usize,
    pub n_tets_owned: usize,
    pub n_tets_ambiguous: usize,
    pub n_inactive_faces: usize,
}

// AI-FUNC-SUMMARY:
// Purpose: The S6 result: per-vertex ownership, the seeded per-tet records, resolved region keys,
//   and the active-patch mask.
// Notes: `region_key` indexes `region_sets`, which is the contract's region-set table. It is the
//   *preliminary* label - a tet still marked `Ambiguous` for some component resolves to what its
//   definite entries say, and S8's cut is what makes it final.
#[derive(Debug, Clone)]
pub struct Classification {
    pub solid_components: Vec<i32>,
    pub vertex_inside: Vec<bool>,
    pub records: Vec<OwnershipRecord>,
    pub region_sets: Vec<Vec<i32>>,
    pub region_key: Vec<i32>,
    pub active_face: Vec<bool>,
    pub warnings: Vec<String>,
    pub stats: ClassifyStats,
}

impl Classification {
    // AI-FUNC-SUMMARY: Whether lattice vertex `vertex` is inside solid component slot `slot`; returns bool; side effects: none.
    pub fn is_inside(&self, vertex: usize, slot: usize) -> bool {
        self.vertex_inside[vertex * self.solid_components.len() + slot]
    }
}

// ---------------------------------------------------------------------------
// resolve() - SPEC_meshgen_geometry §9
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: The frozen label rule (SPEC_meshgen_geometry §9.1): the inside set, reduced to its
//   minimum-priority members.
// Inputs: a record and the priority of each component.
// Returns: the sorted region key; `{0}` (background) when nothing owns the tet.
// Side effects: None.
// Notes: Integer set operations only (predicate class **I**) - no geometry is consulted here, which
//   is why the truth table can be frozen. `Ambiguous` entries do not enter `S`: at S6 they mean
//   "the cut will decide", and S10 must refuse a record that still carries one.
pub fn resolve(record: &OwnershipRecord, priority_of: &BTreeMap<i32, u32>) -> Vec<i32> {
    let inside: Vec<i32> = record
        .entries
        .iter()
        .filter(|(_, side)| *side == Side::Inside)
        .map(|(x, _)| *x)
        .collect();
    let Some(minimum) = inside
        .iter()
        .filter_map(|x| priority_of.get(x).copied())
        .min()
    else {
        return vec![0];
    };
    let mut key: Vec<i32> = inside
        .into_iter()
        .filter(|x| priority_of.get(x).copied() == Some(minimum))
        .collect();
    key.sort_unstable();
    key.dedup();
    if key.is_empty() {
        vec![0]
    } else {
        key
    }
}

// ---------------------------------------------------------------------------
// The projected candidate grid
// ---------------------------------------------------------------------------

/// Triangles bucketed by their footprint in the plane perpendicular to one ray
/// direction, so a ray from any point reduces to a 2D point lookup.
struct ProjectionGrid {
    axis_u: Vec3,
    axis_v: Vec3,
    origin: [f64; 2],
    cell: f64,
    dims: [i64; 2],
    buckets: Vec<Vec<u32>>,
}

impl ProjectionGrid {
    // AI-FUNC-SUMMARY:
    // Purpose: Bucket triangles by their footprint perpendicular to `direction`.
    // Inputs: the triangles' corner points and the ray direction.
    // Returns: ProjectionGrid.
    // Side effects: None.
    // Notes: A ray along `direction` is a single point in this plane, so the candidate set is one
    //   bucket lookup - no traversal. The grid is only ever a *superset* filter; every candidate
    //   still goes through the exact test, so its resolution affects speed and nothing else.
    fn build(triangles: &[[Vec3; 3]], direction: Vec3) -> ProjectionGrid {
        let axis_u = if direction.x.abs() < 0.9 {
            Vec3::new(1.0, 0.0, 0.0)
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        let axis_u = {
            let projected = axis_u.sub(direction.scale(axis_u.dot(direction) / direction.dot(direction)));
            let norm = projected.dot(projected).sqrt();
            projected.scale(1.0 / norm)
        };
        let axis_v = {
            let cross = direction.cross(axis_u);
            let norm = cross.dot(cross).sqrt();
            cross.scale(1.0 / norm)
        };
        let project = |p: Vec3| -> [f64; 2] { [p.dot(axis_u), p.dot(axis_v)] };

        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for triangle in triangles {
            for corner in triangle {
                let q = project(*corner);
                for axis in 0..2 {
                    lo[axis] = lo[axis].min(q[axis]);
                    hi[axis] = hi[axis].max(q[axis]);
                }
            }
        }
        if triangles.is_empty() {
            lo = [0.0, 0.0];
            hi = [1.0, 1.0];
        }
        let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(f64::MIN_POSITIVE);
        let side = (triangles.len() as f64).sqrt().ceil().max(1.0) as i64;
        let side = side.clamp(1, PROJECTION_GRID_DIM);
        let cell = span / side as f64;
        let dims = [
            (((hi[0] - lo[0]) / cell).floor() as i64 + 1).clamp(1, side),
            (((hi[1] - lo[1]) / cell).floor() as i64 + 1).clamp(1, side),
        ];
        let mut grid = ProjectionGrid {
            axis_u,
            axis_v,
            origin: lo,
            cell,
            dims,
            buckets: vec![Vec::new(); (dims[0] * dims[1]) as usize],
        };
        for (index, triangle) in triangles.iter().enumerate() {
            let mut tlo = [f64::INFINITY; 2];
            let mut thi = [f64::NEG_INFINITY; 2];
            for corner in triangle {
                let q = project(*corner);
                for axis in 0..2 {
                    tlo[axis] = tlo[axis].min(q[axis]);
                    thi[axis] = thi[axis].max(q[axis]);
                }
            }
            let (u0, v0) = grid.cell_of(tlo);
            let (u1, v1) = grid.cell_of(thi);
            for v in v0..=v1 {
                for u in u0..=u1 {
                    grid.buckets[(v * grid.dims[0] + u) as usize].push(index as u32);
                }
            }
        }
        grid
    }

    // AI-FUNC-SUMMARY: Grid coordinates of a projected point, clamped into the grid; returns (i64, i64); side effects: none.
    fn cell_of(&self, q: [f64; 2]) -> (i64, i64) {
        let axis = |value: f64, base: f64, dim: i64| -> i64 {
            (((value - base) / self.cell).floor() as i64).clamp(0, dim - 1)
        };
        (
            axis(q[0], self.origin[0], self.dims[0]),
            axis(q[1], self.origin[1], self.dims[1]),
        )
    }

    // AI-FUNC-SUMMARY: Candidate triangle indices whose footprint covers the ray from `point`; returns a slice; side effects: none.
    fn candidates(&self, point: Vec3) -> &[u32] {
        let (u, v) = self.cell_of([point.dot(self.axis_u), point.dot(self.axis_v)]);
        &self.buckets[(v * self.dims[0] + u) as usize]
    }
}

// ---------------------------------------------------------------------------
// The exact crossing test
// ---------------------------------------------------------------------------

/// One ray-triangle test: crossed, missed, or degenerate (a zero `orient3d`,
/// meaning the ray grazes an edge, a vertex, or the triangle's plane).
enum Crossing {
    Miss,
    Hit,
    Degenerate,
}

// AI-FUNC-SUMMARY:
// Purpose: The exact side of a triangle's plane a point is on, with an infinitesimal offset along
//   the ray direction resolving an exact zero.
// Returns: Some(+1/-1), or None when even the offset cannot decide.
// Side effects: Counts filter escalations.
// Notes: This is what makes the stage usable on axis-aligned input. A lattice vertex sitting
//   *exactly* on a face plane - `z = 0.4` on a grid whose nodes include `z = 0.4`, which is the
//   common case, not the corner case - makes `orient3d(a, b, c, p)` exactly zero, and **no
//   re-shoot can help**, because the plane test does not depend on the ray direction at all.
//   Treating it as a degeneracy sent 1.1% of all decisions to the winding number on the review
//   scene.
//
//   The fix is the standard symbolic perturbation: classify `p + delta*direction` for an
//   infinitesimal `delta > 0`. `orient3d(a, b, c, .)` is affine in its last argument with gradient
//   the triangle normal, so the perturbed sign is `sign(n . direction)`. The whole ray shares one
//   `direction`, so the perturbed point is a single consistent point just off the surface and the
//   parity count stays well-defined. When `n . direction` is not certifiably nonzero - the ray lies
//   in the plane - there is nothing to decide and the caller re-shoots, which *does* help there.
fn plane_side(
    triangle: [Vec3; 3],
    point: Vec3,
    direction: Vec3,
    uncertain: &mut usize,
) -> Option<i8> {
    let (a, b, c) = (triangle[0], triangle[1], triangle[2]);
    let (sign, certified, _, _) = orient3d_filtered(a, b, c, point);
    if !certified {
        *uncertain += 1;
    }
    if sign != 0 {
        return Some(sign);
    }
    let normal = b.sub(a).cross(c.sub(a));
    let dot = normal.dot(direction);
    // Static filter on the dot product: three products and two sums, so an error
    // bound of 3u on the sum of magnitudes certifies the sign.
    let magnitude = (normal.x * direction.x).abs()
        + (normal.y * direction.y).abs()
        + (normal.z * direction.z).abs();
    if dot.abs() > 4.0 * f64::EPSILON * magnitude {
        Some(if dot > 0.0 { 1 } else { -1 })
    } else {
        None
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Exact segment-triangle crossing test for the parity count.
// Inputs: the segment `p -> q`, its direction, the triangle, and a counter for filter escalations.
// Returns: Crossing.
// Side effects: Increments `uncertain` when the §6.1 static filter could not certify a sign.
// Notes: Five `orient3d` calls: two decide whether the segment straddles the triangle's plane,
//   three whether the *line* passes through the triangle's interior (all three of the same nonzero
//   sign). A zero in the *plane* tests is resolved by `plane_side`'s perturbation; a zero in the
//   *line* tests is a real degeneracy - the ray passes exactly through an edge or a vertex - and it
//   abandons the ray, because perturbing along the direction does not move the line and so cannot
//   resolve it. Rounding such a hit either way silently corrupts the count for the whole vertex,
//   which is what the re-shoot sequence exists for.
fn segment_crosses_triangle(
    p: Vec3,
    q: Vec3,
    direction: Vec3,
    triangle: [Vec3; 3],
    uncertain: &mut usize,
) -> Crossing {
    let (a, b, c) = (triangle[0], triangle[1], triangle[2]);
    let Some(plane_p) = plane_side(triangle, p, direction, uncertain) else {
        return Crossing::Degenerate;
    };
    let Some(plane_q) = plane_side(triangle, q, direction, uncertain) else {
        return Crossing::Degenerate;
    };
    if plane_p == plane_q {
        return Crossing::Miss;
    }
    let mut sign_of = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| -> i8 {
        let (sign, certified, _, _) = orient3d_filtered(a, b, c, d);
        if !certified {
            *uncertain += 1;
        }
        sign
    };
    let s0 = sign_of(p, q, a, b);
    let s1 = sign_of(p, q, b, c);
    let s2 = sign_of(p, q, c, a);
    if s0 == 0 || s1 == 0 || s2 == 0 {
        return Crossing::Degenerate;
    }
    if s0 == s1 && s1 == s2 {
        Crossing::Hit
    } else {
        Crossing::Miss
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Parity of one point against one component's faces along one direction.
// Returns: Some(true/false) for inside/outside, or None when the ray hit a degenerate feature.
// Side effects: Counts filter escalations.
// Notes: The far endpoint is `p + reach * direction` with `reach` past the whole scene, so the
//   segment test *is* the ray test. Rounding that endpoint perturbs the direction by an ulp, which
//   is harmless - the direction only has to avoid degeneracies, and when it fails to, the caller
//   re-shoots rather than trusting it.
fn parity_along(
    point: Vec3,
    direction: Vec3,
    reach: f64,
    triangles: &[[Vec3; 3]],
    grid: &ProjectionGrid,
    uncertain: &mut usize,
) -> Option<bool> {
    let far = point.add(direction.scale(reach));
    let mut crossings = 0usize;
    for index in grid.candidates(point) {
        match segment_crosses_triangle(
            point,
            far,
            direction,
            triangles[*index as usize],
            uncertain,
        ) {
            Crossing::Hit => crossings += 1,
            Crossing::Miss => {}
            Crossing::Degenerate => return None,
        }
    }
    Some(crossings % 2 == 1)
}

// ---------------------------------------------------------------------------
// The stage
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: Inputs to S6; side effects: none.
#[derive(Debug, Clone)]
pub struct ClassifyOptions {
    pub domain_min: Vec3,
    pub domain_max: Vec3,
}

impl Default for ClassifyOptions {
    // AI-FUNC-SUMMARY: The unit domain; returns ClassifyOptions; side effects: none.
    fn default() -> Self {
        ClassifyOptions {
            domain_min: Vec3::new(0.0, 0.0, 0.0),
            domain_max: Vec3::new(1.0, 1.0, 1.0),
        }
    }
}

/// One component's faces, gathered once and shared by every vertex query.
struct ComponentGeometry {
    x: i32,
    triangles: Vec<[Vec3; 3]>,
    /// The same faces as `triangles`, kept in arranged form for the winding-number
    /// fallback, which needs the surface's own indexing.
    faces: Vec<ArrangedFace>,
    grids: Vec<ProjectionGrid>,
    defective: bool,
}

// AI-FUNC-SUMMARY:
// Purpose: The per-solid-component geometry S6 classifies against, kept so a later stage can ask
//   the same inside/outside question at a point the lattice has no vertex at.
// Notes: Built once and shared. S8 needs it for `SPEC_meshgen_geometry.md` §7.5 - a junction cell's
//   pieces are classified at an interior sample, because the pieces do not exist until S8 makes
//   them and so have no S6 answer to inherit. Before 2026-08-07 they inherited the parent's record
//   verbatim, and a straddling parent's record is `Ambiguous` for every component crossing it;
//   `resolve()` drops `Ambiguous` entries, so those pieces resolved to `{0}` and the material was
//   **lost to background**, not merely chamfered. Rebuilding this from scratch in S8 would double
//   the projection grids, hence one build shared by both stages.
pub struct PointClassifier {
    solids: Vec<ComponentGeometry>,
    /// Arranged-surface vertices, for the winding-number fallback's indexing.
    vertices: Vec<Vec3>,
    reach: f64,
    n_sheets: usize,
    n_defective: usize,
}

impl PointClassifier {
    // AI-FUNC-SUMMARY:
    // Purpose: Gather each closed solid component's faces and its five ray-projection grids.
    // Inputs: the clipped arranged surface, the S2b topology, and the domain.
    // Returns: PointClassifier.
    // Side effects: None.
    // Notes: Sheets are skipped - they never own volume (SPEC §9.1 row 10). A component is routed
    //   to the winding number when S2b could not certify its closure **or** when it intersects
    //   itself, because ray parity counts every shell crossing and flips on the extra faces.
    pub fn build(
        surface: &ArrangedSurface,
        topo: &RebuiltTopology,
        options: &ClassifyOptions,
    ) -> Self {
        // Which components meet *themselves*: an S2 intersection curve whose component set
        // is a single component is that component's own shells interpenetrating.
        let mut self_intersecting: std::collections::BTreeSet<i32> =
            std::collections::BTreeSet::new();
        for curve in &surface.curves {
            if curve.kind != crate::meshgen::arrange::ArrangedCurveKind::Intersection {
                continue;
            }
            let mut components: SmallVec<[i32; 2]> = curve.components.clone();
            components.sort_unstable();
            components.dedup();
            if components.len() == 1 {
                self_intersecting.insert(components[0]);
            }
        }
        // Coplanar self-overlap looks like it belongs here too - parity is equally undefined
        // on a coincident face pair, and S2 reports 144 overlays against 72 proper
        // intersections on the strut lattice. Extending the rule to any self-coincidence
        // event was tried (2026-08-06) and is **not** an improvement: the lattice is unchanged
        // at 8,680 (its overlays are already covered by the intersection rule, same
        // components) while a touching cube-and-limb goes 251 -> 324. Reverted.

        let mut solids: Vec<ComponentGeometry> = Vec::new();
        let mut n_sheets = 0usize;
        let mut n_defective = 0usize;
        for component in &surface.components {
            let classification = topo
                .classifications
                .get(&component.x)
                .copied()
                .unwrap_or(ComponentClassification::Sheet);
            match classification {
                ComponentClassification::Sheet => {
                    n_sheets += 1;
                    continue;
                }
                ComponentClassification::SolidDefective => n_defective += 1,
                ComponentClassification::SolidClosed => {}
            }
            let faces: Vec<ArrangedFace> = surface
                .faces
                .iter()
                .filter(|face| face.components.contains(&component.x))
                .cloned()
                .collect();
            let triangles: Vec<[Vec3; 3]> = faces
                .iter()
                .map(|face| {
                    [
                        surface.vertices[face.nodes[0]],
                        surface.vertices[face.nodes[1]],
                        surface.vertices[face.nodes[2]],
                    ]
                })
                .collect();
            if triangles.is_empty() {
                continue;
            }
            let grids: Vec<ProjectionGrid> = RAY_DIRECTIONS
                .iter()
                .map(|direction| {
                    ProjectionGrid::build(
                        &triangles,
                        Vec3::new(direction[0], direction[1], direction[2]),
                    )
                })
                .collect();
            solids.push(ComponentGeometry {
                x: component.x,
                triangles,
                faces,
                grids,
                // GWN is required for a **self-intersecting** component as well as an
                // uncertifiable one. Ray parity counts every shell crossing, so where a
                // component's own shells interpenetrate a ray picks up the extra faces and the
                // parity flips on them: measured on a lattice of 15 overlapping boxes, 202 of
                // 300 sampled edges carried an "Inside" verdict for a point outside the
                // component's own bounding box. S2b certifies such an input `1 closed, 0
                // defective` - true in the every-edge-used-twice sense and badly misleading -
                // so the existing gate never fired and the fast path answered nonsense.
                defective: classification == ComponentClassification::SolidDefective
                    || self_intersecting.contains(&component.x),
            });
        }
        let extent = options.domain_max.sub(options.domain_min);
        let reach = 4.0 * extent.dot(extent).sqrt().max(f64::MIN_POSITIVE);
        PointClassifier {
            solids,
            vertices: surface.vertices.clone(),
            reach,
            n_sheets,
            n_defective,
        }
    }

    // AI-FUNC-SUMMARY: How many solid components own a slot here; returns usize; side effects: none.
    pub fn slots(&self) -> usize {
        self.solids.len()
    }

    // AI-FUNC-SUMMARY: How many components S2b classified as sheets, for the S6 stats line; returns usize; side effects: none.
    pub fn n_sheets(&self) -> usize {
        self.n_sheets
    }

    // AI-FUNC-SUMMARY: How many solid components S2b could not certify closed, for the S6 stats line; returns usize; side effects: none.
    pub fn n_defective(&self) -> usize {
        self.n_defective
    }

    // AI-FUNC-SUMMARY: The component id at one slot; returns i32; side effects: none.
    pub fn component_at(&self, slot: usize) -> i32 {
        self.solids[slot].x
    }

    // AI-FUNC-SUMMARY: The slot a component id occupies, if it is a classified solid; returns Option<usize>; side effects: none.
    pub fn slot_of(&self, component: i32) -> Option<usize> {
        self.solids.iter().position(|solid| solid.x == component)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The arranged triangles of the solid at `slot` - the surface itself, not a
    //   question about it.
    // Inputs: the slot.
    // Returns: the triangles, empty when the slot is out of range.
    // Side effects: None.
    // Notes: `inside` answers "which side of this surface is the point on"; S8's spoke cut needs
    //   "*where* does this surface cross this segment", which no parity test can give. The
    //   geometry is already gathered and shared, so exposing it costs nothing and rebuilding it
    //   in S8 would be a second copy of the same arranged faces.
    pub fn triangles(&self, slot: usize) -> &[[Vec3; 3]] {
        self.solids
            .get(slot)
            .map(|solid| solid.triangles.as_slice())
            .unwrap_or(&[])
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Whether `point` lies inside the solid component at `slot`.
    // Inputs: the point, the slot, and a counter for filter escalations.
    // Returns: bool.
    // Side effects: increments `uncertain` per exact-predicate escalation.
    // Notes: The five-ray sequence of PLAN §10.5 with the winding number as the fallback, which is
    //   the same decision procedure S6 runs per lattice vertex - deliberately the same code, so a
    //   piece sampled here and a vertex classified there cannot disagree about the same point.
    pub fn inside(&self, point: Vec3, slot: usize, uncertain: &mut usize) -> bool {
        let solid = &self.solids[slot];
        if solid.defective {
            return winding_inside(point, &solid.faces, &self.vertices);
        }
        for (attempt, direction) in RAY_DIRECTIONS.iter().enumerate() {
            let direction = Vec3::new(direction[0], direction[1], direction[2]);
            if let Some(inside) = parity_along(
                point,
                direction,
                self.reach,
                &solid.triangles,
                &solid.grids[attempt],
                uncertain,
            ) {
                return inside;
            }
        }
        winding_inside(point, &solid.faces, &self.vertices)
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Run S6 - classify every lattice vertex against every solid component, seed the ownership
//   records, resolve preliminary region keys, and mark inactive surface patches.
// Inputs: the S5 lattice, the clipped arranged surface, the S2b topology, and the domain.
// Returns: Classification.
// Side effects: None (warnings are collected into the result, not printed).
// Notes: Parallel over vertices in the permitted shape (PLAN §12.5) - each vertex writes its own
//   slice of a pre-sized buffer and reads only shared immutable geometry, so nothing depends on
//   scheduling. The per-vertex filter-escalation counts are summed by a serial fold afterwards for
//   the same reason.
pub fn classify_lattice(
    lattice: &Lattice,
    surface: &ArrangedSurface,
    topo: &RebuiltTopology,
    options: &ClassifyOptions,
) -> Classification {
    let classifier = PointClassifier::build(surface, topo, options);
    classify_lattice_with(lattice, &classifier, surface, options)
}

// AI-FUNC-SUMMARY:
// Purpose: `classify_lattice` against a `PointClassifier` the caller already built.
// Inputs: the S5 lattice, the shared classifier, the clipped arranged surface, and the domain.
// Returns: Classification.
// Side effects: None (warnings are collected into the result, not printed).
// Notes: The pipeline uses this form because S8 needs the same classifier (§7.5) and building the
//   projection grids twice is pure waste. `classify_lattice` is the four-argument wrapper.
pub fn classify_lattice_with(
    lattice: &Lattice,
    classifier: &PointClassifier,
    surface: &ArrangedSurface,
    options: &ClassifyOptions,
) -> Classification {
    let priority_of: BTreeMap<i32, u32> = surface
        .components
        .iter()
        .map(|component| (component.x, component.priority))
        .collect();
    let kind_of: BTreeMap<i32, ArrangeComponent> = surface
        .components
        .iter()
        .map(|component| (component.x, *component))
        .collect();

    // Only solids may own volume; a sheet never enters `S` (SPEC §9.1 row 10).
    let solids = &classifier.solids;
    let n_sheets = classifier.n_sheets();
    let n_defective = classifier.n_defective();

    let extent = options.domain_max.sub(options.domain_min);
    let reach = 4.0 * extent.dot(extent).sqrt().max(f64::MIN_POSITIVE);
    let slots = solids.len();
    let mut vertex_inside = vec![false; lattice.nodes.len() * slots.max(1)];

    // Per-vertex work into disjoint slots; the diagnostics come back as an
    // indexed buffer and are folded serially.
    let per_vertex: Vec<(usize, usize, usize, usize, usize)> = if slots == 0 {
        Vec::new()
    } else {
        vertex_inside
            .par_chunks_mut(slots)
            .zip(lattice.nodes.par_iter())
            .map(|(row, point)| {
                let mut first = 0usize;
                let mut reshoots = 0usize;
                let mut defective = 0usize;
                let mut fallbacks = 0usize;
                let mut uncertain = 0usize;
                for (slot, solid) in solids.iter().enumerate() {
                    if solid.defective {
                        // S2b could not certify this component's closure, so parity
                        // is not defined for it; the winding number is (§10.4).
                        row[slot] = winding_inside(*point, &solid.faces, &surface.vertices);
                        defective += 1;
                        continue;
                    }
                    let mut decided = None;
                    for (attempt, direction) in RAY_DIRECTIONS.iter().enumerate() {
                        let direction = Vec3::new(direction[0], direction[1], direction[2]);
                        if let Some(inside) = parity_along(
                            *point,
                            direction,
                            reach,
                            &solid.triangles,
                            &solid.grids[attempt],
                            &mut uncertain,
                        ) {
                            decided = Some(inside);
                            if attempt == 0 {
                                first += 1;
                            } else {
                                reshoots += 1;
                            }
                            break;
                        }
                    }
                    row[slot] = match decided {
                        Some(inside) => inside,
                        None => {
                            fallbacks += 1;
                            winding_inside(*point, &solid.faces, &surface.vertices)
                        }
                    };
                }
                (first, reshoots, defective, fallbacks, uncertain)
            })
            .collect()
    };

    let mut stats = ClassifyStats {
        n_vertices: lattice.nodes.len(),
        n_solid_components: slots,
        n_sheet_components: n_sheets,
        n_defective_components: n_defective,
        ..Default::default()
    };
    for (first, reshoots, defective, fallbacks, uncertain) in per_vertex {
        stats.n_first_ray += first;
        stats.n_reshoots += reshoots;
        stats.n_defective_gwn += defective;
        stats.n_gwn_fallback += fallbacks;
        stats.n_filter_uncertain += uncertain;
    }
    stats.n_inside_pairs = vertex_inside.iter().filter(|inside| **inside).count();

    let mut warnings = Vec::new();
    if stats.n_reshoots > 0 {
        warnings.push(format!(
            "[CLS-RESHOOT] {} vertex-component decision(s) needed a re-shoot after a ray hit a degenerate feature",
            stats.n_reshoots
        ));
    }
    if stats.n_gwn_fallback > 0 {
        warnings.push(format!(
            "[CLS-BAND] {} vertex-component decision(s) exhausted the ray sequence and fell through to the winding number",
            stats.n_gwn_fallback
        ));
    }

    // Seed one record per tet (`from_lattice`): a component owns the tet when all
    // four vertices are inside it, is absent when all four are outside, and is
    // Ambiguous when the cell straddles the surface - which is exactly the set of
    // cells S8 has to cut.
    let records: Vec<OwnershipRecord> = lattice
        .tets
        .par_iter()
        .map(|tet| {
            let mut entries: SmallVec<[(i32, Side); 2]> = SmallVec::new();
            for (slot, solid) in solids.iter().enumerate() {
                let inside = tet
                    .iter()
                    .filter(|node| vertex_inside[**node as usize * slots + slot])
                    .count();
                let side = match inside {
                    0 => continue,
                    4 => Side::Inside,
                    _ => Side::Ambiguous,
                };
                entries.push((solid.x, side));
            }
            OwnershipRecord {
                entries,
                provenance: Provenance::Lattice,
            }
        })
        .collect();

    for record in &records {
        if record.is_ambiguous() {
            stats.n_tets_ambiguous += 1;
        } else if record.entries.is_empty() {
            stats.n_tets_background += 1;
        } else {
            stats.n_tets_owned += 1;
        }
    }

    // Preliminary region keys. Collected as a canonical set so the contract's
    // region-set table has one row per distinct key.
    let keys: Vec<Vec<i32>> = records
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
    let region_key: Vec<i32> = keys.iter().map(|key| index_of[key]).collect();

    // Active-patch filter (PLAN §5.2): a face strictly inside a strictly
    // higher-priority solid has the same resolved label on both sides, so it
    // separates nothing and is skipped by refinement, snap and cut. A sheet is
    // always active - it is a feature in its own right, not a material boundary.
    let active_face: Vec<bool> = surface
        .faces
        .par_iter()
        .map(|face| {
            let own_priority = face
                .components
                .iter()
                .filter_map(|x| priority_of.get(x).copied())
                .min();
            let is_sheet = face
                .components
                .iter()
                .any(|x| kind_of.get(x).map(|c| c.kind == 1).unwrap_or(false));
            if is_sheet {
                return true;
            }
            let Some(own_priority) = own_priority else {
                return true;
            };
            let centroid = surface.vertices[face.nodes[0]]
                .add(surface.vertices[face.nodes[1]])
                .add(surface.vertices[face.nodes[2]])
                .scale(1.0 / 3.0);
            let mut uncertain = 0usize;
            for solid in solids {
                if face.components.contains(&solid.x) {
                    continue;
                }
                let Some(other) = priority_of.get(&solid.x).copied() else {
                    continue;
                };
                if other >= own_priority {
                    continue;
                }
                let inside = if solid.defective {
                    winding_inside(centroid, &solid.faces, &surface.vertices)
                } else {
                    RAY_DIRECTIONS
                        .iter()
                        .enumerate()
                        .find_map(|(attempt, direction)| {
                            parity_along(
                                centroid,
                                Vec3::new(direction[0], direction[1], direction[2]),
                                reach,
                                &solid.triangles,
                                &solid.grids[attempt],
                                &mut uncertain,
                            )
                        })
                        .unwrap_or_else(|| {
                            winding_inside(centroid, &solid.faces, &surface.vertices)
                        })
                };
                if inside {
                    return false;
                }
            }
            // A face buried in its **own** component's overlap separates nothing either,
            // and until 2026-08-07 nothing suppressed it: the loop above skips a face's
            // own components by construction, so only a *higher-priority other* solid
            // could deactivate one. A self-intersecting input has interior triangles by
            // definition - A-8's struts overlap at every junction - and S7 dutifully found
            // crossings on them while S6, labelling by winding number, correctly reported
            // both endpoints of the edge inside. That mismatch is `[S67-SITE]
            // no-ambiguous-component`, **2,184 of A-8's 2,478 escalations**, every one of
            // them fanned and chamfered: the 4.15 % it loses.
            //
            // The union's boundary is where insideness changes, so the test is exactly
            // that: step off both faces of the triangle and ask whether the component
            // contains both. `winding_inside` is the right instrument here - it is what
            // S6 already trusts on a defective component, and it varies smoothly enough
            // near the surface for a small offset to be decisive. A clean closed solid
            // never trips this, because one side of every boundary face is outside it.
            for solid in solids {
                if !solid.defective || !face.components.contains(&solid.x) {
                    continue;
                }
                let edge = surface.vertices[face.nodes[1]].sub(surface.vertices[face.nodes[0]]);
                let other = surface.vertices[face.nodes[2]].sub(surface.vertices[face.nodes[0]]);
                let normal = edge.cross(other);
                let scale = normal.dot(normal).sqrt();
                if scale <= 0.0 {
                    continue;
                }
                let step = normal.scale(ACTIVE_FACE_PROBE / scale.sqrt());
                // Sample the whole triangle, not just its centroid. S2 does **not**
                // corefine a component against itself, so a self-intersecting solid's
                // triangle can be partly buried and partly on the union boundary; a
                // single centroid probe would deactivate the whole of it and throw the
                // boundary part away. Requiring every sample to agree keeps a mixed
                // triangle active, which is the conservative direction - cutting on a
                // partly-interior face costs an escalation, dropping a boundary face
                // costs material.
                const BARYCENTRIC: [[f64; 3]; 4] = [
                    [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
                    [0.6, 0.2, 0.2],
                    [0.2, 0.6, 0.2],
                    [0.2, 0.2, 0.6],
                ];
                let buried = BARYCENTRIC.iter().all(|weights| {
                    let sample = surface.vertices[face.nodes[0]]
                        .scale(weights[0])
                        .add(surface.vertices[face.nodes[1]].scale(weights[1]))
                        .add(surface.vertices[face.nodes[2]].scale(weights[2]));
                    winding_inside(sample.add(step), &solid.faces, &surface.vertices)
                        && winding_inside(sample.sub(step), &solid.faces, &surface.vertices)
                });
                if buried {
                    return false;
                }
            }
            true
        })
        .collect();
    stats.n_inactive_faces = active_face.iter().filter(|active| !**active).count();

    Classification {
        solid_components: solids.iter().map(|solid| solid.x).collect(),
        vertex_inside,
        records,
        region_sets,
        region_key,
        active_face,
        warnings,
        stats,
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The winding-number fallback (PLAN §10.4, ARB-8/ARB-9).
// Returns: whether the point is inside the (possibly defective) component.
// Side effects: None.
// Notes: Used for a component S2b could not certify closed, and for a vertex whose whole ray
//   sequence degenerated. It is a *classifier*, not a predicate - `atan2` is transcendental - which
//   is why it is a fallback and never the primary path.
fn winding_inside(point: Vec3, faces: &[ArrangedFace], vertices: &[Vec3]) -> bool {
    let (w, _) = generalized_winding_number(point, faces, vertices);
    w.abs() > 0.5
}

// ---------------------------------------------------------------------------
// s06_classified
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Encode the classified lattice as the `s06_classified` snapshot VTU.
// Inputs: the lattice, the classification, and the run's components.
// Returns: VtuDoc of `VTK_TETRA` cells carrying `region_key`, `provenance` and the region-set table.
// Side effects: None.
// Notes: The region key is **preliminary** - a tet still straddling a surface resolves to what its
//   definite entries say, and S8's cut is what settles it. `RegionSetPriority` carries the single
//   `Y` every X in a key shares, with `0xFFFFFFFF` for background, which is what [V6] checks.
pub fn classified_to_doc(
    lattice: &Lattice,
    classification: &Classification,
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
    doc.cell_data.push(DataArray::scalar(
        "arbitrated",
        ArrayData::U8(
            classification
                .records
                .iter()
                .map(|record| u8::from(record.is_ambiguous()))
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
        ArrayData::U8(vec![0; points]),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_ref",
        ArrayData::I32(vec![-1; points]),
    ));

    let mut offsets: Vec<i64> = Vec::with_capacity(classification.region_sets.len());
    let mut flat: Vec<i32> = Vec::new();
    let mut priorities: Vec<u32> = Vec::with_capacity(classification.region_sets.len());
    for key in &classification.region_sets {
        flat.extend(key.iter().copied());
        offsets.push(flat.len() as i64);
        // Background carries the "not applicable" priority; every other key's X
        // values share one Y, which is what [V6] enforces.
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
