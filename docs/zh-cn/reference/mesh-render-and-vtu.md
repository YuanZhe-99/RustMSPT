# 网格生成工具 — VTU 读写与 `mesh-render` 渲染管线

网格生成工具阶段（`PLAN_mesh_generation.md` 的 GA 阶段）目前已实现部分的参考文档：契约
VTU 读写器（`src/io/vtu.rs`）、场景抽取层（`src/meshgen/render_scene.rs`）、CPU 场景渲染器
（`src/geometry/scene_render.rs`），以及 `mesh-render` 子命令
（`src/pipeline/mesh_render.rs`、`src/config/mesh_render.rs`）。

## 索引

| 条目 | 源码位置 | 摘要 |
|---|---|---|
| `VTK_POLY_LINE`/`VTK_TRIANGLE`/`VTK_TETRA`/`VTK_VOXEL`/`VTK_HEXAHEDRON` | `src/io/vtu.rs:6` | 契约子集接受的 VTK 单元类型编码。 |
| `ArrayData` | `src/io/vtu.rs:14` | 单个 DataArray 的类型化存储（U8/I32/I64/U32/U64/F32/F64），提供 `len`、`vtk_type`、`get_f64`、`get_i64` 访问器。 |
| `DataArray` | `src/io/vtu.rs:118` | 具名数组 + 分量数；`DataArray::scalar` 构造单分量数组。 |
| `VtuDoc` | `src/io/vtu.rs:138` | 内存中的契约 VTU：共享点数组、混合单元（VTK 结尾偏移量）、点/单元/字段数据；提供 `cell`、`cell_array`、`point_array`、`field_array`、`num_cells` 访问器。 |
| `VtuDoc::validate` | `src/io/vtu.rs:181` | 结构校验：偏移量单调性、索引范围、按类型的节点数、数组长度一致性。 |
| `VtuEncoding` | `src/io/vtu.rs:255` | `Ascii`（经最短形式浮点数实现精确回环）或 `AppendedRaw`（小端序，UInt64 头）。 |
| `save_vtu` | `src/io/vtu.rs:273` | 将已校验的 `VtuDoc` 写为 VTK XML UnstructuredGrid；自动创建父目录。 |
| `load_vtu` | `src/io/vtu.rs:479` | 读取契约子集（ascii + appended-raw，UInt32/UInt64 头）；对压缩或 base64 文件返回明确命名的错误。 |
| `SetKind` | `src/meshgen/render_scene.rs:8` | 渲染集合归属（`Volume` 边界面 vs 带标签的 `Face` 单元）；重合命中去重时 Face 优先。 |
| `SceneTri`/`SceneSegment`/`SceneMarker` | `src/meshgen/render_scene.rs:15` | 抽取阶段产出的世界坐标图元，携带颜色/不透明度。 |
| `RenderScene` | `src/meshgen/render_scene.rs:43` | 抽取结果：三角形、叠加线段、标记点、用于取景的整文档包围盒。 |
| `SceneFilter` | `src/meshgen/render_scene.rs:52` | 以"与"组合的过滤器：cell_kind、component、region_key、partition、regime、background、array_range、bbox、clip_plane（波浪形裁剪）。 |
| `ColorMode` | `src/meshgen/render_scene.rs:67` | 单色 / 分类调色板（整型数组）/ viridis 标量色带（浮点数组）三种着色方式；指定**点**数组时，单元取其非哨兵点值的均值。 |
| `point_array_cell_value` | `src/meshgen/render_scene.rs:273` | 将点数组归约为每单元一个值（非哨兵点值的均值）以供着色。 |
| `SceneSpec` | `src/meshgen/render_scene.rs:77` | 完整抽取规格：过滤器、着色模式、逐集合不透明度 + 按区域覆盖、叠加开关、高亮点。 |
| `categorical_color` / `scalar_color` | `src/meshgen/render_scene.rs:132` | 12 色分类调色板（哨兵值 → 灰色）与紧凑型 viridis 色带。 |
| `build_scene` | `src/meshgen/render_scene.rs:358` | VtuDoc + SceneSpec → RenderScene：过滤链、确定性的边界面/线框输出、带标签的面、曲线线段、标记点。缺失数组时的错误会指明数组名。 |
| `SceneRenderSettings` | `src/geometry/scene_render.rs:12` | 场景渲染外观设置；背景为 RGBA（alpha 为 0 表示透明 PNG）。 |
| `render_scene_cpu` | `src/geometry/scene_render.rs:95` | CPU 参考渲染器：每条像素射线做全命中 QBVH 遍历、前到后 alpha 合成、重合命中去重（Face 优先于 Volume）、带深度测试的线叠加、标记点。 |
| `named_view` | `src/geometry/scene_render.rs:287` | 将 front/back/left/right/top/bottom/iso_ne/iso_nw/iso_se/iso_sw 解析为 (view_direction, up)。 |
| `ViewSpec`/`FilterSpec` | `src/config/mesh_render.rs:7` | 视角（命名预设或自定义相机块）与按类型标记的过滤器的 YAML 形式。 |
| `MeshRenderParams`/`MeshRenderConfig` | `src/config/mesh_render.rs:38` | `mesh_render:` YAML 配置块（输入 VTU、output_dir、views、图像、着色、不透明度、过滤器、叠加、相机）。 |
| `MeshRenderPipeline` | `src/pipeline/mesh_render.rs:16` | `mesh-render` 子命令：加载 VTU → 构建场景 → 每个视角输出一张 PNG（`<stem>_<view>.png`）。 |

## GPU 预览路径（GA-3c）

`src/gpu/scene_render.rs` 与 `shaders/scene_render.wgsl` 将**同一个**
`RenderScene` 渲染为**不透明**预览。两条管线共用一个 uniform 块：

- **TriangleList**，带逐顶点颜色属性，因此预览会复现抽取层的分类/标量配色，
  而不是单一基色。其头灯着色公式与 CPU 参考实现完全一致 —— 这正是二者可以
  逐像素比对的前提。
- **LineList**，用于曲线线段与标记十字，并在顶点着色器中朝相机偏移
  （`clip.z -= bias * clip.w`），使贴合曲面的叠加线保持可见 —— 相当于 CPU
  渲染器深度偏置的 GPU 对应物。

两个阶段都通过片元 `discard` 支持可选的**裁剪平面**。注意这是*平滑*切割，
与抽取阶段按整单元执行的 `clip_plane` 过滤器（crinkle clip）相互独立；
渲染器绝不会隐式应用其中之一。

`render_views` 是**批量路径**：几何数据只构建并上传一次，供所有相机复用，
因此十个具名视图的诊断图集只需一次上传而非十次。每个视图仅重建 uniform
缓冲与渲染目标。

带标签的 Face 单元会在 Volume 边界三角形之后上传，GPU 深度比较使用
`LessEqual`；这保持了 CPU 参考实现的规则：重合的 Face 集优先于 Volume 集。

与 CPU 参考实现的两处既定差异：

| 方面 | CPU 参考实现 | GPU 预览 |
|---|---|---|
| 透明度 | 对所有命中点做精确的前到后合成 | **仅不透明**；`alpha == 0` 的三角形被丢弃，其余一律实心绘制 |
| 标记 | 屏幕空间 7 像素十字 | 三条世界空间坐标轴臂，长度为场景包围盒对角线的 1%（该管线无法表达屏幕空间十字） |

通过配置中的 `backend:` 选择 —— `cpu`（默认，即参考实现）、`gpu`
（不可用时报错）或 `auto`（有 GPU 时使用，否则静默回退到 CPU）。
验收测试 `gpu_scene_matches_cpu_within_tolerance` 要求两条路径在不透明场景上、
两种投影下，至少 98% 的像素每通道差异不超过 2 LSB；无可用适配器时优雅跳过。

## 契约 VTU 概要

权威规格文档为 `PLAN_mesh_generation.md` §7。目前已实现的子集：单个 `UnstructuredGrid`
piece；共享同一点数组的混合单元（四面体、带标签的三角面、折线曲线，读取器还接受体素单元）；
单元数组 `cell_kind`、`region_key`、`partition_id`、`regime`、`face_tag_key`、`curve_id`；
点数组 `n_id_key`、`constraint_kind`、`constraint_ref`；字段数据集合表
（`RegionSet*`、`FaceTag*`、`Curve*`）及元数据。`save_vtu` 可写出 ascii 或
appended-raw；`load_vtu` 二者皆可读取，并兼容常见的外部变体（Float32 点坐标、Int32
连接性、UInt32 头）。压缩或 base64 编码的 VTU 会被显式拒绝并报错。

## 渲染器行为说明

- **面阶段单元数组**：在不含体单元的文档（s00-s03）中，带标签面*即*网格本身，因此指定的单元数组直接为其着色、`array_range` 亦直接过滤它们，而不套用"指定数组属于四面体"时的面标签分类规则。这正是 `thin_role`、`band_region` 与 `regime` 能在阶段快照上渲染的原因；含四面体的文档不受影响，故 GA-5 体积基准图不变。

- **点场着色**：`color_by` 同时接受点数组与单元数组。此时单元取其**非哨兵点值的均值**（跳过契约的"不适用"标记 `-1` 与非有限值；若单元无有效点则以哨兵灰渲染）。这使得存放于点上的阶段场可被渲染 —— `separation_t`（S3）与 `sizing_h`（S4）—— 包括不含体单元的面阶段快照 s00-s03。标量范围在所有带值单元上自动拟合；与单元数组不同，该模式亦作用于带标签的面单元。

- **透明度**在 CPU 路径上是精确的：每条射线通过对场景 TriMesh 的 QBVH 使用
  `RayIntersectionsVisitor` 枚举全部命中，按距离排序，并以前到后的顺序合成，
  透光率饱和时提前退出。重合命中（在相对容差范围内）去重时优先保留 `Face`
  集合的三角形，因此恰好落在四面体边界面上的带标签面不会被重复叠加。
- **波浪形裁剪（crinkle clipping）**是抽取过程的自然结果：`bbox`/`clip_plane`
  过滤器丢弃单元后，幸存四面体与被丢弃四面体之间的面即成为边界面。
- **线遮挡测试**依据表面通道变为有效不透明时的深度（透光率 < 1/512），并加入
  少量朝相机方向的偏移，使贴附在表面上的曲线仍然可见；完全透明的体积不会遮挡曲线。
- **取景**始终使用整个文档的包围盒，因此同一输入的所有视角和不同过滤条件下的
  取景保持一致。
- **确定性**：由哈希表支持的边界面和线框边会在场景输出前排序，因此重复的透明度
  渲染不会依赖哈希表的迭代顺序。
- **线框永远不会被静默截断。** 先构建完整边集；只有当它超过
  `max_wireframe_edges`（默认 `DEFAULT_MAX_WIREFRAME_EDGES`，4,000,000）时才细化，
  而且是在排序后的列表上按**均匀步长**抽样，而不是到达上限就停止。前缀截断会切掉
  整片连续区域（边表按节点键排序），看上去就像网格上的空洞；步长抽样则留下均匀且
  明显稀疏的线框。`RenderScene` 报告 `wireframe_edges_total` 与
  `wireframe_edges_emitted`，`mesh-render` 在两者不同时打印 WARNING。旧的 200,000
  上限静默截断了 A-6b 和 A-7b——这正是 A-7b 渲染看起来不对的原因。
- **GA-5 回归基线**位于 `data/fixtures/meshgen/render_baselines/cpu/`：
  `good_cube.vtu` x 四个变体（`opaque`、`filtered_region_1`、
  `transparent_exterior`、`clipped_x`）x 全部十个具名视角，尺寸为 96x96。
  CPU 比较允许最多 2% 的像素超过 2 LSB。显式更新方式为：
  `RUSTMSPT_UPDATE_RENDER_BASELINES=1 cargo test --test mesh_visual_regression_tests`。
- GPU 回归将三个不透明变体与同一组 CPU 基线比较。完整夹具场景包含光栅化线框
  叠加，因此 GA-5 GPU 预算是最多 5% 的像素超过 2 LSB；更简单的 GA-3c 不透明
  一致性测试仍保持 2%。透明 GPU 输出不参与比较，因为 GPU 合成有意保持不透明。
