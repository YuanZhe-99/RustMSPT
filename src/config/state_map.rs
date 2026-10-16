use crate::config::AccelerationConfig;
use serde::Deserialize;

// AI-FUNC-SUMMARY:
// Purpose: YAML parameters for the state-map subcommand: where a remesh's sampling points sit in the last frame's mesh.
// Notes: Arrays are .npy files (C order, little endian). `source_points` (n, 3) f64 and `source_cells` (m, 4) integer are the
//   deformed source mesh; `query_points` (..., 3) f64 are the target's sampling points; `fallback_distance` is in the mesh's
//   own unit. Outputs `donors.npy` (int64, -1 for none) and `distances.npy` (f64, NaN for none), shaped as the query's leading
//   axes, and `report.json`, under `output_dir`.
#[derive(Debug, Clone, Deserialize)]
pub struct StateMapParams {
    pub source_points: String,
    pub source_cells: String,
    pub query_points: String,
    pub fallback_distance: f64,
    pub output_dir: String,
    #[serde(default = "default_gpu_slots")]
    pub gpu_slots: usize,
    #[serde(default)]
    pub cpu_max: Option<i32>,
}

// AI-FUNC-SUMMARY: Default number of candidate slots a GPU pass keeps per point; returns 24; side effects: none.
fn default_gpu_slots() -> usize {
    24
}

// AI-FUNC-SUMMARY: Top-level YAML document for state-map: the `state_map` block and an optional `acceleration` block; carries no behaviour.
#[derive(Debug, Clone, Deserialize)]
pub struct StateMapConfig {
    pub state_map: StateMapParams,
    #[serde(default)]
    pub acceleration: AccelerationConfig,
}
