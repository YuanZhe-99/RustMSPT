# RustMSPT 非 Mesh Gen 管线性能优化计划

状态：2026-09-25 Cloud Session 完成 §66 所列批次后按用户要求阶段收尾，**后续交给本地 Agent**，交接见 §67（§65 为上一 Session 交接，保留作历史）。已实施子项及验证见第 10～66 节；PERF-00～19 的整体验收尚未完成。前文“源码确认”描述初始基线，当前实现以各批记录和源码为准。

检查日期：2026-09-12。源码基线：`891badc09ae03fb99ab57d206451e75de226ee60`。

## 1. 目标与范围

目标是降低真实数据上的端到端耗时、峰值内存和重复计算，同时让线程数、GPU 选择、回退行为及输出中的后端信息与实际执行一致。实现顺序是：建立基线 → 修复影响正确性和基准可信度的问题 → 减少算法工作量 → 改进 CPU 并行 → 改进 GPU 数据流。

覆盖：

- `split-filter`、旧版 `pack`（`packing:`）、新版 `pack`（`placement:`）、`optimize`、`measure`、`forge`、`scale`、`crop`、`render`。
- 已有的独立 `mesh-render` 渲染路径，以及上述管线共享的几何、S2、计算后端和 I/O。

不覆盖：

- 未完成的 Mesh Gen 阶段、网格生成算法、Mesh Gen 的质量/拓扑修复及其 GPU 路线。
- `mesh-verify` 内部检查算法的性能审计；它与 Mesh Gen 共用的数值和验证模块另行处理。
- 新增物理模型、改变目标分布、放宽碰撞条件，或用近似结果替换约定的精确结果。

本文采用三种证据标记：

- **源码确认**：可由当前调用关系、循环、参数布局或资源分配直接确认；尚不代表已测出耗时占比。
- **运行确认**：本次检查实际执行了相应测试或检测。
- **待测假设**：有明确的成本来源和优化方向，收益、阈值或跨设备表现仍须测量。

后文的复杂度和传输量是算法分析；没有实测依据的项目不承诺加速倍数。

## 2. 已有检查结果与性能基线的缺口

### 2.1 已执行的功能检查

| 检查 | 结果 | 能证明什么 |
|---|---|---|
| 默认特性下的 `pipeline_smoke_tests`、`render_tests`、`placement_pipeline_tests`、`pack_target_tests`、`mesh_render_tests` | 67 项通过 | 所选 CPU 功能回归通过 |
| `gpu` 特性下单独运行 `render_tests` | 13 项通过，包含 CPU/GPU 图像比较 | 软件适配器上的渲染路径可运行 |
| `gpu` 特性下的 `compute::tests` | 6 项通过 | 后端选择基础测试及适配器检测可运行 |
| `gpu` 特性下编译 `mesh_render_tests` | 3 处编译错误 | GPU 场景渲染测试目前不能完整执行 |

GPU 场景测试的具体错误：`tests/mesh_render_tests.rs:497` 缺少 `SceneSegment` 导入；同文件 `:336`、`:380` 的 `RenderScene` 初始化缺少 `wireframe_edges_emitted`、`wireframe_edges_total`。

检查环境的 `available_parallelism()` 为 8，没有 `/dev/dri`；检测到的是 `llvmpipe (LLVM 21.1.8, 128 bits)`。上述测试使用 debug/test 构建，**不是硬件 GPU 性能测试，也没有形成 release 下的单核/多核加速比**。不能把 19 项启用 GPU 特性的测试全部视为 GPU 计算内核测试；其中大部分仍是普通功能测试。

### 2.2 各管线当前覆盖

| 管线/阶段 | CPU 多核现状 | GPU 现状 | 主要问题与任务 |
|---|---|---|---|
| `split-filter` | 分量发现、指标和过滤主要串行 | 无 | 分量指标重复遍历、小对象分配、文件处理；PERF-15、18 |
| 旧版 `pack` | 在专用池中并行扫描碰撞/距离 | 无 | 每次尝试复制全体、重复构建查询结构、全扫描；PERF-11、13 |
| `placement` | 接受循环串行，标签输出使用 Rayon | 无 | `threads` 未配置线程池；已有空间网格和已接受粒子的查询缓存应保留；PERF-02、12、13 |
| `optimize` | S2 使用专用池，多岛另建线程和池 | 持久 MC 管线，但每岛独立初始化 | 后端与方法选择不一致、全量更新、资源超配；PERF-01、02、10 |
| `measure` | 体素化、S2 半径/FFT 有并行 | 连续网格 MC、体素化、直接壳配对 | 方法语义、重复准备、内存和传输；PERF-05～09 |
| `crop` | 重采样按 z 切片并行，PCA 三遍扫描串行 | 输出超过 100,000 体素时直接尝试 GPU | 绕过统一策略、整数收窄、PCA 和整卷传输；PERF-01、14 |
| `forge`、`scale` | 主要顶点变换及统计串行 | 无 | 低算术强度，多次遍历、复制和 I/O；PERF-16、18 |
| `render` | 已有 QBVH，按行并行，专用线程池 | 已有离屏光栅化 | 小任务启动成本、资源预算与回读；PERF-03、04、17 |
| `mesh-render` | 已有 QBVH 和按行并行 | 已有不透明预览，多视图复用一次几何上传 | CPU 每视图重建场景、逐像素分配、图像积压；PERF-17、19 |

## 3. 实施约束

1. **保持算法语义。** 体素 MC、连续网格 MC、体素 exact 是不同求值方式。GPU/CPU 比较必须在相同定义、边界、壳权重和 VF 约定下进行。
2. **保持 placement 的确定性。** RNG 只在主控制流程消费；预先抽取的尺寸、多次尝试的随机数消费顺序、拒绝原因、接受顺序、体积累计顺序和输出身份均不可因线程调度改变。
3. **保留 CPU 参考和可用的回退路径。** 不把验证失败、设备丢失或无法表示的输入伪装为成功结果。显式关闭回退时返回可理解的错误。
4. **先减少计算，再增加线程。** 已经建立 QBVH、已经缓存已接受粒子、已经复用 GPU 多视图上传的部分，不重复列为“尚未实现”。
5. **有界并行与内存预算同时设计。** 新 CPU 并行使用 Rayon。工作线程、同时在途 GPU 任务、CPU/GPU staging 和 I/O 队列都要有上限。
6. **数值变化单独验收。** 浮点求和重排、FFT padding、GPU f32 和 PCA 特征向量变化都需要针对性验证。不得通过放宽阈值掩盖回归。
7. **真实数据与合成数据结合。** 合成样例提供已知答案，真实 STL/CT 数据用于判断瓶颈和收益；两类结果都保留。

## 4. 优先级与任务总表

P0：影响结果、配置承诺、资源安全或测试可信度；P1：主要热点和跨管线重复成本；P2：在基线支持下实施的进一步优化。

| ID | 优先级 | 工作项 | 主要依赖 |
|---|---|---|---|
| PERF-00 | P0 | 分阶段基准与观测指标 | 无 |
| PERF-01 | P0 | 统一后端、设备和配置的执行语义 | 无 |
| PERF-02 | P0 | 线程池实际生效与岛模型总预算 | 无 |
| PERF-03 | P1 | GPU 上下文和缓冲区复用 | 01、04 |
| PERF-04 | P0 | GPU 内存预算、分块 dispatch、运行时回退 | 01 |
| PERF-05 | P0 | S2 GPU 的方法、布局、容量和边界正确性 | 01、04 |
| PERF-06 | P1 | CPU S2 的查询准备、BVH 和任务粒度 | 00、05 |
| PERF-07 | P1 | FFT/直接法的成本选择与工作区复用 | 00、06 |
| PERF-08 | P1 | GPU MC 的分层归约和少量回读 | 03、04、05 |
| PERF-09 | P1 | GPU exact 的二维分解与设备常驻数据 | 03、04、05 |
| PERF-10 | P1 | SA 增量状态、缓存和岛间迁移 | 01、02、05、06 |
| PERF-11 | P1 | 旧版 pack 的准备缓存和增量广相位 | 00、13 |
| PERF-12 | P1 | placement 的确定性并行及标签加速 | 02、06、13 |
| PERF-13 | P1 | 空间网格去重、更新和宽尺寸分布 | 00 |
| PERF-14 | P1 | crop 的并行统计、分块重采样与类型保护 | 01～04 |
| PERF-15 | P2 | split-filter 的分量指标批处理 | 00、02 |
| PERF-16 | P2 | forge/scale 的遍历融合和多核变换 | 00、02 |
| PERF-17 | P1/P2 | 渲染的准备复用、临时内存与多视图输出 | 02～04、19 |
| PERF-18 | P2 | I/O、解析、摘要和输出的有界流水线 | 00、02 |
| PERF-19 | P0/P1 | GPU 回归入口修复、专项测试及文档闭环 | 首项可独立，后续随各任务 |

## 5. 具体问题与算法方案

### PERF-00：建立能分辨算法、调度和传输成本的基线

**现状与影响：** 运行确认只有功能测试；现有检查无法说明哪条管线受计算、内存带宽、I/O 或 GPU 往返限制。先开更多线程或把一个循环搬到 GPU，可能只转移瓶颈。

**实施：**

- [ ] 在 release 构建下记录 load/parse、prepare、broad phase、narrow phase、voxelize、S2、PCA、transform、encode/write 各阶段 wall time。
- [ ] GPU 单列 adapter/device/pipeline 初始化、上传、kernel、回读和 CPU 后处理。时间戳查询仅在设备支持时使用；队列提交时间不能冒充 kernel 时间。
- [ ] 记录实际线程池 worker 数、岛数、适配器类型、输入规模、拒绝次数、精确查询数、上传/下载字节、峰值 RSS 与程序所分配的 GPU 资源估算值。后者不能称为驱动实际显存占用。
- [ ] 为纯内核提供稳定输入的基准入口；端到端基准使用独立输出目录，避免覆盖已有数据。
- [ ] 区分首次运行和资源复用后的运行。MC/SA 使用可记录的基准种子和相同求值预算；这不要求立即改变公开 RNG 协议。

**验收：** 第 7 节的每组样例能输出原始结果；报告中可以把“速度变化”对应到查询数、复制量或传输量变化。基准脚本本身不自动调整算法精度。

### PERF-01：后端选择结果未统一决定实际执行

**证据：源码确认。** [optimize.rs](src/pipeline/optimize.rs) 的 `OptimizePipeline::run` 打印 `select_backend` 结果后，单岛按 `accel.mode != Cpu` 再次初始化 GPU，多岛无条件尝试 `GpuS2Pipeline::new`。`crop` 根据固定体素阈值直接调用 GPU。`RUSTMSPT_ACCELERATION` 在文档中存在，但当前 `src` 没有相应读取；`cpu_fallback`、`gpu_prefer_power`、`gpu_precision`、`backend` 等字段也未完整进入实际调度。

**算法/结构改进：**

1. 把“请求配置 → 可执行方法/设备 → 资源预算 → 实际后端”集中解析为一次执行决策。下游只消费这个决策，不能重新根据原始 `mode` 猜测。
2. 分清方法选择与设备选择。`exact` 的 CPU 回退仍应求 exact；不能因为存在 MC shader 就改变目标函数。
3. 为没有统一配置入口的 `crop` 增加兼容的 `acceleration` 配置；旧 YAML 继续可读。环境变量优先级需明确，例如“环境变量覆盖 YAML”，非法值明确报错。
4. 给当前配置字段逐项制定行为：实现支持的选项，对不支持的精度/后端明确拒绝；不要把未使用字段继续当作生效配置。
5. `mesh-render` 保留已定义的 `backend: gpu` 不可用时报错、`auto` 可回退的契约；它的不透明预览模式也必须显式保留。

**验收：** `cpu` 单岛/多岛都不进行 GPU 初始化；低于阈值的 `auto` 不建 GPU 管线；设备筛选贯穿 voxel、shell、MC、crop、render；输出记录实际执行方法和回退后的后端。

### PERF-02：线程数未覆盖真实工作，岛模型可能超出预算

**证据：源码确认。** [placement.rs](src/pipeline/placement.rs) 的 `resolve_threads` 结果用于记录，没有建立或安装线程池；[placement_labels.rs](src/pipeline/placement_labels.rs) 使用全局 Rayon 池。`optimize` 的 `threads_per_island = max(thread_count / num_islands, 1)` 在岛数超过预算时仍为每岛创建 worker；部分 VF 调用在专用池外。`measure` 的 GPU exact 初始化失败后，函数内部 CPU 回退也可能离开管线专用池。

**算法/结构改进：**

- [ ] 建立每次运行的 CPU 执行上下文，把计算阶段及 CPU 回退都纳入同一个 worker 预算；日志区分请求值与实际 worker 数。
- [x] placement 整次运行安装到专用 Rayon 池，包括标签输出和嵌套几何调用；主接受循环仍串行。实际 worker 数从执行中的池读取，1/2/8 worker 输出一致性已有回归。仅“建立了池”不等于主循环变快，收益由 PERF-12 提供。
- [x] optimize 全阶段安装在同一 Rayon 池；岛任务按不超过 worker 数的批次执行，多余岛排队，嵌套 S2/VF 共享预算。单岛不再额外复制初始状态；无逐岛线程或线程池。1/2/8 worker 嵌套执行测试与进程线程数实测通过，详见第 11 节。
- [ ] 按“岛间并行”与“岛内 S2 并行”组合测量，避免大量空闲池和竞争；GPU 调度还需要独立的在途任务/显存预算。
- [ ] 小列表/小网格使用串行阈值；阈值来自调度开销基准，而不是对所有循环套 `par_iter`。

**验收：** 在实际热点中采集线程池索引/worker 数，验证 `threads=1/2/8` 和岛数大于 worker 数的情况。placement 输出跨线程数仍逐字节一致。线程数测试不能只检查报告里的 `runtime.threads`。

### PERF-03：GPU 上下文重复初始化，资源复用不完整

**证据：源码确认。** [context.rs](src/gpu/context.rs) 的 `try_init_gpu` 创建并丢弃 device/queue，仅返回能力信息；注释所称缓存没有落实。MC、voxel、shell、volume transform 构造器又分别创建设备，后面三者不遵循同一设备筛选。`measure` 即使只求 exact，也先尝试创建 MC 管线。MC 两个输出缓冲预留 `128 × 40,000 × 4 × 2` 字节，约 **39.1 MiB/实例**，尚不含 staging/几何数据。

**算法/结构改进：**

1. 在一次运行内共享 `Arc` 管理的 Instance/Adapter/Device/Queue 和能力信息；根据设备与 shader 配置缓存编译后的 pipeline。
2. 按实际方法延迟创建 MC、voxel、shell、render 资源，取消与真实工作量无关的大额初始分配。
3. 每个求值任务保留独立参数和输出区间；共享 queue 不意味着可以并发覆盖同一个 uniform/storage 缓冲。
4. staging 和临时缓冲按容量复用，增长受 PERF-04 预算限制；长时间运行允许释放不再使用的峰值容量。
5. 多岛共享设备和不可变源几何，独立管理每岛状态；如要跨岛批处理，使用有界队列，保持各岛自身的迭代顺序。

**验收：** 一次运行中的设备创建数可观测；仅 exact 测量不分配 MC 输出；重复求值不重复创建同类 staging；设备筛选和资源生命周期有集成测试。

### PERF-04：GPU 内存限制不是工作集预算，超限和运行错误不能可靠回退

**证据：源码确认。** [policy.rs](src/compute/policy.rs) 将配置的 MB 数与适配器的单个 storage binding 上限比较，没有估算本次任务分配量；较大的“预算”反而可能触发拒绝。各构造器请求默认 device limits，不能直接把 adapter limits 当作可用 device limits。多个计算方法返回 `Vec`，`map_async` 回调忽略错误，初始化的 `Result` 无法覆盖后续验证/映射失败。

**算法/结构改进：**

- [ ] 用 checked arithmetic 计算每种方法的输入、输出、中间值、staging 和同时在途任务的峰值字节。
- [ ] 分开检查 device 的单 buffer、单 binding、dispatch 三维上限，以及用户对整项任务的资源预算。
- [ ] 按预算确定批大小。用 `base_index`/局部坐标把一维大 dispatch 分成多批或二维网格；不能仅把溢出的尺寸强转成 `u32`。
- [ ] 接近 32 位计数上限的配对/采样分块计数，在 CPU 用 `u64` 合并，或使用经验证的多阶段整数归约。
- [ ] 计算 API 返回显式错误；读取 map 回调结果，在合适位置使用 wgpu error scope 并处理设备丢失。禁止以全局 `catch_unwind` 作为正常 GPU 调度策略。
- [ ] 预算超限优先缩小批次，无法分块则按配置回退；显式禁用回退时返回错误。中途失败应丢弃不完整输出，不能混用旧缓冲内容。

**具体边界：** 默认每维 65,535 个 workgroup 时，voxel shader 的 64-wide 一维 dispatch 只能覆盖 4,194,240 次调用；简单把 `nx*ny*nz` 展平并不支持任意大网格。设备实际限制必须运行时读取。

**验收：** 极小预算、超大逻辑尺寸、buffer/dispatch 临界值、映射失败和设备不可用都有可控测试。预期行为是完整计算、分块、明确回退或错误，不能是 panic、越界或静默截断。

### PERF-05：S2 GPU 正确性前置问题

**证据：源码确认；专项数值复现待补。** 这些问题必须先于 GPU 加速比验收处理：

| 问题 | 当前依据 | 改进方向 |
|---|---|---|
| 方法含义不一致 | CPU 正 pitch 的 MC 基于体素；GPU MC 直接对连续网格采样；SA 有 GPU 时直接调用 MC，未按 `mc_method` 区分 | 明确内部 `voxel_exact`、`voxel_mc`、`mesh_mc` 求值器及各自支持的后端 |
| 优化过程中求值器变化 | SA 迁移后的 `current_s2` 走 CPU，GPU 迭代走连续 MC | 初始、候选、迁移、最终复核使用同一已解析求值规则 |
| voxel 参数布局错位 | [voxel.rs](src/gpu/voxel.rs) 将 `ray_dir` 从字节 24 开始写；[voxelize.wgsl](src/gpu/shaders/voxelize.wgsl) 中 `vec3<f32>` 对齐后应从字节 32 开始 | 显式布局类型与 padding，测试字段 offset 和 shader 端读回值；当前总长 48 正确不代表字段正确 |
| MC 半径数组固定 128 | [s2.rs](src/gpu/s2.rs) 仅打包 128 个半径，dispatch 仍使用 `r_max+1` | 半径分批或动态 storage 数组；临时阶段明确拒绝不支持范围 |
| shell offset 容量不一致 | [s2_shell.rs](src/gpu/s2_shell.rs) 将计算数截到 200,000，却上传完整 offset 列表至固定容量缓冲 | 动态容量/完整分批，不得用截断“完成”exact |
| shell 无效位移下溢 | [s2_shell_pairs.wgsl](src/gpu/shaders/s2_shell_pairs.wgsl) 先做 `nx-u32(dx)` 等无符号减法 | 先判 `abs(offset_axis) >= dimension`，无支持直接返回零计数 |
| 射线交点固定上限 | 两个 ray shader 只保存前 64 个交点 | 溢出标记和 CPU 重算/分块恢复，或经过验证的流式交点算法；不能对截断列表求奇偶 |
| f32 归一化损失精度 | 三角坐标先分别转 f32，再相减 | 在 f64 中先减 bbox 原点，再转 f32；加入大平移、小特征测试 |
| 容差和 pitch 处理不同 | CPU/GPU 命中去重容差不同；GPU exact 尺寸计算使用原始 pitch，CPU exact 对非正 pitch 有专门处理 | 统一输入规范化和边界约定，对无法可靠处理的尺度保守回退 |

**算法约束：** exact 壳结果当前是“每个位移的 `hits/valid`，再对有支持的位移等权平均”。不能替换成整壳的 `Σhits/Σvalid`，两者在边界附近不同。VF 也要区分体素占据率与连续几何体积；不能为了曲线首点一致就隐藏定义差异。

**实施与验收：**

- [ ] 先为布局、129 个及更多半径、超过 200,000 offsets、位移超出维度、超过 64 个交点、大坐标小几何、非正 pitch 建立独立反例。
- [ ] exact 使用小网格穷举 oracle；MC 使用相同采样点或相同统计定义验证，不能要求不同随机实现逐字节相等。
- [ ] 无法稳定分类的 f32 查询标记为“不确定”，批量返回 CPU 参考判定；记录重算比例，避免把全部任务回退误称为 GPU 提速。
- [ ] 仅在上述边界通过后启用 PERF-08/09 的性能验收。

### PERF-06：CPU S2 重复扫描包围盒和全部三角形

**证据：源码确认。** [geometry/s2.rs](src/geometry/s2.rs) 的 `point_inside_mesh` 每次查询重算 `mesh_bbox`、扫描所有三角形、分配并排序交点。体素化虽有分量 bbox 过滤和 Rayon 并行，内部命中测试仍承担这部分成本。连续 MC 和标签输出也调用该入口。

**算法优化：**

1. 引入一次准备、反复查询的结构：缓存 bbox、三角数据和 QBVH/BVH。利用 BVH 只筛选射线可能命中的三角形，末端仍使用约定的精确 CPU 相交/去重规则。
2. 先只增加广相位并保留现有窄相位，避免直接换用伪法线包含测试而改变嵌套壳语义。
3. 将交点 `Vec` 改为每个 worker/任务块复用的 scratch；查询间清空长度，不重复申请容量。
4. MC 按 `(radius, sample_block)` 分解，而不只按半径分解。半径很少、样本很多时也能用满可用 worker；用整数 hit/valid 归约，随机数流不能绑定到不稳定线程 ID。
5. 同一网格、bbox、pitch 的 `measure: both` 复用体素占据和 shell offsets；定义缓存键及失效规则。

**预期成本变化：** 准备约 `O(T log T)`；每次查询从扫描 `O(V+T)` 转为树遍历加候选相交，典型约 `O(log T+h)`，最坏情况仍可能退化。用实际候选三角形数验证，不承诺所有几何同样受益。

**验收：** 原始遍历作为 oracle，对嵌套、凹形、共享边/顶点和射线穿过多粒子的点集逐点比较；同时确认 query 热点不再重算全 mesh bbox。

### PERF-07：FFT/直接枚举只用固定体素阈值，工作区与分配可改进

**证据：源码确认。** `calculate_s2` 用 24,000,000 个 padding cells 决定 FFT/直接法；`measure` 另有 1,500,000 个体素的 exact 上限。FFT 各轴已有并行，实际共享规划结果，不需要按旧注释建议让每线程重复规划。每条 `process` 调用的 scratch、转置大缓冲、填充/功率谱/归一化等扫描值得单独测量。

**算法优化：**

- [ ] 以 `P = padded_nx*padded_ny*padded_nz`、有效 offsets 数 `K` 和原始体素数 `M` 建模：FFT 约 `O(P log P)`，直接枚举约 `O(KM)`。选择时同时考虑峰值字节与实际有效配对数，不能只比较 `M`。
- [x] 缓存维度对应的 FFT plan；使用 `process_with_scratch` 和每任务块工作区，减少反复分配。转置缓冲在重复 SA 求值间复用。（有界缓存实现与限制见 §22；完整预算/整体验收仍未完成。）
- [ ] 对轴长度测试保持 `>=2N-1` 的 FFT 友好 padding；比较更快变换与更大内存之间的取舍，正确处理负位移索引。
- [ ] 在同一线程池内处理填充、功率谱、转置和归一化；按块改善访问局部性，测量内存带宽后再考虑额外布局转换。
- [ ] 直接法可按 offset 与 voxel tile 分解；后续探索位集移位 AND + popcount，但必须正确处理跨字边界、行边界及合法配对范围。
- [ ] 若考虑实数 FFT 或 GPU FFT，作为独立实验项验证数值和资源成本，不假定当前依赖已经提供所需接口。

**验收：** CPU FFT、CPU 穷举和 GPU exact 在同一占据网格上满足一致的壳定义；针对小 `r_max`/大网格、长薄网格和 FFT 不友好轴长测选择结果。取消/调整 exact 硬阈值前必须有完整内存预算。

### PERF-08：GPU MC 输出逐样本结果，回读量与样本数成正比

**证据：源码确认。** [gpu/s2.rs](src/gpu/s2.rs) 对每个 `(radius, sample)` 写两个 `u32`，再建立两个 staging、同步映射并在 CPU 汇总。`R` 个半径、`S` 个样本，仅两组结果的回读就约 `8RS` 字节。

**算法优化：**

1. 第一层在 workgroup 内归约 hit/valid，输出 `(radius, block)` 的整数部分和；不对每个样本做争用严重的全局 atomic。
2. 第二层在 GPU 合并每个半径的部分和，或先在 CPU 合并规模较小的部分和。回读从 `O(RS)` 降到 `O(R·ceil(S/W))`，最终可到 `O(R)`。
3. 射线遍历使用紧凑 BVH/三角数据；先完成 PERF-05 的不确定/溢出处理。保留简单扫描作为小 mesh 的候选路径，避免 BVH 构建成本超过查询收益。
4. 复用 query、输出和 staging 缓冲；对固定样本定义按样本编号分块，使批大小变化不改变实际抽样点。
5. 单岛 SA 的下一次候选依赖上次接受结果，不能盲目异步并行多步；批处理主要跨半径、样本、独立岛或独立请求。

**验收：** 在相同采样点上 hit/valid 与参考一致；超过批大小时不漏样本。记录回读字节和 CPU 汇总时间，分别对小/大 mesh 判断 BVH 与直接扫描阈值。

### PERF-09：GPU exact 每个 invocation 串行扫描整网格，且占据场往返主存

**证据：源码确认。** [s2_shell_pairs.wgsl](src/gpu/shaders/s2_shell_pairs.wgsl) 一个 invocation 负责一个 offset，并串行遍历其全部有效体素；offset 少、网格大时并行度受限。[calculate_s2_gpu_exact](src/geometry/s2.rs) 先下载 voxel 结果，再上传到另一个 device 的 shell 管线。

**算法优化：**

- [ ] 共享设备后让 occupancy buffer 直接供 shell kernel 使用；占据总量在 GPU 做整数归约，只回读 VF 所需计数和最终曲线。
- [ ] 以 `(offset, voxel_tile)` 为任务单元，workgroup 内归约 tile 命中数，再跨 tile 合并。总工作量仍约 `O(KM)`，但不再把 `M` 的循环压在单 invocation 上。
- [x] 合法配对数由三轴重叠长度相乘得到，避免对每个合法体素逐次累计 `valid`；主机检查完整网格 u32 乘积，重叠子体积和 hit 计数随之有界。独立原始计数 oracle 与 release 对照见 §43。
- [ ] 先得到每个 offset 的完整整数计数，再按 PERF-05 的等权规则做壳平均。禁止在 tile 层先平均比例，因为各 tile 的有效样本数不同。
- [ ] offsets 按预算分批，半径归约跨批累积；没有合法配对的 offset 直接过滤，保留无支持半径的既有插值约定。
- [ ] 大半径/大量 offsets 时与 CPU FFT 比较；GPU direct 并不天然优于 CPU FFT。GPU FFT/位集仅在成本模型支持时进入后续实验。

**验收：** 不同 tile/offset 批大小给出相同整数计数；核心求值无整场 CPU 往返；在无效位移和细长网格上不越界、不漏计。

### PERF-10：SA 单粒子变化触发全量合并、空间网格重建和 GPU 上传

**证据：源码确认。** [optimize.rs](src/pipeline/optimize.rs) 的 `run_sa_island` 每个可行候选都会替换粒子、重建 `SpatialGrid`、复制/合并所有网格；GPU 求值上传整个 merged mesh，并计算全体 VF。迁移会复制全体粒子并重新准备，部分深复制发生在共享锁内。预剪枝也存在重复合并和求值。

**分层算法方案：**

1. **低风险缓存。** 预计算每个粒子的不可变拓扑、源指标和 triangle range；复用 merged storage。一次移动只覆盖该粒子的顶点/三角范围。拒绝时用保存的旧范围恢复，接受时提交。
2. **广相位增量更新。** 保留旧/new bbox 的 cell membership，只删除/插入受影响桶；拒绝回滚。周期 ghost 身份及桶更新一并处理。
3. **VF 增量维护。** 缓存每粒子的域内贡献，只重算变化粒子；完全在域内的刚体变换不改变体积。总和更新有浮点漂移风险，应有固定频率的参考重算和误差界；不能把这个方案直接用于要求固定累加顺序的 placement。
4. **CPU/GPU 几何表示。** CPU prepared shape 尽量复用局部坐标形状加刚体 pose；GPU 共享局部三角形和 BVH，只更新 transform/顶层 bounds，或先实现部分 triangle range 上传。
5. **体素求值的增量实验。** 对旧/新 bbox 的并集重算占据；用覆盖计数或局部重新查询，不能简单把旧粒子体素清零而误删其他粒子贡献。直接法/固定样本 MC 可以只更新受影响配对；FFT 先复用占据场，是否增量更新频谱另行评估。
6. **迁移。** 在锁外准备不可变最佳快照，锁内只比较 loss 和替换引用；接收岛再在锁外准备状态。保持 loss、几何、S2 来自同一个求值定义。

**数值约束：** 缓存、增量和拒绝回滚必须带版本/失效规则；迁移、粒子移除、周期边界变化会失效相关状态。使用固定 MC 样本降低噪声是可选算法变更，需另评估偏差和最终质量，不能混入“等价缓存”。

**验收：** 对固定的候选动作序列，逐步比较增量与全量 bbox、碰撞、VF、S2 和接受结果；独立覆盖拒绝、迁移、跨界及移除。测每步复制/上传字节，而不只测总迭代数。

### PERF-11：旧版 pack 每次尝试复制全体并重复构建碰撞结构

**证据：源码确认。** [pack.rs](src/pipeline/pack.rs) 每个候选执行 `placed.clone()`，周期模式重新生成全部 ghost 并重算 bbox；`mesh_collision_exact`/`mesh_distance_exact` 包装器每对查询重建双方 TriMesh。碰撞后另一次全扫描计算最小距离，而接受判断只需要知道是否存在低于阈值的对象。

**算法优化：**

- [x] 为已接受粒子缓存 bbox、prepared shape 和可复用 ghost/平移实例。新候选只在通过便宜拒绝后建立一次 shape；用借用代替 `placed.clone()`。
- [x] 建立增量空间索引，每次接受插入新粒子。查询 candidate bbox 按 gap 扩展后的邻域；典型候选数量从 `N` 降到局部 `k`，总复杂度仍受拥挤程度影响。
- [x] 将“碰撞或嵌套或距离过近”的接受判定在同一候选列表完成；距离只需阈值判定时用提前终止，不必 reduce 出精确全局最小值。
- [ ] 周期边界优先保存 `(particle_id, image_shift)`，避免复制整份顶点。只查询可能影响候选的 image；对角跨界、域宽附近的大粒子和重复 image 必须有参考对照。
- [x] 邻居很少时串行，很多时并行。保留 target histogram/sphericity 的选取、成功后记账和尝试顺序。

**验收：** 同一候选序列逐个与旧广相位/全扫描比较接受结果；包含表面相交、整体嵌套、gap 临界和周期 26 邻域。统计 TriMesh 构建数应从“每对查询”降为“每个被准备对象/姿态”。

### PERF-12：placement 需要不改变随机序列的并行，标签查询仍有全粒子扫描

**证据：源码确认。** placement 已有增量 `SpatialGrid`、接受粒子的 `shape` 缓存和延迟窄相位，不应按旧版 pack 的问题重复改写。可优化处是串行邻居检查、源库准备和标签输出。[particle_at](src/pipeline/placement_labels.rs) 虽先判 bbox，仍对每个体素扫描全部粒子，命中 bbox 后又使用未准备的 `point_inside_mesh`。

**算法优化：**

1. 并行源文件/分量的独立指标准备，结果按原 `source_index/shell_index` 收集；不改变来源顺序、digest 或过滤拒绝语义。
2. 邻居多时并行执行只读 pair 检查。按**当前串行邻居遍历序及每对内部检查顺序**选出第一个拒绝，不能使用任意完成顺序的 `find_any` 改变拒绝计数。
3. 保持候选提出、随机数消费和接受提交串行。跨多个候选投机计算若不能维持旧消费顺序和状态依赖，暂不实施。
4. 标签场按 tile 查询空间索引，预先得到 tile 的粒子候选列表，点查询复用接受粒子的 QBVH；将 `O(voxels × particles)` 的 bbox 检查降为 tile 邻域工作。
5. 标签按有界切片块输出，减少同时持有两个整卷 `i64` 数组。保持 void 优先、边界处 acceptance index 优先、spacing/origin 和文件顺序。

**验收：** 1/2/8 worker 下记录、STL、CSV、标签和 header 保持约定的一致性；固定候选回放下拒绝原因也一致。记录“每体素 bbox 检查数”以验证索引真正生效。

### PERF-13：共享空间网格的查询去重和尺寸选择

**证据：源码确认。** [spatial.rs](src/geometry/spatial.rs) 用 `neighbors.contains` 去重，重复桶引用多时退化为线性查找嵌套；`optimize` 还缺少增量删除/更新。宽粒径分布下按最大粒子尺度建统一网格可能把大量小粒子放进同一桶，实际收益待测。

**算法优化：**

- [x] 为每个查询上下文复用 generation-stamp 数组或等价 visit 标记，按第一次遇到的顺序追加 ID，把去重成本降到接近访问引用数。计数回绕要重置；并发查询的标记不能无保护共享。
- [x] 增加粒子到桶的反向 membership，支持 insert/remove/update 和回滚；保留可用作 oracle 的全量 build。
- [ ] 记录桶占用分布、每次重复引用数和候选/窄相位命中比例，再调整 cell size。
- [ ] 宽粒径比确实导致退化时，比较分级网格、粗细两层或动态 AABB 树。跨层查询必须覆盖完整 bbox 和 gap；不能只查粒子中心所在格。

**验收：** 随机 bbox/gap 查询与全扫描集合一致；在 placement 中保留既有邻居顺序或证明所有受影响决定等价。测试 clamped 边界、超域 bbox、大对象跨多桶和更新回滚。

### PERF-14：crop 的 PCA 串行扫描与整卷 GPU 重采样

**证据：源码确认。** [crop.rs](src/pipeline/crop.rs) 的 PCA 对整卷做 centroid、covariance、投影 bounds 三遍串行扫描；背景检测也遍历体素再判是否在边界。CPU 重采样仅按 z 切片分块，浅而宽的体积并行粒度不足。GPU 上传前 `i64 as i32` 未检查范围，三线性插值又转 f32；大 U32 强度存在数值损坏风险。

**算法优化：**

1. 背景仅遍历六个边界面，明确棱角只计一次；合并局部 histogram 时采用确定的并列规则。
2. 第一版保留三遍统计公式，按固定空间块并行，局部结果按固定顺序合并。进一步尝试可合并的 count/mean/M2 在线协方差，将 centroid+covariance 合为一遍，第二遍求旋转后 bounds；避免 `E[x²]-E[x]²` 的消减误差。
3. 固定块编号和归约顺序与 worker 数无关；对重复/接近重复特征值制定方向符号和排序策略，避免很小误差产生不同裁剪框。
4. CPU 按连续 voxel tile 或多行块分解，避免任务数被 `depth` 限死；每行起点变换加方向增量可减少矩阵乘，但需控制累积误差。
5. GPU 先验证强度范围和精度契约。nearest 可考虑 U32 路径以保持标签；trilinear 的 f32 误差不能仅用“输入能转 i32”判断。无法满足类型/误差要求时回退 CPU。
6. 大体积以输出 tile 为单位，逆变换 tile 的角点得到源 AABB，再加入插值 halo，上传相应源块。不能简单按相同 z 范围切源/输出，因为旋转会跨源切片。
7. 融合已确定的 trim 与最终输出索引，减少额外整卷 clone/copy；自动 trim 的检测与执行分开，保持输出几何。

**验收：** 恒等/90°/斜旋转、nearest/trilinear、U8/U16/U32/I32、背景外采样、tile 接缝、浅卷和近退化 PCA 均覆盖。分别比较 bounds、标签或强度误差；测 CPU 峰值与 GPU 上传量。

### PERF-15：split-filter 的独立分量指标和内存布局

**证据：源码确认。** [split_filter.rs](src/pipeline/split_filter.rs) 串行计算分量体积，并在后续过滤中再次遍历几何获取 bbox/表面积。[split_mesh_into_granules](src/geometry/mesh_ops.rs) 使用 `Vec<Vec<face_id>>` 邻接和逐分量 remap。

**算法优化：**

- [ ] 先按分量并行生成 `Metrics`：bbox、volume、surface area、aspect/sharpness 所需值。保持每分量内部原求和顺序，结果按原分量顺序收集。
- [ ] 过滤阶段只读取缓存指标，维持过滤顺序及统计口径；lognormal 重平衡的 RNG 和删除顺序保持串行。
- [ ] profile 显示分量发现占主导时，将 vertex-to-face 邻接改为 count → prefix sum → contiguous indices 的 CSR，减少小分配；保持“共享顶点连接”的现有定义。
- [ ] 并查集/并行连通分量作为后续选项。即使集合相同，源 shell 的首面顺序、顶点 remap 和几何 digest 也可能改变；先为受该共享函数影响的 placement 建立身份回归。
- [ ] 输出多文件采用少量并发 writer，文件名/序号预先按分量顺序分配。

**验收：** 过滤集合、统计、seeded rebalance 和 shell 身份不变；测小分量很多及单个巨大分量两种负载，避免只优化其中一种。

### PERF-16：forge/scale 的多次遍历和低算术强度

**证据：源码确认。** [forging.rs](src/geometry/forging.rs) 的变换按顶点串行，void 模式另算质心再遍历；[mesh_ops.rs](src/geometry/mesh_ops.rs) 的 scale/translate 也是串行。管线还做变换前后统计及 STL I/O，GPU 上传下载很可能抵消一个乘加循环的收益，后者为待测假设。

**算法优化：**

1. 优先提供内部 in-place 或消耗所有权的变换入口，保留公共 API 的既有语义，避免为只使用一次的输出复制整份 mesh。
2. 大顶点数组分块并行，小数组串行；同一个 affine transform 合并 scale/translate，预计算矩阵或轴系数。
3. affine 变换下顶点均值可随变换更新；void 的额外 centroid 收缩可据此减少一次扫描，但浮点顺序变化必须验证，不能直接假定字节一致。
4. 全 mesh 体积在适用时可由 `abs(det(A))*V` 更新；裁剪 ROI 内体积、sphericity 或非刚性后的面积不能套用不适用的捷径。一般 affine 后的 bbox 仍需保证精确性或明确仅作保守广相位。
5. 只要 I/O 占主导，先实施 PERF-18。GPU 顶点变换延后到几何可连续驻留设备或已测出足够大的交叉点时。

**验收：** 正/负尺度、不同压缩轴、void 收缩、ROI 跟踪和输出方向均与参考一致；收益分开统计 transform 与总耗时。

### PERF-17：渲染已有核心加速，进一步减少准备和临时内存

**证据：源码确认。** STL CPU 渲染已使用 QBVH；GPU `render_views` 已一次上传几何供多个视图复用。CPU [render_scene_cpu](src/geometry/scene_render.rs) 每次调用仍构建 scene shape，每像素分配/排序 hit 列表；[mesh_render.rs](src/pipeline/mesh_render.rs) 保留 GPU 全部输出图像后又逐张 clone。

**算法优化：**

- [x] 将 CPU scene preparation 与 camera rendering 分离，多个视图共享不可变 QBVH、颜色和透明度；每 worker/tile 复用 hit scratch。
- [x] 完全不透明场景评估 nearest-hit 快速路径，但必须处理同深度的 Face-over-Volume 优先规则；不能直接拿任意最近三角形改变覆盖结果。
- [x] 透明场景保留 all-hits 排序/去重与合成定义。近似 OIT 或 GPU 不透明预览不能作为等价替换；透明与不透明分开测。
- [x] CPU 默认使用像素 tile 并行；多视图并行与像素并行共享同一预算，避免两个层次都占满线程。
- [x] GPU 复用 render target、uniform、staging 的有界集合；图像使用所有权移动或流式写出，消除逐张 clone 和无界全批积压。
- [ ] PNG 编码/写盘与下一视图渲染在小队列中重叠，保留顺序及错误传播。分辨率较小时计入初始化和读回成本来选择 auto。

实施证据：准备/scratch §25；透明/不透明任务粒度对照 §40；GPU 资源/流式输出 §26；nearest coincidence §57。以上勾选仅对应具体实施与已记录对照，整个 PERF-17 的完整场景/硬件验收仍未关闭。最后一项的 GPU PNG 重叠已在 §56 实现，但 CPU 写出重叠及 auto 成本选择仍未全部完成，保留未勾选。

**验收：** 原 CPU PNG 基线、通道/边缘容差、透明背景、重合面优先及多视图一致性通过；GPU 测试入口先由 PERF-19 修复。记录 QBVH 构建次数和同时存活的整图数量。

### PERF-18：I/O 与解析可能掩盖计算优化

**证据：源码确认与待测假设。** [io/stl.rs](src/io/stl.rs)、[io/volume.rs](src/io/volume.rs) 多文件加载/保存和解码主要串行。[load_shape_library](src/pipeline/placement_library.rs) 对源 STL 分别进行摘要读取与解析读取；这是真实的重复 I/O，但是否为瓶颈取决于文件规模、缓存和存储介质。

**算法优化：**

1. 统一可从 reader/字节流解析的内部入口，让摘要和解析消费同一原始字节流。不要将当前有界内存的文件摘要改为无条件整文件缓存。
2. 独立 STL 文件、RAW/TIFF 文件可采用有界预读/解码队列，按原排序收集；同一多页 TIFF reader 的状态推进不能直接并发共享。
3. 大 binary STL 可先并行解码独立 triangle records，后续按原 face 顺序确定性去重；先确认解析而非磁盘吞吐占主导。ASCII 不直接套固定记录切分。
4. 输出预先分配逻辑顺序与文件名，编码/写盘使用有界队列，错误返回和清理策略保持可追踪；禁止吞掉后台写入失败。
5. 多次 merge 时预估容量、复用内存或流式输出，减少顶点/face 容器反复扩容。

**验收：** 原始文件 hash、解析出的源身份、切片顺序、字节序、数值类型和 roundtrip 都不变。分别测热缓存/冷缓存近似场景、峰值 RSS 和 I/O 吞吐，避免用 OS 缓存差异冒充算法收益。

### PERF-19：修复 GPU 验证入口，并形成每项优化的回归闭环

**证据：运行确认。** 第 2 节列出的 GPU 场景测试无法编译；渲染通过不覆盖 S2/voxel/crop 的大型边界条件。现有测试通常在没有适配器时打印消息后返回，这种“通过”不能作为硬件执行证明。

**实施：**

- [x] 修复 `mesh_render_tests` 的导入和结构字段；GPU 特性下 18 项场景测试通过，另有 13 项 STL 渲染测试通过，未修改图像基线。
- [ ] 为 PERF-01/02 增加执行观察点，测试实际后端、方法与 worker 数；不要只测试配置反序列化和打印文本。
- [ ] 给 PERF-04/05/14 的边界建立最小反例。小规模在 CPU/llvmpipe 上可运行，大规模资源限制优先用 planner 测试，避免为测试故意分配不可控资源。
- [ ] GPU 测试区分“未发现设备而跳过”“软件 adapter 执行”“硬件 adapter 执行”。专门的硬件验证任务无硬件时应明确未完成，不能绿色代替。
- [ ] 加入真实硬件 release 基准，尽量覆盖一类集成显卡和一类独立显卡；当前机器不具备这些条件，相关验收保留待完成。
- [ ] 每个实施变更同步 `AI-FUNC-SUMMARY`、`AGENTS.md`、对应英文文档和中文镜像。当前新增计划文件不代表函数契约已变更。

**范围控制：** 本计划不要求修复测试构建时顺带出现的 Mesh Gen 警告或运行未完成的网格生成用例。若将来完整 CI 被无关问题阻塞，应分别记录本范围内的结果和独立阻塞。

## 6. 建议实施批次

| 批次 | 工作 | 退出条件 |
|---|---|---|
| A：建立可信入口 | PERF-00、01、02，PERF-19 的现有 GPU 测试修复 | 方法/后端/线程观测准确；可复现基准；已有范围内测试可运行 |
| B：GPU 正确性和资源控制 | PERF-04、05，PERF-14 的类型/精度保护 | 边界反例可处理；无静默截断；同方法 CPU 回退可靠 |
| C：CPU 和几何工作量 | PERF-06、11、12、13，PERF-10 的缓存/回滚基础 | 查询结构复用；候选/复制工作量下降；确定性和几何结果通过 |
| D：GPU 数据流与 S2 | PERF-03、07、08、09，PERF-10 的部分上传 | 资源复用、分块归约和设备常驻通过；端到端收益有实测 |
| E：其他管线热点 | PERF-14 的 PCA/分块、15、16、17、18 | 按 profile 排序实施；小数据无明显回退，大数据收益可复现 |
| F：硬件与文档收尾 | PERF-19 剩余硬件验证，各任务文档与基准归档 | 本范围的默认特性/GPU 特性功能门禁通过，性能结论可追溯 |

每批可拆成小变更；同一变更尽量只涉及一种算法或资源策略。GPU BVH、位集 exact、增量 S2、并行连通分量等复杂方案必须保留前一版本作为可比较路径，测得收益后再决定是否默认启用。

## 7. 基准矩阵与验收规则

### 7.1 样例矩阵

| 领域 | 最少覆盖的变化 | 核心输出与指标 |
|---|---|---|
| pack / placement | 粒子数增长、三角形复杂度、宽粒径比、低/高 VF、gap、嵌套、周期或 clip、带 void | 固定候选接受序列、拒绝分布、VF、候选数、shape 构建数、每次尝试时间 |
| optimize | 1/多岛、worker 数、MC/exact、粒子数、接受/拒绝/迁移、周期边界 | 相同求值预算的 loss 与总时长、每步复制/上传、最终质量分布 |
| S2 | 小/大网格、长薄网格、radii 与 samples、offset 大于轴长、大坐标、交点溢出 | 穷举计数/曲线、query/s、内存、上下行字节、无支持壳处理 |
| crop | 浅宽/大三维卷、旋转角、近退化 PCA、nearest/trilinear、强度类型 | bounds、像素/标签误差、扫描/重采样/写出时间、tile 接缝、峰值内存 |
| split-filter | 很多小分量/单大分量、各过滤项、seeded rebalance | 分量身份与输出集合、指标计算时间、分配量 |
| forge / scale | 顶点数、void/solid、不同轴/尺度、ROI | 坐标/体积/ROI、变换与 I/O 分项时间 |
| render | 三角形数、分辨率、1/多视图、透明/不透明、线和标记 | 图像误差、scene 准备次数、帧时间、整图常驻数 |

性能规模按机器预算递增，不能为了跑“大例子”跳过资源限制。缺少真实输入时先使用合成数据，并明确尚无真实数据结论。

### 7.2 测量方法

- 固定 commit、Cargo features、release 配置、输入/config 摘要及输出目录；记录 CPU、内存、GPU 名称/类型、驱动信息和实际 worker 数。
- CPU 用 1、2、4、8 及机器允许的最大 worker 数；线程较少的机器只运行适用组合。比较 `S(p)=T(1)/T(p)` 和 `E(p)=S(p)/p`，不以 CPU 利用率单独判断成功。
- GPU 分别报告冷启动和复用资源后的总耗时。硬件与 llvmpipe 的结果分组，软件后端不能进入硬件加速比结论。
- 短基准预热后多次测量，建议至少 5 次并保留每次值、中位数及范围；p95 等尾部统计需要足够样本，不从 5 次运行推导稳定结论。
- SA/MC 不只比较一次随机运行。固定求值预算、多种基准种子，比较达到相同质量的时间及最终质量分布；减少样本数导致更快必须注明为算法精度变化。
- 正确性测试通过后才比较性能。对小任务的并行回退、内存增长和 CPU 回退性能也进行检查。

### 7.3 合入门槛

1. 任务所对应的正确性与契约测试通过，未以修改基线或放宽容差掩盖差异。
2. 性能优化项至少在一个目标真实/代表性负载上有超出测量噪声的端到端收益，且已解释收益来自何种工作量减少。纯正确性、配置和资源保护修复不以提速为合入前提；即使耗时增加，也应如实记录成本并保留必要保护。
3. 小数据路径、内存峰值、失败回退和资源预算无未解释的退化。若存在有意取舍，提供数据和明确启用条件。
4. placement 的确定性继续成立；S2、PCA、渲染等数值路径按各自定义验收。
5. 没有适用硬件时可以先完成 CPU/软件后端阶段，但硬件 GPU 收益项保持未勾选。

## 8. 后续实施时的验证命令

以下是未来实施的命令清单，并非本文新增后重新执行的结果。应按变更范围选择测试；文档单独变更不需要重复计算回归。

```bash
cargo test --offline --test pipeline_smoke_tests --test render_tests --test placement_pipeline_tests --test pack_target_tests --test mesh_render_tests
cargo test --offline --test core_tests --test collision_tests --test io_tests
cargo test --offline --features gpu --test render_tests --test mesh_render_tests
cargo test --offline --features gpu --lib compute::tests -- --nocapture
cargo build --offline --release
cargo build --offline --release --features gpu
```

新增 S2/资源/线程专项测试后把准确 target 名称补入本节。离线依赖不齐时先说明缺失依赖；不要将无法构建误记为测试失败。图像基线正常运行只比较，重新生成必须是有明确理由的独立动作。

代码实现按项目要求执行对应 lint；全项目检查出现本范围以外的问题时单独记录，不扩展到 Mesh Gen 修复。

## 9. 进度记录方式

每项 PERF 任务完成时，在该项下补充：实际修改、保留/放弃的方案及依据、基准输入与命令、前后原始数据位置、正确性测试、已知适用边界，并更新复选框。只完成编译或功能测试，不能勾选“性能收益已验证”。

本文建立时所有优化实施项均未完成。以下记录区分首批已实现内容与整项验收，未勾选项仍需后续实施。


## 10. 首批实施记录（2026-09-12）

**范围与状态：** 已开始修复，不表示整份计划完成。未修改 `src/meshgen/`、Mesh Gen 算法或图像基线。

- **PERF-02（placement 部分完成）：** `run_placement` 通过 `with_placement_pool` 创建并安装专用 Rayon 池，覆盖准备、几何和标签输出。`run_placement_in_pool` 从执行池读取实际 worker 数。新增单元测试在嵌套 `par_iter` 内检查 1/2/8 worker 数及索引；集成测试逐字节比较记录、STL、CSV、相位 TIFF、粒子 ID TIFF 和头文件。随机数、接受顺序和体积累计仍串行。optimize 岛模型预算、measure 回退和其他执行上下文未改。
- **PERF-13（查询去重首步完成，整项未验收）：** 保留原 x/y/z 桶遍历与首次遇见顺序。小查询沿用线性去重；一个桶处理完后累计至少 256 个不同 ID，才启用查询私有 HashSet。集合只用于成员判断，绝不遍历，支持稀疏及 `usize::MAX` ID，无跨查询共享可变状态。保留旧扫描作为测试 oracle，覆盖排除、重复插入、反向 bbox、越界钳制、不同 margin 与空网格。尚未实现缓冲复用、generation stamp、增量更新和粒径分层。
- **PERF-19（恢复入口完成）：** 补齐 GPU 场景样例的两个线框计数字段和 `SceneSegment` 导入。GPU 场景 18 项、STL 渲染 13 项、compute 6 项通过。适配器为 **llvmpipe (LLVM 21.1.8, 128 bits)**，不是硬件 GPU 验收。
- **PERF-00（局部基线）：** 已提供完整空间查询的 release 入口和真实 STL placement 对照；尚未完成所有管线分阶段计时、查询计数、GPU 传输观测及硬件矩阵。

### 10.1 空间查询 release 原始数据

命令：`cargo test --offline --release --lib geometry::spatial::tests -- --include-ignored --nocapture`。
均为同一二进制内的新旧完整查询比较：4×4×4 桶，各对象跨全部 64 桶，热身后五次交替测量。
每次重复次数依次为 10,000 / 1,000 / 100 / 20。单位均为 **微秒/查询**，不含网格构建。

| 不同候选数 | 旧线性：五次原始值 | 新实现：五次原始值 | 中位数 |
|---|---|---|---|
| 8 | 1.072, 1.096, 1.077, 1.114, 1.09 | 1.138, 1.115, 1.13, 1.12, 1.127 | 1.090 → 1.127 |
| 64 | 25.289, 25.52, 25.025, 25.119, 25.819 | 25.343, 25.572, 25.372, 25.35, 25.266 | 25.289 → 25.350 |
| 256 | 341.535, 354.657, 343.084, 376.32, 347.37 | 245.789, 243.908, 236.813, 244.658, 234.559 | 347.370 → 243.908 |
| 1024 | 5313.66, 5344.935, 5362.08, 5368.32, 5366.48 | 999.88, 996.54, 1010.555, 1002.43, 998.5 | 5362.080 → 999.880 |

第一次尝试在 32 个 ID 时切换 HashSet：64 候选由约 27 µs 退化至 59 µs，故没有保留该阈值。
256 阈值下，64 候选基本持平，256/1,024 候选约快 1.42/5.35 倍；8 候选多约 0.04 µs（约 3.4%），
来源为每桶的模式分支。结论只适用于这个重复引用负载；不能推广为 placement 或 optimize 的端到端加速。

### 10.2 真实 STL 端到端对照

基线来自计划指定 commit `891badc09ae03fb99ab57d206451e75de226ee60` 的独立 `/tmp` 源码副本，
与修改后程序均使用默认特性 release 构建。输入为 `data/input/particles.stl`，配置为
`data/input/placement_config.yaml`，命令参数 `pack --config <原配置绝对路径> --threads 1 --output <独立目录>`。
每个版本先预热一次，随后交替运行五次。Python `perf_counter` 测完整子进程，GNU time 记录峰值 RSS；
输出路径位于 `data/output/performance/20260912-initial/`，没有覆盖原 placement 输出。

- 输入 SHA-256：`77fc24b9144a6541d604777ff697bc1fed24d2ad5d3a86239e2b068e508bc2f1`。
- 配置 SHA-256：`9a902e56951b9257f843930c9976bec7b03692bc3c32d80b7d8d7640f3aa9bf4`。
- 环境：Linux aarch64；CPU implementer `0x41`、part `0xd4c`；本次 placement 使用 1 worker。
- 两版均放置 82 个颗粒、185 次尝试、VF 0.099372，停止原因 `target_reached`。
- 12 次运行的 STL 与 CSV SHA-256 一致；颗粒 JSON 排除构建身份 `tool` 后完全一致。
  基线归档没有 `.git`，故构建身份本来就不同，不能将其当作几何回归。

- baseline：秒数 0.446456, 0.479278, 0.446179, 0.422999, 0.487355；中位数 0.446456 s；峰值 RSS 原始值（KiB）17592, 17532, 18388, 17388, 17856.
- candidate：秒数 0.447729, 0.456882, 0.470081, 0.465558, 0.435926；中位数 0.456882 s；峰值 RSS 原始值（KiB）16332, 16332, 16328, 16432, 16332.

两组时间范围重叠，中位数差约 +2.3%，**本真实样例未证实端到端加速**。不据此勾选 PERF-13 的整项
性能验收；后续需观测真实候选/重复引用数，并寻找或构建确实受广相位去重限制的代表性整管线负载。
线程池修复属于配置契约修复，不以提速作为前提。

### 10.3 验证与可追溯文件

- 默认特性六组集成测试共 74 项通过：collision、mesh_render、pack_target、pipeline_smoke、placement_pipeline、render。
- 空间顺序 oracle 与实际 worker 观察各 1 项通过；release 查询基准也验证新旧完整结果一致。
- 最终二进制另以 2 workers 运行同一绝对配置路径，记录（排除 tool）、STL、CSV 与基线一致；最终 placement 集成回归 16 项通过；GPU 渲染及 compute 共 37 项通过（软件适配器）。
- 默认特性 release 构建通过；`cargo clippy --offline --features gpu --lib --test placement_pipeline_tests --test mesh_render_tests` 通过，有 47 条现有警告，无本批修改文件的告警。未扩展修复 Mesh Gen 警告。
- 原始日志、两版二进制、配置摘要、完整报告和对照脚本：`data/output/performance/20260912-initial/`。
  `spatial-benchmark-hash32.log` 保留被否决方案；`spatial-benchmark.log` 是最终查询策略；
  `placement-raw.json` 与 `placement-benchmark.log` 保存整管线数据；`compare_placement.py` 可重复该对照。
- 已同步 AGENTS.md、英文算法/函数文档及中文镜像。本仓库没有 PLAN.md，实施进度记入本文件。

**首批结束时安排的下一步（现已在第 11 节实施）：** PERF-01 先统一 optimize 单岛/多岛的后端决策，确保 CPU/auto 阈值真正控制 GPU 初始化；
在初始、候选、迁移和最终复核处固定同一 S2 方法。随后处理 PERF-02 的岛模型总 worker 预算。


## 11. Optimize 执行语义与岛模型预算（2026-09-12）

### 11.1 已完成的具体范围

- **PERF-01 / PERF-05 方法部分：** 新增内部 `optimize_execution.rs`，一次解析 `voxel_exact`、`voxel_mc`、`mesh_mc`。
  目标、输入、剪枝、初始、候选、迁移和最终复核均使用相同 `OptimizeS2`，保持各自 VF 定义。
  现有 GPU MC 仅适用于连续网格 MC；体素方法保留 CPU，不因有 GPU 就换目标函数。
  exact 非正 pitch 一次规范为 1.0，原非 exact 字符串仍沿用旧 MC 路由。
- **模式及回退：** optimize 读取一次 `RUSTMSPT_ACCELERATION` 覆盖 YAML，非法值报错。
  CPU、低于阈值的 auto 在探测之前选 CPU；小 auto 是正常策略选择，不受禁止回退影响。
  真正尝试 GPU 时，方法不支持、设备不可用或管线初始化失败遵循 `cpu_fallback`，禁止回退则错误退出。
  日志和历史记录实际后端/方法/有效 pitch/原因。
- **GPU 支持边界：** 可进入 GPU 的 mesh MC 对不支持的 backend/precision/power 配置报错，当前支持 wgpu/f32、
  `gpu_prefer_power: false`。显式内存预算在工作集规划器完成前保守使用 CPU（或禁止回退时报错），
  不将单 binding 上限伪装成工作集预算。超过 128 个半径或 `65535*256` 调用容量时，在探测前拒绝 GPU 路径。
- **PERF-02 optimize 部分：** 全管线安装到一个共享 Rayon 池，包含参考目标、VF 和回退。
  `run_island_batches` 每批最多 worker 数个岛上下文；多余岛排队，嵌套任务窃取也不能激活全部岛。
  结果按岛编号收集，各岛自己的温度/随机状态独立；单岛直接消耗准备向量。
- **PERF-03 局部工作：** 所有阶段和岛复用一个持久 GPU MC 实例，通过互斥锁串行保护上传、dispatch 和回读。
  **CPU VF 工作前释放 GPU 锁**，迁移计算前释放全局锁，避免嵌套 Rayon 工作等待调用方持有的锁。
  能力探测仍单独初始化临时 device，不表示完成共享 GPU context。
- **迁移与最终复核：** 原迁移只采用几何/loss、不更新 best_s2；现在三者一同交换。
  最终在可选定向之后，以同一方法/完整样本预算复核，分别写 `Selected Search Loss` 和 `Final Best S2/Loss`。
  MC 复核可因噪声不同于历史最佳分数，明确记录，未改变历史分数选择规则。

### 11.2 回归证据

- 默认特性执行器/实际迁移单元测试 **6 项通过**；GPU 特性对应 **7 项通过**。
  在真实 evaluator 边界检查 1/2/8 worker 数、线程索引和各阶段；更多岛/嵌套工作压力测试检查活跃岛上限和完整顺序。
  迁移测试检查最佳几何、loss、S2 同属一份快照。
- 默认/GPU 特性下新的 CLI 集成测试各 **3 项通过**；两种构建的既有 pipeline smoke 各 **11 项通过**。
  覆盖 CPU/auto/GPU 请求的单岛与排队多岛、reference_stl、环境覆盖、非法环境值、禁用回退及设备不存在。
  exact 最终曲线独立对照写出的 STL，而非只断言打印字段。
- 真实 GPU MC 测试在 **llvmpipe (LLVM 21.1.8, 128 bits)** 执行，无跳过：2 worker 下 5 个任务交替上传两种几何，
  共用一个实例，检查连续 VF、有限概率，并与同定义 CPU mesh MC 比较非零半径。硬件 GPU 未验收。
- 默认 release 构建通过；GPU 特性 Clippy 通过，46 条警告为既有代码问题（包括 optimize 原有的 `rounds % 5`），
  未为性能工作扩展修复无关 Mesh Gen 警告。

命令：

```bash
cargo test --offline --lib pipeline::optimize
cargo test --offline --test optimize_execution_tests --test pipeline_smoke_tests
cargo test --offline --features gpu --lib pipeline::optimize -- --nocapture
cargo test --offline --features gpu --test optimize_execution_tests --test pipeline_smoke_tests
cargo clippy --offline --features gpu --lib --test optimize_execution_tests
cargo build --offline --release --bin rustmspt
```

### 11.3 同方法资源与端到端基准

脚本及全部原始值：`data/output/performance/20260912-optimize/compare_optimize.py`、`optimize-raw.json`、
`optimize-benchmark.log`，每次的配置、日志、GNU time 输出分别存放于独立子目录。
基线为 `891badc09ae03fb99ab57d206451e75de226ee60` 默认特性 release 二进制；新旧均显式 CPU、`cpu_max=2`、exact，
参考目标为输入本身、关闭剪枝、**max_iterations=0**。这组基准隔离初始化/并发资源开销，不是 SA 收敛或 GPU 加速比。
每种配置各预热一次，随后交替测五次。用 `perf_counter` 测子进程 wall time，GNU time 记录峰值 RSS，
每约 2 ms 观察 `/proc/<pid>/status` 的进程 OS 线程数（包含主线程，不冒充 worker 数）。

- single / queued17：32³ 域内 12³ 立方体，pitch=1、r_max=2，分别 1 / 17 个岛。
- real9：首批真实 STL placement 的 82 颗粒输出，100³ 域，pitch=10、r_max=2，9 个岛。
  这是明确固定的粗体素资源样例，不能将其精度或耗时与细网格生产设置混比。
- 所有 36 次运行的新旧 STL SHA-256 及 exact 最终曲线按样例分别一致；没有减少新版本的求值精度。
  新版另外进行一次完整最终复核，属于明确新增的验证成本。

| 样例 | 版本 | 五次秒数 | 中位数秒 | 五次峰值 OS 线程 | 五次峰值 RSS（KiB） |
|---|---|---|---|---|---|
| single | baseline | 0.066916, 0.038954, 0.043287, 0.040281, 0.039824 | 0.040281 | 11, 11, 11, 11, 11 | 40760, 41872, 41168, 40820, 40488 |
| single | candidate | 0.047971, 0.045538, 0.042845, 0.047051, 0.044797 | 0.045538 | 3, 3, 3, 3, 3 | 18020, 18020, 18020, 20152, 18020 |
| queued17 | baseline | 0.142794, 0.125497, 0.125971, 0.189348, 0.145285 | 0.142794 | 44, 45, 43, 43, 42 | 177528, 189024, 161876, 168772, 172084 |
| queued17 | candidate | 0.202883, 0.179451, 0.182759, 0.178856, 0.174229 | 0.179451 | 3, 3, 3, 3, 3 | 27748, 27752, 27752, 27736, 27844 |
| real9 | baseline | 0.416441, 0.437854, 0.428404, 0.430423, 0.433890 | 0.430423 | 21, 21, 25, 27, 20 | 122036, 117052, 143232, 143724, 120928 |
| real9 | candidate | 0.468017, 0.449391, 0.456446, 0.455019, 0.454710 | 0.455019 | 3, 3, 3, 3, 3 | 49188, 49188, 49188, 49188, 49204 |

**结论与取舍：** 新版进程稳定 3 个线程（主线程 + 2 worker），旧版即使单岛也因池外参考求值激活全局池而达到 11；
17 岛达到 42–45。17 岛 RSS 中位数从 168.1 MiB 降到 27.1 MiB（约 −84%），真实样例从 119.2 MiB 降到 48.0 MiB（约 −60%）。
但 wall time 中位数分别增加约 13%、26%、6%：旧版实际使用了超过请求预算的 CPU 并行，新版遵守预算并增加最终复核。
**这是配置/资源正确性修复及内存改善，不能宣称整体提速。** SA 的收敛质量/达到目标时间、岛间与岛内预算组合仍需后续固定预算多种种子实验。
上述数据已用最终二进制刷新；基准目录保留 baseline/candidate 二进制、SHA-256 和脚本，初次数据另存为 `*-before-final.*`。

### 11.4 未完成项与下一步

本批未修改 Mesh Gen。已同步 AGENTS.md、英文及中文算法/函数/配置/示例说明。
PERF-01 其他管线（尤其 crop 和 measure）的统一执行决策、PERF-02 measure CPU 回退范围仍待办。
GPU 运行时错误传播与回退、射线 64-hit 溢出、f32 数值认证、voxel 参数布局、完整工作集规划/分块、共享设备仍是 PERF-04/05/03。
下一步优先建立 GPU S2/voxel 的布局与边界反例，修复能导致错误输出的问题，再扩展资源规划和运行时回退。

## 12. 第三批：GPU S2 / voxel 布局、坐标与 shell 边界（2026-09-13）

### 12.1 已完成（PERF-05 的一部分）

- Voxel 参数提取为独立打包函数，保留 48 字节布局；ray_dir 三个分量从 byte 32 开始，与 WGSL vec3 对齐一致。
- MC / voxel 两份三角形上传实现均先在 f64 中减去 bbox 原点，再转为 f32。
- Shell shader 在计算无符号区间前检查三个轴的位移绝对值；等于/超过尺寸的偏移返回零有效对，不再发生减法下溢。
  `i32::MIN` 的幅值用无符号运算处理；host 对无法用 i32 表示的 isize 偏移使用域外哨兵，避免截断后变成有效偏移。
- Shell 偏移列表按最多 200,000 项分批上传、dispatch、读回，共享跨批次的 sum/count；不再向固定缓冲区上传超量列表或丢弃尾部。
  保持每个有效偏移的 hits/valid **等权平均**，不是总 hits / 总 valid，也不是对每批平均值等权平均。
  占据缓冲只上传一次，空偏移列表不 dispatch / map，返回 `[vf, 0, ...]`。

### 12.2 反例与验证

日志目录：`data/output/performance/20260913-gpu/`。

- 修复前 `before.log`：3 个新单元测试均失败。WGSL 对齐位置读取到 `[0.19611613, 0, 0]`，预期为
  `[0.94280905, 0.27059805, 0.19611613]`；平移到 ±1e9 的测试盒，其 MC / voxel 上传坐标全部退化为零。
- 修复后 `after-unit.log`：上述 3 个单元测试通过。该命令的 integration 目标被 `gpu::` 过滤，**不计入通过数**。
- `boundaries.log`：另行运行 2 个 GPU integration 测试，均实际执行并通过，无 SKIP。
  覆盖原点附近及 1e9 平移下逐体素解析占据、3×2×4 非对称网格、逐对 CPU 枚举 oracle、各轴正负等边界/越界、
  i32::MIN / isize::MAX、200,001 项列表（尾项是唯一非零贡献）以及空列表。
- `optimize-gpu.log`：上一批 optimize 的 7 个 GPU-feature 单元回归通过，含共享 GPU MC 几何更新测试。
  适配器为 **llvmpipe (LLVM 21.1.8, 128 bits)**，属于软件实现，不代表真实 GPU 性能或跨厂商验证。
- `default-regression.log`：默认构建 core 11、optimize CLI 3、pipeline smoke 11，共 25 项通过。
- `clippy.log`：`cargo clippy --features gpu --lib --test gpu_s2_boundaries_tests` 成功，46 条已有告警；未修改无关 Mesh Gen 代码。
- `release.log`：`cargo build --release --features gpu` 成功。

这是错误输出与固定缓冲区越界风险的修复，没有进行本批前后吞吐基准，**不宣称整体提速**。
分批只约束 offset 上传及对应读回工作集，不能称为完整显存预算管理；输入占据网格仍整体驻留。

### 12.3 当前状态与下一步

本批实现与中英文算法/函数文档、函数索引、AGENTS.md 已同步；变更未提交，Mesh Gen 不在范围内。
PERF-05 未完成：射线 64-hit 溢出检测/回退、共享 MC API 的半径上限保护、统一 pitch / VF 语义与数值认证仍待处理。
PERF-04 的完整 checked arithmetic、工作组/缓冲区预算、GPU 运行时错误传播和按策略 CPU 回退仍待处理。
下一步优先处理 64-hit 静默截断及 GPU 错误传播，使超出能力边界的输入不会静默返回不可信结果。

## 13. 第四批：GPU 射线 64-hit 溢出恢复（2026-09-13）

### 13.1 已完成（PERF-05）

MC 和 voxel shader 不再静默忽略第 65 次及以后的正向三角形命中。
保留最多 64 次原始命中的排序快速路径；检测到第 65 次命中时，在 GPU 上重新扫描**同一条射线**，
逐轮选出下一个不同的最近交点，直至没有后续交点，再计算奇偶性。
每轮与上一个**保留**交点比较 `t - last_t > 1e-6`，与原 GPU 排序路径的锚定去重一致。
容量计数包含重复三角形命中，不能只对不同交点计数后才检测溢出。
不丢弃或重抽 MC 样本，不增加 CPU 传输，不更改 VF 或 S2 方法。

选择该恢复方式是为了保留原采样及 GPU 容差语义；本批没有扩展 Vec 返回接口，也没有把设备/map 失败当作已处理。
新增两份 WGSL `point_inside_overflow` 函数，分别供 MC 和 voxel 使用，并已同步英文/中文函数契约、索引、算法与 AGENTS.md。

### 13.2 反例与验证

日志目录：`data/output/performance/20260913-ray-overflow/`。

- `before.log`：修复前两个新反例均失败。沿固定射线排列 33 个分离盒子（66 次交叉），再加一个包围查询点的盒子，
  应为奇数 67 次交叉；旧 voxel 返回 0 而不是 1，旧 MC 对完全实心的采样域返回 S2(1)=0 而不是 1。
- `after.log`：GPU 边界 integration 共 5 项通过，实际执行，无 SKIP。
  溢出测试覆盖 31/32/33 个远端盒子、查询点内/外状态（62–67 次不同交点）、三角形顺序反转、64/65 份重合三角形的去重。
  MC 覆盖整个采样域实心/空心的解析结果；上批 voxel 大平移、shell 分批/越界回归也通过。
- `optimize-gpu.log`：7 项 optimize 单元回归通过，含共享 GPU MC 几何更新；适配器是 llvmpipe 软件实现。
- `clippy.log`：`cargo clippy --features gpu --lib --test gpu_s2_boundaries_tests` 成功，46 条已有告警。
- `release.log`：`cargo build --release --features gpu` 成功。

### 13.3 性能取舍与下一步

普通射线保留原排序路径；溢出恢复额外空间为常数，计算成本 O(T×U)，最坏 O(T²)，其中 T 为三角形数量、U 为不同正向命中数。
极端密集几何可能明显变慢，设备运行时失败仍未封装成 Result；没有本批吞吐基准或硬件 GPU 测试，不能宣称提速。
这是避免错误奇偶性输出的完整扫描恢复，不是 CPU f64 数值认证；GPU 的 1e-6 与 CPU 的 1e-8 去重容差仍不同。

当前本批实现及验证完成；未提交，Mesh Gen 未修改。PERF-05/04 整体仍未完成。
下一步：将 GPU 执行/读回改为显式错误结果，并贯通调用层的允许回退/禁止回退策略；同时补齐半径、dispatch 与缓冲区大小的前置检查。

## 14. 第五批：MC 执行错误、容量检查与 optimize 回退（2026-09-13）

### 14.1 已完成（PERF-04/05 的 MC 子链）

- `GpuS2Pipeline::update_mesh` 与 `calculate_s2_gpu` 改为 `Result<_, String>`，更新仓库调用点和本地 precision 工具。
- MC 在分配前检查 r_max < 128、usize→u32 采样数量、调用数乘法、工作组上限、storage binding/max buffer 字节上限。
  三角形展开前检查计数/字节容量，空几何使用 4 字节占位；bbox 尺寸转为 f32 后须有限且为正，拒绝正 f64 下溢为零的情况。
- 新增 `gpu/runtime.rs`：配对收集 validation/out-of-memory/internal 错误作用域；map 回调成功后才访问映射内存，复制后解除映射。
  MC 初始化、上传、执行接入；提前返回 Err 时也会弹出全部作用域。此边界不捕获 Rust panic。
- Optimize 所有阶段求值、剪枝和岛返回 Result。允许回退时，在锁内移除失败 GPU 实例，释放锁后重算 CPU mesh MC；后续阶段保持 CPU，
  warning 记录阶段和原因。禁止回退时向管线传播带阶段信息的 GPU 错误，不写最终 STL/history。已启动岛批次仍先完成等待。
- 旧 `calculate_s2_with_gpu` 的 Vec 包装接口记录执行错误后回退到连续 CPU mesh MC，保持 GPU 尝试的方法；它没有严格回退开关。
  Optimize 使用独立的、遵守 cpu_fallback 的 Result 求值器。启动日志描述初始后端，后续切换以 warning 记录。

### 14.2 验证

日志目录：`data/output/performance/20260913-gpu-errors/`。

- `gpu-unit.log`：5 项通过，包含 127/128/usize::MAX 半径边界、采样转换/乘法溢出、workgroup/storage/max-buffer 上限，
  真实 GPU 非法映射返回 Err 后继续正常调用、空几何和 f32 bbox 下溢，以及上批参数布局/平移回归。
- `optimize-unit.log`：8 项通过。GPU 选择完成后故意请求 r_max=128，验证允许回退会移除实例且后续保持 CPU，禁止回退返回 candidate 阶段错误。
  回退 VF 与现有 CPU mesh-MC 参考比较，不把该测试解释为 VF 解析正确性认证。
- `gpu-integration.log`：边界 5、optimize CLI 3、pipeline smoke 11，共 19 项通过；含射线溢出和 shell 分批旧回归。
- `cpu-integration.log`：默认构建 optimize CLI 3、pipeline smoke 11，共 14 项通过。
- 上述 GPU-feature 测试共 32 项，实际 GPU 路径无 SKIP，适配器仍为 llvmpipe 软件实现。
- `clippy.log`：`cargo clippy --features gpu --lib --tests` 成功；保留已有库及无关测试告警。
- `release.log`：`cargo build --release --features gpu` 成功。

### 14.3 当前状态与下一步

本批实现、文档及 release 验证已完成，尚未提交；Mesh Gen 未修改。
本批是可报告错误及按策略回退的修复，不宣称提速。同步作用域与读回有开销，未进行吞吐基准。
单缓冲区/dispatch 检查不是完整工作集预算；既有 MC 输出预分配、高水位保留、GPU 配置总预算仍待处理。
未测试真实设备丢失或物理 OOM，捕获错误不等同于所有驱动故障都可恢复。
Voxel/shell/crop/render 未接入该 runtime；measure 管线级禁止回退策略与共享包装层方法选择仍需统一。
下一步优先贯通 voxel/shell 的 Result、网格乘法/dispatch 边界和 exact CPU 回退，再扩展完整工作集规划。

## 15. Voxel/shell 错误传播与 measure 方法策略（2026-09-14）

已实现：voxel/shell 构造及执行 Result，检查网格乘法、storage/buffer 容量、二维 dispatch 和读回错误；GPU exact 新增不隐式回退的入口。Measure 全流程安装配置线程池，按方法解析 backend、预算和回退，positive-pitch MC 保留 voxel 定义。exact 超过旧安全阈值明确拒绝而不偷换 MC；阈值还需 PERF-07 完整内存模型替代。CPU fallback 保持同一方法，输出实际 backend。

验证目录 `data/output/performance/20260914-exact/`：GPU 边界 5、smoke 11、GPU unit 7、measure contract 2 项通过。适配器仍为 llvmpipe；不是物理 GPU 验收。加强后的二维 dispatch 尾行测试随下一次 GPU 全面回归复验。GPU resident occupancy、shared context 和完整工作集预算仍未完成。

## 16. CPU 几何准备、MC 样本分块与标签候选（2026-09-14）

已实现：不可变 PreparedMeshQuery 缓存 bbox/BVH，保持原 CPU 三角形谓词、固定射线和 1e-8 anchored 去重；小模型直接扫描，线程任务复用 hit scratch。Voxelization 每分量准备查询，并修复域下方分量负上界转 usize。连续 MC 改成 2048 样本块，seeded 诊断接口在 1/2/8 workers 下曲线逐值一致；普通 MC 每次随机 base seed，原未固定种子的 RNG 协议发生变化，尚须 SA 收敛实验。

Placement 标签按 1024 体素 tile 查询空间候选，候选恢复原粒子顺序，保持 void 优先和 ownership；每粒子复用 prepared query。完整 phase/id 数组仍驻留内存，流式写出未完成。Measure CPU exact/voxel MC 开始复用一个延迟构造的 VoxelS2 网格，避免 both 重复 voxelization。

验证与基准位于 `data/output/performance/20260914-prepared-query/`：查询对照覆盖嵌套、共享面/边/顶点、缩放和大平移；placement 16 项通过。`seeded-benchmark.log` release 真实 STL（5600 faces），每种 workers 首次热身加 5 次，full-scan 与 prepared 使用完全相同 seed/采样且每次曲线相等。中位耗时：1 worker 0.886929→0.012696 s（69.9x），2 workers 0.444258→0.008214 s（54.1x），8 workers 0.222407→0.007549 s（29.5x）。包含 VF/准备，不代表所有输入或整个 optimize 提速。

`measure-raw.json` 为早期候选 CLI 对照，未固定旧 CLI seed，仅作时延参考；其 candidate SHA 对应当时实现，不能冒充最终候选。PERF-00 完整矩阵、PERF-06/07/12 全部验收仍有未完成项。当前未修改 Mesh Gen，尚未提交。

## 17. FFT scratch 与 SA 局部更新（进行中）

FFT 各轴共享 plan，process_with_scratch 保留任务 scratch；Y gather buffer 在任务内复用，功率谱与归一化使用同一池。2N-1 padding 保持，整数 pair oracle 覆盖长薄/不友好轴长、正负位移，已通过。尚未实现跨次 plan/transpose 缓存和完整成本模型，不能关闭 PERF-07。

SpatialGrid 增加反向 memberships、remove/update；SA 候选求值不修改网格，接受时只更新一个颗粒，拒绝直接恢复原 prepared particle，避免额外 mesh clone 和重新构建 QBVH。全体迁移仍重建；移动后的候选顺序可能不同，但只用于布尔碰撞判断。正在验证增量与重建候选集合、优化 CLI。反向映射内存与真实 SA 基准尚待测量。

最近 GPU feature 完整库回归：109 passed/1 ignored；S2 boundaries 5、measure contracts 2、optimize CLI 3，全通过，日志 `/tmp/perf-gpu-final.log`（下一步归档）。CPU shared-grid/query 4、measure 1、core 11 通过。

## 18. Legacy pack 缓存与融合检查（进行中）

候选阶段不再 placed.clone 或重复生成全部已接受颗粒 ghosts；接受后缓存 bbox/TriMesh 并增量插入粗网格。<32 colliders 使用缓存直扫，其余使用 grid 加 bbox-less 保守候选。positive gap 用 solid distance < gap 融合原 collision+distance 两遍，zero gap 保持 solid collision，保留 nesting/周期副本语义。全流程安装配置池，随机 proposal 顺序保持串行。已通过 smoke 11 与 pack-target/collision 回归；完整 oracle、性能和内存验证仍在进行。

## 19. Forge/scale 所有权与顶点分块（进行中，2026-09-15）

新增 forge_owned，公共 borrowed API 保留 clone-returning 语义；ForgePipeline 消耗输入、复用输入 bbox，移除未使用的 before/after 体积扫描。关闭 orientation 时 forge/scale 直接移动输出，避免再 clone。scale/translate/FFD 独立顶点使用大数组分块，void 后变换质心求和保持原顺序，ROI VF 仍实际裁剪测量，没有套用不正确的 determinant 简化。transform 秒数单独记录。

core 11、smoke 11、专门对照 2 项通过：0/12/32767/32768/65539 vertices、1/2/8 workers、正/负/零 scale、三个轴与默认轴、void、ROI、大坐标逐点相等。release 大小数组基准正在执行，以此核实并行 cutoff。PERF-16 的数值融合/统计捷径仍需测量决定，不以本批替代全部验收。

PERF-16 cutoff 调整证据：初始 32768 vertices / 8 workers，20 次 scale 中位串行 0.000587 s、分块 0.005350 s，出现明显回归；改为多 worker 且总量 >= max(131072, workers*65536) 才并行。200 万顶点初测：1/2/8 workers 串行约 0.0840/0.0826/0.0851 s，分块 0.0775/0.0661/0.0700 s。初始原始日志保存在 benchmark-initial-cutoff.log，最终阈值复测中；不把小任务回归隐去。

PERF-18 开始修复 STL 极小写入：64 KiB BufWriter 替代每个 f32 直接 File::write_all，显式 flush 错误传播。I/O 与 placement 回归通过，byte-identical writer release 对照正在执行；流式解析/摘要共读及 volume I/O 仍未完成。

## 20. STL 流式解析与 shape 摘要共读（2026-09-15，进行中）

load_stl_from_reader 保留前 512 bytes sniff、ASCII 优先与 binary fallback；binary 使用单个 50-byte record 增量解析与 dedup，避免整份原始文件缓冲和 tri_count*3 初始容量。load_stl_hashed 用 HashingReader 在同一 forward stream 解析+摘要，包含 binary trailer。Shape library 已接入，独立 sha256_file 仍保持有界读。ASCII 仍全量 text buffer，不宣称已实现全部文本/volume I/O 流式化。

验证：I/O 9、placement sizes 20、placement pipeline 16、transform 2、stream oracle 1 项通过。stream oracle 覆盖 3-byte 短读、模糊 solid/facet/vertex binary header、ASCII、trailer digest、截断 header/record 与 u32::MAX triangle count，不进行 count 大分配。write benchmark 初次因测试 Triangle 非 Copy 编译失败，已改 cloned，不能把失败日志当性能证据。最终 release benchmarks 正在运行，随后需无并发隔离复测。

## 21. SA 合并网格的局部顶点替换（进行中）

初始与迁移阶段 merge_prepared_particles 直接合并借用的 prepared 数据并记住每颗粒 vertex range；候选刚体变换只覆盖对应区间，拒绝恢复原顶点，不再 clone/merge 全体 mesh 与 faces。全体迁移重新建立布局。公共 merge_meshes 预估容量，避免几何容器反复扩容。优化 CLI 3 与 smoke 11 回归已通过；逐次更新/回滚/迁移与完整 merge 的 oracle 正在执行。GPU triangle upload 仍全量，增量 occupancy/VF 等 PERF-10 子项仍待完成。

本轮性能原始证据：20260915-transforms/isolated-benchmarks.log 和 isolated-raw.json（测试线程数 1，1 次 warmup +5 次，含 binary SHA）。32768 vertices /8 workers 调整后串行参考 0.0005544 s、当前 API 0.0003199 s（20 次，API 此时走串行）；200 万 vertices /2 workers 0.08451→0.06608 s，/8 workers 0.08419→0.07511 s。STL write 12/6000/60000 triangles：direct 0.0001443/0.04937/0.47861 s，buffered 0.0000129/0.0001705/0.0020562 s。write 每次逐字节比较，无 fsync；不是持久化磁盘吞吐或全部管线加速。

PERF-18 folder I/O：stl_paths 保留现有目录顺序；最多两个文件并行读取，结果/错误按 path 顺序消费。合并直接消耗每批输入，不再同时持有全文件夹 mesh 列表与合并副本。返回全部 meshes 的 load_folder_stls API 仍按其返回契约保留结果。批量/峰值内存基准仍待补齐。

本轮 GPU feature 回归日志 `20260915-stream-io/final-gpu-regression.log`：lib 112（另2项 benchmark ignored）、GPU boundaries 5、measure 2、optimize CLI 3、smoke 11、placement 16、stream 1、transform 2，全通过；这是合并网格范围/文件夹批处理前的快照，新增部分另有 targeted 回归，不混称同一源码快照。物理 GPU 硬件仍不可用。

2026-09-15 本轮检查收束：GPU release build 成功（release-gpu.log，1m14s）；合并网格 default/GPU unit 各 2 项、optimize CLI 3 和 smoke 11 通过；folder/stream 对照 2 项通过，目录顺序与 face offsets 一致。`git diff --check` 成功。以上证明本轮实现及相应范围，不构成整个 Plan 完成。未修改 src/meshgen，未提交。下一步继续 exact 工作集规划/FFT 复用、GPU 数据流及尚未处理的 crop/render/split-filter；相关 gate 保持未完成。

## 22. CPU exact FFT 工作区复用（2026-09-16，进行中）

FftWorkspace 缓存维度对应的正/反三个轴 plan、complex grid 和 transpose。with_fft_correlation 取出线程本地独占工作区，计算后按条件归还，不跨 Rayon 工作/consumer callback 持有 RefCell 借用；嵌套求值使用另一个工作区。生产 exact shell 直接读取 complex correlation 的实部，不再创建完整 f64 corr 副本。填充、功率谱和归一化保持在当前池中。

保留条件：两个数组 capacity 合计 <=16 MiB，且 padding 各轴 <=4096；FFT plan 内部存储额外占用，不宣称这是完整进程预算。换维度先丢弃旧缓存，超限大数组本次结束即释放。现有 exact 1.5M/FFT24M 限制暂保留，成本选择/完整内存模型仍待完成。

3 项 oracle 通过：所有正负 offset 对整数计数、1/2/8 workers 的 FFT/direct shell 对照、重复数据清空、缓存同地址复用、nested consumer 互不覆盖、超限数组释放。日志 `20260916-fft/oracles.log`。release fresh/cached 对照正在运行，不预报提速。

FFT 粒度实测与调整：初始缓存对照在 1/2 workers 多数提升约 3–8%，8 workers 有较大样本波动，不能据此宣称全面提速。首版 minimum grain 后 `[64,16,16]` /8 的 median fresh/cached 为 0.09254/0.09813 s（10 次），仍有退化；原始日志均保留。当前改为 tasks=min(pool_workers,ceil(P/65536))，tasks=1 时填充/三轴/功率谱/归一化均串行，不分配 transpose；多任务使用同一预算限制轴分块，正在复测。改造期间一次未分配 transpose 的中间实现已由 oracle 发现并修复，相关日志保留；不把失败或旧版本测试当最终验证。

最终 FFT 任务预算对照：`task-budget-isolated-benchmark.log`、`task-budget-raw.json` 记录 release binary/source SHA、1 次 warmup +5 次、每样本 10 次计算、测试线程 1。8 workers 下 `[32,24,16]` 缓存耗时 0.04662 s，上一版固定 grain 0.09324 s；`[64,16,16]` 为 0.06130 s，上一版 0.09813 s。小网格 `[8,8,8]` 的 1/2/8 workers 约 0.00054 s，消除了多线程任务调度造成的明显回退。新版本 fresh/cache 对照：缓存多数配置约 1–19% 收益，也有不足 1% 的负差异，不宣称普遍提速。原始旧版本及波动样本均保留。

当前 release oracle 4、默认 measure 1 / mesh query 4 / optimize CLI 3 全通过；GPU feature 最终回归正在执行。任务预算是 min(pool workers,ceil(P/65536))，不是创建更小的嵌套线程池。完整 exact 内存预算、FFT/direct 成本选择和 padding 实验仍未完成，1.5M/24M 硬阈值尚未取消。

§22 本轮收束：最终 GPU feature 回归 lib 116（3 项性能测试 ignored）、GPU boundaries 5、measure 2、optimize CLI 3 全通过。GPU release 构建成功（release-gpu.log，1m13s）；最终 clippy 成功（47 条库告警，含既有告警；final-clippy.log）；diff whitespace 检查通过。本批没有替换 FFT padding 或 exact 方法定义，软件 GPU 仍不代表真实硬件性能验收。后续先继续 crop/render 的配置和错误策略统一，再补全 exact 成本/内存模型；整个 Plan 保持实施中。

## 23. Crop 执行策略、数值与内存复用（2026-09-18，进行中）

CropConfig 新增 acceleration/cpu_max，全流程进入配置线程池。统一环境覆盖、auto 阈值（默认 250000）、显式 GPU、工作集预算和禁止回退策略。GPU 最近邻要求 i32，三线性另要求整数可精确表示为 f32，宽 U32 标签不再静默截断。构造遵循设备过滤；执行返回 Result，检查维度/容量/有限参数，二维 dispatch 与 checked readback/error scope。WGSL 半整数改为远离零舍入。

GPU feature 专项 2 项通过（软件适配器，无 skip）：策略矩阵、宽标签、禁止回退、实际 CLI CPU/GPU 对照、正负半整数、非法维度/模式、跨 65535*64 dispatch 边界尾部。CPU 重采样改用 4096 体素任务；边缘修剪消耗并原地压紧输出，零修剪直接复用内存。新增 allocation/row-order oracle 验证中。PCA 并行、完整性能矩阵和 f32 变换误差认证仍待完成，不据此关闭 PERF-14。


## 24. STL render 策略与错误检查（2026-09-21，进行中）

RenderPipeline 全流程进入 cpu_max pool，统一环境覆盖、像素阈值、实际工作集预算与禁止回退策略。GPU 构造/渲染使用 error scopes，纹理尺寸/顶点计数/缓冲容量预检查，映射回调错误明确返回。新增 CLI 策略矩阵，GPU 对照测试不再把初始化成功后的渲染失败当作无设备跳过。首轮 render 13、render policy 1、crop policy/GPU 2 通过；增加尺寸错误对照后的最终回归进行中。mesh-render 的策略统一与场景缓存仍待完成。

§23 首轮 crop tile release 测量（1 warmup+5 次）保存在 20260921-crop-render/crop-benchmark.log：1024×512×1 /8 workers，slice median 0.01003s、tile 0.00533s；256×256×32 /2 workers，0.02704s→0.03471s，有回退。调整为每行段计算一次索引/除法，逐体素保持原数值表达式，复测中。先前 /tmp 临时日志因环境切换不可用、进程句柄已不存在，已重新执行并保存仓库输出目录；未将缺失日志当当前版本验收证据。

Crop 行段版仍有 3–9% 多层回退，已保留 crop-row-benchmark.log。当前改为深度 >= worker count 时保留切片任务；浅层体数据才用行对齐分块（目标约 4096 体素，至少一行），避免为已有充足并行度的任务增加调度。最终 release oracle+benchmark 正在执行。渲染最终 feature 回归 16 项通过，没有 skip。

最终 crop adaptive release 3 项（含性能对照和两个 oracle）通过，default crop/render CLI 各 1 项通过。原始样本和 source/binary SHA 保存在 crop-adaptive-raw.json。中位耗时：1024x512x1/1 workers: 0.009433→0.010714s；1024x512x1/2 workers: 0.009272→0.005967s；1024x512x1/8 workers: 0.009295→0.003420s；256x256x32/1 workers: 0.046153→0.050418s；256x256x32/2 workers: 0.025657→0.027391s；256x256x32/8 workers: 0.013126→0.013935s。结果不代表所有形状均提速，多层负差异需后续完整矩阵继续评估；不关闭 PERF-14 验收。


## 25. CPU 多视图准备与命中临时数组复用（2026-09-21，进行中）

PreparedScene 借用不可变 scene，缓存 QBVH/材质；one-shot render_scene_cpu 保留兼容入口。mesh-render CPU 一次准备供所有 camera 复用；每个 Rayon task 复用 hit Vec，coincident dedup 原地压紧，保持排序、移动 anchor、Face-over-Volume 和透明合成语义。GPU 图像直接移动给 writer，消除逐张 clone；全批图片积压与 bounded streaming 尚未解决。原有 mesh render 18 项已通过；新增多尺寸/1、2、8 workers/透明与重合面复用验证进行中。

Crop 后续恢复足够深度时的原始 x/y/z 切片循环，避免通用行任务中的额外索引；release 对照与两个 oracle 全通过（crop-slice-fastpath.log）。完整规模矩阵与独立冷/热验证仍未完成，保留所有中间回退样本。


## 26. GPU 多视图有界输出与目标复用（2026-09-21，进行中）

新增 render_views_to 有序消费 owned image，render_views 保留收集式兼容 API。单次几何上传，跨 view 复用 uniform/color/depth/staging，一张图片保存后才渲染下一张；CLI 不再积压全部 GPU 图片。消费者错误立即停止，保存错误不触发 CPU 回退。GPU auto 中途失败后 CPU 按顺序重写全部 views；strict 模式保留已写出的 views 并返回错误。加入维度、staging/vertex 容量检查及 scoped/readback 错误传播。

先前 prepared CPU 19 项通过。新增流式 vs batch 对照、顺序、消费者失败只交付一次、失败后再次渲染；最终检查进行中。PNG 编码与下一视图渲染的有界重叠仍待完成，没有把顺序流式视为整个 PERF-17 完成。

§26 验证完成：GPU feature mesh render 20 项、默认构建 mesh render 回归以及新增超限尺寸专项通过；日志 scene-stream-checked.log、scene-stream-default.log、scene-stream-limits.log。使用软件适配器，未据此宣称物理 GPU 性能验收。消费者失败前 staging 已 unmap，专项验证失败后同一 pipeline 仍可复用。


## 27. Split-filter 指标缓存、配置池与有界写出（2026-09-21，进行中）

新增 cpu_max，全流程进入配置池；ParticleMetrics 缓存原顺序 volume/aspect/area，>=32 分量 indexed parallel collect，否则串行。未配置的指标不算，aspect/volume 已拒绝的分量跳过 area。过滤和 lognormal RNG/删除顺序保留串行；before volume 不再 clone。STL 最多两个 writer，文件名预分配、错误按输出 rank 消费。

发现原 seeded 回归配置 mode=lognormal，实际只接受 lognormal_rebalance，旧测试没有触发重平衡。已纠正模式，增加确实删除颗粒的断言，并比较 1/8 workers 的文件名及完整 STL 字节。另加 1/31/32/67 分量、1/2/8 workers 的原始串行指标 oracle。测试正在执行；分量发现 CSR、性能矩阵和单巨大分量验收仍未完成。

§27 功能验证：split-filter smoke、组件指标 oracle、真实 seeded rebalance 的默认/GPU feature 回归通过；40 个输入保留 31、删除 9，1/8 workers 输出文件名与完整 STL 字节一致。release 指标性能对照仍在运行，尚不声明端到端提速。

§27 release 指标基准完成（1 warmup+5，逐项 exact 对照，原始样本/source+binary SHA: split-benchmark-raw.json）：many_small/1: 0.0007964→0.0008350s；many_small/2: 0.0007919→0.0004495s；many_small/8: 0.0007920→0.0003626s；one_large/1: 0.0006074→0.0005961s；one_large/2: 0.0005928→0.0005931s；one_large/8: 0.0005994→0.0005957s。仅证明指标阶段，不代表分量发现、I/O 或端到端收益；单个大分量仍串行，不改变内部求和顺序。


## 28. 单个高密度桶的去重退化（2026-09-21，进行中）

原 adaptive dedup 在整个桶结束后才切换 hash，因此第一个高密度桶仍执行 O(k²) contains。现改为遇到第 256 个不同候选立即切换，hash 仅作 membership，不迭代，保持首遇顺序、稀疏 ID、排除与重复插入语义。新增单桶 4096 候选+重复插入/复用 scratch oracle，32/256/4096/16384 候选 release 对照；现有查询/增量更新 oracle 与 benchmarks 同时执行中。

§28 release 空间网格测试与性能对照完成，原始日志 spatial-dense.log，样本与 source/binary SHA: spatial-dense-raw.json。单桶中位数（排除 warmup）：32: 0.000000700→0.000000700s；256: 0.000006000→0.000009700s；4096: 0.001337900→0.000184500s；16384: 0.021852404→0.000754500s。这是单桶查询微基准，不代表完整 pack/optimize 的端到端提速。


## 29. Crop 固定分块统计与边界扫描（2026-09-21，进行中）

foreground_blocks 按固定 65536 体素分块，保持块内源顺序、块间按编号合并。centroid/centered covariance/projected bounds 三遍公式保留，worker 数不影响划分与归约顺序。保留原串行 PCA 作 test-only oracle：非退化斜向样本方向/边界容差与 nearest 输出一致；对称体/线/面/单点在 1/2/8 workers 下完全一致。近重复特征空间的规范基与在线 count/mean/M2 实验仍待完成。

背景改为仅遍历边界面，边棱角及退化维度只计一次；并列众数固定选择最小整数，原哈希顺序相关行为不再保留。边界 oracle、CLI/GPU 回归及 release PCA 基准进行中。原串行求和与固定分块求和的浮点分组不同，不宣称所有输入与旧输出逐位相同。

§29 初始约 205k 体素 PCA 基准（pca-benchmark.log）出现调度回退：1 workers: 0.006265→0.007889s /10 repeats；2 workers: 0.005730→0.005284s /10 repeats；8 workers: 0.004934→0.008265s /10 repeats。当前小于 1,048,576 体素或单 worker 时串行计算同一固定分块，大体积并行；所有线程数保持相同分块与合并顺序。已补实际跨过并行阈值的 oracle，加入放大体积对照，复测中。原始失败/退化样本保留。

§29 验证：固定分块/边界 oracle、GPU crop 专项与 crop smoke 通过。隔离 release 基准 pca-isolated.log / pca-isolated-raw.json 包含 binary 与 PCA kernel SHA、1 warmup+5 样本、每样本 10 次：large/1 workers: 0.039838→0.041711s /10 repeats；large/2 workers: 0.038251→0.022764s /10 repeats；large/8 workers: 0.039814→0.040022s /10 repeats；small/1 workers: 0.004520→0.004874s /10 repeats；small/2 workers: 0.005519→0.005120s /10 repeats；small/8 workers: 0.004902→0.005806s /10 repeats。小体积剩余开销和完整 end-to-end/内存矩阵仍需后续评估，不关闭 PERF-14。

§29 调度继续修正：并行分支按 min(pool workers,ceil(N/1048576)) 推导 blocks_per_task，用 with_min_len 限制 Rayon 细分，不创建新池，也不改变每个 65536 体素块的求和或 merge 顺序。release 完整 crop oracle（排除旧 tile benchmark）与 PCA 对照进行中。

§29 工作量限制版本的 release crop 检查 5 项通过；large/8 workers 中位数串行 0.042679s、当前 0.030625s（每样本 10 次）。完整原始样本/source/binary SHA: pca-task-budget-raw.json。大体积单 worker 和小体积仍有开销，不能据此关闭小数据无回退门槛。正在执行默认/GPU 的全库和非 Mesh Gen 集成回归，目标列表 regression-targets.json。

§29 扩展回归完成：default: 24 suites / 315 passed / 11 ignored；gpu: 24 suites / 340 passed / 11 ignored，无失败。测试目标清单 regression-targets.json，日志 broad-default.log / broad-gpu.log，汇总 broad-summary.json。包括全库及非 Mesh Gen 集成测试；ignored 为显式忽略项，不计为通过。硬件 GPU 缺失与完整 Plan 尚未实现项继续保持开放。


## 30. Voxel/shell GPU 设备选择统一（2026-09-21，进行中）

GpuVoxelPipeline 与 GpuShellS2Pipeline 通过公共 request_adapter_device 创建设备，保持默认 features/limits，但遵循 RUSTMSPT_GPU_DEVICE 的名称/编号选择。独立构造器不再绕过用户选择。新增子进程回归覆盖不存在的名称及越界编号，要求错误携带指定 selector，不接受默认设备替代；现有边界/命中溢出与 measure 回归进行中。

同时纠正 context 注释中的“后续调用已缓存”和“默认低功耗”旧描述：当前仍每次探测并申请设备，公共 selector 不等于设备/编译管线缓存；PERF-03 仍未完成。

§30 扩展为统一 selector：MC 构造器也使用 request_adapter_device，try_init_gpu 和 device factory 共用 request_adapter，移除三份名称/编号/default 选择逻辑。GPU features/limits 与独立 device 生命周期保持原契约。子进程反例同时覆盖 MC/voxel/shell 及 capability probe；边界/measure/optimize 回归进行中。


§30 统一 selector 回归完成：GPU boundaries 6、measure 2、optimize CLI 3 全通过，gpu-common-selector.log。

## 31. GPU Instance 复用与 EGL 初始化竞争（2026-09-22，进行中）

wgpu 24 源 API 明确要求每个 Adapter 只用于一次逻辑设备请求，因此没有保留“缓存 Adapter 后反复 request_device”的方案。当前 OnceLock 复用 backend Instance，每次逻辑设备请求仍选择新 Adapter。初版全局 Instance 并发回归暴露 EGL BadAccess（gpu-instance-reuse.log，3 项失败），定位为共享 EGL 上下文枚举/析构竞争。

修复为串行化共享 Instance 选择与未选适配器析构；如果选中 GL，则改用独立 Instance 的适配器，隔离后续设备操作。错误未 memoize，名称/编号筛选保持原契约。修复后 boundaries 6、measure 2、optimize CLI 3 通过（gpu-instance-serialized.log）。补充 Instance 并发身份、warm invalid selector 及 GL 多设备初始化 oracle 正在执行。Device/Queue/编译管线共享未实现，不将此批当 PERF-03 完成。

§31 context oracle 两项通过，无 skip：并发调用共享同一 Instance、warm 选择失败不缓存、可用 GL backend 的三个并发独立逻辑设备初始化。日志 gpu-instance-final-oracles.log。release 适配器选择微基准正在构建；明确不包含 device 或 shader 编译时间。

§31 release 微基准完成：llvmpipe Vulkan，1 warmup+5 样本，每样本 5 次选择，fresh Instance 中位 0.234489505s、复用 Instance 0.002542700s；样本、binary/source SHA 见 gpu-instance-raw.json。排除首次冷发现、逻辑 device 和 shader 编译，不宣称整条管线同倍率加速。

§31 收束：实例复用后的 crop/render/mesh-render GPU 回归 35 项通过，无 skip；context 并发/GL 两项通过；S2/measure/optimize 11 项通过。clippy --features gpu 成功（49 条库告警，包含既有及当前代码告警，未声称零告警），日志 final-clippy.log。差异空白检查通过。完整设备/队列共享、compiled pipeline 缓存及 Plan 其他开放项继续推进。


## 32. MC 按需输出与 staging 复用（2026-09-22，进行中）

取消构造时两个 MAX_RADII*40000 输出缓冲（约 39.1 MiB 逻辑容量），改为输出/回读各两个 4-byte placeholders。ensure_output_capacity 同步增长四个 buffers，后续等大/更小求值保持句柄；read_u32_prefix 只映射和复制本次有效范围，校验长度与映射完成，随后 unmap。release_output_capacity 可主动释放输出/回读峰值，保留几何和编译 pipeline。

measure 的 fresh MC 预算改为 16*invocations+triangle_bytes+576；没有宣称 optimizer 的显式预算已实现。buffer identity/grow/shrink-request/empty→solid→empty 数据更新/释放后重新求值 oracle 已通过，S2/measure/optimize 11 项通过。新增 1 MiB 严格 GPU MC CLI 验收进行中。物理内存/整条管线提速仍需另外测量，逻辑容量变化不当作 RSS 测量。

§32 1 MiB 严格 GPU MC CLI 测试通过，确认实际 backend=GPU、没有 CPU 回退；同批 measure 3 项均通过（mc-lazy-budget.log）。释放/重新增长和映射前缀 oracle 通过（mc-staging-final-oracle.log）。本批未改动 Mesh Gen 源码。


## 33. Voxel/volume-transform staging 复用（2026-09-22，进行中）

体素化和体积变换改为保留 occupancy/output 配套 staging，容量只在需要时增长，读取本次有效前缀；初始四字节占位。分别提供 release_grid_capacity / release_output_capacity 显式释放输出和回读峰值，保留输入与编译 pipeline。Voxel oracle 已通过：句柄复用、较小网格、pitch 改变、增长、释放后重算与解析占据一致。Transform 新增正负 i32 极值、短输入/大输出的背景填充、较小前缀、增长和释放回归，集成检查进行中。源缓冲容量释放与完整预算策略仍待完成，不把输出 staging 复用当作全部 GPU 资源管理完成。


§33 回归完成：voxel 与 transform 复用 oracle 各 1 项通过；crop 2、S2 boundaries 6、measure 3 共 11 项集成通过，见 staging-integration.log。这里仅验证容量身份与结果正确性，未把分配减少换算为未经测量的耗时或 RSS 改善。

## 34. Shell S2 批次缓冲复用（2026-09-22，进行中）

offset/output/readback 初始仅一个偏移的容量，按批次增长并复用，读回仅本批有效前缀；release_batch_capacity 主动释放批次峰值，保留 occupancy 和编译 pipeline。200000 偏移的批次上限和逐偏移等权归约不变。增加跨批尾部、短前缀、占据更新、域外偏移、释放后重算 oracle。完整工作集预算和 GPU 驻留 occupancy 共享仍未完成。

§34 验收：shell staging oracle 1 项通过；S2 boundaries 6、measure 3 共 9 项集成通过，无 skip。日志 shell-staging-oracle.log 与 shell-staging-integration.log。测试覆盖 200001 个偏移的尾批复用；已有 CPU 穷举对照继续覆盖不同偏移的等权归约。完整缓冲 read_u32 仅保留于测试，生产均使用有效前缀读回。


## 35. 独立 mesh-render CPU 执行预算（2026-09-22，进行中）

新增顶层 cpu_max，缺省/-1 使用可用 CPU，其余夹取 1..available，兼容字符串整数解析。整次 VTU 加载、场景准备、CPU 多视图、GPU 调用及失败后的 CPU 回退安装同一 Rayon 池。日志记录池内实际 worker 数与 CPU 渲染入口的 worker index，GPU opaque preview / CPU transparency 契约不变。嵌套任务池 oracle 和 1/2/8 workers 的透明多视图 CLI、auto 设备筛选失败回退、strict GPU 拒绝矩阵正在验收。该改动不代表 GPU 工作集预算或 PNG 编码重叠已经实现。

§35 同时补齐 mesh-render 的 RUSTMSPT_ACCELERATION 优先级：复用 configured_mode，在加载输入前解析，非法值明确报错；生效 gpu 仍严格、auto 可回退、cpu 跳过 GPU。新增独立子进程四种环境覆盖反例。旧 preview auto 的工作量阈值未改动。

§35 结果：嵌套池 oracle 在默认/GPU 构建各 1 项通过，观察 1/2/8 workers 及缺省/夹取。最终独立 mesh-render 集成默认构建 16、GPU 构建 22 项通过（mesh-render-policy-default.log / mesh-render-policy-gpu.log），含环境覆盖、严格失败、auto 回退、透明多视图逐字节比较。未修改 src/meshgen/。本批为执行预算/契约验收，不宣称耗时改善。


## 36. 独立 GPU scene 工作集规划（2026-09-22，进行中）

新增 SceneRenderMemory 纯 checked planner：可见三角形每面 3×40 字节，启用的 segment 每条 2×32、marker 每个 6×32；一组 color/depth 共 8×pixels，readback 为 align256(4×width)×height，uniform 128。保守计入尚未完成的 queue.write_buffer 几何/常量上传，逻辑 GPU 峰值为 2×geometry+256+targets+readback，多视图共用一组目标，不乘视图数。此预算不包含驱动/编译 pipeline 内部分配与输入场景/PNG 的主存占用，不等于物理 VRAM/RSS 上限。

mesh_render 新增 gpu_memory_limit_mb 和 gpu_min_pixels（缺省 0 保留既有 auto 预览选择）。CPU 不探测 GPU；小于阈值的 auto 在初始化前选择 CPU；显式 GPU 绕过阈值但必须满足预算。预算拒绝时 auto 回退、gpu 报错。GPU API 本身检查真实 device texture/buffer/draw 上限；容量检查移到展开 host 顶点前，展开容量来自相同 planner，上传后立即释放临时顶点向量。构造器也纳入 wgpu validation/OOM/internal error scope。尚未实现图像分块以适应小预算，不将回退当作分块完成。

planner oracle 通过：人工计数字节、行对齐、等于预算/刚超预算、单 buffer 边界、超大逻辑输入及算术溢出，未分配大资源。GPU 集成 24 项通过，含 0 MiB 预算拒绝/回退、阈值优先、显式 GPU 绕过阈值、预算换算溢出、1 MiB 严格 GPU 双视图实际输出。上传后释放临时 host 向量后的最终复验进行中。

§36 最终复验：默认构建 mesh-render 17 项、GPU 构建 24 项通过；后者 --nocapture 日志无 SKIP，包含上传后释放 host 顶点的最终代码，见 scene-memory-default.log 与 scene-memory-final-gpu.log。planner 独立 oracle 1 项通过，差异空白检查通过。仍无物理显卡，未宣称真实显卡吞吐或 VRAM 实测收益。


## 37. Optimize MC 显式 GPU 预算（2026-09-22，进行中）

移除显式预算无条件回退的限制。新增纯 checked MC peak planner：triangle+四个 output/readback+576 参数+pending queue uploads+下一次 mesh/params 上传，增长保守计入旧加新；预算换算溢出明确错误。启动按输入面数/max samples 检查，每次实际求值在原共享 GPU mutex 内按真实 retained capacities 再查，失败仍同 mesh_mc CPU 回退或严格错误。以空网格初始化，取消首次求值前的冗余几何上传。管线记录 pending upload bytes，并在成功回读后清零；构造、重复 update、实际 readback 的状态 oracle 通过。

纯 planner 与实际 GPU 高水位/待上传/释放 oracle 2 项通过（optimize-budget-oracles.log），CLI 0/1 MiB 及既有 S2/measure 回归进行中。预算为已知逻辑 GPU 资源保守上界，不包括驱动内部和主存网格；自动分批/缩容及物理 VRAM 测量仍待完成。

§37 首轮运行中预算测试的“边界完全重合 box 的 VF 必为 1”断言失败（实际既有 volume_fraction_in_bbox 返回约 1/6）；未改生产几何结果。预算测试改用严格域内盒并对照现有 CPU VF 契约，原失败日志保留 optimize-budget-runtime.log。边界重合 VF 异常需另行定位，不能由预算测试绕过后宣称整体正确性完成。measure MC 预算同步采用共享 peak planner，补计待上传几何/参数与容量增长，取代 §32 的不完整估计；exact 预算不在本批修改范围。

§37 验收收束：GPU planner/容量状态 oracle 2、optimize execution oracle 7、运行中预算 oracle 1 全通过；GPU S2 boundaries 6、measure 3、optimize CLI 5 通过，无 SKIP。默认 optimize CLI 4 项通过。measure 同步新 peak planner 后另复验 3 项通过（measure-mc-peak-budget.log）。1 MiB 严格 GPU 优化包含 reference/target、3 islands 与最终复核；0 MiB 在探测前拒绝。已发现的边界 VF 异常和完整预算/分块/真实硬件验收仍保留待办。


## 38. VF 无切割快路径与重合边界修复（2026-09-22，进行中）

§37 的单位盒边界异常定位为 legacy clipper 在没有切掉顶点时仍生成 cap，重复共面表面扰乱体积。平面裁剪改为消费 mesh，按实际面引用顶点及既有 -1e-9 阈值分类：全部在内返回原分配，全部在外返回空，混合保留原算法。bbox 六遍之间移动中间 mesh；particle_volume_in_bbox 对完整包含的粒子直接求原体积，不克隆和裁剪。未引用外部顶点不会误触发切割。

解析 oracle 2 项通过：原点/平移盒、两种绕序、重合边界 VF=1、无切割分配身份、全部在外、外向盒的 1/2 与 1/8 交集、未引用顶点。core/collision/optimize/measure/placement primitives 回归通过。新增 release 微基准与更广 GPU 管线回归正在执行。旧失败日志保留；本补丁没有修复 legacy 部分切割的所有非凸/嵌套 cap 问题，不以此声称整个几何正确性验收完成。

§38 release 微基准：严格位于盒内的 12 面 cube，每样本 10000 次，1 warmup+5 交替顺序样本；旧六遍裁剪中位 0.084290312s，快路径 0.000491100s。原始样本、二进制/源码 SHA 与基线 commit 见 clip-contained-raw.json。排除 split/S2/I/O，不宣称端到端同倍率加速。默认五组回归共 45 项通过。

§38 最终 GPU 回归 25 项通过（boundaries 6、measure 3、optimize 5、pipeline smoke 11）。将 §37 动态预算测试恢复为原边界完全重合单位盒：VF 约为 0.9999999999999999，以 1e-12 解析容差验证后，GPU/CPU 回退结果均与 CPU VF 一致，clip-coincident-optimize-final.log 通过。最初恢复测试使用逐位等于 1.0 导致舍入断言失败的日志也保留；先前约 1/6 的实质错误已消除。两个几何 oracle、45 项默认回归、25 项 GPU 回归、动态预算重合边界 oracle 与 release 微基准均通过。未改动 Mesh Gen 源码。


## 39. SA 不可变迁移快照（2026-09-22，进行中）

每岛 best 与 IslandResult 使用 Arc<GlobalBest>，保证几何/loss/S2 同对象。新最佳在锁外准备；共享槽 Arc<Mutex<Arc<GlobalBest>>> 在锁内仅比较 loss 与替换/克隆 Arc；旧快照最后释放也在锁外。接收后 prepared/grid/merged/S2 重建仍锁外，严格比较和平局行为不变。最终选优借用快照，不为返回 IslandResult 再复制几何。

三项默认 oracle 已通过：1/2/8 worker 实际迁移一致性、merged 更新/回滚、16 并发发布与 Arc/payload 指针身份及旧快照不可变性。微基准将记录固定快照 payload 字节与旧锁内深拷贝/Arc publication 的 release 样本，明确排除新最佳的初始准备和接收端可变状态重建，不把迁移复制消除等同于整个 SA 零复制。

§39 验收收束：默认 optimize oracle 3、默认 CLI 4、GPU CLI 5 全通过。固定球体快照的深拷贝 payload 为 738400 字节/发布，Arc 交换由身份 oracle 确認 payload 不复制。首轮与编译可能重叠，保留原日志并在编译结束后隔离复测：每样本 1000 次，1 warmup+5 样本，旧复制中位 0.117010400s、Arc 交换 0.000053000s，原样本/SHA 见 optimize-arc-raw.json。仅衡量已准备快照的发布，排除新最佳创建、最后持有者 payload 释放、接收端准备、S2 和 SA 总耗时，不外推端到端倍率。


## 40. CPU 渲染像素任务（2026-09-22，进行中）

STL nearest-hit 与 scene all-hits 改用连续像素块：单 worker 单块；其余初版约每 worker 四块，目标夹取 256..4096 像素，能容纳整行则按行对齐，宽行可拆开。chunk 起点一次除法获得像素坐标，随后逐像素递增；保持同一 ray_for_pixel、命中和合成运算、overlay 顺序。scene 的 RGBA/depth 同边界，沿用 task-local hits。

两个 oracle 通过：两种投影、1/2/8 workers、1×1、37×29、65537×1、1×65537、8193×3，与强制按行的相同像素内核逐字节对照；包含透明、Face-over-Volume 和线条深度。默认 STL/scene 集成通过。release 粒度基准正在测量 1/2/8 workers × 宽/窄/方形/小图，每项 1 warmup+5 样本，保留全部退化结果后再选择粒度。

§40 首轮结果：nearest-hit 单行图 2/8 workers 分别约 1.94/5.04× 局部加速；透明 scene 单行图约 1.91/4.44×，不透明约 1.93/4.38×。但 32×32 透明 scene 在 8 workers 从 0.000353s 退化至 0.000893s（每样本 3 张图），原始样本保留 render-pixel-tile-raw.json / scene-pixel-tile-raw.json。修订为 <=1024 像素维持原按行任务，较大图沿用像素块；正在串行运行最终两个 release benchmark，避免测量互相竞争。

§40 将小图回退条件推广为按 worker 预算判断：height>=workers 且 pixels<=1024×workers 时保留行任务，避免只修补某个固定尺寸。增补 37×29、64×64、65×65 的透明/不透明基准；先前固定 1024 方案日志仍保留 pixel-tile-final-benchmarks.log。

§40 最终 worker-aware 基准见 pixel-tile-worker-budget-raw.json（48 个 case，逐项保存全部样本及源码/二进制 SHA）。单行 nearest-hit 在 2/8 workers 约 1.88/4.83×；透明 scene 约 1.87/3.82×，不透明约 1.90/4.20×。窄图 2-worker nearest-hit 有约 4% 的单次中位退化。8-worker 小图即使两边已使用完全相同 row grain，仍出现 0.72～2.58 的比值波动，不能据此宣称收益或消除所有性能风险；增加 grain identity oracle 明确这几项执行同一分块路径。最终两项 release 像素一致性 oracle 通过；GPU 功能回归进行中。

§40 收束：GPU 构建渲染回归 38 项通过（mesh-render 24、render policy 1、STL render 13），无跳过日志；默认渲染回归 29 项通过。最终 release 像素一致性 2 项与 small-row grain identity 1 项通过。单行图的局部收益已实测，窄图与小图波动/退化数据保留；不将像素内核实验外推为文件到 PNG 端到端收益。差异空白检查通过，Mesh Gen 源码未改动。


## 41. GPU MC 工作组归约（2026-09-22，进行中）

每 256-lane workgroup 独立对应 radius/sample-block，保留原逻辑 RNG id，不对填充 lane 求值或重抽样。组内共享 u32 归约 hit/valid，无全局原子；CPU u64 合并部分和。回读实际字节为 8Rceil(max(S,200)/256)，输出/staging 按同一部分和容量保留和增长；预算 planner 同步更新。dispatch guard 改为检查补齐后的组数，并保留逻辑样本编号乘法溢出检查。

固定 seed 新旧 GPU 整数计数 oracle 已通过：0/200/255/256/257/511/512/513/1000 samples，1/4/128 radii，两个 seed，几何更新/空几何；使用冻结的旧 per-sample shader 比较 totals，不只比较比例。既有 5 项 GPU MC 单元测试通过。运行中预算测试改用真实大面数球体触发几何预算（归约后 70000 样本输出已不再超过 1 MiB），保留其阶段错误/回退/恢复约束。GPU/预算回归及 release 字节/CPU 合并/求值耗时基准进行中。BVH、GPU 第二层归约和预算分批仍未完成。

§41 首轮软件 GPU 基准已保存 mc-reduction-raw.json：4 radii×4000 samples 回读由 128000 降为 512 字节，CPU 合并中位由约 10μs 降至约 2μs（每样本 3 次求值）。12 面 case 总耗时从 0.0045583s 退化至 0.0053906s；80 面 case 中位略改善但样本波动明显。不宣称工作组归约在 llvmpipe 或真实 GPU 上普遍提速。增加 40000 samples 基准，并将 1 MiB strict optimize fixture 提升至 40000 samples 验证资源减少确实允许执行原先超预算的工作量。

§41 扩展基准：4 radii×40000 samples 回读从 1280000 到 5024 字节（约 255× 降低）；每 3 次求值 CPU 合并从 132～150μs 到约 2.9μs。llvmpipe 总求值中位在 12 面 case 从 0.0469442s 到 0.0477791s，在 80 面从 0.150135599s 到 0.161020999s，保留退化，不据此宣称整体加速。全部样本及 source/binary SHA 在 mc-reduction-expanded-raw.json。1 MiB strict GPU optimize、每半径 40000 samples 的实际 CLI 通过（mc-reduction-large-budget-cli.log）；原逐样本输出估计会超过该预算。固定 seed 整数 oracle 1、GPU MC 单元 5、budget 相关 oracle 4、S2/measure/optimize 集成 14 项通过，无跳过日志。最后注释更新后的 GPU 编译/原始计数复验进行中。

§41 最终代码 GPU 固定-seed 原始计数 oracle 与默认构建内存 planner oracle 均通过（mc-reduction-final-count-oracle.log / mc-reduction-default-planner.log）。差异空白检查通过；未修改 Mesh Gen 源码。此批完成第一层工作组计数归约与 CPU 部分和合并，PERF-08 的 BVH/分批/真实硬件性能验收仍开放。


## 42. GPU MC 二维调度与启动上限（2026-09-22）

MC dispatch_plan 复用 checked grid_plan，返回样本数、部分和槽位及二维 dispatch。shader 以 group.x + group.y×num_workgroups.x 展平块编号，在所有 barrier/写出前统一排除填充组；随机样本编号和整数归约定义不变。Optimize 启动阶段移除旧 65535×256 单维调用上限，保留逻辑 u32 乘法边界，实际设备容量由执行 planner 检查。

验证目录仍为 data/output/performance/20260921-crop-render/。mc-2d-spare-capacity-oracle.log：1 项通过，无设备跳过；强制 3/4 维度限制、跨行与尾块、两个 seed 的原始计数和冻结逐样本 shader 相等，保留第九槽哨兵证明八个有效组的 3×3 调度不会污染额外容量。65,536 个工作组只执行纯 planner 检查，未实际分配或执行该大任务。mc-2d-startup-policy.log：1 项通过，证明超出旧上限的合法任务到达 backend probe。mc-2d-integration.log：GPU boundaries 6、measure 3、optimize 5 项通过。

本批消除单维 dispatch 人为限制，不等于预算驱动分批；逻辑 u32 总样本边界、128 半径上限及设备缓冲限制仍存在。预算分批、BVH、GPU 常驻数据流及物理硬件性能验收继续开放，不关闭 PERF-04/08 整包。


§42 最终 GPU MC 单元回归 mc-2d-final-unit.log：7 项通过，1 项 release benchmark 按默认测试配置忽略；该 benchmark 的真实运行记录见 §41。没有设备缺失跳过。

## 43. GPU exact 合法配对数解析计算（2026-09-22，进行中）

PERF-09 合法配对计数从逐体素 valid++ 改为三个轴的重叠长度乘积；越界位移仍先拒绝。主机 grid_plan 已证明 nx×ny×nz 不超过 u32，重叠子体积乘积随之有界。hit 遍历、占据定义、逐偏移等权平均及无支持半径行为不变。

新增独立原始整数 oracle，对 1×1×1、1×3×7、4×3×2 网格全部边界内外偏移穷举 signed 坐标，覆盖空/满/混合占据及 i32 极值。测试记录 shell-analytic-counts.log 与集成记录 shell-analytic-integration.log 位于既有 20260921-crop-render 目录。原始计数 oracle 1 项、GPU 边界 6 项、measure 3 项均通过，无设备跳过；尚未测量本项耗时，不宣称 shader 编译器一定产生更快的代码。offset×voxel 分解、共享设备与 GPU 常驻占据仍待完成。

§43 release 对照已完成：shell-valid-benchmark.log / shell-valid-raw.json 保留全部 30 个交替样本、source/fixture/binary SHA。每组各预热一次，计时包含占据/offset 上传、kernel、回读和 CPU 汇总，排除设备及 shader 初始化；所有曲线逐值相等。当前无 /dev/dri，不代表物理 GPU 性能。中位耗时（旧→新）：

- 8³ / 26 offsets：0.000419100 → 0.000406200 s。
- 8³ / 124 offsets：0.000485300 → 0.000505600 s。
- 32³ / 26 offsets：0.003384600 → 0.002915200 s。
- 32³ / 124 offsets：0.011964999 → 0.010973599 s。
- 64³ / 26 offsets：0.022732999 → 0.020015898 s。
- 64³ / 124 offsets：0.096221493 → 0.086213595 s。

8³/124 offsets 出现小幅变慢，保留该结果；不能推广为所有规模都提速。最终 shell 单元测试 2 项通过（shell-valid-final-units.log），默认忽略的 release benchmark 已由上述独立命令实际运行通过。


## 44. GPU shell 工作组协作实验（2026-09-22，进行中）

新增 cooperative shader：每个 offset 一个 256-lane 工作组，每 lane 跨步遍历重叠体素，然后共享内存整数归约；每个 offset 仍只回读一对完整 hit/valid，保留等权平均。统一分支保护越界 offset 和填充工作组，u32 步进在加法前检查剩余长度。主机 shader 构造配套保存 offsets_per_workgroup，由同一 checked planner 展开二维调度。

生产入口仍采用 direct analytic shader，实验通过私有构造器及显式 benchmark 运行。原始计数 oracle 扩展到两个 shader 和 9×7×5 网格，shell-cooperative-counts.log 已通过。新增跨设备 X 上限和保留尾槽的测试，release 对照 shell-cooperative-benchmark.log 与 direct analytic shader 比较（与 §43 的旧逐项 valid++ 基线不同），结果待记录。不以这一步替代 PERF-09 的独立 voxel tile、GPU 驻留及预算驱动分批。

§44 首轮实验完成：shell-cooperative-dispatch.log 跨行/尾槽 oracle 1 项通过；shell-cooperative-benchmark.log 独立 release benchmark 1 项通过，无设备跳过。shell-cooperative-raw.json 保留 30 个原始样本与两份 shader/实际执行 binary SHA（构建期间只追加了独立 dispatch 测试，因此不把事后 Rust 全文件 SHA 当作构建输入）。均为软件 GPU，非物理硬件验收。中位耗时（direct analytic→cooperative analytic）：

- 8³ / 26 offsets：0.000496501 → 0.000575900 s。
- 8³ / 124 offsets：0.000618900 → 0.000629500 s。
- 32³ / 26 offsets：0.002828900 → 0.001759200 s。
- 32³ / 124 offsets：0.010994600 → 0.007080200 s。
- 64³ / 26 offsets：0.020923200 → 0.017830701 s。
- 64³ / 124 offsets：0.087472803 → 0.066515003 s。

小任务 8³ 回归约 2%～16%，32³/64³ 加速约 1.17～1.61 倍。生产切换阈值仍需细长网格、更多偏移及临界规模证据，暂不无条件替换 direct 路径。独立 tile 分解和设备常驻数据仍未完成。

§44 扩展矩阵完成：shell-cooperative-shapes.log / shell-cooperative-shapes-raw.json，12 种形状×3 种偏移数×5 个交替样本，共 180 项原始时间对，所有曲线精确相等。SHA 对应本轮未再改动的 Rust/shader 和实际二进制。

发现第一轮小偏移集不能支持仅按体素数切换：728 offsets 的立方体协作版本比 direct 慢约 1.65～3.35 倍；同为 32768 体素，1×1×32768 会变慢，而 32768×1×1 和 128×256×1 则可显著加速。原因尚未以 kernel profile 证明，不能归因于某个单独硬件因素。生产保留 direct；下一步需要独立 voxel tile 的布局/任务分解及更完整成本选择，不能用“总网格 >= 某阈值”掩盖方向和偏移数回归。


## 45. GPU shell 独立体素块（2026-09-22，进行中）

新增 s2_shell_tiled.wgsl，workgroup 对应（offset，voxel tile），每块整数归约 hit 并输出解析 valid。主机按 offset 汇总全部块的 u64 计数后再作等权比例平均，不平均 tile 比例。尾块和空块显式保护，跨步加法先检查剩余长度，索引受完整网格 u32 乘积约束。每批限制 200000 部分结果槽；tile 增多时缩小 offset 批数，单 offset 超过该槽数报错。参数缓冲扩为 24 字节，direct/cooperative 旧 shader 仍使用前 16 字节。

生产继续选择 direct，tiled 通过实验入口运行。64/256/4096 块大小与独立原始计数穷举已通过（shell-tiled-counts.log）。新增 40003 个偏移、跨批短尾、无效偏移、多个半径和 64/257/4096 块大小的曲线对照。最终单元/集成与 4096 块大小的 release 形状矩阵进行中，结果记录于 shell-tiled-units.log / shell-tiled-integration.log / shell-tiled-benchmark.log。固定部分结果上限不是完整用户预算；设备常驻、最终 GPU 归约、生产选择及性能验收仍开放。

§45 验证完成：shell-tiled-units.log 4 项通过（3 项 benchmark 默认忽略）；shell-tiled-integration.log GPU boundary 6 与 measure 3 项通过。独立 release tiled benchmark 实际运行 1 项通过，全部 180 个时间对、回读字节和 source/binary SHA 保存于 shell-tiled-raw.json；无设备跳过。

4096 tile 不保证提速：64³/728 offsets 的部分结果回读为 372736 字节（direct 为 5824），调度 46592 个工作组；实际中位时间如下。生产不据此无条件切换；下一步应减少空 tile/中间输出并验证设备端最终归约，仍须保留完整整数计数与等权定义。

- (1, 1, 32768) / 26 offsets：direct 0.000821300 s → tiled 0.001144700 s。
- (1, 1, 32768) / 124 offsets：direct 0.000787500 s → tiled 0.001721900 s。
- (1, 1, 32768) / 728 offsets：direct 0.001170800 s → tiled 0.004941601 s。
- (32, 32, 32) / 26 offsets：direct 0.002893300 s → tiled 0.002881701 s。
- (32, 32, 32) / 124 offsets：direct 0.011176001 s → tiled 0.011508801 s。
- (32, 32, 32) / 728 offsets：direct 0.021013602 s → tiled 0.047072105 s。
- (64, 64, 64) / 26 offsets：direct 0.023609103 s → tiled 0.024205902 s。
- (64, 64, 64) / 124 offsets：direct 0.110856810 s → tiled 0.119795711 s。
- (64, 64, 64) / 728 offsets：direct 0.179895117 s → tiled 0.447883042 s。


## 46. GPU shell 设备端 tile 归约（2026-09-22，进行中）

实验 tiled 路径新增第二次 dispatch，GPU 汇总同一 offset 的全部整数部分计数，只回读每个 offset 的一对结果。完整网格计数已受 u32 限制，故每偏移合并不溢出；CPU 仍作完整偏移比值的等权平均。归约 pipeline/最终输出延迟创建并按容量复用，release_batch_capacity 缩小最终输出并保留 pipeline。生产 direct 不创建归约资源。

首轮原始 oracle 失败原因是仍按多个 tile 读取和求和最终结果，修正为每偏移一项；保留失败记录 shell-reduced-units.log。最终单元、集成和 release 对照正在执行，分别记录于 shell-reduced-final-units.log、shell-reduced-integration.log、shell-reduced-benchmark.log。新增跨批、变换块大小、释放/重建最终输出的测试。本批只减少回读，未取消中间缓冲或额外调度；仍不默认启用实验路径。

§46 验证完成：最终单元 4 项、GPU boundary 6 与 measure 3 项通过，无设备跳过；单元默认忽略的本批 release benchmark 已独立实际运行通过。shell-reduced-raw.json 保存全部 180 个交替时间对、回读字节、source/binary SHA。64³/728 offsets 回读从 372736 降至 5824 字节，减少 64 倍；但这是相对未归约 tile 路径，direct 本来也是 5824 字节。以下为本轮同时测量的 direct 与 reduced 中位时间，不把跨轮时间差直接当作归约提速证据：

- (1, 1, 32768) / 26 offsets：direct 0.000696000 s → reduced 0.001205100 s。
- (1, 1, 32768) / 124 offsets：direct 0.000775100 s → reduced 0.002101500 s。
- (1, 1, 32768) / 728 offsets：direct 0.001145300 s → reduced 0.004871700 s。
- (32, 32, 32) / 26 offsets：direct 0.002968800 s → reduced 0.003840500 s。
- (32, 32, 32) / 124 offsets：direct 0.011473099 s → reduced 0.012997999 s。
- (32, 32, 32) / 728 offsets：direct 0.023575099 s → reduced 0.059860197 s。
- (64, 64, 64) / 26 offsets：direct 0.020835099 s → reduced 0.022027299 s。
- (64, 64, 64) / 124 offsets：direct 0.087124696 s → reduced 0.083428300 s。
- (64, 64, 64) / 728 offsets：direct 0.204625598 s → reduced 0.549127197 s。

减少回读尚不足以普遍超过 direct。仍需减少空任务、按工作量选择和物理硬件验证，生产维持 direct，不关闭 PERF-09。


## 47. Shell 无支持偏移的有界过滤（2026-09-22，进行中）

生产 direct 路径上传前用 unsigned_abs 过滤无支持位移，包含 isize 极值；按原顺序复用有界主机批次，避免第二份完整 offsets。没有有效偏移时在 occupancy 上传/输出分配前返回原 VF/零曲线。仍在 shader 保留无效位移保护，原始计数 oracle 用未过滤构造器测试。实验 tiled 可单独选择是否过滤，尚未默认启用 tiled，也未移除支持偏移内部的空 tile。

新增跨多个 200000 边界的混合有效/无效偏移与多个半径的完整曲线对照，并检查全无效请求保持 4 字节 occupancy/output 占位。最终单元/集成和 release 基准记录 shell-filter-units.log / shell-filter-integration.log / shell-filter-benchmark.log；基准两边使用同一 direct analytic shader，仅过滤策略不同，包含过滤 CPU 开销。

§47 最终单元 5 项、GPU boundary 6 与 measure 3 项通过；release 过滤 benchmark 独立运行通过，无设备跳过。shell-filter-raw.json 保存全部 180 个时间对、实际支持偏移数及 source/binary SHA。中位耗时（未过滤→过滤）：

- (1, 1, 32768) / 26 offsets（有效 2）：0.000720700 → 0.000681100 s。
- (1, 1, 32768) / 124 offsets（有效 4）：0.000962800 → 0.000814100 s。
- (1, 1, 32768) / 728 offsets（有效 8）：0.001298800 → 0.001301600 s。
- (8, 8, 8) / 26 offsets（有效 26）：0.000587500 → 0.000555000 s。
- (8, 8, 8) / 124 offsets（有效 124）：0.000535200 → 0.000501000 s。
- (8, 8, 8) / 728 offsets（有效 728）：0.000778500 → 0.000646100 s。
- (64, 64, 64) / 26 offsets（有效 26）：0.021465200 → 0.022249000 s。
- (64, 64, 64) / 124 offsets（有效 124）：0.089171998 → 0.089171597 s。
- (64, 64, 64) / 728 offsets（有效 728）：0.188387994 → 0.187386994 s。
- (128, 256, 1) / 26 offsets（有效 8）：0.005336900 → 0.001887500 s。
- (128, 256, 1) / 124 offsets（有效 24）：0.017125000 → 0.005006000 s。
- (128, 256, 1) / 728 offsets（有效 80）：0.020542600 → 0.015187200 s。

有效偏移少的薄网格减少上传/dispatch/回读；全部有效时仍有主机过滤成本，原始负差异完整保留，不宣称普遍提速或物理 GPU 加速。PERF-09 仍待完整预算批次、设备常驻及生产 tile 策略验收。


## 48. Exact 共享设备与占据缓冲（2026-09-22，进行中）

try_calculate_s2_gpu_exact 的 shell 阶段用 voxel 的同一 Device/Queue 构造，取消阶段内第二次设备申请；绑定已完成 voxelization 的 occupancy buffer，取消整个占据场的再次上传及 shell 副本。两阶段串行，各自错误作用域不重叠。CPU 占据场回读仍用于 VF，尚未全常驻；上层 backend probe 不包括在“exact 内部一次设备申请”中。

compute_s2_shell_resident 检查缓冲长度并只借用输入，不替换/污染主机 API 的私有上传缓冲；后续 host 求值仍可正常执行。设备/队列身份、非零占据读取、缓冲长度拒绝、生产者释放后的句柄寿命与 host 求值恢复已加入专项 oracle。首轮 shared-device oracle 通过；最终共享缓冲单元 6 项、GPU boundaries 6 项及 measure 3 项通过，无设备跳过（exact-shared-final-units.log / exact-resident-integration.log）；5 项历史 release benchmark 在本次普通单元运行中按标记忽略。未测量本项耗时，不宣称实测端到端提速。


## 49. Exact 驻留占据场与 VF 计数（2026-09-22，进行中）

新增 voxelize_count 与 voxel_count.wgsl，voxelization 后设备端整数归约，仅回读一个 u32 计算 VF。完整 occupancy 随后由同设备 shell 直接读取，不再为 VF 下载整场，也不再上传到 shell 副本。计数器使用 256-lane 单工作组跨步扫描，已检查网格大小和二值占据保证 u32 不溢出；不把减少传输直接等同于速度提升。

occupancy/staging 独立增长，count-only 新管线保持 4 字节 staging；切回完整 Vec API 时即使 occupancy 已足够大也会正确扩容 staging。计数 pipeline/output 延迟创建并复用，release 保留固定 4 字节计数结果。voxel-count-units.log：5 项通过（包括解析盒体、尾部、空网格材料、增长/缩小、模式切换、释放后重算和错误恢复）；exact-count-integration.log：GPU boundaries 6 与 measure 3 项通过，无设备跳过。release 对照进行中（voxel-count-benchmark.log），包含 voxelization 与 VF 计数整个阶段，排除构造/编译，比较相同占据计数。

§49 release 基准完成：voxel-count-benchmark.log / voxel-count-raw.json 保存 15 个原始时间对及 source/binary SHA。一轮预热、五轮交替，完整 voxelization+VF 计数计时，排除初始化；同一管线切换模式，保留 baseline 建立的完整 staging 高水位，因此此基准不测新建 count-only 的主存/缓冲收益，也不是 exact 端到端计时。

- 16³：完整回读/CPU 求和 0.000605200 s → GPU 计数 0.000746300 s；回读 16384 → 4 字节。
- 64³：完整回读/CPU 求和 0.012245100 s → GPU 计数 0.014072000 s；回读 1048576 → 4 字节。
- 128³：完整回读/CPU 求和 0.068929301 s → GPU 计数 0.068439001 s；回读 8388608 → 4 字节。

软件适配器的总时延差异和负结果如实保留，不宣称物理 GPU 加速；驻留数据流已接入生产，完整 shell GPU 半径归约、预算与最终性能矩阵仍待完成。


## 50. Exact 偏移逐半径生成（2026-09-22，进行中）

exact 不再收集所有半径的 all_offsets；逐半径生成 Vec 并通过泛型 iterator 供 shell 有界批次消费，可跨半径边界且保留输入顺序。在同一次枚举记录 has_support，取消为插值进行的第二遍 shell_offsets_for_distance。成功时无效尾部也完整消费；错误仍传播，不能使用未完成曲线。诊断总数用 u128，避免静默饱和。

内存为单半径 shell 加有界批次，单半径本身仍可能很大，并未声称常量主存或取消最终完整预算工作。exact-stream-integration.log 中 GPU boundary 6 与 measure 3 项通过，exact-stream-oracle.log 专项 1 项通过，均无设备跳过；专项生成超过 800000 个混合偏移但不物化完整流，跨多批、无效尾部和全无效流均检查消费数量与解析曲线。未测量本项耗时/RSS，不把结构性减少枚举直接当实测加速。


## 51. 单半径偏移的惰性枚举（2026-09-23）

新增 shell_offset_iter，GPU exact 不再物化单半径 Vec；只保留嵌套范围游标，加现有有界批次。原公共 Vec API 保留，继续服务需要随机采样/索引的调用者。GPU 迭代器保持原 x/y/z 顺序、近零原点分支及 [low²,high²) 判断；普通整数范数保持原运算，大范数用 u128 防溢出。has_support 用 peekable 判定，数量在实际消费时用 u128 计数。仍扫描立方体，复杂度未降为表面积级。

shell-lazy-oracle.log 1 项通过：多半径/半宽、空壳、近零、相邻 f64 边界及部分消费后续接与原 Vec 逐项一致。exact-lazy-integration.log 中 GPU boundaries 6、measure 3 项通过，无设备跳过。shell-lazy-benchmark.log / shell-lazy-raw.json 为独立 CPU release 对照，一轮预热+五轮交替，计时含枚举和顺序敏感 checksum；保存 source/binary SHA。以下容量是旧 Vec payload capacity，不是 RSS；新 iterator 不分配此列表，shell 批次内存另计：

- 半径 16：0.000088100 → 0.000076800 s，避免旧列表容量 98304 字节。
- 半径 64：0.003641300 → 0.002181700 s，避免旧列表容量 1572864 字节。
- 半径 128：0.025725300 → 0.014903700 s，避免旧列表容量 6291456 字节。

该基准不包含 GPU 计数、偏移上传或 exact 端到端开销；完整性能矩阵与预算闭环仍待完成。


## 52. 驻留 exact 工作集预算与分批（2026-09-23，进行中）

新增 compute/exact_memory.rs，Measure 后端预估与 try_calculate_s2_gpu_exact_limited 执行共用 planner。T=max(36×faces,4)，M=4×cells，逻辑峰值保守上界 2T+M+128+80B：包含几何存储/待上传、驻留占据、固定参数/计数/占位以及批次新旧输出/staging/offset 与上传；B 是偏移部分结果槽数。配置预算下从 200000 缩批，至少一个；最小批次仍不满足则初始化前报错，调用方按既有 cpu_fallback 处理。设备单 buffer/dispatch 检查保留独立执行。

仅适用新建生产 direct exact；实验 tile/第二层 reducer 或任意已有高水位管线不在该 planner 的证明范围。驱动内部、shader 编译资源和主机内存不算逻辑 GPU 预算。shell setter 拒绝零/超上限/低于已有结果容量的限制。执行日志报告 batch_partials 与 estimated_peak_bytes。

exact-memory-planner.log 纯 planner 测试 1 项通过，覆盖分批上界、最小批次不满足、溢出和空网格。exact-budget-integration.log 中 GPU boundaries 6、measure 4 项通过，无设备跳过；包括禁止回退的 1 MiB CLI：32³ 实心占据、16 半径、超过 11456 个支持偏移，实际 GPU 执行且全部曲线为 1，确认跨批无漏计；既有预算回退回归也通过。


## 53. 公共选择器预算语义（2026-09-23，进行中）

移除 select_gpu_backend 中“配置总预算 > 单 storage binding 上限就回退”的错误比较及未检查 MiB 乘法。旧 select_backend/select_backend_for_workload 缺少工作集参数，现对带预算 GPU 请求在探测前明确返回 CPU/原因，溢出单独报告；不静默忽略预算。具备估计的管线继续通过 resolve_execution 验证实际任务大小，再以 None 调用只负责设备可用性的旧选择器。CPU 和小 Auto 请求保持先返回。

新增无硬件的旧接口预算/溢出/阈值/严格缺失估计测试，以及实际 GPU 上“预算大于单 binding 上限但任务小于预算”仍可选择 GPU 的回归。默认构建 1 项、GPU 构建 2 项通过，无设备跳过（policy-budget-default.log / policy-budget-gpu.log）；不改 Mesh Gen 源码。


## 54. 全量回归复核（2026-09-23，进行中）

在 §53 后的源码上执行 cargo test 全量默认构建：42 个测试报告合计 592 passed / 0 failed / 16 ignored，进程 exit 0。16 项均为显式 release benchmark 标记，本次不计为通过，具体清单写入 full-default-sep23-summary.json。原始日志 full-default-sep23.log 和完整 Rust/WGSL/Cargo/build/tests source SHA 清单保存在既有 performance/20260921-crop-render 目录。此命令也执行已有 Mesh Gen 回归，但未修改 Mesh Gen 源码。

GPU 全量测试与 lint 的最终结果见本节后续记录，不以默认测试代替 GPU 验收；历史 microbenchmark 结果也不冒充本轮全量端到端性能矩阵。完整 Plan 继续开放。

§54 GPU 全量命令 cargo test --features gpu 完成：42 个报告合计 640 passed / 0 failed / 24 ignored，exit 0；full-gpu-sep23-summary.json 保留清单。两轮源码 SHA 完全一致。默认捕获会隐藏成功测试的输出，因此这些数量本身不能证明每个 GPU 测试都未早退；另用 --show-output 对 GPU lib 与相关集成套件复核，记录 gpu-explicit-output-sep23.log。

两种 clippy 均 exit 0：默认 48 条、GPU 55 条 library warnings（clippy-default-sep23.log / clippy-gpu-sep23.log）。并非零警告；包括既有 Mesh Gen/类型告警及当前 GPU 参数数量、条件编译下未使用成员等，未为清理警告修改 Mesh Gen。

§54 GPU 成功输出复核完成：7 个报告合计 207 passed / 0 failed / 20 ignored；未记录设备不可用早退，细节见 gpu-explicit-output-sep23-summary.json。它是全量套件的重叠子集，不能与 640 相加。默认与 GPU release 构建继续验证。

§54 release 默认/GPU 构建均 exit 0（约 1m11s / 1m13s），二进制分别保留为 rustmspt-default-sep23 / rustmspt-gpu-sep23，版本 JSON 与 binary SHA 见 release-*-sep23-summary.json。源码指纹与两轮全量测试一致；git diff --check 通过，src/meshgen 无修改。此次完成当前源码的广泛功能/构建回归，不是 Plan 全部实施或性能验收完成，物理 GPU 验收条件仍缺失。


## 55. SA 连通分量 VF 贡献缓存（2026-09-23，本轮实施及专项验收完成，完整任务仍见 §5）

仅对连续 mesh MC 岛启用 IslandVolumes，保存每粒子/连通分量的未截断域内体积。候选只重算受影响粒子，拒绝恢复旧条目；迁移重建缓存，每 64 次候选求值全量刷新。总和仍逐次按 merged 的组件顺序累加缓存标量，避免增量总量加减漂移。没有把几何 VF 注入 voxel S2，也不假定域内刚体变换的浮点体积位级不变。prune/final 保留完整参考求值。

CPU/GPU 统一 evaluator 增加带已验证 VF 的内部入口，CPU 相同 seed/radius/block 采样不变，GPU 成功后在锁外写入相同 VF；错误回退保持 mesh MC 并使用相同缓存。sa-vf-final-oracles.log 两项通过：160 步接受/拒绝、跨界、多连通分量、全刷新、移除/迁移重建逐次与完整 merged VF 精确相等；固定 seed 的完整 MC 曲线（含 r=0）一致。sa-vf-gpu-integration.log 和 sa-vf-cache-benchmark.log 正在执行；基准含 64 步刷新成本，只测 VF 更新部分，不代表 SA 端到端收益。

§55 集成回归：optimize GPU feature CLI 5、pipeline smoke 11 项通过；release cache benchmark 实际运行通过，全部 15 个原始样本及 source/binary SHA 见 sa-vf-cache-raw.json。每样本 100 次相同变换及 VF 更新，包含缓存 64 步刷新、两边相同的 merged 顶点更新；排除初始准备、碰撞/S2/接受决策与 SA 端到端。中位时间：

- 1 粒子：全量 0.000202900 s → 缓存 0.000204600 s。
- 32 粒子：全量 0.007245499 s → 缓存 0.000300000 s。
- 512 粒子：全量 0.115075495 s → 缓存 0.001528900 s。

完整回归 §54 对应本批之前源码；本批使用上述专项与管线回归，不冒充已重跑全部套件。GPU 部分上传、局部 pose/BVH 与增量占据仍开放。

## 56. GPU 多视图 PNG 有界重叠（2026-09-23，本轮实施及专项验收完成，完整任务仍见 §5）

独立 mesh-render 使用 consume_frames 将拥有所有权的帧通过零容量通道交给单个 PNG writer，GPU 可在前一帧编码时准备/渲染下一帧。最多同时存在 writer 帧与 producer 帧；无图像 clone、无全批积压。多 worker 且多视图时启用；单 worker/单视图维持顺序，CPU 路径仍在原 Rayon 预算内顺序保存。返回前 join 并排空已接收帧，保证 auto CPU 回退不会与 GPU writer 同时写文件。写出错误优先于 GPU 错误，末帧异步失败也不能被 producer 成功掩盖。

专项测试比较顺序/重叠 PNG 全字节，验证写出顺序、GPU producer 中途失败后的排空、首帧/末帧写错传播，以及通过同步事件证明 producer 和 writer 重叠。当前尚未提供端到端 release 性能对照，PERF-17 不据此整体关闭。

§56 验证完成：frame-writer-units.log 4 项通过，frame-writer-integration.log GPU feature mesh-render 24 项通过；将实际双视图 GPU CLI 检查的 cpu_max 改为 8 后另行执行，frame-writer-cli-test.log 1 项通过、无设备跳过。release GPU 构建通过，二进制保留为 rustmspt-gpu-frame-writer。

与 §54 保存的顺序版本 rustmspt-gpu-sep23 做冷进程端到端对照（均为 llvmpipe、8 worker，good_cube.vtu，包含设备初始化、读取、场景构建、渲染与 PNG 写盘；文件系统缓存未清理）。每个尺寸/视图数一轮预热、5 轮交替次序样本，共 72 个进程；所有同场景 PNG SHA 完全一致。脚本 benchmark_frame_writer.py、各配置/stdout/stderr/RSS、原始数据及 binary/source/fixture SHA 存于 frame-writer-cli-raw.json 与 frame-writer-cli/。中位结果：

| 尺寸 | 视图数 | 顺序秒 | 重叠秒 | 顺序峰值 RSS KiB | 重叠峰值 RSS KiB |
|---|---:|---:|---:|---:|---:|
| 64² | 2 | 0.295518 | 0.282114 | 210324 | 211964 |
| 64² | 8 | 0.265630 | 0.251289 | 209340 | 211600 |
| 512² | 2 | 0.270913 | 0.259619 | 212888 | 215008 |
| 512² | 8 | 0.273828 | 0.277465 | 212992 | 215228 |
| 1024² | 2 | 0.309436 | 0.271112 | 225288 | 227208 |
| 1024² | 8 | 0.381428 | 0.350063 | 225156 | 231548 |

软件 GPU 上存在小幅退化样本，不能宣称所有尺寸提速；初始化占比高，不将这些结果归因于纯 kernel 加速。额外 host 图像/编码线程提高 RSS，逻辑 GPU 预算不包含这些 host 分配。该项有界重叠及当前环境的代表性 CLI 对照完成；CPU opaque 快路径、完整场景矩阵和真实硬件验收仍开放，PERF-17 不整体关闭。

## 57. CPU 不透明场景最近命中组（2026-09-23，本轮实施及专项验收完成，完整任务仍见 §5）

为 PERF-17 评估两阶段 QBVH 路径：先求最近距离，再以向外取整的 `nearest + 2 * dedup_tol` 上限枚举附近命中，使用原距离/triangle ID 排序与 Face-over-Volume 去重，然后只保留首个命中组。不直接采用 QBVH 任意最近三角形，保留颜色、法线及用于覆盖线遮挡的实际命中深度。仅全部三角形 alpha 经原规则 clamp 后等于 1 时可用；零 alpha、部分透明和 NaN 均保留 all-hits。当前两项专项通过（一个显式 ignored release benchmark 另行执行中），包含容差边界、相同深度输入逆序、Face 移动锚点、miss，两种投影和 1/2/8 worker 完整 RGBA/覆盖线等价。性能选择与集成验证待本节后续记录，不据此关闭 PERF-17。

§57 首轮 GPU feature 集成出现一次异常：24 项中 23 通过，mesh_render_cli_worker_budget_and_fallback 的 auto/8-worker 子进程已完成两张 PNG 输出，退出时报告 `double free or corruption (!prev)`。输入是透明场景，因此未使用新的 opaque 路径；尚不能确定根因，不宣称已修复。原始失败见 opaque-nearest-integration.log。随后 10 次独立精确复跑（每次包含 cpu/auto/gpu × 1/2/8 worker）均通过，见 opaque-fallback-repro.log；最终带成功输出的完整 24 项集成复核通过、无设备跳过，见 opaque-nearest-integration-final.log。该一次性退出异常保留为 PERF-19 待追踪风险，不能用复跑成功抹去。

首轮分层基准显示 24 三角形因双查询变慢；生产只对至少 192 三角形的全不透明场景启用，这是已测正收益的最小分层样例规模，不是对任意布局的性能保证。benchmark 显式强制小场景候选路径以保留负收益对照。首轮基准与编译时段有重叠，最终对照另行无并发编译执行；两份原始日志均保留。

§57 最终 release benchmark 通过（opaque-nearest-benchmark-final.log）；30 个配对样本、源码/test-binary/log SHA 与中位数存于 opaque-nearest-raw.json。256² 正交投影，复用准备结构，排除建树和 PNG；每组一轮预热、5 次交替次序。所有场景完整 RGBA 与 all-hits 相同。中位秒：

| 三角形数 | worker | all-hits | nearest group |
|---:|---:|---:|---:|
| 24 | 1 | 0.021518 | 0.028079 |
| 24 | 8 | 0.004932 | 0.007233 |
| 192 | 1 | 0.164470 | 0.050248 |
| 192 | 8 | 0.038249 | 0.010273 |
| 1536 | 1 | 1.244688 | 0.049897 |
| 1536 | 8 | 0.278965 | 0.010667 |

24 三角形最近组路径只作为强制实验负对照，生产保持 all-hits；其余生产使用新路径。以上是重叠分层几何的内核收益，不能外推为稀疏几何或整个 mesh-render 管线收益。PERF-17 的 opaque 路径评估和实现取得进展，完整场景矩阵/硬件验收及上文退出异常仍开放。

## 58. 回退退出异常复现对照（2026-09-23，未定位）

对 §57 的一次 `double free or corruption (!prev)` 做三个二进制的独立进程对照：§54 的 rustmspt-gpu-sep23、§56 的 rustmspt-gpu-frame-writer、当前 debug。每个版本 80 次，共 240 次，全部 cpu_max=8、强制不存在的 GPU selector、auto 回退、透明双视图，全部正常退出。使用诊断专用 LD_PRELOAD SIGABRT backtrace 捕获器（abort_trace.c，未加入产品）；诊断器可能影响内存布局/时序，因此不把未复现视为修复。此前未注入的精确测试 10 次也未复现。脚本 reproduce_fallback_exit.py、binary SHA/退出码 fallback-exit-repro.json、配置和进度日志保留。当前没有足够证据将该异常归因于新 opaque 算法、writer 或驱动；PERF-19 风险保持开放，继续其他可执行优化。

## 59. RAW/TIFF 文件夹有界并行解码（2026-09-23，本轮实施及专项验收完成，完整任务仍见 §5）

consume_file_batches 在当前 Rayon 池内最多并行解码两个文件，按原排序逐一验证并合并结果。结果容器保留每文件 Result，避免并行短路选择非确定错误；前一文件的 shape/type 验证错误优先于后一文件的 decode 错误。出错后不启动后续批次，当前批未消费结果自动释放。1 worker 时批量为 1，保持串行；不会新建线程池。RAW 切片范围/类型/字节序不变；TIFF 文件夹的范围仍选择整个文件、包含其所有页面；每个多页 reader 在自身任务中串行推进。内存界限按文件数，非固定字节上限：单个多页 TIFF 仍完整解码，整个输出 Volume3D 仍常驻。

volume-batch-tests.log 三项通过（RAW 六种整数类型×大小端×1/2/8 worker、TIFF 六种类型/多页顺序与范围、较早 validation 错误胜过较晚 decode 错误）；volume-batch-bound.log 一项验证同时存活解码文件 <=2、消费顺序、错误后的停止/释放。release 暖缓存对照执行中，不宣称冷缓存或端到端 crop 收益。

§59 首轮暖缓存基准（volume-batch-benchmark.log）256²×64 RAW 中位串行 0.014074 s、两文件 0.016018 s，存在退化；同轮 TIFF 有小幅收益。扩展为相同总 4,194,304 体素的 256²×64、512²×16、1024²×4 后（volume-batch-sizes.log），RAW 小切片方向反转，说明噪声不宜忽略；512²/1024² RAW 中位分别 0.013588→0.011233 s、0.020917→0.017392 s。生产因此只对每文件 >=512 KiB 的 RAW 启用最多两文件并行，小 RAW 保持串行；TIFF 单文件批直接执行，多个文件上限仍为二。TIFF 1024²×4 样例只有一个多页文件，两池时序差异不属于并行收益。

新增长 RAW 功能反例（513×512、大端 U16、5 文件、尾批一个文件）在 1/2/8 worker 验证全部体素。volume-batch-final-tests.log 4 项通过、显式 release benchmark 单独执行；volume-batch-final-bound.log 缓冲上限专项通过。crop 默认 CLI 回归 volume-batch-crop.log 通过。最终带阈值版本 release 对照另存 volume-batch-final-benchmark.log，冷缓存近似/峰值 RSS 和完整 crop 端到端性能矩阵仍待验收；writers 与单 TIFF 内分页流水线尚未改动。

§59 最终带阈值 release benchmark 已实际执行通过，30 对样本及 source/test-binary/log SHA 存于 volume-batch-raw.json。每例 4,194,304 个 U16 体素；1-worker 串行参考与 2-worker 生产路径，一轮预热加 5 轮交替次序；完整输出逐值一致。中位秒：

| 格式 | 尺寸/深度 | 串行 | 生产路径 |
|---|---|---:|---:|
| raw | 256²×64 | 0.015410 | 0.014798 |
| raw | 512²×16 | 0.016839 | 0.013911 |
| raw | 1024²×4 | 0.019689 | 0.017654 |
| tiff | 256²×64 | 0.013635 | 0.012324 |
| tiff | 512²×16 | 0.028727 | 0.023858 |
| tiff | 1024²×4 | 0.023474 | 0.021295 |

RAW 256² 已因阈值走串行、TIFF 1024² 只有一个完整文件，两者仅作单任务对照，时间差不得算作并行提速。此表是暖缓存文件夹加载，不包含 crop/PCA/变换/保存，不代表冷存储吞吐或全流程加速。

## 60. TIFF 有界写出与末尾刷新错误（2026-09-23，本轮实施及专项验收完成，完整任务仍见 §5）

文件夹 TIFF 输出按 z 固定文件名，借用原切片，在当前 Rayon 池最多两项编码/写盘并行；每批 join 后逐切片顺序检查 Result，返回首个错误，不启动后续批次。单 worker/单切片批直接执行，多页单文件 TIFF 仍串行。当前失败批可能已有一个或两个部分文件，保留它们，不删除/回滚已有用户文件。没有复制整个 Volume3D，仍会为至多两片建立目标数值类型转换缓冲。

write_tiff_slice 泛化为 Write+Seek，新增 write_tiff_pages 借用 writer，在 encoder 结束后显式 flush，避免原先依赖 BufWriter drop 而吞掉末尾写错。两种输出路径均使用该 helper；只承诺缓冲写出完成，不承诺 fsync 持久化。尺寸乘积使用 checked arithmetic，width/height 转 u32 检查在创建文件前完成。

tiff-writer-units.log 两项通过，其中故障 writer 在完整有效的两页 TIFF 编码后拒绝 flush，调用者收到错误；有效编码字节与成功参考完全相同。新增集成覆盖六种类型、固定文件名、1/2/8 worker 字节一致性、5 页尾批、首片范围错误胜过同批另一文件的创建错误、后续批不启动、尺寸溢出不创建输出。release 基准执行中，不以并行实现本身代替收益验收。

§60 集成完成：tiff-writer-integration.log 中 volume_stream 6 项通过、2 个 benchmark 显式 ignored，crop CLI 1 项通过；本次 TIFF writer release benchmark 已独立实际执行通过。15 对原始样本与 source/test-binary/log SHA 存于 tiff-writer-raw.json。固定 U16 输入、相同目录反复覆盖，预热后 5 对交替样本；所有文件逐字节相同。无 fsync，不声称持久化磁盘吞吐。中位秒：

| 尺寸/深度 | 串行写出 | 两 writer |
|---|---:|---:|
| 32²×64 | 0.000781 | 0.000514 |
| 256²×64 | 0.008315 | 0.004663 |
| 1024²×4 | 0.007637 | 0.005073 |

该项有界编码/写出与最终刷新错误传播已完成代表性验收；完整冷缓存近似、RSS 和 crop 端到端对照仍需补齐，PERF-18 不整体关闭。

## 61. Crop 真实输入端到端与阶段观测（2026-09-23，本轮实施及专项验收完成，完整任务仍见 §5）

run_in_pool 增加完成阶段 wall time：load/background/pca/transform_and_backend/trim/encode_write/total_in_pool。transform_and_backend 包含边界与输出尺寸规划、后端选择/初始化、传输/回读和允许的回退，不冒充 GPU kernel 时间。encode_write 包含 §60 flush、不含 fsync。total_in_pool 排除 CLI/配置/建池，进程基准另测包含所有这些成本的 wall time。失败阶段不生成伪造完成时间。

使用仓库真实 CT RAW 三片（744×789×3）及由 §54 baseline 在 cpu=1、nearest、trim=0 下生成的 TIFF 文件夹（731×353×3），第二种是派生输入，不冒称独立原始 CT。prepare.json/stdout/stderr 保留生成过程。原输入 hash 在矩阵前后核对不变。默认 release 构建通过，候选保存为 rustmspt-default-crop-io，对照为 rustmspt-default-sep23。

矩阵覆盖 RAW/TIFF 输入、nearest/trilinear、单 TIFF/文件夹输出、1/2/4/8 worker、显式读取预热/输入文件 POSIX_FADV_DONTNEED 提示，共 64 配置，每配置一轮不计时预热样本及五轮交替次序配对，合计 768 个独立 CLI 进程。fadvise 仅为冷缓存近似提示，不保证真实冷盘；不清空系统全局缓存。每进程保存配置、stdout/stderr、/usr/bin/time 峰值 RSS、外部 wall time；候选另外校验完整阶段观测。每种输入/插值/输出模式的所有 worker/cache/version 输出 TIFF 全文件 SHA 必须一致。脚本 benchmark_crop_io.py、逐样本 JSONL 和 crop-io-cli/ 保存原始证据，最终结果待本节后续记录。

§61 768 个独立 CLI 全部 exit 0；当前与 baseline、所有 worker/cache 下各对应输出全文件 SHA 一致，原始/派生输入 hash 前后未变，候选全部七项阶段计时存在且有限。完整样本、阶段时间、RSS、input/output/source/binary SHA 存于 crop-io-cli-raw.json，原始进程记录在 crop-io-cli/；此轮含真实 RAW 和其派生 TIFF，不将二者当作两份独立真实数据。

64 个配置中 56 个中位总耗时下降；配置级 candidate/baseline 比值中位为 0.9696，范围 0.8995～1.0875，不宣称每例提速。最差 RAW/nearest/folder/2-worker/cold_hint 为 0.050059→0.054439 s（约慢 8.7%），保留且不剔除。暖缓存 trilinear/文件夹代表结果：

| 输入 | worker | baseline 总秒 | 当前总秒 | baseline RSS KiB | 当前 RSS KiB |
|---|---:|---:|---:|---:|---:|
| raw | 1 | 0.060542 | 0.059116 | 26728 | 27352 |
| raw | 2 | 0.054450 | 0.051984 | 28440 | 29048 |
| raw | 4 | 0.044015 | 0.042786 | 32832 | 33520 |
| raw | 8 | 0.044709 | 0.043405 | 40800 | 41084 |
| tiff | 1 | 0.041544 | 0.042387 | 20952 | 22028 |
| tiff | 2 | 0.036642 | 0.034669 | 23008 | 23424 |
| tiff | 4 | 0.031241 | 0.029959 | 27312 | 27280 |
| tiff | 8 | 0.031606 | 0.030523 | 34604 | 36128 |

真实 RAW 暖缓存 8-worker/trilinear/folder 当前阶段中位：load 7.561 ms，background 16.752 ms，PCA 6.593 ms，transform_and_backend 4.615 ms，encode_write 1.968 ms；阶段中位相加不等于总量中位。背景直方图成为明确后续热点。该轮不覆盖大型完整 CT 栈、所有数值类型或 GPU 精度，不替代 PERF-14/18 的完整矩阵；冷缓存仅 fadvise 近似、输出无 fsync。

## 62. 真实热点驱动的背景稠密计数（2026-09-23，本轮实施及专项验收完成，完整任务仍见 §5）

§61 真实 RAW 背景阶段约 16 ms，逐边界体素 HashMap 查询比 PCA 更耗时，因此为常见整数类型增加有界稠密计数：U8/I8 256 counters；U16/I16 在 volume >=65,536 voxels 时 65,536 counters（64 位主机 512 KiB）。小 16 位和全部 32 位保留原 HashMap 计数路径，避免给未验证负载附加逐体素索引/众数比较开销。提取 for_each_boundary_value 共用原边界遍历，角/棱和退化维度不重复计数。

稠密索引 checked，元数据范围外的任意 i64 值进入 sparse spill，不窄化、不丢弃。流式维护当前最大计数，平票选最小值，与 §29 已确定的语义一致。计数内存在 PCA 前释放。新增独立全网格 BTreeMap oracle，覆盖所有六种整数元数据、8/16 位范围、i64 极值、范围不匹配、平票和退化维度；最终单元日志 background-dense-final-tests.log。端到端脚本 benchmark_crop_background.py 对照 §61 保存的二进制，复用相同真实/派生输入与 64 配置/768 进程矩阵，结果待后续记录。

§62 验收完成：background-dense-final-tests.log 两项独立背景 oracle 通过，background-dense-crop-tests.log 默认 crop CLI 回归通过；默认 release 构建通过，候选保存为 rustmspt-default-crop-background。crop-background-cli-raw.json 保存全部 768 进程、64 配置的真实/派生输入对照，均 exit 0、输入 hash 前后不变、对应输出全文件 SHA 完全一致；两版本阶段计时均保留。没有设备跳过问题，因为本矩阵显式 CPU。

64 组配置中位总耗时全部下降，配置级 candidate/baseline 比值中位 0.748187（约降低 25.2%），范围 0.568669～0.977078。该统计不等于全应用统一加速比；数据只有三片原始 CT 及其派生 TIFF。以下为暖缓存 trilinear/文件夹输出的中位值：

| 输入 | worker | 原总秒 | 新总秒 | 原背景 ms | 新背景 ms | 原 RSS KiB | 新 RSS KiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| raw | 1 | 0.060445 | 0.049614 | 16.375 | 2.166 | 26472 | 26844 |
| raw | 2 | 0.052268 | 0.037379 | 16.799 | 2.066 | 29048 | 29332 |
| raw | 4 | 0.046236 | 0.030872 | 17.620 | 2.001 | 32664 | 32520 |
| raw | 8 | 0.046549 | 0.030476 | 16.714 | 2.182 | 42512 | 40664 |
| tiff | 1 | 0.042734 | 0.034732 | 7.505 | 0.793 | 21576 | 20828 |
| tiff | 2 | 0.039786 | 0.030180 | 7.442 | 0.807 | 23420 | 23104 |
| tiff | 4 | 0.030062 | 0.024203 | 7.509 | 0.802 | 27604 | 27664 |
| tiff | 8 | 0.030303 | 0.023432 | 7.344 | 0.838 | 35168 | 36588 |

稠密表增加最多 512 KiB 的阶段性逻辑内存；进程 RSS 受分配器/调度影响有升有降，不将这些波动当作内存优化收益。fadvise 仍只代表冷输入近似，无 fsync、无真实 GPU 性能结论。此项背景直方图优化及当前环境的真实样例验收完成；近重根 PCA 基底、在线 M2 实验、GPU 分块和完整类型/大体积矩阵仍保持原范围待完成，PERF-14 不整体关闭。

## 63. RAW 汇总容量一次预留（2026-09-23，本轮实施及专项验收完成，完整任务仍见 §5）

§62 后真实 RAW 加载仍约 7～8 ms。RAW 已知宽高及选定文件数，原实现仍由空 Vec 分次 append，跨容量时复制已汇总体素。现检查 plane/bytes/total 乘积，在首片成功解码后一次 try_reserve_exact 最终长度；保留首片错误优先顺序，分配失败显式返回，后续 append 无几何增长。双文件临时解码与原顺序未变，没有声称消除所有拷贝或完整内存上限。

新增最小反例覆盖 plane/byte/output 乘积溢出、错误首片在预留前被拒绝，不为测试分配巨量内存；已有类型/字节序/切片顺序和 crop 测试继续执行。release 与 §62 二进制对照脚本 benchmark_raw_reserve.py 覆盖原始 CT、两种插值/输出、1/2/4/8 worker、warm/cold_hint，共 32 配置和 384 个进程；记录 stage wall time/RSS/全部输出 SHA。结果待本节后续记录。

§63 验收：raw-reserve-tests.log 中 volume_stream 7 项、crop CLI 1 项通过（两个 release benchmark 显式 ignored，不计为本轮通过）；默认 release 构建通过。raw-reserve-cli-raw.json 保存 384 个独立 CPU CLI，全 exit 0、全部对应输出 SHA 一致、原始输入 hash 未变，包含 32 配置的阶段耗时、RSS 和二进制/源码 SHA。候选保存为 rustmspt-default-raw-reserve。

32 配置中 29 个总耗时中位改善，配置级 candidate/baseline 中位 0.965661，范围 0.908076～1.003462。保留所有负收益样本。暖缓存 trilinear/文件夹：

| worker | 原总秒 | 新总秒 | 原 load ms | 新 load ms | 原 RSS KiB | 新 RSS KiB |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 0.047084 | 0.047247 | 9.037 | 8.077 | 27304 | 25680 |
| 2 | 0.039743 | 0.038529 | 9.652 | 7.751 | 28660 | 27764 |
| 4 | 0.030116 | 0.028385 | 8.616 | 6.887 | 32912 | 31748 |
| 8 | 0.030540 | 0.029757 | 9.778 | 8.935 | 41140 | 40088 |

该对照仍仅三片真实 CT；RSS 是整个进程峰值，不是 Vec 请求容量。fadvise 仅冷输入近似，输出未 fsync；此项避免已知 RAW 汇总扩容，不替代完整 TIFF/RAW 类型及大卷内存验收。

## 64. §55～63 后全量回归复核（2026-09-24，完成）

对 SA VF 缓存、GPU PNG writer、CPU opaque 最近命中组、RAW/TIFF 有界 I/O、显式 flush、crop 阶段计时、背景稠密计数、RAW 容量预留后的源码执行新一轮默认/GPU 全量回归。full-regression-post63-source-manifest.json 保存 Rust/WGSL（含两个未变化的测试 shader fixture）、Cargo/build/tests SHA，期间不修改实现。full-default-post63.log 与 full-gpu-post63.log 留原始输出；本节后续记录实际退出状态，不以正在执行冒充通过，也不将既有 §54 结果替代当前版本验收。Mesh Gen 源码无修改，现有测试仅随 cargo test 执行。

收尾结果（退出状态均为 0）：

| 检查 | 实际结果 | 证据 |
|---|---|---|
| 默认全量 `cargo test` | 43 组报告，609 passed / 0 failed / 20 ignored | full-default-post63.log、full-default-post63-summary.json |
| GPU 全量 `cargo test --features gpu -- --show-output` | 43 组报告，657 passed / 0 failed / 28 ignored | full-gpu-post63.log、full-gpu-post63-summary.json |
| 默认/GPU clippy | 均成功，分别 48/55 条库警告 | clippy-default-post63.log、clippy-gpu-post63.log |
| 当前 GPU release | 构建成功，保存 rustmspt-gpu-post63 | release-gpu-post63.log、checks-post63-summary.json |
| 当前默认 release | §63 构建成功，保存 rustmspt-default-raw-reserve | raw-reserve-release.log |

两套测试包含大量重叠测试，不相加为独立用例数。所有 ignored 均为显式性能基准，名单随 summary 保存，不算本轮通过；专项性能证据见前述各节。GPU 成功输出已核对，没有因设备不可用提前返回的跳过记录；关键词命中仅为正常 Mesh Gen 区域分类/THIN-SKIP 输出，不是测试跳过。本轮 GPU 执行基于 llvmpipe，不升级为硬件 GPU 验收，也不能关闭 §58 的偶发退出异常。

测试结束后复核全部 152 个源码/测试/Cargo/build 指纹无变化，见 full-regression-post63-manifest-verification.json。git diff --check 通过，src/meshgen/ 无差异。收尾仅补充 PLAN 和中英文 crop 示例的阶段计时说明，无新增实现修改。本轮启动的测试、clippy 和构建进程均已结束，未留下后台任务；未创建提交，后续工作按 §65 交接。

## 65. 下一 Session 交接（2026-09-24）

**本 Session 按用户要求完成当前部分及记录后收尾，不继续启动优化。整个 Performance Plan 尚未完成。** 原范围仍为 §5 的 PERF-00～19（20 个工作包），排除 Mesh Gen；§10 以后的编号是实施批次，不是完成的工作包数，不据此计算完成百分比。先读原计划与本节，再按任务阅读算法文档、函数契约和需要修改的实现。

交接基线：分支 main，HEAD `891badc09ae03fb99ab57d206451e75de226ee60`，实现与文档留在有大量修改和新增文件的工作区，未创建提交。保留既有修改（包括 Mesh Gen 规范文档等其他工作），不可 reset/clean。此批未修改 `src/meshgen/`。仓库当前没有 `PLAN.md`，本文件即本任务的计划和交接入口。

本轮 §55～63 已实现 SA 连通分量 VF 缓存、GPU 多视图 PNG 有界重叠、CPU 不透明最近命中组、RAW/TIFF 有界解码与 TIFF 写出/flush、crop 阶段观测、背景稠密计数和 RAW 汇总容量预留。实现边界及负收益案例见各节，不把微基准加速比当成整个应用收益。真实 crop 矩阵使用三片 CT 及其派生 TIFF；§62 的 64 配置中位耗时比值中位为 0.748187，§63 的 32 配置为 0.965661，输出 SHA 均一致。大型 CT 和完整精度矩阵仍待完成。

证据根目录：`data/output/performance/20260921-crop-render/`。当前源码身份见 `full-regression-post63-source-manifest.json`；当前全量回归见 §64。最后默认 release 二进制为 `rustmspt-default-raw-reserve`。`rustmspt-default-sep23`/`rustmspt-gpu-sep23` 是 §54 历史对照，`rustmspt-gpu-frame-writer` 仅到 §56，不能当作最新 GPU release。当前默认/GPU clippy 和 GPU release 已在收尾复核成功，日志分别为 clippy-default-post63.log、clippy-gpu-post63.log、release-gpu-post63.log；仍有 48/55 条库 clippy 警告，非零警告验收。最新 GPU release 保存为 rustmspt-gpu-post63，SHA 和命令退出状态见 checks-post63-summary.json。各 benchmark 脚本、raw JSON、逐进程配置/日志和 SHA 均保留于证据目录；目录名不表示所有记录的实际执行日期。

下一 Session 优先核查和推进以下未完成内容，仍以原 §5 的完整验收为准，不缩减范围：

- **PERF-19：** §57 曾在透明场景 auto/8-worker CPU 回退、完成 PNG 后发生一次退出 double free。§58 的 10 次精确复跑及 240 进程诊断对照未复现，根因未定位、未修复。保留原失败和诊断器可能改变时序的限制；后续全量通过不能关闭该风险。
- **PERF-00/19：** 补齐其余管线阶段、计数器、峰值 RSS、CPU 1/2/4/8 和冷/暖端到端矩阵，以及各工作包文档/验收审计。普通回归中的 ignored 性能测试不计为通过，专项结果查各批次记录。
- **PERF-03/04/05/08/09：** 设备与编译管线复用、剩余内存/故障规划、GPU MC BVH/采样分批与 rmax 限制、边界 f32 不确定性认证、shell 成本选择和聚合；负收益的 shell 实验不直接启用生产。当前仅 llvmpipe 软件 GPU、无 /dev/dri，真实 GPU 性能与硬件精度验收保持开放。
- **PERF-07/10：** CPU FFT/direct 成本与内存规划、友好 padding；SA GPU 局部上传/姿态复用和增量 occupancy 实验，以及固定动作下质量与端到端对照。VF 缓存专项不替代 SA 整体性能证明。
- **PERF-11/12/13：** 周期 ghost 实例化、placement 有序候选/库规划与标签流式写出、网格桶占用及候选数观测；宽粒径比下层次结构需证据支持。
- **PERF-14/18：** PCA 近重根基底、在线 M2 实验、GPU 输出分块/source halo、所有数值类型和大型体数据验证。当前两文件上限不是字节上限，完整输出 volume 仍驻留；RAW 预留不是总 RSS 限制。
- **PERF-15/16/17：** split-filter 连通分量发现和 forge/scale 后续热点需实测；CPU 编码重叠与渲染自动成本选择仍待做。§56 GPU writer 使用独立 scoped thread 加零容量通道，后续需审视其与 AGENTS 中新并行使用 Rayon 的约定及完整 worker 预算的一致性，不能在未验证阻塞/回退次序时机械替换。

本轮收尾后没有授权扩展到 Mesh Gen，也不提交或清理工作区。继续时复核源码指纹与实际工作区差异；仅文档更新不会使 §64 的实现验证失效，但后续实现修改需要相应重新验证。


## 66. Cloud Session 批次（2026-09-25，已合并至 `claude/blissful-franklin-lpf9g1`）

环境：云容器 4 核/16 GB，无 /dev/dri；安装 mesa lavapipe（`mesa-vulkan-drivers`）作为软件 Vulkan，GPU 结论均为 llvmpipe，不是硬件验收。基线复现：合并前 `cargo test --release` 43 组 609 passed/0 failed/20 ignored，与 §64 一致。本批由 4+3 个并行 worktree agent 实施，逐个合并；未修改 `src/meshgen/`。证据日志只在云 scratchpad（不入库），数值记录于本节。

**第一轮（已合并，全量回归通过）：**

- **PERF-00/02/13 观测：** 新增 `src/pipeline/timing.rs`（`StageTimer`、`peak_rss_bytes` 读 VmHWM，不可用输出 `unavailable`）。split-filter、legacy pack、placement、optimize、measure、forge、scale、render、mesh-render、crop 统一输出 `[Timing] <pipeline> stage=<name> seconds=<f>`、`workers=<n>`、`peak_rss_bytes=<n|unavailable>`；crop 保持原阶段名。`SpatialGrid::stats()` + `[GridStats]` 行（pack/optimize），pack 查询计数为并行短路下的诊断值，随调度变化。新增 `scripts/perf_matrix.py`（1/2/4/8 worker、冷/暖分离、raw JSON + markdown，S(p)/E(p)）；首轮矩阵在其他构建并发时运行（load≈12/4 核），**加速比不可信，需本地安静机器重跑**。
- **PERF-15：** `split_mesh_into_granules` 改 CSR 邻接 + 数组 remap，旧实现保留为 test oracle，输出逐项一致。release：2 万盒乱序 0.139→0.048 s，有序 0.043→0.0135 s，327,680 面单球 0.104→0.044 s。
- **PERF-16：** `map_vertices_centroid` 融合 void 质心（串行融合、并行先映射再按序求和），与原顺序逐位一致（1/2/8 worker 测试）。ROI 平移融合和 |f|³V 体积捷径因非逐位一致未做。
- **PERF-07：** FFT 轴补齐到 ≥2N−1 的最小 2/3/5-smooth 长度；FFT 相关值取整为整数配对数，FFT 与 direct 曲线逐位相同。固定 24M 阈值改为成本模型 `plan_exact_cpu`（FFT 2.0 ns·P·log2P vs direct 0.36 ns·W/并行折扣）+ 768 MiB 工作集预算，打印 `[Info] CPU exact S2 plan`。measure 的 1.5M 体素硬上限取消，仅在占据或任一内核超预算时拒绝（**无运行时上限**）。1 worker 12 例规划器均选中更快内核（例：96³/r3 FFT 0.428 s vs direct 0.051 s）。常数在共享机器上拟合，需本地重新校准。
- **PERF-14 PCA：** 单遍行级精确整数矩 + Chan 合并（固定 65536 块升序），旧三遍保留为 oracle；13.1M 体素 1 worker 0.581→0.403 s，小体积也更快。近简并特征空间规范基（容差 1e-3，三重取扫描轴，二重按 x/y/z 投影 Gram-Schmidt）。**行为变化：** 非简并轴定号（最大分量为正），仓库真实 CT 帧第 1/2 列相对旧输出取反（绕主轴 180°），crop 输出与 §61–63 基线 SHA 不同，bounds 数值相同仅轴符号互换。
- **PERF-03：** 进程级 `Arc<SharedGpuDevice>` 按 `RUSTMSPT_GPU_DEVICE` 键缓存，失败不缓存，device-lost 后逐出重建；`cached_pipeline` 每设备每 shader 只编译一次；六类构造器全部接入，缓冲仍按实例独立。`runtime::scoped` 加进程级错误作用域锁（共享设备下防止串线程捕获错误；会串行化跨线程 GPU 操作，今后可能成为吞吐限制）。`main` 结束时 `release_shared_gpu_devices()`。测试：4 轮 + 8 线程并发构造仅 1 个设备、6 次编译；非法 selector 两次报错不建设备。暖构造 0.00067 s。
- **PERF-05/08：** GPU MC 取消 r_max<128 限制，按 128 半径分批（`radius_base`），随机数按全局样本编号，批大小不影响固定种子计数（1/7/13/64/128 批、r_max 127 vs 300 oracle）。剩余限制 `(r_max+1)*samples` ≤ u32。
- **PERF-10：** `GpuS2Pipeline::update_mesh` 与驻留主机影子逐位比较，只上传变化面区间（间隙≤8 合并；面数变化/扩容/>64 区间/>50% 变化时整体上传），`upload_stats()` 可观测；逐字回读 + 固定种子 MC 与整体上传一致。主机端仍 O(faces) 比较。
- **PERF-11：** 周期模式 3 以 `(particle_id, shift, bbox)` 存 image，仅在查询 bbox 可达时惰性构造平移网格/TriMesh（`OnceLock`）；oracle 覆盖面/棱/角包裹、接近域宽的粒子、重复 image、嵌套/接触、gap 0/0.25/0.5。真实运行存 112–122 image、只建 11–22 个 ghost。
- **PERF-12：** 标签按 z-slab（`max(1, 4,194,304/(nx*ny))` 层）计算并经 `TiffPageEncoder` 流式写多页 TIFF，不再同时持有两个整卷 i64；slab 大小 1/2/3/4/7/29/30/31/1000 与整卷路径逐字节一致。中途错误可能留下部分 TIFF。
- **PERF-17：** CPU mesh-render `render_and_write_overlapped`（`rayon::join`，最多一帧在写，按序，写错误停止后续渲染），1/2/4 worker 字节一致；实测比值 0.95–1.23，**无可靠收益**。GPU writer（std scoped thread + 零容量通道）未改为 Rayon：生产者为回调式，无法证明等价。mesh-render auto 的 gpu_min_pixels 默认保持 0（改动会改变图像而非仅成本）。
- **PERF-18：** ASCII STL 逐行流式解析，嗅探/二进制回退/`load_stl_hashed` 摘要不变，oracle 覆盖 CRLF、tab、科学计数、缺 endsolid、无效 UTF-8、3 字节短读等。35.5 MB 文件峰值堆 114.2→78.8 MB，时间 0.346→0.371 s（噪声内）。
- **PERF-19（§57 double free）：** 审计 mesh_render/scene_render 无 unsafe/裸指针；30+30 次 auto/8 worker 透明场景、valgrind 5 次、既有 CLI 测试循环 20 次均未复现。**仍未定位**，猜测为 Mesa/LLVM 退出时静态实例的 GL/EGL 析构。

第一轮合并后全量回归：默认 `cargo test --release` 44 组 **629 passed/0 failed/28 ignored**；`cargo test --release --features gpu` 44 组 **682 passed/0 failed/36 ignored**，无设备跳过行。ignored 均为显式基准。

**第二轮：**

- **PERF-14 GPU 分块（已合并）：** 超出 `gpu_memory_limit_mb`/设备单缓冲上限时不再回退 CPU，而是按预算规划输出 tile（整卷→z slab→行组→x 段，二分最大尺寸），每 tile 逆映射 8 角点取源 AABB + f32 误差 margin，仅上传源子块；shader 用 `origin + f32(local + tile_offset)` 重建与整卷相同的绝对坐标，逐字节等于单次 dispatch；halo 越界由 atomic guard 报错。仅当单体素 tile 也放不下时 auto 回退/gpu 报错。llvmpipe 251×225×147 输出：未分块 0.219 s、4 tile 0.259 s、19 tile 0.495 s（分块价值在于适配预算，不在提速）。tile 串行、未重叠传输；主机输出整卷仍驻留。
**PERF-05 f32 不确定性认证（已合并）：** 新增 `src/gpu/certify.rs`。MC 与体素 shader 对每个比较（det、u、v、u+v、t 及去重间隙）用一阶前向误差界（u=2^-24，逐三角形/逐查询，SAFETY=2，另含除零与 flush-to-zero 项）给出真/假/不确定；阈值改为与 CPU 相同（1e-10、锚定 1e-8 去重）。不确定查询写入原子列表（MC：逻辑样本 id + p/q 精确位；体素：单元索引，中心在主机逐位重建），主机用 CPU f64 谓词在 bbox 原点平移后的网格上重算并替换，体素结果回写驻留占据。列表溢出时按实际数量扩容重派发。`certification_stats()` 在 measure/optimize 日志中输出重算比例。测试：近平面/共享面/穿顶点与棱/3e-7 薄片/重复壳/1e9 偏移下与 CPU 逐单元、逐计数完全一致；旧未认证 shader 在薄片上错 743/4096 体素（保留为 `tests/fixtures/*_uncertified.wgsl` 基准）。重算比例 1.3e-3～1.3e-2；llvmpipe 开销：particles.stl MC 1.65→3.98 s、体素 2.22→4.66 s，普通球 0.8～1.3×。measure 1 MiB 批次期望值由 11456 改为 11353（计入认证列表）。列表超规划增长只受设备上限约束，未纳入逻辑预算；逐三角形界可预计算以减小 ALU 成本，未做。
- **PERF-12 item 2（已合并，默认关闭）：** 候选的廉价球/盒测试串行；重叠与包围串行至首个失败对 f；仅 f 之前的对计算精确距离，达到 `pair_parallel_min` 时以 `find_map_first` 并行，结果与串行完全一致（1,500 个固定候选、1/2/4/8 worker 逐尝试原因/计数/变换一致）。端到端在共享机器上无收益（0～10% 退化），`PAIR_PARALLEL_MIN = usize::MAX`。精确距离占约 95% 成本，建议下一步做“超过 gap 即停止”的距离查询（需单独验证边界一致性）。
- **PERF-10 item 5（已合并，默认开启）：** `VoxelCoverage` 每体素 u32 覆盖计数，移动只重查该粒子，拒绝无查询回滚，迁移与每 64 次更新全量重建；240 步 oracle 每步占据与全量体素化一致。单步 40～340× 便宜（2.1M 体素 1 线程 638→1.9 ms）；300 次 voxel_mc 优化 134.6→5.7 s（3 次中位），峰值 RSS 36→55 MB。`INCREMENTAL_VOXEL_OCCUPANCY` 可关闭；SA 结果质量未单独测（占据精确，预期无影响）。

第二轮合并后的最终全量回归结果见 §67 首段。

## 67. 本地 Agent 交接（2026-09-25）

**Cloud Session 按用户要求在 §66 完成后停止。** 分支 `claude/blissful-franklin-lpf9g1` 已推送，包含 §66 全部合并；worktree agent 分支已并入，无未合并工作。本地需自行同步到 Gitea。整个 Performance Plan 仍未完成，以 §5 PERF-00～19 的完整验收为准。

最终回归（§66 全部合并后，llvmpipe）：`cargo test --release --features gpu` 44 组 **694 passed/0 failed/41 ignored**；`cargo test --release` 44 组 **634 passed/0 failed/30 ignored**，均 exit 0。ignored 为显式基准。

本地 Agent 优先事项：

1. **重跑可信基准（PERF-00/19）：** 在安静机器用 `scripts/perf_matrix.py` 跑 1/2/4/8 worker 冷/暖矩阵（云端矩阵受并发构建污染，不可用）；如有真实 Intel/AMD/NVIDIA GPU，完成硬件 GPU release 基准与精度验收（云端仅 llvmpipe），并测量 f32 认证在硬件上的开销与重算比例。
2. **重新校准 PERF-07 常数**（`plan_exact_cpu` 的 FFT/direct ns 系数）于空闲主机；决定 measure exact 是否需要运行时上限。
3. **确认 PERF-14 crop 轴定号行为变化**（真实 CT 输出绕主轴 180°，与 §61–63 SHA 不同）是否可接受；如需旧朝向需另加兼容策略。
4. **PERF-12：** 实现 gap 阈值提前终止的距离查询后再评估并行对检查阈值；安静机器复测 `PAIR_PARALLEL_MIN`。
5. **PERF-05/04：** 认证列表增长纳入逻辑内存预算；逐三角形误差界预计算以降低 shader 开销。
6. **PERF-14：** GPU tile 的上传/派发/回读流水重叠；halo guard 触发时放大块重试；主机输出分块写出。
7. **PERF-17：** GPU writer 若改为 Rayon 需拉取式 GPU API；CPU 写出重叠无收益，可考虑保留或回退。
8. **PERF-19：** §57 double free 仍未复现/定位；`runtime::scoped` 进程级锁在多设备下可能成为吞吐限制；clippy 库警告仍非零。
9. 未触及：PERF-08 GPU BVH、PERF-09 shell 成本选择/生产启用 tile 实验、PERF-13 宽粒径层次网格（需证据）、PERF-18 二进制 STL 并行解码（需证明解析主导）、PERF-11 以外 pack 端到端计时（pack 未设种子）。

文档：各 agent 已同步 en-us/zh-cn reference、algorithms、function-index；部分旧 function-index 行号可能过时。`AGENTS.md` 已追加本批的约定（见其末尾 2026-09-25 段落）。

