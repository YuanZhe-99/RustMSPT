# 核心与计算参考

本页文档记录了 crate 根与入口点（`src/lib.rs`、`src/main.rs`、`src/error.rs`、`src/types.rs`）、独立诊断二进制文件 `src/bin/precision_test.rs`，以及 `src/compute/` 中的 CPU/GPU 后端选择层（`mod.rs`、`backend.rs`、`policy.rs`）。

## 索引

| 条目 | 位置 | 摘要 |
|---|---|---|
| `RustMsptError` | `src/error.rs:4` | 覆盖 I/O、YAML、TIFF、配置、网格及 GPU 失败的 crate 级错误枚举。 |
| `Result` | `src/error.rs:27` | 类型别名 `Result<T> = std::result::Result<T, RustMsptError>`，在整个 crate 中使用。 |
| `Cli` | `src/main.rs:19` | 顶层 clap CLI 结构体，包装一个 `Commands` 子命令。 |
| `Commands` | `src/main.rs:25` | 7 个 CLI 子命令（Forge/Measure/Optimize/Pack/Scale/Crop/SplitFilter）的枚举。 |
| `default_config_path` | `src/main.rs:85` | 在 `data/input/` 下构建默认配置路径。 |
| `pick_config_path` | `src/main.rs:90` | 选择用户提供的配置路径，或回退到默认路径。 |
| `main`（main.rs） | `src/main.rs:100` | CLI 入口点：解析参数、加载配置、应用覆盖项、运行所选流水线。 |
| `main`（precision_test.rs） | `src/bin/precision_test.rs:5` | 独立诊断二进制文件，比较 CPU 精确法、CPU 蒙特卡洛法与 GPU 蒙特卡洛法之间 S2 计算的精度/性能。 |
| `Vec3` | `src/types.rs:2` | 由 `f64` 分量组成的三维向量，带基本向量代数方法。 |
| `Vec3::new` | `src/types.rs:10` | 由 x/y/z 分量构造一个向量。 |
| `Vec3::add` | `src/types.rs:15` | 向量加法。 |
| `Vec3::sub` | `src/types.rs:20` | 向量减法。 |
| `Vec3::scale` | `src/types.rs:25` | 标量乘法。 |
| `Vec3::dot` | `src/types.rs:30` | 点积。 |
| `Vec3::cross` | `src/types.rs:35` | 叉积。 |
| `BoundingBox` | `src/types.rs:45` | 由 `min`/`max` 角点定义的轴对齐包围盒。 |
| `BoundingBox::from_size` | `src/types.rs:52` | 由原点到给定尺寸构建一个包围盒。 |
| `BoundingBox::size` | `src/types.rs:60` | 返回包围盒的边长。 |
| `BoundingBox::volume` | `src/types.rs:65` | 返回包围盒的（非负）体积。 |
| `BoundingBox::contains_point` | `src/types.rs:71` | 判断某点是否位于包围盒内部或边界上。 |
| `Triangle` | `src/types.rs:82` | 引用网格顶点数组的索引三元组 `(a, b, c)`。 |
| `Mesh` | `src/types.rs:89` | 顶点/面容器：`vertices: Vec<Vec3>`、`faces: Vec<Triangle>`。 |
| `Mesh::empty` | `src/types.rs:96` | 构造一个空网格。 |
| `Mesh::is_empty` | `src/types.rs:104` | 若网格没有顶点或没有面，则为真。 |
| `AccelerationMode` | `src/compute/backend.rs:5` | 请求的计算模式枚举：`Auto`（默认）、`Cpu`、`Gpu`。 |
| `AccelerationMode::fmt`（Display） | `src/compute/backend.rs:12` | 将模式格式化为 `"auto"`/`"cpu"`/`"gpu"`。 |
| `BackendCaps` | `src/compute/backend.rs:23` | 所选后端上报的能力（名称、GPU 支持情况、缓冲区大小限制）。 |
| `ComputeBackend` | `src/compute/backend.rs:31` | 实际选定的具体后端枚举：`Cpu`，或 `Gpu { .. }`（特性门控）。 |
| `ComputeBackend::name` | `src/compute/backend.rs:43` | 人类可读的后端名称。 |
| `ComputeBackend::is_gpu` | `src/compute/backend.rs:52` | 该后端是否为 GPU 后端。 |
| `ComputeBackend::caps` | `src/compute/backend.rs:64` | 返回该后端的 `BackendCaps` 摘要。 |
| `ComputeBackend::fmt`（Display） | `src/compute/backend.rs:87` | 格式化为 `"cpu"` 或 `"gpu/wgpu/{adapter_name}"`。 |
| `FallbackReason` | `src/compute/policy.rs:4` | 记录所请求的后端为何无法满足，以及实际改用了什么。 |
| `BackendSelection` | `src/compute/policy.rs:10` | 后端选择结果：所选 `ComputeBackend` 加上可选的 `FallbackReason`。 |
| `select_backend` | `src/compute/policy.rs:21` | 计算密集型流水线所使用的中心化 CPU/GPU/Auto 分发策略。 |

## 模块职责：`lib.rs`

`src/lib.rs` 是 crate 根。它声明了顶层公共模块（`compute`、`config`、`error`、`geometry`、`io`、`pipeline`、`types`），在 `gpu` cargo 特性下有条件地声明 `gpu` 模块，并在 crate 根重新导出 `error` 中的 `Result`/`RustMsptError`（`rustmspt::Result`、`rustmspt::RustMsptError`）。它自身不包含任何函数。

## 模块职责：`error.rs`

`src/error.rs` 定义了库中（几乎）每一个可能失败的函数所使用的 crate 级错误类型。

- **`RustMsptError`**（`src/error.rs:4`） — 一个由 `thiserror` 派生的枚举，包含以下变体：`Io`（通过 `#[from]` 包装 `std::io::Error`）、`Yaml`（通过 `#[from]` 包装 `serde_yaml::Error`）、`Tiff`（通过 `#[from]` 包装 `tiff::TiffError`）、`InvalidConfig(String)`、`InvalidMesh(String)`、`NotAvailable(String)` 和 `Gpu(String)`。带 `#[from]` 的变体让调用处的 `?` 能够将 `io::Error`/`serde_yaml::Error`/`tiff::TiffError` 自动转换为 `RustMsptError`。
- **`Result<T>`**（`src/error.rs:27`） — `std::result::Result<T, RustMsptError>` 的别名，在配置加载、几何、I/O 及流水线代码中作为返回类型使用。

以上两者均非函数，因此按本页的文档范围不给出逐函数条目。

## main.rs

`src/main.rs` 是 `rustmspt` 二进制文件的入口点：一个基于 clap 的 CLI，为所请求的子命令加载 YAML 配置，应用可选的 `--input`/`--output` 路径覆盖项，并运行相应的流水线。

### CLI 结构

`Cli`（`src/main.rs:19`）是顶层 `#[derive(Parser)]` 结构体；它持有单个 `command: Commands` 字段。`Commands`（`src/main.rs:25`）是一个 `#[derive(Subcommand)]` 枚举，包含七个变体，每个都携带相同的三个可选参数：

| 子命令 | 加载的配置结构体 | 默认配置文件 |
|---|---|---|
| `Forge` | `ForgingConfig` | `forge_config.yaml` |
| `Measure` | `MeasurementConfig` | `measure_config.yaml` |
| `Optimize` | `OptimizationConfig` | `optimize_config.yaml` |
| `Pack` | `PackingConfig` | `pack_config.yaml` |
| `Scale` | `ScaleConfig` | `scale_config.yaml` |
| `Crop` | `CropConfig` | `crop_config.yaml` |
| `SplitFilter` | `SplitFilterConfig` | `split_filter_config.yaml` |

每个变体都接受 `--config <PathBuf>`、`--input <PathBuf>` 和 `--output <PathBuf>`，均为可选。`--config` 选择要加载哪个 YAML 文件（参见 `pick_config_path`）；`--input`/`--output` 若存在，则会在流水线运行前覆盖已加载配置对象上对应的路径字段。

#### default_config_path

- **签名：** `fn default_config_path(file_name: &str) -> PathBuf`
- **源码位置：** `src/main.rs:85`
- **用途：** 在 `data/input/` 下构建默认配置文件路径。
- **参数：**
  - `file_name` — 配置文件的基础名（例如 `"pack_config.yaml"`）。
- **返回值：** 等于 `data/input/<file_name>` 的 `PathBuf`。
- **副作用：** 无（纯路径构造；不检查文件是否存在）。

#### pick_config_path

- **签名：** `fn pick_config_path(config: Option<PathBuf>, file_name: &str) -> PathBuf`
- **源码位置：** `src/main.rs:90`
- **用途：** 解析某个子命令要使用的配置路径：若给出了用户提供的 `--config` 值则使用它，否则使用 `data/input/` 下的默认路径。
- **参数：**
  - `config` — 可选的 `--config` CLI 参数。
  - `file_name` — 当 `config` 为 `None` 时传给 `default_config_path` 的回退文件基础名。
- **返回值：** 解析得到的 `PathBuf`。
- **副作用：** 无。

#### main

- **签名：** `fn main() -> anyhow::Result<()>`
- **源码位置：** `src/main.rs:100`
- **用途：** 解析 CLI 参数，为所选子命令加载 YAML 配置，应用 `--input`/`--output` 覆盖项，并运行相应的流水线。
- **参数：** 无（通过 `Cli::parse()` 读取 `std::env::args`）。
- **返回值：** 成功时为 `Ok(())`；若配置加载、路径解析或流水线执行（`Pipeline::run`）失败，则为 `anyhow::Error`。
- **副作用：** 从磁盘读取 YAML 配置文件；针对每个子命令，构造对应的流水线结构体（`ForgePipeline`、`MeasurePipeline`、`OptimizePipeline`、`PackPipeline`、`ScalePipeline`、`CropPipeline`、`SplitFilterPipeline`）并调用 `.run()`，后者继而读取输入网格/图像文件，并按配置指示写入输出文件。通过底层流水线向标准输出打印进度。
- **说明：** 这是 `rustmspt` 二进制文件唯一的入口点。`match cli.command { ... }` 代码块是扁平的分发结构：每个分支针对其子命令重复相同的三步模式（解析路径 → 加载并修改配置 → 构造并运行流水线）；除 `pick_config_path` 外，各分支之间没有共享的辅助函数。

## bin/precision_test.rs

`src/bin/precision_test.rs` 作为一个独立的 Cargo 二进制目标（`precision_test`）构建，区别于 `rustmspt` 库与 CLI。它是一个手动诊断工具，用于比较三种方法在 S2（两点相关函数）计算精度与性能上的差异：CPU 精确法（占据网格 + FFT）、CPU 蒙特卡洛法（占据网格采样）以及 GPU 蒙特卡洛法（连续光线投射，仅在启用 `gpu` 特性时构建）。它不受测试套件或任何库流水线的调用。

#### main

- **签名：** `fn main()`
- **源码位置：** `src/bin/precision_test.rs:5`
- **用途：** 加载一个固定的样本网格，计算其体积分数与包围盒，随后在若干体素间距下、以 `"exact"` 和 `"monte_carlo"` 两种 CPU 方法运行 `calculate_s2`（并在启用 `gpu` 特性时通过 `GpuS2Pipeline::calculate_s2_gpu` 运行），打印 S2 曲线采样值、相对参考值的 L2 偏差，以及（对于 GPU）耗时。
- **参数：** 无。
- **返回值：** `()`。若 `data/input/particles.stl` 无法加载或没有包围盒，会通过 `.expect(...)` 触发 panic。
- **副作用：** 从磁盘读取 `data/input/particles.stl`；向标准输出打印诊断表格；当启用 `gpu` 特性时，尝试初始化一个 GPU S2 流水线（`GpuS2Pipeline::new`），若失败则打印错误并提前返回，而不是触发 panic。
- **说明：** 该二进制文件硬编码了输入路径（`data/input/particles.stl`）、间距扫描范围（`[0.5, 1.0, 2.0, 4.0, 8.0]`）、半径计数（`r_max = 10`）以及蒙特卡洛采样数（`n = 50000`）——它是为开发过程中手动运行而设计的，并非任何自动化流水线的一部分。它在自身打印的输出中记录了三种方法之间的关键方法学差异：GPU 蒙特卡洛法与体素间距无关（不做体素化），而 CPU 蒙特卡洛法和 CPU 精确法都依赖于在给定间距下构建的体素占据网格。

## types.rs

`src/types.rs` 定义了 crate 的基本几何值类型：`Vec3`（三维向量）、`BoundingBox`（轴对齐包围盒）、`Triangle`（索引三元组）以及 `Mesh`（顶点/面容器）。这些类型在 `geometry`、`pipeline`、`io` 及 `compute` 中被广泛使用。

### Vec3

| 字段 | 类型 | 描述 |
|---|---|---|
| `x` | `f64` | X 分量。 |
| `y` | `f64` | Y 分量。 |
| `z` | `f64` | Z 分量。 |

`Vec3` 派生了 `Debug, Clone, Copy, PartialEq`；所有方法都按值获取 `self`（它是 `Copy` 的）。

#### new

`pub fn new(x: f64, y: f64, z: f64) -> Self` — `src/types.rs:10`。由三个分量构造一个向量。无副作用。

#### add

`pub fn add(self, other: Self) -> Self` — `src/types.rs:15`。逐分量返回 `self + other`。无副作用。

#### sub

`pub fn sub(self, other: Self) -> Self` — `src/types.rs:20`。逐分量返回 `self - other`。无副作用。

#### scale

`pub fn scale(self, factor: f64) -> Self` — `src/types.rs:25`。返回 `self` 按 `factor` 缩放后的结果（每个分量相乘）。无副作用。

#### dot

`pub fn dot(self, other: Self) -> f64` — `src/types.rs:30`。返回点积 `x*x' + y*y' + z*z'`。无副作用。

#### cross

`pub fn cross(self, other: Self) -> Self` — `src/types.rs:35`。返回叉积 `self × other`，即与两个输入向量都正交的向量。无副作用。

### BoundingBox

| 字段 | 类型 | 描述 |
|---|---|---|
| `min` | `Vec3` | 最小角点。 |
| `max` | `Vec3` | 最大角点。 |

`BoundingBox` 派生了 `Debug, Clone, Copy, PartialEq`。

#### from_size

- **签名：** `pub fn from_size(size: Vec3) -> Self`
- **源码位置：** `src/types.rs:52`
- **用途：** 构建一个从原点 `(0,0,0)` 延伸到 `size` 的轴对齐包围盒。
- **参数：** `size` — 包围盒沿各轴的范围。
- **返回值：** `min = (0,0,0)`、`max = size` 的 `BoundingBox`。
- **副作用：** 无。
- **说明：** 不校验 `size` 各分量是否非负；负分量会产生一个反转的包围盒（参见 `volume`，它会将负的范围截断为 0）。

#### size

`pub fn size(&self) -> Vec3` — `src/types.rs:60`。返回 `max - min`（包围盒的边长）。无副作用。

#### volume

- **签名：** `pub fn volume(&self) -> f64`
- **源码位置：** `src/types.rs:65`
- **用途：** 计算包围盒的体积。
- **参数：** 无（`&self`）。
- **返回值：** 三条边长的乘积，若任一边长为负则截断为 `0.0`——因此反转或退化的包围盒得到的是 `0.0` 而非负体积。
- **副作用：** 无。

#### contains_point

`pub fn contains_point(&self, p: Vec3) -> bool` — `src/types.rs:71`。若 `p` 在三个坐标轴上均落于 `[min, max]`（含边界）范围内，则返回 `true`（边界点也算包含在内）。无副作用。

### Triangle

| 字段 | 类型 | 描述 |
|---|---|---|
| `a` | `usize` | 所属 `Mesh::vertices` 中第一个顶点的索引。 |
| `b` | `usize` | 第二个顶点的索引。 |
| `c` | `usize` | 第三个顶点的索引。 |

`Triangle` 派生了 `Debug, Clone, PartialEq`，不携带任何方法；它纯粹作为 `Mesh` 的字段类型存在。

### Mesh

| 字段 | 类型 | 描述 |
|---|---|---|
| `vertices` | `Vec<Vec3>` | 所有顶点位置。 |
| `faces` | `Vec<Triangle>` | 引用 `vertices` 的三角形。 |

`Mesh` 派生了 `Debug, Clone, PartialEq`。

#### empty

`pub fn empty() -> Self` — `src/types.rs:96`。构造一个 `vertices` 和 `faces` 均为空向量的 `Mesh`。无副作用。

#### is_empty

`pub fn is_empty(&self) -> bool` — `src/types.rs:104`。若该网格没有顶点**或**没有面（即 `vertices.is_empty() || faces.is_empty()`，而非严格的“两者皆空”检查），则返回 `true`。无副作用。

## compute/mod.rs

`src/compute/mod.rs` 是 `compute` 包的模块枢纽：它声明了 `backend` 与 `policy` 子模块，并在 `compute::` 路径下重新导出 `AccelerationMode`、`BackendCaps`、`ComputeBackend`（来自 `backend`）以及 `select_backend`（来自 `policy`）。它自身不包含任何函数。

该文件还包含一个 `#[cfg(test)] mod tests` 代码块，内有六个单元测试（`cpu_backend_is_not_gpu`、`auto_falls_back_for_small_workload`、`default_mode_is_auto`、`display_formats_correctly`、`cpu_caps_show_no_gpu_support`，以及特性门控的 `gpu_adapter_detection`），用于验证 `select_backend` 的 CPU/Auto/Gpu 分支、`AccelerationMode`/`ComputeBackend` 的 `Display` 格式化以及默认模式行为。这些是测试代码，不是生产函数，此处不逐一记录。

## compute/backend.rs

该文件定义了后端*类型*层级：`AccelerationMode`（调用方所请求的）、`ComputeBackend`（实际选定的）以及 `BackendCaps`（所选后端上报的能力）。`compute/policy.rs` 中的 `select_backend` 是将 `AccelerationMode` 转化为 `ComputeBackend` 的函数。

### AccelerationMode

`AccelerationMode`（`src/compute/backend.rs:5`）是一个 `#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Default)]` 枚举，有三个变体：`Auto`（`#[default]`）、`Cpu`、`Gpu`。它从小写字符串反序列化（`#[serde(rename_all = "lowercase")]`），因此配置 YAML 可以指定 `acceleration: auto|cpu|gpu`。

#### fmt (Display for AccelerationMode)

`impl fmt::Display for AccelerationMode` — `src/compute/backend.rs:12`。将 `Auto`/`Cpu`/`Gpu` 格式化为小写字符串 `"auto"`/`"cpu"`/`"gpu"`。无副作用。

### BackendCaps

`BackendCaps`（`src/compute/backend.rs:23`） — `#[derive(Debug, Clone)]`，上报所选后端的能力：

| 字段 | 类型 | 描述 |
|---|---|---|
| `name` | `String` | 后端或 GPU 适配器名称（CPU 后端为 `"cpu"`）。 |
| `supports_gpu` | `bool` | 该后端是否为 GPU 支持。 |
| `max_buffer_size` | `u64` | 适配器的最大缓冲区大小（字节）（CPU 为 `0`）。 |
| `max_storage_buffer_binding_size` | `u64` | 适配器的最大存储缓冲区绑定大小（字节）（CPU 为 `0`）；`select_backend` 用它来强制执行所配置的 GPU 内存限制。 |

### ComputeBackend

`ComputeBackend`（`src/compute/backend.rs:31`） — 具体的、已解析后端的 `#[derive(Debug, Clone)]` 枚举：

- `Cpu` — 始终可用。
- `Gpu { adapter_name: String, max_buffer_size: u64, max_storage_buffer_binding_size: u64 }`

> **特性门控：** `Gpu` 变体，以及 `name`、`is_gpu`、`caps` 和 `Display` 实现中与 GPU 相关的匹配分支，仅在 crate 以启用 `gpu` cargo 特性构建时才存在。若未启用，`ComputeBackend` 实际上就是一个只有 `Cpu` 的类单元枚举，`is_gpu()` 始终返回 `false`。

#### name

`pub fn name(&self) -> &str` — `src/compute/backend.rs:43`。对 `ComputeBackend::Cpu` 返回 `"cpu"`，对 `ComputeBackend::Gpu`（特性门控）返回 GPU 适配器的名称。无副作用。

#### is_gpu

`pub fn is_gpu(&self) -> bool` — `src/compute/backend.rs:52`。仅当启用 `gpu` 特性且为 `Gpu` 变体时返回 `true`；若未启用该特性，则无条件返回 `false`。无副作用。

#### caps

- **签名：** `pub fn caps(&self) -> BackendCaps`
- **源码位置：** `src/compute/backend.rs:64`
- **用途：** 生成描述该后端能力的 `BackendCaps` 摘要。
- **参数：** 无（`&self`）。
- **返回值：** 对于 `Cpu`：`BackendCaps { name: "cpu", supports_gpu: false, max_buffer_size: 0, max_storage_buffer_binding_size: 0 }`。对于 `Gpu`（特性门控）：由适配器已存储的字段填充的 `BackendCaps`，`supports_gpu: true`。
- **副作用：** 无（读取已存储的适配器信息；不会再次查询 GPU）。

#### fmt (Display for ComputeBackend)

`impl fmt::Display for ComputeBackend` — `src/compute/backend.rs:87`。将 `Cpu` 格式化为 `"cpu"`；将 `Gpu`（特性门控）格式化为 `"gpu/wgpu/{adapter_name}"`。无副作用。

## compute/policy.rs

这是 crate 的中心化 CPU/GPU/Auto 分发策略。任何可以选择在 GPU 上运行的流水线阶段（S2 相关性计算、堆积碰撞检测等）都调用一次 `select_backend` 来决定使用哪个 `ComputeBackend`，而不是各自重复实现 GPU 可用性检测与回退逻辑。

### FallbackReason

`FallbackReason`（`src/compute/policy.rs:4`） — `#[derive(Debug, Clone)]`：

| 字段 | 类型 | 描述 |
|---|---|---|
| `requested` | `AccelerationMode` | 调用方最初请求的模式（`Gpu` 或 `Auto`；`Cpu` 永远不会产生回退）。 |
| `reason` | `String` | 说明为何回退到 CPU 的人类可读解释。 |

### BackendSelection

`BackendSelection`（`src/compute/policy.rs:10`） — `#[derive(Debug, Clone)]`，`select_backend` 的返回类型：

| 字段 | 类型 | 描述 |
|---|---|---|
| `backend` | `ComputeBackend` | 实际选定的后端（除非 GPU 初始化成功且未超出限制，否则始终为 `Cpu`）。 |
| `fallback` | `Option<FallbackReason>` | 若所请求的模式无法满足、选择结果回退到 CPU，则为 `Some`；若使用了所请求的后端本身，则为 `None`。 |

#### select_backend

- **签名：** `pub fn select_backend(requested: AccelerationMode, gpu_min_voxels: Option<usize>, gpu_memory_limit_mb: Option<u64>, workload_voxels: usize) -> BackendSelection`
- **源码位置：** `src/compute/policy.rs:21`
- **用途：** 将所请求的 `AccelerationMode`（`Cpu`/`Gpu`/`Auto`）连同工作负载/配置参数，解析为一个具体的 `BackendSelection`，其中应用了 GPU 可用性、内存限制及工作负载规模检查。
- **参数：**
  - `requested` — 调用方/配置所请求的模式。
  - `gpu_min_voxels` — 仅用于 `Auto` 模式：低于该体素数的工作负载不会尝试使用 GPU；若为 `None` 则默认为 `250_000`。
  - `gpu_memory_limit_mb` — 对 GPU 内存的可选上限（单位 MB）；若适配器的 `max_storage_buffer_binding_size` 小于该限制，则选择结果回退到 CPU。当 `gpu` 特性被禁用时不使用（在该配置下标记为 `#[allow(unused_variables)]`）。
  - `workload_voxels` — 当前工作负载的体素数，在 `Auto` 模式下与 `gpu_min_voxels` 比较。
- **返回值：** 一个 `BackendSelection`：
  - `requested == Cpu`：始终为 `{ backend: Cpu, fallback: None }`。
  - `requested == Gpu`：在启用 `gpu` 特性的情况下，尝试 `crate::gpu::try_init_gpu()`；成功后，将 `gpu_memory_limit_mb` 与适配器的 `max_storage_buffer_binding_size` 比较（若超出限制则回退到 CPU 并附带一个 `FallbackReason`），否则返回 `{ backend: Gpu { .. }, fallback: None }`。若 GPU 初始化失败，或 `gpu` 特性被禁用，则返回 `{ backend: Cpu, fallback: Some(FallbackReason { requested: Gpu, reason: "GPU init failed: ..." | "cargo feature 'gpu' is not enabled" }) }`。
  - `requested == Auto`：首先将 `workload_voxels` 与 `gpu_min_voxels.unwrap_or(250_000)` 比较；若低于阈值，立即返回 CPU，并附带一个引用体素数的回退原因，完全不尝试 GPU 初始化。否则，遵循与 `Gpu` 分支相同的 GPU 初始化/内存限制逻辑（生成的任何 `FallbackReason` 中 `requested: Auto`）。
- **副作用：** 当启用 `gpu` 特性、且 `requested` 为 `Gpu`，或为 `Auto` 且工作负载达到或超过体素阈值时，会调用 `crate::gpu::try_init_gpu()`，该调用可能会初始化一个 wgpu 适配器——在别处已记录为一个可能较为昂贵的首次调用（适配器/设备枚举与创建）。
- **说明：** `Auto` 模式的阈值检查发生在任何 GPU 探测*之前*，因此即使有 GPU 可用，小型工作负载也永远不会承担 GPU 初始化的开销。当未编译 `gpu` 特性时，`Gpu` 与 `Auto` 分支始终解析为 `Cpu`，回退原因为 `"cargo feature 'gpu' is not enabled"`，无论 `workload_voxels`（对于 `Gpu`）如何，或在阈值检查之后（对于 `Auto`）。`gpu_memory_limit_mb` 检查仅在 `max_storage_buffer_binding_size > 0` 时才会触发，以避免在报告该字段为 `0` 的适配器上产生误报式回退。
