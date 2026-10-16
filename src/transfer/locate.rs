use crate::error::{Result, RustMsptError};
use nalgebra::Matrix3;
use rayon::prelude::*;

pub const BARYCENTRIC_TOLERANCE: f64 = 1.0e-12;
pub const BOX_RELATIVE_PAD: f64 = 1.0e-9;
pub const LEAF_SIZE: usize = 4;
pub const DISTANCE_TIE: f64 = 1.0e-12;

// AI-FUNC-SUMMARY: A source tetrahedral mesh prepared for point location: per-tet corners, centroid, inverse edge matrix and invertibility; carries no I/O.
pub struct TetMesh {
    corners: Vec<[[f64; 3]; 4]>,
    centroids: Vec<[f64; 3]>,
    inverses: Vec<[f64; 9]>,
    invertible: Vec<bool>,
}

impl TetMesh {
    // AI-FUNC-SUMMARY:
    // Purpose: Gather each tetrahedron's corners and precompute what point location needs.
    // Inputs: points as flat xyz f64 (n*3), cells as flat u32 (m*4).
    // Returns: TetMesh, or InvalidMesh for a ragged array or a cell naming a missing point.
    // Side effects: None.
    // Notes: The centroid is ((v0+v1)+v2)+v3 over 4, numpy's mean order; the inverse is of the matrix whose columns are v1-v0, v2-v0, v3-v0. A singular cell is kept and never contains a point.
    pub fn new(points: &[f64], cells: &[u32]) -> Result<Self> {
        if !points.len().is_multiple_of(3) || !cells.len().is_multiple_of(4) {
            return Err(RustMsptError::InvalidMesh(
                "points must be (n, 3) and cells (m, 4)".into(),
            ));
        }
        let count = points.len() / 3;
        if let Some(bad) = cells.iter().find(|&&c| c as usize >= count) {
            return Err(RustMsptError::InvalidMesh(format!(
                "cell names point {bad} of {count}"
            )));
        }
        let corners: Vec<[[f64; 3]; 4]> = cells
            .par_chunks_exact(4)
            .map(|c| {
                let p = |i: u32| {
                    let k = i as usize * 3;
                    [points[k], points[k + 1], points[k + 2]]
                };
                [p(c[0]), p(c[1]), p(c[2]), p(c[3])]
            })
            .collect();
        let centroids = corners
            .par_iter()
            .map(|v| {
                let mut c = [0.0; 3];
                for (d, slot) in c.iter_mut().enumerate() {
                    *slot = (((v[0][d] + v[1][d]) + v[2][d]) + v[3][d]) / 4.0;
                }
                c
            })
            .collect();
        let (inverses, invertible) = corners
            .par_iter()
            .map(|v| {
                let e = |k: usize, d: usize| v[k][d] - v[0][d];
                let m = Matrix3::new(
                    e(1, 0), e(2, 0), e(3, 0),
                    e(1, 1), e(2, 1), e(3, 1),
                    e(1, 2), e(2, 2), e(3, 2),
                );
                match m.try_inverse() {
                    Some(inv) if inv.iter().all(|x| x.is_finite()) => {
                        let mut out = [0.0; 9];
                        for r in 0..3 {
                            for c in 0..3 {
                                out[r * 3 + c] = inv[(r, c)];
                            }
                        }
                        (out, true)
                    }
                    _ => ([0.0; 9], false),
                }
            })
            .unzip();
        Ok(Self {
            corners,
            centroids,
            inverses,
            invertible,
        })
    }

    // AI-FUNC-SUMMARY: Number of tetrahedra; returns usize; side effects: none.
    pub fn len(&self) -> usize {
        self.corners.len()
    }

    // AI-FUNC-SUMMARY: Whether the mesh has no tetrahedra; returns bool; side effects: none.
    pub fn is_empty(&self) -> bool {
        self.corners.is_empty()
    }

    // AI-FUNC-SUMMARY: Whether tet `t` contains `p` by barycentric coordinates within BARYCENTRIC_TOLERANCE; returns bool (false for a singular tet); side effects: none.
    pub fn contains(&self, t: usize, p: &[f64; 3]) -> bool {
        if !self.invertible[t] {
            return false;
        }
        let v0 = self.corners[t][0];
        let d = [p[0] - v0[0], p[1] - v0[1], p[2] - v0[2]];
        let m = &self.inverses[t];
        let mut sum = 0.0;
        for r in 0..3 {
            let l = m[r * 3] * d[0] + m[r * 3 + 1] * d[1] + m[r * 3 + 2] * d[2];
            if l < -BARYCENTRIC_TOLERANCE {
                return false;
            }
            sum += l;
        }
        sum <= 1.0 + BARYCENTRIC_TOLERANCE
    }

    // AI-FUNC-SUMMARY: Squared distance from `p` to tet `t`'s centroid; returns f64; side effects: none.
    pub fn centroid_distance2(&self, t: usize, p: &[f64; 3]) -> f64 {
        let c = self.centroids[t];
        (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Exact distance from an outside point to tet `t`, as simHelper's `_distance_to_tetra`.
    // Inputs: tet index, point.
    // Returns: The minimum over the six edge segments and the four face feet that land inside their face.
    // Side effects: None.
    pub fn distance(&self, t: usize, p: &[f64; 3]) -> f64 {
        let v = &self.corners[t];
        let sub = |a: &[f64; 3], b: &[f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let dot = |a: &[f64; 3], b: &[f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let mut best = f64::INFINITY;
        for (a, b) in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
            let start = v[a];
            let along = sub(&v[b], &start);
            let length = dot(&along, &along);
            let step = (dot(&sub(p, &start), &along) / length).clamp(0.0, 1.0);
            let q = [
                p[0] - (start[0] + step * along[0]),
                p[1] - (start[1] + step * along[1]),
                p[2] - (start[2] + step * along[2]),
            ];
            best = best.min(dot(&q, &q).sqrt());
        }
        for (a, b, c) in [(0, 1, 2), (0, 1, 3), (0, 2, 3), (1, 2, 3)] {
            let origin = v[a];
            let first = sub(&v[b], &origin);
            let second = sub(&v[c], &origin);
            let normal = [
                first[1] * second[2] - first[2] * second[1],
                first[2] * second[0] - first[0] * second[2],
                first[0] * second[1] - first[1] * second[0],
            ];
            let scale = dot(&normal, &normal);
            let offset = dot(&sub(p, &origin), &normal) / scale;
            let foot = [
                p[0] - offset * normal[0] - origin[0],
                p[1] - offset * normal[1] - origin[1],
                p[2] - offset * normal[2] - origin[2],
            ];
            let d00 = dot(&first, &first);
            let d01 = dot(&first, &second);
            let d11 = dot(&second, &second);
            let d20 = dot(&foot, &first);
            let d21 = dot(&foot, &second);
            let denominator = d00 * d11 - d01 * d01;
            let u = (d11 * d20 - d01 * d21) / denominator;
            let w = (d00 * d21 - d01 * d20) / denominator;
            if u >= 0.0 && w >= 0.0 && u + w <= 1.0 {
                best = best.min(offset.abs() * scale.sqrt());
            }
        }
        best
    }

    // AI-FUNC-SUMMARY: Axis-aligned box of tet `t` padded by BOX_RELATIVE_PAD of its largest extent, so it holds every point the barycentric tolerance admits; returns (min, max).
    fn padded_box(&self, t: usize) -> ([f64; 3], [f64; 3]) {
        let v = &self.corners[t];
        let mut lo = v[0];
        let mut hi = v[0];
        for corner in &v[1..] {
            for d in 0..3 {
                lo[d] = lo[d].min(corner[d]);
                hi[d] = hi[d].max(corner[d]);
            }
        }
        let extent = (0..3).map(|d| hi[d] - lo[d]).fold(0.0, f64::max);
        let pad = BOX_RELATIVE_PAD * extent + f64::MIN_POSITIVE;
        for d in 0..3 {
            lo[d] -= pad;
            hi[d] += pad;
        }
        (lo, hi)
    }
}

// AI-FUNC-SUMMARY: A static bounding-volume hierarchy over a TetMesh's padded tet boxes; nodes are stored so a GPU kernel can walk the same arrays.
// Notes: `link[n] = [first, count]` for a leaf (count > 0, slots first..first+count of `order`/`slot_*`), `[left, 0]` for an internal node whose children are left and left+1.
pub struct TetBvh {
    pub node_min: Vec<[f64; 3]>,
    pub node_max: Vec<[f64; 3]>,
    pub link: Vec<[u32; 2]>,
    pub order: Vec<u32>,
    pub slot_min: Vec<[f64; 3]>,
    pub slot_max: Vec<[f64; 3]>,
    pub depth: usize,
}

impl TetBvh {
    // AI-FUNC-SUMMARY:
    // Purpose: Build the hierarchy by median splits on the longest axis of the box centres.
    // Inputs: the mesh.
    // Returns: TetBvh with leaves of at most LEAF_SIZE tets (more only when every centre coincides).
    // Side effects: None.
    // Notes: Deterministic: the split orders by centre then tet index.
    pub fn build(mesh: &TetMesh) -> Self {
        let n = mesh.len();
        let boxes: Vec<([f64; 3], [f64; 3])> = (0..n).into_par_iter().map(|t| mesh.padded_box(t)).collect();
        let centre = |t: u32| {
            let (lo, hi) = boxes[t as usize];
            [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5]
        };
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut bvh = TetBvh {
            node_min: Vec::new(),
            node_max: Vec::new(),
            link: Vec::new(),
            order: Vec::new(),
            slot_min: Vec::new(),
            slot_max: Vec::new(),
            depth: 0,
        };
        let empty = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        bvh.node_min.push(empty.0);
        bvh.node_max.push(empty.1);
        bvh.link.push([0, 0]);
        let mut stack = vec![(0usize, 0usize, n, 1usize)];
        while let Some((node, lo, hi, level)) = stack.pop() {
            bvh.depth = bvh.depth.max(level);
            let mut bmin = [f64::INFINITY; 3];
            let mut bmax = [f64::NEG_INFINITY; 3];
            let mut cmin = [f64::INFINITY; 3];
            let mut cmax = [f64::NEG_INFINITY; 3];
            for &t in &order[lo..hi] {
                let (a, b) = boxes[t as usize];
                let c = centre(t);
                for d in 0..3 {
                    bmin[d] = bmin[d].min(a[d]);
                    bmax[d] = bmax[d].max(b[d]);
                    cmin[d] = cmin[d].min(c[d]);
                    cmax[d] = cmax[d].max(c[d]);
                }
            }
            bvh.node_min[node] = bmin;
            bvh.node_max[node] = bmax;
            let axis = (0..3)
                .max_by(|&a, &b| (cmax[a] - cmin[a]).total_cmp(&(cmax[b] - cmin[b])))
                .unwrap();
            if hi - lo <= LEAF_SIZE || cmax[axis] <= cmin[axis] {
                bvh.link[node] = [lo as u32, (hi - lo) as u32];
                continue;
            }
            let mid = (lo + hi) / 2;
            order[lo..hi].select_nth_unstable_by(mid - lo, |&a, &b| {
                centre(a)[axis].total_cmp(&centre(b)[axis]).then(a.cmp(&b))
            });
            let left = bvh.link.len();
            for _ in 0..2 {
                bvh.node_min.push(empty.0);
                bvh.node_max.push(empty.1);
                bvh.link.push([0, 0]);
            }
            bvh.link[node] = [left as u32, 0];
            stack.push((left + 1, mid, hi, level + 1));
            stack.push((left, lo, mid, level + 1));
        }
        bvh.slot_min = order.iter().map(|&t| boxes[t as usize].0).collect();
        bvh.slot_max = order.iter().map(|&t| boxes[t as usize].1).collect();
        bvh.order = order;
        bvh
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The containing tet whose centroid is nearest the point (ties: lower tet index).
    // Inputs: mesh, point.
    // Returns: Some(tet) or None when no tet contains the point.
    // Side effects: None.
    pub fn containing(&self, mesh: &TetMesh, p: &[f64; 3]) -> Option<u32> {
        let mut best: Option<(f64, u32)> = None;
        let mut stack = Vec::with_capacity(64);
        stack.push(0usize);
        while let Some(node) = stack.pop() {
            if !inside_box(p, &self.node_min[node], &self.node_max[node]) {
                continue;
            }
            let [first, count] = self.link[node];
            if count == 0 {
                stack.push(first as usize + 1);
                stack.push(first as usize);
                continue;
            }
            for slot in first as usize..(first + count) as usize {
                if inside_box(p, &self.slot_min[slot], &self.slot_max[slot]) {
                    consider_containing(mesh, self.order[slot], p, &mut best);
                }
            }
        }
        best.map(|(_, t)| t)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The nearest tet within `bound` of an outside point; tets within DISTANCE_TIE (relative) of the nearest distance tie, and the lowest index among them wins.
    // Inputs: mesh, point, bound.
    // Returns: Some((tet, its own distance)) or None when no tet lies within the bound.
    // Side effects: None.
    // Notes: Two branch-and-bound passes on padded boxes (which hold their tets): the first finds the nearest distance, the second the lowest index within the tie band. The band makes the answer independent of last-bit differences in how a distance is computed, so simHelper's Python search, which uses the same band, gives the same donor.
    pub fn nearest(&self, mesh: &TetMesh, p: &[f64; 3], bound: f64) -> Option<(u32, f64)> {
        let mut nearest = f64::INFINITY;
        self.visit_within(mesh, p, |limit| limit.min(bound), &mut |_, d, best| {
            if d <= bound && d < *best {
                *best = d;
            }
        }, &mut nearest);
        if !nearest.is_finite() {
            return None;
        }
        let band = (nearest * (1.0 + DISTANCE_TIE)).min(bound);
        let mut chosen: Option<(u32, f64)> = None;
        let mut fixed = band;
        self.visit_within(mesh, p, |_| band, &mut |t, d, _| {
            if d <= band && chosen.is_none_or(|(ct, _)| t < ct) {
                chosen = Some((t, d));
            }
        }, &mut fixed);
        chosen
    }

    // AI-FUNC-SUMMARY: Visit every tet whose padded box is within `limit(state)` of `p`, calling `seen(tet, exact distance, state)`; returns nothing; side effects: calls the closure, which may tighten the state.
    fn visit_within(
        &self,
        mesh: &TetMesh,
        p: &[f64; 3],
        limit: impl Fn(f64) -> f64,
        seen: &mut dyn FnMut(u32, f64, &mut f64),
        state: &mut f64,
    ) {
        let mut stack = Vec::with_capacity(64);
        stack.push(0usize);
        while let Some(node) = stack.pop() {
            if box_distance(p, &self.node_min[node], &self.node_max[node]) > limit(*state) {
                continue;
            }
            let [first, count] = self.link[node];
            if count == 0 {
                let (a, b) = (first as usize, first as usize + 1);
                let da = box_distance(p, &self.node_min[a], &self.node_max[a]);
                let db = box_distance(p, &self.node_min[b], &self.node_max[b]);
                if da <= db {
                    stack.push(b);
                    stack.push(a);
                } else {
                    stack.push(a);
                    stack.push(b);
                }
                continue;
            }
            for slot in first as usize..(first + count) as usize {
                if box_distance(p, &self.slot_min[slot], &self.slot_max[slot]) > limit(*state) {
                    continue;
                }
                let t = self.order[slot];
                let d = mesh.distance(t as usize, p);
                seen(t, d, state);
            }
        }
    }
}

// AI-FUNC-SUMMARY: Replace `best` with tet `t` when it contains `p` and its centroid is nearer (ties: lower index); returns nothing; side effects: mutates best.
fn consider_containing(mesh: &TetMesh, t: u32, p: &[f64; 3], best: &mut Option<(f64, u32)>) {
    if !mesh.contains(t as usize, p) {
        return;
    }
    let d = mesh.centroid_distance2(t as usize, p);
    if best.is_none_or(|(bd, bt)| d < bd || (d == bd && t < bt)) {
        *best = Some((d, t));
    }
}

// AI-FUNC-SUMMARY: Whether `p` lies in the closed box [lo, hi]; returns bool; side effects: none.
fn inside_box(p: &[f64; 3], lo: &[f64; 3], hi: &[f64; 3]) -> bool {
    (0..3).all(|d| p[d] >= lo[d] && p[d] <= hi[d])
}

// AI-FUNC-SUMMARY: Euclidean distance from `p` to the box [lo, hi] (0 inside); returns f64; side effects: none.
fn box_distance(p: &[f64; 3], lo: &[f64; 3], hi: &[f64; 3]) -> f64 {
    let mut s = 0.0;
    for d in 0..3 {
        let g = (lo[d] - p[d]).max(p[d] - hi[d]).max(0.0);
        s += g * g;
    }
    s.sqrt()
}

// AI-FUNC-SUMMARY: Counts of how the points of one location run were settled; carries no state beyond the numbers.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct LocateCounts {
    pub points: usize,
    pub contained: usize,
    pub fallback: usize,
    pub unlocated: usize,
    pub gpu_overflow: usize,
}

// AI-FUNC-SUMMARY: Donor tet (-1 for none) and distance (0 inside, NaN for none) of every point, with the counts; carries no state.
pub struct Located {
    pub donors: Vec<i64>,
    pub distances: Vec<f64>,
    pub counts: LocateCounts,
}

// AI-FUNC-SUMMARY: Settle one point exactly: containing tet, else nearest within bound, else none; returns (donor, distance).
fn settle(mesh: &TetMesh, bvh: &TetBvh, p: &[f64; 3], bound: f64) -> (i64, f64) {
    if let Some(t) = bvh.containing(mesh, p) {
        return (i64::from(t), 0.0);
    }
    match bvh.nearest(mesh, p, bound) {
        Some((t, d)) => (i64::from(t), d),
        None => (-1, f64::NAN),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Locate every point on the CPU, in parallel over the current Rayon pool.
// Inputs: mesh, its BVH, points as flat xyz, the fallback bound.
// Returns: Located, or InvalidConfig for a ragged point array or a bound that is negative or not finite.
// Side effects: None.
// Notes: The rule is simHelper's state mapper's: a containing tet wins (nearest centroid among several), else the nearest tet within the bound, else none. The answer does not depend on the thread count.
pub fn locate_cpu(mesh: &TetMesh, bvh: &TetBvh, points: &[f64], bound: f64) -> Result<Located> {
    check_inputs(points, bound)?;
    let settled: Vec<(i64, f64)> = points
        .par_chunks_exact(3)
        .map(|c| settle(mesh, bvh, &[c[0], c[1], c[2]], bound))
        .collect();
    Ok(collect(settled, 0))
}

// AI-FUNC-SUMMARY:
// Purpose: Finish a location from per-point candidate slots a GPU pass produced.
// Inputs: mesh, BVH, points (flat xyz), bound, per-point candidate counts, slots (k per point), k.
// Returns: Located identical to locate_cpu's, or an error for mismatched lengths.
// Side effects: None.
// Notes: The slots are a superset of every leaf slot whose padded box holds the point, so the containing tet is among them when one exists; a point whose count exceeds k, or with no containing candidate, is settled on the CPU.
pub fn locate_from_candidates(
    mesh: &TetMesh,
    bvh: &TetBvh,
    points: &[f64],
    bound: f64,
    counts: &[u32],
    slots: &[u32],
    k: usize,
) -> Result<Located> {
    check_inputs(points, bound)?;
    let n = points.len() / 3;
    if counts.len() != n || slots.len() != n * k {
        return Err(RustMsptError::InvalidConfig(
            "candidate arrays do not match the points".into(),
        ));
    }
    let settled: Vec<((i64, f64), bool)> = (0..n)
        .into_par_iter()
        .map(|i| {
            let p = [points[i * 3], points[i * 3 + 1], points[i * 3 + 2]];
            let c = counts[i] as usize;
            if c > k {
                return (settle(mesh, bvh, &p, bound), true);
            }
            let mut best = None;
            for &slot in &slots[i * k..i * k + c] {
                consider_containing(mesh, bvh.order[slot as usize], &p, &mut best);
            }
            if let Some((_, t)) = best {
                return ((i64::from(t), 0.0), false);
            }
            let nearest = match bvh.nearest(mesh, &p, bound) {
                Some((t, d)) => (i64::from(t), d),
                None => (-1, f64::NAN),
            };
            (nearest, false)
        })
        .collect();
    let overflow = settled.iter().filter(|(_, o)| *o).count();
    Ok(collect(settled.into_iter().map(|(s, _)| s).collect(), overflow))
}

// AI-FUNC-SUMMARY: Refuse a point array that is not (n, 3) or a bound that is negative or not finite; returns Ok or InvalidConfig.
fn check_inputs(points: &[f64], bound: f64) -> Result<()> {
    if !points.len().is_multiple_of(3) {
        return Err(RustMsptError::InvalidConfig("points must be (..., 3)".into()));
    }
    if !bound.is_finite() || bound < 0.0 {
        return Err(RustMsptError::InvalidConfig(format!(
            "fallback_distance must be finite and non-negative, not {bound}"
        )));
    }
    Ok(())
}

// AI-FUNC-SUMMARY: Split settled (donor, distance) pairs into arrays and count contained / fallback / unlocated points; returns Located.
fn collect(settled: Vec<(i64, f64)>, gpu_overflow: usize) -> Located {
    let mut counts = LocateCounts {
        points: settled.len(),
        gpu_overflow,
        ..Default::default()
    };
    let mut donors = Vec::with_capacity(settled.len());
    let mut distances = Vec::with_capacity(settled.len());
    for (t, d) in settled {
        if t < 0 {
            counts.unlocated += 1;
        } else if d == 0.0 {
            counts.contained += 1;
        } else {
            counts.fallback += 1;
        }
        donors.push(t);
        distances.push(d);
    }
    Located {
        donors,
        distances,
        counts,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};

    // AI-FUNC-SUMMARY: A jittered n^3 grid of cubes, each split into six Kuhn tetrahedra, with one cube left out as a hole; returns (points, cells).
    pub(crate) fn kuhn_mesh(n: usize, seed: u64) -> (Vec<f64>, Vec<u32>) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let id = |i: usize, j: usize, k: usize| ((i * (n + 1) + j) * (n + 1) + k) as u32;
        let mut points = Vec::new();
        for i in 0..=n {
            for j in 0..=n {
                for k in 0..=n {
                    let interior = |a: usize| a > 0 && a < n;
                    let jit = |a: usize, r: &mut rand::rngs::StdRng| {
                        if interior(a) { r.gen_range(-0.2..0.2) } else { 0.0 }
                    };
                    points.push(i as f64 + jit(i, &mut rng));
                    points.push(j as f64 + jit(j, &mut rng));
                    points.push(k as f64 + jit(k, &mut rng));
                }
            }
        }
        let mut cells = Vec::new();
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    if (i, j, k) == (n / 2, n / 2, n / 2) {
                        continue;
                    }
                    let c = |a: usize, b: usize, d: usize| id(i + a, j + b, k + d);
                    let v0 = c(0, 0, 0);
                    let v7 = c(1, 1, 1);
                    for path in [
                        [c(1, 0, 0), c(1, 1, 0)],
                        [c(1, 0, 0), c(1, 0, 1)],
                        [c(0, 1, 0), c(1, 1, 0)],
                        [c(0, 1, 0), c(0, 1, 1)],
                        [c(0, 0, 1), c(1, 0, 1)],
                        [c(0, 0, 1), c(0, 1, 1)],
                    ] {
                        cells.extend_from_slice(&[v0, path[0], path[1], v7]);
                    }
                }
            }
        }
        (points, cells)
    }

    // AI-FUNC-SUMMARY: The location rule by exhaustive search over every tet (fallback ties within DISTANCE_TIE to the lowest index); returns (donor, distance) for one point.
    pub(crate) fn brute(mesh: &TetMesh, p: &[f64; 3], bound: f64) -> (i64, f64) {
        let mut best: Option<(f64, u32)> = None;
        for t in 0..mesh.len() as u32 {
            consider_containing(mesh, t, p, &mut best);
        }
        if let Some((_, t)) = best {
            return (i64::from(t), 0.0);
        }
        let all: Vec<f64> = (0..mesh.len()).map(|t| mesh.distance(t, p)).collect();
        let nearest = all.iter().copied().filter(|&d| d <= bound).fold(f64::INFINITY, f64::min);
        if !nearest.is_finite() {
            return (-1, f64::NAN);
        }
        let band = (nearest * (1.0 + DISTANCE_TIE)).min(bound);
        let t = (0..all.len()).find(|&t| all[t] <= band).unwrap();
        (t as i64, all[t])
    }

    // AI-FUNC-SUMMARY: Random points inside, on the hole, just outside and far outside a size-n mesh, plus every mesh vertex; returns flat xyz.
    pub(crate) fn probe_points(points: &[f64], n: usize, count: usize, seed: u64) -> Vec<f64> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut out: Vec<f64> = points.to_vec();
        for _ in 0..count {
            for _ in 0..3 {
                out.push(rng.gen_range(-0.6..n as f64 + 0.6));
            }
        }
        out
    }

    // AI-FUNC-SUMMARY: Verify the BVH location equals exhaustive search on every probe point, at one and four workers.
    #[test]
    fn bvh_location_equals_exhaustive_search() {
        let n = 6;
        let (points, cells) = kuhn_mesh(n, 7);
        let mesh = TetMesh::new(&points, &cells).unwrap();
        let bvh = TetBvh::build(&mesh);
        let probes = probe_points(&points, n, 4000, 11);
        let bound = 0.35;
        let mut reference = Vec::new();
        for c in probes.chunks_exact(3) {
            reference.push(brute(&mesh, &[c[0], c[1], c[2]], bound));
        }
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
            let got = pool.install(|| locate_cpu(&mesh, &bvh, &probes, bound)).unwrap();
            for (i, (t, d)) in reference.iter().enumerate() {
                assert_eq!(got.donors[i], *t, "point {i}");
                assert!(got.distances[i] == *d || (d.is_nan() && got.distances[i].is_nan()));
            }
            assert!(got.counts.contained > 0 && got.counts.fallback > 0 && got.counts.unlocated > 0);
        }
    }

    // AI-FUNC-SUMMARY: Verify candidate-slot completion equals locate_cpu when the slots are every leaf slot whose box holds the point, including overflowed points.
    #[test]
    fn candidate_completion_equals_cpu() {
        let n = 5;
        let (points, cells) = kuhn_mesh(n, 3);
        let mesh = TetMesh::new(&points, &cells).unwrap();
        let bvh = TetBvh::build(&mesh);
        let probes = probe_points(&points, n, 2000, 5);
        let k = 6;
        let mut counts = Vec::new();
        let mut slots = Vec::new();
        for c in probes.chunks_exact(3) {
            let p = [c[0], c[1], c[2]];
            let hit: Vec<u32> = (0..bvh.order.len() as u32)
                .filter(|&s| inside_box(&p, &bvh.slot_min[s as usize], &bvh.slot_max[s as usize]))
                .collect();
            counts.push(hit.len() as u32);
            let mut row = vec![0u32; k];
            for (r, s) in row.iter_mut().zip(hit.iter()) {
                *r = *s;
            }
            slots.extend(row);
        }
        let a = locate_cpu(&mesh, &bvh, &probes, 0.3).unwrap();
        let b = locate_from_candidates(&mesh, &bvh, &probes, 0.3, &counts, &slots, k).unwrap();
        assert_eq!(a.donors, b.donors);
        assert!(b.counts.gpu_overflow > 0);
    }
}
