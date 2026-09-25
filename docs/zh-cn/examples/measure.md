# `measure` 流水线示例

## 功能说明

`measure` 通过计算固相体积分数 (VF) 及其两点相关函数 (S2) 曲线来表征一个已堆积的微结构网格——
即在一个选定的包围盒范围内，两个相距 `r` 的点都落在固体材料中的概率，可使用精确的体素网格
方法、蒙特卡洛估计，或两者兼用来计算。结果（VF、所用后端以及完整的 S2 序列）会打印到
stdout，并写入一份文本报告。光线投射/体素化方法的完整推导，以及精确方法与蒙特卡洛方法之间
的权衡，见 `../algorithms/s2-two-point-correlation.md`。

## 配置

`data/input/measure_config.yaml`（该流水线在仓库中真实的默认配置）：

```yaml
measurement:
  stl_path: "data/output/optimized_structure.stl"
  stl_bounding_box: [0.0, 0.0, 0.0, 50.0, 50.0, 50.0] # Optional: [min_x, min_y, min_z, max_x, max_y, max_z]
  r_max: 20
  voxel_pitch: 1.0 # Larger pitch = faster but less accurate
  mc_method: 'both' # 'monte_carlo' or 'exact' or 'both'
  mc_samples: 40_000 # Samples for Monte Carlo estimation
  cpu_max: -1 # -1 uses all available cores
  output_path: "data/output/measured_s2.txt"
  # Acceleration: auto selects GPU when available and workload is large enough
  acceleration:
    mode: auto # auto | cpu | gpu
```

与其他两个示例一样，默认配置中的 `stl_path` 指向 `data/output/optimized_structure.stl`
（`optimize` 的预期输出），而这在全新检出的仓库中并不存在。本示例改为通过 `--input` 对真实
存在的 `data/output/packed_result.stl` 进行测量，并通过 `--output`（会覆盖
`measurement.output_path`）将报告写入一个临时路径。

| 字段 | 含义 |
|---|---|
| `stl_bounding_box` / `bounding_box` | 测量所覆盖的区域。优先级为 `bounding_box`（显式指定）> `stl_bounding_box` > 由网格推导的包围盒 > 单位立方体兜底值。此处将测量限制在已堆积域中 `[0,0,0]`–`[50,50,50]` 的角落区域，而不是整个约 105 单位大小的网格。 |
| `r_max` | S2 曲线计算所覆盖的最大相关半径（以体素为单位）；会产生 `r_max + 1` 个数据点（`r = 0..=r_max`）。 |
| `voxel_pitch` | 用于离散化的体素边长；数值越小则越精细/越慢。在 `50x50x50` 的区域上取 `1.0` 会得到 125,000 个体素。 |
| `mc_method` | `"exact"`（体素网格光线投射）、`"monte_carlo"`（随机采样），或 `"both"`（两者都运行并报告其 L2 距离）。`"exact"` 从不降级：当没有 CPU exact 内核的工作集能满足 768 MiB 预算时直接报错（见 stdout 中的 `[Info] CPU exact working-set plan`）。 |
| `mc_samples` | 每个半径下的蒙特卡洛采样点数；接受带下划线分隔的字面量，如 `40_000`。 |
| `cpu_max` | 专用 Rayon 线程池的线程数；`-1` 表示使用所有可用核心。 |
| `acceleration.mode` | `auto`/`cpu`/`gpu`。`auto` 仅在工作负载（体素数量）超过 `gpu_min_voxels`（默认 250,000）时才选择 GPU——低于此阈值时会静默回退到 CPU，正如本次运行中 125,000 个体素的工作负载所示。 |

完整字段参考见 `../reference/config.md`（`measurement.rs` 部分：`MeasurementParams`、
`MeasurementConfig`），准确的后端选择与包围盒解析逻辑见 `../reference/pipeline-core.md` 中的
`MeasurePipeline::run` 条目。

## 运行方式

```bash
./target/release/rustmspt measure \
  --config data/input/measure_config.yaml \
  --input data/output/packed_result.stl \
  --output /tmp/rustmspt-doc-examples/measured_s2.txt
```

## 预期输出

上述运行中真实捕获的 stdout：

```
[Info] CPU setting: cpu_max=-1 -> using 8 worker threads (available 8).
[Info] Rayon pool threads (effective): 8
[Info] STL file(s) loaded from: data/output/packed_result.stl
[Info] Measurement bbox: min=(0.0000,0.0000,0.0000), max=(50.0000,50.0000,50.0000)
[Info] S2 config: method=both, r_max=20, mc_samples=40000, voxel_pitch=1.000000
[Info] Acceleration: requested=auto, effective=cpu
[Info] Acceleration fallback: workload 125000 voxels below gpu_min_voxels threshold 250000
[Info] Measurement completed.
[Info] Volume fraction: 0.024314
[Info] VF detail: particles=31 (in-box clipped-volume sum)
[Info] Compute backend: cpu
[Info] Method: both
[Info] S2(0)-VF diff [exact]: 0.000062
[Info] S2(0)-VF diff [monte_carlo]: 0.000062
[Info] L2 error [exact vs monte_carlo]: 0.001406
[Info] S2 points [exact]: 21
[Info] S2 points [monte_carlo]: 21
[Info] Output written: /tmp/rustmspt-doc-examples/measured_s2.txt
```

`/tmp/rustmspt-doc-examples/measured_s2.txt` 的真实内容：

```
Volume Fraction: 0.024314
Compute Backend: cpu
Method: both
S2(0)-VF diff [exact]: 0.000062
S2(0)-VF diff [monte_carlo]: 0.000062
L2 error [exact vs monte_carlo]: 0.001406
S2 Values [exact]:
0: 0.024376
1: 0.020591
2: 0.017944
3: 0.015522
4: 0.013235
5: 0.010934
6: 0.008934
7: 0.007353
8: 0.005984
9: 0.004693
10: 0.003537
11: 0.002585
12: 0.001797
13: 0.001115
14: 0.000658
15: 0.000450
16: 0.000411
17: 0.000412
18: 0.000427
19: 0.000441
20: 0.000448
S2 Values [monte_carlo]:
0: 0.024376
1: 0.021255
2: 0.017952
3: 0.015765
4: 0.012640
5: 0.011458
6: 0.008620
7: 0.007115
8: 0.005997
9: 0.004140
10: 0.003350
11: 0.002259
12: 0.002145
13: 0.001226
14: 0.000751
15: 0.000457
16: 0.000258
17: 0.000178
18: 0.000418
19: 0.000485
20: 0.000608
```

此次运行耗时 `real 0m0.093s`——在 `voxel_pitch: 1.0` 下，50x50x50 的 ROI 只有 125,000 个
体素，远在精确方法 768 MiB 工作集预算之内，也低于 250,000 体素的 GPU 阈值，因此在
CPU 上完全以“精确”方式运行，同时并行完成了 40,000 采样的蒙特卡洛估计。

## 说明

- `S2(0)`（`0.024376`）比报告的整体运行体积分数（`0.024314`）略高 `0.000062`，两种方法都是
  如此——这就是“S2(0)-VF diff”这一行，属于预期现象：`S2(0)` 是根据体素占用网格计算得出的
  （`voxel_pitch: 1.0` 的离散化结果），而 VF 是对与 ROI 相交的 31 个颗粒计算出的精确
  区域内裁剪体积之和，因此两者之间存在微小的量化差异是正常的，并非缺陷。
- 精确方法与蒙特卡洛方法的 S2 曲线在小 `r` 时吻合得很好，在大 `r` 时（相对而言）分歧更大，
  此时底层计数本身就很小——例如在 `r=17` 处精确方法给出 `0.000412`，而蒙特卡洛方法给出
  `0.000178`，相对差异超过 2 倍，尽管两者的绝对值都很小。报告中的
  `L2 error [exact vs monte_carlo]: 0.001406` 概括了整条曲线上的这一差异。将 `mc_samples`
  大幅提高到远超 `40_000` 的水平，会以成比例的成本收紧蒙特卡洛曲线与精确曲线的吻合度；
  两种方法之间精度/成本的完整权衡见 `../algorithms/s2-two-point-correlation.md`。
- 由于所测量的区域（125,000 个体素）远低于 `gpu_min_voxels: 250_000`，即使
  `acceleration.mode: auto`，即使在拥有可正常工作的 GPU 后端的机器上，这份特定配置也始终会
  在 CPU 上运行——如果想实际走通 GPU 路径，需要扩大 ROI 或减小 `voxel_pitch` 以超过该阈值
  （见 `../reference/gpu.md`）。
- 单独的 `mc_method: 'exact'`（或作为 `'both'` 的一部分）从不被蒙特卡洛替换。体素化之前，
  管线打印 `[Info] CPU exact working-set plan: ...`，给出 FFT/直接法的选择、模型时间与工作集；
  若没有内核满足 768 MiB 预算（例如包围盒大得多，或 `voxel_pitch` 精细得多），运行会明确报错。
  输出数值与所选内核无关。
