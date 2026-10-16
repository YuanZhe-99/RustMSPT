# `state-map` —— 定位重网格化的采样点

`state-map` 回答新网格的每个采样点落在旧（变形后）网格的哪里，供跨重网格化传递已求解状态的调用方（simHelper 的状态映射器）使用。规则和设计见 [`algorithms/state-map-location.md`](../algorithms/state-map-location.md)。

## 输入

调用方用 `numpy.save` 写出三个 `.npy` 数组：

| 文件 | 形状 | 类型 | 含义 |
|---|---|---|---|
| `source_points.npy` | `(n, 3)` | f64 | 源网格节点（变形后） |
| `source_cells.npy` | `(m, 4)` | int64 | 源四面体 |
| `query_points.npy` | `(..., 3)` | f64 | 采样点（simHelper 写 `(n_target, 15, 3)`） |

## 配置

```yaml
state_map:
  source_points: "work/state/locate/source_points.npy"
  source_cells: "work/state/locate/source_cells.npy"
  query_points: "work/state/locate/query_points.npy"
  fallback_distance: 0.0026666666666666666
  output_dir: "work/state/locate/out"
  cpu_max: -1
acceleration:
  mode: auto
```

`gpu_slots`（默认 24）是 GPU 为每个点保留的候选四面体数；超出的点改在 CPU 上处理，只影响速度，不影响结果。

## 运行

```bash
cargo build --release --features gpu
./target/release/rustmspt state-map --config work/state/locate/state_map.yaml
```

在一次真实重网格化上捕获（913,080 单元的变形源网格、1,015,356 单元的目标网格，每个目标单元 15 个采样点），8 个线程加集成显卡：

```
[Info] GPU adapter: name=Intel(R) Iris(R) Plus Graphics 655 (CFL GT3) backend=Vulkan type=IntegratedGpu class=hardware
[state-map] 15230340 points: 15123531 contained, 106809 fallback, 0 unlocated; backend gpu; 8 workers; locate 2.57 s
[Timing] gpu init_seconds=0.083453 upload_bytes=182764320 upload_calls=30 readback_bytes=1523034000 readback_calls=15 readback_wait_seconds=0.669724
```

同一输入用 `mode: cpu` 在 8 个线程上定位耗时 4.9 s，供体与距离完全相同。

## 输出

`donors.npy`（int64，`-1` 表示无）和 `distances.npy`（f64：内部为 `0`，回退供体为距离，无供体为 `NaN`），形状为查询点的前导轴（此处 `(1015356, 15)`），以及 `report.json`。报告字段与英文版 [`examples/state-map.md`](../../en-us/examples/state-map.md) 中的完整示例相同。

`gpu_overflow` 统计候选数超过 `gpu_slots`、改在 CPU 上处理的点。在 `mode: auto` 下若 GPU 无法启动，`used` 为 `"cpu"`，原因写在 `fallback_reason`。
