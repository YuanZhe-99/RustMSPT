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
| `VoidIndex::contains_point` | `src/geometry/void_index.rs:150` | 在层次结构上做射线奇偶判定；对嵌套壳同样正确。 |
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
| `PlacementPipeline` | `src/pipeline/placement.rs:38` | 持有已校验 `ResolvedPlacement` 的流水线结构体。 |
| `PHASE_MATRIX` | `src/pipeline/placement_labels.rs:13` | 标签场中相编码 0。 |
| `VoxelLabelsHeader` | `src/pipeline/placement_labels.rs:22` | 标签体数据的说明：间距、原点、排布与相表。 |
| `PhaseLabel` | `src/pipeline/placement_labels.rs:39` | 一个相编码及其名称。 |
| `write_voxel_labels` | `src/pipeline/placement_labels.rs:60` | 写出三相标签场与逐体素颗粒标识场。 |
| `particle_at` | `src/pipeline/placement_labels.rs:306` | 查找包含某点的已放置颗粒。 |
| `point_in_particle` | `src/pipeline/placement_labels.rs:318` | 对单个颗粒网格做射线奇偶包含判定。 |
| `VoidReport` | `src/pipeline/placement_outputs.rs:278` | 运行如何处理冻结孔隙，以及如何度量它。 |
| `build_void_report` | `src/pipeline/placement.rs:1170` | 为报告描述冻结孔隙，含其体积计算方法。 |
| `PlacementPipeline` | `src/pipeline/placement.rs:38` | 持有已校验 `ResolvedPlacement` 的流水线结构体。 |
| `PlacementOutcome` | `src/pipeline/placement.rs:61` | 一次完整运行的产出，供进程内调用方使用。 |
| `run_placement` | `src/pipeline/placement.rs:76` | 在按配置创建的专用 Rayon 线程池中运行引擎并写出全部输出。 |
| `with_placement_pool` | `src/pipeline/placement.rs:81` | 为一次操作创建并安装专用 Rayon 线程池，传播创建及执行错误。 |
| `run_placement_in_pool` | `src/pipeline/placement.rs:90` | 在当前线程池内执行所有 placement 阶段，并记录实际 worker 数。 |
| `resolve_threads` | `src/pipeline/placement.rs:267` | 把线程设置换算为至少为 1 的工作线程数。 |
| `EngineState` | `src/pipeline/placement.rs:279` | 放置循环累积的全部状态。 |
| `place_all` | `src/pipeline/placement.rs:361` | 按顺序尝试每个已规划尺寸，接受放得下的。 |
| `try_place_one` | `src/pipeline/placement.rs:400` | 在单颗粒尝试预算内尝试放置一个尺寸。 |
| `accept` | `src/pipeline/placement.rs:558` | 把已接受的候选提交进几何与记录。 |
| `run_top_up` | `src/pipeline/placement.rs:619` | 仅因裁剪而未达标时补抽新批次。 |
| `decide_stop` | `src/pipeline/placement.rs:802` | 判定运行以四种停止原因中的哪一种结束。 |
| `write_outputs` | `src/pipeline/placement.rs:802` | 写出几何、逐颗粒记录与尺寸 CSV。 |
| `entity_id` | `src/pipeline/placement.rs:947` | 已放置颗粒的稳定标识。 |
| `particle_record` | `src/pipeline/placement.rs:952` | 把一个已放置颗粒转为其记录条目。 |
| `size_class_rows` | `src/pipeline/placement.rs:1000` | 构造逐分组的目标与实际对照行。 |
| `blank_report` | `src/pipeline/placement.rs:1026` | 放置开始之前的报告初始形态。 |
| `describe_input` | `src/pipeline/placement.rs:1205` | 为报告描述输入文件及其摘要。 |
| `finish_report` | `src/pipeline/placement.rs:1224` | 填入运行结束后已知的全部内容。 |
| `summary` (placement.rs) | `src/pipeline/placement.rs:1288` | 构造供人阅读的 stdout 摘要。 |
| `read_record` | `src/pipeline/placement.rs:1349` | 读回已写出的逐颗粒记录。 |
| `read_report` | `src/pipeline/placement.rs:1357` | 读回已写出的运行报告。 |
| `RejectReason` | `src/pipeline/placement_feasibility.rs:17` | 候选放置未被接受的原因；即报告中的键。 |
| `RejectReason::as_str` | `src/pipeline/placement_feasibility.rs:44` | 拒绝原因在报告中的稳定键名。 |
| `PlacedParticle` | `src/pipeline/placement_feasibility.rs:76` | 通过全部检查的颗粒，附带缓存的形状。 |
| `PlacedParticle::volume_in_domain_solid` | `src/pipeline/placement_feasibility.rs:106` | 计入固相的颗粒体积。 |
| `FeasibilityContext` | `src/pipeline/placement_feasibility.rs:112` | 可行性检查所读取的全部内容。 |
| `Candidate` | `src/pipeline/placement_feasibility.rs:132` | 候选放置，附带已预先算好的廉价量。 |
| `Accepted` | `src/pipeline/placement_feasibility.rs:146` | 通过检查过程中顺带算出的结果。 |
| `check_placement` | `src/pipeline/placement_feasibility.rs:171` | 按序运行全部可行性规则，返回拦下它的那一条。 |
| `PAIR_PARALLEL_MIN` | `src/pipeline/placement_feasibility.rs:15` | 需要精确距离的颗粒对达到该数量时并行计算距离；默认 `usize::MAX`（串行），因为高密度端到端运行未见收益（共享 4 核主机上慢 0～10%），尽管 `pair_threshold_benchmark` 在两个以上无碰撞颗粒对时显示 1.4～2 倍。 |
| `pair_needs_exact_test` | `src/pipeline/placement_feasibility.rs` | 单个邻居的中心球与包围盒分离测试；需要精确测试时返回 true。 |
| `first_pair_rejection` | `src/pipeline/placement_feasibility.rs` | 先串行做相交/嵌套检查直到第一个失败对，再对其之前的颗粒对按序（`find_map_first`）并行计算距离；返回值与串行完全相同。 |
| `solid_pair_rejection` | `src/pipeline/placement_feasibility.rs` | 单个颗粒对的相交检查，其后是嵌套检查。 |
| `retained_depth` | `src/pipeline/placement_feasibility.rs:382` | 跨界颗粒仍伸入域内的深度。 |
| `ToolRecord` | `src/pipeline/placement_outputs.rs:15` | 记录与报告中出现的构建身份。 |
| `StopReason` | `src/pipeline/placement_outputs.rs:53` | 运行可用的四词固定停止原因词表。 |
| `ParticleRecord` | `src/pipeline/placement_outputs.rs:133` | 记录文件中单个已放置颗粒的条目。 |
| `RecordFile` | `src/pipeline/placement_outputs.rs:152` | 逐颗粒记录文件的顶层结构。 |
| `conventions` | `src/pipeline/placement_outputs.rs:173` | 写明读者复原颗粒所需的全部约定。 |
| `ReportFile` | `src/pipeline/placement_outputs.rs:246` | 运行报告文件的顶层结构。 |
| `SizeClassRow` | `src/pipeline/placement_outputs.rs:226` | 单个分组的目标、抽取、放置、缺口与补抽计数。 |
| `write_json` | `src/pipeline/placement_outputs.rs:380` | 把 JSON 值写入磁盘并创建父目录。 |
| `describe_output` | `src/pipeline/placement_outputs.rs:400` | 为报告清单描述已写出的输出文件。 |
| `write_size_distribution_csv` | `src/pipeline/placement_outputs.rs:424` | 写出逐尺寸分组的对照 CSV。 |
| `inverse_normal_cdf` | `src/pipeline/placement_sizes.rs:21` | Wichura AS241 标准正态分位数，误差低于 1e-15。 |
| `poly` (placement_sizes.rs) | `src/pipeline/placement_sizes.rs:133` | 对最高次在前的系数做 Horner 求值。 |
| `normal_cdf` (placement_sizes.rs) | `src/pipeline/placement_sizes.rs:142` | 经由互补误差函数计算标准正态 CDF。 |
| `erfc` | `src/pipeline/placement_sizes.rs:153` | 互补误差函数，用于把截断边界换算成概率。 |
| `SizeDraw` | `src/pipeline/placement_sizes.rs:175` | 一次抽取的直径，附带其统计分组与抽取序号。 |
| `SizeClass` | `src/pipeline/placement_sizes.rs:185` | 一个直径区间及其应占的颗粒份额。 |
| `SizeSource` | `src/pipeline/placement_sizes.rs:192` | 已就绪的目标数量分布：截断对数正态或直方图。 |
| `SizeSource::prepare` | `src/pipeline/placement_sizes.rs:218` | 准备分布源：读取直方图 CSV 并预先算好截断概率。 |
| `SizeSource::sample` | `src/pipeline/placement_sizes.rs:272` | 抽取一个直径，恰好消耗一个 u64。 |
| `SizeSource::support` | `src/pipeline/placement_sizes.rs:309` | 该分布源能产生的最小与最大直径。 |
| `SizeSource::mass_between` | `src/pipeline/placement_sizes.rs:381` | 目标分布落在两个直径之间的份额。 |
| `build_classes` | `src/pipeline/placement_sizes.rs:329` | 构造目标与实际对照所用的统计分组。 |
| `build_equal_width` | `src/pipeline/placement_sizes.rs:355` | 把分布支撑集切成等宽分组并给出各自份额。 |
| `class_for_diameter` | `src/pipeline/placement_sizes.rs:430` | 查找直径所属分组；最上端边界为闭区间。 |
| `SizePlan` | `src/pipeline/placement_sizes.rs:449` | 运行打算放置的尺寸集合，在任何放置之前抽定。 |
| `plan_size_multiset` | `src/pipeline/placement_sizes.rs:473` | 抽取整个尺寸集合，停在离目标更近的那个数量上。 |
| `order_for_placement` | `src/pipeline/placement_sizes.rs:534` | 把尺寸集合按从大到小排序，或还原为抽取顺序。 |
| `ShapeShell` | `src/pipeline/placement_library.rs:13` | 一个闭合壳：已度量、已居中、已计算摘要。 |
| `ShapeSource` | `src/pipeline/placement_library.rs:41` | 构成形状库的源文件，附带摘要与壳数统计。 |
| `RejectedShell` | `src/pipeline/placement_library.rs:53` | 读入但未保留的壳，以及未保留的原因。 |
| `ShapeLibrary` | `src/pipeline/placement_library.rs:61` | 运行可抽取的全部形状，以及读入但未保留的部分。 |
| `load_shape_library` | `src/pipeline/placement_library.rs:85` | 加载、拆分、度量并过滤形状文件。 |
| `filter_reason` | `src/pipeline/placement_library.rs:234` | 指出某个壳未通过哪条形状库过滤规则。 |
| `shell_geometry_sha256` | `src/pipeline/placement_library.rs:274` | 对壳的几何计算摘要，使文件重排可被察觉。 |
| `particle_at_prepared` | `src/pipeline/placement_labels.rs:326` | First particle in ordered cached candidates. |
| `LABEL_SLAB_VOXELS` | `src/pipeline/placement_labels.rs:46` | 每个标签切片块的目标体素数（4,194,304）。 |
| `label_dims` | `src/pipeline/placement_labels.rs:111` | 标签网格尺寸，含溢出检查。 |
| `LabelQuery` | `src/pipeline/placement_labels.rs:124` | 所有切片块共享的颗粒查询、bbox 网格与 void。 |
| `LabelQuery::new` | `src/pipeline/placement_labels.rs:136` | 每次运行只准备一次查询上下文。 |
| `LabelQuery::centre` | `src/pipeline/placement_labels.rs:170` | 体素中心世界坐标。 |
| `LabelQuery::fill_slab` | `src/pipeline/placement_labels.rs:184` | 将一个 z 切片块分类写入 phase/id 缓冲。 |
| `write_label_stacks` | `src/pipeline/placement_labels.rs:257` | 按切片块流式写出两个标签 TIFF。 |

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
├── place_all
│   └── try_place_one           逐尺寸：提出候选，然后检查
│       └── check_placement     全部规则，按固定顺序
├── run_top_up                  仅当没有任何失败时
├── decide_stop                 四个词中的哪一个
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

`StopReason` 是固定的四词词表：`target_reached`、`budget_exhausted`、`distribution_unattainable`、
`no_feasible_placement`。使用方的适配器正是按这份清单编写的，因此绝不能输出第五个取值。

## 交叉说明

- `PlacedParticle` 会保留一次构建好的 parry `TriMesh`，因此已接受的颗粒不会被反复重建。原引擎每次
  比较都重建一个，这正是其碰撞循环的主要开销。
- `RejectReason::ALL` 是报告的输出顺序，读起来像一个漏斗。精确裁剪检查出于开销考虑被安排在邻居检查
  *之后* 求值；而计数的排列顺序是文档所述的规则顺序。
- `particle_overlap` 与 `particle_enclosed` 是两种不同的失败，而后者正是 v0.2.0 所缺的那一种。表面
  相交给出前者；一个颗粒整体位于另一个内部给出后者——而该布局下两个表面从不相交，间隙判定又会把两者
  之间的空间读作间隙。孔隙那一支从一开始就具备论证的两半，颗粒这一支却没有；这正是某次运行把 146 个
  颗粒中的 28 个放进另一个颗粒内部、却报告 `target_reached` 的原因。参见
  [void-aware-placement.md](../algorithms/void-aware-placement.md) 第 6.1 节。
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

`voxel_labels` prepares immutable per-particle mesh queries and a bounded spatial grid once. Parallel 1024-voxel tiles query their enclosing box once, sort candidate slice indices, and preserve original particle ownership order and void-first classification. Worker scratch retains ray hits. `particle_at_prepared` returns the first acceptance id and bbox-test count, reduced without shared atomics. The test-only original particle scan is the differential oracle. Output phase/id arrays and file schema are unchanged; output is now slab-streamed (below).

### 标签按切片块流式输出（PERF-12，2026-09-25）

`write_voxel_labels` 不再整体分配 phase 与 particle-id 两个体数据。`write_label_stacks` 打开两个 TIFF，只准备一次 `LabelQuery`，按每块 `max(1, LABEL_SLAB_VOXELS / (nx*ny))` 个切片循环：用 `fill_slab` 填充两个复用的切片块缓冲（仍为 1024 体素 tile、void 优先、最小候选下标优先），再经 `TiffPageEncoder`（即 `save_tiff_or_folder` 使用的逐页循环）追加。标签峰值内存为两个切片块缓冲（大切片时最多约 64 MiB 的 `i64`；单个切片已超过目标时每块一个切片），而非 `2 * 8 * nx*ny*nz` 字节。文件字节、header、spacing/origin 及清单顺序不变：`slab_label_stacks_are_byte_identical_to_whole_volume_output` 在 28x20x30 网格、void 与颗粒重叠的场景下，将切片块大小 1、2、3、4、7、29、30、31、1000 的输出与整卷 `Volume3D` + `save_tiff_or_folder` 参考逐字节比较；1/2/8 worker 的 placement 标签测试仍通过。输出的 `bbox_tests` 同时报告 `slab_slices`；切片块边界未对齐 1024 体素时 tile 划分及该诊断计数可能与整卷略有不同。中途出错时两个 TIFF 可能只写了一部分。

Shape library input now uses `load_stl_hashed` so geometry and source digest come from one byte stream. Binary raw-file buffering is bounded; ASCII is parsed line by line from the same stream. Source/shell order and digests are unchanged.

### 阶段计时（PERF-00）

`run_placement_in_pool` 向 stdout 打印 `[Timing] placement stage=<load|plan|place|write_outputs|report|total_in_pool> seconds=<f>`、`[Timing] placement workers=<n>` 和 `[Timing] placement peak_rss_bytes=<n|unavailable>`。这些行从不写入记录、报告或 CSV，因此跨线程数的逐字节输出比较不受影响；报告自身的 `elapsed` 字段不变。
