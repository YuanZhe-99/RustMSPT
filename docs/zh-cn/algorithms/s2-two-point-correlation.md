# S2 两点相关函数

S2(r) 是一种标准的微结构表征统计量：在堆积体内随机投放两个相距 `r` 的点，
它们同时落在同一相（此处指均落在固体颗粒材料内部）的概率。当 `r = 0` 时，
该值必然等于体积分数（VF），因为一个点显然与自身一致；随着 `r` 增大，对于
完全随机（无相关）的介质，该值会衰减趋向于 VF²，而两者之间的衰减形状则编码了
颗粒的尺寸、形状与空间排布信息。RustMSPT 将 S2 用于两个不同的目的：

- **`measure`**（`src/pipeline/measure.rs`）将 S2 作为诊断量计算给定堆积几何体——
  与 VF 一并报告，使用户能够表征已完成的堆积体，或通过两者的 L2 距离比较不同方法
  （精确法 vs. 蒙特卡洛法）之间的差异。
- **`optimize`**（`src/pipeline/optimize.rs`）将 S2 视为优化的*目标*：给定一条目标
  S2 曲线（例如从参考微结构测得），其模拟退火岛屿（`run_sa_island`）通过扰动颗粒位置，
  最小化当前堆积体的 S2 曲线与目标曲线之间的 L2 距离，从而有效地重建出一个能够复现
  目标两点统计量的微结构。

以上所述的全部逻辑位于 `src/geometry/s2.rs` 中，其 GPU 加速对应实现位于
`src/gpu/s2.rs` 与 `src/gpu/s2_shell.rs`。

## 点是否在网格内：光线投射

每一种 CPU 侧的 S2 方法最终都需要针对任意查询点回答"该点是否位于固体内部？"这一问题。
`point_inside_mesh` 通过光线投射奇偶性测试来回答该问题：在经过一次廉价的包围盒排除后，
它从查询点沿一个**固定的、非轴对齐的方向**发射一条光线，穿过网格的每一个三角形，
并使用 Möller–Trumbore 光线/三角形求交算法（`ray_intersects_triangle`）求出交点距离
`t`。当且仅当该光线与表面相交的次数为奇数时，该点被判定为位于网格内部。

以下两个细节使该奇偶计数在实践中足够稳健：

- 光线方向是一个固定的单位向量 `(0.9428090415820634, 0.2705980500730985,
  0.19611613513818402)`，之所以选取该方向，是为了避免轴对齐带来的边界情形——即光线
  可能沿三角形边缘掠过，或恰好落在某个三角形所在的平面内。同一常量以 `RAY_DIR_GPU`
  的形式导出，并被 GPU 蒙特卡洛内核原样复用（在 `src/gpu/s2.rs` 的 `pack_params` 中
  被打包进着色器参数），从而保证 CPU 与 GPU 的点在网格内测试结果一致。
- 原始交点距离在计数奇偶性之前会先排序，然后在 **`1e-8`** 范围内**去重**。若不这样做，
  一条恰好穿过两个相邻三角形共享顶点或边的光线将被计为两次相交而非一次，从而破坏
  奇偶性判定，导致网格接缝附近的点被错误分类。

## 体素化

精确方法与体素化蒙特卡洛方法都需要一个离散化的占据网格，而非反复查询网格。
`build_bbox_occupancy` 以给定的 `voxel_pitch`（体素间距）将网格采样到一个布尔网格上：
它根据包围盒尺寸与间距计算出网格维度 `nx, ny, nz`，然后用 `point_inside_mesh` 测试
每个体素的中心点。以下两项优化使该过程在包含大量颗粒的堆积体上依然可行：

- 网格首先被拆分为按颗粒划分的连通分量（`split_mesh_into_granules`），并将每个
  连通分量自身的包围盒与体素网格求交，使得仅需针对某个颗粒可能占据的体素范围，
  测试该颗粒自身的三角形，而不必对整个网格测试每一个体素。
- x 维度通过 rayon 并行化（对 x 方向切片使用 `par_chunks_mut`），因为每个 x 切片
  写入的是互不相交的输出区域。

## 三种计算方法

`calculate_s2` 会根据请求的 `method` 字符串，以及在 `"exact"`（精确）情形下所得到的
网格规模，调度到三种底层策略之一。三者在精度与开销之间的取舍各不相同，这也是代码库
保留全部三种方法而非只选其一的原因：

### 1. 直接在网格上进行蒙特卡洛

`calculate_s2_monte_carlo_mesh` 完全不进行体素化。对每个半径 `r`，它抽取随机的
点/方向对——包围盒内均匀随机的起点 `p` 与均匀随机的单位方向 `dir`——构造
`q = p + r * dir`，丢弃 `q` 落在包围盒之外的配对，否则用 `point_inside_mesh` 分别测试
`p` 与 `q`。两点均命中固体相的有效配对所占比例即为 S2(r) 的估计值。由于无需构建体素
网格，当只需计算少数几个较大半径时，或者作为一种无模型的基准检验（用以核实体素化
计算是否引入了离散化偏差），该方法是最廉价的选择。

### 2. 体素化蒙特卡洛

这是 `calculate_s2` 中 `match method` 的兜底分支，适用于一旦确定了 voxel_pitch 后的
任何非 `"exact"` 取值：它先通过 `build_bbox_occupancy` 一次性构建占据网格，然后对每个
半径抽取随机体素对——一个随机体素加上从该半径的**壳层偏移集合**（见下文）中抽取的
随机偏移——并直接在布尔数组中查询占据情况，而无需重新测试网格。由于占据查询是 O(1)
的数组读取，而非光线/三角形求交，该方法能将一次性体素化的成本分摊到多个半径和每个
半径的多次采样上，因此成为迭代使用场景下的默认/低成本选择（例如在 `optimize` 流水线
中每次迭代都要对同一堆积体重新评估 S2 的场合，每次接受的扰动都会触发一次重新评估）。

### 3. 精确法（FFT 或直接的壳层配对枚举）

`"exact"` 方法穷举式地计算 S2(r)——针对半径 `r` 的壳层距离上的每一个有效体素对，
而非随机抽样——可通过两种等价算法之一实现：

- **基于 FFT**（`calculate_s2_exact_fft` / `autocorrelation_counts_fft`）。占据网格的
  自相关运算能够针对每一个整数偏移 `(dx, dy, dz)`，给出两点均被占据的体素对
  `(v, v+offset)` 的精确计数——这正是壳层配对求和所需要的量，通过
  `FFT → 功率谱 (|F|²) → 逆 FFT` 一次性计算得到，而无需对偏移和网格位置进行三重嵌套
  循环。在变换之前，网格会沿每个轴**零填充至 `2N-1`**：

  ```
  fx = 2*nx - 1, fy = 2*ny - 1, fz = 2*nz - 1
  ```

  之所以需要这种填充，是因为基于 FFT 的自相关本质上是*循环*的：若不填充，靠近网格
  一侧边缘的偏移会发生环绕，从对侧边缘拾取虚假的"配对"。将每个轴填充至 `2N-1`
  可以保证在 `[-(N-1), N-1]` 范围内的任何偏移都不会被环绕效应污染。
- **直接的壳层配对枚举**（`calculate_s2_exact_direct`）。对每个半径，遍历该半径壳层
  内的每一个偏移（见下文），并针对每个偏移扫描每一个有效的 `(x, y, z)` 网格位置，
  直接检查 `(x,y,z)` 和 `(x,y,z)+offset` 两处的占据情况——不使用 FFT，但每个半径的
  开销为 O(壳层大小 × 网格大小)，并通过 rayon 在各个半径之间并行化。

`calculate_s2` 通过**内存预算下的成本模型**（`plan_exact_cpu`，PERF-07）在两者之间选择。
FFT 时间建模为 `c_fft · P · log2 P`（`P` 为填充后单元数）；直接法时间为 `c_pair · W`，其中
`W` 是直接内核实际访问的体素配对数（域内壳偏移重叠体积之和，至多 `K · N`）。只有峰值工作集
（checked 算术：FFT 为占据场、复数网格、一个 x 轴收集带、每 worker scratch；直接法为占据场与每半径偏移
列表）不超过默认 768 MiB 预算的内核才可选，取成本较低者。该预算等于旧的固定 24,000,000
填充单元上限的网格加转置大小。FFT 相关值取整为整数配对计数，且两个内核按相同顺序累加壳项，
因此返回**逐位相同**的曲线：选择只影响时间和内存。所选方法、两种模型时间、两种工作集和原因
每个计划打印一次。

实际中 `r_max` 相对网格较大时 FFT 更优（其成本不随 `r_max` 增长）；大网格上 `r_max` 较小时
直接法更优，此时 `W` 接近 `K · N` 且 `K` 很小。填充使用每轴不小于 `2N-1` 的最小 2,3,5-平滑
长度而非恰好 `2N-1`：任何 `>= 2N-1` 的长度对 `|d| <= N-1` 的位移都不会回绕，平滑长度避免了
素数长度的慢变换（例如 127 -> 128，199 -> 200）。

## 壳层偏移：`shell_offsets_for_distance`

两种精确方法与体素化蒙特卡洛方法都需要同一个基础操作：给定一个目标半径（以体素为
单位）和一个固定为 `0.5 / voxel_pitch`（即半个体素）的半宽，枚举出欧几里得长度落在
该环带范围内的每一个整数体素偏移 `[dx, dy, dz]`（`shell_offsets_for_distance`）。
这将"所有物理距离相差 r 的体素对"转化为一个具体的、有限的偏移列表，供直接枚举法和
蒙特卡洛采样法迭代/抽样，也供精确 FFT 路径直接在填充后的相关网格中查找。特殊情形
`distance_vox <= 1e-12` 只返回 `[[0,0,0]]`（即 r = 0，点与自身配对）。同样的偏移
枚举逻辑也被 GPU 精确路径复用（`calculate_s2_gpu_exact` 通过
`shell_offsets_for_distance` 构建偏移列表并传递给 `GpuShellS2Pipeline`），因此
CPU 与 GPU 的精确结果是基于完全相同的壳层计算出来的。

## 填补不支持的半径：平滑插值

并非每个半径在给定壳层上都存在可用的体素对——较小的网格，或接近包围盒对角线的半径，
可能最终得到零个有效偏移。`fill_missing_s2_with_smooth_interpolation` 用于修补这些
空缺：当恰好有两个已知邻近点时，使用平滑阶跃（smoothstep）加权的线性插值；当已知点
达到三个或更多时，则拟合一条自然三次样条曲线（通过求解一个三对角线性方程组来得到
二阶导数）穿过已支持的半径，并在缺失的半径处求值。所有填补值都被截断到 `[0, 1]`
范围内，因为 S2 是一个概率；`values[0]` 始终被强制设为体积分数，不受插值结果影响。
该函数在每一种基于体素网格的方法（两种精确变体以及体素化蒙特卡洛）末尾都会作为
共享的后处理步骤被调用。

## r = 0 的特殊情形

每种方法都将 `r = 0` 作为特殊情形处理，而不是运行通用的采样/枚举逻辑：
`calculate_s2_monte_carlo_mesh`、`calculate_s2_exact_direct`、`calculate_s2_exact_fft`
以及 `calculate_s2` 的体素化蒙特卡洛分支，在 `r == 0` 时都会立即返回预先计算好的
体积分数 `vf`；并且作为最后一道保障，插值完成后每个结果数组都会再次被强制设置为
`out[0] = vf`。这既是一种定义层面的捷径（S2(0) 依定义即为 VF，无需采样），也是一种
数值稳定性上的考量（`r = 0` 处退化的壳层偏移情形是一个单一的自配对，若不特殊处理，
下游的每一步计算都需要单独应对该情形）。GPU 路径遵循同样的约定：
`calculate_s2_with_gpu` 在蒙特卡洛内核返回后，用 `volume_fraction_in_bbox(mesh, bbox)`
覆盖 `result[0]`；`calculate_s2_gpu_exact` 在返回之前也执行同样的操作。

## GPU 加速

GPU 路径实现了与 CPU 代码相同的三种方法分类，将内部的光线投射或壳层配对循环替换为
wgpu 计算内核，同时保持外围的按半径/壳层组织的结构不变：

- **`calculate_s2_with_gpu`** 在 `gpu_pipeline` 为 `Some` 且方法为 `"monte_carlo"`
  或 `"both"` 时，将蒙特卡洛采样调度给 `GpuS2Pipeline::calculate_s2_gpu`
  （位于 `src/gpu/s2.rs`）。该内核一次性上传归一化后的 f32 三角形缓冲区，并在单次
  调度中，使用与 CPU 路径相同的 `RAY_DIR_GPU` 光线方向，对每个半径运行
  `samples_per_radius` 次独立的点在网格内配对试验，累积命中/有效计数，随后回读并在
  CPU 侧归约为一条 S2 曲线。这正是"直接在网格上进行蒙特卡洛"（上文方法 1）的
  GPU 对应实现——它作用于原始三角形数据，而非体素网格。
- **`calculate_s2_gpu_exact`** 实现了精确方法的 GPU 对应版本：它首先在 GPU 上对网格
  进行体素化（`GpuVoxelPipeline::voxelize`，其光线投射方式与 `build_bbox_occupancy`
  类似，但在设备端执行），从占据体素数量计算体积分数，通过
  `shell_offsets_for_distance` 为每个半径构建完整的壳层偏移列表，并将占据网格与
  偏移列表交给 `GpuShellS2Pipeline::compute_s2_shell`（位于 `src/gpu/s2_shell.rs`）
  处理，该组件在 GPU 上直接统计每个壳层偏移的占据配对数。计算结果之后仍会在 CPU 侧
  经过 `fill_missing_s2_with_smooth_interpolation` 处理。

两个入口点都会**在 GPU 初始化失败时回退到 CPU 路径**：`calculate_s2_gpu_exact` 会
捕获来自 `GpuVoxelPipeline::new` 或 `GpuShellS2Pipeline::new` 的错误，转而调用
`calculate_s2(..., "exact", ...)`，并打印一行 `[Warning]`；`calculate_s2_with_gpu`
则在 `gpu_pipeline` 为 `None` 时直接回退到 `calculate_s2`。这使得 GPU 特性纯粹是
增量式的——每个调用方（`measure`、`optimize`）都可以请求 GPU 加速，而无需自行实现
回退代码路径。

## 比较 S2 曲线：`l2_norm`

`l2_norm(a, b)` 是两条 S2 向量之间的欧几里得距离——在两者公共长度前缀上计算
`sqrt(sum((a[i] - b[i])^2))`。`measure` 通过其自身的 `l2_error` 封装函数使用该
函数，以报告同一几何体上精确法与蒙特卡洛法彼此的吻合程度；`optimize` 则将其用作
模拟退火的损失函数，在 `run_sa_island` 及其配套的 `selective_prune_to_target_vf`
步骤中计算当前候选堆积体的 S2 曲线与目标 S2 曲线之间的 L2 距离，并依据该损失通过
Metropolis 准则接受或拒绝扰动。

## 交叉引用

- [geometry-analysis.md](../reference/geometry-analysis.md) —— 关于
  `point_inside_mesh`、`shell_offsets_for_distance`、`calculate_s2`、`approximate_s2`、
  `l2_norm`、`calculate_s2_with_gpu`、`calculate_s2_gpu_exact`、
  `fill_missing_s2_with_smooth_interpolation`、`autocorrelation_counts_fft`、
  `calculate_s2_exact_direct` 以及 `calculate_s2_exact_fft` 的完整函数级参考。
- [gpu.md](../reference/gpu.md) —— `GpuS2Pipeline`（蒙特卡洛 S2）与
  `GpuShellS2Pipeline`（精确壳层配对 S2）的管线构建、缓冲区布局与调度细节。
- [pipeline-core.md#MeasurePipeline::run](../reference/pipeline-core.md#measurepipelinerun) ——
  `measure` 流水线中计算并报告 S2 诊断量的位置。
- [pipeline-optimize.md#run_sa_island](../reference/pipeline-optimize.md#run_sa_island) ——
  S2 的 L2 损失驱动模拟退火接受准则的位置。

## Optimize execution (PERF-01/02)

optimizer 通过 `OptimizeS2` 一次解析 S2 定义。正 pitch 的 MC 始终是体素 MC，exact 始终是体素 exact，
只有非正 pitch 的非 exact 请求才可使用连续 GPU MC。目标、剪枝、初始/候选/迁移状态和最终复核共享同一方法。
连续方法使用几何 VF，体素方法使用占据率 VF。选中 GPU 时整次运行共享一个 MC 实例。
此处未改变 measure 或上述共享 GPU 封装，其独立的正确性和资源工作仍待处理。

### GPU 参数布局与 shell 边界（2026-09-13）

Voxel 参数共 48 字节，射线方向从字节 32 开始。MC 和 voxel 三角形上传均先在 f64
中减去 bbox 原点，再转为 f32，保留大平移下的局部几何。Shell 计数在无符号运算前
排除域外偏移，每批最多处理 200,000 项；跨批次按有效偏移的 hits/valid 等权平均。
空列表只保留传入的 VF。这些修复尚未解决 64-hit 射线限制、MC 半径限制、通用工作集
规划和运行时错误回退，见 `PLAN.Performance.md` §12。

### 射线溢出恢复（2026-09-13）

MC 和 voxel shader 在正向三角形原始命中超过 64 次时，不再静默截断，而是逐轮完整扫描，
选择下一个不同交点，按原 GPU 的锚定 `1e-6` 容差去重并统计奇偶性（已被下文 f32 认证取代：两条路径现均使用 CPU 的 `1e-8` 带并证明每个间隔）。普通射线仍走原排序数组
路径，MC 样本不丢弃、不重抽。恢复仅需常数额外空间，但成本可达 O(三角形数×不同命中数)，
复杂场景可能较慢。CPU/GPU 数值一致性和设备/读回错误仍待解决，验证见 `PLAN.Performance.md` §13。

MC 执行现返回容量/读回/设备错误；旧 GPU 包装函数回退到连续 CPU mesh MC，optimize 则遵守显式回退策略。Voxel/shell 的运行时传播仍待办，见 GPU 函数文档及 `PLAN.Performance.md` §14。

Measure 现区分正 pitch voxel MC 与连续 mesh MC，CPU 回退在配置池内执行并记录各方法实际后端。可失败的 GPU exact 入口将非正 pitch 规范为 1.0，传播 voxel/shell 执行错误，不静默改用近似方法。

### CPU preparation and sample blocks

CPU mesh MC and voxelization now cache geometry queries through `PreparedMeshQuery`, preserving the full-scan parity predicate and hit tolerance. MC distributes fixed 2048-sample blocks across workers; a supplied base seed gives identical integer hit/valid reductions across worker counts. The ordinary entry draws a new base seed per evaluation. Measure reuses one lazy CPU `VoxelS2` occupancy for exact and positive-pitch MC, including GPU fallback, avoiding repeated splitting and containment. GPU occupancy sharing remains separate pending work.

### Reusable exact FFT storage

CPU exact FFT now reuses forward/inverse axis plans, complex grid and transpose storage for repeated same-dimension evaluations on a calling thread. It fills and normalizes under the current pool, reads shell counts directly from the complex correlation grid, and releases oversized workspaces after use. Retention is capped at 16 MiB of arrays per thread and padded axes <=4096; opaque FFT plan memory is additional. Nested evaluations take separate owned workspaces without holding a thread-local borrow. 上文所述的内存/成本规划与平滑填充随后在 PERF-07 中加入。

### GPU MC 工作组整数归约

生产 MC shader 每个 256-lane 工作组对应一个（半径，样本块）。有效 lane 仍用 radius×samples_per_radius+sample 作为 RNG 逻辑编号；尾部填充 lane 贡献零。工作组以整数归约输出一对 hit/valid 部分和，CPU 用 u64 合并并沿用原比值；回读字节变为 8×(r_max+1)×ceil(max(samples,200)/256)。半径填充不会混合计数或重抽样本。dispatch_plan 分别检查逻辑编号溢出、补齐后的组数和部分和缓冲容量；mc_evaluation_peak 同步采用部分和容量，仍计入保留/上传/增长峰值。

私有 calculate_s2_gpu_counts 固定 seed 返回整数总数；公开 API 仍抽取一个新随机 seed 并返回曲线。tests/fixtures/s2_monte_carlo_samples.wgsl 冻结归约前 shader，仅供相同 seed 的逐项计数对照，覆盖尾块、128 半径、几何更新。BVH、GPU 半径最终归约和按预算拆样本仍待独立实施；这里未证明 CPU/GPU f64 等价或真实硬件加速。

MC 工作组现沿两个 dispatch 维度展开，以 group.x + group.y × num_workgroups.x 得到块编号。统一分支在 barrier 和写出前排除填充组，即使保留缓冲容量大于本次有效结果也不写入尾部。planner 返回样本数、部分和数量及 [x,y] 调度形状；逻辑样本编号仍受 u32 限制。Optimize 启动检查已移除旧单维调用上限。强制 3×3/4×2 的实测整数计数与冻结逐样本 shader 一致，额外容量哨兵验证尾部未被写入。65,536 组的大任务仅验证规划结果，不代表大任务 GPU 实测。

GPU shell 在排除超出任意轴的位移后，以 (nx−|dx|)×(ny−|dy|)×(nz−|dz|) 直接计算合法配对数。主机已检查完整网格乘积不超过 u32，因此重叠子体积乘积不会溢出。命中数仍遍历占据对，CPU 仍对完整偏移比值等权平均；体素分块和设备常驻 occupancy 仍待完成。独立原始计数 oracle 用有符号坐标穷举小网格全部偏移，覆盖薄网格、空/满/混合占据及 i32 极端位移。此算术修改本身不构成已测得的提速结论。

实验 shader s2_shell_cooperative.wgsl 每个偏移分配一个 256-lane 工作组，lane 跨步遍历重叠体积并在共享内存归约整数 hit，valid 保持解析计算。二维工作组展平后在 barrier 前统一拒绝填充组。每个偏移仍输出一对计数，因此尚不是独立调度的多个体素 tile，也未实现 voxel→shell 设备常驻数据流。生产仍选择 direct shader，等待工作量基准决定。new_with_shader(source, offsets_per_workgroup) 将 shader 索引与调度宽度配对：direct 为 256，cooperative 为 1。

实验 tiled shader 按（offset，voxel tile）分派工作组，支持正整数块大小。块内整数归约 hit 并输出该块解析 valid 数；空块输出零。主机用 u64 合并同一 offset 全部块的计数，再形成完整偏移比值。每批至多 200000 个部分结果槽，因此块数增加时减少每批 offset 数；单偏移超过 200000 块时明确报容量错误。此固定上限尚不是完整用户预算规划。参数缓冲为 24 字节（offset 数、三轴维度、每偏移块数、块大小），旧 direct shader 读取前 16 字节。生产继续使用 direct，tiled 路径仍在验证和测量。

实验 tiled 路径可启用第二次设备端计算 s2_shell_reduce.wgsl，将每个 offset 的 tile hit/valid 合并为一对整数。每个偏移的总计数不超过已检查的完整网格大小，因此 u32 合并不溢出；CPU 仍按原定义平均完整偏移比值。归约 pipeline 和最终缓冲延迟构造并复用，显式 batch release 缩小最终缓冲但保留已编译归约器。回读恢复为每偏移 8 字节，与 tile 数无关；中间 tile 缓冲及额外 dispatch 仍存在。此选项仍为实验路径，不代表生产选择或已证明提速。

生产 shell 求值在上传前过滤位移绝对值达到任意轴维度的 offset，以 unsigned_abs 安全处理 isize::MIN。过滤保持输入顺序，复用有界主机批次，不额外保存完整 offset 列表。全部 offset 无支持时直接返回原 VF/零曲线，不上传 occupancy 或分配结果缓冲。测试参考构造器可关闭过滤，继续验证 shader 对无效位移的保护。完整偏移比值及等权平均不变；此过滤尚未移除有效偏移内部的空 tile。

GPU exact 现将 shell 构造在 voxel 的同一 Device/Queue 上，直接绑定其 occupancy 缓冲。shell 不再选择适配器或申请第二个设备，也不再为占据场分配和上传副本。两阶段顺序运行，各自配对错误作用域。Voxel 现于设备端计数，仅为 VF 回读 4 字节；上层 backend 能力探测仍独立计数。独立主机占据场 API 保留上传行为，之后的主机求值不会覆盖借用的 voxel 缓冲。

voxelize_count 在 voxelization 后执行延迟编译的整数占据归约，完整占据场留在设备，仅回读一个 u32。归约器使用一个 256-lane 工作组跨步扫描；二值占据与已检查的网格大小保证各级计数不超过 u32。总工作量仍为 O(网格体素数)，减少传输不等于必然降低时延。occupancy 和 staging 分别按需增长：count-only 只需 4 字节 staging，之后完整回读再按需增长；模式切换和释放后重算已有对照。GPU exact 用该计数计算 VF 并将驻留占据场传给 shell，独立 voxelize 保留 Vec 返回契约。

GPU exact 现逐半径延迟生成一个 shell Vec，经 compute_s2_shell_resident_stream 按批消费。批次仍受已有部分结果槽上限约束，可跨半径但保持原偏移顺序。插值支持标志在生成该半径时记录，取消原来的第二遍枚举；成功求值会消费全部偏移，包括末尾无支持偏移。内存范围为一个半径 shell 加一个批次，并非与半径无关的常量；单个大半径 shell 仍会物化。累计生成数量采用 u128，日志不静默饱和截断。

GPU exact 现使用 shell_offset_iter，单个半径内部也只保留嵌套范围游标；保持原 x/y/z 顺序、原点特殊情况和半开平方距离判定。需要随机访问的公共 Vec API 保持不变。用 peekable 判定该半径是否有支持，生成数量在消费时累计。普通范围沿用原整数范数，更大范数用 u128 避免有符号乘法溢出。仍扫描包围立方体，降低分配并未改变 O(半径³) 搜索复杂度。

新建的驻留 GPU exact 求值在后端选择和执行中共用 ExactMemoryPlan。设 T=max(36×faces,4)、M=4×cells、B 为批次部分结果槽数，保守逻辑峰值为 2T+M+128+80B，计入待完成三角/offset 上传和批次增长时同时存在的新旧缓冲；128 字节覆盖固定参数/计数/占位资源。B 从 200000 按 MiB 预算缩小，至少 1；最小批次仍超限则在设备初始化前拒绝，由调用方执行配置的回退策略。该模型不含驱动/编译器内部资源和 CPU 内存，仅适用于新建生产 direct-shell exact 求值，不声称覆盖实验 tiled/reduced 或任意已有高水位管线。旧 exact 网格硬限制仍独立存在。

### GPU 射线奇偶性的 f32 认证（PERF-05）

MC 着色器（`s2_monte_carlo.wgsl`）与体素化着色器（`voxelize.wgsl`）不再直接采用 f32 奇偶判定。每个查询被分为**确定在外**、**确定在内**或**不确定**三类；所有不确定查询由主机用未改动的 CPU f64 判定（`point_inside_mesh` / `PreparedMeshQuery::contains_point`）在**同一点**重新计算。因此认证后的 GPU 结果与 CPU 参考逐查询完全一致。

**逐三角形常量（§77）。** 每次派发的射线方向固定，因此测试中不涉及查询点的部分——边 `e1`、`e2`，幅值界 `m` 与 `es`，`h = d x e2`，`det = e1 . h` 与 `eps_det`——在主机上按三角形计算一次（`certify::triangle_constants`），着色器从每三角形 16 个浮点的缓冲读取。误差界是针对任意求值顺序下正确舍入的 f32 运算推导的，对主机计算与着色器计算同样成立；`precomputed_terms_respect_their_error_bounds` 在所有对抗性 fixture 上检查 f32 行列式与其 f64 值的差不超过该界。已认证的答案仍与 CPU 完全一致，不确定的查询仍在 CPU 上重算。

**参考定义。** 参考是 CPU 判定作用于以 GPU 原点（`bbox.min`）在 f64 中平移后的网格（`CertReference`），查询点是 GPU 实际使用的 f32 点提升为 f64。MC 点即着色器自身的 `p`/`q`（每个不确定样本都回传其位模式）；体素中心为 `(f32(i) + 0.5) * pitch`，两次均为正确舍入运算，主机 `voxel_center` 逐位复现。着色器与 CPU 阈值完全一致：`|det| > 1e-10`、`u ∈ [-1e-10, 1+1e-10]`、`v >= -1e-10`、`u+v <= 1+1e-10`、`t > 1e-10`、锚定 `1e-8` 命中去重，以及 `网格 bbox ± 1e-9` 提前排除。原 GPU 专用的 `1e-6` 容差已删除。

**精确提前排除。** 主机把 f64 阈值 `bb.min - 1e-9` 向上、`bb.max + 1e-9` 向下舍入到 f32（`f32_at_least`/`f32_at_most`）。对任意 f32 点 `p`，`p < ceil32(L)` 当且仅当 `p < L`，`p > floor32(H)` 当且仅当 `p > H`，因此该测试是精确的，而非有界近似。

**误差界（逐三角形、逐查询）。** 记 `u = 2^-24`。输入：f32 顶点与 f64 参考顶点之差不超过 `u|x|`；方向误差不超过 `u`；查询点精确。令 `m = max|a,b,c 坐标| (1+u)`，安全幅值 `X = |x^|inf + eps_x`（同时界定计算值与精确值），一阶前向分析得：

| 量 | 误差界 |
|---|---|
| `e1 = b-a`、`e2 = c-a` | `eps_e = 4 u m`（两次输入舍入 + 一次减法） |
| `s = o - a` | `eps_s = u (|o|inf + 2m)` |
| `h = d x e2` | `eps_h = 6 u E + 2 eps_e`（叉积：`gamma_2 * 2XY` 舍入加输入扰动 `2(eps_x Y + X eps_y)`） |
| `q = s x e1` | `eps_q = 4 u S E + 2 (eps_s E + S eps_e)` |
| `det = e1.h` | `9 u E H + 3 (eps_e H + E eps_h)`（点积：`gamma_3 * 3XY` 加 `3(eps_x Y + X eps_y)`） |
| `U = s.h`、`V = d.q`、`T = e2.q` | 按同一点积规则代入各自操作数 |

每个分子误差界再乘 `SAFETY = 2`，并加 `1e-36`（冲零余量）。比值 `r = N/det` 以 `N * (1/det)` 计算（WGSL 除法 2.5 ULP = 5u 相对误差，乘法 u），误差界为

`err_r = (eps_N + 1.01 |r| eps_det) / (|det| - eps_det) + SAFETY * 7u |r|`，仅在 `|det| > eps_det` 时有效（否则为不确定）。

`u+v` 额外加 `u|u+v|`。每个比较 `x >= thr` 由 `cert_ge` 判定：若 `x - err - slack >= thr` 则确定为真，若 `x + err + slack < thr` 则确定为假，`slack = u(|thr| + |x| + err) + 1e-36` 吸收阈值的 f32 表示误差（f32 中 `1 + 1e-10` 即 `1.0`）以及判定本身的舍入。NaN 或无穷误差界视为不确定。`SAFETY = 2` 覆盖被忽略的二阶项、`gamma_k` 的 `1/(1-ku)` 因子、FMA 合并（只会减少舍入次数）以及 CPU 自身的 f64 计算误差（形式相同但以 `2^-53` 计，低于 f32 误差界的 `2^-29`）。

近平行三角形使 `|det|` 与其误差界相当，此时无法界定比值。除法之前先用无除法的分子测试证明未命中：若 `|U^| - eps_U > (|det^| + eps_det)(1 + 16u)`，则任意可能的 CPU 行列式都给出 `|u| > 1 + 1e-10`；对 `V` 同理给出 `|v| > 1 + 2e-10`；二者都与命中矛盾（或 CPU 直接拒绝 `det`）。若无此测试，每个侧向三角形都会使查询不确定（`particles.stl` 的查询 100% 不确定）；加入后比例降至 0.2%。

若**任一** CPU 条件确定为假，三角形确定未命中（CPU 结果是这些条件的合取）；仅当**全部**确定为真时确定命中；否则不确定。只要一个三角形不确定，整个查询即不确定。命中只有在排序后每个相邻间隔都超过 `1e-8 + 2 max(err_t) + u t` 时才确定为互不相同；此时 CPU 保留每个命中，奇偶性即命中数。任何更近的一对（射线穿过共享边或顶点、重合重复三角形、薄于误差界的板）均为不确定。超过 64 次命中的溢出路径以常数存储完成同样证明：一轮求误差界，然后每个不同命中各做一轮最近选择和一轮带内计数。

**主机重算。** 不确定查询追加到原子列表：MC 以 7 个字记录 `(逻辑样本 id, p, q)`，并向工作组部分和贡献 `valid = 1, hit = 0`；主机以 `id / samples_per_radius` 还原半径，CPU 判定两点都在内时加一次命中。体素先写 0，并记录扁平单元索引；主机判定中心，把 `1` 写回驻留占据场（合并连续区间的 `write_buffer`，使驻留 shell 阶段看到认证后的场），并加到计数或完整读回结果中。计数器始终递增，因此能检测列表溢出；此时将列表扩容到精确数量并重新 dispatch 该批次（逻辑样本 id 与单元中心是确定的）。MC 列表初始 1024 条记录；体素列表预设为 `max(1024, cells/64)` 条（`voxel_uncertain_entries`），覆盖实测的普通比例而无需重新 dispatch。重算为串行，因为优化器在 Rayon 工作线程中持有 GPU mutex 调用它。

**统计。** `GpuS2Pipeline::certification_stats` / `GpuVoxelPipeline::certification_stats` 返回 `GpuCertificationStats { queries, uncertain, list_regrowths, cpu_recompute_seconds }`（MC 查询数为有效样本数；体素查询数为单元数）。`measure` 在每次 GPU MC 求值后打印 MC 比例，GPU exact 在 `[Info] GPU exact S2` 行打印体素比例，`optimize` 在 `Optimization completed.` 之前打印共享实例的累计比例。

**不声称的内容。** 该认证针对的是在原点平移后的 f64 坐标系中、于 GPU 的 f32 查询点上的 CPU 判定；它不是关于未平移世界坐标 CPU 路径（其在 `1e9` 偏移处有自身舍入）的结论，也不改变 MC 样本位置。不确定列表超出规划容量的增长受设备限制约束，而非逻辑显存预算（`mc_evaluation_peak` 与 `ExactMemoryPlan` 计入规划列表及其 staging，MC 规划器在下一次求值时计入已保留的扩容列表）。

**实测（llvmpipe，release，1 次预热 + 5 次交替采样，4 核共享主机）。** 重算比例：MC 1.6e-3（3 级 icosphere，1280 面）与 2.2e-3（`particles.stl`，5600 面）；体素 1.4e-3（icosphere，64³）与 1.3e-2（`particles.stl`，62×59×64）。主机重算不到认证运行时间的 2%。认证着色器每个三角形的 ALU 成本约为两倍：MC 中位数 0.174 s 对未认证 0.137 s（icosphere）、3.98 s 对 1.65 s（particles）；体素 0.450 s 对 0.585 s（icosphere，本次认证路径更快）、4.66 s 对 2.22 s（particles）。这些是负载主机上的软件实现计时，不代表硬件 GPU。在对抗性样例上，未认证着色器出错而认证路径精确：3e-7 厚的板在 4096 个体素中错 743 个，MC 命中数为 267/101/29/8 而正确值为 0。

## CPU exact 规划器校准（PERF-07）

release 5 个样本中位数（`exact_cost_model_calibration`，秒，同一网格上的 FFT 与直接法；旧固定规则对这些情况全部选择 FFT）：

| 网格 | r_max | workers | FFT | 直接法 | 规划器选择 |
|---|---:|---:|---:|---:|---|
| 32³ | 2 | 1 | 0.006317 | 0.001014 | 直接法 |
| 32³ | 8 | 1 | 0.006575 | 0.025101 | FFT |
| 64³ | 2 | 1 | 0.085595 | 0.007118 | 直接法 |
| 64³ | 6 | 1 | 0.079422 | 0.090754 | FFT |
| 96³ | 3 | 1 | 0.427821 | 0.050650 | 直接法 |
| 128×128×32 | 4 | 1 | 0.197231 | 0.065435 | 直接法 |
| 32³ | 2 | 4 | 0.006848 | 0.000456 | 直接法 |
| 32³ | 8 | 4 | 0.006947 | 0.009020 | FFT |
| 64³ | 2 | 4 | 0.117273 | 0.003751 | 直接法 |
| 64³ | 6 | 4 | 0.139758 | 0.051895 | 直接法 |
| 96³ | 3 | 4 | 0.498781 | 0.033485 | 直接法 |
| 128×128×32 | 4 | 4 | 0.319237 | 0.042846 | 直接法 |

使用拟合常数后，规划器在全部 12 种情况都选中更快的内核。主机与其他任务共享，4 worker FFT 数据（相对 1 worker 无加速）可能低估空闲机器上的 FFT 扩展性；因此模型不给 FFT 并行加成，仅在两者接近时偏向 FFT。

平滑填充（`smooth_padding_benchmark`，4 workers，5 次中位数，正变换+功率谱+逆变换秒数，2N-1 -> 平滑）：50³ 0.024585 -> 0.023407；64³ 0.086713 -> 0.076775；71×67×53 0.074856 -> 0.073318；100×100×20 0.052879 -> 0.036489。

**半径分批与共享设备（PERF-03/05/08）。** GPU MC 不再限制 `r_max < 128`：按每批最多 128 个半径（WGSL `radii` 数组长度）分批求值，每批传入 `radius_base`；着色器以全局编号 `radius * samples + sample` 作为随机数键，因此固定种子下的整数计数与批大小无关，也与 `r_max` 无关。唯一约束是 `(r_max + 1) * samples <= u32::MAX`。CPU 合并工作组部分和的代价为 O((r_max+1) * ceil(samples/256))。所有 GPU 管线现按 `RUSTMSPT_GPU_DEVICE` 选择器共享一个进程级逻辑设备，并在每个设备上只编译一次各着色器；见 `reference/gpu.md` 的“共享设备与管线缓存（PERF-03）”。measure 与 optimize 对任意 `r_max` 均可使用 GPU MC。
