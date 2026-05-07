use crate::error::{Result, RustMsptError};
use crate::types::Vec3;
use rand::Rng;

#[derive(Clone)]
pub enum RotationMode {
    None,
    Axis(Vec3),
    Any,
}

pub fn parse_rotation_mode(prefix: &str, mode: Option<&str>, axis_vec: Option<&Vec<f64>>) -> Result<RotationMode> {
    let raw = mode.unwrap_or("any").trim().to_ascii_lowercase();
    match raw.as_str() {
        "none" => Ok(RotationMode::None),
        "x" => Ok(RotationMode::Axis(Vec3::new(1.0, 0.0, 0.0))),
        "y" => Ok(RotationMode::Axis(Vec3::new(0.0, 1.0, 0.0))),
        "z" => Ok(RotationMode::Axis(Vec3::new(0.0, 0.0, 1.0))),
        "vector" => {
            let v = axis_vec.ok_or_else(|| {
                RustMsptError::InvalidConfig(format!(
                    "{prefix}.rotation_axis_vector is required when rotation_mode='vector'"
                ))
            })?;
            if v.len() != 3 {
                return Err(RustMsptError::InvalidConfig(format!(
                    "{prefix}.rotation_axis_vector must have length 3"
                )));
            }
            let axis = Vec3::new(v[0], v[1], v[2]);
            if crate::geometry::vec_norm(axis) <= 1e-12 {
                return Err(RustMsptError::InvalidConfig(format!(
                    "{prefix}.rotation_axis_vector must be non-zero"
                )));
            }
            Ok(RotationMode::Axis(axis))
        }
        "any" => Ok(RotationMode::Any),
        other => Err(RustMsptError::InvalidConfig(format!(
            "{prefix}.rotation_mode must be one of: none, x, y, z, vector, any (got '{other}')"
        ))),
    }
}

pub fn sample_rotation_axis(rng: &mut rand::rngs::ThreadRng, mode: &RotationMode) -> Option<Vec3> {
    match mode {
        RotationMode::None => None,
        RotationMode::Axis(axis) => Some(*axis),
        RotationMode::Any => Some(Vec3::new(
            rng.gen_range(-1.0..1.0),
            rng.gen_range(-1.0..1.0),
            rng.gen_range(-1.0..1.0),
        )),
    }
}
