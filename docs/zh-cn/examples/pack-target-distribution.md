# 使用目标粒径分布进行堆积

## 功能说明

本演示在设置了 `packing.target_diameter_distribution_csv` 的情况下运行 `pack`，
这样流水线就不再按输入库恰好产生的尺寸接受候选颗粒，而是将每次被接受的放置
引导至用户提供的粒径直方图中代表性不足的区间——按需重缩放候选颗粒——
使*已放置*颗粒的等体积直径逼近目标分布。它还演示了成对的、可选的
`target_mean_sphericity` / `mean_sphericity_tolerance` 软引导旋钮。这是最新的
堆积功能（`git log` 显示它在提交 `cc905c1`，"feat: add target-aware void
packing" 中引入）；完整的算法推导见
[`../algorithms/packing-target-diameter-distribution.md`](../algorithms/packing-target-diameter-distribution.md)，
本文档聚焦于端到端运行它以及解读其输出。

此处使用的目标分布是仓库中真实内置的示例
`data/input/gu2019_fig7b_pore_distribution.csv`——一个数字化的孔径直方图
（来自 Gu 等人 2019 年论文的图 7b），有 25 个宽度为 1 的区间，覆盖直径 5–30，
经 `tests/pack_target_tests.rs::example_paper_distribution_is_valid_and_normalized`
确认（`bins.len() == 25`，第一个区间为 `[5, 6)`，最后一个区间的 `right == 30.0`，
频率之和为 `1.0`）。

## 配置

`data/input/pack_config.yaml` 的临时副本位于
`/tmp/rustmspt-doc-examples/pack_target_distribution.yaml`，输出路径重定向到临时位置，
`target_volume_fraction` 略微提高到 `0.03`（仍然很快，但比普通示例的 `0.02`
产生更多已放置颗粒，从而得到更丰富的直方图），并启用了之前被注释掉的
球形度引导字段：

```yaml
input:
  path: "data/input/particles.stl"

output:
  path: "/tmp/rustmspt-doc-examples/pack_target_distribution_result.stl"

box:
  dimensions: [100.0, 100.0, 100.0] # X, Y, Z size

packing:
  target_volume_fraction: 0.03

  mode: 2
  max_attempts: 2000
  min_neighbor_distance: 1.0

  rotation_mode: 'none'
  rotation_axis_vector: [0.0, 0.0, 1.0]

  min_boundary_dist: 1.5
  min_cross_boundary_depth: 3.0

  cpu_max: -1
  orient_to_positive_volume: false

  # Optional target void diameter count-frequency CSV (unitless, matching STL units).
  # Columns: bin (left edge), right, frequency. Frequencies are normalized and sum to 1.
  # Also writes <output_stem>_diameter_distribution.csv beside the packed STL.
  target_diameter_distribution_csv: "data/input/gu2019_fig7b_pore_distribution.csv"

  # Optional soft target for the count-weighted arithmetic mean sphericity of placed voids.
  # Uniform diameter scaling preserves sphericity. Packing still prioritizes target_volume_fraction.
  target_mean_sphericity: 0.82
  mean_sphericity_tolerance: 0.02

  # Geometry filters. With a target diameter CSV, min_volume is checked after scaling.
  filters:
    min_volume: 15.0
    max_aspect_ratio: 3.0
    max_sharpness_ratio: 2.0
```

| 字段 | 含义 |
|---|---|
| `target_diameter_distribution_csv` | 指向一个 `bin[,right],frequency` CSV 文件的路径（见下文），描述目标粒径直方图；设置该字段会在 `PackPipeline::run` 中启用区间欠账（bin-debt）引导和重缩放。 |
| `target_mean_sphericity` | 已接受放置的按计数加权的运行均值 Wadell 球形度的可选软目标。 |
| `mean_sphericity_tolerance` | 围绕 `target_mean_sphericity` 的接受带宽的半宽；`0.0`/未设置意味着精确值目标。 |
| `filters.min_volume` | 依然生效，但——正如配置注释所述，并在算法文档中得到确认——当启用目标 CSV 时，是针对**缩放后**的候选体积进行检查，而不是自然体积。 |

完整字段参考见 [`../reference/config.md`](../reference/config.md)（`packing.rs` 部分，
`PackingParams` 中 `target_diameter_distribution_csv`、`target_mean_sphericity`、
`mean_sphericity_tolerance` 各行）。

## 运行方式

```bash
./target/release/rustmspt pack --config /tmp/rustmspt-doc-examples/pack_target_distribution.yaml
```

## 预期输出

上述运行的真实捕获 stdout（`real 0m0.235s`）：

```
[Info] Target diameter distribution: 25 bins from 'data/input/gu2019_fig7b_pore_distribution.csv'
[Info] Target mean sphericity: 0.820000 +/- 0.020000
[Info] Packing input mode: preloaded single STL (14 candidate particles)
[Info] CPU setting: cpu_max=-1 -> using 8 worker threads (available 8).
[Info] Rayon pool threads (effective): 8
[Info] Rotation mode: none
[Info] Packing completed.
[Info] Final count: 44
[Info] Final volume fraction: 0.030078
[Info] Diameter distribution comparison CSV: /tmp/rustmspt-doc-examples/pack_target_distribution_result_diameter_distribution.csv
[Info] Diameter distribution bins: left,right,target,ideal_count,actual_count,actual,error,attempts
[Info] Diameter bin 0: 5.000000,6.000000,0.12328767,5.425,5,0.11363636,-0.00965131,6
[Info] Diameter bin 1: 6.000000,7.000000,0.12924360,5.687,6,0.13636364,+0.00712004,6
[Info] Diameter bin 2: 7.000000,8.000000,0.11673615,5.136,5,0.11363636,-0.00309979,8
[Info] Diameter bin 3: 8.000000,9.000000,0.10422871,4.586,5,0.11363636,+0.00940766,8
[Info] Diameter bin 4: 9.000000,10.000000,0.10780226,4.743,5,0.11363636,+0.00583410,7
[Info] Diameter bin 5: 10.000000,11.000000,0.08814771,3.878,4,0.09090909,+0.00276138,4
[Info] Diameter bin 6: 11.000000,12.000000,0.06491959,2.856,3,0.06818182,+0.00326222,6
[Info] Diameter bin 7: 12.000000,13.000000,0.04883859,2.149,2,0.04545455,-0.00338405,3
[Info] Diameter bin 8: 13.000000,14.000000,0.03692674,1.625,2,0.04545455,+0.00852780,4
[Info] Diameter bin 9: 14.000000,15.000000,0.03633115,1.599,2,0.04545455,+0.00912340,2
[Info] Diameter bin 10: 15.000000,16.000000,0.02918404,1.284,1,0.02272727,-0.00645677,1
[Info] Diameter bin 11: 16.000000,17.000000,0.01905896,0.839,1,0.02272727,+0.00366831,1
[Info] Diameter bin 12: 17.000000,18.000000,0.01548541,0.681,1,0.02272727,+0.00724186,2
[Info] Diameter bin 13: 18.000000,19.000000,0.01727219,0.760,1,0.02272727,+0.00545509,1
[Info] Diameter bin 14: 19.000000,20.000000,0.01131626,0.498,1,0.02272727,+0.01141101,1
[Info] Diameter bin 15: 20.000000,21.000000,0.00952948,0.419,0,0.00000000,-0.00952948,0
[Info] Diameter bin 16: 21.000000,22.000000,0.01012507,0.446,0,0.00000000,-0.01012507,0
[Info] Diameter bin 17: 22.000000,23.000000,0.00595593,0.262,0,0.00000000,-0.00595593,0
[Info] Diameter bin 18: 23.000000,24.000000,0.00536033,0.236,0,0.00000000,-0.00536033,0
[Info] Diameter bin 19: 24.000000,25.000000,0.00536033,0.236,0,0.00000000,-0.00536033,0
[Info] Diameter bin 20: 25.000000,26.000000,0.00416915,0.183,0,0.00000000,-0.00416915,0
[Info] Diameter bin 21: 26.000000,27.000000,0.00476474,0.210,0,0.00000000,-0.00476474,0
[Info] Diameter bin 22: 27.000000,28.000000,0.00238237,0.105,0,0.00000000,-0.00238237,0
[Info] Diameter bin 23: 28.000000,29.000000,0.00178678,0.079,0,0.00000000,-0.00178678,0
[Info] Diameter bin 24: 29.000000,30.000000,0.00178678,0.079,0,0.00000000,-0.00178678,0
[Info] Diameter distribution error: max_abs 0.01141101, total_variation 0.07381288, integer_rounding_max 0.01141101
[Info] Placement target kinds: natural 3, scaled 41, fallback 0
[Info] Scale factors: min 0.488649, mean 1.317943, max 3.683837
[Info] Final mean sphericity: 0.918922
[Info] Mean sphericity error: 0.098922
[Info] Mean sphericity tolerance met: false
[Warning] Target mean sphericity not reached; volume fraction was prioritized.
[Info] Orientation fix enabled: false
```

在最终体积分数 `0.030078`（刚好超过 `0.03` 目标）时，共放置了 44 个颗粒。
其中，3 个以其**自然（natural）**尺寸使用（它们未缩放的等体积直径恰好落在
`choose_bin` 为其选择的区间内），41 个被**缩放（scaled）**（通过
`scale_mesh_to_equivalent_diameter` 重缩放至目标区间的中点），0 个回退
（fallback）到宽松的区间选择——本次运行在这一较小的体积分数和区间数下
从未需要 `Fallback` 放置。所应用的缩放系数从 `0.488649`（候选颗粒缩小到约一半大小）
到 `3.683837`（几乎放大四倍）不等，平均为 `1.317943`。

直径 20 以上的区间（区间 15–24）实际计数均为 0，尽管目标频率非零，
且区间 15–19 的尝试次数也非零——44 个颗粒不足以填满这个分布上尾概率
质量极小的全部 25 个区间（例如区间 24 的目标频率为 `0.00178678`，
即低于 0.2%）。报告的 `max_abs 0.01141101` 误差恰好等于
`integer_rounding_max 0.01141101`——即把恰好 44 个离散颗粒装入这个直方图
形状的理论最佳可能误差——这证实了区间欠账算法触及了取整下限，
而没有留下可避免的余量。

球形度引导未达到其 `0.82 ± 0.02` 的目标：最终按计数加权的平均球形度为
`0.918922`（`data/input/particles.stl` 中的颗粒显然比所要求的目标更圆），
误差为 `0.098922`，超出容差范围，因而出现了 `[Warning]` 行。这是预期的、
有文档记录的行为——球形度引导是软性的，每次尝试仅在最多 4 个候选抽样中
进行排序；它绝不会直接拒绝某个候选，体积分数/粒径分箱目标始终优先
（见"说明"部分）。

### `pack_target_distribution_result_diameter_distribution.csv` 的真实内容

这是该功能的关键产出物：`write_distribution_comparison_csv` 在已堆积的 STL
旁边写出 `<output_stem>_diameter_distribution.csv`。以下是本次运行的完整真实内容：

```
bin,right,target_frequency,target_count,actual_count,actual_frequency,frequency_error,count_error,attempts
5.000000000000,6.000000000000,0.123287671233,5.424657534247,5,0.113636363636,-0.009651307597,-0.424657534247,6
6.000000000000,7.000000000000,0.129243597379,5.686718284693,6,0.136363636364,0.007120038984,0.313281715307,6
7.000000000000,8.000000000000,0.116736152472,5.136390708755,5,0.113636363636,-0.003099788835,-0.136390708755,8
8.000000000000,9.000000000000,0.104228707564,4.586063132817,5,0.113636363636,0.009407656072,0.413936867183,8
9.000000000000,10.000000000000,0.107802263252,4.743299583085,5,0.113636363636,0.005834100384,0.256700416915,7
10.000000000000,11.000000000000,0.088147706968,3.878499106611,4,0.090909090909,0.002761383941,0.121500893389,4
11.000000000000,12.000000000000,0.064919594997,2.856462179869,3,0.068181818182,0.003262223185,0.143537820131,6
12.000000000000,13.000000000000,0.048838594401,2.148898153663,2,0.045454545455,-0.003384048947,-0.148898153663,3
13.000000000000,14.000000000000,0.036926742108,1.624776652770,2,0.045454545455,0.008527803346,0.375223347230,4
14.000000000000,15.000000000000,0.036331149494,1.598570577725,2,0.045454545455,0.009123395961,0.401429422275,2
15.000000000000,16.000000000000,0.029184038118,1.284097677189,1,0.022727272727,-0.006456765391,-0.284097677189,1
16.000000000000,17.000000000000,0.019058963669,0.838594401429,1,0.022727272727,0.003668309058,0.161405598571,1
17.000000000000,18.000000000000,0.015485407981,0.681357951161,1,0.022727272727,0.007241864746,0.318642048839,2
18.000000000000,19.000000000000,0.017272185825,0.759976176295,1,0.022727272727,0.005455086902,0.240023823705,1
19.000000000000,20.000000000000,0.011316259678,0.497915425849,1,0.022727272727,0.011411013049,0.502084574151,1
20.000000000000,21.000000000000,0.009529481834,0.419297200715,0,0.000000000000,-0.009529481834,-0.419297200715,0
21.000000000000,22.000000000000,0.010125074449,0.445503275759,0,0.000000000000,-0.010125074449,-0.445503275759,0
22.000000000000,23.000000000000,0.005955926147,0.262060750447,0,0.000000000000,-0.005955926147,-0.262060750447,0
23.000000000000,24.000000000000,0.005360333532,0.235854675402,0,0.000000000000,-0.005360333532,-0.235854675402,0
24.000000000000,25.000000000000,0.005360333532,0.235854675402,0,0.000000000000,-0.005360333532,-0.235854675402,0
25.000000000000,26.000000000000,0.004169148303,0.183442525313,0,0.000000000000,-0.004169148303,-0.183442525313,0
26.000000000000,27.000000000000,0.004764740917,0.209648600357,0,0.000000000000,-0.004764740917,-0.209648600357,0
27.000000000000,28.000000000000,0.002382370459,0.104824300179,0,0.000000000000,-0.002382370459,-0.104824300179,0
28.000000000000,29.000000000000,0.001786777844,0.078618225134,0,0.000000000000,-0.001786777844,-0.078618225134,0
29.000000000000,30.000000000000,0.001786777844,0.078618225134,0,0.000000000000,-0.001786777844,-0.078618225134,0
```

每一行对应一条 `[Info] Diameter bin N` 的 stdout 输出行，但采用完整的 `f64` 精度，
而非控制台上截断显示的 `.12328767`；`target_count` 和 `count_error` 是未取整的
（小数形式的）理想计数及其实际值与理想值之差，对于会丢失整数 stdout 计数精度的
下游分析而言非常有用。

输出的 STL 文件 `/tmp/rustmspt-doc-examples/pack_target_distribution_result.stl`
是一个真实的二进制 STL（841,584 字节，16,830 个三角形）——三角形数量比普通堆积
示例更少，尽管目标体积分数更高（`0.03` 对比 `0.02`），原因是这里许多候选颗粒
被向下重缩放到了较小的目标区间（最小缩放系数 `0.488649`），而不是保持自然尺寸。

## 已知良好的健全性检查（来自 `tests/pack_target_tests.rs`）

`tests/pack_target_tests.rs` 没有从这次具体的运行重新推导预期值，而是为同一算法
提供了确定性的、可手工核对的固定测试用例。例如，`strict_choice_follows_largest_deficit`
断言：对于频率为 `[0.5, 0.3, 0.2]` 的合成三区间分布，从空状态开始的十次连续
`choose_bin`+`record_success` 调用，会按精确顺序 `[0, 1, 2, 0, 0, 1, 0, 2, 1, 0]`
选择区间，最终接受计数为 `state.counts == vec![5, 3, 2]`——正是十个颗粒下
频率所暗示的精确 5:3:2 比例——并且所得的 `summary.max_absolute_error <=
summary.rounding_max_absolute_error + 1e-12`，即区间欠账算法的表现不可能优于
整数取整下限，而在此案例中恰好精确达到该下限。这次实际运行的 stdout 行
（`max_abs 0.01141101` 对比 `integer_rounding_max 0.01141101`——两者相等）
独立地在 44 个真实放置的颗粒上重现了同样的"`max_absolute_error` 处于或低于
取整下限"的关系，而不仅仅是在合成固定测试用例上。

同一测试文件还针对真实内置的 CSV 进行了断言（`example_paper_distribution_is_valid_and_normalized`），
即 `data/input/gu2019_fig7b_pore_distribution.csv` 解析为恰好 25 个覆盖 `[5, 30]`
的区间，频率之和在 `1e-12` 以内等于 `1.0`——这与本演示实际运行的
`[Info] Target diameter distribution: 25 bins` 一行所确认的是同一份文件、
同样的 25 区间结构。

## 说明

- 目标分布引导不会取代或覆盖 `target_volume_fraction`——它是叠加在其上的次要、
  尽力而为（best-effort）目标。当某个目标区间在 `TARGET_BIN_PROBES = 4` 次失败
  探测后仍无法填充时，流水线会回退到欠账/TVD 得分次优的区间，而不是停滞不前；
  上述运行的 `fallback 0` 意味着它在 44 次放置中从未需要触发这一升级机制，
  但更大规模的运行或更棘手的候选库通常会用到它。
- `filters.min_volume` 是针对重缩放**之后**的候选体积进行检查，而不是自然体积——
  一个在原始尺寸下能通过过滤器的候选颗粒，在被缩放到较小的目标区间后仍可能被拒绝，
  反之亦然。
- 球形度引导（`target_mean_sphericity`/`mean_sphericity_tolerance`）是软性的：
  它会按预计的运行均值误差对每次尝试中最多 4 个候选抽样进行排序，但绝不会因为
  超出范围而拒绝某个候选，运行结束时未达到容差带也只会产生一条 `[Warning]`，
  而不是硬性失败——这正是本次运行所展示的情形（均值 `0.918922` 对比
  `0.82 ± 0.02` 目标，依然成功完成）。
- 只有成功的放置才会改变 `DistributionState`/`SphericityState`——失败的尝试
  仍会递增每个区间的 `attempts` 计数器（在 stdout 表格和 CSV 的 `attempts`
  列中均可见），但绝不会递增 `counts`，这就是为什么每个区间的 `attempts`
  可能超过 `actual_count`（例如区间 3：8 次尝试中接受了 5 次）。
- 完整的区间欠账/TVD 回退推导、`Natural`/`Scaled`/`Fallback` 分类规则，以及
  `PackPipeline::run` 中的重试升级状态机，见
  [`../algorithms/packing-target-diameter-distribution.md`](../algorithms/packing-target-diameter-distribution.md)；
  `src/pipeline/pack_targets.rs` 和 `src/pipeline/pack.rs` 中每个函数的 API 参考，
  见 [`../reference/pipeline-packing.md`](../reference/pipeline-packing.md)；
  `mesh_metrics` 和 `scale_mesh_to_equivalent_diameter`（本功能所基于的等体积直径/
  球形度/重缩放基础操作），见 [`../reference/geometry-analysis.md`](../reference/geometry-analysis.md)。
