# `split-filter` 流水线示例

## 功能说明

`split-filter` 接收一个包含多个不相连颗粒的 STL 网格（例如某次锻造或测量步骤的合并输出），将其拆分为每个连通分量一个 STL 文件，并可选地过滤掉不满足几何标准（长径比、尖锐度比和/或体积规则）的颗粒。它会将保留下来的颗粒写为独立的 STL 文件，并附带一份文本报告，汇总保留与移除的情况。

## 配置

`data/input/split_filter_config.yaml`（该流水线在代码仓库中真实的默认配置）：

```yaml
input:
  # Input STL file or folder containing STL files
  path: "data/input/particles.stl"

output:
  # Output is always a folder containing split particle STL files
  folder: "data/output/split_particles"
  # Output naming prefix, generated as: prefix + index + ".stl"
  # Example: "particle_" -> particle_1.stl, particle_2.stl, ...
  prefix: "particle_"
  # Optional report path. If omitted, defaults to: parent(<output.folder>)/split_filter_report.txt
  report_path: "data/output/split_filter_report.txt"

filter:
  # Set false to skip all filtering and only do splitting
  enabled: true

  # Optional geometric filters
  max_aspect_ratio: 3.0
  max_sharpness_ratio: 2.0

  volume:
    # Volume filtering mode:
    # - none: no volume filter
    # - range: keep particles in [min, max], -1 means no bound
    # - lognormal_rebalance: suppress overrepresented size ranges based on fitted lognormal profile
    mode: "range"

    # Used by mode=range
    min: 10.0
    max: -1

    # Used by mode=lognormal_rebalance
    bins: 12
    over_factor: 1.25
```

`input.path` 指向 `data/input/particles.stl`，这是代码仓库中一个真实的、包含 15 个颗粒的合并网格。在本示例中，我们保持过滤设置不变，但将配置复制到 `/tmp/rustmspt-doc-examples/split_filter_config_demo.yaml`，并将 `output.report_path` 重定向到 `/tmp/rustmspt-doc-examples/` 下（同时通过 CLI 的 `--output` 参数覆盖 STL 输出文件夹），这样运行就不会向 `data/output/` 写入任何内容。

| 字段 | 含义 |
|---|---|
| `filter.enabled` | 当为 `false` 时，所有颗粒都会被拆分出来，但不会因其他 `filter.*` 设置而被移除。 |
| `filter.max_aspect_ratio` | 颗粒包围盒长径比（最长边 / 最短边）的上限；超过此值的颗粒将被移除。 |
| `filter.max_sharpness_ratio` | 颗粒尖锐度比（一种网格形状尖刺程度的指标）的上限；超过此值的颗粒将被移除。 |
| `filter.volume.mode` | `"none"` 禁用体积过滤；`"range"` 保留体积落在 `[min, max]` 内的颗粒；`"lognormal_rebalance"` 依据拟合的对数正态尺寸分布削减过度代表的尺寸区间，而不是采用硬性截断。 |
| `filter.volume.min`/`max` | `"range"` 模式下使用的边界；`max` 为 `-1` 表示上界不受限。 |
| `output.report_path` | 文本报告（颗粒数量、保留/移除直方图）写入的位置；若省略，默认位于 `output.folder` 旁边。 |

完整字段参考见 `../reference/config.md`（`SplitFilterConfig` 一节），本示例所演示的 `SplitFilterPipeline::run` 行为（拆分、逐步过滤统计与报告生成）见 `../reference/pipeline-crop-and-splitfilter.md`。

## 运行方式

```bash
cp data/input/split_filter_config.yaml /tmp/rustmspt-doc-examples/split_filter_config_demo.yaml
# edit output.report_path in the copy to /tmp/rustmspt-doc-examples/split_filter_report.txt

./target/release/rustmspt split-filter \
  --config /tmp/rustmspt-doc-examples/split_filter_config_demo.yaml \
  --output /tmp/rustmspt-doc-examples/split_particles
```

`--output` 会覆盖从已加载的 YAML 中读取的 `output.folder`（参见 `src/main.rs` 中的 `Commands::SplitFilter` 分支）；`--output` 不会影响 `output.report_path`，这也是为什么临时配置还需要重定向 `report_path`，使报告同样落在 `/tmp/rustmspt-doc-examples/` 下，从而保持 `data/output/` 不受影响。

## 预期输出

以上运行的真实捕获 stdout：

```
[Info] Split filter completed: input particles=15, kept=14, removed=1
[Info] Output folder: /tmp/rustmspt-doc-examples/split_particles
[Info] Output prefix: particle_
[Info] Report written: /tmp/rustmspt-doc-examples/split_filter_report.txt
```

本次运行耗时约 40 毫秒（`real 0m0.042s`）。输出文件夹中包含 14 个文件——从 `particle_1.stl` 到 `particle_14.stl`——每个存活颗粒对应一个文件。

`/tmp/rustmspt-doc-examples/split_filter_report.txt` 的真实内容：

```
Method: split_filter
Input path: data/input/particles.stl
Output folder: /tmp/rustmspt-doc-examples/split_particles
Report path: /tmp/rustmspt-doc-examples/split_filter_report.txt
Output prefix: particle_
Total split particles: 15

Filter steps:
initial: before=15, after=15, removed=0
max_aspect_ratio <= 3: before=15, after=14, removed=1
max_sharpness_ratio <= 2: before=14, after=14, removed=0
volume range filter (min=10, max=-1): before=14, after=14, removed=0

Summary:
Kept particles: 14
Removed particles: 1
Kept volume statistics: 
  min: 77.660666
  max: 815.672294
  mean: 349.195035
  median: 271.127167

Kept volume histogram (10 bins):
  [77.660666, 151.461829) |    3 | ##############################
  [151.461829, 225.262992) |    3 | ##############################
  [225.262992, 299.064155) |    2 | ####################
  [299.064155, 372.865317) |    1 | ##########
  [372.865317, 446.666480) |    0 | #
  [446.666480, 520.467643) |    2 | ####################
  [520.467643, 594.268806) |    1 | ##########
  [594.268806, 668.069969) |    0 | #
  [668.069969, 741.871131) |    0 | #
  [741.871131, 815.672294) |    2 | ####################

Volume histogram comparison (before vs after, 10 bins):
  [30.282125, 108.821142) | before    2 ########         | after    1 ####            
  [108.821142, 187.360159) | before    4 ################ | after    4 ################
  [187.360159, 265.899176) | before    2 ########         | after    2 ########        
  [265.899176, 344.438193) | before    2 ########         | after    2 ########        
  [344.438193, 422.977210) | before    0 #                | after    0 #               
  [422.977210, 501.516226) | before    1 ####             | after    1 ####            
  [501.516226, 580.055243) | before    1 ####             | after    1 ####            
  [580.055243, 658.594260) | before    1 ####             | after    1 ####            
  [658.594260, 737.133277) | before    0 #                | after    0 #               
  [737.133277, 815.672294) | before    2 ########         | after    2 ########        
```

解读该报告：由 `particles.stl` 拆分出的 15 个颗粒中，只有 `max_aspect_ratio <= 3` 这一步实际移除了内容（移除了 1 个颗粒，一个过于细长的碎片）；`max_sharpness_ratio` 和体积范围过滤（`min=10, max=-1`）在此处都没有进一步移除任何颗粒，因为该合并网格中的颗粒本身已经相当紧凑，没有低于 10 单位的体积下限。底部的"before vs after"直方图展示的是*完整*体积范围（`[30.28, 815.67)`，基于过滤前全部 15 个颗粒计算）相同的 10 个区间划分，因此可以逐区间看出长径比移除操作的影响——第一个区间的颗粒数从 2 降到了 1。

## 说明

- `filter.enabled: false` 适用于"仅拆分"的场景——例如下游工具需要不论形状/尺寸的每个颗粒，并会自行进行后续过滤时。
- 将 `filter.volume.mode` 切换为 `"lognormal_rebalance"` 会用一种统计重平衡处理替代硬性的 `min`/`max` 截断，该处理依据拟合的对数正态分布来削减过度代表的尺寸区间（`filter.volume.bins`/`over_factor` 控制区间数量以及对过度代表区间的削减力度）——当目标是获得接近对数正态的尺寸分布而非硬性的体积上下限时非常有用。
- `input.path` 也可以接受一个包含多个 STL 文件的文件夹（每个文件先合并再拆分），而不仅限于单个合并 STL——这在颗粒已经在早期阶段按文件单独导出时很方便。
- 关于完整的流水线机制——包括拆分过程中如何检测连通分量，以及长径比/尖锐度比的确切定义——见 `../reference/pipeline-crop-and-splitfilter.md`。
