#![cfg(feature = "gpu")]
//! Which kind of adapter a GPU test actually ran on.
//!
//! A test that skips because no adapter exists, one that runs on a software rasterizer and one that runs on
//! real hardware say three different things; only the last is evidence about GPU performance. The hardware
//! acceptance test is ignored by default and fails - rather than passes vacuously - on a software adapter.

use rustmspt::geometry::{calculate_s2, icosphere_mesh, merge_meshes};
use rustmspt::gpu::{classify_adapter, try_init_gpu, AdapterClass};
use rustmspt::types::{BoundingBox, Vec3};

// AI-FUNC-SUMMARY: Build an AdapterInfo with the given name and device type for classifier checks; returns it.
fn info(name: &str, device_type: wgpu::DeviceType) -> wgpu::AdapterInfo {
    wgpu::AdapterInfo {
        name: name.into(),
        vendor: 0,
        device: 0,
        device_type,
        driver: String::new(),
        driver_info: String::new(),
        backend: wgpu::Backend::Vulkan,
    }
}

// AI-FUNC-SUMMARY: CPU device types and known software rasterizers classify as software whatever type they report; integrated and discrete GPUs as hardware.
#[test]
fn adapters_are_classified_by_type_and_known_software_names() {
    for name in ["llvmpipe (LLVM 21.1.8, 128 bits)", "Lavapipe", "SwiftShader Device", "softpipe", "Microsoft Basic Render Driver"] {
        assert_eq!(classify_adapter(&info(name, wgpu::DeviceType::Other)), AdapterClass::Software, "{name}");
    }
    assert_eq!(classify_adapter(&info("some cpu device", wgpu::DeviceType::Cpu)), AdapterClass::Software);
    assert_eq!(classify_adapter(&info("Intel(R) Iris(R) Xe Graphics", wgpu::DeviceType::IntegratedGpu)), AdapterClass::Hardware);
    assert_eq!(classify_adapter(&info("NVIDIA GeForce RTX 4070", wgpu::DeviceType::DiscreteGpu)), AdapterClass::Hardware);
}

// AI-FUNC-SUMMARY: Report which adapter the GPU tests of this run execute on: skipped (none), software or hardware; prints one line and never fails.
#[test]
fn report_the_adapter_the_gpu_tests_run_on() {
    match try_init_gpu() {
        Ok(ctx) => println!("[GPU-TEST] adapter {}", ctx.describe()),
        Err(error) => println!("[GPU-TEST] adapter none: GPU tests skip ({error})"),
    }
}

// AI-FUNC-SUMMARY: Hardware acceptance (run with --ignored on a machine with a hardware GPU): refuses to pass on a software or missing adapter; otherwise checks certified GPU exact S2 against CPU exact on a two-sphere scene and prints both timings.
#[test]
#[ignore = "hardware GPU acceptance: run with --ignored on a machine with a real GPU"]
fn hardware_gpu_acceptance() {
    let ctx = try_init_gpu().expect("hardware GPU acceptance needs a GPU adapter; none was found");
    assert_eq!(
        ctx.adapter_class(),
        AdapterClass::Hardware,
        "hardware GPU acceptance not run: the adapter is a software rasterizer ({})",
        ctx.describe()
    );
    let bbox = BoundingBox::from_size(Vec3::new(48.0, 48.0, 48.0));
    let mesh = merge_meshes(&[
        icosphere_mesh(Vec3::new(17.0, 20.0, 23.0), 11.0, 4),
        icosphere_mesh(Vec3::new(32.0, 28.0, 25.0), 8.0, 4),
    ]);
    let t = std::time::Instant::now();
    let cpu = calculate_s2(&mesh, bbox, 16, 0.5, "exact", 200);
    let cpu_seconds = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let gpu = rustmspt::geometry::s2::try_calculate_s2_gpu_exact(&mesh, bbox, 16, 0.5).expect("GPU exact");
    let gpu_seconds = t.elapsed().as_secs_f64();
    for (r, (a, b)) in gpu.iter().zip(&cpu).enumerate() {
        assert!((a - b).abs() < 1e-12, "r {r}: gpu {a} cpu {b}");
    }
    println!("[GPU-ACCEPTANCE] {} exact_s2 grid=96^3 r_max=16 cpu_seconds={cpu_seconds:.3} gpu_seconds={gpu_seconds:.3}", ctx.describe());
}

// AI-FUNC-SUMMARY: One small GPU MC evaluation raises the process-wide upload and readback counters (bytes and calls) and the blocked-wait time, and a GPU measure run prints its `[Timing] gpu` transfer line; skips without an adapter.
#[test]
fn transfer_counters_track_uploads_and_readbacks() {
    if try_init_gpu().is_err() {
        eprintln!("SKIP: no GPU adapter");
        return;
    }
    let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
    let mesh = icosphere_mesh(Vec3::new(2.0, 2.0, 2.0), 1.2, 2);
    let before = rustmspt::gpu::gpu_transfer_stats();
    let mut gpu = rustmspt::gpu::GpuS2Pipeline::new(&mesh, bbox).unwrap();
    gpu.calculate_s2_gpu(bbox, 2, 400).unwrap();
    let after = rustmspt::gpu::gpu_transfer_stats();
    assert!(after.upload_bytes >= before.upload_bytes + (mesh.faces.len() as u64) * (36 + 64), "{before:?} -> {after:?}");
    assert!(after.upload_calls > before.upload_calls);
    assert!(after.readback_calls > before.readback_calls);
    assert!(after.readback_bytes > before.readback_bytes);
    assert!(after.wait_nanos > before.wait_nanos);

    let tmp = tempfile::tempdir().unwrap();
    let stl = tmp.path().join("s.stl");
    rustmspt::io::save_stl(&stl, &mesh, "s").unwrap();
    let config = tmp.path().join("m.json");
    std::fs::write(&config, serde_json::to_vec(&serde_json::json!({"measurement": {
        "stl_path": stl, "output_path": tmp.path().join("out.txt"), "bounding_box": [4, 4, 4],
        "r_max": 2, "voxel_pitch": 0.0, "mc_method": "monte_carlo", "mc_samples": 400, "cpu_max": 1,
        "acceleration": {"mode": "gpu", "cpu_fallback": false}
    }})).unwrap()).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rustmspt"))
        .args(["measure", "--config"])
        .arg(&config)
        .env("RUSTMSPT_ACCELERATION", "gpu")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout} {}", String::from_utf8_lossy(&out.stderr));
    let line = stdout.lines().find(|l| l.starts_with("[Timing] gpu ")).expect("a GPU run reports its transfers");
    assert!(line.contains("upload_bytes=") && line.contains("readback_wait_seconds="), "{line}");
    assert!(!line.contains("upload_bytes=0 "), "{line}");
}
