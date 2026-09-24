// AI-FUNC-SUMMARY: Run a synchronous GPU operation under balanced validation, allocation and internal error scopes; return the operation result or a captured device error, without catching Rust panics.
pub(super) fn scoped<T>(
    device: &wgpu::Device,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
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
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::Maintain::Wait);
    receiver
        .recv()
        .map_err(|error| format!("GPU map callback unavailable: {error}"))?
        .map_err(|error| format!("GPU readback failed: {error}"))?;
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
