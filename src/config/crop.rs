use super::deserialize::deserialize_option_i32_flexible;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CropRawParams {
    pub width: usize,
    pub height: usize,
    pub bits: u8,
    pub signed: bool,
    pub byte_order: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CropInput {
    pub r#type: String,
    pub path: String,
    #[serde(default, deserialize_with = "deserialize_option_i32_flexible")]
    pub slice_start: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_option_i32_flexible")]
    pub slice_end: Option<i32>,
    pub raw: Option<CropRawParams>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CropOutput {
    pub path: String,
    pub folder_prefix: Option<String>,
    pub folder_extension: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CropConfig {
    #[serde(default)]
    pub acceleration: super::AccelerationConfig,
    #[serde(default, deserialize_with = "deserialize_option_i32_flexible")]
    pub cpu_max: Option<i32>,
    pub input: CropInput,
    pub output: CropOutput,
    pub interpolation: Option<String>,
    pub edge_trim: Option<i32>,
}
