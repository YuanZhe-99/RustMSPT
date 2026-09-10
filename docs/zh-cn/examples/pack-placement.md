# 带 `placement:` 块的 `pack` —— 带种子、可复原的放置

## 功能说明

`pack` 会依据所给配置选择引擎。顶层的 `placement:` 块运行带种子、可复原、感知孔隙的引擎；`packing:`
块则运行原有的打包循环，未作改动。两者必须恰好出现其一，同时出现或都不出现的配置会被按名字拒绝。

本文走的是简单情形：空域，无孔隙。围绕冻结孔隙打包见 [`pack-void.md`](pack-void.md)；原引擎见
[`pack.md`](pack.md)。

有三点把本引擎与原引擎区分开来，本页余下内容也正是围绕它们展开：

- **运行可复现。** 相同的种子、输入与二进制，在任意线程数下放置出相同的颗粒。
- **每颗粒都有记录。** 一份逐颗粒 JSON 携带源壳、缩放、旋转与平移，因此任何颗粒都能仅凭记录重建。
- **运行会说明自己为何停止**，取自固定的四词词表，写在几何旁边的 JSON 报告里。

## 配置

仓库中已提交的配置 `data/input/placement_config.yaml`，原文如下：

```yaml
# Seeded, recorded placement into an empty domain.
#
# A `pack` config carries exactly one of `placement:` or `packing:`, and which
# one decides the engine. This selects the seeded, recorded, void-aware engine.
# For the original packing loop see pack_config.yaml, which is unchanged.
#
# Every relative path below resolves against the directory holding THIS file
# (data/input/), never against the working directory.
placement:
  # Required. The same seed, inputs and binary give the same placement, on any
  # thread count. Overridable with --seed.
  seed: 20260910

  # A label, copied into every output. Nothing scales by it.
  frame:
    unit: "um"

  domain:
    min: [0, 0, 0]
    max: [100, 100, 100]

  shapes:
    # Each file is split into closed shells in first-face order. List order
    # fixes the source index, so (source, shell) is a reproducible address.
    files: ["particles.stl"]
    # Scale-invariant, so they are applied once to the library rather than to
    # every rescaled candidate.
    filters:
      max_aspect_ratio: 3.0

  size:
    # A truncated lognormal in diameter. The alternative is
    # { kind: histogram, csv: "..." }, which reads the same CSV format the
    # original engine uses.
    distribution:
      kind: lognormal
      median: 12.0
      sigma_log: 0.35
      min: 6.0
      max: 24.0
    # Reporting classes for the target-against-actual table.
    classes: { kind: equal_width, count: 6 }

  # Shoemake's uniform unit quaternion: Haar-uniform on SO(3). The original
  # engine's rotation_mode: any is an axis-and-angle sampler and is not.
  orientation: { mode: uniform_so3 }

  # Clearance between placed particles.
  gaps: { particle_particle: 1.0 }

  target:
    volume_fraction: 0.10
    # basis defaults to `domain` without a void, `solid` with one.

  budget:
    attempts_per_particle: 3000
    total_attempts: 2000000

  outputs:
    dir: "../output/placement"
```

每个字段、每个默认值，以及每条拒绝规则及其存在的理由，见
[`../reference/config.md`](../reference/config.md) 的 `placement.rs` 一节。

### 路径契约，各用一句话

*配置中的相对路径相对于该配置文件所在目录解析；从不查阅当前工作目录。* 这就是为什么
`files: ["particles.stl"]` 找到的是 `data/input/particles.stl`，而 `dir: "../output/placement"`
写入的是 `data/output/placement`——无论从哪个目录启动二进制。

*命令行上给出的路径按 shell 的含义解析，即相对于当前工作目录。* `--input` 与 `--output` 在被代入本块
之前会先转为绝对路径，使两条规则同时成立。

经由旧版工作目录默认路径（省略 `--config` 时的 `data/input/pack_config.yaml`）找到的 placement 配置
会被**拒绝**。本引擎的承诺是：它读取的任何东西都不依赖于从何处启动；迁就一个工作目录默认值会悄悄破坏
这一承诺。

## 运行方式

```bash
./target/release/rustmspt pack --config data/input/placement_config.yaml
```

## 预期输出

取自上述运行的真实 stdout：

```
[Info] Placement seed 20260910 (chacha12), -1 thread setting
[Info] Placed 82 particle(s); volume fraction 0.099372 of the domain basis (target 0.100000)
[Info] Attempts 185 of a 2000000 budget
[Info] Stop reason: target_reached
[Info] every planned size was placed and the target volume fraction was reached
[Info] Wrote data/input/../output/placement
```

该次运行耗时 `real 0m0.351s`，向 `data/output/placement/` 写出四个文件：

| 文件 | 字节 | 是什么 |
|---|---|---|
| `particles.stl` | 1,709,484 | 合并后的颗粒几何，按接受顺序 |
| `particles.json` | 163,172 | 每颗粒一条记录，含其变换 |
| `run_report.json` | 5,430 | 身份、种子、输入、目标、实际、拒绝计数、停止原因 |
| `size_distribution.csv` | 467 | 逐尺寸分组的目标与实际 |

`data/input/../output/placement` 正是配置所写路径的解析结果。它没有被规范化，因为只有当 `a` 不是符号
链接时 `a/../b` 才等同于 `b`，而本引擎按词法解析路径，不去触碰文件系统。

## 尺寸分布：目标与实际

`data/output/placement/size_distribution.csv` 的真实内容：

```
class,lo,hi,target_frequency,target_count,drawn,placed,shortfall,top_up_drawn,top_up_placed
0,6.000000000000,9.000000000000,0.190818581175,16,23,23,0,0,0
1,9.000000000000,12.000000000000,0.309181434576,25,22,22,0,0,0
2,12.000000000000,15.000000000000,0.250033318666,21,19,19,0,0,0
3,15.000000000000,18.000000000000,0.145479545122,12,11,11,0,0,0
4,18.000000000000,21.000000000000,0.071838178127,6,5,5,0,0,0
5,21.000000000000,24.000000000000,0.032648942335,3,2,2,0,0,0
```

每个分组中 `drawn` 都等于 `placed`，`shortfall` 全为零：这次运行规划的一切都放置成功了。

这些列值得细读，因为它们之间的差别正是要点所在。

- `target_count` 是该分组在此分布中的份额，按这么多颗粒折算出的数量。
- `drawn` 是本次运行实际抽到的数量。它与 `target_count` 的差异来自抽样噪声，在 82 颗粒的规模上这是
  预料之中的。
- `placed` 是放得下的数量。**`drawn` 减 `placed` 就是 `shortfall`，而且绝不会为此补抽任何东西。**
  所有尺寸都在放置开始之前抽定，失败的尺寸绝不被替换。这正是防止一次运行在大颗粒放不下时，靠添加小
  颗粒不声不响地凑够体积分数——失败会留在它所属的分组里，清晰可见，而不是消融进总量。
- `top_up_drawn` 与 `top_up_placed` 单列存放也是同一个理由。只有在**全部**已规划尺寸都放置成功、
  且体积是在域边界上损失掉的时候，才会补抽一批；一旦有失败，就完全不会补抽。

## 逐颗粒记录

`data/output/placement/particles.json` 的头部，真实内容：

```json
{
  "schema_version": "rustmspt.placement.record/1",
  "tool": {
    "name": "rustmspt",
    "version": "0.2.0",
    "git_commit": "77642fdae99098cf810984f3b5086d469a721e8e",
    "git_dirty": true,
    "features": [
      "default"
    ],
    "target": "aarch64-unknown-linux-gnu",
    "host": "aarch64-unknown-linux-gnu",
    "profile": "release"
  },
  "seed": 20260910,
  "rng": "chacha12",
  "frame": {
    "unit": "um",
    "origin": [0.0, 0.0, 0.0],
    "axis_order": "xyz",
    "handedness": "right",
    "domain": {
      "min": [0.0, 0.0, 0.0],
      "max": [100.0, 100.0, 100.0]
    }
  },
  "conventions": {
    "rotation": "unit quaternion, component order w,x,y,z (scalar first), canonicalised to w >= 0; `matrix` is derived from it and must agree",
    "shell_centroid": "the volume centroid of the closed source shell, by the signed-tetrahedron formula. Use the value recorded here rather than recomputing one: a vertex mean is a different point.",
    "stl_precision": "the merged STL stores float32 with zero normals; this record stores float64. Reconstruction agrees to within 1.000e-3 in this frame, which is over a hundred times the float32 storage error and far below any transform mistake.",
    "transform": "p_world = R(q) * (scale * (p_source - shell_centroid)) + translation",
    "translation": "the placed particle's volume centroid, in the run frame, in um",
    "triangle_range": "half-open [start, end) into the merged STL's triangle list",
    "volume": "in_domain_solid = in_domain - the part of the particle the void owns; in_domain is gross"
  },
```

`conventions` 块不是碰巧躺在文件里的文档，它就是契约，放在使用方一定会读到的地方：其中每一句都是读者
可能弄错、且弄错之后得到的是貌似合理的错误颗粒而非报错的地方。四元数的分量顺序是最明显的例子——
`nalgebra` 的四元数按 `[i, j, k, w]` 存储，而本记录发布的是 `[w, x, y, z]`，把一个当作另一个来读，会
得到一个完全合法、却不是当初所用的旋转。

第一个颗粒，完整内容：

```json
{
  "entity_id": "p000000",
  "acceptance_index": 0,
  "source_shape": {
    "source_index": 0,
    "path": "particles.stl",
    "sha256": "77fc24b9144a6541d604777ff697bc1fed24d2ad5d3a86239e2b068e508bc2f1",
    "shell_index": 1,
    "shell_sha256": "aeb8557dac883c95655a8111586e95662a7d63539faa6a34dbf2d8e33ed60d07",
    "shell_centroid": [166.78509225061003, 632.7816179920254, 1216.7894042395735],
    "shell_volume": 291.5236739545008,
    "shell_equivalent_diameter": 8.226688793732556
  },
  "scale": 2.8713896902584612,
  "rotation": {
    "quaternion": [0.3678065941127522, 0.6762105784147148, 0.0008832030275986394, -0.6383234156128273],
    "matrix": [
      [0.1850848740655714, 0.4707535853382458, -0.8626323963794305],
      [-0.4683646604176454, -0.7294350585591789, -0.4985569578459734],
      [-0.8639317879693951, 0.49630188115294804, 0.08547694718489784]
    ]
  },
  "translation": [21.76732699091492, 52.03848224625089, 45.96408596707188],
  "equivalent_diameter": 23.622029387288478,
  "volume": {
    "full": 6901.607209544339,
    "in_domain": 6901.607209544339,
    "in_domain_solid": 6901.607209544339
  },
  "clipped": { "any": false, "faces": [] },
  "void_overlap_volume": 0.0,
  "size_class": 5,
  "triangle_range": [0, 402],
  "bbox": {
    "min": [9.026609545024524, 32.94119717830499, 33.37545645953148],
    "max": [33.05700803312416, 70.13967774209155, 58.39314874515995]
  }
}
```

有两个字段值得留意。

`shell_sha256` 与 `shell_index` 并列，是因为序号说明的是一个形状是**如何被找到的**，而不是它**是**
哪个形状。把 `particles.stl` 按不同面序重新导出，会悄然移动每一个序号；而摘要取自壳自身的几何，因此
使用方能区分这两种情况。

`triangle_range` 是 `[0, 402)`，指向 `particles.stl` 的三角形列表，因此这个颗粒的几何可以直接从合并
文件中取出，无需重建任何东西。这些区间恰好铺满整个文件，既无空隙也无重叠。

最大的颗粒最先放置：`size_class: 5` 是最高分组，而 `acceptance_index: 0`。默认按由大到小放置，因为
大颗粒才是先放不下的那批，趁还有空间时先放它们，才能让失败以缺口的形式留下痕迹，而不是变成一次悄悄
沦为小颗粒堆的运行。

## 可复现性，实测

下面两次运行只有线程数不同：

```bash
./target/release/rustmspt pack --config data/input/placement_config.yaml --output /tmp/t1 --threads 1
./target/release/rustmspt pack --config data/input/placement_config.yaml --output /tmp/t8 --threads 8
sha256sum /tmp/t1/particles.json /tmp/t8/particles.json
```

两个摘要相同。两份 `particles.stl` 也逐字节相同。换一个 `--seed` 则得到不同的装配体。

`--seed` 与 `--threads` 只适用于本引擎。把二者中任一个用在 `packing:` 配置上是**错误**，而不是空操作：
接受一个确定性开关却忽略它，会把一次并不可复现的运行报告成可复现的。

## 说明

- 本引擎的 `orientation.mode: uniform_so3` 是 Shoemake 的均匀单位四元数，在 SO(3) 上服从 Haar 分布。
  原引擎的 `rotation_mode: any` 在立方体内取轴、均匀取角，并不具备这一性质；它保留原名与原行为。
- 位置采样报告为 `rejection_uniform_rsa`。给定抽到的尺寸、抽到的壳以及已接受的颗粒，一次放置在满足
  全部约束的放置集合上是均匀的——但整个装配体是随机顺序吸附构型，不是平衡态硬核构型，这个名字如实
  说明了这一点，而没有主张更多。
- 未达标的运行仍会**以零码退出**，并在报告中说明。只有配置不可用或输出无法写入才是错误。
- 完整算法，包括孔隙判据的完备性论证与停止原因的优先级，见
  [`../algorithms/void-aware-placement.md`](../algorithms/void-aware-placement.md)。
