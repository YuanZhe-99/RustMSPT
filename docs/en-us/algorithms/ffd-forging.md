# FFD Forging

## What this simulates

Forging in RustMSPT is a **geometric approximation** of axial-compression forging: a mesh
representing a microstructure sample (a particle, a pore/void, or an arbitrary region of a larger
structure) is squeezed along one axis and allowed to bulge laterally, mimicking the shape change a
workpiece undergoes under a press. This is verified directly from `src/geometry/forging.rs`: the
entire transform is a per-vertex affine scaling around a chosen center point (`v' = center +
scale * (v - center)`), with a small extra radial scaling pass for "void" meshes. There is no
material model, no plasticity, no stress/strain field, no contact mechanics, and no FEA — it is a
closed-form kinematic reshaping, not a physics simulation. It answers "how would this bounding
region's shape and a tracked sub-region change if the surrounding material were compressed by this
much," not "what forces or stresses would develop."

Two functions implement this, of increasing generality: `simulate_forging_ffd` (a minimal Z-axis
version) and `simulate_forging_ffd_with_tracking` (the general version actually used by
`ForgePipeline`). Full per-parameter signatures live in the reference doc — see
[Cross-references](#cross-references).

## The simple version: `simulate_forging_ffd`

`simulate_forging_ffd(mesh, compression_ratio, bulge_factor)` compresses a mesh along **Z only**,
around the mesh's **own** axis-aligned bounding-box (bbox) center:

1. Compute `center` as the midpoint of `mesh_bbox(mesh)` (falling back to the unit box if the mesh
   has no geometry).
2. `axis_scale = clamp(1 - compression_ratio, 0.01, 1.0)` — this is the fraction of the original
   Z-extent that remains after compression, i.e. `compression_ratio` is conceptually
   `1 - (final_height / initial_height)`. A `compression_ratio` of `0.2` means the compressed
   height is 80% of the original; the value is clamped so the mesh can never be squashed to zero
   thickness (`axis_scale` bottoms out at `0.01`, a 99% height reduction).
3. `lateral_scale = (1 / sqrt(axis_scale)) ^ clamp(bulge_factor, 0, 1)` — the X/Y scale factor.
   When `bulge_factor = 0`, `lateral_scale = 1` (no bulge: the mesh only gets shorter, its
   cross-section unchanged — net volume shrinks). When `bulge_factor = 1`,
   `lateral_scale = 1 / sqrt(axis_scale)`, which is the scale that would exactly conserve volume
   under incompressible, radially-symmetric bulging in a plane-strain sense (area must grow by
   `1/axis_scale` for volume conservation, and area grows with the square of the linear scale, so
   the linear factor is `1/sqrt(axis_scale)`). Intermediate values of `bulge_factor` interpolate
   (in log space) between "no bulge" and "volume-conserving bulge," letting the caller dial in how
   much lateral material displacement accompanies the compression.
4. Every vertex is remapped: `x' = center.x + (x - center.x) * lateral_scale`, same for `y`, and
   `z' = center.z + (z - center.z) * axis_scale`.

This is the whole-mesh case: the deformation center is derived from the mesh being deformed, so
there is no notion of "this mesh is a sub-region of something larger."

## The general version: `simulate_forging_ffd_with_tracking`

`simulate_forging_ffd_with_tracking` is what `ForgePipeline` actually calls. It keeps the same
scale-around-a-center kinematics but generalizes it in four ways:

**1. Configurable compression axis.** `compression_axis` is `0`/`1`/`2` for X/Y/Z. Whichever axis
is selected gets `axis_scale`; the other two both get `lateral_scale`. The formulas for
`axis_scale` and `lateral_scale` are identical to the simple version.

**2. Deformation center from a caller-supplied `lattice_bbox`, not the mesh's own bbox.** The
center used in the per-vertex transform is the midpoint of `lattice_bbox`, a `BoundingBox` passed
in by the caller — it does not have to equal `mesh_bbox(mesh)`. This matters when the mesh being
deformed is a sub-region carved out of, or tracked within, a larger reference frame (e.g. a
cropped particle/void mesh that should compress as if it were still embedded in the full lattice,
around the lattice's center rather than its own local center). Using the lattice center rather
than the local mesh center keeps sub-region deformations consistent with how the surrounding
structure would move under the same press stroke.

**3. Void densification.** When `mesh_type` (case-insensitively) is `"void"`, an extra pass runs
after the main FFD transform: a `closure` factor is computed as
`clamp(1 - 0.05 * compression_ratio * void_densification, 0.85, 1.0)`, and every vertex is pulled
inward toward the mesh's own centroid (`mesh_centroid`, computed on the already-compressed mesh) by
that factor. Physically this models **void/pore closure under compression**: pores in a real
forged workpiece tend to collapse disproportionately relative to the bulk solid material as
pressure is applied, so a void mesh is scaled down slightly more than a plain lateral-bulge
transform alone would produce. `void_densification` is a tunable multiplier on how aggressive that
extra closure is (default `1.0` in the pipeline config); `closure` is bounded to `[0.85, 1.0]` so
it always shrinks (or leaves unchanged) rather than inverts or over-collapses the void geometry.

**4. Optional ROI bounding-box tracking.** If a `track_bbox: Option<BoundingBox>` is supplied, its
8 corners are each passed through the exact same `transform_point` closure used for mesh vertices
(main compression only — the void-densification pass, which operates on mesh vertices/centroid,
does not apply to the tracked box), and the transformed corners' axis-aligned bounding box is
recomputed as the new tracked ROI. This lets a caller define an arbitrary region of interest
(which need not coincide with the mesh's own extent) and learn where that region ends up — its new
position, size, and shape approximation — after the same deformation that moved the mesh, without
needing to re-derive it from the deformed mesh's geometry.

## How `ForgePipeline::run` uses this

`ForgePipeline::run` (`src/pipeline/forge.rs:58`) wires the above into an end-to-end pipeline:

1. Loads the input STL (or merges a folder of STLs) into `mesh`.
2. Computes `lattice_bbox = mesh_bbox(&mesh)` (the whole loaded mesh's bbox, used as both the
   default deformation center and the default VF/region fallback).
3. Parses an optional ROI box from `config.forging.roi_bounding_box` (a flat 6-element
   `[min_x,min_y,min_z,max_x,max_y,max_z]` list) via `parse_roi_bbox`.
4. Computes the **volume fraction inside the ROI before forging** (`before_roi_vf`), via
   `volume_fraction_in_bbox` over the ROI if present, else over `lattice_bbox`.
5. Resolves parameters with defaults: `compression_ratio` (0.2), `compression_axis` (parsed from a
   string `"x"/"y"/"z"`, default `"z"`, via `parse_compression_axis`), `orient_to_positive_volume`
   (false), `bulge_factor` (0.5), `mesh_type` (`"particle"`), `void_densification` (1.0).
6. Calls `simulate_forging_ffd_with_tracking(&mesh, lattice_bbox, roi_bbox, compression,
   compression_axis, bulge, mesh_type, void_densification)`, obtaining the compressed mesh and the
   transformed ROI box.
7. Computes the **volume fraction inside the (transformed) ROI after forging** (`after_roi_vf`),
   comparing packing density inside the region of interest before vs. after the compression —
   this is the quantitative signal the pipeline exists to produce.
8. If `orient_to_positive_volume` is enabled, calls `orient_components_to_positive_volume` on the
   compressed mesh to flip any mesh components whose signed volume went negative (e.g. from the
   lateral bulge inverting a component's winding), and records how many of how many components
   were flipped.
9. **Aligns the output translation to the original ROI position:** if both the original ROI
   (`roi_bbox`) and the tracked/transformed ROI (`tracked_roi`) are available, computes
   `output_shift = roi_in.min - roi_out.min` and translates the whole output mesh by that shift
   (skipped if the shift is negligible on every axis). This makes the forged output's ROI land back
   at the same absolute position the caller specified the ROI at, rather than drifting because the
   lattice-relative compression moved it.
10. Saves the forged mesh as an STL (`save_stl`) and writes a text report (`<output>.txt`)
    containing: bbox before/after (whole mesh), ROI bbox before/after (with the output shift
    applied to the "after" values), spatial ROI volume fraction before/after, the compression axis
    label, orientation-fix stats (if enabled), and the output translation vector. The same
    information is echoed to stdout as `[Info]` lines.

## Config fields

From `ForgingParams` in `src/config/forging.rs`, under the top-level `forging:` key:

| Field | Type | Default (when omitted) | Meaning |
|---|---|---|---|
| `input_stl_path` | `String` | — (required) | Path to the input STL file or a folder to merge. |
| `output_stl_path` | `Option<String>` | `data/output/forged_mesh.stl` | Path for the forged output STL; the report is written alongside it with a `.txt` extension. |
| `compression_ratio` | `Option<f64>` | `0.2` | Fraction of the compression-axis extent removed (see formula above). |
| `compression_axis` | `Option<String>` | `"z"` | One of `"x"`, `"y"`, `"z"` (case-insensitive); selects which axis is compressed and which two get lateral bulge. |
| `orient_to_positive_volume` | `Option<bool>` | `false` | Whether to run `orient_components_to_positive_volume` after deformation. |
| `bulge_factor` | `Option<f64>` | `0.5` | Interpolates lateral bulge between none (`0`) and volume-conserving (`1`). |
| `roi_bounding_box` | `Option<Vec<f64>>` | `None` (no ROI tracking; falls back to whole-mesh bbox) | Flat 6-element `[min_x,min_y,min_z,max_x,max_y,max_z]` region of interest to track through the deformation. |
| `mesh_type` | `Option<String>` | `"particle"` | Set to `"void"` (case-insensitive) to enable the extra centroid-closure densification pass. |
| `void_densification` | `Option<f64>` | `1.0` | Multiplier on how aggressively void meshes densify under compression. |

## Cross-references

- [geometry-volume-collision.md](../reference/geometry-volume-collision.md) — full function-level
  reference for `simulate_forging_ffd` and `simulate_forging_ffd_with_tracking` (signatures,
  parameter tables, source locations).
- [pipeline-core.md#ForgePipeline::run](../reference/pipeline-core.md#forgepipelinerun) — full
  reference entry for the pipeline that drives this algorithm end to end.
