# 流水线核心参考

> **待翻译：** `RenderPipeline::run` 的详细契约见[英文流水线参考](../../en-us/reference/pipeline-core.md#pipelinerenderrs)。

涵盖 `Pipeline` trait 基础设施以及四个较简单/辅助性的流水线：`rotation`、`scale`、`forge` 和 `measure`。`pack`、`optimize`、`crop` 和 `split_filter` 流水线在其他文档中说明。

## 索引

| 条目 | 位置 | 摘要 |
|---|---|---|
| `Pipeline::run`（trait） | `src/pipeline/mod.rs:17` | 每个流水线结构体实现的 trait 方法，用于端到端执行。 |
| `create_progress_bar` | `src/pipeline/mod.rs:25` | 使用给定模板和填充字符构建一个能感知 tty 的 indicatif 进度条。 |
| `RotationMode`（枚举） | `src/pipeline/rotation.rs:6` | 表示不旋转、固定轴旋转或随机轴旋转。 |
| `parse_rotation_mode` | `src/pipeline/rotation.rs:18` | 将 `none/x/y/z/vector/any` 配置字符串解析为 `RotationMode`。 |
| `sample_rotation_axis` | `src/pipeline/rotation.rs:57` | 为给定的 `RotationMode` 抽取一个具体的旋转轴向量。 |
| `ScalePipeline`（结构体） | `src/pipeline/scale.rs:8` | 持有缩放流水线所需的 `ScaleConfig`。 |
| `ScalePipeline::run` | `src/pipeline/scale.rs:19` | 加载 STL，应用单位换算/系数缩放，可选修正朝向，保存输出。 |
| `ForgePipeline`（结构体） | `src/pipeline/forge.rs:13` | 持有 FFD 锻造流水线所需的 `ForgingConfig`。 |
| `ForgePipeline::parse_roi_bbox` | `src/pipeline/forge.rs:19` | 从配置中解析可选的 6 元素 ROI 包围盒。 |
| `ForgePipeline::parse_compression_axis` | `src/pipeline/forge.rs:38` | 将压缩轴字符串（`x`/`y`/`z`）解析为索引和标签。 |
| `ForgePipeline::run` | `src/pipeline/forge.rs:58` | 执行基于 FFD 的压缩/锻造，跟踪 ROI，写出锻造后的 STL 及文本报告。 |
| `MeasurePipeline`（结构体） | `src/pipeline/measure.rs:16` | 持有 S2/体积分数测量流水线所需的 `MeasurementConfig`。 |
| `MeasurePipeline::parse_optional_bbox` | `src/pipeline/measure.rs:22` | 从配置中解析可选的包围盒（3 元素尺寸或 6 元素最小/最大值）。 |
| `MeasurePipeline::l2_error` | `src/pipeline/measure.rs:31` | 计算两个 S2 值向量在其公共前缀长度上的 L2 距离。 |
| `MeasurePipeline::run` | `src/pipeline/measure.rs:53` | 加载 STL，计算体积分数与 S2 相关性（精确/MC/两者兼有，CPU 或 GPU），写出报告。 |

---

## `pipeline/mod.rs`

#### Pipeline::run (trait)

- **签名：** `fn run(&self) -> Result<()>`
- **源码位置：** `src/pipeline/mod.rs:17`
- **用途：** 本 crate 中每个流水线共用的执行入口点。
- **参数：** `&self` —— 具体流水线结构体自身的配置。
- **返回值：** 成功时为 `Ok(())`，失败时为 `RustMsptError`。
- **副作用：** 完全取决于具体实现——通常会读取输入 STL、执行几何运算、写出输出文件，并向 stdout/stderr 打印 `[Info]`/`[Warning]` 进度信息。
- **说明：** 由本 crate 中全部 7 个流水线结构体实现：`CropPipeline`、`ForgePipeline`、`MeasurePipeline`、`OptimizePipeline`、`PackPipeline`、`ScalePipeline` 以及 `SplitFilterPipeline`。`main.rs` 会根据请求的子命令/配置构造对应的流水线，并在一次程序运行中对其调用一次 `.run()`；单次进程运行内不存在内置的流水线链式调用或循环调用。
- **另请参阅：** 流水线分发逻辑见 `src/main.rs`。

#### create_progress_bar

- **签名：** `pub fn create_progress_bar(length: u64, template: &str, chars: &str) -> ProgressBar`
- **源码位置：** `src/pipeline/mod.rs:25`
- **用途：** 构建一个标准化的 `indicatif` 进度条，当输出不是交互式终端时自动隐藏。
- **参数：**
  - `length` —— 进度条代表的总步数/单位数。
  - `template` —— indicatif 模板字符串（例如 `"{bar} {pos}/{len}"`）。
  - `chars` —— 传给 `progress_chars` 的填充/头部/空白字符。
- **返回值：** 一个配置好的 `ProgressBar`。若模板字符串解析失败，则静默回退到 `ProgressStyle::default_bar()`。
- **副作用：** 检查 `std::io::stderr().is_terminal()`；若 stderr 不是 TTY（例如重定向到文件，或在 CI 中运行），则将进度条的绘制目标设置为 `hidden()`，不再输出进度信息。
- **说明：** 供耗时较长的流水线（pack、optimize）用于报告迭代/放置进度，同时不破坏非交互式日志。

---

## `pipeline/rotation.rs`

`pack` 和 `optimize` 流水线在随机或确定性地为颗粒/构件定向时使用的共享旋转轴工具函数。

#### RotationMode (enum)

- **源码位置：** `src/pipeline/rotation.rs:6`
- **用途：** 表示应用于已放置对象的旋转策略。
- **变体：**
  - `None` —— 不施加旋转。
  - `Axis(Vec3)` —— 旋转被约束到单一固定轴（单位轴 `x`/`y`/`z`，或用户提供的任意向量）。
  - `Any` —— 每次放置都随机抽取旋转轴。
- **说明：** 仅派生 `Clone`，未派生 `Debug`/`Copy`。

#### parse_rotation_mode

- **签名：** `pub fn parse_rotation_mode(prefix: &str, mode: Option<&str>, axis_vec: Option<&Vec<f64>>) -> Result<RotationMode>`
- **源码位置：** `src/pipeline/rotation.rs:18`
- **用途：** 将旋转模式配置字符串解析为 `RotationMode` 值。
- **参数：**
  - `prefix` —— 仅用于构造可读错误信息的配置段名称（例如 `"pack"` 或 `"optimize"`）。
  - `mode` —— 原始模式字符串：取值为 `none`、`x`、`y`、`z`、`vector`、`any` 之一（不区分大小写，忽略首尾空白）。为 `None` 时默认为 `"any"`。
  - `axis_vec` —— 仅当 `mode == "vector"` 时需要；必须恰好包含 3 个元素且模长非零。
- **返回值：** 成功时为 `Ok(RotationMode)`。
- **副作用：** 无（纯解析）。
- **说明：** 以下情况返回 `RustMsptError::InvalidConfig`：模式字符串无法识别；请求了 `"vector"` 但 `axis_vec` 缺失或长度不为 3；或所提供向量的模长 `<= 1e-12`（视为零，通过 `geometry::vec_norm` 检查）。

#### sample_rotation_axis

`pub fn sample_rotation_axis(rng: &mut rand::rngs::ThreadRng, mode: &RotationMode) -> Option<Vec3>` —— `src/pipeline/rotation.rs:57`。对 `RotationMode::None` 返回 `None`；对 `RotationMode::Axis` 原样返回固定轴；对 `RotationMode::Any` 返回一个新抽取的 `Vec3`，其每个分量均从 `[-1.0, 1.0)` 均匀采样。除消耗 RNG 状态外无其他副作用。
> **文档说明：** AI-FUNC-SUMMARY 中写道 "Any" 向量是 "random in cube"（立方体内随机），这与代码相符——它在返回前**并未**被归一化到单位球面，因此若下游代码需要单位旋转轴，必须自行归一化。

---

## `pipeline/scale.rs`

#### ScalePipeline (struct)

- **源码位置：** `src/pipeline/scale.rs:8`
- **字段：** `config: ScaleConfig` —— 完整的缩放配置（输入/输出路径、缩放类型/数值、朝向标志）。

#### ScalePipeline::run

- **签名：** `fn run(&self) -> Result<()>`
- **源码位置：** `src/pipeline/scale.rs:19`
- **用途：** 加载 STL 网格，按单位换算或显式系数进行缩放，可选地将构件朝向修正为正的有符号体积，并保存结果。
- **参数：** `&self` —— 读取 `self.config.input.stl_path`、`self.config.output.stl_path`、`self.config.scaling.{type, value, orient_to_positive_volume}`。
- **返回值：** 成功时为 `Ok(())`；若 `mm_per_voxel`/`voxel_per_mm` 数值非正，或 `scaling.type` 无法识别，则为 `Err(RustMsptError::InvalidConfig(..))`。
- **副作用：** 通过 `load_stl_or_merge_folder` 从磁盘读取 STL（或合并整个文件夹的多个 STL）；通过 `save_stl` 将缩放后的网格写入 `output.stl_path`；向 stdout 打印 `[Info]` 信息，报告模式、数值、原始/缩放后的边界与体积，以及（若启用）有多少个网格构件因朝向修正而被翻转。
- **说明：** 支持三种缩放模式：
  - `"factor"` —— `value` 直接作为乘法缩放系数使用。
  - `"mm_per_voxel"` —— `value`（必须 `> 0`）直接作为系数使用。
  - `"voxel_per_mm"` —— `value`（必须 `> 0`）取倒数（`1.0 / value`）得到系数。

  启用 `orient_to_positive_volume` 时，会在缩放后应用 `orient_components_to_positive_volume`，并报告被翻转/总构件数；否则网格保持不变（克隆使用）。

---

## `pipeline/forge.rs`

#### ForgePipeline (struct)

- **源码位置：** `src/pipeline/forge.rs:13`
- **字段：** `config: ForgingConfig` —— 锻造参数（输入/输出路径、压缩比/压缩轴、膨胀系数、ROI 包围盒、网格类型、孔隙致密化、朝向标志）。

#### ForgePipeline::parse_roi_bbox

- **签名：** `fn parse_roi_bbox(values: &Option<Vec<f64>>) -> Option<BoundingBox>`
- **源码位置：** `src/pipeline/forge.rs:19`
- **用途：** 从配置中解析可选的 6 元素 `[min_x, min_y, min_z, max_x, max_y, max_z]` 感兴趣区域（ROI）包围盒。
- **参数：** `values` —— 来自 `config.forging.roi_bounding_box` 的 `Option<Vec<f64>>`。
- **返回值：** 若向量恰好包含 6 个元素则返回 `Some(BoundingBox)`；否则返回 `None`（配置缺失或长度不对——长度不对的情形被静默视为"无 ROI"而非报错）。
- **副作用：** 无。

#### ForgePipeline::parse_compression_axis

- **签名：** `fn parse_compression_axis(axis: Option<&str>) -> Result<(usize, &'static str)>`
- **源码位置：** `src/pipeline/forge.rs:38`
- **用途：** 将配置中的压缩轴字符串解析为数值型网格轴索引及显示标签。
- **参数：** `axis` —— 可选字符串，为 `None` 时默认 `"z"`；匹配时先去除首尾空白再不区分大小写比较。
- **返回值：** `Ok((0, "x"))`、`Ok((1, "y"))` 或 `Ok((2, "z"))`。
- **副作用：** 无。
- **说明：** 对于 `x`/`y`/`z` 以外的任何字符串返回 `RustMsptError::InvalidConfig`。

#### ForgePipeline::run

- **签名：** `fn run(&self) -> Result<()>`
- **源码位置：** `src/pipeline/forge.rs:58`
- **用途：** 执行自由变形（FFD）锻造流水线：沿一个轴压缩网格（含膨胀效果及可选的孔隙致密化），跟踪感兴趣区域（ROI）在变形过程中的移动情况，重新对齐输出使 ROI 落在请求的位置，并写出锻造后的 STL 及人类可读的报告。
- **参数：** `&self` —— 读取 `self.config.forging.*`：`input_stl_path`、`output_stl_path`（默认 `data/output/forged_mesh.stl`）、`roi_bounding_box`、`compression_ratio`（默认 `0.2`）、`compression_axis`（默认 `"z"`）、`orient_to_positive_volume`、`bulge_factor`（默认 `0.5`）、`mesh_type`（默认 `"particle"`）、`void_densification`（默认 `1.0`）。
- **返回值：** 成功时为 `Ok(())`，或传播的 I/O / 配置错误。
- **副作用：**
  - 从磁盘读取输入 STL（或合并后的文件夹）。
  - 调用 `simulate_forging_ffd_with_tracking` 执行实际的 FFD 变形并获取变形后的 ROI 位置。
  - 可选地将网格构件重新朝向为正的有符号体积（`orient_components_to_positive_volume`）。
  - 当追踪到的 ROI 与原始请求的 ROI 均可用且偏移量不可忽略（任一轴上 `> f64::EPSILON`）时，平移输出网格（`translate_mesh`），使追踪到的 ROI 最小角与原始请求的 ROI 最小角重新对齐。
  - 通过 `save_stl` 写出锻造后的网格。
  - 向 `<output>.txt`（同路径，扩展名替换）写出文本报告，内容包括变形前/后的包围盒（网格和 ROI）、变形前/后的 ROI 体积分数、压缩轴、朝向修正统计信息，以及输出平移向量。若需要则创建父目录。
  - 向 stdout 打印与上述相同的信息，格式为 `[Info]` 行。
- **说明：** 若提供了 ROI 包围盒，则体积分数在该包围盒范围内计算，否则在整个网格的包围盒（`lattice_bbox`）范围内计算。"变形前"体积与"变形后"体积（`_before`、`_after`，通过 `mesh_volume` 计算）被计算出来但除了被丢弃之外未被直接使用——参见下方的 `> **文档说明：**`。
  > **文档说明：** `_before` 和 `_after`（整个网格的体积）通过 `mesh_volume` 计算，但被绑定到以下划线开头的变量上，从未出现在报告或控制台输出中；只有 ROI 体积分数（`before_roi_vf`、`after_roi_vf`）会被报告。这看起来是有意为之（在保留该计算以备将来使用或调试的同时，消除"未使用"警告），而非一个 bug，但这些数值目前对用户不可见。
- **另请参阅：** [../algorithms/ffd-forging.md](../algorithms/ffd-forging.md)

---

## `pipeline/measure.rs`

#### MeasurePipeline (struct)

- **源码位置：** `src/pipeline/measure.rs:16`
- **字段：** `config: MeasurementConfig` —— 测量参数（STL 路径、包围盒、S2 方法/采样数/间距、加速策略、输出路径、CPU 上限）。

#### MeasurePipeline::parse_optional_bbox

- **签名：** `fn parse_optional_bbox(values: &Option<Vec<f64>>) -> Result<Option<BoundingBox>>`
- **源码位置：** `src/pipeline/measure.rs:22`
- **用途：** 从配置中解析可选的包围盒，接受 3 元素尺寸向量或 6 元素最小/最大值向量（具体形状相关的解析工作委托给 `config::parse_box_dimensions`）。
- **参数：** `values` —— `Option<Vec<f64>>`。
- **返回值：** 若 `values` 为 `None` 或空向量，则为 `Ok(None)`；否则为来自 `parse_box_dimensions` 的 `Ok(Some(BoundingBox))`，若该解析失败则为传播的 `Err`。
- **副作用：** 无。

#### MeasurePipeline::l2_error

- **签名：** `fn l2_error(a: &[f64], b: &[f64]) -> f64`
- **源码位置：** `src/pipeline/measure.rs:31`
- **用途：** 计算两个 S2 相关性数值向量之间的 L2（欧几里得）距离，用于在 `method = "both"` 时比较 "exact" 与 "monte_carlo" 结果。
- **参数：** `a`、`b` —— S2 数值切片，长度可能不同。
- **返回值：** 在 `i` 取遍 `0..min(a.len(), b.len())` 时，`sqrt(sum((a[i]-b[i])^2))`；若任一切片为空则为 `0.0`。
- **副作用：** 无。
- **说明：** 该函数重复了 `geometry::l2_norm` 的核心逻辑（在共享长度前缀上求平方差之和再开方）。它被保留为一个私有的、流水线本地的辅助函数，而不是复用 geometry 模块中的函数——目前二者之间不存在共享代码路径，若任一方发生改动，需要手动保持二者同步。

#### MeasurePipeline::run

- **签名：** `fn run(&self) -> Result<()>`
- **源码位置：** `src/pipeline/measure.rs:53`
- **用途：** 执行 S2 两点相关函数与体积分数测量流水线：加载网格，确定包围盒，选择计算后端（CPU/GPU），按所请求的方法计算 S2，并写出报告。
- **参数：** `&self` —— 读取 `self.config.measurement.*`：`cpu_max`、`stl_path`、`bounding_box`、`stl_bounding_box`、`mc_method`（`"exact"` / `"both"` / 其他任意值均视为 `"monte_carlo"`）、`r_max`、`mc_samples`（默认 `10_000`）、`voxel_pitch`、`acceleration`（模式、`gpu_min_voxels`、`gpu_memory_limit_mb`）、`output_path`。
- **返回值：** 成功时为 `Ok(())`，或传播的错误（例如线程池构建失败被包装为 `InvalidConfig`、I/O 错误）。
- **副作用：**
  - 根据 `cpu_max`（若 `cpu_max == -1` 则使用全部可用核心）构建一个专用的 Rayon 线程池。
  - 读取输入 STL（或合并后的文件夹）。
  - 在确定的包围盒范围内计算颗粒数量（`split_mesh_into_granules`）与体积分数（`volume_fraction_in_bbox`）。
  - 通过 `select_backend(accel.mode, Some(accel.gpu_min_voxels), accel.gpu_memory_limit_mb, voxel_count)` 确定计算后端，其中 `voxel_count` 由包围盒尺寸除以 `voxel_pitch` 得出（`voxel_pitch` 若 `<= 0.0` 则钳制为 `1.0`）。
  - 当启用 `gpu` 特性且选定后端为 GPU 时，初始化 `crate::gpu::s2::GpuS2Pipeline`；若初始化失败，则记录警告并静默回退到 CPU。
  - 根据方法与后端的组合，运行 `calculate_s2`（CPU，在自定义线程池内）或 `calculate_s2_with_gpu`/`calculate_s2_gpu_exact`（GPU，特性门控）。
  - 向 `params.output_path` 写出文本报告（按需创建父目录），内容包括体积分数、计算后端、方法、S2(0) 与体积分数之差，以及完整的 S2 数值序列（对于 `"both"`，还包括精确解与蒙特卡洛序列之间的 L2 误差）。
  - 在整个过程中向 stdout 打印大量 `[Info]`/`[Warning]` 诊断信息（线程池大小、包围盒、S2 配置、后端选择/回退、各方法摘要、最终输出路径）。
- **说明：**
  - 包围盒确定的优先级顺序：显式的 `bounding_box` > `stl_bounding_box` > 从网格推导的包围盒（`mesh_bbox`） > 单位立方体兜底值（`BoundingBox::from_size(Vec3::new(1,1,1))`）。
  - 当体素网格超过 `exact_voxel_limit = 1_500_000` 个体素时，`method = "exact"`（无论是单独使用还是作为 `"both"` 的一部分）会自动降级为 `"monte_carlo"`，并打印一条 `[Warning]`；对于 `"both"`，这意味着只运行蒙特卡洛分支，报告中会注明"(exact skipped by voxel limit)"。
  - 当 crate 编译时未启用 `gpu` 特性时，GPU 相关代码路径完全不存在（`#[cfg(not(feature = "gpu"))]` 分支无条件使用 CPU 线程池）；若请求了 GPU 加速但该特性未被编译进来，则会打印运行时警告。
- **另请参阅：** [../algorithms/s2-two-point-correlation.md](../algorithms/s2-two-point-correlation.md)、[gpu.md](gpu.md)
