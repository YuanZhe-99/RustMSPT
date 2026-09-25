# `forge` 流水线示例

## 功能说明

`forge` 对网格施加自由变形 (FFD) “锻造”变换：它沿一个轴压缩网格，同时允许侧向鼓起
（在运动学上近似轴向压缩锻造，不涉及材料/应力模型），跟踪感兴趣区域 (ROI) 在变形过程中的
移动方式，重新对齐输出使 ROI 回到其原始请求的位置，并写出锻造后的 STL 以及一份记录变形前后
包围盒和 ROI 体积分数的文本报告。底层逐顶点变换的完整推导见
`../algorithms/ffd-forging.md`。

## 配置

`data/input/forge_config.yaml`（该流水线在仓库中真实的默认配置）：

```yaml
forging:
  # Input file path (can be absolute or relative to project root)
  input_stl_path: "data/output/optimized_structure.stl"

  # Output file path
  output_stl_path: "data/output/forged_mesh.stl"

  # Orientation fix after deformation (can be expensive)
  # false by default to skip; set true to enforce positive signed volume orientation.
  orient_to_positive_volume: false

  # Compression Ratio (0.0 to 1.0)
  # 0.2 means compressing the selected axis length by 20%
  compression_ratio: 0.5

  # Compression Axis
  # Choose which axis to compress: 'x', 'y', or 'z'
  # Default is 'z' when omitted.
  compression_axis: 'y'

  # Bulge Factor (Material Flow Control)
  # 0.0 = Vertical compression only (no side expansion)
  # 0.5 = Balanced volume conservation (Standard metal)
  # 1.0 = High side expansion (Soft material like rubber/clay)
  bulge_factor: 0.5

  # Region of Interest [min_x, min_y, min_z, max_x, max_y, max_z]
  # Leave empty [] to forge the entire model based on its bounding box.
  # Use this if you only want to squash a specific part of the geometry.
  roi_bounding_box: [0, 0, 0, 50, 50, 50]

  # Mesh Type
  # 'particle': only deform the original vertices
  # 'void': geometric deformation + void densification correction (suitable for void models, noticeable VF reduction)
  mesh_type: 'void'

  # Void Densification Strength (only effective when mesh_type is 'void')
  # 1.0 = standard densification
  # 2.0 = strong densification
  void_densification: 0.05
```

与 `scale` 示例一样，默认的 `input_stl_path` 指向 `data/output/optimized_structure.stl`
（`optimize` 的预期输出），而这个文件在全新检出的仓库中并不存在。本示例改为通过 `--input`
将 `forge` 指向真实存在的 `data/output/packed_result.stl`，并通过 `--output` 写入一个临时
路径。

| 字段 | 含义 |
|---|---|
| `cpu_max`（可选） | 本次运行的 worker 预算；缺省或 `-1` 使用全部可用 worker。 |
| `compression_ratio` | 压缩轴范围被移除的比例。`0.5` 表示压缩后的范围是原来的 50%，内部会被限制在不低于原始值的 1%（`axis_scale` 下限为 `0.01`）。 |
| `compression_axis` | 被压缩的轴（`x`/`y`/`z`）；另外两个轴接受侧向“鼓起”膨胀。省略时默认为 `z`。 |
| `bulge_factor` | 在无侧向膨胀（`0.0`）和体积守恒的侧向膨胀（`1.0`）之间进行（对数空间的）插值。 |
| `roi_bounding_box` | 一个 6 元素的 `[min_x, min_y, min_z, max_x, max_y, max_z]` 子区域，在变形过程中被跟踪；输出网格会被平移，使该区域回到其原始位置。留空/省略则对整个网格包围盒进行锻造。 |
| `mesh_type` | `"particle"` 只变形顶点；`"void"` 还会额外施加一个由 `void_densification` 缩放的孔隙致密化修正。 |
| `void_densification` | 额外致密化处理的强度，仅在 `mesh_type: 'void'` 时使用。 |

完整字段参考见 `../reference/config.md`（`forging.rs` 部分：`ForgingParams`、
`ForgingConfig`），准确的算法顺序（变形 -> 重新定向（可选） -> 平移以重新对齐 ROI ->
写出 STL + 报告）见 `../reference/pipeline-core.md` 中的 `ForgePipeline::run` 条目。

## 运行方式

```bash
./target/release/rustmspt forge \
  --config data/input/forge_config.yaml \
  --input data/output/packed_result.stl \
  --output /tmp/rustmspt-doc-examples/forged_mesh.stl
```

`forge` 还会在 STL 旁边写出一份文本报告，路径相同但扩展名替换为 `.txt`——此处即为
`/tmp/rustmspt-doc-examples/forged_mesh.txt`。

## 预期输出

上述运行中真实捕获的 stdout：

```
[Info] Forging completed.
[Info] BBox before: min=(-10.7846,2.8307,3.6017), max=(104.6636,105.4615,97.3297)
[Info] BBox after:  min=(-12.7416,1.4468,4.3634), max=(124.3786,52.6980,115.6861)
[Info] ROI BBox before: min=(0.0000,0.0000,0.0000), max=(50.0000,50.0000,50.0000)
[Info] ROI BBox after:  min=(0.0000,0.0000,0.0000), max=(59.4604,25.0000,59.4604)
[Info] Spatial ROI VF before: 0.024314
[Info] Spatial ROI VF after:  0.024208
[Info] Compression axis: y
[Info] Orientation fix enabled: false
[Info] Output translation: (8.8813,-27.0730,9.5485)
[Info] Output STL written: /tmp/rustmspt-doc-examples/forged_mesh.stl
[Info] Output report written: /tmp/rustmspt-doc-examples/forged_mesh.txt
```

`/tmp/rustmspt-doc-examples/forged_mesh.txt` 的真实内容：

```
Method: forge
BBox before: min=(-10.7846,2.8307,3.6017), max=(104.6636,105.4615,97.3297)
BBox after:  min=(-12.7416,1.4468,4.3634), max=(124.3786,52.6980,115.6861)
ROI BBox before: min=(0.0000,0.0000,0.0000), max=(50.0000,50.0000,50.0000)
ROI BBox after:  min=(0.0000,0.0000,0.0000), max=(59.4604,25.0000,59.4604)
Spatial ROI VF before: 0.024314
Spatial ROI VF after:  0.024208
Compression axis: y
Orientation fix enabled: false
Output translation: (8.8813,-27.0730,9.5485)
```

报告文本几乎与 stdout 的 `[Info]` 行完全一致，只是少了两行路径确认信息（这两行只有在控制台
场景下才有意义）。此次运行耗时 `real 0m0.120s`——`forge` 是单次确定性处理过程（逐顶点仿射
变换，外加在 `mesh_type: 'void'` 情况下的一次致密化处理），因此其耗时与三角形数量成比例，
不涉及迭代/随机成本。

注意 ROI 的 Y 方向范围（压缩轴）从 50 缩小到 25 个单位——恰好对应所请求的
`compression_ratio: 0.5`（缩减 50%）——而其 X 和 Z 方向范围从 50 增长到约 59.46，这是
`bulge_factor: 0.5` 带来的侧向鼓起膨胀。ROI 体积分数几乎没有变化（`0.024314` ->
`0.024208`，相对变化约 0.4%），因为 `mesh_type: 'void'` 配合较小的 `void_densification: 0.05`
只在几何变换之上施加了轻微的修正。

## 说明

- `roi_bounding_box` 必须恰好包含 6 个元素才会生效——长度错误的列表会被静默地当作“无 ROI”
  （对整个网格锻造）处理，而不会引发配置错误；参见 `../reference/pipeline-core.md` 中的
  `ForgePipeline::parse_roi_bbox`。
- `compression_axis` 仅接受 `x`/`y`/`z`（大小写不敏感，会去除首尾空白）；其他任何值都会是
  硬性配置错误（`InvalidConfig`），而不是静默回退。
- 根据 pipeline-core 参考文档，`ForgePipeline::run` 会计算变换前后的整网格体积，但目前会
  丢弃这些结果（绑定到以 `_` 为前缀的变量）——报告中只会呈现受 ROI 限制的体积分数，这也是
  本示例的 stdout 显示 `Spatial ROI VF` 而不是整网格 VF 数值的原因。
- 对于 `mesh_type: 'void'`，将 `void_densification` 提高到 `1.0`–`2.0` 会产生比这里使用的
  `0.05` 明显得多的体积分数变化；默认配置选用 `0.05` 是为了让修正效果保持轻微。
- `axis_scale` 与 `lateral_scale` 如何由 `compression_ratio` 和 `bulge_factor` 推导得出的
  闭式推导，以及 `simulate_forging_ffd_with_tracking` 如何将单轴版本推广到任意压缩轴和
  调用方提供的 ROI/晶格包围盒，见 `../algorithms/ffd-forging.md`。

### 计时行（2026-09-25 新增）

上面的捕获输出早于共享阶段计时器。当前版本还会为每个已完成阶段打印 `[Timing] forge stage=<name> seconds=<f>`，随后打印 `[Timing] forge workers=<n>` 与 `[Timing] forge peak_rss_bytes=<n|unavailable>`。阶段名称见 `../reference/pipeline-core.md`（`pipeline/timing.rs`）；输出文件不变。
