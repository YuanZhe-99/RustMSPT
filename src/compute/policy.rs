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
// Purpose: Select the compute backend based on requested mode, GPU availability, and workload size.
// Inputs: requested mode, optional GPU min voxel threshold, optional GPU memory limit in MB, current workload voxel count.
// Returns: BackendSelection with chosen backend and optional fallback reason.
// Side effects: When feature "gpu" is enabled, may initialize a wgpu adapter (heavy first call).
// Notes: Auto mode prefers GPU when available and workload exceeds threshold; falls back to CPU otherwise.
pub fn select_backend(
    requested: AccelerationMode,
    gpu_min_voxels: Option<usize>,
    #[allow(unused_variables)] gpu_memory_limit_mb: Option<u64>,
    workload_voxels: usize,
) -> BackendSelection {
    match requested {
        AccelerationMode::Cpu => BackendSelection {
            backend: ComputeBackend::Cpu,
            fallback: None,
        },
        AccelerationMode::Gpu => {
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
                                        requested: AccelerationMode::Gpu,
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
                            requested: AccelerationMode::Gpu,
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
                        requested: AccelerationMode::Gpu,
                        reason: "cargo feature 'gpu' is not enabled".to_string(),
                    }),
                }
            }
        }
        AccelerationMode::Auto => {
            let min_voxels = gpu_min_voxels.unwrap_or(250_000);
            if workload_voxels < min_voxels {
                return BackendSelection {
                    backend: ComputeBackend::Cpu,
                    fallback: Some(FallbackReason {
                        requested: AccelerationMode::Auto,
                        reason: format!(
                            "workload {} voxels below gpu_min_voxels threshold {}",
                            workload_voxels, min_voxels
                        ),
                    }),
                };
            }

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
                                        requested: AccelerationMode::Auto,
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
                            requested: AccelerationMode::Auto,
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
                        requested: AccelerationMode::Auto,
                        reason: "cargo feature 'gpu' is not enabled".to_string(),
                    }),
                }
            }
        }
    }
}
