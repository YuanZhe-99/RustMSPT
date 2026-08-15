# Mesh Generation Tooling — VTU I/O and the `mesh-render` Pipeline

Reference for the first implemented slice of the mesh-generation tooling phase
(`PLAN_mesh_generation.md` phase GA): the contract VTU reader/writer
(`src/io/vtu.rs`), the scene-extraction layer (`src/meshgen/render_scene.rs`),
the CPU scene renderer (`src/geometry/scene_render.rs`), and the `mesh-render`
subcommand (`src/pipeline/mesh_render.rs`, `src/config/mesh_render.rs`).

## Index

| Item | Source | Summary |
|---|---|---|
| `VTK_POLY_LINE`/`VTK_TRIANGLE`/`VTK_TETRA`/`VTK_VOXEL`/`VTK_HEXAHEDRON` | `src/io/vtu.rs:6` | VTK cell type codes accepted by the contract subset. |
| `ArrayData` | `src/io/vtu.rs:14` | Typed storage for one DataArray (U8/I32/I64/U32/U64/F32/F64) with `len`, `vtk_type`, `get_f64`, `get_i64` accessors. |
| `DataArray` | `src/io/vtu.rs:118` | Named array + component count; `DataArray::scalar` builds a 1-component array. |
| `VtuDoc` | `src/io/vtu.rs:138` | In-memory contract VTU: shared points, mixed cells (VTK end-offsets), point/cell/field data; `cell`, `cell_array`, `point_array`, `field_array`, `num_cells` accessors. |
| `VtuDoc::validate` | `src/io/vtu.rs:181` | Structural validation: monotone offsets, index ranges, per-type node counts, array lengths. |
| `VtuEncoding` | `src/io/vtu.rs:255` | `Ascii` (exact round-trip via shortest-form floats) or `AppendedRaw` (LittleEndian, UInt64 headers). |
| `save_vtu` | `src/io/vtu.rs:273` | Write a validated `VtuDoc` as VTK XML UnstructuredGrid; creates parent dirs. |
| `load_vtu` | `src/io/vtu.rs:479` | Read the contract subset (ascii + appended-raw, UInt32/UInt64 headers); rejects compressed/base64 files with a named error. |
| `SetKind` | `src/meshgen/render_scene.rs:8` | Render-set membership (`Volume` boundary faces vs tagged `Face` cells); Face wins coincident-hit dedup. |
| `SceneTri`/`SceneSegment`/`SceneMarker` | `src/meshgen/render_scene.rs:15` | World-space primitives with color/opacity emitted by extraction. |
| `RenderScene` | `src/meshgen/render_scene.rs:43` | Extraction result: triangles, overlay segments, markers, full-document framing bbox. |
| `SceneFilter` | `src/meshgen/render_scene.rs:52` | AND-composed filters: cell_kind, component, region_key, partition, regime, background, array_range, bbox, clip_plane (crinkle). |
| `ColorMode` | `src/meshgen/render_scene.rs:67` | Uniform / categorical (integer arrays) / scalar-viridis (float arrays) coloring; a named **point** array colours a cell by the mean of its non-sentinel point values. |
| `point_array_cell_value` | `src/meshgen/render_scene.rs:273` | Reduce a point array to one value per cell (mean of non-sentinel point values) for coloring. |
| `SceneSpec` | `src/meshgen/render_scene.rs:77` | Full extraction spec: filters, color mode, per-set opacities + region overrides, overlay toggles, highlight points. |
| `categorical_color` / `scalar_color` | `src/meshgen/render_scene.rs:132` | 12-color categorical palette (sentinel → grey) and compact viridis ramp. |
| `build_scene` | `src/meshgen/render_scene.rs:358` | VtuDoc + SceneSpec → RenderScene: filter chain, deterministic boundary-face/wireframe emission, tagged faces, curve segments, markers. Missing-array errors name the array. |
| `SceneRenderSettings` | `src/geometry/scene_render.rs:12` | Scene appearance; background is RGBA (alpha 0 = transparent PNG). |
| `render_scene_cpu` | `src/geometry/scene_render.rs:95` | CPU reference renderer: all-hits QBVH traversal per pixel ray, front-to-back alpha compositing, coincident-hit dedup (Face > Volume), depth-tested line overlay, markers. |
| `named_view` | `src/geometry/scene_render.rs:287` | Resolve front/back/left/right/top/bottom/iso_ne/iso_nw/iso_se/iso_sw to (view_direction, up). |
| `ViewSpec`/`FilterSpec` | `src/config/mesh_render.rs:7` | YAML forms of views (named preset or custom camera block) and kind-tagged filters. |
| `MeshRenderParams`/`MeshRenderConfig` | `src/config/mesh_render.rs:38` | `mesh_render:` YAML block (input VTU, output_dir, views, image, coloring, opacities, filters, overlays, camera). |
| `GpuClipPlane` | `src/gpu/scene_render.rs:45` | Optional half-space clip for the GPU preview (smooth cut, independent of the crinkle-clip filter). |
| `GpuSceneOptions` | `src/gpu/scene_render.rs:52` | GPU-only toggles: clip plane, overlay segments, markers. |
| `GpuScenePipeline` | `src/gpu/scene_render.rs:70` | Offscreen GPU preview: TriangleList with per-vertex colour + LineList overlay, both with clip-plane discard. |
| `GpuScenePipeline::render_views` | `src/gpu/scene_render.rs:330` | Batch path: one geometry upload reused across every camera; one image per view. |
| `MeshRenderPipeline` | `src/pipeline/mesh_render.rs:16` | The `mesh-render` subcommand: load VTU → build scene → one PNG per view (`<stem>_<view>.png`). |

## GPU preview path (GA-3c)

`src/gpu/scene_render.rs` + `shaders/scene_render.wgsl` render the *same*
`RenderScene` as an **opaque** preview. Two pipelines share one uniform block:

- **TriangleList** with a per-vertex colour attribute, so the preview reproduces the
  extraction layer's categorical/scalar colours instead of a single base colour. The
  headlight shading is the same formula as the CPU reference, which is what lets the
  two be compared pixel-by-pixel.
- **LineList** for curve segments and marker crosses, biased toward the camera in the
  vertex shader (`clip.z -= bias * clip.w`) so overlays lying exactly on a surface stay
  visible — the GPU counterpart of the CPU renderer's depth bias.

Both stages honour an optional **clip plane** through fragment `discard`. Note this is
a *smooth* cut and is independent of the extraction-time `clip_plane` filter, which is a
crinkle clip on whole cells; the renderer never applies one implicitly.

`render_views` is the **batch path**: geometry is built and uploaded once and reused
across every camera, so a diagnostic sheet of ten named views costs one upload rather
than ten. Only the uniform buffer and render targets are per-view.

Coincident tagged Face cells are uploaded after Volume boundary triangles and the
GPU depth comparison is `LessEqual`; this preserves the CPU reference rule that a
Face set wins over a coincident Volume set.

Two documented differences from the CPU reference:

| Aspect | CPU reference | GPU preview |
|---|---|---|
| Transparency | exact front-to-back compositing of all hits | **opaque only**; `alpha == 0` triangles are dropped, all others are painted solid |
| Markers | screen-space 7-pixel cross | three world-space axis arms, 1% of the scene bbox diagonal (a screen-space cross is not expressible in this pipeline) |

Select it with `backend:` in the config — `cpu` (default, the reference), `gpu`
(error if unavailable), or `auto` (GPU when present, silently falling back to CPU).
The acceptance test `gpu_scene_matches_cpu_within_tolerance` requires the two paths to
agree on an opaque scene within 2 LSB per channel for at least 98% of pixels, in both
projections, and skips gracefully when no adapter exists.

## The contract VTU (summary)

The authoritative specification is `PLAN_mesh_generation.md` §7. The subset
implemented today: one `UnstructuredGrid` piece; mixed cells (tets, tagged
triangle faces, poly-line curves, voxels accepted by the reader) sharing one
point array; cell arrays `cell_kind`, `region_key`, `partition_id`, `regime`,
`face_tag_key`, `curve_id`; point arrays `n_id_key`, `constraint_kind`,
`constraint_ref`; field-data set tables (`RegionSet*`, `FaceTag*`, `Curve*`)
plus metadata. `save_vtu` writes ascii or appended-raw; `load_vtu` reads both
plus common external variants (Float32 points, Int32 connectivity, UInt32
headers). Compressed and base64 VTUs are rejected with explicit errors.

## Renderer behavior notes

- **Point-field coloring**: `color_by` accepts a point array as well as a cell
  array. A cell then takes the **mean of its non-sentinel point values** (`-1`,
  the contract's not-applicable marker, and non-finite values are skipped; a cell
  with no valid point renders in the sentinel grey). This is what makes the
  stage fields that live on points renderable - `separation_t` (S3) and
  `sizing_h` (S4) - including on the surface-stage snapshots s00-s03, which carry
  no volume cells. The scalar range is auto-fitted over every cell that carries a
  value, and unlike cell arrays the mode also applies to tagged face cells.
- **Surface-stage cell arrays**: on a document with no volume cells (s00-s03) the
  tagged faces *are* the mesh, so a named cell array colours them directly and
  `array_range` filters them, instead of the face-tag categorical rule that applies
  when the named array belongs to tets. This is what makes `thin_role`,
  `band_region`, and `regime` renderable on a stage snapshot; documents with tets
  are unaffected, so the GA-5 volume baselines are unchanged.
- **Transparency** is exact on the CPU path: every ray enumerates all hits via a
  `RayIntersectionsVisitor` over the scene TriMesh's QBVH, sorts by distance,
  and composites front-to-back with early exit at saturation. Coincident hits
  (within a relative tolerance) are deduplicated preferring `Face`-set
  triangles, so a tagged face lying exactly on tet boundary faces does not
  double-composite.
- **Crinkle clipping** falls out of extraction: `bbox`/`clip_plane` filters drop
  cells, and faces of surviving tets against dropped tets become boundary faces.
- **Line occlusion** tests against the depth at which the surface pass became
  effectively opaque (transmittance < 1/512), with a small camera-ward bias so
  curves lying on surfaces stay visible; fully transparent volumes never occlude
  curves.
- **Framing** always uses the full document bbox, so all views and filter
  variations of one input frame identically.
- **Determinism**: hash-map-backed boundary faces and wireframe edges are sorted
  before scene emission, so repeated transparent renders do not depend on map
  iteration order.
- **The wireframe is never silently cut short.** The full edge set is built first;
  only if it exceeds `max_wireframe_edges` (default `DEFAULT_MAX_WIREFRAME_EDGES`,
  4,000,000) is it thinned, and then by a **uniform stride** over the sorted list
  rather than by stopping at the cap. A prefix cut amputates whole contiguous
  patches - the edge list is sorted by node key - and those read as holes in the
  mesh; a stride leaves an even, obviously-sparse frame. `RenderScene` reports
  `wireframe_edges_total` and `wireframe_edges_emitted`, and `mesh-render` prints a
  WARNING whenever they differ. The old 200,000 cap silently amputated A-6b and
  A-7b, which is what made A-7b's render look wrong.
- **GA-5 regression baselines** live under
  `data/fixtures/meshgen/render_baselines/cpu/`: `good_cube.vtu` x four variants
  (`opaque`, `filtered_region_1`, `transparent_exterior`, `clipped_x`) x all ten
  named views at 96x96. CPU comparisons allow no more than 2% of pixels to differ
  by more than 2 LSB. Regenerate deliberately with
  `RUSTMSPT_UPDATE_RENDER_BASELINES=1 cargo test --test mesh_visual_regression_tests`.
- The GPU regression covers the three opaque variants against the same CPU
  references. Full fixture scenes include rasterized line overlays, so the GA-5
  GPU budget is 5% of pixels beyond 2 LSB; the simpler GA-3c opaque parity test
  remains at 2%. Transparent GPU output is not compared because GPU compositing is
  intentionally opaque-only.
