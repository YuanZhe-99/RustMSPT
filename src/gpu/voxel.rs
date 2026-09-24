use crate::types::{BoundingBox, Mesh};

const WORKGROUP_SIZE: u32 = 64;

pub struct GpuVoxelPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    counter: Option<(wgpu::ComputePipeline, wgpu::Buffer)>,
    triangle_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    occupancy_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    num_triangles: u32,
    bind_group_layout: wgpu::BindGroupLayout,
}

// AI-FUNC-SUMMARY: Subtract the bbox origin in f64 before building the f32 triangle buffer from mesh; returns Vec<f32>; side effects: None.
fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32> {
    let (ox, oy, oz) = (bbox.min.x, bbox.min.y, bbox.min.z);
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

// AI-FUNC-SUMMARY: Serialize voxel parameters into the 48-byte WGSL storage layout; returns bytes without side effects.
fn pack_params(num_triangles: u32, nx: u32, ny: u32, nz: u32, pitch: f32) -> Vec<u8> {
    let (dx, dy, dz) = crate::geometry::s2::RAY_DIR_GPU;
    let words = [
        num_triangles,
        nx,
        ny,
        nz,
        pitch.to_bits(),
        0,
        0,
        0,
        (dx as f32).to_bits(),
        (dy as f32).to_bits(),
        (dz as f32).to_bits(),
        0,
    ];
    words.into_iter().flat_map(u32::to_le_bytes).collect()
}

impl GpuVoxelPipeline {
    // AI-FUNC-SUMMARY: Clone the occupancy storage handle for a sequential same-device consumer after successful voxelization; the producer must not overwrite it during consumption.
    pub(crate) fn occupancy_buffer(&self) -> wgpu::Buffer {
        self.occupancy_buffer.clone()
    }

    // AI-FUNC-SUMMARY: Clone device/queue handles for sequential stages of the same run; creates no new device and does not share mutable pipeline buffers.
    pub(crate) fn device_queue(&self) -> (wgpu::Device, wgpu::Queue) {
        (self.device.clone(), self.queue.clone())
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu and create the voxelization compute pipeline.
    // Inputs: mesh and bounding box for normalized triangle upload.
    // Returns: Ok(GpuVoxelPipeline) or init error string.
    // Side effects: Blocking device initialization honoring RUSTMSPT_GPU_DEVICE, followed by triangle upload.
    pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String> {
        let (device, queue) = super::context::request_adapter_device("rustmspt voxel device")?;

        super::runtime::scoped(&device.clone(), || {
            let bytes = mesh
                .faces
                .len()
                .checked_mul(36)
                .ok_or("GPU triangle byte count overflow")? as u64;
            if bytes.max(4) > device.limits().max_buffer_size
                || bytes.max(4) > u64::from(device.limits().max_storage_buffer_binding_size)
            {
                return Err("GPU voxel triangle buffer exceeds device limits".into());
            }
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("voxelize"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/voxelize.wgsl").into()),
            });

            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vox_bgl"),
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
                label: Some("vox_pl"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("vox_pipeline"),
                layout: Some(&pl),
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
                size: 48,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let occupancy_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("occupancy"),
                size: 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("voxel_staging"),
                size: 4,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            Ok(Self {
                device,
                queue,
                pipeline,
                counter: None,
                triangle_buffer,
                params_buffer,
                occupancy_buffer,
                staging_buffer,
                num_triangles: mesh.faces.len() as u32,
                bind_group_layout: bgl,
            })
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Compute occupancy grid on GPU via ray-casting voxelization.
    // Inputs: bounding box (normalized), voxel pitch, grid dimensions [nx, ny, nz].
    // Returns: Occupancy grid or an input, capacity, execution or mapping error.
    // Side effects: Dispatches GPU compute, maps staging buffer.
    pub fn voxelize(&mut self, nx: u32, ny: u32, nz: u32, pitch: f32) -> Result<Vec<u32>, String> {
        self.voxelize_limited(
            nx,
            ny,
            nz,
            pitch,
            self.device.limits().max_compute_workgroups_per_dimension,
        )
    }

    // AI-FUNC-SUMMARY: Keep voxel occupancy resident and read only its exact occupied-cell count; successful execution leaves occupancy available for same-device shell evaluation.
    pub(crate) fn voxelize_count(
        &mut self,
        nx: u32,
        ny: u32,
        nz: u32,
        pitch: f32,
    ) -> Result<u32, String> {
        self.voxelize_impl(
            nx,
            ny,
            nz,
            pitch,
            self.device.limits().max_compute_workgroups_per_dimension,
            true,
        )
        .map(|result| result[0])
    }

    // AI-FUNC-SUMMARY: Lazily compile the binary occupancy reducer and allocate its four-byte result under the caller's error scope.
    fn ensure_counter(&mut self) {
        if self.counter.is_some() {
            return;
        }
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("voxel count"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/voxel_count.wgsl").into()),
            });
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("voxel count layout"),
                bind_group_layouts: &[&self.bind_group_layout],
                push_constant_ranges: &[],
            });
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("voxel count"),
                layout: Some(&layout),
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("voxel occupied count"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.counter = Some((pipeline, output));
    }

    // AI-FUNC-SUMMARY: Execute voxelization with an additional per-axis dispatch cap; used by the public device-limited path and small deterministic multidimensional-dispatch tests.
    fn voxelize_limited(
        &mut self,
        nx: u32,
        ny: u32,
        nz: u32,
        pitch: f32,
        max_groups: u32,
    ) -> Result<Vec<u32>, String> {
        self.voxelize_impl(nx, ny, nz, pitch, max_groups, false)
    }

    // AI-FUNC-SUMMARY: Execute voxelization with checked dispatch and optionally reduce occupancy on device before reading one count instead of the full field.
    fn voxelize_impl(
        &mut self,
        nx: u32,
        ny: u32,
        nz: u32,
        pitch: f32,
        max_groups: u32,
        count_only: bool,
    ) -> Result<Vec<u32>, String> {
        if !pitch.is_finite() || pitch <= 0.0 {
            return Err("GPU voxel pitch must be finite and positive".into());
        }
        let mut limits = self.device.limits();
        limits.max_compute_workgroups_per_dimension =
            limits.max_compute_workgroups_per_dimension.min(max_groups);
        let plan = super::runtime::grid_plan([nx, ny, nz], WORKGROUP_SIZE, &limits)?;
        super::runtime::scoped(&self.device.clone(), || {
            let param_data = pack_params(self.num_triangles, nx, ny, nz, pitch);
            self.queue.write_buffer(&self.params_buffer, 0, &param_data);

            let needed = plan.bytes;
            if needed > self.occupancy_buffer.size() {
                self.occupancy_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("occupancy"),
                    size: needed,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
            }
            let readback_bytes = if count_only { 4 } else { needed };
            if readback_bytes > self.staging_buffer.size() {
                self.staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("voxel_staging"),
                    size: readback_bytes,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }

            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vox_bg"),
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
                        resource: self.occupancy_buffer.as_entire_binding(),
                    },
                ],
            });

            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("vox_enc"),
                });
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("vox_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.dispatch_workgroups(plan.dispatch[0], plan.dispatch[1], 1);
            }

            if count_only {
                self.ensure_counter();
                let (pipeline, output) = self.counter.as_ref().unwrap();
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("voxel count bindings"),
                    layout: &self.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.occupancy_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: self.params_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: output.as_entire_binding(),
                        },
                    ],
                });
                {
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("voxel count"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &bg, &[]);
                    pass.dispatch_workgroups(1, 1, 1);
                }
                enc.copy_buffer_to_buffer(output, 0, &self.staging_buffer, 0, 4);
            } else {
                enc.copy_buffer_to_buffer(
                    &self.occupancy_buffer,
                    0,
                    &self.staging_buffer,
                    0,
                    needed,
                );
            }
            self.queue.submit(Some(enc.finish()));

            super::runtime::read_u32_prefix(&self.device, &self.staging_buffer, readback_bytes)
        })
    }
    // AI-FUNC-SUMMARY: Replace occupancy and staging storage together; callers validate size and capture GPU allocation errors.
    fn resize_grid_buffers(&mut self, bytes: u64) {
        self.occupancy_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("occupancy"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("voxel_staging"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }

    // AI-FUNC-SUMMARY: Release retained grid/readback peak capacity while preserving mesh upload and compiled pipeline.
    pub fn release_grid_capacity(&mut self) -> Result<(), String> {
        super::runtime::scoped(&self.device.clone(), || {
            self.resize_grid_buffers(4);
            Ok(())
        })
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    // AI-FUNC-SUMMARY: Verify the shader-visible ray and grid fields in the serialized voxel parameters.
    #[test]
    fn ray_direction_matches_wgsl_offset() {
        let bytes = pack_params(12, 3, 5, 7, 0.25);
        assert_eq!(bytes.len(), 48);
        let floats: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        let (x, y, z) = crate::geometry::s2::RAY_DIR_GPU;
        assert_eq!(&floats[8..11], &[x as f32, y as f32, z as f32]);
        assert_eq!(floats[4], 0.25);
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
mod dispatch_tests {
    use super::*;
    use crate::types::Vec3;
    // AI-FUNC-SUMMARY: Force the actual voxel shader across two dispatch axes on a small analytic box and reject invalid dimensions/pitch without allocation.
    #[test]
    fn two_dimensional_dispatch_matches_box() {
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 12.0));
        let mesh = crate::geometry::box_mesh(BoundingBox {
            min: Vec3::new(0.25, 0.25, 0.25),
            max: Vec3::new(3.75, 1.75, 1.75),
        });
        let mut gpu = match GpuVoxelPipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP GPU unavailable: {error}");
                return;
            }
        };
        let expected: Vec<u32> = (0..4)
            .flat_map(|x| {
                (0..4).flat_map(move |y| (0..12).map(move |z| u32::from(x < 4 && y < 2 && z < 2)))
            })
            .collect();
        assert_eq!(gpu.voxelize_limited(4, 4, 12, 1.0, 2).unwrap(), expected);
        assert!(gpu.voxelize(0, 1, 1, 1.0).is_err());
        assert!(gpu.voxelize(u32::MAX, 2, 2, 1.0).is_err());
        assert!(gpu.voxelize(1, 1, 1, f32::NAN).is_err());
    }
    // AI-FUNC-SUMMARY: Check retained staging identity and analytic occupancy across growth, smaller live prefixes, changed pitch and explicit release.
    #[test]
    fn voxel_staging_reuses_capacity_and_reads_live_grid() {
        let bbox = BoundingBox::from_size(Vec3::new(8.0, 8.0, 8.0));
        let mesh = crate::geometry::box_mesh(BoundingBox {
            min: Vec3::new(0.25, 0.25, 0.25),
            max: Vec3::new(2.25, 2.25, 2.25),
        });
        let mut gpu = match GpuVoxelPipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU unavailable: {error}");
                return;
            }
        };
        assert_eq!(gpu.staging_buffer.size(), 4);
        let check = |gpu: &mut GpuVoxelPipeline, n: u32, pitch: f32| {
            let expected: Vec<u32> = (0..n)
                .flat_map(|x| {
                    (0..n).flat_map(move |y| {
                        (0..n).map(move |z| {
                            u32::from([x, y, z].iter().all(|&i| {
                                let p = (i as f32 + 0.5) * pitch;
                                p > 0.25 && p < 2.25
                            }))
                        })
                    })
                })
                .collect();
            assert_eq!(gpu.voxelize(n, n, n, pitch).unwrap(), expected);
        };
        check(&mut gpu, 4, 1.0);
        let staging = gpu.staging_buffer.clone();
        let occupancy = gpu.occupancy_buffer.clone();
        check(&mut gpu, 2, 3.0);
        assert_eq!(gpu.staging_buffer, staging);
        assert_eq!(gpu.occupancy_buffer, occupancy);
        check(&mut gpu, 8, 1.0);
        assert_ne!(gpu.staging_buffer, staging);
        check(&mut gpu, 2, 1.0);
        gpu.release_grid_capacity().unwrap();
        assert_eq!(gpu.staging_buffer.size(), 4);
        assert_eq!(gpu.occupancy_buffer.size(), 4);
        check(&mut gpu, 4, 1.0);
    }
}

#[cfg(test)]
mod count_tests {
    use super::*;
    use crate::types::Vec3;

    // AI-FUNC-SUMMARY: Compare resident occupancy counts with analytic boxes and full readback across tails, growth, shrinking, mode changes and explicit release.
    #[test]
    fn resident_count_matches_full_grid() {
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let bbox = BoundingBox::from_size(Vec3::new(64.0, 64.0, 512.0));
        let mesh = crate::geometry::box_mesh(BoundingBox {
            min: Vec3::new(0.25, 0.25, 0.25),
            max: Vec3::new(8.25, 9.25, 1.25),
        });
        let mut gpu = GpuVoxelPipeline::new(&mesh, bbox).unwrap();
        for [nx, ny, nz] in [[17, 19, 3], [1, 1, 1], [1, 1, 257], [32, 32, 32], [3, 5, 7]] {
            gpu.release_grid_capacity().unwrap();
            let count = gpu.voxelize_count(nx, ny, nz, 1.0).unwrap();
            assert_eq!(count, nx.min(8) * ny.min(9) * nz.min(1));
            assert_eq!(
                gpu.staging_buffer.size(),
                4,
                "resident mode allocated a full-grid staging buffer"
            );
            let counter = gpu.counter.as_ref().unwrap().1.clone();
            let all = gpu.voxelize(nx, ny, nz, 1.0).unwrap();
            assert_eq!(count, all.iter().sum::<u32>());
            assert_eq!(gpu.voxelize_count(1, 1, 1, 1.0).unwrap(), 1);
            assert_eq!(counter, gpu.counter.as_ref().unwrap().1);
        }
        assert!(gpu.voxelize_count(0, 1, 1, 1.0).is_err());
        assert!(gpu.voxelize_count(1, 1, 1, f32::NAN).is_err());
        assert_eq!(gpu.voxelize_count(1, 1, 1, 1.0).unwrap(), 1);
        let mut empty = GpuVoxelPipeline::new(&Mesh::empty(), bbox).unwrap();
        assert_eq!(empty.voxelize_count(17, 19, 3, 1.0).unwrap(), 0);
    }

    // AI-FUNC-SUMMARY: Measure warm full voxelization plus host counting versus resident GPU counting, alternating order with exact count equality.
    #[test]
    #[ignore = "release voxel resident-count benchmark"]
    fn resident_count_benchmark() {
        use std::time::Instant;
        let bbox = BoundingBox::from_size(Vec3::new(128.0, 128.0, 128.0));
        let mesh = crate::geometry::box_mesh(BoundingBox {
            min: Vec3::new(0.25, 0.25, 0.25),
            max: Vec3::new(63.25, 63.25, 63.25),
        });
        let mut gpu = GpuVoxelPipeline::new(&mesh, bbox).expect("benchmark requires GPU");
        for n in [16u32, 64, 128] {
            let expected: u32 = gpu.voxelize(n, n, n, 1.0).unwrap().iter().sum();
            assert_eq!(gpu.voxelize_count(n, n, n, 1.0).unwrap(), expected);
            for sample in 0..5 {
                let mut evaluate = |resident: bool| {
                    let start = Instant::now();
                    let value = if resident {
                        gpu.voxelize_count(n, n, n, 1.0).unwrap()
                    } else {
                        gpu.voxelize(n, n, n, 1.0).unwrap().iter().sum()
                    };
                    let elapsed = start.elapsed().as_secs_f64();
                    assert_eq!(value, expected);
                    elapsed
                };
                let (full, resident) = if sample % 2 == 0 {
                    (evaluate(false), evaluate(true))
                } else {
                    let resident = evaluate(true);
                    (evaluate(false), resident)
                };
                eprintln!("VOXEL_COUNT_BENCH n={n} sample={sample} full_seconds={full:.9} resident_seconds={resident:.9}");
            }
        }
    }
}
