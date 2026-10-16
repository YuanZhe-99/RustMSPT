//! Dense random packing by force-biased relaxation ("compress").
//!
//! Every planned particle is dropped at a random position and orientation at a common fraction F
//! of its true size, overlaps allowed. Each iteration then finds every overlap exactly - the
//! penetration depth, normal and contact points between convex pieces, the domain walls and the
//! void pieces - and pushes each particle apart along the contact normals and turns it by the
//! torque of those pushes. Whenever the deepest overlap is below the tolerance, F grows a little.
//! The run succeeds when F reaches 1 with no overlap left; it stops as jammed when F no longer
//! grows. Directed pushes rearrange many particles at once, which is why this is far faster than
//! random trial moves.
//!
//! Overlap is exact for convex pieces: a particle is its convex hull or an approximate convex
//! decomposition enclosing its true surface, the void is a convex decomposition of each pore
//! shell, and both sides are rounded (Minkowski sum with a ball) by the required gap, so zero
//! overlap means the true shapes keep `gaps.particle_particle` from each other and `void.gap`
//! from the void. Scale lives in the support map, so growing or rotating rebuilds nothing.
//!
//! Iterations are Gauss-Seidel sweeps: cells of one 2x2x2 colour (padded so same-coloured cells
//! cannot interact) run in parallel, particles within a cell in id order, each seeing its
//! neighbours' latest poses; a run is reproducible for a given seed on any number of threads.
use super::settle::{finish, template, Finish, Grain, Template};
use super::*;
use crate::config::placement::{CompressSpec, SettleGeometry};
use crate::types::BoundingBox;
use parry3d_f64::math::Isometry;
use parry3d_f64::na;
use parry3d_f64::query::details::{contact_support_map_support_map, intersection_test_support_map_support_map};
use parry3d_f64::shape::SupportMap;
use rayon::prelude::*;
use serde_json::json;

type P3 = na::Point3<f64>;
type V3 = na::Vector3<f64>;

/// A convex polytope's vertices, their edge adjacency (for hill-climbing support queries) and
/// its bounding sphere (centre `mid`, radius `radius`).
struct Cloud {
    points: Vec<P3>,
    adjacency: Vec<Vec<u32>>,
    mid: V3,
    radius: f64,
}

impl Cloud {
    // AI-FUNC-SUMMARY: Polytope from hull vertices and hull triangles (adjacency from triangle edges) with its vertex-mean bounding sphere; no side effects.
    fn new(points: Vec<P3>, triangles: &[[u32; 3]]) -> Cloud {
        let mid = points.iter().fold(V3::zeros(), |a, p| a + p.coords) / points.len().max(1) as f64;
        let radius = points.iter().map(|p| (p.coords - mid).norm()).fold(0.0, f64::max);
        let mut adjacency = vec![Vec::new(); points.len()];
        for t in triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                adjacency[a as usize].push(b);
                adjacency[b as usize].push(a);
            }
        }
        for list in &mut adjacency {
            list.sort_unstable();
            list.dedup();
        }
        Cloud { points, adjacency, mid, radius }
    }

    // AI-FUNC-SUMMARY: Support map of this polytope at a scale, rounded by `round`; no side effects.
    fn sup(&self, scale: f64, round: f64) -> Sup<'_> {
        Sup { cloud: self, scale, round, start: std::cell::Cell::new(0) }
    }
}

/// A shell at unit scale in its canonical frame: its hull and the pieces enclosing its surface.
struct Shape {
    hull: Cloud,
    pieces: Vec<Cloud>,
    /// Bounding radius about the canonical origin (the volume centroid).
    radius: f64,
}

/// Support map of a scaled polytope, optionally rounded (Minkowski sum with a ball). The support
/// vertex is found by hill-climbing the vertex graph from the previous answer: on a convex
/// polytope a vertex no neighbour improves on is a global maximiser, and successive GJK/EPA
/// directions change little, so a query visits a few dozen vertices instead of all of them.
struct Sup<'a> {
    cloud: &'a Cloud,
    scale: f64,
    round: f64,
    start: std::cell::Cell<u32>,
}

impl SupportMap for Sup<'_> {
    fn local_support_point(&self, dir: &V3) -> P3 {
        let points = &self.cloud.points;
        let mut best = self.start.get() as usize;
        let mut top = points[best].coords.dot(dir);
        loop {
            let mut moved = false;
            for &n in &self.cloud.adjacency[best] {
                let v = points[n as usize].coords.dot(dir);
                if v > top {
                    top = v;
                    best = n as usize;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        self.start.set(best as u32);
        let mut q = points[best] * self.scale;
        let n = dir.norm();
        if self.round > 0.0 && n > 0.0 {
            q += dir * (self.round / n);
        }
        q
    }
}

/// One particle: shell, size draw, true scale and pose.
#[derive(Clone)]
struct Part {
    shell: usize,
    draw: SizeDraw,
    /// True scale of the canonical shell.
    k: f64,
    c: V3,
    q: na::UnitQuaternion<f64>,
    /// Final per-particle shrink that clears residual overlaps (1 = none).
    sigma: f64,
}

impl Part {
    // AI-FUNC-SUMMARY: Current scale of the canonical shell at size fraction f; no side effects.
    fn scale(&self, f: f64) -> f64 {
        self.k * f * self.sigma
    }

    // AI-FUNC-SUMMARY: Pose isometry; no side effects.
    fn iso(&self) -> Isometry<f64> {
        Isometry::from_parts(na::Translation3::from(self.c), self.q)
    }
}

/// Static void pieces in world coordinates, binned on a uniform grid.
struct Voids {
    clouds: Vec<Cloud>,
    lo: V3,
    h: f64,
    n: [usize; 3],
    cells: Vec<Vec<u32>>,
}

impl Voids {
    // AI-FUNC-SUMMARY: Decompose every void shell meeting the (expanded) domain into convex pieces and bin them; deterministic; no I/O.
    fn build(mesh: Option<&Mesh>, domain: BoundingBox, reach: f64, h: f64, concavity: f64) -> Voids {
        let lo = V3::new(domain.min.x - reach, domain.min.y - reach, domain.min.z - reach);
        let size = domain.size();
        let n = [
            ((size.x + 2.0 * reach) / h).ceil() as usize + 1,
            ((size.y + 2.0 * reach) / h).ceil() as usize + 1,
            ((size.z + 2.0 * reach) / h).ceil() as usize + 1,
        ];
        let mut voids = Voids { clouds: Vec::new(), lo, h, n, cells: vec![Vec::new(); n[0] * n[1] * n[2]] };
        let Some(mesh) = mesh else { return voids };
        let near = domain.expanded(reach);
        let shells: Vec<Mesh> = crate::geometry::mesh_ops::split_mesh_into_granules(mesh)
            .into_iter()
            .filter(|s| mesh_bbox(s).is_some_and(|b| b.intersects_domain(near)))
            .collect();
        let pieces: Vec<Template> = shells.par_iter().map(|s| template(s, SettleGeometry::Parts, concavity)).collect();
        for (points, triangles) in pieces.into_iter().flatten() {
            let cloud = Cloud::new(points, &triangles);
            let id = voids.clouds.len() as u32;
            let (a, b) = (voids.index(cloud.mid.add_scalar(-cloud.radius)), voids.index(cloud.mid.add_scalar(cloud.radius)));
            for ix in a[0]..=b[0] {
                for iy in a[1]..=b[1] {
                    for iz in a[2]..=b[2] {
                        voids.cells[(ix * n[1] + iy) * n[2] + iz].push(id);
                    }
                }
            }
            voids.clouds.push(cloud);
        }
        voids
    }

    // AI-FUNC-SUMMARY: Clamped grid cell of a point; no side effects.
    fn index(&self, p: V3) -> [usize; 3] {
        let g = |v: f64, lo: f64, n: usize| (((v - lo) / self.h).floor().max(0.0) as usize).min(n - 1);
        [g(p.x, self.lo.x, self.n[0]), g(p.y, self.lo.y, self.n[1]), g(p.z, self.lo.z, self.n[2])]
    }

    // AI-FUNC-SUMMARY: Distinct void pieces whose cells meet the sphere (centre, radius), into `out`; no mutation.
    fn near(&self, c: V3, r: f64, out: &mut Vec<u32>) {
        out.clear();
        let (a, b) = (self.index(c.add_scalar(-r)), self.index(c.add_scalar(r)));
        for ix in a[0]..=b[0] {
            for iy in a[1]..=b[1] {
                for iz in a[2]..=b[2] {
                    out.extend_from_slice(&self.cells[(ix * self.n[1] + iy) * self.n[2] + iz]);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

/// Fixed geometry and rules shared by every query.
struct World {
    shapes: Vec<Shape>,
    voids: Voids,
    domain: BoundingBox,
    wall: f64,
    gap: f64,
    void_gap: f64,
}

/// Push on one particle: translation and torque (about its centre), plus the deepest overlap.
#[derive(Clone, Copy)]
struct Push {
    shift: V3,
    torque: V3,
    depth: f64,
}

impl Push {
    const ZERO: Push = Push { shift: V3::new(0.0, 0.0, 0.0), torque: V3::new(0.0, 0.0, 0.0), depth: 0.0 };

    // AI-FUNC-SUMMARY: Add a force `f` applied at lever arm `arm` with overlap `depth`; mutates self.
    fn add(&mut self, f: V3, arm: V3, depth: f64) {
        self.shift += f;
        self.torque += arm.cross(&f);
        self.depth = self.depth.max(depth);
    }
}

impl World {
    // AI-FUNC-SUMMARY: Pushes from the domain walls on a particle at scale s (overlap beyond `wall` per face, over hull vertices); mutates only `out`.
    fn wall_push(&self, p: &Part, s: f64, out: &mut Push) {
        let hull = &self.shapes[p.shell].hull.points;
        let d = self.domain;
        let (lo, hi) = (V3::new(d.min.x, d.min.y, d.min.z), V3::new(d.max.x, d.max.y, d.max.z));
        for axis in 0..3 {
            let e = V3::ith(axis, 1.0);
            let local = p.q.inverse_transform_vector(&e);
            let (mut top, mut bottom) = (f64::NEG_INFINITY, f64::INFINITY);
            for v in hull {
                let t = v.coords.dot(&local);
                top = top.max(t);
                bottom = bottom.min(t);
            }
            let over = p.c[axis] + s * top - (hi[axis] - self.wall);
            if over > 0.0 {
                out.add(-e * over, V3::zeros(), over);
            }
            let under = lo[axis] + self.wall - (p.c[axis] + s * bottom);
            if under > 0.0 {
                out.add(e * under, V3::zeros(), under);
            }
        }
    }

    // AI-FUNC-SUMMARY: Pushes from void pieces on a particle at scale s (full penetration, the void does not move); scratch reused; mutates only `out`.
    fn void_push(&self, p: &Part, s: f64, scratch: &mut Vec<u32>, out: &mut Push) {
        let shape = &self.shapes[p.shell];
        let reach = s * shape.radius + self.void_gap;
        self.voids.near(p.c, reach, scratch);
        if scratch.is_empty() {
            return;
        }
        let iso = p.iso();
        let pos12 = iso.inverse();
        for &v in scratch.iter() {
            let cloud = &self.voids.clouds[v as usize];
            if (cloud.mid - p.c).norm() >= reach + cloud.radius {
                continue;
            }
            let other = cloud.sup(1.0, self.void_gap);
            if !intersection_test_support_map_support_map(&pos12, &shape.hull.sup(s, 0.0), &other) {
                continue;
            }
            for piece in &shape.pieces {
                let mid = iso * P3::from(piece.mid * s);
                if (cloud.mid - mid.coords).norm() >= piece.radius * s + cloud.radius + self.void_gap {
                    continue;
                }
                if let Some(c) = contact_support_map_support_map(&pos12, &piece.sup(s, 0.0), &other, 0.0) {
                    if c.dist < 0.0 {
                        let n = iso * c.normal1.into_inner();
                        let at = iso * c.point1;
                        out.add(n * c.dist, at.coords - p.c, -c.dist);
                    }
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: True when two particles at scales sa, sb come within the gap (hull test, then piece pairs); no side effects.
    fn touches(&self, a: &Part, sa: f64, b: &Part, sb: f64) -> bool {
        let (ea, eb) = (&self.shapes[a.shell], &self.shapes[b.shell]);
        if (a.c - b.c).norm() >= sa * ea.radius + sb * eb.radius + self.gap {
            return false;
        }
        let (ia, ib) = (a.iso(), b.iso());
        let pos12 = ia.inv_mul(&ib);
        let half = 0.5 * self.gap;
        if !intersection_test_support_map_support_map(
            &pos12,
            &ea.hull.sup(sa, half),
            &eb.hull.sup(sb, half),
        ) {
            return false;
        }
        if ea.pieces.len() == 1 && eb.pieces.len() == 1 {
            return true;
        }
        ea.pieces.iter().any(|pa| {
            let ma = ia * P3::from(pa.mid * sa);
            eb.pieces.iter().any(|pb| {
                let mb = ib * P3::from(pb.mid * sb);
                (ma - mb).norm() < pa.radius * sa + pb.radius * sb + self.gap
                    && intersection_test_support_map_support_map(
                        &pos12,
                        &pa.sup(sa, half),
                        &pb.sup(sb, half),
                    )
            })
        })
    }

    // AI-FUNC-SUMMARY: Pushes between two particles at scales sa, sb (half the penetration each along the contact normal, per overlapping piece pair); None when apart; no side effects.
    fn pair_push(&self, a: &Part, sa: f64, b: &Part, sb: f64) -> Option<(Push, Push)> {
        let (ea, eb) = (&self.shapes[a.shell], &self.shapes[b.shell]);
        if (a.c - b.c).norm() >= sa * ea.radius + sb * eb.radius + self.gap {
            return None;
        }
        let (ia, ib) = (a.iso(), b.iso());
        let pos12 = ia.inv_mul(&ib);
        let half = 0.5 * self.gap;
        if !intersection_test_support_map_support_map(
            &pos12,
            &ea.hull.sup(sa, half),
            &eb.hull.sup(sb, half),
        ) {
            return None;
        }
        let (mut pa_push, mut pb_push) = (Push::ZERO, Push::ZERO);
        let mut any = false;
        for pa in &ea.pieces {
            let ma = ia * P3::from(pa.mid * sa);
            for pb in &eb.pieces {
                let mb = ib * P3::from(pb.mid * sb);
                if (ma - mb).norm() >= pa.radius * sa + pb.radius * sb + self.gap {
                    continue;
                }
                let Some(c) = contact_support_map_support_map(
                    &pos12,
                    &pa.sup(sa, half),
                    &pb.sup(sb, half),
                    0.0,
                ) else {
                    continue;
                };
                if c.dist >= 0.0 {
                    continue;
                }
                any = true;
                let depth = -c.dist;
                // normal1 points out of a toward b: a moves back, b moves on, half each.
                let n = ia * c.normal1.into_inner();
                let on_a = ia * c.point1;
                let on_b = ib * c.point2;
                pa_push.add(-n * (0.5 * depth), on_a.coords - a.c, depth);
                pb_push.add(n * (0.5 * depth), on_b.coords - b.c, depth);
            }
        }
        any.then_some((pa_push, pb_push))
    }
}

/// Uniform cell grid over particle centres.
struct Grid {
    lo: V3,
    h: f64,
    n: [usize; 3],
    cells: Vec<Vec<u32>>,
}

impl Grid {
    // AI-FUNC-SUMMARY: Bin every particle centre into cells of edge h covering the domain; no I/O.
    fn build(domain: BoundingBox, h: f64, parts: &[Part]) -> Grid {
        let size = domain.size();
        let n = [(size.x / h).floor() as usize + 1, (size.y / h).floor() as usize + 1, (size.z / h).floor() as usize + 1];
        let mut grid = Grid { lo: V3::new(domain.min.x, domain.min.y, domain.min.z), h, n, cells: vec![Vec::new(); n[0] * n[1] * n[2]] };
        for (id, p) in parts.iter().enumerate() {
            let i = grid.index(&p.c);
            grid.cells[(i[0] * n[1] + i[1]) * n[2] + i[2]].push(id as u32);
        }
        grid
    }

    // AI-FUNC-SUMMARY: Cell coordinates of a point (clamped); no side effects.
    fn index(&self, p: &V3) -> [usize; 3] {
        let g = |v: f64, lo: f64, n: usize| (((v - lo) / self.h).floor().max(0.0) as usize).min(n - 1);
        [g(p.x, self.lo.x, self.n[0]), g(p.y, self.lo.y, self.n[1]), g(p.z, self.lo.z, self.n[2])]
    }

    // AI-FUNC-SUMMARY: Call f for every particle id in the 27 cells around the cell of `p`; no mutation.
    fn around(&self, p: &V3, mut f: impl FnMut(u32)) {
        let i = self.index(p);
        for ix in i[0].saturating_sub(1)..=(i[0] + 1).min(self.n[0] - 1) {
            for iy in i[1].saturating_sub(1)..=(i[1] + 1).min(self.n[1] - 1) {
                for iz in i[2].saturating_sub(1)..=(i[2] + 1).min(self.n[2] - 1) {
                    for &j in &self.cells[(ix * self.n[1] + iy) * self.n[2] + iz] {
                        f(j);
                    }
                }
            }
        }
    }
}

/// Per-iteration overlap summary.
#[derive(Clone, Copy, Default)]
struct Overlap {
    max: f64,
    /// Mean over overlapping particles of each one's deepest overlap.
    mean: f64,
    pairs: usize,
    touched: usize,
}

// AI-FUNC-SUMMARY: Shift and turn one particle by its accumulated push (shift capped at max_turn times its radius); mutates p.
fn apply(world: &World, p: &mut Part, push: &Push, f: f64, spec: &CompressSpec) {
    if push.depth <= 0.0 {
        return;
    }
    let gain = spec.relaxation * (1.0 + spec.overshoot);
    // Turning by angle a moves a surface point at the bounding radius r by about a*r, the same
    // order as the push that caused the torque. A deep contact clears over several sweeps.
    let r = world.shapes[p.shell].radius * p.scale(f);
    let shift = push.shift * gain;
    let limit = spec.max_turn * r;
    p.c += if shift.norm() > limit { shift * (limit / shift.norm()) } else { shift };
    let spin = push.torque * (gain / (r * r));
    let angle = spin.norm().min(spec.max_turn);
    if angle > 0.0 {
        p.q = na::UnitQuaternion::from_axis_angle(&na::Unit::new_normalize(spin), angle) * p.q;
    }
}

// AI-FUNC-SUMMARY: One Gauss-Seidel relaxation sweep at size fraction `f`: cells of one 2x2x2 colour run in parallel, particles within a cell in id order, each pushed off walls, voids and its neighbours' latest poses (its half of each pair push); returns the overlap summary seen during the sweep; mutates parts.
fn relax(world: &World, parts: &mut [Part], f: f64, rmax: f64, spec: &CompressSpec) -> Overlap {
    // Cells are padded by the largest move of a sweep, so particles of same-coloured cells
    // (one cell apart) cannot interact even after moving.
    let h = (2.0 * rmax * f + world.gap.max(world.void_gap)) + 2.0 * spec.max_turn * rmax * f + 1e-9;
    let grid = Grid::build(world.domain, h, parts);
    let mut summary = Overlap::default();
    let mut sum = 0.0;
    for colour in 0..8usize {
        let cells: Vec<usize> = (0..grid.cells.len())
            .filter(|&c| {
                let (x, y, z) = (c / (grid.n[1] * grid.n[2]), (c / grid.n[2]) % grid.n[1], c % grid.n[2]);
                (x & 1) | ((y & 1) << 1) | ((z & 1) << 2) == colour && !grid.cells[c].is_empty()
            })
            .collect();
        let shared: &[Part] = parts;
        let results: Vec<(Vec<(u32, Part)>, Overlap, f64)> = cells
            .par_iter()
            .map(|&c| {
                let own = &grid.cells[c];
                let mut local: Vec<(u32, Part)> = own.iter().map(|&j| (j, shared[j as usize].clone())).collect();
                let mut scratch = Vec::new();
                let mut seen = Overlap::default();
                let mut sum = 0.0;
                for slot in 0..local.len() {
                    let (id, me) = (local[slot].0, local[slot].1.clone());
                    let mut push = Push::ZERO;
                    world.wall_push(&me, me.scale(f), &mut push);
                    world.void_push(&me, me.scale(f), &mut scratch, &mut push);
                    grid.around(&me.c, |j| {
                        if j == id {
                            return;
                        }
                        let other = match own.binary_search(&j) {
                            Ok(k) => &local[k].1,
                            Err(_) => &shared[j as usize],
                        };
                        if let Some((pa, _)) = world.pair_push(&me, me.scale(f), other, other.scale(f)) {
                            seen.pairs += 1;
                            push.shift += pa.shift;
                            push.torque += pa.torque;
                            push.depth = push.depth.max(pa.depth);
                        }
                    });
                    if push.depth > 0.0 {
                        seen.touched += 1;
                        seen.max = seen.max.max(push.depth);
                        sum += push.depth;
                    }
                    apply(world, &mut local[slot].1, &push, f, spec);
                }
                (local, seen, sum)
            })
            .collect();
        for (local, seen, s) in results {
            summary.pairs += seen.pairs;
            summary.touched += seen.touched;
            summary.max = summary.max.max(seen.max);
            sum += s;
            for (id, p) in local {
                parts[id as usize] = p;
            }
        }
    }
    // Each overlapping pair is seen from both sides.
    summary.pairs /= 2;
    summary.mean = sum / summary.touched.max(1) as f64;
    summary
}

// AI-FUNC-SUMMARY: Deepest remaining overlap of every particle at full size (times its shrink): pairs, walls and void; 0 where clear; no mutation.
fn residual(world: &World, parts: &[Part], rmax: f64) -> Vec<f64> {
    let h = 2.0 * rmax + world.gap.max(world.void_gap) + 1e-9;
    let grid = Grid::build(world.domain, h, parts);
    let pairs: Vec<(u32, u32, f64)> = (0..parts.len())
        .into_par_iter()
        .flat_map_iter(|i| {
            let a = &parts[i];
            let mut out = Vec::new();
            grid.around(&a.c, |j| {
                let b = &parts[j as usize];
                if (j as usize) > i {
                    if let Some((pa, _)) = world.pair_push(a, a.scale(1.0), b, b.scale(1.0)) {
                        out.push((i as u32, j, pa.depth));
                    }
                }
            });
            out
        })
        .collect();
    let mut depth: Vec<f64> = parts
        .par_iter()
        .map_init(Vec::new, |scratch, p| {
            let mut push = Push::ZERO;
            world.wall_push(p, p.scale(1.0), &mut push);
            world.void_push(p, p.scale(1.0), scratch, &mut push);
            push.depth
        })
        .collect();
    for (i, j, d) in pairs {
        depth[i as usize] = depth[i as usize].max(d);
        depth[j as usize] = depth[j as usize].max(d);
    }
    depth
}

// AI-FUNC-SUMMARY: Plan, scatter, relax-and-grow to true size, verify exactly, then hand the particles to the shared filter/output stage; returns the outcome.
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    source: &SizeSource,
    classes: &[SizeClass],
    void: Option<&VoidIndex>,
    void_mesh: Option<&Mesh>,
    void_volume: f64,
    void_method: Option<VoidVolumeMethod>,
    basis: f64,
    tool: &ToolRecord,
    started: Instant,
) -> Result<PlacementOutcome> {
    let spec: &CompressSpec = &config.compress;
    let domain = config.domain;
    let mut rng = seeded_rng(config.seed);
    let target = basis * config.target_volume_fraction;
    let plan = plan_size_multiset(&mut rng, source, classes, target, MAX_PLANNED_PARTICLES)?;
    let mut state = EngineState::new(config, library, classes, &plan.draws);
    state.basis_volume = basis;
    state.control.phase = "compressing";

    let gap = config.gap_particle_particle;
    let void_gap = config.void.as_ref().map_or(0.0, |v| v.gap);
    let t0 = Instant::now();
    let shapes: Vec<Shape> = library
        .shells
        .par_iter()
        .map(|sh| {
            let points: Vec<P3> = sh.canonical.vertices.iter().map(|v| P3::new(v.x, v.y, v.z)).collect();
            let (hull_points, hull_triangles) = parry3d_f64::transformation::convex_hull(&points);
            let hull = Cloud::new(hull_points, &hull_triangles);
            let template: Template = template(&sh.canonical, spec.geometry, spec.concavity);
            let pieces = template.into_iter().map(|(p, t)| Cloud::new(p, &t)).collect();
            Shape { hull, pieces, radius: sh.bounding_radius }
        })
        .collect();
    let rmax = plan
        .draws
        .iter()
        .map(|d| d.diameter / 2.0 * library.max_extent_ratio.max(1.0))
        .fold(0.0, f64::max);
    let voids = Voids::build(void_mesh, domain, 2.0 * rmax + void_gap, 2.0 * rmax + void_gap, spec.void_concavity);
    eprintln!(
        "[Compress] geometry ready in {:.1}s: {} shells (mean {:.1} pieces), {} void pieces",
        t0.elapsed().as_secs_f64(),
        shapes.len(),
        shapes.iter().map(|s| s.pieces.len()).sum::<usize>() as f64 / shapes.len().max(1) as f64,
        voids.clouds.len()
    );
    let world = World { shapes, voids, domain, wall: config.boundary.min_boundary_dist, gap, void_gap };

    // Scatter by random sequential addition at the start size: no overlap, so no pair starts
    // deeply interlocked. A particle that finds no free spot is still placed (overlapping).
    let planned_volume: f64 = plan.draws.iter().map(|d| std::f64::consts::PI * d.diameter.powi(3) / 6.0).sum();
    let free_volume = (domain.volume() - void_volume).max(1e-300);
    let mut f = (spec.start_fraction * free_volume / planned_volume).cbrt().min(1.0);
    let mut parts: Vec<Part> = Vec::with_capacity(plan.draws.len());
    let mut grid = Grid::build(domain, 2.0 * rmax * f + gap.max(void_gap) + 1e-9, &[]);
    let mut scratch = Vec::new();
    let mut forced = 0usize;
    for d in &plan.draws {
        let shell = uniform_index(&mut rng, library.shells.len());
        let sh = &library.shells[shell];
        let k = d.diameter / sh.equivalent_diameter;
        let mut placed = None;
        let mut fallback = None;
        for _ in 0..spec.scatter_attempts {
            let c = V3::new(
                uniform_range(&mut rng, domain.min.x, domain.max.x),
                uniform_range(&mut rng, domain.min.y, domain.max.y),
                uniform_range(&mut rng, domain.min.z, domain.max.z),
            );
            let [w, x, y, z] = sample_uniform_quaternion(&mut rng).to_wxyz();
            let q = na::UnitQuaternion::new_unchecked(na::Quaternion::new(w, x, y, z));
            let p = Part { shell, draw: *d, k, c, q, sigma: 1.0 };
            if void.is_some_and(|v| v.contains_point(Vec3::new(c.x, c.y, c.z))) {
                continue;
            }
            fallback.get_or_insert_with(|| p.clone());
            let mut push = Push::ZERO;
            world.wall_push(&p, p.scale(f), &mut push);
            world.void_push(&p, p.scale(f), &mut scratch, &mut push);
            if push.depth > 0.0 {
                continue;
            }
            let mut hit = false;
            grid.around(&p.c, |j| hit = hit || world.touches(&p, p.scale(f), &parts[j as usize], parts[j as usize].scale(f)));
            if !hit {
                placed = Some(p);
                break;
            }
        }
        let p = match placed {
            Some(p) => p,
            None => {
                forced += 1;
                fallback.expect("some start lies outside the void")
            }
        };
        let i = grid.index(&p.c);
        grid.cells[(i[0] * grid.n[1] + i[1]) * grid.n[2] + i[2]].push(parts.len() as u32);
        parts.push(p);
    }
    eprintln!("[Compress] scatter: {} placed without overlap, {forced} forced, {:.1}s", parts.len() - forced, t0.elapsed().as_secs_f64());
    let full_volume: f64 = parts.iter().map(|p| library.shells[p.shell].volume * p.k.powi(3)).sum();
    let tolerance = spec.overlap_tolerance * source.quantile(0.5);
    eprintln!(
        "[Compress] scattered {} at F={f:.4}; full size fills {:.4} of the basis and {:.4} of the space outside the void",
        parts.len(),
        full_volume / basis,
        full_volume / free_volume
    );

    let mut stop = "max_iterations";
    let mut iterations = 0usize;
    let mut growth = spec.growth;
    let mut since_growth = 0usize;
    let mut history: Vec<(usize, f64, f64)> = Vec::new();
    let mut last = Overlap::default();
    let mut thinned = 0usize;
    let mut thinning_rounds = 0usize;
    let mut timed_out = false;
    let volume_of = |p: &Part| library.shells[p.shell].volume * p.k.powi(3);
    while iterations < spec.max_iterations {
        if state.poll_control(config, false) {
            stop = "interrupted";
            break;
        }
        let out_of_time = spec.max_seconds > 0.0 && t0.elapsed().as_secs_f64() > spec.max_seconds;
        last = relax(&world, &mut parts, f, rmax, spec);
        iterations += 1;
        if f >= 1.0 && (last.touched == 0 || last.mean <= spec.final_overlap * source.quantile(0.5)) {
            // What is left is cleared by the final per-particle shrink.
            if stop == "max_iterations" {
                stop = "complete";
            }
            break;
        }
        if f < 1.0 && last.mean <= tolerance {
            // Grow only while the pushes keep the mean overlap small; quick success speeds up.
            f = (f * (1.0 + growth)).min(1.0);
            growth = (growth * 1.1).min(spec.growth_max);
            since_growth = 0;
        } else {
            growth = (growth * 0.7).max(spec.growth_min);
            since_growth += 1;
        }
        // Out of growth time counts as jammed: thin once and finish at full size.
        let jammed = since_growth >= spec.stall_iterations || (out_of_time && f < 1.0 && !timed_out);
        if out_of_time && f < 1.0 {
            timed_out = true;
        }
        if jammed {
            if f >= 1.0 {
                stop = "full_size_with_residual_overlap";
                break;
            }
            if thinning_rounds >= spec.max_thinning_rounds {
                stop = "jammed";
                break;
            }
            if timed_out {
                stop = "max_seconds_then_thinned";
            }
            // Jammed below full size: remove a random share of particles matching the missing
            // volume (plus a margin), then keep growing.
            let total: f64 = parts.iter().map(volume_of).sum();
            let mut remove = total * (1.0 - f.powi(3) * (1.0 - spec.thinning_margin));
            let mut order: Vec<usize> = (0..parts.len()).collect();
            for k in (1..order.len()).rev() {
                let j = uniform_index(&mut rng, k + 1);
                order.swap(k, j);
            }
            let mut drop = vec![false; parts.len()];
            for i in order {
                if remove <= 0.0 {
                    break;
                }
                remove -= volume_of(&parts[i]);
                drop[i] = true;
            }
            let before = parts.len();
            let mut k = 0;
            parts.retain(|_| {
                k += 1;
                !drop[k - 1]
            });
            thinned += before - parts.len();
            thinning_rounds += 1;
            since_growth = 0;
            growth = spec.growth;
            eprintln!(
                "[Compress] jammed at F={f:.5} (vf {:.4}); thinned {} particles, {} left",
                parts.iter().map(volume_of).sum::<f64>() * f.powi(3) / basis,
                before - parts.len(),
                parts.len()
            );
        }
        if iterations % 50 == 0 {
            let vf = parts.iter().map(volume_of).sum::<f64>() * f.powi(3) / basis;
            history.push((iterations, f, vf));
            eprintln!(
                "[Compress] it {iterations} F {f:.5} vf {vf:.4} growth {growth:.2e} overlapping_pairs {} touched {} mean_depth {:.2e} max_depth {:.2e} {:.1}s",
                last.pairs,
                last.touched,
                last.mean,
                last.max,
                t0.elapsed().as_secs_f64()
            );
            state.attempts = iterations;
            state.control.publish(config, "compressing", parts.len(), iterations, vf);
        }
    }
    // Clear what overlap is left by shrinking each touched particle about its own centroid: a
    // shrink by df moves every surface point at least df times its distance from the centroid,
    // and contacts sit at least about half a bounding radius out, so a few rounds suffice.
    // A particle that would shrink by more than max_shrink is deleted instead.
    let mut shrink_rounds = 0usize;
    let mut shrink_dropped = 0usize;
    if f >= 1.0 {
        while shrink_rounds < 50 {
            let depth = residual(&world, &parts, rmax);
            if depth.iter().all(|d| *d <= 0.0) {
                break;
            }
            shrink_rounds += 1;
            let before = parts.len();
            let mut k = 0;
            parts.retain_mut(|p| {
                let d = depth[k];
                k += 1;
                if d > 0.0 {
                    let r = world.shapes[p.shell].radius * p.scale(1.0);
                    p.sigma *= 1.0 - 1.2 * d / (0.5 * r);
                }
                p.sigma >= 1.0 - spec.max_shrink
            });
            shrink_dropped += before - parts.len();
        }
    }
    let compress_seconds = t0.elapsed().as_secs_f64();
    eprintln!("[Compress] stop={stop} iterations={iterations} F={f:.5} {compress_seconds:.1}s");

    // Exact verification at the final sizes; anything still overlapping is dropped and reported.
    let mut keep = vec![f >= 1.0; parts.len()];
    let mut dropped_unresolved = 0usize;
    if f >= 1.0 {
        for (k, d) in residual(&world, &parts, rmax).iter().enumerate() {
            if *d > 0.0 {
                keep[k] = false;
                dropped_unresolved += 1;
            }
        }
    }
    let grains: Vec<Grain> = parts
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(p, _)| {
            let sh = &library.shells[p.shell];
            Grain {
                shell: p.shell,
                diameter: p.draw.diameter * p.sigma,
                class: p.draw.class,
                draw_index: p.draw.draw_index,
                scale: p.scale(1.0),
                radius: sh.bounding_radius * p.scale(1.0),
                rotation: UnitQuat::new(p.q.w, p.q.i, p.q.j, p.q.k).expect("unit quaternion"),
                centre: Vec3::new(p.c.x, p.c.y, p.c.z),
                volume: sh.volume * p.scale(1.0).powi(3),
                rolls: 0,
                rest: "compressed",
            }
        })
        .collect();
    let sigmas: Vec<f64> = parts.iter().filter(|p| p.sigma < 1.0).map(|p| p.sigma).collect();
    eprintln!(
        "[Compress] thinned {thinned} in {thinning_rounds} rounds; residual shrink: {} particles in {shrink_rounds} rounds (min sigma {:.5}), {shrink_dropped} dropped; unresolved dropped {dropped_unresolved}; kept {}",
        sigmas.len(),
        sigmas.iter().copied().fold(1.0, f64::min),
        grains.len()
    );
    let produced = grains.len();
    finish(
        &Finish {
            config,
            library,
            classes,
            void,
            void_volume,
            void_method,
            basis,
            tool,
            started,
            plan: &plan,
            s: 1.0,
            cell: 2.0 * rmax + gap.max(void_gap),
            measure_gaps: spec.measure_gaps,
            pair_check: false,
            label: "Compress",
            stop: stop.to_string(),
            produced,
            report: json!({
                "config": spec,
                "compression": {
                    "planned": plan.draws.len(),
                    "iterations": iterations,
                    "stop": stop,
                    "seconds": compress_seconds,
                    "final_size_fraction": f,
                    "planned_vf_of_basis": full_volume / basis,
                    "planned_vf_outside_void": full_volume / free_volume,
                    "last_overlap": {"pairs": last.pairs, "touched": last.touched, "mean_depth": last.mean, "max_depth": last.max},
                    "thinned_when_jammed": thinned,
                    "thinning_rounds": thinning_rounds,
                    "shrunk_particles": sigmas.len(),
                    "min_shrink_factor": sigmas.iter().copied().fold(1.0, f64::min),
                    "shrink_rounds": shrink_rounds,
                    "dropped_by_shrink_limit": shrink_dropped,
                    "dropped_unresolved": dropped_unresolved,
                    "history_every_50": history,
                    "gap_particle_particle": gap,
                    "void_gap": void_gap,
                }
            }),
        },
        state,
        grains,
        rng,
    )
}
