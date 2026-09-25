use crate::config::{parse_box_dimensions, PackingConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    bbox_distance, check_boundary_constraints_mode, merge_meshes,
    mesh_bbox, mesh_closer_than_prepared, mesh_collision_exact_prepared, mesh_metrics,
    mesh_surface_area, mesh_volume, move_mesh_to_target_center,
    orient_components_to_positive_volume, particle_volume_in_bbox, rotate_mesh_around_center,
    scale_mesh_to_equivalent_diameter, split_mesh_into_granules, to_parry_trimesh, translate_mesh,
    MeshMetrics,
};
use crate::geometry::spatial::SpatialGrid;
use crate::io::{load_stl, save_stl};
use crate::pipeline::pack_targets::{
    load_target_distribution_csv, write_distribution_comparison_csv, BinChoiceKind,
    SphericityState, TargetDistribution,
};
use crate::pipeline::rotation::{parse_rotation_mode, sample_rotation_axis};
use crate::pipeline::{create_progress_bar, Pipeline};
use crate::types::{BoundingBox, Mesh, Vec3};
use indicatif::ProgressBar;
use rand::Rng;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::collections::BTreeSet;
use std::f64::consts::PI;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;

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
    bbox: Option<BoundingBox>,
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
            mesh_closer_than_prepared(
                self.bbox,
                self.shape.as_ref(),
                other.bbox,
                other.shape.as_ref(),
                gap,
                false,
            )
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

// AI-FUNC-SUMMARY: Mirror PackCollider::blocks' bbox rejection on optional boxes; returns false only when the exact predicate would also reject on bbox distance; side effects: None.
fn bbox_may_block(a: Option<BoundingBox>, b: Option<BoundingBox>, gap: f64) -> bool {
    if let (Some(a), Some(b)) = (a, b) {
        let distance = bbox_distance(a, b);
        if (gap > 0.0 && distance >= gap) || (gap <= 0.0 && distance > 0.0) {
            return false;
        }
    }
    true
}

// AI-FUNC-SUMMARY: List the periodic shifts, in generate_periodic_ghosts order and with its strict domain-overlap rule, together with each shifted bbox; returns empty for bbox-less meshes; side effects: None.
fn periodic_image_shifts(bounds: Option<BoundingBox>, domain: BoundingBox) -> Vec<(Vec3, BoundingBox)> {
    let Some(bounds) = bounds else {
        return Vec::new();
    };
    let size = domain.size();
    let mut out = Vec::new();
    for x in [-1.0, 0.0, 1.0] {
        for y in [-1.0, 0.0, 1.0] {
            for z in [-1.0, 0.0, 1.0] {
                if x == 0.0 && y == 0.0 && z == 0.0 {
                    continue;
                }
                let shift = Vec3::new(x * size.x, y * size.y, z * size.z);
                let shifted = BoundingBox {
                    min: bounds.min.add(shift),
                    max: bounds.max.add(shift),
                };
                if shifted.min.x < domain.max.x
                    && shifted.max.x > domain.min.x
                    && shifted.min.y < domain.max.y
                    && shifted.max.y > domain.min.y
                    && shifted.min.z < domain.max.z
                    && shifted.max.z > domain.min.z
                {
                    out.push((shift, shifted));
                }
            }
        }
    }
    out
}

type CandidateImage = (Vec3, BoundingBox, OnceLock<PackCollider>);

struct PackImage {
    particle: usize,
    shift: Option<Vec3>,
    bbox: Option<BoundingBox>,
    collider: OnceLock<PackCollider>,
}

struct PackScene {
    images: Vec<PackImage>,
    grid: SpatialGrid,
    bbox_less: Vec<usize>,
    ghost_builds: AtomicUsize,
}

impl PackScene {
    // AI-FUNC-SUMMARY: Create an empty image store with the legacy domain/8 incremental grid; side effects: None.
    fn new(domain: BoundingBox) -> Self {
        let extent = domain.size();
        Self {
            images: Vec::new(),
            grid: SpatialGrid::new(domain, extent.x.max(extent.y).max(extent.z) / 8.0),
            bbox_less: Vec::new(),
            ghost_builds: AtomicUsize::new(0),
        }
    }

    // AI-FUNC-SUMMARY: Instantiate the translated mesh of one periodic image and prepare its collider; returns the collider and counts one ghost TriMesh build; side effects: increments ghost_builds.
    fn build_ghost(&self, mesh: &Mesh, shift: Vec3) -> PackCollider {
        self.ghost_builds.fetch_add(1, Ordering::Relaxed);
        let mut ghost = mesh.clone();
        translate_mesh(&mut ghost, shift);
        PackCollider::new(&ghost)
    }

    // AI-FUNC-SUMMARY: Return the prepared collider of an accepted image, lazily and thread-safely translating and preparing a periodic image on first use; side effects: may increment ghost_builds.
    fn collider<'a>(&'a self, index: usize, placed: &[Mesh]) -> &'a PackCollider {
        let image = &self.images[index];
        image.collider.get_or_init(|| {
            let shift = image.shift.expect("original images are prepared on insertion");
            self.build_ghost(&placed[image.particle], shift)
        })
    }

    // AI-FUNC-SUMMARY: Collect accepted images whose (possibly uninstantiated) bbox can block a query bbox at the gap, scanning directly for small stores and via grid plus bbox-less entries otherwise; returns image indices; side effects: counts one collision test, a direct scan or grid query/candidates, and bbox-pruned pairs as pair tests and bbox rejects in stats.
    fn reachable(&self, bbox: Option<BoundingBox>, gap: f64, stats: &PackQueryStats) -> Vec<usize> {
        stats.collision_tests.fetch_add(1, Ordering::Relaxed);
        let Some(query) = bbox else {
            stats.direct_scans.fetch_add(1, Ordering::Relaxed);
            return (0..self.images.len()).collect();
        };
        let mut indices = if self.images.len() < 32 {
            stats.direct_scans.fetch_add(1, Ordering::Relaxed);
            (0..self.images.len()).collect::<Vec<_>>()
        } else {
            let mut n = self
                .grid
                .query_neighbors_with_margin(query, gap.max(0.0), usize::MAX);
            stats.grid_queries.fetch_add(1, Ordering::Relaxed);
            stats.grid_candidates.fetch_add(n.len() as u64, Ordering::Relaxed);
            n.extend_from_slice(&self.bbox_less);
            n
        };
        let before = indices.len();
        indices.retain(|&i| bbox_may_block(Some(query), self.images[i].bbox, gap));
        let pruned = (before - indices.len()) as u64;
        stats.pair_tests.fetch_add(pruned, Ordering::Relaxed);
        stats.bbox_rejects.fetch_add(pruned, Ordering::Relaxed);
        indices
    }

    // AI-FUNC-SUMMARY: Decide whether a prepared query collider is blocked by any reachable accepted image, serial below 32 candidates and rayon any() above; returns bool; side effects: may lazily build ghost colliders and increments per-pair counters in stats.
    fn blocks_any(
        &self,
        query: &PackCollider,
        reachable: &[usize],
        placed: &[Mesh],
        gap: f64,
        stats: &PackQueryStats,
    ) -> bool {
        if reachable.len() < 32 {
            reachable
                .iter()
                .any(|&i| query.blocks(self.collider(i, placed), gap, stats))
        } else {
            reachable
                .par_iter()
                .any(|&i| query.blocks(self.collider(i, placed), gap, stats))
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Apply the legacy feasibility test (candidate and, in mode 3, every periodic candidate image against every accepted particle and periodic image) without instantiating images that cannot reach.
    // Inputs: accepted meshes, candidate mesh and its prepared collider, periodic domain (None outside mode 3), gap.
    // Returns: None when blocked; otherwise the candidate's periodic images as (shift, bbox, collider slot built only if it was needed) for insertion.
    // Side effects: May lazily build accepted ghost colliders, counts candidate ghost builds and updates the diagnostic query counters in stats (decisions unchanged).
    // Notes: bbox+shift equals the translated mesh bbox exactly because rounded addition is monotonic, so pruning matches the exact predicate's own bbox rejection; exact predicates run on the same translated coordinates as the former full ghost copies.
    fn candidate_images(
        &self,
        placed: &[Mesh],
        candidate: &Mesh,
        collider: &PackCollider,
        periodic: Option<BoundingBox>,
        gap: f64,
        stats: &PackQueryStats,
    ) -> Option<Vec<CandidateImage>> {
        let reachable = self.reachable(collider.bbox, gap, stats);
        if self.blocks_any(collider, &reachable, placed, gap, stats) {
            return None;
        }
        let Some(domain) = periodic else {
            return Some(Vec::new());
        };
        let mut images = Vec::new();
        for (shift, bbox) in periodic_image_shifts(collider.bbox, domain) {
            let reachable = self.reachable(Some(bbox), gap, stats);
            let slot = OnceLock::new();
            if !reachable.is_empty() {
                let ghost = slot.get_or_init(|| self.build_ghost(candidate, shift));
                if self.blocks_any(ghost, &reachable, placed, gap, stats) {
                    return None;
                }
            }
            images.push((shift, bbox, slot));
        }
        Some(images)
    }

    // AI-FUNC-SUMMARY: Record an accepted particle and its periodic image descriptors, inserting every bbox into the grid; ghost colliders stay lazy unless already built; side effects: mutates the store.
    fn insert(&mut self, particle: usize, collider: PackCollider, ghosts: Vec<CandidateImage>) {
        let original = PackImage {
            particle,
            shift: None,
            bbox: collider.bbox,
            collider: OnceLock::from(collider),
        };
        let ghost_images = ghosts.into_iter().map(|(shift, bbox, collider)| PackImage {
            particle,
            shift: Some(shift),
            bbox: Some(bbox),
            collider,
        });
        for image in std::iter::once(original).chain(ghost_images) {
            let index = self.images.len();
            match image.bbox {
                Some(bbox) => self.grid.insert(index, bbox),
                None => self.bbox_less.push(index),
            }
            self.images.push(image);
        }
    }

    // AI-FUNC-SUMMARY: Report stored image count, instantiated periodic ghost images and total ghost TriMesh builds; returns (stored, instantiated, builds); side effects: None.
    fn image_stats(&self) -> (usize, usize, usize) {
        let instantiated = self
            .images
            .iter()
            .filter(|i| i.shift.is_some() && i.collider.get().is_some())
            .count();
        (
            self.images.len(),
            instantiated,
            self.ghost_builds.load(Ordering::Relaxed),
        )
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
        let mut scene = PackScene::new(box_bounds);
        let periodic = (self.config.packing.mode == 3).then_some(box_bounds);
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
                let Some(candidate_ghosts) = scene.candidate_images(
                    &placed,
                    &candidate,
                    &candidate_collider,
                    periodic,
                    min_neighbor,
                    &query_stats,
                ) else {
                    reject_candidate!();
                };

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
                scene.insert(placed.len(), candidate_collider, candidate_ghosts);
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
        if scene.images.len() >= 32 {
            println!("{}", scene.grid.stats().summary_line("pack"));
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
        if periodic.is_some() {
            let (stored, instantiated, builds) = scene.image_stats();
            println!(
                "[Info] Periodic images: {stored} stored, {instantiated} accepted ghosts instantiated, {builds} ghost TriMesh builds"
            );
        }
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
    use crate::geometry::{
        box_mesh, generate_periodic_ghosts, icosphere_mesh, mesh_collision_exact,
        mesh_distance_exact,
    };

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

    // AI-FUNC-SUMMARY: Build an axis-aligned box mesh from a min corner and per-axis edges.
    fn cuboid(min: Vec3, edge: Vec3) -> Mesh {
        box_mesh(BoundingBox {
            min,
            max: min.add(edge),
        })
    }

    // AI-FUNC-SUMMARY: Return fixed accepted sources and candidates covering face, edge and diagonal (corner) wraps, near-domain-width particles whose +/- images both overlap the domain, nested and contact cases.
    fn fixtures(domain: BoundingBox) -> (Vec<Mesh>, Vec<Mesh>) {
        let o = domain.min;
        let s = domain.size();
        let at = |fx: f64, fy: f64, fz: f64| Vec3::new(o.x + fx * s.x, o.y + fy * s.y, o.z + fz * s.z);
        let e = |x: f64| Vec3::new(x, x, x);
        let mut sources = vec![
            cuboid(at(0.0, 0.1, 0.1).sub(Vec3::new(0.3, 0.0, 0.0)), e(0.8)),
            cuboid(at(1.0, 1.0, 1.0).sub(e(0.25)), e(0.6)),
            cuboid(at(0.0, 0.5, 0.5).sub(Vec3::new(0.05, 0.0, 0.0)), Vec3::new(s.x + 0.1, 0.4, 0.4)),
            icosphere_mesh(at(0.5, 0.0, 1.0), 0.45, 1),
        ];
        sources.extend((0..40).map(|i| {
            cuboid(
                at(0.05 + (i % 6) as f64 * 0.15, 0.3 + ((i / 6) % 6) as f64 * 0.08, 0.4),
                e(0.3),
            )
        }));
        let mut candidates: Vec<Mesh> = (0..60)
            .map(|i| cuboid(at(0.0, 0.1, 0.1).add(Vec3::new(i as f64 / 8.0 - 0.6, 0.0, 0.0)), e(0.3)))
            .collect();
        candidates.extend([
            cuboid(at(0.0, 0.0, 0.0).sub(e(0.2)), e(0.3)),
            cuboid(at(0.0, 0.0, 0.0).sub(e(0.05)), e(0.3)),
            cuboid(at(1.0, 0.0, 1.0).sub(Vec3::new(0.2, 0.1, 0.2)), e(0.3)),
            cuboid(at(0.0, 1.0, 0.0).sub(Vec3::new(0.1, 0.25, 0.05)), e(0.3)),
            cuboid(at(0.0, 0.2, 0.2).sub(Vec3::new(0.1, 0.0, 0.0)), Vec3::new(s.x - 0.05, 0.2, 0.2)),
            cuboid(at(0.0, 0.8, 0.8).sub(Vec3::new(0.1, 0.0, 0.0)), Vec3::new(s.x + 0.2, 0.2, 0.2)),
            cuboid(at(0.05, 0.05, 0.05), e(s.x.min(s.y).min(s.z) * 0.9)),
            cuboid(at(0.0, 0.52, 0.52).sub(Vec3::new(0.3, 0.0, 0.0)), e(0.2)),
            cuboid(at(0.5, 0.97, 0.03).sub(Vec3::new(0.0, 0.0, 0.1)), e(0.2)),
            icosphere_mesh(at(0.5, 1.0, 0.0), 0.3, 1),
            icosphere_mesh(at(0.5, 0.0, 1.0), 0.1, 1),
        ]);
        (sources, candidates)
    }

    // AI-FUNC-SUMMARY: Insert accepted meshes with lazy periodic descriptors, as the pipeline does after acceptance.
    fn scene_of(meshes: &[Mesh], domain: BoundingBox, periodic: bool) -> PackScene {
        let mut scene = PackScene::new(domain);
        for (i, mesh) in meshes.iter().enumerate() {
            let collider = PackCollider::new(mesh);
            let ghosts = if periodic {
                periodic_image_shifts(collider.bbox, domain)
                    .into_iter()
                    .map(|(shift, bbox)| (shift, bbox, OnceLock::new()))
                    .collect()
            } else {
                Vec::new()
            };
            scene.insert(i, collider, ghosts);
        }
        scene
    }

    // AI-FUNC-SUMMARY: Full-ghost oracle: candidate and every eager ghost copy against accepted meshes plus all their eager ghost copies.
    fn eager_blocked(candidate: &Mesh, accepted: &[Mesh], domain: BoundingBox, periodic: bool, gap: f64) -> bool {
        let mut existing = accepted.to_vec();
        let mut queries = vec![candidate.clone()];
        if periodic {
            existing.extend(accepted.iter().flat_map(|m| generate_periodic_ghosts(m, domain)));
            queries.extend(generate_periodic_ghosts(candidate, domain));
        }
        queries.iter().any(|q| original_blocked(q, &existing, gap))
    }

    // AI-FUNC-SUMMARY: Compare lazy periodic-image feasibility with the former full ghost scan over face/edge/corner wraps, near-width particles with duplicate images, nested solids, contact and clearance, and check shifted bboxes equal instantiated ones.
    #[test]
    fn cached_pack_candidates_match_original() {
        let domains = [
            BoundingBox::from_size(Vec3::new(10.0, 10.0, 10.0)),
            BoundingBox {
                min: Vec3::new(-3.3, 2.1, 5.7),
                max: Vec3::new(6.9, 9.4, 10.2),
            },
        ];
        for domain in domains {
            let (sources, candidates) = fixtures(domain);
            let mut outcomes = std::collections::BTreeMap::new();
            for periodic in [false, true] {
                let scene = scene_of(&sources, domain, periodic);
                for gap in [0.0, 0.25, 0.5] {
                    for (index, candidate) in candidates.iter().enumerate() {
                        let expected = eager_blocked(candidate, &sources, domain, periodic, gap);
                        let actual = scene
                            .candidate_images(
                                &sources,
                                candidate,
                                &PackCollider::new(candidate),
                                periodic.then_some(domain),
                                gap,
                                &PackQueryStats::default(),
                            )
                            .is_none();
                        assert_eq!(actual, expected, "periodic={periodic} gap={gap} candidate={index}");
                        outcomes.insert((periodic, gap.to_bits(), index), actual);
                    }
                }
                for (index, image) in scene.images.iter().enumerate() {
                    if let (Some(built), Some(stored)) = (image.collider.get(), image.bbox) {
                        let built = built.bbox.unwrap();
                        assert_eq!((built.min, built.max), (stored.min, stored.max), "image {index}");
                    }
                }
            }
            let blocked = outcomes.values().filter(|&&v| v).count();
            assert!(blocked > 0 && blocked < outcomes.len());
            let wrap_only = outcomes
                .iter()
                .filter(|((p, g, i), &v)| *p && v && !outcomes[&(false, *g, *i)])
                .count();
            assert!(wrap_only > 0, "no candidate is blocked only through a periodic image");
        }
    }

    // AI-FUNC-SUMMARY: Replay a fixed candidate sequence through incremental acceptance, asserting each decision equals the eager ghost oracle and that lazy ghost TriMesh builds stay below the eager per-image count.
    #[test]
    fn incremental_periodic_acceptance_matches_eager_and_builds_fewer_ghosts() {
        let domain = BoundingBox::from_size(Vec3::new(10.0, 10.0, 10.0));
        let (sources, candidates) = fixtures(domain);
        let sequence: Vec<Mesh> = sources.iter().chain(candidates.iter()).cloned().collect();
        for gap in [0.0, 0.3] {
            let mut scene = PackScene::new(domain);
            let stats = PackQueryStats::default();
            let mut accepted: Vec<Mesh> = Vec::new();
            let mut eager_builds = 0usize;
            for (index, candidate) in sequence.iter().enumerate() {
                let expected = eager_blocked(candidate, &accepted, domain, true, gap);
                eager_builds += generate_periodic_ghosts(candidate, domain).len();
                let collider = PackCollider::new(candidate);
                let result = scene.candidate_images(&accepted, candidate, &collider, Some(domain), gap, &stats);
                assert_eq!(result.is_none(), expected, "gap={gap} step={index}");
                if let Some(ghosts) = result {
                    scene.insert(accepted.len(), collider, ghosts);
                    accepted.push(candidate.clone());
                }
            }
            let (stored, instantiated, builds) = scene.image_stats();
            assert!(accepted.len() > 10 && stored > accepted.len());
            assert!(instantiated <= stored - accepted.len());
            assert!(builds < eager_builds, "lazy {builds} vs eager {eager_builds}");
            let first = scene.ghost_builds.load(Ordering::Relaxed);
            for candidate in &sequence {
                let _ = scene.candidate_images(&accepted, candidate, &PackCollider::new(candidate), Some(domain), gap, &stats);
            }
            let (_, instantiated_after, _) = scene.image_stats();
            assert!(instantiated_after >= instantiated && first == builds);
            let tests = stats.collision_tests.load(Ordering::Relaxed);
            assert!(tests >= sequence.len() as u64);
            assert_eq!(
                stats.pair_tests.load(Ordering::Relaxed),
                stats.bbox_rejects.load(Ordering::Relaxed) + stats.narrow_phase.load(Ordering::Relaxed)
            );
        }
    }
}
