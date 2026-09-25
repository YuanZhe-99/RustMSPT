# `mesh-render` — 将 VTU 体网格渲染为多视角 PNG

渲染契约 VTU（参见 `docs/en-us/reference/mesh-render-and-vtu.md` 与
`PLAN_mesh_generation.md` §7）——或任何符合受支持子集的普通四面体 VTU——
为每个配置的视角输出一张 PNG，支持过滤器、逐区域透明度以及特征曲线叠加。
这是网格生成模块（PLAN GA 阶段）的调试/可视化工具；它先于 `mesh` 主管线本身
实现，以便在开发过程中检查中间快照和外部网格。

## 运行方式

```bash
./target/release/rustmspt mesh-render --config data/input/mesh_render_config.yaml
# 覆盖参数：
./target/release/rustmspt mesh-render --input data/output/mesh.vtu --output data/output/mesh_render
```

`--input` 替换 `mesh_render.input`；`--output` 替换 `mesh_render.output_dir`。
输出文件名为 `<input_stem>_<view>.png`。

## 配置字段说明（`data/input/mesh_render_config.yaml`）

| 字段 | 含义 |
|---|---|
| `input` | `.vtu` 文件路径（ascii 或 appended-raw 编码，未压缩） |
| `output_dir` | PNG 输出目录（不存在时自动创建） |
| `views` | 命名预设列表（`front`、`back`、`left`、`right`、`top`、`bottom`、`iso_ne`、`iso_nw`、`iso_se`、`iso_sw`）和/或自定义相机块 `{name, view_direction, focus_point?, up_vector?}` |
| `width`、`height` | 图像尺寸（默认 1024） |
| `background` | RGB 或 RGBA；alpha 为 0 时生成透明背景的 PNG |
| `color_by` | `uniform` 或某个单元数组名——整型数组使用分类调色板，浮点数组使用 viridis 色带（`scalar_min`/`scalar_max` 可限定取值范围） |
| `volume_opacity` / `face_opacity` | 四面体边界面 / 带标签面单元的不透明度 |
| `opacity_overrides` | region_key（字符串形式）→ 不透明度的映射，例如 `{ "0": 0.15 }` 可使背景区域透明可见而界面保持不透明 |
| `show_faces` / `show_curves` / `wireframe` | 叠加显示开关（带标签的面、特征曲线折线、边界面线框） |
| `filters` | 以"与"组合、按类型标记的过滤器；见下文 |
| `highlight_points` | 以世界坐标绘制的、始终置于最前的十字标记点（例如取自验证报告的坐标） |
| `projection`、`perspective_fov_degrees`、`camera_distance`、`fit_padding` | 相机模型，与 STL `render` 管线共用 |

### 过滤器

```yaml
filters:
  - { kind: cell_kind, values: [0] }          # 0 表示四面体，1 表示面，2 表示曲线
  - { kind: region_key, values: [1, 2] }
  - { kind: partition, values: [1] }
  - { kind: regime, values: [1, 2] }          # 薄层带 / 带-Steiner 四面体
  - { kind: component, values: [3] }          # 通过 region/face-tag/curve 表查询
  - { kind: background, keep: false }         # 丢弃背景四面体
  - { kind: array_range, array: aspect_ratio, min: 10.0, max: 1.0e30 }
  - { kind: bbox, min: [0, 0, 0], max: [1, 1, 0.5] }
  - { kind: clip_plane, origin: [0.5, 0.5, 0.5], normal: [0, 0, 1] }
```

属性过滤器只作用于携带该属性的单元（`region_key` 过滤器不会隐藏带标签的面或曲线）；
`bbox`/`clip_plane` 按质心作用于每个单元，产生波浪形裁剪——幸存单元的内部面会
自动暴露为边界面。若过滤器所指定的数组在该 VTU 中不存在，会报错并指明该数组名。

### 常见调试配置

透明背景基体、界面与曲线保持不透明：

```yaml
color_by: region_key
opacity_overrides: { "0": 0.12 }
show_faces: true
show_curves: true
```

质量排查（在 `mesh-verify --annotate` 添加质量数组之后）：

```yaml
color_by: min_dihedral_deg
filters:
  - { kind: array_range, array: aspect_ratio, min: 5.0, max: 1.0e30 }
```

## 验证输出

每个视角都会打印 `[mesh-render] wrote <path>`；运行时还会打印场景统计信息
（`N cells -> T triangles, S segments, M markers`）。输出的 PNG 为 RGBA 格式；
当 `background` 的 alpha 为 0 时，外部像素完全透明（可用任意图像工具核实）。
集成测试套件（`tests/mesh_render_tests.rs`）覆盖了 I/O 回环、抽取计数、
过滤器报错、透明度合成的解析验证、命名视角，以及本管线的端到端流程。GA-5
另外在 `data/fixtures/meshgen/render_baselines/cpu/` 中提交了 `good_cube.vtu`
的 CPU PNG 基线，覆盖四个诊断变体和全部十个具名视角。普通测试会比较这些文件；
只有在明确需要时才重新生成：

```bash
RUSTMSPT_UPDATE_RENDER_BASELINES=1 cargo test --test mesh_visual_regression_tests
```

GPU 基线测试将不透明变体与 CPU 参考实现比较；无可用适配器时跳过。透明 GPU
输出按设计排除，因为 CPU 路径才是精确透明度参考。

### 计时行（2026-09-25 新增）

上面的捕获输出早于共享阶段计时器。当前版本还会为每个已完成阶段打印 `[Timing] mesh-render stage=<name> seconds=<f>`，随后打印 `[Timing] mesh-render workers=<n>` 与 `[Timing] mesh-render peak_rss_bytes=<n|unavailable>`。阶段名称见 `../reference/pipeline-core.md`（`pipeline/timing.rs`）；输出文件不变。
