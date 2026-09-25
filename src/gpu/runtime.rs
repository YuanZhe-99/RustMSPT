static SCOPE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Process-wide GPU transfer accounting (PERF-00): every upload through `CountedWrite`, every mapped readback,
/// the time spent blocked waiting for a readback (GPU execution plus transfer, never reported as kernel time)
/// and shared-device initialization.
static UPLOAD_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static UPLOAD_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static READBACK_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static READBACK_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static WAIT_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static INIT_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A snapshot of the process-wide GPU transfer counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuTransferStats {
    pub upload_bytes: u64,
    pub upload_calls: u64,
    pub readback_bytes: u64,
    pub readback_calls: u64,
    pub wait_nanos: u64,
    pub init_nanos: u64,
}

impl GpuTransferStats {
    // AI-FUNC-SUMMARY: One `[Timing] gpu ...` line with every counter (wait = blocked on readback: execution plus transfer); returns String; side effects: none.
    pub fn describe(&self) -> String {
        format!(
            "[Timing] gpu init_seconds={:.6} upload_bytes={} upload_calls={} readback_bytes={} readback_calls={} readback_wait_seconds={:.6}",
            self.init_nanos as f64 * 1e-9,
            self.upload_bytes,
            self.upload_calls,
            self.readback_bytes,
            self.readback_calls,
            self.wait_nanos as f64 * 1e-9
        )
    }
}

// AI-FUNC-SUMMARY: Snapshot the process-wide GPU transfer counters; returns GpuTransferStats; side effects: none.
pub fn gpu_transfer_stats() -> GpuTransferStats {
    use std::sync::atomic::Ordering::Relaxed;
    GpuTransferStats {
        upload_bytes: UPLOAD_BYTES.load(Relaxed),
        upload_calls: UPLOAD_CALLS.load(Relaxed),
        readback_bytes: READBACK_BYTES.load(Relaxed),
        readback_calls: READBACK_CALLS.load(Relaxed),
        wait_nanos: WAIT_NANOS.load(Relaxed),
        init_nanos: INIT_NANOS.load(Relaxed),
    }
}

// AI-FUNC-SUMMARY: Add one readback (bytes, blocked wait) to the process counters; returns nothing; side effects: atomic adds.
pub(super) fn record_readback(bytes: u64, wait: std::time::Duration) {
    use std::sync::atomic::Ordering::Relaxed;
    READBACK_BYTES.fetch_add(bytes, Relaxed);
    READBACK_CALLS.fetch_add(1, Relaxed);
    WAIT_NANOS.fetch_add(wait.as_nanos() as u64, Relaxed);
}

// AI-FUNC-SUMMARY: Add shared-device initialization time to the process counters; returns nothing; side effects: atomic add.
pub(super) fn record_init(elapsed: std::time::Duration) {
    INIT_NANOS.fetch_add(elapsed.as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
}

/// `queue.write_buffer` that also counts the upload; every GPU upload in the crate goes through it.
pub(super) trait CountedWrite {
    fn write_counted(&self, buffer: &wgpu::Buffer, offset: wgpu::BufferAddress, data: &[u8]);
}

impl CountedWrite for wgpu::Queue {
    // AI-FUNC-SUMMARY: Queue the write and count its bytes; returns nothing; side effects: queues a GPU upload, atomic adds.
    fn write_counted(&self, buffer: &wgpu::Buffer, offset: wgpu::BufferAddress, data: &[u8]) {
        use std::sync::atomic::Ordering::Relaxed;
        UPLOAD_BYTES.fetch_add(data.len() as u64, Relaxed);
        UPLOAD_CALLS.fetch_add(1, Relaxed);
        self.write_buffer(buffer, offset, data);
    }
}


thread_local! {
    static SCOPE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

struct ScopeDepth;

impl Drop for ScopeDepth {
    // AI-FUNC-SUMMARY: Leave one nesting level of the calling thread's error-scope region, including during unwinding; returns None; side effects: decrements the thread-local depth.
    fn drop(&mut self) {
        SCOPE_DEPTH.with(|d| d.set(d.get() - 1));
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Run a synchronous GPU operation under balanced validation, allocation and internal error scopes.
// Inputs: device whose scope stack is used; work closure issuing GPU commands.
// Returns: The operation result, or the captured device errors joined into one string.
// Side effects: Holds a process-wide reentrant lock for the outermost scope on each thread, polls the device.
// Notes: wgpu 24 error scopes are per device, not per thread, and devices are shared process-wide, so
// outermost regions are serialized to keep another thread's pushes/pops from capturing this thread's
// errors. Nested calls on the same thread reuse the held lock. Rust panics are not caught.
pub(super) fn scoped<T>(
    device: &wgpu::Device,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _lock = if SCOPE_DEPTH.with(|d| d.get()) == 0 {
        Some(SCOPE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
    } else {
        None
    };
    SCOPE_DEPTH.with(|d| d.set(d.get() + 1));
    let _depth = ScopeDepth;
    for filter in [
        wgpu::ErrorFilter::Validation,
        wgpu::ErrorFilter::OutOfMemory,
        wgpu::ErrorFilter::Internal,
    ] {
        device.push_error_scope(filter);
    }
    let result = work();
    device.poll(wgpu::Maintain::Wait);
    let mut errors = Vec::new();
    for _ in 0..3 {
        if let Some(error) = pollster::block_on(device.pop_error_scope()) {
            errors.push(error.to_string());
        }
    }
    if errors.is_empty() {
        result
    } else {
        Err(errors.join("; "))
    }
}

// AI-FUNC-SUMMARY: Map a u32 staging buffer, check the callback result before reading, copy its contents and unmap; return data or a descriptive mapping/channel error.
#[cfg(test)]
pub(super) fn read_u32(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<u32>, String> {
    read_u32_prefix(device, buffer, buffer.size())
}

// AI-FUNC-SUMMARY: Map only a validated live prefix of reusable staging storage, check mapping success and unmap after copying.
pub(super) fn read_u32_prefix(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    bytes: u64,
) -> Result<Vec<u32>, String> {
    if bytes == 0 || bytes % 4 != 0 || bytes > buffer.size() {
        return Err("GPU readback prefix is empty, unaligned or exceeds capacity".into());
    }
    let slice = buffer.slice(..bytes);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let waited = std::time::Instant::now();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::Maintain::Wait);
    receiver
        .recv()
        .map_err(|error| format!("GPU map callback unavailable: {error}"))?
        .map_err(|error| format!("GPU readback failed: {error}"))?;
    record_readback(bytes, waited.elapsed());
    let data = slice.get_mapped_range();
    let result = bytemuck::cast_slice(&data).to_vec();
    drop(data);
    buffer.unmap();
    Ok(result)
}

// AI-FUNC-SUMMARY: Describe checked grid storage and a two-dimensional compute dispatch without allocating buffers.
pub(super) struct GridPlan {
    pub total: u32,
    pub bytes: u64,
    pub dispatch: [u32; 2],
}

// AI-FUNC-SUMMARY: Validate nonempty grid arithmetic, storage limits and padded dispatch indexing; returns a bounded two-dimensional dispatch or a capacity error.
pub(super) fn grid_plan(
    dims: [u32; 3],
    width: u32,
    limits: &wgpu::Limits,
) -> Result<GridPlan, String> {
    if dims.contains(&0) || width == 0 {
        return Err("GPU grid dimensions and workgroup width must be positive".into());
    }
    let total = dims
        .into_iter()
        .try_fold(1u32, |a, b| a.checked_mul(b))
        .ok_or("GPU grid cell count exceeds u32")?;
    let bytes = u64::from(total) * 4;
    if bytes > limits.max_buffer_size || bytes > u64::from(limits.max_storage_buffer_binding_size) {
        return Err(format!(
            "GPU grid requires {bytes} bytes, exceeding device buffer limits"
        ));
    }
    let groups = total.div_ceil(width);
    let x = groups.min(limits.max_compute_workgroups_per_dimension);
    if x == 0 {
        return Err("GPU has no compute dispatch capacity".into());
    }
    let y = groups.div_ceil(x);
    if y > limits.max_compute_workgroups_per_dimension
        || u64::from(x) * u64::from(y) * u64::from(width) > u64::from(u32::MAX)
    {
        return Err("GPU padded dispatch exceeds coordinate limits".into());
    }
    Ok(GridPlan {
        total,
        bytes,
        dispatch: [x, y],
    })
}

#[cfg(test)]
mod grid_tests {
    use super::*;
    // AI-FUNC-SUMMARY: Verify grid product, buffer and padded dispatch limits using synthetic limits without GPU allocation.
    #[test]
    fn grid_limits_and_dispatch_coverage() {
        let mut limits = wgpu::Limits {
            max_compute_workgroups_per_dimension: 2,
            ..Default::default()
        };
        let plan = grid_plan([4, 4, 12], 64, &limits).unwrap();
        assert_eq!(plan.dispatch, [2, 2]);
        assert_eq!(plan.total, 192);
        assert!(grid_plan([4, 4, 17], 64, &limits).is_err());
        assert!(grid_plan([u32::MAX, 2, 1], 64, &limits).is_err());
        assert!(grid_plan([0, 1, 1], 64, &limits).is_err());
        limits.max_storage_buffer_binding_size = 767;
        assert!(grid_plan([4, 4, 12], 64, &limits).is_err());
    }
}
