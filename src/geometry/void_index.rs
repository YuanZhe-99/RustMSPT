use crate::error::{Result, RustMsptError};
use crate::geometry::bbox::mesh_bbox;
use crate::geometry::collision::{to_parry_trimesh, trimesh_contains_point, HIT_EPS, RAY_DIR};
use crate::geometry::mesh_ops::split_mesh_into_granules;
use crate::geometry::volume::{mesh_signed_volume, mesh_volume_in_bbox_exact};
use crate::pipeline::rng::u01;
use crate::types::{BoundingBox, Mesh, Vec3};
use parry3d_f64::bounding_volume::Aabb;
use parry3d_f64::math::{Isometry, Point};
use parry3d_f64::query;
use parry3d_f64::query::RayCast;
use parry3d_f64::shape::{TriMesh, Triangle as PTriangle};
use rand_chacha::rand_core::RngCore;


/// Which method produced a void's in-domain volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoidVolumeMethod {
    /// The void lies inside the domain, so its shells' volumes sum exactly.
    ExactShellSum,
    /// The void crosses the domain, so the part inside was clipped exactly.
    ExactClip,
}

/// A frozen void, indexed for the queries a placement run makes against it.
///
/// Built once. Nothing here ever modifies the mesh: the void is frozen, and the
/// run records that its input and output digests match.
///
/// The Debug impl is deliberately terse: the mesh and its hierarchy would fill a
/// screen and say nothing a reader wants.
pub struct VoidIndex {
    mesh: Mesh,
    shape: TriMesh,
    bbox: BoundingBox,
    shell_volumes: Vec<f64>,
    /// Cumulative triangle areas, for area-weighted surface sampling.
    cumulative_area: Vec<f64>,
    total_area: f64,
    /// True when every shell winds outward. An all-inward void is accepted with
    /// its sign flipped and reported as such; a mixed one is refused.
    outward: bool,
}

impl VoidIndex {
    // AI-FUNC-SUMMARY:
    // Purpose: Validate a void mesh and build the index a placement run queries against it.
    // Inputs: the void mesh.
    // Returns: the index, or an error naming which shell is wrong and why.
    // Side effects: None.
    // Notes: Every shell must be closed and all shells must agree about orientation. Mixed
    // orientation is refused rather than repaired, because mesh_volume takes the absolute value of
    // the whole sum: a void of two pores, one of them inverted, would silently report |V1 - V2|
    // instead of V1 + V2, and a target volume fraction computed against it would be quietly wrong.
    // An all-inward void is accepted - ray parity does not care about winding - and reported.
    pub fn build(mesh: &Mesh) -> Result<VoidIndex> {
        if mesh.faces.is_empty() {
            return Err(RustMsptError::InvalidMesh(
                "the void mesh has no faces".to_string(),
            ));
        }
        let shells = split_mesh_into_granules(mesh);
        let mut signed = Vec::with_capacity(shells.len());
        for (i, shell) in shells.iter().enumerate() {
            if let Err(reason) = crate::geometry::metrics::mesh_closedness(shell) {
                return Err(RustMsptError::InvalidMesh(format!(
                    "the void's shell {i}: {reason}. A void has to be a closed solid: the run \
                     measures distance to its surface and asks whether points are inside it, and \
                     neither question has an answer for an open surface."
                )));
            }
            let v = mesh_signed_volume(shell);
            signed.push(v);
        }
        let positive = signed.iter().filter(|v| **v > 0.0).count();
        let negative = signed.len() - positive;
        if positive > 0 && negative > 0 {
            return Err(RustMsptError::InvalidMesh(format!(
                "the void's shells disagree about orientation: {positive} wind outward and \
                 {negative} wind inward. Their volumes would cancel rather than add, so the solid \
                 fraction computed against this void would be silently wrong. Re-export the file \
                 with one consistent orientation."
            )));
        }
        let outward = negative == 0;

        let shape = to_parry_trimesh(mesh).ok_or_else(|| {
            RustMsptError::InvalidMesh(
                "the void mesh could not be indexed for collision queries".to_string(),
            )
        })?;
        let bbox = mesh_bbox(mesh).ok_or_else(|| {
            RustMsptError::InvalidMesh("the void mesh has no bounding box".to_string())
        })?;

        let mut cumulative_area = Vec::with_capacity(mesh.faces.len());
        let mut running = 0.0;
        for f in &mesh.faces {
            let a = mesh.vertices[f.a];
            let b = mesh.vertices[f.b];
            let c = mesh.vertices[f.c];
            running += b.sub(a).cross(c.sub(a)).dot(b.sub(a).cross(c.sub(a))).sqrt() * 0.5;
            cumulative_area.push(running);
        }

        Ok(VoidIndex {
            mesh: mesh.clone(),
            shape,
            bbox,
            shell_volumes: signed.iter().map(|v| v.abs()).collect(),
            cumulative_area,
            total_area: running,
            outward,
        })
    }

    // AI-FUNC-SUMMARY: The void's bounding box; returns BoundingBox; side effects: none.
    pub fn bbox(&self) -> BoundingBox {
        self.bbox
    }

    // AI-FUNC-SUMMARY: How many closed shells the void has; returns usize; side effects: none.
    pub fn shells(&self) -> usize {
        self.shell_volumes.len()
    }

    // AI-FUNC-SUMMARY: Whether the void's shells wind outward; returns bool; side effects: none.
    pub fn is_outward(&self) -> bool {
        self.outward
    }

    // AI-FUNC-SUMMARY: The void's total closed volume, summed over shells with agreeing signs; returns f64; side effects: none.
    pub fn total_volume(&self) -> f64 {
        self.shell_volumes.iter().sum()
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Decide whether a point lies inside the void.
    // Inputs: the point.
    // Returns: true when inside.
    // Side effects: None.
    // Notes: Ray parity over the bounding-volume hierarchy, with the same fixed direction and
    // 1e-8 hit tolerance as s2::point_inside_mesh, so the two agree. Parity, not a pseudo-normal
    // test: parity is correct for a shell nested inside another and does not care which way the
    // faces wind, and a void made of pores inside pores is a real case.
    // Notes on the shared implementation: the parity walk itself lives in
    // collision::trimesh_contains_point, which every solid-containment question in the crate now
    // goes through. The box pre-check stays here because a void's box is usually a small part of
    // the domain, and most queried points are nowhere near it.
    pub fn contains_point(&self, p: Vec3) -> bool {
        if !self.bbox.expanded(1e-9).contains_point(p) {
            return false;
        }
        trimesh_contains_point(&self.shape, p)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Say whether any void triangle comes within a margin of a box.
    // Inputs: the query box and the margin.
    // Returns: false when no void triangle is that close.
    // Side effects: None.
    // Notes: The prefilter every void query starts with. A `false` means the particle is farther
    // than the margin from every void triangle AND cannot be nested inside a shell, because a
    // nested particle's box necessarily intersects the shell's own leaves. That single fact is what
    // keeps the vertex loops off the hot path.
    pub fn near_box(&self, bbox: BoundingBox, margin: f64) -> bool {
        let b = bbox.expanded(margin);
        let aabb = Aabb::new(
            Point::new(b.min.x, b.min.y, b.min.z),
            Point::new(b.max.x, b.max.y, b.max.z),
        );
        let mut out: Vec<u32> = Vec::new();
        self.shape.qbvh().intersect_aabb(&aabb, &mut out);
        !out.is_empty()
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Test whether a particle's surface intersects the void's.
    // Inputs: the particle's parry shape.
    // Returns: true when the two surfaces intersect.
    // Side effects: None.
    // Notes: Needed alongside the distance test because `query::distance` returns exactly 0.0 for
    // intersecting shapes, so a `distance >= gap` test with gap 0 would pass for a particle driven
    // straight through the void. The config refuses a zero gap for that reason; this is the second
    // line of defence.
    pub fn intersects(&self, shape: &TriMesh) -> bool {
        query::intersection_test(
            &Isometry::identity(),
            shape,
            &Isometry::identity(),
            &self.shape,
        )
        .unwrap_or(false)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Minimum distance from a particle's surface to the void's.
    // Inputs: the particle's parry shape.
    // Returns: the distance, 0.0 when they intersect.
    // Side effects: None.
    // Notes: Only worth calling once `near_box` has said something is close.
    pub fn min_distance_to(&self, shape: &TriMesh) -> f64 {
        query::distance(
            &Isometry::identity(),
            shape,
            &Isometry::identity(),
            &self.shape,
        )
        .unwrap_or(0.0)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Unsigned distance from a point to the void's surface.
    // Inputs: the point.
    // Returns: the distance to the nearest point of the void surface.
    // Side effects: None.
    // Notes: Used by the void-neighbourhood band check, which is about distance to the surface
    // rather than about being inside or outside.
    pub fn surface_distance(&self, p: Vec3) -> f64 {
        let point = Point::new(p.x, p.y, p.z);
        let projection =
            parry3d_f64::query::PointQuery::project_local_point(&self.shape, &point, false);
        (projection.point - point).norm()
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Say whether any of a mesh's vertices lies inside the void.
    // Inputs: the mesh.
    // Returns: true when at least one vertex is inside.
    // Side effects: None.
    // Notes: One half of the nesting test. A closed shell strictly inside another has *all* of its
    // vertices inside, so a vertex test cannot miss the case it exists for.
    pub fn any_vertex_inside(&self, mesh: &Mesh) -> bool {
        mesh.vertices.iter().any(|v| self.contains_point(*v))
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Say whether any of the void's own vertices lies inside a particle.
    // Inputs: the particle mesh and its bounding box.
    // Returns: true when at least one void vertex is inside the particle.
    // Side effects: None.
    // Notes: The other half of the nesting test: it catches a particle that has swallowed a whole
    // void shell, which no distance or vertex-of-the-particle test would see. Only void vertices
    // inside the particle's box are examined.
    pub fn any_void_vertex_inside(&self, particle: &Mesh, bbox: BoundingBox) -> bool {
        let padded = bbox.expanded(1e-9);
        self.mesh
            .vertices
            .iter()
            .filter(|v| padded.contains_point(**v))
            .any(|v| point_inside_mesh_local(particle, *v))
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The void's volume inside a domain, and which method produced it.
    // Inputs: the domain box.
    // Returns: (volume, method).
    // Side effects: None.
    // Notes: When the void lies inside the domain the shells' volumes sum exactly and no clipping
    // is needed. Otherwise the exact clip routine runs, which is cap-free and handles the
    // non-convex, multiply-connected cross-sections a real pore network produces. The method is
    // returned rather than assumed, because the report states it: a solid basis that fell back to a
    // different method silently would change the meaning of the target volume fraction.
    pub fn volume_in_domain(&self, domain: BoundingBox) -> (f64, VoidVolumeMethod) {
        let inside = self.bbox.min.x >= domain.min.x
            && self.bbox.min.y >= domain.min.y
            && self.bbox.min.z >= domain.min.z
            && self.bbox.max.x <= domain.max.x
            && self.bbox.max.y <= domain.max.y
            && self.bbox.max.z <= domain.max.z;
        if inside {
            return (self.total_volume(), VoidVolumeMethod::ExactShellSum);
        }
        let (v, _) = mesh_volume_in_bbox_exact(&self.mesh, domain);
        (v.abs(), VoidVolumeMethod::ExactClip)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Draw a point on the void surface, area-weighted, with its outward normal.
    // Inputs: the generator.
    // Returns: (point on the surface, unit normal pointing away from the void).
    // Side effects: Advances the generator by exactly three u64 draws.
    // Notes: For void_neighbourhood placement. Area-weighted so a finely tessellated region is not
    // over-sampled. The normal is flipped for an inward-wound void so it always points away from
    // the solid the void encloses, which is the direction a neighbouring particle sits in.
    // Exactly three draws whatever happens, so the consumption schedule does not depend on the
    // triangle that came up.
    pub fn sample_surface_point<R: RngCore + ?Sized>(&self, rng: &mut R) -> (Vec3, Vec3) {
        let pick = u01(rng) * self.total_area;
        let u = u01(rng);
        let v = u01(rng);
        let index = self
            .cumulative_area
            .iter()
            .position(|c| pick < *c)
            .unwrap_or(self.mesh.faces.len().saturating_sub(1));
        let f = &self.mesh.faces[index];
        let a = self.mesh.vertices[f.a];
        let b = self.mesh.vertices[f.b];
        let c = self.mesh.vertices[f.c];
        // Uniform on the triangle: fold the unit square onto it.
        let (u, v) = if u + v > 1.0 {
            (1.0 - u, 1.0 - v)
        } else {
            (u, v)
        };
        let point = a
            .add(b.sub(a).scale(u))
            .add(c.sub(a).scale(v));
        let raw = b.sub(a).cross(c.sub(a));
        let len = raw.dot(raw).sqrt();
        let normal = if len > 0.0 {
            raw.scale(if self.outward { 1.0 } else { -1.0 } / len)
        } else {
            Vec3::new(0.0, 0.0, 1.0)
        };
        (point, normal)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Volume of a particle that lies inside the void, by voxel count.
    // Inputs: the particle, its box, the domain, and the voxel size.
    // Returns: the overlapping volume.
    // Side effects: None.
    // Notes: Voxels are anchored to the DOMAIN origin, not to each particle's own box, so two
    // particles overlapping the same pore agree about the same physical voxel. Each voxel centre is
    // tested against the void first (the cheaper hierarchy query) and against the particle only
    // where the void already claims it. Deterministic: the tally is an integer count over a fixed
    // lattice, in a fixed order.
    pub fn overlap_volume(
        &self,
        particle: &Mesh,
        bbox: BoundingBox,
        domain: BoundingBox,
        voxel: f64,
    ) -> f64 {
        if voxel <= 0.0 {
            return 0.0;
        }
        let lo = |a: f64, o: f64| ((a - o) / voxel).floor() as i64;
        let hi = |a: f64, o: f64| ((a - o) / voxel).ceil() as i64;
        let (ix0, ix1) = (lo(bbox.min.x, domain.min.x), hi(bbox.max.x, domain.min.x));
        let (iy0, iy1) = (lo(bbox.min.y, domain.min.y), hi(bbox.max.y, domain.min.y));
        let (iz0, iz1) = (lo(bbox.min.z, domain.min.z), hi(bbox.max.z, domain.min.z));

        let mut count: u64 = 0;
        for ix in ix0..=ix1 {
            let x = domain.min.x + (ix as f64 + 0.5) * voxel;
            for iy in iy0..=iy1 {
                let y = domain.min.y + (iy as f64 + 0.5) * voxel;
                for iz in iz0..=iz1 {
                    let z = domain.min.z + (iz as f64 + 0.5) * voxel;
                    let p = Vec3::new(x, y, z);
                    if !bbox.expanded(1e-12).contains_point(p) {
                        continue;
                    }
                    if self.contains_point(p) && point_inside_mesh_local(particle, p) {
                        count += 1;
                    }
                }
            }
        }
        count as f64 * voxel * voxel * voxel
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Ray-parity point-in-mesh test for a small mesh with no hierarchy.
// Inputs: the mesh and the point.
// Returns: true when the point is inside.
// Side effects: None.
// Notes: Uses the same fixed ray and hit tolerance as VoidIndex::contains_point, so the two agree
// on a shared surface. A particle has a few hundred faces, so building a hierarchy for one query
// would cost more than the scan.
fn point_inside_mesh_local(mesh: &Mesh, p: Vec3) -> bool {
    let mut hits: Vec<f64> = Vec::new();
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        let tri = PTriangle::new(
            Point::new(a.x, a.y, a.z),
            Point::new(b.x, b.y, b.z),
            Point::new(c.x, c.y, c.z),
        );
        let ray = parry3d_f64::query::Ray::new(
            Point::new(p.x, p.y, p.z),
            parry3d_f64::math::Vector::new(RAY_DIR.x, RAY_DIR.y, RAY_DIR.z),
        );
        if let Some(toi) = tri.cast_local_ray(&ray, f64::MAX, false) {
            hits.push(toi);
        }
    }
    hits.sort_by(|a, b| a.total_cmp(b));
    let mut crossings = 0usize;
    let mut last = f64::NEG_INFINITY;
    for t in hits {
        if t >= 0.0 && (t - last).abs() > HIT_EPS {
            crossings += 1;
            last = t;
        }
    }
    crossings % 2 == 1
}

impl std::fmt::Debug for VoidIndex {
    // AI-FUNC-SUMMARY: Render the index's shape rather than its contents; returns fmt::Result; side effects: writes to the formatter.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoidIndex")
            .field("shells", &self.shell_volumes.len())
            .field("triangles", &self.mesh.faces.len())
            .field("outward", &self.outward)
            .field("volume", &self.total_volume())
            .finish()
    }
}
