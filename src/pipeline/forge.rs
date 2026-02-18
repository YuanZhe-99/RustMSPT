use crate::config::ForgingConfig;
use crate::error::Result;
use crate::geometry::{
    mesh_bbox, mesh_volume, orient_components_to_positive_volume,
    simulate_forging_ffd_with_tracking, translate_mesh, volume_fraction_in_bbox,
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
    /// Parse ROI [min_x, min_y, min_z, max_x, max_y, max_z] if provided.
    /// Input: optional f64 vector. Output: optional bounding box.
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

}

impl Pipeline for ForgePipeline {
    fn run(&self) -> Result<()> {
        // Purpose: Execute forging pipeline and write forged STL output.
        // Inputs: forging config and STL input path.
        // Outputs: forged mesh file and diagnostic logs.
        let params = &self.config.forging;
        let input = Path::new(&params.input_stl_path);
        let output = Path::new(
            params
                .output_stl_path
                .as_deref()
                .unwrap_or("data/output/forged_mesh.stl"),
        );
        let report_path = output.with_extension("txt");

        let mesh = load_stl_or_merge_folder(input)?;
        let lattice_bbox = mesh_bbox(&mesh).unwrap_or(BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0)));
        let roi_bbox = Self::parse_roi_bbox(&params.roi_bounding_box);

        let _before = mesh_volume(&mesh);
        let before_roi_vf = roi_bbox
            .map(|roi| volume_fraction_in_bbox(&mesh, roi))
            .unwrap_or_else(|| volume_fraction_in_bbox(&mesh, lattice_bbox));

        let compression = params.compression_ratio.unwrap_or(0.2);
        let bulge = params.bulge_factor.unwrap_or(0.5);
        let mesh_type = params.mesh_type.as_deref().unwrap_or("particle");
        let void_densification = params.void_densification.unwrap_or(1.0);

        let (compressed, tracked_roi) = simulate_forging_ffd_with_tracking(
            &mesh,
            lattice_bbox,
            roi_bbox,
            compression,
            bulge,
            mesh_type,
            void_densification,
        );

        let _after = mesh_volume(&compressed);
        let after_roi_vf = tracked_roi
            .map(|roi| volume_fraction_in_bbox(&compressed, roi))
            .unwrap_or_else(|| volume_fraction_in_bbox(&compressed, lattice_bbox));

        let (mut compressed_oriented, _flipped_components, _component_count) =
            orient_components_to_positive_volume(&compressed);

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

        save_stl(output, &compressed_oriented, "forged_mesh")?;

        let input_bbox = mesh_bbox(&mesh).unwrap_or(lattice_bbox);
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
        report.push_str(&format!(
            "Output translation: ({:.4},{:.4},{:.4})\n",
            output_shift.x, output_shift.y, output_shift.z
        ));

        if let Some(parent) = report_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&report_path, report)?;

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
        println!(
            "[Info] Output translation: ({:.4},{:.4},{:.4})",
            output_shift.x, output_shift.y, output_shift.z
        );
        println!("[Info] Output STL written: {}", output.display());
        println!("[Info] Output report written: {}", report_path.display());

        Ok(())
    }
}
