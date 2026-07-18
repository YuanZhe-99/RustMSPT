use crate::compute::backend::BackendCaps;

pub struct GpuContext {
    adapter_name: String,
    max_buffer_size: u64,
    max_storage_buffer_binding_size: u64,
}

impl GpuContext {
    // AI-FUNC-SUMMARY: Return capabilities for this GPU context; returns BackendCaps; side effects: None.
    pub fn caps(&self) -> BackendCaps {
        BackendCaps {
            name: self.adapter_name.clone(),
            supports_gpu: true,
            max_buffer_size: self.max_buffer_size,
            max_storage_buffer_binding_size: self.max_storage_buffer_binding_size,
        }
    }
}

#[derive(Debug)]
pub struct GpuInitError(String);

impl std::fmt::Display for GpuInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for GpuInitError {}

// AI-FUNC-SUMMARY:
// Purpose: Try to initialize a wgpu adapter/device and return a GpuContext with capabilities.
// Inputs: None (uses environment variable RUSTMSPT_GPU_DEVICE for adapter selection).
// Returns: Ok(GpuContext) on success, Err(GpuInitError) when no suitable GPU is available.
// Side effects: First call performs heavy wgpu adapter enumeration; subsequent calls return cached result.
// Notes: Uses pollster::block_on for synchronous init. Prefers low-power adapter by default; set RUSTMSPT_GPU_DEVICE to override.
pub fn try_init_gpu() -> Result<GpuContext, GpuInitError> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let device_filter = std::env::var("RUSTMSPT_GPU_DEVICE").ok();

    let adapter = pollster::block_on(async {
        let request = if let Some(ref filter) = device_filter {
            if let Ok(idx) = filter.parse::<usize>() {
                let adapters = instance.enumerate_adapters(wgpu::Backends::all());
                if idx < adapters.len() {
                    return Ok(adapters.into_iter().nth(idx).unwrap());
                }
                return Err(GpuInitError(format!("RUSTMSPT_GPU_DEVICE index {} out of range ({} adapters found)", idx, adapters.len())));
            }
            let adapters = instance.enumerate_adapters(wgpu::Backends::all());
            let matched = adapters.into_iter().find(|a| {
                let info = a.get_info();
                info.name.contains(filter.as_str())
            });
            matched.ok_or_else(|| GpuInitError(format!("no adapter matching '{}'", filter)))
        } else {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::default(),
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .ok_or_else(|| GpuInitError("no suitable GPU adapter found".to_string()))
        };
        request
    })?;

    let adapter_info = adapter.get_info();
    let adapter_name = adapter_info.name.clone();

    let (_device, _queue) = pollster::block_on(async {
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rustmspt compute device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            }, None)
            .await
            .map_err(|e| GpuInitError(format!("device request failed: {}", e)))
    })?;

    let limits = adapter.limits();
    Ok(GpuContext {
        adapter_name,
        max_buffer_size: limits.max_buffer_size,
        max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size as u64,
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Shared adapter/device request for GPU pipelines, honoring the RUSTMSPT_GPU_DEVICE filter.
// Inputs: device label used for debugging/profiling tools.
// Returns: Ok((Device, Queue)) on success, Err(message) describing the failure.
// Side effects: Blocking wgpu adapter enumeration and device request (heavy first call).
// Notes: RUSTMSPT_GPU_DEVICE selects an adapter by index (numeric) or name substring; unset uses
// the default power preference. Requests empty features and default limits.
pub(crate) fn request_adapter_device(label: &str) -> Result<(wgpu::Device, wgpu::Queue), String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let device_filter = std::env::var("RUSTMSPT_GPU_DEVICE").ok();

    let adapter = pollster::block_on(async {
        if let Some(ref filter) = device_filter {
            let adapters = instance.enumerate_adapters(wgpu::Backends::all());
            if let Ok(idx) = filter.parse::<usize>() {
                let count = adapters.len();
                return adapters
                    .into_iter()
                    .nth(idx)
                    .ok_or_else(|| format!("RUSTMSPT_GPU_DEVICE index {idx} out of range ({count} adapters found)"));
            }
            adapters
                .into_iter()
                .find(|a| a.get_info().name.contains(filter.as_str()))
                .ok_or_else(|| format!("no adapter matching '{filter}'"))
        } else {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::default(),
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .ok_or_else(|| "no suitable GPU adapter".to_string())
        }
    })?;

    let (device, queue) = pollster::block_on(async {
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some(label),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            }, None)
            .await
            .map_err(|e| format!("device request failed: {e}"))
    })?;

    Ok((device, queue))
}
