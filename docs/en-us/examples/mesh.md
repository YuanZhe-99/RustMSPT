# `mesh` - Generate a tetrahedral volume mesh (S0+S1+G2-1..G2-5+G3+G4+G5+G6 implemented)

The `mesh` subcommand turns input STL surfaces into a contract tetrahedral
volume mesh (`PLAN_mesh_generation.md`). S0 (conditioning + repair), S1
(feature detection), G2-1..G2-3 (arrangement, overlay, degeneracy gate),
G2-5 (hybrid broad phase + box clip), G2-4 (topology rebuild + GWN), and S3
(G3-1 separation field, G3-2 thin-region segmentation and the S3<->S4 coupling
driver), S4 (G4-1 sizing field + grading), S5 (G4-2 balanced background lattice) and
S6 (G5-1 classification), S7 (G6-1 snap) and S8 (G6-2..G6-5 cut) are implemented.
The pipeline emits `s02_arranged` after box clipping and topology rebuild,
`s03_gapfield` after S3, `s04_sizing` after S4, `s05_lattice` after S5,
`s06_classified` after S6, `s07_snapped` after S7 and `s08_cut` after S8, then
returns `NotAvailable` for S9..S11.

**Expect a staircased mesh until S8; `s08_cut` is the one that fits.** This is an immersed-boundary mesher: the
lattice is a *background* grid that ignores the geometry, S6 decides which cells are
inside what, S7 moves the few hundred nodes that sit on a feature or in the cut's
way, and only S8 (cut) makes the boundary follow the input. The sibling `mesh-render` and
`mesh-verify` subcommands consume contract-complete VTUs and snapshots.

See [`reference/meshgen.md`](../reference/meshgen.md) for the full config
contract, defaults, and parse-time rejects.

## Run

```bash
./target/release/rustmspt mesh --config data/input/meshgen_config.yaml
# overrides:
./target/release/rustmspt mesh --input data/input/particles.stl --output data/output/mesh.vtu
```

`--input` replaces `meshgen.inputs` with a single STL; `--output` replaces
`meshgen.output.vtu`.

## Current default run

```
$ ./target/release/rustmspt mesh --config data/input/meshgen_config.yaml
[mesh] config validated; 1 input(s):
  - data/input/particles.stl (priority 0, kind auto)
  domain [0,0,0] - [1,1,1]
  sizing h_max_frac=0.05 h_min_frac=0.002 chord_error_frac=0.2 feature_angle_deg=45
  gaps t_layer_factor=1 t_sheet_factor=0.2 confidence_min=0.9
  envelope eps_frac=0.0001 repair=Conservative coincidence=Merge fem_profile=Implicit determinism=Strict snapshots=Key
  output vtu=data/output/mesh.vtu abaqus=- report=data/output/mesh_verification
[S0] conditioned: 2830 -> 2830 verts (0 welded), 5600 faces, 15 components
[S1] features: 9 curves, 0 junctions, 0 corners
[S2/G2-1..G2-3] arrangement: 25194 candidates, 0 proper intersections, 0 coplanar overlays, 2830 registry vertices, 0 segments, 5600 faces, 25131 point features, 25131 coincidence events, 0 degraded neighborhoods
[G2-5b] box clip: 5600 -> 0 faces, 9 -> 0 curves
[G2-4] topology rebuild: 0 solid (0 closed, 0 defective), 0 sheet, 0 closure defects
[mesh] snapshot s02: data/output/mesh.debug/mesh_s02_arranged.vtu
[S3/G3-1] gap field: 0 samples (0 ray-paired, 0 closest pairs, 0 densified), 0 groups, t_sheet=0.017321 t_layer=0.086603
Error: Algorithm not available yet: mesh: stages S9..S11 are not implemented yet; S0+S1, G2-1..G2-5 arrangement+clip+topology, G3 gap field + regimes, G4-1 sizing field + grading, G4-2 balanced lattice, G5-1 classification, G6-1 snap, and G6-2/G6-3 cut complete; 1 input(s) loaded, output 'data/output/mesh.vtu'
```

The 5,600-triangle sample runs through the full G2 pipeline: the hybrid broad
phase (median-extent uniform grid) handles the candidate generation without the
former 50x work guard. Box clipping drops all faces because the particles.stl
coordinates [0,100] do not match the domain [0,1]; use an input whose vertices
are within the configured domain for non-empty output (with no surviving face there
is nothing for S3 to sample, hence the zero-sample gap field). The exit status is
nonzero because S9..S11 are not yet implemented.

`snapshots: all` emits `s00_conditioned`, `s01_features`, `s02_arranged`,
`s03_gapfield`, `s04_sizing`, `s05_lattice`, `s06_classified`, `s07_snapped` and `s08_cut` under
`<output_stem>.debug/`; `snapshots: key` emits
`s02_arranged` only (the frozen key set is `{s02,s05,s08,s11}`). Each input STL is preloaded before the
plan prints, so a missing or unparseable file fails fast:

```
$ ./target/release/rustmspt mesh --config /tmp/bad_meshgen.yaml
Error: Invalid config: meshgen.inputs: failed to read STL 'data/input/__missing__.stl': No such file or directory (os error 2)
```

## Parse-reject example

A config violating the PLAN §6.3 domain-positivity rule is rejected before any
STL is read:

```
$ cat /tmp/bad_meshgen.yaml
meshgen:
  inputs:
    - stl: data/input/particles.stl
  domain: { min: [0,0,0], max: [1,0,1] }
  output: { vtu: data/output/mesh.vtu }
$ ./target/release/rustmspt mesh --config /tmp/bad_meshgen.yaml
Error: Invalid config: meshgen.domain is nonpositive on axis 1: min 0 must be < max 0
```

The full reject set (domain positivity, `t_sheet_factor < t_layer_factor`,
`eps_frac < 0.5 * t_sheet_factor * h_min_frac`, duplicate component overrides) is
documented in [`reference/meshgen.md`](../reference/meshgen.md) and covered by
`tests/meshgen_config_tests.rs`.

## Inspecting the separation field

With an input that lies inside the configured domain, S3 reports its samples,
pairings, and any group below `gaps.confidence_min`:

```
[S3/G3-1] gap field: 7078 samples (1159 ray-paired, 22748 closest pairs, 0 densified), 25 groups, t_sheet=0.017321 t_layer=0.086603
[S3/G3-2] regions: 391 total (1 sheet, 2 band, 388 skipped)
[THIN] region 0 (Inter(1, 2), component 1 side 1 vs patch 1) -> Band: t_r=0.020000, confidence 1.000, 355 samples, 1 rim loop(s)
[THIN] region 212 (Intra(3), component 3 side -1 vs patch 2) -> Sheet: t_r=0.008000, confidence 1.000, 350 samples, 1 rim loop(s), mid-surface 72 triangles
[THIN] region 249 (SheetSheet(4, 5), component 4 side 1 vs patch 5) -> Band: t_r=0.019985, confidence 1.000, 825 samples, 1 rim loop(s)
[S3/S4] coupling: 1 iteration(s), converged=true, h=0.086603, t_sheet=0.017321, t_layer=0.086603
[mesh] snapshot s03: out/mesh.debug/mesh_s03_gapfield.vtu
```

Each converted region names its pair class, its frozen `t_r`, its confidence and
its rim count; a `Sheet` region also reports the mid-surface it built. Regions that
declared a thin regime but did not survive a gate are aggregated by reason so a
coarse input cannot bury the interesting ones in a wall of WARNs:

```
[THIN-SKIP] WARN: 388 region(s) declared thin but kept volumetric (Speck)
  region 1 (Inter(1, 2), component 1 side 1 vs patch 1) declared Band: t_r=0.053333, confidence 1.000, 1 samples, failed checks 1-5 [0, 0, 0, 0, 0]
  ... and 383 more
```

`t_r` and the thresholds are printed in model units. The `s03_gapfield` snapshot
carries the field as the `separation_t` point array, which `mesh-render` colours
directly:

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s03_gapfield.vtu
  color_by: separation_t
  scalar_min: 0.0
  scalar_max: 0.03
```

The regime outputs render the same way: `color_by: thin_role` separates ordinary
walls from converted ones and from mid-surface faces, `color_by: band_region`
colours each region, and an `array_range` filter isolates the mid-surface alone:

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s03_gapfield.vtu
  color_by: thin_role
  filters:
    - { kind: array_range, array: thin_role, min: 2.0, max: 2.0 }
```

## Inspecting the sizing field

S4 reports its sources, the constraint the coupling loop settled on, and the
octree it built:

```
[S4/G4-1] geometry sources: 1284 total (1208 curvature, 68 feature curve, 8 corner)
[S3/S4] coupling: 2 iteration(s), converged=true, h=0.030000, t_sheet=0.006000, t_layer=0.030000
[S3/S4] constraint bound by region 0 (local feature size): C=0.030000, t_r=0.060000, 355 sample(s)
[S4/G4-1] sizing field: 4126 sources (+2842 LFS), 38104 leaves, levels 0..7, h in [0.002000, 0.050000], grading 2.00
[mesh] snapshot s04: out/mesh.debug/mesh_s04_sizing.vtu
```

The `constraint bound by` line is the one to read when a mesh comes out finer than
expected. `C(R)` is a single scalar for the whole model, so exactly one region sets
the regime thresholds; the line names it, says whether curvature/features or local
feature size bound it, and reports the region's separation and sample count. A
one-sample region with a tiny `t_r` there is a measurement worth distrusting - which
is why a region S3 itself declined is excluded from the term entirely.

`s04_sizing` stores the field as `sizing_h` on the corners of a `cell_kind = 3`
voxel preview, so `mesh-render` colours it like any other field. A clip plane is
what makes it readable - it cuts into the octree and exposes the interior grading
instead of showing you the outer shell:

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s04_sizing.vtu
  color_by: sizing_h
  filters:
    - { kind: clip_plane, origin: [0.5, 0.5, 0.5], normal: [0.0, 1.0, 0.0] }
```

What to look for: concentric bands of decreasing size around every curved patch,
feature curve and volumetric gap, each band roughly one element wide (that is the
`grading: 2.0` gradation - the size may at most double over one element), and a flat
`h_max` plateau everywhere else. A field that is uniformly at `h_min` means a source
is asking for more than it should; a field that never leaves `h_max` means no
criterion fired, which for a curved input usually means `chord_error_frac` is too
loose to bite before the ceiling.

## Inspecting the background lattice

S5 reports the balance pass, the template split and the element budget:

```
[S5/G4-2] lattice: 168841 leaves after 1095 balance split(s) (140497 Freudenthal, 28344 fan), 297771 nodes, 1651366 tets, V in [1.242e-3, 4.069e1]
[mesh] snapshot s05: out/mesh.debug/mesh_s05_lattice.vtu
```

`s05_lattice` is a **key** snapshot, so it is written by default. It is a real tet
mesh, which means the verifier's conformity section applies to it directly - and
that is the check worth running whenever the lattice code is touched:

```bash
rustmspt mesh-verify --config verify.yaml
```

```
[PASS] [V3] Conformity
       interior_faces=3278560  boundary_faces=48344  multi_shared_faces=0  boundary_leaks=0  hanging_nodes=0  non_manifold_edges=0
[PASS] [V4] Quality
       worst_aspect_ratio=1.60517  min_dihedral_deg=35.26439  aspect_ratio_over_gate=0  below_low_dihedral=0
```

`multi_shared_faces = 0` and `hanging_nodes = 0` are Theorem T1 - the lattice is
conforming with no hanging nodes by construction. `min_dihedral_deg = 35.264` is
`arctan(1/√2)`, the worst transition-fan element; it is the expected value, not a
warning sign.

To look at where the level jumps are and what they cost, annotate the mesh with the
per-tet quality arrays and colour by them - a sliver band at a transition would show
up as a connected dark region, and there should not be one:

```yaml
mesh_verify:
  input: out/mesh.debug/mesh_s05_lattice.vtu
  annotate: out/lattice_annotated.vtu
```

```yaml
mesh_render:
  input: out/lattice_annotated.vtu
  color_by: min_dihedral_deg
  scalar_min: 30.0
  scalar_max: 50.0
  wireframe: true
  filters:
    - { kind: bbox, min: [0.0, 0.49, 0.0], max: [1.0, 0.51, 1.0] }
```

## Inspecting the classification

S6 reports what it decided and, just as usefully, *how*:

```
[S6/G5-1] classification: 47400 vertices x 3 solid component(s) (0 sheet, 0 defective), 19195 inside pair(s); tets 136468 background / 89718 owned / 42762 straddling; 4 region key(s); 0 inactive face(s)
[S6/G5-1] rays: 142200 first-shot, 0 re-shot, 0 exhausted (winding-number fallback), 0 by winding number on a defective component (by design), 13758 exact-predicate escalation(s)
```

The second line is the one to watch. `first-shot` should be essentially everything;
`re-shot` counts rays that passed exactly through an edge or a vertex; and
`exhausted` counts decisions that used up all five directions, which should be zero.
A non-zero one means the geometry is degenerate in a way the ray battery cannot
resolve - not that the answer is wrong, the winding number still decides it - but it
is worth looking at. The separate `by winding number on a defective component` count
is **not** a fallback: S2b could not certify that component closed, so parity is not
defined for it and the winding number is the correct path.
`exact-predicate escalation` is just the static filter handing off to the exact
predicate; on lattice-aligned input it is expected to be large.

`s06_classified` carries the resolved `region_key`, so hiding the background shows
which cells each component claimed:

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s06_classified.vtu
  color_by: region_key
  filters:
    - { kind: background, keep: false }
```

The result is a **staircased** version of the input - accurate to one element,
because no vertex has moved yet. That is what S7 and S8 fix. `color_by: arbitrated`
shows the straddling shell they will work on.

## Inspecting the snap

S7 prints what it moved and what it left for S8:

```
[S7/G6-1] snap: 23766 crossing(s) on 23610 of 317883 edge(s); 12407 candidate(s) -> 33 corner / 272 curve / 0 surface; 1354 on-cut node(s)
[S7/G6-1] moves: 0 capped, 0 rejected, 0 box-constrained, 0 promoted by the re-check (0 residual); motion max 8.839e-3, mean 2.056e-3
[CUT-CASE] 156 edge(s) are crossed more than once by one component (invariant K1); S8 must refine or escalate them
```

Read it as follows. **Crossings** are where S8 will cut; **on-cut nodes** are lattice
vertices that already lie on a patch, so the cut passes through them instead. The
three snap counts are the frozen priority - a node takes the highest-ranked target
within `0.3 * L_min` of it, and a *surface* target only where the cut would otherwise
pass within 2.5 % of the node (see `reference/meshgen.md`, "why surfaces are not a
capture target"). `capped` and `rejected` are ARB-11 and ARB-10: a move clamped at
the cap leaves the node under-snapped for the [V5] gate, and a move that would invert
an incident tet is refused outright. Both being zero, as here, means every target was
reachable. `residual` counts crossings still sitting within 2.5 % of an edge end
after the pass; it should be zero.

The `[CUT-CASE]` line is invariant K1: an edge a single component crosses twice
(a plate thinner than one element does this) cannot be cut by the single-patch
tables, and S8 must refine or escalate those cells.

`s07_snapped` carries `constraint_kind` per node - `0` free, `1` surface, `2`
polyline, `3` corner, `4` box face - plus `snap_motion`, the distance each node
moved:

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s07_snapped.vtu
  color_by: snap_motion
```

Expect it to be almost entirely zero: S7 captures features, it does not deform the
mesh to the shape. The shape arrives with S8.

## Inspecting the cut

S8 is the stage that makes the mesh fit:

```
[S8/G6-2] cut: 34897 of 268948 cell(s) cut (383 A / 762 B / 22641 C / 11111 D; 0 by a welded sheet), 23610 cut node(s), 268948 -> 408518 tets, 46008 interface face(s)
[S8/G6-2] quality: min dihedral 0.597 deg, worst volume error 9.519e-15; 1423 cell(s) escalated
[JCT-FALLBACK] 1423 escalated cell(s) re-meshed as a conforming centroid fan (972 face(s) off the frozen table); their material boundary is chamfered by at most one cell
[CUT-3EDGE] 235 cell(s) have an edge one component crosses more than once (invariant K1)
[CUT-CASE] 1188 cell(s) have a side/crossing configuration that is not a legal §6 row
```

The **A/B/C/D counts** are the frozen §6 cases, by how many of the parent's four
nodes were inside the patch. The **worst volume error** is the guarded dry-run's
headline number: the pieces of every cut must add up to the parent to within 1 %, and
9.5e-15 says they add up to rounding. **Escalated** cells are the ones §6 could not
take - two patches, an edge crossed twice, or a state the table does not cover - and
they are *not* skipped: they are re-meshed as a conforming centroid fan, which keeps
the mesh valid and conforming and chamfers their material boundary by at most one
cell.

Every snapshot is written as a **pair** (P-2.1). `mesh_s08_cut.vtu` is the mesh: tets
only, with `region_key` on cells that now follow the geometry. Opened directly in
ParaView its only feature edges are the domain box, which is the point of it having the
plain name. `mesh_s08_cut_contract.vtu` beside it is the mixed-cell contract document —
the same tets plus tagged interface triangles (`cell_kind = 1`) with their
`(inside, outside)` element pairs, and the rim polylines — and it is what to open when
you need the tags, or to hand `mesh-verify` when you want the whole catalog, since
`[V5]`–`[V9]` read them. Both share one point array.

```yaml
mesh_render:
  input: out/mesh.debug/mesh_s08_cut.vtu
  color_by: region_key
  filters:
    - { kind: background, keep: false }
```

Unlike `s06`, this one is round where the input is round. Verify it with
`mesh-verify`: `[V3]` should report zero hanging nodes, zero non-manifold edges and
zero boundary leaks. `[V4]` **will** warn - cutting produces slivers by construction,
and those elements are the input budget for S9's quality stage, not a defect.
