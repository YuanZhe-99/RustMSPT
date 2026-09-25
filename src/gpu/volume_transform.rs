use nalgebra::{Matrix3, Vector3};
use super::runtime::CountedWrite;

const WORKGROUP_SIZE: u32 = 64;
const PARAMS_BYTES: u64 = 160;

/// The error a tile returns when its shader sampled a source voxel outside the uploaded block; callers match on it to retry with a larger block.
pub const HALO_GUARD_ERROR: &str = "crop GPU tile sampled source outside its uploaded halo block";

// AI-FUNC-SUMMARY: Describe one output tile and the source sub-block (origin/dims inside the full source) uploaded for it; carries no GPU state.
pub struct TransformTile<'a> {
    pub block: &'a [i32],
    pub block_origin: [u32; 3],
    pub block_dims: [u32; 3],
    pub source_dims: [u32; 3],
    pub tile_offset: [u32; 3],
    pub tile_dims: [u32; 3],
}

pub struct GpuVolumeTransformPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    src_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    guard_buffer: wgpu::Buffer,
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
        let shared = super::context::shared_device()?;
        let (device, queue) = (shared.device().clone(), shared.queue().clone());
        super::runtime::scoped(&device.clone(), || {
            let (pipeline, bgl) = shared.cached_pipeline("volume_transform", include_str!("shaders/volume_transform.wgsl"), |device| {
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
                (pipeline, bgl)
            })?;

            let src_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("src"),
                size: 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("params"),
                size: PARAMS_BYTES,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out"),
                size: 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("volume_transform_staging"), size: 8,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let guard_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("volume_transform_guard"),
                size: 4,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            Ok(Self {
                device,
                queue,
                pipeline,
                src_buffer,
                params_buffer,
                out_buffer,
                staging_buffer,
                guard_buffer,
                bind_group_layout: bgl,
                current_src_size: 4,
                current_out_size: 4,
            })
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Rotate and crop a whole 3D volume on GPU in one dispatch with the full source resident.
    // Inputs: source volume data (i32), dimensions, background value, rotation matrix, centroid,
    //         output origin (x0,y0,z0), output dimensions, interpolation mode (0=nearest, 1=trilinear).
    // Returns: Checked output volume or a capacity, parameter, device or readback error.
    // Side effects: Dispatches GPU compute, maps staging buffer.
    // Notes: Equivalent to transform_tile with the whole source as block and the whole output as tile.
    #[allow(clippy::too_many_arguments)]
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
    ) -> Result<Vec<i32>, String> {
        let tile = TransformTile {
            block: src_data,
            block_origin: [0, 0, 0],
            block_dims: [src_w, src_h, src_d],
            source_dims: [src_w, src_h, src_d],
            tile_offset: [0, 0, 0],
            tile_dims: [out_w, out_h, out_d],
        };
        self.transform_tile(&tile, background, rot, centroid, origin, interp_mode)
    }

    // AI-FUNC-SUMMARY: Return the device limits used to bound per-tile source blocks and outputs; side effects: none.
    pub fn device_limits(&self) -> wgpu::Limits {
        self.device.limits()
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Grow retained source-block and output/staging capacity once to the largest tile of a plan.
    // Inputs: source-block bytes and output-tile bytes (each raised to at least 4).
    // Returns: Ok or a captured allocation/validation error.
    // Side effects: May replace source, output and staging buffers; never shrinks them.
    // Notes: Pre-sizing avoids old/new buffer overlap between tiles, matching the crop memory plan.
    pub fn reserve_capacity(&mut self, src_bytes: u64, out_bytes: u64) -> Result<(), String> {
        let src_bytes = src_bytes.max(4);
        let out_bytes = out_bytes.max(4);
        super::runtime::scoped(&self.device.clone(), || {
            if src_bytes > self.current_src_size {
                self.resize_source_buffer(src_bytes);
            }
            if out_bytes > self.current_out_size {
                self.resize_output_buffers(out_bytes)?;
            }
            Ok(())
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Transform one output tile using only an uploaded source sub-block while reproducing single-dispatch arithmetic.
    // Inputs: tile descriptor (block values/origin/dims, full source dims, output tile offset/dims), background,
    //         rotation, centroid, global output origin and interpolation mode (0=nearest, 1=trilinear).
    // Returns: The tile's output voxels in x-fastest order, or a capacity, parameter, device, readback or halo error.
    // Side effects: Uploads the block and parameters, dispatches compute, maps staging for the tile prefix plus guard word.
    // Notes: The shader forms f32(tile_offset + local) and bounds-checks against full source dims, so every voxel equals the
    //        whole-volume dispatch; an in-volume sample outside the block sets a guard and returns an error rather than a value.
    pub fn transform_tile(
        &mut self,
        tile: &TransformTile<'_>,
        background: i32,
        rot: &Matrix3<f64>,
        centroid: &Vector3<f64>,
        origin: &Vector3<f64>,
        interp_mode: u32,
    ) -> Result<Vec<i32>, String> {
        let limits = self.device.limits();
        super::runtime::grid_plan(tile.source_dims, 1, &limits)?;
        let output = super::runtime::grid_plan(tile.tile_dims, WORKGROUP_SIZE, &limits)?;
        let block_voxels = tile
            .block_dims
            .iter()
            .try_fold(1u64, |acc, &d| acc.checked_mul(u64::from(d)))
            .ok_or("crop GPU source block size overflows")?;
        if tile.block.len() as u64 != block_voxels {
            return Err("crop GPU source length does not match dimensions".into());
        }
        for axis in 0..3 {
            let end = tile.block_origin[axis]
                .checked_add(tile.block_dims[axis])
                .ok_or("crop GPU source block end overflows")?;
            if end > tile.source_dims[axis] {
                return Err("crop GPU source block exceeds source dimensions".into());
            }
            tile.tile_offset[axis]
                .checked_add(tile.tile_dims[axis])
                .ok_or("crop GPU tile end overflows")?;
        }
        let block_bytes = block_voxels
            .checked_mul(4)
            .ok_or("crop GPU source block bytes overflow")?;
        if block_bytes > limits.max_buffer_size
            || block_bytes > u64::from(limits.max_storage_buffer_binding_size)
        {
            return Err(format!(
                "crop GPU source block requires {block_bytes} bytes, exceeding device buffer limits"
            ));
        }
        if interp_mode > 1 {
            return Err("crop GPU interpolation mode is unsupported".into());
        }
        if interp_mode == 1
            && (tile.block.iter().any(|&v| (v as f32) as i64 != i64::from(v))
                || (background as f32) as i64 != i64::from(background))
        {
            return Err("crop GPU trilinear cannot represent input integers exactly as f32".into());
        }
        if !rot
            .iter()
            .chain(centroid.iter())
            .chain(origin.iter())
            .all(|&v| v.is_finite() && (v as f32).is_finite())
        {
            return Err("crop GPU transform contains nonfinite f32 parameters".into());
        }
        super::runtime::scoped(&self.device.clone(), || {
            if block_bytes > self.current_src_size {
                self.resize_source_buffer(block_bytes);
            }
            if !tile.block.is_empty() {
                self.queue
                    .write_counted(&self.src_buffer, 0, bytemuck::cast_slice::<i32, u8>(tile.block));
            }
            let out_total = output.bytes;
            if out_total > self.current_out_size {
                self.resize_output_buffers(out_total)?;
            }
            self.queue.write_counted(&self.guard_buffer, 0, &0u32.to_le_bytes());

            let mut param_data = Vec::with_capacity(PARAMS_BYTES as usize);
            for value in tile.source_dims.iter().chain(tile.tile_dims.iter()) {
                param_data.extend_from_slice(&value.to_le_bytes());
            }
            param_data.extend_from_slice(&interp_mode.to_le_bytes());
            param_data.extend_from_slice(&background.to_le_bytes());
            for row in 0..3 {
                for col in 0..3 {
                    param_data.extend_from_slice(&(rot[(row, col)] as f32).to_le_bytes());
                }
                param_data.extend_from_slice(&0.0f32.to_le_bytes());
            }
            for vector in [centroid, origin] {
                for value in vector.iter() {
                    param_data.extend_from_slice(&(*value as f32).to_le_bytes());
                }
                param_data.extend_from_slice(&0.0f32.to_le_bytes());
            }
            for triple in [tile.tile_offset, tile.block_origin, tile.block_dims] {
                for value in triple {
                    param_data.extend_from_slice(&value.to_le_bytes());
                }
                param_data.extend_from_slice(&0u32.to_le_bytes());
            }
            debug_assert_eq!(param_data.len() as u64, PARAMS_BYTES);
            self.queue.write_counted(&self.params_buffer, 0, &param_data);

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
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.guard_buffer.as_entire_binding(),
                    },
                ],
            });

            let [wg_x, wg_y] = output.dispatch;

            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("vt_enc"),
                });
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("vt_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.dispatch_workgroups(wg_x, wg_y, 1);
            }

            enc.copy_buffer_to_buffer(&self.out_buffer, 0, &self.staging_buffer, 0, out_total);
            enc.copy_buffer_to_buffer(&self.guard_buffer, 0, &self.staging_buffer, out_total, 4);
            self.queue.submit(Some(enc.finish()));

            let read_bytes = out_total.checked_add(4).ok_or("crop GPU readback size overflows")?;
            let mut words =
                super::runtime::read_u32_prefix(&self.device, &self.staging_buffer, read_bytes)?;
            if words.pop() != Some(0) {
                return Err(HALO_GUARD_ERROR.into());
            }
            Ok(words.into_iter().map(|word| word as i32).collect())
        })
    }

    // AI-FUNC-SUMMARY: Replace the retained source-block buffer with the requested capacity; callers validate bytes and capture GPU errors.
    fn resize_source_buffer(&mut self, bytes: u64) {
        self.src_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("src"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.current_src_size = bytes;
    }

    // AI-FUNC-SUMMARY: Allocate output capacity and staging with one extra guard word; returns an error on size overflow; callers capture GPU errors.
    fn resize_output_buffers(&mut self, bytes: u64) -> Result<(), String> {
        let staging = bytes.checked_add(4).ok_or("crop GPU staging size overflows")?;
        self.out_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out"), size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("volume_transform_staging"), size: staging,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.current_out_size = bytes;
        Ok(())
    }

    // AI-FUNC-SUMMARY: Release retained output/readback peak while keeping source capacity and the compiled transform pipeline.
    pub fn release_output_capacity(&mut self) -> Result<(), String> {
        super::runtime::scoped(&self.device.clone(), || {
            self.resize_output_buffers(4)
        })
    }

}


#[cfg(test)]
mod reuse_tests {
    use super::*;
    // AI-FUNC-SUMMARY: Verify transform staging identity, live lengths, signed values and reuse after explicit capacity release.
    #[test]
    fn transform_staging_reuses_live_prefix() {
        let mut gpu = match GpuVolumeTransformPipeline::new() {
            Ok(gpu) => gpu,
            Err(error) => { eprintln!("SKIP: GPU unavailable: {error}"); return; }
        };
        let run = |gpu: &mut GpuVolumeTransformPipeline, values: &[i32], n| {
            gpu.rotate_and_crop(values, values.len() as u32, 1, 1, -9,
                &Matrix3::identity(), &Vector3::zeros(), &Vector3::zeros(), n, 1, 1, 0).unwrap()
        };
        assert_eq!(run(&mut gpu, &[1,2,3,4], 4), vec![1,2,3,4]);
        let staging = gpu.staging_buffer.clone();
        let output = gpu.out_buffer.clone();
        assert_eq!(run(&mut gpu, &[i32::MIN, i32::MAX], 2), vec![i32::MIN, i32::MAX]);
        assert_eq!(gpu.staging_buffer, staging);
        assert_eq!(gpu.out_buffer, output);
        assert_eq!(run(&mut gpu, &[8], 7), vec![8,-9,-9,-9,-9,-9,-9]);
        assert_ne!(gpu.staging_buffer, staging);
        gpu.release_output_capacity().unwrap();
        assert_eq!(gpu.out_buffer.size(), 4);
        assert_eq!(gpu.staging_buffer.size(), 8);
        assert_eq!(gpu.current_out_size, 4);
        assert_eq!(run(&mut gpu, &[3,2,1], 3), vec![3,2,1]);
    }
}
