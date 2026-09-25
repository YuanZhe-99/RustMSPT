# Render Pipeline Walkthrough

```bash
./target/release/rustmspt render --config data/input/render_config.yaml
```

该示例读取 `data/input/particles.stl`，以正交投影生成 `data/output/rendered.png`。`projection` 可切换为 `perspective`；`--input` 和 `--output` 可覆盖路径。默认构建使用 CPU，带 `gpu` feature 的构建可请求 wgpu 离屏渲染并在失败时回退 CPU。完整捕获输出见[英文示例](../../en-us/examples/render.md)。

### 计时行（2026-09-25 新增）

上面的捕获输出早于共享阶段计时器。当前版本还会为每个已完成阶段打印 `[Timing] render stage=<name> seconds=<f>`，随后打印 `[Timing] render workers=<n>` 与 `[Timing] render peak_rss_bytes=<n|unavailable>`。阶段名称见 `../reference/pipeline-core.md`（`pipeline/timing.rs`）；输出文件不变。
