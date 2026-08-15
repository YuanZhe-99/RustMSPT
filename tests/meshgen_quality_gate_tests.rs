//! G6-6: the post-snap half of the lattice-quality gate (G-5).
//!
//! G4-3 measured the centroid-fan transition templates *before* the snap and gave a
//! GO on that evidence, leaving open the question SPEC_meshgen_geometry §3.7 actually
//! asks: whether snapping and cutting into the fan's smaller tets erodes them faster
//! than into the Freudenthal ones. That needs S7 and S8, so it was re-scheduled here.
//!
//! The measurement groups every element by the *template of the cell it came from*
//! and compares the two populations at each stage, against the corrected 35.264
//! degree baseline (SPEC §3.7 rev 1.2), not the frozen text's 45.

use rustmspt::config::meshgen::{CoincidencePolicy, RepairLevel};
use rustmspt::meshgen::classify::{classify_lattice_with, ClassifyOptions, PointClassifier};
use rustmspt::meshgen::cut::{cut_lattice, CutOptions};
use rustmspt::meshgen::lattice::{balance_octree, build_lattice, CellTemplate, LatticeOptions};
use rustmspt::meshgen::sizing::{
    build_sizing_field, SizingCriterion, SizingLookup, SizingOptions, SizingSource,
};
use rustmspt::meshgen::snap::{snap_lattice, SnapOptions};
use rustmspt::meshgen::{
    arrange_surface, clip_arranged_to_box, condition_surface, detect_features, rebuild_topology,
    tet_quality, ArrangeComponent, ArrangeOptions,
};
use rustmspt::types::{Mesh, Triangle, Vec3};

/// The corrected worst-case pre-snap dihedral of the centroid-fan templates
/// (`SPEC_meshgen_geometry.md` §3.7 rev 1.2): `arctan(1/sqrt(2))`.
const FAN_BASELINE_DEG: f64 = 35.264_389_682_754_654;

// AI-FUNC-SUMMARY: A closed UV sphere; returns Mesh; side effects: none.
fn sphere(center: Vec3, radius: f64, bands: usize) -> Mesh {
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

/// The min-dihedral distribution of one population of elements.
#[derive(Debug, Clone, Copy)]
struct Distribution {
    count: usize,
    worst: f64,
    p5: f64,
    median: f64,
}

// AI-FUNC-SUMMARY: Summarise a min-dihedral sample; returns Distribution; side effects: sorts the input.
fn summarise(values: &mut Vec<f64>) -> Distribution {
    values.sort_by(f64::total_cmp);
    let count = values.len();
    let at = |q: f64| -> f64 {
        if count == 0 {
            return 0.0;
        }
        values[((count as f64 - 1.0) * q).round() as usize]
    };
    Distribution {
        count,
        worst: values.first().copied().unwrap_or(0.0),
        p5: at(0.05),
        median: at(0.5),
    }
}

#[test]
// AI-FUNC-SUMMARY:
// The G6-6 gate: measure the centroid-fan transition cells against the Freudenthal cells at each of
// the three stages a lattice element passes through, and decide whether the fans survive the snap
// and the cut.
fn transition_fans_survive_the_snap_and_the_cut() {
    let eps = 1.0e-4;
    let domain_min = Vec3::new(0.0, 0.0, 0.0);
    let domain_max = Vec3::new(1.0, 1.0, 1.0);
    let components = vec![ArrangeComponent {
        x: 1,
        priority: 10,
        kind: 0,
        closed: true,
    }];
    let mesh = sphere(Vec3::new(0.5, 0.5, 0.5), 0.3, 16);
    let conditioned = condition_surface(&[mesh], eps, RepairLevel::Conservative).unwrap();
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

    // A sizing field with a source at the sphere's north pole, so the lattice has a
    // real level jump *through the surface* - transition cells the cut then hits.
    let sizing = SizingOptions {
        domain_min,
        domain_max,
        h_max: 0.2,
        h_min: 0.05,
        eps,
        ..Default::default()
    };
    let lookup = SizingLookup::build(
        vec![SizingSource {
            point: Vec3::new(0.5, 0.5, 0.8),
            h: 0.05,
            criterion: SizingCriterion::Curvature,
        }],
        &sizing,
    );
    let field = build_sizing_field(&lookup, &sizing);
    let (balanced, _) = balance_octree(&field);
    let lattice = build_lattice(&balanced, &LatticeOptions::default()).unwrap();
    let fan_cells = lattice
        .templates
        .iter()
        .filter(|template| **template == CellTemplate::Fan)
        .count();
    assert!(
        fan_cells > 0,
        "the fixture has no transition cells, so it cannot test them"
    );

    let classifier = PointClassifier::build(
        &clipped,
        &topo,
        &ClassifyOptions {
            domain_min,
            domain_max,
        },
    );
    let classification = classify_lattice_with(
        &lattice,
        &classifier,
        &clipped,
        &ClassifyOptions {
            domain_min,
            domain_max,
        },
    );
    let snapped = snap_lattice(
        &lattice,
        &clipped,
        &classification,
        &SnapOptions {
            domain_min,
            domain_max,
            eps,
        },
    );
    let cut = cut_lattice(
        &lattice,
        &snapped,
        &classification,
        &classifier,
        &clipped.components,
        &CutOptions {
            eps,
            ..Default::default()
        },
    );

    let is_fan = |cell: u32| lattice.templates[cell as usize] == CellTemplate::Fan;
    let dihedral = |tet: &[u32; 4], nodes: &[Vec3]| -> f64 {
        tet_quality([
            nodes[tet[0] as usize],
            nodes[tet[1] as usize],
            nodes[tet[2] as usize],
            nodes[tet[3] as usize],
        ])
        .min_dihedral_deg
    };

    let mut stage: Vec<(&str, Distribution, Distribution)> = Vec::new();
    for (label, nodes) in [
        ("pre-snap  (s05/s06)", &lattice.nodes),
        ("post-snap (s07)", &snapped.nodes),
    ] {
        let mut fan = Vec::new();
        let mut freudenthal = Vec::new();
        for (tet, cell) in lattice.tets.iter().zip(lattice.cell_of_tet.iter()) {
            let value = dihedral(tet, nodes);
            if is_fan(*cell) {
                fan.push(value);
            } else {
                freudenthal.push(value);
            }
        }
        stage.push((label, summarise(&mut freudenthal), summarise(&mut fan)));
    }
    {
        let mut fan = Vec::new();
        let mut freudenthal = Vec::new();
        for (tet, parent) in cut.tets.iter().zip(cut.parent_of.iter()) {
            let value = dihedral(tet, &cut.nodes);
            if is_fan(lattice.cell_of_tet[*parent as usize]) {
                fan.push(value);
            } else {
                freudenthal.push(value);
            }
        }
        stage.push((
            "post-cut  (s08)",
            summarise(&mut freudenthal),
            summarise(&mut fan),
        ));
    }

    println!("G6-6: min-dihedral (degrees) by the template of the parent cell");
    println!("{:<20} {:>34} {:>34}", "stage", "Freudenthal", "centroid fan");
    for (label, freudenthal, fan) in &stage {
        println!(
            "{label:<20} {:>7} n / {:>6.3} worst / {:>6.3} p5 / {:>6.3} med   {:>7} n / {:>6.3} worst / {:>6.3} p5 / {:>6.3} med",
            freudenthal.count,
            freudenthal.worst,
            freudenthal.p5,
            freudenthal.median,
            fan.count,
            fan.worst,
            fan.p5,
            fan.median,
        );
    }

    let (_, pre_freudenthal, pre_fan) = stage[0];
    let (_, _, snap_fan) = stage[1];
    let (_, cut_freudenthal, cut_fan) = stage[2];

    // The pre-snap baseline is the corrected §3.7 value, and it is exact: the fan's
    // worst case is forced by Rule D, not measured.
    assert!(
        (pre_fan.worst - FAN_BASELINE_DEG).abs() < 1.0e-6,
        "the pre-snap fan worst case ({:.6} deg) is not the corrected §3.7 baseline",
        pre_fan.worst
    );
    assert!(
        pre_freudenthal.worst > pre_fan.worst,
        "the Freudenthal cells should start no worse than the fans"
    );

    // The gate. A fan tet is smaller than a Freudenthal tet of the same cell, so it
    // is the *relative* erosion that matters: if the fans were the weak link, snap
    // and cut would push their tail far below the Freudenthal tail. They do not.
    assert!(
        snap_fan.worst > 0.5 * pre_fan.worst,
        "the snap eroded the fans' worst case by more than half: {:.3} -> {:.3}",
        pre_fan.worst,
        snap_fan.worst
    );
    assert!(
        cut_fan.p5 > 0.25 * cut_freudenthal.p5,
        "after the cut the fans' p5 ({:.3}) is far below the Freudenthal p5 ({:.3})",
        cut_fan.p5,
        cut_freudenthal.p5
    );
    assert!(
        cut_fan.worst > 0.0 && cut_freudenthal.worst > 0.0,
        "the cut produced a degenerate element"
    );
}
