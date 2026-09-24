use rustmspt::geometry::forging::{forge_owned, simulate_forging_ffd_with_tracking};
use rustmspt::geometry::{mesh_centroid, scale_mesh, translate_mesh};
use rustmspt::types::{BoundingBox, Mesh, Vec3};

// AI-FUNC-SUMMARY: Generate nonuniform vertices spanning the serial/parallel cutoff and including a large coordinate origin.
fn cloud(n: usize) -> Mesh {
    Mesh {
        vertices: (0..n)
            .map(|i| {
                Vec3::new(
                    1e8 + i as f64 * 0.01,
                    -2.0 + (i % 37) as f64,
                    (i % 113) as f64 * 0.07,
                )
            })
            .collect(),
        faces: vec![],
    }
}

// AI-FUNC-SUMMARY: Verify in-place scale/translate keeps per-coordinate arithmetic exactly across worker counts, negative factors and the chunk cutoff.
#[test]
fn parallel_scale_translate_matches_serial() {
    for n in [0, 12, 32767, 32768, 65539, 524289] {
        for factor in [-2.0, 0.0, 0.125, 3.0] {
            let source = cloud(n);
            let delta = Vec3::new(3.0, -0.5, 8.0);
            let expected: Vec<_> = source
                .vertices
                .iter()
                .map(|v| v.scale(factor).add(delta))
                .collect();
            for threads in [1, 2, 8] {
                let mut actual = source.clone();
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build()
                    .unwrap()
                    .install(|| {
                        scale_mesh(&mut actual, factor);
                        translate_mesh(&mut actual, delta);
                    });
                assert_eq!(actual.vertices, expected);
            }
        }
    }
}

// AI-FUNC-SUMMARY: Compare owned parallel FFD against explicit original axis/centroid arithmetic, including void closure, immutable public wrapper and ROI corner tracking.
#[test]
fn owned_forge_matches_original_arithmetic() {
    let lattice = BoundingBox {
        min: Vec3::new(1e8, -3.0, 0.0),
        max: Vec3::new(1e8 + 800.0, 40.0, 9.0),
    };
    let center = lattice.min.add(lattice.max).scale(0.5);
    for n in [0, 12, 65539, 524289] {
        let source = cloud(n);
        for axis in [0, 1, 2, 3] {
            for kind in ["particle", "VOID"] {
                let compression: f64 = 0.4;
                let axial = 1.0 - compression;
                let lateral = (1.0 / axial.sqrt()).powf(0.6);
                let transform = |p: Vec3| {
                    let d = p.sub(center);
                    let axis = axis.min(2);
                    Vec3::new(
                        center.x + d.x * if axis == 0 { axial } else { lateral },
                        center.y + d.y * if axis == 1 { axial } else { lateral },
                        center.z + d.z * if axis == 2 { axial } else { lateral },
                    )
                };
                let mut expected = source.clone();
                expected
                    .vertices
                    .iter_mut()
                    .for_each(|v| *v = transform(*v));
                if kind == "VOID" {
                    let c = mesh_centroid(&expected);
                    expected
                        .vertices
                        .iter_mut()
                        .for_each(|v| *v = c.add(v.sub(c).scale(0.96)));
                }
                let (actual, roi) = forge_owned(
                    source.clone(),
                    lattice,
                    Some(lattice),
                    compression,
                    axis,
                    0.6,
                    kind,
                    2.0,
                );
                assert_eq!(actual.vertices, expected.vertices);
                let roi = roi.unwrap();
                assert_eq!(roi.min, transform(lattice.min));
                assert_eq!(roi.max, transform(lattice.max));
                let (wrapper, _) = simulate_forging_ffd_with_tracking(
                    &source,
                    lattice,
                    None,
                    compression,
                    axis,
                    0.6,
                    kind,
                    2.0,
                );
                assert_eq!(wrapper.vertices, expected.vertices);
            }
        }
    }
}

// AI-FUNC-SUMMARY: Measure transform-only serial versus chunked scale on fixed vertices, excluding cloning and pool creation; print warmup plus five release samples.
#[test]
#[ignore = "release performance measurement"]
fn scale_benchmark() {
    use std::hint::black_box;
    use std::time::Instant;
    for n in [12, 32768, 2_000_000] {
        for workers in [1, 2, 8] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            for trial in 0..6 {
                let mut times = [0.0; 2];
                for mode in if trial % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let mut mesh = cloud(n);
                    times[mode] = pool.install(|| {
                        let start = Instant::now();
                        for _ in 0..20 {
                            if mode == 0 {
                                for v in &mut mesh.vertices {
                                    *v = v.scale(black_box(1.00001));
                                }
                            } else {
                                scale_mesh(&mut mesh, black_box(1.00001));
                            }
                            black_box(&mesh);
                        }
                        start.elapsed().as_secs_f64()
                    });
                }
                println!("scale n={n} workers={workers} trial={trial} repeats=20 serial_s={} chunked_s={}", times[0], times[1]);
            }
        }
    }
}

// AI-FUNC-SUMMARY: Compare byte-identical buffered STL output with the original direct-file writer on fixed meshes, including file creation and close but not fsync.
#[test]
#[ignore = "release performance measurement"]
fn stl_write_benchmark() {
    use rustmspt::geometry::box_mesh;
    use rustmspt::io::save_stl;
    use std::io::Write;
    use std::time::Instant;
    let folder = tempfile::tempdir().unwrap();
    for copies in [1, 500, 5000] {
        let mut mesh = box_mesh(BoundingBox::from_size(Vec3::new(1.0, 2.0, 3.0)));
        let faces = mesh.faces.clone();
        mesh.faces = (0..copies).flat_map(|_| faces.iter().cloned()).collect();
        for trial in 0..6 {
            let mut times = [0.0; 2];
            for mode in if trial % 2 == 0 { [0, 1] } else { [1, 0] } {
                let path = folder.path().join(format!("{mode}.stl"));
                let start = Instant::now();
                if mode == 1 {
                    save_stl(&path, &mesh, "benchmark").unwrap();
                } else {
                    let mut file = std::fs::File::create(&path).unwrap();
                    let mut header = [0u8; 80];
                    header[..9].copy_from_slice(b"benchmark");
                    file.write_all(&header).unwrap();
                    file.write_all(&(mesh.faces.len() as u32).to_le_bytes())
                        .unwrap();
                    for face in &mesh.faces {
                        for n in [0f32; 3] {
                            file.write_all(&n.to_le_bytes()).unwrap();
                        }
                        for v in [
                            mesh.vertices[face.a],
                            mesh.vertices[face.b],
                            mesh.vertices[face.c],
                        ] {
                            for c in [v.x, v.y, v.z] {
                                file.write_all(&(c as f32).to_le_bytes()).unwrap();
                            }
                        }
                        file.write_all(&0u16.to_le_bytes()).unwrap();
                    }
                }
                times[mode] = start.elapsed().as_secs_f64();
            }
            assert_eq!(
                std::fs::read(folder.path().join("0.stl")).unwrap(),
                std::fs::read(folder.path().join("1.stl")).unwrap()
            );
            println!(
                "stl_write triangles={} trial={trial} direct_s={} buffered_s={}",
                mesh.faces.len(),
                times[0],
                times[1]
            );
        }
    }
}
