use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ForgingParams {
    pub input_stl_path: String,
    pub output_stl_path: Option<String>,
    pub compression_ratio: Option<f64>,
    pub compression_axis: Option<String>,
    pub orient_to_positive_volume: Option<bool>,
    pub bulge_factor: Option<f64>,
    pub roi_bounding_box: Option<Vec<f64>>,
    pub mesh_type: Option<String>,
    pub void_densification: Option<f64>,
    /// Worker budget for the whole run: absent or -1 uses every available worker.
    pub cpu_max: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgingConfig {
    pub forging: ForgingParams,
}

