//! GA-2 acceptance: the corrupted-fixture suite frozen in
//! `SPEC_meshgen_contracts.md` §6, plus geometry-only mode and report contracts.
//!
//! The manifest below is the contract: for each fixture, the set of check codes
//! that must fire is asserted **exactly**, so a new false positive fails the suite
//! just as loudly as a missed defect.

use rustmspt::io::vtu::{load_vtu, save_vtu, ArrayData, DataArray, VtuDoc, VtuEncoding, VTK_TETRA};
use rustmspt::meshgen::predicates::{orient3d, orient3d_sign_test, tet_quality};
use rustmspt::meshgen::verify::{
    annotate, report_to_json, report_to_log, verify, CheckStatus, VerifyGates,
};
use rustmspt::types::Vec3;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from("data/fixtures/meshgen").join(format!("{name}.vtu"))
}

fn run(name: &str) -> rustmspt::meshgen::verify::VerifyReport {
    let doc = load_vtu(&fixture(name)).expect("fixture loads");
    doc.validate().expect("fixture is structurally valid");
    verify(&doc, &VerifyGates::default())
}

/// (fixture, the check its defect is *named for*, the exact set of WARN/FAIL codes
/// it must produce). The full set is asserted exactly, so a new false positive fails
/// the suite as loudly as a missed defect; the primary code is asserted separately so
/// a fixture can never silently stop testing the check it exists for.
///
/// Several fixtures fire a justified cascade: a defect that breaks face matching also
/// disconnects the flood fill, and a duplicated node also lands on its twin's faces.
/// Those are consequences of the single injected defect, not extra defects.
const MANIFEST: &[(&str, Option<&str>, &[&str])] = &[
    ("good_cube", None, &[]),
    (
        "bad_inverted_tet",
        Some("V1.negative_volume"),
        &["V1.negative_volume"],
    ),
    (
        "bad_duplicate_node",
        Some("V2.duplicate_node"),
        &[
            "V2.duplicate_node",
            "V3.boundary_leak",
            "V3.hanging_node",
            // A duplicated node splits the adjacency of every interface face that
            // referenced it: one copy keeps the tet, the other keeps the tagged face, so
            // the volume is open along the interface there. G7-2 made that a FAIL.
            "V3.interface_crack",
            "V3.non_manifold_edge",
            "V8.partition_mismatch",
        ],
    ),
    (
        "bad_hanging_node",
        Some("V3.hanging_node"),
        &[
            "V3.boundary_leak",
            "V3.hanging_node",
            "V3.non_manifold_edge",
            "V8.partition_mismatch",
        ],
    ),
    (
        "bad_triple_face",
        Some("V3.multi_shared_face"),
        &[
            "V3.boundary_leak",
            "V3.multi_shared_face",
            "V3.non_manifold_edge",
        ],
    ),
    (
        "bad_boundary_leak",
        Some("V3.boundary_leak"),
        &["V3.boundary_leak"],
    ),
    (
        "bad_unwelded_sheet",
        Some("V7.unwelded_sheet_face"),
        &[
            "V2.duplicate_node",
            "V3.hanging_node",
            "V7.unwelded_sheet_face",
            "V8.pinhole_sheet",
        ],
    ),
    // the one-layer band check landed with G7-1; the slab's thin elements also
    // legitimately trip the [V4] dihedral gates
    (
        "bad_stacked_band",
        Some("V7.band_layers"),
        &[
            // The hand-built band fixture does not extend past its own sample, so its
            // outermost tagged faces have one adjacent tet. Real pipeline output reports
            // zero of these on all seven acceptance cases.
            "V3.interface_crack",
            "V4.low_dihedral_share",
            "V4.min_dihedral",
            "V7.band_layers",
        ],
    ),
    (
        "bad_partition_id",
        Some("V8.partition_mismatch"),
        &["V3.boundary_leak", "V8.partition_mismatch"],
    ),
    (
        "bad_region_key",
        Some("V6.illegal_region_key"),
        &["V6.illegal_region_key"],
    ),
    (
        "bad_pinhole_sheet",
        Some("V8.pinhole_sheet"),
        &["V8.pinhole_sheet"],
    ),
    (
        "bad_curve_node_id",
        Some("V9.curve_node_id"),
        &["V9.curve_node_id"],
    ),
    (
        "bad_radial_patches",
        Some("V9.radial_patches"),
        &["V9.radial_patches"],
    ),
    (
        // Opening the fan means removing a tet, which necessarily leaves the two faces it
        // alone carried single-sided - the same justified cascade `bad_partition_id` has.
        "bad_open_junction_fan",
        Some("V9.junction_fan"),
        &["V3.boundary_leak", "V9.junction_fan"],
    ),
];

#[test]
fn corrupted_fixture_suite_fires_exactly_its_manifest() {
    for (name, _, expected) in MANIFEST {
        let report = run(name);
        let fired = report.fired_codes();
        let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            fired, want,
            "fixture {name}: fired {fired:?} but the manifest expects {want:?}"
        );
    }
}

#[test]
fn each_fixture_triggers_the_check_it_is_named_for() {
    for (name, primary, _) in MANIFEST {
        let Some(primary) = primary else { continue };
        let report = run(name);
        assert!(
            report.fired_codes().iter().any(|c| c == primary),
            "fixture {name} must trigger {primary}, but fired {:?}",
            report.fired_codes()
        );
    }
}

#[test]
fn reference_fixture_passes_every_runnable_check() {
    let report = run("good_cube");
    assert_eq!(report.fail, 0, "reference fixture must not fail any check");
    assert!(report.passed());
    assert_eq!(report.exit_code(), 0);
    for s in &report.sections {
        assert!(
            s.status == CheckStatus::Pass || s.status == CheckStatus::Skipped,
            "section {} is {:?} on the reference fixture",
            s.id,
            s.status
        );
    }
}

#[test]
fn corrupted_fixtures_all_fail_the_gate() {
    for (name, primary, _) in MANIFEST
        .iter()
        .filter(|(n, p, _)| *n != "good_cube" && p.is_some())
    {
        let report = run(name);
        assert!(
            !report.passed(),
            "fixture {name} (expecting {primary:?}) must not pass the gate"
        );
        assert_eq!(report.exit_code(), 1);
    }
}

#[test]
fn every_catalog_section_is_reported_and_skips_name_a_reason() {
    let report = run("good_cube");
    let ids: Vec<&str> = report.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["V1", "V2", "V3", "V4", "V5", "V6", "V7", "V8", "V9", "V10", "V11", "V12"],
        "the report must cover the whole catalog in contract order"
    );
    for s in &report.sections {
        if s.status == CheckStatus::Skipped {
            let reason = s.skipped_reason.as_deref().unwrap_or("");
            assert!(
                !reason.is_empty(),
                "section {} is SKIPPED without naming a reason",
                s.id
            );
        }
    }
    assert_eq!(report.checks_run + report.checks_skipped, 12);
}

/// A plain tet mesh with no contract arrays at all: the geometry checks must still
/// run, and everything else must degrade to SKIPPED with a named reason.
#[test]
fn external_plain_tet_vtu_verifies_in_geometry_only_mode() {
    let doc = VtuDoc {
        points: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ],
        connectivity: vec![0, 1, 2, 3],
        offsets: vec![4],
        types: vec![VTK_TETRA],
        ..Default::default()
    };
    doc.validate().unwrap();
    let report = verify(&doc, &VerifyGates::default());

    for id in ["V1", "V2", "V4"] {
        assert_eq!(
            report.section(id).unwrap().status,
            CheckStatus::Pass,
            "{id} must run on an external tet-only VTU"
        );
    }
    // no domain metadata and no tags: the four faces are legitimately unclassifiable,
    // so V3 reports them rather than staying silent
    assert!(report.section("V3").is_some());
    for id in ["V6", "V7"] {
        let s = report.section(id).unwrap();
        assert_eq!(s.status, CheckStatus::Skipped, "{id} must skip");
        assert!(s.skipped_reason.as_deref().unwrap().contains("absent"));
    }

    // and it must survive a round-trip through the writer
    let tmp = std::env::temp_dir().join("rustmspt_geometry_only.vtu");
    save_vtu(&tmp, &doc, VtuEncoding::Ascii).unwrap();
    assert_eq!(load_vtu(&tmp).unwrap(), doc);
}

#[test]
fn json_report_matches_the_frozen_schema() {
    let doc = load_vtu(&fixture("bad_inverted_tet")).unwrap();
    let mut report = verify(&doc, &VerifyGates::default());
    report.input = "bad_inverted_tet.vtu".to_string();
    let json = report_to_json(&report);

    for needle in [
        "\"schema\": 1",
        "\"tool\": \"mesh-verify\"",
        "\"input\": \"bad_inverted_tet.vtu\"",
        "\"determinism_mode\": \"strict\"",
        "\"summary\":",
        "\"sections\":",
        "\"id\": \"V1\"",
        "\"status\": \"FAIL\"",
        "\"code\": \"V1.negative_volume\"",
        "\"coordinates\":",
        "\"items_truncated\": false",
        "\"skipped_reason\": null",
    ] {
        assert!(
            json.contains(needle),
            "JSON report is missing {needle}\n{json}"
        );
    }
    // balanced braces and brackets: a cheap structural check without a JSON parser
    let count = |c: char| json.chars().filter(|&x| x == c).count();
    assert_eq!(count('{'), count('}'), "unbalanced braces in JSON report");
    assert_eq!(count('['), count(']'), "unbalanced brackets in JSON report");
}

#[test]
fn report_is_deterministic_across_runs() {
    let a = report_to_json(&run("bad_triple_face"));
    let b = report_to_json(&run("bad_triple_face"));
    assert_eq!(
        a, b,
        "two runs over the same mesh must produce identical JSON"
    );
    assert_eq!(
        report_to_log(&run("good_cube")),
        report_to_log(&run("good_cube"))
    );
}

#[test]
fn human_log_carries_every_section_and_a_summary() {
    let log = report_to_log(&run("bad_boundary_leak"));
    assert!(log.contains("[FAIL] [V3] Conformity"));
    assert!(log.contains("V3.boundary_leak"));
    assert!(log.contains("[SKIP] [V5]"));
    assert!(log.contains("SUMMARY"));
    assert!(log.trim_end().ends_with("-> FAIL"));
}

#[test]
fn annotate_adds_the_quality_arrays_and_verify_flags() {
    let doc = load_vtu(&fixture("bad_inverted_tet")).unwrap();
    let report = verify(&doc, &VerifyGates::default());
    let annotated = annotate(&doc, &report);
    annotated
        .validate()
        .expect("annotated document stays valid");

    for name in [
        "aspect_ratio",
        "radius_ratio",
        "min_dihedral_deg",
        "scaled_jacobian",
        "verify_flags",
    ] {
        let a = annotated
            .cell_array(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(a.data.len(), annotated.num_cells());
    }

    // the inverted tet is cell 0 and only V1 fired, so bit 0 must be set there and
    // nowhere else
    let flags = annotated.cell_array("verify_flags").unwrap();
    assert_eq!(
        flags.data.get_i64(0) & 1,
        1,
        "cell 0 must carry the [V1] flag"
    );
    for c in 1..annotated.num_cells() {
        assert_eq!(flags.data.get_i64(c), 0, "cell {c} must be unflagged");
    }

    // annotating twice must not duplicate arrays
    let twice = annotate(&annotated, &report);
    assert_eq!(
        twice
            .cell_data
            .iter()
            .filter(|a| a.name == "verify_flags")
            .count(),
        1
    );
}

#[test]
fn gates_are_configurable_and_scale_invariant() {
    let doc = load_vtu(&fixture("good_cube")).unwrap();

    // a gate tight enough to catch the Freudenthal tets (AR ≈ 1.394)
    let strict = VerifyGates {
        max_ar_warn: 1.0,
        ..VerifyGates::default()
    };
    let report = verify(&doc, &strict);
    assert!(report.fired_codes().iter().any(|c| c == "V4.aspect_ratio"));

    // warn_is_fatal turns that WARN into a nonzero exit
    let fatal = VerifyGates {
        max_ar_warn: 1.0,
        warn_is_fatal: true,
        ..VerifyGates::default()
    };
    assert_eq!(verify(&doc, &fatal).exit_code(), 1);

    // the same mesh scaled by 1e6 must produce the same verdict
    let mut scaled = doc.clone();
    for p in scaled.points.iter_mut() {
        *p = p.scale(1.0e6);
    }
    for name in ["DomainMin", "DomainMax"] {
        if let Some(a) = scaled.field_data.iter_mut().find(|a| a.name == name) {
            if let ArrayData::F64(v) = &mut a.data {
                for x in v.iter_mut() {
                    *x *= 1.0e6;
                }
            }
        }
    }
    assert_eq!(
        verify(&scaled, &VerifyGates::default()).fired_codes(),
        verify(&doc, &VerifyGates::default()).fired_codes(),
        "gates must be scale-invariant"
    );
}

#[test]
fn orientation_wrapper_matches_the_frozen_convention() {
    // SPEC_meshgen_geometry §1.1: det[b-a, c-a, d-a] > 0 for a positive tet.
    assert!(
        orient3d_sign_test() > 0.0,
        "the robust::orient3d sign negation (Rule N10) has been lost"
    );
    let a = Vec3::new(0.0, 0.0, 0.0);
    let b = Vec3::new(1.0, 0.0, 0.0);
    let c = Vec3::new(0.0, 1.0, 0.0);
    let d = Vec3::new(0.0, 0.0, 1.0);
    assert!(orient3d(a, b, c, d) > 0.0);
    assert!(
        orient3d(a, c, b, d) < 0.0,
        "swapping two nodes must flip the sign"
    );
    assert_eq!(
        orient3d(a, b, c, b),
        0.0,
        "a degenerate tet is exactly zero"
    );
}

#[test]
fn freudenthal_tets_have_the_frozen_quality() {
    // SPEC_meshgen_geometry §3.7: min dihedral 45°, AR 1.3938, V = h³/6.
    let corner = |i: usize| Vec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64);
    for t in [
        [0, 1, 3, 7],
        [0, 1, 7, 5],
        [0, 2, 7, 3],
        [0, 2, 6, 7],
        [0, 4, 5, 7],
        [0, 4, 7, 6],
    ] {
        let q = tet_quality([corner(t[0]), corner(t[1]), corner(t[2]), corner(t[3])]);
        assert!((q.volume - 1.0 / 6.0).abs() < 1e-12, "volume {}", q.volume);
        assert!(
            (q.min_dihedral_deg - 45.0).abs() < 1e-9,
            "dihedral {}",
            q.min_dihedral_deg
        );
        assert!(
            (q.aspect_ratio - 1.3938).abs() < 1e-3,
            "aspect {}",
            q.aspect_ratio
        );
    }
}

#[test]
fn quality_arrays_survive_a_write_read_round_trip() {
    let doc = load_vtu(&fixture("good_cube")).unwrap();
    let report = verify(&doc, &VerifyGates::default());
    let annotated = annotate(&doc, &report);
    let tmp = std::env::temp_dir().join("rustmspt_annotated.vtu");
    save_vtu(&tmp, &annotated, VtuEncoding::AppendedRaw).unwrap();
    let back = load_vtu(&tmp).unwrap();
    assert_eq!(back.num_cells(), annotated.num_cells());
    assert!(back.cell_array("verify_flags").is_some());
    let a = back.cell_array("aspect_ratio").unwrap();
    let b = annotated.cell_array("aspect_ratio").unwrap();
    assert_eq!(a.data, b.data);
    // and a synthetic array is preserved too (opaque round-trip)
    let mut extra = annotated.clone();
    extra.cell_data.push(DataArray::scalar(
        "custom",
        ArrayData::I32(vec![7; extra.num_cells()]),
    ));
    save_vtu(&tmp, &extra, VtuEncoding::Ascii).unwrap();
    assert_eq!(
        load_vtu(&tmp)
            .unwrap()
            .cell_array("custom")
            .unwrap()
            .data
            .get_i64(0),
        7
    );
}

// AI-FUNC-SUMMARY: [V6]'s region-adjacency rule must fail an impossible interface and accept a legal
//   one, including the coincident-surface exemption; side effects: none.
// Notes: The rule this pins is the one whose absence let A-3 ship 521 faces between region key
//   `{1,2}` and background - the lens of two intersecting solids is interior to both, so that
//   interface cannot exist. Built by relabelling a reference mesh rather than by hand, so the
//   geometry stays valid and only the labels are in question.
#[test]
fn v6_region_adjacency_rejects_a_two_component_step_across_an_untagged_face() {
    let base = load_vtu(&fixture("good_cube")).unwrap();

    // Region-set table {0}, {1}, {1,2}: background, one body, and their overlap.
    let mut doc = base.clone();
    doc.field_data.retain(|a| !a.name.starts_with("RegionSet") && a.name != "ComponentX" && a.name != "ComponentY");
    for (name, data) in [
        ("RegionSetOffsets", ArrayData::I64(vec![1, 2, 4])),
        ("RegionSetComponents", ArrayData::I32(vec![0, 1, 1, 2])),
        ("ComponentX", ArrayData::I32(vec![1, 2])),
        ("ComponentY", ArrayData::I32(vec![0, 0])),
    ] {
        doc.field_data.push(DataArray::scalar(name, data));
    }
    let tets: Vec<usize> = (0..doc.types.len()).filter(|c| doc.types[*c] == VTK_TETRA).collect();
    assert!(tets.len() >= 2, "fixture must have at least two tets");

    // Every tet background except one labelled {1,2}: an overlap region touching the
    // outside, which is exactly the impossible interface.
    let mut keys = vec![0i64; doc.types.len()];
    keys[tets[0]] = 2;
    doc.cell_data.retain(|a| a.name != "region_key");
    doc.cell_data
        .push(DataArray::scalar("region_key", ArrayData::I64(keys.clone())));
    let report = verify(&doc, &VerifyGates::default());
    assert!(
        report.fired_codes().iter().any(|c| c == "V6.region_adjacency"),
        "a {{1,2}} tet surrounded by background must fail: got {:?}",
        report.fired_codes()
    );

    // The same tet labelled {1} instead is a one-component step and is legal.
    let mut legal = doc.clone();
    let mut keys_ok = keys.clone();
    keys_ok[tets[0]] = 1;
    legal.cell_data.retain(|a| a.name != "region_key");
    legal.cell_data
        .push(DataArray::scalar("region_key", ArrayData::I64(keys_ok)));
    assert!(
        !verify(&legal, &VerifyGates::default())
            .fired_codes()
            .iter()
            .any(|c| c == "V6.region_adjacency"),
        "a one-component step is the ordinary material boundary and must pass"
    );
}
