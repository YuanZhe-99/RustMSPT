use rustmspt::geometry::{box_mesh, calculate_s2};
use rustmspt::io::{load_stl, save_stl};
use rustmspt::types::{BoundingBox, Vec3};
use std::path::Path;
use std::process::{Command, Output};

// AI-FUNC-SUMMARY: Run the CLI on a fractional cube with a controlled S2/mode/island config and isolated environment; writes fixtures and returns process output.
fn run_case(
    dir: &Path,
    mode: &str,
    method: &str,
    pitch: f64,
    islands: usize,
    fallback: bool,
    override_mode: Option<&str>,
) -> Output {
    let mesh = box_mesh(BoundingBox {
        min: Vec3::new(0.2, 0.2, 0.2),
        max: Vec3::new(1.4, 1.4, 1.4),
    });
    save_stl(&dir.join("input.stl"), &mesh, "fractional cube").unwrap();
    let config = serde_json::json!({
        "input": {"stl_path": dir.join("input.stl")}, "output": {"path": dir.join("output.stl")},
        "box": {"dimensions": [0,0,0,4,4,4]},
        "target": {"type": "reference_stl", "stl_path": dir.join("input.stl"), "stl_bounding_box": [0,0,0,4,4,4]},
        "optimization": {
            "max_iterations": 3, "initial_temperature": 0.1, "cooling_rate": 0.99,
            "r_max": 1, "voxel_pitch": pitch, "mc_method": method, "mc_samples": 200,
            "max_translation": 0.000001, "max_rotation_deg": 0.0, "rotation_mode": "none",
            "prune_enabled": false, "cpu_max": 1, "islands": islands, "migration_interval": 1,
            "acceleration": {"mode": mode, "cpu_fallback": fallback, "gpu_min_voxels": 250000}
        }
    });
    let config_path = dir.join("config.json");
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rustmspt"));
    cmd.args(["optimize", "--config"])
        .arg(config_path)
        .env_remove("RUSTMSPT_ACCELERATION")
        .env("RUSTMSPT_GPU_DEVICE", "nonexistent-optimize-test-adapter");
    if let Some(mode) = override_mode {
        cmd.env("RUSTMSPT_ACCELERATION", mode);
    }
    cmd.output().unwrap()
}

// AI-FUNC-SUMMARY: Verify real CPU/auto single and queued multi-island runs preserve exact S2 and match the saved geometry with no GPU initialization; writes temp output.
#[test]
fn cpu_and_small_auto_exact_match_saved_geometry_with_queued_islands() {
    let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
    for mode in ["cpu", "auto", "gpu"] {
        for islands in [1, 3] {
            let tmp = tempfile::tempdir().unwrap();
            let out = run_case(tmp.path(), mode, "exact", 1.0, islands, true, None);
            assert!(
                out.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let history = std::fs::read_to_string(tmp.path().join("s2_history.txt")).unwrap();
            assert!(
                history.contains("effective=cpu, method=voxel_exact"),
                "{history}"
            );
            assert!(history.contains(&format!(
                "Execution: workers=1, islands={islands}, active_island_limit=1"
            )));
            assert!(!history.contains("nonexistent-optimize-test-adapter"));
            let curve: Vec<f64> = history
                .lines()
                .find_map(|line| line.strip_prefix("Final Best S2: "))
                .unwrap()
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            let mesh = load_stl(&tmp.path().join("output.stl")).unwrap();
            let expected = calculate_s2(&mesh, bbox, 1, 1.0, "exact", 200);
            assert!(
                curve
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| (a - b).abs() < 1e-6),
                "{curve:?}"
            );
        }
    }
}

// AI-FUNC-SUMMARY: Verify the real CLI honors environment mode overrides and rejects invalid values/forbidden GPU fallback before writing results.
#[test]
fn cli_override_and_disabled_fallback_are_enforced() {
    for (mode, override_mode, success, needle) in [
        ("cpu", Some("gpu"), false, "fallback is disabled"),
        ("gpu", Some("cpu"), true, "requested=cpu, effective=cpu"),
        ("gpu", Some("auto"), true, "requested=auto, effective=cpu"),
        ("cpu", Some("typo"), false, "RUSTMSPT_ACCELERATION"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let out = run_case(tmp.path(), mode, "exact", 1.0, 3, false, override_mode);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.success(), success, "{text}");
        assert!(text.contains(needle), "{text}");
        assert_eq!(tmp.path().join("output.stl").exists(), success);
    }
}

// AI-FUNC-SUMMARY: Verify unavailable GPU mesh MC retains its continuous method with allowed fallback, and refuses disabled fallback; uses an invalid adapter filter for deterministic failure.
#[test]
fn unavailable_mesh_gpu_preserves_continuous_method() {
    for fallback in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let out = run_case(tmp.path(), "gpu", "monte_carlo", 0.0, 3, fallback, None);
        assert_eq!(
            out.status.success(),
            fallback,
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        if fallback {
            let history = std::fs::read_to_string(tmp.path().join("s2_history.txt")).unwrap();
            assert!(history.contains("effective=cpu, method=mesh_mc"));
        }
    }
}

// AI-FUNC-SUMMARY: Run a compact continuous-MC optimize fixture with an explicit budget and controlled adapter selection, including target, islands and final evaluation.
fn run_budget_case(dir: &Path, budget: u64, fallback: bool, missing_adapter: bool) -> Output {
    let mesh = box_mesh(BoundingBox { min: Vec3::new(0.2, 0.2, 0.2), max: Vec3::new(1.4, 1.4, 1.4) });
    save_stl(&dir.join("input.stl"), &mesh, "box").unwrap();
    let config = serde_json::json!({
        "input": {"stl_path": dir.join("input.stl")}, "output": {"path": dir.join("output.stl")},
        "box": {"dimensions": [0,0,0,4,4,4]},
        "target": {"type": "reference_stl", "stl_path": dir.join("input.stl"), "stl_bounding_box": [0,0,0,4,4,4]},
        "optimization": {"max_iterations": 2, "initial_temperature": 0.1, "cooling_rate": 0.99,
            "r_max": 1, "voxel_pitch": 0.0, "mc_method": "monte_carlo", "mc_samples": if budget == 1 { 40000 } else { 200 },
            "max_translation": 0.000001, "max_rotation_deg": 0.0, "rotation_mode": "none",
            "prune_enabled": false, "cpu_max": 2, "islands": 3, "migration_interval": 1,
            "acceleration": {"mode": "gpu", "cpu_fallback": fallback, "gpu_memory_limit_mb": budget}}
    });
    let path = dir.join("config.json");
    std::fs::write(&path, config.to_string()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rustmspt"));
    command.args(["optimize", "--config"]).arg(path).env_remove("RUSTMSPT_ACCELERATION");
    if missing_adapter { command.env("RUSTMSPT_GPU_DEVICE", "missing-optimize-budget-adapter"); }
    command.output().unwrap()
}

// AI-FUNC-SUMMARY: Verify a zero GPU cap rejects before probing and allowed fallback preserves continuous mesh-MC semantics.
#[test]
fn optimize_zero_budget_policy() {
    for fallback in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let result = run_budget_case(dir.path(), 0, fallback, true);
        let text = format!("{} {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        assert_eq!(result.status.success(), fallback, "{text}");
        assert!(text.contains("exceeds budget"), "{text}");
        assert!(!text.contains("missing-optimize-budget-adapter"), "{text}");
        if fallback {
            let history = std::fs::read_to_string(dir.path().join("s2_history.txt")).unwrap();
            assert!(history.contains("effective=cpu, method=mesh_mc"));
        } else { assert!(!dir.path().join("output.stl").exists()); }
    }
}

// AI-FUNC-SUMMARY: Exercise actual strict GPU optimization in a one-MiB cap across queued islands and final evaluation; initialization alone may skip on adapterless hosts.
#[cfg(feature = "gpu")]
#[test]
fn optimize_one_mib_gpu_budget_executes() {
    if let Err(error) = rustmspt::gpu::GpuS2Pipeline::new(&rustmspt::types::Mesh::empty(), BoundingBox::from_size(Vec3::new(4.0,4.0,4.0))) {
        eprintln!("SKIP: GPU MC unavailable: {error}"); return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = run_budget_case(dir.path(), 1, false, false);
    let text = format!("{} {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    assert!(result.status.success(), "{text}");
    let history = std::fs::read_to_string(dir.path().join("s2_history.txt")).unwrap();
    assert!(history.contains("effective=gpu"), "{history}");
    assert!(history.contains("method=mesh_mc"));
    assert!(history.contains("Final Best S2:"));
    assert!(dir.path().join("output.stl").exists());
    assert!(text.contains("Optimize GPU mesh_mc f32 certification:"), "{text}");
}
