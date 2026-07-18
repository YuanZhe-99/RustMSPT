use super::deserialize::deserialize_option_i32_flexible;
use super::AccelerationConfig;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct RenderParams {
    pub stl_path: String,
    #[serde(default = "default_output_path")]
    pub output_path: String,
    pub focus_point: Vec<f64>,
    pub view_direction: Vec<f64>,
    #[serde(default)]
    pub up_vector: Option<Vec<f64>>,
    #[serde(default = "default_projection")]
    pub projection: String,
    #[serde(default = "default_fov_degrees")]
    pub perspective_fov_degrees: f64,
    #[serde(default)]
    pub camera_distance: Option<f64>,
    #[serde(default = "default_fit_padding")]
    pub fit_padding: f64,
    #[serde(default = "default_resolution")]
    pub width: usize,
    #[serde(default = "default_resolution")]
    pub height: usize,
    #[serde(default, deserialize_with = "deserialize_option_i32_flexible")]
    pub cpu_max: Option<i32>,
    #[serde(default)]
    pub acceleration: AccelerationConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RenderConfig {
    pub render: RenderParams,
}

// AI-FUNC-SUMMARY: Return the default render output path; returns data/output/rendered.png; side effects: None.
fn default_output_path() -> String {
    "data/output/rendered.png".to_string()
}

// AI-FUNC-SUMMARY: Return the default render projection name; returns "orthographic"; side effects: None.
fn default_projection() -> String {
    "orthographic".to_string()
}

// AI-FUNC-SUMMARY: Return the default perspective vertical FOV; returns 45 degrees; side effects: None.
fn default_fov_degrees() -> f64 {
    45.0
}

// AI-FUNC-SUMMARY: Return the default auto-framing padding fraction; returns 0.05; side effects: None.
fn default_fit_padding() -> f64 {
    0.05
}

// AI-FUNC-SUMMARY: Return the default image width or height; returns 1024 pixels; side effects: None.
fn default_resolution() -> usize {
    1024
}
