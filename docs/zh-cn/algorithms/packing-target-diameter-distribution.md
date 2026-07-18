# 目标粒径分布堆积

标准的随机顺序堆积（`PackPipeline::run`）从用户提供的网格库中抽取候选颗粒，并接受第一个通过几何过滤器、边界/碰撞/距离检查以及周期性虚像检查（模式 3）的候选。因此最终被放置的颗粒的*尺寸*完全取决于输入库恰好产生了什么——对最终得到的等体积直径直方图没有任何控制手段。目标粒径分布这一功能补上了这一控制能力：它引导每个被接受的颗粒被分配到用户提供的直径直方图的哪个区间，将候选颗粒缩放至代表性不足的区间，使得在整个运行结束时，*已放置*颗粒的直径直方图逼近一个目标直方图——通常是从真实材料中数字化提取的孔隙或颗粒尺寸分布测量数据。项目自带的示例 `data/input/gu2019_fig7b_pore_distribution.csv` 就是这样一条数字化曲线（取自 Gu 等人 2019 年论文的 Figure 7b），包含 25 个宽度为一个单位、覆盖直径 5–30 的区间。

该功能的引擎位于 `src/pipeline/pack_targets.rs`；它所依赖的等体积直径/球形度度量位于 `src/geometry/metrics.rs`；将二者接入堆积循环的调用方是 `src/pipeline/pack.rs` 中的 `PackPipeline::run`。按照 `PLAN.md` 自身的表述，"accepted-count bin debt"（已接受计数的区间欠账）的存在正是为了确保"target VF has priority"（目标体积分数优先）——堆积的首要目标始终是达到 `target_volume_fraction`，而粒径分布匹配则是叠加在其上的次要的、尽力而为的目标。当某个目标区间在有限次尝试后仍无法被填充时，管线会退回到当前*任何*可以接受颗粒的区间，而不是为了追求完美的直方图匹配而阻塞体积分数的推进。

## CSV 格式与解析

`load_target_distribution_csv` 读取形如 `bin[,right],frequency` 的 CSV：`bin` 是直径区间的左边界，`right` 是其（可选的）右边界，`frequency` 是相对权重。以下是 `gu2019_fig7b_pore_distribution.csv` 中的几行真实数据：

```
bin,right,frequency
5,6,0.12328767123287673
6,7,0.12924359737939248
7,8,0.11673615247170934
```

解析规则如下：

- 列被限制为 `bin`、`right`、`frequency`；`bin` 和 `frequency` 是必需的，`right` 是可选的，每行可以留空。
- `bin` 的值必须逐行严格递增（区间按左边界顺序读取）。
- 缺失的 `right` 会从**下一行**的 `bin` 值推断得出（相邻两个区间共享一条边界）；**最后一行**缺失的 `right` 则从上一个区间的宽度推断（`left + previous_width`）。若一个单行的分布没有显式的 `right`，则会被拒绝——因为没有下一行可供推断。
- 显式给出的 `right` 值必须大于其自身的 `bin` 值，且不得与上一个区间的 `right` 重叠。
- 所有行解析完成后，频率会被归一化，使其总和恰好为 `1.0`；在归一化之前，原始总和必须已经在 `1e-6` 的误差范围内接近 `1.0`（这是对明显错误输入的合理性检查，而不是对任意权重的通用重归一化）。

得到的 `TargetDistribution` 持有一个 `Vec<DiameterBin>`，其中每个区间的区间范围是半开区间 `[left, right)`——但最后一个区间的 `right` 边界是闭合的，这样恰好等于整体最大值的直径也能落入某个区间（`bin_for_diameter`）。若某个直径落在非相邻的显式区间之间的空隙中，或落在覆盖范围之外，则返回 `None`。

## 度量计算：等体积直径与球形度

在候选网格能够被引导至任何区间之前，必须先计算其等体积直径——而这要求网格本身是良构的。`mesh_metrics` 首先调用私有函数 `mesh_is_closed`，该函数验证：每个三角形的索引均指向真实且不同的顶点、没有重复的面、每条边恰好被两个绕向相反的三角形共享（流形，且绕向一致），并且每个边连通的壳体都具有非零的有符号体积。只有在通过此项检查后，`mesh_metrics` 才会计算：

- **体积**与**表面积**：通过既有的散度定理和三角形求和例程计算。
- **等体积直径**：`d = (6V / π)^(1/3)`——与网格体积相同的球体的直径。
- **球形度**：`Ψ = π^(1/3) · (6V)^(2/3) / A`——即 Wadell 球形度，等体积球体表面积与网格实际表面积之比（完美球体为 1.0，其他形状小于 1.0）。

若网格是开放的/畸形的，或体积/面积/直径/球形度计算结果为非有限值或非正值，则 `mesh_metrics` 返回 `None`。在 `PackPipeline::run` 中，对于一个受目标引导的候选，`mesh_metrics` 返回 `None` 只会简单地丢弃该候选（`reject_candidate!`）——畸形候选会被跳过而不会引发错误，这与"目标体积分数优先于任何单个候选"这一原则是一致的。

## 区间分配算法：`TargetDistribution::choose_bin`

这是该功能的核心。对于每一次放置尝试，只要目标分布处于激活状态，`choose_bin` 就会决定下一个候选应当被推向哪个直方图区间。

### 区间欠账（bin debt）

对于当前放置轮次中尚未尝试过的每个区间，算法计算一个**欠账（debt）**：

```
debt(bin) = frequency(bin) × next_count − observed_count(bin)
```

其中 `next_count` 是目前已接受的颗粒总数加一（即如果本次放置成功，总数将变成多少）。欠账衡量的是：即使把这次放置计入该区间，该区间的观测占比相对其目标占比仍会有多大差距。算法会选择欠账最大且为正的区间（严格大于 `1e-12` 的容差）——这是相对于运行中的直方图理应达到的状态而言，代表性最不足的区间。

**已验证示例**（`tests/pack_target_tests.rs::strict_choice_follows_largest_deficit`）：对于一个三区间分布，频率为 `[0.5, 0.3, 0.2]`，从空状态开始，连续十次 `choose_bin` 调用（每次都立即记录为成功）依次选择区间的顺序为 `[0, 1, 2, 0, 0, 1, 0, 2, 1, 0]`，最终接受计数为 `[5, 3, 2]`——正是十个颗粒按频率所隐含的精确 5:3:2 比例。这十次选择均被归类为 `Scaled`（见下文），因为一个没有天然区间提示的裸 `DiameterBin` 测试夹具总是需要缩放。

### 当没有区间存在正欠账时的回退策略

如果所有剩余区间的欠账都小于或等于零（即观测直方图已经匹配、甚至略微超出每个区间的目标占比），`choose_bin` 会切换策略：对每个未尝试的区间，它会计算——如果该区间被记入下一次放置——*整个*直方图的全变差距离（TVD）将变为多少，并选择使该预计 TVD 最小的区间。若差异在 `1e-15` 以内则视为并列，此时按区间中点最小值打破平局，从而给出确定性的、可复现的区间选择结果。

### `BinChoiceKind`：Natural（天然）、Scaled（缩放）与 Fallback（回退）

一旦某个区间索引被选定，`choose_bin` 会对该选择进行分类：

- **`Natural`（天然）**——候选自身、未经缩放的等体积直径本就落在所选区间内（`natural_bin == Some(index)`），且这是本轮该候选被尝试的*第一个*区间选择（调用开始时 `attempted` 为空）。此时无需缩放，候选按原样使用。
- **`Scaled`（缩放）**——同样满足"本轮第一次尝试"的条件，但候选的天然直径不在所选区间内。此时 `PackPipeline::run` 会调用 `scale_mesh_to_equivalent_diameter`，对候选的顶点进行统一缩放，使其等体积直径恰好落在所选区间的中点（`DiameterBin::midpoint`）上，并重新验证（对缩放后的度量调用 `bin_for_diameter`）缩放后的候选确实落在预期区间内，然后才继续处理。
- **`Fallback`（回退）**——此次调用发生时 `attempted` 已经非空，意味着该候选/本轮之前至少已有一个区间选择被耗尽。此时选中的任何区间——无论是通过欠账规则还是 TVD 回退规则选出的——都会被归类为 `Fallback`，无论它是否恰好等于候选的天然区间，因为算法此时已不再追求其最初的选择。

`attempted` 是一个由调用方持有的 `BTreeSet<usize>`，在一轮放置内的多次 `choose_bin` 调用之间传递；`choose_bin` 本身从不从中移除索引——只有 `PackPipeline::run` 会这样做，且仅在特定的重试条件下（见下一节）。

### 缩放与几何过滤器的重新检查

缩放网格会改变其体积、纵横比以及表面积/体积的尖锐程度——所有这些都可能受 `packing.filters` 的限制。因此，`PackPipeline::run` 会在缩放*之后立即*对**缩放后**的候选（而非原始候选）重新应用 `check_geometry_filters`（并开启体积过滤器的强制检查）。一个在天然尺寸下通过了过滤器的候选，在被放大或缩小以匹配目标区间后仍可能被拒绝。

## `PackPipeline::run` 中的重试与回退升级

`PackPipeline::run` 对为某个区间努力尝试的力度设有上限，超过后就会放弃该区间、转而处理当前候选。每轮设有一个按区间索引的计数器 `bin_probe_failures: Vec<usize>`，用于记录某个被选中的区间在*被选中之后*失败的次数（缩放失败、几何过滤器失败、边界/碰撞/距离/虚像检查失败，或盒内体积非正）。常量 `TARGET_BIN_PROBES = 4` 对此设有上限：

- 只要 `bin_probe_failures[index] < TARGET_BIN_PROBES`，该区间的一次失败就会将其从 `attempted_bins` 中移除，因此*下一次* `choose_bin` 调用可以自由地再次选中同一个区间（依然会被归类为 `Natural`/`Scaled`，因为 `attempted` 恢复到未阻止它的状态）——实际上给了每个区间最多四次使用全新候选进行的独立尝试机会，之后才算作耗尽。
- 一旦 `bin_probe_failures[index]` 达到 `TARGET_BIN_PROBES`，该索引会在本轮中永久留在 `attempted_bins` 中。下一次 `choose_bin` 调用就会看到一个非空的 `attempted` 集合，这正是产生 `Fallback` 分类的条件——算法此时已放弃引导至其偏好的区间，转而接受欠账/TVD 规则找到的任意下一个区间。
- 如果 `choose_bin` 直接返回 `None`（所有区间都已尝试过，没有剩余可尝试的），该候选的放置循环会直接中断，转而处理下一个候选提议或尝试。
- 如果一整轮目标区间的尝试都已耗尽而没有成功放置（`attempts >= max_attempts` 且 `had_bin_choice` 为真），整个堆积循环会终止，而不是无休止地循环追逐一个无法达成的区间。

## 球形度引导：`SphericityState`（可选，软约束）

与直径区间引导相互独立，一次运行可以选择在堆积配置中设置 `target_mean_sphericity`（以及可选的 `mean_sphericity_tolerance` 容差带）。设置后，`PackPipeline::run` 每次放置尝试会抽取多达 4 个候选形状（`proposal_draws = 4`），而不是仅一个，为每个候选计算 `mesh_metrics`，并用 `SphericityState::projected_error` 对每个候选打分：即如果接下来接受该特定候选，*运行中的均值*球形度将偏离 `[target − tolerance, target + tolerance]` 容差带多远（若预计均值本已落在带内，则为零）。候选按此预计误差升序排序，并按此顺序依次尝试。

这明确是一种软的、尽力而为的引导：`projected_error` 仅在同一次抽取内对候选进行相对排序——它从不会仅因为某个候选偏离目标较远就直接拒绝它。`PackPipeline::run` 会接受排序在前、且首个通过其余全部检查的候选；放置循环中没有任何环节会仅因球形度超出范围而拒绝一个候选。运行结束时，如果最终的均值球形度未落入容差带，管线会打印一条警告（"volume fraction was prioritized"，即体积分数被优先考虑），而不会将其视为错误。

**已验证示例**（`tests/pack_target_tests.rs::sphericity_projection_selects_direction_that_repairs_mean`）：从 `sum = 0.6, count = 1`（运行均值 0.6）出发，目标为 0.8 且无容差，一个球形度为 0.6 的候选（不会将均值移向目标）所预计的误差，大于一个球形度为 0.9 的候选（会将均值移向 0.75）所预计的误差——低球形度候选的预计误差恰好比高球形度候选的高出 0.05，这证实了该排序确实正确地偏向能够修复运行均值的候选。

## 状态跟踪：只有成功的放置才会改变状态

`DistributionState`（按区间的 `counts` 和 `attempts`，加上 `natural`/`scaled`/`fallback` 计数以及运行中的最小/均值/最大缩放因子统计）和 `SphericityState` 在 `PackPipeline::run` 中恰好只在一个位置被更新：即候选通过了*每一项*检查之后——边界约束、成对碰撞、最小邻距、周期性虚像碰撞（模式 3），以及有限且为正的盒内裁剪体积。`record_attempt` 会在更早的时刻被调用，即一旦某个区间被选中就调用（因此失败的尝试仍会计入每个区间的 `attempts` 计数，用于报告），但 `record_success` 和 `SphericityState::record_success` 只有在 `current_volume += in_box_volume; placed.push(candidate);` 之后才会被执行——也就是说，失败的放置尝试（任何 `reject_candidate!` 路径）从不会改变已接受的区间计数、类别计数、缩放因子统计，或球形度的运行均值。这正是欠账公式之所以有意义的原因：`observed_count` 始终反映的是真正被放置的颗粒，而不是差点成功的尝试。

## 报告：`summarize` 与 `write_distribution_comparison_csv`

堆积循环结束后，如果配置了目标分布，`TargetDistribution::summarize` 会根据最终的 `DistributionState` 计算三项关键误差统计量：

- **全变差距离（TVD）**：`TVD = 0.5 × Σ |observed_frequency(bin) − target_frequency(bin)|`，对所有区间求和——这是两个离散分布之间标准的 TVD，取值范围为 `[0, 1]`。
- **最大绝对误差**：所有区间中 `max(|observed_frequency(bin) − target_frequency(bin)|)` 的值。
- **舍入误差基线**（`rounding_max_absolute_error`）：即使是一个假想中完美的连续匹配，仍然必须将每个区间的理想小数占比（`frequency × count`）向下取整为整数颗粒计数。`summarize` 通过最大余数分配法计算这一可达到的最佳基线：先对每个区间的理想计数取整（下取整），然后将剩余的颗粒逐一分配给小数余数最大的区间（若并列则按区间索引打破平局），直到总数分配完毕——并报告*这一理想整数分配*仍会表现出的最大绝对频率误差。将实际的 `max_absolute_error` 与该基线相比较，可以将真正不可避免的舍入误差（由堆积有限数量的离散颗粒这一事实所导致）与真正的算法失配区分开来。上文提到的 `strict_choice_follows_largest_deficit` 测试正是断言了这一关系：`summary.max_absolute_error <= summary.rounding_max_absolute_error + 1e-12`，即区间欠账算法在该场景下达到的误差不劣于理论上的舍入下限。

此外，`write_distribution_comparison_csv` 会在已打包的 STL 文件旁写出 `<output_stem>_diameter_distribution.csv`，每个区间一行，列为：`bin, right, target_frequency, target_count, actual_count, actual_frequency, frequency_error, count_error, attempts`。`PackPipeline::run` 也会将同样的逐区间明细连同汇总统计量、natural/scaled/fallback 放置类别计数，以及（如果发生过任何缩放）所有成功放置候选中应用的最小/均值/最大缩放因子，以 `[Info]` 行的形式打印到标准输出。如果有任何放置发生了回退（`distribution_state.fallback > 0`），则会打印一条 `[Warning]` 行，说明目标分布已被放宽以优先保证体积分数——这正是 `PLAN.md` 中所述设计原则在运行自身输出中的直接体现。

## Cross-references

- [pipeline-packing.md](../reference/pipeline-packing.md) — 该功能的完整逐函数参考：
  [`TargetDistribution::choose_bin`](../reference/pipeline-packing.md#targetdistributionchoose_bin),
  [`load_target_distribution_csv`](../reference/pipeline-packing.md#load_target_distribution_csv),
  [`write_distribution_comparison_csv`](../reference/pipeline-packing.md#write_distribution_comparison_csv),
  [`TargetDistribution::summarize`](../reference/pipeline-packing.md#targetdistributionsummarize),
  [`SphericityState::projected_error`](../reference/pipeline-packing.md#sphericitystateprojected_error),
  [`PackPipeline::run`](../reference/pipeline-packing.md#packpipelinerun)。
- [geometry-analysis.md](../reference/geometry-analysis.md) — 底层的度量函数：
  [`mesh_metrics`](../reference/geometry-analysis.md#mesh_metrics),
  [`scale_mesh_to_equivalent_diameter`](../reference/geometry-analysis.md#scale_mesh_to_equivalent_diameter)。
