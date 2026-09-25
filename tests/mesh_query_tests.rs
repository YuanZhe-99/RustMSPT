use rand::{Rng, SeedableRng};
use rustmspt::geometry::{
    box_mesh, icosphere_mesh, merge_meshes, point_inside_mesh, translate_mesh, MeshQueryScratch,
    PreparedMeshQuery,
};
use rustmspt::types::{BoundingBox, Mesh, Vec3};

// AI-FUNC-SUMMARY: Build nested, disconnected and concave/shared-face meshes with known difficult ray topology for differential queries.
fn fixtures() -> Vec<Mesh> {
    let cube = |a, b| {
        box_mesh(BoundingBox {
            min: Vec3::new(a, a, a),
            max: Vec3::new(b, b, b),
        })
    };
    vec![
        cube(-1.0, 1.0),
        merge_meshes(&[cube(-2.0, 2.0), cube(-1.0, 1.0), cube(-0.5, 0.5)]),
        merge_meshes(&[
            cube(-2.0, 0.0),
            cube(0.0, 2.0),
            box_mesh(BoundingBox {
                min: Vec3::new(-2.0, 0.0, -2.0),
                max: Vec3::new(0.0, 2.0, 0.0),
            }),
        ]),
        icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 2.0, 3),
    ]
}

// AI-FUNC-SUMMARY: Compare prepared containment with the untouched full-scan oracle on seeded points, vertices, edge midpoints and near-surface offsets across scales and translations.
#[test]
fn prepared_queries_match_full_scan() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(77123);
    for source in fixtures() {
        for (scale, shift) in [(1.0, 0.0), (1.0, 1e9), (1e-3, 0.0), (1e3, 0.0)] {
            let mut mesh = source.clone();
            for vertex in &mut mesh.vertices {
                *vertex = vertex.scale(scale);
            }
            translate_mesh(&mut mesh, Vec3::new(shift, -shift, shift));
            let query = PreparedMeshQuery::new(&mesh);
            let mut scratch = MeshQueryScratch::default();
            let mut points: Vec<_> = (0..1500)
                .map(|_| {
                    Vec3::new(
                        shift + rng.gen_range(-3.0..3.0) * scale,
                        -shift + rng.gen_range(-3.0..3.0) * scale,
                        shift + rng.gen_range(-3.0..3.0) * scale,
                    )
                })
                .collect();
            points.extend(mesh.vertices.iter().copied());
            for f in mesh.faces.iter().step_by(7) {
                let a = mesh.vertices[f.a];
                let b = mesh.vertices[f.b];
                points.push(a.add(b).scale(0.5));
                for eps in [-1e-8, 0.0, 1e-8] {
                    points.push(a.add(Vec3::new(eps, eps, -eps)));
                }
            }
            for point in points {
                assert_eq!(
                    query.contains_point(point, &mut scratch),
                    point_inside_mesh(&mesh, point),
                    "point={point:?}, scale={scale}"
                );
            }
        }
    }
}

// AI-FUNC-SUMMARY: Ensure BVH preparation removes most triangle tests on a separated-particle query workload while preserving the standalone result.
#[test]
fn prepared_queries_reduce_triangle_work() {
    let meshes: Vec<_> = (0..64)
        .map(|i| icosphere_mesh(Vec3::new(i as f64 * 4.0, 0.0, 0.0), 1.0, 1))
        .collect();
    let mesh = merge_meshes(&meshes);
    let query = PreparedMeshQuery::new(&mesh);
    let mut scratch = MeshQueryScratch::default();
    let mut tested = 0;
    for i in 0..64 {
        let p = Vec3::new(i as f64 * 4.0, 0.0, 0.0);
        assert_eq!(
            query.contains_point(p, &mut scratch),
            point_inside_mesh(&mesh, p)
        );
        tested += scratch.triangle_tests;
    }
    assert!(tested < mesh.faces.len() * 64 / 10, "tested={tested}");
}

// AI-FUNC-SUMMARY: Benchmark full-scan and prepared queries on identical seeded points with warmup and five raw trials; include preparation time and triangle-query counts for synthetic and real STL inputs.
#[test]
#[ignore = "release performance benchmark"]
fn query_benchmark() {
    use std::hint::black_box;
    use std::time::Instant;
    let real = rustmspt::io::load_stl(std::path::Path::new("data/input/particles.stl")).unwrap();
    for (name, mesh) in [
        (
            "small",
            box_mesh(BoundingBox::from_size(Vec3::new(2.0, 2.0, 2.0))),
        ),
        ("sphere", icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 2.0, 3)),
        ("real", real),
    ] {
        let bbox = rustmspt::geometry::mesh_bbox(&mesh).unwrap();
        let mut rng = rand::rngs::StdRng::seed_from_u64(17981);
        let points: Vec<_> = (0..10_000)
            .map(|_| {
                Vec3::new(
                    rng.gen_range(bbox.min.x..bbox.max.x),
                    rng.gen_range(bbox.min.y..bbox.max.y),
                    rng.gen_range(bbox.min.z..bbox.max.z),
                )
            })
            .collect();
        let expected: Vec<_> = points
            .iter()
            .map(|&p| point_inside_mesh(&mesh, p))
            .collect();
        for trial in 0..6 {
            let start = Instant::now();
            let original: Vec<_> = points
                .iter()
                .map(|&p| black_box(point_inside_mesh(black_box(&mesh), p)))
                .collect();
            let full_s = start.elapsed().as_secs_f64();
            let start = Instant::now();
            let query = PreparedMeshQuery::new(&mesh);
            let prepare_s = start.elapsed().as_secs_f64();
            let mut scratch = MeshQueryScratch::default();
            let start = Instant::now();
            let mut triangle_tests = 0;
            let actual: Vec<_> = points
                .iter()
                .map(|&p| {
                    let result = black_box(query.contains_point(p, &mut scratch));
                    triangle_tests += scratch.triangle_tests;
                    result
                })
                .collect();
            let query_s = start.elapsed().as_secs_f64();
            assert_eq!(actual, expected);
            assert_eq!(original, expected);
            println!(
                "{}",
                serde_json::json!({"case":name,"trial":trial,"warmup":trial==0,"vertices":mesh.vertices.len(),"faces":mesh.faces.len(),"queries":points.len(),"full_s":full_s,"prepare_s":prepare_s,"query_s":query_s,"triangle_tests":triangle_tests,"full_triangle_tests":mesh.faces.len()*points.len()})
            );
        }
    }
}

// AI-FUNC-SUMMARY: Compare seeded MC reference and prepared kernels on identical samples and across worker counts, including an incomplete final sample block.
#[test]
fn seeded_mesh_mc_matches_reference_across_workers() {
    let mesh = icosphere_mesh(Vec3::new(2.0, 2.0, 2.0), 1.5, 2);
    let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
    let mut expected = None;
    for workers in [1, 2, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let a = pool.install(|| {
            rustmspt::geometry::s2::calculate_s2_mesh_mc_seeded(&mesh, bbox, 3, 5001, 17981, false)
        });
        let b = pool.install(|| {
            rustmspt::geometry::s2::calculate_s2_mesh_mc_seeded(&mesh, bbox, 3, 5001, 17981, true)
        });
        assert_eq!(a, b);
        if let Some(expected) = &expected {
            assert_eq!(&b, expected);
        } else {
            expected = Some(b);
        }
    }
}

// AI-FUNC-SUMMARY: Benchmark the full seeded mesh-MC computation including VF and preparation, comparing identical samples and budgets at 1/2/8 workers on real STL geometry.
#[test]
#[ignore = "release performance benchmark"]
fn seeded_mc_benchmark() {
    use std::time::Instant;
    let mesh = rustmspt::io::load_stl(std::path::Path::new("data/input/particles.stl")).unwrap();
    let bbox = rustmspt::geometry::mesh_bbox(&mesh).unwrap();
    for workers in [1, 2, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        for trial in 0..6 {
            let seed = 17981 + trial;
            let start = Instant::now();
            let reference = pool.install(|| {
                rustmspt::geometry::s2::calculate_s2_mesh_mc_seeded(
                    &mesh, bbox, 2, 5001, seed, false,
                )
            });
            let full_s = start.elapsed().as_secs_f64();
            let start = Instant::now();
            let prepared = pool.install(|| {
                rustmspt::geometry::s2::calculate_s2_mesh_mc_seeded(
                    &mesh, bbox, 2, 5001, seed, true,
                )
            });
            let prepared_s = start.elapsed().as_secs_f64();
            assert_eq!(reference, prepared);
            println!(
                "{}",
                serde_json::json!({"workers":workers,"trial":trial,"warmup":trial==0,"seed":seed,"samples":5001,"r_max":2,"full_s":full_s,"prepared_s":prepared_s,"curve":prepared})
            );
        }
    }
}

// AI-FUNC-SUMMARY: Verify shared occupancy keeps exact values and occupancy VF, including particles wholly below the sampling domain.
#[test]
fn shared_voxel_grid_preserves_exact_and_mc_vf() {
    use rustmspt::geometry::s2::{calculate_s2, VoxelS2};
    let bbox = BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(4.0, 3.0, 2.0),
    };
    let mesh = merge_meshes(&[
        box_mesh(BoundingBox {
            min: Vec3::new(0.1, 0.1, 0.1),
            max: Vec3::new(1.9, 1.9, 1.9),
        }),
        box_mesh(BoundingBox {
            min: Vec3::new(-10.0, -10.0, -10.0),
            max: Vec3::new(-8.0, -8.0, -8.0),
        }),
    ]);
    let prepared = VoxelS2::new(&mesh, bbox, 1.0);
    let exact = prepared.calculate(4, "exact", 200);
    assert_eq!(exact, calculate_s2(&mesh, bbox, 4, 1.0, "exact", 200));
    assert_eq!(exact[0], 8.0 / 24.0);
    for _ in 0..3 {
        let mc = prepared.calculate(4, "monte_carlo", 500);
        assert_eq!(mc[0], exact[0]);
        assert!(mc.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }
}

// AI-FUNC-SUMMARY: Voxel Monte Carlo S2 must estimate the same quantity as exact S2: on a voxelized sphere with 200k samples per radius every radius lands within 0.01 of exact (about 5 binomial sigma); guards the fast MC generator and its uniform draws; no file output.
#[test]
fn voxel_monte_carlo_estimates_the_exact_s2() {
    use rustmspt::geometry::s2::VoxelS2;
    let bbox = BoundingBox { min: Vec3::new(0.0, 0.0, 0.0), max: Vec3::new(30.0, 30.0, 30.0) };
    let mesh = merge_meshes(&[
        icosphere_mesh(Vec3::new(10.0, 12.0, 14.0), 7.0, 3),
        icosphere_mesh(Vec3::new(21.0, 18.0, 15.0), 5.0, 3),
    ]);
    let grid = VoxelS2::new(&mesh, bbox, 1.0);
    let exact = grid.calculate(8, "exact", 200);
    let mc = grid.calculate(8, "monte_carlo", 200_000);
    assert_eq!(mc[0], exact[0]);
    for r in 1..=8 {
        assert!((mc[r] - exact[r]).abs() < 0.01, "r {r}: mc {} exact {}", mc[r], exact[r]);
    }
    assert!(exact[8] < exact[1], "the fixture must decay, or agreement says little");
}
