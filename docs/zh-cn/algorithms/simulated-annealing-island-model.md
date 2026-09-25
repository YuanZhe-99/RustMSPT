# 优化流水线中的模拟退火与岛屿模型

## 该流水线的作用

`OptimizePipeline`（`src/pipeline/optimize.rs`）重建一个颗粒微结构，使其两点相关函数 (S2) 与目标 S2 曲线匹配。目标既可以直接来自配置数组（`target.type = "manual_array"`），也可以从参考 STL 网格中测得（`target.type = "reference_stl"`）。流水线从一个输入颗粒装配体出发（从 STL 加载并拆分为各个颗粒），搜索这些颗粒的一种排列方式——位置与朝向——使其 S2 曲线在满足边界与不重叠约束的前提下尽可能接近目标。

搜索方法是**模拟退火 (SA)**：一种随机局部搜索的元启发式算法。每一步都对当前构型提出一个随机扰动，评估该扰动是使拟合目标 S2 变好还是变差，并用一条规则来接受或拒绝它——该规则总是接受改进，但*有时*也会接受变差的移动。这种接受更差解的意愿——由一个随运行过程递减的温度参数控制——使搜索在早期（温度较高时）能跳出局部极小值，并在运行后期（温度较低时）收敛到精细调优的最优解。

围绕核心 SA 循环有两个结构性组件：

- **退火前剪枝**（`selective_prune_to_target_vf`）在 SA 开始*之前*将颗粒集合朝目标体积分数方向修剪，因为移除颗粒的成本远低于通过成千上万次昂贵的 S2 重计算来把体积分数扰动到匹配值。
- **岛屿模型**并行运行多个独立的 SA 搜索（`optimization.islands > 1`），每个岛屿探索解空间的不同区域，并定期迁移目前找到的最佳解。

## 退火前剪枝：`selective_prune_to_target_vf`

SA 每次迭代的开销主要来自 S2 的重新计算（对整个装配体进行蒙特卡洛或体素化的相关性计算）。如果加载的颗粒集合数量远超目标体积分数（VF）所要求的水平——VF 读取自 `target[0]`，即目标 S2 曲线上 r=0 的点，其值等于目标固相体积分数——那么 SA 预算中会有很大一部分被耗费在一次接受/拒绝决策接一次地逐步稀疏装配体上。剪枝提前完成这一稀疏化过程，使用的是成本低得多的贪心/候选打分策略，而非完整的退火。

该算法在当前 VF 超过 `target_vf * (1 + prune_tolerance)` 时按轮次运行：

1. **自适应批量大小。** 每轮的候选池大小与移除数量根据当前 VF 与目标的差距（`vf_gap`）进行缩放：
   - `vf_gap > 0.05`：采样 30 个候选，移除得分最好的 10 个。
   - `vf_gap > 0.025`：采样 15 个候选，移除得分最好的 3 个。
   - 其他情况：采样 6 个候选，移除得分最好的 1 个。

   差距较大时需要激进的批量移除；随着装配体接近目标 VF，批量逐渐缩小，以免剪枝过头而不必要地损害 S2。

2. **候选打分。** 对随机采样的 `n_candidates` 个颗粒索引中的每一个，剪枝器暂时移除该颗粒，重新计算精简后集合的 VF 与 S2 损失（相对目标的 `l2_norm`），并记录 `(index, still_above_target_vf, loss, vf)`。

3. **选择移除对象。** 候选按两级规则排序：优先保留能使 VF 维持在目标之上的候选，而非低于目标的候选（移除过多比移除不足更糟）；同一级内，先按较低的 S2 损失打破平局，再按与目标 VF 的接近程度打破平局。按此排序取前 `k_remove` 个候选，一并在本轮移除。

4. **循环控制。** 此过程最多重复 `prune_max_rounds`（默认 200）轮，或直到满足 VF 容差为止。`prune_eval_samples`（默认 `mc_samples / 4`，最小值 1000）控制这些中间 S2 评估所使用的蒙特卡洛采样数——刻意比 SA 后续使用的全精度 S2 更粗糙，因为剪枝只需要足够的信号来给候选排序，而不需要出版级别的精度。

剪枝可以通过 `prune_enabled = false` 完全禁用，并且当颗粒数少于 2 个或目标 VF 非正数时会自动跳过。

## SA 核心循环：`run_sa_island`

每次调用 `run_sa_island` 都会对（已剪枝的）颗粒集合运行一次完整、自包含的模拟退火搜索。它返回一个 `IslandResult`，其中包含找到的最佳颗粒排列、其损失值、其 S2 曲线，以及 S2 评估与碰撞/约束检查的耗时分解。

### 初始化

在循环开始之前，函数为每个颗粒准备加速结构（`ParticlePrepared`：网格 + 包围盒 + 通过 `prepare_particle` 得到的 parry3d 碰撞形状），计算初始合并网格的 S2 与损失，并在颗粒包围盒之上构建一个 `SpatialGrid`（`estimate_cell_size` + `SpatialGrid::build`），用于碰撞检查期间的快速邻居查询。温度从 `initial_temperature` 开始。

### 单次迭代

`max_iterations` 次迭代中的每一次：

1. **随机选取一个颗粒**（`idx`）进行扰动，并保存其原始网格，以便移动被拒绝时可以回滚。

2. **提出一个移动。** 一次均匀随机掷骰以固定概率在三种移动类型中选一种：

   - **60% —— 局部平移+旋转。** 在每个轴上于 `±max_translation` 范围内随机平移，并在 `±max_rotation_deg` 范围内随机旋转，二者都按 `scale = clamp(temperature / initial_temperature, 0.05, 1.0)` 缩放。随着温度降低，`scale` 收缩，这些扰动会逐渐变细——早期是大幅探索性跳跃，后期是小幅精细调整。这是堆积局部精细调优的主力移动类型。

   - **30% —— 朝随机邻居移动。** 挑选另一个随机颗粒，计算从当前颗粒质心指向它的方向，并沿该方向前进 30% 的距离（上限为 `max_translation * scale * 2.0`），再加上一个小幅缩放旋转（满温度时为 ±10°）。这会使搜索偏向于将颗粒*聚拢*在一起，这通常是提高短程相关性、匹配蕴含颗粒接触/聚集的 S2 曲线所必需的——单纯的局部抖动只能缓慢地达到这种效果。

   - **10% —— 完全随机重定位+旋转。** 将颗粒放置在盒子边界内一个均匀随机的位置，并给予完全随机的旋转。这是"大跳跃"移动：与温度缩放无关，它使搜索能够跳出局部及邻居导向移动无法摆脱的构型，并在运行后期仍能持续探索整个解空间。

3. **周期性包裹。** 如果边界 `mode == 3`（周期性），移动之后候选颗粒的质心会被包裹回盒子内（`wrap_mesh_centroid_to_box`），因为周期模式允许颗粒连续跨越边界移动。

4. **约束检查（在昂贵打分之前先做廉价拒绝）。** 依次进行三项检查，每一项都能在任何 S2 重计算（这是最昂贵的步骤）之前立即拒绝该移动：

   - `check_boundary_constraints_mode` —— 针对所配置模式的盒/边界深度约束。
   - **碰撞检查。** 一次全新的 `SpatialGrid` 邻居查询（`query_neighbors_with_margin`）缩小候选邻居范围，随后精确网格碰撞检测（`mesh_collision_exact_prepared`），以及在设置了 `min_neighbor_distance` 时的精确网格距离检测（`mesh_distance_exact_prepared`），如果该移动与其他颗粒重叠或违反最小邻距间隙，则拒绝该移动。
   - **周期性镜像碰撞**（仅 mode 3）。生成候选颗粒的周期性镜像（`generate_periodic_ghosts`），并对它们运行相同的碰撞/距离检查，从而使靠近周期边界的颗粒不能与自身或他人的镜像重叠。

   以上任一检查失败都会直接拒绝该移动：颗粒隐式地还原（候选状态直接被丢弃——`prepared[idx]` 从未被更新），温度仍按 `cooling_rate` 冷却，自适应温度窗口仍将此次试验计入（记为一次未接受）。随后循环 `continue` 进入下一次迭代，全程未触及 S2。

5. **S2 打分。** 只有当所有约束检查都通过时：该颗粒的网格才会被提交进 `prepared[idx]`，`SpatialGrid` 围绕新构型重建，整个装配体重新合并，并为该*候选*构型重新计算 S2——通过整个运行共享的 `OptimizeS2` 求值器，始终保持解析后的 `voxel_exact`、`voxel_mc` 或 `mesh_mc` 定义。现有 GPU MC 内核仅用于连续网格 MC。用于本次迭代评估的蒙特卡洛采样数 `iter_samples`，其缩放**采用与移动幅度相同的 `scale` 因子**：`((mc_samples as f64) * (0.3 + 0.7 * scale)).round()`，并被限制在 `[1000, mc_samples]` 范围内。具体来说：温度较高时（`scale` 接近 1），迭代使用接近完整 `mc_samples` 预算的采样数；随着温度冷却趋于零，迭代使用的采样数逐渐减少，最低降至 `mc_samples` 的 30%。这与"搜索收敛时精度更紧"的直觉相反——代码实际上是在运行早期（此时移动幅度大、S2 估计中的粗噪声对整体探索的影响较小，但采样预算却被慷慨使用）花费*更多*蒙特卡洛采样（更高精度），而随着移动幅度缩小逐渐收回采样数。（此结论直接从 `run_sa_island` 中的 `adaptive_samples` 公式验证得出；若代码发生变化，请勿在未重新检查该行的情况下假定相反的结论。）

   候选损失为 `l2_norm(candidate_s2, target)`，`delta = candidate_loss - current_loss`。

6. **Metropolis 接受准则。**

   ```
   accept = delta <= 0.0
            || rng.gen_bool(exp(-delta / temperature))
   ```

   改进型移动（`delta <= 0`）总是被接受。变差型移动以概率 `exp(-delta / temperature)` 被接受：损失增量（`delta`）越大，或温度越低，该概率就越小——随着运行冷却，接受更差移动的可能性呈指数级降低，这正是标准的 Metropolis SA 接受规则，也正是使搜索在早期表现为宽泛随机搜索、在后期表现为严格爬坡下降搜索的原因。

   若被接受，候选状态成为当前状态；若 `current_loss` 优于 `best_loss`，最佳解快照（`best_particles`、`best_loss`、`best_s2`）会被更新并记录日志。若被拒绝，`prepared[idx]` 会被恢复为保存的原始网格，`SpatialGrid` 围绕还原后的状态重建。

7. **冷却与自适应温度。** 每次迭代之后（无论是否被接受），温度都按几何方式衰减：`temperature = (temperature * cooling_rate).max(temp_floor)`，其中 `cooling_rate` 被限制在 `[0.8, 0.99999]`，`temp_floor = 1e-9`。

   在这种稳定的几何冷却之上，代码还追踪一个滚动接受窗口：每次迭代都会递增 `window_trials`（若该移动被接受，则同时递增 `window_accepts`；被约束检查拒绝的移动也计入试验次数，但从不计入接受次数）。一旦 `window_trials` 达到 `adaptive_temp_window`（默认 `clamp(max_iterations / 40, 20, 100)`），就会计算该窗口的接受率，并用它在常规冷却计划之外微调温度：

   - 若 `accept_rate < target_acceptance_low`（默认 0.20）：温度按 `adaptive_heat_factor`（默认 1.08）*升高*，上限为 `temp_ceiling = initial_temperature * adaptive_temp_ceiling_factor`（默认上限系数 5.0）——搜索已经"冻结"（接受率过低），因此升高温度使其重新松弛；
   - 若 `accept_rate > target_acceptance_high`（默认 0.45）：温度按 `adaptive_cool_factor`（默认 0.94）*降低*，下限为 `temp_floor`——搜索几乎接受一切（表现得像无引导的随机搜索），因此降低温度以加强收敛；
   - 否则本次检查温度保持不变（窗口无论如何都会重置）。

   该检查点在每一个拒绝分支以及主接受/拒绝分支中都会触发，因此 `window_trials`/`window_accepts` 会在约束拒绝、碰撞拒绝以及打分后的接受/拒绝结果中被一致地累积。简言之：自适应温度使接受率保持在一个目标区间内（`[target_acceptance_low, target_acceptance_high]`，默认为 `[0.20, 0.45]`），从而使搜索既不"冻结"（过冷、鲜少接受），也不"纯随机"（过热、几乎全部接受），且独立于基线的几何冷却计划。

8. **迁移**（仅岛屿模型）——见下文。

9. 循环在达到 `max_iterations` 后终止，或若温度降至 `1e-9` 以下则提前终止。

### 最佳解追踪

`best_particles` / `best_loss` / `best_s2` 只在一次*被接受*的移动的 `current_loss` 优于此前最佳时才更新。由于被接受的移动仍可能比当前状态更差（这正是 Metropolis 接受准则的意义所在），当前状态在运行的某些阶段可能会在最佳已知损失之上徘徊；最终返回并保存的是最佳快照的值，与随机游走最终落在何处无关。

## 岛屿模型

当 `optimization.islands`（`OptimizationParams::islands`）大于 1 时，流水线会在同一个已安装的 Rayon 池中分批并行运行独立的 `run_sa_island` 实例，而非单一搜索：

- 每个岛屿都获得初始（剪枝后）颗粒集合各自的克隆、各自的 `RotationMode`，以及——关键的一点——**各自的温度、冷却计划与自适应接受窗口状态**。这些状态从不在岛屿之间共享；`AGENTS.md` 明确将"跨线程共享可变 SA 状态"列为应避免的陷阱。若共享该状态，将会使各岛屿的搜索轨迹产生关联，从而破坏运行多个独立搜索的初衷。
- `run_island_batches` 将每批活跃岛数限制在共享池的 worker 数以内；岛任务和嵌套 S2/VF 工作共用预算，多余岛排队。批次边界防止嵌套任务窃取激活全部岛。
- 若选择兼容的 GPU MC，所有阶段和岛共用一个持久 `GpuS2Pipeline`。互斥锁串行保护上传和计算回读，CPU VF 工作前释放该锁。
- 唯一共享的状态是 `global_best: Arc<Mutex<Arc<GlobalBest>>>`，其中保存着*所有*岛屿目前为止见过的最低损失及其对应颗粒集合。在 `run_sa_island` 内部，每隔 `migration_interval` 次迭代（默认 100，由 `(iter + 1) % migration_interval == 0` 触发），每个岛屿都会锁定该互斥量，并执行以下两种操作之一：
  - 若自身的 `best_loss` 优于全局最佳，则**发布**对应同一几何的最佳几何/loss/S2 快照到 `global_best`；
  - 否则，若全局最佳优于自身，则**拉取**全局最佳解——同时替换自身的 `best_particles`/`best_loss`/`best_s2`，重建自身准备好的颗粒、`SpatialGrid`、合并网格，并从迁移后的状态重新计算 `current_s2`/`current_loss`——因此该岛屿的*当前*游走会从更优的解继续，而不仅仅是更新其记录。

  这是每个岛屿各自独立执行的一种单向"取我方与全局中较优者"的交换方式，而非广播；各岛屿会朝着当前持有全局最佳解的那个岛屿收敛，但每个岛屿始终保持自己的温度/接受率轨迹（迁移替换的是*颗粒构型*，而非驱动后续探索的 SA 状态）。

并行运行多个岛屿意味着同时探索解空间的若干独立区域（不同的随机移动序列、到达不同的局部极小值），而非只探索一个区域；周期性迁移使任一岛屿找到的良好解能够传播给其他岛屿，而不强迫每个岛屿过早收敛到该岛屿自身的局部最优——那些游走到别处的岛屿会继续以自己的 SA 状态探索，仅仅是借用了*构型*作为更强的起点重新出发。

## 顶层编排：`OptimizePipeline::run`

1. **后端与线程设置。** 保留 `cpu_max` 的解析规则，创建一个 Rayon 池并安装整个运行，包括参考目标准备、VF 与 CPU 回退。加载输入后由 `OptimizeS2` 一次解析方法/后端，详见下方执行契约。

2. **加载颗粒。** 从 `input.stl_path`（文件或目录）加载 STL 文件，拆分为各个颗粒（`split_mesh_into_granules`），并预先过滤掉包围盒与优化盒完全不重叠的颗粒。

3. **确定目标 S2。** 既可以直接取自 `target.s2_array`（`type = "manual_array"`），也可以通过对加载的参考 STL 运行 `calculate_s2`（针对 `target.stl_bounding_box` 或其自身包围盒）计算得到（`type = "reference_stl"`）。

4. **基线日志记录。** 在任何剪枝或 SA 运行之前，计算并记录输入装配体相对目标的 S2、VF 与损失，用于 `s2_history.txt` 追踪记录。

5. **剪枝。** 对已加载的颗粒集合调用 `selective_prune_to_target_vf`（见上文）。

6. **单岛屿与多岛屿调度。** 准备碰撞结构，读取 `islands`（默认 1）和 `migration_interval`（默认 100）。单岛直接消耗准备向量；多岛通过 `run_island_batches` 分批使用共享求值器及全局最佳。仅在有界任务真正开始时复制初始几何，按岛编号收集历史，以最小历史 loss 选择最佳结果。不再逐岛创建线程、线程池或设备。

7. **输出。** 在合并及可选定向之后，以相同方法及完整采样预算（至少 2000）复核最佳几何，分别记录 `Selected Search Loss` 与最终曲线/loss；MC 噪声可能使两者不同。 将获胜岛屿的最佳颗粒合并为一个网格，可选地将不相连的组件重新定向为正的带符号体积（`orient_to_positive_volume`），将结果以 STL 格式写入 `output.path`，并在其旁边写入累积的 `s2_history.txt` 日志（目标/输入/剪枝后/每次改进/最终的 S2 序列与损失）。最后打印一份耗时汇总，将总耗时分解为 S2 计算耗时与碰撞/约束检查耗时，累积自产生返回结果的那个岛屿（或单次运行）。

## 交叉引用

- [pipeline-optimize.md](../reference/pipeline-optimize.md) —— 该流水线的逐函数参考文档：[`run_sa_island`](../reference/pipeline-optimize.md#run_sa_island)、
  [`selective_prune_to_target_vf`](../reference/pipeline-optimize.md#selective_prune_to_target_vf)、
  [`OptimizePipeline::run`](../reference/pipeline-optimize.md#optimizepipelinerun)。
- [spatial-grid-collision.md](spatial-grid-collision.md) —— `run_sa_island` 在移动校验期间所使用的 `SpatialGrid` 粗筛结构与周期性镜像碰撞检查。
- [s2-two-point-correlation.md](s2-two-point-correlation.md) —— `calculate_s2` 与 GPU S2 流水线如何计算 SA 损失函数（`l2_norm`）用于与目标比较的相关曲线。
</content>

## 固定 S2 执行与配置（PERF-01/02）

`src/pipeline/optimize_execution.rs` 一次解析定义：exact 为 `voxel_exact`，非正 pitch 规范为 1.0；
非 exact 且 pitch 为正时为 `voxel_mc`，否则为 `mesh_mc`。目标、输入、剪枝、初始化、候选、迁移、最终复核
全部使用同一定义及 VF 约定。GPU MC shader 不执行体素采样，因此体素方法保留 CPU 求值器并记录原因，禁止回退则报错。

在 optimize 中，`RUSTMSPT_ACCELERATION=cpu|gpu|auto` 一次覆盖 YAML，非法值报错。
CPU 和低于阈值的 auto 不探测或初始化 GPU；小任务 auto 是正常策略选择，即使禁止回退仍可选择 CPU。
真正尝试 GPU 时，不兼容方法、设备不可用或初始化失败遵循 `cpu_fallback`。日志与历史记录实际方法、后端和有效 pitch。

连续 GPU MC 复用一个持久实例，目前支持 wgpu/f32 和 `gpu_prefer_power: false`。
工作集预算实现前，显式内存上限保守选择 CPU 或按配置报错；不兼容选项及半径/dispatch 容量在设备探测前检查。
GPU 运行时错误、射线交点溢出和 f32 数值认证仍为 PERF-04/05 待办；其他管线的 GPU 路由不受此改动影响。

迁移同时携带几何、loss 和曲线。全局迁移锁在几何/S2 工作前释放，GPU 锁在 CPU VF 工作前释放，
防止嵌套 Rayon 任务等待其暂停调用方持有的锁。分批调度会改变随机执行交错；optimize 仍不承诺跨线程 SA 逐字节一致。

GPU MC 运行时错误现遵守 `cpu_fallback`：true 停用失败实例并以 CPU 重算同一 mesh-MC 方法；false 在最终输出写入前返回带阶段的错误。见 `PLAN.Performance.md` §14。

### Persistent merged geometry (PERF-10)

`merge_prepared_particles` directly assembles prepared meshes and records each particle's vertex range without temporary particle clones. Rigid candidates overwrite only their range; rejection restores its original vertices and prepared collider. Faces and other vertex ranges stay resident. Population migration recreates the merged mesh/ranges. GPU triangle uploads and voxel occupancy remain full updates; geometric VF now uses the island-local cache described below.

### Optimize MC 显存预算

优化器的共享 GPU MC 管线以空几何启动，各阶段上传实际求值网格。mc_evaluation_peak 保守计算 triangle、四个 output/readback、576 字节参数缓冲、待执行队列上传以及本次几何/参数上传；增长时计入旧容量加新容量。启动按输入面数和最大配置阶段样本数检查，每次求值在同一 GPU mutex 内重新检查实际保留容量后才上传。因此更大的参考网格或保留峰值也可能触发原有同方法 CPU 回退或严格阶段错误。待上传字节仅在成功回读后清零。驱动内部及 CPU 网格/读回向量不属于逻辑 GPU 预算；分批和自动缩容仍待完成，release_output_capacity 提供显式释放。本节取代早先“显式预算始终回退”的说明。

### 不可变迁移快照

每个岛用 Arc<GlobalBest> 同时保存最佳几何/loss/S2，IslandResult 保留该 Arc 和计时。改进时在迁移锁外构造新快照，共享槽为 Arc<Mutex<Arc<GlobalBest>>>。exchange_best_snapshot 在锁内仅严格比较 loss 和交换/克隆 Arc 引用；退役快照在解锁后释放，平局保留当前状态。接收岛在锁外重建可变 prepared/grid 并复算当前游走，历史最佳曲线仍对应原几何/loss。最终选优借用获胜快照，不复制 payload。此改动消除迁移 payload 拷贝；创建新本地最佳和准备接收后的可变游走仍复制几何。

mesh-MC 岛现以 IslandVolumes 缓存每粒子连通分量的域内体积。可行候选只替换对应粒子条目，拒绝恢复旧条目；迁移重建，每 64 次候选求值全量刷新。VF 仍按合并网格的源顺序累加全部缓存标量并使用原分母/截断规则，避免运行总量反复加减导致漂移。缓存仅用于连续 mesh MC，voxel 方法保留体素 VF。CPU 固定种子采样和 GPU 错误回退均可使用已验证 VF，不重复裁剪全体；GPU 锁边界不变。完全域内粒子的变换也重新计算贡献以保持浮点参考行为，未假定刚体体积位级不变。预剪枝和最终验证仍使用全量参考求值。

**GPU 局部上传（PERF-10）。** GPU MC 三角形上传现在将新的合并网格与驻留缓冲的主机影子副本逐位比较，只写入发生变化的面区间（移动一个粒子即只上传该粒子的面）；三角形数量变化、缓冲扩容、区间超过 64 段或变化面超过一半时回退为整体写入；`GpuS2Pipeline::upload_stats` 记录字节数。体素占据仍为整体更新。
