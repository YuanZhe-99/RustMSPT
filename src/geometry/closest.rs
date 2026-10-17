use crate::error::{Result, RustMsptError};
use rayon::prelude::*;

pub const DISTANCE_TIE: f64 = 1e-12;
const LEAF_SIZE: usize = 4;

// AI-FUNC-SUMMARY: Whether the primitives are triangles (three corners) or segments (two); carries no data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Triangles,
    Segments,
}

impl Kind {
    // AI-FUNC-SUMMARY: Corners per primitive; returns 3 for triangles, 2 for segments; side effects: none.
    pub fn corners(self) -> usize {
        match self {
            Kind::Triangles => 3,
            Kind::Segments => 2,
        }
    }
}

// AI-FUNC-SUMMARY: Triangles or segments as corner indices into a vertex list (f64); carries the data the search reads.
pub struct Primitives {
    pub kind: Kind,
    pub vertices: Vec<[f64; 3]>,
    pub corners: Vec<[u32; 3]>,
}

impl Primitives {
    // AI-FUNC-SUMMARY:
    // Purpose: Check and pack flat vertex coordinates and corner indices.
    // Inputs: kind, vertices as flat xyz, corner indices flat (3 or 2 per primitive).
    // Returns: Primitives, or an error for ragged arrays, non-finite vertices or an index out of range.
    // Side effects: None.
    // Notes: A segment is stored as [a, b, b].
    pub fn new(kind: Kind, vertices: &[f64], corners: &[u32]) -> Result<Self> {
        let k = kind.corners();
        if vertices.len() % 3 != 0 || corners.len() % k != 0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "vertices must be (n, 3) and corners (m, {k})"
            )));
        }
        if vertices.iter().any(|v| !v.is_finite()) {
            return Err(RustMsptError::InvalidConfig("vertices must be finite".into()));
        }
        let n = (vertices.len() / 3) as u32;
        if let Some(bad) = corners.iter().find(|&&c| c >= n) {
            return Err(RustMsptError::InvalidConfig(format!(
                "corner index {bad} is out of range for {n} vertices"
            )));
        }
        let vertices = vertices.chunks_exact(3).map(|v| [v[0], v[1], v[2]]).collect();
        let corners = corners
            .chunks_exact(k)
            .map(|c| if k == 3 { [c[0], c[1], c[2]] } else { [c[0], c[1], c[1]] })
            .collect();
        Ok(Self { kind, vertices, corners })
    }

    // AI-FUNC-SUMMARY: Number of primitives; returns usize; side effects: none.
    pub fn len(&self) -> usize {
        self.corners.len()
    }

    // AI-FUNC-SUMMARY: Whether there are no primitives; returns bool; side effects: none.
    pub fn is_empty(&self) -> bool {
        self.corners.is_empty()
    }

    // AI-FUNC-SUMMARY: Corner coordinates of primitive `i` (a segment's third corner repeats its second); returns three points; side effects: none.
    pub fn corner_points(&self, i: usize) -> [[f64; 3]; 3] {
        let c = self.corners[i];
        [self.vertices[c[0] as usize], self.vertices[c[1] as usize], self.vertices[c[2] as usize]]
    }

    // AI-FUNC-SUMMARY: The closest point of primitive `i` to `p` and its exact distance; returns (point, distance); side effects: none.
    pub fn closest(&self, i: usize, p: &[f64; 3]) -> ([f64; 3], f64) {
        let [a, b, c] = self.corner_points(i);
        let q = match self.kind {
            Kind::Triangles => closest_on_triangle(p, &a, &b, &c),
            Kind::Segments => closest_on_segment(p, &a, &b),
        };
        (q, norm(&sub(p, &q)))
    }

    // AI-FUNC-SUMMARY: Axis-aligned box of primitive `i`; returns (min, max); side effects: none.
    fn bounds(&self, i: usize) -> ([f64; 3], [f64; 3]) {
        let pts = self.corner_points(i);
        let mut lo = pts[0];
        let mut hi = pts[0];
        for q in &pts[1..] {
            for d in 0..3 {
                lo[d] = lo[d].min(q[d]);
                hi[d] = hi[d].max(q[d]);
            }
        }
        (lo, hi)
    }
}

// AI-FUNC-SUMMARY: a - b for 3-vectors; returns the difference; side effects: none.
fn sub(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

// AI-FUNC-SUMMARY: Dot product of 3-vectors; returns f64; side effects: none.
fn dot(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// AI-FUNC-SUMMARY: Euclidean length of a 3-vector; returns f64; side effects: none.
fn norm(a: &[f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

// AI-FUNC-SUMMARY: a + s * d for 3-vectors; returns the point; side effects: none.
fn along(a: &[f64; 3], d: &[f64; 3], s: f64) -> [f64; 3] {
    [a[0] + s * d[0], a[1] + s * d[1], a[2] + s * d[2]]
}

// AI-FUNC-SUMMARY: Closest point to `p` on segment ab (a point segment gives a); returns the point; side effects: none.
pub fn closest_on_segment(p: &[f64; 3], a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    let ab = sub(b, a);
    let len2 = dot(&ab, &ab);
    if len2 <= 0.0 {
        return *a;
    }
    let t = (dot(&sub(p, a), &ab) / len2).clamp(0.0, 1.0);
    along(a, &ab, t)
}

// AI-FUNC-SUMMARY:
// Purpose: Closest point to `p` on triangle abc, by Voronoi regions (Ericson, Real-Time Collision Detection 5.1.5).
// Inputs: the point and the triangle's corners.
// Returns: the closest point.
// Side effects: None.
// Notes: A triangle with no area (its face region's denominator not positive) gives the nearest of its three edges.
pub fn closest_on_triangle(p: &[f64; 3], a: &[f64; 3], b: &[f64; 3], c: &[f64; 3]) -> [f64; 3] {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(&ab, &ap);
    let d2 = dot(&ac, &ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return *a;
    }
    let bp = sub(p, b);
    let d3 = dot(&ab, &bp);
    let d4 = dot(&ac, &bp);
    if d3 >= 0.0 && d4 <= d3 {
        return *b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return along(a, &ab, v);
    }
    let cp = sub(p, c);
    let d5 = dot(&ab, &cp);
    let d6 = dot(&ac, &cp);
    if d6 >= 0.0 && d5 <= d6 {
        return *c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return along(a, &ac, w);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return along(b, &sub(c, b), w);
    }
    let denom = va + vb + vc;
    if !(denom > 0.0) {
        let mut best = closest_on_segment(p, a, b);
        for q in [closest_on_segment(p, b, c), closest_on_segment(p, c, a)] {
            if norm(&sub(p, &q)) < norm(&sub(p, &best)) {
                best = q;
            }
        }
        return best;
    }
    let v = vb / denom;
    let w = vc / denom;
    [
        a[0] + ab[0] * v + ac[0] * w,
        a[1] + ab[1] * v + ac[1] * w,
        a[2] + ab[2] * v + ac[2] * w,
    ]
}

// AI-FUNC-SUMMARY: Distance from `p` to the box [lo, hi] (0 inside); returns f64; side effects: none.
fn box_distance(p: &[f64; 3], lo: &[f64; 3], hi: &[f64; 3]) -> f64 {
    let mut s = 0.0;
    for d in 0..3 {
        let g = (lo[d] - p[d]).max(0.0).max(p[d] - hi[d]);
        s += g * g;
    }
    s.sqrt()
}

// AI-FUNC-SUMMARY: A static bounding-volume hierarchy over primitive boxes, stored as flat arrays so a GPU kernel can walk the same layout.
// Notes: `link[n] = [first, count]` for a leaf (slots first..first+count of `order`), `[left, 0]` for an internal node whose children are left and left+1.
pub struct PrimBvh {
    pub node_min: Vec<[f64; 3]>,
    pub node_max: Vec<[f64; 3]>,
    pub link: Vec<[u32; 2]>,
    pub order: Vec<u32>,
    pub slot_min: Vec<[f64; 3]>,
    pub slot_max: Vec<[f64; 3]>,
    pub depth: usize,
}

impl PrimBvh {
    // AI-FUNC-SUMMARY:
    // Purpose: Build the hierarchy by median splits on the longest axis of the box centres.
    // Inputs: the primitives.
    // Returns: PrimBvh with leaves of at most LEAF_SIZE primitives (more only when every centre coincides).
    // Side effects: None.
    // Notes: Deterministic: the split orders by centre then primitive index.
    pub fn build(prims: &Primitives) -> Self {
        let n = prims.len();
        let boxes: Vec<([f64; 3], [f64; 3])> = (0..n).into_par_iter().map(|i| prims.bounds(i)).collect();
        let centre = |i: u32| {
            let (lo, hi) = boxes[i as usize];
            [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5]
        };
        let mut order: Vec<u32> = (0..n as u32).collect();
        let empty = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        let mut bvh = PrimBvh {
            node_min: vec![empty.0],
            node_max: vec![empty.1],
            link: vec![[0, 0]],
            order: Vec::new(),
            slot_min: Vec::new(),
            slot_max: Vec::new(),
            depth: 0,
        };
        let mut stack = vec![(0usize, 0usize, n, 1usize)];
        while let Some((node, lo, hi, level)) = stack.pop() {
            bvh.depth = bvh.depth.max(level);
            let mut bmin = [f64::INFINITY; 3];
            let mut bmax = [f64::NEG_INFINITY; 3];
            let mut cmin = [f64::INFINITY; 3];
            let mut cmax = [f64::NEG_INFINITY; 3];
            for &i in &order[lo..hi] {
                let (a, b) = boxes[i as usize];
                let c = centre(i);
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
        bvh.slot_min = order.iter().map(|&i| boxes[i as usize].0).collect();
        bvh.slot_max = order.iter().map(|&i| boxes[i as usize].1).collect();
        bvh.order = order;
        bvh
    }

    // AI-FUNC-SUMMARY: Visit every primitive whose box is within `limit(state)` of `p`, nearer child first, calling `seen(primitive, exact distance, state)`; returns nothing; side effects: calls the closure, which may tighten the state.
    fn visit_within(
        &self,
        prims: &Primitives,
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
                let i = self.order[slot];
                let d = prims.closest(i as usize, p).1;
                seen(i, d, state);
            }
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The nearest primitive to `p` by the module's rule (least exact distance, ties within DISTANCE_TIE to the lowest index).
    // Inputs: primitives, point, an optional seed primitive whose exact distance bounds the first pass.
    // Returns: (primitive, closest point, distance); primitive is u32::MAX when there are no primitives.
    // Side effects: None.
    // Notes: Two branch-and-bound passes: the first finds the least distance, the second the lowest index within the tie band. A box is pruned only when it is farther than the bound, and the bound is never below the least distance, so a seed cannot change the answer.
    pub fn nearest(&self, prims: &Primitives, p: &[f64; 3], seed: Option<u32>) -> (u32, [f64; 3], f64) {
        if prims.is_empty() {
            return (u32::MAX, [f64::NAN; 3], f64::NAN);
        }
        let mut least = match seed {
            Some(s) if (s as usize) < prims.len() => prims.closest(s as usize, p).1,
            _ => f64::INFINITY,
        };
        self.visit_within(prims, p, |b| b, &mut |_, d, best| {
            if d < *best {
                *best = d;
            }
        }, &mut least);
        let band = least * (1.0 + DISTANCE_TIE);
        let mut chosen = u32::MAX;
        let mut fixed = band;
        self.visit_within(prims, p, |_| band, &mut |i, d, _| {
            if d <= band && i < chosen {
                chosen = i;
            }
        }, &mut fixed);
        let (q, d) = prims.closest(chosen as usize, p);
        (chosen, q, d)
    }
}

// AI-FUNC-SUMMARY: Nearest primitive, closest point and distance for every query point; carries the arrays the pipeline writes.
pub struct Nearest {
    pub primitive: Vec<i64>,
    pub closest: Vec<f64>,
    pub distance: Vec<f64>,
}

// AI-FUNC-SUMMARY:
// Purpose: Settle every query point on every core (rayon), optionally from seeds.
// Inputs: primitives, their BVH, query points as flat xyz, optional seed per point (u32::MAX for none).
// Returns: Nearest, or an error for ragged or non-finite queries or a seed array of the wrong length.
// Side effects: None.
// Notes: The answer for each point is the same with or without seeds and on any number of threads.
pub fn nearest_all(prims: &Primitives, bvh: &PrimBvh, queries: &[f64], seeds: Option<&[u32]>) -> Result<Nearest> {
    if queries.len() % 3 != 0 || queries.iter().any(|v| !v.is_finite()) {
        return Err(RustMsptError::InvalidConfig("queries must be finite and shaped (..., 3)".into()));
    }
    let n = queries.len() / 3;
    if let Some(s) = seeds {
        if s.len() != n {
            return Err(RustMsptError::InvalidConfig(format!("{} seeds for {n} queries", s.len())));
        }
    }
    let settled: Vec<(u32, [f64; 3], f64)> = (0..n)
        .into_par_iter()
        .map(|i| {
            let p = [queries[3 * i], queries[3 * i + 1], queries[3 * i + 2]];
            let seed = seeds.map(|s| s[i]).filter(|&s| s != u32::MAX);
            bvh.nearest(prims, &p, seed)
        })
        .collect();
    let mut out = Nearest {
        primitive: Vec::with_capacity(n),
        closest: Vec::with_capacity(3 * n),
        distance: Vec::with_capacity(n),
    };
    for (i, q, d) in settled {
        out.primitive.push(if i == u32::MAX { -1 } else { i64::from(i) });
        out.closest.extend_from_slice(&q);
        out.distance.push(d);
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};

    // AI-FUNC-SUMMARY: A bumpy triangulated sheet of n x n quads plus random segments, for tests; returns (vertices, triangles, segment corners).
    pub(crate) fn sheet(n: usize, seed: u64) -> (Vec<f64>, Vec<u32>, Vec<u32>) {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        let mut v = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                v.extend_from_slice(&[i as f64, j as f64, rng.gen_range(-0.3..0.3)]);
            }
        }
        let id = |i: usize, j: usize| (j * (n + 1) + i) as u32;
        let mut tri = Vec::new();
        for j in 0..n {
            for i in 0..n {
                tri.extend_from_slice(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                tri.extend_from_slice(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
        let m = (n + 1) * (n + 1);
        let seg: Vec<u32> = (0..2 * n).flat_map(|_| [rng.gen_range(0..m as u32), rng.gen_range(0..m as u32)]).collect();
        (v, tri, seg)
    }

    // AI-FUNC-SUMMARY: Random query points round the sheet, a share of them exactly on its vertices and edge midpoints (ties); returns flat xyz.
    pub(crate) fn queries(v: &[f64], n: usize, count: usize, seed: u64) -> Vec<f64> {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        let mut q = Vec::new();
        for k in 0..count {
            if k % 5 == 0 {
                let a = rng.gen_range(0..v.len() / 3);
                q.extend_from_slice(&v[3 * a..3 * a + 3]);
            } else {
                q.extend_from_slice(&[
                    rng.gen_range(-2.0..n as f64 + 2.0),
                    rng.gen_range(-2.0..n as f64 + 2.0),
                    rng.gen_range(-2.0..2.0),
                ]);
            }
        }
        q
    }

    // AI-FUNC-SUMMARY: Exhaustive search by the module's rule, for tests; returns (primitive, distance).
    pub(crate) fn brute(prims: &Primitives, p: &[f64; 3]) -> (u32, f64) {
        let d: Vec<f64> = (0..prims.len()).map(|i| prims.closest(i, p).1).collect();
        let least = d.iter().cloned().fold(f64::INFINITY, f64::min);
        let band = least * (1.0 + DISTANCE_TIE);
        let i = (0..prims.len()).find(|&i| d[i] <= band).unwrap();
        (i as u32, d[i])
    }

    // AI-FUNC-SUMMARY: Verify the triangle closest point against dense barycentric sampling.
    #[test]
    fn triangle_closest_point_beats_sampling() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(3);
        for _ in 0..200 {
            let r = |rng: &mut rand_chacha::ChaCha8Rng| [rng.gen_range(-1.0..1.0), rng.gen_range(-1.0..1.0), rng.gen_range(-1.0..1.0)];
            let (a, b, c, p) = (r(&mut rng), r(&mut rng), r(&mut rng), r(&mut rng));
            let q = closest_on_triangle(&p, &a, &b, &c);
            let d = norm(&sub(&p, &q));
            for i in 0..=40 {
                for j in 0..=(40 - i) {
                    let (u, v) = (i as f64 / 40.0, j as f64 / 40.0);
                    let s = [
                        a[0] + u * (b[0] - a[0]) + v * (c[0] - a[0]),
                        a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1]),
                        a[2] + u * (b[2] - a[2]) + v * (c[2] - a[2]),
                    ];
                    assert!(d <= norm(&sub(&p, &s)) + 1e-12);
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Verify the BVH search equals the exhaustive search, with and without seeds, for triangles and segments.
    #[test]
    fn bvh_nearest_equals_exhaustive_search() {
        let n = 12;
        let (v, tri, seg) = sheet(n, 7);
        let q = queries(&v, n, 3000, 9);
        for (kind, corners) in [(Kind::Triangles, &tri), (Kind::Segments, &seg)] {
            let prims = Primitives::new(kind, &v, corners).unwrap();
            let bvh = PrimBvh::build(&prims);
            let plain = nearest_all(&prims, &bvh, &q, None).unwrap();
            let seeds: Vec<u32> = (0..q.len() / 3).map(|i| (i * 7919 % prims.len()) as u32).collect();
            let seeded = nearest_all(&prims, &bvh, &q, Some(&seeds)).unwrap();
            assert_eq!(plain.primitive, seeded.primitive);
            assert_eq!(plain.distance, seeded.distance);
            for i in 0..q.len() / 3 {
                let (b, d) = brute(&prims, &[q[3 * i], q[3 * i + 1], q[3 * i + 2]]);
                assert_eq!(plain.primitive[i], i64::from(b), "query {i}");
                assert_eq!(plain.distance[i], d);
            }
        }
    }

    // AI-FUNC-SUMMARY: Verify bad inputs are refused.
    #[test]
    fn refuses_bad_inputs() {
        assert!(Primitives::new(Kind::Triangles, &[0.0; 9], &[0, 1, 3]).is_err());
        assert!(Primitives::new(Kind::Segments, &[0.0; 8], &[0, 1]).is_err());
        assert!(Primitives::new(Kind::Triangles, &[f64::NAN; 9], &[0, 1, 2]).is_err());
        let prims = Primitives::new(Kind::Triangles, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], &[0, 1, 2]).unwrap();
        let bvh = PrimBvh::build(&prims);
        assert!(nearest_all(&prims, &bvh, &[0.0, 0.0], None).is_err());
        assert!(nearest_all(&prims, &bvh, &[0.0, 0.0, 0.0], Some(&[0, 0])).is_err());
    }
}
