use crate::compute::backend::BackendCaps;
use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Whether an adapter is real graphics hardware or a software rasterizer running on the CPU.
///
/// Results on a software adapter are as correct as on hardware - certification recomputes anything
/// uncertain - but its timings say nothing about GPU performance, so every report and test says which it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterClass {
    Software,
    Hardware,
}

impl AdapterClass {
    // AI-FUNC-SUMMARY: Lowercase name of the class for logs; returns "software" or "hardware"; side effects: none.
    pub fn as_str(self) -> &'static str {
        match self {
            AdapterClass::Software => "software",
            AdapterClass::Hardware => "hardware",
        }
    }
}

// AI-FUNC-SUMMARY: Classify an adapter as software (CPU device type, or a known software rasterizer by name) or hardware; returns AdapterClass; side effects: none.
// Notes: Some software drivers report DeviceType::Other, so the name check covers llvmpipe, lavapipe, SwiftShader,
// softpipe and the Microsoft Basic Render Driver as well.
pub fn classify_adapter(info: &wgpu::AdapterInfo) -> AdapterClass {
    let name = info.name.to_lowercase();
    let software_name = ["llvmpipe", "lavapipe", "swiftshader", "softpipe", "microsoft basic render"]
        .iter()
        .any(|n| name.contains(n));
    if info.device_type == wgpu::DeviceType::Cpu || software_name {
        AdapterClass::Software
    } else {
        AdapterClass::Hardware
    }
}

pub struct GpuContext {
    adapter_name: String,
    adapter_class: AdapterClass,
    adapter_backend: String,
    max_buffer_size: u64,
    max_storage_buffer_binding_size: u64,
}

impl GpuContext {
    // AI-FUNC-SUMMARY: Whether the probed adapter is software or hardware; returns AdapterClass; side effects: none.
    pub fn adapter_class(&self) -> AdapterClass {
        self.adapter_class
    }

    // AI-FUNC-SUMMARY: One-line description of the probed adapter (name, backend, class); returns String; side effects: none.
    pub fn describe(&self) -> String {
        format!("name={} backend={} class={}", self.adapter_name, self.adapter_backend, self.adapter_class.as_str())
    }

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

type PipelineKey = (&'static str, String);
type PipelineMap = HashMap<PipelineKey, Box<dyn Any + Send + Sync>>;

static DEVICE_CREATIONS: AtomicU64 = AtomicU64::new(0);
static PIPELINE_BUILDS: AtomicU64 = AtomicU64::new(0);

pub struct SharedGpuDevice {
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    limits: wgpu::Limits,
    lost: Arc<AtomicBool>,
    pipelines: Mutex<PipelineMap>,
}

impl SharedGpuDevice {
    // AI-FUNC-SUMMARY: Borrow the shared logical device; returns the device handle; side effects: None.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    // AI-FUNC-SUMMARY: Borrow the shared submission queue; returns the queue handle; side effects: None.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    // AI-FUNC-SUMMARY: Report the selected adapter identity; returns adapter info; side effects: None.
    pub fn info(&self) -> &wgpu::AdapterInfo {
        &self.info
    }

    // AI-FUNC-SUMMARY: Report whether wgpu signalled loss or destruction of this device; returns the flag; side effects: None.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Return a compiled pipeline bundle for (kind, source) on this device, compiling it once under balanced error scopes.
    // Inputs: kind names the layout/entry-point family; source is the full WGSL text; build creates the Clone-able pipeline bundle.
    // Returns: A clone of the cached bundle, or the captured validation/allocation error.
    // Side effects: Compiles and caches on first use and increments the process pipeline-build counter; failures are never cached.
    // Notes: Runs inside runtime::scoped, so the process scope lock is always taken before the map lock; buffers are never cached here.
    pub(crate) fn cached_pipeline<T: Clone + Send + Sync + 'static>(
        &self,
        kind: &'static str,
        source: &str,
        build: impl FnOnce(&wgpu::Device) -> T,
    ) -> Result<T, String> {
        super::runtime::scoped(&self.device, || {
            let mut map = self
                .pipelines
                .lock()
                .map_err(|_| "GPU pipeline cache lock poisoned".to_string())?;
            let key = (kind, source.to_string());
            if let Some(found) = map.get(&key).and_then(|v| v.downcast_ref::<T>()) {
                return Ok(found.clone());
            }
            let built = super::runtime::scoped(&self.device, || Ok(build(&self.device)))?;
            PIPELINE_BUILDS.fetch_add(1, Ordering::Relaxed);
            map.insert(key, Box::new(built.clone()));
            Ok(built)
        })
    }
}

// AI-FUNC-SUMMARY: Count logical devices created by the shared device cache in this process; returns the monotonic count; side effects: None.
pub fn gpu_device_creation_count() -> u64 {
    DEVICE_CREATIONS.load(Ordering::Relaxed)
}

// AI-FUNC-SUMMARY: Count compute/render pipeline bundles compiled through the shared pipeline cache in this process; returns the monotonic count; side effects: None.
pub fn gpu_pipeline_build_count() -> u64 {
    PIPELINE_BUILDS.load(Ordering::Relaxed)
}

// AI-FUNC-SUMMARY:
// Purpose: Return the process-wide shared device for the current RUSTMSPT_GPU_DEVICE selector, creating it lazily.
// Inputs: None (reads RUSTMSPT_GPU_DEVICE on every call).
// Returns: Arc to the cached device/queue/adapter info, or a selection/device-request error.
// Side effects: May enumerate adapters and request one logical device, incrementing the device-creation counter.
// Notes: Keyed by the exact selector value (unset is its own key). Failures are never cached; a device
// reported lost is evicted and recreated. GL adapters keep their private-instance selection rules.
pub fn shared_gpu_device() -> Result<Arc<SharedGpuDevice>, GpuInitError> {
    let filter = std::env::var("RUSTMSPT_GPU_DEVICE").ok();
    shared_device_for(filter).map_err(GpuInitError)
}

// AI-FUNC-SUMMARY: Return the process-wide selector-to-device cache; returns the static mutex; side effects: initializes it on first use.
fn device_cache() -> &'static Mutex<HashMap<Option<String>, Arc<SharedGpuDevice>>> {
    static CACHE: OnceLock<Mutex<HashMap<Option<String>, Arc<SharedGpuDevice>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

// AI-FUNC-SUMMARY:
// Purpose: Drop every cached shared device so logical devices are destroyed before process exit, as they were when each pipeline owned its device.
// Inputs: None.
// Returns: None.
// Side effects: Empties the device cache; devices still referenced by live pipelines are destroyed when those drop.
// Notes: Called at the end of the CLI entry point; a later shared_gpu_device call recreates a device.
pub fn release_shared_gpu_devices() {
    let drained: Vec<_> = match device_cache().lock() {
        Ok(mut cache) => cache.drain().map(|(_, device)| device).collect(),
        Err(poisoned) => poisoned.into_inner().drain().map(|(_, device)| device).collect(),
    };
    drop(drained);
}

// AI-FUNC-SUMMARY: Look up or create the shared device for one selector under the cache lock, evicting lost entries; returns the device or an uncached error.
fn shared_device_for(filter: Option<String>) -> Result<Arc<SharedGpuDevice>, String> {
    let mut cache = device_cache()
        .lock()
        .map_err(|_| "GPU device cache lock poisoned".to_string())?;
    if let Some(found) = cache.get(&filter) {
        if !found.is_lost() {
            return Ok(found.clone());
        }
        cache.remove(&filter);
    }
    let started = std::time::Instant::now();
    let adapter = select_adapter(filter.as_deref())?;
    let info = adapter.get_info();
    let limits = adapter.limits();
    let (device, queue) = request_device(&adapter, "rustmspt shared device")?;
    let lost = Arc::new(AtomicBool::new(false));
    let flag = lost.clone();
    device.set_device_lost_callback(move |_, _| flag.store(true, Ordering::Release));
    DEVICE_CREATIONS.fetch_add(1, Ordering::Relaxed);
    super::runtime::record_init(started.elapsed());
    let class = classify_adapter(&info);
    println!(
        "[Info] GPU adapter: name={} backend={:?} type={:?} class={}{}",
        info.name,
        info.backend,
        info.device_type,
        class.as_str(),
        if class == AdapterClass::Software { " (results exact; timings not representative of GPU hardware)" } else { "" }
    );
    let shared = Arc::new(SharedGpuDevice {
        device,
        queue,
        info,
        limits,
        lost,
        pipelines: Mutex::new(HashMap::new()),
    });
    cache.insert(filter, shared.clone());
    Ok(shared)
}

// AI-FUNC-SUMMARY:
// Purpose: Probe the shared GPU device and return a GpuContext with capabilities.
// Inputs: None (uses environment variable RUSTMSPT_GPU_DEVICE for adapter selection).
// Returns: Ok(GpuContext) on success, Err(GpuInitError) when no suitable GPU is available.
// Side effects: Creates the shared device on first use for this selector; later probes reuse it.
// Notes: Uses pollster::block_on and the default power preference; RUSTMSPT_GPU_DEVICE selects a name or index.
pub fn try_init_gpu() -> Result<GpuContext, GpuInitError> {
    let shared = shared_gpu_device()?;
    Ok(GpuContext {
        adapter_name: shared.info.name.clone(),
        adapter_class: classify_adapter(&shared.info),
        adapter_backend: format!("{:?}", shared.info.backend),
        max_buffer_size: shared.limits.max_buffer_size,
        max_storage_buffer_binding_size: shared.limits.max_storage_buffer_binding_size as u64,
    })
}

// AI-FUNC-SUMMARY: Return the shared device for pipeline constructors as a String-error result; honors RUSTMSPT_GPU_DEVICE and creates no device when one is cached.
pub(crate) fn shared_device() -> Result<Arc<SharedGpuDevice>, String> {
    shared_gpu_device().map_err(|e| e.0)
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

// AI-FUNC-SUMMARY: Reuse one backend instance for the process; adapters remain fresh because each can create only one logical device, and devices are cached by shared_device_for.
fn shared_instance() -> &'static wgpu::Instance {
    static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();
    INSTANCE.get_or_init(|| wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    }))
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
    // AI-FUNC-SUMMARY: Verify a pipeline family compiles once per device, repeated lookups return the same object, and an invalid shader errors on every call without being cached.
    #[test]
    fn pipeline_cache_reuses_success_and_never_caches_failure() {
        let shared = match shared_device_for(None) {
            Ok(shared) => shared,
            Err(error) => {
                eprintln!("SKIP: GPU unavailable: {error}");
                return;
            }
        };
        let again = shared_device_for(None).unwrap();
        assert!(Arc::ptr_eq(&shared, &again));
        let source = "@compute @workgroup_size(1) fn main() {}";
        let build = |device: &wgpu::Device| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("cache probe"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("cache probe"),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let first = shared.cached_pipeline("cache_probe", source, build).unwrap();
        let second = shared
            .cached_pipeline("cache_probe", source, |_| -> wgpu::ComputePipeline {
                panic!("cached pipeline was rebuilt")
            })
            .unwrap();
        assert_eq!(first, second);
        let invalid = "@compute @workgroup_size(1) fn main() { let x: u32 = 1.5; }";
        for _ in 0..2 {
            let result = shared.cached_pipeline("cache_probe_invalid", invalid, |device| {
                let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("invalid probe"),
                    source: wgpu::ShaderSource::Wgsl(invalid.into()),
                });
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("invalid probe"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                })
            });
            assert!(result.is_err(), "invalid WGSL must fail and must not be cached");
        }
        let map = shared.pipelines.lock().unwrap();
        assert!(!map.keys().any(|(kind, _)| *kind == "cache_probe_invalid"));
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
