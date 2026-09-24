use crate::types::{BoundingBox, Mesh};

const WORKGROUP_SIZE: u32 = crate::compute::mc_memory::MC_BLOCK_SAMPLES as u32;
const MAX_RADII: usize = 128;

// AI-FUNC-SUMMARY: GPU-accelerated Monte Carlo S2 pipeline using wgpu compute shaders.
// Holds the wgpu device, queue, compute pipeline, and pre-allocated buffers for triangle
// data, parameters, and per-radius workgroup output (hit/valid partial counts). Call `calculate_s2_gpu`
// to dispatch the Monte Carlo kernel for all radii in a single dispatch.
pub struct GpuS2Pipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    triangle_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_hits_buffer: wgpu::Buffer,
    out_valids_buffer: wgpu::Buffer,
    staging_hits: wgpu::Buffer,
    staging_valids: wgpu::Buffer,
    #[cfg(test)]
    reference_samples: bool,
    #[cfg(test)]
    test_dispatch_limit: Option<u32>,
    #[cfg(test)]
    last_cpu_reduce: std::time::Duration,
    #[cfg(test)]
    last_readback_bytes: u64,
    pending_upload_bytes: u64,
    num_triangles: u32,
    bind_group_layout: wgpu::BindGroupLayout,
}

// AI-FUNC-SUMMARY: Build f32 triangle position buffer with coordinates normalized to bbox origin.
// Inputs: mesh reference, bounding box for normalization.
// Returns: Vec<f32> with 9 floats per triangle, all positions shifted by -bbox.min in f64 before conversion to f32.
// Side effects: None.
// Notes: Normalization improves f32 precision by keeping coordinates small (0..size instead of 100..1200+).
fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32> {
    let ox = bbox.min.x;
    let oy = bbox.min.y;
    let oz = bbox.min.z;
    let mut buf = Vec::with_capacity(mesh.faces.len() * 9);
    for face in &mesh.faces {
        let a = mesh.vertices[face.a];
        let b = mesh.vertices[face.b];
        let c = mesh.vertices[face.c];
        buf.extend_from_slice(&[(a.x - ox) as f32, (a.y - oy) as f32, (a.z - oz) as f32]);
        buf.extend_from_slice(&[(b.x - ox) as f32, (b.y - oy) as f32, (b.z - oz) as f32]);
        buf.extend_from_slice(&[(c.x - ox) as f32, (c.y - oy) as f32, (c.z - oz) as f32]);
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

// AI-FUNC-SUMMARY: Validate logical sample ids and radius-aligned workgroup/buffer limits before allocation; return samples per radius, total partial-count slots and two-dimensional dispatch, or an error.
fn dispatch_plan(
    r_max: usize,
    samples: usize,
    limits: &wgpu::Limits,
) -> Result<(u32, u32, [u32; 2]), String> {
    if r_max >= MAX_RADII {
        return Err(format!(
            "GPU MC r_max must be below {MAX_RADII}, got {r_max}"
        ));
    }
    let samples = u32::try_from(samples.max(200))
        .map_err(|_| "GPU MC sample count exceeds u32".to_string())?;
    (r_max as u32 + 1)
        .checked_mul(samples)
        .ok_or("GPU MC invocation count overflow")?;
    let total = (r_max as u32 + 1) * samples.div_ceil(WORKGROUP_SIZE);
    let grid = super::runtime::grid_plan([total, 1, 1], 1, limits)?;
    Ok((samples, total, grid.dispatch))
}

// AI-FUNC-SUMMARY: Check storage and buffer byte limits before GPU allocation; returns success or a capacity error.
fn check_buffer_size(bytes: u64, limits: &wgpu::Limits) -> Result<(), String> {
    if bytes > limits.max_buffer_size || bytes > u64::from(limits.max_storage_buffer_binding_size) {
        return Err(format!(
            "GPU MC buffer of {bytes} bytes exceeds device limits"
        ));
    }
    Ok(())
}

// AI-FUNC-SUMMARY: Check triangle count and byte capacity before flattening or uploading geometry; returns success or an overflow/capacity error.
fn check_mesh_capacity(mesh: &Mesh, limits: &wgpu::Limits) -> Result<(), String> {
    u32::try_from(mesh.faces.len()).map_err(|_| "GPU MC triangle count exceeds u32")?;
    let bytes = mesh
        .faces
        .len()
        .checked_mul(36)
        .ok_or("GPU MC triangle byte count overflow")?;
    check_buffer_size((bytes as u64).max(4), limits)
}

impl GpuS2Pipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu device/queue and create the Monte Carlo S2 compute pipeline.
    // Inputs: mesh reference for triangle buffer pre-upload, bounding box for coordinate normalization.
    // Returns: Pipeline or initialization, triangle-capacity or scoped device error.
    // Side effects: Performs heavy wgpu device initialization and uploads normalized triangle data to GPU.
    pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String> {
        Self::new_with_shader(mesh, bbox, include_str!("shaders/s2_monte_carlo.wgsl"))
    }

    // AI-FUNC-SUMMARY: Construct one MC pipeline from supplied shader source; production uses reduced counts and tests may supply the frozen per-sample reference.
    fn new_with_shader(
        mesh: &Mesh,
        bbox: BoundingBox,
        shader_source: &str,
    ) -> Result<Self, String> {
        let (device, queue) = super::context::request_adapter_device("rustmspt s2 device")?;

        super::runtime::scoped(&device.clone(), || {
            check_mesh_capacity(mesh, &device.limits())?;
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
                size: (tri_bytes.len() as u64).max(4),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&triangle_buffer, 0, tri_bytes);

            let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("params"),
                size: (16 + 16 + 16 + 16 + MAX_RADII * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let out_size = 4;
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

            let staging = |label| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: 4,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };
            let staging_hits = staging("staging_hits");
            let staging_valids = staging("staging_valids");

            Ok(Self {
                device,
                queue,
                pipeline,
                triangle_buffer,
                params_buffer,
                out_hits_buffer,
                out_valids_buffer,
                staging_hits,
                staging_valids,
                #[cfg(test)]
                reference_samples: false,
                #[cfg(test)]
                test_dispatch_limit: None,
                #[cfg(test)]
                last_cpu_reduce: std::time::Duration::ZERO,
                #[cfg(test)]
                last_readback_bytes: 0,
                pending_upload_bytes: tri_bytes.len() as u64,
                num_triangles: mesh.faces.len() as u32,
                bind_group_layout,
            })
        })
    }

    // AI-FUNC-SUMMARY: Check retained buffers, pending uploads and growth for the next mesh update/evaluation against an optional logical GPU budget before any allocation or upload.
    pub fn check_evaluation_budget(
        &self,
        mesh: &Mesh,
        r_max: usize,
        samples: usize,
        limit_mb: Option<u64>,
    ) -> Result<(), String> {
        let peak = crate::compute::mc_memory::mc_evaluation_peak(
            self.triangle_buffer.size(),
            self.out_hits_buffer.size(),
            self.pending_upload_bytes,
            mesh.faces.len(),
            r_max,
            samples,
        )?;
        crate::compute::mc_memory::check_mc_budget(peak, limit_mb)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Update the triangle buffer for a new mesh without recreating the pipeline.
    // Inputs: mesh and bounding box for normalized triangle data.
    // Returns: Success or a capacity/upload error.
    // Side effects: Re-uploads triangle data to GPU. Allocates new buffer if mesh size changed.
    pub fn update_mesh(&mut self, mesh: &Mesh, bbox: BoundingBox) -> Result<(), String> {
        check_mesh_capacity(mesh, &self.device.limits())?;
        super::runtime::scoped(&self.device.clone(), || {
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
            let pending = self
                .pending_upload_bytes
                .checked_add(needed)
                .ok_or("GPU MC pending upload size overflow")?;
            self.queue.write_buffer(&self.triangle_buffer, 0, tri_bytes);
            self.pending_upload_bytes = pending;
            self.num_triangles = mesh.faces.len() as u32;
            Ok(())
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Grow output and staging buffers if the new partial-count slot count exceeds capacity.
    // Inputs: radius/workgroup partial-count slots.
    // Returns: None.
    // Side effects: May reallocate GPU buffers if capacity is exceeded.
    fn ensure_output_capacity(&mut self, partials: u32) {
        if u64::from(partials) * 4 > self.out_hits_buffer.size() {
            self.resize_output_buffers(partials);
        }
    }

    // AI-FUNC-SUMMARY: Replace output and staging buffers with matching capacity; callers validate capacity and surround allocation with error scopes.
    fn resize_output_buffers(&mut self, partials: u32) {
        let needed = u64::from(partials) * 4;
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
        self.staging_hits = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_hits"),
            size: needed,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.staging_valids = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_valids"),
            size: needed,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }

    // AI-FUNC-SUMMARY: Release retained output/readback peak capacity while preserving uploaded geometry and the compiled pipeline; return any device allocation error.
    pub fn release_output_capacity(&mut self) -> Result<(), String> {
        super::runtime::scoped(&self.device.clone(), || {
            self.resize_output_buffers(1);
            Ok(())
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Compute Monte Carlo S2 two-point correlation on the GPU for all radii.
    // Inputs: bounding box, r_max (inclusive), sample count per radius.
    // Returns: S2 values indexed by radius or a capacity, execution or mapping error.
    // Side effects: Dispatches GPU compute work; maps staging buffers for readback.
    // Notes: r=0 is set to 0.0 (caller should overwrite with volume fraction). Uses f32 on GPU for positions; results are f64 on CPU.
    pub fn calculate_s2_gpu(
        &mut self,
        bbox: BoundingBox,
        r_max: usize,
        samples: usize,
    ) -> Result<Vec<f64>, String> {
        self.calculate_s2_gpu_counts(bbox, r_max, samples, rand::random::<u32>())
            .map(|counts| {
                counts
                    .into_iter()
                    .map(|[hits, valid]| {
                        if valid > 0 {
                            hits as f64 / valid as f64
                        } else {
                            0.0
                        }
                    })
                    .collect()
            })
    }

    // AI-FUNC-SUMMARY: Evaluate a fixed MC seed and merge workgroup integer partials in u64; return per-radius hit/valid totals for exact reference comparisons.
    fn calculate_s2_gpu_counts(
        &mut self,
        bbox: BoundingBox,
        r_max: usize,
        samples: usize,
        seed: u32,
    ) -> Result<Vec<[u64; 2]>, String> {
        #[allow(unused_mut)]
        let mut limits = self.device.limits();
        #[cfg(test)]
        if let Some(limit) = self.test_dispatch_limit {
            limits.max_compute_workgroups_per_dimension = limit;
        }
        #[allow(unused_mut)]
        let (samples_per_radius, partial_count, mut dispatch) =
            dispatch_plan(r_max, samples, &limits)?;
        #[allow(unused_mut)]
        let mut output_count = partial_count;
        #[allow(unused_mut)]
        let mut per_radius = samples_per_radius.div_ceil(WORKGROUP_SIZE);
        #[cfg(test)]
        if self.reference_samples {
            output_count = (r_max as u32 + 1) * samples_per_radius;
            per_radius = samples_per_radius;
            dispatch = [output_count.div_ceil(WORKGROUP_SIZE), 1];
            check_buffer_size(u64::from(output_count) * 4, &self.device.limits())?;
        }
        let size = bbox.size();
        if [size.x, size.y, size.z]
            .iter()
            .any(|v| !(*v as f32).is_finite() || (*v as f32) <= 0.0)
        {
            return Err("GPU MC bbox must have finite positive f32 extents".into());
        }
        super::runtime::scoped(&self.device.clone(), || {
            let radii: Vec<f32> = (0..=r_max).map(|r| r as f32).collect();
            self.ensure_output_capacity(output_count);

            let param_data =
                pack_params(self.num_triangles, &radii, seed, bbox, samples_per_radius);
            let pending = self
                .pending_upload_bytes
                .checked_add(param_data.len() as u64)
                .ok_or("GPU MC pending upload size overflow")?;
            self.queue.write_buffer(&self.params_buffer, 0, &param_data);
            self.pending_upload_bytes = pending;

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
                pass.dispatch_workgroups(dispatch[0], dispatch[1], 1);
            }

            let readback_size = (output_count as u64) * 4;
            encoder.copy_buffer_to_buffer(
                &self.out_hits_buffer,
                0,
                &self.staging_hits,
                0,
                readback_size,
            );
            encoder.copy_buffer_to_buffer(
                &self.out_valids_buffer,
                0,
                &self.staging_valids,
                0,
                readback_size,
            );
            self.queue.submit(Some(encoder.finish()));

            let hits_u32 =
                super::runtime::read_u32_prefix(&self.device, &self.staging_hits, readback_size)?;
            let valids_u32 =
                super::runtime::read_u32_prefix(&self.device, &self.staging_valids, readback_size)?;

            self.pending_upload_bytes = 0;
            #[cfg(test)]
            let reduce_start = std::time::Instant::now();
            let mut out = vec![[0u64; 2]; r_max + 1];
            let sp = per_radius as usize;
            for (r, slot) in out.iter_mut().enumerate().take(r_max + 1) {
                let start = r * sp;
                let end = ((r + 1) * sp).min(output_count as usize);
                if start >= end {
                    continue;
                }
                let total_hits: u64 = hits_u32[start..end].iter().map(|&v| v as u64).sum();
                let total_valid: u64 = valids_u32[start..end].iter().map(|&v| v as u64).sum();
                *slot = [total_hits, total_valid];
            }

            #[cfg(test)]
            {
                self.last_cpu_reduce = reduce_start.elapsed();
                self.last_readback_bytes = readback_size * 2;
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod normalization_tests {
    use super::*;
    use crate::geometry::{box_mesh, translate_mesh};
    use crate::types::Vec3;

    // AI-FUNC-SUMMARY: Verify that large global translations preserve sub-unit triangle coordinates uploaded to the GPU.
    #[test]
    fn triangle_upload_preserves_local_geometry() {
        let mut bbox = BoundingBox {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(4.0, 4.0, 4.0),
        };
        let mut mesh = box_mesh(BoundingBox {
            min: Vec3::new(0.25, 0.5, 0.75),
            max: Vec3::new(1.25, 2.5, 3.75),
        });
        let expected = build_triangle_buffer(&mesh, bbox);
        let shift = Vec3::new(1e9, -1e9, 1e9);
        translate_mesh(&mut mesh, shift);
        bbox.min = Vec3::new(shift.x, shift.y, shift.z);
        bbox.max = Vec3::new(shift.x + 4.0, shift.y + 4.0, shift.z + 4.0);
        assert_eq!(build_triangle_buffer(&mesh, bbox), expected);
    }
}

#[cfg(test)]
mod execution_tests {
    use super::*;
    use crate::geometry::box_mesh;
    use crate::types::Vec3;

    // AI-FUNC-SUMMARY: Verify checked radius/count/dispatch/storage limits without allocating GPU memory.
    #[test]
    fn dispatch_limits_reject_before_allocation() {
        let mut limits = wgpu::Limits::default();
        assert!(dispatch_plan(127, 200, &limits).is_ok());
        assert!(dispatch_plan(128, 200, &limits).is_err());
        assert!(dispatch_plan(usize::MAX, 200, &limits).is_err());
        assert!(dispatch_plan(0, usize::MAX, &limits).is_err());
        assert!(dispatch_plan(127, u32::MAX as usize, &limits).is_err());
        limits.max_compute_workgroups_per_dimension = 1;
        assert_eq!(dispatch_plan(0, 256, &limits).unwrap(), (256, 1, [1, 1]));
        assert!(dispatch_plan(0, 257, &limits).is_err());
        limits.max_storage_buffer_binding_size = 3;
        assert!(dispatch_plan(0, 256, &limits).is_err());
        limits.max_storage_buffer_binding_size = 4;
        limits.max_buffer_size = 3;
        assert!(dispatch_plan(0, 256, &limits).is_err());
    }

    // AI-FUNC-SUMMARY: Trigger a real invalid GPU mapping under error scopes, verify balanced recovery, and exercise empty geometry and radius refusal.
    #[test]
    fn mapping_errors_are_results_and_scopes_recover() {
        let bbox = BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0));
        let mesh = box_mesh(bbox);
        let mut gpu = match GpuS2Pipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP GPU unavailable: {error}");
                return;
            }
        };
        let error = super::super::runtime::scoped(&gpu.device, || {
            super::super::runtime::read_u32(&gpu.device, &gpu.params_buffer)
        });
        assert!(error.is_err());
        assert!(gpu
            .calculate_s2_gpu(bbox, 128, 200)
            .unwrap_err()
            .contains("r_max"));
        let tiny_bbox = BoundingBox::from_size(Vec3::new(1e-300, 1.0, 1.0));
        assert!(gpu
            .calculate_s2_gpu(tiny_bbox, 0, 200)
            .unwrap_err()
            .contains("bbox"));
        let empty = Mesh {
            vertices: vec![],
            faces: vec![],
        };
        gpu.update_mesh(&empty, bbox).unwrap();
        assert_eq!(gpu.calculate_s2_gpu(bbox, 1, 200).unwrap(), vec![0.0, 0.0]);
        let mut empty_gpu = GpuS2Pipeline::new(&empty, bbox).unwrap();
        assert_eq!(
            empty_gpu.calculate_s2_gpu(bbox, 1, 200).unwrap(),
            vec![0.0, 0.0]
        );
    }
    // AI-FUNC-SUMMARY: Check lazy output/staging allocation, buffer identity reuse, growth and live-prefix results after geometry changes.
    #[test]
    fn mc_staging_reuses_capacity_without_stale_results() {
        let bbox = BoundingBox {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        let empty = Mesh {
            vertices: Vec::new(),
            faces: Vec::new(),
        };
        let mut gpu = match GpuS2Pipeline::new(&empty, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU MC unavailable: {error}");
                return;
            }
        };
        assert_eq!(gpu.out_hits_buffer.size(), 4);
        assert_eq!(gpu.out_valids_buffer.size(), 4);
        assert_eq!(gpu.staging_hits.size(), 4);
        assert_eq!(gpu.staging_valids.size(), 4);
        assert_eq!(gpu.calculate_s2_gpu(bbox, 2, 200).unwrap(), vec![0.0; 3]);
        let hits = gpu.staging_hits.clone();
        let valids = gpu.staging_valids.clone();
        let output = gpu.out_hits_buffer.clone();
        assert_eq!(hits.size(), 3 * 4);
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![0.0]);
        assert_eq!(gpu.staging_hits, hits);
        assert_eq!(gpu.staging_valids, valids);
        assert_eq!(gpu.out_hits_buffer, output);
        let enclosing = box_mesh(BoundingBox {
            min: Vec3::new(-1.0, -1.0, -1.0),
            max: Vec3::new(2.0, 2.0, 2.0),
        });
        gpu.update_mesh(&enclosing, bbox).unwrap();
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![1.0]);
        assert_eq!(gpu.staging_hits, hits);
        assert_eq!(gpu.calculate_s2_gpu(bbox, 3, 400).unwrap()[0], 1.0);
        assert_ne!(gpu.staging_hits, hits);
        assert_eq!(gpu.staging_hits.size(), 4 * 2 * 4);
        gpu.update_mesh(&empty, bbox).unwrap();
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![0.0]);
        assert!(super::super::runtime::read_u32_prefix(&gpu.device, &gpu.staging_hits, 0).is_err());
        assert!(super::super::runtime::read_u32_prefix(&gpu.device, &gpu.staging_hits, 3).is_err());
        assert!(super::super::runtime::read_u32_prefix(
            &gpu.device,
            &gpu.staging_hits,
            gpu.staging_hits.size() + 4
        )
        .is_err());
        gpu.release_output_capacity().unwrap();
        assert_eq!(gpu.out_hits_buffer.size(), 4);
        assert_eq!(gpu.out_valids_buffer.size(), 4);
        assert_eq!(gpu.staging_hits.size(), 4);
        assert_eq!(gpu.staging_valids.size(), 4);
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![0.0]);
    }
    // AI-FUNC-SUMMARY: Verify budget rejection is allocation-free, queued geometry uploads are counted and cleared after completion, and retained large outputs require release before a small cap can be honored.
    #[test]
    fn mc_budget_tracks_live_capacity_and_uploads() {
        let bbox = BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0));
        let mesh = box_mesh(bbox);
        let mut gpu = match GpuS2Pipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU MC unavailable: {error}");
                return;
            }
        };
        assert_eq!(gpu.pending_upload_bytes, 432);
        let original = gpu.out_hits_buffer.clone();
        assert!(gpu.check_evaluation_budget(&mesh, 0, 200, Some(0)).is_err());
        assert_eq!(original, gpu.out_hits_buffer);
        gpu.update_mesh(&mesh, bbox).unwrap();
        assert_eq!(gpu.pending_upload_bytes, 864);
        assert!(gpu.check_evaluation_budget(&mesh, 0, 200, Some(1)).is_ok());
        gpu.calculate_s2_gpu(bbox, 0, 200).unwrap();
        assert_eq!(gpu.pending_upload_bytes, 0);
        gpu.update_mesh(&Mesh::empty(), bbox).unwrap();
        super::super::runtime::scoped(&gpu.device.clone(), || {
            gpu.resize_output_buffers(65_535);
            Ok(())
        })
        .unwrap();
        assert!(gpu
            .check_evaluation_budget(&Mesh::empty(), 0, 200, Some(1))
            .is_err());
        gpu.release_output_capacity().unwrap();
        assert!(gpu
            .check_evaluation_budget(&Mesh::empty(), 0, 200, Some(1))
            .is_ok());
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![0.0]);
    }
    // AI-FUNC-SUMMARY: Compare reduced workgroup counts exactly with the frozen per-sample GPU shader under identical seeds, checking padding, radius boundaries and repeated geometry updates.
    #[test]
    fn workgroup_counts_match_per_sample_reference() {
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let mesh = box_mesh(BoundingBox {
            min: Vec3::new(0.5, 0.5, 0.5),
            max: Vec3::new(2.5, 2.5, 2.5),
        });
        let mut gpu = match GpuS2Pipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU MC unavailable: {error}");
                return;
            }
        };
        let mut reference = GpuS2Pipeline::new_with_shader(
            &mesh,
            bbox,
            include_str!("../../tests/fixtures/s2_monte_carlo_samples.wgsl"),
        )
        .unwrap();
        reference.reference_samples = true;
        for (r_max, samples) in [
            (0, 0),
            (3, 200),
            (3, 255),
            (3, 256),
            (3, 257),
            (3, 511),
            (3, 512),
            (3, 513),
            (3, 1000),
            (127, 257),
        ] {
            for seed in [0, u32::MAX] {
                let expected = reference
                    .calculate_s2_gpu_counts(bbox, r_max, samples, seed)
                    .unwrap();
                let actual = gpu
                    .calculate_s2_gpu_counts(bbox, r_max, samples, seed)
                    .unwrap();
                assert_eq!(
                    actual, expected,
                    "r_max={r_max} samples={samples} seed={seed}"
                );
                assert_eq!(
                    gpu.last_readback_bytes,
                    (r_max as u64 + 1) * samples.max(200).div_ceil(256) as u64 * 8
                );
                assert!(actual
                    .iter()
                    .all(|&[hits, valid]| hits <= valid && valid <= samples.max(200) as u64));
            }
        }
        gpu.update_mesh(&Mesh::empty(), bbox).unwrap();
        reference.update_mesh(&Mesh::empty(), bbox).unwrap();
        assert_eq!(
            gpu.calculate_s2_gpu_counts(bbox, 3, 257, 42).unwrap(),
            reference.calculate_s2_gpu_counts(bbox, 3, 257, 42).unwrap()
        );
    }

    // AI-FUNC-SUMMARY: Measure resident small/medium geometry MC evaluation and CPU integer merging with identical seeds; log actual readback bytes, alternating warm samples and total time for reduced versus frozen per-sample GPU output.
    #[test]
    #[ignore = "release GPU MC reduction benchmark"]
    fn workgroup_reduction_benchmark() {
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        for level in [0, 1] {
            let mesh = if level == 0 {
                box_mesh(BoundingBox {
                    min: Vec3::new(0.5, 0.5, 0.5),
                    max: Vec3::new(2.5, 2.5, 2.5),
                })
            } else {
                crate::geometry::icosphere_mesh(Vec3::new(1.5, 1.5, 1.5), 1.0, 1)
            };
            let mut gpu = match GpuS2Pipeline::new(&mesh, bbox) {
                Ok(gpu) => gpu,
                Err(error) => {
                    eprintln!("SKIP: GPU MC unavailable: {error}");
                    return;
                }
            };
            let mut reference = GpuS2Pipeline::new_with_shader(
                &mesh,
                bbox,
                include_str!("../../tests/fixtures/s2_monte_carlo_samples.wgsl"),
            )
            .unwrap();
            reference.reference_samples = true;
            for samples in [200, 4000, 40000] {
                assert_eq!(
                    gpu.calculate_s2_gpu_counts(bbox, 3, samples, 42).unwrap(),
                    reference
                        .calculate_s2_gpu_counts(bbox, 3, samples, 42)
                        .unwrap()
                );
                let run = |p: &mut GpuS2Pipeline| {
                    let start = std::time::Instant::now();
                    let mut reduce = 0.0;
                    for i in 0..3 {
                        std::hint::black_box(
                            p.calculate_s2_gpu_counts(bbox, 3, samples, 42 + i).unwrap(),
                        );
                        reduce += p.last_cpu_reduce.as_secs_f64();
                    }
                    (start.elapsed().as_secs_f64(), reduce, p.last_readback_bytes)
                };
                for sample in 0..6 {
                    let (old, new) = if sample % 2 == 0 {
                        (run(&mut reference), run(&mut gpu))
                    } else {
                        let new = run(&mut gpu);
                        (run(&mut reference), new)
                    };
                    eprintln!("MC_REDUCE_BENCH faces={} samples={samples} sample={sample} repeats=3 old_seconds={:.9} new_seconds={:.9} old_reduce={:.9} new_reduce={:.9} old_bytes={} new_bytes={}",mesh.faces.len(),old.0,new.0,old.1,new.1,old.2,new.2);
                }
            }
        }
    }
    // AI-FUNC-SUMMARY: Validate two-dimensional dispatch capacity without allocating huge logical workloads, and compare forced multi-row/padded GPU execution with original sample counts at identical seeds.
    #[test]
    fn mc_two_dimensional_dispatch_preserves_samples() {
        let mut limits = wgpu::Limits::default();
        assert_eq!(
            dispatch_plan(0, 65_536 * 256, &limits).unwrap(),
            (65_536 * 256, 65_536, [65_535, 2])
        );
        limits.max_compute_workgroups_per_dimension = 3;
        assert_eq!(dispatch_plan(3, 257, &limits).unwrap(), (257, 8, [3, 3]));
        assert!(dispatch_plan(4, 257, &limits).is_err());
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let mesh = box_mesh(BoundingBox {
            min: Vec3::new(0.5, 0.5, 0.5),
            max: Vec3::new(2.5, 2.5, 2.5),
        });
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let mut gpu =
            GpuS2Pipeline::new(&mesh, bbox).expect("available GPU must compile the 2D MC shader");
        let mut reference = GpuS2Pipeline::new_with_shader(
            &mesh,
            bbox,
            include_str!("../../tests/fixtures/s2_monte_carlo_samples.wgsl"),
        )
        .unwrap();
        reference.reference_samples = true;
        for (r_max, samples) in [(0, 200), (3, 257), (1, 769), (0, 2049)] {
            for seed in [0, 42] {
                let expected = reference
                    .calculate_s2_gpu_counts(bbox, r_max, samples, seed)
                    .unwrap();
                for limit in [3, 4] {
                    gpu.test_dispatch_limit = Some(limit);
                    assert_eq!(
                        gpu.calculate_s2_gpu_counts(bbox, r_max, samples, seed)
                            .unwrap(),
                        expected,
                        "limit={limit} r={r_max} s={samples}"
                    );
                }
            }
        }
        // Keep spare output capacity so a missing padded-group guard cannot hide behind robust out-of-bounds writes.
        gpu.test_dispatch_limit = Some(3);
        gpu.calculate_s2_gpu_counts(bbox, 0, 2049, 42).unwrap();
        let before = super::super::runtime::read_u32_prefix(&gpu.device, &gpu.staging_valids, 36)
            .unwrap()[8];
        gpu.calculate_s2_gpu_counts(bbox, 3, 257, 42).unwrap();
        let after = super::super::runtime::scoped(&gpu.device, || {
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("padded MC output guard oracle"),
                });
            encoder.copy_buffer_to_buffer(&gpu.out_valids_buffer, 0, &gpu.staging_valids, 0, 36);
            gpu.queue.submit(Some(encoder.finish()));
            super::super::runtime::read_u32_prefix(&gpu.device, &gpu.staging_valids, 36)
        })
        .unwrap()[8];
        assert_eq!(
            after, before,
            "padded workgroup wrote into retained spare capacity"
        );
    }
}
