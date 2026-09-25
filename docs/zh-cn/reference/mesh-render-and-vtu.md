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
| `MeshRenderPipeline` | `src/pipeline/mesh_render.rs:18` | `mesh-render` 子命令：加载 VTU → 构建场景 → 每个视角输出一张 PNG（`<stem>_<view>.png`）。 |

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

### PERF-19 regression entry

GPU 场景测试样例显式初始化两个线框计数字段，并在 GPU 特性下导入 `SceneSegment`。
运行 `cargo test --offline --features gpu --test mesh_render_tests` 可验证现有不透明预览路径，
图像基线和渲染语义未改变。无适配器时跳过测试不等于完成硬件验证。


### Prepared CPU scenes (PERF-17)

`geometry::scene_render::PreparedScene::new(&RenderScene)` builds immutable QBVH and material arrays once, borrowing the scene to prevent mutation during reuse. `render(&self, camera, width, height, settings)` retains the all-hits, sorted transparency and Face-over-Volume coincidence rules. Rayon task-local hit vectors retain capacity between pixels; deduplication compacts the same vector without a second allocation. `render_scene_cpu` remains the compatible one-shot wrapper. MeshRenderPipeline prepares once only on the CPU path and reuses across views. The command uses `render_views_to` to deliver owned GPU images to the bounded writer described below, retaining at most two delivered images during overlap. The compatibility `render_views` API explicitly collects images for callers requiring a batch.


### Streamed GPU views

`GpuScenePipeline::render_views_to(..., consume)` uploads scene geometry once and invokes a fallible consumer with `(view_index, owned_image)` in camera order. It reuses one uniform buffer, color/depth pair and unmapped staging buffer, clearing targets for each view. Consumer errors stop immediately; mapping errors and device errors propagate. Dimensions and staging/vertex limits are checked. `render_views` collects this stream for compatibility. The mesh-render command consumes owned frames in order; output errors never trigger CPU fallback. An auto-mode GPU failure may occur after earlier views were saved: CPU fallback rewrites all requested views in order. A strict GPU error preserves already-saved views. GPU PNG overlap now follows the bounded writer contract below.

### CPU 线程预算

与 mesh_render 同级的可选 cpu_max 接受整数或整数字符串。缺省/-1 使用可用 CPU，其余夹取 1..available。场景准备、所有 CPU 视图及 GPU 失败回退均进入同一个 Rayon 池，日志记录请求与实际线程数及 CPU 渲染入口的线程索引。backend: gpu 仍为严格不透明预览；auto 可回退透明 CPU 参考路径，回退不再建池。

| `MeshRenderPipeline::with_worker_pool` | `src/pipeline/mesh_render.rs:193` | Execute scene preparation, rendering and fallback within the worker budget. |

| `MeshRenderPipeline::run_in_pool` | `src/pipeline/mesh_render.rs:218` | Execute scene preparation, rendering and fallback within the worker budget. |

RUSTMSPT_ACCELERATION=cpu|gpu|auto 在加载输入前覆盖已验证的 YAML backend；非法环境值报错。生效的 gpu 仍严格拒绝失败，auto 允许回退，cpu 不初始化 GPU。此处未把 STL 渲染的像素阈值套用于既有 mesh 预览。

### Scene 预览工作集策略

mesh_render.gpu_memory_limit_mb 可选限制逻辑 GPU 工作集；gpu_min_pixels 仅用于 auto，缺省 0 保留既有预览选择。低于阈值的 auto 不初始化 GPU；显式 GPU 绕过阈值但遵守预算。预算/执行失败时 auto 回退，gpu 报错，CPU 跳过 GPU 专属资源选项。

checked planner 按可见三角形每面 120、启用 segment 每条 64、marker 每个 192 字节计数；目标 color/depth 为 8×pixels，readback 为按 256 字节对齐的 RGBA 行，uniform 128 字节。保守计入待完成 queue 上传，逻辑峰值 = 2×geometry_bytes+256+8×pixels+staging_bytes。多个视图共用目标。驱动/pipeline 内部及主存场景/PNG 不计入，所以这不是物理 VRAM/RSS 上限。独立 device 限制检查先于 host 顶点展开，上传后即释放临时 host 顶点。构造器返回受作用域保护的 GPU 验证/分配错误；图像分块仍待实现。

### CPU 像素任务划分

STL 最近命中和 prepared 透明 scene 渲染均使用不重叠连续像素任务。当行数不少于 worker 且每 worker 不超过 1024 像素时保留按行，避免最小块粒度减少并行任务；其他图单 worker 为单任务，否则目标为每 worker 四个任务，粒度夹取 256..4096 像素，整行能放下时按行对齐。宽行可跨任务，短行可合并。任务仅在起点计算 (x,y)，随后递增原整数像素坐标，射线运算、命中排序/合成与串行叠加不变。scene depth/RGBA 共用边界并复用任务内 hits scratch。两种投影、非整除尺寸、极端纵横比在 1/2/8 worker 下逐字节对照按行参考。粒度性能验收见 PLAN.Performance.md §40。

### GPU PNG 有界写出（2026-09-23）

GPU 多视图且配置 worker 数大于 1 时，`consume_frames` 通过零容量通道按顺序将图像所有权移交给单个 PNG writer。最多一张图像正在编码、一张由渲染生产端持有；GPU 渲染目标继续复用。返回或 CPU 回退前，必须等待 writer 退出并排空已接收帧。输出错误优先于 GPU 错误，不触发回退；即使 producer 成功，末帧写出错误也不会丢失。单 worker 或单视图保持顺序执行；这证明有界重叠机制，不代表端到端提速；软件 GPU 冷进程 CLI 对照（含 PNG 身份与 RSS）见 PLAN.Performance.md §56；完整负载和硬件验收仍开放。

### CPU PNG 写出重叠（PERF-17，2026-09-25）

**构建与驻留计数（PERF-17，2026-09-25）。** CPU 路径打印 `[mesh-render] cpu scene_qbvh_builds=<n> views=<n> max_live_images=<n>`。`scene_qbvh_builds` 是进程级计数 `geometry::scene_render::scene_qbvh_build_count()`（每个含几何的 `PreparedScene` 加一）在准备前后的差值，因此无论渲染多少视图都为 1；`max_live_images` 由 `render_and_write_overlapped` 返回（单视图为 1，写出与渲染重叠后为 2）。`mesh_render_cli_worker_budget_and_fallback` 在 1/2/8 worker 下断言 `scene_qbvh_builds=1 views=2 max_live_images=2`。GPU 路径每个 `GpuScenePipeline` 只上传一次，其主机帧由会合式 writer 限制（最多两帧，§56）。

CPU 视图改由 `render_and_write_overlapped` 处理：第 *i* 个视图在 `rayon::join` 中渲染（内部仍按像素并行），同时第 *i-1* 个视图在同一线程池的另一个 worker 上进行 PNG 编码和写盘；因此最多存在两张已完成或正在渲染的图像，写出仍按视图顺序。写出错误在并发开始的那次渲染结束后立即返回；之后的视图不再渲染或写出，CPU 写出错误也不会触发任何回退。单 worker 时 `join` 依次内联执行渲染与写出，即原来的顺序循环。测试在 1/2/4 worker、0/1/2/7 个视图下与顺序循环比较 PNG 字节和顺序，检查首/中/末帧写错（且最多多渲染一个视图），并证明下一视图的渲染在上一张写出完成前开始。GPU writer 保留 `consume_frames`：其生产者由回调驱动（`render_views_to` 推送帧），逐帧 `rayon::join` 需要拉取式 GPU API，而 rayon scope 加阻塞交接会让线程池 worker 阻塞在通道上；其阻塞/排空/回退顺序由现有测试覆盖，本次未改动。忽略的 release 基准 `cpu_overlap_benchmark`（level-5 icosphere 的 8 个视图，预热后 5 个样本，4 核共享机器且有并发构建）未发现可靠差异：256²/1024²/2048² 及 1/2/4 worker 下重叠/顺序中位比值为 0.95～1.23，样本离散度大于差值，原因是 PNG 编码只占 CPU 光线投射时间的一小部分。Auto 后端选择：STL `render` 已通过 `resolve_execution` 使用 `acceleration.gpu_min_pixels`（默认 250,000）；`mesh-render` 默认仍为 `gpu_min_pixels: 0`，因为把小尺寸 `auto` 渲染切到 CPU 会改变图像（CPU 为透明度参考，GPU 为不透明预览），而不仅是成本。

### 不透明场景最近命中组（2026-09-23）

准备后的场景至少有 192 个三角形、且每个 clamp 后的 alpha 都恰为 1 时，先用 QBVH 求最近距离，再以向外取整的 `nearest + 2 * dedup_tol` 为界枚举附近命中。保留原距离/triangle ID 排序及移动锚点的 Face-over-Volume 去重，只取首组；所选法线、颜色和深度交给原合成器与覆盖线逻辑。更小场景或包含零/部分/NaN alpha 时维持全命中枚举。三角形阈值避开了实测 24 三角形场景的双遍历退化，只是保守启发式，不能保证所有空间布局提速。分层场景 release 对照和限制见 PLAN.Performance.md §57。
