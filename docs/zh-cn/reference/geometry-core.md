# Geometry Core Reference

> **待翻译：** `src/geometry/render.rs` 的相机、投影和 CPU 渲染契约见[英文参考](../../en-us/reference/geometry-core.md#renderrs)。

本页记录 `src/geometry/` 中的核心几何图元：轴对齐包围盒与堆积边界检查（`bbox.rs`）、网格操作工具（`mesh_ops.rs`），以及用于邻居查询的均匀空间网格（`spatial.rs`）。本页还涵盖 `src/geometry/mod.rs`，它仅作为更大的 `geometry` 包的模块中枢。

## 索引

| 函数 | 位置 | 摘要 |
|---|---|---|
| `mesh_bbox` | `src/geometry/bbox.rs:8` | 网格的轴对齐包围盒。 |
| `bbox_overlaps` | `src/geometry/bbox.rs:28` | 两个包围盒之间的严格重叠测试。 |
| `bbox_distance` | `src/geometry/bbox.rs:38` | 两个包围盒之间的最小欧氏距离。 |
| `check_boundary_constraints_mode` | `src/geometry/bbox.rs:72` | 依据堆积边界模式规则校验网格的放置位置。 |
| `mesh_centroid` | `src/geometry/mesh_ops.rs:5` | 网格顶点的算术质心。 |
| `vec_norm` | `src/geometry/mesh_ops.rs:19` | 向量的欧氏长度。 |
| `merge_meshes` | `src/geometry/mesh_ops.rs:24` | 将多个网格合并为一个，并重新映射面索引。 |
| `split_mesh_into_granules` | `src/geometry/mesh_ops.rs:44` | 将网格拆分为连通分量（基于共享顶点的 BFS）。 |
| `translate_mesh` | `src/geometry/mesh_ops.rs:117` | 原地按增量向量平移所有网格顶点。 |
| `move_mesh_to_target_center` | `src/geometry/mesh_ops.rs:124` | 移动网格使其质心与目标位置一致。 |
| `wrap_mesh_centroid_to_box` | `src/geometry/mesh_ops.rs:135` | 在周期边界条件下，将网格质心折回到盒子内。 |
| `scale_mesh` | `src/geometry/mesh_ops.rs:156` | 原地围绕原点对网格顶点进行统一缩放。 |
| `mesh_surface_area` | `src/geometry/mesh_ops.rs:163` | 网格的总表面积（各三角形面积之和）。 |
| `rotate_mesh_around_center` | `src/geometry/mesh_ops.rs:182` | 使用罗德里格斯旋转公式，围绕质心旋转网格。 |
| `box_mesh` | `src/geometry/mesh_ops.rs:203` | 从 `BoundingBox` 构建三角剖分的盒状网格。 |
| `SpatialGrid::new` | `src/geometry/spatial.rs:14` | 以给定单元大小，在一个盒子上构造空的均匀网格。 |
| `SpatialGrid::insert` | `src/geometry/spatial.rs:31` | 将一个条目索引插入其包围盒重叠的每个单元格。 |
| `SpatialGrid::build` | `src/geometry/spatial.rs:49` | 从一批 (index, bbox) 对构造并填充网格。 |
| `SpatialGrid::query_neighbors` | `src/geometry/spatial.rs:58` | 查找与查询包围盒重叠的候选邻居索引。 |
| `SpatialGrid::query_neighbors_with_margin` | `src/geometry/spatial.rs:68` | 与 `query_neighbors` 相同，但按边距距离进行扩展。 |
| `SpatialGrid::point_to_cell_clamped` | `src/geometry/spatial.rs:93` | 将点映射到网格单元坐标，并钳制在网格边界内。 |
| `SpatialGrid::point_to_cell` | `src/geometry/spatial.rs:99` | 将点映射到网格单元坐标，不做钳制。 |
| `estimate_cell_size` | `src/geometry/spatial.rs:108` | 根据一组包围盒，启发式地选取 `SpatialGrid` 的单元大小。 |

## 模块角色：`geometry/mod.rs`

`src/geometry/mod.rs` 声明了 `geometry` 包的各个子模块（`bbox`、`collision`、`forging`、`mesh_ops`、`metrics`、`s2`、`spatial`、`volume`），并在 `geometry::` 路径下重新导出它们的公共项。它本身不包含任何函数——其存在的唯一目的是让调用方可以直接写 `crate::geometry::mesh_bbox` 等，而无需深入各个子模块。`gpu` 特性门控还会在此有条件地重新导出经 GPU 加速的 `s2` 变体。

## bbox.rs

这四个函数是贯穿堆积与优化流水线的核心几何门控：`mesh_bbox` 是几乎所有其他空间计算的基础（它是 `bbox_overlaps`、`bbox_distance`、`SpatialGrid` 插入以及 `check_boundary_constraints_mode` 的输入）；`bbox_overlaps`/`bbox_distance` 在精确的网格对网格检查之前提供粗略的碰撞预过滤；`check_boundary_constraints_mode` 则是候选颗粒放置在被提交到堆积或优化过程之前必须通过的验收测试。

#### mesh_bbox

- **签名：** `pub fn mesh_bbox(mesh: &Mesh) -> Option<BoundingBox>`
- **源码位置：** `src/geometry/bbox.rs:8`
- **用途：** 计算包含网格所有顶点的轴对齐包围盒。
- **参数：**
  - `mesh` — 待求边界的网格。
- **返回值：** `Some(BoundingBox)`，其 `min`/`max` 角点跨越所有顶点；若网格没有顶点则返回 `None`。
- **副作用：** 无。
- **说明：** 这是几何模块中其他地方用于粗略碰撞检测和边界检查的主要构建块。

#### bbox_overlaps

- **签名：** `pub fn bbox_overlaps(a: BoundingBox, b: BoundingBox) -> bool`
- **源码位置：** `src/geometry/bbox.rs:28`
- **用途：** 测试两个包围盒是否在全部三个坐标轴上都重叠。
- **参数：**
  - `a`、`b` — 待比较的两个包围盒。
- **返回值：** 若两个盒子在每个坐标轴上都有正重叠，则返回 `true`。
- **副作用：** 无。
- **说明：** 比较是严格的（`<`/`>`），因此仅在面、边或角相接触的盒子不算作重叠。
- **另请参阅：** `../algorithms/spatial-grid-collision.md`，了解此函数如何支撑粗筛阶段碰撞检测。

#### bbox_distance

- **签名：** `pub fn bbox_distance(a: BoundingBox, b: BoundingBox) -> f64`
- **源码位置：** `src/geometry/bbox.rs:38`
- **用途：** 计算两个包围盒之间的最小欧氏距离。
- **参数：**
  - `a`、`b` — 待测量距离的两个包围盒。
- **返回值：** 两个盒子最近点之间的直线距离；若两者在任一轴上重叠或相接触，则为 `0.0`（只要盒子在该轴上未分离，该轴上的间隙就为 `0.0`）。
- **副作用：** 无。
- **说明：** 按轴逐一计算（间隙或零），再通过 `sqrt(dx² + dy² + dz²)` 组合，因此对盒对盒的分离距离是精确的，而非近似值。

#### check_boundary_constraints_mode

- **签名：** `pub fn check_boundary_constraints_mode(mesh: &Mesh, box_bounds: BoundingBox, mode: u8, d1: f64, d2: f64) -> bool`
- **源码位置：** `src/geometry/bbox.rs:72`
- **用途：** 校验网格在堆积盒内部（或跨越堆积盒）的放置位置是否满足所配置的边界模式与间隙距离要求。
- **参数：**
  - `mesh` — 候选网格（已定位于世界/盒子坐标空间中）。
  - `box_bounds` — 堆积域的包围盒。
  - `mode` — 边界模式：`1` = 仅允许完全在内部；`2` 或 `3` = 允许周期性跨界（该函数对两者的处理完全相同——见"说明"）。
  - `d1` — 颗粒完全位于盒内时，必须与盒面保持的最小间隙。
  - `d2` — 在周期模式下颗粒跨越边界时所需的最小穿透/伸出深度。
  - 若网格没有顶点（`mesh_bbox` 返回 `None`），则立即返回 `false`。
- **返回值：** 若网格满足给定模式的约束，则为 `true`；否则为 `false`。
- **副作用：** 无。
- **说明——边界模式语义：**
  - 首先将网格的包围盒转换到盒子的局部坐标（相对于 `box_bounds.min` 的 `local_min`/`local_max`）。
  - **若网格在全部三个轴上都完全位于盒内**：仅当它与盒子每个面都至少保持 `d1` 的间隙时才被接受（每个轴上 `local_min >= d1` 且 `local_max <= size - d1`）。此检查无论 `mode` 为何都会执行。
  - **若网格未完全位于盒内**（至少跨越一个边界）：
    - **模式 1** 直接拒绝该放置——模式 1 完全不允许任何边界跨越。
    - **模式 2 和 3**（该函数对两者处理相同）允许跨界，但需满足逐轴检查：在网格跨越低侧面的任一轴上，其伸出 `min` 的部分或留在盒内的剩余深度必须至少为 `d2`，否则拒绝该放置；对称的逻辑适用于高侧面的轴跨越；仍完全位于盒界内的轴依然强制执行 `d1` 内部间隙。
    - 简言之：模式 1 = 严格非周期堆积（颗粒必须以 `d1` 边距完全位于域内）；模式 2/3 = 周期堆积，颗粒可以跨越域边界，只要两侧的跨越深度都至少为 `d2`。
- **另请参阅：** `../algorithms/spatial-grid-collision.md`；关于 `packing.mode` / 优化边界设置如何映射到此处的 `mode` 参数，请参阅配置文档。

## mesh_ops.rs

用于构建、变换和度量 `Mesh` 值的工具函数。这些函数被堆积、优化和锻造流水线广泛用于定位、调整大小和分析颗粒几何形状。

#### mesh_centroid

- **签名：** `pub fn mesh_centroid(mesh: &Mesh) -> Vec3`
- **源码位置：** `src/geometry/mesh_ops.rs:5`
- **用途：** 计算所有网格顶点位置的算术平均值（质心）。
- **参数：**
  - `mesh` — 待计算质心的网格。
- **返回值：** 质心 `Vec3`；若网格为空则为零向量。
- **副作用：** 无。
- **说明：** 这是顶点平均质心，而非体积或面积加权质心。

#### vec_norm

`#### vec_norm` — `pub fn vec_norm(v: Vec3) -> f64` — `src/geometry/mesh_ops.rs:19`。向量的欧氏长度。无副作用。

#### merge_meshes

- **签名：** `pub fn merge_meshes(meshes: &[Mesh]) -> Mesh`
- **源码位置：** `src/geometry/mesh_ops.rs:24`
- **用途：** 将多个独立的网格合并为单个网格。
- **参数：**
  - `meshes` — 按顺序待合并的网格切片。
- **返回值：** 一个新的 `Mesh`，其顶点缓冲区是所有输入顶点缓冲区的拼接，其面是所有输入面的拼接，索引经过偏移以指向合并后的顶点缓冲区。
- **副作用：** 无（不会修改输入）。
- **说明：** 与 `split_mesh_into_granules` 大致互逆，但合并不会重建邻接关系——它纯粹是拼接。

#### split_mesh_into_granules

- **签名：** `pub fn split_mesh_into_granules(mesh: &Mesh) -> Vec<Mesh>`
- **源码位置：** `src/geometry/mesh_ops.rs:44`
- **用途：** 将单个网格拆分为其连通分量（"颗粒"），连通性由三角形之间共享的顶点定义。
- **参数：**
  - `mesh` — 待分解的网格。
- **返回值：** 一个 `Vec<Mesh>`，每个连通分量对应一项，各自拥有紧凑的、独立索引的顶点缓冲区（与源网格或其他分量不共享索引）。若输入网格没有面或没有顶点，则返回空的 `Vec`。
- **副作用：** 无。
- **算法：**
  1. 构建一个 `vertex_to_faces` 邻接表：对每个顶点索引，记录引用它的面索引列表。
  2. 为每个面维护一个 `visited` 标记。对每个未访问的面，使用 `VecDeque` 队列执行广度优先搜索（BFS）：从该面开始，反复弹出一个面、将其记为当前分量的一部分，并将所有（通过 `vertex_to_faces`）与之共享任一顶点、且尚未访问的面加入队列。
  3. 一旦该 BFS 耗尽，所收集的 `component_faces` 即构成一个连通分量。其顶点通过一个从全局顶点索引到局部顶点索引的 `HashMap<usize, usize>` 被重新映射到一个新的紧凑局部索引空间，并为该分量生成一个新的 `Mesh`。
  4. 重复此过程直到所有面都被访问；分量按其起始面首次被发现的顺序被推入输出向量。
- **说明：** 只要两个三角形共享*任意*一个顶点（不必是一条边），就被视为连通，因此这是顶点邻接 BFS，而非边邻接 BFS。这是整个流水线中在堆积、优化和拆分过滤阶段普遍使用的标准连通分量拆分器——例如，在锻造或裁剪操作可能将单个输入网格破碎为多个不相连的颗粒实体之后，正是此函数将它们重新分离为可单独追踪的颗粒。
- **另请参阅：** `../algorithms/mesh-clipping-volume-fraction.md`，了解下游体积/裁剪操作如何消费所得到的各颗粒网格。

#### translate_mesh

- **签名：** `pub fn translate_mesh(mesh: &mut Mesh, delta: Vec3)`
- **源码位置：** `src/geometry/mesh_ops.rs:117`
- **用途：** 将网格的每个顶点按固定偏移量平移。
- **参数：**
  - `mesh` — 待平移的网格，原地修改。
  - `delta` — 添加到每个顶点的偏移向量。
- **返回值：** 无（`()`）。
- **副作用：** 原地修改 `mesh.vertices`。

#### move_mesh_to_target_center

- **签名：** `pub fn move_mesh_to_target_center(mesh: &mut Mesh, target: Vec3)`
- **源码位置：** `src/geometry/mesh_ops.rs:124`
- **用途：** 重新定位网格，使其质心恰好落在 `target` 上。
- **参数：**
  - `mesh` — 待移动的网格，原地修改。
  - `target` — 移动后期望的质心位置。
- **返回值：** 无（`()`）。
- **副作用：** 原地修改 `mesh.vertices`（先通过 `mesh_centroid` 计算当前质心，再以差值调用 `translate_mesh`）。

#### wrap_mesh_centroid_to_box

- **签名：** `pub fn wrap_mesh_centroid_to_box(mesh: &mut Mesh, box_bounds: BoundingBox)`
- **源码位置：** `src/geometry/mesh_ops.rs:135`
- **用途：** 在周期边界条件下，将网格质心折回堆积盒内，然后将网格移动到该折回后的位置。
- **参数：**
  - `mesh` — 待折回的网格，原地修改。
  - `box_bounds` — 周期域的包围盒。
- **返回值：** 无（`()`）。
- **副作用：** 原地修改 `mesh.vertices`。
- **说明：** 每个轴使用欧氏取模（`rem_euclid`）将质心坐标折回到 `[box_bounds.min, box_bounds.min + size)` 范围内；长度为零或负数的轴会被跳过折回处理（数值原样传递），以避免类似除以零的退化行为。当启用周期边界模式（2/3，参见 `check_boundary_constraints_mode`）时，用于使颗粒质心保持在名义域内。

#### scale_mesh

- **签名：** `pub fn scale_mesh(mesh: &mut Mesh, factor: f64)`
- **源码位置：** `src/geometry/mesh_ops.rs:156`
- **用途：** 围绕坐标原点，将网格的每个顶点按 `factor` 进行统一缩放。
- **参数：**
  - `mesh` — 待缩放的网格，原地修改。
  - `factor` — 应用于每个顶点坐标的标量乘数。
- **返回值：** 无（`()`）。
- **副作用：** 原地修改 `mesh.vertices`。
- **说明：** 缩放是围绕世界原点进行的，而非围绕网格质心——如需围绕质心缩放，调用方必须先平移到原点、缩放、再平移回去（或之后使用 `move_mesh_to_target_center`）。

#### mesh_surface_area

- **签名：** `pub fn mesh_surface_area(mesh: &Mesh) -> f64`
- **源码位置：** `src/geometry/mesh_ops.rs:163`
- **用途：** 计算三角剖分网格的总表面积。
- **参数：**
  - `mesh` — 待测量的网格。
- **返回值：** 所有面上各三角形面积（`0.5 * |ab × ac|`）之和。
- **副作用：** 无。
- **说明：** 假定三角形面引用有效的顶点索引；不会对退化或重叠的三角形去重。

#### rotate_mesh_around_center

- **签名：** `pub fn rotate_mesh_around_center(mesh: &mut Mesh, axis: Vec3, angle: f64)`
- **源码位置：** `src/geometry/mesh_ops.rs:182`
- **用途：** 使用罗德里格斯旋转公式，围绕任意轴将网格绕自身质心旋转 `angle` 弧度。
- **参数：**
  - `mesh` — 待旋转的网格，原地修改。
  - `axis` — 旋转轴；无需预先归一化（函数内部会对其进行归一化）。
  - `angle` — 旋转角度（弧度）。
- **返回值：** 无（`()`）。
- **副作用：** 原地修改 `mesh.vertices`。
- **说明：** 若 `axis` 长度 `<= 1e-12`（退化/零轴），则为空操作，以避免归一化时除以零。旋转是相对于网格质心（`mesh_centroid`）计算的，因此网格的质心位置保持不变；仅其朝向发生变化。

#### box_mesh

- **签名：** `pub fn box_mesh(bbox: BoundingBox) -> Mesh`
- **源码位置：** `src/geometry/mesh_ops.rs:203`
- **用途：** 根据给定的包围盒构建一个封闭的三角剖分矩形盒网格。
- **参数：**
  - `bbox` — 盒子的最小/最大角点。
- **返回值：** 一个具有 8 个顶点（盒子角点）和 12 个三角形（每面 2 个，共 6 面）的 `Mesh`，绕序一致。
- **副作用：** 无。
- **说明：** 常用于将堆积域本身实体化为一个网格，例如用于裁剪或可视化目的。
- **另请参阅：** `../algorithms/mesh-clipping-volume-fraction.md`。

## spatial.rs

`SpatialGrid` 是一个均匀（固定单元大小）空间哈希结构，用于在堆积和优化过程中加速邻居与碰撞查询，避免对每个其他颗粒执行 O(n²) 成对检查。

**字段：**

| 字段 | 类型 | 含义 |
|---|---|---|
| `inv_cell` | `f64` | 单元大小的倒数（`1.0 / cell_size`）；用于通过乘法（而非除法）将世界坐标转换为单元索引。 |
| `nx`、`ny`、`nz` | `usize` | 网格沿各轴的单元数，均至少为 1。 |
| `origin` | `Vec3` | 网格的世界空间原点，等于堆积盒的 `min` 角点。 |
| `cells` | `Vec<Vec<usize>>` | 展平的三维数组（行主序，大小为 `nx * ny * nz`）的单元桶；每个桶保存其包围盒与该单元重叠的条目索引。 |

#### SpatialGrid::new

- **签名：** `pub fn new(box_bounds: BoundingBox, cell_size: f64) -> Self`
- **源码位置：** `src/geometry/spatial.rs:14`
- **用途：** 构造一个覆盖 `box_bounds` 的空 `SpatialGrid`，划分为（近似）`cell_size` 大小的立方单元。
- **参数：**
  - `box_bounds` — 网格覆盖的世界空间区域。
  - `cell_size` — 目标单元边长。
- **返回值：** 一个所有单元桶均为空的新 `SpatialGrid`。`nx`/`ny`/`nz` 通过 `ceil(size / cell_size)` 计算，各自钳制在最小值 1。
- **副作用：** 无（分配一个新的 `cells` 向量）。

#### SpatialGrid::insert

- **签名：** `pub fn insert(&mut self, idx: usize, bbox: BoundingBox)`
- **源码位置：** `src/geometry/spatial.rs:31`
- **用途：** 将一个条目（由 `idx` 标识）注册到其包围盒重叠的每个网格单元中。
- **参数：**
  - `idx` — 待插入的条目索引（通常是颗粒/网格索引）。
  - `bbox` — 该条目的世界空间包围盒。
- **返回值：** 无（`()`）。
- **副作用：** 将 `idx` 推入 `self.cells` 中每个被重叠单元的桶。跨越多个单元的条目会在所有这些单元中重复出现。
- **说明：** 单元范围通过 `point_to_cell_clamped` 将 `bbox.min`/`bbox.max` 钳制为网格坐标来计算。

#### SpatialGrid::build

- **签名：** `pub fn build(bboxes: &[(usize, BoundingBox)], box_bounds: BoundingBox, cell_size: f64) -> Self`
- **源码位置：** `src/geometry/spatial.rs:49`
- **用途：** 便捷构造函数，一次性创建网格并插入一整批 (index, bbox) 对。
- **参数：**
  - `bboxes` — 待插入的 `(index, bbox)` 对切片。
  - `box_bounds` — 网格范围，传递给 `new`。
  - `cell_size` — 单元边长，传递给 `new`。
- **返回值：** 一个已填充的 `SpatialGrid`。
- **副作用：** 除分配和填充返回的网格外无其他副作用。

#### SpatialGrid::query_neighbors

- **签名：** `pub fn query_neighbors(&self, bbox: BoundingBox, exclude: usize) -> Vec<usize>`
- **源码位置：** `src/geometry/spatial.rs:58`
- **用途：** 查找与 `bbox` 重叠的单元中的所有条目索引，排除给定索引。
- **参数：**
  - `bbox` — 查询区域。
  - `exclude` — 从结果中排除的索引（通常是查询方自身）。
- **返回值：** 去重后的候选邻居索引 `Vec<usize>`。
- **副作用：** 无。
- **说明：** 委托给 `query_neighbors_with_margin`，`margin = 0.0`。
- **另请参阅：** `../algorithms/spatial-grid-collision.md`。

#### SpatialGrid::query_neighbors_with_margin

- **签名：** `pub fn query_neighbors_with_margin(&self, bbox: BoundingBox, margin: f64, exclude: usize) -> Vec<usize>`
- **源码位置：** `src/geometry/spatial.rs:68`
- **用途：** 查找与 `bbox` 重叠的单元（向外扩展 `margin`）中的所有条目索引，排除给定索引。
- **参数：**
  - `bbox` — 查询区域。
  - `margin` — 用于扩充搜索区域的额外世界空间距离（通过 `ceil(margin * inv_cell) + 1` 转换为整数单元边距）。
  - `exclude` — 从结果中排除的索引。
- **返回值：** 去重后的候选邻居索引 `Vec<usize>`（在构建结果时通过线性 `contains` 检查去重）。
- **副作用：** 无。
- **说明：** 用于需要检查 `min_neighbor_distance`（或类似间隙）约束的场合，因为即使邻居的包围盒与查询包围盒不直接重叠，只要分隔距离在 `margin` 以内，仍需被考虑。结果是 `margin` 范围内真实邻居的超集——候选项在下游仍需经过精确的几何检查（此函数仅是粗筛阶段过滤器）。
- **另请参阅：** `../algorithms/spatial-grid-collision.md`。

#### SpatialGrid::point_to_cell_clamped

- **签名：** `fn point_to_cell_clamped(&self, p: Vec3) -> (usize, usize, usize)`
- **源码位置：** `src/geometry/spatial.rs:93`
- **用途：** 将世界空间中的点映射到网格单元坐标，并钳制以确保结果始终索引一个有效单元。
- **参数：**
  - `p` — 世界空间中的点。
- **返回值：** 钳制到 `[0, nx-1] × [0, ny-1] × [0, nz-1]` 范围内的 `(cx, cy, cz)`。
- **副作用：** 无。
- **说明：** 私有辅助函数（非 `pub`）；依赖 `point_to_cell` 完成未钳制的转换。

#### SpatialGrid::point_to_cell

- **签名：** `fn point_to_cell(&self, p: Vec3) -> (usize, usize, usize)`
- **源码位置：** `src/geometry/spatial.rs:99`
- **用途：** 将世界空间中的点转换为原始（未钳制的）网格单元坐标。
- **参数：**
  - `p` — 世界空间中的点。
- **返回值：** 按 `floor((p - origin) * inv_cell)` 计算得到的 `(cx, cy, cz)`，每个轴独立地在 0 处取底（低于网格原点的点在该轴上映射到单元 0），但**不**在上界处封顶——超出网格远端边界的点在该轴上可能返回 `>= nx`/`ny`/`nz` 的索引。
- **副作用：** 无。
- **说明：** 私有辅助函数（非 `pub`）；只应使用 `point_to_cell_clamped` 来索引 `self.cells`，因为此函数的原始输出可能越界。

#### estimate_cell_size

- **签名：** `pub fn estimate_cell_size(bboxes: &[BoundingBox]) -> f64`
- **源码位置：** `src/geometry/spatial.rs:108`
- **用途：** 基于一组条目中最大的包围盒范围，为构造 `SpatialGrid` 启发式地选取单元大小。
- **参数：**
  - `bboxes` — 将要填充网格的条目的包围盒（例如某次堆积运行中所有颗粒网格的包围盒）。
- **返回值：** 所有输入盒子中的最大范围（`size.x`、`size.y` 或 `size.z`），下限为 `1.0`。若 `bboxes` 为空则返回 `1.0`。
- **副作用：** 无。
- **说明：** 将单元大小设定为最大条目的范围，可确保任何单个条目至多跨越少量、有界数量的单元，从而使 `insert`/`query_neighbors` 调用保持低成本。调用方通常为每个颗粒计算 `mesh_bbox`，将所得的盒子传入此函数以选取 `cell_size`，然后通过 `SpatialGrid::build` 构建网格。
- **另请参阅：** `../algorithms/spatial-grid-collision.md`。
