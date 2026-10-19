# `tet-repair` — repairing inverted and badly shaped cells

`tet-repair` repairs the inverted cells and the cells over an aspect-ratio limit of a
tetrahedral mesh by flips and node moves, keeping every face between two materials.
The rule is in [`algorithms/tet-repair.md`](../algorithms/tet-repair.md).

## Inputs

| File | Shape | Type | Meaning |
|---|---|---|---|
| `points.npy` | `(n, 3)` | f64 | the nodes |
| `tets.npy` | `(m, 4)` | int64 | the cells |
| `density.npy` | `(m)` | int64 | each cell's material (non-negative) |
| `fixed.npy` | `(n)` | int64 | 1 for a node that must stay (e.g. a cohesive layer's) |
| `layer.npy` (optional) | `(k, 3)` | int64 | faces a fixed node may slide along |

## Config

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

`padding` (a density dropped before a solve), `joint_tol` and `layer` are optional.

## Run

```bash
cargo build --release
./target/release/rustmspt tet-repair --config work/repair/tet_repair.yaml
```

Captured on a closure mesh straight from the mesher (1,152,074 cells, 3 inverted and
21 over the limit), with nothing fixed, eight workers:

```
[tet-repair] 1152074 cells: 24 bad before, 13 flips, 6 nodes moved; 0 inverted and 6 over 9.999999990000001 after; 0.24 s
```

simHelper's Python `flip_repair` takes the same 13 flips and the same 6 moves on
this mesh and leaves the same worst ratio, 16.050044441337057; the six cells still
over the limit are taken by its later steps (a wall node sliding along its wall).

## Outputs

`points.npy`, `tets.npy`, `density.npy` and `report.json` (one flip and two moves
shown, the build identity cut short):

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
