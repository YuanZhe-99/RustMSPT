//! Deterministic FCC/contact-settled templates followed by proxy-only global RSA.
use super::*;
use crate::config::placement::{AggregateMode, AggregateShape, AggregateSpec};
use crate::geometry::{bbox_distance, box_mesh, merge_meshes, to_parry_trimesh, vec_norm};
use crate::pipeline::placement_control::PlacementControl;
use crate::pipeline::placement_sizes::{class_for_diameter, SizePlan};
use crate::types::BoundingBox;
use serde_json::{json, Value};

#[derive(Clone)]
struct Member {
    shell: usize,
    diameter: f64,
    scale: f64,
    radius: f64,
    centre: Vec3,
    rotation: UnitQuat,
}
struct Template {
    members: Vec<Member>,
    half: f64,
    volume: f64,
    initial_half: f64,
    mesh_sweeps: usize,
    sweeps: usize,
    search: SearchStats,
}
#[derive(Clone, Copy)]
struct Proxy {
    centre: Vec3,
    radius: f64,
    bbox: BoundingBox,
}

// AI-FUNC-SUMMARY: Return a vector as JSON-friendly XYZ; no mutation.
fn xyz(p: Vec3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

// AI-FUNC-SUMMARY: Radius/half-side of an origin-centred container enclosing all true member vertices; no mutation.
fn envelope(members: &[Member], shape: AggregateShape, library: &ShapeLibrary) -> f64 {
    members
        .iter()
        .flat_map(|m| {
            library.shells[m.shell]
                .canonical
                .vertices
                .iter()
                .map(move |v| m.rotation.rotate_point(v.scale(m.scale)).add(m.centre))
        })
        .map(|v| match shape {
            AggregateShape::Sphere => vec_norm(v),
            AggregateShape::Cube => v.x.abs().max(v.y.abs()).max(v.z.abs()),
        })
        .fold(0.0, f64::max)
}

// AI-FUNC-SUMMARY: Generate count FCC sites in deterministic container-distance/lexicographic order; no RNG. Nearest-neighbour distance is sqrt(2)*pitch.
fn fcc_sites(count: usize, shape: AggregateShape, pitch: f64) -> Vec<Vec3> {
    let m = (count as f64).cbrt().ceil() as i32 + 1;
    let mut points = Vec::new();
    for x in -m..=m {
        for y in -m..=m {
            for z in -m..=m {
                if (x + y + z).rem_euclid(2) == 0 {
                    points.push((x, y, z));
                }
            }
        }
    }
    points.sort_by_key(|&(x, y, z)| {
        let norm = x * x + y * y + z * z;
        let key = if shape == AggregateShape::Sphere {
            norm
        } else {
            x.abs().max(y.abs()).max(z.abs())
        };
        (key, norm, x, y, z)
    });
    points
        .into_iter()
        .take(count)
        .map(|(x, y, z)| Vec3::new(x as f64 * pitch, y as f64 * pitch, z as f64 * pitch))
        .collect()
}

// AI-FUNC-SUMMARY: Move one bounding ball toward goal up to its first swept contact with another; returns travel, preserves non-overlap and avoids tunnelling; mutates only its centre.
fn settle_one(members: &mut [Member], i: usize, goal: Vec3, gap: f64) -> f64 {
    let delta = goal.sub(members[i].centre);
    let length = vec_norm(delta);
    if length == 0.0 {
        return 0.0;
    }
    let direction = delta.scale(1.0 / length);
    let mut travel = length;
    for (j, other) in members.iter().enumerate() {
        if i == j {
            continue;
        }
        let d = members[i].centre.sub(other.centre);
        let b = d.dot(direction);
        if b >= 0.0 {
            continue;
        }
        let radius = members[i].radius + other.radius + gap;
        let c = d.dot(d) - radius * radius;
        let disc = b * b - c;
        if disc >= 0.0 {
            // First root, using the cancellation-resistant form c/(-b+sqrt(disc)).
            let hit = c.max(0.0) / (-b + disc.sqrt());
            travel = travel.min(hit);
        }
    }
    travel *= 1.0 - 1e-10; // Stay on the separated side of round-off at contact.
    members[i].centre = members[i].centre.add(direction.scale(travel));
    travel
}

// AI-FUNC-SUMMARY: Construct one template with stratified PSD quantiles and cyclic shapes on FCC, then deterministic contact sweeps; no random placements; cancellation leaves valid balls.
fn build_template(
    id: usize,
    spec: &AggregateSpec,
    library: &ShapeLibrary,
    source: &SizeSource,
    config: &ResolvedPlacement,
    control: &mut PlacementControl,
) -> Result<Template> {
    let count = spec.particles_per_cluster;
    let mut members: Vec<Member> = (0..count)
        .map(|i| {
            let q = (i * spec.variants + id) as f64 + 0.5;
            let diameter = source.quantile(q / (count * spec.variants) as f64);
            let shell = (id * count + i) % library.shells.len();
            let scale = diameter / library.shells[shell].equivalent_diameter;
            Member {
                shell,
                diameter,
                scale,
                radius: library.shells[shell].bounding_radius * scale,
                centre: Vec3::new(0.0, 0.0, 0.0),
                rotation: UnitQuat::identity(),
            }
        })
        .collect();
    members.sort_by(|a, b| b.radius.total_cmp(&a.radius).then(a.shell.cmp(&b.shell)));
    let max_r = members.iter().map(|m| m.radius).fold(0.0, f64::max);
    let safe_gap = spec.internal_gap + 1e-9 * max_r.max(1.0);
    let pitch = (2.0 * max_r + safe_gap) / 2.0_f64.sqrt();
    for (member, site) in members.iter_mut().zip(fcc_sites(count, spec.shape, pitch)) {
        member.centre = site;
    }
    let initial_half = envelope(&members, spec.shape, library);
    let initial_members = members.clone();
    let mut sweeps = 0;
    for _ in 0..spec.compaction_sweeps {
        let mut movement = 0.0_f64;
        for i in 0..count {
            if control.poll(config, false, 0, 0, 0.0) {
                break;
            }
            movement = movement.max(settle_one(
                &mut members,
                i,
                Vec3::new(0.0, 0.0, 0.0),
                safe_gap,
            ));
            for axis in 0..3 {
                let c = members[i].centre;
                let goal = match axis {
                    0 => Vec3::new(0.0, c.y, c.z),
                    1 => Vec3::new(c.x, 0.0, c.z),
                    _ => Vec3::new(c.x, c.y, 0.0),
                };
                movement = movement.max(settle_one(&mut members, i, goal, safe_gap));
            }
        }
        sweeps += 1;
        if control.interrupted || movement < 1e-8 * max_r.max(1.0) {
            break;
        }
    }
    // Independent full pair validation before a template is allowed into global Pack.
    for (i, a) in members.iter().enumerate() {
        for b in &members[i + 1..] {
            if vec_norm(a.centre.sub(b.centre)) < a.radius + b.radius + spec.internal_gap {
                return Err(RustMsptError::InvalidMesh(
                    "aggregate contact settlement violated member-ball gap".into(),
                ));
            }
        }
    }
    if envelope(&members, spec.shape, library) > initial_half {
        members = initial_members;
    }
    let mesh_sweeps = refine_meshes(&mut members, spec, library, config, control)?;
    let volume: f64 = members
        .iter()
        .map(|m| library.shells[m.shell].volume * m.scale.powi(3))
        .sum();
    let search = compact_to_target(&mut members, volume, spec, library, config, control)?;
    let half = envelope(&members, spec.shape, library) + 1e-9 * max_r.max(1.0);
    Ok(Template {
        members,
        half,
        volume,
        initial_half,
        mesh_sweeps,
        sweeps,
        search,
    })
}

struct PreparedMember {
    mesh: Mesh,
    bbox: BoundingBox,
    shape: parry3d_f64::shape::TriMesh,
}

// AI-FUNC-SUMMARY: Prepare one real constituent at a deterministic trial centre; returns geometry and cached query shape, refuses unusable meshes.
fn prepare_member(member: &Member, centre: Vec3, library: &ShapeLibrary) -> Result<PreparedMember> {
    let mesh = transform_shell(
        &library.shells[member.shell].canonical,
        member.scale,
        member.rotation,
        centre,
    );
    let bbox = mesh_bbox(&mesh)
        .ok_or_else(|| RustMsptError::InvalidMesh("empty aggregate member".into()))?;
    let shape = to_parry_trimesh(&mesh)
        .ok_or_else(|| RustMsptError::InvalidMesh("cannot prepare aggregate member".into()))?;
    Ok(PreparedMember { mesh, bbox, shape })
}

// AI-FUNC-SUMMARY: Exact solid intersection/nesting and surface-gap predicate with AABB rejection; no mutation.
fn members_clear(a: &PreparedMember, b: &PreparedMember, gap: f64) -> bool {
    if bbox_distance(a.bbox, b.bbox) > gap {
        return true;
    }
    !crate::geometry::mesh_collision_exact_prepared(
        Some(a.bbox),
        Some(&a.shape),
        Some(b.bbox),
        Some(&b.shape),
    ) && !crate::geometry::mesh_closer_than_prepared(
        Some(a.bbox),
        Some(&a.shape),
        Some(b.bbox),
        Some(&b.shape),
        gap,
        true,
    )
}

// AI-FUNC-SUMMARY: Refine FCC/contact templates by deterministic collision-constrained coordinate descent on real meshes; fixed backtracking directions, no random samples. Preserves every member, gap and container bound; stops safely on cancellation.
fn refine_meshes(
    members: &mut [Member],
    spec: &AggregateSpec,
    library: &ShapeLibrary,
    config: &ResolvedPlacement,
    control: &mut PlacementControl,
) -> Result<usize> {
    let mut prepared: Vec<PreparedMember> = members
        .iter()
        .map(|m| prepare_member(m, m.centre, library))
        .collect::<Result<_>>()?;
    let scale = members.iter().map(|m| m.radius).fold(0.0, f64::max);
    let mut completed = 0;
    let mut bound = envelope(members, spec.shape, library);
    'sweeps: for _ in 0..spec.mesh_refinement_sweeps {
        let mut movement = 0.0_f64;
        for i in 0..members.len() {
            for axis in 0..4 {
                let c = members[i].centre;
                let goal = match axis {
                    0 => Vec3::new(0.0, 0.0, 0.0),
                    1 => Vec3::new(0.0, c.y, c.z),
                    2 => Vec3::new(c.x, 0.0, c.z),
                    _ => Vec3::new(c.x, c.y, 0.0),
                };
                let delta = goal.sub(c);
                if vec_norm(delta) < 1e-7 * scale {
                    continue;
                }
                // This is static feasible coordinate descent, not a time-resolved physical trajectory.
                for backtrack in 0..12 {
                    if control.poll(config, false, 0, 0, 0.0) {
                        break 'sweeps;
                    }
                    let step = delta.scale(0.5_f64.powi(backtrack));
                    let trial_centre = c.add(step);
                    let trial = prepare_member(&members[i], trial_centre, library)?;
                    let inside = trial.mesh.vertices.iter().all(|p| match spec.shape {
                        AggregateShape::Sphere => vec_norm(*p) <= bound,
                        AggregateShape::Cube => p.x.abs().max(p.y.abs()).max(p.z.abs()) <= bound,
                    });
                    if inside
                        && prepared.iter().enumerate().all(|(j, other)| {
                            i == j || members_clear(&trial, other, spec.internal_gap + 1e-9 * scale)
                        })
                    {
                        members[i].centre = trial_centre;
                        prepared[i] = trial;
                        movement = movement.max(vec_norm(step));
                        break;
                    }
                }
            }
        }
        completed += 1;
        bound = envelope(members, spec.shape, library);
        if movement < 1e-6 * scale {
            break;
        }
    }
    // Independent all-pairs check after refinement, including the zero-sweep and interrupted paths.
    for (i, a) in prepared.iter().enumerate() {
        for b in &prepared[i + 1..] {
            if !members_clear(a, b, spec.internal_gap) {
                return Err(RustMsptError::InvalidMesh(
                    "aggregate mesh refinement violated member gap".into(),
                ));
            }
        }
    }
    Ok(completed)
}

#[derive(Default, serde::Serialize)]
struct SearchStats {
    rounds: usize,
    trials: usize,
    rotations: usize,
    lateral_moves: usize,
    pair_moves: usize,
    shrink_steps: usize,
    accepted_moves: usize,
    stop_reason: String,
}

// AI-FUNC-SUMMARY: Compose global and local unit rotations in application order; returns canonical normalized quaternion, no mutation.
fn compose_rotation(a: UnitQuat, b: UnitQuat) -> UnitQuat {
    UnitQuat::new(
        a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
        a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
        a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
    )
    .expect("finite unit rotations")
}

// AI-FUNC-SUMMARY: True sphere/cube container volume from radius/half-side; returns f64, no mutation.
fn container_volume(half: f64, shape: AggregateShape) -> f64 {
    match shape {
        AggregateShape::Sphere => 4.0 / 3.0 * std::f64::consts::PI * half.powi(3),
        AggregateShape::Cube => (2.0 * half).powi(3),
    }
}

// AI-FUNC-SUMMARY: Score a constituent's outer support and centre against a progressively shrinking container; returns finite cost, no mutation.
fn compression_cost(p: &PreparedMember, centre: Vec3, goal: f64, shape: AggregateShape) -> f64 {
    let extent = p
        .mesh
        .vertices
        .iter()
        .map(|v| match shape {
            AggregateShape::Sphere => vec_norm(*v),
            AggregateShape::Cube => v.x.abs().max(v.y.abs()).max(v.z.abs()),
        })
        .fold(0.0, f64::max);
    20.0 * (extent - goal).max(0.0).powi(2) + extent.powi(2) + 0.01 * centre.dot(centre)
}

// AI-FUNC-SUMMARY: Recenter a feasible assembly by its vertex AABB midpoint if that reduces its enclosing size; rigid translation preserves pair gaps.
fn recenter(members: &mut [Member], shape: AggregateShape, library: &ShapeLibrary) {
    let mut min = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut max = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for m in members.iter() {
        for v in &library.shells[m.shell].canonical.vertices {
            let p = m.rotation.rotate_point(v.scale(m.scale)).add(m.centre);
            min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
            max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
        }
    }
    let shift = min.add(max).scale(0.5);
    let mut trial = members.to_vec();
    for m in &mut trial {
        m.centre = m.centre.sub(shift);
    }
    if envelope(&trial, shape, library) < envelope(members, shape, library) {
        members.clone_from_slice(&trial);
    }
}

// AI-FUNC-SUMMARY: Target-driven deterministic rotation, lateral/pair rearrangement and shrinking-container search; bounds proposals, preserves best feasible assembly and validates all pairs after cancellation; no RNG or resizing.
fn compact_to_target(
    members: &mut [Member],
    volume: f64,
    spec: &AggregateSpec,
    library: &ShapeLibrary,
    config: &ResolvedPlacement,
    control: &mut PlacementControl,
) -> Result<SearchStats> {
    let mut stats = SearchStats::default();
    let Some(target) = spec.target_internal_volume_fraction else {
        stats.stop_reason = "target_not_configured".into();
        return Ok(stats);
    };
    let desired = (volume / container_volume(1.0, spec.shape) / target).cbrt();
    let mut best = members.to_vec();
    let mut best_half = envelope(members, spec.shape, library);
    let scale = members.iter().map(|m| m.radius).fold(0.0, f64::max);
    let gap = spec.internal_gap + 1e-9 * scale.max(1.0);
    let mut goal = best_half;
    let axes = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    'rounds: for round in 0..spec.strategy_rounds {
        if volume / container_volume(best_half + 1e-9 * scale.max(1.0), spec.shape) >= target {
            break;
        }
        goal = (goal * (1.0 - spec.container_shrink_fraction)).max(desired);
        stats.shrink_steps += 1;
        recenter(members, spec.shape, library);
        let mut prepared = members
            .iter()
            .map(|m| prepare_member(m, m.centre, library))
            .collect::<Result<Vec<_>>>()?;
        let step = scale * 0.25 * 0.5_f64.powi((round / 3).min(8) as i32);
        let angle =
            spec.rotation_step_degrees.to_radians() * 0.5_f64.powi((round / 3).min(5) as i32);
        for index in 0..members.len() {
            let i = if round % 2 == 0 {
                index
            } else {
                members.len() - 1 - index
            };
            // Alternate order to allow neighbours to move out of an earlier member's way.
            for strategy in 0..3 {
                if strategy == 1 && !spec.rotation_search {
                    continue;
                }
                if strategy == 2 && !spec.lateral_rearrangement {
                    continue;
                }
                let current = members[i].clone();
                let inward = current.centre.scale(-spec.container_shrink_fraction);
                let mut proposals = Vec::new();
                if strategy == 0 {
                    for fraction in [1.0, 0.5, 0.25, 0.125] {
                        let mut trial = current.clone();
                        trial.centre = trial.centre.add(inward.scale(fraction));
                        proposals.push(trial);
                    }
                } else {
                    for axis in axes {
                        for sign in [-1.0, 1.0] {
                            for coupled in [false, true] {
                                let mut trial = current.clone();
                                if strategy == 1 {
                                    let a = sign * angle * 0.5;
                                    let q = UnitQuat::new(
                                        a.cos(),
                                        axis.x * a.sin(),
                                        axis.y * a.sin(),
                                        axis.z * a.sin(),
                                    )
                                    .unwrap();
                                    trial.rotation = compose_rotation(q, trial.rotation);
                                } else {
                                    trial.centre = trial.centre.add(axis.scale(sign * step));
                                }
                                if coupled {
                                    trial.centre = trial.centre.add(inward);
                                }
                                proposals.push(trial);
                            }
                        }
                    }
                }
                let mut cost = compression_cost(&prepared[i], current.centre, goal, spec.shape);
                let mut choice = None;
                for candidate in proposals {
                    if control.poll(config, false, 0, 0, 0.0)
                        || stats.trials >= spec.max_compaction_trials
                    {
                        break 'rounds;
                    }
                    stats.trials += 1;
                    if strategy == 1 {
                        stats.rotations += 1;
                    }
                    if strategy == 2 {
                        stats.lateral_moves += 1;
                    }
                    let trial = prepare_member(&candidate, candidate.centre, library)?;
                    let next = compression_cost(&trial, candidate.centre, goal, spec.shape);
                    if next < cost - 1e-10 * scale * scale
                        && prepared
                            .iter()
                            .enumerate()
                            .all(|(j, p)| i == j || members_clear(&trial, p, gap))
                    {
                        cost = next;
                        choice = Some((candidate, trial));
                    }
                }
                if let Some((m, p)) = choice {
                    members[i] = m;
                    prepared[i] = p;
                    stats.accepted_moves += 1;
                }
            }
        }
        if spec.pair_rearrangement && members.len() > 1 {
            for i in 0..members.len() {
                let j = (0..members.len())
                    .filter(|&j| j != i)
                    .min_by(|&a, &b| {
                        vec_norm(members[i].centre.sub(members[a].centre))
                            .total_cmp(&vec_norm(members[i].centre.sub(members[b].centre)))
                    })
                    .unwrap();
                let delta = members[i]
                    .centre
                    .add(members[j].centre)
                    .scale(-0.5 * spec.container_shrink_fraction);
                for fraction in [1.0, 0.5, 0.25] {
                    if control.poll(config, false, 0, 0, 0.0)
                        || stats.trials >= spec.max_compaction_trials
                    {
                        break 'rounds;
                    }
                    stats.trials += 1;
                    stats.pair_moves += 1;
                    let ci = members[i].centre.add(delta.scale(fraction));
                    let cj = members[j].centre.add(delta.scale(fraction));
                    let a = prepare_member(&members[i], ci, library)?;
                    let b = prepare_member(&members[j], cj, library)?;
                    let old = compression_cost(&prepared[i], members[i].centre, goal, spec.shape)
                        + compression_cost(&prepared[j], members[j].centre, goal, spec.shape);
                    let new = compression_cost(&a, ci, goal, spec.shape)
                        + compression_cost(&b, cj, goal, spec.shape);
                    if new < old - 1e-10 * scale * scale
                        && members_clear(&a, &b, gap)
                        && prepared.iter().enumerate().all(|(k, p)| {
                            k == i
                                || k == j
                                || (members_clear(&a, p, gap) && members_clear(&b, p, gap))
                        })
                    {
                        members[i].centre = ci;
                        members[j].centre = cj;
                        prepared[i] = a;
                        prepared[j] = b;
                        stats.accepted_moves += 1;
                        break;
                    }
                }
            }
        }
        recenter(members, spec.shape, library);
        let half = envelope(members, spec.shape, library);
        if half < best_half {
            best_half = half;
            best = members.to_vec();
        }
        stats.rounds += 1;
        eprintln!(
            "[Aggregate compaction] round={} trials={} best_internal_vf={:.6} target={target}",
            stats.rounds,
            stats.trials,
            volume / container_volume(best_half, spec.shape)
        );
    }
    // Include a partial round's valid improvements before restoring the best geometry.
    recenter(members, spec.shape, library);
    let half = envelope(members, spec.shape, library);
    if half < best_half {
        best_half = half;
        best = members.to_vec();
    }
    members.clone_from_slice(&best);
    let prepared = members
        .iter()
        .map(|m| prepare_member(m, m.centre, library))
        .collect::<Result<Vec<_>>>()?;
    for (i, a) in prepared.iter().enumerate() {
        for b in &prepared[i + 1..] {
            if !members_clear(a, b, spec.internal_gap) {
                return Err(RustMsptError::InvalidMesh(
                    "target compaction violated member gap".into(),
                ));
            }
        }
    }
    stats.stop_reason = if control.interrupted {
        "interrupted"
    } else if volume / container_volume(best_half + 1e-9 * scale.max(1.0), spec.shape) >= target {
        "target_reached"
    } else if stats.trials >= spec.max_compaction_trials {
        "trial_budget_exhausted"
    } else {
        "strategy_rounds_exhausted"
    }
    .into();
    Ok(stats)
}

// AI-FUNC-SUMMARY: Conservative world proxy for a rotated template; sphere is exact, rotated cube uses its enclosing world AABB; no geometry expansion.
fn world_proxy(t: &Template, shape: AggregateShape, rotation: UnitQuat, centre: Vec3) -> Proxy {
    let half = match shape {
        AggregateShape::Sphere => Vec3::new(t.half, t.half, t.half),
        AggregateShape::Cube => {
            let r = rotation.to_matrix();
            Vec3::new(
                r[0].iter().map(|v| v.abs()).sum::<f64>() * t.half,
                r[1].iter().map(|v| v.abs()).sum::<f64>() * t.half,
                r[2].iter().map(|v| v.abs()).sum::<f64>() * t.half,
            )
        }
    };
    Proxy {
        centre,
        radius: t.half,
        bbox: BoundingBox {
            min: centre.sub(half),
            max: centre.add(half),
        },
    }
}

// AI-FUNC-SUMMARY: Test container-wall, container-container and container-void separation without member mesh expansion; centre-in-void is unconditional to reject enclosed clusters.
fn proxy_check(
    p: Proxy,
    shape: AggregateShape,
    config: &ResolvedPlacement,
    void: Option<&VoidIndex>,
    placed: &[Proxy],
    neighbours: &[usize],
) -> std::result::Result<(), RejectReason> {
    let domain = config.domain.expanded(-config.boundary.min_boundary_dist);
    if !domain.contains_point(p.bbox.min) || !domain.contains_point(p.bbox.max) {
        return Err(RejectReason::OutsideDomain);
    }
    let slack = 1e-9 * p.radius.max(1.0);
    for &id in neighbours {
        let other = placed[id];
        let distance = if shape == AggregateShape::Sphere {
            vec_norm(p.centre.sub(other.centre)) - p.radius - other.radius
        } else {
            bbox_distance(p.bbox, other.bbox)
        };
        if distance < config.gap_particle_particle + slack {
            return Err(RejectReason::ParticleGap);
        }
    }
    if let Some(v) = void {
        if v.contains_point(p.centre) {
            return Err(RejectReason::InsideVoid);
        }
        let gap = config.void.as_ref().map(|v| v.gap).unwrap_or(0.0) + slack;
        match shape {
            AggregateShape::Sphere => {
                if v.surface_distance(p.centre) < p.radius + gap {
                    return Err(RejectReason::VoidGap);
                }
            }
            AggregateShape::Cube => {
                let mesh = box_mesh(p.bbox);
                let prepared = to_parry_trimesh(&mesh).expect("nondegenerate validated cube");
                if v.any_vertex_inside(&mesh)
                    || v.intersects(&prepared)
                    || v.any_void_vertex_inside(&mesh, p.bbox)
                {
                    return Err(RejectReason::InsideVoid);
                }
                if v.min_distance_to(&prepared) < gap {
                    return Err(RejectReason::VoidGap);
                }
            }
        }
    }
    Ok(())
}

// AI-FUNC-SUMMARY: Export one template's actual particles and reconstruction/proxy statistics; writes STL, returns JSON metadata, never counts proxy volume as material.
fn export_template(
    id: usize,
    t: &Template,
    library: &ShapeLibrary,
    config: &ResolvedPlacement,
) -> Result<Value> {
    let meshes: Vec<Mesh> = t
        .members
        .iter()
        .map(|m| {
            transform_shell(
                &library.shells[m.shell].canonical,
                m.scale,
                m.rotation,
                m.centre,
            )
        })
        .collect();
    let path = config
        .outputs
        .dir
        .join("aggregate_templates")
        .join(format!("template_{id:04}.stl"));
    std::fs::create_dir_all(path.parent().unwrap())?;
    save_stl(&path, &merge_meshes(&meshes), "aggregate particles")?;
    let proxy_volume = match config.aggregates.shape {
        AggregateShape::Sphere => 4.0 / 3.0 * std::f64::consts::PI * t.half.powi(3),
        AggregateShape::Cube => (2.0 * t.half).powi(3),
    };
    let initial_volume = proxy_volume * (t.initial_half / t.half).powi(3);
    let members: Vec<Value> = t.members.iter().map(|m| {
        let s = &library.shells[m.shell];
        json!({"source_index":s.source_index,"shell_index":s.shell_index,"shell_sha256":s.shell_sha256,
               "source_path":library.sources[s.source_index].path.to_string_lossy(),"source_sha256":library.sources[s.source_index].sha256,"shell_centroid":xyz(s.centroid),
               "scale":m.scale,"diameter":m.diameter,"centre":xyz(m.centre),"bounding_radius":m.radius,"rotation":m.rotation.to_wxyz()})
    }).collect();
    Ok(
        json!({"template_id":id,"shape":config.aggregates.shape,"half_extent_or_radius":t.half,
        "material_volume":t.volume,"proxy_volume":proxy_volume,"internal_volume_fraction":t.volume/proxy_volume,
        "initial_internal_volume_fraction":t.volume/initial_volume,"compaction_sweeps":t.sweeps,"mesh_refinement_sweeps":t.mesh_sweeps,
        "target_internal_volume_fraction":config.aggregates.target_internal_volume_fraction,
        "target_reached":config.aggregates.target_internal_volume_fraction.map(|v| t.volume/proxy_volume >= v),
        "compaction_search":t.search,
        "members":members,"stl":path.to_string_lossy()}),
    )
}

// AI-FUNC-SUMMARY: Plan repeated template IDs by nearest cumulative true member volume; never rescale member diameters to fit a target; bounded by the common member-count cap.
fn plan_templates(
    templates: &[Template],
    target: f64,
    classes: &[SizeClass],
) -> Result<(Vec<usize>, SizePlan)> {
    let mut ids = Vec::new();
    let mut volume = 0.0;
    while volume < target {
        let id = ids.len() % templates.len();
        let next = volume + templates[id].volume;
        if next >= target && !ids.is_empty() && (next - target).abs() >= (volume - target).abs() {
            break;
        }
        if (ids.len() + 1) * templates[id].members.len() > MAX_PLANNED_PARTICLES {
            return Err(RustMsptError::InvalidConfig(
                "aggregate plan exceeds member count cap".into(),
            ));
        }
        ids.push(id);
        volume = next;
    }
    let mut draws = Vec::new();
    for &id in &ids {
        for m in &templates[id].members {
            draws.push(SizeDraw {
                diameter: m.diameter,
                class: class_for_diameter(classes, m.diameter),
                draw_index: draws.len(),
            });
        }
    }
    Ok((
        ids,
        SizePlan {
            draws,
            planned_volume: volume,
            target_volume: target,
            planned_volume_error: volume - target,
        },
    ))
}

// AI-FUNC-SUMMARY: Two-stage aggregate pack; deterministic templates, seeded global proxy RSA, flat per-member output and cluster membership sidecars; cooperatively saves accepted clusters on interruption.
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
    let mut resolved_spec = config.aggregates.clone();
    if resolved_spec.variants == 0 {
        resolved_spec.variants = library
            .shells
            .len()
            .div_ceil(resolved_spec.particles_per_cluster)
            .clamp(4, 32);
    }
    let spec = &resolved_spec;
    let mut counts = vec![spec.particles_per_cluster];
    if spec.mode == AggregateMode::Mixed {
        counts.extend_from_slice(&spec.fallback_particles_per_cluster);
    }
    let mut control = PlacementControl::new();
    control.phase = "generating_templates";
    control.poll(config, true, 0, 0, 0.0);
    let mut templates = Vec::new();
    let mut catalog = Vec::new();
    let mut stage_ranges = Vec::new();
    'generation: for (stage, &count) in counts.iter().enumerate() {
        let start = templates.len();
        let mut stage_spec = spec.clone();
        stage_spec.particles_per_cluster = count;
        for variant in 0..spec.variants {
            if control.poll(config, true, 0, 0, 0.0) {
                break 'generation;
            }
            let t = build_template(variant, &stage_spec, library, source, config, &mut control)?;
            let id = templates.len();
            let mut entry = export_template(id, &t, library, config)?;
            entry["stage"] = json!(stage);
            entry["variant"] = json!(variant);
            eprintln!(
                "[Aggregate] template={id} stage={stage} members={count} internal_vf={}",
                entry["internal_volume_fraction"]
            );
            catalog.push(entry);
            templates.push(t);
        }
        stage_ranges.push(start..templates.len());
    }
    let template_path = config.outputs.dir.join("aggregate_templates.json");
    write_json(
        &template_path,
        &json!({"algorithm":"fcc_deterministic_target_compaction_v2","config":spec,"requested_variants":config.aggregates.variants,"resolved_variants":spec.variants,"templates":catalog}),
    )?;
    let target = basis * config.target_volume_fraction;
    let (ids, plan) = if control.interrupted {
        (
            Vec::new(),
            SizePlan {
                draws: Vec::new(),
                planned_volume: 0.0,
                target_volume: target,
                planned_volume_error: -target,
            },
        )
    } else {
        plan_templates(&templates[stage_ranges[0].clone()], target, classes)?
    };
    let frame = FrameRecord {
        unit: config.unit.clone(),
        origin: [0.0; 3],
        axis_order: "xyz".into(),
        handedness: "right".into(),
        domain: DomainRecord {
            min: xyz(config.domain.min),
            max: xyz(config.domain.max),
        },
    };
    let threads = rayon::current_num_threads();
    let mut report = blank_report(
        config,
        tool,
        &frame,
        library,
        &plan,
        basis,
        threads,
        build_void_report(config, void, void_volume, void_method)?,
    );
    report
        .samplers
        .insert("position".into(), "aggregate_proxy_rejection_rsa".into());
    report.samplers.insert(
        "sizes".into(),
        "deterministic_stratified_quantiles_repeated_templates".into(),
    );
    report
        .samplers
        .insert("attempt_unit".into(), "cluster_proposal".into());
    write_json(&config.outputs.report, &report)?;
    let mut state = EngineState::new(config, library, classes, &plan.draws);
    state.basis_volume = basis;
    state.control = control;
    state.control.phase = "packing";
    let max_half = templates.iter().map(|t| t.half).fold(1.0, f64::max);
    let mut grid = SpatialGrid::new(
        config.domain,
        2.0 * 3.0_f64.sqrt() * max_half + config.gap_particle_particle,
    );
    let mut proxies = Vec::new();
    let mut clusters = Vec::new();
    let mut rng = seeded_rng(config.seed);
    let mut failed_clusters = 0;
    let mut processed_clusters = 0;
    state.poll_control(config, true);
    let mut planned_clusters = ids.len();
    let mut stages_report = Vec::new();
    for (stage, range) in stage_ranges.iter().enumerate() {
        if state.control.interrupted || state.budget_spent(config) {
            break;
        }
        if stage > 0 && state.volume_solid >= target * (1.0 - config.target_tolerance) {
            break;
        }
        let stage_ids = if stage == 0 {
            ids.clone()
        } else {
            let (local, extra) = plan_templates(
                &templates[range.clone()],
                (target - state.volume_solid).max(0.0),
                classes,
            )?;
            if report.plan.planned_particles + extra.draws.len() > MAX_PLANNED_PARTICLES {
                return Err(RustMsptError::InvalidConfig(
                    "mixed aggregate plan exceeds total member cap".into(),
                ));
            }
            for d in &extra.draws {
                state.drawn_per_class[d.class] += 1;
            }
            report.plan.planned_particles += extra.draws.len();
            report.plan.planned_volume += extra.planned_volume;
            report.plan.planned_volume_error = report.plan.planned_volume - target;
            planned_clusters += local.len();
            local.into_iter().map(|id| id + range.start).collect()
        };
        let before = clusters.len();
        let before_attempts = state.attempts;
        let remaining_stages = stage_ranges.len() - stage;
        // Reserve a share of finite global proposal budget for every smaller fallback stage.
        let stage_limit =
            state.attempts + (config.budget.total_attempts - state.attempts) / remaining_stages;
        'clusters: for &id in &stage_ids {
            if state.poll_control(config, false)
                || state.budget_spent(config)
                || state.attempts >= stage_limit
            {
                state.stopped_early = true;
                break;
            }
            let t = &templates[id];
            let mut placed = false;
            for _ in 0..config.budget.attempts_per_particle {
                if state.poll_control(config, false)
                    || state.budget_spent(config)
                    || state.attempts >= stage_limit
                {
                    state.stopped_early = true;
                    break 'clusters;
                }
                let rotation = match config.orientation {
                    OrientationMode::UniformSo3 => sample_uniform_quaternion(&mut rng),
                    OrientationMode::Fixed => {
                        let _ = (u01(&mut rng), u01(&mut rng), u01(&mut rng));
                        UnitQuat::identity()
                    }
                };
                // Orientation-independent erosion; cubes use their circumsphere to avoid position/orientation bias.
                let reach = t.half
                    * if spec.shape == AggregateShape::Cube {
                        3.0_f64.sqrt()
                    } else {
                        1.0
                    };
                let domain = config
                    .domain
                    .expanded(-(reach + config.boundary.min_boundary_dist));
                let centre = Vec3::new(
                    uniform_range(&mut rng, domain.min.x, domain.max.x),
                    uniform_range(&mut rng, domain.min.y, domain.max.y),
                    uniform_range(&mut rng, domain.min.z, domain.max.z),
                );
                state.attempts += 1;
                let candidate = world_proxy(t, spec.shape, rotation, centre);
                let neighbours = grid.query_neighbors_with_margin(
                    candidate.bbox,
                    config.gap_particle_particle,
                    usize::MAX,
                );
                let valid = if domain.min.x > domain.max.x
                    || domain.min.y > domain.max.y
                    || domain.min.z > domain.max.z
                {
                    Err(RejectReason::OutsideDomain)
                } else {
                    proxy_check(candidate, spec.shape, config, void, &proxies, &neighbours)
                };
                if let Err(reason) = valid {
                    state.reject(reason);
                    continue;
                }
                let first = state.placed.len();
                // Commit a whole cluster without checking cancellation between its members.
                for m in &t.members {
                    let shell = &library.shells[m.shell];
                    let translation = centre.add(rotation.rotate_point(m.centre));
                    let member_rotation = compose_rotation(rotation, m.rotation);
                    let mesh =
                        transform_shell(&shell.canonical, m.scale, member_rotation, translation);
                    let bbox = mesh_bbox(&mesh).expect("validated nonempty source");
                    let volume = shell.volume * m.scale.powi(3);
                    let draw = SizeDraw {
                        diameter: m.diameter,
                        class: class_for_diameter(classes, m.diameter),
                        draw_index: state.placed.len(),
                    };
                    accept(
                        &mut state,
                        m.shell,
                        library,
                        &draw,
                        m.scale,
                        member_rotation,
                        translation,
                        m.radius,
                        volume,
                        bbox,
                        0.0,
                        mesh,
                        crate::pipeline::placement_feasibility::Accepted {
                            volume_in_domain: volume,
                            clipped_faces: Vec::new(),
                            shape: None,
                        },
                    );
                    state.placed_per_class[draw.class] += 1;
                }
                grid.insert(proxies.len(), candidate.bbox);
                proxies.push(candidate);
                clusters.push(json!({"cluster_id":clusters.len(),"stage":stage,"template_id":id,"first_particle":first,
                "particle_count":t.members.len(),"material_volume":t.volume,"translation":xyz(centre),"rotation":rotation.to_wxyz(),
                "proxy_bbox":{"min":xyz(candidate.bbox.min),"max":xyz(candidate.bbox.max)}}));
                if clusters.len() == 1 {
                    state.poll_control(config, true);
                }
                placed = true;
                break;
            }
            processed_clusters += 1;
            if !placed {
                failed_clusters += 1;
                state.shortfall_total += t.members.len();
                state
                    .first_failed_diameter
                    .get_or_insert(t.members[0].diameter);
                if config.on_unattainable == OnUnattainable::Stop {
                    break;
                }
            }
        }
        stages_report.push(json!({"stage":stage,"particles_per_cluster":counts[stage],"planned_clusters":stage_ids.len(),"placed_clusters":clusters.len()-before,"attempts":state.attempts-before_attempts,"volume_fraction_solid":state.volume_solid/basis}));
    }
    state.poll_control(config, true);
    let global_target_reached =
        (state.volume_solid - target).abs() <= target * config.target_tolerance;
    let reason = if state.control.interrupted {
        StopReason::Interrupted
    } else if state.placed.is_empty() {
        StopReason::NoFeasiblePlacement
    } else if spec.mode == AggregateMode::Mixed && global_target_reached {
        StopReason::TargetReached
    } else if state.budget_spent(config) && processed_clusters < planned_clusters {
        StopReason::BudgetExhausted
    } else if failed_clusters > 0 {
        StopReason::DistributionUnattainable
    } else if (state.volume_solid - target).abs() <= target * config.target_tolerance {
        StopReason::TargetReached
    } else {
        StopReason::DistributionUnattainable
    };
    let unmet_templates = catalog
        .iter()
        .filter(|t| t["target_reached"] == json!(false))
        .count();
    let stop = StopDecision { reason,detail:BTreeMap::from([
        ("message".into(),"Aggregate pack: only true member volume counts; proxy empty space is matrix. Partial results preserve complete accepted clusters.".into()),
        ("planned_clusters".into(),planned_clusters.to_string()),("placed_clusters".into(),clusters.len().to_string()),
        ("failed_clusters".into(),failed_clusters.to_string()),("failed_sizes".into(),state.shortfall_total.to_string()),
        ("attempt_unit".into(),"cluster proposal".into()),("templates_below_internal_target".into(),unmet_templates.to_string()),
        ("aggregate_mode".into(),format!("{:?}",spec.mode)),("global_target_reached".into(),global_target_reached.to_string())]) };
    state.control.publish(
        config,
        "saving",
        state.placed.len(),
        state.attempts,
        state.volume_solid / basis,
    );
    let proxy_volume: f64 = proxies
        .iter()
        .map(|p| match spec.shape {
            AggregateShape::Sphere => 4.0 / 3.0 * std::f64::consts::PI * p.radius.powi(3),
            AggregateShape::Cube => p.bbox.volume(),
        })
        .sum();
    let clusters_path = config.outputs.dir.join("aggregates.json");
    write_json(
        &clusters_path,
        &json!({"schema_version":"rustmspt.aggregates/1","config":spec,"clusters":clusters,
        "planned_clusters":planned_clusters,"processed_clusters":processed_clusters,"stages":stages_report,"global_target_reached":global_target_reached,
        "proxy_volume":proxy_volume,"material_volume":state.volume_solid,
        "proxy_fraction_domain":proxy_volume/config.domain.volume(),
        "template_catalog":"aggregate_templates.json","internal_gap":spec.internal_gap,"global_proxy_gap":config.gap_particle_particle}),
    )?;
    let mut outputs = write_outputs(config, library, &state, tool, &frame, classes, void)?;
    outputs.push(describe_output("aggregate_templates", &template_path));
    outputs.push(describe_output("aggregates", &clusters_path));
    finish_report(
        &mut report,
        config,
        &state,
        classes,
        &TopUpReport {
            batches: 0,
            drawn: 0,
            placed: 0,
        },
        &stop,
        started.elapsed().as_secs_f64(),
        threads,
        outputs,
        target,
    );
    write_json(&config.outputs.report, &report)?;
    state.control.publish(
        config,
        &report.status,
        state.placed.len(),
        state.attempts,
        state.volume_solid / basis,
    );
    Ok(PlacementOutcome {
        placed: state.placed.len(),
        stop_reason: reason,
        volume_fraction_solid: state.volume_solid / basis,
        summary_lines: vec![format!("[Aggregate] mode={:?} particles={} clusters={} VF={:.8} stop={:?} templates_below_internal_target={unmet_templates}",spec.mode,state.placed.len(),clusters.len(),state.volume_solid/basis,reason)],
    })
}
