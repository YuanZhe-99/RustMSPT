use serde::Deserialize;
use super::deserialize::{deserialize_option_usize_flexible};
use super::InputPath;

#[derive(Debug, Clone, Deserialize)]
pub struct SplitFilterOutput {
    pub folder: String,
    pub prefix: String,
    pub report_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SplitFilterVolume {
    pub mode: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_option_usize_flexible")]
    pub bins: Option<usize>,
    pub over_factor: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SplitFilterRules {
    pub enabled: Option<bool>,
    pub max_aspect_ratio: Option<f64>,
    pub max_sharpness_ratio: Option<f64>,
    pub volume: Option<SplitFilterVolume>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SplitFilterConfig {
    pub input: InputPath,
    pub output: SplitFilterOutput,
    pub filter: Option<SplitFilterRules>,
}

