use crate::error::{Result, RustMsptError};
use crate::types::{BoundingBox, Vec3};
use serde::Deserialize;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------- raw YAML shapes

/// The `placement:` block as written in YAML, before validation.
///
/// Every struct in this block sets `deny_unknown_fields`, which the older config
/// structs deliberately do not. A misspelled key in a legacy config is ignored and
/// the run continues with a default; here a misspelled key would silently change
/// what was placed and could never be noticed afterwards, so it is refused by name.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementDocument {
    pub placement: PlacementParams,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementParams {
    pub seed: u64,
    pub frame: FrameSpec,
    pub domain: DomainSpec,
    pub shapes: ShapesSpec,
    #[serde(default)]
    pub void: Option<VoidSpec>,
    pub size: SizeSpec,
    #[serde(default)]
    pub orientation: OrientationSpec,
    #[serde(default)]
    pub position: PositionSpec,
    #[serde(default)]
    pub boundary: BoundarySpec,
    #[serde(default)]
    pub gaps: GapsSpec,
    pub target: TargetSpec,
    #[serde(default)]
    pub budget: BudgetSpec,
    #[serde(default = "default_threads")]
    pub threads: i32,
    pub outputs: OutputsSpec,
    #[serde(default)]
    pub aggregates: AggregateSpec,
    #[serde(default)]
    pub memory: PlacementMemorySpec,
    #[serde(default)]
    pub checkpoint: CheckpointSpec,
    #[serde(default)]
    pub initial_particles: Option<InitialParticlesSpec>,
}

/// Explicitly import a frozen accepted assembly into a new exact-geometry fill task.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialParticlesSpec {
    pub record: PathBuf,
    pub report: PathBuf,
    /// Existing cluster interiors may have a smaller gap than incoming particles.
    pub existing_gap: f64,
    #[serde(default)]
    pub pending_checkpoint: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub retry_failed: bool,
}

/// Durable placement state; final checkpoints are saved even after target attainment.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CheckpointSpec {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub every_particles: usize,
    pub resume_from: Option<String>,
    pub extend: bool,
}
impl Default for CheckpointSpec {
    // AI-FUNC-SUMMARY: Enable final/interrupt and periodic checkpoints by default; continuation remains explicitly requested.
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 60,
            every_particles: 1000,
            resume_from: None,
            extend: false,
        }
    }
}

/// Resident exact geometry is a cache, never the authoritative particle record.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlacementMemorySpec {
    pub geometry_cache_mb: usize,
    pub simplified_collision: bool,
}
impl Default for PlacementMemorySpec {
    // AI-FUNC-SUMMARY: Default to a bounded 256 MiB estimated geometry cache; proxies are opt-in until benchmarked.
    fn default() -> Self {
        Self {
            geometry_cache_mb: 256,
            simplified_collision: false,
        }
    }
}

/// Optional hierarchical packing; disabled unless explicitly enabled.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AggregateSpec {
    pub construction: AggregateConstruction,
    pub contact_directions: usize,
    pub contact_orientations: usize,
    /// Relative to the moving member radius.
    pub contact_tolerance: f64,
    pub contact_max_steps: usize,
    /// Start orientations: fixed rotations, or principal axes laid against the approach direction.
    pub contact_orientation: ContactOrientation,
    /// Roll steps (lift, slide, fall toward the centre) applied to the best straight approaches; 0 disables.
    pub contact_settle_steps: usize,
    /// How many best straight approaches are settled.
    pub contact_settle_candidates: usize,
    pub neighborhood_sweeps: usize,
    pub enabled: bool,
    #[serde(deserialize_with = "deserialize_aggregate_variants")]
    pub variants: usize,
    pub mode: AggregateMode,
    pub fallback_particles_per_cluster: Vec<usize>,
    /// Refine proxy collisions using real member geometry for mixed fallback stages.
    pub exact_fallback: bool,
    pub particles_per_cluster: usize,
    pub shape: AggregateShape,
    pub internal_gap: f64,
    pub compaction_sweeps: usize,
    pub mesh_refinement_sweeps: usize,
    pub target_internal_volume_fraction: Option<f64>,
    pub strategy_rounds: usize,
    pub max_compaction_trials: usize,
    pub rotation_search: bool,
    pub lateral_rearrangement: bool,
    pub pair_rearrangement: bool,
    pub container_shrink_fraction: f64,
    pub rotation_step_degrees: f64,
}
impl Default for AggregateSpec {
    // AI-FUNC-SUMMARY: Defaults for opt-in deterministic aggregate generation; no I/O.
    fn default() -> Self {
        Self {
            construction: AggregateConstruction::Fcc,
            contact_directions: 12,
            contact_orientations: 4,
            contact_tolerance: 1e-5,
            contact_max_steps: 96,
            contact_orientation: ContactOrientation::Fixed,
            contact_settle_steps: 0,
            contact_settle_candidates: 3,
            neighborhood_sweeps: 2,
            enabled: false,
            variants: 8,
            mode: AggregateMode::Clusters,
            fallback_particles_per_cluster: vec![16, 4, 1],
            exact_fallback: false,
            particles_per_cluster: 64,
            shape: AggregateShape::Sphere,
            internal_gap: 0.1,
            compaction_sweeps: 32,
            mesh_refinement_sweeps: 8,
            target_internal_volume_fraction: None,
            strategy_rounds: 8,
            max_compaction_trials: 6000,
            rotation_search: true,
            lateral_rearrangement: true,
            pair_rearrangement: true,
            container_shrink_fraction: 0.03,
            rotation_step_degrees: 15.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateConstruction {
    Fcc,
    ContactGrowth,
}

/// Contact-growth start orientations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactOrientation {
    /// Identity plus fixed axis rotations (the original method).
    Fixed,
    /// Minor or major principal axis along the approach, spun about it: flat faces meet the assembly.
    Principal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateShape {
    Sphere,
    Cube,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateMode {
    Clusters,
    Mixed,
}

// AI-FUNC-SUMMARY: Deserialize explicit template count or auto (stored as zero); rejects zero numeric counts and other strings; no I/O.
fn deserialize_aggregate_variants<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<usize, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Count {
        Number(usize),
        Text(String),
    }
    match Count::deserialize(d)? {
        Count::Number(n) if n > 0 => Ok(n),
        Count::Text(s) if s == "auto" => Ok(0),
        _ => Err(serde::de::Error::custom(
            "variants must be a positive count or auto",
        )),
    }
}

// AI-FUNC-SUMMARY: The default thread setting, -1 meaning every available core; returns i32; side effects: none.
fn default_threads() -> i32 {
    -1
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameSpec {
    /// A label only. Nothing scales by it; it is copied into every output so a
    /// consumer can tell what the numbers mean.
    pub unit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainSpec {
    pub min: Vec<f64>,
    pub max: Vec<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapesSpec {
    pub files: Vec<String>,
    #[serde(default)]
    pub selection: ShapeSelection,
    #[serde(default)]
    pub filters: Option<ShapeFilters>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShapeSelection {
    /// Uniform over every shell of every listed file.
    #[default]
    Uniform,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeFilters {
    /// Longest bounding-box extent over shortest. Scale-invariant, so it can be
    /// applied once to the library rather than to every rescaled candidate.
    #[serde(default)]
    pub max_aspect_ratio: Option<f64>,
    /// Area^3 / (36 pi Volume^2); 1.0 for a sphere. Also scale-invariant.
    #[serde(default)]
    pub max_sharpness_ratio: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoidSpec {
    pub file: String,
    #[serde(default)]
    pub crossing: VoidCrossing,
    pub gap: f64,
    #[serde(default)]
    pub overlap_volume: Option<OverlapVolumeSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VoidCrossing {
    /// The whole particle keeps `gap` from the void surface.
    #[default]
    Forbidden,
    /// A particle may intersect the void; the overlap is measured and owned by the void.
    Allowed,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlapVolumeSpec {
    /// No default: the cost and the accuracy of the overlap measurement are the
    /// same knob, and which trade to make is the caller's to state.
    pub voxel_size: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizeSpec {
    pub distribution: DistributionSpec,
    #[serde(default)]
    pub classes: Option<ClassesSpec>,
    #[serde(default)]
    pub on_unattainable: OnUnattainable,
    #[serde(default)]
    pub placement_order: PlacementOrder,
}

/// A plain struct with a `kind` discriminant rather than an internally tagged
/// enum: serde buffers an internally tagged enum through a map, and
/// `deny_unknown_fields` does not fire on that path, so `{kind: lognormal,
/// mediann: 12}` would be accepted with the real field missing.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributionSpec {
    pub kind: DistributionKind,
    #[serde(default)]
    pub median: Option<f64>,
    #[serde(default)]
    pub sigma_log: Option<f64>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub csv: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionKind {
    Lognormal,
    Histogram,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassesSpec {
    pub kind: ClassKind,
    #[serde(default)]
    pub count: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassKind {
    EqualWidth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnUnattainable {
    /// Skip the size that could not be placed, report the shortfall, and draw no
    /// replacement. The run still ends `distribution_unattainable`.
    #[default]
    SkipReported,
    /// Stop at the first size that could not be placed.
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlacementOrder {
    /// Largest first. Large particles are the ones that stop fitting, so placing
    /// them while there is room is what stops the run from quietly becoming a
    /// pile of small ones.
    #[default]
    Descending,
    /// The order the sizes were drawn in.
    Drawn,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct OrientationSpec {
    #[serde(default)]
    pub mode: OrientationMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrientationMode {
    /// Shoemake's uniform unit quaternion: Haar-uniform on SO(3).
    #[default]
    UniformSo3,
    /// Every particle keeps the orientation it has in its source file.
    Fixed,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PositionSpec {
    #[serde(default)]
    pub mode: PositionMode,
    #[serde(default)]
    pub band: Option<Vec<f64>>,
    #[serde(default)]
    pub free_space: Option<FreeSpaceSpec>,
}

/// Bounded geometry-guided position search; it never relaxes real collision rules.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FreeSpaceSpec {
    pub coarse_cell_size: f64,
    pub min_cell_size: f64,
    pub max_cells: usize,
    pub index_memory_mb: usize,
    pub candidates_per_location: usize,
    pub exploration_fraction: f64,
    pub local_refinement: bool,
}
impl Default for FreeSpaceSpec {
    fn default() -> Self {
        Self {
            coarse_cell_size: 8.0,
            min_cell_size: 1.0,
            max_cells: 250000,
            index_memory_mb: 128,
            candidates_per_location: 8,
            exploration_fraction: 0.10,
            local_refinement: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PositionMode {
    /// Uniform on the set of placements that satisfy every constraint.
    #[default]
    FeasibleUniform,
    /// Positions drawn within a declared distance band of the void surface. A
    /// deliberate construction, and never reported as random.
    VoidNeighbourhood,
    /// Geometric cavity guidance with explicit global exploration.
    FreeSpaceGuided,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BoundarySpec {
    #[serde(default)]
    pub mode: BoundaryMode,
    #[serde(default)]
    pub min_boundary_dist: Option<f64>,
    #[serde(default)]
    pub min_cross_boundary_depth: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryMode {
    /// Every particle lies wholly inside the domain.
    #[default]
    Strict,
    /// A particle may straddle the domain boundary; only the part inside counts.
    Clip,
    /// Wrap-around images are checked for collision.
    Periodic,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GapsSpec {
    #[serde(default)]
    pub particle_particle: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSpec {
    pub volume_fraction: f64,
    #[serde(default)]
    pub basis: Option<TargetBasis>,
    #[serde(default = "default_vf_tolerance")]
    pub tolerance: f64,
}

// AI-FUNC-SUMMARY: Default relative tolerance on the target volume fraction; returns f64; side effects: none.
fn default_vf_tolerance() -> f64 {
    0.01
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetBasis {
    /// The whole domain box.
    Domain,
    /// The domain minus the void: what a solid-phase fraction is usually against.
    Solid,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetSpec {
    #[serde(default = "default_attempts_per_particle")]
    pub attempts_per_particle: usize,
    #[serde(default = "default_total_attempts")]
    pub total_attempts: usize,
    #[serde(default)]
    pub wall_time_s: Option<f64>,
    #[serde(default = "default_max_top_up_batches")]
    pub max_top_up_batches: usize,
}

impl Default for BudgetSpec {
    fn default() -> Self {
        BudgetSpec {
            attempts_per_particle: default_attempts_per_particle(),
            total_attempts: default_total_attempts(),
            wall_time_s: None,
            max_top_up_batches: default_max_top_up_batches(),
        }
    }
}

// AI-FUNC-SUMMARY: Default per-particle placement attempt budget; returns usize; side effects: none.
fn default_attempts_per_particle() -> usize {
    2000
}

// AI-FUNC-SUMMARY: Default whole-run placement attempt budget; returns usize; side effects: none.
fn default_total_attempts() -> usize {
    2_000_000
}

// AI-FUNC-SUMMARY: Default cap on top-up batches drawn after clipping losses; returns usize; side effects: none.
fn default_max_top_up_batches() -> usize {
    5
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputsSpec {
    pub dir: String,
    #[serde(default = "default_particles_stl")]
    pub particles_stl: String,
    #[serde(default = "default_record")]
    pub record: String,
    #[serde(default = "default_report")]
    pub report: String,
    #[serde(default = "default_size_csv")]
    pub size_csv: String,
    #[serde(default = "default_true")]
    pub copy_void: bool,
    #[serde(default)]
    pub per_particle_stl: bool,
    #[serde(default)]
    pub voxel_labels: Option<VoxelLabelsSpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoxelLabelsSpec {
    pub voxel_size: f64,
}

// AI-FUNC-SUMMARY: Default file name for the merged particle STL; returns String; side effects: none.
fn default_particles_stl() -> String {
    "particles.stl".to_string()
}
// AI-FUNC-SUMMARY: Default file name for the per-particle record; returns String; side effects: none.
fn default_record() -> String {
    "particles.json".to_string()
}
// AI-FUNC-SUMMARY: Default file name for the run report; returns String; side effects: none.
fn default_report() -> String {
    "run_report.json".to_string()
}
// AI-FUNC-SUMMARY: Default file name for the size-class comparison CSV; returns String; side effects: none.
fn default_size_csv() -> String {
    "size_distribution.csv".to_string()
}
// AI-FUNC-SUMMARY: Serde default helper yielding true; returns bool; side effects: none.
fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------- validated form

/// A `placement:` block that has been checked and had its paths resolved.
///
/// The engine reads this, not `PlacementParams`: every cross-field rule has already
/// been applied, every path is absolute-or-config-relative, and nothing optional is
/// left for the engine to second-guess.
#[derive(Debug, Clone)]
pub struct ResolvedPlacement {
    pub seed: u64,
    pub unit: String,
    pub domain: BoundingBox,
    pub shape_files: Vec<PathBuf>,
    pub shape_paths_as_written: Vec<String>,
    pub selection: ShapeSelection,
    pub filters: Option<ShapeFilters>,
    pub void: Option<ResolvedVoid>,
    pub distribution: ResolvedDistribution,
    pub classes: ResolvedClasses,
    pub on_unattainable: OnUnattainable,
    pub placement_order: PlacementOrder,
    pub orientation: OrientationMode,
    pub position: ResolvedPosition,
    pub boundary: ResolvedBoundary,
    pub gap_particle_particle: f64,
    pub target_volume_fraction: f64,
    pub target_basis: TargetBasis,
    pub target_tolerance: f64,
    pub budget: BudgetSpec,
    pub threads: i32,
    pub outputs: ResolvedOutputs,
    pub aggregates: AggregateSpec,
    pub memory: PlacementMemorySpec,
    pub checkpoint: CheckpointSpec,
    pub initial_particles: Option<InitialParticlesSpec>,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ResolvedVoid {
    pub file: PathBuf,
    pub path_as_written: String,
    pub crossing: VoidCrossing,
    pub gap: f64,
    pub overlap_voxel_size: Option<f64>,
}

#[derive(Debug, Clone)]
pub enum ResolvedDistribution {
    Lognormal {
        median: f64,
        sigma_log: f64,
        min: f64,
        max: f64,
    },
    Histogram {
        csv: PathBuf,
        path_as_written: String,
    },
}

#[derive(Debug, Clone)]
pub enum ResolvedClasses {
    /// Histogram bins double as the reporting classes.
    FromHistogram,
    EqualWidth {
        count: usize,
    },
}

#[derive(Debug, Clone)]
pub struct ResolvedPosition {
    pub mode: PositionMode,
    pub band: Option<(f64, f64)>,
    pub free_space: FreeSpaceSpec,
}

#[derive(Debug, Clone)]
pub struct ResolvedBoundary {
    pub mode: BoundaryMode,
    pub min_boundary_dist: f64,
    pub min_cross_boundary_depth: f64,
}

#[derive(Debug, Clone)]
pub struct ResolvedOutputs {
    pub dir: PathBuf,
    pub particles_stl: PathBuf,
    pub record: PathBuf,
    pub report: PathBuf,
    pub size_csv: PathBuf,
    pub copy_void: bool,
    pub per_particle_stl: bool,
    pub voxel_labels: Option<f64>,
}

/// Which engine a `pack` config selected.
#[derive(Debug)]
pub enum PackDocument {
    /// The seeded, recorded, void-aware engine (`placement:`).
    Placement(Box<PlacementParams>),
    /// The original packing loop (`packing:`), unchanged.
    Legacy(Box<super::PackingConfig>),
}

/// Probe used to decide which engine a config selects without committing to either
/// shape. `IgnoredAny` accepts whatever is under the key, and unknown top-level
/// keys are ignored, so this cannot fail on a config it is only inspecting.
#[derive(Deserialize)]
struct EngineProbe {
    #[serde(default)]
    placement: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    packing: Option<serde::de::IgnoredAny>,
}

// AI-FUNC-SUMMARY:
// Purpose: Decide which packing engine a config selects and deserialize it into that engine's type.
// Inputs: path to the YAML config.
// Returns: PackDocument::Placement or PackDocument::Legacy.
// Side effects: Reads the file from disk (once).
// Notes: Two passes over the same text rather than one pass through serde_yaml::Value: routing an
// existing config through Value would change how anchors and merge keys are resolved, and a legacy
// config must keep meaning exactly what it meant before. PackingConfig has four required fields, so
// simply attempting it first and falling back is not possible - a placement-only document fails it
// with "missing field `input`", which tells the user nothing. Both blocks present, or neither, is a
// named error listing both keys, so a typo like `placment:` says what is wrong.
pub fn load_pack_document(path: &Path) -> Result<PackDocument> {
    let text = std::fs::read_to_string(path)?;
    let probe: EngineProbe = serde_yaml::from_str(&text)?;
    match (probe.placement.is_some(), probe.packing.is_some()) {
        (true, false) => {
            let doc: PlacementDocument = serde_yaml::from_str(&text)?;
            Ok(PackDocument::Placement(Box::new(doc.placement)))
        }
        (false, true) => {
            let conf: super::PackingConfig = serde_yaml::from_str(&text)?;
            Ok(PackDocument::Legacy(Box::new(conf)))
        }
        (true, true) => Err(RustMsptError::InvalidConfig(format!(
            "{}: a pack config selects one engine, but both `placement:` and `packing:` are present. \
             Keep `placement:` for the seeded, recorded engine or `packing:` for the original one.",
            path.display()
        ))),
        (false, false) => Err(RustMsptError::InvalidConfig(format!(
            "{}: a pack config needs a top-level `placement:` block (the seeded, recorded engine) \
             or `packing:` block (the original engine); neither is present.",
            path.display()
        ))),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Resolve a path written in a config against the directory holding that config.
// Inputs: the config's directory, the path as written.
// Returns: the path as written when absolute, else joined onto the directory.
// Side effects: None.
// Notes: A lexical join, never fs::canonicalize: canonicalize fails on an output path that does not
// exist yet and resolves symlinks, so the report's `path` would stop matching what the user wrote.
// A config given as a bare file name has an empty parent, which callers pass as ".".
pub fn resolve_against(dir: &Path, written: &str) -> PathBuf {
    let p = Path::new(written);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        dir.join(p)
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Return the directory a config's relative paths resolve against.
// Inputs: the config file's path.
// Returns: its parent directory, or "." when it has none.
// Side effects: None.
// Notes: `--config pack.yaml` has an empty parent, which would join to a bare relative path and
// silently reintroduce the CWD dependency this block exists to remove.
pub fn config_dir(config_path: &Path) -> PathBuf {
    match config_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

// AI-FUNC-SUMMARY: Reject a non-finite or non-positive number by field name; returns Ok(value); side effects: none.
fn require_positive(name: &str, value: f64) -> Result<f64> {
    if !value.is_finite() || value <= 0.0 {
        return Err(RustMsptError::InvalidConfig(format!(
            "placement.{name} must be a finite positive number, got {value}"
        )));
    }
    Ok(value)
}

// AI-FUNC-SUMMARY: Reject a non-finite or negative number by field name; returns Ok(value); side effects: none.
fn require_non_negative(name: &str, value: f64) -> Result<f64> {
    if !value.is_finite() || value < 0.0 {
        return Err(RustMsptError::InvalidConfig(format!(
            "placement.{name} must be a finite non-negative number, got {value}"
        )));
    }
    Ok(value)
}

impl PlacementParams {
    // AI-FUNC-SUMMARY:
    // Purpose: Check every cross-field rule and resolve every path, yielding the form the engine runs on.
    // Inputs: self, and the path of the config file this block came from.
    // Returns: ResolvedPlacement, or InvalidConfig naming the field and the rule it broke.
    // Side effects: None (no file is opened; existence is the loader's business).
    // Notes: Every refusal happens here, so the engine has no validation left and no field it must
    // decide the meaning of. Paths resolve against the config's own directory, never the working
    // directory; a path given on the command line is resolved by the caller against the CWD instead,
    // which is what a shell argument means.
    pub fn validate(mut self, config_path: &Path) -> Result<ResolvedPlacement> {
        if self.checkpoint.extend && self.checkpoint.resume_from.is_none() {
            return Err(RustMsptError::InvalidConfig(
                "placement.checkpoint.extend requires resume_from".into(),
            ));
        }
        if self.checkpoint.resume_from.is_some() && !self.checkpoint.enabled {
            return Err(RustMsptError::InvalidConfig(
                "placement.checkpoint.resume_from requires enabled=true".into(),
            ));
        }
        if let Some(path) = &mut self.checkpoint.resume_from {
            let p = Path::new(path);
            if !p.is_absolute() {
                *path = config_path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(p)
                    .to_string_lossy()
                    .into_owned();
            }
        }
        let dir = config_dir(config_path);

        if self.domain.min.len() != 3 || self.domain.max.len() != 3 {
            return Err(RustMsptError::InvalidConfig(
                "placement.domain.min and .max must each have three components".to_string(),
            ));
        }
        for (i, axis) in ["x", "y", "z"].iter().enumerate() {
            // Written out rather than as `max <= min` so a NaN bound is refused too:
            // every comparison against NaN is false, so `max <= min` alone would let
            // a NaN domain through and produce a run with no meaningful extent.
            let extent = self.domain.max[i] - self.domain.min[i];
            if !extent.is_finite() || extent <= 0.0 {
                return Err(RustMsptError::InvalidConfig(format!(
                    "placement.domain must have positive extent on every axis; {axis} runs {} to {}",
                    self.domain.min[i], self.domain.max[i]
                )));
            }
        }
        let domain = BoundingBox {
            min: Vec3::new(self.domain.min[0], self.domain.min[1], self.domain.min[2]),
            max: Vec3::new(self.domain.max[0], self.domain.max[1], self.domain.max[2]),
        };

        if self.shapes.files.is_empty() {
            return Err(RustMsptError::InvalidConfig(
                "placement.shapes.files must list at least one STL".to_string(),
            ));
        }
        let shape_files = self
            .shapes
            .files
            .iter()
            .map(|f| resolve_against(&dir, f))
            .collect();

        if let Some(f) = &self.shapes.filters {
            if let Some(v) = f.max_aspect_ratio {
                require_positive("shapes.filters.max_aspect_ratio", v)?;
            }
            if let Some(v) = f.max_sharpness_ratio {
                require_positive("shapes.filters.max_sharpness_ratio", v)?;
            }
        }

        let void = match &self.void {
            None => None,
            Some(v) => {
                require_non_negative("void.gap", v.gap)?;
                match v.crossing {
                    VoidCrossing::Forbidden => {
                        // A zero gap makes the distance test vacuous: parry reports 0.0
                        // for two shapes that intersect, and 0.0 >= 0.0 passes, so a
                        // particle could interpenetrate the void freely.
                        if v.gap <= 0.0 {
                            return Err(RustMsptError::InvalidConfig(
                                "placement.void.gap must be greater than zero when crossing is \
                                 `forbidden`: a zero gap cannot distinguish touching from \
                                 overlapping, so it would not forbid anything"
                                    .to_string(),
                            ));
                        }
                        if v.overlap_volume.is_some() {
                            return Err(RustMsptError::InvalidConfig(
                                "placement.void.overlap_volume is only read when crossing is \
                                 `allowed`; remove it or set crossing: allowed"
                                    .to_string(),
                            ));
                        }
                    }
                    VoidCrossing::Allowed => {
                        if v.overlap_volume.is_none() {
                            return Err(RustMsptError::InvalidConfig(
                                "placement.void.overlap_volume.voxel_size is required when \
                                 crossing is `allowed`: the overlap each particle has with the \
                                 void is measured on a voxel grid, and its resolution is a cost \
                                 and accuracy trade this tool will not pick for you"
                                    .to_string(),
                            ));
                        }
                    }
                }
                let overlap_voxel_size = match &v.overlap_volume {
                    Some(o) => Some(require_positive(
                        "void.overlap_volume.voxel_size",
                        o.voxel_size,
                    )?),
                    None => None,
                };
                Some(ResolvedVoid {
                    file: resolve_against(&dir, &v.file),
                    path_as_written: v.file.clone(),
                    crossing: v.crossing,
                    gap: v.gap,
                    overlap_voxel_size,
                })
            }
        };

        let distribution = match self.size.distribution.kind {
            DistributionKind::Lognormal => {
                let d = &self.size.distribution;
                if d.csv.is_some() {
                    return Err(RustMsptError::InvalidConfig(
                        "placement.size.distribution.csv belongs to kind: histogram".to_string(),
                    ));
                }
                let median = require_positive(
                    "size.distribution.median",
                    d.median.ok_or_else(|| missing("median", "lognormal"))?,
                )?;
                let sigma_log = require_positive(
                    "size.distribution.sigma_log",
                    d.sigma_log
                        .ok_or_else(|| missing("sigma_log", "lognormal"))?,
                )?;
                let min = require_positive(
                    "size.distribution.min",
                    d.min.ok_or_else(|| missing("min", "lognormal"))?,
                )?;
                let max = require_positive(
                    "size.distribution.max",
                    d.max.ok_or_else(|| missing("max", "lognormal"))?,
                )?;
                if min >= max {
                    return Err(RustMsptError::InvalidConfig(format!(
                        "placement.size.distribution.min must be less than .max, got {min} and {max}"
                    )));
                }
                ResolvedDistribution::Lognormal {
                    median,
                    sigma_log,
                    min,
                    max,
                }
            }
            DistributionKind::Histogram => {
                let d = &self.size.distribution;
                for (name, present) in [
                    ("median", d.median.is_some()),
                    ("sigma_log", d.sigma_log.is_some()),
                    ("min", d.min.is_some()),
                    ("max", d.max.is_some()),
                ] {
                    if present {
                        return Err(RustMsptError::InvalidConfig(format!(
                            "placement.size.distribution.{name} belongs to kind: lognormal"
                        )));
                    }
                }
                let csv = d.csv.clone().ok_or_else(|| missing("csv", "histogram"))?;
                ResolvedDistribution::Histogram {
                    csv: resolve_against(&dir, &csv),
                    path_as_written: csv,
                }
            }
        };

        let classes = match (&self.size.classes, &distribution) {
            (None, ResolvedDistribution::Histogram { .. }) => ResolvedClasses::FromHistogram,
            (None, ResolvedDistribution::Lognormal { .. }) => {
                ResolvedClasses::EqualWidth { count: 10 }
            }
            (Some(c), _) => match c.kind {
                ClassKind::EqualWidth => {
                    let count = c.count.unwrap_or(10);
                    if count == 0 {
                        return Err(RustMsptError::InvalidConfig(
                            "placement.size.classes.count must be at least 1".to_string(),
                        ));
                    }
                    ResolvedClasses::EqualWidth { count }
                }
            },
        };

        let band = match (self.position.mode, &self.position.band) {
            (PositionMode::VoidNeighbourhood, None) => {
                return Err(RustMsptError::InvalidConfig(
                    "placement.position.band is required for mode: void_neighbourhood".to_string(),
                ))
            }
            (PositionMode::VoidNeighbourhood, Some(b)) => {
                if void.is_none() {
                    return Err(RustMsptError::InvalidConfig(
                        "placement.position.mode: void_neighbourhood needs a placement.void to \
                         sample around"
                            .to_string(),
                    ));
                }
                if b.len() != 2 {
                    return Err(RustMsptError::InvalidConfig(
                        "placement.position.band must be [min_distance, max_distance]".to_string(),
                    ));
                }
                require_non_negative("position.band[0]", b[0])?;
                require_positive("position.band[1]", b[1])?;
                if b[0] >= b[1] {
                    return Err(RustMsptError::InvalidConfig(format!(
                        "placement.position.band must increase, got {} then {}",
                        b[0], b[1]
                    )));
                }
                Some((b[0], b[1]))
            }
            (PositionMode::FeasibleUniform | PositionMode::FreeSpaceGuided, Some(_)) => {
                return Err(RustMsptError::InvalidConfig(
                    "placement.position.band is only read for mode: void_neighbourhood".to_string(),
                ))
            }
            (PositionMode::FeasibleUniform | PositionMode::FreeSpaceGuided, None) => None,
        };

        if self.boundary.mode == BoundaryMode::Periodic && void.is_some() {
            return Err(RustMsptError::InvalidConfig(
                "placement.boundary.mode: periodic is not supported together with placement.void: \
                 a wrapped image of a particle would have to be checked against a wrapped image of \
                 a frozen void, and what the void means outside the domain is not established"
                    .to_string(),
            ));
        }
        let boundary = ResolvedBoundary {
            mode: self.boundary.mode,
            min_boundary_dist: require_non_negative(
                "boundary.min_boundary_dist",
                self.boundary.min_boundary_dist.unwrap_or(0.0),
            )?,
            min_cross_boundary_depth: require_non_negative(
                "boundary.min_cross_boundary_depth",
                self.boundary.min_cross_boundary_depth.unwrap_or(0.0),
            )?,
        };

        let gap_particle_particle =
            require_non_negative("gaps.particle_particle", self.gaps.particle_particle)?;

        let vf = self.target.volume_fraction;
        if !vf.is_finite() || vf <= 0.0 || vf >= 1.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "placement.target.volume_fraction must lie strictly between 0 and 1, got {vf}"
            )));
        }
        let target_basis = match self.target.basis {
            Some(TargetBasis::Solid) => {
                if void.is_none() {
                    return Err(RustMsptError::InvalidConfig(
                        "placement.target.basis: solid means `the domain minus the void`, so it \
                         needs a placement.void; use basis: domain instead"
                            .to_string(),
                    ));
                }
                TargetBasis::Solid
            }
            Some(TargetBasis::Domain) => TargetBasis::Domain,
            // With a void present the interesting fraction is almost always of the
            // solid, so that is the default; without one the two coincide anyway.
            None if void.is_some() => TargetBasis::Solid,
            None => TargetBasis::Domain,
        };
        let tolerance = self.target.tolerance;
        if !tolerance.is_finite() || !(0.0..1.0).contains(&tolerance) {
            return Err(RustMsptError::InvalidConfig(format!(
                "placement.target.tolerance is a relative tolerance in [0, 1), got {tolerance}"
            )));
        }

        if self.budget.attempts_per_particle == 0 || self.budget.total_attempts == 0 {
            return Err(RustMsptError::InvalidConfig(
                "placement.budget attempt limits must be at least 1".to_string(),
            ));
        }
        if let Some(w) = self.budget.wall_time_s {
            require_positive("budget.wall_time_s", w)?;
        }

        let out_dir = resolve_against(&dir, &self.outputs.dir);
        let voxel_labels = match &self.outputs.voxel_labels {
            Some(v) => Some(require_positive(
                "outputs.voxel_labels.voxel_size",
                v.voxel_size,
            )?),
            None => None,
        };
        let outputs = ResolvedOutputs {
            particles_stl: out_dir.join(&self.outputs.particles_stl),
            record: out_dir.join(&self.outputs.record),
            report: out_dir.join(&self.outputs.report),
            size_csv: out_dir.join(&self.outputs.size_csv),
            dir: out_dir,
            copy_void: self.outputs.copy_void,
            per_particle_stl: self.outputs.per_particle_stl,
            voxel_labels,
        };

        if self.aggregates.enabled {
            let a = &self.aggregates;
            if !(4..=256).contains(&a.contact_directions)
                || !(1..=64).contains(&a.contact_orientations)
                || !(8..=1024).contains(&a.contact_max_steps)
                || a.neighborhood_sweeps > 128
                || !a.contact_tolerance.is_finite()
                || !(1e-9..=1e-2).contains(&a.contact_tolerance)
                || a.contact_settle_steps > 64
                || !(1..=64).contains(&a.contact_settle_candidates)
            {
                return Err(RustMsptError::InvalidConfig("aggregate contact controls: directions 4..256, orientations 1..64, max_steps 8..1024, neighborhood_sweeps <=128, relative tolerance 1e-9..1e-2, settle_steps <=64, settle_candidates 1..64 required".into()));
            }
            if a.variants > 128
                || a.particles_per_cluster == 0
                || a.particles_per_cluster > 1024
                || a.variants * a.particles_per_cluster > 65536
                || !a.internal_gap.is_finite()
                || a.internal_gap < 0.0
                || a.compaction_sweeps > 1024
                || a.mesh_refinement_sweeps > 128
            {
                return Err(RustMsptError::InvalidConfig("placement.aggregates: variants=1..128 or auto, particles_per_cluster=1..1024, total template members<=65536, finite internal_gap>=0, compaction_sweeps<=1024, mesh_refinement_sweeps<=128 required".into()));
            }
            if a.target_internal_volume_fraction
                .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 1.0)
                || a.strategy_rounds > 128
                || a.max_compaction_trials > 1_000_000
                || !a.container_shrink_fraction.is_finite()
                || a.container_shrink_fraction <= 0.0
                || a.container_shrink_fraction >= 0.5
                || !a.rotation_step_degrees.is_finite()
                || a.rotation_step_degrees <= 0.0
                || a.rotation_step_degrees > 180.0
            {
                return Err(RustMsptError::InvalidConfig("placement.aggregates: target_internal_volume_fraction in (0,1], strategy_rounds<=128, max_compaction_trials<=1000000, container_shrink_fraction in (0,0.5), rotation_step_degrees in (0,180] required".into()));
            }
            if a.mode == AggregateMode::Mixed {
                let mut last = a.particles_per_cluster;
                let mut total = last;
                if a.fallback_particles_per_cluster.is_empty()
                    || a.fallback_particles_per_cluster.len() > 8
                {
                    return Err(RustMsptError::InvalidConfig(
                        "mixed aggregates require 1..8 fallback member counts".into(),
                    ));
                }
                for &n in &a.fallback_particles_per_cluster {
                    if n == 0 || n >= last {
                        return Err(RustMsptError::InvalidConfig("mixed fallback member counts must be positive and strictly decreasing below particles_per_cluster".into()));
                    }
                    total += n;
                    last = n;
                }
                if total * (if a.variants == 0 { 32 } else { a.variants }) > 65536 {
                    return Err(RustMsptError::InvalidConfig(
                        "mixed template catalog exceeds 65536 members".into(),
                    ));
                }
            }
            if boundary.mode != BoundaryMode::Strict
                || self.position.mode != PositionMode::FeasibleUniform
                || void
                    .as_ref()
                    .is_some_and(|v| v.crossing != VoidCrossing::Forbidden)
            {
                return Err(RustMsptError::InvalidConfig("placement.aggregates currently requires strict boundary, feasible_uniform position and forbidden void crossing".into()));
            }
        }
        let free_space = self.position.free_space.clone().unwrap_or_default();
        if self.position.free_space.is_some() && self.position.mode != PositionMode::FreeSpaceGuided
        {
            return Err(RustMsptError::InvalidConfig(
                "position.free_space requires free_space_guided mode".into(),
            ));
        }
        if self.position.mode == PositionMode::FreeSpaceGuided {
            if boundary.mode != BoundaryMode::Strict
                || self.aggregates.enabled
                || void
                    .as_ref()
                    .is_some_and(|v| v.crossing != VoidCrossing::Forbidden)
                || !self.checkpoint.enabled
                || !free_space.coarse_cell_size.is_finite()
                || free_space.coarse_cell_size <= 0.0
                || !free_space.min_cell_size.is_finite()
                || free_space.min_cell_size <= 0.0
                || free_space.min_cell_size > free_space.coarse_cell_size
                || free_space.max_cells == 0
                || free_space.max_cells > 4_000_000
                || free_space.index_memory_mb == 0
                || free_space.index_memory_mb > 8192
                || free_space.candidates_per_location == 0
                || free_space.candidates_per_location > 1024
                || !free_space.exploration_fraction.is_finite()
                || !(0.0..=1.0).contains(&free_space.exploration_fraction)
            {
                return Err(RustMsptError::InvalidConfig("invalid free_space_guided settings: requires individual/strict/forbidden/checkpointed placement and bounded finite search parameters".into()));
            }
            let ext = domain.size();
            let cells = [ext.x, ext.y, ext.z].into_iter().try_fold(1usize, |n, v| {
                n.checked_mul((v / free_space.coarse_cell_size).ceil() as usize)
            });
            let capacity = free_space
                .max_cells
                .min(free_space.index_memory_mb.saturating_mul(1024 * 1024) / 512);
            if cells.is_none_or(|n| n == 0 || n > capacity) {
                return Err(RustMsptError::InvalidConfig(
                    "free-space coarse lattice exceeds memory/cell cap".into(),
                ));
            }
        }
        let initial_particles = self.initial_particles.as_ref().map(|spec| {
            let mut spec = spec.clone();
            spec.record = resolve_against(&dir, &spec.record.to_string_lossy());
            spec.report = resolve_against(&dir, &spec.report.to_string_lossy());
            spec.pending_checkpoint = spec
                .pending_checkpoint
                .as_ref()
                .map(|p| resolve_against(&dir, &p.to_string_lossy()));
            spec
        });
        if let Some(spec) = &initial_particles {
            if !spec.existing_gap.is_finite()
                || spec.existing_gap < 0.0
                || self.aggregates.enabled
                || boundary.mode != BoundaryMode::Strict
                || void
                    .as_ref()
                    .is_some_and(|v| v.crossing != VoidCrossing::Forbidden)
                || !self.checkpoint.enabled
                || !matches!(
                    self.position.mode,
                    PositionMode::FeasibleUniform | PositionMode::FreeSpaceGuided
                )
                || spec.record == outputs.record
                || spec.report == outputs.report
            {
                return Err(RustMsptError::InvalidConfig(
                    "initial_particles requires finite existing_gap>=0, exact individual placement, strict boundary, feasible_uniform, forbidden void crossing, checkpoints, and separate output files".into()));
            }
        }
        Ok(ResolvedPlacement {
            seed: self.seed,
            unit: self.frame.unit.clone(),
            domain,
            shape_files,
            shape_paths_as_written: self.shapes.files.clone(),
            selection: self.shapes.selection,
            filters: self.shapes.filters.clone(),
            void,
            distribution,
            classes,
            on_unattainable: self.size.on_unattainable,
            placement_order: self.size.placement_order,
            orientation: self.orientation.mode,
            position: ResolvedPosition {
                mode: self.position.mode,
                band,
                free_space,
            },
            boundary,
            gap_particle_particle,
            target_volume_fraction: vf,
            target_basis,
            target_tolerance: tolerance,
            budget: self.budget.clone(),
            threads: self.threads,
            outputs,
            config_path: config_path.to_path_buf(),
            aggregates: self.aggregates.clone(),
            memory: self.memory.clone(),
            checkpoint: self.checkpoint.clone(),
            initial_particles,
        })
    }
}

// AI-FUNC-SUMMARY: Build the error for a distribution parameter a kind requires but the config omits; returns RustMsptError; side effects: none.
fn missing(field: &str, kind: &str) -> RustMsptError {
    RustMsptError::InvalidConfig(format!(
        "placement.size.distribution.{field} is required for kind: {kind}"
    ))
}
