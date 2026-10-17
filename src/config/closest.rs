use crate::config::AccelerationConfig;
use serde::Deserialize;

// AI-FUNC-SUMMARY:
// Purpose: YAML parameters for the closest subcommand: the nearest point on a set of triangles or segments for every query point.
// Notes: Arrays are .npy files (C order, little endian). `vertices` (n, 3) f64; exactly one of `triangles` (m, 3) or
//   `segments` (m, 2) integer corner indices; `queries` (..., 3) f64. Outputs `primitive.npy` (int64, -1 when there are no
//   primitives), `closest.npy` (..., 3) f64 and `distance.npy` (...) f64, shaped as the query's leading axes, and
//   `report.json`, under `output_dir`.
#[derive(Debug, Clone, Deserialize)]
pub struct ClosestParams {
    pub vertices: String,
    #[serde(default)]
    pub triangles: Option<String>,
    #[serde(default)]
    pub segments: Option<String>,
    pub queries: String,
    pub output_dir: String,
    #[serde(default)]
    pub cpu_max: Option<i32>,
}

// AI-FUNC-SUMMARY: Top-level YAML document for closest: the `closest` block and an optional `acceleration` block; carries no behaviour.
#[derive(Debug, Clone, Deserialize)]
pub struct ClosestConfig {
    pub closest: ClosestParams,
    #[serde(default)]
    pub acceleration: AccelerationConfig,
}
