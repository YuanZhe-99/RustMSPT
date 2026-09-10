use rustmspt::config::{load_pack_document, PackDocument, ResolvedPlacement};
use rustmspt::geometry::{
    icosphere_mesh, mesh_distance_exact, mesh_volume, merge_meshes, transform_shell, UnitQuat,
};
use rustmspt::io::{load_stl, save_stl};
use rustmspt::pipeline::placement::{read_record, read_report, run_placement};
use rustmspt::pipeline::placement_outputs::StopReason;
use rustmspt::types::{Mesh, Vec3};
use std::fs;
use std::path::{Path, PathBuf};

/// Builds a run directory: a shape library plus a config, both inside `dir`.
struct Case {
    dir: PathBuf,
    config: PathBuf,
}

fn shape_library(dir: &Path, radii: &[f64]) -> PathBuf {
    let meshes: Vec<Mesh> = radii
        .iter()
        .enumerate()
        .map(|(i, r)| icosphere_mesh(Vec3::new(i as f64 * 1000.0, 0.0, 0.0), *r, 2))
        .collect();
    let p = dir.join("shapes.stl");
    save_stl(&p, &merge_meshes(&meshes), "shapes").expect("write library");
    p
}

fn case(dir: &Path, body: &str) -> Case {
    shape_library(dir, &[1.0]);
    let config = dir.join("placement.yaml");
    fs::write(&config, body).expect("write config");
    Case {
        dir: dir.to_path_buf(),
        config,
    }
}

fn resolve(config: &Path) -> ResolvedPlacement {
    match load_pack_document(config).expect("config loads") {
        PackDocument::Placement(p) => p.validate(config).expect("config validates"),
        PackDocument::Legacy(_) => panic!("expected the placement engine"),
    }
}

/// A config with room for many particles: 40-unit box, small spheres, modest target.
fn roomy(seed: u64, extra: &str) -> String {
    format!(
        r#"
placement:
  seed: {seed}
  frame: {{ unit: "um" }}
  domain: {{ min: [0, 0, 0], max: [40, 40, 40] }}
  shapes:
    files: ["shapes.stl"]
  size:
    distribution: {{ kind: lognormal, median: 4.0, sigma_log: 0.25, min: 2.5, max: 7.0 }}
    classes: {{ kind: equal_width, count: 5 }}
  gaps: {{ particle_particle: 0.5 }}
  target: {{ volume_fraction: 0.05 }}
  budget: {{ attempts_per_particle: 400, total_attempts: 200000 }}
  outputs:
    dir: "out"
{extra}
"#
    )
}

// ---------------------------------------------------------------- a plain run

#[test]
fn a_plain_run_places_particles_and_writes_every_output() {
    let tmp = tempfile::tempdir().unwrap();
    let c = case(tmp.path(), &roomy(1, ""));
    let outcome = run_placement(&resolve(&c.config)).expect("run succeeds");

    assert!(outcome.placed > 5, "placed only {}", outcome.placed);
    assert_eq!(outcome.stop_reason, StopReason::TargetReached);

    let out = c.dir.join("out");
    for name in [
        "particles.stl",
        "particles.json",
        "run_report.json",
        "size_distribution.csv",
    ] {
        assert!(out.join(name).is_file(), "{name} was not written");
    }

    let record = read_record(&out.join("particles.json")).expect("record parses");
    assert_eq!(record.schema_version, "rustmspt.placement.record/1");
    assert_eq!(record.seed, 1);
    assert_eq!(record.rng, "chacha12");
    assert_eq!(record.particles.len(), outcome.placed);
    assert_eq!(record.frame.unit, "um");
    assert_eq!(record.frame.axis_order, "xyz");
    assert_eq!(record.frame.handedness, "right");
    // Every convention a reader needs is stated in the file itself.
    for key in [
        "rotation",
        "transform",
        "shell_centroid",
        "translation",
        "volume",
        "stl_precision",
        "triangle_range",
    ] {
        assert!(record.conventions.contains_key(key), "missing convention {key}");
    }
    assert!(record.conventions["rotation"].contains("w,x,y,z"));

    // Every required per-particle field carries a usable value.
    for (i, p) in record.particles.iter().enumerate() {
        assert_eq!(p.acceptance_index, i);
        assert_eq!(p.entity_id, format!("p{i:06}"));
        assert!(p.scale > 0.0);
        assert!(p.equivalent_diameter > 0.0);
        assert!(p.volume.full > 0.0);
        assert!(p.volume.in_domain > 0.0);
        assert!(!p.source_shape.sha256.is_empty());
        assert_eq!(p.source_shape.shell_sha256.len(), 64);
        let q = p.rotation.quaternion;
        let norm = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!((norm - 1.0).abs() < 1e-12, "quaternion is not unit");
        assert!(q[0] >= 0.0, "quaternion is not canonicalised");
    }

    let report = read_report(&out.join("run_report.json")).expect("report parses");
    assert_eq!(report.status, "finished");
    assert_eq!(report.stop_reason, Some(StopReason::TargetReached));
    assert!(!report.stop_detail["message"].is_empty());
    assert_eq!(report.samplers["orientation"], "shoemake_uniform_quaternion");
    assert_eq!(report.samplers["position"], "rejection_uniform_rsa");
    assert_eq!(report.target.basis, "domain");
    assert!((report.target.basis_volume - 64000.0).abs() < 1e-6);
    // Every rejection reason has a key, even at zero, so a reader can see the funnel.
    for key in [
        "outside_domain",
        "particle_overlap",
        "particle_gap",
        "zero_in_domain_volume",
        "inside_void",
        "void_gap",
    ] {
        assert!(report.rejections.contains_key(key), "missing rejection key {key}");
    }
    // The report lists itself with no digest: a file cannot contain its own hash.
    let self_entry = report.outputs.iter().find(|o| o.role == "report").unwrap();
    assert!(self_entry.sha256.is_none());
    let stl_entry = report.outputs.iter().find(|o| o.role == "particles").unwrap();
    assert!(stl_entry.sha256.is_some() && stl_entry.bytes.unwrap() > 0);
}

// ---------------------------------------------------------------- determinism

// Both runs share one directory, so the source path recorded in each file is the
// same string. Running them in separate temporary directories would make the
// records differ on `resolved_path` alone, which says nothing about determinism.
#[test]
fn the_same_seed_gives_byte_identical_output_on_one_thread_and_on_eight() {
    let tmp = tempfile::tempdir().unwrap();
    shape_library(tmp.path(), &[1.0]);
    let write = |name: &str, threads: u32| {
        let p = tmp.path().join(format!("{name}.yaml"));
        fs::write(
            &p,
            roomy(20260910, &format!("  threads: {threads}"))
                .replace("dir: \"out\"", &format!("dir: \"{name}\"")),
        )
        .unwrap();
        p
    };
    let one = write("one", 1);
    let eight = write("eight", 8);

    let oa = run_placement(&resolve(&one)).expect("run on one thread");
    let ob = run_placement(&resolve(&eight)).expect("run on eight threads");
    assert_eq!(oa.placed, ob.placed);
    assert!(oa.placed > 5);

    for name in ["particles.json", "particles.stl", "size_distribution.csv"] {
        let x = fs::read(tmp.path().join("one").join(name)).unwrap();
        let y = fs::read(tmp.path().join("eight").join(name)).unwrap();
        assert_eq!(x, y, "{name} differs between thread counts");
    }

    // The report differs only where it is meant to: the runtime object records
    // the thread count and the elapsed time, and the config and output entries
    // name the two different directories.
    let mut ra: serde_json::Value =
        serde_json::from_slice(&fs::read(tmp.path().join("one/run_report.json")).unwrap()).unwrap();
    let mut rb: serde_json::Value =
        serde_json::from_slice(&fs::read(tmp.path().join("eight/run_report.json")).unwrap())
            .unwrap();
    for v in [&mut ra, &mut rb] {
        let o = v.as_object_mut().unwrap();
        o.remove("runtime");
        o.remove("budget");
        o.remove("config");
        o.remove("outputs");
    }
    assert_eq!(ra, rb, "the report differs beyond its runtime and path fields");
}

#[test]
fn a_different_seed_places_differently() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let ca = case(a.path(), &roomy(1, ""));
    let cb = case(b.path(), &roomy(2, ""));
    run_placement(&resolve(&ca.config)).unwrap();
    run_placement(&resolve(&cb.config)).unwrap();

    let x = read_record(&a.path().join("out/particles.json")).unwrap();
    let y = read_record(&b.path().join("out/particles.json")).unwrap();
    assert_ne!(
        x.particles[0].translation, y.particles[0].translation,
        "two seeds produced the same first placement"
    );
}

// ---------------------------------------------------------------- reconstruction

// The acceptance test the consuming project will run: rebuild every particle from
// the record alone and compare it to the geometry that was written.
#[test]
fn every_particle_reconstructs_from_its_record_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let c = case(tmp.path(), &roomy(777, ""));
    run_placement(&resolve(&c.config)).expect("run succeeds");

    let out = c.dir.join("out");
    let record = read_record(&out.join("particles.json")).unwrap();
    let merged = load_stl(&out.join("particles.stl")).expect("merged STL loads");

    // The tolerance is 1e-5 times the largest domain extent. The dominant error is
    // the merged STL's float32 storage, about 6e-8 relative; a wrong quaternion
    // order, pivot or transform order would each be wrong by about a particle
    // diameter, four orders of magnitude above this bound.
    let domain = &record.frame.domain;
    let extent = (0..3)
        .map(|i| domain.max[i] - domain.min[i])
        .fold(0.0f64, f64::max);
    let tolerance = 1e-5 * extent;

    // The source shells, loaded the way a consumer would.
    let source = load_stl(&out.parent().unwrap().join("shapes.stl")).expect("source loads");
    let shells = rustmspt::geometry::split_mesh_into_granules(&source);

    assert!(!record.particles.is_empty());
    for p in &record.particles {
        let shell = &shells[p.source_shape.shell_index];
        // Centre on the RECORDED centroid, not a recomputed one: the record says
        // to use the value it carries, and a vertex mean is a different point.
        let c0 = Vec3::new(
            p.source_shape.shell_centroid[0],
            p.source_shape.shell_centroid[1],
            p.source_shape.shell_centroid[2],
        );
        let mut canonical = shell.clone();
        for v in &mut canonical.vertices {
            *v = v.sub(c0);
        }
        let q = UnitQuat::new(
            p.rotation.quaternion[0],
            p.rotation.quaternion[1],
            p.rotation.quaternion[2],
            p.rotation.quaternion[3],
        )
        .expect("recorded quaternion is usable");
        let t = Vec3::new(p.translation[0], p.translation[1], p.translation[2]);
        let rebuilt = transform_shell(&canonical, p.scale, q, t);

        let (start, end) = (p.triangle_range[0], p.triangle_range[1]);
        assert_eq!(
            end - start,
            rebuilt.faces.len(),
            "triangle range length disagrees with the source shell"
        );
        let mut worst = 0.0f64;
        for (k, face) in merged.faces[start..end].iter().enumerate() {
            let rf = &rebuilt.faces[k];
            for (a, b) in [(face.a, rf.a), (face.b, rf.b), (face.c, rf.c)] {
                let d = merged.vertices[a].sub(rebuilt.vertices[b]);
                worst = worst.max(d.dot(d).sqrt());
            }
        }
        assert!(
            worst < tolerance,
            "particle {} reconstructs to within {worst}, tolerance {tolerance}",
            p.entity_id
        );

        // The recorded volume must be the rebuilt particle's volume.
        let v = mesh_volume(&rebuilt);
        assert!(
            (v - p.volume.full).abs() / p.volume.full < 1e-6,
            "recorded volume {} against rebuilt {v}",
            p.volume.full
        );
    }

    // The triangle ranges must tile the merged mesh exactly, with no gap or overlap.
    let mut cursor = 0usize;
    for p in &record.particles {
        assert_eq!(p.triangle_range[0], cursor, "a gap in the triangle ranges");
        cursor = p.triangle_range[1];
    }
    assert_eq!(cursor, merged.faces.len(), "the ranges do not cover the mesh");
}

// The record's quaternion must mean what it says under an independent library, or
// a consumer reading it with nalgebra gets a plausible, wrong orientation.
#[test]
fn the_recorded_quaternion_means_the_same_thing_to_nalgebra() {
    let tmp = tempfile::tempdir().unwrap();
    let c = case(tmp.path(), &roomy(31, ""));
    run_placement(&resolve(&c.config)).unwrap();
    let record = read_record(&c.dir.join("out/particles.json")).unwrap();

    for p in record.particles.iter().take(20) {
        let [w, x, y, z] = p.rotation.quaternion;
        let na =
            nalgebra::UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(w, x, y, z));
        let m = na.to_rotation_matrix();
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    (p.rotation.matrix[i][j] - m[(i, j)]).abs() < 1e-12,
                    "the recorded matrix disagrees with the recorded quaternion at {i},{j}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------- constraints

#[test]
fn every_placed_pair_clears_the_required_gap() {
    let tmp = tempfile::tempdir().unwrap();
    let c = case(tmp.path(), &roomy(5, ""));
    run_placement(&resolve(&c.config)).unwrap();
    let record = read_record(&c.dir.join("out/particles.json")).unwrap();
    let merged = load_stl(&c.dir.join("out/particles.stl")).unwrap();

    // Rebuild each particle's own mesh from the merged STL's triangle range and
    // re-measure the pairwise distance independently of the engine's own check.
    let particles: Vec<Mesh> = record
        .particles
        .iter()
        .map(|p| {
            let (s, e) = (p.triangle_range[0], p.triangle_range[1]);
            let mut m = Mesh::empty();
            for f in &merged.faces[s..e] {
                let base = m.vertices.len();
                m.vertices.push(merged.vertices[f.a]);
                m.vertices.push(merged.vertices[f.b]);
                m.vertices.push(merged.vertices[f.c]);
                m.faces.push(rustmspt::types::Triangle {
                    a: base,
                    b: base + 1,
                    c: base + 2,
                });
            }
            m
        })
        .collect();

    assert!(particles.len() > 5);
    let gap = 0.5;
    let boxes: Vec<_> = record
        .particles
        .iter()
        .map(|p| {
            rustmspt::types::BoundingBox {
                min: Vec3::new(p.bbox.min[0], p.bbox.min[1], p.bbox.min[2]),
                max: Vec3::new(p.bbox.max[0], p.bbox.max[1], p.bbox.max[2]),
            }
        })
        .collect();
    let mut measured = 0usize;
    for i in 0..particles.len() {
        for j in (i + 1)..particles.len() {
            // Boxes farther apart than the gap cannot hide a violation, and the
            // exact query is expensive enough to dominate the suite without this.
            if rustmspt::geometry::bbox_distance(boxes[i], boxes[j]) >= gap {
                continue;
            }
            measured += 1;
            let d = mesh_distance_exact(&particles[i], &particles[j]);
            assert!(
                d >= gap - 1e-6,
                "particles {i} and {j} are {d} apart, below the {gap} gap"
            );
        }
    }
    assert!(measured > 0, "no pair was close enough to be worth measuring");
}

#[test]
fn strict_mode_keeps_every_particle_inside_and_reports_nothing_clipped() {
    let tmp = tempfile::tempdir().unwrap();
    let c = case(tmp.path(), &roomy(9, ""));
    run_placement(&resolve(&c.config)).unwrap();
    let record = read_record(&c.dir.join("out/particles.json")).unwrap();

    for p in &record.particles {
        assert!(!p.clipped.any, "{} is marked clipped in strict mode", p.entity_id);
        assert!(p.clipped.faces.is_empty());
        assert!(
            (p.volume.in_domain - p.volume.full).abs() < 1e-9,
            "an interior particle must keep its whole volume"
        );
        for i in 0..3 {
            assert!(p.bbox.min[i] >= record.frame.domain.min[i] - 1e-9);
            assert!(p.bbox.max[i] <= record.frame.domain.max[i] + 1e-9);
        }
    }
}

#[test]
fn clip_mode_reports_the_faces_it_cut_and_a_smaller_in_domain_volume() {
    let tmp = tempfile::tempdir().unwrap();
    let c = case(tmp.path(), &roomy(13, "  boundary: { mode: clip }"));
    run_placement(&resolve(&c.config)).unwrap();
    let record = read_record(&c.dir.join("out/particles.json")).unwrap();

    let clipped: Vec<_> = record.particles.iter().filter(|p| p.clipped.any).collect();
    assert!(
        !clipped.is_empty(),
        "clip mode over a small box should clip something"
    );
    for p in &clipped {
        assert!(!p.clipped.faces.is_empty());
        for f in &p.clipped.faces {
            assert!(
                ["xmin", "xmax", "ymin", "ymax", "zmin", "zmax"].contains(&f.as_str()),
                "unexpected face name {f}"
            );
        }
        assert!(
            p.volume.in_domain < p.volume.full,
            "a clipped particle keeps less than its whole volume"
        );
        assert!(p.volume.in_domain > 0.0);
    }
    for p in record.particles.iter().filter(|p| !p.clipped.any) {
        assert!((p.volume.in_domain - p.volume.full).abs() < 1e-9);
    }
}

// ---------------------------------------------------------------- stop reasons

#[test]
fn a_domain_too_small_for_anything_places_nothing_and_still_exits_ok() {
    let tmp = tempfile::tempdir().unwrap();
    shape_library(tmp.path(), &[1.0]);
    let config = tmp.path().join("placement.yaml");
    fs::write(
        &config,
        r#"
placement:
  seed: 1
  frame: { unit: "um" }
  domain: { min: [0, 0, 0], max: [1, 1, 1] }
  shapes:
    files: ["shapes.stl"]
  size:
    distribution: { kind: lognormal, median: 20.0, sigma_log: 0.1, min: 18.0, max: 22.0 }
  target: { volume_fraction: 0.3 }
  budget: { attempts_per_particle: 20, total_attempts: 200 }
  outputs:
    dir: "out"
"#,
    )
    .unwrap();

    let outcome = run_placement(&resolve(&config)).expect("stopping short is not an error");
    assert_eq!(outcome.placed, 0);
    assert_eq!(outcome.stop_reason, StopReason::NoFeasiblePlacement);

    let out = tmp.path().join("out");
    // No STL: save_stl refuses an empty mesh, and calling it would turn a result
    // into a non-zero exit.
    assert!(!out.join("particles.stl").exists());
    let record = read_record(&out.join("particles.json")).unwrap();
    assert!(record.particles.is_empty());
    assert!(record.phases[1].geometry.is_none());
    let report = read_report(&out.join("run_report.json")).unwrap();
    assert_eq!(report.stop_reason, Some(StopReason::NoFeasiblePlacement));
    assert!(report.stop_detail.contains_key("dominant_rejection"));
    assert!(report.stop_detail["message"].contains("no particle"));
}

#[test]
fn a_tiny_total_budget_stops_as_budget_exhausted() {
    let tmp = tempfile::tempdir().unwrap();
    let c = case(
        tmp.path(),
        &roomy(1, "").replace(
            "budget: { attempts_per_particle: 400, total_attempts: 200000 }",
            "budget: { attempts_per_particle: 400, total_attempts: 3 }",
        ),
    );
    let outcome = run_placement(&resolve(&c.config)).expect("run succeeds");
    let report = read_report(&c.dir.join("out/run_report.json")).unwrap();
    assert_eq!(outcome.stop_reason, StopReason::BudgetExhausted);
    assert_eq!(report.stop_detail["limit"], "total_attempts");
    assert!(report.budget.consumed_attempts <= 3);
}

// The failure mode the plan-first design exists to prevent: a class that cannot be
// placed must show as a shortfall, and nothing may be drawn to make it up.
// The failure mode the plan-first design exists to prevent: a class that cannot be
// placed must show as a shortfall, and nothing may be drawn to make it up.
//
// The domain is a thin slab. A sphere of diameter 9 to 10 cannot fit its 6-unit
// thickness however it is turned, while the small class fits easily. Making the
// impossible class *tall* rather than merely voluminous is what keeps it from
// dominating the planned volume: one particle far too big for the box would
// otherwise be the whole plan, and the run would place nothing at all and report
// no_feasible_placement instead.
#[test]
fn a_size_that_cannot_fit_is_a_reported_shortfall_with_no_replacement() {
    let tmp = tempfile::tempdir().unwrap();
    shape_library(tmp.path(), &[1.0]);
    let config = tmp.path().join("placement.yaml");
    fs::write(tmp.path().join("sizes.csv"), "bin,right,frequency\n3,4,0.8\n9,10,0.2\n").unwrap();
    fs::write(
        &config,
        r#"
placement:
  seed: 4242
  frame: { unit: "um" }
  domain: { min: [0, 0, 0], max: [60, 60, 6] }
  shapes:
    files: ["shapes.stl"]
  size:
    distribution: { kind: histogram, csv: "sizes.csv" }
    on_unattainable: skip_reported
  target: { volume_fraction: 0.05 }
  budget: { attempts_per_particle: 40, total_attempts: 200000 }
  outputs:
    dir: "out"
"#,
    )
    .unwrap();

    let outcome = run_placement(&resolve(&config)).expect("run succeeds");
    assert!(outcome.placed > 0, "the small class must still be placed");
    assert_eq!(outcome.stop_reason, StopReason::DistributionUnattainable);

    let report = read_report(&tmp.path().join("out/run_report.json")).unwrap();
    assert!(report.stop_detail.contains_key("first_failed_diameter"));
    assert_eq!(report.stop_detail["on_unattainable"], "skip_reported");
    // No top-up was drawn: that is what "no replacement" means.
    assert_eq!(report.top_up.batches, 0);
    assert_eq!(report.top_up.drawn, 0);

    // The shortfall sits in the tall class, not spread over the small one.
    let big = report
        .size_classes
        .iter()
        .find(|c| c.lo >= 9.0)
        .expect("the tall class is reported");
    assert!(big.drawn > 0, "the tall class was drawn from");
    assert!(big.shortfall > 0, "the tall class must show a shortfall");
    assert_eq!(big.placed, 0, "nothing 9 units across fits a 6-unit slab");
    assert_eq!(big.top_up_drawn, 0);

    let small = report
        .size_classes
        .iter()
        .find(|c| c.hi <= 4.5)
        .expect("the small class is reported");
    assert!(small.placed > 0, "the small class must have been placed");

    // The CSV says the same thing, in the same columns.
    let csv = fs::read_to_string(tmp.path().join("out/size_distribution.csv")).unwrap();
    let header = csv.lines().next().unwrap();
    assert_eq!(
        header,
        "class,lo,hi,target_frequency,target_count,drawn,placed,shortfall,top_up_drawn,top_up_placed"
    );
    assert_eq!(csv.lines().count(), report.size_classes.len() + 1);
}

#[test]
fn on_unattainable_stop_ends_at_the_first_failure() {
    let tmp = tempfile::tempdir().unwrap();
    shape_library(tmp.path(), &[1.0]);
    fs::write(tmp.path().join("sizes.csv"), "bin,right,frequency\n3,4,0.8\n9,10,0.2\n").unwrap();
    let mk = |mode: &str| {
        let p = tmp.path().join(format!("{mode}.yaml"));
        fs::write(
            &p,
            format!(
                r#"
placement:
  seed: 4242
  frame: {{ unit: "um" }}
  domain: {{ min: [0, 0, 0], max: [60, 60, 6] }}
  shapes:
    files: ["shapes.stl"]
  size:
    distribution: {{ kind: histogram, csv: "sizes.csv" }}
    on_unattainable: {mode}
  target: {{ volume_fraction: 0.05 }}
  budget: {{ attempts_per_particle: 40, total_attempts: 200000 }}
  outputs:
    dir: "out_{mode}"
"#
            ),
        )
        .unwrap();
        p
    };

    let skip = run_placement(&resolve(&mk("skip_reported"))).unwrap();
    let stop = run_placement(&resolve(&mk("stop"))).unwrap();

    // `skip_reported` carries on past the sizes that cannot fit and places the
    // rest, so the run has a distribution to report a shortfall against.
    assert_eq!(skip.stop_reason, StopReason::DistributionUnattainable);
    assert!(skip.placed > 0);

    // `stop` gives up at the first failure. Sizes are placed largest first by
    // default, and here the largest cannot fit at all, so the run ends having
    // placed nothing - and a run with no particles has no distribution to
    // describe, so the zero-particle reason outranks the distribution one. This
    // combination is a sharp edge worth pinning: `stop` with the default
    // descending order turns one impossible size into an empty result.
    assert_eq!(stop.placed, 0);
    assert_eq!(stop.stop_reason, StopReason::NoFeasiblePlacement);
    let report = read_report(&tmp.path().join("out_stop/run_report.json")).unwrap();
    assert_eq!(report.stop_detail["dominant_rejection"], "outside_domain");
    assert!(skip.placed > stop.placed);
}

// ---------------------------------------------------------------- failure modes

#[test]
fn an_unwritable_output_directory_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let blocker = tmp.path().join("out");
    fs::write(&blocker, b"not a directory").unwrap();
    let c = case(tmp.path(), &roomy(1, ""));
    assert!(
        run_placement(&resolve(&c.config)).is_err(),
        "a file where the output directory should be must be an error"
    );
}

#[test]
fn a_void_block_is_refused_until_void_support_lands() {
    let tmp = tempfile::tempdir().unwrap();
    let c = case(
        tmp.path(),
        &roomy(1, "  void: { file: \"v.stl\", crossing: forbidden, gap: 1.0 }"),
    );
    let err = run_placement(&resolve(&c.config)).unwrap_err().to_string();
    assert!(err.contains("void"), "{err}");
}
