# `forge` pipeline example

## What it does

`forge` applies a free-form-deformation (FFD) "forging" transform to a mesh: it compresses the
mesh along one axis while allowing lateral bulge (approximating axial-compression forging
kinematically, with no material/stress model), tracks how a region of interest (ROI) moves through
the deformation, realigns the output so the ROI lands back at its originally requested location,
and writes both the forged STL and a text report of before/after bounding boxes and ROI volume
fractions. See `../algorithms/ffd-forging.md` for the full derivation of the underlying
per-vertex transform.

## Config

`data/input/forge_config.yaml` (the repository's real default config for this pipeline):

```yaml
forging:
  # Input file path (can be absolute or relative to project root)
  input_stl_path: "data/output/optimized_structure.stl"

  # Output file path
  output_stl_path: "data/output/forged_mesh.stl"

  # Orientation fix after deformation (can be expensive)
  # false by default to skip; set true to enforce positive signed volume orientation.
  orient_to_positive_volume: false

  # Compression Ratio (0.0 to 1.0)
  # 0.2 means compressing the selected axis length by 20%
  compression_ratio: 0.5

  # Compression Axis
  # Choose which axis to compress: 'x', 'y', or 'z'
  # Default is 'z' when omitted.
  compression_axis: 'y'

  # Bulge Factor (Material Flow Control)
  # 0.0 = Vertical compression only (no side expansion)
  # 0.5 = Balanced volume conservation (Standard metal)
  # 1.0 = High side expansion (Soft material like rubber/clay)
  bulge_factor: 0.5

  # Region of Interest [min_x, min_y, min_z, max_x, max_y, max_z]
  # Leave empty [] to forge the entire model based on its bounding box.
  # Use this if you only want to squash a specific part of the geometry.
  roi_bounding_box: [0, 0, 0, 50, 50, 50]

  # Mesh Type
  # 'particle': only deform the original vertices
  # 'void': geometric deformation + void densification correction (suitable for void models, noticeable VF reduction)
  mesh_type: 'void'

  # Void Densification Strength (only effective when mesh_type is 'void')
  # 1.0 = standard densification
  # 2.0 = strong densification
  void_densification: 0.05
```

As with the `scale` example, the default `input_stl_path` points at
`data/output/optimized_structure.stl` (the expected output of `optimize`), which doesn't exist in
a fresh checkout. This walkthrough instead points `forge` at the real, already-present
`data/output/packed_result.stl` via `--input`, and writes to a scratch path via `--output`.

| Field | Meaning |
|---|---|
| `cpu_max` (optional) | Worker budget for the run; absent or `-1` uses every available worker. |
| `compression_ratio` | Fraction of the compression axis's extent removed. `0.5` means the compressed extent is 50% of the original, clamped internally to never fall below 1% of original (`axis_scale` floor of `0.01`). |
| `compression_axis` | Which axis (`x`/`y`/`z`) is compressed; the other two receive lateral "bulge" expansion. Defaults to `z` if omitted. |
| `bulge_factor` | Interpolates (in log space) between no lateral expansion (`0.0`) and volume-conserving lateral expansion (`1.0`). |
| `roi_bounding_box` | A 6-element `[min_x, min_y, min_z, max_x, max_y, max_z]` sub-region tracked through the deformation; the output mesh is translated so this region ends up back at its original location. Omit/empty to forge the whole mesh bbox. |
| `mesh_type` | `"particle"` deforms vertices only; `"void"` additionally applies a void-densification correction scaled by `void_densification`. |
| `void_densification` | Strength of the extra densification pass, only used when `mesh_type: 'void'`. |

See `../reference/config.md` (`forging.rs` section: `ForgingParams`, `ForgingConfig`) for the full
field reference, and `../reference/pipeline-core.md`'s `ForgePipeline::run` entry for the exact
algorithm sequence (deform -> re-orient (optional) -> translate to realign ROI -> write STL +
report).

## Running it

```bash
./target/release/rustmspt forge \
  --config data/input/forge_config.yaml \
  --input data/output/packed_result.stl \
  --output /tmp/rustmspt-doc-examples/forged_mesh.stl
```

`forge` also writes a text report alongside the STL, at the same path with the extension replaced
by `.txt` — here that lands at `/tmp/rustmspt-doc-examples/forged_mesh.txt`.

## Expected output

Real captured stdout from the run above:

```
[Info] Forging completed.
[Info] BBox before: min=(-10.7846,2.8307,3.6017), max=(104.6636,105.4615,97.3297)
[Info] BBox after:  min=(-12.7416,1.4468,4.3634), max=(124.3786,52.6980,115.6861)
[Info] ROI BBox before: min=(0.0000,0.0000,0.0000), max=(50.0000,50.0000,50.0000)
[Info] ROI BBox after:  min=(0.0000,0.0000,0.0000), max=(59.4604,25.0000,59.4604)
[Info] Spatial ROI VF before: 0.024314
[Info] Spatial ROI VF after:  0.024208
[Info] Compression axis: y
[Info] Orientation fix enabled: false
[Info] Output translation: (8.8813,-27.0730,9.5485)
[Info] Output STL written: /tmp/rustmspt-doc-examples/forged_mesh.stl
[Info] Output report written: /tmp/rustmspt-doc-examples/forged_mesh.txt
```

Real content of `/tmp/rustmspt-doc-examples/forged_mesh.txt`:

```
Method: forge
BBox before: min=(-10.7846,2.8307,3.6017), max=(104.6636,105.4615,97.3297)
BBox after:  min=(-12.7416,1.4468,4.3634), max=(124.3786,52.6980,115.6861)
ROI BBox before: min=(0.0000,0.0000,0.0000), max=(50.0000,50.0000,50.0000)
ROI BBox after:  min=(0.0000,0.0000,0.0000), max=(59.4604,25.0000,59.4604)
Spatial ROI VF before: 0.024314
Spatial ROI VF after:  0.024208
Compression axis: y
Orientation fix enabled: false
Output translation: (8.8813,-27.0730,9.5485)
```

The report text mirrors the stdout `[Info]` lines almost exactly, minus the two path-confirmation
lines (which only make sense in a console context). The run completed in `real 0m0.120s` — `forge`
is a single deterministic pass (per-vertex affine transform plus, for `mesh_type: 'void'`, a
densification pass), so it scales with triangle count and has no iterative/stochastic cost.

Note the ROI's Y extent (the compression axis) shrank from 50 to 25 units — exactly the requested
`compression_ratio: 0.5` (50% reduction) — while its X and Z extents grew from 50 to ~59.46, the
bulge lateral expansion at `bulge_factor: 0.5`. The ROI volume fraction barely moved (`0.024314`
-> `0.024208`, a ~0.4% relative change) because `mesh_type: 'void'` with a small
`void_densification: 0.05` applies only a mild correction on top of the geometric transform.

## Notes

- `roi_bounding_box` must have exactly 6 elements to take effect — a wrong-length list is silently
  treated as "no ROI" (whole-mesh forging) rather than raising a config error; see
  `ForgePipeline::parse_roi_bbox` in `../reference/pipeline-core.md`.
- `compression_axis` accepts only `x`/`y`/`z` (case-insensitive, trimmed); anything else is a hard
  config error (`InvalidConfig`), not a silent fallback.
- Per the pipeline-core reference, `ForgePipeline::run` computes whole-mesh volumes before/after
  the transform but currently discards them (bound to `_`-prefixed variables) — only the
  ROI-restricted volume fraction is surfaced in the report, which is why this example's stdout
  shows `Spatial ROI VF` rather than whole-mesh VF figures.
- For `mesh_type: 'void'`, increasing `void_densification` toward `1.0`–`2.0` produces a much more
  visible volume-fraction change than the `0.05` used here; `0.05` was chosen in the default config
  to keep the correction subtle.
- See `../algorithms/ffd-forging.md` for the closed-form derivation of `axis_scale` and
  `lateral_scale` from `compression_ratio` and `bulge_factor`, and for how
  `simulate_forging_ffd_with_tracking` generalizes the single-axis version to arbitrary
  compression axes and caller-supplied ROI/lattice bounding boxes.

### Timing lines (added 2026-09-25)

Captured output above predates the shared stage timer. Current builds also print `[Timing] forge stage=<name> seconds=<f>` for each completed stage, then `[Timing] forge workers=<n>` and `[Timing] forge peak_rss_bytes=<n|unavailable>`. Stage names are listed in `../reference/pipeline-core.md` (`pipeline/timing.rs`); output files are unchanged.
