//! Deterministic, resumable rigid-body contact growth. No random insertion.
use super::bodies::{clearance_capped, Bodies, Posed};
use super::*;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Growth {
    pending: Vec<Member>,
    members: Vec<Member>,
    best: Vec<Member>,
    initial_half: f64,
    next: usize,
    sweep: usize,
    member: usize,
    relaxing: bool,
    stats: SearchStats,
    #[serde(skip)]
    bodies: Bodies,
}

/// Search switches resolved once per step from the aggregate specification.
struct Knobs {
    principal: bool,
    settle_steps: usize,
    settle_candidates: usize,
}
// AI-FUNC-SUMMARY: Resolve contact search switches from the spec; no mutation.
fn knobs(spec: &AggregateSpec) -> Knobs {
    Knobs {
        principal: spec.contact_orientation == ContactOrientation::Principal,
        settle_steps: spec.contact_settle_steps,
        settle_candidates: spec.contact_settle_candidates,
    }
}

// AI-FUNC-SUMMARY: Generate fixed spherical directions, independent of RNG/thread count.
fn directions(n: usize) -> Vec<Vec3> {
    (0..n)
        .map(|i| {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let a = i as f64 * std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
            let r = (1.0 - z * z).sqrt();
            Vec3::new(r * a.cos(), r * a.sin(), z)
        })
        .collect()
}

// AI-FUNC-SUMMARY: Unit vector orthogonal to u (deterministic choice); no mutation.
fn orthogonal(u: Vec3) -> Vec3 {
    let a = if u.x.abs() < 0.6 {
        Vec3::new(1., 0., 0.)
    } else {
        Vec3::new(0., 1., 0.)
    };
    let v = a.sub(u.scale(a.dot(u)));
    v.scale(1. / vec_norm(v))
}

// AI-FUNC-SUMMARY: Rotation (unit quaternion) mapping orthonormal local frame columns onto world frame columns; right-handedness enforced on both.
fn frame_rotation(local: [Vec3; 3], world: [Vec3; 3]) -> UnitQuat {
    use parry3d_f64::na;
    let fix = |f: [Vec3; 3]| [f[0], f[1], f[0].cross(f[1])];
    let (l, w) = (fix(local), fix(world));
    let lm = na::Matrix3::from_columns(&l.map(|v| na::Vector3::new(v.x, v.y, v.z)));
    let wm = na::Matrix3::from_columns(&w.map(|v| na::Vector3::new(v.x, v.y, v.z)));
    let r = na::Rotation3::from_matrix(&(wm * lm.transpose()));
    let q = na::UnitQuaternion::from_rotation_matrix(&r);
    UnitQuat::new(q.w, q.i, q.j, q.k).unwrap_or_else(UnitQuat::identity)
}

// AI-FUNC-SUMMARY: Candidate start orientation o for approach direction d. Principal mode lays the
// minor (even o) or middle (odd o) axis along d and spins about it; legacy mode uses fixed rotations.
fn orientation(o: usize, n: usize, d: Vec3, axes: [Vec3; 3], principal: bool) -> Option<UnitQuat> {
    if principal {
        let normal = if o.is_multiple_of(2) { axes[2] } else { axes[0] };
        let others = if o.is_multiple_of(2) {
            [axes[0], axes[1]]
        } else {
            [axes[1], axes[2]]
        };
        let spin = (o / 2) as f64 * std::f64::consts::PI / n.div_ceil(2) as f64;
        let e1 = orthogonal(d);
        let e2 = d.cross(e1);
        let t1 = e1.scale(spin.cos()).add(e2.scale(spin.sin()));
        let t2 = d.cross(t1);
        // local (others[0], others[1], normal) -> world (t1, t2, d)
        let local = [others[0], others[1], normal];
        let local = if others[0].cross(others[1]).dot(normal) < 0. {
            [others[1], others[0], normal]
        } else {
            local
        };
        return Some(frame_rotation(local, [t1, t2, d]));
    }
    if o == 0 {
        return None;
    }
    let axis = directions(n)[o];
    let half = (o as f64 / n as f64) * std::f64::consts::PI;
    UnitQuat::new(
        half.cos(),
        axis.x * half.sin(),
        axis.y * half.sin(),
        axis.z * half.sin(),
    )
}

/// Everything one motion needs that does not change during it.
struct Ctx<'a> {
    spec: &'a AggregateSpec,
    bodies: &'a Bodies,
    config: &'a ResolvedPlacement,
}

// AI-FUNC-SUMMARY: Advance rigid translation/axis rotation by conservative advancement: each step
// moves less than the current exact clearance (Lipschitz bound), so the swept path never crosses or
// tunnels. Distances are capped at what the remaining motion could use. Returns last feasible pose.
#[allow(clippy::too_many_arguments)]
fn advance(
    start: &Member,
    delta: Vec3,
    axis: Vec3,
    angle: f64,
    obstacles: &[Posed],
    ctx: &Ctx,
    control: &mut PlacementControl,
    stats: &mut SearchStats,
) -> Member {
    let mut result = start.clone();
    let speed = vec_norm(delta) + start.radius * angle.abs();
    if speed == 0.0 {
        return result;
    }
    stats.advances += 1;
    let spec = ctx.spec;
    let tol = spec.contact_tolerance * start.radius.max(1e-12);
    let guard = 1e-7 * start.radius.max(1.0);
    let floor = spec.internal_gap + guard;
    // Capping the query keeps branch-and-bound pruning tight: a far obstacle only needs to be
    // proven farther than this step can use. Long motions take a few more, cheaper steps.
    let reach = start.radius * 0.5;
    let cap = |remaining: f64| floor + (speed * remaining / 0.8 * (1.0 + 1e-9) + tol).min(reach);
    let mut slack = clearance_capped(&ctx.bodies.pose(start), obstacles, cap(1.0)) - floor;
    let mut t = 0.0;
    let mut previous_slack = f64::NEG_INFINITY;
    for _ in 0..spec.contact_max_steps {
        if control.poll(ctx.config, false, 0, 0, 0.0) {
            break;
        }
        if slack <= 0.0 || (slack <= tol && slack <= previous_slack) {
            break;
        }
        let dt = (0.8 * slack / speed).min(1.0 - t);
        if dt <= f64::EPSILON {
            break;
        }
        let next_t = t + dt;
        let mut trial = start.clone();
        trial.centre = start.centre.add(delta.scale(next_t));
        let half = angle * next_t * 0.5;
        let q = UnitQuat::new(
            half.cos(),
            axis.x * half.sin(),
            axis.y * half.sin(),
            axis.z * half.sin(),
        )
        .unwrap();
        trial.rotation = compose_rotation(q, start.rotation);
        stats.advance_steps += 1;
        let next_slack =
            clearance_capped(&ctx.bodies.pose(&trial), obstacles, cap(1.0 - next_t)) - floor;
        if next_slack < 0.0 {
            // The displacement bound makes this unreachable; refuse rather than trust round-off.
            break;
        }
        result = trial;
        t = next_t;
        previous_slack = slack;
        slack = next_slack;
        if t >= 1.0 - 1e-14 {
            break;
        }
    }
    result
}

// AI-FUNC-SUMMARY: Tighten enclosing centre by bbox centring and deterministic pattern search; preserves pair distances, never understates the envelope.
fn centre_envelope(members: &mut [Member], spec: &AggregateSpec, bodies: &Bodies) {
    let mut min = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut max = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for m in members.iter() {
        for v in bodies.hull(m) {
            let q = m.rotation.rotate_point(*v).add(m.centre);
            min = Vec3::new(min.x.min(q.x), min.y.min(q.y), min.z.min(q.z));
            max = Vec3::new(max.x.max(q.x), max.y.max(q.y), max.z.max(q.z));
        }
    }
    let mut half = bodies.envelope(members, spec.shape);
    let shift = min.add(max).scale(0.5);
    let mut trial = members.to_vec();
    for m in &mut trial {
        m.centre = m.centre.sub(shift);
    }
    let h = bodies.envelope(&trial, spec.shape);
    if h < half {
        members.clone_from_slice(&trial);
        half = h;
    }
    let mut step = half * 0.1;
    for _ in 0..16 {
        for axis in [
            Vec3::new(1., 0., 0.),
            Vec3::new(0., 1., 0.),
            Vec3::new(0., 0., 1.),
        ] {
            for sign in [-1., 1.] {
                let mut trial = members.to_vec();
                for m in &mut trial {
                    m.centre = m.centre.add(axis.scale(sign * step));
                }
                let h = bodies.envelope(&trial, spec.shape);
                if h < half {
                    members.clone_from_slice(&trial);
                    half = h;
                }
            }
        }
        step *= 0.5;
    }
}

// AI-FUNC-SUMMARY: Compare whole-envelope size first, then total squared radii to avoid ties trapping interior particles.
fn score(members: &[Member], spec: &AggregateSpec, bodies: &Bodies) -> (f64, f64) {
    (
        bodies.envelope(members, spec.shape),
        members.iter().map(|m| m.centre.dot(m.centre)).sum(),
    )
}
// AI-FUNC-SUMMARY: Lexicographic envelope/centre-cost acceptance, no state mutation.
fn better(a: (f64, f64), b: (f64, f64)) -> bool {
    a.0 < b.0 || (a.0 <= b.0 && a.1 < b.1)
}

// AI-FUNC-SUMMARY: Roll a contacting member toward the aggregate centre: lift, slide tangentially, fall; accept only strictly closer feasible poses; halves the slide on failure.
fn settle(
    start: Member,
    steps: usize,
    obstacles: &[Posed],
    ctx: &Ctx,
    control: &mut PlacementControl,
    stats: &mut SearchStats,
) -> Member {
    let mut current = start;
    let mut slide = current.radius * 0.5;
    for _ in 0..steps {
        if control.interrupted || slide < current.radius * 1e-3 {
            break;
        }
        let norm = vec_norm(current.centre);
        if norm <= 0.0 {
            break;
        }
        let radial = current.centre.scale(1. / norm);
        let e1 = orthogonal(radial);
        let e2 = radial.cross(e1);
        let mut improved = false;
        for dir in [e1, e1.scale(-1.), e2, e2.scale(-1.)] {
            let none = Vec3::new(1., 0., 0.);
            let lifted = advance(&current, radial.scale(slide * 0.25), none, 0., obstacles, ctx, control, stats);
            let moved = advance(&lifted, dir.scale(slide), none, 0., obstacles, ctx, control, stats);
            let fallen = advance(&moved, moved.centre.scale(-1.), none, 0., obstacles, ctx, control, stats);
            if control.interrupted {
                return current;
            }
            if vec_norm(fallen.centre) < norm - 1e-6 * current.radius {
                current = fallen;
                improved = true;
                stats.lateral_moves += 1;
                break;
            }
        }
        if !improved {
            slide *= 0.5;
        }
    }
    current
}

/// Result of searching insertion poses for one member against fixed others.
struct Insertion {
    member: Member,
    interrupted: bool,
}

// AI-FUNC-SUMMARY: Add a worker's motion counters into the orchestrator's statistics; trial accounting stays with the orchestrator.
fn absorb(into: &mut SearchStats, from: &SearchStats) {
    into.advances += from.advances;
    into.advance_steps += from.advance_steps;
    into.lateral_moves += from.lateral_moves;
}

// AI-FUNC-SUMMARY: Run independent motion jobs on the rayon pool with cancellation-only worker controls; results keep job order, so output is thread-count independent. Latches interruption into control.
fn parallel<T: Send, R: Send>(
    jobs: Vec<T>,
    control: &mut PlacementControl,
    stats: &mut SearchStats,
    work: impl Fn(T, &mut PlacementControl, &mut SearchStats) -> R + Sync,
) -> Vec<R> {
    use rayon::prelude::*;
    let base = control.worker();
    let out: Vec<(R, SearchStats, bool)> = jobs
        .into_par_iter()
        .map(|job| {
            let mut c = base.worker();
            let mut s = SearchStats::default();
            let r = work(job, &mut c, &mut s);
            (r, s, c.interrupted)
        })
        .collect();
    let mut results = Vec::with_capacity(out.len());
    for (r, s, interrupted) in out {
        absorb(stats, &s);
        control.interrupted |= interrupted;
        results.push(r);
    }
    results
}

// AI-FUNC-SUMMARY: Search deterministic approach directions/orientations for one member against fixed members; straight approaches first (in parallel), then settle the best few (in parallel). Always returns a feasible pose (the exterior fallback when nothing better is found).
#[allow(clippy::too_many_arguments)]
fn insertion_search(
    incoming: &Member,
    fixed: &[Member],
    limit: usize,
    ctx: &Ctx,
    knobs: &Knobs,
    control: &mut PlacementControl,
    stats: &mut SearchStats,
) -> Insertion {
    let spec = ctx.spec;
    let obstacles: Vec<Posed> = fixed.iter().map(|m| ctx.bodies.pose(m)).collect();
    let radius = ctx.bodies.envelope(fixed, AggregateShape::Sphere)
        + incoming.radius
        + spec.internal_gap
        + incoming.radius * 0.02
        + 1e-8;
    let mut fallback = incoming.clone();
    fallback.centre = Vec3::new(radius, 0., 0.);
    let evaluate = |m: &Member| {
        let mut trial_members = fixed.to_vec();
        trial_members.push(m.clone());
        score(&trial_members, spec, ctx.bodies)
    };
    let orientations = if spec.rotation_search {
        spec.contact_orientations
    } else {
        1
    };
    let axes = ctx.bodies.axes(incoming);
    let mut starts = Vec::new();
    'plan: for (k, d) in directions(spec.contact_directions).into_iter().enumerate() {
        for o in 0..orientations {
            if stats.trials >= limit {
                break 'plan;
            }
            stats.trials += 1;
            let mut start = incoming.clone();
            start.centre = d.scale(radius);
            if let Some(q) = orientation(o, orientations, d.scale(-1.), axes, knobs.principal) {
                start.rotation = q;
                stats.rotations += 1;
            }
            // Alternate centre-directed growth and existing-member anchors (surface valleys).
            let goal = if k % 2 == 0 || fixed.is_empty() {
                Vec3::new(0., 0., 0.)
            } else {
                fixed[(k / 2) % fixed.len()].centre
            };
            starts.push((start, goal));
        }
    }
    let mut ranked: Vec<((f64, f64), Member)> = vec![(evaluate(&fallback), fallback)];
    ranked.extend(parallel(starts, control, stats, |(start, goal), c, s| {
        let candidate = advance(
            &start,
            goal.sub(start.centre),
            Vec3::new(1., 0., 0.),
            0.,
            &obstacles,
            ctx,
            c,
            s,
        );
        (evaluate(&candidate), candidate)
    }));
    if control.interrupted {
        return Insertion {
            member: incoming.clone(),
            interrupted: true,
        };
    }
    // Stable sort: ties keep plan order, so the choice is thread-count independent.
    ranked.sort_by(|a, b| a.0 .0.total_cmp(&b.0 .0).then(a.0 .1.total_cmp(&b.0 .1)));
    let mut best = ranked[0].clone();
    if knobs.settle_steps > 0 {
        let chosen: Vec<Member> = ranked
            .iter()
            .take(knobs.settle_candidates)
            .map(|(_, m)| m.clone())
            .collect();
        let settled = parallel(chosen, control, stats, |m, c, s| {
            let settled = settle(m, knobs.settle_steps, &obstacles, ctx, c, s);
            (evaluate(&settled), settled)
        });
        if control.interrupted {
            return Insertion {
                member: incoming.clone(),
                interrupted: true,
            };
        }
        for (s, m) in settled {
            if better(s, best.0) {
                best = (s, m);
            }
        }
    }
    Insertion {
        member: best.1,
        interrupted: false,
    }
}

impl Growth {
    // AI-FUNC-SUMMARY: Fix shape/quantile assignments once; keep full deterministic plan for checkpoint restoration.
    pub(super) fn new(
        id: usize,
        spec: &AggregateSpec,
        library: &ShapeLibrary,
        source: &SizeSource,
    ) -> Self {
        let count = spec.particles_per_cluster;
        let mut pending: Vec<Member> = (0..count)
            .map(|i| {
                let diameter = source.quantile(
                    ((i * spec.variants + id) as f64 + 0.5) / (count * spec.variants) as f64,
                );
                let shell = (id * count + i) % library.shells.len();
                let scale = diameter / library.shells[shell].equivalent_diameter;
                Member {
                    shell,
                    diameter,
                    scale,
                    radius: library.shells[shell].bounding_radius * scale,
                    centre: Vec3::new(0., 0., 0.),
                    rotation: UnitQuat::identity(),
                }
            })
            .collect();
        pending.sort_by(|a, b| b.radius.total_cmp(&a.radius).then(a.shell.cmp(&b.shell)));
        let max_r = pending[0].radius;
        let pitch = (2.0 * max_r + spec.internal_gap + 1e-9 * max_r.max(1.0)) / 2.0_f64.sqrt();
        for (m, p) in pending.iter_mut().zip(fcc_sites(count, spec.shape, pitch)) {
            m.centre = p;
        }
        let initial_half = envelope(&pending, spec.shape, library);
        Self {
            pending,
            members: vec![],
            best: vec![],
            initial_half,
            next: 0,
            sweep: 0,
            member: 0,
            relaxing: false,
            stats: SearchStats::default(),
            bodies: Bodies::default(),
        }
    }

    // AI-FUNC-SUMMARY: Commit one insertion or one neighborhood transaction; cancellation rolls back that unit, allowing exact resume without rebuilding completed members.
    pub(super) fn step(
        &mut self,
        spec: &AggregateSpec,
        library: &ShapeLibrary,
        config: &ResolvedPlacement,
        control: &mut PlacementControl,
    ) -> Result<bool> {
        let started = std::time::Instant::now();
        self.bodies.ensure(&self.pending, library)?;
        let bodies = std::mem::take(&mut self.bodies);
        let result = self.step_with(spec, &bodies, config, control);
        self.bodies = bodies;
        self.stats.seconds += started.elapsed().as_secs_f64();
        result
    }

    // AI-FUNC-SUMMARY: step() body with the body cache borrowed separately from the state.
    fn step_with(
        &mut self,
        spec: &AggregateSpec,
        bodies: &Bodies,
        config: &ResolvedPlacement,
        control: &mut PlacementControl,
    ) -> Result<bool> {
        let ctx = Ctx {
            spec,
            bodies,
            config,
        };
        let knobs = knobs(spec);
        if self.next < self.pending.len() && !self.relaxing {
            let mut incoming = self.pending[self.next].clone();
            if self.members.is_empty() {
                incoming.centre = Vec3::new(0., 0., 0.);
            } else {
                let before_stats = self.stats.clone();
                let insertion_limit = self.stats.trials
                    + spec.max_compaction_trials.saturating_sub(self.stats.trials)
                        / (self.pending.len() - self.next);
                let found = insertion_search(
                    &incoming,
                    &self.members,
                    insertion_limit,
                    &ctx,
                    &knobs,
                    control,
                    &mut self.stats,
                );
                if found.interrupted {
                    self.stats = before_stats;
                    return Ok(false);
                }
                incoming = found.member;
            }
            self.members.push(incoming);
            self.next += 1;
            centre_envelope(&mut self.members, spec, bodies);
            self.best = self.members.clone();
            self.relaxing = self.members.len() > 1;
            self.sweep = 0;
            self.member = 0;
            if self.next == 1 || self.next.is_multiple_of(4) || self.next == self.pending.len() {
                let half = bodies.envelope(&self.members, spec.shape);
                eprintln!(
                    "[Aggregate contact] members={}/{} trials={} internal_vf={:.4} seconds={:.1} advances={} steps={}",
                    self.next,
                    self.pending.len(),
                    self.stats.trials,
                    self.internal_vf(library_volume(&self.members, bodies), half, spec),
                    self.stats.seconds,
                    self.stats.advances,
                    self.stats.advance_steps,
                );
            }
            return Ok(false);
        }
        // Reserve future insertion candidates so early relaxation cannot starve later particles.
        let reserve = (self.pending.len() - self.next)
            * spec.contact_directions
            * if spec.rotation_search {
                spec.contact_orientations
            } else {
                1
            };
        let relaxation_limit = spec.max_compaction_trials.saturating_sub(reserve);
        let final_stage = self.next == self.pending.len();
        let rounds = spec.neighborhood_sweeps
            + if final_stage && spec.target_internal_volume_fraction.is_some() {
                spec.strategy_rounds
            } else {
                0
            };
        if self.sweep >= rounds
            || self.stats.trials >= relaxation_limit
            || self.target_reached(spec, bodies)
        {
            if final_stage {
                return Ok(true);
            }
            self.relaxing = false;
            return Ok(false);
        }
        let i = self.member;
        let original = self.members.clone();
        let before_stats = self.stats.clone();
        let mut best_score = score(&self.members, spec, bodies);
        let mut best_members = original.clone();
        let obstacles: Vec<Posed> = self
            .members
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, m)| bodies.pose(m))
            .collect();
        let current = self.members[i].clone();
        // Plan the six transactions with the same sequential budget rule, then run them in
        // parallel: each starts from the unchanged arrangement, so they are independent.
        let mut plan = Vec::new();
        for axis in [
            Vec3::new(1., 0., 0.),
            Vec3::new(0., 1., 0.),
            Vec3::new(0., 0., 1.),
        ] {
            for sign in [-1., 1.] {
                if self.stats.trials >= relaxation_limit {
                    break;
                }
                self.stats.trials += 1;
                let paired = spec.pair_rearrangement
                    && self.members.len() > 1
                    && self.stats.trials < relaxation_limit;
                if paired {
                    self.stats.trials += 1;
                    self.stats.pair_moves += 1;
                }
                let angle = if spec.rotation_search {
                    sign * spec.rotation_step_degrees.to_radians()
                        * 0.5_f64.powi(self.sweep.min(8) as i32)
                } else {
                    0.
                };
                let lateral = if spec.lateral_rearrangement {
                    axis.scale(sign * current.radius * 0.1)
                } else {
                    Vec3::new(0., 0., 0.)
                };
                if angle != 0. {
                    self.stats.rotations += 1;
                }
                if vec_norm(lateral) > 0. {
                    self.stats.lateral_moves += 1;
                }
                plan.push((axis, angle, lateral, paired));
            }
        }
        // Retreat + rotate/slide + inward settling is one transaction. Its intermediate
        // envelope may grow, but every swept pose remains feasible.
        let members = &self.members;
        let outcomes = parallel(plan, control, &mut self.stats, |(axis, angle, lateral, paired), c, s| {
            let norm = vec_norm(current.centre);
            let outward = if norm > 0. {
                current.centre.scale(1. / norm)
            } else {
                axis
            };
            let retreat = advance(
                &current,
                outward.scale(current.radius * 0.1),
                axis,
                0.,
                &obstacles,
                &ctx,
                c,
                s,
            );
            let rotated = advance(&retreat, lateral, axis, angle, &obstacles, &ctx, c, s);
            let candidate = advance(
                &rotated,
                rotated.centre.scale(-1.),
                axis,
                0.,
                &obstacles,
                &ctx,
                c,
                s,
            );
            let mut single = members.clone();
            single[i] = candidate.clone();
            let mut pair = None;
            if paired {
                let j = (0..single.len())
                    .filter(|j| *j != i)
                    .min_by(|&a, &b| {
                        vec_norm(single[a].centre.sub(candidate.centre))
                            .total_cmp(&vec_norm(single[b].centre.sub(candidate.centre)))
                    })
                    .unwrap();
                let others: Vec<Posed> = single
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k != j)
                    .map(|(_, m)| bodies.pose(m))
                    .collect();
                let mut both = single.clone();
                both[j] = advance(
                    &single[j],
                    single[j].centre.scale(-1.),
                    axis,
                    0.,
                    &others,
                    &ctx,
                    c,
                    s,
                );
                pair = Some(both);
            }
            (single, pair)
        });
        if control.interrupted {
            self.members = original;
            self.stats = before_stats;
            return Ok(false);
        }
        for (single, pair) in outcomes {
            for trial in std::iter::once(single).chain(pair) {
                let s = score(&trial, spec, bodies);
                if better(s, best_score) {
                    best_members = trial;
                    best_score = s;
                }
            }
        }
        self.members = best_members;
        if better(score(&self.members, spec, bodies), score(&original, spec, bodies)) {
            self.stats.accepted_moves += 1;
        }
        centre_envelope(&mut self.members, spec, bodies);
        if better(score(&self.members, spec, bodies), score(&self.best, spec, bodies)) {
            self.best = self.members.clone();
        }
        self.member += 1;
        if self.member == self.members.len() {
            self.member = 0;
            self.sweep += 1;
            self.stats.rounds += 1;
            if final_stage {
                let half = bodies.envelope(&self.best, spec.shape);
                eprintln!(
                    "[Aggregate contact] round={} internal_vf={:.4} seconds={:.1} trials={}",
                    self.sweep,
                    self.internal_vf(library_volume(&self.best, bodies), half, spec),
                    self.stats.seconds,
                    self.stats.trials,
                );
            }
        }
        Ok(false)
    }

    // AI-FUNC-SUMMARY: True member volume over container volume for a given envelope; no mutation.
    fn internal_vf(&self, volume: f64, half: f64, spec: &AggregateSpec) -> f64 {
        volume / container_volume(half + 1e-9, spec.shape)
    }

    // AI-FUNC-SUMMARY: Test the best real-member VF against the requested enclosing-container target.
    fn target_reached(&self, spec: &AggregateSpec, bodies: &Bodies) -> bool {
        spec.target_internal_volume_fraction.is_some_and(|target| {
            self.internal_vf(
                library_volume(&self.best, bodies),
                bodies.envelope(&self.best, spec.shape),
                spec,
            ) >= target
        })
    }

    // AI-FUNC-SUMMARY: Export committed members only, independently validate geometry; partial templates never enter global packing.
    pub(super) fn snapshot(
        &self,
        spec: &AggregateSpec,
        library: &ShapeLibrary,
        interrupted: bool,
    ) -> Result<Template> {
        let members = self.best.clone();
        let prepared = members
            .iter()
            .map(|m| prepare_member(m, m.centre, library))
            .collect::<Result<Vec<_>>>()?;
        for (i, a) in prepared.iter().enumerate() {
            for b in &prepared[i + 1..] {
                if !members_clear(a, b, spec.internal_gap) {
                    return Err(RustMsptError::InvalidMesh(
                        "contact growth violated exact internal clearance".into(),
                    ));
                }
            }
        }
        let volume = members
            .iter()
            .map(|m| library.shells[m.shell].volume * m.scale.powi(3))
            .sum();
        let half = envelope(&members, spec.shape, library) + 1e-9;
        let mut stats = self.stats.clone();
        stats.stop_reason = if interrupted {
            "interrupted"
        } else if volume / container_volume(half, spec.shape)
            >= spec.target_internal_volume_fraction.unwrap_or(f64::INFINITY)
        {
            "target_reached"
        } else if stats.trials >= spec.max_compaction_trials {
            "trial_budget_exhausted"
        } else {
            "neighborhood_sweeps_exhausted"
        }
        .into();
        Ok(Template {
            members,
            volume,
            half,
            initial_half: self.initial_half,
            sweeps: 0,
            mesh_sweeps: self.sweep,
            search: stats,
        })
    }
}

// AI-FUNC-SUMMARY: Total true member volume from the library-derived cached bodies' members; no mutation.
fn library_volume(members: &[Member], bodies: &Bodies) -> f64 {
    members.iter().map(|m| bodies.volume(m)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    // AI-FUNC-SUMMARY: Build a real closed mesh and configuration for motion/serialization regression.
    fn setup() -> (tempfile::TempDir, ResolvedPlacement, ShapeLibrary) {
        let d = tempfile::tempdir().unwrap();
        save_stl(
            &d.path().join("shape.stl"),
            &crate::geometry::icosphere_mesh(Vec3::new(0., 0., 0.), 1., 0),
            "sphere",
        )
        .unwrap();
        let path = d.path().join("config.yaml");
        std::fs::write(&path,"placement:\n  seed: 1\n  frame: {unit: um}\n  domain: {min: [-20,-20,-20], max: [20,20,20]}\n  shapes: {files: [shape.stl]}\n  size:\n    distribution: {kind: lognormal, median: 2, sigma_log: 0.1, min: 1.5, max: 2.5}\n  target: {volume_fraction: 0.01}\n  outputs: {dir: out}\n  aggregates: {enabled: true, construction: contact_growth, particles_per_cluster: 4, variants: 1, contact_directions: 4, contact_orientations: 1, neighborhood_sweeps: 1}\n").unwrap();
        let config = match crate::config::load_pack_document(&path).unwrap() {
            crate::config::PackDocument::Placement(p) => p.validate(&path).unwrap(),
            _ => panic!(),
        };
        let library =
            load_shape_library(&config.shape_files, &config.shape_paths_as_written, None).unwrap();
        (d, config, library)
    }
    // AI-FUNC-SUMMARY: Pose-independent body cache plus a context for a given configuration.
    fn bodies_for(members: &[Member], library: &ShapeLibrary) -> Bodies {
        let mut b = Bodies::default();
        b.ensure(members, library).unwrap();
        b
    }
    // AI-FUNC-SUMMARY: Exact world-frame distance between two members, independent of the posed kernel.
    fn world_gap(a: &Member, b: &Member, library: &ShapeLibrary) -> f64 {
        let pa = prepare_member(a, a.centre, library).unwrap();
        let pb = prepare_member(b, b.centre, library).unwrap();
        crate::geometry::mesh_distance_exact_prepared(
            Some(pa.bbox),
            Some(&pa.shape),
            Some(pb.bbox),
            Some(&pb.shape),
        )
    }
    #[test]
    // AI-FUNC-SUMMARY: Pin first-contact stopping and escape from near-contact without tunnelling.
    fn contact_advance_stops_before_obstacle_even_when_endpoint_is_clear() {
        let (_d, config, library) = setup();
        let shell = &library.shells[0];
        let mut m = Member {
            shell: 0,
            diameter: shell.equivalent_diameter,
            scale: 1.,
            radius: shell.bounding_radius,
            centre: Vec3::new(0., 0., 0.),
            rotation: UnitQuat::identity(),
        };
        let fixed = m.clone();
        let bodies = bodies_for(&[m.clone()], &library);
        let ctx = Ctx {
            spec: &config.aggregates,
            bodies: &bodies,
            config: &config,
        };
        let mut stats = SearchStats::default();
        let obstacle = bodies.pose(&fixed);
        m.centre = Vec3::new(-5., 0., 0.);
        let end = advance(
            &m,
            Vec3::new(10., 0., 0.),
            Vec3::new(1., 0., 0.),
            0.,
            &[obstacle.clone()],
            &ctx,
            &mut PlacementControl::new(),
            &mut stats,
        );
        assert!(end.centre.x < -1.5 && end.centre.x > -3.);
        let gap = world_gap(&end, &fixed, &library);
        assert!(gap >= config.aggregates.internal_gap);
        assert!(gap < config.aggregates.internal_gap + 1e-3);
        let retreat = advance(
            &end,
            Vec3::new(-0.2, 0., 0.),
            Vec3::new(1., 0., 0.),
            0.,
            &[obstacle],
            &ctx,
            &mut PlacementControl::new(),
            &mut stats,
        );
        assert!(
            retreat.centre.x < end.centre.x - 0.19,
            "contact tolerance must not block retreat"
        );
    }
    #[test]
    // AI-FUNC-SUMMARY: Posed capped distance equals the world-frame exact distance for rotated members, and never exceeds the cap.
    fn posed_distance_matches_world_frame_distance() {
        let (_d, _config, library) = setup();
        let shell = &library.shells[0];
        let a = Member {
            shell: 0,
            diameter: shell.equivalent_diameter,
            scale: 1.3,
            radius: shell.bounding_radius * 1.3,
            centre: Vec3::new(0.2, -0.1, 0.3),
            rotation: UnitQuat::new(0.9, 0.1, -0.3, 0.2).unwrap(),
        };
        let mut b = a.clone();
        b.scale = 0.7;
        b.radius = shell.bounding_radius * 0.7;
        b.rotation = UnitQuat::new(0.2, 0.7, 0.1, -0.5).unwrap();
        b.centre = Vec3::new(2.9, 0.4, -0.6);
        let bodies = bodies_for(&[a.clone(), b.clone()], &library);
        let exact = world_gap(&a, &b, &library);
        let posed = super::super::bodies::distance_capped(&bodies.pose(&a), &bodies.pose(&b), 100.);
        assert!((exact - posed).abs() < 1e-9, "{exact} vs {posed}");
        let capped =
            super::super::bodies::distance_capped(&bodies.pose(&a), &bodies.pose(&b), exact * 0.5);
        assert!(capped <= exact * 0.5 + 1e-12);
        let hull = bodies.envelope(&[a.clone(), b.clone()], AggregateShape::Sphere);
        let all = envelope(&[a, b], AggregateShape::Sphere, &library);
        assert!((hull - all).abs() < 1e-12, "{hull} vs {all}");
    }
    #[test]
    // AI-FUNC-SUMMARY: Serialize a partially relaxed template and require identical completed transforms and statistics.
    fn contact_growth_serialized_member_boundary_resumes_exactly() {
        let (_d, config, library) = setup();
        let spec = &config.aggregates;
        let source = SizeSource::prepare(&config.distribution).unwrap();
        let mut original = Growth::new(0, spec, &library, &source);
        for _ in 0..6 {
            assert!(!original
                .step(spec, &library, &config, &mut PlacementControl::new())
                .unwrap());
        }
        let bytes = serde_json::to_vec(&original).unwrap();
        let mut restored: Growth = serde_json::from_slice(&bytes).unwrap();
        for g in [&mut original, &mut restored] {
            while !g
                .step(spec, &library, &config, &mut PlacementControl::new())
                .unwrap()
            {}
        }
        let strip = |t: Template| serde_json::to_value(t).unwrap();
        assert_eq!(
            strip(original.snapshot(spec, &library, false).unwrap()),
            strip(restored.snapshot(spec, &library, false).unwrap())
        );
    }
    #[test]
    // AI-FUNC-SUMMARY: A bar's half-turn has clear endpoints but crosses a sphere; swept rotation must stop before crossing.
    fn contact_rotation_cannot_tunnel_with_clear_endpoints() {
        let (_d, config, mut library) = setup();
        let mut m = Member {
            shell: 0,
            diameter: 2.,
            scale: 1.,
            radius: library.shells[0].bounding_radius,
            centre: Vec3::new(0., 2., 0.),
            rotation: UnitQuat::identity(),
        };
        let sphere = m.clone();
        let sphere_bodies = bodies_for(&[sphere.clone()], &library);
        let obstacle = sphere_bodies.pose(&sphere);
        library.shells[0].canonical = std::sync::Arc::new(box_mesh(BoundingBox {
            min: Vec3::new(-2., -0.1, -0.1),
            max: Vec3::new(2., 0.1, 0.1),
        }));
        m.radius = (4.02_f64).sqrt();
        m.centre = Vec3::new(0., 0., 0.);
        // Distinct scale key keeps the bar's body separate from the sphere's in one cache.
        m.scale = 1.0 + 1e-15;
        let mut bodies = sphere_bodies.clone();
        bodies.ensure(&[m.clone()], &library).unwrap();
        let initial = prepare_member(&m, m.centre, &library).unwrap();
        let mut sphere_lib = library.clone();
        sphere_lib.shells[0].canonical = std::sync::Arc::new(crate::geometry::icosphere_mesh(
            Vec3::new(0., 0., 0.),
            1.,
            0,
        ));
        let sphere_prepared = prepare_member(&sphere, sphere.centre, &sphere_lib).unwrap();
        assert!(members_clear(
            &initial,
            &sphere_prepared,
            config.aggregates.internal_gap
        ));
        let ctx = Ctx {
            spec: &config.aggregates,
            bodies: &bodies,
            config: &config,
        };
        let rotated = advance(
            &m,
            Vec3::new(0., 0., 0.),
            Vec3::new(0., 0., 1.),
            std::f64::consts::PI,
            &[obstacle],
            &ctx,
            &mut PlacementControl::new(),
            &mut SearchStats::default(),
        );
        let angle = 2. * rotated.rotation.to_wxyz()[0].acos();
        assert!(
            angle > 0.01 && angle < std::f64::consts::FRAC_PI_2,
            "must stop at first swept contact: {angle}"
        );
    }
}
