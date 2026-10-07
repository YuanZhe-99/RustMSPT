use rustmspt::config::{load_pack_document, PackDocument, ResolvedPlacement};
use rustmspt::geometry::{icosphere_mesh, transform_shell, UnitQuat};
use rustmspt::io::{save_stl, sha256_file};
use rustmspt::pipeline::placement::{read_record, read_report, run_placement};
use rustmspt::pipeline::placement_geometry::{GeometryCache, GeometryHandle};
use rustmspt::types::Vec3;
use std::{fs, path::Path, sync::Arc};

// AI-FUNC-SUMMARY: Resolve a small generic fixture config in the temporary directory.
fn resolve(path: &Path) -> ResolvedPlacement {
    match load_pack_document(path).unwrap() {
        PackDocument::Placement(p) => p.validate(path).unwrap(),
        _ => panic!("not placement"),
    }
}
// AI-FUNC-SUMMARY: Write a synthetic source and configurable pack case with optional small aggregate stages.
fn fixture(dir: &Path, mode: &str, budget: usize, target: f64) -> ResolvedPlacement {
    let source = dir.join("source.stl");
    if !source.exists() {
        save_stl(
            &source,
            &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1),
            "source",
        )
        .unwrap();
    }
    let aggregates = match mode {
        "clusters" => "  aggregates: {enabled: true, mode: clusters, variants: 1, particles_per_cluster: 4, internal_gap: 0.02, compaction_sweeps: 2, mesh_refinement_sweeps: 0}\n",
        "mixed" => "  aggregates: {enabled: true, mode: mixed, variants: 1, particles_per_cluster: 4, fallback_particles_per_cluster: [1], internal_gap: 0.02, compaction_sweeps: 2, mesh_refinement_sweeps: 0}\n",
        _ => "",
    };
    let path = dir.join("config.yaml");
    fs::write(&path, format!("placement:\n  seed: 772\n  threads: 2\n  frame: {{unit: um}}\n  domain: {{min: [0,0,0], max: [20,20,20]}}\n  shapes: {{files: [source.stl]}}\n  size:\n    distribution: {{kind: lognormal, median: 2.0, sigma_log: 0.2, min: 1.5, max: 3.0}}\n    classes: {{kind: equal_width, count: 4}}\n  gaps: {{particle_particle: 0.05}}\n  target: {{volume_fraction: {target}}}\n  budget: {{attempts_per_particle: 60, total_attempts: {budget}}}\n  outputs: {{dir: out}}\n  checkpoint: {{every_particles: 1, interval_seconds: 0}}\n{aggregates}")).unwrap();
    resolve(&path)
}
// AI-FUNC-SUMMARY: Compare final geometry, transforms, counters and size statistics, ignoring elapsed time/output locations.
fn same_result(a: &ResolvedPlacement, b: &ResolvedPlacement) {
    assert_eq!(
        fs::read(&a.outputs.particles_stl).unwrap(),
        fs::read(&b.outputs.particles_stl).unwrap()
    );
    let a_record = read_record(&a.outputs.record).unwrap();
    let b_record = read_record(&b.outputs.record).unwrap();
    assert_eq!(
        serde_json::to_value(a_record.particles).unwrap(),
        serde_json::to_value(b_record.particles).unwrap()
    );
    assert_eq!(
        fs::read(&a.outputs.size_csv).unwrap(),
        fs::read(&b.outputs.size_csv).unwrap()
    );
    let a_report = serde_json::to_value(read_report(&a.outputs.report).unwrap()).unwrap();
    let b_report = serde_json::to_value(read_report(&b.outputs.report).unwrap()).unwrap();
    for field in ["actual", "plan", "rejections", "top_up", "stop_reason"] {
        assert_eq!(a_report[field], b_report[field], "{field}");
    }
}
#[test]
// AI-FUNC-SUMMARY: Exhaust a global budget inside a particle and recover its exact RNG/per-particle attempt cursor under a larger global budget.
fn individual_budget_resume_matches_uninterrupted_and_final_is_saved() {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = fixture(tmp.path(), "individual", 17, 0.08);
    run_placement(&c).unwrap();
    let checkpoint = c.outputs.dir.join("checkpoint.json");
    let partial = read_record(&c.outputs.record).unwrap().particles;
    let snap: serde_json::Value = serde_json::from_slice(&fs::read(&checkpoint).unwrap()).unwrap();
    assert!(!snap["snapshot"]["complete"].as_bool().unwrap());
    c.budget.total_attempts = 200000;
    c.checkpoint.resume_from = Some(checkpoint.to_string_lossy().into_owned());
    run_placement(&c).unwrap();
    let final_particles = read_record(&c.outputs.record).unwrap().particles;
    assert_eq!(
        serde_json::to_value(partial.clone()).unwrap(),
        serde_json::to_value(&final_particles[..partial.len()]).unwrap()
    );
    let completed: serde_json::Value =
        serde_json::from_slice(&fs::read(&checkpoint).unwrap()).unwrap();
    assert!(completed["snapshot"]["complete"].as_bool().unwrap());
    let mut clean = c.clone();
    clean.checkpoint.resume_from = None;
    clean.outputs.dir = tmp.path().join("clean");
    clean.outputs.particles_stl = clean.outputs.dir.join("particles.stl");
    clean.outputs.record = clean.outputs.dir.join("particles.json");
    clean.outputs.report = clean.outputs.dir.join("run_report.json");
    clean.outputs.size_csv = clean.outputs.dir.join("size_distribution.csv");
    clean.memory.geometry_cache_mb = 0;
    clean.memory.simplified_collision = true;
    run_placement(&clean).unwrap();
    same_result(&c, &clean);
}
#[test]
// AI-FUNC-SUMMARY: Resume the saved prior generation for individual/cluster/mixed tasks without altering final accepted geometry or accounting.
fn prior_generation_resume_matches_all_modes() {
    for mode in ["individual", "clusters", "mixed"] {
        let tmp = tempfile::tempdir().unwrap();
        let mut c = fixture(tmp.path(), mode, 20000, 0.08);
        run_placement(&c).unwrap();
        let stl = fs::read(&c.outputs.particles_stl).unwrap();
        let record =
            serde_json::to_value(read_record(&c.outputs.record).unwrap().particles).unwrap();
        let csv = fs::read(&c.outputs.size_csv).unwrap();
        let previous = c.outputs.dir.join("checkpoint.previous.json");
        let copy = tmp.path().join("saved.json");
        fs::copy(previous, &copy).unwrap();
        c.checkpoint.resume_from = Some(copy.to_string_lossy().into_owned());
        run_placement(&c).unwrap();
        assert_eq!(stl, fs::read(&c.outputs.particles_stl).unwrap(), "{mode}");
        assert_eq!(
            record,
            serde_json::to_value(read_record(&c.outputs.record).unwrap().particles).unwrap(),
            "{mode}"
        );
        assert_eq!(csv, fs::read(&c.outputs.size_csv).unwrap(), "{mode}");
    }
}
#[test]
// AI-FUNC-SUMMARY: Extend successful packs in each mode, preserving every old particle transform, and retain a fresh final checkpoint.
fn extend_completed_pack_preserves_prefix_all_modes() {
    for mode in ["individual", "clusters", "mixed"] {
        let tmp = tempfile::tempdir().unwrap();
        let mut c = fixture(tmp.path(), mode, 20000, 0.02);
        run_placement(&c).unwrap();
        let original = read_record(&c.outputs.record).unwrap().particles;
        c.checkpoint.resume_from = Some(
            c.outputs
                .dir
                .join("checkpoint.json")
                .to_string_lossy()
                .into_owned(),
        );
        c.checkpoint.extend = true;
        c.target_volume_fraction = 0.04;
        run_placement(&c).unwrap();
        let extended = read_record(&c.outputs.record).unwrap().particles;
        assert!(extended.len() > original.len(), "{mode}");
        assert_eq!(
            serde_json::to_value(&original).unwrap(),
            serde_json::to_value(&extended[..original.len()]).unwrap(),
            "{mode}"
        );
        assert!(c.outputs.dir.join("checkpoint.json").exists());
    }
}
#[test]
// AI-FUNC-SUMMARY: Corrupt the current generation and recover the verified prior generation; input/config mismatch must fail before overwriting final output.
fn corruption_fallback_and_input_change_refusal() {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = fixture(tmp.path(), "individual", 20000, 0.03);
    run_placement(&c).unwrap();
    let hash = sha256_file(&c.outputs.particles_stl).unwrap();
    let checkpoint = c.outputs.dir.join("checkpoint.json");
    fs::write(&checkpoint, b"broken").unwrap();
    c.checkpoint.resume_from = Some(checkpoint.to_string_lossy().into_owned());
    run_placement(&c).unwrap();
    assert_eq!(hash, sha256_file(&c.outputs.particles_stl).unwrap());
    // The input file hash is authoritative even when path and shell ordinal stay the same.
    save_stl(
        &tmp.path().join("source.stl"),
        &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.1, 1),
        "changed",
    )
    .unwrap();
    assert!(run_placement(&c).is_err());
    assert_eq!(hash, sha256_file(&c.outputs.particles_stl).unwrap());
}
#[test]
// AI-FUNC-SUMMARY: Check immutable geometry reconstruction and active pins across eviction, cache disable and transforms.
fn cache_eviction_keeps_active_geometry_exact() {
    let source = Arc::new(icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1));
    let cache = Arc::new(GeometryCache::new(60000));
    let a = GeometryHandle::new(
        0,
        source.clone(),
        1.0,
        UnitQuat::identity(),
        Vec3::new(1.0, 2.0, 3.0),
        cache.clone(),
        true,
    );
    let b = GeometryHandle::new(
        1,
        source.clone(),
        2.0,
        UnitQuat::identity(),
        Vec3::new(6.0, 7.0, 8.0),
        cache.clone(),
        true,
    );
    let pinned = a.get();
    let expected = pinned.mesh.vertices.clone();
    let other = b.get();
    assert_eq!(
        other.mesh.vertices,
        transform_shell(&source, 2.0, UnitQuat::identity(), Vec3::new(6.0, 7.0, 8.0)).vertices
    );
    assert_eq!(pinned.mesh.vertices, expected);
    assert_eq!(a.get().mesh.vertices, expected);
    // Active geometry stays valid even when the second mesh cannot be retained.
    drop(other);
    drop(pinned);
    let _ = b.get();
    assert!(cache.summary().contains("evictions=1"));
    assert_eq!(a.get().mesh.vertices, expected);
}
#[test]
// AI-FUNC-SUMMARY: Prove a particle deep inside a pore is rejected even when no pore surface triangle is near its bbox.
fn forbidden_void_rejects_deeply_enclosed_particle_without_surface_hit() {
    use rustmspt::config::placement::VoidCrossing;
    use rustmspt::geometry::{box_mesh, mesh_bbox, mesh_volume, VoidIndex};
    use rustmspt::pipeline::placement_feasibility::{
        check_placement, Candidate, FeasibilityContext, RejectReason,
    };
    use rustmspt::types::BoundingBox;
    let tmp = tempfile::tempdir().unwrap();
    let c = fixture(tmp.path(), "individual", 100, 0.01);
    let pore = box_mesh(BoundingBox {
        min: Vec3::new(2.0, 2.0, 2.0),
        max: Vec3::new(18.0, 18.0, 18.0),
    });
    let index = VoidIndex::build(&pore).unwrap();
    let particle = icosphere_mesh(Vec3::new(10.0, 10.0, 10.0), 0.5, 1);
    let bbox = mesh_bbox(&particle).unwrap();
    assert!(!index.near_box(bbox, 0.1));
    let ctx = FeasibilityContext {
        domain: c.domain,
        boundary: &c.boundary,
        gap_particle_particle: c.gap_particle_particle,
        placed: &[],
        neighbours: &[],
        void: Some(&index),
        void_crossing: VoidCrossing::Forbidden,
        void_gap: 0.1,
        neighbourhood_band: None,
        pair_parallel_min: usize::MAX,
    };
    let result = check_placement(
        &ctx,
        &Candidate {
            mesh: &particle,
            bbox,
            centre: Vec3::new(10.0, 10.0, 10.0),
            reach: 0.5,
            volume_full: mesh_volume(&particle),
        },
    );
    assert!(matches!(result, Err(RejectReason::InsideVoid)));
}
#[cfg(unix)]
#[test]
// AI-FUNC-SUMMARY: Interrupt each CLI mode after acceptance, restore with the same executable, and compare geometry/accounting with an uninterrupted run.
fn sigint_resume_matches_uninterrupted_all_modes() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    for mode in ["individual", "clusters", "mixed"] {
        let tmp = tempfile::tempdir().unwrap();
        let c = fixture(tmp.path(), mode, 20000, 0.10);
        let mut child = Command::new(env!("CARGO_BIN_EXE_rustmspt"))
            .args(["pack", "--config"])
            .arg(&c.config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "{mode} ended before interrupt"
            );
            let progress = fs::read(c.outputs.dir.join("progress.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
            if progress.is_some_and(|p| p["particles"].as_u64().unwrap_or(0) > 0) {
                break;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{mode} did not accept a particle");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{mode} interrupt did not finish");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let report = read_report(&c.outputs.report).unwrap();
        assert_eq!(report.status, "interrupted");
        let prefix = read_record(&c.outputs.record).unwrap().particles;
        let initial_text = fs::read_to_string(&c.config_path).unwrap();
        let resume_text = initial_text.replace(
            "checkpoint: {every_particles: 1, interval_seconds: 0}",
            &format!(
                "checkpoint: {{every_particles: 1, interval_seconds: 0, resume_from: '{}'}}",
                c.outputs.dir.join("checkpoint.json").display()
            ),
        );
        fs::write(&c.config_path, resume_text).unwrap();
        assert!(Command::new(env!("CARGO_BIN_EXE_rustmspt"))
            .args(["pack", "--config"])
            .arg(&c.config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success());
        let resumed = read_record(&c.outputs.record).unwrap().particles;
        assert_eq!(
            serde_json::to_value(&prefix).unwrap(),
            serde_json::to_value(&resumed[..prefix.len()]).unwrap()
        );
        let stl = fs::read(&c.outputs.particles_stl).unwrap();
        let csv = fs::read(&c.outputs.size_csv).unwrap();
        let continued_report =
            serde_json::to_value(read_report(&c.outputs.report).unwrap()).unwrap();
        let clean_text = initial_text.replace("outputs: {dir: out}", "outputs: {dir: clean}");
        fs::write(&c.config_path, clean_text).unwrap();
        assert!(Command::new(env!("CARGO_BIN_EXE_rustmspt"))
            .args(["pack", "--config"])
            .arg(&c.config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success());
        let clean = resolve(&c.config_path);
        assert_eq!(
            stl,
            fs::read(&clean.outputs.particles_stl).unwrap(),
            "{mode}"
        );
        assert_eq!(csv, fs::read(&clean.outputs.size_csv).unwrap(), "{mode}");
        let clean_report =
            serde_json::to_value(read_report(&clean.outputs.report).unwrap()).unwrap();
        for field in ["actual", "plan", "rejections", "top_up", "stop_reason"] {
            assert_eq!(
                continued_report[field], clean_report[field],
                "{mode}: {field}"
            );
        }
        assert_eq!(
            continued_report["budget"]["consumed_attempts"],
            clean_report["budget"]["consumed_attempts"],
            "{mode}: attempts"
        );
    }
}
#[test]
// AI-FUNC-SUMMARY: Preserve nested custom STL output paths and reject collisions with durable checkpoint storage.
fn nested_stl_output_and_checkpoint_name_conflicts() {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = fixture(tmp.path(), "individual", 10000, 0.01);
    c.outputs.particles_stl = c.outputs.dir.join("nested/particles.stl");
    run_placement(&c).unwrap();
    assert!(c.outputs.particles_stl.is_file());
    c.outputs.record = c.outputs.dir.join("checkpoint.json");
    assert!(run_placement(&c).is_err());
}
#[test]
// AI-FUNC-SUMMARY: Completion remains durable when the final cluster acceptance consumes exactly the global attempt limit.
fn final_acceptance_at_budget_limit_keeps_completed_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = fixture(tmp.path(), "clusters", 20000, 0.02);
    run_placement(&c).unwrap();
    let reference = fs::read(&c.outputs.particles_stl).unwrap();
    let report = serde_json::to_value(read_report(&c.outputs.report).unwrap()).unwrap();
    c.budget.total_attempts = report["budget"]["consumed_attempts"].as_u64().unwrap() as usize;
    run_placement(&c).unwrap();
    assert_eq!(reference, fs::read(&c.outputs.particles_stl).unwrap());
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&fs::read(c.outputs.dir.join("checkpoint.json")).unwrap()).unwrap();
    assert_eq!(checkpoint["snapshot"]["complete"], true);
}
