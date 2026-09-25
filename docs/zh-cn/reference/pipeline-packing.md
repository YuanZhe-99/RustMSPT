# 流水线：堆积（Packing）

顺序随机堆积流水线（`src/pipeline/pack.rs`）及其目标粒径分布 / 平均球形度引导引擎
（`src/pipeline/pack_targets.rs`）。

## 索引

| 条目 | 位置 | 摘要 |
|---|---|---|
| `TARGET_BIN_PROBES` | `src/pipeline/pack.rs:27` | 某个已选中区间在被排除出本轮重新选择之前，可容忍的最大连续放置失败次数。 |
| `PackPipeline` | `src/pipeline/pack.rs:95` | 包装 `PackingConfig` 的流水线结构体；实现 `Pipeline`。 |
| `CandidateProposal` | `src/pipeline/pack.rs:34` | 一个抽取的候选网格，及其可选的预计算 `MeshMetrics`。 |
| `validate_sphericity_target` | `src/pipeline/pack.rs:110` | 在堆积开始前验证 `target_mean_sphericity`/`mean_sphericity_tolerance` 配置。 |
| `check_geometry_filters` | `src/pipeline/pack.rs:141` | 对候选网格应用配置中的 `min_volume`、`max_aspect_ratio`、`max_sharpness_ratio` 过滤器。 |
| `PackPipeline::run` | `src/pipeline/pack.rs:124` | 核心的顺序随机堆积循环，可选带有目标粒径分布与平均球形度引导。 |
| `DiameterBin` | `src/pipeline/pack_targets.rs:8` | 半开（最后一个区间为闭区间）的粒径区间，带有目标频率。 |
| `DiameterBin::midpoint` | `src/pipeline/pack_targets.rs:16` | 该区间的算术中点。 |
| `TargetDistribution` | `src/pipeline/pack_targets.rs:22` | 解析并归一化后的目标粒径分布（有序、不重叠的区间）。 |
| `DistributionState` | `src/pipeline/pack_targets.rs:27` | 相对于某个 `TargetDistribution` 跟踪的、每次运行的可变计数器。 |
| `BinChoiceKind` | `src/pipeline/pack_targets.rs:40` | 区间选择的分类：`Natural` / `Scaled` / `Fallback`。 |
| `BinChoice` | `src/pipeline/pack_targets.rs:47` | 所选区间的索引及其 `BinChoiceKind`。 |
| `DistributionSummary` | `src/pipeline/pack_targets.rs:53` | 在一次运行结束时计算的聚合误差指标。 |
| `SphericityState` | `src/pipeline/pack_targets.rs:61` | 已接受球形度值的滚动求和/计数。 |
| `TargetDistribution::state` | `src/pipeline/pack_targets.rs:68` | 构建一个按区间数量归零的 `DistributionState`。 |
| `TargetDistribution::bin_for_diameter` | `src/pipeline/pack_targets.rs:81` | 将某个粒径映射到其所属区间的索引。 |
| `TargetDistribution::choose_bin` | `src/pipeline/pack_targets.rs:98` | 核心分配启发式算法：选择欠账最多的区间，若无则回退到最小 TVD 选择。 |
| `TargetDistribution::record_attempt` | `src/pipeline/pack_targets.rs:165` | 为某个区间的尝试计数器加一。 |
| `TargetDistribution::record_success` | `src/pipeline/pack_targets.rs:176` | 提交一次成功放置的区间、类型及缩放系数。 |
| `TargetDistribution::summarize` | `src/pipeline/pack_targets.rs:212` | 计算最大绝对误差、全变差距离，以及理论最优可达的舍入误差。 |
| `SphericityState::mean` | `src/pipeline/pack_targets.rs:259` | 已接受球形度值的算术平均值。 |
| `SphericityState::projected_error` | `src/pipeline/pack_targets.rs:272` | 若接受该候选，预计均值与目标区间的距离；用于对候选进行排序/引导。 |
| `SphericityState::record_success` | `src/pipeline/pack_targets.rs:298` | 将某候选的球形度提交进滚动平均值。 |
| `load_target_distribution_csv` | `src/pipeline/pack_targets.rs:309` | 解析并验证 `bin[,right],frequency` 格式的 CSV，生成一个 `TargetDistribution`。 |
| `write_distribution_comparison_csv` | `src/pipeline/pack_targets.rs:495` | 写出 `<output_stem>_diameter_distribution.csv` 目标与实际对比报告。 |
| `parse_csv_f64` | `src/pipeline/pack_targets.rs:564` | 解析一个必填的有限浮点 CSV 单元格，错误信息含行/列上下文。 |
| `parse_optional_csv_f64` | `src/pipeline/pack_targets.rs:587` | 解析一个可选的浮点 CSV 单元格，空白表示"缺省"。 |
| `PackCollider` | `src/pipeline/pack.rs:31` | 缓存的碰撞体 bbox 与形状。 |
| `PackCollider::new` | `src/pipeline/pack.rs:38` | 只准备一次碰撞形状。 |
| `PackCollider::blocks` | `src/pipeline/pack.rs:46` | 缓存的重叠或间隙判定。 |
| `bbox_may_block` | `src/pipeline/pack.rs:72` | 精确判定自身的 bbox 拒绝（可选 bbox）。 |
| `periodic_image_shifts` | `src/pipeline/pack.rs:83` | 按 `generate_periodic_ghosts` 顺序给出周期平移及平移后 bbox。 |
| `PackImage` | `src/pipeline/pack.rs:117` | 已接受颗粒或 `(particle_id, shift)` 镜像，碰撞体按需构建。 |
| `PackScene` | `src/pipeline/pack.rs:124` | 镜像存储、增量网格、无 bbox 列表与 ghost 构建计数。 |
| `PackScene::new` | `src/pipeline/pack.rs:133` | 创建使用 domain/8 网格的空存储。 |
| `PackScene::build_ghost` | `src/pipeline/pack.rs:144` | 平移并准备一个镜像（计数）。 |
| `PackScene::collider` | `src/pipeline/pack.rs:152` | 线程安全的惰性镜像碰撞体。 |
| `PackScene::reachable` | `src/pipeline/pack.rs:161` | bbox 在 gap 下可能阻挡查询的镜像。 |
| `PackScene::blocks_any` | `src/pipeline/pack.rs:179` | 对可达镜像串行/并行 any()。 |
| `PackScene::candidate_images` | `src/pipeline/pack.rs:197` | 完整旧版可行性判定，候选及已接受镜像均惰性实例化。 |
| `PackScene::insert` | `src/pipeline/pack.rs:228` | 记录已接受颗粒及其镜像描述。 |
| `PackScene::image_stats` | `src/pipeline/pack.rs:252` | 存储镜像数、已实例化 ghost 数、ghost 构建数。 |
| `PackPipeline::run_in_pool` | `src/pipeline/pack.rs:394` | Packing work under configured pool. |

**另请参阅：** 关于本流水线中大量使用的 `MeshMetrics` 与 `scale_mesh_to_equivalent_diameter`，见
[geometry-analysis.md](geometry-analysis.md)。

---

## `src/pipeline/pack.rs`

#### TARGET_BIN_PROBES

- **签名：** `const TARGET_BIN_PROBES: usize = 4`
- **源码位置：** `src/pipeline/pack.rs:27`
- **用途：** 限制单个被选中的目标区间在一轮堆积中可容忍的最大连续放置失败次数，一旦超过，
  `choose_bin` 会将其排除出本轮的后续考虑。
- **说明：** 在 `PackPipeline::run` 的 `reject_candidate!()` 宏内部被使用：某区间的失败计数器
  在每次拒绝时递增，只有当计数达到 `TARGET_BIN_PROBES` 时该区间才会保留在 `attempted_bins`
  中（在本轮内被永久排除）；低于该阈值时它会从 `attempted_bins` 中移除，以便可以重新尝试。

#### PackPipeline

- **种类：** 结构体
- **源码位置：** `src/pipeline/pack.rs:95`
- **字段：**

| 字段 | 类型 | 描述 |
|---|---|---|
| `config` | `PackingConfig` | 完整的堆积配置（输入/输出/箱体/堆积设置、过滤器、目标分布、球形度目标）。 |

- **用途：** 为 `pack` 命令实现 `Pipeline` trait；`run` 即整个堆积算法。

#### CandidateProposal

- **种类：** 结构体（私有，`Debug, Clone`）
- **源码位置：** `src/pipeline/pack.rs:34`
- **字段：**

| 字段 | 类型 | 描述 |
|---|---|---|
| `mesh` | `Mesh` | 抽取出的候选网格，尚未经过任何目标区间重缩放、旋转或放置。 |
| `metrics` | `Option<MeshMetrics>` | 预计算的度量值（体积、表面积、等体积直径、球形度），仅当目标分布或球形度目标处于激活状态时才存在。 |

- **用途：** 将一个候选网格与其度量值打包在一起，使每次尝试的多个抽取结果可以在进行任何
  代价高昂的放置/碰撞计算之前先进行排序。

#### validate_sphericity_target

- **签名：** `fn validate_sphericity_target(config: &PackingConfig) -> Result<Option<(f64, Option<f64>)>>`
- **源码位置：** `src/pipeline/pack.rs:110`
- **用途：** 在任何堆积工作开始之前，验证可选的 `packing.target_mean_sphericity` /
  `packing.mean_sphericity_tolerance` 配置。
- **参数：** `config` —— 堆积配置。
- **返回值：** 若已配置目标，则为 `Ok(Some((target, tolerance)))`；若两个字段均未设置，则为
  `Ok(None)`；若 `target_mean_sphericity` 超出 `(0, 1]` 范围、`mean_sphericity_tolerance` 超出
  `[0, 1]` 范围，或给定了容差但未给定目标，则为 `Err(InvalidConfig)`。
- **副作用：** 无。

#### check_geometry_filters

- **签名：** `fn check_geometry_filters(mesh: &Mesh, config: &PackingConfig, check_min_volume: bool) -> bool`
- **源码位置：** `src/pipeline/pack.rs:141`
- **用途：** 依据配置中的 `packing.filters`（`min_volume`、`max_aspect_ratio`、
  `max_sharpness_ratio`）验证候选网格。
- **参数：**
  - `mesh` —— 待检查的候选网格。
  - `config` —— 持有可选 `filters` 配置块的堆积配置。
  - `check_min_volume` —— 本次调用是否强制执行 `min_volume` 过滤器；当目标粒径分布处于激活
    状态时，调用方会在*源*网格上跳过该检查（因为该网格在重缩放前的最终体积没有实际意义），
    并在候选网格达到最终尺度后重新强制执行（`true`）。
- **返回值：** 若所有启用的过滤器都通过（或未配置任何过滤器），返回 `true`；在第一个未通过
  的过滤器处返回 `false`。
- **副作用：** 无。
- **说明：** 长宽比是网格包围盒的 `max_extent / min_extent`（`min_extent` 下限设为 `1e-12`）。
  锐度使用等周不等式风格的量 `log_sharpness = 3*ln(area) - ln(36*pi) - 2*ln(volume)`，与
  `ln(max_sharpness_ratio)` 比较；非有限的体积/面积/log_sharpness 或非正输入会使该过滤器判为
  未通过。

#### PackPipeline::run

- **签名：** `fn run(&self) -> Result<()>`（`Pipeline for PackPipeline` 的实现）
- **源码位置：** `src/pipeline/pack.rs:124`
- **用途：** 执行完整的顺序随机堆积算法：抽取候选颗粒，可选地朝配置的目标引导其粒径分布与
  平均球形度，在堆积箱体内无碰撞地放置它们，直至达到目标体积分数或用尽尝试预算，然后保存
  结果并报告统计信息。
- **参数：** 除 `&self` 外无其他参数（使用 `self.config`）。
- **返回值：** 成功时为 `Ok(())`；箱体体积非正、球形度目标配置无效或线程池构建失败时为
  `Err(InvalidConfig)`；未找到候选网格/STL，或根本无法放置任何颗粒时为 `Err(InvalidMesh)`。
- **副作用：** 从磁盘读取输入 STL 文件或目录，以及可选的目标粒径 CSV；构建一个 rayon 线程池；
  将堆积结果 STL 写入 `config.output.path`；可选地写出
  `<output_stem>_diameter_distribution.csv`；向 stdout 打印 `[Info]`/`[Warning]` 进度与摘要
  信息；通过 `create_progress_bar` 渲染进度条。

**流程走读：**

1. **初始化与验证。** 解析 `box.dimensions` 得到 `box_bounds`，拒绝非正体积。调用
   `validate_sphericity_target`。若设置了 `packing.target_diameter_distribution_csv`，通过
   `load_target_distribution_csv` 加载并打印区间数量；否则 `target_distribution` 为 `None`。
2. **输入加载 —— 两种模式：**
   - **惰性目录模式**（`input.path` 为目录）：将每个 `.stl` 文件路径索引进 `lazy_files`，而
     不加载网格数据；每次堆积尝试通过 `load_stl` 按需加载一个文件。适用于无法一次性载入内存
     的大型颗粒库。
   - **预加载单 STL 模式**（`input.path` 为文件）：一次性加载该 STL，通过
     `split_mesh_into_granules` 将其拆分为若干颗粒（granule），并将每个通过
     `check_geometry_filters`（`check_min_volume = target_distribution.is_none()`）检验的颗粒
     保留在 `preloaded_pool` 中。
   若解析出的模式产生零个候选/文件，则报错。
3. **线程池。** 根据 `packing.cpu_max` 构建一个专用的 rayon `ThreadPool`（`-1` 表示使用全部
   可用核心，否则被限制在 `[1, available_cores]` 范围内）；该线程池（通过
   `thread_pool.install(...)`）用于所有并行的碰撞/距离检查，使其独立于全局 rayon 线程池。
4. **主循环** —— `while (current_volume / box_volume) < target && attempts < max_attempts`：
   - **候选抽取。** 正常情况下抽取 1 个候选，若球形度目标处于激活状态则抽取 4 个
     （`proposal_draws`），来源为惰性文件列表或预加载池。当需要目标度量值（目标分布或球形度
     目标处于激活状态）时，为每个抽取结果计算 `mesh_metrics`，并跳过度量计算失败的抽取。当
     球形度目标处于激活状态时，每个抽取结果的排序键是 `SphericityState::projected_error`
     （会把预计的平均球形度推到目标区间之外/更远的候选排序更靠后）；否则排序键为 `0.0`
     （保持原顺序）。候选按此误差升序排序，使最有利的候选被优先尝试。
   - **放置尝试循环**（内层 `loop`），针对每个轮转中的候选（`proposal_index %
     proposals.len()`，或当目标分布处于激活状态时，持续循环遍历各候选，直至某个区间选择耗尽
     或达到 `max_attempts`）：
     - 若目标分布处于激活状态：根据候选网格抽取时的等体积直径（`bin_for_diameter`）计算其
       *天然*区间，然后调用 `TargetDistribution::choose_bin` 选出本次放置要*瞄准*的区间
       （可能与天然区间不同——见下文 `choose_bin`）。`record_attempt` 会被立即调用。若所选
       区间的类型不是 `Natural`，则通过 `scale_mesh_to_equivalent_diameter` 将网格重缩放到该
       区间的中点，重新测量，若重缩放后的粒径不再落回所选区间内，则拒绝该候选（防止边界附近
       的重缩放失败）。当目标分布处于激活状态时，会在（可能已重缩放的）候选上以
       `check_min_volume = true` 重新运行 `check_geometry_filters`。
     - 应用一次随机旋转（通过 `sample_rotation_axis`/`rotate_mesh_around_center`，遵循
       `rotation_mode`）以及在 `box_bounds` 内的一个均匀随机位置
       （`move_mesh_to_target_center`）。
     - 拒绝未通过 `check_boundary_constraints_mode` 的候选（依模式而定的边界规则、
       `min_boundary_dist`、`min_cross_boundary_depth`）。
     - 构建 `collision_set` = 全部已放置网格，当 `packing.mode == 3`（周期边界模式）时，再
       扩展每个已放置网格的 `generate_periodic_ghosts`。运行一次并行（`thread_pool.install`）
       的包围盒剪枝精确碰撞检查（`mesh_collision_exact`，自 0.2.1 起同时拒绝整体位于已放置颗粒内部、或会把已放置颗粒吞入的候选——该布局下两个闭合表面从不相交，因此直到 v0.2.0 本循环都会接受它们，而 `min_neighbor_distance` 会把两个表面之间的空间读作间隙；参见 [spatial-grid-collision.md](../algorithms/spatial-grid-collision.md) 的"为何嵌套需要独立的检测"一节）；当设置了 `min_neighbor_distance`
       时，包围盒距离剪枝改用该阈值而非普通重叠判断。
     - 若 `min_neighbor_distance > 0`，运行第二次并行遍历，计算与任何已存在网格的最小精确
       表面距离（`mesh_distance_exact`），若低于该阈值则拒绝。
     - 在模式 3 下，还会额外检查候选自身的周期镜像与已放置集合及邻域距离阈值。
     - 计算 `particle_volume_in_bbox`（箱内裁剪体积），并拒绝非有限/非正结果。
     - **成功时：** 在适用情况下记录区间选择（`record_success`，包含缩放系数（若已重缩放）
       ）和球形度（`SphericityState::record_success`），将裁剪后的体积累加到
       `current_volume`，将网格压入 `placed`，将 `attempts` 重置为 0，然后跳出以继续外层
       循环。
     - **任意拒绝时：** `reject_candidate!()` 宏会递增 `attempts`，更新进度条，并（若已选择
       了区间）递增该区间的失败计数器；仅当该计数器仍低于 `TARGET_BIN_PROBES` 时才将其从
       `attempted_bins` 中移除（因此一个区间在本轮被放弃之前会得到若干次尝试机会）。
   - 内层循环结束后：若没有放置成功，且目标分布处于激活状态并已作出区间选择
     （`had_bin_choice`），且 `attempts` 达到 `max_attempts`，外层循环将完全跳出（堆积停止）。
     若目标分布处于激活状态但 `choose_bin` 立即返回了 `None`（`!had_bin_choice`，即根本无法
     选出任何区间），`attempts` 只递增一次，循环继续（允许重新抽取新候选）。
5. **收尾。** 若 `placed` 为空则报错。合并所有已放置的网格（`merge_meshes`），在设置了
   `packing.orient_to_positive_volume` 时可选地应用
   `orient_components_to_positive_volume`，并通过 `save_stl` 保存结果。打印最终颗粒数与体积
   分数，若未达到目标体积分数则给出警告。若使用了目标分布，调用 `summarize`，通过
   `write_distribution_comparison_csv` 写出对比 CSV，并打印一张完整的逐区间表格（左边界、右
   边界、目标频率、理想计数、实际计数、实际频率、误差、尝试次数），以及聚合误差、
   natural/scaled/fallback 计数与缩放系数的最小/平均/最大值；若发生任何 fallback 放置（分布
   被放宽以优先保证体积分数），则给出警告。若使用了球形度目标，打印最终平均球形度及误差，
   以及是否满足容差（未满足时给出警告）。始终打印朝向修正是否启用，若启用则打印被翻转的构件
   数量。

- **说明：** 支持惰性目录加载，适用于无法预加载的超大数据集。目标区间失败时总是回退到任意
  仍可放置的区间，因此整体体积分数优先于精确的分布保真度。模式 3（周期边界）在两侧都会新增
  镜像颗粒碰撞检查（已放置颗粒的镜像与候选之间，以及候选的镜像与已放置颗粒之间）。全部碰撞/
  距离计算都在流水线专用的、根据 `cpu_max` 配置尺寸的 rayon 线程池上运行。

---

## `src/pipeline/pack_targets.rs`

> **算法：** 关于区间欠账分配启发式算法及 CSV 分布格式的完整走读（含实例），见
> `../algorithms/packing-target-diameter-distribution.md`。

> **另请参阅：** [geometry-analysis.md](geometry-analysis.md) 记录了本文件中大量使用的
> `MeshMetrics`（`volume`/`surface_area`/`equivalent_diameter`/`sphericity` 结构体）以及
> `scale_mesh_to_equivalent_diameter`，`PackPipeline::run` 调用后者将候选网格重缩放至所选区间
> 的中点。

### 类型

#### DiameterBin

- **种类：** 结构体（`Debug, Clone, Copy, PartialEq`）
- **源码位置：** `src/pipeline/pack_targets.rs:8`
- **字段：**

| 字段 | 类型 | 描述 |
|---|---|---|
| `left` | `f64` | 粒径区间的闭下界。 |
| `right` | `f64` | 上界；除 `TargetDistribution` 中的最后一个区间为闭区间外，其余均为开区间。 |
| `frequency` | `f64` | 该区间应占放置结果的目标比例，已归一化使所有区间之和为 1.0。 |

#### DiameterBin::midpoint

- **签名：** `pub fn midpoint(&self) -> f64`
- **源码位置：** `src/pipeline/pack_targets.rs:16`
- **用途：** 返回 `left + (right - left) * 0.5`，即某候选被分配到该区间时用作重缩放目标的
  粒径值。
- **副作用：** 无。

#### TargetDistribution

- **种类：** 结构体（`Debug, Clone`）
- **源码位置：** `src/pipeline/pack_targets.rs:22`
- **字段：**

| 字段 | 类型 | 描述 |
|---|---|---|
| `bins` | `Vec<DiameterBin>` | 严格有序、不重叠的区间，已归一化使各频率之和为 1.0。由 `load_target_distribution_csv` 生成。 |

#### DistributionState

- **种类：** 结构体（`Debug, Clone, Default`）
- **源码位置：** `src/pipeline/pack_targets.rs:27`
- **字段：**

| 字段 | 类型 | 描述 |
|---|---|---|
| `counts` | `Vec<usize>` | 每个区间已接受的放置计数，索引方式与 `TargetDistribution::bins` 一致。 |
| `attempts` | `Vec<usize>` | 每个区间记录的总尝试次数（无论成功与否）。 |
| `natural` | `usize` | 类型为 `Natural` 的成功次数（候选抽取时的粒径已天然匹配所选区间）。 |
| `scaled` | `usize` | 类型为 `Scaled` 的成功次数（候选在本轮的首次遍历中被重缩放到欠账驱动的区间）。 |
| `fallback` | `usize` | 类型为 `Fallback` 的成功次数（在本轮内至少有一个更早的区间选择放置失败后的重试遍历中被选中，或通过最小 TVD 平局判定选出）。 |
| `scale_factor_min` / `scale_factor_max` / `scale_factor_sum` / `scale_factor_count` | `f64`/`f64`/`f64`/`usize` | 在所有成功的重缩放放置中，应用的有限正缩放系数的滚动最小值、最大值、总和与计数（用于报告平均/最小/最大缩放系数）。 |

#### BinChoiceKind

- **种类：** 枚举（`Debug, Clone, Copy, PartialEq, Eq`）
- **源码位置：** `src/pipeline/pack_targets.rs:40`
- **变体：**
  - `Natural` —— 欠账驱动的区间选择与候选自身抽取时的粒径区间恰好一致；不应用重缩放。
  - `Scaled` —— 欠账驱动的区间选择与天然区间不同（或不存在天然区间），且发生在首次
    （非放宽的）选择遍历中；候选被重缩放到该区间的中点。
  - `Fallback` —— 该选择发生在 `attempted` 中已包含本轮至少一个先前失败区间之后（放宽选择），
    或通过在当前没有区间存在正欠账时使用的最小全变差距离平局判定选出。

#### BinChoice

- **种类：** 结构体（`Debug, Clone, Copy, PartialEq`）
- **源码位置：** `src/pipeline/pack_targets.rs:47`
- **字段：** `index: usize`（所选区间），`kind: BinChoiceKind`。

#### DistributionSummary

- **种类：** 结构体（`Debug, Clone, Copy, PartialEq, Default`）
- **源码位置：** `src/pipeline/pack_targets.rs:53`
- **字段：**

| 字段 | 类型 | 描述 |
|---|---|---|
| `count` | `usize` | 所有区间上已接受放置的总数。 |
| `max_absolute_error` | `f64` | 各区间 `\|observed_frequency - target_frequency\|` 中的最大值。 |
| `total_variation_distance` | `f64` | 对所有区间求 `0.5 * sum(\|observed_frequency - target_frequency\|)`——观测分布与目标分布之间的标准全变差距离（TVD）。 |
| `rounding_max_absolute_error` | `f64` | 给定 `count` 为整数时理论最优可达的最大绝对误差，通过对目标频率进行最大剩余（Hamilton）舍入法计算得出——对于该 `count`，实际放置误差永远无法超越这一下限。 |

#### SphericityState

- **种类：** 结构体（`Debug, Clone, Default`）
- **源码位置：** `src/pipeline/pack_targets.rs:61`
- **字段：** `sum: f64`（已接受球形度值之和），`count: usize`（已接受值的数量）。

### impl TargetDistribution

#### TargetDistribution::state

- **签名：** `pub fn state(&self) -> DistributionState`
- **源码位置：** `src/pipeline/pack_targets.rs:68`
- **用途：** 构建一个全新的 `DistributionState`，其 `counts` 和 `attempts` 归零并按
  `self.bins.len()` 调整大小；其余字段使用 `Default`。
- **副作用：** 无。

#### TargetDistribution::bin_for_diameter

- **签名：** `pub fn bin_for_diameter(&self, diameter: f64) -> Option<usize>`
- **源码位置：** `src/pipeline/pack_targets.rs:81`
- **用途：** 查找包含给定等体积直径的区间索引。
- **参数：** `diameter` —— 候选的等体积直径。
- **返回值：** 若 `diameter` 非有限，或落在所有区间之外（包括超出最后一个区间的右边界，或落
  在不相邻区间之间的显式空隙中），返回 `None`；否则返回匹配的区间索引。
- **副作用：** 无。
- **说明：** 区间为 `[left, right)`，唯有最后一个区间为闭区间（`[left, right]`），这一点由
  `tests/pack_target_tests.rs::classifies_shared_edges_and_final_right` 确认（恰好等于最后一
  个区间 `right` 的粒径映射到该区间；再向后一个 ULP 则映射到 `None`）。

#### TargetDistribution::choose_bin

- **签名：** `pub fn choose_bin(&self, state: &DistributionState, natural_bin: Option<usize>, attempted: &mut BTreeSet<usize>) -> Option<BinChoice>`
- **源码位置：** `src/pipeline/pack_targets.rs:98`
- **用途：** 选择下一个候选放置应被引导到的区间，优先选取相对目标频率“欠账”最多的区间，
  一旦没有区间存在正欠账，则回退到最小全变差距离的选择。
- **参数：**
  - `state` —— 当前的 `DistributionState`（只读；本次调用不会修改它）。
  - `natural_bin` —— 若不重缩放，候选会落入的区间，仅用于将本次选择分类为 `Natural` 还是
    `Scaled`。
  - `attempted` —— 本轮中已尝试（并放置失败）的区间索引可变集合；欠账搜索中会跳过此集合内
    的区间，且所选区间的索引会在返回前被插入其中。
- **返回值：** 若选出了区间，返回 `Some(BinChoice)`，给出所选区间及其类型；若所有区间的频率
  均为零/负，或已全部处于 `attempted` 中，则返回 `None`。
- **副作用：** 不会修改 `self`/`state`；会修改 `attempted`（插入所选索引），使调用方的重试
  循环在本轮不会再次选中同一区间。
- **说明：**
  - **欠账公式。** 对于每个尚未在 `attempted` 中、且 `frequency > 0` 的区间，计算
    `debt = frequency * next_count - counts[index]`，其中 `next_count = sum(counts) + 1` 是
    *若本次放置成功*时的未来总接受计数。这是该区间在该未来总数下的理想计数，减去其当前观测
    到的计数——即该区间目前落后于目标份额多少。选取欠账严格大于 `1e-12` 且最大的区间
    （平局按遍历顺序打破，即索引最小者优先，因为 `>` 要求严格改进）。
  - **Natural / Scaled / Fallback 分类**（首次遍历，调用开始时 `attempted` 为空）：若找到一个
    欠账为正的区间，当调用开始时 `attempted` 已非空（意味着本轮已有至少一个更早的区间失败——
    一次*放宽*的重新选择）时，该选择被分类为 `Fallback`；否则若所选索引等于 `natural_bin`，
    分类为 `Natural`；否则分类为 `Scaled`。换句话说，`Natural`/`Scaled` 只会出现在本轮的
    *首次*选择尝试中；同一轮内的每次后续重试均为 `Fallback`，即使它最终落在了一个欠账为正的
    区间上（由 `tests/pack_target_tests.rs::attempted_deficit_bin_falls_back_to_other_bin`
    确认）。
  - **回退到最小 TVD。** 若没有区间存在正欠账（例如所有符合目标条件的区间均已饱和或已在
    `attempted` 中），该方法转而对每个剩余符合条件的区间求值：若该区间计数加一，*整体*分布
    将具有的全变差距离（`variation = sum over all bins of
    |observed_with_increment/accepted_count - target_frequency|`）。它选取使该预计 TVD 最小
    的区间，平局按中点更小者（即偏好更小的粒径）打破。此选择始终被分类为 `Fallback`。
  - 由 `tests/pack_target_tests.rs::strict_choice_follows_largest_deficit` 确认，该测试对一个
    3 区间（0.5/0.3/0.2）分布连续调用 10 次，断言精确的欠账驱动索引序列为
    `[0,1,2,0,0,1,0,2,1,0]`，全部分类为 `Scaled`（每次调用均是各自独立一轮的*首次*尝试，
    `attempted` 集合都是全新且为空的），最终得到 `counts == [5,3,2]`。

#### TargetDistribution::record_attempt

- **签名：** `pub fn record_attempt(&self, state: &mut DistributionState, index: usize)`
- **源码位置：** `src/pipeline/pack_targets.rs:165`
- **用途：** 若 `index` 在范围内，为给定区间的 `state.attempts[index]` 加一。
- **副作用：** 修改 `state.attempts`。

#### TargetDistribution::record_success

- **签名：** `pub fn record_success(&self, state: &mut DistributionState, index: usize, kind: BinChoiceKind, scale_factor: Option<f64>)`
- **源码位置：** `src/pipeline/pack_targets.rs:176`
- **用途：** 将一次成功放置提交进该区间的已接受计数，并更新聚合的类型/缩放系数统计。
- **参数：** `state`（被修改）；`index` —— 接收该次放置的区间；`kind` ——
  `Natural`/`Scaled`/`Fallback`，被统计进 `state` 中对应的字段；`scale_factor` —— 应用的网格
  重缩放系数（若有），当其有限且为正时被并入
  `scale_factor_min`/`max`/`sum`/`count`。
- **返回值：** 无（若 `index >= state.counts.len()` 则不执行任何操作）。
- **副作用：** 修改 `state.counts`、`state.natural`/`scaled`/`fallback` 之一，以及缩放系数
  聚合值。

#### TargetDistribution::summarize

- **签名：** `pub fn summarize(&self, state: &DistributionState) -> DistributionSummary`
- **源码位置：** `src/pipeline/pack_targets.rs:212`
- **用途：** 依据已接受的计数计算最终的分布保真度指标。
- **参数：** `state` —— 最终的 `DistributionState`。
- **返回值：** 若没有任何放置被接受，返回 `DistributionSummary::default()`（全为零）；否则返回
  一个已填充的摘要。
- **副作用：** 无。
- **说明：** `rounding_max_absolute_error` 通过**最大剩余分配法**计算：每个区间的理想整数目标
  为 `floor(frequency * count)`，随后将剩余的 `count - sum(floor(...))` 个单位，逐一分配给
  小数余数（`frequency * count - floor(frequency * count)`）最大的区间，平局按区间索引更小者
  打破。这为该确切的 `count` 给出了最优可能的逐区间整数分配，`rounding_max_absolute_error`
  即该分配相对目标频率的最大绝对误差——`max_absolute_error`（*实际*达到的误差）可以逼近但通常
  无法超越这一下限，因为要精确达到它，需要每次放置都落入其欠账最优的区间。
  `total_variation_distance` 采用标准的 `0.5 * sum(|observed - target|)` 归一化（取值范围
  `[0, 1]`）。由 `tests/pack_target_tests.rs::integer_rounding_baseline_preserves_total_count`
  与 `strict_choice_follows_largest_deficit`（其断言
  `max_absolute_error <= rounding_max_absolute_error + 1e-12`，即欠账驱动的启发式算法所达到的
  误差不超过理论舍入下限）确认。

### impl SphericityState

#### SphericityState::mean

- **签名：** `pub fn mean(&self) -> Option<f64>`
- **源码位置：** `src/pipeline/pack_targets.rs:259`
- **用途：** 返回所有已接受放置的球形度算术平均值；若尚未接受任何放置，返回 `None`。
- **副作用：** 无。

#### SphericityState::projected_error

- **签名：** `pub fn projected_error(&self, metrics: MeshMetrics, target: f64, tolerance: Option<f64>) -> Option<f64>`
- **源码位置：** `src/pipeline/pack_targets.rs:272`
- **用途：** 依据若该候选被接受，已接受的平均球形度*将*落在何处，相对于目标值或容差区间对
  候选进行评分——供 `PackPipeline::run` 用于对多个候选抽取结果排序，并优先选择能将滚动均值
  引导向目标的那一个。
- **参数：** `metrics` —— 候选的 `MeshMetrics`（只使用 `sphericity`）；`target` —— 目标平均
  球形度；`tolerance` —— 可选的绝对半宽，定义接受区间 `[target - tolerance, target +
  tolerance]`。
- **返回值：** 若 `target` 或 `metrics.sphericity` 非有限，或预计均值非有限，返回 `None`；
  否则，若预计均值落在容差区间内（或当 `tolerance` 为 `None` 时——因其默认值为 `0.0`——恰好
  等于 `target`），返回 `0.0`；否则返回与最近区间边界的绝对距离。
- **副作用：** 无（不修改 `self`；该预计值为
  `(self.sum + metrics.sphericity) / (self.count + 1)`，纯属假设）。
- **说明：** 由
  `tests/pack_target_tests.rs::sphericity_projection_selects_direction_that_repairs_mean`
  确认：给定一个低于目标的滚动均值，球形度更高的候选预计误差小于球形度更低的候选，因此按
  `projected_error` 升序对候选排序（正如 `PackPipeline::run` 所做的那样）会优先选择能将均值
  拉向目标的候选。

#### SphericityState::record_success

- **签名：** `pub fn record_success(&mut self, metrics: MeshMetrics)`
- **源码位置：** `src/pipeline/pack_targets.rs:298`
- **用途：** 将一次成功放置候选的球形度提交进滚动均值。
- **副作用：** 修改 `self.sum` 和 `self.count`。

### 自由函数

#### load_target_distribution_csv

- **签名：** `pub fn load_target_distribution_csv(path: &Path) -> Result<TargetDistribution>`
- **源码位置：** `src/pipeline/pack_targets.rs:309`
- **用途：** 解析并验证目标粒径分布 CSV，生成一个归一化的 `TargetDistribution`。
- **参数：** `path` —— CSV 文件路径。
- **返回值：** 一个 `TargetDistribution`，其区间严格递增、不重叠，频率已重新归一化使其之和
  恰好为 1.0。
- **副作用：** 从磁盘读取该文件。

> **算法：** CSV 格式规范及实例，见 `../algorithms/packing-target-diameter-distribution.md`。

- **CSV 格式：**
  - 表头行只能从 `{bin, right, frequency}` 中选择列（未知列、重复列，或缺少
    `bin`/`frequency` 均为错误）。`right` 为可选列。
  - 每行：`bin`（该区间的 `left`，必须有限、`>= 0`，且严格大于上一行的 `bin`）、可选的
    `right`（若存在，必须有限且 `> bin`），以及 `frequency`（有限、`>= 0`）。
  - **右边界推断**（当 `right` 为空/缺省时）：使用*下一行*的 `bin` 值作为本行的 `right`；对于
    最后一行，则使用上一区间的宽度进行外推（`lefts[index] + previous_width`）；单行分布若无
    显式 `right` 则为错误（无宽度可供推断）——由
    `tests/pack_target_tests.rs::rejects_uninferable_single_row` 确认。
  - 验证每个生成区间的中点有限且 `> 0`，并验证各区间不重叠
    （`lefts[index] >= previous.right`）。
  - **频率归一化：** 对所有行的频率求和；原始总和必须有限、`> 0`，且与 `1.0` 的差异在
    `1e-6` 以内，否则 CSV 被拒绝（`rejects_invalid_frequency_sum` 测试：总和为 `0.4` 时判为
    失败）。通过校验的总和随后会被整体除以其值，使各区间频率之和恰好为 `1.0`。必须至少有
    一个区间最终具有正频率。
- **说明：** 已针对
  `tests/pack_target_tests.rs::parses_explicit_and_inferred_bin_rights`、
  `parses_two_column_distribution`、`rejects_overlapping_explicit_intervals`、
  `finite_large_edges_produce_finite_midpoint`、
  `rejects_interval_with_zero_underflowed_midpoint`，以及
  `example_paper_distribution_is_valid_and_normalized`（验证随附的 25 区间
  `data/input/gu2019_fig7b_pore_distribution.csv` 样本文件，覆盖范围 `5..30`）确认。

#### write_distribution_comparison_csv

- **签名：** `pub fn write_distribution_comparison_csv(output_stl: &Path, distribution: &TargetDistribution, state: &DistributionState) -> Result<PathBuf>`
- **源码位置：** `src/pipeline/pack_targets.rs:495`
- **用途：** 在堆积结果 STL 输出旁边，写出一份逐区间的目标与实际粒径频率对比 CSV。
- **参数：** `output_stl` —— 堆积结果 STL 已（或将要）保存到的路径，用于推导同级 CSV 的
  名称/目录；`distribution` —— 目标分布；`state` —— 最终计数器。
- **返回值：** 所写出 CSV 的 `Path`，命名为 `<output_stem>_diameter_distribution.csv`，位于与
  `output_stl` 相同的目录中（若 `output_stl` 没有文件主干名，则回退为 `packed_result`）。
- **副作用：** 在磁盘上创建或覆盖该 CSV 文件。
- **说明：** 列为：`bin, right, target_frequency, target_count, actual_count,
  actual_frequency, frequency_error, count_error, attempts`，每个浮点数格式化为 12 位小数。
  `target_count = frequency * total_accepted_count`（一个实数，未取整）。由
  `tests/pack_target_tests.rs::writes_target_actual_comparison_csv_beside_output_stl` 确认。

#### parse_csv_f64

- **签名：** `fn parse_csv_f64(value: Option<&str>, column: &str, path: &Path, row: usize) -> Result<f64>`
- **源码位置：** `src/pipeline/pack_targets.rs:564`
- **用途：** 将一个必填的 CSV 单元格解析为 `f64`，若失败则给出描述性的 `InvalidConfig` 错误，
  其中包含文件路径、行号和列名。
- **返回值：** 若单元格缺失、去除空白后为空，或无法解析为浮点数，返回
  `Err(InvalidConfig)`；否则返回解析出的值（此处不检查有限性——调用方会另行验证）。
- **副作用：** 无。

#### parse_optional_csv_f64

- **签名：** `fn parse_optional_csv_f64(value: Option<&str>, column: &str, path: &Path, row: usize) -> Result<Option<f64>>`
- **源码位置：** `src/pipeline/pack_targets.rs:587`
- **用途：** 解析一个可选的 CSV 单元格（用于 `right` 列），其中缺失或空白值表示“未提供”而非
  错误。
- **返回值：** 若单元格缺失或为空白，返回 `Ok(None)`；否则委托给 `parse_csv_f64` 并用 `Some`
  包装。
- **副作用：** 无。

### Cached legacy packing feasibility (PERF-11)

`PackCollider::new(&Mesh)` retains bbox and optional parry TriMesh without another raw mesh copy. `blocks(other, gap)` keeps the old bbox rejection, uses cached solid collision at zero gap and cached solid distance below a positive gap. `PackScene::reachable` directly scans fewer than 32 stored images, otherwise queries an incremental grid plus every bbox-less image. Periodic images are lazy descriptors (see below). This replaces the previous `placed.clone()`, repeated ghost generation, shape construction and separate minimum-distance pass. The grid has at most eight cells on the longest domain axis. Void/nesting semantics and gap equality remain governed by the same prepared collision/distance functions. Periodic checks still include candidate ghosts against accepted real particles and ghosts. `PackPipeline::run` installs the worker pool around `run_in_pool`, including loading and finalization; proposal RNG remains sequential.

### 惰性周期镜像（PERF-11，2026-09-25）

模式 3 不再生成 ghost 副本。`PackScene` 对每个已接受颗粒只存一次，并对每个平移后 bbox 与域严格重叠的周期平移（与 `generate_periodic_ghosts` 相同的规则与顺序，经 `periodic_image_shifts`）存一个 `PackImage`，即 `(particle_id, shift, bbox)`；网格索引全部镜像。镜像碰撞体在首次使用时经 `OnceLock` 构建（在 rayon `any` 内线程安全），构建方式与原 `translate_mesh` 平移完全相同，精确判定看到的坐标与以前一致。`candidate_images` 先检查候选本体，然后仅当某已接受镜像的 bbox 可能阻挡时（`bbox_may_block`，即精确判定自身的 bbox 测试）才实例化该候选镜像；无法到达的镜像从不平移。舍入加法单调，故 `bbox + shift` 与平移后网格 bbox 逐位相同，剪枝不改变任何判定。检查中已构建的候选镜像在接受后复用。模式 3 输出 `Periodic images: stored, instantiated, ghost TriMesh builds`。`pack.rs` 中的对照测试将每个判定与原来的急切全 ghost 扫描比较，覆盖面/棱/角跨界、两个域（其一不在原点）、`-1` 与 `+1` 镜像同时与域重叠的近域宽颗粒、嵌套、接触及 gap 0/0.25/0.5；增量接受重放断言 ghost 构建数少于急切方式。
