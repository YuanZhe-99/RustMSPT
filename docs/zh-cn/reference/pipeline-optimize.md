# Pipeline: Optimize 参考文档

本页面记录 `src/pipeline/optimize.rs`（1141 行 —— 代码库中体量最大的文件）。它实现了颗粒重排优化器：一个在退火之前进行的预剪枝阶段，用于移除颗粒以逼近目标体积分数；随后是模拟退火 (SA) 搜索 —— 可选地以多个并行“岛屿”的形式运行，各岛屿会周期性地交换各自的最优解 —— 通过扰动颗粒的位置/朝向来匹配目标两点相关函数 `S2(r)`。

**另请参阅：** [geometry-core.md](geometry-core.md) 了解 SA 循环中始终使用的 `SpatialGrid` 以及网格/包围盒/碰撞相关的基础组件。[geometry-analysis.md](geometry-analysis.md) 了解用于对候选构型评分的 `calculate_s2`、`l2_norm` 及 GPU S2 流水线。

## 索引

| 项目 | 位置 | 摘要 |
|---|---|---|
| `OptimizePipeline` | `src/pipeline/optimize.rs:27` | 流水线入口结构体，封装解析后的 `OptimizationConfig`。 |
| `ParticlePrepared` | `src/pipeline/optimize.rs:32` | 单个颗粒的缓存：网格 + 预计算的包围盒 + parry3d 碰撞形状。 |
| `IslandResult` | `src/pipeline/optimize.rs:39` | 一次 SA 岛屿运行的结果：最优颗粒集/损失/S2，以及性能分析耗时。 |
| `GlobalBest` | `src/pipeline/optimize.rs:47` | 跨岛屿共享的最优解，由 `Arc<Mutex<GlobalBest>>` 保护。 |
| `prepare_particle` | `src/pipeline/optimize.rs:53` | 从原始网格构建一个 `ParticlePrepared`（包围盒 + parry3d 形状）。 |
| `format_s2_series` | `src/pipeline/optimize.rs:60` | 将 S2 向量格式化为固定精度、空格分隔的字符串。 |
| `push_history_s2` | `src/pipeline/optimize.rs:69` | 向本次运行的历史日志中追加一条带标签的 S2 快照行。 |
| `prune_progress_message` | `src/pipeline/optimize.rs:74` | 构建剪枝阶段进度条所显示的状态字符串。 |
| `selective_prune_to_target_vf` | `src/pipeline/optimize.rs:86` | 退火前阶段：迭代地移除颗粒以逼近目标体积分数，同时尽量减小 S2 损失的增幅。 |
| `run_sa_island` | `src/pipeline/optimize.rs:285` | 单个岛屿的核心模拟退火循环；是代码库中最重要的单一函数。 |
| `OptimizePipeline::run` | `src/pipeline/optimize.rs:797` | 顶层 `Pipeline::run` 编排流程：加载、目标计算、剪枝、单/多岛屿 SA、保存。 |

---

## 类型

#### OptimizePipeline

- **种类：** `pub struct OptimizePipeline`
- **源码位置：** `src/pipeline/optimize.rs:27-29`
- **用途：** 持有完整解析后的 `OptimizationConfig`，并通过 `OptimizePipeline::run` 实现 `Pipeline` trait。

| 字段 | 类型 | 含义 |
|---|---|---|
| `config` | `OptimizationConfig` | 解析后的配置：输入/输出路径、目标 S2 规格、盒体尺寸，以及 `OptimizationParams` 块（SA 超参数、剪枝设置、岛屿模型设置、加速模式）。 |

#### ParticlePrepared

- **种类：** `struct ParticlePrepared`（派生 `Clone`，仅在本模块内私有）
- **源码位置：** `src/pipeline/optimize.rs:31-36`
- **用途：** 将颗粒的网格与预计算的加速数据打包在一起，使得 SA 热循环内部的包围盒和碰撞检测无需在每次迭代中重新计算。

| 字段 | 类型 | 含义 |
|---|---|---|
| `mesh` | `crate::types::Mesh` | 颗粒的三角网格，位于世界坐标系中。 |
| `bbox` | `Option<BoundingBox>` | 轴对齐包围盒；若网格退化（例如为空）则为 `None`。 |
| `shape` | `Option<TriMesh>` | 预计算的 `parry3d_f64::shape::TriMesh`，用于通过 `mesh_collision_exact_prepared` / `mesh_distance_exact_prepared` 进行精确的网格-网格碰撞/距离查询。 |

- **说明：** 由 `prepare_particle` 构建。每次迭代中只有被扰动的那一个颗粒会被重新准备（`prepared[idx] = prepare_particle(candidate)`）；其余颗粒保持不变直接复用，这正是使 `run_sa_island` 每次迭代开销可控的关键所在。

#### IslandResult

- **种类：** `struct IslandResult`（仅在本模块内私有；部分字段带有 `#[allow(dead_code)]`）
- **源码位置：** `src/pipeline/optimize.rs:38-45`
- **用途：** `run_sa_island` 一次调用返回的结果：该岛屿找到的最优解，以及用于最终耗时汇总的性能分析数据。

| 字段 | 类型 | 含义 |
|---|---|---|
| `best_particles` | `Vec<crate::types::Mesh>` | 运行过程中观察到的最优（损失最低）构型下的颗粒网格。 |
| `best_loss` | `f64` | `best_particles` 的 S2 曲线相对目标曲线的 L2 损失。 |
| `best_s2` | `Vec<f64>` | 对应 `best_particles` 的 S2 曲线。 |
| `s2_time` | `Duration` | 计算候选 S2 曲线所耗费的累计墙钟时间。 |
| `collision_time` | `Duration` | 边界/碰撞检测所耗费的累计墙钟时间。 |

- **说明：** `OptimizePipeline::run` 在各岛屿之间使用 `min_by` 依据 `best_loss` 选出总体获胜者，并在最终耗时汇总行中汇总/报告获胜岛屿的 `s2_time`/`collision_time`。

#### GlobalBest

- **种类：** `struct GlobalBest`（仅在本模块内私有）
- **源码位置：** `src/pipeline/optimize.rs:47-50`
- **用途：** 岛屿之间在迁移时交换的共享状态。被包裹在 `Arc<Mutex<GlobalBest>>` 中，并在 `num_islands > 1` 时以 `global_best: Option<&Arc<Mutex<GlobalBest>>>` 的形式传给每一次 `run_sa_island` 调用。

| 字段 | 类型 | 含义 |
|---|---|---|
| `loss` | `f64` | 目前所有岛屿中已知的最优损失（初始化为 `f64::MAX`）。 |
| `particles` | `Vec<crate::types::Mesh>` | 达到该损失的颗粒构型。 |

- **说明：** 迁移是在互斥锁保护下进行的一次比较并交换：无论是岛屿一方还是全局记录一方，较差的一方会采纳另一方的解。参见下文 `run_sa_island` 中的“岛屿迁移”小节。

---

## 函数

#### prepare_particle

- **签名：** `fn prepare_particle(mesh: crate::types::Mesh) -> ParticlePrepared`
- **源码位置：** `src/pipeline/optimize.rs:53`
- **用途：** 为单个颗粒网格预计算包围盒和 parry3d 碰撞形状。
- **参数：** `mesh` —— 该颗粒的三角网格（按值消费）。
- **返回值：** 一个 `ParticlePrepared`，封装了该网格、其 `mesh_bbox` 以及其 `to_parry_trimesh` 形状。
- **副作用：** 无。

#### format_s2_series

- **签名：** `fn format_s2_series(values: &[f64]) -> String`
- **源码位置：** `src/pipeline/optimize.rs:60`
- **用途：** 将一条 S2 曲线渲染为便于日志阅读的固定精度字符串。
- **参数：** `values` —— S2 采样值，每个半径分箱一个。
- **返回值：** 空格分隔的字符串，每个值格式化为 6 位小数（`{v:.6}`）。
- **副作用：** 无。

#### push_history_s2

- **签名：** `fn push_history_s2(history_log: &mut Vec<String>, label: &str, values: &[f64])`
- **源码位置：** `src/pipeline/optimize.rs:69`
- **用途：** 向本次运行的历史日志追加一条带标签的 S2 快照（例如 `"Target S2"`、`"Input S2"`、`"Final Best S2"`），该日志最终会写入 `s2_history.txt`。
- **参数：** `history_log` —— 可变的日志缓冲区；`label` —— 行前缀；`values` —— 需通过 `format_s2_series` 格式化的 S2 曲线。
- **返回值：** 无。
- **副作用：** 向 `history_log` 追加一行格式化后的文本。

#### prune_progress_message

- **签名：** `fn prune_progress_message(current_loss: f64, current_vf: f64, target_vf: f64, particles: usize) -> String`
- **源码位置：** `src/pipeline/optimize.rs:74`
- **用途：** 构建剪枝阶段进度条上显示的单行状态字符串。
- **参数：** `current_loss` —— 当前颗粒集下的 S2 L2 损失；`current_vf` —— 当前体积分数；`target_vf` —— 目标体积分数；`particles` —— 当前颗粒数量。
- **返回值：** 形如 `"Loss {current_loss:.6} | VF {current_vf:.6}->{target_vf:.6} | particles {particles}"` 的字符串。
- **副作用：** 无。

#### selective_prune_to_target_vf

- **签名：** `fn selective_prune_to_target_vf(particles: &mut Vec<crate::types::Mesh>, box_bounds: BoundingBox, target_s2: &[f64], params: &crate::config::OptimizationParams, r_max: usize, s2_method: &str, voxel_pitch: f64, thread_pool: &ThreadPool, history_log: &mut Vec<String>)`
- **源码位置：** `src/pipeline/optimize.rs:86`
- **用途：** 退火前的剪枝阶段。逐批从输入颗粒集中移除颗粒，直到体积分数 (VF) 落入目标 VF 的容差范围内（目标 VF 即 `target_s2[0]`，因为 `S2(0) = VF`），同时在此过程中尽量保持相对目标的 S2 损失处于较低水平。
- **参数：**
  - **输入状态：** `particles` —— 原地修改；待剪枝的颗粒集。
  - **目标：** `target_s2` —— 完整的目标 S2 曲线（仅使用 `target_s2[0]` 作为 VF 目标）。
  - **配置：** `params` —— 读取 `prune_enabled`（默认 `true`）、`prune_tolerance`（默认 `0.01`）、`prune_max_rounds`（默认 `200`）、`prune_eval_samples`（默认 `max(mc_samples/4, 1000)`）以及 `mc_samples`。
  - **盒体/S2 评估：** `box_bounds`、`r_max`、`s2_method`、`voxel_pitch`、`thread_pool` —— 用于在每次候选移除后重新计算 VF 和 S2 损失。
  - **日志：** `history_log` —— 可变的日志缓冲区。
- **返回值：** 无；`particles` 被原地剪枝。
- **副作用：** 修改 `particles` 和 `history_log`；向标准输出打印 `[Info]` 进度行；驱动一个进度条；执行大量 `calculate_s2` 评估（由于每次候选移除都被单独打分，此阶段可能开销较大）。
- **提前退出：** 若 `particles.len() < 2`、`prune_enabled` 为 `false`，或目标 VF（`target_s2[0]`，被限制在 `[0,1]` 范围内）`<= 0.0`，则立即返回（不做任何操作）。
- **算法：**
  1. 计算当前 VF（`volume_fraction_of_meshes_in_bbox`）和当前 S2 损失（`l2_norm` 相对 `target_s2`，在 `min(r_max, target_s2.len()-1)` 个半径分箱上使用 `prune_eval_samples` 个蒙特卡洛样本进行评估 —— 相对主 SA 循环而言样本数较少，因为剪枝阶段每移除一个颗粒都要进行更多次 S2 评估）。
  2. 当 `current_vf > target_vf * (1.0 + tol)` 且剩余颗粒数大于一个且轮次数未超过 `prune_max_rounds` 时循环：
     - 计算 `vf_gap = current_vf - target_vf`，并按此差距缩放批量大小：`vf_gap > 0.05` → 采样 30 个候选颗粒，移除其中最优的 10 个；`vf_gap > 0.025` → 采样 15 个，移除最优的 3 个；否则 → 采样 6 个，移除最优的 1 个。这使得早期轮次（远离目标）更为激进，而后期轮次（接近目标）更为精细。
     - 随机采样 `n_candidates` 个互不相同的颗粒索引（`rand::seq::index::sample`）。
     - 对每个采样索引，构建一个临时颗粒集（移除该颗粒），并评估其结果 `(vf, loss)`。
     - 对候选项打分并排序：移除后能使 VF **保持在或高于**目标值的候选项优先于会使 VF 降到目标以下的候选项；同组内，对于“仍高于目标”的候选项，先按 S2 损失最低再按 VF 与目标最接近打破平局；对于“降到目标以下”的候选项，先按 VF 与目标最接近再按 S2 损失最低打破平局。这意味着只要还存在高于目标的选项，剪枝器就不会使 VF 低于目标值过冲。
     - 从 `particles` 中移除得分最高的 `k_remove` 个候选项（按索引，从高到低降序移除以保持剩余索引有效）。
     - 在缩小后的颗粒集上重新计算 `current_vf`/`current_loss`，更新进度条，并且每一轮都记录一行 `"Pruning Round N"` 历史日志（每 5 轮或收敛时额外打印一行 `[Info]` 标准输出）。
  3. 循环退出时（达到目标、达到轮次上限，或剩余颗粒过少），记录一行 `"Pruning Completed"`，包含最终轮次数、颗粒数、VF 和损失。
- **说明：** 这是一种贪心的、基于采样的局部搜索启发式算法，并非精确算法 —— 它从不评估移除*全部*颗粒的情况，每轮只对随机采样的子集进行评估，因此可能收敛到局部较优但并非全局最优的 VF/损失折中方案。无论配置的加速后端如何，它始终完全在 CPU 线程池上运行（没有 GPU 路径）。

> **算法：** 关于剪枝为何要先于退火进行、以及它如何与 SA 损失地形相互作用的进一步背景说明，请参阅 `../algorithms/simulated-annealing-island-model.md`。

#### run_sa_island

- **签名：**
  ```rust
  #[allow(clippy::too_many_arguments)]
  fn run_sa_island(
      island_id: usize,
      num_islands: usize,
      prepared_init: Vec<ParticlePrepared>,
      target: &[f64],
      params: &crate::config::OptimizationParams,
      box_bounds: BoundingBox,
      mode: u8,
      d1: f64,
      d2: f64,
      min_neighbor: f64,
      rotation_mode: &RotationMode,
      thread_pool: &ThreadPool,
      global_best: Option<&Arc<Mutex<GlobalBest>>>,
      migration_interval: usize,
      history_log: &mut Vec<String>,
      #[cfg(feature = "gpu")] mut gpu_pipeline: Option<crate::gpu::s2::GpuS2Pipeline>,
  ) -> IslandResult
  ```
- **源码位置：** `src/pipeline/optimize.rs:285`
- **用途：** 对颗粒的位置/朝向运行一次完整的模拟退火搜索，每次迭代随机扰动一个颗粒，以匹配目标 S2 曲线。这是整个优化器的计算核心，也是迄今为止代码库中最重要的函数。
- **参数（按组划分）：**
  - **输入状态：** `prepared_init` —— 初始的 `ParticlePrepared` 集合（在各次迭代中作为 `prepared` 被消费并修改）；`target` —— 目标 S2 曲线。
  - **配置：** `params` —— 完整的 `OptimizationParams` 块，下面几乎每个 SA 超参数都从中读取（温度调度、移动步长、接受率窗口调优、S2 采样数）。
  - **盒体/边界：** `box_bounds` —— 优化区域；`mode` —— 边界模式（`3` 启用周期性环绕 + 幽灵颗粒碰撞检测）；`d1`、`d2` —— 传给 `check_boundary_constraints_mode` 的边界距离参数；`min_neighbor` —— 颗粒间表面允许的最小间隙。
  - **旋转：** `rotation_mode` —— 约束所采样的旋转轴（参见 `pipeline::rotation::RotationMode`）。
  - **执行：** `thread_pool` —— 该岛屿（CPU）S2 评估所运行的 rayon 线程池。
  - **岛屿模型：** `island_id`、`num_islands` —— 用于日志中标识本岛屿；`global_best` —— 跨岛屿共享的最优解（单岛屿运行时为 `None`）；`migration_interval` —— 尝试迁移之间间隔的迭代次数。
  - **日志：** `history_log` —— 用于记录 S2/损失快照的可变日志缓冲区。
  - **GPU：** `gpu_pipeline` —— 受 `#[cfg(feature = "gpu")]` 门控的可选持久化 GPU S2 流水线；当为 `Some` 时，S2 在 GPU 上计算而非 CPU。
- **返回值：** `IslandResult` —— 观察到的最优颗粒构型/损失/S2，以及累计的 `s2_time` 和 `collision_time`。
- **副作用：** 在迁移检查点通过互斥锁修改 `global_best`；在接受新最优解及发生迁移事件时向 `history_log` 追加行；打印 `[Info]` 启动行并驱动逐迭代进度条；当 `gpu_pipeline` 为 `Some` 时执行 GPU 派发。

> **特性门控（部分）：** `gpu_pipeline` 参数以及所有读取它的分支仅在 `#[cfg(feature = "gpu")]` 下编译；对应的 `#[cfg(not(feature = "gpu"))]` 分支始终走 CPU（`thread_pool.install(|| calculate_s2(...))`）路径。`run_sa_island` 本身在两种配置下都能正确编译并运行 —— 只有 GPU S2 代码路径是条件性存在的。

- **算法 —— 初始化：**
  - 从 `params` 中读取 SA 超参数：`initial_temperature`（下限为 `1e-8`）、`cooling_rate`（限制在 `[0.8, 0.99999]`）、`adaptive_temp_window`（默认 `clamp(max_iterations/40, 20, 100)`）、`target_acceptance_low`/`target_acceptance_high`（默认分别为 `0.20`/`0.45`，且强制 `high` 高于 `low`）、`adaptive_heat_factor`（默认 `1.08`）、`adaptive_cool_factor`（默认 `0.94`）、`adaptive_temp_ceiling_factor`（默认 `5.0`，得出 `temp_ceiling = initial_temperature * factor`）；`temp_floor` 固定为 `1e-9`。
  - 构建初始合并网格并计算初始 S2 曲线，可通过 GPU 流水线完成（`gpu.update_mesh` + `gpu.calculate_s2_gpu`，并将 `s2[0]` 用 CPU 精确计算出的体积分数覆盖），也可通过 CPU 的 `calculate_s2`（使用 `mc_samples.max(2000)` 个样本）完成。计算 `current_loss = l2_norm(current_s2, target)`，并记录为 `"Post-Pruning S2"` / `"Post-Pruning Loss"`。
  - 将 `best_particles`/`best_loss`/`best_s2` 初始化为起始构型。
  - 基于所有颗粒的包围盒构建初始 `SpatialGrid`（使用 `estimate_cell_size` 确定单元大小），用于限定邻居范围的碰撞查询。
- **算法 —— 逐迭代循环**（最多 `params.max_iterations` 次，每次迭代扰动一个颗粒）：
  1. **选择颗粒：** 在 `prepared` 中均匀随机选取索引 `idx`；将其网格克隆为 `original`/`candidate`。
  2. **选择移动类型**：通过一次均匀随机掷骰决定，并按 `scale = clamp(temperature / initial_temperature, 0.05, 1.0)` 进行缩放（因此随着系统降温，移动幅度会相应收缩）：
     - **60% —— 局部平移 + 旋转：** 沿每个坐标轴在 `[-max_translation, max_translation] * scale` 范围内随机平移，作用于颗粒的质心；此外，若 `max_rotation_deg.to_radians() * scale > 1e-6`，则围绕采样得到的旋转轴（`sample_rotation_axis`，遵循 `rotation_mode`）随机旋转一个位于 `[-rot_limit, rot_limit]` 的角度。
     - **30% —— 朝随机邻居移动：** 选取另一个随机颗粒，将候选颗粒的质心沿方向向量朝其移动 `min(dist * 0.3, max_translation * scale * 2.0)`，并附加一个较小的随机旋转（`±10° * scale`）。
     - **10% —— 完全随机重新定位：** 将质心瞬移到 `box_bounds` 内的一个均匀随机点，并附加一个在 `[0, 2π)` 范围内完全随机的旋转。
  3. **周期性环绕：** 若 `mode == 3`，则将候选颗粒的质心环绕回盒体内（`wrap_mesh_centroid_to_box`）。
  4. **边界检查：** `check_boundary_constraints_mode(candidate, box_bounds, mode, d1, d2)`；若检查失败，该移动在从未计算 S2 的情况下即被拒绝 —— 温度依然照常降低，自适应窗口的接受率计数器也照常推进（参见第 8 步），随后 `continue`。
  5. **碰撞检查（自身颗粒）：** 在 `SpatialGrid` 中查询候选颗粒包围盒 `min_neighbor` 边距内的邻居，若有包围盒缺失则回退为逐一检查所有其他颗粒。对每个邻居，先使用开销较低的包围盒距离快速判定，再使用精确的 `mesh_collision_exact_prepared`（parry3d）判定真实重叠——自 0.2.1 起这指的是两个*实体*重叠，因此整体位于另一个颗粒内部的颗粒会被拒绝，而不再被当作间隙宽裕——并在 `min_neighbor > 0.0` 时使用精确的 `mesh_distance_exact_prepared` 强制执行最小间隙。因此，本就含有嵌套颗粒的输入堆积从第一次扫描起就会被判为受阻；对一个本不合法的集合体而言，这是正确的读法。任何重叠或间隙不足都会将 `blocked` 置为 `true` 并按拒绝冷却流程中止该候选（与第 4 步相同）。
  6. **周期性幽灵碰撞（仅 mode 3）：** 若边界模式为 `3`，则生成候选颗粒的周期性幽灵副本（`generate_periodic_ghosts`），并对每个幽灵副本相对所有其他颗粒重复相同的包围盒/精确碰撞/最小间隙检查；任何幽灵碰撞都会阻止该移动。
  7. **将扰动后的颗粒接受进 `prepared`：** 用 `prepare_particle(candidate)` 替换 `prepared[idx]`，重建 `SpatialGrid`，重新合并网格，并计算候选 S2 曲线。本次评估所用的蒙特卡洛样本数会**随迭代/温度进行缩放**：`adaptive_samples = round(mc_samples * (0.3 + 0.7*scale))`，并限制在 `[1000, mc_samples.max(1000)]` 范围内 —— 温度较高（早期/高温）的迭代使用较少样本以提升速度，降温后期的迭代使用逐渐增多的样本以提升精度。若存在 GPU 流水线则通过其计算 S2，否则在线程池上通过 CPU 的 `calculate_s2` 计算。`candidate_loss = l2_norm(candidate_s2, target)`；`delta = candidate_loss - current_loss`。
  8. **Metropolis 接受准则：** 若 `delta <= 0.0`（候选解不差于当前解），无条件接受；否则以概率 `exp(-delta / temperature)`（限制在 `[0,1]`）通过 `rng.gen_bool` 接受。接受时：提交 `current_s2`/`current_loss`，若 `current_loss < best_loss`，则更新 `best_particles`/`best_loss`/`best_s2` 并记录一行 `"Iter N: Loss ... | S2 ..."` 历史日志。拒绝时：将 `prepared[idx]` 恢复为 `original` 并重建 `SpatialGrid`。
  9. **温度更新：** 每次迭代都执行 `temperature *= cooling_rate`（下限为 `temp_floor`）。此外，每经过 `adaptive_window` 次迭代（通过 `window_trials`/`window_accepts` 试验/接受计数器），会将该窗口内的*接受率*与 `[target_accept_low, target_accept_high]` 进行比较：低于窗口 → 升温（`temperature *= heat_factor`，上限为 `temp_ceiling`）；高于窗口 → 加速降温（`temperature *= cool_factor`，下限为 `temp_floor`）；处于窗口内 → 不改变温度原有的指数衰减轨迹。这一自适应窗口逻辑在每个提前 `continue` 的位置（边界拒绝、碰撞拒绝、幽灵碰撞拒绝）以及一次完整迭代结束时都被同等地应用，因此无论拒绝发生在一次迭代进行到多深的位置，接受率的统计都保持一致。
  10. **岛屿迁移**（仅当 `global_best.is_some()`、`migration_interval > 0` 且 `(iter+1) % migration_interval == 0` 时）：锁定共享的 `GlobalBest` 互斥量，并将 `best_loss` 与 `gb_lock.loss` 进行比较。若本岛屿更优，则将其 `best_particles`/`best_loss` **推送**进全局记录。若全局记录更优，则本岛屿**拉取**全局解：替换 `best_particles`/`best_loss`，对每个传入的网格通过 `prepare_particle` 重新生成 `prepared`，重建 `SpatialGrid`，以完整的 `mc_samples.max(2000)` 精度重新计算 `current_s2`/`current_loss`，并记录一行 `"migrated best loss"`。这就是各岛屿在不对每次迭代状态进行同步的情况下共享进展的机制。
  11. 用当前损失、最优损失、温度及累计接受率百分比更新进度条。若 `temperature < 1e-9`（实际上已冻结），则提前跳出循环。
- **返回值：** 一个 `IslandResult`，包含找到的最优构型以及累计的 `s2_time`/`collision_time` 性能分析耗时（分别在 S2 计算和边界/碰撞检查相关代码路径周围通过 `Instant::now()`/`.elapsed()` 测量）。

> **算法：** 关于完整的模拟退火调度方案、移动选择的设计理由，以及岛屿模型迁移设计的详细说明，请参阅 `../algorithms/simulated-annealing-island-model.md`。

**另请参阅：** [geometry-core.md](geometry-core.md) 记录了 `SpatialGrid`、`bbox_distance`、`bbox_overlaps`、`mesh_collision_exact_prepared`、`mesh_distance_exact_prepared`，以及本循环中始终使用的网格变换辅助函数（`move_mesh_to_target_center`、`rotate_mesh_around_center`、`wrap_mesh_centroid_to_box`、`generate_periodic_ghosts`）。[geometry-analysis.md](geometry-analysis.md) 记录了 `calculate_s2` 和 `l2_norm`，`gpu.md` 记录了 `GpuS2Pipeline`（当启用 `gpu` 特性且选择了加速器时所使用的 `calculate_s2_gpu`/`update_mesh` GPU 路径）。

#### OptimizePipeline::run

- **签名：** `fn run(&self) -> Result<()>`（`OptimizePipeline` 对 `Pipeline` trait 的实现）
- **源码位置：** `src/pipeline/optimize.rs:797`
- **用途：** 对整个 optimize 流水线进行顶层编排：配置初始化、获取目标 S2、退火前剪枝、单岛屿或多岛屿模拟退火，以及结果持久化。
- **参数：** `&self` —— 读取 `self.config: OptimizationConfig`。
- **返回值：** `Result<()>` —— 成功时为 `Ok(())`；配置或输入错误时为 `RustMsptError::InvalidConfig`/`InvalidMesh`，或透传的 I/O 错误。
- **副作用：** 从磁盘读取 STL 输入（单个文件或文件夹）；将优化后的 STL 写入 `self.config.output.path`；在其旁边写入 `s2_history.txt`；向标准输出打印大量 `[Info]`/`[Warning]` 进度与耗时信息。
- **算法：**
  1. **初始化：** 解析 `box_bounds`、边界模式/参数（`mode`、`d1`、`d2`、`min_neighbor`）以及 `rotation_mode`。根据 `params.cpu_max`（`-1` 表示使用全部可用核心）构建一个大小合适的 rayon `ThreadPool`，并限制在 `available_parallelism()` 以内。
  2. **后端选择：** 根据 `box_bounds` 和 `voxel_pitch` 估算体素网格大小，随后调用（来自 `crate::compute::policy` 的）`select_backend` 决定使用 CPU 还是 GPU 加速，并记录所请求/实际生效的后端及任何回退原因。
  3. **加载输入：** 从 `self.config.input.stl_path` 加载 STL（若为目录则使用 `load_folder_stls`，否则使用 `load_stl`），并通过 `split_mesh_into_granules` 将每个加载的网格拆分为独立颗粒。预先过滤掉包围盒与 `box_bounds` 不重叠的颗粒（`bbox_overlaps`），并记录被移除的数量。若没有剩余颗粒则报错。
  4. **目标 S2：** 根据 `self.config.target.r#type` 解析目标曲线：`"manual_array"` 直接读取 `target.s2_array`；`"reference_stl"` 加载参考网格并计算其 S2 曲线（若给定 `target.stl_bounding_box` 则使用之，否则使用该网格自身的包围盒）；其他任何取值均视为配置错误。
  5. **输入阶段度量：** 合并已加载的颗粒，计算其 S2 曲线和体积分数，记录相对目标的输入损失，并记录 `"Target S2"`/`"Input S2"`/`"Input Loss"` 历史日志行。
  6. **剪枝：** 对颗粒集调用 `selective_prune_to_target_vf`（见上文），在退火开始前将体积分数下调至接近目标值。
  7. **准备颗粒：** 通过 `prepare_particle` 将剪枝后的颗粒 `Vec<Mesh>` 转换为 `Vec<ParticlePrepared>`。
  8. **单岛屿与多岛屿分派：** 读取 `params.islands`（默认 `1`）和 `params.migration_interval`（默认 `100`）。
     - **单岛屿（`num_islands <= 1`）：** 可选地初始化一个持久化的 `GpuS2Pipeline`（受特性门控，仅当加速模式不是 `Cpu` 时启用，若 GPU 初始化失败则回退到 CPU），随后以 `global_best = None` 调用一次 `run_sa_island`。
     - **多岛屿（`num_islands > 1`）：** 将 `thread_count` 均分给各岛屿（`threads_per_island = max(thread_count / num_islands, 1)`），创建一个初始化为 `loss: f64::MAX` 的共享 `Arc<Mutex<GlobalBest>>`，并在 `std::thread::scope` 内为每个岛屿生成一个线程，各自运行自己通过 `ThreadPoolBuilder` 构建的线程池、各自可选的 GPU 流水线实例，并以 `global_best = Some(&gb)` 调用 `run_sa_island`。汇合所有岛屿线程后，通过 `min_by` 依据 `best_loss` 选出总体获胜者。
  9. **后处理：** 记录最终的最优 S2/损失；将 `best_particles` 合并为一个网格；若启用了 `params.orient_to_positive_volume`，则调用 `orient_components_to_positive_volume` 翻转法向反转的连通分量（记录在总分量数中翻转了多少个），随后通过 `save_stl` 保存结果。
  10. **历史记录与耗时：** 将累积的 `history_log` 写入输出 STL 旁边的 `s2_history.txt`；打印最终损失、体积、S2 点数、朝向修正状态，以及一行耗时汇总，将总墙钟时间与累计 S2 计算时间、累计碰撞/约束检查时间分列开来（两者均来自获胜岛屿的 `IslandResult`）。
- **说明：** 目标 S2 获取、剪枝以及单岛屿路径都复用同一个顶层 `thread_pool`；而多岛屿路径中每个岛屿都会获得各自独立大小的线程池，因此各岛屿的总 CPU 使用量大致受 `thread_count` 约束（近似而言 —— 各岛屿的线程池是相互独立的，除了均分之外并无全局协调）。
