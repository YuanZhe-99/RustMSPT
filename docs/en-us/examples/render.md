# Render Pipeline Walkthrough

## Config

The checked-in example is `data/input/render_config.yaml`. Its focus point is near the center of `data/input/particles.stl`; `projection` may be changed between `orthographic` and `perspective`.

```bash
./target/release/rustmspt render --config data/input/render_config.yaml
```

Captured CPU-only release output:

```text
[Info] CPU setting: cpu_max=-1 -> using 8 worker threads (available 8).
[Info] STL file(s) loaded from: data/input/particles.stl (2830 vertices, 5600 faces)
[Info] Camera: projection=Orthographic, eye=(74.0129,576.0129,1303.9871), forward=(0.5774,0.5774,-0.5774), resolution=1024x1024
[Info] Acceleration: requested=auto, effective=cpu
[Info] Acceleration fallback: cargo feature 'gpu' is not enabled
[Info] Rendering with CPU backend (ray casting)
[Info] Rendered image (1024x1024) saved to: data/output/rendered.png
```

The output is a 1024x1024 RGBA PNG containing the shaded particles on a white background. `--input` and `--output` override `render.stl_path` and `render.output_path`. Build with `--features gpu` and use `acceleration.mode: gpu` (or `auto` above `gpu_min_pixels`) to request offscreen wgpu rasterization; failures retain CPU output behavior.

### Timing lines (added 2026-09-25)

Captured output above predates the shared stage timer. Current builds also print `[Timing] render stage=<name> seconds=<f>` for each completed stage, then `[Timing] render workers=<n>` and `[Timing] render peak_rss_bytes=<n|unavailable>`. Stage names are listed in `../reference/pipeline-core.md` (`pipeline/timing.rs`); output files are unchanged.
