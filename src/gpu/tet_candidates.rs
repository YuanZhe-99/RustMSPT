use super::runtime::CountedWrite;
use crate::transfer::locate::TetBvh;

const WORKGROUP_SIZE: u32 = 64;
const MAX_BATCH: usize = 1 << 20;
const MAX_STACK_DEPTH: usize = 60;

// AI-FUNC-SUMMARY: Round an f64 down to the largest f32 not above it; returns f32; side effects: none.
fn round_down(x: f64) -> f32 {
    let f = x as f32;
    if f64::from(f) > x {
        f.next_down()
    } else {
        f
    }
}

// AI-FUNC-SUMMARY: Round an f64 up to the smallest f32 not below it; returns f32; side effects: none.
fn round_up(x: f64) -> f32 {
    let f = x as f32;
    if f64::from(f) < x {
        f.next_up()
    } else {
        f
    }
}

// AI-FUNC-SUMMARY: Flatten boxes to [minx,miny,minz,maxx,maxy,maxz] per box, rounded outward to f32; returns the flat array.
fn outward_boxes(lo: &[[f64; 3]], hi: &[[f64; 3]]) -> Vec<f32> {
    let mut out = Vec::with_capacity(lo.len() * 6);
    for (a, b) in lo.iter().zip(hi) {
        out.extend(a.iter().map(|&x| round_down(x)));
        out.extend(b.iter().map(|&x| round_up(x)));
    }
    out
}

pub struct GpuTetCandidatePipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    adapter: String,
}

impl GpuTetCandidatePipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Compile (or reuse) the tet-candidate compute pipeline on the shared device.
    // Inputs: none.
    // Returns: The pipeline, or the device/compile error as a string.
    // Side effects: May create the process-wide shared device.
    pub fn new() -> Result<Self, String> {
        let shared = super::context::shared_device()?;
        let (device, queue) = (shared.device().clone(), shared.queue().clone());
        let adapter = format!("{} ({:?})", shared.info().name, shared.info().backend);
        let source = include_str!("shaders/tet_candidates.wgsl");
        super::runtime::scoped(&device.clone(), || {
            let (pipeline, layout) = shared.cached_pipeline("tet_candidates", source, |device| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("tet_candidates"),
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
                    label: Some("tet_candidates_bgl"),
                    entries: &[
                        storage(0, true),
                        storage(1, true),
                        storage(2, true),
                        storage(3, true),
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        storage(5, false),
                        storage(6, false),
                    ],
                });
                let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("tet_candidates_pl"),
                    bind_group_layouts: &[&layout],
                    push_constant_ranges: &[],
                });
                let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("tet_candidates"),
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
    // Purpose: For every point, the BVH leaf slots whose padded box holds it, found on the GPU in f32.
    // Inputs: the BVH, points as flat xyz f64, k slots per point.
    // Returns: (counts, slots) with counts[i] the full number found and slots[i*k..] its first min(count, k) slots; or an error for a BVH too deep, buffers beyond device limits, or a device failure.
    // Side effects: Uploads the BVH once, then dispatches and reads back one batch of points at a time.
    // Notes: Boxes are rounded outward and points to nearest, and rounding is monotone, so every slot whose f64 box holds the f64 point is found: the result is a superset, and the host's exact test decides.
    pub fn candidates(&self, bvh: &TetBvh, points: &[f64], k: usize) -> Result<(Vec<u32>, Vec<u32>), String> {
        if bvh.depth > MAX_STACK_DEPTH {
            return Err(format!("BVH depth {} exceeds the shader stack", bvh.depth));
        }
        let limits = self.device.limits();
        let binding = u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size);
        let node_box = outward_boxes(&bvh.node_min, &bvh.node_max);
        let slot_box = outward_boxes(&bvh.slot_min, &bvh.slot_max);
        let node_link: Vec<u32> = bvh.link.iter().flat_map(|l| [l[0], l[1]]).collect();
        for (name, bytes) in [
            ("node boxes", node_box.len() * 4),
            ("slot boxes", slot_box.len() * 4),
        ] {
            if bytes as u64 > binding {
                return Err(format!("{name} need {bytes} bytes, above the device's {binding}"));
            }
        }
        let per_point = (k as u64 + 1) * 4;
        let batch = (binding / (k as u64 * 4).max(12)).min(MAX_BATCH as u64) as usize;
        let batch = batch.min(((limits.max_buffer_size / per_point) as usize).max(1));
        if batch == 0 {
            return Err("device buffers cannot hold one point's candidates".into());
        }
        let n = points.len() / 3;
        let mut counts = Vec::with_capacity(n);
        let mut slots = Vec::with_capacity(n * k);
        super::runtime::scoped(&self.device.clone(), || {
            use wgpu::util::DeviceExt;
            let upload = |label: &str, bytes: &[u8], usage: wgpu::BufferUsages| {
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytes,
                    usage,
                })
            };
            let read = wgpu::BufferUsages::STORAGE;
            let node_box_buf = upload("tc_node_box", bytemuck::cast_slice(&node_box), read);
            let node_link_buf = upload("tc_node_link", bytemuck::cast_slice(&node_link), read);
            let slot_box_buf = upload("tc_slot_box", bytemuck::cast_slice(&slot_box), read);
            let cap = batch.min(n.max(1));
            let points_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tc_points"),
                size: (cap * 12) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tc_params"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let out_usage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC;
            let counts_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tc_counts"),
                size: (cap * 4) as u64,
                usage: out_usage,
                mapped_at_creation: false,
            });
            let slots_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tc_slots"),
                size: (cap * k * 4).max(4) as u64,
                usage: out_usage,
                mapped_at_creation: false,
            });
            let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tc_staging"),
                size: (cap * (k + 1) * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tc_bg"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: node_box_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: node_link_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: slot_box_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: points_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: params_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 5, resource: counts_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 6, resource: slots_buf.as_entire_binding() },
                ],
            });
            for start in (0..n).step_by(cap) {
                let m = cap.min(n - start);
                let p32: Vec<f32> = points[start * 3..(start + m) * 3].iter().map(|&x| x as f32).collect();
                self.queue.write_counted(&points_buf, 0, bytemuck::cast_slice(&p32));
                let groups = (m as u32).div_ceil(WORKGROUP_SIZE);
                let x = groups.min(limits.max_compute_workgroups_per_dimension);
                let y = groups.div_ceil(x);
                let mut params = Vec::with_capacity(16);
                for v in [m as u32, k as u32, x * WORKGROUP_SIZE, 0] {
                    params.extend_from_slice(&v.to_le_bytes());
                }
                self.queue.write_counted(&params_buf, 0, &params);
                let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("tc_enc"),
                });
                {
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("tc_pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.dispatch_workgroups(x, y, 1);
                }
                let cb = (m * 4) as u64;
                let sb = (m * k * 4) as u64;
                enc.copy_buffer_to_buffer(&counts_buf, 0, &staging, 0, cb);
                if sb > 0 {
                    enc.copy_buffer_to_buffer(&slots_buf, 0, &staging, cb, sb);
                }
                self.queue.submit(Some(enc.finish()));
                let words = super::runtime::read_u32_prefix(&self.device, &staging, cb + sb)?;
                counts.extend_from_slice(&words[..m]);
                slots.extend_from_slice(&words[m..]);
            }
            Ok(())
        })?;
        Ok((counts, slots))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::locate::{locate_cpu, locate_from_candidates, TetMesh};
    use crate::transfer::locate::tests::{kuhn_mesh, probe_points};

    // AI-FUNC-SUMMARY: Verify outward rounding brackets the f64 value and is exact on representable values.
    #[test]
    fn rounding_brackets() {
        for x in [0.1f64, -0.1, 1.0, 1e-30, -3.3333333333, 0.0] {
            assert!(f64::from(round_down(x)) <= x && f64::from(round_up(x)) >= x);
        }
        assert_eq!(round_down(0.5), 0.5);
        assert_eq!(round_up(0.5), 0.5);
    }

    // AI-FUNC-SUMMARY: Verify GPU candidates completed on the host give exactly the CPU location, with a small k forcing overflow; skips when no GPU is present.
    #[test]
    fn gpu_location_equals_cpu() {
        let gpu = match GpuTetCandidatePipeline::new() {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU unavailable: {error}");
                return;
            }
        };
        let n = 6;
        let (points, cells) = kuhn_mesh(n, 21);
        let mesh = TetMesh::new(&points, &cells).unwrap();
        let bvh = crate::transfer::locate::TetBvh::build(&mesh);
        let probes = probe_points(&points, n, 5000, 4);
        let cpu = locate_cpu(&mesh, &bvh, &probes, 0.3).unwrap();
        for k in [3, 16] {
            let (counts, slots) = gpu.candidates(&bvh, &probes, k).unwrap();
            let got = locate_from_candidates(&mesh, &bvh, &probes, 0.3, &counts, &slots, k).unwrap();
            assert_eq!(got.donors, cpu.donors);
            for (a, b) in got.distances.iter().zip(&cpu.distances) {
                assert!(a == b || (a.is_nan() && b.is_nan()));
            }
        }
    }
}
