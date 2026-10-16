# `state-map` — locating a remesh's sampling points

`state-map` answers where each sampling point of a new mesh sits in the old,
deformed mesh, for a caller that carries a solved state across a remesh
(simHelper's state mapper). The rule and the design are in
[`algorithms/state-map-location.md`](../algorithms/state-map-location.md).

## Inputs

Three `.npy` arrays, written by the caller with `numpy.save`:

| File | Shape | Type | Meaning |
|---|---|---|---|
| `source_points.npy` | `(n, 3)` | f64 | the source mesh's nodes, deformed |
| `source_cells.npy` | `(m, 4)` | int64 | its tetrahedra |
| `query_points.npy` | `(..., 3)` | f64 | the sampling points (simHelper writes `(n_target, 15, 3)`) |

## Config

```yaml
state_map:
  source_points: "work/state/locate/source_points.npy"
  source_cells: "work/state/locate/source_cells.npy"
  query_points: "work/state/locate/query_points.npy"
  fallback_distance: 0.0026666666666666666
  output_dir: "work/state/locate/out"
  cpu_max: -1
acceleration:
  mode: auto
```

`gpu_slots` (default 24) is how many candidate tets the GPU keeps per point; a
point with more is settled on the CPU, so it changes speed, never the answer.

## Run

```bash
cargo build --release --features gpu
./target/release/rustmspt state-map --config work/state/locate/state_map.yaml
```

Captured on a real remesh (a 913,080-cell deformed source mesh and a
1,015,356-cell target, 15 sampling points per target cell), with eight workers
and an integrated GPU:

```
[Info] GPU adapter: name=Intel(R) Iris(R) Plus Graphics 655 (CFL GT3) backend=Vulkan type=IntegratedGpu class=hardware
[state-map] 15230340 points: 15123531 contained, 106809 fallback, 0 unlocated; backend gpu; 8 workers; locate 2.73 s
[Timing] gpu init_seconds=0.080907 upload_bytes=182764320 upload_calls=30 readback_bytes=1523034000 readback_calls=15 readback_wait_seconds=0.687726
```

The same run with `mode: cpu` located the points in 4.9 s on eight workers,
with identical donors and distances, and simHelper's mapping of the state from
these donors was byte-identical to the one its own Python search produced.

## Outputs

`donors.npy` (int64, `-1` for none) and `distances.npy` (f64: `0` inside, the
distance for a fallback donor, `NaN` for none), both shaped as the query's
leading axes — here `(1015356, 15)` — and `report.json`:

```json
{
 "backend": {
  "adapter": "Intel(R) Iris(R) Plus Graphics 655 (CFL GT3) (Vulkan)",
  "fallback_reason": null,
  "requested": "auto",
  "used": "gpu"
 },
 "barycentric_tolerance": 1e-12,
 "box_relative_pad": 1e-09,
 "distance_tie": 1e-12,
 "bvh": {"depth": 19, "nodes": 524287},
 "counts": {
  "contained": 15123531,
  "fallback": 106809,
  "gpu_overflow": 165,
  "points": 15230340,
  "unlocated": 0
 },
 "fallback_distance": 0.0026666666666666666,
 "method_version": "state-map-locate-v2",
 "query_shape": [1015356, 15],
 "rule": "a containing source tet wins (nearest centroid among several, then lower index); else the nearest tet within fallback_distance, where tets within distance_tie (relative) of the nearest tie and the lowest index wins; else none",
 "source_cells": 913080,
 "source_points": 199835,
 "tool": {"name": "rustmspt", "version": "0.2.1", "...": "the build identity, as `version --json`"},
 "wall_time_s": {"build": 0.588, "locate": 2.732, "read": 0.249, "total": 3.821, "write": 0.246},
 "workers": 8
}
```

`gpu_overflow` counts points whose candidates did not fit in `gpu_slots` and
were settled on the CPU instead. Under `mode: auto` a GPU that cannot start
leaves `used: "cpu"` and the reason in `fallback_reason`.
