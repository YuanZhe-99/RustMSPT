use crate::config::{parse_box_dimensions, PackingConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    clip_mesh_by_bbox, merge_meshes, mesh_bbox, mesh_centroid, mesh_volume,
    move_mesh_to_target_center, split_mesh_into_granules,
    orient_components_to_positive_volume,
};
use crate::io::{load_stl, save_stl};
use crate::pipeline::Pipeline;
use crate::types::{BoundingBox, Mesh, Vec3};
use indicatif::{ProgressBar, ProgressStyle};
use parry3d_f64::math::{Isometry, Point};
use parry3d_f64::query;
use parry3d_f64::shape::TriMesh;
use rand::Rng;
use std::f64::consts::PI;
use std::fs;
use std::io::IsTerminal;
use std::path::Path;

pub struct PackPipeline {
    pub config: PackingConfig,
}

fn vec_norm(v: Vec3) -> f64 {
    // Purpose: Compute Euclidean norm of a 3D vector.
    // Inputs: vector value.
    // Outputs: non-negative length.
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

fn mesh_surface_area(mesh: &Mesh) -> f64 {
    // Purpose: Estimate mesh surface area from triangle faces.
    // Inputs: triangle mesh.
    // Outputs: total surface area.
    let mut area = 0.0;
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        let ab = b.sub(a);
        let ac = c.sub(a);
        area += 0.5 * vec_norm(ab.cross(ac));
    }
    area
}

fn bbox_overlaps(a: BoundingBox, b: BoundingBox) -> bool {
    // Purpose: Test overlap of two axis-aligned bounding boxes.
    // Inputs: two bounding boxes.
    // Outputs: true when overlap exists.
    a.min.x < b.max.x
        && a.max.x > b.min.x
        && a.min.y < b.max.y
        && a.max.y > b.min.y
        && a.min.z < b.max.z
        && a.max.z > b.min.z
}

fn bbox_distance(a: BoundingBox, b: BoundingBox) -> f64 {
    // Purpose: Compute shortest distance between two axis-aligned bounding boxes.
    // Inputs: two bounding boxes.
    // Outputs: non-negative distance.
    let dx = if a.max.x < b.min.x {
        b.min.x - a.max.x
    } else if b.max.x < a.min.x {
        a.min.x - b.max.x
    } else {
        0.0
    };

    let dy = if a.max.y < b.min.y {
        b.min.y - a.max.y
    } else if b.max.y < a.min.y {
        a.min.y - b.max.y
    } else {
        0.0
    };

    let dz = if a.max.z < b.min.z {
        b.min.z - a.max.z
    } else if b.max.z < a.min.z {
        a.min.z - b.max.z
    } else {
        0.0
    };

    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn to_parry_trimesh(mesh: &Mesh) -> Option<TriMesh> {
    // Purpose: Convert internal mesh format to Parry TriMesh.
    // Inputs: triangle mesh.
    // Outputs: Parry mesh when indices are valid.
    if mesh.faces.is_empty() || mesh.vertices.is_empty() {
        return None;
    }

    let vertices: Vec<Point<f64>> = mesh
        .vertices
        .iter()
        .map(|v| Point::new(v.x, v.y, v.z))
        .collect();

    let mut indices: Vec<[u32; 3]> = Vec::with_capacity(mesh.faces.len());
    for f in &mesh.faces {
        if f.a > u32::MAX as usize || f.b > u32::MAX as usize || f.c > u32::MAX as usize {
            return None;
        }
        indices.push([f.a as u32, f.b as u32, f.c as u32]);
    }

    TriMesh::new(vertices, indices).ok()
}

fn mesh_collision_exact(a: &Mesh, b: &Mesh) -> bool {
    // Purpose: Perform exact mesh collision test with broad-phase bbox filtering.
    // Inputs: two meshes.
    // Outputs: true when meshes intersect or conversion fails conservatively.
    let Some(a_bbox) = mesh_bbox(a) else {
        return true;
    };
    let Some(b_bbox) = mesh_bbox(b) else {
        return true;
    };
    if !bbox_overlaps(a_bbox, b_bbox) {
        return false;
    }

    let Some(a_shape) = to_parry_trimesh(a) else {
        return true;
    };
    let Some(b_shape) = to_parry_trimesh(b) else {
        return true;
    };

    query::intersection_test(
        &Isometry::identity(),
        &a_shape,
        &Isometry::identity(),
        &b_shape,
    )
    .unwrap_or(true)
}

fn mesh_distance_exact(a: &Mesh, b: &Mesh) -> f64 {
    // Purpose: Compute exact minimum distance between two meshes.
    // Inputs: two meshes.
    // Outputs: distance (0 when intersecting).
    let Some(a_bbox) = mesh_bbox(a) else {
        return 0.0;
    };
    let Some(b_bbox) = mesh_bbox(b) else {
        return 0.0;
    };

    let bbox_d = bbox_distance(a_bbox, b_bbox);
    if bbox_d > 0.0 {
        if let (Some(a_shape), Some(b_shape)) = (to_parry_trimesh(a), to_parry_trimesh(b)) {
            return query::distance(
                &Isometry::identity(),
                &a_shape,
                &Isometry::identity(),
                &b_shape,
            )
            .unwrap_or(bbox_d);
        }
        return bbox_d;
    }

    if mesh_collision_exact(a, b) {
        0.0
    } else {
        if let (Some(a_shape), Some(b_shape)) = (to_parry_trimesh(a), to_parry_trimesh(b)) {
            return query::distance(
                &Isometry::identity(),
                &a_shape,
                &Isometry::identity(),
                &b_shape,
            )
            .unwrap_or(0.0);
        }
        0.0
    }
}

fn rotate_mesh_around_center(mesh: &mut Mesh, axis: Vec3, angle: f64) {
    // Purpose: Rotate mesh around its centroid with Rodrigues formula.
    // Inputs: mutable mesh, rotation axis, rotation angle in radians.
    // Outputs: mesh vertices updated in place.
    let axis_len = vec_norm(axis);
    if axis_len <= 1e-12 {
        return;
    }

    let k = axis.scale(1.0 / axis_len);
    let center = mesh_centroid(mesh);
    let cos_t = angle.cos();
    let sin_t = angle.sin();

    for v in &mut mesh.vertices {
        let p = v.sub(center);
        let term1 = p.scale(cos_t);
        let term2 = k.cross(p).scale(sin_t);
        let term3 = k.scale(k.dot(p) * (1.0 - cos_t));
        *v = center.add(term1.add(term2).add(term3));
    }
}

fn random_transform(mesh: &mut Mesh, box_bounds: BoundingBox, rng: &mut rand::rngs::ThreadRng) {
    // Purpose: Apply random rotation and random center placement inside box.
    // Inputs: mutable mesh, packing box, RNG.
    // Outputs: transformed mesh in place.
    let axis = Vec3::new(
        rng.gen_range(-1.0..1.0),
        rng.gen_range(-1.0..1.0),
        rng.gen_range(-1.0..1.0),
    );
    let angle = rng.gen_range(0.0..(2.0 * PI));
    rotate_mesh_around_center(mesh, axis, angle);

    let random_pos = Vec3::new(
        rng.gen_range(box_bounds.min.x..box_bounds.max.x),
        rng.gen_range(box_bounds.min.y..box_bounds.max.y),
        rng.gen_range(box_bounds.min.z..box_bounds.max.z),
    );
    move_mesh_to_target_center(mesh, random_pos);
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

fn check_boundary_constraints(mesh: &Mesh, box_bounds: BoundingBox, config: &PackingConfig) -> bool {
    // Purpose: Enforce boundary constraints for strict/loose/periodic packing modes.
    // Inputs: candidate mesh, box bounds, packing config.
    // Outputs: true when boundary rules are satisfied.
    let mode = config.packing.mode;
    let d1 = config.packing.min_boundary_dist.unwrap_or(0.0);
    let d2 = config.packing.min_cross_boundary_depth.unwrap_or(0.0);

    let Some(bounds) = mesh_bbox(mesh) else {
        return false;
    };

    let local_min = bounds.min.sub(box_bounds.min);
    let local_max = bounds.max.sub(box_bounds.min);
    let size = box_bounds.size();

    let is_fully_inside = local_min.x >= 0.0
        && local_min.y >= 0.0
        && local_min.z >= 0.0
        && local_max.x <= size.x
        && local_max.y <= size.y
        && local_max.z <= size.z;

    if is_fully_inside {
        if local_min.x < d1
            || local_min.y < d1
            || local_min.z < d1
            || local_max.x > (size.x - d1)
            || local_max.y > (size.y - d1)
            || local_max.z > (size.z - d1)
        {
            return false;
        }
        return true;
    }

    if mode == 1 {
        return false;
    }

    let min_arr = [local_min.x, local_min.y, local_min.z];
    let max_arr = [local_max.x, local_max.y, local_max.z];
    let size_arr = [size.x, size.y, size.z];

    for i in 0..3 {
        if min_arr[i] < 0.0 {
            if min_arr[i].abs() < d2 || max_arr[i] < d2 {
                return false;
            }
        }
        if max_arr[i] > size_arr[i] {
            if (size_arr[i] - min_arr[i]) < d2 || (max_arr[i] - size_arr[i]) < d2 {
                return false;
            }
        }
        if min_arr[i] >= 0.0 && max_arr[i] <= size_arr[i] {
            if min_arr[i] < d1 || max_arr[i] > (size_arr[i] - d1) {
                return false;
            }
        }
    }

    true
}

fn generate_periodic_ghosts(mesh: &Mesh, box_bounds: BoundingBox) -> Vec<Mesh> {
    // Purpose: Create periodic ghost copies that overlap the primary box.
    // Inputs: original mesh and box bounds.
    // Outputs: translated ghost meshes.
    let mut ghosts = Vec::new();
    let Some(bounds) = mesh_bbox(mesh) else {
        return ghosts;
    };

    let size = box_bounds.size();
    for x in [-1.0, 0.0, 1.0] {
        for y in [-1.0, 0.0, 1.0] {
            for z in [-1.0, 0.0, 1.0] {
                if x == 0.0 && y == 0.0 && z == 0.0 {
                    continue;
                }

                let shift = Vec3::new(x * size.x, y * size.y, z * size.z);
                let shifted_bounds = BoundingBox {
                    min: bounds.min.add(shift),
                    max: bounds.max.add(shift),
                };
                if bbox_overlaps(shifted_bounds, box_bounds) {
                    let mut ghost = mesh.clone();
                    for v in &mut ghost.vertices {
                        *v = v.add(shift);
                    }
                    ghosts.push(ghost);
                }
            }
        }
    }

    ghosts
}

fn in_collision(candidate: &Mesh, others: &[Mesh]) -> bool {
    // Purpose: Check candidate collision against a set of meshes.
    // Inputs: candidate mesh and existing meshes.
    // Outputs: true when any collision is found.
    others.iter().any(|m| mesh_collision_exact(candidate, m))
}

fn min_distance_to_set(candidate: &Mesh, others: &[Mesh]) -> f64 {
    // Purpose: Compute minimum distance from candidate to a mesh set.
    // Inputs: candidate mesh and existing meshes.
    // Outputs: smallest distance value.
    let mut best = f64::INFINITY;
    for m in others {
        best = best.min(mesh_distance_exact(candidate, m));
    }
    if best.is_infinite() {
        0.0
    } else {
        best
    }
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

        let progress = ProgressBar::new(10_000);
        if !std::io::stderr().is_terminal() {
            progress.set_draw_target(indicatif::ProgressDrawTarget::hidden());
        }
        progress.set_style(
            ProgressStyle::with_template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>4}/{len:4} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("##-"),
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

            random_transform(&mut candidate, box_bounds, &mut rng);

            if !check_boundary_constraints(&candidate, box_bounds, &self.config) {
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

            if in_collision(&candidate, &collision_set) {
                attempts += 1;
                update_progress(&progress, current_volume, box_volume, target, placed.len(), attempts);
                continue;
            }

            if min_neighbor > 0.0 && !collision_set.is_empty() {
                let distance = min_distance_to_set(&candidate, &collision_set);
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
                    if in_collision(ghost, &collision_set) {
                        ghost_collision = true;
                        break;
                    }
                    if min_neighbor > 0.0 && !collision_set.is_empty() {
                        let d = min_distance_to_set(ghost, &collision_set);
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

            let internal = clip_mesh_by_bbox(&candidate, box_bounds);
            current_volume += mesh_volume(&internal);
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
