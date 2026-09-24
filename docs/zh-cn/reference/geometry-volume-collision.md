# 几何体积、碰撞与锻造参考

本页记录 `src/geometry/` 下的三个子模块：`volume.rs`（网格体积计算，以及用于计算包围盒内体积分数的 Sutherland-Hodgman 网格裁剪流水线）、`collision.rs`（基于 parry3d 的精确网格碰撞/距离查询，包括周期边界镜像体生成）以及 `forging.rs`（自由变形锻造仿真）。

## 索引

| 函数 | 位置 | 摘要 |
|---|---|---|
| `mesh_volume` | `src/geometry/volume.rs:6` | 通过散度定理计算封闭网格的绝对体积。 |
| `mesh_signed_volume` | `src/geometry/volume.rs:18` | 计算封闭网格的有符号体积（符号反映面的绕序）。 |
| `orient_components_to_positive_volume` | `src/geometry/volume.rs:35` | 翻转任何有符号体积为负的连通分量的绕序。 |
| `clip_plane_signed_distance` | `src/geometry/volume.rs:55` | 计算点到平面的有符号距离。 |
| `clip_segment_plane_intersection` | `src/geometry/volume.rs:60` | 计算线段与平面交点的插值坐标。 |
| `clip_polygon_with_plane` | `src/geometry/volume.rs:75` | 对凸多边形执行相对于半平面的 Sutherland-Hodgman 裁剪。 |
| `quantize_point_key` | `src/geometry/volume.rs:125` | 将点四舍五入为固定精度整数键，用于去重/哈希。 |
| `collect_triangle_plane_segment` | `src/geometry/volume.rs:139` | 提取三角形与裁剪平面相交的线段。 |
| `plane_basis` | `src/geometry/volume.rs:175` | 在垂直于法线的平面内构建一组正交基 (u, v)。 |
| `triangulate_cap_from_segments` | `src/geometry/volume.rs:211` | 从跨平面边线段三角剖分出一个平面封盖（环查找 + 扇形三角剖分）。 |
| `clip_mesh_by_plane_with_cap` | `src/geometry/volume.rs:352` | 对网格执行一次平面裁剪，并对产生的开口进行封盖。 |
| `clip_mesh_by_bbox` | `src/geometry/volume.rs:403` | 通过连续六次平面裁剪，将网格裁剪到一个轴对齐包围盒内。 |
| `particle_volume_in_bbox` | `src/geometry/volume.rs:424` | 网格裁剪到包围盒后的体积。 |
| `volume_fraction_in_bbox` | `src/geometry/volume.rs:434` | 单个网格在包围盒内的体积分数。 |
| `volume_fraction_of_meshes_in_bbox` | `src/geometry/volume.rs:444` | 多个网格在包围盒内的总体积分数（并行计算）。 |
| `to_parry_trimesh` | `src/geometry/collision.rs:29` | 将 `Mesh` 转换为 parry3d 的 `TriMesh`。 |
| `trimesh_contains_point` | `src/geometry/collision.rs:61` | 借助形状的层次包围体，以射线奇偶判定点是否位于实体内部。 |
| `mesh_surfaces_intersect_prepared` | `src/geometry/collision.rs:99` | 给定预先构建的包围盒/形状，精确判定两个网格*表面*是否相交。 |
| `mesh_solids_nested_prepared` | `src/geometry/collision.rs:150` | 判定两个闭合实体中是否有一个整体位于另一个内部。 |
| `mesh_collision_exact_prepared` | `src/geometry/collision.rs:196` | 判定两个网格*实体*是否重叠：表面相交，或一个包含另一个。 |
| `mesh_distance_exact_prepared` | `src/geometry/collision.rs:214` | 给定预先构建的包围盒/形状，进行带包围盒过滤的精确距离查询。 |
| `mesh_collision_exact` | `src/geometry/collision.rs:259` | 便捷封装：构建包围盒/形状后测试碰撞。 |
| `mesh_distance_exact` | `src/geometry/collision.rs:268` | 便捷封装：构建包围盒/形状后计算距离。 |
| `generate_periodic_ghosts` | `src/geometry/collision.rs:282` | 为周期边界碰撞生成网格的平移镜像副本。 |
| `simulate_forging_ffd` | `src/geometry/forging.rs:10` | 简单的 Z 轴 FFD 压缩加侧向鼓起。 |
| `simulate_forging_ffd_with_tracking` | `src/geometry/forging.rs:43` | 轴可配置的 FFD 锻造，带孔隙致密化与 ROI 包围盒跟踪。 |
| `forge_owned` | `src/geometry/forging.rs:66` | Ownership-consuming FFD and ROI transform. |

## volume.rs

### 公共函数

#### mesh_volume

- **签名：** `pub fn mesh_volume(mesh: &Mesh) -> f64`
- **源码位置：** `src/geometry/volume.rs:6`
- **用途：** 使用散度定理计算封闭网格的绝对（无符号）体积，即通过累加每个面与原点构成的四面体的有符号体积来实现。
- **参数：**
  - `mesh` — 待测量的网格；假定其是封闭的（watertight），否则结果不具有意义。
- **返回值：** `f64` 类型的绝对体积。对每个面 `(a, b, c)`，累加 `a · (b × c) / 6`，然后对总和取绝对值。
- **副作用：** 无。
- **说明：** 如果网格不是封闭/watertight 的，结果不是有意义的体积。由于最终求和取的是绝对值，面的绕序在此不影响结果（与 `mesh_signed_volume` 相对照）。

#### mesh_signed_volume

- **签名：** `pub fn mesh_signed_volume(mesh: &Mesh) -> f64`
- **源码位置：** `src/geometry/volume.rs:18`
- **用途：** 计算封闭网格的有符号体积；符号取决于面的绕序（按右手定则朝外的法线得到正结果，朝内得到负结果）。
- **参数：**
  - `mesh` — 待测量的网格。
- **返回值：** 有符号的 `f64` 体积；四面体求和公式与 `mesh_volume` 相同，但不做最终的 `.abs()`。
- **副作用：** 无。
- **另请参阅：** [`orient_components_to_positive_volume`](#orient_components_to_positive_volume)，该函数利用此函数的符号来检测并修正朝内的分量。

#### orient_components_to_positive_volume

- **签名：** `pub fn orient_components_to_positive_volume(mesh: &Mesh) -> (Mesh, usize, usize)`
- **源码位置：** `src/geometry/volume.rs:35`
- **用途：** 通过将网格拆分为连通分量（颗粒）并翻转有符号体积为负的分量的绕序，规范化网格朝向，使所有分量最终具有一致的朝外（正体积）法线。
- **参数：**
  - `mesh` — 待定向的网格。
- **返回值：** 元组 `(oriented_mesh, flipped_count, total_component_count)` —— 重新合并后的网格、被翻转的分量数，以及总共找到的分量数。如果网格没有分量（`split_mesh_into_granules` 返回空），则返回 `(mesh.clone(), 0, 0)`。
- **副作用：** 无（在克隆/新数据上操作并返回，不修改输入）。
- **说明：** 通过在分量的每个面上交换 `face.b` 与 `face.c` 来翻转该分量。用于在流水线其他位置进行体积/碰撞计算之前规范化朝向。
- **另请参阅：** [`mesh_signed_volume`](#mesh_signed_volume)；关于 `split_mesh_into_granules` / `merge_meshes` 见 `../reference/geometry-core.md`。

### 私有辅助函数 —— Sutherland-Hodgman 网格裁剪流水线

本文件中其余的函数共同构成了一条对三角网格进行平面裁剪（并最终裁剪到轴对齐包围盒）的流水线，同时保持结果的封闭性。数据流如下：`clip_plane_signed_distance` 相对于平面对点进行分类 → `clip_segment_plane_intersection` 计算精确的交点 → `clip_polygon_with_plane` 对每个三角形运行 Sutherland-Hodgman 裁剪以生成保留的多边形"主体" → `collect_triangle_plane_segment` 独立记录每个三角形的跨平面边 → `plane_basis` 在该平面上构建一个二维坐标系 → `triangulate_cap_from_segments` 将这些边缝合成闭合环并进行扇形三角剖分以生成封盖 → `clip_mesh_by_plane_with_cap` 将裁剪后的主体与生成的封盖合并 → `clip_mesh_by_bbox` 六次调用平面裁剪（每个包围盒面一次）以生成最终的封闭裁剪网格。`quantize_point_key` 是一个共享工具函数，通过整数哈希对浮点坐标点去重。

`#### clip_plane_signed_distance` — `fn clip_plane_signed_distance(p: Vec3, origin: Vec3, normal: Vec3) -> f64` — `src/geometry/volume.rs:55`。计算点到由 `origin`/`normal` 定义的平面的有符号距离（`(p - origin) · normal`）；正值表示法线所指的一侧。无副作用。

`#### clip_segment_plane_intersection` — `fn clip_segment_plane_intersection(a: Vec3, b: Vec3, da: f64, db: f64) -> Vec3` — `src/geometry/volume.rs:60`。给定线段 `(a, b)` 及其预先计算出的到平面的有符号距离 `da`、`db`，返回该线段与平面交点的插值坐标。若 `|da - db| <= 1e-12`（线段近似平行于平面），则退化返回 `a`。否则以 `t = clamp(da / (da - db), 0, 1)` 进行插值。无副作用。

#### clip_polygon_with_plane

- **签名：** `fn clip_polygon_with_plane(poly: &[Vec3], origin: Vec3, normal: Vec3, eps: f64) -> Vec<Vec3>`
- **源码位置：** `src/geometry/volume.rs:75`
- **用途：** 使用经典的 Sutherland-Hodgman 算法，对一个凸多边形（通常是三角形）执行相对于半平面的裁剪。
- **参数：**
  - `poly` — 有序的多边形顶点（边按循环顺序隐含）。
  - `origin`、`normal` — 裁剪平面。
  - `eps` — 用于将顶点判定为"内侧"的容差（`signed_distance >= -eps`）。
- **返回值：** 裁剪后的多边形顶点，按顺序排列；若整个多边形被裁剪掉，可能为空；在退化情形下也可能只有 0-2 个点。
- **副作用：** 无。
- **说明：** 对每条边 `(c, n)`，若 `n` 在内侧则保留，若符号发生变化则插入平面交点，否则丢弃该点。生成原始输出后，去除连续的近重合顶点（平方距离 `<= 1e-20`），并在首尾顶点重合时移除重复的闭合点。

`#### quantize_point_key` — `fn quantize_point_key(v: Vec3) -> (i64, i64, i64)` — `src/geometry/volume.rs:125`。将 `Vec3` 通过按 `1_000_000.0` 缩放每个坐标并四舍五入，量化为一个整数元组键，用于在对近似相同的浮点坐标点去重时作为 `HashMap`/`HashSet` 的键。无副作用。

#### collect_triangle_plane_segment

- **签名：** `fn collect_triangle_plane_segment(tri: [Vec3; 3], origin: Vec3, normal: Vec3, eps: f64) -> Option<(Vec3, Vec3)>`
- **源码位置：** `src/geometry/volume.rs:139`
- **用途：** 确定三角形与裁剪平面相交所沿的线段，用于构建封闭裁剪所留出的孔洞的封盖。
- **参数：**
  - `tri` — 三角形的三个顶点。
  - `origin`、`normal` — 裁剪平面。
  - `eps` — 将顶点视为恰好位于平面上的容差。
- **返回值：** 若三角形至少在两个不同点上与平面相交（或接触），返回 `Some((point_a, point_b))`；否则返回 `None`（例如三角形完全位于一侧）。
- **副作用：** 无。
- **说明：** 遍历三角形的三条边；位于平面 `eps` 范围内的顶点被直接加入，端点严格位于平面两侧的边则通过（`clip_segment_plane_intersection`）贡献一个插值交点。候选点随后去重（平方距离 `<= 1e-16`），并返回其中前两个。

`#### plane_basis` — `fn plane_basis(normal: Vec3) -> (Vec3, Vec3)` — `src/geometry/volume.rs:175`。构建一组张成垂直于 `normal` 的平面的正交基 `(u, v)`，用于将三维封盖点投影到二维以进行角度排序。选取一个切向轴（若 `|normal.x| < 0.5` 则选 `X`，否则选 `Y`），推导出 `u = normalize(normal × tangent)`，`v = normalize(normal × u)`；若任一叉积接近零（幅值 `<= 1e-12`），则使用轴对齐的回退方案。无副作用。

#### triangulate_cap_from_segments

- **签名：** `fn triangulate_cap_from_segments(segments: &[(Vec3, Vec3)], normal: Vec3) -> Mesh`
- **源码位置：** `src/geometry/volume.rs:211`
- **用途：** 给定网格三角形与裁剪平面相交处所形成的一组边线段，重建该平面上的闭合边界环，并将每个环以质心为中心进行扇形三角剖分，生成一个封闭的封盖网格。
- **参数：**
  - `segments` — 无序的 `(point_a, point_b)` 边对，每个与平面相交的三角形对应一对（来自 `collect_triangle_plane_segment`）。
  - `normal` — 裁剪平面的法线，既用于构建角度排序的二维基，也用于确定绕序方向。
- **返回值：** 包含三角剖分后的封盖表面的 `Mesh`（若 `segments` 为空则返回空 `Mesh`）。
- **副作用：** 无。
- **说明：** 算法分三个阶段：
  1. **点/边图构建。** 每个端点通过 `quantize_point_key`（经由局部闭包 `add_point`）去重，纳入一个共享的 `points` 向量。每条线段成为邻接映射（`HashMap<usize, Vec<usize>>`）与边集合中的一条无向边，跳过退化的零长度线段。
  2. **环查找。** 对每条未使用的边 `(a, b)`，代码从 `b` 出发遍历邻接图，始终选择一条未使用且不是刚刚来源的相邻边，直到返回 `a`（闭合该环）或陷入死路（此时该环被丢弃）。即使裁剪平面在网格中切出多个不相连的孔洞，这一过程也能追踪出闭合的多边形边界环。
  3. **扇形三角剖分。** 对每个闭合环（去除重复的闭合顶点后长度 >= 3），计算其质心，将环上的点投影到 `plane_basis` 的 `(u, v)` 坐标系中，并按围绕质心的 `atan2(v, u)` 角度排序，得到一致的角度顺序。然后从新加入的中心顶点向每对相邻的有序环点发射一个三角形扇，绕序方向（`ccw` 标志，由投影多边形面积与 `normal` 点积的符号决定）的选择使封盖的法线与裁剪平面的法线保持一致。
- **另请参阅：** [`clip_mesh_by_plane_with_cap`](#clip_mesh_by_plane_with_cap)，此函数是其唯一调用者。

#### clip_mesh_by_plane_with_cap

- **签名：** `fn clip_mesh_by_plane_with_cap(mesh: Mesh, origin: Vec3, normal: Vec3) -> Mesh`
- **源码位置：** `src/geometry/volume.rs:352`
- **用途：** 对整个网格执行一次平面裁剪，并对产生的开放边界进行封盖，使输出结果仍是一个封闭（watertight）的实体。
- **参数：**
  - `mesh` — 待裁剪的网格。
  - `origin`、`normal` — 裁剪平面（法线所指一侧的点被保留）。
- **返回值：** 由保留的（"保留一侧"）几何体加上覆盖切口的三角剖分封盖构成的新 `Mesh`。
- **副作用：** 无。
- **说明：** 对每个面，在该三角形上运行 `clip_polygon_with_plane`，并将结果多边形（若顶点数 >= 3）扇形三角剖分后加入输出的"主体"网格；同时对同一三角形调用 `collect_triangle_plane_segment` 以收集跨平面边（若有）。处理完所有面后，若收集到任何线段，则通过 `triangulate_cap_from_segments` 将其三角剖分为封盖，并用 `merge_meshes` 合并主体与封盖；否则直接返回主体（说明该平面未与网格相交）。
- **另请参阅：** [`triangulate_cap_from_segments`](#triangulate_cap_from_segments)、[`clip_mesh_by_bbox`](#clip_mesh_by_bbox)。

#### clip_mesh_by_bbox

- **签名：** `pub fn clip_mesh_by_bbox(mesh: &Mesh, bbox: BoundingBox) -> Mesh`
- **源码位置：** `src/geometry/volume.rs:390`
- **用途：** 通过依次对包围盒的六个面平面进行裁剪，使网格完全位于一个轴对齐包围盒内。
- **参数：**
  - `mesh` — 待裁剪的网格。
  - `bbox` — 目标轴对齐包围盒。
- **返回值：** 裁剪后的 `Mesh`；若输入完全位于 `bbox` 之外，可能是空网格。
- **副作用：** 无。
- **说明：** 构建六组 `(origin, normal)` 平面对，每个包围盒面一组（`+X`、`-X`、`+Y`、`-Y`、`+Z`、`-Z`，每个法线均指向内侧），依次应用 `clip_mesh_by_plane_with_cap`，若中间结果变为空则提前短路退出。这是完整的 Sutherland-Hodgman 流水线端到端组装：`clip_plane_signed_distance` 与 `clip_segment_plane_intersection` 提供几何基元，`clip_polygon_with_plane` 对每个平面裁剪每个三角形，`collect_triangle_plane_segment` 记录切割边，`plane_basis` 与 `triangulate_cap_from_segments` 将这些边缝合成封闭的封盖，`clip_mesh_by_plane_with_cap` 将主体与封盖粘合在一起处理一个平面 —— 应用六次（每个包围盒面一次）得到一个裁剪到包围盒内、且仍然封闭的网格，这正是在结果上调用 `mesh_volume` 才有意义的原因（未裁剪/非封闭的部分网格通过散度定理公式不会有定义良好的体积）。

  > **算法：** 关于这条裁剪加封盖流水线及其在体积分数计算中应用的完整设计原理，见 `../algorithms/mesh-clipping-volume-fraction.md`。

#### particle_volume_in_bbox

- **签名：** `pub fn particle_volume_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64`
- **源码位置：** `src/geometry/volume.rs:411`
- **用途：** 计算位于包围盒内的网格部分的体积。
- **参数：**
  - `mesh` — 网格（通常是单个颗粒/分量）。
  - `bbox` — 用于裁剪的包围盒。
- **返回值：** `clip_mesh_by_bbox(mesh, bbox)` 经由 `mesh_volume` 得到的 `f64` 体积。
- **副作用：** 无。

#### volume_fraction_in_bbox

- **签名：** `pub fn volume_fraction_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64`
- **源码位置：** `src/geometry/volume.rs:417`
- **用途：** 计算单个网格占包围盒体积的分数。
- **参数：**
  - `mesh` — 待测量的网格。
  - `bbox` — 参考包围盒。
- **返回值：** `[0, 1]` 范围内的 `f64`；委托给 `volume_fraction_of_meshes_in_bbox`，传入只有一个元素的切片。
- **副作用：** 无。

#### volume_fraction_of_meshes_in_bbox

- **签名：** `pub fn volume_fraction_of_meshes_in_bbox(meshes: &[Mesh], bbox: BoundingBox) -> f64`
- **源码位置：** `src/geometry/volume.rs:427`
- **用途：** 计算一组网格在包围盒内所占的合计体积分数 —— 这是报告堆积密度的核心指标。
- **参数：**
  - `meshes` — 参与体积求和的网格集合（例如一次堆积中的所有颗粒）。
  - `bbox` — 参考包围盒（例如堆积容器）。
- **返回值：** 限制在 `[0, 1]` 内的 `f64`：`(盒内体积之和) / max(bbox.volume(), 1e-12)`。
- **副作用：** 无（仅并行计算，不进行修改）。
- **说明：** 每个网格首先通过 `split_mesh_into_granules` 拆分为连通分量（单个网格可能代表多个不相连的颗粒体），每个分量的盒内体积通过 `particle_volume_in_bbox` 独立计算后求和；若某个网格没有拆分出分量，则回退为将整个网格视为一个颗粒处理。每个网格的计算通过 `rayon` 的 `par_iter` 并行化。
- **另请参阅：** `../algorithms/mesh-clipping-volume-fraction.md`。

## collision.rs

本模块使用一个包围盒粗筛阶段封装了 [parry3d](https://parry.rs/) 的精确三角网格相交与距离查询，构成了在堆积与优化流程中通篇使用的精确碰撞层（相对于 `bbox.rs` 中较粗略的纯 AABB 检测，见 `../reference/geometry-core.md`）。

#### to_parry_trimesh

- **签名：** `pub fn to_parry_trimesh(mesh: &Mesh) -> Option<TriMesh>`
- **源码位置：** `src/geometry/collision.rs:29`
- **用途：** 将内部的 `Mesh` 转换为 `parry3d_f64::shape::TriMesh`，用于精确几何查询。
- **参数：**
  - `mesh` — 待转换的网格。
- **返回值：** 成功时返回 `Some(TriMesh)`；若网格没有面/顶点、任一顶点索引超出 `u32::MAX`，或 `TriMesh::new` 本身失败（例如网格退化），则返回 `None`。
- **副作用：** 无。
- **说明：** parry3d 的 `TriMesh` 使用 `u32` 索引，因此超过约 40 亿个顶点的网格无法转换；此项针对每个索引显式检查。

#### trimesh_contains_point

- **签名：** `pub fn trimesh_contains_point(shape: &TriMesh, p: Vec3) -> bool`
- **源码位置：** `src/geometry/collision.rs:61`
- **用途：** 借助闭合表面自身的层次包围体，判定一个点是否位于该表面内部。
- **参数：**
  - `shape` — 已建立索引的表面。
  - `p` — 查询点。
- **返回值：** 点位于内部时为 `true`。
- **副作用：** 无。
- **说明：** 在 QBVH 上做射线奇偶遍历，使用全 crate 统一的 `RAY_DIR` 与 `HIT_EPS`——与 [`point_inside_mesh`](geometry-analysis.md#point_inside_mesh) 相同的固定非轴对齐方向与 `1e-8` 命中容差，因此层次测试与扫描测试逐点一致；`tests/collision_tests.rs` 对此有断言。采用奇偶而非伪法向测试：奇偶对嵌套壳是正确的，且不关心面的绕向——这一点很重要，因为 `box_mesh` 绕向朝内，而真实 STL 数据绕向朝外。`VoidIndex::contains_point` 在其自身的包围盒预检之后委托到此处。
- **另请参阅：** [`point_inside_mesh`](geometry-analysis.md#point_inside_mesh)、[`mesh_solids_nested_prepared`](#mesh_solids_nested_prepared)。

#### mesh_surfaces_intersect_prepared

- **签名：** `pub fn mesh_surfaces_intersect_prepared(a_bbox: Option<BoundingBox>, a_shape: Option<&TriMesh>, b_bbox: Option<BoundingBox>, b_shape: Option<&TriMesh>) -> bool`
- **源码位置：** `src/geometry/collision.rs:99`
- **用途：** 在给定两个网格预先计算好的包围盒与 parry3d 形状的情况下，先经过包围盒粗筛阶段，再执行精确的精筛阶段测试，判断两个网格*表面*是否相交。
- **参数：**
  - `a_bbox`、`b_bbox` — 两个网格预先计算好的包围盒。
  - `a_shape`、`b_shape` — 两个网格预先计算好的 parry3d `TriMesh` 形状。
- **返回值：** 若表面相交则为 `true`。若任一包围盒缺失，或（包围盒检查通过后）任一形状缺失，或 `parry3d` 的 `query::intersection_test` 本身出错（`.unwrap_or(true)`），则保守地返回 `true`。
- **副作用：** 无。
- **说明：** 首先检查 `bbox_overlaps(a_bbox, b_bbox)` 作为廉价的提前退出；只有通过该检查后才以恒等等距变换运行精确的 parry3d 相交测试（假定网格已经处于世界空间坐标系下）。**它本身不是碰撞检测**：一个闭合表面整体位于另一个内部时，两者从不相交。除非确实只关心表面这一问题，否则应调用 `mesh_collision_exact_prepared`。
- **另请参阅：** [`bbox_overlaps`](geometry-core.md#bbox_overlaps)、[`mesh_collision_exact_prepared`](#mesh_collision_exact_prepared)。

#### mesh_solids_nested_prepared

- **签名：** `pub fn mesh_solids_nested_prepared(a_bbox: Option<BoundingBox>, a_shape: Option<&TriMesh>, b_bbox: Option<BoundingBox>, b_shape: Option<&TriMesh>) -> bool`
- **源码位置：** `src/geometry/collision.rs:150`
- **用途：** 判定两个闭合实体中是否有一个整体位于另一个内部。
- **参数：**
  - `a_bbox`、`b_bbox` — 预先计算好的包围盒。
  - `a_shape`、`b_shape` — 预先计算好的 parry3d 形状。
- **返回值：** 任一实体包含另一个时为 `true`。当包围盒或形状缺失时返回 `false` 而非 `true`，因为调用方的表面检测对该情形已经给出了保守答案。
- **副作用：** 无。
- **说明：** 这正是表面相交看不到、而表面距离会当作宽裕间隙报出的情形：`query::distance` 跨越两个表面之间的空隙度量，于是半径 1 的球体位于半径 3 的球体中心时读数为 `1.96`。包围盒比较（容差 `1e-9`）先行，并解决几乎所有配对，因为位于另一个内部的实体其包围盒必然位于对方之内；只有在一个包围盒确实包含另一个时，才需要一次 `trimesh_contains_point` 射线投射。在两个表面不相交的前提下，**一个顶点即可定论**：严格位于另一个闭合壳内部的闭合壳，其**全部**顶点都在其内。用顶点，绝不用质心——非凸壳的质心可能落在其自身实体之外、落在某个凹陷中，而那里是另一个颗粒可以合法占据的位置。调用方应与 `mesh_surfaces_intersect_prepared` 配对使用，后者提供该前提。
- **另请参阅：** [`trimesh_contains_point`](#trimesh_contains_point)、[`mesh_collision_exact_prepared`](#mesh_collision_exact_prepared)。

#### mesh_collision_exact_prepared

- **签名：** `pub fn mesh_collision_exact_prepared(a_bbox: Option<BoundingBox>, a_shape: Option<&TriMesh>, b_bbox: Option<BoundingBox>, b_shape: Option<&TriMesh>) -> bool`
- **源码位置：** `src/geometry/collision.rs:196`
- **用途：** 判定两个网格*实体*是否重叠——表面相交，或一个包含另一个。
- **参数：**
  - `a_bbox`、`b_bbox` — 两个网格预先计算好的包围盒。
  - `a_shape`、`b_shape` — 两个网格预先计算好的 parry3d `TriMesh` 形状。
- **返回值：** 实体重叠时为 `true`；数据缺失时保守返回 `true`，该行为继承自 `mesh_surfaces_intersect_prepared`。
- **副作用：** 无。
- **说明：** 即 `mesh_surfaces_intersect_prepared || mesh_solids_nested_prepared`，且按此顺序——廉价的表面检测会在嵌套检测产生任何开销之前排除几乎所有配对。此处的"碰撞"指两个实体共享空间，而非两个表面相交；这是两个不同的问题，只问前者是一个真实的缺陷。直到 v0.2.0 为止，本函数*只是*前者，于是两个堆积引擎都会把颗粒整个放进别的颗粒里面：一次 3 µm 到 45 µm 粒径范围的 `placement:` 运行，146 个颗粒中有 28 个位于另一个颗粒内部，并在重复计入域体积 2.1% 的分数上报告 `target_reached`。
- **另请参阅：** [`mesh_surfaces_intersect_prepared`](#mesh_surfaces_intersect_prepared)、[`mesh_solids_nested_prepared`](#mesh_solids_nested_prepared)。

#### mesh_distance_exact_prepared

- **签名：** `pub fn mesh_distance_exact_prepared(a_bbox: Option<BoundingBox>, a_shape: Option<&TriMesh>, b_bbox: Option<BoundingBox>, b_shape: Option<&TriMesh>) -> f64`
- **源码位置：** `src/geometry/collision.rs:214`
- **用途：** 在给定预先计算好的包围盒与 parry3d 形状的情况下，以包围盒距离作为快速路径，计算两个网格间的最小欧氏距离。
- **参数：**
  - `a_bbox`、`b_bbox` — 预先计算好的包围盒。
  - `a_shape`、`b_shape` — 预先计算好的 parry3d 形状。
- **返回值：** `>= 0.0` 的距离。若任一包围盒缺失（回退情形，并非真正的"接触"信号），或两网格被判定为重叠，则返回 `0.0`。
- **副作用：** 无。
- **说明：** 逻辑为：计算 `bbox_d = bbox_distance(a_bbox, b_bbox)`。若 `bbox_d > 0.0`（包围盒分离），则精确形状距离至少为 `bbox_d`，因此计算形状间的 `query::distance`（若形状缺失或查询出错则回退为 `bbox_d`）——包围盒分离即保证网格不重叠，因而跳过碰撞检查。若 `bbox_d == 0.0`（包围盒接触或重叠），则必须通过 `mesh_collision_exact_prepared` 检查实际是否重叠；若确实重叠，返回 `0.0`；否则回退到精确的 `query::distance` 调用（若形状缺失或查询失败，默认返回 `0.0`）。因此**嵌套**配对报出 `0.0`，而非两个表面之间的空隙：嵌套蕴含包围盒重叠，故快速路径无法绕过碰撞调用，而该调用如今返回 `true`。
- **另请参阅：** [`bbox_distance`](geometry-core.md#bbox_distance)、[`mesh_collision_exact_prepared`](#mesh_collision_exact_prepared)。

#### mesh_collision_exact

- **签名：** `pub fn mesh_collision_exact(a: &Mesh, b: &Mesh) -> bool`
- **源码位置：** `src/geometry/collision.rs:259`
- **用途：** 便捷封装函数，为两个网格即时计算包围盒与 parry3d 形状，然后测试碰撞。
- **参数：**
  - `a`、`b` — 待测试的两个网格。
- **返回值：** `bool`，规则同 `mesh_collision_exact_prepared`。
- **副作用：** 无。
- **说明：** 每次调用都会为两个网格重新计算 `mesh_bbox` 与 `to_parry_trimesh`；对同一批网格反复查询的调用方应优先使用带预构建包围盒/形状的 `mesh_collision_exact_prepared`，以避免重复计算。
- **另请参阅：** [`mesh_collision_exact_prepared`](#mesh_collision_exact_prepared)。

#### mesh_distance_exact

- **签名：** `pub fn mesh_distance_exact(a: &Mesh, b: &Mesh) -> f64`
- **源码位置：** `src/geometry/collision.rs:268`
- **用途：** 便捷封装函数，为两个网格即时计算包围盒与 parry3d 形状，然后计算它们的精确距离。
- **参数：**
  - `a`、`b` — 待测量的两个网格。
- **返回值：** `f64` 距离，规则同 `mesh_distance_exact_prepared`。
- **副作用：** 无。
- **另请参阅：** [`mesh_distance_exact_prepared`](#mesh_distance_exact_prepared)。

#### generate_periodic_ghosts

- **签名：** `pub fn generate_periodic_ghosts(mesh: &Mesh, box_bounds: BoundingBox) -> Vec<Mesh>`
- **源码位置：** `src/geometry/collision.rs:282`
- **用途：** 生成网格的平移"镜像"副本，沿各轴组合按堆积盒的尺寸偏移，以支持周期边界条件下的碰撞检测（靠近盒子一个面的颗粒可能与靠近对面的颗粒发生碰撞）。
- **参数：**
  - `mesh` — 待生成镜像的源网格。
  - `box_bounds` — 周期性域的包围盒；其尺寸决定了偏移距离。
- **返回值：** 镜像副本组成的 `Vec<Mesh>`。若 `mesh_bbox(mesh)` 为 `None`（空网格），立即返回空向量。
- **副作用：** 无（每个镜像都是全新的克隆；输入网格不受影响）。
- **说明：** 遍历 x/y/z 上 `{-1, 0, 1}` 偏移的全部 27 种组合（跳过 `(0,0,0)` 恒等情形，剩下 26 个候选镜像），按每轴 `(x, y, z) * box_bounds.size()` 进行偏移。只有当偏移后的包围盒在三个轴上都与 `box_bounds` 重叠（两个方向均为严格不等式）时，该镜像才会被保留，因此只有可能与盒内内容交互的镜像才会被实际生成。专门用于周期边界模式 3 的碰撞处理。

  > **算法：** 关于此函数所支持的周期边界碰撞（模式 3）的完整设计，见 `../algorithms/spatial-grid-collision.md`。

## forging.rs

这两个函数都实现了自由变形（FFD）风格的锻造仿真：顶点相对于一个参考中心进行重新缩放，沿选定的轴压缩，同时侧向鼓起，以近似模拟锻造步骤中体积守恒的塑性变形。

#### simulate_forging_ffd

- **签名：** `pub fn simulate_forging_ffd(mesh: &Mesh, compression_ratio: f64, bulge_factor: f64) -> Mesh`
- **源码位置：** `src/geometry/forging.rs:10`
- **用途：** 应用一个简单的 FFD 风格锻造变形：围绕网格自身包围盒的中心，沿 Z 轴压缩网格并使其在侧向（X/Y）鼓起。
- **参数：**
  - `mesh` — 待变形的网格。
  - `compression_ratio` — 要去除的 Z 方向范围的比例，大致处于 `[0, 1)`（内部会被限幅）；`0` 表示不压缩。
  - `bulge_factor` — 控制施加多少体积守恒的侧向膨胀，`[0, 1]`（内部会被限幅）；`0` 表示无侧向鼓起（纯粹压扁），`1` 表示完全的 `1/sqrt(axis_scale)` 鼓起。
- **返回值：** 一个新的、经过变形的 `Mesh`（输入的克隆，顶点已被变换）。
- **副作用：** 无。
- **说明：** 使用 `mesh_bbox(mesh)` 确定变形中心，若网格为空则回退到单位盒 `[0,0,0]..[1,1,1]`。`axis_scale = clamp(1 - compression_ratio, 0.01, 1.0)`（Z 方向缩放），`lateral_scale = (1 / axis_scale)^bulge_factor.clamp(0,1)`（X/Y 方向缩放）。每个顶点相对于 `center` 按 `(x*lateral_scale, y*lateral_scale, z*axis_scale)` 变换。仅支持固定的 Z 轴压缩 —— 可配置轴的版本见 `simulate_forging_ffd_with_tracking`。
- **另请参阅：** [`simulate_forging_ffd_with_tracking`](#simulate_forging_ffd_with_tracking)。

#### simulate_forging_ffd_with_tracking

- **签名：** `pub fn simulate_forging_ffd_with_tracking(mesh: &Mesh, lattice_bbox: BoundingBox, track_bbox: Option<BoundingBox>, compression_ratio: f64, compression_axis: usize, bulge_factor: f64, mesh_type: &str, void_densification: f64) -> (Mesh, Option<BoundingBox>)`
- **源码位置：** `src/geometry/forging.rs:43`
- **用途：** 比 `simulate_forging_ffd` 更通用的 FFD 锻造变形：压缩轴可配置，标记为孔隙的网格会额外经过一次闭合（致密化）处理，且一个可选的感兴趣区域（ROI）包围盒会随同一变换一起跟踪。
- **参数：**
  - `mesh` — 待变形的网格。
  - `lattice_bbox` — 其中心定义变形原点的包围盒（与网格自身的包围盒不同 —— 通常是周围的晶格/孔隙单元盒）。
  - `track_bbox` — 一个可选的感兴趣区域盒，随网格一同变换（例如用于跟踪某个孔隙或特征区域如何移动/变形）。
  - `compression_ratio` — 要去除的压缩轴方向范围的比例（内部限幅到 `[0.01, 1.0]` 的缩放，公式与 `simulate_forging_ffd` 相同）。
  - `compression_axis` — 压缩所用的轴：`0` = X，`1` = Y，其他任何值（包括 `2`）= Z。
  - `bulge_factor` — 控制两个未压缩轴上的侧向膨胀，`[0, 1]` 限幅。
  - `mesh_type` — 若此字符串不区分大小写地等于 `"void"`，则在主 FFD 变换之后应用一次额外的基于质心的闭合/致密化处理。
  - `void_densification` — 控制孔隙网格闭合力度的缩放因子；仅在 `mesh_type` 为 `"void"` 时使用。
- **返回值：** 元组 `(deformed_mesh, tracked_bbox)`：变形后的网格，以及 —— 若 `track_bbox` 为 `Some` —— 由原始 ROI 盒的全部 8 个变换后角点重新推导出的变换后轴对齐包围盒（若 `track_bbox` 为 `None` 则为 `None`）。
- **副作用：** 无。
- **说明：**
  - `center` 由 `lattice_bbox`（而非网格自身的包围盒）推导得出，这样同一变形原点可以在属于同一晶格的多个网格/孔隙单元之间保持一致共享。
  - `axis_scale` / `lateral_scale` 使用与 `simulate_forging_ffd` 相同的限幅公式。
  - `transform_point` 是一个捕获 `center`、`axis_scale`、`lateral_scale` 和 `compression_axis` 的闭包；它将 `axis_scale` 应用于压缩轴，将 `lateral_scale` 应用于另外两个轴，通过 `match compression_axis { 0 => .., 1 => .., _ => .. }` 匹配 —— 注意 `_` 分支（Z）既会被 `compression_axis == 2` 命中，也会被任何其他未识别的值命中，即对于未知的轴索引，Z 轴压缩是隐式的默认值。
  - 在主顶点变换之后，若 `mesh_type.eq_ignore_ascii_case("void")`，则执行第二遍处理，按 `closure = clamp(1 - 0.05 * compression_ratio * void_densification, 0.85, 1.0)` 将网格朝其自身质心（变换后通过 `mesh_centroid` 计算）收缩 —— 这模拟了锻造压力下孔隙的闭合，其上限确保孔隙闭合幅度永远不超过 15%。
  - ROI 跟踪：若 `track_bbox` 为 `Some`，盒子的全部 8 个角点会分别通过同一个 `transform_point` 闭包（确保与网格变形保持一致，并在压缩轴变化后正确处理轴交换，因为盒子自身的 AABB 角点在压缩轴变化后不能简单地按轴缩放），然后重新计算变换后角点的最小/最大值以形成新的 AABB。这种"变换角点后重新求包围盒"的方法之所以必要而不是直接缩放 `track_bbox`，是因为该映射混合了居中、按轴选择缩放因子等操作，并且（隐式地）只有在没有旋转的情况下才保持轴对齐 —— 但鉴于按轴条件式的缩放分配，从变换后的角点重新推导 AABB 仍然是更稳健的方法。
  - 孔隙致密化处理**不**应用于被跟踪的 ROI 盒（仅应用于网格顶点），因此 `track_bbox` 仅反映致密化之前的 FFD 变换结果。
- **另请参阅：** [`simulate_forging_ffd`](#simulate_forging_ffd)，更简单的仅 Z 轴版本。

  > **算法：** 关于 FFD 锻造模型、轴选择与孔隙致密化启发式方法背后的完整设计原理，见 `../algorithms/ffd-forging.md`。

### Owned CPU transforms (PERF-16)

`forge_owned(mesh, lattice_bbox, track_bbox, compression_ratio, compression_axis, bulge_factor, mesh_type, void_densification)` consumes the mesh and applies the existing tracked FFD mapping. The public borrowed wrapper clones once and delegates. ForgePipeline moves its input into this entry, retains the already computed input bbox, removes unused whole-mesh volume scans, and moves the output when orientation is disabled. ScalePipeline likewise moves its transformed mesh when orientation is disabled. Both log transform-only seconds separately from I/O.

`map_vertices` keeps small slices serial and maps disjoint 8192-vertex blocks on the current Rayon pool only with multiple workers and at least max(131072, workers * 65536) vertices. Scale, translate and both FFD variants use it. Each vertex retains its arithmetic order; void centroid is still accumulated serially after the affine pass. ROI remains unaffected by void closure. Clipped ROI VF and output bbox are still measured from actual geometry; no determinant approximation is substituted.

### 无实际切割时的体积快路径

私有平面裁剪器消费 mesh，按面引用的顶点和既有 distance >= -1e-9 判据分类。全部在内（包括相切）直接返回原分配，不生成封口；全部在外返回空；混合情况保留既有多边形/封口算法。修复完全位于域内且贴面时的重复封口，未被面引用的外部顶点不产生切割。clip_mesh_by_bbox 移动中间 mesh；particle_volume_in_bbox 对所有存储顶点都在盒内的情况直接算原网格体积，避免克隆与六次裁剪。测试覆盖内/外绕序、平移、重合边界、外向盒部分交集、未引用外部顶点。此改动不替代 legacy 部分切割封口算法，也未证明其对任意非凸/嵌套截面正确。
