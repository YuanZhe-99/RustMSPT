use crate::config::{parse_box_dimensions, PackingConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    check_boundary_constraints_mode, generate_periodic_ghosts, merge_meshes, mesh_bbox,
    mesh_collides_with_any, mesh_min_distance_to_set, mesh_surface_area, mesh_volume,
    move_mesh_to_target_center, orient_components_to_positive_volume, particle_volume_in_bbox,
    rotate_mesh_around_center, split_mesh_into_granules,
};
use crate::io::{load_stl, save_stl};
use crate::pipeline::{create_progress_bar, Pipeline};
use crate::types::{Mesh, Vec3};
use indicatif::ProgressBar;
use rand::Rng;
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

pub struct PackPipeline {
    pub config: PackingConfig,
}

fn check_geometry_filters(mesh: &Mesh, config: &PackingConfig) -> bool {
    // Purpose: Validate candidate mesh against configured geometry filters.
    // Inputs: candidate mesh and packing config.
    // Outputs: true when all enabled filters pass.
    if let Some(filters) = &config.packing.filters {
        if let Some(min_vol) = filters.min_volume {
            if mesh_volume(mesh) < min_vol {
                return false;
            }
        }

        if let Some(max_ar) = filters.max_aspect_ratio {
            if let Some(bbox) = mesh_bbox(mesh) {
                let s = bbox.size();
                let min_extent = s.x.min(s.y).min(s.z).max(1e-12);
                let max_extent = s.x.max(s.y).max(s.z);
                let aspect_ratio = max_extent / min_extent;
                if aspect_ratio > max_ar {
                    return false;
                }
            }
        }

        if let Some(max_sharp) = filters.max_sharpness_ratio {
            let volume = mesh_volume(mesh);
            if volume <= 1e-9 {
                return false;
            }
            let area = mesh_surface_area(mesh);
            let sharpness = (area.powi(3)) / (36.0 * PI * volume.powi(2));
            if sharpness > max_sharp {
                return false;
            }
        }
    }

    true
}

impl Pipeline for PackPipeline {
    fn run(&self) -> Result<()> {
        // Purpose: Execute particle packing until target VF or attempt limit is reached.
        // Inputs: packing configuration and STL source.
        // Outputs: packed STL and progress logs.
        let box_bounds = parse_box_dimensions(&self.config.r#box.dimensions)?;
        let box_volume = box_bounds.volume();
        if box_volume <= 0.0 {
            return Err(RustMsptError::InvalidConfig(
                "Packing box volume must be positive".to_string(),
            ));
        }

        let input = Path::new(&self.config.input.path);
        let mut preloaded_pool: Vec<Mesh> = Vec::new();
        let mut lazy_files = Vec::new();
        let mut is_lazy_mode = false;

        if input.is_dir() {
            is_lazy_mode = true;
            for entry in fs::read_dir(input)? {
                let path = entry?.path();
                if path
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase() == "stl")
                    .unwrap_or(false)
                {
                    lazy_files.push(path);
                }
            }
            println!(
                "[Info] Packing input mode: lazy directory loading ({} STL files indexed)",
                lazy_files.len()
            );
        } else {
            let mesh = load_stl(input)?;
            for part in split_mesh_into_granules(&mesh) {
                if check_geometry_filters(&part, &self.config) {
                    preloaded_pool.push(part);
                }
            }
            println!(
                "[Info] Packing input mode: preloaded single STL ({} candidate particles)",
                preloaded_pool.len()
            );
        }

        if !is_lazy_mode && preloaded_pool.is_empty() {
            return Err(RustMsptError::InvalidMesh(
                "No candidate mesh found for packing".to_string(),
            ));
        }
        if is_lazy_mode && lazy_files.is_empty() {
            return Err(RustMsptError::InvalidMesh(
                "No STL files found in input folder".to_string(),
            ));
        }

        let target = self.config.packing.target_volume_fraction.clamp(0.0, 1.0);
        let min_neighbor = self.config.packing.min_neighbor_distance.unwrap_or(0.0);

        let mut rng = rand::thread_rng();
        let mut placed: Vec<Mesh> = Vec::new();
        let mut current_volume = 0.0;
        let mut attempts = 0usize;

        let progress = create_progress_bar(
            10_000,
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>4}/{len:4} {msg}",
            "##-",
        );
        progress.set_message("VF 0.000000 | placed 0 | attempts 0");

        let update_progress = |bar: &ProgressBar, current_volume: f64, box_volume: f64, target: f64, count: usize, attempts: usize| {
            let vf = if box_volume > 0.0 {
                (current_volume / box_volume).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let ratio = if target > 0.0 {
                (vf / target).clamp(0.0, 1.0)
            } else {
                1.0
            };
            bar.set_position((ratio * 10_000.0).round() as u64);
            bar.set_message(format!("VF {vf:.6} | placed {count} | attempts {attempts}"));
        };

        while (current_volume / box_volume) < target && attempts < self.config.packing.max_attempts {
            let mut candidate_opt: Option<Mesh> = None;

            if is_lazy_mode {
                let random_file = &lazy_files[rng.gen_range(0..lazy_files.len())];
                if let Ok(mesh) = load_stl(random_file) {
                    let parts = split_mesh_into_granules(&mesh);
                    let valid_parts: Vec<Mesh> = parts
                        .into_iter()
                        .filter(|p| check_geometry_filters(p, &self.config))
                        .collect();
                    if !valid_parts.is_empty() {
                        candidate_opt = Some(valid_parts[rng.gen_range(0..valid_parts.len())].clone());
                    }
                }
            } else {
                candidate_opt = Some(preloaded_pool[rng.gen_range(0..preloaded_pool.len())].clone());
            }

            let Some(mut candidate) = candidate_opt else {
                attempts += 1;
                update_progress(&progress, current_volume, box_volume, target, placed.len(), attempts);
                continue;
            };

            let axis = Vec3::new(
                rng.gen_range(-1.0..1.0),
                rng.gen_range(-1.0..1.0),
                rng.gen_range(-1.0..1.0),
            );
            let angle = rng.gen_range(0.0..(2.0 * PI));
            rotate_mesh_around_center(&mut candidate, axis, angle);

            let random_pos = Vec3::new(
                rng.gen_range(box_bounds.min.x..box_bounds.max.x),
                rng.gen_range(box_bounds.min.y..box_bounds.max.y),
                rng.gen_range(box_bounds.min.z..box_bounds.max.z),
            );
            move_mesh_to_target_center(&mut candidate, random_pos);

            if !check_boundary_constraints_mode(
                &candidate,
                box_bounds,
                self.config.packing.mode,
                self.config.packing.min_boundary_dist.unwrap_or(0.0),
                self.config.packing.min_cross_boundary_depth.unwrap_or(0.0),
            ) {
                attempts += 1;
                update_progress(&progress, current_volume, box_volume, target, placed.len(), attempts);
                continue;
            }

            let mut collision_set = placed.clone();
            if self.config.packing.mode == 3 {
                for p in &placed {
                    collision_set.extend(generate_periodic_ghosts(p, box_bounds));
                }
            }

            if mesh_collides_with_any(&candidate, &collision_set) {
                attempts += 1;
                update_progress(&progress, current_volume, box_volume, target, placed.len(), attempts);
                continue;
            }

            if min_neighbor > 0.0 && !collision_set.is_empty() {
                let distance = mesh_min_distance_to_set(&candidate, &collision_set);
                if distance < min_neighbor {
                    attempts += 1;
                    update_progress(&progress, current_volume, box_volume, target, placed.len(), attempts);
                    continue;
                }
            }

            if self.config.packing.mode == 3 {
                let candidate_ghosts = generate_periodic_ghosts(&candidate, box_bounds);
                let mut ghost_collision = false;
                for ghost in &candidate_ghosts {
                    if mesh_collides_with_any(ghost, &collision_set) {
                        ghost_collision = true;
                        break;
                    }
                    if min_neighbor > 0.0 && !collision_set.is_empty() {
                        let d = mesh_min_distance_to_set(ghost, &collision_set);
                        if d < min_neighbor {
                            ghost_collision = true;
                            break;
                        }
                    }
                }
                if ghost_collision {
                    attempts += 1;
                    update_progress(&progress, current_volume, box_volume, target, placed.len(), attempts);
                    continue;
                }
            }

            current_volume += particle_volume_in_bbox(&candidate, box_bounds);
            placed.push(candidate);
            attempts = 0;
            update_progress(&progress, current_volume, box_volume, target, placed.len(), attempts);
        }

        progress.finish_with_message("Packing loop completed");

        if placed.is_empty() {
            return Err(RustMsptError::InvalidMesh(
                "No particle could be placed under current constraints".to_string(),
            ));
        }

        let final_mesh = merge_meshes(&placed);
        let (final_mesh_oriented, flipped_components, component_count) =
            orient_components_to_positive_volume(&final_mesh);
        save_stl(
            Path::new(&self.config.output.path),
            &final_mesh_oriented,
            "packed_result",
        )?;

        let vf = (current_volume / box_volume).clamp(0.0, 1.0);
        println!("[Info] Packing completed.");
        println!("[Info] Final count: {}", placed.len());
        println!("[Info] Final volume fraction: {vf:.6}");
        println!(
            "[Info] Orientation fix: flipped {flipped_components}/{component_count} components to positive signed volume"
        );
        Ok(())
    }
}
