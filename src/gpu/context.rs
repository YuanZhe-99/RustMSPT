use crate::compute::backend::BackendCaps;
use std::sync::{Mutex, OnceLock};

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
// Side effects: Reuses the process GPU instance, selecting a fresh adapter and requesting a fresh logical device on each call.
// Notes: Uses pollster::block_on and the default power preference; RUSTMSPT_GPU_DEVICE selects a name or index.
pub fn try_init_gpu() -> Result<GpuContext, GpuInitError> {
    let adapter = request_adapter().map_err(GpuInitError)?;

    let adapter_info = adapter.get_info();
    let adapter_name = adapter_info.name.clone();

    let (_device, _queue) = request_device(&adapter, "rustmspt compute device").map_err(GpuInitError)?;

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
    let adapter = request_adapter()?;

    request_device(&adapter, label)
}

// AI-FUNC-SUMMARY: Request one logical device from a fresh adapter, returning the request error without caching failures.
fn request_device(adapter: &wgpu::Adapter, label: &str) -> Result<(wgpu::Device, wgpu::Queue), String> {
    pollster::block_on(async {
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some(label),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            }, None)
            .await
            .map_err(|e| format!("device request failed: {e}"))
    })
}

// AI-FUNC-SUMMARY: Reuse one backend instance for the process; adapters remain fresh because each can create only one logical device.
fn shared_instance() -> &'static wgpu::Instance {
    static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();
    INSTANCE.get_or_init(|| wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    }))
}

// AI-FUNC-SUMMARY: Select a fresh adapter using the current environment, never memoizing a failed selector.
fn request_adapter() -> Result<wgpu::Adapter, String> {
    let filter = std::env::var("RUSTMSPT_GPU_DEVICE").ok();
    select_adapter(filter.as_deref())
}

// AI-FUNC-SUMMARY: Select one adapter using the shared name/index/default policy; return an explicit error for an unavailable selector.
fn select_adapter(device_filter: Option<&str>) -> Result<wgpu::Adapter, String> {
    // EGL adapter enumeration and dropping unselected adapters share a context in wgpu 24.
    // Keep those operations together; selected GL adapters receive a private instance below.
    static SELECTION: Mutex<()> = Mutex::new(());
    let _selection = SELECTION.lock().map_err(|_| "GPU selection lock poisoned".to_string())?;
    let adapter = select_from_instance(shared_instance(), device_filter)?;
    if adapter.get_info().backend == wgpu::Backend::Gl {
        drop(adapter);
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        select_from_instance(&instance, device_filter)
    } else {
        Ok(adapter)
    }
}

// AI-FUNC-SUMMARY: Apply the existing selector against an instance; callers serialize shared-instance enumeration and retain private GL contexts.
fn select_from_instance(instance: &wgpu::Instance, device_filter: Option<&str>) -> Result<wgpu::Adapter, String> {
    pollster::block_on(async {
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
                .find(|a| a.get_info().name.contains(*filter))
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
    })

}

#[cfg(test)]
mod tests {
    use super::*;

    // AI-FUNC-SUMMARY: Verify concurrent initialization shares one instance and a warm instance still rejects an unavailable selector.
    #[test]
    fn shared_instance_and_fresh_selector_contract() {
        let address = shared_instance() as *const wgpu::Instance as usize;
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8).map(|_| scope.spawn(|| shared_instance() as *const wgpu::Instance as usize)).collect();
            for handle in handles { assert_eq!(handle.join().unwrap(), address); }
        });
        let filter = "definitely-no-shared-instance-adapter";
        for _ in 0..2 {
            let error = select_adapter(Some(filter)).err().expect("unavailable selector must fail, even after instance initialization");
            assert!(error.contains(filter));
        }
    }
    // AI-FUNC-SUMMARY: Exercise concurrent logical-device initialization on isolated GL adapters when the backend supports default device limits.
    #[test]
    fn gl_devices_keep_private_instances() {
        for index in 0..16 {
            let filter = index.to_string();
            let adapter = match select_adapter(Some(&filter)) { Ok(adapter) => adapter, Err(_) => break };
            if adapter.get_info().backend != wgpu::Backend::Gl { continue; }
            let first = match request_device(&adapter, "GL isolation probe") {
                Ok(pair) => pair,
                Err(error) => { eprintln!("SKIP: GL device unavailable: {error}"); return; }
            };
            std::thread::scope(|scope| {
                let handles: Vec<_> = (0..3).map(|_| scope.spawn(|| {
                    let adapter = select_adapter(Some(&filter)).unwrap();
                    assert_eq!(adapter.get_info().backend, wgpu::Backend::Gl);
                    request_device(&adapter, "GL isolated concurrent device").unwrap()
                })).collect();
                for handle in handles { let _device = handle.join().unwrap(); }
            });
            drop(first);
            return;
        }
        eprintln!("SKIP: GL adapter unavailable");
    }

    // AI-FUNC-SUMMARY: Measure fresh-instance and reused-instance adapter selection with matching identity; excludes logical-device and shader work.
    #[test]
    #[ignore = "release performance measurement"]
    fn instance_selection_benchmark() {
        let expected = match select_adapter(None) {
            Ok(adapter) => adapter.get_info(),
            Err(error) => { eprintln!("SKIP: no adapter: {error}"); return; }
        };
        println!("selection_adapter name={} backend={:?}", expected.name, expected.backend);
        for sample in 0..6 {
            let mut times = [0.0; 2];
            for which in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
                let start = std::time::Instant::now();
                for _ in 0..5 {
                    let adapter = if which == 0 {
                        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::all(), ..Default::default() });
                        select_from_instance(&instance, None).unwrap()
                    } else { select_adapter(None).unwrap() };
                    let info = adapter.get_info();
                    assert_eq!(info.name, expected.name);
                    assert_eq!(info.backend, expected.backend);
                }
                times[which] = start.elapsed().as_secs_f64();
            }
            println!("instance_selection sample={sample} repeats=5 fresh_seconds={:.9} reused_seconds={:.9}", times[0], times[1]);
        }
    }

}
