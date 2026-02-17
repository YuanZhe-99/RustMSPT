use crate::config::ForgingConfig;
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    clip_mesh_by_bbox, mesh_bbox, mesh_volume, merge_meshes, orient_components_to_positive_volume,
    simulate_forging_ffd_with_tracking, translate_mesh,
};
use crate::io::{load_folder_stls, load_stl, save_stl};
use crate::pipeline::Pipeline;
use crate::types::{BoundingBox, Vec3};
use std::path::Path;

pub struct ForgingPipeline {
    pub config: ForgingConfig,
}

impl ForgingPipeline {
    /// Load one STL file, or merge all STL files under a directory.
    /// Input: path from config. Output: one mesh used by forging.
    fn load_input_mesh(&self, input: &Path) -> Result<crate::types::Mesh> {
        if input.is_dir() {
            let meshes = load_folder_stls(input)?;
            if meshes.is_empty() {
                return Err(RustMsptError::InvalidConfig(format!(
                    "No STL files found in directory: {}",
                    input.display()
                )));
            }
            Ok(merge_meshes(
                &meshes
                    .into_iter()
                    .map(|(_, m)| m)
                    .collect::<Vec<_>>(),
            ))
        } else {
            load_stl(input)
        }
    }

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

    /// Compute volume fraction in a target box.
    /// Input: mesh and bbox. Output: VF in [0, 1].
    fn volume_fraction_in_box(mesh: &crate::types::Mesh, bbox: BoundingBox) -> f64 {
        let denom = bbox.volume();
        if denom <= f64::EPSILON {
            return 0.0;
        }

        if let Some(mb) = mesh_bbox(mesh) {
            let inside = mb.min.x >= bbox.min.x
                && mb.min.y >= bbox.min.y
                && mb.min.z >= bbox.min.z
                && mb.max.x <= bbox.max.x
                && mb.max.y <= bbox.max.y
                && mb.max.z <= bbox.max.z;
            if inside {
                return (mesh_volume(mesh) / denom).clamp(0.0, 1.0);
            }

            let disjoint = mb.max.x < bbox.min.x
                || mb.min.x > bbox.max.x
                || mb.max.y < bbox.min.y
                || mb.min.y > bbox.max.y
                || mb.max.z < bbox.min.z
                || mb.min.z > bbox.max.z;
            if disjoint {
                return 0.0;
            }
        }

        let clipped = clip_mesh_by_bbox(mesh, bbox);
        (mesh_volume(&clipped) / denom).clamp(0.0, 1.0)
    }
}

impl Pipeline for ForgingPipeline {
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

        let mesh = self.load_input_mesh(input)?;
        let lattice_bbox = mesh_bbox(&mesh).unwrap_or(BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0)));
        let roi_bbox = Self::parse_roi_bbox(&params.roi_bounding_box);

        let before = mesh_volume(&mesh);
        let before_lattice_vf = Self::volume_fraction_in_box(&mesh, lattice_bbox);
        let before_roi_vf = roi_bbox
            .map(|roi| Self::volume_fraction_in_box(&mesh, roi))
            .unwrap_or(before_lattice_vf);

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

        let after = mesh_volume(&compressed);
        let after_lattice_vf = Self::volume_fraction_in_box(&compressed, lattice_bbox);
        let after_roi_vf = tracked_roi
            .map(|roi| Self::volume_fraction_in_box(&compressed, roi))
            .unwrap_or(after_lattice_vf);

        let before_vf = if let Some(roi) = roi_bbox {
            (before / roi.volume()).clamp(0.0, 1.0)
        } else {
            (before / lattice_bbox.volume()).clamp(0.0, 1.0)
        };
        let after_vf = if let Some(roi) = tracked_roi {
            (after / roi.volume()).clamp(0.0, 1.0)
        } else {
            (after / lattice_bbox.volume()).clamp(0.0, 1.0)
        };

        let (mut compressed_oriented, flipped_components, component_count) =
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

        println!("[Info] Forging completed.");
        println!("[Info] Mode: {mesh_type}");
        println!("[Info] Compression ratio: {compression:.4}");
        println!("[Info] Bulge factor: {bulge:.4}");
        println!(
            "[Info] Lattice bounds: min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
            lattice_bbox.min.x,
            lattice_bbox.min.y,
            lattice_bbox.min.z,
            lattice_bbox.max.x,
            lattice_bbox.max.y,
            lattice_bbox.max.z
        );
        println!("[Info] VF before: {before_vf:.6}");
        println!("[Info] VF after:  {after_vf:.6}");
        println!("[Info] Lattice VF before: {before_lattice_vf:.6}");
        println!("[Info] Lattice VF after:  {after_lattice_vf:.6}");
        println!("[Info] Spatial ROI VF before: {before_roi_vf:.6}");
        println!("[Info] Spatial ROI VF after:  {after_roi_vf:.6}");
        if before_vf > 0.0 {
            println!("[Info] Densification factor: {:.3}", after_vf / before_vf);
        }
        println!("[Info] Volume before: {before:.6}");
        println!("[Info] Volume after:  {after:.6}");
        println!(
            "[Info] Orientation fix: flipped {flipped_components}/{component_count} components to positive signed volume"
        );
        println!(
            "[Info] Output translation: ({:.4},{:.4},{:.4})",
            output_shift.x, output_shift.y, output_shift.z
        );

        if let (Some(roi_in), Some(roi_out)) = (roi_bbox, tracked_roi) {
            let in_size = roi_in.size();
            let out_size = roi_out.size();
            let ex = if in_size.x.abs() > f64::EPSILON {
                out_size.x / in_size.x - 1.0
            } else {
                0.0
            };
            let ey = if in_size.y.abs() > f64::EPSILON {
                out_size.y / in_size.y - 1.0
            } else {
                0.0
            };
            let ez = if in_size.z.abs() > f64::EPSILON {
                out_size.z / in_size.z - 1.0
            } else {
                0.0
            };
            println!(
                "[Info] ROI dimensions in/out (x,y,z): ({:.4},{:.4},{:.4}) -> ({:.4},{:.4},{:.4})",
                in_size.x, in_size.y, in_size.z, out_size.x, out_size.y, out_size.z
            );
            println!(
                "[Info] ROI position in:  min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
                roi_in.min.x, roi_in.min.y, roi_in.min.z, roi_in.max.x, roi_in.max.y, roi_in.max.z
            );
            println!(
                "[Info] ROI position out: min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
                roi_out.min.x, roi_out.min.y, roi_out.min.z, roi_out.max.x, roi_out.max.y, roi_out.max.z
            );
            println!("[Info] Principal strains (ex, ey, ez): ({ex:.5}, {ey:.5}, {ez:.5})");
        }

        Ok(())
    }
}
