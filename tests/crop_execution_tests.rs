use rustmspt::io::load_tiff_or_folder;
use std::process::{Command, Output};

// AI-FUNC-SUMMARY: Execute a synthetic RAW-u32 crop in a subprocess with isolated acceleration overrides and deterministic foreground geometry.
fn run_crop(
    root: &std::path::Path,
    value: u32,
    acceleration: serde_json::Value,
    override_mode: &str,
    adapter: Option<&str>,
) -> (Output, std::path::PathBuf) {
    let raw = root.join("raw");
    std::fs::create_dir_all(&raw).unwrap();
    for z in 0..6 {
        let mut bytes = Vec::new();
        for y in 0..8 {
            for x in 0..11 {
                let v = if x > 1 && x < 9 && y > 1 && y < 6 && z > 0 && z < 5 {
                    value
                } else {
                    0
                };
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        std::fs::write(raw.join(format!("{z:03}.raw")), bytes).unwrap();
    }
    let path = root.join("output.tiff");
    let config = serde_json::json!({"input":{"type":"raw","path":raw,"raw":{"width":11,"height":8,"bits":32,"signed":false}},"output":{"path":path},"interpolation":"nearest","edge_trim":0,"cpu_max":2,"acceleration":acceleration});
    let file = root.join("config.json");
    std::fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rustmspt"));
    command
        .args(["crop", "--config"])
        .arg(file)
        .env("RUSTMSPT_ACCELERATION", override_mode);
    if let Some(adapter) = adapter {
        command.env("RUSTMSPT_GPU_DEVICE", adapter);
    }
    let output = command.output().unwrap();
    (output, path)
}

// AI-FUNC-SUMMARY: Check CPU/env/budget/type fallback decisions, configured pool execution, no final output on forbidden fallback, and preservation of unsigned labels above i32.
#[test]
fn crop_policy_preserves_labels_and_refuses_forbidden_fallback() {
    let root = tempfile::tempdir().unwrap();
    let cases = [
        (
            "cpu",
            serde_json::json!({"mode":"gpu","cpu_fallback":false}),
            "cpu",
            u32::MAX - 1,
            true,
        ),
        (
            "small",
            serde_json::json!({"mode":"auto","cpu_fallback":false,"gpu_min_voxels":100000}),
            "auto",
            100,
            true,
        ),
        (
            "budget",
            serde_json::json!({"mode":"gpu","gpu_memory_limit_mb":0}),
            "gpu",
            100,
            true,
        ),
        (
            "budget-refuse",
            serde_json::json!({"mode":"gpu","gpu_memory_limit_mb":0,"cpu_fallback":false}),
            "gpu",
            100,
            false,
        ),
        (
            "wide",
            serde_json::json!({"mode":"gpu"}),
            "gpu",
            u32::MAX - 1,
            true,
        ),
        (
            "wide-refuse",
            serde_json::json!({"mode":"gpu","cpu_fallback":false}),
            "gpu",
            u32::MAX - 1,
            false,
        ),
        (
            "bad-mode",
            serde_json::json!({"mode":"auto"}),
            "typo",
            100,
            false,
        ),
    ];
    for (name, config, mode, value, success) in cases {
        let folder = root.path().join(name);
        let (result, path) = run_crop(
            &folder,
            value,
            config,
            mode,
            Some("definitely-no-crop-adapter"),
        );
        assert_eq!(
            result.status.success(),
            success,
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(path.exists(), success);
        if success {
            let stdout = String::from_utf8_lossy(&result.stdout);
            assert!(stdout.contains("Crop execution: backend=cpu"), "{stdout}");
            assert!(stdout.contains("worker_index=Some("), "{stdout}");
            let workers = std::thread::available_parallelism().unwrap().get().min(2);
            assert!(stdout.contains(&format!("workers={workers}")));
            let volume = load_tiff_or_folder(&path).unwrap();
            assert!(volume.data.contains(&i64::from(value)));
            assert!(volume.data.iter().all(|&v| v == 0 || v == i64::from(value)));
        }
    }
}

// AI-FUNC-SUMMARY: Verify real GPU identity/half-tie sampling, preserved i32 labels, and checked capacity/shape/type errors; skip only when adapter initialization is unavailable.
#[cfg(feature = "gpu")]
#[test]
fn crop_gpu_values_rounding_and_errors() {
    use nalgebra::{Matrix3, Vector3};
    use rustmspt::gpu::volume_transform::GpuVolumeTransformPipeline;
    let mut gpu = match GpuVolumeTransformPipeline::new() {
        Ok(gpu) => gpu,
        Err(error) => {
            eprintln!("SKIP no GPU: {error}");
            return;
        }
    };
    let identity = Matrix3::identity();
    let zero = Vector3::zeros();
    let source = [i32::MIN, -1, 16_777_217, i32::MAX];
    assert_eq!(
        gpu.rotate_and_crop(&source, 4, 1, 1, 0, &identity, &zero, &zero, 4, 1, 1, 0)
            .unwrap(),
        source
    );
    let source = [10, 20, 30, 40];
    let half = Vector3::new(0.5, 0.0, 0.0);
    assert_eq!(
        gpu.rotate_and_crop(&source, 4, 1, 1, -9, &identity, &zero, &half, 3, 1, 1, 0)
            .unwrap(),
        [20, 30, 40]
    );
    let negative_half = Vector3::new(-0.5, 0.0, 0.0);
    assert_eq!(
        gpu.rotate_and_crop(
            &source,
            4,
            1,
            1,
            -9,
            &identity,
            &zero,
            &negative_half,
            1,
            1,
            1,
            0
        )
        .unwrap(),
        [-9]
    );
    assert_eq!(
        gpu.rotate_and_crop(&[0, 1], 2, 1, 1, 0, &identity, &zero, &half, 1, 1, 1, 1)
            .unwrap(),
        [1]
    );
    assert_eq!(
        gpu.rotate_and_crop(&[-1, 0], 2, 1, 1, 0, &identity, &zero, &half, 1, 1, 1, 1)
            .unwrap(),
        [-1]
    );
    assert!(gpu
        .rotate_and_crop(
            &[16_777_217],
            1,
            1,
            1,
            0,
            &identity,
            &zero,
            &zero,
            1,
            1,
            1,
            1
        )
        .is_err());
    assert!(gpu
        .rotate_and_crop(&source, 5, 1, 1, 0, &identity, &zero, &zero, 1, 1, 1, 0)
        .is_err());
    assert!(gpu
        .rotate_and_crop(
            &source,
            4,
            1,
            1,
            0,
            &identity,
            &zero,
            &zero,
            u32::MAX,
            2,
            1,
            0
        )
        .is_err());
    assert!(gpu
        .rotate_and_crop(&source, 4, 1, 1, 0, &identity, &zero, &zero, 0, 1, 1, 0)
        .is_err());
    assert!(gpu
        .rotate_and_crop(&source, 4, 1, 1, 0, &identity, &zero, &zero, 1, 1, 1, 2)
        .is_err());
    let long = 65_535 * 64 + 1;
    let tail = gpu
        .rotate_and_crop(&source, 4, 1, 1, -9, &identity, &zero, &zero, 1, 1, long, 0)
        .unwrap();
    assert_eq!(tail[0], 10);
    assert_eq!(tail.last(), Some(&-9));
    assert_eq!(tail.len(), long as usize);
    let root = tempfile::tempdir().unwrap();
    let (cpu, cpu_path) = run_crop(
        &root.path().join("cpu"),
        100,
        serde_json::json!({"mode":"cpu"}),
        "cpu",
        None,
    );
    let (actual, gpu_path) = run_crop(
        &root.path().join("gpu"),
        100,
        serde_json::json!({"mode":"gpu","cpu_fallback":false}),
        "gpu",
        None,
    );
    assert!(cpu.status.success());
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert!(String::from_utf8_lossy(&actual.stdout).contains("Crop execution: backend=gpu"));
    let cpu = load_tiff_or_folder(&cpu_path).unwrap();
    let actual = load_tiff_or_folder(&gpu_path).unwrap();
    assert_eq!(
        (cpu.width, cpu.height, cpu.depth),
        (actual.width, actual.height, actual.depth)
    );
    assert_eq!(cpu.data, actual.data);
}

// AI-FUNC-SUMMARY: Run crop on a larger synthetic RAW-u16 box volume whose GPU working set exceeds a 1-2 MiB budget; returns the process output and TIFF path.
#[cfg(feature = "gpu")]
fn run_tiled_crop(
    root: &std::path::Path,
    acceleration: serde_json::Value,
    interpolation: &str,
) -> (Output, std::path::PathBuf) {
    let (width, height, depth) = (160usize, 120usize, 41usize);
    let raw = root.join("raw");
    std::fs::create_dir_all(&raw).unwrap();
    for z in 0..depth {
        let mut bytes = Vec::with_capacity(width * height * 2);
        for y in 0..height {
            for x in 0..width {
                let inside =
                    (10..=150).contains(&x) && (10..=110).contains(&y) && (5..=35).contains(&z);
                let v: u16 = if inside {
                    1 + ((x * 7 + y * 13 + z * 29) % 4000) as u16
                } else {
                    0
                };
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        std::fs::write(raw.join(format!("{z:03}.raw")), bytes).unwrap();
    }
    let path = root.join("output.tiff");
    let config = serde_json::json!({"input":{"type":"raw","path":raw,"raw":{"width":width,"height":height,"bits":16,"signed":false}},"output":{"path":path},"interpolation":interpolation,"edge_trim":0,"cpu_max":2,"acceleration":acceleration});
    let file = root.join("config.json");
    std::fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rustmspt"))
        .args(["crop", "--config"])
        .arg(file)
        .env_remove("RUSTMSPT_ACCELERATION")
        .output()
        .unwrap();
    (output, path)
}

// AI-FUNC-SUMMARY: Verify budgeted GPU crop executes as several output tiles (gpu and auto modes, both interpolations) and matches the untiled GPU run and, for nearest, the CPU run; skip when no adapter initializes.
#[cfg(feature = "gpu")]
#[test]
fn crop_gpu_budget_executes_tiles() {
    if rustmspt::gpu::volume_transform::GpuVolumeTransformPipeline::new().is_err() {
        eprintln!("SKIP no GPU");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    for interpolation in ["nearest", "trilinear"] {
        let run = |name: &str, acceleration: serde_json::Value| {
            let (output, path) = run_tiled_crop(
                &root.path().join(format!("{interpolation}-{name}")),
                acceleration,
                interpolation,
            );
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            assert!(
                output.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let tiles = stdout
                .split("tiles=")
                .nth(2)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|n| n.parse::<usize>().ok());
            (stdout, tiles, load_tiff_or_folder(&path).unwrap())
        };
        let (cpu_out, _, cpu) = run("cpu", serde_json::json!({"mode":"cpu"}));
        assert!(cpu_out.contains("backend=cpu"));
        let (whole_out, whole_tiles, whole) =
            run("whole", serde_json::json!({"mode":"gpu","cpu_fallback":false}));
        assert!(whole_out.contains("backend=gpu"), "{whole_out}");
        assert_eq!(whole_tiles, Some(1), "{whole_out}");
        for (name, acceleration) in [
            (
                "gpu2",
                serde_json::json!({"mode":"gpu","cpu_fallback":false,"gpu_memory_limit_mb":2}),
            ),
            (
                "auto1",
                serde_json::json!({"mode":"auto","gpu_min_voxels":0,"gpu_memory_limit_mb":1}),
            ),
        ] {
            let (out, tiles, volume) = run(name, acceleration);
            assert!(out.contains("backend=gpu"), "{name}: {out}");
            assert!(tiles.unwrap_or(0) > 1, "{name}: {out}");
            assert_eq!(
                (volume.width, volume.height, volume.depth),
                (whole.width, whole.height, whole.depth)
            );
            assert_eq!(volume.data, whole.data, "{name} {interpolation}");
        }
        if interpolation == "nearest" {
            assert_eq!(cpu.data, whole.data);
        }
    }
}
