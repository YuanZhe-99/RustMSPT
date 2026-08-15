# `mesh-render` — Render a VTU volume mesh to PNG views

Renders a contract VTU (see `docs/en-us/reference/mesh-render-and-vtu.md` and
`PLAN_mesh_generation.md` §7) — or any plain tet VTU in the supported subset —
to one PNG per configured view, with filters, per-region transparency, and
feature-curve overlays. This is the debugging/visualization tool of the
mesh-generation module (PLAN phase GA); it exists ahead of the `mesh` pipeline
itself so intermediate snapshots and external meshes can be inspected.

## Run

```bash
./target/release/rustmspt mesh-render --config data/input/mesh_render_config.yaml
# overrides:
./target/release/rustmspt mesh-render --input data/output/mesh.vtu --output data/output/mesh_render
```

`--input` replaces `mesh_render.input`; `--output` replaces
`mesh_render.output_dir`. Output files are `<input_stem>_<view>.png`.

## Configuration walkthrough (`data/input/mesh_render_config.yaml`)

| Field | Meaning |
|---|---|
| `input` | Path to the `.vtu` file (ascii or appended-raw, uncompressed) |
| `output_dir` | Directory for the PNGs (created if missing) |
| `views` | List of named presets (`front`, `back`, `left`, `right`, `top`, `bottom`, `iso_ne`, `iso_nw`, `iso_se`, `iso_sw`) and/or custom blocks `{name, view_direction, focus_point?, up_vector?}` |
| `width`, `height` | Image size (default 1024) |
| `background` | RGB or RGBA; alpha 0 gives a transparent PNG background |
| `color_by` | `uniform` or a cell-array name — integer arrays use the categorical palette, float arrays the viridis ramp (`scalar_min`/`scalar_max` clamp the range) |
| `volume_opacity` / `face_opacity` | Opacity of tet boundary faces / tagged face cells |
| `opacity_overrides` | Map of region_key (string) → opacity, e.g. `{ "0": 0.15 }` makes the background region see-through while interfaces stay opaque |
| `show_faces` / `show_curves` / `wireframe` | Overlay toggles (tagged faces, feature-curve polylines, boundary-face edges) |
| `filters` | AND-composed, kind-tagged; see below |
| `highlight_points` | World-space points drawn as always-on-top crosses (e.g. coordinates from a verification report) |
| `projection`, `perspective_fov_degrees`, `camera_distance`, `fit_padding` | Camera model, shared with the STL `render` pipeline |

### Filters

```yaml
filters:
  - { kind: cell_kind, values: [0] }          # 0 tets, 1 faces, 2 curves
  - { kind: region_key, values: [1, 2] }
  - { kind: partition, values: [1] }
  - { kind: regime, values: [1, 2] }          # band / band-Steiner tets
  - { kind: component, values: [3] }          # via region/face-tag/curve tables
  - { kind: background, keep: false }         # drop background tets
  - { kind: array_range, array: aspect_ratio, min: 10.0, max: 1.0e30 }
  - { kind: bbox, min: [0, 0, 0], max: [1, 1, 0.5] }
  - { kind: clip_plane, origin: [0.5, 0.5, 0.5], normal: [0, 0, 1] }
```

Attribute filters apply to the cells that carry the attribute (a `region_key`
filter does not hide tagged faces or curves); `bbox`/`clip_plane` apply to every
cell by centroid and produce crinkle clips — interior faces of the surviving
cells are exposed automatically. A filter naming an array the VTU does not
contain fails with an error that names the array and its producer.

### Typical diagnostic setups

Transparent background matrix with opaque interfaces and curves:

```yaml
color_by: region_key
opacity_overrides: { "0": 0.12 }
show_faces: true
show_curves: true
```

Quality triage (after `mesh-verify --annotate` adds quality arrays):

```yaml
color_by: min_dihedral_deg
filters:
  - { kind: array_range, array: aspect_ratio, min: 5.0, max: 1.0e30 }
```

## Verifying the output

Each view prints `[mesh-render] wrote <path>`; the run also prints the scene
statistics line (`N cells -> T triangles, S segments, M markers`). The PNGs are
RGBA; when `background` has alpha 0, exterior pixels are fully transparent
(checkable with any image tool). The integration suite
(`tests/mesh_render_tests.rs`) exercises round-trip I/O, extraction counts,
filter errors, analytic transparency compositing, named views, and this
pipeline end-to-end. GA-5 adds committed CPU PNG baselines in
`data/fixtures/meshgen/render_baselines/cpu/` for `good_cube.vtu` across four
diagnostic variants and all ten named views. Normal test runs compare against
those files; regenerate them only deliberately:

```bash
RUSTMSPT_UPDATE_RENDER_BASELINES=1 cargo test --test mesh_visual_regression_tests
```

The GPU baseline test compares opaque variants against the CPU reference and
skips when no adapter is available. Transparent GPU output is excluded by
design because the CPU path is the exact transparency reference.
