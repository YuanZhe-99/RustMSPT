# Geometry Analysis Reference

本页记录 `src/geometry/metrics.rs`（网格流形校验以及基于体积/表面积推导的形状度量——一个支持目标粒径分布堆积的新模块）和 `src/geometry/s2.rs`（两点相关函数 `S2(r)` 引擎：光线投射的点在网格内测试、体素化、精确 FFT/直接相关计算、蒙特卡洛估计，以及它们的 GPU 加速对应实现）。

## 索引

| 函数 | 位置 | 摘要 |
|---|---|---|
| `MeshMetrics` | `src/geometry/metrics.rs:8` | 保存体积、表面积、等体积直径和球形度的结构体。 |
| `mesh_is_closed` | `src/geometry/metrics.rs:23` | 校验网格是否为流形、方向一致、体积非零的壳体（或多个壳体的集合）。 |
| `mesh_metrics` | `src/geometry/metrics.rs:149` | 为一个封闭网格计算体积、表面积、等体积直径和球形度。 |
| `scale_mesh_to_equivalent_diameter` | `src/geometry/metrics.rs:182` | 原地重新缩放网格，使其等体积直径匹配目标值。 |
| `RAY_DIR_GPU` | `src/geometry/s2.rs:13` | 固定的非轴对齐单位光线方向常量，与 GPU 光线投射内核共享。 |
| `index_3d_to_flat` | `src/geometry/s2.rs:16` | 将三维体素索引转换为一维数组索引（y/z 为主步长）。 |
| `ray_intersects_triangle` | `src/geometry/s2.rs:26` | Möller–Trumbore 光线-三角形相交测试。 |
| `point_inside_mesh` | `src/geometry/s2.rs:63` | 光线投射的点在网格内包含测试（奇数命中规则）。 |
| `build_bbox_occupancy` | `src/geometry/s2.rs:112` | 将网格并行体素化为布尔占据网格。 |
| `shell_offsets_for_distance` | `src/geometry/s2.rs:184` | 枚举落在球壳环带内的整数体素偏移量。 |
| `fill_missing_s2_with_smooth_interpolation` | `src/geometry/s2.rs:217` | 通过线性或三次样条插值填补不受支持的 S2 半径。 |
| `fft_index_3d` | `src/geometry/s2.rs:328` | 将三维 FFT 网格索引转换为一维索引（逻辑与 `index_3d_to_flat` 相同）。 |
| `FftWorkspace::transform` | `src/geometry/s2.rs:369` | 在复数缓冲区上原地执行可分离的三维 FFT/IFFT。 |
| `autocorrelation_counts_fft` | `src/geometry/s2.rs:549` | 通过 FFT 卷积计算占据自相关计数。 |
| `calculate_s2_exact_direct` | `src/geometry/s2.rs:561` | 通过按壳层偏移直接枚举点对来精确计算 S2（不使用 FFT）。 |
| `calculate_s2_exact_fft` | `src/geometry/s2.rs:651` | 使用基于 FFT 的自相关精确计算 S2。 |
| `calculate_s2_monte_carlo_mesh` | `src/geometry/s2.rs:730` | 直接在网格上采样的蒙特卡洛 S2 估计（不做体素化）。 |
| `calculate_s2` | `src/geometry/s2.rs:785` | 顶层 S2 调度函数；路由至精确方法（FFT 或直接法）或体素化蒙特卡洛。 |
| `approximate_s2` | `src/geometry/s2.rs:924` | 使用默认体素间距的蒙特卡洛 S2 估计便捷封装函数。 |
| `l2_norm` | `src/geometry/s2.rs:929` | 两个 S2 向量在其公共长度前缀上的欧氏距离。 |
| `calculate_s2_with_gpu` | `src/geometry/s2.rs:948` | 针对蒙特卡洛/"both" 方法的 GPU 加速 S2，带 CPU 回退。*（特性 `gpu`）* |
| `calculate_s2_gpu_exact` | `src/geometry/s2.rs:984` | GPU 加速的精确 S2（GPU 体素化 + GPU 壳层点对计数）。*（特性 `gpu`）* |
| `PreparedMeshQuery` | `src/geometry/mesh_query.rs:20` | Immutable cached CPU parity query. |
| `PreparedMeshQuery::new` | `src/geometry/mesh_query.rs:29` | Prepare bbox and triangle BVH. |
| `PreparedMeshQuery::bbox` | `src/geometry/mesh_query.rs:68` | Return cached whole-mesh bbox. |
| `PreparedMeshQuery::contains_point` | `src/geometry/mesh_query.rs:73` | Query parity with reusable hit scratch. |
| `MeshQueryScratch` | `src/geometry/mesh_query.rs:7` | Reusable hits and triangle-test counter. |
| `build_nodes` | `src/geometry/mesh_query.rs:144` | Build median BVH with preorder escape links. |
| `ray_reaches_box` | `src/geometry/mesh_query.rs:195` | Conservative positive-ray slab test. |
| `VoxelS2` | `src/geometry/s2.rs:810` | Owned reusable occupancy grid. |
| `VoxelS2::new` | `src/geometry/s2.rs:818` | Prepare CPU occupancy once. |
| `VoxelS2::calculate` | `src/geometry/s2.rs:825` | Compute exact or voxel MC on shared grid. |
| `calculate_s2_mesh_mc_seeded` | `src/geometry/s2.rs:735` | Reproducible sample-block mesh MC. |
| `try_calculate_s2_gpu_exact` | `src/geometry/s2.rs:996` | Fallible GPU exact with checked dimensions. |
| `FftWorkspace` | `src/geometry/s2.rs:334` | Reusable FFT plans and complex arrays. |
| `FftWorkspace::new` | `src/geometry/s2.rs:348` | Construct dimension-specific FFT workspace. |
| `FftWorkspace::array_bytes` | `src/geometry/s2.rs:363` | Report retained complex-array capacities. |
| `with_fft_correlation` | `src/geometry/s2.rs:489` | Evaluate occupancy FFT with bounded cache retention. |
| `FFT_RETAIN_BYTES` | `src/geometry/s2.rs:332` | Maximum retained FFT array bytes per calling thread. |

---

## src/geometry/metrics.rs

本模块为封闭网格计算形状度量——体积、表面积、等体积直径和球形度——并提供其他度量与堆积代码在信任网格体积计算之前所依赖的流形校验关卡（`mesh_is_closed`）。这是一个新增模块；`mesh_metrics` 和 `scale_mesh_to_equivalent_diameter` 共同构成了度量引擎，使堆积流水线能够将颗粒放置到目标等体积直径分布，而不是使用原始的缩放因子。

> **算法：** 参见 [目标粒径分布堆积](../algorithms/packing-target-diameter-distribution.md)，了解消费 `MeshMetrics` 和 `scale_mesh_to_equivalent_diameter` 的端到端算法。

### 公开 API

#### MeshMetrics

- **种类：** `pub struct MeshMetrics`（派生 `Debug, Clone, Copy, PartialEq`）
- **源码位置：** `src/geometry/metrics.rs:7-13`
- **用途：** 汇总 `mesh_metrics` 为一个封闭网格生成的四个派生形状度量。

| 字段 | 类型 | 含义 |
|---|---|---|
| `volume` | `f64` | 网格所包围的体积（四面体带符号体积之和，经校验后保证为正）。 |
| `surface_area` | `f64` | 总表面积（各三角形面积之和）。 |
| `equivalent_diameter` | `f64` | 与该体积相等的球体的直径：`(6V/π)^(1/3)`。 |
| `sphericity` | `f64` | Wadell 球形度：等体积球的表面积与网格实际表面积之比，`π^(1/3)(6V)^(2/3) / A`。值为 `1.0` 表示完美球体；值越小表示相对于体积的表面积越大（形状越不规则）。 |

- **说明：** 当 `mesh_metrics` 返回结果时，四个字段都保证是有限且为正的——无效组合会产生 `None`，而不是携带异常数据的 `MeshMetrics`。

#### mesh_metrics

- **签名：** `pub fn mesh_metrics(mesh: &Mesh) -> Option<MeshMetrics>`
- **源码位置：** `src/geometry/metrics.rs:149`
- **用途：** 在确认网格是有效的封闭流形之后，一次性计算网格的体积、表面积、等体积直径和球形度。
- **参数：**
  - `mesh` — 待测量的候选网格。
- **返回值：** 当网格通过 `mesh_is_closed` 校验，且 `volume`（经 `mesh_volume` 计算）与 `surface_area`（经 `mesh_surface_area` 计算）均为有限且严格为正，且派生出的 `equivalent_diameter`/`sphericity` 也均为有限且严格为正时，返回 `Some(MeshMetrics)`；否则返回 `None`。
- **副作用：** 无。
- **说明：** `equivalent_diameter = (6V/π)^(1/3)`；`sphericity = π^(1/3)·(6V)^(2/3) / A`。实际的体积与面积计算委托给 `mesh_volume`（`src/geometry/volume.rs`）和 `mesh_surface_area`（`src/geometry/mesh_ops.rs`）——本函数只是加上流形关卡以及两个派生量。
- **另请参阅：** `mesh_is_closed`（本函数首先调用的校验关卡）、`scale_mesh_to_equivalent_diameter`（返回的 `MeshMetrics` 的典型下游消费者）。

#### scale_mesh_to_equivalent_diameter

- **签名：** `pub fn scale_mesh_to_equivalent_diameter(mesh: &mut Mesh, metrics: MeshMetrics, target_diameter: f64) -> Option<f64>`
- **源码位置：** `src/geometry/metrics.rs:182`
- **用途：** 原地对网格进行统一缩放，使其等体积直径达到所要求的目标值。
- **参数：**
  - `mesh` — 待缩放的网格，原地修改。
  - `metrics` — 之前为 `mesh` 计算的 `MeshMetrics`（提供作为缩放基准的当前 `equivalent_diameter`）。
  - `target_diameter` — 期望的等体积直径；必须是有限且严格为正的。
- **返回值：** 缩放每个顶点之后返回 `Some(factor)`——已应用的经校验的缩放因子（`target_diameter / metrics.equivalent_diameter`）；若 `target_diameter` 非有限或非正，或计算出的因子非有限或非正（这只有在 `metrics.equivalent_diameter` 本身无效时才会发生），则返回 `None`。
- **副作用：** 原地修改 `mesh.vertices`，将每个顶点围绕原点按 `factor` 缩放（通过 `Vec3::scale`）。
- **说明：** 不会针对 `mesh` 的当前状态重新计算或重新校验 `metrics`——调用方必须提供与网格缩放前几何形状相对应的度量数据。由于缩放是围绕原点进行的（而非网格质心），若调用方关心位置，需要在需要时单独重新定心网格。
- **另请参阅：** `mesh_metrics`（提供 `MeshMetrics` 输入）。

### 私有辅助函数

#### mesh_is_closed

- **签名：** `fn mesh_is_closed(mesh: &Mesh) -> bool`
- **源码位置：** `src/geometry/metrics.rs:23`
- **用途：** 完整的流形校验关卡：判断一个网格是否表示一个或多个方向一致、封闭无缝、体积非零、适合进行体积/面积计算的壳体。
- **参数：**
  - `mesh` — 候选网格。
- **返回值：** 仅当以下条件全部成立时返回 `true`：
  - 网格非空，且每个顶点坐标均为有限值。
  - 每个面引用三个不同且在范围内的顶点索引。
  - 没有两个面共享同一个（无序的）顶点三元组（无重复面）。
  - 每条边恰好被两个面共享，且这两个面以相反方向遍历该边（方向一致——一条边在其两个相邻面上的边方向之和为零）。
  - 每个边连通壳体（通过对面邻接关系沿共享边进行 BFS/DFS 找到）具有有限且非零的带符号体积（带符号体积通过对其各面累加 `Σ a·(b×c)/6` 得到）。
- **副作用：** 无。
- **说明：** 一个网格可能包含多个不相交的壳体（例如带有内部空腔的网格，或多个不连通的网格碎片）；每个壳体被独立校验，且*全部*壳体都必须拥有非零、有限的带符号体积，整个网格才能通过校验。这是 `mesh_metrics` 在信任 `mesh_volume`/`mesh_surface_area` 之前调用的关卡。
- **另请参阅：** `mesh_metrics`（唯一调用方）。

---

## src/geometry/s2.rs

这是两点相关函数（`S2(r)`）引擎——geometry 模块中体量最大、计算密度最高的文件。`S2(r)` 是在堆积域中随机采样、相距距离为 `r` 的两个点均落在固体材料内部的概率；这是一种标准的微结构表征统计量，用于将堆积结构与目标孔隙/颗粒分布进行比较（例如 `data/input/gu2019_fig7b_pore_distribution.csv`）。该文件提供了三种计算策略——精确体素网格相关（通过 FFT 或直接点对枚举）、直接在网格几何体上进行的蒙特卡洛采样，以及体素化的蒙特卡洛采样——外加体素化和壳层计数步骤的 GPU 加速变体，由 `gpu` 特性门控。

> **算法：** 参见 [S2 两点相关函数](../algorithms/s2-two-point-correlation.md)，了解 `calculate_s2`、`point_inside_mesh` 以及下述 FFT 自相关函数背后的完整数学背景与方法选择依据。

### 公开 API

#### RAY_DIR_GPU

- **种类：** `pub const RAY_DIR_GPU: (f64, f64, f64) = (0.9428090415820634, 0.2705980500730985, 0.19611613513818402)`
- **源码位置：** `src/geometry/s2.rs:13`
- **用途：** 用于光线投射点在网格内测试的固定的、非轴对齐的单位长度光线方向。使用一个没有零分量或重复分量的方向，可以规避与轴对齐网格面/边相交时的退化情形（朴素光线投射包含测试中常见的假阴性/假阳性来源）。
- **说明：** 下面的 `point_inside_mesh` 在本地硬编码了相同的数值字面量，而不是引用这个常量——两者在数值上保持一致，但结构上并未关联。此常量被导出以供 GPU 体素化内核复用（参见 `src/gpu/voxel.rs`），CPU 侧和 GPU 侧的光线投射必须在采样方向上保持一致。
- **另请参阅：** `point_inside_mesh`，[GPU 参考](gpu.md)。

#### point_inside_mesh

- **签名：** `pub fn point_inside_mesh(mesh: &Mesh, point: Vec3) -> bool`
- **源码位置：** `src/geometry/s2.rs:63`
- **用途：** 使用奇数命中（Jordan 曲线）规则的光线投射法，判断查询点是否位于封闭网格的体积内部。
- **参数：**
  - `mesh` — 待测试的网格。
  - `point` — 查询点。
- **返回值：** 若该点位于网格内部（沿固定投射方向的唯一光线-三角形相交次数为奇数），返回 `true`；若在外部、网格没有包围盒（空网格）、该点落在网格包围盒之外（带有 `1e-9` 的小容差），或光线未命中任何三角形，则返回 `false`。
- **副作用：** 无。
- **说明：** 使用与 `RAY_DIR_GPU` 相同的固定非轴对齐光线方向，以避免轴对齐退化情形。在进行奇偶计数之前，会对相互间距在 `1e-8` 以内的相交距离（`t` 值）进行去重，从而防止光线擦过两个三角形共享边而被重复计为两次命中。这是一个每点 `O(faces)` 的测试，没有空间加速结构——它是 `build_bbox_occupancy` 和 `calculate_s2_monte_carlo_mesh` 的热点内循环，两者都会针对每个体素或每个样本调用它。
- **另请参阅：** `ray_intersects_triangle`（内部使用）、`build_bbox_occupancy`、`calculate_s2_monte_carlo_mesh`。

#### shell_offsets_for_distance

- **签名：** `pub fn shell_offsets_for_distance(distance_vox: f64, half_width_vox: f64) -> Vec<[isize; 3]>`
- **源码位置：** `src/geometry/s2.rs:184`
- **用途：** 枚举所有欧氏长度落在球壳环带 `[distance_vox - half_width_vox, distance_vox + half_width_vox)`（低端被钳制在零）内的整数体素网格偏移量 `[dx, dy, dz]`，表示"相距半径 `r`" 这一概念的离散体素网格近似。
- **参数：**
  - `distance_vox` — 目标壳层半径，单位为体素。
  - `half_width_vox` — 环带的半宽（壳层厚度），单位为体素——通常为半个体素间距。
- **返回值：** 位于该壳层上的整数偏移三元组组成的 `Vec`。当 `distance_vox <= 1e-12` 时（退化的 `r = 0` 情形），返回 `[[0, 0, 0]]`。
- **副作用：** 无。
- **说明：** 在原点周围迭代一个边长为 `2*lim+1` 的包围立方体（`lim` 由 `distance_vox + half_width_vox` 推导得出），并将平方距离与 `[low2, high2)` 进行比较，因此对于较大的半径，开销随 `r^3` 增长。精确方法（`calculate_s2_exact_direct`、`calculate_s2_exact_fft`）和蒙特卡洛方法（`calculate_s2` 的体素化分支、`calculate_s2_gpu_exact`）的 S2 代码路径共用此函数。
- **另请参阅：** `calculate_s2_exact_direct`、`calculate_s2_exact_fft`、`calculate_s2`、`calculate_s2_gpu_exact`。

#### calculate_s2

- **签名：** `pub fn calculate_s2(mesh: &Mesh, bbox: BoundingBox, r_max: usize, voxel_pitch: f64, method: &str, samples: usize) -> Vec<f64>`
- **源码位置：** `src/geometry/s2.rs:785`
- **用途：** 两点相关函数 S2 计算的顶层调度函数；是代码库其余部分获取 `r = 0..=r_max` 的 `S2(r)` 时使用的主要入口点。
- **参数：**
  - `mesh` — 待表征的堆积/目标网格几何体。
  - `bbox` — 定义体素化/采样区域的域包围盒。
  - `r_max` — 待计算的最大半径（与 `bbox` 使用相同的长度单位），含端点。
  - `voxel_pitch` — 体素边长。当 `method == "exact"` 时必须提供（`> 0`）；当值 `<= 0` 且 `method != "exact"` 时会被忽略（转而路由至网格级蒙特卡洛）。
  - `method` — `"exact"` 表示精确体素网格相关计算；其他任意值（惯例上为 `"monte_carlo"`）表示体素化蒙特卡洛采样。
  - `samples` — 每个半径的蒙特卡洛样本数（仅由非精确方法使用；内部下限为 200）。
- **返回值：** 长度为 `r_max + 1` 的 `Vec<f64>`，按整数半径索引，其中 `S2(0)` 始终等于 `mesh` 在 `bbox` 内的体积分数。
- **副作用：** 通过 `rayon` 执行并行计算（体素化和逐半径的工作）。如果在 `method == "exact"` 且 `voxel_pitch` 为非正值的情况下被调用，会向标准输出打印 `[Warning]`（回退为 `voxel_pitch = 1.0`）。
- **说明——方法路由（本函数的核心逻辑）：**
  1. **`voxel_pitch <= 0.0` 且 `method != "exact"`** → 直接路由至 `calculate_s2_monte_carlo_mesh`，该函数直接针对网格的三角形对点对进行采样，完全不做体素化（最精确，但每样本最慢，因为每个样本都要对完整三角形列表调用两次 `point_inside_mesh`）。
  2. **否则**，先通过 `build_bbox_occupancy`（被其余两个分支共用）对网格进行一次体素化，生成占据网格和体积分数 `vf`。如果网格中占据体素数为零，则立即返回全零向量。
  3. **`method == "exact"`** → 计算填充后的 FFT 网格大小 `(2nx-1)(2ny-1)(2nz-1)`，并与一个固定阈值 **24,000,000 个单元格**（`max_fft_cells`）进行比较。若填充后的网格超过该阈值，则回退至 `calculate_s2_exact_direct`（直接的 O(壳层大小 × 网格大小) 点对枚举，避免大规模 FFT 内存分配）；否则使用 `calculate_s2_exact_fft`（基于 FFT 的自相关，对大网格渐近更快）。
  4. **任何其他 `method` 值**（体素化蒙特卡洛路径）→ 对于每个半径 `1..=r_max`，先通过 `shell_offsets_for_distance` 预先计算一次壳层偏移量，然后为每个半径抽取 `samples.max(200)` 个随机体素对样本（随机体素 + 该半径壳层中的随机偏移），并以在界内点对中的命中比例估计 `S2(r)`。不受支持的半径（空壳层，或零个有效点对）随后通过 `fill_missing_s2_with_smooth_interpolation` 填补。
- **另请参阅：** `calculate_s2_monte_carlo_mesh`、`build_bbox_occupancy`、`calculate_s2_exact_direct`、`calculate_s2_exact_fft`、`fill_missing_s2_with_smooth_interpolation`、`calculate_s2_with_gpu`（本函数的 GPU 加速封装）、[GPU 参考](gpu.md)。

#### approximate_s2

- **签名：** `pub fn approximate_s2(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64>`
- **源码位置：** `src/geometry/s2.rs:924`
- **用途：** 使用固定默认体素间距 `1.0` 的体素化蒙特卡洛 S2 估计便捷封装函数。
- **参数：** 与 `calculate_s2` 参数的对应子集相同（`mesh`、`bbox`、`r_max`、`samples`）。
- **返回值：** S2 值组成的 `Vec<f64>`，形状与 `calculate_s2` 的返回值相同。
- **副作用：** 除 `calculate_s2` 本身所执行的操作（通过 rayon 的并行计算）外没有其他副作用。
- **说明：** 等价于 `calculate_s2(mesh, bbox, r_max, 1.0, "monte_carlo", samples)`。体素间距 `1.0` 不会随网格的尺度自适应——若调用方的网格具有不同的特征长度，应直接调用 `calculate_s2` 并传入合适的间距，而不是使用此封装函数。
- **另请参阅：** `calculate_s2`。

#### l2_norm

`#### l2_norm` — `pub fn l2_norm(a: &[f64], b: &[f64]) -> f64` — `src/geometry/s2.rs:929`。两个 S2 向量在其公共长度前缀上的欧氏距离（`n = min(a.len(), b.len())`）；若任一切片为空则返回 `0.0`。无副作用。用于评分计算/模拟出的 `S2(r)` 曲线与目标曲线（例如来自 `data/input/gu2019_fig7b_pore_distribution.csv`）的吻合程度。

#### calculate_s2_with_gpu

> **特性门控：** 仅在启用 `--features gpu` 时编译。CPU 回退：`calculate_s2`。

- **签名：** `pub fn calculate_s2_with_gpu(mesh: &Mesh, bbox: BoundingBox, r_max: usize, voxel_pitch: f64, method: &str, samples: usize, gpu_pipeline: Option<&mut crate::gpu::s2::GpuS2Pipeline>) -> Vec<f64>`
- **源码位置：** `src/geometry/s2.rs:948`
- **用途：** 蒙特卡洛 S2 路径的 GPU 加速桥接函数；在有可用且适用的 `GpuS2Pipeline` 时调度给它，否则完全交由 CPU 端的 `calculate_s2` 处理。
- **参数：**
  - `mesh`、`bbox`、`r_max`、`voxel_pitch`、`method`、`samples` — 与 `calculate_s2` 相同。
  - `gpu_pipeline` — 指向预先初始化的 `crate::gpu::s2::GpuS2Pipeline` 的可选可变句柄（参见 `src/gpu/s2.rs`）。
- **返回值：** S2 值组成的 `Vec<f64>`，形状与 `calculate_s2` 相同。
- **副作用：** 若 `gpu_pipeline` 为 `Some` 且 `method` 为 `"monte_carlo"` 或 `"both"`，会调度 GPU 计算工作（`gpu.calculate_s2_gpu`）并向标准输出打印一行 `[Info]` 计时信息。否则回退至 `calculate_s2`（CPU 路径），无任何 GPU 副作用。
- **说明：** 无论走的是哪条路径，`r=0` 始终被 CPU 计算的体积分数（`volume_fraction_in_bbox`）覆盖，因此 GPU 内核本身不需要计算精确的 `r=0` 值。当 `method == "exact"` 或 `gpu_pipeline` 为 `None` 时，本函数是对 `calculate_s2` 的纯粹透传——它不会尝试 GPU 精确路径（那是另一个函数 `calculate_s2_gpu_exact`）。
- **另请参阅：** `calculate_s2`（CPU 回退以及本函数所封装的函数）、`calculate_s2_gpu_exact`（GPU 精确方法的对应实现）、[GPU 参考](gpu.md)。

#### calculate_s2_gpu_exact

> **特性门控：** 仅在启用 `--features gpu` 时编译。CPU 回退：`calculate_s2`（若任一 GPU 流水线初始化失败，则以 `method = "exact"` 调用）。

- **签名：** `pub fn calculate_s2_gpu_exact(mesh: &Mesh, bbox: BoundingBox, r_max: usize, voxel_pitch: f64) -> Vec<f64>`
- **源码位置：** `src/geometry/s2.rs:984`
- **用途：** 完全在 GPU 上计算精确 S2：GPU 体素化，随后进行 GPU 壳层偏移点对计数。
- **参数：**
  - `mesh`、`bbox`、`r_max`、`voxel_pitch` — 含义与 `calculate_s2` 相同，但没有 `method`/`samples` 参数，因为本函数始终使用精确方法。
- **返回值：** `r = 0..=r_max` 的 S2 值组成的 `Vec<f64>`。
- **副作用：** 依次初始化两个 GPU 流水线——`crate::gpu::voxel::GpuVoxelPipeline`（f32 光线投射体素化）和 `crate::gpu::s2_shell::GpuShellS2Pipeline`（u32 壳层点对计数）——并在每个流水线上调度计算工作。任一流水线初始化失败时打印 `[Warning]`（回退至 CPU 端的 `calculate_s2`），成功时打印最终的 `[Info]` 计时/汇总信息。
- **说明：** 预先为每个半径 `1..=r_max` 一次性计算好全部壳层偏移量（`all_offsets`，并标注对应半径），并将整批数据一次性交给壳层计数 GPU 内核，而不是像 CPU 精确路径那样实际上每个半径都分别调度一次。GPU 壳层计算完成后，仍会在 CPU 上运行 `fill_missing_s2_with_smooth_interpolation`，以修补任何具有空壳层的半径。体素化阶段（GPU 侧）使用 `f32` 精度，但壳层偏移几何计算和插值（CPU 侧）使用 `f64` 精度——在将 GPU 精确结果与 `calculate_s2_exact_fft`/`calculate_s2_exact_direct` 进行逐位比较时，这是一个值得注意的精度边界。
- **另请参阅：** `calculate_s2`（CPU 精确回退）、`calculate_s2_with_gpu`（蒙特卡洛 GPU 对应实现）、`shell_offsets_for_distance`、`fill_missing_s2_with_smooth_interpolation`、[GPU 参考](gpu.md)。

### 私有辅助函数

#### index_3d_to_flat

`#### index_3d_to_flat` — `fn index_3d_to_flat(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize` — `src/geometry/s2.rs:16`。通过 `x*ny*nz + y*nz + z`（z 变化最快）将三维体素索引转换为一维数组索引。无副作用。贯穿占据网格代码路径（`build_bbox_occupancy`、`calculate_s2_exact_direct`、`calculate_s2` 的体素化蒙特卡洛分支）使用。

#### ray_intersects_triangle

- **签名：** `fn ray_intersects_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64>`
- **源码位置：** `src/geometry/s2.rs:26`
- **用途：** Möller–Trumbore 光线-三角形相交测试。
- **参数：**
  - `origin`、`dir` — 光线原点与方向（`dir` 不必归一化；返回的 `t` 随 `dir` 的模长缩放）。
  - `a`、`b`、`c` — 三角形顶点。
- **返回值：** 对于原点前方的命中（`t > eps`），返回 `Some(t)`——相交点处的光线参数（`origin + t*dir`）；对于未命中、近乎平行的光线（`|det| <= eps`），或原点后方的相交，返回 `None`。
- **副作用：** 无。
- **说明：** 对 `u`/`v`/`det` 使用 `1e-10` 的重心坐标容差 epsilon。只返回前向（`t > eps`）相交——这是一个光线测试而非直线测试，这一点对 `point_inside_mesh` 的奇数命中计数很重要（后向命中不得计入）。
- **另请参阅：** `point_inside_mesh`（唯一调用方）。

#### build_bbox_occupancy

- **签名：** `fn build_bbox_occupancy(mesh: &Mesh, bbox: BoundingBox, voxel_pitch: f64) -> (Vec<bool>, [usize; 3])`
- **源码位置：** `src/geometry/s2.rs:112`
- **用途：** 通过对每个体素中心进行包含测试，将包围盒内的网格体素化为布尔占据网格。
- **参数：**
  - `mesh` — 待体素化的网格。
  - `bbox` — 体素化域。
  - `voxel_pitch` — 体素边长。
- **返回值：** `(occ, [nx, ny, nz])`——一个一维 `Vec<bool>` 占据网格（通过 `index_3d_to_flat`/`y*nz+z` 布局索引）以及网格维度。网格维度按 `ceil(size/voxel_pitch)` 计算，各维度至少钳制为 1。
- **副作用：** 无（纯计算），但内部通过 rayon 的 `par_chunks_mut` 在 x 方向的切片上并行处理。
- **说明：** 首先通过 `split_mesh_into_granules` 将网格拆分为连通分量（若拆分结果为空，则回退为将整个网格作为单一"部件"），并为每个颗粒计算其局部体素索引边界范围，从而使逐体素的包含测试（`point_inside_mesh`）只针对相关颗粒的三角形运行，而不是针对整个网格——这在将大量小颗粒堆积进一个域时是一项重要的优化。若某体素已被同一切片中更早的颗粒标记为占据，则跳过该体素。
- **另请参阅：** `point_inside_mesh`（逐体素测试）、`split_mesh_into_granules`（`src/geometry/mesh_ops.rs`）、`mesh_bbox`（`src/geometry/bbox.rs`）、`calculate_s2`（唯一调用方，通过精确分支和体素化蒙特卡洛分支）。

#### fill_missing_s2_with_smooth_interpolation

- **签名：** `fn fill_missing_s2_with_smooth_interpolation(values: &mut [f64], has_support: &[bool], vf: f64)`
- **源码位置：** `src/geometry/s2.rs:217`
- **用途：** 使用来自相邻受支持半径的平滑插值，填补那些没有有效样本支持的半径处的 S2 值（例如空壳层，或所有点对都超出边界）。
- **参数：**
  - `values` — S2 值数组，原地修改；不受支持的条目会被覆写。
  - `has_support` — 并行的布尔数组；`true` 标记具有直接计算（可信）值的半径。
  - `vf` — 体积分数，即真实的 `S2(0)` 值。
- **返回值：** 无——原地修改 `values`。
- **副作用：** 修改 `values` 切片。无论 `has_support[0]` 为何值，始终设置 `values[0] = vf`。
- **说明：** 若 `values`/`has_support` 长度不匹配、已知的节点少于 2 个，或两个已知半径相邻（没有可插值的内容），则提前返回（除了设置 `values[0]` 外无其他操作）。恰好有两个受支持节点时，使用平滑阶跃混合的线性插值（`t*t*(3-2t)`）；有三个或更多节点时，通过三对角（Thomas 算法风格）求解对二阶导数拟合一条近似自然三次样条，并对接近零的主元使用小的 epsilon 保护（`1e-12`）。所有插值输出值都被钳制在 `[0, 1]` 范围内，因为 S2 是一个概率。
- **另请参阅：** 由 `calculate_s2_exact_direct`、`calculate_s2_exact_fft`、`calculate_s2`（体素化蒙特卡洛分支）以及 `calculate_s2_gpu_exact` 调用。

#### fft_index_3d

`#### fft_index_3d` — `fn fft_index_3d(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize` — `src/geometry/s2.rs:328`。使用与 `index_3d_to_flat` 相同的 `x*ny*nz + y*nz + z` 布局，将三维 FFT 网格索引转换为一维索引。无副作用。

> **文档说明：** 这是 `index_3d_to_flat`（第 13 行）在另一个名字下的逐字节复制，作用域限定在 FFT 网格代码路径（`autocorrelation_counts_fft`、`FftWorkspace::transform`）内。这不是一个 bug，但值得了解这两个函数是可以互换的——未来的清理工作可以将它们统一起来。

#### FftWorkspace::transform

`FftWorkspace::transform(inverse)` applies cached dimension-specific forward/inverse axis plans to its complex grid. The task count is min(current pool workers, ceil(padded cells / 65536)), at least one. One task uses serial axis gathers and one shared scratch buffer, without allocating a transpose array. Multiple tasks use a lazily allocated persistent transpose buffer and task-local `process_with_scratch` storage, with axis-specific minimum chunk lengths. Filling, power spectrum and normalization use the same task budget. Inverse normalization and the power-spectrum pass execute under the caller's Rayon pool. Padding remains exactly 2N-1.

#### with_fft_correlation

`with_fft_correlation(occ, dims, consume)` takes exclusive ownership of a thread-local cached workspace, resets/fills its grid, transforms the occupancy, and passes complex correlation storage and padded dimensions to the consumer. Production shell evaluation reads clamped real counts directly, avoiding another full f64 correlation allocation. A workspace is retained only when its two array capacities total at most 16 MiB and every padded axis is <=4096. Plan internals occupy additional memory; this is a cache-admission limit, not a complete process-memory budget. Dimension changes discard the prior workspace before allocating its replacement. Large workspaces are released at return. No TLS borrow survives parallel work or the consumer callback, allowing nested Rayon evaluations safely; concurrent callers have independent workspaces. The current exact cell limits remain pending the full workload planner.

#### autocorrelation_counts_fft

- **签名：** `fn autocorrelation_counts_fft(occ: &[bool], nx: usize, ny: usize, nz: usize) -> (Vec<f64>, [usize; 3])`
- **源码位置：** `src/geometry/s2.rs:549`
- **用途：** 通过 FFT 卷积定理（正向 FFT → 功率谱 → 逆 FFT）计算完整的占据自相关计数网格，一次性给出每个可能的整数偏移量对应的占据-占据体素点对计数。
- **参数：**
  - `occ` — 一维布尔占据网格。
  - `nx`、`ny`、`nz` — 网格维度。
- **返回值：** `(corr, [fx, fy, fz])`——填充后 FFT 维度上的一维 `f64` 相关计数网格，以及这些填充后的维度本身。
- **副作用：** 无（分配并返回新的缓冲区；不修改 `occ`）。
- **说明：** 在变换之前将每个轴填充到 `2N-1`（`fx = 2*nx-1` 等），以避免会破坏网格边界附近相关计数的循环卷积回绕伪影。经过 FFT → 共轭平方 → IFFT 的往返变换后，取实部并钳制到 `>= 0.0`（自相关计数应为非负；此钳制是为了防止微小的负浮点噪声）。负偏移量通过调用方（`calculate_s2_exact_fft` 的 `get_corr` 闭包）中的回绕索引从填充网格中恢复。
- **另请参阅：** `FftWorkspace::transform`、`calculate_s2_exact_fft`（唯一调用方）。

#### calculate_s2_exact_direct

- **签名：** `fn calculate_s2_exact_direct(occ: &[bool], nx: usize, ny: usize, nz: usize, r_max: usize, voxel_pitch: f64, vf: f64) -> Vec<f64>`
- **源码位置：** `src/geometry/s2.rs:561`
- **用途：** 通过为每个半径的每个壳层偏移直接枚举并计数体素对来计算精确 S2——不使用 FFT，用于填充后的 FFT 网格过大而无法高效分配/处理的情形。
- **参数：**
  - `occ`、`nx`、`ny`、`nz` — 占据网格及其维度。
  - `r_max` — 最大半径（体素网格距离单位，与 `voxel_pitch` 的尺度一致，但循环以 `0..=r_max` 作为物理长度步长的整数计数）。
  - `voxel_pitch` — 体素边长，用于将物理半径 `r` 转换为体素空间的 `r_vox = r / voxel_pitch`。
  - `vf` — 体积分数，直接作为 `S2(0)` 使用。
- **返回值：** 长度为 `r_max + 1` 的 `Vec<f64>`，每个半径对应一个 S2 值，不受支持的半径通过 `fill_missing_s2_with_smooth_interpolation` 填补。
- **副作用：** 无（纯计算）；通过 rayon 的 `into_par_iter` 并行化外层半径循环。
- **说明：** 对每个半径，通过 `shell_offsets_for_distance` 计算壳层偏移量，然后针对每个偏移遍历网格的完整有效重叠区域（`x_start..x_end` 等，考虑偏移量的符号），统计 `valid_pairs` 和 `hit_pairs`（两端均被占据），并在壳层内所有偏移上对命中比例取平均。每个半径的开销为 `O(壳层大小 × 网格大小)`——对于大半径/大网格而言，逐半径开销比 FFT 方法更高，但避免了 FFT 的 `O((2n)^3)` 内存占用，这正是 `calculate_s2` 在填充网格超过 2400 万个单元格时选择该路径的原因。
- **另请参阅：** `shell_offsets_for_distance`、`fill_missing_s2_with_smooth_interpolation`、`calculate_s2`（唯一调用方，"exact" 大网格分支）、`calculate_s2_exact_fft`（针对较小网格的 FFT 替代方案）。

#### calculate_s2_exact_fft

- **签名：** `fn calculate_s2_exact_fft(occ: &[bool], nx: usize, ny: usize, nz: usize, r_max: usize, voxel_pitch: f64, vf: f64) -> Vec<f64>`
- **源码位置：** `src/geometry/s2.rs:651`
- **用途：** 先通过 FFT（`autocorrelation_counts_fft`）一次性构建完整的自相关网格，然后从该预先计算好的网格中为每个半径的每个壳层偏移读取点对计数，以此计算精确 S2。
- **参数：** 与 `calculate_s2_exact_direct` 相同。
- **返回值：** 长度为 `r_max + 1` 的 `Vec<f64>`，每个半径对应一个 S2 值，不受支持的半径通过 `fill_missing_s2_with_smooth_interpolation` 填补。
- **副作用：** 无（纯计算）；通过 rayon 并行化外层半径循环。
- **说明：** 由于相关网格是预先一次性计算好的（摊销后的 `O((2n)^3 log n)` FFT 开销），而不是逐偏移计算，因此对于大半径范围，这比 `calculate_s2_exact_direct` 在渐近意义上便宜得多，代价是填充网格的内存分配——这正是 `calculate_s2` 在选择该路径前通过 2400 万单元格阈值所权衡的取舍。绝对值会超出网格维度的偏移量（`adx >= nx` 等）会被视为该壳层条目不受支持而跳过，因为在该偏移处不存在有效点对。
- **另请参阅：** `autocorrelation_counts_fft`、`shell_offsets_for_distance`、`fill_missing_s2_with_smooth_interpolation`、`calculate_s2`（唯一调用方，"exact" 小网格分支）、`calculate_s2_exact_direct`（针对大网格的直接枚举替代方案）。

#### calculate_s2_monte_carlo_mesh

- **签名：** `fn calculate_s2_monte_carlo_mesh(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64>`
- **源码位置：** `src/geometry/s2.rs:730`
- **用途：** 通过直接针对网格几何体对点对进行蒙特卡洛采样来估计 S2，完全不做体素化步骤。
- **参数：**
  - `mesh`、`bbox` — 网格与采样域。
  - `r_max` — 最大半径。
  - `samples` — 每个半径请求的样本数（通过 `samples.max(200)` 设定下限为 200）。
- **返回值：** 长度为 `r_max + 1` 的 `Vec<f64>`；`S2(0)` 被设为 `volume_fraction_in_bbox(mesh, bbox)`。
- **副作用：** 无（纯计算）；通过 rayon 并行化外层半径循环。
- **说明：** 对每个半径 `r`，在 `bbox` 内均匀抽取 `mc_samples` 个随机点 `p`，为每个点配对一个随机单位方向（从单位立方体中拒绝采样得到，`x²+y²+z² ∈ (1e-12, 1]`），并构造 `q = p + dir*r`。`q` 落在 `bbox` 之外的点对会被完全丢弃（不计入未命中）——因此每个半径的有效样本数可能低于 `mc_samples`，并且随着 `r` 接近域尺寸而进一步减少。对于存活下来的点对，对 `p` 和 `q` 都调用 `point_inside_mesh`，并统计联合占据命中数。这是几何上最忠实的方法（无体素离散化误差），但也最慢，因为每个样本都需要对原始网格进行两次完整的 `O(faces)` 光线投射——当 `calculate_s2` 中 `voxel_pitch <= 0` 且 `method != "exact"` 时使用此方法。
- **另请参阅：** `point_inside_mesh`、`volume_fraction_in_bbox`（`src/geometry/volume.rs`）、`calculate_s2`（唯一调用方，无体素间距分支）。

`calculate_s2_with_gpu` 现捕获 MC 执行错误并记录原因，用连续 CPU mesh MC（pitch 0）重算，避免变为 voxel MC。旧 Vec 接口没有禁止回退开关；optimize 使用自己的 Result 求值接口。

### Prepared CPU S2 queries (PERF-06)

`PreparedMeshQuery::new(&Mesh)` borrows immutable geometry and caches its bbox and a median triangle BVH (more than 32 finite triangles; leaves at most eight). `contains_point(Vec3, &mut MeshQueryScratch)` keeps the original ray, triangle predicate and anchored 1e-8 hit deduplication. Scratch retains hit capacity and exposes the most recent triangle-test count. Small meshes use cached direct queries. Geometry changes require a new prepared query; the original `point_inside_mesh` remains the full-scan reference.

`calculate_s2_mesh_mc_seeded(mesh, bbox, r_max, samples, seed, prepared)` returns continuous MC S2. Radius/sample blocks of 2048 use independent ChaCha12 streams derived from radius and block, with integer reductions. Results are reproducible across worker counts; `prepared=false` uses full-scan containment for differential benchmarks. Ordinary mesh MC chooses a fresh base seed and uses prepared queries. This changes the old unseeded RNG draw protocol, not the point/direction distribution.

`VoxelS2::new(mesh, bbox, pitch)` owns one voxelization at a positive pitch (minimum 1e-9). `calculate(r_max, method, samples)` reuses it for exact or voxel MC, reporting occupancy VF. Measure lazily retains this object across CPU methods/fallbacks; continuous MC remains independent. Voxelization prepares per-component queries and reuses per-worker hit scratch. Components below the domain have their upper index clamped before unsigned conversion.

GPU exact 现将 shell 构造在 voxel 的同一 Device/Queue 上，直接绑定其 occupancy 缓冲。shell 不再选择适配器或申请第二个设备，也不再为占据场分配和上传副本。两阶段顺序运行，各自配对错误作用域。Voxel 现于设备端计数，仅为 VF 回读 4 字节；上层 backend 能力探测仍独立计数。独立主机占据场 API 保留上传行为，之后的主机求值不会覆盖借用的 voxel 缓冲。

voxelize_count 在 voxelization 后执行延迟编译的整数占据归约，完整占据场留在设备，仅回读一个 u32。归约器使用一个 256-lane 工作组跨步扫描；二值占据与已检查的网格大小保证各级计数不超过 u32。总工作量仍为 O(网格体素数)，减少传输不等于必然降低时延。occupancy 和 staging 分别按需增长：count-only 只需 4 字节 staging，之后完整回读再按需增长；模式切换和释放后重算已有对照。GPU exact 用该计数计算 VF 并将驻留占据场传给 shell，独立 voxelize 保留 Vec 返回契约。

GPU exact 现逐半径延迟生成一个 shell Vec，经 compute_s2_shell_resident_stream 按批消费。批次仍受已有部分结果槽上限约束，可跨半径但保持原偏移顺序。插值支持标志在生成该半径时记录，取消原来的第二遍枚举；成功求值会消费全部偏移，包括末尾无支持偏移。内存范围为一个半径 shell 加一个批次，并非与半径无关的常量；单个大半径 shell 仍会物化。累计生成数量采用 u128，日志不静默饱和截断。

GPU exact 现使用 shell_offset_iter，单个半径内部也只保留嵌套范围游标；保持原 x/y/z 顺序、原点特殊情况和半开平方距离判定。需要随机访问的公共 Vec API 保持不变。用 peekable 判定该半径是否有支持，生成数量在消费时累计。普通范围沿用原整数范数，更大范数用 u128 避免有符号乘法溢出。仍扫描包围立方体，降低分配并未改变 O(半径³) 搜索复杂度。

| Function | Source | Contract |
|---|---|---|
| `shell_offset_iter` | `src/geometry/s2.rs:213` | Lazy ordered shell enumeration with constant cursor storage; GPU exact streaming and tests. |
