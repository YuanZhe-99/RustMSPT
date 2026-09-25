//! What "two meshes collide" has to mean.
//!
//! Surface intersection alone is not it. Two closed surfaces, one wholly inside
//! the other, never cross, and the inner one is a comfortable distance from the
//! outer one measured across the gap between them. Every test here exists because
//! a packing engine that asks only `intersection_test` accepts that arrangement
//! and reports a volume fraction that counts the same space twice.

use rustmspt::geometry::{
    box_mesh, icosphere_mesh, mesh_bbox, mesh_closer_than_prepared, mesh_collision_exact, mesh_collision_exact_prepared,
    mesh_distance_exact_prepared, mesh_solids_nested_prepared, mesh_surfaces_intersect_prepared,
    merge_meshes, point_inside_mesh, to_parry_trimesh, trimesh_contains_point,
};
use rustmspt::types::{BoundingBox, Mesh, Vec3};

/// The four questions, each asked through the prepared entry points the engines
/// actually call. Nothing is cached: a test that reused a hierarchy could hide a
/// disagreement between how it was built and how it is queried.
fn intersects(a: &Mesh, b: &Mesh) -> bool {
    mesh_surfaces_intersect_prepared(
        mesh_bbox(a),
        to_parry_trimesh(a).as_ref(),
        mesh_bbox(b),
        to_parry_trimesh(b).as_ref(),
    )
}

fn nested(a: &Mesh, b: &Mesh) -> bool {
    mesh_solids_nested_prepared(
        mesh_bbox(a),
        to_parry_trimesh(a).as_ref(),
        mesh_bbox(b),
        to_parry_trimesh(b).as_ref(),
    )
}

fn collides(a: &Mesh, b: &Mesh) -> bool {
    mesh_collision_exact_prepared(
        mesh_bbox(a),
        to_parry_trimesh(a).as_ref(),
        mesh_bbox(b),
        to_parry_trimesh(b).as_ref(),
    )
}

fn distance(a: &Mesh, b: &Mesh) -> f64 {
    mesh_distance_exact_prepared(
        mesh_bbox(a),
        to_parry_trimesh(a).as_ref(),
        mesh_bbox(b),
        to_parry_trimesh(b).as_ref(),
    )
}

// ---------------------------------------------------------------- the defect

#[test]
fn one_sphere_inside_another_is_a_collision_however_it_is_asked() {
    let inner = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 2);
    let outer = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 3.0, 2);
    let (a, b) = (&inner, &outer);

    // The surfaces really do not cross - that is the whole problem.
    assert!(!intersects(a, b), "concentric surfaces must not intersect");

    // Nesting is symmetric: neither argument order may miss it.
    assert!(nested(a, b), "the small sphere is inside the large one");
    assert!(nested(b, a), "asked the other way round, still nested");

    // And the question every engine actually asks must answer yes.
    assert!(collides(a, b), "one solid inside another is a collision");
    assert!(
        mesh_collision_exact(&inner, &outer),
        "the convenience wrapper inherits the same answer"
    );

    // A nested pair is zero apart, not "the gap between the two surfaces". A
    // min_neighbor_distance test that saw 2.0 here would accept the placement.
    assert_eq!(
        distance(a, b),
        0.0,
        "a nested pair has no clearance to report"
    );
}

#[test]
fn two_disjoint_spheres_stay_disjoint() {
    let left = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 3);
    let right = icosphere_mesh(Vec3::new(5.0, 0.0, 0.0), 1.0, 3);
    let (a, b) = (&left, &right);

    assert!(!intersects(a, b));
    assert!(!nested(a, b));
    assert!(!nested(b, a));
    assert!(!collides(a, b));

    // Level-3 icospheres sit slightly inside the analytic sphere, so the measured
    // gap is a little wider than 5 - 1 - 1 = 3.
    let d = distance(a, b);
    assert!(
        (d - 3.0).abs() < 0.02,
        "the surface gap should be about 3.0, measured {d}"
    );
}

#[test]
fn two_overlapping_spheres_intersect_and_are_not_nested() {
    let left = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 2);
    let right = icosphere_mesh(Vec3::new(1.2, 0.0, 0.0), 1.0, 2);
    let (a, b) = (&left, &right);

    assert!(intersects(a, b));
    assert!(!nested(a, b), "crossing surfaces are not nesting");
    assert!(!nested(b, a));
    assert!(collides(a, b));
    assert_eq!(distance(a, b), 0.0);
}

#[test]
fn a_particle_in_a_hollow_particles_cavity_is_not_nested_in_its_solid() {
    // A hollow shell: an outer sphere with an inverted inner one, so the solid is
    // the rind between r=2 and r=3 and the middle is empty space. A small particle
    // parked in the cavity has a bounding box wholly inside the shell's, which is
    // exactly the case a bbox-containment test would call nested. It is not: the
    // two solids are disjoint, and the placement is legal.
    let mut inverted = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 2.0, 2);
    for f in &mut inverted.faces {
        std::mem::swap(&mut f.b, &mut f.c);
    }
    let hollow = merge_meshes(&[icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 3.0, 2), inverted]);
    let guest = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 0.5, 2);

    let (a, b) = (&guest, &hollow);
    assert!(
        mesh_bbox(a).unwrap().min.x > mesh_bbox(b).unwrap().min.x,
        "the guest's box is inside the shell's, which is the trap"
    );
    assert!(!intersects(a, b));
    assert!(
        !nested(a, b),
        "parity, not bounding boxes: the cavity is outside the shell's solid"
    );
    assert!(!collides(a, b));
}

#[test]
fn a_particle_buried_in_a_hollow_particles_rind_is_nested() {
    // The same hollow shell, but the guest sits inside the rind itself. Parity
    // says inside, and it must, or a particle could be buried in another's wall.
    let mut inverted = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 6.0, 2);
    for f in &mut inverted.faces {
        std::mem::swap(&mut f.b, &mut f.c);
    }
    let hollow = merge_meshes(&[icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 12.0, 2), inverted]);
    let guest = icosphere_mesh(Vec3::new(9.0, 0.0, 0.0), 0.5, 2);

    let (a, b) = (&guest, &hollow);
    assert!(!intersects(a, b), "the guest fits between the two shells");
    assert!(nested(a, b), "inside the rind is inside the solid");
    assert!(collides(a, b));
}

// ------------------------------------------------- the shared containment test

#[test]
fn the_hierarchy_point_test_agrees_with_the_scanning_one() {
    // trimesh_contains_point and s2::point_inside_mesh are two implementations of
    // one question, and they share a ray direction and a hit tolerance so that
    // they can never disagree. Two that disagreed would give no way to tell, from
    // inside either, which one was wrong.
    let mesh = merge_meshes(&[
        box_mesh(BoundingBox {
            min: Vec3::new(-2.0, -1.0, -1.0),
            max: Vec3::new(2.0, 1.0, 1.0),
        }),
        icosphere_mesh(Vec3::new(4.0, 0.0, 0.0), 1.5, 2),
    ]);
    let shape = to_parry_trimesh(&mesh).expect("indexable");

    // A fixed lattice plus a fixed pseudo-random jitter: deterministic, and not
    // aligned with the geometry.
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 11) as f64) * (1.0 / 9007199254740992.0)
    };
    let mut checked = 0usize;
    let mut inside = 0usize;
    for _ in 0..2000 {
        let p = Vec3::new(
            -3.0 + next() * 10.0,
            -2.0 + next() * 4.0,
            -2.0 + next() * 4.0,
        );
        let a = trimesh_contains_point(&shape, p);
        let b = point_inside_mesh(&mesh, p);
        assert_eq!(a, b, "the two containment tests disagree at {p:?}");
        checked += 1;
        if a {
            inside += 1;
        }
    }
    assert_eq!(checked, 2000);
    assert!(
        inside > 200,
        "only {inside} points landed inside - the sample says nothing"
    );
}

// ------------------------------------------------- the screened gap question

/// `mesh_closer_than_prepared` bounds the distance from triangle pairs before
/// measuring it. It must answer exactly `distance < gap` everywhere: at a gap
/// equal to the measured distance and one ulp above it (inside the band where
/// the screen must defer to the exact distance), and 1e-5 either side of it
/// (just outside that band, where the screen answers on its own).
#[test]
fn the_screened_gap_test_answers_exactly_what_the_distance_answers() {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let base = icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 2);
    let mut checked = 0usize;
    let mut closer = 0usize;
    for _ in 0..120 {
        let dir = Vec3::new(next() - 0.5, next() - 0.5, next() - 0.5);
        let len = dir.dot(dir).sqrt().max(1e-9);
        let offset = 1.4 + 2.0 * next();
        let centre = Vec3::new(dir.x / len * offset, dir.y / len * offset, dir.z / len * offset);
        let sx = 0.4 + next();
        let other = Mesh {
            vertices: base
                .vertices
                .iter()
                .map(|v| Vec3::new(centre.x + v.x * sx, centre.y + v.y * 0.7, centre.z + v.z * 0.5))
                .collect(),
            faces: base.faces.clone(),
        };
        let (ab, bb) = (mesh_bbox(&base), mesh_bbox(&other));
        let (at, bt) = (to_parry_trimesh(&base), to_parry_trimesh(&other));
        let d = mesh_distance_exact_prepared(ab, at.as_ref(), bb, bt.as_ref());
        let apart = !mesh_collision_exact_prepared(ab, at.as_ref(), bb, bt.as_ref());
        for gap in [0.0, 0.05, 0.3, 1.0, d, f64::from_bits(d.to_bits() + 1), d * 0.999_999, d * 1.000_001, d * 0.999_99, d * 1.000_01] {
            let expected = d < gap;
            assert_eq!(mesh_closer_than_prepared(ab, at.as_ref(), bb, bt.as_ref(), gap, false), expected, "d {d} gap {gap}");
            if apart {
                assert_eq!(mesh_closer_than_prepared(ab, at.as_ref(), bb, bt.as_ref(), gap, true), expected, "d {d} gap {gap}");
            }
            checked += 1;
            closer += expected as usize;
        }
    }
    assert!(closer > 100 && closer + 100 < checked, "both answers exercised: {closer} of {checked}");
}
