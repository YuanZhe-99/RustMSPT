const WORKGROUP_SIZE: u32 = 256;
const MAX_OFFSETS: usize = crate::compute::exact_memory::EXACT_MAX_PARTIALS;

#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct OffsetEntry {
    radius_idx: u32,
    dx: i32,
    dy: i32,
    dz: i32,
}

// AI-FUNC-SUMMARY: Retain the optional device-side tile reducer and its per-offset output capacity.
struct ShellReduction {
    pipeline: wgpu::ComputePipeline,
    valid: wgpu::Buffer,
    hits: wgpu::Buffer,
}

pub struct GpuShellS2Pipeline {
    shared: std::sync::Arc<super::context::SharedGpuDevice>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    offsets_per_workgroup: u32,
    tile_voxels: Option<u32>,
    filter_offsets: bool,
    batch_partial_limit: usize,
    reduce_tiles: bool,
    reduction: Option<ShellReduction>,
    occupancy_buffer: wgpu::Buffer,
    offsets_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_valid_buffer: wgpu::Buffer,
    out_hits_buffer: wgpu::Buffer,
    staging_valid: wgpu::Buffer,
    staging_hits: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
}

// AI-FUNC-SUMMARY:
// Purpose: Build flat offset entry buffer from shell offsets per radius.
// Inputs: slice of (radius_idx, [dx, dy, dz]) tuples.
// Returns: Vec<OffsetEntry>; unrepresentable displacements use an out-of-domain i32::MAX sentinel.
// Side effects: None.
fn build_offset_buffer(shell_offsets: &[(u32, [isize; 3])]) -> Vec<OffsetEntry> {
    shell_offsets
        .iter()
        .map(|&(ridx, [dx, dy, dz])| OffsetEntry {
            radius_idx: ridx,
            dx: i32::try_from(dx).unwrap_or(i32::MAX),
            dy: i32::try_from(dy).unwrap_or(i32::MAX),
            dz: i32::try_from(dz).unwrap_or(i32::MAX),
        })
        .collect()
}

// AI-FUNC-SUMMARY: Reject displacements with no overlap using unsigned magnitude, including isize::MIN without signed overflow.
fn offset_has_overlap(shift: [isize; 3], dims: [u32; 3]) -> bool {
    shift
        .into_iter()
        .zip(dims)
        .all(|(d, n)| d.unsigned_abs() < n as usize)
}

impl GpuShellS2Pipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu and create the shell S2 compute pipeline.
    // Inputs: none (uses the process-wide shared device).
    // Returns: Ok(GpuShellS2Pipeline) or init error string.
    // Side effects: Creates the shared device honoring RUSTMSPT_GPU_DEVICE on first use; reuses cached pipelines.
    pub fn new() -> Result<Self, String> {
        let mut gpu =
            Self::new_with_shader(include_str!("shaders/s2_shell_pairs.wgsl"), WORKGROUP_SIZE)?;
        gpu.filter_offsets = true;
        Ok(gpu)
    }

    // AI-FUNC-SUMMARY: Build shell resources from the production shader or a frozen test reference for identical-input comparisons.
    fn new_with_shader(source: &str, offsets_per_workgroup: u32) -> Result<Self, String> {
        Self::build_on_device(super::context::shared_device()?, source, offsets_per_workgroup)
    }

    // AI-FUNC-SUMMARY: Build production shell resources on an already-held shared device without adapter selection or device creation; caller serializes use of resident buffers.
    pub(crate) fn with_device(
        shared: std::sync::Arc<super::context::SharedGpuDevice>,
    ) -> Result<Self, String> {
        let mut gpu = Self::build_on_device(
            shared,
            include_str!("shaders/s2_shell_pairs.wgsl"),
            WORKGROUP_SIZE,
        )?;
        gpu.filter_offsets = true;
        Ok(gpu)
    }

    // AI-FUNC-SUMMARY: Fetch or compile the shell pipeline for this source on the shared device and allocate per-instance buffers under balanced error scopes; no adapter/device initialization.
    fn build_on_device(
        shared: std::sync::Arc<super::context::SharedGpuDevice>,
        source: &str,
        offsets_per_workgroup: u32,
    ) -> Result<Self, String> {
        let (device, queue) = (shared.device().clone(), shared.queue().clone());
        super::runtime::scoped(&device.clone(), || {
            let (pipeline, bgl) = shared.cached_pipeline("s2_shell", source, |device| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("s2_shell"),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });

                let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("shell_bgl"),
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
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
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
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
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
                    label: Some("shell_pl"),
                    bind_group_layouts: &[&bgl],
                    push_constant_ranges: &[],
                });
                let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("shell_pipeline"),
                    layout: Some(&pl),
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
                (pipeline, bgl)
            })?;

            let occupancy_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("occupancy"),
                size: 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let offsets_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("offsets"),
                size: 16,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("params"),
                size: 24,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let out_size = 4;
            let out_valid_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out_valid"),
                size: out_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let out_hits_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out_hits"),
                size: out_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            let staging_valid = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shell_staging_valid"),
                size: 4,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let staging_hits = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shell_staging_hits"),
                size: 4,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            Ok(Self {
                shared: shared.clone(),
                device,
                queue,
                pipeline,
                offsets_per_workgroup,
                tile_voxels: None,
                filter_offsets: false,
                batch_partial_limit: MAX_OFFSETS,
                reduce_tiles: false,
                reduction: None,
                occupancy_buffer,
                offsets_buffer,
                params_buffer,
                out_valid_buffer,
                out_hits_buffer,
                staging_valid,
                staging_hits,
                bind_group_layout: bgl,
            })
        })
    }

    // AI-FUNC-SUMMARY: Limit subsequent partial batches for a fresh budget-planned exact run; reject zero/oversized limits or already retained capacity exceeding the requested bound.
    pub(crate) fn set_batch_partial_limit(&mut self, limit: usize) -> Result<(), String> {
        if limit == 0 || limit > MAX_OFFSETS || self.out_hits_buffer.size() > limit as u64 * 4 {
            return Err("GPU shell batch limit is invalid or below retained capacity".into());
        }
        self.batch_partial_limit = limit;
        Ok(())
    }

    // AI-FUNC-SUMMARY: Lazily fetch the cached integer tile reducer and grow this instance's per-offset outputs under the caller's GPU error scope; returns a compile error without caching it.
    fn ensure_reduction(&mut self, count: u32) -> Result<(), String> {
        let bytes = u64::from(count) * 4;
        if self
            .reduction
            .as_ref()
            .is_some_and(|r| r.valid.size() >= bytes)
        {
            return Ok(());
        }
        let pipeline = if let Some(r) = &self.reduction {
            r.pipeline.clone()
        } else {
            let layout = self.bind_group_layout.clone();
            self.shared.cached_pipeline(
                "s2_shell_reduce",
                include_str!("shaders/s2_shell_reduce.wgsl"),
                |device| {
                    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("shell tile reduction"),
                        source: wgpu::ShaderSource::Wgsl(
                            include_str!("shaders/s2_shell_reduce.wgsl").into(),
                        ),
                    });
                    let pipeline_layout =
                        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                            label: Some("shell reduction layout"),
                            bind_group_layouts: &[&layout],
                            push_constant_ranges: &[],
                        });
                    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some("shell reduction"),
                        layout: Some(&pipeline_layout),
                        module: &module,
                        entry_point: Some("main"),
                        compilation_options: Default::default(),
                        cache: None,
                    })
                },
            )?
        };
        let make = |label| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        self.reduction = Some(ShellReduction {
            pipeline,
            valid: make("shell final valid"),
            hits: make("shell final hits"),
        });
        Ok(())
    }

    // AI-FUNC-SUMMARY: Resize offset/output/readback buffers together for a nonempty bounded batch; caller owns the GPU error scope.
    fn resize_batch_buffers(&mut self, count: u64) {
        let make = |label, size, usage| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        self.offsets_buffer = make(
            "offsets",
            count * 16,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        self.out_valid_buffer = make(
            "out_valid",
            count * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        self.out_hits_buffer = make(
            "out_hits",
            count * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        self.staging_valid = make(
            "shell_staging_valid",
            count * 4,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        self.staging_hits = make(
            "shell_staging_hits",
            count * 4,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
    }

    // AI-FUNC-SUMMARY: Release retained batch capacity to one offset while preserving occupancy and compiled pipeline; return GPU allocation errors.
    pub fn release_batch_capacity(&mut self) -> Result<(), String> {
        super::runtime::scoped(&self.device.clone(), || {
            self.resize_batch_buffers(1);
            if let Some(state) = &mut self.reduction {
                let make = || {
                    self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("released shell reduction output"),
                        size: 4,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    })
                };
                state.valid = make();
                state.hits = make();
            }
            Ok(())
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Compute exact S2 in bounded GPU offset chunks, preserving every valid per-offset ratio.
    // Inputs: occupancy grid (u32), grid dimensions, shell offsets per radius, r_max, voxel pitch, volume fraction.
    // Returns: S2 values or a dimension, capacity, execution or readback error.
    // Side effects: Dispatches GPU compute, maps staging buffers.

    pub fn compute_s2_shell(
        &mut self,
        occ: &[u32],
        nx: u32,
        ny: u32,
        nz: u32,
        shell_offsets: &[(u32, [isize; 3])],
        r_max: usize,
        voxel_pitch: f64,
        vf: f64,
    ) -> Result<Vec<f64>, String> {
        self.compute_shell_input(
            Some(occ),
            None,
            nx,
            ny,
            nz,
            shell_offsets.iter().copied(),
            r_max,
            voxel_pitch,
            vf,
        )
    }

    // AI-FUNC-SUMMARY: Count pairs directly from a same-device occupancy buffer without upload; caller ensures completed voxelization and serial access for this evaluation.
    pub(crate) fn compute_s2_shell_resident(
        &mut self,
        occupancy: &wgpu::Buffer,
        nx: u32,
        ny: u32,
        nz: u32,
        shell_offsets: &[(u32, [isize; 3])],
        r_max: usize,
        voxel_pitch: f64,
        vf: f64,
    ) -> Result<Vec<f64>, String> {
        self.compute_shell_input(
            None,
            Some(occupancy),
            nx,
            ny,
            nz,
            shell_offsets.iter().copied(),
            r_max,
            voxel_pitch,
            vf,
        )
    }

    // AI-FUNC-SUMMARY: Consume a lazy ordered offset iterator in bounded batches against a resident grid without collecting the full offset stream.
    pub(crate) fn compute_s2_shell_resident_stream<I: Iterator<Item = (u32, [isize; 3])>>(
        &mut self,
        occupancy: &wgpu::Buffer,
        dims: [u32; 3],
        offsets: I,
        r_max: usize,
        voxel_pitch: f64,
        vf: f64,
    ) -> Result<Vec<f64>, String> {
        self.compute_shell_input(
            None,
            Some(occupancy),
            dims[0],
            dims[1],
            dims[2],
            offsets,
            r_max,
            voxel_pitch,
            vf,
        )
    }

    // AI-FUNC-SUMMARY: Validate and execute shell counting from exactly one host or resident grid, preserving the same offset batching and reduction semantics.
    fn compute_shell_input<I: Iterator<Item = (u32, [isize; 3])>>(
        &mut self,
        occ: Option<&[u32]>,
        resident: Option<&wgpu::Buffer>,
        nx: u32,
        ny: u32,
        nz: u32,
        shell_offsets: I,
        r_max: usize,
        _voxel_pitch: f64,
        vf: f64,
    ) -> Result<Vec<f64>, String> {
        let plan = super::runtime::grid_plan([nx, ny, nz], 1, &self.device.limits())?;
        if occ.is_some_and(|occ| occ.len() != plan.total as usize) {
            return Err("GPU shell occupancy length does not match dimensions".into());
        }
        if resident.is_some_and(|buffer| buffer.size() < plan.bytes) {
            return Err("GPU shell resident occupancy is shorter than the grid".into());
        }
        if r_max == usize::MAX {
            return Err("GPU shell radius count overflow".into());
        }
        let tile_voxels = self.tile_voxels.unwrap_or(plan.total).max(1);
        let tiles = plan.total.div_ceil(tile_voxels);
        let offsets_per_batch = self.batch_partial_limit / tiles as usize;
        if offsets_per_batch == 0 {
            return Err("GPU shell tile partials exceed the batch capacity".into());
        }
        let filter_offsets = self.filter_offsets;
        let mut supported = shell_offsets
            .filter(move |&(_, shift)| !filter_offsets || offset_has_overlap(shift, [nx, ny, nz]))
            .peekable();
        if supported.peek().is_none() {
            let mut out = vec![0.0; r_max + 1];
            out[0] = vf;
            return Ok(out);
        }
        super::runtime::scoped(&self.device.clone(), || {
            if let Some(occ) = occ {
                let occ_bytes = bytemuck::cast_slice::<u32, u8>(occ);
                let needed_occ = occ_bytes.len() as u64;
                if needed_occ > self.occupancy_buffer.size() {
                    self.occupancy_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("occupancy"),
                        size: needed_occ,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                }
                self.queue
                    .write_buffer(&self.occupancy_buffer, 0, occ_bytes);
            }

            let mut shell_sums = vec![0.0f64; r_max + 1];
            let mut shell_counts = vec![0usize; r_max + 1];
            let mut batch = Vec::new();
            loop {
                batch.clear();
                batch.extend(supported.by_ref().take(offsets_per_batch));
                if batch.is_empty() {
                    break;
                }
                let shell_offsets = batch.as_slice();
                let total_offsets = shell_offsets.len() as u32;
                let partials = total_offsets * tiles;
                if u64::from(partials) * 4 > self.out_valid_buffer.size() {
                    self.resize_batch_buffers(u64::from(partials));
                }
                let entries = build_offset_buffer(shell_offsets);
                let entries_bytes = bytemuck::cast_slice::<OffsetEntry, u8>(&entries);
                self.queue
                    .write_buffer(&self.offsets_buffer, 0, entries_bytes);

                let mut param_data = Vec::with_capacity(24);
                param_data.extend_from_slice(&total_offsets.to_le_bytes());
                param_data.extend_from_slice(&nx.to_le_bytes());
                param_data.extend_from_slice(&ny.to_le_bytes());
                param_data.extend_from_slice(&nz.to_le_bytes());
                param_data.extend_from_slice(&tiles.to_le_bytes());
                param_data.extend_from_slice(&tile_voxels.to_le_bytes());
                self.queue.write_buffer(&self.params_buffer, 0, &param_data);

                let reduce = self.reduce_tiles && tiles > 1;
                let output_tiles = if reduce { 1 } else { tiles };
                let out_needed = u64::from(total_offsets) * u64::from(output_tiles) * 4;
                if reduce {
                    self.ensure_reduction(total_offsets)?;
                }
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shell_bg"),
                    layout: &self.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: resident
                                .unwrap_or(&self.occupancy_buffer)
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: self.offsets_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: self.params_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: self.out_valid_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: self.out_hits_buffer.as_entire_binding(),
                        },
                    ],
                });

                let dispatch = super::runtime::grid_plan(
                    [partials, 1, 1],
                    self.offsets_per_workgroup,
                    &self.device.limits(),
                )?
                .dispatch;
                let mut enc = self
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("shell_enc"),
                    });
                {
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("shell_pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, &bg, &[]);
                    pass.dispatch_workgroups(dispatch[0], dispatch[1], 1);
                }

                if reduce {
                    let state = self.reduction.as_ref().unwrap();
                    let buffers = [
                        &self.out_valid_buffer,
                        &self.out_hits_buffer,
                        &self.params_buffer,
                        &state.valid,
                        &state.hits,
                    ];
                    let entries: Vec<_> = buffers
                        .iter()
                        .enumerate()
                        .map(|(binding, buffer)| wgpu::BindGroupEntry {
                            binding: binding as u32,
                            resource: buffer.as_entire_binding(),
                        })
                        .collect();
                    let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("shell reduction bindings"),
                        layout: &self.bind_group_layout,
                        entries: &entries,
                    });
                    let dispatch = super::runtime::grid_plan(
                        [total_offsets, 1, 1],
                        WORKGROUP_SIZE,
                        &self.device.limits(),
                    )?
                    .dispatch;
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("shell tile reduction"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&state.pipeline);
                    pass.set_bind_group(0, &bg, &[]);
                    pass.dispatch_workgroups(dispatch[0], dispatch[1], 1);
                }
                let (read_valid, read_hits) = if reduce {
                    let state = self.reduction.as_ref().unwrap();
                    (&state.valid, &state.hits)
                } else {
                    (&self.out_valid_buffer, &self.out_hits_buffer)
                };
                enc.copy_buffer_to_buffer(read_valid, 0, &self.staging_valid, 0, out_needed);
                enc.copy_buffer_to_buffer(read_hits, 0, &self.staging_hits, 0, out_needed);
                self.queue.submit(Some(enc.finish()));

                let valid_u32 =
                    super::runtime::read_u32_prefix(&self.device, &self.staging_valid, out_needed)?;
                let hits_u32 =
                    super::runtime::read_u32_prefix(&self.device, &self.staging_hits, out_needed)?;
                for i in 0..total_offsets as usize {
                    let ridx = shell_offsets[i].0 as usize;
                    let range = i * output_tiles as usize..(i + 1) * output_tiles as usize;
                    let valid: u64 = valid_u32[range.clone()].iter().map(|&n| u64::from(n)).sum();
                    let hits: u64 = hits_u32[range].iter().map(|&n| u64::from(n)).sum();
                    if ridx <= r_max && valid > 0 {
                        shell_sums[ridx] += hits as f64 / valid as f64;
                        shell_counts[ridx] += 1;
                    }
                }
            }

            let mut out = vec![0.0f64; r_max + 1];
            out[0] = vf;
            for r in 1..=r_max {
                if shell_counts[r] > 0 {
                    out[r] = shell_sums[r] / shell_counts[r] as f64;
                }
            }

            Ok(out)
        })
    }
}

#[cfg(test)]
mod reuse_tests {
    use super::*;

    // AI-FUNC-SUMMARY: Verify shell construction shares voxel device/queue identity, retains handles after producer drop, and recovers from a failed grid request.
    #[test]
    fn shell_reuses_voxel_device() {
        use crate::types::{BoundingBox, Vec3};
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let mut voxel = super::super::voxel::GpuVoxelPipeline::new(
            &crate::geometry::box_mesh(BoundingBox::from_size(Vec3::new(3.0, 1.0, 1.0))),
            BoundingBox::from_size(Vec3::new(3.0, 1.0, 1.0)),
        )
        .unwrap();
        let shared = voxel.shared_device();
        let mut shell = GpuShellS2Pipeline::with_device(shared.clone()).unwrap();
        assert_eq!(&shell.device, shared.device());
        assert_eq!(&shell.queue, shared.queue());
        assert_eq!(voxel.voxelize(3, 1, 1, 1.0).unwrap(), vec![1; 3]);
        let resident = voxel.occupancy_buffer();
        assert_eq!(
            shell
                .compute_s2_shell_resident(&resident, 3, 1, 1, &[(1, [1, 0, 0])], 1, 1.0, 0.0)
                .unwrap(),
            vec![0.0, 1.0]
        );
        assert_eq!(shell.occupancy_buffer.size(), 4);
        assert!(shell
            .compute_s2_shell_resident(&resident, 4, 1, 1, &[(1, [1, 0, 0])], 1, 1.0, 0.0)
            .is_err());

        assert!(shell
            .compute_s2_shell(&[1, 0, 1], 0, 1, 1, &[(1, [1, 0, 0])], 1, 1.0, 0.0)
            .is_err());
        drop(voxel);
        assert_eq!(
            shell
                .compute_s2_shell(&[1, 0, 1], 3, 1, 1, &[(1, [2, 0, 0])], 1, 1.0, 2.0 / 3.0)
                .unwrap(),
            vec![2.0 / 3.0, 1.0]
        );
    }

    // AI-FUNC-SUMMARY: Consume a generated offset stream across multiple supported batches and exhaust unsupported tails without retaining the full stream.
    #[test]
    fn resident_offset_stream_is_complete_and_bounded() {
        use crate::types::{BoundingBox, Vec3};
        use std::cell::Cell;
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let bbox = BoundingBox::from_size(Vec3::new(3.0, 1.0, 1.0));
        let mut voxel =
            super::super::voxel::GpuVoxelPipeline::new(&crate::geometry::box_mesh(bbox), bbox)
                .unwrap();
        assert_eq!(voxel.voxelize_count(3, 1, 1, 1.0).unwrap(), 3);
        let mut gpu = GpuShellS2Pipeline::with_device(voxel.shared_device()).unwrap();
        let consumed = Cell::new(0usize);
        let total = MAX_OFFSETS * 4 + 17;
        let offsets = (0..total).map(|i| {
            consumed.set(consumed.get() + 1);
            if i % 2 == 0 && i < total - 7 {
                (1, [1, 0, 0])
            } else {
                (2, [3, 0, 0])
            }
        });
        assert_eq!(consumed.get(), 0);
        assert_eq!(
            gpu.compute_s2_shell_resident_stream(
                &voxel.occupancy_buffer(),
                [3, 1, 1],
                offsets,
                2,
                1.0,
                1.0
            )
            .unwrap(),
            vec![1.0, 1.0, 0.0]
        );
        assert_eq!(consumed.get(), total);
        assert_eq!(gpu.out_hits_buffer.size(), MAX_OFFSETS as u64 * 4);
        assert_eq!(gpu.occupancy_buffer.size(), 4);
        let unsupported = (0..17).map(|_| {
            consumed.set(consumed.get() + 1);
            (1, [0, 1, 0])
        });
        assert_eq!(
            gpu.compute_s2_shell_resident_stream(
                &voxel.occupancy_buffer(),
                [3, 1, 1],
                unsupported,
                1,
                1.0,
                1.0
            )
            .unwrap(),
            vec![1.0, 0.0]
        );
        assert_eq!(consumed.get(), total + 17);
    }

    // AI-FUNC-SUMMARY: Check lazy shell buffers, shorter live prefixes, changed occupancy, batch tails and explicit release against analytic pair counts.
    #[test]
    fn shell_staging_reuses_live_batches() {
        let mut gpu = match GpuShellS2Pipeline::new() {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("SKIP: GPU shell unavailable: {err}");
                return;
            }
        };
        assert_eq!(gpu.offsets_buffer.size(), 16);
        assert_eq!(gpu.staging_hits.size(), 4);
        let offsets = vec![(1, [1, 0, 0]); MAX_OFFSETS + 1];
        assert_eq!(
            gpu.compute_s2_shell(&[1, 1], 2, 1, 1, &offsets, 1, 1.0, 1.0)
                .unwrap(),
            vec![1.0, 1.0]
        );
        let staging = gpu.staging_hits.clone();
        let uploaded = gpu.offsets_buffer.clone();
        assert_eq!(staging.size(), MAX_OFFSETS as u64 * 4);
        assert_eq!(
            gpu.compute_s2_shell(&[1, 0], 2, 1, 1, &offsets[..1], 1, 1.0, 0.5)
                .unwrap(),
            vec![0.5, 0.0]
        );
        assert_eq!(staging, gpu.staging_hits);
        assert_eq!(uploaded, gpu.offsets_buffer);
        assert_eq!(
            gpu.compute_s2_shell(&[1, 1], 2, 1, 1, &[(1, [2, 0, 0])], 1, 1.0, 1.0)
                .unwrap(),
            vec![1.0, 0.0]
        );
        gpu.release_batch_capacity().unwrap();
        assert_eq!(gpu.staging_hits.size(), 4);
        assert_eq!(gpu.offsets_buffer.size(), 16);
        assert_eq!(
            gpu.compute_s2_shell(&[1, 1], 2, 1, 1, &offsets[..3], 1, 1.0, 1.0)
                .unwrap(),
            vec![1.0, 1.0]
        );
    }

    // AI-FUNC-SUMMARY: Compare raw GPU overlap and hit counts with explicit signed-coordinate enumeration for all small offsets, thin grids and extreme displacements.
    #[test]
    fn shell_analytic_valid_counts_match_enumeration() {
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let mut gpu = GpuShellS2Pipeline::new().expect("available GPU must compile shell shader");
        gpu.filter_offsets = false;
        for variant in [0, 1, 64, 256, 4096, 65] {
            if variant == 1 {
                gpu = GpuShellS2Pipeline::new_with_shader(
                    include_str!("shaders/s2_shell_cooperative.wgsl"),
                    1,
                )
                .unwrap();
            }
            if variant > 1 {
                gpu = GpuShellS2Pipeline::new_with_shader(
                    include_str!("shaders/s2_shell_tiled.wgsl"),
                    1,
                )
                .unwrap();
                gpu.tile_voxels = Some(if variant == 65 { 64 } else { variant });
                gpu.reduce_tiles = variant == 65;
            }
            for [nx, ny, nz] in [[1u32, 1, 1], [1, 3, 7], [4, 3, 2], [9, 7, 5]] {
                let mut offsets = Vec::new();
                for dx in -(nx as isize)..=nx as isize {
                    for dy in -(ny as isize)..=ny as isize {
                        for dz in -(nz as isize)..=nz as isize {
                            offsets.push((1, [dx, dy, dz]));
                        }
                    }
                }
                offsets.extend([
                    (1, [i32::MIN as isize, 0, 0]),
                    (1, [0, i32::MAX as isize, 0]),
                ]);
                for pattern in 0..3 {
                    let occ: Vec<u32> = (0..nx * ny * nz)
                        .map(|i| match pattern {
                            0 => 0,
                            1 => 1,
                            _ => u32::from(i % 3 != 0),
                        })
                        .collect();
                    gpu.compute_s2_shell(&occ, nx, ny, nz, &offsets, 1, 1.0, 0.0)
                        .unwrap();
                    let tiles = if gpu.reduce_tiles {
                        1
                    } else {
                        (nx * ny * nz).div_ceil(gpu.tile_voxels.unwrap_or(nx * ny * nz)) as usize
                    };
                    let bytes = (offsets.len() * tiles) as u64 * 4;
                    let valid = super::super::runtime::read_u32_prefix(
                        &gpu.device,
                        &gpu.staging_valid,
                        bytes,
                    )
                    .unwrap();
                    let hits = super::super::runtime::read_u32_prefix(
                        &gpu.device,
                        &gpu.staging_hits,
                        bytes,
                    )
                    .unwrap();
                    for (i, (_, [dx, dy, dz])) in offsets.iter().enumerate() {
                        let mut expected = [0u32; 2];
                        for x in 0..nx as i64 {
                            for y in 0..ny as i64 {
                                for z in 0..nz as i64 {
                                    let [qx, qy, qz] =
                                        [x + *dx as i64, y + *dy as i64, z + *dz as i64];
                                    if qx < 0
                                        || qy < 0
                                        || qz < 0
                                        || qx >= nx as i64
                                        || qy >= ny as i64
                                        || qz >= nz as i64
                                    {
                                        continue;
                                    }
                                    expected[0] += 1;
                                    let a = ((x * ny as i64 + y) * nz as i64 + z) as usize;
                                    let b = ((qx * ny as i64 + qy) * nz as i64 + qz) as usize;
                                    expected[1] += u32::from(occ[a] == 1 && occ[b] == 1);
                                }
                            }
                        }
                        assert_eq!(
                            [
                                valid[i * tiles..(i + 1) * tiles].iter().sum::<u32>(),
                                hits[i * tiles..(i + 1) * tiles].iter().sum::<u32>()
                            ],
                            expected,
                            "dims={nx}/{ny}/{nz} offset={dx}/{dy}/{dz} pattern={pattern}"
                        );
                    }
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Exercise cooperative workgroup indexing across the device x limit, including padded groups and spare retained result capacity.
    #[test]
    fn shell_cooperative_dispatch_rows() {
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let mut gpu = GpuShellS2Pipeline::new_with_shader(
            include_str!("shaders/s2_shell_cooperative.wgsl"),
            1,
        )
        .unwrap();
        let count = gpu.device.limits().max_compute_workgroups_per_dimension as usize + 1;
        assert!(count < MAX_OFFSETS);
        let mut offsets = vec![(1, [1, 0, 0]); count + 1];
        offsets[count] = (1, [0, 0, 0]);
        gpu.compute_s2_shell(&[1, 1, 0], 3, 1, 1, &offsets, 1, 1.0, 0.0)
            .unwrap();
        // The extra retained slot has valid=3 and must survive the shorter multi-row dispatch.
        gpu.compute_s2_shell(&[1, 1, 0], 3, 1, 1, &offsets[..count], 1, 1.0, 0.0)
            .unwrap();
        let bytes = (count + 1) as u64 * 4;
        let valid = super::super::runtime::scoped(&gpu.device, || {
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            encoder.copy_buffer_to_buffer(&gpu.out_valid_buffer, 0, &gpu.staging_valid, 0, bytes);
            gpu.queue.submit(Some(encoder.finish()));
            super::super::runtime::read_u32_prefix(&gpu.device, &gpu.staging_valid, bytes)
        })
        .unwrap();
        assert!(valid[..count].iter().all(|&v| v == 2));
        assert_eq!(valid[count], 3);
        let hits = super::super::runtime::read_u32_prefix(
            &gpu.device,
            &gpu.staging_hits,
            count as u64 * 4,
        )
        .unwrap();
        assert!(hits.iter().all(|&v| v == 1));
    }

    // AI-FUNC-SUMMARY: Measure warm full shell evaluations with analytic versus enumerated valid counts, alternating order and checking identical curves outside timed sections.
    #[test]
    #[ignore = "release GPU shell valid-count benchmark"]
    fn shell_valid_count_benchmark() {
        shell_benchmark(0);
    }

    // AI-FUNC-SUMMARY: Benchmark cooperative lanes versus the direct analytic-count shader on identical occupancy and offsets.
    #[test]
    #[ignore = "release GPU shell cooperative benchmark"]
    fn shell_cooperative_benchmark() {
        shell_benchmark(1);
    }

    // AI-FUNC-SUMMARY: Compare independent voxel tiles against direct analytic evaluation over the shape matrix.
    #[test]
    #[ignore = "release GPU shell tiled benchmark"]
    fn shell_tiled_benchmark() {
        shell_benchmark(4096);
    }

    // AI-FUNC-SUMMARY: Benchmark tiled evaluation with on-device final integer reduction against direct counts.
    #[test]
    #[ignore = "release GPU shell reduced tiled benchmark"]
    fn shell_reduced_tiled_benchmark() {
        shell_benchmark(4097);
    }

    // AI-FUNC-SUMMARY: Compare production overlap filtering with the same direct shader processing every offset.
    #[test]
    #[ignore = "release GPU shell offset filtering benchmark"]
    fn shell_filtered_benchmark() {
        shell_benchmark(u32::MAX);
    }

    // AI-FUNC-SUMMARY: Verify stable filtering across a batch boundary and ensure unsupported-only requests allocate no occupancy or result storage.
    #[test]
    fn shell_offset_filtering_preserves_curves() {
        assert!(!offset_has_overlap([isize::MIN, 0, 0], [3, 1, 1]));
        assert!(!offset_has_overlap([isize::MAX, 0, 0], [3, 1, 1]));
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let mut filtered = GpuShellS2Pipeline::new().unwrap();
        let mut reference = GpuShellS2Pipeline::new_with_shader(
            include_str!("shaders/s2_shell_pairs.wgsl"),
            WORKGROUP_SIZE,
        )
        .unwrap();
        let invalid = [(1, [isize::MIN, 0, 0]), (1, [0, 1, 0]), (2, [0, 0, -1])];
        assert_eq!(
            filtered
                .compute_s2_shell(&[1, 0, 1], 3, 1, 1, &invalid, 3, 1.0, 0.25)
                .unwrap(),
            vec![0.25, 0.0, 0.0, 0.0]
        );
        assert_eq!(filtered.occupancy_buffer.size(), 4);
        assert_eq!(filtered.out_hits_buffer.size(), 4);
        let offsets: Vec<_> = (0..MAX_OFFSETS + 7)
            .flat_map(|i| {
                [
                    (1, [1, 0, 0]),
                    ((i % 3 + 1) as u32, [0, 1, 0]),
                    (2, [-2, 0, 0]),
                ]
            })
            .collect();
        assert_eq!(
            filtered
                .compute_s2_shell(&[1, 0, 1], 3, 1, 1, &offsets, 3, 1.0, 0.25)
                .unwrap(),
            reference
                .compute_s2_shell(&[1, 0, 1], 3, 1, 1, &offsets, 3, 1.0, 0.25)
                .unwrap()
        );
    }

    // AI-FUNC-SUMMARY: Verify tile-size and offset-batch invariance including a short final batch, unsupported offsets, and retained capacity reuse.
    #[test]
    fn shell_tile_batches_preserve_offset_ratios() {
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let mut direct = GpuShellS2Pipeline::new().unwrap();
        let mut tiled =
            GpuShellS2Pipeline::new_with_shader(include_str!("shaders/s2_shell_tiled.wgsl"), 1)
                .unwrap();
        let occ: Vec<u32> = (0..315).map(|i| u32::from(i % 3 != 0)).collect();
        let shifts = [[0, 0, 0], [1, -2, 1], [-3, 1, -1], [9, 0, 0], [0, 0, 4]];
        let offsets: Vec<_> = (0..40_003)
            .map(|i| ((i % 3 + 1) as u32, shifts[i % shifts.len()]))
            .collect();
        let expected = direct
            .compute_s2_shell(&occ, 9, 7, 5, &offsets, 3, 1.0, 0.25)
            .unwrap();
        for size in [64, 257, 4096, 65] {
            tiled.reduce_tiles = size == 65;
            let size = if size == 65 { 64 } else { size };
            tiled.tile_voxels = Some(size);
            assert_eq!(
                tiled
                    .compute_s2_shell(&occ, 9, 7, 5, &offsets, 3, 1.0, 0.25)
                    .unwrap(),
                expected
            );
            assert_eq!(
                tiled
                    .compute_s2_shell(&occ, 9, 7, 5, &offsets[..13], 3, 1.0, 0.25)
                    .unwrap(),
                direct
                    .compute_s2_shell(&occ, 9, 7, 5, &offsets[..13], 3, 1.0, 0.25)
                    .unwrap()
            );
        }
        let pipeline = tiled.reduction.as_ref().unwrap().pipeline.clone();
        tiled.release_batch_capacity().unwrap();
        assert_eq!(tiled.reduction.as_ref().unwrap().valid.size(), 4);
        assert_eq!(tiled.reduction.as_ref().unwrap().hits.size(), 4);
        assert_eq!(tiled.reduction.as_ref().unwrap().pipeline, pipeline);
        assert_eq!(
            tiled
                .compute_s2_shell(&occ, 9, 7, 5, &offsets, 3, 1.0, 0.25)
                .unwrap(),
            expected
        );
    }

    // AI-FUNC-SUMMARY: Run alternating warm shell evaluations across a fixed small/large workload matrix and assert identical curves.
    fn shell_benchmark(mode: u32) {
        let cooperative = mode > 0;
        use std::time::Instant;
        let mut analytic = if cooperative {
            GpuShellS2Pipeline::new_with_shader(
                include_str!("shaders/s2_shell_cooperative.wgsl"),
                1,
            )
            .expect("cooperative shader")
        } else {
            GpuShellS2Pipeline::new().expect("benchmark requires GPU")
        };
        if mode > 1 && mode != u32::MAX {
            analytic =
                GpuShellS2Pipeline::new_with_shader(include_str!("shaders/s2_shell_tiled.wgsl"), 1)
                    .unwrap();
            analytic.tile_voxels = Some(if mode == 4097 { 4096 } else { mode });
            analytic.reduce_tiles = mode == 4097;
        }
        let mut enumerated = GpuShellS2Pipeline::new_with_shader(
            include_str!("../../tests/fixtures/s2_shell_enumerated_valid.wgsl"),
            WORKGROUP_SIZE,
        )
        .unwrap();
        if cooperative {
            enumerated = GpuShellS2Pipeline::new_with_shader(
                include_str!("shaders/s2_shell_pairs.wgsl"),
                WORKGROUP_SIZE,
            )
            .unwrap();
        }
        if mode == u32::MAX {
            analytic = GpuShellS2Pipeline::new().unwrap();
        }
        let shapes = if cooperative {
            vec![
                [8, 8, 8],
                [12, 12, 12],
                [16, 16, 16],
                [24, 24, 24],
                [32, 32, 32],
                [64, 64, 64],
                [1, 1, 32768],
                [1, 32768, 1],
                [32768, 1, 1],
                [1, 128, 256],
                [128, 1, 256],
                [128, 256, 1],
            ]
        } else {
            vec![[8, 8, 8], [32, 32, 32], [64, 64, 64]]
        };
        for [nx, ny, nz] in shapes {
            for reach in [1isize, 2, 4] {
                let occ: Vec<u32> = (0..nx * ny * nz)
                    .map(|i| u32::from((i * 17 + 3) % 11 < 7))
                    .collect();
                let mut offsets = Vec::new();
                for x in -reach..=reach {
                    for y in -reach..=reach {
                        for z in -reach..=reach {
                            if [x, y, z] != [0, 0, 0] {
                                offsets.push((1, [x, y, z]));
                            }
                        }
                    }
                }
                let evaluate = |gpu: &mut GpuShellS2Pipeline| {
                    gpu.compute_s2_shell(&occ, nx, ny, nz, &offsets, 1, 1.0, 0.0)
                        .unwrap()
                };
                let expected = evaluate(&mut enumerated);
                assert_eq!(evaluate(&mut analytic), expected);
                for sample in 0..5 {
                    let measure = |gpu: &mut GpuShellS2Pipeline| {
                        let start = Instant::now();
                        let result = evaluate(gpu);
                        let elapsed = start.elapsed().as_secs_f64();
                        assert_eq!(result, expected);
                        elapsed
                    };
                    let (old, new) = if sample % 2 == 0 {
                        (measure(&mut enumerated), measure(&mut analytic))
                    } else {
                        let new = measure(&mut analytic);
                        (measure(&mut enumerated), new)
                    };
                    eprintln!("SHELL_SHAPE_BENCH nx={nx} ny={ny} nz={nz} offsets={} sample={sample} old_seconds={old:.9} new_seconds={new:.9}", offsets.len());
                }
            }
        }
    }
}
