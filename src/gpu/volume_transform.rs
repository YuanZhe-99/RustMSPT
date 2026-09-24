use nalgebra::{Matrix3, Vector3};

const WORKGROUP_SIZE: u32 = 64;

pub struct GpuVolumeTransformPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    src_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
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
        let (device, queue) =
            super::context::request_adapter_device("rustmspt volume transform device")?;
        super::runtime::scoped(&device.clone(), || {
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

            let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("volume_transform_staging"), size: 4,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
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
                bind_group_layout: bgl,
                current_src_size: 4,
                current_out_size: 4,
            })
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Rotate and crop a 3D volume on GPU.
    // Inputs: source volume data (i32), dimensions, background value, rotation matrix, centroid,
    //         output origin (x0,y0,z0), output dimensions, interpolation mode (0=nearest, 1=trilinear).
    // Returns: Checked output volume or a capacity, parameter, device or readback error.
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
    ) -> Result<Vec<i32>, String> {
        let limits = self.device.limits();
        let source = super::runtime::grid_plan([src_w, src_h, src_d], 1, &limits)?;
        let output = super::runtime::grid_plan([out_w, out_h, out_d], WORKGROUP_SIZE, &limits)?;
        if src_data.len() != source.total as usize {
            return Err("crop GPU source length does not match dimensions".into());
        }
        if interp_mode > 1 {
            return Err("crop GPU interpolation mode is unsupported".into());
        }
        if interp_mode == 1
            && (src_data.iter().any(|&v| (v as f32) as i64 != i64::from(v))
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

            let out_total = output.bytes;
            if out_total > self.current_out_size {
                self.resize_output_buffers(out_total);
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
            self.queue.submit(Some(enc.finish()));

            let words = super::runtime::read_u32_prefix(&self.device, &self.staging_buffer, out_total)?;
            Ok(words.into_iter().map(|word| word as i32).collect())
        })
    }
    // AI-FUNC-SUMMARY: Allocate output and staging capacity together; callers validate bytes and capture GPU errors.
    fn resize_output_buffers(&mut self, bytes: u64) {
        self.out_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out"), size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("volume_transform_staging"), size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.current_out_size = bytes;
    }

    // AI-FUNC-SUMMARY: Release retained output/readback peak while keeping source capacity and the compiled transform pipeline.
    pub fn release_output_capacity(&mut self) -> Result<(), String> {
        super::runtime::scoped(&self.device.clone(), || {
            self.resize_output_buffers(4);
            Ok(())
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
        assert_eq!(gpu.staging_buffer.size(), 4);
        assert_eq!(gpu.current_out_size, 4);
        assert_eq!(run(&mut gpu, &[3,2,1], 3), vec![3,2,1]);
    }
}
