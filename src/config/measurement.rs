use serde::Deserialize;
use super::deserialize::{deserialize_option_i32_flexible, deserialize_option_usize_flexible};
use super::AccelerationConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct MeasurementParams {
    pub stl_path: String,
    pub bounding_box: Option<Vec<f64>>,
    pub stl_bounding_box: Option<Vec<f64>>,
    pub r_max: usize,
    pub voxel_pitch: f64,
    pub mc_method: String,
    #[serde(default, deserialize_with = "deserialize_option_usize_flexible")]
    pub mc_samples: Option<usize>,
    #[serde(default, deserialize_with = "deserialize_option_i32_flexible")]
    pub cpu_max: Option<i32>,
    pub output_path: String,
    #[serde(default)]
    pub acceleration: AccelerationConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MeasurementConfig {
    pub measurement: MeasurementParams,
}

