use crate::error::{Result, RustMsptError};
use serde::de::{self, Deserializer};
use serde::Deserialize;
use std::fs;
use std::path::Path;

fn parse_usize_like(value: &str) -> std::result::Result<usize, String> {
    // Purpose: Parse usize from a string allowing underscore separators.
    // Inputs: raw numeric string.
    // Outputs: parsed usize or error message.
    let normalized = value.replace('_', "");
    normalized
        .parse::<usize>()
        .map_err(|e| format!("invalid usize value '{value}': {e}"))
}

fn deserialize_usize_flexible<'de, D>(deserializer: D) -> std::result::Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    // Purpose: Deserialize usize from YAML number or numeric string.
    // Inputs: serde deserializer.
    // Outputs: parsed usize value.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Num(u64),
        Str(String),
    }

    match Value::deserialize(deserializer)? {
        Value::Num(n) => usize::try_from(n).map_err(|_| de::Error::custom("value out of range for usize")),
        Value::Str(s) => parse_usize_like(&s).map_err(de::Error::custom),
    }
}

fn deserialize_option_usize_flexible<'de, D>(deserializer: D) -> std::result::Result<Option<usize>, D::Error>
where
    D: Deserializer<'de>,
{
    // Purpose: Deserialize optional usize from YAML number/string/null.
    // Inputs: serde deserializer.
    // Outputs: optional usize.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Num(u64),
        Str(String),
    }

    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(Value::Num(n)) => usize::try_from(n)
            .map(Some)
            .map_err(|_| de::Error::custom("value out of range for usize")),
        Some(Value::Str(s)) => parse_usize_like(&s).map(Some).map_err(de::Error::custom),
    }
}

fn parse_i32_like(value: &str) -> std::result::Result<i32, String> {
    // Purpose: Parse i32 from a string allowing underscore separators.
    // Inputs: raw numeric string.
    // Outputs: parsed i32 or error message.
    let normalized = value.replace('_', "");
    normalized
        .parse::<i32>()
        .map_err(|e| format!("invalid i32 value '{value}': {e}"))
}

fn deserialize_option_i32_flexible<'de, D>(deserializer: D) -> std::result::Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    // Purpose: Deserialize optional i32 from YAML number/string/null.
    // Inputs: serde deserializer.
    // Outputs: optional i32.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Num(i64),
        Str(String),
    }

    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(Value::Num(n)) => i32::try_from(n)
            .map(Some)
            .map_err(|_| de::Error::custom("value out of range for i32")),
        Some(Value::Str(s)) => parse_i32_like(&s).map(Some).map_err(de::Error::custom),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputStl {
    pub stl_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputPath {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputStl {
    pub stl_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputPath {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoxConfig {
    pub dimensions: Vec<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgingParams {
    pub input_stl_path: String,
    pub output_stl_path: Option<String>,
    pub compression_ratio: Option<f64>,
    pub compression_axis: Option<String>,
    pub bulge_factor: Option<f64>,
    pub roi_bounding_box: Option<Vec<f64>>,
    pub mesh_type: Option<String>,
    pub void_densification: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgingConfig {
    pub forging: ForgingParams,
}

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
}

#[derive(Debug, Clone, Deserialize)]
pub struct MeasurementConfig {
    pub measurement: MeasurementParams,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScalingParams {
    pub r#type: String,
    pub value: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScaleConfig {
    pub input: InputStl,
    pub output: OutputStl,
    pub scaling: ScalingParams,
}

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
    pub input: CropInput,
    pub output: CropOutput,
    pub interpolation: Option<String>,
    pub edge_trim: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackingFilters {
    pub min_volume: Option<f64>,
    pub max_aspect_ratio: Option<f64>,
    pub max_sharpness_ratio: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackingParams {
    pub target_volume_fraction: f64,
    pub mode: u8,
    pub max_attempts: usize,
    pub min_neighbor_distance: Option<f64>,
    pub min_boundary_dist: Option<f64>,
    pub min_cross_boundary_depth: Option<f64>,
    pub filters: Option<PackingFilters>,
    #[serde(default, deserialize_with = "deserialize_option_i32_flexible")]
    pub cpu_max: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackingConfig {
    pub input: InputPath,
    pub output: OutputPath,
    pub r#box: BoxConfig,
    pub packing: PackingParams,
}

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

#[derive(Debug, Clone, Deserialize)]
pub struct TargetConfig {
    pub r#type: String,
    pub s2_array: Option<Vec<f64>>,
    pub stl_path: Option<String>,
    pub stl_bounding_box: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizationParams {
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizationConfig {
    pub input: InputStl,
    pub output: OutputPath,
    pub target: TargetConfig,
    pub r#box: BoxConfig,
    pub optimization: OptimizationParams,
}

pub fn load_yaml<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T> {
    // Purpose: Read and deserialize a YAML file into typed config.
    // Inputs: YAML file path.
    // Outputs: parsed config structure.
    let text = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str::<T>(&text)?)
}

pub fn parse_box_dimensions(dimensions: &[f64]) -> Result<crate::types::BoundingBox> {
    // Purpose: Parse box dimensions from [sx,sy,sz] or [minx,miny,minz,maxx,maxy,maxz].
    // Inputs: dimensions array from config.
    // Outputs: normalized bounding box.
    match dimensions.len() {
        3 => Ok(crate::types::BoundingBox::from_size(crate::types::Vec3::new(
            dimensions[0],
            dimensions[1],
            dimensions[2],
        ))),
        6 => Ok(crate::types::BoundingBox {
            min: crate::types::Vec3::new(dimensions[0], dimensions[1], dimensions[2]),
            max: crate::types::Vec3::new(dimensions[3], dimensions[4], dimensions[5]),
        }),
        _ => Err(RustMsptError::InvalidConfig(
            "box.dimensions must have length 3 or 6".to_string(),
        )),
    }
}
