use crate::types::{BoundingBox, Mesh};

const WORKGROUP_SIZE: u32 = 64;

pub struct GpuVoxelPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    triangle_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    occupancy_buffer: wgpu::Buffer,
    num_triangles: u32,
    bind_group_layout: wgpu::BindGroupLayout,
}

// AI-FUNC-SUMMARY: Build normalized f32 triangle buffer from mesh; returns Vec<f32>; side effects: None.
fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32> {
    let (ox, oy, oz) = (bbox.min.x as f32, bbox.min.y as f32, bbox.min.z as f32);
    let mut buf = Vec::with_capacity(mesh.faces.len() * 9);
    for face in &mesh.faces {
        let a = mesh.vertices[face.a];
        let b = mesh.vertices[face.b];
        let c = mesh.vertices[face.c];
        buf.extend_from_slice(&[(a.x as f32) - ox, (a.y as f32) - oy, (a.z as f32) - oz]);
        buf.extend_from_slice(&[(b.x as f32) - ox, (b.y as f32) - oy, (b.z as f32) - oz]);
        buf.extend_from_slice(&[(c.x as f32) - ox, (c.y as f32) - oy, (c.z as f32) - oz]);
    }
    buf
}

impl GpuVoxelPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu and create the voxelization compute pipeline.
    // Inputs: mesh and bounding box for normalized triangle upload.
    // Returns: Ok(GpuVoxelPipeline) or init error string.
    // Side effects: Heavy wgpu device init, uploads triangle data.
    pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String> {
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
                    label: Some("rustmspt voxel device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                }, None)
                .await
                .map_err(|e| format!("device request failed: {}", e))
        })?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxelize"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/voxelize.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vox_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vox_pl"), bind_group_layouts: &[&bgl], push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("vox_pipeline"), layout: Some(&pl), module: &shader,
            entry_point: Some("main"), compilation_options: Default::default(), cache: None,
        });

        let tri_data = build_triangle_buffer(mesh, bbox);
        let tri_bytes = bytemuck::cast_slice::<f32, u8>(&tri_data);
        let triangle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("triangles"), size: tri_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        queue.write_buffer(&triangle_buffer, 0, tri_bytes);

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"), size: 48,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });

        let occupancy_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("occupancy"), size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false,
        });

        Ok(Self { device, queue, pipeline, triangle_buffer, params_buffer, occupancy_buffer,
            num_triangles: mesh.faces.len() as u32, bind_group_layout: bgl })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Compute occupancy grid on GPU via ray-casting voxelization.
    // Inputs: bounding box (normalized), voxel pitch, grid dimensions [nx, ny, nz].
    // Returns: Vec<u32> occupancy grid (1=occupied, 0=empty), length nx*ny*nz.
    // Side effects: Dispatches GPU compute, maps staging buffer.
    pub fn voxelize(&mut self, nx: u32, ny: u32, nz: u32, pitch: f32) -> Vec<u32> {
        let total = nx * ny * nz;

        let (dx, dy, dz) = crate::geometry::s2::RAY_DIR_GPU;
        let mut param_data = Vec::with_capacity(48);
        param_data.extend_from_slice(&self.num_triangles.to_le_bytes());
        param_data.extend_from_slice(&nx.to_le_bytes());
        param_data.extend_from_slice(&ny.to_le_bytes());
        param_data.extend_from_slice(&nz.to_le_bytes());
        param_data.extend_from_slice(&pitch.to_le_bytes());
        param_data.extend_from_slice(&0u32.to_le_bytes());
        param_data.extend_from_slice(&(dx as f32).to_le_bytes());
        param_data.extend_from_slice(&(dy as f32).to_le_bytes());
        param_data.extend_from_slice(&(dz as f32).to_le_bytes());
        param_data.extend_from_slice(&0u32.to_le_bytes());
        param_data.extend_from_slice(&0u32.to_le_bytes());
        param_data.extend_from_slice(&0u32.to_le_bytes());
        self.queue.write_buffer(&self.params_buffer, 0, &param_data);

        let needed = (total as u64) * 4;
        if needed > self.occupancy_buffer.size() {
            self.occupancy_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("occupancy"), size: needed,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false,
            });
        }

        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vox_bg"), layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.triangle_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.params_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: self.occupancy_buffer.as_entire_binding() },
            ],
        });

        let num_wg = total.div_ceil(WORKGROUP_SIZE);
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("vox_enc") });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("vox_pass"), timestamp_writes: None });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(num_wg, 1, 1);
        }

        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"), size: needed,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        enc.copy_buffer_to_buffer(&self.occupancy_buffer, 0, &staging, 0, needed);
        self.queue.submit(Some(enc.finish()));

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);

        let data = slice.get_mapped_range();
        let result: Vec<u32> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        result
    }
}
