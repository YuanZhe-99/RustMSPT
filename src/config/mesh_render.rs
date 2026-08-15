use serde::Deserialize;
use std::collections::HashMap;

// AI-FUNC-SUMMARY: One requested view: either a named preset ("front", "iso_ne", ...) or a custom camera block; side effects: none.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ViewSpec {
    Named(String),
    Custom {
        name: String,
        view_direction: Vec<f64>,
        #[serde(default)]
        focus_point: Option<Vec<f64>>,
        #[serde(default)]
        up_vector: Option<Vec<f64>>,
    },
}

// AI-FUNC-SUMMARY: YAML form of one scene filter (kind-tagged), mapped 1:1 onto meshgen::SceneFilter by the pipeline; side effects: none.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilterSpec {
    CellKind { values: Vec<u8> },
    Component { values: Vec<i64> },
    RegionKey { values: Vec<i64> },
    Partition { values: Vec<i64> },
    Regime { values: Vec<i64> },
    Background { keep: bool },
    ArrayRange { array: String, min: f64, max: f64 },
    Bbox { min: Vec<f64>, max: Vec<f64> },
    ClipPlane { origin: Vec<f64>, normal: Vec<f64> },
}

// AI-FUNC-SUMMARY:
// Purpose: YAML parameters for the mesh-render subcommand (input VTU, views, image, coloring, opacities, filters, overlays).
// Notes: `color_by` = "uniform", a cell-array name, or a point-array name (`separation_t`, `sizing_h`); integer arrays render categorically, float arrays through the scalar colormap, and a point array colors each cell by the mean of its non-sentinel point values. `background` accepts RGB or RGBA (alpha 0 = transparent PNG background). `opacity_overrides` keys are region_key integers (as YAML strings).
#[derive(Debug, Clone, Deserialize)]
pub struct MeshRenderParams {
    pub input: String,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    #[serde(default = "default_views")]
    pub views: Vec<ViewSpec>,
    #[serde(default = "default_resolution")]
    pub width: usize,
    #[serde(default = "default_resolution")]
    pub height: usize,
    #[serde(default = "default_background")]
    pub background: Vec<u8>,
    #[serde(default = "default_ambient")]
    pub ambient: f64,
    #[serde(default = "default_color_by")]
    pub color_by: String,
    /// "cpu" (default, the transparency reference), "gpu" (opaque preview), or
    /// "auto" (GPU when available, silently falling back to CPU).
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default)]
    pub uniform_color: Option<Vec<u8>>,
    #[serde(default)]
    pub scalar_min: Option<f64>,
    #[serde(default)]
    pub scalar_max: Option<f64>,
    #[serde(default = "default_one")]
    pub volume_opacity: f64,
    #[serde(default = "default_one")]
    pub face_opacity: f64,
    #[serde(default)]
    pub opacity_overrides: HashMap<String, f64>,
    #[serde(default = "default_true")]
    pub show_faces: bool,
    #[serde(default = "default_true")]
    pub show_curves: bool,
    #[serde(default)]
    pub wireframe: bool,
    #[serde(default)]
    pub filters: Vec<FilterSpec>,
    #[serde(default)]
    pub highlight_points: Vec<Vec<f64>>,
    #[serde(default = "default_projection")]
    pub projection: String,
    #[serde(default = "default_fov_degrees")]
    pub perspective_fov_degrees: f64,
    #[serde(default)]
    pub camera_distance: Option<f64>,
    #[serde(default = "default_fit_padding")]
    pub fit_padding: f64,
}

// AI-FUNC-SUMMARY: Top-level YAML document for mesh-render (`mesh_render:` block); side effects: none.
#[derive(Debug, Clone, Deserialize)]
pub struct MeshRenderConfig {
    pub mesh_render: MeshRenderParams,
}

// AI-FUNC-SUMMARY: Default output directory for rendered views; returns data/output/mesh_render; side effects: none.
fn default_output_dir() -> String {
    "data/output/mesh_render".to_string()
}

// AI-FUNC-SUMMARY: Default view list (single iso_ne preset); returns Vec<ViewSpec>; side effects: none.
fn default_views() -> Vec<ViewSpec> {
    vec![ViewSpec::Named("iso_ne".to_string())]
}

// AI-FUNC-SUMMARY: Default image width or height; returns 2048; side effects: none.
// Notes: 1024 was too coarse to see the failure mode `[V5]`'s misattribution metric counts - a
//   ragged edge where elements at a body's edges and corners carry the background label reads as a
//   clean silhouette at 1024 and as a sawtooth at 2048. Raised after A-6a's limb was reported as
//   visibly notched and the render at the old default did not show it.
fn default_resolution() -> usize {
    2048
}

// AI-FUNC-SUMMARY: Default background color (opaque white RGBA); returns vec![255,255,255,255]; side effects: none.
fn default_background() -> Vec<u8> {
    vec![255, 255, 255, 255]
}

// AI-FUNC-SUMMARY: Default ambient light term; returns 0.25; side effects: none.
fn default_ambient() -> f64 {
    0.25
}

// AI-FUNC-SUMMARY: Default coloring array; returns "region_key"; side effects: none.
// AI-FUNC-SUMMARY: Serde default for mesh_render.backend: "cpu" (the exact-transparency reference); side effects: none.
fn default_backend() -> String {
    "cpu".to_string()
}

fn default_color_by() -> String {
    "region_key".to_string()
}

// AI-FUNC-SUMMARY: Serde default 1.0; returns 1.0; side effects: none.
fn default_one() -> f64 {
    1.0
}

// AI-FUNC-SUMMARY: Serde default true; returns true; side effects: none.
fn default_true() -> bool {
    true
}

// AI-FUNC-SUMMARY: Default projection name; returns "orthographic"; side effects: none.
fn default_projection() -> String {
    "orthographic".to_string()
}

// AI-FUNC-SUMMARY: Default perspective vertical FOV; returns 45 degrees; side effects: none.
fn default_fov_degrees() -> f64 {
    45.0
}

// AI-FUNC-SUMMARY: Default auto-framing padding fraction; returns 0.05; side effects: none.
fn default_fit_padding() -> f64 {
    0.05
}
