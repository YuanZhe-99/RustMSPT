# GPU Module (`src/gpu/`)

> **待翻译：** `GpuRenderPipeline` 的详细契约见[英文 GPU 参考](../../en-us/reference/gpu.md#renderrs--gpurenderpipeline)。

> **特性门控：** 整个 `src/gpu/` 模块需要 `cargo build --features gpu`。以下文档中记录的所有函数和类型在默认（仅 CPU）构建中均不可用。

> **锚点说明：** 本文件中的标题使用裸 `Struct::method` 形式（例如 `#### GpuS2Pipeline::new`）。根据所用的 Markdown 渲染器不同，此类标题自动生成的锚点可能是 `#gpus2pipeline-new` 或类似形式（渲染器对 `::` 的转义规则并不一致）。如果来自其他文档的交叉链接无法解析，请在本页中搜索标题文本，而不要依赖锚点标点符号。

本模块实现了基于 [`wgpu`](https://wgpu.rs/) 与 WGSL 计算着色器构建的 GPU 加速计算流水线。它提供四条相互独立的流水线——蒙特卡洛 S2 相关函数、精确壳层对 S2 相关函数、网格体素化，以及体数据旋转裁剪——外加供计算后端选择策略使用的共享适配器/设备启动逻辑（`context.rs`）。

## 索引

| 条目 | 位置 | 摘要 |
|---|---|---|
| `GpuContext` | `src/gpu/context.rs:42` | 成功完成 GPU 初始化后，持有适配器名称与缓冲区大小能力信息。 |
| `GpuContext::caps` | `src/gpu/context.rs:62` | 返回描述此 GPU 上下文的 `BackendCaps`。 |
| `GpuInitError` | `src/gpu/context.rs:73` | 包装 GPU 初始化失败消息的错误类型。 |
| `GpuInitError`（`Display` 实现） | `src/gpu/context.rs:22` | 格式化错误消息。 |
| `try_init_gpu` | `src/gpu/context.rs:238` | 探测 wgpu 适配器/设备并返回 `GpuContext`；供 `compute::policy::select_backend` 使用。 |
| `GpuS2Pipeline` | `src/gpu/s2.rs:25` | 蒙特卡洛 S2 两点相关函数的 GPU 流水线状态。 |
| `build_triangle_buffer`（s2.rs） | `src/gpu/s2.rs:29` | 为 S2 蒙特卡洛流水线构建归一化的 `f32` 三角形位置缓冲区。 |
| `pack_params` | `src/gpu/s2.rs:94` | 将蒙特卡洛 S2 着色器参数打包为与 WGSL `Params` 布局匹配的字节缓冲区。 |
| `dispatch_plan` | `src/gpu/s2.rs:144` | Validate logical MC ids, partial buffers and two-dimensional dispatch. |
| `check_buffer_size` | `src/gpu/s2.rs:168` | Check single-buffer and storage limits. |
| `check_mesh_capacity` | `src/gpu/s2.rs:178` | Check triangle count and upload capacity. |
| `scoped` | `src/gpu/runtime.rs:102` | Capture scoped GPU errors and balance all scopes. |
| `read_u32` | `src/gpu/runtime.rs:137` | Check mapping completion before copying and unmapping u32 readback. |
| `GpuS2Pipeline::new` | `src/gpu/s2.rs:236` | 初始化 wgpu 设备与蒙特卡洛 S2 计算流水线。 |
| `GpuS2Pipeline::update_mesh` | `src/gpu/s2.rs:480` | 无需重建流水线即可为新网格重新上传三角形数据。 |
| `GpuS2Pipeline::ensure_output_capacity` | `src/gpu/s2.rs:575` | 若调用次数超出当前容量，则扩容输出缓冲区。 |
| `GpuS2Pipeline::calculate_s2_gpu` | `src/gpu/s2.rs:630` | 针对所有半径分派蒙特卡洛 S2 内核并回读结果。 |
| `GpuS2Pipeline::calculate_s2_gpu_seeded` | `src/gpu/s2.rs:640` | 带可选种子（折叠为内核 32 位种子）的 `calculate_s2_gpu`。 |
| `OffsetEntry` | `src/gpu/s2_shell.rs:8` | 与 WGSL 布局匹配的打包 `(radius_idx, dx, dy, dz)` 壳层偏移记录。 |
| `point_inside` (s2_monte_carlo.wgsl) | `src/gpu/shaders/s2_monte_carlo.wgsl` | 认证奇偶性：精确 bbox 排除、认证命中、证明互异的 64 命中路径；返回 0/1/不确定。 |
| `point_inside_overflow` (s2_monte_carlo.wgsl) | `src/gpu/shaders/s2_monte_carlo.wgsl` | 认证的超 64 命中恢复，证明每个相邻间隔超过 CPU 去重带。 |
| `point_inside` (voxelize.wgsl) | `src/gpu/shaders/voxelize.wgsl` | 认证奇偶性：精确 bbox 排除、认证命中、证明互异的 64 命中路径；返回 0/1/不确定。 |
| `point_inside_overflow` (voxelize.wgsl) | `src/gpu/shaders/voxelize.wgsl` | 认证的超 64 命中恢复，证明每个相邻间隔超过 CPU 去重带。 |
| `cert_ray_triangle` / `cert_triangle` / `cert_ge` / `cert_ratio_err` / `max3`（两个着色器） | `src/gpu/shaders/*.wgsl` | 带前向误差界的 Moller-Trumbore，将每个 CPU 阈值判为真/假/未知。 |
| `record_uncertain` (s2_monte_carlo.wgsl) | `src/gpu/shaders/s2_monte_carlo.wgsl` | 将（逻辑 id、精确 p、精确 q）追加到不确定列表。 |
| `AdapterClass` / `classify_adapter` | `src/gpu/context.rs` | 区分软件适配器（CPU 设备类型或已知软件光栅化器名：llvmpipe、lavapipe、SwiftShader、softpipe、Microsoft Basic Render）与硬件适配器。 |
| `GpuContext::adapter_class` / `GpuContext::describe` | `src/gpu/context.rs` | 探测到的适配器类别与一行 `name= backend= class=` 描述。 |
| `GpuTransferStats` / `gpu_transfer_stats` | `src/gpu/runtime.rs` | 进程级上传字节/次数、回读字节/次数、阻塞等待回读的时间（执行加传输，不是内核时间）与设备初始化时间；`describe()` 即 `main` 在任何 GPU 运行后打印的 `[Timing] gpu ...` 行。 |
| `CountedWrite::write_counted` | `src/gpu/runtime.rs:72` | 会计数的 `queue.write_buffer`；crate 内所有 GPU 上传都经过它。 |
| `GpuCertificationStats`（含 `recompute_ratio`、`describe`、`accumulate`） | `src/gpu/certify.rs` | 累计认证计数与 CPU 重算比例。 |
| `CertReference`（含 `new`、`params_tail`、`classify`、仅测试的 `mesh`） | `src/gpu/certify.rs` | 原点平移的 f64 CPU 参考与精确 f32 提前排除界。 |
| `f32_at_least` / `f32_at_most` | `src/gpu/certify.rs` | 定向 f64→f32 舍入，使 f32 比较等价于 f64 比较。 |
| `triangle_constants` / `TRI_CONST_FLOATS` | `src/gpu/certify.rs` | 主机预计算的、与查询无关的认证测试项，每个三角形 16 个 f32（`a, m, e1, es, e2, eps_det, h, det`），对应固定射线方向。 |
| `GpuS2Pipeline::certification_stats` | `src/gpu/s2.rs:566` | MC 累计认证计数。 |
| `GpuS2Pipeline::dispatch_batch` | `src/gpu/s2.rs:852` | 清零不确定计数器、dispatch 一个半径批次、复制结果与列表并读取计数器。 |
| `GpuS2Pipeline::resolve_uncertain` | `src/gpu/s2.rs:923` | 在精确 GPU 点上用 CPU 判定重算不确定样本。 |
| `uncertain_buffers` (s2.rs) | `src/gpu/s2.rs` | 分配 MC 不确定列表及 staging。 |
| `GpuVoxelPipeline::certification_stats` | `src/gpu/voxel.rs:629` | 体素累计认证计数。 |
| `GpuVoxelPipeline::dispatch_voxels` | `src/gpu/voxel.rs:521` | 清零计数器、dispatch 体素化（及归约）、复制结果与列表并读取计数器。 |
| `GpuVoxelPipeline::new_with_shader` | `src/gpu/voxel.rs:117` | 由给定 WGSL 构造（测试传入冻结的未认证基线）。 |
| `voxel_center` / `voxel_uncertain_buffers` / `occupancy_usage` | `src/gpu/voxel.rs` | 精确 f32 单元中心、不确定列表分配、可修补占据场用途。 |
| `GpuShellS2Pipeline` | `src/gpu/s2_shell.rs:22` | 精确壳层对 S2 计算的 GPU 流水线状态。 |
| `build_offset_buffer` | `src/gpu/s2_shell.rs:48` | 将 `(radius_idx, [dx,dy,dz])` 元组转换为 `OffsetEntry` 记录。 |
| `GpuShellS2Pipeline::new` | `src/gpu/s2_shell.rs:74` | 初始化 wgpu 设备与壳层 S2 计算流水线。 |
| `GpuShellS2Pipeline::compute_s2_shell` | `src/gpu/s2_shell.rs:383` | 在占据网格上分派精确壳层对计数并回读 S2(r)。 |
| `GpuVoxelPipeline` | `src/gpu/voxel.rs:7` | 网格体素化的 GPU 流水线状态。 |
| `build_triangle_buffer`（voxel.rs） | `src/gpu/voxel.rs:17` | 为体素化流水线构建归一化的 `f32` 三角形位置缓冲区（与 `s2.rs` 中的实现相互独立）。 |
| `pack_params`（voxel.rs） | `src/gpu/voxel.rs` | 序列化 80 字节参数：射线方向从字节 32 开始，认证尾部从字节 48 开始。 |
| `GpuVoxelPipeline::new` | `src/gpu/voxel.rs:112` | 初始化 wgpu 设备与体素化计算流水线。 |
| `GpuVoxelPipeline::voxelize` | `src/gpu/voxel.rs:285` | 分派光线投射体素化并回读占据网格。 |
| `GpuVolumeTransformPipeline` | `src/gpu/volume_transform.rs:20` | 体数据旋转裁剪的 GPU 流水线状态。 |
| `GpuVolumeTransformPipeline::new` | `src/gpu/volume_transform.rs:40` | 初始化 wgpu 设备与体数据变换计算流水线。 |
| `TransformTile` | `src/gpu/volume_transform.rs:11` | 输出块及其上传源子块的描述。 |
| `GpuVolumeTransformPipeline::transform_tile` | `src/gpu/volume_transform.rs:229` | 用 halo 源子块按单次 dispatch 算术变换一个输出块，并带 halo 守卫。 |
| `GpuVolumeTransformPipeline::reserve_capacity` | `src/gpu/volume_transform.rs:207` | 把源和输出/staging 缓冲预留到分块计划最大值。 |
| `GpuVolumeTransformPipeline::device_limits` | `src/gpu/volume_transform.rs:197` | 限制每块缓冲的设备上限。 |
| `GpuVolumeTransformPipeline::rotate_and_crop` | `src/gpu/volume_transform.rs:170` | 分派旋转/裁剪/重采样内核并回读变换后的体数据。 |
| `GridPlan` | `src/gpu/runtime.rs:170` | Checked two-dimensional grid dispatch. |
| `grid_plan` | `src/gpu/runtime.rs:177` | Validate product, buffer and dispatch limits. |
| `GpuVoxelPipeline::voxelize_limited` | `src/gpu/voxel.rs:357` | Fallible voxel execution with bounded dispatch. |

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

- **源码位置：** `src/gpu/context.rs:3`
- **用途：** `try_init_gpu()` 成功探测后返回的不透明句柄，描述所探测到的 GPU 适配器，以及该适配器上报的缓冲区大小限制。它并不保留 `wgpu::Device`/`wgpu::Queue` 本身——这些在 `try_init_gpu` 结束时即被丢弃；该结构体的存在纯粹是为了给后端选择提供一个能力描述符。

| 字段 | 类型 | 含义 |
|---|---|---|
| `adapter_name` | `String` | 人类可读的适配器名称（例如 `"NVIDIA GeForce RTX 4090"`），由 `wgpu::AdapterInfo` 上报。 |
| `max_buffer_size` | `u64` | 该适配器上任意单个 wgpu 缓冲区的最大字节数。 |
| `max_storage_buffer_binding_size` | `u64` | 该适配器上单个存储缓冲区绑定的最大字节数。 |

#### GpuContext::caps

- **签名：** `pub fn caps(&self) -> BackendCaps`
- **源码位置：** `src/gpu/context.rs:11`
- **用途：** 将 `GpuContext` 的内部字段转换为计算后端选择策略所使用的、与后端无关的 `BackendCaps` 结构体（`crate::compute::backend::BackendCaps`）。
- **参数：** `&self`。
- **返回值：** `BackendCaps { name, supports_gpu: true, max_buffer_size, max_storage_buffer_binding_size }`。
- **副作用：** 无；纯数据转换。
- **说明：** 此处 `supports_gpu` 恒为 `true`，因为 `GpuContext` 只在 `try_init_gpu()` 调用成功之后才会存在。

### GpuInitError

```rust
#[derive(Debug)]
pub struct GpuInitError(String);
```

- **源码位置：** `src/gpu/context.rs:22`
- **用途：** 包装一条人类可读错误消息的新类型（newtype），描述 GPU 初始化失败的原因（未找到适配器、适配器过滤器不匹配、设备请求失败等）。实现了 `std::error::Error`。

**`Display` 实现**（`src/gpu/context.rs:22`）：原样写出被包装的消息，即 `write!(f, "{}", self.0)`。

#### try_init_gpu

- **签名：** `pub fn try_init_gpu() -> Result<GpuContext, GpuInitError>`
- **源码位置：** `src/gpu/context.rs:38`
- **用途：** 通用的 GPU 适配器/设备探测函数。这是 `compute::policy::select_backend` 在向 GPU 分派任务之前，用来判断 GPU 后端是否可用时调用的函数。
- **参数：** 无。行为完全由 `RUSTMSPT_GPU_DEVICE` 环境变量控制（见下文）。
- **返回值：** 成功时返回带有适配器名称与缓冲区大小限制的 `Ok(GpuContext)`；若无法获得合适的适配器/设备，则返回 `Err(GpuInitError)`。
- **副作用：** 跨所有后端（`wgpu::Backends::all()`）创建一个 `wgpu::Instance`，枚举和/或请求一个适配器，并以空特性集和默认限制请求一个逻辑设备，全部通过 `pollster::block_on`（即同步阻塞调用线程）完成。这是一个开销较大的调用——不应在每帧调用或热循环中调用；调用方应当缓存结果。
- **`RUSTMSPT_GPU_DEVICE` 环境变量过滤器：** 在 `AGENTS.md` 中记录为 `RUSTMSPT_GPU_DEVICE=<name or index>`——按名称子串或索引过滤适配器。具体而言：
  - 若未设置，则正常请求默认的 `wgpu::PowerPreference::default()` 适配器（不强制回退适配器）。
  - 若设置且可解析为 `usize`，则视为 `instance.enumerate_adapters(wgpu::Backends::all())` 结果中的**索引**；索引越界会产生 `GpuInitError`，报告所请求的索引及找到的适配器数量。
  - 若设置但不是有效整数，则视为**区分大小写的子串**，与每个已枚举适配器的 `AdapterInfo.name` 进行匹配；使用第一个匹配项。若无匹配则产生 `GpuInitError`。
- **说明：** 自 PERF-03 起通过 `shared_gpu_device()` 探测：某选择器的首次探测创建进程级逻辑设备，之后的探测及所有流水线构造函数（`GpuS2Pipeline`、`GpuShellS2Pipeline`、`GpuVoxelPipeline`、`GpuVolumeTransformPipeline`、`GpuRenderPipeline`、`GpuScenePipeline`）复用该设备；失败的选择器从不缓存。见下文“共享设备与管线缓存（PERF-03）”。

---

## `s2.rs` — `GpuS2Pipeline`（蒙特卡洛 S2）

**着色器：** `src/gpu/shaders/s2_monte_carlo.wgsl` —— 每次着色器调用处理一个 `(radius, sample)` 对：它从一个随机点通过网格投射一条光线（利用三角形相交计数进行内/外分类），检查该半径处对应的伙伴点，并写出逐调用的命中/有效计数器，随后在 CPU 侧求和。

**另请参阅：** [geometry-analysis.md#calculate_s2_with_gpu](geometry-analysis.md#calculate_s2_with_gpu)，[../algorithms/s2-two-point-correlation.md](../algorithms/s2-two-point-correlation.md)

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

- **源码位置：** `src/gpu/s2.rs:10`
- **用途：** 持有 wgpu 设备、队列、已编译的计算流水线，以及重复分派蒙特卡洛 S2 内核（例如跨多次测量迭代）所需预分配的 GPU 缓冲区，避免每次都重新初始化 wgpu 或重新编译着色器。

| 字段 | 类型 | 含义 |
|---|---|---|
| `device` | `wgpu::Device` | 用于创建缓冲区、流水线及命令编码器的逻辑 GPU 设备。 |
| `queue` | `wgpu::Queue` | 用于缓冲区写入与分派的命令提交队列。 |
| `pipeline` | `wgpu::ComputePipeline` | 已编译的 `s2_monte_carlo.wgsl` 计算流水线。 |
| `triangle_buffer` | `wgpu::Buffer` | 归一化三角形顶点位置的存储缓冲区（`f32`，每个三角形 9 个浮点数），绑定 0。 |
| `params_buffer` | `wgpu::Buffer` | 持有打包后的 `Params` 结构体（半径、种子、包围盒、样本数）的存储缓冲区，绑定 1。 |
| `out_hits_buffer` | `wgpu::Buffer` | 逐调用命中计数（`u32`）的存储缓冲区，绑定 2。 |
| `out_valids_buffer` | `wgpu::Buffer` | 逐调用有效样本计数（`u32`）的存储缓冲区，绑定 3。 |
| `num_triangles` | `u32` | 当前三角形数量，用于打包参数时使用。 |
| `bind_group_layout` | `wgpu::BindGroupLayout` | 描述上述四个存储缓冲区绑定的布局。 |

模块级常量：`WORKGROUP_SIZE: u32 = 256`，`MAX_RADII: usize = MC_RADIUS_BATCH = 128`（着色器 `Params.radii` 数组固定为 128 项，因此按每批最多 128 个半径求值；只要 `(r_max + 1) * samples` 不超过 u32 即接受任意 `r_max`），`COALESCE_FACES = 8`，`MAX_UPLOAD_RUNS = 64`（局部上传比较限制）。

#### build_triangle_buffer (s2.rs)

- **签名：** `fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32>`
- **源码位置：** `src/gpu/s2.rs:27`
- **用途：** 将网格的三角形顶点位置展平为适合直接上传作为 wgpu 存储缓冲区的 `f32` 缓冲区，坐标经过偏移，使包围盒最小值位于原点。
- **参数：**
  - `mesh: &Mesh` —— 源网格（顶点 + 面）。
  - `bbox: BoundingBox` —— 用于计算坐标原点偏移量（`bbox.min`）的包围盒。
- **返回值：** `Vec<f32>`，每个三角形 9 个浮点数（3 个顶点 × 3 个分量），每个顶点坐标存储为 `(coord as f32) - bbox.min.<axis> as f32`。
- **副作用：** 无（纯函数）。
- **说明：** 先在 **f64 中减去包围盒原点，再转为 f32**，使 GPU 侧的 `f32` 坐标保持较小（大致为 `0..size`，而不是潜在的较大绝对值），从而提升 GPU 上的浮点精度。这是一个私有的、模块局部的辅助函数——`voxel.rs` 定义了一份独立的、文本上近乎相同的副本（见下文）；两者并未共享，以避免在原本自成一体的流水线之间引入跨模块依赖。

#### pack_params

- **签名：** `fn pack_params(num_triangles: u32, radii: &[f32], seed: u32, bbox: BoundingBox, samples_per_radius: u32) -> Vec<u8>`
- **源码位置：** `src/gpu/s2.rs:48`
- **用途：** 将蒙特卡洛着色器 `Params` 结构体的字段序列化为与 WGSL 结构体的 `std430` 风格布局及对齐方式匹配的原始字节缓冲区（每个 `vec3<f32>` 填充到 16 字节）。
- **参数：**
  - `num_triangles: u32` —— 当前上传网格的三角形数量。
  - `radii: &[f32]` —— 需要计算 S2 的半径；仅使用前 `MAX_RADII`（128）项，其余着色器侧插槽以零填充。
  - `seed: u32` —— 着色器基于 `pcg_hash` 的采样器所使用的随机数种子。
  - `bbox: BoundingBox` —— 仅用于 `bbox.size()`；打包后的 `bbox_min` 被硬编码为 `(0,0,0)`，因为三角形数据已被 `build_triangle_buffer` 归一化到该原点。
  - `samples_per_radius: u32` —— 每个半径的蒙特卡洛样本数。
- **返回值：** 长度为 `16 + 16 + 16 + 16 + MAX_RADII * 4` 字节的 `Vec<u8>`：头部（`num_triangles`、`num_radii`、`samples_per_radius`、`seed`）、`bbox_min`（恒为零，经过 vec4 填充）、`bbox_max`/尺寸（经过 vec4 填充）、`ray_dir`（经过 vec4 填充，取自 `geometry::s2::RAY_DIR_GPU`），随后是固定大小的 `radii` 数组。
- **副作用：** 无（纯函数）。
- **说明：** 光线方向从 `super::super::geometry::s2::RAY_DIR_GPU`，即 `crate::geometry::s2::RAY_DIR_GPU` 导入，因此 CPU 与 GPU 光线投射在内/外分类时使用相同的固定光线方向。

#### GpuS2Pipeline::new

> **特性门控：** 需要 `gpu` cargo 特性；默认构建中不存在。

- **签名：** `pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String>`
- **源码位置：** `src/gpu/s2.rs:139`
- **用途：** 初始化 wgpu 设备/队列，将 `s2_monte_carlo.wgsl` 着色器编译为计算流水线，并预先上传网格的归一化三角形数据。
- **参数：**
  - `mesh: &Mesh` —— 立即上传其三角形的网格。
  - `bbox: BoundingBox` —— 用于坐标归一化的包围盒（参见 `build_triangle_buffer`）。
- **返回值：** 成功时返回 `Ok(GpuS2Pipeline)`；`Err(String)` 描述失败原因（无适配器、适配器过滤器不匹配，或设备请求失败）。
- **副作用：** 执行一次完整的 wgpu 适配器/设备请求（通过 `pollster::block_on` 阻塞），编译着色器模块，创建绑定组布局/流水线布局/计算流水线，并分配加上传三角形、参数与输出缓冲区。两个输出与两个回读缓冲区各以 4 字节开始，按实际调用规模增长并复用。
- **说明：** 此构造函数复制了 `try_init_gpu` 的适配器选择逻辑（包括 `RUSTMSPT_GPU_DEVICE` 索引/子串过滤），而不是直接调用它——两者是相互独立的适配器探测，理论上如果调用之间环境发生变化，`try_init_gpu` 探测到的适配器与该构造函数使用的适配器可能不同（实践中 `RUSTMSPT_GPU_DEVICE` 使得同一进程内的选择是确定性的）。

#### GpuS2Pipeline::update_mesh

- **签名：** `pub fn update_mesh(&mut self, mesh: &Mesh, bbox: BoundingBox) -> Result<(), String>`
- **源码位置：** `src/gpu/s2.rs:313`
- **用途：** 用新网格替换流水线已上传的三角形数据，而无需拆除并重建设备/流水线——用于同一个 `GpuS2Pipeline` 在一批次内跨多个颗粒/网格复用的场景。
- **参数：** `mesh: &Mesh`、`bbox: BoundingBox` —— 与 `new` 语义相同。
- **返回值：** `Ok(())` 或三角形容量/上传错误。
- **副作用：** 通过 `queue.write_buffer` 重新上传归一化三角形数据。若新网格的字节大小超过当前 `triangle_buffer` 的容量，则分配一个更大的新缓冲区并替换 `triangle_buffer`；否则原地复用现有缓冲区。更新 `self.num_triangles`。
- **说明：** 由于缓冲区在重新分配时只会增长（从不缩小），用大小不同的网格反复调用此函数是安全的，但会使流水线在其整个生命周期内保留峰值大小的 GPU 内存。

#### GpuS2Pipeline::ensure_output_capacity

- **签名：** `fn ensure_output_capacity(&mut self, invocations: u32)`
- **源码位置：** `src/gpu/s2.rs:338`
- **用途：** 私有辅助函数，若某次请求的分派所需的调用槽位数超过当前已分配数量，则扩容 `out_hits_buffer`/`out_valids_buffer`。
- **参数：** `invocations: u32` —— 即将分派的着色器调用总数（`num_radii * samples_per_radius`）。
- **返回值：** `()`。
- **副作用：** 若 `invocations * 4 > out_hits_buffer.size()`，可能重新分配两个输出缓冲区（各自每次调用 4 字节）。旧缓冲区内容会被丢弃（不保留），因为新缓冲区会在下一次分派中被完整写入。
- **说明：** 由 `calculate_s2_gpu` 在每次分派前内部调用；不属于公共 API 的一部分。

#### GpuS2Pipeline::calculate_s2_gpu

- **签名：** `pub fn calculate_s2_gpu(&mut self, bbox: BoundingBox, r_max: usize, samples: usize) -> Result<Vec<f64>, String>`
- **源码位置：** `src/gpu/s2.rs:362`
- **用途：** 在 GPU 上，通过单次涵盖所有半径的分派，计算从 0 到 `r_max` 的每个整数半径上的蒙特卡洛两点相关函数 S2(r)。
- **参数：**
  - `bbox: BoundingBox` —— 用于打包着色器参数的包围盒（仅使用尺寸；原点已经归一化）。
  - `r_max: usize` —— 需要评估的最大半径（包含），半径为整数 `0..=r_max`。
  - `samples: usize` —— 请求的每半径蒙特卡洛样本数；被限制在最小值 200（`samples.max(200)`）。
- **返回值：** 成功返回 `Ok(Vec<f64>)`，容量、作用域执行或映射失败返回 `Err(String)`；成功向量长度为 `r_max + 1` 的 `Vec<f64>`，以半径 `r` 为索引，包含该半径所有样本上 `hits/valid` 的平均值（若该半径没有记录到有效样本，则为 0.0）。
- **副作用：** 生成一个随机 `u32` 种子（`rand::random`），打包并上传参数，调用 `ensure_output_capacity`，构建绑定组，记录并提交一次计算通道（`dispatch_workgroups`，`WORKGROUP_SIZE = 256`），随后将两个输出缓冲区都复制到暂存缓冲区，映射以供 CPU 读取（`device.poll(wgpu::Maintain::Wait)`——一次阻塞等待），并将映射得到的 `u32` 切片归约为返回的 `f64` 向量。
- **说明：** 元素 `r=0`（零半径）在本函数的内部累加逻辑中始终保留为 `0.0`——按照源码注释的说明，调用方应当用已知的体积分数覆盖索引 0 的值，而不要信任 GPU 在零半径处的退化样本。位置计算在 GPU 上以 `f32` 进行，但返回的平均值以 `f64` 存储/返回。

### MC 执行错误与容量检查

`GpuS2Pipeline::new`、`update_mesh` 和 `calculate_s2_gpu` 使用配对的 wgpu validation、out-of-memory、internal 错误作用域。后两个接口改为返回 `Result`。映射回调成功后才访问映射内存；不捕获 Rust panic。

- `dispatch_plan(r_max, samples, batch, limits) -> Result<(u32,u32,[u32;2]), String>`：检查批大小（1..=128）、`(r_max + 1) * samples` 是否落在 u32 逻辑编号空间内，以及最大一批的工作组数量、存储绑定和单缓冲区限制；仍至少 200 个样本，不再拒绝 `r_max >= 128`。
- `check_buffer_size(bytes, limits) -> Result<(), String>`：检查存储绑定和单缓冲区字节限制。
- `check_mesh_capacity(mesh, limits) -> Result<(), String>`：展开三角形前检查数量及字节运算；空几何使用四字节占位缓冲区。
- `runtime::scoped<T>(device, work) -> Result<T,String>`：收集三个错误类别，即使操作返回错误也弹出全部作用域。调用者须串行访问设备；这里不捕获 panic。
- `runtime::read_u32(device, buffer) -> Result<Vec<u32>,String>`：检查回调/通道错误，复制 u32 数据，成功后解除映射。

MC bbox 尺寸转为 f32 后必须有限且为正。这里检查设备限制，不是配置的总显存预算；完整峰值规划和缓冲区高水位保留仍待办。目前仅 MC 接入此辅助模块，voxel/shell/crop/render API 不变。旧 `calculate_s2_with_gpu` 包装函数记录错误并以连续 CPU mesh MC 重算，保持 GPU 尝试的方法；该接口没有禁止回退参数。Optimize 单独执行显式回退策略。

### 射线包含判定与溢出恢复（WGSL）

`s2_monte_carlo.wgsl` 和 `voxelize.wgsl` 分别实现以下私有函数：

- `point_inside(point: vec3<f32>) -> u32`：返回 0（外）、1（内）或 `CERT_UNKNOWN`（2）。先做精确 f32 网格 bbox 排除，再逐三角形认证命中（`cert_triangle`）；任一三角形不确定则查询不确定。保留排序 64 次命中的快速路径，并要求排序后每个相邻间隔超过 `1e-8 + 2 max(err_t) + u t`。在**第 65 次确定命中**时调用 `point_inside_overflow`。容量计数包含重复三角形命中。
- `point_inside_overflow(point, dir, pm) -> u32`：一轮计算最大命中距离误差界；之后每步选择上一个之后的最近命中，并统计其去重带内的命中数，多于一个即不确定。辅助空间为常数，工作量 O(T x U)。
- `cert_ray_triangle(o, d, i, pm) -> CertHit {t, err, state}`：采用 CPU 阈值与前向误差界的 Moller-Trumbore；先用无除法分子测试排除近平行远处三角形。误差界推导见算法文档“GPU 射线奇偶性的 f32 认证”。
- 自 PLAN.Performance.md §77 起，认证 MC（binding 5）与体素（binding 4）着色器读取 `tri_const`——由 `certify::triangle_constants` 预计算的每三角形 16 个 f32，不再对每个（查询，三角形）对重复计算与查询无关的项（边、幅值界、`h = d x e2`、`det` 及其误差界）；每次派发的射线方向是一个常量。binding 0 的原始 9 浮点缓冲仍保留，供冻结的未认证着色器与逐样本参考着色器使用。误差界针对正确舍入的 f32 运算推导，对主机端计算同样成立，GPU 无法认证的查询仍由 CPU 重算，因此计数始终等于 CPU 参考。MC 管线的部分三角形上传会同步写入常量缓冲；内存规划按每三角形 36 + 64 字节计。llvmpipe 上 particles.stl 的认证/未认证耗时比：MC 2.41× → 1.34×，体素 2.10× → 1.57×；硬件 GPU 预期在此处受 ALU 限制，但未实测。

去重规则与 GPU 快速路径相同：比较的是上一个**保留**距离，而非上一个原始距离。CPU 的 `1e-8` 容差和 f64 判定仍有区别。
恢复不丢弃、不重抽 MC 样本，MC 的 bbox 预筛仍保留。T 个三角形、U 个不同正向命中的恢复成本为 O(T×U)，最坏 O(T²)；复杂场景可能明显变慢。这是正确性恢复，不代表通用 GPU 提速，也未解决设备或 map 错误。

源码：MC 函数位于 `src/gpu/shaders/s2_monte_carlo.wgsl:62` 和 `:85`；voxel 函数位于 `src/gpu/shaders/voxelize.wgsl:40` 和 `:63`。

---

## `s2_shell.rs` — `GpuShellS2Pipeline`（精确壳层对 S2）

**着色器：** `src/gpu/shaders/s2_shell_pairs.wgsl` —— 每次调用处理预先计算的壳层偏移列表中的一个 `(radius, offset)` 对，遍历体素占据网格，统计该精确整数位移下的有效对与命中对数量。

**另请参阅：** [geometry-analysis.md#calculate_s2_gpu_exact](geometry-analysis.md#calculate_s2_gpu_exact)

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

- **源码位置：** `src/gpu/s2_shell.rs:6`
- **用途：** 单个壳层偏移的 GPU 侧（`bytemuck::Pod`）表示：它属于哪个半径桶（`radius_idx`），以及该偏移所代表的整数体素位移 `(dx, dy, dz)`。`#[repr(C)]` + `Pod`/`Zeroable` 使其可直接与原始字节相互转换以供缓冲区上传，与 WGSL 的 `OffsetEntry` 结构体完全对应（16 字节，四个 4 字节字段）。

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

- **源码位置：** `src/gpu/s2_shell.rs:13`
- **用途：** 直接在体素占据网格上（相对于 `GpuS2Pipeline` 的随机光线投射方法而言）执行精确壳层对 S2 计算所需的设备/队列/流水线状态与预分配缓冲区。

| 字段 | 类型 | 含义 |
|---|---|---|
| `device` | `wgpu::Device` | 逻辑 GPU 设备。 |
| `queue` | `wgpu::Queue` | 命令队列。 |
| `pipeline` | `wgpu::ComputePipeline` | 已编译的 `s2_shell_pairs.wgsl` 流水线。 |
| `occupancy_buffer` | `wgpu::Buffer` | 体素占据网格的存储缓冲区（`u32`，每体素一个），绑定 0。 |
| `offsets_buffer` | `wgpu::Buffer` | `OffsetEntry` 记录的存储缓冲区，绑定 1，按最多 `MAX_OFFSETS` 项分配大小。 |
| `params_buffer` | `wgpu::Buffer` | 持有 `total_offsets`/`nx`/`ny`/`nz` 的存储缓冲区，绑定 2。 |
| `out_valid_buffer` | `wgpu::Buffer` | 逐偏移有效对计数（`u32`）的存储缓冲区，绑定 3。 |
| `out_hits_buffer` | `wgpu::Buffer` | 逐偏移命中对计数（`u32`）的存储缓冲区，绑定 4。 |
| `bind_group_layout` | `wgpu::BindGroupLayout` | 上述五个绑定的布局。 |

模块级常量：`WORKGROUP_SIZE: u32 = 256`，`MAX_OFFSETS: usize = 200_000`（偏移/输出缓冲区的单批容量；全部批次共同参与按半径归约，不再截断）。

#### build_offset_buffer

- **签名：** `fn build_offset_buffer(shell_offsets: &[(u32, [isize; 3])]) -> Vec<OffsetEntry>`
- **源码位置：** `src/gpu/s2_shell.rs:30`
- **用途：** 将 CPU 侧的 `(radius_idx, [dx, dy, dz])` 元组（由几何壳层偏移枚举产生）转换为可上传到 GPU 的 `OffsetEntry` 记录。
- **参数：** `shell_offsets: &[(u32, [isize; 3])]` —— 半径索引与 `isize` 位移三元组的配对。
- **返回值：** `Vec<OffsetEntry>`，每个输入元组对应一项，`dx`/`dy`/`dz` 从 `isize` 收窄为 `i32`。
- **副作用：** 无（纯函数）。
- **说明：** 检查 `isize` 到 `i32` 的转换；无法表示的位移使用 `i32::MAX` 哨兵，超出存储缓冲区限制允许的网格范围。shader 在无符号减法前拒绝绝对位移不小于对应轴尺寸的项，也正确处理 `i32::MIN`。

#### GpuShellS2Pipeline::new

> **特性门控：** 需要 `gpu` cargo 特性；默认构建中不存在。

- **签名：** `pub fn new() -> Result<Self, String>`
- **源码位置：** `src/gpu/s2_shell.rs:48`
- **用途：** 初始化 wgpu 并编译 `s2_shell_pairs.wgsl` 计算流水线。
- **参数：** 无。
- **返回值：** `Ok(GpuShellS2Pipeline)` 或描述适配器/设备失败的 `Err(String)`。
- **副作用：** 执行一次阻塞的 wgpu 适配器/设备请求（通过公共选择函数遵循 `RUSTMSPT_GPU_DEVICE`），编译着色器，构建绑定组/流水线布局，并预分配占据（4 字节——首次使用时增长）、偏移（`MAX_OFFSETS * 16` 字节）、参数（16 字节）以及输出（各 `MAX_OFFSETS * 4` 字节）缓冲区。
- **说明：** 与 `GpuS2Pipeline::new` 和 `GpuVoxelPipeline::new` 不同，此构造函数不接受网格/包围盒参数——占据网格数据在每次调用 `compute_s2_shell` 时提供，而不是在构造时提供。

#### GpuShellS2Pipeline::compute_s2_shell

- **签名：** `pub fn compute_s2_shell(&mut self, occ: &[u32], nx: u32, ny: u32, nz: u32, shell_offsets: &[(u32, [isize; 3])], r_max: usize, _voxel_pitch: f64, vf: f64) -> Result<Vec<f64>, String>`
- **源码位置：** `src/gpu/s2_shell.rs:208`
- **用途：** 通过对每个预先计算的壳层偏移在每个半径上统计有多少体素对在该精确位移下同时被占据（“命中”），以及同时处于边界内/有效（相对于整个占据网格），计算精确（非随机）的 S2(r) 相关函数。
- **参数：**
  - `occ: &[u32]` —— 展平后的体素占据网格（行主序，`nx*ny*nz` 项，1 = 已占据）。
  - `nx, ny, nz: u32` —— 网格维度。
  - `shell_offsets: &[(u32, [isize; 3])]` —— 预计算的半径索引和位移，每批最多 `MAX_OFFSETS` 项，无截断。
  - `r_max: usize` —— `shell_offsets` 中出现的最大半径索引；决定返回向量的长度。
  - `_voxel_pitch: f64` —— 接受但未使用（以 `_` 前缀标记）；体素间距的转换在调用方一侧完成。
  - `vf: f64` —— 已知的体积分数，直接写入 `out[0]` 作为 r=0 的值。
- **返回值：** 长度为 `r_max + 1` 的 `Vec<f64>`：`out[0] = vf`；对于 `r >= 1`，为属于该半径桶的所有偏移上 `hits/valid` 的均值（若该半径没有任何偏移/有效对贡献，则为 0.0）。
- **副作用：** 重新上传占据缓冲区（若大于当前容量则重新分配），上传偏移缓冲区与参数，若 `total_offsets` 超过之前的容量则重新分配两个输出缓冲区，构建绑定组，分派 `total_offsets.div_ceil(WORKGROUP_SIZE)` 个工作组，将输出复制到暂存缓冲区，并执行一次阻塞的 `device.poll(wgpu::Maintain::Wait)` 以便映射供 CPU 回读。
- **说明：** 按半径进行的归约（对每个偏移求和 `hits/valid` 比值并按数量求平均，`shell_sums`/`shell_counts`）在回读之后于 CPU 上完成，而不是在 GPU 上完成——着色器本身只产生原始的逐偏移命中/有效计数。

Shell 归约补充：占据网格只上传一次；每批偏移上传、执行、读回后再复用缓冲区。CPU 跨批次累加每个有效偏移的 `hits/valid`，最后等权平均，不能改为总 hits 除以总 valid。空偏移列表不 dispatch、不读回，返回 `[vf, 0, ...]`。

---

## `voxel.rs` — `GpuVoxelPipeline`

**着色器：** `src/gpu/shaders/voxelize.wgsl` —— 每次调用通过光线投射（坐标已归一化到包围盒原点）测试一个体素中心是否被包含在网格内，并为每个体素写出一个 0/1 占据标记。

此流水线是 `geometry::s2::build_bbox_occupancy` 的 GPU 对应实现——参见 [geometry-analysis.md#build_bbox_occupancy](geometry-analysis.md#build_bbox_occupancy)。

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

- **源码位置：** `src/gpu/voxel.rs:5`
- **用途：** 持有 GPU 侧网格体素化（通过光线投射进行占据网格光栅化）所需的设备/队列/流水线状态与缓冲区。

| 字段 | 类型 | 含义 |
|---|---|---|
| `device` | `wgpu::Device` | 逻辑 GPU 设备。 |
| `queue` | `wgpu::Queue` | 命令队列。 |
| `pipeline` | `wgpu::ComputePipeline` | 已编译的 `voxelize.wgsl` 流水线。 |
| `triangle_buffer` | `wgpu::Buffer` | 归一化三角形位置的存储缓冲区，绑定 0。 |
| `params_buffer` | `wgpu::Buffer` | 网格/光线参数的存储缓冲区，绑定 1。 |
| `occupancy_buffer` | `wgpu::Buffer` | 输出占据标记（`u32`）的存储缓冲区，绑定 2。 |
| `num_triangles` | `u32` | 当前三角形数量。 |
| `bind_group_layout` | `wgpu::BindGroupLayout` | 上述三个绑定的布局。 |

模块级常量：`WORKGROUP_SIZE: u32 = 64`。

#### build_triangle_buffer (voxel.rs)

- **签名：** `fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32>`
- **源码位置：** `src/gpu/voxel.rs:17`
- **用途：** 与 `s2.rs` 中的 `build_triangle_buffer`（如上文所述）用途及逻辑相同：将三角形顶点位置展平为归一化到 `bbox.min` 的 `f32` 缓冲区。
- **参数/返回值/副作用：** 与 `s2.rs` 版本完全相同。
- **说明：** 这是该函数**独立的一份副本**，而非共享/复用的实现——`voxel.rs` 与 `s2.rs` 各自定义了自己私有的 `build_triangle_buffer`。修改三角形归一化逻辑时需注意这一点：在一个文件中的修复不会传播到另一个文件。

#### pack_params（voxel.rs）

- **签名：** `fn pack_params(num_triangles: u32, nx: u32, ny: u32, nz: u32, pitch: f32) -> Vec<u8>`
- **用途：** 序列化 48 字节 WGSL 存储布局：三角形数量位于字节 0，尺寸位于 4/8/12，pitch 位于 16，20/24/28 为零填充，射线方向位于 32/36/40，44 为填充，随后是 32 字节认证尾部：`mesh_lo` 位于 48/52/56，`flags` 位于 60，`mesh_hi` 位于 64/68/72，76 为填充（共 80 字节）。
- **返回值及副作用：** 参数字节，无副作用。完整写入 `RAY_DIR_GPU` 的三个分量，符合 WGSL `vec3` 的 16 字节对齐。

#### GpuVoxelPipeline::new

> **特性门控：** 需要 `gpu` cargo 特性；默认构建中不存在。

- **签名：** `pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String>`
- **源码位置：** `src/gpu/voxel.rs:57`
- **用途：** 初始化 wgpu，编译 `voxelize.wgsl` 计算流水线，并为给定网格上传归一化三角形数据。
- **参数：** `mesh: &Mesh`、`bbox: BoundingBox` —— 待体素化的网格及其包围盒（用于归一化）。
- **返回值：** `Ok(GpuVoxelPipeline)` 或适配器/设备失败时的 `Err(String)`。
- **副作用：** 阻塞的 wgpu 适配器/设备请求（默认电源偏好，不使用 `RUSTMSPT_GPU_DEVICE` 过滤）、着色器编译、绑定组/流水线布局创建、三角形缓冲区分配加上传、参数缓冲区分配（80 字节）、初始 1024 条不确定单元列表及其 staging、原点平移的 f64 `CertReference`，以及一个最小的 4 字节占位占据缓冲区（在首次 `voxelize` 调用时增长）。
- **说明：** 与 `GpuShellS2Pipeline::new` 一样，此构造函数不遵循 `RUSTMSPT_GPU_DEVICE`。

#### GpuVoxelPipeline::voxelize

- **签名：** `pub fn voxelize(&mut self, nx: u32, ny: u32, nz: u32, pitch: f32) -> Result<Vec<u32>, String>`
- **源码位置：** `src/gpu/voxel.rs:195`
- **用途：** 使用 GPU 光线投射，将（在构造时或先前调用中上传的）网格光栅化为给定维度与体素间距的三维占据网格。
- **参数：**
  - `nx, ny, nz: u32` —— 网格维度（各轴的体素数量）。
  - `pitch: f32` —— 体素边长，以网格坐标单位表示（各轴统一）。
- **返回值：** 长度为 `nx*ny*nz` 的 `Vec<u32>`，行主序，1 = 体素中心位于网格内部，0 = 位于外部。
- **副作用：** 打包并上传参数（包括固定光线方向 `crate::geometry::s2::RAY_DIR_GPU`，与 CPU 侧及蒙特卡洛 S2 的光线方向共用），若有需要则扩容占据缓冲区，构建绑定组，分派 `total.div_ceil(WORKGROUP_SIZE)`（`WORKGROUP_SIZE = 64`）个工作组，将结果复制到暂存缓冲区，并执行一次阻塞的映射/读取（`device.poll(wgpu::Maintain::Wait)`）。
- **说明：** 假定当前 `triangle_buffer` 中的网格三角形数据（来自 `new` 或使用同一网格的先前 `voxelize` 调用）与 `nx/ny/nz/pitch` 所隐含的归一化坐标空间一致；此流水线上没有 `update_mesh` 方法（不同于 `GpuS2Pipeline`），因此体素化不同网格需要构造一个新的 `GpuVoxelPipeline`。

---

## `volume_transform.rs` — `GpuVolumeTransformPipeline`

**着色器：** `src/gpu/shaders/volume_transform.wgsl` —— 每次调用通过对一个输出体素应用逆旋转，将其映射回（相对于质心的）源体数据坐标，并以最近邻或三线性插值对源体数据进行采样，从而计算出该输出体素的值。

此流水线用于裁剪流水线的 PCA 对齐步骤。

**另请参阅：** [pipeline-crop-and-splitfilter.md](pipeline-crop-and-splitfilter.md)，[../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md)

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

- **源码位置：** `src/gpu/volume_transform.rs:5`
- **用途：** 持有 GPU 加速的体数据旋转裁剪（例如将分割后的体数据旋转到 PCA 对齐坐标系，并裁剪/重采样到目标包围区域）所需的设备/队列/流水线状态与可增长缓冲区。

| 字段 | 类型 | 含义 |
|---|---|---|
| `device` | `wgpu::Device` | 逻辑 GPU 设备。 |
| `queue` | `wgpu::Queue` | 命令队列。 |
| `pipeline` | `wgpu::ComputePipeline` | 已编译的 `volume_transform.wgsl` 流水线。 |
| `src_buffer` | `wgpu::Buffer` | 源体数据（`i32` 标签/数值）的存储缓冲区，绑定 0。 |
| `params_buffer` | `wgpu::Buffer` | 变换参数（维度、插值模式、背景值、旋转矩阵、质心、原点）的存储缓冲区，绑定 1，128 字节。 |
| `out_buffer` | `wgpu::Buffer` | 输出（旋转加裁剪后）体数据的存储缓冲区，绑定 2。 |
| `bind_group_layout` | `wgpu::BindGroupLayout` | 上述三个绑定的布局。 |
| `current_src_size` | `u64` | 跟踪 `src_buffer` 当前已分配的大小，以便在源体数据大小不增长时避免每次调用都重新分配。 |
| `current_out_size` | `u64` | 跟踪 `out_buffer` 当前已分配的大小，用途相同。 |

模块级常量：`WORKGROUP_SIZE: u32 = 64`（仅沿输出 X 维度应用；Y 与 Z 每行/每片分派一个工作组——参见 `rotate_and_crop`）。

#### GpuVolumeTransformPipeline::new

> **特性门控：** 需要 `gpu` cargo 特性；默认构建中不存在。

- **签名：** `pub fn new() -> Result<Self, String>`
- **源码位置：** `src/gpu/volume_transform.rs:23`
- **用途：** 初始化 wgpu 并编译 `volume_transform.wgsl` 计算流水线。
- **参数：** 无。
- **返回值：** `Ok(GpuVolumeTransformPipeline)` 或适配器/设备失败时的 `Err(String)`。
- **副作用：** 阻塞的 wgpu 适配器/设备请求（默认电源偏好，不使用 `RUSTMSPT_GPU_DEVICE` 过滤）、着色器编译、绑定组/流水线布局创建，以及最小占位 `src_buffer`/`out_buffer`（各 4 字节，在首次 `rotate_and_crop` 调用时增长）加上固定 128 字节的 `params_buffer` 的分配。
- **说明：** 与体素化及壳层 S2 构造函数一样，构造时不需要提供网格/体数据——所有按调用提供的数据都在 `rotate_and_crop` 中传入。

#### GpuVolumeTransformPipeline::rotate_and_crop

- **签名：**
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
- **源码位置：** `src/gpu/volume_transform.rs:145`
- **用途：** 使用给定旋转矩阵，围绕一个质心旋转带标签/数值的源体数据，将其重采样为一个从 `origin` 开始、维度为 `out_w × out_h × out_d` 的新轴对齐输出体数据，并以 `background` 填充越界采样。
- **参数：**
  - `src_data: &[i32]` —— 展平后的源体数据，行主序，`idx3d(x,y,z) = z*src_w*src_h + y*src_w + x`。
  - `src_w, src_h, src_d: u32` —— 源体数据维度。
  - `background: i32` —— 用于填充其逆旋转源坐标落在源体数据之外的输出体素的值。
  - `rot: &Matrix3<f64>` —— 旋转矩阵（提取行主序的值并转换为 `f32` 供着色器使用）；着色器应用对应的逆旋转，将输出坐标映射回源空间。
  - `centroid: &Vector3<f64>` —— 旋转基准点，以源体数据坐标表示。
  - `origin: &Vector3<f64>` —— 输出体素 `(0,0,0)` 在旋转/居中坐标系中的坐标（即裁剪的输出偏移量）。
  - `out_w, out_h, out_d: u32` —— 输出体数据维度。
  - `interp_mode: u32` —— `0` = 最近邻，`1` = 三线性插值（按照着色器 `Params.interp_mode` 注释所述）。
- **返回值：** 长度为 `out_w*out_h*out_d` 的 `Vec<i32>`，即重采样/裁剪后的体数据。
- **副作用：** 若新数据超出 `current_src_size`/`current_out_size`，则重新分配 `src_buffer`/`out_buffer`（缓冲区在同一流水线实例的多次调用中只会增长，从不缩小），上传源数据及打包后的参数（旋转矩阵作为三行填充过的 `vec4<f32>`，质心与原点作为填充过的 `vec4<f32>`），构建绑定组，分派 `(out_w.div_ceil(64), out_h, out_d)` 个工作组（每个输出行、每个输出切片一个工作组组，`WORKGROUP_SIZE = 64` 仅沿 X 方向），将结果复制到暂存缓冲区，并执行一次阻塞的映射/读取。
- **说明：** 所有浮点变换输入（`rot`、`centroid`、`origin`）在 Rust 侧为 `f64`，但在为着色器打包时被收窄为 `f32`——这与其他流水线在 GPU 侧使用 `f32` 精度的做法一致。分派形状（`wg_y = out_h`、`wg_z = out_d`，即每行/每片使用一整个工作组，而不是像 X 维度那样进行 `div_ceil` 分块）意味着较大的 `out_h`/`out_d` 值会直接转化为较大的 `y`/`z` 工作组数量——调用方应当注意，这与 X 维度的分块方式不同。

---


### 带源 halo 的输出分块（`TransformTile`、`transform_tile`、`reserve_capacity`、`device_limits`）

```rust
pub struct TransformTile<'a> {
    pub block: &'a [i32],
    pub block_origin: [u32; 3],
    pub block_dims: [u32; 3],
    pub source_dims: [u32; 3],
    pub tile_offset: [u32; 3],
    pub tile_dims: [u32; 3],
}
```

- **`transform_tile(&mut self, tile: &TransformTile, background: i32, rot, centroid, origin, interp_mode) -> Result<Vec<i32>, String>`** 只用 `block`（完整 `source_dims` 内位于 `block_origin`、尺寸为 `block_dims` 的源子块）变换一个输出块（整个输出中位于 `tile_offset`、尺寸为 `tile_dims`）。参数扩展为 160 字节：原 112 字节之后依次是 `tile_offset`、`block_origin`、`block_dims`，各为补齐的 `vec4<u32>`。着色器计算 `origin + f32(local + tile_offset)`，即与整体 dispatch 相同的绝对 f32 坐标；仍按完整源尺寸判断越界（背景判定不变）；体内体素经 `fetch` 按子块索引读取。子块不包含的读取会置位第四个绑定 —— `atomic<u32>` 守卫字，它在块完成后复制进 staging，使调用返回错误。校验：子块长度等于 checked 尺寸乘积，子块与输出块终点按 u32 和源尺寸检查，子块不超过设备缓冲上限，三线性的 f32 精确性按子块检查。
- **`rotate_and_crop`** 现等价于以整个源为子块、整个输出为单块的 `transform_tile`，现有调用者不变。
- **`reserve_capacity(src_bytes, out_bytes)`** 一次性把源与输出/staging 缓冲增长（不缩小）到计划最大值，后续块不再重新分配；**`device_limits()`** 暴露规划器使用的设备上限。staging 现为输出容量 + 4 字节（守卫字），`release_output_capacity` 将其重置为 8 字节。
- **内存：** 分块运行的逻辑峰值为 `2·max_block + 2·max_tile + 4` 个字加 160 字节参数和 8 字节守卫（见 `crop_gpu_peak_bytes`）。

## 四条流水线的共享约定小结

- **实例复用、设备独立：** 枚举复用进程内实例并串行执行；选中的 GL 适配器改用独立实例以隔离 EGL 上下文。构造器仍使用新的适配器、设备和队列。逻辑设备与编译管线共享仍待实现。
- **阻塞式 GPU 回读：** 每个“分派并读取”方法（`calculate_s2_gpu`、`compute_s2_shell`、`voxelize`、`rotate_and_crop`）都使用 `map_async` 加上随后的 `device.poll(wgpu::Maintain::Wait)`，这会阻塞调用线程直到 GPU 工作与缓冲区映射完成。这些流水线均未提供异步或非阻塞 API。
- **只增长、从不缩小的缓冲区：** 用于复用缓冲区的字段（`triangle_buffer`、`out_hits_buffer`、`occupancy_buffer`、`src_buffer`/`out_buffer` 等）仅在新调用的数据超过当前容量时才会重新分配；它们从不缩容，因此一个流水线实例在其生命周期内的峰值 GPU 内存占用，等于该实例所收到的所有调用中的最大占用。
- **GPU 上用 `f32`，CPU 上用 `f64`：** 全部四条流水线都会将 `f64`/`isize` 的 CPU 侧几何数据收窄为 `f32`/`i32` 以供 GPU 上传，并在回读时将结果放宽回 `f64`/`i32`，这与每个 WGSL 着色器中全程使用 32 位类型的做法相匹配。

### Voxel/shell 执行检查（PERF-04/05）

构造器与求值均接入配对错误作用域及检查后的读回。`voxelize`、`compute_s2_shell` 返回 `Result`；零/溢出的网格尺寸、存储/dispatch 超限及 occupancy 长度不符会报错。Voxel pitch 必须有限且为正。`runtime::grid_plan([nx,ny,nz], width, limits)` 返回检查后的体素数、字节数和二维 dispatch，shader 展平 x/y 调用坐标并保护填充索引。`voxelize_limited` 用额外工作组限制测试实际 shader 的二维路径。`try_calculate_s2_gpu_exact` 将非正 pitch 规范为 1.0，拒绝非有限输入并传播构造/执行错误；旧 Vec 包装函数仍记录错误并回退 CPU exact。


### Volume transform validation (2026-09-18)

The constructor honors `RUSTMSPT_GPU_DEVICE` through the common adapter selector. `rotate_and_crop` returns `Result`, validates dimension products, buffer/device limits, source length, finite transform parameters and interpolation mode before dispatch. Trilinear input integers must be exactly representable in f32. Checked two-dimensional dispatch and scoped GPU/readback errors replace unchecked execution. Nearest sampling preserves i32 bit patterns; ties round away from zero. Device and staging reuse across separate pipeline instances is still pending.


### Render error contract

`GpuRenderPipeline::new` and `render` use balanced validation, allocation and internal error scopes. `render` rejects zero/oversize textures, vertex-count overflow, and oversize vertex/staging buffers before allocation. Readback checks the mapping callback before accessing mapped memory. An initialized device returning a render error is a test failure, not an unavailable-adapter skip.


GPU selection regression: direct voxel and shell constructors are tested in child processes with nonexistent adapter names and out-of-range indices. Both must report the requested selector; default-device substitution is forbidden. Common selection does not yet imply shared device or compiled-pipeline caching.


`context::select_adapter` 是能力探测与所有 GPU 构造函数共用的唯一适配器选择实现；`shared_device_for` 对每个选择器只请求并缓存一个逻辑设备（见下文 PERF-03 节）。


### Backend instance lifetime (PERF-03)

`shared_instance` retains one instance in OnceLock. Adapter selection and disposal of unselected adapters are serialized because wgpu 24's EGL enumeration can otherwise race its context access. Selected GL adapters are recreated from private instances, keeping device operations isolated. Every adapter is used for only one device request, as required by wgpu's API contract. No failed selector/device result is memoized. This avoids repeated backend-instance initialization for non-GL paths; it does not yet share devices, queues or compiled pipelines. Initial unsynchronized shared-instance tests exposed EGL BadAccess and are retained alongside the corrected regression logs.


### MC output and staging capacity (PERF-03)

MC initializes two output and two readback buffers at four bytes each, replacing the former fixed 39.1 MiB output reservation. `ensure_output_capacity` grows all four only when necessary. `read_u32_prefix` maps and copies only the live invocation prefix, checking nonzero aligned size and capacity first; successful reads unmap before reuse. Smaller/repeated calls preserve buffer identity. `release_output_capacity() -> Result<(), String>` resets those four buffers to four bytes while retaining geometry, parameters and the compiled pipeline. Capacities otherwise retain their high-water mark. `measure` estimates a fresh MC call as `16 * invocations + triangle_bytes + 576`; optimizer's explicit-budget support remains a separate open item.


### Voxel and volume-transform staging reuse

Voxel occupancy and transform output buffers now retain matching readback buffers, initially four bytes each. Growth occurs only when a call exceeds capacity; checked prefix mapping returns exactly the current grid, even after a larger call. `GpuVoxelPipeline::release_grid_capacity` resets occupancy/readback storage; `GpuVolumeTransformPipeline::release_output_capacity` resets output/readback storage. Both preserve compiled pipelines and input geometry/source capacity. These release methods return allocation errors and permit subsequent evaluation. Per-call values, dimensions and pitch remain uploaded normally, so retained capacity does not imply retained results. Full device-budget/high-water policy and resident voxel-to-shell chaining remain separate work.

### Shell 批次容量

GpuShellS2Pipeline 按需增长并复用 offset/output/staging，初始一个偏移，读回仅有效前缀。release_batch_capacity() 返回 Result<(), String>，重置 offset 为 16 字节、每个 output/staging 为 4 字节，保留 occupancy 和编译状态。私有 resize_batch_buffers(count) 在调用方错误作用域内执行；200000 批次上限及逐偏移等权归约不变。

| `GpuShellS2Pipeline::resize_batch_buffers` | `src/gpu/s2_shell.rs:320` | Shell batch buffer capacity management; occupancy retained. |

| `GpuShellS2Pipeline::release_batch_capacity` | `src/gpu/s2_shell.rs:357` | Shell batch buffer capacity management; occupancy retained. |

`runtime::read_u32` 仅供测试整缓冲映射；生产调用使用 `read_u32_prefix` 指定有效字节数。

### Scene 预览工作集策略

mesh_render.gpu_memory_limit_mb 可选限制逻辑 GPU 工作集；gpu_min_pixels 仅用于 auto，缺省 0 保留既有预览选择。低于阈值的 auto 不初始化 GPU；显式 GPU 绕过阈值但遵守预算。预算/执行失败时 auto 回退，gpu 报错，CPU 跳过 GPU 专属资源选项。

checked planner 按可见三角形每面 120、启用 segment 每条 64、marker 每个 192 字节计数；目标 color/depth 为 8×pixels，readback 为按 256 字节对齐的 RGBA 行，uniform 128 字节。保守计入待完成 queue 上传，逻辑峰值 = 2×geometry_bytes+256+8×pixels+staging_bytes。多个视图共用目标。驱动/pipeline 内部及主存场景/PNG 不计入，所以这不是物理 VRAM/RSS 上限。独立 device 限制检查先于 host 顶点展开，上传后即释放临时 host 顶点。构造器返回受作用域保护的 GPU 验证/分配错误；图像分块仍待实现。

| `SceneRenderMemory::plan` | `src/compute/render_memory.rs:20` | Checked scene preview workset, budget and buffer planning without allocation. |
| `scene_strip_rows` | `src/compute/render_memory.rs:114` | 选出场景预览工作集能放进可选 MiB 预算的最高水平条带（二分得到精确边界）；整图放得下时返回整图高度，一行都放不下时报错。 |

| `SceneRenderMemory::check_budget` | `src/compute/render_memory.rs:77` | Checked scene preview workset, budget and buffer planning without allocation. |

| `SceneRenderMemory::check_buffers` | `src/compute/render_memory.rs:93` | Checked scene preview workset, budget and buffer planning without allocation. |

### Optimize MC 显存预算

优化器的共享 GPU MC 管线以空几何启动，各阶段上传实际求值网格。mc_evaluation_peak 保守计算 triangle、四个 output/readback、608 字节参数缓冲、不确定样本列表及其 staging、待执行队列上传以及本次几何/参数上传；增长时计入旧容量加新容量。启动按输入面数和最大配置阶段样本数检查，每次求值在同一 GPU mutex 内重新检查实际保留容量后才上传。因此更大的参考网格或保留峰值也可能触发原有同方法 CPU 回退或严格阶段错误。待上传字节仅在成功回读后清零。驱动内部及 CPU 网格/读回向量不属于逻辑 GPU 预算；分批和自动缩容仍待完成，release_output_capacity 提供显式释放。本节取代早先“显式预算始终回退”的说明。

**按预算确定半径批（PLAN.Performance.md §79 第 5 项）。** `GpuS2Pipeline::check_evaluation_budget` 不再只做接受/拒绝：它保存峰值在上限内的最大半径批（`mc_largest_batch`，每次派发 1..=128 个半径），派发使用该批大小。计数对任意批大小都相同（逻辑样本编号是全局的），因此预算紧张时缩小批次而不是把该方法交给 CPU；只有每次派发一个半径仍超限时才拒绝。measure 与优化器启动检查出于同一原因按单半径峰值判断 GPU 是否可行，measure 并在求值前先规划批大小。逻辑样本编号 `radius * samples + sample` 按设计保持 u32——它是着色器随机数的键，放宽会改变所有样本流——因此 `(r_max + 1) * samples > u32::MAX` 仍是显式错误。测试：`a_tight_budget_shrinks_the_radius_batch_and_keeps_the_counts`（1 MiB 迫使批小于 128；计数与无预算时相同）与 `largest_batch_is_the_exact_budget_boundary`。

证书列表的扩容现在也纳入预算（PLAN.Performance.md §72）。`GpuS2Pipeline::set_memory_limit_mb` 保存调用方的上限（optimize 与 measure 在 `check_evaluation_budget` 旁设置）；在扩容溢出的不确定列表之前，管线检查 `mc_regrowth_peak(retained_peak, entries)`——保留峰值加上新列表及其 staging（旧的一对此时仍存活）——超出则返回带 `certification list regrowth` 字样的错误，由调用方的回退策略处理。体素管线新增 `GpuVoxelPipeline::set_regrowth_headroom`，exact 路径把它设为预算减去计划峰值；超出计划列表的扩容最多只能增加这么多字节。测试：`mc_uncertain_list_regrowth_respects_the_budget`、`uncertain_list_regrowth_respects_the_budget_headroom`。

| `mc_evaluation_peak` | `src/compute/mc_memory.rs:21` | Check logical MC peak including retained capacity and pending uploads. |
| `mc_evaluation_peak_batched` | `src/compute/mc_memory.rs:35` | 以半径批大小（每次派发的半径数）为参数的 `mc_evaluation_peak`。 |
| `mc_largest_batch` | `src/compute/mc_memory.rs:97` | MC 峰值在 MiB 上限内的最大半径批（1..=128）；只有每次派发一个半径仍放不下时才报错。 |

| `check_mc_budget` | `src/compute/mc_memory.rs:131` | Check logical MC peak including retained capacity and pending uploads. |

| `GpuS2Pipeline::check_evaluation_budget` | `src/gpu/s2.rs:451` | Check logical MC peak including retained capacity and pending uploads. |
| `GpuS2Pipeline::set_memory_limit_mb` | `src/gpu/s2.rs:446` | Store the logical budget that uncertain-list regrowth must respect. |
| `mc_regrowth_peak` | `src/compute/mc_memory.rs:85` | Retained peak plus a regrown uncertain list and its staging. |
| `GpuVoxelPipeline::set_regrowth_headroom` | `src/gpu/voxel.rs:624` | Bytes a voxel uncertain-list regrowth may add beyond the planned list. |

Measure 连续 MC 同步改用共享冷启动增长 peak planner，计入几何/参数待上传数据，取代早先 16×invocations+triangle_bytes+576 估计；本批 exact 预算不变。

### GPU MC 工作组整数归约

生产 MC shader 每个 256-lane 工作组对应一个（半径，样本块）。有效 lane 仍用 radius×samples_per_radius+sample 作为 RNG 逻辑编号；尾部填充 lane 贡献零。工作组以整数归约输出一对 hit/valid 部分和，CPU 用 u64 合并并沿用原比值；回读字节变为 8×(r_max+1)×ceil(max(samples,200)/256)。半径填充不会混合计数或重抽样本。dispatch_plan 分别检查逻辑编号溢出、补齐后的组数和部分和缓冲容量；mc_evaluation_peak 同步采用部分和容量，仍计入保留/上传/增长峰值。

私有 calculate_s2_gpu_counts 固定 seed 返回整数总数；公开 API 仍抽取一个新随机 seed 并返回曲线。tests/fixtures/s2_monte_carlo_samples.wgsl 冻结归约前 shader，仅供相同 seed 的逐项计数对照，覆盖尾块、128 半径、几何更新。BVH、GPU 半径最终归约和按预算拆样本仍待独立实施；这里未证明 CPU/GPU f64 等价或真实硬件加速。

| `GpuS2Pipeline::new_with_shader` | `src/gpu/s2.rs:241` | GPU MC partial-count execution and fixed-seed reference validation. |

| `GpuS2Pipeline::calculate_s2_gpu_counts` | `src/gpu/s2.rs:672` | GPU MC partial-count execution and fixed-seed reference validation. |

MC 工作组现沿两个 dispatch 维度展开，以 group.x + group.y × num_workgroups.x 得到块编号。统一分支在 barrier 和写出前排除填充组，即使保留缓冲容量大于本次有效结果也不写入尾部。planner 返回样本数、部分和数量及 [x,y] 调度形状；逻辑样本编号仍受 u32 限制。Optimize 启动检查已移除旧单维调用上限。强制 3×3/4×2 的实测整数计数与冻结逐样本 shader 一致，额外容量哨兵验证尾部未被写入。65,536 组的大任务仅验证规划结果，不代表大任务 GPU 实测。

GPU shell 在排除超出任意轴的位移后，以 (nx−|dx|)×(ny−|dy|)×(nz−|dz|) 直接计算合法配对数。主机已检查完整网格乘积不超过 u32，因此重叠子体积乘积不会溢出。命中数仍遍历占据对，CPU 仍对完整偏移比值等权平均；体素分块和设备常驻 occupancy 仍待完成。独立原始计数 oracle 用有符号坐标穷举小网格全部偏移，覆盖薄网格、空/满/混合占据及 i32 极端位移。此算术修改本身不构成已测得的提速结论。

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::new_with_shader` | `src/gpu/s2_shell.rs:82` | Private constructor taking shader source; returns initialized resources or a GPU error. Production uses the analytic shader; tests can use the frozen enumerated-count fixture. |

实验 shader s2_shell_cooperative.wgsl 每个偏移分配一个 256-lane 工作组，lane 跨步遍历重叠体积并在共享内存归约整数 hit，valid 保持解析计算。二维工作组展平后在 barrier 前统一拒绝填充组。每个偏移仍输出一对计数，因此尚不是独立调度的多个体素 tile，也未实现 voxel→shell 设备常驻数据流。生产仍选择 direct shader，等待工作量基准决定。new_with_shader(source, offsets_per_workgroup) 将 shader 索引与调度宽度配对：direct 为 256，cooperative 为 1。

实验 tiled shader 按（offset，voxel tile）分派工作组，支持正整数块大小。块内整数归约 hit 并输出该块解析 valid 数；空块输出零。主机用 u64 合并同一 offset 全部块的计数，再形成完整偏移比值。每批至多 200000 个部分结果槽，因此块数增加时减少每批 offset 数；单偏移超过 200000 块时明确报容量错误。此固定上限尚不是完整用户预算规划。参数缓冲为 24 字节（offset 数、三轴维度、每偏移块数、块大小），旧 direct shader 读取前 16 字节。生产继续使用 direct，tiled 路径仍在验证和测量。

实验 tiled 路径可启用第二次设备端计算 s2_shell_reduce.wgsl，将每个 offset 的 tile hit/valid 合并为一对整数。每个偏移的总计数不超过已检查的完整网格大小，因此 u32 合并不溢出；CPU 仍按原定义平均完整偏移比值。归约 pipeline 和最终缓冲延迟构造并复用，显式 batch release 缩小最终缓冲但保留已编译归约器。回读恢复为每偏移 8 字节，与 tile 数无关；中间 tile 缓冲及额外 dispatch 仍存在。此选项仍为实验路径，不代表生产选择或已证明提速。

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::ensure_reduction` | `src/gpu/s2_shell.rs:263` | Lazily compile the device tile reducer and grow its final buffers under the caller error scope. |

生产 shell 求值在上传前过滤位移绝对值达到任意轴维度的 offset，以 unsigned_abs 安全处理 isize::MIN。过滤保持输入顺序，复用有界主机批次，不额外保存完整 offset 列表。全部 offset 无支持时直接返回原 VF/零曲线，不上传 occupancy 或分配结果缓冲。测试参考构造器可关闭过滤，继续验证 shader 对无效位移的保护。完整偏移比值及等权平均不变；此过滤尚未移除有效偏移内部的空 tile。

| Function | Source | Contract |
|---|---|---|
| `offset_has_overlap` | `src/gpu/s2_shell.rs:61` | Check all unsigned displacement magnitudes against grid dimensions without signed overflow. |

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::with_device` | `src/gpu/s2_shell.rs:87` | Build production shell resources on supplied device/queue; no new device. |
| `GpuShellS2Pipeline::build_on_device` | `src/gpu/s2_shell.rs:100` | Compile shell resources on supplied handles with balanced GPU error scopes. |
| `GpuShellS2Pipeline::compute_s2_shell_resident` | `src/gpu/s2_shell.rs:410` | Read a same-device occupancy buffer directly; caller serializes producer and consumer. |
| `GpuShellS2Pipeline::compute_shell_input` | `src/gpu/s2_shell.rs:459` | Shared execution for host-uploaded or resident occupancy with identical offset semantics. |
| `GpuVoxelPipeline::shared_device` | `src/gpu/voxel.rs:103` | 为后续阶段共享进程设备句柄（`Arc<SharedGpuDevice>`）；不创建设备。 |
| `GpuVoxelPipeline::occupancy_buffer` | `src/gpu/voxel.rs:98` | Clone completed occupancy storage handle; producer must not overwrite while consumed. |

GPU exact 现将 shell 构造在 voxel 的同一 Device/Queue 上，直接绑定其 occupancy 缓冲。shell 不再选择适配器或申请第二个设备，也不再为占据场分配和上传副本。两阶段顺序运行，各自配对错误作用域。Voxel 现于设备端计数，仅为 VF 回读 4 字节；上层 backend 能力探测仍独立计数。独立主机占据场 API 保留上传行为，之后的主机求值不会覆盖借用的 voxel 缓冲。

voxelize_count 在 voxelization 后执行延迟编译的整数占据归约，完整占据场留在设备，仅回读一个 u32。归约器使用一个 256-lane 工作组跨步扫描；二值占据与已检查的网格大小保证各级计数不超过 u32。总工作量仍为 O(网格体素数)，减少传输不等于必然降低时延。occupancy 和 staging 分别按需增长：count-only 只需 4 字节 staging，之后完整回读再按需增长；模式切换和释放后重算已有对照。GPU exact 用该计数计算 VF 并将驻留占据场传给 shell，独立 voxelize 保留 Vec 返回契约。

| Function | Source | Contract |
|---|---|---|
| `GpuVoxelPipeline::voxelize_count` | `src/gpu/voxel.rs:296` | Voxelize and return only the occupied-cell count; retain the device field. |
| `GpuVoxelPipeline::ensure_counter` | `src/gpu/voxel.rs:315` | Lazily construct the integer counter and four-byte output under caller error scope. |
| `GpuVoxelPipeline::voxelize_impl` | `src/gpu/voxel.rs:374` | Checked common voxel execution with full-grid or count-only readback. |

GPU exact 现逐半径延迟生成一个 shell Vec，经 compute_s2_shell_resident_stream 按批消费。批次仍受已有部分结果槽上限约束，可跨半径但保持原偏移顺序。插值支持标志在生成该半径时记录，取消原来的第二遍枚举；成功求值会消费全部偏移，包括末尾无支持偏移。内存范围为一个半径 shell 加一个批次，并非与半径无关的常量；单个大半径 shell 仍会物化。累计生成数量采用 u128，日志不静默饱和截断。

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::compute_s2_shell_resident_stream` | `src/gpu/s2_shell.rs:435` | Consume ordered offsets lazily in bounded batches on a resident grid; preserve per-offset ratios. |

GPU exact 现使用 shell_offset_iter，单个半径内部也只保留嵌套范围游标；保持原 x/y/z 顺序、原点特殊情况和半开平方距离判定。需要随机访问的公共 Vec API 保持不变。用 peekable 判定该半径是否有支持，生成数量在消费时累计。普通范围沿用原整数范数，更大范数用 u128 避免有符号乘法溢出。仍扫描包围立方体，降低分配并未改变 O(半径³) 搜索复杂度。

新建的驻留 GPU exact 求值在后端选择和执行中共用 ExactMemoryPlan。设 T=max(36×faces,4)、M=4×cells、B 为批次部分结果槽数，保守逻辑峰值为 2T+M+128+80B，计入待完成三角/offset 上传和批次增长时同时存在的新旧缓冲；128 字节覆盖固定参数/计数/占位资源。B 从 200000 按 MiB 预算缩小，至少 1；最小批次仍超限则在设备初始化前拒绝，由调用方执行配置的回退策略。该模型不含驱动/编译器内部资源和 CPU 内存，仅适用于新建生产 direct-shell exact 求值，不声称覆盖实验 tiled/reduced 或任意已有高水位管线。旧 exact 网格硬限制仍独立存在。

### 共享设备与管线缓存（PERF-03）

wgpu 24 中一个 `Adapter` 只能创建一个逻辑设备，因此缓存位于适配器选择之上：`shared_device_for(selector)` 维护进程级 `HashMap<Option<String>, Arc<SharedGpuDevice>>`，键为 `RUSTMSPT_GPU_DEVICE` 的原值（未设置单独成键）。首次请求时通过未改动的 `select_adapter` 选择适配器（共享 Instance 串行枚举；选中的 GL 适配器仍来自私有 Instance），以空特性/默认限制请求一个设备，注册设备丢失回调并递增 `gpu_device_creation_count()`。创建在缓存锁内进行，并发构造只创建一个设备。错误直接返回、从不写入缓存，无效选择器持续报错且不影响之后的有效请求。报告丢失（驱动丢失或 `Device::destroy`）的条目在下次请求时被剔除并重建。

`SharedGpuDevice::cached_pipeline(kind, source, build)` 在该设备上按 `(kind, WGSL 源文本)` 保存一个可克隆的管线组合（管线加绑定组布局，或场景的两条管线）。编译在 `runtime::scoped` 内以独立错误作用域执行，因此总是先取作用域锁再取映射锁；编译失败（校验错误）直接返回、不缓存。`gpu_pipeline_build_count()` 统计成功编译次数。缓冲、绑定组、staging 与 uniform 仍为每实例独立；队列共享，但不共享任何可变缓冲。

`runtime::scoped`：wgpu 24 的错误作用域按设备而非线程划分，而设备现已进程级共享，因此每个线程最外层的 `scoped` 获取进程级锁（同线程嵌套通过线程局部深度重入），防止其他线程的 push/pop 截获本线程错误。

`GpuVoxelPipeline::shared_device()` 与 `GpuShellS2Pipeline::with_device(Arc<SharedGpuDevice>)` 取代原 `device_queue()`/`with_device(device, queue)`。`release_shared_gpu_devices()` 清空缓存；CLI 在子命令返回后调用，使逻辑设备仍在进程退出前销毁。`try_init_gpu` 现在探测共享设备，不再每次创建并丢弃设备；`request_adapter_device` 已删除。

测试：`context::tests::pipeline_cache_reuses_success_and_never_caches_failure` 与单测试集成二进制 `tests/gpu_device_cache_tests.rs`（各类构造器重复四轮并 8 线程并发：1 个设备、6 次编译；无效选择器两次报错且不创建设备；`destroy` 后剔除、仅重建一个设备并各族重编译一次）。

### f32 射线奇偶性认证（PERF-05）

设计与误差界推导见 `algorithms/s2-two-point-correlation.md` 的“GPU 射线奇偶性的 f32 认证”。函数契约：

| 函数 | 源码 | 契约 |
|---|---|---|
| `GpuCertificationStats { queries, uncertain, list_regrowths, cpu_recompute_seconds }` | `src/gpu/certify.rs` | 每个管线的累计计数。MC 查询 = 有效样本；体素查询 = 单元。`recompute_ratio()` = uncertain/queries（无查询时为 0），`describe()` 生成一行日志，`accumulate(&other)` 累加计数。以 `rustmspt::gpu::GpuCertificationStats` 重新导出。 |
| `f32_at_least(x: f64) -> f32` / `f32_at_most(x: f64) -> f32` | `src/gpu/certify.rs` | 不小于 x 的最小 f32 / 不大于 x 的最大 f32。对 f32 点 p，`p < f32_at_least(L)` 当且仅当 `p < L`，`p > f32_at_most(H)` 当且仅当 `p > H`。 |
| `CertReference::new(mesh, origin)` | `src/gpu/certify.rs:81` | 在 f64 中将每个顶点平移 `origin`；`mesh_lo`/`mesh_hi` 为 CPU 的 `bb.min - 1e-9`/`bb.max + 1e-9` 向外舍入到 f32（空网格为 +inf/-inf）。 |
| `CertReference::params_tail(flags) -> Vec<u8>` | `src/gpu/certify.rs:118` | 32 字节 WGSL 尾部：`mesh_lo`、`flags`（位 0 = 记录全部有效 MC 样本，仅测试）、`mesh_hi`、填充。 |
| `CertReference::classify(points) -> Vec<bool>` | `src/gpu/certify.rs:133` | 在平移坐标系中对精确 f32 点做 CPU f64 判定，串行；少于 16 点用 `point_inside_mesh`，否则用预处理 BVH 查询。 |
| `GpuS2Pipeline::certification_stats()` | `src/gpu/s2.rs:566` | MC 计数副本。 |
| `GpuS2Pipeline::dispatch_batch(dispatch, readback) -> Result<u32, String>` | `src/gpu/s2.rs:852` | 清零列表计数器、dispatch 一个半径批次、复制 hit/valid 部分和及整条列表到 staging，返回报告的不确定数（可能超过容量）。 |
| `GpuS2Pipeline::resolve_uncertain(entries, spr, radii, out)` | `src/gpu/s2.rs:923` | 解码 7 字记录、判定 p 与 q，二者都在内时为半径 `id / spr` 加一次命中；记录不在批次内则报错。更新计数。 |
| `uncertain_buffers(device, entries)` | `src/gpu/s2.rs:963` | 分配 `4 + 28*entries` 字节的列表（STORAGE/COPY_SRC/COPY_DST）与可映射 staging。 |
| `GpuVoxelPipeline::certification_stats()` | `src/gpu/voxel.rs:629` | 体素计数副本。 |
| `GpuVoxelPipeline::dispatch_voxels(dispatch, bytes, count_only) -> Result<u32, String>` | `src/gpu/voxel.rs:521` | 清零计数器、dispatch 体素化与（计数模式）归约、复制结果和列表，返回报告的不确定数。 |
| `GpuVoxelPipeline::voxelize_impl`（已修改） | `src/gpu/voxel.rs` | 将列表预设为 `voxel_uncertain_entries(cells)`，溢出时扩容并重新 dispatch，判定不确定中心，加入完整结果或计数，并把在内单元写 `1` 到驻留占据场。 |
| `GpuVoxelPipeline::new_with_shader(mesh, bbox, source)` | `src/gpu/voxel.rs:117` | 共享构造函数；`new` 传入认证着色器。 |
| `voxel_center(i, pitch) -> f32` | `src/gpu/voxel.rs:50` | `(i as f32 + 0.5) * pitch`，与着色器逐位一致。 |
| `voxel_uncertain_buffers(device, entries)` / `occupancy_usage()` | `src/gpu/voxel.rs` | `4 + 4*entries` 字节的体素列表/staging；占据场用途新增 `COPY_DST` 以便修补。 |
| `mc_uncertain_bytes(entries)` / `MC_UNCERTAIN_INITIAL` / `MC_PARAMS_BYTES` | `src/compute/mc_memory.rs` | MC 列表字节（初始 1024 条）、608 字节参数。`mc_evaluation_peak` 新增 `uncertain_capacity` 参数。 |
| `voxel_uncertain_entries(cells)` / `exact_cert_bytes(cells)` | `src/compute/exact_memory.rs` | 规划体素列表 `max(1024, cells/64)` 及其逻辑字节（列表 + staging + 2×32 字节参数尾部）。 |

`release_output_capacity`（MC）与 `release_grid_capacity`（体素）也会把不确定列表缩回初始大小。测试：`gpu::s2::certification_tests`（对抗性样例在原点与 1e9 偏移、两个 seed、分批半径下，MC 计数等于对相同 GPU 点的 CPU 求值；强制列表溢出）与 `gpu::voxel::certification_tests`（占据、计数和驻留场逐单元等于 CPU 参考；两种模式下强制溢出）；二者均断言冻结的未认证着色器（`tests/fixtures/*_uncertified.wgsl`）在 3e-7 薄板上出错。两个模块中被忽略的 release 基准 `certification_overhead_benchmark` 比较认证与未认证运行。

### MC 半径分批（PERF-05/08）

`calculate_s2_gpu_counts` 以每批最多 `MC_RADIUS_BATCH = 128` 个半径循环：写入 `radius_base`，派发 `batch_radii * ceil(samples/256)` 个工作组，回读该前缀并合并到全局结果。随机数按全局编号 `(radius_base + slot) * samples + sample` 生成，计数与批大小无关（`radius_batches_match_single_batch_counts` 比较批大小 1/7/13/64/128 及 r_max 127 与 300）。输出/staging 容量与 `mc_evaluation_peak` 按单批计算。CPU 合并对每个 u32 部分和只读一次，复杂度 O((r_max+1)·ceil(samples/256))，回读每部分和 8 字节；由于已与回读量线性相关，未增加 GPU 二级归约。冻结的逐样本参考着色器仅支持单批。`dispatch_plan` 新增 `batch` 参数；`pack_params` 新增 `radius_base`（原 `_pad0` 字）。

### SA 局部三角形上传（PERF-10）

`GpuS2Pipeline::update_mesh` 在主机上构建完整三角形缓冲，与驻留内容的主机影子副本（`changed_face_runs`）逐位比较，仅用 `queue.write_buffer` 子区间写入变化的面区间（间隔不超过 8 个面时合并）。三角形数变化、缓冲扩容、超过 `MAX_UPLOAD_RUNS = 64` 段或变化面超过一半时回退为整体写入。由于比较对象是实际驻留内容而非调用者上一候选，拒绝恢复、迁移及多岛交替使用共享实例时均正确。`upload_stats()`/`GpuUploadStats` 记录整体/局部/未变化次数与字节。`partial_triangle_upload_matches_full_upload` 在每一步回读 GPU 缓冲逐字比较，并与新建实例的固定种子 MC 计数比较。主机端比较仍为 O(面数)，仅减少传输字节。
