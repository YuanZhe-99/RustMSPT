use crate::error::Result;
use crate::io::sha256_file;
use crate::version::BuildIdentity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The build identity as it appears in a record or report.
///
/// An owned copy of `BuildIdentity`, whose fields are `&'static str` because they
/// come from compile-time environment variables. Those cannot deserialize into
/// borrowed statics, and the record has to be readable back: the reconstruction
/// check parses it and rebuilds every particle from what it says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRecord {
    pub name: String,
    pub version: String,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    pub features: Vec<String>,
    pub target: Option<String>,
    pub host: Option<String>,
    pub profile: Option<String>,
}

impl From<&BuildIdentity> for ToolRecord {
    // AI-FUNC-SUMMARY: Copy a build identity into owned strings for serialization; returns ToolRecord; side effects: none.
    fn from(id: &BuildIdentity) -> Self {
        ToolRecord {
            name: id.name.to_string(),
            version: id.version.to_string(),
            git_commit: id.git_commit.map(str::to_string),
            git_dirty: id.git_dirty,
            features: id.features.iter().map(|f| f.to_string()).collect(),
            target: id.target.map(str::to_string),
            host: id.host.map(str::to_string),
            profile: id.profile.map(str::to_string),
        }
    }
}

pub const RECORD_SCHEMA: &str = "rustmspt.placement.record/1";
pub const REPORT_SCHEMA: &str = "rustmspt.placement.report/1";

/// The fixed vocabulary a run may stop with.
///
/// Four words, no more: a consumer's adapter is written against this list, and a
/// fifth value would reach it as an unknown one. "Placed everything I planned but
/// lost volume to the boundary" is `target_reached` with a deficit recorded in
/// `stop_detail`, not a new word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    TargetReached,
    BudgetExhausted,
    DistributionUnattainable,
    NoFeasiblePlacement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameRecord {
    pub unit: String,
    pub origin: [f64; 3],
    pub axis_order: String,
    pub handedness: String,
    pub domain: DomainRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainRecord {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseRecord {
    pub id: u8,
    pub name: String,
    pub geometry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlap_owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRecord {
    pub index: usize,
    pub path: String,
    pub resolved_path: String,
    pub sha256: String,
    pub bytes: u64,
    pub shells_found: usize,
    pub shells_kept: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceShapeRecord {
    pub source_index: usize,
    pub path: String,
    pub sha256: String,
    pub shell_index: usize,
    pub shell_sha256: String,
    pub shell_centroid: [f64; 3],
    pub shell_volume: f64,
    pub shell_equivalent_diameter: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationRecord {
    /// Scalar first, unit, canonicalised to w >= 0. This is the authoritative
    /// value; `matrix` is derived from it and stored only because it is what a
    /// reader wants to look at.
    pub quaternion: [f64; 4],
    pub matrix: [[f64; 3]; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeRecord {
    /// The closed shell's volume after scaling. No domain clip, no void subtracted.
    pub full: f64,
    /// Clipped to the domain. Gross: any part inside the void is still counted here.
    pub in_domain: f64,
    /// `in_domain` minus the part the void owns. This is what sums to the solid phase.
    pub in_domain_solid: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClippedRecord {
    pub any: bool,
    pub faces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleRecord {
    pub entity_id: String,
    pub acceptance_index: usize,
    pub source_shape: SourceShapeRecord,
    pub scale: f64,
    pub rotation: RotationRecord,
    pub translation: [f64; 3],
    pub equivalent_diameter: f64,
    pub volume: VolumeRecord,
    pub clipped: ClippedRecord,
    pub void_overlap_volume: f64,
    pub size_class: usize,
    /// Half-open `[start, end)` into the merged particle STL's triangle list, so a
    /// reader can pull one particle's geometry out without reconstructing it.
    pub triangle_range: [usize; 2],
    pub bbox: DomainRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordFile {
    pub schema_version: String,
    pub tool: ToolRecord,
    pub seed: u64,
    pub rng: String,
    pub frame: FrameRecord,
    pub conventions: BTreeMap<String, String>,
    pub phases: Vec<PhaseRecord>,
    pub sources: Vec<SourceRecord>,
    pub particles: Vec<ParticleRecord>,
}

// AI-FUNC-SUMMARY:
// Purpose: State, in the record itself, every convention a reader needs to reconstruct a particle.
// Inputs: the frame unit and the largest domain extent, for the reconstruction tolerance.
// Returns: the conventions map, in a fixed key order.
// Side effects: None.
// Notes: A BTreeMap, not a HashMap: the record must be byte-identical between two runs of the same
// seed, and HashMap iteration order is not. These sentences are the contract - a consumer that
// reconstructs a particle differently from what they say will get a plausible, wrong particle
// rather than an error, so they are written out rather than left to the documentation.
pub fn conventions(unit: &str, max_extent: f64) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert(
        "rotation".to_string(),
        "unit quaternion, component order w,x,y,z (scalar first), canonicalised to w >= 0; \
         `matrix` is derived from it and must agree"
            .to_string(),
    );
    m.insert(
        "transform".to_string(),
        "p_world = R(q) * (scale * (p_source - shell_centroid)) + translation".to_string(),
    );
    m.insert(
        "shell_centroid".to_string(),
        "the volume centroid of the closed source shell, by the signed-tetrahedron formula. Use \
         the value recorded here rather than recomputing one: a vertex mean is a different point."
            .to_string(),
    );
    m.insert(
        "translation".to_string(),
        format!("the placed particle's volume centroid, in the run frame, in {unit}"),
    );
    m.insert(
        "volume".to_string(),
        "in_domain_solid = in_domain - the part of the particle the void owns; in_domain is gross"
            .to_string(),
    );
    m.insert(
        "stl_precision".to_string(),
        format!(
            "the merged STL stores float32 with zero normals; this record stores float64. \
             Reconstruction agrees to within {:.3e} in this frame, which is over a hundred times \
             the float32 storage error and far below any transform mistake.",
            1e-5 * max_extent
        ),
    );
    m.insert(
        "triangle_range".to_string(),
        "half-open [start, end) into the merged STL's triangle list".to_string(),
    );
    m
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub role: String,
    pub path: String,
    /// `None` for the report itself, which cannot contain its own digest.
    pub sha256: Option<String>,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeClassRow {
    pub class: usize,
    pub lo: f64,
    pub hi: f64,
    pub target_frequency: f64,
    pub target_count: usize,
    pub drawn: usize,
    pub placed: usize,
    pub shortfall: usize,
    pub top_up_drawn: usize,
    pub top_up_placed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRecord {
    pub threads: usize,
    pub elapsed_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportFile {
    pub schema_version: String,
    /// `running` while the run is in flight, `finished` once it ends. The report is
    /// written twice so a run that is killed still leaves evidence of what it was.
    pub status: String,
    pub tool: ToolRecord,
    pub seed: u64,
    pub rng: String,
    /// The only volatile object in the file. A determinism check compares two
    /// reports with this key removed.
    pub runtime: RuntimeRecord,
    pub config: FileEntry,
    pub inputs: Vec<FileEntry>,
    pub frame: FrameRecord,
    pub shapes: ShapesReport,
    pub target: TargetReport,
    pub plan: PlanReport,
    pub actual: ActualReport,
    pub size_classes: Vec<SizeClassRow>,
    pub top_up: TopUpReport,
    pub attempts: AttemptsReport,
    pub rejections: BTreeMap<String, usize>,
    pub budget: BudgetReport,
    pub samplers: BTreeMap<String, String>,
    pub stop_reason: Option<StopReason>,
    pub stop_detail: BTreeMap<String, String>,
    pub outputs: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapesReport {
    pub files: usize,
    pub shells_total: usize,
    pub shells_rejected: Vec<RejectedShellRow>,
    /// The largest ratio of a shell's bounding radius to half its equivalent
    /// diameter. Scaling preserves shape, so a value well above 1 says a particle
    /// "of diameter d" reaches much further than d/2 from its centre.
    pub max_extent_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedShellRow {
    pub source_index: usize,
    pub shell_index: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetReport {
    pub volume_fraction: f64,
    pub basis: String,
    pub basis_volume: f64,
    pub tolerance: f64,
    pub distribution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanReport {
    pub planned_particles: usize,
    pub planned_volume: f64,
    pub planned_volume_error: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActualReport {
    pub particles: usize,
    pub volume_in_domain: f64,
    pub volume_in_domain_solid: f64,
    pub volume_fraction_domain: f64,
    pub volume_fraction_solid: f64,
    pub median_diameter: Option<f64>,
    pub mean_diameter: Option<f64>,
    pub sigma_log_diameter: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopUpReport {
    pub batches: usize,
    pub drawn: usize,
    pub placed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptsReport {
    pub total: usize,
    pub per_particle_budget: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetReport {
    pub total_attempts: usize,
    pub wall_time_s: Option<f64>,
    pub consumed_attempts: usize,
    pub elapsed_s: f64,
    pub max_top_up_batches: usize,
}

// AI-FUNC-SUMMARY:
// Purpose: Write a JSON value to disk, creating parent directories.
// Inputs: the path and the value.
// Returns: Ok(()) on success.
// Side effects: Creates directories and writes the file.
// Notes: Pretty-printed with a trailing newline, so the file is diffable and a determinism check
// can compare it byte for byte.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(value).map_err(|e| {
        crate::error::RustMsptError::InvalidConfig(format!("cannot serialize {}: {e}", path.display()))
    })?;
    text.push('\n');
    std::fs::write(path, text)?;
    Ok(())
}

// AI-FUNC-SUMMARY:
// Purpose: Describe a written output file for the report's manifest.
// Inputs: the role name and the path.
// Returns: a FileEntry with the digest and size, or with both None when the file cannot be read.
// Side effects: Reads the file to hash it.
// Notes: The report lists itself with a null digest. A file cannot contain its own hash, and an
// adapter that verifies every listed digest would otherwise choke on the one entry that cannot
// have one.
pub fn describe_output(role: &str, path: &Path) -> FileEntry {
    match sha256_file(path) {
        Ok((sha256, bytes)) => FileEntry {
            role: role.to_string(),
            path: path.to_string_lossy().to_string(),
            sha256: Some(sha256),
            bytes: Some(bytes),
        },
        Err(_) => FileEntry {
            role: role.to_string(),
            path: path.to_string_lossy().to_string(),
            sha256: None,
            bytes: None,
        },
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Write the per-size-class comparison of target against actual.
// Inputs: the destination and the rows.
// Returns: Ok(()) on success.
// Side effects: Creates parent directories and writes the CSV.
// Notes: Top-up columns are kept apart from the target ones. Merging them would hide exactly the
// thing the file exists to show: whether a shortfall in one class was quietly made up elsewhere.
pub fn write_size_distribution_csv(path: &Path, rows: &[SizeClassRow]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = String::from(
        "class,lo,hi,target_frequency,target_count,drawn,placed,shortfall,top_up_drawn,top_up_placed\n",
    );
    for r in rows {
        out.push_str(&format!(
            "{},{:.12},{:.12},{:.12},{},{},{},{},{},{}\n",
            r.class,
            r.lo,
            r.hi,
            r.target_frequency,
            r.target_count,
            r.drawn,
            r.placed,
            r.shortfall,
            r.top_up_drawn,
            r.top_up_placed
        ));
    }
    std::fs::write(path, out)?;
    Ok(())
}
