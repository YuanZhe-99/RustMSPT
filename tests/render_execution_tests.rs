use rustmspt::geometry::box_mesh;
use rustmspt::io::save_stl;
use rustmspt::types::{BoundingBox, Vec3};
use serde_json::json;
use std::process::Command;

// AI-FUNC-SUMMARY: Exercise render policy in child processes so environment overrides remain isolated.
#[test]
fn render_policy_and_pool_are_enforced() {
    for (name, mode, fallback, budget, threshold, override_mode, success) in [
        ("cpu_override", "gpu", false, None, 0, "cpu", true),
        ("small_auto", "auto", false, None, 100000, "auto", true),
        ("budget_fallback", "gpu", true, Some(0), 0, "gpu", true),
        ("budget_forbidden", "gpu", false, Some(0), 0, "gpu", false),
        ("missing_forbidden", "gpu", false, None, 0, "gpu", false),
        ("invalid_environment", "cpu", true, None, 0, "typo", false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("box.stl");
        let output = dir.path().join("image.png");
        let config = dir.path().join("config.json");
        save_stl(
            &input,
            &box_mesh(BoundingBox {
                min: Vec3::new(0.0, 0.0, 0.0),
                max: Vec3::new(1.0, 1.0, 1.0),
            }),
            "box",
        )
        .unwrap();
        std::fs::write(&config, json!({"render": {
            "stl_path": input, "output_path": output, "focus_point": [0.5,0.5,0.5],
            "view_direction": [0,0,-1], "width": 32, "height": 24, "cpu_max": 2,
            "acceleration": {"mode": mode, "cpu_fallback": fallback, "gpu_memory_limit_mb": budget, "gpu_min_pixels": threshold}
        }}).to_string()).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_rustmspt"))
            .args(["render", "--config"])
            .arg(&config)
            .env("RUSTMSPT_ACCELERATION", override_mode)
            .env("RUSTMSPT_GPU_DEVICE", "definitely-no-render-adapter")
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            success,
            "{name}: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(output.exists(), success, "{name}");
        if success {
            let stdout = String::from_utf8_lossy(&result.stdout);
            assert!(stdout.contains("worker_index=Some("), "{stdout}");
            let workers = std::thread::available_parallelism().unwrap().get().min(2);
            assert!(stdout.contains(&format!("workers={workers},")), "{stdout}");
            let decoded = image::open(&output).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (32, 24));
        }
    }
}
