//! G5-1 acceptance tests: exact parity classification and its degeneracy
//! handling, the winding-number path for defective solids, the sheet guard,
//! `resolve()`'s frozen truth table, ownership-record seeding, the active-patch
//! filter, and `s06_classified`.

use rustmspt::config::meshgen::{CoincidencePolicy, RepairLevel};
use rustmspt::meshgen::classify::{
    classified_to_doc, classify_lattice, resolve, Classification, ClassifyOptions, OwnershipRecord,
    Provenance, Side,
};
use rustmspt::meshgen::lattice::{balance_octree, build_lattice, Lattice, LatticeOptions};
use rustmspt::meshgen::sizing::{
    build_sizing_field, SizingLookup, SizingOptions,
};
use rustmspt::meshgen::snapshot::{SnapshotMeta, Stage};
use rustmspt::meshgen::{
    arrange_surface, build_scene, clip_arranged_to_box, condition_surface, detect_features,
    orient3d, rebuild_topology, stamp_metadata, verify, ArrangeComponent, ArrangeOptions,
    ArrangedSurface, ColorMode, RebuiltTopology, SceneSpec, Severity, VerifyGates,
};
use rustmspt::types::{Mesh, Triangle, Vec3};
use smallvec::SmallVec;
use std::collections::BTreeMap;

const EPS: f64 = 1.0e-4;

// AI-FUNC-SUMMARY: Interpolate between two points; returns Vec3; side effects: none.
fn lerp(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    a.add(b.sub(a).scale(t))
}

// AI-FUNC-SUMMARY: Subdivide a quad into an n x n grid of triangles appended to a mesh; side effects: mutates the mesh.
fn push_quad(mesh: &mut Mesh, corners: [Vec3; 4], n: usize) {
    let at = |u: f64, v: f64| -> Vec3 {
        let top = lerp(corners[0], corners[1], u);
        let bottom = lerp(corners[3], corners[2], u);
        lerp(top, bottom, v)
    };
    for i in 0..n {
        for j in 0..n {
            let (u0, u1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let (v0, v1) = (j as f64 / n as f64, (j + 1) as f64 / n as f64);
            let base = mesh.vertices.len();
            mesh.vertices.push(at(u0, v0));
            mesh.vertices.push(at(u1, v0));
            mesh.vertices.push(at(u1, v1));
            mesh.vertices.push(at(u0, v1));
            mesh.faces.push(Triangle {
                a: base,
                b: base + 1,
                c: base + 2,
            });
            mesh.faces.push(Triangle {
                a: base,
                b: base + 2,
                c: base + 3,
            });
        }
    }
}

// AI-FUNC-SUMMARY: Closed axis-aligned box, outward winding; returns Mesh; side effects: none.
fn box_mesh(min: Vec3, max: Vec3, n: usize) -> Mesh {
    let v = |i: usize| -> Vec3 {
        Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        )
    };
    let quads: [[usize; 4]; 6] = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let mut mesh = Mesh::empty();
    for quad in quads {
        push_quad(&mut mesh, [v(quad[0]), v(quad[1]), v(quad[2]), v(quad[3])], n);
    }
    mesh
}

// AI-FUNC-SUMMARY: A closed UV sphere with outward winding; returns Mesh; side effects: none.
fn sphere_mesh(center: Vec3, radius: f64, bands: usize) -> Mesh {
    let mut mesh = Mesh::empty();
    let at = |lat: usize, lon: usize| -> Vec3 {
        let theta = std::f64::consts::PI * lat as f64 / bands as f64;
        let phi = 2.0 * std::f64::consts::PI * lon as f64 / (2 * bands) as f64;
        center.add(Vec3::new(
            radius * theta.sin() * phi.cos(),
            radius * theta.sin() * phi.sin(),
            radius * theta.cos(),
        ))
    };
    for lat in 0..bands {
        for lon in 0..2 * bands {
            let base = mesh.vertices.len();
            for point in [
                at(lat, lon),
                at(lat + 1, lon),
                at(lat + 1, lon + 1),
                at(lat, lon + 1),
            ] {
                mesh.vertices.push(point);
            }
            mesh.faces.push(Triangle {
                a: base,
                b: base + 1,
                c: base + 2,
            });
            mesh.faces.push(Triangle {
                a: base,
                b: base + 2,
                c: base + 3,
            });
        }
    }
    mesh
}

/// Everything S6 needs, built by running S0..S5 for real.
struct Scene {
    surface: ArrangedSurface,
    topo: RebuiltTopology,
    lattice: Lattice,
    domain_min: Vec3,
    domain_max: Vec3,
}

// AI-FUNC-SUMMARY:
// Purpose: Run the whole implemented pipeline up to the lattice, so S6 is tested against real
//   arrangement output rather than a hand-built fixture.
// Inputs: the input meshes, their kinds and priorities, and the target element size.
// Returns: Scene.
// Side effects: None.
fn scene(meshes: &[Mesh], kinds: &[u8], priorities: &[u32], h_max: f64) -> Scene {
    let domain_min = Vec3::new(0.0, 0.0, 0.0);
    let domain_max = Vec3::new(1.0, 1.0, 1.0);
    let components: Vec<ArrangeComponent> = kinds
        .iter()
        .zip(priorities.iter())
        .enumerate()
        .map(|(index, (kind, priority))| ArrangeComponent {
            x: index as i32 + 1,
            priority: *priority,
            kind: *kind,
            closed: *kind == 0,
        })
        .collect();
    let conditioned = condition_surface(meshes, EPS, RepairLevel::Conservative).unwrap();
    let features = detect_features(&conditioned, 45.0);
    let arranged = arrange_surface(
        &conditioned,
        &features,
        &ArrangeOptions {
            domain_min,
            domain_max,
            eps: EPS,
            coincidence: CoincidencePolicy::Merge,
            components,
        },
    )
    .unwrap();
    let mut clipped = clip_arranged_to_box(&arranged, domain_min, domain_max, EPS).unwrap();
    let topo = rebuild_topology(&clipped);
    clipped.components = topo.components.clone();

    let sizing = SizingOptions {
        domain_min,
        domain_max,
        h_max,
        h_min: h_max / 4.0,
        eps: EPS,
        ..Default::default()
    };
    let lookup = SizingLookup::build(Vec::new(), &sizing);
    let field = build_sizing_field(&lookup, &sizing);
    let (balanced, _) = balance_octree(&field);
    let lattice = build_lattice(&balanced, &LatticeOptions::default()).unwrap();
    Scene {
        surface: clipped,
        topo,
        lattice,
        domain_min,
        domain_max,
    }
}

// AI-FUNC-SUMMARY: Run S6 on a scene; returns Classification; side effects: none.
fn classify(scene: &Scene) -> Classification {
    classify_lattice(
        &scene.lattice,
        &scene.surface,
        &scene.topo,
        &ClassifyOptions {
            domain_min: scene.domain_min,
            domain_max: scene.domain_max,
        },
    )
}

// AI-FUNC-SUMMARY:
// Purpose: Total volume of the tets whose resolved key is exactly `{x}`.
// Returns: the volume in the same units as the lattice.
// Side effects: None.
// Notes: A tet still straddling the surface is *excluded*, so this under-counts by up to one
//   element layer - which is why the tests compare it against a band, not a value.
fn volume_of_key(scene: &Scene, classification: &Classification, key: &[i32]) -> f64 {
    let slot = classification
        .region_sets
        .iter()
        .position(|set| set.as_slice() == key);
    let Some(slot) = slot else {
        return 0.0;
    };
    let mut total = 0.0;
    for (tet, region) in scene.lattice.tets.iter().zip(classification.region_key.iter()) {
        if *region != slot as i32 {
            continue;
        }
        let a = scene.lattice.nodes[tet[0] as usize];
        let u = scene.lattice.nodes[tet[1] as usize].sub(a);
        let v = scene.lattice.nodes[tet[2] as usize].sub(a);
        let w = scene.lattice.nodes[tet[3] as usize].sub(a);
        total += u.cross(v).dot(w) / 6.0;
    }
    total
}

// ---------------------------------------------------------------------------
// resolve() - the frozen truth table
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: Build a record from (component, side) pairs; returns OwnershipRecord; side effects: none.
fn record(entries: &[(i32, Side)]) -> OwnershipRecord {
    OwnershipRecord {
        entries: SmallVec::from_slice(entries),
        provenance: Provenance::Lattice,
    }
}

#[test]
fn resolve_reproduces_the_frozen_truth_table() {
    // SPEC_meshgen_geometry §9.2, every row that a record can express.
    let priority: BTreeMap<i32, u32> = [(1, 1), (2, 2), (3, 1), (4, 0), (5, 3), (6, 3), (7, 2)]
        .into_iter()
        .collect();
    let cases: [(&[(i32, Side)], &[i32], &str); 9] = [
        (&[], &[0], "row 1: empty S is background"),
        (&[(1, Side::Inside)], &[1], "row 2: single ownership"),
        (
            &[(1, Side::Inside), (2, Side::Inside)],
            &[1],
            "row 3: R-A4, smaller Y wins",
        ),
        (
            &[(1, Side::Inside), (3, Side::Inside)],
            &[1, 3],
            "row 4: R-A3, same priority keeps every X",
        ),
        (
            &[(1, Side::Inside), (3, Side::Inside), (2, Side::Inside)],
            &[1, 3],
            "row 5: R-A3 and R-A4 together",
        ),
        (
            &[(4, Side::Inside), (1, Side::Inside)],
            &[4],
            "row 6: void inside particle",
        ),
        (
            &[(7, Side::Inside), (5, Side::Inside), (6, Side::Inside)],
            &[7],
            "row 7: unique minimum priority",
        ),
        (
            &[(5, Side::Inside), (6, Side::Inside), (7, Side::Inside)],
            &[7],
            "row 8: the minimum need not be listed first",
        ),
        (
            &[
                (1, Side::Inside),
                (3, Side::Inside),
                (2, Side::Inside),
                (7, Side::Inside),
            ],
            &[1, 3],
            "row 9: ties at the minimum only",
        ),
    ];
    for (entries, expected, why) in cases {
        assert_eq!(resolve(&record(entries), &priority), expected, "{why}");
    }
}

#[test]
fn an_outside_or_ambiguous_entry_never_claims_the_tet() {
    let priority: BTreeMap<i32, u32> = [(1, 1), (2, 2)].into_iter().collect();
    assert_eq!(resolve(&record(&[(1, Side::Outside)]), &priority), vec![0]);
    // Ambiguous means "the cut will decide", so it cannot enter S either - and a
    // record that still carries one is what S10 must refuse (§9.2 row 11).
    let straddling = record(&[(1, Side::Ambiguous)]);
    assert_eq!(resolve(&straddling, &priority), vec![0]);
    assert!(straddling.is_ambiguous());
    assert!(!record(&[(1, Side::Inside)]).is_ambiguous());
    assert_eq!(straddling.side_of(1), Side::Ambiguous);
    assert_eq!(straddling.side_of(2), Side::Outside, "absent is outside");
}

// ---------------------------------------------------------------------------
// Parity classification
// ---------------------------------------------------------------------------

#[test]
fn a_single_box_is_classified_by_volume_within_one_element_layer() {
    // The strongest available correctness check: the classified volume must match
    // the analytic volume, up to the surface layer that is still straddling.
    let lo = Vec3::new(0.25, 0.25, 0.25);
    let hi = Vec3::new(0.70, 0.65, 0.55);
    let h = 0.05;
    let scene = scene(&[box_mesh(lo, hi, 2)], &[0], &[0], h);
    let classification = classify(&scene);
    assert_eq!(classification.solid_components, vec![1]);

    let inside = volume_of_key(&scene, &classification, &[1]);
    let extent = hi.sub(lo);
    let exact = extent.x * extent.y * extent.z;
    // Straddling cells are excluded, so `inside` is a lower bound; the shortfall is
    // bounded by the surface area times the element size.
    let area = 2.0 * (extent.x * extent.y + extent.y * extent.z + extent.z * extent.x);
    assert!(
        inside <= exact + 1.0e-12,
        "classified {inside} exceeds the exact volume {exact}"
    );
    assert!(
        inside > exact - 2.0 * area * h,
        "classified {inside} is far below the exact volume {exact}"
    );
}

#[test]
fn a_sphere_is_classified_by_volume() {
    let radius = 0.2;
    let centre = Vec3::new(0.5, 0.5, 0.5);
    let h = 0.05;
    let scene = scene(&[sphere_mesh(centre, radius, 24)], &[0], &[0], h);
    let classification = classify(&scene);

    let inside = volume_of_key(&scene, &classification, &[1]);
    let exact = 4.0 / 3.0 * std::f64::consts::PI * radius.powi(3);
    let area = 4.0 * std::f64::consts::PI * radius * radius;
    assert!(inside <= exact + 1.0e-12);
    assert!(
        inside > exact - 2.0 * area * h,
        "classified {inside} against exact {exact}"
    );
}

#[test]
fn every_decision_resolves_exactly_without_falling_back() {
    // Axis-aligned input on a lattice grid puts many vertices *exactly* on a face
    // plane. That is the common case, not the corner case, and no re-shoot can fix
    // it - the plane test does not depend on the ray direction. Before the
    // perturbation rule 1.1% of decisions on this kind of scene ended at the
    // winding number; the whole point is that none should.
    // The bounds are dyadic, so the box's faces land *exactly* on lattice planes -
    // which is what puts vertices exactly on a face and makes the fixture test the
    // thing it claims to.
    let lo = Vec3::new(0.25, 0.25, 0.25);
    let hi = Vec3::new(0.625, 0.625, 0.5);
    let scene = scene(
        &[
            box_mesh(lo, hi, 2),
            box_mesh(Vec3::new(0.375, 0.375, 0.5), Vec3::new(0.75, 0.75, 0.75), 2),
        ],
        &[0, 0],
        &[0, 1],
        0.05,
    );

    // Assert the premise before asserting the conclusion: some lattice vertex must
    // really lie on a face plane, or this proves nothing.
    let on_plane = scene.lattice.nodes.iter().any(|node| {
        scene.surface.faces.iter().any(|face| {
            orient3d(
                scene.surface.vertices[face.nodes[0]],
                scene.surface.vertices[face.nodes[1]],
                scene.surface.vertices[face.nodes[2]],
                *node,
            ) == 0.0
        })
    });
    assert!(
        on_plane,
        "no lattice vertex lies exactly on a face plane, so the fixture is not degenerate"
    );

    let classification = classify(&scene);
    assert_eq!(classification.stats.n_gwn_fallback, 0, "winding-number fallbacks");
    assert_eq!(classification.stats.n_defective_gwn, 0, "no component is defective here");
    assert_eq!(classification.stats.n_reshoots, 0, "re-shoots");
    assert_eq!(
        classification.stats.n_first_ray,
        classification.stats.n_vertices * 2
    );
    assert!(classification.warnings.is_empty());
}

#[test]
fn a_vertex_outside_everything_is_background() {
    let scene = scene(
        &[box_mesh(Vec3::new(0.4, 0.4, 0.4), Vec3::new(0.6, 0.6, 0.6), 2)],
        &[0],
        &[0],
        0.05,
    );
    let classification = classify(&scene);
    // A corner of the domain is nowhere near the box.
    let corner = scene
        .lattice
        .nodes
        .iter()
        .position(|p| p.x < 1.0e-12 && p.y < 1.0e-12 && p.z < 1.0e-12)
        .unwrap();
    assert!(!classification.is_inside(corner, 0));
    // And the overwhelming majority of the lattice is background.
    assert!(classification.stats.n_tets_background > classification.stats.n_tets_owned);
}

// ---------------------------------------------------------------------------
// Priorities, sheets, and the active-patch filter
// ---------------------------------------------------------------------------

#[test]
fn a_higher_priority_solid_replaces_a_lower_one_in_the_overlap() {
    // PLAN §5.5 row 1: A(Y=1) overlapping B(Y=2) gives A in the overlap.
    let scene = scene(
        &[
            box_mesh(Vec3::new(0.25, 0.25, 0.25), Vec3::new(0.55, 0.55, 0.55), 2),
            box_mesh(Vec3::new(0.45, 0.35, 0.35), Vec3::new(0.75, 0.65, 0.65), 2),
        ],
        &[0, 0],
        &[1, 2],
        0.04,
    );
    let classification = classify(&scene);
    let keys: Vec<Vec<i32>> = classification.region_sets.clone();
    assert!(keys.contains(&vec![0]));
    assert!(keys.contains(&vec![1]));
    assert!(keys.contains(&vec![2]));
    // The overlap belongs to component 1 alone; a `{1, 2}` key would mean the
    // priority rule did not fire.
    assert!(
        !keys.contains(&vec![1, 2]),
        "the overlap kept both components: {keys:?}"
    );

    // And component 2's volume is short by the overlap.
    let overlap = 0.10 * 0.20 * 0.20;
    let two = volume_of_key(&scene, &classification, &[2]);
    let whole_two = 0.30 * 0.30 * 0.30;
    assert!(
        two < whole_two - 0.5 * overlap,
        "component 2 kept {two} of {whole_two}, so the overlap was not replaced"
    );
}

#[test]
fn equal_priority_overlaps_keep_both_components() {
    // PLAN §5.5 row 2 / SPEC §9.2 row 4: A and A' both Y=1 keeps `{A, A'}`.
    let scene = scene(
        &[
            box_mesh(Vec3::new(0.25, 0.25, 0.25), Vec3::new(0.55, 0.55, 0.55), 2),
            box_mesh(Vec3::new(0.45, 0.35, 0.35), Vec3::new(0.75, 0.65, 0.65), 2),
        ],
        &[0, 0],
        &[1, 1],
        0.04,
    );
    let classification = classify(&scene);
    assert!(
        classification.region_sets.contains(&vec![1, 2]),
        "same-priority overlap lost a component: {:?}",
        classification.region_sets
    );
}

#[test]
fn a_sheet_never_claims_volume() {
    // The hard guard of PLAN §10.4 / SPEC §9.1 row 10: a sheet is not a solid, so
    // it is not even a candidate for ownership, whatever any winding number says.
    let mut sheet = Mesh::empty();
    push_quad(
        &mut sheet,
        [
            Vec3::new(0.30, 0.30, 0.50),
            Vec3::new(0.70, 0.30, 0.50),
            Vec3::new(0.70, 0.70, 0.50),
            Vec3::new(0.30, 0.70, 0.50),
        ],
        2,
    );
    let scene = scene(
        &[
            box_mesh(Vec3::new(0.20, 0.20, 0.20), Vec3::new(0.45, 0.45, 0.45), 2),
            sheet,
        ],
        &[0, 1],
        &[0, 1],
        0.05,
    );
    let classification = classify(&scene);
    assert_eq!(
        classification.solid_components,
        vec![1],
        "the sheet entered the solid set"
    );
    assert!(classification.stats.n_sheet_components >= 1);
    assert!(classification
        .region_sets
        .iter()
        .all(|key| !key.contains(&2)));
    // A sheet is always an active patch - it is a feature in its own right.
    assert!(classification.active_face.iter().any(|active| *active));
}

#[test]
fn a_patch_buried_in_a_higher_priority_solid_is_inactive() {
    // PLAN §5.2: the buried surface separates identical labels, so refinement,
    // snap and cut all skip it.
    let scene = scene(
        &[
            box_mesh(Vec3::new(0.20, 0.20, 0.20), Vec3::new(0.80, 0.80, 0.80), 2),
            box_mesh(Vec3::new(0.40, 0.40, 0.40), Vec3::new(0.60, 0.60, 0.60), 2),
        ],
        &[0, 0],
        &[0, 1],
        0.05,
    );
    let classification = classify(&scene);
    // The inner box (priority 1) is entirely inside the outer one (priority 0),
    // so every one of its faces is inactive...
    assert!(
        classification.stats.n_inactive_faces > 0,
        "nothing was deactivated"
    );
    for (face, active) in scene.surface.faces.iter().zip(classification.active_face.iter()) {
        if face.components.contains(&2) && !face.components.contains(&1) {
            assert!(!active, "a buried inner face stayed active");
        }
        if face.components.contains(&1) {
            assert!(*active, "the outer box's own faces must stay active");
        }
    }
    // ...and the inner box claims no volume of its own, because the outer box wins.
    assert!(classification
        .region_sets
        .iter()
        .all(|key| key.as_slice() != [2]));
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

#[test]
fn records_are_seeded_from_the_lattice_and_mark_straddling_cells() {
    let scene = scene(
        &[box_mesh(Vec3::new(0.25, 0.25, 0.25), Vec3::new(0.70, 0.65, 0.55), 2)],
        &[0],
        &[0],
        0.05,
    );
    let classification = classify(&scene);
    assert_eq!(classification.records.len(), scene.lattice.tets.len());
    assert!(classification
        .records
        .iter()
        .all(|r| r.provenance == Provenance::Lattice));

    // A cell is owned only when all four of its vertices are inside, straddling
    // when they disagree, and absent from the record when all four are outside.
    for (tet, record) in scene.lattice.tets.iter().zip(classification.records.iter()) {
        let inside = tet
            .iter()
            .filter(|node| classification.is_inside(**node as usize, 0))
            .count();
        match inside {
            0 => assert!(record.entries.is_empty()),
            4 => assert_eq!(record.side_of(1), Side::Inside),
            _ => assert_eq!(record.side_of(1), Side::Ambiguous),
        }
    }
    // The straddling cells are exactly the ones S8 has to cut, so there must be a
    // shell of them and it must be a minority of the mesh.
    assert!(classification.stats.n_tets_ambiguous > 0);
    assert!(classification.stats.n_tets_ambiguous < scene.lattice.tets.len() / 2);
}

// ---------------------------------------------------------------------------
// s06_classified
// ---------------------------------------------------------------------------

#[test]
fn the_s06_snapshot_validates_verifies_and_renders() {
    let scene = scene(
        &[
            box_mesh(Vec3::new(0.25, 0.25, 0.25), Vec3::new(0.55, 0.55, 0.55), 2),
            box_mesh(Vec3::new(0.45, 0.35, 0.35), Vec3::new(0.75, 0.65, 0.65), 2),
        ],
        &[0, 0],
        &[1, 2],
        0.05,
    );
    let classification = classify(&scene);
    let mut doc = classified_to_doc(&scene.lattice, &classification, &scene.surface.components);
    doc.validate().unwrap();
    let meta = SnapshotMeta::new(
        Stage::Classified,
        0x5306,
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
    );
    stamp_metadata(&mut doc, &meta);

    for name in ["region_key", "provenance", "arbitrated"] {
        assert!(doc.cell_array(name).is_some(), "{name} missing");
    }
    let report = verify(&doc, &VerifyGates::default());
    let failures: Vec<_> = report
        .sections
        .iter()
        .flat_map(|section| section.items.iter())
        .filter(|item| item.severity == Severity::Fail)
        .map(|item| format!("{}: {}", item.code, item.message))
        .collect();
    assert!(failures.is_empty(), "s06 must verify clean: {failures:?}");

    // [V6] in particular: every region key must be legal and carry one priority.
    let v6 = report
        .sections
        .iter()
        .find(|section| section.id == "V6")
        .unwrap();
    for name in ["illegal_region_keys", "priority_mismatches"] {
        let value = v6
            .metrics
            .iter()
            .find(|(metric, _)| metric == name)
            .map(|(_, value)| *value)
            .unwrap();
        assert_eq!(value, 0.0, "{name}");
    }

    let scene_render = build_scene(
        &doc,
        &SceneSpec {
            color_mode: ColorMode::Categorical {
                array: "region_key".to_string(),
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!scene_render.tris.is_empty());
}

// ---------------------------------------------------------------------------
// Determinism (R-P2)
// ---------------------------------------------------------------------------

#[test]
fn the_classification_is_bit_identical_across_runs_and_thread_counts() {
    let build = || {
        let scene = scene(
            &[
                box_mesh(Vec3::new(0.25, 0.25, 0.25), Vec3::new(0.55, 0.55, 0.55), 2),
                sphere_mesh(Vec3::new(0.65, 0.5, 0.5), 0.15, 12),
            ],
            &[0, 0],
            &[1, 2],
            0.06,
        );
        let classification = classify(&scene);
        (
            classification.vertex_inside,
            classification.region_key,
            classification.region_sets,
            classification.active_face,
        )
    };
    let first = build();
    assert_eq!(first, build());

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    assert_eq!(first, pool.install(build));
}
