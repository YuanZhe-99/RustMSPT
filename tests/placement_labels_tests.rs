use rustmspt::config::{load_pack_document, PackDocument, ResolvedPlacement};
use rustmspt::geometry::icosphere_mesh;
use rustmspt::io::{load_tiff_or_folder, save_stl};
use rustmspt::pipeline::placement::{read_record, run_placement};
use rustmspt::types::Vec3;
use std::fs;
use std::path::Path;

fn resolve(config: &Path) -> ResolvedPlacement {
    match load_pack_document(config).expect("config loads") {
        PackDocument::Placement(p) => p.validate(config).expect("config validates"),
        PackDocument::Legacy(_) => panic!("expected the placement engine"),
    }
}

fn setup(dir: &Path, void: bool, voxel: f64) -> std::path::PathBuf {
    save_stl(
        &dir.join("shapes.stl"),
        &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 2),
        "s",
    )
    .unwrap();
    if void {
        save_stl(
            &dir.join("void.stl"),
            &icosphere_mesh(Vec3::new(20.0, 20.0, 20.0), 6.0, 3),
            "v",
        )
        .unwrap();
    }
    let p = dir.join("placement.yaml");
    let void_block = if void {
        "  void: { file: \"void.stl\", crossing: forbidden, gap: 1.5 }\n"
    } else {
        ""
    };
    let basis = if void { ", basis: solid" } else { "" };
    fs::write(
        &p,
        format!(
            r#"
placement:
  seed: 7
  frame: {{ unit: "um" }}
  domain: {{ min: [0, 0, 0], max: [40, 40, 40] }}
  shapes: {{ files: ["shapes.stl"] }}
{void_block}  size:
    distribution: {{ kind: lognormal, median: 5.0, sigma_log: 0.2, min: 4.0, max: 7.0 }}
  gaps: {{ particle_particle: 0.5 }}
  target: {{ volume_fraction: 0.05{basis} }}
  budget: {{ attempts_per_particle: 400, total_attempts: 300000 }}
  outputs:
    dir: "out"
    voxel_labels: {{ voxel_size: {voxel} }}
"#
        ),
    )
    .unwrap();
    p
}

// The labelled particle volume must agree with the volume the record reports,
// which is what makes the label field usable as a measurement rather than a
// picture.
#[test]
fn the_labelled_particle_volume_matches_the_recorded_volume() {
    let tmp = tempfile::tempdir().unwrap();
    let voxel = 0.5;
    let config = setup(tmp.path(), false, voxel);
    let outcome = run_placement(&resolve(&config)).expect("run succeeds");
    assert!(outcome.placed > 3);

    let dir = tmp.path().join("out/voxel_labels");
    let header: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("voxel_labels.json")).unwrap()).unwrap();
    assert_eq!(header["schema_version"], "rustmspt.placement.voxel_labels/1");
    assert_eq!(header["voxel_size"], voxel);
    assert_eq!(header["unit"], "um");
    assert_eq!(header["axis_order"], "zyx");
    assert_eq!(header["dims"], serde_json::json!([80, 80, 80]));
    // Volume3D carries neither spacing nor origin, so the header has to.
    assert_eq!(
        header["origin"],
        serde_json::json!([0.25, 0.25, 0.25]),
        "the origin is the centre of voxel (0,0,0)"
    );

    let phase = load_tiff_or_folder(&dir.join("phase.tiff")).expect("phase stack loads");
    assert_eq!((phase.width, phase.height, phase.depth), (80, 80, 80));
    let particle_voxels = phase.data.iter().filter(|v| **v == 1).count();
    let labelled_volume = particle_voxels as f64 * voxel.powi(3);

    let record = read_record(&tmp.path().join("out/particles.json")).unwrap();
    let recorded: f64 = record.particles.iter().map(|p| p.volume.in_domain).sum();
    assert!(
        (labelled_volume / recorded - 1.0).abs() < 0.03,
        "labelled {labelled_volume} against recorded {recorded}"
    );
    assert!(phase.data.iter().all(|v| (0..=2).contains(v)));
    assert!(
        phase.data.contains(&0),
        "a 5 percent pack must leave matrix"
    );
}

#[test]
fn every_particle_voxel_names_a_particle_and_the_ids_match_the_record() {
    let tmp = tempfile::tempdir().unwrap();
    let config = setup(tmp.path(), false, 0.5);
    let outcome = run_placement(&resolve(&config)).expect("run succeeds");

    let dir = tmp.path().join("out/voxel_labels");
    let phase = load_tiff_or_folder(&dir.join("phase.tiff")).unwrap();
    let ids = load_tiff_or_folder(&dir.join("particle_id.tiff")).unwrap();
    assert_eq!(phase.data.len(), ids.data.len());

    let mut seen = std::collections::BTreeSet::new();
    for (p, id) in phase.data.iter().zip(ids.data.iter()) {
        match *p {
            1 => {
                assert!(*id >= 1, "a particle voxel must name a particle");
                assert!(
                    *id <= outcome.placed as i64,
                    "id {id} is beyond the {} particles placed",
                    outcome.placed
                );
                seen.insert(*id);
            }
            // Ids start at 1 so that 0 means "no particle" rather than "the first".
            _ => assert_eq!(*id, 0, "only particle voxels carry an id"),
        }
    }
    assert!(
        seen.len() > outcome.placed / 2,
        "most placed particles should appear in the label field"
    );
}

// The void owns any voxel it claims, so no void voxel may name a particle. That
// is the same rule the record's overlap_owner states, applied per voxel.
#[test]
fn the_void_wins_every_voxel_it_claims_and_none_of_them_names_a_particle() {
    let tmp = tempfile::tempdir().unwrap();
    let config = setup(tmp.path(), true, 0.5);
    run_placement(&resolve(&config)).expect("run succeeds");

    let dir = tmp.path().join("out/voxel_labels");
    let phase = load_tiff_or_folder(&dir.join("phase.tiff")).unwrap();
    let ids = load_tiff_or_folder(&dir.join("particle_id.tiff")).unwrap();

    let void_voxels = phase.data.iter().filter(|v| **v == 2).count();
    assert!(void_voxels > 1000, "the void must be labelled: {void_voxels}");
    for (p, id) in phase.data.iter().zip(ids.data.iter()) {
        if *p == 2 {
            assert_eq!(*id, 0, "a void voxel named a particle");
        }
    }

    // The labelled void volume agrees with what the report measured for it.
    let report_text = fs::read(tmp.path().join("out/run_report.json")).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report_text).unwrap();
    let reported = report["void"]["volume_in_domain"].as_f64().unwrap();
    let labelled = void_voxels as f64 * 0.5f64.powi(3);
    assert!(
        (labelled / reported - 1.0).abs() < 0.03,
        "labelled void {labelled} against reported {reported}"
    );
}

// The labels are decided per voxel from the geometry alone, so they cannot depend
// on the thread count.
#[test]
fn the_label_field_is_identical_across_thread_counts() {
    let tmp = tempfile::tempdir().unwrap();
    save_stl(
        &tmp.path().join("shapes.stl"),
        &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 2),
        "s",
    )
    .unwrap();
    let mk = |name: &str, threads: u32| {
        let p = tmp.path().join(format!("{name}.yaml"));
        fs::write(
            &p,
            format!(
                r#"
placement:
  seed: 7
  frame: {{ unit: "um" }}
  domain: {{ min: [0, 0, 0], max: [30, 30, 30] }}
  shapes: {{ files: ["shapes.stl"] }}
  size:
    distribution: {{ kind: lognormal, median: 5.0, sigma_log: 0.2, min: 4.0, max: 7.0 }}
  target: {{ volume_fraction: 0.05 }}
  threads: {threads}
  budget: {{ attempts_per_particle: 400, total_attempts: 300000 }}
  outputs:
    dir: "{name}"
    voxel_labels: {{ voxel_size: 0.5 }}
"#
            ),
        )
        .unwrap();
        p
    };
    run_placement(&resolve(&mk("one", 1))).unwrap();
    run_placement(&resolve(&mk("eight", 8))).unwrap();
    for name in ["phase.tiff", "particle_id.tiff"] {
        assert_eq!(
            fs::read(tmp.path().join("one/voxel_labels").join(name)).unwrap(),
            fs::read(tmp.path().join("eight/voxel_labels").join(name)).unwrap(),
            "{name} differs between thread counts"
        );
    }
}

#[test]
fn the_label_files_are_listed_in_the_reports_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let config = setup(tmp.path(), false, 1.0);
    run_placement(&resolve(&config)).expect("run succeeds");
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(tmp.path().join("out/run_report.json")).unwrap()).unwrap();
    let roles: Vec<String> = report["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["role"].as_str().unwrap().to_string())
        .collect();
    for want in ["voxel_phase", "voxel_particle_id", "voxel_voxel_labels"] {
        assert!(roles.contains(&want.to_string()), "missing role {want} in {roles:?}");
    }
}
