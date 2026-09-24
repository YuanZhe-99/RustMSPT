use rustmspt::config::{load_pack_document, PackDocument};
use rustmspt::config::placement::{
    config_dir, resolve_against, BoundaryMode, OnUnattainable, OrientationMode, PlacementOrder,
    PositionMode, ResolvedClasses, ResolvedDistribution, TargetBasis, VoidCrossing,
};
use std::fs;
use std::path::Path;

/// A complete, valid placement config. Tests edit one field of this rather than
/// each writing their own, so a refusal is always attributable to that one edit.
fn base_yaml() -> String {
    r#"
placement:
  seed: 20260910
  frame: { unit: "um" }
  domain: { min: [0, 0, 0], max: [100, 100, 100] }
  shapes:
    files: ["shapes/particles.stl"]
    filters: { max_aspect_ratio: 3.0 }
  size:
    distribution: { kind: lognormal, median: 12.0, sigma_log: 0.35, min: 5.0, max: 30.0 }
  target: { volume_fraction: 0.2 }
  gaps: { particle_particle: 1.0 }
  outputs:
    dir: "out/run1"
"#
    .to_string()
}

fn write(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    fs::write(&p, text).expect("write config");
    p
}

fn load_resolved(dir: &Path, text: &str) -> rustmspt::config::ResolvedPlacement {
    let p = write(dir, "placement.yaml", text);
    match load_pack_document(&p).expect("document loads") {
        PackDocument::Placement(params) => params.validate(&p).expect("config validates"),
        PackDocument::Legacy(_) => panic!("expected the placement engine"),
    }
}

fn refusal(dir: &Path, text: &str) -> String {
    let p = write(dir, "placement.yaml", text);
    match load_pack_document(&p) {
        Err(e) => e.to_string(),
        Ok(PackDocument::Placement(params)) => match params.validate(&p) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected a refusal, got a valid config"),
        },
        Ok(PackDocument::Legacy(_)) => panic!("expected the placement engine"),
    }
}

// ---------------------------------------------------------------- engine selection

#[test]
fn a_placement_block_selects_the_placement_engine() {
    let tmp = tempfile::tempdir().unwrap();
    let resolved = load_resolved(tmp.path(), &base_yaml());
    assert_eq!(resolved.seed, 20260910);
    assert_eq!(resolved.unit, "um");
    assert_eq!(resolved.domain.max.x, 100.0);
}

// The regression that matters most: the repository's own committed pack config
// must keep loading, and keep selecting the original engine.
#[test]
fn the_repositorys_own_pack_config_still_selects_the_legacy_engine() {
    let path = Path::new("data/input/pack_config.yaml");
    assert!(path.exists(), "the committed legacy config is missing");
    match load_pack_document(path).expect("legacy config loads") {
        PackDocument::Legacy(conf) => {
            assert_eq!(conf.packing.mode, 2);
            assert!(conf.packing.target_volume_fraction > 0.0);
        }
        PackDocument::Placement(_) => panic!("a packing: config must select the legacy engine"),
    }
}

// PackingConfig has four required fields, so a placement-only document cannot be
// probed by attempting the legacy type first: it would fail with "missing field
// `input`", which says nothing about which engine was meant.
#[test]
fn both_blocks_or_neither_is_refused_by_name() {
    let tmp = tempfile::tempdir().unwrap();

    let both = format!("{}\npacking:\n  target_volume_fraction: 0.1\n  mode: 2\n  max_attempts: 10\n", base_yaml());
    let p = write(tmp.path(), "both.yaml", &both);
    let err = load_pack_document(&p).unwrap_err().to_string();
    assert!(err.contains("placement:") && err.contains("packing:"), "{err}");

    let p = write(tmp.path(), "neither.yaml", "input:\n  path: \"a.stl\"\n");
    let err = load_pack_document(&p).unwrap_err().to_string();
    assert!(err.contains("placement:") && err.contains("packing:"), "{err}");
}

// A misspelled key must be refused, not defaulted. This is the whole reason the
// placement structs set deny_unknown_fields where the older ones do not.
#[test]
fn an_unknown_key_is_refused_at_every_nesting_level() {
    let tmp = tempfile::tempdir().unwrap();
    for (label, bad) in [
        ("top level", base_yaml().replace("  seed: 20260910", "  seed: 20260910\n  sed: 1")),
        ("nested one deep", base_yaml().replace("  frame: { unit: \"um\" }", "  frame: { unit: \"um\", unti: \"mm\" }")),
        ("nested two deep", base_yaml().replace(
            "    distribution: { kind: lognormal, median: 12.0, sigma_log: 0.35, min: 5.0, max: 30.0 }",
            "    distribution: { kind: lognormal, mediann: 12.0, sigma_log: 0.35, min: 5.0, max: 30.0 }")),
        ("in outputs", base_yaml().replace("    dir: \"out/run1\"", "    dir: \"out/run1\"\n    recrod: \"r.json\"")),
    ] {
        let p = write(tmp.path(), "bad.yaml", &bad);
        let err = load_pack_document(&p)
            .err()
            .unwrap_or_else(|| panic!("{label}: an unknown key must be refused"))
            .to_string();
        assert!(err.contains("unknown field"), "{label}: {err}");
    }
}

// ---------------------------------------------------------------- path resolution

// Proving config-relative resolution by changing the working directory would be a
// flake generator: set_current_dir is process-global and cargo runs tests in
// threads. Two copies of the same config in two directories prove it instead,
// and neither depends on where the test process happens to be.
#[test]
fn relative_paths_resolve_against_the_config_not_the_working_directory() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let ra = load_resolved(a.path(), &base_yaml());
    let rb = load_resolved(b.path(), &base_yaml());

    assert_eq!(ra.shape_files[0], a.path().join("shapes/particles.stl"));
    assert_eq!(rb.shape_files[0], b.path().join("shapes/particles.stl"));
    assert_ne!(ra.shape_files[0], rb.shape_files[0]);
    assert_eq!(ra.outputs.dir, a.path().join("out/run1"));
    assert_eq!(ra.outputs.record, a.path().join("out/run1/particles.json"));

    // The path as written is kept beside the resolved one: the record reports the
    // file "as given", which is what a reader compares against their own config.
    assert_eq!(ra.shape_paths_as_written[0], "shapes/particles.stl");
}

#[test]
fn an_absolute_path_in_a_config_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let abs = tmp.path().join("elsewhere/lib.stl");
    let text = base_yaml().replace(
        "    files: [\"shapes/particles.stl\"]",
        &format!("    files: [\"{}\"]", abs.display()),
    );
    let resolved = load_resolved(tmp.path(), &text);
    assert_eq!(resolved.shape_files[0], abs);
}

// A config named without a directory has an empty parent, which would join onto a
// bare relative path and quietly bring the working directory back.
#[test]
fn a_bare_config_name_resolves_against_the_current_directory_explicitly() {
    assert_eq!(config_dir(Path::new("pack.yaml")), Path::new("."));
    assert_eq!(config_dir(Path::new("/a/b/pack.yaml")), Path::new("/a/b"));
    assert_eq!(
        resolve_against(Path::new("."), "shapes/a.stl"),
        Path::new("./shapes/a.stl")
    );
}

// ---------------------------------------------------------------- refusals

#[test]
fn a_void_neighbourhood_without_a_void_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace(
        "  target: { volume_fraction: 0.2 }",
        "  position: { mode: void_neighbourhood, band: [1.0, 5.0] }\n  target: { volume_fraction: 0.2 }",
    );
    let err = refusal(tmp.path(), &text);
    assert!(err.contains("void"), "{err}");
}

#[test]
fn a_solid_basis_without_a_void_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace(
        "  target: { volume_fraction: 0.2 }",
        "  target: { volume_fraction: 0.2, basis: solid }",
    );
    let err = refusal(tmp.path(), &text);
    assert!(err.contains("basis") && err.contains("void"), "{err}");
}

// A zero gap cannot forbid anything: parry reports distance 0.0 for two shapes
// that intersect, and 0.0 >= 0.0 passes.
#[test]
fn a_zero_gap_with_crossing_forbidden_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace(
        "  target:",
        "  void: { file: \"v.stl\", crossing: forbidden, gap: 0.0 }\n  target:",
    );
    let err = refusal(tmp.path(), &text);
    assert!(err.contains("gap") && err.contains("forbidden"), "{err}");
}

#[test]
fn crossing_allowed_without_a_voxel_size_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace(
        "  target:",
        "  void: { file: \"v.stl\", crossing: allowed, gap: 1.0 }\n  target:",
    );
    let err = refusal(tmp.path(), &text);
    assert!(err.contains("voxel_size"), "{err}");
}

#[test]
fn periodic_boundaries_with_a_void_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace(
        "  target:",
        "  void: { file: \"v.stl\", crossing: forbidden, gap: 2.0 }\n  boundary: { mode: periodic }\n  target:",
    );
    let err = refusal(tmp.path(), &text);
    assert!(err.contains("periodic") && err.contains("void"), "{err}");
}

#[test]
fn a_lognormal_with_min_at_or_above_max_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace("min: 5.0, max: 30.0", "min: 30.0, max: 30.0");
    let err = refusal(tmp.path(), &text);
    assert!(err.contains("min") && err.contains("max"), "{err}");
}

#[test]
fn a_distribution_parameter_from_the_wrong_kind_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace(
        "    distribution: { kind: lognormal, median: 12.0, sigma_log: 0.35, min: 5.0, max: 30.0 }",
        "    distribution: { kind: histogram, csv: \"h.csv\", median: 12.0 }",
    );
    let err = refusal(tmp.path(), &text);
    assert!(err.contains("median") && err.contains("lognormal"), "{err}");

    let text = base_yaml().replace(
        "    distribution: { kind: lognormal, median: 12.0, sigma_log: 0.35, min: 5.0, max: 30.0 }",
        "    distribution: { kind: lognormal, sigma_log: 0.35, min: 5.0, max: 30.0 }",
    );
    let err = refusal(tmp.path(), &text);
    assert!(err.contains("median") && err.contains("required"), "{err}");
}

#[test]
fn an_empty_shape_list_and_a_degenerate_domain_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let err = refusal(tmp.path(), &base_yaml().replace("[\"shapes/particles.stl\"]", "[]"));
    assert!(err.contains("at least one"), "{err}");

    let err = refusal(
        tmp.path(),
        &base_yaml().replace("max: [100, 100, 100]", "max: [100, 0, 100]"),
    );
    assert!(err.contains("extent") && err.contains('y'), "{err}");
}

#[test]
fn a_volume_fraction_outside_the_open_unit_interval_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    for bad in ["0.0", "1.0", "1.5", "-0.2"] {
        let text = base_yaml().replace("volume_fraction: 0.2", &format!("volume_fraction: {bad}"));
        let err = refusal(tmp.path(), &text);
        assert!(err.contains("volume_fraction"), "{bad}: {err}");
    }
}

#[test]
fn a_band_given_without_the_mode_that_reads_it_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace(
        "  target:",
        "  position: { mode: feasible_uniform, band: [1.0, 5.0] }\n  target:",
    );
    let err = refusal(tmp.path(), &text);
    assert!(err.contains("band") && err.contains("void_neighbourhood"), "{err}");
}

// ---------------------------------------------------------------- defaults

#[test]
fn the_defaults_are_the_ones_the_documentation_states() {
    let tmp = tempfile::tempdir().unwrap();
    let r = load_resolved(tmp.path(), &base_yaml());
    assert_eq!(r.orientation, OrientationMode::UniformSo3);
    assert_eq!(r.position.mode, PositionMode::FeasibleUniform);
    assert_eq!(r.boundary.mode, BoundaryMode::Strict);
    assert_eq!(r.on_unattainable, OnUnattainable::SkipReported);
    assert_eq!(r.placement_order, PlacementOrder::Descending);
    assert_eq!(r.target_tolerance, 0.01);
    assert_eq!(r.budget.attempts_per_particle, 2000);
    assert_eq!(r.budget.total_attempts, 2_000_000);
    assert_eq!(r.budget.max_top_up_batches, 5);
    assert_eq!(r.threads, -1);
    assert!(r.outputs.copy_void);
    assert!(!r.outputs.per_particle_stl);
    assert!(r.outputs.voxel_labels.is_none());
    assert!(matches!(r.classes, ResolvedClasses::EqualWidth { count: 10 }));
    // No void, so the basis defaults to the whole domain.
    assert_eq!(r.target_basis, TargetBasis::Domain);
    assert!(matches!(r.distribution, ResolvedDistribution::Lognormal { .. }));
}

// With a void present, a fraction is almost always meant of the solid, so that is
// the default; stating it explicitly must give the same answer.
#[test]
fn a_void_makes_the_target_basis_default_to_solid() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace(
        "  target:",
        "  void: { file: \"v.stl\", crossing: forbidden, gap: 2.0 }\n  target:",
    );
    let r = load_resolved(tmp.path(), &text);
    assert_eq!(r.target_basis, TargetBasis::Solid);
    let v = r.void.expect("void present");
    assert_eq!(v.crossing, VoidCrossing::Forbidden);
    assert_eq!(v.gap, 2.0);
    assert_eq!(v.file, tmp.path().join("v.stl"));
    assert!(v.overlap_voxel_size.is_none());
}

#[test]
fn a_histogram_distribution_resolves_its_csv_and_uses_its_own_bins() {
    let tmp = tempfile::tempdir().unwrap();
    let text = base_yaml().replace(
        "    distribution: { kind: lognormal, median: 12.0, sigma_log: 0.35, min: 5.0, max: 30.0 }",
        "    distribution: { kind: histogram, csv: \"dist/sizes.csv\" }",
    );
    let r = load_resolved(tmp.path(), &text);
    match r.distribution {
        ResolvedDistribution::Histogram { csv, path_as_written } => {
            assert_eq!(csv, tmp.path().join("dist/sizes.csv"));
            assert_eq!(path_as_written, "dist/sizes.csv");
        }
        _ => panic!("expected a histogram"),
    }
    assert!(matches!(r.classes, ResolvedClasses::FromHistogram));
}

// ---------------------------------------------------------------- legacy seeding

// `split-filter`'s lognormal rebalance shuffles before thinning over-represented
// bins. Absent seed keeps the previous unseeded generator, so existing configs are
// untouched; a seed makes the kept set repeatable.
#[test]
fn a_seeded_split_filter_keeps_the_same_set_twice() {
    use rustmspt::config::{
        InputPath, SplitFilterConfig, SplitFilterOutput, SplitFilterRules, SplitFilterVolume,
    };
    use rustmspt::geometry::{icosphere_mesh, merge_meshes};
    use rustmspt::io::save_stl;
    use rustmspt::pipeline::split_filter::SplitFilterPipeline;
    use rustmspt::pipeline::Pipeline;
    use rustmspt::types::Vec3;

    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("many.stl");
    // A spread of sizes, so the rebalance has over-represented bins to thin.
    let spheres: Vec<_> = (0..40)
        .map(|i| {
            let r = 1.0 + (i % 7) as f64 * 0.35;
            icosphere_mesh(Vec3::new(i as f64 * 12.0, 0.0, 0.0), r, 1)
        })
        .collect();
    save_stl(&input, &merge_meshes(&spheres), "many").expect("write library");

    let run = |dir: &Path, seed: Option<u64>, workers: i32| -> Vec<(String, Vec<u8>)> {
        let out = dir.to_path_buf();
        SplitFilterPipeline {
            config: SplitFilterConfig {
                cpu_max: Some(workers),
                input: InputPath {
                    path: input.to_string_lossy().to_string(),
                },
                output: SplitFilterOutput {
                    folder: out.to_string_lossy().to_string(),
                    prefix: "p_".to_string(),
                    report_path: None,
                },
                filter: Some(SplitFilterRules {
                    enabled: Some(true),
                    max_aspect_ratio: Some(3.0),
                    max_sharpness_ratio: Some(4.0),
                    volume: Some(SplitFilterVolume {
                        mode: Some("lognormal_rebalance".to_string()),
                        min: None,
                        max: None,
                        bins: Some(5),
                        over_factor: Some(1.0),
                    }),
                }),
                seed,
            },
        }
        .run()
        .expect("split-filter runs");
        let mut names: Vec<String> = fs::read_dir(&out)
            .expect("output folder")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names.into_iter().map(|name| {
            let bytes = fs::read(out.join(&name)).unwrap();
            (name, bytes)
        }).collect()
    };

    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = run(a.path(), Some(4242), 1);
    let second = run(b.path(), Some(4242), 8);
    assert!(!first.is_empty() && first.len() < 40, "rebalance must retain and remove particles");
    assert_eq!(first, second, "the same seed must keep the same set");
}
