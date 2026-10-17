# `tet-repair` — 修复翻转和形状差的单元

`tet-repair` 通过翻转和移动节点修复四面体网格中的翻转单元和长宽比超限的单元，同时保留两种材料之间的每一个面。规则见 [`algorithms/tet-repair.md`](../algorithms/tet-repair.md)。

## 输入

| 文件 | 形状 | 类型 | 含义 |
|---|---|---|---|
| `points.npy` | `(n, 3)` | f64 | 节点 |
| `tets.npy` | `(m, 4)` | int64 | 单元 |
| `density.npy` | `(m)` | int64 | 每个单元的材料（非负） |
| `fixed.npy` | `(n)` | int64 | 必须保持不动的节点为 1（例如内聚层的节点） |
| `layer.npy`（可选） | `(k, 3)` | int64 | 固定节点可沿其滑动的面 |

## 配置

```yaml
tet_repair:
  points: "work/repair/points.npy"
  tets: "work/repair/tets.npy"
  density: "work/repair/density.npy"
  fixed: "work/repair/fixed.npy"
  output_dir: "work/repair/out"
  limit: 9.999999990000001
  sweeps: 20
  wall_tol: 0.0
  seed: 0
  cpu_max: -1
```

`padding`（求解前会去掉的材料号）、`joint_tol` 和 `layer` 是可选的。

## 运行

```bash
cargo build --release
./target/release/rustmspt tet-repair --config work/repair/tet_repair.yaml
```

在一个刚由网格生成器输出的闭合网格上捕获（1,152,074 个单元，3 个翻转、21 个超限），不固定任何节点，八个线程：

```
[tet-repair] 1152074 cells: 24 bad before, 13 flips, 6 nodes moved; 0 inverted and 6 over 9.999999990000001 after; 0.24 s
```

在这个网格上，simHelper 的 Python `flip_repair` 采用同样的 13 次翻转和同样的 6 次移动，剩下的最差长宽比也同样是 16.050044441337057；仍然超限的六个单元由其后续步骤（壁上节点沿壁滑动）处理。

## 输出

`points.npy`、`tets.npy`、`density.npy` 和 `report.json`（只列出一次翻转和两次移动，构建标识从略）：

```json
{
  "cells": 1152074,
  "method_version": "tet-repair-v1",
  "params": { "joint_tol": null, "limit": 9.999999990000001, "padding": null, "seed": 0, "sweeps": 20, "wall_tol": 0.0 },
  "points": 232093,
  "repair": {
    "bad_before": 24,
    "flips": [
      { "at": [38.56672477722168, 106.46074867248535, 15.122049808502197], "kind": "4-4", "worst": [4680.393835347047, 2.6704705495733743] }
    ],
    "inverted_after": 0,
    "nodes_moved": [
      { "by": 0.4138909227463959, "excess": [7.555320993042395, 6.050044451337056], "joint": false, "node": 2706, "on_layer": false, "on_wall": false },
      { "by": 0.4510407099831562, "excess": [8.871621240332852, 5.574087516703868], "joint": false, "node": 173019, "on_layer": false, "on_wall": false }
    ],
    "over_limit_after": 6,
    "ratio_max_after": 16.050044441337057,
    "to_padding": []
  },
  "tool": { "name": "rustmspt", "version": "0.2.1" },
  "wall_time_s": { "read": 0.041, "repair": 0.243, "total": 0.321 },
  "workers": 8
}
```
