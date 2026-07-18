# `pack` 流水线示例

## 功能说明

`pack` 将候选颗粒网格随机顺序堆积到一个矩形箱体中：它反复提议一个候选颗粒（可选旋转），
针对几何过滤器、边界域规则、成对碰撞、最小相邻距离，以及（在周期模式下）镜像碰撞对其进行检验，
并在每次尝试中接受第一个通过全部检查的候选——直到达到 `target_volume_fraction` 或
`max_attempts` 耗尽为止。本示例运行的是**普通（plain）**模式：不进行目标粒径分布引导，
只是"用任意出现的候选形状把箱体填充到 X% 的固体体积分数"。关于分布引导变体，见
[`pack-target-distribution.md`](pack-target-distribution.md)。

## 配置

仓库中的真实默认配置 `data/input/pack_config.yaml` 是一个目标分布配置（它设置了
`target_diameter_distribution_csv`）。对于本篇普通堆积的演示，该字段被注释掉，
并且输出路径被重定向到了一个临时（scratch）位置；其余内容——包括真实默认值
`target_volume_fraction: 0.02`——保持不变：

```yaml
# Input settings
input:
  # Path to a single STL or a folder of STLs
  path: "data/input/particles.stl"

# Output settings
output:
  path: "/tmp/rustmspt-doc-examples/pack_plain_result.stl"

# Box dimensions (The container)
box:
  dimensions: [100.0, 100.0, 100.0] # X, Y, Z size

# Packing algorithm settings
packing:
  # Target Volume Fraction (0.0 to 1.0). E.g., 0.3 means 30% filled.
  target_volume_fraction: 0.02

  # Mode 1: Strict (No boundary crossing)
  # Mode 2: Loose (Boundary crossing allowed, ignore walls)
  # Mode 3: Periodic (Boundary crossing allowed + Periodic collision checks)
  mode: 2

  # Max attempts to place a particle before giving up (prevents infinite loops)
  max_attempts: 2000

  # Constraint: Minimum distance between particles (0.0 to allow touching)
  min_neighbor_distance: 1.0

  # Rotation constraint for candidate particle placement
  # Options: 'none', 'x', 'y', 'z', 'vector', 'any'
  # 'vector' uses rotation_axis_vector as axis direction and rotates around particle centroid.
  rotation_mode: 'none'
  rotation_axis_vector: [0.0, 0.0, 1.0]

  # d_1: Distance for fully internal particles
  min_boundary_dist: 1.5
  # d_2: Minimum protrusion/retention depth for crossing particles
  min_cross_boundary_depth: 3.0

  # CPU worker limit for packing collision/distance checks
  # -1 means no limit (use all available cores)
  cpu_max: -1

  # Orientation fix before final output (can be expensive)
  # false by default to skip; set true to enforce positive signed volume orientation.
  orient_to_positive_volume: false

  # target_diameter_distribution_csv: "data/input/gu2019_fig7b_pore_distribution.csv"

  # Geometry filters.
  filters:
    min_volume: 15.0
    max_aspect_ratio: 3.0    # Longest side / Shortest side
    max_sharpness_ratio: 2.0  # (Area^3) / (36*pi*Volume^2). Sphere=1.0. Higher = Sharper/Irregular.
```

`input.path` 指向 `data/input/particles.stl`，这是一个作为候选颗粒形状小型库的单个 STL 文件：
`PackPipeline` 会预先加载它，并在堆积开始前报告其中包含多少个独立的实体壳（候选颗粒）
（本次运行中为 `14` 个——见下方 stdout）。每次放置尝试都会从这 14 个候选中抽取一个，
应用配置的 `rotation_mode`，然后尝试放置它。

| 字段 | 含义 |
|---|---|
| `target_volume_fraction` | 堆积应达到的固体体积分数（本例中 `0.02` = 填充 2%——刻意设得很小，以便本演示能在远低于一秒的时间内完成；生产环境的运行通常以更高的分数为目标，耗时也会相应增加）。 |
| `mode` | `1` 严格（不允许越界），`2` 宽松（允许越界，忽略墙体），`3` 周期（允许越界 + 周期镜像碰撞检查）。本示例使用 `2`。 |
| `max_attempts` | 在流水线放弃并报告已达到的体积分数之前，允许的放置尝试次数。 |
| `min_neighbor_distance` | 已放置颗粒表面之间允许的最小间隙。 |
| `rotation_mode` / `rotation_axis_vector` | 控制放置前候选颗粒的朝向；此处的 `'none'` 意味着每个候选都保持其加载时的原始朝向。 |
| `min_boundary_dist` / `min_cross_boundary_depth` | 宽松/周期模式下使用的越界容差（`d_1`/`d_2`）。 |
| `cpu_max` | 碰撞/距离检查的工作线程数上限；`-1` 表示使用所有可用核心。 |
| `orient_to_positive_volume` | 可选的堆积后朝向修正步骤（开销较大；默认关闭）。 |
| `filters` | 每个候选颗粒在被尝试放置前应用的几何门控（最小体积、最大长宽比、最大尖锐度比）。 |

完整字段参考见 [`../reference/config.md`](../reference/config.md)（`packing.rs` 部分：`PackingFilters`、
`PackingParams`、`PackingConfig`），包括在本普通示例中未设置/注释掉的目标分布字段
（`target_diameter_distribution_csv`、`target_mean_sphericity`、`mean_sphericity_tolerance`）。

## 运行方式

```bash
./target/release/rustmspt pack --config /tmp/rustmspt-doc-examples/pack_plain.yaml
```

（`/tmp/rustmspt-doc-examples/pack_plain.yaml` 是上文所示 `data/input/pack_config.yaml` 的临时副本；
仓库自身的 `data/input/pack_config.yaml` 保持不变。）

## 预期输出

上述运行的真实捕获 stdout：

```
[Info] Packing input mode: preloaded single STL (14 candidate particles)
[Info] CPU setting: cpu_max=-1 -> using 8 worker threads (available 8).
[Info] Rayon pool threads (effective): 8
[Info] Rotation mode: none
[Info] Packing completed.
[Info] Final count: 63
[Info] Final volume fraction: 0.020161
[Info] Orientation fix enabled: false
```

该运行在 `real 0m0.290s` 内完成，放置了 63 个颗粒，最终体积分数为 `0.020161`——
略超过 `0.02` 的目标值，因为流水线一旦某次放置使运行中的体积分数达到或超过目标值就会停止，
而不会尝试精确落在目标值上。

输出的 STL 文件 `/tmp/rustmspt-doc-examples/pack_plain_result.stl` 是一个真实的二进制 STL 文件
（磁盘上 1,195,884 字节），包含 23,916 个三角形——即全部 63 个已放置颗粒实例的并集，
每个实例都是 `data/input/particles.stl` 中 14 个候选壳之一的旋转/平移副本。
本模式下没有配套的 CSV 报告：`<output_stem>_diameter_distribution.csv` 报告仅在设置了
`target_diameter_distribution_csv` 时才会生成（见目标分布示例）。

## 说明

- 由于未设置 `target_diameter_distribution_csv`，候选颗粒按其自然大小使用——
  没有粒径分箱引导或重缩放步骤，已放置颗粒的尺寸分布完全取决于
  `data/input/particles.stl` 的 14 个候选形状在随机选取和旋转下恰好产生的结果。
- `filters.min_volume`（此处为 `15.0`）在普通模式下依然生效：小于该值的候选颗粒
  在任何放置几何检查运行之前就会被拒绝，与目标体积分数无关。
- 随着 `target_volume_fraction` 增大，尝试次数和运行时间会超线性增长，
  因为箱体逐渐填满，无碰撞空间变得稀缺；此处选择 `0.02` 正是为了让本演示保持快速
  （远低于一秒）。关于每次尝试的确切检查顺序（过滤器 -> 边界 -> 碰撞 -> 相邻距离 ->
  模式 3 下的周期镜像检查），见 [`../reference/pipeline-packing.md`](../reference/pipeline-packing.md)
  （`PackPipeline::run`）。
- 对于相同的输入库和箱体，将已放置颗粒的*尺寸*引导至一个测量得到的粒径分布，
  而不是原样接受，这一内容在 [`pack-target-distribution.md`](pack-target-distribution.md) 和
  [`../algorithms/packing-target-diameter-distribution.md`](../algorithms/packing-target-diameter-distribution.md)
  中有介绍。
