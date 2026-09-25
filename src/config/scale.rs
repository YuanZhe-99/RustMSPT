use serde::Deserialize;
use super::{InputStl, OutputStl};

#[derive(Debug, Clone, Deserialize)]
pub struct ScalingParams {
    pub r#type: String,
    pub value: f64,
    pub orient_to_positive_volume: Option<bool>,
    /// Worker budget for the whole run: absent or -1 uses every available worker.
    pub cpu_max: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScaleConfig {
    pub input: InputStl,
    pub output: OutputStl,
    pub scaling: ScalingParams,
}

