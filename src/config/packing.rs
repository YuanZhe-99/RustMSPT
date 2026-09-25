use serde::Deserialize;
use super::deserialize::{deserialize_option_i32_flexible};
use super::{BoxConfig, InputPath, OutputPath};

#[derive(Debug, Clone, Deserialize)]
pub struct PackingFilters {
    pub min_volume: Option<f64>,
    pub max_aspect_ratio: Option<f64>,
    pub max_sharpness_ratio: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackingParams {
    /// Optional seed: the same seed, inputs and binary give the same packing on any worker count (the parallel
    /// collision scan returns a boolean, never an order). Absent, randomness comes from the thread-local generator.
    #[serde(default)]
    pub seed: Option<u64>,
    pub target_volume_fraction: f64,
    pub mode: u8,
    pub max_attempts: usize,
    pub min_neighbor_distance: Option<f64>,
    pub min_boundary_dist: Option<f64>,
    pub min_cross_boundary_depth: Option<f64>,
    pub filters: Option<PackingFilters>,
    #[serde(default, deserialize_with = "deserialize_option_i32_flexible")]
    pub cpu_max: Option<i32>,
    pub orient_to_positive_volume: Option<bool>,
    pub rotation_mode: Option<String>,
    pub rotation_axis_vector: Option<Vec<f64>>,
    pub target_diameter_distribution_csv: Option<String>,
    pub target_mean_sphericity: Option<f64>,
    pub mean_sphericity_tolerance: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackingConfig {
    pub input: InputPath,
    pub output: OutputPath,
    pub r#box: BoxConfig,
    pub packing: PackingParams,
}
