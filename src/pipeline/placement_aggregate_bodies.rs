//! Posed rigid bodies for contact growth: each member's query mesh and hierarchy are built
//! once in its own scaled frame and then queried through an isometry, so a rigid move never
//! rebuilds geometry. Distances are exact triangle-pair minima, optionally capped from above.
use super::*;
use parry3d_f64::bounding_volume::SimdAabb;
use parry3d_f64::math::{Isometry, Real, SimdReal, Vector, SIMD_WIDTH};
use parry3d_f64::na::{self, SimdValue};
use parry3d_f64::partitioning::{SimdSimultaneousVisitStatus, SimdSimultaneousVisitor};
use parry3d_f64::query;
use parry3d_f64::shape::TriMesh;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// One member geometry in its local scaled frame (centroid at the origin).
pub(super) struct Body {
    mesh: TriMesh,
    /// Convex-hull vertices: container envelopes are maxima of convex functions, so they are
    /// attained on these points and equal the all-vertex envelope.
    pub(super) hull: Vec<Vec3>,
    /// Principal axes of the vertex cloud, longest extent first.
    pub(super) axes: [Vec3; 3],
    /// True solid volume at this scale.
    volume: f64,
}

/// Cache keyed by shell and exact scale; never serialized, rebuilt on demand after resume.
#[derive(Clone, Default)]
pub(super) struct Bodies(HashMap<(usize, u64), Arc<Body>>);

/// Query counters for profiling; process-wide, reset by callers that report them.
pub(super) static DISTANCE_QUERIES: AtomicU64 = AtomicU64::new(0);
pub(super) static TRIANGLE_PAIRS: AtomicU64 = AtomicU64::new(0);

impl Bodies {
    // AI-FUNC-SUMMARY: Build missing local bodies for these members; returns error on unusable meshes; mutates only the cache.
    pub(super) fn ensure(&mut self, members: &[Member], library: &ShapeLibrary) -> Result<()> {
        for m in members {
            let key = (m.shell, m.scale.to_bits());
            if self.0.contains_key(&key) {
                continue;
            }
            let mesh = transform_shell(
                &library.shells[m.shell].canonical,
                m.scale,
                UnitQuat::identity(),
                Vec3::new(0., 0., 0.),
            );
            let shape = crate::geometry::to_parry_trimesh(&mesh)
                .ok_or_else(|| RustMsptError::InvalidMesh("cannot prepare aggregate member".into()))?;
            let points: Vec<na::Point3<f64>> = mesh
                .vertices
                .iter()
                .map(|v| na::Point3::new(v.x, v.y, v.z))
                .collect();
            let (hull_points, _) = parry3d_f64::transformation::convex_hull(&points);
            let hull = if hull_points.is_empty() {
                mesh.vertices.clone()
            } else {
                hull_points.iter().map(|p| Vec3::new(p.x, p.y, p.z)).collect()
            };
            self.0.insert(
                key,
                Arc::new(Body {
                    mesh: shape,
                    hull,
                    axes: principal_axes(&mesh.vertices),
                    volume: library.shells[m.shell].volume * m.scale.powi(3),
                }),
            );
        }
        Ok(())
    }

    // AI-FUNC-SUMMARY: Pose a member whose body was ensured; panics only on a caller bug (missing ensure).
    pub(super) fn pose(&self, m: &Member) -> Posed {
        let body = self.0[&(m.shell, m.scale.to_bits())].clone();
        let [w, x, y, z] = m.rotation.to_wxyz();
        let iso = Isometry::from_parts(
            na::Translation3::new(m.centre.x, m.centre.y, m.centre.z),
            na::UnitQuaternion::new_unchecked(na::Quaternion::new(w, x, y, z)),
        );
        Posed {
            body,
            iso,
            centre: m.centre,
            radius: m.radius,
        }
    }

    // AI-FUNC-SUMMARY: Origin-centred sphere/cube envelope from hull vertices; equals the all-vertex envelope; no mutation.
    pub(super) fn envelope(&self, members: &[Member], shape: AggregateShape) -> f64 {
        members
            .iter()
            .map(|m| {
                let body = &self.0[&(m.shell, m.scale.to_bits())];
                body.hull
                    .iter()
                    .map(|v| {
                        let p = m.rotation.rotate_point(*v).add(m.centre);
                        match shape {
                            AggregateShape::Sphere => vec_norm(p),
                            AggregateShape::Cube => p.x.abs().max(p.y.abs()).max(p.z.abs()),
                        }
                    })
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max)
    }

    // AI-FUNC-SUMMARY: Local convex-hull vertices of a member's body; no mutation.
    pub(super) fn hull(&self, m: &Member) -> &[Vec3] {
        &self.0[&(m.shell, m.scale.to_bits())].hull
    }

    // AI-FUNC-SUMMARY: True solid volume of a member; no mutation.
    pub(super) fn volume(&self, m: &Member) -> f64 {
        self.0[&(m.shell, m.scale.to_bits())].volume
    }

    // AI-FUNC-SUMMARY: Local principal axes for a member, longest first.
    pub(super) fn axes(&self, m: &Member) -> [Vec3; 3] {
        self.0[&(m.shell, m.scale.to_bits())].axes
    }
}

/// A body at a rigid pose; cheap to build.
#[derive(Clone)]
pub(super) struct Posed {
    body: Arc<Body>,
    iso: Isometry<Real>,
    centre: Vec3,
    radius: f64,
}

// AI-FUNC-SUMMARY: Eigenvectors of the vertex covariance, sorted by decreasing variance; deterministic; no mutation.
fn principal_axes(vertices: &[Vec3]) -> [Vec3; 3] {
    let n = vertices.len().max(1) as f64;
    let mut c = na::Matrix3::<f64>::zeros();
    for v in vertices {
        let p = na::Vector3::new(v.x, v.y, v.z);
        c += p * p.transpose() / n;
    }
    let e = na::SymmetricEigen::new(c);
    let mut order = [0usize, 1, 2];
    order.sort_by(|&a, &b| e.eigenvalues[b].total_cmp(&e.eigenvalues[a]));
    order.map(|i| {
        let v = e.eigenvectors.column(i);
        Vec3::new(v[0], v[1], v[2])
    })
}

// AI-FUNC-SUMMARY: min(exact surface distance, cap) between two posed bodies; 0 for intersecting surfaces. Nesting is not detected (callers move continuously from separated poses). No mutation besides profiling counters.
pub(super) fn distance_capped(a: &Posed, b: &Posed, cap: f64) -> f64 {
    if vec_norm(a.centre.sub(b.centre)) - a.radius - b.radius >= cap {
        return cap;
    }
    DISTANCE_QUERIES.fetch_add(1, Ordering::Relaxed);
    let pos12 = a.iso.inv_mul(&b.iso);
    let mut visitor = MinDistance {
        a: &a.body.mesh,
        b: &b.body.mesh,
        pos12,
        simd_pos12: Isometry::<SimdReal>::splat(pos12),
        best: cap,
        pairs: 0,
    };
    a.body
        .mesh
        .qbvh()
        .traverse_bvtt(b.body.mesh.qbvh(), &mut visitor);
    TRIANGLE_PAIRS.fetch_add(visitor.pairs, Ordering::Relaxed);
    visitor.best
}

/// Branch-and-bound minimum over triangle pairs. A node pair is kept while its boxes, the left
/// one inflated by the best distance so far on every axis (an L-infinity test that keeps every
/// Euclidean-nearer pair), overlap the right one transformed into the left frame.
struct MinDistance<'m> {
    a: &'m TriMesh,
    b: &'m TriMesh,
    pos12: Isometry<Real>,
    simd_pos12: Isometry<SimdReal>,
    best: f64,
    pairs: u64,
}

impl SimdSimultaneousVisitor<u32, u32, SimdAabb> for MinDistance<'_> {
    fn visit(
        &mut self,
        left_bv: &SimdAabb,
        left_data: Option<[Option<&u32>; SIMD_WIDTH]>,
        right_bv: &SimdAabb,
        right_data: Option<[Option<&u32>; SIMD_WIDTH]>,
    ) -> SimdSimultaneousVisitStatus {
        let inflate = Vector::<SimdReal>::splat(Vector::<Real>::repeat(self.best));
        let loose = SimdAabb {
            mins: left_bv.mins - inflate,
            maxs: left_bv.maxs + inflate,
        };
        let right = right_bv.transform_by(&self.simd_pos12);
        let mask = loose.intersects_permutations(&right);
        if let (Some(data1), Some(data2)) = (left_data, right_data) {
            let identity = Isometry::identity();
            for (ii, face1) in data1.into_iter().enumerate() {
                let Some(face1) = face1 else { continue };
                let t1 = self.a.triangle(*face1);
                for (jj, face2) in data2.into_iter().enumerate() {
                    let Some(face2) = face2 else { continue };
                    if !mask[ii].extract(jj) {
                        continue;
                    }
                    self.pairs += 1;
                    let t2 = self.b.triangle(*face2);
                    let d = query::distance(&identity, &t1, &self.pos12, &t2).unwrap_or(0.0);
                    if d < self.best {
                        self.best = d;
                        if d <= 0.0 {
                            return SimdSimultaneousVisitStatus::ExitEarly;
                        }
                    }
                }
            }
        }
        SimdSimultaneousVisitStatus::MaybeContinue(mask)
    }
}

// AI-FUNC-SUMMARY: Smallest capped surface distance from one posed body to a set; returns cap when all are farther; no mutation.
pub(super) fn clearance_capped(p: &Posed, obstacles: &[Posed], cap: f64) -> f64 {
    let mut best = cap;
    for other in obstacles {
        best = best.min(distance_capped(p, other, best));
        if best <= 0.0 {
            break;
        }
    }
    best
}
