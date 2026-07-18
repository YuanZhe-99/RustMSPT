use crate::config::{parse_box_dimensions, PackingConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    bbox_distance, check_boundary_constraints_mode, generate_periodic_ghosts, merge_meshes,
    mesh_bbox, mesh_collision_exact, mesh_distance_exact, mesh_metrics, mesh_surface_area,
    mesh_volume, move_mesh_to_target_center, orient_components_to_positive_volume,
    particle_volume_in_bbox, rotate_mesh_around_center, scale_mesh_to_equivalent_diameter,
    split_mesh_into_granules, MeshMetrics,
};
use crate::io::{load_stl, save_stl};
use crate::pipeline::pack_targets::{
    load_target_distribution_csv, write_distribution_comparison_csv, BinChoiceKind,
    SphericityState, TargetDistribution,
};
use crate::pipeline::rotation::{parse_rotation_mode, sample_rotation_axis};
use crate::pipeline::{create_progress_bar, Pipeline};
use crate::types::{Mesh, Vec3};
use indicatif::ProgressBar;
use rand::Rng;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::collections::BTreeSet;
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

const TARGET_BIN_PROBES: usize = 4;

pub struct PackPipeline {
    pub config: PackingConfig,
}

#[derive(Debug, Clone)]
struct CandidateProposal {
    mesh: Mesh,
    metrics: Option<MeshMetrics>,
}

// AI-FUNC-SUMMARY:
// Purpose: Validate optional target sphericity configuration before any packing work.
// Inputs: packing config.
// Returns: validated target and optional tolerance.
// Side effects: None.
fn validate_sphericity_target(config: &PackingConfig) -> Result<Option<(f64, Option<f64>)>> {
    let target = config.packing.target_mean_sphericity;
    let tolerance = config.packing.mean_sphericity_tolerance;
    if let Some(target) = target {
        if !target.is_finite() || target <= 0.0 || target > 1.0 {
            return Err(RustMsptError::InvalidConfig(
                "packing.target_mean_sphericity must be finite and in (0, 1]".to_string(),
            ));
        }
        if let Some(tolerance) = tolerance {
            if !tolerance.is_finite() || !(0.0..=1.0).contains(&tolerance) {
                return Err(RustMsptError::InvalidConfig(
                    "packing.mean_sphericity_tolerance must be finite and in [0, 1]".to_string(),
                ));
            }
        }
        Ok(Some((target, tolerance)))
    } else if tolerance.is_some() {
        Err(RustMsptError::InvalidConfig(
            "packing.mean_sphericity_tolerance requires packing.target_mean_sphericity".to_string(),
        ))
    } else {
        Ok(None)
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Validate a candidate mesh against configured geometry filters (min_volume, max_aspect_ratio, max_sharpness_ratio).
// Inputs: mesh reference, packing config, and whether source/final volume should be enforced.
// Returns: true when all enabled filters pass.
// Side effects: None.
fn check_geometry_filters(mesh: &Mesh, config: &PackingConfig, check_min_volume: bool) -> bool {
    if let Some(filters) = &config.packing.filters {
        if let Some(min_vol) = filters.min_volume.filter(|_| check_min_volume) {
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
            let area = mesh_surface_area(mesh);
            if !volume.is_finite()
                || !area.is_finite()
                || !max_sharp.is_finite()
                || volume <= 0.0
                || area <= 0.0
                || max_sharp <= 0.0
            {
                return false;
            }
            let log_sharpness = 3.0 * area.ln() - (36.0 * PI).ln() - 2.0 * volume.ln();
            if !log_sharpness.is_finite() || log_sharpness > max_sharp.ln() {
                return false;
            }
        }
    }

    true
}

impl Pipeline for PackPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Execute particle packing with optional target diameter scaling and soft mean-sphericity steering until target volume fraction or attempt limit.
    // Inputs: PackingConfig with input/output/box/packing settings plus optional diameter CSV and sphericity target.
    // Returns: Ok(()) or error.
    // Side effects: Reads STL/CSV from disk; writes packed STL and optional diameter comparison CSV; prints progress and target statistics to stdout.
    // Notes: Supports lazy directory loading for large datasets. Target-bin failures fall back to placeable bins so volume fraction has priority. Mode 3 adds periodic boundary ghost collision checks. Uses rayon thread pool for parallel collision detection.
    fn run(&self) -> Result<()> {
        let box_bounds = parse_box_dimensions(&self.config.r#box.dimensions)?;
        let box_volume = box_bounds.volume();
        if box_volume <= 0.0 {
            return Err(RustMsptError::InvalidConfig(
                "Packing box volume must be positive".to_string(),
            ));
        }
        let sphericity_target = validate_sphericity_target(&self.config)?;
        let target_distribution = if let Some(csv_path) = self
            .config
            .packing
            .target_diameter_distribution_csv
            .as_deref()
        {
            let distribution = load_target_distribution_csv(Path::new(csv_path))?;
            println!(
                "[Info] Target diameter distribution: {} bins from '{}'",
                distribution.bins.len(),
                csv_path
            );
            Some(distribution)
        } else {
            None
        };
        if let Some((target, tolerance)) = sphericity_target {
            match tolerance {
                Some(tolerance) => {
                    println!("[Info] Target mean sphericity: {target:.6} +/- {tolerance:.6}")
                }
                None => println!("[Info] Target mean sphericity: {target:.6}"),
            }
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
                if check_geometry_filters(&part, &self.config, target_distribution.is_none()) {
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
        let rotation_mode = parse_rotation_mode(
            "packing",
            self.config.packing.rotation_mode.as_deref(),
            self.config.packing.rotation_axis_vector.as_ref(),
        )?;

        let available_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let cpu_max = self.config.packing.cpu_max.unwrap_or(-1);
        let thread_count = if cpu_max == -1 {
            available_cores
        } else {
            (cpu_max.max(1) as usize).min(available_cores)
        };
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .build()
            .map_err(|e| {
                RustMsptError::InvalidConfig(format!("Failed to build thread pool: {e}"))
            })?;
        let effective_pool_threads = thread_pool.install(rayon::current_num_threads);
        println!(
            "[Info] CPU setting: cpu_max={} -> using {} worker threads (available {}).",
            cpu_max, thread_count, available_cores
        );
        println!(
            "[Info] Rayon pool threads (effective): {}",
            effective_pool_threads
        );
        println!(
            "[Info] Rotation mode: {}",
            self.config
                .packing
                .rotation_mode
                .as_deref()
                .unwrap_or("any")
        );

        let mut rng = rand::thread_rng();
        let mut placed: Vec<Mesh> = Vec::new();
        let mut current_volume = 0.0;
        let mut attempts = 0usize;
        let mut distribution_state = target_distribution
            .as_ref()
            .map(TargetDistribution::state)
            .unwrap_or_default();
        let mut sphericity_state = SphericityState::default();

        let progress = create_progress_bar(
            10_000,
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>4}/{len:4} {msg}",
            "##-",
        );
        progress.set_message("VF 0.000000 | placed 0 | attempts 0");

        let update_progress = |bar: &ProgressBar,
                               current_volume: f64,
                               box_volume: f64,
                               target: f64,
                               count: usize,
                               attempts: usize| {
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

        while (current_volume / box_volume) < target && attempts < self.config.packing.max_attempts
        {
            let proposal_draws = if sphericity_target.is_some() { 4 } else { 1 };
            let target_metrics_enabled =
                target_distribution.is_some() || sphericity_target.is_some();
            let mut proposals: Vec<(CandidateProposal, f64)> = Vec::new();

            for _ in 0..proposal_draws {
                let mut proposal_opt: Option<Mesh> = None;
                if is_lazy_mode {
                    let random_file = &lazy_files[rng.gen_range(0..lazy_files.len())];
                    if let Ok(mesh) = load_stl(random_file) {
                        let parts = split_mesh_into_granules(&mesh);
                        let valid_parts: Vec<Mesh> = parts
                            .into_iter()
                            .filter(|p| {
                                check_geometry_filters(
                                    p,
                                    &self.config,
                                    target_distribution.is_none(),
                                )
                            })
                            .collect();
                        if !valid_parts.is_empty() {
                            proposal_opt =
                                Some(valid_parts[rng.gen_range(0..valid_parts.len())].clone());
                        }
                    }
                } else {
                    proposal_opt =
                        Some(preloaded_pool[rng.gen_range(0..preloaded_pool.len())].clone());
                }

                let Some(proposal_mesh) = proposal_opt else {
                    continue;
                };
                let metrics = if target_metrics_enabled {
                    let Some(metrics) = mesh_metrics(&proposal_mesh) else {
                        continue;
                    };
                    Some(metrics)
                } else {
                    None
                };
                let error = if let Some((target, tolerance)) = sphericity_target {
                    match sphericity_state.projected_error(
                        metrics.expect("target metrics were computed"),
                        target,
                        tolerance,
                    ) {
                        Some(error) => error,
                        None => continue,
                    }
                } else {
                    0.0
                };
                proposals.push((
                    CandidateProposal {
                        mesh: proposal_mesh,
                        metrics,
                    },
                    error,
                ));
            }

            if proposals.is_empty() {
                attempts += 1;
                update_progress(
                    &progress,
                    current_volume,
                    box_volume,
                    target,
                    placed.len(),
                    attempts,
                );
                continue;
            }
            proposals.sort_by(|left, right| left.1.total_cmp(&right.1));

            let mut placement_complete = false;
            let mut had_bin_choice = false;
            let mut attempted_bins: BTreeSet<usize> = BTreeSet::new();
            let mut bin_probe_failures = target_distribution
                .as_ref()
                .map(|distribution| vec![0usize; distribution.bins.len()])
                .unwrap_or_default();
            let mut proposal_index = 0usize;

            loop {
                if attempts >= self.config.packing.max_attempts
                    || (target_distribution.is_none() && proposal_index >= proposals.len())
                {
                    break;
                }
                let proposal = &proposals[proposal_index % proposals.len()].0;
                proposal_index += 1;
                let mut candidate = proposal.mesh.clone();
                let mut candidate_metrics = proposal.metrics;
                let mut chosen_bin = None;
                let mut scale_factor = None;

                macro_rules! reject_candidate {
                    () => {{
                        attempts += 1;
                        if let Some((index, _)) = chosen_bin {
                            bin_probe_failures[index] += 1;
                            if bin_probe_failures[index] < TARGET_BIN_PROBES {
                                attempted_bins.remove(&index);
                            }
                        }
                        update_progress(
                            &progress,
                            current_volume,
                            box_volume,
                            target,
                            placed.len(),
                            attempts,
                        );
                        continue;
                    }};
                }

                if let Some(distribution) = &target_distribution {
                    let Some(source_metrics) = candidate_metrics else {
                        reject_candidate!();
                    };
                    let natural_bin =
                        distribution.bin_for_diameter(source_metrics.equivalent_diameter);
                    let Some(choice) = distribution.choose_bin(
                        &distribution_state,
                        natural_bin,
                        &mut attempted_bins,
                    ) else {
                        break;
                    };
                    had_bin_choice = true;
                    distribution.record_attempt(&mut distribution_state, choice.index);
                    chosen_bin = Some((choice.index, choice.kind));

                    if choice.kind != BinChoiceKind::Natural {
                        let target_diameter = distribution.bins[choice.index].midpoint();
                        let Some(factor) = scale_mesh_to_equivalent_diameter(
                            &mut candidate,
                            source_metrics,
                            target_diameter,
                        ) else {
                            reject_candidate!();
                        };
                        let Some(scaled_metrics) = mesh_metrics(&candidate) else {
                            reject_candidate!();
                        };
                        if distribution.bin_for_diameter(scaled_metrics.equivalent_diameter)
                            != Some(choice.index)
                        {
                            reject_candidate!();
                        }
                        candidate_metrics = Some(scaled_metrics);
                        scale_factor = Some(factor);
                    }
                }

                if target_distribution.is_some()
                    && !check_geometry_filters(&candidate, &self.config, true)
                {
                    reject_candidate!();
                }

                if let Some(axis) = sample_rotation_axis(&mut rng, &rotation_mode) {
                    let angle = rng.gen_range(0.0..(2.0 * PI));
                    rotate_mesh_around_center(&mut candidate, axis, angle);
                }

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
                    reject_candidate!();
                }

                let mut collision_set = placed.clone();
                if self.config.packing.mode == 3 {
                    for p in &placed {
                        collision_set.extend(generate_periodic_ghosts(p, box_bounds));
                    }
                }
                let collision_bboxes: Vec<Option<crate::types::BoundingBox>> =
                    collision_set.iter().map(mesh_bbox).collect();
                let candidate_bbox = mesh_bbox(&candidate);

                let candidate_collision = thread_pool.install(|| {
                    collision_set
                        .par_iter()
                        .zip(collision_bboxes.par_iter())
                        .any(|(existing, other_bbox)| {
                            if let (Some(cb), Some(ob)) = (candidate_bbox, *other_bbox) {
                                let bd = bbox_distance(cb, ob);
                                if min_neighbor > 0.0 {
                                    if bd >= min_neighbor {
                                        return false;
                                    }
                                } else if bd > 0.0 {
                                    return false;
                                }
                            }
                            mesh_collision_exact(&candidate, existing)
                        })
                });
                if candidate_collision {
                    reject_candidate!();
                }

                if min_neighbor > 0.0 && !collision_set.is_empty() {
                    let distance = thread_pool.install(|| {
                        collision_set
                            .par_iter()
                            .zip(collision_bboxes.par_iter())
                            .map(|(existing, other_bbox)| {
                                if let (Some(cb), Some(ob)) = (candidate_bbox, *other_bbox) {
                                    let bd = bbox_distance(cb, ob);
                                    if bd >= min_neighbor {
                                        return f64::INFINITY;
                                    }
                                }
                                mesh_distance_exact(&candidate, existing)
                            })
                            .reduce(|| f64::INFINITY, f64::min)
                    });
                    if distance < min_neighbor {
                        reject_candidate!();
                    }
                }

                if self.config.packing.mode == 3 {
                    let candidate_ghosts = generate_periodic_ghosts(&candidate, box_bounds);
                    let ghost_collision = thread_pool.install(|| {
                        candidate_ghosts.par_iter().any(|ghost| {
                            let ghost_bbox = mesh_bbox(ghost);
                            if collision_set
                                .par_iter()
                                .zip(collision_bboxes.par_iter())
                                .any(|(existing, other_bbox)| {
                                    if let (Some(gb), Some(ob)) = (ghost_bbox, *other_bbox) {
                                        let bd = bbox_distance(gb, ob);
                                        if min_neighbor > 0.0 {
                                            if bd >= min_neighbor {
                                                return false;
                                            }
                                        } else if bd > 0.0 {
                                            return false;
                                        }
                                    }
                                    mesh_collision_exact(ghost, existing)
                                })
                            {
                                return true;
                            }

                            if min_neighbor > 0.0 && !collision_set.is_empty() {
                                let d = collision_set
                                    .par_iter()
                                    .zip(collision_bboxes.par_iter())
                                    .map(|(existing, other_bbox)| {
                                        if let (Some(gb), Some(ob)) = (ghost_bbox, *other_bbox) {
                                            let bd = bbox_distance(gb, ob);
                                            if bd >= min_neighbor {
                                                return f64::INFINITY;
                                            }
                                        }
                                        mesh_distance_exact(ghost, existing)
                                    })
                                    .reduce(|| f64::INFINITY, f64::min);
                                return d < min_neighbor;
                            }

                            false
                        })
                    });
                    if ghost_collision {
                        reject_candidate!();
                    }
                }

                let in_box_volume = particle_volume_in_bbox(&candidate, box_bounds);
                if !in_box_volume.is_finite() || in_box_volume <= 0.0 {
                    reject_candidate!();
                }

                if let Some((index, kind)) = chosen_bin {
                    if let Some(distribution) = &target_distribution {
                        distribution.record_success(
                            &mut distribution_state,
                            index,
                            kind,
                            scale_factor,
                        );
                    }
                }
                if sphericity_target.is_some() {
                    if let Some(metrics) = candidate_metrics {
                        sphericity_state.record_success(metrics);
                    }
                }
                current_volume += in_box_volume;
                placed.push(candidate);
                attempts = 0;
                placement_complete = true;
                update_progress(
                    &progress,
                    current_volume,
                    box_volume,
                    target,
                    placed.len(),
                    attempts,
                );
                break;
            }

            if placement_complete {
                continue;
            }
            if target_distribution.is_some()
                && had_bin_choice
                && attempts >= self.config.packing.max_attempts
            {
                break;
            }
            if target_distribution.is_some() && !had_bin_choice {
                attempts += 1;
                update_progress(
                    &progress,
                    current_volume,
                    box_volume,
                    target,
                    placed.len(),
                    attempts,
                );
            }
        }

        progress.finish_with_message("Packing loop completed");

        if placed.is_empty() {
            return Err(RustMsptError::InvalidMesh(
                "No particle could be placed under current constraints".to_string(),
            ));
        }

        let enable_orient = self
            .config
            .packing
            .orient_to_positive_volume
            .unwrap_or(false);
        let final_mesh = merge_meshes(&placed);
        let (final_mesh_oriented, flipped_components, component_count) = if enable_orient {
            orient_components_to_positive_volume(&final_mesh)
        } else {
            (final_mesh.clone(), 0usize, 0usize)
        };
        save_stl(
            Path::new(&self.config.output.path),
            &final_mesh_oriented,
            "packed_result",
        )?;

        let vf = (current_volume / box_volume).clamp(0.0, 1.0);
        println!("[Info] Packing completed.");
        println!("[Info] Final count: {}", placed.len());
        println!("[Info] Final volume fraction: {vf:.6}");
        if vf + 1e-12 < target {
            println!(
                "[Warning] Target volume fraction not reached: target {target:.6}, final {vf:.6}"
            );
        }
        if let Some(distribution) = &target_distribution {
            let summary = distribution.summarize(&distribution_state);
            let comparison_path = write_distribution_comparison_csv(
                Path::new(&self.config.output.path),
                distribution,
                &distribution_state,
            )?;
            println!(
                "[Info] Diameter distribution comparison CSV: {}",
                comparison_path.display()
            );
            println!("[Info] Diameter distribution bins: left,right,target,ideal_count,actual_count,actual,error,attempts");
            for (index, bin) in distribution.bins.iter().enumerate() {
                let actual_count = distribution_state.counts[index];
                let actual = actual_count as f64 / summary.count as f64;
                let ideal_count = bin.frequency * summary.count as f64;
                println!(
                    "[Info] Diameter bin {}: {:.6},{:.6},{:.8},{:.3},{},{:.8},{:+.8},{}",
                    index,
                    bin.left,
                    bin.right,
                    bin.frequency,
                    ideal_count,
                    actual_count,
                    actual,
                    actual - bin.frequency,
                    distribution_state.attempts[index]
                );
            }
            println!(
                "[Info] Diameter distribution error: max_abs {:.8}, total_variation {:.8}, integer_rounding_max {:.8}",
                summary.max_absolute_error,
                summary.total_variation_distance,
                summary.rounding_max_absolute_error
            );
            println!(
                "[Info] Placement target kinds: natural {}, scaled {}, fallback {}",
                distribution_state.natural, distribution_state.scaled, distribution_state.fallback
            );
            if distribution_state.scale_factor_count > 0 {
                println!(
                    "[Info] Scale factors: min {:.6}, mean {:.6}, max {:.6}",
                    distribution_state.scale_factor_min,
                    distribution_state.scale_factor_sum
                        / distribution_state.scale_factor_count as f64,
                    distribution_state.scale_factor_max
                );
            }
            if distribution_state.fallback > 0 {
                println!(
                    "[Warning] Target diameter distribution was relaxed to prioritize volume fraction."
                );
            }
        }
        if let Some((target, tolerance)) = sphericity_target {
            if let Some(mean) = sphericity_state.mean() {
                let error = (mean - target).abs();
                println!("[Info] Final mean sphericity: {mean:.6}");
                println!("[Info] Mean sphericity error: {error:.6}");
                if let Some(tolerance) = tolerance {
                    println!(
                        "[Info] Mean sphericity tolerance met: {}",
                        error <= tolerance + 1e-12
                    );
                    if error > tolerance + 1e-12 {
                        println!(
                            "[Warning] Target mean sphericity not reached; volume fraction was prioritized."
                        );
                    }
                }
            }
        }
        println!("[Info] Orientation fix enabled: {}", enable_orient);
        if enable_orient {
            println!(
                "[Info] Orientation fix: flipped {flipped_components}/{component_count} components to positive signed volume"
            );
        }
        Ok(())
    }
}
