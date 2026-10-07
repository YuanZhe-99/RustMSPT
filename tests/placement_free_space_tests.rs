use rustmspt::config::placement::{FreeSpaceSpec, InitialParticlesSpec, PositionMode};
use rustmspt::config::{load_pack_document, PackDocument, ResolvedPlacement};
use rustmspt::geometry::{
    box_mesh, icosphere_mesh, mesh_distance_exact, transform_shell, UnitQuat,
};
use rustmspt::io::{load_stl, save_stl};
use rustmspt::pipeline::placement::{read_record, read_report, run_placement};
use rustmspt::types::{BoundingBox, Vec3};
use serde_json::Value;
use std::{fs, path::Path};

// AI-FUNC-SUMMARY: Resolve one test config.
fn resolve(path: &Path) -> ResolvedPlacement {
    match load_pack_document(path).unwrap() {
        PackDocument::Placement(p) => p.validate(path).unwrap(),
        _ => panic!("placement"),
    }
}

// AI-FUNC-SUMMARY: Build a small seeded fixture; optional pore leaves a thin available channel.
fn fixture(dir: &Path, guided: bool, channel: bool) -> ResolvedPlacement {
    save_stl(
        &dir.join("source.stl"),
        &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1),
        "source",
    )
    .unwrap();
    let pore = if channel {
        save_stl(
            &dir.join("void.stl"),
            &box_mesh(BoundingBox {
                min: Vec3::new(0.0, 0.0, 0.0),
                max: Vec3::new(9.0, 10.0, 10.0),
            }),
            "pore",
        )
        .unwrap();
        "  void: {file: void.stl, crossing: forbidden, gap: 0.1}\n"
    } else {
        ""
    };
    let sizes = if channel {
        "median: 0.6, sigma_log: 0.1, min: 0.5, max: 0.7"
    } else {
        "median: 2, sigma_log: 0.15, min: 1.6, max: 2.4"
    };
    let position = if guided {
        if channel {
            "mode: free_space_guided, free_space: {coarse_cell_size: 0.5, min_cell_size: 0.125, max_cells: 15000, index_memory_mb: 8, exploration_fraction: 0.1, candidates_per_location: 4}"
        } else {
            "mode: free_space_guided, free_space: {coarse_cell_size: 2, min_cell_size: 0.5, max_cells: 10000, index_memory_mb: 8, exploration_fraction: 0.1, candidates_per_location: 4}"
        }
    } else {
        "mode: feasible_uniform"
    };
    let path = dir.join("config.yaml");
    fs::write(&path, format!("placement:\n  seed: 714\n  threads: 2\n  frame: {{unit: um}}\n  domain: {{min: [0,0,0], max: [10,10,10]}}\n  shapes: {{files: [source.stl]}}\n  size:\n    distribution: {{kind: lognormal, {sizes}}}\n    classes: {{kind: equal_width, count: 5}}\n    on_unattainable: skip_reported\n  position: {{{position}}}\n  boundary: {{mode: strict, min_boundary_dist: 0.05}}\n  gaps: {{particle_particle: 0.1}}\n  target: {{volume_fraction: {}}}\n  budget: {{attempts_per_particle: 100, total_attempts: 10000}}\n  outputs: {{dir: out}}\n  checkpoint: {{enabled: true, interval_seconds: 0, every_particles: 1}}\n{pore}", if channel {0.01} else {0.07})).unwrap();
    resolve(&path)
}

fn destination(c: &mut ResolvedPlacement, dir: &Path) {
    c.outputs.dir = dir.into();
    c.outputs.record = dir.join("particles.json");
    c.outputs.report = dir.join("run_report.json");
    c.outputs.particles_stl = dir.join("particles.stl");
    c.outputs.size_csv = dir.join("size_distribution.csv");
}

// AI-FUNC-SUMMARY: Check every actual particle pair independently of cavity score decisions.
fn check_pairs(c: &ResolvedPlacement) {
    let record = read_record(&c.outputs.record).unwrap();
    let original = load_stl(&c.shape_files[0]).unwrap();
    let meshes = record
        .particles
        .iter()
        .map(|p| {
            let centre = Vec3::new(
                p.source_shape.shell_centroid[0],
                p.source_shape.shell_centroid[1],
                p.source_shape.shell_centroid[2],
            );
            let mut canonical = original.clone();
            rustmspt::geometry::translate_mesh(&mut canonical, centre.scale(-1.0));
            let q = p.rotation.quaternion;
            transform_shell(
                &canonical,
                p.scale,
                UnitQuat {
                    w: q[0],
                    x: q[1],
                    y: q[2],
                    z: q[3],
                },
                Vec3::new(p.translation[0], p.translation[1], p.translation[2]),
            )
        })
        .collect::<Vec<_>>();
    for (i, a) in meshes.iter().enumerate() {
        for b in &meshes[i + 1..] {
            assert!(mesh_distance_exact(a, b) + 1e-8 >= c.gap_particle_particle);
        }
    }
}

#[test]
fn guided_geometry_and_order_are_identical_across_threads() {
    let t = tempfile::tempdir().unwrap();
    let mut c = fixture(t.path(), true, false);
    c.threads = 1;
    run_placement(&c).unwrap();
    let bytes = fs::read(&c.outputs.particles_stl).unwrap();
    let record = read_record(&c.outputs.record).unwrap();
    check_pairs(&c);
    c.threads = 4;
    destination(&mut c, &t.path().join("four"));
    run_placement(&c).unwrap();
    assert_eq!(bytes, fs::read(&c.outputs.particles_stl).unwrap());
    assert_eq!(
        serde_json::to_value(record.particles).unwrap(),
        serde_json::to_value(read_record(&c.outputs.record).unwrap().particles).unwrap()
    );
    let report = read_report(&c.outputs.report).unwrap();
    assert_eq!(
        report.samplers["position"],
        "adaptive_real_geometry_cavity_guidance"
    );
    assert!(report.actual.volume_fraction_domain >= 0.07 * 0.995);
}

#[test]
fn guided_search_finds_thin_channel_under_the_same_proposal_budget() {
    let mut samples = Vec::new();
    for seed in [714, 715, 716] {
        let t = tempfile::tempdir().unwrap();
        let mut random = fixture(t.path(), false, true);
        random.seed = seed;
        random.budget.total_attempts = 32;
        run_placement(&random).unwrap();
        let baseline = read_report(&random.outputs.report).unwrap();
        let mut guided = random.clone();
        guided.position.mode = PositionMode::FreeSpaceGuided;
        guided.position.free_space = FreeSpaceSpec {
            coarse_cell_size: 0.5,
            min_cell_size: 0.125,
            max_cells: 15000,
            index_memory_mb: 8,
            candidates_per_location: 4,
            exploration_fraction: 0.1,
            local_refinement: true,
        };
        destination(&mut guided, &t.path().join("guided"));
        run_placement(&guided).unwrap();
        let result = read_report(&guided.outputs.report).unwrap();
        samples.push((seed, baseline.actual.particles, result.actual.particles));
        let void =
            rustmspt::geometry::VoidIndex::build(&load_stl(&t.path().join("void.stl")).unwrap())
                .unwrap();
        for p in read_record(&guided.outputs.record).unwrap().particles {
            assert!(!void.contains_point(Vec3::new(
                p.translation[0],
                p.translation[1],
                p.translation[2]
            )));
        }
        check_pairs(&guided);
    }
    eprintln!("[GuidedComparison] fixed_seeds_random_guided={samples:?} geometry_budget=32");
    assert!(
        samples.iter().map(|s| s.2).sum::<usize>() > samples.iter().map(|s| s.1).sum::<usize>(),
        "{samples:?}"
    );
}

#[test]
fn partial_guided_cursor_and_cell_scores_resume_exactly() {
    let t = tempfile::tempdir().unwrap();
    let mut c = fixture(t.path(), true, false);
    c.budget.total_attempts = 7;
    run_placement(&c).unwrap();
    let prefix = read_record(&c.outputs.record).unwrap().particles;
    let checkpoint = c.outputs.dir.join("checkpoint.json");
    let snap: Value = serde_json::from_slice(&fs::read(&checkpoint).unwrap()).unwrap();
    assert_eq!(
        snap["snapshot"]["schema"],
        "rustmspt.placement.checkpoint/2"
    );
    assert!(snap["snapshot"]["free_space"]["cells"].is_array());
    c.checkpoint.resume_from = Some(checkpoint.to_string_lossy().into_owned());
    c.budget.total_attempts = 10000;
    run_placement(&c).unwrap();
    let completed = read_record(&c.outputs.record).unwrap().particles;
    assert_eq!(
        serde_json::to_value(&prefix).unwrap(),
        serde_json::to_value(&completed[..prefix.len()]).unwrap()
    );
    let stl = fs::read(&c.outputs.particles_stl).unwrap();
    c.checkpoint.resume_from = None;
    destination(&mut c, &t.path().join("clean"));
    run_placement(&c).unwrap();
    assert_eq!(stl, fs::read(&c.outputs.particles_stl).unwrap());
}

#[test]
fn import_preserves_original_population_and_remaining_size_multiset() {
    let t = tempfile::tempdir().unwrap();
    let mut original = fixture(t.path(), false, false);
    original.budget.total_attempts = 12;
    original.budget.attempts_per_particle = 1;
    run_placement(&original).unwrap();
    let before = read_record(&original.outputs.record).unwrap();
    let original_report = read_report(&original.outputs.report).unwrap();
    let mut c = original.clone();
    c.position.mode = PositionMode::FreeSpaceGuided;
    c.position.free_space.coarse_cell_size = 2.0;
    c.position.free_space.min_cell_size = 0.5;
    c.budget.total_attempts = 10000;
    c.budget.attempts_per_particle = 100;
    c.initial_particles = Some(InitialParticlesSpec {
        record: original.outputs.record.clone(),
        report: original.outputs.report.clone(),
        existing_gap: 0.1,
        pending_checkpoint: Some(original.outputs.dir.join("checkpoint.json")),
        retry_failed: true,
    });
    destination(&mut c, &t.path().join("imported"));
    run_placement(&c).unwrap();
    let after = read_record(&c.outputs.record).unwrap();
    assert_eq!(
        serde_json::to_value(&before.particles).unwrap(),
        serde_json::to_value(&after.particles[..before.particles.len()]).unwrap()
    );
    let report = read_report(&c.outputs.report).unwrap();
    assert_eq!(
        report.plan.planned_particles,
        original_report.plan.planned_particles
    );
    assert!((report.plan.planned_volume - original_report.plan.planned_volume).abs() < 1e-8);
    assert!(after.particles.len() >= before.particles.len());
    check_pairs(&c);
}

#[test]
fn free_space_caps_and_configuration_combinations_are_explicit() {
    let t = tempfile::tempdir().unwrap();
    let c = fixture(t.path(), true, false);
    let path = t.path().join("config.yaml");
    let source = fs::read_to_string(&path).unwrap();
    for modified in [
        source.replace("index_memory_mb: 8", "index_memory_mb: 0"),
        source.replace("max_cells: 10000", "max_cells: 1"),
        source.replace("min_cell_size: 0.5", "min_cell_size: 4"),
        source.replace("exploration_fraction: 0.1", "exploration_fraction: 1.1"),
        source.replace("mode: free_space_guided", "mode: feasible_uniform"),
    ] {
        fs::write(&path, modified).unwrap();
        let p = match load_pack_document(&path).unwrap() {
            PackDocument::Placement(p) => p,
            _ => panic!(),
        };
        assert!(p.validate(&path).is_err());
    }
    assert!(c.position.free_space.max_cells > 0);
}
