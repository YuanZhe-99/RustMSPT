use super::optimize_execution::{OptimizeS2, S2Method, run_island_batches};
use super::optimize_volume::IslandVolumes;
use crate::config::{parse_box_dimensions, OptimizationConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    bbox_distance, bbox_overlaps, check_boundary_constraints_mode,
    generate_periodic_ghosts, l2_norm, merge_meshes, mesh_bbox, mesh_centroid, mesh_volume,
    mesh_collision_exact_prepared, mesh_distance_exact_prepared, move_mesh_to_target_center,
    orient_components_to_positive_volume, rotate_mesh_around_center, split_mesh_into_granules,
    to_parry_trimesh, vec_norm, volume_fraction_of_meshes_in_bbox, wrap_mesh_centroid_to_box,
};
use crate::geometry::spatial::{SpatialGrid, estimate_cell_size};
use crate::io::{load_folder_stls, load_stl, save_stl};
use crate::pipeline::rotation::{parse_rotation_mode, sample_rotation_axis, RotationMode};
use crate::pipeline::{create_progress_bar, Pipeline};
use crate::types::{BoundingBox, Vec3};
use parry3d_f64::shape::TriMesh;
use rand::seq::index::sample;
use rand::Rng;
use rayon::ThreadPool;
use rayon::ThreadPoolBuilder;
use std::f64::consts::PI;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct OptimizePipeline {
    pub config: OptimizationConfig,
}

#[derive(Clone)]
struct ParticlePrepared {
    mesh: crate::types::Mesh,
    bbox: Option<BoundingBox>,
    shape: Option<TriMesh>,
}

#[allow(dead_code)]
struct IslandResult {
    best: Arc<GlobalBest>,
    s2_time: Duration,
    collision_time: Duration,
}

struct GlobalBest {
    loss: f64,
    particles: Vec<crate::types::Mesh>,
    s2: Vec<f64>,
}

// AI-FUNC-SUMMARY: Exchange immutable coherent best snapshots using only loss comparison and Arc operations under the mutex; return a better incoming snapshot, and destroy retired geometry after unlocking.
fn exchange_best_snapshot(global: &Mutex<Arc<GlobalBest>>, local: &Arc<GlobalBest>) -> Option<Arc<GlobalBest>> {
    let (incoming, retired) = {
        let mut shared = global.lock().unwrap();
        if local.loss < shared.loss {
            (None, Some(std::mem::replace(&mut *shared, Arc::clone(local))))
        } else if shared.loss < local.loss {
            (Some(Arc::clone(&shared)), None)
        } else {
            (None, None)
        }
    };
    drop(retired);
    incoming
}

// AI-FUNC-SUMMARY: Precompute acceleration data (bbox + parry3d shape) for one particle mesh; returns ParticlePrepared; side effects: None.
fn prepare_particle(mesh: crate::types::Mesh) -> ParticlePrepared {
    let bbox = mesh_bbox(&mesh);
    let shape = to_parry_trimesh(&mesh);
    ParticlePrepared { mesh, bbox, shape }
}

// AI-FUNC-SUMMARY: Merge prepared particles once without temporary mesh clones, returning stable vertex ranges for rigid candidate updates; rebuild after population migration.
fn merge_prepared_particles(prepared: &[ParticlePrepared]) -> (crate::types::Mesh, Vec<std::ops::Range<usize>>) {
    let mut merged = crate::types::Mesh {
        vertices: Vec::with_capacity(prepared.iter().map(|p| p.mesh.vertices.len()).sum()),
        faces: Vec::with_capacity(prepared.iter().map(|p| p.mesh.faces.len()).sum()),
    };
    let mut ranges = Vec::with_capacity(prepared.len());
    for particle in prepared {
        let start = merged.vertices.len();
        merged.vertices.extend_from_slice(&particle.mesh.vertices);
        merged.faces.extend(particle.mesh.faces.iter().map(|f| crate::types::Triangle { a: f.a + start, b: f.b + start, c: f.c + start }));
        ranges.push(start..merged.vertices.len());
    }
    (merged, ranges)
}

// AI-FUNC-SUMMARY: Format an S2 array as a fixed-width space-separated decimal string; returns String; side effects: None.
fn format_s2_series(values: &[f64]) -> String {
    values
        .iter()
        .map(|v| format!("{v:.6}"))
        .collect::<Vec<_>>()
        .join(" ")
}

// AI-FUNC-SUMMARY: Append a labeled S2 snapshot line to the optimization history log; mutates history_log; side effects: None.
fn push_history_s2(history_log: &mut Vec<String>, label: &str, values: &[f64]) {
    history_log.push(format!("{label}: {}", format_s2_series(values)));
}

// AI-FUNC-SUMMARY: Build a unified pruning progress message string with loss, VF, and particle count; returns String; side effects: None.
fn prune_progress_message(current_loss: f64, current_vf: f64, target_vf: f64, particles: usize) -> String {
    format!(
        "Loss {current_loss:.6} | VF {current_vf:.6}->{target_vf:.6} | particles {particles}"
    )
}

// AI-FUNC-SUMMARY:
// Purpose: Iteratively remove particles to approach the target volume fraction while minimizing S2 loss increase.
// Inputs: mutable particles vec, box bounds, target S2, optimization params, fixed S2 evaluator, and history log.
// Returns: Success or a propagated evaluator error (particles vector is pruned in place).
// Side effects: Mutates particles and history_log; prints progress; computes S2 evaluations (expensive).
// Notes: Uses the run-wide evaluator under the installed Rayon pool; adaptive candidate scoring and sample budget are unchanged. Early exits when VF is within tolerance.
fn selective_prune_to_target_vf(
    particles: &mut Vec<crate::types::Mesh>,
    box_bounds: BoundingBox,
    target_s2: &[f64],
    params: &crate::config::OptimizationParams,
    r_max: usize,
    evaluator: &OptimizeS2,
    history_log: &mut Vec<String>,
) -> Result<()> {
    if particles.len() < 2 {
        return Ok(());
    }

    let enabled = params.prune_enabled.unwrap_or(true);
    if !enabled {
        return Ok(());
    }

    let target_vf = target_s2.first().copied().unwrap_or(0.0).clamp(0.0, 1.0);
    if target_vf <= 0.0 {
        return Ok(());
    }

    let tol = params.prune_tolerance.unwrap_or(0.01).max(0.0);
    let max_rounds = params.prune_max_rounds.unwrap_or(200).max(1);
    let eval_samples = params
        .prune_eval_samples
        .unwrap_or((params.mc_samples / 4).max(1000))
        .max(200);
    let eval_rmax = r_max.min(target_s2.len().saturating_sub(1));

    let eval_loss = |parts: &[crate::types::Mesh]| -> Result<(f64, f64)> {
        let merged = merge_meshes(parts);
        let vf = volume_fraction_of_meshes_in_bbox(parts, box_bounds);
        let s2 = evaluator.evaluate(&merged, box_bounds, eval_rmax, eval_samples, "prune")?;
        let loss = l2_norm(&s2, target_s2);
        Ok((vf, loss))
    };

    let mut rng = rand::thread_rng();
    let mut rounds = 0usize;
    let mut current_vf = volume_fraction_of_meshes_in_bbox(particles, box_bounds);
    let (_, mut current_loss) = eval_loss(particles)?;

    println!(
        "[Info] Pruning stage: initial VF {current_vf:.6}, target VF {target_vf:.6}"
    );
    history_log.push(format!(
        "Pruning Start: particles {} | VF {current_vf:.6} -> target {target_vf:.6} | Loss {current_loss:.6}",
        particles.len(),
    ));

    let total_rounds = max_rounds as u64;
    let progress = create_progress_bar(
        total_rounds,
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>4}/{len:4} {msg}",
        "##-",
    );
    progress.set_message(prune_progress_message(
        current_loss,
        current_vf,
        target_vf,
        particles.len(),
    ));

    while current_vf > target_vf * (1.0 + tol) && particles.len() > 1 && rounds < max_rounds {
        let vf_gap = current_vf - target_vf;
        let (n_candidates, k_remove) = if vf_gap > 0.05 {
            (30usize, 10usize)
        } else if vf_gap > 0.025 {
            (15usize, 3usize)
        } else {
            (6usize, 1usize)
        };

        let n = n_candidates.min(particles.len());
        if n == 0 {
            break;
        }

        let sampled = sample(&mut rng, particles.len(), n);
        let mut scored: Vec<(usize, bool, f64, f64)> = Vec::new();
        for idx in sampled.iter() {
            let mut temp: Vec<crate::types::Mesh> = Vec::with_capacity(particles.len() - 1);
            for (i, p) in particles.iter().enumerate() {
                if i != idx {
                    temp.push(p.clone());
                }
            }
            if temp.is_empty() {
                continue;
            }

            let (vf, loss) = eval_loss(&temp)?;

            let still_above_target = vf >= target_vf;
            scored.push((idx, still_above_target, loss, vf));
        }

        if scored.is_empty() {
            break;
        }

        scored.sort_by(|a, b| {
            let (idx_a, above_a, loss_a, vf_a) = *a;
            let (idx_b, above_b, loss_b, vf_b) = *b;

            match (above_a, above_b) {
                (true, true) => {
                    // Both candidates keep VF above target: prioritize lower S2 loss.
                    loss_a
                        .partial_cmp(&loss_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            (vf_a - target_vf)
                                .abs()
                                .partial_cmp(&(vf_b - target_vf).abs())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                }
                (false, false) => {
                    // Both candidates drop below target: prioritize closest VF to target.
                    (vf_a - target_vf)
                        .abs()
                        .partial_cmp(&(vf_b - target_vf).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            loss_a
                                .partial_cmp(&loss_b)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                }
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
            }
            .then_with(|| idx_a.cmp(&idx_b))
        });
        let remove_count = k_remove.min(scored.len()).max(1);
        let mut remove_indices: Vec<usize> = scored
            .iter()
            .take(remove_count)
            .map(|(idx, _, _, _)| *idx)
            .collect();
        remove_indices.sort_unstable_by(|a, b| b.cmp(a));

        for idx in remove_indices {
            particles.remove(idx);
        }

        rounds += 1;
        let (vf_now, loss_now) = eval_loss(particles)?;
        current_vf = vf_now;
        current_loss = loss_now;
        progress.set_position(rounds as u64);
        progress.set_message(prune_progress_message(
            current_loss,
            current_vf,
            target_vf,
            particles.len(),
        ));
        if rounds % 5 == 0 || current_vf <= target_vf * (1.0 + tol) {
            println!(
                "[Info] Pruning round {rounds:>3} | particles {} | VF {current_vf:.6} | Loss {current_loss:.6}",
                particles.len(),
            );
        }
        history_log.push(format!(
            "Pruning Round {rounds}: particles {} | VF {current_vf:.6} | Loss {current_loss:.6}",
            particles.len(),
        ));
    }

    progress.finish_with_message("Pruning stage completed");

    println!(
        "[Info] Pruning completed: rounds {rounds}, particles {}, VF {current_vf:.6}, Loss {current_loss:.6}",
        particles.len(),
    );
    history_log.push(format!(
        "Pruning Completed: rounds {rounds} | particles {} | VF {current_vf:.6} | Loss {current_loss:.6}",
        particles.len(),
    ));
    Ok(())
}


// AI-FUNC-SUMMARY:
// Purpose: Run one simulated annealing island: perturb one random particle per iteration, check constraints/collisions, compute S2 loss, accept/reject via Metropolis criterion.
// Inputs: island_id, num_islands, initial prepared particles, target S2, optimization params, box bounds, boundary mode/params, rotation mode, fixed S2 evaluator, optional global best for migration, migration interval, and mutable history log.
// Returns: IslandResult with best particles, loss, S2, and timing, or a propagated evaluator error.
// Side effects: Mutates history_log; prints progress; exchanges coherent geometry/loss/S2 snapshots through global_best; evaluates all stages with the run-wide evaluator in the installed pool.
// Notes: Three move types (60% local translate+rotate, 30% move toward neighbor, 10% random reposition). Updates only moved-particle grid cells after acceptance; migration rebuilds the grid. Adaptive temperature adjusts within acceptance window.
#[allow(clippy::too_many_arguments)]
fn run_sa_island(
    island_id: usize,
    num_islands: usize,
    prepared_init: Vec<ParticlePrepared>,
    target: &[f64],
    params: &crate::config::OptimizationParams,
    box_bounds: BoundingBox,
    mode: u8,
    d1: f64,
    d2: f64,
    min_neighbor: f64,
    rotation_mode: &RotationMode,
    evaluator: &OptimizeS2,
    global_best: Option<&Arc<Mutex<Arc<GlobalBest>>>>,
    migration_interval: usize,
    history_log: &mut Vec<String>,
) -> Result<IslandResult> {
    let mut rng = rand::thread_rng();
    let mut temperature = params.initial_temperature.max(1e-8);
    let cooling_rate = params.cooling_rate.clamp(0.8, 0.99999);
    let adaptive_window = params
        .adaptive_temp_window
        .unwrap_or((params.max_iterations / 40).clamp(20, 100))
        .max(5);
    let target_accept_low = params.target_acceptance_low.unwrap_or(0.20).clamp(0.0, 1.0);
    let mut target_accept_high = params.target_acceptance_high.unwrap_or(0.45).clamp(0.0, 1.0);
    if target_accept_high <= target_accept_low {
        target_accept_high = (target_accept_low + 0.05).clamp(0.0, 1.0);
    }
    let heat_factor = params.adaptive_heat_factor.unwrap_or(1.08).max(1.0);
    let cool_factor = params.adaptive_cool_factor.unwrap_or(0.94).clamp(0.01, 1.0);
    let temp_ceiling_factor = params.adaptive_temp_ceiling_factor.unwrap_or(5.0).max(1.0);
    let temp_floor = 1e-9;
    let temp_ceiling = params.initial_temperature.max(1e-8) * temp_ceiling_factor;
    let mut window_trials = 0usize;
    let mut window_accepts = 0usize;

    let mut prepared = prepared_init;
    let (mut merged, mut vertex_ranges) = merge_prepared_particles(&prepared);
    let mut volumes = (evaluator.method == S2Method::MeshMc).then(|| IslandVolumes::new(prepared.iter().map(|p| &p.mesh), box_bounds));
    let mut volume_updates = 0usize;
    let mut current_s2 = evaluator.evaluate_with_vf(&merged, box_bounds, params.r_max, params.mc_samples.max(2000), "initial", volumes.as_ref().map(IslandVolumes::fraction))?;
    let mut current_loss = l2_norm(&current_s2, target);
    push_history_s2(history_log, "Post-Pruning S2", &current_s2);
    history_log.push(format!("Post-Pruning Loss: {current_loss:.6}"));

    let mut best = Arc::new(GlobalBest {
        particles: prepared.iter().map(|p| p.mesh.clone()).collect(),
        loss: current_loss,
        s2: current_s2.clone(),
    });
    let mut accepted_moves = 0usize;

    let mut s2_time = Duration::ZERO;
    let mut collision_time = Duration::ZERO;

    let bboxes_for_grid: Vec<(usize, BoundingBox)> = prepared
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.bbox.map(|b| (i, b)))
        .collect();
    let cell_size = estimate_cell_size(&bboxes_for_grid.iter().map(|&(_, b)| b).collect::<Vec<_>>());
    let mut grid = SpatialGrid::build(&bboxes_for_grid, box_bounds, cell_size);

    if num_islands > 1 {
        println!("[Info] Island {island_id}: starting with {} particles, initial loss {current_loss:.6}", prepared.len());
    } else {
        println!("[Info] Starting optimization with {} particles.", prepared.len());
        println!("[Info] Initial loss: {current_loss:.6}");
    }

    let total_iters = params.max_iterations.max(1) as u64;
    let progress = create_progress_bar(
        total_iters,
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>5}/{len:5} {msg}",
        "##-",
    );
    progress.set_message(format!(
        "Loss {current_loss:.6} | Best {:.6} | Temp {temperature:.6} | Acc 0", best.loss
    ));

    for iter in 0..params.max_iterations {
        let idx = rng.gen_range(0..prepared.len());
        let mut candidate = prepared[idx].mesh.clone();

        let scale = (temperature / params.initial_temperature.max(1e-8)).clamp(0.05, 1.0);
        let move_roll: f64 = rng.gen_range(0.0..1.0);

        if move_roll < 0.6 {
            let trans = Vec3::new(
                rng.gen_range(-params.max_translation..params.max_translation) * scale,
                rng.gen_range(-params.max_translation..params.max_translation) * scale,
                rng.gen_range(-params.max_translation..params.max_translation) * scale,
            );
            let center = mesh_centroid(&candidate);
            move_mesh_to_target_center(&mut candidate, center.add(trans));

            let rot_limit = params.max_rotation_deg.to_radians() * scale;
            if rot_limit > 1e-6 {
                if let Some(axis) = sample_rotation_axis(&mut rng, rotation_mode) {
                    let angle = rng.gen_range(-rot_limit..rot_limit);
                    rotate_mesh_around_center(&mut candidate, axis, angle);
                }
            }
        } else if move_roll < 0.9 {
            let target_idx = rng.gen_range(0..prepared.len());
            if target_idx != idx {
                let c0 = mesh_centroid(&candidate);
                let c1 = mesh_centroid(&prepared[target_idx].mesh);
                let direction = c1.sub(c0);
                let dist = vec_norm(direction);
                if dist > 1e-12 {
                    let step = (dist * 0.3).min(params.max_translation * scale * 2.0);
                    let move_vec = direction.scale(step / dist);
                    move_mesh_to_target_center(&mut candidate, c0.add(move_vec));
                }
            }
            if let Some(axis) = sample_rotation_axis(&mut rng, rotation_mode) {
                let angle =
                    rng.gen_range(-10.0f64.to_radians() * scale..10.0f64.to_radians() * scale);
                rotate_mesh_around_center(&mut candidate, axis, angle);
            }
        } else {
            let random_pos = Vec3::new(
                rng.gen_range(box_bounds.min.x..box_bounds.max.x),
                rng.gen_range(box_bounds.min.y..box_bounds.max.y),
                rng.gen_range(box_bounds.min.z..box_bounds.max.z),
            );
            move_mesh_to_target_center(&mut candidate, random_pos);

            if let Some(axis) = sample_rotation_axis(&mut rng, rotation_mode) {
                let angle = rng.gen_range(0.0..(2.0 * PI));
                rotate_mesh_around_center(&mut candidate, axis, angle);
            }
        }

        if mode == 3 {
            wrap_mesh_centroid_to_box(&mut candidate, box_bounds);
        }

        let candidate_bbox = mesh_bbox(&candidate);
        let candidate_shape = to_parry_trimesh(&candidate);

        let collision_start = Instant::now();
        if !check_boundary_constraints_mode(&candidate, box_bounds, mode, d1, d2) {
            temperature = (temperature * cooling_rate).max(temp_floor);
            window_trials += 1;
            if window_trials >= adaptive_window {
                let accept_rate = window_accepts as f64 / window_trials as f64;
                if accept_rate < target_accept_low {
                    temperature = (temperature * heat_factor).min(temp_ceiling);
                } else if accept_rate > target_accept_high {
                    temperature = (temperature * cool_factor).max(temp_floor);
                }
                window_trials = 0;
                window_accepts = 0;
            }
            progress.set_position((iter + 1) as u64);
            let acc = (accepted_moves as f64 / (iter + 1) as f64) * 100.0;
            progress.set_message(format!(
                "Loss {current_loss:.6} | Best {:.6} | Temp {temperature:.6} | Acc {acc:.1}%", best.loss
            ));
            collision_time += collision_start.elapsed();
            continue;
        }

        let mut blocked = false;
        {
            let mut neighbors = if let Some(cb) = candidate_bbox {
                grid.query_neighbors_with_margin(cb, min_neighbor, idx)
            } else {
                (0..prepared.len()).filter(|&j| j != idx).collect()
            };
            for (j, other) in prepared.iter().enumerate() {
                if j != idx && other.bbox.is_none() && !neighbors.contains(&j) {
                    neighbors.push(j);
                }
            }

            for j in neighbors {
                let other = &prepared[j];

                let mut bbox_gap: Option<f64> = None;
                if let (Some(cb), Some(ob)) = (candidate_bbox, other.bbox) {
                    let bd = bbox_distance(cb, ob);
                    bbox_gap = Some(bd);
                    if min_neighbor > 0.0 {
                        if bd >= min_neighbor {
                            continue;
                        }
                    } else if bd > 0.0 {
                        continue;
                    }
                }

                let overlap = mesh_collision_exact_prepared(
                    candidate_bbox,
                    candidate_shape.as_ref(),
                    other.bbox,
                    other.shape.as_ref(),
                );
                if overlap {
                    blocked = true;
                    break;
                }

                if min_neighbor > 0.0 {
                    let need_exact_distance = match bbox_gap {
                        Some(bd) => bd < min_neighbor,
                        None => true,
                    };
                    if need_exact_distance {
                        let d = mesh_distance_exact_prepared(
                            candidate_bbox,
                            candidate_shape.as_ref(),
                            other.bbox,
                            other.shape.as_ref(),
                        );
                        if d < min_neighbor {
                            blocked = true;
                            break;
                        }
                    }
                }
            }
        }
        if blocked {
            temperature = (temperature * cooling_rate).max(temp_floor);
            window_trials += 1;
            if window_trials >= adaptive_window {
                let accept_rate = window_accepts as f64 / window_trials as f64;
                if accept_rate < target_accept_low {
                    temperature = (temperature * heat_factor).min(temp_ceiling);
                } else if accept_rate > target_accept_high {
                    temperature = (temperature * cool_factor).max(temp_floor);
                }
                window_trials = 0;
                window_accepts = 0;
            }
            progress.set_position((iter + 1) as u64);
            let acc = (accepted_moves as f64 / (iter + 1) as f64) * 100.0;
            progress.set_message(format!(
                "Loss {current_loss:.6} | Best {:.6} | Temp {temperature:.6} | Acc {acc:.1}%", best.loss
            ));
            collision_time += collision_start.elapsed();
            continue;
        }

        if mode == 3 {
            let candidate_ghosts = generate_periodic_ghosts(&candidate, box_bounds);
            let mut ghost_blocked = false;
            for g in &candidate_ghosts {
                let g_bbox = mesh_bbox(g);
                let g_shape = to_parry_trimesh(g);
                let mut ghost_neighbors = if let Some(gb) = g_bbox {
                    grid.query_neighbors_with_margin(gb, min_neighbor, idx)
                } else {
                    (0..prepared.len()).filter(|&j| j != idx).collect()
                };
                for (j, other) in prepared.iter().enumerate() {
                    if j != idx && other.bbox.is_none() && !ghost_neighbors.contains(&j) {
                        ghost_neighbors.push(j);
                    }
                }
                for j in ghost_neighbors {
                    let other = &prepared[j];

                    let mut bbox_gap: Option<f64> = None;
                    if let (Some(gb), Some(ob)) = (g_bbox, other.bbox) {
                        let bd = bbox_distance(gb, ob);
                        bbox_gap = Some(bd);
                        if min_neighbor > 0.0 {
                            if bd >= min_neighbor {
                                continue;
                            }
                        } else if bd > 0.0 {
                            continue;
                        }
                    }

                    if mesh_collision_exact_prepared(
                        g_bbox,
                        g_shape.as_ref(),
                        other.bbox,
                        other.shape.as_ref(),
                    ) {
                        ghost_blocked = true;
                        break;
                    }
                    if min_neighbor > 0.0 {
                        let need_exact_distance = match bbox_gap {
                            Some(bd) => bd < min_neighbor,
                            None => true,
                        };
                        if need_exact_distance {
                            let d = mesh_distance_exact_prepared(
                                g_bbox,
                                g_shape.as_ref(),
                                other.bbox,
                                other.shape.as_ref(),
                            );
                            if d < min_neighbor {
                                ghost_blocked = true;
                                break;
                            }
                        }
                    }
                }
                if ghost_blocked {
                    break;
                }
            }
            if ghost_blocked {
                temperature = (temperature * cooling_rate).max(temp_floor);
                window_trials += 1;
                if window_trials >= adaptive_window {
                    let accept_rate = window_accepts as f64 / window_trials as f64;
                    if accept_rate < target_accept_low {
                        temperature = (temperature * heat_factor).min(temp_ceiling);
                    } else if accept_rate > target_accept_high {
                        temperature = (temperature * cool_factor).max(temp_floor);
                    }
                    window_trials = 0;
                    window_accepts = 0;
                }
                progress.set_position((iter + 1) as u64);
                let acc = (accepted_moves as f64 / (iter + 1) as f64) * 100.0;
                progress.set_message(format!(
                    "Loss {current_loss:.6} | Best {:.6} | Temp {temperature:.6} | Acc {acc:.1}%", best.loss
                ));
                collision_time += collision_start.elapsed();
                continue;
            }
        }
        collision_time += collision_start.elapsed();

        let original = std::mem::replace(&mut prepared[idx], prepare_particle(candidate));
        merged.vertices[vertex_ranges[idx].clone()].copy_from_slice(&prepared[idx].mesh.vertices);
        let adaptive_samples = ((params.mc_samples as f64) * (0.3 + 0.7 * scale)).round() as usize;
        let iter_samples = adaptive_samples.clamp(1000, params.mc_samples.max(1000));
        let s2_start = Instant::now();
        let old_volume = volumes.as_mut().map(|cache| cache.replace(idx, &prepared[idx].mesh));
        if volumes.is_some() {
            volume_updates += 1;
            if volume_updates % 64 == 0 {
                volumes = Some(IslandVolumes::new(prepared.iter().map(|p| &p.mesh), box_bounds));
            }
        }
        let candidate_s2 = evaluator.evaluate_with_vf(&merged, box_bounds, params.r_max, iter_samples, "candidate", volumes.as_ref().map(IslandVolumes::fraction))?;
        s2_time += s2_start.elapsed();
        let candidate_loss = l2_norm(&candidate_s2, target);
        let delta = candidate_loss - current_loss;

        let accept = if delta <= 0.0 {
            true
        } else {
            let threshold = (-delta / temperature).exp();
            rng.gen_bool(threshold.clamp(0.0, 1.0))
        };

        if accept {
            grid.update(idx, prepared[idx].bbox);
            accepted_moves += 1;
            current_s2 = candidate_s2;
            current_loss = candidate_loss;
            if current_loss < best.loss {
                best = Arc::new(GlobalBest {
                    loss: current_loss,
                    particles: prepared.iter().map(|p| p.mesh.clone()).collect(),
                    s2: current_s2.clone(),
                });
                history_log.push(format!(
                    "Iter {iter}: Loss {:.6} | S2 {}",
                    best.loss, format_s2_series(&best.s2)
                ));
            }
        } else {
            if let (Some(cache), Some(old)) = (&mut volumes, old_volume) { cache.restore(idx, old); }
            prepared[idx] = original;
            merged.vertices[vertex_ranges[idx].clone()].copy_from_slice(&prepared[idx].mesh.vertices);
        }

        temperature = (temperature * cooling_rate).max(temp_floor);
        window_trials += 1;
        if accept {
            window_accepts += 1;
        }
        if window_trials >= adaptive_window {
            let accept_rate = window_accepts as f64 / window_trials as f64;
            if accept_rate < target_accept_low {
                temperature = (temperature * heat_factor).min(temp_ceiling);
            } else if accept_rate > target_accept_high {
                temperature = (temperature * cool_factor).max(temp_floor);
            }
            window_trials = 0;
            window_accepts = 0;
        }

        if let Some(gb) = global_best {
            if migration_interval > 0 && (iter + 1) % migration_interval == 0 {
                if let Some(incoming) = exchange_best_snapshot(gb, &best) {
                    best = incoming;
                    prepared = best.particles.iter().map(|m| prepare_particle(m.clone())).collect();
                    (merged, vertex_ranges) = merge_prepared_particles(&prepared);
                    let bboxes_for_rebuild: Vec<(usize, BoundingBox)> = prepared.iter().enumerate()
                        .filter_map(|(i, p)| p.bbox.map(|b| (i, b)))
                        .collect();
                    grid = SpatialGrid::build(&bboxes_for_rebuild, box_bounds, cell_size);
                    volumes = (evaluator.method == S2Method::MeshMc).then(|| IslandVolumes::new(prepared.iter().map(|p| &p.mesh), box_bounds));
                    volume_updates = 0;
                    current_s2 = evaluator.evaluate_with_vf(&merged, box_bounds, params.r_max, params.mc_samples.max(2000), "migration", volumes.as_ref().map(IslandVolumes::fraction))?;
                    current_loss = l2_norm(&current_s2, target);
                    history_log.push(format!(
                        "Island {island_id} Iter {iter}: migrated best loss {:.6}", best.loss
                    ));
                }
            }
        }

        progress.set_position((iter + 1) as u64);
        let acc = (accepted_moves as f64 / (iter + 1) as f64) * 100.0;
        progress.set_message(format!(
            "Loss {current_loss:.6} | Best {:.6} | Temp {temperature:.6} | Acc {acc:.1}%", best.loss
        ));
        if temperature < 1e-9 {
            break;
        }
    }

    progress.finish_with_message("Optimization loop completed");

    Ok(IslandResult {
        best,
        s2_time,
        collision_time,
    })
}

impl Pipeline for OptimizePipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Execute the full optimization pipeline: load input, compute target S2, prune to target VF, run simulated annealing (single or island model), save best result.
    // Inputs: OptimizationConfig with input/output/target/box/optimization settings.
    // Returns: Ok(()) or error.
    // Side effects: Reads STL from disk; writes optimized STL and s2_history.txt; prints timing and progress to stdout.
    // Notes: Installs every stage in one bounded Rayon pool; resolves one S2 execution context and batches islands within the worker budget. Target S2 can come from manual_array or reference_stl.
    fn run(&self) -> Result<()> {
        let params = &self.config.optimization;
        let available_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let cpu_max = params.cpu_max.unwrap_or(-1);
        let thread_count = if cpu_max == -1 {
            available_cores
        } else {
            (cpu_max.max(1) as usize).min(available_cores)
        };
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .build()
            .map_err(|e| RustMsptError::InvalidConfig(format!("Failed to build thread pool: {e}")))?;
        let effective_pool_threads = thread_pool.install(rayon::current_num_threads);
        println!(
            "[Info] CPU setting: cpu_max={} -> using {} worker threads (available {}).",
            cpu_max, thread_count, available_cores
        );
        println!(
            "[Info] Rayon pool threads (effective): {}",
            effective_pool_threads
        );
        thread_pool.install(|| self.run_in_pool(&thread_pool))
    }
}

impl OptimizePipeline {
    // AI-FUNC-SUMMARY: Execute every optimize stage under one installed Rayon pool and a fixed S2 evaluator; returns success or input/output/backend error; writes STL, history and diagnostics.
    fn run_in_pool(&self, thread_pool: &ThreadPool) -> Result<()> {
        let run_start = Instant::now();
        let params = &self.config.optimization;
        let box_bounds = parse_box_dimensions(&self.config.r#box.dimensions)?;
        let mode = params.mode.unwrap_or(1);
        let d1 = params.min_boundary_dist.unwrap_or(0.0);
        let d2 = params.min_cross_boundary_depth.unwrap_or(0.0);
        let min_neighbor = params.min_neighbor_distance.unwrap_or(0.0);
        let rotation_mode = parse_rotation_mode(
            "optimization",
            params.rotation_mode.as_deref(),
            params.rotation_axis_vector.as_ref(),
        )?;

        println!("[Info] Rotation mode: {}", params.rotation_mode.as_deref().unwrap_or("any"));
        let input_path = Path::new(&self.config.input.stl_path);
        let mut particles: Vec<crate::types::Mesh> = Vec::new();
        if input_path.is_dir() {
            for (_, mesh) in load_folder_stls(input_path)? {
                particles.extend(split_mesh_into_granules(&mesh));
            }
        } else {
            let mesh = load_stl(input_path)?;
            particles.extend(split_mesh_into_granules(&mesh));
        }

        let loaded_particles = particles.len();
        particles.retain(|mesh| {
            mesh_bbox(mesh)
                .map(|bbox| bbox_overlaps(bbox, box_bounds))
                .unwrap_or(false)
        });
        let removed_outside = loaded_particles.saturating_sub(particles.len());
        if removed_outside > 0 {
            println!(
                "[Info] Pre-filter removed {} particles fully outside optimization bbox.",
                removed_outside
            );
        }

        if particles.is_empty() {
            return Err(RustMsptError::InvalidMesh(
                "No particles remain after bbox pre-filter for optimization".to_string(),
            ));
        }

        let merged_input = merge_meshes(&particles);
        let evaluator = OptimizeS2::new(params, box_bounds, &merged_input)?;
        println!("[Info] {}", evaluator.description);

        let target = match self.config.target.r#type.as_str() {
            "manual_array" => self
                .config
                .target
                .s2_array
                .clone()
                .ok_or_else(|| RustMsptError::InvalidConfig("target.s2_array is required".to_string()))?,
            "reference_stl" => {
                let reference_path = self
                    .config
                    .target
                    .stl_path
                    .as_ref()
                    .ok_or_else(|| RustMsptError::InvalidConfig("target.stl_path is required".to_string()))?;
                let reference = load_stl(Path::new(reference_path))?;
                let reference_bbox = if let Some(bb) = &self.config.target.stl_bounding_box {
                    parse_box_dimensions(bb)?
                } else {
                    mesh_bbox(&reference).unwrap_or(BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0)))
                };
                evaluator.evaluate(&reference, reference_bbox, params.r_max, params.mc_samples.max(1000), "target")?
            }
            other => {
                return Err(RustMsptError::InvalidConfig(format!(
                    "Unknown target type: {other}"
                )))
            }
        };

        let mut history_log = vec![evaluator.description.clone()];
        let s2_method = evaluator.method.name();
        println!(
            "[Info] S2 config: method={}, r_max={}, mc_samples={}, voxel_pitch={:.6}",
            s2_method,
            params.r_max,
            params.mc_samples,
            params.voxel_pitch
        );

        let input_s2 = evaluator.evaluate(&merged_input, box_bounds, params.r_max, params.mc_samples.max(1000), "input")?;
        let input_vf = volume_fraction_of_meshes_in_bbox(&particles, box_bounds);
        let input_loss = l2_norm(&input_s2, &target);
        println!(
            "[Info] Input stage: VF {input_vf:.6}, Loss {input_loss:.6}, Method {s2_method}"
        );
        push_history_s2(&mut history_log, "Target S2", &target);
        push_history_s2(&mut history_log, "Input S2", &input_s2);
        history_log.push(format!("Input Loss: {input_loss:.6}"));

        selective_prune_to_target_vf(
            &mut particles,
            box_bounds,
            &target,
            params,
            params.r_max,
            &evaluator,
            &mut history_log,
        )?;

        let prepared: Vec<ParticlePrepared> = particles
            .into_iter()
            .map(prepare_particle)
            .collect();

        let num_islands = params.islands.unwrap_or(1).max(1);
        let migration_interval = params.migration_interval.unwrap_or(100).max(1);

        println!("[Info] Island model: {num_islands} islands, shared worker budget {}, migration every {migration_interval} iterations", thread_pool.current_num_threads());
        history_log.push(format!("Execution: workers={}, islands={num_islands}, active_island_limit={}",
            rayon::current_num_threads(), num_islands.min(thread_pool.current_num_threads())));
        let global_best = Arc::new(Mutex::new(Arc::new(GlobalBest { loss: f64::MAX, particles: Vec::new(), s2: Vec::new() })));
        let run = |island_id, initial| {
            let mut island_history = Vec::new();
            let result = run_sa_island(
                island_id, num_islands, initial, &target, params, box_bounds,
                mode, d1, d2, min_neighbor, &rotation_mode, &evaluator,
                (num_islands > 1).then_some(&global_best), migration_interval, &mut island_history,
            );
            (result, island_history)
        };
        let results = if num_islands == 1 {
            vec![run(0, prepared)]
        } else {
            run_island_batches(num_islands, |island_id| run(island_id, prepared.clone()))
        };
        let mut best: Option<IslandResult> = None;
        for (result, history) in results {
            let result = result?;
            history_log.extend(history);
            if best.as_ref().is_none_or(|previous| result.best.loss < previous.best.loss) {
                best = Some(result);
            }
        }
        let best = best.unwrap();
        let search_loss = best.best.loss;
        let (s2_time, collision_time) = (best.s2_time, best.collision_time);
        let best_mesh = merge_meshes(&best.best.particles);
        let enable_orient = params.orient_to_positive_volume.unwrap_or(false);
        let (best_mesh_oriented, flipped_components, component_count) = if enable_orient {
            orient_components_to_positive_volume(&best_mesh)
        } else {
            (best_mesh.clone(), 0usize, 0usize)
        };
        let best_s2 = evaluator.evaluate(&best_mesh_oriented, box_bounds, params.r_max,
            params.mc_samples.max(2000), "final")?;
        let best_loss = l2_norm(&best_s2, &target);
        history_log.push(format!("Selected Search Loss: {search_loss:.6}"));
        push_history_s2(&mut history_log, "Final Best S2", &best_s2);
        history_log.push(format!("Final Best Loss: {best_loss:.6}"));

        save_stl(
            Path::new(&self.config.output.path),
            &best_mesh_oriented,
            "optimized_structure",
        )?;

        let output_path = Path::new(&self.config.output.path);
        let history_path = output_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("s2_history.txt");
        fs::write(&history_path, history_log.join("\n"))?;

        println!("[Info] Optimization completed.");
        println!("[Info] Best loss: {best_loss:.6}");
        println!("[Info] Final volume: {:.6}", mesh_volume(&best_mesh_oriented));
        println!("[Info] Final S2 points: {}", best_s2.len());
        println!("[Info] Orientation fix enabled: {}", enable_orient);
        if enable_orient {
            println!(
                "[Info] Orientation fix: flipped {flipped_components}/{component_count} components to positive signed volume"
            );
        }
        println!("[Info] S2 history saved: {}", history_path.display());
        let total_elapsed = run_start.elapsed();
        println!(
            "[Info] Timing summary: total {:.2}s | S2 {:.2}s | collision/constraints {:.2}s",
            total_elapsed.as_secs_f64(),
            s2_time.as_secs_f64(),
            collision_time.as_secs_f64()
        );
        Ok(())
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    // AI-FUNC-SUMMARY: Exercise actual island initialization, candidate scoring and migration under a bounded pool and verify the adopted best geometry/loss/S2 snapshot stays coherent.
    #[test]
    fn migration_keeps_best_geometry_loss_and_curve_together() {
        let params: crate::config::OptimizationParams = serde_json::from_value(serde_json::json!({
            "max_iterations": 32, "initial_temperature": 0.1, "cooling_rate": 0.99,
            "r_max": 0, "voxel_pitch": 1.0, "mc_method": "exact", "mc_samples": 200,
            "max_translation": 0.00000001, "max_rotation_deg": 0.0,
            "acceleration": {"mode": "cpu"}
        })).unwrap();
        let bbox = BoundingBox::from_size(Vec3::new(8.0, 8.0, 8.0));
        let initial = crate::geometry::box_mesh(BoundingBox { min: Vec3::new(1.0,1.0,1.0), max: Vec3::new(2.0,2.0,2.0) });
        let incoming = crate::geometry::box_mesh(BoundingBox { min: Vec3::new(3.0,3.0,3.0), max: Vec3::new(5.0,5.0,5.0) });
        let target = vec![8.0/512.0];
        for workers in [1, 2, 8] {
            let pool = ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
            let evaluator = OptimizeS2::new(&params, bbox, &initial).unwrap();
            let gb = Arc::new(Mutex::new(Arc::new(GlobalBest { particles: vec![incoming.clone()], loss: 0.0, s2: target.clone() })));
            let mut history = Vec::new();
            let rotation = parse_rotation_mode("optimization", Some("none"), None).unwrap();
            let result = pool.install(|| run_sa_island(0, 2, vec![prepare_particle(initial.clone())], &target,
                &params, bbox, 1, 0.0, 0.0, 0.0, &rotation, &evaluator, Some(&gb), 1, &mut history)).unwrap();
            assert_eq!(result.best.loss, 0.0);
            assert_eq!(result.best.s2, target);
            assert_eq!(result.best.particles[0].vertices, incoming.vertices);
            let observations = evaluator.observations.lock().unwrap();
            for stage in ["initial", "candidate", "migration"] {
                assert!(observations.iter().any(|(s, n, i)| *s == stage && *n == workers && i.is_some_and(|i| i < workers)), "missing {stage}");
            }
        }
    }
    // AI-FUNC-SUMMARY: Compare persistent merged vertex ranges against complete merges after candidate changes, rollback, and a migration with different population topology.
    #[test]
    fn merged_ranges_match_full_rebuild_and_rollback() {
        use crate::geometry::{box_mesh, icosphere_mesh, translate_mesh};
        let sources = vec![
            box_mesh(BoundingBox::from_size(Vec3::new(1.0, 2.0, 3.0))),
            icosphere_mesh(Vec3::new(5.0, 5.0, 5.0), 0.5, 1),
        ];
        let mut particles: Vec<_> = sources.iter().cloned().map(prepare_particle).collect();
        let (mut merged, mut ranges) = merge_prepared_particles(&particles);
        for step in 0..20 {
            let idx = step % particles.len();
            let original = particles[idx].clone();
            translate_mesh(&mut particles[idx].mesh, Vec3::new(0.1, -0.2, 0.3));
            merged.vertices[ranges[idx].clone()].copy_from_slice(&particles[idx].mesh.vertices);
            let oracle = merge_meshes(&particles.iter().map(|p| p.mesh.clone()).collect::<Vec<_>>());
            assert_eq!(merged.vertices, oracle.vertices);
            assert_eq!(merged.faces.iter().map(|f| (f.a,f.b,f.c)).collect::<Vec<_>>(), oracle.faces.iter().map(|f| (f.a,f.b,f.c)).collect::<Vec<_>>());
            particles[idx] = original;
            merged.vertices[ranges[idx].clone()].copy_from_slice(&particles[idx].mesh.vertices);
            assert_eq!(merged.vertices, merge_meshes(&sources).vertices);
        }
        particles.push(prepare_particle(icosphere_mesh(Vec3::new(8.0, 8.0, 8.0), 0.5, 2)));
        (merged, ranges) = merge_prepared_particles(&particles);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges.last().unwrap().end, merged.vertices.len());
        assert_eq!(merged.vertices, merge_meshes(&particles.iter().map(|p| p.mesh.clone()).collect::<Vec<_>>()).vertices);
    }

    // AI-FUNC-SUMMARY: Verify parallel migration publishes and receives the original Arc allocation with coherent loss/curve/geometry, preserves strict ties and leaves old snapshots immutable.
    #[test]
    fn migration_snapshots_share_storage_and_remain_coherent() {
        let make = |id: usize| Arc::new(GlobalBest {
            loss: id as f64,
            s2: vec![id as f64],
            particles: vec![crate::types::Mesh { vertices: vec![Vec3::new(id as f64, 0.0, 0.0)], faces: Vec::new() }],
        });
        let old = make(100);
        let global = Mutex::new(Arc::clone(&old));
        let candidates: Vec<_> = (0..16).map(make).collect();
        std::thread::scope(|scope| {
            for candidate in &candidates {
                let global = &global;
                scope.spawn(move || {
                    if let Some(incoming) = exchange_best_snapshot(global, candidate) {
                        assert!(incoming.loss < candidate.loss);
                        assert_eq!(incoming.s2[0], incoming.loss);
                        assert_eq!(incoming.particles[0].vertices[0].x, incoming.loss);
                    }
                });
            }
        });
        let winner = Arc::clone(&global.lock().unwrap());
        assert!(Arc::ptr_eq(&winner, &candidates[0]));
        let adopted = exchange_best_snapshot(&global, &old).unwrap();
        assert!(Arc::ptr_eq(&adopted, &winner));
        assert_eq!(adopted.particles.as_ptr(), candidates[0].particles.as_ptr());
        assert_eq!(adopted.s2.as_ptr(), candidates[0].s2.as_ptr());
        assert!(exchange_best_snapshot(&global, &make(0)).is_none());
        assert!(Arc::ptr_eq(&global.lock().unwrap(), &winner));
        assert_eq!(old.loss, 100.0);
        assert_eq!(old.s2, vec![100.0]);
        assert_eq!(old.particles[0].vertices[0].x, 100.0);
    }

    // AI-FUNC-SUMMARY: Compare the former locked deep-copy publication with immutable Arc exchange for a prepared sphere snapshot; report payload bytes and alternating warm release samples, excluding snapshot preparation.
    #[test]
    #[ignore = "release migration microbenchmark"]
    fn migration_snapshot_benchmark() {
        let mesh = crate::geometry::icosphere_mesh(Vec3::new(0.0,0.0,0.0),1.0,5);
        let payload = mesh.vertices.len()*std::mem::size_of::<Vec3>() + mesh.faces.len()*std::mem::size_of::<crate::types::Triangle>() + std::mem::size_of::<crate::types::Mesh>() + 128*8;
        let local = Arc::new(GlobalBest { loss: 1.0, particles: vec![mesh], s2: vec![0.5;128] });
        let empty = Arc::new(GlobalBest { loss: f64::MAX, particles: Vec::new(), s2: Vec::new() });
        let run = |legacy: bool| {
            let start = std::time::Instant::now();
            if legacy {
                let global = Mutex::new(GlobalBest { loss: f64::MAX, particles: Vec::new(), s2: Vec::new() });
                for _ in 0..1000 {
                    global.lock().unwrap().loss = f64::MAX;
                    let mut shared = global.lock().unwrap();
                    if local.loss < shared.loss {
                        shared.loss = local.loss;
                        shared.particles = local.particles.clone();
                        shared.s2 = local.s2.clone();
                    }
                    std::hint::black_box(&*shared);
                }
            } else {
                let global = Mutex::new(Arc::clone(&empty));
                for _ in 0..1000 {
                    *global.lock().unwrap() = Arc::clone(&empty);
                    std::hint::black_box(exchange_best_snapshot(&global, &local));
                }
            }
            start.elapsed().as_secs_f64()
        };
        for sample in 0..6 {
            let (old,new) = if sample%2 == 0 { (run(true),run(false)) } else { let new=run(false); (run(true),new) };
            eprintln!("MIGRATION_BENCH sample={sample} repeats=1000 payload_bytes={payload} legacy={old:.9} candidate={new:.9}");
        }
    }

}
