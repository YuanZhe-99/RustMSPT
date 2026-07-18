use crate::compute::backend::AccelerationMode;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AccelerationConfig {
    #[serde(default)]
    pub mode: AccelerationMode,
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_true")]
    pub cpu_fallback: bool,
    #[serde(default = "default_gpu_min_voxels")]
    pub gpu_min_voxels: usize,
    #[serde(default = "default_gpu_min_pixels")]
    pub gpu_min_pixels: usize,
    pub gpu_memory_limit_mb: Option<u64>,
    #[serde(default)]
    pub gpu_prefer_power: bool,
    #[serde(default = "default_gpu_precision")]
    pub gpu_precision: String,
}

fn default_backend() -> String {
    "wgpu".to_string()
}

fn default_true() -> bool {
    true
}

fn default_gpu_min_voxels() -> usize {
    250_000
}

// AI-FUNC-SUMMARY: Return the Auto-mode GPU render threshold; returns 250,000 pixels; side effects: None.
fn default_gpu_min_pixels() -> usize {
    250_000
}

fn default_gpu_precision() -> String {
    "f32".to_string()
}

impl Default for AccelerationConfig {
    fn default() -> Self {
        Self {
            mode: AccelerationMode::Auto,
            backend: default_backend(),
            cpu_fallback: true,
            gpu_min_voxels: default_gpu_min_voxels(),
            gpu_min_pixels: default_gpu_min_pixels(),
            gpu_memory_limit_mb: None,
            gpu_prefer_power: false,
            gpu_precision: default_gpu_precision(),
        }
    }
}
