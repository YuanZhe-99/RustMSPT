# STL Rendering

The `render` pipeline converts an STL mesh into a PNG from a configured viewpoint without opening a window. `focus_point` is the image-center target and `view_direction` points from the camera toward it. `build_render_camera` normalizes this direction, derives a right/up basis from `up_vector`, and selects a deterministic fallback axis when the default up direction is parallel to the view.

## Projection And Framing

Orthographic mode projects all eight mesh-bbox corners onto the camera right/up axes, expands the smaller extent to match the output aspect ratio, and applies `fit_padding`. Perspective mode uses a vertical FOV and either an explicit positive `camera_distance` or an automatically derived distance that fits the bbox bounding sphere in both vertical and horizontal FOV. Both modes derive near/far planes from the same sphere.

## CPU Reference Path

`render_mesh_cpu` converts the mesh once to parry3d `TriMesh`, whose QBVH accelerates `cast_local_ray_and_get_normal`. One ray is generated through every pixel center: parallel rays for orthographic projection and eye-origin rays for perspective. Rayon processes independent output rows. The nearest hit receives flat, two-sided headlight shading (`ambient + diffuse * normal·to_camera`); misses retain the white background.

## GPU Path

`GpuRenderPipeline` expands each triangle into three vertices carrying a flat face normal and rasterizes them with `render.wgsl` into an offscreen `Rgba8Unorm` color texture plus `Depth32Float` depth texture. No window or surface is created. The color texture is copied to a mapped staging buffer whose row pitch is rounded up to wgpu's 256-byte alignment, then unpadded into the shared `RenderedImage` layout.

GPU initialization and rendering errors fall back to the CPU path. CPU uses f64 ray casting while GPU uses f32 rasterization, so validation compares with channel/edge tolerances rather than byte equality.

## Output And Cross-References

`save_image` validates `width * height * 4` top-row-first RGBA8 bytes and currently accepts `.png` only. See [geometry-core.md](../reference/geometry-core.md), [gpu.md](../reference/gpu.md), [io.md](../reference/io.md), [pipeline-core.md](../reference/pipeline-core.md), and the [render walkthrough](../examples/render.md).
