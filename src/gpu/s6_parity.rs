use super::runtime::CountedWrite;

const WORKGROUP_SIZE: u32 = 64;
const MAX_BATCH: usize = 1 << 20;

pub use crate::meshgen::classify::ParityGeometry;

pub struct GpuParityPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    adapter: String,
}

impl GpuParityPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Compile (or reuse) S6's certified parity compute pipeline on the shared device.
    // Inputs: none.
    // Returns: The pipeline, or the device/compile error as a string.
    // Side effects: May create the process-wide shared device.
    pub fn new() -> Result<Self, String> {
        let shared = super::context::shared_device()?;
        let (device, queue) = (shared.device().clone(), shared.queue().clone());
        let adapter = format!("{} ({:?})", shared.info().name, shared.info().backend);
        let source = include_str!("shaders/s6_parity.wgsl");
        super::runtime::scoped(&device.clone(), || {
            let (pipeline, layout) = shared.cached_pipeline("s6_parity", source, |device| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("s6_parity"),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });
                let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                };
                let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("s6_parity_bgl"),
                    entries: &[
                        storage(0, true),
                        storage(1, true),
                        storage(2, true),
                        storage(3, true),
                        storage(4, true),
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        storage(6, false),
                    ],
                });
                let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("s6_parity_pl"),
                    bind_group_layouts: &[&layout],
                    push_constant_ranges: &[],
                });
                let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("s6_parity"),
                    layout: Some(&pl),
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
                (pipeline, layout)
            })?;
            Ok(Self { device: device.clone(), queue, pipeline, layout, adapter })
        })
    }

    // AI-FUNC-SUMMARY: One-line name and backend of the adapter the pipeline runs on; returns String; side effects: none.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    // AI-FUNC-SUMMARY:
    // Purpose: For every point, its certified parity against one solid along one ray, on the GPU.
    // Inputs: the solid's geometry, the points as flat xyz f32, and each point's bucket (computed by
    //   the host in f64, so the candidate set is exactly the CPU's).
    // Returns: one code per point - 0 outside, 1 inside, 2 uncertain (decide on the CPU) - or an
    //   error for buffers beyond device limits or a device failure.
    // Side effects: Uploads the geometry once, then dispatches and reads back one batch at a time.
    pub fn classify(&self, geometry: &ParityGeometry, points: &[f32], buckets: &[u32]) -> Result<Vec<u32>, String> {
        let n = buckets.len();
        if points.len() != n * 3 {
            return Err("points and buckets disagree in length".into());
        }
        let limits = self.device.limits();
        let binding = u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size);
        for (name, bytes) in [
            ("triangles", geometry.triangles.len() * 4),
            ("bucket offsets", geometry.bucket_start.len() * 4),
            ("bucket items", geometry.items.len() * 4),
        ] {
            if bytes as u64 > binding {
                return Err(format!("{name} need {bytes} bytes, above the device's {binding}"));
            }
        }
        let cap = ((binding / 12) as usize).min(MAX_BATCH).min(n.max(1));
        let mut out = Vec::with_capacity(n);
        super::runtime::scoped(&self.device.clone(), || {
            use wgpu::util::DeviceExt;
            let upload = |label: &str, bytes: &[u8]| {
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: if bytes.is_empty() { &[0u8; 4] } else { bytes },
                    usage: wgpu::BufferUsages::STORAGE,
                })
            };
            let tris_buf = upload("s6_tris", bytemuck::cast_slice(geometry.triangles));
            let start_buf = upload("s6_bucket_start", bytemuck::cast_slice(geometry.bucket_start));
            let items_buf = upload("s6_bucket_items", bytemuck::cast_slice(geometry.items));
            let input = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
            let points_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("s6_points"),
                size: (cap * 12) as u64,
                usage: input,
                mapped_at_creation: false,
            });
            let bucket_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("s6_point_bucket"),
                size: (cap * 4) as u64,
                usage: input,
                mapped_at_creation: false,
            });
            let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("s6_params"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let result_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("s6_result"),
                size: (cap * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("s6_staging"),
                size: (cap * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("s6_bg"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: tris_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: start_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: items_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: points_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: bucket_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 5, resource: params_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 6, resource: result_buf.as_entire_binding() },
                ],
            });
            for start in (0..n).step_by(cap) {
                let m = cap.min(n - start);
                self.queue.write_counted(&points_buf, 0, bytemuck::cast_slice(&points[start * 3..(start + m) * 3]));
                self.queue.write_counted(&bucket_buf, 0, bytemuck::cast_slice(&buckets[start..start + m]));
                let groups = (m as u32).div_ceil(WORKGROUP_SIZE);
                let x = groups.min(limits.max_compute_workgroups_per_dimension);
                let y = groups.div_ceil(x);
                let mut params = Vec::with_capacity(32);
                for v in [m as u32, x * WORKGROUP_SIZE, 0, 0] {
                    params.extend_from_slice(&v.to_le_bytes());
                }
                for v in [geometry.direction[0], geometry.direction[1], geometry.direction[2], geometry.reach] {
                    params.extend_from_slice(&v.to_le_bytes());
                }
                self.queue.write_counted(&params_buf, 0, &params);
                let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("s6_enc") });
                {
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("s6_pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.dispatch_workgroups(x, y, 1);
                }
                enc.copy_buffer_to_buffer(&result_buf, 0, &staging, 0, (m * 4) as u64);
                self.queue.submit(Some(enc.finish()));
                let words = super::runtime::read_u32_prefix(&self.device, &staging, (m * 4) as u64)?;
                out.extend_from_slice(&words[..m]);
            }
            Ok(())
        })?;
        Ok(out)
    }
}

impl crate::meshgen::classify::ParityAccelerator for GpuParityPipeline {
    // AI-FUNC-SUMMARY: S6's accelerator hook: the certified parity codes for these points on the GPU; returns codes or the device error; side effects: GPU dispatches.
    fn codes(&self, geometry: &ParityGeometry, points: &[f32], buckets: &[u32]) -> Result<Vec<u32>, String> {
        self.classify(geometry, points, buckets)
    }

    // AI-FUNC-SUMMARY: The adapter's name for the stage log; returns String; side effects: none.
    fn describe(&self) -> String {
        self.adapter().to_string()
    }
}
