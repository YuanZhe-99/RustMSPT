pub mod acceleration;
pub mod crop;
pub mod deserialize;
pub mod forging;
pub mod measurement;
pub mod mesh_render;
pub mod optimization;
pub mod packing;
pub mod render;
pub mod scale;
pub mod split_filter;

use crate::error::{Result, RustMsptError};
use serde::Deserialize;
use std::fs;
use std::path::Path;

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

// AI-FUNC-SUMMARY:
// Purpose: Load and deserialize a YAML config file into a typed struct.
// Inputs: path to the YAML file.
// Returns: Deserialized config struct of type T.
// Side effects: Reads from disk.
// Notes: Returns Yaml error for invalid format, Io error for missing file.
pub fn load_yaml<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str::<T>(&text)?)
}

// AI-FUNC-SUMMARY:
// Purpose: Convert a dimensions array (3-element size or 6-element min/max) into a BoundingBox.
// Inputs: slice of f64 dimensions.
// Returns: BoundingBox with origin+size (len 3) or explicit min/max (len 6).
// Side effects: None.
// Notes: Returns InvalidConfig for any length other than 3 or 6.
pub fn parse_box_dimensions(dimensions: &[f64]) -> Result<crate::types::BoundingBox> {
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

pub use acceleration::AccelerationConfig;
pub use crop::{CropConfig, CropInput, CropOutput, CropRawParams};
pub use forging::{ForgingConfig, ForgingParams};
pub use measurement::{MeasurementConfig, MeasurementParams};
pub use mesh_render::{FilterSpec, MeshRenderConfig, MeshRenderParams, ViewSpec};
pub use optimization::{OptimizationConfig, OptimizationParams, TargetConfig};
pub use packing::{PackingConfig, PackingFilters, PackingParams};
pub use render::{RenderConfig, RenderParams};
pub use scale::{ScaleConfig, ScalingParams};
pub use split_filter::{SplitFilterConfig, SplitFilterOutput, SplitFilterRules, SplitFilterVolume};
