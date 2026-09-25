# Mesh Generation - the `mesh` Pipeline Config and Stages

Reference for the config + CLI + pipeline of the `mesh` subcommand
(`PLAN_mesh_generation.md` phases G1-1 through G3-3): the YAML config block
(`src/config/meshgen.rs`), the pipeline wrapper (`src/pipeline/meshgen.rs`),
the S0 conditioning stage (`src/meshgen/surface.rs`), and the S1 feature
detection stage (`src/meshgen/features.rs`), exact/staged constructions
(`src/meshgen/predicates.rs`), registry-owned corefinement/coplanar overlay
(`src/meshgen/arrange.rs`), the post-clip topology rebuild
(`src/meshgen/topo.rs`), the S3 separation field and thin-region
segmentation (`src/meshgen/gapfield.rs`), and the S3<->S4 coupling driver
(`src/meshgen/sizing.rs`).

The authoritative design is [`PLAN_mesh_generation.md`](../../../PLAN_mesh_generation.md)
§6.3 (config sketch) and §10.1-10.3 (S0/S1/S2 algorithms). The schema of record
for the *output* contract lives in
[`SPEC_meshgen_contracts.md`](../../../SPEC_meshgen_contracts.md); the numerics
ordering enforced by `validate()` lives in
[`SPEC_meshgen_numerics.md`](../../../SPEC_meshgen_numerics.md).

## What runs today

S0, S1, and G2-1 through G2-5 are implemented. The
pipeline normalizes coordinates by the domain diagonal before S0, preserves
source identities through repair, detects/chains component-local S1 features,
then runs exact non-coplanar classification, C1/C2/C3 f64/DD constructions,
registry-owned coplanar overlay, all C1-C10 policy classifications, multi-tag
atomic faces, contact curves/points, typed degraded-neighborhood validation,
hybrid broad phase (median-extent uniform grid with oversized side list and
auto-switch), box clipping (solids capped with `box` tag, sheets open, curves
clipped), and post-clip topology rebuild with GWN fallback for non-closed
solids. S3 (G3-1) then measures the separation field: two-sided samples, normal
rays, the closest-pair sweep, adaptive densification, the five-check pairing
battery, and per-group confidence; G3-2 then segments thin regions with
hysteresis, applies the pairing closure, builds rims and validated mid-surfaces,
and runs the S3<->S4 fixed-point driver. G4-1 then supplies that driver's sizing
constraint and builds the graded sizing field and the background octree it lives
on, G4-2 strongly 2:1-balances that octree and tetrahedralizes it with the frozen
Freudenthal and centroid-fan templates, G5-1 classifies every lattice vertex
against every solid component and seeds the ownership records, and G6-1 snaps the
lattice onto the geometry's corners, feature curves and near-node crossings, and
G6-2..G6-5 cut it. It returns `NotAvailable` for S9 through S11. The `s02_arranged` snapshot is emitted
after box clipping and topology rebuild are complete; `s03_gapfield`, `s04_sizing`,
`s06_classified`, `s07_snapped` and `s08_cut` follow it under `snapshots: all`, and
`s05_lattice` and `s08_cut` are **key** snapshots.

**The mesh does not follow the geometry until S8.** S5 builds a *background*
lattice that ignores the input entirely; S6 decides which of its cells are inside
what, which makes the shape visible but staircased to one element; S7 snaps
vertices onto the surface and **S8 cuts the cells that straddle it**, and only then
is the boundary exact. A coarse-looking `s05`, `s06` or `s07` is the design, not a
defect - S7 moves a few hundred nodes onto features, it does not deform the mesh to
the shape. `s08_cut` is the first snapshot that fits the input.

## Index

| Item | Source | Summary |
|---|---|---|
| `InputKind` | `src/config/meshgen.rs:10` | Per-input surface role override: `auto` (default, detected from closedness) / `solid` / `sheet`. |
| `RepairLevel` | `src/config/meshgen.rs:20` | S0 repair aggressiveness: `strict` / `conservative` (default) / `permissive`. |
| `CoincidencePolicy` | `src/config/meshgen.rs:30` | G2-2 coincidence policy for overlapping surfaces: `merge` (default) / `reject` / `warn`. |
| `FemProfile` | `src/config/meshgen.rs:40` | Target solver profile controlling sheet/thin handling: `implicit` (default) / `explicit` / `none`. |
| `DeterminismMode` | `src/config/meshgen.rs:50` | Run-to-run reproducibility contract: `strict` (default, bitwise) / `fast` (best-effort). |
| `UnmappedPolicy` | `src/config/meshgen.rs:59` | INP export behaviour for regions lacking a material mapping: `error` (default) / `elset-only`; kebab-cased. |
| `SnapshotMode` | `src/config/meshgen.rs:68` | Contract snapshot emission level: `none` / `key` (default, s02/s05/s08/s11) / `all`. |
| `MeshGenInput` | `src/config/meshgen.rs:84` | One STL input: `stl` path, optional `priority` (**defaults to 0 for every input**), `kind` (default `auto`). |
| `MeshGenDomain` | `src/config/meshgen.rs:94` | Axis-aligned generation domain; `min`/`max` must each be 3-component, with `min < max` per axis. |
| `MeshGenSizing` | `src/config/meshgen.rs:107` | Sizing-field limits as fractions of the domain-box diagonal, plus `grading` (default 2.0, the 2:1 gradation) and `gap_cells` (default 2.0). |
| `MeshGenGaps` | `src/config/meshgen.rs:128` | Gap-field thickness factors (x local h(x)) and the separation confidence floor. |
| `MeshGenEnvelope` | `src/config/meshgen.rs:176` | Numerical envelope thickness as a fraction of the domain-box diagonal. |
| `MeshGenRepair` | `src/config/meshgen.rs:183` | S0 repair configuration (`level`). |
| `MeshGenMaterials` | `src/config/meshgen.rs:194` | Material assignments for the Abaqus INP export; `by_component` survives duplicate keys to `validate()`. |
| `MeshGenOutput` | `src/config/meshgen.rs:211` | Output destinations: required `vtu`, optional `abaqus`/`report`. |
| `MeshGenParams` | `src/config/meshgen.rs:225` | The `meshgen:` YAML block; call `validate()` after loading. |
| `MeshGenConfig` | `src/config/meshgen.rs:257` | Top-level YAML wrapper (`meshgen:`). |
| `MeshGenInput::resolved_priority` | `src/config/meshgen.rs:265` | Effective priority: the explicit value, else 0. Takes no file index - deriving a default from file order is the defect it replaced. |
| `MeshGenParams::validate` | `src/config/meshgen.rs:279` | Enforce the PLAN §6.3 parse-time rejects; returns `Ok(())` or `InvalidConfig`. |
| `deserialize_component_map` | `src/config/meshgen.rs:509` | Deserialize `by_component` as an ordered pair list preserving duplicate keys. |
| `MeshGenPipeline` | `src/pipeline/meshgen.rs:57` | Runs normalized S0/S1/G2-1..G2-5/S3/S4, emits s02, s03 and s04, then returns `NotAvailable` for S5..S11. |
| `ConditionedSurface` / `ConditionStats` | `src/meshgen/surface.rs:88/116` | S0 geometry, persistent source-component ids, repair log, and aggregate counts. |
| `SurfaceComponent` (`ArrangeComponent`) | `src/meshgen/surface.rs:99` | Contract component row `{X, priority Y, solid/sheet kind, closed}` shared by surface stages. |
| `condition_surface` | `src/meshgen/surface.rs:135` | Weld on `q=0.1*eps`, drop exact degenerates, dedupe per source identity, orient/repair, and derive provisional components. |
| `source_component_is_closed` | `src/meshgen/surface.rs:332` | Exact combinatorial closure test: every undirected source-component edge has incidence two. |
| `condition_surface_to_doc` / `surface_stage_to_doc` | `src/meshgen/surface.rs:301/351` | Build all always-present schema-v1 face/curve arrays and tables for s00-s03 documents. |
| `FeatureEdgeKind` / `FeatureCurve` / `FeatureSet` | `src/meshgen/features.rs:19/27/40` | Sharp/rim/non-manifold edge classes, component-aware chained polylines, junctions, and corners. |
| `detect_features` | `src/meshgen/features.rs:54` | Deterministically detect and chain component-local S1 features using algebraic dihedral/turn tests. |
| `features_to_doc` | `src/meshgen/features.rs:290` | Build a schema-v1 s01 face/curve document; corners/junctions use `constraint_kind=3`. |
| `ProjectionAxis` / `best_projection_axis` / `project_to_2d` / `orient2d_axis` / value and DD helpers | `src/meshgen/predicates.rs:54/61/74/83/92/178` | Best-conditioned 3D-to-2D projection, exact projected orientation signs, and the f64 permanent/DD values used by C3. |
| `two_sum` / `two_prod` / `DoubleDouble` | `src/meshgen/predicates.rs:111/118/125` | Frozen error-free primitives and add/sub/mul-only DD arithmetic. |
| `DeterminantRatio` / `PrecisionTier` / `ConstructionOutcome` | `src/meshgen/predicates.rs:192/229/254` | Exact-ordering ratio and methods, committed precision provenance, and resolved/deferred construction result. |
| `orient3d_value_permanent` / `orient3d_filtered` / `orient3d_dd_value` | `src/meshgen/predicates.rs:266/286/296` | Shewchuk-order f64 value/permanent, certified static filter with exact fallback, and DD determinant. |
| `construct_edge_triangle_intersection` | `src/meshgen/predicates.rs:434` | Frozen C1 determinant-ratio construction; f64/DD escalation and DD-floor routing. |
| `CoplanarSegmentPoint` / `construct_coplanar_segment_intersection` | `src/meshgen/predicates.rs:244/384` | Frozen C3 affine segment intersection; checks both defining-edge stability ratios and retains DD ordering ratios. |
| `construct_three_triangle_intersection` | `src/meshgen/predicates.rs:631` | Frozen C2 local-frame Cramer construction, fully recomputed in DD on escalation. |
| `EdgeId` / `EdgeId::new` / `IsectProv` / `SegKey` / `SegKey::new` | `src/meshgen/arrange.rs:29/33/44/52/61` | Canonical `EdgeTri`/`EdgeEdge`/`TriTriTri` and segment identities. |
| `CoincidenceCase` / policy methods / `CoincidenceEntity` / `CoincidenceEvent` | `src/meshgen/arrange.rs:73/88/97/105/112` | Typed C1-C10 classification, sorted entities/components, and frozen reject/warn semantics. |
| `DegradedReason` / `DegradedNeighborhood` / `ArrangedPointFeature` | `src/meshgen/arrange.rs:120/130/140` | Durable G2-3 fallback records and welded C5/C6 point features. |
| `ArrangeOptions` / `ArrangeOptions::new` / `with_coincidence` | `src/meshgen/arrange.rs:149/159/175` | Domain, epsilon, policy, and component table; constructor defaults to `merge`. |
| `RegistryVertex` / `RegistrySegment` / `IntersectionRegistry` | `src/meshgen/arrange.rs:183/194/203` | Symbolic-first global registry; one committed node retains every compatible provenance alias. |
| `ArrangedCurve` / `ArrangedFace` / `ArrangedSurface` | `src/meshgen/arrange.rs:218/228/256` | Atomic multi-source/multi-tag faces, curves/radial order, point features, events, warnings, and degraded records. |
| `ArrangementStats` | `src/meshgen/arrange.rs:243` | Candidate, proper/overlay/contact, f64/DD/floor, feature, and degraded counters. |
| `arrange_surface` | `src/meshgen/arrange.rs:439` | Pure deterministic CPU G2-1..G2-3 path; returns the validated arranged diagnostic complex or a policy/invariant error. |
| `arranged_surface_to_doc` | `src/meshgen/arrange.rs:804` | Encode the arranged complex with set-valued face tags, `FaceTagOrientation`, and `FaceTagKind` (`2` for box-clip caps); stamped as s02 by the pipeline. |
| `clip_arranged_to_box` | `src/meshgen/arrange.rs:4789` | G2-5b Sutherland-Hodgman clip to the domain: solids capped with box-tagged faces, sheets clipped open, curves clipped. |
| `triangulate_parent` | `src/meshgen/arrange.rs:3855` | Restricted pre-registered Spade CDT; propagates insertion/refused-constraint errors and verifies constraints/tiling. |
| `SampleKind` / `PairClass` | `src/meshgen/gapfield.rs:48/57` | Sample provenance (vertex / centroid / closest-pair / densified) and the frozen pair classes (`intra`, `inter`, `solid-sheet`, `sheet-sheet`, `surface-box`). |
| `GapPairing` / `GapSample` / `GapSample::passes_battery` | `src/meshgen/gapfield.rs:68/81/100` | One correspondence, one sample (side, direction, `t_raw`/`t`/`t_exact`, flags, smooth-patch index), and the all-applicable-checks predicate. |
| `GapGroup` / `GapFieldStats` / `GapField` | `src/meshgen/gapfield.rs:110/122/137` | Provisional (component, side, opposite patch) group with confidence and `t_r`, S3 counters, and the whole field. |
| `GapFieldOptions` | `src/meshgen/gapfield.rs:280` | Domain, epsilon, bootstrap `h`, gap factors, confidence floor, feature angle (vertex clustering), virtual walls, densification rounds, smoothing sweeps. |
| `FLAG_MUTUAL` / `FLAG_OPPOSITE_PATCH` / `FLAG_CONTINUITY` / `FLAG_NO_CROSSING` / `FLAG_ORIENTATION` / `FLAGS_ALL` | `src/meshgen/gapfield.rs:28-38` | The five battery bits and their union. |
| `compute_gap_field` | `src/meshgen/gapfield.rs:495` | Run S3 over a clipped, topology-rebuilt arranged surface; deterministic, pure. |
| `gapfield_to_doc` | `src/meshgen/gapfield.rs:3080` | Build the `s03_gapfield` document: the arranged surface plus the `separation_t` point field (`-1` = no pairing). |
| `Regime` / `SkipReason` / `MidSurfaceDefect` | `src/meshgen/gapfield.rs:166/177/190` | The three thin-feature regimes, the `[THIN-SKIP]` taxonomy, and the mid-surface validation defects. |
| `MidSurface` / `MidSurface::is_valid` / `ThinRegion` | `src/meshgen/gapfield.rs:204/216/227` | The midpoint patch with its source nodes and defects, and one segmented region (wall A faces, the closed-over opposite wall, rims, regime, confidence). |
| `validate_mid_surface` | `src/meshgen/gapfield.rs:2945` | Run the §3.4 checks (area, orientation, normal deviation, self-intersection, rim agreement, Euler) over a candidate. |
| `CouplingOptions` / `LockReason` / `CouplingReport` / `CouplingReport::locked_for` | `src/meshgen/sizing.rs:35/66/80/94` | Coupling-loop inputs, the three lock reasons, the run report, and the per-reason lock query. |
| `regime_for` | `src/meshgen/sizing.rs:143` | Classify one region against the current thresholds with the 0.9/1.1 hysteresis dead band. |
| `couple_gap_and_sizing` | `src/meshgen/sizing.rs:189` | Run the S3<->S4 fixed point to a regime assignment and a converged `h`; errors on the G-8 ordering assertion. |
| `SizingCriterion` / `SizingSource` | `src/meshgen/sizing.rs:335/351` | Which §10.6 criterion produced a sizing constraint, and the constraint itself. |
| `SizingOptions` / `beta` / `lfs_floor` | `src/meshgen/sizing.rs:363/410/429` | Sizing-field inputs; the Lipschitz constant `grading - 1`; the floor below which a separation is not a gap. |
| `curvature_sources` / `feature_sources` / `curve_sources` / `collect_geometry_sources` | `src/meshgen/sizing.rs:504/605/677` | The regime-independent criteria, read on the conditioned input surface. |
| `gap_sources` | `src/meshgen/sizing.rs:815` | Regime-aware LFS sources: `t / gap_cells` per volumetric S3 sample. |
| `SizingLookup` / `eval` / `eval_box` | `src/meshgen/sizing.rs:969/1067/1083` | The graded field; point evaluation and the exact minimum over a box. |
| `SizingLeaf` / `SizingStats` / `SizingField` / `locate` / `sample` | `src/meshgen/sizing.rs:1156/1164/1182/1220/1248` | The background octree, its build report, and point location. |
| `build_sizing_field` | `src/meshgen/sizing.rs:1416` | Refine the octree one parallel level at a time until every leaf resolves the field. |
| `SizingConstraint` / `evaluate` / `binding_region` | `src/meshgen/sizing.rs:1407/1468/1498` | `C(R)` for the coupling driver, and which region and term set it. |
| `sizing_to_doc` | `src/meshgen/sizing.rs:1699` | Encode a sizing field as the `s04_sizing` voxel preview VTU. |
| `FREUDENTHAL` / `CellTemplate` | `src/meshgen/lattice.rs:54/493` | The frozen 6-tet Kuhn table, and which template a leaf took. |
| `balance_octree` / `balance_violation` | `src/meshgen/lattice.rs:156/286` | Strong (face+edge+vertex) 2:1 balance, and a direct check of the property. |
| `Lattice` / `LatticeStats` / `LatticeOptions` | `src/meshgen/lattice.rs:520/502/531` | The tetrahedralized lattice, its build report, and the tet budget. |
| `build_lattice` / `build_lattice_with_splits` | `src/meshgen/lattice.rs:611/616` | Tetrahedralize a balanced octree with the Freudenthal and fan templates. |
| `lattice_to_doc` | `src/meshgen/lattice.rs:837` | Encode the lattice as the `s05_lattice` snapshot VTU. |
| `Side` / `Provenance` / `OwnershipRecord` | `src/meshgen/classify.rs:60/68/82` | A tet's side of a component, the entry's origin, and the sparse record. |
| `resolve` | `src/meshgen/classify.rs:180` | The frozen label rule (SPEC_meshgen_geometry §9.1). |
| `RAY_DIRECTIONS` | `src/meshgen/classify.rs:46` | The frozen re-shoot sequence (ARB-9). |
| `Classification` / `ClassifyStats` / `ClassifyOptions` | `src/meshgen/classify.rs:140/112/457` | The S6 result and how each decision was reached. |
| `classify_lattice` | `src/meshgen/classify.rs:704` | S6: parity classification, record seeding, active-patch filter. |
| `classified_to_doc` | `src/meshgen/classify.rs:1044` | Encode the `s06_classified` snapshot VTU. |
| `Stage` | `src/meshgen/snapshot.rs:22` | The frozen stage enumeration (0..=11); also the snapshot index. |
| `Stage::from_path` | `src/meshgen/snapshot.rs:102` | Parse the stage from a snapshot filename's `sNN` token (for the [V12] cross-check). |
| `should_emit` | `src/meshgen/snapshot.rs:120` | Whether a stage is emitted under `none`/`key`/`all`. |
| `snapshot_path` | `src/meshgen/snapshot.rs:147` | `<stem>.debug/<stem>_sNN_<name>.vtu` (Quality carries `_r<N>`). |
| `SnapshotMeta` | `src/meshgen/snapshot.rs:195` | Bundled stamping inputs (stage, round, config hash, domain, determinism, generator version). |
| `stamp_metadata` | `src/meshgen/snapshot.rs:232` | Stamp the full §2.4 metadata block onto a snapshot document. |
| `emit_snapshot` | `src/meshgen/snapshot.rs:295` | Stamp metadata, then write the delivered tets-only volume under the plain name and the mixed-cell contract document beside it as `_contract.vtu`; returns the delivered path. |
| `warn_if_large` | `src/meshgen/snapshot.rs:348` | Size WARN: `snapshots: all` + >5 M tets estimate. |

### G6-1 snap (S7)

| Item | Source | Summary |
|---|---|---|
| `SNAP_MOTION_CAP` | `src/meshgen/snap.rs:43` | `0.30` - no accepted move exceeds this fraction of the node's shortest incident lattice edge (ARB-11). |
| `SNAP_RECHECK_LOW` / `SNAP_RECHECK_HIGH` | `src/meshgen/snap.rs` | `0.025` / `0.975` - the re-check band: a crossing this close to an edge end promotes the endpoint instead of cutting. |
| `ALTERNATING_PROJECTION_PASSES` | `src/meshgen/snap.rs:54` | `15` - the fixed pass count of the degraded-arrangement curve target (ARB-2). |
| `WEIGHT_CORNER` / `WEIGHT_CURVE` / `WEIGHT_SURFACE` | `src/meshgen/snap.rs` | `1e7` / `1e4` / `1e0` - the frozen target priority written as numbers. |
| `TargetKind` | `src/meshgen/snap.rs:68` | What a node is bound to, in the contract's `constraint_kind` encoding: `Free` / `Surface` / `Polyline` / `Corner` / `BoxFace`. |
| `EdgeCrossing` | `src/meshgen/snap.rs:90` | One exact edge-patch crossing: the edge, the face, the component, the parameter `t`, and the constructed point. |
| `SnapStats` | `src/meshgen/snap.rs:105` | S7 diagnostics: crossings, candidates, per-kind snap counts, caps, rejections, promotions, on-cut nodes, motion. |
| `Snapped` | `src/meshgen/snap.rs:142` | S7's result: moved nodes, per-node constraints, motion, the under-snapped list, the crossings, and the on-cut set. |
| `SnapOptions` | `src/meshgen/snap.rs:173` | S7 inputs: the domain box and the weld tolerance. |
| `snap_lattice` | `src/meshgen/snap.rs:1485` | Runs S7 - feature capture, band promotion, the re-check pass, and the final crossing list. |
| `unique_edges` | `src/meshgen/snap.rs:887` | The lattice's unique edge set as ascending node pairs (parallel map, parallel sort, dedup). |
| `move_preserves_orientation` | `src/meshgen/snap.rs:957` | Exact ARB-10 test: whether moving one node keeps every incident tet positively oriented. |
| `snapped_to_doc` | `src/meshgen/snap.rs:1841` | Encodes the snapped lattice as the `s07_snapped` snapshot VTU. |

### G6-2..G6-5 cut (S8)

| Item | Source | Summary |
|---|---|---|
| `CUT_VOLUME_TOLERANCE` / `CUT_MIN_DIHEDRAL_DEG` | `src/meshgen/cut.rs` | `0.01` / `8.0` - the guarded dry-run's volume tolerance and the §4.4 runtime dihedral floor. |
| `NodeSide` | `src/meshgen/cut.rs:80` | Where a parent node sits relative to the patch: `Inside` / `Outside` / `OnCut`. |
| `Escalation` | `src/meshgen/cut.rs:91` | Why a cell could not take §6's path: junction, K1 multi-crossing, inconsistent state, dry-run failure, quality. |
| `InterfaceFace` | `src/meshgen/cut.rs:107` | One tagged cut triangle with its `(inside, outside)` element pair. |
| `CutMesh` / `CutStats` / `CutOptions` | `src/meshgen/cut.rs` | S8's nodes, tets, records, interface and escalation list; its diagnostics; its tolerances. |
| `snk_split_quad` / `snk_diagonal_is_02` | `src/meshgen/cut.rs` | Rule SNK (§4.1): a quad's diagonal is the one incident to its smallest-`NodeKey` vertex. |
| `prism_tets` / `prism_tets_with_diagonals` | `src/meshgen/cut.rs` | The frozen six-pattern prism table (§4.3); `None` only for the two cyclic sets Theorem T2 makes unreachable. |
| `FaceCutState` / `face_split` | `src/meshgen/cut.rs` | The §5.2 kirigami face-split table, including `split_R`; `None` for an illegal (dangling-cut) state. |
| `CellCut` / `cut_tet` | `src/meshgen/cut.rs` | The §6 single-patch tet case table - the pieces, their sides, and the interface triangles. |
| `orient_positively` / `guarded_dry_run` | `src/meshgen/cut.rs` | The canonical orientation fix, and §6's guarded dry-run (ARB-15). |
| `cut_lattice` / `cut_to_doc` | `src/meshgen/cut.rs` | Runs S8; encodes the result as `s08_cut`. |
| `FaceMesh` / `face_mesh` / `loop_fan` / `face_centroid` | `src/meshgen/junction.rs` | How an escalated cell's face is triangulated: the frozen table where it applies, a centroid fan of its boundary loop where it does not. |
| `FannedCell` / `fan_cell` / `cell_centroid` / `TET_FACES` | `src/meshgen/junction.rs` | The conforming centroid fan that re-meshes an escalated cell (G6-0's adopted fallback). |

## Config block (PLAN §6.3)

```yaml
meshgen:
  inputs:
    - stl: data/input/particle1.stl        # priority defaults to 0 for every input
    - stl: data/input/particle2.stl        # same rank -> their overlap keeps both X (R-A3)
    - stl: data/input/coating.stl
      priority: 1                           # explicitly outranked (R-A4)
    - stl: data/input/grain_boundary.stl
      kind: sheet                           # auto | solid | sheet
  domain: { min: [0,0,0], max: [1,1,1] }
  sizing:
    h_max_frac: 0.05        # x box diagonal (cap)
    h_min_frac: 0.002       # x box diagonal (floor)
    chord_error_frac: 0.2
    feature_angle_deg: 45.0
    grading: 2.0            # max size ratio over one element (2:1 gradation)
    gap_cells: 2.0          # elements across a gap that stays volumetric
    curve_cells: 2.0        # h_max / this along every locked curve; 1.0 disables
  gaps:
    t_layer_factor: 1.0     # x local h(x)
    t_sheet_factor: 0.2     # x local h(x)
    confidence_min: 0.9
  envelope: { eps_frac: 1.0e-4 }            # x box diagonal
  repair:   { level: conservative }          # strict | conservative | permissive
  coincidence: merge                         # merge | reject | warn
  fem_profile: implicit                      # implicit | explicit | none
  determinism: strict                        # strict | fast
  materials:
    by_component: { 1: steel, 2: pore }
    by_region_key: { "3+5": composite_A }
    unmapped: error                          # error | elset-only
  acceleration: { mode: auto }
  snapshots: key                             # none | key | all
  output:
    vtu: data/output/mesh.vtu
    abaqus: data/output/mesh.inp             # optional
    report: data/output/mesh_verification
  verify:                                    # gate overrides (shared with mesh-verify)
    max_ar_warn: 20.0
```

### Defaults

| Block | Field | Default |
|---|---|---|
| `sizing` | `h_max_frac` / `h_min_frac` / `chord_error_frac` / `feature_angle_deg` / `grading` / `gap_cells` / `curve_cells` | `0.05` / `0.002` / `0.2` / `45.0` / `2.0` / `2.0` / `2.0` |
| `gaps` | `t_layer_factor` / `t_sheet_factor` / `confidence_min` | `1.0` / `0.2` / `0.9` |
| `envelope` | `eps_frac` | `1.0e-4` |
| `repair.level` / `coincidence` / `fem_profile` / `determinism` | - | `conservative` / `merge` / `implicit` / `strict` |
| `materials.unmapped` / `snapshots` | - | `error` / `key` |
| `acceleration` | - | `AccelerationConfig::default()` (`mode: auto`) |
| `verify` | - | `VerifyGateParams::default()` (all gates at contract defaults) |

Required fields: `inputs` (non-empty), `domain`, `output.vtu`.

### Parse-time rejects (`MeshGenParams::validate`)

The PLAN §6.3 rejects are enforced in code, not assumed. `validate()` returns
`InvalidConfig` naming the violated rule for any of:

- `inputs` empty.
- `domain.min`/`domain.max` not 3-component, or `min >= max` on any axis
  ("nonpositive domain").
- `sizing`: not `0 < h_min_frac < h_max_frac`; `chord_error_frac <= 0`;
  `feature_angle_deg` not in `(0, 180)`.
- `gaps.confidence_min` not in `(0, 1]`.
- `gaps.t_sheet_factor >= t_layer_factor` (the load-bearing ordering
  `eps << t_sheet < t_layer <= h`).
- `gaps.t_sheet_factor <= 0`.
- `envelope.eps_frac >= 0.5 * t_sheet_factor * h_min_frac` (the same ordering,
  checked, not assumed - SPEC_meshgen_numerics).
- A negative `inputs[].priority` (Y is carried as `UInt32` in the contract VTU).
- A duplicate key in `materials.by_component`.

> Two `inputs` resolving to the same priority is **not** rejected, and was until
> 2026-08-07. Equal priority is the premise of R-A3: the overlap is preserved and
> carries both X. Rejecting it, together with the old file-index default, meant no
> config could ever produce a region key with more than one X - which in turn left
> `[V6]`'s "same-priority keys share one Y" clause measuring an empty set.

> **Correction to the plan sketch (G1-1):** the sketch's default
> `envelope.eps_frac: 1.0e-3` violates its own parse-time reject
> (`1.0e-3 >= 0.5 * 0.2 * 0.002 = 2.0e-4` at the default gap/sizing factors), so
> the implemented default is lowered to `1.0e-4`. This is the same kind of
> sketch/spec mismatch G0-3 caught (the `u16` table arrays) and is recorded in
> the G1-1 landing note in `PLAN_mesh_generation.md`.

`materials.by_component` is deserialized through `deserialize_component_map`
into a `Vec<(i64, String)>` rather than a `HashMap`, so a duplicate component
override survives deserialization for `validate()` to reject - a plain map would
silently keep the last entry.

### Late check (not yet wired)

INP requested with `materials.unmapped: error` fails at export if any
multi-ID region lacks a mapping. This is a *late* but pre-write check with an
actionable key list; it lands with the S11 export driver (G9-2), not in the
   currently implemented S0/S1/G2-1..G2-3 slice.

## CLI

```bash
./target/release/rustmspt mesh --config data/input/meshgen_config.yaml
# overrides:
./target/release/rustmspt mesh --input data/input/particles.stl --output data/output/mesh.vtu
```

`--input` replaces `meshgen.inputs` with a single STL entry (priority 0, kind
`auto`); `--output` replaces `meshgen.output.vtu`.

## Pipeline behaviour

`MeshGenPipeline::run`:

1. `MeshGenParams::validate()` - enforce the parse-time rejects.
2. Preload every input STL via `load_stl`, failing fast on a missing or
   unparseable file (the error names the offending input path).
3. Print the resolved plan (inputs with resolved priorities/kinds, domain,
   sizing, gaps, envelope, repair, coincidence, fem_profile, determinism,
   snapshots, outputs) to stdout.
4. Normalize every coordinate to `(p-domain_min)/domain_diagonal`; only snapshot
   writing applies the inverse transform. This makes S0 decisions translation
   and scale invariant.
5. Run S0 and S1. `snapshots: all` emits verifier-clean s00/s01; `key` emits
   neither because the frozen key set begins at s02.
6. Run G2-1..G2-3: exact non-coplanar corefinement, C3 coplanar overlay,
   coincidence policy, calibrated q/epsilon routing, and typed degraded fallback.
7. Run G2-5b box clip and G2-4 topology rebuild, then emit `s02_arranged`
   (`key` and `all`).
8. Run S3 (G3-1) and emit `s03_gapfield` under `all`. Every S3 length is computed
   in the normalized frame and reported in model units (`x domain diagonal`);
   groups below `gaps.confidence_min` print a `[THIN-SKIP]` WARN with the
   per-group failing-check histogram.
9. Segment thin regions (G3-2), report every converted region as `[THIN]` and every
   gated one as an aggregated `[THIN-SKIP]` WARN with its per-check histogram, then
   run the coupling driver and report its iterations, `h`, and any lock.
10. Return `Err(NotAvailable)` naming S4..S11, so a partial run cannot be mistaken
    for a contract-complete mesh.

See [`mesh.md`](../examples/mesh.md) for captured output of the current partial run and a
parse-reject run.

## G2-1 through G2-3 arrangement

Registry identity is symbolic before geometry is quantized. `EdgeTri` keys use
stable, sorted post-weld source vertex ids plus a stable triangle id;
`EdgeEdge` sorts both source edges and `TriTriTri` sorts all three triangle ids.
Requests are collected in ordered containers and ids are assigned only in
canonical-key order. Provenances sharing one committed `NodeKey` remain visible
as aliases on one `RegistryVertex`; non-identical coordinates also produce a
typed G2-3 ambiguity record.

Every topology sign uses the project exact wrappers. C1, C2, and C3 first evaluate in
f64 and compare their stability ratio with `kappa_esc`; unstable constructions
are rebuilt in DD and the selected-tier rho is derived from the DD
denominator/permanent, not the f64 ratio, so catastrophic f64 cancellation that
resolves above the DD floor is never falsely deferred. The DD floor is
`kappa_esc * u_dd/u` (numerics spec rev 1.1), approximately `3.5e-26` under the
implemented `eps_frac=1e-4` default. C3 checks the stability ratio on both
defining edges independently, constructs one affine 3-D point, and retains both
DD ratios for edge ordering. C2 translates to its first defining vertex and
recomputes differences, normals, right-hand sides, Cramer determinant, and
numerators in DD before the final f64 divisions.

Coplanar pairs share one pre-registered atomic constraint complex. Exact C3
crossings, contained vertices, collinear contacts, and retessellated coincident
patches are split before restricted CDT. Geometrically identical atoms merge
once with all source triangles, component tags, and per-tag orientation. C3
intersection curves are emitted only on true shared/exclusive atomic-face
boundaries, not source triangulation seams; promoted C1/C2 patches have no C3
curves. Provenance-scoped subdivision ensures only symbolically incident nodes
alter a source edge or contact. C1/C2 promotion is patch-local, so disconnected
exclusive geometry or a separate C7 near-coincident patch for the same component
pair does not block promotion elsewhere. C1 precision-floor degraded records
retain the deferred provenance and edge target points for S7. EdgeEdge alias
degradation retains all non-normative owner TriIds. C4 curves and C5/C6 points
are embedded into every incident source face. The frozen policy rejects only
C1/C2/C3/C7/C8/C9; `warn` produces merge-identical output and the pipeline
aggregates one warning line per case. C10 is classified using exact
separating-axis orientation signs for positive-area overlap with the bounded
domain-face rectangle, but its box clipping, `box` tag, and box-clip curve
remain G2-5 work.

G2-3 freezes C7 as mutual one-to-one triangle-vertex matching by
`distance^2 <= epsilon^2`. Complete-link clusters must retain diameter
`<= epsilon`, preventing transitive over-merging; the representative comes from
the lowest canonical triangle/NodeKey rank. DD-floor, q-order, collapsed-contact,
residual-crossing, and radially-coplanar cases remain explicit
`DegradedNeighborhood` records carrying source ids, points, provenance, and rho
for the later S7 alternating-projection fallback.

Each affected source triangle is triangulated independently with `spade` 2.15.1.
Only pre-registered points are inserted and only `try_add_constraint` is used;
point-creating/panicking constraint APIs are forbidden. Validation proves each
registry edge appears on both sides of each owning source triangle, constraints
survive, split-edge chords cannot bypass intermediate registry nodes, children
are nondegenerate and area-tile the parent, curves/points are embedded, and no
unrecorded proper or coplanar overlap remains. Feature curves are split at every
atomic face node. Every
proper intersection segment records a four-child cyclic radial order using an
exact orientation about the canonical segment direction.

Intentional later-task boundaries:

- G2-4: authoritative patch/component topology rebuild, closure, and GWN.
- G2-5: box clipping and hybrid production broad phase.

Acceptance is in `tests/meshgen_arrange_tests.rs` (43 tests plus 2 private unit
tests), including all ten policy rows across all three modes, retessellated
C1/C2 patches, partial/multiway overlay, endpoint-plus-interior contact, C7
epsilon/tilt/transitive-cluster cases, Spade refusal/insertion propagation,
cancelled-determinant DD resolution, provenance-scoped subdivision, patch-local
C7 coexistence, symbolic registry ordering, EdgeEdge owner retention, S1 curve
ownership, exact bounded C10 overlap, 100,000 near-degenerate predicate
comparisons, and 250 arrangement cases run twice. The measured G2-3 corpus result
is 250/250 deterministic, 500 DD escalations, zero floor routes, zero degraded
cases, and zero hard failures; the frozen totals are asserted in-test and
dedicated fixtures separately exercise every typed fallback trigger.

## Snapshot framework (GA-4)

`src/meshgen/snapshot.rs` implements the contract snapshot workflow frozen in
`SPEC_meshgen_contracts.md` §2.4/§3. Stage drivers (G1-2 onward) call
`emit_snapshot` at their boundaries; `mesh-verify` parses the stage from a
snapshot filename and cross-checks it against `StageIndex` via [V12].

| Stage | Index | Name | `key`? |
|---|---|---|---|
| Conditioned | 0 | `conditioned` | no |
| Features | 1 | `features` | no |
| Arranged | 2 | `arranged` | **yes** |
| Gapfield | 3 | `gapfield` | no |
| Sizing | 4 | `sizing` | no |
| Lattice | 5 | `lattice` | **yes** |
| Classified | 6 | `classified` | no |
| Snapped | 7 | `snapped` | no |
| Cut | 8 | `cut` | **yes** |
| Thin | 9 | `thin` | no |
| Quality | 10 | `quality_r<N>` | no |
| Final | 11 | `final` | **yes** |

- **Naming:** `<output_stem>.debug/<stem>_sNN_<name>.vtu`. `Quality` carries the
  IQD round (`s10_quality_r<N>`), emitted once per round.
- **Mode gating** (`should_emit`): `none` emits nothing; `key` emits {s02, s05,
  s08, s11}; `all` emits every stage.
- **Metadata** (`stamp_metadata`): writes the full §2.4 block - `SchemaVersion`
  (1), `StageIndex`, `GeneratorVersion` (3-tuple semver), `ConfigHash` (u64),
  `DomainMin`/`DomainMax`, `Counts` (recomputed from the mesh), `DeterminismMode`.
  Every snapshot carries the full block, so verifier and renderer accept any
  snapshot interchangeably. `ConfigHash` covers every effective field; unordered
  region-material mappings are sorted before hashing.
- **Surface stages** (s00-s03) emit face/curve cells only; s04/s05 previews write
  `cell_kind = 3` voxel cells with `sizing_h` (enforced by the producer, not the
  framework). `mesh-verify` skips [V7]/[V8] for s00-s03 because those checks
  require volume cells and partitions.
- **Size WARN** (`warn_if_large`): `snapshots: all` combined with an estimate
  above 5 M tets prints a WARN before the run (≈60 B/tet per volume snapshot).
- **[V12] cross-check** (T-C6): `Stage::from_path` parses `sNN` from a snapshot
  filename; `mesh-verify` passes it as `VerifyOptions::expected_stage`, and [V12]
  FAILs when `StageIndex` disagrees with the filename, when `StageIndex` is
  outside 0..=11, or when `SchemaVersion != 1`.

Acceptance (`tests/meshgen_snapshot_tests.rs`): a fixture pipeline emits the full
s-series from `good_cube.vtu`; every snapshot reloads, validates, verifies (no
FAIL, filename cross-check active), and renders via `build_scene`.

The live mesh pipeline emits s00/s01/s03 under `all` and s02 under `key` and
`all`. The fixture-only snapshot-series test additionally exercises every frozen
stage name and metadata contract.

## G3-1 separation field (S3)

`src/meshgen/gapfield.rs` implements PLAN §10.5 over the clipped, topology-rebuilt
arranged complex. `compute_gap_field` is pure and deterministic: triangles follow
arranged-face order, every candidate list is sorted before use, and smoothing is
Jacobi rather than Gauss-Seidel so no result depends on traversal order.

**Sampling.** One centroid sample per face, and at every `(component, vertex)` one
sample per **smooth patch** meeting there - the faces incident to a vertex are
clustered by normal agreement within `feature_angle_deg`, and each cluster fires
along its own area-weighted normal. A single averaged normal is wrong at a sharp
vertex: a box corner belongs to three walls, and their average points into none of
them, sending the ray diagonally out through a side face and twisting the
orientation check. Every sample is emitted on **both sides** of the surface. the reference mesher samples
only the calibrated inward side; the generalization is required here because a thin
region may be material (a thin wall, `intra(X)`) or void (a gap between two
components, `inter(X_a, X_b)`), and R-B1 must decide about both. The six domain
faces join the query set as virtual walls (`surface-box`), with inward normals.

**Rays.** From `p + 1e-3*h*d` along the side direction, length `2*t_layer`,
excluding every triangle incident to the sample's own point. A hit is accepted
when `|d . n_hit| >= cos(60 deg)`; when both walls belong to closed solids the
material-side signs must also oppose, which is what distinguishes a real gap from
the far side of the same wall. Closed-solid orientation is fixed once per
component from the sign of its own signed volume - S0 makes winding consistent
within a patch but does not fix its global sign.

**Closest-pair sweep.** Every triangle pair within `t_layer` (uniform grid, AABBs
expanded by `t_layer`) gets an exact closest pair over the 9 edge-edge and 6
vertex-face candidates. Pairs that share an arranged node are skipped: after
corefinement those touch exactly and are contacts, not gaps. Pairs within one
component must additionally *face* each other, otherwise the sweep would report a
curved patch's own tessellation spacing as a gap. One seed is kept per
`(triangle, opposite component)` - its closest approach - so a corner triangle
does not seed one duplicate sample per triangle it can see.

**Densification and smoothing.** Adjacent samples whose `|grad t|` exceeds `0.5`
get a midpoint sample (at most 3 rounds, never below `2*eps`); the ray field is
then smoothed by 3 Jacobi sweeps `t <- 0.5*t + 0.5*mean(finite neighbours)`.
Smoothing touches `t` only: `t_raw` and `t_exact` are frozen measurements.

**Battery and confidence.** The five checks set one bit each - mutual-ray
consistency, resolved opposite patch, pairing continuity within the opposite
wall's 2-ring, no local crossing of adjacent correspondence segments, and
orientation preservation. A check that cannot apply to a sample (continuity
against a virtual wall, orientation without a full vertex triple) is removed from
`applicable` rather than silently passed. Samples are grouped by
`(component, side, opposite patch)`; group confidence is the fraction passing every
applicable check, and `t_r` is the group's exact closest-pair distance when the
sweep measured one (Rule S3-M), else its smallest ray separation.

Two deviations from the reference thin-feature design §3.3, both recorded here because they change a
written rule:

- Check 1 accepts *any* sample of the opposite wall within `0.5*h` of `q` that
  measures the same separation and pairs back. the reference mesher tests only the single nearest
  sample; coincident closest-pair seeds make "nearest" ambiguous, and one
  arbitrarily chosen duplicate must not decide the check.
- Rays are cast on both sides of every surface (above), so `n_in` calibration by
  point location is not needed and the intra/inter distinction comes from the pair
  class rather than from which side was sampled.

**Snapshot.** `gapfield_to_doc` emits the arranged document plus a `separation_t`
point array (schema v1 §2.2, `Dbg`): per arranged vertex, the smallest finite
separation of any sample anchored at that vertex or on an incident face, with
`-1` for "no pairing here" (the contract's not-applicable convention for an
otherwise non-negative quantity). The pipeline converts it to model units before
writing. `mesh-render` colours by it directly (`color_by: separation_t`): point
arrays reduce to the mean of a cell's non-sentinel point values.

Acceptance (`tests/meshgen_gapfield_tests.rs`, 13 tests): an isolated solid pairs
nothing; a box corner resolves into three smooth patches, each firing along its own
wall normal; two solids 0.02 apart measure within 5%; a thin slab pairs its own two
faces through the material; a branching throat splits into two opposite patches; a
vertical plate above a horizontal one - where every normal ray runs parallel to the
other wall - is found by the sweep alone; a curved 0.02 gap measures within 10%;
virtual walls can be measured and disabled; the field is bit-identical across runs;
and the s03 snapshot validates, verifies, and carries `separation_t`.

## G3-2 thin regions and the S3<->S4 loop

**Segmentation.** Within one group - one wall side facing one opposite patch - a
deterministic ordered BFS seeds below `0.9 * threshold` and grows below
`1.1 * threshold`, `Sheet` first, then `Band` (the reference thin-feature design §3.3). Growth cannot
cross groups, so a branching throat stays two candidate regions.

**Pairing closure.** The sample each member points at joins the same region, and a
sample belongs to at most one region across every group. One gap therefore yields
**one** region owning both of its walls, not two mirror regions - and one
mid-surface rather than two coincident copies of the same sheet. `faces` is wall A
(the wall the region grew from, whose triangulation the mid-surface reuses);
`opposite_faces` is the wall that joined through the closure.

**Growth stays inside its own regime interval.** A `Band` region admits a sample
only when its **frozen** separation (the closest-pair measurement that becomes
`t_r`) is above `1.1 * t_sheet`, and the pairing closure applies the same floor.
the reference design's "LAYER seeds ... (not in SHEET)" is a test on the value, not merely on what
an earlier pass claimed: without it one region can span from below `t_sheet` up to
`t_layer`, a separation no single band template could mesh. Growth *connectivity*
still follows the smoothed field.

**Contacts are not gaps.** Two rules keep the neighbourhood of a contact out of the
regimes. `reject_geodesic_shortcuts` clears any correspondence - ray or sweep -
whose two triangles are also close *along the surface*: a gap is a straight line
through free space, so a short surface path means the segment is a shortcut through
the geometry. Face adjacency deliberately spans components, because corefinement
makes two intersecting surfaces share edges along their curve. Then a region is a
`IntersectionWedge` skip when it touches that component pair's **contact faces** -
the union of the faces incident to its S2 intersection curve and the faces whose own
pairing was cleared as a shortcut, dilated by one face ring (the shortcut pass
already clears the pairings *at* the contact, so a wedge region starts one ring out).
Both sets are keyed by component pair, so one pair's contact can never disqualify
another pair's gap.

Why not distance alone: around a **shallow** crossing the surface detour is as long
as a genuine thin wall's, so no geodesic ratio separates them. The exact fact that
does is the contact itself, which S2 already computed.

**Touching the contact set is necessary and nowhere near sufficient.** On a limb
attached to a block every triangle of the limb is incident to the root curve, so
taking the contact test alone declared the limb's own uniform gap to be the curve's
neighbourhood and skipped **all 19** of the sheet fixture's regions - the whole thin
path, shut by a body being small. What makes a wedge a wedge is that the gap
*closes*, so the region must also straddle the sheet threshold:

```text
IntersectionWedge  <=>  reaches the contact set  AND  t_r < t_sheet < t_max
```

`t_r` is the narrowest separation any member sample reports and `t_max` the widest.
A wedge has one end to be collapsed and the other to be meshed, and no single
template does both; a thin feature holds both on one side of `t_sheet` and is
meshable by exactly one row. Measured on the fixtures: every skipped region reported
`t_r` within a fraction of a percent of `t_max` - a uniform gap, not a wedge.

**Densification is spatial as well as gradient-driven.** Refining only where
`|grad t|` is steep describes a *varying* gap well and a **uniform** one not at all,
which is backwards: a uniform gap is the canonical sheet or band. A plate modelled as
a box carries two samples per wall, the gradient between them is zero, no round
fires, and the region then dies on the speck floor. The second criterion applies only
inside the thin band: an adjacency edge whose two endpoints are both within `t_layer`
and further apart than `h_bootstrap` is subdivided, because a gap the sizing field
will resolve at `h_bootstrap` has to be *measured* at `h_bootstrap` or its extent,
its rims and its mid-surface are all inferred from a handful of corners. The sheet
fixture went from 7 densified samples to 148, and its limb's wall became one region
of 128 samples with `area` exactly its 0.27 x 0.20.

**Battery check 3 is a majority, not a veto.** Marking both ends of a discontinuous
pair as failed reads as symmetry and is not - the discontinuity has one author. At a
plate's rim a sample's ray leaves the gap and lands on a far body, and under the veto
that single outlier condemned all eight of its interior neighbours: 80 vetoes from a
handful of rim samples took the region's confidence to 0.82 against a 0.90 gate. A
sample now fails when it disagrees with more of its neighbourhood than it agrees
with, which makes the outlier fail alone. Confidence 0.820 -> 0.992.

**One gap is one region.** The hysteresis walk cannot guarantee that by itself: the
pairing closure claims the opposite wall's samples as it goes, so a later seed on the
same wall finds them in its way, stops, and becomes an island. The band plate fixture
came out as one region of 212 samples plus **21 fragments of one or two**, each too
sparse to convert and each therefore asking the sizing field for two elements across
the very gap the band conversion exists to mesh with one - 340,360 LFS sources and a
ten-fold mesh. After segmentation, regions of the same group and declared regime
whose samples are adjacent are unioned. The relation is undirected and the result is
re-indexed by lowest member, so the merge is order-independent (R-P2).

**Gates, in order.** Intersection-wedge rejection, then speck suppression (`speck_min_samples`, default 12, or area
below `speck_min_area_factor * h^2`), then the confidence floor
(`gaps.confidence_min`), then - for a `Sheet` candidate - mid-surface construction
and validation. A region that fails any gate falls back to `Normal` (volumetric)
and is logged `[THIN-SKIP]` with its reason and per-check histogram. **The fallback
is always volumetric, never a sheet**: an invalid correspondence must not be able
to force a collapse.

**Mid-surface.** `m_i = (p_i + phi(p_i)) / 2` at every paired vertex sample of wall
A, re-indexed onto wall A's own triangulation. **A node with no vertex sample of its
own is placed by the nearest measurement on the same wall** (ties broken by the
earlier sample, so the choice is a function of the data and not of the traversal).
Restricting the mid-surface to vertex samples alone made it unbuildable for exactly
the shape it exists to describe: at a box plate's corner the vertex normal is the
diagonal of three faces, so the ray leaves the gap instead of crossing it and all
four corners go unpaired - `MidSurfaceUnbuildable` by construction, on the canonical
sheet. The mid-surface of a sheet is wall A carried half the local gap inward, and
that is what a neighbouring sample measures. Inherited connectivity is not
trusted (`validate_mid_surface`): positive area, orientation against the source
wall, adjacent-normal deviation under 60 degrees by `cos^2`, no self-intersection
(exact `orient3d` segment-through-triangle), boundary edges matching the region's
rim loops, and Euler characteristic matching wall A's patch. Local constrained
re-triangulation - the ladder's first repair rung - is **deferred to G7**; today an
invalid candidate goes straight to the volumetric fallback, which is the safe
direction.

**Rims.** Boundary edges of wall A's face set (incidence one), chained
deterministically from the smallest node key.

**Coupling loop** (`src/meshgen/sizing.rs`). `couple_gap_and_sizing` implements the
frozen rule `h^(n+1) = max(h_min, min(h^(n), C(R^(n))))`: the driver applies the
running minimum and the floor itself, so a constraint that tries to raise `h`
cannot break monotonicity. Guards: a region that *tightens* locks immediately; a
region whose regime differs from its value two iterations earlier locks to `Normal`
with a WARN; the iteration cap (5) locks every still-unstable region to `Normal`;
and the post-loop G-8 assertion `eps << t_sheet < t_layer <= h` is re-checked on the
realised field, returning an error rather than a warning. The sizing constraint
`C(R)` is **G4-1's**; until it lands the pipeline passes the bootstrap size, so the
loop settles in one iteration.

A consequence worth knowing: under the frozen rule oscillation is unreachable
through the constraint - `h` only falls and Rule S3-M pins `t_r`, so each region
changes regime at most twice. The oscillation guard covers the one path SPEC §11.4
names, hysteresis-boundary noise, and the test drives it through an inverted dead
band.

**s03 additions.** `band_region` (schema Dbg) carries the region id per wall face;
the additive `thin_role` cell array separates ordinary walls (`0`), walls inside a
converted region (`1`), and mid-surface faces (`2`); and the additive
`ThinRegionRegime` / `ThinRegionConfidence` / `ThinRegionSeparation` /
`ThinRegionSkip` field tables carry one row per region. Mid-surface triangles are
emitted as ordinary tagged face cells carrying their region's component with
`FaceTagKind = 1` (sheet) - a mid-surface is exactly the sheet the region will
collapse to. Both additions are recorded in `SPEC_meshgen_contracts.md` §2.1/§2.3.

**Cost and parallelism (R-P1/R-P2).** Every implemented stage runs multicore. In
S3 the ray cast, closest-pair sweep, geodesic-shortcut pass and battery check 1 are
parallel; in S2 the per-source-triangle split and the residual-crossing test are.
All of them use the permitted shape - per-item work into an indexed buffer,
concatenated in index order - so the committed output is bit-identical to the serial
one and to itself at any thread count.

On the 12,183-face `TestCaseIntersect1` reference dataset the whole `mesh` run is **1.62 s** at 8
threads (3.68 s at one, 310% CPU), split between S2 (~0.8 s) and S3 (~0.65 s). S3
scales 3.4x, its closest-pair sweep 5.6x. One serial block remains: the G2 narrow
phase (~370 ms), which builds the shared intersection registry and so needs a
three-phase restructure rather than a parallel map.

Two non-obvious lessons from that work. The sweep and the geodesic pass did not
scale at all until their per-call allocations were replaced by per-thread scratch
(`TriGrid::query_into`, `GeodesicScratch`) - with 8 threads the global allocator,
not the geometry, was the bottleneck. And the large wins were algorithmic, not
parallel: three O(n^2) scans (component closure per face, registry rescans per
triangle, point-feature checks against every face) accounted for the bulk of a
587 s -> 1.62 s improvement.

Set `RUSTMSPT_TIME_STAGES=1` for per-stage wall times on any run (`[STAGE-TIME]`
per pipeline stage, `[S2-TIME]` inside the arrangement, `[S3-TIME]` inside S3).

Acceptance (`tests/meshgen_thin_tests.rs`, 19 tests): a resolvable gap segments to
`Band` with no mid-surface; a sub-`t_sheet` wall segments to `Sheet` with a valid
mid-surface whose midpoints lie exactly on the mid-plane; the pairing closure yields
one region per gap; specks and a failed confidence gate both fall back to
volumetric; a twisted and a self-intersecting candidate are rejected by the
validator while a flat one passes; s03 carries the tables and the mid-surface
faces; the loop converges within three iterations, clamps `h` from above and below,
locks an oscillating region to volumetric, reports the cap, and fires the G-8
assertion; and two crossing surfaces never collapse to a sheet while every converted
region stays inside its own regime interval.

## G4-1 sizing field and grading (S4)

Three parts, in `src/meshgen/sizing.rs` beside the coupling driver: the **sources**
(what limits the element size, and where), the **graded field** (how a limit
spreads), and the **octree** (where the field is stored). `s04_sizing` previews the
result; G4-2 balances the octree and tetrahedralizes it.

### Sources - the criteria of PLAN §10.6

| Criterion | Where it is read | What it asks for |
|---|---|---|
| `Curvature` | every smooth interior edge of the **conditioned** surface | `2R*sqrt(f(2-f))` for the discrete radius `R = w / (2 sin(theta/2))`, with `f = chord_error_frac` and `w` the width **across** the edge - the chord of a circle of radius `R` whose relative sag is `f` |
| `Feature` | every interior vertex of an S1 feature curve | the same chord rule applied to the curve's turn angle |
| `Corner` | every S1 corner and junction | its shortest incident feature segment |
| `Curve` | every locked curve: sharp edges, S2 intersection curves, rims, non-manifold edges | `h_max / curve_cells`, sampled along the curve at that spacing |
| `Gap` | every wall face carrying an S3 sample that stays volumetric | `t / gap_cells` - enough elements across the gap - over a **cover** of the gap slab, not at the sample point |

Three exclusions carry as much weight as the rules themselves:

- **A sharp edge is not curvature.** Its dihedral is a feature the mesh conforms to
  through S7/S8; counting a 90-degree turn as curvature drives every box corner in
  the model to `h_min`.
- **The length in the discrete radius is the width *across* the edge, not the
  edge's own length.** The dihedral measures how far the normal turns as you cross
  the edge, so the matching distance is the two incident triangles' mean height over
  it, `w = (A1 + A2) / L` - never `L`. The two agree only on an isotropic
  tessellation, and where they disagree the edge-length form is simply wrong: a UV
  sphere's short polar latitude edges carry a full-size dihedral, so it reports a
  radius tending to zero at the poles and drives a surface of *constant* curvature
  to `h_min` there. With the width it returns `1/R` in both directions everywhere on
  that sphere. (This is also why a geodesic-neighbourhood estimator is not needed:
  the defect was a mismatched length, not a too-local stencil.)
- **Curvature is read on the input tessellation, never on the arranged surface.**
  Both lengths come from the *sampling*, and S2 corefinement splits edges without
  changing the dihedral they carry: the same physical curvature measured on a
  thrice-split edge returns a fraction of the radius, hence a fraction of the
  element. Measured on the arranged surface this artifact alone drove the field
  to `h_min` around every intersection curve of `TestCaseIntersect1`. For the same
  reason S2's intersection curves emit no feature or corner sources - they have no
  input tessellation to be read on.
- **A separation below `lfs_floor = max(eps, gap_cells * h_min)` is not a gap.**
  Below `eps` the surfaces are a contact (S2's coincidence policy already calls
  anything that close the same surface, and samples along an intersection curve
  measure exactly zero); below `gap_cells * h_min` no permitted element size spans
  it. Either way the gap belongs to the band/sheet templates, or to G7's ladder -
  not to the sizing field.

**The LFS constraint covers the gap slab, not the sample point** (found by the G4-4
review, below). A point source binds the field only at that point, and between two of
them a `beta`-Lipschitz field rises by `beta * d / 2`. Two failures follow, and both
were visible in the first `s04` render: *along* the wall the field rose between
samples (a 0.02 gap ran up to 0.031, wider than the gap itself), and *across* the gap
it rose between the two walls (a wall-only cover puts `h + beta*t/2` at the
mid-plane, which for the natural `h = t / gap_cells` is again wider than the gap).
So each wall face takes the tightest request among its samples, is covered by a
barycentric grid at `LFS_COVER_TOLERANCE * h / beta`, and that grid is **extruded
across the gap the face measured** (along the correspondence vector, so an oblique
gap is crossed correctly). The realized field over the gap is then within
`(1 + LFS_COVER_TOLERANCE) = 1.5x` of the request - a `beta`-Lipschitz field cannot
be exactly constant over a discretely covered region, but it can be held within a
stated factor, and halving that factor costs eight times the sources for two times
the bound. `lfs_points_per_face` and `lfs_max_sources` bound the cover; when the
budget binds, **one common** spacing factor relaxes every face, so the cover degrades
uniformly rather than in visit order.

A source whose value is at or above `h_max` is dropped: the field's ceiling already
says that, and the source count then tracks the *curved and close* part of the input
rather than its triangle count.

### The graded field - grading by construction

```
h(x) = clamp( min over sources s of ( h_s + beta * |x - s| ),  h_min, h_max )
beta = grading - 1
```

A minimum of `beta`-Lipschitz functions is `beta`-Lipschitz, so
`|h(x) - h(y)| <= beta * |x - y|` holds **everywhere with no smoothing pass**. That
is the 2:1 gradation of §10.6 at the default `grading: 2.0`: over a distance of one
element the size may at most double. Because there is no relaxation sweep, grading
cannot depend on iteration order, visit order or thread count - the determinism
question does not arise.

`SizingLookup` buckets the sources in a uniform grid and expands Chebyshev rings
from the query, stopping as soon as `smallest + beta * (ring - 1) * cell` reaches the
running best. `eval_box` returns the field's **exact** minimum over an axis-aligned
box (the source-to-box distance is closed-form), which is what the octree refines
against - a "centre value minus beta times the half diagonal" *bound* would be valid
but pessimistic, and would split a uniform field one whole level too far everywhere.

### The octree

`build_sizing_field` refines a root cube covering the domain, one parallel pass per
level: a cell splits while its side exceeds `eval_box` over itself, and cells that
miss the domain box are dropped **down to `forest_level`** - the coarsest level whose
cells already satisfy `h_max` - below which every child is kept even where it sticks
out. That restriction is a correctness requirement, not an optimization: the drop set
must be a *subtree cut*, or S5's face rule breaks. A face whose centre is a lattice
corner takes case Q, which emits the face's four edge midpoints, and those are
lattice corners only because the four finer neighbours across the face exist
(SPEC §3.1 L1). Drop one of them and the coarse cell emits a node its neighbour never
sees - a hanging node. On a 1 x 1 x 0.35 domain the unrestricted drop produced 216
hanging nodes and 8 non-manifold edges; the subtree cut produces zero. The lattice
therefore overhangs a non-cubic domain by up to one coarse cell per axis, which is
what S8 trims and why [V3]'s boundary-leak rule is deferred before the cut. Leaves come back in canonical `(level, coord)`
order, so `locate` is one binary search per level with no allocation. Two budgets
guard it - `SIZING_MAX_LEVEL` (12) and `SIZING_MAX_LEAVES` (1,000,000) - and both are
enforced a whole level at a time, so hitting one cannot make the result depend on
traversal order. Leaves left coarser than the field asks for are counted in
`stats.n_unresolved` and reported as a WARN naming which budget bound.

### The constraint `C(R)`

`SizingConstraint::evaluate` is what the frozen driver calls each iteration:

```
C(R) = min over thin regions r of ( geometry(r),  t_r(r)/gap_cells  if R(r) = Normal )
```

`geometry(r)` is the regime-independent field's minimum over region `r`'s samples.
A region that converts to a band or a sheet stops constraining - its gap is meshed
by a template. A region that stays volumetric must be resolved, which is exactly the
feedback the loop is for: a falling `h` shrinks `t_layer`, pushes a region out of
`Band`, and the region's own LFS demand then lowers `h` further, monotonically.

Two readings had to be settled, and both are recorded here because they are not
forced by the frozen text:

- **The scalar `h` is the governing thin-feature size, not a global mesh size.**
  `C(R)` is a minimum over the *thin regions* only. The frozen rule writes `h` as a
  scalar, but the config sketch calls the thresholds "x local `h(x)`"; taking the
  global minimum of the field instead would let one sharp feature anywhere shrink
  the thresholds everywhere and decline every sheet conversion in the model. The
  field itself stays spatial - only the thresholds read a single number.
- **A region S3 declined does not participate.** It enters with `t_r = INFINITY`, so
  `regime_for` reads it as volumetric and it contributes no LFS term. Because `C(R)`
  is one scalar for the whole model, letting a measurement S3 judged unreliable set
  it means one bad measurement declines every legitimate conversion - which is what
  a six-sample speck did to all eight band regions of `TestCaseIntersect1` before
  the exclusion. Nothing is under-resolved by it: those samples still emit LFS
  sources, so the *field* refines around them exactly as measured.
- **Except a `Speck` or an `Undersampled` region, which emit no LFS either.** A
  `Speck` is a region of less **area** than one bootstrap element; `Undersampled` is
  a region of real area with fewer samples than `speck_min_samples`. The two are
  reported apart because they mean different things, and both are suppressed: a small
  `t` there reports a degenerate or unmeasured region rather than a gap needing
  elements across it, and
  `t / gap_cells` drives the field to `h_min` around a feature that carries no
  requirement. A-7b built **361,163 sources, 360,835 of them from speck regions**
  along the plate edges, taking `h` from 0.069 to 0.010 and the mesh to 1.2M tets
  for two boxes; suppressing them leaves 17,148 sources and 105k tets, and the
  pipeline prints `[S4/G4-1] LFS suppressed: N sample(s)` so a coarse result near a
  declined region is attributable. Suppressing `Undersampled` matters just as much:
  a one- or two-sample fragment left beside a *converted* band region is a piece of
  that same gap, and letting it ask for `t / gap_cells` puts two elements across a
  gap the band conversion has already undertaken to mesh with one - 340,360 LFS
  sources on a7b, from seventeen fragments of a gap that had been measured correctly.
  A measurement too sparse for S3 to act on is too sparse to bind the field to its
  floor. `LowConfidence`, `MidSurfaceInvalid` and
  `IntersectionWedge` are **not** suppressed - those are real gaps the thin path
  failed to take. Suppressing `IntersectionWedge` as well was tried on 2026-08-07
  and measurably regressed accuracy (A-3 0.68 % -> 0.90 %, A-8 4.77 % -> 8.45 %):
  the wedge LFS is doing real work at an intersection, and `curve_sources` at
  `h_max / curve_cells` is coarser than what it asks for. Reverted.

The pipeline prints the binding region and which term bound it
(`binding_region`), because with one scalar for the whole model that is the
difference between an explained fine mesh and an unexplained one.

### `s04_sizing`

`sizing_to_doc` emits one `VTK_VOXEL` cell per leaf with `cell_kind = 3` and the
`sizing_h` point array (SPEC_meshgen_contracts §2.2/§3). Corners are deduplicated on
the octree's own integer lattice at `max_level`, so a level jump shares exact
coordinates rather than nearly-equal floats and [V2] finds no coincident nodes; a
point's value is the smallest among the leaves touching it, which is what makes the
level jumps legible in a heatmap. There are no tets, so [V1]/[V3]/[V4] have nothing
to check and the tet-only arrays carry their sentinels.

`build_scene` gained voxel support alongside it: a quad face referenced by exactly
one *selected* voxel is a boundary of the selected subset, so `clip_plane` and
`bbox` filters cut into the octree and expose its interior level jumps rather than
hiding them - the same crinkle-clip semantics tets already had.

### Config

`sizing.grading` (default `2.0`, must be `> 1`) and `sizing.gap_cells` (default
`2.0`, must be `>= 1`) are the two G4-1 additions to the §6.3 block; both enter the
`ConfigHash`.

### Parallelism (R-P1/R-P2)

Curvature sources are a per-face map into an indexed buffer plus a parallel sort;
gap sources are a per-sample map; the octree runs one `par_iter` per level consumed
in index order. Every field value is a `min` fold - exact, so order-independent - and
no float reduction crosses a thread boundary.

Acceptance (`tests/meshgen_sizing_tests.rs`, 25 tests): the empty source set is the
constant ceiling; the field grows at exactly `beta` and clamps at both ends; the
Lipschitz bound holds over 900 probe pairs of an awkward source set; `eval_box`
matches a brute-force scan; a uniform field yields a uniform lattice one level -
not two - below `h_max`; the octree refines at a source and grades away from it;
leaves tile the domain and every leaf is locatable; both budgets report rather than
silently truncate; a non-cubic domain is covered and outside cells dropped; a box
has no curvature but has corners; a sphere's median request matches
`2R*sqrt(f(2-f))` and does **not** move when the input is refined; LFS asks for
`gap/gap_cells`, converted regions ask for nothing and contacts ask for nothing;
`C(R)` is the ceiling with no regions, drops for a volumetric region, ignores a
converted one, and ignores a declined one; the loop converges in <= 3 iterations
against the real constraint; s04 validates, verifies clean, deduplicates its corners
and renders; and sources plus leaves are bit-identical across runs and between the
default pool and a one-thread pool.

## G4-2 background lattice (S5)

`src/meshgen/lattice.rs`. The stage is entirely combinatorial: every lattice node -
cell corner, split-edge midpoint, face centre, cell centroid - lies exactly on the
integer grid of step `h_min/2` (SPEC_meshgen_geometry §3.2), so the module works in
that **doubled index space** throughout. A leaf at level `L` has step
`1 << (max_level + 1 - L)`; every midpoint and centre is an integer point;
`orient3d` is an exact `i64` determinant. No lattice decision consults a tolerance,
and none needs a neighbour walk.

### Strong 2:1 balance

`balance_octree` refines until any two leaves whose closed boxes touch on a face,
an **edge**, or a **vertex** differ by at most one level. Face-only balance is not
enough and the difference is not academic: a leaf one level finer touching `C` on an
edge alone still drops a node in the interior of `C`'s edge, which is exactly the
case that produces a hanging node the face rule cannot see.

The ripple algorithm: for each leaf at level `L`, look at the 26 neighbouring
*cells* at level `L`; if the leaf containing one of them is coarser than `L - 1`,
split it; repeat to a fixed point. It is complete — if two leaves violate the rule,
the coarser one contains a level-`L` neighbour cell of the finer one, whichever way
they touch — and it terminates because each pass strictly deepens some leaf under a
`max_level` cap. A neighbour cell with *no* containing leaf is either refined past
`L` (handled from the finer side next pass) or outside the domain, so skipping it is
correct in both cases. Split children inherit the parent's `h`, which preserves
"a leaf is no larger than the field inside it" (the child is half the size) while
keeping balance independent of the field it balances.

### Split state, and the two templates

`split(edge)` and `split(face)` are membership tests of the edge midpoint / face
centre against the set of leaf corners — exact integer lookups, evaluated identically
by both cells sharing the entity. A leaf with **no** split face and **no** split edge
takes the frozen 6-tet Freudenthal table; every other leaf is a fan cell. That a
single split *edge* is enough is load-bearing: it is what stops a Freudenthal
neighbour drawing a plain diagonal across a face whose edge carries a midpoint.

The face rule `f(F)` is the frozen SPEC §3.3 table — case **P** (Rule D's min-corner
to max-corner diagonal), case **E** (fan the boundary polygon from a new face
centre), case **Q** (four quadrants, each case P at level `L+1`) — and a fan cell
emits `(t0, t1, t2, centroid)` for every triangle of every face, under the canonical
orientation fix. Because `f` is a pure function of the face's *global* coordinates,
two cells sharing a face produce the same triangles without communicating. That is
Invariant C, and Theorem T1 (conformity) follows from it plus strong balance.

### Quality — a correction to the frozen prediction

SPEC §3.7's Q row read `45.000° / 90.000° / AR 1.3938`. Measured over all eight
quadrant triangles it is **`35.264° / 125.264° / AR 1.6052`** — `arctan(1/√2)`,
attained on the two quadrants whose Rule-D diagonal runs away from the centroid. The
original row had the centre–corner–midpoint row's numbers. **No rule changed**; the
value is forced by Rule D and §3.4. The spec is corrected at rev 1.2 with the
verification record in its §14 [8], and the correction is pinned by a test here.

So transition fans *do* cost minimum dihedral — 45° → 35.264°, a 22% reduction — and
15% in aspect ratio. Both stay far clear of any usable FEM gate ([V4]'s default
`low_dihedral_deg` is 5°), so this changes a claim, not the outlook.

### Emission order and dedup

Cells are processed in ascending Morton order (SPEC §1.4) and template rows in table
order. Nodes are collected in two parallel passes — pass one gathers every corner,
every fan centroid and every case-E face centre; the sorted, deduplicated result *is*
`NodeKey` order, because lattice coordinates are exact integers and need no
quantization; pass two re-evaluates the same templates and emits tets as indices.
Doing the template work twice is cheaper than materializing a coordinate quadruple
per tet, and both passes are pure functions of the same input.

### `s05_lattice`

`lattice_to_doc` writes `VTK_TETRA` cells with `cell_kind = 0`, `region_key = 0` (the
background set), `regime = 0`, `partition_id = 0` and the `sizing_h` point array.

> **Recorded amendment to SPEC_meshgen_contracts §3.** The frozen text says the s04
> *and* s05 previews write `cell_kind = 3` voxel cells. s05 writes tets instead: the
> lattice's whole deliverable is its tetrahedralization, and G4-2's acceptance is
> [V3], which reads tets. A voxel preview of a stage whose output is tets would make
> the stage unverifiable. s04 keeps the frozen voxel form.

`partition_id = 0` is correct rather than a placeholder: with no sheet-tagged faces
the whole lattice is one sheet-blocked partition, which is what [V8] recomputes and
compares against.

### Budget

`LATTICE_MAX_TETS` (20 M) bounds emission — the leaf budget alone does not, because a
fan cell emits up to 48 tets. Exceeding it is a named error naming the leaf count and
the knob to turn, not an allocation failure.

### Measured

On the `TestCaseIntersect1` reference dataset (100³ domain, three bodies): 161,176
sizing leaves → 168,841 after 1,095 balance splits (140,497 Freudenthal, 28,344 fan)
→ 297,771 nodes and **1,651,366 tets**, built in ~0.45 s (balance 0.21 s,
tetrahedralization 0.24 s). `mesh-verify` reports `[V3] multi_shared_faces=0
boundary_leaks=0 hanging_nodes=0 non_manifold_edges=0` and `[V4]
worst_aspect_ratio=1.60517 min_dihedral_deg=35.26439 aspect_ratio_over_gate=0
below_low_dihedral=0`. `s04` and `s05` are byte-identical at `RAYON_NUM_THREADS` 1
and 8.

Acceptance (`tests/meshgen_lattice_tests.rs`, 13 tests, plus 3 in-module table
tests): the frozen Freudenthal table is positive and tiles the cube exactly; Rule D
reproduces SPEC §2.3's sub-triangle table and is invariant under rotation of the
quad; a uniform octree needs no balancing; a deep point refinement balances and only
ever grows; corner-to-corner refinement is caught (the strong-balance case);
a uniform lattice is all-Freudenthal and tiles the domain exactly with every tet
positively oriented; a graded lattice stays inside the frozen 6 / 18..48 inventory and
Bound P1; nodes are distinct and in `NodeKey` order; the corrected quality table is
pinned; Theorem T1 holds directly and through [V3] over **12 randomized sizing
fields** (each spanning ≥ 2 octree levels, > 100 fan cells in total) and on the
edge-only refinement pattern; s05 validates, verifies clean and renders; the tet
budget reports instead of exhausting memory; and the whole lattice is bit-identical
across runs and between the default pool and a one-thread pool.

## G5-1 classification (S6)

`src/meshgen/classify.rs`. For every lattice vertex and every **solid** component,
decide inside or outside; seed one ownership record per tet from that; and mark the
surface patches later stages may skip.

### Parity, made exact

The decision is a parity count: shoot a ray from the vertex along a fixed direction
and count how many of the component's faces it crosses; odd is inside. Each candidate
triangle costs five `orient3d` calls - two for whether the segment straddles the
triangle's plane, three for whether the line passes through its interior - every one
through the §6.1 static filter with the exact predicate behind it. So the answer is a
topological fact, not a tolerance.

Candidates come from a **projection grid**: triangles bucketed by their footprint in
the plane perpendicular to the ray direction, so a ray is a single 2D point lookup
with no traversal. One grid per direction, built once per component. It is only ever
a superset filter - every candidate still gets the exact test - so its resolution
affects speed and nothing else.

### The two degeneracies are not the same, and that is the whole design

A ray that grazes geometry makes some `orient3d` exactly zero, and a parity count is
then meaningless rather than imprecise. There are two cases and they need opposite
treatments:

- **The vertex lies exactly on a triangle's plane.** On axis-aligned input meshed on
  a lattice this is the *common* case, not the corner case - a box face at `z = 0.5`
  and a lattice plane at `z = 0.5`. **Re-shooting cannot help**: the plane test does
  not involve the ray direction at all. It is resolved by the standard symbolic
  perturbation - classify `p + delta*direction` for infinitesimal `delta > 0`. Since
  `orient3d(a, b, c, .)` is affine with gradient the triangle normal, the perturbed
  sign is `sign(n . direction)`, and because the whole ray shares one direction the
  perturbed point is a single consistent point just off the surface. Treating this as
  a degeneracy instead sent **1.1%** of all decisions on the review scene to the
  winding number; with the perturbation the same scene resolves **100%** on the first
  ray.
- **The ray passes exactly through an edge or a vertex** of a triangle. Perturbing
  along the direction does not move the *line*, so this one genuinely needs a new
  direction: the ray is abandoned and re-shot down the frozen `RAY_DIRECTIONS`
  sequence (ARB-9). Only when all five degenerate does the vertex fall through to the
  generalized winding number, with a `[CLS-BAND]` WARN.

### Three component kinds, three treatments

| Classification | Path | Why |
|---|---|---|
| `SolidClosed` | parity | closure is certified, so parity is defined |
| `SolidDefective` | winding number | S2b could not certify closure, so parity is not defined (§10.4) |
| `Sheet` | **never classified at all** | a sheet cannot claim volume whatever any winding number says - the hard guard of §10.4, restated as SPEC_meshgen_geometry §9.1 row 10 |

### Records and `resolve()`

Each tet gets a sparse `OwnershipRecord` of `(X, Side)` entries with provenance
`Lattice`; absent means outside, which is what keeps it sparse on a lattice that is
mostly background. A component owns the tet when **all four** vertices are inside,
is absent when all four are outside, and is `Ambiguous` when they disagree - and that
last set is exactly the cells S8 has to cut.

`resolve()` implements the frozen truth table (SPEC_meshgen_geometry §9.1): take the
inside set, keep only its minimum-priority members, and fall back to `{0}` when it is
empty. That single rule delivers R-A4 (a higher-priority solid replaces a lower one
in the overlap) and R-A3 (an equal-priority overlap keeps every X). `Ambiguous`
entries never enter the set - at S6 they mean "the cut will decide" - and S10 must
refuse a record that still carries one.

### The active-patch filter

A face lying strictly inside a strictly higher-priority solid has the same resolved
label on both sides, so it separates nothing; it is marked inactive and refinement,
snap and cut all skip it (PLAN §5.2). A sheet face is always active - it is a feature
in its own right, not a material boundary.

### `s06_classified`

`classified_to_doc` writes `VTK_TETRA` cells with the real `region_key` and its
region-set table (`RegionSetPriority` carries the single `Y` every X in a key shares,
`0xFFFFFFFF` for background, which is what [V6] checks), plus `provenance` and
`arbitrated` - the latter flagging the straddling cells. The key is **preliminary**:
S8's cut is what settles a cell that straddles a surface.

### Parallelism (R-P1/R-P2)

Vertices are classified in parallel, each writing its own slice of a pre-sized buffer
and reading only shared immutable geometry; the per-vertex diagnostics come back as an
indexed buffer and are folded serially. Records, region keys and the active-face mask
are per-item maps. Parity is a *count*, so candidate order cannot affect it, and the
direction sequence is fixed - two runs and two thread counts take the same branch.

Acceptance (`tests/meshgen_classify_tests.rs`, 13 tests): `resolve()` against all
nine expressible rows of the frozen truth table, and that outside/ambiguous entries
never claim a tet; a box and a sphere classified **by volume** against their analytic
values (the strongest check available - the classified volume must not exceed the
exact one and must not fall short by more than the straddling layer); a dyadic
fixture that first *asserts a lattice vertex really lies on a face plane* and then
that it resolves with zero re-shoots and zero fallbacks; background outside
everything; priority replacement and equal-priority retention; the sheet guard;
a buried patch deactivated while its container stays active; record seeding matching
the vertex classification cell by cell; s06 validating, verifying clean (including
[V6]'s legality and priority checks) and rendering; and bit-identical output across
runs and between the default pool and a one-thread pool.

## G6-1 snap (S7)

`src/meshgen/snap.rs`. The first stage that *moves* anything. S5's lattice ignores
the geometry and S6 only labels it, so before S7 the mesh follows the input in
staircases; S7 pulls the vertices that matter onto the surface so that S8's cut
meets it at nodes that are already exact.

### What the stage decides, and with what

The four S7 rows of `SPEC_meshgen_numerics.md` §5 are implemented as written:

| Decision | How | Class |
|---|---|---|
| edge crossing exists | five `orient3d` signs per candidate face - two straddle tests, three line-side tests | **X** exact |
| snap target priority | corner > curve > surface, then nearest, then ascending `NodeKey` | **I** |
| snap would invert | `orient3d > 0` on every incident tet after the move | **X** exact |
| motion cap | total displacement from the S5 position, `<= 0.3 * L_min` | **A** |

Only *existence* is exact. The crossing point itself is a class-C4 construction
(f64, accuracy-only): its validity is re-established by the exact inversion test,
never assumed.

### The capture rule, and why surfaces are not a capture target

Corners and feature curves are captured inside the cap: any lattice node within
`0.3 * L_min` of one is pulled onto it, corner first. Surfaces are not. Snapping
every node within the cap onto the nearest patch sounds stronger and is in fact
destructive - on a lattice whose element size is comparable to the feature, *both*
endpoints of most crossed edges are inside the cap, every crossing becomes an on-cut
vertex, and the surface S8 was going to cut disappears into the lattice. Measured on
a sphere at `h = 0.25`: **74 crossed edges before, 2 after**. A node takes a surface
target only when the cut would otherwise pass too close to it:

- a crossing **inside the re-check band** (within 2.5 % of an end) would hand S8 a
  sliver, so the endpoint is pulled onto it;
- a crossing **within the weld tolerance** of an end is invariant K2's promotion case
  (`SPEC_meshgen_geometry.md` §5.1): the cut node *is* the parent node, and moving the
  parent that last `eps` is what makes that true of the coordinates and not just of
  the bookkeeping.

### What counts as a corner

`ArrangedSurface::corner_nodes` is **not** the sharp-corner set. S2 unions S1's
corners and junctions with the node of every point feature, and the C6 "tangential
point contact" case fires for any two triangles of one component meeting at a single
vertex - ordinary mesh adjacency around a vertex fan. On a plain UV sphere at 12
bands that is 2688 C6 events over 266 vertices, so S1 reports **0** corners and S2's
`corner_nodes` still contains **every** vertex of the sphere. S7 therefore drops a
node whose only claim is a single-component point feature, unless it also lies on a
feature curve. Nothing is lost: a genuine self-touching contact makes the shared
vertex non-manifold, and S1 reports non-manifold vertices as junctions in their own
right.

### Constraints and the domain box

A node on a domain face may only move within that face; on a domain edge, along it;
at a domain corner, not at all. The test is exact equality, which is the right one -
the lattice is dyadic over the domain box, so a node on a domain face has that
coordinate exactly, and a node that merely rounds to it is interior. Every node that
takes no target but sits on the boundary is recorded as `constraint_kind = 4`, which
is what keeps S8 and S9 from deforming the domain.

### Determinism (R-P1/R-P2)

Determinism comes from the shape of the pass, not from luck. Every move *proposal* is
a pure function of the pre-pass state and is computed in parallel; the *acceptance* is
a serial sweep in ascending node index, because whether a move inverts a tet depends
on whether that tet's other corners have already moved. Accepting in parallel would
make the outcome depend on the scheduler. The serial half is cheap - a handful of
`orient3d` per candidate - and only candidate nodes take part. The crossing sweeps are
parallel maps into an indexed buffer concatenated in index order.

### `s07_snapped`

`snapped_to_doc` writes S5's connectivity on S7's coordinates, carrying S6's
(still preliminary) region keys, the `constraint_kind`/`constraint_ref` pair this
stage produces, and `snap_motion` - a debug point array recorded as an **additive
amendment** to `SPEC_meshgen_contracts.md` §2.2, in the spirit of `separation_t` and
`sizing_h`: how far a node moved is the one quantity a reviewer needs and it cannot
be recomputed from the snapshot alone.

### Measured (the G4 review scene, 268,948 tets)

| | before S7 | after S7 |
|---|---|---|
| crossings | 24,670 | 23,766 on 23,610 edges |
| nodes on the surface | 1,099 | 1,354 |
| worst aspect ratio ([V4]) | 1.6052 | 2.3220 |
| min dihedral ([V4]) | 35.264 deg | 24.349 deg |

305 nodes moved (33 to corners, 272 to feature curves), max motion 8.8e-3, mean
2.1e-3; nothing was capped and nothing was rejected. `[V4]` still passes with zero
elements over the aspect-ratio gate and zero below the low-dihedral share - this
table is the pre-cut half of the data **G6-6** needs. 156 edges are crossed twice by
one component (invariant K1) and are reported for S8 to refine or escalate.

### Curve coverage is measured, and it is currently near zero

Capture used to be opportunistic and unmeasured: the stage snapped what happened to
be in reach and nothing checked the result, so "the mesh conforms to the locked
curves" was an assumption. `curve_coverage` now tests it. A mesh edge **covers** part
of a locked segment when both endpoints lie within `eps` of it *and* the edge's own
length matches the span of its endpoints' projections - which rejects a chord that
leaves the curve and comes back. The covering intervals are unioned in parameter
space, so a segment carried by several shorter mesh edges counts.
`SnapStats::n_curve_segments` / `n_curve_segments_covered` carry the result, the
pipeline prints `[S7/G6-1] curve coverage: C of N`, and a shortfall warns
`[SNAP-CURVE]`.

**Measured 2026-08-07: A-1 0/0, A-2 0 of 12, A-3 0 of 122, A-4 0 of 12, A-8 16 of
1,404, A-6a/A-6b 1 of 35, A-7a 0 of 24, A-7b 1 of 24.** A plain cube's twelve sharp
edges are not carried by a single chain of mesh edges. Refining toward curves alone
makes it *worse*, not better - A-3's curve snaps fell 40 -> 16 once `curve_sources`
landed, because the motion cap is `SNAP_MOTION_CAP * l_min` and shrinks with `h`.
Recovery is a refinement-and-insertion problem, not a snap-harder one.

### K1's remedy, and the limit it establishes

`SPEC_meshgen_geometry.md` §5.1 gives one remedy for both of S7's escalation sets -
edges a component crosses more than once, and uncovered locked curve segments:
*"refine one level, and if the sizing floor is reached, hand the cells to §7."* It was
skipped outright until 2026-08-07. The pipeline now implements it as a loop around
S4-S7 (`K1_MAX_PASSES = 3`): S7 returns `Snapped::refine_requests` - a world point and
the length scale that failed there - the field is re-asked for **half** that scale at
each point, and S5-S7 rerun. A request already at `clamp_h`'s floor does not bind, so
the loop stops and the escalation stands: the spec's second clause, reported as
`[S7/K1]`.

**What it recovers:** A-2 coverage 0 -> 6 of 12 and its volume error 0.076 % ->
**1.4e-14, exact**. A-7a 0 -> 4 of 24, A-7b 1 -> 7 of 24, A-8 16 -> 24 of 1,404
(4.77 % -> 4.15 %), A-6a 5.61 % -> 2.43 %. Cost is +3 % to +25 % elements.

**What it cannot, and this is now settled on both kinds of curve.** A-3 stays at **0 of
122** even forced to `h = 0.0052` - four times finer than the intersection curve's own
0.012 polyline - at 1.48M tets. That was easy to attribute to curvature: a chain of
lattice edges cannot follow a curve that bends between every arranged segment. But
A-6a's **contact rim is straight and axis-aligned**, the case most likely to yield, and
refining it 2.5x (`h = 0.001386`, **6.7M tets** against 270k) moved coverage **1 of 35 ->
0 of 35**.

So the limit is not curvature. Coverage requires *consecutive lattice-adjacent* nodes to
land exactly on one segment, and snapping individual nodes toward a curve does not produce
that chain at any `h` - A-2 reaches 6 of 12 only because its cube is the sole body in the
scene and whole rows of lattice nodes can be pulled onto an edge unopposed. Inserting a
curve as mesh edges is **constrained edge recovery with Steiner insertion, which gate
G6-0 deliberately did not adopt** (`PLAN_mesh_generation.md` §0.4) in favour of the
conforming centroid fan. Read a low coverage number as that architectural bound, not as
under-refinement, and do not spend more elements on it.

### Interior faces of a self-intersecting solid (S6)

`active_face` deactivates a face lying inside a **higher-priority other** solid, and skips
the face's own components by construction - so a triangle buried in its *own* component's
overlap stayed active. S7 cut on it while S6, labelling by winding number, correctly
reported both endpoints of the edge inside: `[S67-SITE] no-ambiguous-component`, **2,184
of A-8's 2,478 escalations**, every one fanned and chamfered.

The union's boundary is where insideness changes, so the test is exactly that - probe
`ACTIVE_FACE_PROBE` off both sides of four barycentric samples of the triangle and ask
`winding_inside` about the component's own faces. Every sample must agree: S2 does not
corefine a component against itself, so a triangle can be *partly* buried, and keeping a
mixed one active is the conservative direction (cutting on a partly-interior face costs an
escalation, dropping a boundary face costs material). A clean closed solid never trips
this, because one side of every boundary face is outside it.

**A-8: inactive faces 0 -> 576, escalations 3,687 -> 853, volume error 4.15 % -> 3.73 %,
tets 327,699 -> 303,919**, `[V3]` clean, `[V6]` 0, R-P2 bit-identical.

What survives is a **K1 pocket**: an edge one component crosses twice has both endpoints on
the same side, so S6 is right and there is no disagreement - the surface dips into the cell
and back out, leaving a closed bubble with the whole cell *boundary* on one side. The fan
gives such a cell wholly to one side and loses the pocket, and §7.6 answers `one side
empty`. **It is bound by the sizing floor, and it converges:** `h_max_frac` 0.05 -> 0.025
moves the error 4.148 % -> 4.090 % (nothing), while `h_min_frac` **0.012 -> 0.004** moves
it **3.733 % -> 3.007 %**. The loss is a chamfer and scales with `h` as a chamfer must, so
A-8's headline number is the floor its fixture sets, not a defect.

### §7.6's sequential cut

The centroid fan gives up the cut *inside* an escalated cell, which is what chamfers the
material boundary and leaves adjacent tets stepping two components across an undeclared
face. §7.6's remedy is to cut the cell along the surfaces crossing it instead.
`split_soup_by_surface` partitions the cell's conforming boundary soup by one
component - a triangle takes the unanimous side of its off-surface nodes - and reports
**every** cap the surface leaves; `split_escalated_cell` applies that per crossing
component, caps each piece, and keeps the result only if the holes are **simple loops
agreed by both sides** and the fan volumes over the pieces still **sum to the parent's**. The second check is
what catches a non-convex piece whose centroid fell outside it. Any refusal falls back to
the total fan, whose validity is unconditional.

It first fired **zero** times, and the reason was measured: of A-3's 884 refusals, 840
were `mixed sides on one triangle`. That is a boundary triangle with nodes on both sides
of a surface and no cut node between them - because `face_mesh` split a face by *one*
component's `FaceCutState` and sent a face two surfaces cross to `LoopFan`, whose
triangles cone across both cuts and align with neither. **A cell-level cut cannot repair
a face-level misalignment.**

`crossed_face` fixes that half. A face two patches cross now keeps both chords as
**edges** of its triangulation:

- **Crossing chords** (their endpoints interleave around the boundary walk - a
  combinatorial test, so the two cells cannot disagree) meet at
  `chord_meeting_point`, the closest approach of the two segments taken in canonical
  order. That point is where the S2 intersection curve pierces the face. The four chord
  endpoints cut the walk into four arcs, each closing into a sector through it.
- **Non-crossing chords** are two disjoint diagonals and need no new node: split the walk
  by the first, split whichever half holds the second by it, fan the three parts from
  their lowest-key vertex. This is the common case - A-3: 960 faces against 132.

Measured on A-3: **624 faces aligned, generic faces 832 -> 208, `[V3]` clean, 181,098
tets against 182,082, component 2's volume error 0.407 % -> 0.393 %**, `[V6]` 169 -> 177.

**The cell-level split is on by default.** `split_escalated_cell` cuts 94 of A-3's
escalated cells into 284 material pieces, taking **`[V6]` 169 -> 146** with `[V3]` clean
and R-P2 bit-identical. Getting there needed two more corrections beyond the face
triangulation, both found by measurement:

- A face's **meeting point lies on both surfaces**. Marking only `cut_index` nodes as
  on-surface made the second component ask the classifier for a side at a point exactly
  on its own surface, where the answer is whichever way the predicate rounds - 226
  refusals of `one side empty`.
- **A cap must not be coned to a fresh centre** when the other surface crosses it: that
  puts a triangle across the intersection curve, the second cut declines on both halves,
  and the cell comes out in two pieces instead of four. Splitting the cap along its own
  meeting points keeps the curve an edge and needs no new node.

Three guards remain, and each earns its place. The cap holes must be simple loops that
both sides agree on; the fan volumes over the pieces must still sum to the parent's,
which catches a piece whose centroid fell outside it; and where **two components** share
the cell they must meet inside it. That last one separates a junction from a gap: two
*different bodies*' walls crossing a cell without meeting belong to S8b's band ladder,
and splitting A-7a's 512 plate cells cost 0.36/0.10 % -> 0.57/0.44 % of volume before the
guard was added. It counts distinct **components**, not chords - one component crossing a
face twice is a plate's own two walls, which has no gap to mistake.
`RUSTMSPT_NO_JCT_CUT=1` restores the old total fan; `RUSTMSPT_JCT_DIAG=1` prints refusals.

#### One surface, more than one hole

A lattice cell straddling a thin plate is crossed by that plate's surface **twice**, so
the material between the walls is bounded by two caps and the outside is two disjoint
solids. Assuming a single cap read that as "not one simple loop" and refused, so every
such cell fell back to the fan that chamfers the gap away - which is precisely the
"band declared, no band elements" behaviour, 512 of the sheet fixture's 538 escalations
declined `FaceShape` by the §8.2 table and every one of them fanned. Three changes open it,
none of which needs a new table row:

- `split_soup_by_surface` reports every cap, and the two sides must still report the
  **same** caps - that is what makes the cut a shared set of faces rather than a crack.
- `split_soup_components` separates each side into its edge-connected parts, so the two
  disjoint outside solids do not become one piece with one material tag.
- A triangle whose corners are **all** on the surface is classified by its own centroid.
  The strip between two chords of a doubly-crossed face has every corner on the surface
  while lying squarely in the material between the walls; refusing there declined every
  band cell.

`crossed_face` also describes a component that leaves **four** cut nodes on one face as
two chords rather than refusing. The two walls cannot cross inside the face, and pairing
across the two crossed edges leaves exactly one non-interleaving choice - the nested pair
`(p0,p3)`, `(p1,p2)` on the sorted walk positions. That is a combinatorial test, so the
two cells sharing the face still agree (Invariant J1).

Measured: sheet fixture 0 -> **66 cells cut into 132 material pieces**, band fixture
0 -> **1,089 into 2,178**, and the sheet fixture's component 2 volume error
**2.459 % -> 1.713 %** at 12 % fewer tets, `[V3]` clean.

#### The K1 second cut ships

Keeping invariant K1's **second** crossing on an edge as a real cut node takes those figures
to 145/372 and 2,706/7,029. It is on by default; `RUSTMSPT_NO_K1_SECOND_CUT=1` turns it off
as a bisection handle. It shipped gated for one pass because it cost conformity, and four
separate defects stood behind that. All four are fixed, and each was found only after a
wrong guess, so both the fix and the guess are recorded.

**1. A chord along a walk edge.** An edge carrying two cut nodes contributes three collinear
points to its face's boundary loop (`a, c1, c2, b`), so fanning that loop emits zero-area
triangles - `(a, b, c1)` with `c1` on `ab` - and every cell around the edge emits the same
sliver. A-4 reported the face `(61918, 66209, 303387)` used by **6 tets** and 408
multi-shared faces. `crossed_face` refuses a chord whose endpoints are adjacent on the walk;
the face falls to `LoopFan`, whose apex is the face centroid and so lies off the line.
*Multi-shared faces 408 -> 0.* Two near-misses: an **exact** zero-area test does not detect
these (`c - a` agrees only to an ulp, so the cross product is tiny but non-zero, and the
first filter was bit-identical to no filter), and *dropping* the slivers is the wrong repair
even where it works - it opened 1,488 boundary leaks, because their edges then matched
nothing. Not creating them is the repair.

**2. A closed piece that will not fan.** `split_escalated_cell` checked that every piece is a
closed surface, and that is not enough: a boundary triangle coplanar with the piece's own
centroid gives a flat tet, and `orient_positively` **dropped** it rather than emitting it,
punching a hole through the piece the closure test had just approved. The log said so
outright - `[JCT-DEGENERATE] 588` against exactly **588** interface cracks. `fan_is_sound`
asks the same exact question `orient_positively` asks and declines the split instead;
declining costs the cell its cut and nothing else, because the fallback fan is
unconditionally valid. *Boundary leaks 852 -> 0, interface cracks 588 -> 0, non-manifold
edges 135 -> 0.*

**3. A cap centre on the cell's own face.** A cap coned to a fresh centre puts a node only
that cell knows about. Where a surface enters and leaves through the same face - invariant
K1's own case - the cap is coplanar with that face, so the centre lands **on** it, and the
neighbour across it, which triangulated the face without that node, grows a hanging node:
180 on A-4, 871 on A-6a, 4,417 on A-6b, and provenance tagging showed **every one** was a
cap centre. Testing the centre against the four face planes does *not* find them - the
centre is an average, so it is only numerically on the plane and the exact predicate rightly
says otherwise. Removing the node removes the question.

**4. A fan edge past a meeting point.** Fanning every cap from its lowest-key vertex is
worse than a centre: a cap is not convex in general, and A-3 came back with 12 inverted tets
and A-8 with a duplicated cell. Worse, where the *other* surface crosses the cap its meeting
node sits mid-run on a straight stretch of the cap's boundary - the boundary turns by
nothing there - so a fan from two steps away draws its edge straight past it. A-3's 24
hanging nodes were all meeting points. `fan_cap` tries the cycle's vertices in key order and
keeps the first fan that folds over nothing and swallows no vertex; when none does, the
caller declines. Since the apex is chosen by key it is still a pure function of the cycle.

Two hypotheses were tested against the original failure and **both are wrong**, which is
worth recording because both are the obvious guess:

- It is not a missing escalation rule. All 288 such cells on A-4 already escalate on
  `multi_crossing`; the guard is kept because the invariant is real, and it fires 576 times,
  accounting for every escalated cell there.
- It is not the four-position chord pairing. Disabling `crossed_face`'s two-chord branch
  leaves every conformity count bit-identical.

#### Gate G6-0: the curve reaches S8

`CutOptions::curve_segments` carries every locked curve of the arrangement - sharp edges,
intersection curves, rims, non-manifold edges - into the cut as polyline segments, and
`curve_pierce_points` computes, per lattice face, the point where a curve pierces it. The
key is the face's own three corners ordered by key, so the two cells sharing a face derive
the same entry from the face alone and Invariant J1 survives, exactly as it does for the
chord meeting point. Reported as `[S8/G6-0]`: A-3 764 faces, A-6a 798, A-6b 2,174, A-7a 582,
A-8 4,089. A-1 and A-2 report none, which is correct - a sphere carries no curves, and a
cube's edges lie *along* lattice edges once S7 has snapped to them, so they pierce no face
interior.

Four ways of using that point were measured against A-3/A-6a/A-7a's `[V6]` counts
(139/697/134 at baseline):

| variant | A-3 | A-6a | A-7a |
|---|---|---|---|
| the exact curve point replaces `chord_meeting_point`'s estimate where a face is crossed | 138 | 697 | 134 |
| + the curve point as the `LoopFan` apex, registered for a *guessed* component pair | 123 | **715** | 134 |
| **+ the curve point as the apex, registered for *every* component** | **134** | **697** | **134** |
| + forcing a pierced face to escalate | 123 | 715 | 134 |
| the curve point as a face *vertex*, apex left at the face centre | 155 | **1,275** | 134 |

Only the last is monotone, and it is what ships. The others are recorded because each is a
reasonable-sounding idea that the data refutes.

- **A point on a locked curve lies on *every* surface that meets along it.** Registering it
  for a guessed *pair* of components is what cost A-6a 697 -> 715: the split then asks the
  classifier for a side at a point exactly on a surface it was not told about, gets whichever
  way the predicate rounds, and declines. Registering it for every component removes the
  regression and keeps A-3's gain.

- **Forcing escalation is pure cost.** It yields the identical `[V6]` at about 2 % more tets
  on every case. A cell §6 can cut correctly is not improved by being handed to the fan.
- **The apex must be *on* the curve, not merely near it.** Moving the point off the apex and
  into the face interior - on the theory that an apex lying on both surfaces puts every fan
  tet's centroid inside a surface envelope, where `seed_record` is least reliable - made
  things much worse (A-6a 697 -> 1,275). The theory was wrong.
- **A node per face is necessary and not sufficient.** The best variant trades A-3's 16
  against A-6a's 18. The material boundary still comes from the cap, so until the cut is
  constrained to run **along** the curve inside the cell - a chain of edges, not one point
  per face - the chamfer survives. That is constrained edge recovery proper, the thing gate
  G6-0 originally declined, and it is the remaining work.

#### Why §7.6 still declines, counted

A-6a falls back on 426 of its 538 escalated cells. Counting the reasons
(`RUSTMSPT_JCT_DIAG=1`) rather than guessing them:

| refusal | count |
|---|---|
| `one side empty` | 317 |
| `piece not split by component 2` / `component 1` | 251 / 165 |
| **`a cap does not fan without folding over itself`** | **235** |
| volume guard rejected | 52 |
| `a piece is not closed` | 19 |
| `inside holes are not simple loops` | 17 |
| `mixed sides on one triangle` | 4 |

Setting aside the 317 correct declines (below), the largest real gap is a **re-entrant cap**.
Two ways of meshing one were implemented and measured, and **both are worse than declining**.
Coning the cap to its own centre - the shape this code originally had, with the centre guarded
against the cell's four face planes by a *relative* tolerance rather than the exact predicate -
puts A-8 back to failing `[V1]` and `[V3]`, and does not fire on A-6a at all. That settles an
earlier open question: the exact test was the wrong tool, but a relative one does not help,
because the **node** is the problem, not how it is tested. The second is ear clipping, which a
re-entrant polygon is exactly what exists for, and it is **worse** too: A-3 goes 134 -> 165
with *fewer* cells cut (148 -> 106), and A-6a and A-8 both pick up `[V3]` failures.

The reason is worth keeping, because the mistake is an inviting one. A cap is a piece of a
**curved** surface inside a cell, so it is not planar. Ear clipping fits one plane through
the cycle and tests convexity and containment in it; ears that are ears in that plane are not
ears in space, and the triangles it emits leave the surface. A non-planar cap needs a
triangulation that stays *on* the surface, which is not a polygon-triangulation problem at
all - it is the same insertion problem gate G6-0 names.

`one side empty` turns out to be two different shapes, and reporting the split
(`0 in / N out of N`) separates them:

| case | signature | count | what it is |
|---|---|---|---|
| A-6a | `0 in / N out` | 317 | the whole cell is **outside** the component, which still has crossings on the cell's edges |
| A-3 | `0 in / N out` | 6 | same |
| A-8 | `12 in / 0 out` | 13 | the whole cell is **inside** - invariant K1's pocket |

The first is not a missing capability. A-6a's limb meets its cube only on the plane
x = 0.5817, so for a cell on the cube's side the limb's surface touches the cell's
**boundary** and never enters its interior - `contact_edges` places the coincident crossings
there by design. There is nothing to cut, the cell is one material, and the fan chamfers
nothing; §7.6 declining is the right answer, and only the label made it look like a failure.
That shrinks what the 426 fallback cells appeared to offer, and it explains why forcing
pierced cells into §7.6 cannot help: the surface they would be cut by does not pass through
them.

#### The cap fold test was the obstacle, not the safeguard

`fan_is_simple` asked whether every triangle of a cap fan agrees with the **first**
triangle's normal. A cap is a piece of a *curved* surface, so its triangles genuinely
disagree with each other; once the surface turns by 90 degrees across the cap, a perfectly
good fan fails against whichever triangle happened to come first, and `fan_cap` reports that
**no** apex works. That is what made the 235 re-entrant caps look re-entrant. Measuring the
fold against the cap's **own** orientation - the area-weighted sum of its triangles - still
catches a real fold, because a folded triangle points the other way from the cap as a whole,
without inventing one out of curvature.

Dumping one such cap settles what it really is:

```
cycle 5: 27550 (0.335845, 0.168954, 0.168954)   27557 (0.335845, 0.169146, 0.168954)
         42198 (0.335660, 0.169146, 0.168018)   33437 (0.336036, 0.169146, 0.168413)
         42193 (0.335660, 0.168770, 0.168018)
```

Five nodes straddling x = 0.335845 - the contact plane - and reaching out to 0.335660 and
0.336036. This cap wraps the **rim where the limb's side meets the contact plane**, bending
through a right angle inside one cell. No triangulation of it has all its triangles facing
the same way, so demanding one rejects the cap however it is triangulated.

Three successively weaker versions, each an improvement on the last:

| version | A-6a cap-fold refusals | A-3 `[V6]` |
|---|---|---|
| against the **first triangle's** normal | 235 | 134 |
| against the cap's own area-weighted normal | 142 | 132 |
| **degeneracy only, no orientation test** | **0** | **124** |

Nothing is given up. The guards that decide correctness are downstream and **exact** - every
piece must be closed, and the pieces' volumes must sum to the parent's - and they catch the
genuinely folded caps this used to pre-empt: A-6a's volume-guard rejections rise 52 -> 283,
the same population arriving at a test that can actually tell. A-3 and A-8 both cut more
cells and nothing regresses. The general lesson is worth keeping: **a cheap pre-guard sitting
in front of an exact one is a place to look when a stage declines too much.**

#### The volume guard's measure assumed a shape §7.6 no longer makes

Dumping one of its rejections:

```
tet [10845, 11025, 11026, 10838]   parent 4.779550e-10   measured 4.893833e-10
piece 0  3.976780e-10 (12 tri)   piece 1  2.417293e-11 (8 tri)   piece 2  6.753236e-11 (8 tri)
```

An **overshoot** of 2.4 %, on three pieces that are individually fine - the signature of the
measure, not of the split. `fan_volume` cones a soup to an interior point and sums
**unsigned**, because the soups are assembled per face and carry no agreed winding, and that
is exact only for a **star-shaped** piece. §7.6's pieces stopped being star-shaped the moment
it began cutting the cells it used to decline: a non-convex sliver's centroid falls outside
it, and the sum overshoots. The guard rejected correct splits, and had been growing stricter
in effect with every improvement that made §7.6 cut more.

`soup_volume` removes the assumption rather than loosening the tolerance. Every edge of a
**closed** soup is shared by exactly two triangles, so a breadth-first walk fixes every
triangle's winding relative to the first; the signed sum is then the volume up to overall
sign, and its magnitude is exact for any shape. A soup that will not orient is not closed,
and the caller already refuses those.

| cells cut by §7.6 | before | after |
|---|---|---|
| A-4 | 360 | **540** |
| A-6a | 135 | **189** |
| A-6b | 2,205 | **3,285** |
| A-8 | 474 | **684** |

Two passes running, the defect has been **a guard that was correct when written and became
wrong as the stage around it improved** - first the cap orientation test, now the volume
measure. Both were found by dumping one rejected case rather than by reasoning about code.

#### A partly coincident surface must not cut twice

The volume guard's survivors are **not** another measurement problem: all three pieces
orient cleanly, `soup_volume` agrees with the cone sum to the digit, and the parent is
4.779550e-10 against pieces summing to 4.893833e-10 - a real 2.4 % overlap, the split
genuinely claiming the same material twice.

The cause is the coincident-cap rule being too narrow. It skipped a component whose
**whole** cap was already in the piece, and a *partly* coincident surface slips past that:
it contributes one genuinely new cap triangle alongside two it shares with a cut already
made, so the test passes, the piece is cut again along a wall that is already one of its own
faces, and the overlap lands on the volume guard - which rejects the whole cell. Widening
the test from `all` to `any` is the fix: re-cutting along a wall the piece already carries is
never right, however much else the component contributes.

| | before | after |
|---|---|---|
| A-6a cells cut by §7.6 | 189 | **414** |
| A-6a `[V6]` | 679 | **670** |
| A-6b cells cut by §7.6 | 3,285 | **3,532** |
| A-6b `[V6]` | 1,037 | **1,021** |
| A-6a volume-guard rejections | 229 | **0** |

Three whole refusal categories are now empty - cap folds, volume guard, and (bar one)
unclosed pieces - and of the 434 refusals that remain, 317 were already established as
*correct* declines.

#### The residue is face-level, proved by experiment

Of A-6a's 434 `split refused`, **335 are `one side empty` and every one is `0 in / N out`** -
the class already established as a *correct* decline. Of the rest, 17 are `inside holes are
not simple loops` and about 82 are `mixed sides on one triangle`.

A triangle with corners on both sides and no cut node between them can be classified by its
own centroid instead of declining the cell - the same question `side_of_face` already answers
for the all-on-surface case. That is a large `[V6]` win and it cannot be kept:

| | bail (shipped) | classify by centroid |
|---|---|---|
| A-3 `[V6]` | 124 | **66** |
| A-6a `[V6]` | 670 | 659 |
| A-6b `[V6]` | 1,021 | 1,010 |
| A-3 `[V3]` | PASS | **FAIL** - 2 faces on four tets, 1 hanging node, 6 non-manifold edges |

The neighbour across that face classified it independently and disagreed.

**The decisive experiment**: allow the relaxation only for triangles the cell made itself -
an earlier component's cap, which is interior and which no neighbour can see - and keep
bailing on the cell's own boundary. That recovers `[V3]` and gives back **every single bit**
of the `[V6]` gain, bit for bit. So all of these triangles lie on the cell's own boundary
faces.

That is a proof rather than an inference: a mixed-side boundary triangle means the **face**
was not split where the second surface crosses it, and no cell-level cut can repair a
face-level omission. §7.6's cell split has been taken as far as it goes; what remains is
inserting the crossing into the face's triangulation, which is gate G6-0's insertion.

#### An uncertain predicate answer is not a side

`PointClassifier::inside` reports how many of its predicates were ambiguous, and §7.6's
`side_of` was discarding that count and taking the guess. An uncertain answer means the point
is on the surface as far as the predicate can tell - the same statement `surface` makes for a
cut node, reached geometrically rather than combinatorially - and it is exactly what happens
where two bodies are in **exact contact**: a node on the shared plane is genuinely on both
surfaces, and asking which side it is on has no answer. Taking the guess produced triangles
with corners on both sides and no cut node between them, which is the `mixed sides` refusal.

| | before | after |
|---|---|---|
| A-3 `[V6]` | 124 | **122** |
| A-3 cells cut by §7.6 | 162 | **205** |
| A-6b `[V6]` | 1,021 | **1,018** |

Refuted alongside it: these triangles are **not** produced by `crossed_face` abandoning a
face over an edge-aligned chord. That refusal sends the whole face to `LoopFan`, whose
triangles interleave both cuts - the obvious repair is to keep the face's other chord
(`continue` rather than `return None`), and it is neutral: A-6b 1,021 -> 1,022, nothing else
moves.

#### A-6a's residue is a chamfer at a triple corner

Every sampled violation has `provenance = 3` on **both** tets - both are §7.6 or fan pieces -
and of 50 sampled faces, **none lies on an input plane**. They sit within ~5e-4 of where
three surfaces meet: the contact plane x = 0.5817 and the limb's sides y = 0.2917 and
z = 0.2917/0.2977. For example:

```
(0.582031, 0.421875, 0.292969)  (0.581700, 0.421875, 0.292638)  (0.581671, 0.423017, 0.291347)
```

That rules out both cheap explanations at once. It is **not** a tagging gap: the face is not
on a surface, so declaring it would be a false declaration of coincidence - precisely the
failure `declare_contact_components` is written to avoid. And it is **not** mis-seeding: the
classifier is confident there, and `[JCT-SEED]` reports `0 exact-predicate escalation(s)` on
every case (voting over four interior samples instead of one is exactly neutral).

It is a chamfer at a **triple corner**. §7.6 cuts a piece by one component at a time, so it
follows two surfaces meeting along a curve, but the corner where a third joins them is not
reproduced and the material boundary is rounded off inside one cell. A-6a and A-6b are the
fixtures that have such a corner - a limb rooted flush on a cube face - which is why their
`[V6]` is an order of magnitude above A-3's. Corner insertion was then built and measured, and it is **not** the lever: `cell_corners`
locates every point where three or more curve segments meet, and A-6a has **9** such cells
against 670 violations (A-3 12, A-8 221), while the violations spread along the rim in *y*
rather than clustering at points. The residue lies **along the rim curves**, not at their
corners. The per-face piercing already puts a node on those curves, but only as the
`LoopFan` apex of an **escalated** cell - a cell §6 cuts by the table never sees it, and
forcing those to escalate is measured pure cost. Closing it needs the curve carried through
the cut as a chain of **edges**.

### Session close: `[V6]` is the single open item

`[V1]` and `[V3]` are clean on all nine acceptance cases for the first time, and five of the
nine pass every check that runs. `[V6] region_adjacency` is what remains: A-3 139 -> **122**,
A-6a 697 -> **670**, A-6b 1,026 -> **1,018**, A-7a 134.

It is located rather than merely bounded. Both tets of every sampled violation are §7.6 or fan
pieces (`provenance = 3`), and **0 of 50 sampled faces lie on any input plane** - the boundary
they carry is the chamfer between pieces of *different* cells, on a lattice face that is on no
surface. Everything short of rebuilding what §7.6 emits has been implemented and measured:
per-face curve piercing and all-component registration (shipped, A-3 139 -> 122), forcing a
pierced face to escalate (identical `[V6]`, measured twice), corner insertion (9 cells against
670 violations), relaxing the mixed-sides guard (A-3 -> 66 but breaks `[V3]`, and provably so),
ear clipping (a cap is not planar), a cap centre with a relative guard (`[V1]`/`[V3]` failures),
and voting in `seed_record` (neutral - no sample is ever uncertain).

Closing it needs the cut to carry the locked curve through the cell as a **chain of edges**, so
the material boundary lies on the curve instead of being chamfered to the nearest cell face.
That is constrained edge recovery with Steiner insertion - gate **G6-0** re-opened - and it is a
rebuild of §7.6's output rather than another guard fix. The groundwork is in place: curve
segments reach S8, the per-face piercing map is keyed so both incident cells agree, and curve
nodes are registered as on-surface for every component.

#### One interface cell per face

`derive_interface` derives a contact face once **per component**, which is correct: two
solids in exact contact share one mesh face that is both their boundaries. The writer then
emitted a triangle for each, which is a duplicate cell - A-6a's `[V1]` reported 50, all on
the contact plane - and it also left every `FaceTag` set a singleton, so `[V6]`'s coincidence
exemption had nothing to read. `cut_to_doc` now groups the tags by node **set** and emits one
cell per face. The set matters: a face's nodes are ordered by *quantised* key, two distinct
nodes can share a key, and the stable sort then emits `[a, b, c]` from one component and
`[a, c, b]` from the other, so comparing the arrays missed exactly the pair that mattered.

Acceptance (`tests/meshgen_snap_tests.rs`, 12 tests): no tet inverted; no node past
its cap and every clamped node listed for the [V5] gate; surface-constrained nodes
verified to lie on the surface by an *independently written* point-triangle distance;
a cube's corners captured exactly and its sharp edges captured too; domain-boundary
nodes proved not to move off their plane; the re-check leaving **zero** crossings in
the band, with the residue counter agreeing; the exact inversion test accepting a
small move and rejecting both a coplanar and a through-the-face one; every crossing
consistent with the emitted coordinates and its own face's plane; a plate thinner than
one element producing K1 escalations *and* the warning; an all-inactive scene moving
nothing; bit-identical output across runs and thread counts; and `s07` verifying with
no FAIL item.

## G6-2..G6-5 the cut (S8)

`src/meshgen/cut.rs` and `src/meshgen/junction.rs`. This is the stage that makes the
mesh conform: every cell an active patch passes through is replaced by pieces that
stop exactly at the patch, so the surface becomes a union of element faces.

### Three frozen tables, one reason they work together

- **§4** Rule SNK: a quad's diagonal is the one incident to its smallest-`NodeKey`
  vertex, and Theorem T2 says that rule never produces a cyclic prism.
- **§5.2** the kirigami face-split table: what a *face* looks like after the cut.
- **§6** the six single-patch tet cases: what a *cell* becomes.

Nothing in S8 communicates between cells. What makes the pieces meet is Invariant C:
every choice is a function of node keys and of the cut state, and two cells sharing a
face agree on both. §6's tables are built so that each piece's boundary triangles on
a parent face are exactly what §5.2 produces for that face - a claim checked on
19,200 randomised faces rather than assumed, and again end to end by `[V3]`.

Cut nodes are given global ids **before** any cell is cut, keyed by
`(edge, component)`. That is what makes the per-cell work independent: two cells
sharing an edge look up the same id, so their pieces meet, and the cells can be cut
in parallel and concatenated in cell order.

### The guarded dry-run

Before a cut is committed: every child positively oriented, and the children's
volumes summing to the parent's within 1 %. The volume test is the one that catches
a *wrong table row*, because a mis-assembled case still produces positively oriented
tets - it just does not fill the parent. Measured worst volume error on the review
scene: **9.5e-15**.

Note that §4.3 gives prism *node lists* under "emit under the canonical orientation
fix" - row 6's `(a0,a1,b1,b2)` is negatively oriented on a plain straight prism - so
`orient_positively` is not a nicety. A caller that emits the table verbatim ships
inverted elements.

### Escalation, and why an escalated cell is not a skipped cell

A cell crossed by two or more patches, or with an edge one component crosses twice
(K1), or in a state §6 does not cover, cannot take the table's path. It is **not**
left alone: its neighbours have split the faces they share with it, and a cell that
keeps its four flat faces next to them puts hanging nodes on the mesh. On the review
scene, leaving them whole cost 4,577 hanging nodes and 78 non-manifold edges.

Gate **G6-0** decided what happens instead: **no-go** on a local-PLC constrained
tetrahedralisation, and the plan's named fallback adopted in a stronger form -

> every cell's boundary is triangulated by a **pure function of the face's own
> state**, and its interior is a fan from a Steiner point at the cell's centroid.

Conformity (J1) is then by construction rather than by curve-node pinning: there is
nothing to cache, because the function *is* the invariant. Validity is total: a
parent tet is convex, so its centroid sees its whole boundary. Invariant J2 is exact,
because where the frozen table applies the routine *is* the frozen table. What it
gives up is the cut inside those cells - the material boundary chamfers by at most
one cell (`<= h`), logged as `[JCT-FALLBACK]`. 1,423 of 268,948 cells (0.53 %) on the
review scene.

Two details of that fallback are load-bearing, and both were found by measurement:

- The face fan is coned to the **face centroid**, not to a vertex of the loop.
  Fanning from a vertex emits a zero-area triangle whenever the apex and a
  consecutive pair all lie on one parent edge, which is exactly what happens when a
  corner has a cut node on an incident edge. Those pieces fail the orientation test
  and get dropped, leaving holes - 8 dropped pieces, 12 leaking faces.
- That centroid is allocated **once per face**, shared by both incident cells.
  Allocating one each puts coincident duplicate nodes on the shared face and undoes
  the conformity it exists to provide.

A cell can also need escalating with **no ownership ambiguity at all**: S6 classifies
vertices by parity and S7 finds crossings by exact edge tests, and the two can
disagree on a cell without either being wrong. The triage therefore escalates on *any
cut edge*, not on the record.

### Ownership and the derived interface index (G6-3)

The §6 table says which side each piece is on, so the corner votes, union-find and
oriented probe of `assign_cut_sides` are not needed here - the ownership is read off
the table. The parent's *definite* entries carry over untouched: a cell inside
component 1 being cut by component 2 stays inside component 1 on both sides.

The interface index is **derived, never stored**. The two side elements are recovered
by asking which tets carry the triangle and what their records say, so it can be
rebuilt from the mesh alone and a stale index is impossible. `side_elems[0]` is the
element inside the component and `[1]` the one outside - the export contract PLAN
§10.13 reserves for cohesive and split-node tooling.

### Welded sheet cuts (G6-5)

A sheet has no inside, so §6's sides cannot come from S6 - it refuses to classify one,
and must, since a sheet claiming volume is the error that guard exists to prevent.
They come from the cut instead: **two parent nodes are on the same side iff the edge
between them is not crossed**, so a union-find over the uncut edges recovers the two
sides with no geometric test at all. Both sides then keep the parent's record, and
only the tagged interface distinguishes them - the C0 semantics of §10.13.

Exactly two classes is the condition. One means the sheet ends inside the cell (an
interior rim, `split_R`'s case) and three means the crossings do not describe a single
surface; both escalate to the conforming fan. `split_R` itself is implemented and
tested; the **rim-node construction** it needs is deferred to G7-2.

### Measured (the G4 review scene)

| | |
|---|---|
| cells cut | 34,897 of 268,948 (383 A / 762 B / 22,641 C / 11,111 D) |
| elements | 268,948 -> 408,518 |
| interface faces | 46,008 |
| escalated to the fan | 1,423 (0.53 %) |
| worst volume error | 9.5e-15 |
| `[V3]` | 0 hanging nodes, 0 non-manifold edges, 0 boundary leaks, 0 negative volumes |
| `[V4]` | worst AR 1211.7, min dihedral 0.597 deg, 42,100 below the low-dihedral gate |

`[V4]` warning after S8 is **expected and is S9's budget**, not a defect: SPEC §6 says
in as many words that cutting produces slivers by construction and that those numbers
are the input to S9, not an acceptance gate.

### G6-6 - the post-snap quality gate

The half of G4-3 that needed S7 and S8. Every element is grouped by the template of
the cell it came from, and the two populations compared at each stage against the
corrected 35.264 degree baseline (SPEC §3.7 rev 1.2):

| stage | Freudenthal worst / p5 / median | centroid fan worst / p5 / median |
|---|---|---|
| pre-snap | 45.000 / 45.000 / 45.000 | **35.264** / 45.000 / 45.000 |
| post-snap | 45.000 / 45.000 / 45.000 | 35.073 / 44.165 / 45.000 |
| post-cut | 0.874 / 5.522 / 45.000 | **1.848 / 6.474 / 45.000** |

**GO.** After the cut the fan cells are *better* than the Freudenthal cells on both
the worst case and the fifth percentile: the slivers come from where the surface
grazes a cell, which is independent of the template, and the fan's smaller tets give
the cut less room to graze rather than more. The reference-verbatim SAMR fallback is
not activated.

Acceptance (`tests/meshgen_cut_tests.rs`, 19 tests; `tests/meshgen_quality_gate_tests.rs`,
1): SNK invariant under every rotation and reflection of a quad; Theorem T2 over
20,000 random key orders, and exactly the two cyclic rows rejected; the prism table
tiling the prism by a *boundary-identity* check (a volume comparison would need a
reference decomposition, which is the thing under test); the orientation fix shown to
be necessary and sufficient; every §6 case partitioning the parent to rounding, with
the pieces never spanning the cut; **the conformity claim - the cut's triangles on
every parent face equal §5.2's - over 19,200 faces**; two cells sharing a face
agreeing; the face table tiling the face in `split_2` and `split_R`; a dangling cut
refused; the dry-run catching a cut that does not fill its parent; end-to-end
conformity on a smooth surface *and* on a plate thinner than one element; volume and
orientation preserved end to end; the interface index two-sided and derivable; a
welded sheet cut without changing any ownership; determinism across thread counts;
`s08` verifying clean; and the G6-6 gate measurement above.

### Contact planes: what S2 knows and S8 does not

Two solids in exact face contact share one mesh face that is both their boundaries.
S2 sees this and tags the arranged face with **both** components
(`ArrangedFace::components`); A-6's cube and limb share the plane x = 0.5817, where
S2 emits five faces tagged `{1, 2}`. Everything between S2 and S8 then reads the
single `ArrangedFace::component`, so the shared boundary reached the cut as one
body's and the mesh face between the two bodies was declared for neither.
`CutOptions::contact_patches` now carries S2's coincident patches through, and
`declare_contact_components` declares every mesh face lying on one for **all** the
components that patch belongs to.

The rule is deliberately **geometric, not record-based**. Declaring any face whose
neighbours' inside-sets differ would make `[V6]`'s coincidence exemption vacuous -
every escalation chamfer would declare itself legal. A face is declared only where
two input surfaces are genuinely coincident; a two-component step anywhere else still
fails, as it should.

**Making the plane a mesh face came first, and needed two fixes of its own.**

The measurement that pointed at them: of a6b's 3,156 two-component steps, **zero lay on
the contact plane** - they spanned a sliver straddling x = 0.5817, and the mesh had no
face on the plane at all.

1. **S7 crossed the plane for one body only.** `gather_geometry` read the single
   `ArrangedFace::component` and dropped S2's `components`, so the second solid's boundary
   there produced no crossing and S8 never cut it. It now emits the triangle once per
   component. The two coincident crossings an edge then carries must share **one** node
   id (`contact_edges` in `cut_lattice`) or they are duplicate nodes and a crack; with one
   id the per-edge maps are identical, `face_is_single_patch` reads "one surface reached
   twice", and the face takes an ordinary §5.2 row. `contact_edges` stays distinct from
   G7-2's `collapsed_edges` - both make a cell one surface for §6's purposes, but only a
   collapsed edge is a sheet with a rim to declare.
2. **The tag pass filtered by tet, not by face.** `declare_contact_components` required
   all four nodes of a tet inside a patch's inflated box. A patch is a triangle in a
   plane, so that box has no thickness and a tet touching the plane always has an apex
   `h` away - nothing ever qualified, and the pass stayed inert even once the faces
   existed. Testing the face's three nodes is what made it fire.

**Measured: 62,216 mesh faces on the contact plane (was 0), escalations 5,020 -> 3,579,
`[V6]` A-6b 3,352 -> 1,216 and A-6a 927 -> 461, A-6b component 2 volume error
0.031 % -> 0.026 %**, `[V3]` clean, R-P2 bit-identical.

A third fix followed from measuring what was left. A contact region is several arranged
patches, and `declare_contact_components` demanded all three corners of a mesh face on
**one** of them - so a face straddling two adjacent patches went untagged. Each corner
need only lie on *some* patch, with the component sets required to agree so two unrelated
contacts that happen to touch are not unioned: `[V6]` 1,216 -> **1,026**.

**Where the residue now is.** All 3,211 raw steps are `{1}` against `{2}`. **2,185 lie
exactly on the plane** and are exempt once tagged; the remaining **1,026 sit in a sliver
x in [0.5813, 0.5829]**, bounded by the limb's cross-section - that is the *boundary
curve* of the contact patch, and the mesh does not conform to it (`curve coverage 1 of
35`, with K1 capped at three passes and `h` already at its 0.003464 floor). Unlike A-3's
circular intersection this rim is a rectangle, so refinement can in principle reach it;
that is the open work, and it is curve capture rather than tagging.

## G7-1 thin regimes: the band templates and the FEM-aware ladder (S8b)

`src/meshgen/thin.rs`. S8 cuts a cell wherever *one* active patch passes through it.
A **band region** is the case that defeats it: two walls of one gap pass through the
same cell, closer together than the cell is wide, so the generic path sees a junction
and hands the cell to the conforming fan - valid, but it chamfers the gap away. A band
region is meshed instead by a single layer of elements spanning the gap.

### The one structure everything is derived from

A band cell is three matched pairs `(a_i, b_i)`, each either surviving or collapsed to
a rim node `r_i`. Its **boundary** is a pure function of those pairs and of one
diagonal flag per pair-edge: cap A, cap B, and one quad per pair-edge, with collapsed
vertices substituted. The templates, the Steiner fallback and the volume check are all
derived from that single construction, which is what makes a band layer conform to
itself with no negotiation - two cells sharing a pair-edge build the same quad from the
same nodes under the same rule.

**Collapse lives on the pair, not on the cell** (`SPEC_meshgen_geometry.md` §8.1,
Invariant B1). A pair collapsed in one cell is collapsed in every cell that references
it, so neighbouring cells with different `k` agree on their shared quads automatically.

### The frozen table (§8.2)

| `k` | Cell | Tets |
|---|---|---|
| 0 | prism, quads split by Rule SNK | 3 |
| 1 | pyramid over the surviving quad, apex `r_i` | 2 |
| 2 | single tet `(r_i, r_j, a_l, b_l)` | 1 |
| 3 | degenerate - the cell *is* the sheet triangle `(r0, r1, r2)` | 0 |

The nominal cell of §8.2 rev 1.3 - a right-isoceles cap of legs `h` extruded by `t`,
at `t/h = 0.35` - measures min dihedral `18.281° / 19.561° / 19.853°` and max AR
`2.0953 / 2.0165 / 1.9662` for `k = 0 / 1 / 2`. Rev 1.2's AR column was wrong and was
corrected by this subtask; see §14 [9] and §15 D-14 of the freeze.

### The §4.4 runtime ladder - and the rung the cut skips

Every emitted tet is checked: positively oriented, min dihedral at or above the floor
(8° by default), and the children reproducing the volume the cell's own boundary
encloses. Then, in order:

1. **the table row** under Rule SNK;
2. **a diagonal flip** - and this is the rung `cut.rs`'s §4.4 ladder deliberately
   skips. A flip is *not* a pure function of the quad, so a cell taking one alone
   leaves the neighbour that shares it non-conforming. `mesh_band_layer` owns both
   incident cells, so it proposes the flip for the pair and accepts it only if both
   still mesh - the reference thin-feature design §3.8's "attempted only pairwise";
3. **the Steiner fan** - the cell's own boundary coned to its centroid, 8 tets for a
   full prism, still exactly one geometric layer. No dihedral floor applies: this rung
   exists because the floor could not be met, and its output is valid rather than good.

A region whose Steiner share passes 5 % is demoted whole to volumetric with a
`[THIN-SKIP]` warning, and allocates nothing.

### The FEM-aware ladder (PLAN §10.11)

`predict_band_quality(t, h)` measures the nominal cell over all six non-cyclic diagonal
patterns with the same `tet_quality` the verifier uses - the plan's closed forms
("altitude ≈ t, min dihedral ≈ atan(t/h)") are the right scalings but not the right
numbers, and the numbers are what the gates compare against. `band_ladder` then takes
the frozen rungs in order:

| Rung | Outcome | Condition |
|---|---|---|
| 1 | `Band` | the prediction passes every gate |
| 2 | `RefineLocally` | one extra level (`h/2 ≥ h_min`) would pass |
| 3 | `Sheet` | `t ≤ t_sheet(x)` |
| 4 | `Volumetric` | representable but poor, and the user did not forbid it (WARN) |
| 5 | `Reject` | nothing remains; the reason names the gate that failed |

Gates: `band_min_dihedral_deg` (8°), `band_max_ar` (20), and an altitude floor that is
off under the `implicit` profile and `0.05 · h_local` under `explicit` - a documented
*geometric* proxy for a stable time step, since a true `Δt` needs material data the
mesher does not have. All four live in the `meshgen.thin` config block, together with
`regional_failure_share`, `forbid_volumetric_fallback` (which removes rung 4) and
`collapse_sheets` (G7-2's rim collapse, off by default).

### `[V7]`'s band half

The verifier's one-layer check is expressed through *nodes*, which is what keeps it
VTU-only: a band spans its gap with one layer exactly when no node of a band element
lies strictly inside the gap - every corner is on a wall, and a wall is a tagged face.
A `regime = 2` (Steiner) element is allowed exactly one interior node, its own apex.
`[V7]` also reports band element and Steiner counts, the band's dihedral and
aspect-ratio ranges, the band-region inventory and any `[THIN-SKIP]` regions.

### The ordering key, and why it is finer than the weld grid

`SPEC_meshgen_geometry.md` §1.2 asks two things of a `NodeKey`: that it depend only on
position, and that distinct nodes have distinct ones. The second is what makes "smallest
node key" a *total order*, and every conformity argument in §4 rests on that.

Lattice nodes satisfy it by construction. **S7's crossings do not** — they are
constructed points, they are not welded against each other, and two of them can land
closer together than the weld quantum. When that happens Rule SNK ties, the tie falls to
whichever node the caller listed first, and two cells sharing a quad list it in opposite
orders — so they split it on opposite diagonals and the mesh gains a pair of
single-sided faces. On the reference dataset that was 60 boundary leaks from one
collision class.

S8 therefore builds its key table on `KEY_ORDER_REFINEMENT` (`1e-6`) times the weld
quantum. The order stays a pure function of position, never consults a node index, and
lattice keys stay exact. Frozen as Rule K-O (§1.2 rev 1.4).

**Welding the tied pair is not the alternative.** It was tried: welding by key also
merges crossings on *different* edges and collapses the cells between them — 290
multi-shared faces and 580 non-manifold edges, where the finer key gives zero of both.
The weld invariant is a statement about welded nodes; constructed points need a finer
ordering key, not a coarser identity.

### The doubly-cut face rule, and the three-slab split

`SPEC_meshgen_geometry.md` §5.2 covers a face cut **once**. A band cell's faces are cut
twice - one crossing per wall on each crossed edge - and §7's loop fan cones such a
face to a single centroid, producing triangles that span the gap. That is why a cell
with a thin gap through it used to come out as one solid blob with the gap chamfered
away.

`band_face_split` is the missing rule: a face whose two crossed edges each carry an A-
and a B-crossing splits into the corner triangle, the strip between the walls, and the
remainder, the two quads taken on Rule SNK. Like §5.2 it is a pure function of the
*face*, so both incident cells compute it identically.

**It is applied per face, in every escalated cell** - not only inside cells the
cell-level split covers. That ordering is load-bearing rather than tidy: such a face is
shared by two cells and only one of them may be a sandwich, so a per-cell rule makes
them disagree and puts hanging nodes exactly where the gap is.

`split_band_cell` then splits a sandwiched cell - three doubly-cut faces meeting at one
parent vertex, one uncut face opposite - into three closed slabs, each closed by
`close_open_surface`. The lid of a slab *is* the wall, built by reversing the slab's
unmatched directed edges, so the two slabs on either side of a wall receive lids that
are exact reverses: one shared face, not two coincident ones.

A cell the rule does not cover declines with a counted reason (`BandDecline`) and takes
the existing fan, so the improvement is strictly additive and a run always reports why
the rule did not fire.

### Diagnostics

Two environment variables make S8's conformity work debuggable without a rebuild:

| Variable | What it prints |
|---|---|
| `RUSTMSPT_CUT_DIAG=1` | A `[CUT-DIAG]` block attributing every untagged single-sided face to the escalation reason of the cell that made it and to the node classes it is built from (parent / cut / face-centroid / cell-centroid), plus a few example mismatch pairs. Computed inside `cut_lattice`, where `parent_of` and the escalation list still exist - `[V3]` runs on the written VTU, where that provenance is gone. |
| `RUSTMSPT_CUT_CELL=<ids>` | A `[CUT-CELL]` dump of the named lattice cells: parent tet, component, sides, ambiguous and crossing component sets, the cut node on each edge, and every node's coordinates and key. This is what identified the key collision behind Rule K-O. |

`RUSTMSPT_S67_DIAG=1` prints one line per S6/S7 disagreement, tagged by direction (`straddles-no-crossing` vs `crossing-no-straddle`) with the two side labels. It is the diagnostic that says *which* of the two stages to suspect: on a strut lattice, 77.6% of disagreements are an edge S6 says changes sign that carries no crossing.

### How it reaches production

The chain is: `band_face_split` triangulates every doubly-cut face in an escalated
cell; `split_band_cell` turns a sandwiched cell into three closed slabs and reports the
gap slab's three matched pairs; the gap slab goes to `mesh_band_cell` and takes a
frozen §8.2 row, falling back to the Steiner fan only when the table or the `§4.4`
floor refuses it; the two lids are tagged into the interface index, and `regime`
carries 1 (band) or 2 (band-Steiner) into the VTU so `[V7]` measures real output.
`band_ladder` runs once per thin region against the `meshgen.thin` gates and reports
its rung; under `forbid_volumetric_fallback` a rejected region fails the run with the
gate that rejected it.

Measured on a two-plate fixture with an unresolvable 18-unit gap: 82 sandwiched cells,
gap slabs `prism x32 / steiner x50`, 496 band elements, `band_stacked_elements = 0`,
`[V7]` PASS.

## G7-2 collapsed-sheet integration: regions, tags and side labels

**Thin-region ids reach the mesh.** `gapfield::thin_context` reduces S3's regions to
lookups by *arranged face* — the region owning it, that region's effective regime
(after the S3/S4 coupling and the §10.11 ladder, which is not always
`ThinRegion::regime`), and its pair class — and the result rides on
`CutOptions::thin`. Every cut node knows the arranged face its crossing was found on,
so a band element can name the gap it spans: `CutMesh::band_region` is written per
element and emitted as the `band_region` cell array. **Both** walls of a region are
mapped, not just the wall it grew from, so a band cell resolves the same id from
either lid. `[V7]` now reports `band_regions = 1` on the two-plate fixture, where it
had reported `0` on every mesh S8b ever produced.

**Face tags.** `cut_to_doc` had hard-coded `FaceTagKind` to `0`, so nothing S8 emitted
was ever tagged a sheet and both `[V7]`'s sheet metrics and `[V8]`'s pinhole check were
measuring an empty set. The kind is now written from the component: a declared sheet
component gives `FACE_TAG_SHEET`, and so does any face all of whose corners are rim
nodes, because the collapse has fused the two walls of a gap along it and what is left
is an embedded welded sheet whatever the bodies on either side were declared as
(§10.13). A solid-plus-embedded-sheet fixture reports 220 sheet faces, `unwelded = 0`.

**Side labels.** A sheet owns no volume, so S6 writes it no ownership entry and
`side_of` answers `Outside` for *both* neighbours — which silently degraded
`(elem⁺, elem⁻)`, the pair §10.13 reserves for cohesive and split-node work, to
`(-1, one of them)`. Sheet faces now take their sides geometrically: the neighbour whose
fourth node lies on the positive side of the face's own plane is `elem⁺`. The face's
nodes are key-sorted, so both incident cells compute the same normal and agree on which
is which — the same argument Rule SNK rests on. Solid patches keep the ownership
convention.

**Pair classes.** `ThinRegionPairClass` (`SPEC_meshgen_contracts.md` §2.3) is emitted on
`s03` and on `s08`, together with `ThinRegionRegime`; on `s08` it is indexed by a band
element's `band_region`, so a consumer of the mesh can say what kind of gap an element
spans — a solid–sheet contact, two sheets, two faces of one body — without holding on
to `s03_gapfield`.

**Rim collapse: `meshgen.thin.collapse_sheets`.** A lattice edge whose two wall
crossings belong to one `Sheet` region and lie within `t_sheet` carries a **single
shared rim node** instead of two, so no cell ever sees the gap, every face over it takes
an ordinary §5.2 row, and conformity is the single-patch argument again rather than
something to re-establish. This is §8.2's `k = 3` row — "the cell *is* the sheet
triangle; its faces come from the sheet cut" — read forwards. Doing it at the cut node is
load-bearing: welding the walls *after* the cells are meshed costs 27,336 hanging nodes,
because by then the two walls have triangulated their shared faces as a doubly-cut pair
and every neighbour that split the same face once disagrees about it.

A cell whose crossings are *all* collapsed then takes §6's table once
(`welded_pair` in `cut_one_cell`) rather than escalating, with `cut_record` settling the
second solid as the complement of the first — there is no void left between them for a
child to be outside of both. Making that conform required fixing `face_states`, which had
marked a face inexpressible whenever **two components** touched it. That is the right
test for two patches and the wrong one for a collapsed face, where both walls cross the
same edges at the *same node ids*: the face carries two components and is an ordinary
single-patch face. `face_is_single_patch` now compares what each component actually
contributes per edge, which tells one-surface-reached-twice from two-surfaces.

**The collapsed sheet's rim** is emitted as a declared curve (`CurveKind = 1`, as
`VTK_POLY_LINE` cells with `curve_id`). `[V8]` requires a sheet's boundary edges to be
declared curves precisely so the boundary it *cannot* explain — a pinhole — still fails,
so the rim is derived from the collapse **decision** rather than from the emitted faces:
a cell holding both a collapsed edge and an uncollapsed crossing straddles the region
boundary, and only the rim nodes it carries qualify (`collapsed_sheet_rim`). A hole in
the middle of a sheet has interior endpoints, is not declared, and still fails.

Measured with `collapse_sheets: true` on the two-plate fixture: `[V3]`, `[V7]` and `[V8]`
all PASS — 2,592 sheet faces, `unwelded = 0`, 144 rim curve cells, 0 pinholes —
R-P2 byte-identical, `[V4]` unchanged. **It defaults `false`** on validation scope
alone: the evidence is two synthetic fixtures, the acceptance case that would confirm it
at scale (§17.4 A-7, the near-contact gap sweep) is not built, and where it fires it
changes meshes substantially — 3,888 band elements become a 2,592-face welded sheet.

**The §10.11 ladder governs the regime.** Its outcome had been reported and then dropped
while `effective_regimes` still carried what the thresholds said, so a region the ladder
demoted at rung 4 would still have been banded or collapsed. Rungs 1/3/4 now write
`Band`/`Sheet`/`Normal` back before S4's LFS sources and S8b read them. Rung 2 cannot be
honoured — re-running S4 at a finer `h` is not built — so the region keeps its declared
regime and the rung is reported.

**The open sheet's rim** — the boundary of a sheet that arrives as an STL and ends
inside the mesh — is declared by the same path. It was never a geometry problem: S7
already takes rim curves as snap targets (every `ArrangedCurveKind` except `Box`), so the
cut front already terminates *on* the rim. Measured on the solid-plus-sheet fixture, 19
of the 22 boundary-edge midpoints sit exactly on it, the other three being the chords
that cut the square rim's corners. What was missing was the declaration: `cut_to_doc`
emitted no curve, so `[V8]` had nothing to match a sheet boundary against.

A sheet boundary edge is declared a rim when **both** endpoints lie within `eps` of an
arranged **rim** curve (`nodes_on_rim`). Two details keep the check honest. Membership is
geometric rather than read off S7's `constraint_ref`, because a node *already* exactly on
a curve gets no proposal and so no `Polyline` constraint is recorded — and because a
geometric test cannot quietly become a rubber stamp. And only `Rim` curves qualify: a
sharp or intersection curve is not a place a sheet may end, and accepting one would let a
real pinhole past. The fixture now PASSes with 22 declared rim cells and 0 pinholes.

`split_R` is therefore implemented and unexercised, which is the right outcome rather
than a gap: its row is for a cut front that ends at an interior point of a lattice
*face*, and snapping puts the end on a lattice *node* instead — an ordinary §5.2 row. It
remains the fallback for a rim too coarse to snap, where the cells concerned escalate to
the conforming fan and are logged.
- **A straight sharp edge and an intersection curve drive refinement through
  `Curve`, not through the chord rules.** `curvature_sources` and `feature_sources`
  both measure how much geometry *bends*, and neither of these bends at all - a cube
  edge is straight, and two surfaces crossing transversally curve not at all - so both
  emitted nothing and the field never refined toward the very curves the mesh has to
  reproduce. Measured before `curve_sources` existed (2026-08-07): A-1, A-2, A-3, A-4
  and A-7 all reported `geometry sources: 0 total` while S1 was reporting 12 to 24
  curves, and A-8 reported 0 feature-curve sources against 180 sharp curves. Adding the
  criterion moved A-7's component volume error from 13.6 %/7.5 % to 0.30 %/0.06 % and
  A-8's from 9.40 % to 4.77 %.
- **`curve_cells` interacts with the thin regimes.** `t_sheet` and `t_layer` are
  fractions of the *converged* `h`, so any criterion that lowers `h` narrows the sheet
  window. At `curve_cells = 2` the window halves. A fixture built to land in a named
  regime has to be sized against the converged `h`, not against `h_max`; `curve_cells:
  1.0` disables the criterion where that matters more than curve capture.
## Session close 2026-08-13 - gate G6-0 re-opened, and the residue was face-level

`[V6] region_adjacency` **1,944 -> 148** across the four failing acceptance cases, with
`[V1]` and `[V3]` clean on all nine and **seven of nine passing every check that runs**.

| case | tets | `[V6]` adj (was) | volume error % |
|---|---|---|---|
| A-1 | 31,536 | 0 | 0.911 |
| A-2 | 111,756 | 0 | 0.000 |
| A-3 | 160,711 | **80** (122) | 0.711 / 0.392 |
| A-4 | 1,740,414 | 0 | 0.052 / 0.026 |
| A-6a | 240,542 | **21** (670) | 0.001 / 1.688 |
| A-6b | 985,174 | **47** (1,018) | 0.001 / 0.013 |
| A-7a | 141,186 | **0** (134) | 0.447 / 0.129 |
| A-7b | 113,550 | 0 | 1.069 / 1.257 |
| A-8 | 265,266 | 0 | 4.252 |

The gate was re-opened to build constrained curve recovery inside the cell. **That was not
what the residue needed.** Every violation traced to a *lattice face* whose triangulation did
not follow the material boundary, and §7.6 itself is unchanged.

- **The `LoopFan` apex had lost its curve-pierce lookup.** The comment described it; the code
  called `face_centroid` unconditionally while the `meeting_nodes` registration below still
  declared that centroid to be on every surface. A-3 122 -> 101, A-6a 670 -> 49, A-6b
  1,018 -> 75.
- **A locked curve reaches S8 as a polyline, and excluding both segment endpoints drops every
  crossing that happens at one of its own vertices** - the segment before reports `t = 1`, the
  one after `t = 0`, and no "next segment" contains it. A-6a 49 -> 37, A-6b 75 -> 60.
- **A corner S7 snapped onto a patch is an endpoint of that patch's trace**, and
  `walk_positions` counted only cut nodes - so such a trace had one endpoint, could not be
  paired into a chord, and the face coned to its centroid. **A-7a 134 -> 0**, A-3 101 -> 79,
  and A-3's face-level refusals go to zero.
- **`cell_fan_is_conforming`** asks `[V3]`'s two questions of one escalated cell before it
  commits, because two sibling pieces' centroids are averages over overlapping node sets and
  land coplanar more often than chance suggests.
- **The `mixed sides` relaxation ships**, with the leak it actually causes - a cap landing on
  the cell's own outer boundary, which the neighbour fans too - refused explicitly. A-6a
  37 -> 21, A-6b 60 -> 47.

Refuted and removed: seeding per **slab** rather than per tet (A-7a 0 -> 484 and its volume
error 0.40 % -> 4.17 %), grouping walk positions by which components cross there, and
preferring the shared-endpoint hub fan over the pierce apex (A-6a 37 -> 269).

**Diagnostics.** `RUSTMSPT_CUT_DIAG` now also writes a `parent_cell` cell array into the s08
snapshot, which is what separates a defect inside one escalated cell from a disagreement
between two. `RUSTMSPT_CURVE_PROBE=x,y,z` lists the locked curve segments near a point - in
the **normalized** frame, so multiply a VTU coordinate by `1/sqrt(3)` on a unit-cube domain.
### Follow-up: the second way a curve reaches a face

`segment_pierces_triangle` is strictly interior by design - a crossing on the face's own edge
is a node both cells already share - so a curve passing through a **walk node** yields no
piercing point and the face falls back to its centroid. S7 snaps nodes onto locked curves
deliberately, so near a rim this is the common case. `nodes_on_curve` names those nodes and
`fan_from_walk_node` fans the walk from the one that is on a curve, interning nothing: both
cells derive the same apex from the face alone, so J1 holds exactly as it does for the
piercing point. It fires on 4 faces of A-6a and 16 of A-3, is neutral on `[V6]`, and takes
A-8's volume error 4.252 % -> 4.245 %.

`RUSTMSPT_JCT_DIAG` now prefixes every `[JCT-DIAG]` line with the lattice cell index, so a
violation located through the s08 snapshot's `parent_cell` array traces straight to the
refusal that caused it.
## `[V9] Junctions` — implemented 2026-08-13

`[V9]` had been `SKIPPED` since the check catalog was written, with the reason "lands with
G6-4; needs the curve radial-order table" — while **G6-4 is marked done and its stated
acceptance is "`[V9]` clean on all junction fixtures"**. It now runs on all nine acceptance
cases.

| case | `[V9]` | curves declared | carried | mesh edges | missing `N_ID` | radial | open fans |
|---|---|---|---|---|---|---|---|
| A-1 | PASS | 0 | 0 | 0 | 0 | 0 | 0 |
| A-2 | PASS | 12 | 12 | 408 | 0 | 0 | 0 |
| A-3 | **FAIL** | 114 | 40 | 134 | 1 | 0 | 0 |
| A-4 | PASS | 12 | 12 | 462 | 0 | 0 | 0 |
| A-6a | PASS | 34 | 21 | 751 | 0 | 0 | 0 |
| A-6b | PASS | 34 | 22 | 2,273 | 0 | 0 | 0 |
| A-7a | **FAIL** | 24 | 16 | 186 | 3 | 0 | 0 |
| A-7b | **FAIL** | 24 | 18 | 254 | 43 | 0 | 0 |
| A-8 | **FAIL** | 972 | 149 | 1,216 | 69 | 0 | 0 |

### The three clauses

1. **A curve node's `N_ID` contains the curve's components.** `PLAN §5.3` defines `N_ID` as
   the union of the incident elements' resolved labels and the incident face tags, so a node
   on the curve where components 3 and 5 meet must read `{…, 3, 5}`.
2. **The radial patch count matches.** S2's `radial_patch_order` says how many patches it put
   around the curve, so the mesh must show that many material sectors around an edge lying on
   it. Asked only where **two or more** components meet — around a sharp edge of one solid the
   sectors are inside and outside whatever the patch count is.
3. **The edge fan closes.** Every face incident to a junction edge is carried by exactly two
   of the tets around it. Not asked of an edge incident to a face the mesh owns once: an edge
   on the mesh's own boundary cannot close.

A curve the mesh carries no edge for is reported `INFO`, not failed — gate G6-0 adopted the
conforming fan over constrained edge recovery, so the mesh is not required to reproduce every
curve as a chain of edges.

### What had to be built first

The curve table was **empty** (`CurveCompOffsets`/`CurveCompComponents` written as zero-length
arrays against a non-empty `CurveKind`), and `n_id_key` was a **stub of zeros** — against which
clause 1 would have passed vacuously. S8 now carries S2's curves whole (`LockedCurve`),
`curve_mesh_edges` finds the mesh edges along each (both endpoints *and* the midpoint, because
endpoints alone accept a chord that leaves the curve and comes back), `n_id_key` is computed as
§5.3 defines it, and `CurveRadialPatches` was added to the contract as an additive column.

One curve cell is emitted per **edge**, not per (curve, edge): two S2 curves run along the same
geometry wherever a contact rim is both bodies' own sharp edge, and emitting the edge twice is a
`[V1].duplicate_cell`.

### What it found

A-7b's node 2524 at (0.2017, 0.2017, 0.4217) is the lower plate's own corner, and **all 24
incident tets are background** — the corner's material is gone. A-3's single failure is a node
on the sphere/cube intersection curve reading `{0, 1}`. And the reference fixture `good_cube`
was itself wrong: it declared a component-1 feature curve along nodes 0-1-3, where its own
region layout puts no component-1 material.

Negative fixtures `bad_curve_node_id` and `bad_radial_patches` prove the first two clauses can
fail. Clause 3 reports 0 on all nine cases and has no negative fixture yet.
### Closing `[V9]`'s own limits

**Clause 3 now has a negative fixture, and building it found a bug in the clause.** The
fan-closure test exempted an edge incident to any singly-owned face — but an open fan *is* a
fan with a singly-owned face, so the exemption made the clause unable to fire in the case it
exists for, and the acceptance matrix (0 either way) could not have shown it. The test is now
`on_domain_plane`, the same predicate `[V3]` uses. `bad_open_junction_fan` fires
`V9.junction_fan` with `V3.boundary_leak` as the justified cascade.

**`constraint_kind` / `constraint_ref` were stubs** in the cut output (`vec![0]` / `vec![-1]`)
while S7 had computed both — the third contract array found emitted with no content, after
`n_id_key` and the curve table. Carrying them through S8 names every `[V9]` finding: **each
failing node is one S7 snapped onto a locked curve** (A-7b 34 `Polyline` + 3 `Corner`, A-8
43 + 4), with the nearest material about one element away. Refuted first: they are not
near-duplicate nodes (0 of 84 have a neighbour within 1e-6), and their cells are not simply
uncut.
## The S7/S8 defect `[V9]` exposed — root cause and fix (2026-08-14)

`[V9]` passes on eight of nine acceptance cases; only A-3 fails, on one node.

**Root cause.** §6's cell table has no row for a cell every vertex of which lies *on* the
patch, and S7's snapping makes that configuration systematic. The mirror case was already
fixed — `has_inside && !has_outside`, a cell entirely inside because every *other* vertex sits
on the patch, recorded at 12.9 % of a box's volume. The case with **no** `Inside` vertex was
still falling through to `uncut`, which hard-codes `inside: false`, so the cell's material went
to background. It is invisible to every topological check: the mesh stays watertight and
conforming, it just contains less of the body than it should.

Read off A-7b's lower-plate corner (0.2017, 0.2017, 0.4217): all fourteen 1-ring neighbours
carry a snap constraint, **not one of them is strictly inside the plate**, and the tet whose
interior fills the corner octant has all four vertices on the boundary.

**The fix.** `cut_one_cell` samples the cell's interior for exactly that configuration — no
`Inside` vertex, at least one `OnCut` vertex, no cut edge — using the same `PointClassifier`
§7.5 uses for an escalated piece. New case code `b'i'`, reported as `[S8/G6-2] N cell(s) had
every vertex on the patch`.

| | before | after |
|---|---|---|
| `[V9]` A-7a / A-7b / A-8 | 3 / 43 / 69 | **0 / 0 / 0** |
| A-8 volume error | 4.245 % | **2.850 %** |
| A-7b component 2 | 1.257 % | **0.090 %** |
| A-6a component 2 | 1.687 % | **1.416 %** |

It fires on 343 cells of A-8, 118 of A-7b, 10 of A-6a, 8 of A-7a, 0 of A-3/A-4. Tet counts are
unchanged — a relabelling, not a remeshing, which is why `[V1]`, `[V3]` and `[V6]` do not move.

**Why S8 and not S7.** Rejecting the snap was the other candidate and it is worse: the capture
is correct, the node *should* be on the curve, and refusing it gives up the feature to keep a
vertex the table happens to be able to read. The table was what was incomplete.

**Remaining.** A-3's node 21054 on the sphere/cube intersection curve reads `N_ID = {0, 1}`
against a curve declaring `{1, 2}`. The fix does not reach it: the cell *is* cut, by
component 1, so component 2's entry is inherited through `cut_record` rather than decided.
That is the same chamfer residue as A-3's `[V6]`, not a separate defect.
### A-3's last `[V9]` node — bounded by the envelope

Node 21054 is a **cut node** (`constraint_kind = 0`), created where the sphere crosses edge
(7308, 7347) — unlike A-7/A-8's failures, which were S7-snapped nodes. It sits **5.30e-6 inside
the cube** against `eps = eps_frac · diag = 1.73e-5`, i.e. at **0.31 eps**.

Its four parent cells all report `record [(1, Ambiguous)]` — **component 2 is not in S6's record
at all**, because no vertex of any of them is strictly inside the cube (`per-node ["on", "on",
"out", "out"]`). Node 7347 sits exactly on the cube's face (`0.2886751345948129` normalized is
`0.5/√3` to the digit) and 7308 is 8.4e-6 inside it, while the cell's centroid is 1.2e-3
*outside* — 70 eps. The cube's material there is a corner sliver at the envelope scale, and the
interior sample that fixed the other 115 nodes answers "outside" correctly.

**Tried and reverted:** extending the junction triage to `on_cut` (a surface passing through a
cell's vertices leaves no crossing, so neither the record nor the crossings name it), with the
record augmented so §7.5's seeding considers the hidden component. It fires on 4 cells of A-3
and changes no metric, at +13 tets. The gap is real; the centroid sample is too coarse to
exploit it, because the configuration that triggers it is exactly one where the hidden body
occupies a sliver.

Kept: `RUSTMSPT_CUT_CELL` now prints the cell's parent record and a per-component `in`/`on`/`out`
view of its vertices.
