#![cfg(feature = "gpu")]

use rustmspt::geometry::box_mesh;
use rustmspt::gpu::{
    gpu_device_creation_count, gpu_pipeline_build_count, shared_gpu_device, GpuRenderPipeline,
    GpuS2Pipeline, GpuScenePipeline, GpuShellS2Pipeline, GpuVolumeTransformPipeline,
    GpuVoxelPipeline,
};
use rustmspt::types::{BoundingBox, Vec3};
use std::sync::Arc;

// AI-FUNC-SUMMARY: Construct one instance of every GPU pipeline type on the shared device and run a small MC and voxel evaluation; panics on any failure.
fn construct_all() {
    let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
    let mesh = box_mesh(BoundingBox {
        min: Vec3::new(0.5, 0.5, 0.5),
        max: Vec3::new(2.5, 2.5, 2.5),
    });
    let mut mc = GpuS2Pipeline::new(&mesh, bbox).unwrap();
    assert_eq!(mc.calculate_s2_gpu(bbox, 2, 200).unwrap().len(), 3);
    let mut voxel = GpuVoxelPipeline::new(&mesh, bbox).unwrap();
    assert_eq!(voxel.voxelize(4, 4, 4, 1.0).unwrap().len(), 64);
    GpuShellS2Pipeline::new().unwrap();
    GpuVolumeTransformPipeline::new().unwrap();
    GpuRenderPipeline::new().unwrap();
    GpuScenePipeline::new().unwrap();
}

// AI-FUNC-SUMMARY: One process-level scenario (a single test, so no sibling test perturbs the global counters): repeated and concurrent constructors create one device and compile each pipeline once; an invalid selector errors without being cached; a destroyed device is evicted and recreated.
#[test]
fn shared_device_and_pipeline_cache_contract() {
    std::env::remove_var("RUSTMSPT_GPU_DEVICE");
    let first = match shared_gpu_device() {
        Ok(device) => device,
        Err(error) => {
            eprintln!("SKIP: GPU unavailable: {error}");
            return;
        }
    };
    assert_eq!(gpu_device_creation_count(), 1);
    let cold = std::time::Instant::now();
    construct_all();
    let cold = cold.elapsed();
    let builds = gpu_pipeline_build_count();
    assert!(builds >= 6, "every pipeline family compiled once, got {builds}");
    let warm = std::time::Instant::now();
    for _ in 0..3 {
        construct_all();
    }
    println!(
        "GPU_DEVICE_CACHE_TIMING first_round_seconds={:.6} warm_round_mean_seconds={:.6}",
        cold.as_secs_f64(),
        warm.elapsed().as_secs_f64() / 3.0
    );
    assert_eq!(gpu_pipeline_build_count(), builds, "repeated constructors recompiled");
    assert_eq!(gpu_device_creation_count(), 1, "repeated constructors created devices");

    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    construct_all();
                    shared_gpu_device().unwrap()
                })
            })
            .collect();
        for handle in handles {
            assert!(Arc::ptr_eq(&handle.join().unwrap(), &first));
        }
    });
    assert_eq!(gpu_device_creation_count(), 1, "concurrent constructors created devices");
    assert_eq!(gpu_pipeline_build_count(), builds);

    std::env::set_var("RUSTMSPT_GPU_DEVICE", "definitely-no-cached-adapter");
    for _ in 0..2 {
        let error = shared_gpu_device().err().expect("invalid selector must fail");
        assert!(error.to_string().contains("definitely-no-cached-adapter"));
        assert!(GpuVolumeTransformPipeline::new().is_err());
    }
    std::env::remove_var("RUSTMSPT_GPU_DEVICE");
    assert_eq!(gpu_device_creation_count(), 1, "a failed selector created a device");
    assert!(Arc::ptr_eq(&shared_gpu_device().unwrap(), &first));

    first.device().destroy();
    let _ = first.device().poll(wgpu::Maintain::Wait);
    assert!(first.is_lost(), "destroy must signal device loss");
    let second = shared_gpu_device().unwrap();
    assert!(!Arc::ptr_eq(&second, &first));
    assert_eq!(gpu_device_creation_count(), 2);
    construct_all();
    assert_eq!(gpu_pipeline_build_count(), builds * 2, "new device recompiles each family once");
    assert!(Arc::ptr_eq(&shared_gpu_device().unwrap(), &second));
    println!(
        "GPU_DEVICE_CACHE devices={} pipeline_builds={} adapter={} backend={:?}",
        gpu_device_creation_count(),
        gpu_pipeline_build_count(),
        second.info().name,
        second.info().backend
    );
}
