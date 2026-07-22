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
| `ColorMode` | `src/meshgen/render_scene.rs:67` | Uniform / categorical (integer arrays) / scalar-viridis (float arrays) coloring. |
| `SceneSpec` | `src/meshgen/render_scene.rs:77` | Full extraction spec: filters, color mode, per-set opacities + region overrides, overlay toggles, highlight points. |
| `categorical_color` / `scalar_color` | `src/meshgen/render_scene.rs:132` | 12-color categorical palette (sentinel → grey) and compact viridis ramp. |
| `build_scene` | `src/meshgen/render_scene.rs:358` | VtuDoc + SceneSpec → RenderScene: filter chain, boundary-face extraction of the selected tet subset, tagged faces, curve segments, wireframe, markers. Missing-array errors name the array. |
| `SceneRenderSettings` | `src/geometry/scene_render.rs:12` | Scene appearance; background is RGBA (alpha 0 = transparent PNG). |
| `render_scene_cpu` | `src/geometry/scene_render.rs:95` | CPU reference renderer: all-hits QBVH traversal per pixel ray, front-to-back alpha compositing, coincident-hit dedup (Face > Volume), depth-tested line overlay, markers. |
| `named_view` | `src/geometry/scene_render.rs:287` | Resolve front/back/left/right/top/bottom/iso_ne/iso_nw/iso_se/iso_sw to (view_direction, up). |
| `ViewSpec`/`FilterSpec` | `src/config/mesh_render.rs:7` | YAML forms of views (named preset or custom camera block) and kind-tagged filters. |
| `MeshRenderParams`/`MeshRenderConfig` | `src/config/mesh_render.rs:38` | `mesh_render:` YAML block (input VTU, output_dir, views, image, coloring, opacities, filters, overlays, camera). |
| `MeshRenderPipeline` | `src/pipeline/mesh_render.rs:16` | The `mesh-render` subcommand: load VTU → build scene → one PNG per view (`<stem>_<view>.png`). |

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
- The GPU preview path (PLAN GA-3c) is not yet implemented; `mesh-render` is
  CPU-only in this iteration.
