# `closest` — 曲面或线上的最近点

`closest` 为每个查询点回答：曲面上最近的三角形（或一组线中最近的线段）、其上的最近点和距离。规则与设计见 [`algorithms/closest-point.md`](../algorithms/closest-point.md)。

## 输入

三个 `.npy` 数组，由调用方用 `numpy.save` 写出：

| 文件 | 形状 | 类型 | 含义 |
|---|---|---|---|
| `vertices.npy` | `(n, 3)` | f64 | 曲面（或线）的顶点 |
| `corners.npy` | `(m, 3)` 或 `(m, 2)` | int64 | 三角形或线段，以顶点编号表示 |
| `queries.npy` | `(..., 3)` | f64 | 要求解的点 |

## 配置

```yaml
closest:
  vertices: "work/closest/vertices.npy"
  triangles: "work/closest/corners.npy"
  queries: "work/closest/queries.npy"
  output_dir: "work/closest/out"
  cpu_max: -1
acceleration:
  mode: gpu
```

线段的角点文件写在 `segments:` 下，而不是 `triangles:`；两个都给或都不给会被拒绝。

## 运行

```bash
cargo build --release --features gpu
./target/release/rustmspt closest --config work/closest/closest.yaml
```

在真实算例上捕获：一个四面体网格 1,152,427 个单元的中心，对它所切合的 25,608 个三角形的闭合中面，八个线程，集成 GPU：

```
[Info] GPU adapter: name=Intel(R) Iris(R) Plus Graphics 655 (CFL GT3) backend=Vulkan type=IntegratedGpu class=hardware
[closest] 1152427 points on 25608 Triangles; backend gpu; 8 workers; seed 0.25 s, search 1.21 s
[Timing] gpu init_seconds=0.095582 upload_bytes=13829156 upload_calls=4 readback_bytes=4609708 readback_calls=2 readback_wait_seconds=0.114961
```

同一运行用 `mode: cpu` 搜索耗时 1.45 s，图元、最近点和距离完全相同。VTK 的 `find_closest_cell` 对同一批点用了 25.1 s；它的距离与这里相差在 `2e-12` 以内，三角形编号不同的地方（这些点中的 36 %，几乎都位于几个三角形共享的棱或顶点上）距离是并列的。

## 输出

`primitive.npy`（int64，没有图元时为 `-1`）、`closest.npy`（`(..., 3)` f64）和 `distance.npy`（f64），形状与查询的前导维相同，以及 `report.json`：

```json
{
  "backend": {
    "adapter": "Intel(R) Iris(R) Plus Graphics 655 (CFL GT3) (Vulkan)",
    "fallback_reason": null,
    "requested": "gpu",
    "used": "gpu"
  },
  "bvh": { "depth": 14 },
  "distance_tie": 1e-12,
  "kind": "triangles",
  "method_version": "closest-point-v1",
  "primitives": 25608,
  "query_shape": [1152427],
  "rule": "the primitive at the least exact f64 distance wins; primitives within distance_tie (relative) of it tie and the lowest index wins",
  "tool": { "name": "rustmspt", "version": "0.2.1", "features": ["default", "gpu"], "...": "..." },
  "vertices": 14101,
  "wall_time_s": { "build": 0.017, "read": 0.021, "search": 1.215, "seed": 0.251, "total": 1.542, "write": 0.036 },
  "workers": 8
}
```
