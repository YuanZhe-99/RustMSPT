# RustMSPT 文档（简体中文）

RustMSPT（Rust Microstructure Processing Toolbox，Rust 微结构处理工具箱）是一个用于 STL/CT 体数据微结构处理的 Rust 工具包，提供八条流水线——`split-filter`、`pack`、`optimize`、`measure`、`forge`、`scale`、`crop`、`render`——构建在共享的几何、I/O 以及（可选 GPU 加速的）计算内核模块之上。

本目录在函数级别记录了代码库，从概念上解释了其核心算法，并结合真实的、实际捕获的输出逐一介绍每条流水线。它与 `src/` 中几乎每个函数上已经存在的 `// AI-FUNC-SUMMARY` 注释相辅相成，并基于同一份源材料编写——该约定及其定义的“函数阅读策略”（Function Reading Policy）参见 [AGENTS.md](../../AGENTS.md)。

**快照范围：** 本文档编写时对应的工作树截至 2026-07-18，当时包含尚未提交的堆积目标粒径分布功能（`src/geometry/metrics.rs`、`src/pipeline/pack_targets.rs`、`data/input/gu2019_fig7b_pore_distribution.csv`）。若这些文件后续有修改，请按照 [AGENTS.md](../../AGENTS.md#code-conventions) 中的要求同步更新对应文档。

## 从这里开始

- **[函数索引](reference/function-index.md)** —— `src/` 中每个已记录的函数、方法、结构体、枚举和常量，附带源码位置和一句话概述。这是查找某项功能实现位置最快的方式。

## 参考文档（按模块划分的函数/结构体文档）

镜像 `src/` 的模块结构。每份文档开头是一个 `## Index` 表格（用于编译上方的函数索引），随后是每个函数的完整条目——签名、用途、参数、返回值、副作用、注意事项——以及相关的结构体/枚举字段表。

| 文档 | 覆盖范围 |
|---|---|
| [core-and-compute.md](reference/core-and-compute.md) | `main.rs`、`lib.rs`、`error.rs`、`types.rs`、`bin/precision_test.rs`、`compute/*` |
| [config.md](reference/config.md) | `config/*` —— YAML 配置结构体与反序列化辅助函数 |
| [geometry-core.md](reference/geometry-core.md) | `geometry/{mod,bbox,mesh_ops,spatial,render}.rs` |
| [geometry-volume-collision.md](reference/geometry-volume-collision.md) | `geometry/{volume,collision,forging}.rs` |
| [geometry-analysis.md](reference/geometry-analysis.md) | `geometry/{metrics,s2}.rs` |
| [gpu.md](reference/gpu.md) | `gpu/*`（需要 `cargo build --features gpu`） |
| [io.md](reference/io.md) | `io/{image,stl,volume}.rs` |
| [pipeline-core.md](reference/pipeline-core.md) | `pipeline/{mod,rotation,scale,forge,measure,render}.rs` |
| [pipeline-crop-and-splitfilter.md](reference/pipeline-crop-and-splitfilter.md) | `pipeline/{crop,split_filter}.rs` |
| [pipeline-placement.md](reference/pipeline-placement.md) | `pipeline/placement*.rs`、`geometry/void_index.rs` —— 带种子、可复原、感知孔隙的引擎 |
| [pipeline-packing.md](reference/pipeline-packing.md) | `pipeline/{pack,pack_targets}.rs` |
| [mesh-render-and-vtu.md](reference/mesh-render-and-vtu.md) | `io/vtu.rs`、`meshgen/render_scene.rs`、`geometry/scene_render.rs`、`pipeline/mesh_render.rs`、`config/mesh_render.rs` |
| [mesh-verify.md](reference/mesh-verify.md) | `meshgen/{predicates,verify}.rs`、`pipeline/mesh_verify.rs`、`config/mesh_verify.rs` |
| [meshgen.md](reference/meshgen.md) | `config/meshgen.rs`、`pipeline/meshgen.rs`、`meshgen/{surface,features,predicates,arrange,topo,gapfield,sizing,lattice,snapshot}.rs` - S0/S1/G2/G3/G4 |
| [pipeline-optimize.md](reference/pipeline-optimize.md) | `pipeline/optimize.rs` |

## 算法（概念性说明）

对参考文档背后那些不太直观的算法进行更高层次的解释，并与实现它们的具体函数交叉链接。

| 文档 | 说明内容 |
|---|---|
| [s2-two-point-correlation.md](algorithms/s2-two-point-correlation.md) | S2 统计量、CPU 精确方法（FFT/直接法）与蒙特卡洛方法、GPU 加速 |
| [simulated-annealing-island-model.md](algorithms/simulated-annealing-island-model.md) | `optimize` 流水线的模拟退火核心、自适应温度以及多岛屿迁移 |
| [ffd-forging.md](algorithms/ffd-forging.md) | `forge` 流水线的自由变形压缩/鼓凸模型 |
| [packing-target-diameter-distribution.md](algorithms/packing-target-diameter-distribution.md) | `pack` 流水线的目标粒径直方图分箱分配与球形度导向控制（最新功能） |
| [pca-volume-alignment-crop.md](algorithms/pca-volume-alignment-crop.md) | `crop` 流水线基于 PCA 的取向估计与旋转/裁剪 |
| [spatial-grid-collision.md](algorithms/spatial-grid-collision.md) | 邻域查询加速以及精确/周期性碰撞检测 |
| [void-aware-placement.md](algorithms/void-aware-placement.md) | 确定性、尺寸集合、孔隙判据及其完备性、停止原因的优先级 |
| [mesh-clipping-volume-fraction.md](algorithms/mesh-clipping-volume-fraction.md) | Sutherland-Hodgman 网格裁剪与体积分数统计 |
| [stl-rendering.md](algorithms/stl-rendering.md) | 共享相机取景、CPU 光线投射与 GPU 离屏光栅化 |

## 示例（各流水线走查）

每份走查文档展示了一个真实的配置、确切的 CLI 命令，以及从实际运行中捕获的输出。

| 文档 | 流水线 |
|---|---|
| [forge.md](examples/forge.md) | `forge` |
| [measure.md](examples/measure.md) | `measure` |
| [scale.md](examples/scale.md) | `scale` |
| [crop.md](examples/crop.md) | `crop` |
| [split-filter.md](examples/split-filter.md) | `split-filter` |
| [optimize.md](examples/optimize.md) | `optimize` |
| [pack.md](examples/pack.md) | `pack`（普通体积分数堆积） |
| [pack-placement.md](examples/pack-placement.md) | 带 `placement:` 块的 `pack` —— 带种子、可复原的放置 |
| [pack-void.md](examples/pack-void.md) | 围绕冻结孔隙的 `pack` |
| [version.md](examples/version.md) | `version` —— 这个二进制是什么 |
| [pack-target-distribution.md](examples/pack-target-distribution.md) | `pack`（带目标粒径分布导向） |
| [render.md](examples/render.md) | `render` |
| [mesh-render.md](examples/mesh-render.md) | `mesh-render`（VTU 体网格渲染器） |
| [mesh.md](examples/mesh.md) | `mesh`（四面体网格生成，S0/S1/G2-1..G2-3） |

## 全文使用的约定

- 函数条目标题使用不加反引号的裸函数/方法名（`#### mesh_metrics`、`#### TargetDistribution::choose_bin`），以便 markdown 锚点可预测，便于交叉链接。
- `> **Feature-gated:**` 提示框标注仅在使用 `cargo build --features gpu` 构建时才存在的代码。
- `> **Doc note:**` 提示框标记了少数几处源码中的 `AI-FUNC-SUMMARY` 注释与实际代码相比已经过时的地方——文档正文反映的是经过核实的行为，而非过时的注释。
- `> **Algorithm:**` 和 `**See also:**` 这两行用于将参考条目与相关的概念性算法文档相互交叉链接。

## 中文翻译

本文档现已提供简体中文版本。术语表和翻译约定参见 [../TRANSLATION_GUIDE.md](../TRANSLATION_GUIDE.md)，其规范了 `docs/zh-cn/` 镜像文档的翻译方式。
