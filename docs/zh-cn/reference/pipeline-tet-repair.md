# 四面体网格修复 — `tet-repair`

`tet-repair` 子命令的参考：质量度量、Nelder-Mead 与修复本身（`src/geometry/tet_repair.rs`），流水线（`src/pipeline/tet_repair.rs`）及其配置（`src/config/tet_repair.rs`）。`.npy` 读写沿用 `state-map` 的（[`pipeline-state-map.md`](pipeline-state-map.md)）；点到三角形的距离沿用 `closest` 的（[`pipeline-closest.md`](pipeline-closest.md)）。算法见 [`algorithms/tet-repair.md`](../algorithms/tet-repair.md)。

## Index

| 条目 | 源码 | 摘要 |
|---|---|---|
| `FLAT` | `src/geometry/tet_repair.rs:7` | `1e-30`：体积不超过它的单元长宽比为无穷大。 |
| `RepairParams` | `src/geometry/tet_repair.rs:13` | limit、sweeps、wall_tol、padding、joint_tol、seed。 |
| `Flip` | `src/geometry/tet_repair.rs:24` | 一次翻转：类型、位置、前后最差长宽比。 |
| `Moved` | `src/geometry/tet_repair.rs:32` | 一次节点移动：节点、是否沿壁或沿层、距离、前（`None` 为翻转）后超限量、是否联动修复。 |
| `ToPadding` | `src/geometry/tet_repair.rs:43` | 划给填充区的扁平单元。 |
| `RepairReport` | `src/geometry/tet_repair.rs:51` | 修复前坏单元、划给填充区、翻转、移动、之后翻转和超限数、之后最差长宽比。 |
| `Repaired` | `src/geometry/tet_repair.rs:62` | 节点、存活单元、材料号、报告。 |
| `aspect_ratio` | `src/geometry/tet_repair.rs:95` | 单个单元的 YAFEQ 长宽比和有向体积。 |
| `nelder_mead` | `src/geometry/tet_repair.rs:136` | scipy 的非自适应 Nelder-Mead。 |
| `repair` | `src/geometry/tet_repair.rs:692` | 整套规则：填充区、翻转与移动、联动修复。 |
| `TetRepairParams` | `src/config/tet_repair.rs:9` | YAML 中的 `tet_repair` 块。 |
| `TetRepairConfig` | `src/config/tet_repair.rs:45` | 整个文档。 |
| `METHOD_VERSION` | `src/pipeline/tet_repair.rs:9` | `tet-repair-v1`，写入每份报告。 |
| `TetRepairPipeline` | `src/pipeline/tet_repair.rs:11` | 该子命令的流水线。 |
| `TetRepairPipeline::run` | `src/pipeline/tet_repair.rs:22` | 读取、修复、写出网格和报告。 |

## `src/geometry/tet_repair.rs`

### `aspect_ratio(q: &[[f64; 3]; 4]) -> (f64, f64)`
- **用途：** 单个单元的 YAFEQ 长宽比和有向体积。
- **返回：** `(ratio, volume)`；`|volume| <= FLAT` 时长宽比为无穷大。
- **副作用：** 无。
- **说明：** 与 simHelper 的 `yafeq_aspect_ratio` 和 YAFEQ 的 `MESH_verification` 公式相同。

### `nelder_mead(f, x0, xatol, fatol, maxiter) -> (Vec<f64>, f64)`
- **用途：** 按 scipy `minimize(method="Nelder-Mead")`（不带 `adaptive`）的做法从 `x0` 最小化 `f`。
- **返回：** 最好的顶点及其函数值。
- **副作用：** 调用 `f`。
- **说明：** 初始单纯形每个坐标增加 5 %（为零时取 0.00025）；所有顶点与最好顶点相差在 `xatol` 内且函数值相差在 `fatol` 内时停止，或达到 `maxiter` 次迭代。

### `repair(points, tets, density, fixed, layer, prm) -> Result<Repaired>`
- **用途：** 按算法页的规则修复翻转单元和超过 `prm.limit` 的单元。
- **参数：** `points` 扁平 xyz；`tets` 扁平、每单元四个；`density` 每单元一个；`fixed` 每节点一个（内聚层的节点）；`layer` 扁平的节点三元组（`wall_tol > 0` 时固定节点可沿其滑动的面）；`prm`。
- **返回：** `Repaired`；数组形状不齐或节点编号越界时返回 `InvalidConfig`。
- **副作用：** 无。
- **说明：** 给定种子时是确定的；访问顺序并列时取编号小者。从不在不同材料之间翻转，也不穿过固定面。

## `src/pipeline/tet_repair.rs`

### `TetRepairPipeline::run(&self) -> Result<()>`
- **用途：** 该子命令本身。
- **参数：** `self.config.tet_repair`：数组路径、`output_dir`、`limit`（默认 10）、`sweeps`（20）、`wall_tol`（0）、`padding`、`joint_tol`、`seed`（0）、`cpu_max`。
- **返回：** `Ok`；数组无法读取或形状不齐、编号越界、或读写失败时报错。
- **副作用：** 读取数组；在 `output_dir` 下写出 `points.npy`、`tets.npy`、`density.npy` 和 `report.json`；打印一行摘要。
