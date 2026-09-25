# 流水线：裁剪与拆分-过滤参考

涵盖 `crop` 流水线（`src/pipeline/crop.rs`）——背景检测、PCA 对齐、旋转+裁剪、CT 体数据的边缘伪影裁剪——以及 `split_filter` 流水线（`src/pipeline/split_filter.rs`）——连通分量拆分加几何/统计颗粒过滤。

## 索引

| 条目 | 位置 | 摘要 |
|---|---|---|
| `gpu_crop_values_supported` | `src/pipeline/crop.rs:25` | Check exact integer representation for GPU interpolation. |
| `CropPipeline::run_in_pool` | `src/pipeline/crop.rs:621` | Execute crop stages within the configured pool and report completed-stage wall times. |
| `CropPipeline`（结构体） | `src/pipeline/crop.rs:14` | 持有裁剪流水线所用的 `CropConfig`。 |
| `InterpolationMode`（枚举） | `src/pipeline/crop.rs:19` | 旋转+裁剪期间使用的最近邻与三线性重采样模式。 |
| `parse_byte_order` | `src/pipeline/crop.rs:34` | 将 `little`/`big`（或 `le`/`be`）解析为 `ByteOrder`。 |
| `parse_interpolation_mode` | `src/pipeline/crop.rs:50` | 将 `nearest`/`trilinear` 解析为 `InterpolationMode`，默认为三线性。 |
| `load_input_volume` | `src/pipeline/crop.rs:71` | 根据配置从原始文件夹或 TIFF/TIFF 文件夹加载输入 CT 体数据。 |
| `voxel_index` | `src/pipeline/crop.rs:105` | 计算 `(x, y, z)` 体素坐标对应的扁平数据索引。 |
| `sample_voxel_or_background` | `src/pipeline/crop.rs:110` | 在整数坐标处读取体素，越界时返回背景值。 |
| `sample_nearest` | `src/pipeline/crop.rs:137` | 在小数源坐标处进行最近邻采样。 |
| `sample_trilinear` | `src/pipeline/crop.rs:145` | 在小数源坐标处进行三线性插值采样。 |
| `stabilize_bound` | `src/pipeline/crop.rs:178` | 将接近整数的浮点数在误差范围内吸附到其精确整数值。 |
| `float_bounds_to_inclusive_i64` | `src/pipeline/crop.rs:188` | 将浮点最小/最大边界转换为闭区间整数 `[start, end]` 范围。 |
| `boundary_non_bg_ratio` | `src/pipeline/crop.rs:201` | 给定厚度的边界壳层中非背景体素所占的比例。 |
| `infer_trim_pixels` | `src/pipeline/crop.rs:242` | 根据边界伪影强度启发式地推断需裁剪的 0/1/2 像素。 |
| `resolve_trim_pixels` | `src/pipeline/crop.rs:260` | 从配置中解析出有效的边缘裁剪像素数，支持 `-1` 表示自动。 |
| `trim_volume_border` | `src/pipeline/crop.rs:287` | 从体数据的 XY 面裁剪固定数量的边界体素。 |
| `detect_background_mode` | `src/pipeline/crop.rs:317` | 将体数据边界上的众数体素值检测为背景值。 |
| `estimate_pca_bbox` | `src/pipeline/crop.rs:351` | 计算 PCA 旋转、质心以及旋转坐标系下的前景包围盒。 |
| `MomentState` | `src/pipeline/crop.rs` | 运行中的计数、均值与中心化二阶矩矩阵。 |
| `MomentState::from_row` | `src/pipeline/crop.rs` | 由整数和得到单个前景行段的精确矩。 |
| `MomentState::merge` | `src/pipeline/crop.rs` | 两个矩状态的 Chan 并行合并。 |
| `pca_frame` | `src/pipeline/crop.rs` | 排序、定号、右手系且近重根特征空间取规范基的 PCA 坐标系。 |
| `projected_bounds` | `src/pipeline/crop.rs` | 固定分块求旋转坐标系下的前景边界。 |
| `foreground_row_blocks` | `src/pipeline/crop.rs` | 固定分块扫描，把连续行段交给累加器。 |
| `estimate_pca_bbox_three_pass` | `src/pipeline/crop.rs` | 仅测试使用的原三遍固定分块 PCA oracle。 |
| `rotate_and_crop` | `src/pipeline/crop.rs:459` | CPU 上、基于 rayon 并行的旋转+裁剪，将体数据重采样为轴对齐输出。 |
| `rotate_and_crop_gpu` | `src/pipeline/crop.rs` | 按预算规划、按输出分块执行的 GPU 旋转裁剪（`gpu` 特性）。 |
| `CropSourceBlock` | `src/pipeline/crop.rs` | 单个输出块可读取的、已裁剪到源体范围的源子块（原点/尺寸）。 |
| `CropTilePlan` | `src/pipeline/crop.rs` | 选定的块形状、块数、保留最大值与逻辑 GPU 峰值字节。 |
| `CropTilePlanError` | `src/pipeline/crop.rs` | 规划拒绝原因，附带所需字节的可选下界。 |
| `crop_tile_source_block` | `src/pipeline/crop.rs` | 块 8 个角点逆映射的源 AABB，加插值 halo 与 f32 误差余量。 |
| `crop_gpu_peak_bytes` | `src/pipeline/crop.rs` | 保留最大源块/输出块、排队上传、staging、参数和守卫字的逻辑峰值。 |
| `for_each_crop_tile` | `src/pipeline/crop.rs` | 按 z、y、x 顺序遍历整个输出的块。 |
| `evaluate_crop_tiling` | `src/pipeline/crop.rs` | 检查某一块形状是否满足预算和设备单缓冲上限。 |
| `plan_crop_gpu_tiles` | `src/pipeline/crop.rs` | 选择满足预算与上限的最大 z 板/行/x 段分块。 |
| `CropPipeline::run` | `src/pipeline/crop.rs:598` | 编排加载 → 背景检测 → PCA 包围盒 → 旋转+裁剪（GPU 或 CPU）→ 边缘裁剪 → 保存 TIFF。 |
| `SplitFilterPipeline`（结构体） | `src/pipeline/split_filter.rs:13` | 持有拆分-过滤流水线所用的 `SplitFilterConfig`。 |
| `VolumeStats`（结构体） | `src/pipeline/split_filter.rs:18` | 保留颗粒体积的最小/最大/均值/中位数汇总。 |
| `volume_stats_for_kept` | `src/pipeline/split_filter.rs:26` | 在 `keep` 标志为真的颗粒上计算 `VolumeStats`。 |
| `count_kept` | `src/pipeline/split_filter.rs:54` | 统计 `keep` 布尔切片中 `true` 的条目数。 |
| `report_step` | `src/pipeline/split_filter.rs:59` | 为某一过滤步骤追加一行前/后/移除数量的汇总。 |
| `append_volume_histogram` | `src/pipeline/split_filter.rs:71` | 追加一份体积值的单一文本直方图。 |
| `append_volume_histogram_comparison` | `src/pipeline/split_filter.rs:119` | 追加一份并排显示的前/后体积值文本直方图对比。 |
| `normal_cdf` | `src/pipeline/split_filter.rs:202` | 标准正态分布 CDF，通过 `erf_approx` 计算。 |
| `erf_approx` | `src/pipeline/split_filter.rs:208` | Abramowitz & Stegun 7.1.26 误差函数近似。 |
| `apply_lognormal_rebalance` | `src/pipeline/split_filter.rs:225` | 相对于拟合的对数正态分布，从代表过多的对数体积区间中剔除多余颗粒。 |
| `SplitFilterPipeline::run` | `src/pipeline/split_filter.rs:300` | 编排拆分 → 长宽比/尖锐度/体积过滤 → 保存 STL → 报告生成。 |

---

## `pipeline/crop.rs`

CT 体数据裁剪流水线。加载体数据、检测背景强度、计算基于 PCA 的旋转以将前景主轴与坐标轴对齐、将体数据重采样到该旋转+裁剪后的坐标系中、可选地裁剪残留的边界伪影，并将结果保存为 TIFF。

### 公开条目

#### CropPipeline (struct)

- **源码位置：** `src/pipeline/crop.rs:14`
- **用途：** 封装一个 `CropConfig` 并为 crop 子命令实现 `Pipeline`。
- **字段：** `config: CropConfig`。

#### InterpolationMode (enum)

- **源码位置：** `src/pipeline/crop.rs:19`
- **用途：** 选择将输出体素映射回源坐标时所用的重采样方案。
- **变体：** `Nearest`、`Trilinear`。
- **说明：** `Debug, Clone, Copy, PartialEq, Eq`。配置中未设置时默认为 `Trilinear`（参见 `parse_interpolation_mode`）。

#### CropPipeline::run

- **签名：** `fn run(&self) -> Result<()>`
- **源码位置：** `src/pipeline/crop.rs:559`
- **用途：** 端到端执行完整的裁剪流水线：加载 CT 体数据、检测其背景值、对前景进行 PCA 对齐、旋转并裁剪到一个轴对齐包围盒、可选地裁剪边缘伪影，并保存结果。
- **参数：** 读取 `self.config: CropConfig`——输入类型/路径（原始数据或 TIFF，含可选的切片范围与原始布局说明）、`interpolation` 模式字符串、`edge_trim`（-1/0/1/2），以及输出路径/文件夹前缀/文件夹扩展名。
- **返回值：** 成功时返回 `Ok(())`；配置值有误或裁剪量过大时返回 `RustMsptError::InvalidConfig`；透传 I/O 及网格/体数据错误。
- **副作用：** 从磁盘读取输入体数据（`load_input_volume`）；在 `gpu` 特性下，可能初始化 GPU 流水线并派发一次计算通道；通过 `save_tiff_or_folder_with_ext` 将裁剪后的体数据写为 TIFF（或 TIFF 文件夹）；在每个阶段向标准输出打印 `[Info]`/`[Warning]` 进度行（输入形状、检测到的背景值、插值模式、前景体素数量、旋转后包围盒、整数包围盒、边缘裁剪像素数、输出形状、输出路径）。
- **说明：** 流水线阶段，按顺序：
  1. `load_input_volume`——加载原始数据或 TIFF 输入。
  2. `detect_background_mode`——找出众数边界体素值。
  3. `parse_interpolation_mode`——从配置解析最近邻或三线性模式。
  4. `estimate_pca_bbox`——计算 PCA 旋转、质心以及旋转坐标系下的前景边界。
  5. 旋转+裁剪：遵循 acceleration、环境覆盖、工作量阈值和预算；GPU 不可用或执行失败时仅在 cpu_fallback 允许时回退，否则返回错误。
  6. `resolve_trim_pixels` + `trim_volume_border`——可选的边缘伪影裁剪（当 `edge_trim == -1` 时自动检测）。
  7. `save_tiff_or_folder_with_ext`——写出输出结果。
- **另请参阅：** `estimate_pca_bbox`、`rotate_and_crop`、`rotate_and_crop_gpu`；算法细节见 [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md)；GPU 派发细节见 [gpu.md](gpu.md)。

> **算法：** 完整的 PCA 对齐与裁剪算法描述见 [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md)（涵盖 `estimate_pca_bbox`、`rotate_and_crop`，以及 `CropPipeline::run` 如何组合它们）。

### 私有辅助函数

#### parse_byte_order

- **签名：** `fn parse_byte_order(value: Option<&str>) -> Result<ByteOrder>`
- **源码位置：** `src/pipeline/crop.rs:25`
- **用途：** 将原始体数据的字节序配置字符串解析为 `io::ByteOrder`。
- **参数：** `value`——`"little"`/`"le"` 或 `"big"`/`"be"`（不区分大小写，自动去除空白）；为 `None` 时默认为 `"little"`。
- **返回值：** `Ok(ByteOrder::LittleEndian)`、`Ok(ByteOrder::BigEndian)`，或对其他任何字符串返回 `Err(InvalidConfig)`。
- **副作用：** 无。

#### parse_interpolation_mode

- **签名：** `fn parse_interpolation_mode(value: Option<&str>) -> Result<InterpolationMode>`
- **源码位置：** `src/pipeline/crop.rs:36`
- **用途：** 解析裁剪配置中的插值模式字符串。
- **参数：** `value`——`"nearest"` 或 `"trilinear"`（不区分大小写，自动去除空白）；为 `None` 时默认为 `"trilinear"`。
- **返回值：** `Ok(InterpolationMode)`，或对无法识别的字符串返回 `Err(InvalidConfig)`。
- **副作用：** 无。

#### load_input_volume

- **签名：** `fn load_input_volume(config: &CropConfig) -> Result<Volume3D>`
- **源码位置：** `src/pipeline/crop.rs:52`
- **用途：** 根据 `config.input.type` 加载裁剪流水线的输入体数据。
- **参数：** `config`——完整的 `CropConfig`；读取 `input.type`（`"raw"` 或 `"tiff"`/`"tif"`）、`input.path`、`input.slice_start`/`slice_end`（默认为 -1，表示“不限制”），以及针对原始输入的 `input.raw`（宽/高/位深/是否有符号/字节序）。
- **返回值：** 一个已加载的 `Volume3D`。
- **副作用：** 从磁盘读取文件（原始切片文件夹，或 TIFF 文件/文件夹）。
- **说明：** 若 `input.type=raw` 但缺少 `input.raw`，或 `input.type` 既非 `raw` 也非 `tiff`/`tif`，则返回 `RustMsptError::InvalidConfig`。

#### voxel_index

- **简明形式：** `fn voxel_index(width: usize, height: usize, x: usize, y: usize, z: usize) -> usize` —— `src/pipeline/crop.rs:86`。为 `Volume3D` 的行主序/切片主序布局返回扁平数据索引 `z*width*height + y*width + x`。纯函数，无副作用。

#### sample_voxel_or_background

- **签名：** `fn sample_voxel_or_background(volume: &Volume3D, background: i64, x: isize, y: isize, z: isize) -> i64`
- **源码位置：** `src/pipeline/crop.rs:91`
- **用途：** 在整数坐标处读取单个体素，将任何越界坐标视为背景。
- **参数：** `volume`、`background`（填充值）、`x`/`y`/`z`（有符号，可能为负或超出体数据范围）。
- **返回值：** 体素值；若任一坐标为负或 `>=` 对应维度，则返回 `background`。
- **副作用：** 无。

#### sample_nearest

- **签名：** `fn sample_nearest(volume: &Volume3D, background: i64, src_x: f64, src_y: f64, src_z: f64) -> i64`
- **源码位置：** `src/pipeline/crop.rs:106`
- **用途：** 在小数源坐标处进行最近邻重采样。
- **参数：** 源体数据空间中的小数坐标 `src_x/src_y/src_z`。
- **返回值：** 将每个坐标四舍五入到最近整数后委托给 `sample_voxel_or_background`。
- **副作用：** 无。

#### sample_trilinear

- **签名：** `fn sample_trilinear(volume: &Volume3D, background: i64, src_x: f64, src_y: f64, src_z: f64) -> i64`
- **源码位置：** `src/pipeline/crop.rs:114`
- **用途：** 在小数源坐标处进行三线性插值重采样。
- **参数：** 小数坐标 `src_x/src_y/src_z`。
- **返回值：** 周围 8 个体素（每个体素在越界时经 `sample_voxel_or_background` 单独回退为 `background`）的三线性混合值，四舍五入为 `i64`。
- **副作用：** 无。
- **说明：** 标准三线性公式：先沿 x 方向对 4 条边分别插值，再沿 y 方向对得到的 2 个值插值，最后沿 z 方向插值。

#### stabilize_bound

- **简明形式：** `fn stabilize_bound(value: f64, eps: f64) -> f64` —— `src/pipeline/crop.rs:147`。当 `value` 与 `value.round()` 之差不超过 `eps` 时，将其吸附到 `value.round()`；否则原样返回 `value`。用于抵消 PCA 旋转后包围盒坐标（概念上应为整数）的浮点漂移。纯函数。

#### float_bounds_to_inclusive_i64

- **签名：** `fn float_bounds_to_inclusive_i64(min_v: f64, max_v: f64, eps: f64) -> (isize, isize)`
- **源码位置：** `src/pipeline/crop.rs:157`
- **用途：** 将浮点 `[min_v, max_v]` 边界转换为闭区间整数体素范围。
- **参数：** `min_v`/`max_v`——浮点边界（例如来自 `estimate_pca_bbox`）；`eps`——传给 `stabilize_bound` 的吸附容差。
- **返回值：** `(start, end)`，其中 `start = floor(stabilize(min_v))`，`end = ceil(stabilize(max_v))`。
- **副作用：** 无。
- **说明：** 该模块中始终以 `eps = 1e-3` 调用。

#### boundary_non_bg_ratio

- **签名：** `fn boundary_non_bg_ratio(volume: &Volume3D, background: i64, thickness: usize) -> f64`
- **源码位置：** `src/pipeline/crop.rs:170`
- **用途：** 衡量体数据外壳（给定厚度）中有多少比例为非背景体素——作为遗留旋转/重采样边缘伪影的代理指标。
- **参数：** `volume`、`background`、`thickness`——从每个面算起的壳层厚度（体素为单位）。
- **返回值：** `[0, 1]` 范围内的比例；若 `thickness == 0` 或体数据没有壳层体素，则为 `0.0`。
- **副作用：** 无。遍历体数据中的每个体素，将其分类为若位于任一 6 个面的 `thickness` 范围内则属于“壳层”。
- **说明：** 未做并行化；`infer_trim_pixels` 每次对每个体数据调用两次（厚度分别为 1 和 2），因此每次调用的开销为 O(体数据大小)。

#### infer_trim_pixels

- **签名：** `fn infer_trim_pixels(volume: &Volume3D, background: i64) -> usize`
- **源码位置：** `src/pipeline/crop.rs:211`
- **用途：** 基于边界壳层伪影强度启发式地决定需裁剪多少像素的 XY 边界。
- **参数：** `volume`（通常为旋转+裁剪后的输出）、`background`。
- **返回值：** 若 `r1 > 0.08 && r2 > 0.04` 则为 `2`；否则若 `r1 > 0.03` 则为 `1`；否则为 `0`，其中 `r1`/`r2` 分别为厚度 1 和 2 时的 `boundary_non_bg_ratio`。
- **副作用：** 无。
- **说明：** 这是配置中 `edge_trim` 的 `-1`（“自动”）行为，由 `resolve_trim_pixels` 消费。

#### resolve_trim_pixels

- **签名：** `fn resolve_trim_pixels(config_value: Option<i32>, volume: &Volume3D, background: i64) -> Result<usize>`
- **源码位置：** `src/pipeline/crop.rs:229`
- **用途：** 从 `edge_trim` 配置值解析出有效的边缘裁剪像素数，支持自动检测。
- **参数：** `config_value`——`-1`（通过 `infer_trim_pixels` 自动）、`0`、`1` 或 `2`；为 `None` 时默认为 `0`。`volume`/`background`——同时用于自动推断和限幅。
- **返回值：** `Ok(usize)`，限幅为 `min(requested, min(width-1, height-1)/2, 2)`；对 `{-1, 0, 1, 2}` 之外的任何值返回 `Err(InvalidConfig)`。
- **副作用：** 无。
- **说明：** 该限幅确保 `trim_volume_border` 永远不会收到一个会消除整个 XY 范围的裁剪值。

#### trim_volume_border

- **签名：** `fn trim_volume_border(volume: Volume3D, trim: usize) -> Result<Volume3D>`
- **源码位置：** `src/pipeline/crop.rs:252`
- **用途：** 从体数据的四条 XY 面边界（不包括 Z/深度面）剥离 `trim` 个体素。
- **参数：** `volume`、`trim`——从四个 X/Y 边各自移除的像素数。
- **返回值：** 当 `trim == 0` 时返回 `Ok(volume)` 不变；否则返回一个更小的新 `Volume3D`，`width -= 2*trim`、`height -= 2*trim`，`depth` 不变；若 `width <= 2*trim || height <= 2*trim` 则返回 `Err(InvalidConfig)`。
- **副作用：** 无（会分配一个新的数据缓冲区）。
- **说明：** 深度（Z）方向永不裁剪——仅裁剪 X/Y（面内）边界，这与该流水线中的边缘伪影源于 XY 旋转/重采样、而非切片截断这一事实相符。

#### detect_background_mode

- **签名：** `fn detect_background_mode(volume: &Volume3D) -> i64`
- **源码位置：** `src/pipeline/crop.rs:296`
- **用途：** 将 CT 体数据边界面上出现频率最高的体素值检测为背景强度。
- **参数：** `volume`。
- **返回值：** 众数边界体素值；若体数据没有边界体素（退化/空体数据）则为 `0`。
- **副作用：** 无。每个边界体素只计数一次；符合条件的 8/16 位图像使用有界稠密计数，其他情况使用稀疏表，详见下方契约。
- **说明：** 假定背景在边界上占主导——对于标本不触及体数据边缘的 CT 扫描而言是合理假设。并列时确定性选择最小体素值。

#### estimate_pca_bbox

- **签名：** `fn estimate_pca_bbox(volume: &Volume3D, background: i64) -> Result<(Matrix3<f64>, Vector3<f64>, Vector3<f64>, Vector3<f64>, usize)>`
- **源码位置：** `src/pipeline/crop.rs:330`
- **用途：** 计算一个主成分旋转，将前景的主轴与坐标轴对齐，并求出该旋转坐标系下前景的包围盒。
- **参数：** `volume`、`background`——要排除为背景的值。
- **返回值：** `Ok((rot, centroid, min_v, max_v, count))`：
  - `rot: Matrix3<f64>`——来自 `pca_frame` 的正交旋转矩阵（各列为协方差特征向量，按特征值降序排序、定号，近重根特征空间取规范基，并强制为右手系）。
  - `centroid: Vector3<f64>`——前景体素的平均位置。
  - `min_v`/`max_v: Vector3<f64>`——旋转坐标系下的前景包围盒（即对每个前景体素 `p`，计算 `rot^T * (p - centroid)`）。
  - `count: usize`——前景体素数量。
  - 若未找到前景体素（所有体素均等于 `background`），则返回 `Err(InvalidConfig)`。
- **副作用：** 无。两遍：(1) 单遍 count/mean/M2 矩统计（固定分块内以 Chan 公式合并逐行精确整数矩，块间按序合并），(2) `projected_bounds`。
- **说明：** 见下文“单遍矩统计与规范 PCA 坐标系”。分块与结果不受 worker 数影响。
- **另请参阅：** 算法说明见 [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md)。

> **算法：** 见 [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md)。

#### rotate_and_crop

- **签名：** `fn rotate_and_crop(volume: &Volume3D, background: i64, rot: &Matrix3<f64>, centroid: &Vector3<f64>, min_v: &Vector3<f64>, max_v: &Vector3<f64>, interpolation_mode: InterpolationMode) -> Volume3D`
- **源码位置：** `src/pipeline/crop.rs:432`
- **用途：** CPU 旋转+裁剪：将源体数据重采样为一个新的轴对齐体数据，恰好覆盖旋转坐标系下的前景包围盒。
- **参数：** `volume`、`background`；来自 `estimate_pca_bbox` 的 `rot`/`centroid`；旋转坐标系边界 `min_v`/`max_v`；`interpolation_mode`（`Nearest` 或 `Trilinear`）。
- **返回值：** 一个大小为 `(x1-x0+1, y1-y0+1, z1-z0+1)`（每个维度至少取 1）的新 `Volume3D`，初始化为 `background`，通过对每个输出体素做反向映射到源空间来填充。
- **副作用：** 无（纯计算）。使用 `float_bounds_to_inclusive_i64`（`eps = 1e-3`）确定整数输出范围。
- **说明：** 通过 rayon 的 `par_chunks_mut` 在扁平输出缓冲区上按 Z 切片并行化（每个输出切片一个分块）。对于旋转局部坐标系中的每个输出体素 `(x, y, z)`，对应的源空间坐标为 `rot * local + centroid`，根据 `interpolation_mode` 通过 `sample_nearest` 或 `sample_trilinear` 采样。
- **另请参阅：** `rotate_and_crop_gpu`（GPU 对应实现，`gpu` 特性）；[../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md)。

> **算法：** 见 [../algorithms/pca-volume-alignment-crop.md](../algorithms/pca-volume-alignment-crop.md)。

#### rotate_and_crop_gpu

> **特性门控：** 仅在使用 `--features gpu`（`#[cfg(feature = "gpu")]`）编译时启用。

- **签名：** `fn rotate_and_crop_gpu(volume: &Volume3D, background: i64, rot: &Matrix3<f64>, centroid: &Vector3<f64>, min_v: &Vector3<f64>, max_v: &Vector3<f64>, interpolation_mode: InterpolationMode, budget: Option<u64>) -> std::result::Result<(Volume3D, usize), String>`
- **源码位置：** `src/pipeline/crop.rs`
- **用途：** 按 `plan_crop_gpu_tiles` 选出的输出块执行 GPU 旋转裁剪；每个块只上传其采样可能触及的源子块。
- **参数：** 与 `rotate_and_crop` 相同，另加 `budget` —— 逻辑 GPU 字节预算（`acceleration.gpu_memory_limit_mb × 1 MiB`），`None` 表示无预算。
- **返回值：** `Ok((Volume3D, tiles))`，形状/语义与 CPU 路径相同并附实际派发块数；或初始化、规划（`"... no single minimal tile fits (needs at least N bytes)"`）、派发、回读、halo 守卫失败时的 `Err(String)`。
- **副作用：** 初始化 `GpuVolumeTransformPipeline`，同时按预算和设备单缓冲上限（`min(max_buffer_size, max_storage_buffer_binding_size)`）规划，一次性把源/输出/staging 缓冲预留到计划最大值；随后逐块把子块 `i64 → i32`（带检查）、调用 `transform_tile`、把块内各行写回主机输出。打印 `"[Info] GPU volume transform: {s}s, {src} -> {out}, tiles=N tile=WxHxD peak_bytes=B"`。
- **说明：** 两种插值都与单次 dispatch 的 GPU 路径逐字节一致：着色器按绝对输出索引（`f32(tile_offset + local)`）重算每个体素，并按完整源尺寸判断越界，子块只改变体内数值的读取位置。运行时守卫字会把任何落在已上传子块之外的体内读取变成错误而非数值。无预算且设备上限充足时计划为单块，其子块是输出对应的源 AABB（不一定是整个源体）。最近邻在斜旋转下仅在 f32/f64 坐标恰处 `.5` 平局而舍入不同之处与 CPU 不同；三线性的 f32 混合不宣称与 CPU f64 逐位一致。
- **另请参阅：** `GpuVolumeTransformPipeline` 及底层 WGSL 计算着色器见 [gpu.md](gpu.md)；CPU 回退路径见 `rotate_and_crop`；GPU/CPU 选择逻辑与失败回退行为见 `CropPipeline::run`。

#### Crop GPU 分块规划（`CropSourceBlock`、`CropTilePlan`、`CropTilePlanError`、`crop_tile_source_block`、`crop_gpu_peak_bytes`、`for_each_crop_tile`、`evaluate_crop_tiling`、`plan_crop_gpu_tiles`）

- **源码位置：** `src/pipeline/crop.rs`（未启用 `gpu` 特性也编译：`CropPipeline::run_in_pool` 以计划峰值作为工作集估计）。
- **`crop_tile_source_block(src_dims, rot, centroid, origin, lo, hi) -> Result<CropSourceBlock, String>`** —— 对块的 8 个角点做逆映射（`src = rot * (origin + index) + centroid`，仿射像的 AABB），每轴扩展为 `floor(min − m) .. floor(max + m) + 1`（同时覆盖三线性邻点与最近邻舍入，即 1 体素 halo），再裁剪到源体范围。`m = 16·ε_f32·(Σ|r_ij|·(max|local_j|+1) + |c_i| + 1) + 1e-6` 界定着色器对旋转、质心、原点及乘加的 f32 舍入。所有采样都在源体外的块得到零尺寸子块；非有限坐标报错。旋转会跨越源切片，因此子块绝不按输出 z 范围切取。
- **`crop_gpu_peak_bytes(max_block_voxels, max_tile_voxels) -> Option<u64>`** —— `2·4·block`（缓冲加排队的 `write_buffer` 副本）`+ 4·tile`（输出）`+ 4·tile + 4`（含守卫字的 staging）`+ 160`（参数）`+ 4 + 4`（守卫），全部 checked。
- **`for_each_crop_tile(out_dims, tile_dims, visit)`** —— 按 z、y、x 顺序访问闭区间 `(lo, hi)`；尺寸不整除时尾块更短。
- **`evaluate_crop_tiling(...) -> Result<CropTilePlan, CropTilePlanError>`** —— 在所有块上累计保留最大值（缓冲只预留一次，因此峰值取最大值而非单块），在第一个超出预算、单缓冲上限或 u32 体素索引的块处拒绝。
- **`plan_crop_gpu_tiles(src_dims, rot, centroid, origin, out_dims, budget, buffer_limit) -> Result<CropTilePlan, CropTilePlanError>`** —— 依次尝试整个输出、完整 xy 的 z 板、单切片内的 y 行组、单行内的 x 段；每一层二分查找可行的最大范围，只返回在其全部块上评估通过的计划。仅当单体素 x 段仍放不下时拒绝；错误中的 `needed` 是下界，作为策略估计，使 `resolve_execution` 报告预算拒绝（auto 回退 CPU，gpu 报错）。

---

## `pipeline/split_filter.rs`

连通分量拆分与几何/统计过滤流水线。将一个或多个输入 STL 网格拆分为互不相交的颗粒（“granule”）网格，然后应用一系列可选过滤器（长宽比、尖锐度、体积范围或对数正态重平衡），将幸存颗粒保存为独立的 STL 文件并附带一份文本报告。

### 公开条目

#### SplitFilterPipeline (struct)

- **源码位置：** `src/pipeline/split_filter.rs:13`
- **用途：** 封装一个 `SplitFilterConfig` 并为 split_filter 子命令实现 `Pipeline`。
- **字段：** `config: SplitFilterConfig`。

#### SplitFilterPipeline::run

- **签名：** `fn run(&self) -> Result<()>`
- **源码位置：** `src/pipeline/split_filter.rs:300`
- **用途：** 运行完整的拆分-过滤流水线：加载输入网格、拆分为连通分量颗粒、应用配置的过滤链、将保留下来的颗粒保存为 STL 文件，并写出文本报告。
- **参数：** 读取 `self.config: SplitFilterConfig`——`input.path`（单个 STL 文件或 STL 目录）、`output.folder`/`output.prefix`/`output.report_path`（报告路径默认为 `<output.folder 的父目录>/split_filter_report.txt`），以及可选的 `filter` 部分（`enabled`、`max_aspect_ratio`、`max_sharpness_ratio`、带有 `mode: "range" | "lognormal_rebalance" | "none"` 的 `volume` 子配置）。
- **返回值：** 成功时返回 `Ok(())`；若 `output.prefix` 为空则返回 `RustMsptError::InvalidConfig`；若拆分结果为零个颗粒，或所有颗粒都被过滤掉，则返回 `RustMsptError::InvalidMesh`；透传网格/STL 加载与保存过程中的 I/O 错误。
- **副作用：** 从磁盘读取 STL 文件（`load_stl` 或 `load_folder_stls`）；如缺失则创建 `output.folder`（及报告的父目录）；为每个保留颗粒写出一个 STL 文件（命名为 `{prefix}{rank+1}.stl`，按保留顺序从 1 开始编号）；将文本报告写入 `report_path`；向标准输出打印 `[Info]` 汇总行（颗粒数量、输出文件夹/前缀、报告路径）。
- **说明：** 过滤链按顺序应用，每个阶段仅影响仍标记为 `keep` 的颗粒：
  1. **拆分：** 对每个输入网格，`split_mesh_into_granules` 将其分解为连通分量子网格（“颗粒”）。所有输入文件产生的全部颗粒汇集到一个 `Vec<Mesh>` 中。
  2. **长宽比**（`filter.max_aspect_ratio`，可选）：按颗粒从 `mesh_bbox` 计算为 `max_extent / min_extent.max(1e-12)`；超过阈值的颗粒被丢弃。
  3. **尖锐度比**（`filter.max_sharpness_ratio`，可选）：`sharpness = area^3 / (36 * PI * volume^2)`（一种等周不等式风格的形状比；完美球体为 1.0，细长/尖刺形状更大）。`volume <= 1e-12` 的颗粒直接被丢弃（退化网格）；其余超过阈值的颗粒被丢弃。
  4. **体积过滤**（`filter.volume`，可选）：`mode = "range"` 会丢弃体积落在 `[min, max]` 之外的颗粒（任一边界为负数/未设置时跳过该边界）；`mode = "lognormal_rebalance"` 委托给 `apply_lognormal_rebalance`；`mode = "none"`（或任何其他值）跳过该阶段。
  每个阶段都会向报告追加一行 `report_step`（`before=N, after=M, removed=K`），若该阶段的配置缺失则追加一行 `"skipped"`。若 `filter` 本身为 `None`，或 `filter.enabled == Some(false)`，则完全跳过所有过滤，保留全部颗粒。
  报告中还包含保留颗粒的体积统计（`volume_stats_for_kept`）以及两份文本直方图（`append_volume_histogram`、`append_volume_histogram_comparison`，均固定为 10 个区间）。
- **另请参阅：** `apply_lognormal_rebalance`；`crate::geometry::split_mesh_into_granules`、`mesh_bbox`、`mesh_surface_area`、`mesh_volume`（参见 [geometry-analysis.md](geometry-analysis.md) / [geometry-core.md](geometry-core.md)）。

### 私有辅助函数

#### VolumeStats (struct)

- **源码位置：** `src/pipeline/split_filter.rs:18`
- **用途：** 为报告持有保留颗粒体积的最小/最大/均值/中位数汇总统计。
- **字段：** `min: f64`、`max: f64`、`mean: f64`、`median: f64`。
- **说明：** `Clone, Copy`。

#### volume_stats_for_kept

- **签名：** `fn volume_stats_for_kept(volumes: &[f64], keep: &[bool]) -> Option<VolumeStats>`
- **源码位置：** `src/pipeline/split_filter.rs:26`
- **用途：** 在 `volumes` 中与 `keep` 并行的条目为 `true` 的子集上计算 `VolumeStats`。
- **参数：** `volumes`——按颗粒的体积数组；`keep`——并行的布尔保留掩码。
- **返回值：** 若没有颗粒被保留则为 `None`；否则为 `Some(VolumeStats)`，`min`/`max` 取自排序后的两端极值，`mean` 为算术平均值，`median` 由排序后的仅保留值计算得出（偶数个元素时取中间两个元素的平均）。
- **副作用：** 无（对保留值的本地副本进行排序）。

#### count_kept

- **简明形式：** `fn count_kept(keep: &[bool]) -> usize` —— `src/pipeline/split_filter.rs:54`。返回 `keep` 中 `true` 条目的数量。纯函数。

#### report_step

- **签名：** `fn report_step(lines: &mut Vec<String>, name: &str, before: usize, after: usize)`
- **源码位置：** `src/pipeline/split_filter.rs:59`
- **用途：** 追加一行报告，汇总某个过滤步骤对颗粒数量的影响。
- **参数：** `lines`——要追加的报告缓冲区；`name`——步骤标签；`before`/`after`——该步骤前后的保留颗粒数量。
- **返回值：** 无返回值；就地修改 `lines`。
- **副作用：** 推入一行 `"{name}: before={before}, after={after}, removed={before-after}"`（`removed` 通过 `saturating_sub` 计算，因此不会下溢）。

#### append_volume_histogram

- **签名：** `fn append_volume_histogram(lines: &mut Vec<String>, title: &str, values: &[f64], bins: usize)`
- **源码位置：** `src/pipeline/split_filter.rs:71`
- **用途：** 向报告追加一份 `values` 的单一 ASCII 条形文本直方图。
- **参数：** `lines`——报告缓冲区；`title`——标题行；`values`——要制作直方图的数据；`bins`——请求的区间数，限幅到 `[4, 32]`。
- **返回值：** 无返回值；就地修改 `lines`。
- **副作用：** 若 `values` 为空，追加一行 `"{title}: no data"` 并返回。若所有值相等（范围 `< 1e-12`），则追加一行汇总信息而非完整直方图。否则将值分箱到跨 `[min, max]` 的等宽桶中，并为每个区间追加一行，条形以 `#` 重复表示，最大宽度缩放为 30 个字符（由于 `.max(1)`，即使是空区间条形长度也至少为 1）。
- **说明：** 越界限幅（`b < 0` → `0`，`b >= bins` → `bins-1`）用于防范恰好落在最大值处的浮点边界情况。

#### append_volume_histogram_comparison

- **签名：** `fn append_volume_histogram_comparison(lines: &mut Vec<String>, title: &str, before_values: &[f64], after_values: &[f64], bins: usize)`
- **源码位置：** `src/pipeline/split_filter.rs:119`
- **用途：** 追加一份并排的“前后对比” ASCII 条形直方图，用 `before_values` 的范围为两组数据定义区间边界。
- **参数：** `lines`、`title`；`before_values`/`after_values`——两组数据集（通常为全部体积与保留体积）；`bins`——限幅到 `[4, 32]`。
- **返回值：** 无返回值；就地修改 `lines`。
- **副作用：** 若 `before_values` 为空，追加一行 `"no data"` 并返回。区间边界（`min_v`/`max_v`）始终仅由 `before_values` 推导，因此若 `after_values` 中有值超出该范围，会通过与 `append_volume_histogram` 相同的限幅逻辑落入边界区间。两组条形各自独立缩放（`max_before`、`max_after`），最大宽度各为 16 个字符。
- **说明：** 由于在该流水线的实际用法中 `after_values` 始终是 `before_values` 的子集（保留 ⊆ 全部），其范围实际上永远不会超出 `before_values` 的范围。

#### normal_cdf

- **签名：** `fn normal_cdf(x: f64) -> f64`
- **源码位置：** `src/pipeline/split_filter.rs:202`
- **用途：** 标准正态分布（均值 0，方差 1）累积分布函数。
- **参数：** `x`——标准正态尺度下的输入。
- **返回值：** `0.5 * (1 + erf(x / sqrt(2)))`，位于 `[0, 1]`。
- **副作用：** 无。委托给 `erf_approx`。

#### erf_approx

- **签名：** `fn erf_approx(x: f64) -> f64`
- **源码位置：** `src/pipeline/split_filter.rs:208`
- **用途：** 近似计算高斯误差函数 `erf(x)`。
- **参数：** `x`。
- **返回值：** `erf(x)` 的 `f64` 近似值，精度约为 `1.5e-7`（底层公式的既定误差上限）。
- **副作用：** 无。
- **说明：** 实现 Abramowitz & Stegun 公式 7.1.26，一种有理多项式近似，使用 `t = 1/(1 + 0.3275911*|x|)` 以及一个固定的 5 项 `t` 多项式，结合 `exp(-x^2)` 和 `x` 的符号。这是一种闭式数值近似，而非精确的特殊函数求值——对该流水线的重平衡启发式而言足够，但与真正的 erf 实现并非逐位一致。

#### apply_lognormal_rebalance

- **签名：** `fn apply_lognormal_rebalance(keep: &mut [bool], volumes: &[f64], cfg: &SplitFilterVolume)`
- **源码位置：** `src/pipeline/split_filter.rs:225`
- **用途：** 通过从相对于拟合模型代表过多的区间中随机剔除颗粒，使保留颗粒群体的体积分布更接近拟合的对数正态分布。
- **参数：** `keep`——就地更新的可变保留掩码；`volumes`——按颗粒的体积数组（与 `keep` 并行）；`cfg: &SplitFilterVolume`——读取 `cfg.bins`（默认 12，限幅到 `[4, 64]`）与 `cfg.over_factor`（默认 1.25，下限为 1.0）。
- **返回值：** 无返回值；就地修改 `keep`（将多余条目设为 `false`）。
- **副作用：** 使用 `rand::thread_rng()` 在丢弃前对代表过多的区间内部进行洗牌，因此具体丢弃哪些颗粒在多次运行之间是不确定的（未设种子）。
- **说明：** 算法：
  1. 收集当前保留且 `volume > 0.0` 的颗粒作为候选。若候选数少于 4 个，则直接跳出（不做任何操作）——数据不足以拟合分布。
  2. 对每个候选取 `log_vals = ln(volume)`；拟合 `mu`（均值）与 `sigma`（总体标准差，下限为 `1e-9`）——即对体积做最大似然对数正态拟合。
  3. 若对数体积范围退化（`< 1e-12`），则跳出。
  4. 将候选按其 `ln(volume)` 分箱到跨 `[min_log, max_log]` 的 `bins` 个等宽区间中。
  5. 对每个区间，计算若使用 `(mu, sigma)` 的正态分布，理论上会落入该区间对数体积范围内的总体*期望*占比（`normal_cdf(hi) - normal_cdf(lo)`），乘以候选总数得到期望数量，再计算 `allowed = ceil(expected * over_factor)`。
  6. 若某区间内的候选数超过 `allowed`，则对该区间的索引洗牌，并将除前 `allowed` 个之外的其余标记为 `keep[idx] = false`。
- **说明（配置语义）：** `over_factor` 是在开始剔除之前、允许超出理论对数正态隐含数量的宽裕度——`over_factor = 1.0` 会将每个区间精确剔减到拟合模型的期望值；更大的值则允许更多的代表过多情况后才开始丢弃颗粒。这是一种启发式的重平衡工具，而非真正的重采样/拒绝采样算法——它从不向代表不足的区间*添加*颗粒，只从代表过多的区间中移除。


### Crop execution updates (2026-09-18)

`CropPipeline::run` installs a pool bounded by `cpu_max`; `run_in_pool` performs all stages. `gpu_crop_values_supported` rejects lossy integer narrowing before adapter initialization. `acceleration` and the environment override determine the backend, budget and permission to fall back. GPU runtime errors propagate when fallback is forbidden. Resampling uses adaptive slice/row tasks. `trim_volume_border` consumes and compacts the original allocation, preserving depth, numeric type and row order, including zero-copy zero trim.


### Split-filter metric preparation (PERF-15)

`SplitFilterConfig.cpu_max` bounds one pool for loading, splitting, metric preparation, filtering and saving (`-1`/absent: available workers). `prepare_particle_metrics` computes immutable volume/aspect/area records in component order. At least 32 components use indexed parallel collection; smaller sets remain serial. Each component uses the original geometry functions and reduction order. Bbox is only requested by the aspect filter; area is only computed for sharpness candidates that survive the aspect and positive-volume gates. Filtering reads those records in its original order, and lognormal RNG/deletions remain serial. Before/after reporting shares the immutable volume array instead of cloning it. STL writes run in batches of at most two in the same pool, with names and errors consumed in rank order; an error may leave another file in its current batch written.


`foreground_blocks` maps foreground voxels in fixed 65,536-voxel chunks, collecting partials in block order. `estimate_pca_bbox` 用 `foreground_row_blocks` 统计矩，用 `foreground_blocks`（其逐体素包装）求投影边界；三遍形式只作为测试 oracle 保留。 `detect_background_mode` scans boundary faces only and resolves tied counts by smallest value. The original serial PCA is retained only under tests for numerical comparison.

PCA task grain: for the parallel branch, `foreground_blocks` sets a minimum number of blocks per Rayon job using a workload-derived task budget, `min(workers, ceil(N/1,048,576))`; fixed block boundaries and ordered collection remain unchanged.

### Crop 阶段计时（2026-09-23）

已完成的阶段输出 `[Timing] crop stage=<name> seconds=<wall_seconds>`，名称为 `load`、`background`、`pca`、`transform_and_backend`、`trim`、`encode_write`、`total_in_pool`。`transform_and_backend` 包含输出尺寸、策略/设备初始化、GPU 传输与回读或 CPU 重采样、允许的回退，不是 GPU kernel 时间。`encode_write` 包含显式 flush，不包含 fsync。`total_in_pool` 排除 CLI/配置/线程池创建，但包含日志开销；端到端进程基准另外计入这些成本。失败阶段不伪造零耗时或完成记录。真实输入 CPU 1/2/4/8 worker 的进程/RSS 矩阵及缓存限制见 PLAN.Performance.md §61。

### 背景稠密计数（2026-09-23）

`for_each_boundary_value` 只访问边界面，棱和角只计数一次。U8/I8 使用 256 个 usize 计数；至少 65,536 体素的 U16/I16 体数据使用 65,536 个计数，在 64 位主机最多 512 KiB。更小的 16 位以及所有 32 位数据保持 HashMap 路径。checked 索引将声明范围外的值放入稀疏 spill 表，不依赖元数据截断或拒绝任意 i64 值。两种计数共同维护当前众数，平票仍选择最小值，无需最后扫描整张稠密表。计数局部持有，PCA 前释放。独立全网格有序表 oracle 覆盖退化维度、整数极值、有符号范围和元数据不一致。真实输入端到端证据见 PLAN.Performance.md §62。

### 共享阶段计时器（2026-09-25）

crop 现通过 `pipeline::timing::StageTimer` 输出阶段行，名称、顺序、九位小数格式和阶段边界均与之前相同（`restart` 排除同样的不计时间隙），并追加 `[Timing] crop workers=<n>` 与 `[Timing] crop peak_rss_bytes=<n|unavailable>`。split-filter 输出 `load`、`split`、`metrics`、`filter`、`write_stl`、`write_report` 和 `total_in_pool`；文件夹输入现在先全部加载再拆分（每个源网格拆分后仍立即释放）。辅助模块见 `pipeline-core.md`。

### 单遍矩统计与规范 PCA 坐标系（PERF-14）

`estimate_pca_bbox` 现在只做一遍统计加一遍投影边界。`foreground_row_blocks` 遍历相同的固定 65,536 体素分块，把每个连续行段 `(x0, y, z, values)` 交给累加器。对一行而言，前景计数、`Σx` 和 `Σx²` 都是精确整数（`u64`/`u128`），因此 `MomentState::from_row` 能得到该行精确的均值与中心化二阶矩（y、z 为常数，只有 xx 项非零）。各行用 Chan 并行公式并入块内运行状态 `(count, mean, M2)`（`MomentState::merge`：`mean += δ·nb/n`，`M2 += M2b + δδᵀ·na·nb/n`），块状态再按块编号升序合并，因此任意 worker 数结果相同，并避免 `E[x²]-E[x]²` 的消减误差。协方差为 `M2/count`。这是按行粒度应用的 Welford 在线更新（Welford 即 Chan 合并 `nb = 1` 的情形）。`estimate_pca_bbox_three_pass` 保留原质心 + 中心化协方差 + 边界三遍实现作为测试 oracle；`online_moments_match_three_pass_and_workers` 在斜向样本上要求坐标系误差 < 1e-10、质心 < 1e-11、边界 < 1e-8、nearest 裁剪输出完全相同，且 1/2/8 workers 结果逐位一致。

`pca_frame(cov)` 按特征值降序排序，并把相邻差值不超过最大模 `PCA_DEGENERATE_REL_TOL = 1e-3` 倍的特征值归为一组。三重组直接取扫描轴。二重组按 x、y、z 顺序把扫描轴投影到该特征空间并 Gram-Schmidt 正交化，只接受残差范数不小于 0.5 的轴（总能找到两个），因此基不再依赖求解器噪声。非退化列定号为绝对值最大分量为正（恰好相等时取最低轴），行列式为负时翻转第 2 列。测试：立方体和球（含与不含额外一个体素）得到单位坐标系；沿 z、x 的圆柱（含与不含额外体素）分别得到 `[z, x, y]` 与单位阵；倾斜的二重协方差加 1e-9 噪声后坐标系不变。

非退化列的定号约定是必要的：普通斜向样本的三遍与单遍协方差仅末位不同，`SymmetricEigen` 的特征向量符号就发生了翻转。这是行为变化：在仓库真实 CT RAW 数据上，原坐标系第 1、2 列相对新坐标系取反（绕第一主轴旋转 180 度），因此该裁剪输出相对旧版本改变朝向；边界值集合相同，只是第 1、2 轴取反。
