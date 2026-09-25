# `optimize` 流水线示例

## 功能说明

`optimize` 通过带自适应温度调度和多个并行"岛屿"的模拟退火，在固定尺寸的箱体内调整颗粒的位置与取向，使堆积结构的两点相关函数 (S2) 匹配目标 S2 曲线，同时满足碰撞与边界约束。它通常在 `pack` 之后运行，用于将随机堆积的结构细化为符合实测或理论目标微结构统计量的结构。

## 配置

`data/input/optimize_config.yaml`（该流水线在代码仓库中真实的默认配置）：

```yaml
input:
  stl_path: "data/output/packed_result.stl" # Or single STL

target:
  type: 'manual_array'
  s2_array: [0.063074, 0.051980, 0.041610, 0.034790, 0.027570, 0.022830, 0.017730, 0.014000, 0.010800, 0.008240, 0.006440]
  stl_path: "data/input/particles.stl"
  stl_bounding_box: []

box:
  dimensions: [50.0, 50.0, 50.0]

optimization:
  max_iterations: 5_000
  initial_temperature: 0.1
  cooling_rate: 0.99
  adaptive_temp_window: 50
  target_acceptance_low: 0.20
  target_acceptance_high: 0.45
  adaptive_heat_factor: 1.08
  adaptive_cool_factor: 0.94
  adaptive_temp_ceiling_factor: 5.0

  r_max: 10
  voxel_pitch: 1.0
  mc_method: 'monte_carlo'
  mc_samples: 8_000

  max_translation: 25.0
  max_rotation_deg: 30.0
  rotation_mode: 'none'
  rotation_axis_vector: [0.0, 0.0, 1.0]

  min_neighbor_distance: 1.0
  mode: 2
  min_boundary_dist: 1.5
  min_cross_boundary_depth: 3.0

  prune_enabled: true
  prune_tolerance: 0.01
  prune_max_rounds: 120
  prune_eval_samples: 1000

  cpu_max: -1
  orient_to_positive_volume: false

  acceleration:
    mode: auto

output:
  path: "data/output/optimized_structure.stl"
```

（完整文件还包含大量为每个字段说明含义的行内注释，以及两套备选的"推荐配置"温度调度方案；为简洁起见上面已省略——完整的带注释版本请直接查看 `data/input/optimize_config.yaml`，或参见 `../reference/config.md`。）

`input.stl_path` 指向 `data/output/packed_result.stl`，这是代码仓库中已经存在的一个真实网格（先前一次 `pack` 运行的输出）。本示例除了 `optimization.max_iterations` 和 `output.path` 之外，其余配置保持不变，并复制到一个临时文件中——原因见下方的**说明**部分。

| 字段 | 含义 |
|---|---|
| `seed`（可选） | 固定单岛运行的全部随机决定（含蒙特卡洛 S2 噪声），同一种子在任意 worker 数下复现同一输出。省略则每次运行随机不同。 |
| `target.type` / `target.s2_array` | `"manual_array"` 直接提供 11 个目标 S2 值（对应 `r = 0..10`）；`"reference_stl"` 则从参考网格（`target.stl_path`）计算目标 S2 曲线。 |
| `box.dimensions` | 颗粒进行堆积/优化所在的固定尺寸箱体。 |
| `optimization.max_iterations` | 要运行的模拟退火迭代次数。代码仓库默认值为 **5,000**；本示例出于演示目的将其减少（见"说明"）。 |
| `optimization.initial_temperature` / `cooling_rate` / `adaptive_*` | 模拟退火的温度调度：起始温度、每次迭代的衰减，以及根据近期移动接受率更快升温或降温的自适应窗口。 |
| `optimization.mc_method` / `mc_samples` / `voxel_pitch` / `r_max` | 控制每次迭代如何估算 S2：蒙特卡洛采样（`mc_samples` 个点对）还是精确的体素网格计算，以及 S2 与目标比较时所覆盖的相关长度范围（`r_max`）。 |
| `optimization.prune_enabled` / `prune_tolerance` / `prune_max_rounds` | 一个退火前的预处理阶段，在进入模拟退火循环之前先移除颗粒，使体积分数接近 `target.s2_array[0]`。 |
| `optimization.mode` / `min_boundary_dist` / `min_cross_boundary_depth` | 边界处理模式（1=严格，2=宽松，3=周期性），以及用于校验颗粒相对箱体壁面放置情况的最小间隙/穿出深度。 |
| `optimization.acceleration.mode` | `auto` 会在 GPU 可用且工作量足够大时自动选用 GPU 计算；`cpu`/`gpu` 则强制指定后端。 |

完整字段参考见 `../reference/config.md`（`OptimizationConfig` 一节），本流水线所实现的退火/岛屿模型算法（包括自适应温度调度与 S2 损失定义）见 `../algorithms/simulated-annealing-island-model.md`。

## 运行方式

```bash
cp data/input/optimize_config.yaml /tmp/rustmspt-doc-examples/optimize_config_demo.yaml
# edit the copy: optimization.max_iterations: 5_000 -> 500
# edit the copy: output.path -> /tmp/rustmspt-doc-examples/optimized_structure.stl

./target/release/rustmspt optimize \
  --config /tmp/rustmspt-doc-examples/optimize_config_demo.yaml
```

由于该文件已经作为先前 `pack` 运行的产物存在于代码仓库中，`input.stl_path`（`data/output/packed_result.stl`）在默认配置的基础上未做改动；只修改了临时副本中的 `max_iterations` 和 `output.path`，`data/input/optimize_config.yaml` 本身从未被修改过。

## 预期输出

以上运行的真实捕获 stdout（`max_iterations: 500`）：

```
[Info] CPU setting: cpu_max=-1 -> using 8 worker threads (available 8).
[Info] Rayon pool threads (effective): 8
[Info] Rotation mode: none
[Info] Acceleration: requested=auto, effective=cpu
[Info] Acceleration fallback: workload 125000 voxels below gpu_min_voxels threshold 250000
[Info] Pre-filter removed 25 particles fully outside optimization bbox.
[Info] S2 config: method=monte_carlo, r_max=10, mc_samples=8000, voxel_pitch=1.000000
[Info] Input stage: VF 0.024314, Loss 0.061045, Method monte_carlo
[Info] Pruning stage: initial VF 0.024314, target VF 0.063074
[Info] Pruning completed: rounds 0, particles 6, VF 0.024314, Loss 0.063054
[Info] Starting optimization with 6 particles.
[Info] Initial loss: 0.062740
[Info] Optimization completed.
[Info] Best loss: 0.040908
[Info] Final volume: 4760.167334
[Info] Final S2 points: 11
[Info] Orientation fix enabled: false
[Info] S2 history saved: /tmp/rustmspt-doc-examples/s2_history.txt
[Info] Timing summary: total 2.41s | S2 2.20s | collision/constraints 0.11s
```

整个运行过程——包括 500 次迭代中的 S2 计算——在这台 8 核机器上耗时约 2.4 秒（`real 0m2.425s`），其中 S2 计算占据了大部分耗时（2.41 秒总耗时中的 2.20 秒）。`acceleration.mode: auto` 回退到了 CPU，因为该配置下的工作量（50x50x50 箱体、`voxel_pitch: 1.0` 时对应 125,000 体素）低于 GPU 加速所需的 250,000 体素阈值。

`/tmp/rustmspt-doc-examples/s2_history.txt` 的真实内容：

```
Target S2: 0.063074 0.051980 0.041610 0.034790 0.027570 0.022830 0.017730 0.014000 0.010800 0.008240 0.006440
Input S2: 0.024376 0.022006 0.018800 0.015484 0.015554 0.011562 0.008457 0.007234 0.005827 0.005582 0.004822
Input Loss: 0.061045
Pruning Start: particles 6 | VF 0.024314 -> target 0.063074 | Loss 0.063054
Pruning Completed: rounds 0 | particles 6 | VF 0.024314 | Loss 0.063054
Post-Pruning S2: 0.024376 0.020448 0.019012 0.014409 0.013860 0.010388 0.008816 0.007325 0.005757 0.004330 0.003132
Post-Pruning Loss: 0.062740
Iter 0: Loss 0.055341 | S2 0.027672 0.025171 0.020261 0.019096 0.017981 0.011468 0.008946 0.007190 0.005950 0.005664 0.002733
Iter 8: Loss 0.052881 | S2 0.028968 0.025687 0.022836 0.018341 0.017139 0.012570 0.010298 0.008667 0.007488 0.005842 0.004368
Iter 13: Loss 0.045550 | S2 0.033088 0.026338 0.030219 0.022056 0.018320 0.013229 0.012895 0.010496 0.007875 0.008782 0.004446
Iter 17: Loss 0.044303 | S2 0.033016 0.032918 0.023109 0.022878 0.019002 0.014776 0.011294 0.011076 0.007084 0.006203 0.004090
Iter 19: Loss 0.040908 | S2 0.033104 0.033943 0.027433 0.023158 0.020632 0.018903 0.012926 0.011702 0.008443 0.004379 0.005332
Final Best S2: 0.033104 0.033943 0.027433 0.023158 0.020632 0.018903 0.012926 0.011702 0.008443 0.004379 0.005332
Final Best Loss: 0.040908
```

解读结果：输入网格自身的 S2 曲线（`Input S2`）在每个 `r` 处都远低于 `Target S2`，初始损失为 `0.061045`。剪枝阶段（通过 `prune_enabled: true` 启用）将颗粒数量从 21 个（经过移除 25 个完全位于箱体外的颗粒的边界框预过滤之后）减少到 6 个，使体积分数接近目标值在 `r=0` 处的取值（`0.063074`），在这个例子中收敛得非常快（`rounds 0`），因为剪枝后的颗粒集合已经十分接近目标体积分数。此后，模拟退火在尝试的 500 次迭代中只记录了 5 次提升迭代（`Iter 0`、`8`、`13`、`17`、`19`——仅记录被接受且使损失得到改进的移动），将损失从 `0.062740` 降到最终最优值 `0.040908`。写出的网格文件（`/tmp/rustmspt-doc-examples/optimized_structure.stl`，约 98.9 KB）包含相同的 6 个颗粒，已重新定位/重新定向到找到的最优配置。

## 说明

- **本次运行使用 `max_iterations: 500` 仅用于演示目的。** 代码仓库真实的默认配置（`data/input/optimize_config.yaml`）设置的是 `max_iterations: 5_000`——是本示例的十倍——这会给自适应温度模拟退火提供更充分的机会向目标 S2 曲线收敛，相应地运行时间也会成比例延长。在更大颗粒数量和箱体尺寸下的生产环境运行，运行时间应预期远超此处所见的约 2.4 秒，且应按默认配置中出货的成千上万次迭代来预留时间，而不是以上示例中所用的数百次。
- `data/input/optimize_config.yaml` 从未被修改；所有改动均在 `/tmp/rustmspt-doc-examples/` 下的临时副本中进行，`output.path` 同样被重定向到该目录，因此没有向 `data/output/` 写入任何内容。
- 在本示例中，堆积网格原有的颗粒里只有 6 个在边界框预过滤和剪枝阶段后存活下来，因为目标 S2 在 `r=0` 处的取值（`0.063074`，即目标体积分数）远低于 `packed_result.stl` 最初堆积时所用的体积分数；剪枝会持续移除颗粒，直到体积分数接近该目标值后才开始退火。若目标值更接近输入网格实际的体积分数，则会保留更多颗粒。
- S2 历史日志只记录退火移动被接受*且*改进了已知最优损失的迭代，而非每一次尝试的移动——这就是为什么日志中的迭代编号是跳跃的（`0, 8, 13, 17, 19, ...`），而不是逐行递增 1。
- 在本配置中，`optimization.rotation_mode: 'none'` 会在扰动过程中禁用颗粒旋转；将其设置为 `'x'`/`'y'`/`'z'`/`'vector'`/`'any'` 则分别允许绕固定轴、任意向量或不受约束地进行旋转移动——当需要改变颗粒取向（而不仅仅是位置）以达到目标 S2 时非常有用。
- 完整的退火算法（自适应温度控制、岛屿模型并行以及 S2 损失函数）见 `../algorithms/simulated-annealing-island-model.md`，`OptimizePipeline::run` 的实现参考见 `../reference/pipeline-optimize.md`。

## Execution diagnostics added by PERF-01/02

当前运行输出并保存 `S2 execution: requested=..., effective=..., method=..., voxel_pitch=..., reason=...`，
历史另含 `Execution: workers=..., islands=..., active_island_limit=...`。正 pitch 的 exact/MC 配置保留 CPU 体素语义，
请求 GPU 不会静默切换为连续 MC。`Selected Search Loss` 是历史最佳分数，`Final Best S2/Loss` 使用相同方法和完整预算复核。
SA 是随机搜索，不同于 placement，不承诺跨线程输出一致。上面的旧捕获输出早于这些新增诊断字段。

### 计时行（2026-09-25 新增）

上面的捕获输出早于共享阶段计时器。当前版本还会为每个已完成阶段打印 `[Timing] optimize stage=<name> seconds=<f>`，随后打印 `[Timing] optimize workers=<n>` 与 `[Timing] optimize peak_rss_bytes=<n|unavailable>`，以及一到两行 `[GridStats]`。阶段名称见 `../reference/pipeline-core.md`（`pipeline/timing.rs`）；输出文件不变。
