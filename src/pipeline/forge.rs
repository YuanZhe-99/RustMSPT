use crate::config::ForgingConfig;
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    mesh_bbox, orient_components_to_positive_volume, translate_mesh, volume_fraction_in_bbox,
};
use crate::io::{load_stl_or_merge_folder, save_stl};
use crate::pipeline::Pipeline;
use crate::types::{BoundingBox, Vec3};
use std::fs;
use std::path::Path;

pub struct ForgePipeline {
    pub config: ForgingConfig,
}

impl ForgePipeline {
    // AI-FUNC-SUMMARY: Parse an optional 6-element ROI bounding box [min_x, min_y, min_z, max_x, max_y, max_z] from config; returns Option<BoundingBox>; side effects: None.
    fn parse_roi_bbox(values: &Option<Vec<f64>>) -> Option<BoundingBox> {
        values.as_ref().and_then(|v| {
            if v.len() == 6 {
                Some(BoundingBox {
                    min: Vec3::new(v[0], v[1], v[2]),
                    max: Vec3::new(v[3], v[4], v[5]),
                })
            } else {
                None
            }
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Parse the compression axis string into (axis_index, label) where 0=x, 1=y, 2=z.
    // Inputs: optional axis string, defaults to "z".
    // Returns: Tuple of (usize axis index, &'static str label).
    // Side effects: None.
    // Notes: Returns InvalidConfig for unrecognized axis strings.
    fn parse_compression_axis(axis: Option<&str>) -> Result<(usize, &'static str)> {
        let normalized = axis.unwrap_or("z").trim().to_ascii_lowercase();
        match normalized.as_str() {
            "x" => Ok((0, "x")),
            "y" => Ok((1, "y")),
            "z" => Ok((2, "z")),
            other => Err(RustMsptError::InvalidConfig(format!(
                "forging.compression_axis must be one of: x, y, z (got '{other}')"
            ))),
        }
    }
}

impl ForgePipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Execute forging pipeline: load STL, apply FFD compression along configured axis with bulge and optional void densification, translate output to align ROI, save forged STL and report.
    // Inputs: ForgingConfig with input/output, compression ratio/axis, bulge factor, ROI bbox, mesh_type, and void_densification.
    // Returns: Ok(()) or error.
    // Side effects: Reads STL from disk; writes forged STL and report .txt to disk; prints diagnostics plus load/vf_before/transform/vf_after/orient_shift/write_stl/write_report/total timings, configured-pool workers and peak RSS to stdout.
    fn run_in_pool(&self) -> Result<()> {
        let params = &self.config.forging;
        let input = Path::new(&params.input_stl_path);
        let output = Path::new(
            params
                .output_stl_path
                .as_deref()
                .unwrap_or("data/output/forged_mesh.stl"),
        );
        let report_path = output.with_extension("txt");

        let mut timer = crate::pipeline::timing::StageTimer::start("forge");
        let mesh = load_stl_or_merge_folder(input)?;
        timer.stage("load");
        let lattice_bbox =
            mesh_bbox(&mesh).unwrap_or(BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0)));
        let roi_bbox = Self::parse_roi_bbox(&params.roi_bounding_box);

        let before_roi_vf = roi_bbox
            .map(|roi| volume_fraction_in_bbox(&mesh, roi))
            .unwrap_or_else(|| volume_fraction_in_bbox(&mesh, lattice_bbox));
        timer.stage("vf_before");

        let compression = params.compression_ratio.unwrap_or(0.2);
        let (compression_axis, compression_axis_label) =
            Self::parse_compression_axis(params.compression_axis.as_deref())?;
        let enable_orient = params.orient_to_positive_volume.unwrap_or(false);
        let bulge = params.bulge_factor.unwrap_or(0.5);
        let mesh_type = params.mesh_type.as_deref().unwrap_or("particle");
        let void_densification = params.void_densification.unwrap_or(1.0);

        let transform_started = std::time::Instant::now();
        let (compressed, tracked_roi) = crate::geometry::forging::forge_owned(
            mesh,
            lattice_bbox,
            roi_bbox,
            compression,
            compression_axis,
            bulge,
            mesh_type,
            void_densification,
        );

        println!(
            "[Info] Forge transform seconds: {:.6}",
            transform_started.elapsed().as_secs_f64()
        );
        timer.stage("transform");
        let after_roi_vf = tracked_roi
            .map(|roi| volume_fraction_in_bbox(&compressed, roi))
            .unwrap_or_else(|| volume_fraction_in_bbox(&compressed, lattice_bbox));
        timer.stage("vf_after");

        let (mut compressed_oriented, flipped_components, component_count) = if enable_orient {
            let (mesh_fixed, flipped, total) = orient_components_to_positive_volume(&compressed);
            (mesh_fixed, flipped, total)
        } else {
            (compressed, 0usize, 0usize)
        };

        let mut output_shift = Vec3::new(0.0, 0.0, 0.0);
        if let (Some(roi_in), Some(roi_out)) = (roi_bbox, tracked_roi) {
            output_shift = roi_in.min.sub(roi_out.min);
            if output_shift.x.abs() > f64::EPSILON
                || output_shift.y.abs() > f64::EPSILON
                || output_shift.z.abs() > f64::EPSILON
            {
                translate_mesh(&mut compressed_oriented, output_shift);
            }
        }

        timer.stage("orient_shift");
        save_stl(output, &compressed_oriented, "forged_mesh")?;
        timer.stage("write_stl");

        let input_bbox = lattice_bbox;
        let output_bbox = mesh_bbox(&compressed_oriented).unwrap_or(lattice_bbox);
        let roi_bbox_before = roi_bbox.unwrap_or(lattice_bbox);
        let roi_bbox_after_raw = tracked_roi.unwrap_or(lattice_bbox);
        let roi_bbox_after = BoundingBox {
            min: roi_bbox_after_raw.min.add(output_shift),
            max: roi_bbox_after_raw.max.add(output_shift),
        };

        let mut report = String::new();
        report.push_str("Method: forge\n");
        report.push_str(&format!(
            "BBox before: min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})\n",
            input_bbox.min.x,
            input_bbox.min.y,
            input_bbox.min.z,
            input_bbox.max.x,
            input_bbox.max.y,
            input_bbox.max.z
        ));
        report.push_str(&format!(
            "BBox after:  min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})\n",
            output_bbox.min.x,
            output_bbox.min.y,
            output_bbox.min.z,
            output_bbox.max.x,
            output_bbox.max.y,
            output_bbox.max.z
        ));
        report.push_str(&format!(
            "ROI BBox before: min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})\n",
            roi_bbox_before.min.x,
            roi_bbox_before.min.y,
            roi_bbox_before.min.z,
            roi_bbox_before.max.x,
            roi_bbox_before.max.y,
            roi_bbox_before.max.z
        ));
        report.push_str(&format!(
            "ROI BBox after:  min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})\n",
            roi_bbox_after.min.x,
            roi_bbox_after.min.y,
            roi_bbox_after.min.z,
            roi_bbox_after.max.x,
            roi_bbox_after.max.y,
            roi_bbox_after.max.z
        ));
        report.push_str(&format!("Spatial ROI VF before: {before_roi_vf:.6}\n"));
        report.push_str(&format!("Spatial ROI VF after:  {after_roi_vf:.6}\n"));
        report.push_str(&format!("Compression axis: {compression_axis_label}\n"));
        report.push_str(&format!("Orientation fix enabled: {}\n", enable_orient));
        if enable_orient {
            report.push_str(&format!(
                "Orientation fix: flipped {flipped_components}/{component_count} components to positive signed volume\n"
            ));
        }
        report.push_str(&format!(
            "Output translation: ({:.4},{:.4},{:.4})\n",
            output_shift.x, output_shift.y, output_shift.z
        ));

        if let Some(parent) = report_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&report_path, report)?;
        timer.stage("write_report");

        println!("[Info] Forging completed.");
        println!(
            "[Info] BBox before: min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
            input_bbox.min.x,
            input_bbox.min.y,
            input_bbox.min.z,
            input_bbox.max.x,
            input_bbox.max.y,
            input_bbox.max.z
        );
        println!(
            "[Info] BBox after:  min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
            output_bbox.min.x,
            output_bbox.min.y,
            output_bbox.min.z,
            output_bbox.max.x,
            output_bbox.max.y,
            output_bbox.max.z
        );
        println!(
            "[Info] ROI BBox before: min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
            roi_bbox_before.min.x,
            roi_bbox_before.min.y,
            roi_bbox_before.min.z,
            roi_bbox_before.max.x,
            roi_bbox_before.max.y,
            roi_bbox_before.max.z
        );
        println!(
            "[Info] ROI BBox after:  min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
            roi_bbox_after.min.x,
            roi_bbox_after.min.y,
            roi_bbox_after.min.z,
            roi_bbox_after.max.x,
            roi_bbox_after.max.y,
            roi_bbox_after.max.z
        );
        println!("[Info] Spatial ROI VF before: {before_roi_vf:.6}");
        println!("[Info] Spatial ROI VF after:  {after_roi_vf:.6}");
        println!("[Info] Compression axis: {compression_axis_label}");
        println!("[Info] Orientation fix enabled: {}", enable_orient);
        if enable_orient {
            println!(
                "[Info] Orientation fix: flipped {flipped_components}/{component_count} components to positive signed volume"
            );
        }
        println!(
            "[Info] Output translation: ({:.4},{:.4},{:.4})",
            output_shift.x, output_shift.y, output_shift.z
        );
        println!("[Info] Output STL written: {}", output.display());
        println!("[Info] Output report written: {}", report_path.display());
        timer.total("total");
        timer.report_resources();

        Ok(())
    }
}

impl Pipeline for ForgePipeline {
    // AI-FUNC-SUMMARY: Run the pipeline inside one pool sized by `cpu_max` (absent or -1: all available workers); returns run_in_pool's result; side effects: those of run_in_pool.
    fn run(&self) -> Result<()> {
        crate::pipeline::run_in_cpu_pool("forge", self.config.forging.cpu_max, || self.run_in_pool())
    }
}
