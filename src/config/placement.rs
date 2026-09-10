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
    EqualWidth { count: usize },
}

#[derive(Debug, Clone)]
pub struct ResolvedPosition {
    pub mode: PositionMode,
    pub band: Option<(f64, f64)>,
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
    pub fn validate(self, config_path: &Path) -> Result<ResolvedPlacement> {
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
                    Some(o) => Some(require_positive("void.overlap_volume.voxel_size", o.voxel_size)?),
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
                    d.sigma_log.ok_or_else(|| missing("sigma_log", "lognormal"))?,
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
            (None, ResolvedDistribution::Lognormal { .. }) => ResolvedClasses::EqualWidth { count: 10 },
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
            (PositionMode::FeasibleUniform, Some(_)) => {
                return Err(RustMsptError::InvalidConfig(
                    "placement.position.band is only read for mode: void_neighbourhood".to_string(),
                ))
            }
            (PositionMode::FeasibleUniform, None) => None,
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
            Some(v) => Some(require_positive("outputs.voxel_labels.voxel_size", v.voxel_size)?),
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
        })
    }
}

// AI-FUNC-SUMMARY: Build the error for a distribution parameter a kind requires but the config omits; returns RustMsptError; side effects: none.
fn missing(field: &str, kind: &str) -> RustMsptError {
    RustMsptError::InvalidConfig(format!(
        "placement.size.distribution.{field} is required for kind: {kind}"
    ))
}
