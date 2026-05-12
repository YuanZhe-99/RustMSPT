use crate::types::{BoundingBox, Mesh};

const WORKGROUP_SIZE: u32 = 256;
const MAX_RADII: usize = 128;

// AI-FUNC-SUMMARY: GPU-accelerated Monte Carlo S2 pipeline using wgpu compute shaders.
// Holds the wgpu device, queue, compute pipeline, and pre-allocated buffers for triangle
// data, parameters, and per-invocation output (hit/valid counts). Call `calculate_s2_gpu`
// to dispatch the Monte Carlo kernel for all radii in a single dispatch.
pub struct GpuS2Pipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    triangle_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_hits_buffer: wgpu::Buffer,
    out_valids_buffer: wgpu::Buffer,
    num_triangles: u32,
    bind_group_layout: wgpu::BindGroupLayout,
}

// AI-FUNC-SUMMARY: Build f32 triangle position buffer with coordinates normalized to bbox origin.
// Inputs: mesh reference, bounding box for normalization.
// Returns: Vec<f32> with 9 floats per triangle, all positions shifted by -bbox.min.
// Side effects: None.
// Notes: Normalization improves f32 precision by keeping coordinates small (0..size instead of 100..1200+).
fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32> {
    let ox = bbox.min.x as f32;
    let oy = bbox.min.y as f32;
    let oz = bbox.min.z as f32;
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

// AI-FUNC-SUMMARY:
// Purpose: Pack shader parameters into a byte buffer matching the WGSL Params struct layout.
// Inputs: triangle count, radii slice, seed, bounding box (used for normalized size), sample count per radius.
// Returns: Vec<u8> matching the WGSL Params struct with std430 alignment. Bbox min is always (0,0,0).
// Side effects: None.
fn pack_params(
    num_triangles: u32,
    radii: &[f32],
    seed: u32,
    bbox: BoundingBox,
    samples_per_radius: u32,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16 + 16 + 16 + 16 + MAX_RADII * 4);

    buf.extend_from_slice(&num_triangles.to_le_bytes());
    buf.extend_from_slice(&(radii.len() as u32).to_le_bytes());
    buf.extend_from_slice(&samples_per_radius.to_le_bytes());
    buf.extend_from_slice(&seed.to_le_bytes());

    let size = bbox.size();
    buf.extend_from_slice(&0.0f32.to_le_bytes());
    buf.extend_from_slice(&0.0f32.to_le_bytes());
    buf.extend_from_slice(&0.0f32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    buf.extend_from_slice(&(size.x as f32).to_le_bytes());
    buf.extend_from_slice(&(size.y as f32).to_le_bytes());
    buf.extend_from_slice(&(size.z as f32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    let (dx, dy, dz) = super::super::geometry::s2::RAY_DIR_GPU;
    buf.extend_from_slice(&(dx as f32).to_le_bytes());
    buf.extend_from_slice(&(dy as f32).to_le_bytes());
    buf.extend_from_slice(&(dz as f32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    for i in 0..MAX_RADII {
        if i < radii.len() {
            buf.extend_from_slice(&radii[i].to_le_bytes());
        } else {
            buf.extend_from_slice(&0.0f32.to_le_bytes());
        }
    }
    buf
}

impl GpuS2Pipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu device/queue and create the Monte Carlo S2 compute pipeline.
    // Inputs: mesh reference for triangle buffer pre-upload, bounding box for coordinate normalization.
    // Returns: Ok(GpuS2Pipeline) or wgpu initialization error string.
    // Side effects: Performs heavy wgpu device initialization and uploads normalized triangle data to GPU.
    pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let device_filter = std::env::var("RUSTMSPT_GPU_DEVICE").ok();

        let adapter = pollster::block_on(async {
            if let Some(ref filter) = device_filter {
                if let Ok(idx) = filter.parse::<usize>() {
                    let adapters = instance.enumerate_adapters(wgpu::Backends::all());
                    if idx < adapters.len() {
                        return Ok(adapters.into_iter().nth(idx).unwrap());
                    }
                    return Err(format!(
                        "RUSTMSPT_GPU_DEVICE index {} out of range ({} adapters)",
                        idx,
                        instance.enumerate_adapters(wgpu::Backends::all()).len()
                    ));
                }
                let adapters = instance.enumerate_adapters(wgpu::Backends::all());
                adapters
                    .into_iter()
                    .find(|a| a.get_info().name.contains(filter.as_str()))
                    .ok_or_else(|| format!("no adapter matching '{}'", filter))
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
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("rustmspt s2 compute device"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .map_err(|e| format!("device request failed: {}", e))
        })?;

        let shader_source = include_str!("shaders/s2_monte_carlo.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("s2_monte_carlo"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("s2_mc_bgl"),
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("s2_mc_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("s2_mc_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let tri_data = build_triangle_buffer(mesh, bbox);
        let tri_bytes = bytemuck::cast_slice::<f32, u8>(&tri_data);
        let triangle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("triangles"),
            size: tri_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&triangle_buffer, 0, tri_bytes);

        let max_invocations = MAX_RADII as u32 * 40_000;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: (16 + 16 + 16 + 16 + MAX_RADII * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let out_size = (max_invocations as u64) * 4;
        let out_hits_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out_hits"),
            size: out_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let out_valids_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out_valids"),
            size: out_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            triangle_buffer,
            params_buffer,
            out_hits_buffer,
            out_valids_buffer,
            num_triangles: mesh.faces.len() as u32,
            bind_group_layout,
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Update the triangle buffer for a new mesh without recreating the pipeline.
    // Inputs: mesh and bounding box for normalized triangle data.
    // Returns: None.
    // Side effects: Re-uploads triangle data to GPU. Allocates new buffer if mesh size changed.
    pub fn update_mesh(&mut self, mesh: &Mesh, bbox: BoundingBox) {
        let tri_data = build_triangle_buffer(mesh, bbox);
        let tri_bytes = bytemuck::cast_slice::<f32, u8>(&tri_data);
        let needed = tri_bytes.len() as u64;
        if needed > self.triangle_buffer.size() {
            self.triangle_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("triangles"),
                size: needed,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        self.queue.write_buffer(&self.triangle_buffer, 0, tri_bytes);
        self.num_triangles = mesh.faces.len() as u32;
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Allocate or resize output buffers if the new invocation count exceeds current capacity.
    // Inputs: new invocation count.
    // Returns: None.
    // Side effects: May reallocate GPU buffers if capacity is exceeded.
    fn ensure_output_capacity(&mut self, invocations: u32) {
        let needed = (invocations as u64) * 4;
        if needed > self.out_hits_buffer.size() {
            self.out_hits_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out_hits"),
                size: needed,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            self.out_valids_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out_valids"),
                size: needed,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Compute Monte Carlo S2 two-point correlation on the GPU for all radii.
    // Inputs: bounding box, r_max (inclusive), sample count per radius.
    // Returns: Vec<f64> of S2 values indexed by radius (r=0..r_max).
    // Side effects: Dispatches GPU compute work; maps staging buffers for readback.
    // Notes: r=0 is set to 0.0 (caller should overwrite with volume fraction). Uses f32 on GPU for positions; results are f64 on CPU.
    pub fn calculate_s2_gpu(
        &mut self,
        bbox: BoundingBox,
        r_max: usize,
        samples: usize,
    ) -> Vec<f64> {
        let radii: Vec<f32> = (0..=r_max).map(|r| r as f32).collect();
        let num_radii = radii.len() as u32;
        let samples_per_radius = samples.max(200) as u32;
        let total_invocations = num_radii * samples_per_radius;

        self.ensure_output_capacity(total_invocations);

        let seed = rand::random::<u32>();
        let param_data = pack_params(
            self.num_triangles,
            &radii,
            seed,
            bbox,
            samples_per_radius,
        );
        self.queue.write_buffer(&self.params_buffer, 0, &param_data);

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("s2_mc_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.triangle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.out_hits_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.out_valids_buffer.as_entire_binding(),
                },
            ],
        });

        let num_workgroups = total_invocations.div_ceil(WORKGROUP_SIZE);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("s2_mc_encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("s2_mc_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(num_workgroups, 1, 1);
        }

        let readback_size = (total_invocations as u64) * 4;
        let staging_hits = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_hits"),
            size: readback_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_valids = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_valids"),
            size: readback_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        encoder.copy_buffer_to_buffer(
            &self.out_hits_buffer,
            0,
            &staging_hits,
            0,
            readback_size,
        );
        encoder.copy_buffer_to_buffer(
            &self.out_valids_buffer,
            0,
            &staging_valids,
            0,
            readback_size,
        );
        self.queue.submit(Some(encoder.finish()));

        let hits_slice = staging_hits.slice(..);
        let valids_slice = staging_valids.slice(..);
        hits_slice.map_async(wgpu::MapMode::Read, |_| {});
        valids_slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);

        let hits_data = hits_slice.get_mapped_range();
        let valids_data = valids_slice.get_mapped_range();

        let hits_u32: &[u32] = bytemuck::cast_slice(&hits_data);
        let valids_u32: &[u32] = bytemuck::cast_slice(&valids_data);

        let mut out = vec![0.0f64; r_max + 1];
        let sp = samples_per_radius as usize;
        for (r, slot) in out.iter_mut().enumerate().take(r_max + 1) {
            let start = r * sp;
            let end = ((r + 1) * sp).min(total_invocations as usize);
            if start >= end {
                continue;
            }
            let total_hits: u64 = hits_u32[start..end].iter().map(|&v| v as u64).sum();
            let total_valid: u64 = valids_u32[start..end].iter().map(|&v| v as u64).sum();
            if total_valid > 0 {
                *slot = total_hits as f64 / total_valid as f64;
            }
        }

        drop(hits_data);
        drop(valids_data);
        out
    }
}
