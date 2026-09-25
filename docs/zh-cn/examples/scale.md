# `scale` 流程示例

## 功能说明

`scale` 通过一个统一的缩放因子来重新缩放 STL 网格，该因子可由物理单位换算
（`mm_per_voxel` 或 `voxel_per_mm`）导出，也可以是显式的乘法 `factor`；之后可选地将各连通分量
的朝向修正为正的有符号体积。它通常是流程中的最后一步，用于将以“体素”或任意 CAD 单位生成的几何体
在进一步分析或导出之前转换为真实物理单位（毫米）。

## 配置

`data/input/scale_config.yaml`（该流程在仓库中真实的默认配置）：

```yaml
input:
  stl_path: "data/output/optimized_structure.stl" # STL file or folder containing STL files

output:
  stl_path: "data/output/scaled_mesh.stl"

scaling:
  # Options: "mm_per_voxel", "voxel_per_mm", "factor"
  type: "voxel_per_mm"
  # Value corresponding to the type
  value: 37.5
  # Orientation fix before output (can be expensive)
  # false by default to skip; set true to enforce positive signed volume orientation.
  orient_to_positive_volume: false
```

默认的 `input.stl_path` 指向 `data/output/optimized_structure.stl`，这是 `optimize` 流程的预期
输出。在本示例中，我们改为让流程指向 `data/output/packed_result.stl`——一个仓库中已存在的真实网格，
由此前一次 `pack` 运行产生——通过下文所述的 `--input`/`--output` 命令行覆盖参数实现，因此
`data/input/` 或 `data/output/` 下的任何文件都无需改动。

| 字段 | 含义 |
|---|---|
| `scaling.type` | `"mm_per_voxel"` 和 `"voxel_per_mm"` 是单位换算模式；`"factor"` 则直接将 `value` 作为乘数应用。`"voxel_per_mm"` 会在应用前对 `value` 取倒数（`factor = 1.0 / value`）。 |
| `scaling.value` | 对于 `mm_per_voxel`/`voxel_per_mm` 必须 `> 0`（会进行校验；非正值属于配置错误）。在 `"factor"` 模式下直接被解释为缩放因子。 |
| `scaling.orient_to_positive_volume` | 为 `true` 时，会在缩放后运行 `orient_components_to_positive_volume`，并报告有多少网格分量被翻转。此处保留为 `false` 以跳过这一额外开销。 |

完整字段参考见 `../reference/config.md`（`scale.rs` 部分：`ScalingParams`、`ScaleConfig`）。

## 运行方式

```bash
./target/release/rustmspt scale \
  --config data/input/scale_config.yaml \
  --input data/output/packed_result.stl \
  --output /tmp/rustmspt-doc-examples/scaled_mesh.stl
```

在流程运行之前，`--input`/`--output` 会覆盖从已加载 YAML 中读取的
`scaling.input.stl_path`/`output.stl_path`（参见 `src/main.rs` 中的 `Commands::Scale` 分支），
这也是本示例得以避免依赖磁盘上存在 `optimized_structure.stl` 的方式。

## 预期输出

以上运行实际捕获的 stdout：

```
[Info] Scaling started.
[Info] Mode: voxel_per_mm | Value: 37.500000
[Info] Original bounds: min=(-10.7846,2.8307,3.6017), max=(104.6636,105.4615,97.3297)
[Info] Original volume: 22912.609296
[Info] Scaling completed with factor 0.026667.
[Info] Orientation fix enabled: false
[Info] Scaled bounds: min=(-0.2876,0.0755,0.0960), max=(2.7910,2.8123,2.5955)
[Info] Scaled volume: 0.434491
```

`scale` 仅写出变换后的 STL（`/tmp/rustmspt-doc-examples/scaled_mesh.stl`）；它没有单独的文本报告——
所有相关信息都如上所示打印到 stdout。此次运行在远小于一秒的时间内完成（本机上 `real 0m0.100s`）；
`scale` 是对网格顶点的单次遍历，不含任何迭代或随机成分，因此耗时大致与三角形数量呈线性关系，
对于这种规模的网格（此处为 12,084 个三角形），耗时主要由 STL 的 I/O 主导。

当 `voxel_per_mm = 37.5` 时，有效缩放因子为 `1 / 37.5 ≈ 0.026667`，与打印出的
`Scaling completed with factor 0.026667` 一行相符，并将约 105 单位的包围盒缩小到约 2.8 毫米，
这与将堆积几何体的原始单位视为该分辨率下的体素这一假设是一致的。

## 说明

- `input.stl_path`（以及 `--input`）也可以接受一个包含多个 STL 文件的文件夹，这些文件会在缩放前
  通过 `load_stl_or_merge_folder` 合并——当颗粒几何体是以每颗粒一个 STL 文件的形式导出，
  而非合并为单个网格时，这一点会很有用。
- 体积按线性缩放因子的立方进行缩放：原始体积 `22912.6` -> 缩放后体积 `0.434491`，与
  `22912.6 * 0.026667^3 ≈ 0.434` 相符。
- 将 `scaling.type` 切换为 `"factor"` 并直接把 `scaling.value` 设为所需的乘数
  （例如 `0.026667`），会得到与上面 `voxel_per_mm` 示例完全相同的结果——当你已经知道线性缩放
  因子而非体素分辨率时，这一方式很有用。
- 在下游那些假设各分量已一致缠绕、体积为正的体积分数或堆积密度计算之前，值得启用
  `orient_to_positive_volume: true`；当你知道网格已经具有良好朝向时，保持其为 `false`
  （如默认配置所示），因为该检查在大型网格上每个分量的开销并不小。
- 关于 `ScalePipeline::run` 的完整行为参考，包括 `scaling.type`/`value` 组合无效时的确切错误
  条件，见 `../reference/pipeline-core.md`。

### 计时行（2026-09-25 新增）

上面的捕获输出早于共享阶段计时器。当前版本还会为每个已完成阶段打印 `[Timing] scale stage=<name> seconds=<f>`，随后打印 `[Timing] scale workers=<n>` 与 `[Timing] scale peak_rss_bytes=<n|unavailable>`。阶段名称见 `../reference/pipeline-core.md`（`pipeline/timing.rs`）；输出文件不变。
