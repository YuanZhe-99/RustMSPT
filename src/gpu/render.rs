use crate::geometry::render::{RenderCamera, RenderProjection, RenderSettings};
use crate::types::{Mesh, RenderedImage};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RenderVertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RenderUniforms {
    mvp: [[f32; 4]; 4],
    camera_forward: [f32; 4],
    camera_eye: [f32; 4],
    base_color: [f32; 4],
    params: [f32; 4],
}

pub struct GpuRenderPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

// AI-FUNC-SUMMARY:
// Purpose: Expand a mesh into per-corner vertices (3 per triangle) with flat face normals for GPU upload.
// Inputs: mesh reference.
// Returns: Vec<RenderVertex> with 24 bytes per vertex (position + normal), 3 vertices per face.
// Side effects: None.
// Notes: Degenerate (zero-area) faces get a zero normal; the rasterizer produces no fragments for
// them and the shader falls back to ambient shading for zero-length normals.
fn build_render_vertices(mesh: &Mesh) -> Vec<RenderVertex> {
    let mut vertices = Vec::with_capacity(mesh.faces.len() * 3);
    for face in &mesh.faces {
        let (a, b, c) = (
            mesh.vertices[face.a],
            mesh.vertices[face.b],
            mesh.vertices[face.c],
        );
        let ab = b.sub(a);
        let ac = c.sub(a);
        let n = ab.cross(ac);
        let n_len = n.dot(n).sqrt();
        let normal = if n_len > 1e-20 {
            let inv = 1.0 / n_len;
            [(n.x * inv) as f32, (n.y * inv) as f32, (n.z * inv) as f32]
        } else {
            [0.0, 0.0, 0.0]
        };
        for v in [a, b, c] {
            vertices.push(RenderVertex {
                position: [v.x as f32, v.y as f32, v.z as f32],
                normal,
            });
        }
    }
    vertices
}

// AI-FUNC-SUMMARY:
// Purpose: Convert a row-major f64 view-projection matrix into column-major f32 for WGSL upload.
// Inputs: 4x4 row-major matrix.
// Returns: 4x4 column-major f32 matrix (m[col][row]).
// Side effects: None.
fn to_wgsl_mat4(m: &[[f64; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            out[col][row] = m[row][col] as f32;
        }
    }
    out
}

impl GpuRenderPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu and build the offscreen mesh render pipeline (no surface/window).
    // Inputs: none.
    // Returns: Ok(GpuRenderPipeline) or init error string.
    // Side effects: Heavy wgpu device init; compiles render.wgsl.
    // Notes: Honors RUSTMSPT_GPU_DEVICE via the shared request_adapter_device helper. Two-sided
    // rendering (no face culling) so inconsistent STL winding never hides surfaces.
    pub fn new() -> Result<Self, String> {
        let (device, queue) = super::context::request_adapter_device("rustmspt render device")?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/render.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RenderVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Render a mesh to an offscreen RGBA8 image using the shared camera definition.
    // Inputs: mesh, validated camera, output resolution in pixels, shading settings.
    // Returns: Ok(RenderedImage) with unpadded RGBA rows (top-to-bottom), or Err(message).
    // Side effects: Allocates per-call GPU textures/buffers; blocks on staging-buffer readback.
    // Notes: Rendered once per call (no resource reuse across calls — the render pipeline is a
    // one-shot workload). Handles the 256-byte bytes_per_row readback alignment by copying with
    // padded rows and stripping the padding on the CPU. F32 GPU precision vs f64 CPU ray casting
    // can differ by ~1 LSB per channel and ~1px at triangle edges.
    pub fn render(
        &mut self,
        mesh: &Mesh,
        camera: &RenderCamera,
        width: usize,
        height: usize,
        settings: &RenderSettings,
    ) -> Result<RenderedImage, String> {
        if width == 0 || height == 0 {
            return Err("render resolution must be positive".to_string());
        }
        let background = [
            settings.background[0],
            settings.background[1],
            settings.background[2],
            255,
        ];

        let vertices = build_render_vertices(mesh);
        if vertices.is_empty() {
            return Ok(RenderedImage::filled(width, height, background));
        }

        let vertex_bytes = bytemuck::cast_slice(&vertices);
        let vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render_vertices"),
            size: vertex_bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&vertex_buffer, 0, vertex_bytes);

        let is_perspective = camera.projection == RenderProjection::Perspective;
        let uniforms = RenderUniforms {
            mvp: to_wgsl_mat4(&camera.view_proj_matrix()),
            camera_forward: [
                camera.forward.x as f32,
                camera.forward.y as f32,
                camera.forward.z as f32,
                0.0,
            ],
            camera_eye: [
                camera.eye.x as f32,
                camera.eye.y as f32,
                camera.eye.z as f32,
                0.0,
            ],
            base_color: [
                settings.base_color[0] as f32 / 255.0,
                settings.base_color[1] as f32 / 255.0,
                settings.base_color[2] as f32 / 255.0,
                1.0,
            ],
            params: [
                settings.ambient as f32,
                if is_perspective { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ],
        };
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render_uniforms"),
            size: std::mem::size_of::<RenderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render_bg"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let extent = wgpu::Extent3d {
            width: width as u32,
            height: height as u32,
            depth_or_array_layers: 1,
        };
        let color_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render_color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render_depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_enc"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: settings.background[0] as f64 / 255.0,
                            g: settings.background[1] as f64 / 255.0,
                            b: settings.background[2] as f64 / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }

        let unpadded_bpr = width * 4;
        let padded_bpr = unpadded_bpr.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render_readback"),
            size: (padded_bpr * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bpr as u32),
                    rows_per_image: Some(height as u32),
                },
            },
            extent,
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);

        let data = slice.get_mapped_range();
        let mut rgba = vec![0u8; unpadded_bpr * height];
        for row in 0..height {
            let src = &data[row * padded_bpr..row * padded_bpr + unpadded_bpr];
            rgba[row * unpadded_bpr..(row + 1) * unpadded_bpr].copy_from_slice(src);
        }
        drop(data);
        readback.unmap();

        Ok(RenderedImage::new(width, height, rgba))
    }
}
