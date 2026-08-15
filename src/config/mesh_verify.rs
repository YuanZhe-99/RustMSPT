use serde::Deserialize;

// AI-FUNC-SUMMARY:
// Purpose: YAML gate overrides for the verification catalog (SPEC_meshgen_contracts §4).
// Notes: Every length tolerance is a fraction of the mesh bounding-box diagonal, so gates
//   are scale-invariant. Omitted fields keep the contract defaults.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct VerifyGateParams {
    #[serde(default)]
    pub max_ar_warn: Option<f64>,
    #[serde(default)]
    pub min_dihedral_deg: Option<f64>,
    #[serde(default)]
    pub low_dihedral_deg: Option<f64>,
    #[serde(default)]
    pub low_dihedral_share: Option<f64>,
    #[serde(default)]
    pub duplicate_node_tol_frac: Option<f64>,
    #[serde(default)]
    pub plane_tol_frac: Option<f64>,
    #[serde(default)]
    pub hanging_tol_frac: Option<f64>,
    #[serde(default)]
    pub max_items_per_section: Option<usize>,
    #[serde(default)]
    pub expected_partitions: Option<i64>,
    #[serde(default)]
    pub warn_is_fatal: Option<bool>,
    /// [V5]'s two-sided surface-conformance gate, as a fraction of the mesh bbox diagonal.
    #[serde(default)]
    pub surface_distance_frac: Option<f64>,
    /// [V13]'s "on the surface" tolerance, as a fraction of the face's own edge length.
    #[serde(default)]
    pub interface_on_surface_frac: Option<f64>,
    /// [V13]'s displacement gate on the area-weighted signed offset, same denominator.
    #[serde(default)]
    pub interface_offset_frac: Option<f64>,
}

// AI-FUNC-SUMMARY:
// Purpose: One [V5] input surface: a path, optionally with the priority it was meshed at.
// Notes: Accepts a bare string so `surfaces: [a.stl, b.stl]` keeps working; both forms
//   resolve priority 0 by default, matching `MeshGenInput::resolved_priority`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MeshVerifySurface {
    Path(String),
    Ranked {
        stl: String,
        #[serde(default)]
        priority: Option<u32>,
    },
}

impl MeshVerifySurface {
    // AI-FUNC-SUMMARY: The STL path of this entry; returns &str; side effects: none.
    pub fn stl(&self) -> &str {
        match self {
            Self::Path(p) => p,
            Self::Ranked { stl, .. } => stl,
        }
    }

    // AI-FUNC-SUMMARY: Effective priority (the explicit value, else 0); returns u32; side effects: none.
    pub fn resolved_priority(&self) -> u32 {
        match self {
            Self::Path(_) => 0,
            Self::Ranked { priority, .. } => priority.unwrap_or(0),
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: YAML parameters for the mesh-verify subcommand (input VTU, report outputs, gates).
// Notes: `report`/`json`/`annotate` are optional; when all three are omitted the human log
//   goes to stdout only. Exit status is nonzero on any FAIL (or gated-fatal WARN).
#[derive(Debug, Clone, Deserialize)]
pub struct MeshVerifyParams {
    pub input: String,
    #[serde(default)]
    pub report: Option<String>,
    #[serde(default)]
    pub json: Option<String>,
    #[serde(default)]
    pub annotate: Option<String>,
    #[serde(default)]
    pub verify: VerifyGateParams,
    /// Input surface meshes (STL) the mesh is supposed to follow. Supplying them turns
    /// [V5] geometric conformance on; omitting them leaves it SKIPPED with the reason.
    /// Each entry is either a bare path or `{stl, priority}`; priority must mirror the
    /// `meshgen.inputs` priority the mesh was built with, since [V5]'s expected volume is
    /// the *priority-resolved* one. It defaults to 0 for every surface, the same default
    /// `MeshGenInput::resolved_priority` uses.
    #[serde(default)]
    pub surfaces: Vec<MeshVerifySurface>,
}

// AI-FUNC-SUMMARY: Top-level YAML wrapper for the `mesh_verify:` config block; side effects: none.
#[derive(Debug, Clone, Deserialize)]
pub struct MeshVerifyConfig {
    pub mesh_verify: MeshVerifyParams,
}
