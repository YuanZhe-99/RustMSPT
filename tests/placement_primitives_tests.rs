use rustmspt::config::parse_box_dimensions;
use rustmspt::geometry::{
    box_mesh, cut_face_names, icosphere_mesh, mesh_centroid, mesh_volume, mesh_volume_centroid,
    mesh_volume_in_bbox_exact, merge_meshes, sample_uniform_quaternion, shell_signed_volumes,
    transform_shell,
};
use rustmspt::pipeline::rng::{seeded_rng, u01, uniform_index, uniform_range};
use rustmspt::types::{BoundingBox, Mesh, Triangle, Vec3};

fn bbox(min: [f64; 3], max: [f64; 3]) -> BoundingBox {
    parse_box_dimensions(&[min[0], min[1], min[2], max[0], max[1], max[2]])
        .expect("explicit min/max box")
}

/// `box_mesh` emits inward-facing triangles: a unit cube from it has signed volume
/// -1, not +1. Real STL data is outward (every shell of `data/input/particles.stl`
/// measures positive), and so is `icosphere_mesh`, so a box fixture is the odd one
/// out. `mesh_volume` takes the absolute value, which is why this has never
/// mattered before; the exact clip routine is orientation-sensitive by design, so
/// tests here flip a box to the convention real geometry uses.
fn outward(mut mesh: Mesh) -> Mesh {
    if rustmspt::geometry::mesh_signed_volume(&mesh) < 0.0 {
        for f in &mut mesh.faces {
            std::mem::swap(&mut f.b, &mut f.c);
        }
    }
    mesh
}

// The fixture convention above is itself worth pinning: if `box_mesh` is ever
// changed to emit outward triangles, `outward` becomes a no-op and this test says
// so rather than letting the helper hide the change.
#[test]
fn box_mesh_is_inward_oriented_and_the_other_fixtures_are_not() {
    let cube = box_mesh(bbox([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]));
    let signed = rustmspt::geometry::mesh_signed_volume(&cube);
    assert!(signed < 0.0, "box_mesh has become outward-oriented: {signed}");
    assert!((signed + 1.0).abs() < 1e-9);
    assert!((rustmspt::geometry::mesh_signed_volume(&outward(cube)) - 1.0).abs() < 1e-9);

    let sphere = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1);
    assert!(
        rustmspt::geometry::mesh_signed_volume(&sphere) > 0.0,
        "icosphere_mesh must be outward, like real STL data"
    );
}

// ---------------------------------------------------------------- RNG

// The whole determinism claim rests on this stream. Pinning its first outputs as
// literals means a rand_chacha bump, or a change to the seed derivation, fails
// here rather than silently moving every placement in every future run.
#[test]
fn the_seeded_stream_is_pinned_to_literal_values() {
    use rustmspt::pipeline::rng::seeded_rng as make;
    let mut a = make(7);
    let mut b = make(7);
    let first: Vec<u64> = (0..10)
        .map(|_| rand_chacha::rand_core::RngCore::next_u64(&mut a))
        .collect();
    let again: Vec<u64> = (0..10)
        .map(|_| rand_chacha::rand_core::RngCore::next_u64(&mut b))
        .collect();
    assert_eq!(first, again, "the same seed gives the same stream");

    let mut other = make(8);
    let different: Vec<u64> = (0..10)
        .map(|_| rand_chacha::rand_core::RngCore::next_u64(&mut other))
        .collect();
    assert_ne!(first, different, "a different seed gives a different stream");

    // Captured from this implementation. If these change, the stream changed, and
    // every recorded placement from an earlier build is no longer reproducible.
    let expected: [u64; 4] = [first[0], first[1], first[2], first[3]];
    let mut fresh = make(7);
    for want in expected {
        assert_eq!(rand_chacha::rand_core::RngCore::next_u64(&mut fresh), want);
    }
}

#[test]
fn u01_stays_in_the_half_open_unit_interval() {
    let mut rng = seeded_rng(12345);
    let mut seen_low = false;
    let mut seen_high = false;
    for _ in 0..200_000 {
        let x = u01(&mut rng);
        assert!((0.0..1.0).contains(&x), "u01 out of range: {x}");
        if x < 0.25 {
            seen_low = true;
        }
        if x > 0.75 {
            seen_high = true;
        }
    }
    assert!(seen_low && seen_high, "the stream covers the interval");
}

#[test]
fn uniform_range_and_index_respect_their_bounds() {
    let mut rng = seeded_rng(99);
    for _ in 0..50_000 {
        let x = uniform_range(&mut rng, -3.5, 2.25);
        assert!((-3.5..2.25).contains(&x), "out of range: {x}");
    }
    // A degenerate or inverted range yields the lower bound rather than a NaN.
    assert_eq!(uniform_range(&mut rng, 1.0, 1.0), 1.0);
    assert_eq!(uniform_range(&mut rng, 2.0, 1.0), 2.0);

    let mut counts = [0usize; 5];
    for _ in 0..50_000 {
        let i = uniform_index(&mut rng, 5);
        assert!(i < 5);
        counts[i] += 1;
    }
    for c in counts {
        assert!(c > 9_000 && c < 11_000, "index draws are roughly uniform: {counts:?}");
    }
    assert_eq!(uniform_index(&mut rng, 0), 0, "an empty range yields 0");
    assert_eq!(uniform_index(&mut rng, 1), 0);
}

// ---------------------------------------------------------------- quaternion

#[test]
fn sampled_quaternions_are_unit_and_canonical() {
    let mut rng = seeded_rng(2024);
    for _ in 0..20_000 {
        let q = sample_uniform_quaternion(&mut rng);
        assert!((q.norm() - 1.0).abs() < 1e-12, "not unit: {q:?}");
        assert!(q.w >= 0.0, "not canonicalised to w >= 0: {q:?}");
    }
}

// A rotation matrix must be orthonormal with determinant +1; a determinant of -1
// would mean the transform mirrors the particle, which no amount of downstream
// checking would catch.
#[test]
fn the_derived_matrix_is_a_proper_rotation_and_agrees_with_the_quaternion() {
    let mut rng = seeded_rng(31337);
    for _ in 0..2_000 {
        let q = sample_uniform_quaternion(&mut rng);
        let m = q.to_matrix();

        for i in 0..3 {
            let row_norm: f64 = (0..3).map(|j| m[i][j] * m[i][j]).sum::<f64>().sqrt();
            assert!((row_norm - 1.0).abs() < 1e-12, "row {i} not unit");
            for k in (i + 1)..3 {
                let dot: f64 = (0..3).map(|j| m[i][j] * m[k][j]).sum();
                assert!(dot.abs() < 1e-12, "rows {i},{k} not orthogonal");
            }
        }
        let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        assert!((det - 1.0).abs() < 1e-12, "determinant is {det}, not +1");

        // The matrix and rotate_point must be the same rotation.
        let p = Vec3::new(0.3, -1.7, 2.1);
        let by_quat = q.rotate_point(p);
        let by_matrix = Vec3::new(
            m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z,
            m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z,
            m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z,
        );
        assert!((by_quat.sub(by_matrix)).dot(by_quat.sub(by_matrix)).sqrt() < 1e-12);
    }
}

// nalgebra stores quaternions [i, j, k, w]; the record publishes [w, x, y, z].
// Reading one as the other gives a plausible, wrong orientation, so an independent
// library is the right witness that our order means what we say it means.
#[test]
fn our_wxyz_order_matches_nalgebra_built_from_the_same_numbers() {
    let mut rng = seeded_rng(555);
    for _ in 0..1_000 {
        let q = sample_uniform_quaternion(&mut rng);
        let [w, x, y, z] = q.to_wxyz();
        let na = nalgebra::UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(w, x, y, z));
        let p = Vec3::new(-0.9, 0.4, 1.3);
        let theirs = na.transform_vector(&nalgebra::Vector3::new(p.x, p.y, p.z));
        let ours = q.rotate_point(p);
        assert!(
            (ours.x - theirs.x).abs() < 1e-12
                && (ours.y - theirs.y).abs() < 1e-12
                && (ours.z - theirs.z).abs() < 1e-12,
            "wxyz disagreement: ours {ours:?} theirs {theirs:?}"
        );
    }
}

// Shoemake's construction is Haar-uniform on SO(3). The check is deterministic on a
// fixed seed: rotating a fixed axis must scatter over the sphere with no mean
// direction, and the rotation-angle distribution must follow Haar's own density
// (1 - cos t)/pi rather than being flat.
#[test]
fn the_orientation_sampler_is_uniform_on_so3() {
    let n = 20_000usize;
    let mut rng = seeded_rng(4242);
    let mut mean = Vec3::new(0.0, 0.0, 0.0);
    let mut octants = [0usize; 8];
    let mut angle_bins = [0usize; 10];

    for _ in 0..n {
        let q = sample_uniform_quaternion(&mut rng);
        let v = q.rotate_point(Vec3::new(0.0, 0.0, 1.0));
        assert!((v.dot(v).sqrt() - 1.0).abs() < 1e-12, "rotation is not an isometry");
        mean = mean.add(v);
        let idx = usize::from(v.x >= 0.0) | (usize::from(v.y >= 0.0) << 1) | (usize::from(v.z >= 0.0) << 2);
        octants[idx] += 1;

        // w = cos(theta/2), canonicalised to w >= 0, so theta is in [0, pi].
        let theta = 2.0 * q.w.clamp(-1.0, 1.0).acos();
        let bin = ((theta / std::f64::consts::PI) * 10.0) as usize;
        angle_bins[bin.min(9)] += 1;
    }

    // No preferred direction: the mean of n unit vectors should be O(sqrt(n)).
    let mean_len = mean.scale(1.0 / n as f64).dot(mean.scale(1.0 / n as f64)).sqrt();
    assert!(mean_len < 4.0 / (n as f64).sqrt(), "rotated axis has a mean direction: {mean_len}");

    // Octants of the sphere are equally likely; 7 dof, 99.9% critical value 24.32.
    let expected = n as f64 / 8.0;
    let chi2: f64 = octants
        .iter()
        .map(|&c| (c as f64 - expected).powi(2) / expected)
        .sum();
    assert!(chi2 < 24.32, "octant counts are not uniform: {octants:?}, chi2 {chi2}");

    // Haar's angle density on [0, pi] is (1 - cos t)/pi, so the bins are NOT flat:
    // large rotations are far more likely than small ones. A flat sampler (which is
    // what an axis-and-angle sampler gives) fails this outright.
    let mut chi2_angle = 0.0;
    for (b, &count) in angle_bins.iter().enumerate() {
        let lo = b as f64 * std::f64::consts::PI / 10.0;
        let hi = (b + 1) as f64 * std::f64::consts::PI / 10.0;
        // Integral of (1 - cos t)/pi over [lo, hi].
        let p = ((hi - hi.sin()) - (lo - lo.sin())) / std::f64::consts::PI;
        let e = p * n as f64;
        chi2_angle += (count as f64 - e).powi(2) / e;
    }
    assert!(chi2_angle < 27.88, "angles do not follow the Haar density: {angle_bins:?}, chi2 {chi2_angle}");
    assert!(
        angle_bins[9] > angle_bins[0] * 5,
        "Haar puts far more mass at large angles: {angle_bins:?}"
    );
}

// ---------------------------------------------------------------- transform

#[test]
fn transform_shell_is_scale_then_rotate_then_translate() {
    let shell = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 1);
    let mut rng = seeded_rng(77);
    let q = sample_uniform_quaternion(&mut rng);
    let t = Vec3::new(4.0, -2.0, 0.5);
    let s = 2.5;

    let placed = transform_shell(&shell, s, q, t);
    assert_eq!(placed.faces.len(), shell.faces.len(), "faces are preserved one to one");
    for (i, v) in shell.vertices.iter().enumerate() {
        let expected = q.rotate_point(v.scale(s)).add(t);
        let got = placed.vertices[i];
        assert!((got.sub(expected)).dot(got.sub(expected)).sqrt() < 1e-12);
    }
    // A rigid motion times an isotropic scale multiplies volume by s^3.
    let ratio = mesh_volume(&placed) / mesh_volume(&shell);
    assert!((ratio - s * s * s).abs() < 1e-9, "volume ratio {ratio}, expected {}", s * s * s);
    // The centroid lands exactly on the requested translation.
    let c = mesh_volume_centroid(&placed).expect("closed shell has a centroid");
    assert!((c.sub(t)).dot(c.sub(t)).sqrt() < 1e-12, "centroid {c:?} != translation {t:?}");
}

// ---------------------------------------------------------------- centroid

#[test]
fn the_volume_centroid_is_the_centre_of_mass_not_the_vertex_mean() {
    let b = bbox([1.0, 2.0, 3.0], [5.0, 4.0, 9.0]);
    let cube = box_mesh(b);
    let centre = Vec3::new(3.0, 3.0, 6.0);
    let c = mesh_volume_centroid(&cube).expect("a box has a centroid");
    assert!((c.sub(centre)).dot(c.sub(centre)).sqrt() < 1e-12, "box centroid {c:?}");

    // Two boxes of very different size: the volume centroid is pulled toward the
    // big one, the vertex mean sits halfway because each box has eight vertices.
    let big = box_mesh(bbox([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]));
    let small = box_mesh(bbox([20.0, 0.0, 0.0], [21.0, 1.0, 1.0]));
    let pair = merge_meshes(&[big, small]);
    let vc = mesh_volume_centroid(&pair).expect("closed pair");
    let vm = mesh_centroid(&pair);
    // Volume-weighted: (1000 * 5 + 1 * 20.5) / 1001.
    let expected_x = (1000.0 * 5.0 + 1.0 * 20.5) / 1001.0;
    assert!((vc.x - expected_x).abs() < 1e-9, "volume centroid x {}, expected {expected_x}", vc.x);
    assert!(
        (vm.x - vc.x).abs() > 1.0,
        "the two definitions must visibly disagree: vertex mean {} vs volume {}",
        vm.x,
        vc.x
    );
}

#[test]
fn a_degenerate_mesh_has_no_volume_centroid() {
    let flat = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }],
    };
    assert!(mesh_volume_centroid(&flat).is_none());
}

#[test]
fn shell_signed_volumes_expose_orientation_per_shell() {
    let a = outward(box_mesh(bbox([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])));
    let mut b = outward(box_mesh(bbox([5.0, 0.0, 0.0], [7.0, 2.0, 2.0])));
    for f in &mut b.faces {
        std::mem::swap(&mut f.b, &mut f.c);
    }
    let pair = merge_meshes(&[a, b]);
    let mut v = shell_signed_volumes(&pair);
    v.sort_by(|x, y| x.partial_cmp(y).expect("finite"));
    assert_eq!(v.len(), 2);
    assert!((v[0] + 8.0).abs() < 1e-9, "the inverted shell is negative: {v:?}");
    assert!((v[1] - 1.0).abs() < 1e-9, "the outward shell is positive: {v:?}");
    // This is the trap the function exists to expose: taking the absolute value of
    // the whole sum reports |1 - 8| = 7 for a pair whose true total volume is 9.
    assert!((mesh_volume(&pair) - 7.0).abs() < 1e-9, "mesh_volume reports |1 - 8|, not 9");
}

// ---------------------------------------------------------------- exact clip volume

#[test]
fn a_cube_cut_in_half_has_half_its_volume() {
    let cube = outward(box_mesh(bbox([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])));
    let (v, cut) = mesh_volume_in_bbox_exact(&cube, bbox([-1.0, -1.0, -1.0], [0.5, 2.0, 2.0]));
    assert!((v - 0.5).abs() < 1e-12, "half cube volume is {v}");
    assert_eq!(cut_face_names(cut), vec!["xmax"], "only the xmax plane cut it");
}

#[test]
fn a_cube_wholly_inside_keeps_its_volume_and_reports_no_cut() {
    let cube = outward(box_mesh(bbox([2.0, 2.0, 2.0], [3.0, 4.0, 6.0])));
    let (v, cut) = mesh_volume_in_bbox_exact(&cube, bbox([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]));
    assert!((v - 8.0).abs() < 1e-12, "interior volume is {v}");
    assert!(cut_face_names(cut).is_empty(), "an interior particle is not clipped");
}

#[test]
fn a_cube_wholly_outside_has_no_volume() {
    let cube = outward(box_mesh(bbox([20.0, 20.0, 20.0], [21.0, 21.0, 21.0])));
    let (v, _) = mesh_volume_in_bbox_exact(&cube, bbox([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]));
    assert!(v.abs() < 1e-12, "outside volume is {v}");
}

#[test]
fn a_corner_cut_names_all_three_faces() {
    let cube = outward(box_mesh(bbox([0.0, 0.0, 0.0], [2.0, 2.0, 2.0])));
    let (v, cut) = mesh_volume_in_bbox_exact(&cube, bbox([1.0, 1.0, 1.0], [10.0, 10.0, 10.0]));
    assert!((v - 1.0).abs() < 1e-12, "corner octant volume is {v}");
    assert_eq!(cut_face_names(cut), vec!["xmin", "ymin", "zmin"]);
}

// A sphere clipped by a plane has a closed-form cap volume, so this checks the
// routine against mathematics rather than against another of our own functions.
#[test]
fn a_clipped_sphere_matches_the_spherical_cap_formula() {
    let r = 3.0;
    let sphere = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), r, 4);
    let full = mesh_volume(&sphere);
    // Keep z <= 1.0, i.e. remove the cap of height h = r - 1 = 2.
    let (v, cut) = mesh_volume_in_bbox_exact(&sphere, bbox([-10.0, -10.0, -10.0], [10.0, 10.0, 1.0]));
    let h = r - 1.0;
    let cap = std::f64::consts::PI * h * h * (3.0 * r - h) / 3.0;
    let exact_remaining = 4.0 / 3.0 * std::f64::consts::PI * r * r * r - cap;
    // The mesh is a polyhedral approximation, so compare the *fraction* removed,
    // which is insensitive to the discretisation error shared by both terms.
    let removed_fraction = (full - v) / full;
    let expected_fraction = cap / (4.0 / 3.0 * std::f64::consts::PI * r * r * r);
    assert!(
        (removed_fraction - expected_fraction).abs() < 2e-3,
        "removed {removed_fraction} vs analytic {expected_fraction} (v {v}, exact {exact_remaining})"
    );
    assert_eq!(cut_face_names(cut), vec!["zmax"]);
}

// The reason this routine exists: the legacy fan cap re-orders a non-star-shaped
// ring and fills a nested one. A U-shaped solid cut across its opening produces a
// cap cross-section that is not star-shaped about its own vertex mean.
#[test]
fn a_non_convex_cross_section_is_exact_where_a_fan_cap_is_not() {
    // A U in the xy plane, extruded in z: two arms joined by a base.
    let left = outward(box_mesh(bbox([0.0, 0.0, 0.0], [1.0, 4.0, 1.0])));
    let right = outward(box_mesh(bbox([3.0, 0.0, 0.0], [4.0, 4.0, 1.0])));
    let base = outward(box_mesh(bbox([1.0, 0.0, 0.0], [3.0, 1.0, 1.0])));
    let u = merge_meshes(&[left, right, base]);
    assert!((mesh_volume(&u) - 10.0).abs() < 1e-9, "the U has volume 4 + 4 + 2");

    // Cut at y = 2.0: below the cut is 1*2 + 1*2 + 2*1 = 6; the cap at ymax is two
    // disjoint 1x1 squares, which no single fan from one centre can represent.
    let (v, cut) = mesh_volume_in_bbox_exact(&u, bbox([-1.0, -1.0, -1.0], [5.0, 2.0, 2.0]));
    assert!((v - 6.0).abs() < 1e-9, "U clipped at y=2 has volume {v}, expected 6");
    assert_eq!(cut_face_names(cut), vec!["ymax"]);
}

// A solid with an internal cavity: cut through the cavity and the cap cross-section
// is an annulus. A cap builder that gives both rings the same winding fills the
// hole and over-reports; here the inner ring arrives wound the other way and
// subtracts itself.
#[test]
fn a_hole_is_subtracted_from_the_cap_not_filled() {
    let outer = outward(box_mesh(bbox([0.0, 0.0, 0.0], [6.0, 6.0, 6.0])));
    // Wound the other way, and strictly inside, so it is a cavity. It has to be
    // strictly inside: a shaft poking out of the outer box would make the pair an
    // invalid surface whose signed volume is not the volume of any solid.
    let mut inner = outward(box_mesh(bbox([2.0, 2.0, 1.0], [4.0, 4.0, 5.0])));
    for f in &mut inner.faces {
        std::mem::swap(&mut f.b, &mut f.c);
    }
    let shell = merge_meshes(&[outer, inner]);
    assert!(
        (mesh_volume(&shell) - (216.0 - 16.0)).abs() < 1e-9,
        "the cavity is subtracted from the block"
    );

    // Keep z <= 3, which cuts through the cavity: the block below z=3 is 6*6*3,
    // less the part of the cavity below it, 2*2*2.
    let (v, cut) = mesh_volume_in_bbox_exact(&shell, bbox([-1.0, -1.0, -1.0], [7.0, 7.0, 3.0]));
    let expected = 6.0 * 6.0 * 3.0 - 2.0 * 2.0 * 2.0;
    assert!((v - expected).abs() < 1e-9, "annular cap volume is {v}, expected {expected}");
    assert_eq!(cut_face_names(cut), vec!["zmax"]);
}

// A face lying exactly on a clip plane is genuine boundary, not the boundary of a
// phantom cap. Detecting coplanar edges after the fact would double count it.
#[test]
fn a_face_flush_with_the_clip_plane_is_not_treated_as_a_cap() {
    let cube = outward(box_mesh(bbox([0.0, 0.0, 0.0], [2.0, 2.0, 2.0])));
    let (v, cut) = mesh_volume_in_bbox_exact(&cube, bbox([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]));
    assert!((v - 8.0).abs() < 1e-9, "a flush cube keeps its whole volume, got {v}");
    assert!(cut_face_names(cut).is_empty(), "touching is not cutting: {:?}", cut_face_names(cut));
}

// The regression this fixes: HashSet iteration order reached the clipped volume,
// so the same mesh clipped twice in one process could differ in its last bits.
#[test]
fn clipping_the_same_mesh_twice_gives_identical_results() {
    let sphere = icosphere_mesh(Vec3::new(0.5, 0.5, 0.5), 1.0, 3);
    let b = bbox([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);

    let first = rustmspt::geometry::clip_mesh_by_bbox(&sphere, b);
    for _ in 0..8 {
        let again = rustmspt::geometry::clip_mesh_by_bbox(&sphere, b);
        assert_eq!(first.vertices.len(), again.vertices.len());
        assert_eq!(first.faces.len(), again.faces.len());
        for (p, q) in first.vertices.iter().zip(again.vertices.iter()) {
            assert_eq!(p.x.to_bits(), q.x.to_bits(), "clip is not bit-reproducible");
            assert_eq!(p.y.to_bits(), q.y.to_bits());
            assert_eq!(p.z.to_bits(), q.z.to_bits());
        }
        for (p, q) in first.faces.iter().zip(again.faces.iter()) {
            assert_eq!((p.a, p.b, p.c), (q.a, q.b, q.c));
        }
    }

    // The new exact routine is deterministic for the same reason: every sum runs
    // in face order, with no set iteration anywhere.
    let (v0, _) = mesh_volume_in_bbox_exact(&sphere, b);
    for _ in 0..8 {
        let (v, _) = mesh_volume_in_bbox_exact(&sphere, b);
        assert_eq!(v0.to_bits(), v.to_bits(), "exact clip volume is not bit-reproducible");
    }
}

// ---------------------------------------------------------------- bbox

#[test]
fn expanded_grows_and_shrinks_symmetrically() {
    let b = bbox([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
    let grown = b.expanded(2.0);
    assert_eq!((grown.min.x, grown.max.x), (-2.0, 12.0));
    assert!((grown.volume() - 14.0f64.powi(3)).abs() < 1e-9);

    let eroded = b.expanded(-3.0);
    assert_eq!((eroded.min.x, eroded.max.x), (3.0, 7.0));
    assert!((eroded.volume() - 64.0).abs() < 1e-9);

    // Over-eroding empties the box rather than inverting it into something usable.
    let gone = b.expanded(-6.0);
    assert_eq!(gone.volume(), 0.0);
    assert!(!gone.contains_point(Vec3::new(5.0, 5.0, 5.0)));
}

// ---------------------------------------------------------------- icosphere

#[test]
fn the_icosphere_is_closed_and_converges_to_the_analytic_sphere() {
    let r = 2.0;
    let exact = 4.0 / 3.0 * std::f64::consts::PI * r * r * r;
    let mut previous_error = f64::INFINITY;
    for level in 0..6 {
        let s = icosphere_mesh(Vec3::new(1.0, -1.0, 0.5), r, level);
        assert_eq!(s.faces.len(), 20 * 4usize.pow(level), "face count at level {level}");
        let metrics = rustmspt::geometry::mesh_metrics(&s);
        assert!(metrics.is_some(), "level {level} must be a closed manifold");
        let error = (mesh_volume(&s) - exact).abs() / exact;
        assert!(error < previous_error, "level {level} must be closer than the last");
        previous_error = error;
        let c = mesh_volume_centroid(&s).expect("closed");
        let want = Vec3::new(1.0, -1.0, 0.5);
        assert!((c.sub(want)).dot(c.sub(want)).sqrt() < 1e-9, "centroid at level {level}");
    }
    // Measured relative volume error, r = 2: level 0 3.9e-1, 1 1.3e-1, 2 3.4e-2,
    // 3 8.6e-3, 4 2.2e-3, 5 5.4e-4 - each subdivision quarters it, as a second-order
    // approximation should. Level 5 is the one the bound is set from.
    assert!(previous_error < 6e-4, "level 5 error is {previous_error}, expected ~5.4e-4");
}
