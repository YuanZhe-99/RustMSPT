//! G3-2 acceptance tests: thin-region segmentation, rims, mid-surface construction
//! and its validation ladder, and the S3<->S4 coupling loop with its guards.

use rustmspt::config::meshgen::{CoincidencePolicy, RepairLevel};
use rustmspt::meshgen::gapfield::{
    compute_gap_field, gapfield_to_doc, validate_mid_surface, GapField, GapFieldOptions,
    MidSurface, MidSurfaceDefect, PairClass, Regime, SkipReason,
};
use rustmspt::meshgen::sizing::{
    couple_gap_and_sizing, regime_for, CouplingOptions, LockReason,
};
use rustmspt::meshgen::{
    arrange_surface, clip_arranged_to_box, condition_surface, detect_features, rebuild_topology,
    ArrangeComponent, ArrangeOptions,
};
use rustmspt::types::{Mesh, Triangle, Vec3};

const H_BOOTSTRAP: f64 = 0.05;

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

// AI-FUNC-SUMMARY: Build a closed box subdivided n x n per face (outward winding); returns Mesh; side effects: none.
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

// AI-FUNC-SUMMARY: Build a flat subdivided plate; returns Mesh; side effects: none.
fn plate(corners: [Vec3; 4], n: usize) -> Mesh {
    let mut mesh = Mesh::empty();
    push_quad(&mut mesh, corners, n);
    mesh
}

// AI-FUNC-SUMMARY: Run S0 -> S1 -> S2 -> clip -> S2b -> S3 with the given options; returns GapField; side effects: none.
fn gap_field_with(meshes: &[Mesh], kinds: &[u8], options: &GapFieldOptions) -> GapField {
    let eps = 1.0e-4;
    let domain_min = Vec3::new(0.0, 0.0, 0.0);
    let domain_max = Vec3::new(1.0, 1.0, 1.0);
    let components: Vec<ArrangeComponent> = kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| ArrangeComponent {
            x: index as i32 + 1,
            priority: index as u32,
            kind: *kind,
            closed: *kind == 0,
        })
        .collect();
    let conditioned = condition_surface(meshes, eps, RepairLevel::Conservative).unwrap();
    let features = detect_features(&conditioned, 45.0);
    let arranged = arrange_surface(
        &conditioned,
        &features,
        &ArrangeOptions {
            domain_min,
            domain_max,
            eps,
            coincidence: CoincidencePolicy::Merge,
            components,
        },
    )
    .unwrap();
    let mut clipped = clip_arranged_to_box(&arranged, domain_min, domain_max, eps).unwrap();
    let topo = rebuild_topology(&clipped);
    clipped.components = topo.components.clone();
    compute_gap_field(
        &clipped,
        &topo,
        &GapFieldOptions {
            domain_min,
            domain_max,
            eps,
            h_bootstrap: H_BOOTSTRAP,
            ..options.clone()
        },
    )
}

// AI-FUNC-SUMMARY: Run S3 with the default options; returns GapField; side effects: none.
fn gap_field(meshes: &[Mesh], kinds: &[u8]) -> GapField {
    gap_field_with(meshes, kinds, &GapFieldOptions::default())
}

// AI-FUNC-SUMMARY: Two closed boxes separated by `gap` along z, each face subdivided; returns the meshes; side effects: none.
fn stacked_boxes(gap: f64, n: usize) -> Vec<Mesh> {
    vec![
        box_mesh(Vec3::new(0.3, 0.3, 0.2), Vec3::new(0.6, 0.6, 0.4), n),
        box_mesh(
            Vec3::new(0.3, 0.3, 0.4 + gap),
            Vec3::new(0.6, 0.6, 0.6 + gap),
            n,
        ),
    ]
}

#[test]
fn a_resolvable_gap_segments_into_a_band_region() {
    let gap = 0.03;
    let field = gap_field(&stacked_boxes(gap, 6), &[0, 0]);
    let bands: Vec<_> = field
        .regions
        .iter()
        .filter(|region| region.regime == Regime::Band)
        .collect();
    assert!(
        !bands.is_empty(),
        "a gap between t_sheet and t_layer must segment into a band region"
    );
    for region in &bands {
        assert!(
            (region.t_r - gap).abs() <= 0.05 * gap,
            "band region t_r {} must be within 5% of {gap}",
            region.t_r
        );
        assert!(region.confidence >= 0.9, "clean band, high confidence");
        assert!(
            region.mid_surface.is_none(),
            "a band keeps both walls; only a sheet region collapses to a mid-surface"
        );
        assert!(!region.rims.is_empty(), "a converted region must have a rim");
    }
}

#[test]
fn a_sub_sheet_gap_segments_into_a_sheet_region_with_a_valid_mid_surface() {
    // A 0.008 slab: below t_sheet = 0.2 * 0.05, so the region collapses to a sheet.
    let thickness = 0.008;
    let slab = box_mesh(
        Vec3::new(0.3, 0.3, 0.5),
        Vec3::new(0.6, 0.6, 0.5 + thickness),
        6,
    );
    let field = gap_field(&[slab], &[0]);
    let sheets: Vec<_> = field
        .regions
        .iter()
        .filter(|region| region.regime == Regime::Sheet)
        .collect();
    assert!(
        !sheets.is_empty(),
        "a sub-t_sheet thin wall must segment into a sheet region"
    );
    for region in &sheets {
        assert!(matches!(region.pair_class, PairClass::Intra(1)));
        assert!(
            (region.t_r - thickness).abs() <= 0.05 * thickness,
            "sheet region t_r {} must be within 5% of {thickness}",
            region.t_r
        );
        let mid = region
            .mid_surface
            .as_ref()
            .expect("a converted sheet region must carry a mid-surface");
        assert!(
            mid.is_valid(),
            "the mid-surface must pass every check, got {:?}",
            mid.defects
        );
        assert!(!mid.triangles.is_empty());
        // Every midpoint must sit on the slab's mid-plane.
        for point in &mid.points {
            assert!(
                (point.z - (0.5 + thickness / 2.0)).abs() < 1.0e-9,
                "midpoint {point:?} must lie on the slab mid-plane"
            );
        }
        assert_eq!(region.rims.len(), 1, "the slab wall has one rim loop");
    }
}

#[test]
fn speck_regions_are_suppressed_and_kept_volumetric() {
    // Two tiny plates: a real 0.02 gap, but far too few samples to justify converting.
    let floor = plate(
        [
            Vec3::new(0.40, 0.40, 0.50),
            Vec3::new(0.46, 0.40, 0.50),
            Vec3::new(0.46, 0.46, 0.50),
            Vec3::new(0.40, 0.46, 0.50),
        ],
        1,
    );
    let ceiling = plate(
        [
            Vec3::new(0.40, 0.40, 0.52),
            Vec3::new(0.46, 0.40, 0.52),
            Vec3::new(0.46, 0.46, 0.52),
            Vec3::new(0.40, 0.46, 0.52),
        ],
        1,
    );
    let field = gap_field(&[floor, ceiling], &[1, 1]);
    assert!(
        !field.regions.is_empty(),
        "the gap must still be measured and reported"
    );
    for region in &field.regions {
        assert_eq!(
            region.regime,
            Regime::Normal,
            "a speck must fall back to volumetric"
        );
        assert_eq!(region.skip, Some(SkipReason::Speck));
    }
}

#[test]
fn a_failed_confidence_gate_keeps_the_region_volumetric() {
    // The gate itself is the ladder's first rung: no correspondence, however clean,
    // may force a conversion once confidence falls below the configured floor.
    let field = gap_field_with(
        &stacked_boxes(0.03, 6),
        &[0, 0],
        &GapFieldOptions {
            confidence_min: 1.5,
            ..Default::default()
        },
    );
    assert!(!field.regions.is_empty());
    for region in &field.regions {
        assert_eq!(region.regime, Regime::Normal);
        assert!(matches!(
            region.skip,
            Some(SkipReason::LowConfidence) | Some(SkipReason::Speck)
        ));
    }
    assert!(
        field
            .regions
            .iter()
            .any(|region| region.skip == Some(SkipReason::LowConfidence)),
        "at least one region must be rejected by the confidence gate itself"
    );
}

#[test]
fn a_twisted_mid_surface_is_rejected_by_the_validation_ladder() {
    // A correspondence that folds produces a mid-surface whose triangle reverses
    // against its source wall; §3.4 must catch it rather than emit a folded sheet.
    let mut mid = MidSurface {
        points: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        source_nodes: vec![0, 1, 2],
        triangles: vec![[0, 1, 2]],
        defects: Vec::new(),
    };
    // Source wall normal points the other way: the mapping reversed orientation.
    validate_mid_surface(&mut mid, &[Vec3::new(0.0, 0.0, -1.0)], 3, 1);
    assert!(
        mid.defects.contains(&MidSurfaceDefect::OrientationReversed),
        "a reversed mapping must be reported, got {:?}",
        mid.defects
    );
    assert!(!mid.is_valid());
}

#[test]
fn a_self_intersecting_mid_surface_is_rejected() {
    // Two triangles of one candidate crossing through each other.
    let mut mid = MidSurface {
        points: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.25, 0.25, -0.5),
            Vec3::new(0.35, 0.25, 0.5),
            Vec3::new(0.25, 0.45, 0.5),
        ],
        source_nodes: vec![0, 1, 2, 3, 4, 5],
        triangles: vec![[0, 1, 2], [3, 4, 5]],
        defects: Vec::new(),
    };
    validate_mid_surface(
        &mut mid,
        &[Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0)],
        6,
        2,
    );
    assert!(
        mid.defects.contains(&MidSurfaceDefect::SelfIntersection),
        "a crossing pair must be reported, got {:?}",
        mid.defects
    );
}

#[test]
fn a_valid_flat_mid_surface_reports_no_defects() {
    let mut mid = MidSurface {
        points: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        source_nodes: vec![0, 1, 2, 3],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        defects: Vec::new(),
    };
    let up = Vec3::new(0.0, 0.0, 1.0);
    // A quad split into two triangles: 4 boundary edges, Euler = 4 - 5 + 2 = 1.
    validate_mid_surface(&mut mid, &[up, up], 4, 1);
    assert!(
        mid.is_valid(),
        "a flat, consistently oriented patch must pass, got {:?}",
        mid.defects
    );
}

#[test]
fn s03_carries_regions_regimes_and_mid_surface_faces() {
    let thickness = 0.008;
    let slab = box_mesh(
        Vec3::new(0.3, 0.3, 0.5),
        Vec3::new(0.6, 0.6, 0.5 + thickness),
        6,
    );
    let eps = 1.0e-4;
    let domain_min = Vec3::new(0.0, 0.0, 0.0);
    let domain_max = Vec3::new(1.0, 1.0, 1.0);
    let components = vec![ArrangeComponent {
        x: 1,
        priority: 0,
        kind: 0,
        closed: true,
    }];
    let conditioned = condition_surface(&[slab], eps, RepairLevel::Conservative).unwrap();
    let features = detect_features(&conditioned, 45.0);
    let arranged = arrange_surface(
        &conditioned,
        &features,
        &ArrangeOptions {
            domain_min,
            domain_max,
            eps,
            coincidence: CoincidencePolicy::Merge,
            components,
        },
    )
    .unwrap();
    let mut clipped = clip_arranged_to_box(&arranged, domain_min, domain_max, eps).unwrap();
    let topo = rebuild_topology(&clipped);
    clipped.components = topo.components.clone();
    let field = compute_gap_field(
        &clipped,
        &topo,
        &GapFieldOptions {
            h_bootstrap: H_BOOTSTRAP,
            ..Default::default()
        },
    );
    let doc = gapfield_to_doc(&clipped, &field);
    doc.validate()
        .expect("s03 with mid-surface faces must stay a structurally valid VTU");

    let regimes = doc
        .field_array("ThinRegionRegime")
        .expect("s03 must carry the thin-region regime table");
    assert_eq!(regimes.data.len(), field.regions.len());
    assert!(
        (0..regimes.data.len()).any(|i| regimes.data.get_i64(i) == 2),
        "the slab must be recorded as a sheet region"
    );
    let thin_role = doc
        .cell_array("thin_role")
        .expect("s03 must carry the thin-role cell array");
    assert_eq!(thin_role.data.len(), doc.num_cells());
    let mid_cells = (0..thin_role.data.len())
        .filter(|i| thin_role.data.get_i64(*i) == 2)
        .count();
    assert!(
        mid_cells > 0,
        "the sheet region's mid-surface must be emitted as face cells"
    );
    let band_region = doc
        .cell_array("band_region")
        .expect("s03 must carry band_region");
    assert_eq!(band_region.data.len(), doc.num_cells());
    assert!(
        doc.field_array("ThinRegionConfidence").is_some()
            && doc.field_array("ThinRegionSeparation").is_some()
            && doc.field_array("ThinRegionSkip").is_some(),
        "the full thin-region table set must be present"
    );
}

// --- S3 <-> S4 coupling loop -------------------------------------------------

// AI-FUNC-SUMMARY: Coupling options for the loop tests; returns CouplingOptions; side effects: none.
fn coupling_options() -> CouplingOptions {
    CouplingOptions {
        tau_sheet: 0.2,
        tau_layer: 1.0,
        h_max: 0.05,
        h_min: 0.002,
        eps: 1.0e-4,
        ..Default::default()
    }
}

#[test]
fn the_loop_converges_within_three_iterations_on_a_monotone_constraint() {
    // Two regions: one deep inside Band, one comfortably Normal. A constraint that
    // shrinks h by 20% per iteration until the floor must still settle quickly.
    let t_r = [0.03, 0.4];
    let mut calls = 0usize;
    let report = couple_gap_and_sizing(&t_r, &coupling_options(), |_, h| {
        calls += 1;
        if calls <= 2 {
            h * 0.8
        } else {
            h
        }
    })
    .expect("the ordering assertion must hold");
    assert!(report.converged, "the loop must reach a fixed point");
    assert!(
        report.iterations <= 3,
        "the suite must converge in at most 3 iterations, took {}",
        report.iterations
    );
    assert_eq!(report.regimes[1], Regime::Normal);
}

#[test]
fn h_is_a_running_minimum_and_never_rises() {
    // A constraint that tries to raise h must not be able to: the driver applies
    // min(h, C) itself, which is what the monotonicity argument depends on.
    let report = couple_gap_and_sizing(&[0.03], &coupling_options(), |_, h| h * 4.0)
        .expect("ordering holds");
    assert!(report.h <= 0.05 + f64::EPSILON, "h rose to {}", report.h);
}

#[test]
fn h_never_falls_below_the_configured_floor() {
    let report = couple_gap_and_sizing(&[0.03], &coupling_options(), |_, _| 1.0e-9)
        .expect("ordering holds");
    assert!(
        (report.h - 0.002).abs() < 1.0e-12,
        "h must clamp at h_min, got {}",
        report.h
    );
}

#[test]
fn the_constraint_alone_cannot_make_a_region_oscillate() {
    // The driver clamps every proposal with min(h, C) and the floor, so a constraint
    // that alternates cannot raise h back and cannot cycle a regime. This is the
    // monotonicity guarantee of SPEC_meshgen_geometry §11.3, asserted directly.
    let mut iteration = 0usize;
    let report = couple_gap_and_sizing(&[0.0095, 0.03], &coupling_options(), |_, _| {
        iteration += 1;
        if iteration % 2 == 1 {
            0.05
        } else {
            0.04
        }
    })
    .expect("ordering holds");
    assert!(report.converged);
    for changes in &report.changes {
        assert!(
            *changes <= 2,
            "a region may change regime at most twice, saw {changes}"
        );
    }
    assert!(
        report.locked_for(LockReason::Oscillated).is_empty(),
        "no oscillation is reachable while h is a running minimum"
    );
}

#[test]
fn an_oscillating_region_locks_to_volumetric() {
    // Oscillation is reachable only through hysteresis-boundary noise (SPEC §11.4).
    // An inverted dead band is that pathology in its purest form: a value inside it
    // is claimed by the tighter regime on entry and released on the next pass.
    let options = CouplingOptions {
        hysteresis_enter: 1.1,
        hysteresis_leave: 0.9,
        ..coupling_options()
    };
    let report =
        couple_gap_and_sizing(&[0.0095], &options, |_, h| h).expect("ordering holds");
    let oscillated = report.locked_for(LockReason::Oscillated);
    assert_eq!(
        oscillated,
        vec![0],
        "the cycling region must be caught by the oscillation guard"
    );
    assert_eq!(
        report.regimes[0],
        Regime::Normal,
        "an oscillating region locks to volumetric, never to sheet"
    );
}

#[test]
fn hitting_the_iteration_cap_locks_the_unstable_regions() {
    // A constraint that keeps halving h never settles inside the cap.
    let report = couple_gap_and_sizing(&[0.03, 0.045], &coupling_options(), |_, h| h * 0.5)
        .expect("ordering holds");
    assert!(report.hit_cap, "the cap must be reported");
    assert!(!report.converged);
    assert_eq!(report.iterations, coupling_options().max_iterations);
    for index in report.locked_for(LockReason::IterationCap) {
        assert_eq!(
            report.regimes[index],
            Regime::Normal,
            "a region still unstable at the cap locks volumetric"
        );
    }
}

#[test]
fn the_g8_ordering_assertion_fires_on_the_realised_field() {
    // eps must stay far below t_sheet on the realised field, not merely on the
    // configured one; a floor that drags t_sheet down to eps is a hard failure.
    let options = CouplingOptions {
        eps: 0.01,
        h_min: 0.002,
        ..coupling_options()
    };
    let error = couple_gap_and_sizing(&[0.001], &options, |_, _| 0.002)
        .expect_err("the G-8 assertion must fire");
    assert!(
        error.to_string().contains("G-8"),
        "the error must name the assertion, got {error}"
    );
}

#[test]
fn the_hysteresis_band_delays_but_never_reverses_a_transition() {
    let (t_sheet, t_layer) = (0.01, 0.05);
    // Sitting just above the sheet threshold: entering needs 0.9 * t_sheet.
    assert_eq!(
        regime_for(0.0095, Regime::Band, t_sheet, t_layer, 0.9, 1.1),
        Regime::Band,
        "a value above the enter bound must not tighten"
    );
    // Already a sheet, the same value stays one until it passes 1.1 * t_sheet.
    assert_eq!(
        regime_for(0.0095, Regime::Sheet, t_sheet, t_layer, 0.9, 1.1),
        Regime::Sheet,
        "leaving needs the wider bound"
    );
    assert_eq!(
        regime_for(0.02, Regime::Sheet, t_sheet, t_layer, 0.9, 1.1),
        Regime::Band,
        "well past the leave bound the region relaxes"
    );
    assert_eq!(
        regime_for(f64::INFINITY, Regime::Normal, t_sheet, t_layer, 0.9, 1.1),
        Regime::Normal,
        "an unpaired region is always volumetric"
    );
}

#[test]
fn pairing_closure_gives_one_region_per_gap_not_one_per_wall() {
    // Both walls of a gap belong to the same region (the reference thin-feature design §3.3 closure), so
    // one thin feature yields one region - and one mid-surface, not two coincident
    // copies of the same sheet.
    let thickness = 0.008;
    let slab = box_mesh(
        Vec3::new(0.3, 0.3, 0.5),
        Vec3::new(0.6, 0.6, 0.5 + thickness),
        6,
    );
    let field = gap_field(&[slab], &[0]);
    let sheets: Vec<_> = field
        .regions
        .iter()
        .filter(|region| region.regime == Regime::Sheet)
        .collect();
    assert_eq!(
        sheets.len(),
        1,
        "the slab is one thin feature and must produce exactly one sheet region"
    );
    let region = sheets[0];
    assert!(
        !region.opposite_faces.is_empty(),
        "the closure must pull the opposite wall into the region"
    );
    assert!(
        region.faces.iter().all(|face| !region.opposite_faces.contains(face)),
        "wall A and the opposite wall must stay distinguishable"
    );
    let mid = region.mid_surface.as_ref().expect("sheet carries a mid-surface");
    assert_eq!(
        mid.triangles.len(),
        region.faces.len(),
        "the mid-surface is wall A's own triangulation, re-indexed once"
    );
}

// AI-FUNC-SUMMARY: Two flat plates crossing at a shallow angle, each subdivided; returns the meshes; side effects: none.
fn crossing_plates(slope: f64, n: usize) -> Vec<Mesh> {
    vec![
        plate(
            [
                Vec3::new(0.20, 0.20, 0.50),
                Vec3::new(0.80, 0.20, 0.50),
                Vec3::new(0.80, 0.80, 0.50),
                Vec3::new(0.20, 0.80, 0.50),
            ],
            n,
        ),
        plate(
            [
                Vec3::new(0.20, 0.20, 0.50 - slope / 2.0),
                Vec3::new(0.80, 0.20, 0.50 + slope / 2.0),
                Vec3::new(0.80, 0.80, 0.50 + slope / 2.0),
                Vec3::new(0.20, 0.80, 0.50 - slope / 2.0),
            ],
            n,
        ),
    ]
}

#[test]
fn a_wedge_at_an_intersection_curve_never_collapses_to_a_sheet() {
    // Two surfaces crossing at a shallow angle are arbitrarily thin near their
    // intersection curve. That neighbourhood belongs to S8's junction machinery, not
    // to the sheet collapse: converting it would weld two surfaces that merely cross.
    let field = gap_field(&crossing_plates(0.10, 16), &[1, 1]);
    assert!(
        field
            .regions
            .iter()
            .all(|region| region.regime != Regime::Sheet),
        "no part of an intersection wedge may collapse to a sheet"
    );
    assert!(
        field
            .regions
            .iter()
            .any(|region| region.skip == Some(SkipReason::IntersectionWedge)),
        "the wedge must be reported as such, not silently dropped"
    );
}

#[test]
fn every_converted_region_stays_inside_its_own_regime_interval() {
    // A region whose members span from below t_sheet up to t_layer describes a
    // separation no single band or sheet template could mesh. Growth, and the
    // pairing closure with it, must keep each region inside its own interval.
    for meshes in [crossing_plates(0.10, 16), stacked_boxes(0.03, 6)] {
        let kinds = vec![1u8; meshes.len()];
        let field = gap_field(&meshes, &kinds);
        for region in &field.regions {
            if region.regime == Regime::Normal {
                continue;
            }
            for index in &region.samples {
                let sample = &field.samples[*index];
                let separation = if sample.t_exact.is_finite() {
                    sample.t_exact
                } else {
                    sample.t_raw
                };
                if !separation.is_finite() {
                    continue;
                }
                let lower = match region.regime {
                    Regime::Sheet => 0.0,
                    _ => 1.1 * field.t_sheet,
                };
                assert!(
                    separation > lower - 1.0e-12,
                    "region {} ({:?}) holds a member at {separation}, below its own interval",
                    region.id,
                    region.regime
                );
            }
        }
    }
}

#[test]
fn the_gap_field_is_identical_whatever_the_thread_count() {
    // R-P2: parallelism may not change a committed number. The rayon pool is global
    // and shared with the rest of the suite, so this asserts the property the
    // per-stage parallel shapes guarantee - identical results from the same input -
    // which is what the byte-comparison across RAYON_NUM_THREADS checks end to end.
    let meshes = stacked_boxes(0.03, 6);
    let first = gap_field(&meshes, &[0, 0]);
    let second = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("single-threaded pool")
        .install(|| gap_field(&meshes, &[0, 0]));
    assert_eq!(first.samples.len(), second.samples.len());
    assert_eq!(first.regions.len(), second.regions.len());
    for (a, b) in first.samples.iter().zip(&second.samples) {
        assert_eq!(a.t_raw.to_bits(), b.t_raw.to_bits());
        assert_eq!(a.t.to_bits(), b.t.to_bits());
        assert_eq!(a.flags, b.flags);
    }
    for (a, b) in first.regions.iter().zip(&second.regions) {
        assert_eq!(a.t_r.to_bits(), b.t_r.to_bits());
        assert_eq!(a.regime, b.regime);
        assert_eq!(a.skip, b.skip);
        assert_eq!(a.samples.len(), b.samples.len());
    }
}
