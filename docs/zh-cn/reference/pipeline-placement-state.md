# Placement state and geometry reference

请先阅读[设计文档](../algorithms/placement-checkpoints-and-memory.md)。

## Index

| Item | Source | Contract |
|---|---|---|
| `CheckpointSpec::default` | `src/config/placement.rs` | 启用最终/中断保存；时间=60 s，已接受数量=1000，无恢复/扩展。 |
| `PlacementMemorySpec::default` | `src/config/placement.rs` | 估算的保留缓存为 256 MiB；保守的三角形代理筛选已禁用。 |
| `IndividualCursor` | `src/pipeline/placement_checkpoint.rs` | 初级计划、活动群体/索引/尝试次数、阶段、补抽计数与扩展次数。 |
| `SavedState` / `Snapshot` / `Envelope` | same | 有序的记录/累加器，加上游标、模式、兼容性标识、RNG 位置与校验和。 |
| `Store::new` | same | 对实际解析后的物理配置/输入字节/可执行文件做哈希；创建周期性保存的簿记。 |
| `Store::due` | same | 时间/数量触发，仅在安全点评估。 |
| `Store::load` | same | 校验校验和/模式/兼容性/预算/目标；当前代缺失或损坏时回退。 |
| `Store::save` | same | 持久化的临时/上一代/当前代文件提交；错误向上传播。 |
| `snapshot` | same | 在不构造世界网格的情况下捕获记录与精确累加器。 |
| `save` | same | 保存强制/到期的快照；禁用或未到期时不分配 JSON 负载。 |
| `restore` | same | 校验来源/顺序/范围/计数器长度，恢复 RNG，并以冷几何句柄重建有序空间索引。 |
| `place_resumable` | `src/pipeline/placement.rs` | 在批边界恢复初级/补抽的抽取；保持每次抽取的尝试消耗。 |
| `AggregateCursor` / `save_aggregate` | `src/pipeline/placement_aggregates.rs` | 已完成模板目录与全局混合阶段/尝试游标；仅在到期时序列化。 |
| `GeometryCache::new` | `src/pipeline/placement_geometry.rs` | 字节预算，包括合法的零保留模式。 |
| `GeometryCache::get` | same | 串行构造、LRU 命中、仅空闲项淘汰以及活动的不可变固定。 |
| `GeometryCache::summary` | same | 估算的保留字节数，以及命中/重建/淘汰的诊断计数器。 |
| `GeometryHandle::new` | same | 存储共享的规范来源与精确的不可变世界变换。 |
| `GeometryHandle::get` | same | 获取/固定世界网格与精确的 `TriMesh`。 |
| `GeometryHandle::reconstruct` | same | f64 变换，与原始放置接受时完全一致。 |
| `GeometryHandle::proxy` | same | 保守的 12 三角形有向外包盒；不对颗粒/VF 重新缩放。 |
| `PlacedParticle::prepared` | `src/pipeline/placement_feasibility.rs` | 缓存的生产几何，或独立的直接夹具几何。 |
| `write_particles_stl` | `src/pipeline/placement_geometry.rs` | 流式写出每个颗粒的精确几何，不生成合并网格；对 u32 STL 三角形数做检查。 |
| `particle_at_tile` | `src/pipeline/placement_labels.rs` | 瓦片局部的固定查询视图，所有权按接受顺序。 |

检查点文件与进度摘要相互独立。运行时缓存/QBVH 不做序列化。显式的已完成扩展会保留原有放置记录并追加新计划；已保存的失败尺寸历史仍然可见。


## Initial assembly import

`placement_initial::load` 校验记录/报告哈希、模式、坐标系、来源标识、孔隙标识以及每个继承的真实颗粒，然后初始化有序的接受/材料/类别计数。计划包含继承的与新增的抽取，但放置游标仅尝试补足缺口。该导入是一个新任务，绝不接受不兼容的旧 RNG 检查点。`Store::new` 会对初始记录/报告字节做哈希，因此更改其中任何一个都会使后续恢复被拒绝。

## Free-space guidance contracts

请先阅读[算法文档](../algorithms/free-space-guided-placement.md)。

| Item | Source | Contract |
|---|---|---|
| `FreeSpaceSpec::default` | `src/config/placement.rs` | 可选的有界搜索控制；默认仍为均匀采样。 |
| `free_space::Index::new` | `src/pipeline/placement_free_space.rs` | 在单元数/估算内存上限内分配未知的粗格点。 |
| `Index::score` | same | 真实网格/孔隙的带符号中心间隙；AABB 仅用于查询/剔除。 |
| `Index::propose` | same | 带种子的中心/抖动或探索；惰性打分期间协作停止时返回 None。返回 None 时调用方回退提议 RNG。 |
| `Index::feedback` | same | 提交一次提议结果，有界细分；绝不硬性排除含糊的单元。 |
| `Index::inserted` | same | 在真实材料被接受后使局部分数失效。 |
| `Index::begin_draw` | same | 按逻辑尺寸标识重置临时惩罚。 |
| `Index::rebuild` | same | 恢复后精确重建堆/成员关系。 |
| `Index::summary` | same | 估算的索引内存，以及分开的打分/提议统计。 |
| `checkpoint::import_remaining_plan` | `src/pipeline/placement_checkpoint.rs` | 校验模式 1/2 的校验和/报告/已接受记录；先保留待处理项，再保留可选的失败尺寸多重集，不产生新抽取。 |

快照模式 2 存储引导单元与失败抽取。派生的堆/缓存状态被省略。在所有线程数下，单个颗粒的引导提议都按串行提交。
