use crate::compute::policy::select_backend;
use crate::config::{parse_box_dimensions, MeasurementConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    calculate_s2, mesh_bbox, split_mesh_into_granules, volume_fraction_in_bbox,
};
#[cfg(feature = "gpu")]
use crate::geometry::{calculate_s2_with_gpu, calculate_s2_gpu_exact};
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
    // AI-FUNC-SUMMARY: Parse optional bounding box from config (3-element size or 6-element min/max, empty vec treated as unset); returns Option<BoundingBox>; side effects: None.
    fn parse_optional_bbox(values: &Option<Vec<f64>>) -> Result<Option<BoundingBox>> {
        match values {
            None => Ok(None),
            Some(v) if v.is_empty() => Ok(None),
            Some(v) => Ok(Some(parse_box_dimensions(v)?)),
        }
    }

    // AI-FUNC-SUMMARY: Compute L2 distance between two S2 vectors over their common length prefix; returns f64 (0.0 for empty); side effects: None.
    fn l2_error(a: &[f64], b: &[f64]) -> f64 {
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
    // AI-FUNC-SUMMARY:
    // Purpose: Execute measurement pipeline: load STL, compute volume fraction and S2 correlation, write report.
    // Inputs: MeasurementConfig with stl_path, bounding box, S2 method/samples/pitch, and output path.
    // Returns: Ok(()) or error.
    // Side effects: Reads STL from disk; writes measurement report to disk; prints summary to stdout.
    // Notes: Supports "exact", "monte_carlo", or "both" methods. Falls back from exact to MC when voxel grid is too large (>1.5M voxels). Reports compute backend used.
    fn run(&self) -> Result<()> {
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
        let pitch = if params.voxel_pitch <= 0.0 {
            1.0
        } else {
            params.voxel_pitch
        };
        let nx = (size.x / pitch).ceil().max(1.0) as u64;
        let ny = (size.y / pitch).ceil().max(1.0) as u64;
        let nz = (size.z / pitch).ceil().max(1.0) as u64;
        let voxel_count = nx.saturating_mul(ny).saturating_mul(nz);

        let accel = &params.acceleration;
        let selection = select_backend(
            accel.mode,
            Some(accel.gpu_min_voxels),
            accel.gpu_memory_limit_mb,
            voxel_count as usize,
        );
        println!(
            "[Info] Acceleration: requested={}, effective={}",
            accel.mode, selection.backend
        );
        if let Some(ref fb) = selection.fallback {
            println!("[Info] Acceleration fallback: {}", fb.reason);
        }

        let exact_voxel_limit: u64 = 1_500_000;
        let mut output = String::new();
        output.push_str(&format!("Volume Fraction: {vf:.6}\n"));
        output.push_str(&format!("Compute Backend: {}\n", selection.backend));

        let mut summary_lines: Vec<String> = Vec::new();
        summary_lines.push(format!("[Info] Compute backend: {}", selection.backend));

        let use_gpu = selection.backend.is_gpu();

        #[cfg(feature = "gpu")]
        let mut gpu_pipeline = if use_gpu {
            match crate::gpu::s2::GpuS2Pipeline::new(&mesh, bbox) {
                Ok(p) => {
                    println!("[Info] GPU S2 pipeline initialized for Monte Carlo");
                    Some(p)
                }
                Err(e) => {
                    println!("[Warning] GPU S2 pipeline init failed: {e}, falling back to CPU");
                    None
                }
            }
        } else {
            None
        };

        #[cfg(not(feature = "gpu"))]
        let mut _gpu_pipeline: Option<()> = if use_gpu {
            println!("[Warning] GPU requested but cargo feature 'gpu' not enabled; using CPU");
            None
        } else {
            None
        };

        if requested_method == "both" {
            if voxel_count > exact_voxel_limit {
                println!(
                    "[Warning] both requested but exact voxel grid too large ({voxel_count}); only monte_carlo will run."
                );
                #[cfg(feature = "gpu")]
                let s2_monte = if let Some(ref mut gpu) = gpu_pipeline {
                    calculate_s2_with_gpu(
                        &mesh, bbox, params.r_max, params.voxel_pitch,
                        "monte_carlo", params.mc_samples.unwrap_or(10_000), Some(gpu),
                    )
                } else {
                    thread_pool.install(|| {
                        calculate_s2(&mesh, bbox, params.r_max, params.voxel_pitch, "monte_carlo", params.mc_samples.unwrap_or(10_000))
                    })
                };
                #[cfg(not(feature = "gpu"))]
                let s2_monte = thread_pool.install(|| {
                    calculate_s2(&mesh, bbox, params.r_max, params.voxel_pitch, "monte_carlo", params.mc_samples.unwrap_or(10_000))
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
                #[cfg(feature = "gpu")]
                let s2_exact = if use_gpu {
                    calculate_s2_gpu_exact(&mesh, bbox, params.r_max, params.voxel_pitch)
                } else {
                    thread_pool.install(|| {
                        calculate_s2(&mesh, bbox, params.r_max, params.voxel_pitch, "exact", params.mc_samples.unwrap_or(10_000))
                    })
                };
                #[cfg(not(feature = "gpu"))]
                let s2_exact = thread_pool.install(|| {
                    calculate_s2(&mesh, bbox, params.r_max, params.voxel_pitch, "exact", params.mc_samples.unwrap_or(10_000))
                });

                #[cfg(feature = "gpu")]
                let s2_monte = if let Some(ref mut gpu) = gpu_pipeline {
                    calculate_s2_with_gpu(
                        &mesh, bbox, params.r_max, params.voxel_pitch,
                        "monte_carlo", params.mc_samples.unwrap_or(10_000), Some(gpu),
                    )
                } else {
                    thread_pool.install(|| {
                        calculate_s2(&mesh, bbox, params.r_max, params.voxel_pitch, "monte_carlo", params.mc_samples.unwrap_or(10_000))
                    })
                };
                #[cfg(not(feature = "gpu"))]
                let s2_monte = thread_pool.install(|| {
                    calculate_s2(&mesh, bbox, params.r_max, params.voxel_pitch, "monte_carlo", params.mc_samples.unwrap_or(10_000))
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

            #[cfg(feature = "gpu")]
            let s2 = if method == "monte_carlo" && gpu_pipeline.is_some() {
                calculate_s2_with_gpu(
                    &mesh, bbox, params.r_max, params.voxel_pitch,
                    method, params.mc_samples.unwrap_or(10_000), gpu_pipeline.as_mut(),
                )
            } else if method == "exact" && use_gpu {
                calculate_s2_gpu_exact(&mesh, bbox, params.r_max, params.voxel_pitch)
            } else {
                thread_pool.install(|| {
                    calculate_s2(&mesh, bbox, params.r_max, params.voxel_pitch, method, params.mc_samples.unwrap_or(10_000))
                })
            };
            #[cfg(not(feature = "gpu"))]
            let s2 = thread_pool.install(|| {
                calculate_s2(&mesh, bbox, params.r_max, params.voxel_pitch, method, params.mc_samples.unwrap_or(10_000))
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
