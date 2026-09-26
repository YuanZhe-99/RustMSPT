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

/// One named metric out of one section, or `-1.0` when the section or the metric is absent -
/// a value no count or area can take, so a missing metric fails an assertion rather than
/// reading as zero.
fn metric(report: &rustmspt::meshgen::verify::VerifyReport, section: &str, name: &str) -> f64 {
    report
        .sections
        .iter()
        .find(|s| s.id == section)
        .and_then(|s| s.metrics.iter().find(|(k, _)| k == name))
        .map(|(_, v)| *v)
        .unwrap_or(-1.0)
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
            // the glued tet adds volume the box does not have (contracts §4.4, plan M-1.0)
            "V3.box_volume",
            "V3.multi_shared_face",
            "V3.non_manifold_edge",
        ],
    ),
    (
        "bad_boundary_leak",
        Some("V3.boundary_leak"),
        // the deleted tet's volume is missing from the box (contracts §4.4, plan M-1.0)
        &["V3.boundary_leak", "V3.box_volume"],
    ),
    (
        "bad_unwelded_sheet",
        Some("V7.unwelded_sheet_face"),
        &[
            "V2.duplicate_node",
            "V3.hanging_node",
            // the tag now names a face no tet has, so the material boundary it declared is
            // undeclared - the same single defect, seen by [V6] since M-1.0 gates it
            "V6.undeclared_boundary",
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
            // ...and for the same reason it does not fill its declared domain box
            "V3.box_volume",
            "V3.interface_crack",
            "V4.low_dihedral_share",
            "V4.min_dihedral",
            "V7.band_layers",
        ],
    ),
    (
        "bad_partition_id",
        Some("V8.partition_mismatch"),
        // the second component sits outside the domain box: MG-03's "extra block outside"
        &["V3.boundary_leak", "V3.box_volume", "V3.outside_domain", "V8.partition_mismatch"],
    ),
    (
        "bad_region_key",
        Some("V6.illegal_region_key"),
        // the contract validator (§4.2) sees the same out-of-table key before any check runs
        &["V12.contract", "V6.illegal_region_key"],
    ),
    (
        "bad_pinhole_sheet",
        Some("V8.pinhole_sheet"),
        // the removed triangle was a declared material boundary; the hole leaves it undeclared
        &["V6.undeclared_boundary", "V8.pinhole_sheet"],
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
        &["V3.boundary_leak", "V3.box_volume", "V9.junction_fan"],
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
        vec![
            "V1", "V2", "V3", "V4", "V5", "V6", "V7", "V8", "V9", "V10", "V11", "V12", "V13",
        ],
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
    assert_eq!(report.checks_run + report.checks_skipped, 13);
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

    // ...and *that* is the gap the undeclared-boundary metric exists to measure. The step is
    // legal in size, but the face it crosses declares nothing, so the mesh asserts a boundary
    // of component 1 that no interface names. `[V6]` cannot say so - its allowance floors at
    // one component - and until this metric there was no number for it at all.
    let report = verify(&legal, &VerifyGates::default());
    assert!(
        metric(&report, "V6", "undeclared_boundary_faces") >= 1.0,
        "a {{1}} tet whose faces carry no tag is an undeclared material boundary: got {}",
        metric(&report, "V6", "undeclared_boundary_faces")
    );
    assert!(
        metric(&report, "V6", "undeclared_boundary_area") > 0.0,
        "undeclared faces must contribute area"
    );
}

// AI-FUNC-SUMMARY: A mesh with no material boundary at all must report zero undeclared boundary -
//   the metric's control, so it cannot pass by counting every interior face; side effects: none.
#[test]
fn undeclared_boundary_metric_is_zero_when_no_material_boundary_exists() {
    let base = load_vtu(&fixture("good_cube")).unwrap();
    let mut doc = base.clone();
    doc.field_data.retain(|a| {
        !a.name.starts_with("RegionSet") && a.name != "ComponentX" && a.name != "ComponentY"
    });
    for (name, data) in [
        ("RegionSetOffsets", ArrayData::I64(vec![1, 2])),
        ("RegionSetComponents", ArrayData::I32(vec![0, 1])),
        ("ComponentX", ArrayData::I32(vec![1])),
        ("ComponentY", ArrayData::I32(vec![0])),
    ] {
        doc.field_data.push(DataArray::scalar(name, data));
    }
    // Every tet in the same region: no face anywhere separates two different inside-sets.
    let keys = vec![1i64; doc.types.len()];
    doc.cell_data.retain(|a| a.name != "region_key");
    doc.cell_data
        .push(DataArray::scalar("region_key", ArrayData::I64(keys)));
    let report = verify(&doc, &VerifyGates::default());
    assert_eq!(
        metric(&report, "V6", "undeclared_boundary_faces"),
        0.0,
        "a uniformly labelled mesh has no material boundary to declare"
    );
}

/// MG-01 (contracts §4.3): the item cap bounds what a section stores and prints, never what
/// it counts. The same defect verified at cap 0, 1 and 50 must give one status, one set of
/// counts, one set of fired codes and one exit code - at `891badc` cap 0 on this fixture
/// reported `V1 = FAIL` beside `summary.fail = 0` and exit 0.
#[test]
fn the_item_cap_changes_what_is_stored_never_what_is_counted() {
    let doc = load_vtu(&fixture("bad_inverted_tet")).unwrap();
    let at = |cap: usize| {
        verify(
            &doc,
            &VerifyGates {
                max_items_per_section: cap,
                ..VerifyGates::default()
            },
        )
    };
    let reference = at(50);
    assert!(reference.fail > 0, "the fixture must fail at the default cap");
    for cap in [0usize, 1, 50] {
        let r = at(cap);
        assert_eq!(r.exit_code(), 1, "cap {cap}: a FAIL must give a nonzero exit");
        assert!(!r.passed(), "cap {cap}");
        assert_eq!((r.fail, r.warn, r.info), (reference.fail, reference.warn, reference.info), "cap {cap}");
        assert_eq!(r.fired_codes(), reference.fired_codes(), "cap {cap}");
        for (a, b) in r.sections.iter().zip(reference.sections.iter()) {
            assert_eq!(a.status, b.status, "cap {cap}, section {}", a.id);
            assert!(a.items.len() <= cap, "cap {cap}, section {} stored {}", a.id, a.items.len());
        }
        let v1 = r.section("V1").unwrap();
        assert_eq!(v1.status, CheckStatus::Fail);
        assert_eq!(v1.items_truncated, cap < v1.fail_count + v1.warn_count + v1.info_count);
    }
}

fn with_stage(mut doc: VtuDoc, stage: i32) -> VtuDoc {
    let a = doc
        .field_data
        .iter_mut()
        .find(|a| a.name == "StageIndex")
        .expect("fixture declares StageIndex");
    a.data = ArrayData::I32(vec![stage]);
    doc
}

fn moved(doc: &VtuDoc, f: impl Fn(Vec3) -> Vec3) -> VtuDoc {
    let mut out = doc.clone();
    for p in out.points.iter_mut() {
        *p = f(*p);
    }
    out
}

fn v3_codes(report: &rustmspt::meshgen::verify::VerifyReport) -> Vec<String> {
    report
        .fired_codes()
        .into_iter()
        .filter(|c| c.starts_with("V3."))
        .collect()
}

fn verify_as(doc: &VtuDoc, delivered: bool) -> rustmspt::meshgen::verify::VerifyReport {
    rustmspt::meshgen::verify::verify_with_options(
        doc,
        &VerifyGates::default(),
        rustmspt::meshgen::verify::VerifyOptions {
            delivered,
            ..Default::default()
        },
    )
}

/// MG-03 (contracts §4.4): at the final stage a mesh past its box is a leak, not a bigger box.
/// At `891badc` `good_cube.vtu` with every x doubled, `DomainMax = [1, 1, 1]` and
/// `StageIndex = 11` verified `[V3]` PASS with `boundary_leaks = 0` and exit 0.
#[test]
fn a_final_mesh_past_its_domain_box_fails_v3() {
    let cube = load_vtu(&fixture("good_cube")).unwrap();
    let doubled = moved(&cube, |p| Vec3::new(2.0 * p.x, p.y, p.z));
    let r = verify_as(&doubled, false);
    assert_eq!(r.section("V3").unwrap().status, CheckStatus::Fail);
    assert_eq!(r.exit_code(), 1);
    let codes = v3_codes(&r);
    for code in ["V3.boundary_leak", "V3.box_volume", "V3.outside_domain"] {
        assert!(codes.iter().any(|c| c == code), "doubled cube must fire {code}; fired {codes:?}");
    }

    let translated = moved(&cube, |p| Vec3::new(p.x + 0.5, p.y, p.z));
    let codes = v3_codes(&verify_as(&translated, false));
    assert!(codes.iter().any(|c| c == "V3.outside_domain"), "translated: {codes:?}");
    assert!(codes.iter().any(|c| c == "V3.boundary_leak"), "translated: {codes:?}");
}

/// The relaxation contracts §4.4 does admit: before the trim (`StageIndex` 5-8) the lattice
/// legitimately overhangs the box, its hull is the outside, and the report names the stage.
/// The same document verified as the deliverable is held to the box.
#[test]
fn the_pre_trim_overhang_is_allowed_only_before_the_trim_and_never_when_delivered() {
    let cube = load_vtu(&fixture("good_cube")).unwrap();
    let overhang = with_stage(moved(&cube, |p| Vec3::new(2.0 * p.x, p.y, p.z)), 8);
    let r = verify_as(&overhang, false);
    let v3 = r.section("V3").unwrap();
    assert_eq!(v3.status, CheckStatus::Pass, "stage 8 overhang is reported, not failed: {:?}", v3.items);
    assert!(v3.items.iter().any(|i| i.code == "V3.pre_trim_overhang"));
    assert_eq!(metric(&r, "V3", "hull_relaxation_stage"), 8.0);

    let delivered = verify_as(&overhang, true);
    assert_eq!(delivered.section("V3").unwrap().status, CheckStatus::Fail);
    assert_eq!(metric(&delivered, "V3", "hull_relaxation_stage"), -1.0);

    for stage in [9, 10, 11] {
        let r = verify_as(&with_stage(overhang.clone(), stage), false);
        assert_eq!(r.section("V3").unwrap().status, CheckStatus::Fail, "stage {stage}");
    }
}

fn v12_rules(report: &rustmspt::meshgen::verify::VerifyReport) -> Vec<String> {
    report
        .section("V12")
        .unwrap()
        .items
        .iter()
        .filter(|i| i.code == "V12.contract")
        .map(|i| i.message.clone())
        .collect()
}

fn set_field(doc: &mut VtuDoc, name: &str, data: ArrayData) {
    doc.field_data.iter_mut().find(|a| a.name == name).expect(name).data = data;
}

/// MG-08 (contracts §4.2): a document that declares itself a final contract document is held to
/// the contract, whichever arrays it happens to carry. At `891badc` each of these three
/// injections into `good_cube.vtu` verified with `fail = warn = 0` and exit 0.
#[test]
fn a_self_declared_contract_document_is_validated_not_trusted() {
    let cube = load_vtu(&fixture("good_cube")).unwrap();
    let r = verify(&cube, &VerifyGates::default());
    assert!(v12_rules(&r).is_empty(), "the reference fixture is a valid contract document");
    assert_eq!(metric(&r, "V12", "contract_strength"), 2.0);

    // (1) constraint_kind / constraint_ref removed
    let mut missing = cube.clone();
    missing.point_data.retain(|a| a.name != "constraint_kind" && a.name != "constraint_ref");
    let r = verify(&missing, &VerifyGates::default());
    assert_eq!(r.exit_code(), 1);
    let rules = v12_rules(&r);
    assert!(rules.iter().any(|m| m.contains("[presence]") && m.contains("constraint_kind")), "{rules:?}");
    assert!(rules.iter().any(|m| m.contains("[presence]") && m.contains("constraint_ref")), "{rules:?}");

    // (2) FaceTagSideElems pointing at tets that do not exist
    let mut sides = cube.clone();
    let n = sides.field_array("FaceTagSideElems").unwrap().data.len();
    set_field(
        &mut sides,
        "FaceTagSideElems",
        ArrayData::I32((0..n).map(|i| if i % 2 == 0 { 999_999 } else { 999_998 }).collect()),
    );
    let rules = v12_rules(&verify(&sides, &VerifyGates::default()));
    assert!(rules.iter().any(|m| m.contains("[side_elems]")), "{rules:?}");

    // (3) a Sheet component that owns tets
    let mut sheet = cube.clone();
    set_field(&mut sheet, "ComponentKind", ArrayData::U8(vec![1]));
    let rules = v12_rules(&verify(&sheet, &VerifyGates::default()));
    assert!(rules.iter().any(|m| m.contains("[sheet_volume]")), "{rules:?}");

    // MG-07: one orientation per tag *member*, not per face
    let mut orient = cube.clone();
    set_field(&mut orient, "FaceTagOffsets", ArrayData::I64(vec![2]));
    set_field(&mut orient, "FaceTagComponents", ArrayData::I32(vec![1, 1]));
    orient.field_data.push(DataArray::scalar("FaceTagOrientation", ArrayData::I32(vec![1])));
    let rules = v12_rules(&verify(&orient, &VerifyGates::default()));
    assert!(rules.iter().any(|m| m.contains("[orientation_length]")), "{rules:?}");

    // the same document with no SchemaVersion is an external mesh, verified geometry-only
    let mut external = missing.clone();
    external.field_data.retain(|a| a.name != "SchemaVersion");
    let r = verify(&external, &VerifyGates::default());
    assert_eq!(metric(&r, "V12", "contract_strength"), 0.0);
    assert!(v12_rules(&r).is_empty());
}

/// Two tets can each be positively oriented and share a face with exactly two owners while
/// lying on the same side of it: they overlap, and neither `[V1]` (one tet at a time) nor the
/// face-count rules see it. A-3 at `0a8eb1c` carries 466 such faces.
#[test]
fn two_tets_on_the_same_side_of_their_shared_face_fail_v3() {
    let mut doc = load_vtu(&fixture("good_cube")).unwrap();
    // K0 = (0, 1, 3, 7) and K1 = (0, 1, 7, 5) share face (0, 1, 7). Give K0 a new apex on
    // K1's side of that face and restore K0's positive orientation by swapping two nodes.
    doc.points.push(Vec3::new(0.8, 0.2, 0.6));
    let q = (doc.points.len() - 1) as i64;
    for a in doc.point_data.iter_mut() {
        let v = a.data.get_i64(0);
        match &mut a.data {
            ArrayData::I32(x) => x.push(v as i32),
            ArrayData::U8(x) => x.push(v as u8),
            ArrayData::I64(x) => x.push(v),
            _ => panic!("unexpected point array type"),
        }
    }
    let k0 = &mut doc.connectivity[0..4];
    assert_eq!(k0, &[0, 1, 3, 7]);
    k0.copy_from_slice(&[1, 0, q, 7]);
    doc.validate().unwrap();
    let r = verify(&doc, &VerifyGates::default());
    assert!(
        !r.fired_codes().iter().any(|c| c == "V1.negative_volume"),
        "the construction must keep both tets positive: {:?}",
        r.fired_codes()
    );
    assert!(r.fired_codes().iter().any(|c| c == "V3.folded_face"), "{:?}", r.fired_codes());
    assert_eq!(metric(&r, "V3", "folded_faces"), 1.0);
}

/// Contracts D-18: the delivered file is the tets-only volume. It keeps the contract's tables but
/// none of the face or curve cells they index, so [V7]/[V9] and [V6]'s declaration rules have
/// nothing to read there and must skip with that reason rather than report the absence as a
/// defect - while everything that can be judged from the volume still runs.
#[test]
fn the_delivered_volume_skips_only_the_checks_that_read_tagged_cells() {
    let cube = load_vtu(&fixture("good_cube")).unwrap();
    let delivered = rustmspt::io::vtu::volume_only(&cube);
    let r = verify_as(&delivered, true);
    assert!(r.passed(), "fired {:?}", r.fired_codes());
    assert_eq!(metric(&r, "V12", "contract_strength"), 1.0);
    for id in ["V7", "V9"] {
        let s = r.section(id).unwrap();
        assert_eq!(s.status, CheckStatus::Skipped, "{id}");
        assert!(s.skipped_reason.as_deref().unwrap().contains("_contract.vtu"), "{id}");
    }
    let v6 = r.section("V6").unwrap();
    assert!(v6.items.iter().any(|i| i.code == "V6.deferred"));
    for id in ["V1", "V2", "V3", "V4"] {
        assert_ne!(r.section(id).unwrap().status, CheckStatus::Skipped, "{id} must run");
    }
}
