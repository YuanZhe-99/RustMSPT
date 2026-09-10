# 围绕冻结孔隙的 `pack`

## 功能说明

把颗粒放入已含孔隙的域的固相区域，并让每一颗粒与孔面保持规定的间隙。孔隙是**冻结**的：绝不被填补、
移动或清理；它会被逐字节复制到输出旁边，报告还记录其前后摘要，因此读者可以自行核对，而不必只是听信。

同一引擎在没有孔隙时的用法见 [`pack-placement.md`](pack-placement.md)，那里更详细地讲了记录、路径
契约与可复现性。本页讲的是孔隙额外带来的东西。

## 配置

仓库中已提交的配置 `data/input/placement_void_config.yaml`，原文如下：

```yaml
# Placement around a frozen void.
#
# The void is never filled, moved or cleaned. It is copied byte for byte beside
# the outputs, and the report records its digest before and after, so the freeze
# is something a reader can check rather than take on trust.
#
# Every relative path below resolves against the directory holding THIS file.
placement:
  seed: 20260910

  frame:
    unit: "um"

  domain:
    min: [0, 0, 0]
    max: [100, 100, 100]

  shapes:
    files: ["particles.stl"]
    filters:
      max_aspect_ratio: 3.0

  void:
    # Three spherical pores. Regenerate with:
    #   python3 data/fixtures/placement/make_void.py data/input/placement/void_spheres.stl
    file: "placement/void_spheres.stl"
    # `forbidden`: the whole particle keeps `gap` from the void surface.
    # `allowed`: a particle may reach in, and the overlap is measured and owned
    # by the void, which then needs overlap_volume.voxel_size.
    crossing: forbidden
    # g_pv. Must be greater than zero when crossing is forbidden: two shapes
    # that intersect report distance 0.0, so a zero gap forbids nothing.
    gap: 2.0

  size:
    distribution:
      kind: lognormal
      median: 10.0
      sigma_log: 0.3
      min: 5.0
      max: 20.0
    classes: { kind: equal_width, count: 5 }

  orientation: { mode: uniform_so3 }
  gaps: { particle_particle: 1.0 }

  target:
    volume_fraction: 0.12
    # The domain minus the void, which is what a solid-phase fraction is
    # normally quoted against.
    basis: solid

  budget:
    attempts_per_particle: 3000
    total_attempts: 2000000

  outputs:
    dir: "../output/placement_void"
    # Uncomment to also write the three-phase label field and per-voxel ids.
    # voxel_labels: { voxel_size: 1.0 }
```

孔隙 fixture 由一个仅用标准库、带 `--check` 模式的脚本生成：

```bash
python3 data/fixtures/placement/make_void.py data/input/placement/void_spheres.stl --check
```

它刻意不使用本仓库自己的 STL 读取器编写。用读取它的同一份代码去生成 fixture，对二者都证明不了什么。
之所以选球体，是因为两个球之间的距离、以及它们交集的体积，都有闭式解，于是测试可以拿引擎与算术核对，
而不是与本 crate 自己的另一个函数核对。

## 运行方式

```bash
./target/release/rustmspt pack --config data/input/placement_void_config.yaml
```

## 预期输出

真实 stdout：

```
[Info] Placement seed 20260910 (chacha12), -1 thread setting
[Info] Placed 174 particle(s); volume fraction 0.118939 of the solid basis (target 0.120000)
[Info] Attempts 571 of a 2000000 budget
[Info] Stop reason: target_reached
[Info] every planned size was placed and the target volume fraction was reached
[Info] Wrote data/input/../output/placement_void
```

该次运行耗时 `real 0m0.806s`，向 `data/output/placement_void/` 写出五个文件。第五个是
`void_spheres.stl`，即被原样复制的冻结孔隙。

## 报告如何描述孔隙

取自 `data/output/placement_void/run_report.json`：

```json
  "void": {
    "path": "placement/void_spheres.stl",
    "sha256_in": "2ffb2938ce6a23fcba8045abd915c2c738fd7e759538c929fdc77474aafef392",
    "sha256_out": "2ffb2938ce6a23fcba8045abd915c2c738fd7e759538c929fdc77474aafef392",
    "shells": 3,
    "orientation": "outward",
    "crossing": "forbidden",
    "gap": 2.0,
    "volume_total": 5212.543167292105,
    "volume_in_domain": 5212.543167292105,
    "volume_method": "exact_shell_sum",
    "inside_domain": true,
    "overlap_voxel_size": null,
    "overlap_owner": "void"
  },
  "target": {
    "volume_fraction": 0.12,
    "basis": "solid",
    "basis_volume": 994787.4568327079,
    "tolerance": 0.01,
    "distribution": "lognormal(median=10, sigma_log=0.3, min=5, max=20)"
  },
```

`sha256_in` 与 `sha256_out` 相等。这就是"冻结"，并且被表述成可核对的形式：出去的文件就是进来的文件。

`volume_method` 为 `exact_shell_sum`，因为孔隙整体位于域内，其三个壳的体积精确相加。若孔隙跨越域边界，
则会报告 `exact_clip`。所用方法一律说明而非假定，因为 `basis: solid` 意味着*域减去这个数*，因此这个数
如何得来，改变了目标体积分数的含义：`basis_volume` 为 `1000000 - 5212.54 = 994787.46`。

`orientation: outward` 是检查出来的，不是假定的。壳与壳朝向不一致的孔隙——有的外向、有的内向——会在
**加载时被拒绝**，因为 `mesh_volume` 取整个和的绝对值：两个孔中若有一个反向，报出的将是二者体积之差
而非之和，据此算出的固相基准就会悄然出错。全部内向的孔隙则被接受，并报告为 `inward`。

## 拒绝计数，以及它们说明了什么

```json
  "rejections": {
    "boundary_depth": 0,
    "inside_void": 24,
    "neighbourhood_band": 0,
    "outside_domain": 0,
    "particle_gap": 76,
    "particle_overlap": 288,
    "void_enclosed": 0,
    "void_gap": 9,
    "zero_in_domain_volume": 0
  },
```

每个原因都有键位，哪怕计数为零，于是这份统计读起来像一个漏斗，而不是一堆互不相干的计数。这里有 24 个
候选的中心落在孔内，9 个比 2.0 的间隙更靠近孔面，其余都被别的颗粒挡下。`void_enclosed` 为零：没有任何
候选颗粒大到足以吞下一整个孔，在这个尺寸范围下这正是意料之中的。

## 尺寸表，以及那两个补抽颗粒

`data/output/placement_void/size_distribution.csv` 的真实内容：

```
class,lo,hi,target_frequency,target_count,drawn,placed,shortfall,top_up_drawn,top_up_placed
0,5.000000000000,8.000000000000,0.222710611771,38,45,45,0,1,1
1,8.000000000000,11.000000000000,0.404588947685,70,67,67,0,1,1
2,11.000000000000,14.000000000000,0.249539902406,43,46,46,0,0,0
3,14.000000000000,17.000000000000,0.094526817981,16,10,10,0,0,0
4,17.000000000000,20.000000000000,0.028633720156,5,4,4,0,0,0
```

每个分组的 `shortfall` 都是零：这次运行规划的一切都放置成功了。报告中 `plan.planned_particles: 172`
而 `actual.particles: 174`，多出来的两个正是上表中的 `top_up_placed`。

这就是补抽规则在起作用，值得顺着看一遍。计划一直抽取尺寸，直到运行体积首次达到目标，然后在最后两个
数量中保留**更接近**目标的那个——这里留下了 `planned_volume_error: -1583.71`，约低 1.3%。因为**全部**
已规划尺寸都放置成功，缺口纯粹来自算术而非失败，所以允许补抽一批：又两个颗粒，记在它们自己的列里。

若有任何尺寸失败，则完全不会补抽。失败之后再补抽，会从同一分布中抽取、多数产出小颗粒，从而不声不响地
补上缺失的体积——这正是"先抽定整批"的设计所要防止的唯一一件事。

## 穿越孔隙

上面的 `crossing: forbidden` 让整个颗粒都避开孔。另一种选择是：

```yaml
  void:
    file: "placement/void_spheres.stl"
    crossing: allowed
    gap: 0.0
    overlap_volume: { voxel_size: 0.5 }
```

此时颗粒可以伸进孔里。每一颗粒的重叠量都在锚定于**域**原点的体素上度量——绝不锚定于颗粒自身的包围盒，
否则两个伸入同一个孔的颗粒会对同一个物理体素产生分歧——记录逐颗粒给出：

- `volume.in_domain` 是毛量：其中仍包含落在孔内的部分。
- `void_overlap_volume` 就是那一部分。
- `volume.in_domain_solid` 是前者减去后者，也是汇总为固相的那个数。

孔隙拥有这份重叠，记录在孔隙相上以 `overlap_owner: "void"` 写明。孔隙是冻结的，因此伸进去的颗粒并不会
把那部分体积从孔隙相中取走。

`voxel_size` 没有默认值。它同时是代价与精度的旋钮，如何取舍由调用方决定。

## 有意围绕某个孔放置颗粒

```yaml
  position:
    mode: void_neighbourhood
    band: [3.0, 7.0]
```

此时位置将按面积加权在孔面上抽取，并沿外法向偏移一个落在该带内的距离。这是为了在孔周围有意堆出许多
颗粒而设的构造，报告将其命名为 `void_neighbourhood_band`——绝不使用任何含"uniform"字样的名称。有测试
断言这一点，因为这个模式唯一不能做的事，就是被当成随机。

## 说明

- `crossing: forbidden` 时 `void.gap` 必须大于零。相交的两个形状返回的距离恰好是 `0.0`，于是
  `0.0 >= 0.0` 会通过，零间隙什么也禁止不了。配置会按名字拒绝它。
- `boundary.mode: periodic` 与孔隙同时出现会被拒绝：周期镜像的颗粒需要与冻结孔隙的周期镜像比对，而
  孔隙在域外的含义尚未确立。
- 完全够不到域的孔隙会被当作坐标系或单位错误拒绝，而不是当作空的孔隙网络。
- 加上 `outputs.voxel_labels: { voxel_size: 1.0 }` 会写出三相标签场（`0` 基体、`1` 颗粒、`2` 孔隙）
  与逐体素颗粒标识场，并附带记录体素尺寸与原点的头文件。孔隙拥有它所声明的每一个体素，且孔隙体素绝不
  携带颗粒标识。
- 完整判据，包括为何"孔隙没有顶点落在颗粒内部"是常被略去的那一条，见
  [`../algorithms/void-aware-placement.md`](../algorithms/void-aware-placement.md)。
