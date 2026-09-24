# GPU Module (`src/gpu/`)

> **Feature-gated:** the entire `src/gpu/` module requires `cargo build --features gpu`. All functions and types documented below are unavailable in a default (CPU-only) build.

> **Anchor note:** headings in this file use the bare `Struct::method` form (e.g. `#### GpuS2Pipeline::new`). Depending on the Markdown renderer, the auto-generated anchor for such a heading may come out as `#gpus2pipeline-new` or similar (renderers slugify `::` inconsistently). If a cross-link from another doc does not resolve, search this page for the heading text rather than relying on the anchor punctuation.

This module implements wgpu compute pipelines plus offscreen STL rasterization, with shared adapter/device selection and CPU fallback at pipeline call sites.

## Index

| Item | Location | Summary |
|---|---|---|
| `GpuContext` | `src/gpu/context.rs:3` | Holds adapter name and buffer-size capabilities after successful GPU init. |
| `GpuContext::caps` | `src/gpu/context.rs:11` | Returns `BackendCaps` describing this GPU context. |
| `GpuInitError` | `src/gpu/context.rs:22` | Error type wrapping a GPU initialization failure message. |
| `GpuInitError` (`Display` impl) | `src/gpu/context.rs:22` | Formats the error message. |
| `try_init_gpu` | `src/gpu/context.rs:38` | Probes for a wgpu adapter/device and returns a `GpuContext`; used by `compute::policy::select_backend`. |
| `request_adapter_device` | `src/gpu/context.rs` | Shared filtered adapter/device request used by rendering. |
| `GpuRenderPipeline` | `src/gpu/render.rs` | Offscreen STL rasterization pipeline. |
| `GpuRenderPipeline::new` | `src/gpu/render.rs` | Compiles `render.wgsl` and creates render state. |
| `GpuRenderPipeline::render` | `src/gpu/render.rs` | Rasterizes and reads back top-row-first RGBA8. |
| `GpuS2Pipeline` | `src/gpu/s2.rs:10` | GPU pipeline state for Monte Carlo S2 two-point correlation. |
| `build_triangle_buffer` (s2.rs) | `src/gpu/s2.rs:29` | Builds a normalized `f32` triangle position buffer for the S2 Monte Carlo pipeline. |
| `pack_params` | `src/gpu/s2.rs:50` | Packs Monte Carlo S2 shader parameters into a byte buffer matching the WGSL `Params` layout. |
| `dispatch_plan` | `src/gpu/s2.rs:101` | Validate logical MC ids, partial buffers and two-dimensional dispatch. |
| `check_buffer_size` | `src/gpu/s2.rs:115` | Check single-buffer and storage limits. |
| `check_mesh_capacity` | `src/gpu/s2.rs:125` | Check triangle count and upload capacity. |
| `scoped` | `src/gpu/runtime.rs:2` | Capture scoped GPU errors and balance all scopes. |
| `read_u32` | `src/gpu/runtime.rs:29` | Check mapping completion before copying and unmapping u32 readback. |
| `GpuS2Pipeline::new` | `src/gpu/s2.rs:141` | Initializes the wgpu device and Monte Carlo S2 compute pipeline. |
| `GpuS2Pipeline::update_mesh` | `src/gpu/s2.rs:277` | Re-uploads triangle data for a new mesh without recreating the pipeline. |
| `GpuS2Pipeline::ensure_output_capacity` | `src/gpu/s2.rs:302` | Grows the output buffers if the invocation count exceeds current capacity. |
| `GpuS2Pipeline::calculate_s2_gpu` | `src/gpu/s2.rs:351` | Dispatches the Monte Carlo S2 kernel for all radii and reads back results. |
| `OffsetEntry` | `src/gpu/s2_shell.rs:6` | Packed `(radius_idx, dx, dy, dz)` shell-offset record matching the WGSL layout. |
| `point_inside` (s2_monte_carlo.wgsl) | `src/gpu/shaders/s2_monte_carlo.wgsl:85` | Classify ray parity with overflow recovery. |
| `point_inside_overflow` (s2_monte_carlo.wgsl) | `src/gpu/shaders/s2_monte_carlo.wgsl:62` | Classify ray parity with overflow recovery. |
| `point_inside` (voxelize.wgsl) | `src/gpu/shaders/voxelize.wgsl:63` | Classify ray parity with overflow recovery. |
| `point_inside_overflow` (voxelize.wgsl) | `src/gpu/shaders/voxelize.wgsl:40` | Classify ray parity with overflow recovery. |
| `GpuShellS2Pipeline` | `src/gpu/s2_shell.rs:13` | GPU pipeline state for exact shell-pair S2 computation. |
| `build_offset_buffer` | `src/gpu/s2_shell.rs:30` | Converts `(radius_idx, [dx,dy,dz])` tuples into `OffsetEntry` records. |
| `GpuShellS2Pipeline::new` | `src/gpu/s2_shell.rs:48` | Initializes the wgpu device and shell S2 compute pipeline. |
| `GpuShellS2Pipeline::compute_s2_shell` | `src/gpu/s2_shell.rs:181` | Dispatches exact shell-pair counting over an occupancy grid and reads back S2(r). |
| `GpuVoxelPipeline` | `src/gpu/voxel.rs:5` | GPU pipeline state for mesh voxelization. |
| `build_triangle_buffer` (voxel.rs) | `src/gpu/voxel.rs:17` | Builds a normalized `f32` triangle position buffer for the voxelization pipeline (separate copy from `s2.rs`). |
| `pack_params` (voxel.rs) | `src/gpu/voxel.rs:32` | Serializes the 48-byte voxel parameter layout with ray direction at byte 32. |
| `GpuVoxelPipeline::new` | `src/gpu/voxel.rs:57` | Initializes the wgpu device and voxelization compute pipeline. |
| `GpuVoxelPipeline::voxelize` | `src/gpu/voxel.rs:168` | Dispatches ray-casting voxelization and reads back the occupancy grid. |
| `GpuVolumeTransformPipeline` | `src/gpu/volume_transform.rs:5` | GPU pipeline state for volume rotate-and-crop. |
| `GpuVolumeTransformPipeline::new` | `src/gpu/volume_transform.rs:23` | Initializes the wgpu device and volume-transform compute pipeline. |
| `GpuVolumeTransformPipeline::rotate_and_crop` | `src/gpu/volume_transform.rs:123` | Dispatches the rotate/crop/resample kernel and reads back the transformed volume. |
| `GridPlan` | `src/gpu/runtime.rs:60` | Checked two-dimensional grid dispatch. |
| `grid_plan` | `src/gpu/runtime.rs:67` | Validate product, buffer and dispatch limits. |
| `GpuVoxelPipeline::voxelize_limited` | `src/gpu/voxel.rs:179` | Fallible voxel execution with bounded dispatch. |

---

## `context.rs`

### GpuContext

```rust
pub struct GpuContext {
    adapter_name: String,
    max_buffer_size: u64,
    max_storage_buffer_binding_size: u64,
}
```

- **Source:** `src/gpu/context.rs:3`
- **Purpose:** Opaque handle returned by `try_init_gpu()` describing the GPU adapter that was successfully probed, along with the buffer-size limits reported by that adapter. It does not retain the `wgpu::Device`/`wgpu::Queue` themselves — those are dropped at the end of `try_init_gpu`; this struct exists purely as a capability descriptor for backend selection.

| Field | Type | Meaning |
|---|---|---|
| `adapter_name` | `String` | Human-readable adapter name (e.g. `"NVIDIA GeForce RTX 4090"`), as reported by `wgpu::AdapterInfo`. |
| `max_buffer_size` | `u64` | Maximum size in bytes of any single wgpu buffer on this adapter. |
| `max_storage_buffer_binding_size` | `u64` | Maximum size in bytes of a single storage-buffer binding on this adapter. |

#### GpuContext::caps

- **Signature:** `pub fn caps(&self) -> BackendCaps`
- **Source:** `src/gpu/context.rs:11`
- **Purpose:** Convert the internal `GpuContext` fields into the backend-agnostic `BackendCaps` struct (`crate::compute::backend::BackendCaps`) used by the compute-backend selection policy.
- **Parameters:** `&self`.
- **Returns:** `BackendCaps { name, supports_gpu: true, max_buffer_size, max_storage_buffer_binding_size }`.
- **Side effects:** None; pure data conversion.
- **Notes:** `supports_gpu` is unconditionally `true` here because `GpuContext` only exists after a successful `try_init_gpu()` call.

### GpuInitError

```rust
#[derive(Debug)]
pub struct GpuInitError(String);
```

- **Source:** `src/gpu/context.rs:22`
- **Purpose:** Newtype wrapping a human-readable error message describing why GPU initialization failed (no adapter found, adapter-filter mismatch, device request failure, etc.). Implements `std::error::Error`.

**`Display` impl** (`src/gpu/context.rs:22`): writes the wrapped message verbatim, i.e. `write!(f, "{}", self.0)`.

#### try_init_gpu

- **Signature:** `pub fn try_init_gpu() -> Result<GpuContext, GpuInitError>`
- **Source:** `src/gpu/context.rs:38`
- **Purpose:** General-purpose GPU adapter/device probe. This is the function `compute::policy::select_backend` calls to determine whether a GPU backend is available before dispatching work to it.
- **Parameters:** None. Behavior is controlled entirely via the `RUSTMSPT_GPU_DEVICE` environment variable (see below).
- **Returns:** `Ok(GpuContext)` with adapter name and buffer-size limits on success; `Err(GpuInitError)` if no suitable adapter/device could be obtained.
- **Side effects:** Creates a `wgpu::Instance` across all backends (`wgpu::Backends::all()`), enumerates and/or requests an adapter, and requests a logical device with empty features and default limits, all via `pollster::block_on` (i.e. synchronously blocking the calling thread). This is a heavyweight call — it should not be invoked per-frame or in a hot loop; callers are expected to cache the result.
- **`RUSTMSPT_GPU_DEVICE` env var filter:** documented in `AGENTS.md` as `RUSTMSPT_GPU_DEVICE=<name or index>` — filter adapter by name substring or index. Concretely:
  - If unset, the default `wgpu::PowerPreference::default()` adapter is requested normally (no fallback-adapter forcing).
  - If set and parseable as a `usize`, it is treated as an **index** into `instance.enumerate_adapters(wgpu::Backends::all())`; out-of-range indices produce a `GpuInitError` reporting the requested index and the number of adapters found.
  - If set and not a valid integer, it is treated as a **case-sensitive substring** matched against each enumerated adapter's `AdapterInfo.name`; the first match is used. No match produces a `GpuInitError`.
- **Notes:** The device/queue obtained here (`_device`, `_queue`) are intentionally unused after adapter/device limits are read — this function is a *capability probe*, not a pipeline constructor. Each of the four pipeline constructors below (`GpuS2Pipeline::new`, `GpuShellS2Pipeline::new`, `GpuVoxelPipeline::new`, `GpuVolumeTransformPipeline::new`) performs its own independent adapter/device request and does **not** reuse the context returned by `try_init_gpu`; all four constructors honor `RUSTMSPT_GPU_DEVICE`; voxel, shell and volume-transform use `request_adapter_device`.

---

## `s2.rs` — `GpuS2Pipeline` (Monte Carlo S2)

**Shader:** `src/gpu/shaders/s2_monte_carlo.wgsl` — each shader invocation processes one `(radius, sample)` pair: it casts a ray from a random point through the mesh (using triangle intersection counting for inside/outside classification), checks the corresponding partner point at that radius, and writes per-invocation hit/valid counters that are summed on the CPU side.

**See also:** [geometry-analysis.md#calculate_s2_with_gpu](geometry-analysis.md#calculate_s2_with_gpu), [../algorithms/s2-two-point-correlation.md](../algorithms/s2-two-point-correlation.md)

### GpuS2Pipeline

```rust
pub struct GpuS2Pipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    triangle_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_hits_buffer: wgpu::Buffer,
    out_valids_buffer: wgpu::Buffer,
    num_triangles: u32,
    bind_group_layout: wgpu::BindGroupLayout,
}
```

- **Source:** `src/gpu/s2.rs:10`
- **Purpose:** Holds the wgpu device, queue, compiled compute pipeline, and pre-allocated GPU buffers needed to dispatch the Monte Carlo S2 kernel repeatedly (e.g. across measurement iterations) without re-initializing wgpu or recompiling the shader each time.

| Field | Type | Meaning |
|---|---|---|
| `device` | `wgpu::Device` | Logical GPU device used to create buffers, pipelines, and command encoders. |
| `queue` | `wgpu::Queue` | Command submission queue for buffer writes and dispatches. |
| `pipeline` | `wgpu::ComputePipeline` | Compiled `s2_monte_carlo.wgsl` compute pipeline. |
| `triangle_buffer` | `wgpu::Buffer` | Storage buffer of normalized triangle vertex positions (`f32`, 9 floats/triangle), binding 0. |
| `params_buffer` | `wgpu::Buffer` | Storage buffer holding the packed `Params` struct (radii, seed, bbox, sample count), binding 1. |
| `out_hits_buffer` | `wgpu::Buffer` | Storage buffer of per-radius/workgroup hit partials (`u32`), binding 2. |
| `out_valids_buffer` | `wgpu::Buffer` | Storage buffer of per-radius/workgroup valid partials (`u32`), binding 3. |
| `num_triangles` | `u32` | Current triangle count, used when packing params. |
| `bind_group_layout` | `wgpu::BindGroupLayout` | Layout describing the four storage-buffer bindings above. |

Module-level constants: `WORKGROUP_SIZE: u32 = 256`, `MAX_RADII: usize = 128` (the shader's `Params.radii` array is fixed-size at 128 entries; the execution entry rejects r_max >= 128 before packing).

#### build_triangle_buffer (s2.rs)

- **Signature:** `fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32>`
- **Source:** `src/gpu/s2.rs:29`
- **Purpose:** Flatten a mesh's triangle vertex positions into a `f32` buffer suitable for direct upload as a wgpu storage buffer, with coordinates shifted so the bounding-box minimum sits at the origin.
- **Parameters:**
  - `mesh: &Mesh` — source mesh (vertices + faces).
  - `bbox: BoundingBox` — bounding box used to compute the coordinate origin shift (`bbox.min`).
- **Returns:** `Vec<f32>` with 9 floats per triangle (3 vertices × 3 components), each vertex coordinate stored as `(coord - bbox.min.<axis>) as f32`.
- **Side effects:** None (pure function).
- **Notes:** Subtracting the bbox origin in **f64 before converting to f32** keeps GPU-side `f32` coordinates small (roughly `0..size` rather than potentially large absolute values), improving floating-point precision on the GPU. This is a private, module-local helper — `voxel.rs` defines an independent, textually near-identical copy (see below); the two are not shared to avoid a cross-module dependency between otherwise self-contained pipelines.

#### pack_params

- **Signature:** `fn pack_params(num_triangles: u32, radii: &[f32], seed: u32, bbox: BoundingBox, samples_per_radius: u32) -> Vec<u8>`
- **Source:** `src/gpu/s2.rs:50`
- **Purpose:** Serialize the Monte Carlo shader's `Params` struct fields into a raw byte buffer matching the WGSL struct's `std430`-style layout and alignment (each `vec3<f32>` padded to 16 bytes).
- **Parameters:**
  - `num_triangles: u32` — triangle count for the currently uploaded mesh.
  - `radii: &[f32]` — radii to evaluate S2 at; the caller validates at most `MAX_RADII` (128) entries; remaining shader-side slots are zero-filled.
  - `seed: u32` — RNG seed consumed by the shader's `pcg_hash`-based sampler.
  - `bbox: BoundingBox` — used only for `bbox.size()`; the packed `bbox_min` is hardcoded to `(0,0,0)` because triangle data is already normalized to that origin by `build_triangle_buffer`.
  - `samples_per_radius: u32` — number of Monte Carlo samples per radius.
- **Returns:** `Vec<u8>` of length `16 + 16 + 16 + 16 + MAX_RADII * 4` bytes: header (`num_triangles`, `num_radii`, `samples_per_radius`, `seed`), `bbox_min` (always zero, padded vec4), `bbox_max`/size (padded vec4), `ray_dir` (padded vec4, taken from `geometry::s2::RAY_DIR_GPU`), then the fixed-size `radii` array.
- **Side effects:** None (pure function).
- **Notes:** The ray direction is imported from `super::super::geometry::s2::RAY_DIR_GPU`, i.e. `crate::geometry::s2::RAY_DIR_GPU`, so CPU and GPU ray-casting use an identical fixed ray direction for inside/outside classification.

#### GpuS2Pipeline::new

> **Feature-gated:** requires the `gpu` cargo feature; not present in a default build.

- **Signature:** `pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String>`
- **Source:** `src/gpu/s2.rs:141`
- **Purpose:** Initialize a wgpu device/queue, compile the `s2_monte_carlo.wgsl` shader into a compute pipeline, and pre-upload the mesh's normalized triangle data.
- **Parameters:**
  - `mesh: &Mesh` — mesh whose triangles are uploaded immediately.
  - `bbox: BoundingBox` — bounding box for coordinate normalization (see `build_triangle_buffer`).
- **Returns:** `Ok(GpuS2Pipeline)` on success; `Err(String)` describing the failure (no adapter, adapter-filter mismatch, or device request failure).
- **Side effects:** Performs a full wgpu adapter/device request (blocking, via `pollster::block_on`), compiles the shader module, creates the bind group layout/pipeline layout/compute pipeline, and allocates + uploads the triangle, params, and output buffers. Output and readback buffers start at 4 bytes each and grow to the actual invocation count.
- **Notes:** Uses the shared `request_adapter_device` selector; device creation is still per constructor, with unchanged empty features/default limits.

#### GpuS2Pipeline::update_mesh

- **Signature:** `pub fn update_mesh(&mut self, mesh: &Mesh, bbox: BoundingBox) -> Result<(), String>`
- **Source:** `src/gpu/s2.rs:277`
- **Purpose:** Replace the pipeline's uploaded triangle data with a new mesh, without tearing down and recreating the device/pipeline — used when the same `GpuS2Pipeline` is reused across multiple particles/meshes in a batch.
- **Parameters:** `mesh: &Mesh`, `bbox: BoundingBox` — same semantics as `new`.
- **Returns:** `Ok(())` or a triangle-capacity/upload error.
- **Side effects:** Re-uploads normalized triangle data via `queue.write_buffer`. If the new mesh's byte size exceeds the current `triangle_buffer`'s capacity, a new, larger buffer is allocated and `triangle_buffer` is replaced; otherwise the existing buffer is reused in place. Updates `self.num_triangles`.
- **Notes:** Because the buffer only grows (never shrinks) on reallocation, repeatedly calling this with meshes of varying size is safe but can retain peak-size GPU memory for the pipeline's lifetime.

#### GpuS2Pipeline::ensure_output_capacity

- **Signature:** `fn ensure_output_capacity(&mut self, partials: u32)`
- **Source:** `src/gpu/s2.rs:302`
- **Purpose:** Private helper that grows `out_hits_buffer`/`out_valids_buffer` if a requested dispatch would need more workgroup partial slots than currently allocated.
- **Parameters:** `partials: u32` — total number of radius/workgroup outputs about to be dispatched (`num_radii * ceil(samples_per_radius / 256)`).
- **Returns:** `()`.
- **Side effects:** May reallocate both output buffers (4 bytes/invocation each) if `invocations * 4 > out_hits_buffer.size()`. Old buffer contents are discarded (not preserved) since the new buffer is written fully by the next dispatch.
- **Notes:** Called internally by `calculate_s2_gpu` before every dispatch; not part of the public API.

#### GpuS2Pipeline::calculate_s2_gpu

- **Signature:** `pub fn calculate_s2_gpu(&mut self, bbox: BoundingBox, r_max: usize, samples: usize) -> Result<Vec<f64>, String>`
- **Source:** `src/gpu/s2.rs:351`
- **Purpose:** Compute the Monte Carlo two-point correlation function S2(r) on the GPU for every integer radius from 0 to `r_max`, in a single dispatch covering all radii.
- **Parameters:**
  - `bbox: BoundingBox` — bounding box used to pack shader params (size only; origin is already normalized).
  - `r_max: usize` — largest radius (inclusive) to evaluate; radii are the integers `0..=r_max`.
  - `samples: usize` — requested Monte Carlo samples per radius; clamped to a minimum of 200 (`samples.max(200)`).
- **Returns:** `Ok(Vec<f64>)` on success, or `Err(String)` for capacity, scoped execution or mapping failure. The vector has length `r_max + 1`, indexed by radius `r`, containing `hits/valid` averaged over that radius's samples (0.0 if no valid samples were recorded for that radius).
- **Side effects:** Generates a random `u32` seed (`rand::random`), packs and uploads params, calls `ensure_output_capacity`, builds a bind group, records and submits a compute pass (`dispatch_workgroups` with `WORKGROUP_SIZE = 256`), then copies both output buffers to staging buffers, maps them for CPU read (`device.poll(wgpu::Maintain::Wait)` — a blocking wait), and reduces the mapped `u32` slices into the returned `f64` vector.
- **Notes:** Element `r=0` is the sampled hit/valid ratio here. Pipeline callers overwrite it with geometric volume fraction where their S2 contract requires that value. Positions are computed in `f32` on the GPU but the returned averages are stored/returned as `f64`.

### MC execution errors and capacity checks

`GpuS2Pipeline::new`, `update_mesh`, and `calculate_s2_gpu` use balanced wgpu validation, out-of-memory, and internal error scopes. The latter two now return `Result` rather than unconditional success/data. A map callback must succeed before any mapped memory is accessed. Rust panics are not caught.

- `dispatch_plan(r_max, samples, limits) -> Result<(u32,u32,[u32;2]), String>` checks `r_max < 128`, sample conversion and invocation multiplication, workgroup count, storage binding size and buffer size before output allocation or radius-vector construction. The minimum remains 200 samples.
- `check_buffer_size(bytes, limits) -> Result<(), String>` checks both storage and single-buffer limits.
- `check_mesh_capacity(mesh, limits) -> Result<(), String>` checks triangle count and byte arithmetic before flattening; empty geometry uses a four-byte placeholder.
- `runtime::scoped<T>(device, work) -> Result<T,String>` collects all three scope classes and pops every scope even when the operation returns an error. Callers must serialize operations on the device; this is not a panic-catching boundary.
- `runtime::read_u32(device, buffer) -> Result<Vec<u32>,String>` checks callback/channel errors, copies mapped u32 data, and unmaps on success.

MC bbox extents must be positive and finite after f32 conversion. These checks cover device limits, not a configured total-memory budget; peak working-set planning and high-water buffer retention remain separate work. Only MC currently uses this runtime helper; voxel/shell/crop/render APIs are unchanged. The legacy `calculate_s2_with_gpu` wrapper logs execution failures and retries continuous CPU mesh MC, preserving the method the GPU attempted. It has no strict-fallback parameter. Optimize implements its own explicit fallback policy.

### Ray containment and overflow recovery (WGSL)

Both `s2_monte_carlo.wgsl` and `voxelize.wgsl` implement the following private helpers:

- `point_inside(point: vec3<f32>) -> bool`: retains a sorted 64-hit fast path. On the **65th positive triangle hit**, it calls `point_inside_overflow` for the original ray. The capacity counts raw triangle hits, including duplicates.
- `point_inside_overflow(point: vec3<f32>, dir: vec3<f32>) -> bool`: repeatedly scans every triangle to find the smallest positive hit more than `1e-6` beyond the last retained hit (the first scan accepts any positive hit). Counts these retained distances and returns odd parity. Uses constant auxiliary storage, no writes or CPU readback, and at most one scan per triangle; normally terminates when no further distinct hit exists.

This matches the GPU fast path's sorted, **anchored** deduplication: compare with the last retained distance, not the previous raw distance. CPU's `1e-8` tolerance and f64 predicates remain different. No sample is discarded or redrawn. The MC bbox prefilter remains in place. Recovery costs O(T × U), for T triangles and U distinct forward hits, up to O(T²); dense overlapping scenes may be much slower. This is a correctness recovery, not a general GPU speedup or a solution to device/map failures.

Sources: MC helpers at `src/gpu/shaders/s2_monte_carlo.wgsl:62` and `:85`; voxel helpers at `src/gpu/shaders/voxelize.wgsl:40` and `:63`.

---

## `s2_shell.rs` — `GpuShellS2Pipeline` (exact shell-pair S2)

**Shader:** `src/gpu/shaders/s2_shell_pairs.wgsl` — each invocation handles one `(radius, offset)` pair from a precomputed shell-offset list, walking the voxel occupancy grid to count valid and hit pairs at that exact integer displacement.

**See also:** [geometry-analysis.md#calculate_s2_gpu_exact](geometry-analysis.md#calculate_s2_gpu_exact)

### OffsetEntry

```rust
#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct OffsetEntry {
    radius_idx: u32,
    dx: i32,
    dy: i32,
    dz: i32,
}
```

- **Source:** `src/gpu/s2_shell.rs:6`
- **Purpose:** GPU-side (`bytemuck::Pod`) representation of one shell offset: which radius bucket it belongs to (`radius_idx`) and the integer voxel displacement `(dx, dy, dz)` that offset represents. `#[repr(C)]` + `Pod`/`Zeroable` make it directly castable to/from raw bytes for buffer upload, mirroring the WGSL `OffsetEntry` struct exactly (16 bytes, four 4-byte fields).

### GpuShellS2Pipeline

```rust
pub struct GpuShellS2Pipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    occupancy_buffer: wgpu::Buffer,
    offsets_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_valid_buffer: wgpu::Buffer,
    out_hits_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
}
```

- **Source:** `src/gpu/s2_shell.rs:13`
- **Purpose:** Holds device/queue/pipeline state and pre-allocated buffers for exact shell-pair S2 computation directly over a voxel occupancy grid (as opposed to `GpuS2Pipeline`'s stochastic ray-cast approach).

| Field | Type | Meaning |
|---|---|---|
| `device` | `wgpu::Device` | Logical GPU device. |
| `queue` | `wgpu::Queue` | Command queue. |
| `pipeline` | `wgpu::ComputePipeline` | Compiled `s2_shell_pairs.wgsl` pipeline. |
| `occupancy_buffer` | `wgpu::Buffer` | Storage buffer of the voxel occupancy grid (`u32`, one per voxel), binding 0. |
| `offsets_buffer` | `wgpu::Buffer` | Storage buffer of `OffsetEntry` records, binding 1, sized for up to `MAX_OFFSETS` entries. |
| `params_buffer` | `wgpu::Buffer` | Storage buffer holding `total_offsets`/`nx`/`ny`/`nz`, binding 2. |
| `out_valid_buffer` | `wgpu::Buffer` | Storage buffer of per-offset valid-pair counts (`u32`), binding 3. |
| `out_hits_buffer` | `wgpu::Buffer` | Storage buffer of per-offset hit-pair counts (`u32`), binding 4. |
| `bind_group_layout` | `wgpu::BindGroupLayout` | Layout for the five bindings above. |

Module-level constants: `WORKGROUP_SIZE: u32 = 256`, `MAX_OFFSETS: usize = 200_000` (per-chunk capacity for the offsets/output buffers; all chunks contribute to the same per-radius reduction).

#### build_offset_buffer

- **Signature:** `fn build_offset_buffer(shell_offsets: &[(u32, [isize; 3])]) -> Vec<OffsetEntry>`
- **Source:** `src/gpu/s2_shell.rs:30`
- **Purpose:** Convert CPU-side `(radius_idx, [dx, dy, dz])` tuples (as produced by the geometry shell-offset enumeration) into GPU-uploadable `OffsetEntry` records.
- **Parameters:** `shell_offsets: &[(u32, [isize; 3])]` — radius index paired with an `isize` displacement triple.
- **Returns:** `Vec<OffsetEntry>`, one entry per input tuple, with `dx`/`dy`/`dz` narrowed from `isize` to `i32`.
- **Side effects:** None (pure function).
- **Notes:** Conversion to `i32` is checked. Unrepresentable displacements use an `i32::MAX` sentinel, outside grids supported by the storage-buffer limits. The shader rejects any displacement magnitude at least its axis dimension before unsigned subtraction, including `i32::MIN`.

#### GpuShellS2Pipeline::new

> **Feature-gated:** requires the `gpu` cargo feature; not present in a default build.

- **Signature:** `pub fn new() -> Result<Self, String>`
- **Source:** `src/gpu/s2_shell.rs:48`
- **Purpose:** Initialize wgpu and compile the `s2_shell_pairs.wgsl` compute pipeline.
- **Parameters:** None.
- **Returns:** `Ok(GpuShellS2Pipeline)` or `Err(String)` describing an adapter/device failure.
- **Side effects:** Performs a blocking wgpu adapter/device request (honoring `RUSTMSPT_GPU_DEVICE` through the common selector), compiles the shader, builds the bind group/pipeline layout, and pre-allocates the occupancy (4 bytes — grows on first use), offsets (16 bytes), params (16 bytes), output and retained staging (4 bytes each) buffers; batch buffers grow together on demand.
- **Notes:** Unlike `GpuS2Pipeline::new` and `GpuVoxelPipeline::new`, this constructor takes no mesh/bbox — occupancy grid data is supplied per-call to `compute_s2_shell` instead of at construction time.

#### GpuShellS2Pipeline::compute_s2_shell

- **Signature:** `pub fn compute_s2_shell(&mut self, occ: &[u32], nx: u32, ny: u32, nz: u32, shell_offsets: &[(u32, [isize; 3])], r_max: usize, _voxel_pitch: f64, vf: f64) -> Result<Vec<f64>, String>`
- **Source:** `src/gpu/s2_shell.rs:181`
- **Purpose:** Compute the exact (non-stochastic) S2(r) correlation function by counting, for every precomputed shell offset at every radius, how many voxel pairs at that exact displacement are both occupied ("hits") versus both in-bounds/valid, over the entire occupancy grid.
- **Parameters:**
  - `occ: &[u32]` — flattened voxel occupancy grid (row-major, `nx*ny*nz` entries, 1 = occupied).
  - `nx, ny, nz: u32` — grid dimensions.
  - `shell_offsets: &[(u32, [isize; 3])]` — precomputed `(radius_idx, displacement)` pairs (typically from a CPU-side shell enumeration); processed in chunks of at most `MAX_OFFSETS`, without truncation.
  - `r_max: usize` — largest radius index present in `shell_offsets`; determines the returned vector's length.
  - `_voxel_pitch: f64` — accepted but unused (prefixed with `_`); voxel pitch conversion happens on the caller side.
  - `vf: f64` — known volume fraction, written directly into `out[0]` as the r=0 value.
- **Returns:** `Vec<f64>` of length `r_max + 1`: `out[0] = vf`; for `r >= 1`, the mean of `hits/valid` over all offsets belonging to that radius bucket (0.0 if no offsets/valid pairs contributed to that radius).
- **Side effects:** Re-uploads the occupancy buffer (reallocating it if larger than current capacity), uploads the offset buffer and params, reallocates the two output buffers if `total_offsets` exceeds prior capacity, builds a bind group, dispatches `total_offsets.div_ceil(WORKGROUP_SIZE)` workgroups, copies outputs to staging buffers, and performs a blocking `device.poll(wgpu::Maintain::Wait)` to map them for CPU readback.
- **Notes:** Empty offset lists return `[vf, 0, ...]` without dispatch or readback. Occupancy is uploaded once; each offset chunk is uploaded, dispatched, and read back before reusing buffers. The per-radius reduction (summing `hits/valid` ratios per offset and averaging by count, `shell_sums`/`shell_counts`) happens on the CPU across all chunks after readback, not on the GPU — the shader itself only produces raw per-offset hit/valid counts.

---

## `voxel.rs` — `GpuVoxelPipeline`

**Shader:** `src/gpu/shaders/voxelize.wgsl` — each invocation tests containment of one voxel center against the mesh via ray-casting (coordinates normalized to the bbox origin), writing a 0/1 occupancy flag per voxel.

This pipeline is the GPU counterpart of `geometry::s2::build_bbox_occupancy` — see [geometry-analysis.md#build_bbox_occupancy](geometry-analysis.md#build_bbox_occupancy).

### GpuVoxelPipeline

```rust
pub struct GpuVoxelPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    triangle_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    occupancy_buffer: wgpu::Buffer,
    num_triangles: u32,
    bind_group_layout: wgpu::BindGroupLayout,
}
```

- **Source:** `src/gpu/voxel.rs:5`
- **Purpose:** Holds device/queue/pipeline state and buffers for GPU-side mesh voxelization (occupancy-grid rasterization via ray-casting).

| Field | Type | Meaning |
|---|---|---|
| `device` | `wgpu::Device` | Logical GPU device. |
| `queue` | `wgpu::Queue` | Command queue. |
| `pipeline` | `wgpu::ComputePipeline` | Compiled `voxelize.wgsl` pipeline. |
| `triangle_buffer` | `wgpu::Buffer` | Storage buffer of normalized triangle positions, binding 0. |
| `params_buffer` | `wgpu::Buffer` | Storage buffer of grid/ray params, binding 1. |
| `occupancy_buffer` | `wgpu::Buffer` | Storage buffer of output occupancy flags (`u32`), binding 2. |
| `num_triangles` | `u32` | Current triangle count. |
| `bind_group_layout` | `wgpu::BindGroupLayout` | Layout for the three bindings above. |

Module-level constant: `WORKGROUP_SIZE: u32 = 64`.

#### build_triangle_buffer (voxel.rs)

- **Signature:** `fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32>`
- **Source:** `src/gpu/voxel.rs:17`
- **Purpose:** Same purpose and logic as `s2.rs`'s `build_triangle_buffer` (documented above): flattens triangle vertex positions into an `f32` buffer normalized to `bbox.min`.
- **Parameters/Returns/Side effects:** Identical to the `s2.rs` version.
- **Notes:** This is a **separate, independent copy** of the function, not a shared/reused implementation — `voxel.rs` and `s2.rs` each define their own private `build_triangle_buffer`. Keep this in mind when modifying triangle-normalization logic: a fix in one file does not propagate to the other.

#### pack_params (voxel.rs)

- **Signature:** `fn pack_params(num_triangles: u32, nx: u32, ny: u32, nz: u32, pitch: f32) -> Vec<u8>`
- **Purpose:** Serialize the 48-byte WGSL storage-buffer layout: triangle count at byte 0, dimensions at 4/8/12, pitch at 16, zero padding at 20/24/28, ray direction at 32/36/40, and final padding at 44.
- **Returns / side effects:** Parameter bytes; no side effects. The direction is `RAY_DIR_GPU`, with all three components preserved at WGSL's 16-byte-aligned `vec3` offset.

#### GpuVoxelPipeline::new

> **Feature-gated:** requires the `gpu` cargo feature; not present in a default build.

- **Signature:** `pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String>`
- **Source:** `src/gpu/voxel.rs:57`
- **Purpose:** Initialize wgpu, compile the `voxelize.wgsl` compute pipeline, and upload normalized triangle data for the given mesh.
- **Parameters:** `mesh: &Mesh`, `bbox: BoundingBox` — mesh to voxelize and its bounding box (for normalization).
- **Returns:** `Ok(GpuVoxelPipeline)` or `Err(String)` on adapter/device failure.
- **Side effects:** Blocking wgpu adapter/device request (honoring `RUSTMSPT_GPU_DEVICE`), shader compilation, bind group/pipeline layout creation, triangle buffer allocation + upload, params buffer allocation (48 bytes), and a minimal 4-byte placeholder occupancy buffer (grown on first `voxelize` call).
- **Notes:** Like `GpuShellS2Pipeline::new`, this constructor does not honor `RUSTMSPT_GPU_DEVICE`.

#### GpuVoxelPipeline::voxelize

- **Signature:** `pub fn voxelize(&mut self, nx: u32, ny: u32, nz: u32, pitch: f32) -> Result<Vec<u32>, String>`
- **Source:** `src/gpu/voxel.rs:168`
- **Purpose:** Rasterize the mesh (uploaded at construction time or via a prior call) into a 3D occupancy grid of the given dimensions and voxel pitch, using GPU ray-casting.
- **Parameters:**
  - `nx, ny, nz: u32` — grid dimensions (voxel counts along each axis).
  - `pitch: f32` — voxel edge length in mesh coordinate units (uniform across axes).
- **Returns:** `Vec<u32>` of length `nx*ny*nz`, row-major, 1 = voxel center inside mesh, 0 = outside.
- **Side effects:** Packs and uploads params (including the fixed ray direction `crate::geometry::s2::RAY_DIR_GPU`, shared with the CPU-side and Monte-Carlo-S2 ray direction), grows the occupancy buffer if needed, builds a bind group, dispatches `total.div_ceil(WORKGROUP_SIZE)` (`WORKGROUP_SIZE = 64`) workgroups, copies the result to a staging buffer, and performs a blocking map/read (`device.poll(wgpu::Maintain::Wait)`).
- **Notes:** Assumes the mesh's triangle data currently in `triangle_buffer` (from `new` or a prior `voxelize` call using the same mesh) corresponds to the same normalized coordinate space implied by `nx/ny/nz/pitch`; there is no `update_mesh` method on this pipeline (unlike `GpuS2Pipeline`), so voxelizing a different mesh requires constructing a new `GpuVoxelPipeline`.

---

## `volume_transform.rs` — `GpuVolumeTransformPipeline`

**Shader:** `src/gpu/shaders/volume_transform.wgsl` — each invocation computes one output voxel by applying the inverse rotation to map it back to source-volume coordinates (relative to a centroid) and sampling the source volume with nearest-neighbor or trilinear interpolation.

This pipeline is used by the crop pipeline's PCA-alignment step.

**See also:** [pipeline-crop-and-splitfilter.md](pipeline-crop-and-splitfilter.md), [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md)

### GpuVolumeTransformPipeline

```rust
pub struct GpuVolumeTransformPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    src_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    current_src_size: u64,
    current_out_size: u64,
}
```

- **Source:** `src/gpu/volume_transform.rs:5`
- **Purpose:** Holds device/queue/pipeline state and growable buffers for GPU-accelerated volume rotate-and-crop (e.g. rotating a segmented volume into a PCA-aligned frame and cropping/resampling it to a target bounding region).

| Field | Type | Meaning |
|---|---|---|
| `device` | `wgpu::Device` | Logical GPU device. |
| `queue` | `wgpu::Queue` | Command queue. |
| `pipeline` | `wgpu::ComputePipeline` | Compiled `volume_transform.wgsl` pipeline. |
| `src_buffer` | `wgpu::Buffer` | Storage buffer of the source volume (`i32` labels/values), binding 0. |
| `params_buffer` | `wgpu::Buffer` | Storage buffer of transform params (dims, interpolation mode, background, rotation matrix, centroid, origin), binding 1, 128 bytes. |
| `out_buffer` | `wgpu::Buffer` | Storage buffer of the output (rotated + cropped) volume, binding 2. |
| `bind_group_layout` | `wgpu::BindGroupLayout` | Layout for the three bindings above. |
| `current_src_size` | `u64` | Tracks the currently allocated size of `src_buffer`, to avoid reallocating on every call when the source volume size doesn't grow. |
| `current_out_size` | `u64` | Tracks the currently allocated size of `out_buffer`, same purpose. |

Module-level constant: `WORKGROUP_SIZE: u32 = 64` (applied along the output X dimension only; Y and Z are dispatched one workgroup per row/slice — see `rotate_and_crop`).

#### GpuVolumeTransformPipeline::new

> **Feature-gated:** requires the `gpu` cargo feature; not present in a default build.

- **Signature:** `pub fn new() -> Result<Self, String>`
- **Source:** `src/gpu/volume_transform.rs:23`
- **Purpose:** Initialize wgpu and compile the `volume_transform.wgsl` compute pipeline.
- **Parameters:** None.
- **Returns:** `Ok(GpuVolumeTransformPipeline)` or `Err(String)` on adapter/device failure.
- **Side effects:** Blocking wgpu adapter/device request (honoring `RUSTMSPT_GPU_DEVICE`), shader compilation, bind group/pipeline layout creation, and allocation of minimal placeholder `src_buffer`/`out_buffer` (4 bytes each, grown on first `rotate_and_crop` call) plus a fixed 128-byte `params_buffer`.
- **Notes:** Like the voxel and shell-S2 constructors, no mesh/volume data is required at construction time — all per-call data is supplied to `rotate_and_crop`.

#### GpuVolumeTransformPipeline::rotate_and_crop

- **Signature:**
  ```rust
  pub fn rotate_and_crop(
      &mut self,
      src_data: &[i32],
      src_w: u32, src_h: u32, src_d: u32,
      background: i32,
      rot: &Matrix3<f64>,
      centroid: &Vector3<f64>,
      origin: &Vector3<f64>,
      out_w: u32, out_h: u32, out_d: u32,
      interp_mode: u32,
  ) -> Result<Vec<i32>, String>
  ```
- **Source:** `src/gpu/volume_transform.rs:123`
- **Purpose:** Rotate a labeled/valued source volume about a centroid using the given rotation matrix, resample it into a new axis-aligned output volume of dimensions `out_w × out_h × out_d` starting at `origin`, and fill out-of-bounds samples with `background`.
- **Parameters:**
  - `src_data: &[i32]` — flattened source volume, row-major with `idx3d(x,y,z) = z*src_w*src_h + y*src_w + x`.
  - `src_w, src_h, src_d: u32` — source volume dimensions.
  - `background: i32` — fill value for output voxels whose inverse-rotated source coordinate falls outside the source volume.
  - `rot: &Matrix3<f64>` — rotation matrix (row-major values are extracted and cast to `f32` for the shader); the shader applies the corresponding inverse rotation to map output coordinates back to source space.
  - `centroid: &Vector3<f64>` — rotation pivot, in source-volume coordinates.
  - `origin: &Vector3<f64>` — coordinate of output voxel `(0,0,0)` in the rotated/centered frame (i.e. the crop's output offset).
  - `out_w, out_h, out_d: u32` — output volume dimensions.
  - `interp_mode: u32` — `0` = nearest-neighbor, `1` = trilinear interpolation (per the shader's `Params.interp_mode` comment).
- **Returns:** `Ok(Vec<i32>)` or a validation, device or readback error.
- **Side effects:** Reallocates `src_buffer`/`out_buffer` if the new data exceeds `current_src_size`/`current_out_size` (buffers only grow, never shrink, across calls on the same pipeline instance), uploads source data and packed params (rotation matrix as three padded `vec4<f32>` rows, centroid and origin as padded `vec4<f32>`), builds a bind group, dispatches `(out_w.div_ceil(64), out_h, out_d)` workgroups (one workgroup group per output row per output slice, `WORKGROUP_SIZE = 64` along X only), copies the result to a staging buffer, and performs a blocking map/read.
- **Notes:** Checked flattened two-dimensional dispatch uses 64 invocations per workgroup. Source values and transform parameters are validated before upload; readback errors propagate.

---

## Summary of shared conventions across all four pipelines

- **Instance reuse, independent devices:** selection uses a process-wide instance with serialized enumeration; selected GL adapters instead come from private instances to isolate EGL contexts. Each constructor still gets a fresh adapter/device/queue. Compiled-pipeline and logical-device sharing remain pending.
- **Blocking GPU readback:** every dispatch-and-read method (`calculate_s2_gpu`, `compute_s2_shell`, `voxelize`, `rotate_and_crop`) uses `map_async` followed by `device.poll(wgpu::Maintain::Wait)`, which blocks the calling thread until the GPU work and buffer mapping complete. None of these pipelines expose an async or non-blocking API.
- **Growable, never-shrinking buffers:** buffer-reuse fields (`triangle_buffer`, `out_hits_buffer`, `occupancy_buffer`, `src_buffer`/`out_buffer`, etc.) are only reallocated when a new call's data exceeds current capacity; they are never downsized, so a pipeline instance's peak GPU memory footprint is the maximum footprint across all calls made against it in its lifetime.
- **`f32` on GPU, `f64` on CPU:** all four pipelines narrow `f64`/`isize` CPU-side geometry to `f32`/`i32` for GPU upload and widen results back to `f64`/`i32` on readback, matching each WGSL shader's use of 32-bit types throughout.

## `render.rs` — `GpuRenderPipeline`

`request_adapter_device(label)` performs a blocking adapter/device request honoring `RUSTMSPT_GPU_DEVICE`; it returns `(Device, Queue)` or an error string.

`GpuRenderPipeline::new() -> Result<Self, String>` compiles `render.wgsl`, creates a uniform bind-group layout, and builds a two-sided triangle-list pipeline targeting `Rgba8Unorm` with `Depth32Float` depth.

`GpuRenderPipeline::render(&mut self, mesh, camera, width, height, settings) -> Result<RenderedImage, String>` expands each face to three position/flat-normal vertices, uploads the shared view-projection and appearance values, renders offscreen, copies color to a mapped staging buffer, strips 256-byte row padding, and returns RGBA8. Empty geometry returns a background image. Private helpers `build_render_vertices` and `to_wgsl_mat4` prepare flat vertices and column-major f32 matrices.

### Voxel/shell checked execution (PERF-04/05)

Both constructors and evaluation methods now use the balanced runtime error scopes and checked readback helpers. `voxelize` and `compute_s2_shell` return `Result`; zero/overflowing grid dimensions, storage/dispatch limits, and mismatched occupancy length return errors before dispatch. Voxel pitch must be finite and positive. `runtime::grid_plan([nx,ny,nz], width, limits)` checks storage and returns total cells, bytes and a two-dimensional dispatch with overflow-safe padded indexing. The shaders flatten x/y invocation coordinates. `voxelize_limited` additionally caps both workgroup dimensions for small actual-shader tests. The exact geometry API `try_calculate_s2_gpu_exact` normalizes nonpositive pitch to 1.0, rejects nonfinite input, and propagates initialization/execution errors. The legacy Vec wrapper still logs and retries CPU exact.


### Volume transform validation (2026-09-18)

The constructor honors `RUSTMSPT_GPU_DEVICE` through the common adapter selector. `rotate_and_crop` returns `Result`, validates dimension products, buffer/device limits, source length, finite transform parameters and interpolation mode before dispatch. Trilinear input integers must be exactly representable in f32. Checked two-dimensional dispatch and scoped GPU/readback errors replace unchecked execution. Nearest sampling preserves i32 bit patterns; ties round away from zero. Device and staging reuse across separate pipeline instances is still pending.


### Render error contract

`GpuRenderPipeline::new` and `render` use balanced validation, allocation and internal error scopes. `render` rejects zero/oversize textures, vertex-count overflow, and oversize vertex/staging buffers before allocation. Readback checks the mapping callback before accessing mapped memory. An initialized device returning a render error is a test failure, not an unavailable-adapter skip.


GPU selection regression: direct voxel and shell constructors are tested in child processes with nonexistent adapter names and out-of-range indices. Both must report the requested selector; default-device substitution is forbidden. Common selection does not yet imply shared device or compiled-pipeline caching.


`context::request_adapter` is the single adapter-selection implementation for capability probing and all GPU pipeline constructors. `request_adapter_device` requests a fresh logical device using that adapter. MC, voxel, shell, transforms and renderers therefore share name/index/default semantics. This refactor does not cache devices.


### Backend instance lifetime (PERF-03)

`shared_instance` retains one instance in OnceLock. Adapter selection and disposal of unselected adapters are serialized because wgpu 24's EGL enumeration can otherwise race its context access. Selected GL adapters are recreated from private instances, keeping device operations isolated. Every adapter is used for only one device request, as required by wgpu's API contract. No failed selector/device result is memoized. This avoids repeated backend-instance initialization for non-GL paths; it does not yet share devices, queues or compiled pipelines. Initial unsynchronized shared-instance tests exposed EGL BadAccess and are retained alongside the corrected regression logs.


### MC output and staging capacity (PERF-03)

MC initializes two output and two readback buffers at four bytes each, replacing the former fixed 39.1 MiB output reservation. `ensure_output_capacity` grows all four only when necessary. `read_u32_prefix` maps and copies only the live invocation prefix, checking nonzero aligned size and capacity first; successful reads unmap before reuse. Smaller/repeated calls preserve buffer identity. `release_output_capacity() -> Result<(), String>` resets those four buffers to four bytes while retaining geometry, parameters and the compiled pipeline. Capacities otherwise retain their high-water mark. `measure` estimates a fresh MC call as `16 * invocations + triangle_bytes + 576`; optimizer's explicit-budget support remains a separate open item.


### Voxel and volume-transform staging reuse

Voxel occupancy and transform output buffers now retain matching readback buffers, initially four bytes each. Growth occurs only when a call exceeds capacity; checked prefix mapping returns exactly the current grid, even after a larger call. `GpuVoxelPipeline::release_grid_capacity` resets occupancy/readback storage; `GpuVolumeTransformPipeline::release_output_capacity` resets output/readback storage. Both preserve compiled pipelines and input geometry/source capacity. These release methods return allocation errors and permit subsequent evaluation. Per-call values, dimensions and pitch remain uploaded normally, so retained capacity does not imply retained results. Full device-budget/high-water policy and resident voxel-to-shell chaining remain separate work.

### Shell batch capacity

`GpuShellS2Pipeline` retains offset/output/staging buffers across bounded batches and evaluations. It reads only the live prefix, including a shorter final batch. `release_batch_capacity()` returns `Result<(), String>` and resets offsets to 16 bytes and each output/staging buffer to 4 bytes, preserving occupancy and compiled state. `resize_batch_buffers(count)` is private and runs inside the caller’s GPU error scope. Capacity reuse does not change equal per-offset weighting or the 200,000-offset bound.

| `GpuShellS2Pipeline::resize_batch_buffers` | `src/gpu/s2_shell.rs:193` | Shell batch buffer capacity management; occupancy retained. |

| `GpuShellS2Pipeline::release_batch_capacity` | `src/gpu/s2_shell.rs:230` | Shell batch buffer capacity management; occupancy retained. |

`runtime::read_u32` is a test-only whole-buffer mapping wrapper; production callers use `read_u32_prefix` for their live byte count.

### Scene preview working-set policy

`mesh_render.gpu_memory_limit_mb` optionally limits the planned logical GPU working set. `gpu_min_pixels` applies only to auto mode and defaults to zero for legacy compatibility. Below-threshold auto avoids GPU initialization; explicit GPU ignores that threshold but obeys the budget. Auto falls back on budget/execution failure, while explicit GPU fails. CPU mode ignores GPU-only resource options.

The checked planner counts visible triangles at 120 bytes each, enabled segments at 64 bytes and markers at 192 bytes. It includes one color/depth pair (8 bytes/pixel), one readback buffer (256-byte-aligned RGBA rows), and 128 uniform bytes. Pending queue uploads coexist with destination geometry/uniform buffers, giving the conservative logical peak `2 * geometry_bytes + 256 + 8 * pixels + staging_bytes`. Multiple views reuse targets. Driver/pipeline internals and host scene/PNG memory are not included, so this is not a physical VRAM/RSS cap. Device limits are checked independently before host vertex expansion; expanded host arrays are released immediately after upload. The constructor also returns scoped GPU validation/allocation errors. Image tiling remains future work.

| `SceneRenderMemory::plan` | `src/compute/render_memory.rs:20` | Checked scene preview workset, budget and buffer planning without allocation. |

| `SceneRenderMemory::check_budget` | `src/compute/render_memory.rs:77` | Checked scene preview workset, budget and buffer planning without allocation. |

| `SceneRenderMemory::check_buffers` | `src/compute/render_memory.rs:93` | Checked scene preview workset, budget and buffer planning without allocation. |

### Optimize MC memory budget

The optimizer starts its shared GPU MC pipeline with empty geometry; each stage uploads the mesh it actually evaluates. `mc_evaluation_peak` bounds the triangle buffer, four output/readback buffers, 576-byte parameter storage, pending queue uploads and the next mesh/parameter uploads. Growth conservatively counts old plus new allocations. Startup uses the input face count and maximum configured stage sample count; every actual evaluation rechecks current retained capacities under the same GPU mutex before upload. A larger reference mesh or retained high-water capacity can therefore trigger the existing same-method fallback (or a stage-labelled strict error). Pending upload bytes reset only after successful readback. Driver internals and CPU mesh/readback vectors are outside this logical GPU budget. Batch splitting/automatic high-water trimming remain separate work; `release_output_capacity` provides explicit release.

| `mc_evaluation_peak` | `src/compute/mc_memory.rs:4` | Check logical MC peak including retained capacity and pending uploads. |

| `check_mc_budget` | `src/compute/mc_memory.rs:44` | Check logical MC peak including retained capacity and pending uploads. |

| `GpuS2Pipeline::check_evaluation_budget` | `src/gpu/s2.rs:275` | Check logical MC peak including retained capacity and pending uploads. |

Measure continuous MC now uses the shared cold-growth peak planner, including queued geometry/parameter uploads; this supersedes the earlier `16 * invocations + triangle_bytes + 576` estimate. Exact-method budgeting is unchanged in this batch.

### GPU MC workgroup integer reduction

The production MC shader assigns each 256-lane workgroup to one `(radius, sample block)`. Active lanes use the original logical id `radius * samples_per_radius + sample` for the RNG; padded lanes contribute zero. A uniform workgroup reduction emits one hit/valid pair per block. CPU merges these partials in u64 and applies the same ratio, reading `8 * (r_max+1) * ceil(max(samples,200)/256)` bytes instead of per-sample flags. Radius padding cannot mix counts or redraw samples. `dispatch_plan` checks logical-id overflow, the padded workgroup count and partial-buffer sizes independently. `mc_evaluation_peak` uses these partial capacities and still accounts for retained buffers/uploads/growth.

`calculate_s2_gpu_counts` is a private fixed-seed path returning integer totals; the public API still draws one fresh random seed and returns the curve. A frozen pre-reduction shader at `tests/fixtures/s2_monte_carlo_samples.wgsl` is used only in tests to compare exact hit/valid totals under identical seeds, including tail blocks, 128 radii, geometry updates and invalid offsets. BVH traversal, per-radius GPU final reduction and budget-dependent sample batching remain separate work. This change establishes reduction/count semantics, not CPU/GPU f64 equivalence or a hardware speedup claim.

| `GpuS2Pipeline::new_with_shader` | `src/gpu/s2.rs:154` | GPU MC partial-count execution and fixed-seed reference validation. |

| `GpuS2Pipeline::calculate_s2_gpu_counts` | `src/gpu/s2.rs:420` | GPU MC partial-count execution and fixed-seed reference validation. |

MC workgroups now span two dispatch dimensions, with block index `group.x + group.y * num_workgroups.x`. A uniform guard rejects padded groups before any barrier or write, including when retained output capacity exceeds the live result length. The planner returns samples, partial count and `[x,y]` dispatch; logical sample IDs remain bounded by u32. Optimize startup no longer applies the obsolete one-dimensional invocation ceiling. Forced 3×3/4×2 execution matches the frozen per-sample shader at identical seeds; a spare-capacity sentinel checks that padding never writes beyond live partials. The large 65,536-group case is planner-only coverage, not a large GPU execution benchmark.

GPU shell valid-pair counts are computed analytically as `(nx-|dx|)*(ny-|dy|)*(nz-|dz|)` after rejecting displacements outside any axis. Host grid validation bounds the full product by u32, so every overlap product is safe. The shader still enumerates occupancy pairs for hits and CPU still averages complete per-offset ratios with equal weight; offset tiling and device-resident occupancy remain pending. A raw-count oracle independently enumerates signed coordinates for all small offsets, thin grids, empty/full/mixed occupancy and i32 extreme displacements. This arithmetic change alone does not establish a measured speedup.

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::new_with_shader` | `src/gpu/s2_shell.rs:55` | Private constructor taking shader source; returns initialized resources or a GPU error. Production uses the analytic shader; tests can use the frozen enumerated-count fixture. |

The experimental `s2_shell_cooperative.wgsl` assigns one 256-lane workgroup per offset. Lanes stride through the overlap volume, then reduce integer hits in shared memory; valid counts remain analytic. Two-dimensional group flattening rejects padding uniformly before barriers. It retains one output pair per offset, so this is not yet multiple independently scheduled voxel tiles or resident voxel-to-shell dataflow. Production still selects the direct shader pending the workload benchmark. `new_with_shader(source, offsets_per_workgroup)` pairs shader indexing with the dispatch planner: direct uses 256 offsets per group and cooperative uses 1.

The experimental tiled shader uses `(offset, voxel tile)` workgroups with a selectable positive tile width. Each tile reduces integer hit counts and emits its analytic overlap length; empty tiles emit zero. Host code sums all tile counts for one offset in u64 before forming that offset ratio. Batches contain at most 200,000 partial slots, reducing offsets per batch by the tile count; workloads with more than 200,000 tiles per offset currently return an explicit capacity error. This fixed cap is not a complete user-budget planner. Parameter storage is 24 bytes (offset count, dimensions, tiles per offset, tile width); older direct shaders read the first 16 bytes. Production continues to use direct evaluation while the tiled path is validated and benchmarked.

The experimental tiled path can also enable a second device pass (`s2_shell_reduce.wgsl`) that sums tile hit/valid counts into one pair per offset. These sums fit u32 because every offset has at most the checked full-grid cell count. CPU still performs the original per-offset ratio averaging. The reducer and final buffers are initialized lazily and reused; explicit batch release shrinks final buffers while retaining the compiled reducer. Readback is 8 bytes per offset regardless of tile count, but tile buffers and an additional dispatch remain. This is an experimental option, not a production selection or proof of speedup.

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::ensure_reduction` | `src/gpu/s2_shell.rs:211` | Lazily compile the device tile reducer and grow its final buffers under the caller error scope. |

Production shell evaluation now filters offsets whose unsigned displacement magnitude reaches any grid dimension before GPU upload. Filtering preserves input order and uses a reusable bounded host batch, not a second full offset list. An unsupported-only request returns the same VF/zero curve without uploading occupancy or allocating result buffers. Test reference constructors can disable filtering so raw invalid-offset shader behavior remains covered. Per-offset ratios and their equal weighting are unchanged; this filter does not remove empty tiles inside otherwise supported offsets.

| Function | Source | Contract |
|---|---|---|
| `offset_has_overlap` | `src/gpu/s2_shell.rs:57` | Check all unsigned displacement magnitudes against grid dimensions without signed overflow. |

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::with_device` | `src/gpu/s2_shell.rs:84` | Build production shell resources on supplied device/queue; no new device. |
| `GpuShellS2Pipeline::build_on_device` | `src/gpu/s2_shell.rs:96` | Compile shell resources on supplied handles with balanced GPU error scopes. |
| `GpuShellS2Pipeline::compute_s2_shell_resident` | `src/gpu/s2_shell.rs:385` | Read a same-device occupancy buffer directly; caller serializes producer and consumer. |
| `GpuShellS2Pipeline::compute_shell_input` | `src/gpu/s2_shell.rs:410` | Shared execution for host-uploaded or resident occupancy with identical offset semantics. |
| `GpuVoxelPipeline::device_queue` | `src/gpu/voxel.rs:59` | Clone device/queue handles for sequential stages; no device creation. |
| `GpuVoxelPipeline::occupancy_buffer` | `src/gpu/voxel.rs:54` | Clone completed occupancy storage handle; producer must not overwrite while consumed. |

GPU exact now constructs the shell stage on the voxel stage’s Device/Queue and binds its occupancy buffer directly. Shell construction does not select an adapter or request another device, and the shell does not allocate/upload a duplicate occupancy field. Both stages run sequentially with separate error scopes. Voxel occupancy counting now runs on the device and only a four-byte count is read for VF; upstream backend capability probes remain separate. Standalone host-occupancy shell calls retain their upload behavior, and later host calls cannot overwrite the producer’s borrowed buffer.

`voxelize_count` dispatches voxelization followed by a lazily compiled integer occupancy reducer, leaves the full field on device and reads one u32 count. The reducer uses a single 256-lane workgroup with bounded strided reads; binary occupancy and the checked grid size bound every sum by u32. Its total work remains O(grid cells), so reduced transfer is not a guarantee of lower latency. Occupancy and staging grow independently: count-only execution needs four staging bytes, while subsequent full-readback calls grow staging as required. Switching modes and releasing capacity preserves correctness. GPU exact uses this count for VF and passes the resident field to shell; standalone `voxelize` retains its Vec-returning contract.

| Function | Source | Contract |
|---|---|---|
| `GpuVoxelPipeline::voxelize_count` | `src/gpu/voxel.rs:199` | Voxelize and return only the occupied-cell count; retain the device field. |
| `GpuVoxelPipeline::ensure_counter` | `src/gpu/voxel.rs:218` | Lazily construct the integer counter and four-byte output under caller error scope. |
| `GpuVoxelPipeline::voxelize_impl` | `src/gpu/voxel.rs:267` | Checked common voxel execution with full-grid or count-only readback. |

GPU exact now lazily generates one shell-radius Vec at a time and passes its offsets through `compute_s2_shell_resident_stream`. Host batches retain at most the existing partial-slot allowance; they can span radius boundaries while preserving offset order. The support flags used for interpolation are captured when each shell is generated, eliminating the former second enumeration. All offsets, including unsupported tails, are consumed on successful evaluation. Memory is bounded by one radius shell plus a batch, not by a constant independent of radius; an individual large-radius shell is still materialized. Count diagnostics use u128 so aggregate generated counts are not silently saturated.

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::compute_s2_shell_resident_stream` | `src/gpu/s2_shell.rs:410` | Consume ordered offsets lazily in bounded batches on a resident grid; preserve per-offset ratios. |

GPU exact now uses `shell_offset_iter`, retaining only nested range cursors even within one radius. It preserves the original x/y/z order, origin special case and half-open squared-distance test; the public Vec API remains unchanged for random-access consumers. Support is detected with a peekable iterator and every generated offset is counted as consumed. Safe ordinary integer norms match the Vec implementation; larger norms use u128 to avoid signed multiplication overflow. Enumeration still scans the enclosing cube, so this reduces allocation without changing its O(radius³) search complexity.

Fresh resident GPU exact evaluations use `ExactMemoryPlan` for both backend selection and execution. With triangle storage T=max(36*faces,4), occupancy M=4*cells, and B partial slots, the conservative logical peak is 2T+M+128+80B. This includes pending triangle/offset uploads and simultaneous old/new batch buffers; the 128-byte allowance covers fixed parameter/count/placeholder resources. B is reduced from 200,000 to fit an optional MiB budget, with a minimum of one. If even that does not fit, execution rejects before GPU initialization and the caller applies its fallback policy. The model excludes driver/compiler internals and CPU memory, applies to a fresh production direct-shell evaluation, and does not claim to budget experimental tiled/reduced or arbitrary retained pipelines. Existing hard exact-grid limits remain independent.
