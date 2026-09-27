use super::acceleration::AccelerationConfig;
use super::mesh_verify::VerifyGateParams;
use crate::error::{Result, RustMsptError};
use serde::Deserialize;
use std::collections::HashMap;

// AI-FUNC-SUMMARY: Per-input surface role override (`auto` detects from closedness); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    #[default]
    Auto,
    Solid,
    Sheet,
}

// AI-FUNC-SUMMARY: S0 repair aggressiveness (`strict` rejects defects, `permissive` fixes most); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepairLevel {
    Strict,
    #[default]
    Conservative,
    Permissive,
}

// AI-FUNC-SUMMARY: G2-2 coincidence policy for overlapping surfaces (`merge` | `reject` | `warn`); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CoincidencePolicy {
    #[default]
    Merge,
    Reject,
    Warn,
}

// AI-FUNC-SUMMARY: Target solver profile controlling sheet/thin handling (`implicit` | `explicit` | `none`); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FemProfile {
    #[default]
    Implicit,
    Explicit,
    None,
}

// AI-FUNC-SUMMARY: Run-to-run reproducibility contract (`strict` bitwise, `fast` best-effort); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeterminismMode {
    #[default]
    Strict,
    Fast,
}

// AI-FUNC-SUMMARY: INP export behaviour for regions lacking a material mapping (`error` | `elset-only`); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum UnmappedPolicy {
    #[default]
    Error,
    ElsetOnly,
}

// AI-FUNC-SUMMARY: Contract snapshot emission level (`none` | `key` = s02/s05/s08/s11 | `all`); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotMode {
    None,
    #[default]
    Key,
    All,
}

// AI-FUNC-SUMMARY:
// Purpose: One STL input of the `mesh` pipeline.
// Notes: `priority` defaults to 0 for every input when omitted (lower = higher priority);
//   `kind` defaults to `auto`. Two inputs sharing a priority is legal and is the R-A3 case -
//   their overlap keeps both X (SPEC_meshgen_geometry §9.2 rows 4/5), so validate() must not
//   reject it. The default was the *file index* until 2026-08-07, which made R-A3 unreachable:
//   file order is not a statement of precedence, but R-A4 acted on it and deleted the
//   lower-ranked body wherever two inputs overlapped.
#[derive(Debug, Clone, Deserialize)]
pub struct MeshGenInput {
    pub stl: String,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub kind: InputKind,
    /// This input's own refinement cap below the background (plan M-1.8 (c)); must not exceed
    /// `sizing.max_level`.
    #[serde(default)]
    pub max_level: Option<LevelSpec>,
}

// AI-FUNC-SUMMARY: Axis-aligned generation domain; every axis must satisfy min < max (checked by validate()); side effects: none.
#[derive(Debug, Clone, Deserialize)]
pub struct MeshGenDomain {
    pub min: Vec<f64>,
    pub max: Vec<f64>,
}

// AI-FUNC-SUMMARY:
// Purpose: Sizing-field limits as fractions of the domain-box diagonal (PLAN_mesh_generation §6.3).
// Notes: Defaults match the plan sketch; validate() enforces 0 < h_min_frac < h_max_frac.
//   `grading` and `gap_cells` are the two G4-1 additions the sketch left implicit: `grading` is the
//   largest size ratio the field may develop over one element (2.0 = the 2:1 gradation of §10.6,
//   realised as the Lipschitz constant `grading - 1`), and `gap_cells` is how many elements must
//   span a gap that stays volumetric.
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "MeshGenSizingRaw")]
pub struct MeshGenSizing {
    pub h_max_frac: f64,
    pub h_min_frac: f64,
    pub chord_error_frac: f64,
    pub feature_angle_deg: f64,
    pub grading: f64,
    pub gap_cells: f64,
    pub curve_cells: f64,
    /// The background lattice (plan M-1.8, R-E4): `{cells: n}`, `{cells: [nx, ny, nz]}`,
    /// `{size: model units}`, `{size_frac: of the diagonal}`, or `auto` (M-4.6).
    pub background: Option<BackgroundSpec>,
    /// The number of refinement levels below the background, or `auto` (M-4.6).
    pub max_level: Option<LevelSpec>,
    /// Whether `h_max_frac` or `h_min_frac` was written in the config (not defaulted): the
    /// fractions and the ladder are mutually exclusive, and only an explicit key can conflict.
    pub fractions_given: bool,
}

/// The YAML form of `meshgen.sizing`; `MeshGenSizing` is built from it so an omitted fraction
/// can be told apart from a defaulted one.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct MeshGenSizingRaw {
    #[serde(default)]
    h_max_frac: Option<f64>,
    #[serde(default)]
    h_min_frac: Option<f64>,
    #[serde(default = "default_chord_error_frac")]
    chord_error_frac: f64,
    #[serde(default = "default_feature_angle_deg")]
    feature_angle_deg: f64,
    #[serde(default = "default_grading")]
    grading: f64,
    #[serde(default = "default_gap_cells")]
    gap_cells: f64,
    #[serde(default = "default_curve_cells")]
    curve_cells: f64,
    #[serde(default)]
    background: Option<BackgroundSpec>,
    #[serde(default)]
    max_level: Option<LevelSpec>,
}

impl From<MeshGenSizingRaw> for MeshGenSizing {
    // AI-FUNC-SUMMARY: Fill defaults and remember whether a fraction was written; returns MeshGenSizing; side effects: none.
    fn from(r: MeshGenSizingRaw) -> Self {
        MeshGenSizing {
            fractions_given: r.h_max_frac.is_some() || r.h_min_frac.is_some(),
            h_max_frac: r.h_max_frac.unwrap_or_else(default_h_max_frac),
            h_min_frac: r.h_min_frac.unwrap_or_else(default_h_min_frac),
            chord_error_frac: r.chord_error_frac,
            feature_angle_deg: r.feature_angle_deg,
            grading: r.grading,
            gap_cells: r.gap_cells,
            curve_cells: r.curve_cells,
            background: r.background,
            max_level: r.max_level,
        }
    }
}

// AI-FUNC-SUMMARY: `sizing.background` as written: `auto` or one explicit form; side effects: none.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum BackgroundSpec {
    Auto(String),
    Given(BackgroundForm),
}

// AI-FUNC-SUMMARY: The explicit background forms; exactly one field must be set (validated); side effects: none.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackgroundForm {
    #[serde(default)]
    pub cells: Option<CellCounts>,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(default)]
    pub size_frac: Option<f64>,
}

// AI-FUNC-SUMMARY: `cells: n` (along the longest axis) or `cells: [nx, ny, nz]`; side effects: none.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CellCounts {
    Longest(u32),
    PerAxis([u32; 3]),
}

// AI-FUNC-SUMMARY: `sizing.max_level` / `inputs[].max_level` as written: a level or `auto`; side effects: none.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LevelSpec {
    Level(u32),
    Auto(String),
}

/// The largest refinement depth below the background (plan M-1.8 (a)); the octree's own
/// `SIZING_MAX_LEVEL` restated as a cap on `L`.
pub const RESOLUTION_MAX_LEVEL: u32 = 12;

// AI-FUNC-SUMMARY:
// Purpose: The resolution ladder a config states (plan M-1.8): background edge, level, realised counts, the octree root and levels.
// Notes: Lengths `*_frac` are in the normalized frame (the domain diagonal is 1), which is the
//   frame every sizing quantity lives in; `h_bg` is in model units.
#[derive(Debug, Clone, PartialEq)]
pub struct Ladder {
    pub h_bg: f64,
    pub h_bg_frac: f64,
    pub level: u32,
    pub counts: [u32; 3],
    pub overhang: [f64; 3],
    /// Octree level of the background: the root is `h_bg * 2^m`.
    pub root_level: u32,
    pub root_frac: f64,
    /// Each input's own level (plan M-1.8 (c)), in input order; the global level where unstated.
    pub input_levels: Vec<u32>,
}

/// What the sizing stages read: the two bounds, and the ladder when one was stated.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    pub h_max_frac: f64,
    pub h_min_frac: f64,
    pub ladder: Option<Ladder>,
}

// AI-FUNC-SUMMARY:
// Purpose: Gap-field thickness factors (multiples of the local size h(x)) and the separation confidence floor.
// Notes: validate() enforces the load-bearing ordering t_sheet_factor < t_layer_factor (PLAN §6.3).
#[derive(Debug, Clone, Deserialize)]
pub struct MeshGenGaps {
    #[serde(default = "default_t_layer_factor")]
    pub t_layer_factor: f64,
    #[serde(default = "default_t_sheet_factor")]
    pub t_sheet_factor: f64,
    #[serde(default = "default_confidence_min")]
    pub confidence_min: f64,
}

// AI-FUNC-SUMMARY:
// Purpose: S8b's thin-regime gates - the FEM-aware ladder's thresholds and the band layer's
//   fallback policy (PLAN §10.11).
// Notes: `min_altitude_frac` is the altitude floor as a fraction of the local element size; 0 means
//   "take the floor from `meshgen.fem_profile`", which is off for `implicit` and 0.05 for
//   `explicit`. `forbid_volumetric_fallback` removes the ladder's rung 4, so a region that is
//   neither bandable nor collapsible is *rejected* with an actionable report rather than silently
//   kept volumetric.
//
//   `collapse_sheets` is G7-2's production rim collapse. A lattice edge whose two wall crossings
//   belong to one `Sheet` region and lie within `t_sheet` carries a single shared rim node, so the
//   gap becomes a welded sheet rather than a pair of walls (SPEC_meshgen_geometry.md §8.2, the
//   `k = 3` row) and the collapsed sheet's rim is emitted as a declared curve. It is **verified
//   clean** - `[V3]`, `[V7]` and `[V8]` all PASS, R-P2 byte-identical, element quality unchanged -
//   but it defaults **off** because the evidence is two synthetic fixtures and the acceptance case
//   that would confirm it at scale (PLAN §17.4 A-7, the near-contact gap sweep) is not built. It
//   changes meshes substantially where it fires: on the two-plate fixture 3,888 band elements
//   become a 2,592-face welded sheet. Turning it on is the frozen spec's behaviour for a region
//   below `t_sheet`; see PLAN §0.2.
#[derive(Debug, Clone, Deserialize)]
pub struct MeshGenThin {
    #[serde(default = "default_thin_enabled")]
    pub enabled: bool,
    #[serde(default = "default_band_min_dihedral_deg")]
    pub band_min_dihedral_deg: f64,
    #[serde(default = "default_band_max_ar")]
    pub band_max_ar: f64,
    #[serde(default)]
    pub min_altitude_frac: f64,
    #[serde(default = "default_band_regional_failure_share")]
    pub regional_failure_share: f64,
    #[serde(default)]
    pub forbid_volumetric_fallback: bool,
    #[serde(default)]
    pub collapse_sheets: bool,
}

// AI-FUNC-SUMMARY: Numerical envelope thickness as a fraction of the domain-box diagonal; validate() enforces eps_frac < 0.5 * t_sheet_factor * h_min_frac; side effects: none.
#[derive(Debug, Clone, Deserialize)]
pub struct MeshGenEnvelope {
    #[serde(default = "default_eps_frac")]
    pub eps_frac: f64,
}

// AI-FUNC-SUMMARY: S0 repair configuration; side effects: none.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MeshGenRepair {
    #[serde(default)]
    pub level: RepairLevel,
}

// AI-FUNC-SUMMARY:
// Purpose: Material assignments for the Abaqus INP export (PLAN §10.14).
// Notes: `by_component` deserializes through a pair list so duplicate component
//   overrides survive to validate(), which rejects them — a plain map would
//   silently keep the last entry.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MeshGenMaterials {
    #[serde(default, deserialize_with = "deserialize_component_map")]
    pub by_component: Vec<(i64, String)>,
    #[serde(default)]
    pub by_region_key: HashMap<String, String>,
    #[serde(default)]
    pub unmapped: UnmappedPolicy,
}

// AI-FUNC-SUMMARY: Output destinations; `vtu` is required, `abaqus`/`report` optional; side effects: none.
//
// `vtu` names the **delivered** mesh: a tets-only unstructured grid carrying region identity as a
// cell array (requirement R4). The mixed-cell contract document - tagged faces, rim curves, the
// tag tables S9-S11 and the INP export need - is written beside it as `<stem>_contract.vtu`. Both
// are always written; there is no setting for it, because which file is the mesh is not a matter
// of taste.
#[derive(Debug, Clone, Deserialize)]
pub struct MeshGenOutput {
    pub vtu: String,
    #[serde(default)]
    pub abaqus: Option<String>,
    #[serde(default)]
    pub report: Option<String>,
}

// AI-FUNC-SUMMARY:
// Purpose: YAML parameters for the `mesh` subcommand (PLAN_mesh_generation §6.3 config sketch).
// Notes: Call validate() after loading; deserialization alone does not enforce the
//   parse-time rejects frozen in the plan (domain positivity, the eps/t_sheet/t_layer
//   ordering, duplicate component overrides).
#[derive(Debug, Clone, Deserialize)]
pub struct MeshGenParams {
    pub inputs: Vec<MeshGenInput>,
    pub domain: MeshGenDomain,
    #[serde(default)]
    pub sizing: MeshGenSizing,
    #[serde(default)]
    pub gaps: MeshGenGaps,
    #[serde(default)]
    pub thin: MeshGenThin,
    #[serde(default)]
    pub envelope: MeshGenEnvelope,
    #[serde(default)]
    pub repair: MeshGenRepair,
    #[serde(default)]
    pub coincidence: CoincidencePolicy,
    #[serde(default)]
    pub fem_profile: FemProfile,
    #[serde(default)]
    pub determinism: DeterminismMode,
    #[serde(default)]
    pub materials: MeshGenMaterials,
    #[serde(default)]
    pub acceleration: AccelerationConfig,
    #[serde(default)]
    pub snapshots: SnapshotMode,
    pub output: MeshGenOutput,
    #[serde(default)]
    pub verify: VerifyGateParams,
}

// AI-FUNC-SUMMARY: Top-level YAML wrapper for the `meshgen:` config block; side effects: none.
#[derive(Debug, Clone, Deserialize)]
pub struct MeshGenConfig {
    pub meshgen: MeshGenParams,
}

impl MeshGenInput {
    // AI-FUNC-SUMMARY: Effective priority (the explicit value, else 0); returns i64; side effects: none.
    // Notes: takes no input index on purpose - deriving a default from file order is the defect
    //   this replaced. Unranked inputs share priority 0, so their overlaps take R-A3.
    pub fn resolved_priority(&self) -> i64 {
        self.priority.unwrap_or(0)
    }
}

impl MeshGenParams {
    // AI-FUNC-SUMMARY:
    // Purpose: Resolve the stated resolution (plan M-1.8): the two fractions, or the background-and-level ladder, into the bounds every sizing stage reads.
    // Returns: Resolution, or InvalidConfig naming the conflict (both forms given, one half of the ladder alone, `auto` before M-4.6, an inconsistent triple, a level past the cap or the index width, an input level above the global one).
    // Side effects: None.
    // Notes: `cells: [nx, ny, nz]` takes `h_bg = max_i E_i / n_i` and is accepted iff every axis
    //   realises its count; the reject names the consistent triples on either side. Counts use a
    //   relative tolerance of 1e-9 so `0.6 / 0.05` is 12, not 13. The root is `h_bg * 2^m` with
    //   `m = ceil(log2 max n_i)`, anchored at the domain minimum; `h_min = h_bg / 2^L` is exact.
    pub fn resolution(&self) -> Result<Resolution> {
        let bad = |m: String| Err(RustMsptError::InvalidConfig(m));
        let sz = &self.sizing;
        let auto_msg = "derived resolution (`auto`) lands with plan M-4.6; state both \
            meshgen.sizing.background and meshgen.sizing.max_level";
        let (background, level) = match (&sz.background, &sz.max_level) {
            (None, None) => {
                if self.inputs.iter().any(|i| i.max_level.is_some()) {
                    return bad("meshgen.inputs[].max_level needs meshgen.sizing.background and \
                        meshgen.sizing.max_level"
                        .to_string());
                }
                return Ok(Resolution {
                    h_max_frac: sz.h_max_frac,
                    h_min_frac: sz.h_min_frac,
                    ladder: None,
                });
            }
            (Some(BackgroundSpec::Auto(_)), _) | (_, Some(LevelSpec::Auto(_))) => {
                return bad(auto_msg.to_string())
            }
            (Some(BackgroundSpec::Given(b)), Some(LevelSpec::Level(l))) => (b, *l),
            _ => return bad(format!("meshgen.sizing: only one of background / max_level is given; {auto_msg}")),
        };
        if sz.fractions_given {
            return bad("meshgen.sizing: h_max_frac/h_min_frac and background/max_level are two \
                ways of stating one resolution; give one"
                .to_string());
        }
        let extent: Vec<f64> = (0..3).map(|a| self.domain.max[a] - self.domain.min[a]).collect();
        let diag = extent.iter().map(|e| e * e).sum::<f64>().sqrt();
        let longest = extent.iter().cloned().fold(0.0, f64::max);
        let given = [background.cells.is_some(), background.size.is_some(), background.size_frac.is_some()];
        if given.iter().filter(|g| **g).count() != 1 {
            return bad("meshgen.sizing.background takes exactly one of cells, size, size_frac".to_string());
        }
        let count = |e: f64, h: f64| ((e / h) * (1.0 - 1e-9)).ceil().max(1.0) as u32;
        let h_bg = match (&background.cells, background.size, background.size_frac) {
            (Some(CellCounts::Longest(n)), _, _) if *n > 0 => longest / *n as f64,
            (Some(CellCounts::PerAxis(n)), _, _) if n.iter().all(|v| *v > 0) => {
                let h = (0..3).map(|a| extent[a] / n[a] as f64).fold(0.0, f64::max);
                let realised: Vec<u32> = (0..3).map(|a| count(extent[a], h)).collect();
                if realised != n.to_vec() {
                    let h_fine = (0..3).map(|a| extent[a] / n[a] as f64).fold(f64::INFINITY, f64::min);
                    let fine: Vec<u32> = (0..3).map(|a| count(extent[a], h_fine)).collect();
                    return bad(format!(
                        "meshgen.sizing.background cells {n:?} cannot all be realised by one cubic cell on \
                         a domain of extent {extent:?}; the consistent triples on either side are {fine:?} \
                         (size {h_fine}) and {realised:?} (size {h})"
                    ));
                }
                h
            }
            (_, Some(size), _) if size > 0.0 => size,
            (_, _, Some(frac)) if frac > 0.0 => frac * diag,
            _ => return bad("meshgen.sizing.background must be positive".to_string()),
        };
        if level > RESOLUTION_MAX_LEVEL {
            return bad(format!(
                "meshgen.sizing.max_level {level} exceeds the cap {RESOLUTION_MAX_LEVEL}"
            ));
        }
        let counts = [count(extent[0], h_bg), count(extent[1], h_bg), count(extent[2], h_bg)];
        let largest = *counts.iter().max().unwrap_or(&1);
        let root_level = (largest as f64).log2().ceil().max(0.0) as u32;
        if root_level + level + 1 > 31 {
            return bad(format!(
                "meshgen.sizing: a background of {largest} cells refined {level} levels needs {} index \
                 bits; the lattice has 31",
                root_level + level + 1
            ));
        }
        for input in &self.inputs {
            match &input.max_level {
                None => {}
                Some(LevelSpec::Auto(_)) => return bad(auto_msg.to_string()),
                Some(LevelSpec::Level(l)) if *l > level => {
                    return bad(format!(
                        "meshgen.inputs[{}].max_level {l} exceeds meshgen.sizing.max_level {level}",
                        input.stl
                    ))
                }
                Some(_) => {}
            }
        }
        let h_bg_frac = h_bg / diag;
        let overhang = [0, 1, 2].map(|a| counts[a] as f64 * h_bg - extent[a]);
        Ok(Resolution {
            h_max_frac: h_bg_frac,
            h_min_frac: h_bg_frac / (1u64 << level) as f64,
            ladder: Some(Ladder {
                h_bg,
                h_bg_frac,
                level,
                counts,
                overhang,
                root_level,
                root_frac: h_bg_frac * (1u64 << root_level) as f64,
                input_levels: self
                    .inputs
                    .iter()
                    .map(|i| match i.max_level {
                        Some(LevelSpec::Level(l)) => l,
                        _ => level,
                    })
                    .collect(),
            }),
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Enforce the parse-time rejects frozen in PLAN_mesh_generation §6.3.
    // Returns: Ok(()) or InvalidConfig naming the violated rule.
    // Side effects: None.
    // Notes: The ordering eps << t_sheet < t_layer <= h is load-bearing for the
    //   numerics spec (SPEC_meshgen_numerics), so it is checked here, not assumed.
    //   Also rejects duplicate resolved input priorities and duplicate
    //   materials.by_component keys ("duplicate component overrides").
    pub fn validate(&self) -> Result<()> {
        if self.inputs.is_empty() {
            return Err(RustMsptError::InvalidConfig(
                "meshgen.inputs must list at least one STL".to_string(),
            ));
        }
        if self.domain.min.len() != 3 || self.domain.max.len() != 3 {
            return Err(RustMsptError::InvalidConfig(
                "meshgen.domain min/max must be 3-component vectors".to_string(),
            ));
        }
        for axis in 0..3 {
            if self.domain.min[axis] >= self.domain.max[axis] {
                return Err(RustMsptError::InvalidConfig(format!(
                    "meshgen.domain is nonpositive on axis {axis}: min {} must be < max {}",
                    self.domain.min[axis], self.domain.max[axis]
                )));
            }
        }
        let resolution = self.resolution()?;
        if resolution.ladder.is_none()
            && (self.sizing.h_min_frac <= 0.0 || self.sizing.h_min_frac >= self.sizing.h_max_frac)
        {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.sizing requires 0 < h_min_frac ({}) < h_max_frac ({})",
                self.sizing.h_min_frac, self.sizing.h_max_frac
            )));
        }
        if self.sizing.chord_error_frac <= 0.0 {
            return Err(RustMsptError::InvalidConfig(
                "meshgen.sizing.chord_error_frac must be positive".to_string(),
            ));
        }
        if self.sizing.feature_angle_deg <= 0.0 || self.sizing.feature_angle_deg >= 180.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.sizing.feature_angle_deg must be in (0, 180), got {}",
                self.sizing.feature_angle_deg
            )));
        }
        // `grading = 1` would make the field piecewise constant with a jump at every
        // source - the Lipschitz constant is `grading - 1`, so it must be positive.
        if self.sizing.grading <= 1.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.sizing.grading must be > 1 (2.0 is the 2:1 gradation), got {}",
                self.sizing.grading
            )));
        }
        if self.sizing.curve_cells < 1.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.sizing.curve_cells must be >= 1 (1 disables the criterion), got {}",
                self.sizing.curve_cells
            )));
        }
        // R3: a setting may not choose between a correct mesh and an incorrect one. Fewer than
        // four elements across a gap does exactly that - see `default_gap_cells` for the measured
        // step - so the floor is the threshold, not 1.
        if self.sizing.gap_cells < 4.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.sizing.gap_cells must be >= 4 elements across a volumetric gap (below \
                 that no lattice vertex lands inside the gap and its boundary is a staircase of \
                 uncut lattice faces, measured), got {}",
                self.sizing.gap_cells
            )));
        }
        if self.gaps.confidence_min <= 0.0 || self.gaps.confidence_min > 1.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.gaps.confidence_min must be in (0, 1], got {}",
                self.gaps.confidence_min
            )));
        }
        if self.gaps.t_sheet_factor >= self.gaps.t_layer_factor {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.gaps requires t_sheet_factor ({}) < t_layer_factor ({})",
                self.gaps.t_sheet_factor, self.gaps.t_layer_factor
            )));
        }
        if self.gaps.t_sheet_factor <= 0.0 {
            return Err(RustMsptError::InvalidConfig(
                "meshgen.gaps.t_sheet_factor must be positive".to_string(),
            ));
        }
        let eps_cap = 0.5 * self.gaps.t_sheet_factor * resolution.h_min_frac;
        if self.envelope.eps_frac <= 0.0 || self.envelope.eps_frac >= eps_cap {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.envelope.eps_frac ({}) must be in (0, 0.5 * t_sheet_factor * h_min_frac = {eps_cap}) \
                 - the ordering eps << t_sheet < t_layer <= h is load-bearing",
                self.envelope.eps_frac
            )));
        }
        if self.thin.band_min_dihedral_deg <= 0.0 || self.thin.band_min_dihedral_deg >= 70.5 {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.thin.band_min_dihedral_deg must lie in (0, 70.5) - 70.53 deg is the regular tet's own dihedral - got {}",
                self.thin.band_min_dihedral_deg
            )));
        }
        if self.thin.band_max_ar < 1.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.thin.band_max_ar must be at least 1.0 (the regular tet's aspect ratio), got {}",
                self.thin.band_max_ar
            )));
        }
        if self.thin.min_altitude_frac < 0.0 || self.thin.min_altitude_frac >= 1.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.thin.min_altitude_frac must lie in [0, 1), got {}",
                self.thin.min_altitude_frac
            )));
        }
        if self.thin.regional_failure_share <= 0.0 || self.thin.regional_failure_share > 1.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "meshgen.thin.regional_failure_share must lie in (0, 1], got {}",
                self.thin.regional_failure_share
            )));
        }
        // Two inputs resolving to one priority is NOT an error: it is R-A3, the case where the
        // overlap is preserved and carries both X. Rejecting it (as this did until 2026-08-07)
        // deleted a requirement rather than guarding one - it made every region key a singleton,
        // which in turn left [V6]'s "same-priority keys share one Y" clause measuring an empty
        // set. Negative priorities are the only thing worth refusing here, since Y is carried as
        // u32 in the contract VTU.
        for (i, input) in self.inputs.iter().enumerate() {
            let p = input.resolved_priority();
            if p < 0 {
                return Err(RustMsptError::InvalidConfig(format!(
                    "meshgen.inputs[{i}].priority must be >= 0, got {p}"
                )));
            }
        }
        let mut seen_components = HashMap::new();
        for (component, material) in &self.materials.by_component {
            if seen_components.insert(component, material).is_some() {
                return Err(RustMsptError::InvalidConfig(format!(
                    "meshgen.materials.by_component: duplicate override for component {component}"
                )));
            }
        }
        Ok(())
    }
}

// AI-FUNC-SUMMARY: Serde default for sizing.h_max_frac; returns 0.05; side effects: none.
fn default_h_max_frac() -> f64 {
    0.05
}

// AI-FUNC-SUMMARY: Serde default for sizing.h_min_frac; returns 0.002; side effects: none.
fn default_h_min_frac() -> f64 {
    0.002
}

// AI-FUNC-SUMMARY: Serde default for sizing.chord_error_frac; returns 0.2; side effects: none.
fn default_chord_error_frac() -> f64 {
    0.2
}

// AI-FUNC-SUMMARY: Serde default for sizing.feature_angle_deg; returns 45.0; side effects: none.
fn default_feature_angle_deg() -> f64 {
    45.0
}

// AI-FUNC-SUMMARY: Serde default for sizing.grading; returns 2.0 (the 2:1 gradation of PLAN §10.6); side effects: none.
fn default_grading() -> f64 {
    2.0
}

// AI-FUNC-SUMMARY: Serde default for sizing.curve_cells; returns 2.0, one octree level below h_max along every locked curve; side effects: none.
// Notes: 1.0 disables the criterion (it asks for `h_max`, which the ambient size already meets).
fn default_curve_cells() -> f64 {
    2.0
}

// AI-FUNC-SUMMARY: Serde default for sizing.gap_cells; returns 4.0 elements across a volumetric gap; side effects: none.
// Notes: **Four, and it is a representability threshold rather than a taste (P-4.2).** Measured on
//   A-7a and A-7b at their frozen h: `gap_cells` 2 and 3 give *identical* meshes to the digit -
//   88.717 % and 91.377 % of material-boundary area on the surface - and 4 jumps both to 99.829 %
//   and 99.892 %, cutting off-surface area by 69x and 83x. There is no gradient between 2 and 4;
//   below 4 no lattice vertex lands inside the gap, so S8 is never offered a cut and the boundary
//   is a staircase of raw lattice faces whatever the rest of the pipeline does.
fn default_gap_cells() -> f64 {
    4.0
}

// AI-FUNC-SUMMARY: Serde default for gaps.t_layer_factor; returns 1.0; side effects: none.
fn default_t_layer_factor() -> f64 {
    1.0
}

// AI-FUNC-SUMMARY: Serde default for gaps.t_sheet_factor; returns 0.2; side effects: none.
fn default_t_sheet_factor() -> f64 {
    0.2
}

// AI-FUNC-SUMMARY: Serde default for gaps.confidence_min; returns 0.9; side effects: none.
fn default_confidence_min() -> f64 {
    0.9
}

// AI-FUNC-SUMMARY: Serde default for thin.enabled; returns true (S8b runs); side effects: none.
fn default_thin_enabled() -> bool {
    true
}

// AI-FUNC-SUMMARY: Serde default for thin.band_min_dihedral_deg; returns 8.0 (PLAN §10.11); side effects: none.
fn default_band_min_dihedral_deg() -> f64 {
    8.0
}

// AI-FUNC-SUMMARY: Serde default for thin.band_max_ar; returns 20.0 (PLAN §10.11); side effects: none.
fn default_band_max_ar() -> f64 {
    20.0
}

// AI-FUNC-SUMMARY: Serde default for thin.regional_failure_share; returns 0.05 (the reference thin-feature design §3.8); side effects: none.
fn default_band_regional_failure_share() -> f64 {
    0.05
}

// AI-FUNC-SUMMARY: Serde default for envelope.eps_frac; returns 1.0e-4; side effects: none.
// Notes: The plan sketch's 1.0e-3 violates its own parse-time reject
//   (eps_frac < 0.5 * t_sheet_factor * h_min_frac = 2.0e-4 at the default gap/sizing
//   factors), so the default is lowered to keep an all-default config valid.
fn default_eps_frac() -> f64 {
    1.0e-4
}

// AI-FUNC-SUMMARY:
// Purpose: Deserialize a YAML mapping `{ component: material }` into an ordered pair
//   list, preserving duplicate component keys for validate() to reject.
// Returns: Vec of (component, material) pairs in document order.
// Side effects: None.
// Notes: The plan sketch writes materials.by_component as a map (`{ 1: steel, 2: pore }`),
//   but a HashMap target would silently drop duplicate component overrides before
//   validate() could flag them. Walking the map visitor as pairs keeps duplicates
//   while still accepting the documented map syntax.
fn deserialize_component_map<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<(i64, String)>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct ComponentMapVisitor;

    impl<'de> serde::de::Visitor<'de> for ComponentMapVisitor {
        type Value = Vec<(i64, String)>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(
                f,
                "a mapping of component id (integer) to material name (string)"
            )
        }

        fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut out = Vec::new();
            while let Some((k, v)) = map.next_entry::<i64, String>()? {
                out.push((k, v));
            }
            Ok(out)
        }
    }

    deserializer.deserialize_map(ComponentMapVisitor)
}

impl Default for MeshGenSizing {
    // AI-FUNC-SUMMARY: Rust-level Default matching the serde defaults; returns plan-sketch sizing; side effects: none.
    fn default() -> Self {
        Self {
            h_max_frac: default_h_max_frac(),
            h_min_frac: default_h_min_frac(),
            chord_error_frac: default_chord_error_frac(),
            feature_angle_deg: default_feature_angle_deg(),
            grading: default_grading(),
            gap_cells: default_gap_cells(),
            curve_cells: default_curve_cells(),
            background: None,
            max_level: None,
            fractions_given: false,
        }
    }
}

impl Default for MeshGenGaps {
    // AI-FUNC-SUMMARY: Rust-level Default matching the serde defaults; returns plan-sketch gap factors; side effects: none.
    fn default() -> Self {
        Self {
            t_layer_factor: default_t_layer_factor(),
            t_sheet_factor: default_t_sheet_factor(),
            confidence_min: default_confidence_min(),
        }
    }
}

impl Default for MeshGenThin {
    // AI-FUNC-SUMMARY: Rust-level Default matching the serde defaults; returns the frozen §10.11 gates; side effects: none.
    fn default() -> Self {
        Self {
            enabled: default_thin_enabled(),
            band_min_dihedral_deg: default_band_min_dihedral_deg(),
            band_max_ar: default_band_max_ar(),
            min_altitude_frac: 0.0,
            regional_failure_share: default_band_regional_failure_share(),
            forbid_volumetric_fallback: false,
            collapse_sheets: false,
        }
    }
}

impl Default for MeshGenEnvelope {
    // AI-FUNC-SUMMARY: Rust-level Default matching the serde default; returns eps_frac 1.0e-4; side effects: none.
    fn default() -> Self {
        Self {
            eps_frac: default_eps_frac(),
        }
    }
}
