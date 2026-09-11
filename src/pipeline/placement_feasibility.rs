use crate::config::placement::{BoundaryMode, ResolvedBoundary};
use crate::config::placement::VoidCrossing;
use crate::geometry::{
    bbox_distance, bbox_overlaps, cut_face_names, mesh_distance_exact_prepared,
    mesh_solids_nested_prepared, mesh_surfaces_intersect_prepared, mesh_volume_in_bbox_exact,
    to_parry_trimesh, UnitQuat, VoidIndex,
};
use crate::types::{BoundingBox, Mesh, Vec3};
use parry3d_f64::shape::TriMesh;

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

    // Neighbours come from the spatial grid, already dilated by the gap.
    for &index in ctx.neighbours {
        let other = &ctx.placed[index];

        // Two particles whose centres are farther apart than the sum of their
        // reaches plus the gap cannot possibly be too close, whatever shape they
        // are. This costs three subtractions and rejects almost every pair.
        let delta = candidate.centre.sub(other.translation);
        let centre_distance = delta.dot(delta).sqrt();
        if centre_distance > candidate.reach + other.reach + ctx.gap_particle_particle {
            continue;
        }
        // Then the boxes, which are tighter than the spheres.
        if bbox_distance(bbox, other.bbox) >= ctx.gap_particle_particle
            && !bbox_overlaps(bbox, other.bbox)
        {
            continue;
        }

        // Only now is a bounding-volume hierarchy worth building.
        if shape.is_none() {
            shape = to_parry_trimesh(candidate.mesh);
        }
        if mesh_surfaces_intersect_prepared(
            Some(bbox),
            shape.as_ref(),
            Some(other.bbox),
            other.shape.as_ref(),
        ) {
            return Err(RejectReason::ParticleOverlap);
        }
        // The same nesting case the void arm above tests for, asked of a pair of
        // particles. Surface intersection cannot see it and the distance below
        // reports the gap between the two surfaces as if it were clearance, so
        // without this a particle sits inside another and its volume is counted
        // twice. One box comparison settles almost every pair.
        if mesh_solids_nested_prepared(
            Some(bbox),
            shape.as_ref(),
            Some(other.bbox),
            other.shape.as_ref(),
        ) {
            return Err(RejectReason::ParticleEnclosed);
        }
        if ctx.gap_particle_particle > 0.0 {
            let d = mesh_distance_exact_prepared(
                Some(bbox),
                shape.as_ref(),
                Some(other.bbox),
                other.shape.as_ref(),
            );
            if d < ctx.gap_particle_particle {
                return Err(RejectReason::ParticleGap);
            }
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
