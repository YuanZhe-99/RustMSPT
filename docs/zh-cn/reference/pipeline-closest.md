# 最近点 — `closest`

`closest` 子命令的参考：图元、其包围体层次结构与搜索（`src/geometry/closest.rs`），GPU 种子阶段（`src/gpu/closest_candidates.rs`、`src/gpu/shaders/closest_candidates.wgsl`），流水线（`src/pipeline/closest.rs`）及其配置（`src/config/closest.rs`）。`.npy` 读写沿用 `state-map` 的（[`pipeline-state-map.md`](pipeline-state-map.md)）。算法见 [`algorithms/closest-point.md`](../algorithms/closest-point.md)。

## Index

| 条目 | 源码 | 摘要 |
|---|---|---|
| `DISTANCE_TIE` | `src/geometry/closest.rs:4` | `1e-12`：与最小距离相差在此比例内视为并列，取图元编号最小者。 |
| `Kind` | `src/geometry/closest.rs:10` | `Triangles` 或 `Segments`。 |
| `Kind::corners` | `src/geometry/closest.rs:17` | 每个图元的角点数：3 或 2。 |
| `Primitives` | `src/geometry/closest.rs:26` | 顶点与角点编号；线段存为 `[a, b, b]`。 |
| `Primitives::new` | `src/geometry/closest.rs:39` | 检查并打包数组；拒绝形状不齐、顶点非有限值、编号越界。 |
| `Primitives::len` / `is_empty` | `src/geometry/closest.rs:64` | 图元数。 |
| `Primitives::corner_points` | `src/geometry/closest.rs:74` | 一个图元的角点坐标。 |
| `Primitives::closest` | `src/geometry/closest.rs:80` | 一个图元上的最近点及其精确距离。 |
| `closest_on_segment` | `src/geometry/closest.rs:125` | 线段上的截断投影。 |
| `closest_on_triangle` | `src/geometry/closest.rs:141` | 按 Voronoi 区域求三角形上的最近点；面积为零的三角形取最近的边。 |
| `PrimBvh` | `src/geometry/closest.rs:208` | 建立在精确图元包围盒上的静态层次结构，布局可供 GPU 直接遍历。 |
| `PrimBvh::build` | `src/geometry/closest.rs:225` | 确定性的中位数划分构建。 |
| `PrimBvh::nearest` | `src/geometry/closest.rs:335` | 对一个点执行规则，可带种子。 |
| `Nearest` | `src/geometry/closest.rs:362` | 每个查询的图元（无则 `-1`）、最近点、距离。 |
| `nearest_all` | `src/geometry/closest.rs:374` | 在当前线程池中并行处理所有查询，可带种子。 |
| `GpuClosestSeedPipeline` | `src/gpu/closest_candidates.rs:8` | 共享设备上编译好的种子内核。 |
| `GpuClosestSeedPipeline::new` | `src/gpu/closest_candidates.rs:22` | 编译或复用内核。 |
| `GpuClosestSeedPipeline::adapter` | `src/gpu/closest_candidates.rs:90` | 适配器名称和后端，写入报告。 |
| `GpuClosestSeedPipeline::seeds` | `src/gpu/closest_candidates.rs:100` | 以 `f32` 求每个点的最近图元，按设备上限分批。 |
| `ClosestParams` | `src/config/closest.rs:11` | YAML 中的 `closest` 块。 |
| `ClosestConfig` | `src/config/closest.rs:25` | 整个文档：`closest` 加可选的 `acceleration`。 |
| `METHOD_VERSION` | `src/pipeline/closest.rs:10` | `closest-point-v1`，写入每份报告。 |
| `ClosestPipeline` | `src/pipeline/closest.rs:12` | 该子命令的流水线。 |
| `ClosestPipeline::run` | `src/pipeline/closest.rs:32` | 读取、构建、种子、搜索，写出 primitive/closest/distance/report。 |

## `src/geometry/closest.rs`

### `Primitives::new(kind: Kind, vertices: &[f64], corners: &[u32]) -> Result<Primitives>`
- **用途：** 检查并打包扁平的顶点坐标和角点编号。
- **参数：** `kind`；`vertices` 扁平 xyz；`corners` 扁平，每个三角形三个、每条线段两个。
- **返回：** 图元；顶点数组长度不是 3 的倍数、角点数不是该类型角点数的倍数、顶点非有限值、或编号不小于顶点数时返回 `InvalidConfig`。
- **副作用：** 无。
- **说明：** 线段存为 `[a, b, b]`，使两种类型共用一种布局（GPU 每个图元读九个浮点数）。

### `Primitives::closest(&self, i: usize, p: &[f64; 3]) -> ([f64; 3], f64)`
- **用途：** 图元 `i` 上离 `p` 最近的点及到它的精确距离。
- **返回：** 该点及其到 `p` 的欧氏距离。
- **副作用：** 无。

### `closest_on_segment(p, a, b) -> [f64; 3]` / `closest_on_triangle(p, a, b, c) -> [f64; 3]`
- **用途：** 线段（截断投影；退化为点的线段返回 `a`）或三角形（Ericson 的 Voronoi 区域法）上离 `p` 最近的点。
- **返回：** 该点。
- **副作用：** 无。
- **说明：** 面区域分母不为正（面积为零）的三角形取三条边中最近的一条。

### `PrimBvh::build(prims: &Primitives) -> PrimBvh`
- **用途：** 在每个图元的精确包围盒上建立层次结构。
- **返回：** 节点（`node_min`、`node_max`、`link`）、图元 `order`、各槽包围盒和深度。叶子的 `link[n]` 为 `[first, count]`（槽 `first..first+count`），内部节点为 `[left, 0]`，其子节点为 `left` 和 `left + 1`。
- **副作用：** 无。
- **说明：** 按盒心最长轴中位数划分，按盒心再按编号排序，叶子最多四个；确定性。

### `PrimBvh::nearest(&self, prims, p, seed: Option<u32>) -> (u32, [f64; 3], f64)`
- **用途：** 对一个点执行本模块的规则：精确距离最小者，与之相差在 `DISTANCE_TIE` 内的并列者取编号最小。
- **参数：** `seed`：一个图元，用其精确距离限定第一遍；越界或 `None` 时不设限。
- **返回：** 图元、最近点、距离；没有图元时为 `u32::MAX` 和 `NaN`。
- **副作用：** 无。
- **说明：** 两遍分支定界（先求最小距离，再在容差带内取最小编号）。种子只改变工作量，不改变答案。

### `nearest_all(prims, bvh, queries: &[f64], seeds: Option<&[u32]>) -> Result<Nearest>`
- **用途：** 用所有核心处理全部查询点。
- **参数：** `queries` 扁平 xyz；`seeds` 每点一个，`u32::MAX` 表示无。
- **返回：** `Nearest`（`primitive` 为 int64、无则 `-1`，`closest` 扁平 xyz，`distance`）；查询形状不齐或含非有限值、种子数组长度不对时返回 `InvalidConfig`。
- **副作用：** 无。
- **说明：** 在当前线程池上用 rayon；答案与线程数和种子无关。

## `src/gpu/closest_candidates.rs`

### `GpuClosestSeedPipeline::seeds(&self, prims, bvh, points: &[f64]) -> Result<Vec<u32>, String>`
- **用途：** 为每个点求 GPU 在 `f32` 下找到的最近图元，作为 `nearest_all` 的种子。
- **返回：** 每点一个编号（没找到为 `u32::MAX`）；或无法运行的原因：层次结构深于着色器栈（60）、缓冲区超出设备上限、设备故障。
- **副作用：** 节点包围盒、链接、图元角点（每槽九个 `f32`）和顺序只上传一次；然后按批分派并读回。
- **说明：** 坐标在 `f64` 下先减去根包围盒中心再取整。盒距离不向外取整：种子只用来限定主机上的精确搜索。

## `src/pipeline/closest.rs`

### `ClosestPipeline::run(&self) -> Result<()>`
- **用途：** 该子命令本身。
- **参数：** `self.config`：`closest.vertices`，`closest.triangles` 与 `closest.segments` 二者恰取其一，`closest.queries`，`closest.output_dir`，`closest.cpu_max`，以及 `acceleration`。
- **返回：** `Ok`；数组无法读取或形状不齐、同时给出或都不给出三角形与线段、角点形状不是 `(m, 3)` / `(m, 2)`、禁止回退时 GPU 失败、或读写失败时报错。
- **副作用：** 读取三个 `.npy` 文件；创建 `output_dir`，写出 `primitive.npy`、`closest.npy`、`distance.npy` 和 `report.json`；打印一行摘要。

## `src/config/closest.rs`

### `ClosestParams`
- **字段：** `vertices`、`triangles`（可选）、`segments`（可选）、`queries`、`output_dir`：路径；`cpu_max`：线程上限（`-1` 或不填为全部核心）。
