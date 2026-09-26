# STL Rendering

`render` 流水线无需窗口即可把 STL 网格渲染为 PNG。`focus_point` 是画面中心目标点，`view_direction` 从相机指向该点；可选 `up_vector` 用于确定画面旋转方向，平行时会自动选择备用轴。

正交投影将网格包围盒八个角投影到相机平面，按输出宽高比和 `fit_padding` 自动取景。透视投影使用垂直 FOV；未指定 `camera_distance` 时，根据包围球自动确定相机距离。

CPU 路径通过 parry3d `TriMesh` 的 QBVH 为每个像素查询最近交点，并由 rayon 按行并行。GPU 路径使用 `GpuRenderPipeline` 将三角形离屏光栅化到颜色和深度纹理，再通过 256 字节对齐的 staging buffer 读回。GPU 失败时回退 CPU。

两条路径共享相机与着色定义。CPU 为 f64 光线投射，GPU 为 f32 光栅化，因此测试允许边缘约 1 像素和通道约 1 LSB 的差异。参见[英文完整算法说明](../../en-us/algorithms/stl-rendering.md)。


### Execution policy (2026-09-21)

The entire STL render pipeline runs inside its `cpu_max` pool. `RUSTMSPT_ACCELERATION` overrides the configured mode, with invalid values rejected. Auto uses `gpu_min_pixels`; explicit GPU bypasses this threshold. Device budget estimation includes expanded triangle vertices (72 bytes/face), color/depth textures (8 bytes/pixel), 256-byte-aligned readback rows and 128 uniform bytes. GPU options use the common execution validator. Device texture/buffer limits are checked before allocation, and scoped validation/allocation errors plus checked readback propagate through the fallback policy. Per-call resources are still allocated afresh; persistent scene/target caching remains pending.

### Scene 预览工作集策略

mesh_render.gpu_memory_limit_mb 可选限制逻辑 GPU 工作集；gpu_min_pixels 仅用于 auto，缺省 0 保留既有预览选择。低于阈值的 auto 不初始化 GPU；显式 GPU 绕过阈值但遵守预算。预算/执行失败时 auto 回退，gpu 报错，CPU 跳过 GPU 专属资源选项。

checked planner 按可见三角形每面 120、启用 segment 每条 64、marker 每个 192 字节计数；目标 color/depth 为 8×pixels，readback 为按 256 字节对齐的 RGBA 行，uniform 128 字节。保守计入待完成 queue 上传，逻辑峰值 = 2×geometry_bytes+256+8×pixels+staging_bytes。多个视图共用目标。驱动/pipeline 内部及主存场景/PNG 不计入，所以这不是物理 VRAM/RSS 上限。独立 device 限制检查先于 host 顶点展开，上传后即释放临时 host 顶点。构造器返回受作用域保护的 GPU 验证/分配错误；图像分块仍待实现。

### CPU 像素任务划分

STL 最近命中和 prepared 透明 scene 渲染均使用不重叠连续像素任务。当行数不少于 worker 且每 worker 不超过 1024 像素时保留按行，避免最小块粒度减少并行任务；其他图单 worker 为单任务，否则目标为每 worker 四个任务，粒度夹取 256..4096 像素，整行能放下时按行对齐。宽行可跨任务，短行可合并。任务仅在起点计算 (x,y)，随后递增原整数像素坐标，射线运算、命中排序/合成与串行叠加不变。scene depth/RGBA 共用边界并复用任务内 hits scratch。两种投影、非整除尺寸、极端纵横比在 1/2/8 worker 下逐字节对照按行参考。粒度性能验收见 PLAN.Performance.md@1349c46 §40。

### GPU PNG 有界写出（2026-09-23）

GPU 多视图且配置 worker 数大于 1 时，`consume_frames` 通过零容量通道按顺序将图像所有权移交给单个 PNG writer。最多一张图像正在编码、一张由渲染生产端持有；GPU 渲染目标继续复用。返回或 CPU 回退前，必须等待 writer 退出并排空已接收帧。输出错误优先于 GPU 错误，不触发回退；即使 producer 成功，末帧写出错误也不会丢失。单 worker 或单视图保持顺序执行；这证明有界重叠机制，不代表端到端提速；软件 GPU 冷进程 CLI 对照（含 PNG 身份与 RSS）见 PLAN.Performance.md@1349c46 §56；完整负载和硬件验收仍开放。

### CPU PNG 写出重叠（PERF-17，2026-09-25）

CPU 视图改由 `render_and_write_overlapped` 处理：第 *i* 个视图在 `rayon::join` 中渲染（内部仍按像素并行），同时第 *i-1* 个视图在同一线程池的另一个 worker 上进行 PNG 编码和写盘；因此最多存在两张已完成或正在渲染的图像，写出仍按视图顺序。写出错误在并发开始的那次渲染结束后立即返回；之后的视图不再渲染或写出，CPU 写出错误也不会触发任何回退。单 worker 时 `join` 依次内联执行渲染与写出，即原来的顺序循环。测试在 1/2/4 worker、0/1/2/7 个视图下与顺序循环比较 PNG 字节和顺序，检查首/中/末帧写错（且最多多渲染一个视图），并证明下一视图的渲染在上一张写出完成前开始。GPU writer 保留 `consume_frames`：其生产者由回调驱动（`render_views_to` 推送帧），逐帧 `rayon::join` 需要拉取式 GPU API，而 rayon scope 加阻塞交接会让线程池 worker 阻塞在通道上；其阻塞/排空/回退顺序由现有测试覆盖，本次未改动。忽略的 release 基准 `cpu_overlap_benchmark`（level-5 icosphere 的 8 个视图，预热后 5 个样本，4 核共享机器且有并发构建）未发现可靠差异：256²/1024²/2048² 及 1/2/4 worker 下重叠/顺序中位比值为 0.95～1.23，样本离散度大于差值，原因是 PNG 编码只占 CPU 光线投射时间的一小部分。Auto 后端选择：STL `render` 已通过 `resolve_execution` 使用 `acceleration.gpu_min_pixels`（默认 250,000）；`mesh-render` 默认仍为 `gpu_min_pixels: 0`，因为把小尺寸 `auto` 渲染切到 CPU 会改变图像（CPU 为透明度参考，GPU 为不透明预览），而不仅是成本。

### 不透明场景最近命中组（2026-09-23）

准备后的场景至少有 192 个三角形、且每个 clamp 后的 alpha 都恰为 1 时，先用 QBVH 求最近距离，再以向外取整的 `nearest + 2 * dedup_tol` 为界枚举附近命中。保留原距离/triangle ID 排序及移动锚点的 Face-over-Volume 去重，只取首组；所选法线、颜色和深度交给原合成器与覆盖线逻辑。更小场景或包含零/部分/NaN alpha 时维持全命中枚举。三角形阈值避开了实测 24 三角形场景的双遍历退化，只是保守启发式，不能保证所有空间布局提速。分层场景 release 对照和限制见 PLAN.Performance.md@1349c46 §57。
