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
| `RenderScene` | `src/meshgen/render_scene.rs:46` | Extraction result: triangles, overlay segments, markers, full-document framing bbox. |
| `SceneFilter` | `src/meshgen/render_scene.rs:57` | AND-composed filters: cell_kind, component, region_key, partition, regime, background, array_range, bbox, clip_plane (crinkle). |
| `ColorMode` | `src/meshgen/render_scene.rs:72` | Uniform / categorical (integer arrays) / scalar-viridis (float arrays) coloring; a named **point** array colours a cell by the mean of its non-sentinel point values. |
| `point_array_cell_value` | `src/meshgen/render_scene.rs:281` | Reduce a point array to one value per cell (mean of non-sentinel point values) for coloring. |
| `SceneSpec` | `src/meshgen/render_scene.rs:88` | Full extraction spec: filters, color mode, per-set opacities + region overrides, overlay toggles, highlight points. |
| `categorical_color` / `scalar_color` | `src/meshgen/render_scene.rs:132` | 12-color categorical palette (sentinel → grey) and compact viridis ramp. |
| `build_scene` | `src/meshgen/render_scene.rs:437` | VtuDoc + SceneSpec → RenderScene: filter chain, deterministic boundary-face/wireframe emission, tagged faces, curve segments, markers. Missing-array errors name the array. |
| `SceneRenderSettings` | `src/geometry/scene_render.rs:12` | Scene appearance; background is RGBA (alpha 0 = transparent PNG). |
| `render_scene_cpu` | `src/geometry/scene_render.rs:116` | CPU reference renderer: all-hits QBVH traversal per pixel ray, front-to-back alpha compositing, coincident-hit dedup (Face > Volume), depth-tested line overlay, markers. |
| `named_view` | `src/geometry/scene_render.rs:343` | Resolve front/back/left/right/top/bottom/iso_ne/iso_nw/iso_se/iso_sw to (view_direction, up). |
| `ViewSpec`/`FilterSpec` | `src/config/mesh_render.rs:7` | YAML forms of views (named preset or custom camera block) and kind-tagged filters. |
| `MeshRenderParams`/`MeshRenderConfig` | `src/config/mesh_render.rs:38` | `mesh_render:` YAML block (input VTU, output_dir, views, image, coloring, opacities, filters, overlays, camera). |
| `GpuClipPlane` | `src/gpu/scene_render.rs:53` | Optional half-space clip for the GPU preview (smooth cut, independent of the crinkle-clip filter). |
| `GpuSceneOptions` | `src/gpu/scene_render.rs:60` | GPU-only toggles: clip plane, overlay segments, markers, and `strip_rows` (rows per horizontal strip; `None` renders in one pass). |
| `GpuScenePipeline` | `src/gpu/scene_render.rs:85` | Offscreen GPU preview: TriangleList with per-vertex colour + LineList overlay, both with clip-plane discard. |
| `GpuScenePipeline::render_views` | `src/gpu/scene_render.rs:383` | Batch path: one geometry upload reused across every camera; one image per view. |
| `MeshRenderPipeline` | `src/pipeline/mesh_render.rs:20` | The `mesh-render` subcommand: load VTU → build scene → one PNG per view (`<stem>_<view>.png`). |

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
than ten. Uniform values change per view; one uniform buffer, color/depth target pair and staging buffer are reused throughout the batch.

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

### PERF-19 regression entry

GPU scene fixtures explicitly initialize both wireframe counters and import `SceneSegment`
under the GPU feature. Run `cargo test --offline --features gpu --test mesh_render_tests`.
This restores the existing opaque-preview tests without changing image baselines or rendering
semantics. Adapter-unavailable skips are not hardware validation.


### Prepared CPU scenes (PERF-17)

`geometry::scene_render::PreparedScene::new(&RenderScene)` builds immutable QBVH and material arrays once, borrowing the scene to prevent mutation during reuse. `render(&self, camera, width, height, settings)` retains the all-hits, sorted transparency and Face-over-Volume coincidence rules. Rayon task-local hit vectors retain capacity between pixels; deduplication compacts the same vector without a second allocation. `render_scene_cpu` remains the compatible one-shot wrapper. MeshRenderPipeline prepares once only on the CPU path and reuses across views. The command uses `render_views_to` to deliver owned GPU images to the bounded writer described below, retaining at most two delivered images during overlap. The compatibility `render_views` API explicitly collects images for callers requiring a batch.


### Streamed GPU views

`GpuScenePipeline::render_views_to(..., consume)` uploads scene geometry once and invokes a fallible consumer with `(view_index, owned_image)` in camera order. It reuses one uniform buffer, color/depth pair and unmapped staging buffer, clearing targets for each view. Consumer errors stop immediately; mapping errors and device errors propagate. Dimensions and staging/vertex limits are checked. `render_views` collects this stream for compatibility. The mesh-render command consumes owned frames in order; output errors never trigger CPU fallback. An auto-mode GPU failure may occur after earlier views were saved: CPU fallback rewrites all requested views in order. A strict GPU error preserves already-saved views. GPU PNG overlap now follows the bounded writer contract below.

### CPU worker budget

Optional top-level `cpu_max` (beside `mesh_render`) accepts an integer or integer string. Absent/-1 uses available CPUs; other values clamp to 1..available. The complete pipeline installs one Rayon pool, including scene preparation, all CPU views and GPU-to-CPU fallback. Logs distinguish requested and actual workers, and the CPU rendering entry reports its pool index. `backend: gpu` remains strict and opaque; `auto` can fall back to the CPU transparency reference. A CPU fallback does not create another pool.

| `MeshRenderPipeline::with_worker_pool` | `src/pipeline/mesh_render.rs:201` | Execute scene preparation, rendering and fallback within the worker budget. |

| `MeshRenderPipeline::run_in_pool` | `src/pipeline/mesh_render.rs:226` | Execute scene preparation, rendering and fallback within the worker budget. |

`RUSTMSPT_ACCELERATION=cpu|gpu|auto` overrides the validated YAML backend before input loading. Invalid environment values are errors. The effective `gpu` mode remains strict; `auto` permits fallback, and `cpu` does not initialize GPU. This does not apply STL render’s pixel threshold to the legacy mesh preview.

### Scene preview working-set policy

`mesh_render.gpu_memory_limit_mb` optionally limits the planned logical GPU working set. `gpu_min_pixels` applies only to auto mode and defaults to zero for legacy compatibility. Below-threshold auto avoids GPU initialization; explicit GPU ignores that threshold but obeys the budget. Auto falls back on budget/execution failure, while explicit GPU fails. CPU mode ignores GPU-only resource options.

The checked planner counts visible triangles at 120 bytes each, enabled segments at 64 bytes and markers at 192 bytes. It includes one color/depth pair (8 bytes/pixel), one readback buffer (256-byte-aligned RGBA rows), and 128 uniform bytes. Pending queue uploads coexist with destination geometry/uniform buffers, giving the conservative logical peak `2 * geometry_bytes + 256 + 8 * pixels + staging_bytes`. Multiple views reuse targets. Driver/pipeline internals and host scene/PNG memory are not included, so this is not a physical VRAM/RSS cap. Device limits are checked independently before host vertex expansion; expanded host arrays are released immediately after upload. The constructor also returns scoped GPU validation/allocation errors. Image tiling remains future work.

### CPU pixel task scheduling

Both nearest-hit STL rendering and prepared transparent scene rendering use disjoint contiguous pixel tasks. Images with at least one row per worker and at most 1024 pixels per worker retain row tasks, avoiding loss of parallelism from the minimum tile grain. Other images in a one-worker pool use one task; otherwise the initial grain targets four tasks per worker, clamped to 256..4096 pixels, aligning to complete rows when a row fits. Wide rows can span several tasks and short rows can share one. Each task derives its starting `(x,y)` once and advances the original integer pixel coordinates; ray arithmetic, hit ordering/compositing and serial overlays are unchanged. Scene depth and RGBA use identical task boundaries and reuse task-local hit scratch. Row-grain reference tests compare complete images under 1/2/8 workers for both projections, ragged tasks and extreme aspect ratios. Grain performance acceptance is tracked in PLAN.Performance.md §40.

### Bounded GPU PNG writer (2026-09-23)

For multiple GPU views with more than one configured worker, `consume_frames` moves ordered images through a zero-capacity channel to one PNG writer. At most one image is being encoded and one is held by the rendering producer; GPU targets remain reused. The writer is joined and accepted frames drained before returning or starting CPU fallback. Output errors, including failure of the final frame after production succeeds, take precedence over GPU errors and never trigger fallback. One-worker and one-view execution remain sequential. This establishes bounded overlap, not an end-to-end speedup claim; the software-GPU cold-process CLI comparison, including PNG identity and RSS, is recorded in PLAN.Performance.md §56; full workload/hardware acceptance remains open.

### Overlapped CPU PNG writing (PERF-17, 2026-09-25)

**Build and residency counters (PERF-17, 2026-09-25).** The CPU path prints `[mesh-render] cpu scene_qbvh_builds=<n> views=<n> max_live_images=<n>`. `scene_qbvh_builds` is the difference of the process-wide `geometry::scene_render::scene_qbvh_build_count()` (incremented once per `PreparedScene` with geometry) across preparation, so it reads 1 however many views are rendered; `max_live_images` is returned by `render_and_write_overlapped` (1 for a single view, 2 once a write overlaps a render). `mesh_render_cli_worker_budget_and_fallback` asserts `scene_qbvh_builds=1 views=2 max_live_images=2` at 1/2/8 workers. The GPU path uploads once per `GpuScenePipeline` and its host frames are bounded by the rendezvous writer (at most two, §56).

**GPU strips under a small budget (2026-09-25).** When `gpu_memory_limit_mb` cannot hold the whole image's color/depth targets and staging, the preflight no longer refuses: `scene_strip_rows` picks the tallest horizontal strip whose working set fits (exact boundary by binary search), the log reports `strip rows <n> of <height>`, and `GpuScenePipeline` renders each view strip by strip into a strip-sized target set. Each strip re-targets clip-space y onto its own NDC range (`y' = s*(y - c*w)`, composed in f64 before the f32 conversion) and a short final strip sets its viewport to its own rows; x, z and w are untouched, so depth is bit-identical and only y rounding could move an edge by a pixel. Geometry is still uploaded whole, so a budget below one row plus the geometry is still refused (auto falls back, gpu errors). On llvmpipe strip renders of 1/7/16/60 rows equal the one-pass render exactly for both projections (`gpu_strip_rendering_matches_one_pass`, which allows 2 % edge pixels for hardware); `mesh_render_gpu_budget_renders_in_strips` renders 1024x1024 under 1 MiB on the GPU with no CPU fallback and compares it with the unbudgeted image.

CPU views now go through `render_and_write_overlapped`: view *i* renders (with its nested pixel parallelism) inside `rayon::join` while view *i-1* is PNG-encoded and written on another worker of the same pool, so at most two finished-or-rendering frames exist and writes stay in view order. A write error is returned as soon as the concurrently started render finishes; no later view is rendered or written, and CPU writing never triggers any fallback. With one worker `join` runs render then write inline, which is the former sequential loop. Tests compare PNG bytes and order against a sequential loop at 1/2/4 workers for 0/1/2/7 views, check first/middle/last write errors (and that at most one extra view is rendered), and prove the next render starts before the previous write completes. The GPU writer keeps `consume_frames`: its producer is callback-driven (`render_views_to` pushes frames), so a per-frame `rayon::join` would need a pull-based GPU API, and a rayon scope with a blocking hand-off would park a pool worker on a channel; its blocking/drain/fallback order is covered by its existing tests and was not changed. The ignored release benchmark `cpu_overlap_benchmark` (8 views of a level-5 icosphere, 5 samples after warm-up, shared 4-core machine under concurrent builds) found no reliable difference: median overlapped/sequential ratios 0.95-1.23 across 256²/1024²/2048² and 1/2/4 workers, with sample spreads larger than the differences, because PNG encoding is a small fraction of CPU ray-cast time. Auto backend selection: STL `render` already applies `acceleration.gpu_min_pixels` (default 250,000) through `resolve_execution`; `mesh-render` keeps `gpu_min_pixels: 0` by default because switching small `auto` renders to CPU changes the image (CPU is the transparency reference, GPU the opaque preview), not only its cost.

### Opaque nearest coincidence group (2026-09-23)

Prepared scenes with at least 192 triangles and every clamped alpha exactly 1 use a nearest QBVH query followed by bounded all-hit enumeration through the outward-rounded `nearest + 2 * dedup_tol`. The same distance/triangle-ID sort and moving-anchor Face-over-Volume deduplication select the first group. Its chosen normal, color and depth feed the unchanged compositor and overlays. Smaller scenes, zero/partial/NaN alpha retain full all-hits. The triangle threshold avoids the measured extra-traversal regression on a 24-triangle scene; it is a conservative workload heuristic, not a guarantee for every spatial layout. Layered release comparisons and limitations are recorded in PLAN.Performance.md §57.
