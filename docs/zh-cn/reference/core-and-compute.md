# 核心与计算参考

> **待翻译：** `RenderedImage`、`select_backend_for_workload`、`RustMsptError::Image` 和 `Render` CLI 的详细契约见[英文参考](../../en-us/reference/core-and-compute.md)。

本页文档记录了 crate 根与入口点（`src/lib.rs`、`src/main.rs`、`src/error.rs`、`src/types.rs`）、`src/version.rs` 中的构建身份及其构建脚本 `build.rs`、独立诊断二进制文件 `src/bin/precision_test.rs`，以及 `src/compute/` 中的 CPU/GPU 后端选择层（`mod.rs`、`backend.rs`、`policy.rs`）。

## 索引

| 条目 | 位置 | 摘要 |
|---|---|---|
| `RustMsptError` | `src/error.rs:4` | 覆盖 I/O、YAML、TIFF、配置、网格及 GPU 失败的 crate 级错误枚举。 |
| `Result` | `src/error.rs:27` | 类型别名 `Result<T> = std::result::Result<T, RustMsptError>`，在整个 crate 中使用。 |
| `Cli` | `src/main.rs:27` | 顶层 clap CLI 结构体，包装一个 `Commands` 子命令。 |
| `Commands` | `src/main.rs:33` | 7 个 CLI 子命令（Forge/Measure/Optimize/Pack/Scale/Crop/SplitFilter）的枚举。 |
| `default_config_path` | `src/main.rs:179` | 在 `data/input/` 下构建默认配置路径。 |
| `pick_config_path` | `src/main.rs:184` | 选择用户提供的配置路径，或回退到默认路径。 |
| `main`（main.rs） | `src/main.rs:194` | CLI 入口点：解析参数、加载配置、应用覆盖项、运行所选流水线。 |
| `main`（precision_test.rs） | `src/bin/precision_test.rs:6` | 独立诊断二进制文件，比较 CPU 精确法、CPU 蒙特卡洛法与 GPU 蒙特卡洛法之间 S2 计算的精度/性能。 |
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
| `Triangle` | `src/types.rs:114` | 引用网格顶点数组的索引三元组 `(a, b, c)`。 |
| `Mesh` | `src/types.rs:121` | 顶点/面容器：`vertices: Vec<Vec3>`、`faces: Vec<Triangle>`。 |
| `Mesh::empty` | `src/types.rs:128` | 构造一个空网格。 |
| `Mesh::is_empty` | `src/types.rs:136` | 若网格没有顶点或没有面，则为真。 |
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
| `select_backend` | `src/compute/policy.rs:22` | 计算密集型流水线所使用的中心化 CPU/GPU/Auto 分发策略。 |
| `configured_mode` | `src/compute/policy.rs:142` | Resolve strict environment override. |
| `resolve_execution` | `src/compute/policy.rs:162` | Resolve method support, workload budget and fallback. |

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

`Cli`（`src/main.rs:27`）是顶层 `#[derive(Parser)]` 结构体；它持有单个 `command: Commands` 字段。`Commands`（`src/main.rs:33`）是一个 `#[derive(Subcommand)]` 枚举，包含七个变体，每个都携带相同的三个可选参数：

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
- **源码位置：** `src/main.rs:179`
- **用途：** 在 `data/input/` 下构建默认配置文件路径。
- **参数：**
  - `file_name` — 配置文件的基础名（例如 `"pack_config.yaml"`）。
- **返回值：** 等于 `data/input/<file_name>` 的 `PathBuf`。
- **副作用：** 无（纯路径构造；不检查文件是否存在）。

#### pick_config_path

- **签名：** `fn pick_config_path(config: Option<PathBuf>, file_name: &str) -> PathBuf`
- **源码位置：** `src/main.rs:184`
- **用途：** 解析某个子命令要使用的配置路径：若给出了用户提供的 `--config` 值则使用它，否则使用 `data/input/` 下的默认路径。
- **参数：**
  - `config` — 可选的 `--config` CLI 参数。
  - `file_name` — 当 `config` 为 `None` 时传给 `default_config_path` 的回退文件基础名。
- **返回值：** 解析得到的 `PathBuf`。
- **副作用：** 无。

#### main

- **签名：** `fn main() -> anyhow::Result<()>`
- **源码位置：** `src/main.rs:194`
- **用途：** 解析 CLI 参数，为所选子命令加载 YAML 配置，应用 `--input`/`--output` 覆盖项，并运行相应的流水线。
- **参数：** 无（通过 `Cli::parse()` 读取 `std::env::args`）。
- **返回值：** 成功时为 `Ok(())`；若配置加载、路径解析或流水线执行（`Pipeline::run`）失败，则为 `anyhow::Error`。
- **副作用：** 从磁盘读取 YAML 配置文件；针对每个子命令，构造对应的流水线结构体（`ForgePipeline`、`MeasurePipeline`、`OptimizePipeline`、`PackPipeline`、`ScalePipeline`、`CropPipeline`、`SplitFilterPipeline`）并调用 `.run()`，后者继而读取输入网格/图像文件，并按配置指示写入输出文件。通过底层流水线向标准输出打印进度。
- **说明：** 这是 `rustmspt` 二进制文件唯一的入口点。`match cli.command { ... }` 代码块是扁平的分发结构：每个分支针对其子命令重复相同的三步模式（解析路径 → 加载并修改配置 → 构造并运行流水线）；除 `pick_config_path` 外，各分支之间没有共享的辅助函数。

## version.rs 与 build.rs

`src/version.rs` 只回答一个问题——这个二进制是什么——并且是唯一回答它的地方。`rustmspt --version`、`rustmspt version [--json]` 以及写入放置输出的身份都读取同一个 `BuildIdentity`，因此它们不可能互相矛盾。

这些值来自 `build.rs`：它在编译期运行，把结果写入由 `env!` 读取的环境变量。有两条性质是刻意为之的：

- **没有 git 的检出仍然能构建。** 所有 git 调用都经由 `git_output`，当 git 不存在、没有 `.git`、或命令失败（例如没有任何提交的仓库）时返回 `None`。未确定的值在 Rust 侧是 `None`，序列化为 JSON `null`。
- **`null` 不等于 `false`。** "无法确定工作树是否有改动"与"工作树是干净的"是两种不同的断言，因此 `git_dirty` 的类型是 `Option<bool>`。只有在同时能给出提交号时才会报告干净与否。

诚实性的边界值得说明：`git_dirty` 描述的是 `build.rs` 最后一次运行时的工作树状态，可能早于最后一次编译之后所做的修改。`emit_rerun_triggers` 通过在 `build.rs`、`Cargo.toml`、`Cargo.lock`、`src/` 或 git 引用变化时重跑脚本来缩小这个窗口，但无法将其完全消除。

#### BuildIdentity

- **定义：** `src/version.rs:18`
- **用途：** 在构建所能确定的范围内，说明这个二进制是什么。

| 字段 | 类型 | 含义 |
|---|---|---|
| `name` | `&'static str` | 包名（`CARGO_PKG_NAME`）。 |
| `version` | `&'static str` | 包版本（`CARGO_PKG_VERSION`）。 |
| `git_commit` | `Option<&'static str>` | 完整的 40 字符提交 sha1，或 `None`。 |
| `git_dirty` | `Option<bool>` | `build.rs` 最后一次运行时工作树是否有未提交改动；未确定时为 `None`。 |
| `features` | `Vec<&'static str>` | 已启用的 cargo 特性，已排序。 |
| `target` | `Option<&'static str>` | 编译目标三元组。 |
| `host` | `Option<&'static str>` | 执行编译的主机三元组。 |
| `profile` | `Option<&'static str>` | `debug` 或 `release`。 |
| `source_date_epoch` | `Option<&'static str>` | 为可重现构建设置的 `SOURCE_DATE_EPOCH`。 |

#### build_identity

- **签名：** `pub fn build_identity() -> BuildIdentity`
- **源码：** `src/version.rs:36`
- **用途：** 返回编译进该二进制的身份。
- **参数：** 无。
- **返回：** `BuildIdentity`，构建无法确定的字段为 `None`。
- **副作用：** 无——所有值都是编译期常量，运行时不派生进程、不读文件。
- **说明：** `features` 由逗号分隔的列表解析而来；空列表表示未启用任何 cargo 特性。由于 `default = []` 本身也是一个声明的特性，默认构建会报告 `["default"]`。

#### BuildIdentity::version_detail

- **签名：** `pub fn version_detail(&self) -> String`
- **源码：** `src/version.rs:64`
- **用途：** 将身份渲染为**不含**程序名的单行文本。
- **返回：** 例如 `0.2.0 (git 0a8eb1c, clean; features: none)`。
- **副作用：** 无。
- **说明：** clap 会在其 `--version` 字符串前自动加上程序名，因此这里必须省略，否则名字会出现两次。未知提交渲染为 `git unknown`；未知的干净状态渲染为 `dirt unknown`；空特性列表渲染为 `none`。

#### BuildIdentity::version_line

- **签名：** `pub fn version_line(&self) -> String`
- **源码：** `src/version.rs:92`
- **用途：** 将身份渲染为**含**程序名的单行文本。
- **返回：** 例如 `rustmspt 0.2.0 (git 0a8eb1c, clean; features: none)`。
- **副作用：** 无。
- **说明：** 恰好是 `format!("{name} {detail}")`，因此 `rustmspt version` 与 `rustmspt --version` 打印完全相同的文本。

#### identity_json

- **签名：** `pub fn identity_json(identity: &BuildIdentity) -> String`
- **源码：** `src/version.rs:103`
- **用途：** 将构建身份序列化为格式化 JSON。
- **参数：**
  - `identity` — 待渲染的身份。
- **返回：** 包含全部字段的 JSON 对象，未确定的值为 `null`。
- **副作用：** 无。
- **说明：** 序列化万一失败则回退为 `"{}"`，因此该函数是全函数。同一对象会作为 `tool` 嵌入放置记录与运行报告。

#### non_empty (version.rs)

- **签名：** `fn non_empty(value: &'static str) -> Option<&'static str>`
- **源码：** `src/version.rs:4`
- **用途：** 将构建脚本传入的空字符串映射为 `None`。
- **返回：** 非空时为 `Some(value)`，否则为 `None`。
- **副作用：** 无。
- **说明：** `build.rs` 对每个无法确定的值输出空字符串；这里是唯一把该约定翻译成 `Option` 的地方。

#### git_output

- **签名：** `fn git_output(args: &[&str]) -> Option<String>`
- **源码：** `build.rs:11`
- **用途：** 在 crate 目录下运行 git 命令并返回去除首尾空白的 stdout。
- **参数：**
  - `args` — 追加在 `git -C <CARGO_MANIFEST_DIR>` 之后的参数。
- **返回：** git 存在且退出码为零时返回 `Some(stdout)`，否则为 `None`。
- **副作用：** 构建期派生一个 git 进程。
- **说明：** 绝不 panic。缺少 git 二进制、没有 `.git`、以及没有任何提交的仓库，三种情况都返回 `None`——构建成功，而身份如实声明自己不知道。

#### rerun_if_exists

- **签名：** `fn rerun_if_exists(path: &Path)`
- **源码：** `build.rs:25`
- **用途：** 仅当路径存在时才输出 `cargo:rerun-if-changed`。
- **副作用：** 打印一条 cargo 指令。
- **说明：** 对 `.git` 条目而言，检查存在性很重要：worktree 或 submodule 检出中的 `.git` 是文件而非目录；而指定一个不存在的路径会让 cargo 在每次构建时都重跑脚本。

#### emit_rerun_triggers

- **签名：** `fn emit_rerun_triggers()`
- **源码：** `build.rs:37`
- **用途：** 输出所有会改变所记录身份的重跑触发条件。
- **副作用：** 打印 cargo 指令。
- **说明：** 覆盖 `build.rs`、`Cargo.toml`、`Cargo.lock`、`src/`、`.git/HEAD`、`.git/index`、`.git/packed-refs`、`.git/HEAD` 所指向的分支引用，以及环境变量 `SOURCE_DATE_EPOCH`。包含 `src/` 是为了让任何源文件的修改都能刷新 dirty 标志。

#### enabled_features

- **签名：** `fn enabled_features() -> Vec<String>`
- **源码：** `build.rs:62`
- **用途：** 列出本次构建启用的 cargo 特性，已排序。
- **返回：** cargo 写法的特性名（小写、连字符）。
- **副作用：** 无。
- **说明：** 从环境变量 `CARGO_FEATURE_*` 读取，**而非** `cfg!(feature = ...)`。构建脚本的编译不带 crate 自身的特性，因此在 `build.rs` 内部使用 `cfg!` 会一律报告特性未启用。

#### main (build.rs)

- **签名：** `fn main()`
- **源码：** `build.rs:80`
- **用途：** 将 git 提交、工作树是否干净、已启用特性与构建平台写入编译期环境变量。
- **副作用：** 打印 `cargo:rustc-env` 与 `cargo:rerun-if-changed` 指令。
- **说明：** 每个值都以"可能为空"的字符串输出，空表示"未确定"。只有在已经确定提交号之后才会查询干净状态，因此 `git_dirty` 不可能对一个提交未知的工作树声称 `clean`。此处不打印 `cargo:warning`，否则会让启用 `-D warnings` 的流水线失败。

## bin/precision_test.rs

`src/bin/precision_test.rs` 作为一个独立的 Cargo 二进制目标（`precision_test`）构建，区别于 `rustmspt` 库与 CLI。它是一个手动诊断工具，用于比较三种方法在 S2（两点相关函数）计算精度与性能上的差异：CPU 精确法（占据网格 + FFT）、CPU 蒙特卡洛法（占据网格采样）以及 GPU 蒙特卡洛法（连续光线投射，仅在启用 `gpu` 特性时构建）。它不受测试套件或任何库流水线的调用。

#### main

- **签名：** `fn main()`
- **源码位置：** `src/bin/precision_test.rs:6`
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

`pub fn empty() -> Self` — `src/types.rs:128`。构造一个 `vertices` 和 `faces` 均为空向量的 `Mesh`。无副作用。

#### is_empty

`pub fn is_empty(&self) -> bool` — `src/types.rs:136`。若该网格没有顶点**或**没有面（即 `vertices.is_empty() || faces.is_empty()`，而非严格的“两者皆空”检查），则返回 `true`。无副作用。

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
| `max_storage_buffer_binding_size` | `u64` | 单个 binding 的字节上限（CPU 为 0），与总任务预算独立。 |

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
- **源码位置：** `src/compute/policy.rs:22`
- **用途：** 将所请求的 `AccelerationMode`（`Cpu`/`Gpu`/`Auto`）连同工作负载/配置参数，解析为一个具体的 `BackendSelection`，其中应用了 GPU 可用性、内存限制及工作负载规模检查。
- **参数：**
  - `requested` — 调用方/配置所请求的模式。
  - `gpu_min_voxels` — 仅用于 `Auto` 模式：低于该体素数的工作负载不会尝试使用 GPU；若为 `None` 则默认为 `250_000`。
  - `gpu_memory_limit_mb` — 旧预算参数。该接口没有工作集估计，带预算的 GPU 请求在探测前返回 CPU 及明确原因；MiB 转换溢出单独报告。应使用 resolve_execution 与已验证工作集估计来执行预算。
  - `workload_voxels` — 当前工作负载的体素数，在 `Auto` 模式下与 `gpu_min_voxels` 比较。
- **返回值：** 一个 `BackendSelection`：
  - `requested == Cpu`：始终为 `{ backend: Cpu, fallback: None }`。
  - `requested == Gpu`：先按上述规则拒绝无工作集估计的预算请求；否则探测 GPU，成功返回 GPU，初始化失败或未启用特性时返回 CPU 及原因。
  - `requested == Auto`：首先将 `workload_voxels` 与 `gpu_min_voxels.unwrap_or(250_000)` 比较；若低于阈值，立即返回 CPU，并附带一个引用体素数的回退原因，完全不尝试 GPU 初始化。否则，遵循与 `Gpu` 分支相同的 GPU 初始化/内存限制逻辑（生成的任何 `FallbackReason` 中 `requested: Auto`）。
- **副作用：** 当启用 `gpu` 特性、且 `requested` 为 `Gpu`，或为 `Auto` 且工作负载达到或超过体素阈值时，会调用 `crate::gpu::try_init_gpu()`，该调用可能会初始化一个 wgpu 适配器——在别处已记录为一个可能较为昂贵的首次调用（适配器/设备枚举与创建）。
- **说明：** CPU 与低于阈值的 Auto 请求在预算检查和设备探测前返回。总任务预算与单 buffer/binding 上限是独立约束；执行 planner 检查具体缓冲大小，resolve_execution 检查任务字节数与禁止回退策略。

### configured_mode / resolve_execution

`configured_mode(&AccelerationConfig) -> Result<AccelerationMode>` 一次读取严格的 cpu/gpu/auto 环境覆盖，不探测 GPU。`resolve_execution(config, requested, workload, threshold, supports_gpu, estimated_gpu_bytes) -> Result<BackendSelection>` 先处理 CPU/小 auto，再检查方法支持、实现的选项和任务估算字节/MB 预算，最后探测 GPU。未支持选项明确报错；禁止回退时 GPU 失败返回错误。估算不等于驱动实际显存。Measure 已采用此方法感知策略，其余管线继续迁移。

新建的驻留 GPU exact 求值在后端选择和执行中共用 ExactMemoryPlan。设 T=max(36×faces,4)、M=4×cells、B 为批次部分结果槽数，保守逻辑峰值为 2T+M+128+80B，计入待完成三角/offset 上传和批次增长时同时存在的新旧缓冲；128 字节覆盖固定参数/计数/占位资源。B 从 200000 按 MiB 预算缩小，至少 1；最小批次仍超限则在设备初始化前拒绝，由调用方执行配置的回退策略。该模型不含驱动/编译器内部资源和 CPU 内存，仅适用于新建生产 direct-shell exact 求值，不声称覆盖实验 tiled/reduced 或任意已有高水位管线。旧 exact 网格硬限制仍独立存在。

| Symbol | Source | Contract |
|---|---|---|
| `ExactMemoryPlan` | `src/compute/exact_memory.rs:5` | Fresh resident exact logical GPU peak and budget-selected partial batch. |
| `ExactMemoryPlan::new` | `src/compute/exact_memory.rs:12` | Checked resource arithmetic and batch selection; may still require check_budget for infeasible minima. |
| `ExactMemoryPlan::check_budget` | `src/compute/exact_memory.rs:52` | Enforce configured MiB cap before initialization. |
