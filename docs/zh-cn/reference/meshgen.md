# 网格生成 -- `mesh` 流水线配置与阶段

`mesh` 子命令（`PLAN_mesh_generation.md` 阶段 G1-1 至 G3-3）的配置 + CLI + 流水线参考：YAML 配置块（`src/config/meshgen.rs`）、流水线包装（`src/pipeline/meshgen.rs`）、S0 调理阶段（`src/meshgen/surface.rs`）、S1 特征检测阶段（`src/meshgen/features.rs`）、精确/分级构造（`src/meshgen/predicates.rs`）、注册表驱动的核心精化/共面覆盖（`src/meshgen/arrange.rs`）、裁剪后拓扑重建（`src/meshgen/topo.rs`）、S3 分离场与薄区域分割（`src/meshgen/gapfield.rs`）以及 S3<->S4 耦合驱动（`src/meshgen/sizing.rs`）。

权威设计见 [`PLAN_mesh_generation.md`](../../../PLAN_mesh_generation.md) §6.3（配置草图）与 §10.1-10.5（S0/S1/S2/S3 算法）。*输出*契约的规范记录位于 [`SPEC_meshgen_contracts.md`](../../../SPEC_meshgen_contracts.md)；`validate()` 强制的数值排序见 [`SPEC_meshgen_numerics.md`](../../../SPEC_meshgen_numerics.md)。

## 当前可运行内容

S0、S1 与 G2-1 至 G2-5 已实现。流水线在 S0 前按区域对角线归一化坐标，在修复中保留源身份，按构件检测并链接 S1 特征，随后执行精确非共面分类、C1/C2/C3 f64/DD 构造、注册表驱动的共面覆盖、C1-C10 策略分类、多标签原子面、接触曲线/点、具名降级邻域校验、混合宽相（中值幅度均匀网格+超大方侧列表+自动切换）、盒裁剪（实体封顶 `box` 标签、片体开放、曲线裁剪）与裁剪后拓扑重建（含 GWN 回退）。随后 S3（G3-1）测量分离场：双侧采样、法向射线、最近点对扫掠、自适应加密、五项配对校验组与分组置信度；G3-2 继而以滞回带分割薄区域、执行配对闭合、构建边缘环与经校验的中面，并运行 S3<->S4 不动点驱动。G4-1 提供该驱动的尺寸约束并构建分级尺寸场及其所依托的背景八叉树；G4-2 对该八叉树作强 2:1 平衡，并以冻结的 Freudenthal 与质心扇模板将其四面体化；G5-1 对每个晶格顶点与每个实体构件作分类并播种归属记录；G6-1 把晶格吸附到几何的角点、特征曲线与近节点交点上；G6-2..G6-5 完成切割。之后仅对 S9 至 S11 返回 `NotAvailable`。`s02_arranged` 快照在盒裁剪与拓扑重建完成后产出；`s03_gapfield`、`s04_sizing`、`s06_classified`、`s07_snapped` 与 `s08_cut` 在 `snapshots: all` 下随后产出，`s05_lattice` 与 `s08_cut` 属**关键**快照。

**在 S8 之前，网格并不贴合几何。** S5 构建的是完全无视输入的*背景*晶格；S6 判定其单元各自位于何物之内，形状因此可见但仍以单个单元为阶梯；S7 把顶点吸附到曲面上，**S8 切割跨越曲面的单元**，此后边界才精确。看上去粗糙的 `s05`、`s06` 或 `s07` 是设计使然，而非缺陷——S7 移动的是几百个落到特征上的节点，并不会把网格揉成形状。`s08_cut` 是第一个真正贴合输入的快照。

## 索引

| 条目 | 源码位置 | 概述 |
|---|---|---|
| `InputKind` | `src/config/meshgen.rs:10` | 单个输入的面角色覆盖：`auto`（默认，按闭合性检测）/ `solid` / `sheet`。 |
| `RepairLevel` | `src/config/meshgen.rs:20` | S0 修复激进度：`strict` / `conservative`（默认）/ `permissive`。 |
| `CoincidencePolicy` | `src/config/meshgen.rs:30` | G2-2 重合面策略：`merge`（默认）/ `reject` / `warn`。 |
| `FemProfile` | `src/config/meshgen.rs:40` | 目标求解器配置，控制薄层/薄片处理：`implicit`（默认）/ `explicit` / `none`。 |
| `DeterminismMode` | `src/config/meshgen.rs:50` | 运行可复现性契约：`strict`（默认，按位）/ `fast`（尽力）。 |
| `UnmappedPolicy` | `src/config/meshgen.rs:59` | INP 导出时对缺少材料映射区域的行为：`error`（默认）/ `elset-only`；kebab-case。 |
| `SnapshotMode` | `src/config/meshgen.rs:68` | 契约快照产出级别：`none` / `key`（默认，s02/s05/s08/s11）/ `all`。 |
| `MeshGenInput` | `src/config/meshgen.rs:84` | 单个 STL 输入：`stl` 路径、可选 `priority`（**每个输入均默认为 0**）、`kind`（默认 `auto`）。 |
| `MeshGenDomain` | `src/config/meshgen.rs:94` | 轴对齐生成区域；`min`/`max` 各须为 3 分量，且每轴 `min < max`。 |
| `MeshGenSizing` | `src/config/meshgen.rs:107` | 作为区域包围盒对角线分数的尺寸场上下限，外加 `grading`（默认 2.0，即 2:1 梯度）与 `gap_cells`（默认 2.0）。 |
| `MeshGenGaps` | `src/config/meshgen.rs:128` | 间隙场厚度因子（x 局部 h(x)）与分离置信度下限。 |
| `MeshGenEnvelope` | `src/config/meshgen.rs:176` | 作为区域包围盒对角线分数的数值包络厚度。 |
| `MeshGenRepair` | `src/config/meshgen.rs:183` | S0 修复配置（`level`）。 |
| `MeshGenMaterials` | `src/config/meshgen.rs:194` | Abaqus INP 导出的材料分配；`by_component` 会保留重复键到 `validate()`。 |
| `MeshGenOutput` | `src/config/meshgen.rs:211` | 输出目的地：必填 `vtu`，可选 `abaqus`/`report`。 |
| `MeshGenParams` | `src/config/meshgen.rs:225` | `meshgen:` YAML 块；加载后须调用 `validate()`。 |
| `MeshGenConfig` | `src/config/meshgen.rs:257` | 顶层 YAML 包装（`meshgen:`）。 |
| `MeshGenInput::resolved_priority` | `src/config/meshgen.rs:265` | 有效优先级：显式取值，否则为 0。不接收文件索引——「由文件顺序推导默认优先级」正是它所修复的缺陷。 |
| `MeshGenParams::validate` | `src/config/meshgen.rs:279` | 强制 PLAN §6.3 的解析期拒绝；返回 `Ok(())` 或 `InvalidConfig`。 |
| `deserialize_component_map` | `src/config/meshgen.rs:509` | 将 `by_component` 反序列化为保留重复键的有序对列表。 |
| `MeshGenPipeline` | `src/pipeline/meshgen.rs:57` | 运行归一化 S0/S1/G2-1..G2-5/S3/S4，产出 s02、s03 与 s04，并对 S5..S11 返回 `NotAvailable`。 |
| `ConditionedSurface` / `ConditionStats` | `src/meshgen/surface.rs:88/116` | S0 几何、持久源构件 ID、修复日志与汇总计数。 |
| `SurfaceComponent`（`ArrangeComponent`） | `src/meshgen/surface.rs:99` | 面阶段共享的契约构件行 `{X, 优先级 Y, solid/sheet 类型, closed}`。 |
| `condition_surface` | `src/meshgen/surface.rs:135` | 在 `q=0.1*eps` 上焊接、丢弃精确退化面、按源身份去重、定向/修复并导出临时构件。 |
| `source_component_is_closed` | `src/meshgen/surface.rs:332` | 精确组合闭合检查：源构件每条无向边的关联度均为 2。 |
| `condition_surface_to_doc` / `surface_stage_to_doc` | `src/meshgen/surface.rs:301/351` | 为 s00-s03 文档构建全部必备 schema-v1 面/曲线数组与表。 |
| `FeatureEdgeKind` / `FeatureCurve` / `FeatureSet` | `src/meshgen/features.rs:19/27/40` | 锐边/边缘/非流形边类型、构件感知链接折线、交汇点与角点。 |
| `detect_features` | `src/meshgen/features.rs:54` | 以代数二面角/转角测试按构件确定性检测并链接 S1 特征。 |
| `features_to_doc` | `src/meshgen/features.rs:290` | 构建 schema-v1 s01 面/曲线文档；角点/交汇点使用 `constraint_kind=3`。 |
| `ProjectionAxis` / `best_projection_axis` / `project_to_2d` / `orient2d_axis` / 值与 DD 辅助函数 | `src/meshgen/predicates.rs:54/61/74/83/92/178` | 最佳条件 3D→2D 投影、精确投影定向符号及 C3 使用的 f64 permanent/DD 值。 |
| `two_sum` / `two_prod` / `DoubleDouble` | `src/meshgen/predicates.rs:111/118/125` | 冻结的无误差原语与仅含加/减/乘的 DD 算术。 |
| `DeterminantRatio` / `PrecisionTier` / `ConstructionOutcome` | `src/meshgen/predicates.rs:192/229/254` | 精确排序比值及其方法、提交精度溯源与已解析/延迟构造结果。 |
| `orient3d_value_permanent` / `orient3d_filtered` / `orient3d_dd_value` | `src/meshgen/predicates.rs:266/286/296` | Shewchuk 顺序 f64 值/permanent、带精确回退的认证静态过滤器与 DD 行列式。 |
| `construct_edge_triangle_intersection` | `src/meshgen/predicates.rs:434` | 冻结 C1 行列式比值构造；f64/DD 升级与 DD 下限路由。 |
| `CoplanarSegmentPoint` / `construct_coplanar_segment_intersection` | `src/meshgen/predicates.rs:244/384` | 冻结 C3 仿射线段交点；检查两条定义边的稳定比并保留 DD 排序比值。 |
| `construct_three_triangle_intersection` | `src/meshgen/predicates.rs:631` | 冻结 C2 局部坐标 Cramer 构造；升级时完全以 DD 重算。 |
| `EdgeId` / `EdgeId::new` / `IsectProv` / `SegKey` / `SegKey::new` | `src/meshgen/arrange.rs:29/33/44/52/61` | 规范 `EdgeTri`/`EdgeEdge`/`TriTriTri` 与线段身份。 |
| `CoincidenceCase` / 策略方法 / `CoincidenceEntity` / `CoincidenceEvent` | `src/meshgen/arrange.rs:73/88/97/105/112` | C1-C10 类型化分类、排序实体/构件与冻结拒绝/警告语义。 |
| `DegradedReason` / `DegradedNeighborhood` / `ArrangedPointFeature` | `src/meshgen/arrange.rs:120/130/140` | 持久 G2-3 回退记录与焊接 C5/C6 点特征。 |
| `ArrangeOptions` / `ArrangeOptions::new` / `with_coincidence` | `src/meshgen/arrange.rs:149/159/175` | 区域、epsilon、策略与构件表；构造器默认 `merge`。 |
| `RegistryVertex` / `RegistrySegment` / `IntersectionRegistry` | `src/meshgen/arrange.rs:183/194/203` | 符号优先全局注册表；一个提交节点保留全部兼容溯源别名。 |
| `ArrangedCurve` / `ArrangedFace` / `ArrangedSurface` | `src/meshgen/arrange.rs:218/228/256` | 多源/多标签原子面、曲线/径向顺序、点特征、事件、警告与降级记录。 |
| `ArrangementStats` | `src/meshgen/arrange.rs:243` | 候选、真相交/覆盖/接触、f64/DD/下限、特征与降级计数。 |
| `arrange_surface` | `src/meshgen/arrange.rs:439` | 纯确定性 CPU G2-1..G2-3 路径；返回已校验诊断复形或策略/不变量错误。 |
| `arranged_surface_to_doc` | `src/meshgen/arrange.rs:804` | 以集合面标签和 `FaceTagOrientation` 编码诊断复形；流水线暂不打标为 s02。 |
| `triangulate_parent` | `src/meshgen/arrange.rs:3855` | 受限预注册 Spade CDT；传播插入/拒绝约束错误并校验约束与铺满。 |
| `SampleKind` / `PairClass` | `src/meshgen/gapfield.rs:48/57` | S3 采样来源（顶点/形心/最近点对/加密）与冻结的配对类别（intra / inter / solid-sheet / sheet-sheet / surface-box）。 |
| `GapPairing` / `GapSample` / `GapSample::passes_battery` | `src/meshgen/gapfield.rs:68/81/100` | 单条对应关系、单个采样（侧、方向、`t_raw`/`t`/`t_exact`、校验位）与"全部适用检查通过"判据。 |
| `GapGroup` / `GapFieldStats` / `GapField` | `src/meshgen/gapfield.rs:110/122/137` | 带置信度与 `t_r` 的临时（构件, 侧, 对侧面片）分组、S3 计数器与整体分离场。 |
| `GapFieldOptions` | `src/meshgen/gapfield.rs:280` | S3 输入：区域、epsilon、引导 h、间隙因子、置信度下限、特征角（顶点聚类）、虚拟壁、加密轮次、平滑次数。 |
| `FLAG_MUTUAL` / `FLAG_OPPOSITE_PATCH` / `FLAG_CONTINUITY` / `FLAG_NO_CROSSING` / `FLAG_ORIENTATION` / `FLAGS_ALL` | `src/meshgen/gapfield.rs:28-38` | 五项配对校验位及其并集。 |
| `compute_gap_field` | `src/meshgen/gapfield.rs:495` | 在裁剪且拓扑重建后的排布面上运行 S3（射线 + 最近点对扫掠 + 校验组 + 置信度）。 |
| `gapfield_to_doc` | `src/meshgen/gapfield.rs:3080` | 构建 `s03_gapfield` 文档：排布面加 `separation_t` 点场（`-1` 表示无配对）。 |
| `Regime` / `SkipReason` / `MidSurfaceDefect` | `src/meshgen/gapfield.rs:166/177/190` | 三种薄特征状态、`[THIN-SKIP]` 分类与中面校验缺陷。 |
| `MidSurface` / `MidSurface::is_valid` / `ThinRegion` | `src/meshgen/gapfield.rs:204/216/227` | 带源节点与缺陷的中点面片，以及一个分割后的薄区域（壁 A 面、闭合并入的对侧壁、边缘环、状态、置信度）。 |
| `validate_mid_surface` | `src/meshgen/gapfield.rs:2945` | 对候选中面执行 §3.4 检查（面积、定向、法向偏差、自交、边缘一致性、欧拉数）。 |
| `CouplingOptions` / `LockReason` / `CouplingReport` / `CouplingReport::locked_for` | `src/meshgen/sizing.rs:35/66/80/94` | 耦合循环输入、三种锁定原因、运行报告与按原因查询锁定项。 |
| `regime_for` | `src/meshgen/sizing.rs:143` | 以 0.9/1.1 滞回死区将单个区域与当前阈值比较分类。 |
| `couple_gap_and_sizing` | `src/meshgen/sizing.rs:189` | 运行 S3<->S4 不动点；违反 G-8 排序断言时返回错误。 |
| `SizingCriterion` / `SizingSource` | `src/meshgen/sizing.rs:335/351` | 产出该尺寸约束的 §10.6 准则，以及约束本身。 |
| `SizingOptions` / `beta` / `lfs_floor` | `src/meshgen/sizing.rs:363/410/429` | 尺寸场输入；Lipschitz 常数 `grading - 1`；低于该下限的分离量不算间隙。 |
| `curvature_sources` / `feature_sources` / `curve_sources` / `collect_geometry_sources` | `src/meshgen/sizing.rs:504/605/677` | 与状态无关的准则，读自条件化输入曲面。 |
| `gap_sources` | `src/meshgen/sizing.rs:815` | 状态相关的 LFS 源：每个体网格 S3 采样取 `t / gap_cells`。 |
| `SizingLookup` / `eval` / `eval_box` | `src/meshgen/sizing.rs:969/1067/1083` | 梯度场；点求值与盒上精确最小值。 |
| `SizingLeaf` / `SizingStats` / `SizingField` / `locate` / `sample` | `src/meshgen/sizing.rs:1156/1164/1182/1220/1248` | 背景八叉树、构建报告与点定位。 |
| `build_sizing_field` | `src/meshgen/sizing.rs:1416` | 每层一趟并行细化八叉树，直至每个叶子都解析该场。 |
| `SizingConstraint` / `evaluate` / `binding_region` | `src/meshgen/sizing.rs:1407/1468/1498` | 供耦合驱动的 `C(R)`，以及绑定它的区域与项。 |
| `sizing_to_doc` | `src/meshgen/sizing.rs:1699` | 将尺寸场编码为 `s04_sizing` 体素预览 VTU。 |
| `FREUDENTHAL` / `CellTemplate` | `src/meshgen/lattice.rs:54/493` | 冻结的 6-tet Kuhn 表，以及叶子采用了哪类模板。 |
| `balance_octree` / `balance_violation` | `src/meshgen/lattice.rs:156/286` | 强（面+边+顶点）2:1 平衡，以及对该性质的直接检验。 |
| `Lattice` / `LatticeStats` / `LatticeOptions` | `src/meshgen/lattice.rs:520/502/531` | 四面体化晶格、其构建报告与四面体预算。 |
| `build_lattice` / `build_lattice_with_splits` | `src/meshgen/lattice.rs:611/616` | 以 Freudenthal 与扇形模板对平衡八叉树作四面体化。 |
| `lattice_to_doc` | `src/meshgen/lattice.rs:837` | 将晶格编码为 `s05_lattice` 快照 VTU。 |
| `Side` / `Provenance` / `OwnershipRecord` | `src/meshgen/classify.rs:60/68/82` | 四面体相对构件的内外侧、条目来源与稀疏记录。 |
| `resolve` | `src/meshgen/classify.rs:180` | 冻结的标签规则（SPEC_meshgen_geometry §9.1）。 |
| `RAY_DIRECTIONS` | `src/meshgen/classify.rs:46` | 冻结的重发射序列（ARB-9）。 |
| `Classification` / `ClassifyStats` / `ClassifyOptions` | `src/meshgen/classify.rs:140/112/457` | S6 结果及各判定的达成方式。 |
| `classify_lattice` | `src/meshgen/classify.rs:704` | S6：奇偶分类、记录播种、活跃面片过滤。 |
| `classified_to_doc` | `src/meshgen/classify.rs:1044` | 编码 `s06_classified` 快照 VTU。 |
| `Stage` | `src/meshgen/snapshot.rs:22` | 冻结的阶段枚举（0..=11）；亦为快照索引。 |
| `Stage::from_path` | `src/meshgen/snapshot.rs:102` | 从快照文件名的 `sNN` 标记解析阶段（用于 [V12] 交叉校验）。 |
| `should_emit` | `src/meshgen/snapshot.rs:120` | 在 `none`/`key`/`all` 下是否产出某阶段。 |
| `snapshot_path` | `src/meshgen/snapshot.rs:147` | `<stem>.debug/<stem>_sNN_<name>.vtu`（Quality 带 `_r<N>`）。 |
| `SnapshotMeta` | `src/meshgen/snapshot.rs:195` | 打标输入集合（阶段、轮次、配置哈希、区域、确定性、生成器版本）。 |
| `stamp_metadata` | `src/meshgen/snapshot.rs:232` | 将完整 §2.4 元数据块打标到快照文档。 |
| `emit_snapshot` | `src/meshgen/snapshot.rs:295` | 打标元数据，然后以普通名写出交付用的仅四面体体网格，并在其旁写出混合单元契约文档 `_contract.vtu`；返回交付文件路径。 |
| `warn_if_large` | `src/meshgen/snapshot.rs:348` | 尺寸 WARN：`snapshots: all` + 估计 >5 M 四面体。 |

## 配置块（PLAN §6.3）

```yaml
meshgen:
  inputs:
    - stl: data/input/particle1.stl        # priority 每个输入均默认为 0
    - stl: data/input/particle2.stl        # 同级 -> 其重叠区保留两个 X（R-A3）
    - stl: data/input/coating.stl
      priority: 1                           # 被显式压制（R-A4）
    - stl: data/input/grain_boundary.stl
      kind: sheet                           # auto | solid | sheet
  domain: { min: [0,0,0], max: [1,1,1] }
  sizing:
    h_max_frac: 0.05        # x 包围盒对角线（上限）
    h_min_frac: 0.002       # x 包围盒对角线（下限）
    chord_error_frac: 0.2
    feature_angle_deg: 45.0
  gaps:
    t_layer_factor: 1.0     # x 局部 h(x)
    t_sheet_factor: 0.2     # x 局部 h(x)
    confidence_min: 0.9
  envelope: { eps_frac: 1.0e-4 }            # x 包围盒对角线
  repair:   { level: conservative }          # strict | conservative | permissive
  coincidence: merge                         # merge | reject | warn
  fem_profile: implicit                      # implicit | explicit | none
  determinism: strict                        # strict | fast
  materials:
    by_component: { 1: steel, 2: pore }
    by_region_key: { "3+5": composite_A }
    unmapped: error                          # error | elset-only
  acceleration: { mode: auto }
  snapshots: key                             # none | key | all
  output:
    vtu: data/output/mesh.vtu
    abaqus: data/output/mesh.inp             # 可选
    report: data/output/mesh_verification
  verify:                                    # 门限覆盖（与 mesh-verify 共享）
    max_ar_warn: 20.0
```

### 默认值

| 块 | 字段 | 默认值 |
|---|---|---|
| `sizing` | `h_max_frac` / `h_min_frac` / `chord_error_frac` / `feature_angle_deg` / `grading` / `gap_cells` / `curve_cells` | `0.05` / `0.002` / `0.2` / `45.0` / `2.0` / `2.0` / `2.0` |
| `gaps` | `t_layer_factor` / `t_sheet_factor` / `confidence_min` | `1.0` / `0.2` / `0.9` |
| `envelope` | `eps_frac` | `1.0e-4` |
| `repair.level` / `coincidence` / `fem_profile` / `determinism` | - | `conservative` / `merge` / `implicit` / `strict` |
| `materials.unmapped` / `snapshots` | - | `error` / `key` |
| `acceleration` | - | `AccelerationConfig::default()`（`mode: auto`） |
| `verify` | - | `VerifyGateParams::default()`（全部门限取契约默认） |

必填字段：`inputs`（非空）、`domain`、`output.vtu`。

### 解析期拒绝（`MeshGenParams::validate`）

PLAN §6.3 的拒绝在代码中强制执行，而非假设。出现以下任一情况，`validate()` 返回 `InvalidConfig` 并指出违反的规则：

- `inputs` 为空。
- `domain.min`/`domain.max` 非 3 分量，或任一轴 `min >= max`（"nonpositive domain"）。
- `sizing`：不满足 `0 < h_min_frac < h_max_frac`；`chord_error_frac <= 0`；`feature_angle_deg` 不在 `(0, 180)`。
- `gaps.confidence_min` 不在 `(0, 1]`。
- `gaps.t_sheet_factor >= t_layer_factor`（承重排序 `eps << t_sheet < t_layer <= h`）。
- `gaps.t_sheet_factor <= 0`。
- `envelope.eps_frac >= 0.5 * t_sheet_factor * h_min_frac`（同一排序，经检查而非假设 -- SPEC_meshgen_numerics）。
- `inputs[].priority` 为负值（契约 VTU 中 Y 以 `UInt32` 承载）。

> 两个 `inputs` 解析到同一优先级**不会**被拒绝——而在 2026-08-07 之前会。同优先级
> 正是 R-A3 的前提：重叠区被保留并同时携带两个 X。拒绝它，再叠加旧的「默认取文件
> 索引」，就使得任何配置都无法产生含一个以上 X 的区域键——这又进一步让 `[V6]` 的
> 「同优先级键共享同一 Y」子句始终在度量空集。
- `materials.by_component` 存在重复键。

> **对计划草图的修正（G1-1）：** 草图默认 `envelope.eps_frac: 1.0e-3` 违反了其自身的解析期拒绝（在默认 gap/sizing 因子下 `1.0e-3 >= 0.5 * 0.2 * 0.002 = 2.0e-4`），因此实现默认值下调为 `1.0e-4`。这与 G0-3 捕获的 `u16` 表数组属于同类草图/规范不一致，并记录在 `PLAN_mesh_generation.md` 的 G1-1 落地说明中。

`materials.by_component` 通过 `deserialize_component_map` 反序列化为 `Vec<(i64, String)>` 而非 `HashMap`，因此重复的组件覆盖能存活到 `validate()` 予以拒绝 -- 普通映射会静默保留最后一条。

### 延迟检查（尚未接线）

请求 INP 导出且 `materials.unmapped: error` 时，若任一多 ID 区域缺少映射，则在导出时失败。这是一个*延迟*但写盘前的检查，会给出可操作的键列表；它随 S11 导出驱动（G9-2）落地，不在当前 S0/S1/G2-1..G2-3 实现切片中。

## CLI

```bash
./target/release/rustmspt mesh --config data/input/meshgen_config.yaml
# 覆盖：
./target/release/rustmspt mesh --input data/input/particles.stl --output data/output/mesh.vtu
```

`--input` 将 `meshgen.inputs` 替换为单条 STL 条目（priority 0，kind `auto`）；`--output` 替换 `meshgen.output.vtu`。

## 流水线行为

`MeshGenPipeline::run`：

1. `MeshGenParams::validate()` -- 强制解析期拒绝。
2. 通过 `load_stl` 预加载每个输入 STL，在文件缺失或无法解析时立即失败（错误信息指明出错的输入路径）。
3. 将解析后的计划（输入及其解析优先级/类型、区域、尺寸、间隙、包络、修复、重合策略、fem_profile、确定性、快照、输出）打印到 stdout。
4. 将每个坐标归一化为 `(p-domain_min)/domain_diagonal`；仅在写快照时执行逆变换，使 S0 判定与平移和尺度无关。
5. 运行 S0 与 S1。`snapshots: all` 产出可通过验证器的 s00/s01；`key` 不产出二者，因为冻结的 key 集从 s02 开始。
6. 运行 G2-1..G2-3：精确非共面精化、C3 共面覆盖、重合策略、经标定的 q/epsilon 路由与类型化降级回退；CPU 参考扫描仍在检查对数超过 `50 * triangle_count` 时路由到 G2-5。
7. 在 G2-4/G2-5 完成前无条件扣留 `s02_arranged`，然后对这些任务及 S3..S11 返回 `Err(NotAvailable)`。部分运行不会被误认为契约完整网格。

当前部分运行与解析拒绝运行的捕获输出见 [`mesh.md`](../examples/mesh.md)。

## G2-1 至 G2-3 排布

注册身份先于几何量化建立。`EdgeTri` 使用稳定源边/三角形 ID，`EdgeEdge` 排序两条源边，`TriTriTri` 排序三个三角形 ID。请求在有序容器中收集，仅按规范键顺序分配 ID。共享一个 `NodeKey` 的溯源仍作为同一 `RegistryVertex` 的别名可见；非同坐标别名同时产生 G2-3 歧义记录。

所有拓扑符号均使用项目精确包装器。C1/C2/C3 先以 f64 计算并将稳定比与 `kappa_esc` 比较；不稳定构造以 DD 重建，且选用层 rho 由 DD 分母/permanent 推导，而非 f64 比值，因此 f64 灾难性相消但 DD 可解的构造不会被误判为下限延迟。按实现默认 `eps_frac=1e-4`，DD 下限 `kappa_esc * u_dd/u` 约为 `3.5e-26`。C3 独立检查两条定义边的稳定比、仅构造一次仿射 3-D 点并保留两条 DD 排序比值。C2 在局部坐标中完整重算。

共面三角形共享一个预注册原子约束复形。C3 交点、包含顶点、共线接触与不同重铺分片均在受限 CDT 前切分；相同原子面仅发射一次并保留全部源三角形、构件标签与每标签方向。C3 交线仅在真实共享/排他原子面边界上发射，不含源三角化接缝；提升后的 C1/C2 patch 无 C3 交线。溯源受限细分确保仅符号关联节点可修改源边或接触。C1/C2 提升为 patch 局部，故不相邻的排他几何或同一构件对的另一 C7 近重合 patch 不会阻止别处提升。C1 精度下限降级记录保留延迟溯源与边目标点供 S7 使用。EdgeEdge 别名降级保留全部非规范拥有者 TriId。C4 曲线与 C5/C6 点强制嵌入每个关联源面。冻结策略仅拒绝 C1/C2/C3/C7/C8/C9；`warn` 的几何与 `merge` 完全相同，流水线按 case 聚合一条 WARN。C10 使用精确分离轴定向符号判定与有界域面的正面积重叠，但 box 裁剪、box 标签与 box 曲线仍属 G2-5。

G2-3 将 C7 冻结为互相一一对应的三角形顶点且 `distance^2 <= epsilon^2`。完全链接簇直径必须保持 `<= epsilon`，防止传递过度合并；代表点按最小规范三角形/`NodeKey` 排名选取。DD 下限、q 排序、折叠接触、残余相交与径向共面均保留为携带源 ID、点、溯源与 rho 的 `DegradedNeighborhood`，供后续 S7 交替投影回退使用。

每个受影响源三角形独立使用 `spade` 2.15.1 三角化。仅插入预注册点，且只使用 `try_add_constraint`。分裂父边禁止跨越中间注册节点的 chord；校验约束、面积铺满、曲线/点嵌入，并把任何未记录的真相交或共面覆盖转为具名降级。特征曲线在所有原子面节点处分裂；真相交线段记录精确四面循环径向顺序。

有意保留的后续任务边界：

- G2-4：权威 patch/构件拓扑重建、闭合性与 GWN。
- G2-5：盒裁剪与混合生产级宽相。

验收位于 `tests/meshgen_arrange_tests.rs`（43 项加 2 项私有单元测试）：十个策略行的三模式、重铺分片 C1/C2、部分/多路覆盖、端点加内部接触、C7 epsilon/倾斜/传递簇、Spade 拒绝/插入错误、相消行列式 DD 解析、溯源受限细分、C7 与 C1/C2 的 patch 局部共存、符号化注册排序、EdgeEdge 拥有者保留、S1 曲线归属、精确有界 C10 重叠、100,000 个近退化谓词对照，以及重复运行的 250 例排布语料。G2-3 实测为 250/250 确定、500 次 DD 升级、0 次下限路由、0 个降级 case、0 个硬失败；冻结总计在测试中断言，专用夹具另行覆盖每个类型化回退触发器。

## 快照框架（GA-4）

`src/meshgen/snapshot.rs` 实现了 `SPEC_meshgen_contracts.md` §2.4/§3 冻结的契约快照工作流。阶段驱动（G1-2 起）在其边界调用 `emit_snapshot`；`mesh-verify` 从快照文件名解析阶段，并通过 [V12] 与 `StageIndex` 交叉校验。

| 阶段 | 索引 | 名称 | `key`？ |
|---|---|---|---|
| Conditioned | 0 | `conditioned` | 否 |
| Features | 1 | `features` | 否 |
| Arranged | 2 | `arranged` | **是** |
| Gapfield | 3 | `gapfield` | 否 |
| Sizing | 4 | `sizing` | 否 |
| Lattice | 5 | `lattice` | **是** |
| Classified | 6 | `classified` | 否 |
| Snapped | 7 | `snapped` | 否 |
| Cut | 8 | `cut` | **是** |
| Thin | 9 | `thin` | 否 |
| Quality | 10 | `quality_r<N>` | 否 |
| Final | 11 | `final` | **是** |

- **命名：** `<output_stem>.debug/<stem>_sNN_<name>.vtu`。`Quality` 带 IQD 轮次（`s10_quality_r<N>`），每轮产出一次。
- **模式门控**（`should_emit`）：`none` 不产出；`key` 产出 {s02, s05, s08, s11}；`all` 产出每个阶段。
- **元数据**（`stamp_metadata`）：写出完整 §2.4 块 -- `SchemaVersion`（1）、`StageIndex`、`GeneratorVersion`（3 元组 semver）、`ConfigHash`（u64）、`DomainMin`/`DomainMax`、`Counts`（从网格重算）、`DeterminismMode`。每个快照携带完整块，故验证器与渲染器可互换地接受任一快照。`ConfigHash` 覆盖全部有效字段；无序区域材料映射在哈希前排序。
- **面阶段**（s00-s03）仅含面/曲线单元；s04/s05 预览写 `cell_kind = 3` 体素单元并带 `sizing_h`（由生产者保证，非框架）。`mesh-verify` 对 s00-s03 跳过 [V7]/[V8]，因为这些检查需要体单元与分区。
- **尺寸 WARN**（`warn_if_large`）：`snapshots: all` 且估计超过 5 M 四面体时在运行前打印 WARN（每体积快照约 60 B/四面体）。
- **[V12] 交叉校验**（T-C6）：`Stage::from_path` 从快照文件名解析 `sNN`；`mesh-verify` 将其作为 `VerifyOptions::expected_stage` 传入，[V12] 在 `StageIndex` 与文件名不一致、`StageIndex` 超出 0..=11、或 `SchemaVersion != 1` 时 FAIL。

验收（`tests/meshgen_snapshot_tests.rs`）：夹具流水线从 `good_cube.vtu` 产出完整 s 系列；每个快照可重新加载、校验、验证（无 FAIL，文件名交叉校验激活）并通过 `build_scene` 渲染。

当前真实 mesh 流水线在 `all` 下产出 s00/s01/s03，在 `key` 与 `all` 下产出 s02。仅夹具使用的快照系列测试另外覆盖每个冻结阶段名与元数据契约。

## G3-1 分离场（S3）

`src/meshgen/gapfield.rs` 在裁剪且拓扑重建后的排布复形上实现 PLAN §10.5。`compute_gap_field` 是纯函数且确定性：三角形沿排布面顺序，所有候选列表使用前排序，平滑采用 Jacobi 而非 Gauss-Seidel，因此任何结果都不依赖遍历顺序。

**采样。** 每个面一个形心采样；在每个 `(构件, 顶点)` 处，为该点相交的每个**光滑面片**各产出一个采样——顶点的相邻面按 `feature_angle_deg` 内的法向一致性聚类，每个聚类沿其自身面积加权法向发射。在尖锐顶点上使用单一平均法向是错误的：立方体角点属于三个壁面，其平均法向不指向任何一个，会使射线斜穿侧面并扭曲定向检查。所有采样均在曲面**两侧**产出。参考实现只采样标定后的内侧；此处的推广是必需的，因为薄区域可能是材料（薄壁，`intra(X)`）也可能是空隙（两构件间的间隙，`inter(X_a, X_b)`），而 R-B1 必须对两者都作判定。六个区域面作为虚拟壁（`surface-box`）加入查询集，法向朝内。

**射线。** 自 `p + 1e-3*h*d` 沿该侧方向发射，长度 `2*t_layer`，排除与采样点关联的全部三角形。命中条件为 `|d . n_hit| >= cos 60°`；当两侧壁均属闭合实体时还要求材料侧符号相反——这正是区分真实间隙与同一壁另一面的判据。闭合实体的外法向由该构件自身有符号体积的符号一次性确定：S0 只保证面片内绕向一致，并不确定其全局符号。

**最近点对扫掠。** 对 `t_layer` 内的每个三角形对（均匀网格，AABB 按 `t_layer` 膨胀），在 9 条边-边与 6 个顶点-面候选上求精确最近点对。共享排布节点的对被跳过：核心精化后它们精确接触，属于接触而非间隙。同一构件内的对还必须彼此相对，否则扫掠会把弯曲面片自身的离散间距当作间隙。每个 `(三角形, 对侧构件)` 只保留其最近一次接近作为种子。

**加密与平滑。** 相邻采样 `|grad t|` 超过 `0.5` 时插入中点采样（至多 3 轮，不低于 `2*eps`）；随后对射线场做 3 次 Jacobi 平滑 `t <- 0.5*t + 0.5*mean(有限邻居)`。平滑只写 `t`，`t_raw` 与 `t_exact` 是冻结测量值。

**校验组与置信度。** 五项检查各置一位：互射一致性、对侧面片可解析、对侧壁 2 环内的配对连续性、相邻对应线段无局部交叉、对应保持定向。对某采样不适用的检查（对虚拟壁的连续性、缺少完整顶点三元组的定向）会从 `applicable` 中清除，而非静默通过。采样按 `(构件, 侧, 对侧面片)` 分组；分组置信度为通过全部适用检查的比例，`t_r` 在扫掠给出测量时取该组的精确最近点对距离（规则 S3-M），否则取最小射线分离。

相对 参考薄特征设计 §3.3 有两处偏离，因其改动了成文规则而在此记录：

- 检查 1 接受 `q` 的 `0.5*h` 邻域内**任一**满足条件的对侧采样；参考实现只检验最近的一个，而重合的最近点对种子会使"最近"变得不确定，不应由任意选中的重复项决定该检查。
- 射线在每个曲面两侧发射（见上），因此不需要按点定位标定 `n_in`，intra/inter 的区分来自配对类别而非采样侧。

**快照。** `gapfield_to_doc` 输出排布文档加 `separation_t` 点数组（schema v1 §2.2，`Dbg`）：每个排布顶点取该顶点或其关联面上任一采样的最小有限分离，`-1` 表示"此处无配对"（对非负量的"不适用"约定）。流水线在写出前转换为模型单位。`mesh-render` 可直接按其着色（`color_by: separation_t`）：点数组按单元非哨兵点值的均值归约。

验收（`tests/meshgen_gapfield_tests.rs`，13 项）：孤立实体不产生任何配对；立方体角点解析为三个光滑面片，各沿其自身壁面法向发射；相距 0.02 的两实体测量误差在 5% 内；薄板穿过自身材料配对其两个面；分叉喉部拆分为两个对侧面片；竖直板位于水平板上方（每条法向射线都与对侧壁平行）的情形仅由扫掠发现；弯曲 0.02 间隙误差在 10% 内；虚拟壁可测量亦可禁用；多次运行按位一致；s03 快照通过结构校验、验证器并携带 `separation_t`。

## G3-2 薄区域与 S3<->S4 循环

**分割。** 在单个分组（一侧壁面对一个对侧面片）内，确定性有序 BFS 以 `0.9 * 阈值` 为种子、`1.1 * 阈值` 为生长边界，先 `Sheet` 后 `Band`（参考薄特征设计 §3.3）。生长不跨分组，故分叉喉部保持为两个候选区域。

**配对闭合。** 每个成员所指向的采样并入同一区域，且每个采样在所有分组中至多属于一个区域。因此一处间隙产生**一个**同时拥有两侧壁的区域，而非两个镜像区域——中面也只有一张，不会出现两张重合副本。`faces` 是壁 A（区域生长所自、其三角化被中面复用），`opposite_faces` 是经闭合并入的对侧壁。

**生长不越出自身状态区间。** `Band` 区域仅在采样的**冻结**分离量（即将成为 `t_r` 的最近点对测量值）大于 `1.1 * t_sheet` 时才接纳它，配对闭合同样施加该下限。参考实现 的"LAYER 种子……（不属于 SHEET）"是对数值的判定，而非仅指"未被前一趟占用"：否则一个区域可从低于 `t_sheet` 一直跨到 `t_layer`，这样的分离范围没有任何单一带状模板能够划分。生长的*连通性*仍沿用平滑场。

**接触不是间隙。** 两条规则将接触的邻域排除在状态之外。`reject_geodesic_shortcuts` 清除任何（射线或扫掠得到的）对应关系，只要其两个三角形沿曲面也很近：间隙是穿过自由空间的直线，曲面路径短即意味着该线段是穿过几何体的捷径。面邻接刻意跨构件，因为核心精化使相交曲面沿其交线共享边。随后，若区域触及该构件对的**接触面集**——其 S2 交线所关联的面，与自身配对被判为捷径而清除的面之并集，并按一个面环膨胀（捷径判定已清除接触*处*的配对，故楔形区域自其外一环起始）——则记为 `IntersectionWedge` 的**必要**条件成立。两个集合均按构件对索引，故某一对的接触绝不会取消另一对的间隙。

**触及接触面集只是必要条件，远不充分。** 一根悬臂附着于方块时，悬臂的**每一个**三角形都关联其根部曲线，于是仅凭接触判据就把悬臂自身均匀的间隙判成"曲线的邻域"，把片体夹具的全部 19 个区域悉数跳过——薄路径整体因一个物体太小而被关闭。楔形之所以是楔形，在于间隙会**收拢**，故区域还必须跨越片体阈值：

```text
IntersectionWedge  <=>  触及接触面集  且  t_r < t_sheet < t_max
```

`t_r` 是各成员采样报告的最窄分离，`t_max` 是最宽的。楔形的一端须塌缩、另一端须成网，没有单一模板能兼顾；而薄特征的两端同处 `t_sheet` 一侧，恰好由一行模板处理。实测：所有被跳过的区域其 `t_r` 与 `t_max` 相差不足百分之一——那是均匀间隙，不是楔形。

**加密同时按梯度与按空间分辨率。** 只在 `|grad t|` 陡峭处加密，能很好地描述*变化*的间隙，却完全描述不了**均匀**的间隙，而这正好反了：均匀间隙才是典型的片体或带体。以长方体建模的薄板每侧壁只有两个采样，二者之间梯度为零，任何一轮都不会触发，区域随后死在碎片门控上。第二条判据只在薄带内生效：邻接边的两端都在 `t_layer` 之内、且间距大于 `h_bootstrap` 时予以细分——尺寸场将以 `h_bootstrap` 分辨的间隙，就必须以 `h_bootstrap` 去*测量*，否则它的范围、边缘与中面全靠几个角点推断。片体夹具的加密样本由 7 个升至 148 个，其悬臂壁面随之成为一个 128 采样、`area` 恰为 0.27 x 0.20 的区域。

**校验组第 3 项按多数判定，而非一票否决。** 把不连续配对的两端一并判失败看似对称，其实不然——不连续只有一个作者。在板的边缘处，采样的射线会离开间隙而落到远处的物体上；在否决制下，这样一个离群点会把它周围全部八个内部邻居一并定罪：区区几个边缘采样带来 80 次否决，把区域置信度压到 0.82，而门槛是 0.90。现在一个采样只有在与其邻域中不一致者多于一致者时才判失败，离群点因而独自失败。置信度 0.820 -> 0.992。

**一处间隙就是一个区域。** 仅靠滞回生长无法保证这一点：配对闭合会边走边占用对侧壁的采样，于是同侧壁上后续的种子遇到这些已被占用的采样便停下，成为孤岛。带体板夹具因此得到一个 212 采样的区域外加**21 个只含一两个采样的碎片**，每个碎片都太稀疏而无法转换，于是各自向尺寸场索要跨越同一处间隙的两个单元——而带体转换本就承诺以一个单元解决它：340,360 个 LFS 源、十倍规模的网格。分割完成后，同组、同声明状态且采样相邻的区域会被合并。该关系是无向的，结果按最小成员重新编号，故合并与顺序无关（R-P2）。

为何不能只用距离：在**浅角**相交处，绕行的曲面路径与真实薄壁一样长，任何测地比值都无法区分二者；能够区分的确切事实是接触本身，而 S2 已将其算出。

**依次的门控。** 交线楔形剔除，随后碎片抑制（`speck_min_samples`，默认 12；或面积低于 `speck_min_area_factor * h^2`），随后置信度下限（`gaps.confidence_min`），再对 `Sheet` 候选执行中面构建与校验。任一门控失败即回退为 `Normal`（体网格），并以 `[THIN-SKIP]` 记录原因与各检查直方图。**回退方向恒为体网格，绝不为片体**：无效的对应关系不得迫使塌缩。

**中面。** 对壁 A 上每个已配对顶点采样取 `m_i = (p_i + phi(p_i)) / 2`，并复用壁 A 自身的三角化重新索引。**没有自身顶点采样的节点，由同一壁面上最近的一次测量定位**（同距时取更早的采样，故该选择是数据的函数而非遍历顺序的函数）。若把中面限制为仅用顶点采样，它对自己本该描述的那一种形状恰恰无法构建：长方体薄板的角点处，顶点法向是三个面的对角方向，射线因而离开间隙而非穿过它，四个角点全部未配对——在典型片体上按构造得到 `MidSurfaceUnbuildable`。片体的中面就是壁 A 沿局部间隙的一半向内平移，而这正是邻近采样所测得的量。继承的连接关系不被信任（`validate_mid_surface`）：正面积、相对源壁的定向、以 `cos^2` 判定的相邻法向偏差小于 60°、无自交（精确 `orient3d` 线段穿三角形）、边界边与区域边缘环一致、欧拉示性数与壁 A 面片一致。局部约束重三角化（阶梯的第一级修复）**推迟至 G7**；当前无效候选直接回退为体网格，这是安全方向。

**边缘环。** 壁 A 面集的边界边（关联度为 1），自最小节点键起确定性链接。

**耦合循环**（`src/meshgen/sizing.rs`）。`couple_gap_and_sizing` 实现冻结规则 `h^(n+1) = max(h_min, min(h^(n), C(R^(n))))`：运行最小值与下限由驱动自身施加，故试图抬高 `h` 的约束无法破坏单调性。守卫：发生**收紧**的区域立即锁定；与两次迭代前状态相同的区域锁定为 `Normal` 并 WARN；迭代上限（5）将仍不稳定的区域锁定为 `Normal`；循环后在实际场上重新检查 G-8 断言 `eps << t_sheet < t_layer <= h`，违反时返回错误而非警告。尺寸约束 `C(R)` 由 **G4-1** 提供，见下节。

值得知晓的推论：在冻结规则下，约束无法引发振荡——`h` 只降不升且规则 S3-M 固定 `t_r`，故每个区域至多变更两次状态。振荡守卫覆盖 SPEC §11.4 指出的唯一路径（滞回边界噪声），测试通过反转死区驱动之。

**s03 新增。** `band_region`（schema Dbg）按壁面携带区域 id；附加的 `thin_role` 单元数组区分普通壁（`0`）、已转换区域内的壁（`1`）与中面（`2`）；附加的 `ThinRegionRegime` / `ThinRegionConfidence` / `ThinRegionSeparation` / `ThinRegionSkip` 场数据表每区域一行。中面三角形作为普通带标签面单元产出，携带其区域构件且 `FaceTagKind = 1`（sheet）——中面正是该区域将塌缩成的片体。两项新增均记录于 `SPEC_meshgen_contracts.md` §2.1/§2.3。

**开销与并行（R-P1/R-P2）。** 所有已实现阶段均多核运行：S3 的射线投射、最近点对扫掠、测地捷径判定与校验组检查 1，S2 的按源三角形拆分与残余相交检测。它们一律采用许可形式——按项工作写入索引缓冲区并按索引顺序拼接——故提交输出与串行结果、以及任意线程数下的结果逐字节一致。

在 12,183 面的 `TestCaseIntersect1` 参考数据集上，整个 `mesh` 运行 8 线程为 **1.62 秒**（单线程 3.68 秒，CPU 310%），S2 约 0.8 秒、S3 约 0.65 秒。S3 加速 3.4 倍，其最近点对扫掠加速 5.6 倍。仅余一处串行块：G2 窄相（约 370 毫秒），它构建共享交点注册表，需三阶段重构而非简单并行映射。

两条非显然的经验：在以每线程暂存结构（`TriGrid::query_into`、`GeodesicScratch`）取代每次调用的分配之前，扫掠与测地判定完全无法加速——8 线程下瓶颈是全局分配器而非几何计算；此外，主要收益来自算法而非并行——三处 O(n^2) 扫描（按面重复的构件闭合检测、按三角形重复的注册表全扫、按点特征重复的全face扫描）占据了 587 秒 → 1.62 秒改进的绝大部分。

任意运行设置 `RUSTMSPT_TIME_STAGES=1` 可获得分阶段耗时（`[STAGE-TIME]` 流水线、`[S2-TIME]` 排布内部、`[S3-TIME]` 间隙场内部）。

验收（`tests/meshgen_thin_tests.rs`，19 项）：可解析间隙分割为 `Band` 且无中面；低于 `t_sheet` 的薄壁分割为 `Sheet`，其中面有效且中点精确落于中面；配对闭合使每处间隙仅一个区域；碎片与置信度门控失败均回退为体网格；扭曲与自交候选被校验器拒绝而平坦候选通过；s03 携带各表与中面单元；循环在三次迭代内收敛、对 `h` 上下夹紧、将振荡区域锁定为体网格、报告上限并触发 G-8 断言；两张相交曲面绝不塌缩为片体，且每个已转换区域均保持在自身状态区间内。

## G4-1 尺寸场与梯度控制（S4）

位于 `src/meshgen/sizing.rs` 中耦合驱动之侧，分三部分：**源**（何处、以何值限制单元尺寸）、**梯度场**（限制如何扩散）、**八叉树**（场存放于何处）。`s04_sizing` 预览其结果；G4-2 负责平衡该八叉树并作四面体化。

### 源——PLAN §10.6 的各项准则

| 准则 | 读取位置 | 请求值 |
|---|---|---|
| `Curvature` | **条件化后**曲面的每条光滑内部边 | 由离散半径 `R = w / (2 sin(theta/2))` 得 `2R*sqrt(f(2-f))`，`f = chord_error_frac`，`w` 为**跨越**该边的宽度；即半径 `R` 之圆在相对矢高 `f` 下的弦长 |
| `Feature` | S1 特征曲线的每个内部顶点 | 同一弦长规则作用于曲线转折角 |
| `Corner` | 每个 S1 角点与交汇点 | 其最短关联特征段长 |
| `Curve` | 每条锁定曲线：尖锐棱边、S2 交线、边缘、非流形边 | `h_max / curve_cells`，沿曲线按该间距采样 |
| `Gap` | 每个承载了保持体网格 S3 采样的壁面 | `t / gap_cells`，即跨间隙所需单元数——覆盖整个间隙**层**，而非仅采样点 |

三条排除规则与准则本身同等重要：

- **尖锐边不是曲率。** 其二面角是网格通过 S7/S8 予以贴合的特征；把 90 度转折计入曲率会把模型中每个立方体角点都拉到 `h_min`。
- **离散半径中的长度是**跨越**该边的宽度，而非该边自身的长度。** 二面角度量的是法向在跨越该边时转过多少，故与之匹配的距离是两个相邻三角形在该边上的平均高 `w = (A1 + A2) / L`，绝不是 `L`。二者仅在各向同性的三角化上一致；一旦不一致，按边长的形式就是错的：UV 球面靠近极点的短纬向边携带的却是全尺寸二面角，于是它报出趋于零的半径，把一个曲率**恒定**的曲面在极点压到 `h_min`。改用宽度后，该球面上两个方向处处都返回 `1/R`。（这也是为何不需要测地邻域估计器：缺陷在于长度选错，而非模板过于局部。）
- **曲率读自输入三角化，绝不读自排布后曲面。** 两个长度都取自*采样*，而 S2 共细化在不改变二面角的前提下分裂边：同一物理曲率在三分后的边上测得的半径只剩一小部分，因而单元也只剩一小部分。在 `TestCaseIntersect1` 上，仅此一项伪影就把每条交线周围的场压到了 `h_min`。同理，S2 交线不产出特征源与角点源——它们没有可供读取的输入三角化。
- **低于 `lfs_floor = max(eps, gap_cells * h_min)` 的分离量不是间隙。** 低于 `eps` 时两曲面属接触（S2 重合策略本就把如此接近者视为同一曲面，交线上的采样测得恰为零）；低于 `gap_cells * h_min` 时没有任何许可的单元尺寸能跨越它。两种情形下该间隙都归带状/片体模板或 G7 的阶梯处理，而非尺寸场。

**LFS 约束覆盖整个间隙层，而非采样点**（由下文 G4-4 复审发现）。点源只在该点约束场值，而两点之间 `beta`-Lipschitz 场会上升 `beta * d / 2`。由此产生两种失效，且在首次 `s04` 渲染中都清晰可见：*沿*壁面方向，场在采样之间抬升（0.02 的间隙一路升到 0.031，比间隙本身还宽）；*跨*间隙方向，场在两壁之间抬升（仅覆盖壁面时中面处为 `h + beta*t/2`，对自然取值 `h = t / gap_cells` 而言同样宽于间隙）。因此每个壁面取其各采样中最紧的请求，以 `LFS_COVER_TOLERANCE * h / beta` 的间距用重心网格覆盖，并将该网格**沿该面所测得的间隙方向拉伸**（沿对应向量，故斜交间隙也能正确跨越）。间隙内实际场值随之控制在请求值的 `(1 + LFS_COVER_TOLERANCE) = 1.5` 倍以内——`beta`-Lipschitz 场在离散覆盖的区域内不可能恰为常值，但可以控制在给定倍数内，而将该倍数减半需付出八倍的源数只换来两倍的界。`lfs_points_per_face` 与 `lfs_max_sources` 限制覆盖规模；预算触顶时以**同一个**公共间距因子放松所有面，故覆盖是均匀退化，而非按访问顺序退化。

取值达到或超过 `h_max` 的源被丢弃：场的上限本已表达该含义，源的数量因此正比于输入中*弯曲且靠近*的部分，而非其三角形总数。

### 梯度场——梯度由构造保证

```
h(x) = clamp( 对所有源 s 取 min( h_s + beta * |x - s| ),  h_min, h_max )
beta = grading - 1
```

`beta`-Lipschitz 函数族的下确界仍是 `beta`-Lipschitz，故 `|h(x) - h(y)| <= beta * |x - y|` **无需任何平滑趟次即处处成立**。在默认 `grading: 2.0` 下这正是 §10.6 的 2:1 梯度：跨越一个单元的距离，尺寸至多翻倍。由于没有松弛趟次，梯度不可能依赖迭代顺序、访问顺序或线程数——确定性问题根本不会出现。

`SizingLookup` 将源装入均匀网格，自查询点向外扩展 Chebyshev 环，一旦 `smallest + beta * (ring - 1) * cell` 达到当前最优即停止。`eval_box` 返回场在轴对齐盒上的**精确**最小值（点到盒距离有闭式解），八叉树即以此细化——"中心值减去 beta 乘半对角线"式的*下界*虽然有效但过于保守，会把均匀场处处多分裂整整一层。

### 八叉树

`build_sizing_field` 细化覆盖区域的根立方体，每层一趟并行：当单元边长超过 `eval_box` 在自身上的取值时分裂；未与区域盒相交的单元被丢弃，但**只丢弃到 `forest_level`**（即单元尺寸已满足 `h_max` 的最粗层级）为止，在此之下即使子单元探出区域也一律保留。该限制是正确性要求而非优化：丢弃集合必须是*子树切割*，否则 S5 的面规则会失效。面心为晶格角点的面走情形 Q，它会产出该面的四个边中点，而这些点之所以是晶格角点，正是因为该面对侧的四个更细邻居都存在（SPEC §3.1 L1）。一旦丢掉其中之一，粗单元就会产出一个邻居从未见过的节点——即悬挂节点。在 1 x 1 x 0.35 的区域上，不加限制的丢弃产生了 216 个悬挂节点与 8 条非流形边；子树切割则为零。因此在非立方区域上晶格每个轴向最多超出一个粗单元，这正是 S8 所修剪的对象，也是切割之前 [V3] 边界泄漏规则被推迟的原因。叶子以规范的 `(level, coord)` 顺序返回，故 `locate` 每层一次二分查找且无分配。两道预算守卫——`SIZING_MAX_LEVEL`（12）与 `SIZING_MAX_LEAVES`（1,000,000）——均按整层施行，故触及预算不会使结果依赖遍历顺序。仍粗于场所需的叶子计入 `stats.n_unresolved`，并以 WARN 指明触及的是哪道预算。

### 约束 `C(R)`

`SizingConstraint::evaluate` 即冻结驱动每次迭代所调用者：

```
C(R) = 对薄区域 r 取 min( geometry(r),  若 R(r) = Normal 则 t_r(r)/gap_cells )
```

`geometry(r)` 是与状态无关的场在区域 `r` 各采样处的最小值。转换为带状或片体的区域停止施加约束——其间隙由模板划分。保持体网格的区域必须被解析，而这正是该循环存在的意义：下降的 `h` 缩小 `t_layer`，把某区域挤出 `Band`，该区域自身的 LFS 需求又进一步单调压低 `h`。

有两处读法须予以裁定，且因其并非冻结文本所强制而记录于此：

- **标量 `h` 是薄特征的主导尺寸，而非全局网格尺寸。** `C(R)` 仅对*薄区域*取最小。冻结规则将 `h` 写作标量，但配置草案称阈值为"x 局部 `h(x)`"；若改取全场最小值，模型中任意一处尖锐特征都会处处压低阈值，从而否决模型中的每一次片体转换。场本身保持空间分布——只有阈值读取单一数值。
- **被 S3 否决的区域不参与。** 它以 `t_r = INFINITY` 进入，`regime_for` 据此判为体网格，且不贡献 LFS 项。由于 `C(R)` 是全模型唯一标量，让 S3 判定为不可靠的测量来设定它，就意味着一处坏测量否决模型中所有合法转换——在加入该排除之前，一个六采样的碎片正是这样否决了 `TestCaseIntersect1` 全部八个带状区域。该排除不会造成欠解析：这些采样仍通过 `gap_sources` 产出 LFS 源，故*场*仍按实测在其周围细化。
- **但 `Speck` 与 `Undersampled` 例外，二者同样不发出 LFS。** `Speck` 是**面积**小于一个 bootstrap 单元的区域，`Undersampled` 则是面积真实、但采样数少于 `speck_min_samples` 的区域；二者分开上报是因为含义不同，但都予以抑制：其很小的 `t` 描述的是一个退化或未被测清的区域，而不是需要跨越单元的间隙；`t / gap_cells` 会把该特征周围的场压到 `h_min`，而它并没有提出任何要求。A-7b 曾构建 **361,163 个源，其中 360,835 个来自板边的 speck 区域**，把 `h` 从 0.069 压到 0.010，两个立方体产生 120 万个四面体；抑制后只剩 17,148 个源、10.5 万个四面体，并且流水线会打印 `[S4/G4-1] LFS suppressed: N sample(s)`，使得被声明区域附近偏粗的结果可归因。抑制 `Undersampled` 同样要紧：留在一个**已转换**带体区域旁的一两采样碎片，本就是同一处间隙的一部分，让它索要 `t / gap_cells` 便是在带体转换已承诺以一个单元解决的间隙上再放两个单元——a7b 由此产生 340,360 个 LFS 源，源头是同一处已被正确测量的间隙的十七个碎片。稀疏到 S3 不敢据以行动的测量，也稀疏到不该把场绑到它的下限上。`LowConfidence`、`MidSurfaceInvalid` 与 `IntersectionWedge` **不**被抑制——它们是薄路径未能接手的真实间隙。2026-08-07 曾尝试连 `IntersectionWedge` 一起抑制，精度出现可测量的回退（A-3 0.68 % -> 0.90 %，A-8 4.77 % -> 8.45 %）：楔形处的 LFS 在相交处确实在起作用，而 `curve_sources` 的 `h_max / curve_cells` 比它要求的更粗。已回退。

流水线会打印约束的绑定区域及绑定项（`binding_region`）：在全模型只有一个标量的情况下，这是"网格为何偏细"可解释与不可解释之别。

### `s04_sizing`

`sizing_to_doc` 为每个叶子产出一个 `VTK_VOXEL` 单元，`cell_kind = 3`，并携带 `sizing_h` 点数组（SPEC_meshgen_contracts §2.2/§3）。角点在八叉树自身的 `max_level` 整数格上去重，故跨层跳变共享精确坐标而非近似相等的浮点数，[V2] 找不到重合节点；某点的取值取自与之相接的叶子中的最小者，这使层级跳变在热图中清晰可辨。文档中没有四面体，故 [V1]/[V3]/[V4] 无可检查，四面体专属数组一律携带哨兵值。

`build_scene` 同步获得体素支持：恰被一个*已选中*体素引用的四边形面即为所选子集的边界，故 `clip_plane` 与 `bbox` 过滤器会切入八叉树、暴露其内部层级跳变而非将其掩盖——与四面体既有的 crinkle-clip 语义一致。

### 配置

`sizing.grading`（默认 `2.0`，须 `> 1`）与 `sizing.gap_cells`（默认 `2.0`，须 `>= 1`）是 G4-1 对 §6.3 块的两项新增；两者均计入 `ConfigHash`。

### 并行（R-P1/R-P2）

曲率源为按面映射进索引缓冲区加一次并行排序；间隙源为按采样映射；八叉树每层一次 `par_iter` 并按索引顺序消费。场的每个取值都是 `min` 折叠——精确，因而与顺序无关——没有任何浮点归约跨越线程边界。

验收（`tests/meshgen_sizing_tests.rs`，25 项）：空源集为常值上限；场以恰为 `beta` 的速率增长并在两端夹紧；Lipschitz 界在一组刁钻源上的 900 对探针间成立；`eval_box` 与暴力扫描一致；均匀场产出比 `h_max` 低一层（而非两层）的均匀格；八叉树在源处细化并向外分级；叶子铺满区域且每个叶子可定位；两道预算均报告而非静默截断；非立方区域被覆盖且外部单元被丢弃；立方体无曲率但有角点；球面的中位请求值符合 `2R*sqrt(f(2-f))` 且在输入加密后**不变**；LFS 请求 `gap/gap_cells`，已转换区域与接触均不请求；`C(R)` 在无区域时为上限、对体网格区域下降、忽略已转换区域、忽略被否决区域；循环对真实约束在 3 次迭代内收敛；s04 通过校验、验证无 FAIL、角点去重并可渲染；源与叶子在多次运行之间、以及默认线程池与单线程池之间逐位一致。

## G4-2 背景晶格（S5）

位于 `src/meshgen/lattice.rs`。该阶段完全是组合性的：晶格的每个节点——单元角点、被分裂边的中点、面心、单元形心——都精确落在步长 `h_min/2` 的整数格上（SPEC_meshgen_geometry §3.2），故本模块全程工作在该**倍化索引空间**中。层级 `L` 的叶子步长为 `1 << (max_level + 1 - L)`；所有中点与中心均为整数点；`orient3d` 是精确的 `i64` 行列式。晶格的任何判定都不查询容差，也都不需要邻居行走。

### 强 2:1 平衡

`balance_octree` 持续细化，直到任意两个闭包围盒相接触（面、**边**或**顶点**）的叶子层级相差不超过 1。仅面平衡是不够的，而且差别并不学究：比 `C` 细一级、仅与之棱接触的叶子，仍会在 `C` 的边内部放置一个节点——这正是面规则无法察觉的悬挂节点来源。

涟漪算法：对层级 `L` 的每个叶子，考察其 26 个层级 `L` 的邻*单元*；若包含某邻单元的叶子比 `L - 1` 更粗，则分裂之；迭代至不动点。该算法是完备的——若两叶子违反规则，无论以何种方式接触，较粗者必包含较细者的某个层级 `L` 邻单元——且必然终止，因为每趟都严格加深某个叶子，而 `max_level` 是上界。若某邻单元**没有**包含它的叶子，则它要么已细化到 `L` 以下（下一趟从细的一侧处理），要么在区域之外（那里什么也没有），故两种情形跳过都正确。分裂产生的子单元继承父的 `h`，这既保持了"叶子不大于其内部场值"的不变量（子单元只有一半大），又使平衡独立于它所平衡的场。

### 分裂状态与两类模板

`split(edge)` 与 `split(face)` 是把边中点/面心拿去与叶子角点集合做成员测试——精确整数查找，且共享该实体的两个单元求值完全一致。**没有**分裂面且**没有**分裂边的叶子采用冻结的 6-tet Freudenthal 表；其余叶子皆为扇形单元。"单条分裂边即已足够"这一点是承重的：正是它阻止了 Freudenthal 邻居在某条边带有中点的面上画出朴素对角线。

面规则 `f(F)` 即冻结的 SPEC §3.3 表——情形 **P**（规则 D 的最小角到最大角对角线）、情形 **E**（自新面心对边界多边形扇形化）、情形 **Q**（四个象限，每个在层级 `L+1` 上按情形 P 处理）——扇形单元对每个面的每个三角形发出 `(t0, t1, t2, 形心)`，并施加规范定向修正。由于 `f` 是该面**全局**坐标的纯函数，共享一个面的两个单元无需通信即产出相同三角形。这就是不变量 C，而定理 T1（协调性）由它加上强平衡即可推出。

### 质量——对冻结预测的一处更正

SPEC §3.7 的 Q 行原为 `45.000° / 90.000° / AR 1.3938`。在全部八个象限三角形上实测的结果是 **`35.264° / 125.264° / AR 1.6052`**——即 `arctan(1/√2)`，出现在规则 D 对角线背离形心的那两个象限上。原行填的是"中心—角点—中点"行的数字。**规则未作任何改动**；该值由规则 D 与 §3.4 强制决定。规范已在 rev 1.2 更正，验证记录见其 §14 [8]，且此处以测试固定该更正。

因此过渡扇形**确实**牺牲最小二面角——45° → 35.264°，下降 22%——并使纵横比增加 15%。两者都远优于任何可用的 FEM 门限（[V4] 默认 `low_dihedral_deg` 为 5°），故这更正的是一个论断，而非结论走向。

### 产出顺序与去重

单元按 Morton 序升序处理（SPEC §1.4），模板行按表序产出。节点经两趟并行收集——第一趟收集全部角点、扇形形心与情形 E 的面心；排序去重后的结果**就是** `NodeKey` 序，因为晶格坐标是精确整数、无需量化；第二趟重新求值同样的模板并以索引产出四面体。把模板工作做两遍比为每个四面体物化一组坐标四元组更省，且两趟都是同一输入的纯函数。

### `s05_lattice`

`lattice_to_doc` 写出 `VTK_TETRA` 单元，`cell_kind = 0`、`region_key = 0`（背景集合）、`regime = 0`、`partition_id = 0`，并携带 `sizing_h` 点数组。

> **对 SPEC_meshgen_contracts §3 的记录性修订。** 冻结文本称 s04 **与** s05 预览均写 `cell_kind = 3` 体素单元。此处 s05 改写四面体：晶格的全部交付物就是它的四面体化，而 G4-2 的验收标准是读取四面体的 [V3]。对一个输出为四面体的阶段做体素预览，会使该阶段无法被验证。s04 保持冻结的体素形式。

`partition_id = 0` 是正确值而非占位：在没有片体标签面的情况下，整个晶格是单个片体阻断分区，这正是 [V8] 重算并比对的对象。

### 预算

`LATTICE_MAX_TETS`（2000 万）约束产出量——仅靠叶子预算是不够的，因为一个扇形单元最多产出 48 个四面体。超出预算时给出具名错误，指明叶子数与应调整的旋钮，而不是分配失败。

### 实测

在 `TestCaseIntersect1` 参考数据集上（100³ 区域，三个物体）：161,176 个尺寸叶子 → 1,095 次平衡分裂后为 168,841 个（140,497 Freudenthal，28,344 扇形）→ 297,771 个节点与 **1,651,366 个四面体**，构建耗时约 0.45 秒（平衡 0.21 秒，四面体化 0.24 秒）。`mesh-verify` 报告 `[V3] multi_shared_faces=0 boundary_leaks=0 hanging_nodes=0 non_manifold_edges=0`，`[V4] worst_aspect_ratio=1.60517 min_dihedral_deg=35.26439 aspect_ratio_over_gate=0 below_low_dihedral=0`。`s04` 与 `s05` 在 `RAYON_NUM_THREADS` 为 1 与 8 时逐字节一致。

验收（`tests/meshgen_lattice_tests.rs` 13 项，另加模块内 3 项表格测试）：冻结的 Freudenthal 表为正且精确铺满立方体；规则 D 复现 SPEC §2.3 的子三角形表且对四边形的旋转不变；均匀八叉树无需平衡；深点细化能被平衡且只增不减；角对角细化被捕获（强平衡情形）；均匀晶格全为 Freudenthal 且精确铺满区域、每个四面体定向为正；分级晶格保持在冻结的 6 / 18..48 清单与界 P1 之内；节点互异且按 `NodeKey` 序排列；更正后的质量表被固定；定理 T1 在 **12 个随机尺寸场**（每个跨越 ≥ 2 个八叉树层级，合计 > 100 个扇形单元）以及仅棱细化模式上直接成立并通过 [V3]；s05 通过校验、验证无 FAIL 且可渲染；四面体预算给出报告而非耗尽内存；整个晶格在多次运行之间、以及默认线程池与单线程池之间逐位一致。

## G5-1 分类（S6）

位于 `src/meshgen/classify.rs`。对每个晶格顶点与每个**实体**构件判定内外；据此为每个四面体播种一条归属记录；并标出后续阶段可跳过的曲面片。

### 精确化的奇偶判定

判定方式是奇偶计数：自顶点沿固定方向发射射线，统计其穿越该构件面片的次数；奇数即在内部。每个候选三角形耗费五次 `orient3d`——两次判定线段是否跨越三角形所在平面，三次判定直线是否穿过其内部——每一次都经由 §6.1 静态滤波并以精确谓词兜底。因此结论是拓扑事实，而非容差判断。

候选来自**投影网格**：三角形按其在垂直于射线方向平面上的投影足迹装桶，故一条射线即化为一次二维点查找，无需遍历。每个方向一张网格，按构件构建一次。它始终只是超集过滤器——每个候选仍要走精确判定——故其分辨率只影响速度，不影响结果。

### 两类退化并不相同，而这正是设计的关键

擦过几何的射线会使某个 `orient3d` 恰为零，此时奇偶计数不是不精确而是无意义。这有两种情形，且需要相反的处置：

- **顶点恰好落在某三角形所在平面上。** 对于在晶格上剖分的轴对齐输入，这是*常见*情形而非边角情形——立方体面在 `z = 0.5`，晶格平面也在 `z = 0.5`。**重发射无济于事**：平面判定根本不含射线方向。其解法是标准的符号扰动——对无穷小 `delta > 0` 判定 `p + delta*direction`。由于 `orient3d(a, b, c, .)` 关于末位参数是仿射的、梯度为三角形法向，扰动后的符号即 `sign(n . direction)`；又因整条射线共享同一方向，扰动点是一个位于曲面外侧的一致点。若改按退化处理，复审场景中 **1.1%** 的判定会落到卷绕数；加入扰动后同一场景 **100%** 在首条射线上解出。
- **射线恰好穿过三角形的某条边或某个顶点。** 沿方向扰动并不移动该*直线*，故此情形确实需要新方向：放弃该射线并沿冻结的 `RAY_DIRECTIONS` 序列重发射（ARB-9）。仅当五个方向全部退化时，该顶点才落到广义卷绕数，并给出 `[CLS-BAND]` 警告。

### 三类构件，三种处置

| 分类 | 路径 | 理由 |
|---|---|---|
| `SolidClosed` | 奇偶判定 | 闭合性已认证，故奇偶性有定义 |
| `SolidDefective` | 卷绕数 | S2b 无法认证其闭合性，故奇偶性无定义（§10.4） |
| `Sheet` | **根本不参与分类** | 无论任何卷绕数证据，片体都不得占有体积——§10.4 的硬性守卫，亦即 SPEC_meshgen_geometry §9.1 第 10 行 |

### 记录与 `resolve()`

每个四面体获得一条稀疏 `OwnershipRecord`，由 `(X, Side)` 条目构成，来源为 `Lattice`；缺省即为外部，这正是它在以背景为主的晶格上保持稀疏的原因。当**四个**顶点全部在内时该构件占有此单元，四个全部在外时条目缺省，意见不一时为 `Ambiguous`——而最后这一集合恰是 S8 必须切割的单元。

`resolve()` 实现冻结的真值表（SPEC_meshgen_geometry §9.1）：取内部集合，仅保留其中优先级最小的成员，集合为空时回落为 `{0}`。这一条规则即同时给出 R-A4（高优先级实体在重叠区取代低优先级者）与 R-A3（同优先级重叠保留全部 X）。`Ambiguous` 条目绝不进入该集合——在 S6 它们意味着"由切割裁定"——且 S10 必须拒绝仍携带此类条目的记录。

### 活跃面片过滤

严格位于某更高优先级实体内部的面，其两侧解析标签相同，因而并未分隔任何东西；它被标记为不活跃，细化、吸附与切割都会跳过它（PLAN §5.2）。片体面恒为活跃——它本身即是特征，而非材料边界。

### `s06_classified`

`classified_to_doc` 写出 `VTK_TETRA` 单元，携带真实的 `region_key` 及其区域集合表（`RegionSetPriority` 记录该键中全部 X 共享的唯一 `Y`，背景为 `0xFFFFFFFF`，这正是 [V6] 所检查的），另有 `provenance` 与 `arbitrated`，后者标出跨越曲面的单元。该键是**初步**的：跨越曲面的单元由 S8 的切割最终裁定。

### 并行（R-P1/R-P2）

顶点并行分类，各自写入预分配缓冲区中自己的切片，且只读取共享的不可变几何；逐顶点的诊断以索引缓冲区返回并串行折叠。记录、区域键与活跃面掩码均为按项映射。奇偶性是*计数*，故候选顺序无从影响它；方向序列亦固定——两次运行、两种线程数会走同一分支。

验收（`tests/meshgen_classify_tests.rs`，13 项）：`resolve()` 对冻结真值表全部九个可表达行的复现，以及外部/歧义条目绝不占有单元；立方体与球面**按体积**与解析值比对分类（这是可得的最强检查——分类体积不得超过精确值，且亏空不得超过跨越层）；一个二进分数构造的夹具，先*断言确有晶格顶点恰落在某面所在平面上*，再断言其零重发射、零回落地解出；一切之外即背景；优先级取代与同优先级保留；片体守卫；被掩埋的面片失活而其容器保持活跃；记录播种逐单元与顶点分类一致；s06 通过校验、验证无 FAIL（含 [V6] 的合法性与优先级检查）并可渲染；以及多次运行之间、默认线程池与单线程池之间逐位一致。

## G6-1 吸附（S7）

`src/meshgen/snap.rs`。这是第一个真正*移动*几何的阶段。S5 的晶格完全不看输入，S6 只为其贴标签，因此在 S7 之前网格只能以阶梯逼近输入；S7 把真正要紧的顶点拉到曲面上，使 S8 的切割落在已然精确的节点上。

### 本阶段的判定及其依据

`SPEC_meshgen_numerics.md` §5 的四条 S7 行按原文实现：

| 判定 | 方式 | 类别 |
|---|---|---|
| 边是否存在交点 | 每个候选面五次 `orient3d` 符号——两次跨面判定、三次直线侧判定 | **X** 精确 |
| 吸附目标优先级 | 角点 > 曲线 > 曲面，其次最近，再次 `NodeKey` 升序 | **I** |
| 移动是否翻转 | 移动后每个相邻四面体 `orient3d > 0` | **X** 精确 |
| 位移上限 | 相对 S5 位置的总位移 `<= 0.3 * L_min` | **A** |

只有*存在性*是精确的。交点本身是 C4 类构造（f64，仅关乎精度）：其有效性由精确的翻转判据重新确立，绝不假定。

### 捕获规则，以及曲面为何不作捕获目标

角点与特征曲线在上限半径内被捕获：任何距其 `0.3 * L_min` 以内的晶格节点都会被拉过去，角点优先。曲面则不然。把上限内的每个节点都吸到最近面片上听起来更强，实则具有破坏性——当单元尺寸与特征尺寸相当时，多数被穿越边的*两个*端点都落在上限内，于是每个交点都变成在切面上的顶点，S8 本要切割的曲面就此消融进晶格。在 `h = 0.25` 的球面上实测：**吸附前 74 条被穿越边，吸附后 2 条**。只有当切割本会过于贴近某节点时，该节点才取曲面目标：

- 位于**复核带内**（距端点 2.5% 以内）的交点会给 S8 留下薄片，故把端点拉到该交点上；
- 位于端点**焊接容差内**的交点属不变量 K2 的提升情形（`SPEC_meshgen_geometry.md` §5.1）：切割节点*就是*母节点，把母节点再移动那最后 `eps`，才使这一点在坐标上而不仅在记账上成立。

### 什么才算角点

`ArrangedSurface::corner_nodes` **并非**尖锐角点集合。S2 把 S1 的角点与结点，与*每一个*点特征的节点取并集，而 C6「切触点接触」情形会对同一构件中仅共享一个顶点的任意两个三角形触发——这不过是顶点扇周围的普通网格邻接。在 12 带的普通 UV 球面上，这意味着 266 个顶点上有 2688 次 C6 事件：S1 报告 **0** 个角点，而 S2 的 `corner_nodes` 仍包含球面的**每一个**顶点。因此 S7 会丢弃那些仅凭单构件点特征入选的节点，除非它同时落在某条特征曲线上。这不会丢失任何东西：真正的自接触会使共享顶点成为非流形，而 S1 本就会把非流形顶点作为结点报告。

### 约束与域盒

位于域面上的节点只能在该面内移动；位于域边上的沿边移动；位于域角的完全不动。判据是精确相等，这正合适——晶格在域盒上是二进分数的，故域面上的节点该坐标精确成立，而仅仅四舍五入到该面的节点本就在内部。凡未取到目标却位于边界上的节点记为 `constraint_kind = 4`，正是它使 S8 与 S9 不会让域盒变形。

### 确定性（R-P1/R-P2）

确定性来自流程的形状，而非运气。每个移动*提案*都是吸附前状态的纯函数，并行计算；而*接受*是按节点索引升序的串行扫描，因为一次移动是否翻转四面体，取决于该四面体的其他角点是否已经移动过。并行接受会让结果依赖调度器。串行部分很廉价——每个候选节点几次 `orient3d`——且只有候选节点参与。交点扫描则是写入索引缓冲区、按索引顺序拼接的并行映射。

### `s07_snapped`

`snapped_to_doc` 在 S7 的坐标上写出 S5 的连接关系，携带 S6 的（仍属初步的）区域键、本阶段产出的 `constraint_kind`/`constraint_ref`，以及 `snap_motion`——一个调试用点数组，按 `separation_t`、`sizing_h` 的先例，作为对 `SPEC_meshgen_contracts.md` §2.2 的**追加修订**记录在案：节点移动了多远是评审最需要的量，而它无法仅由快照反推。

### 实测（G4 评审场景，268,948 个四面体）

| | S7 之前 | S7 之后 |
|---|---|---|
| 交点数 | 24,670 | 23,610 条边上 23,766 个 |
| 位于曲面上的节点 | 1,099 | 1,354 |
| 最差长宽比（[V4]） | 1.6052 | 2.3220 |
| 最小二面角（[V4]） | 35.264 度 | 24.349 度 |

305 个节点发生移动（33 个到角点，272 个到特征曲线），最大位移 8.8e-3，平均 2.1e-3；无一被上限截断，无一被拒绝。`[V4]` 仍然通过，长宽比越界与低二面角占比均为零——此表正是 **G6-6** 所需数据的切割前一半。另有 156 条边被同一构件穿越两次（不变量 K1），已报告交由 S8 细化或升级处理。

验收（`tests/meshgen_snap_tests.rs`，12 项）：无四面体翻转；无节点越过其上限，且每个被截断的节点都为 [V5] 门列出；带曲面约束的节点由*独立实现*的点—三角形距离验证确实落在曲面上；立方体的角点被精确捕获，其尖锐棱边亦被捕获；证明域边界节点不会离开其所在平面；复核后带内交点为**零**，且残留计数与之相符；精确翻转判据接受小位移、拒绝共面与穿透两种移动；每个交点都与输出坐标及其所属面的平面一致；薄于一个单元的板件产生 K1 升级*并*给出告警；全不活跃场景不移动任何节点；多次运行与不同线程数逐位一致；`s07` 验证无 FAIL 项。


### 曲线覆盖率已被测量，而它当前接近零

以往的捕获是机会性且未经测量的：该阶段吸附恰好在范围内的东西，没有任何检查验证
结果，因此"网格与锁定曲线共形"只是一个假设。`curve_coverage` 现在对其进行检验。
当一条网格边的两个端点都落在锁定线段的 `eps` 之内，**并且**该边自身长度与其端点
投影所张的跨度相符时（这会排除离开曲线又折回的弦），它就**覆盖**了该线段的一部分。
覆盖区间在参数空间中取并集，因此由若干较短网格边共同承载的线段也算覆盖。
`SnapStats::n_curve_segments` / `n_curve_segments_covered` 记录结果，流水线打印
`[S7/G6-1] curve coverage: C of N`，不足时发出 `[SNAP-CURVE]` 警告。

**2026-08-07 实测：A-1 0/0，A-2 12 中 0，A-3 122 中 0，A-4 12 中 0，A-8 1,404 中
16，A-6a/A-6b 35 中 1，A-7a 24 中 0，A-7b 24 中 1。** 一个普通立方体的十二条锐边
没有一条由网格边链承载。仅仅朝曲线加密只会让情况**更糟**——`curve_sources` 落地
后 A-3 的曲线吸附数从 40 降到 16，因为运动上限是 `SNAP_MOTION_CAP * l_min`，会随
`h` 一起收缩。恢复是一个加密与插入问题，而不是"吸附得更用力"。

### K1 的补救措施，以及它确立的边界

`SPEC_meshgen_geometry.md` §5.1 为 S7 的两类升级集——被同一分量多次穿越的边，以及
未被覆盖的锁定曲线段——给出同一条补救措施：*"加密一级；若已达到尺寸下限，则将这些
单元交给 §7。"* 该措施在 2026-08-07 之前被完全跳过。流水线现在将其实现为围绕
S4-S7 的循环（`K1_MAX_PASSES = 3`）：S7 返回 `Snapped::refine_requests`——失败处的
世界坐标点及其长度尺度——场在每个点上被重新要求该尺度的**一半**，然后重跑 S5-S7。
已处于 `clamp_h` 下限的请求不会生效，于是循环停止、升级保留：这正是规范的第二个
子句，以 `[S7/K1]` 报告。

**它能恢复的：** A-2 覆盖率 0 -> 12 中 6，体积误差 0.076 % -> **1.4e-14，精确**。
A-7a 0 -> 24 中 4，A-7b 1 -> 24 中 7，A-8 1,404 中 16 -> 24（4.77 % -> 4.15 %），
A-6a 5.61 % -> 2.43 %。代价是单元数增加 3 % 到 25 %。

**它不能恢复的（现已在两类曲线上得到确认）：** 即使强制 `h = 0.0052`——比相交曲线自身
0.012 的折线离散还细四倍、共 148 万个四面体——A-3 仍停留在 **122 中 0**。这很容易归因
于曲率：晶格边链无法跟随一条在每个排布段之间都发生弯折的曲线。但 A-6a 的**接触边界是
笔直且轴对齐的**，属最有希望成功的情形，而将其加密 2.5 倍（`h = 0.001386`，**670 万
个四面体**，原为 27 万）后，覆盖率从 **35 中 1 变为 35 中 0**。

因此限制并非来自曲率。覆盖率要求*晶格上相邻的连续*节点恰好落在同一条线段上，而把单个
节点向曲线吸附在任何 `h` 下都无法形成这样的链——A-2 能达到 12 中 6，仅因其立方体是
场景中唯一的物体，整排晶格节点可以毫无阻碍地被拉到棱上。把曲线作为网格边插入属于
**带 Steiner 插点的约束边恢复，而这正是 G6-0 关卡有意未采纳的方案**
（`PLAN_mesh_generation.md` §0.4），转而采用共形质心扇形。请将低覆盖率理解为这一架构
边界，而非加密不足，也不要再为此投入更多单元。

### 自相交实体的内部面（S6）

`active_face` 只会停用位于**更高优先级的其他**实体内部的面，并且按构造跳过面自身的
分量——因此埋在其*自身*分量重叠区内的三角形始终保持激活。S7 在其上进行切割，而 S6
以缠绕数标注时正确地报告边的两个端点都在内部：`[S67-SITE] no-ambiguous-component`，
占 **A-8 的 2,478 次升级中的 2,184 次**，每一次都被扇形化并削角。

并集的边界正是内外性发生变化之处，因此判据就是它：在三角形的四个重心采样点两侧各偏移
`ACTIVE_FACE_PROBE`，对该分量自身的面调用 `winding_inside`。所有采样必须一致：S2 不会
将一个分量与其自身互相求交细化，因此三角形可能*部分*被埋没，而让混合三角形保持激活是
保守方向（在部分内部的面上切割只需付出一次升级，丢弃边界面则损失材料）。干净的闭合实体
永远不会触发它，因为每个边界面总有一侧在外部。

**A-8：非激活面 0 -> 576，升级单元 3,687 -> 853，体积误差 4.15 % -> 3.73 %，四面体
327,699 -> 303,919**，`[V3]` 干净，`[V6]` 为 0，R-P2 逐位一致。

残留下来的是 **K1 口袋**：被同一分量穿越两次的边，其两端点位于同侧，因此 S6 是正确的、
并不存在分歧——曲面探入单元又退出，留下一个闭合气泡，而整个单元*边界*都在一侧。扇形会
把这样的单元整体判给一侧从而丢失该口袋，§7.6 则回答 `one side empty`。**它受尺寸下限
约束，并且是收敛的：** `h_max_frac` 0.05 -> 0.025 使误差从 4.148 % 变为 4.090 %
（几乎不变），而 `h_min_frac` **0.012 -> 0.004** 使其从 **3.733 % 降到 3.007 %**。该损失
是削角，并如削角应有的那样随 `h` 缩放，因此 A-8 的表头数字是其夹具设定的下限，而非缺陷。

### §7.6 的顺序切割

质心扇形放弃了升级单元**内部**的切割，这正是材料边界被削角、并使相邻四面体跨越一个
未声明的面发生双分量跳变的原因。§7.6 的补救措施是改为沿穿过该单元的曲面切开它。
`split_soup_by_surface` 按单张曲面划分该单元的共形边界三角汤——一个三角形取其非
曲面节点的一致侧——并报告它留下的**每一个**封盖多边形；`split_escalated_cell` 对每个
穿越分量施加该操作、为每块封盖，并且只有在孔洞都是**两侧一致认可的简单环**、且各块的
扇形体积之和仍**等于父单元体积**时才保留结果。第二项检查用于捕获质心落在块体之外的非凸块。任何
拒绝都会回退到整体扇形，后者的有效性是无条件的。

它最初触发次数为**零**，原因已被实测：A-3 的 884 次拒绝中有 840 次是
`mixed sides on one triangle`。这指一个边界三角形的节点分处曲面两侧，而它们之间没有
切割节点——因为 `face_mesh` 只按*单个*分量的 `FaceCutState` 划分面，并把两张曲面穿过
的面交给 `LoopFan`，其三角形跨越两处切割，与任何一方都不对齐。**单元级的切割无法修复
面级的不对齐。**

`crossed_face` 修复了这一半。两张面片穿过的面现在把两条弦保持为三角剖分的**边**：

- **相交的弦**（其端点沿边界行走互相穿插——这是组合判据，因此两个单元不可能产生分歧）
  在 `chord_meeting_point` 处相遇，即两条线段按规范顺序取得的最近接近点。该点正是 S2
  交线穿刺该面之处。四个弦端点把行走切成四段弧，每段经由该点闭合成一个扇区。
- **不相交的弦**只是两条互不相交的对角线，无需新节点：用第一条切分行走，用第二条切分
  包含它的那一半，再从最小键顶点扇形化这三块。这是常见情形——A-3：960 个面对 132 个。

A-3 实测：**624 个面对齐，通用面 832 -> 208，`[V3]` 干净，181,098 个四面体（原
182,082），分量 2 的体积误差 0.407 % -> 0.393 %**，`[V6]` 169 -> 177。

**单元级切割现已默认开启。** `split_escalated_cell` 将 A-3 的 94 个升级单元切成 284
块材料，使 **`[V6]` 从 169 降到 146**，`[V3]` 干净，R-P2 逐位一致。除面三角剖分之外，
还需要两项修正，二者都由实测发现：

- 面的**相遇点同时位于两张曲面上**。若只把 `cut_index` 中的节点标记为位于曲面上，
  第二个分量就会在恰好位于其自身曲面的点上向分类器询问侧别，而答案取决于谓词的舍入
  方向——由此产生 226 次 `one side empty` 拒绝。
- 当另一张曲面穿过封盖时，**封盖不能锥化到新的中心点**：那会让一个三角形横跨交线，
  第二次切割在两半上都被拒绝，单元最终只有两块而非四块。改为沿封盖自身的相遇点切分，
  可使交线保持为边，且无需新节点。

保留三道守卫，各有其理由。封盖孔洞必须是两侧一致认可的简单环；各块的扇形体积之和
必须仍等于父单元体积，这可捕获质心落在块外的情形；当**两个分量**共处一个单元时，它们
必须在单元内部相遇。最后一条区分了结点与间隙：两个*不同物体*的壁面穿过单元却不相遇，
属于 S8b 的带状阶梯——在加入该守卫之前，切分 A-7a 的 512 个平板单元使体积误差从
0.36/0.10 % 变为 0.57/0.44 %。它统计的是不同的**分量**而非弦：同一分量穿过一个面两次
是薄板自身的两道壁，并不存在可被误认的间隙。
`RUSTMSPT_NO_JCT_CUT=1` 可恢复旧的整体扇形；`RUSTMSPT_JCT_DIAG=1` 可打印拒绝原因。

#### 一张曲面，多个孔洞

跨骑在薄板上的晶格单元被该板的曲面穿越**两次**，因此两壁之间的材料由两个封盖围成，
而外侧则是两块互不相连的实体。假定只有一个封盖，就会把这种情形读成"不是单一简单环"
而拒绝，于是每个这样的单元都回退到把间隙削掉的扇形——这正是"声明了带体却没有带体单元"
的成因：片体夹具 538 个升级单元中有 512 个被 §8.2 表以 `FaceShape` 拒绝，且全部被扇形化。
三项改动将其打开，且都不需要新的表行：

- `split_soup_by_surface` 报告每一个封盖，且两侧必须报告**相同**的封盖集合——这正是让
  切割成为一组共享面、而非一道裂缝的依据。
- `split_soup_components` 把每一侧拆成按边连通的各块，使外侧两块互不相连的实体不会
  被当作携带同一材料标签的一整块。
- 三个角点**全部**位于曲面上的三角形，改由其自身形心判别。被两条弦夹住的那条窄带，
  其角点全在曲面上，却实实在在位于两壁之间的材料内；在此处拒绝就等于拒绝了每一个
  带体单元。

`crossed_face` 也不再拒绝同一分量在一个面上留下**四个**切割节点的情形，而是描述为两条弦。
两道壁不可能在面内相交，而跨两条被切边配对后恰好只剩一种不穿插的选择——按排序后的行走
位置取嵌套对 `(p0,p3)`、`(p1,p2)`。这是组合判据，故共享该面的两个单元仍然一致（不变量 J1）。

实测：片体夹具由 0 增至 **66 个单元、切成 132 块材料**，带体夹具由 0 增至
**1,089 个、2,178 块**，片体夹具分量 2 的体积误差 **2.459 % -> 1.713 %**，四面体数反而
少了 12 %，`[V3]` 干净。

#### K1 第二切割已默认启用

把不变量 K1 的**第二个**交点作为真实切割节点保留下来，可把上述数字提升到 145/372 与
2,706/7,029。它现已默认启用，`RUSTMSPT_NO_K1_SECOND_CUT=1` 可关闭，作为二分排查的开关。
它曾被门控一轮，因为当时会破坏共形性；其背后共有四个独立缺陷，现已全部修复。每一个都是在
一次错误猜测之后才找到的，故连同猜测一并记录。

**1. 沿行走边的弦。** 一条边上带两个切割节点，会给该面的边界环贡献三个共线点
（`a, c1, c2, b`），扇形化即产出零面积三角形——`c1` 位于 `ab` 上时的 `(a, b, c1)`——
且该边周围每个单元都产出同一细片：A-4 报告面 `(61918, 66209, 303387)` 被 **6 个四面体**
共用，408 个多重共享面。`crossed_face` 现已拒绝两端点在行走中相邻的弦，该面转由 `LoopFan`
处理，其顶点为面形心、不在该直线上。*多重共享面 408 -> 0。* 两处近失：**精确**的零面积
判据检测不到它们（`c - a` 各分量仅差一个 ulp，叉积极小却不为零，故第一版过滤器与不过滤
逐位一致）；而*丢弃*细片即便奏效也是错误修法——它带来 1,488 处边界泄漏，因为这些三角形的
边随之无可匹配。正确的修法是不去产生它们。

**2. 闭合却无法扇形化的碎块。** `split_escalated_cell` 原本只校验每块碎片是闭合曲面，
而这并不充分：与碎块自身形心共面的边界三角形会给出零体积四面体，`orient_positively`
将其**丢弃**而非产出，于是在刚刚通过闭合校验的碎块上凿出一个洞。日志本已明言——
`[JCT-DEGENERATE] 588`，恰好对应 **588** 处界面裂缝，一一对应。`fan_is_sound` 以
`orient_positively` 所问的同一精确判据发问，改为拒绝该次切分；拒绝只让该单元失去切割，
别无代价，因为回退的扇形化无条件有效。*边界泄漏 852 -> 0，界面裂缝 588 -> 0，
非流形边 135 -> 0。*

**3. 落在单元自身面上的盖心。** 把盖锥化到一个新的中心点，等于引入只有该单元知道的节点。
当曲面从同一个面进入又离开——正是不变量 K1 的情形——盖与该面共面，于是中心点落在该面
**之上**，而对面那个按无此节点剖分该面的邻居就多出一个悬挂节点：A-4 上 180 个、
A-6a 上 871 个、A-6b 上 4,417 个，来源标注显示**无一例外**都是盖心。用精确谓词把中心点
与四个面的平面比对*找不到*它们——中心点是平均值，只在数值意义上位于平面内。去掉该节点，
问题即不复存在。

**4. 越过交会点的扇形边。** 一律从盖的最小键顶点扇形化比用中心点更糟：盖一般并非凸多边形，
A-3 因此出现 12 个反向四面体、A-8 出现一个重复单元。更关键的是，*另一张*曲面与盖相交处的
交会节点位于盖边界某段直线的中段——边界在该处不发生转折——因此从相隔两步的顶点扇形化时，
其边会径直越过该节点。A-3 余下的 24 个悬挂节点全是交会点。`fan_cap` 按键序逐个尝试环上
顶点，取第一个既不自我折叠、也不吞没任何顶点的扇形；若无一可行则由调用方拒绝该次切分。
顶点按键选取，故仍是该环的纯函数。

针对最初的失败曾测试过两种假设，**均被否定**，值得记录，因为二者都是最容易想到的猜测：

- 不是缺少升级规则。A-4 上全部 288 个这类单元本就因 `multi_crossing` 而升级；该守卫予以
  保留（不变量本身成立），且它触发 576 次，涵盖该算例的每一个升级单元。
- 不是四位置的弦配对。禁用 `crossed_face` 的双弦分支后，所有共形性计数逐位一致。

#### 门 G6-0：曲线进入 S8

`CutOptions::curve_segments` 把排布中全部锁定曲线——尖锐棱、相交曲线、边缘环、非流形边——
以折线段形式送入切割阶段，`curve_pierce_points` 则按晶格面计算曲线刺穿该面的位置。键取该面
自身按键排序的三个角点，故共享该面的两个单元由面本身导出同一条目，不变量 J1 得以保持，与弦
交会点完全同理。日志为 `[S8/G6-0]`：A-3 764 个面、A-6a 798、A-6b 2,174、A-7a 582、A-8 4,089。
A-1 与 A-2 为零，这是对的——球面没有曲线，而立方体的棱在 S7 吸附之后正落在晶格边*之上*，
不会刺穿任何面的内部。

针对该点的四种用法，均以 A-3/A-6a/A-7a 的 `[V6]` 计数（基线 139/697/134）实测：

| 方案 | A-3 | A-6a | A-7a |
|---|---|---|---|
| 面被穿越时，用精确曲线点取代 `chord_meeting_point` 的估计 | 138 | 697 | 134 |
| 再把曲线点用作 `LoopFan` 的锥顶，并按*猜测的*分量对登记 | 123 | **715** | 134 |
| **再把曲线点用作锥顶，并按**每一个**分量登记** | **134** | **697** | **134** |
| 再强制被刺穿的面升级 | 123 | 715 | 134 |
| 曲线点作为面的*顶点*，锥顶仍取面心 | 155 | **1,275** | 134 |

只有最后一种单调不劣，故予以采用。其余一并记录，因为每一种听上去都合理，却被数据否定。

- **位于锁定曲线上的点，落在沿该曲线相交的**每一张**曲面上。** 只按*猜测的*分量对登记，正是
  A-6a 由 697 恶化到 715 的原因：切分随后会在一个未被告知的曲面上取点询问侧属，得到的是谓词
  的舍入结果，于是拒绝。按每一个分量登记既消除该退化，又保住 A-3 的收益。

- **强制升级纯属代价。** 各算例的 `[V6]` 完全相同，四面体却多出约 2 %。§6 能正确切割的单元，
  交给扇形化并不会变好。
- **锥顶必须落*在*曲线上，而非仅仅靠近。** 把该点移出锥顶、改置于面内部——理由是位于两张曲面
  上的锥顶会让每个扇形四面体的形心落入曲面包络之内，而那正是 `seed_record` 最不可靠之处——
  结果明显更差（A-6a 697 -> 1,275）。该推断是错的。
- **每面一个节点必要而不充分。** 最优方案在 A-3 上省下 16、在 A-6a 上多出 18。材料边界仍来自
  盖面，因此除非把切割约束为在单元内**沿**曲线走一条边链（而非每面一点），倒角就依然存在。
  那才是真正的受约束边恢复，也正是门 G6-0 当初回避的东西，是余下的工作。

#### §7.6 为何仍然拒绝：实测计数

A-6a 的 538 个升级单元中有 426 个回退。用 `RUSTMSPT_JCT_DIAG=1` 逐条计数而非猜测：

| 拒绝原因 | 次数 |
|---|---|
| `one side empty` | 317 |
| `piece not split by component 2` / `component 1` | 251 / 165 |
| **`a cap does not fan without folding over itself`** | **235** |
| 体积校验拒绝 | 52 |
| `a piece is not closed` | 19 |
| `inside holes are not simple loops` | 17 |
| `mixed sides on one triangle` | 4 |

把 317 个正确拒绝（见下）排除后，真正的缺口是**凹（re-entrant）盖面**。为其构造网格的两种方案
均已实现并实测，且**都比拒绝更差**。把盖面锥化到其自身中心点——本代码最初的形态，并以*相对*
容差（而非精确谓词）防止中心点落在单元四个面的平面上——会让 A-8 重新出现 `[V1]` 与 `[V3]`
失败，且在 A-6a 上根本不触发。这澄清了先前悬而未决的一点：精确判据确实用错了工具，但换成相对
判据也无济于事，因为问题在于那个**节点**本身，而非如何检验它。第二种是耳切法——凹多边形正是
它的用武之地——结果同样**更差**：A-3 由 134 恶化至 165，且被切开的单元反而更少（148 -> 106），A-6a 与 A-8 还各自
新增 `[V3]` 失败。

原因值得记录，因为这个错误很有诱惑力。盖面是单元内一张**弯曲**曲面的一块，因而并不共面。
耳切法会给该环拟合一个平面，并在其中判定凸性与包含关系；在该平面里成立的"耳"在空间中并不
成立，于是产出的三角形离开了曲面。非共面的盖面需要一种始终*留在*曲面上的三角剖分，这根本
不是多边形三角剖分问题——它正是门 G6-0 所指的那个插入问题。

`one side empty` 实为两种不同形状，把计数拆开报告（`0 in / N out of N`）即可区分：

| 算例 | 特征 | 次数 | 含义 |
|---|---|---|---|
| A-6a | `0 in / N out` | 317 | 整个单元位于该分量**之外**，而该分量仍在单元棱上留有交点 |
| A-3 | `0 in / N out` | 6 | 同上 |
| A-8 | `12 in / 0 out` | 13 | 整个单元位于**内部**——不变量 K1 的凹兜 |

第一种并非能力缺失。A-6a 的凸出体与立方体仅在平面 x = 0.5817 上相接，故对立方体一侧的单元而言，
凸出体的曲面只触及该单元的**边界**、从不进入其内部——重合交点正是 `contact_edges` 按设计放在
那里的。没有可切之物，单元是单一材料，扇形化也不产生任何倒角；§7.6 拒绝才是正确答案，只是标签
让它看起来像失败。这大幅收窄了那 426 个回退单元看似提供的空间，也直接解释了为何强制把被刺穿的
单元送入 §7.6 无济于事：它们本应被切的那张曲面，根本不穿过它们。

#### 盖面折叠判据本身就是阻碍，而非保障

`fan_is_simple` 原本是问：盖面扇形化的每个三角形，是否与**第一个**三角形的法向一致。而盖面是
一张*弯曲*曲面的一块，其三角形本就彼此不一致；一旦曲面在盖面范围内转过 90 度，一个完全正确的
扇形化就会因为与"恰好排在第一个"的三角形不符而被判失败，`fan_cap` 于是报告**没有**可用的锥顶。
那 235 个"凹盖面"之所以看起来是凹的，原因正在于此。改为与盖面**自身**的朝向——其三角形的面积
加权和——比对，仍能捕获真正的折叠（折叠的三角形与整个盖面朝向相反），却不会凭空由曲率造出一个。

把这样一个盖面导出来，就能看清它究竟是什么：

```
cycle 5: 27550 (0.335845, 0.168954, 0.168954)   27557 (0.335845, 0.169146, 0.168954)
         42198 (0.335660, 0.169146, 0.168018)   33437 (0.336036, 0.169146, 0.168413)
         42193 (0.335660, 0.168770, 0.168018)
```

五个节点跨骑在 x = 0.335845（接触平面）两侧，并延伸到 0.335660 与 0.336036。这个盖面包裹的正是
**凸出体侧面与接触平面相交的那道棱**，在单个单元内部转过一个直角。它的任何三角剖分都不可能让所有
三角形朝向一致，因此只要坚持要求一致，无论怎么剖分都会被拒绝。

三个逐次放宽的版本，一个比一个好：

| 版本 | A-6a 盖面折叠拒绝数 | A-3 `[V6]` |
|---|---|---|
| 与**第一个三角形**的法向比对 | 235 | 134 |
| 与盖面自身面积加权法向比对 | 142 | 132 |
| **仅判退化，不做朝向判据** | **0** | **124** |

并无任何损失。真正判定正确性的守卫在下游且是**精确**的——每块碎片必须闭合，且各碎片体积之和必须
等于母体积——它们会捕获此处原本被提前拦下的、确实发生折叠的盖面：A-6a 的体积守卫拒绝数由 52 升至
283，正是同一批对象抵达了一个真正能作出判断的判据。A-3 与 A-8 都切开了更多单元，且无任何退化。
一般性教训值得记住：**当某个阶段拒绝得过多时，先去看看精确判据之前那个廉价的前置守卫。**

#### 体积守卫的度量，假定了 §7.6 早已不再产出的形状

把它拒绝的一个案例导出：

```
tet [10845, 11025, 11026, 10838]   parent 4.779550e-10   measured 4.893833e-10
piece 0  3.976780e-10 (12 tri)   piece 1  2.417293e-11 (8 tri)   piece 2  6.753236e-11 (8 tri)
```

**超出** 2.4 %，而三块碎片各自都没有问题——这是度量本身的特征，而非切分的问题。`fan_volume`
把三角汤锥化到一个内部点并作**无符号**求和（因为这些汤按面拼装、没有一致绕向），而这只有对
**星形**碎片才精确。自 §7.6 开始切开它过去所拒绝的那些单元起，它的碎片就不再是星形的：凹形薄片
的形心落在其外，求和随即超出。于是该守卫拒绝了正确的切分，而且每一次让 §7.6 切得更多的改进，
都让它在实效上变得更严苛。

`soup_volume` 是移除该假定，而非放宽容差。**闭合**三角汤的每条边恰好被两个三角形共用，因此一次
广度优先遍历即可把每个三角形的绕向定到与第一个一致；此时带符号求和就是体积（至多差一个总符号），
其绝对值对任何形状都精确。无法定向的三角汤即非闭合，而调用方本就会拒绝它们。

| 由 §7.6 切开的单元 | 之前 | 之后 |
|---|---|---|
| A-4 | 360 | **540** |
| A-6a | 135 | **189** |
| A-6b | 2,205 | **3,285** |
| A-8 | 474 | **684** |

连续两轮，缺陷都出在**一个写下时正确、却随着周围阶段变强而变得错误的守卫**——先是盖面朝向判据，
现在是体积度量。两者都是靠导出一个被拒绝的案例查明的，而非靠推敲代码。

#### 部分重合的曲面不得二次切割

体积守卫的余项**并非**又一个度量问题：三块碎片都能干净定向，`soup_volume` 与锥化求和逐位一致，
而母体积为 4.779550e-10、各碎片之和为 4.893833e-10——这是实实在在的 2.4 % 重叠，切分确实把同一份
材料算了两次。

原因在于"重合盖面"规则过窄。它只跳过盖面**整体**已在碎片中的分量，而*部分*重合的曲面会绕过它：
该分量在两个与既有切割共享的三角形之外，还贡献了一个确实新的盖面三角形，于是判据通过，碎片沿着
一堵本就是它自身面的墙再切一次，重叠随即撞上体积守卫——后者索性拒绝整个单元。把判据由 `all`
放宽为 `any` 即是修复：沿碎片已经持有的墙再切一次，无论该分量另外贡献多少，都绝不正确。

| | 之前 | 之后 |
|---|---|---|
| A-6a 由 §7.6 切开的单元 | 189 | **414** |
| A-6a `[V6]` | 679 | **670** |
| A-6b 由 §7.6 切开的单元 | 3,285 | **3,532** |
| A-6b `[V6]` | 1,037 | **1,021** |
| A-6a 体积守卫拒绝数 | 229 | **0** |

至此已有三整类拒绝清空——盖面折叠、体积守卫，以及（仅余一例的）未闭合碎片；而余下的 434 项拒绝
中，已有 317 项此前即被确认为*正确*的拒绝。

#### 残留问题属于面层，且已由实验证明

A-6a 的 434 项 `split refused` 中，**335 项为 `one side empty`，且无一例外都是 `0 in / N out`**
——即此前已确认为*正确*拒绝的那一类。其余当中，17 项为 `inside holes are not simple loops`，
约 82 项为 `mixed sides on one triangle`。

对于角点分处两侧、且其间没有切割节点的三角形，可以改用其自身形心来判定，而不是拒绝整个单元
——这与 `side_of_face` 为"全部位于曲面上"一类所回答的是同一个问题。它带来可观的 `[V6]` 收益，
但无法保留：

| | 拒绝（现行） | 按形心判定 |
|---|---|---|
| A-3 `[V6]` | 124 | **66** |
| A-6a `[V6]` | 670 | 659 |
| A-6b `[V6]` | 1,021 | 1,010 |
| A-3 `[V3]` | PASS | **FAIL**——2 个面被四个四面体共用、1 个悬挂节点、6 条非流形边 |

原因是该面对侧的邻居独立作出了判定，并与之不符。

**决定性实验**：只对单元自己生成的三角形（前一个分量的盖面，位于内部、邻居无从看见）放宽判据，
而对单元自身边界仍然拒绝。结果是 `[V3]` 恢复，同时 `[V6]` 的收益**分毫不剩**地全部退回，逐位一致。
可见这些三角形全都位于单元自身的边界面上。

这是证明而非推断：边界三角形出现两侧不一致，意味着**面**没有在第二张曲面穿过之处被切开，而单元层
的切割无法弥补面层的缺失。§7.6 的单元切分已经做到头了；余下的是把该穿越插入到面的三角剖分中，
也就是门 G6-0 的插入。

#### 谓词的"不确定"不是一个侧属

`PointClassifier::inside` 会报告其中有多少个谓词是模棱两可的，而 §7.6 的 `side_of` 丢弃了这一
计数、直接采信了猜测值。"不确定"意味着就谓词所能判断的而言该点位于曲面上——这与 `surface` 对
切割节点所作的陈述相同，只是由几何而非组合得出——而这正是两个物体**精确接触**处会发生的情况：
共享平面上的节点确实同时位于两张曲面上，追问它在哪一侧本就没有答案。采信猜测，便产生了角点分处
两侧、其间又没有切割节点的三角形，也就是 `mixed sides` 拒绝。

| | 之前 | 之后 |
|---|---|---|
| A-3 `[V6]` | 124 | **122** |
| A-3 由 §7.6 切开的单元 | 162 | **205** |
| A-6b `[V6]` | 1,021 | **1,018** |

同时被否定的是：这些三角形**并非**源于 `crossed_face` 因弦与行走边重合而放弃整个面。该拒绝会把
整个面交给 `LoopFan`，其三角形会与两条切割交错——显而易见的修法是保留该面的另一条弦（用
`continue` 而非 `return None`），实测为中性：A-6b 由 1,021 变为 1,022，其余毫无变化。

#### A-6a 的残留是三面交角处的倒角

每一个被抽样的违规项，其两个四面体的 `provenance` 都是 3——两者都是 §7.6 或扇形化的碎片；
而在抽样的 50 个面中，**没有一个位于输入平面上**。它们都落在三张曲面交汇处约 5e-4 的范围内：
接触平面 x = 0.5817 与凸出体的侧面 y = 0.2917、z = 0.2917/0.2977。例如：

```
(0.582031, 0.421875, 0.292969)  (0.581700, 0.421875, 0.292638)  (0.581671, 0.423017, 0.291347)
```

这一次排除了两个省事的解释。它**不是**标签缺失：该面并不位于任何曲面上，声明它等于谎称共面——
而这正是 `declare_contact_components` 所要避免的失败模式。它也**不是**种子判定错误：分类器在该处
是确定的，且 `[JCT-SEED]` 在所有算例上都报告 `0 exact-predicate escalation(s)`（改用四个内部采样
投票取代单点采样，实测完全中性）。

它是**三面交角**处的倒角。§7.6 每次只按一个分量切分，因此能跟随沿一条曲线相交的两张曲面，却无法
再现第三张曲面加入之处的那个角点，材料边界于是在单个单元内被抹圆。A-6a 与 A-6b 恰恰是带有这种角
的夹具——凸出体齐平地扎根在立方体面上——这也是它们的 `[V6]` 比 A-3 高一个数量级的原因。随后角点插入也已实现并实测，它**不是**关键杠杆：`cell_corners` 定位每一个有三条及以上曲线段交汇的点，
而 A-6a 只有 **9** 个这样的单元，却有 670 项违规（A-3 为 12，A-8 为 221）；且这些违规沿棱在 *y* 方向
铺开，并非聚集于若干点。残留位于**沿棱的曲线上**，而非其角点处。逐面刺穿已经在这些曲线上放置了节点，
但仅作为**升级**单元的 `LoopFan` 锥顶——由 §6 表格切开的单元根本看不到它，而强制它们升级已实测为纯粹的
代价。要闭合它，需要把曲线作为一条**棱链**贯穿整个切割。

### 本轮收尾：`[V6]` 是唯一遗留项

`[V1]` 与 `[V3]` 首次在全部九个验收算例上干净通过，其中五个通过了所有实际运行的检查。
余下的是 `[V6] region_adjacency`：A-3 由 139 降至 **122**，A-6a 由 697 降至 **670**，
A-6b 由 1,026 降至 **1,018**，A-7a 为 134。

它已被定位，而不只是被界定。每个抽样违规项的两个四面体都是 §7.6 或扇形化碎片
（`provenance = 3`），且抽样的 50 个面中**没有一个位于任何输入平面上**——它们承载的边界，是
*不同*单元的碎片之间的倒角，落在一个不属于任何曲面的晶格面上。除重建 §7.6 的产出之外，其余方案
均已实现并实测：逐面曲线刺穿与按全部分量登记（已采用，A-3 由 139 降至 122）、强制被刺穿的面升级
（`[V6]` 完全不变，且实测两次）、角点插入（9 个单元对应 670 项违规）、放宽 mixed-sides 判据
（A-3 降至 66，但破坏 `[V3]`，且已被证明）、耳切法（盖面并不共面）、带相对容差守卫的盖心
（`[V1]`/`[V3]` 失败），以及 `seed_record` 的多点投票（中性——采样从不出现不确定）。

要闭合它，需要让切割把锁定曲线作为一条**棱链**贯穿单元，使材料边界落在曲线上，而不是被倒角到最近
的单元面。这就是带 Steiner 插入的受约束边恢复——即重新开启门 **G6-0**——它是对 §7.6 产出的重建，
而非又一次守卫修补。基础工作已经就位：曲线段已进入 S8，逐面刺穿映射的键使共享该面的两个单元达成
一致，曲线节点也已按每一个分量登记为位于曲面上。

#### 每个面只出一个界面单元

`derive_interface` 会**按分量**各导出一次接触面，这是对的：两个精确接触的实体共享同一个
网格面，它同时是二者的边界。但写出时为每一条各产出一个三角形，这既是重复单元——A-6a 的
`[V1]` 报告 50 个，全在接触平面上——也使每个 `FaceTag` 集合只有一个成员，于是 `[V6]` 的
共面豁免无内容可读。`cut_to_doc` 现按节点**集合**归组，每个面只产出一个单元。用集合而非
数组是关键：面的节点按*量化*键排序，两个不同节点可能共享同一个键，稳定排序于是让一个分量
给出 `[a, b, c]`、另一个给出 `[a, c, b]`——按数组比较恰好漏掉的正是这一对。

## G6-2..G6-5 切割（S8）

`src/meshgen/cut.rs` 与 `src/meshgen/junction.rs`。这是使网格*协调贴体*的阶段：凡被活跃面片穿过的单元，都被替换为恰好止于该面片的碎片，于是曲面成为单元面的并集。

### 三张冻结表，以及它们协同成立的理由

- **§4** SNK 规则：四边形的对角线取过其最小 `NodeKey` 顶点的那条；定理 T2 断言该规则绝不产生循环棱柱。
- **§5.2** 剪纸式面剖分表：切割后一个*面*长成什么样。
- **§6** 单面片六种四面体情形：一个*单元*变成什么。

S8 内部不存在任何跨单元通信。使碎片彼此吻合的是不变量 C：每个选择都是节点键与切割状态的函数，而共享一个面的两个单元对二者的看法一致。§6 的表格构造得使每块碎片落在母面上的边界三角形恰好等于 §5.2 对该面给出的结果——这一断言在 19,200 个随机面上被检验，而非假定，并由 `[V3]` 端到端复核。

切割节点在任何单元被切之前就按 `(边, 构件)` 分配好全局编号。这正是逐单元工作彼此独立的原因：共享一条边的两个单元查到同一个编号，故其碎片必然吻合，单元因而可并行切割并按单元序拼接。

### 受保护的试运行

提交切割前须满足：每个子单元正定向，且子单元体积之和与母体积相差不超过 1%。体积判据正是捕获*错误表行*的那一条，因为拼装错误的情形照样给出正定向四面体，只是填不满母体。评审场景实测最差体积误差 **9.5e-15**。

注意 §4.3 给出的是棱柱*节点表*，并注明「按规范定向修正后发射」——第 6 行 `(a0,a1,b1,b2)` 在一个笔直棱柱上就是负定向——故 `orient_positively` 并非可有可无：照表逐字发射的调用方会输出翻转单元。

### 升级处理，以及为何升级单元不等于跳过单元

被两个及以上面片穿过、或有某条边被同一构件穿越两次（K1）、或处于 §6 未覆盖状态的单元，无法走表格路径。它**不会**被原样放过：其邻居已经剖分了与它共享的面，若它保留四个平面，网格上就会出现悬挂节点。评审场景中，原样保留的代价是 4,577 个悬挂节点与 78 条非流形边。

门 **G6-0** 决定了替代方案：对局部 PLC 约束四面体化给出**否决**，转而采用计划中具名的回退方案，并以更强的形式实现——

> 每个单元的边界由**该面自身状态的纯函数**三角化，其内部则由位于单元质心的 Steiner 点扇形填充。

如此，协调性（J1）由构造保证而非依赖曲线节点钉扎：无需缓存，因为该函数*本身*就是不变量。有效性是完全的：母四面体是凸的，其质心可见整个边界。不变量 J2 精确成立，因为在冻结表适用之处，该例程*就是*冻结表。所放弃的是这些单元内部的切割——材料边界至多按一个单元尺度倒角（`<= h`），并以 `[JCT-FALLBACK]` 逐单元记录。评审场景中为 268,948 个单元中的 1,423 个（0.53%）。

该回退方案中有两处关键细节，均由实测发现：

- 面扇形以**面质心**为顶点，而非取自环上的某个顶点。以顶点为心时，只要顶点与某相邻对同处一条母边上，就会发射零面积三角形——而当某角点的相邻边上有切割节点时正是如此。这些碎片通不过定向判据而被丢弃，从而留下孔洞：8 块被丢弃，12 个面泄漏。
- 该质心**每个面只分配一次**，由共享该面的两个单元共用。各自分配会在共享面上产生重合的重复节点，反倒摧毁它本要提供的协调性。

单元也可能在**完全没有归属歧义**的情况下需要升级：S6 按顶点奇偶分类、S7 按精确边判据求交，二者可以在某个单元上不一致而彼此都没有错。因此分诊依据的是*是否存在被切割的边*，而非记录。

### 归属与派生界面索引（G6-3）

§6 的表格给出每块碎片位于哪一侧，故此处无需 `assign_cut_sides` 的角点投票、并查集与定向探针——归属直接自表读出。母单元的*确定*条目原样继承：位于构件 1 内部、正被构件 2 切割的单元，在新切面两侧仍都位于构件 1 内。

界面索引是**派生的，从不存储**。两侧单元通过「哪些四面体带有该三角形、其记录如何」反查得到，因而可仅凭网格本身重建，索引失效在构造上不可能发生。`side_elems[0]` 是构件内侧单元，`[1]` 是外侧单元——正是 PLAN §10.13 为内聚单元与分裂节点工具预留的导出契约。

### 焊接片体切割（G6-5）

片体没有内部，故 §6 的侧别不能来自 S6——S6 拒绝对片体分类，且必须如此，因为片体占有体积正是那道守卫所要防止的错误。侧别转而来自切割本身：**两个母节点同侧，当且仅当它们之间的边未被穿越**，于是对未切边做并查集即可直接恢复两侧，完全不需要几何判据。随后两侧都保留母单元的记录，仅由带标签的界面区分二者——这正是 §10.13 的 C0 语义。

恰好两个等价类是干净切割的条件。一个类意味着片体终止于单元内部（内部边缘环，即 `split_R` 的情形），三个类意味着这些交点并不描述单一曲面；两者都升级到协调扇形。`split_R` 本身已实现并通过测试；它所需的**边缘环节点构造**推迟至 G7-2。

### 实测（G4 评审场景）

| | |
|---|---|
| 被切单元 | 268,948 中的 34,897（383 A / 762 B / 22,641 C / 11,111 D） |
| 单元数 | 268,948 -> 408,518 |
| 界面面片 | 46,008 |
| 升级为扇形 | 1,423（0.53%） |
| 最差体积误差 | 9.5e-15 |
| `[V3]` | 悬挂节点 0、非流形边 0、边界泄漏 0、负体积 0 |
| `[V4]` | 最差长宽比 1211.7、最小二面角 0.597 度、42,100 个低于低二面角门限 |

S8 之后 `[V4]` 告警是**预期之中的，属于 S9 的预算**，并非缺陷：SPEC §6 明言切割在构造上就会产生薄片，这些数字是 S9 的输入而非验收门限。

### G6-6——吸附后质量门

G4-3 中需要 S7 与 S8 才能完成的那一半。每个单元按其来源单元的模板分组，在各阶段对照修正后的 35.264 度基线（SPEC §3.7 rev 1.2）比较两个总体：

| 阶段 | Freudenthal 最差 / p5 / 中位 | 质心扇 最差 / p5 / 中位 |
|---|---|---|
| 吸附前 | 45.000 / 45.000 / 45.000 | **35.264** / 45.000 / 45.000 |
| 吸附后 | 45.000 / 45.000 / 45.000 | 35.073 / 44.165 / 45.000 |
| 切割后 | 0.874 / 5.522 / 45.000 | **1.848 / 6.474 / 45.000** |

**通过（GO）。** 切割之后，扇形单元在最差值与第 5 百分位上都*优于* Freudenthal 单元：薄片源自曲面恰好擦过某单元之处，这与该单元用了哪种模板无关，而扇形较小的四面体留给这种擦切的余地更少而非更多。参考实现逐字 SAMR 回退方案未被启用。

验收（`tests/meshgen_cut_tests.rs` 19 项，`tests/meshgen_quality_gate_tests.rs` 1 项）：SNK 对四边形的任意旋转与翻转保持不变；定理 T2 在 20,000 组随机键序上成立，且恰有两行循环组合被拒；棱柱表以*边界恒等式*检验其铺满棱柱（体积比较需要一个参考分解，而那正是被测对象）；证明定向修正既必要又充分；§6 每种情形按舍入精度剖分母体，且碎片绝不跨越切面；**协调性断言——切割在每个母面上的三角形等于 §5.2 的结果——覆盖 19,200 个面**；共享一个面的两个单元结果一致；面表在 `split_2` 与 `split_R` 下铺满该面；悬空切割被拒；试运行捕获填不满母体的切割；在光滑曲面*以及*薄于一个单元的板件上端到端协调；端到端体积与定向守恒；界面索引双侧且可派生；焊接片体切割不改变任何归属；跨线程数确定性；`s08` 验证无 FAIL；以及上表的 G6-6 门测量。

### 接触平面：S2 知道而 S8 不知道的事

两个实体面对面精确接触时，共享一个同时作为二者边界的网格面。S2 能识别这一点，并
用**两个**分量标记该排布面（`ArrangedFace::components`）；A-6 的立方体与肢体共享
平面 x = 0.5817，S2 在此发出五个标记为 `{1, 2}` 的面。但 S2 与 S8 之间的所有环节
都只读取单一的 `ArrangedFace::component`，于是共享边界到达切割阶段时成了某一个物
体的边界，而两个物体之间的网格面则谁都没有声明。`CutOptions::contact_patches` 现
在把 S2 的重合面片传递下去，`declare_contact_components` 为落在其上的每个网格面声
明该面片所属的**全部**分量。

这条规则刻意采用**几何判据而非记录判据**。若把邻居内部集不同的任意面都声明出来，
`[V6]` 的重合豁免就会形同虚设——每一处升级倒角都会自证合法。只有在两个输入曲面
确实重合处才声明；其他位置的双分量跳变仍然失败，这正是应有的行为。

**首先要让该平面成为网格面，这本身需要两项修正。**

指向它们的测量：在 a6b 的 3,156 处双分量跳变中，**没有一处位于接触平面上**——它们
分布在跨越 x = 0.5817 的一层薄片中，而网格上根本不存在该平面上的面。

1. **S7 只为一个物体穿越了该平面。** `gather_geometry` 读取单一的
   `ArrangedFace::component` 而丢弃了 S2 的 `components`，于是第二个实体在该处的边界
   没有产生交点，S8 也从未切割它。现在它按每个分量各发出一次三角形。随之一条边上的
   两个重合交点必须共享**同一个**节点 id（`cut_lattice` 中的 `contact_edges`），否则
   它们就是重复节点与裂缝；共享 id 后逐边映射完全相同，`face_is_single_patch` 判定为
   "同一曲面被访问两次"，该面便走普通的 §5.2 行。`contact_edges` 与 G7-2 的
   `collapsed_edges` 保持区分：两者在 §6 意义下都使单元成为单一曲面，但只有折叠边才是
   带有待声明轮廓的薄片。
2. **标记流程按四面体而非按面过滤。** `declare_contact_components` 要求四面体的全部四
   个节点都位于面片的膨胀包围盒内。面片是平面内的三角形，该盒没有厚度，而接触该平面的
   四面体总有一个顶点在 `h` 之外——因此从未有候选通过，即使面已经存在，该流程仍然静默。
   改为检验面的三个节点才使其生效。

**实测：接触平面上的网格面 62,216 个（原为 0），升级单元 5,020 -> 3,579，`[V6]`
A-6b 3,352 -> 1,216、A-6a 927 -> 461，A-6b 分量 2 的体积误差 0.031 % -> 0.026 %**，
`[V3]` 干净，R-P2 逐位一致。

据此再测又得出第三项修正。一个接触区域由若干排布面片组成，而
`declare_contact_components` 要求网格面的三个角点都落在**同一**面片上——于是横跨两个
相邻面片的面未被标记。实际上每个角点只需落在*某个*面片上即可，并要求分量集合一致，
以免把两处恰好相邻的无关接触合并：`[V6]` 1,216 -> **1,026**。

**残余现在的位置。** 全部 3,211 处原始跳变都是 `{1}` 对 `{2}`。**2,185 处恰好位于该
平面上**，标记后即被豁免；其余 **1,026 处位于 x ∈ [0.5813, 0.5829] 的薄片中**，其范围
由肢体截面界定——那正是接触面片的*边界曲线*，而网格并未与之共形（`curve coverage 1 of
35`，K1 已在三遍处封顶且 `h` 已达 0.003464 的下限）。与 A-3 的圆形交线不同，这道边界
是矩形，因此加密原则上能够抵达；这就是尚未完成的工作，属于曲线捕获而非标记。

## G7-1 薄区域：条带模板与 FEM 感知阶梯（S8b）

`src/meshgen/thin.rs`。S8 只在**一个**活动面片穿过单元时切割它。**条带区域（band region）**正是击穿该路径的情形：同一间隙的两面壁穿过同一单元，而两壁间距比单元还窄，于是通用路径判定其为结点单元并交给协调扇形——结果合法，但把间隙削平了。条带区域改为用**跨越间隙的单层单元**来剖分。

### 一切推导所依赖的唯一结构

一个条带单元由三对匹配点对 `(a_i, b_i)` 构成，每对或保留、或塌缩为一个边缘节点 `r_i`。其**边界**是这三对点与每条对边一个对角线标志的纯函数：A 面盖、B 面盖，以及每条对边一个四边形（塌缩顶点做替换）。模板、Steiner 回退与体积校验全部由这一个构造推出，这正是条带层无需协商即自洽协调的原因。

**塌缩记录在点对上，而非单元上**（`SPEC_meshgen_geometry.md` §8.1，不变式 B1）：某点对在一个单元中塌缩，则在引用它的每个单元中都塌缩，因此 `k` 不同的相邻单元自动就共享四边形达成一致。

### 冻结表（§8.2）

| `k` | 单元 | 四面体数 |
|---|---|---|
| 0 | 棱柱，四边形按 SNK 规则剖分 | 3 |
| 1 | 以 `r_i` 为顶点、架在存留四边形上的金字塔 | 2 |
| 2 | 单个四面体 `(r_i, r_j, a_l, b_l)` | 1 |
| 3 | 退化——单元*即*片体三角形 `(r0, r1, r2)` | 0 |

§8.2 rev 1.3 的标称单元（直角等腰面盖、直角边 `h`、拉伸 `t`）在 `t/h = 0.35` 处测得最小二面角 `18.281° / 19.561° / 19.853°`、最大长宽比 `2.0953 / 2.0165 / 1.9662`（对应 `k = 0 / 1 / 2`）。rev 1.2 的长宽比一列有误，已由本子任务更正，见冻结文档 §14 [9] 与 §15 D-14。

### §4.4 运行期阶梯——以及切割阶段跳过的那一级

每个生成的四面体都要校验：定向为正、最小二面角不低于阈值（默认 8°）、且子单元体积复现该单元自身边界所围体积。随后依次：

1. **表行本身**（Rule SNK）；
2. **对角线翻转**——这正是 `cut.rs` 的 §4.4 阶梯有意跳过的一级。翻转**不是**四边形的纯函数，单元独自翻转会让共享该四边形的邻居失去协调。`mesh_band_layer` 同时拥有两个相邻单元，因此它为这一对同时提出翻转，且仅当两者都仍能成功剖分时才接受——即参考薄特征设计 §3.8 所说的“仅成对尝试”；
3. **Steiner 扇形**——把单元自身边界锥化到其形心，完整棱柱为 8 个四面体，仍严格是一层几何层。此级不施加二面角阈值：它存在的前提就是阈值无法满足，其输出保证合法而非优质。

某区域若 Steiner 占比超过 5%，则整体降级为体元剖分并给出 `[THIN-SKIP]` 警告，且不分配任何节点。

### FEM 感知阶梯（PLAN §10.11）

`predict_band_quality(t, h)` 用与验证器相同的 `tet_quality`，在全部六种非循环对角线组合上测量标称单元——计划书的闭式（“高 ≈ t，最小二面角 ≈ atan(t/h)”）量纲正确但数值不对，而门限比较的正是数值。随后 `band_ladder` 按冻结顺序取级：

| 级 | 结果 | 条件 |
|---|---|---|
| 1 | `Band` | 预测通过全部门限 |
| 2 | `RefineLocally` | 再细化一级（`h/2 ≥ h_min`）即可通过 |
| 3 | `Sheet` | `t ≤ t_sheet(x)` |
| 4 | `Volumetric` | 可表示但质量差，且用户未禁止（WARN） |
| 5 | `Reject` | 已无余地；理由中点名失败的门限 |

门限：`band_min_dihedral_deg`（8°）、`band_max_ar`（20），以及一个高度下限——`implicit` 剖面下关闭，`explicit` 剖面下为 `0.05 · h_local`，作为稳定时间步的**几何**代理（真正的 `Δt` 需要网格生成器不掌握的材料数据）。四者连同 `regional_failure_share` 与 `forbid_volumetric_fallback`（后者移除第 4 级）以及 `collapse_sheets`（G7-2 的边缘塌缩，默认关闭）均位于 `meshgen.thin` 配置块。

### `[V7]` 的条带部分

验证器的单层检查通过**节点**表达，这正是它能仅依赖 VTU 的原因：条带以单层跨越间隙，当且仅当没有条带单元的节点严格位于间隙内部——每个角点都落在壁面上，而壁面即带标签的面。`regime = 2`（Steiner）单元恰允许一个内部节点，即其自身顶点。`[V7]` 另报告条带单元数与 Steiner 数、条带的二面角与长宽比区间、条带区域清单，以及全部 `[THIN-SKIP]` 区域。

### 排序键，以及它为何比焊接网格更细

`SPEC_meshgen_geometry.md` §1.2 对 `NodeKey` 有两点要求：只依赖位置，且不同节点具有不同的键。第二点正是「最小节点键」构成**全序**的前提，而 §4 的全部协调性论证都建立在该全序之上。

背景格点按构造满足这一点；**S7 的交点不满足**——它们是构造出来的点，彼此之间并未做焊接，两个交点可能落得比焊接量子还近。一旦发生，Rule SNK 出现并列，并列结果取决于调用方先列出哪个节点，而共享同一四边形的两个单元列出的顺序相反——于是它们沿相反的对角线剖分，网格随之出现一对单侧面。在参考数据集上，仅此一类并列就造成 60 处边界泄漏。

因此 S8 的键表建立在焊接量子的 `KEY_ORDER_REFINEMENT`（`1e-6`）倍之上。该顺序仍只是位置的纯函数，绝不查阅节点索引，且背景格点的键依然精确。已冻结为 Rule K-O（§1.2 rev 1.4）。

**把并列的两点焊接并不是替代方案。** 该方案已试过：按键焊接同时会合并*不同边*上的交点，并压塌其间的单元——产生 290 个多重共享面与 580 条非流形边，而更细的键在这两项上均为零。焊接不变式描述的是*已焊接*的节点；构造点需要的是更细的**排序**键，而非更粗的同一性判定。

### 双切面剖分规则与三层切分

`SPEC_meshgen_geometry.md` §5.2 只覆盖被切**一次**的面。条带单元的面被切两次——每条被切边上每面壁各一个交点——而 §7 的环形扇会把这样的面锥化到单一形心，生成横跨间隙的三角形。这正是含薄间隙的单元过去会变成一整块实体、间隙被削平的原因。

`band_face_split` 即所缺的规则：若一个面的两条被切边各自同时携带 A 与 B 的交点，则该面剖分为角部三角形、两壁之间的条带、以及剩余部分，其中两个四边形按 Rule SNK 剖分。与 §5.2 一样，它是该**面**的纯函数，因此两侧单元的计算结果必然一致。

**该规则按面施加于每个升级单元**——而非仅在单元级切分成功的单元内部。这一顺序至关重要而非仅是整洁：这样的面由两个单元共享，而其中可能只有一个是夹层单元；按单元施加会让二者产生分歧，并恰好在间隙处引入悬空节点。

随后 `split_band_cell` 把夹层单元（三个双切面交于一个母顶点、其对面一个未切面）切分为三个闭合层，每层由 `close_open_surface` 封闭。层的盖面**就是**壁面本身，由该层未配对有向边反向构成，因此壁面两侧的两个层得到互为反向的盖面：是一个共享面，而非两个重合面。

规则未覆盖的单元会带着可计数的原因（`BandDecline`）拒绝并走原有扇形路径，因此改进严格是增量式的，且每次运行都能报告规则为何未触发。

### 诊断开关

两个环境变量让 S8 的协调性排查无需重新编译：

| 变量 | 输出内容 |
|---|---|
| `RUSTMSPT_CUT_DIAG=1` | 输出 `[CUT-DIAG]` 块：把每一个未打标签的单侧面归因到产生它的单元的升级原因，以及构成它的节点类别（母节点／切点／面形心／单元形心），并给出若干不匹配配对示例。该统计在 `cut_lattice` 内部完成——此时 `parent_of` 与升级列表仍然存在，而 `[V3]` 运行在已写出的 VTU 上，这些来源信息已经丢失。 |
| `RUSTMSPT_CUT_CELL=<ids>` | 输出指定背景格单元的 `[CUT-CELL]` 明细：母四面体、组件、各顶点侧别、ambiguous 与 crossing 组件集合、每条边上的切点，以及每个节点的坐标与键。正是它定位到了 Rule K-O 背后的键碰撞。 |

`RUSTMSPT_S67_DIAG=1` 会为每一处 S6/S7 判定分歧打印一行，按方向标注（`straddles-no-crossing` 与 `crossing-no-straddle`）并给出两端的侧标记。它用于判断应怀疑两个阶段中的哪一个：在支杆点阵上，77.6% 的分歧是 S6 认为变号、却不携带交点的棱边。

### 如何接入生产路径

调用链为：`band_face_split` 对升级单元中的每个双切面做三角化；`split_band_cell` 把夹层单元切分为三个闭合层，并给出间隙层的三对匹配点对；间隙层交给 `mesh_band_cell`，按冻结的 §8.2 表行剖分，仅当表或 §4.4 阈值拒绝时才回退到 Steiner 扇形；两个盖面被登记进界面索引，`regime` 则把 1（条带）或 2（条带-Steiner）写入 VTU，使 `[V7]` 度量真实输出。`band_ladder` 对每个薄区域按 `meshgen.thin` 门限运行一次并报告其级别；在 `forbid_volumetric_fallback` 下，被拒绝的区域会连同拒绝它的门限一起使本次运行失败。

在间隙为 18 单位（尺寸下限无法分辨）的双板夹具上实测：82 个夹层单元，间隙层 `prism x32 / steiner x50`，496 个条带单元，`band_stacked_elements = 0`，`[V7]` PASS。

## G7-2 塌缩薄片集成：薄区编号、标签与侧标记

**薄区编号进入网格。** `gapfield::thin_context` 把 S3 的薄区归约为按*排布面*查表的形式——该面所属薄区、该薄区的**生效**制式（经 S3/S4 耦合与 §10.11 阶梯之后的结果，未必等于 `ThinRegion::regime`），以及其配对类别——结果挂在 `CutOptions::thin` 上。每个切割节点都知道其交点所在的排布面，因此条带单元可以指明自己所跨的间隙：`CutMesh::band_region` 逐单元写出，并作为 `band_region` 单元数组输出。映射覆盖薄区的**两侧**壁面，而不仅是其生长起始的那一侧，因此条带单元从任一盖面都解析出同一编号。在双板夹具上 `[V7]` 现报告 `band_regions = 1`——此前 S8b 产出的每一个网格上它都报告 `0`。

**面标签。** `cut_to_doc` 曾把 `FaceTagKind` 硬编码为 `0`，因此 S8 输出的任何面都从未被标记为薄片，`[V7]` 的薄片指标与 `[V8]` 的针孔检查实际都在度量空集。现在该取值来自组件：声明为薄片的组件给出 `FACE_TAG_SHEET`；任何三个角点全为边缘节点的面同样如此——塌缩已沿该面熔接了间隙的两侧壁面，无论两侧实体当初如何声明，剩下的都是一张内嵌的熔接薄片（§10.13）。在"实体+内嵌薄片"夹具上实测 220 个薄片面，`unwelded = 0`。

**侧标记。** 薄片不占据体积，因此 S6 不为其写入归属条目，`side_of` 对**两侧**邻元都回答 `Outside`——这使得 §10.13 为内聚力与分裂节点预留的 `(elem⁺, elem⁻)` 静默退化为 `(-1, 其中之一)`。薄片面的两侧现由几何决定：第四个节点位于该面自身平面正侧的邻元为 `elem⁺`。面的节点按键排序，因此两侧单元算出相同法向、对谁是谁达成一致——与规则 SNK 所依赖的论证相同。实体面片仍沿用归属约定。

**配对类别。** `ThinRegionPairClass`（`SPEC_meshgen_contracts.md` §2.3）与 `ThinRegionRegime` 一同在 `s03` 与 `s08` 上输出；在 `s08` 上按条带单元的 `band_region` 索引，因此网格的下游消费者无需保留 `s03_gapfield` 即可说明某单元所跨间隙的类型——实体与薄片接触、两张薄片，还是同一物体的两个面。

**边缘塌缩：`meshgen.thin.collapse_sheets`。** 若一条格边的两个壁面交点同属一个 `Sheet` 薄区且间距不超过 `t_sheet`，该边只携带**一个共享边缘节点**而非两个，于是没有任何单元会看到这个间隙，其上每个面都走 §5.2 的常规表行，一致性重新回到单面片论证，而无需事后重建。这正是 §8.2 的 `k = 3` 行——"该单元**就是**薄片三角形，其面来自薄片切割"——的正向读法。把塌缩放在切割节点这一层是关键：在单元剖分**之后**再熔接两壁会带来 27336 个悬挂节点，因为那时两壁已把共享面按双切点对三角化，而每个只切分过同一面一次的邻元都会与之不一致。

所有交点**均已**塌缩的单元随后走一次 §6 的表（`cut_one_cell` 中的 `welded_pair`）而非升级，第二个实体的归属由 `cut_record` 取第一个的补集——两者之间已无空隙，子单元不可能同时位于二者之外。要让这条路径保持一致性，还必须修正 `face_states`：它此前只要有**两个组件**触及某个面就判定其不可用表表达。该判据对两张面片是对的，对塌缩面是错的——塌缩后两壁在**相同的节点编号**处穿过相同的边，该面虽携带两个组件，却仍是普通的单面片面。`face_is_single_patch` 现改为比较各组件逐边实际贡献的切割节点，从而区分"同一曲面被触及两次"与"两张曲面"。

**塌缩薄片的边缘**作为已声明曲线输出（`CurveKind = 1`，以 `VTK_POLY_LINE` 单元加 `curve_id` 表示）。`[V8]` 要求薄片的边界边必须是已声明曲线，正是为了让它**无法**解释的边界——针孔——仍然失败；因此边缘由塌缩**决策**导出，而非由输出的面导出：同时含有一条塌缩边与一条未塌缩交点边的单元位于薄区边界之上，只有它所携带的边缘节点才够格（`collapsed_sheet_rim`）。薄片正中的孔洞其端点位于内部，不会被声明，因而仍然失败。

在双板夹具上以 `collapse_sheets: true` 实测：`[V3]`、`[V7]`、`[V8]` 全部 PASS——2592 个薄片面、`unwelded = 0`、144 个边缘曲线单元、0 个针孔——R-P2 逐字节一致，`[V4]` 无变化。**其默认值为 `false`**，仅出于验证覆盖面的考虑：现有证据只有两个人造夹具，能在规模上确认它的验收用例（§17.4 A-7，近接触间隙扫描）尚未建立，且它一旦生效就会显著改变网格——3888 个条带单元会变成一张 2592 面的熔接薄片。

**§10.11 阶梯现在真正决定薄区制式。** 此前它的结论只被打印随即丢弃，而 `effective_regimes` 仍沿用阈值给出的判断，于是被阶梯在第 4 级降级的区域仍会被做成条带或被塌缩。第 1/3/4 级现在会在 S4 的 LFS 源与 S8b 读取之前写回 `Band`/`Sheet`/`Normal`。第 2 级无法兑现——以更细的 `h` 重跑 S4 尚未实现——因此该区域保留其声明制式，并如实报告该级别。

**开放薄片的边缘**——即以 STL 输入、在网格内部终止的薄片其边界——由同一条路径声明。它从来就不是几何问题：S7 早已把边缘曲线纳入吸附目标（除 `Box` 外的每一种 `ArrangedCurveKind`），因此切割前沿本就终止**于**边缘之上。在"实体+薄片"夹具上实测，22 条边界边中有 19 条的中点精确落在边缘上，其余 3 条是横切方形边缘拐角的弦。真正缺失的是声明：`cut_to_doc` 未输出任何曲线，`[V8]` 因而没有任何可与薄片边界比对的对象。

当一条薄片边界边的**两个**端点都位于某条排布**边缘**曲线的 `eps` 范围内时，该边被声明为边缘（`nodes_on_rim`）。有两处细节使该检查保持有效。其一，归属判定基于几何，而非读取 S7 的 `constraint_ref`——因为**本就**精确位于曲线上的节点不会产生吸附提案，也就不会记录 `Polyline` 约束；同时几何判定不会像"信任生产方的标志位"那样悄然沦为橡皮图章。其二，只有 `Rim` 类曲线够格：尖锐曲线或交线并非薄片可以终止的位置，接受它们会放过真正的针孔。该夹具现已 PASS，声明了 22 个边缘曲线单元，针孔数为 0。

因此 `split_R` 处于"已实现但未被触发"的状态，这是正确结果而非缺口：该表行针对的是切割前沿终止于格面**内部**某点的情形，而吸附使前沿终止于格**节点**——那是 §5.2 的普通表行。当边缘过粗以致无法吸附时，它仍是应有的回退路径，相关单元会升级为一致性扇形剖分并被记录。
- **笔直的尖锐棱边与交线依靠 `Curve` 驱动加密，而非弦高准则。**
  `curvature_sources` 与 `feature_sources` 度量的都是几何的**弯曲程度**，而这两者
  根本不弯曲——立方体的棱是直的，两张曲面横截相交也不产生曲率——于是二者都不发出
  任何源，尺寸场从不向网格必须复现的那些曲线加密。在 `curve_sources` 出现之前实测
  （2026-08-07）：A-1、A-2、A-3、A-4、A-7 全部报告 `geometry sources: 0 total`，而
  同时 S1 报告了 12 至 24 条曲线；A-8 面对 180 条尖锐曲线却报告 0 条特征曲线源。
  加入该准则后，A-7 的分量体积误差由 13.6 %/7.5 % 降至 0.30 %/0.06 %，
  A-8 由 9.40 % 降至 4.77 %。
- **`curve_cells` 与薄区制式相互影响。** `t_sheet` 与 `t_layer` 都是**收敛后** `h`
  的比例，因此任何压低 `h` 的准则都会收窄薄片窗口：`curve_cells = 2` 时窗口减半。
  若某夹具意在落入某一具名制式，其尺寸必须依据收敛后的 `h` 而非 `h_max` 来设定；
  当曲线捕获不如制式重要时，可用 `curve_cells: 1.0` 关闭该准则。
## 会话小结 2026-08-13 —— 重启门 G6-0，残留问题实为“面”一级

四个未通过用例的 `[V6] region_adjacency` 由 **1,944 降至 148**，九个用例的 `[V1]` 与
`[V3]` 全部干净，且**九个中已有七个通过所有实际运行的检查**。

| 用例 | 四面体数 | `[V6]` 邻接（原） | 体积误差 % |
|---|---|---|---|
| A-1 | 31,536 | 0 | 0.911 |
| A-2 | 111,756 | 0 | 0.000 |
| A-3 | 160,711 | **80**（122） | 0.711 / 0.392 |
| A-4 | 1,740,414 | 0 | 0.052 / 0.026 |
| A-6a | 240,542 | **21**（670） | 0.001 / 1.688 |
| A-6b | 985,174 | **47**（1,018） | 0.001 / 0.013 |
| A-7a | 141,186 | **0**（134） | 0.447 / 0.129 |
| A-7b | 113,550 | 0 | 1.069 / 1.257 |
| A-8 | 265,266 | 0 | 4.252 |

重启该门本是为了在单元内部实现受约束的曲线恢复，**但残留问题并不需要它**：每一处违规都可
追溯到某个**晶格面**的三角剖分未沿材料边界走，而 §7.6 本身未作改动。

- **`LoopFan` 的扇心丢失了曲线刺穿点查询。** 注释描述了该行为，代码却无条件调用
  `face_centroid`，而其下的 `meeting_nodes` 登记仍宣称该形心位于每一张曲面上。
  A-3 122 → 101，A-6a 670 → 49，A-6b 1,018 → 75。
- **锁定曲线进入 S8 时是一条折线，排除线段两端点会丢掉所有发生在曲线自身顶点处的穿越**——
  前一段报 `t = 1`，后一段报 `t = 0`，没有任何“下一段”会把它算作内部点。
  A-6a 49 → 37，A-6b 75 → 60。
- **被 S7 吸附到某片曲面**上的角点，正是该片曲面迹线的一个端点，而 `walk_positions` 只统计
  切割节点——于是这类迹线只有一个端点，无法配成弦，该面便退化为向其形心作扇。
  **A-7a 134 → 0**，A-3 101 → 79，且 A-3 在面一级的拒绝数降为零。
- **`cell_fan_is_conforming`** 在升级单元提交之前，先对它自己提出 `[V3]` 的两项检查：两个
  同胞碎片的形心都是在大量重合的节点集合上取平均，因而共面的概率远高于偶然。
- **`mixed sides` 放宽已正式启用**，并显式拒绝它真正引发的泄漏——盖片落在单元自身外边界上，
  从而被邻居再扇一次。A-6a 37 → 21，A-6b 60 → 47。

已证伪并移除：按**碎片**而非按四面体播种（A-7a 0 → 484，其体积误差 0.40 % → 4.17 %）、
按“哪些分量在此处穿越”对走线位置分组，以及在两者皆可用时优先采用共享端点的中枢扇
（A-6a 37 → 269）。

**诊断。** `RUSTMSPT_CUT_DIAG` 现在还会在 s08 快照中写出 `parent_cell` 单元数组，用以区分
“单个升级单元内部的缺陷”与“两个单元之间的分歧”。`RUSTMSPT_CURVE_PROBE=x,y,z` 列出某点附近
的锁定曲线段——坐标取**归一化**坐标系，故单位立方体域下需把 VTU 坐标乘以 `1/sqrt(3)`。
### 后续：曲线抵达某个面的第二种方式

`segment_pierces_triangle` 按设计只取严格内部——落在该面自身棱上的穿越点本就是两个单元共享
的节点——因此当曲线经过某个**走线节点**时不会给出刺穿点，该面便退回到形心。而 S7 本就会
刻意把节点吸附到锁定曲线上，故在缘线附近这才是常见情形。`nodes_on_curve` 标出这些节点，
`fan_from_walk_node` 以其中位于曲线上的那个为扇心三角化走线，且不新建任何节点：两个单元
都仅凭该面本身导出同一个扇心，因此 J1 与刺穿点情形一样成立。它在 A-6a 的 4 个面、A-3 的
16 个面上生效，对 `[V6]` 中性，并使 A-8 的体积误差由 4.252 % 降至 4.245 %。

`RUSTMSPT_JCT_DIAG` 现在会在每条 `[JCT-DIAG]` 前加上晶格单元索引，因此通过 s08 快照的
`parent_cell` 数组定位到的违规，可直接追溯到造成它的那次拒绝。
## `[V9] 结点检查` —— 2026-08-13 实现

自检查目录写成以来，`[V9]` 一直是 `SKIPPED`，理由为“随 G6-4 落地；需要曲线径向次序表”——
而 **G6-4 已被标记为 done，其验收标准正是“`[V9]` 在所有结点夹具上干净”**。现已在九个验收
用例上全部运行。

| 用例 | `[V9]` | 声明曲线 | 被承载 | 网格棱 | 缺失 `N_ID` | 径向 | 开口扇 |
|---|---|---|---|---|---|---|---|
| A-1 | PASS | 0 | 0 | 0 | 0 | 0 | 0 |
| A-2 | PASS | 12 | 12 | 408 | 0 | 0 | 0 |
| A-3 | **FAIL** | 114 | 40 | 134 | 1 | 0 | 0 |
| A-4 | PASS | 12 | 12 | 462 | 0 | 0 | 0 |
| A-6a | PASS | 34 | 21 | 751 | 0 | 0 | 0 |
| A-6b | PASS | 34 | 22 | 2,273 | 0 | 0 | 0 |
| A-7a | **FAIL** | 24 | 16 | 186 | 3 | 0 | 0 |
| A-7b | **FAIL** | 24 | 18 | 254 | 43 | 0 | 0 |
| A-8 | **FAIL** | 972 | 149 | 1,216 | 69 | 0 | 0 |

### 三项子检查

1. **曲线节点的 `N_ID` 包含该曲线的分量集合。** `PLAN §5.3` 将 `N_ID` 定义为相邻单元已解析
   标签与相邻带标记面的并集，因此位于分量 3 与 5 交线上的节点必须读作 `{…, 3, 5}`。
2. **径向面片计数相符。** S2 的 `radial_patch_order` 给出它在曲线周围排布了多少片，网格在
   沿该曲线的棱周围就必须显示同样多的材料扇区。仅在**两个及以上**分量相交处提出：单个实体
   的锐边周围，扇区无论面片数多少都只有内外两个。
3. **棱扇闭合。** 与结点棱相邻的每个面都恰好被其周围的两个四面体共享。若某棱相邻的面在整个
   网格中只被一个单元拥有则不作要求——位于网格自身边界上的棱无法闭合。

网格未以任何棱承载的曲线只作 `INFO` 上报而非判失败：门 G6-0 选择了协调扇而非受约束棱恢复，
故网格并不要求把每条曲线都重现为一条棱链。

### 先要补齐的东西

曲线表原本是**空的**（`CurveCompOffsets`/`CurveCompComponents` 在 `CurveKind` 非空的情况下
被写成零长数组），而 `n_id_key` 是**全零占位**——在后者之下第 1 项子检查会平凡通过。现在
S8 完整携带 S2 的曲线（`LockedCurve`），`curve_mesh_edges` 找出沿每条曲线的网格棱（两端点
**及中点**都要在曲线上，因为只测两端会接受一条离开曲线又折回的弦），`n_id_key` 按 §5.3 的
定义计算，并向契约新增了 `CurveRadialPatches` 列。

每条**棱**只发一个曲线单元，而非按 (曲线, 棱) 发：凡接触缘同时是两个实体各自的锐边处，就有
两条 S2 曲线沿同一几何走向，重复发出即构成 `[V1].duplicate_cell`。

### 它发现了什么

A-7b 的节点 2524 位于 (0.2017, 0.2017, 0.4217)，即下板自身的角点，而**与之相邻的 24 个四面体
全是背景**——该角点的材料整个丢失。A-3 唯一的失败是球/立方交线上一个读作 `{0, 1}` 的节点。
参考夹具 `good_cube` 本身也是错的：它沿节点 0-1-3 声明了一条分量 1 的特征曲线，而其自身的
区域布局在那里根本没有分量 1 的材料。

反例夹具 `bad_curve_node_id` 与 `bad_radial_patches` 证明前两项子检查确实会失败；第 3 项在
九个用例上均报 0，目前还没有反例夹具。
### 收束 `[V9]` 自身的局限

**第 3 项子检查现已有反例夹具，而构造它的过程恰好暴露了该子检查本身的缺陷。** 扇形闭合判定
原本会豁免任何与“只被一个单元拥有的面”相邻的棱——但开口扇本身就是带有单侧面的扇，于是该
豁免使这一子检查恰恰无法在它存在的意义所在的情形中触发；而验收矩阵（两种写法都报 0）也不
可能揭示这一点。现改用 `on_domain_plane`，与 `[V3]` 所用判定一致。`bad_open_junction_fan`
会触发 `V9.junction_fan`，并以 `V3.boundary_leak` 作为合理的连带项。

**`constraint_kind` / `constraint_ref` 同样是占位**（切割输出中写作 `vec![0]` / `vec![-1]`），
而 S7 早已算出二者——这是继 `n_id_key` 与曲线表之后，第三个被发现“契约声明、内容为空”的
数组。将它们贯通 S8 之后，`[V9]` 的每一处发现都有了名字：**每个失败节点都是被 S7 吸附到某条
锁定曲线上的节点**（A-7b 为 34 个 `Polyline` + 3 个 `Corner`，A-8 为 43 + 4），而最近的材料
约在一个单元之外。先行证伪的两个假设：这些节点并非近重合节点（84 个中有 0 个在 1e-6 内存在
邻居），其所在单元也并非单纯未被切割。
## `[V9]` 暴露出的 S7/S8 缺陷——根因与修复（2026-08-14）

`[V9]` 已在九个验收用例中的八个通过；仅 A-3 仍失败，且只涉及一个节点。

**根因。** §6 的单元表没有为“所有顶点都落**在**面片上”的单元准备行，而 S7 的吸附使这种构型
成为常态。其镜像情形早已修复——`has_inside && !has_outside`，即因其余顶点都落在面片上而整体
位于内部的单元，记录为某立方体体积的 12.9 %。而**完全没有** `Inside` 顶点的情形仍会落到
`uncut`，后者把 `inside` 硬编码为 false，于是该单元的材料被划归背景。这对任何拓扑检查都不
可见：网格依然水密且协调，只是所含实体比应有的少。

取自 A-7b 下板角点 (0.2017, 0.2017, 0.4217)：其一环的十四个邻居全部带有吸附约束，**没有
任何一个严格位于板内**，而其内部填充该角点卦限的四面体四个顶点全在边界上。

**修复。** `cut_one_cell` 针对该构型（无 `Inside` 顶点、至少一个 `OnCut` 顶点、无切割棱）
对单元内部取样，所用 `PointClassifier` 与 §7.5 处理升级碎片时相同。新增情形码 `b'i'`，并以
`[S8/G6-2] N cell(s) had every vertex on the patch` 上报。

| | 修复前 | 修复后 |
|---|---|---|
| `[V9]` A-7a / A-7b / A-8 | 3 / 43 / 69 | **0 / 0 / 0** |
| A-8 体积误差 | 4.245 % | **2.850 %** |
| A-7b 分量 2 | 1.257 % | **0.090 %** |
| A-6a 分量 2 | 1.687 % | **1.416 %** |

它在 A-8 的 343 个、A-7b 的 118 个、A-6a 的 10 个、A-7a 的 8 个单元上生效，在 A-3/A-4 上为 0。
各用例四面体数量不变——这是重新标注而非重新剖分，因此 `[V1]`、`[V3]` 与 `[V6]` 均未变动。

**为何改 S8 而非 S7。** 另一候选方案是拒绝该次吸附，但那更糟：捕获本身是正确的，节点**本就
应当**落在曲线上，拒绝它等于为了保留一个恰好能被表读懂的顶点而放弃特征。不完备的是表。

**遗留。** A-3 的节点 21054 位于球/立方交线上，其 `N_ID` 读作 `{0, 1}`，而曲线声明为
`{1, 2}`。本修复够不到它：该单元**确实**被切割，只是由分量 1 驱动，因此分量 2 的条目是经
`cut_record` 继承而非重新判定。这与 A-3 的 `[V6]` 属同一类倒角残留，并非独立缺陷。
### A-3 最后一个 `[V9]` 节点——受数值包络所限

节点 21054 是一个**切割节点**（`constraint_kind = 0`），产生于球面穿过棱 (7308, 7347) 之处——
这与 A-7/A-8 的失败节点不同，后者都是被 S7 吸附的节点。它位于立方体内侧 **5.30e-6** 处，而
`eps = eps_frac · diag = 1.73e-5`，即仅为 **0.31 eps**。

其四个母单元的记录均为 `record [(1, Ambiguous)]`——**分量 2 根本不在 S6 的记录中**，因为它们
没有任何一个顶点严格位于立方体内部（逐节点视图为 `["on", "on", "out", "out"]`）。节点 7347
恰好落在立方体面上（归一化坐标 `0.2886751345948129` 与 `0.5/√3` 逐位相同），7308 在其内侧
8.4e-6 处，而该单元的形心却在**外侧** 1.2e-3 处，为 70 eps。可见立方体在这些单元中的材料只是
包络尺度上的一片角落薄楔，而修复了其余 115 个节点的内部取样在此正确地给出“在外”。

**已尝试并回退：** 把结点分诊扩展到 `on_cut`（穿过单元顶点的曲面不留下任何交点，因此记录与
交点两者都指认不出它），并扩充记录使 §7.5 的播种能够考虑该隐藏分量。它在 A-3 的 4 个单元上
生效，但所有指标均无变化，代价是 +13 个四面体。该缺口确实存在；只是形心取样太粗，无法加以
利用——因为触发它的构型恰恰是隐藏实体只占据一片薄楔的情形。

保留下来的：`RUSTMSPT_CUT_CELL` 现在会打印单元的母记录，以及各分量下四个顶点的
`in`/`on`/`out` 视图。
