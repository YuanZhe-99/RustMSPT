use crate::geometry::{mesh_bbox, point_inside_mesh, PreparedMeshQuery, MeshQueryScratch};
use crate::types::{BoundingBox, Mesh, Vec3};

const PREPARED_QUERY_MIN: usize = 16;

// AI-FUNC-SUMMARY: Cumulative f32 certification counters of one GPU pipeline: predicate queries, queries the shader could not prove and the CPU re-evaluated, uncertain-list regrow re-dispatches and host recompute time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuCertificationStats {
    pub queries: u64,
    pub uncertain: u64,
    pub list_regrowths: u64,
    pub cpu_recompute_seconds: f64,
}

impl GpuCertificationStats {
    // AI-FUNC-SUMMARY: Fraction of queries recomputed on the CPU (0 when no query ran); returns f64; side effects: None.
    pub fn recompute_ratio(&self) -> f64 {
        if self.queries == 0 {
            0.0
        } else {
            self.uncertain as f64 / self.queries as f64
        }
    }

    // AI-FUNC-SUMMARY: One-line log description of the counters and ratio; returns String; side effects: None.
    pub fn describe(&self) -> String {
        format!(
            "f32 certification: {} of {} queries recomputed on CPU (ratio {:.3e}), list_regrowths={}, cpu_recompute={:.6}s",
            self.uncertain,
            self.queries,
            self.recompute_ratio(),
            self.list_regrowths,
            self.cpu_recompute_seconds
        )
    }

    // AI-FUNC-SUMMARY: Add another pipeline's counters into this one; returns None; side effects: mutates self.
    pub fn accumulate(&mut self, other: &GpuCertificationStats) {
        self.queries += other.queries;
        self.uncertain += other.uncertain;
        self.list_regrowths += other.list_regrowths;
        self.cpu_recompute_seconds += other.cpu_recompute_seconds;
    }
}

// AI-FUNC-SUMMARY: Smallest f32 not below an f64 value (infinite values map to f32 infinities); returns f32; side effects: None.
pub(crate) fn f32_at_least(x: f64) -> f32 {
    let f = x as f32;
    if (f as f64) >= x || f.is_nan() {
        f
    } else {
        f.next_up()
    }
}

// AI-FUNC-SUMMARY: Largest f32 not above an f64 value; returns f32; side effects: None.
pub(crate) fn f32_at_most(x: f64) -> f32 {
    let f = x as f32;
    if (f as f64) <= x || f.is_nan() {
        f
    } else {
        f.next_down()
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Host-side CPU f64 reference for certified GPU ray parity: the mesh shifted by the GPU origin in f64, and the exact f32 form of the CPU bbox early-out.
// Inputs: mesh and the origin the GPU triangle buffer subtracts before f32 conversion.
// Returns: CertReference.
// Side effects: None.
// Notes: For any f32 point p, `p < mesh_lo` equals the CPU's f64 test `p < bb.min - 1e-9` and `p > mesh_hi` equals `p > bb.max + 1e-9`,
// because mesh_lo/mesh_hi are the f64 thresholds rounded up/down to f32. An empty mesh yields +inf/-inf so every point is outside, as on the CPU.
pub(crate) struct CertReference {
    mesh: Mesh,
    pub mesh_lo: [f32; 3],
    pub mesh_hi: [f32; 3],
}

impl CertReference {
    // AI-FUNC-SUMMARY: Build the origin-shifted f64 reference mesh and exact f32 early-out bounds; returns CertReference; side effects: None.
    pub fn new(mesh: &Mesh, origin: Vec3) -> Self {
        let shifted = Mesh {
            vertices: mesh.vertices.iter().map(|v| v.sub(origin)).collect(),
            faces: mesh.faces.clone(),
        };
        let (mesh_lo, mesh_hi) = match mesh_bbox(&shifted) {
            Some(BoundingBox { min, max }) => {
                let eps = 1e-9;
                (
                    [
                        f32_at_least(min.x - eps),
                        f32_at_least(min.y - eps),
                        f32_at_least(min.z - eps),
                    ],
                    [
                        f32_at_most(max.x + eps),
                        f32_at_most(max.y + eps),
                        f32_at_most(max.z + eps),
                    ],
                )
            }
            None => ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
        };
        Self {
            mesh: shifted,
            mesh_lo,
            mesh_hi,
        }
    }

    // AI-FUNC-SUMMARY: The origin-shifted f64 mesh the certified GPU result must equal; returns a borrow; side effects: None.
    #[cfg(test)]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    // AI-FUNC-SUMMARY: Serialize mesh_lo, flags and mesh_hi as the 32-byte WGSL tail (vec3, u32, vec3, pad); returns bytes; side effects: None.
    pub fn params_tail(&self, flags: u32) -> Vec<u8> {
        let words = [
            self.mesh_lo[0].to_bits(),
            self.mesh_lo[1].to_bits(),
            self.mesh_lo[2].to_bits(),
            flags,
            self.mesh_hi[0].to_bits(),
            self.mesh_hi[1].to_bits(),
            self.mesh_hi[2].to_bits(),
            0,
        ];
        words.into_iter().flat_map(u32::to_le_bytes).collect()
    }

    // AI-FUNC-SUMMARY: Classify exact f32 query points with the unchanged CPU f64 predicate in the shifted frame, serially (callers may hold locks); uses a prepared BVH query for larger batches; returns one bool per point.
    pub fn classify(&self, points: &[[f32; 3]]) -> Vec<bool> {
        let to_vec = |p: &[f32; 3]| Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
        if points.len() < PREPARED_QUERY_MIN {
            return points
                .iter()
                .map(|p| point_inside_mesh(&self.mesh, to_vec(p)))
                .collect();
        }
        let query = PreparedMeshQuery::new(&self.mesh);
        let mut scratch = MeshQueryScratch::default();
        points
            .iter()
            .map(|p| query.contains_point(to_vec(p), &mut scratch))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AI-FUNC-SUMMARY: Verify directed f32 rounding brackets f64 thresholds so f32 comparisons reproduce the CPU bbox early-out exactly, including empty meshes.
    #[test]
    fn directed_rounding_reproduces_f64_comparisons() {
        for x in [0.1f64, -0.1, 1e-9, -1e-9, 3.0, 1e20, -7.25] {
            let lo = f32_at_least(x);
            let hi = f32_at_most(x);
            assert!(lo as f64 >= x && hi as f64 <= x);
            assert!((lo.next_down() as f64) < x && (hi.next_up() as f64) > x || lo == hi);
            for p in [lo.next_down(), lo, lo.next_up(), hi.next_down(), hi, hi.next_up()] {
                assert_eq!(p < lo, (p as f64) < x);
                assert_eq!(p > hi, (p as f64) > x);
            }
        }
        let empty = CertReference::new(&Mesh::empty(), Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(empty.mesh_lo, [f32::INFINITY; 3]);
        let stats = GpuCertificationStats {
            queries: 8,
            uncertain: 2,
            ..Default::default()
        };
        assert_eq!(stats.recompute_ratio(), 0.25);
        assert_eq!(GpuCertificationStats::default().recompute_ratio(), 0.0);
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::geometry::{box_mesh, icosphere_mesh, merge_meshes, translate_mesh};
    use crate::geometry::s2::RAY_DIR_GPU;
    use crate::types::{BoundingBox, Mesh, Triangle, Vec3};

    // AI-FUNC-SUMMARY: Closed octahedron with vertices c +/- r along each axis; returns Mesh; side effects: None.
    pub fn octahedron(c: Vec3, r: f64) -> Mesh {
        let vertices = vec![
            Vec3::new(c.x + r, c.y, c.z),
            Vec3::new(c.x - r, c.y, c.z),
            Vec3::new(c.x, c.y + r, c.z),
            Vec3::new(c.x, c.y - r, c.z),
            Vec3::new(c.x, c.y, c.z + r),
            Vec3::new(c.x, c.y, c.z - r),
        ];
        let faces = [
            [0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4],
            [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5],
        ]
        .iter()
        .map(|&[a, b, c]| Triangle { a, b, c })
        .collect();
        Mesh { vertices, faces }
    }

    // AI-FUNC-SUMMARY: Axis box from explicit corners; returns Mesh; side effects: None.
    fn cuboid(min: [f64; 3], max: [f64; 3]) -> Mesh {
        box_mesh(BoundingBox {
            min: Vec3::new(min[0], min[1], min[2]),
            max: Vec3::new(max[0], max[1], max[2]),
        })
    }

    // AI-FUNC-SUMMARY: Voxel-center point (f32 arithmetic promoted to f64) of cell (i, j, k) at the given pitch; returns Vec3; side effects: None.
    pub fn center(i: u32, j: u32, k: u32, pitch: f32) -> Vec3 {
        let c = |n: u32| ((n as f32 + 0.5) * pitch) as f64;
        Vec3::new(c(i), c(j), c(k))
    }

    // AI-FUNC-SUMMARY: Adversarial and ordinary meshes inside the [0,4]^3 domain: faces on or within ulps of voxel-center planes, shared faces, ray-through-vertex and ray-through-edge octahedra, a sub-1e-6 slab, coincident duplicate shells, tiny features and an ordinary sphere; returns (name, mesh) pairs; side effects: None.
    pub fn adversarial_meshes(pitch: f32) -> Vec<(&'static str, Mesh)> {
        let d = Vec3::new(RAY_DIR_GPU.0, RAY_DIR_GPU.1, RAY_DIR_GPU.2);
        let along = |p: Vec3, t: f64| Vec3::new(p.x + t * d.x, p.y + t * d.y, p.z + t * d.z);
        let r = 0.3;
        let vertex_on_ray = merge_meshes(
            &[(3, 4, 5), (8, 8, 8), (10, 2, 7)]
                .iter()
                .map(|&(i, j, k)| {
                    let v = along(center(i, j, k, pitch), 0.7);
                    octahedron(Vec3::new(v.x - r, v.y, v.z), r)
                })
                .collect::<Vec<_>>(),
        );
        let edge_on_ray = merge_meshes(
            &[(4, 6, 3), (9, 9, 9), (2, 11, 6)]
                .iter()
                .map(|&(i, j, k)| {
                    let v = along(center(i, j, k, pitch), 0.6);
                    octahedron(Vec3::new(v.x - r / 2.0, v.y - r / 2.0, v.z), r)
                })
                .collect::<Vec<_>>(),
        );
        let duplicated = cuboid([0.6, 0.6, 0.6], [3.1, 3.1, 3.1]);
        vec![
            ("faces_on_centers", cuboid([0.125, 0.125, 0.125], [2.125, 2.375, 1.875])),
            (
                "faces_ulp_off",
                cuboid(
                    [0.125 + 1e-9, 0.125 - 2e-8, 0.375 + 6e-8],
                    [2.125 - 1e-9, 2.375 + 2e-8, 1.875 - 6e-8],
                ),
            ),
            (
                "shared_faces",
                merge_meshes(&[
                    cuboid([0.3, 0.3, 0.3], [1.375, 2.0, 2.0]),
                    cuboid([1.375, 0.3, 0.3], [2.3, 2.0, 2.0]),
                    cuboid([2.3, 0.3, 0.3], [3.3, 2.0, 2.0]),
                ]),
            ),
            ("vertex_on_ray", vertex_on_ray),
            ("edge_on_ray", edge_on_ray),
            ("thin_slab", cuboid([1.0, 0.3, 0.3], [1.0 + 3e-7, 3.7, 3.7])),
            (
                "coincident_duplicates",
                merge_meshes(&[duplicated.clone(), duplicated.clone(), duplicated]),
            ),
            (
                "tiny_features",
                merge_meshes(&[
                    cuboid([2.0, 2.0, 2.0], [2.001, 2.001, 2.001]),
                    cuboid([1.125, 1.125, 1.125], [1.125 + 1e-4, 1.375, 1.375]),
                ]),
            ),
            ("icosphere", icosphere_mesh(Vec3::new(2.0, 2.0, 2.0), 1.3, 2)),
        ]
    }

    // AI-FUNC-SUMMARY: Translate a mesh and its [0,4]^3 domain by the same large offset; returns (mesh, bbox); side effects: None.
    pub fn shifted(mesh: &Mesh, shift: f64) -> (Mesh, BoundingBox) {
        let mut mesh = mesh.clone();
        let delta = Vec3::new(shift, -shift, shift);
        translate_mesh(&mut mesh, delta);
        let bbox = BoundingBox {
            min: delta,
            max: Vec3::new(delta.x + 4.0, delta.y + 4.0, delta.z + 4.0),
        };
        (mesh, bbox)
    }
}
