use crate::config::ScaleConfig;
use crate::error::{Result, RustMsptError};
use crate::geometry::{mesh_bbox, mesh_volume, orient_components_to_positive_volume, scale_mesh};
use crate::io::{load_stl_or_merge_folder, save_stl};
use crate::pipeline::Pipeline;
use std::path::Path;

pub struct ScalePipeline {
    pub config: ScaleConfig,
}

impl Pipeline for ScalePipeline {
    fn run(&self) -> Result<()> {
        // Purpose: Execute scaling pipeline and save scaled STL.
        // Inputs: scaling config and source mesh path.
        // Outputs: scaled mesh file and summary logs.
        let mut mesh = load_stl_or_merge_folder(Path::new(&self.config.input.stl_path))?;
        let mode = self.config.scaling.r#type.as_str();
        let value = self.config.scaling.value;
        let enable_orient = self.config.scaling.orient_to_positive_volume.unwrap_or(false);

        let original_volume = mesh_volume(&mesh);
        let original_bbox = mesh_bbox(&mesh);

        println!("[Info] Scaling started.");
        println!("[Info] Mode: {mode} | Value: {value:.6}");
        if let Some(bb) = original_bbox {
            println!(
                "[Info] Original bounds: min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
                bb.min.x, bb.min.y, bb.min.z, bb.max.x, bb.max.y, bb.max.z
            );
        }
        println!("[Info] Original volume: {original_volume:.6}");

        let factor = match mode {
            "factor" => value,
            "mm_per_voxel" => {
                if value <= 0.0 {
                    return Err(RustMsptError::InvalidConfig(
                        "mm_per_voxel must be positive".to_string(),
                    ));
                }
                value
            }
            "voxel_per_mm" => {
                if value <= 0.0 {
                    return Err(RustMsptError::InvalidConfig(
                        "voxel_per_mm must be positive".to_string(),
                    ));
                }
                1.0 / value
            }
            _ => {
                return Err(RustMsptError::InvalidConfig(format!(
                    "Unknown scaling type: {mode}"
                )))
            }
        };

        scale_mesh(&mut mesh, factor);

        let (mesh_oriented, flipped_components, component_count) = if enable_orient {
            orient_components_to_positive_volume(&mesh)
        } else {
            (mesh.clone(), 0usize, 0usize)
        };
        let scaled_volume = mesh_volume(&mesh_oriented);
        let scaled_bbox = mesh_bbox(&mesh_oriented);

        save_stl(Path::new(&self.config.output.stl_path), &mesh_oriented, "scaled_mesh")?;
        println!("[Info] Scaling completed with factor {factor:.6}.");
        println!("[Info] Orientation fix enabled: {}", enable_orient);
        if let Some(bb) = scaled_bbox {
            println!(
                "[Info] Scaled bounds: min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
                bb.min.x, bb.min.y, bb.min.z, bb.max.x, bb.max.y, bb.max.z
            );
        }
        println!("[Info] Scaled volume: {scaled_volume:.6}");
        if enable_orient {
            println!(
                "[Info] Orientation fix: flipped {flipped_components}/{component_count} components to positive signed volume"
            );
        }
        Ok(())
    }
}
