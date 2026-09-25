use crate::config::placement::{BoundaryMode, ResolvedBoundary};
use crate::config::placement::VoidCrossing;
use crate::geometry::{
    bbox_distance, bbox_overlaps, cut_face_names, mesh_distance_exact_prepared,
    mesh_solids_nested_prepared, mesh_surfaces_intersect_prepared, mesh_volume_in_bbox_exact,
    to_parry_trimesh, UnitQuat, VoidIndex,
};
use crate::types::{BoundingBox, Mesh, Vec3};
use parry3d_f64::shape::TriMesh;
use rayon::prelude::*;

/// The smallest number of neighbour pairs needing an exact distance for which those distances are
/// evaluated in parallel; `usize::MAX` keeps the engine serial. `pair_threshold_benchmark` shows
/// 1.4-2x from two clear pairs upward, but end to end on dense runs (PLAN.Performance, PERF-12 item 2)
/// about 98 % of attempts are rejected, the ordered search still waits for in-flight speculative
/// distances, and the parallel path measured 0-10 % slower on a shared 4-core host, so it is off by
/// default. The answer is identical either way; only speed changes.
pub const PAIR_PARALLEL_MIN: usize = usize::MAX;

/// Why a proposed placement was not accepted.
///
/// The names are the report's keys, so a rejection tally can be read against the
/// order the checks actually run in. Each variant is produced by exactly one arm
/// of `check_placement`, which is what makes the tally attributable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RejectReason {
    /// Strict mode: some part of the particle lies outside the domain.
    OutsideDomain,
    /// Clip mode: the particle straddles the boundary but keeps too little inside.
    BoundaryDepth,
    /// The particle's centroid lies inside the void.
    InsideVoid,
    /// The particle comes closer to the void surface than the required clearance.
    VoidGap,
    /// The particle would swallow a void shell, or sit inside one.
    VoidEnclosed,
    /// The particle's surface crosses that of one already placed.
    ParticleOverlap,
    /// The particle lies wholly inside one already placed, or would swallow one.
    /// Two closed surfaces in that arrangement never cross, so nothing in
    /// `ParticleOverlap` can see it.
    ParticleEnclosed,
    /// The particle clears every other one, but by less than the required gap.
    ParticleGap,
    /// Nothing of the particle is left inside the domain.
    ZeroInDomainVolume,
    /// Void-neighbourhood mode: the centroid fell outside the declared band.
    NeighbourhoodBand,
}

impl RejectReason {
    // AI-FUNC-SUMMARY: The report key for this reason; returns a stable snake_case name; side effects: none.
    pub fn as_str(self) -> &'static str {
        match self {
            RejectReason::OutsideDomain => "outside_domain",
            RejectReason::BoundaryDepth => "boundary_depth",
            RejectReason::InsideVoid => "inside_void",
            RejectReason::VoidGap => "void_gap",
            RejectReason::VoidEnclosed => "void_enclosed",
            RejectReason::ParticleOverlap => "particle_overlap",
            RejectReason::ParticleEnclosed => "particle_enclosed",
            RejectReason::ParticleGap => "particle_gap",
            RejectReason::ZeroInDomainVolume => "zero_in_domain_volume",
            RejectReason::NeighbourhoodBand => "neighbourhood_band",
        }
    }

    /// Every reason, in the order the checks run. The report emits its tally in
    /// this order so the counts read as a funnel rather than as an arbitrary map.
    pub const ALL: [RejectReason; 10] = [
        RejectReason::NeighbourhoodBand,
        RejectReason::OutsideDomain,
        RejectReason::BoundaryDepth,
        RejectReason::InsideVoid,
        RejectReason::VoidGap,
        RejectReason::VoidEnclosed,
        RejectReason::ParticleOverlap,
        RejectReason::ParticleEnclosed,
        RejectReason::ParticleGap,
        RejectReason::ZeroInDomainVolume,
    ];
}

/// A particle that cleared every check, with everything the record needs.
pub struct PlacedParticle {
    pub acceptance_index: usize,
    pub source_index: usize,
    pub shell_index: usize,
    pub scale: f64,
    pub rotation: UnitQuat,
    pub translation: Vec3,
    pub equivalent_diameter: f64,
    /// Distance from the centroid to the furthest vertex, after scaling.
    pub reach: f64,
    pub size_class: usize,
    pub volume_full: f64,
    pub volume_in_domain: f64,
    pub void_overlap_volume: f64,
    pub clipped_faces: Vec<&'static str>,
    pub bbox: BoundingBox,
    pub mesh: Mesh,
    /// Built once when the particle is accepted and kept for every later query.
    /// The original engine rebuilds one per comparison, which is the dominant cost
    /// of its collision loop.
    pub shape: Option<TriMesh>,
    pub triangle_range: (usize, usize),
}

impl PlacedParticle {
    // AI-FUNC-SUMMARY: The particle volume that counts toward the solid phase; returns f64; side effects: none.
    // Notes: in_domain is gross - it includes any part sitting inside the void. The void owns that
    // overlap, so the solid share is in_domain minus it, clamped at zero because the two numbers
    // come from different methods (an exact clip and a voxel count) and can disagree in the last
    // place for a particle almost entirely inside the void.
    pub fn volume_in_domain_solid(&self) -> f64 {
        (self.volume_in_domain - self.void_overlap_volume).max(0.0)
    }
}

/// Everything a feasibility check reads, gathered once per attempt.
pub struct FeasibilityContext<'a> {
    pub domain: BoundingBox,
    pub boundary: &'a ResolvedBoundary,
    pub gap_particle_particle: f64,
    pub placed: &'a [PlacedParticle],
    /// Indices of already-placed particles worth comparing against, from the
    /// spatial grid. Comparing against all of them is what the original engine
    /// does, and it is O(n) per attempt.
    pub neighbours: &'a [usize],
    /// The frozen void, when the run has one.
    pub void: Option<&'a VoidIndex>,
    pub void_crossing: VoidCrossing,
    /// `g_pv`: the clearance every particle keeps from the void surface.
    pub void_gap: f64,
    /// The band a void_neighbourhood run samples within, as distance from the
    /// void surface to the particle's centroid.
    pub neighbourhood_band: Option<(f64, f64)>,
    /// Number of pairs needing an exact distance at which those distances run in parallel. The
    /// first rejection is always taken in neighbour order, so this changes speed, never the answer.
    /// `usize::MAX` forces serial evaluation, `0` forces parallel.
    pub pair_parallel_min: usize,
}

/// A proposed placement, with the cheap quantities already worked out.
pub struct Candidate<'a> {
    pub mesh: &'a Mesh,
    pub bbox: BoundingBox,
    /// The placed volume centroid, which is where the proposal put it.
    pub centre: Vec3,
    /// Distance from the centre to the furthest vertex, after scaling. Rotation
    /// cannot change it, so it bounds the particle whatever its orientation.
    pub reach: f64,
    /// `scale^3` times the source shell's volume. Exact by construction, and far
    /// cheaper than summing the transformed mesh's tetrahedra on every attempt.
    pub volume_full: f64,
}

/// What a passing check worked out along the way, so the caller need not redo it.
pub struct Accepted {
    pub volume_in_domain: f64,
    pub clipped_faces: Vec<&'static str>,
    pub shape: Option<TriMesh>,
}

// AI-FUNC-SUMMARY:
// Purpose: Decide whether a proposed particle may be placed, and if not, say which rule stopped it.
// Inputs: the context (domain, boundary rule, gap, placed particles, neighbour indices) and the candidate.
// Returns: Ok(Accepted) with the in-domain volume and the particle's TriMesh, or Err(reason).
// Side effects: None.
// Notes: The checks run in one fixed order, cheapest and most selective first, and each returns its
// own named reason - which is what makes the report's rejection tally readable as a funnel rather
// than as a set of unrelated counts. Void checks slot in between the boundary and the neighbours.
//
// Three things are deliberately deferred rather than computed up front, because in a run of any
// size most attempts are rejected and never need them:
//   - the parry TriMesh, built only when a neighbour survives the cheap tests. Building one per
//     attempt means building a bounding-volume hierarchy per attempt, which dominated everything
//     else when it was written that way.
//   - the exact in-box volume, run only for a particle whose bounding box actually straddles the
//     domain. One wholly inside keeps its full volume by definition.
//   - the exact pair distance, reached only after a centre-to-centre test and a box test have both
//     failed to separate the pair. Two spheres of known reach cannot be too close if their centres
//     are farther apart than the sum of their reaches plus the gap.
// The cheap neighbour tests run serially. Overlap and enclosure then run serially up to the first
// failing survivor, and only the pairs before it need the expensive exact distance; when at least
// `ctx.pair_parallel_min` of them do, the distances run in parallel with the ordered
// `find_map_first` (see first_pair_rejection). The returned reason is therefore always the one serial
// evaluation returns, in neighbour order and per-pair check order, on any thread count. The caller
// increments every counter.
pub fn check_placement(
    ctx: &FeasibilityContext,
    candidate: &Candidate,
) -> std::result::Result<Accepted, RejectReason> {
    let bbox = candidate.bbox;

    let inside_domain = |b: BoundingBox, d: BoundingBox| {
        b.min.x >= d.min.x
            && b.min.y >= d.min.y
            && b.min.z >= d.min.z
            && b.max.x <= d.max.x
            && b.max.y <= d.max.y
            && b.max.z <= d.max.z
    };
    let separated = |b: BoundingBox, d: BoundingBox| {
        b.max.x <= d.min.x
            || b.min.x >= d.max.x
            || b.max.y <= d.min.y
            || b.min.y >= d.max.y
            || b.max.z <= d.min.z
            || b.min.z >= d.max.z
    };

    // Domain rule first: a handful of comparisons, and it rejects most of the
    // proposals that will ever be rejected.
    let mut deferred_clip = false;
    let (mut volume_in_domain, mut clipped_faces) = match ctx.boundary.mode {
        BoundaryMode::Strict => {
            let inner = ctx.domain.expanded(-ctx.boundary.min_boundary_dist);
            if !inside_domain(bbox, inner) {
                return Err(RejectReason::OutsideDomain);
            }
            (candidate.volume_full, Vec::new())
        }
        BoundaryMode::Clip | BoundaryMode::Periodic => {
            if separated(bbox, ctx.domain) {
                return Err(RejectReason::ZeroInDomainVolume);
            }
            if inside_domain(bbox, ctx.domain) {
                (candidate.volume_full, Vec::new())
            } else {
                deferred_clip = true;
                (candidate.volume_full, Vec::new())
            }
        }
    };

    let mut shape: Option<TriMesh> = None;

    // The void, if there is one. The order here is the completeness argument, and
    // it is worth stating in full because the predicate is not obvious.
    //
    // For closed, non-self-intersecting surfaces, these three together are
    // equivalent to "the particle solid and the void solid are disjoint and at
    // least `gap` apart":
    //   (a) no particle vertex is inside the void,
    //   (b) the surfaces do not intersect and are at least `gap` apart,
    //   (c) no void vertex is inside the particle.
    // Because (b) says the surfaces never cross, each closed solid is wholly
    // inside or wholly outside the other. (a) rules out the particle being inside
    // the void, (c) rules out the void being inside the particle, and (b) supplies
    // the separation. A shell strictly inside another has *all* of its vertices
    // inside it, so neither vertex test can miss the case it exists for.
    //
    // The cost is bounded by the box prefilter: if no void triangle comes within
    // `gap` of the particle's box, the particle is both far from the void and not
    // nested in it, and none of the three tests needs to run.
    //
    // The same argument governs a pair of particles, and the neighbour loop below
    // runs it: (b) is `mesh_surfaces_intersect_prepared` plus the distance test,
    // and (a) and (c) collapse into the single symmetric
    // `mesh_solids_nested_prepared`. v0.2.0 proved the argument here and then left
    // both vertex tests out of the particle arm, which is how it came to place
    // particles inside other particles and report the target as reached.
    if let Some(void) = ctx.void {
        if let Some((lo, hi)) = ctx.neighbourhood_band {
            let d = void.surface_distance(candidate.centre);
            if d < lo || d > hi {
                return Err(RejectReason::NeighbourhoodBand);
            }
        }
        match ctx.void_crossing {
            VoidCrossing::Forbidden => {
                if void.near_box(bbox, ctx.void_gap) {
                    if void.any_vertex_inside(candidate.mesh) {
                        return Err(RejectReason::InsideVoid);
                    }
                    shape = to_parry_trimesh(candidate.mesh);
                    if void.intersects(shape.as_ref().ok_or(RejectReason::VoidGap)?) {
                        return Err(RejectReason::VoidGap);
                    }
                    let d = void.min_distance_to(shape.as_ref().ok_or(RejectReason::VoidGap)?);
                    if d < ctx.void_gap {
                        return Err(RejectReason::VoidGap);
                    }
                    if void.any_void_vertex_inside(candidate.mesh, bbox) {
                        return Err(RejectReason::VoidEnclosed);
                    }
                }
            }
            VoidCrossing::Allowed => {
                // Crossing is permitted, but a particle whose centre sits inside a
                // pore is not a particle in the solid phase at all.
                if void.contains_point(candidate.centre) {
                    return Err(RejectReason::InsideVoid);
                }
            }
        }
    }

    // Neighbours come from the spatial grid, already dilated by the gap. The cheap
    // sphere and box tests run serially in neighbour order; only the survivors
    // reach the exact pair tests.
    let survivors: Vec<usize> = ctx
        .neighbours
        .iter()
        .copied()
        .filter(|&index| pair_needs_exact_test(ctx, candidate, &ctx.placed[index]))
        .collect();
    if !survivors.is_empty() {
        // Only now is a bounding-volume hierarchy worth building.
        if shape.is_none() {
            shape = to_parry_trimesh(candidate.mesh);
        }
        let shape_ref = shape.as_ref();
        let first = first_pair_rejection(
            bbox,
            shape_ref,
            ctx.placed,
            &survivors,
            ctx.gap_particle_particle,
            ctx.pair_parallel_min,
        );
        if let Some(reason) = first {
            return Err(reason);
        }
    }

    // The exact clip runs last, and only for a particle that really does straddle
    // the boundary: it is the most expensive check and the least selective.
    if deferred_clip {
        let (v, cut) = mesh_volume_in_bbox_exact(candidate.mesh, ctx.domain);
        let names = cut_face_names(cut);
        if !v.is_finite() || v <= 0.0 {
            return Err(RejectReason::ZeroInDomainVolume);
        }
        if !names.is_empty() && ctx.boundary.min_cross_boundary_depth > 0.0 {
            let depth = retained_depth(bbox, ctx.domain, &names);
            if depth < ctx.boundary.min_cross_boundary_depth {
                return Err(RejectReason::BoundaryDepth);
            }
        }
        volume_in_domain = v;
        clipped_faces = names;
    }

    if !volume_in_domain.is_finite() || volume_in_domain <= 0.0 {
        return Err(RejectReason::ZeroInDomainVolume);
    }

    // An accepted particle keeps its hierarchy for every later comparison, which
    // is what the original engine never does: it rebuilds one per query.
    if shape.is_none() {
        shape = to_parry_trimesh(candidate.mesh);
    }

    Ok(Accepted {
        volume_in_domain,
        clipped_faces,
        shape,
    })
}

// AI-FUNC-SUMMARY: Whether a placed neighbour survives the centre-sphere and box separation tests; returns true when the exact pair tests must run; side effects: none.
// Notes: Two particles whose centres are farther apart than the sum of their reaches plus the gap
// cannot be too close whatever their shape; then the boxes, which are tighter than the spheres.
fn pair_needs_exact_test(ctx: &FeasibilityContext, candidate: &Candidate, other: &PlacedParticle) -> bool {
    let delta = candidate.centre.sub(other.translation);
    let centre_distance = delta.dot(delta).sqrt();
    if centre_distance > candidate.reach + other.reach + ctx.gap_particle_particle {
        return false;
    }
    if bbox_distance(candidate.bbox, other.bbox) >= ctx.gap_particle_particle
        && !bbox_overlaps(candidate.bbox, other.bbox)
    {
        return false;
    }
    true
}

// AI-FUNC-SUMMARY:
// Purpose: Find the first exact pair rejection over the surviving neighbours, exactly as a serial scan would.
// Inputs: the candidate's box and prepared shape, the placed particles, survivor indices in neighbour order, the gap, and the parallel threshold.
// Returns: the reason serial evaluation returns - the first failing pair in survivor order, and within it the first failing check (overlap, then enclosure, then gap) - or None.
// Side effects: None; reads only immutable prepared shapes.
// Notes: Serial evaluation per pair is overlap -> enclosure -> gap. Overlap and enclosure cost tens of
// microseconds; the exact distance costs hundreds and is the whole bill. So the scan is split: phase A
// walks survivors serially through overlap and enclosure, stopping at the first failure f; phase B
// needs distances only for pairs before f, since no later pair can be first. Those distances run in
// parallel above `parallel_min` with the ordered `find_map_first`, so the smallest gap-failing index
// c wins and the answer is Gap at c if it exists, else phase A's reason at f. Every pair phase B
// reaches passed overlap and enclosure, which is why its gap failure is also its serial reason.
fn first_pair_rejection(
    bbox: BoundingBox,
    shape: Option<&TriMesh>,
    placed: &[PlacedParticle],
    survivors: &[usize],
    gap: f64,
    parallel_min: usize,
) -> Option<RejectReason> {
    let mut cut = survivors.len();
    let mut solid = None;
    for (position, &index) in survivors.iter().enumerate() {
        if let Some(reason) = solid_pair_rejection(bbox, shape, &placed[index]) {
            cut = position;
            solid = Some(reason);
            break;
        }
    }
    if gap > 0.0 {
        let too_close = |index: &usize| {
            let other = &placed[*index];
            let d = mesh_distance_exact_prepared(Some(bbox), shape, Some(other.bbox), other.shape.as_ref());
            (d < gap).then_some(RejectReason::ParticleGap)
        };
        let before = &survivors[..cut];
        let gap_reason = if before.len() >= parallel_min {
            before.par_iter().find_map_first(too_close)
        } else {
            before.iter().find_map(too_close)
        };
        if gap_reason.is_some() {
            return gap_reason;
        }
    }
    solid
}

// AI-FUNC-SUMMARY: The overlap and enclosure tests for one candidate-neighbour pair, in that order; returns the first failing reason or None; side effects: none.
// Notes: Surface intersection cannot see one solid wholly inside the other, and the distance reports
// the gap between the two surfaces as if it were clearance, so the nesting test must run before it.
fn solid_pair_rejection(bbox: BoundingBox, shape: Option<&TriMesh>, other: &PlacedParticle) -> Option<RejectReason> {
    if mesh_surfaces_intersect_prepared(Some(bbox), shape, Some(other.bbox), other.shape.as_ref()) {
        return Some(RejectReason::ParticleOverlap);
    }
    if mesh_solids_nested_prepared(Some(bbox), shape, Some(other.bbox), other.shape.as_ref()) {
        return Some(RejectReason::ParticleEnclosed);
    }
    None
}

// AI-FUNC-SUMMARY: The full serial per-pair check order (overlap, enclosure, gap) for one pair; returns the first failing reason or None; side effects: none.
// Notes: The reference first_pair_rejection must reproduce; used by the fixtures that prove it does.
#[cfg(test)]
fn exact_pair_rejection(bbox: BoundingBox, shape: Option<&TriMesh>, other: &PlacedParticle, gap: f64) -> Option<RejectReason> {
    if let Some(reason) = solid_pair_rejection(bbox, shape, other) {
        return Some(reason);
    }
    if gap > 0.0 {
        let d = mesh_distance_exact_prepared(Some(bbox), shape, Some(other.bbox), other.shape.as_ref());
        if d < gap {
            return Some(RejectReason::ParticleGap);
        }
    }
    None
}

// AI-FUNC-SUMMARY:
// Purpose: Measure how far a straddling particle still reaches inside the domain.
// Inputs: the particle's bounding box, the domain, and the faces it crosses.
// Returns: the smallest inward reach across those faces.
// Side effects: None.
// Notes: Bounding-box based, matching what the original engine's min_cross_boundary_depth measures,
// so the same config number means the same thing in both engines. The smallest reach wins: a
// particle that barely enters across any one face is as unretained as one that barely enters across
// all of them.
fn retained_depth(bbox: BoundingBox, domain: BoundingBox, faces: &[&'static str]) -> f64 {
    let mut depth = f64::INFINITY;
    for face in faces {
        let reach = match *face {
            "xmin" => bbox.max.x - domain.min.x,
            "xmax" => domain.max.x - bbox.min.x,
            "ymin" => bbox.max.y - domain.min.y,
            "ymax" => domain.max.y - bbox.min.y,
            "zmin" => bbox.max.z - domain.min.z,
            "zmax" => domain.max.z - bbox.min.z,
            _ => f64::INFINITY,
        };
        depth = depth.min(reach);
    }
    if depth.is_finite() {
        depth
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::spatial::SpatialGrid;
    use crate::geometry::{icosphere_mesh, mesh_bbox, mesh_volume, sample_uniform_quaternion, transform_shell};
    use crate::pipeline::rng::{seeded_rng, u01, uniform_index, uniform_range};
    use std::collections::BTreeMap;
    use std::time::Instant;

    #[derive(Debug, PartialEq)]
    enum Outcome {
        Rejected(RejectReason),
        Accepted { volume_bits: u64, clipped: Vec<&'static str> },
    }

    type Transform = (u64, [u64; 4], [u64; 3]);

    struct Replay {
        outcomes: Vec<Outcome>,
        tally: BTreeMap<RejectReason, usize>,
        transforms: Vec<Transform>,
        max_survivors: usize,
        parallel_attempts: usize,
        oracle_checks: usize,
    }

    // AI-FUNC-SUMMARY: A centred triaxial ellipsoid shell for orientation-sensitive pair tests; returns (mesh, reach); side effects: none.
    fn ellipsoid(level: u32) -> (Mesh, f64) {
        let mut mesh = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, level);
        for v in mesh.vertices.iter_mut() {
            *v = Vec3::new(v.x * 1.8, v.y, v.z * 0.7);
        }
        let reach = mesh
            .vertices
            .iter()
            .map(|v| v.dot(*v).sqrt())
            .fold(0.0f64, f64::max);
        (mesh, reach)
    }

    // AI-FUNC-SUMMARY: A placed particle built directly from a transformed mesh for feasibility fixtures; returns PlacedParticle with its prepared shape; side effects: none.
    #[allow(clippy::too_many_arguments)]
    fn placed_particle(
        index: usize,
        scale: f64,
        rotation: UnitQuat,
        centre: Vec3,
        reach: f64,
        volume_in_domain: f64,
        clipped_faces: Vec<&'static str>,
        mesh: Mesh,
        shape: Option<TriMesh>,
    ) -> PlacedParticle {
        let bbox = mesh_bbox(&mesh).expect("particle box");
        PlacedParticle {
            acceptance_index: index,
            source_index: 0,
            shell_index: 0,
            scale,
            rotation,
            translation: centre,
            equivalent_diameter: 0.0,
            reach,
            size_class: 0,
            volume_full: volume_in_domain,
            volume_in_domain,
            void_overlap_volume: 0.0,
            clipped_faces,
            bbox,
            mesh,
            shape,
            triangle_range: (0, 0),
        }
    }

    // AI-FUNC-SUMMARY: Replay a fixed seeded candidate sequence through check_placement at one pair threshold, accepting what passes; returns per-attempt outcomes, the tally and accepted transforms; side effects: none.
    // Notes: every attempt draws its variates up front, so the stream never depends on outcomes; counters are incremented here, in the sequential caller.
    fn replay(pair_parallel_min: usize, attempts: usize) -> Replay {
        let (canonical, unit_reach) = ellipsoid(1);
        let unit_volume = mesh_volume(&canonical);
        let domain = BoundingBox { min: Vec3::new(0.0, 0.0, 0.0), max: Vec3::new(24.0, 24.0, 24.0) };
        let boundary = ResolvedBoundary {
            mode: BoundaryMode::Clip,
            min_boundary_dist: 0.0,
            min_cross_boundary_depth: 0.0,
        };
        let gap = 0.3;
        let mut grid = SpatialGrid::new(domain, 3.0 * 2.0 * unit_reach + gap);
        let mut placed: Vec<PlacedParticle> = Vec::new();
        let mut rng = seeded_rng(20260925);
        let mut replay = Replay {
            outcomes: Vec::new(),
            tally: BTreeMap::new(),
            transforms: Vec::new(),
            max_survivors: 0,
            parallel_attempts: 0,
            oracle_checks: 0,
        };
        for _ in 0..attempts {
            let scale = uniform_range(&mut rng, 0.6, 3.0);
            let rotation = sample_uniform_quaternion(&mut rng);
            let free = Vec3::new(
                uniform_range(&mut rng, domain.min.x, domain.max.x),
                uniform_range(&mut rng, domain.min.y, domain.max.y),
                uniform_range(&mut rng, domain.min.z, domain.max.z),
            );
            let aim = u01(&mut rng);
            let host = uniform_index(&mut rng, placed.len().max(1));
            let centre = if aim < 0.2 && !placed.is_empty() {
                placed[host].translation
            } else {
                free
            };
            let mesh = transform_shell(&canonical, scale, rotation, centre);
            let bbox = mesh_bbox(&mesh).expect("candidate box");
            let neighbours = grid.query_neighbors_with_margin(bbox, gap, usize::MAX);
            let ctx = FeasibilityContext {
                domain,
                boundary: &boundary,
                gap_particle_particle: gap,
                placed: &placed,
                neighbours: &neighbours,
                void: None,
                void_crossing: VoidCrossing::Forbidden,
                void_gap: 0.0,
                neighbourhood_band: None,
                pair_parallel_min,
            };
            let candidate = Candidate {
                mesh: &mesh,
                bbox,
                centre,
                reach: unit_reach * scale,
                volume_full: unit_volume * scale * scale * scale,
            };
            let survivor_list: Vec<usize> = neighbours
                .iter()
                .copied()
                .filter(|&i| pair_needs_exact_test(&ctx, &candidate, &placed[i]))
                .collect();
            let survivors = survivor_list.len();
            replay.max_survivors = replay.max_survivors.max(survivors);
            if survivors > 0 {
                let shape = to_parry_trimesh(&mesh);
                let reference = survivor_list
                    .iter()
                    .find_map(|&i| exact_pair_rejection(bbox, shape.as_ref(), &placed[i], gap));
                let phased = first_pair_rejection(bbox, shape.as_ref(), &placed, &survivor_list, gap, pair_parallel_min);
                assert_eq!(phased, reference, "phased pair search disagrees with the per-pair serial scan");
                replay.oracle_checks += 1;
                if survivors >= 2 && survivors >= pair_parallel_min {
                    replay.parallel_attempts += 1;
                }
            }
            match check_placement(&ctx, &candidate) {
                Err(reason) => {
                    *replay.tally.entry(reason).or_insert(0) += 1;
                    replay.outcomes.push(Outcome::Rejected(reason));
                }
                Ok(accepted) => {
                    replay.outcomes.push(Outcome::Accepted {
                        volume_bits: accepted.volume_in_domain.to_bits(),
                        clipped: accepted.clipped_faces.clone(),
                    });
                    replay.transforms.push((
                        scale.to_bits(),
                        [rotation.w.to_bits(), rotation.x.to_bits(), rotation.y.to_bits(), rotation.z.to_bits()],
                        [centre.x.to_bits(), centre.y.to_bits(), centre.z.to_bits()],
                    ));
                    let index = placed.len();
                    grid.insert(index, bbox);
                    placed.push(placed_particle(
                        index,
                        scale,
                        rotation,
                        centre,
                        unit_reach * scale,
                        accepted.volume_in_domain,
                        accepted.clipped_faces,
                        mesh,
                        accepted.shape,
                    ));
                }
            }
        }
        replay
    }

    // AI-FUNC-SUMMARY: Replay one fixed candidate sequence with the exact pair tests forced serial and forced parallel on 1/2/4/8 workers; asserts identical per-attempt reasons, tallies and accepted transforms, and that the phased pair search equals the plain per-pair serial scan on every attempt; no file output.
    #[test]
    fn forced_parallel_pair_checks_match_serial_attempt_by_attempt() {
        let serial = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| replay(usize::MAX, 1500));
        assert_eq!(serial.parallel_attempts, 0);
        for reason in [
            RejectReason::ParticleOverlap,
            RejectReason::ParticleEnclosed,
            RejectReason::ParticleGap,
        ] {
            assert!(
                serial.tally.get(&reason).copied().unwrap_or(0) > 0,
                "fixture never exercised {reason:?}: {:?}",
                serial.tally
            );
        }
        assert!(serial.max_survivors >= 4, "fixture too sparse: {}", serial.max_survivors);
        assert!(serial.transforms.len() > 50, "fixture accepted too little: {}", serial.transforms.len());
        assert!(serial.oracle_checks > 500, "per-pair oracle ran {} times", serial.oracle_checks);
        println!(
            "replay: {} attempts, {} accepted, tally {:?}, max survivors {}, oracle checks {}",
            serial.outcomes.len(),
            serial.transforms.len(),
            serial.tally,
            serial.max_survivors,
            serial.oracle_checks
        );
        for threads in [1usize, 2, 4, 8] {
            let parallel = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| replay(0, 1500));
            assert!(parallel.parallel_attempts > 100, "{}", parallel.parallel_attempts);
            assert_eq!(parallel.outcomes.len(), serial.outcomes.len());
            for (attempt, (a, b)) in serial.outcomes.iter().zip(&parallel.outcomes).enumerate() {
                assert_eq!(a, b, "attempt {attempt} differs at {threads} workers");
            }
            assert_eq!(parallel.tally, serial.tally);
            assert_eq!(parallel.transforms, serial.transforms);
        }
    }

    // AI-FUNC-SUMMARY: Release timing of check_placement against k clear surviving neighbours, serial against ordered parallel, to choose PAIR_PARALLEL_MIN; prints medians of 5 samples after 1 warmup; no file output.
    #[test]
    #[ignore]
    fn pair_threshold_benchmark() {
        let level: u32 = std::env::var("PAIR_BENCH_LEVEL").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
        let (canonical, unit_reach) = ellipsoid(level);
        let domain = BoundingBox { min: Vec3::new(-50.0, -50.0, -50.0), max: Vec3::new(50.0, 50.0, 50.0) };
        let boundary = ResolvedBoundary {
            mode: BoundaryMode::Strict,
            min_boundary_dist: 0.0,
            min_cross_boundary_depth: 0.0,
        };
        let gap = 0.5;
        let identity = UnitQuat::identity();
        let candidate_mesh = transform_shell(&canonical, 2.0, identity, Vec3::new(0.0, 0.0, 0.0));
        let candidate = Candidate {
            mesh: &candidate_mesh,
            bbox: mesh_bbox(&candidate_mesh).unwrap(),
            centre: Vec3::new(0.0, 0.0, 0.0),
            reach: unit_reach * 2.0,
            volume_full: 1.0,
        };
        let candidate_shape = to_parry_trimesh(&candidate_mesh);
        let mut rng = seeded_rng(7);
        let mut placed = Vec::new();
        for index in 0..64usize {
            let theta = uniform_range(&mut rng, 0.0, std::f64::consts::TAU);
            let z = uniform_range(&mut rng, -1.0, 1.0);
            let r = (1.0 - z * z).sqrt();
            let dir = Vec3::new(r * theta.cos(), r * theta.sin(), z);
            let rotation = sample_uniform_quaternion(&mut rng);
            let centre = dir.scale(2.0 * 0.7 + 1.8 + 0.4);
            let mesh = transform_shell(&canonical, 1.0, rotation, centre);
            let shape = to_parry_trimesh(&mesh);
            placed.push(placed_particle(index, 1.0, rotation, centre, unit_reach, 1.0, Vec::new(), mesh, shape));
        }
        let probe = FeasibilityContext {
            domain,
            boundary: &boundary,
            gap_particle_particle: gap,
            placed: &placed,
            neighbours: &[],
            void: None,
            void_crossing: VoidCrossing::Forbidden,
            void_gap: 0.0,
            neighbourhood_band: None,
            pair_parallel_min: usize::MAX,
        };
        let clear: Vec<usize> = (0..placed.len())
            .filter(|&i| pair_needs_exact_test(&probe, &candidate, &placed[i]))
            .filter(|&i| exact_pair_rejection(candidate.bbox, candidate_shape.as_ref(), &placed[i], gap).is_none())
            .collect();
        let survivors_ok = clear.iter().all(|&i| {
            let ctx = FeasibilityContext {
                domain,
                boundary: &boundary,
                gap_particle_particle: gap,
                placed: &placed,
                neighbours: &[],
                void: None,
                void_crossing: VoidCrossing::Forbidden,
                void_gap: 0.0,
                neighbourhood_band: None,
                pair_parallel_min: usize::MAX,
            };
            pair_needs_exact_test(&ctx, &candidate, &placed[i])
        });
        println!(
            "faces per particle {}, clear exact-tested neighbours available {} (all survive cheap tests: {survivors_ok})",
            canonical.faces.len(),
            clear.len()
        );
        for threads in [2usize, 4] {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            for k in [1usize, 2, 3, 4, 6, 8, 12, 16] {
                if k > clear.len() {
                    continue;
                }
                let neighbours = &clear[..k];
                let time = |min: usize| {
                    pool.install(|| {
                        let ctx = FeasibilityContext {
                            domain,
                            boundary: &boundary,
                            gap_particle_particle: gap,
                            placed: &placed,
                            neighbours,
                            void: None,
                            void_crossing: VoidCrossing::Forbidden,
                            void_gap: 0.0,
                            neighbourhood_band: None,
                            pair_parallel_min: min,
                        };
                        let mut samples = Vec::new();
                        for round in 0..6 {
                            let t = Instant::now();
                            for _ in 0..40 {
                                assert!(check_placement(&ctx, &candidate).is_ok());
                            }
                            if round > 0 {
                                samples.push(t.elapsed().as_secs_f64() / 40.0);
                            }
                        }
                        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        samples[2]
                    })
                };
                let serial = time(usize::MAX);
                let parallel = time(0);
                println!(
                    "threads {threads} survivors {k:2}: serial {serial:.6e} s, parallel {parallel:.6e} s, serial/parallel {:.3}",
                    serial / parallel
                );
            }
        }
    }
}
