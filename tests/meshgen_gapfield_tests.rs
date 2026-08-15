//! G3-1 S3 separation-field acceptance tests: rays, closest-pair sweep, pairing
//! battery, confidence, provisional grouping, and the s03 snapshot contract.

use rustmspt::config::meshgen::{CoincidencePolicy, RepairLevel};
use rustmspt::meshgen::gapfield::{
    compute_gap_field, gapfield_to_doc, GapField, GapFieldOptions, PairClass, SampleKind,
};
use rustmspt::meshgen::{
    arrange_surface, clip_arranged_to_box, condition_surface, detect_features, rebuild_topology,
    stamp_metadata, verify_with_options, ArrangeComponent, ArrangeOptions, CheckStatus,
    SnapshotMeta, Stage, VerifyGates, VerifyOptions,
};
use rustmspt::types::{Mesh, Triangle, Vec3};

const H_BOOTSTRAP: f64 = 0.05;
const T_LAYER: f64 = 0.05;

// AI-FUNC-SUMMARY: Build an axis-aligned closed box mesh (12 triangles, outward winding); returns Mesh; side effects: none.
fn box_mesh(min: Vec3, max: Vec3) -> Mesh {
    let vertices = vec![
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(max.x, max.y, max.z),
        Vec3::new(min.x, max.y, max.z),
    ];
    let quads: [[usize; 4]; 6] = [
        [0, 3, 2, 1], // z = min, outward -z
        [4, 5, 6, 7], // z = max, outward +z
        [0, 1, 5, 4], // y = min
        [3, 7, 6, 2], // y = max
        [0, 4, 7, 3], // x = min
        [1, 2, 6, 5], // x = max
    ];
    let mut faces = Vec::with_capacity(12);
    for quad in quads {
        faces.push(Triangle {
            a: quad[0],
            b: quad[1],
            c: quad[2],
        });
        faces.push(Triangle {
            a: quad[0],
            b: quad[2],
            c: quad[3],
        });
    }
    Mesh { vertices, faces }
}

// AI-FUNC-SUMMARY: Build a flat rectangular sheet in a coordinate plane; returns Mesh; side effects: none.
fn plate(corners: [Vec3; 4]) -> Mesh {
    Mesh {
        vertices: corners.to_vec(),
        faces: vec![
            Triangle { a: 0, b: 1, c: 2 },
            Triangle { a: 0, b: 2, c: 3 },
        ],
    }
}

// AI-FUNC-SUMMARY: Build one curved strip of a circular arc at the given radius; returns Mesh; side effects: none.
fn arc_strip(radius: f64, segments: usize, half_height: f64) -> Mesh {
    let mut vertices = Vec::new();
    let mut faces = Vec::new();
    for i in 0..=segments {
        let angle = std::f64::consts::FRAC_PI_2 * (i as f64) / (segments as f64);
        let (x, y) = (0.5 + radius * angle.cos(), 0.5 + radius * angle.sin());
        vertices.push(Vec3::new(x, y, 0.5 - half_height));
        vertices.push(Vec3::new(x, y, 0.5 + half_height));
    }
    for i in 0..segments {
        let a = 2 * i;
        faces.push(Triangle {
            a,
            b: a + 1,
            c: a + 3,
        });
        faces.push(Triangle {
            a,
            b: a + 3,
            c: a + 2,
        });
    }
    Mesh { vertices, faces }
}

// AI-FUNC-SUMMARY: Run S0 -> S1 -> S2 -> clip -> S2b -> S3 over the given meshes in the unit domain; returns GapField; side effects: none.
fn gap_field(meshes: &[Mesh], kinds: &[u8]) -> GapField {
    gap_field_with(meshes, kinds, &GapFieldOptions::default())
}

// AI-FUNC-SUMMARY: Same as `gap_field` with caller-supplied gap options (thresholds, box walls); returns GapField; side effects: none.
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
    let options = GapFieldOptions {
        domain_min,
        domain_max,
        eps,
        h_bootstrap: H_BOOTSTRAP,
        ..options.clone()
    };
    compute_gap_field(&clipped, &topo, &options)
}

// AI-FUNC-SUMMARY: Smallest group separation over groups whose pair class is not a domain-wall pairing; returns f64; side effects: none.
fn min_surface_gap(field: &GapField) -> f64 {
    field
        .groups
        .iter()
        .filter(|group| !matches!(group.pair_class, PairClass::SurfaceBox(_)))
        .map(|group| group.t_r)
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn isolated_solid_has_no_gap_pairings() {
    // Control case: one closed box, alone, far from every domain wall and thicker
    // than 2*t_layer in every direction. Nothing may pair.
    let field = gap_field(
        &[box_mesh(
            Vec3::new(0.3, 0.3, 0.3),
            Vec3::new(0.7, 0.7, 0.7),
        )],
        &[0],
    );
    assert!(field.stats.n_samples > 0, "the control case must be sampled");
    assert_eq!(
        field.stats.n_closest_pairs, 0,
        "an isolated solid has no triangle pair within t_layer across the gap"
    );
    assert!(
        min_surface_gap(field_ref(&field)).is_infinite(),
        "an isolated solid must report no finite surface-to-surface separation"
    );
}

// AI-FUNC-SUMMARY: Identity helper keeping the borrow explicit in assertions; returns the same reference; side effects: none.
fn field_ref(field: &GapField) -> &GapField {
    field
}

#[test]
fn parallel_solids_measure_the_hand_computed_gap() {
    // Dumbbell-style case: two closed boxes with a known 0.02 gap.
    let gap = 0.02;
    let lower = box_mesh(Vec3::new(0.3, 0.3, 0.2), Vec3::new(0.7, 0.7, 0.4));
    let upper = box_mesh(
        Vec3::new(0.3, 0.3, 0.4 + gap),
        Vec3::new(0.7, 0.7, 0.6 + gap),
    );
    let field = gap_field(&[lower, upper], &[0, 0]);
    let measured = min_surface_gap(&field);
    assert!(
        (measured - gap).abs() <= 0.05 * gap,
        "measured separation {measured} must be within 5% of {gap}"
    );
    assert!(
        field.stats.n_ray_paired > 0,
        "facing parallel walls must pair by normal rays"
    );
    let facing: Vec<_> = field
        .groups
        .iter()
        .filter(|group| matches!(group.pair_class, PairClass::Inter(1, 2)))
        .collect();
    assert!(
        !facing.is_empty(),
        "the two solids must produce an inter-component pair class"
    );
    for group in facing {
        assert!(
            group.confidence >= 0.9,
            "a clean parallel gap must not be a low-confidence pairing (got {})",
            group.confidence
        );
    }
}

#[test]
fn branching_throat_splits_into_two_opposite_patches() {
    // One wall facing two disconnected opposite walls must yield two groups, not
    // one chimeric pairing (battery check 2).
    let floor = plate([
        Vec3::new(0.1, 0.1, 0.5),
        Vec3::new(0.9, 0.1, 0.5),
        Vec3::new(0.9, 0.9, 0.5),
        Vec3::new(0.1, 0.9, 0.5),
    ]);
    let left = plate([
        Vec3::new(0.1, 0.1, 0.52),
        Vec3::new(0.35, 0.1, 0.52),
        Vec3::new(0.35, 0.9, 0.52),
        Vec3::new(0.1, 0.9, 0.52),
    ]);
    let right = plate([
        Vec3::new(0.65, 0.1, 0.52),
        Vec3::new(0.9, 0.1, 0.52),
        Vec3::new(0.9, 0.9, 0.52),
        Vec3::new(0.65, 0.9, 0.52),
    ]);
    let field = gap_field(&[floor, left, right], &[1, 1, 1]);
    let patches: std::collections::BTreeSet<i32> = field
        .groups
        .iter()
        .filter(|group| group.component == 1 && group.side > 0)
        .map(|group| group.opposite_patch)
        .collect();
    assert!(
        patches.len() >= 2,
        "the branching throat must split component 1 into at least two opposite patches, got {patches:?}"
    );
}

#[test]
fn oblique_edge_to_face_gap_is_found_only_by_the_closest_pair_sweep() {
    // A vertical plate standing above a horizontal plate: every normal ray of one
    // runs parallel to the other, so the rays-only design provably misses this
    // gap. The closest-pair sweep must still report it.
    let gap = 0.02;
    let floor = plate([
        Vec3::new(0.2, 0.2, 0.5),
        Vec3::new(0.8, 0.2, 0.5),
        Vec3::new(0.8, 0.8, 0.5),
        Vec3::new(0.2, 0.8, 0.5),
    ]);
    let wall = plate([
        Vec3::new(0.3, 0.5, 0.5 + gap),
        Vec3::new(0.7, 0.5, 0.5 + gap),
        Vec3::new(0.7, 0.5, 0.5 + gap + 0.2),
        Vec3::new(0.3, 0.5, 0.5 + gap + 0.2),
    ]);
    let field = gap_field(&[floor, wall], &[1, 1]);

    let ray_paired_across = field.samples.iter().any(|sample| {
        sample.kind != SampleKind::ClosestPair
            && matches!(sample.pair_class, PairClass::SheetSheet(1, 2))
    });
    assert!(
        !ray_paired_across,
        "the normal rays must not pair these two plates - the fixture encodes the rays-only miss"
    );
    assert!(
        field.stats.n_closest_pairs > 0,
        "the closest-pair sweep must find the edge-to-face approach"
    );
    let measured = min_surface_gap(&field);
    assert!(
        (measured - gap).abs() <= 0.05 * gap,
        "closest-pair separation {measured} must be within 5% of {gap}"
    );
}

#[test]
fn curved_gap_is_measured_within_chord_error() {
    let gap = 0.02;
    let inner = arc_strip(0.25, 24, 0.1);
    let outer = arc_strip(0.25 + gap, 24, 0.1);
    let field = gap_field(&[inner, outer], &[1, 1]);
    let measured = min_surface_gap(&field);
    assert!(
        measured.is_finite() && (measured - gap).abs() <= 0.1 * gap,
        "curved-gap separation {measured} must be within 10% of {gap} (coarse chord error)"
    );
}

#[test]
fn domain_walls_participate_as_virtual_walls() {
    let near_wall = plate([
        Vec3::new(0.3, 0.3, 0.01),
        Vec3::new(0.7, 0.3, 0.01),
        Vec3::new(0.7, 0.7, 0.01),
        Vec3::new(0.3, 0.7, 0.01),
    ]);
    let field = gap_field(&[near_wall], &[1]);
    let wall_groups: Vec<_> = field
        .groups
        .iter()
        .filter(|group| matches!(group.pair_class, PairClass::SurfaceBox(_)))
        .collect();
    assert!(
        !wall_groups.is_empty(),
        "a surface within t_layer of a domain face must produce a surface-box pairing"
    );
    let measured = wall_groups
        .iter()
        .map(|group| group.t_r)
        .fold(f64::INFINITY, f64::min);
    assert!(
        (measured - 0.01).abs() <= 0.05 * 0.01,
        "surface-box separation {measured} must be within 5% of 0.01"
    );
}

#[test]
fn box_walls_can_be_disabled() {
    let near_wall = plate([
        Vec3::new(0.3, 0.3, 0.01),
        Vec3::new(0.7, 0.3, 0.01),
        Vec3::new(0.7, 0.7, 0.01),
        Vec3::new(0.3, 0.7, 0.01),
    ]);
    let field = gap_field_with(
        &[near_wall],
        &[1],
        &GapFieldOptions {
            include_box_walls: false,
            ..Default::default()
        },
    );
    assert!(
        !field
            .groups
            .iter()
            .any(|group| matches!(group.pair_class, PairClass::SurfaceBox(_))),
        "no surface-box pairing may exist when the virtual walls are disabled"
    );
}

#[test]
fn thresholds_follow_the_bootstrap_sizing() {
    let field = gap_field(
        &[box_mesh(
            Vec3::new(0.3, 0.3, 0.3),
            Vec3::new(0.7, 0.7, 0.7),
        )],
        &[0],
    );
    assert_eq!(field.t_layer, T_LAYER);
    assert_eq!(field.t_sheet, 0.2 * H_BOOTSTRAP);
}

#[test]
fn gap_field_is_deterministic_across_runs() {
    let gap = 0.02;
    let build = || {
        vec![
            box_mesh(Vec3::new(0.3, 0.3, 0.2), Vec3::new(0.7, 0.7, 0.4)),
            box_mesh(
                Vec3::new(0.3, 0.3, 0.4 + gap),
                Vec3::new(0.7, 0.7, 0.6 + gap),
            ),
        ]
    };
    let first = gap_field(&build(), &[0, 0]);
    let second = gap_field(&build(), &[0, 0]);
    assert_eq!(first.samples.len(), second.samples.len());
    assert_eq!(first.groups.len(), second.groups.len());
    assert_eq!(first.stats, second.stats);
    for (a, b) in first.samples.iter().zip(&second.samples) {
        assert_eq!(a.t_raw.to_bits(), b.t_raw.to_bits());
        assert_eq!(a.t.to_bits(), b.t.to_bits());
        assert_eq!(a.flags, b.flags);
        assert_eq!(a.applicable, b.applicable);
    }
    for (a, b) in first.groups.iter().zip(&second.groups) {
        assert_eq!(a.t_r.to_bits(), b.t_r.to_bits());
        assert_eq!(a.confidence.to_bits(), b.confidence.to_bits());
        assert_eq!(a.opposite_patch, b.opposite_patch);
    }
}

#[test]
fn s03_snapshot_validates_verifies_and_carries_separation_t() {
    let gap = 0.02;
    let eps = 1.0e-4;
    let domain_min = Vec3::new(0.0, 0.0, 0.0);
    let domain_max = Vec3::new(1.0, 1.0, 1.0);
    let meshes = vec![
        box_mesh(Vec3::new(0.3, 0.3, 0.2), Vec3::new(0.7, 0.7, 0.4)),
        box_mesh(
            Vec3::new(0.3, 0.3, 0.4 + gap),
            Vec3::new(0.7, 0.7, 0.6 + gap),
        ),
    ];
    let components: Vec<ArrangeComponent> = (0..meshes.len())
        .map(|index| ArrangeComponent {
            x: index as i32 + 1,
            priority: index as u32,
            kind: 0,
            closed: true,
        })
        .collect();
    let conditioned = condition_surface(&meshes, eps, RepairLevel::Conservative).unwrap();
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

    let mut doc = gapfield_to_doc(&clipped, &field);
    doc.validate().expect("s03 must be a structurally valid VTU");
    let meta = SnapshotMeta::new(Stage::Gapfield, 0, (domain_min, domain_max));
    stamp_metadata(&mut doc, &meta);

    let separation = doc
        .point_array("separation_t")
        .expect("s03 must carry the separation_t point field");
    assert_eq!(separation.data.len(), doc.points.len());
    let finite: Vec<f64> = (0..separation.data.len())
        .map(|i| separation.data.get_f64(i))
        .filter(|value| *value >= 0.0)
        .collect();
    assert!(
        !finite.is_empty(),
        "the facing walls must have a measured separation"
    );
    let smallest = finite.iter().cloned().fold(f64::INFINITY, f64::min);
    assert!(
        (smallest - gap).abs() <= 0.05 * gap,
        "smallest separation_t {smallest} must be within 5% of {gap}"
    );

    let report = verify_with_options(
        &doc,
        &VerifyGates::default(),
        VerifyOptions {
            expected_stage: Some(Stage::Gapfield.index()),
            surfaces: Vec::new(),
        },
    );
    let failed: Vec<&str> = report
        .sections
        .iter()
        .filter(|section| section.status == CheckStatus::Fail)
        .map(|section| section.id.as_str())
        .collect();
    assert!(
        failed.is_empty(),
        "the s03 snapshot must not fail any verifier section, got {failed:?}"
    );
}

#[test]
fn thin_solid_slab_pairs_intra_component_through_its_own_material() {
    // A thin wall is a gap on the material side: the two faces of one component
    // face each other across 0.008, well inside t_layer.
    let thickness = 0.008;
    let slab = box_mesh(
        Vec3::new(0.3, 0.3, 0.5),
        Vec3::new(0.7, 0.7, 0.5 + thickness),
    );
    let field = gap_field(&[slab], &[0]);
    let intra: Vec<_> = field
        .groups
        .iter()
        .filter(|group| matches!(group.pair_class, PairClass::Intra(1)))
        .collect();
    assert!(
        !intra.is_empty(),
        "a thin slab must pair its own two faces as an intra-component gap"
    );
    let measured = intra
        .iter()
        .map(|group| group.t_r)
        .fold(f64::INFINITY, f64::min);
    assert!(
        (measured - thickness).abs() <= 0.05 * thickness,
        "intra separation {measured} must be within 5% of {thickness}"
    );
    for group in &intra {
        assert!(
            group.confidence >= 0.9,
            "a clean thin wall must not be gated out by the battery (got {})",
            group.confidence
        );
    }
}

#[test]
fn a_sharp_vertex_samples_every_wall_meeting_there() {
    // A box corner belongs to three walls. One averaged normal points into none of
    // them and sends the ray diagonally out through a side face; one sample per
    // smooth patch keeps each wall's ray on that wall's normal.
    let field = gap_field(
        &[box_mesh(
            Vec3::new(0.3, 0.3, 0.3),
            Vec3::new(0.7, 0.7, 0.7),
        )],
        &[0],
    );
    let corner = Vec3::new(0.3, 0.3, 0.3);
    let at_corner: Vec<_> = field
        .samples
        .iter()
        .filter(|sample| {
            sample.kind == SampleKind::Vertex
                && (sample.point.x - corner.x).abs() < 1e-9
                && (sample.point.y - corner.y).abs() < 1e-9
                && (sample.point.z - corner.z).abs() < 1e-9
        })
        .collect();
    assert_eq!(
        at_corner.len(),
        6,
        "three walls x two sides must each get their own corner sample"
    );
    for sample in &at_corner {
        let axis_aligned = [sample.direction.x, sample.direction.y, sample.direction.z]
            .iter()
            .filter(|component| (component.abs() - 1.0).abs() < 1e-9)
            .count();
        assert_eq!(
            axis_aligned, 1,
            "each corner sample must fire along one wall's own normal, not an average of three"
        );
    }
    let clusters: std::collections::BTreeSet<usize> =
        at_corner.iter().map(|sample| sample.vertex_cluster).collect();
    assert_eq!(clusters.len(), 3, "the corner must resolve into three smooth patches");
}

#[test]
fn samples_cover_both_sides_of_every_surface() {
    let field = gap_field(
        &[box_mesh(
            Vec3::new(0.3, 0.3, 0.3),
            Vec3::new(0.7, 0.7, 0.7),
        )],
        &[0],
    );
    let positive = field.samples.iter().filter(|s| s.side > 0).count();
    let negative = field.samples.iter().filter(|s| s.side < 0).count();
    assert_eq!(
        positive, negative,
        "every surface sample must exist on both sides so material-side and void-side gaps are both measurable"
    );
}
