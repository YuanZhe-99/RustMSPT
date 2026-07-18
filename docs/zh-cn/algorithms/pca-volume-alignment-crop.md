# 基于 PCA 的体数据对齐与裁剪

裁剪流水线（`src/pipeline/crop.rs`）接收一个原始 CT（计算机断层扫描）重建结果——一个包含被扫描样品及其周围安装/背景材料的三维体素网格——并生成一个只包含样品的、紧密裁剪的轴对齐体数据。本文档解释该转换背后的算法：为什么简单的轴对齐裁剪行不通，以及流水线如何在前景体素点云上使用主成分分析（PCA）在裁剪之前找到样品的真实朝向。

## 问题所在：样品从不与扫描轴对齐

CT 扫描仪生成的体数据的 `(x, y, z)` 轴由扫描仪几何结构决定，而非样品本身。实际中，样品在其安装座中总会存在某种任意倾斜——有时只是几度的偏差，有时则是一根细长的棒状物斜跨整个视野。如果用轴对齐包围盒来裁剪这样的体数据，会出现两种情况之一：要么包围盒被放大以容纳倾斜的样品（这会浪费大量输出体数据在下游分析必须跳过的背景体素上），要么紧密的包围盒会直接切掉样品的边角。

解决方法是先估计样品的*自然*朝向——即其质量分布最集中和最分散的轴向——然后旋转体数据，使这些轴成为坐标轴。一旦样品在此意义上实现了轴对齐，轴对齐包围盒同时也是*最紧密*的包围盒，据此裁剪几乎不会浪费空间。`CropPipeline` 正是实现了这一流程：检测背景 → 通过 PCA 估计朝向 → 在裁剪的同时旋转到新坐标系 → 修剪新边界处遗留的重建伪影 → 保存。

## 步骤 1：背景检测（`detect_background_mode`）

在进行任何其他操作之前，流水线需要知道哪个体素值代表"非样品"。它不会假定某个特定值（例如 0），因为实际的背景/安装材料强度取决于具体的扫描及其数值编码方式。相反，它利用了一个结构性假设：**体数据的外边界壳层（六个面，厚度为一个体素）绝大部分是背景**，因为样品在安装时会远离扫描体数据的边缘，以避免正是本流水线要解决的裁切问题。`detect_background_mode` 只对该边界壳层中出现的体素值建立直方图统计，并返回众数（出现频率最高的值）。这个值就成为后续每一步都会用到的参考 `background`（背景）值——前景则简单地定义为"任何取值不同于 `background` 的体素"。

参见参考条目：[`detect_background_mode`](../reference/pipeline-crop-and-splitfilter.md#detect_background_mode)。

## 步骤 2：PCA 朝向估计（`estimate_pca_bbox`）

这是整个算法的核心。一旦背景已知，每个非背景体素的 `(x, y, z)` 位置都被视为一个三维数据点，并对该点云应用 PCA 以找出其主要的空间方差轴。

### 直观理解

想象前景体素构成一团形状与样品相仿的点云——比如一个细长的、略微倾斜的团块。PCA 要回答的问题是："如果可以自由旋转这团点云，什么朝向能让它看起来最自然地与坐标轴对齐？"它的做法是找出三个相互垂直的方向，使点云沿这些方向的散布（方差）分别为最大、次大和最小。将点云旋转到这些方向与 x/y/z 轴重合，恰好就是能使点云周围的轴对齐包围盒最小化的那个旋转——因为任何轴对齐包围盒都必须至少与数据沿每个轴的真实散布同样宽，而 PCA 找到的正是数据的散布被分解为相互独立、无冗余方向的那组轴。

### 具体机制

1. **质心。** 将所有前景体素的 `(x, y, z)` 位置求和后除以体素数，得到平均位置——即样品在体素索引空间中的"质心"。
2. **协方差矩阵。** 对每个前景体素，计算其相对质心的偏移量，并将该偏移量与自身的外积累加到一个运行中的 3×3 求和矩阵里，最后除以体素数。所得结果即为前景点云的协方差矩阵：其对角线元素是沿 x、y、z 的方差，非对角线元素则刻画了这些轴之间的协变关系（即点云相对于原始扫描轴的倾斜程度）。
3. **特征分解。** 由于协方差矩阵天然是对称的，代码使用 `nalgebra::SymmetricEigen` 直接计算其特征向量和特征值（对于对称矩阵，这比通用特征求解器更快，数值上也更稳健）。每个特征向量代表三维空间中的一个方向，其对应的特征值则是点云沿该方向的方差。
4. **按特征值降序排序。** 三对特征向量/特征值被重新排序，使第一个特征向量指向散布最大的方向（样品的"长轴"），第二个指向次大方向，第三个指向最小方向。
5. **强制右手坐标系。** 三个排序后的特征向量作为列组装成一个 3×3 旋转矩阵。由于特征向量的符号并不唯一确定，所得矩阵可能是一个反射（行列式为 −1）而非真正的旋转。代码会检查 `rot.determinant() < 0.0`，如果成立，则翻转第三列的符号——这样便能在不扰动前两个（已经是最大/次大方向的）轴的情况下，保证得到一个右手坐标系。
6. **计算旋转后的包围盒。** 利用该旋转矩阵的*逆*（即其转置，因为旋转矩阵是正交归一的），将每个前景体素相对质心的位置变换到新的、与 PCA 对齐的坐标系中。这些变换后坐标在三个轴上分别的最小/最大值，即给出样品在新坐标系下的紧密包围盒。

该函数返回旋转矩阵、质心、旋转坐标系下的最小/最大边界，以及前景体素数量——这些正是下一步实际重采样体数据所需的全部信息。

参见参考条目：[`estimate_pca_bbox`](../reference/pipeline-crop-and-splitfilter.md#estimate_pca_bbox)。

## 步骤 3：旋转与裁剪（`rotate_and_crop` / `rotate_and_crop_gpu`）

在已知旋转矩阵和包围盒的情况下，流水线会构建一个全新的、轴对齐的输出体数据，其尺寸恰好等于旋转后的包围盒（外加一个小的浮点数稳定化步骤——`stabilize_bound`/`float_bounds_to_inclusive_i64`——将接近整数的边界值吸附为精确整数，避免输出包围盒因浮点噪声而多长出一个杂散体素）。

对于这个*新*输出网格中的每个体素，算法采用反向思路：将正向旋转（`rot`，而非其逆）应用于输出体素相对质心的局部坐标，从而找出该点在*原始*、未旋转的源体数据中对应的位置，然后在该位置对源体数据进行采样。这是图像/体数据重采样中标准的逆映射方法——它能保证每个输出体素都获得一个取值（不会出现空洞），不像正向映射方法那样，可能因为源体素没有恰好落在输出网格点上而留下空隙。

有两种插值模式控制如何对非整数的源坐标进行采样（配置字段 `interpolation`，取值为 `"nearest"` 或 `"trilinear"`；**默认值为 `trilinear`**——参见 `parse_interpolation_mode`）：

- **最近邻**（`sample_nearest`）将源坐标四舍五入到最近的整数体素，并直接复制其值。计算开销小，能精确保留原始体素值，但旋转后边缘会呈现更粗糙的阶梯状。
- **三线性插值**（`sample_trilinear`）读取周围八个整数体素，并按各轴方向上的距离比例进行加权混合，从而产生更平滑、更精确的输出，代价是每个体素的采样工作量约增加 8 倍。

任何落在源体数据边界之外的采样坐标都会被当作 `background`（`sample_voxel_or_background`）处理，因此新的轴对齐包围盒中，凡是倾斜的原始体数据未能覆盖到的地方，都会用背景值填充。

**CPU 路径（`rotate_and_crop`）。** 输出体数据沿 z 方向逐切片填充，各切片通过 `rayon` 的 `par_chunks_mut` 并行处理（每个 z 切片，或按 `out_w * out_h` 大小划分的 z 块，对应一个并行任务）。在单个切片内部，x/y 循环按顺序执行。

**GPU 路径（`rotate_and_crop_gpu`，特性门控）。**

> **特性门控：** 仅当 crate 以 `gpu` Cargo 特性构建时才会被编译并可达。若未启用该特性，`CropPipeline::run` 始终走上述 CPU 路径。

当 `gpu` 特性启用时，`CropPipeline::run` 会根据*输出*体素数量在 CPU 和 GPU 路径之间做出选择：它根据旋转后的包围盒计算 `out_total = out_w * out_h * out_d`，仅当 `out_total > 100_000` 体素时才调度到 GPU。对于较小的输出，它会直接使用 CPU 路径（在这种规模下 GPU 调度的开销并不划算）。GPU 路径会将 `i64` 体数据转换为 `i32` 以便上传，初始化/复用一个 `GpuVolumeTransformPipeline`（参见 [gpu.md](../reference/gpu.md)），调度一个执行上述相同逆旋转采样逻辑的 WGSL 计算着色器（最近邻或三线性插值，由传给着色器的 `0`/`1` 插值标志选定），并在下载结果时将其转换回 `i64`。如果 GPU 初始化或调度因任何原因失败，`CropPipeline::run` 会捕获该错误、记录警告，并**回退到 CPU 路径**，而不是中止整个流水线。

参见参考条目：
[`rotate_and_crop`](../reference/pipeline-crop-and-splitfilter.md#rotate_and_crop)、
[`rotate_and_crop_gpu`](../reference/pipeline-crop-and-splitfilter.md#rotate_and_crop_gpu)。

## 步骤 4：边缘伪影裁剪（`infer_trim_pixels` / `resolve_trim_pixels` / `trim_volume_border`）

即使在完成背景检测和 PCA 裁剪之后，CT 重建算法仍可能在体数据外边界处遗留零星的非背景伪影体素（重建噪声、截断伪影等）。若不加处理，这些边界伪影本身在下游步骤以及后续 PCA 处理看来会像是"前景"，从而微妙地使测量结果产生偏差。流水线通过一个可选的裁剪后修剪步骤来解决这一问题，即修剪 x/y 方向最外层的 0、1 或 2 个体素（仅限 XY 边界面，z/深度方向保持不变）。

- **`boundary_non_bg_ratio(volume, background, thickness)`** 用于测量污染程度：它扫描体数据各个面上厚度为 `thickness` 个体素的壳层，并返回该壳层中*非*背景值所占的比例。比例越高，说明边界处堆积的伪影体素越多。
- **`infer_trim_pixels`** 对两种壳层厚度（1 个体素：`r1`；2 个体素：`r2`）应用一个简单的启发式规则：若 `r1 > 0.08 && r2 > 0.04`，修剪 2 个体素；否则若 `r1 > 0.03`，修剪 1 个体素；否则不修剪。这种双厚度检查可以避免对污染仅局限于单薄壳层的体数据进行过度修剪。
- **`resolve_trim_pixels`** 将配置字段 `edge_trim` 转换为实际的修剪数量：`-1` 表示"自动"（调用 `infer_trim_pixels`），`0`/`1`/`2` 为用户显式强制指定的值，其他任何值都会被视为配置错误。所得结果会被限制在
  `min(requested, floor((width-1)/2), floor((height-1)/2), 2)` 范围内，以确保修剪请求既不会耗尽整个体数据，也不会超过 2 个体素的硬性上限。
- **`trim_volume_border`** 执行实际的裁剪：它从 XY 平面的四条边（所有 z 切片）各削去 `trim` 个体素，生成一个深度不变但更小的体数据。

参见参考条目：
[`boundary_non_bg_ratio`](../reference/pipeline-crop-and-splitfilter.md#boundary_non_bg_ratio)、
[`infer_trim_pixels`](../reference/pipeline-crop-and-splitfilter.md#infer_trim_pixels)、
[`resolve_trim_pixels`](../reference/pipeline-crop-and-splitfilter.md#resolve_trim_pixels)、
[`trim_volume_border`](../reference/pipeline-crop-and-splitfilter.md#trim_volume_border)。

## 整体编排（`CropPipeline::run`）

`CropPipeline::run` 按顺序将上述各步骤串联起来：

1. **加载**输入体数据，可以来自一个 RAW 切片文件夹（`input.type = "raw"`，使用 `input.raw` 指定宽度/高度/位深/是否有符号/字节序），也可以来自 TIFF 文件/文件夹（`input.type = "tiff"`/`"tif"`），并可选地限定切片范围（`input.slice_start`/`input.slice_end`）。
2. **检测背景**，通过 `detect_background_mode`。
3. **估计朝向**，通过 `estimate_pca_bbox`，获得旋转矩阵、质心以及旋转后的包围盒；同时记录前景体素数量，以及浮点和稳定化后的整数形式的旋转包围盒，用于诊断。
4. **旋转与裁剪**：在启用 GPU 的构建中，当旋转后的输出体素数超过 100,000 时调度到 GPU，若 GPU 失败则自动回退到 CPU；否则始终使用 CPU 路径。
5. **解析并应用边缘修剪**，通过 `resolve_trim_pixels`（遵循 `config.edge_trim`）和 `trim_volume_border`。
6. **保存**最终裁剪、修剪后的轴对齐体数据为 TIFF（单个文件或切片文件夹，取决于 `output.path`/`output.folder_prefix`/`output.folder_extension`）。

每个阶段都会打印一行 `[Info]` 诊断信息（形状、背景值、插值模式、前景计数、包围盒、修剪体素数、最终形状），这有助于验证 PCA 是否找到了合理的朝向，以及修剪启发式是否出现了过度或不足修正。

上文提及的配置字段定义在 `src/config/crop.rs` 中：`CropConfig`（`input`、`output`、`interpolation`、`edge_trim`）、`CropInput`（`type`、`path`、`slice_start`、`slice_end`、`raw`），以及 `CropOutput`（`path`、`folder_prefix`、`folder_extension`）。

参见参考条目：[`CropPipeline::run`](../reference/pipeline-crop-and-splitfilter.md#croppipelinerun)。

## Cross-references

- [pipeline-crop-and-splitfilter.md](../reference/pipeline-crop-and-splitfilter.md) —
  `pipeline/crop.rs` 的完整逐函数参考，包括
  [`estimate_pca_bbox`](../reference/pipeline-crop-and-splitfilter.md#estimate_pca_bbox)、
  [`rotate_and_crop`](../reference/pipeline-crop-and-splitfilter.md#rotate_and_crop) 和
  [`CropPipeline::run`](../reference/pipeline-crop-and-splitfilter.md#croppipelinerun)。
- [gpu.md](../reference/gpu.md) — `GpuVolumeTransformPipeline`（`src/gpu/volume_transform.rs`）的参考文档，
  即 `rotate_and_crop_gpu` 所调度的 WGSL 计算流水线。
