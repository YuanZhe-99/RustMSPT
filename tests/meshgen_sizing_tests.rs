//! G4-1 acceptance tests: the sizing sources of PLAN §10.6, the graded (Lipschitz)
//! field, the background octree, the constraint `C(R)` the coupling driver calls,
//! and the `s04_sizing` snapshot.

use rustmspt::config::meshgen::{CoincidencePolicy, RepairLevel};
use rustmspt::io::vtu::{VtuDoc, VTK_VOXEL};
use rustmspt::meshgen::gapfield::{compute_gap_field, GapField, GapFieldOptions, Regime};
use rustmspt::meshgen::sizing::{
    build_sizing_field, collect_geometry_sources, couple_gap_and_sizing, curvature_sources,
    feature_sources, gap_sources, sizing_to_doc, CouplingOptions, SizingConstraint,
    SizingCriterion, SizingField, SizingLookup, SizingOptions, SizingSource,
    LFS_COVER_TOLERANCE,
};
use rustmspt::meshgen::snapshot::{SnapshotMeta, Stage};
use rustmspt::meshgen::{
    arrange_surface, build_scene, clip_arranged_to_box, condition_surface, detect_features,
    rebuild_topology, stamp_metadata, verify, ArrangeComponent, ArrangeOptions, ArrangedSurface,
    ColorMode, ConditionedSurface, SceneSpec, Severity, VerifyGates,
};
use rustmspt::types::{Mesh, Triangle, Vec3};

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

// AI-FUNC-SUMMARY: Closed axis-aligned box, each face subdivided n x n, outward winding; returns Mesh; side effects: none.
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

// AI-FUNC-SUMMARY:
// Purpose: A closed UV sphere - the fixture the curvature criterion is actually about.
// Inputs: centre, radius, latitude/longitude band counts.
// Returns: Mesh with outward-facing triangles.
// Side effects: None.
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

// AI-FUNC-SUMMARY: Run S0 + S1 on the given meshes; returns the conditioned surface and feature set; side effects: none.
fn condition(meshes: &[Mesh]) -> (ConditionedSurface, rustmspt::meshgen::FeatureSet) {
    let conditioned = condition_surface(meshes, EPS, RepairLevel::Conservative).unwrap();
    let features = detect_features(&conditioned, 45.0);
    (conditioned, features)
}

// AI-FUNC-SUMMARY: Run the whole S0..S3 chain; returns the clipped arranged surface and the gap field; side effects: none.
fn gap_field(meshes: &[Mesh], kinds: &[u8]) -> (ArrangedSurface, GapField) {
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
    let (conditioned, features) = condition(meshes);
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
    let field = compute_gap_field(
        &clipped,
        &topo,
        &GapFieldOptions {
            domain_min,
            domain_max,
            eps: EPS,
            h_bootstrap: 0.05,
            ..Default::default()
        },
    );
    (clipped, field)
}

// AI-FUNC-SUMMARY: The default G4-1 options over the unit domain; returns SizingOptions; side effects: none.
// Notes: `gap_cells` is pinned rather than inherited. These fixtures size their gaps against it —
//   0.02 and 0.03 against an `h_min` of 0.01 — so the production value (4, a representability
//   threshold measured in P-4.2) puts `lfs_floor = gap_cells * h_min` above the gaps themselves and
//   the constraint correctly emits nothing, which is a different scenario from the one each test
//   was written to check. Stating the value here keeps them testing the *mechanism* (`h = t /
//   gap_cells`, and the Lipschitz field between samples) at a scale where it is exercised, and
//   stops the production default from silently redefining what they assert.
fn options() -> SizingOptions {
    SizingOptions {
        domain_min: Vec3::new(0.0, 0.0, 0.0),
        domain_max: Vec3::new(1.0, 1.0, 1.0),
        eps: EPS,
        gap_cells: 2.0,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// The graded field
// ---------------------------------------------------------------------------

#[test]
fn an_empty_source_set_is_the_constant_ceiling() {
    let options = options();
    let field = SizingLookup::build(Vec::new(), &options);
    assert!(field.is_empty());
    for point in [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::new(1.0, 1.0, 1.0),
    ] {
        assert_eq!(field.eval(point), options.h_max);
    }
}

#[test]
fn the_field_grows_at_exactly_the_grading_rate_and_clamps_at_both_ends() {
    let options = options();
    let beta = options.beta();
    let source = SizingSource {
        point: Vec3::new(0.5, 0.5, 0.5),
        h: options.h_min,
        criterion: SizingCriterion::Curvature,
    };
    let field = SizingLookup::build(vec![source], &options);
    assert!((field.eval(source.point) - options.h_min).abs() < 1.0e-12);
    for distance in [0.001, 0.005, 0.01, 0.02, 0.05, 0.2, 0.4] {
        let probe = source.point.add(Vec3::new(distance, 0.0, 0.0));
        let expected = (options.h_min + beta * distance).clamp(options.h_min, options.h_max);
        assert!(
            (field.eval(probe) - expected).abs() < 1.0e-12,
            "at distance {distance}: {} != {expected}",
            field.eval(probe)
        );
    }
    // Far from every source the ceiling wins.
    assert_eq!(field.eval(Vec3::new(0.0, 0.0, 0.0)), options.h_max);
}

#[test]
fn the_field_is_lipschitz_everywhere_which_is_the_2_to_1_gradation() {
    let options = options();
    let beta = options.beta();
    // A deliberately awkward set: different sizes, clustered and isolated sources.
    let sources: Vec<SizingSource> = (0..40)
        .map(|index| {
            let t = index as f64 / 40.0;
            SizingSource {
                point: Vec3::new(t, (3.0 * t).fract(), (7.0 * t).fract()),
                h: options.h_min * (1.0 + 8.0 * (5.0 * t).fract()),
                criterion: SizingCriterion::Gap,
            }
        })
        .collect();
    let field = SizingLookup::build(sources, &options);
    let mut probes = Vec::new();
    for index in 0..30 {
        let t = index as f64 / 30.0;
        probes.push(Vec3::new((11.0 * t).fract(), t, (2.0 * t).fract()));
    }
    for left in &probes {
        for right in &probes {
            let distance = right.sub(*left).dot(right.sub(*left)).sqrt();
            let difference = (field.eval(*left) - field.eval(*right)).abs();
            assert!(
                difference <= beta * distance + 1.0e-12,
                "|{} - {}| = {difference} exceeds beta*{distance}",
                field.eval(*left),
                field.eval(*right)
            );
        }
    }
}

#[test]
fn the_box_minimum_matches_a_brute_force_scan() {
    let options = options();
    let sources: Vec<SizingSource> = (0..25)
        .map(|index| {
            let t = index as f64 / 25.0;
            SizingSource {
                point: Vec3::new(t, (5.0 * t).fract(), (13.0 * t).fract()),
                h: options.h_min * (1.0 + 6.0 * (3.0 * t).fract()),
                criterion: SizingCriterion::Curvature,
            }
        })
        .collect();
    let field = SizingLookup::build(sources.clone(), &options);
    let beta = options.beta();
    for corner in 0..8 {
        let lo = Vec3::new(
            0.1 * (corner % 3) as f64,
            0.15 * (corner % 4) as f64,
            0.2 * (corner % 2) as f64,
        );
        let hi = lo.add(Vec3::new(0.12, 0.09, 0.2));
        // The definition, evaluated the slow way.
        let mut expected = options.h_max;
        for source in &sources {
            let axis = |value: f64, lo: f64, hi: f64| (lo - value).max(value - hi).max(0.0);
            let d = Vec3::new(
                axis(source.point.x, lo.x, hi.x),
                axis(source.point.y, lo.y, hi.y),
                axis(source.point.z, lo.z, hi.z),
            );
            expected = expected.min(source.h + beta * d.dot(d).sqrt());
        }
        expected = expected.clamp(options.h_min, options.h_max);
        assert!(
            (field.eval_box(lo, hi) - expected).abs() < 1.0e-12,
            "{} != {expected}",
            field.eval_box(lo, hi)
        );
    }
}

// ---------------------------------------------------------------------------
// The octree
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: Assert every leaf is no larger than the field inside it; side effects: panics on violation.
fn assert_leaves_resolve_the_field(field: &SizingField, lookup: &SizingLookup) {
    for leaf in &field.leaves {
        let size = field.level_size(leaf.level);
        let min = field.leaf_min(leaf);
        let max = min.add(Vec3::new(size, size, size));
        let smallest = lookup.eval_box(min, max);
        assert!(
            size <= smallest + 1.0e-12 || leaf.level == field.max_level,
            "leaf at level {} has side {size} but the field asks for {smallest}",
            leaf.level
        );
    }
}

#[test]
fn a_uniform_field_produces_a_uniform_lattice_at_the_ceiling() {
    let options = options();
    let lookup = SizingLookup::build(Vec::new(), &options);
    let field = build_sizing_field(&lookup, &options);
    assert!(!field.leaves.is_empty());
    let level = field.leaves[0].level;
    assert!(field.leaves.iter().all(|leaf| leaf.level == level));
    let size = field.level_size(level);
    assert!(size <= options.h_max);
    // And not one level finer than it needs to be: a pessimistic split test would
    // halve this and octuple the leaf count for nothing.
    assert!(size * 2.0 > options.h_max);
    assert!(field.leaves.iter().all(|leaf| leaf.h == options.h_max));
    assert_leaves_resolve_the_field(&field, &lookup);
}

#[test]
fn the_octree_refines_around_a_source_and_grades_away_from_it() {
    let options = options();
    let source = SizingSource {
        point: Vec3::new(0.5, 0.5, 0.5),
        h: options.h_min,
        criterion: SizingCriterion::Gap,
    };
    let lookup = SizingLookup::build(vec![source], &options);
    let field = build_sizing_field(&lookup, &options);
    assert_leaves_resolve_the_field(&field, &lookup);

    let near = field.sample(Vec3::new(0.5, 0.5, 0.5)).unwrap();
    let mid = field.sample(Vec3::new(0.5, 0.5, 0.53)).unwrap();
    let far = field.sample(Vec3::new(0.05, 0.05, 0.05)).unwrap();
    assert!(near < mid, "{near} should be finer than {mid}");
    assert!(mid < far, "{mid} should be finer than {far}");
    assert!((near - options.h_min).abs() < 1.0e-9);
    assert!((far - options.h_max).abs() < 1.0e-9);

    // Levels only appear where the field asks for them.
    assert!(field.stats.max_level_used > 4);
    assert_eq!(field.stats.n_unresolved, 0);
}

#[test]
fn every_leaf_is_locatable_and_leaves_tile_the_domain() {
    let options = options();
    let lookup = SizingLookup::build(
        vec![SizingSource {
            point: Vec3::new(0.25, 0.75, 0.5),
            h: options.h_min * 3.0,
            criterion: SizingCriterion::Curvature,
        }],
        &options,
    );
    let field = build_sizing_field(&lookup, &options);
    let mut volume = 0.0;
    for leaf in &field.leaves {
        let size = field.level_size(leaf.level);
        volume += size * size * size;
        let located = field.locate(field.leaf_center(leaf)).unwrap();
        assert_eq!(field.leaves[located].coord, leaf.coord);
        assert_eq!(field.leaves[located].level, leaf.level);
    }
    // The unit domain is its own root cube here, so the leaves tile it exactly.
    assert!((volume - 1.0).abs() < 1.0e-9, "leaves cover {volume}");
    assert!(field.locate(Vec3::new(-0.5, 0.5, 0.5)).is_none());
}

#[test]
fn the_depth_cap_is_reported_rather_than_silently_exceeded() {
    let mut options = options();
    options.max_level = 3;
    let lookup = SizingLookup::build(
        vec![SizingSource {
            point: Vec3::new(0.5, 0.5, 0.5),
            h: options.h_min,
            criterion: SizingCriterion::Gap,
        }],
        &options,
    );
    let field = build_sizing_field(&lookup, &options);
    assert_eq!(field.max_level, 3);
    assert!(field.leaves.iter().all(|leaf| leaf.level <= 3));
    assert!(field.stats.level_capped);
    assert!(field.stats.n_unresolved > 0);
}

#[test]
fn the_leaf_budget_stops_refinement_a_whole_level_at_a_time() {
    let mut options = options();
    options.max_leaves = 5_000;
    let lookup = SizingLookup::build(
        vec![SizingSource {
            point: Vec3::new(0.5, 0.5, 0.5),
            h: options.h_min,
            criterion: SizingCriterion::Gap,
        }],
        &options,
    );
    let field = build_sizing_field(&lookup, &options);
    assert!(field.leaves.len() <= options.max_leaves);
    assert!(field.stats.leaf_capped);
}

#[test]
fn a_non_cubic_domain_is_covered_and_outside_cells_are_dropped() {
    let mut options = options();
    options.domain_max = Vec3::new(1.0, 0.25, 0.5);
    let lookup = SizingLookup::build(Vec::new(), &options);
    let field = build_sizing_field(&lookup, &options);
    assert!((field.root_size - 1.0).abs() < 1.0e-12);
    for leaf in &field.leaves {
        let min = field.leaf_min(leaf);
        assert!(min.y < options.domain_max.y && min.z < options.domain_max.z);
    }
    // Every domain point still lands in a leaf.
    for probe in [
        Vec3::new(0.01, 0.01, 0.01),
        Vec3::new(0.99, 0.24, 0.49),
        Vec3::new(0.5, 0.125, 0.25),
    ] {
        assert!(field.sample(probe).is_some(), "{probe:?} has no leaf");
    }
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

#[test]
fn a_flat_box_produces_no_curvature_sources() {
    let (conditioned, _) = condition(&[box_mesh(
        Vec3::new(0.3, 0.3, 0.3),
        Vec3::new(0.7, 0.7, 0.7),
        4,
    )]);
    let sources = curvature_sources(&conditioned, &options());
    assert!(
        sources.is_empty(),
        "a box has no curvature, only features: {} sources",
        sources.len()
    );
}

#[test]
fn a_sphere_produces_curvature_sources_near_the_analytic_size() {
    let options = options();
    let radius = 0.02;
    let (conditioned, _) = condition(&[sphere_mesh(Vec3::new(0.5, 0.5, 0.5), radius, 16)]);
    let sources = curvature_sources(&conditioned, &options);
    assert!(!sources.is_empty(), "a sphere must produce curvature sources");
    assert!(sources
        .iter()
        .all(|source| source.criterion == SizingCriterion::Curvature));

    // The chord-error rule on a sphere of radius R asks for 2*R*sqrt(f*(2-f)).
    let f = options.chord_error_frac;
    let expected = (2.0 * radius * (f * (2.0 - f)).sqrt()).clamp(options.h_min, options.h_max);
    // The median, not the extremes: a UV sphere's polar caps are genuinely finer
    // sampled than its equator, and the criterion is right to say so there.
    let mut values: Vec<f64> = sources.iter().map(|source| source.h).collect();
    values.sort_by(f64::total_cmp);
    let median = values[values.len() / 2];
    assert!(
        median > 0.7 * expected && median < 1.3 * expected,
        "median {median} should sit around {expected} (range [{}, {}])",
        values[0],
        values[values.len() - 1]
    );
    assert!(values.iter().all(|value| *value >= options.h_min));

    // The load-bearing property is the *smallest* request, because the field is a
    // minimum over sources: one spuriously small radius anywhere pulls the whole
    // neighbourhood down with it, and nothing later can undo that.
    //
    // The edge-length form failed exactly here. A UV sphere's polar latitude edges
    // are short but carry a full-size dihedral, so it reported a radius tending to
    // zero and asked for `h_min` at the poles - on a surface of constant curvature.
    // Measured on this fixture it bottomed out at 0.0117, **half** the analytic size.
    // Reading the width *across* the edge instead returns the sphere's own radius in
    // both directions.
    assert!(
        (values[0] - expected).abs() < 0.1 * expected,
        "the smallest request is {} but the sphere's curvature only justifies {expected}",
        values[0]
    );
    // Some edges over-estimate (the diagonal of a warped quad measures the quad's
    // warp, not the surface), which is the harmless direction under a `min`.
    assert!(values[values.len() - 1] <= 2.5 * expected);
}

#[test]
fn a_coarser_sphere_of_the_same_radius_asks_for_the_same_size() {
    // The discrete radius must read the *surface*, not the tessellation: doubling the
    // band count halves the chord and halves the dihedral, and the ratio is what the
    // criterion depends on.
    let options = options();
    let mut sizes = Vec::new();
    for bands in [12usize, 24] {
        let (conditioned, _) = condition(&[sphere_mesh(Vec3::new(0.5, 0.5, 0.5), 0.02, bands)]);
        let sources = curvature_sources(&conditioned, &options);
        assert!(!sources.is_empty(), "{bands} bands produced no curvature source");
        let mean = sources.iter().map(|source| source.h).sum::<f64>() / sources.len() as f64;
        sizes.push(mean);
    }
    let ratio = sizes[0] / sizes[1];
    assert!(
        ratio > 0.8 && ratio < 1.25,
        "refining the input changed the requested size by {ratio}x"
    );
}

#[test]
fn a_sharp_edge_is_not_curvature_and_a_corner_is_a_feature_source() {
    let options = options();
    // Small enough that the corner rule's shortest incident segment lands below the
    // ceiling - a box whose edges are already coarser than h_max asks for nothing.
    let mesh = box_mesh(Vec3::new(0.45, 0.45, 0.45), Vec3::new(0.55, 0.55, 0.55), 4);
    let (conditioned, features) = condition(&[mesh]);
    assert!(
        curvature_sources(&conditioned, &options).is_empty(),
        "a 90-degree box edge is a feature, not curvature - counting it would drive \
         every corner in the model to h_min"
    );
    let sources = feature_sources(&conditioned, &features, &options);
    // Every source is a corner: the box's feature curves are straight.
    assert!(sources
        .iter()
        .all(|source| source.criterion == SizingCriterion::Corner));
    assert!(
        sources.len() >= 8,
        "expected the box's 8 corners, got {}",
        sources.len()
    );
    for source in &sources {
        assert!(source.h >= options.h_min && source.h < options.h_max);
    }
}

#[test]
fn local_feature_size_asks_for_gap_cells_across_a_volumetric_gap() {
    let gap = 0.03;
    let (surface, field) = gap_field(
        &[
            box_mesh(Vec3::new(0.3, 0.3, 0.2), Vec3::new(0.6, 0.6, 0.4), 3),
            box_mesh(
                Vec3::new(0.3, 0.3, 0.4 + gap),
                Vec3::new(0.6, 0.6, 0.6 + gap),
                3,
            ),
        ],
        &[0, 0],
    );
    let options = options();
    // Every region volumetric: the LFS term must resolve the gap itself.
    let regimes = vec![Regime::Normal; field.regions.len()];
    let sources = gap_sources(&surface, &field, &regimes, &options);
    assert!(!sources.is_empty(), "a measured 0.03 gap must ask for elements");
    let smallest = sources
        .iter()
        .map(|source| source.h)
        .fold(f64::INFINITY, f64::min);
    let expected = gap / options.gap_cells;
    assert!(
        (smallest - expected).abs() < 0.2 * expected,
        "{smallest} should be about {expected} = gap / gap_cells"
    );
}

#[test]
fn the_local_feature_size_constraint_holds_between_samples_not_only_at_them() {
    // Found by the G4-4 visual review: point sources leave the *middle* of a
    // coarsely tessellated wall unresolved, because the beta-Lipschitz field rises
    // by beta*d/2 between two of them. On a 0.02 gap that produced a field running
    // up to 0.031 - coarser than the gap is wide. Each wall face is now covered.
    let gap = 0.02;
    let (surface, field) = gap_field(
        &[
            // Deliberately coarse walls: two triangles per face, far apart
            // relative to the gap they bound.
            box_mesh(Vec3::new(0.2, 0.2, 0.2), Vec3::new(0.8, 0.8, 0.4), 1),
            box_mesh(
                Vec3::new(0.2, 0.2, 0.4 + gap),
                Vec3::new(0.8, 0.8, 0.6 + gap),
                1,
            ),
        ],
        &[0, 0],
    );
    let options = options();
    let sources = gap_sources(
        &surface,
        &field,
        &vec![Regime::Normal; field.regions.len()],
        &options,
    );
    assert!(!sources.is_empty());
    let lookup = SizingLookup::build(sources, &options);
    let target = gap / options.gap_cells;

    // Probe the mid-plane of the gap on a grid, including the points furthest
    // from any sample.
    let mut worst = 0.0f64;
    for i in 0..=20 {
        for j in 0..=20 {
            let point = Vec3::new(
                0.2 + 0.6 * i as f64 / 20.0,
                0.2 + 0.6 * j as f64 / 20.0,
                0.4 + gap * 0.5,
            );
            worst = worst.max(lookup.eval(point));
        }
    }
    // The bound the cover spacing is chosen for (`LFS_COVER_TOLERANCE`): a
    // beta-Lipschitz field cannot be exactly constant over a discretely covered
    // region, but it can be held within a stated factor of the request.
    assert!(
        worst <= (1.0 + LFS_COVER_TOLERANCE) * target,
        "the field reaches {worst} inside a {gap} gap that asked for {target}"
    );
    // And far below where it was before the cover existed: wall-only point sources
    // put 0.031 - wider than the gap - in the middle of this fixture.
    assert!(worst < gap, "an element wider than the gap itself");
}

#[test]
fn a_converted_region_contributes_no_local_feature_size_sources() {
    let gap = 0.03;
    let (surface, field) = gap_field(
        &[
            box_mesh(Vec3::new(0.3, 0.3, 0.2), Vec3::new(0.6, 0.6, 0.4), 3),
            box_mesh(
                Vec3::new(0.3, 0.3, 0.4 + gap),
                Vec3::new(0.6, 0.6, 0.6 + gap),
                3,
            ),
        ],
        &[0, 0],
    );
    let options = options();
    let volumetric = gap_sources(&surface, &field, &vec![Regime::Normal; field.regions.len()], &options);
    let converted = gap_sources(&surface, &field, &vec![Regime::Band; field.regions.len()], &options);
    assert!(
        converted.len() < volumetric.len(),
        "converting every region must remove sources, not keep {} of {}",
        converted.len(),
        volumetric.len()
    );
    // What remains must not be asking to resolve a gap a band template will mesh.
    let smallest_converted = converted
        .iter()
        .map(|source| source.h)
        .fold(f64::INFINITY, f64::min);
    let smallest_volumetric = volumetric
        .iter()
        .map(|source| source.h)
        .fold(f64::INFINITY, f64::min);
    assert!(smallest_converted > smallest_volumetric);
}

#[test]
fn a_contact_is_not_a_gap_and_asks_for_nothing() {
    // Two boxes sharing a face: every sample there measures zero separation. Without
    // the floor these drive the whole field - and the coupling thresholds - to h_min.
    let (surface, field) = gap_field(
        &[
            box_mesh(Vec3::new(0.3, 0.3, 0.2), Vec3::new(0.6, 0.6, 0.4), 3),
            box_mesh(Vec3::new(0.3, 0.3, 0.4), Vec3::new(0.6, 0.6, 0.6), 3),
        ],
        &[0, 0],
    );
    let options = options();
    let sources = gap_sources(&surface, &field, &vec![Regime::Normal; field.regions.len()], &options);
    let floor = options.lfs_floor();
    for source in &sources {
        assert!(
            source.h > floor / options.gap_cells - 1.0e-12,
            "a contact leaked a source at h={}",
            source.h
        );
    }
    assert!(sources.iter().all(|source| source.h > options.h_min));
}

// ---------------------------------------------------------------------------
// The constraint C(R) and the coupling loop
// ---------------------------------------------------------------------------

#[test]
fn the_constraint_is_the_ceiling_when_there_are_no_thin_regions() {
    let (_surface, field) = gap_field(
        &[box_mesh(
            Vec3::new(0.3, 0.3, 0.3),
            Vec3::new(0.5, 0.5, 0.5),
            3,
        )],
        &[0],
    );
    let options = options();
    let lookup = SizingLookup::build(Vec::new(), &options);
    let constraint = SizingConstraint::new(&lookup, &field, &options);
    let regimes = vec![Regime::Normal; field.regions.len()];
    assert_eq!(constraint.evaluate(&regimes), options.h_max);
    assert!(constraint.binding_region(&regimes).is_none());
}

#[test]
fn a_volumetric_region_lowers_h_and_a_converted_one_does_not() {
    let options = options();
    let constraint = SizingConstraint {
        geometry: vec![options.h_max, options.h_max],
        separation: vec![0.02, 0.03],
        h_max: options.h_max,
        gap_cells: options.gap_cells,
        lfs_floor: options.lfs_floor(),
    };
    assert_eq!(
        constraint.evaluate(&[Regime::Normal, Regime::Normal]),
        0.01,
        "the tightest volumetric gap sets C(R)"
    );
    assert_eq!(
        constraint.evaluate(&[Regime::Sheet, Regime::Normal]),
        0.015,
        "a converted region stops constraining"
    );
    assert_eq!(
        constraint.evaluate(&[Regime::Sheet, Regime::Band]),
        options.h_max,
    );
    let (region, value, from_gap) = constraint
        .binding_region(&[Regime::Normal, Regime::Normal])
        .unwrap();
    assert_eq!((region, value, from_gap), (0, 0.01, true));
}

#[test]
fn the_coupling_loop_converges_against_the_real_constraint() {
    let options = options();
    // A gap wide enough to stay volumetric under the bootstrap thresholds; its LFS
    // demand then pulls h down, which is the whole point of the S3<->S4 coupling.
    let constraint = SizingConstraint {
        geometry: vec![options.h_max],
        separation: vec![0.06],
        h_max: options.h_max,
        gap_cells: options.gap_cells,
        lfs_floor: options.lfs_floor(),
    };
    let coupling = couple_gap_and_sizing(
        &[0.06],
        &CouplingOptions {
            h_max: options.h_max,
            h_min: options.h_min,
            eps: EPS,
            ..Default::default()
        },
        |regimes, _| constraint.evaluate(regimes),
    )
    .unwrap();
    assert!(coupling.converged);
    assert!(coupling.iterations <= 3, "SPEC §11.4 expects <= 3");
    assert_eq!(coupling.regimes[0], Regime::Normal);
    assert!((coupling.h - 0.03).abs() < 1.0e-12, "h = {}", coupling.h);
    // And monotone: h never rose above where it started.
    assert!(coupling.h <= options.h_max);
}

#[test]
fn a_declined_region_never_sets_the_global_thresholds() {
    // One speck S3 declined, with a separation small enough to pin h at the floor.
    let gap = 0.03;
    let (_surface, mut field) = gap_field(
        &[
            box_mesh(Vec3::new(0.3, 0.3, 0.2), Vec3::new(0.6, 0.6, 0.4), 3),
            box_mesh(
                Vec3::new(0.3, 0.3, 0.4 + gap),
                Vec3::new(0.6, 0.6, 0.6 + gap),
                3,
            ),
        ],
        &[0, 0],
    );
    assert!(!field.regions.is_empty());
    let options = options();
    let lookup = SizingLookup::build(Vec::new(), &options);

    let live = SizingConstraint::new(&lookup, &field, &options);
    let live_value = live.evaluate(&vec![Regime::Normal; field.regions.len()]);

    for region in &mut field.regions {
        region.skip = Some(rustmspt::meshgen::SkipReason::Speck);
    }
    let declined = SizingConstraint::new(&lookup, &field, &options);
    let declined_value = declined.evaluate(&vec![Regime::Normal; field.regions.len()]);

    assert!(live_value < options.h_max);
    assert_eq!(
        declined_value, options.h_max,
        "a measurement S3 refused to act on must not set the model-wide thresholds"
    );
}

// ---------------------------------------------------------------------------
// s04_sizing
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: Build the s04 document for a one-source field; returns (doc, field); side effects: none.
fn s04_document() -> (VtuDoc, SizingField) {
    let options = options();
    let lookup = SizingLookup::build(
        vec![SizingSource {
            point: Vec3::new(0.5, 0.5, 0.5),
            h: options.h_min * 4.0,
            criterion: SizingCriterion::Gap,
        }],
        &options,
    );
    let field = build_sizing_field(&lookup, &options);
    let components = vec![ArrangeComponent {
        x: 1,
        priority: 0,
        kind: 0,
        closed: true,
    }];
    (sizing_to_doc(&field, &components), field)
}

#[test]
fn the_s04_snapshot_validates_and_carries_the_contract_arrays() {
    let (mut doc, field) = s04_document();
    doc.validate().unwrap();
    assert_eq!(doc.num_cells(), field.leaves.len());
    assert!(doc.types.iter().all(|kind| *kind == VTK_VOXEL));
    let kinds = doc.cell_array("cell_kind").unwrap();
    assert!((0..kinds.data.len()).all(|i| kinds.data.get_i64(i) == 3));
    for name in [
        "region_key",
        "partition_id",
        "regime",
        "face_tag_key",
        "curve_id",
    ] {
        assert!(doc.cell_array(name).is_some(), "{name} missing");
    }
    for name in ["n_id_key", "constraint_kind", "constraint_ref", "sizing_h"] {
        assert!(doc.point_array(name).is_some(), "{name} missing");
    }
    let sizing = doc.point_array("sizing_h").unwrap();
    assert_eq!(sizing.data.len(), doc.points.len());
    // Every emitted point belongs to a leaf, so nothing carries the sentinel.
    assert!((0..sizing.data.len()).all(|i| sizing.data.get_f64(i) > 0.0));

    let meta = SnapshotMeta::new(
        Stage::Sizing,
        0x5121,
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
    );
    stamp_metadata(&mut doc, &meta);
    let report = verify(&doc, &VerifyGates::default());
    let failures: Vec<_> = report
        .sections
        .iter()
        .flat_map(|section| section.items.iter())
        .filter(|item| item.severity == Severity::Fail)
        .collect();
    assert!(failures.is_empty(), "s04 must verify clean: {failures:?}");
}

#[test]
fn the_s04_corners_are_deduplicated_on_the_octree_lattice() {
    let (doc, field) = s04_document();
    // A level jump shares exact coordinates, so no two points may coincide and the
    // point count must be far below 8 per leaf.
    assert!(doc.points.len() < field.leaves.len() * 8);
    let mut keys: Vec<(u64, u64, u64)> = doc
        .points
        .iter()
        .map(|p| (p.x.to_bits(), p.y.to_bits(), p.z.to_bits()))
        .collect();
    keys.sort_unstable();
    let before = keys.len();
    keys.dedup();
    assert_eq!(before, keys.len(), "duplicate corner coordinates emitted");
}

#[test]
fn the_s04_snapshot_renders() {
    let (doc, _) = s04_document();
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
    assert!(
        !scene.tris.is_empty(),
        "the voxel preview must produce boundary triangles"
    );
    // Only the outer shell of the octree is a boundary quad of the selected subset.
    assert!(scene.tris.len() < doc.num_cells() * 12);
}

// ---------------------------------------------------------------------------
// Determinism (R-P2)
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: Build the geometry sources and the octree for a fixed scene; returns both; side effects: none.
fn deterministic_run() -> (Vec<SizingSource>, SizingField) {
    let options = options();
    let meshes = [
        sphere_mesh(Vec3::new(0.35, 0.5, 0.5), 0.06, 14),
        box_mesh(Vec3::new(0.6, 0.35, 0.35), Vec3::new(0.85, 0.65, 0.65), 3),
    ];
    let (conditioned, features) = condition(&meshes);
    let sources = collect_geometry_sources(&conditioned, &features, &options);
    let lookup = SizingLookup::build(sources.clone(), &options);
    (sources, build_sizing_field(&lookup, &options))
}

#[test]
fn the_field_is_bit_identical_across_runs_and_thread_counts() {
    let (sources, field) = deterministic_run();
    let (again, field_again) = deterministic_run();
    assert_eq!(sources.len(), again.len());
    for (left, right) in sources.iter().zip(again.iter()) {
        assert_eq!(left.h.to_bits(), right.h.to_bits());
        assert_eq!(left.point.x.to_bits(), right.point.x.to_bits());
        assert_eq!(left.criterion, right.criterion);
    }
    assert_eq!(field.leaves, field_again.leaves);

    // PLAN §12.5 acceptance: the same input on one thread must give the same bits.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let (serial_sources, serial_field) = pool.install(deterministic_run);
    assert_eq!(sources.len(), serial_sources.len());
    for (left, right) in sources.iter().zip(serial_sources.iter()) {
        assert_eq!(left.h.to_bits(), right.h.to_bits());
        assert_eq!(left.point.y.to_bits(), right.point.y.to_bits());
    }
    assert_eq!(field.leaves, serial_field.leaves);
    for (left, right) in field.leaves.iter().zip(serial_field.leaves.iter()) {
        assert_eq!(left.h.to_bits(), right.h.to_bits());
    }
}
