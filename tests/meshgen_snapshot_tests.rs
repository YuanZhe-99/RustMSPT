//! GA-4 acceptance: the snapshot framework (PLAN phase GA, subtask GA-4).
//!
//! Covers SPEC_meshgen_contracts §3 (naming, `key` set, mode gating), §2.4
//! (metadata stamping), T-C6 (StageIndex matches the filename; SchemaVersion
//! mismatch is a named error), and the acceptance criterion: a fixture pipeline
//! emits the s-series, and the verifier + renderer accept every snapshot.

use rustmspt::config::meshgen::SnapshotMode;
use rustmspt::io::vtu::{load_vtu, VtuEncoding};
use rustmspt::meshgen::render_scene::{build_scene, SceneSpec};
use rustmspt::meshgen::snapshot::{
    emit_snapshot, should_emit, snapshot_path, warn_if_large, SnapshotMeta, Stage,
};
use rustmspt::meshgen::verify::{verify, verify_with_options, VerifyGates, VerifyOptions};
use rustmspt::types::Vec3;
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from("data/fixtures/meshgen/good_cube.vtu")
}

/// The full s-series with its (stage, round) emission plan. `Quality` carries an
/// IQD round; the rest are single-shot.
const SERIES: &[(Stage, Option<u32>)] = &[
    (Stage::Conditioned, None),
    (Stage::Features, None),
    (Stage::Arranged, None),
    (Stage::Gapfield, None),
    (Stage::Sizing, None),
    (Stage::Lattice, None),
    (Stage::Classified, None),
    (Stage::Snapped, None),
    (Stage::Cut, None),
    (Stage::Thin, None),
    (Stage::Quality, Some(0)),
    (Stage::Final, None),
];

#[test]
fn stage_index_and_name_are_frozen() {
    let names: Vec<&str> = SERIES.iter().map(|(s, _)| s.name()).collect();
    assert_eq!(
        names,
        [
            "conditioned",
            "features",
            "arranged",
            "gapfield",
            "sizing",
            "lattice",
            "classified",
            "snapped",
            "cut",
            "thin",
            "quality",
            "final"
        ]
    );
    let indices: Vec<u8> = SERIES.iter().map(|(s, _)| s.index()).collect();
    assert_eq!(indices, (0..=12u8).collect::<Vec<_>>()[..12]);
    assert_eq!(Stage::from_index(12), None);
}

#[test]
fn key_set_is_s02_s05_s08_s11() {
    let key: Vec<Stage> = SERIES
        .iter()
        .map(|(s, _)| *s)
        .filter(|s| s.is_key())
        .collect();
    assert_eq!(
        key,
        vec![Stage::Arranged, Stage::Lattice, Stage::Cut, Stage::Final]
    );
}

#[test]
fn surface_stages_are_s00_through_s03() {
    for (s, _) in SERIES {
        assert_eq!(s.is_surface_stage(), matches!(s.index(), 0..=3));
    }
}

#[test]
fn should_emit_respects_mode() {
    for (s, _) in SERIES {
        assert!(!should_emit(SnapshotMode::None, *s));
    }
    for (s, _) in SERIES {
        assert_eq!(should_emit(SnapshotMode::Key, *s), s.is_key());
    }
    for (s, _) in SERIES {
        assert!(should_emit(SnapshotMode::All, *s));
    }
}

#[test]
fn snapshot_path_naming_matches_contract() {
    let out = PathBuf::from("data/output/mesh.vtu");
    assert_eq!(
        snapshot_path(&out, Stage::Arranged, None),
        PathBuf::from("data/output/mesh.debug/mesh_s02_arranged.vtu")
    );
    assert_eq!(
        snapshot_path(&out, Stage::Final, None),
        PathBuf::from("data/output/mesh.debug/mesh_s11_final.vtu")
    );
    // Quality carries the IQD round suffix.
    assert_eq!(
        snapshot_path(&out, Stage::Quality, Some(2)),
        PathBuf::from("data/output/mesh.debug/mesh_s10_quality_r2.vtu")
    );
}

#[test]
fn stage_from_path_parses_snn() {
    let p = PathBuf::from("data/output/mesh.debug/mesh_s05_lattice.vtu");
    assert_eq!(Stage::from_path(&p), Some(Stage::Lattice));
    let p = PathBuf::from("data/output/mesh.debug/mesh_s10_quality_r0.vtu");
    assert_eq!(Stage::from_path(&p), Some(Stage::Quality));
    // A non-snapshot path yields None.
    assert_eq!(
        Stage::from_path(&PathBuf::from("data/output/mesh.vtu")),
        None
    );
}

#[test]
fn emit_series_verifier_and_renderer_accept_every_snapshot() {
    // The "fixture pipeline": load the reference contract VTU, re-stamp it as
    // each stage, emit, then verify + render. good_cube triggers no verifier
    // codes, so any new FAIL must come from the snapshot framework.
    let tmp = PathBuf::from("/tmp/opencode/ga4_snapshot");
    let _ = std::fs::remove_dir_all(&tmp);
    let out_vtu = tmp.join("mesh.vtu");

    for (stage, round) in SERIES {
        let mut doc = load_vtu(&fixture()).expect("fixture loads");
        let meta = SnapshotMeta {
            stage: *stage,
            round: *round,
            config_hash: 0x1234_5678,
            domain: (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
            determinism_strict: true,
            generator_version: [0, 1, 0],
        };
        let path =
            emit_snapshot(&mut doc, &out_vtu, &meta, VtuEncoding::Ascii).expect("emit succeeds");

        // T-C6: the filename stage matches the stamped StageIndex.
        let expected = Stage::from_path(&path).expect("snapshot filename parses");
        assert_eq!(expected.index(), stage.index());

        let reloaded = load_vtu(&path).expect("snapshot reloads");
        reloaded.validate().expect("snapshot validates");

        // The verifier accepts it (no FAIL), with the filename cross-check active.
        let options = VerifyOptions::from_path(&path);
        let report = verify_with_options(&reloaded, &VerifyGates::default(), options);
        let fired = report.fired_codes();
        let fails: Vec<&str> = fired
            .iter()
            .filter(|c| c.starts_with("V12."))
            .map(|s| s.as_str())
            .collect();
        assert!(
            fails.is_empty(),
            "stage {:?}: unexpected V12 codes {:?}",
            stage,
            fails
        );

        // The renderer accepts it (build_scene does not error).
        let scene =
            build_scene(&reloaded, &SceneSpec::default()).expect("renderer accepts snapshot");
        assert!(
            !scene.tris.is_empty() || !scene.segments.is_empty(),
            "stage {:?}: scene has no geometry",
            stage
        );
    }
}

#[test]
fn schema_version_mismatch_is_a_named_error() {
    // A contract VTU with a wrong SchemaVersion fires V12.schema_version_mismatch.
    let mut doc = load_vtu(&fixture()).expect("fixture loads");
    // Overwrite SchemaVersion with 2 (contract is 1).
    let arr = doc
        .field_data
        .iter_mut()
        .find(|a| a.name == "SchemaVersion")
        .expect("SchemaVersion present");
    arr.data = rustmspt::io::vtu::ArrayData::I32(vec![2]);

    let report = verify(&doc, &VerifyGates::default());
    assert!(
        report
            .fired_codes()
            .iter()
            .any(|c| c == "V12.schema_version_mismatch"),
        "expected V12.schema_version_mismatch, got {:?}",
        report.fired_codes()
    );
}

#[test]
fn stage_index_filename_mismatch_is_a_fail() {
    // Stamp a doc as Final (s11) but write it to an s02 (arranged) path; the
    // verifier's filename cross-check must catch the disagreement.
    let tmp = PathBuf::from("/tmp/opencode/ga4_mismatch");
    let _ = std::fs::remove_dir_all(&tmp);
    let out_vtu = tmp.join("mesh.vtu");

    let mut doc = load_vtu(&fixture()).expect("fixture loads");
    let meta = SnapshotMeta::new(
        Stage::Final,
        0,
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
    );
    let path = emit_snapshot(&mut doc, &out_vtu, &meta, VtuEncoding::Ascii).expect("emit succeeds");

    // Rename to an s02 path so the filename disagrees with StageIndex.
    let mismatch_path = tmp.join("mesh.debug").join("mesh_s02_arranged.vtu");
    std::fs::rename(&path, &mismatch_path).expect("rename");

    let reloaded = load_vtu(&mismatch_path).expect("loads");
    let options = VerifyOptions::from_path(&mismatch_path);
    let report = verify_with_options(&reloaded, &VerifyGates::default(), options);
    assert!(
        report
            .fired_codes()
            .iter()
            .any(|c| c == "V12.stage_index_filename_mismatch"),
        "expected V12.stage_index_filename_mismatch, got {:?}",
        report.fired_codes()
    );
}

#[test]
fn warn_if_large_fires_only_for_all_above_threshold() {
    // Below threshold: no warn.
    assert!(!warn_if_large(SnapshotMode::All, 5_000_000));
    // Above threshold with `all`: warn.
    assert!(warn_if_large(SnapshotMode::All, 5_000_001));
    // `key` never warns regardless of size.
    assert!(!warn_if_large(SnapshotMode::Key, 20_000_000));
    assert!(!warn_if_large(SnapshotMode::None, 20_000_000));
}
