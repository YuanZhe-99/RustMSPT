use rustmspt::config::{load_pack_document, PackDocument, ResolvedPlacement};
use rustmspt::geometry::{
    icosphere_mesh, mesh_distance_exact, mesh_volume, split_mesh_into_granules, transform_shell,
    translate_mesh, UnitQuat,
};
use rustmspt::io::{load_stl, save_stl};
use rustmspt::pipeline::placement::{read_record, read_report, run_placement};
use rustmspt::types::Vec3;
use serde_json::Value;
use std::fs;
use std::path::Path;

// AI-FUNC-SUMMARY: Build a small independent aggregate fixture with a closed icosphere source; writes config and source STL.
fn fixture(dir: &Path, shape: &str, variants: usize, enabled: bool) -> std::path::PathBuf {
    save_stl(
        &dir.join("shape.stl"),
        &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1),
        "sphere",
    )
    .unwrap();
    let path = dir.join("config.yaml");
    fs::write(
        &path,
        format!(
            r#"
placement:
  seed: 73
  frame: {{unit: um}}
  domain: {{min: [0,0,0], max: [30,30,30]}}
  shapes: {{files: [shape.stl]}}
  size:
    distribution: {{kind: lognormal, median: 2, sigma_log: 0.12, min: 1.6, max: 2.4}}
  boundary: {{mode: strict, min_boundary_dist: 0.2}}
  gaps: {{particle_particle: 0.3}}
  target: {{volume_fraction: 0.015}}
  budget: {{attempts_per_particle: 200, total_attempts: 5000}}
  outputs: {{dir: out, per_particle_stl: true}}
  aggregates:
    enabled: {enabled}
    variants: {variants}
    particles_per_cluster: 8
    shape: {shape}
    internal_gap: 0.15
    compaction_sweeps: 16
"#
        ),
    )
    .unwrap();
    path
}
// AI-FUNC-SUMMARY: Resolve the test placement config; panics only on a fixture error.
fn resolve(path: &Path) -> ResolvedPlacement {
    match load_pack_document(path).unwrap() {
        PackDocument::Placement(p) => p.validate(path).unwrap(),
        _ => panic!("placement expected"),
    }
}
// AI-FUNC-SUMMARY: Read JSON output from a test directory.
fn json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
// AI-FUNC-SUMMARY: Check real exported particles, actual VF, reconstruction, template containment and pair clearances independently of proxy collision implementation.
fn check_geometry(dir: &Path, shape: &str) {
    let out = dir.join("out");
    let record = read_record(&out.join("particles.json")).unwrap();
    let report = read_report(&out.join("run_report.json")).unwrap();
    let blocks = json(&out.join("aggregates.json"));
    let templates = json(&out.join("aggregate_templates.json"));
    assert!(record.particles.len() >= 8);
    assert_eq!(record.particles.len(), report.actual.particles);
    let catalog = templates["templates"].as_array().unwrap();
    assert_eq!(catalog.len(), 3);
    for t in catalog {
        assert!(t["internal_volume_fraction"].as_f64().unwrap() > 0.0);
        assert!(t["internal_volume_fraction"].as_f64().unwrap() < 1.0);
        assert!(
            t["internal_volume_fraction"].as_f64().unwrap() + 1e-8
                >= t["initial_internal_volume_fraction"].as_f64().unwrap()
        );
        assert!(t["mesh_refinement_sweeps"].as_u64().unwrap() <= 8);
    }
    let mut owners = vec![usize::MAX; record.particles.len()];
    for block in blocks["clusters"].as_array().unwrap() {
        let id = block["cluster_id"].as_u64().unwrap() as usize;
        let first = block["first_particle"].as_u64().unwrap() as usize;
        let count = block["particle_count"].as_u64().unwrap() as usize;
        assert_eq!(count, 8);
        let template = &catalog[block["template_id"].as_u64().unwrap() as usize];
        let half = template["half_extent_or_radius"].as_f64().unwrap();
        let q = &block["rotation"];
        let inverse = UnitQuat::new(
            q[0].as_f64().unwrap(),
            -q[1].as_f64().unwrap(),
            -q[2].as_f64().unwrap(),
            -q[3].as_f64().unwrap(),
        )
        .unwrap();
        let c = &block["translation"];
        let centre = Vec3::new(
            c[0].as_f64().unwrap(),
            c[1].as_f64().unwrap(),
            c[2].as_f64().unwrap(),
        );
        for (index, owner) in owners.iter_mut().enumerate().skip(first).take(count) {
            assert_eq!(*owner, usize::MAX);
            *owner = id;
            let mesh = load_stl(&out.join("particles").join(format!("p{index:06}.stl"))).unwrap();
            for v in mesh.vertices {
                let p = inverse.rotate_point(v.sub(centre));
                let extent = if shape == "sphere" {
                    p.dot(p).sqrt()
                } else {
                    p.x.abs().max(p.y.abs()).max(p.z.abs())
                };
                assert!(extent <= half + 1e-5);
            }
        }
    }
    assert!(owners.iter().all(|&x| x != usize::MAX));
    let meshes: Vec<_> = record
        .particles
        .iter()
        .map(|p| load_stl(&out.join("particles").join(format!("{}.stl", p.entity_id))).unwrap())
        .collect();
    let actual_volume: f64 = meshes.iter().map(mesh_volume).sum();
    assert!(
        (actual_volume / report.target.basis_volume - report.actual.volume_fraction_solid).abs()
            < 1e-7
    );
    for (i, a) in meshes.iter().enumerate() {
        for (j, b) in meshes.iter().enumerate().skip(i + 1) {
            let required = if owners[i] == owners[j] { 0.15 } else { 0.3 };
            assert!(
                mesh_distance_exact(a, b) + 2e-5 >= required,
                "particles {i}, {j} gap"
            );
        }
    }
    let source = load_stl(&dir.join("shape.stl")).unwrap();
    for (p, mesh) in record.particles.iter().zip(meshes.iter()) {
        let q = p.rotation.quaternion;
        let mut canonical = split_mesh_into_granules(&source)[p.source_shape.shell_index].clone();
        let c = p.source_shape.shell_centroid;
        translate_mesh(&mut canonical, Vec3::new(-c[0], -c[1], -c[2]));
        let reconstructed = transform_shell(
            &canonical,
            p.scale,
            UnitQuat::new(q[0], q[1], q[2], q[3]).unwrap(),
            Vec3::new(p.translation[0], p.translation[1], p.translation[2]),
        );
        // STL welding changes vertex ordering; compare face vertices in face order.
        for (a, b) in reconstructed.faces.iter().zip(&mesh.faces) {
            for (ia, ib) in [(a.a, b.a), (a.b, b.b), (a.c, b.c)] {
                let d = reconstructed.vertices[ia].sub(mesh.vertices[ib]);
                assert!(d.dot(d).sqrt() < 2e-5);
            }
        }
    }
}
#[test]
fn sphere_clusters_export_valid_real_particles() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 3, true);
    run_placement(&resolve(&c)).unwrap();
    check_geometry(d.path(), "sphere");
}
#[test]
fn rotated_cube_clusters_export_valid_real_particles() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "cube", 3, true);
    run_placement(&resolve(&c)).unwrap();
    check_geometry(d.path(), "cube");
}
#[test]
fn disabled_is_byte_identical_to_omitted_and_variants_are_configurable() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 3, false);
    let mut config = resolve(&c);
    config.threads = 1;
    run_placement(&config).unwrap();
    let baseline = fs::read(d.path().join("out/particles.stl")).unwrap();
    let text = fs::read_to_string(&c).unwrap();
    fs::write(&c, text.split("  aggregates:").next().unwrap()).unwrap();
    run_placement(&resolve(&c)).unwrap();
    assert_eq!(
        baseline,
        fs::read(d.path().join("out/particles.stl")).unwrap()
    );
    for variants in [1, 5] {
        let c = fixture(d.path(), "sphere", variants, true);
        run_placement(&resolve(&c)).unwrap();
        assert_eq!(
            json(&d.path().join("out/aggregate_templates.json"))["templates"]
                .as_array()
                .unwrap()
                .len(),
            variants
        );
    }
}
#[test]
fn deterministic_templates_do_not_depend_on_seed_and_outputs_match_worker_counts() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 3, true);
    let mut config = resolve(&c);
    config.threads = 1;
    run_placement(&config).unwrap();
    let stl = fs::read(d.path().join("out/particles.stl")).unwrap();
    let templates = fs::read(d.path().join("out/aggregate_templates.json")).unwrap();
    config.threads = 4;
    run_placement(&config).unwrap();
    assert_eq!(stl, fs::read(d.path().join("out/particles.stl")).unwrap());
    config.seed += 1;
    run_placement(&config).unwrap();
    assert_eq!(
        templates,
        fs::read(d.path().join("out/aggregate_templates.json")).unwrap()
    );
}
#[test]
fn rejects_unsupported_or_invalid_aggregate_options() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 3, true);
    let base = fs::read_to_string(&c).unwrap();
    for text in [
        base.replace("variants: 3", "variants: 0"),
        base.replace("internal_gap: 0.15", "internal_gap: -1"),
        base.replace("shape: sphere", "shape: triangle"),
        base.replace("mode: strict", "mode: periodic"),
    ] {
        fs::write(&c, text).unwrap();
        let invalid = match load_pack_document(&c) {
            Err(_) => true,
            Ok(PackDocument::Placement(p)) => p.validate(&c).is_err(),
            _ => false,
        };
        assert!(invalid);
    }
}
#[test]
fn stop_before_generation_preserves_empty_partial_results() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 3, true);
    let t = fs::read_to_string(&c)
        .unwrap()
        .replace("particle_particle: 0.3", "particle_particle: 0.0");
    fs::write(&c, t).unwrap();
    fs::create_dir(d.path().join("out")).unwrap();
    fs::write(d.path().join("out/STOP"), "stop").unwrap();
    run_placement(&resolve(&c)).unwrap();
    assert_eq!(
        read_report(&d.path().join("out/run_report.json"))
            .unwrap()
            .status,
        "interrupted"
    );
    assert_eq!(
        read_record(&d.path().join("out/particles.json"))
            .unwrap()
            .particles
            .len(),
        0
    );
}

#[test]
fn proxies_deep_inside_a_pore_are_rejected_for_both_shapes() {
    use rustmspt::geometry::box_mesh;
    use rustmspt::types::BoundingBox;
    for shape in ["sphere", "cube"] {
        let d = tempfile::tempdir().unwrap();
        let c = fixture(d.path(), shape, 3, true);
        save_stl(
            &d.path().join("void.stl"),
            &box_mesh(BoundingBox {
                min: Vec3::new(2.0, 2.0, 2.0),
                max: Vec3::new(28.0, 28.0, 28.0),
            }),
            "void",
        )
        .unwrap();
        let text = fs::read_to_string(&c).unwrap().replace(
            "  size:",
            "  void: {file: void.stl, crossing: forbidden, gap: 0.1}\n  size:",
        );
        fs::write(&c, text).unwrap();
        let outcome = run_placement(&resolve(&c)).unwrap();
        assert_eq!(outcome.placed, 0);
        let report = read_report(&d.path().join("out/run_report.json")).unwrap();
        assert!(report.rejections["inside_void"] > 0);
    }
}

#[test]
fn stop_during_aggregate_pack_saves_whole_clusters() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 3, true);
    let text = fs::read_to_string(&c)
        .unwrap()
        .replace("volume_fraction: 0.015", "volume_fraction: 0.8")
        .replace(
            "attempts_per_particle: 200, total_attempts: 5000",
            "attempts_per_particle: 1000000, total_attempts: 10000000",
        );
    fs::write(&c, text).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rustmspt"))
        .arg("pack")
        .arg("--config")
        .arg(&c)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            panic!("pack finished before cancellation");
        }
        let progress = fs::read(d.path().join("out/progress.json"))
            .ok()
            .and_then(|v| serde_json::from_slice::<Value>(&v).ok());
        if progress.is_some_and(|v| v["particles"].as_u64().unwrap_or(0) > 0) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    fs::write(d.path().join("out/STOP"), "stop and save").unwrap();
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("stop timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let report = read_report(&d.path().join("out/run_report.json")).unwrap();
    let clusters = json(&d.path().join("out/aggregates.json"));
    assert_eq!(report.status, "interrupted");
    assert!(report.actual.particles > 0);
    assert_eq!(
        report.actual.particles,
        clusters["clusters"].as_array().unwrap().len() * 8
    );
    assert!(d.path().join("out/particles.stl").exists());
}

#[test]
// AI-FUNC-SUMMARY: Exercise all deterministic strategies on anisotropic real particles and independently verify final world geometry and transforms.
fn target_search_rotates_rearranges_and_preserves_real_geometry() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 3, true);
    let mut mesh = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1);
    for v in &mut mesh.vertices {
        v.x *= 2.0;
        v.y *= 0.6;
        v.z *= 0.7;
    }
    save_stl(&d.path().join("shape.stl"), &mesh, "ellipsoid").unwrap();
    let text = fs::read_to_string(&c).unwrap();
    let text = text + "    mesh_refinement_sweeps: 0\n";
    fs::write(&c,format!("{text}    target_internal_volume_fraction: 0.99\n    strategy_rounds: 3\n    max_compaction_trials: 1500\n")).unwrap();
    run_placement(&resolve(&c)).unwrap();
    check_geometry(d.path(), "sphere");
    let catalog = json(&d.path().join("out/aggregate_templates.json"));
    for t in catalog["templates"].as_array().unwrap() {
        assert_eq!(t["target_reached"], false);
        assert!(t["compaction_search"]["rotations"].as_u64().unwrap() > 0);
        assert!(t["compaction_search"]["lateral_moves"].as_u64().unwrap() > 0);
        assert!(t["compaction_search"]["pair_moves"].as_u64().unwrap() > 0);
        assert!(t["compaction_search"]["trials"].as_u64().unwrap() <= 1500);
        assert!(t["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["rotation"] != serde_json::json!([1.0, 0.0, 0.0, 0.0])));
    }
    let baseline = fs::read(d.path().join("out/particles.stl")).unwrap();
    let mut config = resolve(&c);
    config.threads = 4;
    run_placement(&config).unwrap();
    assert_eq!(
        baseline,
        fs::read(d.path().join("out/particles.stl")).unwrap()
    );
}

#[test]
// AI-FUNC-SUMMARY: Verify target success and bounded shortfall, plus strict validation of all new controls.
fn target_search_reports_success_budget_and_invalid_controls() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "cube", 1, true);
    let base = fs::read_to_string(&c).unwrap();
    for (target, budget, reason) in [
        (0.001, 1, "target_reached"),
        (0.99, 1, "trial_budget_exhausted"),
    ] {
        fs::write(&c,format!("{base}    target_internal_volume_fraction: {target}\n    max_compaction_trials: {budget}\n")).unwrap();
        run_placement(&resolve(&c)).unwrap();
        let catalog = json(&d.path().join("out/aggregate_templates.json"));
        let t = &catalog["templates"][0];
        assert_eq!(t["compaction_search"]["stop_reason"], reason);
        assert_eq!(t["target_reached"], target == 0.001);
        assert!(t["compaction_search"]["trials"].as_u64().unwrap() <= budget);
    }
    for extra in [
        "target_internal_volume_fraction: 0",
        "target_internal_volume_fraction: 1.1",
        "target_internal_volume_fraction: .nan",
        "strategy_rounds: 129",
        "max_compaction_trials: 1000001",
        "container_shrink_fraction: 0.5",
        "container_shrink_fraction: .nan",
        "rotation_step_degrees: 0",
    ] {
        fs::write(&c, format!("{base}    {extra}\n")).unwrap();
        let invalid = match load_pack_document(&c) {
            Err(_) => true,
            Ok(PackDocument::Placement(p)) => p.validate(&c).is_err(),
            _ => false,
        };
        assert!(invalid, "{extra}");
    }
}

#[test]
// AI-FUNC-SUMMARY: Verify automatic catalog count, explicit mixed smaller-block fallback, finite budget reservation and independent cross-stage particle gaps.
fn automatic_templates_and_mixed_fallback_place_smaller_blocks() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 1, true);
    let text = fs::read_to_string(&c)
        .unwrap()
        .replace("variants: 1", "variants: auto")
        .replace("particles_per_cluster: 8", "particles_per_cluster: 64")
        .replace("max: [30,30,30]", "max: [6,6,6]")
        .replace("volume_fraction: 0.015", "volume_fraction: 0.15")
        .replace("compaction_sweeps: 16", "compaction_sweeps: 0")
        .replace(
            "attempts_per_particle: 200, total_attempts: 5000",
            "attempts_per_particle: 30, total_attempts: 300",
        );
    fs::write(&c,format!("{text}    mode: mixed\n    fallback_particles_per_cluster: [4,1]\n    mesh_refinement_sweeps: 0\n")).unwrap();
    run_placement(&resolve(&c)).unwrap();
    let catalog = json(&d.path().join("out/aggregate_templates.json"));
    assert_eq!(catalog["resolved_variants"], 4);
    assert_eq!(catalog["templates"].as_array().unwrap().len(), 12);
    let blocks = json(&d.path().join("out/aggregates.json"));
    let stages = blocks["stages"].as_array().unwrap();
    assert!(stages.len() >= 2);
    assert_eq!(stages[0]["placed_clusters"], 0);
    assert!(blocks["clusters"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["stage"].as_u64().unwrap() > 0));
    let report = read_report(&d.path().join("out/run_report.json")).unwrap();
    assert!(report.attempts.total <= 300);
    assert_eq!(
        report.actual.particles,
        blocks["clusters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["particle_count"].as_u64().unwrap() as usize)
            .sum::<usize>()
    );
    let record = read_record(&d.path().join("out/particles.json")).unwrap();
    let meshes: Vec<_> = record
        .particles
        .iter()
        .map(|p| {
            load_stl(
                &d.path()
                    .join("out/particles")
                    .join(format!("{}.stl", p.entity_id)),
            )
            .unwrap()
        })
        .collect();
    for (i, a) in meshes.iter().enumerate() {
        for b in &meshes[i + 1..] {
            assert!(mesh_distance_exact(a, b) + 2e-5 >= 0.15);
        }
    }
    assert!(report.size_classes.iter().all(|c| c.placed <= c.drawn));
    let base = fs::read_to_string(&c).unwrap();
    fs::write(
        &c,
        base.replace("variants: auto", "variants: 1")
            .replace("particles_per_cluster: 64", "particles_per_cluster: 1024")
            .replace("[4,1]", "[1023,1022]"),
    )
    .unwrap();
    assert_eq!(resolve(&c).aggregates.variants, 1);
    fs::write(&c, base.replace("[4,1]", "[1,4]")).unwrap();
    let p = match load_pack_document(&c).unwrap() {
        PackDocument::Placement(p) => p,
        _ => panic!("placement"),
    };
    assert!(p.validate(&c).is_err());
}

#[test]
// AI-FUNC-SUMMARY: Resolve all shipped placement templates and verify explicit default/cluster/mixed mode and target controls stay synchronized with supported config types.
fn shipped_configuration_templates_expose_supported_modes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/input");
    for (name, enabled, mixed) in [
        ("placement_config.yaml", false, false),
        ("placement_void_config.yaml", false, false),
        ("placement_aggregate_config.yaml", true, false),
        ("placement_mixed_config.yaml", true, true),
    ] {
        let config = resolve(&root.join(name));
        assert_eq!(config.aggregates.enabled, enabled);
        assert_eq!(format!("{:?}", config.aggregates.mode) == "Mixed", mixed);
        assert_eq!(config.aggregates.variants, 0);
        assert_eq!(config.aggregates.target_internal_volume_fraction, Some(0.5));
    }
}

#[test]
// AI-FUNC-SUMMARY: Interrupt target search mid-generation and verify its best valid real template is saved along with interrupted standard output.
fn stop_during_target_search_preserves_valid_best_template() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 3, true);
    let text = fs::read_to_string(&c)
        .unwrap()
        .replace("particles_per_cluster: 8", "particles_per_cluster: 64")
        .replace("compaction_sweeps: 16", "compaction_sweeps: 0");
    fs::write(&c,format!("{text}    mesh_refinement_sweeps: 0\n    target_internal_volume_fraction: 0.99\n    strategy_rounds: 128\n    max_compaction_trials: 1000000\n")).unwrap();
    let log = d.path().join("pack.log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rustmspt"))
        .arg("pack")
        .arg("--config")
        .arg(&c)
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("[Aggregate compaction]")
    {
        assert!(child.try_wait().unwrap().is_none());
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("search not started");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    fs::write(d.path().join("out/STOP"), "save current best template").unwrap();
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("stop timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let report = read_report(&d.path().join("out/run_report.json")).unwrap();
    assert_eq!(report.status, "interrupted");
    assert_eq!(report.actual.particles, 0);
    let catalog = json(&d.path().join("out/aggregate_templates.json"));
    assert_eq!(catalog["templates"].as_array().unwrap().len(), 1);
    let t = &catalog["templates"][0];
    assert_eq!(t["compaction_search"]["stop_reason"], "interrupted");
    assert!(
        t["internal_volume_fraction"].as_f64().unwrap()
            >= t["initial_internal_volume_fraction"].as_f64().unwrap()
    );
    let mesh = load_stl(&d.path().join("out/aggregate_templates/template_0000.stl")).unwrap();
    let shells = split_mesh_into_granules(&mesh);
    assert_eq!(shells.len(), 64);
    for (i, a) in shells.iter().enumerate() {
        for b in &shells[i + 1..] {
            assert!(mesh_distance_exact(a, b) + 2e-5 >= 0.15);
        }
    }
}

#[test]
// AI-FUNC-SUMMARY: Contact growth improves a sparse bounding-ball fixture, preserves exact gaps/reconstruction and is deterministic across workers.
fn contact_growth_geometry_density_and_determinism() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 3, true);
    let base = fs::read_to_string(&c).unwrap();
    // Anisotropic real surfaces leave substantial slack in their bounding balls.
    let mut shape = icosphere_mesh(Vec3::new(0., 0., 0.), 1., 0);
    for v in &mut shape.vertices {
        v.x *= 1.8;
        v.z *= 0.6;
    }
    save_stl(&d.path().join("shape.stl"), &shape, "ellipsoid").unwrap();
    fs::write(&c,format!("{base}    construction: contact_growth\n    contact_directions: 8\n    contact_orientations: 3\n    neighborhood_sweeps: 1\n    max_compaction_trials: 1000\n    target_internal_volume_fraction: 0.5\n")).unwrap();
    let mut config = resolve(&c);
    config.threads = 1;
    run_placement(&config).unwrap();
    check_geometry(d.path(), "sphere");
    let first = json(&d.path().join("out/aggregate_templates.json"));
    assert_eq!(first["algorithm"], "deterministic_contact_growth_v1");
    for t in first["templates"].as_array().unwrap() {
        assert!(
            t["internal_volume_fraction"].as_f64().unwrap()
                > 1.1 * t["initial_internal_volume_fraction"].as_f64().unwrap()
        );
        eprintln!(
            "[Contact validation] initial_vf={} final_vf={}",
            t["initial_internal_volume_fraction"], t["internal_volume_fraction"]
        );
        assert!(t["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["rotation"] != serde_json::json!([1., 0., 0., 0.])));
    }
    let record = fs::read(d.path().join("out/particles.json")).unwrap();
    config.threads = 3;
    run_placement(&config).unwrap();
    assert_eq!(first, json(&d.path().join("out/aggregate_templates.json")));
    assert_eq!(
        record,
        fs::read(d.path().join("out/particles.json")).unwrap()
    );
}

#[test]
// AI-FUNC-SUMMARY: New controls reject invalid values; zero search budget preserves every member and reports shortfall instead of false success.
fn contact_growth_controls_and_exhaustion() {
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "cube", 1, true);
    let base = fs::read_to_string(&c).unwrap();
    for extra in [
        "contact_directions: 0",
        "contact_orientations: 0",
        "contact_tolerance: .nan",
        "contact_tolerance: 0",
        "contact_max_steps: 0",
        "neighborhood_sweeps: 129",
        "construction: random",
    ] {
        fs::write(&c, format!("{base}    {extra}\n")).unwrap();
        let invalid = match load_pack_document(&c) {
            Err(_) => true,
            Ok(PackDocument::Placement(p)) => p.validate(&c).is_err(),
            _ => false,
        };
        assert!(invalid, "{extra}");
    }
    fs::write(&c,format!("{base}    construction: contact_growth\n    max_compaction_trials: 0\n    target_internal_volume_fraction: 0.99\n")).unwrap();
    run_placement(&resolve(&c)).unwrap();
    let catalog = json(&d.path().join("out/aggregate_templates.json"));
    let t = &catalog["templates"][0];
    assert_eq!(t["members"].as_array().unwrap().len(), 8);
    assert_eq!(t["target_reached"], false);
    assert_eq!(
        t["compaction_search"]["stop_reason"],
        "trial_budget_exhausted"
    );
    let mesh = load_stl(&d.path().join("out/aggregate_templates/template_0000.stl")).unwrap();
    let shells = split_mesh_into_granules(&mesh);
    for (i, a) in shells.iter().enumerate() {
        for b in &shells[i + 1..] {
            assert!(mesh_distance_exact(a, b) + 2e-5 >= 0.15);
        }
    }
}

#[test]
// AI-FUNC-SUMMARY: Stop a CLI during template growth, inspect its saved members, resume and require the uninterrupted final geometry and counters.
fn contact_growth_cli_stop_resume_matches_uninterrupted() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let d = tempfile::tempdir().unwrap();
    let c = fixture(d.path(), "sphere", 1, true);
    let text = fs::read_to_string(&c)
        .unwrap()
        .replace("particles_per_cluster: 8", "particles_per_cluster: 16");
    fs::write(&c,format!("{text}    construction: contact_growth\n    contact_directions: 64\n    contact_orientations: 8\n    neighborhood_sweeps: 0\n    strategy_rounds: 0\n")).unwrap();
    let log = d.path().join("contact.log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rustmspt"))
        .args(["pack", "--config"])
        .arg(&c)
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    while !fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("[Aggregate contact] members=1/")
    {
        assert!(child.try_wait().unwrap().is_none());
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("contact growth did not start");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    // The posed-geometry kernel finishes this fixture's template in well under the STOP-file
    // poll interval, so stop with SIGINT (checked on every poll) as well as the STOP file.
    fs::write(d.path().join("out/STOP"), "save contact growth").unwrap();
    let _ = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status();
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("contact stop timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let checkpoint = d.path().join("out/checkpoint.json");
    let saved = json(&checkpoint);
    // Templates are generated in concurrent batches; the first is the interrupted variant 0.
    let growth = &saved["snapshot"]["aggregate"]["batch"][0];
    assert!(growth["next"].as_u64().unwrap() > 0);
    assert!(growth["next"].as_u64().unwrap() < 16);
    assert_eq!(
        read_report(&d.path().join("out/run_report.json"))
            .unwrap()
            .status,
        "interrupted"
    );
    fs::remove_file(d.path().join("out/STOP")).unwrap();
    // Resume with the same executable: checkpoints deliberately authenticate binary identity.
    let mut resume = fs::read_to_string(&c).unwrap();
    resume.push_str(&format!(
        "  checkpoint: {{resume_from: '{}'}}\n",
        checkpoint.display()
    ));
    fs::write(&c, &resume).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_rustmspt"))
        .args(["pack", "--config"])
        .arg(&c)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let resumed = json(&d.path().join("out/aggregate_templates.json"));
    let particles = fs::read(d.path().join("out/particles.json")).unwrap();
    fs::write(&c, resume.split("  checkpoint:").next().unwrap()).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_rustmspt"))
        .args(["pack", "--config"])
        .arg(&c)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        resumed,
        json(&d.path().join("out/aggregate_templates.json"))
    );
    assert_eq!(
        particles,
        fs::read(d.path().join("out/particles.json")).unwrap()
    );
}
