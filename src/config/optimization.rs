use serde::Deserialize;
use super::deserialize::{deserialize_usize_flexible, deserialize_option_usize_flexible, deserialize_option_i32_flexible};
use super::{BoxConfig, InputStl, OutputPath, AccelerationConfig};

#[derive(Debug, Clone, Deserialize)]
pub struct TargetConfig {
    pub r#type: String,
    pub s2_array: Option<Vec<f64>>,
    pub stl_path: Option<String>,
    pub stl_bounding_box: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizationParams {
    /// Optional seed. With one island a seeded run is reproducible on any worker count: moves, pruning and every
    /// Monte Carlo S2 evaluation draw from streams fixed by it. Several islands migrate on thread timing, so their
    /// runs are not. Absent, randomness is drawn from the thread-local generator as before.
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(deserialize_with = "deserialize_usize_flexible")]
    pub max_iterations: usize,
    pub initial_temperature: f64,
    pub cooling_rate: f64,
    #[serde(default, deserialize_with = "deserialize_option_usize_flexible")]
    pub adaptive_temp_window: Option<usize>,
    pub target_acceptance_low: Option<f64>,
    pub target_acceptance_high: Option<f64>,
    pub adaptive_heat_factor: Option<f64>,
    pub adaptive_cool_factor: Option<f64>,
    pub adaptive_temp_ceiling_factor: Option<f64>,
    pub r_max: usize,
    pub voxel_pitch: f64,
    pub mc_method: String,
    #[serde(deserialize_with = "deserialize_usize_flexible")]
    pub mc_samples: usize,
    pub max_translation: f64,
    pub max_rotation_deg: f64,
    pub min_neighbor_distance: Option<f64>,
    pub mode: Option<u8>,
    pub min_boundary_dist: Option<f64>,
    pub min_cross_boundary_depth: Option<f64>,
    pub prune_enabled: Option<bool>,
    pub prune_tolerance: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_option_usize_flexible")]
    pub prune_max_rounds: Option<usize>,
    #[serde(default, deserialize_with = "deserialize_option_usize_flexible")]
    pub prune_eval_samples: Option<usize>,
    #[serde(default, deserialize_with = "deserialize_option_i32_flexible")]
    pub cpu_max: Option<i32>,
    pub orient_to_positive_volume: Option<bool>,
    pub rotation_mode: Option<String>,
    pub rotation_axis_vector: Option<Vec<f64>>,
    #[serde(default, deserialize_with = "deserialize_option_usize_flexible")]
    pub islands: Option<usize>,
    #[serde(default, deserialize_with = "deserialize_option_usize_flexible")]
    pub migration_interval: Option<usize>,
    #[serde(default)]
    pub acceleration: AccelerationConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizationConfig {
    pub input: InputStl,
    pub output: OutputPath,
    pub target: TargetConfig,
    pub r#box: BoxConfig,
    pub optimization: OptimizationParams,
}

