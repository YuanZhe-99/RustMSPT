//! G7-1 acceptance tests: the frozen `SPEC_meshgen_geometry.md` §8.2 band table,
//! Invariant B1 (per-pair collapse implies conformity), the §4.4 runtime ladder with
//! its negotiated flip and its Steiner fallback, regional demotion, and the
//! `PLAN_mesh_generation.md` §10.11 FEM-aware ladder including the explicit
//! profile's altitude rejection.

use rustmspt::meshgen::predicates::node_key;
use rustmspt::meshgen::thin::{
    band_cell_boundary, band_cell_table, band_face_split, band_ladder, enclosed_volume,
    mesh_band_cell, split_band_cell, Slab,
    mesh_band_layer, predict_band_quality, snk_cell_diagonals, BandCell, BandFailure, BandLayer,
    BandPair, BandTemplate, FemProfile, LadderOutcome, ThinOptions, ThinRegime,
};
use rustmspt::meshgen::gapfield::{
    pair_class_code, thin_context, GapField, PairClass, Regime, ThinRegion,
};
use rustmspt::meshgen::cut::{
    collapsed_sheet_rim, face_is_single_patch, nodes_on_rim, InterfaceFace, FACE_TAG_SHEET,
};
use rustmspt::meshgen::NodeKey;
use rustmspt::types::Vec3;
use std::collections::BTreeMap;

const QUANTUM: f64 = 1.0e-9;

// AI-FUNC-SUMMARY: Node keys for a point list; returns the key table; side effects: none.
fn keys_of(points: &[Vec3]) -> Vec<NodeKey> {
    points.iter().map(|p| node_key(*p, QUANTUM)).collect()
}

// AI-FUNC-SUMMARY:
// Purpose: The nominal band cell of `SPEC_meshgen_geometry.md` §8.2 - a right-isoceles cap of legs
//   `h` in `z = 0`, extruded by `t`.
// Returns: the six node positions, `a0,a1,a2,b0,b1,b2`.
// Side effects: None.
fn model_cell(h: f64, t: f64) -> Vec<Vec3> {
    let cap = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(h, 0.0, 0.0),
        Vec3::new(0.0, h, 0.0),
    ];
    cap.iter()
        .copied()
        .chain(cap.iter().map(|p| p.add(Vec3::new(0.0, 0.0, t))))
        .collect()
}

// AI-FUNC-SUMMARY: The three pairs of a model cell, with the listed pairs collapsed to a rim node; returns the pairs and the extended node table; side effects: none.
fn collapsed_cell(h: f64, t: f64, collapse: &[usize]) -> ([BandPair; 3], Vec<Vec3>) {
    let mut nodes = model_cell(h, t);
    let mut pairs = [
        BandPair {
            a: 0,
            b: 3,
            collapsed: None,
        },
        BandPair {
            a: 1,
            b: 4,
            collapsed: None,
        },
        BandPair {
            a: 2,
            b: 5,
            collapsed: None,
        },
    ];
    for &index in collapse {
        let mid = nodes[pairs[index].a as usize]
            .add(nodes[pairs[index].b as usize])
            .scale(0.5);
        pairs[index].collapsed = Some(nodes.len() as u32);
        nodes.push(mid);
    }
    (pairs, nodes)
}

// AI-FUNC-SUMMARY: Assert a facet list is a closed, consistently oriented surface; returns nothing; side effects: panics on failure.
fn assert_closed(facets: &[[u32; 3]]) {
    let mut directed: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for facet in facets {
        for slot in 0..3 {
            let edge = (facet[slot], facet[(slot + 1) % 3]);
            *directed.entry(edge).or_default() += 1;
        }
    }
    for (&(from, to), &count) in &directed {
        assert_eq!(
            count, 1,
            "directed edge {from}->{to} appears {count} times, so the surface is not simple"
        );
        assert_eq!(
            directed.get(&(to, from)).copied().unwrap_or(0),
            1,
            "edge {from}-{to} is not matched by its opposite: the surface is open or inconsistently oriented"
        );
    }
}

// AI-FUNC-SUMMARY: The tets' total absolute volume; returns f64; side effects: none.
fn total_volume(tets: &[[u32; 4]], nodes: &[Vec3]) -> f64 {
    tets.iter()
        .map(|tet| {
            let a = nodes[tet[0] as usize];
            let u = nodes[tet[1] as usize].sub(a);
            let v = nodes[tet[2] as usize].sub(a);
            let w = nodes[tet[3] as usize].sub(a);
            (u.cross(v).dot(w) / 6.0).abs()
        })
        .sum()
}

#[test]
fn band_cell_boundary_is_closed_for_every_k() {
    for k in 0..=3usize {
        let collapse: Vec<usize> = (0..k).collect();
        let (pairs, nodes) = collapsed_cell(1.0, 0.3, &collapse);
        let keys = keys_of(&nodes);
        let diagonals = snk_cell_diagonals(pairs, &keys);
        let facets = band_cell_boundary(pairs, diagonals);
        assert_closed(&facets);
        let expected = match k {
            0 => 8,
            1 => 6,
            2 => 4,
            _ => 0,
        };
        assert_eq!(facets.len(), expected, "k = {k} boundary facet count");
    }
}

#[test]
fn frozen_table_emits_the_frozen_tet_counts() {
    for (k, expected_tets, expected_template) in [
        (0usize, 3usize, BandTemplate::Prism),
        (1, 2, BandTemplate::Pyramid),
        (2, 1, BandTemplate::Tet),
        (3, 0, BandTemplate::Sheet),
    ] {
        let collapse: Vec<usize> = (0..k).collect();
        let (pairs, nodes) = collapsed_cell(1.0, 0.3, &collapse);
        let keys = keys_of(&nodes);
        let diagonals = snk_cell_diagonals(pairs, &keys);
        let (tets, template, sheet) = band_cell_table(pairs, diagonals).expect("table row");
        assert_eq!(tets.len(), expected_tets, "k = {k} tet count");
        assert_eq!(template, expected_template, "k = {k} template");
        assert_eq!(sheet.is_some(), k == 3, "k = {k} sheet triangle");
    }
}

#[test]
fn every_k_case_fills_its_own_boundary() {
    for k in 0..=2usize {
        let collapse: Vec<usize> = (0..k).collect();
        let (pairs, nodes) = collapsed_cell(1.0, 0.35, &collapse);
        let keys = keys_of(&nodes);
        let diagonals = snk_cell_diagonals(pairs, &keys);
        let meshed = mesh_band_cell(pairs, diagonals, &nodes, 8.0, 0).expect("k case meshes");
        assert_eq!(meshed.rung, 0, "k = {k} took the frozen row");
        let facets = band_cell_boundary(pairs, diagonals);
        let target = enclosed_volume(&facets, &nodes).abs();
        let volume = total_volume(&meshed.tets, &nodes);
        assert!(
            (volume - target).abs() <= 1.0e-12 * target.max(1.0),
            "k = {k}: children {volume} vs boundary {target}"
        );
        assert!(meshed.volume_error <= 1.0e-12, "k = {k} dry-run error");
    }
}

#[test]
fn nominal_band_cell_matches_the_corrected_spec_row() {
    // `SPEC_meshgen_geometry.md` §8.2 rev 1.3. The frozen text's original AR column
    // (1.89 / 1.80 / 1.74) did not belong to this cell: measured under [V4]'s
    // convention `R / (3 r_in)`, and confirmed by an independent exact recomputation,
    // the nominal cell at `t/h = 0.35` reads AR 2.0953 / 2.0165 / 1.9662. The dihedral
    // column was right to the digit it was quoted at. The corrected row is pinned here.
    let expected = [
        (18.280_994, 2.095_307),
        (19.561_073, 2.016_547),
        (19.852_538, 1.966_193),
    ];
    for (k, (dihedral, ar)) in expected.iter().enumerate() {
        let collapse: Vec<usize> = (0..k).collect();
        let (pairs, nodes) = collapsed_cell(1.0, 0.35, &collapse);
        let keys = keys_of(&nodes);
        let diagonals = snk_cell_diagonals(pairs, &keys);
        let meshed = mesh_band_cell(pairs, diagonals, &nodes, 0.0, 0).expect("nominal cell meshes");
        assert!(
            (meshed.min_dihedral_deg - dihedral).abs() < 1.0e-4,
            "k = {k}: min dihedral {} vs the corrected {dihedral}",
            meshed.min_dihedral_deg
        );
        assert!(
            (meshed.max_aspect_ratio - ar).abs() < 1.0e-4,
            "k = {k}: AR {} vs the corrected {ar}",
            meshed.max_aspect_ratio
        );
    }
}

#[test]
fn invariant_b1_neighbouring_cells_agree_on_a_shared_quad() {
    // Two cells sharing the pair-edge (p0, p1): the first has p2 uncollapsed (k = 0),
    // the second has p3 collapsed (k = 1), so they take different table rows. The quad
    // they share must be triangulated identically all the same.
    let mut nodes = model_cell(1.0, 0.3);
    nodes.push(Vec3::new(-0.5, 0.75_f64.sqrt(), 0.0));
    nodes.push(Vec3::new(-0.5, 0.75_f64.sqrt(), 0.3));
    nodes.push(Vec3::new(-0.5, 0.75_f64.sqrt(), 0.15));
    let pairs = vec![
        BandPair {
            a: 0,
            b: 3,
            collapsed: None,
        },
        BandPair {
            a: 1,
            b: 4,
            collapsed: None,
        },
        BandPair {
            a: 2,
            b: 5,
            collapsed: None,
        },
        BandPair {
            a: 6,
            b: 7,
            collapsed: Some(8),
        },
    ];
    let keys = keys_of(&nodes);
    let cells = vec![
        BandCell {
            region: 0,
            pairs: [0, 1, 2],
        },
        BandCell {
            region: 0,
            pairs: [1, 0, 3],
        },
    ];
    let layer = BandLayer {
        pairs: pairs.clone(),
        cells,
    };
    let mesh = mesh_band_layer(&layer, &nodes, &keys, &ThinOptions::default());
    assert!(mesh.failures.is_empty(), "{:?}", mesh.failures);
    let first = mesh.cells[0].as_ref().expect("cell 0");
    let second = mesh.cells[1].as_ref().expect("cell 1");
    assert_eq!(first.template, BandTemplate::Prism);
    assert_eq!(second.template, BandTemplate::Pyramid);

    let shared = |cell: usize| {
        let index = layer.cells[cell].pairs;
        let cell_pairs = [pairs[index[0]], pairs[index[1]], pairs[index[2]]];
        let diagonals = snk_cell_diagonals(cell_pairs, &keys);
        let facets = band_cell_boundary(cell_pairs, diagonals);
        let mut shared: Vec<[u32; 3]> = facets
            .iter()
            .filter(|facet| facet.iter().all(|node| [0u32, 1, 3, 4].contains(node)))
            .map(|facet| {
                let mut sorted = *facet;
                sorted.sort_unstable();
                sorted
            })
            .collect();
        shared.sort_unstable();
        shared
    };
    let a = shared(0);
    let b = shared(1);
    assert_eq!(a.len(), 2, "the shared quad is two triangles");
    assert_eq!(a, b, "the two cells triangulated the shared quad differently");
}

#[test]
fn the_ladder_falls_back_to_a_steiner_fan_on_a_cell_no_diagonal_saves() {
    // A cap that is a near-degenerate sliver: every diagonal choice leaves a tet under
    // the floor, so the cell must end on the Steiner rung rather than fail.
    let nodes = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.5, 1.0e-4, 0.0),
        Vec3::new(0.0, 0.0, 0.2),
        Vec3::new(1.0, 0.0, 0.2),
        Vec3::new(0.5, 1.0e-4, 0.2),
    ];
    let keys = keys_of(&nodes);
    let layer = BandLayer {
        pairs: vec![
            BandPair {
                a: 0,
                b: 3,
                collapsed: None,
            },
            BandPair {
                a: 1,
                b: 4,
                collapsed: None,
            },
            BandPair {
                a: 2,
                b: 5,
                collapsed: None,
            },
        ],
        cells: vec![BandCell {
            region: 0,
            pairs: [0, 1, 2],
        }],
    };
    let options = ThinOptions {
        regional_failure_share: 1.0,
        ..ThinOptions::default()
    };
    let mesh = mesh_band_layer(&layer, &nodes, &keys, &options);
    let cell = mesh.cells[0].as_ref().expect("the fallback meshes the cell");
    assert_eq!(cell.template, BandTemplate::Steiner);
    assert_eq!(cell.tets.len(), 8, "the fan is one tet per boundary facet");
    assert_eq!(mesh.steiner_nodes.len(), 1);
    assert!(cell.volume_error <= 1.0e-9, "the fan fills the cell");
}

#[test]
fn a_region_needing_too_many_steiner_cells_is_demoted() {
    let mut nodes = Vec::new();
    let mut pairs = Vec::new();
    let mut cells = Vec::new();
    for index in 0..4usize {
        let x = index as f64 * 2.0;
        let sliver = index == 0 || index == 1;
        let y = if sliver { 1.0e-4 } else { 0.75_f64.sqrt() };
        let base = nodes.len() as u32;
        nodes.push(Vec3::new(x, 0.0, 0.0));
        nodes.push(Vec3::new(x + 1.0, 0.0, 0.0));
        nodes.push(Vec3::new(x + 0.5, y, 0.0));
        nodes.push(Vec3::new(x, 0.0, 0.3));
        nodes.push(Vec3::new(x + 1.0, 0.0, 0.3));
        nodes.push(Vec3::new(x + 0.5, y, 0.3));
        let first = pairs.len();
        for slot in 0..3u32 {
            pairs.push(BandPair {
                a: base + slot,
                b: base + 3 + slot,
                collapsed: None,
            });
        }
        cells.push(BandCell {
            region: 0,
            pairs: [first, first + 1, first + 2],
        });
    }
    let keys = keys_of(&nodes);
    let layer = BandLayer { pairs, cells };
    let mesh = mesh_band_layer(&layer, &nodes, &keys, &ThinOptions::default());
    assert!(
        mesh.demoted_regions.contains(&0),
        "2 of 4 cells on the Steiner rung is far past the 5 % share"
    );
    assert!(mesh.cells.iter().all(|cell| cell.is_none()));
    assert!(
        mesh.steiner_nodes.is_empty(),
        "a demoted region allocates nothing"
    );
    assert!(mesh.warnings.iter().any(|w| w.contains("[THIN-SKIP]")));
}

#[test]
fn a_cell_whose_pairs_repeat_is_rejected() {
    let (mut pairs, nodes) = collapsed_cell(1.0, 0.3, &[]);
    pairs[2] = pairs[1];
    let keys = keys_of(&nodes);
    let diagonals = snk_cell_diagonals(pairs, &keys);
    assert_eq!(
        mesh_band_cell(pairs, diagonals, &nodes, 8.0, 0),
        Err(BandFailure::Degenerate)
    );
}

#[test]
fn the_prediction_degrades_monotonically_as_the_gap_closes() {
    let mut previous = 0.0;
    for step in 1..=10 {
        let t = 0.05 * step as f64;
        let prediction = predict_band_quality(t, 1.0);
        assert!(
            prediction.min_dihedral_deg >= previous,
            "t = {t}: the prediction should improve as the gap opens"
        );
        previous = prediction.min_dihedral_deg;
        assert!(prediction.min_altitude <= t * 1.000_001);
    }
}

#[test]
fn the_fem_ladder_takes_each_rung_in_the_frozen_order() {
    let options = ThinOptions {
        h_min: 0.01,
        ..ThinOptions::default()
    };
    // Rung 1: a comfortable gap.
    let decision = band_ladder(0.35, 1.0, 0.02, &options);
    assert_eq!(decision.outcome, LadderOutcome::Band);

    // Rung 2: too thin at this size, fine one level finer.
    let t = 0.10;
    let coarse = predict_band_quality(t, 1.0);
    let fine = predict_band_quality(t, 0.5);
    assert!(
        coarse.min_dihedral_deg < 8.0 && fine.min_dihedral_deg >= 8.0,
        "the rung-2 fixture needs a gap that only refinement saves: {} / {}",
        coarse.min_dihedral_deg,
        fine.min_dihedral_deg
    );
    let decision = band_ladder(t, 1.0, 0.001, &options);
    assert_eq!(decision.outcome, LadderOutcome::RefineLocally);
    assert!(decision.refined.is_some());

    // Rung 3: at the sheet threshold, with refinement pinned at the floor.
    let pinned = ThinOptions { h_min: 1.0, ..options };
    let decision = band_ladder(0.001, 1.0, 0.01, &pinned);
    assert_eq!(decision.outcome, LadderOutcome::Sheet);

    // Rung 4: representable but poor, volumetric allowed.
    let decision = band_ladder(0.001, 1.0, 1.0e-6, &pinned);
    assert_eq!(decision.outcome, LadderOutcome::Volumetric);
    assert!(decision.reason.contains("kept volumetric"));

    // Rung 5: the same region with the fallback forbidden.
    let forbidden = ThinOptions {
        forbid_volumetric: true,
        ..pinned
    };
    let decision = band_ladder(0.001, 1.0, 1.0e-6, &forbidden);
    assert_eq!(decision.outcome, LadderOutcome::Reject);
    assert!(decision.reason.contains("min dihedral"));
}

#[test]
fn the_explicit_profile_rejects_a_band_the_implicit_profile_accepts() {
    let (t, h) = (0.2, 1.0);
    let implicit = ThinOptions {
        h_min: 1.0,
        ..ThinOptions::default()
    };
    assert_eq!(
        band_ladder(t, h, 1.0e-6, &implicit).outcome,
        LadderOutcome::Band,
        "the implicit profile has no altitude floor"
    );
    let explicit = ThinOptions {
        fem_profile: FemProfile::Explicit,
        altitude_ratio: 0.5,
        forbid_volumetric: true,
        ..implicit
    };
    let decision = band_ladder(t, h, 1.0e-6, &explicit);
    assert_eq!(decision.outcome, LadderOutcome::Reject);
    assert!(
        decision.reason.contains("altitude"),
        "the rejection must name the altitude floor: {}",
        decision.reason
    );
}

#[test]
fn a_gap_sweep_stays_one_layer_and_valid() {
    for step in 1..=12 {
        let t = 0.02 * step as f64;
        let (pairs, nodes) = collapsed_cell(1.0, t, &[]);
        let keys = keys_of(&nodes);
        let layer = BandLayer {
            pairs: pairs.to_vec(),
            cells: vec![BandCell {
                region: 0,
                pairs: [0, 1, 2],
            }],
        };
        let options = ThinOptions {
            regional_failure_share: 1.0,
            ..ThinOptions::default()
        };
        let mesh = mesh_band_layer(&layer, &nodes, &keys, &options);
        let cell = mesh.cells[0].as_ref().expect("every gap in the sweep meshes");
        let extended: Vec<Vec3> = nodes
            .iter()
            .copied()
            .chain(mesh.steiner_nodes.iter().copied())
            .collect();
        // One layer, in the two forms the frozen text allows. A table row spans the gap
        // with every element (each touches both walls); the Steiner rung spans it with a
        // fan whose apex is the *only* interior node, so each element still touches a
        // wall and no element is stacked on another.
        let apex = nodes.len() as u32;
        for tet in &cell.tets {
            let touches_a = tet.iter().any(|&n| extended[n as usize].z <= 1.0e-12);
            let touches_b = tet.iter().any(|&n| extended[n as usize].z >= t - 1.0e-12);
            if cell.template == BandTemplate::Steiner {
                assert!(
                    tet.contains(&apex),
                    "t = {t}: a fan element does not reach the cell's only interior node"
                );
                assert!(touches_a || touches_b, "t = {t}: a fan element touches no wall");
            } else {
                assert!(touches_a, "t = {t}: an element misses wall A");
                assert!(touches_b, "t = {t}: an element misses wall B");
            }
        }
        assert!(
            mesh.steiner_nodes.len() <= 1,
            "t = {t}: one cell must not allocate more than its own apex"
        );
        let volume = total_volume(&cell.tets, &extended);
        let expected = 0.5 * t;
        assert!(
            (volume - expected).abs() < 1.0e-9,
            "t = {t}: the layer's volume {volume} is not the gap's {expected}"
        );
    }
}

// ---------------------------------------------------------------------------
// The doubly-cut face rule and the three-slab split (the piece §5.2 does not have)
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: A sandwiched cell - a tet whose three edges at one vertex are each crossed by two
//   near-parallel walls, which is the configuration a thin gap always produces.
// Returns: the node table, the four face loops in `face_states` order, and the first cut-node id.
// Side effects: None.
fn sandwich_cell(near: f64, far: f64) -> (Vec<Vec3>, [Vec<u32>; 4], u32) {
    // Parent nodes 0..3: the apex and the three it is separated from.
    let mut nodes = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let first_cut = nodes.len() as u32;
    // Per edge (0,w): the near wall's node then the far wall's node.
    for w in 1..4usize {
        let direction = nodes[w];
        nodes.push(direction.scale(near));
        nodes.push(direction.scale(far));
    }
    let cut = |w: usize, far_wall: bool| first_cut + ((w - 1) * 2 + usize::from(far_wall)) as u32;
    // The three faces at the apex, each walked as 0 -> w -> x -> 0 with its cuts
    // inserted in edge order, plus the uncut face opposite the apex.
    let face = |w: usize, x: usize| {
        vec![
            0,
            cut(w, false),
            cut(w, true),
            w as u32,
            x as u32,
            cut(x, true),
            cut(x, false),
        ]
    };
    (
        nodes,
        [face(1, 2), face(2, 3), face(3, 1), vec![1, 2, 3]],
        first_cut,
    )
}

#[test]
fn the_doubly_cut_face_rule_splits_a_face_into_three_slabs() {
    let (nodes, faces, first_cut) = sandwich_cell(0.30, 0.40);
    let keys = keys_of(&nodes);
    let (split, pairs) = band_face_split(&faces[0], first_cut, &keys).expect("the canonical face splits");
    assert_eq!(pairs.len(), 2, "a doubly-cut face spans two matched pairs");
    assert_eq!(split.len(), 5, "corner triangle + two quads");
    let by_slab = |slab: Slab| split.iter().filter(|(s, _)| *s == slab).count();
    assert_eq!(by_slab(Slab::NearSide), 1);
    assert_eq!(by_slab(Slab::Band), 2);
    assert_eq!(by_slab(Slab::FarSide), 2);
    let triangles: Vec<[u32; 3]> = split.iter().map(|(_, t)| *t).collect();
    assert_closed_after_adding_the_loop(&triangles, &faces[0]);
}

// AI-FUNC-SUMMARY: Assert a face's split covers it exactly - every loop edge used once, no edge used twice; returns nothing; side effects: panics on failure.
fn assert_closed_after_adding_the_loop(triangles: &[[u32; 3]], loop_nodes: &[u32]) {
    let mut directed: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for triangle in triangles {
        for slot in 0..3 {
            *directed
                .entry((triangle[slot], triangle[(slot + 1) % 3]))
                .or_default() += 1;
        }
    }
    for (&(from, to), &count) in &directed {
        assert_eq!(count, 1, "edge {from}->{to} used {count} times");
    }
    for slot in 0..loop_nodes.len() {
        let edge = (loop_nodes[slot], loop_nodes[(slot + 1) % loop_nodes.len()]);
        assert_eq!(
            directed.get(&edge).copied().unwrap_or(0),
            1,
            "the split does not cover the face's boundary edge {edge:?}"
        );
        assert_eq!(
            directed.get(&(edge.1, edge.0)).copied().unwrap_or(0),
            0,
            "the split runs back over the face's own boundary at {edge:?}"
        );
    }
}

#[test]
fn a_sandwiched_cell_splits_into_three_closed_slabs_and_keeps_its_gap() {
    let (near, far) = (0.30, 0.40);
    let (nodes, faces, first_cut) = sandwich_cell(near, far);
    let keys = keys_of(&nodes);
    let loops: [&[u32]; 4] = [&faces[0], &faces[1], &faces[2], &faces[3]];
    let plan = split_band_cell(loops, first_cut, &keys, &nodes).expect("the sandwich splits");
    let slabs = &plan.slabs;

    // Each slab is a closed, consistently oriented surface ...
    for slab in slabs {
        assert_closed(slab);
    }
    // ... their volumes sum to the parent tet's ...
    let parent = 1.0 / 6.0;
    let volumes: Vec<f64> = slabs
        .iter()
        .map(|slab| enclosed_volume(slab, &nodes).abs())
        .collect();
    let total: f64 = volumes.iter().sum();
    assert!(
        (total - parent).abs() < 1.0e-12,
        "slabs total {total}, parent {parent}"
    );
    // ... and the middle slab is the gap itself, not a chamfer: a tet scaled to `far`
    // minus one scaled to `near`.
    let expected_gap = parent * (far.powi(3) - near.powi(3));
    assert!(
        (volumes[Slab::Band as usize] - expected_gap).abs() < 1.0e-12,
        "the gap slab is {} but the gap is {expected_gap}",
        volumes[Slab::Band as usize]
    );
    // The two walls are shared faces, not coincident pairs: the near wall appears once
    // in each of the two slabs it separates, with opposite winding.
    let wall: Vec<[u32; 3]> = slabs[Slab::NearSide as usize]
        .iter()
        .filter(|t| t.iter().all(|&n| n >= first_cut))
        .copied()
        .collect();
    assert_eq!(wall.len(), 1, "the near wall is one triangle");
    let mirrored = [wall[0][0], wall[0][2], wall[0][1]];
    assert!(
        slabs[Slab::Band as usize]
            .iter()
            .any(|t| *t == mirrored || *t == [mirrored[1], mirrored[2], mirrored[0]] || *t == [mirrored[2], mirrored[0], mirrored[1]]),
        "the gap slab does not carry the near wall reversed"
    );
}

#[test]
fn a_cell_the_rule_does_not_cover_is_declined_rather_than_guessed() {
    let (nodes, mut faces, first_cut) = sandwich_cell(0.30, 0.40);
    let keys = keys_of(&nodes);
    // Drop one crossing: the cell is no longer a sandwich, and the rule must say so
    // rather than produce a plausible-looking split.
    faces[0].remove(2);
    let loops: [&[u32]; 4] = [&faces[0], &faces[1], &faces[2], &faces[3]];
    assert!(split_band_cell(loops, first_cut, &keys, &nodes).is_err());
}

// ---------------------------------------------------------------------------
// G7-2 - the S3 -> S8b bridge
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: A minimal `GapField` carrying one region over the given wall-A/wall-B triangles.
// Inputs: the region's declared regime, its pair class, wall-A tris, wall-B tris, and the number of
//   arranged faces (the identity `face_of_tri` the fixture uses).
// Returns: the field.
// Side effects: None.
fn field_with_region(
    regime: Regime,
    pair_class: PairClass,
    faces: Vec<usize>,
    opposite_faces: Vec<usize>,
    n_faces: usize,
) -> GapField {
    GapField {
        regions: vec![ThinRegion {
            id: 0,
            component: 1,
            side: 1,
            opposite_patch: 2,
            pair_class,
            declared_regime: regime,
            regime,
            samples: Vec::new(),
            faces,
            opposite_faces,
            area: 1.0,
            t_r: 0.01,
            t_max: 0.01,
            confidence: 1.0,
            failed_checks: [0; 5],
            skip: None,
            rims: Vec::new(),
            mid_surface: None,
        }],
        face_of_tri: (0..n_faces).map(|face| face as i64).collect(),
        t_sheet: 0.02,
        ..Default::default()
    }
}

#[test]
fn thin_context_attributes_both_walls_of_a_region() {
    // The whole point of the bridge: a region owns both walls of its gap, so a band
    // cell resolves the same region id from either of its two lids. `gapfield_to_doc`
    // walks `faces` only, which is right for an s03 wall attribution and wrong here.
    let field = field_with_region(Regime::Band, PairClass::Inter(1, 2), vec![0, 1], vec![2, 3], 4);
    let context = thin_context(&field, &[Regime::Band], true);
    for face in 0..4u32 {
        assert_eq!(
            context.region_of(face),
            Some(0),
            "face {face} is a wall of region 0 and must resolve to it"
        );
    }
    assert_eq!(context.regime_of(0), ThinRegime::Band);
    assert_eq!(context.pair_class_of(0), 2, "Inter is pair-class code 2");
}

#[test]
fn thin_context_takes_the_effective_regime_not_the_declared_one() {
    // A region the S3/S4 coupling or §10.11's ladder demoted is volumetric from S8b's
    // point of view, whatever the thresholds originally said. Reading
    // `ThinRegion::regime` here would collapse gaps the ladder had already refused.
    let field = field_with_region(Regime::Sheet, PairClass::SolidSheet(1, 2), vec![0], vec![1], 2);
    let context = thin_context(&field, &[Regime::Normal], true);
    assert_eq!(context.regime_of(0), ThinRegime::Normal);
    assert_eq!(context.pair_class_of(0), 3, "SolidSheet is pair-class code 3");
}

#[test]
fn an_unattributed_face_resolves_to_no_region() {
    let field = field_with_region(Regime::Band, PairClass::Intra(1), vec![0], vec![1], 4);
    let context = thin_context(&field, &[Regime::Band], true);
    assert_eq!(context.region_of(2), None);
    assert_eq!(context.region_of(3), None);
    assert_eq!(context.region_of(99), None, "out of range is not a region");
    // And an unknown region reads as volumetric rather than panicking, which is what
    // lets `CutOptions::thin = None` behave exactly like "no thin regions at all".
    assert_eq!(context.regime_of(7), ThinRegime::Normal);
}

#[test]
fn pair_class_codes_are_the_frozen_enumeration() {
    // SPEC_meshgen_contracts.md §2.3 `ThinRegionPairClass`.
    for (class, code) in [
        (PairClass::Unpaired, 0),
        (PairClass::Intra(1), 1),
        (PairClass::Inter(1, 2), 2),
        (PairClass::SolidSheet(1, 2), 3),
        (PairClass::SheetSheet(1, 2), 4),
        (PairClass::SurfaceBox(1), 5),
    ] {
        assert_eq!(pair_class_code(class), code, "{class:?}");
    }
}

// ---------------------------------------------------------------------------
// G7-2 - the collapse's two load-bearing rules
// ---------------------------------------------------------------------------

#[test]
fn a_collapsed_face_is_still_a_single_patch_face() {
    // Both walls of a collapsed gap cross the same edges at the *same* node ids, so
    // the face carries two components and is nonetheless an ordinary §5.2 face. The
    // obvious test - "more than one component means more than one patch" - sent every
    // collapsed face to the loop fan while the cell that cut it took the table, and the
    // two triangulations of the shared face disagreed (1,152 boundary leaks).
    let mut collapsed: BTreeMap<i32, [Option<u32>; 3]> = BTreeMap::new();
    collapsed.insert(1, [Some(40), Some(41), None]);
    collapsed.insert(2, [Some(40), Some(41), None]);
    assert!(face_is_single_patch(&collapsed));

    // Two patches that genuinely differ must stay inexpressible, or a real junction
    // face would take a table row it does not satisfy.
    let mut two_patches: BTreeMap<i32, [Option<u32>; 3]> = BTreeMap::new();
    two_patches.insert(1, [Some(40), Some(41), None]);
    two_patches.insert(2, [Some(42), None, Some(43)]);
    assert!(!face_is_single_patch(&two_patches));

    // Including the case that differs only on one edge - two walls that collapsed
    // along part of a face and not the rest.
    let mut partly: BTreeMap<i32, [Option<u32>; 3]> = BTreeMap::new();
    partly.insert(1, [Some(40), Some(41), None]);
    partly.insert(2, [Some(40), Some(44), None]);
    assert!(!face_is_single_patch(&partly));

    // One component, and none at all, are both single-patch by definition.
    let mut one: BTreeMap<i32, [Option<u32>; 3]> = BTreeMap::new();
    one.insert(1, [Some(40), None, None]);
    assert!(face_is_single_patch(&one));
    assert!(face_is_single_patch(&BTreeMap::new()));
}

// AI-FUNC-SUMMARY: A sheet-tagged interface face over the given nodes; returns InterfaceFace; side effects: none.
fn sheet_face(nodes: [u32; 3]) -> InterfaceFace {
    InterfaceFace {
        nodes,
        component: 1,
        kind: FACE_TAG_SHEET,
        side_elems: [0, 1],
    }
}

#[test]
fn the_declared_rim_covers_the_region_boundary_and_not_a_pinhole() {
    // Four triangles around a centre, with the middle-left one missing: node 0 is an
    // interior node, so the hole it bounds is a pinhole, while nodes 1..4 sit where the
    // collapsed region ends.
    //
    //   The rim must declare the *outer* boundary and must not declare the hole, or
    //   `[V8]`'s pinhole check becomes the third check in this codebase that passes by
    //   measuring nothing.
    let faces = vec![
        sheet_face([1, 2, 0]),
        sheet_face([2, 3, 0]),
        sheet_face([3, 4, 0]),
    ];
    let boundary: std::collections::BTreeSet<u32> = [1u32, 2, 3, 4].into_iter().collect();
    let rim = collapsed_sheet_rim(&faces, &boundary);
    // (1,2), (2,3) and (3,4) are interior to the fan; the single-use edges with both
    // endpoints on the region boundary are exactly the outer ones.
    assert!(rim.contains(&[1, 2]), "outer edge (1,2) is not declared: {rim:?}");
    assert!(rim.contains(&[3, 4]), "outer edge (3,4) is not declared: {rim:?}");
    // The hole's edges all touch node 0, which is interior, so none may be declared.
    assert!(
        rim.iter().all(|edge| edge[0] != 0 && edge[1] != 0),
        "an edge of the pinhole was declared a rim: {rim:?}"
    );
    // And with no collapse at all there is no rim to declare.
    assert!(collapsed_sheet_rim(&faces, &std::collections::BTreeSet::new()).is_empty());
}

#[test]
fn only_nodes_actually_on_a_rim_curve_may_end_a_sheet() {
    // The declaration that lets `[V8]` pass must be a geometric fact about the rim, not
    // a flag the mesher sets on its own output - otherwise the pinhole check becomes a
    // rubber stamp, which is the failure mode this file has already hit three times.
    let rim = vec![(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0))];
    let nodes = vec![
        Vec3::new(0.5, 0.0, 0.0),     // 0: on the segment
        Vec3::new(0.0, 0.0, 0.0),     // 1: at an endpoint
        Vec3::new(1.0, 0.0, 0.0),     // 2: at the other endpoint
        Vec3::new(0.5, 1.0e-7, 0.0),  // 3: within tolerance
        Vec3::new(0.5, 1.0e-3, 0.0),  // 4: outside tolerance
        Vec3::new(0.5, 0.5, 0.5),     // 5: nowhere near - a pinhole's node
        Vec3::new(2.0, 0.0, 0.0),     // 6: on the segment's *line* but past its end
    ];
    let on = nodes_on_rim(&nodes, &rim, 1.0e-6);
    for node in [0u32, 1, 2, 3] {
        assert!(on.contains(&node), "node {node} lies on the rim and was not accepted");
    }
    for node in [4u32, 5, 6] {
        assert!(
            !on.contains(&node),
            "node {node} is off the rim and was accepted - the declaration is a rubber stamp"
        );
    }
    // No rim curves at all means nothing may be declared, which is what keeps a mesh
    // with no open sheet from silently gaining a licence to leak.
    assert!(nodes_on_rim(&nodes, &[], 1.0e-6).is_empty());
}
