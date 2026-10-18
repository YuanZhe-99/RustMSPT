//! S9 (plan M-5.4): quality improvement of the cut mesh without moving its geometry.
//!
//! The cut leaves needles - a short edge between a cut node and a lattice node a few percent of an
//! element away, or two cut nodes close together on one trace - and the owner's FEM gate (minimum
//! dihedral 10 deg, aspect ratio below 10) fails on them. Every one measured so far is removable by
//! collapsing its short edge, and the collapse is allowed only where it cannot move the geometry:
//! a node is classified by the surfaces, curves and box planes it lies on, and `u` collapses onto
//! `v` only if `v` lies on every one of them, the surfaces through `u` are locally flat, and a curve
//! through `u` is straight there with `uv` one of its edges. Each collapse must keep every tet
//! positively oriented, make the worst aspect ratio of `u`'s star strictly better, leave no face on
//! three tets, and keep "this face separates two materials or partitions" equal to "this face is
//! tagged". Serial and in a fixed order, so R-P2 holds by construction.

use crate::io::vtu::{ArrayData, DataArray, VtuDoc};
use crate::meshgen::predicates::orient3d;
use crate::types::Vec3;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

const VTK_TRIANGLE: u8 = 5;
const VTK_TETRA: u8 = 10;
const VTK_LINE: u8 = 3;
const VTK_POLY_LINE: u8 = 4;

/// What S9 is asked to reach.
#[derive(Debug, Clone, Copy)]
pub struct ImproveOptions {
    /// A tet is bad at or above this aspect ratio (longest edge over shortest altitude, scaled so
    /// a regular tet reads 1).
    pub ar_target: f64,
    /// ... or below this minimum dihedral angle, in degrees.
    pub min_dihedral_deg: f64,
    /// Passes over the bad tets; a pass that collapses nothing ends the loop early.
    pub max_passes: usize,
    /// The weld quantum in the document's units (Invariant M1, `0.1 eps`): four surface points
    /// within it of one plane are one facet, as `merge_coplanar` already treated them. 0 uses the
    /// arithmetic tolerance only.
    pub weld: f64,
}

impl Default for ImproveOptions {
    // AI-FUNC-SUMMARY: The owner's FEM gate (AR < 10, dihedral >= 10 deg) and six passes; returns ImproveOptions; side effects: none.
    fn default() -> Self {
        Self { ar_target: 10.0, min_dihedral_deg: 10.0, max_passes: 6, weld: 0.0 }
    }
}

/// What S9 did.
#[derive(Debug, Clone, Default)]
pub struct ImproveStats {
    pub tets_before: usize,
    pub tets_after: usize,
    pub bad_before: usize,
    pub bad_after: usize,
    pub collapses: usize,
    pub smoothed: usize,
    pub flips: usize,
    pub passes: usize,
    pub worst_ar_before: f64,
    pub worst_ar_after: f64,
}

// AI-FUNC-SUMMARY: Aspect ratio of a tet, longest edge over shortest altitude scaled so a regular tet is 1 (the Abaqus convention the owner's gate uses); returns f64, infinity when degenerate; side effects: none.
pub fn aspect_ratio(p: [Vec3; 4]) -> f64 {
    let v6 = p[1].sub(p[0]).dot(p[2].sub(p[0]).cross(p[3].sub(p[0]))).abs();
    if v6 <= 0.0 {
        return f64::INFINITY;
    }
    let mut longest = 0.0f64;
    for a in 0..4 {
        for b in (a + 1)..4 {
            let d = p[b].sub(p[a]);
            longest = longest.max(d.dot(d));
        }
    }
    let mut largest_area2 = 0.0f64;
    for skip in 0..4 {
        let f: Vec<Vec3> = (0..4).filter(|k| *k != skip).map(|k| p[k]).collect();
        let n = f[1].sub(f[0]).cross(f[2].sub(f[0]));
        largest_area2 = largest_area2.max(n.dot(n));
    }
    // shortest altitude = 3V / largest area = v6 / |n|max
    let altitude = v6 / largest_area2.sqrt();
    longest.sqrt() / altitude * (2.0f64 / 3.0).sqrt()
}

// AI-FUNC-SUMMARY: Smallest dihedral angle of a tet in degrees; returns f64 (0 when degenerate); side effects: none.
pub fn min_dihedral_deg(p: [Vec3; 4]) -> f64 {
    let mut best = 180.0f64;
    for (a, b) in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
        let others: Vec<usize> = (0..4).filter(|k| *k != a && *k != b).collect();
        let (c, d) = (others[0], others[1]);
        let e = p[b].sub(p[a]);
        let n1 = e.cross(p[c].sub(p[a]));
        let n2 = e.cross(p[d].sub(p[a]));
        let l = (n1.dot(n1) * n2.dot(n2)).sqrt();
        if l <= 0.0 {
            return 0.0;
        }
        best = best.min((n1.dot(n2) / l).clamp(-1.0, 1.0).acos().to_degrees());
    }
    best
}

fn sorted3(mut f: [u32; 3]) -> [u32; 3] {
    f.sort_unstable();
    f
}

fn sorted4(mut t: [u32; 4]) -> [u32; 4] {
    t.sort_unstable();
    t
}

const TET_FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];

fn i32_cells(doc: &VtuDoc, name: &str) -> Option<Vec<i32>> {
    match &doc.cell_array(name)?.data {
        ArrayData::I32(v) => Some(v.clone()),
        ArrayData::I64(v) => Some(v.iter().map(|x| *x as i32).collect()),
        _ => None,
    }
}

fn field<'a>(doc: &'a VtuDoc, name: &str) -> Option<&'a ArrayData> {
    doc.field_data.iter().find(|a| a.name == name).map(|a| &a.data)
}

fn as_i64(data: &ArrayData) -> Vec<i64> {
    match data {
        ArrayData::I64(v) => v.clone(),
        ArrayData::I32(v) => v.iter().map(|x| *x as i64).collect(),
        ArrayData::U8(v) => v.iter().map(|x| *x as i64).collect(),
        ArrayData::U32(v) => v.iter().map(|x| *x as i64).collect(),
        ArrayData::U64(v) => v.iter().map(|x| *x as i64).collect(),
        ArrayData::F32(v) => v.iter().map(|x| *x as i64).collect(),
        ArrayData::F64(v) => v.iter().map(|x| *x as i64).collect(),
    }
}

struct Mesh {
    points: Vec<Vec3>,
    tets: Vec<[u32; 4]>,
    alive: Vec<bool>,
    /// region_key and partition_id per tet: a face between two tets that differ in either is a
    /// boundary the mesh must declare.
    material: Vec<(i32, i32)>,
    /// The document cell each tet takes its per-cell row from (itself, or the tet a flip replaced).
    source: Vec<usize>,
    node_tets: Vec<Vec<u32>>,
    /// Tagged faces by sorted node triple -> index into `faces`.
    faces: Vec<[u32; 3]>,
    face_alive: Vec<bool>,
    face_index: BTreeMap<[u32; 3], usize>,
    node_faces: Vec<Vec<u32>>,
    /// Surface identity per tagged face (component set + kind, interned).
    face_surface: Vec<u32>,
    curves: Vec<Vec<u32>>,
    /// Per node: the surfaces (tagged-face identities and box planes, the latter offset by
    /// `BOX_BASE`) and the curves it lies on.
    surfaces: Vec<BTreeSet<u32>>,
    curve_ids: Vec<BTreeSet<i32>>,
    /// Curve edges (sorted) -> curve id.
    curve_edges: BTreeMap<[u32; 2], i32>,
    tol: f64,
    approx: Option<Approx>,
    /// Print-only (`RUSTMSPT_S9_DIAG`): how often each refusal site fired, keyed by source line
    /// (9xx: the guards of `try_unfold`, in order).
    refusals: std::cell::RefCell<BTreeMap<usize, usize>>,
    /// `ImproveOptions::weld`, at least `tol`: the coplanarity bound of `try_unfold`.
    weld: f64,
    /// Per tet slot: its aspect ratio and whether it misses the gate, valid where `fresh` is set.
    /// Every write to a slot's nodes or to a node's position clears `fresh` for the tets it
    /// touches, so `bad_list` recomputes only what changed and returns what a full scan would.
    quality: Vec<(f64, bool)>,
    fresh: Vec<bool>,
}

const BOX_BASE: u32 = u32::MAX - 16;

// AI-FUNC-SUMMARY: Closest point to `p` on triangle (a, b, c) (Ericson's region test); returns Vec3; side effects: none.
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b.sub(a), c.sub(a), p.sub(a));
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p.sub(b);
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a.add(ab.scale(d1 / (d1 - d3)));
    }
    let cp = p.sub(c);
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a.add(ac.scale(d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b.add(c.sub(b).scale((d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    a.add(ab.scale(vb * denom)).add(ac.scale(vc * denom))
}

/// An input surface (world frame) bucketed on a uniform grid, for S9's approximate mode: how far a
/// point is from it, and where its nearest point is.
pub struct InputSurface {
    tris: Vec<[Vec3; 3]>,
    cell: f64,
    grid: BTreeMap<(i64, i64, i64), Vec<u32>>,
}

impl InputSurface {
    // AI-FUNC-SUMMARY: Bucket triangles on a grid of cell `max(mean edge, tolerance)`; returns InputSurface; side effects: none.
    pub fn new(tris: Vec<[Vec3; 3]>, tolerance: f64) -> Self {
        let mut mean = 0.0;
        for t in &tris {
            for k in 0..3 {
                let d = t[(k + 1) % 3].sub(t[k]);
                mean += d.dot(d).sqrt();
            }
        }
        let cell = (mean / (3.0 * tris.len().max(1) as f64)).max(tolerance).max(f64::MIN_POSITIVE);
        let mut grid: BTreeMap<(i64, i64, i64), Vec<u32>> = BTreeMap::new();
        for (i, t) in tris.iter().enumerate() {
            let lo = |k: usize| t.iter().map(|p| [p.x, p.y, p.z][k]).fold(f64::INFINITY, f64::min);
            let hi = |k: usize| t.iter().map(|p| [p.x, p.y, p.z][k]).fold(f64::NEG_INFINITY, f64::max);
            let r = |k: usize| ((lo(k) / cell).floor() as i64, (hi(k) / cell).floor() as i64);
            let (x, y, z) = (r(0), r(1), r(2));
            for a in x.0..=x.1 {
                for b in y.0..=y.1 {
                    for c in z.0..=z.1 {
                        grid.entry((a, b, c)).or_default().push(i as u32);
                    }
                }
            }
        }
        Self { tris, cell, grid }
    }

    // AI-FUNC-SUMMARY: The nearest point of the surface within `radius` of `p`, with its distance; returns Option<(Vec3, f64)>; side effects: none.
    pub fn nearest(&self, p: Vec3, radius: f64) -> Option<(Vec3, f64)> {
        let span = (radius / self.cell).ceil() as i64;
        let c = ((p.x / self.cell).floor() as i64, (p.y / self.cell).floor() as i64, (p.z / self.cell).floor() as i64);
        let mut best: Option<(Vec3, f64)> = None;
        let mut seen: BTreeSet<u32> = BTreeSet::new();
        for a in (c.0 - span)..=(c.0 + span) {
            for b in (c.1 - span)..=(c.1 + span) {
                for z in (c.2 - span)..=(c.2 + span) {
                    for i in self.grid.get(&(a, b, z)).into_iter().flatten() {
                        if !seen.insert(*i) {
                            continue;
                        }
                        let [x, y, w] = self.tris[*i as usize];
                        let q = closest_on_triangle(p, x, y, w);
                        let d = q.sub(p).dot(q.sub(p)).sqrt();
                        if d <= radius && best.is_none_or(|(_, bd)| d < bd) {
                            best = Some((q, d));
                        }
                    }
                }
            }
        }
        best
    }
}

/// S9's approximate mode: smooth surface parts may move within `tol` of the input.
struct Approx {
    tol: f64,
    feature_cos: f64,
    by_surface: BTreeMap<u32, InputSurface>,
}

impl Mesh {
    fn refused(&self, line: usize) {
        *self.refusals.borrow_mut().entry(line).or_insert(0) += 1;
    }

    fn tet_points(&self, t: [u32; 4]) -> [Vec3; 4] {
        t.map(|n| self.points[n as usize])
    }

    // AI-FUNC-SUMMARY: Mark one tet slot's cached quality stale; returns nothing; side effects: clears `fresh[slot]`.
    fn stale(&mut self, slot: u32) {
        if let Some(f) = self.fresh.get_mut(slot as usize) {
            *f = false;
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The alive tets missing the gate, recomputing only the slots a move has touched since
    //   the last call.
    // Inputs: the gate.
    // Returns: (aspect ratio, tet) for every alive tet missing the gate, ascending by tet - exactly
    //   what a full scan of every alive tet returns.
    // Side effects: refreshes `quality`/`fresh`.
    // Notes: The stale slots are evaluated in parallel (`aspect_ratio`, `min_dihedral_deg`).
    fn bad_list(&mut self, opts: &ImproveOptions) -> Vec<(f64, u32)> {
        let n = self.tets.len();
        self.quality.resize(n, (0.0, false));
        self.fresh.resize(n, false);
        let stale: Vec<u32> = (0..n as u32)
            .filter(|i| self.alive[*i as usize] && !self.fresh[*i as usize])
            .collect();
        let (points, tets) = (&self.points, &self.tets);
        let computed: Vec<(f64, bool)> = stale
            .par_iter()
            .map(|i| {
                let p = tets[*i as usize].map(|x| points[x as usize]);
                let ar = aspect_ratio(p);
                (ar, ar >= opts.ar_target || min_dihedral_deg(p) < opts.min_dihedral_deg)
            })
            .collect();
        for (i, q) in stale.iter().zip(computed) {
            self.quality[*i as usize] = q;
            self.fresh[*i as usize] = true;
        }
        (0..n)
            .filter(|i| self.alive[*i] && self.quality[*i].1)
            .map(|i| (self.quality[i].0, i as u32))
            .collect()
    }

    // AI-FUNC-SUMMARY: The worst aspect ratio over alive tets from the cache `bad_list` keeps fresh; returns f64 (0 when empty); side effects: none.
    fn worst_ar(&self) -> f64 {
        (0..self.tets.len())
            .filter(|i| self.alive[*i])
            .map(|i| self.quality[i].0)
            .fold(0.0f64, f64::max)
    }

    fn bad(&self, t: [u32; 4], opts: &ImproveOptions) -> bool {
        let p = self.tet_points(t);
        aspect_ratio(p) >= opts.ar_target || min_dihedral_deg(p) < opts.min_dihedral_deg
    }

    // Tets (alive) containing all of `nodes`.
    fn tets_with(&self, nodes: &[u32]) -> Vec<u32> {
        self.node_tets[nodes[0] as usize]
            .iter()
            .copied()
            .filter(|t| self.alive[*t as usize] && nodes.iter().all(|n| self.tets[*t as usize].contains(n)))
            .collect()
    }

    // The distinct planes the faces of `u` tagged with surface `s` lie in - one where the surface
    // is flat at `u`, two along a crease of the input's faceting (a faceted sphere's edges), where
    // `u` may still slide along the crease line without moving the surface. None at three or
    // more (an input vertex, a corner): `u` is pinned.
    fn surface_planes(&self, u: u32, s: u32) -> Option<Vec<Vec3>> {
        if s >= BOX_BASE {
            let axis = ((s - BOX_BASE) / 2) as usize;
            return Some(vec![[Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)][axis]]);
        }
        let origin = self.points[u as usize];
        let mut planes: Vec<Vec3> = Vec::new();
        for f in &self.node_faces[u as usize] {
            let f = *f as usize;
            if !self.face_alive[f] || self.face_surface[f] != s {
                continue;
            }
            let [a, b, c] = self.faces[f].map(|n| self.points[n as usize]);
            let n = b.sub(a).cross(c.sub(a));
            let l = n.dot(n).sqrt();
            if l <= 0.0 {
                return None;
            }
            let n = n.scale(1.0 / l);
            // u is a vertex of the face, so every plane passes through it; a plane is new when
            // its normal differs or the face's far corners leave the matched plane.
            let matched = planes.iter().any(|m| {
                m.dot(n).abs() >= 1.0 - 1.0e-12
                    && [a, b, c].iter().all(|q| m.dot(q.sub(origin)).abs() <= self.tol)
            });
            if !matched {
                planes.push(n);
                if planes.len() > 2 {
                    return None;
                }
            }
        }
        Some(planes)
    }

    // Whether `uv` is an edge of a tagged face of surface `s`.
    fn surface_edge(&self, u: u32, v: u32, s: u32) -> bool {
        self.node_faces[u as usize].iter().any(|f| {
            let f = *f as usize;
            self.face_alive[f] && self.face_surface[f] == s && self.faces[f].contains(&v)
        })
    }

    // Approximate mode only: the faces of `u` on surface `s` all face within the feature angle of
    // one another, so `s` is smooth at `u` and `u` may slide within the tolerance.
    fn smooth_at(&self, u: u32, s: u32) -> bool {
        let Some(approx) = &self.approx else { return false };
        if s >= BOX_BASE || !approx.by_surface.contains_key(&s) {
            return false;
        }
        let normals: Vec<Vec3> = self.node_faces[u as usize]
            .iter()
            .map(|f| *f as usize)
            .filter(|f| self.face_alive[*f] && self.face_surface[*f] == s)
            .filter_map(|f| {
                let [a, b, c] = self.faces[f].map(|n| self.points[n as usize]);
                let n = b.sub(a).cross(c.sub(a));
                let l = n.dot(n).sqrt();
                (l > 0.0).then(|| n.scale(1.0 / l))
            })
            .collect();
        !normals.is_empty()
            && normals.iter().all(|n| normals.iter().all(|m| n.dot(*m).abs() >= approx.feature_cos))
    }

    // Approximate mode: every sample of face (a, b, c) - corners, edge midpoints, centroid - within
    // the tolerance of surface `s` of the input.
    fn face_within(&self, s: u32, f: [Vec3; 3]) -> bool {
        let Some(approx) = &self.approx else { return false };
        let Some(input) = approx.by_surface.get(&s) else { return false };
        let samples = [
            f[0],
            f[1],
            f[2],
            f[0].add(f[1]).scale(0.5),
            f[1].add(f[2]).scale(0.5),
            f[2].add(f[0]).scale(0.5),
            f[0].add(f[1]).add(f[2]).scale(1.0 / 3.0),
        ];
        samples.iter().all(|q| input.nearest(*q, approx.tol).is_some())
    }

    fn allowed(&self, u: u32, v: u32) -> bool {
        let (su, sv) = (&self.surfaces[u as usize], &self.surfaces[v as usize]);
        if !su.is_subset(sv) || !self.curve_ids[u as usize].is_subset(&self.curve_ids[v as usize]) {
            { self.refused(226); return false; }
        }
        let (pu, pv) = (self.points[u as usize], self.points[v as usize]);
        for s in su {
            // Approximate mode: on a smooth surface the facet planes do not bind - u may go to any
            // neighbour on it, and `try_collapse` holds the faces it drags to the input.
            if self.smooth_at(u, *s) {
                if !self.surface_edge(u, v, *s) {
                    self.refused(234);
                    return false;
                }
                continue;
            }
            let Some(planes) = self.surface_planes(u, *s) else {
                // Approximate mode: a smooth surface lets u go to any neighbour on it; the faces
                // it drags along are checked against the input in `try_collapse`.
                if self.smooth_at(u, *s) && self.surface_edge(u, v, *s) {
                    continue;
                }
                self.refused(230);
                return false;
            };
            // v must lie in every plane u is held to, so sliding u to it moves no surface.
            if planes.iter().any(|n| n.dot(pv.sub(pu)).abs() > self.tol) {
                self.refused(231);
                return false;
            }
            if *s < BOX_BASE && !self.surface_edge(u, v, *s) {
                { self.refused(233); return false; }
            }
        }
        // A curve through u: uv must be one of its edges and the curve straight at u.
        for c in &self.curve_ids[u as usize] {
            let key = if u < v { [u, v] } else { [v, u] };
            if self.curve_edges.get(&key) != Some(c) {
                { self.refused(240); return false; }
            }
            let neighbours: Vec<u32> = self
                .curve_edges
                .iter()
                .filter(|(e, id)| *id == c && e.contains(&u))
                .map(|(e, _)| if e[0] == u { e[1] } else { e[0] })
                .collect();
            if neighbours.len() != 2 {
                { self.refused(249); return false; }
            }
            let (a, b, p) = (
                self.points[neighbours[0] as usize],
                self.points[neighbours[1] as usize],
                self.points[u as usize],
            );
            let d = b.sub(a);
            let l = d.dot(d).sqrt();
            if l <= 0.0 || d.cross(p.sub(a)).dot(d.cross(p.sub(a))).sqrt() / l > self.tol {
                { self.refused(259); return false; }
            }
        }
        true
    }

    // The planes a node must stay in: each flat surface through it, and each box plane. None
    // when it may not move at all (a curve node, a node on a curved surface).
    fn constraint_planes(&self, u: u32) -> Option<(Vec<Vec3>, Option<u32>)> {
        if !self.curve_ids[u as usize].is_empty() {
            return None;
        }
        let mut normals: Vec<Vec3> = Vec::new();
        let mut smooth: Option<u32> = None;
        for s in &self.surfaces[u as usize] {
            if self.smooth_at(u, *s) {
                if smooth.is_some() {
                    return None;
                }
                smooth = Some(*s);
                continue;
            }
            normals.extend(self.surface_planes(u, *s)?);
        }
        // A smooth surface is followed by projection, which knows nothing of other planes.
        if smooth.is_some() && !normals.is_empty() {
            return None;
        }
        Some((normals, smooth))
    }

    // Project a displacement onto the space the constraint planes leave free (Gram-Schmidt on the
    // normals, then remove each component). Returns None when nothing is free.
    fn project(normals: &[Vec3], d: Vec3) -> Option<Vec3> {
        let mut basis: Vec<Vec3> = Vec::new();
        for n in normals {
            let mut m = *n;
            for b in &basis {
                m = m.sub(b.scale(m.dot(*b)));
            }
            let l = m.dot(m).sqrt();
            if l > 1.0e-9 {
                basis.push(m.scale(1.0 / l));
            }
        }
        if basis.len() >= 3 {
            return None;
        }
        let mut out = d;
        for b in &basis {
            out = out.sub(b.scale(out.dot(*b)));
        }
        Some(out)
    }

    // Move u towards the mean of its neighbours (within its constraint planes) when that makes the
    // worst aspect ratio of its star strictly better and keeps every tet positive by a margin.
    fn try_smooth(&mut self, u: u32) -> bool {
        let Some((normals, smooth)) = self.constraint_planes(u) else { return false };
        let star: Vec<u32> = self.node_tets[u as usize]
            .iter()
            .copied()
            .filter(|t| self.alive[*t as usize])
            .collect();
        if star.is_empty() {
            return false;
        }
        let mut neighbours: BTreeSet<u32> = BTreeSet::new();
        for t in &star {
            for x in self.tets[*t as usize] {
                if x != u {
                    neighbours.insert(x);
                }
            }
        }
        let origin = self.points[u as usize];
        // On a smooth surface (approximate mode) the node follows its neighbours ON that surface
        // and is then put back onto the input; elsewhere it follows all its neighbours within
        // its planes.
        if let Some(s) = smooth {
            neighbours = self.node_faces[u as usize]
                .iter()
                .map(|f| *f as usize)
                .filter(|f| self.face_alive[*f] && self.face_surface[*f] == s)
                .flat_map(|f| self.faces[f])
                .filter(|x| *x != u)
                .collect();
            if neighbours.is_empty() {
                return false;
            }
        }
        let mut mean = Vec3::new(0.0, 0.0, 0.0);
        for x in &neighbours {
            mean = mean.add(self.points[*x as usize]);
        }
        let mean = mean.scale(1.0 / neighbours.len() as f64);
        let Some(step) = Self::project(&normals, mean.sub(origin)) else { return false };
        let onto_surface = |mesh: &Mesh, q: Vec3| -> Option<Vec3> {
            let Some(s) = smooth else { return Some(q) };
            let approx = mesh.approx.as_ref()?;
            let input = approx.by_surface.get(&s)?;
            let reach = q.sub(origin).dot(q.sub(origin)).sqrt() + approx.tol;
            let (q, _) = input.nearest(q, reach)?;
            let faces_ok = mesh.node_faces[u as usize]
                .iter()
                .map(|f| *f as usize)
                .filter(|f| mesh.face_alive[*f] && mesh.face_surface[*f] == s)
                .all(|f| mesh.face_within(s, mesh.faces[f].map(|n| if n == u { q } else { mesh.points[n as usize] })));
            faces_ok.then_some(q)
        };
        let worst_at = |mesh: &Mesh, q: Vec3| -> Option<f64> {
            let mut worst = 0.0f64;
            for t in &star {
                let p = mesh.tets[*t as usize].map(|n| if n == u { q } else { mesh.points[n as usize] });
                let mut longest = 0.0f64;
                for a in 0..4 {
                    for b in (a + 1)..4 {
                        let d = p[b].sub(p[a]);
                        longest = longest.max(d.dot(d));
                    }
                }
                if orient3d(p[0], p[1], p[2], p[3]) <= longest * mesh.tol {
                    return None;
                }
                worst = worst.max(aspect_ratio(p));
            }
            Some(worst)
        };
        let Some(before) = worst_at(self, origin) else { return false };
        let mut best: Option<(f64, Vec3)> = None;
        for fraction in [1.0, 0.5, 0.25] {
            let Some(q) = onto_surface(self, origin.add(step.scale(fraction))) else { continue };
            if let Some(w) = worst_at(self, q) {
                if w < before && best.is_none_or(|(b, _)| w < b) {
                    best = Some((w, q));
                }
            }
        }
        match best {
            Some((_, q)) => {
                self.points[u as usize] = q;
                for t in &self.node_tets[u as usize] {
                    if let Some(f) = self.fresh.get_mut(*t as usize) {
                        *f = false;
                    }
                }
                true
            }
            None => false,
        }
    }

    fn oriented(&self, t: [u32; 4]) -> Option<([u32; 4], f64)> {
        let p = self.tet_points(t);
        let v = orient3d(p[0], p[1], p[2], p[3]);
        let mut longest = 0.0f64;
        for a in 0..4 {
            for b in (a + 1)..4 {
                let d = p[b].sub(p[a]);
                longest = longest.max(d.dot(d));
            }
        }
        if v.abs() <= longest * self.tol {
            return None;
        }
        Some(if v > 0.0 { (t, v) } else { ([t[1], t[0], t[2], t[3]], -v) })
    }

    // Replace the tets `old` (all alive, one material) by `new` when the new ones are valid, fill
    // exactly the same volume, and make the worst aspect ratio strictly better. Interior only:
    // the caller has checked that every face removed is untagged.
    fn replace(&mut self, old: &[u32], new: &[[u32; 4]]) -> bool {
        let material = self.material[old[0] as usize];
        if old.iter().any(|t| self.material[*t as usize] != material) {
            return false;
        }
        let mut before_volume = 0.0;
        let mut before_worst = 0.0f64;
        for t in old {
            let p = self.tet_points(self.tets[*t as usize]);
            before_volume += orient3d(p[0], p[1], p[2], p[3]).abs();
            before_worst = before_worst.max(aspect_ratio(p));
        }
        let mut made: Vec<[u32; 4]> = Vec::new();
        let mut after_volume = 0.0;
        let mut after_worst = 0.0f64;
        for t in new {
            // A tet with all four corners on tagged surfaces lies along the interface, where its
            // centroid can sit between a chord and the curved surface it approximates and read as
            // the wrong material (a3: two such slivers, `[V5]` PASS -> WARN). Flips may not make one.
            if t.iter().all(|x| self.surfaces[*x as usize].iter().any(|s| *s < BOX_BASE)) {
                return false;
            }
            let Some((o, v)) = self.oriented(*t) else { return false };
            after_volume += v;
            after_worst = after_worst.max(aspect_ratio(self.tet_points(o)));
            made.push(o);
        }
        if (after_volume - before_volume).abs() > 1.0e-10 * before_volume || after_worst >= before_worst {
            return false;
        }
        let mut touched: BTreeSet<u32> = BTreeSet::new();
        for t in old {
            touched.extend(self.tets[*t as usize]);
        }
        let old_set: BTreeSet<u32> = old.iter().copied().collect();
        for x in &touched {
            self.node_tets[*x as usize].retain(|t| !old_set.contains(t));
        }
        let source = self.source[old[0] as usize];
        let mut slots: Vec<u32> = old.to_vec();
        for (k, t) in made.iter().enumerate() {
            let slot = if k < slots.len() {
                slots[k]
            } else {
                self.tets.push(*t);
                self.alive.push(true);
                self.material.push(material);
                self.source.push(source);
                let id = (self.tets.len() - 1) as u32;
                slots.push(id);
                id
            };
            self.tets[slot as usize] = *t;
            self.alive[slot as usize] = true;
            self.stale(slot);
            for x in t {
                self.node_tets[*x as usize].push(slot);
            }
        }
        for slot in slots.iter().skip(made.len()) {
            self.alive[*slot as usize] = false;
        }
        true
    }

    // 3-2: an interior edge with exactly three tets around it, none of its faces tagged, not a
    // curve edge, becomes the two tets on the triangle of its three ring nodes.
    fn try_flip32(&mut self, a: u32, b: u32) -> bool {
        let key = if a < b { [a, b] } else { [b, a] };
        if self.curve_edges.contains_key(&key) {
            return false;
        }
        let ring = self.tets_with(&[a, b]);
        if ring.len() != 3 {
            return false;
        }
        let mut others: BTreeSet<u32> = BTreeSet::new();
        for t in &ring {
            for x in self.tets[*t as usize] {
                if x != a && x != b {
                    others.insert(x);
                }
            }
        }
        if others.len() != 3 {
            return false;
        }
        let o: Vec<u32> = others.into_iter().collect();
        for x in &o {
            if self.face_index.contains_key(&sorted3([a, b, *x])) {
                return false;
            }
        }
        // The ring must close: each face (a, b, x) is shared by two of the three tets.
        for x in &o {
            if ring.iter().filter(|t| self.tets[**t as usize].contains(x)).count() != 2 {
                return false;
            }
        }
        self.replace(&ring, &[[o[0], o[1], o[2], a], [o[0], o[1], o[2], b]])
    }

    // A flat tet lying IN a tagged surface is a fold: the material side carries the quad (a, b, c, d)
    // on one diagonal (a, c) and the other side on the other (b, d). When the two material tets
    // over (a, c) share an apex q, they become (q, a, b, d), (q, c, b, d), the flat tet goes, and
    // the two tagged face records move onto its other two faces - the same plane within `tol`, so
    // no surface moves and no node does. The worst elements S9 left on the rotated plates
    // (a7a_r, AR 5.3e7) are exactly this.
    fn try_unfold(&mut self, t: u32, flat: f64) -> bool {
        let tet = self.tets[t as usize];
        let p = self.tet_points(tet);
        let mut longest = 0.0f64;
        for a in 0..4 {
            for b in (a + 1)..4 {
                let d = p[b].sub(p[a]);
                longest = longest.max(d.dot(d).sqrt());
            }
        }
        // Coplanar within `flat`: every corner that close to the plane of the other three.
        let six_v = orient3d(p[0], p[1], p[2], p[3]).abs();
        for slots in TET_FACES {
            let [a, b, c] = slots.map(|k| p[k]);
            let area2 = b.sub(a).cross(c.sub(a));
            let area2 = area2.dot(area2).sqrt();
            if area2 <= 0.0 || six_v / area2 > flat {
                { self.refused(901); return false; }
            }
        }
        let tagged: Vec<(usize, [u32; 3])> = TET_FACES
            .iter()
            .filter_map(|slots| {
                let face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                self.face_index.get(&sorted3(face)).map(|f| (*f, face))
            })
            .collect();
        if tagged.len() != 2 || self.face_surface[tagged[0].0] != self.face_surface[tagged[1].0] {
            { self.refused(902); return false; }
        }
        let (f1, f2) = (tagged[0].0, tagged[1].0);
        let shared: Vec<u32> = tagged[0].1.iter().copied().filter(|x| tagged[1].1.contains(x)).collect();
        let [a, c] = [shared[0], shared[1]];
        let key = if a < c { [a, c] } else { [c, a] };
        if self.curve_edges.contains_key(&key) {
            { self.refused(903); return false; }
        }
        let Some(b) = tagged[0].1.iter().copied().find(|x| *x != a && *x != c) else { return false };
        let Some(d) = tagged[1].1.iter().copied().find(|x| *x != a && *x != c) else { return false };
        let ring = self.tets_with(&[a, c]);
        if ring.len() != 3 || !ring.contains(&t) {
            { self.refused(904); return false; }
        }
        let others: Vec<u32> = ring.iter().copied().filter(|x| *x != t).collect();
        let x1 = *others.iter().find(|x| self.tets[**x as usize].contains(&b)).unwrap_or(&u32::MAX);
        let x2 = *others.iter().find(|x| self.tets[**x as usize].contains(&d)).unwrap_or(&u32::MAX);
        if x1 == u32::MAX || x2 == u32::MAX || x1 == x2 {
            { self.refused(905); return false; }
        }
        let apex = |x: u32| self.tets[x as usize].iter().copied().find(|n| ![a, b, c, d].contains(n));
        let (Some(q), Some(q2)) = (apex(x1), apex(x2)) else { return false };
        let material = self.material[x1 as usize];
        if q != q2 || self.material[x2 as usize] != material || self.material[t as usize] == material {
            { self.refused(906); return false; }
        }
        // The far side of each new tagged face must be of another material, or it is no boundary.
        for face in [[a, b, d], [c, b, d]] {
            let far: Vec<u32> = self.tets_with(&face).into_iter().filter(|x| *x != t).collect();
            if far.len() != 1 || self.material[far[0] as usize] == material || self.face_index.contains_key(&sorted3(face)) {
                { self.refused(907); return false; }
            }
        }
        let new = [[q, a, b, d], [q, c, b, d]];
        if new.iter().any(|n| n.iter().all(|x| self.surfaces[*x as usize].iter().any(|s| *s < BOX_BASE))) {
            { self.refused(908); return false; }
        }
        let volume = |n: [u32; 4]| {
            let p = self.tet_points(n);
            orient3d(p[0], p[1], p[2], p[3])
        };
        // The two new tets fill exactly the ring of (a, c): both material tets and the flat one.
        let before: f64 =
            volume(self.tets[x1 as usize]).abs() + volume(self.tets[x2 as usize]).abs() + volume(tet).abs();
        let mut made: Vec<[u32; 4]> = Vec::new();
        let mut after = 0.0;
        let mut after_worst = 0.0f64;
        for n in new {
            let Some((o, v)) = self.oriented(n) else { return false };
            after += v;
            after_worst = after_worst.max(aspect_ratio(self.tet_points(o)));
            made.push(o);
        }
        let before_worst = [t, x1, x2]
            .iter()
            .map(|x| aspect_ratio(self.tet_points(self.tets[*x as usize])))
            .fold(0.0f64, f64::max);
        if (after - before).abs() > 1.0e-9 * before || after_worst >= before_worst {
            { self.refused(909); return false; }
        }
        // Apply: the two material tets take the other diagonal, the flat tet goes, the tagged
        // records move with their winding (normal sense) kept.
        for x in tet {
            self.node_tets[x as usize].retain(|y| *y != t);
        }
        self.alive[t as usize] = false;
        for (slot, n) in [x1, x2].into_iter().zip(made) {
            for x in self.tets[slot as usize] {
                self.node_tets[x as usize].retain(|y| *y != slot);
            }
            self.tets[slot as usize] = n;
            self.stale(slot);
            for x in n {
                self.node_tets[x as usize].push(slot);
            }
        }
        for (f, face) in [(f1, [a, b, d]), (f2, [c, b, d])] {
            let old = self.faces[f];
            let normal = |g: [u32; 3]| {
                let [u, v, w] = g.map(|n| self.points[n as usize]);
                v.sub(u).cross(w.sub(u))
            };
            let sense = normal(old);
            let g = if normal(face).dot(sense) >= 0.0 { face } else { [face[0], face[2], face[1]] };
            self.face_index.remove(&sorted3(old));
            for x in old {
                self.node_faces[x as usize].retain(|y| *y != f as u32);
            }
            self.faces[f] = g;
            self.face_index.insert(sorted3(g), f);
            for x in g {
                self.node_faces[x as usize].push(f as u32);
            }
        }
        true
    }

    // 2-3: an untagged interior face shared by two tets of one material becomes three tets
    // around the segment joining the two apexes.
    fn try_flip23(&mut self, f: [u32; 3]) -> bool {
        if self.face_index.contains_key(&sorted3(f)) {
            return false;
        }
        let pair = self.tets_with(&f);
        if pair.len() != 2 {
            return false;
        }
        let apex = |t: u32| self.tets[t as usize].iter().copied().find(|x| !f.contains(x));
        let (Some(d), Some(e)) = (apex(pair[0]), apex(pair[1])) else { return false };
        let [a, b, c] = f;
        self.replace(&pair, &[[a, b, d, e], [b, c, d, e], [c, a, d, e]])
    }

    // Try to collapse u onto v; apply it and return true when every check passes.
    fn try_collapse(&mut self, u: u32, v: u32) -> bool {
        if !self.allowed(u, v) {
            { self.refused(268); return false; }
        }
        let star: Vec<u32> = self.node_tets[u as usize]
            .iter()
            .copied()
            .filter(|t| self.alive[*t as usize])
            .collect();
        let (gone, keep): (Vec<u32>, Vec<u32>) =
            star.iter().partition(|t| self.tets[**t as usize].contains(&v));
        if gone.is_empty() {
            { self.refused(278); return false; }
        }
        let old_worst = star
            .iter()
            .map(|t| aspect_ratio(self.tet_points(self.tets[*t as usize])))
            .fold(0.0f64, f64::max);
        let mut new_tets: Vec<[u32; 4]> = Vec::with_capacity(keep.len());
        let mut new_worst = 0.0f64;
        for t in &keep {
            let mut n = self.tets[*t as usize];
            for x in &mut n {
                if *x == u {
                    *x = v;
                }
            }
            let p = self.tet_points(n);
            let mut longest = 0.0f64;
            for a in 0..4 {
                for b in (a + 1)..4 {
                    let d = p[b].sub(p[a]);
                    longest = longest.max(d.dot(d));
                }
            }
            // Positive, and by more than a node moving within `tol` could undo (the written file
            // is denormalised, so a tet a few ULP from flat comes out inverted).
            if orient3d(p[0], p[1], p[2], p[3]) <= longest * self.tol {
                { self.refused(304); return false; }
            }
            new_worst = new_worst.max(aspect_ratio(p));
            new_tets.push(n);
        }
        if new_worst >= old_worst {
            { self.refused(310); return false; }
        }
        // No duplicate tet, no face on three tets, and every face through v that separates two
        // materials (or partitions) is a tagged face after the collapse - and vice versa.
        let star_set: BTreeSet<u32> = star.iter().copied().collect();
        let around_v: Vec<u32> = self.node_tets[v as usize]
            .iter()
            .copied()
            .filter(|t| self.alive[*t as usize] && !star_set.contains(t))
            .collect();
        let existing: BTreeSet<[u32; 4]> = around_v.iter().map(|t| sorted4(self.tets[*t as usize])).collect();
        if new_tets.iter().any(|n| existing.contains(&sorted4(*n))) {
            { self.refused(322); return false; }
        }
        // Tagged faces after the collapse: u's faces remapped, those carrying both u and v gone.
        let mut tagged_after: BTreeSet<[u32; 3]> = BTreeSet::new();
        let mut remapped_faces: Vec<(usize, [u32; 3])> = Vec::new();
        for f in &self.node_faces[u as usize] {
            let f = *f as usize;
            if !self.face_alive[f] {
                continue;
            }
            if self.faces[f].contains(&v) {
                remapped_faces.push((f, [u32::MAX; 3]));
                continue;
            }
            let g = self.faces[f].map(|x| if x == u { v } else { x });
            // Not flipped or flattened: the remapped face keeps its normal's direction.
            let [a, b, c] = self.faces[f].map(|n| self.points[n as usize]);
            let [a2, b2, c2] = g.map(|n| self.points[n as usize]);
            let n1 = b.sub(a).cross(c.sub(a));
            let n2 = b2.sub(a2).cross(c2.sub(a2));
            if n1.dot(n2) <= 0.0 || n2.dot(n2).sqrt() <= self.tol * self.tol {
                { self.refused(343); return false; }
            }
            if self.smooth_at(u, self.face_surface[f])
                && !self.face_within(self.face_surface[f], [a2, b2, c2])
            {
                return false;
            }
            if self.face_index.contains_key(&sorted3(g)) {
                { self.refused(346); return false; }
            }
            tagged_after.insert(sorted3(g));
            remapped_faces.push((f, g));
        }
        for f in &self.node_faces[v as usize] {
            let f = *f as usize;
            if self.face_alive[f] && !self.faces[f].contains(&u) {
                tagged_after.insert(sorted3(self.faces[f]));
            }
        }
        let mut owners: BTreeMap<[u32; 3], Vec<(i32, i32, f64)>> = BTreeMap::new();
        let note = |tet: [u32; 4], material: (i32, i32), owners: &mut BTreeMap<[u32; 3], Vec<(i32, i32, f64)>>| {
            for slots in TET_FACES {
                let face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                if !face.contains(&v) {
                    continue;
                }
                let apex = tet.iter().copied().find(|x| !face.contains(x)).unwrap_or(v);
                let key = sorted3(face);
                let [a, b, c] = key.map(|n| self.points[n as usize]);
                let side = orient3d(a, b, c, self.points[apex as usize]);
                owners.entry(key).or_default().push((material.0, material.1, side));
            }
        };
        for (t, n) in keep.iter().zip(new_tets.iter()) {
            note(*n, self.material[*t as usize], &mut owners);
        }
        for t in &around_v {
            note(self.tets[*t as usize], self.material[*t as usize], &mut owners);
        }
        for (face, list) in &owners {
            if list.len() > 2 {
                { self.refused(379); return false; }
            }
            if list.len() == 2 {
                if list[0].2 * list[1].2 >= 0.0 {
                    { self.refused(383); return false; }
                }
                let differs = list[0].0 != list[1].0 || list[0].1 != list[1].1;
                if differs != tagged_after.contains(face) {
                    { self.refused(387); return false; }
                }
            } else {
                // One owner: the face is on the mesh boundary, which is the domain box; a tagged
                // face there would be half an interface.
                if tagged_after.contains(face) {
                    { self.refused(393); return false; }
                }
                let on_box = (0..3).any(|k| {
                    let c = |n: u32| {
                        let p = self.points[n as usize];
                        [p.x, p.y, p.z][k]
                    };
                    (c(face[0]) - c(face[1])).abs() <= self.tol && (c(face[0]) - c(face[2])).abs() <= self.tol
                });
                if !on_box {
                    { self.refused(403); return false; }
                }
            }
        }
        // Apply.
        for t in &gone {
            self.alive[*t as usize] = false;
        }
        for (t, n) in keep.iter().zip(new_tets.iter()) {
            self.tets[*t as usize] = *n;
            self.stale(*t);
            self.node_tets[v as usize].push(*t);
        }
        self.node_tets[u as usize].clear();
        for (f, g) in remapped_faces {
            self.face_index.remove(&sorted3(self.faces[f]));
            if g[0] == u32::MAX {
                self.face_alive[f] = false;
                continue;
            }
            self.faces[f] = g;
            self.face_index.insert(sorted3(g), f);
            self.node_faces[v as usize].push(f as u32);
        }
        self.node_faces[u as usize].clear();
        if !self.curve_ids[u as usize].is_empty() {
            let edges: Vec<([u32; 2], i32)> = self
                .curve_edges
                .iter()
                .filter(|(e, _)| e.contains(&u))
                .map(|(e, c)| (*e, *c))
                .collect();
            for (e, c) in edges {
                self.curve_edges.remove(&e);
                let other = if e[0] == u { e[1] } else { e[0] };
                if other != v {
                    let key = if other < v { [other, v] } else { [v, other] };
                    self.curve_edges.insert(key, c);
                }
            }
            for poly in &mut self.curves {
                for x in poly.iter_mut() {
                    if *x == u {
                        *x = v;
                    }
                }
                poly.dedup();
            }
        }
        true
    }
}

// AI-FUNC-SUMMARY:
// Purpose: S9 - collapse short edges of bad tets in a stage-8/11 contract document wherever doing so
//   moves no geometry and strictly improves the worst aspect ratio around the collapsed node.
// Inputs: the document (tets first, then tagged faces and curve cells, as `cut_to_doc` writes it)
//   and the quality target.
// Returns: ImproveStats.
// Side effects: rewrites the document's points, cells, per-point and per-cell arrays and the
//   face-tag tables (side elements recomputed from the geometry; orientations carried, since a
//   collapse never flips a face).
// Notes: Serial, deterministic order (bad tets by worst aspect ratio, ties by index; edges by
//   length, ties by node ids). Nodes on curved surfaces never move (their faces are not coplanar);
//   letting them slide within a tolerance is M-4.5b's optional approximate mode.
pub fn improve_doc(doc: &mut VtuDoc, opts: &ImproveOptions) -> ImproveStats {
    improve_doc_with(doc, opts, None)
}

/// Approximate mode's inputs: the tolerance (world units), the feature angle, and each
/// component's input triangles in the document's frame.
pub struct ApproximateInputs<'a> {
    pub tolerance: f64,
    pub feature_angle_deg: f64,
    pub components: &'a [(i32, Vec<[Vec3; 3]>)],
}


// AI-FUNC-SUMMARY:
// Purpose: `improve_doc`, optionally in approximate mode (plan M-4.5b): smooth surface nodes may
//   also move, each face they touch held within the tolerance of the input surface.
// Inputs: the document, the target, and the approximate-mode inputs or None for exact.
// Returns: ImproveStats.
// Side effects: as `improve_doc`.
pub fn improve_doc_with(doc: &mut VtuDoc, opts: &ImproveOptions, approximate: Option<ApproximateInputs>) -> ImproveStats {
    let mut stats = ImproveStats::default();
    let timing = std::env::var_os("RUSTMSPT_TIME_STAGES").is_some();
    let mut clock = std::time::Instant::now();
    let mut lap = |label: &str| {
        if timing {
            println!("[S9-TIME] {} {:?}", label, clock.elapsed());
        }
        clock = std::time::Instant::now();
    };
    let cells = doc.num_cells();
    let region = i32_cells(doc, "region_key").unwrap_or_else(|| vec![0; cells]);
    let partition = i32_cells(doc, "partition_id").unwrap_or_else(|| vec![0; cells]);
    let face_key = i32_cells(doc, "face_tag_key").unwrap_or_else(|| vec![-1; cells]);
    let curve_id = i32_cells(doc, "curve_id").unwrap_or_else(|| vec![-1; cells]);
    let tag_offsets = field(doc, "FaceTagOffsets").map(as_i64).unwrap_or_default();
    let tag_components = field(doc, "FaceTagComponents").map(as_i64).unwrap_or_default();
    let tag_kind = field(doc, "FaceTagKind").map(as_i64).unwrap_or_default();

    let n = doc.points.len();
    let lo = doc.points.iter().fold(Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY), |m, p| {
        Vec3::new(m.x.min(p.x), m.y.min(p.y), m.z.min(p.z))
    });
    let hi = doc.points.iter().fold(Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |m, p| {
        Vec3::new(m.x.max(p.x), m.y.max(p.y), m.z.max(p.z))
    });
    let diag = hi.sub(lo).dot(hi.sub(lo)).sqrt().max(f64::MIN_POSITIVE);
    let mut mesh = Mesh {
        points: doc.points.clone(),
        tets: Vec::new(),
        alive: Vec::new(),
        material: Vec::new(),
        source: Vec::new(),
        node_tets: vec![Vec::new(); n],
        faces: Vec::new(),
        face_alive: Vec::new(),
        face_index: BTreeMap::new(),
        node_faces: vec![Vec::new(); n],
        face_surface: Vec::new(),
        curves: Vec::new(),
        surfaces: vec![BTreeSet::new(); n],
        curve_ids: vec![BTreeSet::new(); n],
        curve_edges: BTreeMap::new(),
        tol: 1.0e-9 * diag,
        weld: opts.weld.max(1.0e-9 * diag),
        refusals: std::cell::RefCell::new(BTreeMap::new()),
        approx: None,
        quality: Vec::new(),
        fresh: Vec::new(),
    };
    let mut tet_cell: Vec<usize> = Vec::new();
    let mut face_cell: Vec<usize> = Vec::new();
    let mut curve_cell: Vec<usize> = Vec::new();
    let mut surface_ids: BTreeMap<(Vec<i64>, i64), u32> = BTreeMap::new();
    for c in 0..cells {
        let nodes: Vec<u32> = doc.cell(c).iter().map(|x| *x as u32).collect();
        match doc.types[c] {
            VTK_TETRA if nodes.len() == 4 => {
                let t = [nodes[0], nodes[1], nodes[2], nodes[3]];
                let index = mesh.tets.len() as u32;
                for x in t {
                    mesh.node_tets[x as usize].push(index);
                }
                mesh.tets.push(t);
                mesh.alive.push(true);
                mesh.material.push((region[c], partition[c]));
                mesh.source.push(c);
                tet_cell.push(c);
            }
            VTK_TRIANGLE if nodes.len() == 3 => {
                let f = [nodes[0], nodes[1], nodes[2]];
                let key = face_key[c];
                let identity = if key >= 0 && (key as usize) < tag_offsets.len() {
                    let start = if key == 0 { 0 } else { tag_offsets[key as usize - 1] as usize };
                    let end = tag_offsets[key as usize] as usize;
                    let mut comps: Vec<i64> = tag_components[start..end].to_vec();
                    comps.sort_unstable();
                    (comps, tag_kind.get(key as usize).copied().unwrap_or(0))
                } else {
                    (Vec::new(), -1)
                };
                let next = surface_ids.len() as u32;
                let s = *surface_ids.entry(identity).or_insert(next);
                let index = mesh.faces.len();
                for x in f {
                    mesh.node_faces[x as usize].push(index as u32);
                    mesh.surfaces[x as usize].insert(s);
                }
                mesh.face_index.insert(sorted3(f), index);
                mesh.faces.push(f);
                mesh.face_alive.push(true);
                mesh.face_surface.push(s);
                face_cell.push(c);
            }
            VTK_LINE | VTK_POLY_LINE => {
                let id = curve_id[c];
                for w in nodes.windows(2) {
                    let key = if w[0] < w[1] { [w[0], w[1]] } else { [w[1], w[0]] };
                    mesh.curve_edges.insert(key, id);
                }
                for x in &nodes {
                    mesh.curve_ids[*x as usize].insert(id);
                }
                mesh.curves.push(nodes);
                curve_cell.push(c);
            }
            _ => {}
        }
    }
    lap("read");
    // A material boundary the document did not tag (a3r: 1,193 faces where a lower-priority surface
    // ends inside a higher one) is geometry all the same: it joins the classification as a virtual
    // face, so its nodes obey the same plane rules and the "tagged" check sees it. Virtual faces
    // are never written back - `rebuild` walks the document's own face cells only.
    {
        // Every (face, tet) pair, sorted: the same faces in the same order, with each face's
        // tets ascending, as a map built by one serial scan would give.
        let mut pairs: Vec<([u32; 3], usize)> = mesh
            .tets
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, t)| TET_FACES.iter().map(move |slots| (sorted3([t[slots[0]], t[slots[1]], t[slots[2]]]), i)))
            .collect();
        pairs.par_sort_unstable();
        let mut start = 0usize;
        while start < pairs.len() {
            let face = pairs[start].0;
            let mut end = start + 1;
            while end < pairs.len() && pairs[end].0 == face {
                end += 1;
            }
            let list: Vec<usize> = pairs[start..end].iter().map(|p| p.1).collect();
            start = end;
            if list.len() != 2 || mesh.material[list[0]] == mesh.material[list[1]] || mesh.face_index.contains_key(&face) {
                continue;
            }
            let (m0, m1) = (mesh.material[list[0]], mesh.material[list[1]]);
            let pair = if m0 <= m1 { (m0, m1) } else { (m1, m0) };
            let identity = (vec![-1 - pair.0 .0 as i64, -1 - pair.1 .0 as i64, pair.0 .1 as i64, pair.1 .1 as i64], -2i64);
            let next = surface_ids.len() as u32;
            let s = *surface_ids.entry(identity).or_insert(next);
            let index = mesh.faces.len();
            for x in face {
                mesh.node_faces[x as usize].push(index as u32);
                mesh.surfaces[x as usize].insert(s);
            }
            mesh.face_index.insert(face, index);
            mesh.faces.push(face);
            mesh.face_alive.push(true);
            mesh.face_surface.push(s);
        }
    }
    lap("untagged-boundaries");
    for (i, p) in mesh.points.iter().enumerate() {
        for (k, (l, h)) in [(lo.x, hi.x), (lo.y, hi.y), (lo.z, hi.z)].iter().enumerate() {
            let c = [p.x, p.y, p.z][k];
            if (c - l).abs() <= mesh.tol {
                mesh.surfaces[i].insert(BOX_BASE + 2 * k as u32);
            }
            if (c - h).abs() <= mesh.tol {
                mesh.surfaces[i].insert(BOX_BASE + 2 * k as u32 + 1);
            }
        }
    }

    if let Some(inputs) = approximate {
        let mut by_surface: BTreeMap<u32, InputSurface> = BTreeMap::new();
        for ((comps, kind), id) in &surface_ids {
            // A sheet's or a solid's own surface; box caps (kind 2) stay exact.
            if *kind == 2 || *kind == -2 || comps.is_empty() {
                continue;
            }
            let tris: Vec<[Vec3; 3]> = inputs
                .components
                .iter()
                .filter(|(x, _)| comps.contains(&(*x as i64)))
                .flat_map(|(_, t)| t.iter().copied())
                .collect();
            if !tris.is_empty() {
                by_surface.insert(*id, InputSurface::new(tris, inputs.tolerance));
            }
        }
        mesh.approx = Some(Approx {
            tol: inputs.tolerance,
            feature_cos: inputs.feature_angle_deg.to_radians().cos(),
            by_surface,
        });
    }
    // A maximum is an element of the input, so it does not depend on the order it is taken in.
    let count_bad = |mesh: &mut Mesh| -> (usize, f64) {
        let bad = mesh.bad_list(opts).len();
        (bad, mesh.worst_ar())
    };
    stats.tets_before = mesh.tets.len();
    (stats.bad_before, stats.worst_ar_before) = count_bad(&mut mesh);
    lap("setup");
    let mut move_time = [std::time::Duration::ZERO; 5];
    let mut move_count = [[0usize; 2]; 5];
    macro_rules! timed {
        ($slot:expr, $call:expr) => {{
            let started = std::time::Instant::now();
            let ok = $call;
            move_time[$slot] += started.elapsed();
            move_count[$slot][0] += 1;
            move_count[$slot][1] += usize::from(ok);
            ok
        }};
    }
    for _ in 0..opts.max_passes {
        let mut order: Vec<(f64, u32)> = mesh.bad_list(opts);
        order.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut done = 0usize;
        for (_, t) in order {
            if !mesh.alive[t as usize] {
                continue;
            }
            let tet = mesh.tets[t as usize];
            if !mesh.bad(tet, opts) {
                continue;
            }
            let mut edges: Vec<(f64, u32, u32)> = Vec::with_capacity(6);
            for a in 0..4 {
                for b in (a + 1)..4 {
                    let (x, y) = (tet[a].min(tet[b]), tet[a].max(tet[b]));
                    let d = mesh.points[y as usize].sub(mesh.points[x as usize]);
                    edges.push((d.dot(d), x, y));
                }
            }
            edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
            'edge: for (_, x, y) in edges.into_iter().take(3) {
                for (u, v) in [(x, y), (y, x)] {
                    if timed!(0, mesh.try_collapse(u, v)) {
                        done += 1;
                        break 'edge;
                    }
                }
            }
        }
        // Then flip around what is still bad: 3-2 on its edges, 2-3 on its faces.
        let mut flipped = 0usize;
        let bad_now: Vec<u32> =
            mesh.bad_list(opts).into_iter().map(|(_, t)| t).collect();
        for t in bad_now {
            if !mesh.alive[t as usize] {
                continue;
            }
            let tet = mesh.tets[t as usize];
            if !mesh.bad(tet, opts) {
                continue;
            }
            let mut done_here = timed!(1, mesh.try_unfold(t, mesh.tol));
            for (i, j) in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
                if done_here {
                    break;
                }
                if timed!(2, mesh.try_flip32(tet[i], tet[j])) {
                    done_here = true;
                    break;
                }
            }
            if !done_here {
                for slots in TET_FACES {
                    if timed!(3, mesh.try_flip23([tet[slots[0]], tet[slots[1]], tet[slots[2]]])) {
                        done_here = true;
                        break;
                    }
                }
            }
            flipped += usize::from(done_here);
        }
        stats.flips += flipped;
        lap("collapse+flip");
        // Then smooth the nodes of whatever is still bad, in node order.
        let mut nodes: BTreeSet<u32> = BTreeSet::new();
        for (_, t) in mesh.bad_list(opts) {
            nodes.extend(mesh.tets[t as usize].iter().copied());
        }
        let mut moved = 0usize;
        for u in nodes {
            moved += usize::from(timed!(4, mesh.try_smooth(u)));
        }
        lap("smooth");
        stats.passes += 1;
        stats.collapses += done;
        stats.smoothed += moved;
        if done == 0 && moved == 0 && flipped == 0 {
            break;
        }
    }
    if timing {
        for (name, (time, count)) in ["collapse", "unfold", "flip32", "flip23", "smooth"]
            .iter()
            .zip(move_time.iter().zip(move_count.iter()))
        {
            println!("[S9-TIME] move {name}: {time:?} over {} attempt(s), {} taken", count[0], count[1]);
        }
    }
    (stats.bad_after, stats.worst_ar_after) = count_bad(&mut mesh);
    if std::env::var_os("RUSTMSPT_S9_DIAG").is_some() {
        if let Some(approx) = &mesh.approx {
            let smooth = (0..mesh.points.len() as u32)
                .filter(|u| mesh.surfaces[*u as usize].iter().any(|s| mesh.smooth_at(*u, *s)))
                .count();
            println!(
                "[S9-DIAG] approximate: {} surface(s) indexed of {}, {} node(s) smooth, tol {:.3e}",
                approx.by_surface.len(),
                surface_ids.len(),
                smooth,
                approx.tol
            );
        }
        println!("[S9-DIAG] refusals by improve.rs line: {:?}", mesh.refusals.borrow());
    }
    // Last, the folds that are flat only to the weld quantum (a float32 input's inclined plane is
    // flat to ~1e-7, which `tol` refuses). Not inside the passes: the moved tags are then
    // coplanar to the weld only, which gives their nodes a further plane and pins them against
    // every later move (p1: 165 more bad tets when this ran in the passes).
    if mesh.weld > mesh.tol {
        let weld = mesh.weld;
        for (_, t) in mesh.bad_list(opts) {
            if mesh.alive[t as usize] && mesh.try_unfold(t, weld) {
                stats.flips += 1;
            }
        }
        (stats.bad_after, stats.worst_ar_after) = count_bad(&mut mesh);
    }
    stats.tets_after = mesh.alive.iter().filter(|a| **a).count();
    lap("count");
    if stats.collapses > 0 {
        rebuild(doc, &mesh, &tet_cell, &face_cell, &curve_cell);
    }
    lap("rebuild");
    stats
}

// Write the improved mesh back into the document: kept cells in their original order, points
// compacted, per-point and per-cell arrays filtered, face-tag tables rebuilt row by row.
fn rebuild(doc: &mut VtuDoc, mesh: &Mesh, tet_cell: &[usize], face_cell: &[usize], curve_cell: &[usize]) {
    let cells = doc.num_cells();
    let _ = tet_cell;
    // The output cells: every live tet first, in index order (a flipped tet copies its per-cell
    // row from the tet it replaced), then the original non-tet cells in their order.
    let mut entries: Vec<(Vec<u32>, u8, usize)> = Vec::new();
    for (i, t) in mesh.tets.iter().enumerate() {
        if mesh.alive[i] {
            entries.push((t.to_vec(), VTK_TETRA, mesh.source[i]));
        }
    }
    let mut replaced: Vec<Option<Option<Vec<u32>>>> = vec![None; cells];
    for (i, c) in face_cell.iter().enumerate() {
        replaced[*c] = Some(mesh.face_alive[i].then(|| mesh.faces[i].to_vec()));
    }
    for (i, c) in curve_cell.iter().enumerate() {
        replaced[*c] = Some((mesh.curves[i].len() >= 2).then(|| mesh.curves[i].clone()));
    }
    for c in 0..cells {
        if doc.types[c] == VTK_TETRA {
            continue;
        }
        let nodes = match &replaced[c] {
            Some(Some(n)) => n.clone(),
            Some(None) => continue,
            None => doc.cell(c).iter().map(|x| *x as u32).collect(),
        };
        entries.push((nodes, doc.types[c], c));
    }
    let mut used = vec![false; doc.points.len()];
    for (cell, _, _) in &entries {
        for x in cell {
            used[*x as usize] = true;
        }
    }
    let mut remap = vec![u32::MAX; doc.points.len()];
    let mut points = Vec::new();
    for (i, u) in used.iter().enumerate() {
        if *u {
            remap[i] = points.len() as u32;
            points.push(mesh.points[i]);
        }
    }
    let kept_points: Vec<usize> = (0..used.len()).filter(|i| used[*i]).collect();
    let kept_cells: Vec<usize> = entries.iter().map(|e| e.2).collect();
    let mut connectivity = Vec::new();
    let mut offsets = Vec::new();
    let mut types = Vec::new();
    for (nodes, kind, _) in &entries {
        for x in nodes {
            connectivity.push(remap[*x as usize] as i64);
        }
        offsets.push(connectivity.len() as i64);
        types.push(*kind);
    }
    fn filter(data: &ArrayData, comps: usize, keep: &[usize]) -> ArrayData {
        macro_rules! pick {
            ($v:expr, $variant:ident) => {{
                let mut out = Vec::with_capacity(keep.len() * comps);
                for k in keep {
                    out.extend_from_slice(&$v[k * comps..(k + 1) * comps]);
                }
                ArrayData::$variant(out)
            }};
        }
        match data {
            ArrayData::U8(v) => pick!(v, U8),
            ArrayData::I32(v) => pick!(v, I32),
            ArrayData::I64(v) => pick!(v, I64),
            ArrayData::U32(v) => pick!(v, U32),
            ArrayData::U64(v) => pick!(v, U64),
            ArrayData::F32(v) => pick!(v, F32),
            ArrayData::F64(v) => pick!(v, F64),
        }
    }
    for a in &mut doc.point_data {
        a.data = filter(&a.data, a.components, &kept_points);
    }
    for a in &mut doc.cell_data {
        a.data = filter(&a.data, a.components, &kept_cells);
    }
    // Face-tag rows: keep each surviving face's own row, renumber its key, recompute side elems.
    let old_offsets = field(doc, "FaceTagOffsets").map(as_i64).unwrap_or_default();
    let old_components = match field(doc, "FaceTagComponents") {
        Some(ArrayData::I32(v)) => v.clone(),
        _ => Vec::new(),
    };
    let old_orientation = match field(doc, "FaceTagOrientation") {
        Some(ArrayData::I32(v)) => v.clone(),
        _ => Vec::new(),
    };
    let old_kind = match field(doc, "FaceTagKind") {
        Some(ArrayData::U8(v)) => v.clone(),
        _ => Vec::new(),
    };
    let old_keys = i32_cells_from(&doc.cell_data, "face_tag_key");
    // New tet index per kept tet cell (tets stay first, in order).
    let mut tet_of_face: BTreeMap<[u32; 3], Vec<u32>> = BTreeMap::new();
    let mut new_tet = 0u32;
    for (slot, entry) in entries.iter().enumerate() {
        if types[slot] == VTK_TETRA {
            let t: Vec<u32> = entry.0.iter().map(|x| remap[*x as usize]).collect();
            for f in TET_FACES {
                tet_of_face.entry(sorted3([t[f[0]], t[f[1]], t[f[2]]])).or_default().push(new_tet);
            }
            new_tet += 1;
        }
    }
    let tets_new: Vec<[u32; 4]> = entries
        .iter()
        .enumerate()
        .filter(|(slot, _)| types[*slot] == VTK_TETRA)
        .map(|(_, e)| {
            let t = &e.0;
            [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize], remap[t[3] as usize]]
        })
        .collect();
    let mut tag_offsets = Vec::new();
    let mut tag_components = Vec::new();
    let mut tag_orientation = Vec::new();
    let mut tag_kind = Vec::new();
    let mut side = Vec::new();
    let mut new_keys = vec![-1i32; kept_cells.len()];
    let mut next_key = 0i32;
    for (slot, _) in kept_cells.iter().enumerate() {
        let key = old_keys.get(slot).copied().unwrap_or(-1);
        if types[slot] != VTK_TRIANGLE || key < 0 || key as usize >= old_offsets.len() {
            continue;
        }
        let start = if key == 0 { 0 } else { old_offsets[key as usize - 1] as usize };
        let end = old_offsets[key as usize] as usize;
        tag_components.extend_from_slice(&old_components[start..end]);
        tag_orientation.extend_from_slice(&old_orientation[start..end.min(old_orientation.len())]);
        tag_offsets.push(tag_components.len() as i64);
        tag_kind.push(old_kind.get(key as usize).copied().unwrap_or(0));
        let f: Vec<u32> = {
            let start = if slot == 0 { 0 } else { offsets[slot - 1] as usize };
            connectivity[start..offsets[slot] as usize].iter().map(|x| *x as u32).collect()
        };
        let [a, b, c] = [f[0], f[1], f[2]].map(|x| points[x as usize]);
        let owners = tet_of_face.get(&sorted3([f[0], f[1], f[2]])).cloned().unwrap_or_default();
        let mut plus = -1i32;
        let mut minus = -1i32;
        for t in owners {
            let apex = tets_new[t as usize].iter().copied().find(|x| !f.contains(x)).unwrap();
            if orient3d(a, b, c, points[apex as usize]) > 0.0 {
                plus = t as i32;
            } else {
                minus = t as i32;
            }
        }
        side.push(plus);
        side.push(minus);
        new_keys[slot] = next_key;
        next_key += 1;
    }
    if let Some(a) = doc.cell_data.iter_mut().find(|a| a.name == "face_tag_key") {
        a.data = ArrayData::I32(new_keys);
    }
    let set = |doc: &mut VtuDoc, name: &str, comps: usize, data: ArrayData| {
        if let Some(a) = doc.field_data.iter_mut().find(|a| a.name == name) {
            a.data = data;
        } else {
            doc.field_data.push(DataArray { name: name.to_string(), components: comps, data });
        }
    };
    set(doc, "FaceTagOffsets", 1, ArrayData::I64(tag_offsets));
    set(doc, "FaceTagComponents", 1, ArrayData::I32(tag_components));
    set(doc, "FaceTagOrientation", 1, ArrayData::I32(tag_orientation));
    set(doc, "FaceTagKind", 1, ArrayData::U8(tag_kind));
    set(doc, "FaceTagSideElems", 2, ArrayData::I32(side));
    doc.points = points;
    doc.connectivity = connectivity;
    doc.offsets = offsets;
    doc.types = types;
}

fn i32_cells_from(cell_data: &[DataArray], name: &str) -> Vec<i32> {
    match cell_data.iter().find(|a| a.name == name).map(|a| &a.data) {
        Some(ArrayData::I32(v)) => v.clone(),
        Some(ArrayData::I64(v)) => v.iter().map(|x| *x as i32).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AI-FUNC-SUMMARY: The quality measures read 1 and arccos(1/3) on a regular tet and blow up on a needle; returns nothing; side effects: none.
    #[test]
    fn aspect_ratio_and_dihedral_on_regular_and_needle_tets() {
        let regular = [
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(-1.0, 1.0, -1.0),
            Vec3::new(-1.0, -1.0, 1.0),
        ];
        assert!((aspect_ratio(regular) - 1.0).abs() < 1e-12);
        assert!((min_dihedral_deg(regular) - (1.0f64 / 3.0).acos().to_degrees()).abs() < 1e-9);
        let needle = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.001, 0.001, 0.001),
        ];
        assert!(aspect_ratio(needle) > 10.0);
    }

    // AI-FUNC-SUMMARY: A cube of tets with one interior node nearly on a corner's neighbour (a needle star) is improved - the needle goes, the cube's volume and its boundary are untouched, every tet stays positive; returns nothing; side effects: none.
    #[test]
    fn a_needle_star_is_collapsed_without_moving_the_boundary() {
        // Cube corners 0..8, centre 8, and node 9 a hair away from the centre: the twelve tets
        // coning each boundary triangle to the centre are split again through node 9.
        let mut points: Vec<Vec3> = Vec::new();
        for i in 0..8 {
            points.push(Vec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64));
        }
        points.push(Vec3::new(0.5, 0.5, 0.5));
        points.push(Vec3::new(0.5, 0.5, 0.5005));
        let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        let mut tets: Vec<[u32; 4]> = Vec::new();
        for q in quads {
            for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                // The face nearer node 9 (z = 1) cones to 9, the rest to 8, and the lid between
                // them is split through both.
                let apex = 8u32;
                tets.push([tri[0], tri[1], tri[2], apex]);
            }
        }
        // Split the tet (4,6,7,8)'s apex region: replace every tet touching the top face by two.
        let mut split: Vec<[u32; 4]> = Vec::new();
        for t in tets {
            if t[..3].iter().all(|n| points[*n as usize].z == 1.0) {
                split.push([t[0], t[1], t[2], 9]);
                split.push([t[0], t[1], 9, 8]);
                split.push([t[1], t[2], 9, 8]);
                split.push([t[2], t[0], 9, 8]);
            } else {
                split.push(t);
            }
        }
        let orient = |t: [u32; 4]| {
            let p = t.map(|n| points[n as usize]);
            if orient3d(p[0], p[1], p[2], p[3]) < 0.0 { [t[1], t[0], t[2], t[3]] } else { t }
        };
        let split: Vec<[u32; 4]> = split.into_iter().map(orient).collect();
        let mut doc = VtuDoc {
            points: points.clone(),
            connectivity: split.iter().flatten().map(|n| *n as i64).collect(),
            offsets: (1..=split.len()).map(|k| 4 * k as i64).collect(),
            types: vec![VTK_TETRA; split.len()],
            point_data: Vec::new(),
            cell_data: vec![DataArray::scalar("region_key", ArrayData::I32(vec![0; split.len()]))],
            field_data: Vec::new(),
        };
        let volume = |doc: &VtuDoc| -> f64 {
            (0..doc.num_cells())
                .map(|c| {
                    let n: Vec<Vec3> = doc.cell(c).iter().map(|x| doc.points[*x as usize]).collect();
                    orient3d(n[0], n[1], n[2], n[3]) / 6.0
                })
                .sum()
        };
        assert!((volume(&doc) - 1.0).abs() < 1e-12);
        let stats = improve_doc(&mut doc, &ImproveOptions::default());
        assert!(stats.bad_before > 0, "the fixture has needles");
        assert!(stats.bad_after < stats.bad_before, "{stats:?}");
        assert!((volume(&doc) - 1.0).abs() < 1e-12, "the cube's volume is unchanged");
        for c in 0..doc.num_cells() {
            let n: Vec<Vec3> = doc.cell(c).iter().map(|x| doc.points[*x as usize]).collect();
            assert!(orient3d(n[0], n[1], n[2], n[3]) > 0.0);
        }
        // Every cube corner is still a node, where it was.
        for i in 0..8 {
            assert!(doc.points.iter().any(|p| p.sub(points[i]).dot(p.sub(points[i])) == 0.0));
        }
    }
}
