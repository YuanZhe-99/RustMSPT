# GPU Module (`src/gpu/`)

> **Feature-gated:** the entire `src/gpu/` module requires `cargo build --features gpu`. All functions and types documented below are unavailable in a default (CPU-only) build.

> **Anchor note:** headings in this file use the bare `Struct::method` form (e.g. `#### GpuS2Pipeline::new`). Depending on the Markdown renderer, the auto-generated anchor for such a heading may come out as `#gpus2pipeline-new` or similar (renderers slugify `::` inconsistently). If a cross-link from another doc does not resolve, search this page for the heading text rather than relying on the anchor punctuation.

This module implements GPU-accelerated compute pipelines built on [`wgpu`](https://wgpu.rs/) and WGSL compute shaders. It provides four independent pipelines — Monte Carlo S2 correlation, exact shell-pair S2 correlation, mesh voxelization, and volume rotate-and-crop — plus a shared adapter/device bootstrap (`context.rs`) used by the compute backend selection policy.

## Index

| Item | Location | Summary |
|---|---|---|
| `GpuContext` | `src/gpu/context.rs:3` | Holds adapter name and buffer-size capabilities after successful GPU init. |
| `GpuContext::caps` | `src/gpu/context.rs:11` | Returns `BackendCaps` describing this GPU context. |
| `GpuInitError` | `src/gpu/context.rs:22` | Error type wrapping a GPU initialization failure message. |
| `GpuInitError` (`Display` impl) | `src/gpu/context.rs:24` | Formats the error message. |
| `try_init_gpu` | `src/gpu/context.rs:38` | Probes for a wgpu adapter/device and returns a `GpuContext`; used by `compute::policy::select_backend`. |
| `GpuS2Pipeline` | `src/gpu/s2.rs:10` | GPU pipeline state for Monte Carlo S2 two-point correlation. |
| `build_triangle_buffer` (s2.rs) | `src/gpu/s2.rs:27` | Builds a normalized `f32` triangle position buffer for the S2 Monte Carlo pipeline. |
| `pack_params` | `src/gpu/s2.rs:48` | Packs Monte Carlo S2 shader parameters into a byte buffer matching the WGSL `Params` layout. |
| `GpuS2Pipeline::new` | `src/gpu/s2.rs:95` | Initializes the wgpu device and Monte Carlo S2 compute pipeline. |
| `GpuS2Pipeline::update_mesh` | `src/gpu/s2.rs:266` | Re-uploads triangle data for a new mesh without recreating the pipeline. |
| `GpuS2Pipeline::ensure_output_capacity` | `src/gpu/s2.rs:287` | Grows the output buffers if the invocation count exceeds current capacity. |
| `GpuS2Pipeline::calculate_s2_gpu` | `src/gpu/s2.rs:311` | Dispatches the Monte Carlo S2 kernel for all radii and reads back results. |
| `OffsetEntry` | `src/gpu/s2_shell.rs:6` | Packed `(radius_idx, dx, dy, dz)` shell-offset record matching the WGSL layout. |
| `GpuShellS2Pipeline` | `src/gpu/s2_shell.rs:13` | GPU pipeline state for exact shell-pair S2 computation. |
| `build_offset_buffer` | `src/gpu/s2_shell.rs:30` | Converts `(radius_idx, [dx,dy,dz])` tuples into `OffsetEntry` records. |
| `GpuShellS2Pipeline::new` | `src/gpu/s2_shell.rs:48` | Initializes the wgpu device and shell S2 compute pipeline. |
| `GpuShellS2Pipeline::compute_s2_shell` | `src/gpu/s2_shell.rs:135` | Dispatches exact shell-pair counting over an occupancy grid and reads back S2(r). |
| `GpuVoxelPipeline` | `src/gpu/voxel.rs:5` | GPU pipeline state for mesh voxelization. |
| `build_triangle_buffer` (voxel.rs) | `src/gpu/voxel.rs:17` | Builds a normalized `f32` triangle position buffer for the voxelization pipeline (separate copy from `s2.rs`). |
| `GpuVoxelPipeline::new` | `src/gpu/voxel.rs:37` | Initializes the wgpu device and voxelization compute pipeline. |
| `GpuVoxelPipeline::voxelize` | `src/gpu/voxel.rs:116` | Dispatches ray-casting voxelization and reads back the occupancy grid. |
| `GpuVolumeTransformPipeline` | `src/gpu/volume_transform.rs:5` | GPU pipeline state for volume rotate-and-crop. |
| `GpuVolumeTransformPipeline::new` | `src/gpu/volume_transform.rs:23` | Initializes the wgpu device and volume-transform compute pipeline. |
| `GpuVolumeTransformPipeline::rotate_and_crop` | `src/gpu/volume_transform.rs:145` | Dispatches the rotate/crop/resample kernel and reads back the transformed volume. |

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

**`Display` impl** (`src/gpu/context.rs:24`): writes the wrapped message verbatim, i.e. `write!(f, "{}", self.0)`.

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
- **Notes:** The device/queue obtained here (`_device`, `_queue`) are intentionally unused after adapter/device limits are read — this function is a *capability probe*, not a pipeline constructor. Each of the four pipeline constructors below (`GpuS2Pipeline::new`, `GpuShellS2Pipeline::new`, `GpuVoxelPipeline::new`, `GpuVolumeTransformPipeline::new`) performs its own independent adapter/device request and does **not** reuse the context returned by `try_init_gpu`; only `GpuS2Pipeline::new` currently honors `RUSTMSPT_GPU_DEVICE` (see notes below) — the other three pipeline constructors always use `wgpu::PowerPreference::default()` with no adapter filtering.

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
| `out_hits_buffer` | `wgpu::Buffer` | Storage buffer of per-invocation hit counts (`u32`), binding 2. |
| `out_valids_buffer` | `wgpu::Buffer` | Storage buffer of per-invocation valid-sample counts (`u32`), binding 3. |
| `num_triangles` | `u32` | Current triangle count, used when packing params. |
| `bind_group_layout` | `wgpu::BindGroupLayout` | Layout describing the four storage-buffer bindings above. |

Module-level constants: `WORKGROUP_SIZE: u32 = 256`, `MAX_RADII: usize = 128` (the shader's `Params.radii` array is fixed-size at 128 entries; requesting more than 128 radii in one call will silently truncate — see `pack_params`).

#### build_triangle_buffer (s2.rs)

- **Signature:** `fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32>`
- **Source:** `src/gpu/s2.rs:27`
- **Purpose:** Flatten a mesh's triangle vertex positions into a `f32` buffer suitable for direct upload as a wgpu storage buffer, with coordinates shifted so the bounding-box minimum sits at the origin.
- **Parameters:**
  - `mesh: &Mesh` — source mesh (vertices + faces).
  - `bbox: BoundingBox` — bounding box used to compute the coordinate origin shift (`bbox.min`).
- **Returns:** `Vec<f32>` with 9 floats per triangle (3 vertices × 3 components), each vertex coordinate stored as `(coord as f32) - bbox.min.<axis> as f32`.
- **Side effects:** None (pure function).
- **Notes:** Normalizing to the bbox origin keeps GPU-side `f32` coordinates small (roughly `0..size` rather than potentially large absolute values), improving floating-point precision on the GPU. This is a private, module-local helper — `voxel.rs` defines an independent, textually near-identical copy (see below); the two are not shared to avoid a cross-module dependency between otherwise self-contained pipelines.

#### pack_params

- **Signature:** `fn pack_params(num_triangles: u32, radii: &[f32], seed: u32, bbox: BoundingBox, samples_per_radius: u32) -> Vec<u8>`
- **Source:** `src/gpu/s2.rs:48`
- **Purpose:** Serialize the Monte Carlo shader's `Params` struct fields into a raw byte buffer matching the WGSL struct's `std430`-style layout and alignment (each `vec3<f32>` padded to 16 bytes).
- **Parameters:**
  - `num_triangles: u32` — triangle count for the currently uploaded mesh.
  - `radii: &[f32]` — radii to evaluate S2 at; only the first `MAX_RADII` (128) entries are used, remaining shader-side slots are zero-filled.
  - `seed: u32` — RNG seed consumed by the shader's `pcg_hash`-based sampler.
  - `bbox: BoundingBox` — used only for `bbox.size()`; the packed `bbox_min` is hardcoded to `(0,0,0)` because triangle data is already normalized to that origin by `build_triangle_buffer`.
  - `samples_per_radius: u32` — number of Monte Carlo samples per radius.
- **Returns:** `Vec<u8>` of length `16 + 16 + 16 + 16 + MAX_RADII * 4` bytes: header (`num_triangles`, `num_radii`, `samples_per_radius`, `seed`), `bbox_min` (always zero, padded vec4), `bbox_max`/size (padded vec4), `ray_dir` (padded vec4, taken from `geometry::s2::RAY_DIR_GPU`), then the fixed-size `radii` array.
- **Side effects:** None (pure function).
- **Notes:** The ray direction is imported from `super::super::geometry::s2::RAY_DIR_GPU`, i.e. `crate::geometry::s2::RAY_DIR_GPU`, so CPU and GPU ray-casting use an identical fixed ray direction for inside/outside classification.

#### GpuS2Pipeline::new

> **Feature-gated:** requires the `gpu` cargo feature; not present in a default build.

- **Signature:** `pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String>`
- **Source:** `src/gpu/s2.rs:95`
- **Purpose:** Initialize a wgpu device/queue, compile the `s2_monte_carlo.wgsl` shader into a compute pipeline, and pre-upload the mesh's normalized triangle data.
- **Parameters:**
  - `mesh: &Mesh` — mesh whose triangles are uploaded immediately.
  - `bbox: BoundingBox` — bounding box for coordinate normalization (see `build_triangle_buffer`).
- **Returns:** `Ok(GpuS2Pipeline)` on success; `Err(String)` describing the failure (no adapter, adapter-filter mismatch, or device request failure).
- **Side effects:** Performs a full wgpu adapter/device request (blocking, via `pollster::block_on`), compiles the shader module, creates the bind group layout/pipeline layout/compute pipeline, and allocates + uploads the triangle, params, and output buffers. Output buffers are pre-sized for `MAX_RADII * 40_000` invocations (`max_invocations`), so this constructor performs one relatively large GPU allocation up front.
- **Notes:** This constructor duplicates the adapter-selection logic of `try_init_gpu` (including `RUSTMSPT_GPU_DEVICE` index/substring filtering) rather than calling it — the two are independent adapter probes and it is possible, in principle, for `try_init_gpu`'s probe adapter and this constructor's adapter to differ if the environment changes between calls (in practice `RUSTMSPT_GPU_DEVICE` makes selection deterministic within a process).

#### GpuS2Pipeline::update_mesh

- **Signature:** `pub fn update_mesh(&mut self, mesh: &Mesh, bbox: BoundingBox)`
- **Source:** `src/gpu/s2.rs:266`
- **Purpose:** Replace the pipeline's uploaded triangle data with a new mesh, without tearing down and recreating the device/pipeline — used when the same `GpuS2Pipeline` is reused across multiple particles/meshes in a batch.
- **Parameters:** `mesh: &Mesh`, `bbox: BoundingBox` — same semantics as `new`.
- **Returns:** `()`.
- **Side effects:** Re-uploads normalized triangle data via `queue.write_buffer`. If the new mesh's byte size exceeds the current `triangle_buffer`'s capacity, a new, larger buffer is allocated and `triangle_buffer` is replaced; otherwise the existing buffer is reused in place. Updates `self.num_triangles`.
- **Notes:** Because the buffer only grows (never shrinks) on reallocation, repeatedly calling this with meshes of varying size is safe but can retain peak-size GPU memory for the pipeline's lifetime.

#### GpuS2Pipeline::ensure_output_capacity

- **Signature:** `fn ensure_output_capacity(&mut self, invocations: u32)`
- **Source:** `src/gpu/s2.rs:287`
- **Purpose:** Private helper that grows `out_hits_buffer`/`out_valids_buffer` if a requested dispatch would need more invocation slots than currently allocated.
- **Parameters:** `invocations: u32` — total number of shader invocations about to be dispatched (`num_radii * samples_per_radius`).
- **Returns:** `()`.
- **Side effects:** May reallocate both output buffers (4 bytes/invocation each) if `invocations * 4 > out_hits_buffer.size()`. Old buffer contents are discarded (not preserved) since the new buffer is written fully by the next dispatch.
- **Notes:** Called internally by `calculate_s2_gpu` before every dispatch; not part of the public API.

#### GpuS2Pipeline::calculate_s2_gpu

- **Signature:** `pub fn calculate_s2_gpu(&mut self, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64>`
- **Source:** `src/gpu/s2.rs:311`
- **Purpose:** Compute the Monte Carlo two-point correlation function S2(r) on the GPU for every integer radius from 0 to `r_max`, in a single dispatch covering all radii.
- **Parameters:**
  - `bbox: BoundingBox` — bounding box used to pack shader params (size only; origin is already normalized).
  - `r_max: usize` — largest radius (inclusive) to evaluate; radii are the integers `0..=r_max`.
  - `samples: usize` — requested Monte Carlo samples per radius; clamped to a minimum of 200 (`samples.max(200)`).
- **Returns:** `Vec<f64>` of length `r_max + 1`, indexed by radius `r`, containing `hits/valid` averaged over that radius's samples (0.0 if no valid samples were recorded for that radius).
- **Side effects:** Generates a random `u32` seed (`rand::random`), packs and uploads params, calls `ensure_output_capacity`, builds a bind group, records and submits a compute pass (`dispatch_workgroups` with `WORKGROUP_SIZE = 256`), then copies both output buffers to staging buffers, maps them for CPU read (`device.poll(wgpu::Maintain::Wait)` — a blocking wait), and reduces the mapped `u32` slices into the returned `f64` vector.
- **Notes:** Element `r=0` (radius zero) is always left as `0.0` by this function's internal accumulation logic — per the source comment, the caller is expected to overwrite index 0 with the known volume fraction rather than trust the GPU's degenerate zero-radius sample. Positions are computed in `f32` on the GPU but the returned averages are stored/returned as `f64`.

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

Module-level constants: `WORKGROUP_SIZE: u32 = 256`, `MAX_OFFSETS: usize = 200_000` (pre-allocated capacity for the offsets/output buffers; `compute_s2_shell` truncates to this cap via `.min(MAX_OFFSETS)`).

#### build_offset_buffer

- **Signature:** `fn build_offset_buffer(shell_offsets: &[(u32, [isize; 3])]) -> Vec<OffsetEntry>`
- **Source:** `src/gpu/s2_shell.rs:30`
- **Purpose:** Convert CPU-side `(radius_idx, [dx, dy, dz])` tuples (as produced by the geometry shell-offset enumeration) into GPU-uploadable `OffsetEntry` records.
- **Parameters:** `shell_offsets: &[(u32, [isize; 3])]` — radius index paired with an `isize` displacement triple.
- **Returns:** `Vec<OffsetEntry>`, one entry per input tuple, with `dx`/`dy`/`dz` narrowed from `isize` to `i32`.
- **Side effects:** None (pure function).
- **Notes:** No bounds/overflow check is performed when narrowing `isize` to `i32`; displacement values are expected to be small (bounded by the maximum voxel-grid radius), so this is safe in practice but not defensively guarded.

#### GpuShellS2Pipeline::new

> **Feature-gated:** requires the `gpu` cargo feature; not present in a default build.

- **Signature:** `pub fn new() -> Result<Self, String>`
- **Source:** `src/gpu/s2_shell.rs:48`
- **Purpose:** Initialize wgpu and compile the `s2_shell_pairs.wgsl` compute pipeline.
- **Parameters:** None.
- **Returns:** `Ok(GpuShellS2Pipeline)` or `Err(String)` describing an adapter/device failure.
- **Side effects:** Performs a blocking wgpu adapter/device request (no `RUSTMSPT_GPU_DEVICE` filtering — always uses `wgpu::PowerPreference::default()` with no fallback forcing, unlike `GpuS2Pipeline::new`), compiles the shader, builds the bind group/pipeline layout, and pre-allocates the occupancy (4 bytes — grows on first use), offsets (`MAX_OFFSETS * 16` bytes), params (16 bytes), and output (`MAX_OFFSETS * 4` bytes each) buffers.
- **Notes:** Unlike `GpuS2Pipeline::new` and `GpuVoxelPipeline::new`, this constructor takes no mesh/bbox — occupancy grid data is supplied per-call to `compute_s2_shell` instead of at construction time.

#### GpuShellS2Pipeline::compute_s2_shell

- **Signature:** `pub fn compute_s2_shell(&mut self, occ: &[u32], nx: u32, ny: u32, nz: u32, shell_offsets: &[(u32, [isize; 3])], r_max: usize, _voxel_pitch: f64, vf: f64) -> Vec<f64>`
- **Source:** `src/gpu/s2_shell.rs:135`
- **Purpose:** Compute the exact (non-stochastic) S2(r) correlation function by counting, for every precomputed shell offset at every radius, how many voxel pairs at that exact displacement are both occupied ("hits") versus both in-bounds/valid, over the entire occupancy grid.
- **Parameters:**
  - `occ: &[u32]` — flattened voxel occupancy grid (row-major, `nx*ny*nz` entries, 1 = occupied).
  - `nx, ny, nz: u32` — grid dimensions.
  - `shell_offsets: &[(u32, [isize; 3])]` — precomputed `(radius_idx, displacement)` pairs (typically from a CPU-side shell enumeration); truncated to `MAX_OFFSETS` if larger.
  - `r_max: usize` — largest radius index present in `shell_offsets`; determines the returned vector's length.
  - `_voxel_pitch: f64` — accepted but unused (prefixed with `_`); voxel pitch conversion happens on the caller side.
  - `vf: f64` — known volume fraction, written directly into `out[0]` as the r=0 value.
- **Returns:** `Vec<f64>` of length `r_max + 1`: `out[0] = vf`; for `r >= 1`, the mean of `hits/valid` over all offsets belonging to that radius bucket (0.0 if no offsets/valid pairs contributed to that radius).
- **Side effects:** Re-uploads the occupancy buffer (reallocating it if larger than current capacity), uploads the offset buffer and params, reallocates the two output buffers if `total_offsets` exceeds prior capacity, builds a bind group, dispatches `total_offsets.div_ceil(WORKGROUP_SIZE)` workgroups, copies outputs to staging buffers, and performs a blocking `device.poll(wgpu::Maintain::Wait)` to map them for CPU readback.
- **Notes:** The per-radius reduction (summing `hits/valid` ratios per offset and averaging by count, `shell_sums`/`shell_counts`) happens on the CPU after readback, not on the GPU — the shader itself only produces raw per-offset hit/valid counts.

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

#### GpuVoxelPipeline::new

> **Feature-gated:** requires the `gpu` cargo feature; not present in a default build.

- **Signature:** `pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String>`
- **Source:** `src/gpu/voxel.rs:37`
- **Purpose:** Initialize wgpu, compile the `voxelize.wgsl` compute pipeline, and upload normalized triangle data for the given mesh.
- **Parameters:** `mesh: &Mesh`, `bbox: BoundingBox` — mesh to voxelize and its bounding box (for normalization).
- **Returns:** `Ok(GpuVoxelPipeline)` or `Err(String)` on adapter/device failure.
- **Side effects:** Blocking wgpu adapter/device request (default power preference, no `RUSTMSPT_GPU_DEVICE` filtering), shader compilation, bind group/pipeline layout creation, triangle buffer allocation + upload, params buffer allocation (48 bytes), and a minimal 4-byte placeholder occupancy buffer (grown on first `voxelize` call).
- **Notes:** Like `GpuShellS2Pipeline::new`, this constructor does not honor `RUSTMSPT_GPU_DEVICE`.

#### GpuVoxelPipeline::voxelize

- **Signature:** `pub fn voxelize(&mut self, nx: u32, ny: u32, nz: u32, pitch: f32) -> Vec<u32>`
- **Source:** `src/gpu/voxel.rs:116`
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
- **Side effects:** Blocking wgpu adapter/device request (default power preference, no `RUSTMSPT_GPU_DEVICE` filtering), shader compilation, bind group/pipeline layout creation, and allocation of minimal placeholder `src_buffer`/`out_buffer` (4 bytes each, grown on first `rotate_and_crop` call) plus a fixed 128-byte `params_buffer`.
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
  ) -> Vec<i32>
  ```
- **Source:** `src/gpu/volume_transform.rs:145`
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
- **Returns:** `Vec<i32>` of length `out_w*out_h*out_d`, the resampled/cropped volume.
- **Side effects:** Reallocates `src_buffer`/`out_buffer` if the new data exceeds `current_src_size`/`current_out_size` (buffers only grow, never shrink, across calls on the same pipeline instance), uploads source data and packed params (rotation matrix as three padded `vec4<f32>` rows, centroid and origin as padded `vec4<f32>`), builds a bind group, dispatches `(out_w.div_ceil(64), out_h, out_d)` workgroups (one workgroup group per output row per output slice, `WORKGROUP_SIZE = 64` along X only), copies the result to a staging buffer, and performs a blocking map/read.
- **Notes:** All floating-point transform inputs (`rot`, `centroid`, `origin`) are `f64` on the Rust side but narrowed to `f32` when packed for the shader — this is consistent with the other pipelines' use of `f32` GPU-side precision. The dispatch shape (`wg_y = out_h`, `wg_z = out_d`, i.e. one full workgroup per row/slice rather than `div_ceil`-tiled) means large `out_h`/`out_d` values translate directly into large `y`/`z` workgroup counts — callers should be aware this is not tiled the way the X dimension is.

---

## Summary of shared conventions across all four pipelines

- **No shared `GpuContext` reuse:** each of `GpuS2Pipeline::new`, `GpuShellS2Pipeline::new`, `GpuVoxelPipeline::new`, and `GpuVolumeTransformPipeline::new` independently creates its own `wgpu::Instance`/adapter/device rather than accepting a pre-initialized `GpuContext` from `try_init_gpu`. Only `GpuS2Pipeline::new` currently reads `RUSTMSPT_GPU_DEVICE`.
- **Blocking GPU readback:** every dispatch-and-read method (`calculate_s2_gpu`, `compute_s2_shell`, `voxelize`, `rotate_and_crop`) uses `map_async` followed by `device.poll(wgpu::Maintain::Wait)`, which blocks the calling thread until the GPU work and buffer mapping complete. None of these pipelines expose an async or non-blocking API.
- **Growable, never-shrinking buffers:** buffer-reuse fields (`triangle_buffer`, `out_hits_buffer`, `occupancy_buffer`, `src_buffer`/`out_buffer`, etc.) are only reallocated when a new call's data exceeds current capacity; they are never downsized, so a pipeline instance's peak GPU memory footprint is the maximum footprint across all calls made against it in its lifetime.
- **`f32` on GPU, `f64` on CPU:** all four pipelines narrow `f64`/`isize` CPU-side geometry to `f32`/`i32` for GPU upload and widen results back to `f64`/`i32` on readback, matching each WGSL shader's use of 32-bit types throughout.
