# Render Pipeline Walkthrough

```bash
./target/release/rustmspt render --config data/input/render_config.yaml
```

该示例读取 `data/input/particles.stl`，以正交投影生成 `data/output/rendered.png`。`projection` 可切换为 `perspective`；`--input` 和 `--output` 可覆盖路径。默认构建使用 CPU，带 `gpu` feature 的构建可请求 wgpu 离屏渲染并在失败时回退 CPU。完整捕获输出见[英文示例](../../en-us/examples/render.md)。
