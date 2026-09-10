use rustmspt::config::{load_pack_document, PackDocument, ResolvedPlacement};
use rustmspt::geometry::{
    icosphere_mesh, merge_meshes, mesh_volume, point_inside_mesh, split_mesh_into_granules,
    to_parry_trimesh, VoidIndex, VoidVolumeMethod,
};
use rustmspt::io::{load_stl, save_stl, sha256_file};
use rustmspt::pipeline::placement::{read_record, read_report, run_placement};
use rustmspt::pipeline::rng::seeded_rng;
use rustmspt::types::{BoundingBox, Mesh, Vec3};
use std::fs;
use std::path::{Path, PathBuf};

fn sphere_void(centre: Vec3, radius: f64) -> Mesh {
    icosphere_mesh(centre, radius, 3)
}

fn resolve(config: &Path) -> ResolvedPlacement {
    match load_pack_document(config).expect("config loads") {
        PackDocument::Placement(p) => p.validate(config).expect("config validates"),
        PackDocument::Legacy(_) => panic!("expected the placement engine"),
    }
}

fn write_shapes(dir: &Path) -> PathBuf {
    let p = dir.join("shapes.stl");
    save_stl(&p, &icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 2), "s").unwrap();
    p
}

fn write_void(dir: &Path, mesh: &Mesh) -> PathBuf {
    let p = dir.join("void.stl");
    save_stl(&p, mesh, "v").unwrap();
    p
}

// ---------------------------------------------------------------- the index

// Two independent point-in-solid implementations must agree on every point, or
// one of them is wrong and there is no way to tell which from inside either.
#[test]
fn the_void_index_agrees_with_the_existing_point_in_mesh_test() {
    let mesh = sphere_void(Vec3::new(5.0, 5.0, 5.0), 3.0);
    let index = VoidIndex::build(&mesh).expect("a sphere is a valid void");
    let mut rng = seeded_rng(1);
    let mut inside = 0usize;
    for _ in 0..4_000 {
        let p = Vec3::new(
            rustmspt::pipeline::rng::uniform_range(&mut rng, 0.0, 10.0),
            rustmspt::pipeline::rng::uniform_range(&mut rng, 0.0, 10.0),
            rustmspt::pipeline::rng::uniform_range(&mut rng, 0.0, 10.0),
        );
        let a = index.contains_point(p);
        let b = point_inside_mesh(&mesh, p);
        assert_eq!(a, b, "the two point-in-solid tests disagree at {p:?}");
        if a {
            inside += 1;
        }
    }
    assert!(inside > 100, "the sampling must actually reach the void");
}

// Ray parity is correct for a shell inside another; a pseudo-normal test is not,
// which is why the index uses parity.
#[test]
fn nested_shells_are_classified_by_parity_not_by_winding() {
    let outer = sphere_void(Vec3::new(0.0, 0.0, 0.0), 8.0);
    let inner = sphere_void(Vec3::new(0.0, 0.0, 0.0), 3.0);
    let nested = merge_meshes(&[outer, inner]);
    let index = VoidIndex::build(&nested).expect("nested shells are valid");
    assert_eq!(index.shells(), 2);

    // Inside the inner shell, parity crosses twice going out: not in the void.
    assert!(!index.contains_point(Vec3::new(0.0, 0.0, 0.0)));
    // Between the shells: in the void.
    assert!(index.contains_point(Vec3::new(5.5, 0.0, 0.0)));
    // Outside both.
    assert!(!index.contains_point(Vec3::new(12.0, 0.0, 0.0)));
}

#[test]
fn an_open_or_mixed_orientation_void_is_refused_with_distinct_reasons() {
    let mut open = sphere_void(Vec3::new(0.0, 0.0, 0.0), 3.0);
    open.faces.pop();
    let err = VoidIndex::build(&open).unwrap_err().to_string();
    assert!(err.contains("shell 0"), "{err}");
    assert!(err.contains("closed solid"), "{err}");

    let a = sphere_void(Vec3::new(0.0, 0.0, 0.0), 3.0);
    let mut b = sphere_void(Vec3::new(20.0, 0.0, 0.0), 3.0);
    for f in &mut b.faces {
        std::mem::swap(&mut f.b, &mut f.c);
    }
    let err = VoidIndex::build(&merge_meshes(&[a, b]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("disagree about orientation"), "{err}");
    assert!(err.contains("cancel"), "the message must say why it matters: {err}");
}

// An all-inward void is usable: parity does not care about winding. It is
// reported as inward rather than silently normalised.
#[test]
fn an_all_inward_void_is_accepted_and_reported_as_inward() {
    let mut mesh = sphere_void(Vec3::new(0.0, 0.0, 0.0), 3.0);
    for f in &mut mesh.faces {
        std::mem::swap(&mut f.b, &mut f.c);
    }
    let index = VoidIndex::build(&mesh).expect("an inward void is usable");
    assert!(!index.is_outward());
    assert!(index.contains_point(Vec3::new(0.0, 0.0, 0.0)));
    assert!((index.total_volume() - mesh_volume(&mesh)).abs() < 1e-9);
}

#[test]
fn the_box_prefilter_separates_far_geometry() {
    let mesh = sphere_void(Vec3::new(0.0, 0.0, 0.0), 3.0);
    let index = VoidIndex::build(&mesh).unwrap();
    let far = BoundingBox {
        min: Vec3::new(50.0, 50.0, 50.0),
        max: Vec3::new(52.0, 52.0, 52.0),
    };
    assert!(!index.near_box(far, 1.0));
    let close = BoundingBox {
        min: Vec3::new(3.5, -0.5, -0.5),
        max: Vec3::new(4.5, 0.5, 0.5),
    };
    assert!(!index.near_box(close, 0.1), "half a unit away is not within 0.1");
    assert!(index.near_box(close, 1.0), "half a unit away is within 1.0");
}

// Two spheres a known distance apart: the measured gap must be the analytic one.
#[test]
fn the_surface_distance_matches_the_closed_form_for_two_spheres() {
    let void = sphere_void(Vec3::new(0.0, 0.0, 0.0), 3.0);
    let index = VoidIndex::build(&void).unwrap();
    for centre_gap in [8.0, 10.0, 15.0] {
        let particle = icosphere_mesh(Vec3::new(centre_gap, 0.0, 0.0), 2.0, 3);
        let shape = to_parry_trimesh(&particle).unwrap();
        let d = index.min_distance_to(&shape);
        let analytic = centre_gap - 3.0 - 2.0;
        // Both spheres are polyhedral approximations sitting inside the true
        // sphere, so the measured gap is slightly larger than the analytic one.
        assert!(
            d >= analytic && d - analytic < 0.05,
            "gap {d} against analytic {analytic}"
        );
    }
    // Touching and overlapping both give zero, which is why a zero gap cannot
    // forbid anything and the config refuses one.
    let overlapping = icosphere_mesh(Vec3::new(4.0, 0.0, 0.0), 2.0, 3);
    let shape = to_parry_trimesh(&overlapping).unwrap();
    assert_eq!(index.min_distance_to(&shape), 0.0);
    assert!(index.intersects(&shape));
}

#[test]
fn the_in_domain_volume_is_exact_when_the_void_is_inside_and_clipped_when_it_is_not() {
    let void = sphere_void(Vec3::new(20.0, 20.0, 20.0), 5.0);
    let index = VoidIndex::build(&void).unwrap();
    let big = BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(40.0, 40.0, 40.0),
    };
    let (v, method) = index.volume_in_domain(big);
    assert_eq!(method, VoidVolumeMethod::ExactShellSum);
    assert!((v - mesh_volume(&void)).abs() < 1e-9);
    // Within 0.3% of the analytic sphere at subdivision level 3.
    let analytic = 4.0 / 3.0 * std::f64::consts::PI * 125.0;
    assert!((v / analytic - 1.0).abs() < 0.01, "{v} against {analytic}");

    // Cut the sphere in half with the domain.
    let half = BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(40.0, 40.0, 20.0),
    };
    let (v2, method2) = index.volume_in_domain(half);
    assert_eq!(method2, VoidVolumeMethod::ExactClip);
    assert!(
        (v2 / (v * 0.5) - 1.0).abs() < 0.01,
        "half the sphere is {v2}, expected about {}",
        v * 0.5
    );
}

#[test]
fn surface_sampling_lands_on_the_surface_and_points_outward() {
    let void = sphere_void(Vec3::new(10.0, 10.0, 10.0), 4.0);
    let index = VoidIndex::build(&void).unwrap();
    let mut rng = seeded_rng(99);
    for _ in 0..2_000 {
        let (p, n) = index.sample_surface_point(&mut rng);
        let r = p.sub(Vec3::new(10.0, 10.0, 10.0));
        let radius = r.dot(r).sqrt();
        // On the polyhedral surface, so between the inscribed and circumscribed radii.
        assert!(
            (3.9..=4.01).contains(&radius),
            "sampled point is {radius} from the centre, not on the surface"
        );
        assert!((n.dot(n).sqrt() - 1.0).abs() < 1e-9, "the normal is not unit");
        // Outward: the normal agrees with the radial direction for a sphere.
        assert!(
            n.dot(r) > 0.0,
            "the normal points into the void rather than away from it"
        );
    }
}

// ---------------------------------------------------------------- runs with a void

fn void_case(dir: &Path, extra: &str, void_block: &str) -> PathBuf {
    write_shapes(dir);
    let p = dir.join("placement.yaml");
    fs::write(
        &p,
        format!(
            r#"
placement:
  seed: 20260910
  frame: {{ unit: "um" }}
  domain: {{ min: [0, 0, 0], max: [40, 40, 40] }}
  shapes:
    files: ["shapes.stl"]
  void:
{void_block}
  size:
    distribution: {{ kind: lognormal, median: 4.0, sigma_log: 0.25, min: 2.5, max: 6.0 }}
    classes: {{ kind: equal_width, count: 4 }}
  gaps: {{ particle_particle: 0.5 }}
  target: {{ volume_fraction: 0.04, basis: solid }}
  budget: {{ attempts_per_particle: 400, total_attempts: 300000 }}
  outputs:
    dir: "out"
{extra}
"#
        ),
    )
    .unwrap();
    p
}

// The central claim: every placed particle keeps its clearance from the void, and
// the check is re-run here independently of the engine's own.
#[test]
fn every_particle_keeps_its_clearance_from_the_void() {
    let tmp = tempfile::tempdir().unwrap();
    let void = sphere_void(Vec3::new(20.0, 20.0, 20.0), 8.0);
    write_void(tmp.path(), &void);
    let config = void_case(
        tmp.path(),
        "",
        "    file: \"void.stl\"\n    crossing: forbidden\n    gap: 2.0",
    );

    let outcome = run_placement(&resolve(&config)).expect("run succeeds");
    assert!(outcome.placed > 10, "placed only {}", outcome.placed);

    let index = VoidIndex::build(&void).unwrap();
    let record = read_record(&tmp.path().join("out/particles.json")).unwrap();
    let merged = load_stl(&tmp.path().join("out/particles.stl")).unwrap();

    for p in &record.particles {
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
        let shape = to_parry_trimesh(&m).expect("particle indexes");
        let d = index.min_distance_to(&shape);
        assert!(
            d >= 2.0 - 1e-6,
            "{} is {d} from the void, below the 2.0 clearance",
            p.entity_id
        );
        // Nothing of the particle is inside the void, and the void is not inside it.
        assert!(!index.any_vertex_inside(&m), "{} reaches into the void", p.entity_id);
        assert_eq!(p.void_overlap_volume, 0.0);
        assert_eq!(p.volume.in_domain_solid, p.volume.in_domain);
    }
}

#[test]
fn a_void_run_reports_the_void_and_copies_it_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let void = merge_meshes(&[
        sphere_void(Vec3::new(12.0, 12.0, 12.0), 5.0),
        sphere_void(Vec3::new(28.0, 28.0, 28.0), 4.0),
    ]);
    let void_path = write_void(tmp.path(), &void);
    let config = void_case(
        tmp.path(),
        "",
        "    file: \"void.stl\"\n    crossing: forbidden\n    gap: 1.5",
    );
    run_placement(&resolve(&config)).expect("run succeeds");

    let report = read_report(&tmp.path().join("out/run_report.json")).unwrap();
    let v = report.void.expect("the report describes the void");
    assert_eq!(v.shells, 2);
    assert_eq!(v.orientation, "outward");
    assert_eq!(v.crossing, "forbidden");
    assert_eq!(v.gap, 1.5);
    assert_eq!(v.volume_method, "exact_shell_sum");
    assert!(v.inside_domain);
    assert_eq!(v.overlap_owner, "void");
    assert!(v.overlap_voxel_size.is_none());

    // The freeze claim, checkable rather than asserted: the copy is the input.
    let (input_digest, _) = sha256_file(&void_path).unwrap();
    assert_eq!(v.sha256_in, input_digest);
    assert_eq!(v.sha256_out.as_deref(), Some(input_digest.as_str()));
    let copied = tmp.path().join("out/void.stl");
    assert!(copied.is_file());
    assert_eq!(fs::read(&copied).unwrap(), fs::read(&void_path).unwrap());

    // The basis is the domain minus the void, and the record names the phase.
    // The comparison is against the void as the engine read it, not as it was
    // built: `save_stl` stores float32, and the round trip moves this mesh's
    // volume by about 4e-5, which a 1e-6 tolerance against the in-memory value
    // would fail on for a reason that has nothing to do with the basis.
    assert_eq!(report.target.basis, "solid");
    let as_read = load_stl(&void_path).unwrap();
    let expected = 40.0f64.powi(3) - mesh_volume(&as_read);
    assert!(
        (report.target.basis_volume - expected).abs() < 1e-6,
        "basis {} against {expected}",
        report.target.basis_volume
    );
    // And it really is the domain minus something of the void's size.
    assert!(report.target.basis_volume < 40.0f64.powi(3));

    let record = read_record(&tmp.path().join("out/particles.json")).unwrap();
    let phase = record.phases.iter().find(|p| p.name == "void").unwrap();
    assert_eq!(phase.id, 2);
    assert_eq!(phase.overlap_owner.as_deref(), Some("void"));
    assert_eq!(phase.geometry.as_deref(), Some("void.stl"));
}

// A void that fills the domain leaves nowhere to place anything.
#[test]
fn a_void_filling_the_domain_places_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    write_void(tmp.path(), &sphere_void(Vec3::new(20.0, 20.0, 20.0), 60.0));
    let config = void_case(
        tmp.path(),
        "",
        "    file: \"void.stl\"\n    crossing: forbidden\n    gap: 1.0",
    );
    // The whole domain is inside the void, so the solid basis is negative and the
    // run refuses rather than dividing by it.
    let err = run_placement(&resolve(&config)).unwrap_err().to_string();
    assert!(err.contains("no solid region"), "{err}");
}

#[test]
fn a_void_that_does_not_reach_the_domain_is_a_frame_error() {
    let tmp = tempfile::tempdir().unwrap();
    write_void(tmp.path(), &sphere_void(Vec3::new(500.0, 500.0, 500.0), 5.0));
    let config = void_case(
        tmp.path(),
        "",
        "    file: \"void.stl\"\n    crossing: forbidden\n    gap: 1.0",
    );
    let err = run_placement(&resolve(&config)).unwrap_err().to_string();
    assert!(err.contains("frame or unit error"), "{err}");
}

// Crossing allowed: particles may reach into the void, the overlap is measured,
// and it comes out of the solid share rather than being counted twice.
#[test]
fn crossing_allowed_measures_the_overlap_and_the_void_keeps_it() {
    let tmp = tempfile::tempdir().unwrap();
    let void = sphere_void(Vec3::new(20.0, 20.0, 20.0), 9.0);
    write_void(tmp.path(), &void);
    let config = void_case(
        tmp.path(),
        "",
        "    file: \"void.stl\"\n    crossing: allowed\n    gap: 0.0\n    overlap_volume: { voxel_size: 0.25 }",
    );
    let outcome = run_placement(&resolve(&config)).expect("run succeeds");
    assert!(outcome.placed > 10);

    let index = VoidIndex::build(&void).unwrap();
    let record = read_record(&tmp.path().join("out/particles.json")).unwrap();
    let overlapping: Vec<_> = record
        .particles
        .iter()
        .filter(|p| p.void_overlap_volume > 0.0)
        .collect();
    assert!(
        !overlapping.is_empty(),
        "with crossing allowed some particle should reach into the void"
    );
    for p in &record.particles {
        assert!(p.void_overlap_volume >= 0.0);
        assert!(
            p.void_overlap_volume <= p.volume.full + 1e-9,
            "the overlap cannot exceed the particle"
        );
        // The identity the record publishes.
        assert!(
            (p.volume.in_domain_solid - (p.volume.in_domain - p.void_overlap_volume).max(0.0))
                .abs()
                < 1e-9
        );
        // A particle's centre is never inside the void, even when crossing is allowed.
        let c = Vec3::new(p.translation[0], p.translation[1], p.translation[2]);
        assert!(
            !index.contains_point(c),
            "{} has its centre inside a pore",
            p.entity_id
        );
    }

    let report = read_report(&tmp.path().join("out/run_report.json")).unwrap();
    let v = report.void.unwrap();
    assert_eq!(v.crossing, "allowed");
    assert_eq!(v.overlap_voxel_size, Some(0.25));
    // The solid fraction is net of the overlap, so it is the smaller of the two.
    assert!(report.actual.volume_in_domain_solid < report.actual.volume_in_domain);
}

// void_neighbourhood is a deliberate construction, and must never be reported as
// random. Every centroid must also land inside the declared band.
#[test]
fn void_neighbourhood_places_within_the_band_and_is_never_called_random() {
    let tmp = tempfile::tempdir().unwrap();
    let void = sphere_void(Vec3::new(20.0, 20.0, 20.0), 8.0);
    write_void(tmp.path(), &void);
    let config = void_case(
        tmp.path(),
        "  position: { mode: void_neighbourhood, band: [3.0, 7.0] }",
        "    file: \"void.stl\"\n    crossing: forbidden\n    gap: 2.0",
    );
    let outcome = run_placement(&resolve(&config)).expect("run succeeds");
    assert!(outcome.placed > 3, "placed only {}", outcome.placed);

    let index = VoidIndex::build(&void).unwrap();
    let record = read_record(&tmp.path().join("out/particles.json")).unwrap();
    for p in &record.particles {
        let c = Vec3::new(p.translation[0], p.translation[1], p.translation[2]);
        let d = index.surface_distance(c);
        assert!(
            (3.0 - 1e-6..=7.0 + 1e-6).contains(&d),
            "{} sits {d} from the void surface, outside the 3 to 7 band",
            p.entity_id
        );
    }

    let report = read_report(&tmp.path().join("out/run_report.json")).unwrap();
    assert_eq!(report.samplers["position"], "void_neighbourhood_band");
    assert!(
        !report.samplers["position"].contains("uniform"),
        "a deliberate construction must not be reported as uniform"
    );
    // The uniform mode is named differently, so the two cannot be confused.
    let plain = tempfile::tempdir().unwrap();
    write_void(plain.path(), &void);
    let c2 = void_case(
        plain.path(),
        "",
        "    file: \"void.stl\"\n    crossing: forbidden\n    gap: 2.0",
    );
    run_placement(&resolve(&c2)).unwrap();
    let r2 = read_report(&plain.path().join("out/run_report.json")).unwrap();
    assert_eq!(r2.samplers["position"], "rejection_uniform_rsa");
}

// Determinism must survive the void: the void queries add no thread dependence.
#[test]
fn a_void_run_is_byte_identical_across_thread_counts() {
    let tmp = tempfile::tempdir().unwrap();
    write_void(tmp.path(), &sphere_void(Vec3::new(20.0, 20.0, 20.0), 7.0));
    write_shapes(tmp.path());
    let mk = |name: &str, threads: u32| {
        let p = tmp.path().join(format!("{name}.yaml"));
        fs::write(
            &p,
            format!(
                r#"
placement:
  seed: 4242
  frame: {{ unit: "um" }}
  domain: {{ min: [0, 0, 0], max: [40, 40, 40] }}
  shapes: {{ files: ["shapes.stl"] }}
  void: {{ file: "void.stl", crossing: forbidden, gap: 1.5 }}
  size:
    distribution: {{ kind: lognormal, median: 4.0, sigma_log: 0.25, min: 2.5, max: 6.0 }}
  gaps: {{ particle_particle: 0.5 }}
  target: {{ volume_fraction: 0.04, basis: solid }}
  threads: {threads}
  budget: {{ attempts_per_particle: 400, total_attempts: 300000 }}
  outputs: {{ dir: "{name}" }}
"#
            ),
        )
        .unwrap();
        p
    };
    let a = run_placement(&resolve(&mk("one", 1))).unwrap();
    let b = run_placement(&resolve(&mk("eight", 8))).unwrap();
    assert_eq!(a.placed, b.placed);
    assert!(a.placed > 5);
    for name in ["particles.json", "particles.stl"] {
        assert_eq!(
            fs::read(tmp.path().join("one").join(name)).unwrap(),
            fs::read(tmp.path().join("eight").join(name)).unwrap(),
            "{name} differs between thread counts"
        );
    }
}

// ---------------------------------------------------------------- the committed fixture

#[test]
fn the_committed_void_fixture_loads_and_has_three_shells() {
    let path = Path::new("data/input/placement/void_spheres.stl");
    assert!(path.is_file(), "the committed void fixture is missing");
    let mesh = load_stl(path).expect("the fixture loads");
    assert_eq!(split_mesh_into_granules(&mesh).len(), 3);
    let index = VoidIndex::build(&mesh).expect("the fixture is a valid void");
    assert_eq!(index.shells(), 3);
    assert!(index.is_outward());
    // The three spheres the generator declares: radii 9, 7 and 6, at subdivision
    // level 2. A level-2 icosphere is an inscribed polyhedron, so it under-states
    // the analytic sphere by a measured 3.385 percent - the fixture is meant to be
    // small enough to commit, and this is the price. The bound is set from that
    // measurement rather than from a guess, and it is one-sided because a
    // polyhedron inscribed in a sphere can only be smaller.
    let analytic: f64 = [9.0f64, 7.0, 6.0]
        .iter()
        .map(|r| 4.0 / 3.0 * std::f64::consts::PI * r * r * r)
        .sum();
    let ratio = index.total_volume() / analytic;
    assert!(
        (0.960..=1.0).contains(&ratio),
        "fixture volume {} is {ratio} of the analytic {analytic}; level-2 icospheres measure 0.966",
        index.total_volume()
    );
    assert!(index.contains_point(Vec3::new(30.0, 30.0, 30.0)));
    assert!(!index.contains_point(Vec3::new(5.0, 5.0, 5.0)));
}
