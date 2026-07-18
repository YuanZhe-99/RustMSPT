# FFD 锻造

## 本模拟内容

RustMSPT 中的锻造是对轴向压缩锻造的**几何近似**：代表微结构样本（一个颗粒、一个孔隙/空洞，或更大结构中的任意区域）的网格沿一个轴被挤压，并允许在侧向发生鼓胀，以模仿工件在压力机下发生的形状变化。这一点可以直接从 `src/geometry/forging.rs` 中验证：整个变换是围绕所选中心点的逐顶点仿射缩放（`v' = center + scale * (v - center)`），对于“空洞”网格还会附加一个较小的径向缩放步骤。这里没有材料模型、没有塑性、没有应力/应变场、没有接触力学，也没有有限元分析——它是一个闭式的运动学重塑，而不是物理仿真。它回答的是“如果周围材料被压缩这么多，这个包围区域的形状及其中被跟踪的子区域会如何变化”，而不是“会产生什么力或应力”。

有两个函数实现了这一点，泛化程度依次提升：`simulate_forging_ffd`（一个仅限 Z 轴的简化版本）和 `simulate_forging_ffd_with_tracking`（`ForgePipeline` 实际使用的通用版本）。完整的逐参数签名见参考文档——参见[交叉引用](#cross-references)。

## 简化版本：`simulate_forging_ffd`

`simulate_forging_ffd(mesh, compression_ratio, bulge_factor)` 仅沿 **Z 轴**压缩网格，围绕网格**自身**的轴对齐包围盒（bbox）中心进行：

1. 计算 `center` 为 `mesh_bbox(mesh)` 的中点（如果网格没有几何体，则回退到单位立方体）。
2. `axis_scale = clamp(1 - compression_ratio, 0.01, 1.0)`——这是压缩后剩余的原始 Z 方向延伸比例，也就是说 `compression_ratio` 在概念上是 `1 - (final_height / initial_height)`。`compression_ratio` 为 `0.2` 意味着压缩后的高度是原始高度的 80%；该值被限幅，使网格永远不会被压成零厚度（`axis_scale` 的下限为 `0.01`，即 99% 的高度缩减）。
3. `lateral_scale = (1 / sqrt(axis_scale)) ^ clamp(bulge_factor, 0, 1)`——即 X/Y 方向的缩放系数。当 `bulge_factor = 0` 时，`lateral_scale = 1`（无鼓胀：网格只是变短，横截面不变——净体积会缩小）。当 `bulge_factor = 1` 时，`lateral_scale = 1 / sqrt(axis_scale)`，这正是在不可压缩、径向对称鼓胀（平面应变意义下）下精确保持体积所需的缩放（为保持体积，面积必须增长 `1/axis_scale` 倍，而面积随线性缩放系数的平方增长，因此线性系数为 `1/sqrt(axis_scale)`）。`bulge_factor` 的中间值在“无鼓胀”与“体积守恒鼓胀”之间进行（对数空间的）插值，让调用者可以调节压缩时伴随发生的侧向材料位移量。
4. 每个顶点都被重新映射：`x' = center.x + (x - center.x) * lateral_scale`，`y` 同理，`z' = center.z + (z - center.z) * axis_scale`。

这是整网格的情形：变形中心由被变形的网格自身推导得出，因此不存在“该网格是更大结构的一个子区域”这一概念。

## 通用版本：`simulate_forging_ffd_with_tracking`

`simulate_forging_ffd_with_tracking` 是 `ForgePipeline` 实际调用的函数。它保留了相同的“围绕中心缩放”运动学，但在四个方面进行了泛化：

**1. 可配置的压缩轴。** `compression_axis` 取值为 `0`/`1`/`2`，分别对应 X/Y/Z。被选中的轴使用 `axis_scale`，另外两个轴都使用 `lateral_scale`。`axis_scale` 和 `lateral_scale` 的计算公式与简化版本完全相同。

**2. 变形中心来自调用者提供的 `lattice_bbox`，而非网格自身的 bbox。** 逐顶点变换中使用的中心是 `lattice_bbox`（由调用者传入的一个 `BoundingBox`）的中点——它不必等于 `mesh_bbox(mesh)`。当被变形的网格是从更大参考系中裁剪出来或在其中被跟踪的子区域时，这一点很重要（例如，一个被裁剪出的颗粒/空洞网格，应当像仍然嵌入在完整晶格中一样进行压缩，围绕晶格中心而不是自身的局部中心）。使用晶格中心而非局部网格中心，可以使子区域的变形与周围结构在同一压制行程下的运动保持一致。

**3. 孔隙致密化。** 当 `mesh_type`（不区分大小写）为 `"void"` 时，在主 FFD 变换之后会额外运行一个步骤：计算一个 `closure` 系数，为 `clamp(1 - 0.05 * compression_ratio * void_densification, 0.85, 1.0)`，并将每个顶点按该系数向网格自身的质心（`mesh_centroid`，在已压缩的网格上计算）方向收拢。物理上，这模拟的是**压缩下的空洞/孔隙闭合**：在实际锻造工件中，随着压力施加，孔隙相对于块体固体材料往往会不成比例地塌缩，因此空洞网格的缩小程度会比单纯的侧向鼓胀变换所产生的略大。`void_densification` 是一个可调乘数，用于控制这种额外闭合的激进程度（在流水线配置中默认值为 `1.0`）；`closure` 被限定在 `[0.85, 1.0]` 范围内，因此它总是收缩（或保持不变），而不会反转或使空洞几何过度塌缩。

**4. 可选的 ROI 包围盒跟踪。** 如果提供了 `track_bbox: Option<BoundingBox>`，其 8 个角点会各自通过与网格顶点相同的 `transform_point` 闭包（仅经过主压缩阶段——孔隙致密化步骤作用于网格顶点/质心，不适用于被跟踪的包围盒），随后重新计算变换后角点的轴对齐包围盒，作为新的被跟踪 ROI。这使调用者能够定义一个任意的感兴趣区域（不必与网格自身的范围重合），并了解在与网格相同的变形之后，该区域最终落在何处——包括其新的位置、大小和形状近似——而无需从变形后的网格几何体重新推导。

## `ForgePipeline::run` 如何使用它

`ForgePipeline::run`（`src/pipeline/forge.rs:58`）将上述内容串联成一个端到端的流水线：

1. 将输入 STL（或合并一个文件夹中的多个 STL）加载为 `mesh`。
2. 计算 `lattice_bbox = mesh_bbox(&mesh)`（整个已加载网格的 bbox，既作为默认的变形中心，也作为默认的体积分数/区域回退值）。
3. 通过 `parse_roi_bbox` 从 `config.forging.roi_bounding_box`（一个扁平的 6 元素 `[min_x,min_y,min_z,max_x,max_y,max_z]` 列表）解析出可选的 ROI 包围盒。
4. 计算**锻造前 ROI 内的体积分数**（`before_roi_vf`）：如果存在 ROI，则通过 `volume_fraction_in_bbox` 在 ROI 上计算，否则在 `lattice_bbox` 上计算。
5. 解析带默认值的参数：`compression_ratio`（0.2）、`compression_axis`（从字符串 `"x"/"y"/"z"` 解析，默认为 `"z"`，通过 `parse_compression_axis`）、`orient_to_positive_volume`（false）、`bulge_factor`（0.5）、`mesh_type`（`"particle"`）、`void_densification`（1.0）。
6. 调用 `simulate_forging_ffd_with_tracking(&mesh, lattice_bbox, roi_bbox, compression, compression_axis, bulge, mesh_type, void_densification)`，得到压缩后的网格以及变换后的 ROI 包围盒。
7. 计算**锻造后（变换后）ROI 内的体积分数**（`after_roi_vf`），比较感兴趣区域内压缩前后的堆积密度——这正是该流水线存在的目的所要产出的定量信号。
8. 如果启用了 `orient_to_positive_volume`，则对压缩后的网格调用 `orient_components_to_positive_volume`，翻转任何有符号体积变为负值的网格分量（例如由于侧向鼓胀使某个分量的绕向发生反转），并记录在总分量数中有多少个被翻转。
9. **将输出平移对齐回原始 ROI 位置：** 如果原始 ROI（`roi_bbox`）和被跟踪/变换后的 ROI（`tracked_roi`）都可用，则计算 `output_shift = roi_in.min - roi_out.min`，并将整个输出网格平移该偏移量（若各轴上的偏移可忽略不计则跳过）。这使锻造输出的 ROI 落回调用者指定 ROI 的相同绝对位置，而不会因为晶格相对压缩而产生漂移。
10. 将锻造后的网格保存为 STL（`save_stl`），并写入一个文本报告（`<output>.txt`），其中包含：变形前后的（整体网格）bbox、变形前后的 ROI bbox（“变形后”的值已应用输出平移）、变形前后 ROI 空间体积分数、压缩轴标签、朝向修正统计信息（如果启用）以及输出平移向量。相同信息也会以 `[Info]` 行的形式输出到标准输出。

## 配置字段

来自 `src/config/forging.rs` 中的 `ForgingParams`，位于顶层 `forging:` 键下：

| 字段 | 类型 | 默认值（省略时） | 含义 |
|---|---|---|---|
| `input_stl_path` | `String` | —（必填） | 输入 STL 文件的路径，或待合并的文件夹路径。 |
| `output_stl_path` | `Option<String>` | `data/output/forged_mesh.stl` | 锻造后输出 STL 的路径；报告将以 `.txt` 扩展名写在同一目录下。 |
| `compression_ratio` | `Option<f64>` | `0.2` | 压缩轴方向被移除的延伸比例（见上文公式）。 |
| `compression_axis` | `Option<String>` | `"z"` | `"x"`、`"y"`、`"z"` 之一（不区分大小写）；选择哪个轴被压缩，另外两个轴则获得侧向鼓胀。 |
| `orient_to_positive_volume` | `Option<bool>` | `false` | 是否在变形后运行 `orient_components_to_positive_volume`。 |
| `bulge_factor` | `Option<f64>` | `0.5` | 在无鼓胀（`0`）与体积守恒（`1`）之间插值侧向鼓胀程度。 |
| `roi_bounding_box` | `Option<Vec<f64>>` | `None`（不跟踪 ROI；回退到整体网格 bbox） | 扁平的 6 元素 `[min_x,min_y,min_z,max_x,max_y,max_z]` 感兴趣区域，用于在变形过程中跟踪。 |
| `mesh_type` | `Option<String>` | `"particle"` | 设为 `"void"`（不区分大小写）以启用额外的质心闭合致密化步骤。 |
| `void_densification` | `Option<f64>` | `1.0` | 控制空洞网格在压缩下致密化激进程度的乘数。 |

## Cross-references

- [geometry-volume-collision.md](../reference/geometry-volume-collision.md) — `simulate_forging_ffd` 与 `simulate_forging_ffd_with_tracking` 的完整函数级参考（签名、参数表、源码位置）。
- [pipeline-core.md#ForgePipeline::run](../reference/pipeline-core.md#forgepipelinerun) — 驱动本算法端到端运行的流水线的完整参考条目。
