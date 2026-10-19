use super::runtime::CountedWrite;
use crate::geometry::closest::{Kind, PrimBvh, Primitives};

const WORKGROUP_SIZE: u32 = 64;
const MAX_BATCH: usize = 1 << 20;
const MAX_STACK_DEPTH: usize = 60;

pub struct GpuClosestSeedPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    adapter: String,
}

impl GpuClosestSeedPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Compile (or reuse) the closest-seed compute pipeline on the shared device.
    // Inputs: none.
    // Returns: The pipeline, or the device/compile error as a string.
    // Side effects: May create the process-wide shared device.
    pub fn new() -> Result<Self, String> {
        let shared = super::context::shared_device()?;
        let (device, queue) = (shared.device().clone(), shared.queue().clone());
        let adapter = format!("{} ({:?})", shared.info().name, shared.info().backend);
        let source = include_str!("shaders/closest_candidates.wgsl");
        super::runtime::scoped(&device.clone(), || {
            let (pipeline, layout) = shared.cached_pipeline("closest_candidates", source, |device| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("closest_candidates"),
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
                    label: Some("closest_candidates_bgl"),
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
                    label: Some("closest_candidates_pl"),
                    bind_group_layouts: &[&layout],
                    push_constant_ranges: &[],
                });
                let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("closest_candidates"),
                    layout: Some(&pl),
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
                (pipeline, layout)
            })?;
            Ok(Self {
                device: device.clone(),
                queue,
                pipeline,
                layout,
                adapter,
            })
        })
    }

    // AI-FUNC-SUMMARY: One-line name and backend of the adapter the pipeline runs on; returns String; side effects: none.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    // AI-FUNC-SUMMARY:
    // Purpose: For every query point, the primitive the GPU finds nearest in f32: a seed for the exact host search.
    // Inputs: primitives, their BVH, query points as flat xyz f64.
    // Returns: one primitive index per point (u32::MAX when none was found), or an error for a BVH too deep, buffers beyond device limits, or a device failure.
    // Side effects: Uploads the BVH and the primitives' corners once, then dispatches and reads back one batch of points at a time.
    // Notes: Coordinates are taken relative to the centre of the root box in f64 before rounding to f32, so far-off meshes keep their precision. Box distances are not rounded outward: a seed only bounds the host's search and never decides it.
    pub fn seeds(&self, prims: &Primitives, bvh: &PrimBvh, points: &[f64]) -> Result<Vec<u32>, String> {
        if bvh.depth > MAX_STACK_DEPTH {
            return Err(format!("BVH depth {} exceeds the shader stack", bvh.depth));
        }
        let n = points.len() / 3;
        if prims.is_empty() || n == 0 {
            return Ok(vec![u32::MAX; n]);
        }
        let origin: [f64; 3] = std::array::from_fn(|d| 0.5 * (bvh.node_min[0][d] + bvh.node_max[0][d]));
        let rel = |x: f64, d: usize| (x - origin[d]) as f32;
        let node_box: Vec<f32> = bvh
            .node_min
            .iter()
            .zip(&bvh.node_max)
            .flat_map(|(a, b)| [rel(a[0], 0), rel(a[1], 1), rel(a[2], 2), rel(b[0], 0), rel(b[1], 1), rel(b[2], 2)])
            .collect();
        let node_link: Vec<u32> = bvh.link.iter().flat_map(|l| [l[0], l[1]]).collect();
        let slot_corner: Vec<f32> = bvh
            .order
            .iter()
            .flat_map(|&i| {
                let c = prims.corner_points(i as usize);
                (0..9).map(move |k| (c[k / 3][k % 3], k % 3))
            })
            .map(|(x, d)| rel(x, d))
            .collect();
        let limits = self.device.limits();
        let binding = u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size);
        for (name, bytes) in [("node boxes", node_box.len() * 4), ("primitive corners", slot_corner.len() * 4)] {
            if bytes as u64 > binding {
                return Err(format!("{name} need {bytes} bytes, above the device's {binding}"));
            }
        }
        let cap = ((binding / 12) as usize).min(MAX_BATCH).min(n);
        let mut seeds = Vec::with_capacity(n);
        super::runtime::scoped(&self.device.clone(), || {
            use wgpu::util::DeviceExt;
            let upload = |label: &str, bytes: &[u8]| {
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytes,
                    usage: wgpu::BufferUsages::STORAGE,
                })
            };
            let node_box_buf = upload("cs_node_box", bytemuck::cast_slice(&node_box));
            let node_link_buf = upload("cs_node_link", bytemuck::cast_slice(&node_link));
            let corner_buf = upload("cs_slot_corner", bytemuck::cast_slice(&slot_corner));
            let prim_buf = upload("cs_slot_prim", bytemuck::cast_slice(&bvh.order));
            let points_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cs_points"),
                size: (cap * 12) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cs_params"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let seeds_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cs_seeds"),
                size: (cap * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cs_staging"),
                size: (cap * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cs_bg"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: node_box_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: node_link_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: corner_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: prim_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: points_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 5, resource: params_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 6, resource: seeds_buf.as_entire_binding() },
                ],
            });
            let triangles = u32::from(prims.kind == Kind::Triangles);
            for start in (0..n).step_by(cap) {
                let m = cap.min(n - start);
                let p32: Vec<f32> = points[start * 3..(start + m) * 3]
                    .iter()
                    .enumerate()
                    .map(|(k, &x)| rel(x, k % 3))
                    .collect();
                self.queue.write_counted(&points_buf, 0, bytemuck::cast_slice(&p32));
                let groups = (m as u32).div_ceil(WORKGROUP_SIZE);
                let x = groups.min(limits.max_compute_workgroups_per_dimension);
                let y = groups.div_ceil(x);
                let mut params = Vec::with_capacity(16);
                for v in [m as u32, triangles, x * WORKGROUP_SIZE, 0] {
                    params.extend_from_slice(&v.to_le_bytes());
                }
                self.queue.write_counted(&params_buf, 0, &params);
                let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cs_enc"),
                });
                {
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("cs_pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.dispatch_workgroups(x, y, 1);
                }
                let bytes = (m * 4) as u64;
                enc.copy_buffer_to_buffer(&seeds_buf, 0, &staging, 0, bytes);
                self.queue.submit(Some(enc.finish()));
                let words = super::runtime::read_u32_prefix(&self.device, &staging, bytes)?;
                seeds.extend_from_slice(&words[..m]);
            }
            Ok(())
        })?;
        Ok(seeds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::closest::nearest_all;
    use crate::geometry::closest::tests::{queries, sheet};

    // AI-FUNC-SUMMARY: Verify GPU seeds completed on the host give exactly the CPU answer, for triangles and segments, with the sheet shifted far from the origin; skips when no GPU is present.
    #[test]
    fn gpu_seeded_nearest_equals_cpu() {
        let gpu = match GpuClosestSeedPipeline::new() {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU unavailable: {error}");
                return;
            }
        };
        let n = 20;
        let (mut v, tri, seg) = sheet(n, 5);
        for (k, x) in v.iter_mut().enumerate() {
            *x += [1.0e4, -3.0e3, 50.0][k % 3];
        }
        let mut q = queries(&v, n, 8000, 6);
        for (k, x) in q.iter_mut().enumerate() {
            if k / 3 % 5 != 0 {
                *x += [1.0e4, -3.0e3, 50.0][k % 3];
            }
        }
        for (kind, corners) in [(Kind::Triangles, &tri), (Kind::Segments, &seg)] {
            let prims = Primitives::new(kind, &v, corners).unwrap();
            let bvh = PrimBvh::build(&prims);
            let cpu = nearest_all(&prims, &bvh, &q, None).unwrap();
            let seeds = gpu.seeds(&prims, &bvh, &q).unwrap();
            let got = nearest_all(&prims, &bvh, &q, Some(&seeds)).unwrap();
            assert_eq!(got.primitive, cpu.primitive);
            assert_eq!(got.distance, cpu.distance);
            assert_eq!(got.closest, cpu.closest);
        }
    }
}
