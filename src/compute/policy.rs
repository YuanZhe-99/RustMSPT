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
// Purpose: Probe the GPU and apply the memory-limit guard for Gpu/Auto requests.
// Inputs: requested mode (used in fallback messages), optional GPU memory limit in MB.
// Returns: BackendSelection with the GPU backend on success, or CPU plus a fallback reason.
// Side effects: When feature "gpu" is enabled, calls crate::gpu::try_init_gpu() (heavy first call).
// Notes: Without the "gpu" feature, always falls back to CPU with a feature-not-enabled reason.
fn select_gpu_backend(
    requested: AccelerationMode,
    #[allow(unused_variables)] gpu_memory_limit_mb: Option<u64>,
) -> BackendSelection {
    #[cfg(feature = "gpu")]
    {
        match crate::gpu::try_init_gpu() {
            Ok(ctx) => {
                let caps = ctx.caps();
                if let Some(limit_mb) = gpu_memory_limit_mb {
                    let max_bytes = limit_mb * 1024 * 1024;
                    if caps.max_storage_buffer_binding_size > 0
                        && caps.max_storage_buffer_binding_size < max_bytes
                    {
                        return BackendSelection {
                            backend: ComputeBackend::Cpu,
                            fallback: Some(FallbackReason {
                                requested,
                                reason: format!(
                                    "GPU memory limit {}MB exceeds adapter max buffer size {}MB",
                                    limit_mb,
                                    caps.max_storage_buffer_binding_size / (1024 * 1024)
                                ),
                            }),
                        };
                    }
                }
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
