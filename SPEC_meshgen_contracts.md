# SPEC — Mesh generation: data contracts freeze (subtask G0-3)

**Status:** frozen (rev 1.1, schema v1; additive G2 diagnostic orientation field recorded 2026-07-28). Normative for `src/io/vtu.rs`,
`src/meshgen/verify.rs`, `src/meshgen/render_scene.rs`, and every producer of a
contract VTU.
**Date:** 2026-07-28
**Subtask:** G0-3 (Phase G0, tier T2) of [`PLAN_mesh_generation.md`](PLAN_mesh_generation.md).
**Scope (from the plan's G0-3 acceptance):** §7 VTU schema (array names/types/
sentinels, tables, metadata), snapshot naming, §8 check catalog + severities +
JSON schema, §15 accuracy table. *Acceptance: schema doc + fixture VTUs
hand-written to it.*
**Companion freezes:** [`SPEC_meshgen_geometry.md`](SPEC_meshgen_geometry.md)
(G0-1), [`SPEC_meshgen_numerics.md`](SPEC_meshgen_numerics.md) (G0-2).

The fixture suite required by the acceptance criterion is committed at
[`data/fixtures/meshgen/`](data/fixtures/meshgen) — 11 files, hand-written to
this schema, all verified to load through the real GA-1 reader, pass structural
validation, and re-emit byte-identically (§11). Writing them found two genuine
schema/implementation mismatches before any mesher code exists (§12 D-1, D-3).

---

## 0. Normativity

- **MUST/MUST NOT** are binding. This document wins over `PLAN_mesh_generation.md`
  §7/§8/§15 prose.
- Schema version is **1**. Any change to array names, types, sentinels, or table
  encodings increments `SchemaVersion` and requires a reader that rejects
  unknown majors with a named error.
- Producers MUST write every **A**-presence array. Consumers MUST degrade
  explicitly on a missing optional array ("check skipped: array absent"), never
  silently.

---

## 1. File organization

**One primary mixed-cell `.vtu`** (VTK XML `UnstructuredGrid`, single `Piece`):
`VTK_TETRA` (10) volume cells + `VTK_TRIANGLE` (5) tagged-face cells +
`VTK_POLY_LINE` (4) feature-curve cells + `VTK_VOXEL` (11) lattice-preview cells,
**all indexing one shared point array**. Shared topology is then verifiable by
point-index identity, which is the whole reason the contract is one file.

| Property | Frozen value |
|---|---|
| Root attributes | `type="UnstructuredGrid" version="1.0" byte_order="LittleEndian" header_type="UInt64"` |
| Default encoding | appended raw (`format="appended"`, `encoding="raw"`) |
| Debug/fixture encoding | ascii (`format="ascii"`) |
| Rejected on read | compressed (`compressor` attribute present), base64 (`format="binary"`) — with a named error each |
| Accepted on read | `header_type` `UInt64` (written) or `UInt32` (tolerated) |
| Appended-array offsets | collected in **emission order**: FieldData, Points, Cells, PointData, CellData |
| Cell `offsets` array | VTK **end**-offsets: cell *i* spans `connectivity[offsets[i-1] .. offsets[i]]`, `offsets[-1] ≡ 0` |
| Float formatting (ascii) | shortest round-trip (`{}`), so write→read→write is byte-stable |
| `NumberOfTuples` | **required** on every `FieldData` array; omitted elsewhere |
| `NumberOfComponents` | emitted only when `≠ 1` |

`--split` companions (`_volume`/`_faces`/`_curves.vtu`) are an optional export for
tools that reject mixed cells; each carries `GlobalPointId` mapping into the
primary numbering. They are never the primary artefact and are not verified
independently.

**External VTUs** are accepted with only points + tet connectivity;
geometry-only verification (§7, checks [V1]–[V5]) and geometry-only rendering run
without any contract array.

---

## 2. Array schema v1

Presence: **A** always · **Dbg** debug/snapshot builds · **Ann** written by
`mesh-verify --annotate`. Sentinel `−1` for signed integer "not applicable",
`255` for `u8`, `0xFFFFFFFF` for `u32`.

### 2.1 Cell data

| Array | Type | Presence | Domain / meaning |
|---|---|---|---|
| `cell_kind` | `UInt8` | A | `0` tet · `1` tagged face · `2` curve segment · `3` lattice-preview voxel |
| `region_key` | `Int32` | A | tets: index into the region-set table; `−1` on all other kinds |
| `partition_id` | `Int32` | A | tets: partition index from §5.4 flood fill; `−1` otherwise |
| `regime` | `UInt8` | A | tets: `0` normal · `1` band · `2` band-Steiner; `255` n/a |
| `face_tag_key` | `Int32` | A | face cells: index into the face-tag table; `−1` otherwise |
| `curve_id` | `Int32` | A | curve cells: index into the curve table; `−1` otherwise |
| `provenance` | `UInt8` | Dbg | dominant record provenance: `0` lattice · `1` cut · `2` arbitrated · `3` junction · `4` band |
| `arbitrated` | `UInt8` | Dbg | `1` if any record entry was arbitrated (§12 of G0-2's catalog) |
| `band_region` | `Int32` | Dbg | thin-region id; `−1` n/a. On s03 this is the G3-2 region id per wall face |
| `thin_role` | `UInt8` | Dbg | **additive G3-2 diagnostic** (recorded 2026-07-29): `0` ordinary wall · `1` wall inside a converted region · `2` mid-surface face; `255` n/a. G7 must promote it or replace it with the final sheet encoding before the first final VTU |
| `aspect_ratio` | `Float32` | Ann | `R/(3·r_in)` |
| `radius_ratio` | `Float32` | Ann | — |
| `min_dihedral_deg` | `Float32` | Ann | reported in degrees; **gates compare `cos²` algebraically** (G0-2 §8.1) |
| `scaled_jacobian` | `Float32` | Ann | — |
| `verify_flags` | `UInt32` | Ann | bitmask of failed check ids; bit *k* = check `[V(k+1)]` |

### 2.2 Point data

| Array | Type | Presence | Domain / meaning |
|---|---|---|---|
| `n_id_key` | `Int32` | A | index into the node-ID-set table |
| `constraint_kind` | `UInt8` | A | `0` free · `1` surface · `2` polyline · `3` corner · `4` box face |
| `constraint_ref` | `Int32` | A | component X or curve id the constraint binds to; `−1` when free |
| `separation_t` | `Float32` | Dbg | S3 field snapshot; per vertex, the smallest finite separation measured at that vertex or on an incident face, in output coordinates. `-1` = no pairing here (the not-applicable convention for an otherwise non-negative quantity, recorded additively 2026-07-29 with G3-1) |
| `sizing_h` | `Float32` | Dbg | S4 field snapshot |

### 2.3 Field data — tables

All set tables use the **same end-offset encoding as cell `offsets`**: set *k*
occupies `Components[Offsets[k-1] .. Offsets[k]]`, with `Offsets[-1] ≡ 0`. This
matches `VtuDoc::cell()` and `render_scene::set_members` as implemented.

| Table | Arrays | Types |
|---|---|---|
| Region sets | `RegionSetOffsets`, `RegionSetComponents` (X values), `RegionSetPriority` (one Y per key) | `Int64`, `Int32`, `UInt32` |
| Node-ID sets | `NIdSetOffsets`, `NIdSetComponents` | `Int64`, `Int32` |
| Face tags | `FaceTagOffsets`, `FaceTagComponents`, `FaceTagKind`, `FaceTagSideElems` | `Int64`, `Int32`, `UInt8`, `Int32` (2 components) |
| Face-tag orientation (G2 diagnostic) | `FaceTagOrientation` | `Int32`, one `+1`/`-1` per flattened `FaceTagComponents` member |
| Components | `ComponentX`, `ComponentY`, `ComponentKind`, `ComponentClosed` | `Int32`, `UInt32`, `UInt8`, `UInt8` |
| Curves | `CurveKind`, `CurveCompOffsets`, `CurveCompComponents`, `CurveRadialPatches` | `UInt8`, `Int64`, `Int32`, `Int32` |
| Thin regions (G3-2 diagnostic) | `ThinRegionRegime` (`0` normal · `1` band · `2` sheet), `ThinRegionPairClass`, `ThinRegionConfidence`, `ThinRegionSeparation`, `ThinRegionSkip` (`0` none · `1` low confidence · `2` speck · `3` mid-surface invalid · `4` mid-surface unbuildable · `5` intersection wedge) | `UInt8`, `UInt8`, `Float32`, `Float64`, `UInt8` |

Enumerations: `FaceTagKind` `0` interface · `1` sheet · `2` box cap.
`ComponentKind` `0` solid · `1` sheet. `CurveKind` `0` sharp · `1` rim ·
`2` intersection · `3` box. `CurveRadialPatches` is an **additive** column
(recorded 2026-08-13 with `[V9]`): one non-negative count per curve, the number of
arranged patches S2's `radial_patch_order` put around it. `[V9]`'s second clause is
specified against "the curve table" and the frozen table carried no column that could
answer it, so the check could not be written at all without this. `0` means the
producer did not order patches around that curve and the clause is not asked of it.
A curve cell's `curve_id` names **one** table entry; where a mesh edge lies on more
than one curve - a contact rim is both bodies' own sharp edge - the producer emits the
edge once under the lowest curve index, because a duplicate cell is a `[V1]` failure. `ThinRegionPairClass` `0` unpaired · `1` intra
(two faces of one component) · `2` inter (two components) · `3` solid–sheet ·
`4` sheet–sheet · `5` surface–box, mirroring S3's `PairClass` (added with G7-2;
`ThinRegionRegime` and `ThinRegionPairClass` are also emitted on `s08_cut`, where
they are indexed by the `band_region` of a band element rather than by a wall
face, so a consumer of the mesh can say what kind of gap an element spans without
holding on to `s03_gapfield`).

`FaceTagSideElems` stores `(elem⁺, elem⁻)` per **face cell**, in face-cell order —
the reserved cohesive/split-node hook (plan §10.13). `−1` marks a side with no
adjacent tet (a face on the domain boundary).

The `ThinRegion*` tables are additive schema-v1 diagnostics emitted by
`gapfield_to_doc`, one row per segmented region, indexed by `band_region`. A
mid-surface triangle is emitted as an ordinary tagged face cell carrying its
region's component with `FaceTagKind = sheet`, and is told apart from a wall face
by `thin_role = 2`. Like `FaceTagOrientation` they are not retroactively required
from the pre-G3 fixtures, and G7 must promote or replace them before the first
final VTU.

`FaceTagOrientation` is an additive schema-v1 diagnostic field emitted by
`arranged_surface_to_doc`; its offsets are exactly `FaceTagOffsets`. It records
orientation relative to the canonical emitted face and discharges coincidence
case C2 internally while `s02_arranged` is withheld. It is not retroactively
required from the 11 pre-G2 fixtures. G2-4 must either promote it to an always-
present final-contract field or derive equivalent side orientation before the
first live s02/final VTU is emitted. For a mixed sheet/solid C9 tag,
`FaceTagKind=sheet` and `FaceTagComponents` retains both identities. G2-5 encodes
C10 as `FaceTagKind=box` with the sheet X retained in the same component set.

The background region key `{0}` carries `RegionSetPriority = 0xFFFFFFFF`
("not applicable"); every other key's priority is the single Y shared by all its
X values, which verifier check [V6] enforces.

### 2.4 Field data — metadata

> **Rule C1 (numeric-only metadata).** Every metadata entry is a numeric array.
> The contract carries no string arrays: `ArrayData` has no string variant, and
> introducing one would add an encoding, a length convention, and a
> ParaView-display question for no gain. The human-readable stage name lives in
> the **filename** (§3).

| Array | Type | Tuples | Meaning |
|---|---|---|---|
| `SchemaVersion` | `Int32` | 1 | `1` for this document |
| `StageIndex` | `Int32` | 1 | stage enumeration below; `11` for a final mesh |
| `GeneratorVersion` | `Int32` | 3 | semver major, minor, patch |
| `ConfigHash` | `UInt64` | 1 | hash of the effective config; equality is checked by [V12] |
| `DomainMin` / `DomainMax` | `Float64` (3 comp.) | 1 | the domain box **in output coordinates** |
| `Counts` | `Int64` | 4 | `[points, tets, tagged faces, curve cells]` |
| `DeterminismMode` | `UInt8` | 1 | `0` strict · `1` fast (selects the [V11] comparison) |

Stage enumeration (frozen; also the snapshot index): `0` conditioned · `1`
features · `2` arranged · `3` gapfield · `4` sizing · `5` lattice · `6`
classified · `7` snapped · `8` cut · `9` thin · `10` quality · `11` final.

---

## 3. Snapshot workflow

`snapshots: none | key | all`, files at
`<output_stem>.debug/<stem>_sNN_<name>.vtu` with the frozen names:

```
s00_conditioned  s01_features  s02_arranged  s03_gapfield  s04_sizing
s05_lattice      s06_classified s07_snapped  s08_cut       s09_thin
s10_quality_r<N> s11_final
```

`key` = `{s02, s05, s08, s11}`. Surface-stage snapshots (`s00`–`s03`) contain
face and curve cells only. `s04`/`s05` previews write `cell_kind = 3` voxel cells
carrying `sizing_h`. `s10_quality_r<N>` is emitted once per IQD round `N`.

Every snapshot carries the **full** §2.4 metadata block, so the verifier and the
renderer accept any snapshot interchangeably — one contract end to end. A
snapshot whose `StageIndex` disagrees with its filename is a [V12] FAIL.
Because s00-s03 intentionally contain no volume cells, verifier checks [V7]
(sheet-to-volume welding) and [V8] (volume partitions) report **SKIPPED** with a
stage-specific reason for those snapshots; tagged surface faces are not treated
as failed final sheets.

Size control: `snapshots: all` combined with an estimate above 5 M tets emits a
WARN before the run (≈60 B/tet per volume snapshot, plan §14).

---

## 4. Verifier check catalog

Severity classes: **FAIL** (exit nonzero) · **WARN** (gated; configurable to
fatal) · **INFO**. Every threshold lives in the `verify:` config block with the
defaults below.

| § | Check | Severity | Default gate |
|---|---|---|---|
| **[V1]** Cells | negative or zero tet volume; repeated node within a cell; duplicate cells; NaN/inf coordinates | FAIL | — |
| **[V2]** Nodes | duplicate coincident nodes (`NodeKey` equality); unreferenced nodes | FAIL / WARN | — |
| **[V3]** Conformity | interior face with ≠2 adjacent tets (≥3 = FAIL); hanging nodes (node interior to a neighbour's face/edge within tol, spatial-grid assisted); boundary face off every domain plane and untagged (**boundary leak**); mismatched shared faces at region boundaries; unexpected non-manifold edge not within tol of a declared curve | FAIL (non-manifold: WARN) | — |
| **[V4]** Quality | `AR = R/(3·r_in)`; radius ratio; min/max dihedral; scaled Jacobian; min altitude; distributions + worst-10 | WARN | `AR > 20` counted; global min dihedral > 5°; share below 10° < 0.01% |
| **[V5]** Geometric conformance | interface-node surface distance; face-normal deviation; feature-curve conformance; sharp-corner error; sampled two-sided Hausdorff | WARN | §5's accuracy table |
| **[V6]** ID semantics *(contract arrays required)* | per-component volume error vs input solid; region labels ∈ legal key table; same-priority-overlap keys share one Y; priority-resolution audit (sampled `robust_inside`); ownership completeness | FAIL (semantic) / WARN (sampled) | volume ≤ 1% rel; arbitration rate < 0.5% |
| **[V7]** Sheets & thin | sheet faces with exactly 2 adjacent tets sharing all 3 nodes (**welded**); sheet-node mid-surface distance; rim conformance; **one-layer band check**; band AR/dihedral ranges; Steiner count; `[THIN-SKIP]` inventory | FAIL (topology) / INFO (counts) | mid-surface ≤ 2% h; rim ≤ 10% h |
| **[V8]** Partitions | recomputed flood fill ≡ stored `partition_id`; pinhole-leak heuristic; per-partition volume + label composition; `expected_partitions` | FAIL / WARN / INFO | `expected_partitions: null` |
| **[V9]** Junctions | curve-node `n_id_key` set ⊇ the curve's component set; radial patch count matches the curve table; multi-surface cell conformity across the shared-face cache | FAIL | — |

`[V9]` **implemented 2026-08-13**, with two clarifications the first run forced. The
radial-patch clause is asked only where **two or more** components meet along the
curve: around a sharp edge of a single solid the material sectors are inside and
outside whatever the patch count is, so the identity does not hold there. The
conformity clause is read on the written mesh as "the tets around a junction edge form
a closed fan", and is not asked of an edge incident to a face the mesh owns only once -
an edge on the mesh's own boundary cannot close, and a locked curve sitting there is
ordinary. A curve the mesh carries no edge for is reported `INFO`, not failed: gate
G6-0 adopted the conforming fan over constrained edge recovery, so the mesh is not
required to reproduce every curve as a chain of edges.
| **[V10]** Export completeness | INP↔VTU cross-check: element/node counts, per-set sums, unmapped-region audit | FAIL | — |
| **[V11]** Compare mode | strict: canonical-order arrays byte-equal, coordinates bit-identical. topology: identical connectivity/labels/tables, coordinates within `1e-6·diag`, quality within 1% | FAIL | selected by `DeterminismMode` |
| **[V12]** Provenance & stats | counts, `[OWN-STATS]`, repair-log echo, config-hash match, stage/filename agreement, memory summary | INFO (mismatch: WARN) | — |

Invocation:

```
rustmspt mesh-verify --input mesh.vtu [--config cfg.yaml] [--report out.log]
                     [--json out.json] [--annotate annotated.vtu]
rustmspt mesh-verify --compare a.vtu b.vtu --mode strict|topology
```

Exit code is nonzero on any FAIL, or on any WARN configured as fatal.

### 4.1 JSON report schema (frozen)

```jsonc
{
  "schema": 1,
  "tool": "mesh-verify",
  "generator_version": [0, 1, 0],
  "input": "data/output/mesh.vtu",
  "stage_index": 11,
  "config_hash": "0x…",
  "determinism_mode": "strict",
  "summary": { "fail": 0, "warn": 2, "info": 9, "checks_run": 12, "checks_skipped": 1 },
  "sections": [
    {
      "id": "V3",
      "title": "Conformity",
      "status": "PASS",                 // PASS | WARN | FAIL | SKIPPED
      "skipped_reason": null,           // set iff status == SKIPPED, names the missing array
      "metrics": { "interior_faces": 1024, "non_paired": 0, "hanging_nodes": 0 },
      "items": [                        // bounded; `items_truncated` when capped
        {
          "severity": "FAIL",
          "code": "V3.hanging_node",
          "message": "node 812 lies inside face (401, 655, 709)",
          "point_ids": [812],
          "cell_ids": [3197],
          "coordinates": [[0.51, 0.25, 0.75]]
        }
      ],
      "items_truncated": false
    }
  ]
}
```

`coordinates` is what `mesh-render --highlight-from report.json` consumes; every
item that names a location MUST carry it. Item ordering is deterministic:
ascending `(code, first point id, first cell id)`.

---

## 5. Accuracy contract

Every relative metric names its denominator; the absolute value in model units is
always reported alongside. Enforced by [V5]/[V6].

| Metric | Definition | Default gate |
|---|---|---|
| Surface conformance | interface-node distance to its component surface / local interface edge length | ≤ 2% (max) |
| Surface fidelity | sampled two-sided Hausdorff, tagged faces ↔ input patch | ≤ `max(ε, 0.25·h(x))` |
| Chord error | tagged-face midpoint deviation / local `h` | ≤ `chord_error_frac` (0.2) |
| Feature-curve conformance | curve-node distance to the arranged polyline / local `h` | ≤ 10% |
| Sharp-corner error | corner-node displacement / local `h` | ≤ 1% |
| Solid volume error | per component `\|V_mesh − V_input\| / V_input` | ≤ 1% |
| Interface area error | per tagged patch set | ≤ 2% |
| Gap thickness error | band regions: reconstructed `t` vs field `t` | ≤ 20% of `t` |
| Mid-surface error | sheet nodes to mid-surface / local `h` | ≤ 2% |
| Topology counts | components, curves, partitions vs the arranged complex | exact |

---

## 6. Fixture suite

Committed at `data/fixtures/meshgen/`. The reference fixture is one unit cube
under the **Freudenthal 6-tet decomposition of G0-1 §2.2**, split into region
`{0}` (tets K0–K2) and region `{1}` (K3–K5), with the two shared faces emitted as
tagged interface cells and one 3-node polyline as a box-edge curve — so the
fixtures cross-validate the geometry freeze as well as this one.

Most corrupted fixtures are the reference mutated in exactly one way;
`bad_stacked_band` is purpose-built, because a stacked band only exists in a real
band and cannot be produced by perturbing the cube.

| File | Defect | Named check | Full fired set (the test manifest) |
|---|---|---|---|
| `good_cube.vtu` | none — the reference | — | *(empty)* |
| `bad_inverted_tet.vtu` | tet K0's nodes 1 and 2 swapped | `V1.negative_volume` | `V1.negative_volume` |
| `bad_duplicate_node.vtu` | v7 duplicated; K4 rewired to the copy | `V2.duplicate_node` | + `V3.boundary_leak`, `V3.hanging_node`, `V3.non_manifold_edge`, `V8.partition_mismatch` |
| `bad_hanging_node.vtu` | K0 split at the midpoint of edge v0–v7; neighbours keep the unsplit edge | `V3.hanging_node` | + `V3.boundary_leak`, `V3.non_manifold_edge`, `V8.partition_mismatch` |
| `bad_triple_face.vtu` | a third tet glued to interior face (v0,v1,v7) | `V3.multi_shared_face` | + `V3.boundary_leak`, `V3.non_manifold_edge` |
| `bad_boundary_leak.vtu` | tet K5 deleted; the exposed faces are interior to the box and untagged | `V3.boundary_leak` | `V3.boundary_leak` |
| `bad_unwelded_sheet.vtu` | one node of a sheet-tagged face duplicated, so the face is not welded to either side | `V7.unwelded_sheet_face` | + `V2.duplicate_node`, `V3.hanging_node`, `V8.pinhole_sheet` |
| `bad_stacked_band.vtu` | a band meshed with **two** element layers across one gap | `V7.band_layers` | `V4.min_dihedral`, `V4.low_dihedral_share` |
| `bad_partition_id.vtu` | a disconnected second component labelled `partition_id = 0` | `V8.partition_mismatch` | + `V3.boundary_leak` |
| `bad_region_key.vtu` | one tet's `region_key = 7` with a 2-entry table | `V6.illegal_region_key` | `V6.illegal_region_key` |
| `bad_pinhole_sheet.vtu` | one triangle removed from a sheet, leaving an interior hole | `V8.pinhole_sheet` | `V8.pinhole_sheet` |

> **Rule C2 (fixture manifest).** A fixture's contract is the **full set** of check
> codes it produces, asserted exactly — a new false positive fails the suite as
> loudly as a missed defect — **plus** its named check, asserted separately so a
> fixture can never silently stop testing the check it exists for.

The earlier formulation ("each must trigger exactly its check and no other") does
not survive contact with real meshes: one injected defect legitimately cascades.
A duplicated node also lands on its twin's faces (a hanging node) and disconnects
the flood fill; a T-junction necessarily leaves unmatched faces. Those are
consequences of the single defect, not additional defects, and suppressing them
would mean weakening real checks. `bad_stacked_band`'s [V4] warnings are likewise
honest: thin band elements *are* low-quality, which is precisely why the FEM-aware
ladder (§10.11 of the plan) exists.

All eleven are structurally valid VTU: they **must load** and pass
`VtuDoc::validate()`. They test the *verifier*, not the reader — which is what
lets the verifier be trusted before the mesher exists (plan §2, tooling-first).

Fixtures are **byte-canonical**: re-emitting a loaded fixture with
`save_vtu(.., Ascii)` reproduces the file byte-for-byte, so a diff against a
regenerated fixture is a meaningful review artefact.

---

## 7. Test obligations

| ID | Test | Asserts | Lands with |
|---|---|---|---|
| T-C1 | fixture load | all 11 fixtures load, `validate()` ok, re-emit byte-identical | GA-1 (done, §11) |
| T-C2 | corrupted-fixture suite | each `bad_*` fixture triggers exactly its check; `good_cube` triggers none | GA-2 |
| T-C3 | schema completeness | a producer that omits any **A** array fails a contract assertion | GA-2 |
| T-C4 | sentinel discipline | non-applicable entries carry `−1`/`255`/`0xFFFFFFFF`, never `0` | GA-2 |
| T-C5 | table encoding | set tables decode with end-offset semantics; a start-offset file is rejected | GA-2 |
| T-C6 | metadata | `StageIndex` matches the filename; `SchemaVersion` mismatch is a named error | GA-4 |
| T-C7 | geometry-only mode | an external tet-only VTU verifies with [V1]–[V5] and reports the rest SKIPPED with the missing array named | GA-2 |
| T-C8 | JSON schema | report validates against §4.1; item ordering deterministic across runs | GA-2 |
| T-C9 | renderer contract | every fixture renders; extraction counts match the analytic value | GA-3a |
| T-C10 | accuracy gates | each §5 row is enforced by a check and reports both relative and absolute values | G9-3 |

---

## 8. Verification record

Executed 2026-07-25 against the real GA-1 reader/writer
(`fixture_check` scratch project, release build, `rustmspt` by path):

- **11/11 fixtures**: load ok, `VtuDoc::validate()` ok, round-trip to an equal
  `VtuDoc`, and re-emitted **bytes identical**. 24 field arrays, 3 point arrays,
  6 cell arrays each.
- **Reference fixture through `mesh-render`** (the second implemented consumer):
  `9 cells -> 14 triangles, 2 segments, 0 markers` — analytically correct
  (6 cube faces × 2 triangles = 12 boundary + 2 tagged interface faces; the
  3-node polyline yields 2 segments). Rendered `color_by: region_key` shows the
  two regions in distinct categorical colours, split along the expected diagonal.
- Both schema/implementation mismatches found this way, before any mesher code:
  D-1 and D-3 below.

Generator: [`data/fixtures/meshgen/generate_fixtures.py`](data/fixtures/meshgen/generate_fixtures.py)
— committed alongside the fixtures per the plan's "generators committed"
convention (§17.3). It is a self-contained `uv` script with no dependencies:

```bash
uv run data/fixtures/meshgen/generate_fixtures.py data/fixtures/meshgen
```

Deterministic and re-runnable; because the fixtures are byte-canonical,
regeneration is a no-op diff, so a non-empty diff after regeneration means the
schema and the fixtures have drifted apart.

---

## 9. Deviations and open items

| # | Deviation | Reason |
|---|---|---|
| D-1 | Plan §7.2's `u16` arrays (`RegionSetPriority`, `ComponentY`) are frozen as **`UInt32`** | `ArrayData` has no `U16` variant, so every fixture failed to load with *"unsupported DataArray type UInt16"*. `UInt32` needs no implementation change and the value range is unaffected. Caught by the fixture round-trip |
| D-2 | Metadata is **numeric-only**; plan §7.2's string-valued `Stage`, `GeneratorVersion`, `ConfigHash`, `DeterminismMode` become `StageIndex` + a frozen enumeration, a 3-tuple semver, a `UInt64`, and a `UInt8` | No string array type exists, and adding one buys nothing: the human-readable stage name is already in the filename, which the verifier cross-checks ([V12]) |
| D-3 | `NumberOfTuples` is **required on FieldData arrays** and omitted elsewhere | VTK requires it there and `save_vtu` always emits it; fixtures without it round-tripped to a different byte stream. Making it part of the contract is what makes fixtures byte-canonical |
| D-4 | Set tables use **VTK end-offset** semantics, stated explicitly | The plan's §7.2 sketch never said which convention; the implemented `render_scene::set_members` uses end-offsets, matching cell `offsets`. Freezing the other convention would have silently shifted every set by one |
| D-5 | The background region key carries `RegionSetPriority = 0xFFFFFFFF` | `{0}` has no priority; a real value (e.g. `0`) would make it win every resolution comparison in a consumer that does not special-case it |
| D-6 | `verify_flags` is `UInt32` with bit *k* = check `[V(k+1)]` | The plan named the array but not the bit assignment; the renderer filters on it, so it needs a fixed mapping |
| D-7 | JSON report items carry an explicit `code` (`V3.hanging_node`) and a deterministic ordering | Tests assert on verifier JSON (plan §17.2); without a stable code and order those assertions would be brittle |

Open items:

- **[V6]–[V10] semantics land with their producing stages** (plan §19); this
  document freezes their contract, not their implementation.
- **Compressed VTU support** stays rejected. If ParaView-side file sizes become a
  problem, `vtkZLibDataCompressor` is the named future addition and a
  `SchemaVersion` bump is *not* required (it is an encoding, not a schema change).
- **`--split` companion verification** is out of scope for iteration 1: the
  primary file is the contract, companions are a convenience export.
