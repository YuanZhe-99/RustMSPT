use super::backend::{AccelerationMode, ComputeBackend};

#[derive(Debug, Clone)]
pub struct FallbackReason {
    pub requested: AccelerationMode,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct BackendSelection {
    pub backend: ComputeBackend,
    pub fallback: Option<FallbackReason>,
}

// AI-FUNC-SUMMARY:
// Purpose: Select the compute backend based on requested mode, GPU availability, and workload size (voxel-count wrapper).
// Inputs: requested mode, optional GPU min voxel threshold, optional GPU memory limit in MB, current workload voxel count.
// Returns: BackendSelection with chosen backend and optional fallback reason.
// Side effects: When feature "gpu" is enabled, may initialize a wgpu adapter (heavy first call).
// Notes: Auto mode prefers GPU when available and workload exceeds threshold; falls back to CPU otherwise.
// Delegates to select_backend_for_workload with voxel semantics and a 250,000 default threshold.
pub fn select_backend(
    requested: AccelerationMode,
    gpu_min_voxels: Option<usize>,
    gpu_memory_limit_mb: Option<u64>,
    workload_voxels: usize,
) -> BackendSelection {
    select_backend_for_workload(
        requested,
        Some(gpu_min_voxels.unwrap_or(250_000)),
        gpu_memory_limit_mb,
        workload_voxels,
        "voxels",
    )
}

// AI-FUNC-SUMMARY:
// Purpose: Generic compute backend selection for any workload unit (voxels, pixels, etc.).
// Inputs: requested mode, optional minimum workload threshold for Auto mode, optional GPU memory
//         limit in MB, workload size, and a unit label used in fallback messages ("voxels"/"pixels").
// Returns: BackendSelection with chosen backend and optional fallback reason.
// Side effects: When feature "gpu" is enabled and requested is Gpu (or Auto above the threshold),
// may initialize a wgpu adapter (heavy first call).
// Notes: Auto mode checks the threshold before any GPU probing; a None threshold means "always
// attempt GPU in Auto mode". Fallback messages name the unit, e.g. "below gpu_min_pixels threshold".
pub fn select_backend_for_workload(
    requested: AccelerationMode,
    gpu_min_workload: Option<usize>,
    #[allow(unused_variables)] gpu_memory_limit_mb: Option<u64>,
    workload: usize,
    workload_unit: &str,
) -> BackendSelection {
    match requested {
        AccelerationMode::Cpu => BackendSelection {
            backend: ComputeBackend::Cpu,
            fallback: None,
        },
        AccelerationMode::Gpu => select_gpu_backend(requested, gpu_memory_limit_mb),
        AccelerationMode::Auto => {
            let min_workload = gpu_min_workload.unwrap_or(0);
            if workload < min_workload {
                return BackendSelection {
                    backend: ComputeBackend::Cpu,
                    fallback: Some(FallbackReason {
                        requested: AccelerationMode::Auto,
                        reason: format!(
                            "workload {} {} below gpu_min_{} threshold {}",
                            workload, workload_unit, workload_unit, min_workload
                        ),
                    }),
                };
            }
            select_gpu_backend(requested, gpu_memory_limit_mb)
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Probe GPU availability for Gpu/Auto requests; reject unvalidated budget requests before probing rather than comparing total budget with a per-binding limit.
// Inputs: requested mode (used in fallback messages), optional GPU memory limit in MB.
// Returns: BackendSelection with the GPU backend on success, or CPU plus a fallback reason.
// Side effects: When feature "gpu" is enabled, calls crate::gpu::try_init_gpu() (heavy first call).
// Notes: Without the "gpu" feature, always falls back to CPU with a feature-not-enabled reason.
fn select_gpu_backend(
    requested: AccelerationMode,
    #[allow(unused_variables)] gpu_memory_limit_mb: Option<u64>,
) -> BackendSelection {
    if let Some(mb) = gpu_memory_limit_mb {
        let reason = if mb.checked_mul(1024 * 1024).is_none() {
            "GPU memory budget overflows bytes"
        } else {
            "GPU budget requires a validated working-set estimate; use resolve_execution"
        };
        return BackendSelection {
            backend: ComputeBackend::Cpu,
            fallback: Some(FallbackReason {
                requested,
                reason: reason.into(),
            }),
        };
    }
    #[cfg(feature = "gpu")]
    {
        match crate::gpu::try_init_gpu() {
            Ok(ctx) => {
                let caps = ctx.caps();
                BackendSelection {
                    backend: ComputeBackend::Gpu {
                        adapter_name: caps.name,
                        max_buffer_size: caps.max_buffer_size,
                        max_storage_buffer_binding_size: caps.max_storage_buffer_binding_size,
                    },
                    fallback: None,
                }
            }
            Err(e) => BackendSelection {
                backend: ComputeBackend::Cpu,
                fallback: Some(FallbackReason {
                    requested,
                    reason: format!("GPU init failed: {}", e),
                }),
            },
        }
    }
    #[cfg(not(feature = "gpu"))]
    {
        BackendSelection {
            backend: ComputeBackend::Cpu,
            fallback: Some(FallbackReason {
                requested,
                reason: "cargo feature 'gpu' is not enabled".to_string(),
            }),
        }
    }
}

// AI-FUNC-SUMMARY: Resolve the documented environment override once; return a configured mode or an explicit invalid-value error without probing GPU.
pub fn configured_mode(
    config: &crate::config::AccelerationConfig,
) -> crate::error::Result<AccelerationMode> {
    match std::env::var("RUSTMSPT_ACCELERATION") {
        Ok(value) => match value.as_str() {
            "cpu" => Ok(AccelerationMode::Cpu),
            "gpu" => Ok(AccelerationMode::Gpu),
            "auto" => Ok(AccelerationMode::Auto),
            _ => Err(crate::error::RustMsptError::InvalidConfig(format!(
                "invalid RUSTMSPT_ACCELERATION: {value}"
            ))),
        },
        Err(std::env::VarError::NotPresent) => Ok(config.mode),
        Err(_) => Err(crate::error::RustMsptError::InvalidConfig(
            "RUSTMSPT_ACCELERATION must be Unicode".into(),
        )),
    }
}

// AI-FUNC-SUMMARY: Resolve a method-specific execution decision with threshold-before-probe, explicit option support, estimated task bytes and strict fallback policy; may probe GPU once.
pub fn resolve_execution(
    config: &crate::config::AccelerationConfig,
    requested: AccelerationMode,
    workload: usize,
    threshold: usize,
    supports_gpu: bool,
    estimated_gpu_bytes: Option<u64>,
) -> crate::error::Result<BackendSelection> {
    use crate::error::RustMsptError;
    if requested == AccelerationMode::Cpu
        || (requested == AccelerationMode::Auto && workload < threshold)
    {
        return Ok(BackendSelection {
            backend: ComputeBackend::Cpu,
            fallback: None,
        });
    }
    let failure = |reason: String| {
        if config.cpu_fallback {
            Ok(BackendSelection {
                backend: ComputeBackend::Cpu,
                fallback: Some(FallbackReason { requested, reason }),
            })
        } else {
            Err(RustMsptError::Gpu(reason))
        }
    };
    if !supports_gpu {
        return failure("selected method has no supported GPU execution path".into());
    }
    if config.backend != "wgpu" || config.gpu_precision != "f32" || config.gpu_prefer_power {
        return Err(RustMsptError::InvalidConfig(
            "GPU currently requires backend=wgpu, gpu_precision=f32 and gpu_prefer_power=false"
                .into(),
        ));
    }
    if let Some(mb) = config.gpu_memory_limit_mb {
        let bytes = mb.checked_mul(1024 * 1024).ok_or_else(|| {
            RustMsptError::InvalidConfig("GPU memory budget overflows bytes".into())
        })?;
        match estimated_gpu_bytes {
            Some(needed) if needed <= bytes => {}
            Some(needed) => {
                return failure(format!(
                    "GPU task requires {needed} bytes, exceeding budget {bytes}"
                ))
            }
            None => return failure("GPU task has no validated working-set estimate".into()),
        }
    }
    let selection = select_backend(requested, Some(threshold), None, workload);
    if !selection.backend.is_gpu() && !config.cpu_fallback {
        return Err(RustMsptError::Gpu(
            selection
                .fallback
                .map(|f| f.reason)
                .unwrap_or_else(|| "GPU unavailable".into()),
        ));
    }
    Ok(selection)
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    // AI-FUNC-SUMMARY: Verify legacy selection refuses budgets lacking a workload estimate, checks conversion overflow, and preserves CPU/Auto threshold ordering without GPU probing.
    #[test]
    fn legacy_budget_needs_working_set_estimate() {
        for budget in [0, 1, 1024] {
            let selected = select_backend(AccelerationMode::Gpu, None, Some(budget), 1);
            assert!(!selected.backend.is_gpu());
            assert!(selected
                .fallback
                .unwrap()
                .reason
                .contains("validated working-set estimate"));
        }
        let overflow = select_backend(AccelerationMode::Gpu, None, Some(u64::MAX), 1);
        assert!(overflow.fallback.unwrap().reason.contains("overflows"));
        assert!(
            select_backend(AccelerationMode::Cpu, None, Some(u64::MAX), 1)
                .fallback
                .is_none()
        );
        assert!(
            select_backend(AccelerationMode::Auto, Some(2), Some(u64::MAX), 1)
                .fallback
                .unwrap()
                .reason
                .contains("below gpu_min_voxels")
        );
        let config = crate::config::AccelerationConfig {
            gpu_memory_limit_mb: Some(1),
            cpu_fallback: false,
            ..Default::default()
        };
        assert!(resolve_execution(&config, AccelerationMode::Gpu, 1, 0, true, None).is_err());
    }

    // AI-FUNC-SUMMARY: Verify a validated tiny workload can select GPU under a total budget larger than the device single-binding limit.
    #[cfg(feature = "gpu")]
    #[test]
    fn total_budget_is_not_a_single_binding_request() {
        let context = match crate::gpu::try_init_gpu() {
            Ok(context) => context,
            Err(error) => {
                eprintln!("SKIP: GPU device unavailable: {error:?}");
                return;
            }
        };
        let budget_mb = context.caps().max_storage_buffer_binding_size / (1024 * 1024) + 1;
        let config = crate::config::AccelerationConfig {
            gpu_memory_limit_mb: Some(budget_mb),
            cpu_fallback: false,
            ..Default::default()
        };
        let selected =
            resolve_execution(&config, AccelerationMode::Gpu, 1, 0, true, Some(1024)).unwrap();
        assert!(selected.backend.is_gpu());
    }
}
