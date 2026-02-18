use crate::config::{parse_box_dimensions, MeasurementConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    calculate_s2, mesh_bbox, split_mesh_into_granules, volume_fraction_in_bbox,
};
use crate::io::load_stl_or_merge_folder;
use crate::pipeline::Pipeline;
use crate::types::BoundingBox;
use rayon::ThreadPoolBuilder;
use std::fs;
use std::path::Path;

pub struct MeasurePipeline {
    pub config: MeasurementConfig,
}

impl MeasurePipeline {
    /// Parse optional bbox from config, treating empty vectors as unset.
    /// Input: optional vector from config. Output: optional parsed bbox.
    fn parse_optional_bbox(values: &Option<Vec<f64>>) -> Result<Option<BoundingBox>> {
        match values {
            None => Ok(None),
            Some(v) if v.is_empty() => Ok(None),
            Some(v) => Ok(Some(parse_box_dimensions(v)?)),
        }
    }

    fn l2_error(a: &[f64], b: &[f64]) -> f64 {
        // Purpose: Compute L2 distance between two S2 vectors.
        // Inputs: two S2 arrays.
        // Outputs: non-negative L2 error.
        let n = a.len().min(b.len());
        if n == 0 {
            return 0.0;
        }
        let sum_sq = (0..n)
            .map(|i| {
                let d = a[i] - b[i];
                d * d
            })
            .sum::<f64>();
        sum_sq.sqrt()
    }
}

impl Pipeline for MeasurePipeline {
    fn run(&self) -> Result<()> {
        // Purpose: Execute measurement pipeline and write VF/S2 report.
        // Inputs: measurement config and STL input source.
        // Outputs: report file and runtime diagnostics.
        let params = &self.config.measurement;

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

        let mesh = load_stl_or_merge_folder(Path::new(&params.stl_path))?;

        let user_bbox = Self::parse_optional_bbox(&params.bounding_box)?;
        let stl_bbox = Self::parse_optional_bbox(&params.stl_bounding_box)?;
        let bbox = user_bbox
            .or(stl_bbox)
            .or_else(|| mesh_bbox(&mesh))
            .unwrap_or(BoundingBox::from_size(crate::types::Vec3::new(1.0, 1.0, 1.0)));

        println!("[Info] STL file(s) loaded from: {}", params.stl_path);
        println!(
            "[Info] Measurement bbox: min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4})",
            bbox.min.x, bbox.min.y, bbox.min.z, bbox.max.x, bbox.max.y, bbox.max.z
        );

        let particle_count = split_mesh_into_granules(&mesh).len();
        let vf = volume_fraction_in_bbox(&mesh, bbox);

        let method_raw = params.mc_method.trim();
        let requested_method = if method_raw.eq_ignore_ascii_case("exact") {
            "exact"
        } else if method_raw.eq_ignore_ascii_case("both") {
            "both"
        } else {
            "monte_carlo"
        };
        println!(
            "[Info] S2 config: method={}, r_max={}, mc_samples={}, voxel_pitch={:.6}",
            requested_method,
            params.r_max,
            params.mc_samples.unwrap_or(10_000),
            params.voxel_pitch
        );

        let size = bbox.size();
        let pitch = params.voxel_pitch.max(1e-9);
        let nx = (size.x / pitch).ceil().max(1.0) as u64;
        let ny = (size.y / pitch).ceil().max(1.0) as u64;
        let nz = (size.z / pitch).ceil().max(1.0) as u64;
        let voxel_count = nx.saturating_mul(ny).saturating_mul(nz);
        let exact_voxel_limit: u64 = 1_500_000;
        let mut output = String::new();
        output.push_str(&format!("Volume Fraction: {vf:.6}\n"));

        let mut summary_lines: Vec<String> = Vec::new();

        if requested_method == "both" {
            if voxel_count > exact_voxel_limit {
                println!(
                    "[Warning] both requested but exact voxel grid too large ({voxel_count}); only monte_carlo will run."
                );
                let s2_monte = thread_pool.install(|| {
                    calculate_s2(
                        &mesh,
                        bbox,
                        params.r_max,
                        params.voxel_pitch,
                        "monte_carlo",
                        params.mc_samples.unwrap_or(10_000),
                    )
                });
                output.push_str("Method: monte_carlo (exact skipped by voxel limit)\n");
                output.push_str(&format!("S2(0)-VF diff [monte_carlo]: {:.6}\n", (s2_monte[0] - vf).abs()));
                output.push_str("S2 Values [monte_carlo]:\n");
                for (idx, value) in s2_monte.iter().enumerate() {
                    output.push_str(&format!("{idx}: {value:.6}\n"));
                }
                summary_lines.push("[Info] Method: monte_carlo (exact skipped by voxel limit)".to_string());
                summary_lines.push(format!("[Info] S2(0)-VF diff [monte_carlo]: {:.6}", (s2_monte[0] - vf).abs()));
                summary_lines.push(format!("[Info] S2 points [monte_carlo]: {}", s2_monte.len()));
            } else {
                let s2_exact = thread_pool.install(|| {
                    calculate_s2(
                        &mesh,
                        bbox,
                        params.r_max,
                        params.voxel_pitch,
                        "exact",
                        params.mc_samples.unwrap_or(10_000),
                    )
                });
                let s2_monte = thread_pool.install(|| {
                    calculate_s2(
                        &mesh,
                        bbox,
                        params.r_max,
                        params.voxel_pitch,
                        "monte_carlo",
                        params.mc_samples.unwrap_or(10_000),
                    )
                });
                let l2 = Self::l2_error(&s2_exact, &s2_monte);

                output.push_str("Method: both\n");
                output.push_str(&format!("S2(0)-VF diff [exact]: {:.6}\n", (s2_exact[0] - vf).abs()));
                output.push_str(&format!("S2(0)-VF diff [monte_carlo]: {:.6}\n", (s2_monte[0] - vf).abs()));
                output.push_str(&format!("L2 error [exact vs monte_carlo]: {:.6}\n", l2));

                output.push_str("S2 Values [exact]:\n");
                for (idx, value) in s2_exact.iter().enumerate() {
                    output.push_str(&format!("{idx}: {value:.6}\n"));
                }

                output.push_str("S2 Values [monte_carlo]:\n");
                for (idx, value) in s2_monte.iter().enumerate() {
                    output.push_str(&format!("{idx}: {value:.6}\n"));
                }

                summary_lines.push("[Info] Method: both".to_string());
                summary_lines.push(format!("[Info] S2(0)-VF diff [exact]: {:.6}", (s2_exact[0] - vf).abs()));
                summary_lines.push(format!("[Info] S2(0)-VF diff [monte_carlo]: {:.6}", (s2_monte[0] - vf).abs()));
                summary_lines.push(format!("[Info] L2 error [exact vs monte_carlo]: {:.6}", l2));
                summary_lines.push(format!("[Info] S2 points [exact]: {}", s2_exact.len()));
                summary_lines.push(format!("[Info] S2 points [monte_carlo]: {}", s2_monte.len()));
            }
        } else {
            let method = if requested_method == "exact" && voxel_count > exact_voxel_limit {
                println!(
                    "[Warning] Exact method requested but voxel grid too large ({voxel_count}). Falling back to monte_carlo."
                );
                "monte_carlo"
            } else {
                requested_method
            };

            let s2 = thread_pool.install(|| {
                calculate_s2(
                    &mesh,
                    bbox,
                    params.r_max,
                    params.voxel_pitch,
                    method,
                    params.mc_samples.unwrap_or(10_000),
                )
            });

            output.push_str(&format!("Method: {method}\n"));
            output.push_str(&format!("S2(0)-VF diff: {:.6}\n", (s2[0] - vf).abs()));
            output.push_str("S2 Values:\n");
            for (idx, value) in s2.iter().enumerate() {
                output.push_str(&format!("{idx}: {value:.6}\n"));
            }

            summary_lines.push(format!("[Info] Method: {method}"));
            summary_lines.push(format!("[Info] S2(0)-VF diff: {:.6}", (s2[0] - vf).abs()));
            summary_lines.push(format!("[Info] S2 points: {}", s2.len()));
        }

        if let Some(parent) = Path::new(&params.output_path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&params.output_path, output)?;

        println!("[Info] Measurement completed.");
        println!("[Info] Volume fraction: {vf:.6}");
        println!("[Info] VF detail: particles={} (in-box clipped-volume sum)", particle_count);
        for line in summary_lines {
            println!("{line}");
        }
        println!("[Info] Output written: {}", params.output_path);
        Ok(())
    }
}
