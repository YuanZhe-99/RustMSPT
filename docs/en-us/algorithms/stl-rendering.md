# STL Rendering

The `render` pipeline converts an STL mesh into a PNG from a configured viewpoint without opening a window. `focus_point` is the image-center target and `view_direction` points from the camera toward it. `build_render_camera` normalizes this direction, derives a right/up basis from `up_vector`, and selects a deterministic fallback axis when the default up direction is parallel to the view.

## Projection And Framing

Orthographic mode projects all eight mesh-bbox corners onto the camera right/up axes, expands the smaller extent to match the output aspect ratio, and applies `fit_padding`. Perspective mode uses a vertical FOV and either an explicit positive `camera_distance` or an automatically derived distance that fits the bbox bounding sphere in both vertical and horizontal FOV. Both modes derive near/far planes from the same sphere.

## CPU Reference Path

`render_mesh_cpu` converts the mesh once to parry3d `TriMesh`, whose QBVH accelerates `cast_local_ray_and_get_normal`. One ray is generated through every pixel center: parallel rays for orthographic projection and eye-origin rays for perspective. Rayon processes independent contiguous pixel tasks. The nearest hit receives flat, two-sided headlight shading (`ambient + diffuse * normal·to_camera`); misses retain the white background.

## GPU Path

`GpuRenderPipeline` expands each triangle into three vertices carrying a flat face normal and rasterizes them with `render.wgsl` into an offscreen `Rgba8Unorm` color texture plus `Depth32Float` depth texture. No window or surface is created. The color texture is copied to a mapped staging buffer whose row pitch is rounded up to wgpu's 256-byte alignment, then unpadded into the shared `RenderedImage` layout.

GPU initialization and rendering errors fall back only when `cpu_fallback` permits it; otherwise the command returns an error before saving an image. CPU uses f64 ray casting while GPU uses f32 rasterization, so validation compares with channel/edge tolerances rather than byte equality.

## Output And Cross-References

`save_image` validates `width * height * 4` top-row-first RGBA8 bytes and currently accepts `.png` only. See [geometry-core.md](../reference/geometry-core.md), [gpu.md](../reference/gpu.md), [io.md](../reference/io.md), [pipeline-core.md](../reference/pipeline-core.md), and the [render walkthrough](../examples/render.md).


### Execution policy (2026-09-21)

The entire STL render pipeline runs inside its `cpu_max` pool. `RUSTMSPT_ACCELERATION` overrides the configured mode, with invalid values rejected. Auto uses `gpu_min_pixels`; explicit GPU bypasses this threshold. Device budget estimation includes expanded triangle vertices (72 bytes/face), color/depth textures (8 bytes/pixel), 256-byte-aligned readback rows and 128 uniform bytes. GPU options use the common execution validator. Device texture/buffer limits are checked before allocation, and scoped validation/allocation errors plus checked readback propagate through the fallback policy. Per-call resources are still allocated afresh; persistent scene/target caching remains pending.

### Scene preview working-set policy

`mesh_render.gpu_memory_limit_mb` optionally limits the planned logical GPU working set. `gpu_min_pixels` applies only to auto mode and defaults to zero for legacy compatibility. Below-threshold auto avoids GPU initialization; explicit GPU ignores that threshold but obeys the budget. Auto falls back on budget/execution failure, while explicit GPU fails. CPU mode ignores GPU-only resource options.

The checked planner counts visible triangles at 120 bytes each, enabled segments at 64 bytes and markers at 192 bytes. It includes one color/depth pair (8 bytes/pixel), one readback buffer (256-byte-aligned RGBA rows), and 128 uniform bytes. Pending queue uploads coexist with destination geometry/uniform buffers, giving the conservative logical peak `2 * geometry_bytes + 256 + 8 * pixels + staging_bytes`. Multiple views reuse targets. Driver/pipeline internals and host scene/PNG memory are not included, so this is not a physical VRAM/RSS cap. Device limits are checked independently before host vertex expansion; expanded host arrays are released immediately after upload. The constructor also returns scoped GPU validation/allocation errors. Image tiling remains future work.

### CPU pixel task scheduling

Both nearest-hit STL rendering and prepared transparent scene rendering use disjoint contiguous pixel tasks. Images with at least one row per worker and at most 1024 pixels per worker retain row tasks, avoiding loss of parallelism from the minimum tile grain. Other images in a one-worker pool use one task; otherwise the initial grain targets four tasks per worker, clamped to 256..4096 pixels, aligning to complete rows when a row fits. Wide rows can span several tasks and short rows can share one. Each task derives its starting `(x,y)` once and advances the original integer pixel coordinates; ray arithmetic, hit ordering/compositing and serial overlays are unchanged. Scene depth and RGBA use identical task boundaries and reuse task-local hit scratch. Row-grain reference tests compare complete images under 1/2/8 workers for both projections, ragged tasks and extreme aspect ratios. Grain performance acceptance is tracked in PLAN.Performance.md §40.

### Bounded GPU PNG writer (2026-09-23)

For multiple GPU views with more than one configured worker, `consume_frames` moves ordered images through a zero-capacity channel to one PNG writer. At most one image is being encoded and one is held by the rendering producer; GPU targets remain reused. The writer is joined and accepted frames drained before returning or starting CPU fallback. Output errors, including failure of the final frame after production succeeds, take precedence over GPU errors and never trigger fallback. One-worker and one-view execution remain sequential. CPU views still encode sequentially to keep their full Rayon worker budget. This establishes bounded overlap, not an end-to-end speedup claim; the software-GPU cold-process CLI comparison, including PNG identity and RSS, is recorded in PLAN.Performance.md §56; full workload/hardware acceptance remains open.

### Opaque nearest coincidence group (2026-09-23)

Prepared scenes with at least 192 triangles and every clamped alpha exactly 1 use a nearest QBVH query followed by bounded all-hit enumeration through the outward-rounded `nearest + 2 * dedup_tol`. The same distance/triangle-ID sort and moving-anchor Face-over-Volume deduplication select the first group. Its chosen normal, color and depth feed the unchanged compositor and overlays. Smaller scenes, zero/partial/NaN alpha retain full all-hits. The triangle threshold avoids the measured extra-traversal regression on a 24-triangle scene; it is a conservative workload heuristic, not a guarantee for every spatial layout. Layered release comparisons and limitations are recorded in PLAN.Performance.md §57.
