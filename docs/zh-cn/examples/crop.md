# `crop` 流程示例

当前版本还为已完成阶段输出 `[Timing] crop stage=<name> seconds=<value>`：`load`、`background`、`pca`、`transform_and_backend`、`trim`、`encode_write`、`total_in_pool`。下方历史捕获输出早于这些计时行。后端阶段包含规划、初始化、传输及允许的回退；写出包含 flush，但不含 fsync。池内总时间不含 CLI、配置及建池；数值依实际运行而变化。

## 功能说明

`crop` 加载原始 CT 切片堆栈（或 TIFF 堆栈），根据自动检测到的背景值对其进行阈值处理，
通过主成分分析（PCA）找到前景体积的主轴，将体积旋转以使这些主轴与 X/Y/Z 对齐，
然后将其裁剪到最紧凑的轴对齐包围盒（可选地进行边缘裁边以去除旋转引入的锯齿状边界）。
它通常是流程中的第一步，用于从原始断层扫描出发，得到进一步测量或锻造前所需的干净、
轴对齐的体数据。

## 配置

`data/input/crop_config.yaml`（该流程在仓库中真实的默认配置）：

```yaml
input:
  # Options: raw | tiff
  type: "raw"
  # RAW: folder path. TIFF: file path or folder path.
  path: "data/input/ct_stack/"
  # Shared slice range for RAW/TIFF. Inclusive indices. -1 means start/end.
  slice_start: -1
  slice_end: -1

  # Required when type=raw.
  raw:
    width: 744
    height: 789
    bits: 16
    signed: false
    # little | big
    byte_order: "little"

output:
  # Output can be TIFF file (.tif/.tiff) or folder path.
  path: "data/output/cropped_ct.tiff"
  # Used only when output.path is a folder.
  folder_prefix: "crop"
  # Used only when output.path is a folder. Options: tif | tiff
  folder_extension: "tiff"

# Resampling mode during rotation: nearest | trilinear
interpolation: "trilinear"

# Optional XY border trim after rotation/crop to suppress jagged edges.
# -1: auto infer (0~2), 0: off, 1/2: manual trim pixels.
edge_trim: -1
```

`input.path` 指向 `data/input/ct_stack/`，这是仓库中签入的一个真实的小型 RAW CT 切片序列
（`IN_0001.raw`、`IN_0002.raw`、`IN_0003.raw`——三个 744x789、16 位无符号、小端序的切片，
每个约 1.12 MB）。本示例原样使用该配置，仅在命令行上覆盖 `output.path`，
使得 `data/output/` 下不会写入任何文件。

| 字段 | 含义 |
|---|---|
| `input.type` | `"raw"` 使用 `input.raw.*` 来解释字节内容，读取一个无头部的 `.raw` 切片文件夹；`"tiff"` 直接读取一个 TIFF 文件或 TIFF 切片文件夹。 |
| `input.raw.width`/`height`/`bits`/`signed`/`byte_order` | 仅在 `type: "raw"` 时必需；描述如何将每个切片的原始字节重新解释为二维像素网格。 |
| `input.slice_start`/`slice_end` | 闭区间的切片索引范围；`-1` 表示“从第一片开始”/“到最后一片结束”。 |
| `interpolation` | 在旋转体数据以使其与 PCA 主轴对齐时所使用的重采样模式：`"nearest"`（快速，但有块状效应）或 `"trilinear"`（更平滑，此处为默认值）。 |
| `edge_trim` | 旋转/裁剪后，为去除旋转体数据边缘处插值伪影而额外裁去的 XY 边界像素数。`-1` 让流程根据旋转角度自动推断出 0–2 像素。 |

完整字段参考见 `../reference/config.md`（`CropConfig` 部分），该流程所实现的 PCA 对齐与
包围盒算法见 `../algorithms/pca-volume-alignment-crop.md`。

## 运行方式

```bash
./target/release/rustmspt crop \
  --config data/input/crop_config.yaml \
  --output /tmp/rustmspt-doc-examples/cropped_ct.tiff
```

在流程运行之前，`--output` 会覆盖从已加载 YAML 中读取的 `output.path`（参见 `src/main.rs`
中的 `Commands::Crop` 分支），因此真实的 `data/input/crop_config.yaml` 与
`data/input/ct_stack/` 示例数据可以原样使用，而无需改动 `data/output/`。

## 预期输出

以上运行实际捕获的 stdout：

```
[Info] Crop input loaded: shape=(744,789,3)
[Info] Background value detected: 0
[Info] Interpolation mode: Trilinear
[Info] Foreground voxels: 768151
[Info] Rotated bbox: min=(-364.546,-175.598,-1.000) max=(364.546,175.600,1.000)
[Info] Rotated integer bbox: x=[-365,365] y=[-176,176] z=[-1,1]
[Info] Edge trim pixels (xy): 2
[Info] Cropped output shape=(727,349,3)
[Info] Crop pipeline completed. Output written: /tmp/rustmspt-doc-examples/cropped_ct.tiff
```

此次运行在本机上耗时约 60 毫秒（`real 0m0.062s`）——这个三切片的示例堆栈相比动辄数百切片的
生产环境 CT 堆栈来说非常小，因此该耗时并不能代表全尺寸运行的情况。

写出的文件是一个真实的 16 位、小端序、无压缩的多页 TIFF：

```
/tmp/rustmspt-doc-examples/cropped_ct.tiff: TIFF image data, little-endian, direntries=14,
width=727, height=349, bps=16, compression=none, PhotometricInterpretation=BlackIsZero
```

自上而下阅读日志：加载器报告输入体数据的形状为 `(744, 789, 3)`（宽度、高度、切片数，
与 `input.raw.width`/`height` 及 `ct_stack/` 中的三个文件相符），自动检测到背景像素值为
`0`，据此进行阈值处理，找到 `768151` 个前景体素，计算出经 PCA 对齐后旋转的包围盒
（旋转坐标系下为 `x=[-365,365] y=[-176,176] z=[-1,1]`），为该旋转角度推断出 `2` 像素的
边缘裁边量，最终写出一个裁剪后的 `727 x 349 x 3` 体数据——比原始的 `744 x 789` 平面尺寸更小，
因为旋转并裁边后的包围盒比原始坐标系更紧凑。

## 说明

- 由于 `edge_trim: -1`，流程针对本次旋转自动选择了 2 像素的裁边量；将 `edge_trim: 0`
  设置为可完全禁用裁边（当前景触及 RAW 切片边界、任何裁边都会裁掉真实数据时很有用），
  而 `1`/`2` 则强制使用固定的手动裁边量。
- `input.type: "tiff"` 可以接受单个多页 TIFF 文件，或包含多个单页 TIFF 切片的文件夹，
  以替代此处使用的 RAW 文件夹输入——当 CT 堆栈已经以 TIFF 而非原始二进制切片的形式导出时
  很有用。
- 背景检测与 PCA 旋转的详细说明见 `../algorithms/pca-volume-alignment-crop.md`；
  该文档还解释了 `edge_trim: -1` 的自动推断启发式方法是如何根据旋转角度得出的。
- 关于 `CropPipeline::run` 的完整行为——包括 `output.path` 的文件夹/文件模式是如何决定的，
  以及在写出逐切片 TIFF 时如何使用 `folder_prefix`/`folder_extension`——见
  `../reference/pipeline-crop-and-splitfilter.md`。

### 计时行（2026-09-25 新增）

上面的捕获输出早于共享阶段计时器。当前版本还会为每个已完成阶段打印 `[Timing] crop stage=<name> seconds=<f>`，随后打印 `[Timing] crop workers=<n>` 与 `[Timing] crop peak_rss_bytes=<n|unavailable>`。阶段名称见 `../reference/pipeline-core.md`（`pipeline/timing.rs`）；输出文件不变。
