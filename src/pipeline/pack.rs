use crate::config::{parse_box_dimensions, PackingConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    bbox_distance, check_boundary_constraints_mode, generate_periodic_ghosts, merge_meshes,
    mesh_bbox, mesh_collision_exact_prepared, mesh_distance_exact_prepared, mesh_metrics,
    mesh_surface_area, mesh_volume, move_mesh_to_target_center,
    orient_components_to_positive_volume, particle_volume_in_bbox, rotate_mesh_around_center,
    scale_mesh_to_equivalent_diameter, split_mesh_into_granules, to_parry_trimesh, MeshMetrics,
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

#[derive(Default)]
struct PackQueryStats {
    collision_tests: AtomicU64,
    direct_scans: AtomicU64,
    grid_queries: AtomicU64,
    grid_candidates: AtomicU64,
    pair_tests: AtomicU64,
    bbox_rejects: AtomicU64,
    narrow_phase: AtomicU64,
}

impl PackQueryStats {
    // AI-FUNC-SUMMARY: Format the collision counters as one `[GridStats] pack queries ...` line; counts under parallel short-circuiting `any` depend on scheduling and are diagnostic only; returns String; side effects: none.
    fn summary_line(&self) -> String {
        let get = |c: &AtomicU64| c.load(Ordering::Relaxed);
        format!(
            "[GridStats] pack queries collision_tests={} direct_scans={} grid_queries={} grid_candidates={} pair_tests={} bbox_rejects={} narrow_phase={}",
            get(&self.collision_tests),
            get(&self.direct_scans),
            get(&self.grid_queries),
            get(&self.grid_candidates),
            get(&self.pair_tests),
            get(&self.bbox_rejects),
            get(&self.narrow_phase)
        )
    }
}

struct PackCollider {
    bbox: Option<crate::types::BoundingBox>,
    shape: Option<parry3d_f64::shape::TriMesh>,
}

impl PackCollider {
    // AI-FUNC-SUMMARY: Prepare immutable bbox and collision acceleration once for a candidate or accepted periodic image, without retaining a second mesh copy.
    fn new(mesh: &Mesh) -> Self {
        Self {
            bbox: mesh_bbox(mesh),
            shape: to_parry_trimesh(mesh),
        }
    }

    // AI-FUNC-SUMMARY: Apply the legacy bbox rejection and solid-overlap/clearance predicate using cached shapes; missing geometry retains conservative behavior; increments the pair/bbox-reject/narrow-phase counters in stats (relaxed atomics, decisions unchanged).
    fn blocks(&self, other: &Self, gap: f64, stats: &PackQueryStats) -> bool {
        stats.pair_tests.fetch_add(1, Ordering::Relaxed);
        if let (Some(a), Some(b)) = (self.bbox, other.bbox) {
            let distance = bbox_distance(a, b);
            if (gap > 0.0 && distance >= gap) || (gap <= 0.0 && distance > 0.0) {
                stats.bbox_rejects.fetch_add(1, Ordering::Relaxed);
                return false;
            }
        }
        stats.narrow_phase.fetch_add(1, Ordering::Relaxed);
        if gap > 0.0 {
            mesh_distance_exact_prepared(
                self.bbox,
                self.shape.as_ref(),
                other.bbox,
                other.shape.as_ref(),
            ) < gap
        } else {
            mesh_collision_exact_prepared(
                self.bbox,
                self.shape.as_ref(),
                other.bbox,
                other.shape.as_ref(),
            )
        }
    }
}

// AI-FUNC-SUMMARY: Test a prepared candidate against incremental spatial neighbors and bbox-less colliders; small populations scan directly and all exact predicates share cached shapes; counts calls, direct scans, grid queries and grid candidates in stats without changing the result.
fn pack_blocked(
    candidate: &PackCollider,
    colliders: &[PackCollider],
    grid: &crate::geometry::spatial::SpatialGrid,
    gap: f64,
    stats: &PackQueryStats,
) -> bool {
    stats.collision_tests.fetch_add(1, Ordering::Relaxed);
    if colliders.len() < 32 || candidate.bbox.is_none() {
        stats.direct_scans.fetch_add(1, Ordering::Relaxed);
        return colliders
            .iter()
            .any(|other| candidate.blocks(other, gap, stats));
    }
    let mut neighbors = grid.query_neighbors_with_margin(candidate.bbox.unwrap(), gap, usize::MAX);
    stats.grid_queries.fetch_add(1, Ordering::Relaxed);
    stats.grid_candidates.fetch_add(neighbors.len() as u64, Ordering::Relaxed);
    neighbors.extend(
        colliders
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.bbox.is_none().then_some(i)),
    );
    if neighbors.len() < 32 {
        neighbors.iter().any(|&i| candidate.blocks(&colliders[i], gap, stats))
    } else {
        neighbors.par_iter().any(|&i| candidate.blocks(&colliders[i], gap, stats))
    }
}

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
        thread_pool.install(|| self.run_in_pool())
    }
}

impl PackPipeline {
    // AI-FUNC-SUMMARY: Execute loading, sequential proposal RNG, cached parallel feasibility and output under the configured Rayon worker budget; prints load/pack_loop/merge_orient/write_stl/report timings, [GridStats] occupancy and query counters, workers and peak RSS.
    fn run_in_pool(&self) -> Result<()> {
        let mut timer = crate::pipeline::timing::StageTimer::start("pack");
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

        println!(
            "[Info] Rotation mode: {}",
            self.config
                .packing
                .rotation_mode
                .as_deref()
                .unwrap_or("any")
        );

        timer.stage("load");
        let query_stats = PackQueryStats::default();
        let mut rng = rand::thread_rng();
        let mut placed: Vec<Mesh> = Vec::new();
        let mut colliders: Vec<PackCollider> = Vec::new();
        let extent = box_bounds.size();
        let mut collision_grid = crate::geometry::spatial::SpatialGrid::new(
            box_bounds,
            extent.x.max(extent.y).max(extent.z) / 8.0,
        );
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

                let candidate_collider = PackCollider::new(&candidate);
                if pack_blocked(
                    &candidate_collider,
                    &colliders,
                    &collision_grid,
                    min_neighbor,
                    &query_stats,
                ) {
                    reject_candidate!();
                }
                let candidate_ghosts: Vec<PackCollider> = if self.config.packing.mode == 3 {
                    generate_periodic_ghosts(&candidate, box_bounds)
                        .iter()
                        .map(PackCollider::new)
                        .collect()
                } else {
                    Vec::new()
                };
                if candidate_ghosts
                    .iter()
                    .any(|ghost| pack_blocked(ghost, &colliders, &collision_grid, min_neighbor, &query_stats))
                {
                    reject_candidate!();
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
                for collider in std::iter::once(candidate_collider).chain(candidate_ghosts) {
                    if let Some(bbox) = collider.bbox {
                        collision_grid.insert(colliders.len(), bbox);
                    }
                    colliders.push(collider);
                }
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
        timer.stage("pack_loop");
        if colliders.len() >= 32 {
            println!("{}", collision_grid.stats().summary_line("pack"));
        }
        println!("{}", query_stats.summary_line());

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
        timer.stage("merge_orient");
        save_stl(
            Path::new(&self.config.output.path),
            &final_mesh_oriented,
            "packed_result",
        )?;
        timer.stage("write_stl");

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
        timer.stage("report");
        timer.total("total_in_pool");
        timer.report_resources();
        Ok(())
    }
}

#[cfg(test)]
mod collider_tests {
    use super::*;
    use crate::geometry::spatial::SpatialGrid;
    use crate::geometry::{box_mesh, mesh_collision_exact, mesh_distance_exact};
    use crate::types::BoundingBox;

    // AI-FUNC-SUMMARY: Reproduce the old bbox-pruned collision plus minimum-distance scan as an independent cache/broad-phase oracle.
    fn original_blocked(candidate: &Mesh, existing: &[Mesh], gap: f64) -> bool {
        existing.iter().any(|other| {
            if let (Some(a), Some(b)) = (mesh_bbox(candidate), mesh_bbox(other)) {
                let d = bbox_distance(a, b);
                if (gap > 0.0 && d >= gap) || (gap <= 0.0 && d > 0.0) {
                    return false;
                }
            }
            mesh_collision_exact(candidate, other)
                || (gap > 0.0 && mesh_distance_exact(candidate, other) < gap)
        })
    }

    // AI-FUNC-SUMMARY: Compare cached incremental packing feasibility with the former full scan over boundary ghosts, nested solids, contact and exact clearance.
    #[test]
    fn cached_pack_candidates_match_original() {
        let domain = BoundingBox::from_size(Vec3::new(10.0, 10.0, 10.0));
        let cube = |x, y, z, edge| {
            box_mesh(BoundingBox {
                min: Vec3::new(x, y, z),
                max: Vec3::new(x + edge, y + edge, z + edge),
            })
        };
        let mut sources = vec![cube(-0.3, 1.0, 1.0, 0.8), cube(8.0, 8.0, 8.0, 3.0)];
        sources.extend(
            (0..40).map(|i| cube((i % 6) as f64 * 1.5, 3.0 + ((i / 6) % 6) as f64, 4.0, 0.4)),
        );
        for periodic in [false, true] {
            let mut meshes = sources.clone();
            if periodic {
                meshes.extend(
                    sources
                        .iter()
                        .flat_map(|m| generate_periodic_ghosts(m, domain)),
                );
            }
            let prepared: Vec<_> = meshes.iter().map(PackCollider::new).collect();
            let boxes: Vec<_> = prepared
                .iter()
                .enumerate()
                .filter_map(|(i, c)| c.bbox.map(|b| (i, b)))
                .collect();
            let grid = SpatialGrid::build(&boxes, domain, 1.25);
            let mut candidates: Vec<_> = (0..80)
                .map(|i| cube(i as f64 / 8.0 - 0.2, 1.0, 1.0, 0.3))
                .collect();
            candidates.extend([
                cube(8.5, 8.5, 8.5, 0.2),
                cube(7.0, 7.0, 7.0, 5.0),
                cube(0.75, 1.0, 1.0, 0.25),
            ]);
            for gap in [0.0, 0.25, 0.5] {
                for candidate in &candidates {
                    let mut queries = vec![candidate.clone()];
                    if periodic {
                        queries.extend(generate_periodic_ghosts(candidate, domain));
                    }
                    let expected = queries.iter().any(|q| original_blocked(q, &meshes, gap));
                    let actual = queries
                        .iter()
                        .any(|q| pack_blocked(&PackCollider::new(q), &prepared, &grid, gap, &PackQueryStats::default()));
                    assert_eq!(actual, expected, "periodic={periodic} gap={gap}");
                }
            }
        }
    }
}
