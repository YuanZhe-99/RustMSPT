use crate::pipeline::rng::u01;
use crate::types::{Mesh, Vec3};
use rand_chacha::rand_core::RngCore;
use serde::{Deserialize, Serialize};

/// A unit quaternion, stored scalar-first.
///
/// The component order is `[w, x, y, z]` and is part of the placement record's
/// published contract. `nalgebra` stores its quaternions `[i, j, k, w]`; the two
/// orders are not interchangeable, and a reader that assumes the wrong one gets a
/// plausible but wrong orientation rather than an error.
///
/// Sampled and recorded quaternions are canonicalised to `w >= 0`. `q` and `-q`
/// name the same rotation, so without that convention two records of the same
/// placement could differ component-wise while meaning the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UnitQuat {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl UnitQuat {
    // AI-FUNC-SUMMARY: The identity rotation; returns UnitQuat {1,0,0,0}; side effects: none.
    pub fn identity() -> Self {
        UnitQuat {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Build a unit quaternion from raw components, normalizing and canonicalizing the sign.
    // Inputs: the four components in w, x, y, z order.
    // Returns: Some(unit quaternion with w >= 0), or None when the norm is not finite and positive.
    // Side effects: None.
    // Notes: The w >= 0 canonicalization is what makes a recorded quaternion a function of the placement
    // rather than of which of the two equivalent representations happened to be produced.
    pub fn new(w: f64, x: f64, y: f64, z: f64) -> Option<Self> {
        let norm = (w * w + x * x + y * y + z * z).sqrt();
        if !norm.is_finite() || norm <= 0.0 {
            return None;
        }
        let sign = if w < 0.0 { -1.0 } else { 1.0 };
        let inv = sign / norm;
        Some(UnitQuat {
            w: w * inv,
            x: x * inv,
            y: y * inv,
            z: z * inv,
        })
    }

    // AI-FUNC-SUMMARY: The four components in the record's published w,x,y,z order; returns [f64; 4]; side effects: none.
    pub fn to_wxyz(self) -> [f64; 4] {
        [self.w, self.x, self.y, self.z]
    }

    // AI-FUNC-SUMMARY: Euclidean norm of the four components, for verifying unitness; returns f64; side effects: none.
    pub fn norm(self) -> f64 {
        (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Rotate a point about the origin by this quaternion.
    // Inputs: the point.
    // Returns: the rotated point.
    // Side effects: None.
    // Notes: Uses the standard v + 2w(q x v) + 2(q x (q x v)) form, which needs no matrix and no
    // quaternion multiplication. About the origin only: the placement transform's pivot is applied
    // by the caller, which is why the canonical shell copy is centred first.
    pub fn rotate_point(self, p: Vec3) -> Vec3 {
        let u = Vec3::new(self.x, self.y, self.z);
        let t = u.cross(p).scale(2.0);
        p.add(t.scale(self.w)).add(u.cross(t))
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Render this rotation as a row-major 3x3 matrix.
    // Inputs: self.
    // Returns: [[f64; 3]; 3] with row i, column j at [i][j].
    // Side effects: None.
    // Notes: The record stores this beside the quaternion because a matrix is what a reader wants to
    // look at, but the quaternion is authoritative: a reader that finds the two disagreeing should
    // refuse the record rather than pick one.
    pub fn to_matrix(self) -> [[f64; 3]; 3] {
        let (w, x, y, z) = (self.w, self.x, self.y, self.z);
        [
            [
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y - w * z),
                2.0 * (x * z + w * y),
            ],
            [
                2.0 * (x * y + w * z),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z - w * x),
            ],
            [
                2.0 * (x * z - w * y),
                2.0 * (y * z + w * x),
                1.0 - 2.0 * (x * x + y * y),
            ],
        ]
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Draw a rotation uniformly at random from SO(3).
// Inputs: the generator.
// Returns: a unit quaternion with w >= 0.
// Side effects: Advances the generator by exactly three u64 draws.
// Notes: Shoemake's method (Ken Shoemake, "Uniform Random Rotations", Graphics Gems III, 1992):
// from u1, u2, u3 uniform on [0,1), the quaternion
//   (sqrt(1-u1) sin 2*pi*u2, sqrt(1-u1) cos 2*pi*u2, sqrt(u1) sin 2*pi*u3, sqrt(u1) cos 2*pi*u3)
// is uniform on the unit 3-sphere, hence Haar-uniform on SO(3) through the double cover. This is
// NOT what pipeline/rotation.rs's RotationMode::Any does - that samples an axis in a cube and an
// angle uniformly, which is not uniform on rotations. Exactly three draws, always, so the RNG
// consumption schedule does not depend on any downstream check.
pub fn sample_uniform_quaternion<R: RngCore + ?Sized>(rng: &mut R) -> UnitQuat {
    let u1 = u01(rng);
    let u2 = u01(rng);
    let u3 = u01(rng);
    let r1 = (1.0 - u1).max(0.0).sqrt();
    let r2 = u1.max(0.0).sqrt();
    let t1 = std::f64::consts::TAU * u2;
    let t2 = std::f64::consts::TAU * u3;
    UnitQuat::new(r2 * t2.cos(), r1 * t1.sin(), r1 * t1.cos(), r2 * t2.sin())
        .unwrap_or_else(UnitQuat::identity)
}

// AI-FUNC-SUMMARY:
// Purpose: Place a canonical (centroid-at-origin) shell into the run frame.
// Inputs: the canonical mesh, the isotropic scale, the rotation, the target centroid.
// Returns: a new mesh with the same face list and transformed vertices.
// Side effects: None.
// Notes: THE single definition of the placement transform,
//   p_world = R(q) * (scale * p_canonical) + translation,
// which for a shell whose centroid is already at the origin is exactly the record's published
// contract p_world = R(q) * (scale * (p_source - shell_centroid)) + translation. Both the engine
// and the reconstruction test call this function, so the test cannot drift from the engine; a
// second, independent reconstruction from the recorded quaternion is what checks the contract
// itself. Face indices are preserved, so a placed particle's triangle range maps one-to-one onto
// the source shell's faces.
pub fn transform_shell(canonical: &Mesh, scale: f64, rotation: UnitQuat, translation: Vec3) -> Mesh {
    let vertices = canonical
        .vertices
        .iter()
        .map(|v| rotation.rotate_point(v.scale(scale)).add(translation))
        .collect();
    Mesh {
        vertices,
        faces: canonical.faces.clone(),
    }
}
