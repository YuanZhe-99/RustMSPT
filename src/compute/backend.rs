use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccelerationMode {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

impl fmt::Display for AccelerationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccelerationMode::Auto => write!(f, "auto"),
            AccelerationMode::Cpu => write!(f, "cpu"),
            AccelerationMode::Gpu => write!(f, "gpu"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackendCaps {
    pub name: String,
    pub supports_gpu: bool,
    pub max_buffer_size: u64,
    pub max_storage_buffer_binding_size: u64,
}

#[derive(Debug, Clone)]
pub enum ComputeBackend {
    Cpu,
    #[cfg(feature = "gpu")]
    Gpu {
        adapter_name: String,
        max_buffer_size: u64,
        max_storage_buffer_binding_size: u64,
    },
}

impl ComputeBackend {
    // AI-FUNC-SUMMARY: Return a human-readable name for the backend; returns String; side effects: None.
    pub fn name(&self) -> &str {
        match self {
            ComputeBackend::Cpu => "cpu",
            #[cfg(feature = "gpu")]
            ComputeBackend::Gpu { adapter_name, .. } => adapter_name,
        }
    }

    // AI-FUNC-SUMMARY: Return whether this backend represents a GPU; returns bool; side effects: None.
    pub fn is_gpu(&self) -> bool {
        #[cfg(feature = "gpu")]
        {
            matches!(self, ComputeBackend::Gpu { .. })
        }
        #[cfg(not(feature = "gpu"))]
        {
            false
        }
    }

    // AI-FUNC-SUMMARY: Return capabilities summary for the backend; returns BackendCaps; side effects: None.
    pub fn caps(&self) -> BackendCaps {
        match self {
            ComputeBackend::Cpu => BackendCaps {
                name: "cpu".to_string(),
                supports_gpu: false,
                max_buffer_size: 0,
                max_storage_buffer_binding_size: 0,
            },
            #[cfg(feature = "gpu")]
            ComputeBackend::Gpu {
                adapter_name,
                max_buffer_size,
                max_storage_buffer_binding_size,
            } => BackendCaps {
                name: adapter_name.clone(),
                supports_gpu: true,
                max_buffer_size: *max_buffer_size,
                max_storage_buffer_binding_size: *max_storage_buffer_binding_size,
            },
        }
    }
}

impl fmt::Display for ComputeBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComputeBackend::Cpu => write!(f, "cpu"),
            #[cfg(feature = "gpu")]
            ComputeBackend::Gpu { adapter_name, .. } => write!(f, "gpu/wgpu/{}", adapter_name),
        }
    }
}
