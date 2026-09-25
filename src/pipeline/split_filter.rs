use rayon::prelude::*;
use crate::config::{SplitFilterConfig, SplitFilterVolume};
use crate::error::{Result, RustMsptError};
use crate::geometry::{mesh_bbox, mesh_surface_area, mesh_volume, split_mesh_into_granules};
use crate::io::{load_folder_stls, load_stl, save_stl};
use crate::pipeline::Pipeline;
use crate::types::Mesh;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

/// Average kept-particle face count from which output files are written two at a time. Below it the
/// pool fork/join per pair costs more than the write it overlaps (PLAN.Performance.md section 72).
const WRITE_PAIR_MIN_FACES: usize = 65_536;

pub struct SplitFilterPipeline {
    pub config: SplitFilterConfig,
}

#[derive(Clone, Copy)]
struct VolumeStats {
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
}

// AI-FUNC-SUMMARY: Compute min/max/mean/median volume statistics for particles that pass the keep filter; returns Option<VolumeStats>; side effects: None.
fn volume_stats_for_kept(volumes: &[f64], keep: &[bool]) -> Option<VolumeStats> {
    let mut vals: Vec<f64> = volumes
        .iter()
        .zip(keep.iter())
        .filter_map(|(v, k)| if *k { Some(*v) } else { None })
        .collect();
    if vals.is_empty() {
        return None;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = vals.len();
    let min = vals[0];
    let max = vals[n - 1];
    let mean = vals.iter().sum::<f64>() / n as f64;
    let median = if n % 2 == 1 {
        vals[n / 2]
    } else {
        0.5 * (vals[n / 2 - 1] + vals[n / 2])
    };
    Some(VolumeStats {
        min,
        max,
        mean,
        median,
    })
}

// AI-FUNC-SUMMARY: Count how many entries in the keep boolean vector are true; returns usize; side effects: None.
fn count_kept(keep: &[bool]) -> usize {
    keep.iter().filter(|k| **k).count()
}

// AI-FUNC-SUMMARY: Append a filter-step summary line showing before/after/removed counts; mutates lines vec; side effects: None.
fn report_step(lines: &mut Vec<String>, name: &str, before: usize, after: usize) {
    lines.push(format!(
        "{name}: before={before}, after={after}, removed={}",
        before.saturating_sub(after)
    ));
}

// AI-FUNC-SUMMARY:
// Purpose: Append a text histogram of volume values with configurable bin count.
// Inputs: mutable report lines, title, volume values, and bin count.
// Returns: None (appends lines).
// Side effects: Mutates the lines vector.
fn append_volume_histogram(lines: &mut Vec<String>, title: &str, values: &[f64], bins: usize) {
    if values.is_empty() {
        lines.push(format!("{title}: no data"));
        return;
    }

    let bins = bins.clamp(4, 32);
    let min_v = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max_v = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    lines.push(title.to_string());
    if (max_v - min_v).abs() < 1e-12 {
        lines.push(format!("  [{:.6}, {:.6}] | {}", min_v, max_v, values.len()));
        return;
    }

    let width = (max_v - min_v) / bins as f64;
    let mut counts = vec![0usize; bins];
    for &v in values {
        let mut b = ((v - min_v) / width).floor() as isize;
        if b < 0 {
            b = 0;
        }
        if b as usize >= bins {
            b = bins as isize - 1;
        }
        counts[b as usize] += 1;
    }

    let max_count = counts.iter().copied().max().unwrap_or(1).max(1);
    for (i, c) in counts.iter().enumerate() {
        let lo = min_v + i as f64 * width;
        let hi = if i + 1 == bins {
            max_v
        } else {
            min_v + (i + 1) as f64 * width
        };
        let bar_len = ((*c as f64 / max_count as f64) * 30.0).round() as usize;
        let bar = "#".repeat(bar_len.max(1));
        lines.push(format!("  [{:.6}, {:.6}) | {:>4} | {}", lo, hi, c, bar));
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Append a side-by-side comparison histogram of volume distributions before and after filtering.
// Inputs: mutable report lines, title, before/after value arrays, and bin count.
// Returns: None (appends lines).
// Side effects: Mutates the lines vector.
fn append_volume_histogram_comparison(
    lines: &mut Vec<String>,
    title: &str,
    before_values: &[f64],
    after_values: &[f64],
    bins: usize,
) {
    if before_values.is_empty() {
        lines.push(format!("{title}: no data"));
        return;
    }

    let bins = bins.clamp(4, 32);
    let min_v = before_values.iter().copied().fold(f64::INFINITY, f64::min);
    let max_v = before_values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    lines.push(title.to_string());
    if (max_v - min_v).abs() < 1e-12 {
        lines.push(format!(
            "  [{:.6}, {:.6}] | before {:>4} | after {:>4}",
            min_v,
            max_v,
            before_values.len(),
            after_values.len()
        ));
        return;
    }

    let width = (max_v - min_v) / bins as f64;
    let mut before_counts = vec![0usize; bins];
    let mut after_counts = vec![0usize; bins];

    for &v in before_values {
        let mut b = ((v - min_v) / width).floor() as isize;
        if b < 0 {
            b = 0;
        }
        if b as usize >= bins {
            b = bins as isize - 1;
        }
        before_counts[b as usize] += 1;
    }

    for &v in after_values {
        let mut b = ((v - min_v) / width).floor() as isize;
        if b < 0 {
            b = 0;
        }
        if b as usize >= bins {
            b = bins as isize - 1;
        }
        after_counts[b as usize] += 1;
    }

    let max_before = before_counts.iter().copied().max().unwrap_or(1).max(1);
    let max_after = after_counts.iter().copied().max().unwrap_or(1).max(1);

    for i in 0..bins {
        let lo = min_v + i as f64 * width;
        let hi = if i + 1 == bins {
            max_v
        } else {
            min_v + (i + 1) as f64 * width
        };

        let bc = before_counts[i];
        let ac = after_counts[i];
        let bbar_len = ((bc as f64 / max_before as f64) * 16.0).round() as usize;
        let abar_len = ((ac as f64 / max_after as f64) * 16.0).round() as usize;
        let bbar = "#".repeat(bbar_len.max(1));
        let abar = "#".repeat(abar_len.max(1));

        lines.push(format!(
            "  [{:.6}, {:.6}) | before {:>4} {:<16} | after {:>4} {:<16}",
            lo, hi, bc, bbar, ac, abar
        ));
    }
}

// AI-FUNC-SUMMARY: Approximate the normal CDF using the error function; returns f64 in [0,1]; side effects: None.
fn normal_cdf(x: f64) -> f64 {
    let z = x / (2.0f64).sqrt();
    0.5 * (1.0 + erf_approx(z))
}

// AI-FUNC-SUMMARY: Approximate the error function (erf) using Abramowitz & Stegun formula 7.1.26; returns f64; side effects: None.
fn erf_approx(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let ax = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t)
            * (-ax * ax).exp();
    sign * y
}

// AI-FUNC-SUMMARY:
// Purpose: Rebalance kept particles by fitting their log-volumes to a lognormal distribution and removing over-represented bins.
// Inputs: mutable keep flags, volume array, and SplitFilterVolume config with bins/over_factor.
// Returns: None (mutates keep to mark excess particles as false).
// Side effects: Mutates keep array; uses RNG for shuffling within over-represented bins.
fn apply_lognormal_rebalance(
    keep: &mut [bool],
    volumes: &[f64],
    cfg: &SplitFilterVolume,
    seed: Option<u64>,
) {
    let candidates: Vec<usize> = keep
        .iter()
        .enumerate()
        .filter_map(|(i, k)| if *k && volumes[i] > 0.0 { Some(i) } else { None })
        .collect();
    if candidates.len() < 4 {
        return;
    }

    let bins = cfg.bins.unwrap_or(12).clamp(4, 64);
    let over_factor = cfg.over_factor.unwrap_or(1.25).max(1.0);

    let log_vals: Vec<f64> = candidates.iter().map(|&i| volumes[i].ln()).collect();
    let n = log_vals.len() as f64;
    let mu = log_vals.iter().sum::<f64>() / n;
    let var = log_vals
        .iter()
        .map(|v| {
            let d = *v - mu;
            d * d
        })
        .sum::<f64>()
        / n.max(1.0);
    let sigma = var.sqrt().max(1e-9);

    let min_log = log_vals.iter().copied().fold(f64::INFINITY, f64::min);
    let max_log = log_vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (max_log - min_log).abs() < 1e-12 {
        return;
    }

    let width = (max_log - min_log) / bins as f64;
    let mut by_bin: HashMap<usize, Vec<usize>> = HashMap::new();
    for &idx in &candidates {
        let lv = volumes[idx].ln();
        let mut b = ((lv - min_log) / width).floor() as isize;
        if b < 0 {
            b = 0;
        }
        if b as usize >= bins {
            b = bins as isize - 1;
        }
        by_bin.entry(b as usize).or_default().push(idx);
    }

    let total = candidates.len() as f64;
    // Absent seed keeps the original unseeded generator, so an existing config
    // behaves exactly as it did; a seed makes the kept set reproducible.
    let mut rng: Box<dyn rand::RngCore> = match seed {
        Some(seed) => Box::new(crate::pipeline::rng::seeded_rng(seed)),
        None => Box::new(rand::thread_rng()),
    };

    for b in 0..bins {
        let Some(indices) = by_bin.get_mut(&b) else {
            continue;
        };
        let lo = min_log + b as f64 * width;
        let hi = lo + width;
        let p = (normal_cdf((hi - mu) / sigma) - normal_cdf((lo - mu) / sigma)).max(0.0);
        let expected = (p * total).ceil() as usize;
        let allowed = ((expected as f64) * over_factor).ceil() as usize;

        if indices.len() > allowed {
            indices.shuffle(&mut rng);
            for idx in indices.iter().skip(allowed) {
                keep[*idx] = false;
            }
        }
    }
}

// AI-FUNC-SUMMARY: Cache each component's original-order geometry reductions without changing component order.
struct ParticleMetrics {
    volume: f64,
    aspect: Option<f64>,
    area: Option<f64>,
}

// AI-FUNC-SUMMARY: Compute requested metrics per component; omit area for components rejected by the cheaper aspect/volume gates.
fn prepare_particle_metrics(particles: &[Mesh], max_aspect: Option<f64>, need_area: bool) -> Vec<ParticleMetrics> {
    let compute = |mesh: &Mesh| {
        let volume = mesh_volume(mesh);
        let aspect = max_aspect.and_then(|_| mesh_bbox(mesh)).map(|bbox| {
            let s = bbox.size();
            s.x.max(s.y).max(s.z) / s.x.min(s.y).min(s.z).max(1e-12)
        });
        let rejected = aspect.zip(max_aspect).is_some_and(|(value, limit)| value > limit);
        let area = (need_area && !rejected && (volume > 1e-12 || volume.is_nan())).then(|| mesh_surface_area(mesh));
        ParticleMetrics { volume, aspect, area }
    };
    if particles.len() < 32 {
        particles.iter().map(compute).collect()
    } else {
        particles.par_iter().map(compute).collect()
    }
}

impl Pipeline for SplitFilterPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Run the split-filter pipeline: load STL, split into connected components, apply geometric filters, save kept particles, and write report.
    // Inputs: SplitFilterConfig with input/output/filter settings.
    // Returns: Ok(()) or error.
    // Side effects: Reads STL from disk; writes filtered STL files and report to disk; prints summary to stdout.
    // Notes: Applies filters in order: max_aspect_ratio, max_sharpness_ratio, then volume filter (range or lognormal_rebalance).
    fn run(&self) -> Result<()> {
        let available = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        let requested = self.config.cpu_max.unwrap_or(-1);
        let workers = if requested == -1 { available } else { (requested.max(1) as usize).min(available) };
        let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build()
            .map_err(|e| RustMsptError::InvalidConfig(format!("split-filter worker pool: {e}")))?;
        pool.install(|| self.run_in_pool())
    }
}

impl SplitFilterPipeline {
    // AI-FUNC-SUMMARY: Run split, ordered metric preparation, serial filtering/RNG and output inside one configured pool; prints load/split/metrics/filter/write_stl/write_report/total_in_pool timings, workers and peak RSS.
    fn run_in_pool(&self) -> Result<()> {
        let input = Path::new(&self.config.input.path);
        let output_folder = Path::new(&self.config.output.folder);
        let prefix = self.config.output.prefix.trim();
        let report_path = self
            .config
            .output
            .report_path.as_deref()
            .map(Path::new)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                output_folder
                    .parent()
                    .unwrap_or(output_folder)
                    .join("split_filter_report.txt")
            });

        if prefix.is_empty() {
            return Err(RustMsptError::InvalidConfig(
                "output.prefix must not be empty".to_string(),
            ));
        }

        let mut timer = crate::pipeline::timing::StageTimer::start("split-filter");
        let sources: Vec<Mesh> = if input.is_dir() {
            load_folder_stls(input)?.into_iter().map(|(_, mesh)| mesh).collect()
        } else {
            vec![load_stl(input)?]
        };
        timer.stage("load");
        let mut particles: Vec<Mesh> = Vec::new();
        for mesh in sources {
            particles.extend(split_mesh_into_granules(&mesh));
        }
        timer.stage("split");

        if particles.is_empty() {
            return Err(RustMsptError::InvalidMesh(
                "No particles found after splitting input STL(s)".to_string(),
            ));
        }

        let mut keep = vec![true; particles.len()];
        let active_filter = self.config.filter.as_ref().filter(|filter| filter.enabled.unwrap_or(true));
        let metrics = prepare_particle_metrics(&particles, active_filter.and_then(|f| f.max_aspect_ratio), active_filter.is_some_and(|f| f.max_sharpness_ratio.is_some()));
        timer.stage("metrics");
        let volumes: Vec<f64> = metrics.iter().map(|m| m.volume).collect();
        let before_volumes = &volumes;
        let mut report_lines: Vec<String> = Vec::new();
        report_lines.push("Method: split_filter".to_string());
        report_lines.push(format!("Input path: {}", input.display()));
        report_lines.push(format!("Output folder: {}", output_folder.display()));
        report_lines.push(format!("Report path: {}", report_path.display()));
        report_lines.push(format!("Output prefix: {}", prefix));
        report_lines.push(format!("Total split particles: {}", particles.len()));
        report_lines.push("".to_string());
        report_lines.push("Filter steps:".to_string());
        let mut before_step = count_kept(&keep);
        report_step(&mut report_lines, "initial", before_step, before_step);

        if let Some(filter) = &self.config.filter {
            if filter.enabled.unwrap_or(true) {
                if let Some(max_ar) = filter.max_aspect_ratio {
                    for (i, metric) in metrics.iter().enumerate() {
                        if keep[i] && metric.aspect.is_some_and(|aspect| aspect > max_ar) {
                            keep[i] = false;
                        }
                    }
                    let after = count_kept(&keep);
                    report_step(
                        &mut report_lines,
                        &format!("max_aspect_ratio <= {max_ar}"),
                        before_step,
                        after,
                    );
                    before_step = after;
                } else {
                    report_lines.push("max_aspect_ratio: skipped".to_string());
                }

                if let Some(max_sharp) = filter.max_sharpness_ratio {
                    for (i, metric) in metrics.iter().enumerate() {
                        if !keep[i] {
                            continue;
                        }
                        let volume = volumes[i];
                        if volume <= 1e-12 {
                            keep[i] = false;
                            continue;
                        }
                        let area = metric.area.expect("area prepared for surviving sharpness candidate");
                        let sharpness = (area.powi(3)) / (36.0 * PI * volume.powi(2));
                        if sharpness > max_sharp {
                            keep[i] = false;
                        }
                    }
                    let after = count_kept(&keep);
                    report_step(
                        &mut report_lines,
                        &format!("max_sharpness_ratio <= {max_sharp}"),
                        before_step,
                        after,
                    );
                    before_step = after;
                } else {
                    report_lines.push("max_sharpness_ratio: skipped".to_string());
                }

                if let Some(vol_cfg) = &filter.volume {
                    let mode = vol_cfg.mode.as_deref().unwrap_or("none").to_ascii_lowercase();
                    if mode == "range" {
                        let vmin = vol_cfg.min.unwrap_or(-1.0);
                        let vmax = vol_cfg.max.unwrap_or(-1.0);
                        for (i, v) in volumes.iter().enumerate() {
                            if !keep[i] {
                                continue;
                            }
                            if vmin >= 0.0 && *v < vmin {
                                keep[i] = false;
                                continue;
                            }
                            if vmax >= 0.0 && *v > vmax {
                                keep[i] = false;
                            }
                        }
                        let after = count_kept(&keep);
                        report_step(
                            &mut report_lines,
                            &format!(
                                "volume range filter (min={}, max={})",
                                vol_cfg.min.unwrap_or(-1.0),
                                vol_cfg.max.unwrap_or(-1.0)
                            ),
                            before_step,
                            after,
                        );
                    } else if mode == "lognormal_rebalance" {
                        apply_lognormal_rebalance(&mut keep, &volumes, vol_cfg, self.config.seed);
                        let after = count_kept(&keep);
                        report_step(
                            &mut report_lines,
                            &format!(
                                "volume lognormal_rebalance (bins={}, over_factor={})",
                                vol_cfg.bins.unwrap_or(12),
                                vol_cfg.over_factor.unwrap_or(1.25)
                            ),
                            before_step,
                            after,
                        );
                    } else {
                        report_lines.push("volume filter: skipped (mode=none)".to_string());
                    }
                } else {
                    report_lines.push("volume filter: skipped".to_string());
                }
            } else {
                report_lines.push("All filtering skipped (filter.enabled=false).".to_string());
            }
        } else {
            report_lines.push("All filtering skipped (filter section missing).".to_string());
        }

        let kept_indices: Vec<usize> = keep
            .iter()
            .enumerate()
            .filter_map(|(i, k)| if *k { Some(i) } else { None })
            .collect();

        if kept_indices.is_empty() {
            return Err(RustMsptError::InvalidMesh(
                "All particles were removed by split_filter rules".to_string(),
            ));
        }

        timer.stage("filter");
        fs::create_dir_all(output_folder)?;

        // Keep at most two STL writers active; consume errors in stable output-rank order. Pairs of
        // writers pay one pool fork/join per two files, which only a large file amortises: on a split
        // of ~3,700 small granules it cost more than the writing (0.074 s serial against 0.222 s on 8
        // workers, PLAN.Performance.md section 72), so small outputs are written one at a time.
        let kept_faces: usize = kept_indices.iter().map(|&i| particles[i].faces.len()).sum();
        let parallel_writes = kept_faces / kept_indices.len() >= WRITE_PAIR_MIN_FACES;
        let write_rank = |rank: usize, idx: usize| {
            let out_path = output_folder.join(format!("{}{num}.stl", prefix, num = rank + 1));
            save_stl(&out_path, &particles[idx], "split_filter_particle")
        };
        if parallel_writes {
            for (batch, indices) in kept_indices.chunks(2).enumerate() {
                let results: Vec<Result<()>> = indices
                    .par_iter()
                    .enumerate()
                    .map(|(offset, &idx)| write_rank(batch * 2 + offset, idx))
                    .collect();
                for result in results { result?; }
            }
        } else {
            for (rank, &idx) in kept_indices.iter().enumerate() {
                write_rank(rank, idx)?;
            }
        }
        timer.stage("write_stl");

        report_lines.push("".to_string());
        report_lines.push("Summary:".to_string());
        report_lines.push(format!("Kept particles: {}", kept_indices.len()));
        report_lines.push(format!(
            "Removed particles: {}",
            particles.len().saturating_sub(kept_indices.len())
        ));

        if let Some(stats) = volume_stats_for_kept(&volumes, &keep) {
            report_lines.push("Kept volume statistics: ".to_string());
            report_lines.push(format!("  min: {:.6}", stats.min));
            report_lines.push(format!("  max: {:.6}", stats.max));
            report_lines.push(format!("  mean: {:.6}", stats.mean));
            report_lines.push(format!("  median: {:.6}", stats.median));
        }

        let kept_volumes: Vec<f64> = volumes
            .iter()
            .zip(keep.iter())
            .filter_map(|(v, k)| if *k { Some(*v) } else { None })
            .collect();
        report_lines.push("".to_string());
        append_volume_histogram(&mut report_lines, "Kept volume histogram (10 bins):", &kept_volumes, 10);
        report_lines.push("".to_string());
        append_volume_histogram_comparison(
            &mut report_lines,
            "Volume histogram comparison (before vs after, 10 bins):",
            before_volumes,
            &kept_volumes,
            10,
        );

        if let Some(parent) = report_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&report_path, report_lines.join("\n"))?;
        timer.stage("write_report");

        println!(
            "[Info] Split filter completed: input particles={}, kept={}, removed={}",
            particles.len(),
            kept_indices.len(),
            particles.len().saturating_sub(kept_indices.len())
        );
        println!("[Info] Output folder: {}", output_folder.display());
        println!("[Info] Output prefix: {}", prefix);
        println!("[Info] Report written: {}", report_path.display());
        timer.total("total_in_pool");
        timer.report_resources();

        Ok(())
    }
}

#[cfg(test)]
mod metric_tests {
    use super::*;
    use crate::geometry::box_mesh;
    use crate::types::{BoundingBox, Vec3};

    // AI-FUNC-SUMMARY: Check ordered cached metrics against serial geometry reductions across task counts and cheap rejections.
    #[test]
    fn cached_metrics_preserve_serial_values_and_order() {
        let particles: Vec<_> = (0..67).map(|i| box_mesh(BoundingBox {
            min: Vec3::new(i as f64 * 20.0, 0.0, 0.0),
            max: Vec3::new(i as f64 * 20.0 + 1.0 + (i%5) as f64, 2.0, 3.0),
        })).collect();
        for count in [1, 31, 32, 67] {
            for workers in [1, 2, 8] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                for max_aspect in [None, Some(2.0), Some(100.0)] {
                    for need_area in [false, true] {
                        let metrics = pool.install(|| prepare_particle_metrics(&particles[..count], max_aspect, need_area));
                        for (mesh, metric) in particles.iter().zip(metrics) {
                            assert_eq!(metric.volume.to_bits(), mesh_volume(mesh).to_bits());
                            let bbox = mesh_bbox(mesh).unwrap().size();
                            let aspect = bbox.x.max(bbox.y).max(bbox.z) / bbox.x.min(bbox.y).min(bbox.z).max(1e-12);
                            assert_eq!(metric.aspect, max_aspect.map(|_| aspect));
                            let expected = (need_area && !max_aspect.is_some_and(|limit| aspect > limit)).then(|| mesh_surface_area(mesh));
                            assert_eq!(metric.area, expected);
                        }
                    }
                }
            }
        }
    }
    // AI-FUNC-SUMMARY: Record serial-versus-prepared metric timings for tiny components and one large component, checking exact values.
    #[test]
    #[ignore = "release performance measurement"]
    fn component_metrics_benchmark() {
        use std::time::Instant;
        for (label, particles) in [
            ("many_small", (0..8192).map(|i| box_mesh(BoundingBox {
                min: Vec3::new(i as f64 * 10.0, 0.0, 0.0),
                max: Vec3::new(i as f64 * 10.0 + 1.0, 2.0, 3.0),
            })).collect::<Vec<_>>()),
            ("one_large", vec![crate::geometry::icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 6)]),
        ] {
            for workers in [1, 2, 8] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                for repeat in 0..6 {
                    pool.install(|| {
                        let start = Instant::now();
                        let expected: Vec<_> = particles.iter().map(|mesh| {
                            let volume = mesh_volume(mesh);
                            let size = mesh_bbox(mesh).unwrap().size();
                            let aspect = size.x.max(size.y).max(size.z) / size.x.min(size.y).min(size.z).max(1e-12);
                            (volume, aspect, mesh_surface_area(mesh))
                        }).collect();
                        let serial = start.elapsed().as_secs_f64();
                        let start = Instant::now();
                        let actual = prepare_particle_metrics(&particles, Some(100.0), true);
                        let prepared = start.elapsed().as_secs_f64();
                        for (a, b) in actual.iter().zip(expected) {
                            assert_eq!((a.volume, a.aspect.unwrap(), a.area.unwrap()), b);
                        }
                        println!("split_metrics case={label} workers={workers} repeat={repeat} serial_seconds={serial:.9} prepared_seconds={prepared:.9}");
                    });
                }
            }
        }
    }

}
