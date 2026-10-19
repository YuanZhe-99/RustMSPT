use crate::error::{Result, RustMsptError};
use crate::geometry::closest::closest_on_triangle;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const FLAT: f64 = 1e-30;
const JOINT_TRIES: usize = 20000;
const PENALTY: f64 = 1e6;

// AI-FUNC-SUMMARY: Parameters of one repair: the aspect-ratio limit, flip sweeps, how far a wall or layer node may slide, the padding density, the joint pass's tolerance and seed; carries no behaviour.
#[derive(Debug, Clone)]
pub struct RepairParams {
    pub limit: f64,
    pub sweeps: usize,
    pub wall_tol: f64,
    pub padding: Option<i64>,
    pub joint_tol: Option<f64>,
    pub seed: u64,
}

// AI-FUNC-SUMMARY: One flip taken: its kind ("2-3", "3-2", "4-4"), where (mean of the first replaced cell's corners) and the worst ratio before and after; carries no behaviour.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Flip {
    pub kind: String,
    pub at: [f64; 3],
    pub worst: [f64; 2],
}

// AI-FUNC-SUMMARY: One node moved: which, whether along a wall or a layer, how far, the excess before (None: inverted) and after, and whether by the joint pass; carries no behaviour.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Moved {
    pub node: u32,
    pub on_wall: bool,
    pub on_layer: bool,
    pub by: f64,
    pub excess: [Option<f64>; 2],
    pub joint: bool,
}

// AI-FUNC-SUMMARY: A cell given to the padding before the repair (flat, touching the padding): its index, former density and centre; carries no behaviour.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToPadding {
    pub cell: u32,
    pub from: i64,
    pub at: [f64; 3],
}

// AI-FUNC-SUMMARY: What a repair did and left: the cells bad before, cells given to the padding, flips, nodes moved, and the cells inverted or over the limit after with the worst ratio; carries no behaviour.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RepairReport {
    pub bad_before: usize,
    pub to_padding: Vec<ToPadding>,
    pub flips: Vec<Flip>,
    pub nodes_moved: Vec<Moved>,
    pub inverted_after: usize,
    pub over_limit_after: usize,
    pub ratio_max_after: f64,
}

// AI-FUNC-SUMMARY: The repaired mesh: points, cells, densities, and the report; carries no behaviour.
pub struct Repaired {
    pub points: Vec<[f64; 3]>,
    pub tets: Vec<[u32; 4]>,
    pub density: Vec<i64>,
    pub report: RepairReport,
}

// AI-FUNC-SUMMARY: a - b; returns the 3-vector; side effects: none.
fn sub(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

// AI-FUNC-SUMMARY: a x b; returns the 3-vector; side effects: none.
fn cross(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

// AI-FUNC-SUMMARY: a . b; returns f64; side effects: none.
fn dot(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// AI-FUNC-SUMMARY: |a|; returns f64; side effects: none.
fn norm(a: &[f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

// AI-FUNC-SUMMARY:
// Purpose: YAFEQ's aspect ratio of a tetrahedron (circumradius over three times the inradius; 1 for a regular cell) and its signed volume.
// Inputs: the four corners.
// Returns: (ratio, volume); the ratio is infinite when |volume| <= FLAT.
// Side effects: None.
// Notes: The same formula as simHelper's `yafeq_aspect_ratio` and YAFEQ's MESH_verification.
pub fn aspect_ratio(q: &[[f64; 3]; 4]) -> (f64, f64) {
    let (a, b, c, d) = (&q[0], &q[1], &q[2], &q[3]);
    let volume = dot(&cross(&sub(b, a), &sub(c, a)), &sub(d, a)) / 6.0;
    let mut area = 0.0;
    for (i, j, k) in [(0, 1, 2), (0, 1, 3), (0, 2, 3), (1, 2, 3)] {
        area += 0.5 * norm(&cross(&sub(&q[j], &q[i]), &sub(&q[k], &q[i])));
    }
    if volume.abs() <= FLAT {
        return (f64::INFINITY, volume);
    }
    let (u, v, w) = (sub(a, d), sub(b, d), sub(c, d));
    let (vw, wu, uv) = (cross(&v, &w), cross(&w, &u), cross(&u, &v));
    let (su, sv, sw) = (dot(&u, &u), dot(&v, &v), dot(&w, &w));
    let num = [
        su * vw[0] + sv * wu[0] + sw * uv[0],
        su * vw[1] + sv * wu[1] + sw * uv[1],
        su * vw[2] + sv * wu[2] + sw * uv[2],
    ];
    let circum = norm(&num) / (12.0 * volume.abs());
    let inner = 3.0 * volume.abs() / area;
    (circum / (3.0 * inner), volume)
}

// AI-FUNC-SUMMARY: Distance from x to the nearest of the triangles; returns f64 (infinite for none); side effects: none.
fn distance_to_triangles(x: &[f64; 3], tri: &[[[f64; 3]; 3]]) -> f64 {
    tri.iter()
        .map(|t| norm(&sub(x, &closest_on_triangle(x, &t[0], &t[1], &t[2]))))
        .fold(f64::INFINITY, f64::min)
}

// AI-FUNC-SUMMARY: numpy's isclose with its defaults (rtol 1e-5, atol 1e-8); returns bool; side effects: none.
fn isclose(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-8 + 1e-5 * b.abs()
}

// AI-FUNC-SUMMARY:
// Purpose: Nelder-Mead minimisation as scipy's (non-adaptive: reflection 1, expansion 2, contraction 0.5, shrink 0.5; initial simplex 5 % per coordinate, 0.00025 for a zero coordinate).
// Inputs: objective, start, xatol, fatol, maxiter.
// Returns: (best point, its value).
// Side effects: Calls the objective.
// Notes: Stops when every vertex is within xatol of the best and every value within fatol, or after maxiter iterations.
pub fn nelder_mead(f: &mut dyn FnMut(&[f64]) -> f64, x0: &[f64], xatol: f64, fatol: f64, maxiter: usize) -> (Vec<f64>, f64) {
    let n = x0.len();
    let mut sim: Vec<Vec<f64>> = vec![x0.to_vec()];
    for k in 0..n {
        let mut y = x0.to_vec();
        y[k] = if y[k] != 0.0 { 1.05 * y[k] } else { 0.00025 };
        sim.push(y);
    }
    let mut fsim: Vec<f64> = sim.iter().map(|x| f(x)).collect();
    let sort = |sim: &mut Vec<Vec<f64>>, fsim: &mut Vec<f64>| {
        let mut ind: Vec<usize> = (0..fsim.len()).collect();
        ind.sort_by(|&a, &b| fsim[a].total_cmp(&fsim[b]));
        *sim = ind.iter().map(|&i| sim[i].clone()).collect();
        *fsim = ind.iter().map(|&i| fsim[i]).collect();
    };
    sort(&mut sim, &mut fsim);
    let mut iterations = 1;
    while iterations < maxiter {
        let xspread = sim[1..].iter().flat_map(|v| v.iter().zip(&sim[0]).map(|(a, b)| (a - b).abs())).fold(0.0, f64::max);
        let fspread = fsim[1..].iter().map(|v| (fsim[0] - v).abs()).fold(0.0, f64::max);
        if xspread <= xatol && fspread <= fatol {
            break;
        }
        let xbar: Vec<f64> = (0..n).map(|d| sim[..n].iter().map(|v| v[d]).sum::<f64>() / n as f64).collect();
        let last = sim[n].clone();
        let at = |s: f64| -> Vec<f64> { (0..n).map(|d| (1.0 + s) * xbar[d] - s * last[d]).collect() };
        let xr = at(1.0);
        let fxr = f(&xr);
        let mut shrink = false;
        if fxr < fsim[0] {
            let xe = at(2.0);
            let fxe = f(&xe);
            if fxe < fxr {
                sim[n] = xe;
                fsim[n] = fxe;
            } else {
                sim[n] = xr;
                fsim[n] = fxr;
            }
        } else if fxr < fsim[n - 1] {
            sim[n] = xr;
            fsim[n] = fxr;
        } else if fxr < fsim[n] {
            let xc = at(0.5);
            let fxc = f(&xc);
            if fxc <= fxr {
                sim[n] = xc;
                fsim[n] = fxc;
            } else {
                shrink = true;
            }
        } else {
            let xcc: Vec<f64> = (0..n).map(|d| 0.5 * xbar[d] + 0.5 * last[d]).collect();
            let fxcc = f(&xcc);
            if fxcc < fsim[n] {
                sim[n] = xcc;
                fsim[n] = fxcc;
            } else {
                shrink = true;
            }
        }
        if shrink {
            for j in 1..=n {
                sim[j] = (0..n).map(|d| sim[0][d] + 0.5 * (sim[j][d] - sim[0][d])).collect();
                fsim[j] = f(&sim[j]);
            }
        }
        iterations += 1;
        sort(&mut sim, &mut fsim);
    }
    (sim[0].clone(), fsim[0])
}

// AI-FUNC-SUMMARY: One standard normal draw by Box-Muller; returns f64; side effects: advances the generator.
fn normal(rng: &mut rand_chacha::ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

struct State<'a> {
    p: Vec<[f64; 3]>,
    cells: Vec<[u32; 4]>,
    dens: Vec<i64>,
    alive: Vec<bool>,
    by_node: Vec<Vec<u32>>,
    pinned: &'a [bool],
    on_layer: HashMap<u32, Vec<[u32; 3]>>,
    lo: [f64; 3],
    hi: [f64; 3],
    prm: &'a RepairParams,
    watch: BTreeSet<u32>,
    flips: Vec<Flip>,
    moved: Vec<Moved>,
}

impl State<'_> {
    // AI-FUNC-SUMMARY: Ratio (infinite for no volume) and signed volume of cell corners t; returns (f64, f64); side effects: none.
    fn quality(&self, t: &[u32; 4]) -> (f64, f64) {
        let q = [self.p[t[0] as usize], self.p[t[1] as usize], self.p[t[2] as usize], self.p[t[3] as usize]];
        let (r, v) = aspect_ratio(&q);
        (if v > FLAT { r } else { f64::INFINITY }, v)
    }

    // AI-FUNC-SUMMARY: Signed volume of cell corners t; returns f64; side effects: none.
    fn svol(&self, t: &[u32; 4]) -> f64 {
        let p = |i: usize| &self.p[t[i] as usize];
        dot(&cross(&sub(p(1), p(0)), &sub(p(2), p(0))), &sub(p(3), p(0))) / 6.0
    }

    // AI-FUNC-SUMMARY: Live cells holding every node of `nodes`, ascending; returns Vec<u32>; side effects: none.
    fn holding(&self, nodes: &[u32]) -> Vec<u32> {
        let mut out: Vec<u32> = self.by_node[nodes[0] as usize]
            .iter()
            .copied()
            .filter(|&j| self.alive[j as usize] && nodes[1..].iter().all(|&n| self.cells[j as usize].contains(&n)))
            .collect();
        out.sort_unstable();
        out
    }

    // AI-FUNC-SUMMARY: Live cells round node n, ascending; returns Vec<u32>; side effects: none.
    fn star(&self, n: u32) -> Vec<u32> {
        self.holding(&[n])
    }

    // AI-FUNC-SUMMARY: Whether every node of a face is held fixed; returns bool; side effects: none.
    fn pinned_face(&self, f: &[u32]) -> bool {
        f.iter().all(|&n| self.pinned[n as usize])
    }

    // AI-FUNC-SUMMARY: Whether node n lies on the bounding box of the mesh as given; returns bool; side effects: none.
    fn on_box(&self, n: u32) -> bool {
        let x = self.p[n as usize];
        (0..3).any(|d| isclose(x[d], self.lo[d]) || isclose(x[d], self.hi[d]))
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The flips available for cell i, as (replaced cells, new cells): 2-3 through each face shared with one cell of its density, 3-2 and 4-4 round each edge whose ring is all of its density.
    // Inputs: the cell.
    // Returns: the candidates, in the order simHelper's flip_repair yields them.
    // Side effects: None.
    // Notes: Never through a face whose nodes are all fixed, round an edge whose two nodes are fixed, or removing a face of the edge and a ring node that is all fixed.
    fn options(&self, i: u32) -> Vec<(Vec<u32>, Vec<[u32; 4]>)> {
        let t = self.cells[i as usize];
        let di = self.dens[i as usize];
        let mut out = Vec::new();
        for f in [[t[0], t[1], t[2]], [t[0], t[1], t[3]], [t[0], t[2], t[3]], [t[1], t[2], t[3]]] {
            let nb: Vec<u32> = self.holding(&f).into_iter().filter(|&j| j != i).collect();
            if nb.len() != 1 || self.dens[nb[0] as usize] != di || self.pinned_face(&f) {
                continue;
            }
            let j = nb[0];
            let a = *t.iter().find(|n| !f.contains(n)).unwrap();
            let b = *self.cells[j as usize].iter().find(|n| !f.contains(n)).unwrap();
            out.push((vec![i, j], vec![[a, b, f[0], f[1]], [a, b, f[1], f[2]], [a, b, f[2], f[0]]]));
        }
        for e in [[t[0], t[1]], [t[0], t[2]], [t[0], t[3]], [t[1], t[2]], [t[1], t[3]], [t[2], t[3]]] {
            let ring = self.holding(&e);
            if !(ring.len() == 3 || ring.len() == 4) || ring.iter().any(|&j| self.dens[j as usize] != di) {
                continue;
            }
            if self.pinned[e[0] as usize] && self.pinned[e[1] as usize] {
                continue;
            }
            let rest: Vec<u32> = ring
                .iter()
                .flat_map(|&j| self.cells[j as usize].iter().copied().filter(|n| !e.contains(n)))
                .collect::<BTreeSet<u32>>()
                .into_iter()
                .collect();
            if rest.iter().any(|&o| self.pinned_face(&[e[0], e[1], o])) {
                continue;
            }
            if ring.len() == 3 && rest.len() == 3 {
                out.push((ring.clone(), vec![[e[0], rest[0], rest[1], rest[2]], [e[1], rest[0], rest[1], rest[2]]]));
            } else if ring.len() == 4 && rest.len() == 4 {
                let mut adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
                for &j in &ring {
                    let o: Vec<u32> = self.cells[j as usize].iter().copied().filter(|n| !e.contains(n)).collect();
                    adj.entry(o[0]).or_default().insert(o[1]);
                    adj.entry(o[1]).or_default().insert(o[0]);
                }
                if adj.values().any(|s| s.len() != 2) {
                    continue;
                }
                let mut cycle = vec![rest[0], *adj[&rest[0]].iter().next().unwrap()];
                while cycle.len() < 4 {
                    let prev = cycle[cycle.len() - 2];
                    let next = *adj[&cycle[cycle.len() - 1]].iter().find(|&&n| n != prev).unwrap();
                    cycle.push(next);
                }
                for s in 0..2 {
                    let c: Vec<u32> = (0..4).map(|k| cycle[(s + k) % 4]).collect();
                    out.push((
                        ring.clone(),
                        vec![[e[0], c[0], c[1], c[2]], [e[0], c[0], c[2], c[3]], [e[1], c[0], c[1], c[2]], [e[1], c[0], c[2], c[3]]],
                    ));
                }
            }
        }
        out
    }

    // AI-FUNC-SUMMARY: Cell corners wound to positive volume (first two swapped otherwise); returns [u32; 4]; side effects: none.
    fn oriented(&self, t: [u32; 4]) -> [u32; 4] {
        if self.quality(&t).1 > 0.0 {
            t
        } else {
            [t[1], t[0], t[2], t[3]]
        }
    }

    // AI-FUNC-SUMMARY: Retire cells `old` and add `new` of density `d`, keeping the node index and the watch list; returns nothing; side effects: mutates the mesh state.
    fn replace(&mut self, old: &[u32], new: &[[u32; 4]], d: i64) {
        for &j in old {
            self.alive[j as usize] = false;
            for &n in &self.cells[j as usize] {
                self.by_node[n as usize].retain(|&c| c != j);
            }
        }
        for x in new {
            let k = self.cells.len() as u32;
            self.cells.push(*x);
            self.dens.push(d);
            self.alive.push(true);
            for &n in x {
                self.by_node[n as usize].push(k);
            }
            self.watch.insert(k);
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Sweeps of flips, worst watched cell first, each taking the candidate with the lowest worst new ratio when every new cell is positive, the signed volume is kept and the worst ratio drops.
    // Inputs: none (the state).
    // Returns: whether any flip was taken.
    // Side effects: Flips cells, extends the watch list and the flip log.
    fn flip_pass(&mut self) -> bool {
        let mut any = false;
        for _ in 0..self.prm.sweeps {
            let mut order: Vec<(f64, u32)> = self
                .watch
                .iter()
                .map(|&i| (if self.alive[i as usize] { -self.quality(&self.cells[i as usize]).0 } else { 0.0 }, i))
                .collect();
            order.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut changed = false;
            for (_, i) in order {
                if !self.alive[i as usize] || self.quality(&self.cells[i as usize]).0 <= self.prm.limit {
                    continue;
                }
                let mut best: Option<(f64, Vec<u32>, Vec<[u32; 4]>, f64)> = None;
                for (old, new) in self.options(i) {
                    let new: Vec<[u32; 4]> = new.into_iter().map(|x| self.oriented(x)).collect();
                    if new.iter().any(|x| self.quality(x).1 <= FLAT) {
                        continue;
                    }
                    let before: f64 = old.iter().map(|&j| self.svol(&self.cells[j as usize])).sum();
                    let after: f64 = new.iter().map(|x| self.svol(x)).sum();
                    if (after - before).abs() > 1e-9 * before.abs().max(1e-12) + 1e-15 {
                        continue;
                    }
                    let worst_old = old.iter().map(|&j| self.quality(&self.cells[j as usize]).0).fold(f64::NEG_INFINITY, f64::max);
                    let worst_new = new.iter().map(|x| self.quality(x).0).fold(f64::NEG_INFINITY, f64::max);
                    if worst_new < worst_old && best.as_ref().is_none_or(|b| worst_new < b.0) {
                        best = Some((worst_new, old, new, worst_old));
                    }
                }
                let Some((worst_new, old, new, worst_old)) = best else { continue };
                let c = self.cells[old[0] as usize];
                let at = std::array::from_fn(|d| (0..4).map(|k| self.p[c[k] as usize][d]).sum::<f64>() / 4.0);
                let d = self.dens[i as usize];
                self.replace(&old, &new, d);
                self.flips.push(Flip { kind: format!("{}-{}", old.len(), new.len()), at, worst: [worst_old, worst_new] });
                changed = true;
                any = true;
            }
            if !changed {
                break;
            }
        }
        any
    }

    // AI-FUNC-SUMMARY: The faces round node n between its two densities, as corner coordinates; returns the triangles; side effects: none.
    fn wall_faces(&self, n: u32, star: &[u32]) -> Vec<[[f64; 3]; 3]> {
        let mut seen: BTreeMap<[u32; 3], BTreeSet<i64>> = BTreeMap::new();
        for &j in star {
            let c = self.cells[j as usize];
            for f in [[c[0], c[1], c[2]], [c[0], c[1], c[3]], [c[0], c[2], c[3]], [c[1], c[2], c[3]]] {
                if f.contains(&n) {
                    let mut k = f;
                    k.sort_unstable();
                    seen.entry(k).or_default().insert(self.dens[j as usize]);
                }
            }
        }
        seen.into_iter().filter(|(_, d)| d.len() == 2).map(|(f, _)| self.corners(&f)).collect()
    }

    // AI-FUNC-SUMMARY: Corner coordinates of a face; returns three points; side effects: none.
    fn corners(&self, f: &[u32; 3]) -> [[f64; 3]; 3] {
        [self.p[f[0] as usize], self.p[f[1] as usize], self.p[f[2] as usize]]
    }

    // AI-FUNC-SUMMARY: Total excess over the limit of the cells round node n with n at x (PENALTY when one is inverted or n is farther than wall_tol off `wall`); returns f64; side effects: none.
    fn excess(&self, x: &[f64; 3], n: u32, star: &[u32], wall: Option<&[[[f64; 3]; 3]]>) -> f64 {
        if let Some(w) = wall {
            if distance_to_triangles(x, w) > self.prm.wall_tol {
                return PENALTY;
            }
        }
        let mut total = 0.0;
        for &j in star {
            let c = self.cells[j as usize];
            let q: [[f64; 3]; 4] = std::array::from_fn(|k| if c[k] == n { *x } else { self.p[c[k] as usize] });
            let (r, v) = aspect_ratio(&q);
            if v <= FLAT {
                return PENALTY;
            }
            total += (r - self.prm.limit).max(0.0);
        }
        total
    }

    // AI-FUNC-SUMMARY:
    // Purpose: For each watched cell still bad, move one of its nodes (free; or with wall_tol > 0 along its wall, or a fixed layer node along its layer) to where the cells round it are all positive and their total excess is least, by Nelder-Mead from the node and from the mean of its neighbours.
    // Inputs: none (the state).
    // Returns: whether any node moved.
    // Side effects: Moves nodes, extends the watch list and the move log.
    fn move_pass(&mut self) -> bool {
        let mut any = false;
        let watched: Vec<u32> = self.watch.iter().copied().collect();
        for i in watched {
            if !self.alive[i as usize] || self.quality(&self.cells[i as usize]).0 <= self.prm.limit {
                continue;
            }
            for n in self.cells[i as usize] {
                let star = self.star(n);
                let kinds = star.iter().map(|&j| self.dens[j as usize]).collect::<BTreeSet<_>>().len();
                let slide = self.pinned[n as usize] && kinds == 1 && self.on_layer.contains_key(&n);
                if (self.pinned[n as usize] && !slide) || !(kinds == 1 || (kinds == 2 && self.prm.wall_tol > 0.0)) {
                    continue;
                }
                if self.on_box(n) {
                    continue;
                }
                let start = self.p[n as usize];
                let wall: Option<Vec<[[f64; 3]; 3]>> = if slide {
                    Some(self.on_layer[&n].iter().map(|f| self.corners(f)).collect())
                } else if kinds == 2 {
                    Some(self.wall_faces(n, &star))
                } else {
                    None
                };
                let now = self.excess(&start, n, &star, wall.as_deref());
                let mut centre = [0.0; 3];
                let mut count = 0.0;
                for &j in &star {
                    for &m in &self.cells[j as usize] {
                        if m != n {
                            for d in 0..3 {
                                centre[d] += self.p[m as usize][d];
                            }
                            count += 1.0;
                        }
                    }
                }
                let centre = centre.map(|c| c / count);
                let mut best = (now, start);
                for s0 in [start, centre] {
                    let mut f = |x: &[f64]| self.excess(&[x[0], x[1], x[2]], n, &star, wall.as_deref());
                    let (x1, _) = nelder_mead(&mut f, &s0, 1e-6, 1e-6, 4000);
                    let (x2, f2) = nelder_mead(&mut f, &x1, 1e-7, 1e-7, 4000);
                    if f2 < best.0 {
                        best = (f2, [x2[0], x2[1], x2[2]]);
                    }
                }
                if best.0 < now - 1e-9 && best.0 < 1e5 {
                    self.p[n as usize] = best.1;
                    self.watch.extend(star.iter().copied());
                    self.moved.push(Moved {
                        node: n,
                        on_wall: wall.is_some() && !slide,
                        on_layer: slide,
                        by: norm(&sub(&best.1, &start)),
                        excess: [if now >= 1e5 { None } else { Some(now) }, Some(best.0)],
                        joint: false,
                    });
                    any = true;
                }
                if self.quality(&self.cells[i as usize]).0 <= self.prm.limit {
                    break;
                }
            }
        }
        any
    }

    // AI-FUNC-SUMMARY: Live cells of no or negative volume, ascending; returns Vec<u32>; side effects: none.
    fn inverted(&self) -> Vec<u32> {
        (0..self.cells.len() as u32)
            .into_par_iter()
            .filter(|&i| self.alive[i as usize] && self.quality(&self.cells[i as usize]).1 <= 0.0)
            .collect()
    }

    // AI-FUNC-SUMMARY: Whether node n may move in the joint pass, and the face sets it must stay within the joint tolerance of (its wall, its layer faces, or both); returns (bool, Vec<triangles>); side effects: none.
    fn constraint(&self, n: u32, tol: f64) -> (bool, Vec<Vec<[[f64; 3]; 3]>>) {
        let star = self.star(n);
        let kinds = star.iter().map(|&j| self.dens[j as usize]).collect::<BTreeSet<_>>().len();
        if self.on_box(n) || kinds > 2 {
            return (false, vec![]);
        }
        let mut keep = Vec::new();
        if kinds == 2 {
            if tol <= 0.0 {
                return (false, vec![]);
            }
            keep.push(self.wall_faces(n, &star));
        }
        if self.pinned[n as usize] {
            let Some(faces) = self.on_layer.get(&n).filter(|_| tol > 0.0) else {
                return (false, vec![]);
            };
            keep.push(faces.iter().map(|f| self.corners(f)).collect());
        }
        (true, keep)
    }

    // AI-FUNC-SUMMARY: (inverted cells, total excess) of cells `star` with the nodes `free` at `xyz`; returns (usize, f64); side effects: none.
    fn score(&self, xyz: &[f64], free: &[u32], star: &[u32]) -> (usize, f64) {
        let mut bad = 0;
        let mut total = 0.0;
        for &j in star {
            let c = self.cells[j as usize];
            let q: [[f64; 3]; 4] = std::array::from_fn(|k| match free.iter().position(|&m| m == c[k]) {
                Some(s) => [xyz[3 * s], xyz[3 * s + 1], xyz[3 * s + 2]],
                None => self.p[c[k] as usize],
            });
            let (r, v) = aspect_ratio(&q);
            if v <= FLAT {
                bad += 1;
            } else {
                total += (r - self.prm.limit).max(0.0);
            }
        }
        (bad, total)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Cells still inverted, in groups sharing nodes: move every node of a group the constraint allows at once (random search shrinking from 0.4 of the group's mean edge, then Nelder-Mead) to where none round them is inverted and their total excess is least.
    // Inputs: none (the state).
    // Returns: whether any group moved.
    // Side effects: Moves nodes, extends the watch list and the move log.
    // Notes: The draws come from a ChaCha8 generator seeded with params.seed, so a run is reproducible.
    fn joint_pass(&mut self) -> bool {
        let tol = self.prm.joint_tol.unwrap_or(self.prm.wall_tol);
        let mut groups: Vec<BTreeSet<u32>> = Vec::new();
        for i in self.inverted() {
            let nodes: BTreeSet<u32> = self.cells[i as usize].iter().copied().collect();
            let (hit, mut rest): (Vec<_>, Vec<_>) = groups.into_iter().partition(|g| !g.is_disjoint(&nodes));
            let mut merged = nodes;
            for g in hit {
                merged.extend(g);
            }
            rest.push(merged);
            groups = rest;
        }
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(self.prm.seed);
        let mut any = false;
        for nodes in groups {
            let mut free = Vec::new();
            let mut keeps = Vec::new();
            for &n in &nodes {
                let (ok, keep) = self.constraint(n, tol);
                if ok {
                    free.push(n);
                    keeps.push(keep);
                }
            }
            if free.is_empty() {
                continue;
            }
            let star: Vec<u32> = free.iter().flat_map(|&n| self.star(n)).collect::<BTreeSet<_>>().into_iter().collect();
            let start: Vec<f64> = free.iter().flat_map(|&n| self.p[n as usize]).collect();
            let allowed = |xyz: &[f64]| -> bool {
                keeps.iter().enumerate().all(|(s, keep)| {
                    let x = [xyz[3 * s], xyz[3 * s + 1], xyz[3 * s + 2]];
                    keep.iter().all(|w| distance_to_triangles(&x, w) <= tol)
                })
            };
            let mut edges = 0.0;
            let mut count = 0.0;
            for &j in &star {
                let c = self.cells[j as usize];
                for (a, b) in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
                    edges += norm(&sub(&self.p[c[a] as usize], &self.p[c[b] as usize]));
                    count += 1.0;
                }
            }
            let edge = edges / count;
            let at_start = self.score(&start, &free, &star);
            let mut best = (at_start, start.clone());
            for k in 0..JOINT_TRIES {
                let sigma = edge * (0.4 * (1.0 - k as f64 / JOINT_TRIES as f64) + 0.02);
                let trial: Vec<f64> = best.1.iter().map(|x| x + sigma * normal(&mut rng)).collect();
                if !allowed(&trial) {
                    continue;
                }
                let s = self.score(&trial, &free, &star);
                if s.0 < best.0 .0 || (s.0 == best.0 .0 && s.1 < best.0 .1) {
                    best = (s, trial);
                }
            }
            if best.0 .0 == 0 {
                let mut f = |x: &[f64]| {
                    let s = self.score(x, &free, &star);
                    s.1 + if s.0 > 0 || !allowed(x) { PENALTY } else { 0.0 }
                };
                let (x, fx) = nelder_mead(&mut f, &best.1, 1e-7, 1e-7, 4000);
                if fx < best.0 .1 {
                    best = (self.score(&x, &free, &star), x);
                }
            }
            let better = best.0 .0 < at_start.0 || (best.0 .0 == at_start.0 && best.0 .1 < at_start.1);
            if better && best.0 .0 == 0 {
                for (s, &n) in free.iter().enumerate() {
                    let x = [best.1[3 * s], best.1[3 * s + 1], best.1[3 * s + 2]];
                    let x0 = self.p[n as usize];
                    self.p[n as usize] = x;
                    let kinds = self.star(n).iter().map(|&j| self.dens[j as usize]).collect::<BTreeSet<_>>().len();
                    self.moved.push(Moved {
                        node: n,
                        on_wall: kinds == 2,
                        on_layer: self.pinned[n as usize],
                        by: norm(&sub(&x, &x0)),
                        excess: [None, Some(best.0 .1)],
                        joint: true,
                    });
                }
                self.watch.extend(star.iter().copied());
                any = true;
            }
        }
        any
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Repair inverted cells and cells over the aspect-ratio limit by simHelper's flip_repair rule: padding flats first, then flips and single-node moves alternating (at most four times), then a joint pass for cells still inverted and flips and moves again.
// Inputs: points, cells, densities, fixed nodes (a cohesive layer's), optional layer faces, parameters.
// Returns: Repaired (points, live cells in order, densities, report), or an error for ragged inputs or an index out of range.
// Side effects: None.
// Notes: Never removes a face or edge whose nodes are all fixed, never flips between densities, never leaves a cell over the limit it was not over. Ties in the order cells are visited go to the lower index, so a run is deterministic; simHelper's Python visits tied cells in set order, so the two may take different but equally valid repairs.
pub fn repair(points: &[f64], tets: &[u32], density: &[i64], fixed: &[bool], layer: &[u32], prm: &RepairParams) -> Result<Repaired> {
    let n = points.len() / 3;
    let m = tets.len() / 4;
    if points.len() % 3 != 0 || tets.len() % 4 != 0 || density.len() != m || fixed.len() != n || layer.len() % 3 != 0 {
        return Err(RustMsptError::InvalidConfig("tet-repair needs points (n,3), tets (m,4), density (m), fixed (n), layer (k,3)".into()));
    }
    if tets.iter().chain(layer).any(|&v| v as usize >= n) {
        return Err(RustMsptError::InvalidConfig("a node index is out of range".into()));
    }
    let p: Vec<[f64; 3]> = points.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    let cells: Vec<[u32; 4]> = tets.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    let mut by_node: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (i, c) in cells.iter().enumerate() {
        for &v in c {
            by_node[v as usize].push(i as u32);
        }
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for x in &p {
        for d in 0..3 {
            lo[d] = lo[d].min(x[d]);
            hi[d] = hi[d].max(x[d]);
        }
    }
    let mut on_layer: HashMap<u32, Vec<[u32; 3]>> = HashMap::new();
    if prm.wall_tol > 0.0 {
        for f in layer.chunks_exact(3) {
            for &v in f {
                on_layer.entry(v).or_default().push([f[0], f[1], f[2]]);
            }
        }
    }
    let mut st = State {
        p,
        cells,
        dens: density.to_vec(),
        alive: vec![true; m],
        by_node,
        pinned: fixed,
        on_layer,
        lo,
        hi,
        prm,
        watch: BTreeSet::new(),
        flips: Vec::new(),
        moved: Vec::new(),
    };
    let first: Vec<(f64, f64)> = st.cells.par_iter().map(|c| {
        let q = [st.p[c[0] as usize], st.p[c[1] as usize], st.p[c[2] as usize], st.p[c[3] as usize]];
        aspect_ratio(&q)
    }).collect();
    let bad0: Vec<u32> = (0..m as u32).filter(|&i| first[i as usize].0 > prm.limit || first[i as usize].1 <= 0.0).collect();
    let mut to_padding = Vec::new();
    if let Some(pad) = prm.padding {
        for &i in &bad0 {
            let c = st.cells[i as usize];
            let mut size: f64 = 0.0;
            for (a, b) in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
                size = size.max(norm(&sub(&st.p[c[a] as usize], &st.p[c[b] as usize])));
            }
            if st.dens[i as usize] == pad || first[i as usize].1.abs() > 1e-9 * size.powi(3) {
                continue;
            }
            let touching = [[c[0], c[1], c[2]], [c[0], c[1], c[3]], [c[0], c[2], c[3]], [c[1], c[2], c[3]]]
                .iter()
                .flat_map(|f| st.holding(f))
                .filter(|&j| j != i)
                .any(|j| st.dens[j as usize] == pad);
            // every corner on the padding too: a flat cell reaching into the body by a corner
            // would leave, once given away, a slit of no thickness in the body's outer surface
            let on_padding = c.iter().all(|&v| st.by_node[v as usize].iter().any(|&j| st.dens[j as usize] == pad));
            if touching && on_padding {
                let at = std::array::from_fn(|d| (0..4).map(|k| st.p[c[k] as usize][d]).sum::<f64>() / 4.0);
                to_padding.push(ToPadding { cell: i, from: st.dens[i as usize], at });
                st.dens[i as usize] = pad;
            }
        }
    }
    st.watch = bad0.iter().copied().filter(|&i| prm.padding.is_none_or(|pad| st.dens[i as usize] != pad)).collect();
    for _ in 0..4 {
        let flipped = st.flip_pass();
        let shifted = st.move_pass();
        if !(flipped || shifted) {
            break;
        }
    }
    if !st.inverted().is_empty() && st.joint_pass() {
        for _ in 0..4 {
            let flipped = st.flip_pass();
            let shifted = st.move_pass();
            if !(flipped || shifted) {
                break;
            }
        }
    }
    let keep: Vec<usize> = (0..st.cells.len()).filter(|&i| st.alive[i]).collect();
    let tets_out: Vec<[u32; 4]> = keep.iter().map(|&i| st.cells[i]).collect();
    let dens_out: Vec<i64> = keep.iter().map(|&i| st.dens[i]).collect();
    let after: Vec<(f64, f64)> = tets_out
        .par_iter()
        .zip(dens_out.par_iter())
        .filter(|(_, &d)| prm.padding.is_none_or(|pad| d != pad))
        .map(|(c, _)| aspect_ratio(&[st.p[c[0] as usize], st.p[c[1] as usize], st.p[c[2] as usize], st.p[c[3] as usize]]))
        .collect();
    let report = RepairReport {
        bad_before: bad0.len(),
        to_padding,
        flips: st.flips,
        nodes_moved: st.moved,
        inverted_after: after.iter().filter(|q| q.1 <= 0.0).count(),
        over_limit_after: after.iter().filter(|q| q.0 > prm.limit).count(),
        ratio_max_after: after.iter().map(|q| q.0).fold(0.0, f64::max),
    };
    Ok(Repaired { points: st.p, tets: tets_out, density: dens_out, report })
}

#[cfg(test)]
mod tests {
    use super::*;

    // AI-FUNC-SUMMARY: A cube of 6 Kuhn tetrahedra per voxel, n per side, each wound positive; returns (points, tets).
    fn cube(n: usize) -> (Vec<f64>, Vec<u32>) {
        let mut pts = Vec::new();
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    pts.extend_from_slice(&[i as f64, j as f64, k as f64]);
                }
            }
        }
        let id = |i: usize, j: usize, k: usize| ((k * (n + 1) + j) * (n + 1) + i) as u32;
        let mut tets = Vec::new();
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    for path in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
                        let mut s = [i, j, k];
                        let mut walk = vec![id(s[0], s[1], s[2])];
                        for a in path {
                            s[a] += 1;
                            walk.push(id(s[0], s[1], s[2]));
                        }
                        let q: [[f64; 3]; 4] = std::array::from_fn(|c| {
                            let v = walk[c] as usize;
                            [pts[3 * v], pts[3 * v + 1], pts[3 * v + 2]]
                        });
                        if aspect_ratio(&q).1 < 0.0 {
                            walk.swap(0, 1);
                        }
                        tets.extend_from_slice(&walk);
                    }
                }
            }
        }
        (pts, tets)
    }

    // AI-FUNC-SUMMARY: Verify the ratio is 1 for a regular tetrahedron and infinite for a flat one.
    #[test]
    fn regular_cell_has_ratio_one() {
        let s = 1.0 / 2f64.sqrt();
        let q = [[1.0, 0.0, -s], [-1.0, 0.0, -s], [0.0, 1.0, s], [0.0, -1.0, s]];
        let (r, v) = aspect_ratio(&q);
        assert!((r - 1.0).abs() < 1e-12 && v.abs() > 0.0);
        let flat = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]];
        assert!(aspect_ratio(&flat).0.is_infinite());
    }

    // AI-FUNC-SUMMARY: Verify Nelder-Mead finds the minimum of a shifted quadratic.
    #[test]
    fn nelder_mead_finds_a_quadratic_minimum() {
        let mut f = |x: &[f64]| (x[0] - 1.0).powi(2) + 2.0 * (x[1] + 0.5).powi(2) + (x[2] - 3.0).powi(2);
        let (x, fx) = nelder_mead(&mut f, &[0.0, 0.0, 0.0], 1e-9, 1e-12, 4000);
        assert!(fx < 1e-12 && (x[0] - 1.0).abs() < 1e-5 && (x[1] + 0.5).abs() < 1e-5);
    }

    // AI-FUNC-SUMMARY: Verify a node pushed through its neighbours (cells inverted round it) is moved back so no cell is inverted or over the limit, and fixed nodes and the box stay.
    #[test]
    fn an_inverted_node_is_moved_back() {
        let (mut pts, tets) = cube(3);
        let n = (pts.len() / 3) as u32;
        let centre = 21usize;
        assert_eq!(&pts[3 * centre..3 * centre + 3], &[1.0, 1.0, 1.0]);
        pts[3 * centre] = 2.3;
        let density = vec![1001i64; tets.len() / 4];
        let fixed = vec![false; n as usize];
        let prm = RepairParams { limit: 10.0, sweeps: 20, wall_tol: 0.0, padding: None, joint_tol: None, seed: 0 };
        let out = repair(&pts, &tets, &density, &fixed, &[], &prm).unwrap();
        assert!(out.report.bad_before > 0);
        assert_eq!(out.report.inverted_after, 0);
        assert_eq!(out.report.over_limit_after, 0);
        for (k, p) in out.points.iter().enumerate() {
            if k != centre {
                assert_eq!(p, &[pts[3 * k], pts[3 * k + 1], pts[3 * k + 2]]);
            }
        }
    }

    // AI-FUNC-SUMMARY: The faces between two densities, as sorted node triples; returns the set.
    fn interface(tets: &[[u32; 4]], density: &[i64]) -> BTreeSet<[u32; 3]> {
        let mut seen: BTreeMap<[u32; 3], Vec<i64>> = BTreeMap::new();
        for (c, &d) in tets.iter().zip(density) {
            for f in [[c[0], c[1], c[2]], [c[0], c[1], c[3]], [c[0], c[2], c[3]], [c[1], c[2], c[3]]] {
                let mut k = f;
                k.sort_unstable();
                seen.entry(k).or_default().push(d);
            }
        }
        seen.into_iter().filter(|(_, d)| d.len() == 2 && d[0] != d[1]).map(|(f, _)| f).collect()
    }

    // AI-FUNC-SUMMARY: Verify a repair across a two-density mesh leaves the faces between the densities exactly as they were, and that two runs agree.
    #[test]
    fn repair_keeps_the_faces_between_densities_and_is_deterministic() {
        let (mut pts, tets) = cube(4);
        let m = tets.len() / 4;
        let cells: Vec<[u32; 4]> = tets.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
        let density: Vec<i64> = cells
            .iter()
            .map(|c| if c.iter().map(|&v| pts[3 * v as usize]).sum::<f64>() < 8.0 { 1 } else { 1001 })
            .collect();
        for v in [31usize, 37, 81] {
            pts[3 * v + 1] += 0.93;
        }
        let fixed = vec![false; pts.len() / 3];
        let prm = RepairParams { limit: 10.0, sweeps: 20, wall_tol: 0.0, padding: None, joint_tol: None, seed: 3 };
        let a = repair(&pts, &tets, &density, &fixed, &[], &prm).unwrap();
        let b = repair(&pts, &tets, &density, &fixed, &[], &prm).unwrap();
        assert!(a.report.bad_before > 0, "the fixture must need repair");
        assert_eq!(a.tets, b.tets);
        assert_eq!(a.points, b.points);
        assert_eq!(interface(&cells, &density), interface(&a.tets, &a.density));
        assert_eq!(a.density.iter().filter(|&&d| d == 1).count() + a.density.iter().filter(|&&d| d == 1001).count(), a.tets.len());
        assert!(m > 0);
    }

    // AI-FUNC-SUMMARY: Verify a flat cell with a face and every corner on the padding is given to it, and one reaching into the body by a corner is not (simHelper's tests of the same rule).
    #[test]
    fn a_flat_cell_goes_to_the_padding_only_with_every_corner_on_it() {
        let prm = RepairParams { limit: 10.0, sweeps: 20, wall_tol: 0.0, padding: Some(1000), joint_tol: None, seed: 0 };
        let pts = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.5, 0.5, 1.0, 0.5, 0.5, -1.0];
        let tets = [0, 1, 2, 3, 0, 1, 2, 4, 1, 3, 2, 4, 0, 2, 3, 5, 0, 3, 1, 5];
        let r = repair(&pts, &tets, &[1001, 1000, 1000, 1001, 1001], &[false; 6], &[], &prm).unwrap();
        assert_eq!(r.report.to_padding.len(), 1);
        assert_eq!(r.report.to_padding[0].from, 1001);
        // corner 6 lies in the plane of the flat cell, under the body only
        let pts = [
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.5, 0.5, 1.0, 0.5, 0.5, -1.0, -1.0, -1.0, 0.0,
        ];
        let tets = [6, 1, 2, 3, 1, 3, 2, 4, 6, 2, 3, 5, 6, 3, 1, 5, 0, 6, 1, 5];
        let r = repair(&pts, &tets, &[1001, 1000, 1001, 1001, 1001], &[false; 7], &[], &prm).unwrap();
        assert!(r.report.to_padding.is_empty());
        assert_eq!(r.density.iter().filter(|&&d| d == 1000).count(), 1);
    }

    // AI-FUNC-SUMMARY: Verify ragged inputs and out-of-range indices are refused.
    #[test]
    fn refuses_bad_inputs() {
        let prm = RepairParams { limit: 10.0, sweeps: 20, wall_tol: 0.0, padding: None, joint_tol: None, seed: 0 };
        assert!(repair(&[0.0; 12], &[0, 1, 2, 4], &[1], &[false; 4], &[], &prm).is_err());
        assert!(repair(&[0.0; 12], &[0, 1, 2, 3], &[1, 2], &[false; 4], &[], &prm).is_err());
    }
}
