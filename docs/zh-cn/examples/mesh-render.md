# `mesh-render` — 将 VTU 体网格渲染为多视角 PNG

> **待翻译（pending translation）**：本页为结构占位，完整演练见英文版
> [`docs/en-us/examples/mesh-render.md`](../../en-us/examples/mesh-render.md)。
> 摘要：`mesh-render` 读取契约 VTU（或受支持子集内的普通四面体 VTU），
> 按配置的命名视角（front/back/left/right/top/bottom/iso_*）或自定义相机
> 输出 PNG；支持组合过滤器（region_key、partition、cell_kind、裁剪面等）、
> 逐区域透明度、特征曲线叠加与透明背景。配置文件：
> `data/input/mesh_render_config.yaml`。
