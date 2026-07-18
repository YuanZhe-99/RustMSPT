# 配置参考（`src/config/`）

本模块定义了每个 RustMSPT 流水线所使用的、可从 YAML 反序列化的配置结构体。每个流水线（forging、scaling、measurement、optimization、packing、crop、split/filter）都拥有自己的顶层 `*Config` 结构体，该结构体组合了一个 `input`、一个 `output`，以及一个承载流水线专属参数的 `*Params` 结构体。所有结构体都派生了 `serde::Deserialize`，并通过 `load_yaml` 从 YAML 加载（参见[函数](#函数)）。少数字段使用了 `deserialize.rs` 中定义的自定义 `deserialize_with` 辅助函数，以便整数字段能同时接受数值和字符串两种 YAML 表示形式（例如 `"1_000_000"` 或 `1000000`）。

以下按源文件对结构体定义进行分组，顺序与任务分配中列出的一致。字段表中标注了 Rust 类型、（当与字段名不同时的）YAML 键（通过 `r#type`/`r#box` 原始标识符或 `#[serde(rename)]`）、`#[serde(default = ...)]` 生效时的默认值，以及基于该字段在代码库其他地方如何被使用而给出的简短含义描述。

## `mod.rs` — 共享的输入/输出/box 结构体

`src/config/mod.rs` 重新导出每个流水线的配置类型，并定义了若干在各流水线间共享、包装单一路径或维度列表的小结构体。

### `InputStl`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `stl_path` | `String` | `stl_path` | — | 输入 STL 网格文件的路径。 |

### `InputPath`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `path` | `String` | `path` | — | 通用输入路径（用于 packing/split-filter，此时输入不必是单个 STL）。 |

### `OutputStl`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `stl_path` | `String` | `stl_path` | — | 写入输出 STL 网格文件的路径。 |

### `OutputPath`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `path` | `String` | `path` | — | 通用输出路径。 |

### `BoxConfig`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `dimensions` | `Vec<f64>` | `dimensions` | — | 要么是 3 个元素的尺寸（包围盒置于原点），要么是 6 个元素的 `[min_x, min_y, min_z, max_x, max_y, max_z]`。由 `parse_box_dimensions` 解析为 `BoundingBox`。 |

## `acceleration.rs` — `AccelerationConfig`

嵌入在 `MeasurementParams` 与 `OptimizationParams` 中的共享子配置，用于控制 CPU/GPU 计算后端的选择。

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `mode` | `AccelerationMode` | `mode` | `AccelerationMode::Auto` | 计算模式：`auto`、`cpu` 或 `gpu`（参见 `src/compute/backend.rs`）。 |
| `backend` | `String` | `backend` | `"wgpu"` | 要使用的 GPU 后端名称。 |
| `cpu_fallback` | `bool` | `cpu_fallback` | `true` | 当 GPU 加速不可用或失败时，是否回退到 CPU 执行。 |
| `gpu_min_voxels` | `usize` | `gpu_min_voxels` | `250_000` | 低于该体素网格规模时，GPU 加速的开销不值得（由 `auto` 模式启发式使用）。 |
| `gpu_memory_limit_mb` | `Option<u64>` | `gpu_memory_limit_mb` | `None` | 对 GPU 内存使用的可选上限，单位为兆字节。 |
| `gpu_prefer_power` | `bool` | `gpu_prefer_power` | `false` | 在设备选择时，是否优先选择高性能（独立）GPU 适配器而非低功耗/集成适配器。 |
| `gpu_precision` | `String` | `gpu_precision` | `"f32"` | GPU 计算着色器所请求的浮点精度。 |

`AccelerationConfig` 还实现了 `Default`（镜像与 `#[serde(default = ...)]` 函数相同的默认值），因此它可以在 YAML 中完全省略。

#### default_backend
`fn default_backend() -> String` — `src/config/acceleration.rs:21`。`backend` 的 serde 默认值函数：返回 `"wgpu"`。无副作用。

#### default_true
`fn default_true() -> bool` — `src/config/acceleration.rs:25`。`cpu_fallback` 的 serde 默认值函数：返回 `true`。无副作用。

#### default_gpu_min_voxels
`fn default_gpu_min_voxels() -> usize` — `src/config/acceleration.rs:29`。`gpu_min_voxels` 的 serde 默认值函数：返回 `250_000`。无副作用。

#### default_gpu_precision
`fn default_gpu_precision() -> String` — `src/config/acceleration.rs:33`。`gpu_precision` 的 serde 默认值函数：返回 `"f32"`。无副作用。

#### AccelerationConfig::default
- **签名：** `fn default() -> Self`（`impl Default for AccelerationConfig`）
- **源码位置：** `src/config/acceleration.rs:38`
- **用途：** Rust 层面（非 serde）的默认实现，与上述 `#[serde(default = ...)]` 辅助函数的值一致，使得 `AccelerationConfig::default()` 可以在反序列化之外使用（例如测试中手动构造配置结构体时）。
- **返回值：** `AccelerationConfig`，其中 `mode: Auto`、`backend: "wgpu"`、`cpu_fallback: true`、`gpu_min_voxels: 250_000`、`gpu_memory_limit_mb: None`、`gpu_prefer_power: false`、`gpu_precision: "f32"`。
- **副作用：** 无。

## `crop.rs`

### `CropRawParams`

仅当 `CropInput.r#type == "raw"` 时才存在；描述如何解读原始二进制体数据文件。

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `width` | `usize` | `width` | — | 切片宽度（体素数）。 |
| `height` | `usize` | `height` | — | 切片高度（体素数）。 |
| `bits` | `u8` | `bits` | — | 每个体素的位深度（例如 8、16、32）。 |
| `signed` | `bool` | `signed` | — | 体素值是否为有符号整数。 |
| `byte_order` | `Option<String>` | `byte_order` | — | 原始数据的字节序（例如 `"little"`/`"big"`）。 |

### `CropInput`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `r#type` | `String` | `type` | — | 输入格式判别标识（例如 `"raw"`、图像堆栈文件夹等）。 |
| `path` | `String` | `path` | — | 输入体数据/图像数据的路径。 |
| `slice_start` | `Option<i32>` | `slice_start` | `None` | 要包含的第一个切片索引（通过 `deserialize_option_i32_flexible` 解析，接受数值或字符串形式的 YAML 值，允许负索引表示“从末尾算起”）。 |
| `slice_end` | `Option<i32>` | `slice_end` | `None` | 要包含的最后一个切片索引（与 `slice_start` 相同的灵活解析方式）。 |
| `raw` | `Option<CropRawParams>` | `raw` | — | 原始格式解码参数；当 `r#type == "raw"` 时必填。 |

### `CropOutput`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `path` | `String` | `path` | — | 裁剪后切片的输出路径/文件夹。 |
| `folder_prefix` | `Option<String>` | `folder_prefix` | — | 输出切片文件的文件名前缀。 |
| `folder_extension` | `Option<String>` | `folder_extension` | — | 输出切片文件的扩展名。 |

### `CropConfig`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `input` | `CropInput` | `input` | — | 输入体数据描述。 |
| `output` | `CropOutput` | `output` | — | 输出目标描述。 |
| `interpolation` | `Option<String>` | `interpolation` | — | 裁剪时重采样所用的插值方法（如有）。 |
| `edge_trim` | `Option<i32>` | `edge_trim` | — | 裁剪后从每条边裁去的体素/像素数。 |

## `deserialize.rs` — 灵活解析辅助函数

这不是结构体——该文件提供了普通函数与 `deserialize_with` 辅助函数，供其他配置结构体使用，以便在 YAML 中同时接受整数的数值形式与带下划线分隔的字符串形式（例如以 YAML 字符串形式给出的 `1_000_000`）。各函数的完整文档见下方[函数](#函数)一节。

## `forging.rs`

### `ForgingParams`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `input_stl_path` | `String` | `input_stl_path` | — | 待锻造/变形的输入 STL 网格路径。 |
| `output_stl_path` | `Option<String>` | `output_stl_path` | — | 写入锻造后输出 STL 的路径。 |
| `compression_ratio` | `Option<f64>` | `compression_ratio` | — | 沿 `compression_axis` 施加的压缩比例。 |
| `compression_axis` | `Option<String>` | `compression_axis` | — | 施加压缩所沿的坐标轴（例如 `"x"`、`"y"`、`"z"`）。 |
| `orient_to_positive_volume` | `Option<bool>` | `orient_to_positive_volume` | — | 处理前是否重新定向网格，使其带符号体积为正。 |
| `bulge_factor` | `Option<f64>` | `bulge_factor` | — | 用于模拟体积守恒变形而施加的侧向鼓起量。 |
| `roi_bounding_box` | `Option<Vec<f64>>` | `roi_bounding_box` | — | 感兴趣区域包围盒（3 或 6 元素形式，约定与 `BoxConfig.dimensions` 相同），限制锻造施加的范围。 |
| `mesh_type` | `Option<String>` | `mesh_type` | — | 用于选择锻造策略的网格分类/类型提示。 |
| `void_densification` | `Option<f64>` | `void_densification` | — | 锻造过程中控制内部孔隙致密化程度的因子。 |

### `ForgingConfig`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `forging` | `ForgingParams` | `forging` | — | 顶层包装；整个 YAML 文档是单个 `forging:` 代码块。 |

## `measurement.rs`

### `MeasurementParams`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `stl_path` | `String` | `stl_path` | — | 待测量的 STL 网格路径。 |
| `bounding_box` | `Option<Vec<f64>>` | `bounding_box` | — | 可选的显式包围盒，测量在其范围内进行（3 或 6 元素形式）。 |
| `stl_bounding_box` | `Option<Vec<f64>>` | `stl_bounding_box` | — | 描述 STL 自身范围的可选显式包围盒，用于代替从网格几何重新计算。 |
| `r_max` | `usize` | `r_max` | — | 两点相关函数/S2 测量所用的最大半径（体素单位）。 |
| `voxel_pitch` | `f64` | `voxel_pitch` | — | 用于将网格离散化以进行测量的体素边长。 |
| `mc_method` | `String` | `mc_method` | — | 用于 S2 估计的蒙特卡洛采样方法标识符。 |
| `mc_samples` | `Option<usize>` | `mc_samples` | `None` | 蒙特卡洛采样点数量（通过 `deserialize_option_usize_flexible` 解析，接受数值或带下划线分隔的字符串）。 |
| `cpu_max` | `Option<i32>` | `cpu_max` | `None` | 要使用的最大 CPU 线程/核心数（通过 `deserialize_option_i32_flexible` 解析）。 |
| `output_path` | `String` | `output_path` | — | 写入测量结果的路径。 |
| `acceleration` | `AccelerationConfig` | `acceleration` | `AccelerationConfig::default()` | GPU/CPU 加速设置（参见 `acceleration.rs`）。 |

### `MeasurementConfig`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `measurement` | `MeasurementParams` | `measurement` | — | 顶层包装；整个 YAML 文档是单个 `measurement:` 代码块。 |

## `optimization.rs`

### `TargetConfig`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `r#type` | `String` | `type` | — | 目标判别标识，例如 `"s2_array"` 与 `"stl"`，用来选择使用 `s2_array`/`stl_path` 中的哪一个作为优化目标。 |
| `s2_array` | `Option<Vec<f64>>` | `s2_array` | — | 显式的目标两点相关函数（S2）曲线值。 |
| `stl_path` | `Option<String>` | `stl_path` | — | 目标 STL 网格的路径，其 S2 统计量将作为优化目标。 |
| `stl_bounding_box` | `Option<Vec<f64>>` | `stl_bounding_box` | — | 目标 STL 的可选显式包围盒。 |

### `OptimizationParams`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `max_iterations` | `usize` | `max_iterations` | —（必填） | 最大模拟退火迭代次数。通过 `deserialize_usize_flexible` 解析（接受数值或带下划线分隔的字符串），但没有 `#[serde(default)]`，因此该键仍然是必填的。 |
| `initial_temperature` | `f64` | `initial_temperature` | — | 模拟退火的起始温度。 |
| `cooling_rate` | `f64` | `cooling_rate` | — | 每次迭代/窗口应用的乘性冷却因子。 |
| `adaptive_temp_window` | `Option<usize>` | `adaptive_temp_window` | `None` | 自适应温度控制用于衡量接受率的窗口大小（迭代次数）。 |
| `target_acceptance_low` | `Option<f64>` | `target_acceptance_low` | — | 用于自适应冷却/加热的目标接受率区间下限。 |
| `target_acceptance_high` | `Option<f64>` | `target_acceptance_high` | — | 目标接受率区间上限。 |
| `adaptive_heat_factor` | `Option<f64>` | `adaptive_heat_factor` | — | 当接受率低于 `target_acceptance_low` 时，用于提高温度的因子。 |
| `adaptive_cool_factor` | `Option<f64>` | `adaptive_cool_factor` | — | 当接受率高于 `target_acceptance_high` 时，用于降低温度的因子。 |
| `adaptive_temp_ceiling_factor` | `Option<f64>` | `adaptive_temp_ceiling_factor` | — | 自适应加热可达到的初始温度的最大倍数。 |
| `r_max` | `usize` | `r_max` | — | S2/两点相关函数评估所用的最大半径。 |
| `voxel_pitch` | `f64` | `voxel_pitch` | — | 优化过程中离散化所用的体素边长。 |
| `mc_method` | `String` | `mc_method` | — | 蒙特卡洛采样方法标识符。 |
| `mc_samples` | `usize` | `mc_samples` | —（必填） | 每次 S2 评估的蒙特卡洛采样数。通过 `deserialize_usize_flexible` 解析。 |
| `max_translation` | `f64` | `max_translation` | — | 退火提议分布的单步最大平移幅度。 |
| `max_rotation_deg` | `f64` | `max_rotation_deg` | — | 退火提议分布的单步最大旋转角度（度）。 |
| `min_neighbor_distance` | `Option<f64>` | `min_neighbor_distance` | — | 相邻颗粒/特征之间允许的最小距离。 |
| `mode` | `Option<u8>` | `mode` | — | 控制优化器行为的数值模式选择器（语义由使用该配置的流水线定义）。 |
| `min_boundary_dist` | `Option<f64>` | `min_boundary_dist` | — | 距离域边界所允许的最小距离。 |
| `min_cross_boundary_depth` | `Option<f64>` | `min_cross_boundary_depth` | — | 特征跨越边界时所允许的最小穿透深度。 |
| `prune_enabled` | `Option<bool>` | `prune_enabled` | — | 是否启用对拟合较差的解元素的周期性剪枝。 |
| `prune_tolerance` | `Option<f64>` | `prune_tolerance` | — | 用于判定某元素是否被剪枝的容差阈值。 |
| `prune_max_rounds` | `Option<usize>` | `prune_max_rounds` | `None` | 要运行的最大剪枝轮数（灵活 usize 解析）。 |
| `prune_eval_samples` | `Option<usize>` | `prune_eval_samples` | `None` | 剪枝过程中用于评估候选项的样本数（灵活 usize 解析）。 |
| `cpu_max` | `Option<i32>` | `cpu_max` | `None` | 要使用的最大 CPU 线程数（灵活 i32 解析）。 |
| `orient_to_positive_volume` | `Option<bool>` | `orient_to_positive_volume` | — | 是否重新定向输入网格，使带符号体积为正。 |
| `rotation_mode` | `Option<String>` | `rotation_mode` | — | 旋转策略标识符（例如自由旋转 vs. 受限的单轴旋转）。 |
| `rotation_axis_vector` | `Option<Vec<f64>>` | `rotation_axis_vector` | — | 固定旋转轴向量，当 `rotation_mode` 将旋转限制在单一轴上时使用。 |
| `islands` | `Option<usize>` | `islands` | `None` | 岛屿模型并行退火所用的并行“岛屿”种群数量（灵活 usize 解析）。 |
| `migration_interval` | `Option<usize>` | `migration_interval` | `None` | 各岛屿间迁移事件之间的迭代次数间隔（灵活 usize 解析）。 |
| `acceleration` | `AccelerationConfig` | `acceleration` | `AccelerationConfig::default()` | GPU/CPU 加速设置。 |

### `OptimizationConfig`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `input` | `InputStl` | `input` | — | 要优化的输入 STL 网格。 |
| `output` | `OutputPath` | `output` | — | 优化结果的输出目标。 |
| `target` | `TargetConfig` | `target` | — | 优化目标（S2 曲线或参考 STL）。 |
| `r#box` | `BoxConfig` | `box` | — | 域包围盒。 |
| `optimization` | `OptimizationParams` | `optimization` | — | 优化器参数。 |

## `packing.rs`

### `PackingFilters`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `min_volume` | `Option<f64>` | `min_volume` | — | 可接受的最小颗粒体积；更小的颗粒会被过滤掉。 |
| `max_aspect_ratio` | `Option<f64>` | `max_aspect_ratio` | — | 颗粒可接受的最大长宽比。 |
| `max_sharpness_ratio` | `Option<f64>` | `max_sharpness_ratio` | — | 颗粒可接受的最大尖锐度比。 |

### `PackingParams`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `target_volume_fraction` | `f64` | `target_volume_fraction` | — | 堆积体应达到的目标固相体积分数。 |
| `mode` | `u8` | `mode` | — | 数值型堆积模式选择器。 |
| `max_attempts` | `usize` | `max_attempts` | — | 每个颗粒放置失败前的最大尝试次数。 |
| `min_neighbor_distance` | `Option<f64>` | `min_neighbor_distance` | — | 相邻颗粒之间允许的最小距离。 |
| `min_boundary_dist` | `Option<f64>` | `min_boundary_dist` | — | 距离域边界所允许的最小距离。 |
| `min_cross_boundary_depth` | `Option<f64>` | `min_cross_boundary_depth` | — | 颗粒跨越边界时所允许的最小穿透深度。 |
| `filters` | `Option<PackingFilters>` | `filters` | — | 可选的颗粒接受过滤条件（见上方 `PackingFilters`）。 |
| `cpu_max` | `Option<i32>` | `cpu_max` | `None` | 要使用的最大 CPU 线程数（通过 `deserialize_option_i32_flexible` 解析）。 |
| `orient_to_positive_volume` | `Option<bool>` | `orient_to_positive_volume` | — | 是否重新定向颗粒网格，使带符号体积为正。 |
| `rotation_mode` | `Option<String>` | `rotation_mode` | — | 颗粒放置所用的旋转策略标识符。 |
| `rotation_axis_vector` | `Option<Vec<f64>>` | `rotation_axis_vector` | — | 固定旋转轴向量，当 `rotation_mode` 将旋转限制在单一轴上时使用。 |
| `target_diameter_distribution_csv` | `Option<String>` | `target_diameter_distribution_csv` | `None` | 描述目标颗粒粒径（孔径）分布的 CSV 文件路径，堆积过程会向该分布靠拢。此为伴随目标粒径分布引导功能新增的字段。 |
| `target_mean_sphericity` | `Option<f64>` | `target_mean_sphericity` | `None` | 堆积后颗粒群体应逼近的目标平均球形度值。新增字段，与 `target_diameter_distribution_csv` 配对使用。 |
| `mean_sphericity_tolerance` | `Option<f64>` | `mean_sphericity_tolerance` | `None` | 在堆积器判定颗粒群体球形度超出容差之前，允许偏离 `target_mean_sphericity` 的幅度。新增字段。 |

> **另请参阅：** `../algorithms/packing-target-diameter-distribution.md`，了解堆积流水线（`src/pipeline/pack.rs`、`src/pipeline/pack_targets.rs`）如何使用 `target_diameter_distribution_csv`、`target_mean_sphericity` 和 `mean_sphericity_tolerance` 来引导颗粒选择，使其趋向目标孔隙/粒径分布与球形度。

### `PackingConfig`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `input` | `InputPath` | `input` | — | 输入路径（例如候选颗粒 STL 所在的文件夹）。 |
| `output` | `OutputPath` | `output` | — | 堆积结果的输出目标。 |
| `r#box` | `BoxConfig` | `box` | — | 堆积域包围盒。 |
| `packing` | `PackingParams` | `packing` | — | 堆积算法参数。 |

## `scale.rs`

### `ScalingParams`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `r#type` | `String` | `type` | — | 缩放模式判别标识（例如按倍数缩放 vs. 缩放到目标尺寸/体积）。 |
| `value` | `f64` | `value` | — | 缩放值，其含义取决于 `r#type`。 |
| `orient_to_positive_volume` | `Option<bool>` | `orient_to_positive_volume` | — | 缩放前/后是否重新定向网格，使其带符号体积为正。 |

### `ScaleConfig`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `input` | `InputStl` | `input` | — | 待缩放的输入 STL 网格。 |
| `output` | `OutputStl` | `output` | — | 缩放后 STL 的输出路径。 |
| `scaling` | `ScalingParams` | `scaling` | — | 缩放参数。 |

## `split_filter.rs`

### `SplitFilterOutput`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `folder` | `String` | `folder` | — | 拆分后颗粒文件的输出文件夹。 |
| `prefix` | `String` | `prefix` | — | 拆分输出文件的文件名前缀。 |
| `report_path` | `Option<String>` | `report_path` | — | 写入拆分/过滤操作摘要报告的可选路径。 |

### `SplitFilterVolume`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `mode` | `Option<String>` | `mode` | — | 体积过滤模式（例如绝对范围 vs. 分箱/基于直方图）。 |
| `min` | `Option<f64>` | `min` | — | 可接受的最小颗粒体积。 |
| `max` | `Option<f64>` | `max` | — | 可接受的最大颗粒体积。 |
| `bins` | `Option<usize>` | `bins` | `None` | 当 `mode` 为分箱模式时的直方图区间数（通过 `deserialize_option_usize_flexible` 解析）。 |
| `over_factor` | `Option<f64>` | `over_factor` | — | 应用于体积区间接受度的过采样/容差因子。 |

### `SplitFilterRules`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `enabled` | `Option<bool>` | `enabled` | — | 是否应用过滤规则（相对于仅做拆分）。 |
| `max_aspect_ratio` | `Option<f64>` | `max_aspect_ratio` | — | 拆分后颗粒可接受的最大长宽比。 |
| `max_sharpness_ratio` | `Option<f64>` | `max_sharpness_ratio` | — | 拆分后颗粒可接受的最大尖锐度比。 |
| `volume` | `Option<SplitFilterVolume>` | `volume` | — | 基于体积的过滤规则（见上方 `SplitFilterVolume`）。 |

### `SplitFilterConfig`

| 字段 | Rust 类型 | YAML 键 | 默认值 | 含义 |
|---|---|---|---|---|
| `input` | `InputPath` | `input` | — | 输入路径（待拆分的网格，或网格所在的文件夹）。 |
| `output` | `SplitFilterOutput` | `output` | — | 拆分后颗粒的输出目标。 |
| `filter` | `Option<SplitFilterRules>` | `filter` | — | 应用于拆分颗粒的可选过滤规则。 |

## 索引

| 函数 | 源码位置 | 摘要 |
|---|---|---|
| `load_yaml` | `src/config/mod.rs:47` | 读取一个文件，并将其反序列化为 YAML，得到一个类型化的配置结构体。 |
| `parse_box_dimensions` | `src/config/mod.rs:58` | 将一个 3 或 6 元素的维度切片转换为一个 `BoundingBox`。 |
| `parse_usize_like` | `src/config/deserialize.rs:4` | 从字符串解析出一个 `usize`，同时剥离下划线分隔符。 |
| `deserialize_usize_flexible` | `src/config/deserialize.rs:17` | Serde `deserialize_with` 辅助函数：接受 YAML 数字或数值字符串，解析为 `usize`。 |
| `deserialize_option_usize_flexible` | `src/config/deserialize.rs:39` | Serde `deserialize_with` 辅助函数：接受 YAML 数字/字符串/null，解析为 `Option<usize>`。 |
| `parse_i32_like` | `src/config/deserialize.rs:61` | 从字符串解析出一个 `i32`，同时剥离下划线分隔符。 |
| `deserialize_option_i32_flexible` | `src/config/deserialize.rs:73` | Serde `deserialize_with` 辅助函数：接受 YAML 数字/字符串/null，解析为 `Option<i32>`。 |
| `default_backend` | `src/config/acceleration.rs:21` | `backend` 的 serde 默认值函数：`"wgpu"`。 |
| `default_true` | `src/config/acceleration.rs:25` | `cpu_fallback` 的 serde 默认值函数：`true`。 |
| `default_gpu_min_voxels` | `src/config/acceleration.rs:29` | `gpu_min_voxels` 的 serde 默认值函数：`250_000`。 |
| `default_gpu_precision` | `src/config/acceleration.rs:33` | `gpu_precision` 的 serde 默认值函数：`"f32"`。 |
| `AccelerationConfig::default` | `src/config/acceleration.rs:38` | 与 serde 默认值一致的 Rust 层 `Default` 实现。 |

## 函数

#### load_yaml

- **签名：** `pub fn load_yaml<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T>`
- **源码位置：** `src/config/mod.rs:47`
- **用途：** 从磁盘读取一个文件，并将其内容反序列化为 YAML，得到任何实现了 `Deserialize` 的类型。
- **参数：**
  - `path` — YAML 配置文件的文件系统路径。
- **返回值：** `Result<T>` — 反序列化得到的配置结构体，或一个 `RustMsptError`。
- **副作用：** 从磁盘读取文件（`fs::read_to_string`）。
- **说明：** 若文件缺失/不可读，则透传一个 `Io` 错误；若内容无法反序列化为 `T`，则透传一个 `Yaml` 错误（通过对 `serde_yaml::from_str` 使用 `?`）。

#### parse_box_dimensions

- **签名：** `pub fn parse_box_dimensions(dimensions: &[f64]) -> Result<crate::types::BoundingBox>`
- **源码位置：** `src/config/mod.rs:58`
- **用途：** 将 `BoxConfig` 中的扁平维度数组转换为 `BoundingBox`。
- **参数：**
  - `dimensions` — `f64` 切片；要么是 3 个元素（`[size_x, size_y, size_z]`，通过 `BoundingBox::from_size` 将包围盒置于原点），要么是 6 个元素（`[min_x, min_y, min_z, max_x, max_y, max_z]`，显式的最小/最大角点）。
- **返回值：** `Result<BoundingBox>`。
- **副作用：** 无。
- **说明：** 对于长度既非 3 也非 6 的输入，返回 `RustMsptError::InvalidConfig`。

#### parse_usize_like

`pub fn parse_usize_like(value: &str) -> std::result::Result<usize, String>` — `src/config/deserialize.rs:4`。剥离字符串中的 `_` 分隔符并将其解析为 `usize`，若失败则返回格式化的错误信息。无副作用。

#### deserialize_usize_flexible

- **签名：** `pub fn deserialize_usize_flexible<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<usize, D::Error>`
- **源码位置：** `src/config/deserialize.rs:17`
- **用途：** Serde `deserialize_with` 辅助函数，允许一个 `usize` 字段在 YAML 中以裸整数或（可能带下划线分隔的）字符串形式给出。
- **参数：**
  - `deserializer` — 由派生宏提供的 serde `Deserializer`。
- **返回值：** `Result<usize, D::Error>` — 解析得到的值，或一个自定义的反序列化错误。
- **副作用：** 无。
- **说明：** 内部反序列化为一个无标签的 `enum Value { Num(u64), Str(String) }`；`Num` 会通过 `usize::try_from` 做范围检查，`Str` 会通过 `parse_usize_like` 解析。用于（不带 `Option`）`OptimizationParams::max_iterations` 与 `OptimizationParams::mc_samples`，尽管有灵活解析，这两个字段仍然是必填的，因为没有设置 `#[serde(default)]`。

#### deserialize_option_usize_flexible

- **签名：** `pub fn deserialize_option_usize_flexible<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<usize>, D::Error>`
- **源码位置：** `src/config/deserialize.rs:39`
- **用途：** Serde `deserialize_with` 辅助函数，允许一个 `Option<usize>` 字段在 YAML 中以数字、数值/带下划线的字符串形式给出，或省略/为 null。
- **参数：**
  - `deserializer` — 由派生宏提供的 serde `Deserializer`。
- **返回值：** `Result<Option<usize>, D::Error>`。
- **副作用：** 无。
- **说明：** 用于诸如 `MeasurementParams::mc_samples`、`OptimizationParams::adaptive_temp_window`/`prune_max_rounds`/`prune_eval_samples`/`islands`/`migration_interval`，以及 `SplitFilterVolume::bins` 等字段。始终与 `#[serde(default, deserialize_with = "...")]` 配对使用，因此该键可以完全省略。

#### parse_i32_like

`pub fn parse_i32_like(value: &str) -> std::result::Result<i32, String>` — `src/config/deserialize.rs:61`。剥离字符串中的 `_` 分隔符并将其解析为 `i32`，若失败则返回格式化的错误信息。无副作用。

#### deserialize_option_i32_flexible

- **签名：** `pub fn deserialize_option_i32_flexible<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<i32>, D::Error>`
- **源码位置：** `src/config/deserialize.rs:73`
- **用途：** Serde `deserialize_with` 辅助函数，允许一个 `Option<i32>` 字段在 YAML 中以数字、数值/带下划线的字符串形式给出，或省略/为 null。
- **参数：**
  - `deserializer` — 由派生宏提供的 serde `Deserializer`。
- **返回值：** `Result<Option<i32>, D::Error>`。
- **副作用：** 无。
- **说明：** 内部反序列化为一个无标签的 `enum Value { Num(i64), Str(String) }`，因此与 usize 版本不同，它接受负数（用于 `CropInput` 中的 `slice_start`/`slice_end`，以及 measurement/optimization/packing 中的 `cpu_max` 等字段，此时负值可能带有流水线特定的含义，例如“使用除 N 个核心外的全部核心”）。
