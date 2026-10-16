//! Gravity deposition of enlarged particles ("settle"), then shrink about each centroid and keep
//! what the ordinary exact feasibility check accepts.
//!
//! Particles are dropped one by one at s times their size into a bed that is periodic in x/y and
//! floored below the domain. Each particle keeps its drawn orientation and moves by translation
//! only, event to event: it falls until the first contact, then slides down its contacts (one
//! contact: along the face; two: along the crease) to the next contact, until it rests on three
//! or more contacts, the floor, its roll budget or its horizontal reach. Every motion is an exact
//! shape cast between convex pieces, so no step size is involved. Particles are represented by
//! their convex hull or by an approximate convex decomposition whose parts enclose the true
//! surface; true shapes therefore never overlap. Afterwards every particle is shrunk back by 1/s
//! about its centroid, which turns contacts into gaps, and the standard exact checks delete
//! particles in, crossing or too close to the void and particles leaving the strict domain.
//! Optional uniform random thinning brings the result to the target.
//!
//! Particles are processed in waves: a particle joins a wave only if its horizontal reach is
//! disjoint from every earlier pending particle, so wave members cannot interact and run in
//! parallel. Waves are formed from a fixed-size queue independent of the thread count, so the
//! deposit is reproducible for a given seed on any number of workers.
use super::aggregates::bodies::{distance_capped, Body, Posed};
use crate::config::placement::SettleGeometry;
use parry3d_f64::math::Isometry;
use parry3d_f64::na;
use parry3d_f64::query::{self, ShapeCastOptions};
use parry3d_f64::shape::ConvexPolyhedron;
use parry3d_f64::transformation::vhacd::{VHACDParameters, VHACD};
use super::*;
use crate::config::placement::SettleSpec;
use crate::geometry::vec_norm;
use crate::types::BoundingBox;
use rayon::prelude::*;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock};

/// Pending particles a wave is chosen from; fixed so waves do not depend on the thread count.
const QUEUE: usize = 512;
/// Waves between evictions of buried particle bodies.
const EVICT_EVERY: usize = 64;
/// Longest single slide, in bounding radii; a slide that leaves its contacts falls afresh.
const SLIDE: f64 = 0.5;
/// Tilt of every slide away from the contacts it slides on, so the cast starts separating.
const LIFT: f64 = 1e-3;

/// Convex pieces of one shell at unit scale in its canonical frame: one hull or a decomposition.
pub(super) type Template = Vec<(Vec<na::Point3<f64>>, Vec<[u32; 3]>)>;

// AI-FUNC-SUMMARY: Convex pieces enclosing one shell's surface: its hull, or the exact convex hulls of a VHACD decomposition (each part hulls the surface triangles it owns, so their union covers the surface); deterministic; no I/O.
pub(super) fn template(mesh: &crate::types::Mesh, geometry: SettleGeometry, concavity: f64) -> Template {
    let points: Vec<na::Point3<f64>> = mesh.vertices.iter().map(|v| na::Point3::new(v.x, v.y, v.z)).collect();
    if geometry == SettleGeometry::Parts {
        let indices: Vec<[u32; 3]> = mesh.faces.iter().map(|t| [t.a as u32, t.b as u32, t.c as u32]).collect();
        let params = VHACDParameters { concavity, ..Default::default() };
        // parry's hull builder can panic on near-degenerate clipped pieces; such a shell keeps
        // its plain hull.
        let parts = std::panic::catch_unwind(|| {
            VHACD::decompose(&params, &points, &indices, true).compute_exact_convex_hulls(&points, &indices)
        });
        if let Ok(parts) = parts {
            let parts: Template = parts.into_iter().filter(|(p, i)| p.len() >= 4 && !i.is_empty()).collect();
            if !parts.is_empty() {
                return parts;
            }
        }
    }
    vec![parry3d_f64::transformation::convex_hull(&points)]
}

/// One convex piece posed relative to its particle centre (rotation already applied).
struct Piece {
    shape: ConvexPolyhedron,
    offset: Vec3,
    radius: f64,
}

/// Deposition geometry of one particle at its enlarged size and fixed orientation.
struct Solid {
    pieces: Vec<Piece>,
    /// Lowest z relative to the centre.
    low: f64,
}

impl Solid {
    // AI-FUNC-SUMMARY: Scale and rotate a shell template into posed convex pieces with bounding spheres; no side effects.
    fn build(template: &Template, scale: f64, rotation: UnitQuat) -> Solid {
        let mut low = f64::INFINITY;
        let pieces = template
            .iter()
            .map(|(points, indices)| {
                let world: Vec<na::Point3<f64>> = points
                    .iter()
                    .map(|p| {
                        let v = rotation.rotate_point(Vec3::new(p.x * scale, p.y * scale, p.z * scale));
                        na::Point3::new(v.x, v.y, v.z)
                    })
                    .collect();
                let n = world.len() as f64;
                let mid = world.iter().fold(na::Vector3::zeros(), |a, p| a + p.coords) / n;
                let radius = world.iter().map(|p| (p.coords - mid).norm()).fold(0.0, f64::max);
                low = world.iter().map(|p| p.z).fold(low, f64::min);
                let shape = ConvexPolyhedron::from_convex_mesh(world.clone(), indices)
                    .or_else(|| ConvexPolyhedron::from_convex_hull(&world))
                    .expect("a template piece has a 3-D hull");
                Piece { shape, offset: Vec3::new(mid.x, mid.y, mid.z), radius }
            })
            .collect();
        Solid { pieces, low }
    }
}

// AI-FUNC-SUMMARY: Pure translation isometry; no side effects.
fn at(c: Vec3) -> Isometry<f64> {
    Isometry::translation(c.x, c.y, c.z)
}

#[derive(Clone, serde::Serialize)]
pub(super) struct Grain {
    pub(super) shell: usize,
    pub(super) diameter: f64,
    pub(super) class: usize,
    pub(super) draw_index: usize,
    /// Enlarged scale (s times the true scale).
    pub(super) scale: f64,
    /// Enlarged bounding radius.
    pub(super) radius: f64,
    pub(super) rotation: UnitQuat,
    pub(super) centre: Vec3,
    /// Enlarged solid volume.
    pub(super) volume: f64,
    /// Slides taken and why the particle came to rest.
    pub(super) rolls: usize,
    pub(super) rest: &'static str,
}

/// A particle drawn but not yet deposited; every variate is fixed when it enters the queue.
struct Pending {
    grain: Grain,
    starts: Vec<(f64, f64)>,
    reach: f64,
}

/// One obstacle image: grain index plus its (possibly periodically shifted) centre.
#[derive(Clone, Copy)]
struct Obstacle {
    index: usize,
    centre: Vec3,
    radius: f64,
}

/// Committed bed: grains, a 3-D bin grid periodic in x/y, column surface heights and lazily
/// rebuilt bodies (buried bodies are dropped and rebuilt on demand, so eviction never changes
/// a result).
struct Bed {
    domain: BoundingBox,
    floor: f64,
    nx: usize,
    ny: usize,
    hx: f64,
    hy: f64,
    hz: f64,
    cells: Vec<Vec<Vec<usize>>>,
    surface: Vec<f64>,
    grains: Vec<Grain>,
    bodies: Vec<Mutex<Option<Arc<Solid>>>>,
    templates: Vec<Template>,
    rmax: f64,
    gap: f64,
    rebuilt: AtomicU64,
    casts: AtomicU64,
    piece_casts: AtomicU64,
    contact_queries: AtomicU64,
}

impl Bed {
    // AI-FUNC-SUMMARY: Empty bed over the domain's x/y period with bins of at least `cell`; no I/O.
    fn new(domain: BoundingBox, floor: f64, cell: f64, rmax: f64, gap: f64, templates: Vec<Template>) -> Self {
        let size = domain.size();
        let nx = ((size.x / cell).floor() as usize).max(1);
        let ny = ((size.y / cell).floor() as usize).max(1);
        Bed {
            domain,
            floor,
            nx,
            ny,
            hx: size.x / nx as f64,
            hy: size.y / ny as f64,
            hz: cell,
            cells: vec![Vec::new(); nx * ny],
            surface: vec![floor; nx * ny],
            grains: Vec::new(),
            bodies: Vec::new(),
            templates,
            rmax,
            gap,
            rebuilt: AtomicU64::new(0),
            casts: AtomicU64::new(0),
            piece_casts: AtomicU64::new(0),
            contact_queries: AtomicU64::new(0),
        }
    }

    // AI-FUNC-SUMMARY: Wrap an x/y coordinate into the period; returns (wrapped column index, cell index); no mutation.
    fn column(&self, x: f64, y: f64) -> (usize, usize) {
        let ix = (((x - self.domain.min.x) / self.hx).floor() as i64).rem_euclid(self.nx as i64) as usize;
        let iy = (((y - self.domain.min.y) / self.hy).floor() as i64).rem_euclid(self.ny as i64) as usize;
        (ix, iy)
    }

    // AI-FUNC-SUMMARY: Deposition geometry of a committed grain, rebuilding it if evicted; deterministic geometry; locks only that slot.
    fn solid(&self, index: usize) -> Arc<Solid> {
        let mut slot = self.bodies[index].lock().unwrap();
        if let Some(solid) = slot.as_ref() {
            return solid.clone();
        }
        self.rebuilt.fetch_add(1, Relaxed);
        let g = &self.grains[index];
        let solid = Arc::new(Solid::build(&self.templates[g.shell], g.scale, g.rotation));
        *slot = Some(solid.clone());
        solid
    }

    // AI-FUNC-SUMMARY: Every committed grain image whose bounding sphere may meet the box [lo, hi]; periodic in x/y; no mutation.
    fn gather(&self, lo: Vec3, hi: Vec3) -> Vec<Obstacle> {
        // Grains are binned by centre, so the bin range is padded by the largest radius; the
        // sphere test below uses the box itself.
        let m = self.rmax;
        let size = self.domain.size();
        let ix0 = ((lo.x - m - self.domain.min.x) / self.hx).floor() as i64;
        let ix1 = ((hi.x + m - self.domain.min.x) / self.hx).floor() as i64;
        let iy0 = ((lo.y - m - self.domain.min.y) / self.hy).floor() as i64;
        let iy1 = ((hi.y + m - self.domain.min.y) / self.hy).floor() as i64;
        let iz0 = (((lo.z - m - self.floor) / self.hz).floor() as i64).max(0) as usize;
        let iz1 = ((hi.z + m - self.floor) / self.hz).floor() as i64;
        let mut out = Vec::new();
        if iz1 < 0 {
            return out;
        }
        for ix in ix0..=ix1 {
            let wx = ix.rem_euclid(self.nx as i64);
            let sx = (ix - wx) / self.nx as i64;
            for iy in iy0..=iy1 {
                let wy = iy.rem_euclid(self.ny as i64);
                let sy = (iy - wy) / self.ny as i64;
                let column = &self.cells[wx as usize * self.ny + wy as usize];
                let shift = Vec3::new(sx as f64 * size.x, sy as f64 * size.y, 0.0);
                if column.is_empty() {
                    continue;
                }
                for iz in iz0..=(iz1 as usize).min(column.len() - 1) {
                    for &index in &column[iz] {
                        let g = &self.grains[index];
                        let c = g.centre.add(shift);
                        if c.x + g.radius >= lo.x
                            && c.x - g.radius <= hi.x
                            && c.y + g.radius >= lo.y
                            && c.y - g.radius <= hi.y
                            && c.z + g.radius >= lo.z
                            && c.z - g.radius <= hi.z
                        {
                            out.push(Obstacle { index, centre: c, radius: g.radius });
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| a.index.cmp(&b.index).then(a.centre.x.total_cmp(&b.centre.x)).then(a.centre.y.total_cmp(&b.centre.y)));
        out
    }

    // AI-FUNC-SUMMARY: Highest grain top over the columns within `reach` of (x, y); floor when empty; no mutation.
    fn top_near(&self, x: f64, y: f64, reach: f64) -> f64 {
        let r = reach + self.rmax;
        let ix0 = ((x - r - self.domain.min.x) / self.hx).floor() as i64;
        let ix1 = ((x + r - self.domain.min.x) / self.hx).floor() as i64;
        let iy0 = ((y - r - self.domain.min.y) / self.hy).floor() as i64;
        let iy1 = ((y + r - self.domain.min.y) / self.hy).floor() as i64;
        let mut top = self.floor;
        for ix in ix0..=ix1 {
            let wx = ix.rem_euclid(self.nx as i64) as usize;
            for iy in iy0..=iy1 {
                let wy = iy.rem_euclid(self.ny as i64) as usize;
                top = top.max(self.surface[wx * self.ny + wy]);
            }
        }
        top
    }

    // AI-FUNC-SUMMARY: Commit a settled grain into bins, surface and body slots; mutates the bed.
    fn commit(&mut self, mut grain: Grain, body: Arc<Solid>) {
        let size = self.domain.size();
        let wrap = |v: f64, lo: f64, len: f64| lo + (v - lo).rem_euclid(len);
        grain.centre = Vec3::new(
            wrap(grain.centre.x, self.domain.min.x, size.x),
            wrap(grain.centre.y, self.domain.min.y, size.y),
            grain.centre.z,
        );
        let (ix, iy) = self.column(grain.centre.x, grain.centre.y);
        let iz = (((grain.centre.z - self.floor) / self.hz).floor().max(0.0)) as usize;
        let column = &mut self.cells[ix * self.ny + iy];
        if column.len() <= iz {
            column.resize(iz + 1, Vec::new());
        }
        let index = self.grains.len();
        column[iz].push(index);
        let s = &mut self.surface[ix * self.ny + iy];
        *s = s.max(grain.centre.z + grain.radius);
        self.grains.push(grain);
        self.bodies.push(Mutex::new(Some(body)));
    }

    // AI-FUNC-SUMMARY: Drop bodies of grains far below the lowest column surface; results unaffected (bodies rebuild on demand); returns bodies dropped.
    fn evict(&mut self, depth: f64) -> usize {
        let lowest = self.surface.iter().copied().fold(f64::INFINITY, f64::min);
        let mut dropped = 0;
        for (g, slot) in self.grains.iter().zip(&self.bodies) {
            if g.centre.z + g.radius < lowest - depth {
                let mut slot = slot.lock().unwrap();
                if slot.take().is_some() {
                    dropped += 1;
                }
            }
        }
        dropped
    }
}

/// The moving particle: its solid and the contact band.
struct Mover<'b> {
    bed: &'b Bed,
    solid: Arc<Solid>,
    radius: f64,
    /// Contact detection band (surface distance), above the cast target gap.
    band: f64,
}

impl Mover<'_> {
    // AI-FUNC-SUMMARY: Exact translation cast from `c` along unit `dir` up to `length`: the largest travel keeping every piece pair at least the bed gap apart, also stopping at the floor; contacts the motion separates from are ignored; no mutation.
    fn cast(&self, c: Vec3, dir: Vec3, length: f64) -> f64 {
        self.bed.casts.fetch_add(1, Relaxed);
        let gap = self.bed.gap;
        let mut best = length;
        if dir.z < 0.0 {
            let room = (c.z + self.solid.low - self.bed.floor - gap).max(0.0);
            best = best.min(room / -dir.z);
        }
        let end = c.add(dir.scale(length));
        let pad = self.radius + gap;
        let lo = Vec3::new(c.x.min(end.x) - pad, c.y.min(end.y) - pad, c.z.min(end.z) - pad);
        let hi = Vec3::new(c.x.max(end.x) + pad, c.y.max(end.y) + pad, c.z.max(end.z) + pad);
        let velocity = na::Vector3::new(dir.x, dir.y, dir.z);
        let still = na::Vector3::zeros();
        for o in self.bed.gather(lo, hi) {
            // Swept bounding spheres first: the obstacle centre's distance to the travelled segment.
            if sweep_miss(c, dir, best, o.centre, self.radius + o.radius + gap) {
                continue;
            }
            let other = self.bed.solid(o.index);
            for a in &self.solid.pieces {
                let ca = c.add(a.offset);
                for b in &other.pieces {
                    let cb = o.centre.add(b.offset);
                    if sweep_miss(ca, dir, best, cb, a.radius + b.radius + gap) {
                        continue;
                    }
                    self.bed.piece_casts.fetch_add(1, Relaxed);
                    let options = ShapeCastOptions {
                        max_time_of_impact: best,
                        target_distance: gap,
                        stop_at_penetration: false,
                        compute_impact_geometry_on_penetration: false,
                    };
                    if let Ok(Some(hit)) = query::cast_shapes(&at(c), &velocity, &a.shape, &at(o.centre), &still, &b.shape, options) {
                        best = best.min(hit.time_of_impact.max(0.0));
                    }
                }
            }
        }
        best
    }

    // AI-FUNC-SUMMARY: Fall straight down until the first contact or the floor; returns the resting centre.
    fn fall(&self, c: Vec3) -> Vec3 {
        let drop = c.z + self.solid.low - self.bed.floor;
        c.add(Vec3::new(0.0, 0.0, -self.cast(c, Vec3::new(0.0, 0.0, -1.0), drop.max(0.0))))
    }

    // AI-FUNC-SUMMARY: Unit contact normals (pointing toward the mover) of every piece pair within the contact band, plus whether the floor is within it; no mutation.
    fn contacts(&self, c: Vec3) -> (Vec<Vec3>, bool) {
        let band = self.band;
        let floor = c.z + self.solid.low - self.bed.floor <= band;
        let pad = self.radius + band;
        let obstacles = self.bed.gather(Vec3::new(c.x - pad, c.y - pad, c.z - pad), Vec3::new(c.x + pad, c.y + pad, c.z + pad));
        self.bed.contact_queries.fetch_add(1, Relaxed);
        let mut normals: Vec<Vec3> = Vec::new();
        for o in &obstacles {
            if vec_norm(c.sub(o.centre)) - self.radius - o.radius >= band {
                continue;
            }
            let other = self.bed.solid(o.index);
            for a in &self.solid.pieces {
                let ca = c.add(a.offset);
                for b in &other.pieces {
                    let cb = o.centre.add(b.offset);
                    if vec_norm(ca.sub(cb)) - a.radius - b.radius >= band {
                        continue;
                    }
                    if let Ok(Some(k)) = query::contact(&at(c), &a.shape, &at(o.centre), &b.shape, band) {
                        let n = Vec3::new(-k.normal1.x, -k.normal1.y, -k.normal1.z);
                        // Pieces of one particle meeting along a seam repeat a normal; keep one.
                        if !normals.iter().any(|m| m.dot(n) > 1.0 - 1e-9) {
                            normals.push(n);
                        }
                    }
                }
            }
        }
        (normals, floor)
    }
}

// AI-FUNC-SUMMARY: True when a sphere of radius `reach` about `p` cannot meet the segment from `c` along unit `dir` of length `len`; no side effects.
fn sweep_miss(c: Vec3, dir: Vec3, len: f64, p: Vec3, reach: f64) -> bool {
    let w = p.sub(c);
    let t = w.dot(dir).clamp(0.0, len);
    vec_norm(w.sub(dir.scale(t))) >= reach
}

// AI-FUNC-SUMMARY: Steepest admissible descent over contact normals: one-contact slides, then two-contact creases; returns a unit direction with negative z, or None when supported.
fn descent(normals: &[Vec3]) -> Option<Vec3> {
    let g = Vec3::new(0.0, 0.0, -1.0);
    if normals.is_empty() {
        return Some(g);
    }
    let admissible = |t: Vec3, skip: &[usize]| {
        normals
            .iter()
            .enumerate()
            .all(|(k, n)| skip.contains(&k) || t.dot(*n) >= -1e-9)
    };
    let better = |t: Vec3, best: Option<Vec3>| t.z < -1e-3 && best.is_none_or(|b| t.z < b.z);
    let mut best: Option<Vec3> = None;
    if admissible(g, &[]) {
        return Some(g);
    }
    for (k, n) in normals.iter().enumerate() {
        let t = g.sub(n.scale(g.dot(*n)));
        let len = vec_norm(t);
        if len > 1e-9 {
            let t = t.scale(1.0 / len);
            if admissible(t, &[k]) && better(t, best) {
                best = Some(t);
            }
        }
    }
    if best.is_some() {
        return best;
    }
    for i in 0..normals.len() {
        for j in i + 1..normals.len() {
            let t = normals[i].cross(normals[j]);
            let len = vec_norm(t);
            if len <= 1e-9 {
                continue;
            }
            let mut t = t.scale(1.0 / len);
            if t.z > 0.0 {
                t = t.scale(-1.0);
            }
            if admissible(t, &[i, j]) && better(t, best) {
                best = Some(t);
            }
        }
    }
    best
}

/// Result of depositing one pending particle.
struct Landing {
    grain: Grain,
    body: Arc<Solid>,
}

// AI-FUNC-SUMMARY: Deposit one pending particle against the committed bed: build its solid, drop from each start, fall and slide contact to contact; keep the lowest resting pose (first on ties); pure function of bed and variates.
fn deposit(bed: &Bed, p: &Pending, spec: &SettleSpec) -> Landing {
    let g = &p.grain;
    let body = Arc::new(Solid::build(&bed.templates[g.shell], g.scale, g.rotation));
    let mover = Mover {
        bed,
        solid: body.clone(),
        radius: g.radius,
        band: bed.gap + spec.contact_tolerance * g.radius,
    };
    let mut best: Option<(Vec3, usize, &'static str)> = None;
    for &(x, y) in &p.starts {
        let top = bed.top_near(x, y, g.radius);
        let z = (top + g.radius).max(bed.floor - body.low) + 2.0 * mover.band;
        let start = Vec3::new(x, y, z);
        let mut c = mover.fall(start);
        let mut rolls = 0;
        let mut rest = "roll_budget";
        while rolls < spec.roll_moves {
            let (normals, floor) = mover.contacts(c);
            if floor {
                rest = "floor";
                break;
            }
            if normals.is_empty() {
                c = mover.fall(c);
                rolls += 1;
                continue;
            }
            let Some(t) = descent(&normals) else {
                rest = "supported";
                break;
            };
            // Tilt off the contacts being slid on so the cast starts separating from them.
            let lift = normals.iter().filter(|n| t.dot(**n) < 1e-6).fold(Vec3::new(0.0, 0.0, 0.0), |a, n| a.add(*n));
            let dir = t.add(lift.scale(LIFT));
            let dir = dir.scale(1.0 / vec_norm(dir));
            // Longest slide that keeps the centre within the horizontal reach of its start.
            let (hx, hy) = (c.x - start.x, c.y - start.y);
            let (dx, dy) = (dir.x, dir.y);
            let a = dx * dx + dy * dy;
            let room = if a <= 1e-18 {
                f64::INFINITY
            } else {
                let b = hx * dx + hy * dy;
                let q = b * b - a * (hx * hx + hy * hy - p.reach * p.reach);
                if q <= 0.0 { 0.0 } else { (-b + q.sqrt()) / a }
            };
            let length = (SLIDE * g.radius).min(room);
            if length <= spec.contact_tolerance * g.radius {
                rest = "reach";
                break;
            }
            let travel = mover.cast(c, dir, length);
            if travel <= 1e-9 * g.radius {
                rest = "stuck";
                break;
            }
            c = c.add(dir.scale(travel));
            rolls += 1;
        }
        if best.is_none_or(|(b, _, _)| c.z < b.z) {
            best = Some((c, rolls, rest));
        }
    }
    let (centre, rolls, rest) = best.unwrap();
    let mut grain = g.clone();
    grain.centre = centre;
    grain.rolls = rolls;
    grain.rest = rest;
    Landing { grain, body }
}

// AI-FUNC-SUMMARY: Exact nearest-neighbour surface gaps (capped) of a seeded sample of accepted particles at their true size, overall and by diameter band; builds each needed body once; no mutation of placement state.
fn measure_gaps(
    grains: &[Grain],
    kept: &[usize],
    state: &EngineState,
    library: &ShapeLibrary,
    s: f64,
    count: usize,
    rng: &mut rand_chacha::ChaCha12Rng,
) -> Value {
    let cap = 3.0;
    let mut sample: Vec<usize> = (0..kept.len()).collect();
    for k in (1..sample.len()).rev() {
        let j = uniform_index(rng, k + 1);
        sample.swap(k, j);
    }
    sample.truncate(count);
    sample.sort_unstable();
    let cache: Vec<OnceLock<Posed>> = (0..kept.len()).map(|_| OnceLock::new()).collect();
    let body = |i: usize| {
        cache[i].get_or_init(|| {
            let g = &grains[kept[i]];
            let sh = &library.shells[g.shell];
            let scale = g.scale / s;
            Posed::at(
                Arc::new(Body::build(&sh.canonical, scale, sh.volume * scale.powi(3)).expect("library shell was prepared before")),
                g.rotation,
                g.centre,
                g.radius / s,
            )
        })
    };
    let rows: Vec<(f64, f64)> = sample
        .par_iter()
        .map(|&i| {
            let me = body(i);
            let mut best = cap;
            for j in state.grid.query_neighbors_with_margin(state.placed[i].bbox, cap, usize::MAX) {
                if j == i {
                    continue;
                }
                best = best.min(distance_capped(me, body(j), best));
            }
            (grains[kept[i]].diameter, best)
        })
        .collect();
    let quantiles = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.total_cmp(b));
        let q = |p: f64| v.get(((v.len() as f64 - 1.0) * p).round() as usize).copied();
        json!({"n": v.len(), "min": q(0.0), "p05": q(0.05), "p25": q(0.25), "p50": q(0.5), "p75": q(0.75), "p95": q(0.95), "max": q(1.0)})
    };
    let bands = [(0.0, 2.0), (2.0, 3.0), (3.0, 4.5), (4.5, f64::INFINITY)];
    json!({
        "unit_note": "frame units; nearest-neighbour surface distance capped at 3",
        "cap": cap,
        "all": quantiles(rows.iter().map(|r| r.1).collect()),
        "gap_over_diameter": quantiles(rows.iter().map(|r| r.1 / r.0).collect()),
        "by_diameter": bands.iter().map(|&(lo, hi)| json!({"d_lo": lo, "d_hi": if hi.is_finite() { json!(hi) } else { json!(null) },
            "gap": quantiles(rows.iter().filter(|r| r.0 >= lo && r.0 < hi).map(|r| r.1).collect())})).collect::<Vec<_>>(),
    })
}

/// Deposition summary for settle.json.
#[derive(serde::Serialize)]
struct BedStats {
    deposited: usize,
    waves: usize,
    mean_wave: f64,
    stop: String,
    floor_z: f64,
    top_target_z: f64,
    lowest_surface_z: f64,
    highest_surface_z: f64,
    pieces_per_shell: Vec<usize>,
    bodies_rebuilt: u64,
    casts: u64,
    piece_casts: u64,
    contact_queries: u64,
    rest_reasons: BTreeMap<String, usize>,
    mean_rolls: f64,
    profile: Vec<Value>,
    seconds: f64,
}

// AI-FUNC-SUMMARY: Deposit enlarged particles, shrink them, filter them with the exact checks, optionally thin to the target, and write all standard outputs plus settle.json/settle_bed.json; returns the outcome.
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    source: &SizeSource,
    classes: &[SizeClass],
    void: Option<&VoidIndex>,
    void_volume: f64,
    void_method: Option<VoidVolumeMethod>,
    basis: f64,
    tool: &ToolRecord,
    started: Instant,
) -> Result<PlacementOutcome> {
    let spec = &config.settle;
    let s = spec.scale;
    let domain = config.domain;
    let size = domain.size();
    let d50 = source.quantile(0.5) * s;
    let floor_margin = spec.floor_margin.unwrap_or(5.0 * d50);
    let top_margin = spec.top_margin.unwrap_or(5.0 * d50);
    let floor = domain.min.z - floor_margin;
    let top_target = domain.max.z + top_margin;

    // Plan true sizes for a bed that could not hold more: its whole volume at the true size.
    let mut rng = seeded_rng(config.seed);
    let bed_volume = size.x * size.y * (top_target - floor + 4.0 * d50);
    let plan = plan_size_multiset(&mut rng, source, classes, bed_volume / (s * s * s), MAX_PLANNED_PARTICLES)?;
    let rmax = plan
        .draws
        .iter()
        .map(|d| d.diameter / 2.0 * s)
        .fold(0.0, f64::max)
        * library.max_extent_ratio.max(1.0);
    let reach_max = spec.roll_reach * rmax;
    let cell = 2.0 * rmax;
    // Only a body overlapping its own periodic image is impossible to handle; small periods just
    // make every pending pair conflict, so waves shrink to one particle.
    if size.x <= 2.0 * rmax || size.y <= 2.0 * rmax {
        return Err(RustMsptError::InvalidConfig(format!(
            "placement.settle: the x/y period ({} x {}) must exceed the largest enlarged particle reach (2 x {rmax})",
            size.x, size.y
        )));
    }

    let mut state = EngineState::new(config, library, classes, &plan.draws);
    state.basis_volume = basis;
    state.control.phase = "settling";
    let templates: Vec<Template> = library
        .shells
        .par_iter()
        .map(|sh| template(&sh.canonical, spec.geometry, spec.concavity))
        .collect();
    let pieces: Vec<usize> = templates.iter().map(Vec::len).collect();
    let mut bed = Bed::new(domain, floor, cell, rmax, spec.gap * d50, templates);
    let mut queue: std::collections::VecDeque<Pending> = std::collections::VecDeque::new();
    let mut draws = plan.draws.iter();
    let mut waves = 0usize;
    let mut wave_members = 0usize;
    let stop;
    loop {
        if state.poll_control(config, false) {
            stop = "interrupted";
            break;
        }
        let lowest = bed.surface.iter().copied().fold(f64::INFINITY, f64::min);
        if lowest >= top_target {
            stop = "bed_complete";
            break;
        }
        while queue.len() < QUEUE {
            let Some(d) = draws.next() else { break };
            let shell = uniform_index(&mut rng, library.shells.len());
            let rotation = match config.orientation {
                OrientationMode::UniformSo3 => sample_uniform_quaternion(&mut rng),
                OrientationMode::Fixed => {
                    let _ = (u01(&mut rng), u01(&mut rng), u01(&mut rng));
                    UnitQuat::identity()
                }
            };
            let sh = &library.shells[shell];
            let scale = d.diameter / sh.equivalent_diameter * s;
            let radius = sh.bounding_radius * scale;
            let starts = (0..spec.drop_candidates)
                .map(|_| {
                    (
                        uniform_range(&mut rng, domain.min.x, domain.max.x),
                        uniform_range(&mut rng, domain.min.y, domain.max.y),
                    )
                })
                .collect();
            queue.push_back(Pending {
                grain: Grain {
                    shell,
                    diameter: d.diameter,
                    class: d.class,
                    draw_index: d.draw_index,
                    scale,
                    radius,
                    rotation,
                    centre: Vec3::new(0.0, 0.0, 0.0),
                    volume: sh.volume * scale * scale * scale,
                    rolls: 0,
                    rest: "",
                },
                starts,
                reach: spec.roll_reach * radius,
            });
        }
        if queue.is_empty() {
            stop = "plan_exhausted";
            break;
        }
        if bed.grains.len() >= spec.max_particles {
            stop = "max_particles";
            break;
        }
        // A wave: each member is disjoint (periodically, over all its starts) from every earlier
        // pending particle, so no member can read what another writes.
        let periodic = |a: (f64, f64), b: (f64, f64)| {
            let mut dx = (a.0 - b.0).abs() % size.x;
            let mut dy = (a.1 - b.1).abs() % size.y;
            dx = dx.min(size.x - dx);
            dy = dy.min(size.y - dy);
            (dx * dx + dy * dy).sqrt()
        };
        let mut chosen = Vec::new();
        for i in 0..queue.len() {
            let pi = &queue[i];
            let free = (0..i).all(|j| {
                let pj = &queue[j];
                // Each particle reads and writes only within radius + reach (+ contact band) of
                // its starts.
                let need = (pi.grain.radius + pi.reach + pj.grain.radius + pj.reach) * (1.0 + 1e-3) + 1e-9;
                pi.starts.iter().all(|a| pj.starts.iter().all(|b| periodic(*a, *b) > need))
            });
            if free {
                chosen.push(i);
            }
        }
        let landings: Vec<Landing> = chosen
            .par_iter()
            .map(|&i| deposit(&bed, &queue[i], spec))
            .collect();
        for landing in landings {
            bed.commit(landing.grain, landing.body);
        }
        for &i in chosen.iter().rev() {
            queue.remove(i);
        }
        waves += 1;
        wave_members += chosen.len();
        state.attempts = bed.grains.len();
        if waves % EVICT_EVERY == 0 {
            bed.evict(4.0 * (rmax + reach_max));
        }
    }
    let deposit_seconds = started.elapsed().as_secs_f64();

    // Bed statistics at the enlarged size; the shrunk fraction is this times 1/s^3.
    let slabs = 20usize;
    let span = top_target - floor;
    let mut slab_volume = vec![0.0; slabs];
    for g in &bed.grains {
        let k = ((g.centre.z - floor) / span * slabs as f64).floor();
        if k >= 0.0 && (k as usize) < slabs {
            slab_volume[k as usize] += g.volume;
        }
    }
    let slab_box = size.x * size.y * span / slabs as f64;
    let profile = slab_volume
        .iter()
        .enumerate()
        .map(|(k, v)| {
            json!({"z_lo": floor + span * k as f64 / slabs as f64, "z_hi": floor + span * (k + 1) as f64 / slabs as f64,
                   "vf_enlarged_by_centre": v / slab_box, "vf_shrunk_by_centre": v / slab_box / (s * s * s)})
        })
        .collect();
    let mut rest_reasons = BTreeMap::new();
    let mut rolls = 0usize;
    for g in &bed.grains {
        *rest_reasons.entry(g.rest.to_string()).or_insert(0) += 1;
        rolls += g.rolls;
    }
    let bed_stats = BedStats {
        deposited: bed.grains.len(),
        waves,
        mean_wave: wave_members as f64 / waves.max(1) as f64,
        stop: stop.into(),
        floor_z: floor,
        top_target_z: top_target,
        lowest_surface_z: bed.surface.iter().copied().fold(f64::INFINITY, f64::min),
        highest_surface_z: bed.surface.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        pieces_per_shell: pieces,
        bodies_rebuilt: bed.rebuilt.load(Relaxed),
        casts: bed.casts.load(Relaxed),
        piece_casts: bed.piece_casts.load(Relaxed),
        contact_queries: bed.contact_queries.load(Relaxed),
        rest_reasons,
        mean_rolls: rolls as f64 / bed.grains.len().max(1) as f64,
        profile,
        seconds: deposit_seconds,
    };
    eprintln!(
        "[Settle] deposited={} waves={} mean_wave={:.1} stop={} {:.1}s casts={} piece_casts={} contacts={} rebuilt={}",
        bed_stats.deposited, waves, bed_stats.mean_wave, stop, deposit_seconds, bed_stats.casts, bed_stats.piece_casts, bed_stats.contact_queries, bed_stats.bodies_rebuilt
    );

    let grains = std::mem::take(&mut bed.grains);
    drop(bed);
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
            s,
            cell,
            measure_gaps: spec.measure_gaps,
            pair_check: true,
            label: "Settle",
            stop: stop.to_string(),
            produced: bed_stats.deposited,
            report: json!({"config": spec, "deposition": bed_stats}),
        },
        state,
        grains,
        rng,
    )
}

/// Everything the shrink-filter-output stage needs besides the grains.
pub(super) struct Finish<'a> {
    pub config: &'a ResolvedPlacement,
    pub library: &'a ShapeLibrary,
    pub classes: &'a [SizeClass],
    pub void: Option<&'a VoidIndex>,
    pub void_volume: f64,
    pub void_method: Option<VoidVolumeMethod>,
    pub basis: f64,
    pub tool: &'a ToolRecord,
    pub started: Instant,
    pub plan: &'a SizePlan,
    /// Grains carry s times their true scale; s = 1 for grains already at true size.
    pub s: f64,
    pub cell: f64,
    pub measure_gaps: usize,
    /// Check every pair exactly; false when the producer already guarantees pair separation.
    pub pair_check: bool,
    pub label: &'static str,
    pub stop: String,
    pub produced: usize,
    /// Mode-specific fields merged into settle.json.
    pub report: Value,
}

// AI-FUNC-SUMMARY: Shrink grains by 1/s, delete those failing the exact void/boundary rules (and pair rules when asked), thin uniformly to the target, measure gaps, and write all standard outputs plus settle.json/settle_bed.json; returns the outcome.
pub(super) fn finish(f: &Finish, mut state: EngineState, grains: Vec<Grain>, mut rng: rand_chacha::ChaCha12Rng) -> Result<PlacementOutcome> {
    let Finish { config, library, classes, void, void_volume, void_method, basis, tool, started, plan, s, cell, .. } = *f;
    let domain = config.domain;
    // Shrink and filter. Only grains whose true bounding sphere meets the domain can survive.
    state.control.phase = "filtering";
    let proposal = |g: &Grain| {
        let sh = &library.shells[g.shell];
        let scale = g.scale / s;
        Proposal {
            shell_index: g.shell,
            scale,
            rotation: g.rotation,
            centre: g.centre,
            reach: sh.bounding_radius * scale,
            fits_domain: true,
            stream_after: 0,
            guided_cell: None,
        }
    };
    let near: Vec<usize> = (0..grains.len())
        .filter(|&i| {
            let g = &grains[i];
            let r = g.radius / s;
            let c = g.centre;
            c.x + r >= domain.min.x && c.x - r <= domain.max.x && c.y + r >= domain.min.y && c.y - r <= domain.max.y && c.z + r >= domain.min.z && c.z - r <= domain.max.z
        })
        .collect();
    let empty_grid = SpatialGrid::new(domain, cell.max(1e-9));
    // Stage 1: void and boundary rules alone, in parallel.
    let alone: Vec<std::result::Result<f64, RejectReason>> = near
        .par_iter()
        .map(|&i| match evaluate_proposal(config, library, void, &[], &empty_grid, None, &proposal(&grains[i])) {
            Evaluation::Accepted(hit) => Ok(hit.accepted.volume_in_domain),
            Evaluation::Rejected(reason) => Err(reason),
        })
        .collect();
    let mut survivors = Vec::new();
    let mut deleted: BTreeMap<String, usize> = BTreeMap::new();
    let mut survivor_volume = 0.0;
    for (&i, r) in near.iter().zip(&alone) {
        match r {
            Ok(v) => {
                survivors.push(i);
                survivor_volume += v;
            }
            Err(reason) => {
                state.reject(*reason);
                *deleted.entry(format!("{reason:?}")).or_insert(0) += 1;
            }
        }
    }
    let before_thinning_vf = survivor_volume / basis;
    // Stage 2: uniform random thinning to the target (PSD preserved in expectation).
    let target = basis * config.target_volume_fraction;
    let mut thinned = 0usize;
    if survivor_volume > target * (1.0 + config.target_tolerance) {
        let volumes: BTreeMap<usize, f64> = near
            .iter()
            .zip(&alone)
            .filter_map(|(&i, r)| r.as_ref().ok().map(|v| (i, *v)))
            .collect();
        let mut order = survivors.clone();
        for k in (1..order.len()).rev() {
            let j = uniform_index(&mut rng, k + 1);
            order.swap(k, j);
        }
        let mut removed = std::collections::BTreeSet::new();
        let mut volume = survivor_volume;
        for i in order {
            let v = volumes[&i];
            if (volume - v - target).abs() >= (volume - target).abs() {
                break;
            }
            volume -= v;
            removed.insert(i);
        }
        thinned = removed.len();
        survivors.retain(|i| !removed.contains(i));
    }
    // Stage 3: acceptance in production order; exact pair checks against the accepted set, or,
    // when the producer guarantees separation, the void/boundary evaluation again in parallel
    // chunks (it carries the clipped mesh the accept step needs).
    let mut kept: Vec<usize> = Vec::new();
    if !f.pair_check {
        for chunk in survivors.chunks(4096) {
            let evaluated: Vec<Evaluation> = chunk
                .par_iter()
                .map(|&i| evaluate_proposal(config, library, void, &[], &empty_grid, None, &proposal(&grains[i])))
                .collect();
            for (&i, e) in chunk.iter().zip(evaluated) {
                let Evaluation::Accepted(hit) = e else { unreachable!("stage 1 accepted this grain") };
                let g = &grains[i];
                let p = proposal(g);
                let AcceptedProposal { candidate, bbox, volume_full, accepted } = *hit;
                accept(
                    &mut state,
                    p.shell_index,
                    library,
                    &SizeDraw { diameter: g.diameter, class: g.class, draw_index: g.draw_index },
                    p.scale,
                    p.rotation,
                    p.centre,
                    p.reach,
                    volume_full,
                    bbox,
                    0.0,
                    candidate,
                    accepted,
                );
                state.placed_per_class[g.class] += 1;
                kept.push(i);
            }
        }
    }
    for &i in survivors.iter().filter(|_| f.pair_check) {
        if state.poll_control(config, false) {
            break;
        }
        let g = &grains[i];
        let p = proposal(g);
        match evaluate_proposal(config, library, void, &state.placed, &state.grid, None, &p) {
            Evaluation::Rejected(reason) => {
                state.reject(reason);
                *deleted.entry(format!("pair_{reason:?}")).or_insert(0) += 1;
            }
            Evaluation::Accepted(hit) => {
                let AcceptedProposal { candidate, bbox, volume_full, accepted } = *hit;
                accept(
                    &mut state,
                    p.shell_index,
                    library,
                    &SizeDraw { diameter: g.diameter, class: g.class, draw_index: g.draw_index },
                    p.scale,
                    p.rotation,
                    p.centre,
                    p.reach,
                    volume_full,
                    bbox,
                    0.0,
                    candidate,
                    accepted,
                );
                state.placed_per_class[g.class] += 1;
                kept.push(i);
            }
        }
    }
    // drawn_per_class counted every planned draw; report the grains that met the domain instead.
    state.drawn_per_class = vec![0; classes.len()];
    for &i in &near {
        state.drawn_per_class[grains[i].class] += 1;
    }

    let gaps = if f.measure_gaps > 0 && !kept.is_empty() {
        state.control.phase = "measuring_gaps";
        Some(measure_gaps(&grains, &kept, &state, library, s, f.measure_gaps, &mut rng))
    } else {
        None
    };
    let settle_path = config.outputs.dir.join("settle.json");
    let mut summary = json!({
            "schema_version": "rustmspt.settle/1",
            "mode": f.label,
            "filter": {
                "grains_meeting_domain": near.len(),
                "deleted": deleted,
                "thinned": thinned,
                "vf_basis_after_deletion_before_thinning": before_thinning_vf,
                "accepted": state.placed.len(),
                "vf_basis": state.volume_solid / basis,
            },
            "gaps_after_shrink": gaps,
            "note": "nearest-neighbour surface gaps of accepted particles at true size, measured on the exact meshes",
        });
    if let (Some(out), Some(extra)) = (summary.as_object_mut(), f.report.as_object()) {
        out.extend(extra.clone());
    }
    write_json(&settle_path, &summary)?;
    let bed_path = config.outputs.dir.join("settle_bed.json");
    write_json(&bed_path, &json!({"schema_version": "rustmspt.settle_bed/1", "scale": s, "grains": grains}))?;

    let frame = FrameRecord {
        unit: config.unit.clone(),
        origin: [domain.min.x, domain.min.y, domain.min.z],
        axis_order: "xyz".into(),
        handedness: "right".into(),
        domain: DomainRecord {
            min: [domain.min.x, domain.min.y, domain.min.z],
            max: [domain.max.x, domain.max.y, domain.max.z],
        },
    };
    let threads = rayon::current_num_threads();
    let mut report = blank_report(
        config,
        tool,
        &frame,
        library,
        plan,
        basis,
        threads,
        build_void_report(config, void, void_volume, void_method)?,
    );
    report.samplers.insert("position".into(), "gravity_deposition_enlarged_then_shrunk".into());
    report.samplers.insert("sizes".into(), "iid_draws_in_deposition_order".into());
    let vf = state.volume_solid / basis;
    let reached = (state.volume_solid - target).abs() <= target * config.target_tolerance;
    let reason = if state.control.interrupted {
        StopReason::Interrupted
    } else if state.placed.is_empty() {
        StopReason::NoFeasiblePlacement
    } else if reached {
        StopReason::TargetReached
    } else {
        StopReason::DistributionUnattainable
    };
    let stop_decision = StopDecision {
        reason,
        detail: BTreeMap::from([
            ("message".into(), "Settle deposition: enlarged gravity deposit, shrunk about centroids, exact void/boundary/pair filter, uniform thinning.".into()),
            ("deposition_stop".into(), f.stop.clone()),
            ("deposited".into(), f.produced.to_string()),
            ("thinned".into(), thinned.to_string()),
            ("vf_before_thinning".into(), before_thinning_vf.to_string()),
        ]),
    };
    state.control.publish(config, "saving", state.placed.len(), state.attempts, vf);
    let mut outputs = write_outputs(config, library, &state, tool, &frame, classes, void)?;
    outputs.push(describe_output("settle", &settle_path));
    outputs.push(describe_output("settle_bed", &bed_path));
    finish_report(
        &mut report,
        config,
        &state,
        classes,
        &TopUpReport { batches: 0, drawn: 0, placed: 0 },
        &stop_decision,
        started.elapsed().as_secs_f64(),
        threads,
        outputs,
        target,
    );
    write_json(&config.outputs.report, &report)?;
    state.control.publish(config, &report.status, state.placed.len(), state.attempts, vf);
    Ok(PlacementOutcome {
        placed: state.placed.len(),
        stop_reason: reason,
        volume_fraction_solid: vf,
        summary_lines: vec![format!(
            "[{}] s={s} produced={} meeting_domain={} accepted={} thinned={thinned} VF={vf:.8} (before thinning {before_thinning_vf:.8}) stop={reason:?}",
            f.label,
            f.produced,
            near.len(),
            state.placed.len()
        )],
    })
}
