use nalgebra::{Matrix3, Vector3};

const WORKGROUP_SIZE: u32 = 64;

pub struct GpuVolumeTransformPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    src_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    current_src_size: u64,
    current_out_size: u64,
}

impl GpuVolumeTransformPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu and create the volume transform compute pipeline.
    // Inputs: none.
    // Returns: Ok(GpuVolumeTransformPipeline) or init error string.
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
                    label: Some("rustmspt volume transform device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                }, None)
                .await
                .map_err(|e| format!("device request failed: {}", e))
        })?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("volume_transform"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/volume_transform.wgsl").into(),
            ),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vt_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vt_pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("vt_pipeline"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let src_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("src"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: 128,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            src_buffer,
            params_buffer,
            out_buffer,
            bind_group_layout: bgl,
            current_src_size: 0,
            current_out_size: 0,
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Rotate and crop a 3D volume on GPU.
    // Inputs: source volume data (i32), dimensions, background value, rotation matrix, centroid,
    //         output origin (x0,y0,z0), output dimensions, interpolation mode (0=nearest, 1=trilinear).
    // Returns: Vec<i32> of output volume data.
    // Side effects: Dispatches GPU compute, maps staging buffer.
    pub fn rotate_and_crop(
        &mut self,
        src_data: &[i32],
        src_w: u32,
        src_h: u32,
        src_d: u32,
        background: i32,
        rot: &Matrix3<f64>,
        centroid: &Vector3<f64>,
        origin: &Vector3<f64>,
        out_w: u32,
        out_h: u32,
        out_d: u32,
        interp_mode: u32,
    ) -> Vec<i32> {
        let src_bytes = bytemuck::cast_slice::<i32, u8>(src_data);
        let src_size = src_bytes.len() as u64;
        if src_size > self.current_src_size {
            self.src_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("src"),
                size: src_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.current_src_size = src_size;
        }
        self.queue.write_buffer(&self.src_buffer, 0, src_bytes);

        let out_total = (out_w * out_h * out_d) as u64 * 4;
        if out_total > self.current_out_size {
            self.out_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out"),
                size: out_total,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            self.current_out_size = out_total;
        }

        let mut param_data = Vec::with_capacity(128);
        param_data.extend_from_slice(&src_w.to_le_bytes());
        param_data.extend_from_slice(&src_h.to_le_bytes());
        param_data.extend_from_slice(&src_d.to_le_bytes());
        param_data.extend_from_slice(&out_w.to_le_bytes());
        param_data.extend_from_slice(&out_h.to_le_bytes());
        param_data.extend_from_slice(&out_d.to_le_bytes());
        param_data.extend_from_slice(&interp_mode.to_le_bytes());
        param_data.extend_from_slice(&background.to_le_bytes());

        // Rotation matrix (row-major, f32, with padding for vec4 alignment)
        param_data.extend_from_slice(&(rot[(0, 0)] as f32).to_le_bytes());
        param_data.extend_from_slice(&(rot[(0, 1)] as f32).to_le_bytes());
        param_data.extend_from_slice(&(rot[(0, 2)] as f32).to_le_bytes());
        param_data.extend_from_slice(&0.0f32.to_le_bytes());
        param_data.extend_from_slice(&(rot[(1, 0)] as f32).to_le_bytes());
        param_data.extend_from_slice(&(rot[(1, 1)] as f32).to_le_bytes());
        param_data.extend_from_slice(&(rot[(1, 2)] as f32).to_le_bytes());
        param_data.extend_from_slice(&0.0f32.to_le_bytes());
        param_data.extend_from_slice(&(rot[(2, 0)] as f32).to_le_bytes());
        param_data.extend_from_slice(&(rot[(2, 1)] as f32).to_le_bytes());
        param_data.extend_from_slice(&(rot[(2, 2)] as f32).to_le_bytes());
        param_data.extend_from_slice(&0.0f32.to_le_bytes());

        // Centroid (vec4)
        param_data.extend_from_slice(&(centroid.x as f32).to_le_bytes());
        param_data.extend_from_slice(&(centroid.y as f32).to_le_bytes());
        param_data.extend_from_slice(&(centroid.z as f32).to_le_bytes());
        param_data.extend_from_slice(&0.0f32.to_le_bytes());

        // Origin (vec4)
        param_data.extend_from_slice(&(origin.x as f32).to_le_bytes());
        param_data.extend_from_slice(&(origin.y as f32).to_le_bytes());
        param_data.extend_from_slice(&(origin.z as f32).to_le_bytes());
        param_data.extend_from_slice(&0.0f32.to_le_bytes());

        self.queue.write_buffer(&self.params_buffer, 0, &param_data);

        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vt_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.src_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.out_buffer.as_entire_binding(),
                },
            ],
        });

        let wg_x = out_w.div_ceil(WORKGROUP_SIZE);
        let wg_y = out_h;
        let wg_z = out_d;

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("vt_enc"),
            });
        {
            let mut pass =
                enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("vt_pass"),
                    timestamp_writes: None,
                });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(wg_x, wg_y, wg_z);
        }

        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: out_total,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        enc.copy_buffer_to_buffer(&self.out_buffer, 0, &staging, 0, out_total);
        self.queue.submit(Some(enc.finish()));

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);

        let data = slice.get_mapped_range();
        let result: Vec<i32> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        result
    }
}
