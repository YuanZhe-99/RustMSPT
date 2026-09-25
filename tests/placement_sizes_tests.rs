use rustmspt::config::placement::{ResolvedClasses, ResolvedDistribution};
use rustmspt::geometry::{icosphere_mesh, merge_meshes};
use rustmspt::io::save_stl;
use rustmspt::pipeline::placement_library::load_shape_library;
use rustmspt::pipeline::placement_sizes::{
    build_classes, class_for_diameter, inverse_normal_cdf, order_for_placement, plan_size_multiset,
    SizeSource,
};
use rustmspt::pipeline::rng::seeded_rng;
use rustmspt::types::Vec3;
use std::path::PathBuf;

fn lognormal(median: f64, sigma_log: f64, min: f64, max: f64) -> ResolvedDistribution {
    ResolvedDistribution::Lognormal {
        median,
        sigma_log,
        min,
        max,
    }
}

// ---------------------------------------------------------------- normal quantile

// Checked against published values rather than against our own inverse of our own
// CDF, which would agree with itself no matter how wrong both were.
#[test]
fn the_normal_quantile_matches_published_values() {
    // (p, quantile) to 12 decimal places.
    let table: [(f64, f64); 13] = [
        (0.5, 0.0),
        (0.75, 0.674489750196),
        (0.9, 1.281551565545),
        (0.95, 1.644853626951),
        (0.975, 1.959963984540),
        (0.99, 2.326347874041),
        (0.995, 2.575829303549),
        (0.999, 3.090232306168),
        (0.9999, 3.719016485455),
        (0.25, -0.674489750196),
        (0.1, -1.281551565545),
        (0.025, -1.959963984540),
        (0.001, -3.090232306168),
    ];
    for (p, want) in table {
        let got = inverse_normal_cdf(p);
        assert!(
            (got - want).abs() < 1e-9,
            "Phi^-1({p}) = {got}, published {want}"
        );
    }
    assert!(inverse_normal_cdf(0.0).is_infinite() && inverse_normal_cdf(0.0) < 0.0);
    assert!(inverse_normal_cdf(1.0).is_infinite() && inverse_normal_cdf(1.0) > 0.0);
    assert!(inverse_normal_cdf(-0.1).is_nan());
    assert!(inverse_normal_cdf(1.1).is_nan());
    // Symmetry, which the two-branch implementation could easily break.
    for p in [0.01, 0.2, 0.33, 0.47] {
        assert!((inverse_normal_cdf(p) + inverse_normal_cdf(1.0 - p)).abs() < 1e-12);
    }
}

// ---------------------------------------------------------------- lognormal sampler

#[test]
fn every_lognormal_draw_lies_inside_the_truncation_bounds() {
    let source = SizeSource::prepare(&lognormal(12.0, 0.35, 5.0, 30.0)).unwrap();
    let mut rng = seeded_rng(1);
    for _ in 0..200_000 {
        let d = source.sample(&mut rng);
        assert!((5.0..=30.0).contains(&d), "draw outside bounds: {d}");
    }
}

// With bounds wide enough that truncation barely bites, the sample must reproduce
// the parameters it was given: median back to the median, and the spread of log d
// back to sigma_log.
#[test]
fn a_widely_bounded_lognormal_reproduces_its_median_and_spread() {
    let source = SizeSource::prepare(&lognormal(12.0, 0.35, 0.5, 300.0)).unwrap();
    let mut rng = seeded_rng(20260910);
    let n = 200_000;
    let mut draws: Vec<f64> = (0..n).map(|_| source.sample(&mut rng)).collect();
    draws.sort_by(|a, b| a.total_cmp(b));

    let median = draws[n / 2];
    assert!(
        (median / 12.0 - 1.0).abs() < 0.01,
        "sample median {median}, expected about 12.0"
    );

    let logs: Vec<f64> = draws.iter().map(|d| d.ln()).collect();
    let mean = logs.iter().sum::<f64>() / n as f64;
    let var = logs.iter().map(|l| (l - mean) * (l - mean)).sum::<f64>() / (n as f64 - 1.0);
    let sd = var.sqrt();
    assert!(
        (sd / 0.35 - 1.0).abs() < 0.02,
        "sd(ln d) = {sd}, expected about 0.35"
    );
    assert!(
        (mean - 12.0f64.ln()).abs() < 0.01,
        "mean(ln d) = {mean}, expected ln(12) = {}",
        12.0f64.ln()
    );
}

// Truncation must renormalize: the share landing in any sub-interval has to match
// the truncated distribution, not the untruncated one.
#[test]
fn truncation_renormalizes_rather_than_piling_mass_at_the_bounds() {
    let source = SizeSource::prepare(&lognormal(12.0, 0.35, 8.0, 18.0)).unwrap();
    let mut rng = seeded_rng(7);
    let n = 100_000;
    let draws: Vec<f64> = (0..n).map(|_| source.sample(&mut rng)).collect();

    for (lo, hi) in [(8.0, 10.0), (10.0, 12.0), (12.0, 15.0), (15.0, 18.0)] {
        let observed =
            draws.iter().filter(|d| **d >= lo && **d < hi).count() as f64 / n as f64;
        let expected = source.mass_between(lo, hi);
        assert!(
            (observed - expected).abs() < 0.01,
            "[{lo}, {hi}): observed {observed:.4}, expected {expected:.4}"
        );
    }
    // No pile-up at the edges: a rejection-free inverse CDF must not clamp mass there.
    let at_lo = draws.iter().filter(|d| **d < 8.0 + 1e-9).count();
    let at_hi = draws.iter().filter(|d| **d > 18.0 - 1e-9).count();
    assert!(at_lo < n / 500 && at_hi < n / 500, "mass piled at the bounds: {at_lo}, {at_hi}");
    // The class shares must sum to one, which is what "renormalized" means.
    let total = source.mass_between(8.0, 18.0);
    assert!((total - 1.0).abs() < 1e-9, "truncated mass is {total}");
}

// ---------------------------------------------------------------- histogram sampler

fn write_histogram(dir: &std::path::Path) -> PathBuf {
    let p = dir.join("sizes.csv");
    std::fs::write(
        &p,
        "bin,right,frequency\n5,10,0.5\n10,15,0.3\n15,20,0.2\n",
    )
    .unwrap();
    p
}

#[test]
fn histogram_draws_follow_the_bin_frequencies_and_stay_inside_their_bin() {
    let tmp = tempfile::tempdir().unwrap();
    let csv = write_histogram(tmp.path());
    let source = SizeSource::prepare(&ResolvedDistribution::Histogram {
        csv: csv.clone(),
        path_as_written: "sizes.csv".to_string(),
    })
    .unwrap();

    let mut rng = seeded_rng(99);
    let n = 200_000;
    let mut counts = [0usize; 3];
    for _ in 0..n {
        let d = source.sample(&mut rng);
        assert!((5.0..=20.0).contains(&d), "draw outside the support: {d}");
        let idx = if d < 10.0 {
            0
        } else if d < 15.0 {
            1
        } else {
            2
        };
        counts[idx] += 1;
    }
    for (idx, want) in [0.5, 0.3, 0.2].iter().enumerate() {
        let observed = counts[idx] as f64 / n as f64;
        assert!(
            (observed - want).abs() < 0.005,
            "bin {idx}: observed {observed:.4}, expected {want}"
        );
    }

    // Uniform inside a bin, not concentrated at its midpoint: split the first bin
    // in half and both halves must be equally likely.
    let mut rng = seeded_rng(100);
    let (mut low, mut high) = (0usize, 0usize);
    for _ in 0..100_000 {
        let d = source.sample(&mut rng);
        if d < 10.0 {
            if d < 7.5 {
                low += 1;
            } else {
                high += 1;
            }
        }
    }
    let ratio = low as f64 / (low + high) as f64;
    assert!(
        (ratio - 0.5).abs() < 0.02,
        "sub-bin law is not uniform: {ratio:.4} in the lower half"
    );
}

// ---------------------------------------------------------------- classes

#[test]
fn equal_width_classes_span_the_support_and_their_shares_sum_to_one() {
    let source = SizeSource::prepare(&lognormal(12.0, 0.35, 5.0, 30.0)).unwrap();
    let classes = build_classes(&ResolvedClasses::EqualWidth { count: 5 }, &source);
    assert_eq!(classes.len(), 5);
    assert!((classes[0].lo - 5.0).abs() < 1e-12);
    assert!((classes[4].hi - 30.0).abs() < 1e-12);
    for w in classes.windows(2) {
        assert!((w[0].hi - w[1].lo).abs() < 1e-12, "classes must abut");
    }
    let total: f64 = classes.iter().map(|c| c.target_frequency).sum();
    assert!((total - 1.0).abs() < 1e-9, "class shares sum to {total}");
}

#[test]
fn a_histogram_reports_over_its_own_bins() {
    let tmp = tempfile::tempdir().unwrap();
    let csv = write_histogram(tmp.path());
    let source = SizeSource::prepare(&ResolvedDistribution::Histogram {
        csv,
        path_as_written: "sizes.csv".to_string(),
    })
    .unwrap();
    let classes = build_classes(&ResolvedClasses::FromHistogram, &source);
    assert_eq!(classes.len(), 3);
    assert_eq!((classes[0].lo, classes[0].hi), (5.0, 10.0));
    assert!((classes[0].target_frequency - 0.5).abs() < 1e-12);
    assert!((classes[2].target_frequency - 0.2).abs() < 1e-12);
}

// The last class's upper edge is inclusive, matching how the histogram bins already
// behave, so a diameter exactly at the maximum still lands somewhere.
#[test]
fn class_lookup_is_half_open_except_at_the_very_top() {
    let source = SizeSource::prepare(&lognormal(12.0, 0.35, 5.0, 30.0)).unwrap();
    let classes = build_classes(&ResolvedClasses::EqualWidth { count: 5 }, &source);
    assert_eq!(class_for_diameter(&classes, 5.0), 0);
    assert_eq!(class_for_diameter(&classes, 9.999), 0);
    assert_eq!(class_for_diameter(&classes, 10.0), 1);
    assert_eq!(class_for_diameter(&classes, 30.0), 4, "the top edge is inclusive");
    assert_eq!(class_for_diameter(&classes, 1.0), 0, "below the support clamps low");
    assert_eq!(class_for_diameter(&classes, 99.0), 4, "above the support clamps high");
}

// ---------------------------------------------------------------- multiset plan

// The plan keeps whichever of the last two counts lands closer to the target.
// Stopping at the first count to exceed it would overshoot every time, and the
// particle it added would be the largest one - the one most likely to fail.
#[test]
fn the_plan_lands_on_whichever_side_of_the_target_is_closer() {
    let tmp = tempfile::tempdir().unwrap();
    let csv = tmp.path().join("one.csv");
    // A single bin of width zero in effect: every draw is 10.0, so the arithmetic
    // is checkable by hand. One sphere of diameter 10 has volume pi*1000/6.
    std::fs::write(&csv, "bin,right,frequency\n10,10.0000001,1.0\n").unwrap();
    let source = SizeSource::prepare(&ResolvedDistribution::Histogram {
        csv,
        path_as_written: "one.csv".to_string(),
    })
    .unwrap();
    let classes = build_classes(&ResolvedClasses::FromHistogram, &source);
    let one = std::f64::consts::PI * 1000.0 / 6.0;

    // Target 2.4 particles: 2 is 0.4 short, 3 is 0.6 over, so 2 wins.
    let mut rng = seeded_rng(3);
    let plan = plan_size_multiset(&mut rng, &source, &classes, one * 2.4, 1000).unwrap();
    assert_eq!(plan.draws.len(), 2, "2.4 particles rounds down to 2");

    // Target 2.6: 3 is 0.4 over, 2 is 0.6 short, so 3 wins.
    let mut rng = seeded_rng(3);
    let plan = plan_size_multiset(&mut rng, &source, &classes, one * 2.6, 1000).unwrap();
    assert_eq!(plan.draws.len(), 3, "2.6 particles rounds up to 3");

    // Exactly on a whole count: no overshoot at all.
    let mut rng = seeded_rng(3);
    let plan = plan_size_multiset(&mut rng, &source, &classes, one * 3.0, 1000).unwrap();
    assert_eq!(plan.draws.len(), 3);
    assert!(plan.planned_volume_error.abs() < 1e-6 * one);

    // A target smaller than one particle still plans one: zero particles is not a
    // packing, and the shortfall is what the report is for.
    let mut rng = seeded_rng(3);
    let plan = plan_size_multiset(&mut rng, &source, &classes, one * 0.1, 1000).unwrap();
    assert_eq!(plan.draws.len(), 1);
    assert!(plan.planned_volume_error > 0.0);
}

#[test]
fn the_plan_is_reproducible_and_records_what_it_planned() {
    let source = SizeSource::prepare(&lognormal(12.0, 0.35, 5.0, 30.0)).unwrap();
    let classes = build_classes(&ResolvedClasses::EqualWidth { count: 5 }, &source);
    let target = 100.0 * 100.0 * 100.0 * 0.2;

    let mut a = seeded_rng(4242);
    let mut b = seeded_rng(4242);
    let pa = plan_size_multiset(&mut a, &source, &classes, target, 1_000_000).unwrap();
    let pb = plan_size_multiset(&mut b, &source, &classes, target, 1_000_000).unwrap();
    assert_eq!(pa.draws.len(), pb.draws.len());
    for (x, y) in pa.draws.iter().zip(pb.draws.iter()) {
        assert_eq!(x.diameter.to_bits(), y.diameter.to_bits());
        assert_eq!(x.class, y.class);
    }
    assert_eq!(pa.target_volume, target);
    assert!((pa.planned_volume - target).abs() / target < 0.02);
    assert!((pa.planned_volume_error - (pa.planned_volume - target)).abs() < 1e-9);

    let mut c = seeded_rng(4243);
    let pc = plan_size_multiset(&mut c, &source, &classes, target, 1_000_000).unwrap();
    assert!(
        pc.draws[0].diameter != pa.draws[0].diameter,
        "a different seed must plan differently"
    );
}

#[test]
fn an_unreachable_target_is_refused_rather_than_looping() {
    let source = SizeSource::prepare(&lognormal(1.0, 0.1, 0.9, 1.1)).unwrap();
    let classes = build_classes(&ResolvedClasses::EqualWidth { count: 3 }, &source);
    let mut rng = seeded_rng(1);
    let err = plan_size_multiset(&mut rng, &source, &classes, 1e9, 1000)
        .unwrap_err()
        .to_string();
    assert!(err.contains("1000"), "{err}");
}

#[test]
fn descending_order_is_total_and_drawn_order_is_recoverable() {
    let source = SizeSource::prepare(&lognormal(12.0, 0.35, 5.0, 30.0)).unwrap();
    let classes = build_classes(&ResolvedClasses::EqualWidth { count: 5 }, &source);
    let mut rng = seeded_rng(11);
    let mut plan = plan_size_multiset(&mut rng, &source, &classes, 50_000.0, 100_000).unwrap();
    let drawn: Vec<f64> = plan.draws.iter().map(|d| d.diameter).collect();

    order_for_placement(&mut plan.draws, true);
    for w in plan.draws.windows(2) {
        assert!(
            w[0].diameter > w[1].diameter
                || (w[0].diameter == w[1].diameter && w[0].draw_index < w[1].draw_index),
            "descending order must be total"
        );
    }

    order_for_placement(&mut plan.draws, false);
    let back: Vec<f64> = plan.draws.iter().map(|d| d.diameter).collect();
    assert_eq!(drawn, back, "drawn order is recoverable from the draw index");
}

// ---------------------------------------------------------------- shape library

fn write_library(dir: &std::path::Path, name: &str, meshes: &[rustmspt::types::Mesh]) -> PathBuf {
    let p = dir.join(name);
    save_stl(&p, &merge_meshes(meshes), "lib").unwrap();
    p
}

#[test]
fn shells_are_found_in_first_face_order_and_measured() {
    let tmp = tempfile::tempdir().unwrap();
    let small = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 2);
    let large = icosphere_mesh(Vec3::new(50.0, 0.0, 0.0), 3.0, 2);
    let path = write_library(tmp.path(), "two.stl", &[small, large]);

    let lib = load_shape_library(
        std::slice::from_ref(&path),
        &["two.stl".to_string()],
        None,
    )
    .expect("library loads");

    assert_eq!(lib.shells.len(), 2);
    assert_eq!(lib.sources.len(), 1);
    assert_eq!(lib.sources[0].shells_found, 2);
    assert_eq!(lib.sources[0].shells_kept, 2);
    assert_eq!(lib.sources[0].path_as_written, "two.stl");

    // First-face order: the small sphere was merged first, so it is shell 0.
    assert_eq!(lib.shells[0].shell_index, 0);
    assert!(lib.shells[0].equivalent_diameter < lib.shells[1].equivalent_diameter);
    assert!((lib.shells[1].equivalent_diameter / lib.shells[0].equivalent_diameter - 3.0).abs() < 0.01);

    for shell in &lib.shells {
        // The canonical copy is centred, which is what makes the placement
        // transform a plain scale-rotate-translate with no pivot bookkeeping.
        let c = rustmspt::geometry::mesh_volume_centroid(&shell.canonical).unwrap();
        assert!(c.dot(c).sqrt() < 1e-9, "canonical copy is not centred: {c:?}");
        assert!(shell.bounding_radius > 0.0);
        // A sphere's furthest vertex is essentially its radius, and its equivalent
        // diameter is essentially twice that, so the ratio is close to 1.
        let ratio = shell.bounding_radius / (shell.equivalent_diameter * 0.5);
        assert!((ratio - 1.0).abs() < 0.03, "sphere extent ratio {ratio}");
        assert_eq!(shell.shell_sha256.len(), 64);
    }
    assert!(lib.shells[0].centroid.dot(lib.shells[0].centroid).sqrt() < 1e-9);
    assert!((lib.shells[1].centroid.x - 50.0).abs() < 1e-6);
    assert!(lib.rejected.is_empty());
    assert!((lib.max_extent_ratio - 1.0).abs() < 0.03);
}

// An open shell has no volume, centroid or equivalent diameter, so dropping it
// would silently change the distribution the run drew from.
#[test]
fn an_open_shell_refuses_the_run_and_says_why() {
    let tmp = tempfile::tempdir().unwrap();
    let mut sphere = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1);
    sphere.faces.pop();
    let path = tmp.path().join("open.stl");
    save_stl(&path, &sphere, "open").unwrap();

    let err = load_shape_library(std::slice::from_ref(&path), &["open.stl".to_string()], None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("open") || err.contains("boundary"), "{err}");
    assert!(err.contains("shell 0"), "the message must locate the shell: {err}");
}

#[test]
fn an_inward_shell_is_refused_with_its_signed_volume() {
    let tmp = tempfile::tempdir().unwrap();
    let mut sphere = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1);
    for f in &mut sphere.faces {
        std::mem::swap(&mut f.b, &mut f.c);
    }
    let path = tmp.path().join("inward.stl");
    save_stl(&path, &sphere, "inward").unwrap();

    let err = load_shape_library(std::slice::from_ref(&path), &["inward.stl".to_string()], None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("inward"), "{err}");
}

// A filter rejection is different from a refusal: aspect and sharpness ratios are
// scale-invariant, so a shell that fails one fails at every size, and the run can
// carry on with the rest of the library as long as something is left.
#[test]
fn a_filtered_shell_is_recorded_as_rejected_not_refused() {
    use rustmspt::config::placement::ShapeFilters;
    let tmp = tempfile::tempdir().unwrap();
    let round = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 2.0, 2);
    // A very elongated ellipsoid: scale one axis of a sphere.
    let mut long = icosphere_mesh(Vec3::new(60.0, 0.0, 0.0), 2.0, 2);
    for v in &mut long.vertices {
        v.x = 60.0 + (v.x - 60.0) * 6.0;
    }
    let path = write_library(tmp.path(), "mixed.stl", &[round, long]);

    let filters = ShapeFilters {
        max_aspect_ratio: Some(3.0),
        max_sharpness_ratio: None,
    };
    let lib = load_shape_library(
        std::slice::from_ref(&path),
        &["mixed.stl".to_string()],
        Some(&filters),
    )
    .expect("library loads with one shell left");
    assert_eq!(lib.shells.len(), 1, "the elongated shell is filtered out");
    assert_eq!(lib.rejected.len(), 1);
    assert_eq!(lib.rejected[0].shell_index, 1);
    assert!(lib.rejected[0].reason.contains("aspect ratio"), "{}", lib.rejected[0].reason);
    assert_eq!(lib.sources[0].shells_found, 2);
    assert_eq!(lib.sources[0].shells_kept, 1);

    // Filtering everything away is a refusal, and it lists why. A bounding-box
    // aspect ratio is at least 1 by construction, so a limit below 1 rejects
    // everything, sphere included.
    let strict = ShapeFilters {
        max_aspect_ratio: Some(0.5),
        max_sharpness_ratio: None,
    };
    let err = load_shape_library(
        std::slice::from_ref(&path),
        &["mixed.stl".to_string()],
        Some(&strict),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("empty after filtering"), "{err}");
    assert!(err.contains("aspect ratio"), "the refusal lists the rejections: {err}");
}

// A shell ordinal says how a shape was found, not which shape it is. The geometry
// digest is what turns a silently re-ordered file into a detectable change.
#[test]
fn the_shell_digest_follows_the_geometry_not_the_ordinal() {
    let tmp = tempfile::tempdir().unwrap();
    let a = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1);
    let b = icosphere_mesh(Vec3::new(40.0, 0.0, 0.0), 2.0, 1);

    let ab = write_library(tmp.path(), "ab.stl", &[a.clone(), b.clone()]);
    let ba = write_library(tmp.path(), "ba.stl", &[b, a]);

    let lib_ab = load_shape_library(std::slice::from_ref(&ab), &["ab.stl".into()], None).unwrap();
    let lib_ba = load_shape_library(std::slice::from_ref(&ba), &["ba.stl".into()], None).unwrap();

    // Swapping the order swaps the ordinals, and the digests follow the shapes.
    assert_eq!(lib_ab.shells[0].shell_index, 0);
    assert_eq!(lib_ba.shells[0].shell_index, 0);
    assert_ne!(
        lib_ab.shells[0].shell_sha256, lib_ba.shells[0].shell_sha256,
        "ordinal 0 now names a different shape, and the digest says so"
    );
    // The small sphere is ordinal 0 in one file and ordinal 1 in the other, but it
    // is the same geometry, so it keeps its digest.
    let small_ab = &lib_ab.shells[0];
    let small_ba = &lib_ba.shells[1];
    assert!((small_ab.volume - small_ba.volume).abs() < 1e-9);
    assert_eq!(
        small_ab.shell_sha256, small_ba.shell_sha256,
        "the same geometry keeps its digest whatever ordinal it lands on"
    );
}

#[test]
fn list_order_across_files_fixes_the_source_index() {
    let tmp = tempfile::tempdir().unwrap();
    let one = write_library(
        tmp.path(),
        "one.stl",
        &[icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1)],
    );
    let two = write_library(
        tmp.path(),
        "two.stl",
        &[icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 2.0, 1)],
    );

    let lib = load_shape_library(
        &[one.clone(), two.clone()],
        &["one.stl".into(), "two.stl".into()],
        None,
    )
    .unwrap();
    assert_eq!(lib.shells[0].source_index, 0);
    assert_eq!(lib.shells[1].source_index, 1);

    let swapped = load_shape_library(&[two, one], &["two.stl".into(), "one.stl".into()], None).unwrap();
    assert_eq!(swapped.shells[0].source_index, 0);
    assert!(
        swapped.shells[0].equivalent_diameter > swapped.shells[1].equivalent_diameter,
        "source index follows the list, not the disk"
    );
}

#[test]
fn a_missing_file_is_refused_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("absent.stl");
    let err = load_shape_library(
        std::slice::from_ref(&missing),
        &["absent.stl".to_string()],
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("absent.stl"), "{err}");
    assert!(err.contains("shapes.files[0]"), "the message locates the entry: {err}");
}

#[test]
fn the_source_digest_matches_the_file_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_library(
        tmp.path(),
        "one.stl",
        &[icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1)],
    );
    let lib =
        load_shape_library(std::slice::from_ref(&path), &["one.stl".into()], None).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(lib.sources[0].sha256, rustmspt::io::sha256_bytes(&bytes));
    assert_eq!(lib.sources[0].bytes, bytes.len() as u64);
}

// AI-FUNC-SUMMARY: A 40-shell library takes the parallel preparation path and still keeps shell order, per-shell digests and in-order rejections, and reports the first defective shell in order when several are defective; writes temporary files.
#[test]
fn a_large_library_is_prepared_in_shell_order() {
    use rustmspt::config::placement::ShapeFilters;
    let tmp = tempfile::tempdir().unwrap();
    let shells: Vec<_> = (0..40)
        .map(|i| {
            let mut s = icosphere_mesh(Vec3::new(10.0 * i as f64, 0.0, 0.0), 1.0 + 0.02 * i as f64, 1);
            if i % 7 == 3 {
                for v in &mut s.vertices {
                    v.x = 10.0 * i as f64 + (v.x - 10.0 * i as f64) * 5.0;
                }
            }
            s
        })
        .collect();
    let path = write_library(tmp.path(), "many.stl", &shells);
    let filters = ShapeFilters { max_aspect_ratio: Some(3.0), max_sharpness_ratio: None };
    let lib = load_shape_library(std::slice::from_ref(&path), &["many.stl".to_string()], Some(&filters)).unwrap();
    let kept: Vec<usize> = lib.shells.iter().map(|s| s.shell_index).collect();
    let rejected: Vec<usize> = lib.rejected.iter().map(|r| r.shell_index).collect();
    assert_eq!(rejected, (0..40).filter(|i| i % 7 == 3).collect::<Vec<_>>(), "rejections in shell order");
    assert_eq!(kept, (0..40).filter(|i| i % 7 != 3).collect::<Vec<_>>(), "kept shells in shell order");
    let again = load_shape_library(std::slice::from_ref(&path), &["many.stl".to_string()], Some(&filters)).unwrap();
    let digests = |l: &rustmspt::pipeline::placement_library::ShapeLibrary| l.shells.iter().map(|s| s.shell_sha256.clone()).collect::<Vec<_>>();
    assert_eq!(digests(&lib), digests(&again));

    let mut broken = shells.clone();
    broken[33].faces.pop();
    broken[12].faces.pop();
    let bad = write_library(tmp.path(), "broken.stl", &broken);
    let err = load_shape_library(std::slice::from_ref(&bad), &["broken.stl".to_string()], None).unwrap_err().to_string();
    assert!(err.contains("shell 12"), "the first defective shell in order is reported: {err}");
}
