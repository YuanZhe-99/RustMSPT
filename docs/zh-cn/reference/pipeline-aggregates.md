# Aggregate placement reference

在阅读这些契约之前，请先阅读[算法文档](../algorithms/aggregate-placement.md)。
实现位于 `src/pipeline/placement_aggregates.rs`，它是放置引擎的子模块，因而与常规颗粒的接受流程及输出写入器共用同一套机制。

| Item | Contract |
|---|---|
| `Member` | 来源壳索引、直径、缩放、包围半径、局部中心与局部四元数；代理体不计入材料。 |
| `Template` | 固定的成员、包络半边长/半径、材料体积总和、初始范围、已完成的扫描次数与 SearchStats。 |
| `Proxy` | 已接受的团簇中心、球半径与世界 AABB，用于廉价的全局查询。 |
| `xyz` | 将 Vec3 转换为序列化的 XYZ 坐标。 |
| `envelope` | 用以原点为中心的球/立方体包住所有实际成员顶点。 |
| `fcc_sites` | 确定性的偶宇称整数格点，按所选容器排序。不使用 RNG。 |
| `settle_one` | 将一个球移动，但不超过其第一次扫掠接触；修改其中心，返回移动距离。 |
| `build_template` | 按分位数确定尺寸、循环形状的 FCC 初始化，固定顺序的接触扫描，可选的真实网格细化，以及独立的成对间隙校验。轮询取消信号。 |
| `world_proxy` | 球体，或旋转后立方体的世界 AABB 包络，不扩展真实成员网格。 |
| `proxy_check` | 域、团簇邻居以及无条件的孔隙包含/表面检查；返回具名的拒绝原因。 |
| `export_template` | 写出真实模板 STL；返回重建信息与密度 JSON。 |
| `plan_templates` | 以整份重复模板逼近累计真实体积，并设有 500 万成员上限；不对 PSD 重新缩放。 |
| `run` | 生成模板、放置代理体、提交已接受的真实成员、保存元数据与标准输出，包括被中断的结果。 |
| `AggregateSpec::default` | 禁用；variants=8，particles_per_cluster=64，sphere，internal_gap=0.1，compaction_sweeps=32，mesh_refinement_sweeps=8。 |
| `SizeSource::quantile` | 对有限的 u in [0,1) 求值，不使用 RNG；sample 以原有的单次 u64 抽取委托给它。 |

`PlacementParams::validate` 校验可选启用项的取值范围与模式兼容性，并将团聚体配置复制到 `ResolvedPlacement`。
`run_placement_in_pool` 仅在启用时才分支。`EngineState::new` 对空计划使用单个单元，以避免在生成之前被取消时出现退化网格。
`PlacementControl` 现在带有阶段标签；轮询、信号与保存语义不变。

Output limit notes：模板最多有 1024 个成员和 128 个变体，目录成员总数最多 65536，包围球扫描最多 1024 次，网格细化扫描最多 128 次。已接受的世界几何通过有界缓存重建；最终的颗粒 STL 一次流式写出一个重建的成员。全局放置是串行的；配置的工作线程池仍用于源加载以及标准的可选输出生成。


| Mesh-refinement item | Contract |
|---|---|
| `PreparedMember` | 一个当前的真实网格、包围盒与缓存的查询层次结构。 |
| `prepare_member` | 构建成员精确的缩放/平移后几何与碰撞形状；拒绝不可用的形状。 |
| `members_clear` | 先做 AABB 拒绝，再做实体相交/嵌套与表面间隙判定。 |
| `refine_meshes` | 固定顺序的原点/坐标轴平面坐标下降，带确定性回溯，不使用随机位置；保持容器边界与每个成员；完成/取消之后做全对检查。 |

| Target-search item | Contract |
|---|---|
| `SearchStats` | 已完成的轮数、候选/策略/已接受计数器，以及目标达成/被中断/预算耗尽的停止原因。 |
| `compose_rotation` | 归一化的 global*local 四元数；保持变换可重建。 |
| `container_volume` | 给定一个半边长时，真实包络球/立方体的体积。 |
| `compression_cost` | 外部支撑的平方，加上收缩容器的超量惩罚与较小的中心范数惩罚。 |
| `recenter` | 仅当实际包络尺寸减小时才做刚性中点平移；保持成对间隙。 |
| `compact_to_target` | 可选的有界确定性向内、旋转、横向与成对搜索；保留最佳可行装配并独立校验，包括被中断的路径。 |
| `deserialize_aggregate_variants` | 正整数或 `auto` 哨兵值；拒绝数值零与任意字符串。 |

`AggregateSpec` 新增可选的内部目标与可配置的策略/预算；新的控制项与混合目录的上限会在加载几何之前校验。
`run` 解析 auto 变体数，构建初级/回退目录，为每个混合阶段预留全局尝试预算，将显式回退抽取追加到计划统计中，
并记录每个阶段以及模板的目标缺口。默认模式为 clusters，默认功能 enabled=false。局部旋转作用于包络、已准备的碰撞几何、
模板 STL、模板元数据以及全局展平的颗粒输出。

## Additional regression contracts

| Test | Independent contract |
|---|---|
| `target_search_rotates_rearranges_and_preserves_real_geometry` | 各向异性来源、所有策略、非恒等的局部旋转、导出的世界重建、间隙与线程确定性。 |
| `target_search_reports_success_budget_and_invalid_controls` | 目标成功与预算缺口的对比，以及无效的有限值/范围控制项。 |
| `automatic_templates_and_mixed_fallback_place_smaller_blocks` | 自动数量、较小阶段的放置、预算与抽取记账、真实的跨阶段间隙、显式的小目录上限。 |
| `shipped_configuration_templates_expose_supported_modes` | 随附的默认/孔隙/团簇/混合 YAML 能以所有受支持的控制项解析。 |
| `stop_during_target_search_preserves_valid_best_template` | 搜索中途停止会保存最佳模板、标准的中断输出以及独立的全对网格间隙校验。 |

这些回归契约实现于 `tests/placement_aggregate_tests.rs`。

## Resumable aggregate state

`AggregateCursor` 存储已完成的模板/目录、生成游标、当前混合阶段及其预留预算、模板 ID 计划、团簇尝试游标、代理体、成员关系与阶段记账。
`save_aggregate` 仅在已提交的安全点、且到期或被强制时才序列化。`run` 按接受顺序恢复代理体索引，并把模板重新导出到所选输出目录。
被中断的当前模板仍作为可视化产物保留，并会被确定性地重建；已完成的模板不会重建。全局尝试次数精确恢复。已完成的快照仍可用于显式的目标扩展。


| Additional item | Contract |
|---|---|
| `exact_fallback_check` | 对每个被提议的较小阶段成员，查询实际颗粒并复用常规的精确可行性判定，保持成员局部的内部间隙；绝不仅因代理体重叠而拒绝。 |

混合模式的 `exact_fallback` 查询在检查点恢复之后使用重建的单个颗粒网格。仅用代理体的放置仍为默认。

## Contact-growth contracts

实现：`src/pipeline/placement_aggregate_contact.rs`（搜索）与 `src/pipeline/placement_aggregate_bodies.rs`（位姿几何内核）。

| Item | Contract |
|---|---|
| `Growth` | 可序列化的待处理计划、已提交/最佳成员以及插入/松弛游标；位姿体缓存不做序列化，恢复时重建。 |
| `Body` / `Bodies` | 每个成员在其自身缩放坐标系中的查询网格/层次结构，加上凸包顶点、主轴与体积，以壳和精确缩放为键；仅构建一次，绝不按位姿重建。 |
| `Bodies::ensure` / `pose` | 构建缺失的体；以等距变换给出成员位姿，不复制或变换几何。 |
| `Bodies::envelope` | 由凸包顶点得到的球/立方体包络；与全顶点包络相等（两种范数均为凸）。 |
| `principal_axes` | 顶点协方差的特征向量，最长范围者在前。 |
| `distance_capped` | min(精确的三角形对表面距离, cap)，通过相对等距变换同时对两个层次结构做分支限界遍历；先做包围球拒绝。不检测嵌套（运动从分离状态开始，并保守地移动）。 |
| `clearance_capped` | 到一组已摆位障碍物的最小带上限距离，过程中逐步收紧上限。 |
| `directions` | 返回确定性的球面方向，不使用 RNG，也不依赖线程。 |
| `orientation` | 每个接近方向的起始朝向：传统的固定旋转，或（`contact_orientation: principal`）使次/主主轴沿接近方向并带确定性自旋。 |
| `advance` | 平移/旋转的保守推进：每一步移动量小于当前精确间隙余量，每步一次带上限距离查询（cap <= 包围半径的一半以保持剪枝紧凑）；在接触、达到步数上限或取消时返回最后一个可行位姿。 |
| `settle` | 可选的朝中心滚动（抬起、切向滑动、下落）；仅接受严格更近的可行位姿。 |
| `insertion_search` | 对方向 x 朝向的直线接近并行评估，稳定排序，可选地对最佳的 `contact_settle_candidates` 做沉降；始终返回可行位姿。 |
| `parallel` | 在 Rayon 线程池上运行相互独立的运动作业，工作线程只带取消控制；结果保持作业顺序，因此输出与线程数无关。 |
| `centre_envelope` | 包围盒居中，然后对凸包顶点做确定性模式搜索；仅当包络缩小时才刚性平移所有成员。 |
| `score` | 整模板包络与中心平方的次级代价，不做修改。 |
| `better` | 先比较包络、再比较中心代价的字典序接受。 |
| `Growth::new` | 固定分位数尺寸/来源分配，并初始化可序列化的构造状态。 |
| `Growth::step` | 提交一次插入或松弛事务（一个成员的六次松弛事务从同一布局并行运行）；预留未来的插入预算；停止时回滚未完成的单元；累计性能剖析计数器。 |
| `Growth::target_reached` | 将最佳成员的材料体积与真实包络代理体积比较。 |
| `Growth::snapshot` | 用世界坐标系中的精确判定独立校验每一对，并以如实的目标/停止状态导出已提交的成员。 |
| `run_growth_batch` | 并发推进同一阶段至多 `threads` 个连续的接触生长模板；心跳线程保持进度刷新；停止信号锁存到运行控制中。 |
| `commit_template` | 追加一个已完成的模板，导出它，并推进变体/阶段游标。 |
| `contact::tests::setup` | 构建一个闭合网格的运动/序列化夹具。 |
| `contact_advance_stops_before_obstacle_even_when_endpoint_is_clear` | 固定首次接触停止、表面间隙与分离运动的逃逸。 |
| `posed_distance_matches_world_frame_distance` | 对旋转/缩放的成员，位姿带上限距离等于世界坐标系的精确距离并遵守上限；凸包包络等于全顶点包络。 |
| `contact_growth_serialized_member_boundary_resumes_exactly` | 要求在序列化的中间状态之后，最终变换/计数器完全一致（不含墙钟时间）。 |
| `contact_growth_geometry_density_and_determinism` | 检查导出的间隙/变换、密度提升与线程确定性。 |
| `contact_growth_controls_and_exhaustion` | 拒绝无效控制项，并在搜索预算为零时保留所有成员。 |

`AggregateCursor::batch` 保存并发生成中的进行中模板（较早的单个 `growth` 条目作为第一个批成员恢复）。`run` 在批边界和停止时保存，且只接纳已完成的模板。`AggregateConstruction` 选择传统 FCC 或接触生长路径。旧的 FCC 部分模板重建语义仅适用于 FCC。

其他回归：`contact_rotation_cannot_tunnel_with_clear_endpoints` 检查扫掠的杆旋转，`contact_growth_cli_stop_resume_matches_uninterrupted` 检查已保存的模板内状态以及二进制一致的恢复后几何。
