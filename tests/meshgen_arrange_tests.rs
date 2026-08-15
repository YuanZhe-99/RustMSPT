//! G2-1 S2 corefinement acceptance tests.

use rand::{rngs::StdRng, Rng, SeedableRng};
use rustmspt::config::meshgen::{CoincidencePolicy, MeshGenConfig, RepairLevel};
use rustmspt::error::RustMsptError;
use rustmspt::io::{load_vtu, save_stl};
use rustmspt::meshgen::arrange::clip_arranged_to_box;
use rustmspt::meshgen::predicates::{
    construct_coplanar_segment_intersection, construct_edge_triangle_intersection,
    construct_three_triangle_intersection, orient2d_axis, orient2d_axis_value_permanent, orient3d,
    orient3d_dd_value, orient3d_filtered, orient3d_value_permanent, ConstructionOutcome,
    PrecisionTier, ProjectionAxis,
};
use rustmspt::meshgen::{
    arrange_surface, arranged_surface_to_doc, condition_surface, detect_features, stamp_metadata,
    triangulate_parent, verify_with_options, ArrangeComponent, ArrangeOptions, ArrangedCurveKind,
    ArrangedFace, CoincidenceCase, CoincidenceEntity, DegradedReason, IsectProv, SnapshotMeta,
    Stage, TriId, VerifyGates, VerifyOptions,
};
use rustmspt::pipeline::meshgen::MeshGenPipeline;
use rustmspt::pipeline::Pipeline;
use rustmspt::types::{Mesh, Triangle, Vec3};
use std::collections::BTreeSet;
use std::path::Path;

// AI-FUNC-SUMMARY: Build one triangle mesh from three points; returns Mesh; side effects: none.
fn triangle(points: [Vec3; 3]) -> Mesh {
    Mesh {
        vertices: points.to_vec(),
        faces: vec![Triangle { a: 0, b: 1, c: 2 }],
    }
}

// AI-FUNC-SUMMARY: Run S0/S1/S2 over input triangle meshes in a fixed normalized test domain; returns ArrangedSurface; side effects: none.
fn arrange(meshes: &[Mesh]) -> rustmspt::meshgen::ArrangedSurface {
    let components = (0..meshes.len())
        .map(|index| ArrangeComponent {
            x: index as i32 + 1,
            priority: index as u32,
            kind: 1,
            closed: false,
        })
        .collect();
    arrange_with(meshes, 1.0e-6, CoincidencePolicy::Merge, components).unwrap()
}

// AI-FUNC-SUMMARY: Run S0/S1/S2 with explicit epsilon, coincidence policy, and component metadata; returns arranged result; side effects: none.
fn arrange_with(
    meshes: &[Mesh],
    eps: f64,
    coincidence: CoincidencePolicy,
    components: Vec<ArrangeComponent>,
) -> rustmspt::error::Result<rustmspt::meshgen::ArrangedSurface> {
    let conditioned = condition_surface(meshes, eps, RepairLevel::Conservative)?;
    let features = detect_features(&conditioned, 45.0);
    arrange_surface(
        &conditioned,
        &features,
        &ArrangeOptions {
            domain_min: Vec3::new(-2.0, -2.0, -2.0),
            domain_max: Vec3::new(2.0, 2.0, 2.0),
            eps,
            coincidence,
            components,
        },
    )
}

// AI-FUNC-SUMMARY: Run S0/S1/S2 in a unit-diagonal numerical frame for exact cancellation/floor fixtures; returns arranged result; side effects: none.
fn arrange_unit_scale(
    meshes: &[Mesh],
    eps: f64,
) -> rustmspt::error::Result<rustmspt::meshgen::ArrangedSurface> {
    let conditioned = condition_surface(meshes, eps, RepairLevel::Conservative)?;
    let features = detect_features(&conditioned, 45.0);
    let components = (0..meshes.len())
        .map(|index| ArrangeComponent {
            x: index as i32 + 1,
            priority: index as u32,
            kind: 0,
            closed: false,
        })
        .collect();
    arrange_surface(
        &conditioned,
        &features,
        &ArrangeOptions {
            domain_min: Vec3::new(0.0, 0.0, 0.0),
            domain_max: Vec3::new(1.0, 0.0, 0.0),
            eps,
            coincidence: CoincidencePolicy::Merge,
            components,
        },
    )
}

// AI-FUNC-SUMMARY: Build deterministic sheet component metadata with caller-supplied priorities; returns component rows; side effects: none.
fn sheet_components(priorities: &[u32]) -> Vec<ArrangeComponent> {
    priorities
        .iter()
        .enumerate()
        .map(|(index, priority)| ArrangeComponent {
            x: index as i32 + 1,
            priority: *priority,
            kind: 1,
            closed: false,
        })
        .collect()
}

#[test]
fn proper_crossing_is_split_and_shared() {
    let horizontal = triangle([
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(0.2, 1.0, 0.0),
    ]);
    let vertical = triangle([
        Vec3::new(0.0, -0.5, -1.0),
        Vec3::new(0.0, -0.5, 1.0),
        Vec3::new(0.0, 0.8, 1.0),
    ]);
    let arranged = arrange(&[horizontal, vertical]);
    assert_eq!(arranged.stats.proper_intersections, 1);
    assert_eq!(arranged.registry.segments.len(), 1);
    assert_eq!(arranged.registry.vertices.len(), 2);
    assert!(arranged.faces.len() > 2);
    assert!(arranged
        .curves
        .iter()
        .any(|curve| curve.kind == ArrangedCurveKind::Intersection));
    let segment = &arranged.registry.segments[0];
    assert_ne!(segment.nodes[0], segment.nodes[1]);
    assert_eq!(segment.components, [1, 2]);
    let intersection = arranged
        .curves
        .iter()
        .find(|curve| curve.kind == ArrangedCurveKind::Intersection)
        .unwrap();
    assert_eq!(intersection.radial_patches.len(), 4);
    let radial: std::collections::BTreeSet<_> =
        intersection.radial_patches.iter().copied().collect();
    assert_eq!(radial.len(), 4);
    for face in radial {
        let face = &arranged.faces[face as usize];
        assert!(face.nodes.contains(&segment.nodes[0]));
        assert!(face.nodes.contains(&segment.nodes[1]));
    }
    for vertex in &arranged.registry.vertices {
        assert!(arranged.curves.iter().any(|curve| {
            curve.kind == ArrangedCurveKind::Rim && curve.nodes.contains(&vertex.node)
        }));
    }
}

#[test]
fn adjacent_triangles_share_one_registry_vertex() {
    let sheet = Mesh {
        vertices: vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 0, b: 2, c: 3 }],
    };
    let cutter = triangle([
        Vec3::new(0.0, -1.2, -1.0),
        Vec3::new(0.0, 1.2, -1.0),
        Vec3::new(0.0, 0.0, 2.0),
    ]);
    let arranged = arrange(&[sheet, cutter]);
    assert_eq!(arranged.stats.proper_intersections, 2);
    let shared_edge_vertices: Vec<_> = arranged
        .registry
        .vertices
        .iter()
        .filter(|vertex| matches!(vertex.provenance, IsectProv::EdgeTri { triangle: 2, .. }))
        .collect();
    assert_eq!(
        shared_edge_vertices.len(),
        1,
        "the shared source edge must produce one registry identity"
    );
    assert_eq!(arranged.registry.segments.len(), 2);
}

#[test]
fn three_planes_create_one_canonical_triple_point() {
    let z_plane = triangle([
        Vec3::new(-10.0, -10.0, 0.0),
        Vec3::new(12.0, -8.0, 0.0),
        Vec3::new(-2.0, 13.0, 0.0),
    ]);
    let x_plane = triangle([
        Vec3::new(0.0, -5.0, -5.0),
        Vec3::new(0.0, 6.0, -4.0),
        Vec3::new(0.0, -1.0, 7.0),
    ]);
    let y_plane = triangle([
        Vec3::new(-2.0, 0.0, -2.0),
        Vec3::new(3.0, 0.0, -1.0),
        Vec3::new(-1.0, 0.0, 3.0),
    ]);
    let arranged = arrange(&[z_plane, x_plane, y_plane]);
    assert_eq!(arranged.stats.proper_intersections, 3);
    assert_eq!(arranged.stats.triple_points, 1);
    let triples: Vec<_> = arranged
        .registry
        .vertices
        .iter()
        .filter(|vertex| matches!(vertex.provenance, IsectProv::TriTriTri(_)))
        .collect();
    assert_eq!(triples.len(), 1);
    assert_eq!(triples[0].provenance, IsectProv::TriTriTri([0, 1, 2]));
}

#[test]
fn arranged_snapshot_uses_surface_stage_contract_arrays() {
    let a = triangle([
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(0.2, 1.0, 0.0),
    ]);
    let b = triangle([
        Vec3::new(0.0, -0.5, -1.0),
        Vec3::new(0.0, -0.5, 1.0),
        Vec3::new(0.0, 0.8, 1.0),
    ]);
    let arranged = arrange(&[a, b]);
    let mut doc = arranged_surface_to_doc(&arranged);
    doc.validate().unwrap();
    for name in [
        "cell_kind",
        "region_key",
        "partition_id",
        "regime",
        "face_tag_key",
        "curve_id",
    ] {
        assert!(doc.cell_array(name).is_some(), "missing {name}");
    }
    for name in ["n_id_key", "constraint_kind", "constraint_ref"] {
        assert!(doc.point_array(name).is_some(), "missing {name}");
    }
    for name in [
        "RegionSetOffsets",
        "NIdSetOffsets",
        "FaceTagOffsets",
        "ComponentX",
        "CurveKind",
    ] {
        assert!(doc.field_array(name).is_some(), "missing {name}");
    }
    assert!(doc.types.iter().all(|cell| *cell == 4 || *cell == 5));
    stamp_metadata(
        &mut doc,
        &SnapshotMeta::new(
            Stage::Arranged,
            0,
            (Vec3::new(-2.0, -2.0, -2.0), Vec3::new(2.0, 2.0, 2.0)),
        ),
    );
    let report = verify_with_options(&doc, &VerifyGates::default(), VerifyOptions::default());
    assert_eq!(report.fail, 0, "{:?}", report.fired_codes());
}

#[test]
// AI-FUNC-SUMMARY: Exercise the frozen C3 f64, DD-escalated, and DD-floor routes while pinning exact crossing signs and the 0.25q position bound; side effects: none.
fn coplanar_segment_construction_obeys_rule_n5() {
    let first = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)];
    let second = [Vec3::new(0.25, -0.5, 0.0), Vec3::new(0.25, 0.5, 0.0)];
    assert!(
        orient2d_axis(first[0], first[1], second[0], ProjectionAxis::Z)
            * orient2d_axis(first[0], first[1], second[1], ProjectionAxis::Z)
            < 0.0
    );
    assert!(
        orient2d_axis(second[0], second[1], first[0], ProjectionAxis::Z)
            * orient2d_axis(second[0], second[1], first[1], ProjectionAxis::Z)
            < 0.0
    );

    let weld_step = 1.0e-4;
    match construct_coplanar_segment_intersection(first, second, ProjectionAxis::Z, weld_step) {
        ConstructionOutcome::Resolved { value, tier, .. } => {
            assert_eq!(tier, PrecisionTier::F64);
            assert_eq!(value.point, Vec3::new(0.25, 0.0, 0.0));
            assert_eq!(value.first_parameter, 0.25);
            assert_eq!(value.second_parameter, 0.5);
        }
        ConstructionOutcome::Deferred { .. } => panic!("well-conditioned C3 should resolve"),
    }

    let delta = f64::EPSILON * 4096.0;
    let near_parallel_first = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0)];
    let near_parallel_second = [
        Vec3::new(0.25, 0.25 - 0.5 * delta, 0.0),
        Vec3::new(0.75, 0.75 + 0.5 * delta, 0.0),
    ];
    assert!(
        orient2d_axis(
            near_parallel_first[0],
            near_parallel_first[1],
            near_parallel_second[0],
            ProjectionAxis::Z,
        ) * orient2d_axis(
            near_parallel_first[0],
            near_parallel_first[1],
            near_parallel_second[1],
            ProjectionAxis::Z,
        ) < 0.0
    );
    match construct_coplanar_segment_intersection(
        near_parallel_first,
        near_parallel_second,
        ProjectionAxis::Z,
        weld_step,
    ) {
        ConstructionOutcome::Resolved {
            value, tier, rho, ..
        } => {
            assert_eq!(tier, PrecisionTier::DoubleDouble);
            assert!(rho > 0.0);
            let expected = Vec3::new(0.5, 0.5, 0.0);
            let error = value.point.sub(expected);
            assert!(error.dot(error).sqrt() <= 0.25 * weld_step);
            assert!((value.first_parameter - 0.5).abs() <= f64::EPSILON);
            assert!((value.second_parameter - 0.5).abs() <= f64::EPSILON);
        }
        ConstructionOutcome::Deferred { .. } => {
            panic!("near-parallel C3 above the DD floor should resolve")
        }
    }

    assert!(matches!(
        construct_coplanar_segment_intersection(
            first,
            second,
            ProjectionAxis::Z,
            1.0e-40,
        ),
        ConstructionOutcome::Deferred { rho } if rho > 0.0
    ));
}

#[test]
// AI-FUNC-SUMMARY: Verify C3 endpoint and edge swaps preserve the canonical point while determinant ratios complement or exchange deterministically; side effects: none.
fn coplanar_segment_parameters_are_canonical_under_swaps() {
    let first = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)];
    let second = [Vec3::new(0.375, -0.25, 0.0), Vec3::new(0.375, 0.75, 0.0)];
    let resolve = |first, second| match construct_coplanar_segment_intersection(
        first,
        second,
        ProjectionAxis::Z,
        1.0e-4,
    ) {
        ConstructionOutcome::Resolved {
            value, tier, rho, ..
        } => (value, tier, rho),
        ConstructionOutcome::Deferred { .. } => panic!("canonical crossing should resolve"),
    };

    let base = resolve(first, second);
    let first_reversed = resolve([first[1], first[0]], second);
    let second_reversed = resolve(first, [second[1], second[0]]);
    let edges_exchanged = resolve(second, first);
    for candidate in [&first_reversed, &second_reversed, &edges_exchanged] {
        assert_eq!(candidate.0.point, base.0.point);
        assert_eq!(candidate.1, base.1);
        assert_eq!(candidate.2, base.2);
    }

    assert_eq!(base.0.first_parameter, 0.375);
    assert_eq!(base.0.second_parameter, 0.25);
    assert_eq!(first_reversed.0.first_parameter, 0.625);
    assert_eq!(first_reversed.0.second_parameter, 0.25);
    assert_eq!(second_reversed.0.first_parameter, 0.375);
    assert_eq!(second_reversed.0.second_parameter, 0.75);
    assert_eq!(edges_exchanged.0.first_parameter, base.0.second_parameter);
    assert_eq!(edges_exchanged.0.second_parameter, base.0.first_parameter);

    assert_eq!(
        base.0
            .first_ratio
            .compare(first_reversed.0.first_ratio.complement()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        base.0.second_ratio.compare(first_reversed.0.second_ratio),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        base.0.first_ratio.compare(second_reversed.0.first_ratio),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        base.0
            .second_ratio
            .compare(second_reversed.0.second_ratio.complement()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        base.0.first_ratio.compare(edges_exchanged.0.second_ratio),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        base.0.second_ratio.compare(edges_exchanged.0.first_ratio),
        std::cmp::Ordering::Equal
    );
}

#[test]
// AI-FUNC-SUMMARY: Pin C3 behavior after the mandatory translation/scale normalization and across equivalent explicit projection axes; side effects: none.
fn coplanar_segment_construction_respects_normalized_frames() {
    let first = [Vec3::new(0.125, 0.25, 0.5), Vec3::new(0.875, 0.25, 0.5)];
    let second = [Vec3::new(0.5, 0.0, 0.5), Vec3::new(0.5, 0.75, 0.5)];
    let base = construct_coplanar_segment_intersection(first, second, ProjectionAxis::Z, 1.0e-4);

    let origin = Vec3::new(1_048_576.0, -524_288.0, 262_144.0);
    let scale = 8.0;
    let normalize = |point: Vec3| {
        origin
            .add(point.scale(scale))
            .sub(origin)
            .scale(1.0 / scale)
    };
    let normalized_first = first.map(normalize);
    let normalized_second = second.map(normalize);
    assert_eq!(normalized_first, first);
    assert_eq!(normalized_second, second);
    assert_eq!(
        construct_coplanar_segment_intersection(
            normalized_first,
            normalized_second,
            ProjectionAxis::Z,
            1.0e-4,
        ),
        base
    );

    let to_yz = |point: Vec3| Vec3::new(point.z, point.x, point.y);
    let yz = construct_coplanar_segment_intersection(
        first.map(to_yz),
        second.map(to_yz),
        ProjectionAxis::X,
        1.0e-4,
    );
    match (base, yz) {
        (
            ConstructionOutcome::Resolved {
                value: base,
                tier: base_tier,
                rho: base_rho,
            },
            ConstructionOutcome::Resolved {
                value: yz,
                tier: yz_tier,
                rho: yz_rho,
            },
        ) => {
            assert_eq!(yz.point, to_yz(base.point));
            assert_eq!(yz.first_parameter, base.first_parameter);
            assert_eq!(yz.second_parameter, base.second_parameter);
            assert_eq!(yz_tier, base_tier);
            assert_eq!(yz_rho, base_rho);
        }
        _ => panic!("equivalent normalized projections should resolve identically"),
    }
}

#[test]
fn static_filter_and_dd_construction_keep_exact_sign_contract() {
    let a = Vec3::new(0.0, 0.0, 0.0);
    let b = Vec3::new(1.0, 0.0, 0.0);
    let c = Vec3::new(0.0, 1.0, 0.0);
    let d = Vec3::new(0.0, 0.0, 1.0);
    let (sign, filtered, _, _) = orient3d_filtered(a, b, c, d);
    assert_eq!(sign, 1);
    assert!(filtered);
    assert!(orient3d(a, b, c, d) > 0.0);

    let outcome = construct_edge_triangle_intersection(
        [a, b, c],
        Vec3::new(0.25, 0.25, -1.0),
        Vec3::new(0.25, 0.25, 1.0),
        1.0e-20,
    );
    match outcome {
        ConstructionOutcome::Resolved { value, tier, .. } => {
            assert_eq!(tier, PrecisionTier::DoubleDouble);
            assert!((value.parameter - 0.5).abs() < 1.0e-15);
            assert_eq!(value.point.z, 0.0);
        }
        ConstructionOutcome::Deferred { .. } => panic!("crossing should resolve in DD"),
    }

    let c2 = construct_three_triangle_intersection(
        [
            [
                Vec3::new(-2.0, -2.0, 0.0),
                Vec3::new(3.0, -1.0, 0.0),
                Vec3::new(-1.0, 3.0, 0.0),
            ],
            [
                Vec3::new(0.0, -3.0, -2.0),
                Vec3::new(0.0, 4.0, -1.0),
                Vec3::new(0.0, -1.0, 3.0),
            ],
            [
                Vec3::new(-3.0, 0.0, -2.0),
                Vec3::new(4.0, 0.0, -1.0),
                Vec3::new(-1.0, 0.0, 3.0),
            ],
        ],
        1.0e-20,
    );
    match c2 {
        ConstructionOutcome::Resolved { value, tier, .. } => {
            assert_eq!(tier, PrecisionTier::DoubleDouble);
            assert!(value.dot(value).sqrt() < 1.0e-14, "{value:?}");
        }
        ConstructionOutcome::Deferred { .. } => panic!("three orthogonal planes should resolve"),
    }

    assert!(matches!(
        construct_edge_triangle_intersection(
            [a, b, c],
            Vec3::new(0.25, 0.25, -1.0),
            Vec3::new(0.25, 0.25, 1.0),
            1.0e-40,
        ),
        ConstructionOutcome::Deferred { .. }
    ));
}

#[test]
// AI-FUNC-SUMMARY: Pin C1/C2/C3 selected-tier Rule N5 behavior when f64 determinants cancel exactly, including DD resolution above and deferral below the true DD floor; side effects: none.
fn rule_n5_recomputes_cancelled_determinants_before_applying_the_dd_floor() {
    let weld_step = 1.0e-4;
    let run = |delta: f64| {
        let first = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0 + delta, 1.0, 0.0)];
        let second = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0 - delta, 0.0)];
        let (op, _) =
            orient2d_axis_value_permanent(first[0], first[1], second[0], ProjectionAxis::Z);
        let (oq, _) =
            orient2d_axis_value_permanent(first[0], first[1], second[1], ProjectionAxis::Z);
        let (oa, _) =
            orient2d_axis_value_permanent(second[0], second[1], first[0], ProjectionAxis::Z);
        let (ob, _) =
            orient2d_axis_value_permanent(second[0], second[1], first[1], ProjectionAxis::Z);
        assert_eq!(
            op - oq,
            0.0,
            "C3 second-edge f64 denominator did not cancel"
        );
        assert_eq!(oa - ob, 0.0, "C3 first-edge f64 denominator did not cancel");

        let c1_triangle = [
            Vec3::new(1.0 + delta, 1.0, 0.0),
            Vec3::new(1.0, 1.0 - delta, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let p = Vec3::new(0.0, 0.0, 0.0);
        let q = c1_triangle[0];
        let (dp, _) = orient3d_value_permanent(c1_triangle[0], c1_triangle[1], c1_triangle[2], p);
        let (dq, _) = orient3d_value_permanent(c1_triangle[0], c1_triangle[1], c1_triangle[2], q);
        assert_eq!(dp - dq, 0.0, "C1 f64 denominator did not cancel");

        let origin = Vec3::new(0.0, 0.0, 0.0);
        let c2_triangles = [
            [
                origin,
                Vec3::new(-1.0, 1.0 + delta, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            [
                origin,
                Vec3::new(-(1.0 - delta), 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            [origin, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)],
        ];
        let normals = c2_triangles.map(|triangle| {
            triangle[1]
                .sub(triangle[0])
                .cross(triangle[2].sub(triangle[0]))
        });
        assert_eq!(
            normals[0].dot(normals[1].cross(normals[2])),
            0.0,
            "C2 f64 Cramer denominator did not cancel"
        );

        (
            construct_edge_triangle_intersection(c1_triangle, p, q, weld_step),
            construct_coplanar_segment_intersection(first, second, ProjectionAxis::Z, weld_step),
            construct_three_triangle_intersection(c2_triangles, weld_step),
        )
    };

    let (c1, c3, c2) = run(2.0f64.powi(-27));
    let c1_meta = match c1 {
        ConstructionOutcome::Resolved { tier, rho, .. } => (tier, rho),
        ConstructionOutcome::Deferred { rho } => {
            panic!("DD-resolvable C1 cancellation was deferred at rho={rho:e}")
        }
    };
    let c3_meta = match c3 {
        ConstructionOutcome::Resolved { tier, rho, .. } => (tier, rho),
        ConstructionOutcome::Deferred { rho } => {
            panic!("DD-resolvable C3 cancellation was deferred at rho={rho:e}")
        }
    };
    let c2_meta = match c2 {
        ConstructionOutcome::Resolved { tier, rho, .. } => (tier, rho),
        ConstructionOutcome::Deferred { rho } => {
            panic!("DD-resolvable C2 cancellation was deferred at rho={rho:e}")
        }
    };
    for (tier, rho) in [c1_meta, c3_meta, c2_meta] {
        assert_eq!(tier, PrecisionTier::DoubleDouble);
        assert!(rho > 1.0e-20, "selected DD rho was not returned: {rho:e}");
    }

    let (c1, c3, c2) = run(2.0f64.powi(-52));
    for rho in [
        match c1 {
            ConstructionOutcome::Deferred { rho } => rho,
            ConstructionOutcome::Resolved { rho, .. } => {
                panic!("true C1 DD-floor case resolved at rho={rho:e}")
            }
        },
        match c3 {
            ConstructionOutcome::Deferred { rho } => rho,
            ConstructionOutcome::Resolved { rho, .. } => {
                panic!("true C3 DD-floor case resolved at rho={rho:e}")
            }
        },
        match c2 {
            ConstructionOutcome::Deferred { rho } => rho,
            ConstructionOutcome::Resolved { rho, .. } => {
                panic!("true C2 DD-floor case resolved at rho={rho:e}")
            }
        },
    ] {
        assert!(rho > 0.0 && rho < 1.0e-26, "unexpected floor rho={rho:e}");
    }
}

#[test]
// AI-FUNC-SUMMARY: Pin C3's independent Rule N5 floor checks with asymmetric defining-edge permanents, including the ordering where only the non-coordinate edge falls below the floor; side effects: none.
fn c3_validates_both_selected_tier_edge_ratios_independently() {
    let scale = 1.0e-6;
    let multiplier = 1.0e3;
    let delta = 1.0e-3;
    let first = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(multiplier * scale * (1.0 + delta), multiplier * scale, 0.0),
    ];
    let second = [
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(1.0 + scale, 1.0 + scale * (1.0 - delta), 0.0),
    ];
    let ratios = |first: [Vec3; 2], second: [Vec3; 2]| {
        let (op, perm_p) =
            orient2d_axis_value_permanent(first[0], first[1], second[0], ProjectionAxis::Z);
        let (oq, perm_q) =
            orient2d_axis_value_permanent(first[0], first[1], second[1], ProjectionAxis::Z);
        let (oa, perm_a) =
            orient2d_axis_value_permanent(second[0], second[1], first[0], ProjectionAxis::Z);
        let (ob, perm_b) =
            orient2d_axis_value_permanent(second[0], second[1], first[1], ProjectionAxis::Z);
        (
            (oa - ob).abs() / (perm_a + perm_b),
            (op - oq).abs() / (perm_p + perm_q),
        )
    };
    let weld_step = 1.0e-20;
    let unit_roundoff = f64::EPSILON * 0.5;
    let dd_floor = 4.0 * 7.0 * unit_roundoff * unit_roundoff / weld_step;
    let base_rhos = ratios(first, second);
    assert!(
        base_rhos.0 > dd_floor,
        "first={base_rhos:?} floor={dd_floor:e}"
    );
    assert!(
        base_rhos.1 < dd_floor,
        "second={base_rhos:?} floor={dd_floor:e}"
    );
    assert!(matches!(
        construct_coplanar_segment_intersection(first, second, ProjectionAxis::Z, weld_step),
        ConstructionOutcome::Deferred { rho } if rho > 0.0 && rho < dd_floor
    ));

    let swapped_rhos = ratios(second, first);
    assert!(
        swapped_rhos.0 < dd_floor,
        "first={swapped_rhos:?} floor={dd_floor:e}"
    );
    assert!(
        swapped_rhos.1 > dd_floor,
        "second={swapped_rhos:?} floor={dd_floor:e}"
    );
    assert!(matches!(
        construct_coplanar_segment_intersection(second, first, ProjectionAxis::Z, weld_step),
        ConstructionOutcome::Deferred { rho } if rho > 0.0 && rho < dd_floor
    ));
}

#[test]
fn small_pipeline_emits_s02_and_returns_not_available_for_s9() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("cube.stl");
    let output = temp.path().join("mesh.vtu");
    save_stl(
        &input,
        &triangle([
            Vec3::new(0.1, 0.1, 0.5),
            Vec3::new(0.9, 0.1, 0.5),
            Vec3::new(0.5, 0.9, 0.5),
        ]),
        "triangle",
    )
    .unwrap();
    let yaml = format!(
        "meshgen:\n  inputs:\n    - stl: {}\n  domain: {{ min: [0,0,0], max: [1,1,1] }}\n  snapshots: key\n  output: {{ vtu: {} }}\n",
        input.display(),
        output.display()
    );
    let config: MeshGenConfig = serde_yaml::from_str(&yaml).unwrap();
    let error = MeshGenPipeline { config }.run().unwrap_err().to_string();
    assert!(error.contains("S9..S11"), "{error}");
    let snapshot = temp.path().join("mesh.debug/mesh_s02_arranged.vtu");
    assert!(
        snapshot.exists(),
        "s02_arranged must be emitted after G2-4/G2-5 complete"
    );
}

#[test]
fn pipeline_s00_and_s01_snapshots_pass_the_contract_verifier() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("triangle.stl");
    let output = temp.path().join("mesh.vtu");
    save_stl(
        &input,
        &triangle([
            Vec3::new(0.1, 0.1, 0.5),
            Vec3::new(0.9, 0.1, 0.5),
            Vec3::new(0.5, 0.9, 0.5),
        ]),
        "triangle",
    )
    .unwrap();
    let yaml = format!(
        "meshgen:\n  inputs:\n    - stl: {}\n      priority: 7\n  domain: {{ min: [0,0,0], max: [1,1,1] }}\n  snapshots: all\n  output: {{ vtu: {} }}\n",
        input.display(),
        output.display()
    );
    let config: MeshGenConfig = serde_yaml::from_str(&yaml).unwrap();
    MeshGenPipeline { config }.run().unwrap_err();
    for name in ["mesh_s00_conditioned.vtu", "mesh_s01_features.vtu"] {
        let path = temp.path().join("mesh.debug").join(name);
        let doc = load_vtu(Path::new(&path)).unwrap();
        doc.validate().unwrap();
        let report = verify_with_options(
            &doc,
            &VerifyGates::default(),
            VerifyOptions::from_path(&path),
        );
        assert_eq!(report.fail, 0, "{name}: {:?}", report.fired_codes());
        assert_eq!(doc.field_array("ComponentY").unwrap().data.get_i64(0), 7);
    }
}

#[test]
fn production_normalization_is_translation_invariant_before_s0() {
    let temp = tempfile::tempdir().unwrap();
    let run_case = |name: &str, offset: f64| {
        let input = temp.path().join(format!("{name}.stl"));
        let output = temp.path().join(format!("{name}.vtu"));
        save_stl(
            &input,
            &triangle([
                Vec3::new(offset + 0.125, offset + 0.125, offset + 0.5),
                Vec3::new(offset + 0.875, offset + 0.125, offset + 0.5),
                Vec3::new(offset + 0.5, offset + 0.875, offset + 0.5),
            ]),
            name,
        )
        .unwrap();
        let yaml = format!(
            "meshgen:\n  inputs:\n    - stl: {input}\n  domain: {{ min: [{offset},{offset},{offset}], max: [{max},{max},{max}] }}\n  snapshots: all\n  output: {{ vtu: {output} }}\n",
            input = input.display(),
            max = offset + 1.0,
            output = output.display(),
        );
        let config: MeshGenConfig = serde_yaml::from_str(&yaml).unwrap();
        MeshGenPipeline { config }.run().unwrap_err();
        let debug = temp.path().join(format!("{name}.debug"));
        (
            load_vtu(&debug.join(format!("{name}_s00_conditioned.vtu"))).unwrap(),
            load_vtu(&debug.join(format!("{name}_s01_features.vtu"))).unwrap(),
        )
    };
    let base = run_case("base", 0.0);
    let translated = run_case("translated", 1_000_000.0);
    for (base, translated) in [(&base.0, &translated.0), (&base.1, &translated.1)] {
        assert_eq!(base.connectivity, translated.connectivity);
        assert_eq!(base.offsets, translated.offsets);
        assert_eq!(base.types, translated.types);
        assert_eq!(base.cell_data, translated.cell_data);
        assert_eq!(base.point_data, translated.point_data);
        let base_points: Vec<_> = base
            .points
            .iter()
            .map(|point| Vec3::new(point.x, point.y, point.z))
            .collect();
        let translated_points: Vec<_> = translated
            .points
            .iter()
            .map(|point| {
                Vec3::new(
                    point.x - 1_000_000.0,
                    point.y - 1_000_000.0,
                    point.z - 1_000_000.0,
                )
            })
            .collect();
        assert_eq!(base_points, translated_points);
        for name in [
            "NIdSetOffsets",
            "NIdSetComponents",
            "FaceTagOffsets",
            "FaceTagComponents",
            "FaceTagKind",
            "FaceTagSideElems",
            "ComponentX",
            "ComponentY",
            "ComponentKind",
            "ComponentClosed",
            "CurveKind",
            "CurveCompOffsets",
            "CurveCompComponents",
        ] {
            assert_eq!(
                base.field_array(name),
                translated.field_array(name),
                "{name}"
            );
        }
    }
}

#[test]
fn same_component_proper_crossing_survives_for_s2b_rebuild() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.2, 1.0, 0.0),
            Vec3::new(0.0, -0.5, -1.0),
            Vec3::new(0.0, -0.5, 1.0),
            Vec3::new(0.0, 0.8, 1.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 3, b: 4, c: 5 }],
    };
    let arranged = arrange(&[mesh]);
    assert_eq!(arranged.stats.proper_intersections, 1);
    assert_eq!(arranged.registry.segments[0].components, [1, 1]);
    assert_eq!(
        arranged
            .curves
            .iter()
            .find(|curve| curve.kind == ArrangedCurveKind::Intersection)
            .unwrap()
            .radial_patches
            .len(),
        4
    );
}

#[test]
fn registry_and_children_are_deterministic_under_face_order_reversal() {
    let vertices = vec![
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(-1.0, 1.0, 0.0),
    ];
    let forward = Mesh {
        vertices: vertices.clone(),
        faces: vec![Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 0, b: 2, c: 3 }],
    };
    let reverse = Mesh {
        vertices,
        faces: vec![Triangle { a: 0, b: 2, c: 3 }, Triangle { a: 0, b: 1, c: 2 }],
    };
    let cutter = triangle([
        Vec3::new(0.0, -1.2, -1.0),
        Vec3::new(0.0, 1.2, -1.0),
        Vec3::new(0.0, 0.0, 2.0),
    ]);
    let a = arrange(&[forward, cutter.clone()]);
    let b = arrange(&[reverse, cutter]);
    let a_prov: Vec<_> = a
        .registry
        .vertices
        .iter()
        .map(|vertex| vertex.provenance.clone())
        .collect();
    let b_prov: Vec<_> = b
        .registry
        .vertices
        .iter()
        .map(|vertex| vertex.provenance.clone())
        .collect();
    assert_eq!(a_prov, b_prov);
    assert_eq!(
        a.registry
            .segments
            .iter()
            .map(|segment| segment.key)
            .collect::<Vec<_>>(),
        b.registry
            .segments
            .iter()
            .map(|segment| segment.key)
            .collect::<Vec<_>>()
    );
    assert_eq!(a.vertices, b.vertices);
    assert_eq!(a.faces, b.faces);
}

#[test]
// AI-FUNC-SUMMARY: Verify registry IDs follow smallest canonical provenance-group order even when that order is the reverse of geometric NodeKey order; side effects: none.
fn registry_ids_are_symbolic_group_order_not_spatial_order() {
    let right_a = triangle([
        Vec3::new(0.5, -0.5, 0.0),
        Vec3::new(1.8, -0.5, 0.0),
        Vec3::new(1.0, 0.8, 0.0),
    ]);
    let right_b = triangle([
        Vec3::new(1.0, -0.2, -1.0),
        Vec3::new(1.0, -0.2, 1.0),
        Vec3::new(1.0, 0.6, 1.0),
    ]);
    let left_a = triangle([
        Vec3::new(-1.8, -0.8, 0.0),
        Vec3::new(-0.4, -0.8, 0.0),
        Vec3::new(-1.8, 0.6, 0.0),
    ]);
    let left_b = triangle([
        Vec3::new(-1.5, -0.5, 0.0),
        Vec3::new(-0.1, -0.5, 0.0),
        Vec3::new(-1.5, 0.9, 0.0),
    ]);
    let arranged = arrange(&[right_a, right_b, left_a, left_b]);
    for (id, vertex) in arranged.registry.vertices.iter().enumerate() {
        assert_eq!(vertex.id, id as u32);
        assert_eq!(vertex.provenance, *vertex.aliases.iter().min().unwrap());
    }
    assert!(arranged
        .registry
        .vertices
        .windows(2)
        .all(|pair| pair[0].provenance < pair[1].provenance));
    let right_edge_tri = arranged
        .registry
        .vertices
        .iter()
        .find(|vertex| {
            vertex.point.x > 0.0 && matches!(vertex.provenance, IsectProv::EdgeTri { .. })
        })
        .expect("missing right-side EdgeTri group");
    let left_edge_edge = arranged
        .registry
        .vertices
        .iter()
        .find(|vertex| {
            vertex.point.x < 0.0 && matches!(vertex.provenance, IsectProv::EdgeEdge { .. })
        })
        .expect("missing left-side EdgeEdge group");
    assert!(right_edge_tri.id < left_edge_edge.id);
    assert!(right_edge_tri.point.x > left_edge_edge.point.x);
}

// AI-FUNC-SUMMARY: Test whether an arranged result contains one typed coincidence case; returns bool; side effects: none.
fn has_case(surface: &rustmspt::meshgen::ArrangedSurface, case: CoincidenceCase) -> bool {
    surface
        .coincidence_events
        .iter()
        .any(|event| event.case == case)
}

// AI-FUNC-SUMMARY: Collect every undirected edge emitted by arranged atomic faces; returns deterministic edge set; side effects: none.
fn arranged_face_edges(surface: &rustmspt::meshgen::ArrangedSurface) -> BTreeSet<(usize, usize)> {
    surface
        .faces
        .iter()
        .flat_map(|face| {
            [
                (face.nodes[0], face.nodes[1]),
                (face.nodes[1], face.nodes[2]),
                (face.nodes[2], face.nodes[0]),
            ]
        })
        .map(|(a, b)| if a <= b { (a, b) } else { (b, a) })
        .collect()
}

// AI-FUNC-SUMMARY: Collect canonical arranged-face geometry whose component tags are all in an allowed set; returns deterministic coordinate-bit triangles; side effects: none.
fn arranged_face_geometry(
    surface: &rustmspt::meshgen::ArrangedSurface,
    allowed: &BTreeSet<i32>,
) -> BTreeSet<[(u64, u64, u64); 3]> {
    surface
        .faces
        .iter()
        .filter(|face| {
            face.components
                .iter()
                .all(|component| allowed.contains(component))
        })
        .map(|face| {
            let mut points = face.nodes.map(|node| surface.vertices[node]);
            points.sort_by(|left, right| {
                left.x
                    .total_cmp(&right.x)
                    .then_with(|| left.y.total_cmp(&right.y))
                    .then_with(|| left.z.total_cmp(&right.z))
            });
            points.map(|point| (point.x.to_bits(), point.y.to_bits(), point.z.to_bits()))
        })
        .collect()
}

// AI-FUNC-SUMMARY: Collect canonical arranged-curve segment geometry whose component incidence is all allowed; returns deterministic coordinate-bit edges; side effects: none.
fn arranged_curve_geometry(
    surface: &rustmspt::meshgen::ArrangedSurface,
    allowed: &BTreeSet<i32>,
) -> BTreeSet<[(u64, u64, u64); 2]> {
    surface
        .curves
        .iter()
        .filter(|curve| {
            curve
                .components
                .iter()
                .all(|component| allowed.contains(component))
        })
        .flat_map(|curve| curve.nodes.windows(2))
        .map(|edge| {
            let mut points = [surface.vertices[edge[0]], surface.vertices[edge[1]]];
            points.sort_by(|left, right| {
                left.x
                    .total_cmp(&right.x)
                    .then_with(|| left.y.total_cmp(&right.y))
                    .then_with(|| left.z.total_cmp(&right.z))
            });
            points.map(|point| (point.x.to_bits(), point.y.to_bits(), point.z.to_bits()))
        })
        .collect()
}

#[test]
fn coincidence_policy_helpers_cover_all_thirty_case_mode_combinations() {
    let cases = [
        CoincidenceCase::C1,
        CoincidenceCase::C2,
        CoincidenceCase::C3,
        CoincidenceCase::C4,
        CoincidenceCase::C5,
        CoincidenceCase::C6,
        CoincidenceCase::C7,
        CoincidenceCase::C8,
        CoincidenceCase::C9,
        CoincidenceCase::C10,
    ];
    for case in cases {
        assert!(!case.rejected_by(CoincidencePolicy::Merge));
        assert!(!case.warned_by(CoincidencePolicy::Merge));
        assert!(!case.rejected_by(CoincidencePolicy::Warn));
        assert!(case.warned_by(CoincidencePolicy::Warn));
        assert_eq!(
            case.rejected_by(CoincidencePolicy::Reject),
            matches!(
                case,
                CoincidenceCase::C1
                    | CoincidenceCase::C2
                    | CoincidenceCase::C3
                    | CoincidenceCase::C7
                    | CoincidenceCase::C8
                    | CoincidenceCase::C9
            )
        );
        assert!(!case.warned_by(CoincidencePolicy::Reject));
    }
    let options = ArrangeOptions::new(
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, 1.0, 1.0),
        1.0e-6,
        Vec::new(),
    );
    assert_eq!(options.coincidence, CoincidencePolicy::Merge);
    assert_eq!(
        options
            .with_coincidence(CoincidencePolicy::Reject)
            .coincidence,
        CoincidencePolicy::Reject
    );
}

#[test]
fn c1_c2_full_coincidence_merge_warn_and_reject_preserve_orientations() {
    let points = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    for (case, second) in [
        (CoincidenceCase::C1, triangle(points)),
        (
            CoincidenceCase::C2,
            triangle([points[0], points[2], points[1]]),
        ),
    ] {
        let meshes = [triangle(points), second];
        let merged = arrange_with(
            &meshes,
            1.0e-6,
            CoincidencePolicy::Merge,
            sheet_components(&[0, 0]),
        )
        .unwrap();
        assert!(has_case(&merged, case));
        assert_eq!(merged.faces.len(), 1);
        assert_eq!(merged.faces[0].source_triangles.as_slice(), &[0, 1]);
        assert_eq!(merged.faces[0].components.as_slice(), &[1, 2]);
        assert_eq!(merged.faces[0].tag_orientations.len(), 2);
        assert_eq!(merged.faces[0].tag_orientations[0], 1);
        assert_eq!(
            merged.faces[0].tag_orientations[1],
            if case == CoincidenceCase::C1 { 1 } else { -1 }
        );

        let warned = arrange_with(
            &meshes,
            1.0e-6,
            CoincidencePolicy::Warn,
            sheet_components(&[0, 0]),
        )
        .unwrap();
        assert!(warned.warnings.iter().any(|event| event.case == case));
        let error = arrange_with(
            &meshes,
            1.0e-6,
            CoincidencePolicy::Reject,
            sheet_components(&[0, 0]),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("[ARR-COINC]"), "{error}");
        assert!(error.contains(&format!("{case:?}")), "{error}");
    }
}

#[test]
// AI-FUNC-SUMMARY: Verify fully coincident patches with opposite source diagonals promote pairwise overlay atoms to patch-level C1/C2 events; side effects: none.
fn retessellated_full_patch_is_classified_as_c1_or_c2() {
    let vertices = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    let first = Mesh {
        vertices: vertices.clone(),
        faces: vec![Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 0, b: 2, c: 3 }],
    };
    let second = Mesh {
        vertices: vertices.clone(),
        faces: vec![Triangle { a: 0, b: 1, c: 3 }, Triangle { a: 1, b: 2, c: 3 }],
    };
    let arranged = arrange_with(
        &[first.clone(), second],
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(
        has_case(&arranged, CoincidenceCase::C1),
        "events={:?} vertices={:?} faces={:?}",
        arranged.coincidence_events,
        arranged.vertices,
        arranged.faces
    );
    assert!(!has_case(&arranged, CoincidenceCase::C3));
    assert!(!arranged
        .curves
        .iter()
        .any(|curve| curve.kind == ArrangedCurveKind::Intersection));
    assert!(arranged
        .faces
        .iter()
        .all(|face| face.components.as_slice() == [1, 2]));
    assert!(!arranged
        .degraded
        .iter()
        .any(|item| item.reason == DegradedReason::ResidualCrossing));

    let opposite = Mesh {
        vertices,
        faces: vec![Triangle { a: 0, b: 3, c: 1 }, Triangle { a: 1, b: 3, c: 2 }],
    };
    let opposite = arrange_with(
        &[first, opposite],
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&opposite, CoincidenceCase::C2));
    assert!(!has_case(&opposite, CoincidenceCase::C3));
    assert!(!opposite
        .curves
        .iter()
        .any(|curve| curve.kind == ArrangedCurveKind::Intersection));
    assert!(opposite.faces.iter().all(|face| {
        face.components.as_slice() == [1, 2] && face.tag_orientations.as_slice() == [1, -1]
    }));
}

#[test]
fn c3_partial_overlay_emits_shared_and_exclusive_atomic_patches() {
    let a = triangle([
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.5, 0.0, 0.0),
        Vec3::new(0.0, 1.5, 0.0),
    ]);
    let b = triangle([
        Vec3::new(0.25, 0.25, 0.0),
        Vec3::new(1.75, 0.25, 0.0),
        Vec3::new(0.25, 1.75, 0.0),
    ]);
    let arranged = arrange_with(
        &[a, b],
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&arranged, CoincidenceCase::C3));
    assert!(arranged.faces.iter().any(|face| face.components.len() == 2));
    assert!(arranged
        .faces
        .iter()
        .any(|face| face.components.as_slice() == [1]));
    assert!(arranged
        .faces
        .iter()
        .any(|face| face.components.as_slice() == [2]));
    assert!(arranged
        .curves
        .iter()
        .any(|curve| curve.kind == ArrangedCurveKind::Intersection && curve.components.len() == 2));
    let doc = arranged_surface_to_doc(&arranged);
    assert!(doc.field_array("FaceTagOrientation").is_some());
}

#[test]
// AI-FUNC-SUMMARY: Verify C3 keeps source triangulation fragments constrained but emits curves only where shared atomic support meets exclusive support; side effects: none.
fn c3_curves_exclude_source_triangulation_seams() {
    let outer = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 0, b: 2, c: 3 }],
    };
    let inner = Mesh {
        vertices: vec![
            Vec3::new(0.5, 0.5, 0.0),
            Vec3::new(1.5, 0.5, 0.0),
            Vec3::new(1.5, 1.5, 0.0),
            Vec3::new(0.5, 1.5, 0.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 3 }, Triangle { a: 1, b: 2, c: 3 }],
    };
    let arranged = arrange_with(
        &[outer, inner],
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&arranged, CoincidenceCase::C3));
    let curves: Vec<_> = arranged
        .curves
        .iter()
        .filter(|curve| {
            curve.kind == ArrangedCurveKind::Intersection && curve.components.as_slice() == [1, 2]
        })
        .collect();
    assert!(!curves.is_empty());
    for curve in curves {
        let points = [
            arranged.vertices[curve.nodes[0]],
            arranged.vertices[curve.nodes[1]],
        ];
        let midpoint = points[0].add(points[1]).scale(0.5);
        assert!(
            midpoint.x == 0.5 || midpoint.x == 1.5 || midpoint.y == 0.5 || midpoint.y == 1.5,
            "source seam leaked into C3 curves: {points:?}"
        );
    }
    let face_edges = arranged_face_edges(&arranged);
    let mut constrained_seam: Vec<usize> = arranged
        .vertices
        .iter()
        .enumerate()
        .filter_map(|(node, point)| {
            (point.z == 0.0 && point.x == point.y && (0.0..=2.0).contains(&point.x)).then_some(node)
        })
        .collect();
    constrained_seam.sort_by(|left, right| {
        arranged.vertices[*left]
            .x
            .total_cmp(&arranged.vertices[*right].x)
    });
    for edge in constrained_seam.windows(2) {
        assert!(face_edges.contains(&(edge[0].min(edge[1]), edge[0].max(edge[1]))));
    }
}

#[test]
// AI-FUNC-SUMMARY: Verify disconnected retessellated coincidence is promoted with patch-local orientation while a separate partial overlay for the same component pair remains C3; side effects: none.
fn coincidence_promotion_is_patch_local_and_can_coexist_with_c3() {
    let full = [
        Vec3::new(-1.8, -0.5, 0.0),
        Vec3::new(-0.8, -0.5, 0.0),
        Vec3::new(-0.8, 0.5, 0.0),
        Vec3::new(-1.8, 0.5, 0.0),
    ];
    let first = Mesh {
        vertices: full
            .into_iter()
            .chain([
                Vec3::new(0.0, -0.75, 0.0),
                Vec3::new(1.2, -0.75, 0.0),
                Vec3::new(0.0, 0.75, 0.0),
            ])
            .collect(),
        faces: vec![
            Triangle { a: 0, b: 1, c: 2 },
            Triangle { a: 0, b: 2, c: 3 },
            Triangle { a: 4, b: 5, c: 6 },
        ],
    };
    let second = Mesh {
        vertices: full
            .into_iter()
            .chain([
                Vec3::new(0.25, -0.5, 0.0),
                Vec3::new(1.45, -0.5, 0.0),
                Vec3::new(0.25, 1.0, 0.0),
            ])
            .collect(),
        faces: vec![
            Triangle { a: 0, b: 3, c: 1 },
            Triangle { a: 1, b: 3, c: 2 },
            Triangle { a: 4, b: 5, c: 6 },
        ],
    };
    let arranged = arrange_with(
        &[first, second],
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&arranged, CoincidenceCase::C2));
    assert!(has_case(&arranged, CoincidenceCase::C3));
    assert!(!has_case(&arranged, CoincidenceCase::C1));
    for curve in arranged.curves.iter().filter(|curve| {
        curve.kind == ArrangedCurveKind::Intersection && curve.components.as_slice() == [1, 2]
    }) {
        assert!(
            curve
                .nodes
                .iter()
                .all(|node| arranged.vertices[*node].x > -0.1),
            "promoted C2 patch emitted a C3 curve: {curve:?}"
        );
    }
}

#[test]
// AI-FUNC-SUMMARY: Verify a disconnected C7 patch does not suppress exact patch-local C1/C2 promotion for the same component pair; side effects: none.
fn c7_is_patch_local_to_coincidence_promotion() {
    let eps = 1.0e-3;
    let full = [
        Vec3::new(-1.8, -0.5, 0.0),
        Vec3::new(-0.8, -0.5, 0.0),
        Vec3::new(-0.8, 0.5, 0.0),
        Vec3::new(-1.8, 0.5, 0.0),
    ];
    let near = [
        Vec3::new(0.0, -0.5, 0.0),
        Vec3::new(1.0, -0.5, 0.0),
        Vec3::new(0.0, 0.5, 0.0),
    ];
    let first = Mesh {
        vertices: full.into_iter().chain(near).collect(),
        faces: vec![
            Triangle { a: 0, b: 1, c: 2 },
            Triangle { a: 0, b: 2, c: 3 },
            Triangle { a: 4, b: 5, c: 6 },
        ],
    };
    let second = Mesh {
        vertices: full
            .into_iter()
            .chain(near.map(|point| point.add(Vec3::new(0.0, 0.0, 0.5 * eps))))
            .collect(),
        faces: vec![
            Triangle { a: 0, b: 3, c: 1 },
            Triangle { a: 1, b: 3, c: 2 },
            Triangle { a: 4, b: 5, c: 6 },
        ],
    };
    let arranged = arrange_with(
        &[first, second],
        eps,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&arranged, CoincidenceCase::C7));
    assert!(has_case(&arranged, CoincidenceCase::C2));
    assert!(!has_case(&arranged, CoincidenceCase::C3));
}

#[test]
fn c4_c5_c6_contacts_continue_in_reject_mode_with_typed_features() {
    let c4 = [
        triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]),
        triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
        ]),
    ];
    for policy in [
        CoincidencePolicy::Merge,
        CoincidencePolicy::Warn,
        CoincidencePolicy::Reject,
    ] {
        let result = arrange_with(&c4, 1.0e-6, policy, sheet_components(&[0, 0])).unwrap();
        assert!(has_case(&result, CoincidenceCase::C4));
        assert_eq!(
            result
                .warnings
                .iter()
                .any(|event| event.case == CoincidenceCase::C4),
            policy == CoincidencePolicy::Warn
        );
    }
    let edge = arrange_with(
        &c4,
        1.0e-6,
        CoincidencePolicy::Reject,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&edge, CoincidenceCase::C4));
    assert!(edge
        .curves
        .iter()
        .any(|curve| curve.kind == ArrangedCurveKind::Intersection));

    let c5 = [
        c4[0].clone(),
        triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
        ]),
    ];
    for policy in [
        CoincidencePolicy::Merge,
        CoincidencePolicy::Warn,
        CoincidencePolicy::Reject,
    ] {
        let result = arrange_with(&c5, 1.0e-6, policy, sheet_components(&[0, 0])).unwrap();
        assert!(has_case(&result, CoincidenceCase::C5));
        assert_eq!(
            result
                .warnings
                .iter()
                .any(|event| event.case == CoincidenceCase::C5),
            policy == CoincidencePolicy::Warn
        );
    }
    let vertex = arrange_with(
        &c5,
        1.0e-6,
        CoincidencePolicy::Reject,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&vertex, CoincidenceCase::C5));
    assert!(vertex
        .point_features
        .iter()
        .any(|feature| feature.case == CoincidenceCase::C5));

    let c6 = [
        c4[0].clone(),
        triangle([
            Vec3::new(0.25, 0.25, 0.0),
            Vec3::new(0.25, 0.25, 1.0),
            Vec3::new(0.75, 0.25, 1.0),
        ]),
    ];
    for policy in [
        CoincidencePolicy::Merge,
        CoincidencePolicy::Warn,
        CoincidencePolicy::Reject,
    ] {
        let result = arrange_with(&c6, 1.0e-6, policy, sheet_components(&[0, 0])).unwrap();
        assert!(has_case(&result, CoincidenceCase::C6));
        assert_eq!(
            result
                .warnings
                .iter()
                .any(|event| event.case == CoincidenceCase::C6),
            policy == CoincidencePolicy::Warn
        );
    }
    let tangent = arrange_with(
        &c6,
        1.0e-6,
        CoincidencePolicy::Reject,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&tangent, CoincidenceCase::C6));
    assert!(tangent
        .point_features
        .iter()
        .any(|feature| feature.case == CoincidenceCase::C6));
}

#[test]
// AI-FUNC-SUMMARY: Verify a point contact does not leak the other surface's component into arranged S1 rim-curve ownership; side effects: none.
fn arranged_feature_curves_preserve_s1_components_at_point_contacts() {
    let meshes = [
        triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]),
        triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
        ]),
    ];
    let arranged = arrange_with(
        &meshes,
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    let ownership: BTreeSet<Vec<i32>> = arranged
        .curves
        .iter()
        .filter(|curve| curve.kind == ArrangedCurveKind::Rim)
        .map(|curve| curve.components.iter().copied().collect())
        .collect();
    assert!(ownership.contains(&vec![1]), "ownership={ownership:?}");
    assert!(ownership.contains(&vec![2]), "ownership={ownership:?}");
    assert!(!ownership.contains(&vec![1, 2]), "ownership={ownership:?}");
}

#[test]
// AI-FUNC-SUMMARY: Verify a zero-sign endpoint plus an interior C1 endpoint remains a conforming C4 segment rather than collapsing to C6; side effects: none.
fn endpoint_plus_interior_crossing_is_a_constrained_contact_segment() {
    let base = triangle([
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
    ]);
    let touching = triangle([
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::new(0.5, -0.5, 1.0),
        Vec3::new(0.5, 1.5, -0.5),
    ]);
    let arranged = arrange_with(
        &[base, touching],
        1.0e-6,
        CoincidencePolicy::Reject,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&arranged, CoincidenceCase::C4));
    assert!(!has_case(&arranged, CoincidenceCase::C6));
    let face_edges = arranged_face_edges(&arranged);
    let contacts: Vec<_> = arranged
        .curves
        .iter()
        .filter(|curve| {
            curve.kind == ArrangedCurveKind::Intersection && curve.components.as_slice() == [1, 2]
        })
        .collect();
    assert!(!contacts.is_empty());
    for curve in contacts {
        for edge in curve.nodes.windows(2) {
            let edge = if edge[0] <= edge[1] {
                (edge[0], edge[1])
            } else {
                (edge[1], edge[0])
            };
            assert!(
                face_edges.contains(&edge),
                "missing constrained edge {edge:?}"
            );
        }
    }
    assert!(arranged
        .registry
        .vertices
        .iter()
        .any(|vertex| vertex.aliases.len() > 1));
}

#[test]
fn c7_epsilon_boundary_uses_lower_triangle_geometry_and_inclusive_distance() {
    let base = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    let eps = 1.0e-3;
    let inside = base.map(|point| Vec3::new(point.x, point.y, eps));
    let merged = arrange_with(
        &[triangle(base), triangle(inside)],
        eps,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&merged, CoincidenceCase::C7));
    assert_eq!(merged.faces.len(), 1);
    assert!(merged.vertices.iter().all(|point| point.z == 0.0));

    let outside = base.map(|point| Vec3::new(point.x, point.y, eps + 4.0 * f64::EPSILON));
    let separate = arrange_with(
        &[triangle(base), triangle(outside)],
        eps,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(!has_case(&separate, CoincidenceCase::C7));
    assert_eq!(separate.faces.len(), 2);
}

#[test]
// AI-FUNC-SUMMARY: Pin C7 for a slightly tilted mutually matched patch and prevent transitive clusters from exceeding epsilon diameter; side effects: none.
fn c7_accepts_tilt_but_rejects_transitive_overmerge() {
    let base = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    let eps = 1.0e-3;
    let tilted = [
        Vec3::new(0.0, 0.0, 0.4 * eps),
        Vec3::new(1.0, 0.0, 0.6 * eps),
        Vec3::new(0.0, 1.0, 0.5 * eps),
    ];
    let tilted_result = arrange_with(
        &[triangle(base), triangle(tilted)],
        eps,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    assert!(has_case(&tilted_result, CoincidenceCase::C7));
    assert_eq!(tilted_result.faces.len(), 1);

    let layers = [0.0, 0.75 * eps, 1.5 * eps]
        .map(|z| triangle(base.map(|point| Vec3::new(point.x, point.y, z))));
    let chained = arrange_with(
        &layers,
        eps,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0, 0]),
    )
    .unwrap();
    assert_eq!(
        chained
            .coincidence_events
            .iter()
            .filter(|event| event.case == CoincidenceCase::C7)
            .count(),
        2
    );
    assert_eq!(chained.faces.len(), 2);
    assert!(chained
        .degraded
        .iter()
        .any(|item| item.reason == DegradedReason::QuantizedOrderAmbiguity));
    assert!(chained.vertices.iter().any(|point| point.z == 0.0));
    assert!(chained
        .vertices
        .iter()
        .any(|point| (point.z - 1.5 * eps).abs() <= f64::EPSILON));
}

#[test]
fn c8_c9_and_c10_semantics_are_typed_and_follow_frozen_reject_column() {
    let coincident = [
        triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]),
        triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]),
    ];
    for policy in [CoincidencePolicy::Merge, CoincidencePolicy::Warn] {
        let result = arrange_with(&coincident, 1.0e-6, policy, sheet_components(&[2, 7])).unwrap();
        assert!(has_case(&result, CoincidenceCase::C8));
        assert_eq!(
            result
                .warnings
                .iter()
                .any(|event| event.case == CoincidenceCase::C8),
            policy == CoincidencePolicy::Warn
        );
    }
    let priorities = arrange_with(
        &coincident,
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_components(&[2, 7]),
    )
    .unwrap();
    assert!(has_case(&priorities, CoincidenceCase::C8));
    let priority_error = arrange_with(
        &coincident,
        1.0e-6,
        CoincidencePolicy::Reject,
        sheet_components(&[2, 7]),
    )
    .unwrap_err()
    .to_string();
    assert!(priority_error.contains("C8"), "{priority_error}");

    let mut sheet_solid = sheet_components(&[0, 0]);
    sheet_solid[0].kind = 0;
    for policy in [CoincidencePolicy::Merge, CoincidencePolicy::Warn] {
        let result = arrange_with(&coincident, 1.0e-6, policy, sheet_solid.clone()).unwrap();
        assert!(has_case(&result, CoincidenceCase::C9));
        assert_eq!(
            result
                .warnings
                .iter()
                .any(|event| event.case == CoincidenceCase::C9),
            policy == CoincidencePolicy::Warn
        );
    }
    let mixed = arrange_with(
        &coincident,
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_solid.clone(),
    )
    .unwrap();
    assert!(has_case(&mixed, CoincidenceCase::C9));
    assert_eq!(mixed.faces[0].components.as_slice(), &[1, 2]);
    let mixed_error = arrange_with(&coincident, 1.0e-6, CoincidencePolicy::Reject, sheet_solid)
        .unwrap_err()
        .to_string();
    assert!(mixed_error.contains("C9"), "{mixed_error}");

    let domain_sheet = triangle([
        Vec3::new(-1.0, -1.0, -2.0),
        Vec3::new(1.0, -1.0, -2.0),
        Vec3::new(0.0, 1.0, -2.0),
    ]);
    let domain = arrange_with(
        &[domain_sheet],
        1.0e-6,
        CoincidencePolicy::Reject,
        sheet_components(&[0]),
    )
    .unwrap();
    let c10 = domain
        .coincidence_events
        .iter()
        .find(|event| event.case == CoincidenceCase::C10)
        .unwrap();
    assert!(matches!(c10.entities[1], CoincidenceEntity::DomainFace(4)));
    assert!(!domain
        .curves
        .iter()
        .any(|curve| curve.components.as_slice() == [1]
            && curve.kind == ArrangedCurveKind::Intersection));
}

#[test]
// AI-FUNC-SUMMARY: Pin C10 to positive-area overlap with the bounded domain face for inside, partial, outside, SAT-disjoint-AABB, line, and point contacts; side effects: none.
fn c10_requires_positive_area_overlap_with_the_bounded_domain_face() {
    let is_c10 = |points: [Vec3; 3]| {
        let arranged = arrange_with(
            &[triangle(points)],
            1.0e-6,
            CoincidencePolicy::Reject,
            sheet_components(&[0]),
        )
        .unwrap();
        has_case(&arranged, CoincidenceCase::C10)
    };
    assert!(is_c10([
        Vec3::new(-1.0, -1.0, -2.0),
        Vec3::new(1.0, -1.0, -2.0),
        Vec3::new(0.0, 1.0, -2.0),
    ]));
    assert!(is_c10([
        Vec3::new(-3.0, 0.0, -2.0),
        Vec3::new(0.0, -1.0, -2.0),
        Vec3::new(0.0, 1.0, -2.0),
    ]));
    assert!(!is_c10([
        Vec3::new(3.0, 3.0, -2.0),
        Vec3::new(4.0, 3.0, -2.0),
        Vec3::new(3.0, 4.0, -2.0),
    ]));
    assert!(!is_c10([
        Vec3::new(1.5, 3.0, -2.0),
        Vec3::new(3.0, 1.5, -2.0),
        Vec3::new(3.0, 3.0, -2.0),
    ]));
    assert!(!is_c10([
        Vec3::new(2.0, -1.0, -2.0),
        Vec3::new(2.0, 1.0, -2.0),
        Vec3::new(3.0, 0.0, -2.0),
    ]));
    assert!(!is_c10([
        Vec3::new(2.0, 2.0, -2.0),
        Vec3::new(3.0, 2.0, -2.0),
        Vec3::new(2.0, 3.0, -2.0),
    ]));
    assert!(!is_c10([
        Vec3::new(1.0, 3.0, -2.0),
        Vec3::new(3.0, 1.0, -2.0),
        Vec3::new(3.0, 3.0, -2.0),
    ]));
}

#[test]
// AI-FUNC-SUMMARY: Verify partial shared subpatches retain C8 priority and C9 sheet-solid semantic events in addition to C3; side effects: none.
fn c3_partial_overlay_retains_c8_and_c9_semantics() {
    let a = triangle([
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.5, 0.0, 0.0),
        Vec3::new(0.0, 1.5, 0.0),
    ]);
    let b = triangle([
        Vec3::new(0.25, 0.25, 0.0),
        Vec3::new(1.75, 0.25, 0.0),
        Vec3::new(0.25, 1.75, 0.0),
    ]);
    let mut components = sheet_components(&[2, 7]);
    components[0].kind = 0;
    let arranged = arrange_with(&[a, b], 1.0e-6, CoincidencePolicy::Merge, components).unwrap();
    for case in [
        CoincidenceCase::C3,
        CoincidenceCase::C8,
        CoincidenceCase::C9,
    ] {
        assert!(has_case(&arranged, case), "missing {case:?}");
    }
}

#[test]
// AI-FUNC-SUMMARY: Exercise real C3-C10 fixtures under every merge/warn/reject policy rather than testing only policy helper booleans; side effects: none.
fn coincidence_geometry_fixtures_cover_all_policy_modes() {
    let partial = [
        triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.5, 0.0, 0.0),
            Vec3::new(0.0, 1.5, 0.0),
        ]),
        triangle([
            Vec3::new(0.25, 0.25, 0.0),
            Vec3::new(1.75, 0.25, 0.0),
            Vec3::new(0.25, 1.75, 0.0),
        ]),
    ];
    for policy in [
        CoincidencePolicy::Merge,
        CoincidencePolicy::Warn,
        CoincidencePolicy::Reject,
    ] {
        let result = arrange_with(&partial, 1.0e-6, policy, sheet_components(&[0, 0]));
        if policy == CoincidencePolicy::Reject {
            assert!(result.unwrap_err().to_string().contains("C3"));
        } else {
            let arranged = result.unwrap();
            assert!(has_case(&arranged, CoincidenceCase::C3));
            assert_eq!(
                arranged
                    .warnings
                    .iter()
                    .any(|event| event.case == CoincidenceCase::C3),
                policy == CoincidencePolicy::Warn
            );
        }
    }

    let base = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    let near = base.map(|point| Vec3::new(point.x, point.y, 5.0e-4));
    for policy in [
        CoincidencePolicy::Merge,
        CoincidencePolicy::Warn,
        CoincidencePolicy::Reject,
    ] {
        let result = arrange_with(
            &[triangle(base), triangle(near)],
            1.0e-3,
            policy,
            sheet_components(&[0, 0]),
        );
        if policy == CoincidencePolicy::Reject {
            assert!(result.unwrap_err().to_string().contains("C7"));
        } else {
            let arranged = result.unwrap();
            assert!(has_case(&arranged, CoincidenceCase::C7));
            assert_eq!(
                arranged
                    .warnings
                    .iter()
                    .any(|event| event.case == CoincidenceCase::C7),
                policy == CoincidencePolicy::Warn
            );
        }
    }

    let domain_sheet = triangle([
        Vec3::new(-1.0, -1.0, -2.0),
        Vec3::new(1.0, -1.0, -2.0),
        Vec3::new(0.0, 1.0, -2.0),
    ]);
    for policy in [
        CoincidencePolicy::Merge,
        CoincidencePolicy::Warn,
        CoincidencePolicy::Reject,
    ] {
        let arranged = arrange_with(
            std::slice::from_ref(&domain_sheet),
            1.0e-6,
            policy,
            sheet_components(&[0]),
        )
        .unwrap();
        assert!(has_case(&arranged, CoincidenceCase::C10));
        assert_eq!(
            arranged
                .warnings
                .iter()
                .any(|event| event.case == CoincidenceCase::C10),
            policy == CoincidencePolicy::Warn
        );
    }
}

#[test]
fn multiway_coplanar_overlay_and_source_order_are_deterministic() {
    let meshes = [
        triangle([
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ]),
        triangle([
            Vec3::new(0.25, 0.25, 0.0),
            Vec3::new(1.75, 0.25, 0.0),
            Vec3::new(0.25, 1.75, 0.0),
        ]),
        triangle([
            Vec3::new(0.5, 0.5, 0.0),
            Vec3::new(1.5, 0.5, 0.0),
            Vec3::new(0.5, 1.5, 0.0),
        ]),
    ];
    let forward = arrange_with(
        &meshes,
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0, 0]),
    )
    .unwrap();
    let mut reversed = meshes.clone();
    reversed.reverse();
    let reverse = arrange_with(
        &reversed,
        1.0e-6,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0, 0]),
    )
    .unwrap();
    assert!(forward.faces.iter().any(|face| face.components.len() == 3));
    let forward_geometry: BTreeSet<_> = forward
        .faces
        .iter()
        .map(|face| {
            let mut points = face.nodes.map(|node| forward.vertices[node]);
            points.sort_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y)));
            points.map(|point| (point.x.to_bits(), point.y.to_bits(), point.z.to_bits()))
        })
        .collect();
    let reverse_geometry: BTreeSet<_> = reverse
        .faces
        .iter()
        .map(|face| {
            let mut points = face.nodes.map(|node| reverse.vertices[node]);
            points.sort_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y)));
            points.map(|point| (point.x.to_bits(), point.y.to_bits(), point.z.to_bits()))
        })
        .collect();
    assert_eq!(forward_geometry, reverse_geometry);
}

#[test]
// AI-FUNC-SUMMARY: Verify an unrelated off-plane third-surface node inside another edge's q band cannot subdivide source/overlay/feature geometry, including reversed input order; side effects: none.
fn q_close_off_plane_nodes_are_not_discovered_globally() {
    let eps = 1.0e-3;
    let first = triangle([
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.5, 0.0, 0.0),
        Vec3::new(0.0, 1.5, 0.0),
    ]);
    let second = triangle([
        Vec3::new(0.25, 0.25, 0.0),
        Vec3::new(1.75, 0.25, 0.0),
        Vec3::new(0.25, 1.75, 0.0),
    ]);
    let unrelated = triangle([
        Vec3::new(0.75, 0.0, 0.075 * eps),
        Vec3::new(0.5, -0.4, 0.2),
        Vec3::new(1.0, -0.4, 0.2),
    ]);
    let baseline = arrange_with(
        &[first.clone(), second.clone()],
        eps,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0]),
    )
    .unwrap();
    let forward = arrange_with(
        &[first.clone(), second.clone(), unrelated.clone()],
        eps,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0, 0]),
    )
    .unwrap();
    let reverse = arrange_with(
        &[unrelated, second, first],
        eps,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0, 0]),
    )
    .unwrap();

    let pair = BTreeSet::from([1, 2]);
    assert_eq!(
        arranged_face_geometry(&baseline, &pair),
        arranged_face_geometry(&forward, &pair)
    );
    assert_eq!(
        arranged_curve_geometry(&baseline, &pair),
        arranged_curve_geometry(&forward, &pair)
    );
    let all = BTreeSet::from([1, 2, 3]);
    assert_eq!(
        arranged_face_geometry(&forward, &all),
        arranged_face_geometry(&reverse, &all)
    );
    assert_eq!(
        arranged_curve_geometry(&forward, &all),
        arranged_curve_geometry(&reverse, &all)
    );
}

#[test]
fn crossing_spade_constraints_are_refused_without_panicking() {
    let points = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(0.25, 0.75, 0.0),
        Vec3::new(1.25, 0.75, 0.0),
        Vec3::new(0.75, 0.25, 0.0),
        Vec3::new(0.75, 1.25, 0.0),
    ];
    let constraints = BTreeSet::from([(3usize, 4usize), (5usize, 6usize)]);
    let result = std::panic::catch_unwind(|| {
        triangulate_parent(
            [0, 1, 2],
            &constraints,
            BTreeSet::from([3, 4, 5, 6]),
            &points,
            1.0e-6,
        )
    });
    assert!(result.is_ok(), "try_add_constraint path panicked");
    let error = result.unwrap().unwrap_err().to_string();
    assert!(error.contains("refused local CDT constraint"), "{error}");
}

#[test]
// AI-FUNC-SUMMARY: Verify non-finite Spade point insertion errors propagate as ARR-RESID InvalidMesh instead of panicking or being ignored; side effects: none.
fn spade_insertion_errors_are_propagated() {
    let points = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(f64::NAN, 0.25, 0.0),
    ];
    let error = triangulate_parent(
        [0, 1, 2],
        &BTreeSet::new(),
        BTreeSet::from([3]),
        &points,
        1.0e-6,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("Spade insertion failed"), "{error}");
}

#[test]
fn g2_3_epsilon_q_and_ambiguity_calibration_is_typed_and_deterministic() {
    let eps = 0.1;
    let base = triangle([
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ]);
    for (offset, expected) in [
        (eps * (1.0 - 8.0 * f64::EPSILON), true),
        (eps * (1.0 + 8.0 * f64::EPSILON), false),
        (0.1 * eps * (1.0 + 8.0 * f64::EPSILON), true),
    ] {
        let shifted = triangle([
            Vec3::new(0.0, 0.0, offset),
            Vec3::new(1.0, 0.0, offset),
            Vec3::new(0.0, 1.0, offset),
        ]);
        let arranged = arrange_with(
            &[base.clone(), shifted],
            eps,
            CoincidencePolicy::Merge,
            sheet_components(&[0, 0]),
        )
        .unwrap();
        assert_eq!(has_case(&arranged, CoincidenceCase::C7), expected);
    }

    let ambiguous = triangle([
        Vec3::new(0.0, 0.0, 0.05),
        Vec3::new(0.02, 0.0, 0.05),
        Vec3::new(0.0, 1.0, 0.05),
    ]);
    let run = || {
        arrange_with(
            &[base.clone(), ambiguous.clone()],
            eps,
            CoincidencePolicy::Merge,
            sheet_components(&[0, 0]),
        )
        .unwrap()
    };
    let first = run();
    let second = run();
    assert_eq!(first.degraded, second.degraded);
    let neighborhood = first
        .degraded
        .iter()
        .find(|item| item.reason == DegradedReason::QuantizedOrderAmbiguity)
        .expect("ambiguous mutual match must remain public");
    assert_eq!(neighborhood.triangles.as_slice(), &[0, 1]);
    assert!(!neighborhood.points.is_empty());
}

#[test]
// AI-FUNC-SUMMARY: Verify q-aliased EdgeEdge proposals retain all non-normative owner TriIds in typed degradation while their normative provenance remains unchanged; side effects: none.
fn edge_edge_alias_degradation_retains_owner_triangles() {
    let base = triangle([
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
    ]);
    let first = triangle([
        Vec3::new(0.5, -1.0, 0.0),
        Vec3::new(0.5, 1.0, 0.0),
        Vec3::new(1.0, 0.5, 0.0),
    ]);
    let second = triangle([
        Vec3::new(0.2, -1.0, 0.0),
        Vec3::new(0.80004, 1.0, 0.0),
        Vec3::new(1.1, 0.5, 0.0),
    ]);
    let arranged = arrange_with(
        &[base, first, second],
        1.0e-3,
        CoincidencePolicy::Merge,
        sheet_components(&[0, 0, 0]),
    )
    .unwrap();
    let alias = arranged
        .degraded
        .iter()
        .find(|neighborhood| {
            neighborhood.reason == DegradedReason::QuantizedOrderAmbiguity
                && neighborhood
                    .provenances
                    .iter()
                    .filter(|provenance| matches!(provenance, IsectProv::EdgeEdge { .. }))
                    .count()
                    >= 2
        })
        .expect("missing EdgeEdge alias degradation");
    assert_eq!(alias.triangles.as_slice(), &[0, 1, 2]);
    assert!(alias
        .provenances
        .iter()
        .all(|provenance| matches!(provenance, IsectProv::EdgeEdge { .. })));
}

#[test]
// AI-FUNC-SUMMARY: Exercise the public arrangement PrecisionFloor trigger with a well-sized triangle and an edge whose DD-resolved crossing ratio is truly below Rule N5's floor; side effects: none.
fn precision_floor_degradation_is_public_and_deterministic() {
    let delta = 2.0f64.powi(-52);
    let origin = Vec3::new(0.0, 0.0, 0.0);
    let tangent = Vec3::new(1.0 + delta, 1.0, 0.0);
    let vertical = Vec3::new(0.0, 0.0, 1.0);
    let plane = triangle([origin, tangent, vertical]);
    let contact_point = vertical.scale(0.25);
    let contact = triangle([
        contact_point,
        contact_point.add(Vec3::new(1.0, 1.0 - delta, 0.0)),
        contact_point.add(Vec3::new(0.0, 1.0, 1.0)),
    ]);
    let run = || arrange_unit_scale(&[plane.clone(), contact.clone()], 1.0e-3).unwrap();
    let first = run();
    let second = run();
    assert_eq!(first.degraded, second.degraded);
    let floor = first
        .degraded
        .iter()
        .find(|item| item.reason == DegradedReason::PrecisionFloor)
        .unwrap_or_else(|| {
            panic!("true DD-floor geometry did not produce a public fallback: {first:?}")
        });
    assert_eq!(floor.triangles.as_slice(), &[0, 1]);
    assert!(!floor.provenances.is_empty());
    assert!(floor.points.len() >= 2);
    assert!(matches!(floor.rho, Some(rho) if rho > 0.0 && rho < 1.0e-26));
    assert_eq!(first.stats.precision_floor_routes, 1);
}

#[test]
// AI-FUNC-SUMMARY: Exercise deterministic CollapsedContactSegment degradation with a sub-q proper-contact slice between otherwise well-sized triangles; side effects: none.
fn collapsed_contact_segment_degradation_is_public_and_deterministic() {
    let eps = 1.0e-3;
    let base = triangle([
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ]);
    let offset = 0.01 * eps;
    let cutter = triangle([
        Vec3::new(0.0, 0.0, -offset),
        Vec3::new(0.0, -1.0, 1.0),
        Vec3::new(0.0, 1.0, 1.0),
    ]);
    let run = || {
        arrange_with(
            &[base.clone(), cutter.clone()],
            eps,
            CoincidencePolicy::Merge,
            sheet_components(&[0, 0]),
        )
        .unwrap()
    };
    let first = run();
    let second = run();
    assert_eq!(first.degraded, second.degraded);
    let collapsed = first
        .degraded
        .iter()
        .find(|item| item.reason == DegradedReason::CollapsedContactSegment)
        .expect("sub-q contact segment was not retained");
    assert_eq!(collapsed.triangles.as_slice(), &[0, 1]);
    assert!(collapsed.points.len() >= 2);
}

#[test]
fn near_degenerate_filter_and_dd_signs_match_exact_predicate() {
    let mut rng = StdRng::seed_from_u64(0x4732_3100);
    for index in 0..100_000 {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        let exponent = 8 + index % 18;
        let magnitude = 10.0f64.powi(-exponent);
        let z = if index % 2 == 0 {
            magnitude
        } else {
            -magnitude
        };
        let d = Vec3::new(rng.gen_range(-1.0..1.0), rng.gen_range(-1.0..1.0), z);
        let exact = orient3d(a, b, c, d).total_cmp(&0.0);
        let dd = orient3d_dd_value(a, b, c, d).to_f64().total_cmp(&0.0);
        let (filtered_sign, filtered, det, _) = orient3d_filtered(a, b, c, d);
        assert_eq!(dd, exact);
        if filtered {
            assert_eq!(filtered_sign.cmp(&0), exact);
            assert_eq!(det.total_cmp(&0.0), exact);
        }
    }
}

#[test]
// AI-FUNC-SUMMARY: Run the frozen 250-case G2-3 corpus twice, requiring deterministic committed/degraded outcomes and no G2-2/G2-3 NotAvailable route; side effects: prints aggregate calibration counts when uncaptured.
fn near_degenerate_pair_fuzz_has_no_g2_2_or_g2_3_not_available_route() {
    let mut rng = StdRng::seed_from_u64(0x4732_3101);
    let mut degraded_cases = 0usize;
    let mut precision_floor_routes = 0usize;
    let mut dd_escalations = 0usize;
    for index in 0..250 {
        let exponent = 5 + index % 10;
        let delta = 10.0f64.powi(-exponent);
        let edge_offset = delta * rng.gen_range(0.25..4.0);
        let base = triangle([
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]);
        let cutter = triangle([
            Vec3::new(-delta, -1.0 + edge_offset, -delta),
            Vec3::new(delta, -1.0 + edge_offset, delta),
            Vec3::new(0.5 * delta, 0.75, 2.0 * delta),
        ]);
        let conditioned =
            condition_surface(&[base, cutter], 1.0e-14, RepairLevel::Conservative).unwrap();
        let features = detect_features(&conditioned, 45.0);
        let options = ArrangeOptions {
            domain_min: Vec3::new(-2.0, -2.0, -2.0),
            domain_max: Vec3::new(2.0, 2.0, 2.0),
            eps: 1.0e-14,
            coincidence: CoincidencePolicy::Merge,
            components: vec![
                ArrangeComponent {
                    x: 1,
                    priority: 0,
                    kind: 1,
                    closed: false,
                },
                ArrangeComponent {
                    x: 2,
                    priority: 1,
                    kind: 1,
                    closed: false,
                },
            ],
        };
        let first = arrange_surface(&conditioned, &features, &options);
        let second = arrange_surface(&conditioned, &features, &options);
        match (first, second) {
            (Ok(arranged), Ok(repeated)) => {
                assert_eq!(arranged, repeated, "nondeterministic fuzz case {index}");
                assert!(arranged.faces.iter().all(|face| face
                    .nodes
                    .iter()
                    .all(|node| *node < arranged.vertices.len())));
                degraded_cases += usize::from(!arranged.degraded.is_empty());
                precision_floor_routes += arranged.stats.precision_floor_routes;
                dd_escalations += arranged.stats.precision_escalations;
            }
            (Err(RustMsptError::NotAvailable(message)), _)
            | (_, Err(RustMsptError::NotAvailable(message))) => {
                panic!("G2-2/G2-3 must not defer calibration geometry: {message}")
            }
            (Err(error), _) | (_, Err(error)) => {
                panic!("near-degenerate case {index} was not safely degraded: {error}")
            }
        }
    }
    println!(
        "G2-3 calibration: cases=250 degraded_cases={degraded_cases} precision_floor_routes={precision_floor_routes} dd_escalations={dd_escalations} hard_failures=0"
    );
    assert_eq!(dd_escalations, 500, "frozen corpus DD total changed");
    assert_eq!(
        precision_floor_routes, 0,
        "frozen corpus floor total changed"
    );
    assert_eq!(degraded_cases, 0, "frozen corpus degraded total changed");
}

#[test]
fn emitting_s0_s1_s2_modules_have_no_random_state_hashmap_or_raw_orient3d() {
    for (name, source) in [
        ("arrange", include_str!("../src/meshgen/arrange.rs")),
        ("surface", include_str!("../src/meshgen/surface.rs")),
        ("features", include_str!("../src/meshgen/features.rs")),
    ] {
        assert!(!source.contains("HashMap"), "{name} contains HashMap");
        assert!(
            !source.contains("robust::orient3d"),
            "{name} bypasses the project orientation wrapper"
        );
    }
}

// AI-FUNC-SUMMARY: Build one closed axis-aligned cube mesh with outward normals; returns Mesh; side effects: none.
fn solid_cube(min: Vec3, max: Vec3) -> Mesh {
    let v = |x: bool, y: bool, z: bool| {
        Vec3::new(
            if x { max.x } else { min.x },
            if y { max.y } else { min.y },
            if z { max.z } else { min.z },
        )
    };
    Mesh {
        vertices: vec![
            v(false, false, false),
            v(true, false, false),
            v(true, true, false),
            v(false, true, false),
            v(false, false, true),
            v(true, false, true),
            v(true, true, true),
            v(false, true, true),
        ],
        faces: vec![
            Triangle { a: 0, b: 2, c: 1 },
            Triangle { a: 0, b: 3, c: 2 },
            Triangle { a: 4, b: 5, c: 6 },
            Triangle { a: 4, b: 6, c: 7 },
            Triangle { a: 0, b: 1, c: 5 },
            Triangle { a: 0, b: 5, c: 4 },
            Triangle { a: 3, b: 6, c: 2 },
            Triangle { a: 3, b: 7, c: 6 },
            Triangle { a: 0, b: 7, c: 3 },
            Triangle { a: 0, b: 4, c: 7 },
            Triangle { a: 1, b: 2, c: 6 },
            Triangle { a: 1, b: 6, c: 5 },
        ],
    }
}

// AI-FUNC-SUMMARY: Build one solid ArrangeComponent row; returns ArrangeComponent; side effects: none.
fn solid_component(x: i32) -> ArrangeComponent {
    ArrangeComponent {
        x,
        priority: 0,
        kind: 0,
        closed: true,
    }
}

#[test]
fn solid_cube_clipped_to_smaller_box_produces_box_cap_faces() {
    let cube = solid_cube(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
    let arranged = arrange_with(
        &[cube],
        1.0e-6,
        CoincidencePolicy::Merge,
        vec![solid_component(1)],
    )
    .unwrap();
    let clip_min = Vec3::new(-2.0, -2.0, -0.5);
    let clip_max = Vec3::new(2.0, 2.0, 0.5);
    let clipped = clip_arranged_to_box(&arranged, clip_min, clip_max, 1.0e-6).unwrap();

    let cap_faces: Vec<&ArrangedFace> = clipped.faces.iter().filter(|f| f.box_tagged).collect();
    assert!(
        !cap_faces.is_empty(),
        "expected box cap faces after clipping a solid"
    );
    for cap in &cap_faces {
        assert!(
            cap.source_triangles.contains(&TriId::MAX),
            "cap face should carry the box sentinel source triangle"
        );
        assert_eq!(cap.components.as_slice(), &[1]);
        assert_eq!(cap.tag_orientations.as_slice(), &[1]);
    }
    let planes = [(2usize, clip_min.z), (2, clip_max.z)];
    for (axis, value) in planes {
        let has_cap = cap_faces.iter().any(|face| {
            face.nodes.iter().all(|node| {
                let point = clipped.vertices[*node];
                axis_coord_test(point, axis) == value
            })
        });
        assert!(
            has_cap,
            "expected a box cap face on axis {axis} at value {value}"
        );
    }
    let box_curves: Vec<_> = clipped
        .curves
        .iter()
        .filter(|c| c.kind == ArrangedCurveKind::Box)
        .collect();
    assert!(
        !box_curves.is_empty(),
        "expected box-clip cap boundary curves"
    );
    for curve in &box_curves {
        assert_eq!(curve.components.as_slice(), &[1]);
    }
}

#[test]
fn sheet_clipped_open_has_no_cap_faces() {
    let sheet = Mesh {
        vertices: vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 0, b: 2, c: 3 }],
    };
    let arranged = arrange(&[sheet]);
    let clip_min = Vec3::new(-0.5, -0.5, -0.5);
    let clip_max = Vec3::new(0.5, 0.5, 0.5);
    let clipped = clip_arranged_to_box(&arranged, clip_min, clip_max, 1.0e-6).unwrap();
    let cap_faces: Vec<&ArrangedFace> = clipped.faces.iter().filter(|f| f.box_tagged).collect();
    assert!(
        cap_faces.is_empty(),
        "sheet components clipped open must not produce cap faces"
    );
    let box_curves: Vec<_> = clipped
        .curves
        .iter()
        .filter(|c| c.kind == ArrangedCurveKind::Box)
        .collect();
    assert!(
        box_curves.is_empty(),
        "sheet components must not produce box-clip cap boundary curves"
    );
}

#[test]
fn outside_box_geometry_is_dropped() {
    let inside = triangle([
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.3, 0.0, 0.0),
        Vec3::new(0.0, 0.3, 0.0),
    ]);
    let outside = triangle([
        Vec3::new(5.0, 5.0, 5.0),
        Vec3::new(6.0, 5.0, 5.0),
        Vec3::new(5.0, 6.0, 5.0),
    ]);
    let arranged = arrange(&[inside, outside]);
    let before = arranged.faces.len();
    assert!(before >= 2);
    let clip_min = Vec3::new(-1.0, -1.0, -1.0);
    let clip_max = Vec3::new(1.0, 1.0, 1.0);
    let clipped = clip_arranged_to_box(&arranged, clip_min, clip_max, 1.0e-6).unwrap();
    for face in &clipped.faces {
        for node in face.nodes {
            let point = clipped.vertices[node];
            assert!(point.x >= clip_min.x && point.x <= clip_max.x);
            assert!(point.y >= clip_min.y && point.y <= clip_max.y);
            assert!(point.z >= clip_min.z && point.z <= clip_max.z);
        }
    }
    let outside_face_count = clipped.faces.iter().filter(|f| f.component == 2).count();
    assert_eq!(
        outside_face_count, 0,
        "faces from the outside mesh should be dropped"
    );
    let inside_face_count = clipped.faces.iter().filter(|f| f.component == 1).count();
    assert!(
        inside_face_count > 0,
        "faces from the inside mesh should survive"
    );
}

#[test]
fn c10_sheet_coincident_with_domain_face_gets_box_tag() {
    let sheet = Mesh {
        vertices: vec![
            Vec3::new(-1.0, -1.0, 0.5),
            Vec3::new(1.0, -1.0, 0.5),
            Vec3::new(1.0, 1.0, 0.5),
            Vec3::new(-1.0, 1.0, 0.5),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 0, b: 2, c: 3 }],
    };
    let arranged = arrange(&[sheet]);
    let clip_min = Vec3::new(-0.5, -0.5, -0.5);
    let clip_max = Vec3::new(0.5, 0.5, 0.5);
    let clipped = clip_arranged_to_box(&arranged, clip_min, clip_max, 1.0e-6).unwrap();
    assert!(
        !clipped.faces.is_empty(),
        "sheet on the domain face should survive clipping"
    );
    for face in &clipped.faces {
        assert!(
            face.box_tagged,
            "sheet face on the z=max domain plane should be box-tagged (C10)"
        );
    }
}

#[test]
fn curves_straddling_box_boundary_are_split() {
    let cube = solid_cube(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
    let arranged = arrange_with(
        &[cube],
        1.0e-6,
        CoincidencePolicy::Merge,
        vec![solid_component(1)],
    )
    .unwrap();
    let before_curve_count = arranged.curves.len();
    assert!(
        before_curve_count > 0,
        "a cube should have sharp-edge curves after arrangement"
    );
    let clip_min = Vec3::new(-2.0, -2.0, -0.5);
    let clip_max = Vec3::new(2.0, 2.0, 0.5);
    let clipped = clip_arranged_to_box(&arranged, clip_min, clip_max, 1.0e-6).unwrap();
    for curve in &clipped.curves {
        for node in &curve.nodes {
            let point = clipped.vertices[*node];
            assert!(point.x >= clip_min.x && point.x <= clip_max.x);
            assert!(point.y >= clip_min.y && point.y <= clip_max.y);
            assert!(point.z >= clip_min.z && point.z <= clip_max.z);
        }
    }
    let has_box_curve = clipped
        .curves
        .iter()
        .any(|c| c.kind == ArrangedCurveKind::Box);
    assert!(
        has_box_curve,
        "clipping a solid should produce at least one box-clip cap boundary curve"
    );
    let non_box_count = clipped
        .curves
        .iter()
        .filter(|c| c.kind != ArrangedCurveKind::Box)
        .count();
    assert!(
        non_box_count > 0,
        "original sharp-edge curves should survive clipping"
    );
}

// AI-FUNC-SUMMARY: Return the coordinate of one Vec3 along the given axis for tests; returns f64; side effects: none.
fn axis_coord_test(point: Vec3, axis: usize) -> f64 {
    match axis {
        0 => point.x,
        1 => point.y,
        _ => point.z,
    }
}
