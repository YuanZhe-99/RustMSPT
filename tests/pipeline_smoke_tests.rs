use rustmspt::config::{
    BoxConfig, CropConfig, CropInput, CropOutput, CropRawParams, ForgingConfig, ForgingParams,
    InputPath, InputStl, MeasurementConfig, MeasurementParams, OptimizationConfig,
    OptimizationParams, OutputPath, OutputStl, PackingConfig, PackingFilters, PackingParams,
    ScaleConfig, ScalingParams, SplitFilterConfig, SplitFilterOutput, SplitFilterRules,
    SplitFilterVolume, TargetConfig,
};
use rustmspt::geometry::{box_mesh, mesh_metrics, split_mesh_into_granules};
use rustmspt::io::{load_stl, load_tiff_or_folder, save_stl};
use rustmspt::pipeline::crop::CropPipeline;
use rustmspt::pipeline::forge::ForgePipeline;
use rustmspt::pipeline::measure::MeasurePipeline;
use rustmspt::pipeline::optimize::OptimizePipeline;
use rustmspt::pipeline::pack::PackPipeline;
use rustmspt::pipeline::scale::ScalePipeline;
use rustmspt::pipeline::split_filter::SplitFilterPipeline;
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
                compression_axis: Some("z".to_string()),
                orient_to_positive_volume: Some(false),
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
                cpu_max: Some(-1),
                output_path: output.to_string_lossy().to_string(),
                acceleration: Default::default(),
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
                orient_to_positive_volume: Some(false),
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
                rotation_mode: Some("any".to_string()),
                rotation_axis_vector: Some(vec![0.0, 0.0, 1.0]),
                min_boundary_dist: Some(0.0),
                min_cross_boundary_depth: Some(0.0),
                cpu_max: Some(1),
                orient_to_positive_volume: Some(false),
                target_diameter_distribution_csv: None,
                target_mean_sphericity: None,
                mean_sphericity_tolerance: None,
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
fn packing_pipeline_scales_to_diameter_distribution() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output = tmp.path().join("packed_distribution.stl");
    let csv_path = tmp.path().join("distribution.csv");
    let tiny_mesh = box_mesh(BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(0.0001, 0.0001, 0.0001),
    });
    save_stl(&input, &tiny_mesh, "tiny_sample").expect("tiny sample should be written");
    fs::write(&csv_path, "bin,right,frequency\n1.5,2.5,1.0\n")
        .expect("distribution fixture should be written");

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
                rotation_mode: Some("none".to_string()),
                rotation_axis_vector: Some(vec![0.0, 0.0, 1.0]),
                min_boundary_dist: Some(0.0),
                min_cross_boundary_depth: Some(0.0),
                cpu_max: Some(1),
                orient_to_positive_volume: Some(false),
                target_diameter_distribution_csv: Some(csv_path.to_string_lossy().to_string()),
                target_mean_sphericity: Some(0.8),
                mean_sphericity_tolerance: Some(0.1),
                filters: Some(PackingFilters {
                    min_volume: Some(2.0),
                    max_aspect_ratio: None,
                    max_sharpness_ratio: Some(2.0),
                }),
            },
        },
    };

    pipeline.run().expect("packing pipeline should run");
    let packed = load_stl(&output).expect("packed STL should load");
    let parts = split_mesh_into_granules(&packed);
    assert_eq!(parts.len(), 1);
    let metrics = mesh_metrics(&parts[0]).expect("placed particle should have metrics");
    assert!((metrics.equivalent_diameter - 2.0).abs() < 2e-5);
    let comparison = tmp
        .path()
        .join("packed_distribution_diameter_distribution.csv");
    let comparison_text =
        fs::read_to_string(comparison).expect("diameter comparison CSV should be written");
    assert!(comparison_text.starts_with("bin,right,target_frequency,target_count"));
}

#[test]
fn packing_pipeline_accepts_mean_sphericity_target_without_blocking_vf() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output = tmp.path().join("packed_sphericity.stl");
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
                rotation_mode: Some("none".to_string()),
                rotation_axis_vector: Some(vec![0.0, 0.0, 1.0]),
                min_boundary_dist: Some(0.0),
                min_cross_boundary_depth: Some(0.0),
                cpu_max: Some(1),
                orient_to_positive_volume: Some(false),
                target_diameter_distribution_csv: None,
                target_mean_sphericity: Some(1.0),
                mean_sphericity_tolerance: Some(0.001),
                filters: Some(PackingFilters {
                    min_volume: None,
                    max_aspect_ratio: None,
                    max_sharpness_ratio: None,
                }),
            },
        },
    };

    pipeline
        .run()
        .expect("infeasible sphericity target should not block packing");
    assert!(output.exists());
}

#[test]
fn packing_pipeline_falls_back_from_unplaceable_diameter_bin() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output = tmp.path().join("packed_fallback.stl");
    let csv_path = tmp.path().join("distribution.csv");
    write_sample_mesh(&input);
    fs::write(&csv_path, "bin,right,frequency\n1,2,0.1\n20,21,0.9\n")
        .expect("distribution fixture should be written");

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
                rotation_mode: Some("none".to_string()),
                rotation_axis_vector: Some(vec![0.0, 0.0, 1.0]),
                min_boundary_dist: Some(0.0),
                min_cross_boundary_depth: Some(0.0),
                cpu_max: Some(1),
                orient_to_positive_volume: Some(false),
                target_diameter_distribution_csv: Some(csv_path.to_string_lossy().to_string()),
                target_mean_sphericity: Some(0.8),
                mean_sphericity_tolerance: Some(0.1),
                filters: Some(PackingFilters {
                    min_volume: None,
                    max_aspect_ratio: None,
                    max_sharpness_ratio: None,
                }),
            },
        },
    };

    pipeline
        .run()
        .expect("a smaller configured bin should allow VF completion");
    let packed = load_stl(&output).expect("packed STL should load");
    let parts = split_mesh_into_granules(&packed);
    assert_eq!(parts.len(), 1);
    let metrics = mesh_metrics(&parts[0]).expect("placed particle should have metrics");
    assert!((metrics.equivalent_diameter - 1.5).abs() < 2e-5);
}

#[test]
fn split_filter_pipeline_smoke() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output_dir = tmp.path().join("split_out");
    write_sample_mesh(&input);

    let pipeline = SplitFilterPipeline {
        config: SplitFilterConfig {
            input: InputPath {
                path: input.to_string_lossy().to_string(),
            },
            output: SplitFilterOutput {
                folder: output_dir.to_string_lossy().to_string(),
                prefix: "piece_".to_string(),
                report_path: None,
            },
            filter: Some(SplitFilterRules {
                enabled: Some(true),
                max_aspect_ratio: Some(10.0),
                max_sharpness_ratio: Some(10.0),
                volume: Some(SplitFilterVolume {
                    mode: Some("range".to_string()),
                    min: Some(-1.0),
                    max: Some(-1.0),
                    bins: None,
                    over_factor: None,
                }),
            }),
            seed: None,
        },
    };

    pipeline.run().expect("split_filter pipeline should run");
    let entries = fs::read_dir(&output_dir)
        .expect("split output folder should exist")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("stl"))
                .unwrap_or(false)
        })
        .count();
    assert!(entries > 0);
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
                adaptive_temp_window: Some(10),
                target_acceptance_low: Some(0.20),
                target_acceptance_high: Some(0.45),
                adaptive_heat_factor: Some(1.08),
                adaptive_cool_factor: Some(0.94),
                adaptive_temp_ceiling_factor: Some(5.0),
                r_max: 4,
                voxel_pitch: 1.0,
                mc_method: "monte_carlo".to_string(),
                mc_samples: 2_000,
                max_translation: 0.2,
                max_rotation_deg: 5.0,
                min_neighbor_distance: Some(0.0),
                rotation_mode: Some("any".to_string()),
                rotation_axis_vector: Some(vec![0.0, 0.0, 1.0]),
                mode: Some(1),
                min_boundary_dist: Some(0.0),
                min_cross_boundary_depth: Some(0.0),
                prune_enabled: Some(true),
                prune_tolerance: Some(0.01),
                prune_max_rounds: Some(20),
                prune_eval_samples: Some(500),
                cpu_max: Some(1),
                orient_to_positive_volume: Some(false),
                islands: None,
                migration_interval: None,
                acceleration: Default::default(),
            },
        },
    };

    pipeline.run().expect("optimization pipeline should run");
    assert!(output.exists());
}

#[test]
fn crop_pipeline_smoke() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let raw_dir = tmp.path().join("raw_input");
    fs::create_dir_all(&raw_dir).expect("raw input folder should be created");

    let width = 6usize;
    let height = 5usize;
    let depth = 4usize;

    for z in 0..depth {
        let mut slice = vec![0u8; width * height];
        for y in 1..4 {
            for x in 2..5 {
                let idx = y * width + x;
                slice[idx] = 80u8 + z as u8;
            }
        }
        fs::write(raw_dir.join(format!("{:03}.raw", z)), slice)
            .expect("raw slice write should succeed");
    }

    let output_tiff = tmp.path().join("cropped.tiff");
    let pipeline = CropPipeline {
        config: CropConfig {
            input: CropInput {
                r#type: "raw".to_string(),
                path: raw_dir.to_string_lossy().to_string(),
                slice_start: Some(-1),
                slice_end: Some(-1),
                raw: Some(CropRawParams {
                    width,
                    height,
                    bits: 8,
                    signed: false,
                    byte_order: Some("little".to_string()),
                }),
            },
            output: CropOutput {
                path: output_tiff.to_string_lossy().to_string(),
                folder_prefix: Some("crop".to_string()),
                folder_extension: Some("tiff".to_string()),
            },
            interpolation: Some("trilinear".to_string()),
            edge_trim: Some(-1),
        },
    };

    pipeline.run().expect("crop pipeline should run");
    assert!(output_tiff.exists());

    let out_vol = load_tiff_or_folder(&output_tiff).expect("cropped tiff should be readable");
    assert!(out_vol.width > 0);
    assert!(out_vol.height > 0);
    assert!(out_vol.depth > 0);
    assert!(out_vol.data.iter().any(|v| *v > 0));
}
