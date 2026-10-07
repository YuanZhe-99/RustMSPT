use rustmspt::config::placement::InitialParticlesSpec;
use rustmspt::config::{load_pack_document, PackDocument, ResolvedPlacement};
use rustmspt::geometry::icosphere_mesh;
use rustmspt::io::{save_stl, sha256_file};
use rustmspt::pipeline::placement::{read_record, read_report, run_placement};
use rustmspt::types::Vec3;
use std::{fs, path::Path};

fn fixture(dir: &Path, aggregate: bool) -> ResolvedPlacement {
    let source = dir.join("sphere.stl");
    save_stl(
        &source,
        &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1),
        "sphere",
    )
    .unwrap();
    let config = dir.join("config.yaml");
    let cluster = if aggregate {
        "  aggregates: {enabled: true, mode: clusters, variants: 2, particles_per_cluster: 4, internal_gap: 0.02, compaction_sweeps: 4, mesh_refinement_sweeps: 0}\n"
    } else {
        ""
    };
    fs::write(&config, format!("placement:\n  seed: 972\n  threads: 2\n  frame: {{unit: um}}\n  domain: {{min: [0,0,0], max: [15,15,15]}}\n  shapes: {{files: [sphere.stl]}}\n  size:\n    distribution: {{kind: lognormal, median: 2, sigma_log: 0.1, min: 1.8, max: 2.2}}\n    classes: {{kind: equal_width, count: 4}}\n  gaps: {{particle_particle: 0.3}}\n  boundary: {{mode: strict, min_boundary_dist: 0.1}}\n  target: {{volume_fraction: 0.02}}\n  budget: {{attempts_per_particle: 100, total_attempts: 10000}}\n  outputs: {{dir: original}}\n{cluster}")).unwrap();
    match load_pack_document(&config).unwrap() {
        PackDocument::Placement(p) => p.validate(&config).unwrap(),
        _ => panic!("placement"),
    }
}

fn fill(original: &ResolvedPlacement) -> ResolvedPlacement {
    let mut c = original.clone();
    c.aggregates.enabled = false;
    c.target_volume_fraction = 0.05;
    c.initial_particles = Some(InitialParticlesSpec {
        record: original.outputs.record.clone(),
        report: original.outputs.report.clone(),
        existing_gap: 0.02,
        pending_checkpoint: None,
        retry_failed: true,
    });
    let out = original.outputs.dir.parent().unwrap().join("fill");
    c.outputs.dir = out.clone();
    c.outputs.record = out.join("particles.json");
    c.outputs.report = out.join("run_report.json");
    c.outputs.particles_stl = out.join("particles.stl");
    c.outputs.size_csv = out.join("size_distribution.csv");
    c
}

#[test]
fn imported_clusters_keep_exact_prefix_and_append_real_particles() {
    let temp = tempfile::tempdir().unwrap();
    let original = fixture(temp.path(), true);
    run_placement(&original).unwrap();
    let before = read_record(&original.outputs.record).unwrap();
    let c = fill(&original);
    run_placement(&c).unwrap();
    let after = read_record(&c.outputs.record).unwrap();
    assert!(after.particles.len() > before.particles.len());
    for (i, (before, after)) in before.particles.iter().zip(&after.particles).enumerate() {
        let a = serde_json::to_value(before).unwrap();
        let b = serde_json::to_value(after).unwrap();
        for (field, value) in a.as_object().unwrap() {
            assert_eq!(value, &b[field], "particle {i} field {field}");
        }
    }
    assert!(
        read_report(&c.outputs.report)
            .unwrap()
            .actual
            .volume_fraction_domain
            >= 0.05 * 0.995
    );
    assert!(c.outputs.dir.join("checkpoint.json").is_file());
}

#[test]
fn imported_geometry_is_rechecked_even_with_a_matching_record_digest() {
    let temp = tempfile::tempdir().unwrap();
    let original = fixture(temp.path(), false);
    run_placement(&original).unwrap();
    let mut record = read_record(&original.outputs.record).unwrap();
    assert!(record.particles.len() > 1);
    record.particles[1].translation = record.particles[0].translation;
    fs::write(
        &original.outputs.record,
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();
    let mut report = read_report(&original.outputs.report).unwrap();
    report
        .outputs
        .iter_mut()
        .find(|e| e.role == "record")
        .unwrap()
        .sha256 = Some(sha256_file(&original.outputs.record).unwrap().0);
    fs::write(
        &original.outputs.report,
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    let error = run_placement(&fill(&original)).unwrap_err().to_string();
    assert!(error.contains("initial_particles"), "{error}");
}

#[test]
fn imported_source_change_and_record_damage_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let original = fixture(temp.path(), false);
    run_placement(&original).unwrap();
    let source = temp.path().join("sphere.stl");
    save_stl(
        &source,
        &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.01, 1),
        "changed",
    )
    .unwrap();
    assert!(run_placement(&fill(&original))
        .unwrap_err()
        .to_string()
        .contains("source geometry"));
    save_stl(
        &source,
        &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1),
        "sphere",
    )
    .unwrap();
    let record = fs::read_to_string(&original.outputs.record).unwrap();
    fs::write(&original.outputs.record, record + " ").unwrap();
    assert!(run_placement(&fill(&original))
        .unwrap_err()
        .to_string()
        .contains("digest"));
}

#[test]
fn imported_task_can_be_resumed_and_extended_without_reimporting() {
    let temp = tempfile::tempdir().unwrap();
    let original = fixture(temp.path(), true);
    run_placement(&original).unwrap();
    let mut c = fill(&original);
    c.budget.total_attempts = 7;
    run_placement(&c).unwrap();
    let prefix = read_record(&c.outputs.record).unwrap().particles;
    c.checkpoint.resume_from = Some(
        c.outputs
            .dir
            .join("checkpoint.json")
            .to_string_lossy()
            .into_owned(),
    );
    c.budget.total_attempts = 10000;
    run_placement(&c).unwrap();
    let complete = read_record(&c.outputs.record).unwrap().particles;
    assert_eq!(
        serde_json::to_value(&prefix).unwrap(),
        serde_json::to_value(&complete[..prefix.len()]).unwrap()
    );
    c.target_volume_fraction = 0.07;
    c.checkpoint.extend = true;
    run_placement(&c).unwrap();
    let extended = read_record(&c.outputs.record).unwrap().particles;
    assert_eq!(
        serde_json::to_value(&complete).unwrap(),
        serde_json::to_value(&extended[..complete.len()]).unwrap()
    );
}
