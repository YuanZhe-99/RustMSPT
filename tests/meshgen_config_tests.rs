//! G1-1 acceptance: the `mesh` subcommand config + parse-time rejects.
//!
//! Covers the frozen PLAN_mesh_generation §6.3 rejects (nonpositive domain,
//! t_sheet_factor >= t_layer_factor, eps_frac >= 0.5 * t_sheet_factor * h_min_frac,
//! duplicate component overrides) plus the partial pipeline's NotAvailable return.

use rustmspt::config::meshgen::{CoincidencePolicy, MeshGenConfig, MeshGenParams};
use rustmspt::io::save_stl;
use rustmspt::pipeline::meshgen::MeshGenPipeline;
use rustmspt::pipeline::Pipeline;
use rustmspt::types::{Mesh, Triangle, Vec3};

fn valid_minimal() -> MeshGenParams {
    // Same defaults as data/input/meshgen_config.yaml; built directly so the test
    // does not depend on a sample STL existing.
    let text = r#"
meshgen:
  inputs:
    - stl: data/input/particles.stl
  domain: { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }
  output: { vtu: data/output/mesh.vtu }
"#;
    let conf: MeshGenConfig = serde_yaml::from_str(text).unwrap();
    conf.meshgen
}

fn overrides(yaml_fragment: &str) -> MeshGenParams {
    // Fragment lines are top-level (e.g. "domain: { ... }"); indent each by two
    // spaces so they land under the `meshgen:` block.
    let indented: String = yaml_fragment
        .lines()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    let text = format!(
        "meshgen:\n  inputs:\n    - stl: data/input/particles.stl\n  domain: {{ min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }}\n  output: {{ vtu: data/output/mesh.vtu }}\n{indented}\n"
    );
    let conf: MeshGenConfig = serde_yaml::from_str(&text).unwrap();
    conf.meshgen
}

#[test]
fn example_config_loads_and_validates() {
    let text = std::fs::read_to_string("data/input/meshgen_config.yaml").unwrap();
    let conf: MeshGenConfig = serde_yaml::from_str(&text).unwrap();
    conf.meshgen.validate().expect("example config is valid");
}

#[test]
fn defaults_applied_and_valid() {
    let p = valid_minimal();
    p.validate().unwrap();
    assert_eq!(p.sizing.h_max_frac, 0.05);
    assert_eq!(p.sizing.h_min_frac, 0.002);
    assert_eq!(p.gaps.t_layer_factor, 1.0);
    assert_eq!(p.gaps.t_sheet_factor, 0.2);
    assert_eq!(p.envelope.eps_frac, 1.0e-4);
    assert!(matches!(
        p.repair.level,
        rustmspt::config::meshgen::RepairLevel::Conservative
    ));
    assert!(matches!(
        p.snapshots,
        rustmspt::config::meshgen::SnapshotMode::Key
    ));
    assert!(matches!(
        p.materials.unmapped,
        rustmspt::config::meshgen::UnmappedPolicy::Error
    ));
    assert_eq!(p.coincidence, CoincidencePolicy::Merge);
}

#[test]
fn coincidence_modes_parse_merge_reject_and_warn() {
    for (text, expected) in [
        ("merge", CoincidencePolicy::Merge),
        ("reject", CoincidencePolicy::Reject),
        ("warn", CoincidencePolicy::Warn),
    ] {
        let p = overrides(&format!("coincidence: {text}\n"));
        assert_eq!(p.coincidence, expected);
        p.validate().unwrap();
    }
}

// Unranked inputs must share one priority. Defaulting to the file index instead made every
// overlap a different-priority overlap, so R-A4 replaced the later body and R-A3 - the
// same-priority branch of SPEC_meshgen_geometry §9.2 - became unreachable from any config.
#[test]
fn resolved_priority_defaults_to_zero_for_every_input() {
    let text = "meshgen:
  inputs:
    - stl: a.stl
    - stl: b.stl
    - stl: c.stl
      priority: 2
  domain: { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }
  output: { vtu: data/output/mesh.vtu }
";
    let conf: MeshGenConfig = serde_yaml::from_str(text).unwrap();
    let p = conf.meshgen;
    let priorities: Vec<i64> = p.inputs.iter().map(|x| x.resolved_priority()).collect();
    assert_eq!(priorities, vec![0, 0, 2], "file order must not imply rank");
}

#[test]
fn rejects_nonpositive_domain() {
    let text = "meshgen:\n  inputs:\n    - stl: data/input/particles.stl\n  domain: { min: [0.0, 0.0, 0.0], max: [0.5, 0.0, 1.0] }\n  output: { vtu: data/output/mesh.vtu }\n";
    let p = serde_yaml::from_str::<MeshGenConfig>(text).unwrap().meshgen;
    let err = p.validate().unwrap_err().to_string();
    assert!(err.contains("nonpositive on axis"), "{err}");
    assert!(err.contains("axis 1"), "{err}");
}

#[test]
fn rejects_t_sheet_not_less_than_t_layer() {
    let p = overrides("gaps: { t_layer_factor: 1.0, t_sheet_factor: 1.0 }\n");
    let err = p.validate().unwrap_err().to_string();
    assert!(
        err.contains("t_sheet_factor") && err.contains("t_layer_factor"),
        "{err}"
    );
}

#[test]
fn rejects_eps_too_large() {
    let p = overrides("envelope: { eps_frac: 1.0e-3 }\n");
    let err = p.validate().unwrap_err().to_string();
    assert!(
        err.contains("eps_frac") && err.contains("load-bearing"),
        "{err}"
    );
}

// The R-A3 premise: two inputs at one priority must be *accepted*, because their overlap is
// meant to be preserved and carry both X. This was rejected as a "duplicate component
// override" until 2026-08-07, which is why no shipped mesh ever had a multi-X region key.
#[test]
fn accepts_equal_input_priorities() {
    let text = "meshgen:
  inputs:
    - stl: a.stl
      priority: 5
    - stl: b.stl
      priority: 5
  domain: { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }
  output: { vtu: data/output/mesh.vtu }
";
    let p = serde_yaml::from_str::<MeshGenConfig>(text).unwrap().meshgen;
    p.validate().expect("equal priorities are R-A3, not an error");
}

#[test]
fn rejects_negative_priority() {
    let text = "meshgen:
  inputs:
    - stl: a.stl
      priority: -1
  domain: { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }
  output: { vtu: data/output/mesh.vtu }
";
    let p = serde_yaml::from_str::<MeshGenConfig>(text).unwrap().meshgen;
    let err = p.validate().unwrap_err().to_string();
    assert!(err.contains("priority must be >= 0"), "{err}");
}

#[test]
fn rejects_duplicate_material_component_overrides() {
    let p = overrides("materials:\n  by_component: { 1: steel, 1: aluminium }\n");
    let err = p.validate().unwrap_err().to_string();
    assert!(
        err.contains("by_component") && err.contains("component 1"),
        "{err}"
    );
}

#[test]
fn rejects_empty_inputs() {
    let text = "meshgen:\n  inputs: []\n  domain: { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }\n  output: { vtu: data/output/mesh.vtu }\n";
    let p = serde_yaml::from_str::<MeshGenConfig>(text).unwrap().meshgen;
    let err = p.validate().unwrap_err().to_string();
    assert!(err.contains("at least one"), "{err}");
}

#[test]
fn rejects_wrong_domain_arity() {
    let text = "meshgen:\n  inputs:\n    - stl: data/input/particles.stl\n  domain: { min: [0.0, 0.0], max: [1.0, 1.0, 1.0] }\n  output: { vtu: data/output/mesh.vtu }\n";
    let p = serde_yaml::from_str::<MeshGenConfig>(text).unwrap().meshgen;
    let err = p.validate().unwrap_err().to_string();
    assert!(err.contains("3-component"), "{err}");
}

#[test]
fn kebab_case_unmapped_policy_parses() {
    let p = overrides("materials: { unmapped: elset-only }\n");
    p.validate().unwrap();
    assert!(matches!(
        p.materials.unmapped,
        rustmspt::config::meshgen::UnmappedPolicy::ElsetOnly
    ));
}

#[test]
fn pipeline_returns_not_available_after_validating() {
    let temp = tempfile::tempdir().unwrap();
    let stl = temp.path().join("triangle.stl");
    let output = temp.path().join("mesh.vtu");
    save_stl(
        &stl,
        &Mesh {
            vertices: vec![
                Vec3::new(0.1, 0.1, 0.5),
                Vec3::new(0.9, 0.1, 0.5),
                Vec3::new(0.5, 0.9, 0.5),
            ],
            faces: vec![Triangle { a: 0, b: 1, c: 2 }],
        },
        "triangle",
    )
    .unwrap();
    let text = format!(
        "meshgen:\n  inputs:\n    - stl: {}\n  domain: {{ min: [0,0,0], max: [1,1,1] }}\n  output: {{ vtu: {} }}\n",
        stl.display(),
        output.display(),
    );
    let conf: MeshGenConfig = serde_yaml::from_str(&text).unwrap();
    let err = MeshGenPipeline { config: conf }.run().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("not implemented") || msg.contains("not available"),
        "{msg}"
    );
    assert!(msg.contains("S0"), "{msg}");
    assert!(msg.contains("S9"), "{msg}");
}

#[test]
fn pipeline_rejects_missing_input_stl() {
    let text = "meshgen:
  inputs:
    - stl: data/input/__does_not_exist__.stl
  domain: { min: [0,0,0], max: [1,1,1] }
  output: { vtu: data/output/mesh.vtu }
";
    let conf: MeshGenConfig = serde_yaml::from_str(text).unwrap();
    let err = MeshGenPipeline { config: conf }.run().unwrap_err();
    assert!(err.to_string().contains("failed to read STL"), "{}", err);
}

// --- plan M-1.8: resolution as a background and a maximum level --------------------------

fn ladder(fragment: &str, zmax: f64) -> Result<rustmspt::config::meshgen::Resolution, String> {
    let text = format!(
        "meshgen:\n  inputs:\n    - stl: data/input/particles.stl\n  domain: {{ min: [0.0, 0.0, 0.0], max: [1.0, 1.0, {zmax}] }}\n  output: {{ vtu: data/output/mesh.vtu }}\n  sizing:\n{fragment}\n"
    );
    let conf: MeshGenConfig = serde_yaml::from_str(&text).map_err(|e| e.to_string())?;
    conf.meshgen.validate().map_err(|e| e.to_string())?;
    conf.meshgen.resolution().map_err(|e| e.to_string())
}

#[test]
fn a_consistent_background_triple_is_realised_exactly() {
    let r = ladder("    background: {cells: [20, 20, 12]}\n    max_level: 1", 0.6).unwrap();
    let l = r.ladder.unwrap();
    assert_eq!(l.counts, [20, 20, 12]);
    assert!(l.overhang.iter().all(|o| o.abs() < 1e-12), "{:?}", l.overhang);
    assert_eq!(l.level, 1);
    assert_eq!(l.root_level, 5, "20 cells need a 32-cell root");
    assert!((l.h_bg - 0.05).abs() < 1e-15);
    assert!((r.h_min_frac * 2.0 - r.h_max_frac).abs() < 1e-15);
}

#[test]
fn an_inconsistent_triple_is_rejected_naming_both_neighbours() {
    let e = ladder("    background: {cells: [20, 20, 10]}\n    max_level: 1", 0.6).unwrap_err();
    assert!(e.contains("[20, 20, 12]") && e.contains("[17, 17, 10]"), "{e}");
}

#[test]
fn level_zero_is_a_uniform_lattice_and_is_legal() {
    let r = ladder("    background: {cells: 20}\n    max_level: 0", 1.0).unwrap();
    assert_eq!(r.h_max_frac, r.h_min_frac);
    assert_eq!(r.ladder.unwrap().counts, [20, 20, 20]);
}

#[test]
fn seventeen_cells_are_not_rounded_to_a_power_of_two() {
    let l = ladder("    background: {cells: 17}\n    max_level: 2", 1.0).unwrap().ladder.unwrap();
    assert_eq!(l.counts, [17, 17, 17]);
    assert_eq!(l.root_level, 5);
    assert!((l.h_bg - 1.0 / 17.0).abs() < 1e-15);
}

#[test]
fn the_ladder_rejects_what_it_cannot_honour() {
    // both forms of one resolution
    let e = ladder("    h_max_frac: 0.05\n    background: {cells: 20}\n    max_level: 1", 1.0).unwrap_err();
    assert!(e.contains("give one"), "{e}");
    // one half only, and `auto`, before M-4.6
    assert!(ladder("    background: {cells: 20}", 1.0).unwrap_err().contains("M-4.6"));
    assert!(ladder("    background: auto\n    max_level: 2", 1.0).unwrap_err().contains("M-4.6"));
    assert!(ladder("    background: {cells: 20}\n    max_level: auto", 1.0).unwrap_err().contains("M-4.6"));
    // the level cap
    assert!(ladder("    background: {cells: 20}\n    max_level: 13", 1.0).unwrap_err().contains("cap"));
    // two background forms at once
    assert!(ladder("    background: {cells: 20, size: 0.05}\n    max_level: 1", 1.0).unwrap_err().contains("exactly one"));
    // the envelope rule, on the realised finest edge: eps_frac must stay under 0.5 * 0.2 * h_min
    let text = "meshgen:\n  inputs:\n    - stl: data/input/particles.stl\n  domain: { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }\n  output: { vtu: data/output/mesh.vtu }\n  sizing:\n    background: {cells: 20}\n    max_level: 12\n";
    let conf: MeshGenConfig = serde_yaml::from_str(text).unwrap();
    assert!(conf.meshgen.validate().unwrap_err().to_string().contains("eps_frac"));
}

#[test]
fn an_input_level_above_the_global_one_is_rejected() {
    let text = "meshgen:\n  inputs:\n    - stl: data/input/particles.stl\n      max_level: 3\n  domain: { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }\n  output: { vtu: data/output/mesh.vtu }\n  sizing:\n    background: {cells: 20}\n    max_level: 2\n";
    let conf: MeshGenConfig = serde_yaml::from_str(text).unwrap();
    assert!(conf.meshgen.validate().unwrap_err().to_string().contains("exceeds"));
}

#[test]
fn without_the_new_keys_the_fractions_resolve_unchanged() {
    let p = valid_minimal();
    let r = p.resolution().unwrap();
    assert!(r.ladder.is_none());
    assert_eq!((r.h_max_frac, r.h_min_frac), (0.05, 0.002));
}
