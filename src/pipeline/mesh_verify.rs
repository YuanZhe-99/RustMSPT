use super::Pipeline;
use crate::config::mesh_verify::{MeshVerifyConfig, VerifyGateParams};
use crate::error::{Result, RustMsptError};
use crate::io::vtu::{load_vtu, save_vtu, VtuEncoding};
use crate::meshgen::verify::{
    annotate, report_to_json, report_to_log, verify_with_options, VerifyGates, VerifyOptions,
    VerifyReport,
};
use std::path::Path;

// AI-FUNC-SUMMARY: Pipeline wrapper for the mesh-verify subcommand; holds the parsed config; side effects: none until run().
pub struct MeshVerifyPipeline {
    pub config: MeshVerifyConfig,
}

// AI-FUNC-SUMMARY:
// Purpose: Overlay YAML gate overrides onto the contract defaults.
// Inputs: the `verify:` config block.
// Returns: VerifyGates.
// Side effects: None.
pub fn gates_from_config(p: &VerifyGateParams) -> VerifyGates {
    let d = VerifyGates::default();
    VerifyGates {
        max_ar_warn: p.max_ar_warn.unwrap_or(d.max_ar_warn),
        min_dihedral_deg: p.min_dihedral_deg.unwrap_or(d.min_dihedral_deg),
        low_dihedral_deg: p.low_dihedral_deg.unwrap_or(d.low_dihedral_deg),
        low_dihedral_share: p.low_dihedral_share.unwrap_or(d.low_dihedral_share),
        duplicate_node_tol_frac: p
            .duplicate_node_tol_frac
            .unwrap_or(d.duplicate_node_tol_frac),
        plane_tol_frac: p.plane_tol_frac.unwrap_or(d.plane_tol_frac),
        hanging_tol_frac: p.hanging_tol_frac.unwrap_or(d.hanging_tol_frac),
        surface_distance_frac: p.surface_distance_frac.unwrap_or(d.surface_distance_frac),
        max_items_per_section: p.max_items_per_section.unwrap_or(d.max_items_per_section),
        expected_partitions: p.expected_partitions.or(d.expected_partitions),
        warn_is_fatal: p.warn_is_fatal.unwrap_or(d.warn_is_fatal),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Load a VTU, run the verification catalog, and write the requested outputs.
// Inputs: input path, gates, and optional report/json/annotate destinations.
// Returns: the VerifyReport (caller decides the exit status).
// Side effects: Reads the input VTU; writes log/JSON/annotated VTU when paths are given;
//   prints the human log to stdout.
// Notes: The annotated VTU is written in ascii when the source path ends in `.ascii.vtu`,
//   otherwise in appended-raw, matching the writer's default.
#[allow(clippy::too_many_arguments)]
pub fn verify_file(
    input: &Path,
    gates: &VerifyGates,
    report_path: Option<&Path>,
    json_path: Option<&Path>,
    annotate_path: Option<&Path>,
    surfaces: &[crate::config::mesh_verify::MeshVerifySurface],
) -> Result<VerifyReport> {
    let doc = load_vtu(input)?;
    doc.validate()?;
    let mut options = VerifyOptions::from_path(input);
    // [V5]'s input side. Loaded here rather than in `verify.rs` so the check stays a
    // pure function of geometry it is handed - it never reads a file itself, which is
    // what lets the same check run inside the mesh pipeline at S11.
    for surface in surfaces {
        let mesh = crate::io::stl::load_stl(Path::new(surface.stl()))?;
        let tris: Vec<[crate::types::Vec3; 3]> = mesh
            .faces
            .iter()
            .map(|f| [mesh.vertices[f.a], mesh.vertices[f.b], mesh.vertices[f.c]])
            .collect();
        // Priority resolves exactly as `meshgen.inputs` resolves it - 0 unless stated - so the
        // surfaces listed here mean the same thing they meant to the mesher. Deriving it from
        // the file index (as this did until 2026-08-07) made [V5] mask a body the mesher had
        // *not* overridden, reporting A-4's contained cube as owning nothing at all.
        options.surfaces.push(crate::meshgen::verify::SurfaceComponent {
            priority: surface.resolved_priority(),
            // Closed-surface test by edge parity: an open sheet can never mask another
            // body, so only closed components take part in the priority mask.
            closed: is_closed_soup(&mesh),
            tris,
        });
    }
    let mut report = verify_with_options(&doc, gates, options);
    report.input = input.to_string_lossy().to_string();

    let log = report_to_log(&report);
    print!("{log}");
    if let Some(p) = report_path {
        write_with_parent(p, log.as_bytes())?;
    }
    if let Some(p) = json_path {
        write_with_parent(p, report_to_json(&report).as_bytes())?;
    }
    if let Some(p) = annotate_path {
        let annotated = annotate(&doc, &report);
        annotated.validate()?;
        let encoding = if p.to_string_lossy().ends_with(".ascii.vtu") {
            VtuEncoding::Ascii
        } else {
            VtuEncoding::AppendedRaw
        };
        if let Some(dir) = p.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        save_vtu(p, &annotated, encoding)?;
        println!("[mesh-verify] wrote annotated mesh {}", p.display());
    }
    Ok(report)
}

// AI-FUNC-SUMMARY: Write bytes, creating the parent directory when needed; returns Ok(()) or Io error; side effects: writes to disk.
fn write_with_parent(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    std::fs::write(path, bytes)?;
    println!("[mesh-verify] wrote {}", path.display());
    Ok(())
}

impl Pipeline for MeshVerifyPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Run the standalone verifier over the configured mesh.
    // Returns: Ok(()) when the mesh passes; InvalidMesh error when a gate fails.
    // Side effects: Reads the input VTU, writes the configured outputs, prints the log.
    // Notes: The error return is what gives the subcommand its nonzero exit status
    //   (SPEC_meshgen_contracts §4); the human log has already been printed by then.
    fn run(&self) -> Result<()> {
        let p = &self.config.mesh_verify;
        let gates = gates_from_config(&p.verify);
        let report = verify_file(
            Path::new(&p.input),
            &gates,
            p.report.as_deref().map(Path::new),
            p.json.as_deref().map(Path::new),
            p.annotate.as_deref().map(Path::new),
            &p.surfaces,
        )?;
        if !report.passed() {
            return Err(RustMsptError::InvalidMesh(format!(
                "mesh-verify: {} failed ({} FAIL, {} WARN) — see the report for details",
                p.input, report.fail, report.warn
            )));
        }
        Ok(())
    }
}

// AI-FUNC-SUMMARY: Whether an STL's triangle soup is a closed surface (every edge used twice); returns bool; side effects: none.
fn is_closed_soup(mesh: &crate::types::Mesh) -> bool {
    use std::collections::HashMap;
    let mut edges: HashMap<(usize, usize), i32> = HashMap::new();
    for f in &mesh.faces {
        for (a, b) in [(f.a, f.b), (f.b, f.c), (f.c, f.a)] {
            *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    !mesh.faces.is_empty() && edges.values().all(|n| *n == 2)
}
