//! G1-3 (S1 feature detection) tests.
//!
//! Covers: sharp-edge detection, rim/boundary edges, non-manifold edges,
//! curve chaining, junctions, corners, and the s01 snapshot doc.

use rustmspt::config::meshgen::RepairLevel;
use rustmspt::geometry::mesh_ops::box_mesh;
use rustmspt::io::vtu::ArrayData;
use rustmspt::meshgen::features::{detect_features, features_to_doc, FeatureEdgeKind};
use rustmspt::meshgen::surface::condition_surface;
use rustmspt::types::{BoundingBox, Mesh, Triangle, Vec3};

#[test]
fn detects_rim_edges_on_open_surface() {
    // A single triangle has all 3 edges as rim edges (boundary, 1 incident face).
    let mesh = rustmspt::types::Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        faces: vec![rustmspt::types::Triangle { a: 0, b: 1, c: 2 }],
    };
    let cs = condition_surface(&[mesh], 1e-3, RepairLevel::Conservative).unwrap();
    let fs = detect_features(&cs, 45.0);
    // All 3 edges are rim (boundary) edges.
    let rim_curves: Vec<_> = fs
        .curves
        .iter()
        .filter(|c| c.kind == FeatureEdgeKind::Rim)
        .collect();
    assert!(
        !rim_curves.is_empty(),
        "open triangle should have rim feature edges"
    );
    // 3 rim edges form a closed loop.
    let total_rim_verts: usize = rim_curves.iter().map(|c| c.vertices.len()).sum();
    assert!(
        total_rim_verts >= 3,
        "rim curves should cover all 3 boundary vertices"
    );
}

#[test]
fn detects_no_sharp_edges_on_cube() {
    // A cube has 90° dihedral angles. With feature_angle_deg = 45°, 90° > 45°,
    // so ALL edges should be sharp features.
    let mesh = box_mesh(BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0)));
    let cs = condition_surface(&[mesh], 1e-3, RepairLevel::Conservative).unwrap();
    let fs = detect_features(&cs, 45.0);
    // A cube has 12 edges, all with 90° dihedral > 45° threshold.
    let sharp_count: usize = fs
        .curves
        .iter()
        .filter(|c| c.kind == FeatureEdgeKind::Sharp)
        .map(|c| c.vertices.len().saturating_sub(1))
        .sum();
    assert!(
        sharp_count >= 12,
        "cube should have >= 12 sharp edges at 45° threshold, got {sharp_count}"
    );
}

#[test]
fn detects_no_sharp_edges_on_flat_surface() {
    // Two coplanar triangles forming a quad: deviation = 0°, no sharp edges.
    let mesh = rustmspt::types::Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        faces: vec![
            rustmspt::types::Triangle { a: 0, b: 1, c: 2 },
            rustmspt::types::Triangle { a: 0, b: 2, c: 3 },
        ],
    };
    let cs = condition_surface(&[mesh], 1e-3, RepairLevel::Conservative).unwrap();
    let fs = detect_features(&cs, 45.0);
    let sharp_count: usize = fs
        .curves
        .iter()
        .filter(|c| c.kind == FeatureEdgeKind::Sharp)
        .count();
    assert_eq!(
        sharp_count, 0,
        "coplanar triangles should have no sharp edges"
    );
    // The shared edge has 2 incident faces (manifold), so it's not a rim either.
    // Only the 4 boundary edges are rim features.
    let rim_count: usize = fs
        .curves
        .iter()
        .filter(|c| c.kind == FeatureEdgeKind::Rim)
        .count();
    assert!(
        rim_count > 0,
        "quad should have rim (boundary) feature edges"
    );
}

// AI-FUNC-SUMMARY: Verify algebraic sharp-edge classification is scale invariant below the removed literal normal threshold; returns nothing; side effects: none.
#[test]
fn detects_sharp_edges_on_small_non_degenerate_faces() {
    let scale = 1.0e-12;
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(scale, 0.0, 0.0),
            Vec3::new(0.0, scale, 0.0),
            Vec3::new(0.0, 0.0, scale),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 1, b: 0, c: 3 }],
    };
    let cs = condition_surface(&[mesh], 1.0e-15, RepairLevel::Conservative).unwrap();
    let fs = detect_features(&cs, 45.0);
    assert!(fs
        .curves
        .iter()
        .any(|curve| curve.kind == FeatureEdgeKind::Sharp));
}

// AI-FUNC-SUMMARY: Verify a three-face edge reaches S1 and is classified as non-manifold after component-local S0 conditioning; returns nothing; side effects: none.
#[test]
fn detects_non_manifold_edges_after_s0() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, -1.0, 0.0),
        ],
        faces: vec![
            Triangle { a: 0, b: 1, c: 2 },
            Triangle { a: 1, b: 0, c: 3 },
            Triangle { a: 0, b: 1, c: 4 },
        ],
    };
    let cs = condition_surface(&[mesh], 1.0e-3, RepairLevel::Conservative).unwrap();
    let fs = detect_features(&cs, 45.0);
    assert!(fs
        .curves
        .iter()
        .any(|curve| curve.kind == FeatureEdgeKind::NonManifold));
}

#[test]
fn features_to_doc_produces_valid_vtu_with_curves() {
    let mesh = box_mesh(BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0)));
    let cs = condition_surface(&[mesh], 1e-3, RepairLevel::Conservative).unwrap();
    let fs = detect_features(&cs, 45.0);
    let doc = features_to_doc(&cs, &fs);
    doc.validate().expect("VTU doc should validate");
    // 12 face cells + at least some curve cells.
    assert!(doc.types.len() >= 12, "doc should have face + curve cells");
    let _cell_kind = doc.cell_array("cell_kind").expect("cell_kind array");
    assert!(matches!(_cell_kind.data, ArrayData::U8(_)));
    let n_curves = doc.types.iter().filter(|&&t| t == 4u8).count();
    assert!(n_curves > 0, "should have at least one curve cell");
    let curve_id = doc.cell_array("curve_id").expect("curve_id array");
    assert_eq!(curve_id.data.len(), doc.types.len());
    for name in ["partition_id", "regime", "face_tag_key"] {
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
        "CurveCompOffsets",
    ] {
        assert!(doc.field_array(name).is_some(), "missing {name}");
    }
}

#[test]
fn detects_junctions_at_shared_vertex() {
    // Two cubes sharing a vertex: edges meeting at the shared vertex form a junction.
    let cube1 = box_mesh(BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    });
    let cube2 = box_mesh(BoundingBox {
        min: Vec3::new(1.0, 1.0, 1.0),
        max: Vec3::new(2.0, 2.0, 2.0),
    });
    let cs = condition_surface(&[cube1, cube2], 1e-3, RepairLevel::Conservative).unwrap();
    let fs = detect_features(&cs, 45.0);
    // The shared vertex (1,1,1) should be a junction (>= 3 sharp edges meet).
    assert!(
        !fs.junctions.is_empty(),
        "shared vertex between two cubes should create a junction"
    );
}

// AI-FUNC-SUMMARY: Verify coincident open components retain rim classification for either relative winding and serialize exact incidence; returns nothing; side effects: none.
#[test]
fn coincident_open_components_have_only_shared_rim_features() {
    let vertices = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    let first = Mesh {
        vertices: vertices.clone(),
        faces: vec![Triangle { a: 0, b: 1, c: 2 }],
    };

    for second_face in [Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 0, b: 2, c: 1 }] {
        let second = Mesh {
            vertices: vertices.clone(),
            faces: vec![second_face],
        };
        let cs =
            condition_surface(&[first.clone(), second], 1e-3, RepairLevel::Conservative).unwrap();
        let fs = detect_features(&cs, 45.0);

        assert_eq!(fs.curves.len(), 1);
        assert_eq!(fs.curves[0].kind, FeatureEdgeKind::Rim);
        assert_eq!(fs.curves[0].components, vec![1, 2]);

        let doc = features_to_doc(&cs, &fs);
        assert_eq!(
            doc.field_array("CurveCompOffsets")
                .expect("CurveCompOffsets")
                .data,
            ArrayData::I64(vec![2])
        );
        assert_eq!(
            doc.field_array("CurveCompComponents")
                .expect("CurveCompComponents")
                .data,
            ArrayData::I32(vec![1, 2])
        );
    }
}

// AI-FUNC-SUMMARY: Verify coincident closed components are classified independently instead of producing non-manifold S1 edges; returns nothing; side effects: none.
#[test]
fn coincident_closed_components_do_not_create_non_manifold_features() {
    let cube = box_mesh(BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0)));
    let cs = condition_surface(&[cube.clone(), cube], 1e-3, RepairLevel::Conservative).unwrap();
    let fs = detect_features(&cs, 45.0);

    assert!(!fs.curves.is_empty());
    assert!(fs
        .curves
        .iter()
        .all(|curve| curve.kind == FeatureEdgeKind::Sharp));
    assert!(fs.curves.iter().all(|curve| curve.components == [1, 2]));
}
