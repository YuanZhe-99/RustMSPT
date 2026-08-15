# `mesh` - 生成四面体体网格（S0+S1+G2-1..G2-5+G3+G4+G5 已实现）

`mesh` 子命令将输入 STL 面转换为契约四面体体网格（`PLAN_mesh_generation.md`）。S0（调理 + 修复）、S1（特征检测）、G2-1..G2-3（排布/覆盖/退化门控）、G2-5（混合宽相 + 盒裁剪）、G2-4（拓扑重建 + GWN）、S3（G3-1 分离场、G3-2 薄区域分割与 S3<->S4 耦合驱动）、S4（G4-1 尺寸场与梯度控制）、S5（G4-2 平衡背景晶格）、S6（G5-1 分类）、S7（G6-1 吸附）与 S8（G6-2..G6-5 切割）已实现。流水线在盒裁剪与拓扑重建完成后产出 `s02_arranged`，S3 之后产出 `s03_gapfield`，S4 之后产出 `s04_sizing`，S5 之后产出 `s05_lattice`，S6 之后产出 `s06_classified`，S7 之后产出 `s07_snapped`，S8 之后产出 `s08_cut`，随后仅对 S9..S11 返回 `NotAvailable`。

**在 S8 之前网格呈阶梯状，这是预期的；`s08_cut` 才是贴合输入的那一个。** 这是一款浸入边界网格生成器：晶格是不理会几何的*背景*网格，S6 判定各单元位于何物之内，S7 移动那几百个落在特征上或挡住切割的节点，唯有 S8（切割）才使边界贴合输入。

完整配置契约、默认值与解析期拒绝见 [`reference/meshgen.md`](../reference/meshgen.md)。

## 运行

```bash
./target/release/rustmspt mesh --config data/input/meshgen_config.yaml
# 覆盖：
./target/release/rustmspt mesh --input data/input/particles.stl --output data/output/mesh.vtu
```

`--input` 将 `meshgen.inputs` 替换为单条 STL；`--output` 替换 `meshgen.output.vtu`。

## 当前默认运行

```
$ ./target/release/rustmspt mesh --config data/input/meshgen_config.yaml
[mesh] config validated; 1 input(s):
  - data/input/particles.stl (priority 0, kind auto)
  domain [0,0,0] - [1,1,1]
  sizing h_max_frac=0.05 h_min_frac=0.002 chord_error_frac=0.2 feature_angle_deg=45
  gaps t_layer_factor=1 t_sheet_factor=0.2 confidence_min=0.9
  envelope eps_frac=0.0001 repair=Conservative coincidence=Merge fem_profile=Implicit determinism=Strict snapshots=Key
  output vtu=data/output/mesh.vtu abaqus=- report=data/output/mesh_verification
[S0] conditioned: 2830 -> 2830 verts (0 welded), 5600 faces, 15 components
[S1] features: 9 curves, 0 junctions, 0 corners
[S2/G2-1..G2-3] arrangement: 25194 candidates, 0 proper intersections, 0 coplanar overlays, 2830 registry vertices, 0 segments, 5600 faces, 25131 point features, 25131 coincidence events, 0 degraded neighborhoods
[G2-5b] box clip: 5600 -> 0 faces, 9 -> 0 curves
[G2-4] topology rebuild: 0 solid (0 closed, 0 defective), 0 sheet, 0 closure defects
[mesh] snapshot s02: data/output/mesh.debug/mesh_s02_arranged.vtu
[S3/G3-1] gap field: 0 samples (0 ray-paired, 0 closest pairs, 0 densified), 0 groups, t_sheet=0.017321 t_layer=0.086603
Error: Algorithm not available yet: mesh: stages S9..S11 are not implemented yet; S0+S1, G2-1..G2-5 arrangement+clip+topology, G3 gap field + regimes, G4-1 sizing field + grading, G4-2 balanced lattice, G5-1 classification, G6-1 snap, and G6-2/G6-3 cut complete; 1 input(s) loaded, output 'data/output/mesh.vtu'
```

这个 5,600 三角形样例可完整通过 G2 流水线：混合宽相（中值幅度均匀网格）无需旧的 50x 工作量保护即可完成候选生成。盒裁剪丢弃了全部面，因为 particles.stl 的坐标范围 [0,100] 与区域 [0,1] 不匹配；请使用顶点位于配置区域内的输入以获得非空输出（没有幸存面时 S3 无可采样，故间隙场样本数为 0）。退出码非零，因为 S9..S11 尚未实现。

`snapshots: all` 在 `<output_stem>.debug/` 下产出 `s00_conditioned`、`s01_features`、`s02_arranged` 与 `s03_gapfield`；`snapshots: key` 仅产出 `s02_arranged`（冻结 key 集为 `{s02,s05,s08,s11}`）。每个输入 STL 在打印计划前预加载，故文件缺失或无法解析会立即失败：

```
$ ./target/release/rustmspt mesh --config /tmp/bad_meshgen.yaml
Error: Invalid config: meshgen.inputs: failed to read STL 'data/input/__missing__.stl': No such file or directory (os error 2)
```

## 解析拒绝示例

违反 PLAN §6.3 区域正定性规则的配置会在读取任何 STL 前被拒绝：

```
$ cat /tmp/bad_meshgen.yaml
meshgen:
  inputs:
    - stl: data/input/particles.stl
  domain: { min: [0,0,0], max: [1,0,1] }
  output: { vtu: data/output/mesh.vtu }
$ ./target/release/rustmspt mesh --config /tmp/bad_meshgen.yaml
Error: Invalid config: meshgen.domain is nonpositive on axis 1: min 0 must be < max 0
```

完整拒绝集合（区域正定性、`t_sheet_factor < t_layer_factor`、`eps_frac < 0.5 * t_sheet_factor * h_min_frac`、重复组件覆盖）记录于 [`reference/meshgen.md`](../reference/meshgen.md)，并由 `tests/meshgen_config_tests.rs` 覆盖。

## 查看分离场

当输入位于配置区域内时，S3 会报告其采样、配对以及低于 `gaps.confidence_min` 的分组：

```
[S3/G3-1] gap field: 7078 samples (1159 ray-paired, 22748 closest pairs, 0 densified), 25 groups, t_sheet=0.017321 t_layer=0.086603
[S3/G3-2] regions: 391 total (1 sheet, 2 band, 388 skipped)
[THIN] region 0 (Inter(1, 2), component 1 side 1 vs patch 1) -> Band: t_r=0.020000, confidence 1.000, 355 samples, 1 rim loop(s)
[THIN] region 212 (Intra(3), component 3 side -1 vs patch 2) -> Sheet: t_r=0.008000, confidence 1.000, 350 samples, 1 rim loop(s), mid-surface 72 triangles
[S3/S4] coupling: 1 iteration(s), converged=true, h=0.086603, t_sheet=0.017321, t_layer=0.086603
[mesh] snapshot s03: out/mesh.debug/mesh_s03_gapfield.vtu
```

每个已转换区域会给出其配对类别、冻结的 `t_r`、置信度与边缘环数量；`Sheet` 区域还会报告所构建的中面。声明为薄但未通过门控的区域按原因聚合输出，以免粗网格输入用大量 WARN 淹没关键信息：

```
[THIN-SKIP] WARN: 388 region(s) declared thin but kept volumetric (Speck)
  region 1 (Inter(1, 2), component 1 side 1 vs patch 1) declared Band: t_r=0.053333, confidence 1.000, 1 samples, failed checks 1-5 [0, 0, 0, 0, 0]
  ... and 383 more
```

`t_r` 与阈值均以模型单位打印。`s03_gapfield` 快照以 `separation_t` 点数组携带该场，`mesh-render` 可直接着色：

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s03_gapfield.vtu
  color_by: separation_t
  scalar_min: 0.0
  scalar_max: 0.03
```

状态输出同样可渲染：`color_by: thin_role` 区分普通壁、已转换壁与中面，`color_by: band_region` 为每个区域着色，`array_range` 过滤器可单独隔离中面：

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s03_gapfield.vtu
  color_by: thin_role
  filters:
    - { kind: array_range, array: thin_role, min: 2.0, max: 2.0 }
```

## 查看尺寸场

S4 报告其源、耦合循环最终采用的约束，以及构建出的八叉树：

```
[S4/G4-1] geometry sources: 1284 total (1208 curvature, 68 feature curve, 8 corner)
[S3/S4] coupling: 2 iteration(s), converged=true, h=0.030000, t_sheet=0.006000, t_layer=0.030000
[S3/S4] constraint bound by region 0 (local feature size): C=0.030000, t_r=0.060000, 355 sample(s)
[S4/G4-1] sizing field: 4126 sources (+2842 LFS), 38104 leaves, levels 0..7, h in [0.002000, 0.050000], grading 2.00
[mesh] snapshot s04: out/mesh.debug/mesh_s04_sizing.vtu
```

当网格比预期更细时，应当阅读 `constraint bound by` 这一行。`C(R)` 是全模型唯一的标量，故恰有一个区域设定了状态阈值；该行给出它是哪个区域、绑定项是曲率/特征还是局部特征尺寸，并报告该区域的分离量与采样数。若其为一个 `t_r` 极小的单采样区域，则该测量值得怀疑——这正是被 S3 自身否决的区域被完全排除在该项之外的原因。

`s04_sizing` 把场存放在 `cell_kind = 3` 体素预览的角点上（数组名 `sizing_h`），故 `mesh-render` 可像着色任何其它场一样着色它。剖切平面是使其可读的关键——它切入八叉树、暴露内部梯度，而非只让你看到外壳：

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s04_sizing.vtu
  color_by: sizing_h
  filters:
    - { kind: clip_plane, origin: [0.5, 0.5, 0.5], normal: [0.0, 1.0, 0.0] }
```

应当看到：每个弯曲面片、特征曲线与体网格间隙周围呈现尺寸递减的同心带，每条带约一个单元宽（这正是 `grading: 2.0` 的梯度——跨越一个单元尺寸至多翻倍），其余处为平坦的 `h_max` 平台。若整场一律为 `h_min`，说明某个源的请求超出应有范围；若整场从不离开 `h_max`，说明没有任何准则生效——对弯曲输入而言，这通常意味着 `chord_error_frac` 过于宽松，未能在上限之前起作用。

## 查看背景晶格

S5 报告平衡趟次、模板分布与单元预算：

```
[S5/G4-2] lattice: 168841 leaves after 1095 balance split(s) (140497 Freudenthal, 28344 fan), 297771 nodes, 1651366 tets, V in [1.242e-3, 4.069e1]
[mesh] snapshot s05: out/mesh.debug/mesh_s05_lattice.vtu
```

`s05_lattice` 属**关键**快照，默认即会写出。它是真正的四面体网格，因而验证器的协调性检查可直接施加于其上——这也是每次改动晶格代码后最值得运行的检查：

```bash
rustmspt mesh-verify --config verify.yaml
```

```
[PASS] [V3] Conformity
       interior_faces=3278560  boundary_faces=48344  multi_shared_faces=0  boundary_leaks=0  hanging_nodes=0  non_manifold_edges=0
[PASS] [V4] Quality
       worst_aspect_ratio=1.60517  min_dihedral_deg=35.26439  aspect_ratio_over_gate=0  below_low_dihedral=0
```

`multi_shared_faces = 0` 与 `hanging_nodes = 0` 即定理 T1——晶格按构造即协调且无悬挂节点。`min_dihedral_deg = 35.264` 是 `arctan(1/√2)`，即最差的过渡扇形单元；这是预期值，而非警讯。

若要查看层级跳变的位置及其代价，可为网格标注逐单元质量数组并据此着色——过渡处若存在薄片带，会呈现为连通的深色区域，而实际上不应出现：

```yaml
mesh_verify:
  input: out/mesh.debug/mesh_s05_lattice.vtu
  annotate: out/lattice_annotated.vtu
```

```yaml
mesh_render:
  input: out/lattice_annotated.vtu
  color_by: min_dihedral_deg
  scalar_min: 30.0
  scalar_max: 50.0
  wireframe: true
  filters:
    - { kind: bbox, min: [0.0, 0.49, 0.0], max: [1.0, 0.51, 1.0] }
```

## 查看分类结果

S6 报告它判定了什么，以及同样有用的——它*如何*判定：

```
[S6/G5-1] classification: 47400 vertices x 3 solid component(s) (0 sheet, 0 defective), 19195 inside pair(s); tets 136468 background / 89718 owned / 42762 straddling; 4 region key(s); 0 inactive face(s)
[S6/G5-1] rays: 142200 first-shot, 0 re-shot, 0 exhausted (winding-number fallback), 0 by winding number on a defective component (by design), 13758 exact-predicate escalation(s)
```

值得关注的是第二行。`first-shot` 基本应当是全部；`re-shot` 统计恰好穿过边或顶点的射线；`exhausted` 统计耗尽全部五个方向的判定，应为零。若非零，意味着几何以射线序列无法解决的方式退化——并不意味着答案错误（卷绕数仍会作出判定），但值得查看。另计的 `by winding number on a defective component` **不是**回落：S2b 无法认证该构件闭合，故奇偶性对它无定义，卷绕数才是正确路径。`exact-predicate escalation` 只是静态滤波移交精确谓词；在与晶格对齐的输入上，其数量偏大属于预期。

`s06_classified` 携带解析后的 `region_key`，故隐藏背景即可看到各构件占有了哪些单元：

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s06_classified.vtu
  color_by: region_key
  filters:
    - { kind: background, keep: false }
```

结果是输入的**阶梯化**版本——精度为一个单元，因为此时尚未移动任何顶点。这正是 S7 与 S8 要解决的问题。`color_by: arbitrated` 可显示它们将要处理的跨越壳层。

## 检视吸附结果

S7 会报告它移动了什么、又给 S8 留下了什么：

```
[S7/G6-1] snap: 23766 crossing(s) on 23610 of 317883 edge(s); 12407 candidate(s) -> 33 corner / 272 curve / 0 surface; 1354 on-cut node(s)
[S7/G6-1] moves: 0 capped, 0 rejected, 0 box-constrained, 0 promoted by the re-check (0 residual); motion max 8.839e-3, mean 2.056e-3
[CUT-CASE] 156 edge(s) are crossed more than once by one component (invariant K1); S8 must refine or escalate them
```

读法如下。**crossings** 是 S8 将要切割之处；**on-cut nodes** 是已经落在面片上的晶格顶点，切割将穿过它们而非另行取点。三个吸附计数体现冻结的优先级——节点取其 `0.3 * L_min` 半径内排序最高的目标，而*曲面*目标只在切割本会落到该节点 2.5% 以内时才采用（参见 `reference/meshgen.md` 的「曲面为何不作捕获目标」）。`capped` 与 `rejected` 对应 ARB-11 与 ARB-10：被上限截断的移动会使该节点保持欠吸附状态并交由 [V5] 门处理，而会翻转相邻四面体的移动则直接拒绝。此处两者皆为零，说明每个目标都可达。`residual` 统计本趟之后仍位于距边端 2.5% 以内的交点，应为零。

`[CUT-CASE]` 一行是不变量 K1：被同一构件穿越两次的边（薄于一个单元的板件即会如此）无法用单面片表处理，S8 必须细化或升级这些单元。

`s07_snapped` 逐节点携带 `constraint_kind`——`0` 自由、`1` 曲面、`2` 折线、`3` 角点、`4` 盒面——以及 `snap_motion`，即每个节点移动的距离：

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s07_snapped.vtu
  color_by: snap_motion
```

预期它几乎处处为零：S7 捕获特征，并不把网格揉成形状。形状要到 S8 才出现。

## 检视切割结果

S8 是使网格贴合的阶段：

```
[S8/G6-2] cut: 34897 of 268948 cell(s) cut (383 A / 762 B / 22641 C / 11111 D; 0 by a welded sheet), 23610 cut node(s), 268948 -> 408518 tets, 46008 interface face(s)
[S8/G6-2] quality: min dihedral 0.597 deg, worst volume error 9.519e-15; 1423 cell(s) escalated
[JCT-FALLBACK] 1423 escalated cell(s) re-meshed as a conforming centroid fan (972 face(s) off the frozen table); their material boundary is chamfered by at most one cell
[CUT-3EDGE] 235 cell(s) have an edge one component crosses more than once (invariant K1)
[CUT-CASE] 1188 cell(s) have a side/crossing configuration that is not a legal §6 row
```

**A/B/C/D 计数**是冻结的 §6 情形，按母单元四个节点中有几个位于面片内部划分。**最差体积误差**是受保护试运行的核心数字：每次切割的碎片之和与母体相差须在 1% 以内，而 9.5e-15 表明其吻合到舍入级别。**升级**单元是 §6 无法处理的那些——两个面片、某条边被穿越两次，或表格未覆盖的状态——它们*并未*被跳过：它们被重新划分为协调质心扇形，从而保持网格有效且协调，其材料边界至多按一个单元尺度倒角。

`s08_cut` 在已贴合几何的单元上携带真实的 `region_key`，另有带标签的界面三角形（`cell_kind = 1`）及其 `(内侧, 外侧)` 单元对：

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s08_cut.vtu
  color_by: region_key
  filters:
    - { kind: background, keep: false }
```

与 `s06` 不同，这一个在输入是圆的地方就是圆的。用 `mesh-verify` 验证：`[V3]` 应报告悬挂节点、非流形边与边界泄漏均为零。`[V4]` **会**告警——切割在构造上就会产生薄片，那些单元是 S9 质量阶段的输入预算，而非缺陷。
