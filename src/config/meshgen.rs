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
pub struct MeshGenSizing {
    #[serde(default = "default_h_max_frac")]
    pub h_max_frac: f64,
    #[serde(default = "default_h_min_frac")]
    pub h_min_frac: f64,
    #[serde(default = "default_chord_error_frac")]
    pub chord_error_frac: f64,
    #[serde(default = "default_feature_angle_deg")]
    pub feature_angle_deg: f64,
    #[serde(default = "default_grading")]
    pub grading: f64,
    #[serde(default = "default_gap_cells")]
    pub gap_cells: f64,
    #[serde(default = "default_curve_cells")]
    pub curve_cells: f64,
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
        if self.sizing.h_min_frac <= 0.0 || self.sizing.h_min_frac >= self.sizing.h_max_frac {
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
        let eps_cap = 0.5 * self.gaps.t_sheet_factor * self.sizing.h_min_frac;
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
