use rustmspt::geometry::{box_mesh, calculate_s2};
use rustmspt::io::{load_stl, save_stl};
use rustmspt::types::{BoundingBox, Vec3};
use std::process::Command;

// AI-FUNC-SUMMARY: Exercise CLI environment overrides, exact normalization and strict budget fallback under a real requested worker pool; compare output with the CPU exact reference.
#[test]
fn measurement_execution_contract() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.stl");
    let output = dir.path().join("result.txt");
    let config = dir.path().join("config.json");
    let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
    save_stl(
        &input,
        &box_mesh(BoundingBox {
            min: Vec3::new(0.2, 0.2, 0.2),
            max: Vec3::new(1.4, 1.4, 1.4),
        }),
        "cube",
    )
    .unwrap();
    let mesh = load_stl(&input).unwrap();
    let expected = calculate_s2(&mesh, bbox, 2, 1.0, "exact", 200);
    for (mode, override_mode, budget, fallback, success) in [
        ("gpu", "cpu", None, false, true),
        ("gpu", "gpu", Some(0), true, true),
        ("gpu", "gpu", Some(0), false, false),
        ("cpu", "invalid", None, true, false),
    ] {
        let value = serde_json::json!({"measurement": {
            "stl_path": input, "output_path": output, "bounding_box": [4,4,4], "r_max":2,
            "voxel_pitch":0.0, "mc_method":"exact", "mc_samples":200, "cpu_max":2,
            "acceleration":{"mode":mode,"cpu_fallback":fallback,"gpu_memory_limit_mb":budget}
        }});
        std::fs::write(&config, value.to_string()).unwrap();
        if output.exists() {
            std::fs::remove_file(&output).unwrap();
        }
        let result = Command::new(env!("CARGO_BIN_EXE_rustmspt"))
            .args(["measure", "--config"])
            .arg(&config)
            .env("RUSTMSPT_ACCELERATION", override_mode)
            .env("RUSTMSPT_GPU_DEVICE", "missing-adapter-for-measure-test")
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if success {
            let text = std::fs::read_to_string(&output).unwrap();
            assert!(text.contains("exact=cpu"));
            for (r, value) in expected.iter().enumerate() {
                assert!(text.contains(&format!("{r}: {value:.6}")));
            }
            let stdout = String::from_utf8_lossy(&result.stdout);
            let workers = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
                .min(2);
            assert!(
                stdout.contains(&format!("workers={workers}"))
                    && stdout.contains("worker_index=Some")
            );
        } else {
            assert!(!output.exists());
        }
    }
}

// AI-FUNC-SUMMARY: Compare normalized nonpositive-pitch GPU exact output with the same CPU definition and reject invalid grid input explicitly.
#[cfg(feature = "gpu")]
#[test]
fn gpu_exact_normalizes_pitch_and_propagates_errors() {
    if let Err(error) = rustmspt::gpu::try_init_gpu() {
        eprintln!("SKIP: no GPU adapter: {error}");
        return;
    }

    let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
    let mesh = box_mesh(BoundingBox {
        min: Vec3::new(0.2, 0.2, 0.2),
        max: Vec3::new(1.4, 1.4, 1.4),
    });
    let expected = calculate_s2(&mesh, bbox, 2, 1.0, "exact", 200);
    for pitch in [0.0, -1.0, 1.0] {
        let result =
            rustmspt::geometry::s2::try_calculate_s2_gpu_exact(&mesh, bbox, 2, pitch).unwrap();
        for (a, b) in result.iter().zip(&expected) {
            assert!((a - b).abs() < 1e-12, "{result:?} vs {expected:?}");
        }
    }
    assert!(rustmspt::geometry::s2::try_calculate_s2_gpu_exact(&mesh, bbox, 2, f64::NAN).is_err());
    assert!(rustmspt::geometry::s2::try_calculate_s2_gpu_exact(&mesh, bbox, 2, 1e-100).is_err());
}

// AI-FUNC-SUMMARY: Verify a small strict GPU MC job fits its actual sub-MiB work set after removing the old fixed output reservation.
#[cfg(feature = "gpu")]
#[test]
fn small_gpu_mc_fits_one_mib_budget() {
    if let Err(error) = rustmspt::gpu::try_init_gpu() {
        eprintln!("SKIP: no GPU adapter: {error}");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.stl");
    let output = dir.path().join("result.txt");
    let config = dir.path().join("config.json");
    save_stl(&input, &box_mesh(BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0))), "cube").unwrap();
    std::fs::write(&config, serde_json::json!({"measurement": {
        "stl_path": input, "output_path": output, "bounding_box": [2,2,2],
        "r_max": 2, "voxel_pitch": 0.0, "mc_method": "monte_carlo", "mc_samples": 200,
        "acceleration": {"mode": "gpu", "cpu_fallback": false, "gpu_memory_limit_mb": 1}
    }}).to_string()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rustmspt")).args(["measure", "--config"]).arg(config)
        .env("RUSTMSPT_ACCELERATION", "gpu").output().unwrap();
    assert!(result.status.success(), "{} {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    let output = std::fs::read_to_string(output).unwrap();
    assert!(output.contains("monte_carlo=gpu"), "{output}");
}


// AI-FUNC-SUMMARY: Exercise strict one-MiB exact execution with enough supported offsets to cross its planned batch size; verify analytic solid-grid results and reported GPU execution.
#[cfg(feature = "gpu")]
#[test]
fn gpu_exact_shrinks_batches_to_one_mib() {
    if let Err(error) = rustmspt::gpu::try_init_gpu() {
        eprintln!("SKIP: no GPU adapter: {error}");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input=dir.path().join("solid.stl");
    let output=dir.path().join("result.txt");
    let config=dir.path().join("config.json");
    save_stl(&input,&box_mesh(BoundingBox::from_size(Vec3::new(32.0,32.0,32.0))),"solid").unwrap();
    std::fs::write(&config,serde_json::json!({"measurement": {
        "stl_path":input,"output_path":output,"bounding_box":[32,32,32],"r_max":16,
        "voxel_pitch":1.0,"mc_method":"exact","cpu_max":2,
        "acceleration":{"mode":"gpu","cpu_fallback":false,"gpu_memory_limit_mb":1}
    }}).to_string()).unwrap();
    let result=Command::new(env!("CARGO_BIN_EXE_rustmspt")).args(["measure","--config"]).arg(&config).env("RUSTMSPT_ACCELERATION","gpu").output().unwrap();
    let stdout=String::from_utf8_lossy(&result.stdout);
    assert!(result.status.success(),"{stdout} {}",String::from_utf8_lossy(&result.stderr));
    assert!(stdout.contains("batch_partials=11456"),"{stdout}");
    let text=std::fs::read_to_string(output).unwrap();
    assert!(text.contains("exact=gpu"),"{text}");
    for r in 0..=16 { assert!(text.contains(&format!("{r}: 1.000000")),"{text}"); }
    let generated: usize = (1..=16).map(|r| rustmspt::geometry::s2::shell_offsets_for_distance(r as f64,0.5).len()).sum();
    assert!(generated>11456);
}
