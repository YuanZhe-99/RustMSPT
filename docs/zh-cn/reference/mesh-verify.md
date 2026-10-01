# 网格验证 —— 检查目录与 `mesh-verify` 流水线

网格生成工具阶段验证切片的参考文档（`PLAN_mesh_generation.md` 阶段 GA，子任务
GA-2）：精确谓词与单元质量原语（`src/meshgen/predicates.rs`）、检查目录
（`src/meshgen/verify.rs`）以及 `mesh-verify` 子命令
（`src/pipeline/mesh_verify.rs`、`src/config/mesh_verify.rs`）。

权威契约是 [`SPEC_meshgen_contracts.md`](../../../SPEC_meshgen_contracts.md)
§4（目录、严重级别、门限）与 §4.1（JSON 模式）。检查代码与 JSON 结构已冻结：
测试直接断言它们。

## 索引

| 条目 | 源码位置 | 概述 |
|---|---|---|
| `orient3d` | `src/meshgen/predicates.rs:843` | 符号精确的四面体定向行列式 `det[b-a, c-a, d-a]`；全项目**唯一**对 `robust::orient3d` 相反符号约定取负的位置（规则 N10）。 |
| `tet_signed_volume` | `src/meshgen/predicates.rs:853` | `orient3d/6`；正向四面体为正值。 |
| `orient3d_sign_test` | `src/meshgen/predicates.rs:858` | 固定定向约定的基准用例：单位四面体必须为正。 |
| `TetQuality` | `src/meshgen/predicates.rs:874` | 体积、纵横比、半径比、最大/最小二面角（角度**与**余弦）、缩放雅可比、最小高。 |
| `tet_quality` | `src/meshgen/predicates.rs:893` | 计算单个四面体的 [V4] 指标；返回余弦值使角度门限保持代数化（判定路径中不出现 `acos`）。 |
| `node_key` | `src/meshgen/predicates.rs:1012` | 尺度相关网格上的量化整数节点键；键相同即同一节点。 |
| `Severity` | `src/meshgen/verify.rs:21` | 发现项严重级别 `Info < Warn < Fail`，采用日志与 JSON 的契约拼写。 |
| `CheckStatus` | `src/meshgen/verify.rs:40` | 单节结论：`PASS`/`WARN`/`FAIL`/`SKIPPED`；跳过时必定附带原因。 |
| `VerifyItem` | `src/meshgen/verify.rs:63` | 单条发现：严重级别、稳定的 `code`、消息、点/单元编号与坐标（供 `mesh-render --highlight-from` 使用）。 |
| `VerifySection` | `src/meshgen/verify.rs:88` | 单个目录条目：状态、具名指标、**不受上限影响**的 `fail_count`/`warn_count`/`info_count` 与 `fired` 代码集合，以及限长条目列表（`items_truncated`）。`push` 统计每一条发现，只保留最严重的 `cap` 条（契约 §4.3）；`finish` 按契约顺序排序。 |
| `VerifyGates` | `src/meshgen/verify.rs:200` | 可配置门限；所有长度容差均为包围盒对角线的比例，因此门限与尺度无关。 |
| `VerifyReport` | `src/meshgen/verify.rs:254` | 完整结果与元数据回显；提供 `passed`、`exit_code`、`fired_codes`、`section` 访问器。 |
| `VerifyOptions` | `src/meshgen/verify.rs:769` | 文档之外的验证器输入：`expected_stage`（从快照文件名解析）用于 [V12] 交叉校验；`delivered`（无论 `StageIndex` 为何都按区域盒严格检查，契约 §4.4）；以及 [V5]/[V13] 的 `surfaces`。 |
| `verify` | `src/meshgen/verify.rs:798` | 对契约 VTU 或外部 VTU 运行检查目录；按契约顺序为每个条目返回一节。 |
| `verify_with_options` | `src/meshgen/verify.rs:809` | 带文档之外选项的 `verify`；面阶段 s00-s03 会跳过仅适用于体网格的 [V7]/[V8]/[V13]。 |
| `BoundaryFace` | `src/meshgen/verify.rs:4135` | [V13] 眼中的一个材料边界面：面积、局部边长、到该分量曲面的平均/最大 \|距离\| 与**带符号**偏移。 |
| `FidelityAcc` | `src/meshgen/verify.rs:4167` | [V13] 的逐分量累加器；所有求和均按面积加权，因此粗面不会仅凭「同样计一次」压过细面。 |
| `absorb` | `src/meshgen/verify.rs:4184` | 将一个 `BoundaryFace` 折叠进 `FidelityAcc`。 |
| `check_v13` | `src/meshgen/verify.rs:4238` | [V13] 界面保真度：从体网格读出材料边界（区域集对区域集，绝不查看面标签），并与输入曲面比对。 |
| `strict_domain_box` | `src/meshgen/verify.rs:1343` | [V3] 中自由面检查看不到的两项区域盒条件：每个被使用的节点都在 `[DomainMin, DomainMax]` 内（`V3.outside_domain`），四面体总体积等于区域盒体积（`V3.box_volume`）。凡不允许修剪前外壳放宽之处都运行。 |
| `ContractStrength` / `contract_strength` | `src/meshgen/contract.rs:13/74` | §4.2 的校验强度，仅由元数据决定：仅几何（无 `SchemaVersion`）、主体网格（仅四面体且有 `region_key`）、混合契约（其余声明 `SchemaVersion = 1` 的文档）。 |
| `validate_contract` | `src/meshgen/contract.rs:169` | 按该强度校验文档；返回每条被违反的规则（`presence`、`type`、`counts`、`sentinel`、`key_range`、`set_offsets`、`table_length`、`component`、`sheet_volume`、`orientation_length`、`side_elems`、`cell_kind`、`cell_type`）。[V12] 将每条报告为 `V12.contract` FAIL。 |
| `report_to_json` | `src/meshgen/verify.rs:2603` | 序列化冻结的 JSON 报告（手写实现；项目不引入 JSON 依赖）。 |
| `report_to_log` | `src/meshgen/verify.rs:2712` | 分节的人读日志，每项检查一行 `[PASS]/[WARN]/[FAIL]/[SKIP]`，末尾附汇总。 |
| `annotate` | `src/meshgen/verify.rs:2774` | 附带质量数组与 `verify_flags` 位掩码的网格副本（第 *k* 位对应 `[V(k+1)]`）。 |
| `VerifyGateParams`/`MeshVerifyParams`/`MeshVerifyConfig` | `src/config/mesh_verify.rs:8` | `mesh_verify:` YAML 块：输入、report/json/annotate 输出路径、门限覆盖。 |
| `gates_from_config` | `src/pipeline/mesh_verify.rs:21` | 将 YAML 覆盖项叠加到契约默认门限上。 |
| `verify_file` | `src/pipeline/mesh_verify.rs:53` | 加载 → 校验 → 验证 → 写出日志/JSON/带注解 VTU；返回报告。`delivered`（来自 YAML `delivered: true`）设置 `VerifyOptions::delivered`。 |
| `MeshVerifyPipeline` | `src/pipeline/mesh_verify.rs:12` | `mesh-verify` 子命令；门限不通过时返回错误（非零退出码）。 |

## 当前实现范围

目录始终报告全部**十三**节。凡是所需数据当前文档并不携带的检查，都会以
`SKIPPED` 状态报告，并指明缺失的数组或尚未落地的生产阶段 —— 报告永远不会
静默省略某项检查。

| 检查 | 状态 | 说明 |
|---|---|---|
| [V1] 单元 | **完整** | 非正体积（精确谓词）、单元内重复节点、重复单元、非有限坐标 |
| [V2] 节点 | **完整** | 量化键判定的重合重复节点（FAIL）、未被引用节点（WARN） |
| [V3] 协调性 | **完整** | 面的相邻四面体数 ≠2、边界泄漏（未打标签且不在任何域平面上的面）、悬挂节点（空间哈希加速）、非流形边（WARN），以及**折叠面**（`V3.folded_face`：共享一个面的两个四面体位于该面的同一侧——二者重叠，[V1] 与面计数都看不到；A-3 在 `0a8eb1c` 上有 466 个）。**区域盒（契约 §4.4）：** 只有在 `StageIndex` 5–8、且未以 `delivered: true` 验证时，超出区域盒的网格才可把自身外壳当作外部（INFO `V3.pre_trim_overhang` 指明阶段）。其余情况下，每个被使用的节点都必须在盒内（`V3.outside_domain`），四面体必须填满区域盒（`V3.box_volume`，1%）。边界泄漏规则在 5–7 阶段仍被推迟（INFO `V3.deferred`）；面共享、悬挂节点、折叠与流形性在任何阶段都是 FAIL。 |
| [V4] 质量 | **完整** | 纵横比、半径比、二面角极值、缩放雅可比、最小高；分布、最差 10 个、三项门限 |
| [V5] 几何一致性 | **设置 `surfaces:` 后完整** | 双向曲面贴合度（网格界面到输入曲面，以及输入曲面回到界面）与逐分量体积。未提供输入曲面时报告 SKIPPED 并说明原因——此时没有可供一致性比对的对象。这里有两个分母各自出错过一次，值得记住：贴合容差是**局部界面边长**的比例，而非包围盒对角线的比例；`volume_expected` 是**优先级裁决后**的体积——即该分量中未被更高优先级实体覆盖的部分——而非其原始输入体积。 |
| [V6] ID 语义 | **部分** | 区域键合法性、同一键单一优先级、区域邻接，以及**未声明的材料边界**（`V6.undeclared_boundary`，自计划 M-1.0 起为 FAIL：进入或离开某分量的面必须携带该分量的标签）。在切割前的 5–7 阶段尚无界面，此 FAIL 被推迟，但计数仍会报告。面按键序遍历，因此保留的条目与 `undeclared_boundary_area` 每次运行都完全相同。分量体积误差与抽样审计需要输入曲面 |
| [V7] 薄片与薄特征 | **部分** | 体阶段运行薄片面焊接性；s00-s03 因无四面体而报告 SKIPPED；单层带、中面与边缘一致性随 G7-1 落地 |
| [V8] 分区 | **体阶段完整** | 重算薄片阻断的洪水填充并与 `partition_id` 比对（允许重编号）、针孔泄漏启发式、`expected_partitions` 门限；s00-s03 报告 SKIPPED |
| [V9] 交汇 | 跳过 | 随 G6-4 落地 |
| [V10] 导出完整性 | 跳过 | 随 G9-2 落地 |
| [V11] 比较模式 | 跳过 | 随 GK-3 落地 |
| [V12] 溯源与统计 | **完整** | **契约校验**（契约 §4.2，计划 MG-08），强度由元数据选定——每条被违反的规则都是一条 `V12.contract` FAIL，强度以 INFO `V12.contract_strength` 与指标 `contract_strength`（0 仅几何、1 主体网格、2 混合契约）报告；计数、元数据回显、`Counts` 与网格实际值一致性、`SchemaVersion` 校验、`StageIndex` 范围与文件名交叉校验（T-C6），以及计划中 P2 所依据的**单元数归因**：按 `provenance` 分类的四面体计数，以及在 `RUSTMSPT_CUT_DIAG` 下存在 `parent_cell` 时，其背后的 S5 单元数与每单元发射率。S5 单元本身就是一个晶格**四面体**，因此未被处理的单元恰好发射 1 个，`tets_per_cell_*` 可直接读作「该路径相对于放着不动多付出的代价」。 |
| [V13] 界面保真度 | **设置 `surfaces:` 后完整** | 计划中的 P3，以**包含性**度量（计划 M-1.5）：每个材料边界面必须位于与之共面的输入面片的并集之内（`contained_area_frac`、`V13.not_contained`），每个未被更高优先级实体遮住、且不在区域盒上的输入面片必须被网格的边界面或标签面覆盖（`input_covered_area_frac`、`V13.boundary_missing`）。下面的角点检查是必要条件，继续保留。只要有任何材料边界面积不在曲面上，`V13.off_surface` 即为 **FAIL**（契约 rev 1.3；计划 M-1.0），每个违规面一条发现。见下文——它并不是 [V5] 的一个变体。 |

**[V13] 做的是 [V5] 做不到的事。** [V5] 度量的是**已声明**的界面：带标签的 `VTK_TRIANGLE`
单元，其节点已由 S7 吸附到输入曲面上。这些节点几乎精确地落在曲面上，因此即使一个网格真正
的材料边界——即对「自己在哪些实体内部」判断不一致的两个四面体之间的面——是半个单元之外
的一段阶梯、且完全没有携带标签，[V5] 仍会报告近乎完美的贴合。[V13] 从体网格推导边界，绝不
查看任何标签，这正是目标中「曲面精确」这一性质要从它读取的原因。

**它是在面的角点上读取的，这一点很关键。** 三个顶点都是曲面上切割节点的平面小面，是该曲面
的一条**弦**——由平面小面构成的网格所能做到的最好情况，其中央下垂量按 `h²` 衰减，而这正是加密
所换来的。若一个小面的顶点是晶格节点或单元形心，它就完全在别处了，那才是阶梯。把整个小面
一次性度量会把两者混为一谈：在球体夹具上，它把 87% 的边界面积算作网格生成器的过错，而其中
绝大部分是不可消除的分面误差。下垂量仍会被报告，记为 `chord_mean` / `chord_max`，作为独立
的一个数，而不是一项违规。

它报告两个数，因为边界失效有两种方式，而单一距离无法区分：

- **粗糙但居中**——边界在曲面两侧来回穿插。平均 \|距离\| 很大，而按面积加权的**带符号**
  偏移约为 0。
- **光滑但偏移**——边界是一整片干净的、但位置错误的面。两者都很大。

`displacement_share` = \|偏移\| / 偏差 就是把这一区分压成一个数：≈0 为粗糙，≈1 为偏移。
它之所以存在，是因为曾有一次改动在某个代理指标上被记为「改善 26%」，实际却把一个夹具从
第一种失效搬到了第二种，并使一块平板比输入薄了 2.6 倍——而整个测试套件毫无察觉。两者都是
P3 违规；这一对数字用于诊断，绝不用于把其中之一评为可接受。

值得注意的设计要点：

- **条目上限只约束存储，从不约束计数**（契约 §4.3）。每条发现都会抬升所在节的状态并
  计入其严重级别计数；只保留最严重的 `max_items_per_section` 条，且保留哪些与发现的
  先后顺序无关。`passed()` 读取各节状态。在计划 M-1.0 之前，汇总只统计已存储的条目，
  因此在一个失败网格上设置 `max_items_per_section: 0` 会以 0 退出。故生产方应推送每一条
  发现，而不是先用 `.take(cap)` 截取。
- **自称契约的文档要被校验，而不是被信任**（契约 §4.2）。过去删除某个数组会让对应检查
  SKIP，于是有缺陷的生产方也能验证通过；现在由元数据选择校验强度，缺失或格式错误的
  数组就是一条 `V12.contract` FAIL。

- **[V5] 的曲面优先级必须与生成网格时一致。** `volume_expected` 是**优先级裁决后**的
  体积——即该分量中未被更高优先级实体覆盖的部分——因此若某曲面以错误的优先级列出，
  [V5] 就会掩蔽一个网格生成器从未压制过的实体，其损失会被报告为零而非被度量。
  `surfaces:` 的每个条目为裸路径（优先级 0）或 `{stl, priority}`。在 2026-08-07 之前，
  优先级取自**列表下标**，这会静默吞掉被包含的实体：某个「立方体位于球体内部」的
  验收用例把该立方体报告为完全不占有任何单元，而测试套件依然全绿。
- **[V5] 在自相交输入上会切换分母。** 散度定理算出的曲面体积，只有在曲面不自相交时
  才是精确的。若某实体是作为**若干相互重叠部件的并集**导出的——例如支柱点阵、螺栓
  装配体——则每个重叠区域会被每个部件各计一次，于是精确求和得到的是各部件之和，而
  网格正确包含的是它们的并集。[V5] 会用抽样估计（广义绕数，按连通壳分别采样）交叉
  校验该求和值；当二者相差超过 5% 时，抬起 `V5.self_intersecting_input`，并改用**估计
  值**计算 `volume_expected`/`volume_error`。在某个 15 盒点阵上，这就是「报告 18% 体积
  损失」与「真实 9%」之间的差别。其余场合仍以精确求和为默认：它在干净输入上精确到
  十五位有效数字，而估计值带有约 0.5% 的抽样噪声。
- **`[V6]` 校验区域邻接关系，正是这项检查的缺失使材料被无声地丢失。**
  共享一个面的两个四面体之间恰好跨越一个分量的曲面，因此其内部集合应恰好相差一个成员——
  除非该面上有两张曲面**重合**，此时跨越所改变的分量数可以等于该面所携带的标签数。
  背景是哨兵键 `{0}`，按空集解读。`{1,2}` 与 `{0}` 之间的面是不可能存在的：两个相交
  实体的透镜区完全位于二者内部。A-3 曾携带 **521** 个这样的面（2026-08-07 实测），而
  整个测试套件依然全绿，因为每一项检查度量的都是别的东西。若一对区域键的分量优先级
  不同，则跳过该规则——此时高优先级标签替换其下方的标签，两个成员的跨越是合法的。
- **分区比对允许重编号。** 存储的 `partition_id` 数值不必与重算的连通分量编号
  一致，必须成立的是二者之间存在双射。因此「把两个不连通分量标成同一编号」的
  夹具会失败，而只是编号方式不同的有效网格不会。
- **角度门限从不调用 `acos`。** `tet_quality` 返回最小二面角的余弦值，门限直接
  比较余弦。角度值只用于报告、从不用于判定 —— 超越函数并非正确舍入，会使门限
  依赖平台（`SPEC_meshgen_numerics.md` §8.1 规则 4）。
- **面快照不是最终薄片网格。** s00-s03 有意只含标签面/曲线而没有体单元。因此
  [V7]/[V8] 对这些阶段报告 `SKIPPED`，而不是把每个面误报为未焊接，或把开放面
  误报为分区泄漏。

## 用法

```bash
# 默认配置位于 data/input/mesh_verify_config.yaml
cargo run --release -- mesh-verify --input data/output/mesh.vtu

# 显式指定输出；门限不通过时退出码非零
cargo run --release -- mesh-verify \
  --input data/fixtures/meshgen/good_cube.vtu \
  --report data/output/mesh_verification.log \
  --json   data/output/mesh_verification.json \
  --annotate data/output/mesh_annotated.vtu
```

`fidelity_vtu: out/compare.vtu`（需要 `surfaces:`）写出**输入与输出的对照曲面**：`[V13]` 所测的材料边界面，每个面带 `dev_pct`（角点到输入曲面的距离占该面边长的百分比）、`offset_pct`（带符号）和 `dev_class`（在 `mesh-render` 的分类调色板中：灰 < 2%、黄 2–10%、橙 10–25%、红 ≥ 25%），以及输入 STL 三角形（`source = 0`，细分到与输出面相近的尺寸）。用 `color_by: dev_class` 并对 `source` 加 `array_range` 过滤，即可把输入与输出并排显示；`data/fixtures/meshgen/acceptance/render_focus.py` 对每个算例自动完成这些，包括最差聚集区的特写。

带注解的 VTU 携带 `aspect_ratio`、`radius_ratio`、`min_dihedral_deg`、
`scaled_jacobian` 与 `verify_flags`，`mesh-render` 可据此着色或过滤
（`{ kind: array_range, array: aspect_ratio, min: 10.0, max: 1.0e30 }`）。

## 验收工具

`data/fixtures/meshgen/acceptance/` 下的脚本（都读取已完成的工作目录，并遵循 `RUSTMSPT_BIN`）：

| 脚本 | 回答的问题 |
|---|---|
| `run_acceptance.py --path default\|gated [--focus] [--jobs N]` | 在一条 S8 路径上跑十五个算例的矩阵（原有九个、它们的五个旋转版本 `a6a_r` … `a8_r`，以及 `p1`）：P3 表（`cont%`、`cover%`、`on%`）、P2 表、检查状态、各阶段耗时；`--focus` 为每个算例运行 `render_focus.py`。网格生成串行执行；每个算例的 `mesh-verify` 与渲染在最多 `N` 个工作线程上与下一个算例的网格生成重叠（默认 3，`--jobs 1` 完全串行）。`RUSTMSPT_ACCEPTANCE_WORK` 可改工作目录。 |
| `generate_rotated_cases.py` | 写出 `<name>_r.stl`：a6a、a6b、a7a、a7b、a8 绕过定义域中心的 (1, 2, 3) 轴转 23 度，不缩放，使任何面、棱和接触面都不沿点阵方向。若有部分离开单位定义域则拒绝。 |
| `p1/` | 一块金属体与其中十个气孔，单位毫米（最大气孔有 0.35 µm 的细颈）；气孔优先级 0 高于金属体的 1，定义域为金属体包围盒外扩 2 %。为此 CASES 条目支持 `(路径, 优先级)` 与 `domain` 覆盖。 |
| `render_focus.py <work> <case>` | 同一相机下输入 STL 与输出材料边界并排，输出按偏离着色；缺陷聚类、排序并以正视/斜视渲染（R9）。 |
| `run_reference.py` | 三个参考算例，按参考工具自身的背景网格与分段层级运行；报告单元比与 P3。路径跟随环境中的 `RUSTMSPT_PLC_PASS`。每个算例的 `mesh-verify` 与下一个算例的网格生成重叠。 |
| `check_determinism.py <work> --path P [--out table]` | 矩阵上的 R-P2：每个算例在 `RAYON_NUM_THREADS=1` 下重跑，按 SHA-256 比较交付文件与契约文件（排除标识运行的行）。表格：`determinism_{gated,default,reference}.txt`。 |
| `p2_equal_fidelity.py <work> [--out table]` | M-1.4：默认路径达到门控路径贴面比例所需 `h` 下的单元数（`h_max_frac`、`h_min_frac`、`eps_frac` 一起减半），在尺寸叶子预算或 2 GB 处停止。表格：`p2_equal_fidelity.md`。 |
| `node_dump.py <contract.vtu> <node>` | 单个节点的坐标、`constraint_kind`、`node_origin`，以及所有相邻四面体的区域集、来源、`parent_cell`、`escalation_reason`、`plc_path`（后三者需要网格化时设置 `RUSTMSPT_CUT_DIAG=1`）。 |

配合使用的仅打印诊断（只改变打印内容，从不改变网格）：`RUSTMSPT_CUT_DIAG`、`RUSTMSPT_JCT_DIAG`、`RUSTMSPT_PLC_DIAG`、`RUSTMSPT_PLC_CSV=<路径>`（§7.4 逐单元统计）、`RUSTMSPT_PLC_CELL=<点阵索引>[,<索引>...]`（为这些单元打开 cdt 小面/外壳/平坦四面体/侧别转储）、`RUSTMSPT_FACET_DIAG`、`RUSTMSPT_FLAT_DIAG`、`RUSTMSPT_HULL_DIAG`、`RUSTMSPT_SPLIT_DIAG`（搁浅面积统计同时给出小面分割扇形拒绝的原因）、`RUSTMSPT_TRACE_PROBE=x,y,z`（归一化坐标：经过该点的每个点阵面、其角点的 `constraint_kind`/`on_cut`、迹线弦与棱上交点）、`RUSTMSPT_THIN_DIAG`、`RUSTMSPT_TIME_STAGES`（对 `mesh-verify` 会逐项检查打印 `[VERIFY-TIME]`；各项检查并行运行，报告与串行运行逐字节一致）。

## 夹具套件

`data/fixtures/meshgen/` 保存 `SPEC_meshgen_contracts.md` §6 冻结的十一个手写
夹具 —— 一个基准网格，以及十个各自注入单一缺陷的网格。
`tests/mesh_verify_tests.rs` 对每个夹具同时断言：它触发的检查代码**精确集合**，
以及它所对应的具名检查确实触发。因此新出现的误报与漏报同样会让测试失败。

重新生成（模式与夹具一致时差异为空）：

```bash
uv run data/fixtures/meshgen/generate_fixtures.py data/fixtures/meshgen
```

## 相关文档

- [`SPEC_meshgen_contracts.md`](../../../SPEC_meshgen_contracts.md) —— 冻结的检查目录、JSON 模式、精度表、夹具清单。
- [`SPEC_meshgen_numerics.md`](../../../SPEC_meshgen_numerics.md) —— 谓词清单与 `predicates.rs` 实现的算术规则。
- [`SPEC_meshgen_geometry.md`](../../../SPEC_meshgen_geometry.md) —— 定向约定与基准夹具所用的晶格模板。
- [mesh-render-and-vtu.md](mesh-render-and-vtu.md) —— VTU 读写器与消费带注解网格的渲染器。
