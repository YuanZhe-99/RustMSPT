# SPEC — Mesh generation: data contracts freeze (subtask G0-3)

**Status:** frozen (rev 1.3, schema v1, 2026-09-23 — audited against the code at `891badc`: §4's severities and gates are stated as normative with every as-built departure recorded; §4.2 (contract validation), §4.3 (the item cap and the exit status) and §4.4 (the domain at the final stage) are new; §5's P3 row is re-stated as containment with the corner test as its necessary half; §2.5 lists the non-contract diagnostic arrays; §6 brings the fixture table to the fourteen committed files; §8 gains the audit record; §9 gains D-8..D-17. `SchemaVersion` stays 1 — no array name, type, sentinel or encoding changed. rev 1.2, schema v1; additive G2 diagnostic orientation field recorded 2026-07-28; **rev 1.2 2026-08-15: §4 gains `[V13]` and §5 the P3 rows (plan P-1.1); §1 changes which file is the deliverable (plan P-2.1 / R4)** — the contract document's schema is untouched by both, so `SchemaVersion` stays 1). Normative for `src/io/vtu.rs`,
`src/meshgen/verify.rs`, `src/meshgen/render_scene.rs`, and every producer of a
contract VTU.
**Date:** 2026-07-28 (rev 1.3: 2026-09-23)
**Subtask:** G0-3 (Phase G0, tier T2) of [`PLAN_mesh_generation.md`](PLAN_mesh_generation.md); the rev 1.3 audit is that plan's M-0.1.
**Scope (from the plan's G0-3 acceptance):** §7 VTU schema (array names/types/
sentinels, tables, metadata), snapshot naming, §8 check catalog + severities +
JSON schema, §15 accuracy table. *Acceptance: schema doc + fixture VTUs
hand-written to it.*
**Companion freezes:** [`SPEC_meshgen_geometry.md`](SPEC_meshgen_geometry.md)
(G0-1), [`SPEC_meshgen_numerics.md`](SPEC_meshgen_numerics.md) (G0-2).

> **Plan cross-references (rev 1.3, 2026-09-23).** This document was frozen against the rev-2
> plan, whose section numbers it cites as "plan §N" / "§N of the plan". That document was deleted
> on 2026-09-01 (plan D-2) and replaced by `PLAN_mesh_generation.md` v3 (rev 3.1). Every such
> citation resolves through the plan's **Appendix B.0** (a map from the old section numbers to
> where the content lives now) and **Appendix A** (the record's findings, keyed by the old section
> numbers). The plan's own ids are `M-n.m` (subtasks), `D-n` (owner decisions), `MG-nn` (the
> 2026-09-11 review's findings) and `X-n` (measured-and-closed routes).

The fixture suite required by the acceptance criterion is committed at
[`data/fixtures/meshgen/`](data/fixtures/meshgen) — 11 files at rev 1.0, **14** at rev 1.3
(§6), hand-written to this schema, all verified to load through the real GA-1 reader, pass
structural validation, and re-emit byte-identically (§8). Writing them found two genuine
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
  silently — and a consumer verifying a document that *declares itself* a schema-v1 final
  contract MUST treat a missing **A** array as a failure of the producer, not as an optional
  absence (§4.2, rev 1.3).
- **Frozen is a statement about change control, not about correctness** (plan R8): a clause an
  audit shows to be insufficient is amended with a §9 row and a §8 record; "the implementation
  matches the frozen text" closes no finding that says the text is wrong. Rev 1.3 is the first
  such audit (§8).

---

## 1. File organization

**Amended 2026-08-15 (rev 1.2, P-2.1 / requirement R4): which file is the deliverable
changed; the contract document itself did not.** Every write now produces a **pair**:

| file | contents | role |
|---|---|---|
| `<name>.vtu` | `VTK_TETRA` only, region identity on the `region_key` cell array | **the delivered mesh.** Opened directly, its only feature edges are the domain box |
| `<name>_contract.vtu` | the mixed-cell contract document below | the auxiliary the face-tag contract, S9–S11 and the INP export read |

The volume is *derived* (`volume_only`), never authored, so the two cannot drift; they
share one point array, `GlobalPointId` maps back, and `Counts` is restated on the derived
file so it does not misdescribe itself. A document with **no tets** — the surface stages
`s00`–`s03` and the `s04` voxel sizing preview — is written whole under the plain name and no
auxiliary appears; the rule keys off the document (`volume_only` finds no tets), not the stage
label. There is no setting for any of this — which file is the mesh is not
a matter of taste (R3).

Everything the verifier's full catalog needs lives in the auxiliary: `[V5]`–`[V9]` read
the face tags, so `mesh-verify` is pointed at `_contract.vtu` when the whole catalog is
wanted. `[V1]`–`[V4]` and `[V12]` give the same result on either file. *Measured at rev 1.3 on A-3's
`s08` (D-18):* `[V6]`'s key checks run on the delivered file but its adjacency and
undeclared-boundary counts need the tags and differ there (undeclared 24,873 against 73,
adjacency 75 against 73); `[V7]` **false-FAILs** on the delivered file (`band_stacked_elements`
514 against 0, the gap lids' tags being absent); `[V9]` reads no curve cells there (0 edges
against 136). `[V7]` and `[V9]` are therefore run on `_contract.vtu` only, and `[V13]` reads
`region_key` and is unaffected. The delivered file also carries every field table verbatim: the
`FaceTag*`/`Curve*`/`ThinRegion*` tables describe cells that exist only in the contract file
(A-6a: 50,970 face-tag rows against 0 face cells), and a consumer of the delivered file MUST NOT
read them.

**The contract document** (VTK XML `UnstructuredGrid`, single `Piece`) is unchanged:
`VTK_TETRA` (10) volume cells + `VTK_TRIANGLE` (5) tagged-face cells +
`VTK_POLY_LINE` (4) feature-curve cells + `VTK_VOXEL` (11) lattice-preview cells,
**all indexing one shared point array**. Shared topology is then verifiable by
point-index identity, which is the whole reason the contract is one file.

| Property | Frozen value |
|---|---|
| Root attributes | `type="UnstructuredGrid" version="1.0" byte_order="LittleEndian" header_type="UInt64"` |
| Default encoding | appended raw (`format="appended"`, `encoding="raw"`). *As built (D-19):* the `mesh` pipeline passes `VtuEncoding::Ascii` to every snapshot, so its whole output is ascii (A-6a's `s08_cut_contract.vtu`: 30 MB); appended raw is produced only by `save_vtu` callers that ask for it (`mesh-verify --annotate`). Ascii is selected by an output path ending in `.ascii.vtu` |
| Debug/fixture encoding | ascii (`format="ascii"`) |
| Rejected on read | compressed (`compressor` attribute present), base64 (`format="binary"`) — with a named error each |
| Accepted on read | `header_type` `UInt64` (written) or `UInt32` (tolerated) |
| Appended-array offsets | collected in **emission order**: FieldData, Points, Cells, PointData, CellData |
| Cell `offsets` array | VTK **end**-offsets: cell *i* spans `connectivity[offsets[i-1] .. offsets[i]]`, `offsets[-1] ≡ 0` |
| Float formatting (ascii) | shortest round-trip (`{}`), so write→read→write is byte-stable |
| `NumberOfTuples` | **required** on every `FieldData` array; omitted elsewhere |
| `NumberOfComponents` | emitted only when `≠ 1` |

`--split` companions (`_faces`/`_curves.vtu`) remain an optional export for tools that
reject mixed cells; each carries `GlobalPointId` mapping into the shared numbering. *Rev 1.3:*
no producer emits them — there is no flag, config field or writer — so they are an open item,
not a contract obligation. The
`_volume` companion is no longer one of them — it *is* the deliverable, under the plain
name, and it **is** verified independently (that is P-2.1's acceptance).

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
| `partition_id` | `Int32` | A | tets: partition index from the flood fill of plan B.1; `−1` otherwise. *As built (D-14):* `cut_to_doc` writes `0` on every cell, non-tets included — no flood fill has ever been produced; plan M-6.1 builds it |
| `regime` | `UInt8` | A | tets: `0` normal · `1` band · `2` band-Steiner; `255` n/a |
| `face_tag_key` | `Int32` | A | face cells: index into the face-tag table; `−1` otherwise |
| `curve_id` | `Int32` | A | curve cells: index into the curve table; `−1` otherwise |
| `provenance` | `UInt8` | Dbg | dominant record provenance: `0` lattice · `1` cut · `2` arbitrated · `3` junction · `4` band. *As built:* only `0`, `1` and `3` are produced — band pieces are seeded by geometry §7.5 and carry `3`; `2` is reserved for ARB-23, which is not built (geometry D-33); non-tet cells are padded with `0` where §2's u8 sentinel is `255` (D-20) |
| `arbitrated` | `UInt8` | Dbg | `1` if any record entry was arbitrated (geometry §12). *As built:* emitted on `s06` only, as `is_ambiguous()` — "still unresolved", a different predicate from "was arbitrated"; `s08` emits no such array |
| `band_region` | `Int32` | Dbg | thin-region id; `−1` n/a. On s03 this is the G3-2 region id per wall face |
| `thin_role` | `UInt8` | Dbg | **additive G3-2 diagnostic** (recorded 2026-07-29): `0` ordinary wall · `1` wall inside a converted region · `2` mid-surface face; `255` n/a. G7 must promote it or replace it with the final sheet encoding before the first final VTU |
| `aspect_ratio` | `Float32` | Ann | `R/(3·r_in)` |
| `radius_ratio` | `Float32` | Ann | — |
| `min_dihedral_deg` | `Float32` | Ann | reported in degrees; **gates compare `cos²` algebraically** (G0-2 §8.1) |
| `scaled_jacobian` | `Float32` | Ann | — |
| `verify_flags` | `UInt32` | Ann | bitmask of checks that produced a WARN or FAIL item naming this cell (*as built*: WARN items set the bit as well as FAIL — D-20); bit *k* = check `[V(k+1)]` |

### 2.2 Point data

| Array | Type | Presence | Domain / meaning |
|---|---|---|---|
| `n_id_key` | `Int32` | A | index into the node-ID-set table. *As built:* realised by S8's `cut_to_doc` (2026-08-13; a stub of zeros before); `s04`–`s07` carry a placeholder — every node indexes one set `{0}`; on `s00`–`s03` it is the set of components incident to the surface node, never `0` |
| `constraint_kind` | `UInt8` | A | `0` free · `1` surface · `2` polyline · `3` corner · `4` box face. *As built:* S7's own record, carried through S8 since 2026-08-13 (a stub of zeros before); a **cut node** S8 interns is `0` (free) although it was constructed on a surface — plan M-5.4 must make it truthful before any smoothing reads it |
| `constraint_ref` | `Int32` | A | component X or curve id the constraint binds to; `−1` when free. *As built:* X for kind `1`; for kind `2` the S2 arranged-curve index, which is curve-table row `rims + ref` on `s08` (rim rows come first; `rims = 0` on the acceptance cases); `−1` for kinds `3` (corner) and `4` (box face) and for a polyline capture from the alternating pass (D-20) |
| `separation_t` | `Float32` | Dbg | S3 field snapshot; per vertex, the smallest finite separation measured at that vertex or on an incident face, in output coordinates. `-1` = no pairing here (the not-applicable convention for an otherwise non-negative quantity, recorded additively 2026-07-29 with G3-1) |
| `sizing_h` | `Float32` | Dbg | S4 field snapshot |
| `snap_motion` | `Float32` | Dbg | `s07` only: the distance S7 moved the node, in output units (rescaled by the domain diagonal); `0` for an unmoved node. Additive, recorded at rev 1.3 |
| `GlobalPointId` | `Int64` | delivered file only | index of the point in `_contract.vtu` — the identity, since `volume_only` does not compact points |

### 2.3 Field data — tables

All set tables use the **same end-offset encoding as cell `offsets`**: set *k*
occupies `Components[Offsets[k-1] .. Offsets[k]]`, with `Offsets[-1] ≡ 0`. This
matches `VtuDoc::cell()` and `render_scene::set_members` as implemented.

| Table | Arrays | Types |
|---|---|---|
| Region sets | `RegionSetOffsets`, `RegionSetComponents` (X values), `RegionSetPriority` (one Y per key) | `Int64`, `Int32`, `UInt32` |
| Node-ID sets | `NIdSetOffsets`, `NIdSetComponents` | `Int64`, `Int32` |
| Face tags | `FaceTagOffsets`, `FaceTagComponents`, `FaceTagKind`, `FaceTagSideElems` | `Int64`, `Int32`, `UInt8`, `Int32` (2 components) |
| Face-tag orientation (G2 diagnostic) | `FaceTagOrientation` | `Int32`, one `+1`/`-1` per flattened `FaceTagComponents` member. *As built on `s08` (D-13, plan MG-07):* `cut_to_doc` pushes one `+1` per face cell, so the array is shorter than `FaceTagComponents` wherever a face carries several tags (two cubes in exact contact: 280 members, 270 entries) and carries no orientation information; `s02`'s producer (`arranged_surface_to_doc`) is per member as specified |
| Components | `ComponentX`, `ComponentY`, `ComponentKind`, `ComponentClosed` | `Int32`, `UInt32`, `UInt8`, `UInt8` |
| Curves | `CurveKind`, `CurveCompOffsets`, `CurveCompComponents`, `CurveRadialPatches` | `UInt8`, `Int64`, `Int32`, `Int32` |
| Thin regions (G3-2 diagnostic) | `ThinRegionRegime` (`0` normal · `1` band · `2` sheet), `ThinRegionPairClass`, `ThinRegionConfidence`, `ThinRegionSeparation`, `ThinRegionSkip` (`0` none · `1` low confidence · `2` speck · `3` mid-surface invalid · `4` mid-surface unbuildable · `5` intersection wedge · `6` undersampled — the emitter's sixth arm, added at rev 1.3, D-20) | `UInt8`, `UInt8`, `Float32`, `Float64`, `UInt8` |

Enumerations: `FaceTagKind` `0` interface · `1` sheet · `2` box cap.
`ComponentKind` `0` solid · `1` sheet. `CurveKind` `0` sharp · `1` rim ·
`2` intersection · `3` box. `CurveRadialPatches` is an **additive** column
(recorded 2026-08-13 with `[V9]`): one non-negative count per curve, the number of
arranged patches S2's `radial_patch_order` put around it. `[V9]`'s second clause is
specified against "the curve table" and the frozen table carried no column that could
answer it, so the check could not be written at all without this. *The curve table itself was
empty until the same day* — `CurveCompOffsets`/`CurveCompComponents` were emitted as zero-length
arrays against a non-empty `CurveKind` — which is the second of three contract arrays found
emitted with no content (§8). `0` means the
producer did not order patches around that curve and the clause is not asked of it. *As built (D-21):* on pipeline output **every** value is `0` —
`radial_patch_order` fills the count in S2, `clip_arranged_to_box` rebuilds every curve with an
empty `radial_patches`, and the column is emitted on `s08` only — so `[V9]`'s radial-patch clause
has never been asked of a mesh the pipeline produced (A-3: 0 of 114 curves, 102 of them
intersection curves; A-6a: 0 of 34). The `bad_radial_patches` fixture exercises the clause; no
pipeline output does.
A curve cell's `curve_id` names **one** table entry; where a mesh edge lies on more
than one curve - a contact rim is both bodies' own sharp edge - the producer emits the
edge once under the lowest curve index, because a duplicate cell is a `[V1]` failure. `s08`'s curve
table lists first one row per collapsed-sheet rim edge (`CurveKind` 1, an **empty** component
set, radial `0` — so `[V9]`'s `N_ID` clause has nothing to ask of a rim node), then one row per
locked S2 curve, including curves the mesh carries no edge for. `ThinRegionPairClass` `0` unpaired · `1` intra
(two faces of one component) · `2` inter (two components) · `3` solid–sheet ·
`4` sheet–sheet · `5` surface–box, mirroring S3's `PairClass` (added with G7-2;
`ThinRegionRegime` and `ThinRegionPairClass` are also emitted on `s08_cut`, where
they are indexed by the `band_region` of a band element rather than by a wall
face, so a consumer of the mesh can say what kind of gap an element spans without
holding on to `s03_gapfield`).

`FaceTagSideElems` stores `(elem⁺, elem⁻)` per **face cell**, in face-cell order —
the reserved cohesive/split-node hook (plan Appendix B.6). `−1` marks a side with no
adjacent tet (a face on the domain boundary). *As built (D-13):* on a multi-tag face the pair
is the **first** tag's `(inside, outside)`; which body is `+` on a contact face is therefore the
lower component's convention, not a geometric side — plan M-4.8 defines one geometric plus/minus
side per face with each component's inside/outside mapped onto it, which M-6.6's split export and
M-4.7's contact fixture need. A tagged face both of whose sides seed to one region is **dropped**
by the producer rather than recorded one-sided (`the_interface_index_is_derivable_and_two_sided`).

The `ThinRegion*` tables are additive schema-v1 diagnostics emitted by
`gapfield_to_doc`, one row per segmented region, indexed by `band_region`. `ThinRegionSeparation` is the region's frozen `t_r` in the
**normalized** frame (domain diagonal = 1) — unlike `separation_t`, which the pipeline rescales
to output units — and `−1` when non-finite (D-20). A
mid-surface triangle is emitted as an ordinary tagged face cell carrying its
region's component with `FaceTagKind = sheet`, and is told apart from a wall face
by `thin_role = 2`. Like `FaceTagOrientation` they are not retroactively required
from the pre-G3 fixtures, and G7 must promote or replace them before the first
final VTU.

`FaceTagOrientation` is an additive schema-v1 diagnostic field emitted by
`arranged_surface_to_doc`; its offsets are exactly `FaceTagOffsets`. It records
orientation relative to the canonical emitted face and discharges coincidence
case C2 internally. It is not retroactively required from the pre-G2 fixtures.
*Rev 1.3:* `s02_arranged` is emitted live. The field is per member on `s02`/`s03`, an empty
table on `s04`–`s07`, absent on `s00`/`s01`, and a per-face placeholder on `s08` (D-13). It was
not promoted: on `s08` side orientation is carried by `FaceTagSideElems` instead, whose own
limits D-13 records. For a mixed sheet/solid C9 tag,
`FaceTagKind=sheet` and `FaceTagComponents` retains both identities. G2-5 encodes
C10 as `FaceTagKind=box` with the sheet X retained in the same component set.

The background region key `{0}` carries `RegionSetPriority = 0xFFFFFFFF`
("not applicable"); every other key's priority is the single Y shared by all its
X values, which verifier check [V6] enforces.

### 2.5 Non-contract arrays (rev 1.3, plan S-9)

Four arrays the S8 producer can write are **diagnostics, not contract**: no check reads them,
no fixture carries them, and their presence or absence changes no verdict.

| Array | Kind | Type | When emitted | Meaning |
|---|---|---|---|---|
| `node_origin` | point | `UInt8` | always, on `s08` | `0` lattice node · `1` node the cut interned (trace point, face Steiner node, fan apex, arena point). The plan's P-4.1 census reads it |
| `parent_cell` | cell | `Int32` | `RUSTMSPT_CUT_DIAG` | the S5 cell index a tet came from; `−1` on non-tets |
| `plc_path` | cell | `Int32` | `RUSTMSPT_CUT_DIAG` | the arm that emitted the tet: `1` §7.4-meshed · `2` whole-cell fan · `3` escalated (§7.6) · `4` facet-split fan · `−1` §6 table (geometry §7.7 F) |
| `escalation_reason` | cell | `Int32` | `RUSTMSPT_CUT_DIAG` | the `Escalation` ordinal of the parent cell; `−1` when it did not escalate |

A future revision that wants one of these gated on MUST give it a presence class in §2.1/§2.2
and a fixture; until then a reader treats them as absent-by-default. `node_origin` is listed
here rather than in §2.2 because it is always written but nothing in the catalog depends on it;
`snap_motion` (§2.2) is the same kind of array on `s07`. All four are inherited by the delivered
volume, and `[V6]`/`[V13]` read them only for optional breakdown metrics.


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

*Rev 1.3 (D-15):* the enumeration is what `snapshot.rs` implements, index for index. There is
no index for an S10 "regions" stage: the plan's M-6.1 originally named an `s10_regions`
snapshot, which would have given index 10 two meanings; S10's tables are part of `s11_final`.

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
face and curve cells only. `s04` previews write `cell_kind = 3` voxel cells
carrying `sizing_h`; *as built* `s05` writes the lattice as `VTK_TETRA` cells (region key `0`,
`sizing_h` per node) rather than voxels — an amendment `lattice_to_doc` records and this
revision adopts. `s10_quality_r<N>` is emitted once per IQD round `N` (S9 is not built; the
pipeline returns `NotAvailable` after `s08`).

Every snapshot carries the **full** §2.4 metadata block, so the verifier and the
renderer accept any snapshot interchangeably — one contract end to end. A
snapshot whose `StageIndex` disagrees with its filename is a [V12] FAIL.
Because s00-s03 intentionally contain no volume cells, verifier checks [V7]
(sheet-to-volume welding) and [V8] (volume partitions) report **SKIPPED** with a
stage-specific reason for those snapshots; tagged surface faces are not treated
as failed final sheets.

Size control: `snapshots: all` combined with an estimate above 5 M tets emits a
WARN before the run (≈60 B/tet per volume snapshot, plan Appendix B.4).

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
| **[V6]** ID semantics *(contract arrays required)* | per-component volume error vs input solid; region labels ∈ legal key table; same-priority-overlap keys share one Y; priority-resolution audit (sampled `robust_inside`); ownership completeness; **region adjacency** (rev 1.3): a face whose two tets' region sets differ by two or more components is a step no single interface can produce and is a FAIL unless the face is tagged for the components in the difference (an exact contact both bodies declared) or the keys differ in priority (a legitimate two-member change under R-A4, reported not failed); **undeclared material boundary** (rev 1.3): every face whose two owners resolve to different keys MUST carry a tag for each component in the difference — plan §9's invariant — and `undeclared_boundary_faces > 0` is a FAIL | FAIL (semantic) / WARN (sampled) | volume ≤ 1% rel; arbitration rate < 0.5% |
| | *`[V6]` as built (rev 1.3, `check_v6`):* `V6.illegal_region_key` and `V6.priority_mismatch` are FAIL as specified. **Region adjacency** is FAIL (`V6.region_adjacency`) when two tets sharing a face carry inside-sets differing by more components than the face is tagged with (background `{0}` read as the empty set; a face with no tag allows one); pairs whose keys carry different priorities are counted (`region_adjacency_mixed_priority`) and reported INFO (`V6.adjacency_mixed_priority`), not failed. **Undeclared boundary** is a *metric only* (`undeclared_boundary_faces`, `undeclared_boundary_area`, `undeclared_boundary_mixed_priority`, and `undeclared_boundary_same_cell` under `RUSTMSPT_CUT_DIAG`) — the FAIL the rev 1.3 clause states is not implemented (plan M-1.0), and the plan's own record warns the metric measures *declaration*, not *placement*, and must not steer a change on its own. The per-component volume error and the sampled priority audit are deferred (`V6.deferred`, INFO) because they need the input surfaces; `[V5]` carries the volume error where surfaces are given. | | |
| **[V7]** Sheets & thin | sheet faces with exactly 2 adjacent tets sharing all 3 nodes (**welded**); sheet-node mid-surface distance; rim conformance; **one-layer band check**; band AR/dihedral ranges; Steiner count; `[THIN-SKIP]` inventory | FAIL (topology) / INFO (counts) | mid-surface ≤ 2% h; rim ≤ 10% h |
| **[V8]** Partitions | recomputed flood fill ≡ stored `partition_id`; pinhole-leak heuristic; per-partition volume + label composition; `expected_partitions` | FAIL / WARN / INFO | `expected_partitions: null` |
| **[V9]** Junctions | curve-node `n_id_key` set ⊇ the curve's component set; radial patch count matches the curve table; multi-surface cell conformity across the shared-face cache (as built: the tets around a junction edge form a closed fan) | FAIL | — |

`[V9]` **implemented 2026-08-13**, with two clarifications the first run forced. The
radial-patch clause is asked only where **two or more** components meet along the
curve: around a sharp edge of a single solid the material sectors are inside and
outside whatever the patch count is, so the identity does not hold there. The
conformity clause is read on the written mesh as "the tets around a junction edge form
a closed fan", and is not asked of an edge incident to a face the mesh owns only once -
an edge on the mesh's own boundary cannot close, and a locked curve sitting there is
ordinary. A curve the mesh carries no edge for is reported `INFO`, not failed: gate
G6-0 adopted the conforming fan over constrained edge recovery, so the mesh is not
required to reproduce every curve as a chain of edges. *Rev 1.3:* plan R1 voids that
justification — the fan is a deviation slated for removal (geometry D-17) — and this clause
becomes FAIL at rev 1.4 once plan M-2 makes the kernel carry every curve (plan S-8); left INFO
here because R6 forbids gating on an unmeasured state.
| **[V10]** Export completeness | INP↔VTU cross-check: element/node counts, per-set sums, unmapped-region audit. *As built:* emitted `SKIPPED` unconditionally — no INP writer exists (plan M-6.2); once the split export lands the cross-check reads the node map of §2.3 (plan M-6.6), never count equality alone | FAIL | — |
| **[V11]** Compare mode | strict: canonical-order arrays byte-equal, coordinates bit-identical. topology: identical connectivity/labels/tables, coordinates within `1e-6·diag`, quality within 1%. *As built:* emitted `SKIPPED` unconditionally — `--compare` does not exist (plan M-6.3); R-P2 is measured by `sha256sum` of the two files | FAIL | selected by `DeterminismMode` |
| **[V12]** Provenance & stats | counts, `[OWN-STATS]`, repair-log echo, config-hash match, stage/filename agreement, memory summary; **per-`provenance` element counts and per-S5-cell emission rates** (`tets_provenance_*`, `cells_provenance_*`, `tets_per_cell_*`, `lattice_cells`) | INFO (mismatch: WARN) | — |
| **[V13]** Interface fidelity *(added 2026-08-15; needs the input surfaces)* | the **material boundary** — a face whose two tets carry different region sets, or a single-owner face inside a body off the domain box — measured against the input surface at its **corners**: on-surface area share, area-weighted mean and max \|distance\|, area-weighted **signed** offset, `displacement_share`; the interior sag is reported apart as `chord_*`. Rev 1.3: the corner test is P3's *necessary* half (§5); a boundary missing entirely is a FAIL, never SKIPPED | **FAIL** at `on_surface_area_frac < 1.0` (normative, rev 1.3; *as built* WARN — D-8) | §5's P3 rows |

`[V13]` **added 2026-08-15** under the record's P-1.1 (plan Appendix A), and it is not a
variant of `[V5]`. `[V5]` measures the *declared* interface — the tagged `VTK_TRIANGLE`
cells — whose nodes S7 snaps onto the surface, so it reads essentially exact on a mesh
whose real material boundary is a staircase of **undeclared** faces half a cell away.
`[V13]` derives the boundary from the volume, region set against region set, and never
consults a tag.

Two rules make its numbers mean what they say:

1. **P3 is read at the face corners.** A flat facet whose three vertices are cut nodes on
   the surface is a *chord* of it — the best a mesh of flat facets can do, with a sag that
   falls as `h²` and is traded against P2 by refinement. A facet whose vertices are lattice
   nodes or a cell centroid is somewhere else entirely, and that is the staircase P3
   forbids. Measuring the whole facet at once conflates them: on the sphere fixture it
   charged the mesher for 87 % of its boundary area when most of that was irreducible
   faceting. The sag is still measured, as `chord_mean`/`chord_max`, as its own number.
2. **Two numbers, because a boundary fails in two ways one distance cannot separate** —
   *rough but centred* and *smooth but displaced*. Only the **signed** mean tells them
   apart: roughness cancels in it, displacement does not. Both are violations; the pair is
   for diagnosis, not for grading one as acceptable.


**Gate values as built (rev 1.3, `VerifyGates::default`, every length as a fraction of the
bounding-box diagonal unless stated):** `max_ar_warn 20`, `min_dihedral_deg 5`,
`low_dihedral_deg 10`, `low_dihedral_share 1e-4`, `duplicate_node_tol_frac 1e-6`,
`plane_tol_frac 1e-9`, `hanging_tol_frac 1e-9`, `surface_distance_frac 0.02` (of the local
interface edge), `interface_on_surface_frac 0.02` (of the face's own edge),
`interface_offset_frac 0.005`, `max_items_per_section 50`, `expected_partitions null`,
`warn_is_fatal false`. These are frozen by this revision (plan S-7): the three tolerances
`duplicate_node_tol_frac`, `hanging_tol_frac` and `plane_tol_frac` were code-only until now, and
`[V3]`'s meaning under R7 depends on the last two. `[V2]`'s `1e-6·diag` is finer than the weld
grid `q` (geometry §1.2 as-built note): it is the contract's duplicate-node gate, not a check of
the weld invariant.

Invocation:

```
rustmspt mesh-verify --input mesh.vtu [--config cfg.yaml] [--report out.log]
                     [--json out.json] [--annotate annotated.vtu]
rustmspt mesh-verify --compare a.vtu b.vtu --mode strict|topology
```

Exit code is nonzero on any FAIL, or on any WARN configured as fatal.

### 4.2 Contract validation — normative (rev 1.3, plan MG-08)

A document's **validation strength is chosen from its metadata, never from which arrays happen
to be present.** Three strengths:

| strength | selected when | what MUST be validated before any check runs |
|---|---|---|
| **primary volume** | `SchemaVersion = 1`, tets only (`Counts[2] = Counts[3] = 0`, or `cell_kind` all `0`), `region_key` present | structural validity (`VtuDoc::validate`), `Counts` restated for this file, `region_key` within the region-set table |
| **mixed contract** | `SchemaVersion = 1` and any face or curve cell, or `StageIndex ≥ 8` | every **A**-presence array of §2.1–§2.3 present with its frozen type and component count; sentinels where §2 requires them (`−1`/`255`/`0xFFFFFFFF`, never `0`); every set-table offset monotone and in range; every `region_key`, `face_tag_key`, `curve_id`, `n_id_key` inside its table; every component X in a region set or tag present in the component table; **no `Sheet` component in any region set** (G0-1 §9.1 row 10); `FaceTagOrientation` length equal to the flattened `FaceTagComponents` length (§2.3); `FaceTagSideElems` entries either `−1` or a tet index whose tet shares all three nodes of the face, with `elem⁺` and `elem⁻` on opposite sides of it; `StageIndex` consistent with the filename (§3) |
| **external geometry-only** | requested explicitly (`surfaces`-less external VTU with no `SchemaVersion`) | points and tet connectivity only; `[V1]`–`[V5]` run, the rest report SKIPPED naming the missing array |

A failure of the mixed-contract validation is a **FAIL** finding of its own (`V12.contract`, one
item per violated rule) and the report says which strength was applied. A self-declared final
document that omits an **A** array is a broken producer, not a geometry-only file, and MUST NOT be
verified as one.

> **As built (D-11).** None of the mixed-contract rules is checked at `891badc`: `VtuDoc::validate`
> checks structure and array lengths only, and every semantic check degrades to SKIPPED when its
> array is absent. Three documents derived from `good_cube.vtu` — `constraint_kind`/`constraint_ref`
> removed; `FaceTagSideElems = [999999, 999998, 999999, 999998]`; `ComponentKind[0] = 1` while the
> component owns three tets — each verify with `fail = warn = 0` and exit 0 (§8, 2026-09-23). Plan
> M-1.0 builds the validator and commits the three as fixtures.

### 4.3 The item cap and the exit status — normative (rev 1.3, plan MG-01)

`max_items_per_section` bounds what a section **stores and prints**, and nothing else. The
severity counts in `summary` (`fail`, `warn`, `info`), every section's `status`, and the process
exit status are computed over **every finding produced**, whether or not it was stored; a section
whose stored items are capped says so with `items_truncated = true` and still carries the status
and counts of all its findings. `passed()` is a function of the section statuses, not of the
stored items. Every producer that samples or truncates its findings before reporting them MUST
count before truncating.

> **As built (D-9).** `VerifySection::push` sets the section status before applying the cap, but
> `verify_with_options` derives `summary.fail`/`summary.warn` from the stored items and `passed()`
> reads those counts. `max_items_per_section: 0` on `bad_inverted_tet.vtu` reports `V1 = FAIL`,
> `summary.fail = 0`, exit **0**; a section whose INFO items fill the cap hides a later FAIL the
> same way (§8, 2026-09-23). Plan M-1.0 fixes it and commits the fixtures (cap 0/1/50 give one
> answer; an INFO-then-FAIL section).

### 4.4 The domain at the final stage — normative (rev 1.3, plan MG-03)

`[V3]`'s boundary-leak rule measures against the octree hull rather than the domain box **only
while the lattice legitimately overhangs the box**, which is `StageIndex` 5–8 (G0-1 §3.1's subtree
cut; plan B.7). At `StageIndex` 9–11, and on any document verified as delivered: every node some
cell uses lies inside `[DomainMin, DomainMax]` (within `plane_tol_frac·diag`), every free face lies
on one of the six box planes, and the total tet volume equals the box volume within `[V6]`'s
volume tolerance. A hull built from the mesh's own points is never the reference at those stages:
a mesh that extends past the box is a leak, not a bigger box. When the relaxation is applied at
5–8 the report names the stage that justified it.

> **As built (D-10).** `check_v3` builds the hull whenever any point exceeds the declared domain
> and accepts single-sided faces on it at **every** stage, 11 included, so the mesh's own extent
> relaxes the box requirement: `good_cube.vtu` with every `x` doubled, `DomainMax = [1, 1, 1]` and
> `StageIndex = 11` reports `[V3]` PASS with `boundary_leaks = 0`, `fail = warn = 0`, exit 0.
> `run_acceptance.py`'s `check_delivered` reads the same `[V3]` codes and is therefore not an
> independent box test (§8, 2026-09-23). Plan M-1.0 fixes it and commits the fixtures (the doubled
> cube, a translated mesh, an extra block outside, an interior cavity, an allowed pre-trim overhang);
> plan M-6.2 makes the box an S8 constraint so the relaxation ends at S8.

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
ascending `(code, first point id, first cell id)`. `items_truncated` says the section's stored
items were capped; under §4.3 its counts and status are still those of every finding.

> **As built (rev 1.3, D-17; numerics D-17).** Item ordering is deterministic on every fixture
> (`report_is_deterministic_across_runs`) and **not** on a large mesh: `[V6]`'s region-adjacency
> items are pushed in `HashMap` iteration order and truncated at `max_items_per_section`, so two
> runs on A-3's `s08_cut_contract.vtu` (218 violations, cap 50) list different items, and its
> `undeclared_boundary_area` metric is a float sum in that order (`0.1971761865274474` against
> `0.19717618652744737`, measured 2026-09-23). The clause above is the contract; plan M-1.0 sorts
> the keys and sums in index order, and the fixture that pins it must exceed the cap.


---

## 5. Accuracy contract

Every relative metric names its denominator; the absolute value in model units is
always reported alongside. Enforced by [V5]/[V6]/[V13]. Which surface the rows are measured
against — the original input, the effective (repaired, decimated, `ε`-merged) surface, or the
collapsed model after S8b — is plan D-6, open at rev 1.3; the rows below read "the input
surface" as the effective surface once D-6 is decided.

| Metric | Definition | Default gate |
|---|---|---|
| **P3 surface exactness** (definition, rev 1.3) | share of **material-boundary** area **contained** in the effective input surface (plan D-6): a boundary triangle is on the surface iff it lies in the plane of an arranged patch it names and is covered by that patch's clipped fragments — coplanarity by the exact predicate at the anchoring tolerance, coverage by an exact 2-D overlay in the patch plane with clipped area equal to the triangle's; a triangle crossing an input crease is split at it and each part tested; a triangle no patch covers, and a missing boundary, are failures, never SKIPPED. Hidden inactive patches (R-A4) are legitimate absences the criterion knows | **1.0** — P3 admits no displacement |
| **P3 anchoring** (the necessary half, as built) | share of material-boundary area whose every *corner* is within `interface_on_surface_frac` of the input surface, measured against the face's own edge length. **Necessary, not sufficient**: three corners on a piecewise-planar surface do not put the triangle they span on it (plan MG-02, D-12) | **1.0**; a PASS here is not a P3 PASS until the containment row is implemented (plan M-1.5) |
| **P3 displacement** | area-weighted **signed** corner distance from a component's material boundary to its surface / local edge length; + outside the body, − inside | ≤ `interface_offset_frac` (0.5%) |
| **Material-boundary chord sag** | area-weighted distance from a boundary face's edge midpoints and centroid to the surface / local edge length; reported, not gated, and **not** a P3 violation | — (falls as `h²`; steered by `chord_error_frac`) |
| Surface conformance | interface-node distance to its component surface / local interface edge length | ≤ 2% (max) |
| Surface fidelity | sampled two-sided Hausdorff, tagged faces ↔ input patch | ≤ `max(ε, 0.25·h(x))` |
| Chord error | tagged-face midpoint deviation / local `h` | ≤ `chord_error_frac` (0.2) |
| Feature-curve conformance | curve-node distance to the arranged polyline / local `h`. *Not implemented* (plan S-12): `verify.rs` has no such metric; `[V9]`'s curve-carriage count is the only curve measure and is INFO | ≤ 10% |
| Sharp-corner error | corner-node displacement / local `h`. *Not implemented* (plan S-12) | ≤ 1% |
| Solid volume error | per component `\|V_mesh − V_input\| / V_input` | ≤ 1% |
| Interface area error | per tagged patch set | ≤ 2% |
| Gap thickness error | band regions: reconstructed `t` vs field `t` | ≤ 20% of `t` |
| Mid-surface error | sheet nodes to mid-surface / local `h` | ≤ 2% |
| Topology counts | components, curves, partitions vs the arranged complex | exact |

> **As built (rev 1.3, D-12).** `[V13]` implements the anchoring row and not the containment row.
> `good_cube.vtu` verified against a `[0, 1]³` cube STL reports `on_surface_area_frac = 1.0`,
> `deviation_max = offset_mean = 0`, `chord_max = 0.5`, `[V13]` PASS — its two interior interface
> triangles run through the cube with every corner on the cube's surface (§8, 2026-09-23). Changing
> the severity to FAIL or the tolerance to zero does not close this; only containment does (plan
> M-1.5). The feature-curve conformance and sharp-corner rows below are **not implemented** in
> `verify.rs` (plan S-12); the remaining rows are as §4 states.

---

## 6. Fixture suite

Committed at `data/fixtures/meshgen/` — **fourteen** files at rev 1.3. The reference fixture is
one unit cube under the **Freudenthal 6-tet decomposition of G0-1 §2.2**, split into region
`{0}` (tets K0–K2) and region `{1}` (K3–K5), with the two shared faces emitted as
tagged interface cells and one 3-node polyline declared as a component-1 feature curve along
nodes 0-2-6 (moved there on 2026-08-13: the original 0-1-3 ran where the fixture's own region
layout puts no component-1 material, and `[V9]` found it on its first run) — so the
fixtures cross-validate the geometry freeze as well as this one.

Most corrupted fixtures are the reference mutated in exactly one way;
`bad_stacked_band` is purpose-built, because a stacked band only exists in a real
band and cannot be produced by perturbing the cube.

| File | Defect | Named check | Full fired set (the test manifest) |
|---|---|---|---|
| `good_cube.vtu` | none — the reference | — | *(empty)* |
| `bad_inverted_tet.vtu` | tet K0's nodes 1 and 2 swapped | `V1.negative_volume` | `V1.negative_volume` |
| `bad_duplicate_node.vtu` | v7 duplicated; K4 rewired to the copy | `V2.duplicate_node` | + `V3.boundary_leak`, `V3.hanging_node`, `V3.interface_crack`, `V3.non_manifold_edge`, `V8.partition_mismatch` |
| `bad_hanging_node.vtu` | K0 split at the midpoint of edge v0–v7; neighbours keep the unsplit edge | `V3.hanging_node` | + `V3.boundary_leak`, `V3.non_manifold_edge`, `V8.partition_mismatch` |
| `bad_triple_face.vtu` | a third tet glued to interior face (v0,v1,v7) | `V3.multi_shared_face` | + `V3.boundary_leak`, `V3.non_manifold_edge` |
| `bad_boundary_leak.vtu` | tet K5 deleted; the exposed faces are interior to the box and untagged | `V3.boundary_leak` | `V3.boundary_leak` |
| `bad_unwelded_sheet.vtu` | one node of a sheet-tagged face duplicated, so the face is not welded to either side | `V7.unwelded_sheet_face` | + `V2.duplicate_node`, `V3.hanging_node`, `V8.pinhole_sheet` |
| `bad_stacked_band.vtu` | a band meshed with **two** element layers across one gap | `V7.band_layers` | + `V3.interface_crack` (the hand-built band does not extend past its own sample, so its outermost tagged faces have one adjacent tet), `V4.min_dihedral`, `V4.low_dihedral_share` |
| `bad_partition_id.vtu` | a disconnected second component labelled `partition_id = 0` | `V8.partition_mismatch` | + `V3.boundary_leak` |
| `bad_region_key.vtu` | one tet's `region_key = 7` with a 2-entry table | `V6.illegal_region_key` | `V6.illegal_region_key` |
| `bad_pinhole_sheet.vtu` | one triangle removed from a sheet, leaving an interior hole | `V8.pinhole_sheet` | `V8.pinhole_sheet` |
| `bad_curve_node_id.vtu` *(2026-08-13)* | a curve node whose `N_ID` omits one of the components meeting along its curve | `V9.curve_node_id` | `V9.curve_node_id` |
| `bad_radial_patches.vtu` *(2026-08-13)* | a curve whose `CurveRadialPatches` disagrees with the patches around it | `V9.radial_patches` | `V9.radial_patches` |
| `bad_open_junction_fan.vtu` *(2026-08-13)* | the cube's main diagonal 0–7 declared a curve and one of its six tets removed | `V9.junction_fan` | + `V3.boundary_leak` (removing a tet leaves its two faces single-sided — the same justified cascade as `bad_partition_id`) |
| *pending (plan M-1.0, M-1.5, M-4.8)* | the review's counterexamples of §4.2–§4.4 and §5 — the item-cap document, the doubled-domain cube, the three schema injections, the per-face orientation table, the interior-interface cube against a cube STL | `V12.contract`, `V3.boundary_leak`, `V13.containment` | to be asserted when they are committed; each is a fixture whose *absence* is what let the defect ship (R8) |

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
ladder (geometry §8.4) exists.

All fourteen are structurally valid VTU: they **must load** and pass
`VtuDoc::validate()`. They test the *verifier*, not the reader — which is what
lets the verifier be trusted before the mesher exists (the tooling-first decision, plan
Appendix A §2.0/§2.2) — and a verifier is trusted only after its negative fixture fails it (plan
R8): the three defects of §4.2–§4.4 shipped precisely because no fixture exercised them.

Fixtures are **byte-canonical**: re-emitting a loaded fixture with
`save_vtu(.., Ascii)` reproduces the file byte-for-byte, so a diff against a
regenerated fixture is a meaningful review artefact.

---

## 7. Test obligations

| ID | Test | Asserts | Lands with |
|---|---|---|---|
| T-C1 | fixture load | all 11 fixtures load, `validate()` ok, re-emit byte-identical | GA-1 (done, §11) |
| T-C2 | corrupted-fixture suite | each `bad_*` fixture fires exactly its manifest set and its named check; `good_cube` fires none (`corrupted_fixture_suite_fires_exactly_its_manifest`, Rule C2) | GA-2 (met) |
| T-C3 | schema completeness | a producer that omits any **A** array fails a contract assertion | GA-2 |
| T-C4 | sentinel discipline | non-applicable entries carry `−1`/`255`/`0xFFFFFFFF`, never `0` | GA-2 |
| T-C5 | table encoding | set tables decode with end-offset semantics; a start-offset file is rejected | GA-2 |
| T-C6 | metadata | `StageIndex` matches the filename; `SchemaVersion` mismatch is a named error | GA-4 |
| T-C7 | geometry-only mode | an external tet-only VTU verifies with [V1]–[V5] and reports the rest SKIPPED with the missing array named | GA-2 |
| T-C8 | JSON schema | report validates against §4.1; item ordering deterministic across runs | GA-2 |
| T-C9 | renderer contract | every fixture renders; extraction counts match the analytic value | GA-3a |
| T-C10 | accuracy gates | each §5 row is enforced by a check and reports both relative and absolute values. *Rev 1.3:* the feature-curve and sharp-corner rows have no check (S-12) and the containment row has no check (D-12) | G9-3 (open) |
| T-C11 | contract validation (rev 1.3) | each §4.2 rule has a fixture that fails it; a self-declared final document with a missing **A** array is a FAIL, not a SKIPPED | plan M-1.0 |
| T-C12 | counts vs cap (rev 1.3) | cap 0 / 1 / 50 on one defect give one exit status and one `summary`; an INFO-then-FAIL section is a FAIL | plan M-1.0 |
| T-C13 | domain at the final stage (rev 1.3) | a stage-11 document extending past `DomainMax` fails `[V3]`; a pre-trim overhang at stage 5–8 is reported as deferred | plan M-1.0 |

**Status at rev 1.3** (test names from `tests/`, 2026-09-23): T-C1 met (`vtu_roundtrip_ascii_is_exact`,
`vtu_roundtrip_appended_raw_is_exact`, `emit_series_verifier_and_renderer_accept_every_snapshot`);
T-C2 met (`corrupted_fixture_suite_fires_exactly_its_manifest`,
`each_fixture_triggers_the_check_it_is_named_for`, `corrupted_fixtures_all_fail_the_gate`); **T-C3
not met** — no test makes a producer that omits an **A** array fail, which is exactly D-11's
defect (plan M-1.0); T-C4 and T-C5 have no test by that name (sentinels and end-offsets are
exercised by every fixture and by `render_scene::set_members`; a `0`-for-sentinel document or a
start-offset table is not rejected by any test); T-C6 met (`stage_index_and_name_are_frozen`,
`schema_version_mismatch_is_a_named_error`, `stage_index_filename_mismatch_is_a_fail`,
`snapshot_path_naming_matches_contract`); T-C7 met
(`external_plain_tet_vtu_verifies_in_geometry_only_mode`); T-C8 met
(`json_report_matches_the_frozen_schema`, `report_is_deterministic_across_runs`); T-C9 met
(`extraction_counts_default_spec`, `all_named_views_resolve`, the GA-5 baselines); T-C10 open
(`gates_are_configurable_and_scale_invariant` covers configurability; the feature-curve,
sharp-corner and containment rows have no check). The rev 1.3 `[V6]` clauses' as-built halves are
pinned by `v6_region_adjacency_rejects_a_two_component_step_across_an_untagged_face` and
`undeclared_boundary_metric_is_zero_when_no_material_boundary_exists`.

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
convention (Appendix B.8). It is a self-contained `uv` script with no dependencies:

```bash
uv run data/fixtures/meshgen/generate_fixtures.py data/fixtures/meshgen
```

Deterministic and re-runnable; because the fixtures are byte-canonical,
regeneration is a no-op diff, so a non-empty diff after regeneration means the
schema and the fixtures have drifted apart.

**Executed 2026-09-23 against the code at `891badc`** (package `0.2.1`, release, `aarch64`;
`rustmspt version --json` reports `git_dirty = true` on files outside `src/meshgen`, which is
byte-identical between `0a8eb1c` and `891badc`):

- The review's reproduction script (plan Appendix C) re-run with `BASE` pointed at a fresh
  directory: every verifier counterexample it records **reproduces** — `hidden_failure` (D-9),
  `final_outside_domain` (D-10), `missing_required` / `invalid_side_elements` /
  `sheet_claims_volume` (D-11), `corner_only` (D-12), `contact_orientation` (D-13). The numbers
  are in §4.2–§4.4 and §5.
- The fixture suite is **fourteen** files (§6), all loading through the reader and re-emitting
  byte-identically; the manifest in `tests/mesh_verify_tests.rs` asserts each file's full fired set.
- Every §2 array was traced to its producer and type (plan Appendix A §R and the audit findings
  the plan's §13 names); the departures are D-13, D-14, D-16 and D-18..D-21.
- The array census (§1, §2.3, D-18..D-21) was measured on A-6a (`run_acceptance.py`'s config
  template with `h_max_frac 0.04`, `h_min_frac 0.004`, `snapshots: all`, `RUSTMSPT_CUT_DIAG=1`)
  and A-3 (`snapshots: key`), the arrays read with an ascii regex parser — which is D-19's
  finding — and `mesh-verify` run on both of A-3's `s08` files.

---

## 9. Deviations and open items

| # | Deviation | Reason |
|---|---|---|
| D-1 | The record's §7.2 sketch's `u16` arrays (`RegionSetPriority`, `ComponentY`) are frozen as **`UInt32`** | `ArrayData` has no `U16` variant, so every fixture failed to load with *"unsupported DataArray type UInt16"*. `UInt32` needs no implementation change and the value range is unaffected. Caught by the fixture round-trip |
| D-2 | Metadata is **numeric-only**; the record's §7.2 sketch's string-valued `Stage`, `GeneratorVersion`, `ConfigHash`, `DeterminismMode` become `StageIndex` + a frozen enumeration, a 3-tuple semver, a `UInt64`, and a `UInt8` | No string array type exists, and adding one buys nothing: the human-readable stage name is already in the filename, which the verifier cross-checks ([V12]) |
| D-3 | `NumberOfTuples` is **required on FieldData arrays** and omitted elsewhere | VTK requires it there and `save_vtu` always emits it; fixtures without it round-tripped to a different byte stream. Making it part of the contract is what makes fixtures byte-canonical |
| D-4 | Set tables use **VTK end-offset** semantics, stated explicitly | The record's §7.2 sketch never said which convention; the implemented `render_scene::set_members` uses end-offsets, matching cell `offsets`. Freezing the other convention would have silently shifted every set by one |
| D-5 | The background region key carries `RegionSetPriority = 0xFFFFFFFF` | `{0}` has no priority; a real value (e.g. `0`) would make it win every resolution comparison in a consumer that does not special-case it |
| D-6 | `verify_flags` is `UInt32` with bit *k* = check `[V(k+1)]` | The plan named the array but not the bit assignment; the renderer filters on it, so it needs a fixed mapping |
| D-7 | JSON report items carry an explicit `code` (`V3.hanging_node`) and a deterministic ordering | Tests assert on verifier JSON (plan Appendix B.8); without a stable code and order those assertions would be brittle |
| D-8 | **`[V13]` is a FAIL gate at `on_surface_area_frac = 1.0`** (§4), as §5 has always stated P3; the code emits WARN | Plan §6.2 item 3: the contract gated P3 as absolute while the verifier exited zero on a mesh that failed it. The severity is normative from rev 1.3; the code follows at plan M-1.0 |
| D-9 | **Severity counts and the exit status are computed over every finding, not over the stored items** (§4.3); as built the cap changes the exit status | Plan MG-01; the mechanism (`push` before the cap, `summary` from stored items) is recorded so the fix is not a `cap ≥ 1` band-aid |
| D-10 | **The octree-hull relaxation of `[V3]`'s box test ends at `StageIndex` 8** (§4.4); as built it applies at 11 | Plan MG-03; `check_delivered` is not an independent box test until this lands |
| D-11 | **Contract validation by metadata-selected strength** (§4.2); as built only structural validation exists and absent arrays degrade every semantic check to SKIPPED | Plan MG-08; a self-declared final document is not an external geometry-only file |
| D-12 | **P3 is containment; the corner test is its necessary half** (§5); as built only the corner test exists | Plan MG-02; `[V13]`'s own landing note said corners were read to separate chord sag from the staircase, which is right, and never claimed sufficiency — the gate did |
| D-13 | **`FaceTagOrientation` is one `±1` per flattened member** (§2.3, unchanged); as built `cut_to_doc` writes one `+1` per face cell, so the array is shorter than `FaceTagComponents` wherever a face carries several tags (two cubes in contact at `x = 0.4`: 280 members, 270 entries), and `FaceTagSideElems` takes the first tag's `(inside, outside)` | Plan MG-07; the writer accepts the inconsistent lengths, which §4.2's validator must refuse. Plan M-4.8 |
| D-14 | **`partition_id` is written as `0` on every cell** by `cut_to_doc`, including non-tets where §2.1 requires `−1`; the flood fill of `[V8]` has never been produced by the mesher | Plan MG-13; plan M-6.1 builds it (v3 said "moves" it). `[V8]` on every acceptance case has passed a stored array of zeros against a recomputed fill of one partition, which is vacuous agreement on a single-partition domain |
| D-15 | **S10 has no snapshot index of its own**; the stage enumeration (`9` thin, `10` quality, `11` final) is unchanged and plan M-6.1's `s10_regions` is withdrawn | The review's erratum: one index cannot carry two names |
| D-16 | **The four diagnostic cell/point arrays `plc_path`, `parent_cell`, `escalation_reason`, `node_origin` are non-contract** (§2.5) | Plan S-9, corrected at rev 1.3: `plc_path`, `parent_cell` and `escalation_reason` are emitted only under `RUSTMSPT_CUT_DIAG`; `node_origin` is written on every `s08` (S-9 said all four were gated). They are read by the plan's censuses, and no check depends on them |
| D-17 | **Report determinism is a contract clause the verifier breaks on a large mesh** (§4.1 as-built note): `[V6]` items in hash order, one float metric summed in hash order | Measured on A-3, two identical runs; invisible to the fixture suite because no fixture exceeds the cap. Plan M-1.0 (e) |
| D-18 | **The delivered file carries the contract's field tables verbatim and gives `[V6]`/`[V7]`/`[V9]` different answers from the contract file** (§1): `[V7]` false-FAILs and `[V9]` sees no curve on it | Measured on A-3, 2026-09-23. §1's "run on the delivered file unchanged" was written for `[V1]`–`[V4]`/`[V12]` and over-claimed the rest; `mesh-verify` should skip `[V7]`/`[V9]` with a named reason when `Counts` reports no tagged face (plan M-1.0), and `volume_only` may drop the face/curve tables (plan M-6.2) |
| D-19 | **The `mesh` pipeline writes every snapshot in ascii** (`pipeline/meshgen.rs` passes `VtuEncoding::Ascii`), so the frozen appended-raw default never applies to its output and the `.ascii.vtu` suffix rule is inert there | The audit's own array census had to parse ascii, which is how it was found. Plan M-6.2 decides the delivered encoding; the writer and reader are unchanged |
| D-20 | **Small departures from §2's rows:** `provenance` is padded with `0` on non-tet cells where the u8 sentinel is `255`; `ThinRegionSkip` has a sixth code (`6` undersampled) the table lacked; `ThinRegionSeparation` is in the normalized frame while `separation_t` is in output units; `verify_flags` sets a bit for WARN items as well as FAIL; `constraint_ref` is `−1` for corner and box-face constraints and for the alternating polyline pass | Each row is corrected to the as-built value where the value is defensible (the sixth code, WARN bits, the `−1` refs) and recorded as a defect where it is not (the `0` padding, the frame mismatch — plan M-6.1) |
| D-21 | **`CurveRadialPatches` is `0` for every curve the pipeline writes** — the box clip drops S2's radial order and the column is emitted on `s08` only — so `[V9]`'s radial-patch clause is exercised by the `bad_radial_patches` fixture and by no pipeline output (§2.3) | A check whose producer never fills its input: the plan's own recorded anti-pattern. Carry `radial_patches` through `clip_arranged_to_box`'s polyline split and emit the column on `s02`/`s03` (plan M-4.0); until then the clause is stated as not exercised |

Open items:

- **[V6]–[V10] semantics land with their producing stages** (plan Appendix B.10 for the
  GPU phases; M-6 for S10/S11); this document freezes their contract, not their implementation.
- **`[V9]`'s "a curve the mesh carries no edge for is INFO" clause** (§4) rests on gate G6-0's
  adoption of the conforming fan, which plan R1 voids; it becomes a FAIL clause at rev 1.4 once
  plan M-2 has made the kernel carry every curve (plan S-8). Left as INFO at rev 1.3 because R6
  forbids gating on an unmeasured state.
- **`[V10]` and `[V11]`** are emitted `Skipped` unconditionally: the INP writer and `--compare` do
  not exist (plan M-6.2, M-6.3). Their rows in §4 are the contract they will meet.
- **The split-export node map** (plan M-6.6, MG-14) is a rev 1.4 addition to §2.3: the INP's split
  node numbering as an explicit map from the welded VTU ids, which `[V10]` verifies through.
- **Compressed VTU support** stays rejected. If ParaView-side file sizes become a
  problem, `vtkZLibDataCompressor` is the named future addition and a
  `SchemaVersion` bump is *not* required (it is an encoding, not a schema change).
- **`--split` companion verification** is out of scope for iteration 1: the
  primary file is the contract, companions are a convenience export.
