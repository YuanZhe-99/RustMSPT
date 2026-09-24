#![cfg(feature = "gpu")]

use rustmspt::geometry::box_mesh;
use rustmspt::gpu::{GpuShellS2Pipeline, GpuVoxelPipeline};
use rustmspt::types::{BoundingBox, Vec3};

// AI-FUNC-SUMMARY: Compare GPU voxel occupancy with analytic box containment, including a large translated origin.
#[test]
fn voxel_occupancy_preserves_translated_box() {
    for origin in [0.0, 1e9] {
        let bbox = BoundingBox {
            min: Vec3::new(origin, origin, origin),
            max: Vec3::new(origin + 4.0, origin + 3.0, origin + 2.0),
        };
        let mesh = box_mesh(BoundingBox {
            min: Vec3::new(origin + 0.25, origin + 0.25, origin + 0.25),
            max: Vec3::new(origin + 2.25, origin + 1.25, origin + 1.25),
        });
        let mut gpu = match GpuVoxelPipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("SKIP: GPU voxel unavailable: {err}");
                return;
            }
        };
        let expected: Vec<u32> = (0..4)
            .flat_map(|x| {
                (0..3).flat_map(move |y| (0..2).map(move |z| u32::from(x < 2 && y == 0 && z == 0)))
            })
            .collect();
        assert_eq!(
            gpu.voxelize(4, 3, 2, 1.0).unwrap(),
            expected,
            "origin={origin}"
        );
    }
}

// AI-FUNC-SUMMARY: Check signed out-of-domain shell offsets against exhaustive pair enumeration and preserve equal per-offset weighting across upload chunks.
#[test]
fn shell_boundaries_and_chunk_reduction_match_cpu() {
    let mut gpu = match GpuShellS2Pipeline::new() {
        Ok(gpu) => gpu,
        Err(err) => {
            eprintln!("SKIP: GPU shell unavailable: {err}");
            return;
        }
    };
    let dims = [3isize, 2, 4];
    let occ: Vec<u32> = (0..24).map(|i| u32::from(i % 3 != 0)).collect();
    let mut offsets = vec![
        (1, [1, 0, 0]),
        (1, [-1, 0, 0]),
        (1, [0, 1, 0]),
        (1, [0, 0, -2]),
        (1, [4, 0, 0]),
        (1, [-4, 0, 0]),
        (1, [0, 3, 0]),
        (1, [0, 0, 5]),
        (1, [i32::MIN as isize, 0, 0]),
        (1, [isize::MAX, 0, 0]),
    ];
    for axis in 0..3 {
        for displacement in [-dims[axis] - 1, -dims[axis], dims[axis], dims[axis] + 1] {
            let mut delta = [0; 3];
            delta[axis] = displacement;
            offsets.push((1, delta));
        }
    }
    let mut sum = 0.0;
    let mut count = 0;
    for &(_, delta) in &offsets {
        let (mut valid, mut hits) = (0, 0);
        for x in 0..dims[0] {
            for y in 0..dims[1] {
                for z in 0..dims[2] {
                    let other = [
                        x as i128 + delta[0] as i128,
                        y as i128 + delta[1] as i128,
                        z as i128 + delta[2] as i128,
                    ];
                    if (0..3).all(|a| other[a] >= 0 && other[a] < dims[a] as i128) {
                        valid += 1;
                        let i = (x * 8 + y * 4 + z) as usize;
                        let j = (other[0] * 8 + other[1] * 4 + other[2]) as usize;
                        hits += usize::from(occ[i] == 1 && occ[j] == 1);
                    }
                }
            }
        }
        if valid > 0 {
            sum += hits as f64 / valid as f64;
            count += 1;
        }
    }
    let actual = gpu
        .compute_s2_shell(&occ, 3, 2, 4, &offsets, 1, 1.0, 2.0 / 3.0)
        .unwrap();
    assert_eq!(actual, vec![2.0 / 3.0, sum / count as f64]);
    let mut many = vec![(1, [1, 0, 0]); 200_000];
    many.push((1, [0, 0, 0]));
    let actual = gpu
        .compute_s2_shell(&[1, 0], 2, 1, 1, &many, 1, 1.0, 0.5)
        .unwrap();
    assert_eq!(actual, vec![0.5, 0.5 / 200_001.0]);
    assert_eq!(
        gpu.compute_s2_shell(&[1, 0], 2, 1, 1, &[], 1, 1.0, 0.5)
            .unwrap(),
        vec![0.5, 0.0]
    );
}

// AI-FUNC-SUMMARY: Construct disjoint closed boxes whose forward ray intersections exceed the local shader array capacity.
fn overflow_boxes(include_origin: bool, count: usize) -> rustmspt::types::Mesh {
    let (dx, dy, dz) = rustmspt::geometry::s2::RAY_DIR_GPU;
    let mut boxes = Vec::new();
    if include_origin {
        boxes.push(box_mesh(BoundingBox {
            min: Vec3::new(-1.0, -1.0, -1.0),
            max: Vec3::new(2.0, 2.0, 2.0),
        }));
    }
    for i in 1..=count {
        let t = i as f64 * 8.0;
        boxes.push(box_mesh(BoundingBox {
            min: Vec3::new(dx * t - 2.0, dy * t - 2.0, dz * t - 2.0),
            max: Vec3::new(dx * t + 3.0, dy * t + 3.0, dz * t + 3.0),
        }));
    }
    rustmspt::geometry::merge_meshes(&boxes)
}

// AI-FUNC-SUMMARY: Verify inside/outside parity after more than 64 ray hits, independently of face order, using analytic disjoint boxes.
#[test]
fn voxel_ray_overflow_preserves_parity() {
    let bbox = BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    };
    for count in [31, 32, 33] {
        for inside in [true, false] {
            let mut mesh = overflow_boxes(inside, count);
            for reverse in [false, true] {
                if reverse {
                    mesh.faces.reverse();
                }
                let mut gpu = match GpuVoxelPipeline::new(&mesh, bbox) {
                    Ok(gpu) => gpu,
                    Err(err) => {
                        eprintln!("SKIP: GPU voxel unavailable: {err}");
                        return;
                    }
                };
                assert_eq!(
                    gpu.voxelize(1, 1, 1, 1.0).unwrap(),
                    vec![u32::from(inside)],
                    "inside={inside}, reversed={reverse}"
                );
            }
        }
    }
}

// AI-FUNC-SUMMARY: Verify GPU Monte Carlo containment recovers overflow rays without discarding trials; the sampled domain is wholly solid or empty.
#[test]
fn mc_ray_overflow_preserves_parity() {
    let bbox = BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    };
    for inside in [true, false] {
        let mesh = overflow_boxes(inside, 33);
        let mut gpu = match rustmspt::gpu::GpuS2Pipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("SKIP: GPU MC unavailable: {err}");
                return;
            }
        };
        let s2 = gpu.calculate_s2_gpu(bbox, 1, 2000).unwrap();
        assert_eq!(s2[1], if inside { 1.0 } else { 0.0 });
    }
}

// AI-FUNC-SUMMARY: Verify raw-hit overflow caused by coincident triangles preserves deduplication parity at and above the 64-hit boundary.
#[test]
fn voxel_ray_overflow_deduplicates_coincident_hits() {
    let bbox = BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    };
    for copies in [64, 65] {
        let mut mesh = box_mesh(bbox);
        mesh.faces = (0..copies)
            .flat_map(|_| mesh.faces.iter().cloned())
            .collect();
        let mut gpu = match GpuVoxelPipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("SKIP: GPU voxel unavailable: {err}");
                return;
            }
        };
        assert_eq!(
            gpu.voxelize(1, 1, 1, 1.0).unwrap(),
            vec![1],
            "copies={copies}"
        );
    }
}

// AI-FUNC-SUMMARY: Verify direct voxel/shell constructors honor name and index selection in isolated child processes.
#[test]
fn s2_gpu_constructors_obey_device_filter() {
    const CHILD: &str = "RUSTMSPT_TEST_GPU_SELECTOR_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let filter = std::env::var("RUSTMSPT_GPU_DEVICE").unwrap();
        let bbox = BoundingBox { min: Vec3::new(0.0, 0.0, 0.0), max: Vec3::new(1.0, 1.0, 1.0) };
        let mesh = box_mesh(bbox);
        for error in [GpuVoxelPipeline::new(&mesh, bbox).err(), GpuShellS2Pipeline::new().err(), rustmspt::gpu::GpuS2Pipeline::new(&mesh, bbox).err(), rustmspt::gpu::try_init_gpu().err().map(|e| e.to_string())] {
            let error = error.expect("unavailable selected device must not use a default adapter");
            assert!(error.contains(&filter), "{error}");
        }
        return;
    }
    for filter in ["definitely-no-such-rustmspt-adapter".to_string(), usize::MAX.to_string()] {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "s2_gpu_constructors_obey_device_filter", "--nocapture"])
            .env(CHILD, "1").env("RUSTMSPT_GPU_DEVICE", filter).output().unwrap();
        assert!(result.status.success(), "{} {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    }
}
