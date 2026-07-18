# GPU Module (`src/gpu/`)

> **特性门控：** 整个 `src/gpu/` 模块需要 `cargo build --features gpu`。以下文档中记录的所有函数和类型在默认（仅 CPU）构建中均不可用。

> **锚点说明：** 本文件中的标题使用裸 `Struct::method` 形式（例如 `#### GpuS2Pipeline::new`）。根据所用的 Markdown 渲染器不同，此类标题自动生成的锚点可能是 `#gpus2pipeline-new` 或类似形式（渲染器对 `::` 的转义规则并不一致）。如果来自其他文档的交叉链接无法解析，请在本页中搜索标题文本，而不要依赖锚点标点符号。

本模块实现了基于 [`wgpu`](https://wgpu.rs/) 与 WGSL 计算着色器构建的 GPU 加速计算流水线。它提供四条相互独立的流水线——蒙特卡洛 S2 相关函数、精确壳层对 S2 相关函数、网格体素化，以及体数据旋转裁剪——外加供计算后端选择策略使用的共享适配器/设备启动逻辑（`context.rs`）。

## 索引

| 条目 | 位置 | 摘要 |
|---|---|---|
| `GpuContext` | `src/gpu/context.rs:3` | 成功完成 GPU 初始化后，持有适配器名称与缓冲区大小能力信息。 |
| `GpuContext::caps` | `src/gpu/context.rs:11` | 返回描述此 GPU 上下文的 `BackendCaps`。 |
| `GpuInitError` | `src/gpu/context.rs:22` | 包装 GPU 初始化失败消息的错误类型。 |
| `GpuInitError`（`Display` 实现） | `src/gpu/context.rs:24` | 格式化错误消息。 |
| `try_init_gpu` | `src/gpu/context.rs:38` | 探测 wgpu 适配器/设备并返回 `GpuContext`；供 `compute::policy::select_backend` 使用。 |
| `GpuS2Pipeline` | `src/gpu/s2.rs:10` | 蒙特卡洛 S2 两点相关函数的 GPU 流水线状态。 |
| `build_triangle_buffer`（s2.rs） | `src/gpu/s2.rs:27` | 为 S2 蒙特卡洛流水线构建归一化的 `f32` 三角形位置缓冲区。 |
| `pack_params` | `src/gpu/s2.rs:48` | 将蒙特卡洛 S2 着色器参数打包为与 WGSL `Params` 布局匹配的字节缓冲区。 |
| `GpuS2Pipeline::new` | `src/gpu/s2.rs:95` | 初始化 wgpu 设备与蒙特卡洛 S2 计算流水线。 |
| `GpuS2Pipeline::update_mesh` | `src/gpu/s2.rs:266` | 无需重建流水线即可为新网格重新上传三角形数据。 |
| `GpuS2Pipeline::ensure_output_capacity` | `src/gpu/s2.rs:287` | 若调用次数超出当前容量，则扩容输出缓冲区。 |
| `GpuS2Pipeline::calculate_s2_gpu` | `src/gpu/s2.rs:311` | 针对所有半径分派蒙特卡洛 S2 内核并回读结果。 |
| `OffsetEntry` | `src/gpu/s2_shell.rs:6` | 与 WGSL 布局匹配的打包 `(radius_idx, dx, dy, dz)` 壳层偏移记录。 |
| `GpuShellS2Pipeline` | `src/gpu/s2_shell.rs:13` | 精确壳层对 S2 计算的 GPU 流水线状态。 |
| `build_offset_buffer` | `src/gpu/s2_shell.rs:30` | 将 `(radius_idx, [dx,dy,dz])` 元组转换为 `OffsetEntry` 记录。 |
| `GpuShellS2Pipeline::new` | `src/gpu/s2_shell.rs:48` | 初始化 wgpu 设备与壳层 S2 计算流水线。 |
| `GpuShellS2Pipeline::compute_s2_shell` | `src/gpu/s2_shell.rs:135` | 在占据网格上分派精确壳层对计数并回读 S2(r)。 |
| `GpuVoxelPipeline` | `src/gpu/voxel.rs:5` | 网格体素化的 GPU 流水线状态。 |
| `build_triangle_buffer`（voxel.rs） | `src/gpu/voxel.rs:17` | 为体素化流水线构建归一化的 `f32` 三角形位置缓冲区（与 `s2.rs` 中的实现相互独立）。 |
| `GpuVoxelPipeline::new` | `src/gpu/voxel.rs:37` | 初始化 wgpu 设备与体素化计算流水线。 |
| `GpuVoxelPipeline::voxelize` | `src/gpu/voxel.rs:116` | 分派光线投射体素化并回读占据网格。 |
| `GpuVolumeTransformPipeline` | `src/gpu/volume_transform.rs:5` | 体数据旋转裁剪的 GPU 流水线状态。 |
| `GpuVolumeTransformPipeline::new` | `src/gpu/volume_transform.rs:23` | 初始化 wgpu 设备与体数据变换计算流水线。 |
| `GpuVolumeTransformPipeline::rotate_and_crop` | `src/gpu/volume_transform.rs:145` | 分派旋转/裁剪/重采样内核并回读变换后的体数据。 |

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

**`Display` 实现**（`src/gpu/context.rs:24`）：原样写出被包装的消息，即 `write!(f, "{}", self.0)`。

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
- **说明：** 此处获得的设备/队列（`_device`、`_queue`）在读取适配器/设备限制之后就故意不再使用——本函数是一个**能力探测器**，而非流水线构造函数。下文四个流水线构造函数（`GpuS2Pipeline::new`、`GpuShellS2Pipeline::new`、`GpuVoxelPipeline::new`、`GpuVolumeTransformPipeline::new`）各自独立执行自己的适配器/设备请求，**不会**复用 `try_init_gpu` 返回的上下文；目前只有 `GpuS2Pipeline::new` 会遵循 `RUSTMSPT_GPU_DEVICE`（见下文说明）——其余三个流水线构造函数始终使用 `wgpu::PowerPreference::default()`，不做适配器过滤。

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

模块级常量：`WORKGROUP_SIZE: u32 = 256`，`MAX_RADII: usize = 128`（着色器 `Params.radii` 数组固定大小为 128 项；单次调用请求超过 128 个半径会被静默截断——见 `pack_params`）。

#### build_triangle_buffer (s2.rs)

- **签名：** `fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32>`
- **源码位置：** `src/gpu/s2.rs:27`
- **用途：** 将网格的三角形顶点位置展平为适合直接上传作为 wgpu 存储缓冲区的 `f32` 缓冲区，坐标经过偏移，使包围盒最小值位于原点。
- **参数：**
  - `mesh: &Mesh` —— 源网格（顶点 + 面）。
  - `bbox: BoundingBox` —— 用于计算坐标原点偏移量（`bbox.min`）的包围盒。
- **返回值：** `Vec<f32>`，每个三角形 9 个浮点数（3 个顶点 × 3 个分量），每个顶点坐标存储为 `(coord as f32) - bbox.min.<axis> as f32`。
- **副作用：** 无（纯函数）。
- **说明：** 归一化到包围盒原点使 GPU 侧的 `f32` 坐标保持较小（大致为 `0..size`，而不是潜在的较大绝对值），从而提升 GPU 上的浮点精度。这是一个私有的、模块局部的辅助函数——`voxel.rs` 定义了一份独立的、文本上近乎相同的副本（见下文）；两者并未共享，以避免在原本自成一体的流水线之间引入跨模块依赖。

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
- **源码位置：** `src/gpu/s2.rs:95`
- **用途：** 初始化 wgpu 设备/队列，将 `s2_monte_carlo.wgsl` 着色器编译为计算流水线，并预先上传网格的归一化三角形数据。
- **参数：**
  - `mesh: &Mesh` —— 立即上传其三角形的网格。
  - `bbox: BoundingBox` —— 用于坐标归一化的包围盒（参见 `build_triangle_buffer`）。
- **返回值：** 成功时返回 `Ok(GpuS2Pipeline)`；`Err(String)` 描述失败原因（无适配器、适配器过滤器不匹配，或设备请求失败）。
- **副作用：** 执行一次完整的 wgpu 适配器/设备请求（通过 `pollster::block_on` 阻塞），编译着色器模块，创建绑定组布局/流水线布局/计算流水线，并分配加上传三角形、参数与输出缓冲区。输出缓冲区预先按 `MAX_RADII * 40_000` 次调用（`max_invocations`）分配大小，因此此构造函数会预先执行一次相对较大的 GPU 分配。
- **说明：** 此构造函数复制了 `try_init_gpu` 的适配器选择逻辑（包括 `RUSTMSPT_GPU_DEVICE` 索引/子串过滤），而不是直接调用它——两者是相互独立的适配器探测，理论上如果调用之间环境发生变化，`try_init_gpu` 探测到的适配器与该构造函数使用的适配器可能不同（实践中 `RUSTMSPT_GPU_DEVICE` 使得同一进程内的选择是确定性的）。

#### GpuS2Pipeline::update_mesh

- **签名：** `pub fn update_mesh(&mut self, mesh: &Mesh, bbox: BoundingBox)`
- **源码位置：** `src/gpu/s2.rs:266`
- **用途：** 用新网格替换流水线已上传的三角形数据，而无需拆除并重建设备/流水线——用于同一个 `GpuS2Pipeline` 在一批次内跨多个颗粒/网格复用的场景。
- **参数：** `mesh: &Mesh`、`bbox: BoundingBox` —— 与 `new` 语义相同。
- **返回值：** `()`。
- **副作用：** 通过 `queue.write_buffer` 重新上传归一化三角形数据。若新网格的字节大小超过当前 `triangle_buffer` 的容量，则分配一个更大的新缓冲区并替换 `triangle_buffer`；否则原地复用现有缓冲区。更新 `self.num_triangles`。
- **说明：** 由于缓冲区在重新分配时只会增长（从不缩小），用大小不同的网格反复调用此函数是安全的，但会使流水线在其整个生命周期内保留峰值大小的 GPU 内存。

#### GpuS2Pipeline::ensure_output_capacity

- **签名：** `fn ensure_output_capacity(&mut self, invocations: u32)`
- **源码位置：** `src/gpu/s2.rs:287`
- **用途：** 私有辅助函数，若某次请求的分派所需的调用槽位数超过当前已分配数量，则扩容 `out_hits_buffer`/`out_valids_buffer`。
- **参数：** `invocations: u32` —— 即将分派的着色器调用总数（`num_radii * samples_per_radius`）。
- **返回值：** `()`。
- **副作用：** 若 `invocations * 4 > out_hits_buffer.size()`，可能重新分配两个输出缓冲区（各自每次调用 4 字节）。旧缓冲区内容会被丢弃（不保留），因为新缓冲区会在下一次分派中被完整写入。
- **说明：** 由 `calculate_s2_gpu` 在每次分派前内部调用；不属于公共 API 的一部分。

#### GpuS2Pipeline::calculate_s2_gpu

- **签名：** `pub fn calculate_s2_gpu(&mut self, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64>`
- **源码位置：** `src/gpu/s2.rs:311`
- **用途：** 在 GPU 上，通过单次涵盖所有半径的分派，计算从 0 到 `r_max` 的每个整数半径上的蒙特卡洛两点相关函数 S2(r)。
- **参数：**
  - `bbox: BoundingBox` —— 用于打包着色器参数的包围盒（仅使用尺寸；原点已经归一化）。
  - `r_max: usize` —— 需要评估的最大半径（包含），半径为整数 `0..=r_max`。
  - `samples: usize` —— 请求的每半径蒙特卡洛样本数；被限制在最小值 200（`samples.max(200)`）。
- **返回值：** 长度为 `r_max + 1` 的 `Vec<f64>`，以半径 `r` 为索引，包含该半径所有样本上 `hits/valid` 的平均值（若该半径没有记录到有效样本，则为 0.0）。
- **副作用：** 生成一个随机 `u32` 种子（`rand::random`），打包并上传参数，调用 `ensure_output_capacity`，构建绑定组，记录并提交一次计算通道（`dispatch_workgroups`，`WORKGROUP_SIZE = 256`），随后将两个输出缓冲区都复制到暂存缓冲区，映射以供 CPU 读取（`device.poll(wgpu::Maintain::Wait)`——一次阻塞等待），并将映射得到的 `u32` 切片归约为返回的 `f64` 向量。
- **说明：** 元素 `r=0`（零半径）在本函数的内部累加逻辑中始终保留为 `0.0`——按照源码注释的说明，调用方应当用已知的体积分数覆盖索引 0 的值，而不要信任 GPU 在零半径处的退化样本。位置计算在 GPU 上以 `f32` 进行，但返回的平均值以 `f64` 存储/返回。

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

模块级常量：`WORKGROUP_SIZE: u32 = 256`，`MAX_OFFSETS: usize = 200_000`（偏移/输出缓冲区的预分配容量；`compute_s2_shell` 通过 `.min(MAX_OFFSETS)` 截断到此上限）。

#### build_offset_buffer

- **签名：** `fn build_offset_buffer(shell_offsets: &[(u32, [isize; 3])]) -> Vec<OffsetEntry>`
- **源码位置：** `src/gpu/s2_shell.rs:30`
- **用途：** 将 CPU 侧的 `(radius_idx, [dx, dy, dz])` 元组（由几何壳层偏移枚举产生）转换为可上传到 GPU 的 `OffsetEntry` 记录。
- **参数：** `shell_offsets: &[(u32, [isize; 3])]` —— 半径索引与 `isize` 位移三元组的配对。
- **返回值：** `Vec<OffsetEntry>`，每个输入元组对应一项，`dx`/`dy`/`dz` 从 `isize` 收窄为 `i32`。
- **副作用：** 无（纯函数）。
- **说明：** 将 `isize` 收窄为 `i32` 时未做边界/溢出检查；位移值预期较小（受体素网格最大半径限制），因此实践中是安全的，但没有做防御性保护。

#### GpuShellS2Pipeline::new

> **特性门控：** 需要 `gpu` cargo 特性；默认构建中不存在。

- **签名：** `pub fn new() -> Result<Self, String>`
- **源码位置：** `src/gpu/s2_shell.rs:48`
- **用途：** 初始化 wgpu 并编译 `s2_shell_pairs.wgsl` 计算流水线。
- **参数：** 无。
- **返回值：** `Ok(GpuShellS2Pipeline)` 或描述适配器/设备失败的 `Err(String)`。
- **副作用：** 执行一次阻塞的 wgpu 适配器/设备请求（不使用 `RUSTMSPT_GPU_DEVICE` 过滤——与 `GpuS2Pipeline::new` 不同，始终使用 `wgpu::PowerPreference::default()`，不做回退强制），编译着色器，构建绑定组/流水线布局，并预分配占据（4 字节——首次使用时增长）、偏移（`MAX_OFFSETS * 16` 字节）、参数（16 字节）以及输出（各 `MAX_OFFSETS * 4` 字节）缓冲区。
- **说明：** 与 `GpuS2Pipeline::new` 和 `GpuVoxelPipeline::new` 不同，此构造函数不接受网格/包围盒参数——占据网格数据在每次调用 `compute_s2_shell` 时提供，而不是在构造时提供。

#### GpuShellS2Pipeline::compute_s2_shell

- **签名：** `pub fn compute_s2_shell(&mut self, occ: &[u32], nx: u32, ny: u32, nz: u32, shell_offsets: &[(u32, [isize; 3])], r_max: usize, _voxel_pitch: f64, vf: f64) -> Vec<f64>`
- **源码位置：** `src/gpu/s2_shell.rs:135`
- **用途：** 通过对每个预先计算的壳层偏移在每个半径上统计有多少体素对在该精确位移下同时被占据（“命中”），以及同时处于边界内/有效（相对于整个占据网格），计算精确（非随机）的 S2(r) 相关函数。
- **参数：**
  - `occ: &[u32]` —— 展平后的体素占据网格（行主序，`nx*ny*nz` 项，1 = 已占据）。
  - `nx, ny, nz: u32` —— 网格维度。
  - `shell_offsets: &[(u32, [isize; 3])]` —— 预先计算的 `(radius_idx, displacement)` 配对（通常来自 CPU 侧壳层枚举）；若超出则截断到 `MAX_OFFSETS`。
  - `r_max: usize` —— `shell_offsets` 中出现的最大半径索引；决定返回向量的长度。
  - `_voxel_pitch: f64` —— 接受但未使用（以 `_` 前缀标记）；体素间距的转换在调用方一侧完成。
  - `vf: f64` —— 已知的体积分数，直接写入 `out[0]` 作为 r=0 的值。
- **返回值：** 长度为 `r_max + 1` 的 `Vec<f64>`：`out[0] = vf`；对于 `r >= 1`，为属于该半径桶的所有偏移上 `hits/valid` 的均值（若该半径没有任何偏移/有效对贡献，则为 0.0）。
- **副作用：** 重新上传占据缓冲区（若大于当前容量则重新分配），上传偏移缓冲区与参数，若 `total_offsets` 超过之前的容量则重新分配两个输出缓冲区，构建绑定组，分派 `total_offsets.div_ceil(WORKGROUP_SIZE)` 个工作组，将输出复制到暂存缓冲区，并执行一次阻塞的 `device.poll(wgpu::Maintain::Wait)` 以便映射供 CPU 回读。
- **说明：** 按半径进行的归约（对每个偏移求和 `hits/valid` 比值并按数量求平均，`shell_sums`/`shell_counts`）在回读之后于 CPU 上完成，而不是在 GPU 上完成——着色器本身只产生原始的逐偏移命中/有效计数。

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

#### GpuVoxelPipeline::new

> **特性门控：** 需要 `gpu` cargo 特性；默认构建中不存在。

- **签名：** `pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String>`
- **源码位置：** `src/gpu/voxel.rs:37`
- **用途：** 初始化 wgpu，编译 `voxelize.wgsl` 计算流水线，并为给定网格上传归一化三角形数据。
- **参数：** `mesh: &Mesh`、`bbox: BoundingBox` —— 待体素化的网格及其包围盒（用于归一化）。
- **返回值：** `Ok(GpuVoxelPipeline)` 或适配器/设备失败时的 `Err(String)`。
- **副作用：** 阻塞的 wgpu 适配器/设备请求（默认电源偏好，不使用 `RUSTMSPT_GPU_DEVICE` 过滤）、着色器编译、绑定组/流水线布局创建、三角形缓冲区分配加上传、参数缓冲区分配（48 字节），以及一个最小的 4 字节占位占据缓冲区（在首次 `voxelize` 调用时增长）。
- **说明：** 与 `GpuShellS2Pipeline::new` 一样，此构造函数不遵循 `RUSTMSPT_GPU_DEVICE`。

#### GpuVoxelPipeline::voxelize

- **签名：** `pub fn voxelize(&mut self, nx: u32, ny: u32, nz: u32, pitch: f32) -> Vec<u32>`
- **源码位置：** `src/gpu/voxel.rs:116`
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
  ) -> Vec<i32>
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

## 四条流水线的共享约定小结

- **不复用共享 `GpuContext`：** `GpuS2Pipeline::new`、`GpuShellS2Pipeline::new`、`GpuVoxelPipeline::new` 与 `GpuVolumeTransformPipeline::new` 各自独立创建自己的 `wgpu::Instance`/适配器/设备，而不是接受来自 `try_init_gpu` 的预初始化 `GpuContext`。目前只有 `GpuS2Pipeline::new` 会读取 `RUSTMSPT_GPU_DEVICE`。
- **阻塞式 GPU 回读：** 每个“分派并读取”方法（`calculate_s2_gpu`、`compute_s2_shell`、`voxelize`、`rotate_and_crop`）都使用 `map_async` 加上随后的 `device.poll(wgpu::Maintain::Wait)`，这会阻塞调用线程直到 GPU 工作与缓冲区映射完成。这些流水线均未提供异步或非阻塞 API。
- **只增长、从不缩小的缓冲区：** 用于复用缓冲区的字段（`triangle_buffer`、`out_hits_buffer`、`occupancy_buffer`、`src_buffer`/`out_buffer` 等）仅在新调用的数据超过当前容量时才会重新分配；它们从不缩容，因此一个流水线实例在其生命周期内的峰值 GPU 内存占用，等于该实例所收到的所有调用中的最大占用。
- **GPU 上用 `f32`，CPU 上用 `f64`：** 全部四条流水线都会将 `f64`/`isize` 的 CPU 侧几何数据收窄为 `f32`/`i32` 以供 GPU 上传，并在回读时将结果放宽回 `f64`/`i32`，这与每个 WGSL 着色器中全程使用 32 位类型的做法相匹配。
