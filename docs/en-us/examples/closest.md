# `closest` — the nearest point on a surface or on lines

`closest` answers, for every query point, the nearest triangle of a surface (or
the nearest segment of a set of lines), the closest point on it and the
distance. The rule and the design are in
[`algorithms/closest-point.md`](../algorithms/closest-point.md).

## Inputs

Three `.npy` arrays, written by the caller with `numpy.save`:

| File | Shape | Type | Meaning |
|---|---|---|---|
| `vertices.npy` | `(n, 3)` | f64 | the surface's (or the lines') vertices |
| `corners.npy` | `(m, 3)` or `(m, 2)` | int64 | triangles, or segments, as vertex indices |
| `queries.npy` | `(..., 3)` | f64 | the points to settle |

## Config

```yaml
closest:
  vertices: "work/closest/vertices.npy"
  triangles: "work/closest/corners.npy"
  queries: "work/closest/queries.npy"
  output_dir: "work/closest/out"
  cpu_max: -1
acceleration:
  mode: gpu
```

For segments, name the corner file under `segments:` instead of `triangles:`;
giving both, or neither, is refused.

## Run

```bash
cargo build --release --features gpu
./target/release/rustmspt closest --config work/closest/closest.yaml
```

Captured on a real case: the centres of the 1,152,427 cells of a tetrahedral
mesh against the 25,608-triangle closure mid-surfaces it was cut to, with eight
workers and an integrated GPU:

```
[Info] GPU adapter: name=Intel(R) Iris(R) Plus Graphics 655 (CFL GT3) backend=Vulkan type=IntegratedGpu class=hardware
[closest] 1152427 points on 25608 Triangles; backend gpu; 8 workers; seed 0.25 s, search 1.21 s
[Timing] gpu init_seconds=0.095582 upload_bytes=13829156 upload_calls=4 readback_bytes=4609708 readback_calls=2 readback_wait_seconds=0.114961
```

The same run with `mode: cpu` took 1.45 s to search, with identical
primitives, points and distances. VTK's `find_closest_cell` took 25.1 s for
the same points; its distances agree with these to `2e-12`, and where its
triangle index differs (36 % of these points, almost all on an edge or a vertex
several triangles share) the distances tie.

## Outputs

`primitive.npy` (int64, `-1` when there are no primitives), `closest.npy`
(`(..., 3)` f64) and `distance.npy` (f64), shaped as the query's leading axes,
and `report.json`:

```json
{
  "backend": {
    "adapter": "Intel(R) Iris(R) Plus Graphics 655 (CFL GT3) (Vulkan)",
    "fallback_reason": null,
    "requested": "gpu",
    "used": "gpu"
  },
  "bvh": { "depth": 14 },
  "distance_tie": 1e-12,
  "kind": "triangles",
  "method_version": "closest-point-v1",
  "primitives": 25608,
  "query_shape": [1152427],
  "rule": "the primitive at the least exact f64 distance wins; primitives within distance_tie (relative) of it tie and the lowest index wins",
  "tool": { "name": "rustmspt", "version": "0.2.1", "features": ["default", "gpu"], "...": "..." },
  "vertices": 14101,
  "wall_time_s": { "build": 0.017, "read": 0.021, "search": 1.215, "seed": 0.251, "total": 1.542, "write": 0.036 },
  "workers": 8
}
```
