# `scale` pipeline example

## What it does

`scale` rescales an STL mesh by a uniform factor derived from a physical unit conversion
(`mm_per_voxel` or `voxel_per_mm`) or from an explicit multiplicative `factor`, optionally fixing
component orientation to positive signed volume afterward. It is typically the last step in a
pipeline that produced geometry in "voxel" or arbitrary CAD units and needs it converted to real
physical units (millimeters) before further analysis or export.

## Config

`data/input/scale_config.yaml` (the repository's real default config for this pipeline):

```yaml
input:
  stl_path: "data/output/optimized_structure.stl" # STL file or folder containing STL files

output:
  stl_path: "data/output/scaled_mesh.stl"

scaling:
  # Options: "mm_per_voxel", "voxel_per_mm", "factor"
  type: "voxel_per_mm"
  # Value corresponding to the type
  value: 37.5
  # Orientation fix before output (can be expensive)
  # false by default to skip; set true to enforce positive signed volume orientation.
  orient_to_positive_volume: false
```

The default `input.stl_path` points at `data/output/optimized_structure.stl`, which is the
expected output of the `optimize` pipeline. For this walkthrough we instead point the pipeline at
`data/output/packed_result.stl` — a real mesh already present in the repo, produced by a prior
`pack` run — using the `--input`/`--output` CLI overrides described below, so nothing under
`data/input/` or `data/output/` had to change.

| Field | Meaning |
|---|---|
| `scaling.type` | `"mm_per_voxel"` and `"voxel_per_mm"` are unit-conversion modes; `"factor"` applies `value` directly as a multiplier. `"voxel_per_mm"` inverts `value` (`factor = 1.0 / value`) before applying it. |
| `scaling.value` | Must be `> 0` for `mm_per_voxel`/`voxel_per_mm` (validated; a non-positive value is a config error). Interpreted directly as the factor for `"factor"` mode. |
| `scaling.orient_to_positive_volume` | When `true`, runs `orient_components_to_positive_volume` after scaling and reports how many mesh components were flipped. Left `false` here to skip the extra cost. |

See `../reference/config.md` (`scale.rs` section: `ScalingParams`, `ScaleConfig`) for the full
field reference.

## Running it

```bash
./target/release/rustmspt scale \
  --config data/input/scale_config.yaml \
  --input data/output/packed_result.stl \
  --output /tmp/rustmspt-doc-examples/scaled_mesh.stl
```

`--input`/`--output` override `scaling.input.stl_path`/`output.stl_path` from the loaded YAML
before the pipeline runs (see `src/main.rs`'s `Commands::Scale` arm), which is how this example
avoids depending on `optimized_structure.stl` existing on disk.

## Expected output

Real captured stdout from the run above:

```
[Info] Scaling started.
[Info] Mode: voxel_per_mm | Value: 37.500000
[Info] Original bounds: min=(-10.7846,2.8307,3.6017), max=(104.6636,105.4615,97.3297)
[Info] Original volume: 22912.609296
[Info] Scaling completed with factor 0.026667.
[Info] Orientation fix enabled: false
[Info] Scaled bounds: min=(-0.2876,0.0755,0.0960), max=(2.7910,2.8123,2.5955)
[Info] Scaled volume: 0.434491
```

`scale` writes only the transformed STL (`/tmp/rustmspt-doc-examples/scaled_mesh.stl`); it has no
separate text report — everything relevant is printed to stdout as shown above. The run completed
in well under a second (`real 0m0.100s` on this machine); `scale` is a single pass over mesh
vertices with no iterative or stochastic component, so timing scales roughly linearly with
triangle count and is dominated by STL I/O for meshes of this size (12,084 triangles here).

With `voxel_per_mm = 37.5`, the effective factor is `1 / 37.5 ≈ 0.026667`, matching the printed
`Scaling completed with factor 0.026667` line and shrinking the ~105-unit bounding box down to
~2.8 mm, consistent with treating the packed geometry's original units as voxels at that
resolution.

## Notes

- `input.stl_path` (and thus `--input`) also accepts a folder of STL files, which are merged via
  `load_stl_or_merge_folder` before scaling — useful if particle geometry was exported as
  per-particle STL files rather than a single merged mesh.
- Volume scales with the cube of the linear factor: original volume `22912.6` -> scaled volume
  `0.434491` is consistent with `22912.6 * 0.026667^3 ≈ 0.434`.
- Switching `scaling.type` to `"factor"` and setting `scaling.value` directly to the desired
  multiplier (e.g. `0.026667`) produces an identical result to the `voxel_per_mm` example above —
  useful when you already know the linear factor rather than a voxel resolution.
- `orient_to_positive_volume: true` is worth enabling before downstream volume-fraction or
  packing-density calculations that assume consistently-wound, positive-volume components; leave
  it `false` (as in the default config) when you know the mesh is already well-oriented, since the
  check has a non-trivial per-component cost on large meshes.
- See `../reference/pipeline-core.md` for the full `ScalePipeline::run` behavior reference,
  including the exact error conditions for invalid `scaling.type`/`value` combinations.
