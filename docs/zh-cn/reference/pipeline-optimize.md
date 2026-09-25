# Optimize 管线参考

[SA 算法](../algorithms/simulated-annealing-island-model.md) · [S2 定义](../algorithms/s2-two-point-correlation.md) · [配置](config.md)。

## 索引

| Item | Location | Summary |
|---|---|---|
| `OptimizePipeline` | `src/pipeline/optimize.rs:27` | 管线配置与有界执行入口。 |
| `ParticlePrepared` | `src/pipeline/optimize.rs:32` | 缓存网格、包围盒与碰撞形状。 |
| `IslandResult` | `src/pipeline/optimize.rs:39` | 最佳几何/loss/S2 快照与候选阶段计时。 |
| `GlobalBest` | `src/pipeline/optimize.rs:47` | 由互斥锁保护的完整几何/loss/S2 迁移快照。 |
| `prepare_particle` | `src/pipeline/optimize.rs:54` | 为单个颗粒准备碰撞查询结构。 |
| `format_s2_series` | `src/pipeline/optimize.rs:61` | 将曲线格式化为六位小数。 |
| `push_history_s2` | `src/pipeline/optimize.rs:86` | 向历史记录追加带标签的曲线。 |
| `prune_progress_message` | `src/pipeline/optimize.rs:91` | 格式化剪枝 loss、VF 与颗粒数。 |
| `selective_prune_to_target_vf` | `src/pipeline/optimize.rs:103` | 在当前线程池内使用统一 S2 定义进行剪枝。 |
| `run_sa_island` | `src/pipeline/optimize.rs:296` | 用固定求值器和完整迁移快照运行单岛 SA。 |
| `OptimizePipeline::run` | `src/pipeline/optimize.rs:741` | 将全部 optimize 阶段安装到一个按配置创建的 Rayon 池。 |
| `OptimizePipeline::run_in_pool` | `src/pipeline/optimize.rs:768` | 解析执行策略、加载准备、剪枝、分批运行岛并复核保存最佳结果。 |
| `S2Method` | `src/pipeline/optimize_execution.rs:12` | 内部 voxel_exact、voxel_mc 与 mesh_mc 三种定义。 |
| `S2Method::resolve` | `src/pipeline/optimize_execution.rs:20` | 保留现有 exact/非 exact 与 pitch 路由语义。 |
| `S2Method::name` | `src/pipeline/optimize_execution.rs:31` | 返回诊断使用的实际方法名。 |
| `resolve_mode` | `src/pipeline/optimize_execution.rs:41` | 环境变量覆盖 YAML，拒绝非法值。 |
| `select_s2_backend` | `src/pipeline/optimize_execution.rs:57` | GPU 探测前检查方法、CPU/auto 与容量条件，并执行回退策略。 |
| `OptimizeS2` | `src/pipeline/optimize_execution.rs:130` | 每次运行固定的方法、pitch 与可选共享 GPU 求值器。 |
| `OptimizeS2::new` | `src/pipeline/optimize_execution.rs:144` | 一次解析执行策略，最多初始化一个持久 GPU MC 实例。 |
| `OptimizeS2::evaluate` | `src/pipeline/optimize_execution.rs:220` | 一致地计算各阶段 S2；串行使用 GPU 缓冲，VF 计算前释放锁。 |
| `run_island_batches` | `src/pipeline/optimize_execution.rs:275` | 按当前 Rayon worker 数限制分批执行，保留岛顺序。 |
| `merge_prepared_particles` | `src/pipeline/optimize.rs:61` | Merge geometry and assign stable particle vertex ranges. |

## 类型

`OptimizePipeline { config: OptimizationConfig }` 实现 `Pipeline`。
`ParticlePrepared` 缓存 `mesh: Mesh`、`bbox: Option<BoundingBox>`、`shape: Option<TriMesh>`。
`IslandResult` 保存 `best_particles`、`best_loss`、`best_s2`、`s2_time`、`collision_time`；
计时仅覆盖该岛的候选求值及约束检查，不代表整个管线耗时。
`GlobalBest { loss: f64, particles: Vec<Mesh>, s2: Vec<f64> }` 由 `Arc<Mutex<Arc<GlobalBest>>>` 保护，
三者必须来自同一个已求值的几何状态。

## optimize.rs 函数

#### prepare_particle

`fn prepare_particle(mesh: Mesh) -> ParticlePrepared` 消耗网格，计算 bbox 和 parry 碰撞形状；无 I/O。

#### format_s2_series

`fn format_s2_series(values: &[f64]) -> String` 将六位小数的数值以空格连接；无副作用。

#### push_history_s2

`fn push_history_s2(history_log: &mut Vec<String>, label: &str, values: &[f64])`
向缓冲追加 `label: formatted_curve`；不执行 I/O。

#### prune_progress_message

`fn prune_progress_message(current_loss: f64, current_vf: f64, target_vf: f64, particles: usize) -> String`
格式化剪枝进度；无副作用。

#### selective_prune_to_target_vf

```rust
fn selective_prune_to_target_vf(
    particles: &mut Vec<Mesh>, box_bounds: BoundingBox, target_s2: &[f64],
    params: &OptimizationParams, r_max: usize, evaluator: &OptimizeS2,
    history_log: &mut Vec<String>,
) -> Result<()>
```

必须在调用方已安装的线程池内运行。所有移除候选通过同一个 `evaluator` 的 `prune` 阶段求值，
保留原减少采样预算与按目标长度截取半径的规则。返回 `Result<()>`，传播求值错误并修改颗粒及历史记录。
禁用剪枝、颗粒不足两个或目标 VF 非正时立即返回。批量大小、移除顺序、随机数与几何 VF 统计不变；
不创建额外后端或线程池。

#### run_sa_island

```rust
fn run_sa_island(
    island_id: usize, num_islands: usize, prepared_init: Vec<ParticlePrepared>,
    target: &[f64], params: &OptimizationParams, box_bounds: BoundingBox,
    mode: u8, d1: f64, d2: f64, min_neighbor: f64, rotation_mode: &RotationMode,
    evaluator: &OptimizeS2, global_best: Option<&Arc<Mutex<Arc<GlobalBest>>>>,
    migration_interval: usize, history_log: &mut Vec<String>,
) -> Result<IslandResult>
```

每个岛拥有独立的颗粒、随机数、温度和接受窗口。初始化、候选与迁移复核使用相同求值器，
阶段分别为 `initial`、`candidate`、`migration`。边界/碰撞拒绝、周期镜像、回滚、网格重建与
Metropolis 规则见算法文档。候选 MC 样本数为 `round(mc_samples * (0.3 + 0.7 * scale))`，
钳制到 `[1000, max(mc_samples,1000)]`：高温样本更多，降温后更少。初始化与迁移至少使用 2000 个样本。

迁移同时发布/采用几何、loss 和 S2；采用历史最佳快照后，以完整预算重新求值当前状态。
历史最佳仍绑定其自身曲线；MC 噪声可使该曲线与复核不同。几何准备和 S2 求值前释放全局锁，
避免嵌套 Rayon 工作等待调用方持有的同一把锁。返回最佳快照和候选阶段计时，修改历史及全局状态，打印进度。

#### OptimizePipeline::run

`fn run(&self) -> Result<()>` 保留 `cpu_max` 到可用并行度的钳制规则，创建一个 Rayon 池并安装
`run_in_pool`。创建失败返回 `InvalidConfig`。输入/参考准备、VF、剪枝、岛任务和输出处理都在池内执行。

#### OptimizePipeline::run_in_pool

`fn run_in_pool(&self, thread_pool: &ThreadPool) -> Result<()>` 要求已安装该池。
加载、分裂、预过滤几何，解析 `OptimizeS2`，测量目标/输入并剪枝。单岛直接消耗准备好的向量；
多岛通过 `run_island_batches` 分批执行，仅在各任务真正开始时复制初始几何。
按岛编号收集历史，由最小历史 loss 选择最佳结果。

使用相同方法、`max(mc_samples,2000)` 样本复核最佳几何，记录 `Selected Search Loss`、
`Final Best S2` 与 `Final Best Loss`，此复核在可选定向之后执行，随后写 STL 和历史。
因此 MC 最终 loss 可能不同于历史选择分数。集成测试将 exact 最终曲线与已保存几何对照。
历史包含实际方法、后端、有效 pitch、原因、worker 数、岛数和活跃岛上限。
传播配置、后端和 I/O 错误；不再逐岛创建线程、线程池或 GPU 管线。

## optimize_execution.rs 执行契约

#### S2Method::resolve

`fn resolve(params: &OptimizationParams) -> S2Method`：`exact` 对应 `VoxelExact`；其余方法在
pitch 非正时对应 `MeshMc`，pitch 为正时对应 `VoxelMc`。旧非 exact 字符串（包括 `both`）保留 MC 路由。
无副作用。`S2Method::name(self) -> &'static str` 返回 `voxel_exact`、`voxel_mc` 或 `mesh_mc`。

#### resolve_mode

`fn resolve_mode(configured: AccelerationMode, override_value: Option<&str>) -> Result<AccelerationMode>`
接受 `cpu`、`gpu`、`auto` 或无覆盖值；其他覆盖值返回 `InvalidConfig`。无 I/O。

#### select_s2_backend

`fn select_s2_backend(params: &OptimizationParams, requested: AccelerationMode, workload: usize,
probe: impl FnOnce() -> BackendSelection) -> Result<BackendSelection>`：

- CPU 与低于阈值的 auto 在探测前返回 CPU。低于阈值的 auto 属于正常策略选择，即使禁止回退也允许使用 CPU。
- GPU MC 只兼容 `mesh_mc`。体素方法保留 CPU 定义并报告原因；禁止回退时返回错误，不改变 S2 定义。
- 当前 optimizer GPU 仅支持 `backend: wgpu`、`gpu_precision: f32`、`gpu_prefer_power: false`。
  当方法可进入 GPU 时，不兼容选项明确报错。
- 明确设置 GPU 内存上限时保守使用 CPU，禁止回退则报错，直到工作集规划器完成。
  不把共享策略中单 binding 上限比较伪装成整个工作集预算。
- 超过 128 个半径、调用数溢出或超过 `65535*256` 时，在探测前拒绝 GPU 路径；计数包含剪枝预算和 2000 样本下限。
- 其他情况调用一次探测函数；GPU 不可用时遵循 `cpu_fallback`。

#### OptimizeS2::new

`fn new(params: &OptimizationParams, bbox: BoundingBox, mesh: &Mesh) -> Result<OptimizeS2>`
读取一次 `RUSTMSPT_ACCELERATION`（在 optimize 中覆盖 YAML），检查 pitch 有限，并解析方法、有效 pitch 与后端。
exact 的非正 pitch 一次性规范为 1.0。mesh MC 的 auto 工作量估算仍采用旧 pitch=1 网格估算。
选中 GPU 后，最多创建一个由所有阶段和岛共享的持久 MC 实例；初始化失败准确报告回退或按配置报错。
现有能力探测仍单独创建临时 device，因此不等于完成 GPU 上下文复用。

#### OptimizeS2::evaluate

`fn evaluate(&self, mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize,
stage: &'static str) -> Result<Vec<f64>>` 在当前 Rayon 池中用固定方法及 pitch 求值。
GPU 互斥锁覆盖上传、dispatch 与回读，**CPU VF 计算之前释放**，因为后者可使用嵌套 Rayon。
mesh MC 的 VF 遵循 CPU 连续网格参考定义，体素方法保留占据率 VF。
测试构建在此边界记录阶段、实际 worker 数及索引。阶段为 `target`、`input`、`prune`、`initial`、
`candidate`、`migration`、`final`。MC 的映射及捕获的设备错误现按下文策略传播；f32 数值认证和完整工作集预算仍为 PERF-04/05 待办；
本改动不宣称完成全部 GPU 正确性和资源验收。

#### run_island_batches

`fn run_island_batches<T: Send>(islands: usize, run: impl Fn(usize) -> T + Sync) -> Vec<T>`
按当前池大小限制连续批次，在批内运行 indexed Rayon 任务，按岛顺序返回所有结果。
嵌套任务窃取也不能激活超过一批的岛上下文，多余岛排队。结果保留内存仍随岛数增长。
不创建额外线程或线程池；调用方必须先安装预期线程池。

## 验证

`cargo test --offline --lib pipeline::optimize` 在真实求值/worker 边界观察执行，验证无 GPU 探测的选择条件，
并检查迁移快照一致性。`cargo test --offline --test optimize_execution_tests --test pipeline_smoke_tests`
覆盖 CLI、环境变量覆盖、参考目标、保存后的 exact 曲线及岛数超过 worker 的情况。
加 `--features gpu` 重复运行；共享 GPU MC 测试打印适配器或明确跳过原因。通过现有管线测试不代表完成硬件 GPU 性能验收。

### 运行时 GPU 回退（2026-09-13）

`OptimizeS2` 保存 `Option<Mutex<Option<GpuS2Pipeline>>>` 和 `cpu_fallback`。上传/执行/读回失败后，允许回退时在锁内移除失败实例，释放锁后记录阶段和原因，再用 CPU mesh MC 重算；后续阶段保持 CPU。禁止回退时返回带阶段及原因的 `RustMsptError::Gpu`。`evaluate`、pruning、island 均改为返回 `Result`，管线在写最终 STL/history 前传播错误；已启动的岛批次会先完成等待。启动描述记录初始后端，后续切换由 warning 记录。

### Incremental grid acceptance (PERF-10/13)

`SpatialGrid` retains reverse item-to-cell membership. `remove(idx)` removes all insertions of that id and preserves remaining bucket order; `update(idx, Option<BoundingBox>)` replaces membership or removes the item. Query order remains first encounter, but moving an item appends it in its new buckets, so consumers must not assume rebuild order. Candidate sets match a full rebuild. Optimize queries the current grid before evaluating a proposal, updates one membership only after acceptance, and restores the original prepared particle directly on rejection. Whole-population migration still rebuilds the grid. Repeated inserts retain their old semantics and removal clears every copy. Reverse membership consumes additional memory proportional to inserted cell references.

### Persistent merged geometry (PERF-10)

`merge_prepared_particles` directly assembles prepared meshes and records each particle's vertex range without temporary particle clones. Rigid candidates overwrite only their range; rejection restores its original vertices and prepared collider. Faces and other vertex ranges stay resident. Population migration recreates the merged mesh/ranges. GPU triangle uploads and voxel occupancy remain full updates; geometric VF now uses the island-local cache described below.

### Optimize MC 显存预算

优化器的共享 GPU MC 管线以空几何启动，各阶段上传实际求值网格。mc_evaluation_peak 保守计算 triangle、四个 output/readback、576 字节参数缓冲、待执行队列上传以及本次几何/参数上传；增长时计入旧容量加新容量。启动按输入面数和最大配置阶段样本数检查，每次求值在同一 GPU mutex 内重新检查实际保留容量后才上传。因此更大的参考网格或保留峰值也可能触发原有同方法 CPU 回退或严格阶段错误。待上传字节仅在成功回读后清零。驱动内部及 CPU 网格/读回向量不属于逻辑 GPU 预算；分批和自动缩容仍待完成，release_output_capacity 提供显式释放。本节取代早先“显式预算始终回退”的说明。

### 不可变迁移快照

每个岛用 Arc<GlobalBest> 同时保存最佳几何/loss/S2，IslandResult 保留该 Arc 和计时。改进时在迁移锁外构造新快照，共享槽为 Arc<Mutex<Arc<GlobalBest>>>。exchange_best_snapshot 在锁内仅严格比较 loss 和交换/克隆 Arc 引用；退役快照在解锁后释放，平局保留当前状态。接收岛在锁外重建可变 prepared/grid 并复算当前游走，历史最佳曲线仍对应原几何/loss。最终选优借用获胜快照，不复制 payload。此改动消除迁移 payload 拷贝；创建新本地最佳和准备接收后的可变游走仍复制几何。

| `exchange_best_snapshot` | `src/pipeline/optimize.rs:52` | Exchange immutable best Arc snapshots; release retired payload outside the lock. |

mesh-MC 岛现以 IslandVolumes 缓存每粒子连通分量的域内体积。可行候选只替换对应粒子条目，拒绝恢复旧条目；迁移重建，每 64 次候选求值全量刷新。VF 仍按合并网格的源顺序累加全部缓存标量并使用原分母/截断规则，避免运行总量反复加减导致漂移。缓存仅用于连续 mesh MC，voxel 方法保留体素 VF。CPU 固定种子采样和 GPU 错误回退均可使用已验证 VF，不重复裁剪全体；GPU 锁边界不变。完全域内粒子的变换也重新计算贡献以保持浮点参考行为，未假定刚体体积位级不变。预剪枝和最终验证仍使用全量参考求值。

| Function | Source | Contract |
|---|---|---|
| `IslandVolumes::new` | `src/pipeline/optimize_volume.rs:12` | Build ordered connected-component contributions for a population. |
| `IslandVolumes::contributions` | `src/pipeline/optimize_volume.rs:20` | Clip connected components using the existing geometric volume definition. |
| `IslandVolumes::replace` | `src/pipeline/optimize_volume.rs:33` | Update one particle and return prior entries for rollback. |
| `IslandVolumes::restore` | `src/pipeline/optimize_volume.rs:41` | Restore entries after rejection. |
| `IslandVolumes::fraction` | `src/pipeline/optimize_volume.rs:46` | Sum cached scalars in merged component order, then clamp. |
| `OptimizeS2::evaluate_with_vf` | `src/pipeline/optimize_execution.rs:242` | Evaluate mesh MC with optional validated VF; reject geometric cache for voxel methods. |
| `calculate_s2_mesh_mc_seeded_with_vf` | `src/geometry/s2.rs:765` | Preserve fixed-seed mesh MC samples while using caller-provided VF. |

### 网格统计与查询计数（PERF-13 观测）

每个岛结束时，`run_sa_island` 打印 `SpatialGrid::stats()`（`[GridStats] optimize island=<id> buckets=..`）以及 `[GridStats] optimize island=<id> queries grid_queries=.. grid_candidates=.. bbox_rejects=.. narrow_phase=.. distance_checks=..`，统计串行碰撞循环中的候选与 ghost 查询、包围盒拒绝、精确重叠测试和精确距离测试。计数是紧挨现有分支递增的局部整数；不改变任何决策、RNG 抽样或历史记录。

**GPU 局部上传（PERF-10）。** GPU MC 三角形上传现在将新的合并网格与驻留缓冲的主机影子副本逐位比较，只写入发生变化的面区间（移动一个粒子即只上传该粒子的面）；三角形数量变化、缓冲扩容、区间超过 64 段或变化面超过一半时回退为整体写入；`GpuS2Pipeline::upload_stats` 记录字节数。体素占据仍为整体更新。

### GPU MC 认证摘要

`OptimizeS2::gpu_certification_summary() -> Option<String>`（`src/pipeline/optimize_execution.rs`）短暂锁定共享 GPU MC mutex，描述其累计 `GpuCertificationStats`（有效样本、CPU 重算样本数、比例）。CPU 执行或回退已移除 GPU 实例时返回 `None`。`OptimizePipeline::run` 在 `Optimization completed.` 之前打印 `[Info] Optimize GPU mesh_mc f32 certification: ...`。
