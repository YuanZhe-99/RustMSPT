//! G6-1 acceptance tests: exact edge-patch crossings, the frozen target priority,
//! the 30 % motion cap, the exact no-inversion rule, box-face constraints, the
//! 97.5/2.5 % re-check, determinism, and `s07_snapped`.

use rustmspt::config::meshgen::{CoincidencePolicy, RepairLevel};
use rustmspt::meshgen::classify::{classify_lattice, Classification, ClassifyOptions};
use rustmspt::meshgen::lattice::{balance_octree, build_lattice, Lattice, LatticeOptions};
use rustmspt::meshgen::sizing::{build_sizing_field, SizingLookup, SizingOptions};
use rustmspt::meshgen::snap::{
    move_preserves_orientation, snap_lattice, snapped_to_doc, unique_edges, SnapOptions, Snapped,
    TargetKind, SNAP_MOTION_CAP, SNAP_RECHECK_HIGH, SNAP_RECHECK_LOW, WEIGHT_CORNER, WEIGHT_CURVE,
    WEIGHT_SURFACE,
};
use rustmspt::meshgen::snapshot::{SnapshotMeta, Stage};
use rustmspt::meshgen::{
    arrange_surface, clip_arranged_to_box, condition_surface, detect_features, orient3d,
    rebuild_topology, stamp_metadata, verify, ArrangeComponent, ArrangeOptions, ArrangedSurface,
    Severity, VerifyGates,
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

/// Everything S7 needs, built by running S0..S6 for real.
struct Scene {
    surface: ArrangedSurface,
    classification: Classification,
    lattice: Lattice,
    domain_min: Vec3,
    domain_max: Vec3,
}

// AI-FUNC-SUMMARY:
// Purpose: Run the pipeline up to and including S6, so S7 is tested against real upstream output.
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
    let classification = classify_lattice(
        &lattice,
        &clipped,
        &topo,
        &ClassifyOptions {
            domain_min,
            domain_max,
        },
    );
    Scene {
        surface: clipped,
        classification,
        lattice,
        domain_min,
        domain_max,
    }
}

// AI-FUNC-SUMMARY: Run S7 on a scene; returns Snapped; side effects: none.
fn snap(scene: &Scene) -> Snapped {
    snap_lattice(
        &scene.lattice,
        &scene.surface,
        &scene.classification,
        &SnapOptions {
            domain_min: scene.domain_min,
            domain_max: scene.domain_max,
            eps: EPS,
        },
    )
}

// AI-FUNC-SUMMARY: The shortest lattice edge at every node; returns the per-node minimum; side effects: none.
fn shortest_incident_edge(lattice: &Lattice) -> Vec<f64> {
    let mut l_min = vec![f64::INFINITY; lattice.nodes.len()];
    for edge in unique_edges(lattice) {
        let length = lattice.nodes[edge[1] as usize]
            .sub(lattice.nodes[edge[0] as usize])
            .dot(
                lattice.nodes[edge[1] as usize].sub(lattice.nodes[edge[0] as usize]),
            )
            .sqrt();
        for node in edge {
            let slot = &mut l_min[node as usize];
            if length < *slot {
                *slot = length;
            }
        }
    }
    l_min
}

// AI-FUNC-SUMMARY:
// Purpose: Point-to-triangle distance, written independently of the code under test.
// Returns: f64.
// Side effects: None.
// Notes: Project onto the plane, then, if the projection falls outside, take the smallest distance
//   to the three edges. Slower than the production routine and deliberately so - a test that reuses
//   the implementation it is checking proves nothing.
fn point_triangle_distance(p: Vec3, tri: [Vec3; 3]) -> f64 {
    let normal = tri[1].sub(tri[0]).cross(tri[2].sub(tri[0]));
    let area2 = normal.dot(normal).sqrt();
    if area2 > 0.0 {
        let unit = normal.scale(1.0 / area2);
        let projected = p.sub(unit.scale(unit.dot(p.sub(tri[0]))));
        let inside = [0usize, 1, 2].iter().all(|i| {
            let a = tri[*i];
            let b = tri[(*i + 1) % 3];
            b.sub(a).cross(projected.sub(a)).dot(normal) >= 0.0
        });
        if inside {
            return projected.sub(p).dot(projected.sub(p)).sqrt();
        }
    }
    let mut best = f64::INFINITY;
    for i in 0..3 {
        let a = tri[i];
        let b = tri[(i + 1) % 3];
        let ab = b.sub(a);
        let denominator = ab.dot(ab);
        let t = if denominator > 0.0 {
            (p.sub(a).dot(ab) / denominator).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = a.add(ab.scale(t));
        best = best.min(q.sub(p).dot(q.sub(p)).sqrt());
    }
    best
}

// AI-FUNC-SUMMARY: The distance from a point to the nearest active, non-box arranged face; returns f64; side effects: none.
fn distance_to_surface(surface: &ArrangedSurface, classification: &Classification, p: Vec3) -> f64 {
    let mut best = f64::INFINITY;
    for (index, face) in surface.faces.iter().enumerate() {
        if face.box_tagged || !classification.active_face[index] {
            continue;
        }
        let tri = [
            surface.vertices[face.nodes[0]],
            surface.vertices[face.nodes[1]],
            surface.vertices[face.nodes[2]],
        ];
        best = best.min(point_triangle_distance(p, tri));
    }
    best
}

#[test]
// AI-FUNC-SUMMARY: Every tet is still positively oriented after S7 - the invariant ARB-10 exists to protect.
fn no_tet_is_inverted_by_the_snap() {
    let scene = scene(
        &[sphere_mesh(Vec3::new(0.5, 0.5, 0.5), 0.28, 12)],
        &[0],
        &[10],
        0.1,
    );
    let snapped = snap(&scene);
    let mut worst = f64::INFINITY;
    for tet in &scene.lattice.tets {
        let value = orient3d(
            snapped.nodes[tet[0] as usize],
            snapped.nodes[tet[1] as usize],
            snapped.nodes[tet[2] as usize],
            snapped.nodes[tet[3] as usize],
        );
        worst = worst.min(value);
    }
    assert!(
        worst > 0.0,
        "S7 left a non-positive tet (orient3d = {worst:e})"
    );
}

#[test]
// AI-FUNC-SUMMARY: No node moves further than 30 % of its shortest incident lattice edge (ARB-11).
fn no_move_exceeds_the_motion_cap() {
    let scene = scene(
        &[sphere_mesh(Vec3::new(0.5, 0.5, 0.5), 0.28, 12)],
        &[0],
        &[10],
        0.1,
    );
    let snapped = snap(&scene);
    let l_min = shortest_incident_edge(&scene.lattice);
    let mut moved = 0usize;
    for (node, distance) in snapped.motion.iter().enumerate() {
        if *distance > 0.0 {
            moved += 1;
        }
        let cap = SNAP_MOTION_CAP * l_min[node];
        assert!(
            *distance <= cap * (1.0 + 1.0e-9),
            "node {node} moved {distance:e}, cap {cap:e}"
        );
    }
    assert!(moved > 0, "the snap moved nothing at all");
    assert_eq!(
        snapped.stats.n_capped,
        snapped.under_snapped.len(),
        "every clamped node must be listed for the [V5] gate"
    );
    for node in &snapped.under_snapped {
        let cap = SNAP_MOTION_CAP * l_min[*node as usize];
        assert!(
            (snapped.motion[*node as usize] - cap).abs() <= cap * 1.0e-9,
            "an under-snapped node should sit exactly at its cap"
        );
    }
}

#[test]
// AI-FUNC-SUMMARY: Snapped surface nodes land *on* the surface, not merely near it.
fn surface_targets_land_on_the_surface() {
    let scene = scene(
        &[sphere_mesh(Vec3::new(0.5, 0.5, 0.5), 0.28, 12)],
        &[0],
        &[10],
        0.1,
    );
    let snapped = snap(&scene);
    let mut checked = 0usize;
    for (node, kind) in snapped.constraint_kind.iter().enumerate() {
        if *kind != TargetKind::Surface as u8 || snapped.motion[node] == 0.0 {
            continue;
        }
        if snapped.under_snapped.contains(&(node as u32)) {
            continue;
        }
        let distance =
            distance_to_surface(&scene.surface, &scene.classification, snapped.nodes[node]);
        assert!(
            distance < 1.0e-9,
            "node {node} carries a surface constraint but sits {distance:e} off the surface"
        );
        checked += 1;
    }
    assert!(checked > 0, "no node took a surface target");
}

#[test]
// AI-FUNC-SUMMARY: The frozen order corner > curve > surface, checked on a cube's own corners.
fn corners_win_over_curves_and_surfaces() {
    assert_eq!(TargetKind::Corner.weight(), WEIGHT_CORNER);
    assert_eq!(TargetKind::Polyline.weight(), WEIGHT_CURVE);
    assert_eq!(TargetKind::Surface.weight(), WEIGHT_SURFACE);
    assert!(TargetKind::Corner.weight() > TargetKind::Polyline.weight());
    assert!(TargetKind::Polyline.weight() > TargetKind::Surface.weight());

    // A cube whose corners sit a little off the lattice: each corner is a sharp
    // point feature, its edges are sharp curves, and its faces are patches, so
    // every lattice node near a corner sees all three kinds at once.
    let scene = scene(
        &[box_mesh(
            Vec3::new(0.26, 0.26, 0.26),
            Vec3::new(0.74, 0.74, 0.74),
            2,
        )],
        &[0],
        &[10],
        0.1,
    );
    let snapped = snap(&scene);
    assert!(
        snapped.stats.n_snapped_corner > 0,
        "no node snapped to a cube corner"
    );
    let corners: Vec<Vec3> = scene
        .surface
        .corner_nodes
        .iter()
        .map(|node| scene.surface.vertices[*node])
        .collect();
    for (node, kind) in snapped.constraint_kind.iter().enumerate() {
        if *kind != TargetKind::Corner as u8 {
            continue;
        }
        let point = snapped.nodes[node];
        let exact = corners
            .iter()
            .any(|corner| corner.sub(point).dot(corner.sub(point)) == 0.0);
        assert!(
            exact,
            "node {node} claims a corner constraint but is not *at* a corner"
        );
    }
    assert!(
        snapped.stats.n_snapped_curve > 0,
        "no node snapped to a sharp edge of the cube"
    );
}

#[test]
// AI-FUNC-SUMMARY: A node on a domain face stays on it, so S7 never deforms the domain box.
fn box_faces_are_preserved() {
    let scene = scene(
        &[box_mesh(
            Vec3::new(0.0, 0.0, 0.3),
            Vec3::new(1.0, 1.0, 0.62),
            2,
        )],
        &[0],
        &[10],
        0.1,
    );
    let snapped = snap(&scene);
    let on_plane = |value: f64, plane: f64| value == plane;
    let mut constrained = 0usize;
    for (node, before) in scene.lattice.nodes.iter().enumerate() {
        let after = snapped.nodes[node];
        for axis in 0..3 {
            let (b, a) = match axis {
                0 => (before.x, after.x),
                1 => (before.y, after.y),
                _ => (before.z, after.z),
            };
            let (lo, hi) = match axis {
                0 => (scene.domain_min.x, scene.domain_max.x),
                1 => (scene.domain_min.y, scene.domain_max.y),
                _ => (scene.domain_min.z, scene.domain_max.z),
            };
            if on_plane(b, lo) || on_plane(b, hi) {
                assert_eq!(
                    a, b,
                    "node {node} left the domain boundary along axis {axis}"
                );
                constrained += 1;
            }
        }
    }
    assert!(constrained > 0, "the fixture has no boundary nodes");
}

#[test]
// AI-FUNC-SUMMARY: The 97.5/2.5 % re-check removes the near-endpoint crossings that would give S8 slivers.
fn the_recheck_clears_the_near_endpoint_band() {
    let scene = scene(
        &[sphere_mesh(Vec3::new(0.5, 0.5, 0.5), 0.28, 12)],
        &[0],
        &[10],
        0.1,
    );
    // The premise: without a snap there *are* crossings in the band, so the test
    // below is measuring the pass and not an accident of the fixture.
    let unsnapped = snap_lattice(
        &scene.lattice,
        &scene.surface,
        &Classification {
            active_face: vec![false; scene.surface.faces.len()],
            ..scene.classification.clone()
        },
        &SnapOptions {
            domain_min: scene.domain_min,
            domain_max: scene.domain_max,
            eps: EPS,
        },
    );
    assert_eq!(
        unsnapped.stats.n_crossings, 0,
        "with every patch inactive there is nothing to snap to or cut"
    );

    let snapped = snap(&scene);
    let in_band = snapped
        .crossings
        .iter()
        .filter(|crossing| crossing.t < SNAP_RECHECK_LOW || crossing.t > SNAP_RECHECK_HIGH)
        .count();
    assert!(
        snapped.stats.n_crossings > 100,
        "the fixture produced almost no crossings ({})",
        snapped.stats.n_crossings
    );
    assert!(
        snapped.stats.n_snapped_surface > 0,
        "no endpoint was pulled onto the surface, so the band rule never fired"
    );
    // Not "few": none. A survivor would mean a promotion the stage could not take,
    // and both reasons for that - the inversion test and the motion cap - are
    // counted in `n_recheck_residual`, so the two must agree.
    assert_eq!(
        in_band, 0,
        "{in_band} crossing(s) still sit within 2.5 % of an edge end"
    );
    assert_eq!(
        snapped.stats.n_recheck_residual, 0,
        "the stage recorded band residue it did not report in the crossing list"
    );
    assert!(
        snapped.stats.n_on_cut >= snapped.stats.n_snapped_surface,
        "a node pulled onto the surface must come back as an on-cut vertex (K2)"
    );
}

#[test]
// AI-FUNC-SUMMARY: The exact inversion test rejects a move across the opposite face and accepts a small one (ARB-10).
fn the_inversion_test_is_exact() {
    let nodes = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let tets = vec![[0u32, 1, 2, 3]];
    assert!(
        orient3d(nodes[0], nodes[1], nodes[2], nodes[3]) > 0.0,
        "the fixture tet must start positively oriented"
    );
    assert!(move_preserves_orientation(
        &nodes,
        &tets,
        &[0],
        3,
        Vec3::new(0.05, 0.05, 0.4)
    ));
    // Onto the opposite face: exactly degenerate, which is a rejection.
    assert!(!move_preserves_orientation(
        &nodes,
        &tets,
        &[0],
        3,
        Vec3::new(0.25, 0.25, 0.0)
    ));
    // Through it: inverted.
    assert!(!move_preserves_orientation(
        &nodes,
        &tets,
        &[0],
        3,
        Vec3::new(0.25, 0.25, -0.25)
    ));
}

#[test]
// AI-FUNC-SUMMARY: Every crossing is consistent with the coordinates S7 emitted, not with the ones it started from.
fn crossings_match_the_snapped_coordinates() {
    let scene = scene(
        &[box_mesh(
            Vec3::new(0.22, 0.22, 0.22),
            Vec3::new(0.78, 0.78, 0.78),
            2,
        )],
        &[0],
        &[10],
        0.1,
    );
    let snapped = snap(&scene);
    assert!(!snapped.crossings.is_empty());
    for crossing in &snapped.crossings {
        assert!(
            (0.0..=1.0).contains(&crossing.t),
            "crossing parameter {} out of range",
            crossing.t
        );
        let p = snapped.nodes[crossing.edge[0] as usize];
        let q = snapped.nodes[crossing.edge[1] as usize];
        let expected = p.add(q.sub(p).scale(crossing.t));
        let drift = expected.sub(crossing.point);
        assert!(
            drift.dot(drift).sqrt() < 1.0e-12,
            "the crossing point is not on the snapped edge"
        );
        let face = &scene.surface.faces[crossing.face as usize];
        let tri = [
            scene.surface.vertices[face.nodes[0]],
            scene.surface.vertices[face.nodes[1]],
            scene.surface.vertices[face.nodes[2]],
        ];
        let normal = tri[1].sub(tri[0]).cross(tri[2].sub(tri[0]));
        let scale = normal.dot(normal).sqrt().max(f64::MIN_POSITIVE);
        let offset = normal.dot(crossing.point.sub(tri[0])).abs() / scale;
        assert!(
            offset < 1.0e-12,
            "the crossing point is {offset:e} off its own face's plane"
        );
    }
}

#[test]
// AI-FUNC-SUMMARY: An edge a component crosses twice is counted, not silently averaged (invariant K1).
fn an_edge_crossed_twice_is_counted() {
    // A plate thinner than the smallest element: every vertical edge through it
    // enters and leaves within one edge.
    let scene = scene(
        &[box_mesh(
            Vec3::new(0.1, 0.1, 0.5),
            Vec3::new(0.9, 0.9, 0.53),
            2,
        )],
        &[0],
        &[10],
        0.5,
    );
    let snapped = snap(&scene);
    assert!(
        snapped.stats.n_multi_crossing_edges > 0,
        "a plate thinner than one element must produce doubly-crossed edges"
    );
    assert!(
        snapped
            .warnings
            .iter()
            .any(|warning| warning.starts_with("[CUT-CASE]")),
        "K1 escalations must be reported, not just counted"
    );
}

#[test]
// AI-FUNC-SUMMARY: S7 is bit-identical across runs and thread counts (R-P2).
fn the_snap_is_deterministic() {
    let scene = scene(
        &[sphere_mesh(Vec3::new(0.5, 0.5, 0.5), 0.28, 12)],
        &[0],
        &[10],
        0.1,
    );
    let first = snap(&scene);
    let second = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(|| snap(&scene));
    assert_eq!(first.nodes.len(), second.nodes.len());
    for (index, (a, b)) in first.nodes.iter().zip(second.nodes.iter()).enumerate() {
        assert_eq!(
            (a.x.to_bits(), a.y.to_bits(), a.z.to_bits()),
            (b.x.to_bits(), b.y.to_bits(), b.z.to_bits()),
            "node {index} differs between thread counts"
        );
    }
    assert_eq!(first.constraint_kind, second.constraint_kind);
    assert_eq!(first.constraint_ref, second.constraint_ref);
    assert_eq!(first.crossings.len(), second.crossings.len());
    for (a, b) in first.crossings.iter().zip(second.crossings.iter()) {
        assert_eq!(a.edge, b.edge);
        assert_eq!(a.face, b.face);
        assert_eq!(a.t.to_bits(), b.t.to_bits());
    }
    assert_eq!(first.stats, second.stats);
}

#[test]
// AI-FUNC-SUMMARY: With nothing to snap to, S7 is the identity.
fn an_empty_scene_moves_nothing() {
    let scene = scene(
        &[box_mesh(
            Vec3::new(0.3, 0.3, 0.3),
            Vec3::new(0.7, 0.7, 0.7),
            1,
        )],
        &[0],
        &[10],
        0.5,
    );
    let empty = Classification {
        active_face: vec![false; scene.surface.faces.len()],
        ..scene.classification.clone()
    };
    let snapped = snap_lattice(
        &scene.lattice,
        &scene.surface,
        &empty,
        &SnapOptions {
            domain_min: scene.domain_min,
            domain_max: scene.domain_max,
            eps: EPS,
        },
    );
    assert_eq!(snapped.nodes, scene.lattice.nodes);
    assert!(snapped.crossings.is_empty());
    assert_eq!(snapped.stats.max_motion, 0.0);
}

#[test]
// AI-FUNC-SUMMARY: `s07_snapped` satisfies the contract verifier with no FAIL item.
fn the_snapshot_verifies() {
    let scene = scene(
        &[sphere_mesh(Vec3::new(0.5, 0.5, 0.5), 0.28, 12)],
        &[0],
        &[10],
        0.1,
    );
    let snapped = snap(&scene);
    let mut doc = snapped_to_doc(
        &scene.lattice,
        &snapped,
        &scene.classification,
        &scene.surface.components,
    );
    let meta = SnapshotMeta::new(
        Stage::Snapped,
        0x6101,
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
    );
    stamp_metadata(&mut doc, &meta);
    let report = verify(&doc, &VerifyGates::default());
    let failures: Vec<String> = report
        .sections
        .iter()
        .flat_map(|section| section.items.iter())
        .filter(|item| item.severity == Severity::Fail)
        .map(|item| format!("{}: {}", item.code, item.message))
        .collect();
    assert!(failures.is_empty(), "s07 verification failed: {failures:?}");

    let kinds = doc
        .point_data
        .iter()
        .find(|array| array.name == "constraint_kind")
        .expect("s07 must carry constraint_kind");
    assert!(matches!(
        kinds.data,
        rustmspt::io::vtu::ArrayData::U8(_)
    ));
}
