use rustmspt::config::{
    BoxConfig, ForgingConfig, ForgingParams, InputPath, InputStl,
    MeasurementConfig, MeasurementParams, OptimizationConfig, OptimizationParams, OutputPath,
    OutputStl, PackingConfig, PackingFilters, PackingParams, ScaleConfig, ScalingParams,
    TargetConfig,
};
use rustmspt::geometry::box_mesh;
use rustmspt::io::save_stl;
use rustmspt::pipeline::forge::ForgePipeline;
use rustmspt::pipeline::measure::MeasurePipeline;
use rustmspt::pipeline::optimize::OptimizePipeline;
use rustmspt::pipeline::pack::PackPipeline;
use rustmspt::pipeline::scale::ScalePipeline;
use rustmspt::pipeline::Pipeline;
use rustmspt::types::{BoundingBox, Vec3};
use std::fs;

fn write_sample_mesh(path: &std::path::Path) {
    let bbox = BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    };
    let mesh = box_mesh(bbox);
    save_stl(path, &mesh, "sample").expect("sample mesh save should succeed");
}

#[test]
fn forging_pipeline_smoke() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output = tmp.path().join("forged.stl");
    write_sample_mesh(&input);

    let pipeline = ForgePipeline {
        config: ForgingConfig {
            forging: ForgingParams {
                input_stl_path: input.to_string_lossy().to_string(),
                output_stl_path: Some(output.to_string_lossy().to_string()),
                compression_ratio: Some(0.2),
                bulge_factor: Some(0.5),
                roi_bounding_box: None,
                mesh_type: Some("particle".to_string()),
                void_densification: Some(1.0),
            },
        },
    };

    pipeline.run().expect("forging pipeline should run");
    assert!(output.exists());
}

#[test]
fn measurement_pipeline_smoke() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output = tmp.path().join("measurement.txt");
    write_sample_mesh(&input);

    let pipeline = MeasurePipeline {
        config: MeasurementConfig {
            measurement: MeasurementParams {
                stl_path: input.to_string_lossy().to_string(),
                bounding_box: Some(vec![0.0, 0.0, 0.0, 2.0, 2.0, 2.0]),
                stl_bounding_box: None,
                r_max: 4,
                voxel_pitch: 1.0,
                mc_method: "monte_carlo".to_string(),
                mc_samples: Some(2_000),
                output_path: output.to_string_lossy().to_string(),
            },
        },
    };

    pipeline.run().expect("measurement pipeline should run");
    let text = fs::read_to_string(&output).expect("measurement output should exist");
    assert!(text.contains("Volume Fraction"));
    assert!(text.contains("S2 Values"));
}

#[test]
fn scale_pipeline_smoke() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output = tmp.path().join("scaled.stl");
    write_sample_mesh(&input);

    let pipeline = ScalePipeline {
        config: ScaleConfig {
            input: InputStl {
                stl_path: input.to_string_lossy().to_string(),
            },
            output: OutputStl {
                stl_path: output.to_string_lossy().to_string(),
            },
            scaling: ScalingParams {
                r#type: "factor".to_string(),
                value: 2.0,
            },
        },
    };

    pipeline.run().expect("scale pipeline should run");
    assert!(output.exists());
}

#[test]
fn packing_pipeline_smoke() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output = tmp.path().join("packed.stl");
    write_sample_mesh(&input);

    let pipeline = PackPipeline {
        config: PackingConfig {
            input: InputPath {
                path: input.to_string_lossy().to_string(),
            },
            output: OutputPath {
                path: output.to_string_lossy().to_string(),
            },
            r#box: BoxConfig {
                dimensions: vec![0.0, 0.0, 0.0, 10.0, 10.0, 10.0],
            },
            packing: PackingParams {
                target_volume_fraction: 0.0001,
                mode: 1,
                max_attempts: 100,
                min_neighbor_distance: Some(0.0),
                min_boundary_dist: Some(0.0),
                min_cross_boundary_depth: Some(0.0),
                filters: Some(PackingFilters {
                    min_volume: None,
                    max_aspect_ratio: None,
                    max_sharpness_ratio: None,
                }),
            },
        },
    };

    pipeline.run().expect("packing pipeline should run");
    assert!(output.exists());
}

#[test]
fn optimization_pipeline_smoke() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output = tmp.path().join("optimized.stl");
    write_sample_mesh(&input);

    let pipeline = OptimizePipeline {
        config: OptimizationConfig {
            input: InputStl {
                stl_path: input.to_string_lossy().to_string(),
            },
            output: OutputPath {
                path: output.to_string_lossy().to_string(),
            },
            target: TargetConfig {
                r#type: "manual_array".to_string(),
                s2_array: Some(vec![0.1, 0.08, 0.05, 0.03, 0.02]),
                stl_path: None,
                stl_bounding_box: None,
            },
            r#box: BoxConfig {
                dimensions: vec![0.0, 0.0, 0.0, 10.0, 10.0, 10.0],
            },
            optimization: OptimizationParams {
                max_iterations: 10,
                initial_temperature: 0.1,
                cooling_rate: 0.95,
                r_max: 4,
                voxel_pitch: 1.0,
                mc_method: "monte_carlo".to_string(),
                mc_samples: 2_000,
                max_translation: 0.2,
                max_rotation_deg: 5.0,
                min_neighbor_distance: Some(0.0),
                mode: Some(1),
                min_boundary_dist: Some(0.0),
                min_cross_boundary_depth: Some(0.0),
                prune_enabled: Some(true),
                prune_tolerance: Some(0.01),
                prune_max_rounds: Some(20),
                prune_eval_samples: Some(500),
                cpu_max: Some(1),
            },
        },
    };

    pipeline.run().expect("optimization pipeline should run");
    assert!(output.exists());
}
