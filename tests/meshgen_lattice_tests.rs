//! G4-2 acceptance tests: strong 2:1 balance, the Freudenthal and centroid-fan
//! templates, and Theorem T1 (SPEC_meshgen_geometry §3.6) checked the way the
//! plan asks for it - by the verifier's [V3] conformity section, on randomized
//! sizing fields.

use rustmspt::io::vtu::VTK_TETRA;
use rustmspt::meshgen::lattice::{
    balance_octree, balance_violation, build_lattice, lattice_to_doc, CellTemplate, Lattice,
    LatticeOptions,
};
use rustmspt::meshgen::sizing::{
    build_sizing_field, SizingCriterion, SizingField, SizingLookup, SizingOptions, SizingSource,
};
use rustmspt::meshgen::snapshot::{SnapshotMeta, Stage};
use rustmspt::meshgen::{
    build_scene, stamp_metadata, tet_quality, verify, ArrangeComponent, ColorMode, SceneSpec,
    Severity, VerifyGates,
};
use rustmspt::types::Vec3;
use std::collections::{HashMap, HashSet};

// AI-FUNC-SUMMARY: The default sizing options over the unit cube; returns SizingOptions; side effects: none.
fn options() -> SizingOptions {
    SizingOptions {
        domain_min: Vec3::new(0.0, 0.0, 0.0),
        domain_max: Vec3::new(1.0, 1.0, 1.0),
        h_max: 0.5,
        h_min: 0.03,
        eps: 1.0e-4,
        ..Default::default()
    }
}

// AI-FUNC-SUMMARY:
// Purpose: A pseudo-random but reproducible sizing field, for the randomized conformity sweep.
// Inputs: a seed and the number of sources.
// Returns: the balanced octree, its split count, and the lattice built on it.
// Side effects: None.
fn random_lattice(seed: u64, n_sources: usize) -> (SizingField, usize, Lattice) {
    let options = options();
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut next = || -> f64 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };
    let sources: Vec<SizingSource> = (0..n_sources)
        .map(|_| SizingSource {
            point: Vec3::new(next(), next(), next()),
            h: options.h_min * (1.0 + 6.0 * next()),
            criterion: SizingCriterion::Gap,
        })
        .collect();
    let lookup = SizingLookup::build(sources, &options);
    let field = build_sizing_field(&lookup, &options);
    let (balanced, splits) = balance_octree(&field);
    let lattice = build_lattice(&balanced, &LatticeOptions::default()).unwrap();
    (balanced, splits, lattice)
}

// AI-FUNC-SUMMARY: Wrap a lattice in its snapshot document over the unit domain; returns VtuDoc; side effects: none.
fn lattice_doc(lattice: &Lattice, field: &SizingField) -> rustmspt::io::vtu::VtuDoc {
    let sizing: Vec<f64> = field.leaves.iter().map(|leaf| leaf.h).collect();
    let components = vec![ArrangeComponent {
        x: 1,
        priority: 0,
        kind: 0,
        closed: true,
    }];
    let mut doc = lattice_to_doc(lattice, &sizing, &components);
    let meta = SnapshotMeta::new(
        Stage::Lattice,
        0x5205,
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
    );
    stamp_metadata(&mut doc, &meta);
    doc
}

// AI-FUNC-SUMMARY: Every FAIL item the contract verifier reports for a document; returns the code list; side effects: none.
fn failures(doc: &rustmspt::io::vtu::VtuDoc) -> Vec<String> {
    let report = verify(doc, &VerifyGates::default());
    report
        .sections
        .iter()
        .flat_map(|section| section.items.iter())
        .filter(|item| item.severity == Severity::Fail)
        .map(|item| format!("{}: {}", item.code, item.message))
        .collect()
}

// ---------------------------------------------------------------------------
// Balance
// ---------------------------------------------------------------------------

#[test]
fn a_uniform_octree_is_already_strongly_balanced() {
    let options = options();
    let lookup = SizingLookup::build(Vec::new(), &options);
    let field = build_sizing_field(&lookup, &options);
    assert!(balance_violation(&field).is_none());
    let (balanced, splits) = balance_octree(&field);
    assert_eq!(splits, 0);
    assert_eq!(balanced.leaves, field.leaves);
}

#[test]
fn a_deep_point_refinement_is_balanced_and_only_ever_grows() {
    let options = options();
    let lookup = SizingLookup::build(
        vec![SizingSource {
            point: Vec3::new(0.5, 0.5, 0.5),
            h: options.h_min,
            criterion: SizingCriterion::Gap,
        }],
        &options,
    );
    let field = build_sizing_field(&lookup, &options);
    let (balanced, splits) = balance_octree(&field);
    assert!(balance_violation(&balanced).is_none(), "still unbalanced");
    assert!(balanced.leaves.len() >= field.leaves.len());
    assert_eq!(splits > 0, balanced.leaves.len() > field.leaves.len());
    // Balancing may only refine: every original leaf is a leaf or an ancestor of leaves.
    let after: std::collections::BTreeSet<(u32, [u32; 3])> = balanced
        .leaves
        .iter()
        .map(|leaf| (leaf.level, leaf.coord))
        .collect();
    for leaf in &field.leaves {
        let covered = after.contains(&(leaf.level, leaf.coord))
            || after.iter().any(|(level, coord)| {
                *level > leaf.level && {
                    let shift = level - leaf.level;
                    [coord[0] >> shift, coord[1] >> shift, coord[2] >> shift] == leaf.coord
                }
            });
        assert!(covered, "leaf {leaf:?} vanished");
    }
}

#[test]
fn balance_is_strong_not_merely_face_to_face() {
    // Two sources placed so the fine regions meet corner-to-corner: a face-only
    // balance pass leaves a two-level vertex contact, which is exactly the case
    // that produces hanging nodes (SPEC §3.1).
    let options = options();
    let lookup = SizingLookup::build(
        vec![
            SizingSource {
                point: Vec3::new(0.24, 0.24, 0.24),
                h: options.h_min,
                criterion: SizingCriterion::Gap,
            },
            SizingSource {
                point: Vec3::new(0.76, 0.76, 0.76),
                h: options.h_min,
                criterion: SizingCriterion::Gap,
            },
        ],
        &options,
    );
    let field = build_sizing_field(&lookup, &options);
    let (balanced, _) = balance_octree(&field);
    assert!(balance_violation(&balanced).is_none());
    // Verify the property directly, including vertex-only contacts.
    let leaves: Vec<(u32, [u32; 3])> = balanced
        .leaves
        .iter()
        .map(|leaf| (leaf.level, leaf.coord))
        .collect();
    let scale = |level: u32, coord: [u32; 3]| -> ([i64; 3], i64) {
        let step = 1i64 << (balanced.max_level - level);
        (
            [
                coord[0] as i64 * step,
                coord[1] as i64 * step,
                coord[2] as i64 * step,
            ],
            step,
        )
    };
    for (la, ca) in &leaves {
        for (lb, cb) in &leaves {
            if la.abs_diff(*lb) < 2 {
                continue;
            }
            let (oa, sa) = scale(*la, *ca);
            let (ob, sb) = scale(*lb, *cb);
            let touching = (0..3).all(|axis| oa[axis] <= ob[axis] + sb && ob[axis] <= oa[axis] + sa);
            assert!(
                !touching,
                "levels {la} and {lb} touch but differ by more than one"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

#[test]
fn a_uniform_lattice_is_all_freudenthal_and_tiles_the_domain_exactly() {
    let options = options();
    let lookup = SizingLookup::build(Vec::new(), &options);
    let field = build_sizing_field(&lookup, &options);
    let (balanced, _) = balance_octree(&field);
    let lattice = build_lattice(&balanced, &LatticeOptions::default()).unwrap();

    assert_eq!(lattice.stats.n_fan, 0);
    assert_eq!(lattice.stats.n_freudenthal, balanced.leaves.len());
    assert_eq!(lattice.tets.len(), balanced.leaves.len() * 6);

    // Volume closes exactly against the leaves it was built from.
    let mut lattice_volume = 0.0;
    for tet in &lattice.tets {
        let a = lattice.nodes[tet[0] as usize];
        let u = lattice.nodes[tet[1] as usize].sub(a);
        let v = lattice.nodes[tet[2] as usize].sub(a);
        let w = lattice.nodes[tet[3] as usize].sub(a);
        let volume = u.cross(v).dot(w) / 6.0;
        assert!(volume > 0.0, "every emitted tet must be positively oriented");
        lattice_volume += volume;
    }
    let cell_volume: f64 = balanced
        .leaves
        .iter()
        .map(|leaf| balanced.level_size(leaf.level).powi(3))
        .sum();
    assert!(
        (lattice_volume - cell_volume).abs() < 1.0e-9,
        "{lattice_volume} != {cell_volume}"
    );
}

#[test]
fn a_graded_lattice_uses_fan_cells_and_stays_within_the_template_inventory() {
    let (balanced, _, lattice) = random_lattice(7, 6);
    assert!(lattice.stats.n_fan > 0, "a graded field must produce fans");
    assert_eq!(
        lattice.stats.n_fan + lattice.stats.n_freudenthal,
        balanced.leaves.len()
    );
    // SPEC §3.5: 6 tets for a Freudenthal cell, 18..48 for a fan cell.
    let mut per_cell: HashMap<u32, usize> = HashMap::new();
    for cell in &lattice.cell_of_tet {
        *per_cell.entry(*cell).or_insert(0) += 1;
    }
    for (cell, count) in &per_cell {
        match lattice.templates[*cell as usize] {
            CellTemplate::Freudenthal => assert_eq!(*count, 6),
            CellTemplate::Fan => assert!(
                (18..=48).contains(count),
                "fan cell emitted {count} tets, outside the frozen 18..48 range"
            ),
        }
    }
    // Bound P1: h^3/48 <= V <= h^3/6 for the finest and coarsest cells present.
    let coarsest = balanced.level_size(balanced.leaves.iter().map(|l| l.level).min().unwrap());
    let finest = balanced.level_size(balanced.leaves.iter().map(|l| l.level).max().unwrap());
    assert!(lattice.stats.max_volume <= coarsest.powi(3) / 6.0 + 1.0e-12);
    assert!(lattice.stats.min_volume >= finest.powi(3) / 48.0 - 1.0e-12);
}

#[test]
fn every_lattice_node_is_distinct_and_in_node_key_order() {
    let (_, _, lattice) = random_lattice(11, 5);
    for pair in lattice.node_index.windows(2) {
        assert!(pair[0] < pair[1], "nodes must be strictly ascending (deduplicated, NodeKey order)");
    }
    let mut seen: HashSet<(u64, u64, u64)> = HashSet::new();
    for point in &lattice.nodes {
        assert!(
            seen.insert((point.x.to_bits(), point.y.to_bits(), point.z.to_bits())),
            "duplicate node coordinate emitted"
        );
    }
}

#[test]
fn the_templates_match_the_corrected_quality_table() {
    // SPEC §3.7 rev 1.2. The frozen text's Q row read 45 degrees, which is the
    // centre-corner-midpoint row's number; the true worst case over all eight
    // quadrant triangles is arctan(1/sqrt2). Nothing about the *rules* changed -
    // the value is forced by Rule D and §3.4 - so this test pins the corrected
    // prediction the G4-3 gate is measured against.
    let (_, _, lattice) = random_lattice(4, 5);
    let mut worst_dihedral = 180.0f64;
    let mut worst_ratio = 0.0f64;
    for tet in &lattice.tets {
        let quality = tet_quality([
            lattice.nodes[tet[0] as usize],
            lattice.nodes[tet[1] as usize],
            lattice.nodes[tet[2] as usize],
            lattice.nodes[tet[3] as usize],
        ]);
        worst_dihedral = worst_dihedral.min(quality.min_dihedral_deg);
        worst_ratio = worst_ratio.max(quality.aspect_ratio);
    }
    let expected = (1.0f64 / 2.0f64.sqrt()).atan().to_degrees();
    assert!(
        (worst_dihedral - expected).abs() < 1.0e-9,
        "worst dihedral {worst_dihedral} should be arctan(1/sqrt2) = {expected}"
    );
    assert!(
        (worst_ratio - 1.605_171_715_522_5).abs() < 1.0e-9,
        "worst aspect ratio {worst_ratio}"
    );
    // And well clear of any usable FEM gate - the correction changes a claim, not
    // the go/no-go outlook.
    assert!(worst_dihedral > 30.0);
    assert!(worst_ratio < 2.0);
}

// ---------------------------------------------------------------------------
// Theorem T1 - conformity
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Check Theorem T1 directly on a lattice: every interior triangular face is shared by
//   exactly two tets, and no node lies in the interior of another tet's face.
// Returns: (interior faces, boundary faces, faces shared by 3+ tets, hanging nodes).
// Side effects: None.
fn conformity(lattice: &Lattice) -> (usize, usize, usize, usize) {
    let mut owners: HashMap<[u32; 3], usize> = HashMap::new();
    const TET_FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];
    for tet in &lattice.tets {
        for face in TET_FACES {
            let mut key = [tet[face[0]], tet[face[1]], tet[face[2]]];
            key.sort_unstable();
            *owners.entry(key).or_insert(0) += 1;
        }
    }
    let interior = owners.values().filter(|count| **count == 2).count();
    let boundary = owners.values().filter(|count| **count == 1).count();
    let multi = owners.values().filter(|count| **count > 2).count();

    // A hanging node is a lattice node strictly inside a boundary face's triangle.
    // Working in the exact integer index space, "inside" is decidable with no
    // tolerance: the node must be coplanar and inside all three edges.
    let mut hanging = 0usize;
    let nodes = &lattice.node_index;
    for (key, count) in &owners {
        if *count != 1 {
            continue;
        }
        let tri = [
            nodes[key[0] as usize],
            nodes[key[1] as usize],
            nodes[key[2] as usize],
        ];
        let lo = [
            tri.iter().map(|p| p[0]).min().unwrap(),
            tri.iter().map(|p| p[1]).min().unwrap(),
            tri.iter().map(|p| p[2]).min().unwrap(),
        ];
        let hi = [
            tri.iter().map(|p| p[0]).max().unwrap(),
            tri.iter().map(|p| p[1]).max().unwrap(),
            tri.iter().map(|p| p[2]).max().unwrap(),
        ];
        for (index, node) in nodes.iter().enumerate() {
            if key.contains(&(index as u32)) {
                continue;
            }
            if (0..3).any(|axis| node[axis] < lo[axis] || node[axis] > hi[axis]) {
                continue;
            }
            if point_in_triangle(*node, tri) {
                hanging += 1;
            }
        }
    }
    (interior, boundary, multi, hanging)
}

// AI-FUNC-SUMMARY: Exact integer test for a point lying in a triangle's closed plane region; returns bool; side effects: none.
fn point_in_triangle(p: [u32; 3], tri: [[u32; 3]; 3]) -> bool {
    let sub = |a: [u32; 3], b: [u32; 3]| -> [i64; 3] {
        [
            a[0] as i64 - b[0] as i64,
            a[1] as i64 - b[1] as i64,
            a[2] as i64 - b[2] as i64,
        ]
    };
    let cross = |a: [i64; 3], b: [i64; 3]| -> [i64; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [i64; 3], b: [i64; 3]| -> i64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] };
    let normal = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
    if dot(normal, sub(p, tri[0])) != 0 {
        return false;
    }
    for index in 0..3 {
        let edge = sub(tri[(index + 1) % 3], tri[index]);
        let to_point = sub(p, tri[index]);
        if dot(cross(edge, to_point), normal) < 0 {
            return false;
        }
    }
    true
}

#[test]
fn a_graded_lattice_is_conforming_theorem_t1() {
    let (_, _, lattice) = random_lattice(3, 4);
    let (interior, boundary, multi, hanging) = conformity(&lattice);
    assert!(interior > 0);
    assert!(boundary > 0);
    assert_eq!(multi, 0, "a face shared by three or more tets");
    assert_eq!(hanging, 0, "a node interior to another tet's face");
}

#[test]
fn conformity_holds_over_randomized_sizing_fields() {
    // The G4-2 acceptance criterion: [V3] passes on randomized fields.
    let mut total_fans = 0usize;
    let mut deepest_span = 0u32;
    for seed in 0..12u64 {
        let (balanced, _, lattice) = random_lattice(seed, 1 + (seed as usize % 6));
        assert!(
            balance_violation(&balanced).is_none(),
            "seed {seed}: octree not strongly balanced"
        );
        // The sweep must actually exercise transitions, or it proves nothing.
        let levels: Vec<u32> = balanced.leaves.iter().map(|leaf| leaf.level).collect();
        let span = levels.iter().max().unwrap() - levels.iter().min().unwrap();
        assert!(span >= 1, "seed {seed}: uniform octree, no transition to test");
        assert!(lattice.stats.n_fan > 0, "seed {seed}: no fan cells");
        total_fans += lattice.stats.n_fan;
        deepest_span = deepest_span.max(span);
        let (_, _, multi, hanging) = conformity(&lattice);
        assert_eq!(multi, 0, "seed {seed}: non-manifold face");
        assert_eq!(hanging, 0, "seed {seed}: hanging node");

        let doc = lattice_doc(&lattice, &balanced);
        let found = failures(&doc);
        assert!(found.is_empty(), "seed {seed}: verifier FAILs {found:?}");
    }
    assert!(total_fans > 100, "only {total_fans} fan cells across the sweep");
    assert!(deepest_span >= 2, "no field spanned three octree levels");
}

#[test]
fn the_edge_only_refinement_pattern_conforms() {
    // Two sources on a shared cube edge but in diagonally opposite octants: the
    // refined regions meet along an edge only, which face-only balance misses.
    let options = options();
    let lookup = SizingLookup::build(
        vec![
            SizingSource {
                point: Vec3::new(0.2, 0.2, 0.5),
                h: options.h_min,
                criterion: SizingCriterion::Gap,
            },
            SizingSource {
                point: Vec3::new(0.8, 0.8, 0.5),
                h: options.h_min,
                criterion: SizingCriterion::Curvature,
            },
        ],
        &options,
    );
    let field = build_sizing_field(&lookup, &options);
    let (balanced, _) = balance_octree(&field);
    let lattice = build_lattice(&balanced, &LatticeOptions::default()).unwrap();
    let (_, _, multi, hanging) = conformity(&lattice);
    assert_eq!(multi, 0);
    assert_eq!(hanging, 0);
    assert!(failures(&lattice_doc(&lattice, &balanced)).is_empty());
}

// ---------------------------------------------------------------------------
// s05_lattice
// ---------------------------------------------------------------------------

#[test]
fn the_s05_snapshot_validates_verifies_and_renders() {
    let (balanced, _, lattice) = random_lattice(5, 3);
    let doc = lattice_doc(&lattice, &balanced);
    doc.validate().unwrap();
    assert_eq!(doc.num_cells(), lattice.tets.len());
    assert!(doc.types.iter().all(|kind| *kind == VTK_TETRA));
    for name in [
        "cell_kind",
        "region_key",
        "partition_id",
        "regime",
        "face_tag_key",
        "curve_id",
    ] {
        assert!(doc.cell_array(name).is_some(), "{name} missing");
    }
    assert!(doc.point_array("sizing_h").is_some());
    assert!(failures(&doc).is_empty());

    let scene = build_scene(
        &doc,
        &SceneSpec {
            color_mode: ColorMode::Scalar {
                array: "sizing_h".to_string(),
                min: None,
                max: None,
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!scene.tris.is_empty());
}

#[test]
fn the_tet_budget_reports_instead_of_exhausting_memory() {
    let (balanced, _, _) = random_lattice(2, 5);
    let error = build_lattice(&balanced, &LatticeOptions { max_tets: 10 })
        .unwrap_err()
        .to_string();
    assert!(error.contains("budget"), "{error}");
}

// ---------------------------------------------------------------------------
// Determinism (R-P2)
// ---------------------------------------------------------------------------

#[test]
fn the_lattice_is_bit_identical_across_runs_and_thread_counts() {
    let (_, splits, lattice) = random_lattice(9, 5);
    let (_, splits_again, again) = random_lattice(9, 5);
    assert_eq!(splits, splits_again);
    assert_eq!(lattice.tets, again.tets);
    assert_eq!(lattice.node_index, again.node_index);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let (_, serial_splits, serial) = pool.install(|| random_lattice(9, 5));
    assert_eq!(splits, serial_splits);
    assert_eq!(lattice.node_index, serial.node_index);
    assert_eq!(lattice.tets, serial.tets);
    assert_eq!(lattice.cell_of_tet, serial.cell_of_tet);
    for (left, right) in lattice.nodes.iter().zip(serial.nodes.iter()) {
        assert_eq!(left.x.to_bits(), right.x.to_bits());
    }
}


#[test]
fn a_non_cubic_domain_stays_conforming() {
    // The octree root is a cube around the domain, so on a non-cubic domain the
    // leaves overhang it. Trimming that overhang is only safe as a **subtree cut**:
    // dropping an individual finer cell takes its corners with it, and the coarse
    // neighbour's case-Q face then emits a quadrant corner no other cell has a node
    // for - a hanging node. This fixture produced 216 of them and 8 non-manifold
    // edges before the drop was restricted to whole subtrees.
    let mut options = options();
    options.domain_max = Vec3::new(1.0, 1.0, 0.35);
    let lookup = SizingLookup::build(
        vec![SizingSource {
            point: Vec3::new(0.5, 0.5, 0.17),
            h: options.h_min,
            criterion: SizingCriterion::Gap,
        }],
        &options,
    );
    let field = build_sizing_field(&lookup, &options);
    let (balanced, _) = balance_octree(&field);
    assert!(balance_violation(&balanced).is_none());
    let lattice = build_lattice(&balanced, &LatticeOptions::default()).unwrap();

    // The refinement really does span levels here, so the case-Q path is exercised.
    let levels: Vec<u32> = balanced.leaves.iter().map(|leaf| leaf.level).collect();
    assert!(levels.iter().max().unwrap() - levels.iter().min().unwrap() >= 2);
    assert!(lattice.stats.n_fan > 0);

    let (_, _, multi, hanging) = conformity(&lattice);
    assert_eq!(multi, 0, "non-manifold face on a non-cubic domain");
    assert_eq!(hanging, 0, "hanging node on a non-cubic domain");

    // And the domain is fully covered - trimming may not leave a hole in it.
    for i in 0..=8 {
        for j in 0..=8 {
            let probe = Vec3::new(
                0.5 + 0.49 * (i as f64 / 8.0 - 0.5) * 2.0,
                0.5 + 0.49 * (j as f64 / 8.0 - 0.5) * 2.0,
                0.17,
            );
            assert!(field.sample(probe).is_some(), "{probe:?} has no leaf");
        }
    }
}

#[test]
fn a_pre_cut_lattice_hull_is_not_reported_as_a_leak() {
    // The lattice legitimately overhangs the domain until S8 trims it, so [V3]'s
    // boundary-leak rule cannot apply to a pre-cut snapshot - but face sharing,
    // hanging nodes and manifoldness still must.
    let mut options = options();
    options.domain_max = Vec3::new(1.0, 1.0, 0.35);
    let lookup = SizingLookup::build(
        vec![SizingSource {
            point: Vec3::new(0.5, 0.5, 0.17),
            h: options.h_min,
            criterion: SizingCriterion::Gap,
        }],
        &options,
    );
    let field = build_sizing_field(&lookup, &options);
    let (balanced, _) = balance_octree(&field);
    let lattice = build_lattice(&balanced, &LatticeOptions::default()).unwrap();

    let sizing: Vec<f64> = balanced.leaves.iter().map(|leaf| leaf.h).collect();
    let components = vec![ArrangeComponent {
        x: 1,
        priority: 0,
        kind: 0,
        closed: true,
    }];
    let mut doc = lattice_to_doc(&lattice, &sizing, &components);
    let meta = SnapshotMeta::new(
        Stage::Lattice,
        0x5205,
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.35)),
    );
    stamp_metadata(&mut doc, &meta);

    let report = verify(&doc, &VerifyGates::default());
    let v3 = report
        .sections
        .iter()
        .find(|section| section.id == "V3")
        .unwrap();
    // The hull faces are counted and explained, not silently dropped. They are
    // counted as *boundary faces*, which is what they are: G7-1 made `boundary_leaks`
    // mean leaks, by accepting a single-sided face on the octree hull as the mesh's
    // own outside whenever the lattice overhangs the domain box. Before that the
    // whole overhang was reported as leaking - 128 of them on a two-plate fixture
    // with no hole in it - and the count was only survivable because the pre-cut
    // stages deferred it to INFO.
    let metric = |name: &str| {
        v3.metrics
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| *value)
            .unwrap()
    };
    assert!(
        metric("boundary_faces") > 0.0,
        "this fixture is supposed to have hull faces"
    );
    assert_eq!(
        metric("boundary_leaks"),
        0.0,
        "the octree hull is the mesh's own outside, not a leak"
    );
    assert!(v3
        .items
        .iter()
        .any(|item| item.code == "V3.deferred" && item.severity == Severity::Info));
    // ...and nothing FAILs anywhere in the report.
    assert!(failures(&doc).is_empty());
}
