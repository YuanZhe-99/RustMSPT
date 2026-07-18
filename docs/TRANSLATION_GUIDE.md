# English → Chinese Translation Guide

This guide scopes the `docs/zh-cn/` translation. It is a process document, not itself part of the English or Chinese content — it lives here at the top of `docs/`, beside `en-us/` and `zh-cn/`, rather than inside either.

**Status: `docs/zh-cn/` mirrors `docs/en-us/`, including the render algorithm and walkthrough.** Detailed render additions inside existing reference pages are explicitly flagged pending translation and link to the English contracts.

## Required mirrored file list

`docs/zh-cn/` must reproduce the exact same relative paths as `docs/en-us/`, so cross-links and tooling can assume a 1:1 structural mirror:

```
docs/zh-cn/
  README.md
  reference/
    function-index.md
    core-and-compute.md
    config.md
    geometry-core.md
    geometry-volume-collision.md
    geometry-analysis.md
    gpu.md
    io.md
    pipeline-core.md
    pipeline-crop-and-splitfilter.md
    pipeline-packing.md
    pipeline-optimize.md
  algorithms/
    s2-two-point-correlation.md
    simulated-annealing-island-model.md
    ffd-forging.md
    packing-target-diameter-distribution.md
    pca-volume-alignment-crop.md
    spatial-grid-collision.md
    mesh-clipping-volume-fraction.md
    stl-rendering.md
  examples/
    forge.md
    measure.md
    scale.md
    crop.md
    split-filter.md
    optimize.md
    pack.md
    pack-target-distribution.md
    render.md
```

29 files total (1 README + 11 reference files + 8 algorithm files + 9 example files).

## What to translate vs. what to keep in English

**Translate:** prose — explanations, purpose statements, notes, callout text, algorithm descriptions, example walkthrough narration.

**Never translate:**
- Code identifiers: function/method/struct/enum/field names (`mesh_metrics`, `TargetDistribution::choose_bin`, `equivalent_diameter`)
- Signatures and type names (`pub fn mesh_metrics(mesh: &Mesh) -> Option<MeshMetrics>`)
- CLI flags and subcommand names (`--config`, `--input`, `--output`, `pack`, `optimize`, `split-filter`)
- YAML config keys (`target_diameter_distribution_csv`, `target_volume_fraction`, `packing.mode`)
- File paths (`src/pipeline/pack_targets.rs:98`, `data/input/gu2019_fig7b_pore_distribution.csv`)
- Source-code comments quoted in docs (e.g. quoted `AI-FUNC-SUMMARY` text) — quote verbatim, translate only the surrounding doc prose
- Markdown structure: heading text used as anchors (`#### mesh_metrics` stays `#### mesh_metrics`, not a translated heading) — this is required so cross-links between `en-us/` and `zh-cn/` (and within `zh-cn/` itself) keep working. If a translated heading is desired for readability, put it as body text immediately under the untranslated anchor heading, not as the heading itself.

## Terminology glossary

Baseline proposed terms, extracted from the English corpus. Flag ambiguous or contested terms during translation review rather than silently picking one.

| English | Proposed Chinese | Notes |
|---|---|---|
| microstructure | 微结构 | |
| particle packing | 颗粒堆积 / 颗粒填充 | "堆积" for the physical process, "填充" acceptable for the algorithmic act of placing particles |
| volume fraction (VF) | 体积分数 | keep "VF" abbreviation in code/config contexts |
| equivalent-volume diameter | 等体积直径 | |
| sphericity | 球形度 | |
| two-point correlation function (S2) | 两点相关函数 (S2) | keep "S2" as-is, it's used as a variable/field name throughout the code |
| simulated annealing (SA) | 模拟退火 | keep "SA" abbreviation where used as shorthand |
| island model | 岛屿模型 | in the SA/migration sense, not biological |
| Metropolis acceptance criterion | Metropolis 接受准则 | keep "Metropolis" untranslated (proper noun) |
| adaptive temperature | 自适应温度 | |
| free-form deformation (FFD) | 自由变形 (FFD) | keep "FFD" abbreviation |
| void densification | 孔隙致密化 | |
| region of interest (ROI) | 感兴趣区域 (ROI) | keep "ROI" abbreviation |
| voxel | 体素 | |
| bounding box (bbox) | 包围盒 | keep "bbox" in code-adjacent contexts |
| mesh | 网格 | in the triangle-mesh sense |
| watertight / manifold mesh | 封闭网格 / 流形网格 | context-dependent; "封闭" (closed/watertight) is usually the more natural fit here since the code's own term is `mesh_is_closed` |
| triangulation | 三角剖分 | |
| collision detection | 碰撞检测 | |
| periodic boundary (condition) | 周期边界（条件） | |
| spatial grid | 空间网格 | distinguish from "mesh" (网格 is used for both — consider "空间哈希网格" for `SpatialGrid` if disambiguation is needed against mesh contexts in the same passage |
| broad-phase / narrow-phase | 粗筛阶段 / 精确阶段 | collision-detection two-tier terminology |
| packing target distribution | 目标粒径分布 | "diameter distribution" specifically, since the feature is diameter-histogram-based |
| bin (histogram bin) | 区间 / 分箱 | "区间" when referring to the `[left,right)` interval concept, "分箱" when referring to the histogram-bucketing action |
| bin debt | 区间欠账 | a coined term for `choose_bin`'s allocation heuristic — flag as non-standard, may need a translator's note on first use |
| total variation distance (TVD) | 全变差距离 | keep "TVD" abbreviation |
| PCA (principal component analysis) | 主成分分析 (PCA) | keep "PCA" abbreviation |
| CT (computed tomography) volume | CT（计算机断层扫描）体数据 | keep "CT" abbreviation |
| voxelization | 体素化 | |
| ray casting | 光线投射 | |
| Monte Carlo (method) | 蒙特卡洛（方法） | |
| FFT (Fast Fourier Transform) | 快速傅里叶变换 (FFT) | keep "FFT" abbreviation |
| GPU / CPU fallback | GPU / CPU 回退 | |
| compute backend | 计算后端 | |
| feature-gated (Rust `#[cfg(feature = ...)]`) | 特性门控 / （通过 Cargo feature 启用） | prefer a parenthetical gloss over a terse coinage on first use per document |
| connected component | 连通分量 | mesh-splitting sense (`split_mesh_into_granules`) |
| granule | 颗粒（分量） | the codebase uses "granule" for one connected-component particle; keep consistent with "颗粒" used for particle elsewhere, disambiguate with context |
| lognormal rebalancing | 对数正态重平衡 | |
| edge-artifact trimming | 边缘伪影裁剪 | CT-scan context |

This list is a starting point, not exhaustive — the translator should extend it as new terms surface, and should keep it in sync with this file (or split it into its own glossary file under `docs/zh-cn/` if it grows large).

## Style and formatting conventions

1. **Structure mirrors `en-us/` exactly** — same headings, same table columns, same document boundaries. Do not merge, split, or reorganize sections during translation; if the English structure has a problem, fix it in `en-us/` first (per the `AGENTS.md` documentation-sync requirement) and then mirror the fix.
2. **Code blocks, signatures, and file:line references are copied verbatim**, never translated or reformatted.
3. **Tables**: translate header labels and cell prose; keep type names, field names, and file paths in the original English/code form.
4. **Callouts** (`> **Feature-gated:**`, `> **Doc note:**`, `> **Algorithm:**`) — translate the label itself as agreed in a callout-label glossary (e.g. `> **特性门控：**`, `> **文档说明：**`, `> **算法：**`) and keep the callout markdown syntax (`>` blockquote, bold label) identical.
5. **Cross-reference links** (`[text](relative/path.md#anchor)`) — the `.md` filename and `#anchor` must stay byte-identical to the English version's link target's *heading text* (since anchors are derived from headings, and headings are not translated per the anchor rule above); only the link's visible `text` may be translated.

## Known risk: anchor slugification and CJK headings

**Flag this explicitly before starting the translation pass.** The anchor convention used throughout `docs/en-us/` relies on GitHub-flavored Markdown's automatic heading-to-anchor slugification (e.g. `#### mesh_metrics` → `#mesh_metrics`). Because function/struct headings are intentionally left untranslated (see above), `zh-cn/` documents will have **English-language anchors on Chinese-language pages** — this is intended and should work correctly on GitHub's renderer. However, if any other Markdown renderer is used to host `docs/zh-cn/` (e.g. a static site generator, IDE preview, or documentation platform other than GitHub), verify its CJK slugification behavior before relying on cross-links — some renderers strip, transliterate, or hash CJK characters differently, and mixed English/CJK headings can produce different slugs across tools. If any *prose* subheadings within the algorithm docs (which are not part of the fixed function-anchor convention) are translated to Chinese, their anchors will need separate verification against whatever renderer is actually used.
