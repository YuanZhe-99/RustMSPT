use crate::compute::policy::{configured_mode, resolve_execution};
use crate::config::{parse_box_dimensions, MeasurementConfig};
use crate::error::{Result, RustMsptError};
use crate::geometry::{calculate_s2, mesh_bbox, split_mesh_into_granules, volume_fraction_in_bbox};
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
    // Notes: Supports exact/MC/both under one worker pool; exact over the safety limit returns an error without substituting MC.
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

impl MeasurePipeline {
    // AI-FUNC-SUMMARY: Measure under the installed worker pool with method-specific backend decisions and same-method runtime fallback; write results only after all requested methods succeed.
    fn run_in_pool(&self) -> Result<()> {
        let params = &self.config.measurement;
        if !params.voxel_pitch.is_finite() || params.r_max == usize::MAX {
            return Err(RustMsptError::InvalidConfig(
                "measurement requires finite pitch and representable radius count".into(),
            ));
        }
        let requested = configured_mode(&params.acceleration)?;
        let mesh = load_stl_or_merge_folder(Path::new(&params.stl_path))?;

        let user_bbox = Self::parse_optional_bbox(&params.bounding_box)?;
        let stl_bbox = Self::parse_optional_bbox(&params.stl_bounding_box)?;
        let bbox = user_bbox
            .or(stl_bbox)
            .or_else(|| mesh_bbox(&mesh))
            .unwrap_or(BoundingBox::from_size(crate::types::Vec3::new(
                1.0, 1.0, 1.0,
            )));

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

        let pitch = if params.voxel_pitch <= 0.0 {
            1.0
        } else {
            params.voxel_pitch
        };
        let size = bbox.size();
        let mut voxel_count = 1usize;
        for extent in [size.x, size.y, size.z] {
            let n = (extent / pitch).ceil().max(1.0);
            if !extent.is_finite() || extent <= 0.0 || !n.is_finite() || n >= usize::MAX as f64 {
                return Err(RustMsptError::InvalidConfig(
                    "measurement grid dimensions are unsupported".into(),
                ));
            }
            voxel_count = voxel_count.checked_mul(n as usize).ok_or_else(|| {
                RustMsptError::InvalidConfig("measurement grid count overflow".into())
            })?;
        }
        if requested_method != "monte_carlo" && voxel_count > 1_500_000 {
            return Err(RustMsptError::InvalidConfig("exact measurement exceeds the current 1,500,000-cell safety limit; increase voxel_pitch (exact is never silently replaced by MC)".into()));
        }
        let methods: &[&str] = if requested_method == "both" {
            &["exact", "monte_carlo"]
        } else {
            &[requested_method]
        };
        let mut curves = Vec::new();
        let mut backends = Vec::new();
        let mut cpu_voxels = None;
        for &method in methods {
            let samples = params.mc_samples.unwrap_or(10_000);
            let estimated = if method == "exact" {
                crate::compute::exact_memory::ExactMemoryPlan::new(mesh.faces.len(), voxel_count, params.acceleration.gpu_memory_limit_mb).ok().map(|plan| plan.peak_bytes)
            } else {
                crate::compute::mc_memory::mc_evaluation_peak(
                    4, 4, 0, mesh.faces.len(), params.r_max, samples,
                ).ok()
            };
            let supports_gpu =
                method == "exact" || (params.voxel_pitch <= 0.0 && params.r_max < 128);
            let selection = resolve_execution(
                &params.acceleration,
                requested,
                voxel_count,
                params.acceleration.gpu_min_voxels,
                supports_gpu,
                estimated,
            )?;
            let mut backend = selection.backend.to_string();
            if let Some(reason) = &selection.fallback {
                eprintln!("[Info] {method}: {}", reason.reason);
            }
            #[cfg(feature = "gpu")]
            let attempted = if selection.backend.is_gpu() {
                Some(if method == "exact" {
                    crate::geometry::s2::try_calculate_s2_gpu_exact_limited(
                        &mesh,
                        bbox,
                        params.r_max,
                        pitch,
                        params.acceleration.gpu_memory_limit_mb,
                    )
                } else {
                    crate::gpu::GpuS2Pipeline::new(&mesh, bbox)
                        .and_then(|mut gpu| gpu.calculate_s2_gpu(bbox, params.r_max, samples))
                        .map(|mut s2| {
                            s2[0] = vf;
                            s2
                        })
                })
            } else {
                None
            };
            #[cfg(not(feature = "gpu"))]
            let attempted: Option<std::result::Result<Vec<f64>, String>> = None;
            let curve = match attempted {
                Some(Ok(curve)) => curve,
                Some(Err(error)) if !params.acceleration.cpu_fallback => {
                    return Err(RustMsptError::Gpu(format!("measure {method}: {error}")))
                }
                other => {
                    if let Some(Err(error)) = other {
                        eprintln!(
                            "[Warning] measure {method}: {error}; falling back to CPU {method}"
                        );
                    }
                    backend = "cpu".into();
                    if method == "exact" || params.voxel_pitch > 0.0 {
                        let prepared = cpu_voxels.get_or_insert_with(|| {
                            println!("[Info] Preparing shared CPU S2 occupancy grid");
                            crate::geometry::s2::VoxelS2::new(&mesh, bbox, pitch)
                        });
                        prepared.calculate(params.r_max, method, samples)
                    } else {
                        calculate_s2(&mesh, bbox, params.r_max, params.voxel_pitch, method, samples)
                    }
                }
            };
            println!("[Info] Measure execution: method={method}, backend={backend}, workers={}, worker_index={:?}", rayon::current_num_threads(), rayon::current_thread_index());
            backends.push(format!("{method}={backend}"));
            curves.push(curve);
        }
        let mut output = format!(
            "Volume Fraction: {vf:.6}\nCompute Backend: {}\nMethod: {requested_method}\n",
            backends.join(", ")
        );
        for (method, curve) in methods.iter().zip(&curves) {
            output.push_str(&format!(
                "S2(0)-VF diff [{method}]: {:.6}\n",
                (curve[0] - vf).abs()
            ));
            output.push_str(if methods.len() == 1 {
                "S2 Values:\n"
            } else if *method == "exact" {
                "S2 Values [exact]:\n"
            } else {
                "S2 Values [monte_carlo]:\n"
            });
            for (r, value) in curve.iter().enumerate() {
                output.push_str(&format!("{r}: {value:.6}\n"));
            }
        }
        if curves.len() == 2 {
            output.push_str(&format!(
                "L2 error [exact vs monte_carlo]: {:.6}\n",
                Self::l2_error(&curves[0], &curves[1])
            ));
        }
        if let Some(parent) = Path::new(&params.output_path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&params.output_path, output)?;
        println!(
            "[Info] Measurement completed: {particle_count} particles, VF {vf:.6}; output {}",
            params.output_path
        );
        Ok(())
    }
}
