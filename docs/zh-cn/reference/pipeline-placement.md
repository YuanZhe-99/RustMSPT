# 流水线：放置（Placement）参考文档

本页记录带种子、可复原、感知孔隙的打包引擎：`src/pipeline/placement.rs`（主循环与输出）、
`placement_sizes.rs`（放什么尺寸）、`placement_library.rs`（有哪些形状可用）、
`placement_feasibility.rs`（某个候选是否被允许）、`placement_outputs.rs`（记录与报告类型）、
`placement_labels.rs`（可选的体素场），以及 `src/geometry/void_index.rs` 中的孔隙索引。

该引擎由携带顶层 `placement:` 块的 `pack` 配置选中。`packing:` 块则选中原有的打包循环，它记录在
[pipeline-packing.md](pipeline-packing.md)，且未作改动。

> **算法：** 这些代码背后的决策及其论证见
> [void-aware-placement.md](../algorithms/void-aware-placement.md)。配置见 [config.md](config.md)。
> 带真实输出的完整运行见 [pack-placement.md](../examples/pack-placement.md) 与
> [pack-void.md](../examples/pack-void.md)。

## 索引

| 条目 | 位置 | 摘要 |
|---|---|---|
| `VoidVolumeMethod` | `src/geometry/void_index.rs:18` | 给出孔隙域内体积的计算方法。 |
| `VoidIndex` | `src/geometry/void_index.rs:32` | 冻结孔隙，为放置运行的各类查询建立索引。 |
| `VoidIndex::build` | `src/geometry/void_index.rs:56` | 校验孔隙网格并建立索引；朝向不一致时拒绝。 |
| `VoidIndex::bbox` | `src/geometry/void_index.rs:118` | 孔隙的包围盒。 |
| `VoidIndex::shells` | `src/geometry/void_index.rs:123` | 孔隙包含多少个闭合壳。 |
| `VoidIndex::is_outward` | `src/geometry/void_index.rs:128` | 孔隙各壳是否朝外缠绕。 |
| `VoidIndex::total_volume` | `src/geometry/void_index.rs:133` | 符号一致的各壳求和得到的孔隙总体积。 |
| `VoidIndex::contains_point` | `src/geometry/void_index.rs:150` | 射线奇偶的点在孔隙内判定；先做包围盒预检，再调用 `trimesh_contains_point`。 |
| `VoidIndex::near_box` | `src/geometry/void_index.rs:166` | 包围盒预筛：为假即远离孔隙且未被其嵌套。 |
| `VoidIndex::intersects` | `src/geometry/void_index.rs:186` | 颗粒表面是否与孔面相交。 |
| `VoidIndex::min_distance_to` | `src/geometry/void_index.rs:202` | 颗粒到孔隙的最小面到面距离。 |
| `VoidIndex::surface_distance` | `src/geometry/void_index.rs:219` | 点到孔面的无符号距离。 |
| `VoidIndex::any_vertex_inside` | `src/geometry/void_index.rs:233` | 网格是否有顶点落在孔隙内部。 |
| `VoidIndex::any_void_vertex_inside` | `src/geometry/void_index.rs:245` | 孔隙是否有顶点落在颗粒内部。 |
| `VoidIndex::volume_in_domain` | `src/geometry/void_index.rs:264` | 孔隙在域内的体积，以及所用的计算方法。 |
| `VoidIndex::sample_surface_point` | `src/geometry/void_index.rs:288` | 按面积加权在孔面上取点，并给出外法向。 |
| `VoidIndex::overlap_volume` | `src/geometry/void_index.rs:330` | 以域锚定的体素计数给出颗粒落在孔隙内的体积。 |
| `point_inside_mesh_local` | `src/geometry/void_index.rs:375` | 对无层次结构的小网格做射线奇偶判定。 |
| `PlacementPipeline` | `src/pipeline/placement.rs` | 持有已校验 `ResolvedPlacement` 的流水线结构体。 |
| `PHASE_MATRIX` | `src/pipeline/placement_labels.rs` | 标签场中相编码 0。 |
| `VoxelLabelsHeader` | `src/pipeline/placement_labels.rs` | 标签体数据的说明：间距、原点、排布与相表。 |
| `PhaseLabel` | `src/pipeline/placement_labels.rs` | 一个相编码及其名称。 |
| `write_voxel_labels` | `src/pipeline/placement_labels.rs` | 写出三相标签场与逐体素颗粒标识场。 |
| `particle_at` | `src/pipeline/placement_labels.rs` | 查找包含某点的已放置颗粒。 |
| `point_in_particle` | `src/pipeline/placement_labels.rs` | 对单个颗粒网格做射线奇偶包含判定。 |
| `VoidReport` | `src/pipeline/placement_outputs.rs:278` | 运行如何处理冻结孔隙，以及如何度量它。 |
| `build_void_report` | `src/pipeline/placement.rs` | 为报告描述冻结孔隙，含其体积计算方法。 |
| `PlacementPipeline` | `src/pipeline/placement.rs` | 持有已校验 `ResolvedPlacement` 的流水线结构体。 |
| `PlacementOutcome` | `src/pipeline/placement.rs` | 一次完整运行的产出，供进程内调用方使用。 |
| `run_placement` | `src/pipeline/placement.rs` | 在按配置创建的专用 Rayon 线程池中运行引擎并写出全部输出。 |
| `with_placement_pool` | `src/pipeline/placement.rs` | 为一次操作创建并安装专用 Rayon 线程池，传播创建及执行错误。 |
| `run_placement_in_pool` | `src/pipeline/placement.rs` | 在当前线程池内执行所有 placement 阶段，并记录实际 worker 数。 |
| `resolve_threads` | `src/pipeline/placement.rs` | 把线程设置换算为至少为 1 的工作线程数。 |
| `EngineState` | `src/pipeline/placement.rs` | 放置循环累积的全部状态。 |
| `place_resumable` | `src/pipeline/placement.rs` | 恢复主计划与补抽计划以及当前各尺寸的尝试计数。 |
| `try_place_one` | `src/pipeline/placement.rs` | 在单颗粒尝试预算内尝试放置一个尺寸。 |
| `Proposal` | `src/pipeline/placement.rs` | 一次尝试的随机变量及其后的流位置。 |
| `Evaluation` | `src/pipeline/placement.rs` | 候选的检查结果：拒绝原因，或被接受的候选及其检查结果。 |
| `draw_proposal` | `src/pipeline/placement.rs` | 按固定消费顺序抽取一次尝试的随机变量，并记录流位置。 |
| `evaluate_proposal` | `src/pipeline/placement.rs` | 对未改变的已放置集合运行一个候选的全部检查；只读。 |
| `SPECULATIVE_BATCH_PER_WORKER` / `SERIAL_ATTEMPTS_BEFORE_BATCHING` | `src/pipeline/placement.rs` | 投机批次上限（每 worker 8 个）与开始批处理前的串行尝试数（4），均为实测确定。 |
| `accept` | `src/pipeline/placement.rs` | 把已接受的候选提交进几何与记录。 |
| `decide_stop` | `src/pipeline/placement.rs` | 判定正常完成还是用户中断；中断优先。 |
| `write_outputs` | `src/pipeline/placement.rs` | 写出几何、逐颗粒记录与尺寸 CSV。 |
| `entity_id` | `src/pipeline/placement.rs` | 已放置颗粒的稳定标识。 |
| `particle_record` | `src/pipeline/placement.rs` | 把一个已放置颗粒转为其记录条目。 |
| `size_class_rows` | `src/pipeline/placement.rs` | 构造逐分组的目标与实际对照行。 |
| `blank_report` | `src/pipeline/placement.rs` | 放置开始之前的报告初始形态。 |
| `describe_input` | `src/pipeline/placement.rs` | 为报告描述输入文件及其摘要。 |
| `finish_report` | `src/pipeline/placement.rs` | 填入运行结束后已知的全部内容。 |
| `summary` (placement.rs) | `src/pipeline/placement.rs` | 构造供人阅读的 stdout 摘要。 |
| `read_record` | `src/pipeline/placement.rs` | 读回已写出的逐颗粒记录。 |
| `read_report` | `src/pipeline/placement.rs` | 读回已写出的运行报告。 |
| `RejectReason` | `src/pipeline/placement_feasibility.rs` | 候选放置未被接受的原因；即报告中的键。共十个变体。 |
| `RejectReason::as_str` | `src/pipeline/placement_feasibility.rs` | 拒绝原因在报告中的稳定键名。 |
| `PlacedParticle` | `src/pipeline/placement_feasibility.rs` | 通过全部检查的颗粒，附带缓存的形状。 |
| `PlacedParticle::volume_in_domain_solid` | `src/pipeline/placement_feasibility.rs` | 计入固相的颗粒体积。 |
| `FeasibilityContext` | `src/pipeline/placement_feasibility.rs` | 可行性检查所读取的全部内容。 |
| `Candidate` | `src/pipeline/placement_feasibility.rs` | 候选放置，附带已预先算好的廉价量。 |
| `Accepted` | `src/pipeline/placement_feasibility.rs` | 通过检查过程中顺带算出的结果。 |
| `check_placement` | `src/pipeline/placement_feasibility.rs` | 按序运行全部可行性规则，返回拦下它的那一条。 |
| `PAIR_PARALLEL_MIN` | `src/pipeline/placement_feasibility.rs` | 需要精确距离的颗粒对达到该数量时并行计算距离；默认 `usize::MAX`（串行），因为高密度端到端运行未见收益（共享 4 核主机上慢 0～10%），尽管 `pair_threshold_benchmark` 在两个以上无碰撞颗粒对时显示 1.4～2 倍。 |
| `pair_needs_exact_test` | `src/pipeline/placement_feasibility.rs` | 单个邻居的中心球与包围盒分离测试；需要精确测试时返回 true。 |
| `first_pair_rejection` | `src/pipeline/placement_feasibility.rs` | 先串行做相交/嵌套检查直到第一个失败对，再对其之前的颗粒对按序（`find_map_first`）并行计算距离；返回值与串行完全相同。 |
| `solid_pair_rejection` | `src/pipeline/placement_feasibility.rs` | 单个颗粒对的相交检查，其后是嵌套检查。 |
| `retained_depth` | `src/pipeline/placement_feasibility.rs` | 跨界颗粒仍伸入域内的深度。 |
| `ToolRecord` | `src/pipeline/placement_outputs.rs:15` | 记录与报告中出现的构建身份。 |
| `StopReason` | `src/pipeline/placement_outputs.rs:53` | 四个正常完成原因，外加用于已保存部分输出的 `Interrupted`。 |
| `ParticleRecord` | `src/pipeline/placement_outputs.rs:133` | 记录文件中单个已放置颗粒的条目。 |
| `RecordFile` | `src/pipeline/placement_outputs.rs:152` | 逐颗粒记录文件的顶层结构。 |
| `conventions` | `src/pipeline/placement_outputs.rs:173` | 写明读者复原颗粒所需的全部约定。 |
| `ReportFile` | `src/pipeline/placement_outputs.rs:246` | 运行报告文件的顶层结构。 |
| `SizeClassRow` | `src/pipeline/placement_outputs.rs:226` | 单个分组的目标、抽取、放置、缺口与补抽计数。 |
| `write_json` | `src/pipeline/placement_outputs.rs:380` | 把 JSON 值写入磁盘并创建父目录。 |
| `describe_output` | `src/pipeline/placement_outputs.rs:400` | 为报告清单描述已写出的输出文件。 |
| `write_size_distribution_csv` | `src/pipeline/placement_outputs.rs:424` | 写出逐尺寸分组的对照 CSV。 |
| `inverse_normal_cdf` | `src/pipeline/placement_sizes.rs` | Wichura AS241 标准正态分位数，误差低于 1e-15。 |
| `poly` (placement_sizes.rs) | `src/pipeline/placement_sizes.rs` | 对最高次在前的系数做 Horner 求值。 |
| `normal_cdf` (placement_sizes.rs) | `src/pipeline/placement_sizes.rs` | 经由互补误差函数计算标准正态 CDF。 |
| `erfc` | `src/pipeline/placement_sizes.rs` | 互补误差函数，用于把截断边界换算成概率。 |
| `SizeDraw` | `src/pipeline/placement_sizes.rs` | 一次抽取的直径，附带其统计分组与抽取序号。 |
| `SizeClass` | `src/pipeline/placement_sizes.rs` | 一个直径区间及其应占的颗粒份额。 |
| `SizeSource` | `src/pipeline/placement_sizes.rs` | 已就绪的目标数量分布：截断对数正态或直方图。 |
| `SizeSource::prepare` | `src/pipeline/placement_sizes.rs` | 准备分布源：读取直方图 CSV 并预先算好截断概率。 |
| `SizeSource::sample` | `src/pipeline/placement_sizes.rs` | 抽取一个直径，恰好消耗一个 u64。 |
| `SizeSource::support` | `src/pipeline/placement_sizes.rs` | 该分布源能产生的最小与最大直径。 |
| `SizeSource::mass_between` | `src/pipeline/placement_sizes.rs` | 目标分布落在两个直径之间的份额。 |
| `build_classes` | `src/pipeline/placement_sizes.rs` | 构造目标与实际对照所用的统计分组。 |
| `build_equal_width` | `src/pipeline/placement_sizes.rs` | 把分布支撑集切成等宽分组并给出各自份额。 |
| `class_for_diameter` | `src/pipeline/placement_sizes.rs` | 查找直径所属分组；最上端边界为闭区间。 |
| `SizePlan` | `src/pipeline/placement_sizes.rs` | 运行打算放置的尺寸集合，在任何放置之前抽定。 |
| `plan_size_multiset` | `src/pipeline/placement_sizes.rs` | 抽取整个尺寸集合，停在离目标更近的那个数量上。 |
| `order_for_placement` | `src/pipeline/placement_sizes.rs` | 把尺寸集合按从大到小排序，或还原为抽取顺序。 |
| `ShapeShell` | `src/pipeline/placement_library.rs` | 一个闭合壳：已度量、已居中、已计算摘要。 |
| `ShapeSource` | `src/pipeline/placement_library.rs` | 构成形状库的源文件，附带摘要与壳数统计。 |
| `RejectedShell` | `src/pipeline/placement_library.rs` | 读入但未保留的壳，以及未保留的原因。 |
| `ShapeLibrary` | `src/pipeline/placement_library.rs` | 运行可抽取的全部形状，以及读入但未保留的部分。 |
| `load_shape_library` | `src/pipeline/placement_library.rs` | 加载、拆分、度量并过滤形状文件。壳层数不少于 32（`LIBRARY_PARALLEL_MIN_SHELLS`）的文件并行准备各壳层，写入按序索引的缓冲后按壳层顺序遍历，因此顺序、拒绝与首个报告的缺陷都与串行扫描相同（§79）。 |
| `filter_reason` | `src/pipeline/placement_library.rs` | 指出某个壳未通过哪条形状库过滤规则。 |
| `shell_geometry_sha256` | `src/pipeline/placement_library.rs` | 对壳的几何计算摘要，使文件重排可被察觉。 |
| `particle_at_prepared` | `src/pipeline/placement_labels.rs` | 在有序的缓存候选中取第一个颗粒。 |
| `LABEL_SLAB_VOXELS` | `src/pipeline/placement_labels.rs` | 每个标签切片块的目标体素数（4,194,304）。 |
| `label_dims` | `src/pipeline/placement_labels.rs` | 标签网格尺寸，含溢出检查。 |
| `LabelQuery` | `src/pipeline/placement_labels.rs` | 所有切片块共享的颗粒查询、bbox 网格与 void。 |
| `LabelQuery::new` | `src/pipeline/placement_labels.rs` | 每次运行只准备一次查询上下文。 |
| `LabelQuery::centre` | `src/pipeline/placement_labels.rs` | 体素中心世界坐标。 |
| `LabelQuery::fill_slab` | `src/pipeline/placement_labels.rs` | 将一个 z 切片块分类写入 phase/id 缓冲。 |
| `write_label_stacks` | `src/pipeline/placement_labels.rs` | 按切片块流式写出两个标签 TIFF。 |

## 阅读顺序

整个引擎就是一个循环，其余部分都挂在它下面。`run_placement` 是入口，也是唯一写文件的函数；
`PlacementPipeline::run` 只是一层薄包装，使测试无需经由 stdout 就能驱动一次运行。

```
run_placement
├── load_shape_library          有哪些形状，已度量并居中
├── VoidIndex::build            冻结孔隙，已校验并建立索引
├── SizeSource::prepare         目标数量分布
├── build_classes               统计分组
├── plan_size_multiset          全部尺寸，在放置任何东西之前抽定
├── blank_report + write_json   报告，先以 `running` 写一次
├── place_resumable            主计划与裁剪补抽计划
│   └── try_place_one           逐尺寸：提出候选，然后检查
│       ├── draw_proposal       串行、固定消费顺序；记录流位置
│       └── evaluate_proposal   批内并行；只读
│           └── check_placement     全部规则，按固定顺序
├── decide_stop                 正常完成或中断
├── write_outputs               几何、记录、尺寸 CSV、孔隙副本、标签
└── finish_report + write_json  报告，再写一次，状态为 `finished`
```

## 读代码之前值得先读的四条契约

**RNG 消耗时序是固定的。** 每次尝试都在任何检查运行之前抽完全部随机数。若某个检查可能在抽数之前
短路，随机流就会依赖于哪个检查先触发，那么日后重排检查顺序就会悄然改变每一次放置。

**体积累加是顺序进行的。** 对 f64 做并行求和的结果依赖于归约树，因而依赖线程数，而这个数值正是决定
停止与否的那个数。报告输出的就是这同一个累加值，而不是对已放置集合重新求和，因此报告不会与它所描述
的那个决定自相矛盾。

**昂贵的工作被推迟到廉价拒绝之后。** parry 的 `TriMesh`、精确的域内体积、精确的两两距离，都只有在更
廉价的检查无法定论时才会执行。把它们写成即时计算，在一次小规模运行上代价是十六倍。

**投机尝试批次从不改变结果。** `try_place_one` 按固定消费顺序串行抽取一批候选，并记录每个候选之后的 ChaCha 流位置；然后对未改变的已放置集合并行评估，再按顺序检查结果：第一个接受之前的拒绝照常计数；一旦接受，就把生成器回退（`set_word_pos`）到该次尝试结束处，丢弃其后的全部抽样。每个颗粒的前 `SERIAL_ATTEMPTS_BEFORE_BATCHING`（4）次尝试逐个进行，之后批次加倍，上限为 worker 数的 `SPECULATIVE_BATCH_PER_WORKER`（8）倍；单 worker 从不批处理。体积分数 0.30 时 4/8 worker 的墙钟时间降至串行的 0.53/0.44 倍；0.10 时差异在噪声内。`a_dense_run_is_identical_on_one_two_and_eight_threads` 在每次放置约 175 次尝试的运行上逐字节比较 1/2/8 worker 的输出，去掉回退即失败。

**网格统计（PLAN.Performance.md@1349c46 §79 第 8 项）。** 放置结束后打印 `[GridStats] placement buckets=.. non_empty=.. max_occupancy=.. mean_occupancy_non_empty=.. memberships=.. items=..` 与 `[GridStats] placement queries grid_queries=.. grid_candidates=..`（并行评估中以 relaxed 原子量累计，包含投机评估，从不影响决定）。标签生成已在 `[Info] Voxel label queries` 行打印逐体素 bbox 检查数。VF 0.30 时网格 27 个桶、平均每桶 18 个颗粒（每次查询约 130 个候选）；把单元缩小到 1/2、1/4 使候选最多减少 2.8 倍而运行时间不变，因此单元大小（最大颗粒外延加间隙）保持不变。

**并行的颗粒对检查从不改变结果。** 廉价的邻居测试串行执行；随后相交与嵌套检查串行进行到第一个失败的
存活邻居为止，只有在它之前的颗粒对才需要代价占绝大部分的精确距离。当这样的颗粒对数量达到
`FeasibilityContext::pair_parallel_min`（默认 `PAIR_PARALLEL_MIN`）时，这些距离用有序的
`find_map_first` 并行计算，因此返回的原因始终是按邻居顺序和每对内部检查顺序的第一个，与串行扫描完全
相同。计数器仍只由串行调用方递增。间隙判定本身调用 `mesh_closer_than_prepared`，并告知碰撞已被排除：
不重复相交与嵌套检查，且先用有界的双层次结构筛查排除明显远于间隙的颗粒对，再测精确距离。在随仓库提供的
placement 配置上，整次运行加快 2.2～4.2 倍（体积分数 0.10～0.30，1 与 8 线程），记录、STL 与尺寸 CSV
逐字节一致。`forced_parallel_pair_checks_match_serial_attempt_by_attempt` 以固定
候选序列分别强制串行和强制并行，在 1/2/4/8 个 worker 下逐次比较。

**未达标是一种结果。** 什么也没放置的运行仍会写出报告、不写 STL、并以零码退出。只有配置不可用或输出
无法写入才是错误。

## 详细条目

下列每个函数与类型在源码中都带有 `AI-FUNC-SUMMARY` 注释，写明其用途、输入、返回、副作用及背后的假设。
本页不重复这些内容，而是为其建立索引；那些摘要才是契约，并与本文件保持同步。

最值得先读源码的几项，以及理由：

| 条目 | 理由 |
|---|---|
| `check_placement` | 全部放置规则集中一处、顺序固定，孔隙判据的完备性论证就写在它上方。 |
| `plan_size_multiset` | 最接近求和的停止规则，以及失败的尺寸绝不被替换的理由。 |
| `decide_stop` | 停止原因的优先级，包括为何"分布不可达"必须压过"预算耗尽"。 |
| `conventions` | 使用方复原一个颗粒所需的那些句子，直接写进记录本身，而不是留给文档。 |
| `VoidIndex::contains_point` | 为何内部判定采用射线奇偶而非伪法向测试。 |
| `mesh_volume_in_bbox_exact` | 为何逐平面封盖，以及旧版封盖错在哪里。 |

## 输出模式（schema）

| 文件 | Schema | 写出者 |
|---|---|---|
| `particles.json` | `rustmspt.placement.record/1` | `RecordFile`、`write_json` |
| `run_report.json` | `rustmspt.placement.report/1` | `ReportFile`、`write_json` |
| `size_distribution.csv` | 表头行，十列 | `write_size_distribution_csv` |
| `voxel_labels/voxel_labels.json` | `rustmspt.placement.voxel_labels/1` | `VoxelLabelsHeader` |

`StopReason` 包括 `target_reached`、`budget_exhausted`、
`distribution_unattainable`、`no_feasible_placement` 与 `interrupted`。使用方
必须把 `interrupted` 识别为已保存的部分输出，绝不能当作目标达成。

## 交叉说明

- 生产环境中的 `PlacedParticle` 保留共享的源几何与不可变变换，独立的 `mesh`/`shape` 字段为空。`prepared()` 通过全运行范围、
  有界的缓存固定精确的世界几何；独立的测试夹具保留直接字段。`EngineState` 统计三角形数量，而不累积合并后的网格。
- `RejectReason::ALL` 是报告的输出顺序，读起来像一个漏斗。精确裁剪检查出于开销考虑被安排在邻居检查
  *之后* 求值；而计数的排列顺序是文档所述的规则顺序。
- `particle_overlap` 与 `particle_enclosed` 是两种不同的失败，而后者正是 v0.2.0 所缺的那一种。表面
  相交给出前者；一个颗粒整体位于另一个内部给出后者——而该布局下两个表面从不相交，间隙判定又会把两者
  之间的空间读作间隙。孔隙那一支从一开始就具备论证的两半，颗粒这一支却没有；这正是某次运行把 146 个
  颗粒中的 28 个放进另一个颗粒内部、却报告 `target_reached` 的原因。参见
  [void-aware-placement.md](../algorithms/void-aware-placement.md#61-the-predicate-and-why-it-is-complete) 第 6.1 节。
- `VoidIndex` 的 `Debug` 实现刻意精简：网格及其层次结构会刷满一屏，却说不出读者想要的任何信息。
- 报告在 `outputs` 中列出自身，摘要为 null。文件无法包含自身的哈希；若不如此，逐一校验清单中每个
  摘要的适配器就会在唯一那个不可能有摘要的条目上卡住。

### run_placement / run_placement_in_pool — PERF-02

`run_placement(config: &ResolvedPlacement) -> Result<PlacementOutcome>` 使用 `resolve_threads`
创建专用 Rayon 线程池，并在池中执行私有
`run_placement_in_pool(config: &ResolvedPlacement) -> Result<PlacementOutcome>`，覆盖源库准备、
几何计算、void 查询与标签输出。创建线程池失败时，在读写文件前返回 `InvalidConfig`。
内部执行函数保持随机数消费和体积累计串行，并用 `rayon::current_num_threads()` 记录实际池大小。
因此 `runtime.threads` 是执行现场观测值。正数线程配置按原值执行，非正值采用可用并行度。
本改动修复资源契约，接受循环并行化仍为后续工作。

`with_placement_pool<T: Send>(threads: i32, work: impl FnOnce() -> Result<T> + Send) -> Result<T>`
是私有执行封装：启动并回收 worker，返回操作结果。单元测试在 1/2/8 worker 的嵌套 `par_iter`
内部观察池大小和 worker 索引，与标签输出集成测试共同验证执行契约。

### Label preparation (PERF-12)

`voxel_labels` 一次性建立有界空间网格，并仅使用各分块固定的候选为其准备精确的网格查询。并行的 1024 体素分块对其外包盒只查询一次，对候选切片下标排序，并保留原有的颗粒归属顺序与孔隙优先的分类。工作线程的暂存区保留射线命中。`particle_at_prepared` 返回第一个接受的 id 与 bbox 检查数，归约时不使用共享原子量。仅用于测试的原始颗粒扫描是差分基准。输出的 phase/id 数组与文件 schema 不变；输出现为按切片块流式写出（见下）。

### 标签按切片块流式输出（PERF-12，2026-09-25）

`write_voxel_labels` 不再整体分配 phase 与 particle-id 两个体数据。`write_label_stacks` 打开两个 TIFF，只准备一次 `LabelQuery`，按每块 `max(1, LABEL_SLAB_VOXELS / (nx*ny))` 个切片循环：用 `fill_slab` 填充两个复用的切片块缓冲（仍为 1024 体素 tile、void 优先、最小候选下标优先），再经 `TiffPageEncoder`（即 `save_tiff_or_folder` 使用的逐页循环）追加。标签峰值内存为两个切片块缓冲（大切片时最多约 64 MiB 的 `i64`；单个切片已超过目标时每块一个切片），而非 `2 * 8 * nx*ny*nz` 字节。文件字节、header、spacing/origin 及清单顺序不变：`slab_label_stacks_are_byte_identical_to_whole_volume_output` 在 28x20x30 网格、void 与颗粒重叠的场景下，将切片块大小 1、2、3、4、7、29、30、31、1000 的输出与整卷 `Volume3D` + `save_tiff_or_folder` 参考逐字节比较；1/2/8 worker 的 placement 标签测试仍通过。输出的 `bbox_tests` 同时报告 `slab_slices`；切片块边界未对齐 1024 体素时 tile 划分及该诊断计数可能与整卷略有不同。中途出错时两个 TIFF 可能只写了一部分。

形状库输入现在使用 `load_stl_hashed`，使几何与源摘要来自同一字节流。二进制原始文件的缓冲是有界的；ASCII 则从同一流中逐行解析。源/壳的顺序与摘要不变。

### 阶段计时（PERF-00）

`run_placement_in_pool` 向 stdout 打印 `[Timing] placement stage=<load|plan|place|write_outputs|report|total_in_pool> seconds=<f>`、`[Timing] placement workers=<n>` 和 `[Timing] placement peak_rss_bytes=<n|unavailable>`。这些行从不写入记录、报告或 CSV，因此跨线程数的逐字节输出比较不受影响；报告自身的 `elapsed` 字段不变。


## Cooperative stop and partial-result preservation

对于带 `placement:` 配置的 `pack`，创建 `<outputs.dir>/STOP` 即可请求可移植的协作式停止。在 Unix 上，CLI 还会处理
SIGINT（Ctrl-C）和 SIGTERM。处理程序只设置一个原子标志；几何与文件 I/O 留在处理程序之外。重复请求仍允许保存。SIGKILL、崩溃和断电不会触发最终保存；恢复时使用
最近一次成功提交的检查点（若已启用）。

引擎在提议批次之间（以及颗粒/补抽批次之间）观察取消，完成已在运行的几何查询，并使用常规输出写入器导出每一个已接受的颗粒。被中断的候选
不计为已证实的尺寸失败。取消之后不再启动替换尺寸或新的补抽批次。加载/规划与输出写入不会被抢占；请等待最终报告，因为复杂查询和大型
STL 文件可能耗时较长。

`particles.stl`（非空时）、`particles.json`、`size_distribution.csv`、冻结孔隙副本以及已配置的可选输出，描述的是已接受的部分结果。
最终报告带有 `status: interrupted` 与 `stop_reason: interrupted`；仅在输出保存成功之后才写出。I/O 失败作为错误传播，
而不是作为成功保存的中断。在任何接受之前收到请求，会写出空的记录/报告而不带颗粒 STL。退出码为零表示输出已保存，
并不表示目标已达成。原有的四个正常停止原因保持其行为。使用方必须接受额外的中断原因。CSV
中的缺口是规划数减去已放置数，因此在部分运行中包含未尝试的尺寸；`stop_detail.failed_sizes` 则单独统计已耗尽尝试的尺寸。

`progress.json` 在开始时、首次接受时、大约每 10 秒的批次边界处、保存之前以及保存成功之后被原子替换。
它报告数量、尝试次数、已用时间以及在所配置基准上的体积分数；原始 VF 并不是独立的孔隙筛查认证。同一份摘要
也打印到 stderr。单次长查询可能推迟心跳。进度 I/O
错误只发出警告，不会丢弃打包结果。STOP 文件轮询在边界处被限制为 250 ms 一次；信号轮询在每个边界都进行。新运行之前请删除
STOP。若要保留旧结果，请使用新的输出目录。

此功能保存一个部分装配体，并默认在导出几何之前保存可恢复的 RNG/引擎
检查点。它不改变旧版 `packing:` 引擎。CLI 信号处理程序
在返回时被恢复；进程内调用方使用各输出目录的 STOP 文件，且不安装进程全局处理程序。正常的打包顺序、RNG 流与几何
检查均不变。

### Control function contracts

| 条目 | 位置 | 契约 |
|---|---|---|
| `SignalGuard::install` | `src/pipeline/placement_control.rs` | 仅限 CLI 的 Unix 信号设置；在 drop 时恢复先前的处置方式；返回操作系统错误。 |
| `request_stop` | `src/pipeline/placement_control.rs` | 信号处理程序只设置一个无锁原子标志。 |
| `SignalGuard::drop` | `src/pipeline/placement_control.rs` | 恢复先前的信号处理程序。 |
| `PlacementControl::new` | `src/pipeline/placement_control.rs` | 初始化每次运行的取消与进度时钟。 |
| `PlacementControl::poll` | `src/pipeline/placement_control.rs` | 锁存停止请求并定期发布进度；不触碰 RNG 或几何。 |
| `PlacementControl::publish` | `src/pipeline/placement_control.rs` | 原子替换进度 JSON 并发出 stderr 心跳；遥测 I/O 失败时发出警告。 |
| `EngineState::poll_control` | `src/pipeline/placement.rs` | 在安全边界处向控制层提供计数/尝试次数/VF。 |

`run_placement` 现在在协作式停止时写出部分输出。`decide_stop`
赋予中断优先权，`finish_report` 区分被中断与已完成。`place_resumable` 与 `try_place_one` 在边界处检查控制；
未被接受的被中断候选不会增加已耗尽尺寸的计数。


## Optional aggregate path

仅当已校验的聚集体功能启用时，`run_placement_in_pool` 才委托给 `aggregates::run`。共享的 `accept`、`write_outputs` 与 `finish_report`
仍对单个颗粒操作。空计划的 `EngineState::new` 使用一个安全的
单一空间单元。见[聚集体函数契约](pipeline-aggregates.md)。
`SizeSource::sample` 现在调用确定性的 `SizeSource::quantile(u01(rng))`，
保留其原有的随机流与逆 CDF 算术。

## Checkpoint and cache contracts

见[设计文档](../algorithms/placement-checkpoints-and-memory.md)与
[函数契约](pipeline-placement-state.md)。`place_resumable` 以一个主计划/补抽状态机取代
`place_all` / `run_top_up`。`try_place_one`
返回 `Result<bool>`，并在安全点持久化每次抽取的尝试消耗；
检查点 I/O 失败会向上传播，而不是声称已保存状态。原有的
几何/RNG 顺序保持不变，除了已修正的孔隙包含判定缺陷，以及全局预算失败不再错误地耗尽某个尺寸。


`run_placement` 可选地在规划缺失的材料体积之前校验一个 `initial_particles` 装配体。保留继承的顺序与几何，
将其计入计划/报告核算，并且只放置新的抽取。这会开始一个
新任务；普通的检查点恢复仍会检查可执行文件的精确身份。
