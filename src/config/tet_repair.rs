use serde::Deserialize;

// AI-FUNC-SUMMARY:
// Purpose: YAML parameters for the tet-repair subcommand: repair inverted cells and cells over an aspect-ratio limit by flips and node moves.
// Notes: Arrays are .npy files (C order, little endian). `points` (n, 3) f64; `tets` (m, 4), `density` (m) and `fixed` (n, 0/1)
//   non-negative integers; optional `layer` (k, 3) node triples (a cohesive layer's faces, along which a fixed node may slide).
//   Outputs `points.npy`, `tets.npy` (int64), `density.npy` (int64) and `report.json` under `output_dir`.
#[derive(Debug, Clone, Deserialize)]
pub struct TetRepairParams {
    pub points: String,
    pub tets: String,
    pub density: String,
    pub fixed: String,
    #[serde(default)]
    pub layer: Option<String>,
    pub output_dir: String,
    #[serde(default = "default_limit")]
    pub limit: f64,
    #[serde(default = "default_sweeps")]
    pub sweeps: usize,
    #[serde(default)]
    pub wall_tol: f64,
    #[serde(default)]
    pub padding: Option<i64>,
    #[serde(default)]
    pub joint_tol: Option<f64>,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub cpu_max: Option<i32>,
}

// AI-FUNC-SUMMARY: Default aspect-ratio limit; returns 10; side effects: none.
fn default_limit() -> f64 {
    10.0
}

// AI-FUNC-SUMMARY: Default number of flip sweeps per pass; returns 20; side effects: none.
fn default_sweeps() -> usize {
    20
}

// AI-FUNC-SUMMARY: Top-level YAML document for tet-repair: the `tet_repair` block; carries no behaviour.
#[derive(Debug, Clone, Deserialize)]
pub struct TetRepairConfig {
    pub tet_repair: TetRepairParams,
}
