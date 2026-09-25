use crate::compute::backend::{AccelerationMode, ComputeBackend};
use crate::compute::policy::{select_backend, BackendSelection, FallbackReason};
use crate::config::OptimizationParams;
use crate::error::{Result, RustMsptError};
use crate::geometry::calculate_s2_seeded;
use crate::geometry::s2::{VoxelCoverage, VoxelS2};
use crate::types::{BoundingBox, Mesh};
use rayon::prelude::*;
#[cfg(any(feature = "gpu", test))]
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum S2Method {
    VoxelExact,
    VoxelMc,
    MeshMc,
}

impl S2Method {
    // AI-FUNC-SUMMARY: Resolve the existing exact/non-exact and pitch semantics once; returns a named S2 method; no side effects.
    fn resolve(params: &OptimizationParams) -> Self {
        if params.mc_method == "exact" {
            Self::VoxelExact
        } else if params.voxel_pitch <= 0.0 {
            Self::MeshMc
        } else {
            Self::VoxelMc
        }
    }

    // AI-FUNC-SUMMARY: Return the actual S2 definition for execution/history diagnostics; no side effects.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::VoxelExact => "voxel_exact",
            Self::VoxelMc => "voxel_mc",
            Self::MeshMc => "mesh_mc",
        }
    }
}

// AI-FUNC-SUMMARY: Resolve an optional environment override above YAML mode; rejects unknown values; no side effects.
fn resolve_mode(
    configured: AccelerationMode,
    override_value: Option<&str>,
) -> Result<AccelerationMode> {
    match override_value {
        None => Ok(configured),
        Some("cpu") => Ok(AccelerationMode::Cpu),
        Some("gpu") => Ok(AccelerationMode::Gpu),
        Some("auto") => Ok(AccelerationMode::Auto),
        Some(value) => Err(RustMsptError::InvalidConfig(format!(
            "Invalid RUSTMSPT_ACCELERATION={value:?}; expected cpu, gpu or auto"
        ))),
    }
}

// AI-FUNC-SUMMARY: Select only a method-compatible backend, checking CPU/auto gates before the supplied GPU probe; returns selection or forbidden-fallback/config error; may invoke probe once.
fn select_s2_backend(
    params: &OptimizationParams,
    requested: AccelerationMode,
    workload: usize,
    faces: usize,
    probe: impl FnOnce() -> BackendSelection,
) -> Result<BackendSelection> {
    let accel = &params.acceleration;
    if requested == AccelerationMode::Cpu {
        return Ok(BackendSelection {
            backend: ComputeBackend::Cpu,
            fallback: None,
        });
    }
    if requested == AccelerationMode::Auto && workload < accel.gpu_min_voxels {
        return Ok(BackendSelection {
            backend: ComputeBackend::Cpu,
            fallback: Some(FallbackReason {
                requested,
                reason: format!(
                    "workload {workload} voxels below gpu_min_voxels threshold {}",
                    accel.gpu_min_voxels
                ),
            }),
        });
    }
    let method = S2Method::resolve(params);
    let max_samples = params
        .mc_samples
        .max(2000)
        .max(params.prune_eval_samples.unwrap_or(0));
    let reason = if method != S2Method::MeshMc {
        Some(format!(
            "{} has no compatible optimizer GPU evaluator; retaining its CPU definition",
            method.name()
        ))
    } else if accel.backend != "wgpu" || accel.gpu_precision != "f32" || accel.gpu_prefer_power {
        return Err(RustMsptError::InvalidConfig(
            "optimizer GPU supports only backend=wgpu, gpu_precision=f32 and gpu_prefer_power=false".into()
        ));
    } else if let Err(error) =
        crate::compute::mc_memory::mc_evaluation_peak_batched(4, 4, 0, 0, faces, params.r_max, max_samples, 1)
            .and_then(|peak| {
                crate::compute::mc_memory::check_mc_budget(peak, accel.gpu_memory_limit_mb)
            })
    {
        Some(error)
    } else if params
            .r_max
            .checked_add(1)
            .and_then(|n| n.checked_mul(max_samples))
            .is_none_or(|n| n > u32::MAX as usize)
    {
        Some("optimizer GPU MC logical sample capacity exceeded; retaining the mesh_mc CPU evaluator".into())
    } else {
        None
    };
    let selection = if let Some(reason) = reason {
        BackendSelection {
            backend: ComputeBackend::Cpu,
            fallback: Some(FallbackReason { requested, reason }),
        }
    } else {
        probe()
    };
    if !selection.backend.is_gpu() && !accel.cpu_fallback {
        return Err(RustMsptError::Gpu(format!(
            "optimizer CPU fallback is disabled: {}",
            selection
                .fallback
                .as_ref()
                .map(|r| r.reason.as_str())
                .unwrap_or("GPU unavailable")
        )));
    }
    Ok(selection)
}

/// Maintain voxel occupancy incrementally (per-voxel coverage counts) during SA for voxel methods
/// instead of re-voxelizing the merged population for every candidate. Exact by construction and
/// pinned by `incremental_coverage_matches_full_voxelization`; switch it off to fall back to full
/// voxelization.
pub(super) const INCREMENTAL_VOXEL_OCCUPANCY: bool = true;

pub(super) struct OptimizeS2 {
    pub(super) method: S2Method,
    pitch: f64,
    pub(super) description: String,
    #[cfg(feature = "gpu")]
    gpu: Option<Mutex<Option<crate::gpu::s2::GpuS2Pipeline>>>,
    #[cfg(feature = "gpu")]
    cpu_fallback: bool,
    #[cfg(feature = "gpu")]
    gpu_memory_limit_mb: Option<u64>,
    #[cfg(test)]
    pub(super) observations: Mutex<Vec<(&'static str, usize, Option<usize>)>>,
}

impl OptimizeS2 {
    // AI-FUNC-SUMMARY: Resolve mode/method once and create at most one persistent GPU MC instance for the whole run; returns execution context or config/init error; reads environment and may initialize GPU.
    pub(super) fn new(params: &OptimizationParams, bbox: BoundingBox, mesh: &Mesh) -> Result<Self> {
        if !params.voxel_pitch.is_finite() {
            return Err(RustMsptError::InvalidConfig(
                "optimization.voxel_pitch must be finite".into(),
            ));
        }
        let override_value = std::env::var("RUSTMSPT_ACCELERATION")
            .map(Some)
            .or_else(|error| match error {
                std::env::VarError::NotPresent => Ok(None),
                _ => Err(RustMsptError::InvalidConfig(
                    "RUSTMSPT_ACCELERATION must be valid Unicode".into(),
                )),
            })?;
        let requested = resolve_mode(params.acceleration.mode, override_value.as_deref())?;
        let method = S2Method::resolve(params);
        let pitch = if method == S2Method::VoxelExact && params.voxel_pitch <= 0.0 {
            1.0
        } else {
            params.voxel_pitch
        };
        let size = bbox.size();
        let estimate_pitch = pitch.max(1e-9);
        let estimate_pitch = if method == S2Method::MeshMc {
            1.0
        } else {
            estimate_pitch
        };
        let workload = [size.x, size.y, size.z]
            .into_iter()
            .map(|side| (side / estimate_pitch).ceil().max(1.0) as usize)
            .fold(1usize, usize::saturating_mul);
        let selection = select_s2_backend(params, requested, workload, mesh.faces.len(), || {
            select_backend(
                requested,
                Some(params.acceleration.gpu_min_voxels),
                None,
                workload,
            )
        })?;
        #[allow(unused_mut)]
        let mut backend = selection.backend;
        #[allow(unused_mut)]
        let mut reason = selection.fallback.map(|f| f.reason);
        #[cfg(feature = "gpu")]
        let gpu = if backend.is_gpu() {
            match crate::gpu::s2::GpuS2Pipeline::new(&Mesh::empty(), bbox) {
                Ok(gpu) => Some(Mutex::new(Some(gpu))),
                Err(error) if params.acceleration.cpu_fallback => {
                    backend = ComputeBackend::Cpu;
                    reason = Some(format!("GPU S2 pipeline initialization failed: {error}"));
                    None
                }
                Err(error) => return Err(RustMsptError::Gpu(error)),
            }
        } else {
            None
        };
        #[cfg(not(feature = "gpu"))]
        let _ = mesh;
        let description = format!("S2 execution: requested={requested}, effective={backend}, method={}, voxel_pitch={pitch}, reason={}",
            method.name(), reason.as_deref().unwrap_or("none"));
        Ok(Self {
            method,
            pitch,
            description,
            #[cfg(feature = "gpu")]
            gpu,
            #[cfg(feature = "gpu")]
            cpu_fallback: params.acceleration.cpu_fallback,
            #[cfg(feature = "gpu")]
            gpu_memory_limit_mb: params.acceleration.gpu_memory_limit_mb,
            #[cfg(test)]
            observations: Mutex::new(Vec::new()),
        })
    }

    // AI-FUNC-SUMMARY: Describe the shared GPU MC instance's cumulative f32 certification counters (CPU recompute ratio) while it is still active; returns None on CPU or after a fallback removed it; side effects: briefly locks the GPU mutex.
    pub(super) fn gpu_certification_summary(&self) -> Option<String> {
        #[cfg(feature = "gpu")]
        if let Some(gpu) = &self.gpu {
            return gpu
                .lock()
                .ok()
                .and_then(|state| state.as_ref().map(|gpu| gpu.certification_stats().describe()));
        }
        None
    }

    // AI-FUNC-SUMMARY: Evaluate one stage using the fixed method and active Rayon budget; serializes GPU buffer use and releases its lock before parallel VF work; returns S2 or a policy-forbidden GPU error and records test-only execution observations.
    pub(super) fn evaluate(
        &self,
        mesh: &Mesh,
        bbox: BoundingBox,
        r_max: usize,
        samples: usize,
        _stage: &'static str,
        seed: Option<u64>,
    ) -> Result<Vec<f64>> {
        self.evaluate_with_vf(mesh, bbox, r_max, samples, _stage, None, seed)
    }

    // AI-FUNC-SUMMARY: Evaluate the fixed S2 definition with optional cached geometric VF for mesh MC only; preserve GPU fallback and perform no cached-volume work under its mutex.
#[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_with_vf(&self, mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize, _stage: &'static str, vf: Option<f64>, seed: Option<u64>) -> Result<Vec<f64>> {
        if vf.is_some() && self.method != S2Method::MeshMc {
            return Err(RustMsptError::InvalidConfig("geometric VF cache cannot replace voxel S2 volume fraction".into()));
        }
        #[cfg(test)]
        self.observations.lock().unwrap().push((
            _stage,
            rayon::current_num_threads(),
            rayon::current_thread_index(),
        ));
        #[cfg(feature = "gpu")]
        if let Some(gpu) = &self.gpu {
            let attempt = {
                let mut state = gpu
                    .lock()
                    .map_err(|_| RustMsptError::Gpu("GPU S2 mutex poisoned".into()))?;
                let result = state.as_mut().map(|gpu| {
                    gpu.set_memory_limit_mb(self.gpu_memory_limit_mb);
                    gpu.check_evaluation_budget(mesh, r_max, samples, self.gpu_memory_limit_mb)
                        .and_then(|()| gpu.update_mesh(mesh, bbox))
                        .and_then(|()| gpu.calculate_s2_gpu_seeded(bbox, r_max, samples, seed))
                });
                if self.cpu_fallback && result.as_ref().is_some_and(|r| r.is_err()) {
                    state.take();
                }
                result
            };
            match attempt {
                Some(Ok(mut result)) => {
                    result[0] = vf.unwrap_or_else(|| crate::geometry::volume_fraction_in_bbox(mesh, bbox));
                    return Ok(result);
                }
                Some(Err(error)) if !self.cpu_fallback => return Err(RustMsptError::Gpu(format!("S2 stage {_stage}: {error}"))),
                Some(Err(error)) => eprintln!("[Warning] S2 stage {_stage}: {error}; disabling GPU and falling back to CPU mesh_mc"),
                None => {}
            }
        }
        if let Some(vf) = vf {
            return Ok(crate::geometry::s2::calculate_s2_mesh_mc_seeded_with_vf(mesh, bbox, r_max, samples, seed.unwrap_or_else(rand::random), true, vf));
        }
        Ok(calculate_s2_seeded(
            mesh,
            bbox,
            r_max,
            self.pitch,
            if self.method == S2Method::VoxelExact {
                "exact"
            } else {
                "monte_carlo"
            },
            samples,
            seed,
        ))
    }

    // AI-FUNC-SUMMARY: Build island-local incremental voxel coverage when the fixed method is voxel based and the incremental path is enabled; returns None for mesh MC or when disabled; side effects: parallel containment queries.
    // Notes: Uses the evaluator's resolved pitch, so coverage.grid() is the same grid calculate_s2 would voxelize from the merged mesh.
    pub(super) fn voxel_coverage<'a>(&self, meshes: impl Iterator<Item = &'a Mesh>, bbox: BoundingBox) -> Option<VoxelCoverage> {
        (INCREMENTAL_VOXEL_OCCUPANCY && self.method != S2Method::MeshMc)
            .then(|| VoxelCoverage::new(meshes, bbox, self.pitch))
    }

    // AI-FUNC-SUMMARY: Evaluate the fixed voxel S2 definition on an already maintained occupancy grid; returns S2 or InvalidConfig for mesh MC; records the test-only execution observation.
    // Notes: Equivalent to calculate_s2 on the merged mesh with the same pitch and method, minus the voxelization.
    pub(super) fn evaluate_voxel_grid(&self, grid: &VoxelS2, r_max: usize, samples: usize, _stage: &'static str, seed: Option<u64>) -> Result<Vec<f64>> {
        if self.method == S2Method::MeshMc {
            return Err(RustMsptError::InvalidConfig("incremental voxel occupancy cannot evaluate mesh_mc S2".into()));
        }
        #[cfg(test)]
        self.observations.lock().unwrap().push((
            _stage,
            rayon::current_num_threads(),
            rayon::current_thread_index(),
        ));
        Ok(grid.calculate_seeded(
            r_max,
            if self.method == S2Method::VoxelExact { "exact" } else { "monte_carlo" },
            samples,
            seed,
        ))
    }
}

// AI-FUNC-SUMMARY: Execute all island IDs in ordered batches no larger than the current Rayon worker budget; clones/resources created by run stay bounded even with nested work stealing; returns results in island order.
pub(super) fn run_island_batches<T: Send>(
    islands: usize,
    run: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    let budget = rayon::current_num_threads();
    let mut results = Vec::new();
    for start in (0..islands).step_by(budget) {
        let batch: Vec<T> = (start..start.saturating_add(budget).min(islands))
            .into_par_iter()
            .map(&run)
            .collect();
        results.extend(batch);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "gpu")]
    use crate::geometry::calculate_s2;
    use crate::geometry::box_mesh;
    use crate::types::Vec3;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // AI-FUNC-SUMMARY: Build minimal optimizer parameters with a chosen S2 definition; no side effects.
    pub(crate) fn params(method: &str, pitch: f64) -> OptimizationParams {
        serde_json::from_value(serde_json::json!({
            "max_iterations": 8, "initial_temperature": 0.1, "cooling_rate": 0.99,
            "r_max": 0, "voxel_pitch": pitch, "mc_method": method, "mc_samples": 200,
            "max_translation": 0.0, "max_rotation_deg": 0.0, "prune_enabled": false,
            "acceleration": {"mode": "cpu"}
        }))
        .unwrap()
    }

    // AI-FUNC-SUMMARY: Verify CPU/auto/method gates never invoke a GPU probe and forbid only actual GPU fallback; no external side effects.
    #[test]
    fn backend_gates_preserve_methods_without_gpu_probe() {
        for method in ["exact", "monte_carlo", "both"] {
            for pitch in [-1.0, 0.0, 1.0] {
                let mut p = params(method, pitch);
                p.acceleration.cpu_fallback = false;
                assert!(
                    !select_s2_backend(&p, AccelerationMode::Cpu, usize::MAX, 0, || panic!(
                        "CPU probed GPU"
                    ))
                    .unwrap()
                    .backend
                    .is_gpu()
                );
                assert!(
                    !select_s2_backend(&p, AccelerationMode::Auto, 1, 0, || panic!(
                        "small auto probed GPU"
                    ))
                    .unwrap()
                    .backend
                    .is_gpu()
                );
                if S2Method::resolve(&p) != S2Method::MeshMc {
                    assert!(select_s2_backend(
                        &p,
                        AccelerationMode::Gpu,
                        usize::MAX,
                        0,
                        || panic!("voxel method probed GPU")
                    )
                    .is_err());
                    p.acceleration.cpu_fallback = true;
                    let selection =
                        select_s2_backend(&p, AccelerationMode::Gpu, usize::MAX, 0, || {
                            panic!("voxel method probed GPU")
                        })
                        .unwrap();
                    assert!(!selection.backend.is_gpu());
                    assert!(selection
                        .fallback
                        .unwrap()
                        .reason
                        .contains(S2Method::resolve(&p).name()));
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Validate the environment-mode precedence and explicit rejection of unknown values; no environment mutation.
    #[test]
    fn mode_override_is_explicit() {
        for configured in [
            AccelerationMode::Cpu,
            AccelerationMode::Auto,
            AccelerationMode::Gpu,
        ] {
            assert_eq!(resolve_mode(configured, None).unwrap(), configured);
            assert_eq!(
                resolve_mode(configured, Some("cpu")).unwrap(),
                AccelerationMode::Cpu
            );
            assert_eq!(
                resolve_mode(configured, Some("gpu")).unwrap(),
                AccelerationMode::Gpu
            );
            assert_eq!(
                resolve_mode(configured, Some("auto")).unwrap(),
                AccelerationMode::Auto
            );
            assert!(resolve_mode(configured, Some("cpui")).is_err());
        }
    }

    // AI-FUNC-SUMMARY: Exercise compatible GPU selection/probe failure and conservative capacity/config guards before device creation; no GPU allocation.
    #[test]
    fn mesh_mc_backend_capacity_and_fallback_contract() {
        let mut p = params("monte_carlo", 0.0);
        let calls = AtomicUsize::new(0);
        let probe = || {
            calls.fetch_add(1, Ordering::SeqCst);
            BackendSelection {
                backend: ComputeBackend::Cpu,
                fallback: Some(FallbackReason {
                    requested: AccelerationMode::Gpu,
                    reason: "injected init failure".into(),
                }),
            }
        };
        assert!(!select_s2_backend(&p, AccelerationMode::Gpu, 1, 0, probe)
            .unwrap()
            .backend
            .is_gpu());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        p.acceleration.cpu_fallback = false;
        assert!(select_s2_backend(&p, AccelerationMode::Gpu, 1, 0, probe)
            .unwrap_err()
            .to_string()
            .contains("injected init failure"));
        p.r_max = usize::MAX;
        assert!(
            select_s2_backend(&p, AccelerationMode::Gpu, 1, 0, || panic!(
                "capacity guard probed GPU"
            ))
            .is_err()
        );
        p.r_max = 300;
        assert!(select_s2_backend(&p, AccelerationMode::Gpu, 1, 0, probe)
            .unwrap_err()
            .to_string()
            .contains("injected init failure"));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        p.r_max = 1;
        p.prune_eval_samples = Some(usize::MAX);
        assert!(
            select_s2_backend(&p, AccelerationMode::Gpu, 1, 0, || panic!(
                "sample guard probed GPU"
            ))
            .is_err()
        );
        p.prune_eval_samples = None;
        p.acceleration.gpu_precision = "f64".into();
        assert!(
            select_s2_backend(&p, AccelerationMode::Gpu, 1, 0, || panic!(
                "unsupported precision probed GPU"
            ))
            .is_err()
        );
    }

    // AI-FUNC-SUMMARY: Verify the incremental voxel path gives the same exact S2 as full voxelization of the merged mesh for the resolved pitch (including the exact pitch<=0 fallback), and that mesh MC never builds coverage; no file output.
    #[test]
    fn incremental_voxel_grid_matches_merged_evaluation() {
        let bbox = BoundingBox::from_size(Vec3::new(6.0, 5.0, 4.0));
        let mut parts = vec![
            crate::geometry::icosphere_mesh(Vec3::new(1.5, 1.5, 1.5), 1.1, 2),
            crate::geometry::icosphere_mesh(Vec3::new(4.2, 3.0, 2.0), 1.3, 2),
            crate::geometry::icosphere_mesh(Vec3::new(5.6, 0.4, 3.8), 0.9, 2),
        ];
        let merged = crate::geometry::merge_meshes(&parts);
        for (method, pitch) in [("exact", 0.25), ("exact", 0.0)] {
            let evaluator = OptimizeS2::new(&params(method, pitch), bbox, &merged).unwrap();
            let mut coverage = evaluator.voxel_coverage(parts.iter(), bbox).expect("voxel methods build coverage");
            for step in 0..6 {
                let index = step % parts.len();
                crate::geometry::translate_mesh(&mut parts[index], Vec3::new(0.37, -0.21, 0.15));
                let old = coverage.replace(index, &parts[index]);
                let merged = crate::geometry::merge_meshes(&parts);
                assert_eq!(
                    evaluator.evaluate_voxel_grid(coverage.grid(), 4, 200, "candidate", None).unwrap(),
                    evaluator.evaluate(&merged, bbox, 4, 200, "candidate", None).unwrap(),
                    "{method} {pitch} step {step}"
                );
                if step % 2 == 1 {
                    crate::geometry::translate_mesh(&mut parts[index], Vec3::new(-0.37, 0.21, -0.15));
                    coverage.restore(index, old);
                }
            }
        }
        let mesh_mc = OptimizeS2::new(&params("monte_carlo", 0.0), bbox, &merged).unwrap();
        assert!(mesh_mc.voxel_coverage(parts.iter(), bbox).is_none());
        assert!(mesh_mc.evaluate_voxel_grid(&crate::geometry::s2::VoxelS2::new(&merged, bbox, 1.0), 0, 200, "candidate", None).is_err());
    }

    // AI-FUNC-SUMMARY: Compare actual CPU evaluation against exact/voxel/mesh reference VF and observe the execution pool for every stage; no file output.
    #[test]
    fn evaluator_preserves_voxel_and_mesh_definitions_at_every_stage() {
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let mesh = box_mesh(BoundingBox {
            min: Vec3::new(0.2, 0.2, 0.2),
            max: Vec3::new(1.4, 1.4, 1.4),
        });
        for workers in [1, 2, 8] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            for (method, pitch, expected) in [
                ("exact", 1.0, 1.0 / 64.0),
                ("monte_carlo", 1.0, 1.0 / 64.0),
                ("monte_carlo", 0.0, 1.728 / 64.0),
                ("exact", 0.0, 1.0 / 64.0),
            ] {
                let evaluator = OptimizeS2::new(&params(method, pitch), bbox, &mesh).unwrap();
                pool.install(|| {
                    for stage in [
                        "target",
                        "input",
                        "prune",
                        "initial",
                        "candidate",
                        "migration",
                        "final",
                    ] {
                        let s2 = evaluator.evaluate(&mesh, bbox, 0, 200, stage, None).unwrap();
                        assert!((s2[0] - expected).abs() < 1e-12, "{method} {pitch}: {s2:?}");
                    }
                });
                let observed = evaluator.observations.lock().unwrap();
                assert_eq!(observed.len(), 7);
                assert!(observed.iter().all(
                    |(_, count, index)| *count == workers && index.is_some_and(|i| i < workers)
                ));
            }
        }
    }

    // AI-FUNC-SUMMARY: Stress more islands than workers with nested parallel jobs; assert bounded live island contexts and ordered complete results, at 1/2/8 workers.
    #[test]
    fn island_batches_bound_live_work_and_preserve_order() {
        for workers in [1, 2, 8] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            for islands in [1, 3, 17] {
                let active = AtomicUsize::new(0);
                let peak = AtomicUsize::new(0);
                let results = pool.install(|| {
                    run_island_batches(islands, |id| {
                        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        (0..256usize).into_par_iter().for_each(|_| {
                            assert_eq!(rayon::current_num_threads(), workers);
                            assert!(rayon::current_thread_index().unwrap() < workers);
                            std::thread::yield_now();
                        });
                        active.fetch_sub(1, Ordering::SeqCst);
                        id
                    })
                });
                assert_eq!(results, (0..islands).collect::<Vec<_>>());
                assert!(peak.load(Ordering::SeqCst) <= workers.min(islands));
                assert_eq!(active.load(Ordering::SeqCst), 0);
            }
        }
    }

    // AI-FUNC-SUMMARY: Exercise one real GPU MC instance shared across nested island work with changing geometry; verify continuous VF and finite correlations; explicitly reports no-device skips.
    #[cfg(feature = "gpu")]
    #[test]
    fn shared_gpu_mesh_evaluator_serializes_geometry_updates() {
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let small = box_mesh(BoundingBox {
            min: Vec3::new(1.0, 1.0, 1.0),
            max: Vec3::new(2.0, 2.0, 2.0),
        });
        let large = box_mesh(BoundingBox {
            min: Vec3::new(1.0, 1.0, 1.0),
            max: Vec3::new(3.0, 3.0, 3.0),
        });
        let mut p = params("monte_carlo", 0.0);
        p.r_max = 1;
        p.mc_samples = 20_000;
        p.acceleration.mode = AccelerationMode::Gpu;
        let evaluator = OptimizeS2::new(&p, bbox, &small).unwrap();
        println!("{}", evaluator.description);
        if evaluator.gpu.is_none() {
            println!("SKIP: no GPU MC instance available; this is not GPU execution evidence");
            return;
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();
        let reference = pool.install(|| {
            [
                calculate_s2(&small, bbox, 1, 0.0, "monte_carlo", 20_000),
                calculate_s2(&large, bbox, 1, 0.0, "monte_carlo", 20_000),
            ]
        });
        let results = pool.install(|| {
            run_island_batches(5, |id| {
                let mesh = if id % 2 == 0 { &small } else { &large };
                let s2 = evaluator
                    .evaluate(mesh, bbox, 1, 20_000, "candidate", None)
                    .unwrap();
                assert!((s2[0] - if id % 2 == 0 { 1.0 / 64.0 } else { 8.0 / 64.0 }).abs() < 1e-12);
                assert!(
                    (s2[1] - reference[id % 2][1]).abs() < 0.02,
                    "GPU {s2:?}, CPU {:?}",
                    reference[id % 2]
                );
                assert!(s2
                    .iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(value)));
                s2
            })
        });
        assert_eq!(results.len(), 5);
    }
    // AI-FUNC-SUMMARY: Force an execution capacity error after GPU selection; verify forbidden fallback propagates stage errors and allowed fallback disables the instance while retaining mesh-MC semantics.
    #[cfg(feature = "gpu")]
    #[test]
    fn runtime_failure_honors_fallback_policy() {
        let bbox = BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0));
        let mesh = box_mesh(bbox);
        let expected_vf = calculate_s2(&mesh, bbox, 0, 0.0, "monte_carlo", 200)[0];
        for fallback in [true, false] {
            let mut p = params("monte_carlo", 0.0);
            p.r_max = 1;
            p.acceleration.mode = AccelerationMode::Gpu;
            p.acceleration.cpu_fallback = true;
            let mut evaluator = OptimizeS2::new(&p, bbox, &mesh).unwrap();
            if evaluator.gpu.is_none() {
                eprintln!("SKIP: GPU unavailable");
                return;
            }
            evaluator.cpu_fallback = fallback;
            let degenerate = BoundingBox::from_size(Vec3::new(1e-300, 1.0, 1.0));
            let result = evaluator.evaluate(&mesh, degenerate, 1, 200, "candidate", None);
            if fallback {
                let result = result.unwrap();
                assert_eq!(result.len(), 2);
                assert!(evaluator.gpu.as_ref().unwrap().lock().unwrap().is_none());
                assert_eq!(
                    evaluator.evaluate(&mesh, bbox, 0, 200, "final", None).unwrap(),
                    vec![expected_vf]
                );
            } else {
                let error = result.unwrap_err().to_string();
                assert!(error.contains("candidate") && error.contains("bbox"), "{error}");
                assert!(evaluator.gpu.as_ref().unwrap().lock().unwrap().is_some());
            }
        }
    }
    // AI-FUNC-SUMMARY: Verify per-stage budget excess disables the shared GPU under allowed fallback and preserves it on strict failure, before upload or growth.
    #[cfg(feature = "gpu")]
    #[test]
    fn runtime_budget_excess_honors_fallback_policy() {
        let bbox = BoundingBox::from_size(crate::types::Vec3::new(1.0, 1.0, 1.0));
        let mesh = crate::geometry::box_mesh(bbox);
        let expected = vec![crate::geometry::volume_fraction_in_bbox(&mesh, bbox)];
        assert!((expected[0] - 1.0).abs() < 1e-12);
        if let Err(error) = crate::gpu::GpuS2Pipeline::new(&Mesh::empty(), bbox) {
            eprintln!("SKIP: GPU MC unavailable: {error}");
            return;
        }
        let large = crate::geometry::icosphere_mesh(crate::types::Vec3::new(0.5, 0.5, 0.5), 0.4, 5);
        let large_expected = vec![crate::geometry::volume_fraction_in_bbox(&large, bbox)];
        for fallback in [false, true] {
            let mut p = params("monte_carlo", 0.0);
            p.acceleration.mode = AccelerationMode::Gpu;
            p.acceleration.cpu_fallback = fallback;
            p.acceleration.gpu_memory_limit_mb = Some(1);
            let evaluator = OptimizeS2::new(&p, bbox, &mesh).unwrap();
            assert!(evaluator.gpu.as_ref().unwrap().lock().unwrap().is_some());
            let result = evaluator.evaluate(&large, bbox, 0, 200, "large-budget-stage", None);
            if fallback {
                assert_eq!(result.unwrap(), large_expected);
                assert!(evaluator.gpu.as_ref().unwrap().lock().unwrap().is_none());
                assert_eq!(
                    evaluator
                        .evaluate(&mesh, bbox, 0, 200, "after-budget-fallback", None)
                        .unwrap(),
                    expected
                );
            } else {
                let error = result.unwrap_err().to_string();
                assert!(
                    error.contains("large-budget-stage") && error.contains("exceeds budget"),
                    "{error}"
                );
                assert!(evaluator.gpu.as_ref().unwrap().lock().unwrap().is_some());
                assert_eq!(
                    evaluator
                        .evaluate(&mesh, bbox, 0, 200, "after-budget-error", None)
                        .unwrap(),
                    expected
                );
            }
        }
    }
    // AI-FUNC-SUMMARY: Verify a workload above the old one-dimensional MC bound reaches backend probing rather than being rejected by a stale optimizer guard; allocate no GPU data.
    #[test]
    fn gpu_large_logical_workload_reaches_probe() {
        let mut p = params("monte_carlo", 0.0);
        p.r_max = 0;
        p.mc_samples = 65_536 * 256;
        p.acceleration.cpu_fallback = true;
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let result = select_s2_backend(&p, AccelerationMode::Gpu, 1, 0, || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            BackendSelection {
                backend: ComputeBackend::Cpu,
                fallback: Some(FallbackReason {
                    requested: AccelerationMode::Gpu,
                    reason: "large workload probe reached".into(),
                }),
            }
        })
        .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            result.fallback.unwrap().reason,
            "large workload probe reached"
        );
    }
}
