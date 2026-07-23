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
| `ColorMode` | `src/meshgen/render_scene.rs:67` | 单色 / 分类调色板（整型数组）/ viridis 标量色带（浮点数组）三种着色方式。 |
| `SceneSpec` | `src/meshgen/render_scene.rs:77` | 完整抽取规格：过滤器、着色模式、逐集合不透明度 + 按区域覆盖、叠加开关、高亮点。 |
| `categorical_color` / `scalar_color` | `src/meshgen/render_scene.rs:132` | 12 色分类调色板（哨兵值 → 灰色）与紧凑型 viridis 色带。 |
| `build_scene` | `src/meshgen/render_scene.rs:358` | VtuDoc + SceneSpec → RenderScene：过滤链、对选中四面体子集的边界面抽取、带标签的面、曲线线段、线框、标记点。缺失数组时的错误会指明数组名。 |
| `SceneRenderSettings` | `src/geometry/scene_render.rs:12` | 场景渲染外观设置；背景为 RGBA（alpha 为 0 表示透明 PNG）。 |
| `render_scene_cpu` | `src/geometry/scene_render.rs:95` | CPU 参考渲染器：每条像素射线做全命中 QBVH 遍历、前到后 alpha 合成、重合命中去重（Face 优先于 Volume）、带深度测试的线叠加、标记点。 |
| `named_view` | `src/geometry/scene_render.rs:287` | 将 front/back/left/right/top/bottom/iso_ne/iso_nw/iso_se/iso_sw 解析为 (view_direction, up)。 |
| `ViewSpec`/`FilterSpec` | `src/config/mesh_render.rs:7` | 视角（命名预设或自定义相机块）与按类型标记的过滤器的 YAML 形式。 |
| `MeshRenderParams`/`MeshRenderConfig` | `src/config/mesh_render.rs:38` | `mesh_render:` YAML 配置块（输入 VTU、output_dir、views、图像、着色、不透明度、过滤器、叠加、相机）。 |
| `MeshRenderPipeline` | `src/pipeline/mesh_render.rs:16` | `mesh-render` 子命令：加载 VTU → 构建场景 → 每个视角输出一张 PNG（`<stem>_<view>.png`）。 |

## 契约 VTU 概要

权威规格文档为 `PLAN_mesh_generation.md` §7。目前已实现的子集：单个 `UnstructuredGrid`
piece；共享同一点数组的混合单元（四面体、带标签的三角面、折线曲线，读取器还接受体素单元）；
单元数组 `cell_kind`、`region_key`、`partition_id`、`regime`、`face_tag_key`、`curve_id`；
点数组 `n_id_key`、`constraint_kind`、`constraint_ref`；字段数据集合表
（`RegionSet*`、`FaceTag*`、`Curve*`）及元数据。`save_vtu` 可写出 ascii 或
appended-raw；`load_vtu` 二者皆可读取，并兼容常见的外部变体（Float32 点坐标、Int32
连接性、UInt32 头）。压缩或 base64 编码的 VTU 会被显式拒绝并报错。

## 渲染器行为说明

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
- GPU 预览路径（PLAN GA-3c）尚未实现；本迭代中 `mesh-render` 仅支持 CPU 渲染。
