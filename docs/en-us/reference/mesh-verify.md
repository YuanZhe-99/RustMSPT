# Mesh Verification — the Check Catalog and the `mesh-verify` Pipeline

Reference for the verification slice of the mesh-generation tooling phase
(`PLAN_mesh_generation.md` phase GA, subtask GA-2): the exact-predicate and
element-quality primitives (`src/meshgen/predicates.rs`), the check catalog
(`src/meshgen/verify.rs`), and the `mesh-verify` subcommand
(`src/pipeline/mesh_verify.rs`, `src/config/mesh_verify.rs`).

The authoritative contract is [`SPEC_meshgen_contracts.md`](../../../SPEC_meshgen_contracts.md)
§4 (catalog, severities, gates) and §4.1 (JSON schema). Check codes and the JSON
shape are frozen: tests assert on them.

## Index

| Item | Source | Summary |
|---|---|---|
| `orient3d` | `src/meshgen/predicates.rs:698` | Exact-sign tetrahedron orientation `det[b-a, c-a, d-a]`; the **only** place `robust::orient3d`'s opposite sign convention is negated (Rule N10). |
| `tet_signed_volume` | `src/meshgen/predicates.rs:708` | `orient3d/6`; positive for a positively oriented tet. |
| `orient3d_sign_test` | `src/meshgen/predicates.rs:713` | Reference case pinning the orientation convention; a unit tet must be positive. |
| `TetQuality` | `src/meshgen/predicates.rs:729` | Volume, aspect ratio, radius ratio, min/max dihedral (degrees **and** cosine), scaled Jacobian, min altitude. |
| `tet_quality` | `src/meshgen/predicates.rs:748` | Compute [V4] metrics for one tet; `max_dihedral_cos` lets angle gates stay algebraic (no `acos` in a decision path). |
| `node_key` | `src/meshgen/predicates.rs:867` | Quantized integer node key on a scale-relative grid; equal keys mean the same node. |
| `Severity` | `src/meshgen/verify.rs:21` | `Info < Warn < Fail`, with the contract spellings used in the log and JSON. |
| `CheckStatus` | `src/meshgen/verify.rs:40` | Per-section outcome: `PASS`/`WARN`/`FAIL`/`SKIPPED`; a skip always carries a reason. |
| `VerifyItem` | `src/meshgen/verify.rs:63` | One finding: severity, stable `code`, message, point/cell ids and coordinates (consumed by `mesh-render --highlight-from`). |
| `VerifySection` | `src/meshgen/verify.rs:88` | One catalog entry with status, named metrics, and a capped item list (`items_truncated`). |
| `VerifyGates` | `src/meshgen/verify.rs:145` | Configurable thresholds; all length tolerances are fractions of the bbox diagonal, so gates are scale-invariant. |
| `VerifyReport` | `src/meshgen/verify.rs:179` | Full result plus metadata echo; `passed`, `exit_code`, `fired_codes`, `section` accessors. |
| `VerifyOptions` | `src/meshgen/verify.rs:410` | Out-of-document verifier inputs; carries `expected_stage` (parsed from a snapshot filename) for the [V12] cross-check. |
| `verify` | `src/meshgen/verify.rs:430` | Run the catalog over a contract or external VTU; returns one section per entry in contract order. |
| `verify_with_options` | `src/meshgen/verify.rs:744` | `verify` with out-of-document options; surface stages s00-s03 skip volume-only [V7]/[V8]/[V13]. |
| `BoundaryFace` | `src/meshgen/verify.rs:3531` | One material-boundary face as [V13] measures it: area, local edge length, mean/max \|distance\| and **signed** offset to the component's surface. |
| `FidelityAcc` | `src/meshgen/verify.rs:3548` | [V13]'s per-component accumulator; every sum is area-weighted so a coarse face cannot outvote a fine one by being counted once. |
| `absorb` | `src/meshgen/verify.rs:3561` | Fold one `BoundaryFace` into a `FidelityAcc`. |
| `check_v13` | `src/meshgen/verify.rs:3603` | [V13] interface fidelity: the material boundary read off the volume (region set vs region set, tags never consulted) and measured against the input surface. |
| `report_to_json` | `src/meshgen/verify.rs:1510` | Serialize the frozen JSON report (hand-rolled; the project carries no JSON dependency). |
| `report_to_log` | `src/meshgen/verify.rs:1619` | Sectioned human log with a `[PASS]/[WARN]/[FAIL]/[SKIP]` line per check and a summary. |
| `annotate` | `src/meshgen/verify.rs:1681` | Copy of the document carrying the quality arrays plus the `verify_flags` bitmask (bit *k* = `[V(k+1)]`). |
| `VerifyGateParams`/`MeshVerifyParams`/`MeshVerifyConfig` | `src/config/mesh_verify.rs:8` | The `mesh_verify:` YAML block: input, report/json/annotate destinations, gate overrides. |
| `gates_from_config` | `src/pipeline/mesh_verify.rs:21` | Overlay YAML overrides onto the contract defaults. |
| `verify_file` | `src/pipeline/mesh_verify.rs:53` | Load → validate → verify → write log/JSON/annotated VTU; returns the report. |
| `MeshVerifyPipeline` | `src/pipeline/mesh_verify.rs:12` | The `mesh-verify` subcommand; returns an error (nonzero exit) when a gate fails. |

## What runs today

The catalog always reports **all thirteen** sections. Checks that need data the
document does not carry report `SKIPPED` with a reason naming the missing array
or the producing stage — a report never silently omits a check.

| Check | State | Notes |
|---|---|---|
| [V1] Cells | **full** | non-positive volume (exact predicate), repeated node in a cell, duplicate cells, non-finite coordinates |
| [V2] Nodes | **full** | coincident duplicates by quantized key (FAIL), unreferenced nodes (WARN) |
| [V3] Conformity | **full** | face shared by ≠2 tets, boundary leak (untagged face off every domain plane), hanging nodes (spatial-hash assisted), non-manifold edges (WARN). **The boundary-leak rule is deferred on a pre-cut snapshot** (`StageIndex` 5–7): the background lattice's boundary is the octree hull, which overhangs the domain box by up to one coarse cell per axis until S8 trims it. The count is still reported as a metric and explained by an INFO `V3.deferred` item; face sharing, hanging nodes and manifoldness — the real content of Theorem T1 — stay FAIL at every stage. |
| [V4] Quality | **full** | AR, radius ratio, dihedral extremes, scaled Jacobian, min altitude; distributions, worst-10, three gates |
| [V5] Geometric conformance | **full when `surfaces:` is set** | two-sided surface fit (mesh interface to input, and input back to interface) plus per-component volume. SKIPPED with a reason when no input surfaces are given, since there is nothing to conform *to*. Two denominators here were wrong once each and are worth knowing: the fit tolerance is a fraction of the **local interface edge length**, not the bbox diagonal; and `volume_expected` is the **priority-resolved** volume — the part of a component no higher-priority body covers — not its raw input volume. |
| [V6] ID semantics | **partial** | region-key legality and one-priority-per-key run now; per-component volume error and the sampled audit need the input surfaces |
| [V7] Sheets & thin | **partial** | sheet-face weldedness runs on volume stages; s00-s03 report SKIPPED because they contain no tets; one-layer band, mid-surface and rim conformance land with G7-1 |
| [V8] Partitions | **full on volume stages** | sheet-blocked flood fill recomputed and compared with `partition_id` (up to renumbering), pinhole-leak heuristic, `expected_partitions` gate; s00-s03 report SKIPPED |
| [V9] Junctions | skipped | lands with G6-4 |
| [V10] Export completeness | skipped | lands with G9-2 |
| [V11] Compare mode | skipped | lands with GK-3 |
| [V12] Provenance & stats | **full** | counts, metadata echo, `Counts`-vs-mesh agreement, `SchemaVersion` check, `StageIndex` range + filename cross-check (T-C6), and the **element-count attribution** the plan's P2 is steered on: per-`provenance` tet counts, and — when `parent_cell` is present under `RUSTMSPT_CUT_DIAG` — the S5 cells behind them and the emission rate per cell. The S5 cell is a lattice **tet**, so an untouched one emits exactly 1 and `tets_per_cell_*` reads directly as what that path costs over leaving the cell alone. |
| [V13] Interface fidelity | **full when `surfaces:` is set** | the plan's P3, measured. See below — it is not a variant of [V5]. |

**[V13] is what [V5] cannot be.** [V5] measures the *declared* interface: the tagged
`VTK_TRIANGLE` cells, whose nodes S7 snapped onto the input surface. Those nodes are on
the surface essentially exactly, so [V5] reports a near-perfect fit for a mesh whose real
material boundary — the faces between tets that disagree about which body they are inside
— is a staircase half a cell away and carries no tag at all. [V13] derives the boundary
from the volume and never consults a tag, which is why it is the check the goal's "exact
surfaces" property is read from.

**It is read at the face corners, and that matters.** A flat facet whose three vertices
are cut nodes on the surface is a *chord* of it — the best a mesh of flat facets can do,
with a sag that falls as `h²` and is what refinement buys. A facet whose vertices are
lattice nodes or a cell centroid is somewhere else entirely, and that is the staircase.
Measuring the whole facet at once conflates them: on the sphere fixture it charged the
mesher for 87% of its boundary area when most of that was irreducible faceting. The sag is
still reported, as `chord_mean` / `chord_max`, as its own number and not as a violation.

It reports two numbers because a boundary fails in two ways one distance cannot separate:

- **rough but centred** — it zigzags across the surface. Mean \|distance\| is large; the
  area-weighted **signed** offset is ~0.
- **smooth but displaced** — it is a clean sheet in the wrong place. Both are large.

`displacement_share` = \|offset\| / deviation is that discrimination as one number: ~0
rough, ~1 displaced. This exists because a change was once scored as a 26% improvement on
a proxy metric while it moved a fixture from the first failure to the second and made a
plate 2.6× thinner than the input — and nothing in the suite noticed. Both are P3
violations; the pair is for diagnosis, never for grading one as acceptable.

Design points worth knowing:

- **[V5]'s surface priorities must mirror the mesher's.** `volume_expected` is the
  *priority-resolved* volume — the part of a component no higher-priority body covers
  — so a surface listed at the wrong rank makes [V5] mask a body the mesher never
  overrode, and the loss is reported as zero rather than measured. Each `surfaces:`
  entry is a bare path (priority 0) or `{stl, priority}`. Until 2026-08-07 the
  priority was derived from the *list index*, which silently absorbed a contained
  body: an acceptance case with a cube inside a sphere reported the cube as owning no
  elements at all, and the suite stayed green.
- **[V5] switches denominators on self-intersecting input.** The divergence-theorem
  volume of a surface is exact only if the surface does not intersect itself. A body
  exported as a *union of overlapping parts* — a strut lattice, a bolted assembly —
  has every overlap counted once per part, so the exact sum is the sum of the parts
  while the mesh correctly contains the union. [V5] cross-checks the sum against a
  sampled estimate (generalized winding number, sampled per connected shell); when
  they disagree by more than 5 % it raises `V5.self_intersecting_input` and scores
  `volume_expected`/`volume_error` from the *estimate* instead. On one 15-box lattice
  that is the difference between a reported 18 % loss and a real 9 %. The exact sum
  stays the default everywhere else: it is right to fifteen digits on clean input,
  and the estimate carries ~0.5 % sampling noise.
- **`[V6]` checks region adjacency, and it is the check whose absence let material be
  lost silently.** Two tets sharing a face differ by crossing exactly one component's
  surface, so their inside-sets differ in exactly one member - unless two surfaces are
  *coincident* on that face, in which case the step may change by as many components as
  the face is tagged with. Background is the sentinel key `{0}`, read as the empty set.
  A face between `{1,2}` and `{0}` is impossible: the lens of two intersecting solids is
  interior to both. A-3 carried **521** of them (measured 2026-08-07) and the whole suite
  stayed green, because every check was measuring something else. The rule is skipped for
  a pair whose components carry different priorities, where a higher-priority label
  replaces the one beneath it and a two-member step is legitimate.
- **The partition comparison is up to renumbering.** Stored `partition_id` values
  need not match the recomputed component indices numerically; what must hold is a
  bijection between them. A fixture that labels two disconnected components with
  the same id therefore fails, while a valid mesh numbered differently does not.
- **Angle gates never call `acos`.** `tet_quality` returns the cosine of the
  smallest dihedral, and the gate compares cosines. Degrees are reported, never
  decided on — transcendentals are not correctly rounded and would make the gate
  platform-dependent (`SPEC_meshgen_numerics.md` §8.1 rule 4).
- **Surface snapshots are not final sheet meshes.** Stages s00-s03 intentionally
  contain tagged faces/curves and no volume cells. [V7]/[V8] therefore report
  `SKIPPED` for those stage indices instead of falsely flagging every face as
  unwelded or every open surface as a partition leak.

## Usage

```bash
# default config at data/input/mesh_verify_config.yaml
cargo run --release -- mesh-verify --input data/output/mesh.vtu

# with explicit outputs; exit status is nonzero when a gate fails
cargo run --release -- mesh-verify \
  --input data/fixtures/meshgen/good_cube.vtu \
  --report data/output/mesh_verification.log \
  --json   data/output/mesh_verification.json \
  --annotate data/output/mesh_annotated.vtu
```

The annotated VTU carries `aspect_ratio`, `radius_ratio`, `min_dihedral_deg`,
`scaled_jacobian` and `verify_flags`, which `mesh-render` can colour by or filter
on (`{ kind: array_range, array: aspect_ratio, min: 10.0, max: 1.0e30 }`).

## Fixture suite

`data/fixtures/meshgen/` holds the eleven hand-written fixtures frozen in
`SPEC_meshgen_contracts.md` §6 — one reference mesh and ten carrying a single
injected defect each. `tests/mesh_verify_tests.rs` asserts, for every fixture,
both the **exact** set of codes it fires and the presence of the check it is named
for, so a new false positive fails the suite as loudly as a missed defect.

Regenerate them (a no-op diff when the schema and fixtures agree):

```bash
uv run data/fixtures/meshgen/generate_fixtures.py data/fixtures/meshgen
```

## Related documents

- [`SPEC_meshgen_contracts.md`](../../../SPEC_meshgen_contracts.md) — the frozen catalog, JSON schema, accuracy table, fixture manifest.
- [`SPEC_meshgen_numerics.md`](../../../SPEC_meshgen_numerics.md) — predicate inventory and the arithmetic rules `predicates.rs` implements.
- [`SPEC_meshgen_geometry.md`](../../../SPEC_meshgen_geometry.md) — the orientation convention and the lattice templates the reference fixture is built from.
- [mesh-render-and-vtu.md](mesh-render-and-vtu.md) — the VTU reader/writer and the renderer that consumes annotated meshes.
