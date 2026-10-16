# State-Map 点定位 —— `state-map`

`state-map` 子命令的参考文档：`.npy` 读写（`src/transfer/npy.rs`）、四面体网格及其包围体层次结构与定位规则（`src/transfer/locate.rs`）、GPU 候选遍（`src/gpu/tet_candidates.rs`、`src/gpu/shaders/tet_candidates.wgsl`）、流水线（`src/pipeline/state_map.rs`）及其配置（`src/config/state_map.rs`）。算法与设计理由见 [`algorithms/state-map-location.md`](../algorithms/state-map-location.md)。

> **待翻译：** 各函数条目（签名、用途、参数、返回值、副作用、说明）尚未翻译，以英文版 [`reference/pipeline-state-map.md`](../../en-us/reference/pipeline-state-map.md) 为准。下面的索引与英文版一致。

## 索引

| 项 | 源码 | 概述 |
|---|---|---|
| `read_npy` / `write_npy_f64` / `write_npy_i64` | `src/transfer/npy.rs` | 读写 `.npy`（读：v1–3，小端数值；写：f64、i64）。 |
| `TetMesh` | `src/transfer/locate.rs:10` | 源四面体：角点、形心、边矩阵之逆、是否可逆。 |
| `TetBvh` | `src/transfer/locate.rs:198` | 填充后四面体包围盒上的静态层次结构，GPU 遍历同一组数组。 |
| `TetBvh::containing` | `src/transfer/locate.rs:287` | 规则 1：包含该点且形心最近的四面体。 |
| `TetBvh::nearest` | `src/transfer/locate.rs:316` | 规则 2：界限内最近四面体的分支定界。 |
| `locate_cpu` | `src/transfer/locate.rs:415` | 在 CPU 上并行定位所有点。 |
| `locate_from_candidates` | `src/transfer/locate.rs:430` | 由 GPU 候选槽完成定位；结果与 `locate_cpu` 相同。 |
| `GpuTetCandidatePipeline::candidates` | `src/gpu/tet_candidates.rs:130` | 每个点的、向外取整盒包含该点的叶槽（超集），按设备上限分批。 |
| `StateMapParams` / `StateMapConfig` | `src/config/state_map.rs` | `state_map` YAML 块与可选的 `acceleration` 块。 |
| `StateMapPipeline::run` | `src/pipeline/state_map.rs:32` | 读入、构建、定位，写出 donors/distances/report。 |
