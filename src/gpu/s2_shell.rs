const WORKGROUP_SIZE: u32 = 256;
const MAX_OFFSETS: usize = 200_000;

#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct OffsetEntry {
    radius_idx: u32,
    dx: i32,
    dy: i32,
    dz: i32,
}

pub struct GpuShellS2Pipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    occupancy_buffer: wgpu::Buffer,
    offsets_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_valid_buffer: wgpu::Buffer,
    out_hits_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
}

// AI-FUNC-SUMMARY:
// Purpose: Build flat offset entry buffer from shell offsets per radius.
// Inputs: slice of (radius_idx, [dx, dy, dz]) tuples.
// Returns: Vec<OffsetEntry> and total count.
// Side effects: None.
fn build_offset_buffer(shell_offsets: &[(u32, [isize; 3])]) -> Vec<OffsetEntry> {
    shell_offsets
        .iter()
        .map(|&(ridx, [dx, dy, dz])| OffsetEntry {
            radius_idx: ridx,
            dx: dx as i32,
            dy: dy as i32,
            dz: dz as i32,
        })
        .collect()
}

impl GpuShellS2Pipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu and create the shell S2 compute pipeline.
    // Inputs: none (device/queue created fresh).
    // Returns: Ok(GpuShellS2Pipeline) or init error string.
    // Side effects: Heavy wgpu device init.
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(async {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::default(),
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .ok_or_else(|| "no suitable GPU adapter".to_string())
        })?;
        let (device, queue) = pollster::block_on(async {
            adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("rustmspt shell s2 device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                }, None)
                .await
                .map_err(|e| format!("device request failed: {}", e))
        })?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("s2_shell"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/s2_shell_pairs.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shell_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shell_pl"), bind_group_layouts: &[&bgl], push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("shell_pipeline"), layout: Some(&pl), module: &shader,
            entry_point: Some("main"), compilation_options: Default::default(), cache: None,
        });

        let occupancy_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("occupancy"), size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let offsets_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offsets"), size: (MAX_OFFSETS * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"), size: 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let out_size = (MAX_OFFSETS as u64) * 4;
        let out_valid_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out_valid"), size: out_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false,
        });
        let out_hits_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out_hits"), size: out_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false,
        });

        Ok(Self { device, queue, pipeline, occupancy_buffer, offsets_buffer, params_buffer,
            out_valid_buffer, out_hits_buffer, bind_group_layout: bgl })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Compute exact S2 via direct shell pair counting on GPU.
    // Inputs: occupancy grid (u32), grid dimensions, shell offsets per radius, r_max, voxel pitch, volume fraction.
    // Returns: Vec<f64> of S2 values for r=0..r_max.
    // Side effects: Dispatches GPU compute, maps staging buffers.
    pub fn compute_s2_shell(
        &mut self,
        occ: &[u32],
        nx: u32,
        ny: u32,
        nz: u32,
        shell_offsets: &[(u32, [isize; 3])],
        r_max: usize,
        _voxel_pitch: f64,
        vf: f64,
    ) -> Vec<f64> {
        let total_offsets = shell_offsets.len().min(MAX_OFFSETS) as u32;

        let occ_bytes = bytemuck::cast_slice::<u32, u8>(occ);
        let needed_occ = occ_bytes.len() as u64;
        if needed_occ > self.occupancy_buffer.size() {
            self.occupancy_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("occupancy"), size: needed_occ,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
            });
        }
        self.queue.write_buffer(&self.occupancy_buffer, 0, occ_bytes);

        let entries = build_offset_buffer(shell_offsets);
        let entries_bytes = bytemuck::cast_slice::<OffsetEntry, u8>(&entries);
        self.queue.write_buffer(&self.offsets_buffer, 0, entries_bytes);

        let mut param_data = Vec::with_capacity(16);
        param_data.extend_from_slice(&total_offsets.to_le_bytes());
        param_data.extend_from_slice(&nx.to_le_bytes());
        param_data.extend_from_slice(&ny.to_le_bytes());
        param_data.extend_from_slice(&nz.to_le_bytes());
        self.queue.write_buffer(&self.params_buffer, 0, &param_data);

        let out_needed = (total_offsets as u64) * 4;
        if out_needed > self.out_valid_buffer.size() {
            self.out_valid_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out_valid"), size: out_needed,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false,
            });
            self.out_hits_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out_hits"), size: out_needed,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false,
            });
        }

        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shell_bg"), layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.occupancy_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.offsets_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: self.params_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: self.out_valid_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: self.out_hits_buffer.as_entire_binding() },
            ],
        });

        let num_wg = total_offsets.div_ceil(WORKGROUP_SIZE);
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("shell_enc") });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("shell_pass"), timestamp_writes: None });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(num_wg, 1, 1);
        }

        let staging_v = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stg_v"), size: out_needed,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let staging_h = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stg_h"), size: out_needed,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        enc.copy_buffer_to_buffer(&self.out_valid_buffer, 0, &staging_v, 0, out_needed);
        enc.copy_buffer_to_buffer(&self.out_hits_buffer, 0, &staging_h, 0, out_needed);
        self.queue.submit(Some(enc.finish()));

        let sv = staging_v.slice(..);
        let sh = staging_h.slice(..);
        sv.map_async(wgpu::MapMode::Read, |_| {});
        sh.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);

        let valid_data = sv.get_mapped_range();
        let hits_data = sh.get_mapped_range();
        let valid_u32: &[u32] = bytemuck::cast_slice(&valid_data);
        let hits_u32: &[u32] = bytemuck::cast_slice(&hits_data);

        let mut shell_sums = vec![0.0f64; r_max + 1];
        let mut shell_counts = vec![0usize; r_max + 1];

        for i in 0..total_offsets as usize {
            let ridx = shell_offsets[i].0 as usize;
            if ridx <= r_max && valid_u32[i] > 0 {
                shell_sums[ridx] += hits_u32[i] as f64 / valid_u32[i] as f64;
                shell_counts[ridx] += 1;
            }
        }

        let mut out = vec![0.0f64; r_max + 1];
        out[0] = vf;
        for r in 1..=r_max {
            if shell_counts[r] > 0 {
                out[r] = shell_sums[r] / shell_counts[r] as f64;
            }
        }

        drop(valid_data);
        drop(hits_data);
        out
    }
}
