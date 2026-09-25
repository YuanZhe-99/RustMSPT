# 流水线核心参考

> **待翻译：** `RenderPipeline::run` 的详细契约见[英文流水线参考](../../en-us/reference/pipeline-core.md#pipelinerenderrs)。

涵盖 `Pipeline` trait 基础设施以及四个较简单/辅助性的流水线：`rotation`、`scale`、`forge` 和 `measure`。`pack`、`optimize`、`crop` 和 `split_filter` 流水线在其他文档中说明。

## 索引

| 条目 | 位置 | 摘要 |
|---|---|---|
| `RenderPipeline::run_in_pool` | `src/pipeline/render.rs:59` | Execute render stages and fallback within the configured pool. |
| `Pipeline::run`（trait） | `src/pipeline/mod.rs:17` | 每个流水线结构体实现的 trait 方法，用于端到端执行。 |
| `create_progress_bar` | `src/pipeline/mod.rs:53` | 使用给定模板和填充字符构建一个能感知 tty 的 indicatif 进度条。 |
| `run_in_cpu_pool` | `src/pipeline/mod.rs:37` | 在按 `cpu_max`（缺省/-1：全部 worker）确定大小的专用 Rayon 池中运行；forge 与 scale 使用，使所有并行段共享一个预算。 |
| `RotationMode`（枚举） | `src/pipeline/rotation.rs:6` | 表示不旋转、固定轴旋转或随机轴旋转。 |
| `parse_rotation_mode` | `src/pipeline/rotation.rs:17` | 将 `none/x/y/z/vector/any` 配置字符串解析为 `RotationMode`。 |
| `sample_rotation_axis` | `src/pipeline/rotation.rs:56` | 为给定的 `RotationMode` 抽取一个具体的旋转轴向量。 |
| `ScalePipeline`（结构体） | `src/pipeline/scale.rs:8` | 持有缩放流水线所需的 `ScaleConfig`。 |
| `ScalePipeline::run` | `src/pipeline/scale.rs:116` | 加载 STL，应用单位换算/系数缩放，可选修正朝向，保存输出。 |
| `ForgePipeline`（结构体） | `src/pipeline/forge.rs:12` | 持有 FFD 锻造流水线所需的 `ForgingConfig`。 |
| `ForgePipeline::parse_roi_bbox` | `src/pipeline/forge.rs:18` | 从配置中解析可选的 6 元素 ROI 包围盒。 |
| `ForgePipeline::parse_compression_axis` | `src/pipeline/forge.rs:37` | 将压缩轴字符串（`x`/`y`/`z`）解析为索引和标签。 |
| `ForgePipeline::run` | `src/pipeline/forge.rs:259` | 执行基于 FFD 的压缩/锻造，跟踪 ROI，写出锻造后的 STL 及文本报告。 |
| `MeasurePipeline`（结构体） | `src/pipeline/measure.rs:12` | 持有 S2/体积分数测量流水线所需的 `MeasurementConfig`。 |
| `MeasurePipeline::parse_optional_bbox` | `src/pipeline/measure.rs:18` | 从配置中解析可选的包围盒（3 元素尺寸或 6 元素最小/最大值）。 |
| `MeasurePipeline::l2_error` | `src/pipeline/measure.rs:27` | 计算两个 S2 值向量在其公共前缀长度上的 L2 距离。 |
| `MeasurePipeline::run` | `src/pipeline/measure.rs:49` | 加载 STL，计算体积分数与 S2 相关性（精确/MC/两者兼有，CPU 或 GPU），写出报告。 |
| `MeasurePipeline::run_in_pool` | `src/pipeline/measure.rs:83` | Method-specific measurement in configured pool. |

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
- **源码位置：** `src/pipeline/mod.rs:37`
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

- **源码位置：** `src/pipeline/forge.rs:12`
- **字段：** `config: ForgingConfig` —— 锻造参数（输入/输出路径、压缩比/压缩轴、膨胀系数、ROI 包围盒、网格类型、孔隙致密化、朝向标志）。

#### ForgePipeline::parse_roi_bbox

- **签名：** `fn parse_roi_bbox(values: &Option<Vec<f64>>) -> Option<BoundingBox>`
- **源码位置：** `src/pipeline/forge.rs:18`
- **用途：** 从配置中解析可选的 6 元素 `[min_x, min_y, min_z, max_x, max_y, max_z]` 感兴趣区域（ROI）包围盒。
- **参数：** `values` —— 来自 `config.forging.roi_bounding_box` 的 `Option<Vec<f64>>`。
- **返回值：** 若向量恰好包含 6 个元素则返回 `Some(BoundingBox)`；否则返回 `None`（配置缺失或长度不对——长度不对的情形被静默视为"无 ROI"而非报错）。
- **副作用：** 无。

#### ForgePipeline::parse_compression_axis

- **签名：** `fn parse_compression_axis(axis: Option<&str>) -> Result<(usize, &'static str)>`
- **源码位置：** `src/pipeline/forge.rs:37`
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

- **源码位置：** `src/pipeline/measure.rs:12`
- **字段：** `config: MeasurementConfig` —— 测量参数（STL 路径、包围盒、S2 方法/采样数/间距、加速策略、输出路径、CPU 上限）。

#### MeasurePipeline::parse_optional_bbox

- **签名：** `fn parse_optional_bbox(values: &Option<Vec<f64>>) -> Result<Option<BoundingBox>>`
- **源码位置：** `src/pipeline/measure.rs:18`
- **用途：** 从配置中解析可选的包围盒，接受 3 元素尺寸向量或 6 元素最小/最大值向量（具体形状相关的解析工作委托给 `config::parse_box_dimensions`）。
- **参数：** `values` —— `Option<Vec<f64>>`。
- **返回值：** 若 `values` 为 `None` 或空向量，则为 `Ok(None)`；否则为来自 `parse_box_dimensions` 的 `Ok(Some(BoundingBox))`，若该解析失败则为传播的 `Err`。
- **副作用：** 无。

#### MeasurePipeline::l2_error

- **签名：** `fn l2_error(a: &[f64], b: &[f64]) -> f64`
- **源码位置：** `src/pipeline/measure.rs:27`
- **用途：** 计算两个 S2 相关性数值向量之间的 L2（欧几里得）距离，用于在 `method = "both"` 时比较 "exact" 与 "monte_carlo" 结果。
- **参数：** `a`、`b` —— S2 数值切片，长度可能不同。
- **返回值：** 在 `i` 取遍 `0..min(a.len(), b.len())` 时，`sqrt(sum((a[i]-b[i])^2))`；若任一切片为空则为 `0.0`。
- **副作用：** 无。
- **说明：** 该函数重复了 `geometry::l2_norm` 的核心逻辑（在共享长度前缀上求平方差之和再开方）。它被保留为一个私有的、流水线本地的辅助函数，而不是复用 geometry 模块中的函数——目前二者之间不存在共享代码路径，若任一方发生改动，需要手动保持二者同步。

#### MeasurePipeline::run / run_in_pool

`run() -> Result<()>` 将整次测量安装在配置的 Rayon 池内，包括读取、准备和 GPU 失败后的 CPU 回退。`run_in_pool() -> Result<()>` 一次解析 `RUSTMSPT_ACCELERATION`，检查有限 pitch/尺寸，并对 exact/MC/both 分别选择后端。正 pitch MC 保持 voxel MC，不调用连续 GPU 内核；只为请求的方法分配管线。GPU 错误遵守 `cpu_fallback`，报告记录各方法回退后的实际后端，全部方法成功才写输出。exact（单独或属于 `both`）在网格占据字节本身超过 `DEFAULT_CPU_EXACT_BUDGET_BYTES`（768 MiB），或 `plan_exact_cpu` 找不到工作集不超过该预算的 CPU 内核时明确报错，绝不替换为 MC；报错前先打印计划行（`[Info] CPU exact working-set plan: ...`）。这取代了原固定的 1,500,000 体素上限（PERF-07）。该预算只约束内存：体素化时间以及大网格上直接内核的运行时间不设上限，计划行会报告模型内核时间。worker 数和索引在运行池内观察。


### Owned CPU transforms (PERF-16)

`forge_owned(mesh, lattice_bbox, track_bbox, compression_ratio, compression_axis, bulge_factor, mesh_type, void_densification)` consumes the mesh and applies the existing tracked FFD mapping. The public borrowed wrapper clones once and delegates. ForgePipeline moves its input into this entry, retains the already computed input bbox, removes unused whole-mesh volume scans, and moves the output when orientation is disabled. ScalePipeline likewise moves its transformed mesh when orientation is disabled. Both log transform-only seconds separately from I/O.

`map_vertices` keeps small slices serial and maps disjoint 8192-vertex blocks on the current Rayon pool only with multiple workers and at least max(131072, workers * 65536) vertices. Scale, translate and both FFD variants use it. Each vertex retains its arithmetic order; void centroid is still accumulated serially after the affine pass. ROI remains unaffected by void closure. Clipped ROI VF and output bbox are still measured from actual geometry; no determinant approximation is substituted.


### Render execution policy

`RenderPipeline::run` installs the whole pipeline in the configured worker pool. `run_in_pool` loads geometry, resolves the environment override and method budget, renders and writes only after success. Both selection and runtime failures honor `cpu_fallback`. GPU work estimates include vertices, color/depth, aligned staging and uniforms. Actual CPU fallback executes in the same pool.

## `pipeline/timing.rs` —— 阶段计时与峰值 RSS（PERF-00）

| 条目 | 位置 | 摘要 |
|---|---|---|
| `StageTimer` | `src/pipeline/timing.rs:3` | 每条流水线的总时钟和当前阶段时钟。 |
| `StageTimer::start` | `src/pipeline/timing.rs:11` | 为指定流水线同时启动两个时钟。 |
| `StageTimer::restart` | `src/pipeline/timing.rs:17` | 不打印地重置阶段时钟（排除不计时的工作）。 |
| `StageTimer::stage` | `src/pipeline/timing.rs:22` | 打印距上次标记的时间并重启阶段时钟。 |
| `StageTimer::report` | `src/pipeline/timing.rs:30` | 把外部累计的时长作为一个阶段打印。 |
| `StageTimer::total` | `src/pipeline/timing.rs:35` | 以给定阶段名打印自启动以来的时间。 |
| `StageTimer::report_resources` | `src/pipeline/timing.rs:42` | 打印 worker 数和峰值 RSS 两行。 |
| `format_stage_line` | `src/pipeline/timing.rs:49` | 格式化 `[Timing] <pipeline> stage=<name> seconds=<f>`（九位小数）。 |
| `report_workers` | `src/pipeline/timing.rs:54` | 打印 `[Timing] <pipeline> workers=<rayon::current_num_threads()>`。 |
| `report_peak_rss` | `src/pipeline/timing.rs:59` | 打印 `[Timing] <pipeline> peak_rss_bytes=<n|unavailable>`。 |
| `peak_rss_bytes` | `src/pipeline/timing.rs:67` | 从 `/proc/self/status` 读取 VmHWM（字节），不可用时返回 `None`。 |
| `parse_vm_hwm` | `src/pipeline/timing.rs:73` | 解析 `VmHWM: <n> kB` 行；缺失、格式错误或单位不是 kB 时返回 `None`。 |

除 meshgen 外的每条流水线都以统一格式 `[Timing] <pipeline> stage=<name> seconds=<f>` 打印各阶段墙钟时间，随后打印 `[Timing] <pipeline> workers=<n>`（阶段实际运行所在池的 Rayon worker 数：自建池的流水线为配置的池，`forge`/`scale` 为全局池，受 `RAYON_NUM_THREADS` 控制）和 `[Timing] <pipeline> peak_rss_bytes=<n|unavailable>`。峰值 RSS 为从 `VmHWM` 读取的进程高水位，覆盖整个进程而非仅计时阶段，且从不估算：没有 `/proc` 的平台打印 `unavailable`。只为已完成的阶段输出计时行；出错时直接返回，不伪造测量值。

| 流水线 | 阶段（按顺序） | 最后一行 |
|---|---|---|
| `split-filter` | `load`、`split`、`metrics`、`filter`、`write_stl`、`write_report` | `total_in_pool` |
| `pack`（旧版 `packing:`） | `load`（目标、输入、拆分、过滤）、`pack_loop`、`merge_orient`、`write_stl`、`report`（摘要及分布 CSV） | `total_in_pool` |
| `pack`（`placement:`） | `load`（形状库、尺寸、孔隙）、`plan`（多重集与首次报告）、`place`（放置与补充）、`write_outputs`、`report` | `total_in_pool` |
| `optimize` | `load`、`prepare_evaluator`、`target_s2`、`input_s2`、`prune`、`anneal`、`final_s2`、`write_stl`、`write_history` | `total_in_pool` |
| `measure` | `load`、`split_vf`、`s2_exact` 和/或 `s2_monte_carlo`、`write` | `total_in_pool` |
| `forge` | `load`、`vf_before`、`transform`、`vf_after`、`orient_shift`、`write_stl`、`write_report` | `total` |
| `scale` | `load`、`stats_before`、`transform`、`orient_stats_after`、`write_stl` | `total` |
| `render` | `load`、`prepare`、`render_and_backend`、`encode_write` | `total_in_pool` |
| `mesh-render` | `load`、`scene`、`cameras`；尝试 GPU 时有 `gpu_render_write`；CPU 渲染时有 `cpu_prepare`、`cpu_render`、`encode_write`（各视图累计；第 i 个视图渲染与第 i-1 个视图写出相互重叠）及 `cpu_render_write_wall`（二者的墙钟时间） | `total_in_pool` |
| `crop` | `load`、`background`、`pca`、`transform_and_backend`、`trim`、`encode_write`（名称不变） | `total_in_pool` |

`total_in_pool` 不含 CLI 解析、配置加载和线程池创建；`scripts/perf_matrix.py` 中的 `process_wall` 度量整个进程。计时行只写到 stdout，从不进入输出文件、记录或报告，因此输出与 placement 确定性不变。旧版 `pack` 和 `optimize` 另外打印 `[GridStats]` 行（见 `pipeline-packing.md` 和 `pipeline-optimize.md`）。

### 基准矩阵脚本

`scripts/perf_matrix.py`（python3，仅标准库）按 worker 数（默认 1/2/4/8）运行所选流水线，含 `--warmup` 次冷运行（默认 1，单独报告）和 `--repeats` 次暖运行（默认 5）。每次运行都有独立的配置副本（worker 字段通过 `cpu_max`、`packing.cpu_max`、`optimization.cpu_max`、`measurement.cpu_max`、`render.cpu_max` 设置，placement 用 `--threads`；同时设置 `RAYON_NUM_THREADS` 与 `RUSTMSPT_ACCELERATION=cpu`）和 `--work` 下的独立输出目录，因此不会覆盖 `data/` 下任何文件。optimize、measure、forge、scale 使用在工作目录中生成的小依赖链（pack 再 optimize，各运行一次）。输出：`perf_matrix_raw.json`（每次运行的阶段、报告的 worker 数、峰值 RSS、网格行、返回码、进程墙钟时间）和 `perf_matrix_summary.md`（按阶段和 worker 数：冷运行、暖运行中位数/最小/最大、S(p) = T1 中位数 / Tp 中位数、E(p) = S(p) / 报告的 worker 数、峰值 RSS 中位数）。超过核心数的 worker 数会被钳制 `cpu_max` 的流水线截断，`workers` 列显示实际运行值。“冷”指该配置的第一个进程，并非清空页缓存。`--optimize-iterations` 可缩减 `optimization.max_iterations` 以缩短运行时间。`--large` 加大每次运行的工作量，使 worker 扩展性可测（自带输入让大多数流水线在 3～90 ms 内结束）：旧版 pack 目标 0.15、20,000 次尝试，placement VF 0.30，measure pitch 0.5、40 万 MC 样本，render 4096²，mesh-render 3072²，split-filter/forge/scale 使用 `dense_particles.stl`——一次带种子的 placement（200³ 域、VF 0.30、约 34 MB），作为依赖链步骤只生成一次。它只改变规模与目标，从不改变方法。`--chain DIR` 复用已有的依赖链目录而不重新生成：pack 无种子，两次矩阵只有在共享依赖链时才能公平比较 optimize 或 measure。

### PERF-16 补充（2026-09-25）

void 质心仍保持 `mesh_centroid` 的串行索引顺序累加，但当仿射变换走串行分支时在同一遍内求和（`map_vertices_centroid`，逐位一致）。未把 ROI 输出平移融合进 FFD 遍历：`vf_after` 和朝向修正读取的是未平移网格，而在平移后坐标上裁剪不能保证逐位一致。scale 的前后体积与包围盒扫描保留：`|f|^3 * V` 不是同一浮点求和，缩放包围盒捷径只能省一次顶点遍历。
