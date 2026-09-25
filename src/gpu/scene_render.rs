use crate::compute::render_memory::{
    SceneRenderMemory, LINE_VERTEX_BYTES, SCENE_UNIFORM_BYTES, SCENE_VERTEX_BYTES,
};
use crate::geometry::render::{RenderCamera, RenderProjection};
use crate::geometry::scene_render::SceneRenderSettings;
use crate::meshgen::render_scene::RenderScene;
use crate::types::{RenderedImage, Vec3};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Clip-space depth pulled off overlay lines so they win against coincident surfaces.
const LINE_DEPTH_BIAS: f32 = 1.0e-4;
/// World-space marker arm length, as a fraction of the scene bounding-box diagonal.
const MARKER_ARM_FRAC: f64 = 0.01;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineVertex {
    position: [f32; 3],
    _pad: f32,
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneUniforms {
    mvp: [[f32; 4]; 4],
    camera_forward: [f32; 4],
    camera_eye: [f32; 4],
    clip_plane: [f32; 4],
    params: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<SceneVertex>() as u64 == SCENE_VERTEX_BYTES);
const _: () = assert!(std::mem::size_of::<LineVertex>() as u64 == LINE_VERTEX_BYTES);
const _: () = assert!(std::mem::size_of::<SceneUniforms>() as u64 == SCENE_UNIFORM_BYTES);

// AI-FUNC-SUMMARY:
// Purpose: Optional half-space clip for the GPU preview: fragments on the positive side of the plane are discarded.
// Notes: This is a *smooth* cut, unlike the extraction-time `clip_plane` filter, which is a
//   crinkle clip on whole cells. The two are independent; the renderer never applies one implicitly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuClipPlane {
    pub origin: Vec3,
    pub normal: Vec3,
}

// AI-FUNC-SUMMARY: GPU-only appearance options layered on top of SceneRenderSettings; side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GpuSceneOptions {
    pub clip_plane: Option<GpuClipPlane>,
    pub show_segments: bool,
    pub show_markers: bool,
}

impl GpuSceneOptions {
    // AI-FUNC-SUMMARY: Options with overlays enabled and no clip plane; returns GpuSceneOptions; side effects: none.
    pub fn with_overlays() -> Self {
        Self {
            clip_plane: None,
            show_segments: true,
            show_markers: true,
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Offscreen GPU preview renderer for an extracted RenderScene (PLAN §9.2 GPU path).
// Notes: Holds the device, both render pipelines and the shared bind-group layout. Opaque only:
//   per-set and per-region opacities are ignored, which is the documented iteration-1 GPU
//   limitation — the CPU renderer is the transparency reference.
pub struct GpuScenePipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    tri_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

// AI-FUNC-SUMMARY: Convert a row-major f64 view-projection matrix to column-major f32 for WGSL; side effects: none.
fn to_wgsl_mat4(m: &[[f64; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for (col, o) in out.iter_mut().enumerate() {
        for (row, v) in o.iter_mut().enumerate() {
            *v = m[row][col] as f32;
        }
    }
    out
}

// AI-FUNC-SUMMARY: Normalized RGB with unit alpha for shader upload; returns [f32; 4]; side effects: none.
fn color4(c: [u8; 3]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        1.0,
    ]
}

// AI-FUNC-SUMMARY:
// Purpose: Expand scene triangles into per-corner vertices with flat face normals and per-face colour.
// Inputs: the extracted scene.
// Returns: Vec<SceneVertex>, 3 per triangle.
// Side effects: None.
// Notes: Fully transparent triangles (alpha 0) are dropped so the opaque preview does not paint
//   geometry the CPU reference would have shown straight through.
fn build_triangle_vertices(scene: &RenderScene, capacity: usize) -> Vec<SceneVertex> {
    let mut out = Vec::with_capacity(capacity);
    for t in &scene.tris {
        if t.alpha <= 0.0 {
            continue;
        }
        let n = t.b.sub(t.a).cross(t.c.sub(t.a));
        let len = n.dot(n).sqrt();
        let normal = if len > 1e-20 {
            [(n.x / len) as f32, (n.y / len) as f32, (n.z / len) as f32]
        } else {
            [0.0, 0.0, 0.0]
        };
        let color = color4(t.color);
        for v in [t.a, t.b, t.c] {
            out.push(SceneVertex {
                position: [v.x as f32, v.y as f32, v.z as f32],
                normal,
                color,
            });
        }
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Build the LineList vertex buffer from curve segments and marker crosses.
// Inputs: the scene and the option flags.
// Returns: Vec<LineVertex> (pairs of endpoints).
// Side effects: None.
// Notes: Markers become three world-space axis arms sized from the scene bbox, because a
//   screen-space cross (what the CPU renderer draws) is not expressible in this pipeline.
fn build_line_vertices(
    scene: &RenderScene,
    options: &GpuSceneOptions,
    capacity: usize,
) -> Vec<LineVertex> {
    let mut out = Vec::with_capacity(capacity);
    let mut push = |a: Vec3, b: Vec3, c: [u8; 3]| {
        let color = color4(c);
        for p in [a, b] {
            out.push(LineVertex {
                position: [p.x as f32, p.y as f32, p.z as f32],
                _pad: 0.0,
                color,
            });
        }
    };
    if options.show_segments {
        for s in &scene.segments {
            push(s.a, s.b, s.color);
        }
    }
    if options.show_markers && !scene.markers.is_empty() {
        let arm = scene
            .bbox
            .map(|b| b.max.sub(b.min))
            .map(|d| d.dot(d).sqrt() * MARKER_ARM_FRAC)
            .filter(|v| *v > 0.0)
            .unwrap_or(1.0);
        for m in &scene.markers {
            for axis in [
                Vec3::new(arm, 0.0, 0.0),
                Vec3::new(0.0, arm, 0.0),
                Vec3::new(0.0, 0.0, arm),
            ] {
                push(m.p.sub(axis), m.p.add(axis), m.color);
            }
        }
    }
    out
}

impl GpuScenePipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu and build the scene triangle + line render pipelines (no surface/window).
    // Inputs: none.
    // Returns: Ok(GpuScenePipeline) or an init error message.
    // Side effects: Heavy wgpu device init; compiles scene_render.wgsl.
    // Notes: Honors RUSTMSPT_GPU_DEVICE through the shared adapter helper. No face culling, so
    //   boundary faces are visible from either side exactly as in the CPU reference.
    pub fn new() -> Result<Self, String> {
        let shared = super::context::shared_device()?;
        let (device, queue) = (shared.device().clone(), shared.queue().clone());

        super::runtime::scoped(&device.clone(), || {
            let (tri_pipeline, line_pipeline, bind_group_layout) = shared.cached_pipeline("scene_render", include_str!("shaders/scene_render.wgsl"), |device| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("scene_render"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene_render.wgsl").into()),
                });

                let bind_group_layout =
                    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("scene_bgl"),
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
                    label: Some("scene_pl"),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

                // Scene extraction uploads Volume boundaries first and tagged Face cells
                // second. LessEqual lets a coincident Face win, matching the CPU
                // renderer's explicit Face-over-Volume deduplication rule.
                let depth_stencil = Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                });
                let targets = [Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })];

                let tri_layout = wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SceneVertex>() as u64,
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
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 24,
                            shader_location: 2,
                        },
                    ],
                };
                let tri_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("scene_tri_pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_tri"),
                        compilation_options: Default::default(),
                        buffers: &[tri_layout],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_tri"),
                        compilation_options: Default::default(),
                        targets: &targets,
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: depth_stencil.clone(),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

                let line_layout = wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 1,
                        },
                    ],
                };
                let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("scene_line_pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_line"),
                        compilation_options: Default::default(),
                        buffers: &[line_layout],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_line"),
                        compilation_options: Default::default(),
                        targets: &targets,
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::LineList,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });
                (tri_pipeline, line_pipeline, bind_group_layout)
            })?;

            Ok(Self {
                device,
                queue,
                tri_pipeline,
                line_pipeline,
                bind_group_layout,
            })
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Render one scene from one camera as an opaque GPU preview.
    // Inputs: scene, validated camera, resolution, appearance settings, GPU-only options.
    // Returns: Ok(RenderedImage) or Err(message).
    // Side effects: Allocates GPU resources; blocks on staging-buffer readback.
    pub fn render(
        &mut self,
        scene: &RenderScene,
        camera: &RenderCamera,
        width: usize,
        height: usize,
        settings: &SceneRenderSettings,
        options: &GpuSceneOptions,
    ) -> Result<RenderedImage, String> {
        let mut images = self.render_views(
            scene,
            std::slice::from_ref(camera),
            width,
            height,
            settings,
            options,
        )?;
        Ok(images.remove(0))
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Render one scene from several cameras in a single call (the batch-views path).
    // Inputs: scene, camera list, resolution, appearance settings, GPU-only options.
    // Returns: Ok(one RenderedImage per camera, in order) or Err(message).
    // Side effects: Allocates GPU resources; blocks on staging-buffer readback once per view.
    // Notes: The geometry buffers are built and uploaded **once** and reused across every view —
    //   render_views_to reuses the uniform buffer and render targets; this wrapper collects owned
    //   images for callers that explicitly need the complete batch.
    pub fn render_views(
        &mut self,
        scene: &RenderScene,
        cameras: &[RenderCamera],
        width: usize,
        height: usize,
        settings: &SceneRenderSettings,
        options: &GpuSceneOptions,
    ) -> Result<Vec<RenderedImage>, String> {
        let mut images = Vec::with_capacity(cameras.len());
        self.render_views_to(
            scene,
            cameras,
            width,
            height,
            settings,
            options,
            |_, image| {
                images.push(image);
                Ok(())
            },
        )?;
        Ok(images)
    }

    // AI-FUNC-SUMMARY: Upload geometry once, reuse one target/staging set, and deliver each owned image in order; stop on rendering or consumer error.
    pub fn render_views_to(
        &mut self,
        scene: &RenderScene,
        cameras: &[RenderCamera],
        width: usize,
        height: usize,
        settings: &SceneRenderSettings,
        options: &GpuSceneOptions,
        mut consume: impl FnMut(usize, RenderedImage) -> Result<(), String>,
    ) -> Result<(), String> {
        if width == 0 || height == 0 {
            return Err("render resolution must be positive".to_string());
        }
        let limits = self.device.limits();
        if width > limits.max_texture_dimension_2d as usize
            || height > limits.max_texture_dimension_2d as usize
        {
            return Err("scene resolution exceeds GPU texture limits".into());
        }
        let plan = SceneRenderMemory::plan(
            scene.tris.iter().filter(|t| !(t.alpha <= 0.0)).count(),
            if options.show_segments {
                scene.segments.len()
            } else {
                0
            },
            if options.show_markers {
                scene.markers.len()
            } else {
                0
            },
            width,
            height,
        )?;
        plan.check_buffers(limits.max_buffer_size)?;
        let padded = usize::try_from(plan.padded_row_bytes)
            .map_err(|_| "scene row exceeds host address range")?;
        usize::try_from(plan.staging_bytes)
            .map_err(|_| "scene staging exceeds host address range")?;
        super::runtime::scoped(&self.device.clone(), || {
            let background = settings.background;
            let tri_vertices = build_triangle_vertices(scene, plan.triangle_vertices as usize);
            let line_vertices = build_line_vertices(scene, options, plan.line_vertices as usize);
            debug_assert_eq!(tri_vertices.len() as u64, plan.triangle_vertices);
            debug_assert_eq!(line_vertices.len() as u64, plan.line_vertices);
            if tri_vertices.is_empty() && line_vertices.is_empty() {
                for index in 0..cameras.len() {
                    consume(index, RenderedImage::filled(width, height, background))?;
                }
                return Ok(());
            }

            let tri_buffer =
                self.upload_vertices("scene_tri_vertices", bytemuck::cast_slice(&tri_vertices));
            let line_buffer =
                self.upload_vertices("scene_line_vertices", bytemuck::cast_slice(&line_vertices));
            drop(tri_vertices);
            drop(line_vertices);

            let extent = wgpu::Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: 1,
            };
            let unpadded_bpr = width * 4;
            let padded_bpr = padded;

            let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene_uniforms"),
                size: std::mem::size_of::<SceneUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene_bg"),
                layout: &self.bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });

            let color_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scene_color"),
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
                label: Some("scene_depth"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

            let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene_readback"),
                size: (padded_bpr * height) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            for (index, camera) in cameras.iter().enumerate() {
                let uniforms = self.build_uniforms(camera, settings, options);
                self.queue
                    .write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("scene_enc"),
                        });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("scene_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &color_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: background[0] as f64 / 255.0,
                                    g: background[1] as f64 / 255.0,
                                    b: background[2] as f64 / 255.0,
                                    a: background[3] as f64 / 255.0,
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
                    pass.set_bind_group(0, &bind_group, &[]);
                    if let Some(buffer) = &tri_buffer {
                        pass.set_pipeline(&self.tri_pipeline);
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..plan.triangle_vertices as u32, 0..1);
                    }
                    if let Some(buffer) = &line_buffer {
                        pass.set_pipeline(&self.line_pipeline);
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..plan.line_vertices as u32, 0..1);
                    }
                }

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
                let (sender, receiver) = std::sync::mpsc::sync_channel(1);
                slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sender.send(result);
                });
                self.device.poll(wgpu::Maintain::Wait);
                receiver
                    .recv()
                    .map_err(|e| format!("scene map callback unavailable: {e}"))?
                    .map_err(|e| format!("scene readback failed: {e}"))?;
                let data = slice.get_mapped_range();
                let mut rgba = vec![0u8; unpadded_bpr * height];
                for row in 0..height {
                    let src = &data[row * padded_bpr..row * padded_bpr + unpadded_bpr];
                    rgba[row * unpadded_bpr..(row + 1) * unpadded_bpr].copy_from_slice(src);
                }
                drop(data);
                readback.unmap();
                consume(index, RenderedImage::new(width, height, rgba))?;
            }
            Ok(())
        })
    }

    // AI-FUNC-SUMMARY: Upload a vertex slice, or None when empty; returns Option<wgpu::Buffer>; side effects: allocates and writes a GPU buffer.
    fn upload_vertices(&self, label: &str, bytes: &[u8]) -> Option<wgpu::Buffer> {
        if bytes.is_empty() {
            return None;
        }
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buffer, 0, bytes);
        Some(buffer)
    }

    // AI-FUNC-SUMMARY: Pack the per-view uniform block (matrix, camera, clip plane, shading params); returns SceneUniforms; side effects: none.
    fn build_uniforms(
        &self,
        camera: &RenderCamera,
        settings: &SceneRenderSettings,
        options: &GpuSceneOptions,
    ) -> SceneUniforms {
        let is_perspective = camera.projection == RenderProjection::Perspective;
        let (clip_plane, clip_on) = match options.clip_plane {
            Some(p) => {
                let n = p.normal;
                let len = n.dot(n).sqrt();
                if len > 0.0 {
                    let n = n.scale(1.0 / len);
                    (
                        [n.x as f32, n.y as f32, n.z as f32, -n.dot(p.origin) as f32],
                        1.0,
                    )
                } else {
                    ([0.0; 4], 0.0)
                }
            }
            None => ([0.0; 4], 0.0),
        };
        SceneUniforms {
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
            clip_plane,
            params: [
                settings.ambient as f32,
                if is_perspective { 1.0 } else { 0.0 },
                clip_on,
                LINE_DEPTH_BIAS,
            ],
        }
    }
}
