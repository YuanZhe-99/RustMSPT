//! G1-2 (S0 conditioning and repair) tests.
//!
//! Covers: weld, degenerate-triangle drop, duplicate-face merge, pinhole
//! closure, orientation fix, provisional components, repair log, repair
//! levels (strict/conservative/permissive), and the s00 snapshot doc.

use rustmspt::config::meshgen::RepairLevel;
use rustmspt::geometry::mesh_ops::box_mesh;
use rustmspt::io::vtu::ArrayData;
use rustmspt::meshgen::surface::{condition_surface, condition_surface_to_doc, RepairActionType};
use rustmspt::types::{BoundingBox, Mesh, Triangle, Vec3};

// AI-FUNC-SUMMARY: Build a unit cube mesh; returns Mesh; side effects: none.
fn unit_cube() -> Mesh {
    box_mesh(BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0)))
}

// AI-FUNC-SUMMARY: Build a consistently orientable tetrahedron with the same reversed face emitted first or last; returns Mesh; side effects: none.
fn tetrahedron_with_reversed_face(reversed_first: bool) -> Mesh {
    let vertices = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let reversed = Triangle { a: 1, b: 3, c: 2 };
    let mut faces = vec![
        Triangle { a: 0, b: 3, c: 2 },
        Triangle { a: 0, b: 1, c: 3 },
        Triangle { a: 0, b: 2, c: 1 },
    ];
    if reversed_first {
        faces.insert(0, reversed);
    } else {
        faces.push(reversed);
    }
    Mesh { vertices, faces }
}

// AI-FUNC-SUMMARY: Canonicalize triangle cycles without erasing winding and sort by face identity; returns canonical faces; side effects: none.
fn canonical_winding(faces: &[[usize; 3]]) -> Vec<[usize; 3]> {
    let mut canonical: Vec<[usize; 3]> = faces
        .iter()
        .map(|face| {
            let rotations = [
                *face,
                [face[1], face[2], face[0]],
                [face[2], face[0], face[1]],
            ];
            *rotations
                .iter()
                .min()
                .expect("triangle has three rotations")
        })
        .collect();
    canonical.sort_unstable();
    canonical
}

#[test]
fn welds_duplicate_vertices() {
    // Build a cube with deliberately duplicated vertices (8 unique -> 14 total).
    let verts = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(0.0, 1.0, 1.0),
        // Duplicates (within weld tolerance q = 0.1 * eps = 1e-4).
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 1.0),
    ];
    let faces = vec![
        Triangle { a: 0, b: 1, c: 2 },
        Triangle { a: 0, b: 2, c: 3 },
        Triangle { a: 4, b: 5, c: 6 },
        Triangle { a: 4, b: 6, c: 7 },
        // Faces using duplicate vertices.
        Triangle { a: 8, b: 9, c: 10 },
        Triangle { a: 8, b: 10, c: 11 },
        Triangle { a: 12, b: 13, c: 6 },
        Triangle { a: 12, b: 6, c: 7 },
    ];
    let mesh = Mesh {
        vertices: verts,
        faces,
    };
    let cs = condition_surface(&[mesh], 1e-3, RepairLevel::Conservative).unwrap();
    // 14 input vertices, 8 unique after weld -> 6 welded.
    assert_eq!(
        cs.vertices.len(),
        8,
        "14 input verts (6 dups) should weld to 8"
    );
    assert_eq!(
        cs.stats.n_welded, 6,
        "6 duplicate vertices should have been welded"
    );
}

#[test]
fn drops_degenerate_triangles_in_conservative_mode() {
    let mut mesh = unit_cube();
    // Add a degenerate triangle (two identical vertices).
    mesh.faces.push(Triangle { a: 0, b: 0, c: 1 });
    let cs = condition_surface(&[mesh], 1e-3, RepairLevel::Conservative).unwrap();
    assert!(
        cs.stats.n_degenerate >= 1,
        "degenerate triangle should be detected"
    );
    assert_eq!(
        cs.faces.len(),
        12,
        "degenerate triangle should be dropped, leaving 12 faces"
    );
    assert!(
        cs.repair_log
            .count(RepairActionType::DegenerateTriangleDropped)
            >= 1
    );
}

#[test]
fn strict_mode_rejects_degenerate_triangle() {
    let mut mesh = unit_cube();
    mesh.faces.push(Triangle { a: 0, b: 0, c: 1 });
    let result = condition_surface(&[mesh], 1e-3, RepairLevel::Strict);
    assert!(
        result.is_err(),
        "strict mode should reject degenerate triangle"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("degenerate"),
        "error should mention degenerate: {err}"
    );
}

#[test]
fn merges_duplicate_faces_in_conservative_mode() {
    let mut mesh = unit_cube();
    // Duplicate one face.
    mesh.faces.push(mesh.faces[0].clone());
    let cs = condition_surface(&[mesh], 1e-3, RepairLevel::Conservative).unwrap();
    assert!(
        cs.stats.n_duplicate >= 1,
        "duplicate face should be detected"
    );
    assert_eq!(
        cs.faces.len(),
        12,
        "duplicate face should be merged, leaving 12 faces"
    );
    assert!(cs.repair_log.count(RepairActionType::DuplicateFaceMerged) >= 1);
}

#[test]
fn strict_mode_rejects_duplicate_face() {
    let mut mesh = unit_cube();
    mesh.faces.push(mesh.faces[0].clone());
    let result = condition_surface(&[mesh], 1e-3, RepairLevel::Strict);
    assert!(result.is_err(), "strict mode should reject duplicate face");
}

#[test]
fn detects_multiple_components() {
    let cube1 = box_mesh(BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    });
    let cube2 = box_mesh(BoundingBox {
        min: Vec3::new(5.0, 0.0, 0.0),
        max: Vec3::new(6.0, 1.0, 1.0),
    });
    let cs = condition_surface(&[cube1, cube2], 1e-3, RepairLevel::Conservative).unwrap();
    assert_eq!(
        cs.stats.n_components, 2,
        "two separated cubes = 2 components"
    );
}

#[test]
fn repair_log_has_structured_fields() {
    let mut mesh = unit_cube();
    mesh.faces.push(Triangle { a: 0, b: 0, c: 1 });
    let cs = condition_surface(&[mesh], 1e-3, RepairLevel::Conservative).unwrap();
    assert!(
        !cs.repair_log.actions.is_empty(),
        "repair log should have actions"
    );
    let action = &cs.repair_log.actions[0];
    // Every field should be populated with a meaningful value.
    assert!(!action.action_type.as_str().is_empty());
    assert!(action.tolerance > 0.0);
    assert!(action.delta_euler != 0 || action.delta_boundary_loops != 0);
}

#[test]
fn conditioned_surface_to_doc_produces_valid_vtu() {
    let mesh = unit_cube();
    let cs = condition_surface(&[mesh], 1e-3, RepairLevel::Conservative).unwrap();
    let doc = condition_surface_to_doc(&cs);
    doc.validate().expect("VTU doc should validate");
    assert_eq!(doc.types.len(), 12, "doc should have 12 face cells");
    let cell_kind = doc.cell_array("cell_kind").expect("cell_kind array");
    assert_eq!(cell_kind.data.len(), 12);
    assert!(matches!(cell_kind.data, ArrayData::U8(_)));
    let region_key = doc.cell_array("region_key").expect("region_key array");
    assert_eq!(region_key.data.len(), 12);
    for name in ["partition_id", "regime", "face_tag_key", "curve_id"] {
        assert!(doc.cell_array(name).is_some(), "missing {name}");
    }
    for name in ["n_id_key", "constraint_kind", "constraint_ref"] {
        assert!(doc.point_array(name).is_some(), "missing {name}");
    }
    for name in [
        "RegionSetOffsets",
        "RegionSetComponents",
        "RegionSetPriority",
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
        assert!(doc.field_array(name).is_some(), "missing {name}");
    }
}

#[test]
fn two_inputs_get_distinct_component_labels() {
    let cube1 = box_mesh(BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    });
    let cube2 = box_mesh(BoundingBox {
        min: Vec3::new(5.0, 0.0, 0.0),
        max: Vec3::new(6.0, 1.0, 1.0),
    });
    let cs = condition_surface(&[cube1, cube2], 1e-3, RepairLevel::Conservative).unwrap();
    let labels: std::collections::HashSet<i64> = cs.component.iter().copied().collect();
    assert!(
        labels.len() >= 2,
        "two inputs should produce at least 2 component labels"
    );
}

#[test]
fn coincident_faces_from_different_inputs_keep_both_source_identities() {
    let face = rustmspt::types::Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }],
    };
    let cs = condition_surface(&[face.clone(), face], 1e-3, RepairLevel::Conservative).unwrap();
    assert_eq!(cs.faces.len(), 2);
    assert_eq!(cs.source_component, vec![1, 2]);
    assert_eq!(cs.stats.n_duplicate, 0);
}

// AI-FUNC-SUMMARY: Verify S0 preserves equal winding for coincident faces owned by separate source components; returns nothing; side effects: none.
#[test]
fn coincident_same_winding_survives_s0() {
    let face = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }],
    };

    let cs = condition_surface(&[face.clone(), face], 1e-3, RepairLevel::Conservative).unwrap();

    assert_eq!(cs.faces, vec![[0, 1, 2], [0, 1, 2]]);
    assert_eq!(cs.source_component, vec![1, 2]);
    assert_eq!(cs.stats.n_orientation_fixed, 0);
}

// AI-FUNC-SUMMARY: Verify S0 preserves opposite winding for coincident faces owned by separate source components; returns nothing; side effects: none.
#[test]
fn coincident_opposite_winding_survives_s0() {
    let vertices = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    let forward = Mesh {
        vertices: vertices.clone(),
        faces: vec![Triangle { a: 0, b: 1, c: 2 }],
    };
    let reverse = Mesh {
        vertices,
        faces: vec![Triangle { a: 0, b: 2, c: 1 }],
    };

    let cs = condition_surface(&[forward, reverse], 1e-3, RepairLevel::Conservative).unwrap();

    assert_eq!(cs.faces, vec![[0, 1, 2], [0, 2, 1]]);
    assert_eq!(cs.source_component, vec![1, 2]);
    assert_eq!(cs.stats.n_orientation_fixed, 0);
}

// AI-FUNC-SUMMARY: Verify permissive S0 flips only a tetrahedron's minority face regardless of whether that face is emitted first or last; returns nothing; side effects: none.
#[test]
fn permissive_orientation_repair_is_minimum_flip_and_face_order_independent() {
    let reversed_first = condition_surface(
        &[tetrahedron_with_reversed_face(true)],
        1e-3,
        RepairLevel::Permissive,
    )
    .unwrap();
    let reversed_last = condition_surface(
        &[tetrahedron_with_reversed_face(false)],
        1e-3,
        RepairLevel::Permissive,
    )
    .unwrap();

    for repaired in [&reversed_first, &reversed_last] {
        assert_eq!(repaired.stats.n_orientation_fixed, 1);
        assert_eq!(
            repaired
                .repair_log
                .count(RepairActionType::OrientationFixed),
            1
        );
    }

    let expected = canonical_winding(&[[1, 2, 3], [0, 3, 2], [0, 1, 3], [0, 2, 1]]);
    assert_eq!(canonical_winding(&reversed_first.faces), expected);
    assert_eq!(
        canonical_winding(&reversed_first.faces),
        canonical_winding(&reversed_last.faces)
    );
}

// AI-FUNC-SUMMARY: Verify strict and conservative S0 reject the same required tetrahedron orientation repair for either face order; returns nothing; side effects: none.
#[test]
fn strict_and_conservative_reject_required_orientation_repair() {
    for repair_level in [RepairLevel::Strict, RepairLevel::Conservative] {
        let first_error =
            condition_surface(&[tetrahedron_with_reversed_face(true)], 1e-3, repair_level)
                .unwrap_err()
                .to_string();
        let last_error =
            condition_surface(&[tetrahedron_with_reversed_face(false)], 1e-3, repair_level)
                .unwrap_err()
                .to_string();

        assert_eq!(first_error, last_error);
        assert!(first_error.contains("1 orientation flip(s)"));
        assert!(first_error.contains("repair.level=permissive"));
    }
}

// AI-FUNC-SUMMARY: Verify a tied orientation partition keeps the canonical face unflipped rather than depending on emission order; returns nothing; side effects: none.
#[test]
fn orientation_partition_tie_break_is_canonical() {
    let vertices = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let face_a = Triangle { a: 0, b: 1, c: 2 };
    let face_b = Triangle { a: 0, b: 1, c: 3 };
    let first = Mesh {
        vertices: vertices.clone(),
        faces: vec![face_a.clone(), face_b.clone()],
    };
    let second = Mesh {
        vertices,
        faces: vec![face_b, face_a],
    };

    let first = condition_surface(&[first], 1e-3, RepairLevel::Permissive).unwrap();
    let second = condition_surface(&[second], 1e-3, RepairLevel::Permissive).unwrap();

    assert_eq!(first.stats.n_orientation_fixed, 1);
    assert_eq!(second.stats.n_orientation_fixed, 1);
    assert_eq!(
        canonical_winding(&first.faces),
        canonical_winding(&second.faces)
    );
}

// AI-FUNC-SUMMARY: Verify S0 preserves a three-face non-manifold edge for S1 incidence classification instead of forcing impossible pairwise orientation constraints; returns nothing; side effects: none.
#[test]
fn non_manifold_edges_survive_orientation_conditioning() {
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

    for repair_level in [
        RepairLevel::Strict,
        RepairLevel::Conservative,
        RepairLevel::Permissive,
    ] {
        let conditioned = condition_surface(&[mesh.clone()], 1e-3, repair_level).unwrap();
        assert_eq!(conditioned.faces.len(), 3);
        assert_eq!(conditioned.stats.n_orientation_fixed, 0);
    }
}

// AI-FUNC-SUMMARY: Verify welded shared edges never propagate S0 orientation repair across source components; returns nothing; side effects: none.
#[test]
fn orientation_repair_does_not_cross_source_components() {
    let first = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(0.0, 1.0, 1.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }],
    };
    let second = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(0.0, -1.0, 1.0),
        ],
        faces: vec![Triangle { a: 0, b: 1, c: 2 }],
    };

    let cs = condition_surface(&[first, second], 1e-3, RepairLevel::Permissive).unwrap();

    assert_eq!(cs.faces, vec![[0, 1, 2], [0, 1, 3]]);
    assert_eq!(cs.source_component, vec![1, 2]);
    assert_eq!(cs.stats.n_orientation_fixed, 0);
}

#[test]
fn pinhole_caps_retain_the_owning_source_component() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ],
        faces: vec![
            Triangle { a: 0, b: 1, c: 3 },
            Triangle { a: 1, b: 2, c: 3 },
            Triangle { a: 2, b: 0, c: 3 },
        ],
    };
    let cs = condition_surface(&[mesh], 2.0, RepairLevel::Conservative).unwrap();
    assert!(cs.stats.n_pinhole >= 1);
    assert!(cs.source_component.iter().all(|component| *component == 1));
}

// AI-FUNC-SUMMARY: Verify strict S0 reports a detected pinhole as InvalidMesh instead of logging an unapplied closure; returns nothing; side effects: none.
#[test]
fn strict_mode_rejects_pinhole_instead_of_logging_repair() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ],
        faces: vec![
            Triangle { a: 0, b: 1, c: 3 },
            Triangle { a: 1, b: 2, c: 3 },
            Triangle { a: 2, b: 0, c: 3 },
        ],
    };

    let error = condition_surface(&[mesh], 2.0, RepairLevel::Strict)
        .unwrap_err()
        .to_string();

    assert!(error.contains("pinhole boundary loop"));
    assert!(error.contains("source component 1"));
    assert!(error.contains("strict mode forbids closure"));
}
