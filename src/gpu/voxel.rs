use crate::types::{BoundingBox, Mesh};

const WORKGROUP_SIZE: u32 = 64;
const UNCERTAIN_INITIAL: usize = crate::compute::exact_memory::VOXEL_UNCERTAIN_INITIAL;

pub struct GpuVoxelPipeline {
    shared: std::sync::Arc<super::context::SharedGpuDevice>,
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
    uncertain_buffer: wgpu::Buffer,
    uncertain_staging: wgpu::Buffer,
    reference: super::certify::CertReference,
    cert_stats: super::certify::GpuCertificationStats,
    regrowth_headroom: Option<u64>,
    #[cfg(test)]
    last_uncertain: Vec<u32>,
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

// AI-FUNC-SUMMARY: Occupancy storage usage; COPY_DST lets the host patch CPU-certified cells into the resident field; returns buffer usages; side effects: None.
fn occupancy_usage() -> wgpu::BufferUsages {
    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST
}

// AI-FUNC-SUMMARY: The exact f32 cell-center coordinate (f32(i) + 0.5) * pitch the voxel shader evaluates; both operations are correctly rounded in WGSL and Rust, so host and GPU agree bit for bit; returns f32; side effects: None.
pub(crate) fn voxel_center(i: u32, pitch: f32) -> f32 {
    (i as f32 + 0.5) * pitch
}

// AI-FUNC-SUMMARY: Allocate an uncertain-cell list (atomic counter plus one index per cell) and its map-read staging for `entries` cells; callers check limits and hold an error scope.
fn voxel_uncertain_buffers(device: &wgpu::Device, entries: usize) -> (wgpu::Buffer, wgpu::Buffer) {
    let size = (entries.max(1) as u64 + 1) * 4;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("voxel_uncertain"),
        size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("voxel_uncertain_staging"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (buffer, staging)
}

// AI-FUNC-SUMMARY: Serialize voxel parameters into the 80-byte WGSL storage layout (grid, pitch, ray at byte 32, then the certification tail: exact f32 mesh early-out bounds and flags); returns bytes without side effects.
fn pack_params(num_triangles: u32, nx: u32, ny: u32, nz: u32, pitch: f32, tail: &[u8]) -> Vec<u8> {
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
    let mut bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    bytes.extend_from_slice(tail);
    bytes
}

impl GpuVoxelPipeline {
    // AI-FUNC-SUMMARY: Clone the occupancy storage handle for a sequential same-device consumer after successful voxelization; the producer must not overwrite it during consumption.
    pub(crate) fn occupancy_buffer(&self) -> wgpu::Buffer {
        self.occupancy_buffer.clone()
    }

    // AI-FUNC-SUMMARY: Share the process device handle for sequential stages of the same run; creates no new device and does not share mutable pipeline buffers.
    pub(crate) fn shared_device(&self) -> std::sync::Arc<super::context::SharedGpuDevice> {
        self.shared.clone()
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu and create the voxelization compute pipeline.
    // Inputs: mesh and bounding box for normalized triangle upload.
    // Returns: Ok(GpuVoxelPipeline) or init error string.
    // Side effects: Blocking device initialization honoring RUSTMSPT_GPU_DEVICE, followed by triangle upload.
    pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String> {
        Self::new_with_shader(mesh, bbox, include_str!("shaders/voxelize.wgsl"))
    }

    // AI-FUNC-SUMMARY: Construct a voxel pipeline from supplied shader source; production passes the certified shader and tests may pass the frozen uncertified baseline for overhead comparison.
    fn new_with_shader(mesh: &Mesh, bbox: BoundingBox, shader_source: &str) -> Result<Self, String> {
        let shared = super::context::shared_device()?;
        let (device, queue) = (shared.device().clone(), shared.queue().clone());

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
            let (pipeline, bgl) = shared.cached_pipeline("voxelize", shader_source, |device| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("voxelize"),
                    source: wgpu::ShaderSource::Wgsl(shader_source.into()),
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
                (pipeline, bgl)
            })?;

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
                size: 80,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let occupancy_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("occupancy"),
                size: 4,
                usage: occupancy_usage(),
                mapped_at_creation: false,
            });

            let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("voxel_staging"),
                size: 4,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let (uncertain_buffer, uncertain_staging) =
                voxel_uncertain_buffers(&device, UNCERTAIN_INITIAL);
            Ok(Self {
                shared: shared.clone(),
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
                uncertain_buffer,
                uncertain_staging,
                reference: super::certify::CertReference::new(mesh, bbox.min),
                cert_stats: Default::default(),
                regrowth_headroom: None,
                #[cfg(test)]
                last_uncertain: Vec::new(),
            })
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Compute occupancy grid on GPU via ray-casting voxelization.
    // Inputs: bounding box (normalized), voxel pitch, grid dimensions [nx, ny, nz].
    // Returns: Occupancy grid equal to the CPU f64 reference at the f32 cell centers, or an input, capacity, execution or mapping error.
    // Side effects: Dispatches GPU compute, maps staging buffers, re-evaluates uncertain cells on the CPU and updates certification counters.
    pub fn voxelize(&mut self, nx: u32, ny: u32, nz: u32, pitch: f32) -> Result<Vec<u32>, String> {
        self.voxelize_limited(
            nx,
            ny,
            nz,
            pitch,
            self.device.limits().max_compute_workgroups_per_dimension,
        )
    }

    // AI-FUNC-SUMMARY: Keep voxel occupancy resident and read only its exact occupied-cell count; CPU-certified uncertain cells are added to the count and patched into the resident field, so same-device shell evaluation sees the certified occupancy.
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

    // AI-FUNC-SUMMARY: Lazily fetch the cached binary occupancy reducer and allocate this instance's four-byte result under the caller's error scope; returns a compile error without caching it.
    fn ensure_counter(&mut self) -> Result<(), String> {
        if self.counter.is_some() {
            return Ok(());
        }
        let layout = self.bind_group_layout.clone();
        let pipeline = self.shared.cached_pipeline(
            "voxel_count",
            include_str!("shaders/voxel_count.wgsl"),
            |device| {
                let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("voxel count"),
                    source: wgpu::ShaderSource::Wgsl(
                        include_str!("shaders/voxel_count.wgsl").into(),
                    ),
                });
                let pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("voxel count layout"),
                        bind_group_layouts: &[&layout],
                        push_constant_ranges: &[],
                    });
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("voxel count"),
                    layout: Some(&pipeline_layout),
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                })
            },
        )?;
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("voxel occupied count"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.counter = Some((pipeline, output));
        Ok(())
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

    // AI-FUNC-SUMMARY:
    // Purpose: Execute certified voxelization with checked dispatch, optionally reducing occupancy on device before reading one count instead of the full field.
    // Inputs: grid dimensions, pitch, per-axis dispatch cap, count-only flag.
    // Returns: Full occupancy (0/1 per cell) or [occupied count], both equal to the CPU f64 reference at the f32 cell centers.
    // Side effects: Dispatches voxelization (and the reducer), reads the uncertain list, regrows and re-dispatches it on overflow,
    // re-evaluates uncertain cells on the CPU, patches their resident occupancy to 1 where inside, and updates certification counters.
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
            let param_data = pack_params(
                self.num_triangles,
                nx,
                ny,
                nz,
                pitch,
                &self.reference.params_tail(0),
            );
            self.queue.write_buffer(&self.params_buffer, 0, &param_data);

            let needed = plan.bytes;
            if needed > self.occupancy_buffer.size() {
                self.occupancy_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("occupancy"),
                    size: needed,
                    usage: occupancy_usage(),
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
            if count_only {
                self.ensure_counter()?;
            }
            let planned = crate::compute::exact_memory::voxel_uncertain_entries(plan.total as usize);
            if (planned as u64 + 1) * 4 > self.uncertain_buffer.size() {
                let (buffer, staging) = voxel_uncertain_buffers(&self.device, planned);
                self.uncertain_buffer = buffer;
                self.uncertain_staging = staging;
            }
            let uncertain = loop {
                let count = self.dispatch_voxels(plan.dispatch, needed, count_only)?;
                if u64::from(count) < self.uncertain_buffer.size() / 4 {
                    break count as usize;
                }
                let bytes = (u64::from(count) + 1) * 4;
                if bytes > self.device.limits().max_buffer_size
                    || bytes > u64::from(self.device.limits().max_storage_buffer_binding_size)
                {
                    return Err("GPU voxel uncertain list exceeds device limits".into());
                }
                if let Some(headroom) = self.regrowth_headroom {
                    let planned_pair = 2 * (planned as u64 + 1) * 4;
                    let extra = (2 * self.uncertain_buffer.size() + 2 * bytes).saturating_sub(planned_pair);
                    if extra > headroom {
                        return Err(format!(
                            "GPU voxel certification list regrowth to {count} entries needs {extra} bytes beyond the planned list, over the {headroom}-byte budget headroom"
                        ));
                    }
                }
                let (buffer, staging) = voxel_uncertain_buffers(&self.device, count as usize);
                self.uncertain_buffer = buffer;
                self.uncertain_staging = staging;
                self.cert_stats.list_regrowths += 1;
            };
            let mut result =
                super::runtime::read_u32_prefix(&self.device, &self.staging_buffer, readback_bytes)?;
            self.cert_stats.queries += u64::from(plan.total);
            if uncertain > 0 {
                let start = std::time::Instant::now();
                let words = super::runtime::read_u32_prefix(
                    &self.device,
                    &self.uncertain_staging,
                    (uncertain as u64 + 1) * 4,
                )?;
                let mut cells = words[1..].to_vec();
                cells.sort_unstable();
                let centers: Vec<[f32; 3]> = cells
                    .iter()
                    .map(|&idx| {
                        let z = idx % nz;
                        let y = (idx / nz) % ny;
                        let x = idx / (nz * ny);
                        [voxel_center(x, pitch), voxel_center(y, pitch), voxel_center(z, pitch)]
                    })
                    .collect();
                let inside = self.reference.classify(&centers);
                let occupied: Vec<u32> = cells
                    .iter()
                    .zip(&inside)
                    .filter(|(_, &inside)| inside)
                    .map(|(&idx, _)| idx)
                    .collect();
                if count_only {
                    result[0] += occupied.len() as u32;
                } else {
                    for &idx in &occupied {
                        result[idx as usize] = 1;
                    }
                }
                let mut run = 0;
                while run < occupied.len() {
                    let mut end = run + 1;
                    while end < occupied.len() && occupied[end] == occupied[end - 1] + 1 {
                        end += 1;
                    }
                    let ones = vec![1u32; end - run];
                    self.queue.write_buffer(
                        &self.occupancy_buffer,
                        u64::from(occupied[run]) * 4,
                        bytemuck::cast_slice(&ones),
                    );
                    run = end;
                }
                self.cert_stats.uncertain += uncertain as u64;
                self.cert_stats.cpu_recompute_seconds += start.elapsed().as_secs_f64();
                #[cfg(test)]
                {
                    self.last_uncertain = cells;
                }
            } else {
                #[cfg(test)]
                self.last_uncertain.clear();
            }
            Ok(result)
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Clear the uncertain counter, dispatch voxelization (plus the reducer in count mode), copy the result and the whole uncertain list to staging, and read the uncertain counter.
    // Inputs: two-dimensional dispatch, occupancy bytes, count-only flag (the reducer must already exist).
    // Returns: The uncertain cell count the kernel reported (may exceed list capacity), or a mapping error.
    // Side effects: Writes 4 bytes and submits one command buffer; staging buffers hold this dispatch's results.
    fn dispatch_voxels(&mut self, dispatch: [u32; 2], needed: u64, count_only: bool) -> Result<u32, String> {
        self.queue
            .write_buffer(&self.uncertain_buffer, 0, &0u32.to_le_bytes());
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.uncertain_buffer.as_entire_binding(),
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
            pass.dispatch_workgroups(dispatch[0], dispatch[1], 1);
        }

        if count_only {
            let (pipeline, output) = self
                .counter
                .as_ref()
                .ok_or("GPU voxel reducer was not prepared")?;
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
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.uncertain_buffer.as_entire_binding(),
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
            enc.copy_buffer_to_buffer(&self.occupancy_buffer, 0, &self.staging_buffer, 0, needed);
        }
        enc.copy_buffer_to_buffer(
            &self.uncertain_buffer,
            0,
            &self.uncertain_staging,
            0,
            self.uncertain_buffer.size(),
        );
        self.queue.submit(Some(enc.finish()));
        Ok(super::runtime::read_u32_prefix(&self.device, &self.uncertain_staging, 4)?[0])
    }

    // AI-FUNC-SUMMARY: Set the bytes a certification-list regrowth may add beyond the planned list (the logical budget minus the planned peak); None leaves regrowth bounded by device limits only; side effects: stores the headroom.
    // Notes: The planned list is part of every exact memory plan; only a regrowth past it is unforeseen, and
    // while it happens the current list and the new one are both alive, so both are charged.
    pub fn set_regrowth_headroom(&mut self, headroom: Option<u64>) {
        self.regrowth_headroom = headroom;
    }

    // AI-FUNC-SUMMARY: Return cumulative f32 certification counters (cells evaluated, cells recomputed on the CPU, list regrowths, host time); returns a copy; side effects: None.
    pub fn certification_stats(&self) -> super::certify::GpuCertificationStats {
        self.cert_stats
    }

    // AI-FUNC-SUMMARY: Replace occupancy and staging storage together; callers validate size and capture GPU allocation errors.
    fn resize_grid_buffers(&mut self, bytes: u64) {
        self.occupancy_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("occupancy"),
            size: bytes,
            usage: occupancy_usage(),
            mapped_at_creation: false,
        });
        self.staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("voxel_staging"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }

    // AI-FUNC-SUMMARY: Release retained grid/readback and uncertain-list peak capacity (list back to its initial size) while preserving mesh upload and compiled pipeline.
    pub fn release_grid_capacity(&mut self) -> Result<(), String> {
        super::runtime::scoped(&self.device.clone(), || {
            self.resize_grid_buffers(4);
            let (buffer, staging) = voxel_uncertain_buffers(&self.device, UNCERTAIN_INITIAL);
            self.uncertain_buffer = buffer;
            self.uncertain_staging = staging;
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
        let tail = super::super::certify::CertReference::new(
            &crate::geometry::box_mesh(BoundingBox::from_size(crate::types::Vec3::new(1.0, 2.0, 3.0))),
            crate::types::Vec3::new(0.0, 0.0, 0.0),
        )
        .params_tail(0);
        let bytes = pack_params(12, 3, 5, 7, 0.25, &tail);
        assert_eq!(bytes.len(), 80);
        let floats: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        let (x, y, z) = crate::geometry::s2::RAY_DIR_GPU;
        assert_eq!(&floats[8..11], &[x as f32, y as f32, z as f32]);
        assert_eq!(floats[4], 0.25);
        assert!((-1e-9..0.0).contains(&(floats[12] as f64)));
        assert_eq!(&floats[16..19], &[1.0, 2.0, 3.0]);
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

#[cfg(test)]
mod certification_tests {
    use super::*;
    use crate::geometry::point_inside_mesh;
    use crate::gpu::certify::fixtures::{adversarial_meshes, shifted};
    use crate::types::Vec3;

    const N: u32 = 16;
    const PITCH: f32 = 0.25;

    // AI-FUNC-SUMMARY: CPU f64 reference occupancy at the exact f32 cell centers of a pipeline's origin-shifted mesh; returns one 0/1 per cell in shader index order.
    fn cpu_reference(gpu: &GpuVoxelPipeline, n: u32, pitch: f32) -> Vec<u32> {
        (0..n)
            .flat_map(|x| (0..n).flat_map(move |y| (0..n).map(move |z| [x, y, z])))
            .map(|[x, y, z]| {
                let p = Vec3::new(
                    voxel_center(x, pitch) as f64,
                    voxel_center(y, pitch) as f64,
                    voxel_center(z, pitch) as f64,
                );
                u32::from(point_inside_mesh(gpu.reference.mesh(), p))
            })
            .collect()
    }

    // AI-FUNC-SUMMARY: Read the resident occupancy field through a temporary staging copy so tests can prove patched cells reach same-device consumers.
    fn resident(gpu: &GpuVoxelPipeline, cells: u64) -> Vec<u32> {
        super::super::runtime::scoped(&gpu.device.clone(), || {
            let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("resident occupancy oracle"),
                size: cells * 4,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            encoder.copy_buffer_to_buffer(&gpu.occupancy_buffer, 0, &staging, 0, cells * 4);
            gpu.queue.submit(Some(encoder.finish()));
            super::super::runtime::read_u32_prefix(&gpu.device, &staging, cells * 4)
        })
        .unwrap()
    }

    // AI-FUNC-SUMMARY: Certified voxel occupancy equals the CPU f64 reference cell for cell on adversarial meshes (faces on/within ulps of centers, shared faces, ray through vertices/edges, sub-1e-6 slab, coincident duplicates, tiny features) at the origin and translated by 1e9, in full and resident-count modes, with patched resident cells; the frozen uncertified shader is shown to disagree on the slab.
    #[test]
    fn certified_voxels_equal_cpu_reference() {
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let mut uncertain_total = 0u64;
        for (name, mesh) in adversarial_meshes(PITCH) {
            for shift in [0.0, 1e9] {
                let (mesh, bbox) = shifted(&mesh, shift);
                let mut gpu = GpuVoxelPipeline::new(&mesh, bbox).unwrap();
                let expected = cpu_reference(&gpu, N, PITCH);
                assert_eq!(gpu.voxelize(N, N, N, PITCH).unwrap(), expected, "{name} shift={shift}");
                let flagged = gpu.last_uncertain.len();
                assert_eq!(resident(&gpu, u64::from(N * N * N)), expected, "{name} resident");
                gpu.release_grid_capacity().unwrap();
                let count = gpu.voxelize_count(N, N, N, PITCH).unwrap();
                assert_eq!(count, expected.iter().sum::<u32>(), "{name} count shift={shift}");
                assert_eq!(resident(&gpu, u64::from(N * N * N)), expected, "{name} resident count");
                let stats = gpu.certification_stats();
                assert_eq!(stats.queries, 2 * u64::from(N * N * N));
                uncertain_total += stats.uncertain;
                eprintln!(
                    "VOXEL_CERT mesh={name} shift={shift:e} flagged={flagged} {}",
                    stats.describe()
                );
                if matches!(name, "faces_on_centers" | "coincident_duplicates" | "vertex_on_ray") {
                    assert!(flagged > 0, "{name}: adversarial cells must be flagged");
                }
                if name == "thin_slab" && shift == 0.0 {
                    let mut baseline = GpuVoxelPipeline::new_with_shader(
                        &mesh,
                        bbox,
                        include_str!("../../tests/fixtures/voxelize_uncertified.wgsl"),
                    )
                    .unwrap();
                    let raw = baseline.voxelize(N, N, N, PITCH).unwrap();
                    let wrong = raw.iter().zip(&expected).filter(|(a, b)| a != b).count();
                    eprintln!("VOXEL_CERT uncertified thin_slab disagreements={wrong}");
                    assert!(wrong > 0, "the uncertified f32 dedup must be shown to fail here");
                }
            }
        }
        assert!(uncertain_total > 0);
    }

    // AI-FUNC-SUMMARY: Force more uncertain cells than the planned list capacity (coincident duplicate shells on a 24^3 grid) and verify regrowth re-dispatches to the same exact result in both modes.
    #[test]
    fn uncertain_list_overflow_regrows() {
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let (_, mesh) = adversarial_meshes(PITCH)
            .into_iter()
            .find(|(name, _)| *name == "coincident_duplicates")
            .unwrap();
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let (n, pitch) = (24u32, 4.0f32 / 24.0);
        let mut gpu = GpuVoxelPipeline::new(&mesh, bbox).unwrap();
        let expected = cpu_reference(&gpu, n, pitch);
        for count_only in [false, true] {
            super::super::runtime::scoped(&gpu.device.clone(), || {
                let (buffer, staging) = voxel_uncertain_buffers(&gpu.device, 1);
                gpu.uncertain_buffer = buffer;
                gpu.uncertain_staging = staging;
                Ok(())
            })
            .unwrap();
            let before = gpu.certification_stats().list_regrowths;
            if count_only {
                assert_eq!(gpu.voxelize_count(n, n, n, pitch).unwrap(), expected.iter().sum::<u32>());
            } else {
                assert_eq!(gpu.voxelize(n, n, n, pitch).unwrap(), expected);
            }
            assert_eq!(gpu.certification_stats().list_regrowths, before + 1);
            assert!(gpu.last_uncertain.len() > UNCERTAIN_INITIAL);
        }
    }

    // AI-FUNC-SUMMARY: A forced uncertain-list overflow past the planned list is refused with a named error when the budget headroom is zero, and succeeds with the exact grid under ample headroom.
    #[test]
    fn uncertain_list_regrowth_respects_the_budget_headroom() {
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let (_, mesh) = adversarial_meshes(PITCH)
            .into_iter()
            .find(|(name, _)| *name == "coincident_duplicates")
            .unwrap();
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let (n, pitch) = (24u32, 4.0f32 / 24.0);
        let mut gpu = GpuVoxelPipeline::new(&mesh, bbox).unwrap();
        let expected = cpu_reference(&gpu, n, pitch);
        let shrink = |gpu: &mut GpuVoxelPipeline| {
            super::super::runtime::scoped(&gpu.device.clone(), || {
                let (buffer, staging) = voxel_uncertain_buffers(&gpu.device, 1);
                gpu.uncertain_buffer = buffer;
                gpu.uncertain_staging = staging;
                Ok(())
            })
            .unwrap();
        };
        shrink(&mut gpu);
        gpu.set_regrowth_headroom(Some(0));
        let refused = gpu.voxelize(n, n, n, pitch).unwrap_err();
        assert!(refused.contains("regrowth"), "{refused}");
        shrink(&mut gpu);
        gpu.set_regrowth_headroom(Some(1 << 30));
        assert_eq!(gpu.voxelize(n, n, n, pitch).unwrap(), expected);
    }

    // AI-FUNC-SUMMARY: Release benchmark: certified versus frozen uncertified voxelization on an ordinary sphere and data/input/particles.stl (64^3 grid); one warmup then five alternating samples, logging seconds and recompute ratios.
    #[test]
    #[ignore = "release voxel certification overhead benchmark"]
    fn certification_overhead_benchmark() {
        let sphere = crate::geometry::icosphere_mesh(Vec3::new(2.0, 2.0, 2.0), 1.3, 3);
        let mut cases = vec![("icosphere_l3", sphere, BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0)))];
        if let Ok(particles) = crate::io::load_stl(std::path::Path::new("data/input/particles.stl")) {
            let bbox = crate::geometry::mesh_bbox(&particles).unwrap();
            cases.push(("particles", particles, bbox));
        }
        for (name, mesh, bbox) in cases {
            let size = bbox.size();
            let pitch = (size.x.max(size.y).max(size.z) / 64.0) as f32;
            let mut certified = GpuVoxelPipeline::new(&mesh, bbox).expect("benchmark requires GPU");
            let mut baseline = GpuVoxelPipeline::new_with_shader(
                &mesh,
                bbox,
                include_str!("../../tests/fixtures/voxelize_uncertified.wgsl"),
            )
            .unwrap();
            let dims = [size.x, size.y, size.z].map(|s| ((s / pitch as f64).ceil() as u32).max(1));
            let mut run = |gpu: &mut GpuVoxelPipeline| {
                let start = std::time::Instant::now();
                let count = gpu.voxelize_count(dims[0], dims[1], dims[2], pitch).unwrap();
                (start.elapsed().as_secs_f64(), count)
            };
            run(&mut certified);
            run(&mut baseline);
            let warm = certified.certification_stats();
            for sample in 0..5 {
                let (c, b) = if sample % 2 == 0 {
                    (run(&mut certified), run(&mut baseline))
                } else {
                    let b = run(&mut baseline);
                    (run(&mut certified), b)
                };
                eprintln!(
                    "VOXEL_CERT_BENCH mesh={name} faces={} grid={dims:?} sample={sample} certified_seconds={:.6} uncertified_seconds={:.6} certified_count={} uncertified_count={}",
                    mesh.faces.len(), c.0, b.0, c.1, b.1
                );
            }
            let stats = certified.certification_stats();
            eprintln!(
                "VOXEL_CERT_BENCH mesh={name} per_run_uncertain={} per_run_queries={} {}",
                (stats.uncertain - warm.uncertain) / 5,
                (stats.queries - warm.queries) / 5,
                stats.describe()
            );
        }
    }
}
