use rustmspt::error::RustMsptError;
use rustmspt::geometry::MeshMetrics;
use rustmspt::pipeline::pack_targets::{
    load_target_distribution_csv, write_distribution_comparison_csv, BinChoiceKind,
    SphericityState, TargetDistribution,
};
use std::collections::BTreeSet;
use std::io::Write;

fn write_distribution(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let path = dir.path().join("distribution.csv");
    let mut file = std::fs::File::create(&path).expect("CSV fixture should be created");
    file.write_all(content.as_bytes())
        .expect("CSV fixture should be written");
    (dir, path)
}

#[test]
fn parses_explicit_and_inferred_bin_rights() {
    let (_dir, path) = write_distribution("bin,right,frequency\n5,6,0.25\n6,,0.50\n7,,0.25\n");
    let distribution = load_target_distribution_csv(&path).expect("CSV should parse");
    assert_eq!(distribution.bins.len(), 3);
    assert_eq!(distribution.bins[0].left, 5.0);
    assert_eq!(distribution.bins[0].right, 6.0);
    assert_eq!(distribution.bins[1].right, 7.0);
    assert_eq!(distribution.bins[2].right, 8.0);
    let sum: f64 = distribution.bins.iter().map(|bin| bin.frequency).sum();
    assert!((sum - 1.0).abs() < 1e-12);
}

#[test]
fn parses_two_column_distribution() {
    let (_dir, path) = write_distribution("bin,frequency\n5,0.2\n6,0.3\n7,0.5\n");
    let distribution = load_target_distribution_csv(&path).expect("CSV should parse");
    assert_eq!(distribution.bins[0].right, 6.0);
    assert_eq!(distribution.bins[1].right, 7.0);
    assert_eq!(distribution.bins[2].right, 8.0);
}

#[test]
fn classifies_shared_edges_and_final_right() {
    let distribution = TargetDistribution {
        bins: vec![
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 1.0,
                right: 2.0,
                frequency: 0.5,
            },
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 2.0,
                right: 4.0,
                frequency: 0.5,
            },
        ],
    };
    assert_eq!(distribution.bin_for_diameter(1.0), Some(0));
    assert_eq!(distribution.bin_for_diameter(1.999999), Some(0));
    assert_eq!(distribution.bin_for_diameter(2.0), Some(1));
    assert_eq!(distribution.bin_for_diameter(4.0), Some(1));
    assert_eq!(distribution.bin_for_diameter(4.000001), None);
}

#[test]
fn rejects_invalid_frequency_sum() {
    let (_dir, path) = write_distribution("bin,right,frequency\n1,2,0.2\n2,3,0.2\n");
    let error = load_target_distribution_csv(&path).expect_err("CSV should fail");
    assert!(matches!(error, RustMsptError::InvalidConfig(_)));
}

#[test]
fn rejects_uninferable_single_row() {
    let (_dir, path) = write_distribution("bin,frequency\n5,1.0\n");
    let error = load_target_distribution_csv(&path).expect_err("CSV should fail");
    assert!(matches!(error, RustMsptError::InvalidConfig(_)));
}

#[test]
fn rejects_overlapping_explicit_intervals() {
    let (_dir, path) = write_distribution("bin,right,frequency\n1,3,0.5\n2,4,0.5\n");
    let error = load_target_distribution_csv(&path).expect_err("CSV should fail");
    assert!(matches!(error, RustMsptError::InvalidConfig(_)));
}

#[test]
fn finite_large_edges_produce_finite_midpoint() {
    let (_dir, path) = write_distribution("bin,right,frequency\n1e308,1.7e308,1.0\n");
    let distribution = load_target_distribution_csv(&path).expect("finite interval should parse");
    assert!(distribution.bins[0].midpoint().is_finite());
}

#[test]
fn rejects_interval_with_zero_underflowed_midpoint() {
    let (_dir, path) = write_distribution("bin,right,frequency\n0,5e-324,1.0\n");
    let error = load_target_distribution_csv(&path).expect_err("zero midpoint should fail");
    assert!(matches!(error, RustMsptError::InvalidConfig(_)));
}

#[test]
fn integer_rounding_baseline_preserves_total_count() {
    let distribution = TargetDistribution {
        bins: vec![
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 1.0,
                right: 2.0,
                frequency: 1.0 / 3.0,
            },
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 2.0,
                right: 3.0,
                frequency: 1.0 / 3.0,
            },
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 3.0,
                right: 4.0,
                frequency: 1.0 / 3.0,
            },
        ],
    };
    let mut state = distribution.state();
    state.counts = vec![1, 1, 0];
    let summary = distribution.summarize(&state);
    assert!((summary.rounding_max_absolute_error - 1.0 / 3.0).abs() < 1e-12);
}

#[test]
fn writes_target_actual_comparison_csv_beside_output_stl() {
    let distribution = TargetDistribution {
        bins: vec![
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 1.0,
                right: 2.0,
                frequency: 0.25,
            },
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 2.0,
                right: 3.0,
                frequency: 0.75,
            },
        ],
    };
    let mut state = distribution.state();
    state.counts = vec![1, 3];
    state.attempts = vec![2, 4];
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let output_stl = dir.path().join("packed.stl");
    let comparison = write_distribution_comparison_csv(&output_stl, &distribution, &state)
        .expect("comparison CSV should be written");
    assert_eq!(
        comparison.file_name().and_then(|name| name.to_str()),
        Some("packed_diameter_distribution.csv")
    );

    let mut reader = csv::Reader::from_path(comparison).expect("comparison CSV should parse");
    assert_eq!(
        reader.headers().expect("headers should exist"),
        &csv::StringRecord::from(vec![
            "bin",
            "right",
            "target_frequency",
            "target_count",
            "actual_count",
            "actual_frequency",
            "frequency_error",
            "count_error",
            "attempts",
        ])
    );
    let records: Vec<csv::StringRecord> = reader
        .records()
        .collect::<std::result::Result<_, _>>()
        .expect("records should parse");
    assert_eq!(records.len(), 2);
    assert_eq!(&records[0][2], "0.250000000000");
    assert_eq!(&records[0][4], "1");
    assert_eq!(&records[0][5], "0.250000000000");
    assert_eq!(&records[1][8], "4");
}

#[test]
fn example_paper_distribution_is_valid_and_normalized() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data/input/gu2019_fig7b_pore_distribution.csv");
    let distribution = load_target_distribution_csv(&path).expect("example CSV should parse");
    assert_eq!(distribution.bins.len(), 25);
    assert_eq!(distribution.bins[0].left, 5.0);
    assert_eq!(distribution.bins[0].right, 6.0);
    assert_eq!(distribution.bins[24].right, 30.0);
    let sum: f64 = distribution.bins.iter().map(|bin| bin.frequency).sum();
    assert!((sum - 1.0).abs() < 1e-12);
}

#[test]
fn strict_choice_follows_largest_deficit() {
    let distribution = TargetDistribution {
        bins: vec![
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 1.0,
                right: 2.0,
                frequency: 0.5,
            },
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 2.0,
                right: 3.0,
                frequency: 0.3,
            },
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 3.0,
                right: 4.0,
                frequency: 0.2,
            },
        ],
    };
    let mut state = distribution.state();
    let expected = [0, 1, 2, 0, 0, 1, 0, 2, 1, 0];
    for expected_index in expected {
        let mut attempted = BTreeSet::new();
        let choice = distribution
            .choose_bin(&state, None, &mut attempted)
            .expect("a positive-frequency bin should be selected");
        assert_eq!(choice.index, expected_index);
        assert_eq!(choice.kind, BinChoiceKind::Scaled);
        distribution.record_attempt(&mut state, choice.index);
        distribution.record_success(&mut state, choice.index, choice.kind, Some(1.0));
    }
    assert_eq!(state.counts, vec![5, 3, 2]);
    let summary = distribution.summarize(&state);
    assert!(summary.max_absolute_error <= summary.rounding_max_absolute_error + 1e-12);
}

#[test]
fn natural_choice_is_preserved_for_deficit_bin() {
    let distribution = TargetDistribution {
        bins: vec![rustmspt::pipeline::pack_targets::DiameterBin {
            left: 1.0,
            right: 2.0,
            frequency: 1.0,
        }],
    };
    let state = distribution.state();
    let mut attempted = BTreeSet::new();
    let choice = distribution
        .choose_bin(&state, Some(0), &mut attempted)
        .expect("natural bin should be selected");
    assert_eq!(choice.kind, BinChoiceKind::Natural);
}

#[test]
fn attempted_deficit_bin_falls_back_to_other_bin() {
    let distribution = TargetDistribution {
        bins: vec![
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 10.0,
                right: 20.0,
                frequency: 0.9,
            },
            rustmspt::pipeline::pack_targets::DiameterBin {
                left: 1.0,
                right: 2.0,
                frequency: 0.1,
            },
        ],
    };
    let state = distribution.state();
    let mut attempted = BTreeSet::from([0]);
    let choice = distribution
        .choose_bin(&state, None, &mut attempted)
        .expect("fallback bin should be selected");
    assert_eq!(choice.index, 1);
    assert_eq!(choice.kind, BinChoiceKind::Fallback);
}

#[test]
fn sphericity_projection_uses_tolerance_band() {
    let metrics = MeshMetrics {
        volume: 1.0,
        surface_area: 6.0,
        equivalent_diameter: 1.0,
        sphericity: 0.7,
    };
    let state = SphericityState { sum: 0.9, count: 1 };
    let error = state
        .projected_error(metrics, 0.8, Some(0.05))
        .expect("projection should be finite");
    assert_eq!(error, 0.0);
}

#[test]
fn sphericity_projection_selects_direction_that_repairs_mean() {
    let low = MeshMetrics {
        volume: 1.0,
        surface_area: 6.0,
        equivalent_diameter: 1.0,
        sphericity: 0.6,
    };
    let high = MeshMetrics {
        sphericity: 0.9,
        ..low
    };
    let state = SphericityState { sum: 0.6, count: 1 };
    let low_error = state
        .projected_error(low, 0.8, None)
        .expect("projection should be finite");
    let high_error = state
        .projected_error(high, 0.8, None)
        .expect("projection should be finite");
    assert!((high_error - 0.05).abs() < 1e-12);
    assert!(low_error > high_error);
}
