use crate::config::{parse_box_dimensions, OptimizationConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    bbox_distance, calculate_s2, check_boundary_constraints_mode,
    generate_periodic_ghosts, l2_norm, merge_meshes, mesh_bbox, mesh_centroid, mesh_volume,
    mesh_collision_exact_prepared, mesh_distance_exact_prepared, move_mesh_to_target_center,
    orient_components_to_positive_volume, rotate_mesh_around_center, split_mesh_into_granules,
    to_parry_trimesh, volume_fraction_of_meshes_in_bbox, wrap_mesh_centroid_to_box, vec_norm,
};
use crate::io::{load_folder_stls, load_stl, save_stl};
use crate::pipeline::{create_progress_bar, Pipeline};
use crate::types::{BoundingBox, Vec3};
use indicatif::ProgressBar;
use parry3d_f64::shape::TriMesh;
use rand::seq::index::sample;
use rand::Rng;
use rayon::ThreadPool;
use rayon::ThreadPoolBuilder;
use std::f64::consts::PI;
use std::fs;
use std::path::Path;
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

fn prepare_particle(mesh: crate::types::Mesh) -> ParticlePrepared {
    // Purpose: Precompute acceleration data for one particle mesh.
    // Inputs: particle mesh.
    // Outputs: particle with cached bbox and collision shape.
    let bbox = mesh_bbox(&mesh);
    let shape = to_parry_trimesh(&mesh);
    ParticlePrepared { mesh, bbox, shape }
}

fn format_s2_series(values: &[f64]) -> String {
    // Purpose: Convert S2 array to stable fixed-width text line.
    // Inputs: S2 values.
    // Outputs: whitespace-joined decimal series.
    values
        .iter()
        .map(|v| format!("{v:.6}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn push_history_s2(history_log: &mut Vec<String>, label: &str, values: &[f64]) {
    // Purpose: Append a labeled S2 snapshot to optimization history.
    // Inputs: mutable history log, label, and S2 values.
    // Outputs: one history line appended.
    history_log.push(format!("{label}: {}", format_s2_series(values)));
}

fn prune_progress_message(current_loss: f64, current_vf: f64, target_vf: f64, particles: usize) -> String {
    // Purpose: Build unified pruning progress/status message.
    // Inputs: loss, current/target VF, and particle count.
    // Outputs: formatted progress text.
    format!(
        "Loss {current_loss:.6} | VF {current_vf:.6}->{target_vf:.6} | particles {particles}"
    )
}

fn selective_prune_to_target_vf(
    particles: &mut Vec<crate::types::Mesh>,
    box_bounds: BoundingBox,
    target_s2: &[f64],
    params: &crate::config::OptimizationParams,
    r_max: usize,
    s2_method: &str,
    voxel_pitch: f64,
    thread_pool: &ThreadPool,
) {
    // Purpose: Remove particles before annealing to approach target VF while limiting S2 loss.
    // Inputs: particle set, target, optimization params, S2 settings, and thread pool.
    // Outputs: particles vector pruned in place with progress logs.
    if particles.len() < 2 {
        return;
    }

    let enabled = params.prune_enabled.unwrap_or(true);
    if !enabled {
        return;
    }

    let target_vf = target_s2.first().copied().unwrap_or(0.0).clamp(0.0, 1.0);
    if target_vf <= 0.0 {
        return;
    }

    let tol = params.prune_tolerance.unwrap_or(0.01).max(0.0);
    let max_rounds = params.prune_max_rounds.unwrap_or(200).max(1);
    let eval_samples = params
        .prune_eval_samples
        .unwrap_or((params.mc_samples / 4).max(1000))
        .max(200);
    let eval_rmax = r_max.min(target_s2.len().saturating_sub(1));

    let eval_loss = |parts: &[crate::types::Mesh]| {
        let merged = merge_meshes(parts);
        let vf = volume_fraction_of_meshes_in_bbox(parts, box_bounds);
        let s2 = thread_pool.install(|| {
            calculate_s2(&merged, box_bounds, eval_rmax, voxel_pitch, s2_method, eval_samples)
        });
        let loss = l2_norm(&s2, target_s2);
        (vf, loss)
    };

    let mut rng = rand::thread_rng();
    let mut rounds = 0usize;
    let mut current_vf = volume_fraction_of_meshes_in_bbox(particles, box_bounds);
    let (_, mut current_loss) = eval_loss(particles);

    println!(
        "[Info] Pruning stage: initial VF {current_vf:.6}, target VF {target_vf:.6}"
    );

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

            let (vf, loss) = eval_loss(&temp);

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
        let (vf_now, loss_now) = eval_loss(particles);
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
    }

    progress.finish_with_message("Pruning stage completed");

    println!(
        "[Info] Pruning completed: rounds {rounds}, particles {}, VF {current_vf:.6}, Loss {current_loss:.6}",
        particles.len(),
    );
}

impl Pipeline for OptimizePipeline {
    fn run(&self) -> Result<()> {
        // Purpose: Execute full optimization workflow (load, prune, anneal, save).
        // Inputs: optimization config and STL inputs.
        // Outputs: optimized structure STL and history logs.
        let run_start = Instant::now();
        let params = &self.config.optimization;
        let box_bounds = parse_box_dimensions(&self.config.r#box.dimensions)?;
        let mode = params.mode.unwrap_or(1);
        let d1 = params.min_boundary_dist.unwrap_or(0.0);
        let d2 = params.min_cross_boundary_depth.unwrap_or(0.0);
        let min_neighbor = params.min_neighbor_distance.unwrap_or(0.0);

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
        if particles.is_empty() {
            return Err(RustMsptError::InvalidMesh(
                "No particles loaded for optimization".to_string(),
            ));
        }

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
                calculate_s2(
                    &reference,
                    reference_bbox,
                    params.r_max,
                    params.voxel_pitch,
                    params.mc_method.as_str(),
                    params.mc_samples.max(1000),
                )
            }
            other => {
                return Err(RustMsptError::InvalidConfig(format!(
                    "Unknown target type: {other}"
                )))
            }
        };

        let mut history_log: Vec<String> = Vec::new();

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

        let mut apply_temperature_step = |accepted_this_iter: bool, temperature: &mut f64| {
            *temperature = (*temperature * cooling_rate).max(temp_floor);
            window_trials += 1;
            if accepted_this_iter {
                window_accepts += 1;
            }

            if window_trials >= adaptive_window {
                let accept_rate = window_accepts as f64 / window_trials as f64;
                if accept_rate < target_accept_low {
                    *temperature = (*temperature * heat_factor).min(temp_ceiling);
                } else if accept_rate > target_accept_high {
                    *temperature = (*temperature * cool_factor).max(temp_floor);
                }
                window_trials = 0;
                window_accepts = 0;
            }
        };
        let s2_method = params.mc_method.as_str();
        println!(
            "[Info] S2 config: method={}, r_max={}, mc_samples={}, voxel_pitch={:.6}",
            s2_method,
            params.r_max,
            params.mc_samples,
            params.voxel_pitch
        );
        println!(
            "[Info] SA temperature schedule: base_cooling={:.5}, adaptive_window={}, target_acceptance=[{:.2},{:.2}], heat_factor={:.3}, cool_factor={:.3}, ceiling_factor={:.2}",
            cooling_rate,
            adaptive_window,
            target_accept_low,
            target_accept_high,
            heat_factor,
            cool_factor,
            temp_ceiling_factor
        );

        let merged_input = merge_meshes(&particles);
        let input_s2 = thread_pool.install(|| {
            calculate_s2(
                &merged_input,
                box_bounds,
                params.r_max,
                params.voxel_pitch,
                s2_method,
                params.mc_samples.max(1000),
            )
        });
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
            s2_method,
            params.voxel_pitch,
            &thread_pool,
        );

        let mut prepared: Vec<ParticlePrepared> = particles
            .into_iter()
            .map(prepare_particle)
            .collect();

        let mut merged = merge_meshes(
            &prepared
                .iter()
                .map(|p| p.mesh.clone())
                .collect::<Vec<_>>(),
        );
        let mut current_s2 = thread_pool.install(|| {
            calculate_s2(
                &merged,
                box_bounds,
                params.r_max,
                params.voxel_pitch,
                s2_method,
                params.mc_samples.max(2000),
            )
        });
        let mut current_loss = l2_norm(&current_s2, &target);
        push_history_s2(&mut history_log, "Post-Pruning S2", &current_s2);
        history_log.push(format!("Post-Pruning Loss: {current_loss:.6}"));

        let mut best_particles = prepared.iter().map(|p| p.mesh.clone()).collect::<Vec<_>>();
        let mut best_loss = current_loss;
        let mut accepted_moves = 0usize;

        let mut s2_time = Duration::ZERO;
        let mut collision_time = Duration::ZERO;

        println!("[Info] Starting optimization with {} particles.", prepared.len());
        println!("[Info] Initial loss: {current_loss:.6}");

        let total_iters = params.max_iterations.max(1) as u64;
        let progress = create_progress_bar(
            total_iters,
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>5}/{len:5} {msg}",
            "##-",
        );
        progress.set_message(format!(
            "Loss {current_loss:.6} | Best {best_loss:.6} | Temp {temperature:.6} | Acc 0"
        ));

        let update_progress = |bar: &ProgressBar,
                               iter_done: usize,
                               current_loss: f64,
                               best_loss: f64,
                               temperature: f64,
                               accepted_moves: usize| {
            let acc = if iter_done == 0 {
                0.0
            } else {
                (accepted_moves as f64 / iter_done as f64) * 100.0
            };
            bar.set_position(iter_done as u64);
            bar.set_message(format!(
                "Loss {current_loss:.6} | Best {best_loss:.6} | Temp {temperature:.6} | Acc {acc:.1}%"
            ));
        };

        for iter in 0..params.max_iterations {
            let idx = rng.gen_range(0..prepared.len());
            let original = prepared[idx].mesh.clone();
            let mut candidate = original.clone();

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
                    let axis = Vec3::new(
                        rng.gen_range(-1.0..1.0),
                        rng.gen_range(-1.0..1.0),
                        rng.gen_range(-1.0..1.0),
                    );
                    let angle = rng.gen_range(-rot_limit..rot_limit);
                    rotate_mesh_around_center(&mut candidate, axis, angle);
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
                let axis = Vec3::new(
                    rng.gen_range(-1.0..1.0),
                    rng.gen_range(-1.0..1.0),
                    rng.gen_range(-1.0..1.0),
                );
                let angle = rng.gen_range(-10.0f64.to_radians() * scale..10.0f64.to_radians() * scale);
                rotate_mesh_around_center(&mut candidate, axis, angle);
            } else {
                let random_pos = Vec3::new(
                    rng.gen_range(box_bounds.min.x..box_bounds.max.x),
                    rng.gen_range(box_bounds.min.y..box_bounds.max.y),
                    rng.gen_range(box_bounds.min.z..box_bounds.max.z),
                );
                move_mesh_to_target_center(&mut candidate, random_pos);

                let axis = Vec3::new(
                    rng.gen_range(-1.0..1.0),
                    rng.gen_range(-1.0..1.0),
                    rng.gen_range(-1.0..1.0),
                );
                let angle = rng.gen_range(0.0..(2.0 * PI));
                rotate_mesh_around_center(&mut candidate, axis, angle);
            }

            if mode == 3 {
                wrap_mesh_centroid_to_box(&mut candidate, box_bounds);
            }

            let candidate_bbox = mesh_bbox(&candidate);
            let candidate_shape = to_parry_trimesh(&candidate);

            let collision_start = Instant::now();
            if !check_boundary_constraints_mode(&candidate, box_bounds, mode, d1, d2) {
                apply_temperature_step(false, &mut temperature);
                update_progress(&progress, iter + 1, current_loss, best_loss, temperature, accepted_moves);
                collision_time += collision_start.elapsed();
                continue;
            }

            let mut blocked = false;
            for (j, other) in prepared.iter().enumerate() {
                if j == idx {
                    continue;
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
                    // Broad-phase cull by bbox distance.
                    if let (Some(cb), Some(ob)) = (candidate_bbox, other.bbox) {
                        let bd = bbox_distance(cb, ob);
                        if bd < min_neighbor {
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
                apply_temperature_step(false, &mut temperature);
                update_progress(&progress, iter + 1, current_loss, best_loss, temperature, accepted_moves);
                collision_time += collision_start.elapsed();
                continue;
            }

            if mode == 3 {
                let candidate_ghosts = generate_periodic_ghosts(&candidate, box_bounds);
                let mut ghost_blocked = false;
                for g in &candidate_ghosts {
                    let g_bbox = mesh_bbox(g);
                    let g_shape = to_parry_trimesh(g);
                    for (j, other) in prepared.iter().enumerate() {
                        if j == idx {
                            continue;
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
                            if let (Some(gb), Some(ob)) = (g_bbox, other.bbox) {
                                let bd = bbox_distance(gb, ob);
                                if bd < min_neighbor {
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
                    }
                    if ghost_blocked {
                        break;
                    }
                }
                if ghost_blocked {
                    apply_temperature_step(false, &mut temperature);
                    update_progress(&progress, iter + 1, current_loss, best_loss, temperature, accepted_moves);
                    collision_time += collision_start.elapsed();
                    continue;
                }
            }
            collision_time += collision_start.elapsed();

            prepared[idx] = prepare_particle(candidate);
            merged = merge_meshes(
                &prepared
                    .iter()
                    .map(|p| p.mesh.clone())
                    .collect::<Vec<_>>(),
            );
            let adaptive_samples = ((params.mc_samples as f64) * (0.3 + 0.7 * scale)).round() as usize;
            let iter_samples = adaptive_samples.clamp(1000, params.mc_samples.max(1000));
            let s2_start = Instant::now();
            let candidate_s2 = thread_pool.install(|| {
                calculate_s2(
                    &merged,
                    box_bounds,
                    params.r_max,
                    params.voxel_pitch,
                    s2_method,
                    iter_samples,
                )
            });
            s2_time += s2_start.elapsed();
            let candidate_loss = l2_norm(&candidate_s2, &target);
            let delta = candidate_loss - current_loss;

            let accept = if delta <= 0.0 {
                true
            } else {
                let threshold = (-delta / temperature).exp();
                rng.gen_bool(threshold.clamp(0.0, 1.0))
            };

            if accept {
                accepted_moves += 1;
                current_s2 = candidate_s2;
                current_loss = candidate_loss;
                if current_loss < best_loss {
                    best_loss = current_loss;
                    best_particles = prepared.iter().map(|p| p.mesh.clone()).collect::<Vec<_>>();
                    let best_s2 = current_s2.clone();
                    history_log.push(format!(
                        "Iter {iter}: Loss {best_loss:.6} | S2 {}",
                        format_s2_series(&best_s2)
                    ));
                }
            } else {
                prepared[idx] = prepare_particle(original);
            }

            apply_temperature_step(accept, &mut temperature);
            update_progress(&progress, iter + 1, current_loss, best_loss, temperature, accepted_moves);
            if temperature < 1e-9 {
                break;
            }
        }

        progress.finish_with_message("Optimization loop completed");

        let best_mesh = merge_meshes(&best_particles);
        let (best_mesh_oriented, flipped_components, component_count) =
            orient_components_to_positive_volume(&best_mesh);
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
        println!("[Info] Final S2 points: {}", current_s2.len());
        println!(
            "[Info] Orientation fix: flipped {flipped_components}/{component_count} components to positive signed volume"
        );
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
